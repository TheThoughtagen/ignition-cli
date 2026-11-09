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
//! [`ProjectGateway`] has eight reads and four write kinds: the project
//! import, [`ProjectGateway::project_import`], the tag import,
//! [`ProjectGateway::tag_import`] (Phase 26, D-08: the gateway writes its own
//! tag files), and an owned provider's settings,
//! [`ProjectGateway::tag_provider_create`] and
//! [`ProjectGateway::tag_provider_modify`] (26-10). `--check` reaches none of
//! them. The one host write besides the records is the tag-group commit
//! through the master's [`ApplyHost`] (26-10), which only
//! [`deploy_projects_run`] builds. [`ProjectHost`](crate::deploy::transport::projects::ProjectHost) is
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
//! run_id, run_dir, record_written, restore_command, children, missing_tags,
//! missing_tags_count, missing_tags_unknown_providers,
//! missing_tags_unknown_providers_count, missing_tags_unavailable,
//! missing_tags_limit, tags, outcome, ok, slug`. Later plans add keys
//! additively.
//! Lists are byte-sorted and capped at [`LIST_CAP`] entries with the full
//! count beside them. Output carries names, hashes, counts and tag paths
//! only. A tag path is a name the project's own code spells out; it is read
//! from anchored literals, never from surrounding text.
//!
//! ## Project-owned tags (Phase 26, D-07)
//!
//! A project whose `deploy/project.yaml` lists `tag_providers` owns those
//! providers: [`load_project`] reads each one from `tags/<P>/`
//! ([`read_provider_with_raw`]; a provider with no node refuses) and checks
//! each file's byte form ([`check_canonical_form`]);
//! [`deploy_projects_prepare`] reads this project's tag record of each
//! provider from the host (another project's record refuses
//! `tag_provider_claimed`), exports each provider from the gateway once and
//! plans it against the repo and the record ([`plan_tags`]) into `data.tags`
//! ([`TagsReport`]; null for a project that owns none). It also reads each
//! owned provider's resource record and plans its settings
//! (`data.tags.provider_settings`: `same`, `create` for a provider the
//! gateway lacks, or `modify`), and for a provider whose repo holds
//! `.tag-groups.json` reads its `tag-group` units from the master
//! ([`TreeReader::read_tag_groups`]) and plans them (`data.tags.groups`,
//! [`plan_groups`]).
//!
//! Two more refusals fire in the prepare (26-10, so `--check` fires them):
//! R3-GRP, a tag the run sends naming a group in neither the summary nor
//! the live units (C13: the import accepts it silently); and R3-MODE on a
//! pair, a `tag-definition` or `tag-type-definition` path of an owned
//! provider in the mode collection (C8, F7: the import would be written
//! there and wedge the pair's config sync). On a standalone target C8 shows
//! the import edits the effective mode copy (A4 holds), so nothing is
//! refused there.
//!
//! The tag gates (26-07) fire before any write: `provider_emptied` and
//! `first_deploy` (an owned provider with no record that the plan adds to or
//! changes, without `--allow-first-change`) and the D-14 secret rule
//! (`embedded_secret` for a credential the run would send) in the prepare,
//! so `--check` fires them too; `tag_removal` in [`deploy_projects_run`],
//! before any contact, so `--check` lists the removals (no flag passes it in
//! this release: there is no tag delete, R3-DEL). The run deploys the tags
//! and the project in one run, tags first:
//!
//! 1. the project pre-image and its stale check (K15) as before, so nothing
//!    is written for a stale project;
//! 2. per owned provider, its live export is saved as the pre-image
//!    `export/tags/<P>.json` (0600, inside the [`PROJECTS_CANARY_EXEMPT`]
//!    `export` subtree: tag values can hold secrets);
//! 3. the plan is drawn again against those exports and must be the
//!    reviewed one (`plan_stale`), as must every owned provider's settings
//!    plan (a fresh record; a modified provider's record is saved as
//!    `export/tags/<P>.provider.json`) and the group plan (a fresh read); with
//!    group writes ahead, an apply journal on the box refuses
//!    `deploy_journal_present`;
//! 4. the provider settings: created or modified (with the fresh record's
//!    signature) and read back equal; then the tag groups: one file-engine
//!    batch of the group units only (`ledger_new: None`, the gateway-config
//!    ledger is never touched) staged, committed under the config scan-lock,
//!    scanned, read back and finalized, restored from the snapshot on any
//!    failure after the commit;
//! 5. every create and change goes as the ordered, bounded batches of
//!    [`import_batches`] (types first, in dependency order; new nodes
//!    `Abort`; changed nodes `MergeOverwrite` without the AtomicTag runtime
//!    keys; a changed node that drops a property `Overwrite` with its
//!    memory-sourced live values carried from an export read right before
//!    that batch, listed in `data.tags.carried`), and every answer must be
//!    clean: the first that is not stops the step;
//! 6. every owned provider is exported every [`TAG_VERIFY_POLL_S`] until two
//!    reads agree or [`TAG_VERIFY_SETTLE_S`] passes, and every repo node must
//!    equal the export in configuration (R3-VK on every node, created ones
//!    included; R3-KEY; R3-DV with the seed check on created nodes) while
//!    every drift node equals its pre-image (`data.tags.verify`);
//! 7. the project steps as before. Once the project is verified (or, for a
//!    `record` or `skip` project action, once the tag verify is green), one
//!    tag record per owned provider is written to the master's host with the
//!    run id and release of the release record. A verified run ends
//!    `applied`.
//!
//! Provider settings that do not read back equal, a failed group commit, a
//! non-clean import or a red tag verify ends the run before the project
//! import with `rollback_failed` naming the run directory and the writes the
//! run sent; only the group units are restored (from the run's snapshot),
//! nothing else is rolled back automatically yet (26-11).

use std::io::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;

use super::deploy::load_target;
use super::projects::validate_import;
use super::resources::export_zip_bytes;
use super::workspace::rfc3339_now_utc;
use crate::client::deploy_api::scan_and_wait;
use crate::client::projects::{ImportOutcome, ProjectRecord};
use crate::client::query::ListQuery;
use crate::client::scan::{LockVerdict, ScanKind, ScanOutcome};
use crate::client::tags::{
    CollisionPolicy, TagImportAnswer, TagProviderCreate, TagProviderModify, TagProviderRecord,
};
use crate::client::{DeployApi, GatewayApi};
use crate::deploy::apply::fingerprint::fingerprint;
use crate::deploy::apply::hold::SystemClock;
use crate::deploy::apply::hold::{HOLD_S, HeldLock, LOCK_MARGIN_S};
use crate::deploy::apply::host::{ApplyHost, BatchSpec, DeleteOp, WriteOp};
use crate::deploy::apply::journal::{Journal, JournalPhase};
use crate::deploy::apply::preflight::{
    DesignerSeen, DesignersPolicy, DesignersVerdict, designers_detail, designers_gate,
    read_designers, wrong_target_failure, wrong_target_gate,
};
use crate::deploy::apply::record::{RecordState, RunRecord, TouchedEntry};
use crate::deploy::apply::run_store::{EventLog, RunDir, RunId, run_dir_rel};
use crate::deploy::exclude;
use crate::deploy::ledger::{
    LedgerRoot, journal_location, release_record_location, tag_record_location,
};
use crate::deploy::lint::{Sends, scan_tag_node};
use crate::deploy::manifest::Manifest;
use crate::deploy::native_tags::{
    NativeNode, NativeProvider, NonCanonical, TAG_GROUPS_FILE, check_canonical_form,
    read_provider_with_raw,
};
use crate::deploy::project_artifact::{
    Artifact, CanonicalDigest, build_artifact, canonical_digest,
};
use crate::deploy::project_tags::{
    FILES_PER_ENTRY, Lookup, MISSING_TAGS_LIMIT, MissingTag, TagRef, TagRefs, TagTree,
    TagsUnavailable, UnknownProvider, scan_artifact,
};
use crate::deploy::projects::{
    MemberDelta, PROJECT_FILE_REL, ParentLink, ProjectAction, ProjectFile, ProjectMeta,
    ProviderSettings, ReadOnlySources, Release, ReleaseOrder, ReleaseRecord, check_parent,
    check_tag_ownership, gateway_manifest_rel, load_project_file, load_provider_settings,
    member_delta, parent_chain, read_project_meta,
};
use crate::deploy::tag_apply::{
    Carried, ImportBatch, REMOVED_PROPERTY_POLICY, carried, import_batches,
};
use crate::deploy::tag_canon::{CanonOpts, diff_properties, flatten_export, to_node};
use crate::deploy::tag_groups::{
    GroupPlan, GroupSummary, LiveGroups, check_group_refs, parse_group_value, plan_groups,
    referenced_groups,
};
use crate::deploy::tag_plan::{
    ProviderSettingsAction, ProviderSettingsPlan, TagPlan, TagVerify, TagsReport,
    plan_provider_settings, plan_tags, repo_tags_sha256,
};
use crate::deploy::tag_record::TagRecord;
use crate::deploy::target::{NodeTarget, Target};
use crate::deploy::transport::projects::{
    ProjectHost, ReleaseRecordWriter, TagRecordWriter, git_entry_location, release_writer_for_node,
    tag_record_names_problem, tag_record_writer_for_node,
};
use crate::deploy::transport::remote::safe_remote_path;
use crate::deploy::transport::{TreeReader, apply_host_for_node, for_node};
use crate::deploy::tree::read_repo_file;
use crate::deploy::tree::{RenderedTree, sha256_hex};
use crate::error::{CoreError, PreflightFailure, RollbackFailure};

/// Most entries a result list carries; the full count rides beside it.
pub const LIST_CAP: usize = 200;

/// The longest the children sweep waits for the children's exports to settle
/// after a parent import, in seconds (K7, D-06). Capture C9 measured the
/// settle at 5 s on the 8.3.6 rig; 30 s is six times that.
pub const CHILDREN_SETTLE_S: u64 = 30;

/// How often the sweep reads the children while it waits, in seconds (K7).
pub const CHILDREN_POLL_S: u64 = 5;

/// The longest the tag verify waits for two consecutive exports of every
/// owned provider to agree, in seconds (26-08). Capture C9 measured, for
/// 3,000 memory tags, a provider export of 4.3 s and the core files settled
/// 2.3 s after the import answered; 30 s is several such reads. After it the
/// last read is compared anyway and `data.tags.verify.settled` is false.
pub const TAG_VERIFY_SETTLE_S: u64 = 30;

/// How often the tag verify exports the owned providers while it waits, in
/// seconds (26-08): at least one C9-sized export (4.3 s) per poll.
pub const TAG_VERIFY_POLL_S: u64 = 5;

/// The only entry of a run directory that may hold project content (DPRJ-03,
/// SC-5): the `export/` subtree, which holds the pre-image
/// `export/<project>.zip`. A project export is the one secret-bearing file a
/// run writes, because scripts and resources can embed credentials. It is
/// written 0600 inside a 0700 directory below the git-ignored `.ign/`, and
/// nothing else a run writes (`run.json`, `plan.json`, `events.ndjson`,
/// `result.json`), prints or records may carry content. The canary suites name
/// this constant as the sole exemption and assert the file exists and is
/// private, so the exemption cannot hide a missing file.
pub const PROJECTS_CANARY_EXEMPT: &[&str] = &["export"];

/// The gateway calls a project deploy needs, and nothing else.
///
/// Eight reads and four write kinds. There is deliberately no method that
/// deletes, renames, copies or modifies a project: the project write is the
/// import of the project the repo declares, the tag write is a native tag
/// import into a provider, the provider writes create or modify an owned
/// provider's settings (26-10), and the read-only check never reaches any of
/// them (`ForbidProjectWrites` proves that in the engine tests).
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
    /// The names of the tag providers the gateway lists (a read), in the
    /// order it lists them.
    async fn tag_provider_names(&self) -> Result<Vec<String>, CoreError>;
    /// The native export of one tag provider's tree, as the gateway's own
    /// bytes (a read). `provider` is a name `tag_provider_names` returned.
    async fn tag_export(&self, provider: &str) -> Result<Vec<u8>, CoreError>;
    /// Import `zip` as project `name`: `overwrite=true` replaces the whole
    /// project, `overwrite=false` creates it. The only write of the verb. A
    /// 200 answer carrying `success: false` is `import_denied`.
    async fn project_import(
        &self,
        name: &str,
        zip: Vec<u8>,
        overwrite: bool,
    ) -> Result<ImportOutcome, CoreError>;
    /// Import the native tag JSON `body` (`{"tags":[...]}`) into `provider`
    /// below the parent `path` (empty is the provider root) under `policy`.
    /// A tag write: only the run path of later plans calls it, and `--check`
    /// must never reach it. The answer is returned as read; the caller treats
    /// a non-clean answer as a failed step.
    async fn tag_import(
        &self,
        provider: &str,
        path: &str,
        policy: CollisionPolicy,
        body: Vec<u8>,
    ) -> Result<TagImportAnswer, CoreError>;
    /// One tag provider's resource record (a read, 26-10), or `None` when the
    /// gateway has no provider of that name.
    async fn tag_provider_record(&self, name: &str)
    -> Result<Option<TagProviderRecord>, CoreError>;
    /// Create the providers of `body` (a provider-settings write, 26-10).
    /// Only the run path calls it, for an owned provider the gateway lacks.
    async fn tag_provider_create(&self, body: &[TagProviderCreate]) -> Result<(), CoreError>;
    /// Modify the providers of `body` (a provider-settings write, 26-10),
    /// each with the signature of a fresh read. Only the run path calls it,
    /// for an owned provider whose settings differ from the repo's.
    async fn tag_provider_modify(&self, body: &[TagProviderModify]) -> Result<(), CoreError>;
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

    async fn tag_provider_names(&self) -> Result<Vec<String>, CoreError> {
        Ok(self
            .0
            .tag_provider_list(&ListQuery::default())
            .await?
            .items
            .into_iter()
            .map(|record| record.name)
            .collect())
    }

    async fn tag_export(&self, provider: &str) -> Result<Vec<u8>, CoreError> {
        self.0.tags_export_provider(provider).await
    }

    async fn project_import(
        &self,
        name: &str,
        zip: Vec<u8>,
        overwrite: bool,
    ) -> Result<ImportOutcome, CoreError> {
        self.0.project_import(name, zip, overwrite).await
    }

    async fn tag_import(
        &self,
        provider: &str,
        path: &str,
        policy: CollisionPolicy,
        body: Vec<u8>,
    ) -> Result<TagImportAnswer, CoreError> {
        self.0.tags_import(provider, path, policy, body).await
    }

    async fn tag_provider_record(
        &self,
        name: &str,
    ) -> Result<Option<TagProviderRecord>, CoreError> {
        match self.0.tag_provider_find(name).await {
            Ok(record) => Ok(Some(record)),
            Err(CoreError::NotFound { .. }) => Ok(None),
            Err(err) => Err(err),
        }
    }

    async fn tag_provider_create(&self, body: &[TagProviderCreate]) -> Result<(), CoreError> {
        self.0.tag_provider_create(body).await
    }

    async fn tag_provider_modify(&self, body: &[TagProviderModify]) -> Result<(), CoreError> {
        self.0.tag_provider_modify(body).await
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
    /// The master's tree reader (26-10): the tag-group units, the apply
    /// journal and, on a pair, the mode collection's tag type folders. The
    /// CLI builds it for a project that lists `tag_providers`; a check that
    /// needs it and finds `None` is an internal error.
    pub tree: Option<&'a dyn TreeReader>,
    /// The master's apply host (26-10): the one writer of the tag-group
    /// units, through the file engine's commit and restore. A test seam: when
    /// `None`, [`deploy_projects_run`] builds it from the target for a plan
    /// that writes groups, so no CLI surface names the write half.
    pub apply_host: Option<&'a dyn ApplyHost>,
    /// The master's deploy API (26-10): the scan-lock and the config scan
    /// around the tag-group commit.
    pub deploy: Option<&'a dyn DeployApi>,
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
    /// `--tags-sha256`: the `data.tags.tags_sha256` the reviewer saw (64
    /// lowercase hex), the digest of the owned tag content (26-05, TAGDEP-10,
    /// D-07). Compared with the checkout's owned tags before any host or
    /// gateway contact. Refused for a project that owns no provider; `None`
    /// is allowed here because the CLI (Decision C) decides when it is
    /// required.
    pub tags_sha256: Option<String>,
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
    /// The owned tag providers (`tag_providers`), read from `tags/<P>/`, in
    /// the order the file lists them; empty for a project that owns none.
    pub owned_tags: Vec<NativeProvider>,
    /// Each owned provider's settings (`deploy/tag-providers/<P>.json`,
    /// D-09), in the order the file lists them; empty for a project that owns
    /// none.
    pub provider_settings: Vec<ProviderSettings>,
    /// The tag providers the run never reads or writes: the effective
    /// exclusion set ([`exclude::effective`]). For a tag-owning project it
    /// includes the gateway-config manifest's exclude and read-only lists
    /// when the manifest exists; otherwise it is the target's set, as before
    /// Phase 26.
    pub tag_exclusions: Vec<String>,
    /// What the ownership check could not establish (no gateway-config
    /// manifest to check against), reported in `data.tags.warnings`.
    pub ownership_warnings: Vec<String>,
    /// Each owned provider's repo files the git module would rewrite
    /// ([`check_canonical_form`], TAGDEP-12), keyed by the provider's name;
    /// reported in `data.tags.non_canonical`.
    pub non_canonical: std::collections::BTreeMap<String, Vec<NonCanonical>>,
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
    if let Some(hash) = &input.tags_sha256
        && !is_lower_hex64(hash)
    {
        return Err(CoreError::InvalidInput {
            reason: "--tags-sha256 must be 64 lowercase hex characters (the \
                     `data.tags.tags_sha256` of `ign deploy projects --check --json`)"
                .to_string(),
        });
    }
    let file = load_project_file(&input.repo)?;
    file.require_env(&input.env)?;
    let meta = read_project_meta(&input.repo)?;
    let (target_file, target) = load_target(&input.targets, &input.env)?;
    // Ownership (D-04, D-05, D-06), offline: a read-only provider (`System`,
    // an excluded one, one on a `read_only_tag_providers` list) or one the
    // gateway-config manifest still deploys cannot be owned. The manifest is
    // read only for a project that owns a provider.
    let (tag_exclusions, ownership_warnings) = if file.tag_providers.is_empty() {
        (exclude::effective(&target, None), Vec::new())
    } else {
        let manifest_rel = gateway_manifest_rel(&target.gateway);
        let manifest = read_gateway_manifest(&input.targets, &manifest_rel)?;
        let manifest = manifest.as_ref().map(|m| (manifest_rel.as_str(), m));
        let sources = ReadOnlySources::new(&target_file, &target, manifest);
        let warnings = check_tag_ownership(&file, &sources, manifest)?;
        (
            exclude::effective(&target, manifest.map(|(_, m)| m)),
            warnings,
        )
    };
    // The release record is written after the import. A name the writer will
    // refuse then is refused now, before any gateway or host is contacted.
    record_names_fit_the_host(
        &file.project,
        &file.tag_providers,
        &target.gateway,
        &target_file,
        &target.nodes.master,
    )?;
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
            hash: "artifact_sha256",
            expected: expected.clone(),
            actual: artifact.digest.sha256.clone(),
        });
    }
    // Every owned provider is read now, offline: a missing `tags/<P>/` or a
    // bad node file refuses before any host or gateway contact.
    let provider_settings = file
        .tag_providers
        .iter()
        .map(|provider| load_provider_settings(&input.repo, provider))
        .collect::<Result<Vec<_>, _>>()?;
    let mut owned_tags = Vec::new();
    let mut non_canonical = std::collections::BTreeMap::new();
    for provider in &file.tag_providers {
        let (tags, raw) = read_provider_with_raw(&input.repo, provider)?;
        // Pitfall 2: an owned provider is never deployed empty.
        if tags.nodes.is_empty() {
            return Err(CoreError::deploy_tree_invalid(
                format!("{}/{provider}", crate::deploy::native_tags::TAGS_DIR),
                None,
                None,
                "holds no tag node: an owned provider is never deployed empty; drop it from \
                 tag_providers in deploy/project.yaml or keep at least one node"
                    .to_string(),
            ));
        }
        non_canonical.insert(provider.clone(), check_canonical_form(&tags, &raw));
        owned_tags.push(tags);
    }
    // The reviewed owned tag content against what this checkout holds, the
    // same way (TAGDEP-10): nothing has been contacted yet.
    if let Some(expected) = &input.tags_sha256 {
        if owned_tags.is_empty() {
            return Err(CoreError::InvalidInput {
                reason: "--tags-sha256 is given, but deploy/project.yaml lists no \
                         tag_providers: there is no owned tag content to compare it with"
                    .to_string(),
            });
        }
        let actual = repo_tags_sha256(&owned_tags);
        if *expected != actual {
            return Err(CoreError::ProjectHashMismatch {
                project: file.project.clone(),
                hash: "tags_sha256",
                expected: expected.clone(),
                actual,
            });
        }
    }
    Ok(LoadedProject {
        file,
        meta,
        target_file,
        target,
        artifact,
        release,
        owned_tags,
        provider_settings,
        tag_exclusions,
        ownership_warnings,
        non_canonical,
    })
}

/// The gateway-config manifest at `rel` below the targets root, strictly
/// parsed; `None` when it does not exist. A manifest that exists but is not
/// a regular file or does not parse is its `deploy_tree_invalid`.
fn read_gateway_manifest(
    targets: &std::path::Path,
    rel: &str,
) -> Result<Option<Manifest>, CoreError> {
    match std::fs::symlink_metadata(targets.join(rel)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        _ => {}
    }
    let bytes = read_repo_file(targets, rel, None)?;
    Manifest::parse(rel, &bytes).map(Some)
}

/// Refuse, offline, a project or gateway name the ssh release-record writer
/// cannot carry (WR-03). The record path `ign-deploy/<gateway>/projects/
/// <project>/release.json` reaches sftp on a batch line, and sftp globs: a
/// name with `[ ] { } * ?` (or a quote, backslash or control character) is
/// refused by the writer, which would otherwise happen only after the import,
/// leaving a deployed project that no record can ever be written for. A
/// `local` node writes with plain file calls and takes any name that passed
/// the component checks.
///
/// The tag records (26-06) sit at `ign-deploy/<gateway>/tag-providers/
/// <provider>/record.json`, and their reader and writer refuse the names
/// [`tag_record_names_problem`] refuses on that node. A provider name they
/// would refuse is refused here, so it never fails at the host read.
fn record_names_fit_the_host(
    project: &str,
    tag_providers: &[String],
    gateway: &str,
    target_file: &str,
    master: &NodeTarget,
) -> Result<(), CoreError> {
    if master.ssh.is_some() {
        if let Err(why) = safe_remote_path(project) {
            return Err(CoreError::deploy_tree_invalid(
                PROJECT_FILE_REL,
                None,
                None,
                format!(
                    "project: the name cannot be used for the release record on an ssh node ({why})"
                ),
            ));
        }
        if let Err(why) = safe_remote_path(gateway) {
            return Err(CoreError::deploy_tree_invalid(
                target_file,
                None,
                None,
                format!(
                    "gateway: the name cannot be used for the release record on an ssh node ({why})"
                ),
            ));
        }
    }
    let findings: Vec<crate::error::DeployFinding> = tag_providers
        .iter()
        .enumerate()
        .filter_map(|(i, provider)| {
            tag_record_names_problem(gateway, provider, master).map(|why| {
                crate::error::DeployFinding {
                    file: PROJECT_FILE_REL.to_string(),
                    line: None,
                    column: None,
                    message: format!(
                        "tag_providers[{i}]: the name cannot be used for the tag record on \
                         the master node ({why})"
                    ),
                }
            })
        })
        .collect();
    if !findings.is_empty() {
        return Err(CoreError::deploy_findings(findings));
    }
    Ok(())
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
    /// A project that owns tag providers (Phase 26, D-07): the tags and the
    /// project were deployed in one run and both verified.
    Applied,
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
            ProjectOutcome::Applied => "applied",
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
    /// Tag paths the project uses that the target lacks, byte-sorted by path
    /// and capped. Null when nothing could be read, and when a file over the
    /// read limit was scanned in part and nothing was missing in the part
    /// read (an empty list would read as "no missing tags"). A warning only:
    /// it changes nothing else.
    pub missing_tags: Option<Vec<MissingTag>>,
    /// Full count of missing tag paths; null with `missing_tags`.
    pub missing_tags_count: Option<usize>,
    /// Providers the project references that the target does not list,
    /// byte-sorted and capped; null when the provider list could not be read,
    /// and when a file over the read limit was scanned in part and no unknown
    /// provider was found in the part read. Never counted in `missing_tags`.
    pub missing_tags_unknown_providers: Option<Vec<UnknownProvider>>,
    /// Full count of unknown providers; null with the list.
    pub missing_tags_unknown_providers_count: Option<usize>,
    /// The reads that failed and why; null when every read was made.
    pub missing_tags_unavailable: Option<Vec<TagsUnavailable>>,
    /// What the check does not cover ([`MISSING_TAGS_LIMIT`]).
    pub missing_tags_limit: &'static str,
    /// The owned tag providers' plan (Phase 26); null for a project whose
    /// `deploy/project.yaml` lists no `tag_providers`. Boxed: the report is
    /// large and absent for most projects.
    pub tags: Option<Box<TagsReport>>,
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

/// A list cut to [`LIST_CAP`] with its full count.
fn cap_list<T>(mut items: Vec<T>) -> (Vec<T>, usize) {
    let count = items.len();
    items.truncate(LIST_CAP);
    (items, count)
}

/// What [`check_tag_references`] learned. The lists are whole and sorted; the
/// caller caps them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagCheck {
    /// Missing tag paths, byte-sorted by path; `None` when nothing could be
    /// read or no path could be resolved (every path of every provider read
    /// reached a UDT type its export does not resolve), and when a file over
    /// the read limit was scanned in part and the part read held none.
    pub missing: Option<Vec<MissingTag>>,
    /// Providers the project references that the gateway does not list;
    /// `None` when the provider list could not be read, and when a file over
    /// the read limit was scanned in part and the part read named none.
    pub unknown_providers: Option<Vec<UnknownProvider>>,
    /// The reads that failed, the files read only in part, and per provider
    /// the paths a UDT type its export does not resolve left unchecked.
    pub unavailable: Vec<TagsUnavailable>,
}

/// Fixed wording plus the error slug. Never a response body.
fn unavailable_reason(what: &str, err: &CoreError) -> String {
    format!("{what} ({})", err.code())
}

/// The most unresolved paths one reason names before it counts the rest.
const UNRESOLVED_PATHS_SHOWN: usize = 3;

/// Fixed wording plus the paths a UDT type left unresolved, byte-sorted: the
/// first [`UNRESOLVED_PATHS_SHOWN`] named and the rest counted. Every path
/// passed `parse_literal`, which refuses control and format characters, so
/// the reason is one printable line.
fn unresolved_reason(paths: &std::collections::BTreeSet<String>) -> String {
    let shown: Vec<&str> = paths
        .iter()
        .take(UNRESOLVED_PATHS_SHOWN)
        .map(String::as_str)
        .collect();
    let rest = paths.len() - shown.len();
    let mut reason = format!(
        "the provider's tag export does not resolve the UDT type of {} tag path(s): {}",
        paths.len(),
        shown.join(", ")
    );
    if rest > 0 {
        reason.push_str(&format!(" and {rest} more"));
    }
    reason
}

/// The part of `path` before a property suffix (`A/B.value` is `A/B`), or
/// `None` when the last segment holds no property.
fn strip_property(path: &str) -> Option<&str> {
    let start = path.rfind('/').map_or(0, |at| at + 1);
    let dot = path[start..].find('.')?;
    (dot > 0).then(|| &path[..start + dot])
}

/// Whether `name` could be a gateway tag provider: it holds a letter, only
/// letters, digits, `_`, `-` and inner spaces, and is not `client`, Vision's
/// project-scoped client tag prefix. Text such as `[.]`, `[~]`, `[0].x`,
/// `[%s]` or `[a, b]` is bracket text that only looks like a provider.
fn provider_shaped(name: &str) -> bool {
    name.chars().any(char::is_alphabetic)
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ' '))
        && !name.starts_with(' ')
        && !name.ends_with(' ')
        && !name.eq_ignore_ascii_case("client")
}

/// A provider the gateway does not list, under construction.
struct UnknownAcc {
    provider: String,
    paths: std::collections::BTreeSet<String>,
    files: std::collections::BTreeSet<String>,
}

/// A missing path under construction.
struct MissingAcc {
    path: String,
    files: std::collections::BTreeSet<String>,
}

/// Which tag paths the project in `zip` uses that the gateway lacks.
///
/// Infallible by signature: no tag failure can fail the check. It scans the
/// artifact; with no reference outside the excluded providers it makes no
/// gateway call; otherwise it lists the providers once and exports each
/// referenced, listed, non-excluded provider once, one at a time, dropping
/// the tree before the next.
///
/// A file over the read limit was scanned in part. Whichever way the check
/// returns, one entry naming those files is added to `unavailable`, and an
/// empty `missing` or `unknown_providers` becomes `None`: an empty list would
/// read as "no missing tags" when a reference past the limit was never seen.
pub async fn check_tag_references(
    zip: &[u8],
    excluded: &[String],
    gateway: &dyn ProjectGateway,
) -> TagCheck {
    check_tag_references_with(zip, excluded, gateway, &TagReads::default()).await
}

/// Gateway tag reads a run already made, so the missing-tag check does not
/// make them again: an owned provider is exported once per plan (Phase 26).
#[derive(Debug, Default)]
pub struct TagReads {
    /// The provider list, when it was read.
    pub names: Option<Vec<String>>,
    /// Exports already read, keyed by the provider name the gateway lists.
    pub exports: std::collections::BTreeMap<String, Vec<u8>>,
}

/// [`check_tag_references`] reusing `reads` instead of asking the gateway
/// again.
pub async fn check_tag_references_with(
    zip: &[u8],
    excluded: &[String],
    gateway: &dyn ProjectGateway,
    reads: &TagReads,
) -> TagCheck {
    let refs = match scan_artifact(zip) {
        Ok(refs) => refs,
        Err(err) => {
            return TagCheck {
                missing: None,
                unknown_providers: None,
                unavailable: vec![TagsUnavailable {
                    provider: None,
                    reason: unavailable_reason("the project files could not be scanned", &err),
                }],
            };
        }
    };
    let mut check = check_refs(&refs, excluded, gateway, reads).await;
    if let Some(reason) = refs.cut_reason() {
        check.unavailable.insert(
            0,
            TagsUnavailable {
                provider: None,
                reason,
            },
        );
        check.missing = check.missing.filter(|list| !list.is_empty());
        check.unknown_providers = check.unknown_providers.filter(|list| !list.is_empty());
    }
    check
}

/// [`check_tag_references`] over the references already scanned.
async fn check_refs(
    refs: &TagRefs,
    excluded: &[String],
    gateway: &dyn ProjectGateway,
    reads: &TagReads,
) -> TagCheck {
    // A reference to an excluded provider is never read and never reported,
    // so it is dropped before any request: a project whose references all
    // name excluded providers makes no tag request at all.
    let mut by_provider: std::collections::BTreeMap<String, Vec<&TagRef>> =
        std::collections::BTreeMap::new();
    for tag in refs.iter() {
        let fold = tag.provider.to_ascii_lowercase();
        if exclude::is_excluded(&fold, excluded) {
            continue;
        }
        by_provider.entry(fold).or_default().push(tag);
    }
    if by_provider.is_empty() {
        return TagCheck {
            missing: Some(Vec::new()),
            unknown_providers: Some(Vec::new()),
            unavailable: Vec::new(),
        };
    }
    let listed_names = match &reads.names {
        Some(names) => Ok(names.clone()),
        None => gateway.tag_provider_names().await,
    };
    let names = match listed_names {
        Ok(names) => names,
        Err(err) => {
            return TagCheck {
                missing: None,
                unknown_providers: None,
                unavailable: vec![TagsUnavailable {
                    provider: None,
                    reason: unavailable_reason(
                        "the gateway's tag providers could not be listed",
                        &err,
                    ),
                }],
            };
        }
    };
    let mut listed: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for name in names {
        let fold = name.to_ascii_lowercase();
        match listed.get(&fold) {
            Some(kept) if kept.as_bytes() <= name.as_bytes() => {}
            _ => {
                listed.insert(fold, name);
            }
        }
    }

    let mut found: std::collections::BTreeMap<(String, String), MissingAcc> =
        std::collections::BTreeMap::new();
    let mut unknown: std::collections::BTreeMap<String, UnknownAcc> =
        std::collections::BTreeMap::new();
    let mut unavailable = Vec::new();
    // `checked` counts the references a tree answered (found or missing).
    let (mut needed, mut checked) = (0usize, 0usize);
    for (fold, tags) in &by_provider {
        let Some(provider) = listed.get(fold) else {
            // A provider the target lacks must not pass as clean. It costs no
            // export; bracket text that cannot be a provider name is dropped.
            if provider_shaped(fold) {
                let acc = unknown.entry(fold.clone()).or_insert_with(|| UnknownAcc {
                    provider: tags[0].provider.clone(),
                    paths: std::collections::BTreeSet::new(),
                    files: std::collections::BTreeSet::new(),
                });
                for tag in tags {
                    if tag.provider < acc.provider {
                        acc.provider = tag.provider.clone();
                    }
                    let path = strip_property(&tag.path).unwrap_or(&tag.path);
                    acc.paths.insert(path.to_lowercase());
                    acc.files.extend(tag.files.iter().cloned());
                    while acc.files.len() > FILES_PER_ENTRY {
                        acc.files.pop_last();
                    }
                }
            }
            continue;
        };
        needed += 1;
        let read = match reads.exports.get(provider) {
            Some(bytes) => Ok(bytes.clone()),
            None => gateway.tag_export(provider).await,
        };
        let bytes = match read {
            Ok(bytes) => bytes,
            Err(err) => {
                unavailable.push(TagsUnavailable {
                    provider: Some(provider.clone()),
                    reason: unavailable_reason("the provider's tag export could not be read", &err),
                });
                continue;
            }
        };
        let tree = match TagTree::parse(&bytes, provider) {
            Ok(tree) => tree,
            Err(err) => {
                unavailable.push(TagsUnavailable {
                    provider: Some(provider.clone()),
                    reason: unavailable_reason("the provider's tag export is not a tag tree", &err),
                });
                continue;
            }
        };
        drop(bytes);
        // Paths whose lookup reached a UDT type the export does not resolve:
        // neither missing nor clean, so the provider is said as not checked.
        let mut unresolved: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for tag in tags {
            let full = tree.lookup(&tag.path);
            let answer = match (full, strip_property(&tag.path)) {
                (Lookup::Found, _) => Lookup::Found,
                (other, None) => other,
                (full, Some(stripped)) => match (full, tree.lookup(stripped)) {
                    (_, Lookup::Found) => Lookup::Found,
                    (Lookup::Missing, Lookup::Missing) => Lookup::Missing,
                    _ => Lookup::Unknown,
                },
            };
            let reported = match answer {
                Lookup::Found => {
                    checked += 1;
                    continue;
                }
                Lookup::Unknown => {
                    unresolved.insert(format!("[{}]{}", tag.provider, tag.path));
                    continue;
                }
                Lookup::Missing => {
                    checked += 1;
                    strip_property(&tag.path).unwrap_or(&tag.path)
                }
            };
            let display = format!("[{}]{}", tag.provider, reported);
            let acc = found
                .entry((fold.clone(), reported.to_lowercase()))
                .or_insert_with(|| MissingAcc {
                    path: display.clone(),
                    files: std::collections::BTreeSet::new(),
                });
            if display < acc.path {
                acc.path = display;
            }
            acc.files.extend(tag.files.iter().cloned());
            while acc.files.len() > FILES_PER_ENTRY {
                acc.files.pop_last();
            }
        }
        if !unresolved.is_empty() {
            unavailable.push(TagsUnavailable {
                provider: Some(provider.clone()),
                reason: unresolved_reason(&unresolved),
            });
        }
    }
    let missing = if needed > 0 && checked == 0 {
        None
    } else {
        let mut list: Vec<MissingTag> = found
            .into_values()
            .map(|acc| MissingTag {
                path: acc.path,
                files: acc.files.into_iter().collect(),
            })
            .collect();
        list.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
        Some(list)
    };
    let mut unknown_providers: Vec<UnknownProvider> = unknown
        .into_values()
        .map(|acc| UnknownProvider {
            provider: acc.provider,
            path_count: acc.paths.len(),
            files: acc.files.into_iter().collect(),
        })
        .collect();
    unknown_providers.sort_by(|a, b| a.provider.as_bytes().cmp(b.provider.as_bytes()));
    TagCheck {
        missing,
        unknown_providers: Some(unknown_providers),
        unavailable,
    }
}

/// The most provider names one warning spells out.
const WARNING_PROVIDERS_SHOWN: usize = 5;

/// The warnings for what [`check_tag_references`] found. A warning only:
/// none of them changes `ok`, `slug`, `outcome` or the exit code.
fn tag_warnings(project: &str, gateway: &str, check: &TagCheck) -> Vec<String> {
    let mut warnings = Vec::new();
    let missing = check.missing.as_ref().map_or(0, Vec::len);
    if missing > 0 {
        warnings.push(format!(
            "project {project}: {missing} tag path(s) the project uses are not on gateway {gateway}; \
             ign deploy projects does not deploy tags, and paths built at runtime aren't checked"
        ));
    }
    if let Some(unknown) = check.unknown_providers.as_ref().filter(|u| !u.is_empty()) {
        let shown: Vec<&str> = unknown
            .iter()
            .take(WARNING_PROVIDERS_SHOWN)
            .map(|u| u.provider.as_str())
            .collect();
        let rest = unknown.len().saturating_sub(WARNING_PROVIDERS_SHOWN);
        warnings.push(format!(
            "project {project}: tag paths the project uses name {} provider(s) that gateway {gateway} does not have: {}{}; \
             ign deploy projects does not deploy tags, and paths built at runtime aren't checked",
            unknown.len(),
            shown.join(", "),
            if rest > 0 {
                format!(" and {rest} more")
            } else {
                String::new()
            }
        ));
    }
    for entry in &check.unavailable {
        warnings.push(format!(
            "project {project}: tag paths were not checked for {}: {}",
            entry.provider.as_deref().unwrap_or("every provider"),
            entry.reason
        ));
    }
    warnings
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
        /// This project's tag record of each owned provider the plan was
        /// drawn against, keyed by the provider's name (26-07); empty for a
        /// project that owns none or a first deploy. The run re-plans with
        /// them.
        tag_records: std::collections::BTreeMap<String, TagRecord>,
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
    // What the project uses that the gateway lacks. Said only when there is
    // something to say, so a plan with no tag finding reads as it always did.
    let missing = plan.missing_tags_count.unwrap_or(0);
    if missing > 0 {
        lines.push(format!(
            "{missing} tag path(s) the project uses are not on gateway {} (this verb does not deploy tags):",
            plan.gateway
        ));
        for tag in plan.missing_tags.iter().flatten().take(PROSE_REMOVED_SHOWN) {
            lines.push(format!("  {}", tag.path));
        }
        if missing > PROSE_REMOVED_SHOWN {
            lines.push(format!("  ... and {} more", missing - PROSE_REMOVED_SHOWN));
        }
    }
    let unknown = plan.missing_tags_unknown_providers_count.unwrap_or(0);
    if unknown > 0 {
        lines.push(format!(
            "{unknown} provider(s) the project uses are not on gateway {}:",
            plan.gateway
        ));
        for entry in plan
            .missing_tags_unknown_providers
            .iter()
            .flatten()
            .take(PROSE_REMOVED_SHOWN)
        {
            lines.push(format!("  {}", entry.provider));
        }
        if unknown > PROSE_REMOVED_SHOWN {
            lines.push(format!("  ... and {} more", unknown - PROSE_REMOVED_SHOWN));
        }
    }
    for entry in plan.missing_tags_unavailable.iter().flatten() {
        lines.push(format!(
            "tags NOT CHECKED for {}: {}",
            entry.provider.as_deref().unwrap_or("every provider"),
            entry.reason
        ));
    }
    if let Some(tags) = &plan.tags {
        lines.push(format!(
            "owned tag providers ({}), deployed first, then the project, in one run:",
            tags.providers.len()
        ));
        for p in &tags.providers {
            lines.push(format!(
                "  {}: {} added, {} changed, {} same, {} live-only (kept)",
                p.provider, p.added, p.changed, p.same, p.drift
            ));
        }
        for p in &tags.providers {
            let extra = [
                (p.kept_values, "kept values (live value kept)"),
                (p.removed, "removed"),
                (p.already_absent, "already absent"),
                (p.recreated, "recreated"),
                (p.non_canonical, "non-canonical file finding(s)"),
            ];
            let said: Vec<String> = extra
                .iter()
                .filter(|(n, _)| *n > 0)
                .map(|(n, what)| format!("{n} {what}"))
                .collect();
            if !said.is_empty() {
                lines.push(format!("  {}: {}", p.provider, said.join(", ")));
            }
            if p.first_change {
                lines.push(format!(
                    "  {}: first deploy of the provider by this tool (--allow-first-change)",
                    p.provider
                ));
            } else if p.record_only {
                lines.push(format!(
                    "  {}: first deploy of the provider by this tool, every node equal",
                    p.provider
                ));
            }
        }
        let list = |lines: &mut Vec<String>, verb: &str, paths: &[String], count: usize| {
            for path in paths.iter().take(PROSE_REMOVED_SHOWN) {
                lines.push(format!("  {verb} {path}"));
            }
            if count > PROSE_REMOVED_SHOWN {
                lines.push(format!(
                    "  ... and {} more {verb}",
                    count - PROSE_REMOVED_SHOWN
                ));
            }
        };
        list(&mut lines, "add", &tags.added, tags.added_count);
        let changed: Vec<String> = tags
            .changed
            .iter()
            .map(|c| {
                format!(
                    "{} ({}){}",
                    c.path,
                    c.properties.join(", "),
                    if c.restart_likely {
                        ", restart likely"
                    } else {
                        ""
                    }
                )
            })
            .collect();
        list(&mut lines, "change", &changed, tags.changed_count);
        // A removal is refused by the run in this release (`tag_removal`).
        list(
            &mut lines,
            "refused removal",
            &tags.removed,
            tags.removed_count,
        );
        list(
            &mut lines,
            "kept value",
            &tags.kept_values,
            tags.kept_values_count,
        );
        for warning in &tags.warnings {
            lines.push(format!("  warning: {warning}"));
        }
        lines.push(format!("tags_sha256 {}", tags.tags_sha256));
        lines.push(format!("tags plan_sha256 {}", tags.plan_sha256));
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

    // This project's tag record of every owned provider (26-07, D-02, D-05),
    // read from the host before any gateway read: another project's record
    // claims the provider, and a record that cannot be trusted refuses.
    let tag_records = read_tag_records(loaded, surfaces.host).await?;

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
    // The owned tag providers (Phase 26): each is exported once and planned
    // against the repo. The exports are handed to the missing-tag check so it
    // does not read them again.
    let (tag_plan, tag_reads, provider_settings) =
        match plan_owned_tags(loaded, gateway, &tag_records).await? {
            Some(owned) => (Some(owned.plan), owned.reads, owned.settings),
            None => (None, TagReads::default(), Vec::new()),
        };
    // The tag gates and the secret rule, before the plan is reported and
    // long before any write (26-07).
    let tag_secret_warnings = match &tag_plan {
        Some(plan) => tag_gates(loaded, input, plan)?,
        None => Vec::new(),
    };
    // The tag groups (26-10), the mode-collection refusal on a pair
    // (R3-MODE) and the group check of every node the run would send
    // (R3-GRP), all before any write and in `--check` too.
    let groups = match &tag_plan {
        Some(plan) => plan_owned_groups(loaded, surfaces, plan, &tag_records).await?,
        None => Vec::new(),
    };
    // What the project uses that the target lacks is reported here, after
    // the gates that can refuse and before the plan is built. A warning only:
    // nothing below depends on it.
    let tag_check = check_tag_references_with(
        &loaded.artifact.zip,
        &loaded.tag_exclusions,
        gateway,
        &tag_reads,
    )
    .await;
    drop(tag_reads);
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
    warnings.extend(tag_warnings(project, gateway_name, &tag_check));
    let (members, removed) = member_delta(artifact, live.as_ref());
    let (removed, removed_count) = capped(removed);
    let (excluded, excluded_count) = capped(loaded.artifact.excluded.clone());
    let (unmanaged, unmanaged_count) = capped(others);
    let (missing_tags, missing_tags_count) = match tag_check.missing {
        Some(list) => {
            let (list, count) = cap_list(list);
            (Some(list), Some(count))
        }
        None => (None, None),
    };
    let (missing_tags_unknown_providers, missing_tags_unknown_providers_count) =
        match tag_check.unknown_providers {
            Some(list) => {
                let (list, count) = cap_list(list);
                (Some(list), Some(count))
            }
            None => (None, None),
        };
    let missing_tags_unavailable =
        (!tag_check.unavailable.is_empty()).then_some(tag_check.unavailable);
    let settings_change = provider_settings
        .iter()
        .any(|p| p.action != ProviderSettingsAction::Same);
    let would_change = action.would_change()
        || tag_plan.as_ref().is_some_and(TagPlan::would_change)
        || settings_change
        || groups.iter().any(GroupPlan::would_change);
    let tags = tag_plan.map(|plan| {
        let mut report = TagsReport::from_plan(&plan, &loaded.non_canonical, LIST_CAP);
        report.warnings.clone_from(&loaded.ownership_warnings);
        report.warnings.extend(tag_secret_warnings);
        report.provider_settings = provider_settings;
        report.groups = groups;
        Box::new(report)
    });
    let outcome = if !would_change {
        ProjectOutcome::Skipped
    } else if tags.is_some() {
        ProjectOutcome::Applied
    } else {
        match action {
            ProjectAction::Create => ProjectOutcome::Created,
            ProjectAction::Skip => ProjectOutcome::Skipped,
            ProjectAction::Import => ProjectOutcome::Imported,
            ProjectAction::Record => ProjectOutcome::Recorded,
        }
    };

    let plan = DeployProjectsResult {
        env: input.env.clone(),
        gateway: loaded.target.gateway.clone(),
        project: project.clone(),
        release: input.release.clone(),
        check: false,
        action,
        would_change,
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
        missing_tags,
        missing_tags_count,
        missing_tags_unknown_providers,
        missing_tags_unknown_providers_count,
        missing_tags_unavailable,
        missing_tags_limit: MISSING_TAGS_LIMIT,
        tags,
        outcome,
        ok: true,
        slug: None,
    };
    Ok(if !would_change {
        ProjectsPrepared::Unchanged(plan)
    } else {
        ProjectsPrepared::Ready {
            prose: plan_prose(&plan),
            plan,
            tag_records,
        }
    })
}

/// What [`plan_owned_tags`] read and planned.
struct OwnedTags {
    plan: TagPlan,
    reads: TagReads,
    /// Each owned provider's settings plan, byte-sorted by provider (26-10).
    settings: Vec<ProviderSettingsPlan>,
}

/// Export and plan every owned tag provider (Phase 26); `None` for a project
/// that owns none. Reads only: the provider list once, each owned provider's
/// resource record once (26-10) and each listed owned provider's export once.
/// A provider the gateway does not list is planned against nothing and its
/// settings plan `create` (26-10); one it lists only under a name that
/// differs in letter case refuses `deploy_tree_invalid` naming
/// `deploy/project.yaml`, since a second provider beside it is never what the
/// repo meant.
async fn plan_owned_tags(
    loaded: &LoadedProject,
    gateway: &dyn ProjectGateway,
    records: &std::collections::BTreeMap<String, TagRecord>,
) -> Result<Option<OwnedTags>, CoreError> {
    if loaded.owned_tags.is_empty() {
        return Ok(None);
    }
    let names = gateway.tag_provider_names().await?;
    let mut findings = Vec::new();
    for (i, provider) in loaded.owned_tags.iter().enumerate() {
        if names.contains(&provider.name)
            || !names.iter().any(|n| n.eq_ignore_ascii_case(&provider.name))
        {
            continue;
        }
        findings.push(crate::error::DeployFinding {
            file: PROJECT_FILE_REL.to_string(),
            line: None,
            column: None,
            message: format!(
                "tag_providers[{i}]: gateway {} has no tag provider named {} (it lists one whose \
                 name differs only in letter case; the names must match exactly)",
                loaded.target.gateway, provider.name
            ),
        });
    }
    if !findings.is_empty() {
        return Err(CoreError::deploy_findings(findings));
    }
    let mut settings = Vec::new();
    for provider in sorted(&loaded.owned_tags) {
        let listed = names.contains(&provider.name);
        let record = if listed {
            let record = gateway.tag_provider_record(&provider.name).await?;
            if record.is_none() {
                return Err(CoreError::Internal(format!(
                    "the gateway lists tag provider {} but has no record of it",
                    provider.name
                )));
            }
            record
        } else {
            None
        };
        settings.push(plan_provider_settings(
            &provider.name,
            provider_settings_of(loaded, &provider.name)?,
            record.as_ref(),
        ));
    }
    let mut reads = TagReads {
        names: Some(names),
        exports: std::collections::BTreeMap::new(),
    };
    let mut live = std::collections::BTreeMap::new();
    for provider in &loaded.owned_tags {
        if !reads
            .names
            .as_ref()
            .is_some_and(|names| names.contains(&provider.name))
        {
            continue;
        }
        let bytes = gateway.tag_export(&provider.name).await?;
        live.insert(
            provider.name.clone(),
            flatten_export(&bytes, &provider.name)?,
        );
        reads.exports.insert(provider.name.clone(), bytes);
    }
    Ok(Some(OwnedTags {
        plan: plan_tags(&loaded.owned_tags, &live, records),
        reads,
        settings,
    }))
}

/// The master's tree reader, or an internal error naming what needed it.
fn need_tree<'a>(
    surfaces: &ProjectsSurfaces<'a>,
    what: &str,
) -> Result<&'a dyn TreeReader, CoreError> {
    surfaces.tree.ok_or_else(|| {
        CoreError::Internal(format!(
            "{what} needs the master's host reader, and none was supplied"
        ))
    })
}

/// How many unit paths a refusal names.
const UNITS_SHOWN: usize = 10;

/// R3-MODE (C8, F7): on a redundant pair, a `tag-definition` or
/// `tag-type-definition` unit (or folder) of an owned provider in the mode
/// collection would take the import into that collection, which the pair
/// cannot sync. Refused before any write, naming the paths. Names only: no
/// byte is read.
async fn refuse_mode_tag_units(
    loaded: &LoadedProject,
    reader: &dyn TreeReader,
) -> Result<(), CoreError> {
    let mode = &loaded.target.mode;
    let dirs: Vec<(String, String)> = crate::deploy::layout::PAIR_UNSYNCED_TAG_TYPES
        .iter()
        .map(|ty| ("ignition".to_string(), (*ty).to_string()))
        .collect();
    let listed = reader.list_tree(mode, &dirs).await?;
    let mut found: Vec<String> = Vec::new();
    for listing in &listed {
        for provider in &loaded.file.tag_providers {
            let root = format!("{}/{}/{provider}", listing.module, listing.ty);
            let below = format!("{root}/");
            found.extend(
                listing
                    .entries
                    .iter()
                    .map(|e| e.path.as_str())
                    .filter(|p| *p == root || p.starts_with(&below))
                    .map(str::to_string),
            );
        }
    }
    if found.is_empty() {
        return Ok(());
    }
    found.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    let count = found.len();
    let shown: Vec<&str> = found.iter().take(UNITS_SHOWN).map(String::as_str).collect();
    Err(CoreError::deploy_tree_invalid(
        &loaded.target_file,
        None,
        None,
        format!(
            "nodes.master's {mode} collection holds {count} tag definition path(s) of an owned \
             provider: {}{}. On a redundant pair the tag import would be written into the {mode} \
             collection (C8), which stops the backup's configuration sync; move these units to \
             core with a gateway-config deploy (ign deploy apply) first",
            shown.join(", "),
            if count > shown.len() {
                format!(" and {} more", count - shown.len())
            } else {
                String::new()
            }
        ),
    ))
}

/// The tag-group inputs of one owned provider.
struct GroupInputs<'a> {
    provider: &'a NativeProvider,
    summary: Option<std::collections::BTreeMap<String, GroupSummary>>,
    /// The nodes the run sends: added and changed.
    sent: Vec<&'a NativeNode>,
}

/// The tag-group inputs of every owned provider, in plan order.
fn group_inputs<'a>(
    loaded: &'a LoadedProject,
    plan: &TagPlan,
) -> Result<Vec<GroupInputs<'a>>, CoreError> {
    let mut out = Vec::new();
    for (provider_plan, provider) in plan.providers.iter().zip(sorted(&loaded.owned_tags)) {
        let summary = match &provider.groups {
            Some(value) => Some(parse_group_value(
                &format!("tags/{}/{TAG_GROUPS_FILE}", provider.name),
                value,
            )?),
            None => None,
        };
        let sending: std::collections::BTreeSet<&str> = provider_plan
            .added
            .iter()
            .map(String::as_str)
            .chain(provider_plan.changed.iter().map(|c| c.path.as_str()))
            .collect();
        let sent = provider
            .nodes
            .values()
            .filter(|n| sending.contains(n.path.as_str()))
            .collect();
        out.push(GroupInputs {
            provider,
            summary,
            sent,
        });
    }
    Ok(out)
}

/// The group plans of `inputs` against `observed` (the providers' units),
/// with this project's tag records. Only a provider whose repo holds a
/// summary is planned; without one the project does not manage the
/// provider's groups, and nothing is written or deleted.
fn group_plans(
    inputs: &[GroupInputs<'_>],
    observed: &crate::deploy::transport::ObservedFiles,
    records: &std::collections::BTreeMap<String, TagRecord>,
) -> Result<Vec<GroupPlan>, CoreError> {
    let mut plans = Vec::new();
    for input in inputs {
        let Some(summary) = &input.summary else {
            continue;
        };
        let name = &input.provider.name;
        let live = LiveGroups::from_observed(name, observed);
        let recorded = records
            .get(name)
            .map(|r| r.groups.clone())
            .unwrap_or_default();
        let referenced = referenced_groups(input.provider.nodes.values());
        plans.push(plan_groups(name, summary, &live, &recorded, &referenced)?);
    }
    Ok(plans)
}

/// The providers of `inputs` whose units must be read: those with a summary
/// and those whose sent nodes name a group (R3-GRP).
fn providers_needing_groups(inputs: &[GroupInputs<'_>]) -> Vec<String> {
    inputs
        .iter()
        .filter(|input| {
            input.summary.is_some() || !referenced_groups(input.sent.iter().copied()).is_empty()
        })
        .map(|input| input.provider.name.clone())
        .collect()
}

/// Read `providers`' tag-group units from the master; a unit the host holds
/// but cannot serve refuses.
async fn read_groups(
    reader: &dyn TreeReader,
    mode: &str,
    providers: &[String],
) -> Result<crate::deploy::transport::ObservedFiles, CoreError> {
    let observed = reader.read_tag_groups(mode, providers).await?;
    if let Some(first) = observed.unreadable.first() {
        return Err(CoreError::TransportFailed {
            operation: "read".to_string(),
            node: "nodes.master".to_string(),
            detail: format!(
                "{} tag-group file(s) cannot be read, the first {}/{}",
                observed.unreadable.len(),
                first.collection,
                first.path
            ),
        });
    }
    Ok(observed)
}

/// Plan the owned providers' tag groups (26-10) and run the two
/// prepare-time refusals: R3-MODE on a pair, then R3-GRP. Reads only. On a
/// standalone target, a project whose repo holds no summary and whose sent
/// nodes name no group reads nothing here.
async fn plan_owned_groups(
    loaded: &LoadedProject,
    surfaces: &ProjectsSurfaces<'_>,
    plan: &TagPlan,
    records: &std::collections::BTreeMap<String, TagRecord>,
) -> Result<Vec<GroupPlan>, CoreError> {
    // C8 holds (A4): on a standalone target an import edits the effective
    // (mode) copy, so nothing is refused there.
    if loaded.target.nodes.backup.is_some() {
        refuse_mode_tag_units(
            loaded,
            need_tree(surfaces, "the pair's mode-collection check")?,
        )
        .await?;
    }
    let inputs = group_inputs(loaded, plan)?;
    let providers = providers_needing_groups(&inputs);
    if providers.is_empty() {
        return Ok(Vec::new());
    }
    let reader = need_tree(surfaces, "the tag-group read")?;
    let observed = read_groups(reader, &loaded.target.mode, &providers).await?;
    let mut findings = Vec::new();
    let empty = std::collections::BTreeMap::new();
    for input in &inputs {
        let live = LiveGroups::from_observed(&input.provider.name, &observed).names();
        match check_group_refs(
            &input.provider.name,
            &input.sent,
            input.summary.as_ref().unwrap_or(&empty),
            &live,
        ) {
            Ok(()) => {}
            Err(CoreError::DeployTreeInvalid(details)) => findings.extend(details.findings),
            Err(other) => return Err(other),
        }
    }
    if !findings.is_empty() {
        return Err(CoreError::deploy_findings(findings));
    }
    group_plans(&inputs, &observed, records)
}

/// The repo settings of the owned provider `provider` (26-05 loads one per
/// owned provider, in the file's order).
fn provider_settings_of<'a>(
    loaded: &'a LoadedProject,
    provider: &str,
) -> Result<&'a ProviderSettings, CoreError> {
    loaded
        .file
        .tag_providers
        .iter()
        .position(|name| name == provider)
        .and_then(|i| loaded.provider_settings.get(i))
        .ok_or_else(|| {
            CoreError::Internal(format!(
                "no settings were loaded for owned tag provider {provider}"
            ))
        })
}

/// This project's tag record of each owned provider (26-06, 26-07), keyed by
/// the provider's name. Host reads only. A record absent from the host is a
/// first deploy of that provider and is left out. A record another project
/// wrote refuses `tag_provider_claimed` (D-05); a record that cannot be
/// trusted is `deploy_run_invalid` naming its location, never absent.
async fn read_tag_records(
    loaded: &LoadedProject,
    host: &dyn ProjectHost,
) -> Result<std::collections::BTreeMap<String, TagRecord>, CoreError> {
    let gateway = &loaded.target.gateway;
    let mut records = std::collections::BTreeMap::new();
    for provider in &loaded.file.tag_providers {
        let Some(bytes) = host.read_tag_record(gateway, provider).await? else {
            continue;
        };
        let location = tag_record_location(
            LedgerRoot::for_node(&loaded.target.nodes.master),
            gateway,
            provider,
        );
        let record = TagRecord::parse(&bytes, &location, gateway, provider)?;
        if record.project != loaded.file.project {
            return Err(CoreError::TagProviderClaimed {
                provider: provider.clone(),
                claimed_by: format!("project {} (host record)", record.project),
            });
        }
        records.insert(provider.clone(), record);
    }
    Ok(records)
}

/// The tag gates of [`deploy_projects_prepare`] (26-07), in this order, then
/// the D-14 secret rule. Returns the secret warnings for
/// `data.tags.warnings`.
///
/// 1. `provider_emptied`: a plan that removes every node a provider's record
///    lists; no flag passes it (Pitfall 2).
/// 2. `first_deploy`: an owned provider with no record whose plan adds or
///    changes a node, without `--allow-first-change` (the Phase 25 flag and
///    reason, D-12). Without a record nothing is ever removed.
/// 3. `embedded_secret`: a credential-like value the run would send
///    ([`scan_tag_node`]): added nodes are sent whole ([`Sends::All`]),
///    changed nodes without their runtime value ([`Sends::Config`]), and no
///    other repo node is sent ([`Sends::Nothing`]: warnings only).
///
/// The removal gate `tag_removal` is the run's ([`deploy_projects_run`]), so
/// `--check` lists the removals a run would refuse.
fn tag_gates(
    loaded: &LoadedProject,
    input: &ProjectsInput,
    plan: &TagPlan,
) -> Result<Vec<String>, CoreError> {
    let named = |pick: fn(&crate::deploy::tag_plan::ProviderPlan) -> bool| -> Vec<String> {
        plan.providers
            .iter()
            .filter(|p| pick(p))
            .map(|p| format!("tag provider {}", p.provider))
            .collect()
    };
    let emptied = named(|p| p.provider_emptied);
    if !emptied.is_empty() {
        return Err(CoreError::DestructiveChangeRefused {
            gate: "provider_emptied".to_string(),
            count: emptied.len(),
            keys: emptied,
        });
    }
    let first = named(|p| p.first_change);
    if !first.is_empty() && input.allow_first_change.is_none() {
        return Err(CoreError::DestructiveChangeRefused {
            gate: "first_deploy".to_string(),
            count: first.len(),
            keys: first,
        });
    }
    let mut refused = Vec::new();
    let mut warnings = Vec::new();
    for (provider_plan, provider) in plan.providers.iter().zip(sorted(&loaded.owned_tags)) {
        let added: std::collections::BTreeSet<&str> =
            provider_plan.added.iter().map(String::as_str).collect();
        let changed: std::collections::BTreeSet<&str> = provider_plan
            .changed
            .iter()
            .map(|c| c.path.as_str())
            .collect();
        for (path, node) in &provider.nodes {
            let sends = if added.contains(path.as_str()) {
                Sends::All
            } else if changed.contains(path.as_str()) {
                Sends::Config
            } else {
                Sends::Nothing
            };
            let scan = scan_tag_node(&node.file, path, &to_node(&node.json), sends);
            refused.extend(scan.refused);
            warnings.extend(scan.warnings);
        }
    }
    if !refused.is_empty() {
        return Err(CoreError::embedded_secret(refused, Vec::new()));
    }
    Ok(warnings)
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
    /// An owned tag provider's live export was saved into the run directory
    /// (`export/tags/<P>.json`) before any tag write (Phase 26).
    TagsPreImageExported {
        /// The provider.
        provider: String,
        /// Size of the saved export.
        bytes: usize,
    },
    /// A tag import request is about to be sent.
    TagImportSent {
        /// The provider.
        provider: String,
        /// The provider-relative parent path the nodes go below (empty at the
        /// provider root).
        path: String,
        /// The collision policy, as the gateway spells it.
        policy: &'static str,
        /// How many planned nodes the body carries.
        nodes: usize,
        /// Whether live values were carried into the body (R3-CARRY).
        carry: bool,
    },
    /// A fresh export of the provider was read right before a carried
    /// batch, and the live values of its nodes were put into the body
    /// (R3-CARRY, R3-RACE). Paths and counts only, never a value.
    TagValuesCarried {
        /// The provider.
        provider: String,
        /// The provider-relative parent path of the batch.
        path: String,
        /// How many nodes had their values carried.
        nodes: usize,
    },
    /// An owned provider's settings were created or modified and read back
    /// equal to the repo's (26-10).
    TagProviderSettingsApplied {
        /// The provider.
        provider: String,
        /// `create` or `modify`.
        action: &'static str,
    },
    /// The owned providers' tag-group units were committed, scanned and read
    /// back (26-10).
    TagGroupsWritten {
        /// The providers whose groups changed, byte-sorted.
        providers: Vec<String>,
        /// Unit files written.
        written: usize,
        /// Unit files deleted.
        deleted: usize,
    },
    /// The gateway answered a tag import.
    TagImportAnswered {
        /// The provider.
        provider: String,
        /// Whether the answer reported no failure and no bad quality code.
        clean: bool,
    },
    /// An owned provider was read back after the tag writes and compared
    /// with the repo and its pre-image.
    TagsVerified {
        /// The provider.
        provider: String,
        /// Whether every repo node equals the export in configuration and
        /// every drift node its pre-image.
        equal: bool,
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
    /// The tag record of an owned provider was written (26-08).
    TagRecordWritten {
        /// The provider.
        provider: String,
    },
    /// The tag record of an owned provider could not be written.
    TagRecordFailed {
        /// The provider.
        provider: String,
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
            ProjectsEvent::TagsPreImageExported { .. } => "tags_pre_image_exported",
            ProjectsEvent::TagImportSent { .. } => "tag_import_sent",
            ProjectsEvent::TagValuesCarried { .. } => "tag_values_carried",
            ProjectsEvent::TagProviderSettingsApplied { .. } => "tag_provider_settings_applied",
            ProjectsEvent::TagGroupsWritten { .. } => "tag_groups_written",
            ProjectsEvent::TagImportAnswered { .. } => "tag_import_answered",
            ProjectsEvent::TagsVerified { .. } => "tags_verified",
            ProjectsEvent::ImportSent { .. } => "import_sent",
            ProjectsEvent::ImportAnswered => "import_answered",
            ProjectsEvent::Verified { .. } => "verified",
            ProjectsEvent::RecordWritten => "record_written",
            ProjectsEvent::RecordFailed { .. } => "record_failed",
            ProjectsEvent::TagRecordWritten { .. } => "tag_record_written",
            ProjectsEvent::TagRecordFailed { .. } => "tag_record_failed",
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
            ProjectsEvent::TagsPreImageExported { provider, bytes } => {
                serde_json::json!({ "provider": provider, "bytes": bytes })
            }
            ProjectsEvent::TagImportSent {
                provider,
                path,
                policy,
                nodes,
                carry,
            } => serde_json::json!({
                "provider": provider, "path": path, "policy": policy, "nodes": nodes,
                "carry": carry
            }),
            ProjectsEvent::TagValuesCarried {
                provider,
                path,
                nodes,
            } => serde_json::json!({ "provider": provider, "path": path, "nodes": nodes }),
            ProjectsEvent::TagProviderSettingsApplied { provider, action } => {
                serde_json::json!({ "provider": provider, "action": action })
            }
            ProjectsEvent::TagGroupsWritten {
                providers,
                written,
                deleted,
            } => serde_json::json!({
                "providers": providers, "written": written, "deleted": deleted
            }),
            ProjectsEvent::TagImportAnswered { provider, clean } => {
                serde_json::json!({ "provider": provider, "clean": clean })
            }
            ProjectsEvent::TagsVerified { provider, equal } => {
                serde_json::json!({ "provider": provider, "equal": equal })
            }
            ProjectsEvent::ImportSent { overwrite } => {
                serde_json::json!({ "overwrite": overwrite })
            }
            ProjectsEvent::Verified { equal } => serde_json::json!({ "equal": equal }),
            ProjectsEvent::RecordFailed { code } => serde_json::json!({ "code": code }),
            ProjectsEvent::TagRecordWritten { provider } => {
                serde_json::json!({ "provider": provider })
            }
            ProjectsEvent::TagRecordFailed { provider, code } => {
                serde_json::json!({ "provider": provider, "code": code })
            }
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
    /// A tag record writer standing in for the one built from `master`
    /// (26-08). For the engine tests only: the CLI always passes `None`.
    #[doc(hidden)]
    pub tag_record_writer: Option<&'a dyn TagRecordWriter>,
    /// `--designers`, `--designers-timeout` and the poll interval (25-14).
    pub designers: DesignersPolicy,
    /// The longest the children sweep waits for the children's exports to
    /// settle (K7); the CLI passes [`CHILDREN_SETTLE_S`] seconds.
    pub children_settle: Duration,
    /// How often the sweep reads the children while it waits; the CLI passes
    /// [`CHILDREN_POLL_S`] seconds. The engine tests pass short values, never
    /// an environment variable.
    pub children_poll: Duration,
    /// The longest the tag verify waits for two reads of every owned
    /// provider to agree; the CLI passes [`TAG_VERIFY_SETTLE_S`] seconds.
    pub tag_verify_settle: Duration,
    /// How often the tag verify reads while it waits; the CLI passes
    /// [`TAG_VERIFY_POLL_S`] seconds.
    pub tag_verify_poll: Duration,
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
    let (plan, tag_records) = match prepared {
        ProjectsPrepared::Unchanged(result) => return Ok(result),
        ProjectsPrepared::Ready {
            plan, tag_records, ..
        } => (plan, tag_records),
    };
    // A removal (D-02) needs a tag delete, and TAGDEP-00 found none on the
    // REST surface (R3-DEL, C4/C5): in this release no flag passes it. It is
    // refused here, before anything is contacted or written; `--check` lists
    // the paths (`data.tags.removed`).
    if let Some(tags) = &plan.tags
        && tags.removed_count > 0
    {
        return Err(CoreError::DestructiveChangeRefused {
            gate: "tag_removal".to_string(),
            keys: tags.removed.clone(),
            count: tags.removed_count,
        });
    }
    // The record writers are built before anything is written, so a node
    // that cannot take a record refuses now and not after the import.
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
    let built_tags;
    let tag_writer: Option<&dyn TagRecordWriter> = match (&plan.tags, opts.tag_record_writer) {
        (None, _) => None,
        (Some(_), Some(writer)) => Some(writer),
        (Some(_), None) => {
            built_tags = tag_record_writer_for_node(
                &opts.targets_root,
                &opts.target_file,
                "nodes.master",
                &opts.master,
            )?;
            Some(built_tags.as_ref())
        }
    };
    // The master's apply host, for a plan that writes tag-group units
    // (26-10): built here, like the record writers, so a node that cannot
    // take the commit refuses before anything is written. Only this module
    // and the deploy apply verb build it (`apply_host_is_confined`).
    let built_host;
    let group_writes = plan
        .tags
        .as_ref()
        .is_some_and(|tags| tags.groups.iter().any(GroupPlan::would_change));
    let apply_host: Option<&dyn ApplyHost> = match surfaces.apply_host {
        Some(host) => Some(host),
        None if group_writes => {
            built_host = apply_host_for_node(
                &opts.targets_root,
                &opts.target_file,
                "nodes.master",
                &opts.master,
                &loaded.target.gateway,
                &loaded.target.mode,
            )?;
            Some(built_host.as_ref())
        }
        None => None,
    };
    let surfaces = &ProjectsSurfaces {
        gateway: surfaces.gateway,
        host: surfaces.host,
        tree: surfaces.tree,
        apply_host,
        deploy: surfaces.deploy,
    };
    // The pre-checks read the gateway and write nothing: a refusal leaves no
    // run directory, no pre-image, no import and no record.
    wrong_target_check(loaded, surfaces.gateway, opts).await?;
    let designers = designers_check(&plan, surfaces.gateway, &opts.designers, observer).await?;
    let (run_id, run_dir) = RunDir::create_fresh(&opts.repo, &plan.gateway)?;
    let rel = run_dir_rel(&plan.gateway, &run_id);
    let outcome = execute(
        &plan,
        &tag_records,
        loaded,
        input,
        surfaces,
        opts,
        Writers {
            release: writer,
            tags: tag_writer,
        },
        &run_dir,
        &run_id,
        &rel,
        &designers,
        observer,
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
    tag_records: &std::collections::BTreeMap<String, TagRecord>,
    loaded: &LoadedProject,
    input: &ProjectsInput,
    surfaces: &ProjectsSurfaces<'_>,
    opts: &ProjectsRunOptions<'_>,
    writers: Writers<'_>,
    run_dir: &RunDir,
    run_id: &RunId,
    rel: &str,
    designers: &DesignersFound,
    observer: &dyn ProjectsObserver,
) -> Result<DeployProjectsResult, CoreError> {
    let project = plan.project.as_str();
    let gateway = surfaces.gateway;
    let artifact = &loaded.artifact.digest;
    let writer = writers.release;

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
            "tag_providers": loaded.file.tag_providers,
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
    let tag_owning = plan.tags.is_some();

    if plan.action == ProjectAction::Record {
        // Only the record moves, and only for a project that still is the
        // artifact: the record claims exactly that.
        let zip = gateway.export_bytes(project).await?;
        let live = live_digest(project, &zip)?;
        if live.sha256 != artifact.sha256 {
            return Err(stale_live(artifact.sha256.clone(), live.sha256));
        }
        events.emit(ProjectsEvent::Verified { equal: true })?;
        // D-07: the owned tags go first, then the records.
        let step = deploy_owned_tags(
            plan,
            tag_records,
            loaded,
            input,
            surfaces,
            opts,
            run_dir,
            run_id,
            rel,
            &mut events,
        )
        .await?;
        note_tag_step(&mut result, &step);
        if let Err(err) = writer
            .write_release_record(&plan.gateway, project, &record.to_bytes())
            .await
        {
            events.emit_late(ProjectsEvent::RecordFailed {
                code: err.code().to_string(),
            });
            return Err(err);
        }
        // The project is verified and its record is on the host: the tag
        // records follow (26-08).
        write_tag_records(
            &mut result,
            loaded,
            run_id,
            writers.tags,
            &mut events,
            step.batches_sent + step.other_writes > 0,
        )
        .await?;
        // The record is on the host: from here a log failure is remembered
        // and reported, never turned into an error for a run that changed
        // state.
        finish_recorded(&mut result, &mut events);
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

    // D-07: the owned tags go first, after every pre-image and stale check.
    let step = deploy_owned_tags(
        plan,
        tag_records,
        loaded,
        input,
        surfaces,
        opts,
        run_dir,
        run_id,
        rel,
        &mut events,
    )
    .await?;
    note_tag_step(&mut result, &step);
    if plan.action == ProjectAction::Skip {
        // Only a tag-owning plan with tag work reaches a run with a `skip`
        // project: the tags are deployed and verified, the project is left
        // alone and its record already names this release. The tag records
        // follow the green tag verify.
        write_tag_records(
            &mut result,
            loaded,
            run_id,
            writers.tags,
            &mut events,
            step.batches_sent + step.other_writes > 0,
        )
        .await?;
        result.outcome = ProjectOutcome::Applied;
        events.emit_late(ProjectsEvent::Finished {
            outcome: result.outcome,
        });
        note_late_log_failure(&mut result, &events);
        return Ok(result);
    }

    // The gateway changed when tags were written: from then on a log failure
    // is remembered, never an error.
    if tag_owning {
        events.emit_late(ProjectsEvent::ImportSent { overwrite });
    } else {
        events.emit(ProjectsEvent::ImportSent { overwrite })?;
    }
    let answer = match gateway
        .project_import(project, loaded.artifact.zip.clone(), overwrite)
        .await
    {
        Ok(answer) => Some(answer),
        // The request may have reached the gateway and been applied (a
        // timeout or a reset on a 300 s synchronous import). Do not call that
        // "nothing happened": read the project back and let the verdict and
        // the restore command speak. No second import is ever sent.
        Err(err) if import_outcome_unknown(&err) => {
            result.warnings.push(format!(
                "the import request failed after it may have reached the gateway ({}); the project was read back to find out what it holds",
                err.code()
            ));
            None
        }
        // The gateway answered with a refusal, or the request never went out.
        Err(err) => return Err(err),
    };
    // From here the gateway may have changed. Nothing below returns an error:
    // what is left is verifying, recording and reporting.
    if let Some(answer) = &answer {
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
        // The project is verified: the tag records follow (26-08). The
        // gateway changed, so a failure is a warning.
        let _ =
            write_tag_records(&mut result, loaded, run_id, writers.tags, &mut events, true).await;
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
        result.outcome = if tag_owning {
            ProjectOutcome::Applied
        } else if overwrite {
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
    let swept = match result.outcome {
        ProjectOutcome::Imported | ProjectOutcome::ImportedUnverified => true,
        ProjectOutcome::Applied => overwrite,
        _ => false,
    };
    if swept {
        let sweep = sweep_children(&result, loaded, surfaces, opts, &mut events).await;
        result.children = sweep.children;
        result.warnings.extend(sweep.warnings);
    }
    events.emit_late(ProjectsEvent::Finished {
        outcome: result.outcome,
    });
    note_late_log_failure(&mut result, &events);
    Ok(result)
}

/// The most paths a tag-step failure names before it counts the rest.
const TAG_FAILURE_PATHS_SHOWN: usize = 20;

/// `[provider]path`.
fn full_tag_path(provider: &str, path: &str) -> String {
    format!("[{provider}]{path}")
}

/// What the tag step of a run did, for `data.tags` and the record step.
struct TagStep {
    /// Full tag paths of the nodes sent with their live values carried.
    carried: Vec<String>,
    /// The verify's result; `None` for a project that owns no provider.
    verify: Option<TagVerify>,
    /// How many import requests were sent.
    batches_sent: usize,
    /// How many other tag-step writes were sent: provider settings and the
    /// tag-group commit (26-10).
    other_writes: usize,
}

/// The record writers of a run: the release record and, for a tag-owning
/// project, the tag records (26-08).
#[derive(Clone, Copy)]
struct Writers<'a> {
    release: &'a dyn ReleaseRecordWriter,
    tags: Option<&'a dyn TagRecordWriter>,
}

/// One export of every owned provider, flattened, keyed by the provider.
type ProviderReads = std::collections::BTreeMap<String, std::collections::BTreeMap<String, Value>>;

/// Export every provider in `names` and flatten it.
async fn read_providers(
    gateway: &dyn ProjectGateway,
    names: &[&str],
) -> Result<ProviderReads, CoreError> {
    let mut out = std::collections::BTreeMap::new();
    for name in names {
        let bytes = gateway.tag_export(name).await?;
        out.insert((*name).to_string(), flatten_export(&bytes, name)?);
    }
    Ok(out)
}

/// Whether two reads of the owned providers agree as exports (R3-DV part 5,
/// R3-VK, R3-KEY): the same node paths, each equal in configuration.
fn reads_agree(a: &ProviderReads, b: &ProviderReads) -> bool {
    let opts = CanonOpts {
        strip_values: true,
        created: false,
        exports: true,
    };
    a.len() == b.len()
        && a.iter().all(|(provider, nodes)| {
            b.get(provider).is_some_and(|other| {
                nodes.len() == other.len()
                    && nodes.iter().all(|(path, node)| {
                        other
                            .get(path)
                            .is_some_and(|o| diff_properties(node, o, path, opts).is_empty())
                    })
            })
        })
}

/// The tag step of a tag-owning run (D-07), before the project is touched:
/// save every owned provider's export as the pre-image, re-plan against it
/// and refuse a plan that is no longer the reviewed one, send every batch of
/// [`import_batches`] in order (a carried batch after a fresh export of its
/// provider), then verify every owned provider ([`TAG_VERIFY_POLL_S`],
/// [`TAG_VERIFY_SETTLE_S`]).
///
/// A failure after the first import request is `rollback_failed` naming the
/// run directory, the pre-image and the batches the run sent: nothing is
/// rolled back automatically in this build (26-11 adds the rollback). A
/// failure before it returns the error as it is: nothing was written.
#[allow(clippy::too_many_arguments)]
async fn deploy_owned_tags(
    plan: &DeployProjectsResult,
    tag_records: &std::collections::BTreeMap<String, TagRecord>,
    loaded: &LoadedProject,
    input: &ProjectsInput,
    surfaces: &ProjectsSurfaces<'_>,
    opts: &ProjectsRunOptions<'_>,
    run_dir: &RunDir,
    run_id: &RunId,
    rel: &str,
    events: &mut RunEvents<'_>,
) -> Result<TagStep, CoreError> {
    let gateway = surfaces.gateway;
    let Some(report) = &plan.tags else {
        return Ok(TagStep {
            carried: Vec::new(),
            verify: None,
            batches_sent: 0,
            other_writes: 0,
        });
    };
    // The provider settings the plan was drawn against are the ones about to
    // change (26-10): each owned provider's record is read again, and the
    // live record of a provider the run modifies is its pre-image.
    let settings = fresh_provider_settings(loaded, gateway, report, run_dir, run_id).await?;
    let created: std::collections::BTreeSet<&str> = report
        .provider_settings
        .iter()
        .filter(|p| p.action == ProviderSettingsAction::Create)
        .map(|p| p.provider.as_str())
        .collect();
    // Pre-images of every owned provider, before the first write. Tag values
    // can hold secrets, so the files are 0600 inside `export/`. A provider
    // the run creates has no tree yet.
    let mut live = std::collections::BTreeMap::new();
    for provider in &loaded.owned_tags {
        if created.contains(provider.name.as_str()) {
            continue;
        }
        let bytes = gateway.tag_export(&provider.name).await?;
        let mut file = run_dir
            .create_private_file_nested(&["export", "tags"], &format!("{}.json", provider.name))?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| CoreError::DeployRunInvalid {
                run_id: run_id.as_str().to_string(),
                problem: format!(
                    "could not write the tag pre-image of provider {} ({:?})",
                    provider.name,
                    e.kind()
                ),
            })?;
        events.emit(ProjectsEvent::TagsPreImageExported {
            provider: provider.name.clone(),
            bytes: bytes.len(),
        })?;
        live.insert(
            provider.name.clone(),
            flatten_export(&bytes, &provider.name)?,
        );
    }
    // The tags the plan was drawn against are the ones about to change.
    let now = plan_tags(&loaded.owned_tags, &live, tag_records);
    if now.live_sha256 != report.live_sha256 {
        return Err(CoreError::PlanStale {
            expected: report.live_sha256.clone(),
            actual: now.live_sha256,
            detail: "the owned tag providers changed since the plan".to_string(),
        });
    }
    // The tag groups the plan was drawn against are the ones about to change
    // (26-10), and no apply journal is on the box.
    let groups = fresh_groups(loaded, input, surfaces, report, &now, tag_records).await?;
    // Every batch of every provider is built before the first write, so a
    // plan the batches refuse (a type cycle) writes nothing.
    let empty = std::collections::BTreeMap::new();
    let owned = sorted(&loaded.owned_tags);
    let mut queue: std::collections::VecDeque<ImportBatch> = std::collections::VecDeque::new();
    for (provider_plan, provider) in now.providers.iter().zip(&owned) {
        let nodes = live.get(&provider.name).unwrap_or(&empty);
        queue.extend(import_batches(
            provider_plan,
            provider,
            nodes,
            REMOVED_PROPERTY_POLICY,
        )?);
    }

    let mut sent: Vec<String> = Vec::new();
    let mut carried_paths: Vec<String> = Vec::new();
    let fail_at = |journal: &str, sent: &[String], what: String| {
        let shown: Vec<&str> = sent
            .iter()
            .take(TAG_FAILURE_PATHS_SHOWN)
            .map(String::as_str)
            .collect();
        let more = sent.len().saturating_sub(shown.len());
        CoreError::RollbackFailed(Box::new(RollbackFailure {
            run_id: run_id.as_str().to_string(),
            env: input.env.clone(),
            run_dir: Some(rel.to_string()),
            journal: journal.to_string(),
            cause: format!(
                "{what}; the project was not imported and nothing was rolled back \
                 automatically; the run sent {} tag import batch(es): {}{}",
                sent.len(),
                if shown.is_empty() {
                    "none".to_string()
                } else {
                    shown.join(", ")
                },
                if more > 0 {
                    format!(" and {more} more")
                } else {
                    String::new()
                }
            ),
        }))
    };
    let fail = |provider: &str, sent: &[String], what: String| {
        fail_at(&format!("{rel}/export/tags/{provider}.json"), sent, what)
    };
    // Provider settings first (26-10, the git module's order plus D-07): a
    // provider is created or modified, then read again and compared.
    let mut other_writes = 0;
    for (planned, record) in &settings {
        if planned.action == ProviderSettingsAction::Same {
            continue;
        }
        let name = planned.provider.as_str();
        let repo = provider_settings_of(loaded, name)?;
        let answer = match (planned.action, record) {
            (ProviderSettingsAction::Create, _) => {
                gateway
                    .tag_provider_create(&[TagProviderCreate::with_settings(
                        name,
                        repo.enabled,
                        repo.description.clone(),
                        repo.config.clone(),
                    )])
                    .await
            }
            (ProviderSettingsAction::Modify, Some(record)) => {
                let signature = record.signature.clone().unwrap_or_default();
                gateway
                    .tag_provider_modify(&[TagProviderModify {
                        name: name.to_string(),
                        collection: record.collection().unwrap_or("core").to_string(),
                        enabled: repo.enabled,
                        description: repo.description.clone(),
                        config: repo.config.clone(),
                        signature,
                    }])
                    .await
            }
            _ => Err(CoreError::Internal(format!(
                "tag provider {name} is planned to be modified but has no record"
            ))),
        };
        // The request may have reached the gateway whatever came back.
        sent.push(format!(
            "[{name}] provider settings ({})",
            planned.action.as_str()
        ));
        other_writes += 1;
        let pre = provider_pre_image(rel, name, planned.action);
        if let Err(err) = answer {
            return Err(fail_at(
                &pre,
                &sent,
                format!(
                    "the settings of tag provider {name} were not applied ({})",
                    err.code()
                ),
            ));
        }
        let reread = match gateway.tag_provider_record(name).await {
            Ok(reread) => reread,
            Err(err) => {
                return Err(fail_at(
                    &pre,
                    &sent,
                    format!(
                        "tag provider {name} could not be read after its settings were applied ({})",
                        err.code()
                    ),
                ));
            }
        };
        let check = plan_provider_settings(name, repo, reread.as_ref());
        if check.action != ProviderSettingsAction::Same {
            return Err(fail_at(
                &pre,
                &sent,
                format!(
                    "the settings of tag provider {name} read back different from the repo ({})",
                    if check.properties.is_empty() {
                        "the provider is absent".to_string()
                    } else {
                        check.properties.join(", ")
                    }
                ),
            ));
        }
        events.emit_late(ProjectsEvent::TagProviderSettingsApplied {
            provider: name.to_string(),
            action: planned.action.as_str(),
        });
    }

    // Then the tag groups (26-10, D-12): the group units through the file
    // engine's host commit and a config scan, before any type or tag import.
    if let Some(work) = groups.as_ref().filter(|work| work.would_change()) {
        let providers = work
            .plans
            .iter()
            .filter(|p| p.would_change())
            .map(|p| p.provider.clone())
            .collect::<Vec<_>>();
        let label = format!("tag groups of {}", providers.join(", "));
        match commit_groups(loaded, surfaces, run_id, &report.plan_sha256, work).await {
            Ok((written, deleted)) => {
                sent.push(format!("{label} ({written} written, {deleted} deleted)"));
                other_writes += 1;
                events.emit_late(ProjectsEvent::TagGroupsWritten {
                    providers,
                    written,
                    deleted,
                });
            }
            Err(GroupFailure::NotWritten(err)) if sent.is_empty() => return Err(err),
            Err(GroupFailure::NotWritten(err)) => {
                return Err(fail_at(
                    rel,
                    &sent,
                    format!("the {label} were not written ({})", err.code()),
                ));
            }
            Err(GroupFailure::Stale) if sent.is_empty() => {
                return Err(CoreError::PlanStale {
                    expected: "the planned tag-group units".to_string(),
                    actual: "a unit changed before the commit".to_string(),
                    detail: "the owned providers' tag-group units changed since the plan"
                        .to_string(),
                });
            }
            Err(GroupFailure::Stale) => {
                return Err(fail_at(
                    rel,
                    &sent,
                    format!("the {label} were not written: a unit changed since the plan"),
                ));
            }
            Err(GroupFailure::Restored(why)) => {
                sent.push(format!("{label} (restored)"));
                return Err(fail_at(
                    rel,
                    &sent,
                    format!(
                        "the {label} did not verify ({why}); the group units were restored from \
                         the run's snapshot"
                    ),
                ));
            }
            Err(GroupFailure::Unrestored(why)) => {
                sent.push(format!("{label} (not restored)"));
                return Err(fail_at(
                    &journal_location(
                        LedgerRoot::for_node(&loaded.target.nodes.master),
                        &loaded.target.gateway,
                        &loaded.target.mode,
                    ),
                    &sent,
                    format!(
                        "the {label} failed and could not be restored ({why}); the on-box \
                         journal stays"
                    ),
                ));
            }
        }
    }

    while let Some(batch) = queue.pop_front() {
        let name = batch.provider.clone();
        let body = if batch.carry {
            // R3-CARRY: the values come from an export read right now.
            let fresh = match gateway.tag_export(&name).await {
                Ok(bytes) => flatten_export(&bytes, &name),
                Err(err) => Err(err),
            };
            let fresh = match fresh {
                Ok(fresh) => fresh,
                Err(err) if sent.is_empty() => return Err(err),
                Err(err) => {
                    return Err(fail(
                        &name,
                        &sent,
                        format!(
                            "provider {name} could not be read to carry the live values ({})",
                            err.code()
                        ),
                    ));
                }
            };
            match carried(&batch, &fresh)? {
                Carried::Body(bytes) => bytes,
                Carried::Split(parts) => {
                    for part in parts.into_iter().rev() {
                        queue.push_front(part);
                    }
                    continue;
                }
            }
        } else {
            batch.body.clone()
        };
        if batch.carry {
            let event = ProjectsEvent::TagValuesCarried {
                provider: name.clone(),
                path: batch.path.clone(),
                nodes: batch.nodes.len(),
            };
            if sent.is_empty() {
                events.emit(event)?;
            } else {
                events.emit_late(event);
            }
            carried_paths.extend(batch.nodes.iter().map(|p| full_tag_path(&name, p)));
        }
        let event = ProjectsEvent::TagImportSent {
            provider: name.clone(),
            path: batch.path.clone(),
            policy: batch.policy.as_wire(),
            nodes: batch.nodes.len(),
            carry: batch.carry,
        };
        if sent.is_empty() {
            events.emit(event)?;
        } else {
            events.emit_late(event);
        }
        let answer = gateway
            .tag_import(&name, &batch.path, batch.policy, body)
            .await;
        // The request may have reached the gateway whatever came back.
        sent.push(format!(
            "{} ({}, {} node(s))",
            full_tag_path(&name, &batch.path),
            batch.policy.as_wire(),
            batch.nodes.len()
        ));
        let answer = match answer {
            Ok(answer) => answer,
            Err(err) => {
                return Err(fail(
                    &name,
                    &sent,
                    format!(
                        "the tag import into provider {name} failed ({})",
                        err.code()
                    ),
                ));
            }
        };
        let clean = answer.is_clean();
        events.emit_late(ProjectsEvent::TagImportAnswered {
            provider: name.clone(),
            clean,
        });
        if !clean {
            let mut codes = answer.bad_quality.clone();
            codes.truncate(TAG_FAILURE_PATHS_SHOWN);
            let first = batch.nodes.first().map_or_else(
                || full_tag_path(&name, &batch.path),
                |p| full_tag_path(&name, p),
            );
            return Err(fail(
                &name,
                &sent,
                format!(
                    "the tag import headed by {first} into provider {name} was not clean ({} failure(s){})",
                    answer.failure_count.unwrap_or(codes.len() as u64),
                    if codes.is_empty() {
                        String::new()
                    } else {
                        format!(": {}", codes.join(", "))
                    }
                ),
            ));
        }
    }

    // Verify: read every owned provider until two reads agree or the bound
    // passes, then compare the last read.
    let names: Vec<&str> = owned.iter().map(|p| p.name.as_str()).collect();
    let journal_provider = names.first().copied().unwrap_or_default().to_string();
    let read_failed = |err: CoreError, sent: &[String]| {
        let what = format!(
            "the owned tag providers could not be read back to verify ({})",
            err.code()
        );
        if sent.is_empty() {
            err
        } else {
            fail(&journal_provider, sent, what)
        }
    };
    let started = Instant::now();
    let mut reads = 1;
    let mut last = match read_providers(gateway, &names).await {
        Ok(read) => read,
        Err(err) => return Err(read_failed(err, &sent)),
    };
    let mut settled = false;
    loop {
        let elapsed = started.elapsed();
        if elapsed >= opts.tag_verify_settle {
            break;
        }
        tokio::time::sleep(opts.tag_verify_poll.min(opts.tag_verify_settle - elapsed)).await;
        let next = match read_providers(gateway, &names).await {
            Ok(read) => read,
            Err(err) => return Err(read_failed(err, &sent)),
        };
        reads += 1;
        let agree = reads_agree(&last, &next);
        last = next;
        if agree {
            settled = true;
            break;
        }
    }
    let elapsed_s = (started.elapsed().as_millis() as f64) / 1000.0;

    let mut mismatches = Vec::new();
    for (provider_plan, provider) in now.providers.iter().zip(&owned) {
        let name = provider.name.as_str();
        let after = last.get(name).unwrap_or(&empty);
        let pre = live.get(name).unwrap_or(&empty);
        let created: std::collections::BTreeSet<&str> =
            provider_plan.added.iter().map(String::as_str).collect();
        let mut differ = Vec::new();
        for (path, node) in &provider.nodes {
            match after.get(path) {
                None => differ.push(format!("{} (absent)", full_tag_path(name, path))),
                Some(live_node) => {
                    let properties = diff_properties(
                        &node.json,
                        live_node,
                        path,
                        CanonOpts {
                            strip_values: true,
                            created: created.contains(path.as_str()),
                            exports: false,
                        },
                    );
                    if !properties.is_empty() {
                        differ.push(format!(
                            "{} ({})",
                            full_tag_path(name, path),
                            properties.join(", ")
                        ));
                    }
                }
            }
        }
        // D-01: every live node the repo lacks is as the pre-image held it.
        for path in &provider_plan.drift {
            let properties = match (pre.get(path), after.get(path)) {
                (Some(before), Some(now)) => diff_properties(
                    before,
                    now,
                    path,
                    CanonOpts {
                        strip_values: true,
                        created: false,
                        exports: true,
                    },
                ),
                (Some(_), None) => vec!["absent".to_string()],
                _ => Vec::new(),
            };
            if !properties.is_empty() {
                differ.push(format!(
                    "{} (drift: {})",
                    full_tag_path(name, path),
                    properties.join(", ")
                ));
            }
        }
        events.emit_late(ProjectsEvent::TagsVerified {
            provider: name.to_string(),
            equal: differ.is_empty(),
        });
        mismatches.extend(differ);
    }
    mismatches.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    let mismatches_count = mismatches.len();
    if mismatches_count > 0 {
        let shown: Vec<&str> = mismatches
            .iter()
            .take(TAG_FAILURE_PATHS_SHOWN)
            .map(String::as_str)
            .collect();
        let what = format!(
            "the tag verify found {mismatches_count} node(s) unlike the repo or their pre-image \
             after {reads} read(s){}: {}",
            if settled { "" } else { ", unsettled" },
            shown.join("; ")
        );
        return Err(fail(&journal_provider, &sent, what));
    }
    let mut capped = mismatches;
    capped.truncate(LIST_CAP);
    carried_paths.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    Ok(TagStep {
        carried: carried_paths,
        verify: Some(TagVerify {
            equal: true,
            settled,
            reads,
            elapsed_s,
            mismatches: capped,
            mismatches_count,
        }),
        batches_sent: sent.len().saturating_sub(other_writes),
        other_writes,
    })
}

/// Put what the tag step did into the result's `data.tags`, and warn when
/// the verify reads did not settle.
/// How often a tag-group commit's config scan is polled.
const GROUP_SCAN_POLL: Duration = Duration::from_millis(500);

/// How long a tag-group commit's config scan may take before the units are
/// restored (the gateway-config apply's bound: the longest scan measured,
/// 23-10, took under 1.6 s).
const GROUP_SCAN_WAIT: Duration = Duration::from_secs(120);

fn group_scan_poll() -> crate::poll::PollConfig {
    crate::poll::PollConfig {
        subject: "the config scan after the tag-group commit".to_string(),
        interval: GROUP_SCAN_POLL,
        max: Duration::from_secs(2),
        deadline: GROUP_SCAN_WAIT,
    }
}

/// The tag-group work of a run, re-planned right before the first write.
struct GroupWork {
    /// The providers the groups were read for.
    providers: Vec<String>,
    /// The fresh plans; equal to the reviewed ones.
    plans: Vec<GroupPlan>,
}

impl GroupWork {
    fn would_change(&self) -> bool {
        self.plans.iter().any(GroupPlan::would_change)
    }
}

/// Read the owned providers' tag groups again and re-plan them (26-10):
/// a plan that is no longer the reviewed one is `plan_stale`. With group
/// writes ahead, an apply journal on the box (a gateway-config run in flight
/// or interrupted) refuses `deploy_journal_present`. Reads only.
async fn fresh_groups(
    loaded: &LoadedProject,
    input: &ProjectsInput,
    surfaces: &ProjectsSurfaces<'_>,
    report: &TagsReport,
    tag_plan: &TagPlan,
    tag_records: &std::collections::BTreeMap<String, TagRecord>,
) -> Result<Option<GroupWork>, CoreError> {
    if report.groups.is_empty() {
        return Ok(None);
    }
    let reader = need_tree(surfaces, "the tag-group write")?;
    let inputs = group_inputs(loaded, tag_plan)?;
    let providers: Vec<String> = report.groups.iter().map(|g| g.provider.clone()).collect();
    let observed = read_groups(reader, &loaded.target.mode, &providers).await?;
    let plans = group_plans(&inputs, &observed, tag_records)?;
    if plans != report.groups {
        let shown = |plans: &[GroupPlan]| {
            plans
                .iter()
                .map(|p| {
                    format!(
                        "{}: {} added, {} changed, {} removed",
                        p.provider,
                        p.added.len(),
                        p.changed.len(),
                        p.removed.len()
                    )
                })
                .collect::<Vec<_>>()
                .join("; ")
        };
        return Err(CoreError::PlanStale {
            expected: shown(&report.groups),
            actual: shown(&plans),
            detail: "the owned providers' tag groups changed since the plan".to_string(),
        });
    }
    let work = GroupWork { providers, plans };
    if work.would_change() {
        let gateway = &loaded.target.gateway;
        let mode = &loaded.target.mode;
        if let Some(bytes) = reader.read_journal(gateway, mode).await? {
            let location = journal_location(
                LedgerRoot::for_node(&loaded.target.nodes.master),
                gateway,
                mode,
            );
            let journal = Journal::parse(&location, &bytes, gateway, mode)?;
            return Err(CoreError::DeployJournalPresent {
                run_id: journal.run_id,
                env: input.env.clone(),
                location,
            });
        }
    }
    Ok(Some(work))
}

/// The file-engine batch of a tag-group commit: only the group units, the
/// gateway-config ledger left alone (`ledger_new: None`), this run's id, the
/// target's mode, and a journal and record describing exactly these files.
fn group_batch_spec(
    loaded: &LoadedProject,
    run_id: &RunId,
    plan_sha256: &str,
    plans: &[GroupPlan],
) -> (BatchSpec, RenderedTree) {
    let mut tree = RenderedTree::default();
    let mut writes: Vec<WriteOp> = Vec::new();
    let mut deletes: Vec<DeleteOp> = Vec::new();
    for plan in plans {
        for write in &plan.writes {
            writes.push(WriteOp {
                path: write.path.clone(),
                sha256: sha256_hex(&write.bytes),
                pre: write.pre.clone(),
            });
            tree.files.insert(write.path.clone(), write.bytes.clone());
        }
        for delete in &plan.deletes {
            deletes.push(DeleteOp {
                path: delete.path.clone(),
                pre: delete.pre.clone(),
            });
        }
    }
    writes.sort_by(|a, b| a.path.as_str().as_bytes().cmp(b.path.as_str().as_bytes()));
    deletes.sort_by(|a, b| a.path.as_str().as_bytes().cmp(b.path.as_str().as_bytes()));
    let mut touched: Vec<TouchedEntry> = writes
        .iter()
        .map(|w| TouchedEntry {
            path: w.path.as_str().to_string(),
            pre_sha256: w.pre.clone(),
            post_sha256: Some(w.sha256.clone()),
        })
        .chain(deletes.iter().map(|d| TouchedEntry {
            path: d.path.as_str().to_string(),
            pre_sha256: Some(d.pre.clone()),
            post_sha256: None,
        }))
        .collect();
    touched.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    let pre_fingerprint = fingerprint(
        &touched
            .iter()
            .map(|e| (e.path.clone(), e.pre_sha256.clone()))
            .collect(),
    );
    let gateway = loaded.target.gateway.clone();
    let mode = loaded.target.mode.clone();
    let run = run_id.as_str().to_string();
    let budget = HOLD_S.saturating_sub(LOCK_MARGIN_S.unsigned_abs()).max(1);
    let spec = BatchSpec {
        run_id: run_id.clone(),
        gateway: gateway.clone(),
        mode: mode.clone(),
        writes,
        deletes,
        ledger_new: None,
        ledger_pre: None,
        journal: Journal {
            schema_version: 1,
            run_id: run.clone(),
            gateway: gateway.clone(),
            mode: mode.clone(),
            phase: JournalPhase::Snapshotting,
            pre_fingerprint: pre_fingerprint.clone(),
            touched: touched.iter().map(|e| e.path.clone()).collect(),
        },
        record: RunRecord {
            schema_version: 1,
            run_id: run,
            gateway,
            mode,
            render_sha256: tree.render_sha256(),
            plan_sha256: plan_sha256.to_string(),
            state: RecordState::Applying,
            rolled_back_by: None,
            touched,
            pre_fingerprint,
            post_fingerprint: None,
        },
        commit_budget_s: Some(u32::try_from(budget).unwrap_or(u32::MAX)),
    };
    (spec, tree)
}

/// How a tag-group commit ended when it did not verify.
enum GroupFailure {
    /// Nothing was written into a collection: the error.
    NotWritten(CoreError),
    /// The pre-image check under the lock found a changed unit; nothing was
    /// moved.
    Stale,
    /// Files moved and were put back from the snapshot: the reason.
    Restored(String),
    /// Files moved and could not be put back: the reason. The journal stays.
    Unrestored(String),
}

/// Put the group units of `record` back from the run's snapshot (26-10):
/// under the scan-lock (`lock` when still ours with margin, else a fresh
/// one), one `ApplyHost::restore`, one config scan, then the record as
/// `rolled_back`. An `Err` is a value-free reason; the journal then stays.
async fn restore_groups(
    host: &dyn ApplyHost,
    deploy: &dyn DeployApi,
    record: &RunRecord,
    run_id: &RunId,
    lock: Option<&HeldLock>,
) -> Result<(), String> {
    let held = match lock {
        Some(lock) => matches!(
            lock.check(deploy, LOCK_MARGIN_S, &SystemClock).await,
            Ok(LockVerdict::Held { .. })
        ),
        None => false,
    };
    if !held {
        HeldLock::acquire(deploy, ScanKind::Config, HOLD_S, &SystemClock)
            .await
            .map_err(|e| format!("taking the scan-lock for the restore: {}", e.code()))?;
    }
    let report = host
        .restore(record)
        .await
        .map_err(|e| format!("restore: {}", e.code()))?;
    if let Some(failure) = report.failure {
        return Err(format!("restore: {failure}"));
    }
    match scan_and_wait(deploy, ScanKind::Config, group_scan_poll()).await {
        Ok(ScanOutcome::Complete(_)) => {}
        Ok(ScanOutcome::NotComplete { .. }) => {
            return Err("the scan after the restore did not complete".to_string());
        }
        Err(e) => return Err(format!("the scan after the restore: {}", e.code())),
    }
    let mut rolled_back = record.clone();
    rolled_back.state = RecordState::RolledBack;
    rolled_back.rolled_back_by = Some(run_id.as_str().to_string());
    rolled_back.post_fingerprint = Some(record.pre_fingerprint.clone());
    host.finalize(&rolled_back)
        .await
        .map_err(|e| format!("finalize: {}", e.code()))
}

/// [`restore_groups`] after `why`, as the failure it leaves.
async fn restored(
    host: &dyn ApplyHost,
    deploy: &dyn DeployApi,
    record: &RunRecord,
    run_id: &RunId,
    lock: &HeldLock,
    why: String,
) -> GroupFailure {
    match restore_groups(host, deploy, record, run_id, Some(lock)).await {
        Ok(()) => GroupFailure::Restored(why),
        Err(cause) => GroupFailure::Unrestored(format!("{why}; {cause}")),
    }
}

/// Write the group units of `work` through the master's file engine
/// (26-10, D-12): stage, take the config scan-lock, one commit, check the
/// lock, one config scan, read the units back and require them, then
/// finalize the run record. A failure after the commit moved a file restores
/// the units from the snapshot. Returns how many files were written and
/// deleted.
async fn commit_groups(
    loaded: &LoadedProject,
    surfaces: &ProjectsSurfaces<'_>,
    run_id: &RunId,
    plan_sha256: &str,
    work: &GroupWork,
) -> Result<(usize, usize), GroupFailure> {
    let missing = |what: &str| {
        GroupFailure::NotWritten(CoreError::Internal(format!(
            "the tag-group write needs the master's {what}, and none was supplied"
        )))
    };
    let host = surfaces.apply_host.ok_or_else(|| missing("apply host"))?;
    let deploy = surfaces.deploy.ok_or_else(|| missing("deploy API"))?;
    let reader = surfaces.tree.ok_or_else(|| missing("host reader"))?;
    let (spec, tree) = group_batch_spec(loaded, run_id, plan_sha256, &work.plans);
    let touched = spec.touched_paths();
    let probe = host
        .probe_paths(&touched)
        .await
        .map_err(GroupFailure::NotWritten)?;
    if !probe.writable {
        return Err(GroupFailure::NotWritten(CoreError::TransportFailed {
            operation: "write".to_string(),
            node: "nodes.master".to_string(),
            detail: format!(
                "the {} collection cannot take the tag-group units ({})",
                spec.mode,
                probe.problems.join("; ")
            ),
        }));
    }
    let prepared = host
        .prepare(&spec, &tree)
        .await
        .map_err(GroupFailure::NotWritten)?;
    let lock = match HeldLock::acquire(deploy, ScanKind::Config, HOLD_S, &SystemClock).await {
        Ok(lock) => lock,
        Err(e) => {
            // Nothing was written into a collection; only this run's
            // staging is dropped.
            let _ = host.discard_interrupted(run_id).await;
            return Err(GroupFailure::NotWritten(e));
        }
    };
    // A refusal that wrote nothing (a journal of another run, an invalid
    // spec) leaves the lock to lapse: a scan could load another run's state.
    let report = host
        .commit(&prepared)
        .await
        .map_err(GroupFailure::NotWritten)?;
    if report.stale {
        // Nothing moved; one scan releases the lock (D12).
        let _ = scan_and_wait(deploy, ScanKind::Config, group_scan_poll()).await;
        return Err(GroupFailure::Stale);
    }
    let restore = |why: String| restored(host, deploy, &spec.record, run_id, &lock, why);
    if let Some(failure) = &report.failure {
        return Err(restore(format!("the commit stopped: {failure}")).await);
    }
    match lock.check(deploy, LOCK_MARGIN_S, &SystemClock).await {
        Ok(LockVerdict::Held { .. }) => {}
        Ok(_) => return Err(restore("the scan-lock lapsed during the commit".to_string()).await),
        Err(e) => {
            return Err(restore(format!("the scan-lock could not be read ({})", e.code())).await);
        }
    }
    match scan_and_wait(deploy, ScanKind::Config, group_scan_poll()).await {
        Ok(ScanOutcome::Complete(_)) => {}
        Ok(ScanOutcome::NotComplete { .. }) => {
            return Err(restore("the config scan did not complete".to_string()).await);
        }
        Err(e) => return Err(restore(format!("the config scan failed ({})", e.code())).await),
    }
    // Read the units back: every written file holds the bytes, every deleted
    // one is gone from the mode collection.
    let observed = match reader
        .read_tag_groups(&loaded.target.mode, &work.providers)
        .await
    {
        Ok(observed) => observed,
        Err(e) => {
            return Err(restore(format!("the units could not be read back ({})", e.code())).await);
        }
    };
    let mut differing: Vec<String> = spec
        .writes
        .iter()
        .filter(|w| observed.mode.get(&w.path).map(|b| b.sha256.as_str()) != Some(&w.sha256))
        .map(|w| w.path.as_str().to_string())
        .collect();
    differing.extend(
        spec.deletes
            .iter()
            .filter(|d| observed.mode.contains_key(&d.path))
            .map(|d| d.path.as_str().to_string()),
    );
    if !differing.is_empty() {
        return Err(restore(format!(
            "{} unit file(s) read back different: {}",
            differing.len(),
            differing.join(", ")
        ))
        .await);
    }
    let post = match host.fingerprint(&touched).await {
        Ok(post) => post,
        Err(e) => {
            return Err(GroupFailure::Unrestored(format!(
                "the units verified but their hashes could not be read ({}); the journal stays",
                e.code()
            )));
        }
    };
    let mut record = spec.record.clone();
    record.state = RecordState::Applied;
    record.post_fingerprint = Some(fingerprint(&post));
    host.finalize(&record).await.map_err(|e| {
        GroupFailure::Unrestored(format!(
            "the units verified but the run record could not be finalized ({}); the journal stays",
            e.code()
        ))
    })?;
    Ok((spec.writes.len(), spec.deletes.len()))
}

/// The run-directory path of `provider`'s settings pre-image when the run
/// modifies it (`export/tags/<P>.provider.json`), or the run directory for a
/// provider the run creates (it had no pre-image).
fn provider_pre_image(rel: &str, provider: &str, action: ProviderSettingsAction) -> String {
    match action {
        ProviderSettingsAction::Modify => format!("{rel}/export/tags/{provider}.provider.json"),
        _ => rel.to_string(),
    }
}

/// Read every owned provider's record again before the run writes anything
/// (26-10, T-26-35) and refuse `plan_stale` when its settings plan is no
/// longer the reviewed one. The record of a provider the run modifies is
/// saved as its pre-image `export/tags/<P>.provider.json` (0600). Returns the
/// planned entries with the fresh records, in the report's order.
async fn fresh_provider_settings(
    loaded: &LoadedProject,
    gateway: &dyn ProjectGateway,
    report: &TagsReport,
    run_dir: &RunDir,
    run_id: &RunId,
) -> Result<Vec<(ProviderSettingsPlan, Option<TagProviderRecord>)>, CoreError> {
    let mut out = Vec::new();
    for planned in &report.provider_settings {
        let name = planned.provider.as_str();
        let record = gateway.tag_provider_record(name).await?;
        let now =
            plan_provider_settings(name, provider_settings_of(loaded, name)?, record.as_ref());
        if &now != planned {
            let shown = |p: &ProviderSettingsPlan| {
                if p.properties.is_empty() {
                    p.action.as_str().to_string()
                } else {
                    format!("{} ({})", p.action.as_str(), p.properties.join(", "))
                }
            };
            return Err(CoreError::PlanStale {
                expected: shown(planned),
                actual: shown(&now),
                detail: format!("the settings of tag provider {name} changed since the plan"),
            });
        }
        if planned.action == ProviderSettingsAction::Modify
            && let Some(record) = &record
        {
            let bytes = serde_json::to_vec_pretty(record).map_err(|e| {
                CoreError::Internal(format!("a tag provider record did not serialize: {e}"))
            })?;
            let mut file = run_dir.create_private_file_nested(
                &["export", "tags"],
                &format!("{name}.provider.json"),
            )?;
            file.write_all(&bytes)
                .and_then(|()| file.sync_all())
                .map_err(|e| CoreError::DeployRunInvalid {
                    run_id: run_id.as_str().to_string(),
                    problem: format!(
                        "could not write the settings pre-image of tag provider {name} ({:?})",
                        e.kind()
                    ),
                })?;
        }
        out.push((planned.clone(), record));
    }
    Ok(out)
}

fn note_tag_step(result: &mut DeployProjectsResult, step: &TagStep) {
    let Some(tags) = result.tags.as_mut() else {
        return;
    };
    tags.carried_count = step.carried.len();
    tags.carried = step.carried.iter().take(LIST_CAP).cloned().collect();
    tags.verify = step.verify.clone();
    if step.verify.as_ref().is_some_and(|v| !v.settled) {
        result.warnings.push(format!(
            "the owned tag providers did not read the same twice within the verify bound; \
             the last read was compared and equals the repo ({} read(s))",
            step.verify.as_ref().map_or(0, |v| v.reads)
        ));
    }
}

/// The tag record of each owned provider (26-08, TAGDEP-09): the repo's node
/// paths and group names, under the run id and release of the release record,
/// written to the master's host after the run is verified.
///
/// A record that cannot be written after the run changed the gateway is a
/// warning, as for the release record. When the run changed nothing (no tag
/// batch and no project import: the record was the run's only purpose) it is
/// the run's error.
async fn write_tag_records(
    result: &mut DeployProjectsResult,
    loaded: &LoadedProject,
    run_id: &RunId,
    writer: Option<&dyn TagRecordWriter>,
    events: &mut RunEvents<'_>,
    changed_gateway: bool,
) -> Result<(), CoreError> {
    let (Some(tags), Some(writer)) = (result.tags.as_ref(), writer) else {
        return Ok(());
    };
    let tags_sha256 = tags.tags_sha256.clone();
    for provider in sorted(&loaded.owned_tags) {
        let mut groups: Vec<String> = provider
            .groups
            .as_ref()
            .and_then(|g| g.get("tagGroups"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|g| g.get("name").and_then(Value::as_str))
            .map(str::to_string)
            .collect();
        groups.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        groups.dedup();
        let record = TagRecord {
            schema_version: 1,
            gateway: result.gateway.clone(),
            provider: provider.name.clone(),
            project: result.project.clone(),
            release: loaded.release.tag.clone(),
            run_id: run_id.as_str().to_string(),
            deployed_at: rfc3339_now_utc(),
            tags_sha256: tags_sha256.clone(),
            nodes: provider.nodes.keys().cloned().collect(),
            groups,
        };
        match writer
            .write_tag_record(&result.gateway, &provider.name, &record.to_bytes())
            .await
        {
            Ok(()) => events.emit_late(ProjectsEvent::TagRecordWritten {
                provider: provider.name.clone(),
            }),
            Err(err) => {
                events.emit_late(ProjectsEvent::TagRecordFailed {
                    provider: provider.name.clone(),
                    code: err.code().to_string(),
                });
                if !changed_gateway {
                    return Err(err);
                }
                result.warnings.push(format!(
                    "the run is verified but the tag record of provider {} could not be written ({})",
                    provider.name,
                    err.code()
                ));
            }
        }
    }
    Ok(())
}

/// `providers` byte-sorted by name, the order [`plan_tags`] plans them in.
fn sorted(providers: &[NativeProvider]) -> Vec<&NativeProvider> {
    let mut out: Vec<&NativeProvider> = providers.iter().collect();
    out.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
    out
}

/// Whether a failed import request may nonetheless have been applied by the
/// gateway: a transport failure after the request went out (a timeout, a
/// reset) and an unclassified answer (a 5xx can follow a partial apply). A
/// refusal the gateway gave (`auth_failed`, `import_denied`, `not_found`, an
/// uncommissioned or restarting gateway) and a connection that was never made
/// are not: nothing was applied, and the error stands as it is.
fn import_outcome_unknown(err: &CoreError) -> bool {
    match err {
        CoreError::Network {
            source: Some(source),
            ..
        } => !(source.is_connect() || source.is_builder()),
        CoreError::Network { source: None, .. } | CoreError::Internal(_) => true,
        _ => false,
    }
}

/// The tail of a `record` run, after the release record is on the host: the
/// result says so first, then the events are logged late, so a log that fails
/// here is a warning and never an error (the import branch does the same).
fn finish_recorded(result: &mut DeployProjectsResult, events: &mut RunEvents<'_>) {
    result.record_written = true;
    result.outcome = if result.tags.is_some() {
        ProjectOutcome::Applied
    } else {
        ProjectOutcome::Recorded
    };
    events.emit_late(ProjectsEvent::RecordWritten);
    events.emit_late(ProjectsEvent::Finished {
        outcome: result.outcome,
    });
    note_late_log_failure(result, events);
}

/// Warn when an event could not be logged after the gateway or host changed.
fn note_late_log_failure(result: &mut DeployProjectsResult, events: &RunEvents<'_>) {
    if events.late_failure {
        result
            .warnings
            .push("the run's events log could not be written to the end".to_string());
    }
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

    fn sample_result() -> DeployProjectsResult {
        DeployProjectsResult {
            env: "prod".to_string(),
            gateway: "gw".to_string(),
            project: "CapApp".to_string(),
            release: "v1.0.0".to_string(),
            check: false,
            action: ProjectAction::Record,
            would_change: true,
            artifact_sha256: "a".repeat(64),
            live_sha256: Some("a".repeat(64)),
            members: MemberDelta::default(),
            removed: Vec::new(),
            removed_count: 0,
            excluded: Vec::new(),
            excluded_count: 0,
            unmanaged: Vec::new(),
            unmanaged_count: 0,
            parent: None,
            previous_release: None,
            recorded_sha256: None,
            live_matches_last_deploy: None,
            allow_downgrade: false,
            allow_first_change: None,
            warnings: Vec::new(),
            run_id: None,
            run_dir: None,
            record_written: false,
            restore_command: None,
            children: Vec::new(),
            missing_tags: Some(Vec::new()),
            missing_tags_count: Some(0),
            missing_tags_unknown_providers: Some(Vec::new()),
            missing_tags_unknown_providers_count: Some(0),
            missing_tags_unavailable: None,
            missing_tags_limit: MISSING_TAGS_LIMIT,
            tags: None,
            outcome: ProjectOutcome::Checked,
            ok: true,
            slug: None,
        }
    }

    /// WR-05: once the release record is on the host, a log that cannot be
    /// written is a warning on a successful `record` run, not an error, and
    /// the result still says the record was written.
    #[test]
    fn a_log_failure_after_the_record_is_written_is_a_warning() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("events.ndjson");
        std::fs::write(&path, b"").expect("seed");
        // Opened read-only: every append fails.
        let read_only = std::fs::File::open(&path).expect("open read-only");
        let mut events = RunEvents {
            log: EventLog::from_file(read_only),
            observer: &NoProjectsObserver,
            late_failure: false,
        };
        let mut result = sample_result();

        finish_recorded(&mut result, &mut events);

        assert!(result.record_written, "the record is on the host");
        assert_eq!(result.outcome, ProjectOutcome::Recorded);
        assert!(result.ok, "a log failure does not fail the run");
        assert!(events.late_failure);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("events log could not be written")),
            "{:?}",
            result.warnings
        );
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
