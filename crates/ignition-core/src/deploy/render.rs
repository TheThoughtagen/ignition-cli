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
//!    (CD5), root-level tag files (CD5a) and nested type folders (CD5b)
//!    included; two sources on one collection path are refused
//! 4. **policy**: the manifest type allowlist and its D-02 refusal, the deny
//!    list, gateway-owned files, dot-names, undeclared tag providers and tag
//!    types under `resources/` (21-06), and tag folders named like a tag
//!    member file (CD5d, 261030-8m8)
//! 5. **JSON classification**, the Gson gate (21-05): `node::classify` on every
//!    `.json` member; findings name the source path; parsed nodes are kept.
//!    Then the tag marker check (CD5c, 261030-8m8): each tag folder's
//!    `unary-resource.json` `files` must list exactly its member files
//! 6. **uuid** (21-07): the rule-table checks of stage 4 (`enforce_rules`),
//!    then per Named resource keep, inject or synthesize `attributes.uuid`
//! 7. lints (21-08): secrets (Embedded, plaintext at schema paths, plaintext
//!    by key name) over the parsed nodes, then the tree lints; one final raise
//! 8. **finish**: build the [`RenderedTree`], resources and the digest
//!
//! Pure: no filesystem, no network, no clock, no randomness. Ordered
//! collections only, so equal inputs give equal outputs on every OS.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::exclude;
use super::layout::{RelPath, ResourceKey, TAG_TYPES, collection_path, resource_key, tag_provider};
use super::lint::{lint_files, scan_embedded, scan_secrets};
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
    // Excluded tag providers (gateway-owned) cannot be declared: in the
    // manifest's `tag_providers` or as a `tags/<P>/` tree (enforce_policy).
    let excluded = exclude::effective(input.target, Some(input.manifest));
    findings.extend(exclude::contradictions(
        &excluded,
        input.manifest,
        &format!("gateways/{}/manifest.yaml", input.gateway),
    ));
    let effective = overlay(input, &mut findings);
    let effective = screen_names(effective, &mut findings);
    let mut mapped = map_to_collection(effective, &mut findings);
    enforce_policy(input, &mapped, &excluded, &mut findings);
    enforce_rules(input, &mapped, &mut findings);
    let mut json = classify_json(&mapped.files, &mapped.origins, &mut findings);
    check_tag_markers(&mapped, &json, &mut findings);
    let minted = assign_uuids(input.gateway, &mut mapped, &mut json, &mut findings);
    let mut warnings = Vec::new();
    let secret_paths = scan_all_secrets(&mapped, &json, &mut warnings);
    findings.extend(lint_files(
        &mapped.files,
        input.target,
        input.env,
        &|path| {
            mapped
                .origins
                .get(path)
                .map_or_else(|| path.to_string(), |o| o.file.clone())
        },
    ));
    // one raise: secret locations win, every other finding rides along
    if !secret_paths.is_empty() {
        return Err(CoreError::embedded_secret(secret_paths, findings));
    }
    if !findings.is_empty() {
        return Err(CoreError::deploy_findings(findings));
    }
    Ok(finish(mapped, &json, minted, warnings))
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
/// `resources/`, and undeclared or excluded tag providers (`excluded` is the
/// effective exclusion set, [`exclude::effective`]).
///
/// Findings are one per type directory or provider, naming the first source
/// in byte order (an overlay path when the overlay supplied it).
fn enforce_policy(
    input: &RenderInput<'_>,
    mapped: &Mapped,
    excluded: &[String],
    findings: &mut Vec<DeployFinding>,
) {
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
    // CD5d (261030-8m8-LIVE-CAPTURES.md): the gateway accepts a tag folder
    // named `tags.json` despite its own refusal text, writes it as a
    // directory, then loses the provider's root `tags.json` to an I/O error.
    // Matched without case: NTFS and the gateway (CD6) fold case.
    for path in mapped.tags.keys() {
        if let Some((folders, _)) = marked_tag_parts(path)
            && folders.iter().any(|f| is_tag_member_name(f))
        {
            findings.push(finding(
                mapped.origins[path].file.clone(),
                "a tag folder is named like a tag file (tags.json, udts.json or unary-resource.json); the gateway would store it beside that file and lose the provider's root tag data (CD5d)".to_string(),
            ));
        }
    }
    for (provider, prefix) in &providers {
        if exclude::is_excluded(provider, excluded) {
            findings.push(finding(
                format!("{prefix}tags/{provider}/"),
                "tag provider is excluded (by exclude_tag_providers or the built-in System default); an excluded provider is gateway-owned and cannot be declared".to_string(),
            ));
        } else if !input.manifest.tag_providers.iter().any(|p| p == provider) {
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

/// The tag types whose folders hold a `unary-resource.json` marker and the
/// `tags.json` / `udts.json` member files it lists (CD5, CD5a, CD5b).
const MARKED_TAG_TYPES: [&str; 2] = ["tag-definition", "tag-type-definition"];

/// The member files a tag marker can list.
const TAG_MEMBERS: [&str; 2] = ["tags.json", "udts.json"];

/// A tag folder's marker file.
const TAG_MARKER: &str = "unary-resource.json";

/// The folder components and the file name of a collection path
/// `ignition/<marked tag type>/<P>/<folders...>/<file>`; `None` for any
/// other path.
fn marked_tag_parts(path: &RelPath) -> Option<(Vec<&str>, &str)> {
    let parts: Vec<&str> = path.components().collect();
    if parts.len() < 4 || parts[0] != "ignition" || !MARKED_TAG_TYPES.contains(&parts[1]) {
        return None;
    }
    let (file, folders) = parts[3..].split_last()?;
    Some((folders.to_vec(), file))
}

/// Whether `name` is a tag member or marker file name, ASCII case folded.
fn is_tag_member_name(name: &str) -> bool {
    TAG_MEMBERS
        .iter()
        .chain(std::iter::once(&TAG_MARKER))
        .any(|m| m.eq_ignore_ascii_case(name))
}

/// Stage 5 tail (CD5c, 261030-8m8-LIVE-CAPTURES.md): the gateway loads only
/// the member files a tag folder's `unary-resource.json` lists in `files`.
/// An unlisted `tags.json` or `udts.json` is silently skipped, and a listed
/// one that is missing logs an ERROR. So every rendered marker under the two
/// marked tag types, at every level (the provider root, folders and nested
/// type folders alike), must list exactly the member files beside it.
///
/// Fail-closed (261030-8m8 review):
///
/// - any file whose name is not exactly `tags.json`, `udts.json` or
///   `unary-resource.json` is a finding on that file (WR-03): every CD5
///   capture, on both hosts, shows the gateway writing only those three in a
///   tag folder, and it loads nothing else. A letter-case variant is named as
///   such: a Linux gateway never matches it, and an NTFS host folds it onto
///   the real name, so either way it is not what the marker lists (CD6);
/// - a `tags.json` or `udts.json` whose own folder renders no marker is a
///   finding on that file (WR-01): the gateway would never load it, and the
///   plan would group it under an ancestor unit;
/// - a marker that is not a JSON object, or has no `files` member, is a
///   finding on the marker (WR-02): every gateway-written marker is an object
///   with `files` (`files: []` when the folder has no member file);
/// - otherwise one finding per marker whose `files` disagrees, naming its
///   source path, the member names missing from `files` and those listed but
///   absent. Any other `files` entry is only counted, never echoed.
///
/// A marker that does not parse already has a stage-5 finding.
fn check_tag_markers(mapped: &Mapped, json: &JsonTable, findings: &mut Vec<DeployFinding>) {
    let mut finding = |file: String, message: String| {
        findings.push(DeployFinding {
            file,
            line: None,
            column: None,
            message,
        });
    };
    for path in mapped.files.keys() {
        if !mapped.tags.contains_key(path) {
            continue;
        }
        let Some((_, file)) = marked_tag_parts(path) else {
            continue;
        };
        let origin = mapped.origins[path].file.clone();
        let dir = path
            .as_str()
            .strip_suffix(file)
            .expect("the path ends in its file name");
        if !TAG_MEMBERS.contains(&file) && file != TAG_MARKER {
            let message = if is_tag_member_name(file) {
                "a tag file is named like tags.json, udts.json or unary-resource.json in another letter case; the gateway matches these names exactly, and an NTFS host folds it onto the real file, so the marker never lists it (CD5c, CD6)"
            } else {
                "a tag folder holds only tags.json, udts.json and its unary-resource.json marker; the gateway writes no other file there and never loads one (CD5 captures, CD5c)"
            };
            finding(origin, message.to_string());
            continue;
        }
        if TAG_MEMBERS.contains(&file) {
            let marked = RelPath::new(&format!("{dir}{TAG_MARKER}"))
                .is_ok_and(|m| mapped.files.contains_key(&m));
            if !marked {
                finding(
                    origin,
                    "a tag member file needs a unary-resource.json in its own folder listing it; the gateway loads only the member files a marker lists, so without one this file is never loaded (CD5c)".to_string(),
                );
            }
            continue;
        }
        if file != TAG_MARKER {
            continue;
        }
        // unparseable: stage 5 reported it
        let Some((_, node)) = json.get(path) else {
            continue;
        };
        if !matches!(node, Node::Obj(_)) {
            finding(
                origin,
                "unary-resource.json is not a JSON object; the gateway writes every tag marker as an object whose `files` lists its member files (CD5c)".to_string(),
            );
            continue;
        }
        let Some(listed) = node.get("files") else {
            finding(
                origin,
                "`files` is missing; the gateway loads only the member files a tag marker lists in `files`, and writes `files: []` when there are none (CD5c)".to_string(),
            );
            continue;
        };
        let present: BTreeSet<&str> = TAG_MEMBERS
            .iter()
            .copied()
            .filter(|m| {
                RelPath::new(&format!("{dir}{m}")).is_ok_and(|p| mapped.files.contains_key(&p))
            })
            .collect();
        let Node::Arr(items) = listed else {
            finding(
                origin,
                "`files` is not an array of file names (CD5c)".to_string(),
            );
            continue;
        };
        let mut names: BTreeSet<&str> = BTreeSet::new();
        let mut other = 0usize;
        for item in items {
            match item.as_str() {
                Some(name) if TAG_MEMBERS.contains(&name) => {
                    names.insert(name);
                }
                _ => other += 1,
            }
        }
        let mut parts = Vec::new();
        let unlisted: Vec<&str> = present.difference(&names).copied().collect();
        if !unlisted.is_empty() {
            parts.push(format!("not listed: {}", unlisted.join(", ")));
        }
        let absent: Vec<&str> = names.difference(&present).copied().collect();
        if !absent.is_empty() {
            parts.push(format!("listed but absent: {}", absent.join(", ")));
        }
        if other > 0 {
            parts.push(format!(
                "{other} other {}",
                if other == 1 { "entry" } else { "entries" }
            ));
        }
        if !parts.is_empty() {
            finding(
                origin,
                format!(
                    "`files` must list exactly the tags.json and udts.json beside it; the gateway skips an unlisted file and logs an error for a listed one that is missing (CD5c): {}",
                    parts.join("; ")
                ),
            );
        }
    }
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

/// Stage 7 (21-08, DRND-07), secret half: scan every parsed `.json` member.
///
/// Resource files get the full rule set for their type; tag-origin files
/// (CD5) have no resource type and get the generic Embedded rule only.
/// Returns the refused locations in (collection path, document) order; the
/// scan output carries paths only, never a value. Warnings are appended to
/// `warnings`.
fn scan_all_secrets(mapped: &Mapped, json: &JsonTable, warnings: &mut Vec<String>) -> Vec<String> {
    let mut refused = Vec::new();
    for (path, (_, node)) in json {
        let origin = mapped
            .origins
            .get(path)
            .map_or_else(|| path.to_string(), |o| o.file.clone());
        let scan = if mapped.tags.contains_key(path) {
            scan_embedded(&origin, node)
        } else {
            let key = resource_key(path);
            let skip = if key.name.is_some() { 3 } else { 2 };
            let rest: Vec<&str> = path.components().skip(skip).collect();
            scan_secrets(&origin, &key, &rest.join("/"), node)
        };
        refused.extend(scan.refused);
        warnings.extend(scan.warnings);
    }
    refused
}

/// Stage 8: the rendered tree and its resource listing.
fn finish(
    mapped: Mapped,
    json: &JsonTable,
    uuids_minted: Vec<String>,
    mut warnings: Vec<String>,
) -> RenderOutcome {
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
