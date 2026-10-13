//! The render pipeline: a declared gateway tree to the bytes the gateway
//! holds on disk.
//!
//! Stages, in order (later plans insert at the named stage; each stage is a
//! private function below):
//!
//! 1. **collect** the source tree ([`super::tree::SourceTree`]) - done by the
//!    caller and handed in as [`RenderInput::source`]
//! 2. **overlay** `modes/<env>/` onto the base: a file replaces the base file
//!    at the same path whole, or is added (SF-7: there is no removal). Only
//!    the selected environment is applied.
//! 3. **map** repo paths to collection paths: `resources/...` drops its
//!    prefix, `tags/<P>/<type>/<rest>` becomes `ignition/<type>/<P>/<rest>`
//!    (CD5); two sources on one collection path are refused
//! 4. **policy**: the manifest type allowlist and its D-02 refusal, the deny
//!    list, gateway-owned files, dot-names, undeclared tag providers and tag
//!    types under `resources/` (21-06)
//! 5. **JSON classification**, the Gson gate (21-05): `node::classify` on every
//!    `.json` member; findings name the source path; parsed nodes are kept
//! 6. **uuid** (21-07): the rule-table checks of stage 4 (`enforce_rules`),
//!    then per Named resource keep, inject or synthesize `attributes.uuid`
//! 7. lints (21-08)
//! 8. **finish**: build the [`RenderedTree`], resources and the digest
//!
//! Pure: no filesystem, no network, no clock, no randomness. Ordered
//! collections only, so equal inputs give equal outputs on every OS.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::layout::{RelPath, ResourceKey, TAG_TYPES, collection_path, resource_key, tag_provider};
use super::manifest::Manifest;
use super::node::{self, JsonClass, Node};
use super::rules::{GATEWAY_OWNED_FILES, TypeRule, UuidRule, is_denied, type_rule};
use super::target::Target;
use super::tree::{RenderedTree, SourceTree};
use super::uuid::{is_canonical_uuid, name_based_uuid};
use crate::error::{CoreError, DeployFinding};

/// Everything render needs.
#[derive(Debug, Clone, Copy)]
pub struct RenderInput<'a> {
    /// Gateway directory name under `gateways/`.
    pub gateway: &'a str,
    /// The environment name (`--env`).
    pub env: &'a str,
    /// The parsed `manifest.yaml`.
    pub manifest: &'a Manifest,
    /// The parsed `targets/<env>.yaml`.
    pub target: &'a Target,
    /// The collected gateway tree.
    pub source: &'a SourceTree,
}

/// One rendered resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderedResource {
    /// Module id.
    pub module: String,
    /// Resource type.
    #[serde(rename = "type")]
    pub ty: String,
    /// Resource name; null for a singleton.
    pub name: Option<String>,
    /// Files relative to the resource directory, byte-sorted.
    pub files: Vec<String>,
}

/// What render produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOutcome {
    /// The rendered files.
    pub tree: RenderedTree,
    /// Resources ordered by (module, type, name) bytes, singletons first.
    pub resources: Vec<RenderedResource>,
    /// Tag providers that rendered at least one file, byte-ordered.
    pub tag_providers: Vec<String>,
    /// Ids (`module/type/name`) of resources that received a minted uuid,
    /// byte-ordered.
    pub uuids_minted: Vec<String>,
    /// Collection paths of `.json` files that are not in the gateway's writer
    /// format and so pass through untouched, byte-ordered.
    pub opaque_files: Vec<String>,
    /// Warnings in pipeline order.
    pub warnings: Vec<String>,
}

/// Render `input`. Every policy defect is collected and raised once as a
/// `deploy_tree_invalid`.
pub fn render(input: &RenderInput<'_>) -> Result<RenderOutcome, CoreError> {
    let mut findings = Vec::new();
    let effective = overlay(input, &mut findings);
    let effective = screen_names(effective, &mut findings);
    let mut mapped = map_to_collection(effective, &mut findings);
    enforce_policy(input, &mapped, &mut findings);
    enforce_rules(input, &mapped, &mut findings);
    let mut json = classify_json(&mapped.files, &mapped.origins, &mut findings);
    let minted = assign_uuids(input.gateway, &mut mapped, &mut json, &mut findings);
    if !findings.is_empty() {
        return Err(CoreError::deploy_findings(findings));
    }
    Ok(finish(mapped, &json, minted))
}

/// One gateway-relative file after the overlay: its bytes and the repo path
/// prefix (`gateways/<g>/` or `gateways/<g>/modes/<env>/`) it came from, so a
/// finding can name the file the operator should open.
struct Entry {
    bytes: Vec<u8>,
    prefix: String,
}

/// Gateway-relative path (`resources/...` or `tags/...`) to its entry.
type Effective = BTreeMap<RelPath, Entry>;

/// Where a collection path came from.
struct Origin {
    /// Full repo-relative source path.
    file: String,
    /// The prefix of `file` before the gateway-relative path.
    prefix: String,
}

/// Collection path to its origin, so every later finding names a file the
/// operator can open.
type Origins = BTreeMap<RelPath, Origin>;

/// The mapped tree (stage 3 output).
struct Mapped {
    files: BTreeMap<RelPath, Vec<u8>>,
    origins: Origins,
    /// Collection paths that came from `tags/`, with their provider.
    tags: BTreeMap<RelPath, String>,
}

/// Parsed JSON members keyed by collection path, with their class. Stage 6
/// (uuid, 21-07) updates it for files it writes; stage 7 (lints, 21-08) reads it.
type JsonTable = BTreeMap<RelPath, (JsonClass, Node)>;

/// Stage 2: apply `modes/<env>/` over the base.
///
/// An overlay file replaces the base file at the same path or is added; a
/// base file with no overlay counterpart is unchanged. Keys under
/// `modes/<other>/` are not this environment's and are ignored (the reader
/// never opens them either).
fn overlay(input: &RenderInput<'_>, findings: &mut Vec<DeployFinding>) -> Effective {
    let base_prefix = format!("gateways/{}/", input.gateway);
    let env_dir = format!("modes/{}", input.env);
    let overlay_prefix = format!("{base_prefix}{env_dir}/");
    let mut effective = Effective::new();
    let mut overlaid = Vec::new();
    for (path, bytes) in &input.source.files {
        if path.components().next() == Some("modes") {
            if let Some(rest) = path.strip_prefix(&env_dir) {
                overlaid.push((rest, bytes));
            }
        } else {
            effective.insert(
                path.clone(),
                Entry {
                    bytes: bytes.clone(),
                    prefix: base_prefix.clone(),
                },
            );
        }
    }
    for (rest, bytes) in overlaid {
        let top = rest.components().next();
        if !matches!(top, Some("resources" | "tags")) && rest.as_str() != ".gitkeep" {
            findings.push(DeployFinding {
                file: format!("{overlay_prefix}{rest}"),
                line: None,
                column: None,
                message: "unexpected entry under modes/<env>/ (expected: resources/, tags/)"
                    .to_string(),
            });
            continue;
        }
        effective.insert(
            rest,
            Entry {
                bytes: bytes.clone(),
                prefix: overlay_prefix.clone(),
            },
        );
    }
    effective
}

/// Stage 4 (21-06 share, name screening): a path component that starts with
/// `.` is refused, except a file named `.gitkeep`, which is dropped; a file
/// whose name is gateway-owned is refused. Offending entries are dropped
/// after being reported, so later stages do not repeat the finding.
fn screen_names(effective: Effective, findings: &mut Vec<DeployFinding>) -> Effective {
    let mut kept = Effective::new();
    let mut reported: BTreeSet<String> = BTreeSet::new();
    for (path, entry) in effective {
        let parts: Vec<&str> = path.components().collect();
        let last = parts.len() - 1;
        let dot = parts.iter().position(|c| c.starts_with('.'));
        match dot {
            Some(i) if i == last && parts[i] == ".gitkeep" => continue,
            Some(i) => {
                let at = format!("{}{}", entry.prefix, parts[..=i].join("/"));
                if reported.insert(at.clone()) {
                    findings.push(DeployFinding {
                        file: at,
                        line: None,
                        column: None,
                        message: "name starts with `.`; dot-named entries are refused (only a `.gitkeep` file is skipped)".to_string(),
                    });
                }
                continue;
            }
            None => {}
        }
        if GATEWAY_OWNED_FILES.contains(&parts[last]) {
            findings.push(DeployFinding {
                file: format!("{}{path}", entry.prefix),
                line: None,
                column: None,
                message: format!(
                    "`{}` is written by the gateway itself and is not deployed; remove it from the tree",
                    parts[last]
                ),
            });
            continue;
        }
        kept.insert(path, entry);
    }
    kept
}

/// Stage 3: gateway-relative paths to collection paths. The mapping must be
/// injective: a second source on a collection path is a finding naming both.
fn map_to_collection(effective: Effective, findings: &mut Vec<DeployFinding>) -> Mapped {
    let mut mapped = Mapped {
        files: BTreeMap::new(),
        origins: Origins::new(),
        tags: BTreeMap::new(),
    };
    for (path, entry) in effective {
        let file = format!("{}{path}", entry.prefix);
        match collection_path(&path) {
            Ok(collection) => {
                if let Some(first) = mapped.origins.get(&collection) {
                    findings.push(DeployFinding {
                        file,
                        line: None,
                        column: None,
                        message: format!(
                            "maps to collection path `{collection}`, which `{}` already produces",
                            first.file
                        ),
                    });
                    continue;
                }
                if let Some(provider) = tag_provider(&path) {
                    mapped.tags.insert(collection.clone(), provider.to_string());
                }
                mapped.origins.insert(
                    collection.clone(),
                    Origin {
                        file,
                        prefix: entry.prefix,
                    },
                );
                mapped.files.insert(collection, entry.bytes);
            }
            Err(why) => findings.push(DeployFinding {
                file,
                line: None,
                column: None,
                message: why,
            }),
        }
    }
    mapped
}

/// Stage 4: the manifest allowlist (D-02), the deny list, tag types under
/// `resources/`, and undeclared tag providers.
///
/// Findings are one per type directory or provider, naming the first source
/// in byte order (an overlay path when the overlay supplied it).
fn enforce_policy(input: &RenderInput<'_>, mapped: &Mapped, findings: &mut Vec<DeployFinding>) {
    // the first origin prefix of each resource-side type directory
    let mut types: BTreeMap<(String, String), &str> = BTreeMap::new();
    let mut providers: BTreeMap<&str, &str> = BTreeMap::new();
    for path in mapped.files.keys() {
        let origin = &mapped.origins[path];
        if let Some(provider) = mapped.tags.get(path) {
            providers.entry(provider).or_insert(&origin.prefix);
        } else {
            let key = resource_key(path);
            types.entry((key.module, key.ty)).or_insert(&origin.prefix);
        }
    }
    let dir = |prefix: &str, module: &str, ty: &str| format!("{prefix}resources/{module}/{ty}/");
    let finding = |file: String, message: String| DeployFinding {
        file,
        line: None,
        column: None,
        message,
    };

    for ((module, ty), prefix) in &types {
        if !input.manifest.allows_type(module, ty) {
            findings.push(finding(
                dir(prefix, module, ty),
                "type not in manifest.yaml types (D-02: un-allowlisted type directories are refused, not skipped)".to_string(),
            ));
        }
        if module == "ignition" && TAG_TYPES.contains(&ty.as_str()) {
            findings.push(finding(
                dir(prefix, module, ty),
                "tag types are declared under tags/<provider>/, not resources/".to_string(),
            ));
        }
        if is_denied(module, ty) {
            findings.push(finding(
                dir(prefix, module, ty),
                "denied type: rendering it can lock an administrator out of the gateway, so it stays gateway-owned even when allowlisted".to_string(),
            ));
        }
    }
    for entry in &input.manifest.types {
        if let Some((module, ty)) = entry.split_once('/')
            && is_denied(module, ty)
        {
            findings.push(finding(
                format!("gateways/{}/manifest.yaml", input.gateway),
                format!(
                    "types entry `{entry}` is a denied type and is refused even though it is allowlisted (rendering it can lock an administrator out of the gateway)"
                ),
            ));
        }
    }
    for (provider, prefix) in &providers {
        if !input.manifest.tag_providers.iter().any(|p| p == provider) {
            findings.push(finding(
                format!("{prefix}tags/{provider}/"),
                "tag provider not in manifest.yaml tag_providers (D-02: undeclared providers are refused)".to_string(),
            ));
        }
    }
}

/// Stage 5 (21-05): classify every `.json` member through the Gson gate.
///
/// A member that cannot be inspected (too large, not UTF-8, a BOM, a syntax
/// error, too deep, a repeated key) is a finding naming its source path;
/// the secret lint (21-08) needs the parsed tree, so JSON render cannot
/// parse is refused rather than copied blind. Bytes are never changed here.
/// Non-`.json` members are not parsed. Findings carry the parser's message,
/// which names positions and keys, never values.
fn classify_json(
    mapped: &BTreeMap<RelPath, Vec<u8>>,
    origins: &Origins,
    findings: &mut Vec<DeployFinding>,
) -> JsonTable {
    let mut table = JsonTable::new();
    for (path, bytes) in mapped {
        if !path.as_str().to_ascii_lowercase().ends_with(".json") {
            continue;
        }
        match node::classify(bytes) {
            Ok(classified) => {
                table.insert(path.clone(), classified);
            }
            Err(message) => findings.push(DeployFinding {
                file: origins
                    .get(path)
                    .map(|o| o.file.clone())
                    .unwrap_or_else(|| path.to_string()),
                line: None,
                column: None,
                message,
            }),
        }
    }
    table
}

/// Stage 4 (21-07 share): the per-type rule table, fail-closed.
///
/// - a manifest `types` entry, or a rendered resource type, with no
///   [`type_rule`] row is a finding (only evidence-backed types render);
/// - a Singleton type with a name directory, or a Named / NamedNoUuid type
///   with a file directly under the type directory, is a finding naming the
///   path.
///
/// Denied types, tag types and types that are not allowlisted already have
/// their own findings from [`enforce_policy`] and are skipped here, so one
/// defect yields one finding. Tag-origin files follow the `tags/` layout
/// (CD5) and are not subject to the table.
fn enforce_rules(input: &RenderInput<'_>, mapped: &Mapped, findings: &mut Vec<DeployFinding>) {
    let is_tag_type = |module: &str, ty: &str| module == "ignition" && TAG_TYPES.contains(&ty);
    let mut finding = |file: String, message: String| {
        findings.push(DeployFinding {
            file,
            line: None,
            column: None,
            message,
        });
    };
    for entry in &input.manifest.types {
        if let Some((module, ty)) = entry.split_once('/')
            && !is_denied(module, ty)
            && !is_tag_type(module, ty)
            && type_rule(module, ty).is_none()
        {
            finding(
                format!("gateways/{}/manifest.yaml", input.gateway),
                format!(
                    "no render rule for {module}/{ty}; only evidence-backed types render (types entry `{entry}`)"
                ),
            );
        }
    }
    let mut no_rule: BTreeSet<(String, String)> = BTreeSet::new();
    let mut singleton_dirs: BTreeSet<(String, String, String)> = BTreeSet::new();
    for (path, origin) in &mapped.origins {
        if mapped.tags.contains_key(path) {
            continue;
        }
        let key = resource_key(path);
        let (module, ty) = (key.module.as_str(), key.ty.as_str());
        if is_denied(module, ty)
            || is_tag_type(module, ty)
            || !input.manifest.allows_type(module, ty)
        {
            continue;
        }
        let Some(rule) = type_rule(module, ty) else {
            if no_rule.insert((key.module.clone(), key.ty.clone())) {
                finding(
                    format!("{}resources/{module}/{ty}/", origin.prefix),
                    format!("no render rule for {module}/{ty}; only evidence-backed types render"),
                );
            }
            continue;
        };
        match (rule.uuid, &key.name) {
            (UuidRule::Singleton, Some(name)) => {
                if singleton_dirs.insert((key.module.clone(), key.ty.clone(), name.clone())) {
                    finding(
                        format!("{}resources/{module}/{ty}/{name}/", origin.prefix),
                        format!(
                            "{module}/{ty} is a singleton type: its files sit directly under the type directory, not in a name directory"
                        ),
                    );
                }
            }
            (UuidRule::Named | UuidRule::NamedNoUuid, None) => finding(
                origin.file.clone(),
                format!(
                    "{module}/{ty} is a named type: a resource sits in a name directory, not directly under the type directory"
                ),
            ),
            _ => {}
        }
    }
}

/// `module/type/name`: the id `uuids_minted` carries.
fn resource_id(module: &str, ty: &str, name: &str) -> String {
    format!("{module}/{ty}/{name}")
}

/// What `resource.json`'s `attributes.uuid` holds.
enum UuidState {
    Absent,
    Valid,
    /// Unusable; the message names the defect, never the value.
    Invalid(&'static str),
}

/// Inspect `attributes.uuid` of a parsed `resource.json`.
fn uuid_state(node: &Node) -> UuidState {
    if !matches!(node, Node::Obj(_)) {
        return UuidState::Invalid("resource.json is not a JSON object");
    }
    let attributes = match node.get("attributes") {
        None => return UuidState::Absent,
        Some(a @ Node::Obj(_)) => a,
        Some(_) => return UuidState::Invalid("attributes is not a JSON object"),
    };
    match attributes.get("uuid") {
        None => UuidState::Absent,
        Some(Node::Str(s)) if s.is_empty() => UuidState::Invalid("attributes.uuid is empty"),
        Some(Node::Str(s)) if is_canonical_uuid(s) => UuidState::Valid,
        Some(Node::Str(_)) => {
            UuidState::Invalid("attributes.uuid is not an 8-4-4-4-12 hexadecimal uuid")
        }
        Some(Node::Lit(l)) if l == "null" => UuidState::Invalid("attributes.uuid is null"),
        Some(_) => UuidState::Invalid("attributes.uuid is not a string"),
    }
}

/// Insert `"uuid": <uuid>` into `node`'s `attributes`, immediately after
/// `lastModification` and `legacyId` when present (otherwise first); append an
/// `attributes` holding only the uuid when there is none. The caller checked
/// [`uuid_state`] is `Absent`.
fn inject_uuid(node: &mut Node, uuid: &str) {
    let Node::Obj(members) = node else { return };
    let uuid = ("uuid".to_string(), Node::Str(uuid.to_string()));
    match members.iter_mut().find(|(k, _)| k == "attributes") {
        Some((_, Node::Obj(attrs))) => {
            let at = attrs
                .iter()
                .rposition(|(k, _)| k == "lastModification" || k == "legacyId")
                .map_or(0, |i| i + 1);
            attrs.insert(at, uuid);
        }
        Some(_) => {}
        None => members.push(("attributes".to_string(), Node::Obj(vec![uuid]))),
    }
}

/// The `resource.json` render writes for a new Named resource: the shape the
/// gateway writes for a resource created through its API (CD3), without
/// `lastModification` or `lastModificationSignature`, which an 8.3.6 gateway
/// loads without (CD2).
fn synthesized_resource_json(rule: &TypeRule, members: &[String], uuid: &str) -> Node {
    Node::Obj(vec![
        ("scope".to_string(), Node::Str(rule.scope.to_string())),
        ("version".to_string(), Node::Lit("1".to_string())),
        ("restricted".to_string(), Node::Lit("false".to_string())),
        ("overridable".to_string(), Node::Lit("true".to_string())),
        (
            "files".to_string(),
            Node::Arr(members.iter().map(|m| Node::Str(m.clone())).collect()),
        ),
        (
            "attributes".to_string(),
            Node::Obj(vec![
                ("uuid".to_string(), Node::Str(uuid.to_string())),
                ("enabled".to_string(), Node::Lit("true".to_string())),
            ]),
        ),
    ])
}

/// Stage 6 (21-07, DRND-06): resource identity.
///
/// Per resource of a type with a rule (tag-origin files follow CD5 and are
/// skipped; a resource whose `resource.json` failed stage 5 was already
/// reported):
///
/// - **Named**: an existing valid `attributes.uuid` is kept byte-for-byte; an
///   invalid one (empty, null, not a string, not 8-4-4-4-12 hex) is a finding
///   and is never replaced; a gateway-format `resource.json` without one gets
///   the derived uuid inserted (an opaque one is refused, never rewritten); no
///   `resource.json` at all gets a synthesized one, written by the Gson writer.
/// - **NamedNoUuid**: never synthesized or injected; a missing `resource.json`
///   is a finding, an existing uuid is kept (a gateway writes one for
///   `store-and-forward-engine`, CD3), an invalid one is a finding.
/// - **Singleton**: `resource.json` must exist and must carry no uuid.
///
/// Returns the ids (`module/type/name`) that received a uuid, byte-sorted.
fn assign_uuids(
    gateway: &str,
    mapped: &mut Mapped,
    json: &mut JsonTable,
    findings: &mut Vec<DeployFinding>,
) -> Vec<String> {
    // (module, type, name) -> member paths; a singleton's name is empty
    let mut groups: BTreeMap<(String, String, String), Vec<RelPath>> = BTreeMap::new();
    for path in mapped.files.keys() {
        if mapped.tags.contains_key(path) {
            continue;
        }
        let key = resource_key(path);
        let Some(rule) = type_rule(&key.module, &key.ty) else {
            continue;
        };
        let name = match (rule.uuid, key.name) {
            (UuidRule::Singleton, None) => String::new(),
            (UuidRule::Named | UuidRule::NamedNoUuid, Some(name)) => name,
            _ => continue, // wrong structure, reported by enforce_rules
        };
        groups
            .entry((key.module, key.ty, name))
            .or_default()
            .push(path.clone());
    }

    let mut minted = Vec::new();
    let mut synthesized: Vec<(RelPath, Vec<u8>, Node, Origin)> = Vec::new();
    for ((module, ty, name), members) in &groups {
        let rule = type_rule(module, ty).expect("grouped by a rule");
        let dir = if name.is_empty() {
            format!("{module}/{ty}")
        } else {
            format!("{module}/{ty}/{name}")
        };
        let rj_path = RelPath::new(&format!("{dir}/resource.json")).expect("valid path");
        let first = &mapped.origins[&members[0]];
        let rj_label = format!("{}resources/{rj_path}", first.prefix);
        let mut finding = |file: String, message: String| {
            findings.push(DeployFinding {
                file,
                line: None,
                column: None,
                message,
            });
        };

        if mapped.files.contains_key(&rj_path) {
            // unparseable: stage 5 reported it
            let Some((class, node)) = json.get_mut(&rj_path) else {
                continue;
            };
            let file = mapped.origins[&rj_path].file.clone();
            match rule.uuid {
                UuidRule::Singleton => {
                    let has_key = node
                        .get("attributes")
                        .is_some_and(|a| a.get("uuid").is_some());
                    if has_key {
                        finding(
                            file,
                            format!(
                                "{module}/{ty} is a singleton type; its resource.json must not carry attributes.uuid"
                            ),
                        );
                    } else if let UuidState::Invalid(why) = uuid_state(node) {
                        finding(file, why.to_string());
                    }
                }
                UuidRule::NamedNoUuid => {
                    if let UuidState::Invalid(why) = uuid_state(node) {
                        finding(file, why.to_string());
                    }
                }
                UuidRule::Named => match uuid_state(node) {
                    UuidState::Valid => {}
                    UuidState::Invalid(why) => finding(file, why.to_string()),
                    UuidState::Absent if *class == JsonClass::Opaque => finding(
                        file,
                        "resource.json is not in gateway format; render will not rewrite it; add attributes.uuid or re-pull it".to_string(),
                    ),
                    UuidState::Absent => {
                        inject_uuid(node, &name_based_uuid(gateway, module, ty, name));
                        mapped
                            .files
                            .insert(rj_path.clone(), node::write(node).into_bytes());
                        minted.push(resource_id(module, ty, name));
                    }
                },
            }
            continue;
        }

        match rule.uuid {
            UuidRule::Singleton | UuidRule::NamedNoUuid => finding(
                rj_label,
                format!(
                    "{module}/{ty} has no resource.json; render only synthesizes one for types whose resources carry a uuid"
                ),
            ),
            UuidRule::Named => {
                // CD2: a hand-written resource.json without lastModification
                // or lastModificationSignature loads and is not rewritten.
                let dir_prefix = format!("{dir}/");
                let names: Vec<String> = members
                    .iter()
                    .map(|p| p.as_str()[dir_prefix.len()..].to_string())
                    .collect();
                let uuid = name_based_uuid(gateway, module, ty, name);
                let node = synthesized_resource_json(rule, &names, &uuid);
                let bytes = node::write(&node).into_bytes();
                let origin = Origin {
                    file: rj_label,
                    prefix: first.prefix.clone(),
                };
                synthesized.push((rj_path, bytes, node, origin));
                minted.push(resource_id(module, ty, name));
            }
        }
    }
    for (path, bytes, node, origin) in synthesized {
        mapped.files.insert(path.clone(), bytes);
        mapped.origins.insert(path.clone(), origin);
        json.insert(path, (JsonClass::GatewayFormat, node));
    }
    minted.sort();
    minted
}

/// Stage 8: the rendered tree and its resource listing.
fn finish(mapped: Mapped, json: &JsonTable, uuids_minted: Vec<String>) -> RenderOutcome {
    let mut groups: BTreeMap<ResourceKey, Vec<String>> = BTreeMap::new();
    for path in mapped.files.keys() {
        let key = resource_key(path);
        let skip = if key.name.is_some() { 3 } else { 2 };
        let rest: Vec<&str> = path.components().skip(skip).collect();
        groups.entry(key).or_default().push(rest.join("/"));
    }
    let resources = groups
        .into_iter()
        .map(|(key, mut files)| {
            files.sort();
            RenderedResource {
                module: key.module,
                ty: key.ty,
                name: key.name,
                files,
            }
        })
        .collect();
    let mut warnings = Vec::new();
    if mapped.files.is_empty() {
        warnings.push("render produced no files".to_string());
    }
    RenderOutcome {
        tag_providers: mapped
            .tags
            .values()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        tree: RenderedTree {
            files: mapped.files,
        },
        resources,
        uuids_minted,
        opaque_files: json
            .iter()
            .filter(|(_, (class, _))| *class == JsonClass::Opaque)
            .map(|(path, _)| path.to_string())
            .collect(),
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TARGET: &[u8] =
        b"schema_version: 1\ngateway: g\nmode: dev\nnodes:\n  master:\n    profile: dev\n";

    fn run(types: &str, files: &[(&str, &str)]) -> Result<RenderOutcome, CoreError> {
        let manifest = Manifest::parse(
            "m.yaml",
            format!("schema_version: 1\ntypes:\n{types}").as_bytes(),
        )
        .unwrap();
        let target = Target::parse("t.yaml", TARGET).unwrap();
        let source = SourceTree::from_files(files.iter().copied()).unwrap();
        render(&RenderInput {
            gateway: "g",
            env: "dev",
            manifest: &manifest,
            target: &target,
            source: &source,
        })
    }

    #[test]
    fn unlisted_types_are_refused_together() {
        let err = run(
            "  - m/a\n",
            &[
                ("resources/m/a/x/config.json", "{}"),
                ("resources/m/b/x/config.json", "{}"),
                ("resources/m/b/y/config.json", "{}"),
                ("resources/n/c/config.json", "{}"),
            ],
        )
        .unwrap_err();
        let text = err.to_string();
        assert!(text.contains("gateways/g/resources/m/b/: type not in manifest.yaml types"));
        assert!(text.contains("gateways/g/resources/n/c/: type not in"));
        assert_eq!(text.matches("resources/m/b/").count(), 1, "{text}");
    }

    #[test]
    fn shallow_files_are_findings() {
        let err = run("  - m/a\n", &[("resources/m/x.json", "{}")]).unwrap_err();
        assert!(err.to_string().contains("gateways/g/resources/m/x.json"));
    }

    #[test]
    fn empty_tree_warns() {
        let out = run("  - ignition/schedule\n", &[]).unwrap();
        assert_eq!(out.warnings, ["render produced no files"]);
        assert!(out.resources.is_empty());
    }

    const NAMED_RJ: &str = "{\n  \"scope\": \"A\",\n  \"version\": 1,\n  \"restricted\": false,\n  \"overridable\": true,\n  \"files\": [\n    \"config.json\"\n  ],\n  \"attributes\": {\n    \"uuid\": \"4a14d749-bb78-47c7-847a-cafa17da4da8\",\n    \"enabled\": true\n  }\n}";
    const SINGLETON_RJ: &str = "{\n  \"scope\": \"A\",\n  \"version\": 1,\n  \"restricted\": false,\n  \"overridable\": true,\n  \"files\": [\n    \"config.json\"\n  ],\n  \"attributes\": {\n    \"enabled\": true\n  }\n}";

    #[test]
    fn named_resources_sort_by_name_bytes_and_singletons_follow_their_type() {
        let out = run(
            "  - ignition/schedule\n  - ignition/system-properties\n",
            &[
                ("resources/ignition/schedule/Z/config.json", "{}"),
                ("resources/ignition/schedule/Z/resource.json", NAMED_RJ),
                ("resources/ignition/schedule/B/sub/x.txt", "{}"),
                ("resources/ignition/schedule/B/resource.json", NAMED_RJ),
                ("resources/ignition/system-properties/config.json", "{}"),
                (
                    "resources/ignition/system-properties/resource.json",
                    SINGLETON_RJ,
                ),
            ],
        )
        .unwrap();
        let names: Vec<(&str, Option<&str>)> = out
            .resources
            .iter()
            .map(|r| (r.ty.as_str(), r.name.as_deref()))
            .collect();
        assert_eq!(
            names,
            [
                ("schedule", Some("B")),
                ("schedule", Some("Z")),
                ("system-properties", None)
            ]
        );
        assert_eq!(out.resources[0].files, ["resource.json", "sub/x.txt"]);
        assert_eq!(out.resources[2].files, ["config.json", "resource.json"]);
    }
}
