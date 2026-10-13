//! `manifest.yaml`: the declared tree's root file.
//!
//! The schema includes `verify.tag_probes[]` (SF-8) and `projects[]` now so
//! strict parsing does not reject them when Phases 22 and 25 start reading
//! them. Every struct is `deny_unknown_fields`; semantic defects (shapes
//! the type system cannot express) are collected into ONE
//! `deploy_tree_invalid`, with no line or column because they are not tied
//! to a single source position.

use serde::Deserialize;

use super::component_problem;
use super::yamlio::{SchemaVersion, strict_parse};
use crate::error::{CoreError, DeployFinding};

/// The parsed `manifest.yaml`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Always 1; any other value is refused by [`SchemaVersion`].
    pub schema_version: SchemaVersion,
    /// Free-form label of the house standard the tree follows. Informational;
    /// render does not interpret it.
    #[serde(default)]
    pub standards_version: Option<String>,
    /// Allowlist of `module/type` resource types the tree may declare
    /// (render, Phase 21). Required.
    pub types: Vec<String>,
    /// Tag provider directory names under `tags/` (render, Phase 21).
    #[serde(default)]
    pub tag_providers: Vec<String>,
    /// Pinned project artifacts (consumed by Phase 25).
    #[serde(default)]
    pub projects: Vec<ProjectPin>,
    /// Post-deploy verification inputs (consumed by Phase 22).
    #[serde(default)]
    pub verify: Option<Verify>,
}

/// One pinned project artifact (Phase 25).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPin {
    /// Project name on the gateway.
    pub name: String,
    /// Version label of the artifact.
    pub version: String,
    /// Lowercase hex sha256 (64 chars) of the artifact bytes.
    pub sha256: String,
    /// Where the artifact is fetched from; opaque to Phase 21.
    pub artifact: String,
}

/// Verification inputs (SF-8, consumed by Phase 22). Phase 21 only parses them.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verify {
    /// Tag paths read back after a deploy. An absent list is a Phase 22
    /// warning, not a render concern.
    #[serde(default)]
    pub tag_probes: Vec<String>,
}

impl Manifest {
    /// Parse and validate `bytes` (labelled `file` in errors).
    pub fn parse(file: &str, bytes: &[u8]) -> Result<Manifest, CoreError> {
        let manifest: Manifest = strict_parse(file, bytes)?;
        manifest.validate(file)?;
        Ok(manifest)
    }

    /// Whether `module/ty` is on the declared type allowlist (exact match).
    pub fn allows_type(&self, module: &str, ty: &str) -> bool {
        self.types
            .iter()
            .any(|t| t.split_once('/') == Some((module, ty)))
    }

    fn validate(&self, file: &str) -> Result<(), CoreError> {
        let mut findings = Vec::new();
        let mut push = |message: String| {
            findings.push(DeployFinding {
                file: file.to_string(),
                line: None,
                column: None,
                message,
            });
        };

        for (i, t) in self.types.iter().enumerate() {
            match t.split_once('/') {
                Some((module, ty)) if !ty.contains('/') => {
                    for (what, half) in [("module", module), ("type", ty)] {
                        if let Some(why) = component_problem(half) {
                            push(format!("types[{i}] {t:?}: the {what} half {why}"));
                        }
                    }
                }
                _ => push(format!(
                    "types[{i}] {t:?}: must have the form `module/type`"
                )),
            }
            if self.types[..i].contains(t) {
                push(format!("types[{i}] {t:?}: duplicate type"));
            }
        }
        for (i, p) in self.tag_providers.iter().enumerate() {
            if let Some(why) = component_problem(p) {
                push(format!("tag_providers[{i}] {p:?}: {why}"));
            }
            if self.tag_providers[..i].contains(p) {
                push(format!("tag_providers[{i}] {p:?}: duplicate tag provider"));
            }
        }
        for (i, pin) in self.projects.iter().enumerate() {
            if pin.name.is_empty() {
                push(format!("projects[{i}].name: must not be empty"));
            }
            let hex = pin.sha256.len() == 64
                && pin
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
            if !hex {
                push(format!(
                    "projects[{i}].sha256: must be 64 lowercase hex characters"
                ));
            }
        }

        if findings.is_empty() {
            Ok(())
        } else {
            Err(CoreError::deploy_findings(findings))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn findings(e: CoreError) -> Vec<DeployFinding> {
        match e {
            CoreError::DeployTreeInvalid(d) => d.findings,
            other => panic!("expected deploy_tree_invalid, got {other}"),
        }
    }

    #[test]
    fn full_manifest_parses() {
        let yaml = format!(
            "schema_version: 1\nstandards_version: flux-2026\ntypes:\n  - ignition/database-connection\n  - ignition/tag-provider\ntag_providers: [default, edge]\nprojects:\n  - name: scada\n    version: 1.2.3\n    sha256: {SHA}\n    artifact: https://example.invalid/scada.zip\nverify:\n  tag_probes:\n    - \"[default]Plant/Heartbeat\"\n"
        );
        let m = Manifest::parse("manifest.yaml", yaml.as_bytes()).expect("parses");
        assert_eq!(m.standards_version.as_deref(), Some("flux-2026"));
        assert_eq!(m.types.len(), 2);
        assert_eq!(m.tag_providers, ["default", "edge"]);
        assert_eq!(m.projects[0].name, "scada");
        assert_eq!(m.projects[0].sha256, SHA);
        assert_eq!(
            m.verify.expect("verify").tag_probes,
            ["[default]Plant/Heartbeat"]
        );
    }

    #[test]
    fn minimal_manifest_parses() {
        let m = Manifest::parse(
            "manifest.yaml",
            b"schema_version: 1\ntypes: [ignition/database-connection]\n",
        )
        .expect("parses");
        assert!(m.tag_providers.is_empty());
        assert!(m.projects.is_empty());
        assert_eq!(m.verify, None);
        assert!(m.allows_type("ignition", "database-connection"));
        assert!(!m.allows_type("ignition", "tag-provider"));
        assert!(!m.allows_type("ignition", "database"));

        let m = Manifest::parse(
            "manifest.yaml",
            b"schema_version: 1\ntypes: []\nverify: {}\n",
        )
        .expect("empty verify");
        assert_eq!(m.verify.expect("verify").tag_probes, Vec::<String>::new());
    }

    #[test]
    fn manifest_semantic_findings_collected() {
        let yaml = "schema_version: 1\ntypes:\n  - nosep\n  - ignition/a\n  - ignition/a\ntag_providers: [dup, dup, \"a/b\"]\nprojects:\n  - name: p\n    version: '1'\n    sha256: ABC\n    artifact: x\n";
        let found =
            findings(Manifest::parse("manifest.yaml", yaml.as_bytes()).expect_err("invalid"));
        let all: Vec<&str> = found.iter().map(|f| f.message.as_str()).collect();
        let has = |needle: &str| all.iter().any(|m| m.contains(needle));
        assert!(has("types[0]") && has("module/type"), "{all:?}");
        assert!(has("duplicate type"), "{all:?}");
        assert!(has("duplicate tag provider"), "{all:?}");
        assert!(has("tag_providers[2]") && has("path separator"), "{all:?}");
        assert!(has("projects[0].sha256"), "{all:?}");
        for f in &found {
            assert_eq!(f.file, "manifest.yaml");
            assert_eq!((f.line, f.column), (None, None));
        }
    }

    #[test]
    fn manifest_unknown_field_is_positioned() {
        let e = Manifest::parse(
            "manifest.yaml",
            b"schema_version: 1\ntypes: []\nverify:\n  extra: 1\n",
        )
        .expect_err("unknown nested field");
        let f = findings(e).remove(0);
        assert_eq!((f.line, f.column), (Some(4), Some(3)));
    }
}
