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

use super::apply::run_store::RunId;
use super::component_problem;
use super::ledger::RELEASE_RECORD_MAX_BYTES;
use super::node::{self, Node};
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
    /// The live project already equals the artifact, but the release record
    /// is absent (the git module's handover, D-12) or names another release:
    /// only the record is written, the project is not touched (K14).
    Record,
}

impl ProjectAction {
    /// Whether the action changes the gateway side: `create`, `import` and
    /// `record` do (the last one writes the release record, K14).
    pub fn would_change(self) -> bool {
        !matches!(self, ProjectAction::Skip)
    }
}

/// The longest release tag accepted, so a record or a flag cannot make a
/// result line unbounded.
const RELEASE_TAG_MAX: usize = 128;

/// A release tag, parsed as semver after at most one leading `v` (D-02, V4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// The tag as given, including its `v`.
    pub tag: String,
    /// The version the tag names.
    pub version: semver::Version,
}

/// How one release ranks against another (D-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseOrder {
    /// Higher semver precedence.
    Newer,
    /// The same precedence (build metadata is ignored).
    Equal,
    /// Lower semver precedence.
    Older,
}

impl Release {
    /// Parse `tag`: one optional leading `v`, then a semver version. A
    /// pre-release is older than its release (`v1.2.0-rc.1` < `v1.2.0`).
    /// Refuses `invalid_input` without echoing the text.
    pub fn parse(tag: &str) -> Result<Release, CoreError> {
        Release::try_parse(tag).ok_or_else(|| CoreError::InvalidInput {
            reason: "--release <tag>: not a semver tag (for example v1.4.0)".to_string(),
        })
    }

    fn try_parse(tag: &str) -> Option<Release> {
        if tag.is_empty() || tag.len() > RELEASE_TAG_MAX {
            return None;
        }
        let bare = tag.strip_prefix('v').unwrap_or(tag);
        let version = semver::Version::parse(bare).ok()?;
        Some(Release {
            tag: tag.to_string(),
            version,
        })
    }

    /// How `self` ranks against `other` by semver precedence; build metadata
    /// is ignored, so `v1.2.0+a` and `v1.2.0` are [`ReleaseOrder::Equal`].
    pub fn order_against(&self, other: &Release) -> ReleaseOrder {
        match self.version.cmp_precedence(&other.version) {
            std::cmp::Ordering::Greater => ReleaseOrder::Newer,
            std::cmp::Ordering::Equal => ReleaseOrder::Equal,
            std::cmp::Ordering::Less => ReleaseOrder::Older,
        }
    }
}

/// The only record schema this build reads and writes.
const RECORD_SCHEMA_VERSION: u32 = 1;

/// The record's keys, in write order.
const RECORD_FIELDS: [&str; 7] = [
    "schema_version",
    "gateway",
    "project",
    "release",
    "artifact_sha256",
    "run_id",
    "deployed_at",
];

/// What the last `ign` deploy of one project on one gateway left on the host
/// (`ign-deploy/<gateway>/projects/<project>/release.json`, V5). Later runners
/// read it for the downgrade gate (D-03), the first-deploy gate (D-12) and the
/// live-drift warning (K13), so it is parsed as strictly as the ledger: a
/// record that cannot be trusted refuses, it is never read as absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseRecord {
    /// Always 1.
    pub schema_version: u32,
    /// The gateway directory name the record belongs to (the target's
    /// `gateway`).
    pub gateway: String,
    /// The gateway project name.
    pub project: String,
    /// The release tag that was deployed, as given.
    pub release: String,
    /// Canonical sha256 of the artifact that was deployed: 64 lowercase hex.
    pub artifact_sha256: String,
    /// The run that wrote the record: `[A-Za-z0-9._-]{1,128}`.
    pub run_id: String,
    /// When the record was written, UTC (informational; never compared).
    pub deployed_at: String,
}

/// At most 64 printable characters of `text`, for a problem message.
fn shown(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(64).collect()
}

impl ReleaseRecord {
    /// Parse and validate `bytes` read from `location`, for `gateway` and
    /// `project`. Every refusal is [`CoreError::DeployRunInvalid`] naming
    /// `location`; the problem text names fields and never echoes a value the
    /// file holds.
    pub fn parse(
        bytes: &[u8],
        location: &str,
        gateway: &str,
        project: &str,
    ) -> Result<ReleaseRecord, CoreError> {
        let bad = |problem: String| CoreError::DeployRunInvalid {
            run_id: "unknown".to_string(),
            problem: format!("{location}: {problem}"),
        };
        if bytes.is_empty() {
            return Err(bad("the file is empty".to_string()));
        }
        if bytes.len() > RELEASE_RECORD_MAX_BYTES {
            return Err(bad(format!(
                "the file is {} bytes, over the {RELEASE_RECORD_MAX_BYTES} byte cap",
                bytes.len()
            )));
        }
        // Strict UTF-8, no BOM, no repeated key, bounded depth.
        let (_, root) = node::classify(bytes).map_err(bad)?;
        let Node::Obj(members) = &root else {
            return Err(bad("the top level is not a JSON object".to_string()));
        };
        for (key, _) in members {
            if !RECORD_FIELDS.contains(&key.as_str()) {
                return Err(bad(format!("unknown field `{}`", shown(key))));
            }
        }
        for field in RECORD_FIELDS {
            if root.get(field).is_none() {
                return Err(bad(format!("missing field `{field}`")));
            }
        }
        let record: ReleaseRecord = serde_json::from_slice(bytes).map_err(|e| {
            bad(format!(
                "a field has the wrong type (line {} column {})",
                e.line(),
                e.column()
            ))
        })?;

        if record.schema_version != RECORD_SCHEMA_VERSION {
            return Err(bad(format!(
                "schema_version is not {RECORD_SCHEMA_VERSION}, the only version this build reads"
            )));
        }
        if record.gateway != gateway {
            return Err(bad(format!(
                "gateway names a different gateway than the target (`{}`)",
                shown(gateway)
            )));
        }
        if record.project != project {
            return Err(bad(format!(
                "project names a different project than this deploy (`{}`)",
                shown(project)
            )));
        }
        let hex = record.artifact_sha256.len() == 64
            && record
                .artifact_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !hex {
            return Err(bad(
                "artifact_sha256 is not 64 lowercase hex digits".to_string()
            ));
        }
        if Release::try_parse(&record.release).is_none() {
            return Err(bad("release is not a semver tag".to_string()));
        }
        if RunId::parse(&record.run_id).is_err() {
            return Err(bad(
                "run_id is not 1 to 128 characters of [A-Za-z0-9._-]".to_string()
            ));
        }
        if record.deployed_at.is_empty()
            || record.deployed_at.len() > 64
            || !record.deployed_at.bytes().all(|b| b.is_ascii_graphic())
        {
            return Err(bad(
                "deployed_at is not 1 to 64 printable ASCII characters".to_string()
            ));
        }
        Ok(record)
    }

    /// The canonical bytes: pretty JSON in field order plus one LF.
    /// `parse(to_bytes(r)) == r`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = serde_json::to_vec_pretty(self).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }

    /// The release this record names. Valid for any record `parse` accepted.
    pub fn release(&self) -> Result<Release, CoreError> {
        Release::parse(&self.release)
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
        assert!(ProjectAction::Record.would_change());
        assert!(!ProjectAction::Skip.would_change());
    }

    fn rel(tag: &str) -> Release {
        Release::parse(tag).expect(tag)
    }

    #[test]
    fn release_order() {
        assert_eq!(
            rel("v1.2.0").order_against(&rel("1.1.9")),
            ReleaseOrder::Newer
        );
        assert_eq!(
            rel("1.1.9").order_against(&rel("v1.2.0")),
            ReleaseOrder::Older
        );
        assert_eq!(
            rel("v1.2.0-rc.1").order_against(&rel("v1.2.0")),
            ReleaseOrder::Older
        );
        assert_eq!(
            rel("v1.2.0").order_against(&rel("v1.2.0-rc.1")),
            ReleaseOrder::Newer
        );
        assert_eq!(
            rel("v1.2.0-rc.2").order_against(&rel("v1.2.0-rc.10")),
            ReleaseOrder::Older
        );
        // build metadata is not part of precedence
        assert_eq!(
            rel("v1.2.0+build").order_against(&rel("v1.2.0")),
            ReleaseOrder::Equal
        );
        assert_eq!(
            rel("v1.2.0+b").order_against(&rel("1.2.0+a")),
            ReleaseOrder::Equal
        );
        // the leading v is optional and the tag is kept as given
        assert_eq!(rel("v1.0.0").tag, "v1.0.0");
        assert_eq!(rel("1.0.0").tag, "1.0.0");
        for bad in [
            "vv1.0.0", "", "1.0", "v", "banana", "v1.0.0 ", " v1.0.0", "V1.0.0",
        ] {
            let err = Release::parse(bad).expect_err(bad);
            assert_eq!(err.code(), "invalid_input", "{bad:?}");
            assert!(err.to_string().contains("--release"), "{err}");
            // the refusal never echoes the text (too short to tell apart is skipped)
            assert!(bad.len() < 6 || !err.to_string().contains(bad), "{err}");
        }
        let long = format!("v1.0.0-{}", "a".repeat(200));
        assert!(Release::parse(&long).is_err(), "over the length cap");
    }

    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn record() -> ReleaseRecord {
        ReleaseRecord {
            schema_version: 1,
            gateway: "capgw".to_string(),
            project: "CapApp".to_string(),
            release: "v1.2.0".to_string(),
            artifact_sha256: HASH.to_string(),
            run_id: "20261020T101500Z-ab12".to_string(),
            deployed_at: "2026-10-20T10:15:00Z".to_string(),
        }
    }

    const LOC: &str = "ign-deploy/capgw/projects/CapApp/release.json";

    fn parse(bytes: &[u8]) -> Result<ReleaseRecord, CoreError> {
        ReleaseRecord::parse(bytes, LOC, "capgw", "CapApp")
    }

    fn with(edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
        let mut value = serde_json::to_value(record()).expect("value");
        edit(&mut value);
        serde_json::to_vec(&value).expect("bytes")
    }

    #[test]
    fn release_record_parse() {
        let bytes = record().to_bytes();
        assert!(bytes.ends_with(b"}\n"));
        assert_eq!(parse(&bytes).expect("round trip"), record());
        assert_eq!(record().release().expect("tag").tag, "v1.2.0");

        let refusals: Vec<(&str, Vec<u8>)> = vec![
            ("empty", Vec::new()),
            ("not json", b"nope".to_vec()),
            ("array", b"[]".to_vec()),
            ("bom", {
                let mut b = vec![0xEF, 0xBB, 0xBF];
                b.extend(record().to_bytes());
                b
            }),
            (
                "unknown field",
                with(|v| v["extra"] = serde_json::json!("SECRET-VALUE")),
            ),
            (
                "missing field",
                with(|v| {
                    v.as_object_mut().expect("object").remove("run_id");
                }),
            ),
            ("schema 2", with(|v| v["schema_version"] = 2.into())),
            (
                "wrong gateway",
                with(|v| v["gateway"] = "SECRET-GW".into()),
            ),
            (
                "wrong project",
                with(|v| v["project"] = "SECRET-PROJ".into()),
            ),
            ("uppercase hash", with(|v| v["artifact_sha256"] = HASH.to_uppercase().into())),
            ("short hash", with(|v| v["artifact_sha256"] = "abc".into())),
            ("bad release", with(|v| v["release"] = "SECRET-banana".into())),
            ("bad run id", with(|v| v["run_id"] = "../SECRET".into())),
            ("empty run id", with(|v| v["run_id"] = "".into())),
            ("bad stamp", with(|v| v["deployed_at"] = "".into())),
            ("wrong type", with(|v| v["schema_version"] = "1".into())),
            (
                "duplicate key",
                format!(
                    r#"{{"schema_version":1,"schema_version":1,"gateway":"capgw","project":"CapApp","release":"v1.0.0","artifact_sha256":"{HASH}","run_id":"r","deployed_at":"t"}}"#
                )
                .into_bytes(),
            ),
            ("over cap", vec![b' '; RELEASE_RECORD_MAX_BYTES + 1]),
        ];
        for (name, bytes) in refusals {
            let err = parse(&bytes).expect_err(name);
            assert_eq!(err.code(), "deploy_run_invalid", "{name}: {err}");
            let text = err.to_string();
            assert!(text.contains(LOC), "{name}: names the location: {text}");
            assert!(!text.contains("SECRET"), "{name}: echoes a value: {text}");
        }
    }

    #[test]
    fn release_record_at_the_cap_is_read_not_refused_for_size() {
        // exactly at the cap is not a size refusal (it fails on content)
        let err = parse(&vec![b' '; RELEASE_RECORD_MAX_BYTES]).expect_err("blank");
        assert!(!err.to_string().contains("byte cap"), "{err}");
    }
}
