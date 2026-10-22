//! `deploy/project.yaml` and the pure model of a project deploy (Phase 25).
//!
//! One Ignition project is deployed from its own repo (CONTEXT D-08). The
//! repo names the gateway project and the environments it deploys to in
//! `deploy/project.yaml`, a strict file like `manifest.yaml`: unknown fields
//! are refused and `schema_version` is 1. The parent project is read from the
//! root `project.json` (D-05). Everything here is pure: bytes in, models out,
//! no network, no clock.
//!
//! The comparison model lives here too: [`ProjectAction`] is what a deploy
//! would do and [`member_delta`] counts how the artifact's members differ
//! from the live project's (K5).

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::component_problem;
use super::project_artifact::CanonicalDigest;
use super::transport::winps::windows_component_problem;
use super::tree::read_repo_file;
use super::yamlio::{SchemaVersion, strict_parse};
use crate::error::{CoreError, DeployFinding};

/// Repo-relative path of the project file.
pub const PROJECT_FILE_REL: &str = "deploy/project.yaml";

/// Repo-relative path of the project descriptor every project repo holds at
/// its root (D-09).
pub const PROJECT_JSON_REL: &str = "project.json";

/// The parsed `deploy/project.yaml` (D-08, V9).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectFile {
    /// Always 1; any other value is refused by [`SchemaVersion`].
    pub schema_version: SchemaVersion,
    /// The project's name on the gateway. `project.json` holds only a display
    /// `title`, so the name cannot come from there. It is a path segment on
    /// the gateway host, so it must be one safe component on both systems.
    pub project: String,
    /// The environments (`targets/<ENV>.yaml`) this project deploys to.
    pub environments: Vec<String>,
}

impl ProjectFile {
    /// Parse and validate `bytes` (labelled `file` in errors). Every semantic
    /// defect is collected into ONE `deploy_tree_invalid`.
    pub fn parse(file: &str, bytes: &[u8]) -> Result<ProjectFile, CoreError> {
        let parsed: ProjectFile = strict_parse(file, bytes)?;
        parsed.validate(file)?;
        Ok(parsed)
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

        if let Some(why) = component_problem(&self.project) {
            push(format!("project: the name {why}"));
        } else if let Some(why) = windows_component_problem(&self.project) {
            push(format!("project: {why}"));
        }
        if self.environments.is_empty() {
            push("environments: must list at least one environment".to_string());
        }
        for (i, env) in self.environments.iter().enumerate() {
            if let Some(why) = component_problem(env) {
                push(format!("environments[{i}]: the name {why}"));
            }
            if self.environments[..i].contains(env) {
                push(format!("environments[{i}]: duplicate environment"));
            }
        }

        if findings.is_empty() {
            Ok(())
        } else {
            Err(CoreError::deploy_findings(findings))
        }
    }

    /// Refuse an environment the file does not list (`deploy_tree_invalid`
    /// naming [`PROJECT_FILE_REL`]).
    pub fn require_env(&self, env: &str) -> Result<(), CoreError> {
        if self.environments.iter().any(|e| e == env) {
            Ok(())
        } else {
            Err(CoreError::deploy_tree_invalid(
                PROJECT_FILE_REL,
                None,
                None,
                format!("--env {env} is not listed under environments"),
            ))
        }
    }
}

/// Read and parse [`PROJECT_FILE_REL`] below `repo`.
pub fn load_project_file(repo: &Path) -> Result<ProjectFile, CoreError> {
    let bytes = read_repo_file(
        repo,
        PROJECT_FILE_REL,
        Some("a project repo names its gateway project and environments here"),
    )?;
    ProjectFile::parse(PROJECT_FILE_REL, &bytes)
}

/// What a repo's root `project.json` says about the deploy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMeta {
    /// The parent project's name; `None` for a root project (absent, null or
    /// empty).
    pub parent: Option<String>,
}

/// Read the root `project.json` (D-09): it must exist as a regular file and
/// parse as a JSON object.
pub fn read_project_meta(repo: &Path) -> Result<ProjectMeta, CoreError> {
    if let Err(e) = std::fs::symlink_metadata(repo.join(PROJECT_JSON_REL))
        && e.kind() == std::io::ErrorKind::NotFound
    {
        return Err(CoreError::deploy_tree_invalid(
            PROJECT_JSON_REL,
            None,
            None,
            "does not exist (a project repo holds project.json at its root)",
        ));
    }
    let bytes = read_repo_file(repo, PROJECT_JSON_REL, None)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| {
        CoreError::deploy_tree_invalid(PROJECT_JSON_REL, None, None, "is not valid JSON")
    })?;
    let Some(object) = value.as_object() else {
        return Err(CoreError::deploy_tree_invalid(
            PROJECT_JSON_REL,
            None,
            None,
            "must be a JSON object",
        ));
    };
    let parent = object
        .get("parent")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Ok(ProjectMeta { parent })
}

/// What a deploy would do to the named project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectAction {
    /// The project is absent from the gateway and would be created (D-13).
    Create,
    /// The live project equals the artifact; nothing would change.
    Skip,
    /// The live project differs from the artifact and would be overwritten.
    Import,
}

impl ProjectAction {
    /// Whether the action changes the gateway.
    pub fn would_change(self) -> bool {
        !matches!(self, ProjectAction::Skip)
    }
}

/// How the artifact's members differ from the live project's (K5). The
/// gateway deletes every member an overwrite omits and never says so, so the
/// counts and the removed paths are the only place the removals show.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct MemberDelta {
    /// In the artifact only.
    pub added: usize,
    /// In both, canonical hashes differ.
    pub changed: usize,
    /// In the live project only (an overwrite would delete them).
    pub removed: usize,
    /// In both, canonical hashes equal.
    pub same: usize,
}

/// Compare the artifact with the live project (`None` when the project is
/// absent). Returns the counts and the removed member keys, byte-sorted.
pub fn member_delta(
    artifact: &CanonicalDigest,
    live: Option<&CanonicalDigest>,
) -> (MemberDelta, Vec<String>) {
    let mut delta = MemberDelta::default();
    let Some(live) = live else {
        delta.added = artifact.members.len();
        return (delta, Vec::new());
    };
    for (key, hash) in &artifact.members {
        match live.members.get(key) {
            None => delta.added += 1,
            Some(live_hash) if live_hash == hash => delta.same += 1,
            Some(_) => delta.changed += 1,
        }
    }
    let removed: Vec<String> = live
        .members
        .keys()
        .filter(|key| !artifact.members.contains_key(*key))
        .cloned()
        .collect();
    delta.removed = removed.len();
    (delta, removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn digest(members: &[(&str, &str)]) -> CanonicalDigest {
        CanonicalDigest {
            sha256: String::new(),
            members: members
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect::<BTreeMap<_, _>>(),
        }
    }

    const GOOD: &str = "schema_version: 1\nproject: CapApp\nenvironments: [dev, prod]\n";

    #[test]
    fn parses_a_valid_file_and_checks_the_environment() {
        let file = ProjectFile::parse(PROJECT_FILE_REL, GOOD.as_bytes()).expect("valid");
        assert_eq!(file.project, "CapApp");
        file.require_env("dev").expect("listed");
        let err = file.require_env("staging").expect_err("unlisted");
        assert_eq!(err.code(), "deploy_tree_invalid");
        assert!(err.to_string().contains(PROJECT_FILE_REL), "{err}");
    }

    #[test]
    fn unknown_fields_and_bad_versions_are_refused() {
        for bad in [
            "schema_version: 1\nproject: A\nenvironments: [dev]\nextra: 1\n",
            "schema_version: 2\nproject: A\nenvironments: [dev]\n",
            "schema_version: 1\nenvironments: [dev]\n",
        ] {
            let err = ProjectFile::parse(PROJECT_FILE_REL, bad.as_bytes()).expect_err(bad);
            assert_eq!(err.code(), "deploy_tree_invalid", "{bad}");
        }
    }

    #[test]
    fn semantic_defects_are_collected_into_one_error() {
        let bytes = "schema_version: 1\nproject: ../x\nenvironments: [dev, dev, '']\n";
        let err = ProjectFile::parse(PROJECT_FILE_REL, bytes.as_bytes()).expect_err("bad");
        let text = err.to_string();
        assert!(text.contains("project:"), "{text}");
        assert!(text.contains("duplicate environment"), "{text}");
        assert!(text.contains("environments[2]"), "{text}");
        let empty = "schema_version: 1\nproject: A\nenvironments: []\n";
        let err = ProjectFile::parse(PROJECT_FILE_REL, empty.as_bytes()).expect_err("empty");
        assert!(err.to_string().contains("at least one"), "{err}");
    }

    #[test]
    fn windows_unsafe_project_names_are_refused() {
        for name in ["CON", "a:b", "trailing.", "a*b"] {
            let bytes = format!("schema_version: 1\nproject: '{name}'\nenvironments: [dev]\n");
            assert!(
                ProjectFile::parse(PROJECT_FILE_REL, bytes.as_bytes()).is_err(),
                "{name}"
            );
        }
    }

    #[test]
    fn project_meta_reads_the_parent_and_refuses_a_missing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = read_project_meta(dir.path()).expect_err("no project.json");
        assert!(
            err.to_string()
                .contains("a project repo holds project.json at its root"),
            "{err}"
        );
        std::fs::write(
            dir.path().join("project.json"),
            br#"{"title":"T","parent":"WHK-Global"}"#,
        )
        .expect("write");
        assert_eq!(
            read_project_meta(dir.path()).expect("meta").parent,
            Some("WHK-Global".to_string())
        );
        std::fs::write(dir.path().join("project.json"), br#"{"parent":""}"#).expect("write");
        assert_eq!(read_project_meta(dir.path()).expect("meta").parent, None);
        std::fs::write(dir.path().join("project.json"), b"[]").expect("write");
        assert!(read_project_meta(dir.path()).is_err());
    }

    #[test]
    fn member_delta_counts_and_sorts_removed() {
        let artifact = digest(&[("a", "1"), ("b", "2"), ("c", "3")]);
        let live = digest(&[("a", "1"), ("b", "X"), ("z", "9"), ("y", "8")]);
        let (delta, removed) = member_delta(&artifact, Some(&live));
        assert_eq!(
            delta,
            MemberDelta {
                added: 1,
                changed: 1,
                removed: 2,
                same: 1
            }
        );
        assert_eq!(removed, ["y", "z"]);
        let (created, none) = member_delta(&artifact, None);
        assert_eq!(created.added, 3);
        assert!(none.is_empty());
    }

    #[test]
    fn only_skip_does_not_change_the_gateway() {
        assert!(ProjectAction::Create.would_change());
        assert!(ProjectAction::Import.would_change());
        assert!(!ProjectAction::Skip.would_change());
    }
}
