//! `ign deploy projects` (Phase 25): deploy one Ignition project from its own
//! repo to a gateway. This slice is the read-only half, `--check` (DPRJ-05).
//!
//! [`load_project`] is the offline front half: it reads `deploy/project.yaml`,
//! refuses an environment the file does not list, reads
//! `targets/<ENV>.yaml` below the targets root (D-07, D-14) and builds the
//! artifact from the checkout (D-01, D-09). [`deploy_projects_prepare`] then
//! reads the live project through a [`ProjectGateway`], compares canonical
//! digests and reports what a deploy would do: `create` for a project absent
//! from the gateway (D-13), `skip` when the live project equals the artifact,
//! `import` otherwise.
//!
//! ## Release order (25-09)
//!
//! `--release` is a semver tag (D-02, V4), parsed before any host or gateway
//! contact. The host's release record (V5) names the release the gateway last
//! received from this tool; an older `--release` refuses
//! `destructive_change_refused` gate `downgrade` unless `--allow-downgrade`
//! (D-03). A record that cannot be trusted is `deploy_run_invalid`, never
//! absent.
//!
//! ## The first deploy and the record (25-09)
//!
//! A project the git module (or anyone) left on the gateway has no release
//! record. The first `ign` deploy of it must prove the release equals what the
//! gateway runs: a live project that differs from the artifact refuses
//! `destructive_change_refused` gate `first_deploy` unless
//! `--allow-first-change <REASON>` (D-12). A live project that equals the
//! artifact plans `record` (only the record is written, K14), or `skip` when
//! the record already names this release. A live project that no longer
//! matches the artifact digest the record holds is a warning (K13).
//!
//! ## Reads only
//!
//! [`ProjectGateway`] has exactly three methods and all three are reads, and
//! [`ProjectHost`](crate::deploy::transport::projects::ProjectHost) is read-only
//! too (`git_entry`, `read_release_record`). No
//! method deletes, renames, copies, modifies, creates or imports a project,
//! so nothing in this module can touch a project other than to read it
//! (DPRJ-04, D-05). Projects other than the named one are listed by name in
//! `unmanaged` and nothing else.
//!
//! The [`DeployProjectsResult`] `data` keys, in order: `env, gateway,
//! project, release, check, action, would_change, artifact_sha256,
//! live_sha256, members, removed, removed_count, excluded, excluded_count,
//! unmanaged, unmanaged_count, parent, previous_release, recorded_sha256,
//! live_matches_last_deploy, allow_downgrade, allow_first_change, warnings,
//! outcome, ok, slug`. Later plans add keys additively. Lists are
//! byte-sorted and capped at [`LIST_CAP`] entries with the full count beside
//! them. Output carries names, hashes and counts only.

use std::path::PathBuf;

use async_trait::async_trait;
use serde::Serialize;

use super::deploy::load_target;
use super::projects::validate_import;
use super::resources::export_zip_bytes;
use crate::client::GatewayApi;
use crate::client::projects::ProjectRecord;
use crate::client::query::ListQuery;
use crate::deploy::ledger::{LedgerRoot, release_record_location};
use crate::deploy::project_artifact::{
    Artifact, CanonicalDigest, build_artifact, canonical_digest,
};
use crate::deploy::projects::{
    MemberDelta, ParentLink, ProjectAction, ProjectFile, ProjectMeta, Release, ReleaseOrder,
    ReleaseRecord, check_parent, load_project_file, member_delta, parent_chain, read_project_meta,
};
use crate::deploy::target::Target;
use crate::deploy::transport::projects::{ProjectHost, git_entry_location};
use crate::error::CoreError;

/// Most entries a result list carries; the full count rides beside it.
pub const LIST_CAP: usize = 200;

/// The gateway reads a project deploy needs, and nothing else.
///
/// There is deliberately no method that deletes, renames, copies, modifies or
/// creates a project: the read-only check cannot write by construction.
#[async_trait]
pub trait ProjectGateway: Send + Sync {
    /// Every project the gateway lists (`limit=-1`).
    async fn projects(&self) -> Result<Vec<ProjectRecord>, CoreError>;
    /// One project by name; `None` when the gateway has no such project.
    async fn project_find(&self, name: &str) -> Result<Option<ProjectRecord>, CoreError>;
    /// The project's export zip, in memory.
    async fn export_bytes(&self, name: &str) -> Result<Vec<u8>, CoreError>;
}

/// [`ProjectGateway`] over the real gateway client.
pub struct GatewayProjects<'a>(pub &'a dyn GatewayApi);

#[async_trait]
impl ProjectGateway for GatewayProjects<'_> {
    async fn projects(&self) -> Result<Vec<ProjectRecord>, CoreError> {
        Ok(self.0.projects(&ListQuery::default()).await?.items)
    }

    async fn project_find(&self, name: &str) -> Result<Option<ProjectRecord>, CoreError> {
        match self.0.project_find(name).await {
            Ok(record) => Ok(Some(record)),
            Err(CoreError::NotFound { .. }) => Ok(None),
            Err(err) => Err(err),
        }
    }

    async fn export_bytes(&self, name: &str) -> Result<Vec<u8>, CoreError> {
        export_zip_bytes(self.0, name).await
    }
}

/// The two read-only surfaces a project deploy talks to: the gateway REST API
/// and the gateway host's filesystem (the `.git` probe, the release record).
/// Neither trait has a write method in this plan.
pub struct ProjectsSurfaces<'a> {
    /// The gateway project reads.
    pub gateway: &'a dyn ProjectGateway,
    /// The host reads.
    pub host: &'a dyn ProjectHost,
}

/// What the caller supplies.
#[derive(Debug, Clone)]
pub struct ProjectsInput {
    /// The project repo root.
    pub repo: PathBuf,
    /// The root holding `targets/<ENV>.yaml` (default: `repo`).
    pub targets: PathBuf,
    /// The environment name.
    pub env: String,
    /// The release tag being deployed, as given.
    pub release: String,
    /// `--allow-downgrade`: let an older release than the recorded one
    /// replace it (D-03).
    pub allow_downgrade: bool,
    /// `--allow-first-change <REASON>`: let the first `ign` deploy of a
    /// project change what the gateway runs (D-12). 1 to 500 characters.
    pub allow_first_change: Option<String>,
}

/// The longest `--allow-first-change` reason accepted.
pub const FIRST_CHANGE_REASON_MAX: usize = 500;

/// Everything read offline.
#[derive(Debug)]
pub struct LoadedProject {
    /// `deploy/project.yaml`.
    pub file: ProjectFile,
    /// What the root `project.json` says (the parent).
    pub meta: ProjectMeta,
    /// The target file's name below the targets root.
    pub target_file: String,
    /// The parsed target.
    pub target: Target,
    /// The artifact built from the checkout.
    pub artifact: Artifact,
    /// `--release`, parsed.
    pub release: Release,
}

/// Load and build everything that needs no network (D-01, D-07, D-08, D-09).
pub fn load_project(input: &ProjectsInput) -> Result<LoadedProject, CoreError> {
    // The flags are checked first: nothing is read, let alone contacted, for
    // an input that cannot be valid (V4, V3).
    let release = Release::parse(&input.release)?;
    if let Some(reason) = &input.allow_first_change {
        let chars = reason.chars().count();
        if chars == 0 || chars > FIRST_CHANGE_REASON_MAX {
            return Err(CoreError::InvalidInput {
                reason: format!(
                    "--allow-first-change <reason>: the reason is 1 to {FIRST_CHANGE_REASON_MAX} characters"
                ),
            });
        }
    }
    let file = load_project_file(&input.repo)?;
    file.require_env(&input.env)?;
    let meta = read_project_meta(&input.repo)?;
    let (target_file, target) = load_target(&input.targets, &input.env)?;
    let artifact = build_artifact(&input.repo)?;
    check_artifact_zip(&artifact.zip)?;
    Ok(LoadedProject {
        file,
        meta,
        target_file,
        target,
        artifact,
        release,
    })
}

/// The built artifact must pass the same zip checks as `projects import`
/// before it is reported or uploaded: ZIP magic, the size limit, and every
/// entry decompressed to its end (a truncated zip imports "successfully" and,
/// on overwrite, empties the project; C8b). `invalid_import_file`, exit 2.
pub(crate) fn check_artifact_zip(zip: &[u8]) -> Result<(), CoreError> {
    validate_import(zip)
}

/// How a project deploy ended. `checked` is the only word this slice emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectOutcome {
    /// `--check`: nothing was written.
    Checked,
}

/// `ign deploy projects` output model.
#[derive(Debug, Clone, Serialize)]
pub struct DeployProjectsResult {
    /// The `--env` value.
    pub env: String,
    /// The target's gateway name.
    pub gateway: String,
    /// The gateway project name from `deploy/project.yaml`.
    pub project: String,
    /// The release tag, as given.
    pub release: String,
    /// True for `--check`: no write was made.
    pub check: bool,
    /// What a deploy would do.
    pub action: ProjectAction,
    /// Whether the gateway would change.
    pub would_change: bool,
    /// Canonical sha256 of the artifact built from the checkout.
    pub artifact_sha256: String,
    /// Canonical sha256 of the live project's export; null when absent.
    pub live_sha256: Option<String>,
    /// How the artifact's members differ from the live project's.
    pub members: MemberDelta,
    /// Live members an overwrite would delete, byte-sorted, capped.
    pub removed: Vec<String>,
    /// Full count of removed members.
    pub removed_count: usize,
    /// Repo-relative paths left out of the artifact (root entries outside
    /// the allowlist, the built-in denylist, `.ignexclude`, files no
    /// `resource.json` lists), byte-sorted, capped. A directory carries a
    /// trailing `/`. Paths only, never file content.
    pub excluded: Vec<String>,
    /// Full count of excluded paths.
    pub excluded_count: usize,
    /// Other projects on the gateway, by name, byte-sorted, capped. They are
    /// never read, changed or deleted.
    pub unmanaged: Vec<String>,
    /// Full count of other projects.
    pub unmanaged_count: usize,
    /// The parent named in the repo's `project.json`; null for a root
    /// project. Checked with reads only (K6 a, D-05).
    pub parent: Option<String>,
    /// The release the gateway last received from this tool, as recorded;
    /// null when there is no release record (K13, D-03).
    pub previous_release: Option<String>,
    /// The `artifact_sha256` the release record holds: the canonical digest
    /// of what this tool last deployed; null when there is no record.
    pub recorded_sha256: Option<String>,
    /// Whether the live project still equals `recorded_sha256`; null without
    /// a record or without a live project. False is a warning (K13).
    pub live_matches_last_deploy: Option<bool>,
    /// Whether `--allow-downgrade` was given.
    pub allow_downgrade: bool,
    /// The `--allow-first-change` reason, as given; null without the flag.
    pub allow_first_change: Option<String>,
    /// Warnings in pipeline order.
    pub warnings: Vec<String>,
    /// How the run ended.
    pub outcome: ProjectOutcome,
    /// False only for a verdict exit (never in a check).
    pub ok: bool,
    /// The verdict slug when `ok` is false; otherwise null.
    pub slug: Option<String>,
}

fn capped(mut items: Vec<String>) -> (Vec<String>, usize) {
    let count = items.len();
    items.truncate(LIST_CAP);
    (items, count)
}

/// The parent-relevant facts of one listed project (K6 a).
fn parent_link(record: &ProjectRecord) -> ParentLink {
    ParentLink {
        name: record.name.clone(),
        parent: record.parent.clone().filter(|p| !p.is_empty()),
        inheritable: record.inheritable,
        invalid_parent: record.invalid_parent,
    }
}

/// The live export must hold members. A gateway answering an export with
/// none breaks its own contract, which is not a defect in the repo.
fn live_digest(project: &str, zip: &[u8]) -> Result<CanonicalDigest, CoreError> {
    canonical_digest(zip).map_err(|err| match err {
        CoreError::DeployTreeInvalid(_) => CoreError::Internal(format!(
            "the gateway's export of project {project} holds no members"
        )),
        other => other,
    })
}

/// The read-only comparison (`--check`): probe the host for a `.git`, then
/// list, find, export and digest the live project and decide the action.
/// Sends GET requests only and writes nothing.
///
/// The `.git` probe comes first (DPRJ-04, D-11): a project the git module
/// still manages is refused as `project_git_managed` before any gateway read.
/// An unreadable probe is `transport_failed`, never "no git".
pub async fn deploy_projects_prepare(
    loaded: &LoadedProject,
    input: &ProjectsInput,
    surfaces: &ProjectsSurfaces<'_>,
) -> Result<DeployProjectsResult, CoreError> {
    let project = &loaded.file.project;
    let gateway = surfaces.gateway;
    let gateway_name = &loaded.target.gateway;

    let git = surfaces.host.git_entry(project).await?;
    if git.is_present() {
        return Err(CoreError::ProjectGitManaged {
            project: project.clone(),
            location: git_entry_location(project),
            kind: git.as_str().to_string(),
        });
    }

    // The release record (V5): absent means this tool has not deployed the
    // project here; a record that cannot be trusted refuses and is never read
    // as absent (T-25-28).
    let record = match surfaces
        .host
        .read_release_record(gateway_name, project)
        .await?
    {
        None => None,
        Some(bytes) => {
            let location = release_record_location(
                LedgerRoot::for_node(&loaded.target.nodes.master),
                gateway_name,
                project,
            );
            Some(ReleaseRecord::parse(
                &bytes,
                &location,
                gateway_name,
                project,
            )?)
        }
    };
    if let Some(record) = &record
        && loaded.release.order_against(&record.release()?) == ReleaseOrder::Older
        && !input.allow_downgrade
    {
        return Err(CoreError::DestructiveChangeRefused {
            gate: "downgrade".to_string(),
            keys: vec![project.clone()],
            count: 1,
        });
    }

    let records = gateway.projects().await?;
    // The parent check reads the one list and nothing else (K6 a, D-05): a
    // missing parent, a cycle or a parent that is not inheritable refuses
    // before the project itself is exported, and the parent is never touched.
    let mut warnings = Vec::new();
    if let Some(parent) = &loaded.meta.parent {
        let all: Vec<ParentLink> = records.iter().map(parent_link).collect();
        let (chain, bound_hit) = parent_chain(parent, &all);
        check_parent(project, parent, &chain, bound_hit).map_err(|reason| {
            CoreError::ProjectParentInvalid {
                project: project.clone(),
                parent: parent.clone(),
                reason: reason.as_str().to_string(),
            }
        })?;
        // The gateway's own flag on the parent: the three reasons above found
        // nothing, so the cause is the parent's own inheritance. Reported, not
        // refused (K6 a).
        if chain
            .first()
            .is_some_and(|p| p.invalid_parent == Some(true))
        {
            warnings.push(format!(
                "project {project}: the gateway reports that parent {parent} has an invalid parent of its own"
            ));
        }
    }
    // An import writes the repo's `parent` onto the live project, so a live
    // parent that differs is a reparent. Warned about, never refused: the
    // repo is the source of truth (D-05). Read from the list already held.
    let repo_parent = loaded.meta.parent.as_deref().filter(|p| !p.is_empty());
    if let Some(live_record) = records.iter().find(|r| r.name == *project) {
        let live_parent = live_record.parent.as_deref().filter(|p| !p.is_empty());
        if live_parent != repo_parent {
            warnings.push(format!(
                "project {project}: importing this project changes its parent from {} to {}",
                live_parent.unwrap_or("none"),
                repo_parent.unwrap_or("none")
            ));
        }
    }
    let mut others: Vec<String> = records
        .into_iter()
        .map(|r| r.name)
        .filter(|name| name != project)
        .collect();
    others.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    others.dedup();

    let live = match gateway.project_find(project).await? {
        None => None,
        Some(_) => {
            let zip = gateway.export_bytes(project).await?;
            Some(live_digest(project, &zip)?)
        }
    };

    let artifact = &loaded.artifact.digest;
    let equal = live.as_ref().is_some_and(|live| {
        !live.members.is_empty() && !artifact.members.is_empty() && live.sha256 == artifact.sha256
    });
    let action = match &live {
        None => ProjectAction::Create,
        // The live project already is the artifact: the project is left alone
        // and only the record may be written (K14, D-12).
        Some(_) if equal => match &record {
            Some(record) if record.release()?.version == loaded.release.version => {
                ProjectAction::Skip
            }
            _ => ProjectAction::Record,
        },
        Some(_) => ProjectAction::Import,
    };
    // The first `ign` deploy of a project must not change what the gateway
    // runs without a stated reason (D-12, V3).
    if action == ProjectAction::Import && record.is_none() && input.allow_first_change.is_none() {
        return Err(CoreError::DestructiveChangeRefused {
            gate: "first_deploy".to_string(),
            keys: vec![project.clone()],
            count: 1,
        });
    }
    let live_matches_last_deploy = match (&record, &live) {
        (Some(record), Some(live)) => Some(record.artifact_sha256 == live.sha256),
        _ => None,
    };
    if let (Some(record), Some(false)) = (&record, live_matches_last_deploy) {
        warnings.push(format!(
            "project {project}: the live project no longer matches the last release this tool deployed ({})",
            record.release
        ));
    }
    let (members, removed) = member_delta(artifact, live.as_ref());
    let (removed, removed_count) = capped(removed);
    let (excluded, excluded_count) = capped(loaded.artifact.excluded.clone());
    let (unmanaged, unmanaged_count) = capped(others);

    Ok(DeployProjectsResult {
        env: input.env.clone(),
        gateway: loaded.target.gateway.clone(),
        project: project.clone(),
        release: input.release.clone(),
        check: true,
        action,
        would_change: action.would_change(),
        artifact_sha256: artifact.sha256.clone(),
        live_sha256: live.map(|l| l.sha256),
        members,
        removed,
        removed_count,
        excluded,
        excluded_count,
        unmanaged,
        unmanaged_count,
        parent: loaded.meta.parent.clone(),
        previous_release: record.as_ref().map(|r| r.release.clone()),
        recorded_sha256: record.map(|r| r.artifact_sha256),
        live_matches_last_deploy,
        allow_downgrade: input.allow_downgrade,
        allow_first_change: input.allow_first_change.clone(),
        warnings,
        outcome: ProjectOutcome::Checked,
        ok: true,
        slug: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_zip() -> Vec<u8> {
        use std::io::Write as _;
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in [
            ("project.json", br#"{"title":"T"}"#.as_slice()),
            ("ignition/s/resource.json", br#"{"files":["a.py"]}"#),
            ("ignition/s/a.py", &[b'x'; 4096]),
        ] {
            writer.start_file(name, options).expect("start");
            writer.write_all(bytes).expect("write");
        }
        writer.finish().expect("finish").into_inner()
    }

    #[test]
    fn check_artifact_zip_refuses_truncated() {
        let whole = sample_zip();
        check_artifact_zip(&whole).expect("the whole zip passes");
        let half = &whole[..whole.len() / 2];
        let err = check_artifact_zip(half).expect_err("half a zip");
        assert_eq!(err.code(), "invalid_import_file");
        // Cut inside the last member's data, central directory gone too.
        let err = check_artifact_zip(&whole[..whole.len() - 40]).expect_err("tail cut");
        assert_eq!(err.code(), "invalid_import_file");
        let err = check_artifact_zip(b"not a zip").expect_err("no magic");
        assert_eq!(err.code(), "invalid_import_file");
    }

    #[test]
    fn a_built_artifact_passes_the_check() {
        let dir = tempfile::tempdir().expect("tempdir");
        for (rel, bytes) in [
            ("project.json", br#"{"title":"T"}"#.as_slice()),
            ("ignition/s/resource.json", br#"{"files":["a.py"]}"#),
            ("ignition/s/a.py", b"pass"),
        ] {
            let path = dir.path().join(rel);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            std::fs::write(path, bytes).expect("write");
        }
        let artifact = build_artifact(dir.path()).expect("artifact");
        check_artifact_zip(&artifact.zip).expect("a built artifact is a valid import");
    }
}
