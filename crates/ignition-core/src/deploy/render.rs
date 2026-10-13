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
//! 6. uuid (21-07)
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
use super::rules::{GATEWAY_OWNED_FILES, is_denied};
use super::target::Target;
use super::tree::{RenderedTree, SourceTree};
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
    /// Collection paths of files that received a minted uuid (none until
    /// 21-07), byte-ordered.
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
    let mapped = map_to_collection(effective, &mut findings);
    enforce_policy(input, &mapped, &mut findings);
    let json = classify_json(&mapped.files, &mapped.origins, &mut findings);
    if !findings.is_empty() {
        return Err(CoreError::deploy_findings(findings));
    }
    Ok(finish(mapped, &json))
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

/// Parsed JSON members keyed by collection path, with their class. Stages 6
/// (uuid, 21-07) and 7 (lints, 21-08) read this table.
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

/// Stage 8: the rendered tree and its resource listing.
fn finish(mapped: Mapped, json: &JsonTable) -> RenderOutcome {
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
        uuids_minted: Vec::new(),
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
        let out = run("  - m/a\n", &[]).unwrap();
        assert_eq!(out.warnings, ["render produced no files"]);
        assert!(out.resources.is_empty());
    }

    #[test]
    fn singletons_sort_first_and_files_are_resource_relative() {
        let out = run(
            "  - m/a\n",
            &[
                ("resources/m/a/Z/config.json", "{}"),
                ("resources/m/a/B/sub/x.txt", "{}"),
                ("resources/m/a/config.json", "{}"),
            ],
        );
        // singleton `config.json` and named resources of the same type are
        // distinct keys; the singleton orders first
        let out = out.unwrap();
        let names: Vec<Option<&str>> = out.resources.iter().map(|r| r.name.as_deref()).collect();
        assert_eq!(names, [None, Some("B"), Some("Z")]);
        assert_eq!(out.resources[1].files, ["sub/x.txt"]);
        assert_eq!(out.resources[0].files, ["config.json"]);
    }
}
