//! `ign deploy projects` (Phase 25): deploy one Ignition project from its own
//! repo to a gateway. `--check` is the read-only half (DPRJ-05); the run
//! ([`deploy_projects_run`], 25-11) is the guarded write path.
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
//! ## The write path (25-11)
//!
//! [`deploy_projects_prepare`] is shared by `--check` and the apply, so the
//! `.git` probe, the release gates and the parent check run on both, before
//! any write. The artifact hash (`--artifact-sha256`, D-01) is compared in
//! [`load_project`], after the local build and before any host or gateway
//! contact. A confirmed apply then runs [`deploy_projects_run`]:
//!
//! 1. the release-record writer for the master node is built first, so a node
//!    that cannot take a record refuses before anything is imported;
//! 2. a fresh run directory (`.ign/deploy/<gateway>/<run-id>/`, git-ignore
//!    guard, 0700/0600) gets `run.json`, `plan.json` and `events.ndjson`;
//! 3. for an `import` the live project is exported into the run directory
//!    (`export/<project>.zip`, 0600) and its canonical digest must still be
//!    the planned one, else `plan_stale` and nothing is imported (K15);
//! 4. the project is imported (`overwrite=true`; a project absent from the
//!    gateway is created with `overwrite=false` and no pre-image, D-13);
//! 5. the project is exported again and compared by canonical digest with the
//!    artifact; equal writes the release record, different is the fail-safe
//!    outcome `imported_unverified` with no record, no second import and no
//!    further gateway call (K11);
//! 6. a `record` plan (the live project already is the artifact, K14) only
//!    writes the record.
//!
//! The import goes to the master node's gateway; the backup node is never
//! contacted (the redundancy sync carries the project to it). Every return
//! after the run directory exists writes `result.json` first.
//!
//! ## Children of a parent project (25-15)
//!
//! After an `imported` or `imported_unverified` outcome the run looks at the
//! projects whose `parent` is the deployed project (D-06, K7). It reads each
//! child's export every [`ProjectsRunOptions::children_poll`] until two
//! consecutive reads of every child agree or
//! [`ProjectsRunOptions::children_settle`] ([`CHILDREN_SETTLE_S`]) has passed,
//! then compares each child's canonical digest with the `artifact_sha256` of
//! that child's own release record: [`ChildStatus::Ok`] when equal,
//! [`ChildStatus::Mismatch`] (a warning) when not, [`ChildStatus::Unverified`]
//! when the child has no record or cannot be read. The sweep only reads
//! (`projects` and the children's `export_bytes` on the gateway,
//! `read_release_record` on the host), never changes the outcome or the exit
//! code and never rolls back. It reports hash drift only: what an open
//! Designer or Perspective session of a child sees after a parent overwrite
//! is not claimed (the S5 residual stays open).
//!
//! ## One write
//!
//! [`ProjectGateway`] has four reads and one write, [`ProjectGateway::project_import`],
//! and [`ProjectHost`](crate::deploy::transport::projects::ProjectHost) is
//! read-only (`git_entry`, `read_release_record`). No method deletes, renames,
//! copies or modifies a project, and the import only ever names the project
//! the repo declares, so nothing in this module can touch another project
//! (DPRJ-04, D-05). Projects other than the named one are listed by name in
//! `unmanaged` and nothing else.
//!
//! The [`DeployProjectsResult`] `data` keys, in order: `env, gateway,
//! project, release, check, action, would_change, artifact_sha256,
//! live_sha256, members, removed, removed_count, excluded, excluded_count,
//! unmanaged, unmanaged_count, parent, previous_release, recorded_sha256,
//! live_matches_last_deploy, allow_downgrade, allow_first_change, warnings,
//! run_id, run_dir, record_written, restore_command, children, outcome, ok,
//! slug`. Later plans add keys additively. Lists are
//! byte-sorted and capped at [`LIST_CAP`] entries with the full count beside
//! them. Output carries names, hashes and counts only.

use std::io::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::Serialize;

use super::deploy::load_target;
use super::projects::validate_import;
use super::resources::export_zip_bytes;
use super::workspace::rfc3339_now_utc;
use crate::client::GatewayApi;
use crate::client::projects::{ImportOutcome, ProjectRecord};
use crate::client::query::ListQuery;
use crate::deploy::apply::hold::SystemClock;
use crate::deploy::apply::preflight::{
    DesignerSeen, DesignersPolicy, DesignersVerdict, designers_detail, designers_gate,
    read_designers, wrong_target_failure, wrong_target_gate,
};
use crate::deploy::apply::run_store::{EventLog, RunDir, RunId, run_dir_rel};
use crate::deploy::ledger::{LedgerRoot, release_record_location};
use crate::deploy::project_artifact::{
    Artifact, CanonicalDigest, build_artifact, canonical_digest,
};
use crate::deploy::projects::{
    MemberDelta, ParentLink, ProjectAction, ProjectFile, ProjectMeta, Release, ReleaseOrder,
    ReleaseRecord, check_parent, load_project_file, member_delta, parent_chain, read_project_meta,
};
use crate::deploy::target::{NodeTarget, Target};
use crate::deploy::transport::for_node;
use crate::deploy::transport::projects::{
    ProjectHost, ReleaseRecordWriter, git_entry_location, release_writer_for_node,
};
use crate::error::{CoreError, PreflightFailure};

/// Most entries a result list carries; the full count rides beside it.
pub const LIST_CAP: usize = 200;

/// The longest the children sweep waits for the children's exports to settle
/// after a parent import, in seconds (K7, D-06). Capture C9 measured the
/// settle at 5 s on the 8.3.6 rig; 30 s is six times that.
pub const CHILDREN_SETTLE_S: u64 = 30;

/// How often the sweep reads the children while it waits, in seconds (K7).
pub const CHILDREN_POLL_S: u64 = 5;

/// The gateway calls a project deploy needs, and nothing else.
///
/// Four reads and one write. There is deliberately no method that deletes,
/// renames, copies or modifies a project: the only write is the import of the
/// project the repo declares, and the read-only check never reaches it
/// (`ForbidProjectWrites` proves that in the engine tests).
#[async_trait]
pub trait ProjectGateway: Send + Sync {
    /// Every project the gateway lists (`limit=-1`).
    async fn projects(&self) -> Result<Vec<ProjectRecord>, CoreError>;
    /// One project by name; `None` when the gateway has no such project.
    async fn project_find(&self, name: &str) -> Result<Option<ProjectRecord>, CoreError>;
    /// The project's export zip, in memory.
    async fn export_bytes(&self, name: &str) -> Result<Vec<u8>, CoreError>;
    /// The open Designer sessions as `(user, project, host)` (a read). The
    /// pre-check before an import (25-14, D-14, D-15).
    async fn designers(&self) -> Result<Vec<DesignerSeen>, CoreError>;
    /// The gateway's own name over REST (`gateway-info`), or `None` when it
    /// reports none or a blank one. Read before any write to prove the
    /// gateway is the one the target names (25-14, T-25-44).
    async fn gateway_name(&self) -> Result<Option<String>, CoreError>;
    /// Import `zip` as project `name`: `overwrite=true` replaces the whole
    /// project, `overwrite=false` creates it. The only write of the verb. A
    /// 200 answer carrying `success: false` is `import_denied`.
    async fn project_import(
        &self,
        name: &str,
        zip: Vec<u8>,
        overwrite: bool,
    ) -> Result<ImportOutcome, CoreError>;
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

    async fn designers(&self) -> Result<Vec<DesignerSeen>, CoreError> {
        read_designers(self.0).await
    }

    async fn gateway_name(&self) -> Result<Option<String>, CoreError> {
        Ok(self
            .0
            .gateway_info()
            .await?
            .name
            .filter(|name| !name.trim().is_empty()))
    }

    async fn project_import(
        &self,
        name: &str,
        zip: Vec<u8>,
        overwrite: bool,
    ) -> Result<ImportOutcome, CoreError> {
        self.0.project_import(name, zip, overwrite).await
    }
}

/// The two surfaces a project deploy talks to: the gateway REST API and the
/// gateway host's filesystem (the `.git` probe, the release record read). The
/// host surface is read-only; the record is written through the writer
/// [`deploy_projects_run`] builds for the master node.
pub struct ProjectsSurfaces<'a> {
    /// The gateway project calls.
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
    /// `--artifact-sha256`: the canonical digest of the artifact the reviewer
    /// saw (64 lowercase hex). Compared with the artifact the checkout builds
    /// before any host or gateway contact (D-01, V1); `None` is allowed here
    /// because the CLI (Decision C) decides when it is required.
    pub artifact_sha256: Option<String>,
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
    if let Some(hash) = &input.artifact_sha256
        && !is_lower_hex64(hash)
    {
        return Err(CoreError::InvalidInput {
            reason: "--artifact-sha256 must be 64 lowercase hex characters (the \
                     `artifact_sha256` of `ign deploy projects --check --json`)"
                .to_string(),
        });
    }
    let file = load_project_file(&input.repo)?;
    file.require_env(&input.env)?;
    let meta = read_project_meta(&input.repo)?;
    let (target_file, target) = load_target(&input.targets, &input.env)?;
    let artifact = build_artifact(&input.repo)?;
    check_artifact_zip(&artifact.zip)?;
    // The reviewed hash against the artifact this checkout builds: any
    // difference, including letter case (refused above), never passes, and
    // nothing has been uploaded or even contacted (SC-1, D-01).
    if let Some(expected) = &input.artifact_sha256
        && *expected != artifact.digest.sha256
    {
        return Err(CoreError::ProjectHashMismatch {
            project: file.project.clone(),
            expected: expected.clone(),
            actual: artifact.digest.sha256.clone(),
        });
    }
    Ok(LoadedProject {
        file,
        meta,
        target_file,
        target,
        artifact,
        release,
    })
}

/// 64 lowercase hex characters.
fn is_lower_hex64(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The built artifact must pass the same zip checks as `projects import`
/// before it is reported or uploaded: ZIP magic, the size limit, and every
/// entry decompressed to its end (a truncated zip imports "successfully" and,
/// on overwrite, empties the project; C8b). `invalid_import_file`, exit 2.
pub(crate) fn check_artifact_zip(zip: &[u8]) -> Result<(), CoreError> {
    validate_import(zip)
}

/// How a project deploy ended (V6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectOutcome {
    /// `--check`: nothing was written.
    Checked,
    /// The live project already was this release; nothing was written and no
    /// run directory was created.
    Skipped,
    /// The project was absent from the gateway and was created (D-13).
    Created,
    /// The live project was replaced and its canonical digest equals the
    /// artifact's.
    Imported,
    /// The live project already equalled the artifact; only the release record
    /// was written (K14).
    Recorded,
    /// The project was imported but the canonical digest read back does not
    /// equal the artifact's (K11). Fail-safe: no record, no second import.
    ImportedUnverified,
}

impl ProjectOutcome {
    /// The word the result and the events carry.
    pub fn as_str(self) -> &'static str {
        match self {
            ProjectOutcome::Checked => "checked",
            ProjectOutcome::Skipped => "skipped",
            ProjectOutcome::Created => "created",
            ProjectOutcome::Imported => "imported",
            ProjectOutcome::Recorded => "recorded",
            ProjectOutcome::ImportedUnverified => "imported_unverified",
        }
    }
}

/// The verdict slug of an unverified import (V6). It rides the success
/// payload as `data.slug` beside `data.ok: false` and the process then exits 6
/// (the SIXTH sanctioned success-path exit exception, Decision A). It is not a
/// [`CoreError`], is not in [`CoreError::DEPLOY_SLUGS`] and is not an
/// exit-table row.
pub const PROJECT_UNVERIFIED_SLUG: &str = "deploy_project_unverified";

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
    /// The run's id; null for a check and for a project left alone.
    pub run_id: Option<String>,
    /// The run directory, relative to the repo (`.ign/deploy/<gateway>/<id>`);
    /// null when no run directory was created.
    pub run_dir: Option<String>,
    /// Whether the release record was written on the gateway host.
    pub record_written: bool,
    /// The command that puts the saved pre-image back: set only on the
    /// unverified verdict of an overwrite import, where a pre-image exists
    /// (K11). Nothing runs it; the operator does.
    pub restore_command: Option<String>,
    /// The projects that inherit from this one, checked against the release
    /// record of their own last deploy (D-06, K7); byte-sorted by name. Empty
    /// when no sweep ran: a check, a project with no children, and the
    /// `created`, `recorded` and `skipped` outcomes. Reported, never acted on.
    pub children: Vec<ChildReport>,
    /// How the run ended.
    pub outcome: ProjectOutcome,
    /// False only for a verdict exit (never in a check).
    pub ok: bool,
    /// The verdict slug when `ok` is false; otherwise null.
    pub slug: Option<String>,
}

/// How one child compares with the release its own last deploy recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildStatus {
    /// The child's canonical digest equals its record's `artifact_sha256`.
    Ok,
    /// They differ. Also a warning naming the child.
    Mismatch,
    /// There is nothing to compare: the child has no release record, or its
    /// export or record could not be read.
    Unverified,
}

impl ChildStatus {
    /// The word the result and the events carry.
    pub fn as_str(self) -> &'static str {
        match self {
            ChildStatus::Ok => "ok",
            ChildStatus::Mismatch => "mismatch",
            ChildStatus::Unverified => "unverified",
        }
    }
}

/// One inheriting project in `data.children` (D-06). Names, a release tag and
/// hashes only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChildReport {
    /// The child's project name, as the gateway lists it.
    pub name: String,
    /// How it compares with its record.
    pub status: ChildStatus,
    /// The release its record names; null without a record.
    pub release: Option<String>,
    /// The `artifact_sha256` its record holds; null without a record.
    pub recorded_sha256: Option<String>,
    /// The canonical sha256 of the child's export after the settle wait; null
    /// when it could not be read.
    pub live_sha256: Option<String>,
}

impl DeployProjectsResult {
    /// The exit code the process ends with after the envelope: `Some(6)` for
    /// an unverified import (V6), `None` for every other outcome. The
    /// envelope is a success envelope; the code is the verdict.
    pub fn failure_exit_code(&self) -> Option<u8> {
        (self.outcome == ProjectOutcome::ImportedUnverified).then_some(6)
    }
}

/// `text` as one shell word: unchanged when it holds only characters no shell
/// treats specially, else single-quoted.
fn shell_word(text: &str) -> String {
    let plain = !text.is_empty()
        && text.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '@' | '+' | ',')
        });
    if plain {
        text.to_string()
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}

/// The command that puts the saved pre-image back over the project (K11): the
/// plain `ign project import` of the run's `export/<project>.zip`. The path is
/// relative to the repo root, where the run directory lives. It names the
/// project and the path only, never file content.
fn restore_command(project: &str, run_dir_rel: &str) -> String {
    format!(
        "ign project import {} --file {} --collision-policy overwrite --yes",
        shell_word(project),
        shell_word(&format!("{run_dir_rel}/export/{project}.zip")),
    )
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

/// What [`deploy_projects_prepare`] decided.
#[derive(Debug, Clone)]
pub enum ProjectsPrepared {
    /// The gateway would change (`create`, `import` or `record`): the plan
    /// and the prose the confirmation refusal carries.
    Ready {
        /// The confirmation prose: the action, project, gateway, environment,
        /// release, member counts, the removed paths and `artifact_sha256`.
        prose: String,
        /// The plan; its `outcome` is the one the action leads to when it
        /// verifies.
        plan: DeployProjectsResult,
    },
    /// Nothing to do (`skip`): outcome `skipped`. Returned before any session
    /// write, and no `.ign/` directory is created.
    Unchanged(DeployProjectsResult),
}

impl ProjectsPrepared {
    /// The `--check` result: the plan with `check` true and outcome
    /// `checked`. Nothing was written.
    pub fn into_check_result(self) -> DeployProjectsResult {
        let (ProjectsPrepared::Ready {
            plan: mut result, ..
        }
        | ProjectsPrepared::Unchanged(mut result)) = self;
        result.check = true;
        result.outcome = ProjectOutcome::Checked;
        result
    }
}

/// The plan as the confirmation refusal shows it (K5): what will happen, to
/// which project on which gateway, which release, how many members change,
/// the paths an overwrite deletes and the hash that was reviewed. Names,
/// counts and hashes only; the CLI escapes it for a terminal.
fn plan_prose(plan: &DeployProjectsResult) -> String {
    let action = match plan.action {
        ProjectAction::Create => "create project",
        ProjectAction::Import => "overwrite-import project",
        ProjectAction::Record => "record the release only for project",
        ProjectAction::Skip => "leave project",
    };
    let mut lines = vec![format!(
        "deploy projects will {action} {} on gateway {} (env {}, release {})",
        plan.project, plan.gateway, plan.env, plan.release
    )];
    if plan.action == ProjectAction::Record {
        lines.push("the project itself is not changed".to_string());
    } else {
        lines.push(format!(
            "members: {} added, {} changed, {} removed, {} same",
            plan.members.added, plan.members.changed, plan.members.removed, plan.members.same
        ));
    }
    if plan.removed_count > 0 {
        lines.push(format!(
            "the import deletes {} member(s) the repo does not have:",
            plan.removed_count
        ));
        for path in plan.removed.iter().take(PROSE_REMOVED_SHOWN) {
            lines.push(format!("  {path}"));
        }
        if plan.removed_count > PROSE_REMOVED_SHOWN {
            lines.push(format!(
                "  ... and {} more",
                plan.removed_count - PROSE_REMOVED_SHOWN
            ));
        }
    }
    if plan.action == ProjectAction::Import {
        lines.push("the live project is saved into a run directory before the import".to_string());
    }
    lines.push(format!("artifact_sha256 {}", plan.artifact_sha256));
    lines.join("\n")
}

/// Removed paths the confirmation prose lists before it counts the rest.
const PROSE_REMOVED_SHOWN: usize = 20;

/// The comparison both `--check` and the apply start from: probe the host for
/// a `.git`, then list, find, export and digest the live project and decide
/// the action. Sends GET requests only and writes nothing; the apply's writes
/// are [`deploy_projects_run`]'s.
///
/// The `.git` probe comes first (DPRJ-04, D-11): a project the git module
/// still manages is refused as `project_git_managed` before any gateway read.
/// An unreadable probe is `transport_failed`, never "no git".
pub async fn deploy_projects_prepare(
    loaded: &LoadedProject,
    input: &ProjectsInput,
    surfaces: &ProjectsSurfaces<'_>,
) -> Result<ProjectsPrepared, CoreError> {
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

    let plan = DeployProjectsResult {
        env: input.env.clone(),
        gateway: loaded.target.gateway.clone(),
        project: project.clone(),
        release: input.release.clone(),
        check: false,
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
        run_id: None,
        run_dir: None,
        record_written: false,
        restore_command: None,
        children: Vec::new(),
        outcome: match action {
            ProjectAction::Create => ProjectOutcome::Created,
            ProjectAction::Skip => ProjectOutcome::Skipped,
            ProjectAction::Import => ProjectOutcome::Imported,
            ProjectAction::Record => ProjectOutcome::Recorded,
        },
        ok: true,
        slug: None,
    };
    Ok(if action == ProjectAction::Skip {
        ProjectsPrepared::Unchanged(plan)
    } else {
        ProjectsPrepared::Ready {
            prose: plan_prose(&plan),
            plan,
        }
    })
}

/// A step of a run, for the events log and the observer.
///
/// The fields are names, ids, counts and hashes only: never a project file,
/// a secret or a gateway response body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectsEvent {
    /// The Designer list was read under `--designers wait` (25-14). Heard
    /// as it happens and logged once the run directory exists.
    DesignersPoll {
        /// How many sessions were open at that read.
        open: usize,
    },
    /// `--designers proceed` went ahead over these open sessions (D-15).
    DesignersProceed {
        /// `(user, project, host)` of each open session.
        sessions: Vec<DesignerSeen>,
    },
    /// The Designer check did not run: a `record` run does not touch the
    /// project.
    DesignersSkipped,
    /// The run directory exists and the run is about to start.
    RunStarted {
        /// The run's id.
        run_id: String,
        /// What the run will do.
        action: ProjectAction,
    },
    /// The live project was saved into the run directory.
    PreImageExported {
        /// Size of the saved export.
        bytes: usize,
    },
    /// The import request is about to be sent.
    ImportSent {
        /// True when the live project is replaced, false when it is created.
        overwrite: bool,
    },
    /// The gateway answered the import.
    ImportAnswered,
    /// The project was read back and compared with the artifact.
    Verified {
        /// Whether the canonical digests are equal.
        equal: bool,
    },
    /// The release record was written.
    RecordWritten,
    /// The release record could not be written.
    RecordFailed {
        /// The error's slug.
        code: String,
    },
    /// The sweep found projects that inherit from the deployed one and is
    /// reading their exports until they settle (D-06, K7).
    ChildrenSettling {
        /// The children, byte-sorted.
        children: Vec<String>,
    },
    /// One child was compared with its release record.
    ChildChecked {
        /// The child's name.
        name: String,
        /// How it compared.
        status: ChildStatus,
    },
    /// The run ended.
    Finished {
        /// How it ended.
        outcome: ProjectOutcome,
    },
}

impl ProjectsEvent {
    /// The event name `events.ndjson` carries.
    pub fn name(&self) -> &'static str {
        match self {
            ProjectsEvent::DesignersPoll { .. } => "designers_poll",
            ProjectsEvent::DesignersProceed { .. } => "designers_proceed",
            ProjectsEvent::DesignersSkipped => "designers_skipped",
            ProjectsEvent::RunStarted { .. } => "run_started",
            ProjectsEvent::PreImageExported { .. } => "pre_image_exported",
            ProjectsEvent::ImportSent { .. } => "import_sent",
            ProjectsEvent::ImportAnswered => "import_answered",
            ProjectsEvent::Verified { .. } => "verified",
            ProjectsEvent::RecordWritten => "record_written",
            ProjectsEvent::RecordFailed { .. } => "record_failed",
            ProjectsEvent::ChildrenSettling { .. } => "children_settling",
            ProjectsEvent::ChildChecked { .. } => "child_checked",
            ProjectsEvent::Finished { .. } => "finished",
        }
    }

    fn fields(&self) -> serde_json::Value {
        match self {
            ProjectsEvent::DesignersPoll { open } => serde_json::json!({ "open": open }),
            ProjectsEvent::DesignersProceed { sessions } => {
                serde_json::json!({ "sessions": sessions_json(sessions) })
            }
            ProjectsEvent::DesignersSkipped => serde_json::json!({ "reason": "record only" }),
            ProjectsEvent::RunStarted { run_id, action } => {
                serde_json::json!({ "run_id": run_id, "action": action })
            }
            ProjectsEvent::PreImageExported { bytes } => serde_json::json!({ "bytes": bytes }),
            ProjectsEvent::ImportSent { overwrite } => {
                serde_json::json!({ "overwrite": overwrite })
            }
            ProjectsEvent::Verified { equal } => serde_json::json!({ "equal": equal }),
            ProjectsEvent::RecordFailed { code } => serde_json::json!({ "code": code }),
            ProjectsEvent::ChildrenSettling { children } => {
                serde_json::json!({ "children": children })
            }
            ProjectsEvent::ChildChecked { name, status } => {
                serde_json::json!({ "name": name, "status": status.as_str() })
            }
            ProjectsEvent::Finished { outcome } => {
                serde_json::json!({ "outcome": outcome.as_str() })
            }
            ProjectsEvent::ImportAnswered | ProjectsEvent::RecordWritten => serde_json::Value::Null,
        }
    }
}

/// Receives each [`ProjectsEvent`] as the run goes (the CLI prints them to
/// stderr in human mode, 25-12). Actions never print.
pub trait ProjectsObserver: Send + Sync {
    /// One step happened.
    fn event(&self, event: &ProjectsEvent);
}

/// An observer that ignores every event.
pub struct NoProjectsObserver;

impl ProjectsObserver for NoProjectsObserver {
    fn event(&self, _event: &ProjectsEvent) {}
}

/// What [`deploy_projects_run`] needs besides the plan.
pub struct ProjectsRunOptions<'a> {
    /// The project repo root: the run directory lives below it.
    pub repo: PathBuf,
    /// The root `target_file` and a relative `known_hosts` or `data_dir`
    /// resolve against (`--targets`, default the repo).
    pub targets_root: PathBuf,
    /// The target file's name below the targets root.
    pub target_file: String,
    /// The master node. The import and the record go here and nowhere else;
    /// `nodes.backup` is never contacted (the redundancy sync carries the
    /// project to it).
    pub master: NodeTarget,
    /// A record writer standing in for the one built from `master`. For the
    /// engine tests only: the CLI always passes `None`.
    #[doc(hidden)]
    pub record_writer: Option<&'a dyn ReleaseRecordWriter>,
    /// `--designers`, `--designers-timeout` and the poll interval (25-14).
    pub designers: DesignersPolicy,
    /// The longest the children sweep waits for the children's exports to
    /// settle (K7); the CLI passes [`CHILDREN_SETTLE_S`] seconds.
    pub children_settle: Duration,
    /// How often the sweep reads the children while it waits; the CLI passes
    /// [`CHILDREN_POLL_S`] seconds. The engine tests pass short values, never
    /// an environment variable.
    pub children_poll: Duration,
}

/// The events log of one run. Before the gateway changes a log failure stops
/// the run; after it, the work that is left (verify, record) matters more than
/// the log, so a failure is remembered and reported as a warning.
struct RunEvents<'a> {
    log: EventLog,
    observer: &'a dyn ProjectsObserver,
    late_failure: bool,
}

impl RunEvents<'_> {
    fn emit(&mut self, event: ProjectsEvent) -> Result<(), CoreError> {
        self.observer.event(&event);
        self.log.append(event.name(), event.fields())
    }

    /// Log `event` without telling the observer, which heard it live.
    fn record(&mut self, event: &ProjectsEvent) -> Result<(), CoreError> {
        self.log.append(event.name(), event.fields())
    }

    fn emit_late(&mut self, event: ProjectsEvent) {
        if self.emit(event).is_err() {
            self.late_failure = true;
        }
    }
}

/// `(user, project, host)` triples as `run.json` and the events carry them:
/// names only (D-15).
fn sessions_json(sessions: &[DesignerSeen]) -> Vec<serde_json::Value> {
    sessions
        .iter()
        .map(|(user, project, host)| {
            serde_json::json!({ "user": user, "project": project, "host": host })
        })
        .collect()
}

/// What the Designer pre-check found, for `run.json` and the events.
struct DesignersFound {
    /// `fail`, `wait`, `proceed`, or `skipped` for a run that does not touch
    /// the project.
    mode: &'static str,
    /// The timeout in seconds; `None` when the check did not run.
    timeout_s: Option<u64>,
    /// The reads `wait` made, in order.
    polls: Vec<usize>,
    /// The sessions open at the last read.
    seen: Vec<DesignerSeen>,
    /// Whether the run goes ahead over those sessions.
    proceeded: bool,
    /// Whether the check was skipped (a `record` plan).
    skipped: bool,
}

/// The Designer pre-check (25-14, K12): any open session refuses an import
/// unless `--designers proceed` was given. Runs before the run directory
/// exists, so a refusal leaves nothing behind. A `record` plan does not touch
/// the project and skips it.
async fn designers_check(
    plan: &DeployProjectsResult,
    gateway: &dyn ProjectGateway,
    policy: &DesignersPolicy,
    observer: &dyn ProjectsObserver,
) -> Result<DesignersFound, CoreError> {
    if plan.action == ProjectAction::Record {
        return Ok(DesignersFound {
            mode: "skipped",
            timeout_s: None,
            polls: Vec::new(),
            seen: Vec::new(),
            proceeded: false,
            skipped: true,
        });
    }
    let mut polls = Vec::new();
    let (verdict, seen) =
        designers_gate(|| gateway.designers(), policy, &SystemClock, &mut |open| {
            polls.push(open);
            observer.event(&ProjectsEvent::DesignersPoll { open });
            Ok(())
        })
        .await?;
    if verdict == DesignersVerdict::Open {
        return Err(CoreError::DeployPreflightFailed {
            failures: Box::new([PreflightFailure {
                check: "designers".to_string(),
                detail: designers_detail(&seen),
            }]),
        });
    }
    Ok(DesignersFound {
        mode: policy.mode.as_str(),
        timeout_s: Some(policy.timeout.as_secs()),
        polls,
        seen,
        proceeded: verdict == DesignersVerdict::Proceeded,
        skipped: false,
    })
}

/// The wrong-target pre-check (25-14, T-25-44): the gateway's REST name must
/// be the host's `systemName` and `nodes.master.gateway_name` when the target
/// declares it. Runs for every confirmed plan, `record` included, because the
/// record is a write to that host too. An unanswerable read is its own error.
async fn wrong_target_check(
    loaded: &LoadedProject,
    gateway: &dyn ProjectGateway,
    opts: &ProjectsRunOptions<'_>,
) -> Result<(), CoreError> {
    let rest = gateway.gateway_name().await?;
    let reader = for_node(
        &opts.targets_root,
        &opts.target_file,
        "nodes.master",
        &opts.master,
    )?;
    let problems = wrong_target_gate(
        rest.as_deref(),
        reader.as_ref(),
        &loaded.target.mode,
        opts.master.gateway_name.as_deref(),
    )
    .await?;
    if problems.is_empty() {
        return Ok(());
    }
    Err(CoreError::DeployPreflightFailed {
        failures: Box::new([wrong_target_failure(&problems)]),
    })
}

/// The release record this run leaves (V5).
fn release_record(plan: &DeployProjectsResult, release: &str, run_id: &RunId) -> ReleaseRecord {
    ReleaseRecord {
        schema_version: 1,
        gateway: plan.gateway.clone(),
        project: plan.project.clone(),
        release: release.to_string(),
        artifact_sha256: plan.artifact_sha256.clone(),
        run_id: run_id.as_str().to_string(),
        deployed_at: rfc3339_now_utc(),
    }
}

fn stale_live(expected: String, actual: String) -> CoreError {
    CoreError::PlanStale {
        expected,
        actual,
        detail: "live project changed since the plan".to_string(),
    }
}

/// The confirmed write path: see the module documentation for the order.
///
/// `prepared` is what [`deploy_projects_prepare`] returned for the same
/// `loaded` and `input`; an `Unchanged` plan returns as it is, before any run
/// directory exists. Every return after the run directory exists writes
/// `result.json` first (an error is recorded by its slug only).
pub async fn deploy_projects_run(
    prepared: ProjectsPrepared,
    loaded: &LoadedProject,
    input: &ProjectsInput,
    surfaces: &ProjectsSurfaces<'_>,
    opts: &ProjectsRunOptions<'_>,
    observer: &dyn ProjectsObserver,
) -> Result<DeployProjectsResult, CoreError> {
    let plan = match prepared {
        ProjectsPrepared::Unchanged(result) => return Ok(result),
        ProjectsPrepared::Ready { plan, .. } => plan,
    };
    // The record writer is built before anything is written, so a node that
    // cannot take a record refuses now and not after the import.
    let built;
    let writer: &dyn ReleaseRecordWriter = match opts.record_writer {
        Some(writer) => writer,
        None => {
            built = release_writer_for_node(
                &opts.targets_root,
                &opts.target_file,
                "nodes.master",
                &opts.master,
            )?;
            built.as_ref()
        }
    };
    // The pre-checks read the gateway and write nothing: a refusal leaves no
    // run directory, no pre-image, no import and no record.
    wrong_target_check(loaded, surfaces.gateway, opts).await?;
    let designers = designers_check(&plan, surfaces.gateway, &opts.designers, observer).await?;
    let (run_id, run_dir) = RunDir::create_fresh(&opts.repo, &plan.gateway)?;
    let rel = run_dir_rel(&plan.gateway, &run_id);
    let outcome = execute(
        &plan, loaded, input, surfaces, opts, writer, &run_dir, &run_id, &rel, &designers, observer,
    )
    .await;
    match outcome {
        Ok(mut result) => {
            if let Err(err) = run_dir.write_json("result.json", &result) {
                result.warnings.push(format!(
                    "the run's result.json could not be written ({})",
                    err.code()
                ));
            }
            Ok(result)
        }
        Err(err) => {
            // The slug only: no message text, no hash, no file content.
            let _ = run_dir.write_json(
                "result.json",
                &serde_json::json!({
                    "ok": false,
                    "slug": err.code(),
                    "run_id": run_id.as_str(),
                }),
            );
            Err(err)
        }
    }
}

/// What the children sweep found.
struct ChildSweep {
    children: Vec<ChildReport>,
    warnings: Vec<String>,
}

/// One child's export as the sweep reads it: the canonical digest, or the
/// error code of the read.
type ChildRead = Result<String, String>;

async fn read_children(gateway: &dyn ProjectGateway, names: &[String]) -> Vec<ChildRead> {
    let mut reads = Vec::with_capacity(names.len());
    for name in names {
        reads.push(match gateway.export_bytes(name).await {
            Ok(zip) => live_digest(name, &zip)
                .map(|digest| digest.sha256)
                .map_err(|err| err.code().to_string()),
            Err(err) => Err(err.code().to_string()),
        });
    }
    reads
}

/// The children sweep (D-06, K7): for every project whose `parent` is the
/// deployed one, wait until two consecutive reads of every child's export
/// agree or `children_settle` has passed, then compare each child's canonical
/// digest with the record its own last deploy left.
///
/// It never fails: a listing, export or record that cannot be read becomes a
/// warning carrying the error code only and, for a child, the status
/// `unverified`. Its gateway calls are `projects` and the children's
/// `export_bytes`; its host call is `read_release_record`. Only direct
/// children are swept (a flagged assumption of 25-15).
async fn sweep_children(
    result: &DeployProjectsResult,
    loaded: &LoadedProject,
    surfaces: &ProjectsSurfaces<'_>,
    opts: &ProjectsRunOptions<'_>,
    events: &mut RunEvents<'_>,
) -> ChildSweep {
    let project = result.project.as_str();
    let gateway = surfaces.gateway;
    let mut sweep = ChildSweep {
        children: Vec::new(),
        warnings: Vec::new(),
    };

    let records = match gateway.projects().await {
        Ok(records) => records,
        Err(err) => {
            sweep.warnings.push(format!(
                "could not list the projects to check the children of {project} ({})",
                err.code()
            ));
            return sweep;
        }
    };
    let mut names: Vec<String> = records
        .into_iter()
        .filter(|record| record.parent.as_deref() == Some(project))
        .map(|record| record.name)
        .collect();
    names.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    names.dedup();
    if names.is_empty() {
        return sweep;
    }
    events.emit_late(ProjectsEvent::ChildrenSettling {
        children: names.clone(),
    });

    // Settle: read every child, wait, read again; stop when the last two
    // reads of every child agree or the bound has passed.
    let started = Instant::now();
    let mut current = read_children(gateway, &names).await;
    let mut unsettled = vec![true; names.len()];
    loop {
        let elapsed = started.elapsed();
        if elapsed >= opts.children_settle {
            break;
        }
        tokio::time::sleep(opts.children_poll.min(opts.children_settle - elapsed)).await;
        let next = read_children(gateway, &names).await;
        unsettled = current.iter().zip(&next).map(|(a, b)| a != b).collect();
        current = next;
        if !unsettled.iter().any(|changing| *changing) {
            break;
        }
    }

    let gateway_name = result.gateway.as_str();
    let root = LedgerRoot::for_node(&loaded.target.nodes.master);
    for ((name, read), changing) in names.iter().zip(current).zip(unsettled) {
        let live = match read {
            Ok(sha256) => Some(sha256),
            Err(code) => {
                sweep.warnings.push(format!(
                    "child project {name}: its export could not be read ({code})"
                ));
                None
            }
        };
        if changing {
            sweep.warnings.push(format!(
                "child project {name} did not settle within {} s",
                opts.children_settle.as_secs()
            ));
        }
        let record = match surfaces.host.read_release_record(gateway_name, name).await {
            Ok(None) => None,
            Ok(Some(bytes)) => {
                let location = release_record_location(root, gateway_name, name);
                match ReleaseRecord::parse(&bytes, &location, gateway_name, name) {
                    Ok(record) => Some(record),
                    Err(err) => {
                        sweep.warnings.push(format!(
                            "child project {name}: its release record could not be read ({})",
                            err.code()
                        ));
                        None
                    }
                }
            }
            Err(err) => {
                sweep.warnings.push(format!(
                    "child project {name}: its release record could not be read ({})",
                    err.code()
                ));
                None
            }
        };
        let status = match (&record, &live) {
            (Some(record), Some(live)) if record.artifact_sha256 == *live => ChildStatus::Ok,
            (Some(record), Some(_)) => {
                sweep.warnings.push(format!(
                    "child project {name} no longer matches the release this tool last deployed to it ({})",
                    record.release
                ));
                ChildStatus::Mismatch
            }
            _ => ChildStatus::Unverified,
        };
        events.emit_late(ProjectsEvent::ChildChecked {
            name: name.clone(),
            status,
        });
        sweep.children.push(ChildReport {
            name: name.clone(),
            status,
            release: record.as_ref().map(|r| r.release.clone()),
            recorded_sha256: record.map(|r| r.artifact_sha256),
            live_sha256: live,
        });
    }
    sweep
}

#[allow(clippy::too_many_arguments)]
async fn execute(
    plan: &DeployProjectsResult,
    loaded: &LoadedProject,
    input: &ProjectsInput,
    surfaces: &ProjectsSurfaces<'_>,
    opts: &ProjectsRunOptions<'_>,
    writer: &dyn ReleaseRecordWriter,
    run_dir: &RunDir,
    run_id: &RunId,
    rel: &str,
    designers: &DesignersFound,
    observer: &dyn ProjectsObserver,
) -> Result<DeployProjectsResult, CoreError> {
    let project = plan.project.as_str();
    let gateway = surfaces.gateway;
    let artifact = &loaded.artifact.digest;

    run_dir.write_json(
        "run.json",
        &serde_json::json!({
            "run_id": run_id.as_str(),
            "started_at": rfc3339_now_utc(),
            "env": plan.env,
            "gateway": plan.gateway,
            "project": plan.project,
            "release": plan.release,
            "action": plan.action,
            "artifact_sha256": plan.artifact_sha256,
            "live_sha256": plan.live_sha256,
            "allow_downgrade": input.allow_downgrade,
            "allow_first_change": input.allow_first_change,
            "designers": designers.mode,
            "designers_timeout_s": designers.timeout_s,
            "designers_seen": sessions_json(&designers.seen),
            "run_dir_git": run_dir.git_status().as_record(),
        }),
    )?;
    run_dir.write_json("plan.json", plan)?;
    let mut events = RunEvents {
        log: EventLog::open(run_dir)?,
        observer,
        late_failure: false,
    };
    events.emit(ProjectsEvent::RunStarted {
        run_id: run_id.as_str().to_string(),
        action: plan.action,
    })?;
    // The Designer pre-check ran before this directory existed: its reads,
    // and the choice to go ahead, are put on record now (D-15).
    for open in &designers.polls {
        events.record(&ProjectsEvent::DesignersPoll { open: *open })?;
    }
    if designers.proceeded {
        events.emit(ProjectsEvent::DesignersProceed {
            sessions: designers.seen.clone(),
        })?;
    }
    if designers.skipped {
        events.emit(ProjectsEvent::DesignersSkipped)?;
    }

    let mut result = plan.clone();
    result.run_id = Some(run_id.as_str().to_string());
    result.run_dir = Some(rel.to_string());
    let record = release_record(plan, &loaded.release.tag, run_id);

    if plan.action == ProjectAction::Record {
        // Only the record moves, and only for a project that still is the
        // artifact: the record claims exactly that.
        let zip = gateway.export_bytes(project).await?;
        let live = live_digest(project, &zip)?;
        if live.sha256 != artifact.sha256 {
            return Err(stale_live(artifact.sha256.clone(), live.sha256));
        }
        events.emit(ProjectsEvent::Verified { equal: true })?;
        if let Err(err) = writer
            .write_release_record(&plan.gateway, project, &record.to_bytes())
            .await
        {
            events.emit_late(ProjectsEvent::RecordFailed {
                code: err.code().to_string(),
            });
            return Err(err);
        }
        events.emit(ProjectsEvent::RecordWritten)?;
        result.record_written = true;
        result.outcome = ProjectOutcome::Recorded;
        events.emit_late(ProjectsEvent::Finished {
            outcome: result.outcome,
        });
        return Ok(result);
    }

    let overwrite = plan.action == ProjectAction::Import;
    if overwrite {
        // The pre-image (DPRJ-03): the live project as it is now, saved
        // before it is replaced. It is secret-bearing and goes to the 0600
        // file in the run directory and nowhere else.
        let pre = gateway.export_bytes(project).await?;
        let live = live_digest(project, &pre)?;
        let mut file = run_dir.create_private_file_in("export", &format!("{project}.zip"))?;
        file.write_all(&pre)
            .and_then(|()| file.sync_all())
            .map_err(|e| CoreError::DeployRunInvalid {
                run_id: run_id.as_str().to_string(),
                problem: format!("could not write the pre-image export ({:?})", e.kind()),
            })?;
        events.emit(ProjectsEvent::PreImageExported { bytes: pre.len() })?;
        // K15: the project the plan was drawn against is the one that is
        // about to be replaced.
        if plan.live_sha256.as_deref() != Some(live.sha256.as_str()) {
            return Err(stale_live(
                plan.live_sha256
                    .clone()
                    .unwrap_or_else(|| "none".to_string()),
                live.sha256,
            ));
        }
    }

    events.emit(ProjectsEvent::ImportSent { overwrite })?;
    let answer = gateway
        .project_import(project, loaded.artifact.zip.clone(), overwrite)
        .await?;
    // The gateway has changed. Nothing below returns an error: what is left
    // is verifying, recording and reporting.
    events.emit_late(ProjectsEvent::ImportAnswered);
    if answer
        .response
        .get("changes")
        .and_then(serde_json::Value::as_array)
        .is_some_and(Vec::is_empty)
    {
        result
            .warnings
            .push("the gateway reported no changes for this import".to_string());
    }

    let equal = match gateway.export_bytes(project).await {
        Ok(zip) => match live_digest(project, &zip) {
            Ok(live) => Some(live.sha256 == artifact.sha256),
            Err(err) => {
                result.warnings.push(format!(
                    "could not verify the import: the project read back is unusable ({})",
                    err.code()
                ));
                None
            }
        },
        Err(err) => {
            result.warnings.push(format!(
                "could not verify the import: the project could not be read back ({})",
                err.code()
            ));
            None
        }
    };
    events.emit_late(ProjectsEvent::Verified {
        equal: equal == Some(true),
    });

    if equal == Some(true) {
        match writer
            .write_release_record(&plan.gateway, project, &record.to_bytes())
            .await
        {
            Ok(()) => {
                events.emit_late(ProjectsEvent::RecordWritten);
                result.record_written = true;
            }
            Err(err) => {
                events.emit_late(ProjectsEvent::RecordFailed {
                    code: err.code().to_string(),
                });
                result.warnings.push(format!(
                    "the import is verified but the release record could not be written ({})",
                    err.code()
                ));
            }
        }
        // K6 a: the gateway's own verdict on the parent link, read after the
        // import. A true value is a warning, never a failure.
        if let Some(parent) = &loaded.meta.parent {
            match gateway.projects().await {
                Ok(records) => {
                    if records
                        .iter()
                        .find(|r| r.name == project)
                        .is_some_and(|r| r.invalid_parent == Some(true))
                    {
                        result.warnings.push(format!(
                            "project {project}: after the import the gateway reports that its parent {parent} is invalid"
                        ));
                    }
                }
                Err(err) => result.warnings.push(format!(
                    "project {project}: the parent could not be read back after the import ({})",
                    err.code()
                )),
            }
        }
        result.outcome = if overwrite {
            ProjectOutcome::Imported
        } else {
            ProjectOutcome::Created
        };
    } else {
        // K11: the project is not what was reviewed, and nothing here makes
        // it worse: no record, no second import, no further gateway call.
        result.outcome = ProjectOutcome::ImportedUnverified;
        result.ok = false;
        result.slug = Some(PROJECT_UNVERIFIED_SLUG.to_string());
        // K11: no automatic restore. The operator gets the command that
        // would, when a pre-image exists (an overwrite; a created project has
        // nothing to go back to).
        if overwrite {
            result.restore_command = Some(restore_command(project, rel));
        }
    }
    // D-06, K7: a parent that was replaced is followed by a look at the
    // projects that inherit from it. Reads only, and nothing in it can change
    // the outcome, the exit code or the gateway.
    if matches!(
        result.outcome,
        ProjectOutcome::Imported | ProjectOutcome::ImportedUnverified
    ) {
        let sweep = sweep_children(&result, loaded, surfaces, opts, &mut events).await;
        result.children = sweep.children;
        result.warnings.extend(sweep.warnings);
    }
    events.emit_late(ProjectsEvent::Finished {
        outcome: result.outcome,
    });
    if events.late_failure {
        result
            .warnings
            .push("the run's events log could not be written to the end".to_string());
    }
    Ok(result)
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
