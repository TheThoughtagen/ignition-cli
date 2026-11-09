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
use super::manifest::Manifest;
use super::node::{self, Node};
use super::project_artifact::CanonicalDigest;
use super::target::Target;
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
    /// The tag providers this project owns (Phase 26, D-04): each one is read
    /// from `tags/<P>/` in the repo and deployed with the project in one run.
    /// Providers not listed are never touched. Absent means none. A build
    /// older than Phase 26 refuses the key (`deny_unknown_fields`), which
    /// fails closed.
    #[serde(default)]
    pub tag_providers: Vec<String>,
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
        // A provider name becomes the directory `tags/<P>/` and a run-directory
        // file name, so it must be one safe component on both systems. The
        // gateway folds provider names, so two entries that differ only in
        // ASCII case are one provider listed twice.
        for (i, provider) in self.tag_providers.iter().enumerate() {
            if let Some(why) = component_problem(provider) {
                push(format!("tag_providers[{i}]: the name {why}"));
            } else if let Some(why) = windows_component_problem(provider) {
                push(format!("tag_providers[{i}]: {why}"));
            }
            if self.tag_providers[..i]
                .iter()
                .any(|seen| seen.eq_ignore_ascii_case(provider))
            {
                push(format!("tag_providers[{i}]: duplicate tag provider"));
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

/// Repo-relative directory of the owned providers' settings (Phase 26, D-09,
/// D-12): `deploy/tag-providers/<P>.json`, beside [`PROJECT_FILE_REL`] and
/// outside `tags/`, which the git module reads.
pub const TAG_PROVIDERS_DIR_REL: &str = "deploy/tag-providers";

/// One owned provider's settings, `deploy/tag-providers/<P>.json`. Strict:
/// unknown keys are refused, `schema_version` is 1 and `config` is an object
/// (the provider's `config` as the gateway's tag provider resource holds it).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSettings {
    /// Always 1; any other value is refused by [`SchemaVersion`].
    pub schema_version: SchemaVersion,
    /// Whether the provider is enabled.
    pub enabled: bool,
    /// The provider's description; absent means none.
    #[serde(default)]
    pub description: Option<String>,
    /// The provider's configuration object.
    pub config: serde_json::Value,
}

/// The repo-relative settings file of `provider`.
fn provider_settings_rel(provider: &str) -> String {
    format!("{TAG_PROVIDERS_DIR_REL}/{provider}.json")
}

/// Read and validate `deploy/tag-providers/<provider>.json` below `repo`.
///
/// A missing file, invalid JSON (a duplicate key included), an unknown key, a
/// `schema_version` other than 1 or a `config` that is not an object is
/// `deploy_tree_invalid` naming the file. ign would write these settings, so
/// an Embedded secret anywhere in them is `embedded_secret` naming the file
/// and the JSON path (D-14). No message carries a value.
pub fn load_provider_settings(repo: &Path, provider: &str) -> Result<ProviderSettings, CoreError> {
    let rel = provider_settings_rel(provider);
    let bytes = read_repo_file(
        repo,
        &rel,
        Some("each provider a project lists in tag_providers has its settings here"),
    )?;
    let invalid = |line: Option<usize>, column: Option<usize>, why: String| {
        CoreError::deploy_tree_invalid(rel.as_str(), line, column, why)
    };
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| invalid(None, None, "is not valid UTF-8".to_string()))?;
    let tree = node::parse(text)
        .map_err(|why| invalid(None, None, format!("is not valid JSON ({why})")))?;
    let settings: ProviderSettings = serde_json::from_str(text).map_err(|e| {
        let mut why = e.to_string();
        if let Some(at) = why.rfind(" at line ") {
            why.truncate(at);
        }
        // serde_json quotes the offending scalar in a type error: say what is
        // wrong without echoing it.
        if why.starts_with("invalid type") || why.starts_with("invalid value") {
            why = "a value has the wrong type".to_string();
        }
        invalid(Some(e.line()), Some(e.column()), why)
    })?;
    if !settings.config.is_object() {
        return Err(invalid(None, None, "config: must be an object".to_string()));
    }
    let scan = super::lint::scan_embedded(&rel, &tree);
    if !scan.refused.is_empty() {
        return Err(CoreError::embedded_secret(scan.refused, Vec::new()));
    }
    Ok(settings)
}

/// The gateway-config manifest of `gateway`, relative to the targets root:
/// `gateways/<gateway>/manifest.yaml` (D-05).
pub fn gateway_manifest_rel(gateway: &str) -> String {
    format!("gateways/{gateway}/manifest.yaml")
}

/// One list of providers no project may own, with the label a refusal names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadOnlySource {
    /// Where the list comes from, as a refusal names it (`the built-in
    /// default`, `read_only_tag_providers of targets/dev.yaml`, ...).
    pub label: String,
    /// The providers it holds, as written.
    pub names: Vec<String>,
}

/// Every source of read-only tag providers (D-06), in the order a refusal
/// looks them up: the built-in default (`System`), the target's
/// `exclude_tag_providers` and `read_only_tag_providers`, then the
/// manifest's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadOnlySources {
    /// The target's gateway, for the warning that names its manifest path.
    pub gateway: String,
    /// The lists.
    pub sources: Vec<ReadOnlySource>,
}

impl ReadOnlySources {
    /// The sources of `target` (labelled `target_file`) and, when one exists,
    /// the gateway-config `manifest` (labelled with its path).
    pub fn new(target_file: &str, target: &Target, manifest: Option<(&str, &Manifest)>) -> Self {
        let mut sources = vec![
            ReadOnlySource {
                label: "the built-in default".to_string(),
                names: super::exclude::DEFAULT_EXCLUDED_TAG_PROVIDERS
                    .iter()
                    .map(|p| (*p).to_string())
                    .collect(),
            },
            ReadOnlySource {
                label: format!("exclude_tag_providers of {target_file}"),
                names: target.exclude_tag_providers.clone(),
            },
            ReadOnlySource {
                label: format!("read_only_tag_providers of {target_file}"),
                names: target.read_only_tag_providers.clone(),
            },
        ];
        if let Some((rel, manifest)) = manifest {
            sources.push(ReadOnlySource {
                label: format!("exclude_tag_providers of {rel}"),
                names: manifest.exclude_tag_providers.clone(),
            });
            sources.push(ReadOnlySource {
                label: format!("read_only_tag_providers of {rel}"),
                names: manifest.read_only_tag_providers.clone(),
            });
        }
        Self {
            gateway: target.gateway.clone(),
            sources,
        }
    }

    /// The label of the first source holding `provider` under ASCII case
    /// folding, or `None`.
    fn holder(&self, provider: &str) -> Option<&str> {
        self.sources
            .iter()
            .find(|s| s.names.iter().any(|n| n.eq_ignore_ascii_case(provider)))
            .map(|s| s.label.as_str())
    }
}

/// Whether `file` may own every provider it lists (D-04, D-05, D-06). Pure.
///
/// Each listed provider that a read-only source holds (`System`, either
/// `exclude_tag_providers`, either `read_only_tag_providers`, under ASCII
/// case folding) is a finding naming the source; all of them are one
/// `deploy_tree_invalid` on [`PROJECT_FILE_REL`]. Without a manifest the
/// check passes and the returned warning names the manifest path it did not
/// find. A project that owns no provider passes silently.
pub fn check_tag_ownership(
    file: &ProjectFile,
    read_only: &ReadOnlySources,
    manifest: Option<(&str, &Manifest)>,
) -> Result<Vec<String>, CoreError> {
    if file.tag_providers.is_empty() {
        return Ok(Vec::new());
    }
    let findings: Vec<DeployFinding> = file
        .tag_providers
        .iter()
        .enumerate()
        .filter_map(|(i, provider)| {
            read_only.holder(provider).map(|label| DeployFinding {
                file: PROJECT_FILE_REL.to_string(),
                line: None,
                column: None,
                message: format!(
                    "tag_providers[{i}]: {provider} is read-only to ign ({label}) and cannot be owned by a project"
                ),
            })
        })
        .collect();
    if !findings.is_empty() {
        return Err(CoreError::deploy_findings(findings));
    }
    let mut warnings = Vec::new();
    if manifest.is_none() {
        warnings.push(format!(
            "no gateway-config manifest at {} below the targets root: tag provider ownership was checked against the target file only",
            gateway_manifest_rel(&read_only.gateway)
        ));
    }
    Ok(warnings)
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
    // A present `parent` that is neither a string nor null is refused: reading
    // it as "no parent" would skip the K6 parent checks while the artifact
    // still ships the malformed descriptor verbatim.
    let parent = match object.get("parent") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(s)) if s.is_empty() => None,
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(_) => {
            return Err(CoreError::deploy_tree_invalid(
                PROJECT_JSON_REL,
                None,
                None,
                "`parent` must be a string (or null)",
            ));
        }
    };
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

/// Why a parent cannot be used (K6 a, D-05, DPRJ-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParentReason {
    /// The gateway has no such project (or an ancestor in its chain is
    /// missing, which the gateway itself flags `invalidParent`).
    Missing,
    /// The project is its own ancestor, or the chain never ends.
    Cycle,
    /// The parent exists but `inheritable` is false.
    NotInheritable,
}

impl ParentReason {
    /// The word the error message, the hint and the JSON details use.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Cycle => "cycle",
            Self::NotInheritable => "not_inheritable",
        }
    }
}

/// One gateway project as the parent check needs it, read from
/// `projects/list` (K6 a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParentLink {
    /// The project's name.
    pub name: String,
    /// Its parent, if any (an empty name counts as none).
    pub parent: Option<String>,
    /// Whether it may serve as a parent; `None` when the gateway did not say.
    pub inheritable: Option<bool>,
    /// The gateway's own verdict on this project's parent link
    /// (`invalidParent`); `None` when the gateway did not say.
    pub invalid_parent: Option<bool>,
}

/// Follow `parent` through `all` (every project the gateway lists) and return
/// the chain as found, nearest first, plus whether the walk hit its bound.
///
/// The walk takes at most `all.len() + 1` steps: an acyclic chain cannot
/// visit more projects than exist, so a longer walk is a cycle (T-25-31). It
/// stops early at a name the gateway does not list.
pub fn parent_chain(parent: &str, all: &[ParentLink]) -> (Vec<ParentLink>, bool) {
    let bound = all.len() + 1;
    let mut chain = Vec::new();
    let mut next = Some(parent.to_string());
    let mut steps = 0;
    while let Some(name) = next.take() {
        if steps >= bound {
            return (chain, true);
        }
        steps += 1;
        let Some(link) = all.iter().find(|l| l.name == name) else {
            break;
        };
        next = link.parent.clone().filter(|p| !p.is_empty());
        chain.push(link.clone());
    }
    (chain, false)
}

/// Decide whether `parent` may serve as `project`'s parent (K6 a). `chain` is
/// [`parent_chain`]'s answer: `chain[0]` is the parent as found, and an empty
/// chain means the gateway lists no such project. Pure: no request is made.
pub fn check_parent(
    project: &str,
    parent: &str,
    chain: &[ParentLink],
    bound_hit: bool,
) -> Result<(), ParentReason> {
    if parent == project
        || bound_hit
        || chain
            .iter()
            .any(|link| link.name == project || link.parent.as_deref() == Some(project))
    {
        return Err(ParentReason::Cycle);
    }
    let Some(first) = chain.first() else {
        return Err(ParentReason::Missing);
    };
    if first.inheritable == Some(false) {
        return Err(ParentReason::NotInheritable);
    }
    // The walk ended at a link that names a parent the gateway does not list.
    if chain
        .last()
        .is_some_and(|last| last.parent.as_deref().is_some_and(|p| !p.is_empty()))
    {
        return Err(ParentReason::Missing);
    }
    Ok(())
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
    fn tag_providers_parse_and_default_to_none() {
        let file = ProjectFile::parse(PROJECT_FILE_REL, GOOD.as_bytes()).expect("valid");
        assert!(file.tag_providers.is_empty(), "absent means none");
        let bytes =
            "schema_version: 1\nproject: A\nenvironments: [dev]\ntag_providers: [PlantA, Site B]\n";
        let file = ProjectFile::parse(PROJECT_FILE_REL, bytes.as_bytes()).expect("valid");
        assert_eq!(file.tag_providers, ["PlantA", "Site B"]);
    }

    #[test]
    fn tag_provider_defects_are_findings_of_one_error() {
        let bytes = "schema_version: 1\nproject: A\nenvironments: [dev]\n\
                     tag_providers: [PlantA, planta, '../x', 'CON', '']\n";
        let err = ProjectFile::parse(PROJECT_FILE_REL, bytes.as_bytes()).expect_err("bad");
        assert_eq!(err.code(), "deploy_tree_invalid");
        let text = err.to_string();
        for needle in [
            "tag_providers[1]: duplicate tag provider",
            "tag_providers[2]",
            "tag_providers[3]",
            "tag_providers[4]",
        ] {
            assert!(text.contains(needle), "{needle}: {text}");
        }
        assert!(!text.contains("tag_providers[0]"), "{text}");
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
            br#"{"title":"T","parent":"ParentProject"}"#,
        )
        .expect("write");
        assert_eq!(
            read_project_meta(dir.path()).expect("meta").parent,
            Some("ParentProject".to_string())
        );
        std::fs::write(dir.path().join("project.json"), br#"{"parent":""}"#).expect("write");
        assert_eq!(read_project_meta(dir.path()).expect("meta").parent, None);
        std::fs::write(dir.path().join("project.json"), b"[]").expect("write");
        assert!(read_project_meta(dir.path()).is_err());
    }

    #[test]
    fn project_meta_refuses_a_parent_that_is_not_a_string() {
        let dir = tempfile::tempdir().expect("tempdir");
        for body in [
            &br#"{"parent":5}"#[..],
            br#"{"parent":[]}"#,
            br#"{"parent":{}}"#,
            br#"{"parent":true}"#,
            br#"{"parent":["ParentProject"]}"#,
        ] {
            std::fs::write(dir.path().join("project.json"), body).expect("write");
            let err = read_project_meta(dir.path()).expect_err("a non-string parent");
            assert_eq!(err.code(), "deploy_tree_invalid", "{err}");
            assert!(err.to_string().contains("project.json"), "{err}");
            assert!(
                err.to_string().contains("`parent` must be a string"),
                "{err}"
            );
        }
        // Absent and null stay "no parent".
        for body in [&br#"{"title":"T"}"#[..], br#"{"parent":null}"#] {
            std::fs::write(dir.path().join("project.json"), body).expect("write");
            assert_eq!(read_project_meta(dir.path()).expect("meta").parent, None);
        }
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

    fn link(name: &str, parent: Option<&str>, inheritable: Option<bool>) -> ParentLink {
        ParentLink {
            name: name.into(),
            parent: parent.map(str::to_string),
            inheritable,
            invalid_parent: None,
        }
    }

    fn reason(project: &str, parent: &str, all: &[ParentLink]) -> Result<(), ParentReason> {
        let (chain, bound_hit) = parent_chain(parent, all);
        check_parent(project, parent, &chain, bound_hit)
    }

    #[test]
    fn check_parent_reasons() {
        let all = [
            link("Root", None, Some(true)),
            link("Mid", Some("Root"), Some(true)),
            link("Closed", None, Some(false)),
            link("A", Some("B"), Some(true)),
            link("B", Some("A"), Some(true)),
            link("Orphan", Some("Gone"), Some(true)),
            link("CapApp", Some("Root"), Some(true)),
            link("Loop", Some("CapApp"), Some(true)),
        ];
        // No parent chain problem.
        assert_eq!(reason("CapApp", "Mid", &all), Ok(()));
        assert_eq!(reason("CapApp", "Root", &all), Ok(()));
        // Missing.
        assert_eq!(reason("CapApp", "Nope", &all), Err(ParentReason::Missing));
        // Self, whether or not the gateway lists the project.
        assert_eq!(reason("CapApp", "CapApp", &all), Err(ParentReason::Cycle));
        assert_eq!(reason("New", "New", &all), Err(ParentReason::Cycle));
        // The parent's chain reaches the project.
        assert_eq!(reason("CapApp", "Loop", &all), Err(ParentReason::Cycle));
        // A cycle among other projects never ends.
        assert_eq!(reason("CapApp", "A", &all), Err(ParentReason::Cycle));
        // Not inheritable.
        assert_eq!(
            reason("CapApp", "Closed", &all),
            Err(ParentReason::NotInheritable)
        );
        // An ancestor the gateway does not list is a missing parent chain.
        assert_eq!(reason("CapApp", "Orphan", &all), Err(ParentReason::Missing));
        // Unknown inheritable flag is not a refusal.
        let unknown = [link("P", None, None)];
        assert_eq!(reason("CapApp", "P", &unknown), Ok(()));
    }

    #[test]
    fn parent_chain_is_bounded_by_the_listed_projects() {
        // Three projects that parent each other in a ring.
        let all = [
            link("X", Some("Y"), Some(true)),
            link("Y", Some("Z"), Some(true)),
            link("Z", Some("X"), Some(true)),
        ];
        let (chain, bound_hit) = parent_chain("X", &all);
        assert!(bound_hit);
        assert!(chain.len() <= all.len() + 1);
        // A legitimate chain as long as the list never trips the bound.
        let line = [
            link("P3", None, Some(true)),
            link("P2", Some("P3"), Some(true)),
            link("P1", Some("P2"), Some(true)),
        ];
        let (chain, bound_hit) = parent_chain("P1", &line);
        assert!(!bound_hit);
        assert_eq!(chain.len(), 3);
    }

    #[test]
    fn parent_reason_words_are_stable() {
        assert_eq!(ParentReason::Missing.as_str(), "missing");
        assert_eq!(ParentReason::Cycle.as_str(), "cycle");
        assert_eq!(ParentReason::NotInheritable.as_str(), "not_inheritable");
        assert_eq!(
            serde_json::to_string(&ParentReason::NotInheritable).expect("json"),
            "\"not_inheritable\""
        );
    }

    // -- 26-05: provider settings and tag ownership ------------------------

    /// A repo holding `deploy/tag-providers/PlantA.json` = `json`.
    fn settings_repo(json: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let at = dir.path().join(TAG_PROVIDERS_DIR_REL);
        std::fs::create_dir_all(&at).expect("mkdir");
        std::fs::write(at.join("PlantA.json"), json).expect("write");
        dir
    }

    const SETTINGS_REL: &str = "deploy/tag-providers/PlantA.json";

    #[test]
    fn provider_settings_dir_is_beside_the_project_file() {
        assert_eq!(TAG_PROVIDERS_DIR_REL, "deploy/tag-providers");
        assert_eq!(
            gateway_manifest_rel("capgw"),
            "gateways/capgw/manifest.yaml"
        );
    }

    #[test]
    fn provider_settings_load_strictly() {
        let repo = settings_repo(
            r#"{"schema_version": 1, "enabled": true, "description": "Plant A", "config": {"profile": {"type": "STANDARD"}}}"#,
        );
        let settings = load_provider_settings(repo.path(), "PlantA").expect("valid");
        assert!(settings.enabled);
        assert_eq!(settings.description.as_deref(), Some("Plant A"));
        assert_eq!(settings.config["profile"]["type"], "STANDARD");

        let repo = settings_repo(r#"{"schema_version": 1, "enabled": false, "config": {}}"#);
        let settings = load_provider_settings(repo.path(), "PlantA").expect("no description");
        assert!(!settings.enabled);
        assert_eq!(settings.description, None);

        for bad in [
            r#"{"schema_version": 1, "enabled": true, "config": {}, "extra": 1}"#,
            r#"{"schema_version": 2, "enabled": true, "config": {}}"#,
            r#"{"schema_version": 1, "enabled": true, "config": []}"#,
            r#"{"schema_version": 1, "enabled": true, "config": "x"}"#,
            r#"{"schema_version": 1, "enabled": true}"#,
            r#"{"schema_version": 1, "enabled": true, "config": {"a": 1, "a": 2}}"#,
            r#"{"schema_version": 1, "enabled": true, "config": {}"#,
            r#"[]"#,
        ] {
            let repo = settings_repo(bad);
            let err = load_provider_settings(repo.path(), "PlantA").expect_err(bad);
            assert_eq!(err.code(), "deploy_tree_invalid", "{bad}: {err}");
            assert!(err.to_string().contains(SETTINGS_REL), "{bad}: {err}");
        }

        let repo = tempfile::tempdir().expect("tempdir");
        let err = load_provider_settings(repo.path(), "PlantA").expect_err("missing");
        assert_eq!(err.code(), "deploy_tree_invalid");
        let text = err.to_string();
        assert!(text.contains(SETTINGS_REL), "{text}");
        assert!(text.contains("does not exist"), "{text}");
    }

    /// D-14: ign would write the settings, so an Embedded secret anywhere in
    /// them refuses `embedded_secret`, naming the file and the JSON path and
    /// never the value.
    #[test]
    fn provider_settings_embedded_secret_is_refused() {
        let repo = settings_repo(
            r#"{"schema_version": 1, "enabled": true, "config": {"auth": {"password": {"type": "Embedded", "data": {"ciphertext": "SENTINEL-CIPHER"}}}}}"#,
        );
        let err = load_provider_settings(repo.path(), "PlantA").expect_err("embedded");
        assert_eq!(err.code(), "embedded_secret");
        let text = err.to_string();
        assert!(text.contains(SETTINGS_REL), "{text}");
        assert!(text.contains("config.auth.password"), "{text}");
        assert!(!text.contains("SENTINEL-CIPHER"), "never a value: {text}");
    }

    fn owning(providers: &str) -> ProjectFile {
        let yaml = format!(
            "schema_version: 1\nproject: CapApp\nenvironments: [dev]\ntag_providers: {providers}\n"
        );
        ProjectFile::parse(PROJECT_FILE_REL, yaml.as_bytes()).expect("project file")
    }

    fn target_with(extra: &str) -> Target {
        let yaml = format!(
            "schema_version: 1\ngateway: capgw\nmode: dev\nnodes:\n  master:\n    profile: dev\n{extra}"
        );
        Target::parse("targets/dev.yaml", yaml.as_bytes()).expect("target")
    }

    fn manifest_with(extra: &str) -> Manifest {
        let yaml = format!("schema_version: 1\ntypes: []\n{extra}");
        Manifest::parse("gateways/capgw/manifest.yaml", yaml.as_bytes()).expect("manifest")
    }

    const MANIFEST_REL: &str = "gateways/capgw/manifest.yaml";

    /// The one finding message of a `deploy_tree_invalid` on the project file.
    fn refusal(err: CoreError) -> Vec<String> {
        assert_eq!(err.code(), "deploy_tree_invalid", "{err}");
        let CoreError::DeployTreeInvalid(details) = err else {
            unreachable!()
        };
        details
            .findings
            .iter()
            .map(|f| {
                assert_eq!(f.file, PROJECT_FILE_REL);
                f.message.clone()
            })
            .collect()
    }

    #[test]
    fn ownership_refuses_system_and_excluded_providers() {
        let target = target_with("exclude_tag_providers: [\"MQTT Engine\"]\n");
        let manifest = manifest_with("exclude_tag_providers: [Sparkplug]\n");
        let sources =
            ReadOnlySources::new("targets/dev.yaml", &target, Some((MANIFEST_REL, &manifest)));
        let err = check_tag_ownership(
            &owning("[system, PlantA, \"mqtt engine\", SPARKPLUG]"),
            &sources,
            Some((MANIFEST_REL, &manifest)),
        )
        .expect_err("three are read-only");
        let messages = refusal(err);
        assert_eq!(messages.len(), 3, "{messages:?}");
        assert!(
            messages[0].starts_with("tag_providers[0]") && messages[0].contains("built-in"),
            "{messages:?}"
        );
        assert!(
            messages[1].starts_with("tag_providers[2]")
                && messages[1].contains("exclude_tag_providers of targets/dev.yaml"),
            "{messages:?}"
        );
        assert!(
            messages[2].starts_with("tag_providers[3]")
                && messages[2].contains(&format!("exclude_tag_providers of {MANIFEST_REL}")),
            "{messages:?}"
        );
    }

    /// D-06: a module-managed shape (`Broker Transmission`) and an
    /// edge-gateway-managed shape (`EdgeGateway01`) are refused from the
    /// target's and from the manifest's `read_only_tag_providers`, in any
    /// letter case.
    #[test]
    fn ownership_refuses_read_only_providers_from_target_and_manifest() {
        for name in ["Broker Transmission", "EdgeGateway01"] {
            for listed in [name.to_ascii_lowercase(), name.to_ascii_uppercase()] {
                let list = format!("read_only_tag_providers: [\"{name}\"]\n");
                let project = owning(&format!("[PlantA, \"{listed}\"]"));

                let target = target_with(&list);
                let manifest = manifest_with("");
                let sources = ReadOnlySources::new(
                    "targets/dev.yaml",
                    &target,
                    Some((MANIFEST_REL, &manifest)),
                );
                let messages = refusal(
                    check_tag_ownership(&project, &sources, Some((MANIFEST_REL, &manifest)))
                        .expect_err("target read-only"),
                );
                assert_eq!(messages.len(), 1, "{messages:?}");
                assert!(
                    messages[0].starts_with("tag_providers[1]")
                        && messages[0].contains("read_only_tag_providers of targets/dev.yaml"),
                    "{messages:?}"
                );

                let target = target_with("");
                let manifest = manifest_with(&list);
                let sources = ReadOnlySources::new(
                    "targets/dev.yaml",
                    &target,
                    Some((MANIFEST_REL, &manifest)),
                );
                let messages = refusal(
                    check_tag_ownership(&project, &sources, Some((MANIFEST_REL, &manifest)))
                        .expect_err("manifest read-only"),
                );
                assert_eq!(messages.len(), 1, "{messages:?}");
                assert!(
                    messages[0].contains(&format!("read_only_tag_providers of {MANIFEST_REL}")),
                    "{messages:?}"
                );
            }
        }
    }

    #[test]
    fn ownership_without_a_manifest_passes_with_a_warning() {
        let target = target_with("");
        let sources = ReadOnlySources::new("targets/dev.yaml", &target, None);
        let warnings = check_tag_ownership(&owning("[PlantA]"), &sources, None).expect("passes");
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains(MANIFEST_REL), "{warnings:?}");

        let manifest = manifest_with("tag_providers: [Other]\n");
        let sources =
            ReadOnlySources::new("targets/dev.yaml", &target, Some((MANIFEST_REL, &manifest)));
        let warnings = check_tag_ownership(
            &owning("[PlantA]"),
            &sources,
            Some((MANIFEST_REL, &manifest)),
        )
        .expect("not claimed");
        assert!(warnings.is_empty(), "{warnings:?}");

        let project = ProjectFile::parse(PROJECT_FILE_REL, GOOD.as_bytes()).expect("no owned");
        let sources = ReadOnlySources::new("targets/dev.yaml", &target, None);
        assert!(
            check_tag_ownership(&project, &sources, None)
                .expect("owns nothing")
                .is_empty()
        );
    }
}
