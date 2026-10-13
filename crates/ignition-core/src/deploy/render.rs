//! The render pipeline: a declared gateway tree to the bytes the gateway
//! holds on disk.
//!
//! Stages, in order (later plans insert at the named stage; each stage is a
//! private function below):
//!
//! 1. **collect** the source tree ([`super::tree::SourceTree`]) - done by the
//!    caller and handed in as [`RenderInput::source`]
//! 2. overlay `modes/<env>` (21-06)
//! 3. **map** repo paths to collection paths (tags in 21-06)
//! 4. **policy**: the manifest type allowlist and its D-02 refusal (21-04);
//!    deny list, gateway-owned, dot-names and tag providers (21-06)
//! 5. JSON classification, the Gson gate (21-05)
//! 6. uuid (21-07)
//! 7. lints (21-08)
//! 8. **finish**: build the [`RenderedTree`], resources and the digest
//!
//! Pure: no filesystem, no network, no clock, no randomness. Ordered
//! collections only, so equal inputs give equal outputs on every OS.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::layout::{RelPath, ResourceKey, collection_path, resource_key};
use super::manifest::Manifest;
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
    /// Tag providers rendered (none until 21-06), byte-ordered.
    pub tag_providers: Vec<String>,
    /// Collection paths of files that received a minted uuid (none until
    /// 21-07), byte-ordered.
    pub uuids_minted: Vec<String>,
    /// Collection paths passed through as opaque bytes (none until 21-05),
    /// byte-ordered.
    pub opaque_files: Vec<String>,
    /// Warnings in pipeline order.
    pub warnings: Vec<String>,
}

/// Render `input`. Every policy defect is collected and raised once as a
/// `deploy_tree_invalid`.
pub fn render(input: &RenderInput<'_>) -> Result<RenderOutcome, CoreError> {
    let mut findings = Vec::new();
    let mapped = map_to_collection(input, &mut findings);
    enforce_allowlist(input, &mapped, &mut findings);
    if !findings.is_empty() {
        return Err(CoreError::deploy_findings(findings));
    }
    Ok(finish(mapped))
}

/// Stage 3: gateway-relative paths to collection paths.
fn map_to_collection(
    input: &RenderInput<'_>,
    findings: &mut Vec<DeployFinding>,
) -> BTreeMap<RelPath, Vec<u8>> {
    let mut mapped = BTreeMap::new();
    for (path, bytes) in &input.source.files {
        match collection_path(path) {
            Ok(collection) => {
                mapped.insert(collection, bytes.clone());
            }
            Err(why) => findings.push(DeployFinding {
                file: format!("gateways/{}/{path}", input.gateway),
                line: None,
                column: None,
                message: why,
            }),
        }
    }
    mapped
}

/// Stage 4 (21-04 share): one finding per type directory that is not on the
/// manifest allowlist (D-02: refused, never skipped).
fn enforce_allowlist(
    input: &RenderInput<'_>,
    mapped: &BTreeMap<RelPath, Vec<u8>>,
    findings: &mut Vec<DeployFinding>,
) {
    let mut refused: BTreeSet<(String, String)> = BTreeSet::new();
    for path in mapped.keys() {
        let key = resource_key(path);
        if !input.manifest.allows_type(&key.module, &key.ty) {
            refused.insert((key.module, key.ty));
        }
    }
    for (module, ty) in refused {
        findings.push(DeployFinding {
            file: format!("gateways/{}/resources/{module}/{ty}/", input.gateway),
            line: None,
            column: None,
            message: "type not in manifest.yaml types (D-02: un-allowlisted type directories are refused, not skipped)".to_string(),
        });
    }
}

/// Stage 8: the rendered tree and its resource listing.
fn finish(mapped: BTreeMap<RelPath, Vec<u8>>) -> RenderOutcome {
    let mut groups: BTreeMap<ResourceKey, Vec<String>> = BTreeMap::new();
    for path in mapped.keys() {
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
    if mapped.is_empty() {
        warnings.push("render produced no files".to_string());
    }
    RenderOutcome {
        tree: RenderedTree { files: mapped },
        resources,
        tag_providers: Vec::new(),
        uuids_minted: Vec::new(),
        opaque_files: Vec::new(),
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
