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
//! unmanaged, unmanaged_count, warnings, outcome, ok, slug`. Later plans add keys additively. Lists are
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
use crate::deploy::project_artifact::{
    Artifact, CanonicalDigest, build_artifact, canonical_digest,
};
use crate::deploy::projects::{
    MemberDelta, ProjectAction, ProjectFile, ProjectMeta, load_project_file, member_delta,
    read_project_meta,
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
}

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
}

/// Load and build everything that needs no network (D-01, D-07, D-08, D-09).
pub fn load_project(input: &ProjectsInput) -> Result<LoadedProject, CoreError> {
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

    let git = surfaces.host.git_entry(project).await?;
    if git.is_present() {
        return Err(CoreError::ProjectGitManaged {
            project: project.clone(),
            location: git_entry_location(project),
            kind: git.as_str().to_string(),
        });
    }

    let records = gateway.projects().await?;
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
    let action = match &live {
        None => ProjectAction::Create,
        Some(live)
            if !live.members.is_empty()
                && !artifact.members.is_empty()
                && live.sha256 == artifact.sha256 =>
        {
            ProjectAction::Skip
        }
        Some(_) => ProjectAction::Import,
    };
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
        warnings: Vec::new(),
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
