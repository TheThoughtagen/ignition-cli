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
//! [`ProjectGateway::tag_provider_modify`] (26-10), whose undo for a provider
//! the run created is [`ProjectGateway::tag_provider_delete`] (26-11, called
//! only by the rollback). `--check` reaches none of
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
//! before any contact, so `--check` lists the removals. `--allow-destructive`
//! passes it (26-20, D-02) when the master's tag-delete route is configured,
//! ready after its bounded wait and lists every provider with removals;
//! otherwise the run refuses before any write. The run deploys the tags and
//! the project in one run, tags first:
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
//! 6. the removals (26-20): per provider with removals an export read right
//!    now, then the removed paths it holds through the tag-delete route
//!    ([`delete_removed`], [`delete_calls`]), each call recorded before it
//!    is sent and its answer required clean;
//! 7. every owned provider is exported every [`TAG_VERIFY_POLL_S`] until two
//!    reads agree or [`TAG_VERIFY_SETTLE_S`] passes, and every repo node must
//!    equal the export in configuration (R3-VK on every node, created ones
//!    included; R3-KEY; R3-DV with the seed check on created nodes) while
//!    every drift node equals its pre-image and every removed node is
//!    absent (`data.tags.verify`);
//! 8. the project steps as before. Once the project is verified (or, for a
//!    `record` or `skip` project action, once the tag verify is green), one
//!    tag record per owned provider is written to the master's host with the
//!    run id and release of the release record. A verified run ends
//!    `applied`.
//!
//! Any failure after the first tag write rolls the run back (26-11, D-07):
//! provider settings that do not read back equal, a failed group commit, a
//! failed or non-clean import, a red tag verify, or a project import that is
//! refused, fails or reads back unlike the artifact. The project is imported
//! again from its pre-image when it changed, the tags go back by the batches
//! of [`rollback_plan`] (R3-RB), the group units from the run's snapshot, the
//! provider settings from their saved records (a provider the run created is
//! deleted), and every owned provider is verified against its pre-image. A
//! green rollback is the verdict `tags_rolled_back` (the project import was
//! never sent) or `rolled_back`, slug [`ROLLED_BACK_SLUG`], exit 6 after the
//! envelope. A node the run created in a provider that existed cannot be
//! deleted (R3-DEL): such a rollback, and one whose step or verify fails,
//! ends `rollback_failed` naming what is left, after every other step ran. A
//! run that wrote no tag keeps K11.
//!
//! ## The host copy and `--rollback-run` (26-12)
//!
//! Before its first gateway write a tag-owning run keeps its pre-image on the
//! master's host beside the release record ([`ProjectRunWriter`]): the
//! project export when it imports the project, each existing owned
//! provider's export, resource record and tag record, then `run.json`
//! ([`PreImageRun`], every file with its sha256), then the journal
//! `applying` ([`ProjectJournal`]). A host write that fails stops the run with
//! nothing written to the gateway. The journal goes `rolling_back` when a
//! rollback starts, and is cleared after a green run (after the records) or a
//! verified rollback; it stays after `rollback_failed` and
//! `imported_unverified`. While it is there the prepare reports it
//! (`data.tags.journal`, a warning) and [`deploy_projects_run`] refuses
//! `deploy_journal_present` naming `ign deploy projects --env <ENV>
//! --rollback-run <RUN_ID>`. [`deploy_projects_rollback_run`] is that
//! command: it reads the latest run's copy and hands it to the same
//! `roll_back`, then puts the pre-run tag records back and clears the
//! journal.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
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
use crate::deploy::apply::rollback::live_changes;
use crate::deploy::apply::run_store::{EventLog, RunDir, RunId, run_dir_rel};
use crate::deploy::apply::{BACKUP_UNSYNCED_SLUG, ROLLED_BACK_SLUG};
use crate::deploy::exclude;
use crate::deploy::ledger::{
    LedgerRoot, journal_location, pre_image_location, project_journal_location,
    release_record_location, tag_record_location,
};
use crate::deploy::lint::{Sends, scan_tag_node};
use crate::deploy::manifest::Manifest;
use crate::deploy::native_tags::{
    NativeNode, NativeProvider, NonCanonical, TAG_GROUPS_FILE, check_canonical_form,
    read_provider_with_raw,
};
use crate::deploy::pair::{BackupPolicy, wall_clock_ms};
use crate::deploy::project_artifact::{
    Artifact, CanonicalDigest, build_artifact, canonical_digest,
};
use crate::deploy::project_run::{
    CreatedProof, JournalState, PreImageFile, PreImageProvider, PreImageRun, ProjectJournal,
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
    AppliedSteps, Carried, ImportBatch, REMOVED_PROPERTY_POLICY, TAG_DELETE_DESCENDANTS, carried,
    created_deletes_now, delete_calls, import_batches, rollback_over_cap, rollback_plan,
    uncarriable_in_batch,
};
use crate::deploy::tag_canon::{CanonOpts, canonical, diff_properties, flatten_export, to_node};
use crate::deploy::tag_converge::{
    BackupTags, DeployResync, MasterResync, TagsBackupEvent, TagsBackupReport, converge_tags,
};
use crate::deploy::tag_delete::{RouteHandshake, TagDeleteRoute, delete_chunks, wait_ready};
use crate::deploy::tag_groups::{
    GroupPlan, GroupSummary, LiveGroups, check_group_refs, live_group_users, parse_group_value,
    plan_groups, referenced_groups,
};
use crate::deploy::tag_plan::{
    NodeChange, ProviderPlan, ProviderSettingsAction, ProviderSettingsPlan, TagDeleteRouteReport,
    TagJournal, TagPlan, TagRollback, TagVerify, TagsReport, plan_provider_settings, plan_tags,
    removed_types_in_use, repo_tags_sha256,
};
use crate::deploy::tag_record::TagRecord;
use crate::deploy::target::{NodeTarget, Target};
use crate::deploy::transport::projects::{
    ProjectHost, ProjectRunWriter, ReleaseRecordWriter, TagRecordWriter, git_entry_location,
    project_run_writer_for_node, release_writer_for_node, tag_record_names_problem,
    tag_record_writer_for_node,
};
use crate::deploy::transport::remote::safe_remote_path;
use crate::deploy::transport::{TreeReader, apply_host_for_node, for_node};
use crate::deploy::tree::read_repo_file;
use crate::deploy::tree::{RenderedTree, sha256_hex};
use crate::error::{
    APPLY_LIST_SHOWN, CoreError, DeployFinding, PreflightFailure, RollbackFailure, RollbackScope,
};

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
/// provider's settings (26-10) or, in a rollback only, delete a provider the
/// run created (26-11), and the read-only check never reaches any of them
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
    /// Delete one tag provider with the signature of a fresh read (26-11).
    /// Only the rollback of a run that created the provider calls it: it is
    /// the undo of that create, and it removes the tags the run put in it.
    async fn tag_provider_delete(&self, name: &str, signature: &str) -> Result<(), CoreError>;
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

    async fn tag_provider_delete(&self, name: &str, signature: &str) -> Result<(), CoreError> {
        self.0.tag_provider_delete(name, signature).await
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
    /// The master's tagDelete route (26-19, D-13 item 9): what a rollback
    /// removes the nodes a failed run created with. The CLI builds it from
    /// the master node's profile only; the backup is never given one.
    pub tag_delete: TagDeleteSurface<'a>,
}

/// Whether the master's profile configures the tagDelete route (26-19).
#[derive(Clone)]
pub enum TagDeleteSurface<'a> {
    /// The route client (26-18's seam).
    Configured(&'a dyn TagDeleteRoute),
    /// The profile lacks what the route needs: the names of the missing
    /// parts (never a value), as `tag_delete_config` reports them.
    NotConfigured {
        /// The missing parts by name.
        missing: Vec<String>,
    },
}

/// The master's tag-delete route as a check or a run found it (26-19).
struct RouteState<'a> {
    /// The route, when the profile configures it.
    route: Option<&'a dyn TagDeleteRoute>,
    /// What the profile lacks, when it does not.
    missing: Vec<String>,
    /// The handshake, when the route answered one.
    handshake: Option<RouteHandshake>,
    /// The slug of the answer that was not a handshake.
    problem: Option<String>,
}

impl<'a> RouteState<'a> {
    /// The state of `surface` after `answer` (`None`: no handshake made).
    fn of(
        surface: &TagDeleteSurface<'a>,
        answer: Option<Result<RouteHandshake, CoreError>>,
    ) -> Self {
        match surface {
            TagDeleteSurface::Configured(route) => {
                let (handshake, problem) = match answer {
                    Some(Ok(handshake)) => (Some(handshake), None),
                    Some(Err(err)) => (None, Some(err.code().to_string())),
                    None => (None, None),
                };
                Self {
                    route: Some(*route),
                    missing: Vec::new(),
                    handshake,
                    problem,
                }
            }
            TagDeleteSurface::NotConfigured { missing } => Self {
                route: None,
                missing: missing.clone(),
                handshake: None,
                problem: None,
            },
        }
    }

    /// The route and its handshake, when it answered.
    fn ready(&self) -> Option<(&'a dyn TagDeleteRoute, &RouteHandshake)> {
        Some((self.route?, self.handshake.as_ref()?))
    }

    /// `data.tags.delete_route` for the owned providers `owned`.
    fn report(&self, owned: &[&str]) -> TagDeleteRouteReport {
        let mut providers_not_allowed: Vec<String> = match &self.handshake {
            Some(handshake) => owned
                .iter()
                .filter(|name| !handshake.providers.iter().any(|p| p == *name))
                .map(|name| (*name).to_string())
                .collect(),
            None => Vec::new(),
        };
        providers_not_allowed.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        TagDeleteRouteReport {
            configured: self.route.is_some(),
            missing: self.missing.clone(),
            ready: self.handshake.is_some(),
            route_version: self.handshake.as_ref().map(|h| h.route_version.clone()),
            providers_not_allowed,
            problem: self.problem.clone(),
        }
    }

    /// Why a node the run created in `provider` stays after the rollback.
    fn kept_reason(&self, provider: &str) -> String {
        match (self.route, &self.handshake) {
            // 26-11's wording, unchanged for a profile without the route.
            (None, _) => "created by this run; this release has no tag delete".to_string(),
            (Some(_), None) => format!(
                "created by this run; the tag-delete route is not ready ({}), so it was not \
                 removed",
                self.problem.as_deref().unwrap_or("no handshake")
            ),
            (Some(_), Some(handshake)) if !handshake.providers.iter().any(|p| p == provider) => {
                format!(
                    "created by this run; the tag-delete route does not allow provider \
                     {provider}, so it was not removed"
                )
            }
            (Some(_), Some(_)) => "created by this run; it holds a node this run did not \
                                   create, so it was not removed"
                .to_string(),
        }
    }
}

/// The owned providers' names, byte-sorted.
fn owned_names(loaded: &LoadedProject) -> Vec<&str> {
    sorted(&loaded.owned_tags)
        .into_iter()
        .map(|p| p.name.as_str())
        .collect()
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
    /// `--allow-destructive` (26-20, D-02): let the run delete the tags the
    /// previous deploy of this project placed and the repo no longer holds
    /// (`data.tags.removed`), through the master's tag-delete route.
    pub allow_destructive: bool,
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
    load_with(input, release)
}

/// [`load_project`] for `--rollback-run` (26-12): the release the undo
/// restores comes from the host copy, so `input.release` is not read and
/// [`LoadedProject::release`] holds an empty placeholder no rollback reads.
/// Everything else is loaded and checked the same way.
pub fn load_project_for_rollback(input: &ProjectsInput) -> Result<LoadedProject, CoreError> {
    load_with(
        input,
        Release {
            tag: String::new(),
            version: semver::Version::new(0, 0, 0),
        },
    )
}

fn load_with(input: &ProjectsInput, release: Release) -> Result<LoadedProject, CoreError> {
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
    /// A tag-owning run failed in its tag step or its tag verify, before the
    /// project import was sent, and its automatic rollback restored and
    /// verified everything it wrote (26-11). A verdict: `data.ok` false,
    /// slug `deploy_rolled_back`, exit 6.
    TagsRolledBack,
    /// A tag-owning run failed at the project import or its read-back after
    /// the tags were applied, and its automatic rollback restored the
    /// project from its pre-image (when it had one) and the tags (26-11). A
    /// verdict: `data.ok` false, slug `deploy_rolled_back`, exit 6.
    RolledBack,
    /// A tag-owning run on a redundant pair applied and verified everything
    /// on the master, wrote its records, and the backup did not prove it
    /// holds the owned tags within the budget, the one resync and the second
    /// budget (26-13). Nothing is rolled back for it. A verdict: `data.ok`
    /// false, slug `deploy_backup_unsynced`, exit 6, the reason in
    /// `data.tags.backup`.
    AppliedBackupUnsynced,
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
            ProjectOutcome::TagsRolledBack => "tags_rolled_back",
            ProjectOutcome::RolledBack => "rolled_back",
            ProjectOutcome::AppliedBackupUnsynced => "applied_backup_unsynced",
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
    /// an unverified import (V6), for the two rolled-back verdicts (26-11)
    /// and for a backup that did not converge (26-13), `None` for every other
    /// outcome. The envelope is a success envelope; the code is the verdict.
    pub fn failure_exit_code(&self) -> Option<u8> {
        matches!(
            self.outcome,
            ProjectOutcome::ImportedUnverified
                | ProjectOutcome::TagsRolledBack
                | ProjectOutcome::RolledBack
                | ProjectOutcome::AppliedBackupUnsynced
        )
        .then_some(6)
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
        // A removal is deleted only with `--allow-destructive` (26-20); the
        // run refuses it otherwise (`tag_removal`).
        list(
            &mut lines,
            "remove (with --allow-destructive)",
            &tags.removed,
            tags.removed_count,
        );
        // A tag-group removal passes the same gate (26-REVIEW CR-02).
        let groups: Vec<String> = tags
            .groups
            .iter()
            .flat_map(|g| g.removed.iter().map(|n| full_tag_path(&g.provider, n)))
            .collect();
        list(
            &mut lines,
            "remove tag group (with --allow-destructive)",
            &groups,
            groups.len(),
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
    let mut warnings = Vec::new();
    // The host journal of an earlier tag-owning run (26-12): a `--check`
    // reports it, an apply refuses before any write.
    let journal = read_project_journal(loaded, surfaces.host).await?;
    if let Some(journal) = &journal {
        warnings.push(format!(
            "project {project}: run {run} left its host journal ({state}) at {location}; an apply \
             refuses until `{command}` undoes it",
            run = journal.run_id,
            state = journal.state.as_str(),
            location = project_journal_location(
                LedgerRoot::for_node(&loaded.target.nodes.master),
                gateway_name,
                project
            ),
            command = rollback_run_command(&input.env, &journal.run_id),
        ));
    }

    let records = gateway.projects().await?;
    // The parent check reads the one list and nothing else (K6 a, D-05): a
    // missing parent, a cycle or a parent that is not inheritable refuses
    // before the project itself is exported, and the parent is never touched.
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
    let (tag_plan, tag_reads, provider_settings, tag_live) =
        match plan_owned_tags(loaded, gateway, &tag_records).await? {
            Some(owned) => (Some(owned.plan), owned.reads, owned.settings, owned.live),
            None => (None, TagReads::default(), Vec::new(), ProviderReads::new()),
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
        Some(plan) => plan_owned_groups(loaded, surfaces, plan, &tag_records, &tag_live).await?,
        None => Vec::new(),
    };
    drop(tag_live);
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
    // The master's tagDelete route (26-19): one handshake, no wait, so a
    // `--check` shows whether a rollback could remove what a run creates.
    let delete_route = match (&tag_plan, &surfaces.tag_delete) {
        (None, _) => None,
        (Some(_), TagDeleteSurface::Configured(route)) => Some(RouteState::of(
            &surfaces.tag_delete,
            Some(route.handshake().await),
        )),
        (Some(_), surface) => Some(RouteState::of(surface, None)),
    }
    .map(|state| state.report(&owned_names(loaded)));
    let tags = tag_plan.map(|plan| {
        let mut report = TagsReport::from_plan(&plan, &loaded.non_canonical, LIST_CAP);
        report.delete_route = delete_route;
        report.warnings.clone_from(&loaded.ownership_warnings);
        report.warnings.extend(tag_secret_warnings);
        report.provider_settings = provider_settings;
        report.groups = groups;
        report.journal = journal.as_ref().map(|j| TagJournal {
            run_id: j.run_id.clone(),
            state: j.state.as_str().to_string(),
            started_at: j.started_at.clone(),
        });
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
    /// Each listed owned provider's export, flattened (26-REVIEW CR-02: the
    /// live users of a group the plan removes).
    live: ProviderReads,
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
    let plan = plan_tags(&loaded.owned_tags, &live, records);
    // T-26-82 (26-20): a removed type that a surviving node uses is refused
    // here, so `--check` refuses it too; the delete step checks again
    // against the export it reads right before its deletes.
    let empty = std::collections::BTreeMap::new();
    let mut in_use = Vec::new();
    for (provider_plan, provider) in plan.providers.iter().zip(sorted(&loaded.owned_tags)) {
        let nodes = live.get(&provider.name).unwrap_or(&empty);
        for (type_path, referrer) in removed_types_in_use(provider_plan, provider, nodes) {
            in_use.push(crate::error::DeployFinding {
                file: provider.nodes.get(&referrer).map_or_else(
                    || full_tag_path(&provider.name, &referrer),
                    |node| node.file.clone(),
                ),
                line: None,
                column: None,
                message: format!(
                    "the repo removes the type {} and {} still uses it (typeId), so deleting it \
                     would break a tag that stays; keep the type in the repo or remove its users \
                     too; nothing was written",
                    full_tag_path(&provider.name, &type_path),
                    full_tag_path(&provider.name, &referrer)
                ),
            });
        }
    }
    if !in_use.is_empty() {
        return Err(CoreError::deploy_findings(in_use));
    }
    Ok(Some(OwnedTags {
        plan,
        reads,
        settings,
        live,
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
    /// The nodes the run removes (26-20): no longer a group's user.
    removed: std::collections::BTreeSet<String>,
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
            removed: provider_plan.removed.iter().cloned().collect(),
        });
    }
    Ok(out)
}

/// The group plans of `inputs` against `observed` (the providers' units),
/// with this project's tag records. Only a provider whose repo holds a
/// summary is planned; without one the project does not manage the
/// provider's groups, and nothing is written or deleted.
///
/// A group the plan removes that a live node of the provider still names
/// (`live`, the flattened exports; a node the run removes aside) is refused
/// `deploy_tree_invalid` naming the group and the node (26-REVIEW CR-02):
/// deleting it would leave a tag the run keeps without its group.
fn group_plans(
    inputs: &[GroupInputs<'_>],
    observed: &crate::deploy::transport::ObservedFiles,
    records: &std::collections::BTreeMap<String, TagRecord>,
    live_nodes: &ProviderReads,
) -> Result<Vec<GroupPlan>, CoreError> {
    let mut plans = Vec::new();
    let mut in_use = Vec::new();
    let empty = BTreeMap::new();
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
        let plan = plan_groups(name, summary, &live, &recorded, &referenced)?;
        if !plan.removed.is_empty() {
            let users = live_group_users(live_nodes.get(name).unwrap_or(&empty), &input.removed);
            for group in &plan.removed {
                if let Some(user) = users.get(group) {
                    in_use.push(crate::error::DeployFinding {
                        file: format!("tags/{name}/{TAG_GROUPS_FILE}"),
                        line: None,
                        column: None,
                        message: format!(
                            "the repo drops tag group `{group}` of provider {name}, which this \
                             project recorded, and the live tag {} still names it, so deleting \
                             the group would leave that tag without its group; keep the group in \
                             the summary or change the tag first; nothing was written",
                            full_tag_path(name, user)
                        ),
                    });
                }
            }
        }
        plans.push(plan);
    }
    if !in_use.is_empty() {
        return Err(CoreError::deploy_findings(in_use));
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
    live: &ProviderReads,
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
    group_plans(&inputs, &observed, records, live)
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
/// The host journal an earlier tag-owning run of this project left on the
/// master (26-12), strictly parsed; `None` for a project that owns no
/// provider or when there is none. A journal that cannot be trusted refuses.
async fn read_project_journal(
    loaded: &LoadedProject,
    host: &dyn ProjectHost,
) -> Result<Option<ProjectJournal>, CoreError> {
    if loaded.file.tag_providers.is_empty() {
        return Ok(None);
    }
    let gateway = &loaded.target.gateway;
    let project = &loaded.file.project;
    let Some(bytes) = host.read_journal(gateway, project).await? else {
        return Ok(None);
    };
    let location = project_journal_location(
        LedgerRoot::for_node(&loaded.target.nodes.master),
        gateway,
        project,
    );
    ProjectJournal::parse(&bytes, &location, gateway, project).map(Some)
}

/// The command that undoes run `run_id` from the host copy (26-12).
fn rollback_run_command(env: &str, run_id: &str) -> String {
    format!(
        "ign deploy projects --env {} --rollback-run {run_id}",
        shell_word(env)
    )
}

/// `deploy_journal_present` for the host journal `journal` (26-12), naming
/// the command that undoes its run.
fn project_journal_present(loaded: &LoadedProject, env: &str, run_id: &str) -> CoreError {
    CoreError::DeployJournalPresent {
        run_id: run_id.to_string(),
        env: env.to_string(),
        location: project_journal_location(
            LedgerRoot::for_node(&loaded.target.nodes.master),
            &loaded.target.gateway,
            &loaded.file.project,
        ),
        recover_with: Some(rollback_run_command(env, run_id)),
    }
}

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
    /// The run failed after its first tag write and its automatic rollback
    /// starts (26-11).
    RollbackStarted {
        /// Why.
        cause: RollbackCause,
    },
    /// The rollback imported the project's pre-image again and read it back
    /// (26-11).
    ProjectRestored {
        /// Whether it reads back as the pre-image.
        equal: bool,
    },
    /// The rollback put the run's tag-group units back from its snapshot
    /// (26-11).
    TagGroupsRestored {
        /// The providers whose groups the run changed, byte-sorted.
        providers: Vec<String>,
    },
    /// The rollback put an owned provider's settings back (`modify`) or
    /// deleted a provider the run created (`delete`) (26-11).
    TagProviderRestored {
        /// The provider.
        provider: String,
        /// `modify` or `delete`.
        action: &'static str,
    },
    /// The rollback read the owned providers back and compared them with
    /// their pre-images (26-11).
    RollbackVerified {
        /// Whether every node equals its pre-image.
        equal: bool,
    },
    /// A tagDelete route call was sent (26-19): the provider and the
    /// provider-relative paths it named; `rollback` for the removal of what
    /// a failed run created.
    TagDeleteSent {
        /// The provider.
        provider: String,
        /// The paths the call named, in request order.
        paths: Vec<String>,
        /// Whether the call is part of a rollback.
        rollback: bool,
    },
    /// The tagDelete route answered a call (26-19).
    TagDeleteAnswered {
        /// The provider.
        provider: String,
        /// Every result was a clean delete.
        clean: bool,
        /// How many paths it removed.
        deleted: usize,
    },
    /// The run's pre-image was written to the gateway host (26-12):
    /// every file of the set, `run.json` last, before the first gateway
    /// write.
    HostPreImageWritten {
        /// How many files the set holds, `run.json` included.
        files: usize,
    },
    /// The host journal was written (26-12).
    JournalWritten {
        /// `applying` or `rolling_back`.
        state: &'static str,
    },
    /// The host journal was cleared after a green run or a verified
    /// rollback (26-12).
    JournalCleared,
    /// The wait for the backup's copy of the owned tags began (26-13).
    BackupWaitStarted {
        /// The budget of one wait, in seconds.
        budget_s: u64,
    },
    /// The one resync request was sent to the master (26-13).
    BackupResync {
        /// The request was accepted.
        ok: bool,
        /// The error code when it was not.
        code: Option<String>,
    },
    /// The wait for the backup ended (26-13).
    BackupVerdict {
        /// The backup holds the master's owned tags.
        converged: bool,
        /// Why not: `unreachable`, `sync_exhausted`, `not_syncing` or
        /// `content_differs`.
        reason: Option<&'static str>,
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
            ProjectsEvent::RollbackStarted { .. } => "rollback_started",
            ProjectsEvent::ProjectRestored { .. } => "project_restored",
            ProjectsEvent::TagGroupsRestored { .. } => "tag_groups_restored",
            ProjectsEvent::TagProviderRestored { .. } => "tag_provider_restored",
            ProjectsEvent::RollbackVerified { .. } => "rollback_verified",
            ProjectsEvent::TagDeleteSent { .. } => "tag_delete_sent",
            ProjectsEvent::TagDeleteAnswered { .. } => "tag_delete_answered",
            ProjectsEvent::HostPreImageWritten { .. } => "host_pre_image_written",
            ProjectsEvent::JournalWritten { .. } => "journal_written",
            ProjectsEvent::JournalCleared => "journal_cleared",
            ProjectsEvent::BackupWaitStarted { .. } => "backup_wait_started",
            ProjectsEvent::BackupResync { .. } => "backup_resync",
            ProjectsEvent::BackupVerdict { .. } => "backup_verdict",
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
            ProjectsEvent::RollbackStarted { cause } => {
                serde_json::json!({ "cause": cause.as_str() })
            }
            ProjectsEvent::ProjectRestored { equal }
            | ProjectsEvent::RollbackVerified { equal } => {
                serde_json::json!({ "equal": equal })
            }
            ProjectsEvent::TagGroupsRestored { providers } => {
                serde_json::json!({ "providers": providers })
            }
            ProjectsEvent::TagProviderRestored { provider, action } => {
                serde_json::json!({ "provider": provider, "action": action })
            }
            ProjectsEvent::TagDeleteSent {
                provider,
                paths,
                rollback,
            } => serde_json::json!({ "provider": provider, "paths": paths, "rollback": rollback }),
            ProjectsEvent::TagDeleteAnswered {
                provider,
                clean,
                deleted,
            } => serde_json::json!({ "provider": provider, "clean": clean, "deleted": deleted }),
            ProjectsEvent::HostPreImageWritten { files } => serde_json::json!({ "files": files }),
            ProjectsEvent::JournalWritten { state } => serde_json::json!({ "state": state }),
            ProjectsEvent::BackupWaitStarted { budget_s } => {
                serde_json::json!({ "budget_s": budget_s })
            }
            ProjectsEvent::BackupResync { ok, code } => {
                serde_json::json!({ "ok": ok, "code": code })
            }
            ProjectsEvent::BackupVerdict { converged, reason } => {
                serde_json::json!({ "converged": converged, "reason": reason })
            }
            ProjectsEvent::Finished { outcome } => {
                serde_json::json!({ "outcome": outcome.as_str() })
            }
            ProjectsEvent::ImportAnswered
            | ProjectsEvent::RecordWritten
            | ProjectsEvent::JournalCleared => serde_json::Value::Null,
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
    /// `nodes.backup` is never written (the redundancy sync carries the
    /// project and the tags to it) and is read only through `backup`.
    pub master: NodeTarget,
    /// A record writer standing in for the one built from `master`. For the
    /// engine tests only: the CLI always passes `None`.
    #[doc(hidden)]
    pub record_writer: Option<&'a dyn ReleaseRecordWriter>,
    /// A tag record writer standing in for the one built from `master`
    /// (26-08). For the engine tests only: the CLI always passes `None`.
    #[doc(hidden)]
    pub tag_record_writer: Option<&'a dyn TagRecordWriter>,
    /// A project run writer (the host journal and pre-image, 26-12) standing
    /// in for the one built from `master`. For the engine tests only: the
    /// CLI always passes `None`.
    #[doc(hidden)]
    pub run_writer: Option<&'a dyn ProjectRunWriter>,
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
    /// The backup node of a redundant pair, read only (26-13): a tag-owning
    /// run waits for it after a green run and after a verified rollback.
    /// `None` for a target without `nodes.backup`: no backup call is made.
    pub backup: Option<&'a dyn BackupTags>,
    /// How long that wait lasts and how often it looks (`--backup-timeout`).
    /// Unused without `backup`.
    pub backup_policy: BackupPolicy,
    /// The longest a tag-owning run waits for the master's tagDelete route
    /// to answer its handshake before the first write (26-19); the CLI
    /// passes [`crate::deploy::tag_delete::TAG_DELETE_READY_TIMEOUT_S`]
    /// seconds. Unused without a configured route.
    pub tag_delete_ready: Duration,
    /// The pause between two handshakes of that wait; the CLI passes
    /// [`crate::deploy::tag_delete::TAG_DELETE_READY_POLL_S`] seconds.
    pub tag_delete_poll: Duration,
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

/// The refusal of a run whose removals fall in providers the tag-delete
/// route does not allow (26-20): the route's own code, naming the providers
/// and the reinstall that adds them.
fn provider_not_allowed(providers: &[&str]) -> CoreError {
    CoreError::WebdevRouteError {
        code: "provider_not_allowed".to_string(),
        message: format!(
            "the tag-delete route does not allow tag provider(s) {}, which this run removes tags \
             from; reinstall the route from this project's repo with `ign webdev deploy \
             --with-tag-delete --tag-delete-repo <repo> --tag-delete-user-source <name> \
             --tag-delete-role <role>`; nothing was written",
            providers.join(", ")
        ),
        endpoint: None,
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
    // A host journal of an earlier run (26-12) refuses the apply before
    // anything is contacted or written, an unchanged plan included.
    let planned = match &prepared {
        ProjectsPrepared::Unchanged(result) => result,
        ProjectsPrepared::Ready { plan, .. } => plan,
    };
    if let Some(journal) = planned.tags.as_ref().and_then(|t| t.journal.as_ref()) {
        return Err(project_journal_present(loaded, &input.env, &journal.run_id));
    }
    let (mut plan, tag_records) = match prepared {
        ProjectsPrepared::Unchanged(result) => return Ok(result),
        ProjectsPrepared::Ready {
            plan, tag_records, ..
        } => (plan, tag_records),
    };
    // A removal (D-02) deletes live tags through the master's tag-delete
    // route (26-17, 26-18), and only with `--allow-destructive`: without the
    // flag it is refused here, before anything is contacted or written;
    // `--check` lists the paths (`data.tags.removed`). With the flag a
    // profile that gives the master no route refuses here too.
    //
    // A tag-group removal (26-10) is the same destructive change
    // (26-REVIEW CR-02): it passes the same gate, with the same flag, and
    // needs no route (the file engine deletes the units).
    let removals = plan
        .tags
        .as_ref()
        .is_some_and(|tags| tags.removed_count > 0);
    let group_removals: Vec<String> = plan
        .tags
        .as_ref()
        .map(|tags| {
            tags.groups
                .iter()
                .flat_map(|g| {
                    g.removed
                        .iter()
                        .map(|name| format!("tag group {}", full_tag_path(&g.provider, name)))
                })
                .collect()
        })
        .unwrap_or_default();
    if let Some(tags) = &plan.tags
        && (removals || !group_removals.is_empty())
        && !input.allow_destructive
    {
        let mut keys = tags.removed.clone();
        keys.extend(group_removals.iter().cloned());
        return Err(CoreError::DestructiveChangeRefused {
            gate: "tag_removal".to_string(),
            keys,
            count: tags.removed_count + group_removals.len(),
        });
    }
    // With the flag, a tag removal needs the route; a profile that gives the
    // master none refuses here.
    if removals && let TagDeleteSurface::NotConfigured { missing } = &surfaces.tag_delete {
        return Err(CoreError::TagDeleteNotConfigured {
            profile: loaded.target.nodes.master.profile.clone(),
            missing: missing.clone(),
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
    // The host journal and pre-image writer (26-12), for a tag-owning plan.
    let built_run;
    let run_writer: Option<&dyn ProjectRunWriter> = match (&plan.tags, opts.run_writer) {
        (None, _) => None,
        (Some(_), Some(writer)) => Some(writer),
        (Some(_), None) => {
            built_run = project_run_writer_for_node(
                &opts.targets_root,
                &opts.target_file,
                "nodes.master",
                &opts.master,
            )?;
            Some(built_run.as_ref())
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
        tag_delete: surfaces.tag_delete.clone(),
    };
    // The pre-checks read the gateway and write nothing: a refusal leaves no
    // run directory, no pre-image, no import and no record.
    wrong_target_check(loaded, surfaces.gateway, opts).await?;
    let designers = designers_check(&plan, surfaces.gateway, &opts.designers, observer).await?;
    // The master's tagDelete route (26-19): a tag-owning run waits for it,
    // bounded, before its first write, so a route installed right before
    // the run is ready when a rollback needs it. For a run without removals
    // a route that does not become ready is a warning, and the run goes on:
    // its rollback then cannot remove what it creates. A run with removals
    // (26-20) refuses instead, before any write: with the wait's own slug,
    // or `webdev_route_error` code `provider_not_allowed` when the route's
    // allowlist lacks a provider it removes tags from.
    let mut answer = match (&plan.tags, &surfaces.tag_delete) {
        (Some(_), TagDeleteSurface::Configured(route)) => {
            Some(wait_ready(*route, opts.tag_delete_ready, opts.tag_delete_poll).await)
        }
        _ => None,
    };
    if removals {
        if let Some(Err(_)) = &answer
            && let Some(Err(err)) = answer.take()
        {
            return Err(err);
        }
        if let (Some(Ok(handshake)), Some(tags)) = (&answer, &plan.tags) {
            let refused: Vec<&str> = tags
                .providers
                .iter()
                .filter(|p| p.removed > 0)
                .map(|p| p.provider.as_str())
                .filter(|name| !handshake.providers.iter().any(|p| p == name))
                .collect();
            if !refused.is_empty() {
                return Err(provider_not_allowed(&refused));
            }
        }
    }
    let route = RouteState::of(&surfaces.tag_delete, answer);
    if let Some(tags) = plan.tags.as_mut() {
        tags.delete_route = Some(route.report(&owned_names(loaded)));
        if let (Some(_), None) = (route.route, &route.handshake) {
            tags.warnings.push(format!(
                "the tag-delete route is not ready ({}); a rollback cannot remove tags this run \
                 creates",
                route.problem.as_deref().unwrap_or("no handshake")
            ));
        }
    }
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
            run: run_writer,
        },
        &run_dir,
        &run_id,
        &rel,
        &designers,
        &route,
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
    route: &RouteState<'_>,
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
    // The anchor of the backup's sync-exhausted line (26-13): a line logged
    // before this run never ends its wait.
    let backup_since_ms = wall_clock_ms();
    let owned_names: Vec<&str> = sorted(&loaded.owned_tags)
        .into_iter()
        .map(|p| p.name.as_str())
        .collect();

    if plan.action == ProjectAction::Record {
        // Only the record moves, and only for a project that still is the
        // artifact: the record claims exactly that.
        let zip = gateway.export_bytes(project).await?;
        let live = live_digest(project, &zip)?;
        if live.sha256 != artifact.sha256 {
            return Err(stale_live(artifact.sha256.clone(), live.sha256));
        }
        events.emit(ProjectsEvent::Verified { equal: true })?;
        // D-07: the owned tags go first, then the records. A tag step that
        // fails after its first write is rolled back (26-11).
        let step = match deploy_owned_tags(
            plan,
            tag_records,
            loaded,
            input,
            surfaces,
            opts,
            run_dir,
            run_id,
            HostWrites {
                writer: writers.run,
                project_zip: None,
            },
            route,
            &mut events,
        )
        .await
        {
            Ok(step) => step,
            Err(TagStepError::Unwritten(err)) => return Err(err),
            Err(TagStepError::Written(failure)) => {
                return roll_back(
                    *failure,
                    None,
                    result,
                    loaded,
                    input,
                    surfaces,
                    opts,
                    run_id,
                    rel,
                    UndoWriters {
                        run: writers.run,
                        records: None,
                        release: None,
                    },
                    route,
                    &mut events,
                )
                .await;
            }
        };
        note_tag_step(&mut result, &step);
        // The tags were written and verified: from here a failed release
        // record is a warning, as on the import path (26-REVIEW WR-02), so
        // the tag records and the journal clear still follow. A run that
        // wrote no tag changed nothing, and the error stands.
        let changed = step.writes.wrote();
        let mut record_written = true;
        if let Err(err) = writer
            .write_release_record(&plan.gateway, project, &record.to_bytes())
            .await
        {
            events.emit_late(ProjectsEvent::RecordFailed {
                code: err.code().to_string(),
            });
            if !changed {
                return Err(unwritten(
                    err,
                    loaded,
                    writers.run,
                    step.writes.host.as_ref(),
                    &mut events,
                )
                .await);
            }
            record_written = false;
            result.warnings.push(format!(
                "the tags are verified but the release record could not be written ({})",
                err.code()
            ));
        }
        // The project is verified and its record is on the host: the tag
        // records follow (26-08).
        if let Err(err) = write_tag_records(
            &mut result,
            loaded,
            run_id,
            writers.tags,
            &mut events,
            step.batches_sent + step.other_writes > 0,
        )
        .await
        {
            if changed {
                return Err(err);
            }
            return Err(unwritten(
                err,
                loaded,
                writers.run,
                step.writes.host.as_ref(),
                &mut events,
            )
            .await);
        }
        // Green: the host journal goes (26-12).
        clear_host_journal(
            &mut result,
            loaded,
            writers.run,
            step.writes.host.as_ref(),
            &mut events,
        )
        .await;
        // On a pair the backup's copy of the owned tags is checked (26-13).
        let backup = backup_step(
            &mut result,
            &owned_names,
            surfaces,
            opts,
            backup_since_ms,
            &mut events,
        )
        .await;
        // The record is on the host: from here a log failure is remembered
        // and reported, never turned into an error for a run that changed
        // state.
        finish_recorded(
            &mut result,
            &mut events,
            backup == Some(false),
            record_written,
        );
        return Ok(result);
    }

    let overwrite = plan.action == ProjectAction::Import;
    // The project's pre-image, kept for a rollback after the tags were
    // applied (26-11).
    let mut pre_zip: Option<Vec<u8>> = None;
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
        pre_zip = Some(pre);
    }

    // D-07: the owned tags go first, after every pre-image and stale check.
    // A tag step that fails after its first write is rolled back (26-11).
    let step = match deploy_owned_tags(
        plan,
        tag_records,
        loaded,
        input,
        surfaces,
        opts,
        run_dir,
        run_id,
        HostWrites {
            writer: writers.run,
            project_zip: pre_zip.as_deref(),
        },
        route,
        &mut events,
    )
    .await
    {
        Ok(step) => step,
        Err(TagStepError::Unwritten(err)) => return Err(err),
        Err(TagStepError::Written(failure)) => {
            return roll_back(
                *failure,
                None,
                result,
                loaded,
                input,
                surfaces,
                opts,
                run_id,
                rel,
                UndoWriters {
                    run: writers.run,
                    records: None,
                    release: None,
                },
                route,
                &mut events,
            )
            .await;
        }
    };
    note_tag_step(&mut result, &step);
    // From the first tag write on, a failed project step rolls the tags and
    // the project back (26-11); a run that wrote no tag keeps K11.
    let tags_wrote = step.writes.wrote();
    if plan.action == ProjectAction::Skip {
        // Only a tag-owning plan with tag work reaches a run with a `skip`
        // project: the tags are deployed and verified, the project is left
        // alone and its record already names this release. The tag records
        // follow the green tag verify.
        if let Err(err) = write_tag_records(
            &mut result,
            loaded,
            run_id,
            writers.tags,
            &mut events,
            step.batches_sent + step.other_writes > 0,
        )
        .await
        {
            if tags_wrote {
                return Err(err);
            }
            return Err(unwritten(
                err,
                loaded,
                writers.run,
                step.writes.host.as_ref(),
                &mut events,
            )
            .await);
        }
        clear_host_journal(
            &mut result,
            loaded,
            writers.run,
            step.writes.host.as_ref(),
            &mut events,
        )
        .await;
        result.outcome = ProjectOutcome::Applied;
        if backup_step(
            &mut result,
            &owned_names,
            surfaces,
            opts,
            backup_since_ms,
            &mut events,
        )
        .await
            == Some(false)
        {
            mark_backup_unsynced(&mut result);
        }
        events.emit_late(ProjectsEvent::Finished {
            outcome: result.outcome,
        });
        note_late_log_failure(&mut result, &events);
        return Ok(result);
    }

    // The Designer check ran before the tag step, which can take minutes:
    // it is made again, under the same policy, right before the import it
    // guards (ultrareview U7). A refusal after tag writes rolls back like a
    // refused import; before any write it is the run's error.
    if tag_owning && !designers.skipped {
        match designers_check(plan, gateway, &opts.designers, observer).await {
            Ok(found) => {
                if found.proceeded {
                    events.emit_late(ProjectsEvent::DesignersProceed {
                        sessions: found.seen.clone(),
                    });
                }
            }
            Err(err) if tags_wrote => {
                let failure = TagFailure {
                    cause: RollbackCause::ProjectImportFailed,
                    what: format!(
                        "the Designer check right before the project import did not pass ({err})"
                    ),
                    writes: step.writes,
                };
                return roll_back(
                    failure,
                    Some(ProjectUndo { pre: pre_zip }),
                    result,
                    loaded,
                    input,
                    surfaces,
                    opts,
                    run_id,
                    rel,
                    UndoWriters {
                        run: writers.run,
                        records: None,
                        release: None,
                    },
                    route,
                    &mut events,
                )
                .await;
            }
            Err(err) => {
                return Err(unwritten(
                    err,
                    loaded,
                    writers.run,
                    step.writes.host.as_ref(),
                    &mut events,
                )
                .await);
            }
        }
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
        // The gateway answered with a refusal, or the request never went
        // out. After tag writes the run rolls back (26-11): the project is
        // read back and restored only if it changed.
        Err(err) if tags_wrote => {
            let failure = TagFailure {
                cause: RollbackCause::ProjectImportFailed,
                what: format!("the project import was refused ({})", err.code()),
                writes: step.writes,
            };
            return roll_back(
                failure,
                Some(ProjectUndo { pre: pre_zip }),
                result,
                loaded,
                input,
                surfaces,
                opts,
                run_id,
                rel,
                UndoWriters {
                    run: writers.run,
                    records: None,
                    release: None,
                },
                route,
                &mut events,
            )
            .await;
        }
        Err(err) => {
            return Err(unwritten(
                err,
                loaded,
                writers.run,
                step.writes.host.as_ref(),
                &mut events,
            )
            .await);
        }
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

    let equal = match retry_transient(|| gateway.export_bytes(project)).await {
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
        // The project is verified and its records follow: the host journal
        // goes (26-12).
        clear_host_journal(
            &mut result,
            loaded,
            writers.run,
            step.writes.host.as_ref(),
            &mut events,
        )
        .await;
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
    } else if tags_wrote {
        // 26-11, D-07: the tags were applied and the project is not what was
        // reviewed: both are rolled back (this replaces K11 for such a run).
        let failure = TagFailure {
            cause: if answer.is_none() {
                RollbackCause::ProjectImportFailed
            } else {
                RollbackCause::ProjectUnverified
            },
            what: if equal == Some(false) {
                "the project read back does not equal the artifact".to_string()
            } else {
                "the project could not be read back or compared after its import".to_string()
            },
            writes: step.writes,
        };
        return roll_back(
            failure,
            Some(ProjectUndo { pre: pre_zip }),
            result,
            loaded,
            input,
            surfaces,
            opts,
            run_id,
            rel,
            UndoWriters {
                run: writers.run,
                records: None,
                release: None,
            },
            route,
            &mut events,
        )
        .await;
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
        // The host journal stays (26-12): the gateway is not what was
        // reviewed, and the next run refuses until it is undone.
        if step.writes.host.is_some() {
            result.warnings.push(format!(
                "the host journal {} stays; `{}` restores the project from the host copy",
                host_journal_location(loaded),
                rollback_run_command(&input.env, run_id.as_str())
            ));
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
    // 26-13: a green tag-owning run on a pair ends by proving the backup
    // holds the owned tags. The master is never undone for it.
    if result.outcome == ProjectOutcome::Applied
        && backup_step(
            &mut result,
            &owned_names,
            surfaces,
            opts,
            backup_since_ms,
            &mut events,
        )
        .await
            == Some(false)
    {
        mark_backup_unsynced(&mut result);
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
    /// What the step wrote, for the rollback of a project step that fails
    /// after it (26-11).
    writes: TagWrites,
    /// Full tag paths the step deleted through the tag-delete route (26-20).
    deleted: Vec<String>,
    /// Full tag paths of removals already gone when the step read the
    /// provider right before its deletes (26-20).
    absent: Vec<String>,
}

/// The record writers of a run: the release record and, for a tag-owning
/// project, the tag records (26-08).
#[derive(Clone, Copy)]
struct Writers<'a> {
    release: &'a dyn ReleaseRecordWriter,
    tags: Option<&'a dyn TagRecordWriter>,
    /// The host journal and pre-image (26-12), for a tag-owning plan.
    run: Option<&'a dyn ProjectRunWriter>,
}

/// One export of every owned provider, flattened, keyed by the provider.
type ProviderReads = BTreeMap<String, BTreeMap<String, Value>>;

/// Export every provider in `names` and flatten it.
async fn read_providers(
    gateway: &dyn ProjectGateway,
    names: &[&str],
) -> Result<ProviderReads, CoreError> {
    let mut out = BTreeMap::new();
    for name in names {
        out.insert((*name).to_string(), read_flat_once(gateway, name).await?);
    }
    Ok(out)
}

/// One provider's export, flattened.
async fn read_flat(
    gateway: &dyn ProjectGateway,
    name: &str,
) -> Result<BTreeMap<String, Value>, CoreError> {
    let bytes = retry_transient(|| gateway.tag_export(name)).await?;
    flatten_export(&bytes, name)
}

/// [`read_flat`] tried once: the settle loop's polls, where a transient
/// error is a read that does not agree yet.
async fn read_flat_once(
    gateway: &dyn ProjectGateway,
    name: &str,
) -> Result<BTreeMap<String, Value>, CoreError> {
    let bytes = gateway.tag_export(name).await?;
    flatten_export(&bytes, name)
}

/// Whether two reads of the owned providers agree as exports (R3-DV part 5,
/// R3-VK, R3-KEY): the same node paths, each equal in configuration.
fn reads_agree(a: &ProviderReads, b: &ProviderReads) -> bool {
    a.len() == b.len()
        && a.iter().all(|(provider, nodes)| {
            b.get(provider).is_some_and(|other| {
                nodes.len() == other.len()
                    && nodes.iter().all(|(path, node)| {
                        other
                            .get(path)
                            .is_some_and(|o| diff_properties(node, o, path, EXPORTS).is_empty())
                    })
            })
        })
}

/// Export against export (R3-DV part 5): AtomicTag runtime keys removed on
/// both sides (R3-VK), children keyed by name (R3-KEY), `defaultValue`
/// compared as configuration.
const EXPORTS: CanonOpts = CanonOpts {
    strip_values: true,
    created: false,
    exports: true,
};

/// A node the rollback re-created from its pre-image after a delete,
/// against that pre-image (26-21): R3-VK and R3-KEY as [`EXPORTS`], and
/// R3-DV with the seed check of a created node, so a `defaultValue` the
/// gateway added equal to the pre-image's `value` (C12) is not a
/// difference and any other `defaultValue` still is.
const RECREATED: CanonOpts = CanonOpts {
    strip_values: true,
    created: true,
    exports: false,
};

/// Whether `path` is one of the re-created top-most paths `tops`, or below
/// one of them.
fn recreated_at(path: &str, tops: &[String]) -> bool {
    tops.iter().any(|top| {
        path == top
            || (path.len() > top.len() + 1
                && path.starts_with(top.as_str())
                && path.as_bytes()[top.len()] == b'/')
    })
}

/// Read every provider in `names` until two consecutive reads agree
/// ([`reads_agree`]) or `settle` passes: the last read, how many reads were
/// made, whether two agreed, and the seconds from the first to the last.
async fn settled_reads(
    gateway: &dyn ProjectGateway,
    names: &[&str],
    settle: Duration,
    poll: Duration,
) -> Result<(ProviderReads, usize, bool, f64), CoreError> {
    let started = Instant::now();
    let mut reads = 1;
    let mut last = retry_transient(|| read_providers(gateway, names)).await?;
    let mut settled = false;
    loop {
        let elapsed = started.elapsed();
        if elapsed >= settle {
            break;
        }
        tokio::time::sleep(poll.min(settle - elapsed)).await;
        reads += 1;
        // A transient error is a read that does not agree yet (ultrareview
        // U6): the window goes on with the last good read.
        let next = match read_providers(gateway, names).await {
            Ok(next) => next,
            Err(err) if read_transient(&err) => continue,
            Err(err) => return Err(err),
        };
        let agree = reads_agree(&last, &next);
        last = next;
        if agree {
            settled = true;
            break;
        }
    }
    let elapsed_s = (started.elapsed().as_millis() as f64) / 1000.0;
    Ok((last, reads, settled, elapsed_s))
}

/// Why a run's automatic rollback started (26-11): the `cause` of the
/// `rollback_started` event and of `data.tags.rollback`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RollbackCause {
    /// A provider-settings write, the tag-group commit or a tag import
    /// failed, or a carried value could no longer be carried.
    TagStepFailed,
    /// The tag verify after the writes was red or could not be read.
    TagVerifyFailed,
    /// The project import was refused, or failed after it may have reached
    /// the gateway and did not read back as the artifact.
    ProjectImportFailed,
    /// The project was imported and did not read back as the artifact, or
    /// could not be read back.
    ProjectUnverified,
    /// `--rollback-run` (26-12): the undo of a run from its host copy.
    RollbackRun,
}

impl RollbackCause {
    /// The word the events and `data.tags.rollback` carry.
    pub fn as_str(self) -> &'static str {
        match self {
            RollbackCause::TagStepFailed => "tag_step_failed",
            RollbackCause::TagVerifyFailed => "tag_verify_failed",
            RollbackCause::ProjectImportFailed => "project_import_failed",
            RollbackCause::ProjectUnverified => "project_unverified",
            RollbackCause::RollbackRun => "rollback_run",
        }
    }
}

/// How the tag-group commit of a run stands, for its rollback (26-11).
enum GroupsDone {
    /// Committed, verified and finalized: the run record to restore from.
    Committed {
        /// The finalized run record.
        record: Box<RunRecord>,
        /// The providers whose groups changed.
        providers: Vec<String>,
    },
    /// The commit failed and the units were put back from the snapshot.
    Restored,
    /// The commit failed and the units could not be put back: what is left.
    Unrestored(String),
}

/// What the tag step of a run wrote, kept for its rollback (26-11). The
/// pre-images it holds are also in the run directory's `export/`.
#[derive(Default)]
struct TagWrites {
    /// The tag plan drawn against the pre-images right before the first
    /// write.
    plan: Option<TagPlan>,
    /// Each owned provider's pre-image export, flattened; none for a
    /// provider the run creates.
    pre: ProviderReads,
    /// Owned providers this run created, or may have created (a create
    /// whose answer was lost), keyed by name. A create the gateway refused
    /// (another client created the provider first) is never here, so the
    /// rollback never deletes a provider this run did not create.
    created: BTreeMap<String, CreatedProvider>,
    /// Owned providers the run sent a settings modify for, with the record
    /// read right before it (saved as `export/tags/<P>.provider.json`).
    modified: Vec<(String, TagProviderRecord)>,
    /// Per provider, the nodes of every import batch the run sent.
    applied: BTreeMap<String, AppliedSteps>,
    /// The tag-group commit, when the run made one.
    groups: Option<GroupsDone>,
    /// Every write the run sent, in order, by label: a provider's settings,
    /// the tag groups, or an import batch as `[P]path (policy, n node(s))`.
    sent: Vec<String>,
    /// Full tag paths sent with their live values carried.
    carried: Vec<String>,
    /// The tag verify, when it ran (red on a `tag_verify_failed` rollback).
    verify: Option<TagVerify>,
    /// The host copy of the run (26-12): its pre-image set and its journal,
    /// once both are on the host.
    host: Option<HostCopy>,
    /// Per provider, the nodes its tag record listed before the run: a
    /// changed node another client removed since is re-created when listed
    /// (26-12, `--rollback-run` only; empty for the automatic rollback).
    recreate: BTreeMap<String, BTreeSet<String>>,
    /// The tag records to put back once the undo verified (26-12,
    /// `--rollback-run` only): per provider its record before the run, or
    /// `None` when it had none.
    records_back: Option<Vec<(String, Option<Vec<u8>>)>>,
    /// The release record to put back once the undo verified
    /// (`--rollback-run` only, ultrareview U1): the record before the run,
    /// or `None` when the host copy holds none.
    release_back: Option<Option<Vec<u8>>>,
    /// Full tag paths of removals the export read right before the deletes
    /// no longer held (26-20): already absent, never sent.
    delete_absent: Vec<String>,
}

/// The host writers a rollback uses (26-12): the journal, and for
/// `--rollback-run` the tag records it puts back.
#[derive(Clone, Copy)]
struct UndoWriters<'a> {
    run: Option<&'a dyn ProjectRunWriter>,
    records: Option<&'a dyn TagRecordWriter>,
    /// The release record writer that puts the record before the run back
    /// (`--rollback-run` only, ultrareview U1).
    release: Option<&'a dyn ReleaseRecordWriter>,
}

/// What a run keeps on the gateway host (26-12): the pre-image set as
/// `run.json` describes it and the journal as last written.
struct HostCopy {
    run: PreImageRun,
    journal: ProjectJournal,
}

/// What the tag step needs to keep the run's host copy (26-12): the writer
/// (`None` for a test that keeps none) and the project pre-image, when the
/// run imports the project.
#[derive(Clone, Copy)]
struct HostWrites<'a> {
    writer: Option<&'a dyn ProjectRunWriter>,
    project_zip: Option<&'a [u8]>,
}

/// The pre-images a run keeps on the host (26-12).
struct PreImageInputs<'a> {
    project_zip: Option<&'a [u8]>,
    /// The project's release record before the run, as stored (ultrareview
    /// U1).
    release: Option<&'a [u8]>,
    /// Each existing owned provider's export, as the gateway sent it.
    exports: &'a BTreeMap<String, Vec<u8>>,
    /// Each owned provider's settings plan and the record read right before
    /// the run's writes.
    settings: &'a [(ProviderSettingsPlan, Option<TagProviderRecord>)],
    /// This project's tag record of each owned provider before the run.
    records: &'a BTreeMap<String, TagRecord>,
    /// The tag plan drawn against the exports.
    plan: &'a TagPlan,
    /// Whether the run commits tag-group units.
    groups: bool,
}

/// The word `run.json` keeps for a project action.
fn action_word(action: ProjectAction) -> &'static str {
    match action {
        ProjectAction::Create => "create",
        ProjectAction::Skip => "skip",
        ProjectAction::Import => "import",
        ProjectAction::Record => "record",
    }
}

/// Write the run's pre-image set to the gateway host, `run.json` last, then
/// the journal `applying` (26-12). Nothing has been written to the gateway.
async fn write_host_copy(
    writer: &dyn ProjectRunWriter,
    plan: &DeployProjectsResult,
    loaded: &LoadedProject,
    run_id: &RunId,
    pre: &PreImageInputs<'_>,
    events: &mut RunEvents<'_>,
) -> Result<HostCopy, CoreError> {
    let gateway = &loaded.target.gateway;
    let project = &loaded.file.project;
    let mut blobs: Vec<(String, std::borrow::Cow<'_, [u8]>)> = Vec::new();
    if let Some(zip) = pre.project_zip {
        blobs.push(("project.zip".to_string(), zip.into()));
    }
    if let Some(release) = pre.release {
        blobs.push((PreImageFile::Release.file_name(), release.into()));
    }
    let owned = sorted(&loaded.owned_tags);
    for provider in &owned {
        let name = provider.name.as_str();
        if let Some(bytes) = pre.exports.get(name) {
            blobs.push((format!("tags-{name}.json"), bytes.as_slice().into()));
        }
        if let Some((_, Some(record))) = pre.settings.iter().find(|(p, _)| p.provider == name) {
            let bytes = serde_json::to_vec_pretty(record).map_err(|e| {
                CoreError::Internal(format!("tag provider {name}: its record: {e}"))
            })?;
            blobs.push((format!("provider-{name}.json"), bytes.into()));
        }
        if let Some(record) = pre.records.get(name) {
            blobs.push((format!("record-{name}.json"), record.to_bytes().into()));
        }
        // What the run sends at each added path of an existing provider
        // (26-REVIEW CR-01): `--rollback-run` deletes a created node only
        // when the live node is this one.
        if pre.exports.contains_key(name)
            && let Some(provider_plan) = pre.plan.providers.iter().find(|p| p.provider == name)
            && !provider_plan.added.is_empty()
        {
            let sent: BTreeMap<&str, &Value> = provider_plan
                .added
                .iter()
                .filter_map(|path| {
                    provider
                        .nodes
                        .get(path)
                        .map(|node| (path.as_str(), &node.json))
                })
                .collect();
            let bytes = serde_json::to_vec(&sent).map_err(|e| {
                CoreError::Internal(format!("tag provider {name}: the sent nodes: {e}"))
            })?;
            blobs.push((format!("sent-{name}.json"), bytes.into()));
        }
    }
    let mut files = BTreeMap::new();
    for (name, bytes) in &blobs {
        writer
            .write_pre_image(gateway, project, run_id.as_str(), name, bytes)
            .await?;
        files.insert(name.clone(), sha256_hex(bytes));
    }
    drop(blobs);
    let providers = owned
        .iter()
        .map(|provider| {
            let name = provider.name.as_str();
            let provider_plan = pre.plan.providers.iter().find(|p| p.provider == name);
            let mut added = provider_plan.map(|p| p.added.clone()).unwrap_or_default();
            let mut changed: Vec<String> = provider_plan
                .map(|p| p.changed.iter().map(|c| c.path.clone()).collect())
                .unwrap_or_default();
            for list in [&mut added, &mut changed] {
                list.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
                list.dedup();
            }
            let mut removed: Vec<String> =
                provider_plan.map(|p| p.removed.clone()).unwrap_or_default();
            removed.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            removed.dedup();
            PreImageProvider {
                name: name.to_string(),
                settings: pre
                    .settings
                    .iter()
                    .find(|(p, _)| p.provider == name)
                    .map_or("same", |(p, _)| p.action.as_str())
                    .to_string(),
                added,
                changed,
                created: None,
                removed,
                deleted: Vec::new(),
                sent: Some(Vec::new()),
                collided: Vec::new(),
            }
        })
        .collect();
    let run = PreImageRun {
        schema_version: 1,
        gateway: gateway.clone(),
        project: project.clone(),
        run_id: run_id.as_str().to_string(),
        release: loaded.release.tag.clone(),
        written_at: rfc3339_now_utc(),
        project_action: action_word(plan.action).to_string(),
        groups: pre.groups,
        providers,
        files,
    };
    writer
        .write_pre_image(
            gateway,
            project,
            run_id.as_str(),
            "run.json",
            &run.to_bytes(),
        )
        .await?;
    events.emit(ProjectsEvent::HostPreImageWritten {
        files: run.files.len() + 1,
    })?;
    let journal = ProjectJournal {
        schema_version: 1,
        gateway: gateway.clone(),
        project: project.clone(),
        run_id: run_id.as_str().to_string(),
        state: JournalState::Applying,
        started_at: rfc3339_now_utc(),
        providers: owned.iter().map(|p| p.name.clone()).collect(),
    };
    writer
        .write_journal(gateway, project, &journal.to_bytes())
        .await?;
    events.emit(ProjectsEvent::JournalWritten {
        state: JournalState::Applying.as_str(),
    })?;
    Ok(HostCopy { run, journal })
}

/// Keep the proof of a provider this run created in the host's `run.json`
/// (26-12): the create's answer, what was sent and the signature read right
/// after it, so a rollback from the host copy deletes it on the same terms
/// as the automatic one.
async fn persist_created(
    loaded: &LoadedProject,
    writer: Option<&dyn ProjectRunWriter>,
    writes: &mut TagWrites,
    name: &str,
) -> Result<(), CoreError> {
    let TagWrites { host, created, .. } = writes;
    let (Some(writer), Some(host), Some(created)) = (writer, host.as_mut(), created.get(name))
    else {
        return Ok(());
    };
    if let Some(entry) = host.run.providers.iter_mut().find(|p| p.name == name) {
        entry.created = Some(CreatedProof {
            confirmed: created.confirmed,
            signature: created.signature.clone(),
            enabled: created.sent.0,
            description: created.sent.1.clone(),
            config: created.sent.2.clone(),
        });
    }
    writer
        .write_pre_image(
            &loaded.target.gateway,
            &loaded.file.project,
            &host.run.run_id,
            "run.json",
            &host.run.to_bytes(),
        )
        .await
}

/// Name `paths`, about to go to the tag-delete route for `name`, in the
/// host's `run.json` (26-20, rule 5): `--rollback-run` re-creates every path
/// a delete call of the run named from the pre-image beside it. Nothing to
/// do for a run that keeps no host copy.
async fn persist_deleted(
    host: Option<(&dyn ProjectRunWriter, &LoadedProject)>,
    writes: &mut TagWrites,
    name: &str,
    paths: &[String],
) -> Result<(), CoreError> {
    let (Some((writer, loaded)), Some(copy)) = (host, writes.host.as_mut()) else {
        return Ok(());
    };
    let Some(entry) = copy.run.providers.iter_mut().find(|p| p.name == name) else {
        return Err(CoreError::Internal(format!(
            "the host pre-image lists no tag provider {name}"
        )));
    };
    entry.deleted.extend(paths.iter().cloned());
    entry.deleted.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    entry.deleted.dedup();
    writer
        .write_pre_image(
            &loaded.target.gateway,
            &loaded.file.project,
            &copy.run.run_id,
            "run.json",
            &copy.run.to_bytes(),
        )
        .await
}

/// Name `paths`, the nodes of an import batch about to be sent to `name`,
/// in the host's `run.json` (26-REVIEW CR-01, the 26-20 pattern): only the
/// batches the host copy records as sent count for `--rollback-run`.
/// Nothing to do for a run that keeps no host copy.
async fn persist_sent(
    host: Option<(&dyn ProjectRunWriter, &LoadedProject)>,
    writes: &mut TagWrites,
    name: &str,
    paths: &[String],
    collided: &[String],
) -> Result<(), CoreError> {
    let (Some((writer, loaded)), Some(copy)) = (host, writes.host.as_mut()) else {
        return Ok(());
    };
    let Some(entry) = copy.run.providers.iter_mut().find(|p| p.name == name) else {
        return Err(CoreError::Internal(format!(
            "the host pre-image lists no tag provider {name}"
        )));
    };
    for (list, more) in [
        (entry.sent.get_or_insert_with(Vec::new), paths),
        (&mut entry.collided, collided),
    ] {
        list.extend(more.iter().cloned());
        list.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        list.dedup();
    }
    writer
        .write_pre_image(
            &loaded.target.gateway,
            &loaded.file.project,
            &copy.run.run_id,
            "run.json",
            &copy.run.to_bytes(),
        )
        .await
}

/// The host journal location of the project, as messages name it.
fn host_journal_location(loaded: &LoadedProject) -> String {
    project_journal_location(
        LedgerRoot::for_node(&loaded.target.nodes.master),
        &loaded.target.gateway,
        &loaded.file.project,
    )
}

/// Clear the run's host journal after a green run or a verified rollback
/// (26-12). A failure is a warning: the gateway is as the run says, and the
/// next run refuses until the journal is gone.
async fn clear_host_journal(
    result: &mut DeployProjectsResult,
    loaded: &LoadedProject,
    writer: Option<&dyn ProjectRunWriter>,
    host: Option<&HostCopy>,
    events: &mut RunEvents<'_>,
) {
    let (Some(writer), Some(_)) = (writer, host) else {
        return;
    };
    match writer
        .clear_journal(&loaded.target.gateway, &loaded.file.project)
        .await
    {
        Ok(()) => events.emit_late(ProjectsEvent::JournalCleared),
        Err(err) => result.warnings.push(format!(
            "the host journal {} could not be cleared ({}); the next run of this project refuses \
             until it is removed",
            host_journal_location(loaded),
            err.code()
        )),
    }
}

/// `err`, after the run's host journal is cleared: an error return taken
/// before any gateway write changed nothing, so the journal its host copy
/// wrote goes (26-REVIEW WR-01, ultrareview U3) and the next run is not
/// refused for it. A failed clear leaves the journal; `err` stands either
/// way.
async fn unwritten(
    err: CoreError,
    loaded: &LoadedProject,
    writer: Option<&dyn ProjectRunWriter>,
    host: Option<&HostCopy>,
    events: &mut RunEvents<'_>,
) -> CoreError {
    if let (Some(writer), Some(_)) = (writer, host)
        && writer
            .clear_journal(&loaded.target.gateway, &loaded.file.project)
            .await
            .is_ok()
    {
        events.emit_late(ProjectsEvent::JournalCleared);
    }
    err
}

/// Switch the run's host journal to `rolling_back` as its rollback starts
/// (26-12). A failure is a warning: the journal still names the run.
async fn journal_rolling_back(
    result: &mut DeployProjectsResult,
    loaded: &LoadedProject,
    writer: Option<&dyn ProjectRunWriter>,
    host: Option<&mut HostCopy>,
    events: &mut RunEvents<'_>,
) {
    let (Some(writer), Some(host)) = (writer, host) else {
        return;
    };
    if host.journal.state == JournalState::RollingBack {
        // `--rollback-run` wrote it so before its first gateway write.
        return;
    }
    host.journal.state = JournalState::RollingBack;
    host.journal.started_at = rfc3339_now_utc();
    match writer
        .write_journal(
            &loaded.target.gateway,
            &loaded.file.project,
            &host.journal.to_bytes(),
        )
        .await
    {
        Ok(()) => events.emit_late(ProjectsEvent::JournalWritten {
            state: JournalState::RollingBack.as_str(),
        }),
        Err(err) => result.warnings.push(format!(
            "the host journal {} could not be switched to rolling_back ({}); it still names this \
             run",
            host_journal_location(loaded),
            err.code()
        )),
    }
}

impl TagWrites {
    /// Whether any write was sent.
    fn wrote(&self) -> bool {
        !self.sent.is_empty()
    }
}

/// A provider the run sent a create for, and what proves it is this run's
/// (26-11 security review). The rollback deletes it only with `signature`.
struct CreatedProvider {
    /// The create answered success. `false`: the answer was lost after the
    /// request went out, so the provider may be this run's or another
    /// client's.
    confirmed: bool,
    /// What the run sent: `enabled`, `description`, `config`.
    sent: (bool, Option<String>, Value),
    /// The signature of the record read right after the run's own create
    /// (for a lost answer: only when that record held exactly what the run
    /// sent). `None`: nothing proves the provider is this run's, so it is
    /// never deleted.
    signature: Option<String>,
}

/// How many times a gateway read after the run's first write is tried
/// (26-21 open point 3, user-approved; ultrareview U6 for every such read),
/// and how long apart.
const READ_ATTEMPTS: usize = 3;
const READ_PAUSE: std::time::Duration = std::time::Duration::from_secs(2);

/// Whether `err` is worth another try of a read: the gateway's own error
/// (`internal`, which an HTTP 5xx is), a restart (503) or a timeout.
fn read_transient(err: &CoreError) -> bool {
    match err {
        CoreError::Internal(_) | CoreError::GatewayRestarting { .. } => true,
        CoreError::Network {
            source: Some(source),
            ..
        } => source.is_timeout(),
        _ => false,
    }
}

/// `read` up to [`READ_ATTEMPTS`] times [`READ_PAUSE`] apart, retrying only
/// a transient error ([`read_transient`]). The last answer stands. Every
/// gateway read after the run's first write goes through it, so one 5xx,
/// restart or timeout does not turn an applied deploy into a rollback, or a
/// rollback into `rollback_failed`.
async fn retry_transient<T, F, Fut>(mut read: F) -> Result<T, CoreError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, CoreError>>,
{
    let mut attempt = 1;
    loop {
        match read().await {
            Err(err) if attempt < READ_ATTEMPTS && read_transient(&err) => {
                attempt += 1;
                tokio::time::sleep(READ_PAUSE).await;
            }
            answer => return answer,
        }
    }
}

/// Whether `record` holds exactly the settings `sent` (`enabled`,
/// `description`, `config` whole, key order aside).
fn holds_sent(record: &TagProviderRecord, sent: &(bool, Option<String>, Value)) -> bool {
    record.enabled == sent.0
        && record.description() == sent.1.as_deref()
        && canonical(&record.config) == canonical(&sent.2)
}

/// A tag or project step that failed after the first tag write (26-11):
/// why, what failed (paths, property names and slugs only), and what the
/// run wrote.
struct TagFailure {
    cause: RollbackCause,
    what: String,
    writes: TagWrites,
}

/// How [`deploy_owned_tags`] failed.
enum TagStepError {
    /// Nothing was written: the error stands as it is.
    Unwritten(CoreError),
    /// A write was sent: the run rolls back ([`roll_back`]).
    Written(Box<TagFailure>),
}

impl From<CoreError> for TagStepError {
    fn from(err: CoreError) -> Self {
        TagStepError::Unwritten(err)
    }
}

/// How the write half of the tag step stopped.
enum Stop {
    /// An error. Before the first write it stands as it is; after it the run
    /// rolls back (`tag_step_failed`).
    Error(CoreError),
    /// A failed step after the first write: why, and what failed.
    Failed(RollbackCause, String),
}

impl From<CoreError> for Stop {
    fn from(err: CoreError) -> Self {
        Stop::Error(err)
    }
}

/// The tag step of a tag-owning run (D-07), before the project is touched:
/// save every owned provider's export as the pre-image, re-plan against it
/// and refuse a plan that is no longer the reviewed one, refuse a carried
/// `Overwrite` that would reset a live value ([`uncarriable_in_batch`]),
/// then [`send_owned_tags`].
///
/// A failure before the first write returns the error as it is
/// ([`TagStepError::Unwritten`]): nothing was written. A failure after it is
/// [`TagStepError::Written`] with everything the run wrote, and the caller
/// rolls the run back (26-11).
#[allow(clippy::too_many_arguments)]
async fn deploy_owned_tags(
    plan: &DeployProjectsResult,
    tag_records: &BTreeMap<String, TagRecord>,
    loaded: &LoadedProject,
    input: &ProjectsInput,
    surfaces: &ProjectsSurfaces<'_>,
    opts: &ProjectsRunOptions<'_>,
    run_dir: &RunDir,
    run_id: &RunId,
    host: HostWrites<'_>,
    route: &RouteState<'_>,
    events: &mut RunEvents<'_>,
) -> Result<TagStep, TagStepError> {
    let gateway = surfaces.gateway;
    let Some(report) = &plan.tags else {
        return Ok(TagStep {
            carried: Vec::new(),
            verify: None,
            batches_sent: 0,
            other_writes: 0,
            writes: TagWrites::default(),
            deleted: Vec::new(),
            absent: Vec::new(),
        });
    };
    // The provider settings the plan was drawn against are the ones about to
    // change (26-10): each owned provider's record is read again, and the
    // live record of a provider the run modifies is its pre-image.
    let settings = fresh_provider_settings(loaded, gateway, report, run_dir, run_id).await?;
    let creating: BTreeSet<&str> = report
        .provider_settings
        .iter()
        .filter(|p| p.action == ProviderSettingsAction::Create)
        .map(|p| p.provider.as_str())
        .collect();
    // Pre-images of every owned provider, before the first write. Tag values
    // can hold secrets, so the files are 0600 inside `export/`. A provider
    // the run creates has no tree yet.
    let mut live = BTreeMap::new();
    let mut raw: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for provider in &loaded.owned_tags {
        if creating.contains(provider.name.as_str()) {
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
        raw.insert(provider.name.clone(), bytes);
    }
    // The tags the plan was drawn against are the ones about to change.
    let now = plan_tags(&loaded.owned_tags, &live, tag_records);
    if now.live_sha256 != report.live_sha256 {
        return Err(CoreError::PlanStale {
            expected: report.live_sha256.clone(),
            actual: now.live_sha256,
            detail: "the owned tag providers changed since the plan".to_string(),
        }
        .into());
    }
    // The tag groups the plan was drawn against are the ones about to change
    // (26-10), and no apply journal is on the box.
    let groups = fresh_groups(loaded, input, surfaces, report, &now, tag_records, &live).await?;
    // Every batch of every provider is built before the first write, so a
    // plan the batches refuse (a type cycle) writes nothing.
    let empty = BTreeMap::new();
    let owned = sorted(&loaded.owned_tags);
    let mut queue: VecDeque<ImportBatch> = VecDeque::new();
    let mut over_cap = Vec::new();
    for (provider_plan, provider) in now.providers.iter().zip(&owned) {
        let nodes = live.get(&provider.name).unwrap_or(&empty);
        queue.extend(import_batches(
            provider_plan,
            provider,
            nodes,
            REMOVED_PROPERTY_POLICY,
        )?);
        // A node the rollback could not send back within the cap is refused
        // now, before any write (26-REVIEW WR-03).
        over_cap.extend(rollback_over_cap(provider_plan, provider, nodes));
    }
    if !over_cap.is_empty() {
        return Err(CoreError::deploy_findings(over_cap).into());
    }
    // The carry guard (26-11, T-26-91, KL-5): a carried `Overwrite` that
    // would reset a live value whose `valueSource` does not resolve is
    // refused against the pre-image, before any write.
    let mut refused = Vec::new();
    for batch in queue.iter().filter(|batch| batch.carry) {
        let nodes = live.get(&batch.provider).unwrap_or(&empty);
        for (path, found) in uncarriable_in_batch(batch, nodes)? {
            let file = owned
                .iter()
                .find(|p| p.name == batch.provider)
                .and_then(|p| p.nodes.get(&path))
                .map_or_else(
                    || full_tag_path(&batch.provider, &path),
                    |node| node.file.clone(),
                );
            refused.push(DeployFinding {
                file,
                line: None,
                column: None,
                message: format!(
                    "{} hold(s) a live value and a valueSource that names no source (not a \
                     string): the repo drops a property, so the tag would go Overwrite, which \
                     cannot carry that value and would reset it; nothing was written",
                    found
                        .iter()
                        .map(|p| full_tag_path(&batch.provider, p))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            });
        }
    }
    if !refused.is_empty() {
        return Err(CoreError::deploy_findings(refused).into());
    }

    // The host copy (26-12): the pre-image set, `run.json` last, then the
    // journal, all before the first gateway write. A failure here stops the
    // run with nothing written to the gateway.
    let mut writes = TagWrites::default();
    if let Some(writer) = host.writer {
        let release = surfaces
            .host
            .read_release_record(&loaded.target.gateway, &loaded.file.project)
            .await?;
        let pre = PreImageInputs {
            project_zip: host.project_zip,
            release: release.as_deref(),
            exports: &raw,
            settings: &settings,
            records: tag_records,
            plan: &now,
            groups: groups.as_ref().is_some_and(GroupWork::would_change),
        };
        writes.host = Some(write_host_copy(writer, plan, loaded, run_id, &pre, events).await?);
    }
    drop(raw);
    let outcome = send_owned_tags(
        loaded,
        surfaces,
        opts,
        run_id,
        report,
        &settings,
        groups.as_ref(),
        &now,
        &live,
        queue,
        &mut writes,
        host.writer,
        route,
        events,
    )
    .await;
    writes.plan = Some(now);
    writes.pre = live;
    match outcome {
        Ok((verify, other_writes)) => {
            let mut carried = writes.carried.clone();
            carried.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            let mut deleted: Vec<String> = writes
                .applied
                .iter()
                .flat_map(|(name, applied)| applied.deleted.iter().map(|p| full_tag_path(name, p)))
                .collect();
            deleted.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            let absent = writes.delete_absent.clone();
            Ok(TagStep {
                carried,
                verify: Some(verify),
                batches_sent: writes.sent.len().saturating_sub(other_writes),
                other_writes,
                writes,
                deleted,
                absent,
            })
        }
        Err(Stop::Error(err)) if !writes.wrote() => {
            // Nothing reached the gateway (26-REVIEW WR-01).
            Err(TagStepError::Unwritten(
                unwritten(err, loaded, host.writer, writes.host.as_ref(), events).await,
            ))
        }
        Err(Stop::Error(err)) => Err(TagStepError::Written(Box::new(TagFailure {
            cause: RollbackCause::TagStepFailed,
            what: format!("the tag step stopped ({})", err.code()),
            writes,
        }))),
        Err(Stop::Failed(cause, what)) => Err(TagStepError::Written(Box::new(TagFailure {
            cause,
            what,
            writes,
        }))),
    }
}

/// The write half of the tag step: the provider settings, then the tag
/// groups, then every batch of `queue` in order (a carried batch after a
/// fresh export of its provider, guarded again against it), then the verify
/// of every owned provider ([`TAG_VERIFY_POLL_S`], [`TAG_VERIFY_SETTLE_S`]).
/// Every write is recorded in `writes` before it is sent. Returns the green
/// verify and how many writes were not import batches.
#[allow(clippy::too_many_arguments)]
async fn send_owned_tags(
    loaded: &LoadedProject,
    surfaces: &ProjectsSurfaces<'_>,
    opts: &ProjectsRunOptions<'_>,
    run_id: &RunId,
    report: &TagsReport,
    settings: &[(ProviderSettingsPlan, Option<TagProviderRecord>)],
    groups: Option<&GroupWork>,
    now: &TagPlan,
    live: &ProviderReads,
    mut queue: VecDeque<ImportBatch>,
    writes: &mut TagWrites,
    run_writer: Option<&dyn ProjectRunWriter>,
    route: &RouteState<'_>,
    events: &mut RunEvents<'_>,
) -> Result<(TagVerify, usize), Stop> {
    let gateway = surfaces.gateway;
    let failed = RollbackCause::TagStepFailed;
    // Provider settings first (26-10, the git module's order plus D-07): a
    // provider is created or modified, then read again and compared.
    let repos = settings
        .iter()
        .map(|(planned, _)| provider_settings_of(loaded, &planned.provider))
        .collect::<Result<Vec<_>, _>>()?;
    let mut other_writes = 0;
    for ((planned, record), repo) in settings.iter().zip(repos) {
        if planned.action == ProviderSettingsAction::Same {
            continue;
        }
        let name = planned.provider.as_str();
        let answer = match (planned.action, record) {
            (ProviderSettingsAction::Create, _) => {
                let sent = (repo.enabled, repo.description.clone(), repo.config.clone());
                let answer = gateway
                    .tag_provider_create(&[TagProviderCreate::with_settings(
                        name,
                        repo.enabled,
                        repo.description.clone(),
                        repo.config.clone(),
                    )])
                    .await;
                // Only a create this run made is the rollback's to delete
                // (26-11 security review). A refusal (`resource_exists`:
                // another client created it first) is not. A lost answer
                // is recorded with the signature of a read right after it
                // only when that read holds exactly what the run sent.
                match &answer {
                    Ok(()) => {
                        writes.created.insert(
                            name.to_string(),
                            CreatedProvider {
                                confirmed: true,
                                sent,
                                signature: None,
                            },
                        );
                        // The host records the create at once, before the
                        // read below (ultrareview U2): a read that never
                        // answers, or a process that dies here, still
                        // leaves the proof for `--rollback-run`. Without a
                        // signature it deletes nothing. A failed write is
                        // caught by the write after the read.
                        let _ = persist_created(loaded, run_writer, writes, name).await;
                    }
                    Err(err) if import_outcome_unknown(err) => {
                        let signature =
                            match retry_transient(|| gateway.tag_provider_record(name)).await {
                                Ok(Some(record)) if holds_sent(&record, &sent) => record.signature,
                                _ => None,
                            };
                        writes.created.insert(
                            name.to_string(),
                            CreatedProvider {
                                confirmed: false,
                                sent,
                                signature,
                            },
                        );
                        // The proof goes to the host too (26-12); the run
                        // stops right below, and its rollback has it in
                        // memory whatever this write did.
                        let _ = persist_created(loaded, run_writer, writes, name).await;
                    }
                    Err(_) => {}
                }
                answer
            }
            (ProviderSettingsAction::Modify, Some(record)) => {
                writes.modified.push((name.to_string(), record.clone()));
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
            _ => {
                return Err(Stop::Error(CoreError::Internal(format!(
                    "tag provider {name} is planned to be modified but has no record"
                ))));
            }
        };
        // The request may have reached the gateway whatever came back.
        writes.sent.push(format!(
            "[{name}] provider settings ({})",
            planned.action.as_str()
        ));
        other_writes += 1;
        if let Err(err) = answer {
            return Err(Stop::Failed(
                failed,
                format!(
                    "the settings of tag provider {name} were not applied ({})",
                    err.code()
                ),
            ));
        }
        // A transient error is retried, bounded, so right after the run's
        // own create the signature the rollback deletes with is still
        // captured under load (26-21 open point 3).
        let reread = retry_transient(|| gateway.tag_provider_record(name))
            .await
            .map_err(|err| {
            Stop::Failed(
                failed,
                format!(
                    "tag provider {name} could not be read after its settings were applied ({})",
                    err.code()
                ),
            )
        })?;
        // The signature right after the run's own create: the only one the
        // rollback may delete it with.
        if let Some(created) = writes.created.get_mut(name) {
            created.signature = reread.as_ref().and_then(|r| r.signature.clone());
            // ...kept on the host as well, so `--rollback-run` deletes the
            // provider with this signature and no other (26-12).
            if let Err(err) = persist_created(loaded, run_writer, writes, name).await {
                return Err(Stop::Failed(
                    failed,
                    format!(
                        "the host pre-image could not record tag provider {name}, which this run \
                         created ({})",
                        err.code()
                    ),
                ));
            }
        }
        let check = plan_provider_settings(name, repo, reread.as_ref());
        if check.action != ProviderSettingsAction::Same {
            return Err(Stop::Failed(
                failed,
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
    if let Some(work) = groups.filter(|work| work.would_change()) {
        let providers = work
            .plans
            .iter()
            .filter(|p| p.would_change())
            .map(|p| p.provider.clone())
            .collect::<Vec<_>>();
        let label = format!("tag groups of {}", providers.join(", "));
        match commit_groups(loaded, surfaces, run_id, &report.plan_sha256, work).await {
            Ok((written, deleted, record)) => {
                writes
                    .sent
                    .push(format!("{label} ({written} written, {deleted} deleted)"));
                other_writes += 1;
                writes.groups = Some(GroupsDone::Committed {
                    record: Box::new(record),
                    providers: providers.clone(),
                });
                events.emit_late(ProjectsEvent::TagGroupsWritten {
                    providers,
                    written,
                    deleted,
                });
            }
            Err(GroupFailure::NotWritten(err)) if !writes.wrote() => return Err(err.into()),
            Err(GroupFailure::NotWritten(err)) => {
                return Err(Stop::Failed(
                    failed,
                    format!("the {label} were not written ({})", err.code()),
                ));
            }
            Err(GroupFailure::Stale) if !writes.wrote() => {
                return Err(CoreError::PlanStale {
                    expected: "the planned tag-group units".to_string(),
                    actual: "a unit changed before the commit".to_string(),
                    detail: "the owned providers' tag-group units changed since the plan"
                        .to_string(),
                }
                .into());
            }
            Err(GroupFailure::Stale) => {
                return Err(Stop::Failed(
                    failed,
                    format!("the {label} were not written: a unit changed since the plan"),
                ));
            }
            Err(GroupFailure::Restored(why)) => {
                writes.sent.push(format!("{label} (restored)"));
                writes.groups = Some(GroupsDone::Restored);
                return Err(Stop::Failed(
                    failed,
                    format!(
                        "the {label} did not verify ({why}); the group units were restored from \
                         the run's snapshot"
                    ),
                ));
            }
            Err(GroupFailure::Unrestored(why)) => {
                writes.sent.push(format!("{label} (not restored)"));
                let journal = journal_location(
                    LedgerRoot::for_node(&loaded.target.nodes.master),
                    &loaded.target.gateway,
                    &loaded.target.mode,
                );
                writes.groups = Some(GroupsDone::Unrestored(format!(
                    "the {label} failed and could not be restored ({why}); the on-box journal \
                     {journal} stays"
                )));
                return Err(Stop::Failed(
                    failed,
                    format!("the {label} failed and could not be restored ({why})"),
                ));
            }
        }
    }

    while let Some(batch) = queue.pop_front() {
        let name = batch.provider.clone();
        let body = if batch.carry {
            // R3-CARRY: the values come from an export read right now.
            let fresh = match read_flat(gateway, &name).await {
                Ok(fresh) => fresh,
                Err(err) if !writes.wrote() => return Err(err.into()),
                Err(err) => {
                    return Err(Stop::Failed(
                        failed,
                        format!(
                            "provider {name} could not be read to carry the live values ({})",
                            err.code()
                        ),
                    ));
                }
            };
            // The carry guard again, against the export the values come
            // from: a value that can no longer be carried stops the step.
            let refused = uncarriable_in_batch(&batch, &fresh)?;
            if !refused.is_empty() {
                let paths: Vec<String> = refused
                    .iter()
                    .flat_map(|(_, found)| found.iter().map(|p| full_tag_path(&name, p)))
                    .collect();
                let what = format!(
                    "{} hold(s) a live value and a valueSource that names no source in the \
                     export read right before their Overwrite, which would reset it, so the \
                     batch was not sent",
                    paths.join(", ")
                );
                if !writes.wrote() {
                    return Err(CoreError::deploy_tree_invalid(
                        full_tag_path(&name, &batch.path),
                        None,
                        None,
                        what,
                    )
                    .into());
                }
                return Err(Stop::Failed(failed, what));
            }
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
            if writes.wrote() {
                events.emit_late(event);
            } else {
                events.emit(event)?;
            }
            writes
                .carried
                .extend(batch.nodes.iter().map(|p| full_tag_path(&name, p)));
        }
        let event = ProjectsEvent::TagImportSent {
            provider: name.clone(),
            path: batch.path.clone(),
            policy: batch.policy.as_wire(),
            nodes: batch.nodes.len(),
            carry: batch.carry,
        };
        if writes.wrote() {
            events.emit_late(event);
        } else {
            events.emit(event)?;
        }
        // On the host first (26-REVIEW CR-01): `--rollback-run` counts only
        // the batches recorded as sent. A set that cannot record the batch
        // stops the step before it goes out.
        let host = run_writer.map(|writer| (writer, loaded));
        if let Err(err) = persist_sent(host, writes, &name, &batch.nodes, &[]).await {
            if !writes.wrote() {
                return Err(Stop::Error(err));
            }
            return Err(Stop::Failed(
                failed,
                format!(
                    "the host pre-image could not record the tag import into provider {name} \
                     ({}), so it was not sent",
                    err.code()
                ),
            ));
        }
        let answer = gateway
            .tag_import(&name, &batch.path, batch.policy, body)
            .await;
        // The request may have reached the gateway whatever came back, and a
        // batch that is not clean may have applied part of its nodes. What
        // it sent at each added path is kept, so a rollback deletes a
        // created node only when it is this one (26-REVIEW CR-01).
        let sent_nodes = sent_bodies(loaded, now, &name, &batch);
        let applied = writes.applied.entry(name.clone()).or_default();
        applied.sent(&batch);
        applied.bodies.extend(sent_nodes);
        writes.sent.push(format!(
            "{} ({}, {} node(s))",
            full_tag_path(&name, &batch.path),
            batch.policy.as_wire(),
            batch.nodes.len()
        ));
        let answer = answer.map_err(|err| {
            Stop::Failed(
                failed,
                format!(
                    "the tag import into provider {name} failed ({})",
                    err.code()
                ),
            )
        })?;
        let clean = answer.is_clean();
        events.emit_late(ProjectsEvent::TagImportAnswered {
            provider: name.clone(),
            clean,
        });
        // An `Abort` collision is another client's node (26-REVIEW CR-01):
        // the paths the answer names, or every node of the batch when a
        // collision names none.
        let collided = collided_paths(&name, &batch, &answer);
        if !collided.is_empty() {
            if let Some(applied) = writes.applied.get_mut(&name) {
                applied.collided.extend(collided.iter().cloned());
            }
            // Kept on the host too; the run stops right below, and its
            // rollback has them in memory whatever this write did.
            let _ = persist_sent(host, writes, &name, &[], &collided).await;
        }
        if !clean {
            let mut codes = answer.bad_quality.clone();
            codes.truncate(TAG_FAILURE_PATHS_SHOWN);
            let first = batch.nodes.first().map_or_else(
                || full_tag_path(&name, &batch.path),
                |p| full_tag_path(&name, p),
            );
            return Err(Stop::Failed(
                failed,
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

    // Then the removals (26-20, D-02): after every create and change, before
    // the verify, per provider from an export read right now.
    let mut absent = Vec::new();
    let host = run_writer.map(|writer| (writer, loaded));
    let sent = delete_removed(gateway, now, route, writes, host, &mut absent, events).await;
    writes.delete_absent = absent;
    other_writes += sent?;

    // Verify: read every owned provider until two reads agree or the bound
    // passes, then compare the last read.
    let owned = sorted(&loaded.owned_tags);
    let names: Vec<&str> = owned.iter().map(|p| p.name.as_str()).collect();
    let wrote = writes.wrote();
    let (last, reads, settled, elapsed_s) = settled_reads(
        gateway,
        &names,
        opts.tag_verify_settle,
        opts.tag_verify_poll,
    )
    .await
    .map_err(|err| {
        if wrote {
            Stop::Failed(
                RollbackCause::TagVerifyFailed,
                format!(
                    "the owned tag providers could not be read back to verify ({})",
                    err.code()
                ),
            )
        } else {
            Stop::Error(err)
        }
    })?;

    let empty = BTreeMap::new();
    let mut mismatches = Vec::new();
    for (provider_plan, provider) in now.providers.iter().zip(&owned) {
        let name = provider.name.as_str();
        let after = last.get(name).unwrap_or(&empty);
        let pre = live.get(name).unwrap_or(&empty);
        let created: BTreeSet<&str> = provider_plan.added.iter().map(String::as_str).collect();
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
        // D-02: every removed node is gone (26-20).
        for path in &provider_plan.removed {
            if after.contains_key(path) {
                differ.push(format!("{} (present)", full_tag_path(name, path)));
            }
        }
        // D-01: every live node the repo lacks is as the pre-image held it.
        for path in &provider_plan.drift {
            let properties = match (pre.get(path), after.get(path)) {
                (Some(before), Some(now)) => diff_properties(before, now, path, EXPORTS),
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
    let what = (mismatches_count > 0).then(|| {
        let shown: Vec<&str> = mismatches
            .iter()
            .take(TAG_FAILURE_PATHS_SHOWN)
            .map(String::as_str)
            .collect();
        format!(
            "the tag verify found {mismatches_count} node(s) unlike the repo or their pre-image \
             after {reads} read(s){}: {}",
            if settled { "" } else { ", unsettled" },
            shown.join("; ")
        )
    });
    let mut capped = mismatches;
    capped.truncate(LIST_CAP);
    let verify = TagVerify {
        equal: mismatches_count == 0,
        settled,
        reads,
        elapsed_s,
        mismatches: capped,
        mismatches_count,
    };
    writes.verify = Some(verify.clone());
    if let Some(what) = what {
        if !wrote {
            // Nothing was written, so the providers changed under the run.
            return Err(CoreError::PlanStale {
                expected: "the owned tags as planned".to_string(),
                actual: format!("{mismatches_count} node(s) unlike the repo"),
                detail: what,
            }
            .into());
        }
        return Err(Stop::Failed(RollbackCause::TagVerifyFailed, what));
    }
    Ok((verify, other_writes))
}

/// The nodes `batch` sends at the added paths of `provider`'s plan in `now`,
/// as the repo holds them (flattened): what a created node must equal to be
/// the run's (26-REVIEW CR-01).
fn sent_bodies(
    loaded: &LoadedProject,
    now: &TagPlan,
    provider: &str,
    batch: &ImportBatch,
) -> Vec<(String, Value)> {
    let Some(plan) = now.providers.iter().find(|p| p.provider == provider) else {
        return Vec::new();
    };
    let Some(repo) = loaded.owned_tags.iter().find(|p| p.name == provider) else {
        return Vec::new();
    };
    batch
        .nodes
        .iter()
        // The plan lists its added paths in byte order (the repo's map).
        .filter(|path| plan.added.binary_search(path).is_ok())
        .filter_map(|path| {
            repo.nodes
                .get(path)
                .map(|node| (path.clone(), node.json.clone()))
        })
        .collect()
}

/// The provider-relative paths of `batch` that its `answer` says already
/// existed (an `Abort` collision, 26-REVIEW CR-01): the paths the answer
/// names that the batch carries, or every node of the batch when a
/// collision names no path or one outside the batch.
fn collided_paths(provider: &str, batch: &ImportBatch, answer: &TagImportAnswer) -> Vec<String> {
    let prefix = format!("[{provider}]");
    let mut out = Vec::new();
    let mut whole = answer.collisions_unnamed > 0;
    for full in &answer.collided {
        match full.strip_prefix(&prefix) {
            Some(path) if batch.nodes.iter().any(|n| n == path) => out.push(path.to_string()),
            _ => whole = true,
        }
    }
    if whole {
        return batch.nodes.clone();
    }
    out.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    out.dedup();
    out
}

/// The delete step of a run (26-20, D-02, T-26-81, T-26-83): for each owned
/// provider whose plan removes nodes, an export read right now, then the
/// removed paths that export still holds through the master's tag-delete
/// route, as [`delete_calls`] sets them (calls of at most the route's
/// `maxPaths`; a removed folder that gained a live node, or a removed type
/// a surviving node uses, stops the step before anything of the provider is
/// sent). Removed paths the export no longer holds go to `absent`.
/// Every path of a call goes to `writes` (`AppliedSteps::deleted`) before
/// the call is sent, so a rollback re-creates it whatever came back; each
/// call is logged (`tag_delete_sent`, `rollback: false`) and its answer
/// (`tag_delete_answered`) checked against the request. The first failure
/// stops the step (`tag_step_failed`). Returns how many calls were sent.
async fn delete_removed(
    gateway: &dyn ProjectGateway,
    now: &TagPlan,
    route: &RouteState<'_>,
    writes: &mut TagWrites,
    host: Option<(&dyn ProjectRunWriter, &LoadedProject)>,
    absent: &mut Vec<String>,
    events: &mut RunEvents<'_>,
) -> Result<usize, Stop> {
    let failed = RollbackCause::TagStepFailed;
    let mut calls = 0;
    for provider_plan in now.providers.iter().filter(|p| !p.removed.is_empty()) {
        let name = provider_plan.provider.as_str();
        let Some((client, handshake)) = route.ready() else {
            // The run refused this before any write; reaching here is a bug.
            return Err(Stop::Error(CoreError::Internal(format!(
                "tag provider {name} has removals and the run has no ready tag-delete route"
            ))));
        };
        let fresh = match read_flat(gateway, name).await {
            Ok(fresh) => fresh,
            Err(err) if !writes.wrote() => return Err(err.into()),
            Err(err) => {
                return Err(Stop::Failed(
                    failed,
                    format!(
                        "provider {name} could not be read before its deletes ({})",
                        err.code()
                    ),
                ));
            }
        };
        // Gone since the plan: already absent, nothing to send (26-20).
        absent.extend(
            provider_plan
                .removed
                .iter()
                .filter(|path| !fresh.contains_key(*path))
                .map(|path| full_tag_path(name, path)),
        );
        let calls_of = match delete_calls(provider_plan, &fresh, handshake, TAG_DELETE_DESCENDANTS)
        {
            Ok(calls_of) => calls_of,
            Err(refused) => {
                let what = refused.describe(name);
                if !writes.wrote() {
                    // Nothing was written: the provider changed under the
                    // run since its plan.
                    return Err(Stop::Error(CoreError::PlanStale {
                        expected: "the owned tags as planned".to_string(),
                        actual: "a removal no longer holds".to_string(),
                        detail: what,
                    }));
                }
                return Err(Stop::Failed(failed, what));
            }
        };
        for chunk in calls_of.into_iter().map(|call| call.paths) {
            // On the host first (rule 5): `--rollback-run` on another runner
            // re-creates every path a call named. A set that cannot record
            // the call stops the step before the call goes out.
            if let Err(err) = persist_deleted(host, writes, name, &chunk).await {
                let what = format!(
                    "the host pre-image could not record the deletes in provider {name} ({}), so \
                     they were not sent",
                    err.code()
                );
                if !writes.wrote() {
                    return Err(Stop::Error(err));
                }
                return Err(Stop::Failed(failed, what));
            }
            // Recorded before it is sent: the call may reach the gateway
            // whatever comes back.
            writes
                .applied
                .entry(name.to_string())
                .or_default()
                .deleted
                .extend(chunk.iter().cloned());
            writes
                .sent
                .push(format!("[{name}] tag delete ({} path(s))", chunk.len()));
            calls += 1;
            events.emit_late(ProjectsEvent::TagDeleteSent {
                provider: name.to_string(),
                paths: chunk.clone(),
                rollback: false,
            });
            let answer = match client.delete(name, &chunk).await {
                Ok(answer) => answer.check(name, &chunk).map(|()| answer),
                Err(err) => Err(err),
            };
            let answer = answer.map_err(|err| {
                Stop::Failed(
                    failed,
                    format!("the tag delete in provider {name} failed ({})", err.code()),
                )
            })?;
            let clean = answer.is_clean();
            let removed = answer
                .results
                .iter()
                .filter(|r| r.good != Some(false) && !r.exists_after)
                .count();
            events.emit_late(ProjectsEvent::TagDeleteAnswered {
                provider: name.to_string(),
                clean,
                deleted: removed,
            });
            if !clean {
                let kept: Vec<String> = answer
                    .results
                    .iter()
                    .filter(|r| r.good == Some(false) || r.exists_after)
                    .take(TAG_FAILURE_PATHS_SHOWN)
                    .map(|r| {
                        format!(
                            "{} (quality {})",
                            full_tag_path(name, &r.path),
                            r.quality.as_deref().unwrap_or("none")
                        )
                    })
                    .collect();
                return Err(Stop::Failed(
                    failed,
                    format!("the tag-delete route did not remove {}", kept.join(", ")),
                ));
            }
        }
    }
    Ok(calls)
}

/// The project half of a rollback (26-11): the import was sent, with the
/// saved pre-image when it was an overwrite (`None` for a created project).
struct ProjectUndo {
    pre: Option<Vec<u8>>,
}

/// The canonical sha256 of `project` as the gateway exports it now, or
/// `None` when it cannot be read or digested.
async fn project_sha(gateway: &dyn ProjectGateway, project: &str) -> Option<String> {
    let zip = retry_transient(|| gateway.export_bytes(project))
        .await
        .ok()?;
    live_digest(project, &zip).ok().map(|digest| digest.sha256)
}

/// Whether two provider records hold the same settings: `enabled`,
/// `description` and `config` whole (key order aside), as the settings plan
/// compares them (26-10).
fn settings_equal(a: &TagProviderRecord, b: &TagProviderRecord) -> bool {
    a.enabled == b.enabled
        && a.description() == b.description()
        && canonical(&a.config) == canonical(&b.config)
}

/// Put an owned provider's settings back to `pre`, the record the run read
/// right before its modify (`export/tags/<P>.provider.json`), with the
/// signature of a fresh read, and read it back. `Ok(false)` when it already
/// holds them (the modify did not land). `Err` is a value-free reason.
async fn restore_provider_settings(
    gateway: &dyn ProjectGateway,
    name: &str,
    pre: &TagProviderRecord,
) -> Result<bool, String> {
    let now = retry_transient(|| gateway.tag_provider_record(name))
        .await
        .map_err(|e| format!("it could not be read ({})", e.code()))?
        .ok_or_else(|| "it is no longer listed".to_string())?;
    if settings_equal(&now, pre) {
        return Ok(false);
    }
    gateway
        .tag_provider_modify(&[TagProviderModify {
            name: name.to_string(),
            collection: pre
                .collection()
                .or(now.collection())
                .unwrap_or("core")
                .to_string(),
            enabled: pre.enabled,
            description: pre.description().map(str::to_string),
            config: pre.config.clone(),
            signature: now.signature.clone().unwrap_or_default(),
        }])
        .await
        .map_err(|e| format!("the modify back failed ({})", e.code()))?;
    match retry_transient(|| gateway.tag_provider_record(name)).await {
        Ok(Some(back)) if settings_equal(&back, pre) => Ok(true),
        Ok(_) => Err("it does not read back as its pre-image".to_string()),
        Err(e) => Err(format!("it could not be read back ({})", e.code())),
    }
}

/// Delete an owned provider the run created (26-11, the rollback of a
/// create); the tags the run put in it go with it. It is deleted only with
/// the signature captured right after the run's own create, and only while
/// the live record still carries it (and, for a create whose answer was
/// lost, still holds exactly what the run sent): a provider another client
/// created or changed since is left, and the reason says so. Never a
/// signature from a later read. `Ok(false)` when the gateway does not list
/// it (the create did not land). `Err` is a value-free reason.
async fn delete_created_provider(
    gateway: &dyn ProjectGateway,
    name: &str,
    created: &CreatedProvider,
) -> Result<bool, String> {
    let Some(record) = retry_transient(|| gateway.tag_provider_record(name))
        .await
        .map_err(|e| format!("it could not be read ({})", e.code()))?
    else {
        return Ok(false);
    };
    let Some(signature) = created.signature.as_deref() else {
        return Err(if created.confirmed {
            "its record could not be read right after the create, so nothing proves it is this \
             run's; it was left in place"
                .to_string()
        } else {
            "the create's answer was lost and the provider read right after it did not hold \
             what this run sent, so it may be another client's; it was left in place"
                .to_string()
        });
    };
    if record.signature.as_deref() != Some(signature) {
        return Err(
            "it changed since this run's create (its signature is not the one read right after \
             the create), so it was left in place"
                .to_string(),
        );
    }
    if !created.confirmed && !holds_sent(&record, &created.sent) {
        return Err(
            "the create's answer was lost and the provider no longer holds what this run sent, \
             so it was left in place"
                .to_string(),
        );
    }
    gateway
        .tag_provider_delete(name, signature)
        .await
        .map_err(|e| format!("its delete failed ({})", e.code()))?;
    match retry_transient(|| gateway.tag_provider_record(name)).await {
        Ok(None) => Ok(true),
        Ok(Some(_)) => Err("it is still listed after the delete".to_string()),
        Err(e) => Err(format!("its delete could not be read back ({})", e.code())),
    }
}

/// Remove `paths` (provider-relative, the nodes the run created in
/// `provider`, in [`rollback_plan`]'s order) through the master's tagDelete
/// route (26-19, D-13 item 9, T-26-79): in calls of at most the route's
/// `maxPaths` ([`delete_chunks`]), each logged with its provider and paths
/// (`tag_delete_sent`, `rollback: true`) and its answer (`tag_delete_answered`),
/// never a value. Every answer is checked against its request
/// ([`crate::deploy::tag_delete::DeleteAnswer::check`]). Returns the full
/// paths removed and, for the others, `(full path, why)`: a path whose call
/// failed or whose result is not a clean delete.
async fn delete_created(
    route: &dyn TagDeleteRoute,
    handshake: &RouteHandshake,
    provider: &str,
    paths: &[String],
    events: &mut RunEvents<'_>,
) -> (Vec<String>, Vec<(String, String)>) {
    let mut removed = Vec::new();
    let mut kept = Vec::new();
    let chunks = match delete_chunks(paths, handshake) {
        Ok(chunks) => chunks,
        Err(err) => {
            let why = format!(
                "created by this run; it was not sent to the tag-delete route ({})",
                err.code()
            );
            kept.extend(
                paths
                    .iter()
                    .map(|p| (full_tag_path(provider, p), why.clone())),
            );
            return (removed, kept);
        }
    };
    for chunk in chunks.into_iter().filter(|chunk| !chunk.is_empty()) {
        events.emit_late(ProjectsEvent::TagDeleteSent {
            provider: provider.to_string(),
            paths: chunk.clone(),
            rollback: true,
        });
        let answer = match route.delete(provider, &chunk).await {
            Ok(answer) => answer.check(provider, &chunk).map(|()| answer),
            Err(err) => Err(err),
        };
        match answer {
            Ok(answer) => {
                let mut count = 0;
                for result in &answer.results {
                    let full = full_tag_path(provider, &result.path);
                    if result.good != Some(false) && !result.exists_after {
                        count += 1;
                        removed.push(full);
                    } else {
                        kept.push((
                            full,
                            format!(
                                "created by this run; the tag-delete route did not remove it \
                                 (quality {})",
                                result.quality.as_deref().unwrap_or("none")
                            ),
                        ));
                    }
                }
                events.emit_late(ProjectsEvent::TagDeleteAnswered {
                    provider: provider.to_string(),
                    clean: answer.is_clean(),
                    deleted: count,
                });
            }
            Err(err) => {
                let why = format!(
                    "created by this run; its delete call failed ({})",
                    err.code()
                );
                kept.extend(
                    chunk
                        .iter()
                        .map(|p| (full_tag_path(provider, p), why.clone())),
                );
            }
        }
    }
    (removed, kept)
}

/// `items` byte-sorted and capped at [`LIST_CAP`], with the full count.
fn sorted_capped(mut items: Vec<String>) -> (Vec<String>, usize) {
    items.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    items.dedup();
    capped(items)
}

/// The automatic rollback of a tag-owning run (26-11, D-07, TAGDEP-10),
/// after `failure` (a tag step, a red tag verify, or a project step after
/// the tags were applied). In order, each step running whatever the one
/// before did:
///
/// 1. the project, when its import was sent: a project that no longer reads
///    back as its pre-image is imported again from `export/<project>.zip`
///    (`overwrite=true`) and read back; a project the run created stays (no
///    project delete exists) and a warning says so;
/// 2. per owned provider the run imported into: a fresh export, then the
///    batches of [`rollback_plan`] (R3-RB), each required clean, a carried
///    one after an export of its own ([`carried`], guarded again); then,
///    when the master's tag-delete route answered and lists the provider,
///    the deletes of the nodes the run created ([`delete_created`], 26-19);
/// 3. the tag groups, from the run's snapshot ([`restore_groups`]);
/// 4. the provider settings the run modified, back to the saved record; a
///    provider the run created is deleted;
/// 5. the verify: every owned provider the run did not create is read until
///    two reads agree and compared with its pre-image, export against export
///    (R3-VK, R3-KEY, `defaultValue` as configuration).
///
/// Green: outcome `tags_rolled_back` (the project import was never sent) or
/// `rolled_back`, `data.ok: false`, `data.slug` [`ROLLED_BACK_SLUG`],
/// `data.tags.rollback`, and no record of any kind. Anything left (a step
/// that failed, a red verify, a node the run created in a provider that
/// existed and the route could not remove (R3-DEL), a value that cannot be
/// carried) ends `rollback_failed` naming the run directory and what is
/// left, after every other step ran.
#[allow(clippy::too_many_arguments)]
async fn roll_back(
    failure: TagFailure,
    project_undo: Option<ProjectUndo>,
    mut result: DeployProjectsResult,
    loaded: &LoadedProject,
    input: &ProjectsInput,
    surfaces: &ProjectsSurfaces<'_>,
    opts: &ProjectsRunOptions<'_>,
    run_id: &RunId,
    rel: &str,
    undo_writers: UndoWriters<'_>,
    route: &RouteState<'_>,
    events: &mut RunEvents<'_>,
) -> Result<DeployProjectsResult, CoreError> {
    let run_writer = undo_writers.run;
    let gateway = surfaces.gateway;
    let TagFailure {
        cause,
        what,
        mut writes,
    } = failure;
    let project = result.project.clone();
    // The anchor of the backup's sync-exhausted line (26-13).
    let backup_since_ms = wall_clock_ms();
    events.emit_late(ProjectsEvent::RollbackStarted { cause });
    // The host journal says the run is rolling back (26-12).
    journal_rolling_back(
        &mut result,
        loaded,
        run_writer,
        writes.host.as_mut(),
        events,
    )
    .await;
    let mut left: Vec<String> = Vec::new();
    let mut reimported: Vec<String> = Vec::new();
    let mut carried_back: Vec<String> = Vec::new();
    let mut undeletable: Vec<String> = Vec::new();
    // Each created node left, as `[P]path (why)` (26-19).
    let mut undeletable_left: Vec<String> = Vec::new();
    // Created nodes the route removed, and those its calls did not.
    let mut deleted: Vec<String> = Vec::new();
    let mut delete_left: Vec<(String, String)> = Vec::new();
    let mut uncarriable_paths: Vec<String> = Vec::new();
    // The top-most paths each provider's rollback re-created after a delete
    // (26-19), for the verify's R3-DV on them (26-21).
    let mut recreated_tops: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut project_reimported = false;

    // 1. The project.
    if let Some(undo) = &project_undo {
        match &undo.pre {
            Some(zip) => match live_digest(&project, zip) {
                Ok(pre) => {
                    let now = project_sha(gateway, &project).await;
                    if now.as_deref() != Some(pre.sha256.as_str()) {
                        match gateway.project_import(&project, zip.clone(), true).await {
                            Ok(_) => project_reimported = true,
                            Err(err) => left.push(format!(
                                "project {project}: the import of its pre-image failed ({})",
                                err.code()
                            )),
                        }
                        let back = project_sha(gateway, &project).await;
                        let equal = back.as_deref() == Some(pre.sha256.as_str());
                        events.emit_late(ProjectsEvent::ProjectRestored { equal });
                        if !equal && project_reimported {
                            left.push(format!(
                                "project {project}: it does not read back as its pre-image \
                                 ({rel}/export/{project}.zip)"
                            ));
                        }
                    }
                }
                Err(err) => left.push(format!(
                    "project {project}: its pre-image cannot be read ({})",
                    err.code()
                )),
            },
            None => match gateway.project_find(&project).await {
                Ok(Some(_)) => result.warnings.push(format!(
                    "project {project} was created by this run and stays: the gateway surface \
                     has no project delete, so the rollback cannot remove it"
                )),
                Ok(None) => {}
                Err(err) => result.warnings.push(format!(
                    "project {project}: whether this run created it could not be read ({})",
                    err.code()
                )),
            },
        }
    }

    // 2. The tags of every provider the run imported into.
    let empty = BTreeMap::new();
    for provider in sorted(&loaded.owned_tags) {
        let name = provider.name.as_str();
        if writes.created.contains_key(name) {
            // Its nodes go with the provider (step 4).
            continue;
        }
        let Some(applied) = writes.applied.get(name).filter(|a| !a.is_empty()) else {
            continue;
        };
        let Some(provider_plan) = writes
            .plan
            .as_ref()
            .and_then(|p| p.providers.iter().find(|pp| pp.provider == name))
        else {
            left.push(format!("[{name}]: no plan to roll back against"));
            continue;
        };
        let pre = writes.pre.get(name).unwrap_or(&empty);
        let current = match read_flat(gateway, name).await {
            Ok(current) => current,
            Err(err) => {
                left.push(format!(
                    "[{name}]: the provider could not be read to plan its rollback ({}); every \
                     node the run sent stays as it is",
                    err.code()
                ));
                continue;
            }
        };
        let no_record = BTreeSet::new();
        let recreate = writes.recreate.get(name).unwrap_or(&no_record);
        let undo = match rollback_plan(
            provider_plan,
            pre,
            &current,
            applied,
            false,
            recreate,
            route.ready().map(|(_, handshake)| handshake),
            TAG_DELETE_DESCENDANTS,
        ) {
            Ok(undo) => undo,
            Err(err) => {
                left.push(format!(
                    "[{name}]: its rollback could not be planned ({})",
                    err.code()
                ));
                continue;
            }
        };
        if !undo.recreated_deleted.is_empty() {
            recreated_tops
                .entry(name.to_string())
                .or_default()
                .extend(undo.recreated_deleted.iter().cloned());
        }
        let kept = route.kept_reason(name);
        // A kept created folder is named once its deletes are drawn again,
        // after the restores (26-REVIEW WR-04).
        for path in undo
            .undeletable
            .iter()
            .filter(|p| !undo.created.contains(p))
        {
            let full = full_tag_path(name, path);
            undeletable_left.push(format!("{full} ({kept})"));
            undeletable.push(full);
        }
        // A node at an added path the run cannot show it wrote stays
        // (26-REVIEW CR-01), with its own reason.
        for (path, why) in &undo.foreign {
            let full = full_tag_path(name, path);
            undeletable_left.push(format!("{full} ({})", why.describe()));
            undeletable.push(full);
        }
        uncarriable_paths.extend(undo.uncarriable.iter().map(|p| full_tag_path(name, p)));
        let full = |batch: &ImportBatch| -> Vec<String> {
            batch.nodes.iter().map(|p| full_tag_path(name, p)).collect()
        };
        let mut queue: VecDeque<ImportBatch> = undo.imports.into();
        while let Some(batch) = queue.pop_front() {
            let body = if batch.carry {
                let fresh = match read_flat(gateway, name).await {
                    Ok(fresh) => fresh,
                    Err(err) => {
                        left.extend(full(&batch).into_iter().map(|p| {
                            format!(
                                "{p} (not restored: the export to carry its value from failed, {})",
                                err.code()
                            )
                        }));
                        continue;
                    }
                };
                match uncarriable_in_batch(&batch, &fresh) {
                    Ok(bad) if bad.is_empty() => {}
                    Ok(bad) => {
                        let bad: BTreeSet<String> = bad
                            .into_iter()
                            .map(|(p, _)| full_tag_path(name, &p))
                            .collect();
                        left.extend(full(&batch).into_iter().filter(|p| !bad.contains(p)).map(
                            |p| format!("{p} (not restored: its batch held an uncarriable tag)"),
                        ));
                        uncarriable_paths.extend(bad);
                        continue;
                    }
                    Err(err) => {
                        left.extend(
                            full(&batch)
                                .into_iter()
                                .map(|p| format!("{p} (not restored: {})", err.code())),
                        );
                        continue;
                    }
                }
                match carried(&batch, &fresh) {
                    Ok(Carried::Body(bytes)) => bytes,
                    Ok(Carried::Split(parts)) => {
                        for part in parts.into_iter().rev() {
                            queue.push_front(part);
                        }
                        continue;
                    }
                    Err(err) => {
                        left.extend(
                            full(&batch)
                                .into_iter()
                                .map(|p| format!("{p} (not restored: {})", err.code())),
                        );
                        continue;
                    }
                }
            } else {
                batch.body.clone()
            };
            if batch.carry {
                events.emit_late(ProjectsEvent::TagValuesCarried {
                    provider: name.to_string(),
                    path: batch.path.clone(),
                    nodes: batch.nodes.len(),
                });
            }
            events.emit_late(ProjectsEvent::TagImportSent {
                provider: name.to_string(),
                path: batch.path.clone(),
                policy: batch.policy.as_wire(),
                nodes: batch.nodes.len(),
                carry: batch.carry,
            });
            let label = format!(
                "{} ({}, {} node(s))",
                full_tag_path(name, &batch.path),
                batch.policy.as_wire(),
                batch.nodes.len()
            );
            match gateway
                .tag_import(name, &batch.path, batch.policy, body)
                .await
            {
                Ok(answer) => {
                    let clean = answer.is_clean();
                    events.emit_late(ProjectsEvent::TagImportAnswered {
                        provider: name.to_string(),
                        clean,
                    });
                    if clean {
                        reimported.extend(full(&batch));
                        if batch.carry {
                            carried_back.extend(full(&batch));
                        }
                    } else {
                        left.push(format!("{label}: the restore import was not clean"));
                    }
                }
                Err(err) => left.push(format!(
                    "{label}: the restore import failed ({})",
                    err.code()
                )),
            }
        }
        // Then the nodes the run created, through the route (26-19): after
        // the import restores, as 26-11 orders the undo, drawn again from an
        // export read right now (26-REVIEW WR-04), so a node another client
        // added below a created folder during the restores keeps it.
        if !undo.created.is_empty() {
            let left_all = |why: String| -> Vec<(String, String)> {
                undo.created
                    .iter()
                    .map(|p| (full_tag_path(name, p), why.clone()))
                    .collect()
            };
            let Some((client, handshake)) = route.ready() else {
                delete_left.extend(left_all(
                    "created by this run; the tag-delete route is not ready".to_string(),
                ));
                continue;
            };
            let fresh = match read_flat(gateway, name).await {
                Ok(fresh) => fresh,
                Err(err) => {
                    delete_left.extend(left_all(format!(
                        "created by this run; the provider could not be read before its deletes \
                         ({})",
                        err.code()
                    )));
                    continue;
                }
            };
            match created_deletes_now(name, &undo.created, &fresh, applied, TAG_DELETE_DESCENDANTS)
            {
                Ok(drawn) => {
                    let deletes = drawn.deletes;
                    let foreign = drawn.foreign;
                    for path in &drawn.kept {
                        let full = full_tag_path(name, path);
                        undeletable_left.push(format!("{full} ({kept})"));
                        undeletable.push(full);
                    }
                    for (path, user) in &drawn.in_use {
                        let full = full_tag_path(name, path);
                        undeletable_left.push(format!(
                            "{full} (created by this run; the live node {} still uses it, so it \
                             was not removed)",
                            full_tag_path(name, user)
                        ));
                        undeletable.push(full);
                    }
                    for (path, why) in &foreign {
                        let full = full_tag_path(name, path);
                        undeletable_left.push(format!("{full} ({})", why.describe()));
                        undeletable.push(full);
                    }
                    if !deletes.is_empty() {
                        let (removed, kept) =
                            delete_created(client, handshake, name, &deletes, events).await;
                        deleted.extend(removed);
                        delete_left.extend(kept);
                    }
                }
                Err(err) => delete_left.extend(left_all(format!(
                    "created by this run; its deletes could not be planned ({})",
                    err.code()
                ))),
            }
        }
    }

    // 3. The tag groups.
    let mut groups_restored = false;
    match &writes.groups {
        Some(GroupsDone::Committed { record, providers }) => {
            let journal = journal_location(
                LedgerRoot::for_node(&loaded.target.nodes.master),
                &loaded.target.gateway,
                &loaded.target.mode,
            );
            match (surfaces.apply_host, surfaces.deploy) {
                (Some(host), Some(deploy)) => {
                    match restore_groups(host, deploy, record, run_id, None).await {
                        Ok(()) => {
                            groups_restored = true;
                            events.emit_late(ProjectsEvent::TagGroupsRestored {
                                providers: providers.clone(),
                            });
                        }
                        Err(why) => left.push(format!(
                            "the tag groups of {}: {why}; see the on-box journal {journal}",
                            providers.join(", ")
                        )),
                    }
                }
                _ => left.push(format!(
                    "the tag groups of {}: no apply host to restore them with",
                    providers.join(", ")
                )),
            }
        }
        Some(GroupsDone::Restored) => groups_restored = true,
        Some(GroupsDone::Unrestored(why)) => left.push(why.clone()),
        None => {}
    }

    // 4. The provider settings: put back, or the created provider deleted.
    let mut provider_restored = Vec::new();
    let mut provider_deleted = Vec::new();
    for (name, pre) in writes.modified.iter().rev() {
        match restore_provider_settings(gateway, name, pre).await {
            Ok(true) => {
                provider_restored.push(name.clone());
                events.emit_late(ProjectsEvent::TagProviderRestored {
                    provider: name.clone(),
                    action: "modify",
                });
            }
            Ok(false) => {}
            Err(why) => left.push(format!(
                "[{name}] provider settings: {why} (pre-image {rel}/export/tags/{name}.provider.json)"
            )),
        }
    }
    for (name, created) in &writes.created {
        match delete_created_provider(gateway, name, created).await {
            Ok(true) => {
                provider_deleted.push(name.clone());
                events.emit_late(ProjectsEvent::TagProviderRestored {
                    provider: name.clone(),
                    action: "delete",
                });
            }
            Ok(false) => {}
            Err(why) => left.push(format!("[{name}] the provider this run created: {why}")),
        }
    }

    // 5. The verify, export against export.
    let owned = sorted(&loaded.owned_tags);
    // Only providers with a pre-image: one the run created, or meant to
    // create and found another client's, has nothing to compare with.
    let names: Vec<&str> = owned
        .iter()
        .map(|p| p.name.as_str())
        .filter(|name| writes.pre.contains_key(*name))
        .collect();
    let exempt: BTreeSet<String> = undeletable
        .iter()
        .chain(&uncarriable_paths)
        .chain(delete_left.iter().map(|(path, _)| path))
        .cloned()
        .collect();
    let mut verify = None;
    if !names.is_empty() {
        match settled_reads(
            gateway,
            &names,
            opts.tag_verify_settle,
            opts.tag_verify_poll,
        )
        .await
        {
            Ok((last, reads, settled, elapsed_s)) => {
                let mut mismatches = Vec::new();
                for name in &names {
                    let pre = writes.pre.get(*name).unwrap_or(&empty);
                    let now = last.get(*name).unwrap_or(&empty);
                    let tops = recreated_tops.get(*name).map(Vec::as_slice).unwrap_or(&[]);
                    for (path, before) in pre {
                        let full = full_tag_path(name, path);
                        if exempt.contains(&full) {
                            continue;
                        }
                        match now.get(path) {
                            None => mismatches.push(format!("{full} (absent)")),
                            Some(after) => {
                                // A node re-created from its pre-image is a
                                // node this rollback created: the gateway's
                                // C12 `defaultValue` is absorbed as R3-DV
                                // absorbs it on a created node, the seed
                                // checked against the pre-image's `value`.
                                let opts = if recreated_at(path, tops) {
                                    RECREATED
                                } else {
                                    EXPORTS
                                };
                                let properties = diff_properties(before, after, path, opts);
                                if !properties.is_empty() {
                                    mismatches.push(format!("{full} ({})", properties.join(", ")));
                                }
                            }
                        }
                    }
                    for path in now.keys() {
                        let full = full_tag_path(name, path);
                        if !pre.contains_key(path) && !exempt.contains(&full) {
                            mismatches.push(format!("{full} (not in the pre-image)"));
                        }
                    }
                }
                mismatches.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
                let equal = mismatches.is_empty();
                events.emit_late(ProjectsEvent::RollbackVerified { equal });
                left.extend(mismatches.iter().map(|m| format!("verify: {m}")));
                let mismatches_count = mismatches.len();
                mismatches.truncate(LIST_CAP);
                verify = Some(TagVerify {
                    equal,
                    settled,
                    reads,
                    elapsed_s,
                    mismatches,
                    mismatches_count,
                });
            }
            Err(err) => left.push(format!(
                "the owned tag providers could not be read back to verify the rollback ({})",
                err.code()
            )),
        }
    }

    left.extend(undeletable_left);
    left.extend(
        delete_left
            .iter()
            .map(|(path, why)| format!("{path} ({why})")),
    );
    left.extend(uncarriable_paths.iter().map(|p| {
        format!("{p} (its live value cannot be carried into an Overwrite, so it was not restored)")
    }));

    if left.is_empty() {
        result.outcome = if project_undo.is_some() {
            ProjectOutcome::RolledBack
        } else {
            ProjectOutcome::TagsRolledBack
        };
        result.ok = false;
        result.slug = Some(ROLLED_BACK_SLUG.to_string());
        result.record_written = false;
        result.restore_command = None;
        if let Some(tags) = result.tags.as_mut() {
            let (carried, carried_count) = sorted_capped(writes.carried.clone());
            tags.carried = carried;
            tags.carried_count = carried_count;
            tags.verify = writes.verify.clone();
            let (reimported, reimported_count) = sorted_capped(reimported);
            let (carried, carried_count) = sorted_capped(carried_back);
            provider_restored.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            provider_deleted.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            let (deleted, deleted_count) = sorted_capped(deleted);
            tags.rollback = Some(TagRollback {
                cause: cause.as_str().to_string(),
                detail: what,
                reimported,
                reimported_count,
                carried,
                carried_count,
                undeletable: Vec::new(),
                undeletable_count: 0,
                groups_restored,
                provider_restored,
                provider_deleted,
                project_reimported,
                verify,
                deleted,
                deleted_count,
            });
        }
        // `--rollback-run`: the tag records go back to the pre-image ones
        // (26-12); the journal stays when one of them could not.
        let records_back = match &writes.records_back {
            Some(back) => {
                restore_tag_records(&mut result, loaded, undo_writers.records, back, events).await
            }
            None => true,
        };
        // ...and the release record before the run (ultrareview U1), so it
        // no longer names the undone release. A failure is a warning: the
        // gateway is restored.
        if let Some(back) = &writes.release_back {
            restore_release_record(&mut result, loaded, undo_writers.release, back.as_deref())
                .await;
        }
        // Verified: the host journal goes (26-12).
        if records_back {
            clear_host_journal(
                &mut result,
                loaded,
                run_writer,
                writes.host.as_ref(),
                events,
            )
            .await;
        }
        // 26-13: on a pair the backup's copy of the restored providers is
        // checked. The verdict stays a rollback; an unsynced backup is a
        // warning and never a second rollback.
        if backup_step(&mut result, &names, surfaces, opts, backup_since_ms, events).await
            == Some(false)
        {
            let reason = result
                .tags
                .as_ref()
                .and_then(|tags| tags.backup.as_ref())
                .and_then(|backup| backup.reason)
                .map_or("unknown", |reason| reason.as_str());
            result.warnings.push(format!(
                "the backup did not converge after the rollback ({reason}): the master is \
                 restored and verified, and data.tags.backup says what the backup holds"
            ));
        }
        events.emit_late(ProjectsEvent::Finished {
            outcome: result.outcome,
        });
        note_late_log_failure(&mut result, events);
        return Ok(result);
    }

    let listed = |items: &[String]| {
        let shown: Vec<&str> = items
            .iter()
            .take(TAG_FAILURE_PATHS_SHOWN)
            .map(String::as_str)
            .collect();
        let more = items.len().saturating_sub(shown.len());
        format!(
            "{}{}",
            if shown.is_empty() {
                "none".to_string()
            } else {
                shown.join("; ")
            },
            if more > 0 {
                format!(" and {more} more")
            } else {
                String::new()
            }
        )
    };
    // `--rollback-run` keeps its pre-images on the host, not in the run
    // directory (26-12).
    let pre_images = match (&writes.host, cause) {
        (Some(_), RollbackCause::RollbackRun) => pre_image_location(
            LedgerRoot::for_node(&loaded.target.nodes.master),
            &loaded.target.gateway,
            &loaded.file.project,
            crate::deploy::project_run::PRE_IMAGE_DIR,
        ),
        _ => format!("{rel}/export"),
    };
    Err(CoreError::RollbackFailed(Box::new(RollbackFailure {
        run_id: run_id.as_str().to_string(),
        env: input.env.clone(),
        run_dir: Some(rel.to_string()),
        journal: pre_images,
        cause: format!(
            "{}: {what}; {} {} tag write(s): {}; the rollback put back what it could \
             and {} item(s) are left: {}{}",
            cause.as_str(),
            if cause == RollbackCause::RollbackRun {
                "the undone run planned"
            } else {
                "the run sent"
            },
            writes.sent.len(),
            listed(&writes.sent),
            left.len(),
            listed(&left),
            match &writes.host {
                // The host journal stays (26-12): the next run refuses until
                // the undo from the host copy is green.
                Some(host) => format!(
                    "; the host journal {} stays, and `{}` retries the undo from the host copy",
                    host_journal_location(loaded),
                    rollback_run_command(&input.env, &host.journal.run_id)
                ),
                None => String::new(),
            }
        ),
        scope: RollbackScope::Projects,
    })))
}

// -- `--rollback-run` (26-12, TAGDEP-10) -------------------------------------

/// The confirmation prose of `--rollback-run`: what will be undone, where,
/// and what it deletes and re-creates, by whether the master's tag-delete
/// route is configured (`route`, 26-REVIEW WR-06). Names only;
/// `--check --rollback-run` lists the paths.
pub fn rollback_run_prose(
    loaded: &LoadedProject,
    env: &str,
    run_id: &str,
    route: &TagDeleteSurface<'_>,
) -> String {
    let created = match route {
        TagDeleteSurface::Configured(_) => {
            "Tags the run created in a provider that existed are removed through the tag-delete \
             route when it answers and lists the provider; one the route cannot remove, or that \
             is not what the run wrote, stays and ends the undo as rollback_failed"
        }
        TagDeleteSurface::NotConfigured { .. } => {
            "Tags the run created in a provider that existed cannot be removed without the \
             tag-delete route, which this profile does not configure, and end the undo as \
             rollback_failed"
        }
    };
    format!(
        "deploy projects will undo run {run_id} of project {} on gateway {} (env {env}) from \
         the copy the gateway host keeps: the owned tags of {}, their tag groups, provider \
         settings and tag records, and the project, back to that run's pre-image. Tags the run \
         deleted are re-created from the pre-image. {created}. `ign deploy projects --env {env} \
         --check --rollback-run {run_id}` lists what it restores",
        loaded.file.project,
        loaded.target.gateway,
        loaded.file.tag_providers.join(", "),
    )
}

/// What the gateway host keeps of one run, read and checked (26-12).
struct HostSet {
    run: PreImageRun,
    journal: Option<ProjectJournal>,
    project_zip: Option<Vec<u8>>,
    /// The project's release record before the run, as stored
    /// (ultrareview U1).
    release: Option<Vec<u8>>,
    pre: ProviderReads,
    settings: BTreeMap<String, TagProviderRecord>,
    /// Each provider's tag record before the run, parsed and as stored.
    records: BTreeMap<String, (TagRecord, Vec<u8>)>,
    /// Per provider, the nodes the run sends at its added paths
    /// (`sent-<P>.json`, 26-REVIEW CR-01).
    sent: BTreeMap<String, BTreeMap<String, Value>>,
}

/// Read the host copy of run `target` (26-12, T-26-44): a journal of another
/// run refuses `deploy_journal_present`; a missing `run.json`, one naming
/// another run (only the latest is kept), a provider the checkout does not
/// own, or a file that is missing or does not hash as `run.json` says is
/// `deploy_run_invalid`.
async fn read_host_set(
    loaded: &LoadedProject,
    input: &ProjectsInput,
    host: &dyn ProjectHost,
    target: &RunId,
) -> Result<HostSet, CoreError> {
    let gateway = &loaded.target.gateway;
    let project = &loaded.file.project;
    let root = LedgerRoot::for_node(&loaded.target.nodes.master);
    let invalid = |problem: String| CoreError::DeployRunInvalid {
        run_id: target.as_str().to_string(),
        problem,
    };
    let journal = read_project_journal(loaded, host).await?;
    if let Some(journal) = &journal
        && journal.run_id != target.as_str()
    {
        return Err(project_journal_present(loaded, &input.env, &journal.run_id));
    }
    let run_location = pre_image_location(root, gateway, project, "run.json");
    let Some(bytes) = host.read_pre_image(gateway, project, "run.json").await? else {
        return Err(invalid(format!(
            "{run_location} does not exist: the host keeps the pre-image of the latest \
             tag-owning run of this project only, and holds none"
        )));
    };
    let run = PreImageRun::parse(&bytes, &run_location, gateway, project)?;
    if run.run_id != target.as_str() {
        return Err(invalid(format!(
            "the host keeps the pre-image of run {} only, the latest run of this project; \
             run {} cannot be undone from it",
            run.run_id,
            target.as_str()
        )));
    }
    for provider in &run.providers {
        if !loaded.file.tag_providers.contains(&provider.name) {
            return Err(invalid(format!(
                "{run_location} lists tag provider {}, which deploy/project.yaml does not own: \
                 only the project's own providers are restored",
                provider.name
            )));
        }
    }
    if run.project_action == "import" && !run.files.contains_key("project.zip") {
        return Err(invalid(format!(
            "{run_location}: the run imported the project but the set holds no project.zip"
        )));
    }
    let mut set = HostSet {
        run,
        journal,
        project_zip: None,
        release: None,
        pre: BTreeMap::new(),
        settings: BTreeMap::new(),
        records: BTreeMap::new(),
        sent: BTreeMap::new(),
    };
    for (name, sha) in &set.run.files {
        let location = pre_image_location(root, gateway, project, name);
        let Some(bytes) = host.read_pre_image(gateway, project, name).await? else {
            return Err(invalid(format!(
                "{location} is missing: the pre-image set is incomplete"
            )));
        };
        if sha256_hex(&bytes) != *sha {
            return Err(invalid(format!(
                "{location} does not hash as run.json records: the set is incomplete or changed"
            )));
        }
        match PreImageFile::parse(name) {
            Some(PreImageFile::Project) => set.project_zip = Some(bytes),
            Some(PreImageFile::Release) => {
                ReleaseRecord::parse(&bytes, &location, gateway, project)?;
                set.release = Some(bytes);
            }
            Some(PreImageFile::Tags(provider)) => {
                let flat = flatten_export(&bytes, &provider)?;
                set.pre.insert(provider, flat);
            }
            Some(PreImageFile::Provider(provider)) => {
                let record: TagProviderRecord = serde_json::from_slice(&bytes)
                    .map_err(|_| invalid(format!("{location} is not a tag provider record")))?;
                set.settings.insert(provider, record);
            }
            Some(PreImageFile::Record(provider)) => {
                let record = TagRecord::parse(&bytes, &location, gateway, &provider)?;
                if record.project != *project {
                    return Err(invalid(format!(
                        "{location} is another project's tag record"
                    )));
                }
                set.records.insert(provider, (record, bytes));
            }
            Some(PreImageFile::Sent(provider)) => {
                let sent: BTreeMap<String, Value> =
                    serde_json::from_slice(&bytes).map_err(|_| {
                        invalid(format!(
                            "{location} is not an object of node paths to the nodes the run sent"
                        ))
                    })?;
                set.sent.insert(provider, sent);
            }
            Some(PreImageFile::Run) | None => {
                return Err(invalid(format!("{location}: not a file of the set")));
            }
        }
    }
    // 26-20 (rule 5): every path a delete call of the run named is re-created
    // from the provider's pre-image, so the set must hold it.
    let empty = BTreeMap::new();
    for provider in &set.run.providers {
        let pre = set.pre.get(&provider.name).unwrap_or(&empty);
        if let Some(path) = provider.deleted.iter().find(|p| !pre.contains_key(*p)) {
            return Err(invalid(format!(
                "{run_location} names {} among the paths run {} deleted, and the set's \
                 pre-image of the provider does not hold it: the set is stale or partial",
                full_tag_path(&provider.name, path),
                target.as_str()
            )));
        }
    }
    Ok(set)
}

/// The undo of the run `set` describes, in the automatic rollback's own
/// terms (26-11): its plan's added and changed nodes as possibly applied,
/// the pre-image exports, the proof of each provider it created, the
/// settings records of the providers it modified, and (26-12) the pre-run
/// tag records' nodes and the records to put back.
fn undo_writes(set: &HostSet) -> TagWrites {
    let mut writes = TagWrites::default();
    let mut providers = Vec::new();
    let mut records_back = Vec::new();
    for provider in &set.run.providers {
        let name = provider.name.clone();
        providers.push(ProviderPlan {
            provider: name.clone(),
            added: provider.added.clone(),
            recreated: Vec::new(),
            changed: provider
                .changed
                .iter()
                .map(|path| NodeChange {
                    path: path.clone(),
                    properties: Vec::new(),
                    restart_likely: false,
                })
                .collect(),
            kept_values: Vec::new(),
            removed: provider.removed.clone(),
            already_absent: Vec::new(),
            drift: Vec::new(),
            same: 0,
            first_change: false,
            record_only: false,
            provider_emptied: false,
            record_run_id: None,
        });
        let mut applied = AppliedSteps::default();
        // Only the batches the host copy records as sent (26-REVIEW CR-01);
        // a set an earlier release wrote records none, so every planned
        // node counts, and no created node has a sent body to match.
        match &provider.sent {
            Some(sent) => applied.nodes.extend(sent.iter().cloned()),
            None => {
                applied.nodes.extend(provider.added.iter().cloned());
                applied.nodes.extend(provider.changed.iter().cloned());
            }
        }
        if let Some(bodies) = set.sent.get(&name) {
            applied.bodies.extend(
                bodies
                    .iter()
                    .filter(|(path, _)| applied.nodes.contains(*path))
                    .map(|(path, node)| (path.clone(), node.clone())),
            );
        }
        applied.collided.extend(provider.collided.iter().cloned());
        // What its delete calls named (26-20): re-created from the
        // pre-image when the provider lacks it now.
        applied.deleted.clone_from(&provider.deleted);
        if !applied.is_empty() {
            writes.sent.push(format!(
                "[{name}] {} planned node(s), {} deleted path(s)",
                applied.nodes.len(),
                applied.deleted.len()
            ));
            writes.applied.insert(name.clone(), applied);
        }
        if let Some(proof) = &provider.created {
            writes
                .sent
                .push(format!("[{name}] provider settings (create)"));
            writes.created.insert(
                name.clone(),
                CreatedProvider {
                    confirmed: proof.confirmed,
                    sent: (
                        proof.enabled,
                        proof.description.clone(),
                        proof.config.clone(),
                    ),
                    signature: proof.signature.clone(),
                },
            );
        }
        if provider.settings == "modify"
            && let Some(record) = set.settings.get(&name)
        {
            writes
                .sent
                .push(format!("[{name}] provider settings (modify)"));
            writes.modified.push((name.clone(), record.clone()));
        }
        match set.records.get(&name) {
            Some((record, bytes)) => {
                writes
                    .recreate
                    .insert(name.clone(), record.nodes.iter().cloned().collect());
                records_back.push((name, Some(bytes.clone())));
            }
            None => records_back.push((name, None)),
        }
    }
    writes.plan = Some(TagPlan {
        providers,
        tags_sha256: String::new(),
        live_sha256: String::new(),
        plan_sha256: String::new(),
    });
    writes.pre = set.pre.clone();
    writes.records_back = Some(records_back);
    writes.release_back = Some(set.release.clone());
    writes
}

/// The tag-group half of the undo (26-12): the run's group commit, read from
/// its on-box run record. A commit that never started needs nothing; one
/// already rolled back needs nothing; one that did not finish is the file
/// engine's to recover and is left, named.
async fn undo_groups(
    set: &HostSet,
    input: &ProjectsInput,
    apply_host: Option<&dyn ApplyHost>,
    target: &RunId,
) -> Option<GroupsDone> {
    if !set.run.groups {
        return None;
    }
    let providers: Vec<String> = set.run.providers.iter().map(|p| p.name.clone()).collect();
    let recover = format!(
        "the tag groups of {}: the group commit of run {} did not finish on the box; recover \
         it with `ign deploy rollback --run {} --env {}` first",
        providers.join(", "),
        target.as_str(),
        target.as_str(),
        shell_word(&input.env)
    );
    let Some(host) = apply_host else {
        return Some(GroupsDone::Unrestored(format!(
            "the tag groups of {}: no apply host to restore them with",
            providers.join(", ")
        )));
    };
    match host.read_record(target).await {
        Ok(None) => None,
        Ok(Some(record)) => match record.state {
            RecordState::RolledBack => None,
            // A unit changed since the run is never put back over that
            // change (ultrareview U5, as `ign deploy rollback` refuses
            // `deploy_live_changed`): the groups are left, named.
            RecordState::Applied => match live_changes(host, &record).await {
                Ok(changed) if changed.is_empty() => Some(GroupsDone::Committed {
                    record: Box::new(record),
                    providers,
                }),
                Ok(changed) => Some(GroupsDone::Unrestored(format!(
                    "the tag groups of {}: {} unit file(s) changed since run {} ({}{}); they were \
                     not put back over that change",
                    providers.join(", "),
                    changed.len(),
                    target.as_str(),
                    changed
                        .iter()
                        .take(APPLY_LIST_SHOWN)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", "),
                    if changed.len() > APPLY_LIST_SHOWN {
                        ", ..."
                    } else {
                        ""
                    }
                ))),
                Err(err) => Some(GroupsDone::Unrestored(format!(
                    "the tag groups of {}: their unit files could not be compared with run {} ({})",
                    providers.join(", "),
                    target.as_str(),
                    err.code()
                ))),
            },
            RecordState::Applying => Some(GroupsDone::Unrestored(recover)),
        },
        Err(err) => Some(GroupsDone::Unrestored(format!(
            "the tag groups of {}: the run record of run {} could not be read ({})",
            providers.join(", "),
            target.as_str(),
            err.code()
        ))),
    }
}

/// Put the project's release record from before the run back (ultrareview
/// U1), through the release-record writer. With none in the host copy (the
/// project had none, or the run predates the file) the record the run left
/// stays, and a warning says so; a failed write is a warning too.
async fn restore_release_record(
    result: &mut DeployProjectsResult,
    loaded: &LoadedProject,
    writer: Option<&dyn ReleaseRecordWriter>,
    back: Option<&[u8]>,
) {
    let (Some(writer), Some(bytes)) = (writer, back) else {
        result.warnings.push(format!(
            "the release record of {} was not put back: the host copy holds none from before \
             the run, so the record may still name the undone release",
            loaded.file.project
        ));
        return;
    };
    if let Err(err) = writer
        .write_release_record(&loaded.target.gateway, &loaded.file.project, bytes)
        .await
    {
        result.warnings.push(format!(
            "the release record of {} could not be put back ({}); it may still name the \
             undone release",
            loaded.file.project,
            err.code()
        ));
    }
}

/// Put each provider's tag record from before the run back (26-12), through
/// the tag-record writer. A provider that had none keeps the record the run
/// left, if any (this release cannot remove a record), and a warning says
/// so. `false` when a record could not be written.
async fn restore_tag_records(
    result: &mut DeployProjectsResult,
    loaded: &LoadedProject,
    writer: Option<&dyn TagRecordWriter>,
    back: &[(String, Option<Vec<u8>>)],
    events: &mut RunEvents<'_>,
) -> bool {
    let mut ok = true;
    for (provider, bytes) in back {
        let Some(bytes) = bytes else {
            result.warnings.push(format!(
                "tag provider {provider} had no tag record before the undone run: the record \
                 that run left, if any, stays (this release cannot remove one)"
            ));
            continue;
        };
        let Some(writer) = writer else {
            ok = false;
            continue;
        };
        match writer
            .write_tag_record(&loaded.target.gateway, provider, bytes)
            .await
        {
            Ok(()) => events.emit_late(ProjectsEvent::TagRecordWritten {
                provider: provider.clone(),
            }),
            Err(err) => {
                ok = false;
                events.emit_late(ProjectsEvent::TagRecordFailed {
                    provider: provider.clone(),
                    code: err.code().to_string(),
                });
                result.warnings.push(format!(
                    "the tag record of {provider} could not be put back ({}); the host journal \
                     {} stays",
                    err.code(),
                    host_journal_location(loaded)
                ));
            }
        }
    }
    ok
}

/// The project action a run kept in `run.json`.
fn action_of(word: &str) -> ProjectAction {
    match word {
        "create" => ProjectAction::Create,
        "import" => ProjectAction::Import,
        "record" => ProjectAction::Record,
        _ => ProjectAction::Skip,
    }
}

/// The result `--rollback-run` reports: the undone run's names, its plan in
/// `data.tags`, the journal it found.
fn rollback_run_result(
    loaded: &LoadedProject,
    input: &ProjectsInput,
    set: &HostSet,
    writes: &TagWrites,
    live_sha256: Option<String>,
    check: bool,
) -> DeployProjectsResult {
    let empty = TagPlan {
        providers: Vec::new(),
        tags_sha256: String::new(),
        live_sha256: String::new(),
        plan_sha256: String::new(),
    };
    let mut report = TagsReport::from_plan(
        writes.plan.as_ref().unwrap_or(&empty),
        &BTreeMap::new(),
        LIST_CAP,
    );
    report.journal = set.journal.as_ref().map(|j| TagJournal {
        run_id: j.run_id.clone(),
        state: j.state.as_str().to_string(),
        started_at: j.started_at.clone(),
    });
    let restored_sha = set
        .project_zip
        .as_ref()
        .and_then(|zip| live_digest(&loaded.file.project, zip).ok())
        .map(|d| d.sha256);
    DeployProjectsResult {
        env: input.env.clone(),
        gateway: loaded.target.gateway.clone(),
        project: loaded.file.project.clone(),
        release: set.run.release.clone(),
        check,
        action: action_of(&set.run.project_action),
        would_change: true,
        artifact_sha256: restored_sha
            .or_else(|| live_sha256.clone())
            .unwrap_or_default(),
        live_sha256,
        members: MemberDelta::default(),
        removed: Vec::new(),
        removed_count: 0,
        excluded: Vec::new(),
        excluded_count: 0,
        unmanaged: Vec::new(),
        unmanaged_count: 0,
        parent: loaded.meta.parent.clone(),
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
        missing_tags: None,
        missing_tags_count: None,
        missing_tags_unknown_providers: None,
        missing_tags_unknown_providers_count: None,
        missing_tags_unavailable: None,
        missing_tags_limit: MISSING_TAGS_LIMIT,
        tags: Some(Box::new(report)),
        outcome: if check {
            ProjectOutcome::Checked
        } else {
            ProjectOutcome::RolledBack
        },
        ok: true,
        slug: None,
    }
}

/// What the undo would do, read only (`--check --rollback-run`, 26-12): the
/// same rules as the undo itself ([`rollback_plan`] per provider, the project
/// against its pre-image, the settings, the provider deletes and the
/// groups), into `data.tags.rollback`; a node the undo cannot remove is a
/// warning that it would end `rollback_failed`.
#[allow(clippy::too_many_arguments)]
async fn rollback_run_preview(
    gateway: &dyn ProjectGateway,
    loaded: &LoadedProject,
    set: &HostSet,
    writes: &TagWrites,
    groups: Option<&GroupsDone>,
    target: &RunId,
    route: &RouteState<'_>,
    result: &mut DeployProjectsResult,
) {
    let project = loaded.file.project.as_str();
    let mut reimported = Vec::new();
    let mut carried = Vec::new();
    let mut undeletable = Vec::new();
    let mut deleted = Vec::new();
    let mut project_reimported = false;
    if let Some(zip) = &set.project_zip
        && let Ok(pre) = live_digest(project, zip)
    {
        project_reimported = result.live_sha256.as_deref() != Some(pre.sha256.as_str());
    }
    if set.run.project_action == "create" && result.live_sha256.is_some() {
        result.warnings.push(format!(
            "project {project} was created by run {} and stays: the gateway surface has no \
             project delete",
            target.as_str()
        ));
    }
    let empty = BTreeMap::new();
    let no_record = BTreeSet::new();
    for provider in &set.run.providers {
        let name = provider.name.as_str();
        if writes.created.contains_key(name) {
            continue;
        }
        let (Some(applied), Some(plan)) = (
            writes.applied.get(name),
            writes
                .plan
                .as_ref()
                .and_then(|p| p.providers.iter().find(|pp| pp.provider == name)),
        ) else {
            continue;
        };
        let current = match read_flat(gateway, name).await {
            Ok(current) => current,
            Err(err) => {
                result.warnings.push(format!(
                    "[{name}]: the provider could not be read to plan its undo ({})",
                    err.code()
                ));
                continue;
            }
        };
        let pre = set.pre.get(name).unwrap_or(&empty);
        let recreate = writes.recreate.get(name).unwrap_or(&no_record);
        match rollback_plan(
            plan,
            pre,
            &current,
            applied,
            false,
            recreate,
            route.ready().map(|(_, handshake)| handshake),
            TAG_DELETE_DESCENDANTS,
        ) {
            Ok(undo) => {
                for batch in &undo.imports {
                    reimported.extend(batch.nodes.iter().map(|p| full_tag_path(name, p)));
                }
                carried.extend(undo.carried.iter().map(|p| full_tag_path(name, p)));
                undeletable.extend(undo.undeletable.iter().map(|p| full_tag_path(name, p)));
                for (path, why) in &undo.foreign {
                    undeletable.push(full_tag_path(name, path));
                    result.warnings.push(format!(
                        "{}: {}",
                        full_tag_path(name, path),
                        why.describe()
                    ));
                }
                deleted.extend(undo.deletes.iter().map(|p| full_tag_path(name, p)));
                for path in &undo.uncarriable {
                    result.warnings.push(format!(
                        "{}: its live value cannot be carried into an Overwrite, so the undo \
                         would leave it and end rollback_failed",
                        full_tag_path(name, path)
                    ));
                }
            }
            Err(err) => result.warnings.push(format!(
                "[{name}]: its undo could not be planned ({})",
                err.code()
            )),
        }
    }
    let mut provider_restored = Vec::new();
    for (name, pre) in &writes.modified {
        if let Ok(Some(now)) = gateway.tag_provider_record(name).await
            && !settings_equal(&now, pre)
        {
            provider_restored.push(name.clone());
        }
    }
    let mut provider_deleted = Vec::new();
    for name in writes.created.keys() {
        if let Ok(Some(_)) = gateway.tag_provider_record(name).await {
            provider_deleted.push(name.clone());
        }
    }
    if let Some(GroupsDone::Unrestored(why)) = groups {
        result.warnings.push(why.clone());
    }
    if !undeletable.is_empty() {
        result.warnings.push(format!(
            "the undo would end rollback_failed: {} node(s) run {} created cannot be removed \
             (data.tags.delete_route says whether the tag-delete route can be used)",
            undeletable.len(),
            target.as_str()
        ));
    }
    if let Some(back) = &writes.records_back {
        for (provider, bytes) in back {
            if bytes.is_none() {
                result.warnings.push(format!(
                    "tag provider {provider} had no tag record before run {}: the record that \
                     run left, if any, would stay",
                    target.as_str()
                ));
            }
        }
    }
    let (reimported, reimported_count) = sorted_capped(reimported);
    let (carried, carried_count) = sorted_capped(carried);
    let (undeletable, undeletable_count) = sorted_capped(undeletable);
    let (deleted, deleted_count) = sorted_capped(deleted);
    provider_restored.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    result.would_change = project_reimported
        || reimported_count > 0
        || deleted_count > 0
        || !provider_restored.is_empty()
        || !provider_deleted.is_empty()
        || matches!(groups, Some(GroupsDone::Committed { .. }));
    if let Some(tags) = result.tags.as_mut() {
        tags.rollback = Some(TagRollback {
            cause: RollbackCause::RollbackRun.as_str().to_string(),
            detail: format!(
                "what the undo of run {} from the host copy would restore; nothing was written",
                target.as_str()
            ),
            reimported,
            reimported_count,
            carried,
            carried_count,
            undeletable,
            undeletable_count,
            groups_restored: matches!(groups, Some(GroupsDone::Committed { .. })),
            provider_restored,
            provider_deleted,
            project_reimported,
            verify: None,
            deleted,
            deleted_count,
        });
    }
}

/// `ign deploy projects --rollback-run <RUN_ID>` (26-12, TAGDEP-10): undo the
/// latest tag-owning run of this project on this gateway from the copy the
/// gateway host keeps, so a runner that did not perform the run can undo
/// it, interrupted or finished.
///
/// The host copy is read and checked first ([`read_host_set`]): only the
/// latest run's pre-image is kept, so any other run id is
/// `deploy_run_invalid`, and a journal of another run is
/// `deploy_journal_present`. The undo then follows the automatic rollback's
/// rules and code ([`roll_back`], 26-11): the project re-imported from its
/// pre-image when it differs; per owned provider the nodes the run's plan
/// added or changed against a fresh export by [`rollback_plan`] (R3-RB, no
/// pre-run value is ever written; R3-CARRY where `Overwrite` is needed; a
/// changed node another client removed is re-created stripped with `Abort`
/// when the pre-run tag record lists it); the tag groups from the run's
/// on-box record; the provider settings from the pre-image records; a
/// provider the run created deleted only with the signature captured right
/// after the run's own create; the verify against the pre-image exports,
/// export against export. A green undo puts the pre-run tag records back,
/// clears the journal and ends `rolled_back` (or `tags_rolled_back` when the
/// run did not touch the project), slug `deploy_rolled_back`, exit 6. A node
/// the run created in a provider that existed is removed through the
/// master's tagDelete route when it answers and lists the provider (26-19,
/// on [`roll_back`]'s terms); otherwise (R3-DEL) the undo ends
/// `rollback_failed` naming it after every other restore, and the journal
/// stays.
///
/// With `check` it reads only and reports what it would restore in
/// `data.tags.rollback`. Without it the journal is written `rolling_back`
/// before the first gateway write, and the undo keeps a run directory of its
/// own for its events and `result.json`.
pub async fn deploy_projects_rollback_run(
    loaded: &LoadedProject,
    input: &ProjectsInput,
    surfaces: &ProjectsSurfaces<'_>,
    opts: &ProjectsRunOptions<'_>,
    run_id: &str,
    check: bool,
    observer: &dyn ProjectsObserver,
) -> Result<DeployProjectsResult, CoreError> {
    let target = RunId::parse(run_id)?;
    if loaded.owned_tags.is_empty() {
        return Err(CoreError::InvalidInput {
            reason: "--rollback-run undoes a tag-owning run, and deploy/project.yaml lists no \
                     tag_providers"
                .to_string(),
        });
    }
    let project = loaded.file.project.as_str();
    let gateway = surfaces.gateway;
    // The project is restored by import, so a project the git module manages
    // refuses as it does for a deploy.
    let git = surfaces.host.git_entry(project).await?;
    if git.is_present() {
        return Err(CoreError::ProjectGitManaged {
            project: project.to_string(),
            location: git_entry_location(project),
            kind: git.as_str().to_string(),
        });
    }
    let set = read_host_set(loaded, input, surfaces.host, &target).await?;
    let mut writes = undo_writes(&set);
    let project_undo = match set.run.project_action.as_str() {
        "import" => Some(ProjectUndo {
            pre: set.project_zip.clone(),
        }),
        "create" => Some(ProjectUndo { pre: None }),
        _ => None,
    };
    let live_sha256 = project_sha(gateway, project).await;

    // The group commit's on-box record is read through the apply host, built
    // here as the run builds it, only for a run that committed groups.
    let built_host;
    let apply_host: Option<&dyn ApplyHost> = match surfaces.apply_host {
        Some(host) => Some(host),
        None if set.run.groups => {
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
    writes.groups = undo_groups(&set, input, apply_host, &target).await;
    let mut result = rollback_run_result(loaded, input, &set, &writes, live_sha256, check);
    // The master's tagDelete route (26-19): the undo removes what the run
    // created through it when it answers and lists the provider. A check
    // makes one handshake; the undo waits for it, bounded, before its first
    // write.
    let route = match &surfaces.tag_delete {
        TagDeleteSurface::Configured(client) => {
            let answer = if check {
                client.handshake().await
            } else {
                wait_ready(*client, opts.tag_delete_ready, opts.tag_delete_poll).await
            };
            RouteState::of(&surfaces.tag_delete, Some(answer))
        }
        surface => RouteState::of(surface, None),
    };
    if let Some(tags) = result.tags.as_mut() {
        tags.delete_route = Some(route.report(&owned_names(loaded)));
    }
    if let (Some(_), None) = (route.route, &route.handshake) {
        result.warnings.push(format!(
            "the tag-delete route is not ready ({}); the undo cannot remove the tags run {} \
             created",
            route.problem.as_deref().unwrap_or("no handshake"),
            target.as_str()
        ));
    }

    if check {
        rollback_run_preview(
            gateway,
            loaded,
            &set,
            &writes,
            writes.groups.as_ref(),
            &target,
            &route,
            &mut result,
        )
        .await;
        return Ok(result);
    }

    // The writers, built before anything is written.
    let built_run;
    let run_writer: &dyn ProjectRunWriter = match opts.run_writer {
        Some(writer) => writer,
        None => {
            built_run = project_run_writer_for_node(
                &opts.targets_root,
                &opts.target_file,
                "nodes.master",
                &opts.master,
            )?;
            built_run.as_ref()
        }
    };
    let built_tags;
    let tag_writer: &dyn TagRecordWriter = match opts.tag_record_writer {
        Some(writer) => writer,
        None => {
            built_tags = tag_record_writer_for_node(
                &opts.targets_root,
                &opts.target_file,
                "nodes.master",
                &opts.master,
            )?;
            built_tags.as_ref()
        }
    };
    let built_release;
    let release_writer: &dyn ReleaseRecordWriter = match opts.record_writer {
        Some(writer) => writer,
        None => {
            built_release = release_writer_for_node(
                &opts.targets_root,
                &opts.target_file,
                "nodes.master",
                &opts.master,
            )?;
            built_release.as_ref()
        }
    };
    // The pre-checks read and write nothing.
    wrong_target_check(loaded, gateway, opts).await?;
    // Open Designer sessions matter only when the project is imported again.
    let designers = if project_undo.as_ref().is_some_and(|undo| undo.pre.is_some()) {
        designers_check(&result, gateway, &opts.designers, observer).await?
    } else {
        DesignersFound {
            mode: "skipped",
            timeout_s: None,
            polls: Vec::new(),
            seen: Vec::new(),
            proceeded: false,
            skipped: true,
        }
    };
    let (rid, run_dir) = RunDir::create_fresh(&opts.repo, &loaded.target.gateway)?;
    let rel = run_dir_rel(&loaded.target.gateway, &rid);
    result.run_id = Some(rid.as_str().to_string());
    result.run_dir = Some(rel.clone());
    let surfaces = &ProjectsSurfaces {
        gateway: surfaces.gateway,
        host: surfaces.host,
        tree: surfaces.tree,
        apply_host,
        deploy: surfaces.deploy,
        tag_delete: surfaces.tag_delete.clone(),
    };
    let outcome = async {
        run_dir.write_json(
            "run.json",
            &serde_json::json!({
                "kind": "rollback_run",
                "run_id": rid.as_str(),
                "target_run_id": target.as_str(),
                "started_at": rfc3339_now_utc(),
                "env": input.env,
                "gateway": loaded.target.gateway,
                "project": project,
                "release": set.run.release,
                "tag_providers": loaded.file.tag_providers,
                "designers": designers.mode,
                "designers_seen": sessions_json(&designers.seen),
                "run_dir_git": run_dir.git_status().as_record(),
            }),
        )?;
        let mut events = RunEvents {
            log: EventLog::open(&run_dir)?,
            observer,
            late_failure: false,
        };
        events.emit(ProjectsEvent::RunStarted {
            run_id: rid.as_str().to_string(),
            action: result.action,
        })?;
        if designers.proceeded {
            events.emit(ProjectsEvent::DesignersProceed {
                sessions: designers.seen.clone(),
            })?;
        }
        // The journal says rolling_back before the first gateway write; a
        // host that cannot take it stops the undo with nothing written.
        let journal = ProjectJournal {
            schema_version: 1,
            gateway: loaded.target.gateway.clone(),
            project: project.to_string(),
            run_id: target.as_str().to_string(),
            state: JournalState::RollingBack,
            started_at: rfc3339_now_utc(),
            providers: set.run.providers.iter().map(|p| p.name.clone()).collect(),
        };
        run_writer
            .write_journal(&loaded.target.gateway, project, &journal.to_bytes())
            .await?;
        events.emit(ProjectsEvent::JournalWritten {
            state: JournalState::RollingBack.as_str(),
        })?;
        writes.host = Some(HostCopy {
            run: set.run.clone(),
            journal,
        });
        let failure = TagFailure {
            cause: RollbackCause::RollbackRun,
            what: format!("the undo of run {} from the host copy", target.as_str()),
            writes,
        };
        roll_back(
            failure,
            project_undo,
            result,
            loaded,
            input,
            surfaces,
            opts,
            &rid,
            &rel,
            UndoWriters {
                run: Some(run_writer),
                records: Some(tag_writer),
                release: Some(release_writer),
            },
            &route,
            &mut events,
        )
        .await
    }
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
            let _ = run_dir.write_json(
                "result.json",
                &serde_json::json!({
                    "ok": false,
                    "slug": err.code(),
                    "run_id": rid.as_str(),
                    "target_run_id": target.as_str(),
                }),
            );
            Err(err)
        }
    }
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
    live: &ProviderReads,
) -> Result<Option<GroupWork>, CoreError> {
    if report.groups.is_empty() {
        return Ok(None);
    }
    let reader = need_tree(surfaces, "the tag-group write")?;
    let inputs = group_inputs(loaded, tag_plan)?;
    let providers: Vec<String> = report.groups.iter().map(|g| g.provider.clone()).collect();
    let observed = read_groups(reader, &loaded.target.mode, &providers).await?;
    let plans = group_plans(&inputs, &observed, tag_records, live)?;
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
                recover_with: None,
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
/// deleted, and the finalized run record (26-11 restores from it).
async fn commit_groups(
    loaded: &LoadedProject,
    surfaces: &ProjectsSurfaces<'_>,
    run_id: &RunId,
    plan_sha256: &str,
    work: &GroupWork,
) -> Result<(usize, usize, RunRecord), GroupFailure> {
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
    Ok((spec.writes.len(), spec.deletes.len(), record))
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
    tags.deleted_count = step.deleted.len();
    tags.deleted = step.deleted.iter().take(LIST_CAP).cloned().collect();
    // A removal gone by the time of its delete is already absent (26-20);
    // the plan's removed list stays as reviewed.
    if !step.absent.is_empty() {
        tags.already_absent_count += step.absent.len();
        tags.already_absent.extend(step.absent.iter().cloned());
        tags.already_absent
            .sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        tags.already_absent.dedup();
        tags.already_absent.truncate(LIST_CAP);
    }
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
/// `backup_unsynced` is a backup that did not converge (26-13): a tag-owning
/// run then ends `applied_backup_unsynced`. `record_written` is false for a
/// tag-owning run whose verified tags stand without a release record
/// (26-REVIEW WR-02).
fn finish_recorded(
    result: &mut DeployProjectsResult,
    events: &mut RunEvents<'_>,
    backup_unsynced: bool,
    record_written: bool,
) {
    result.record_written = record_written;
    result.outcome = if result.tags.is_some() {
        ProjectOutcome::Applied
    } else {
        ProjectOutcome::Recorded
    };
    if backup_unsynced && result.tags.is_some() {
        mark_backup_unsynced(result);
    }
    if record_written {
        events.emit_late(ProjectsEvent::RecordWritten);
    }
    events.emit_late(ProjectsEvent::Finished {
        outcome: result.outcome,
    });
    note_late_log_failure(result, events);
}

/// The verdict of a green tag-owning run whose backup did not converge
/// (26-13): the master stays applied and recorded.
fn mark_backup_unsynced(result: &mut DeployProjectsResult) {
    result.outcome = ProjectOutcome::AppliedBackupUnsynced;
    result.ok = false;
    result.slug = Some(BACKUP_UNSYNCED_SLUG.to_string());
}

/// The run's event for a step of the backup wait.
fn backup_event(event: TagsBackupEvent) -> ProjectsEvent {
    match event {
        TagsBackupEvent::WaitStarted { budget_s } => ProjectsEvent::BackupWaitStarted { budget_s },
        TagsBackupEvent::Resync { ok, code } => ProjectsEvent::BackupResync { ok, code },
        TagsBackupEvent::Verdict { converged, reason } => ProjectsEvent::BackupVerdict {
            converged,
            reason: reason.map(|reason| reason.as_str()),
        },
    }
}

/// On a pair (`opts.backup`), wait for the backup to hold the master's
/// `providers` and put the report in `data.tags.backup` (26-13). `None`
/// without a backup node or without a tags report: no call is made.
/// Otherwise whether the backup converged. The master's exports are read
/// once, here; the backup is read through [`BackupTags`] only, and the one
/// resync goes to the master's deploy surface. Never an error.
async fn backup_step(
    result: &mut DeployProjectsResult,
    providers: &[&str],
    surfaces: &ProjectsSurfaces<'_>,
    opts: &ProjectsRunOptions<'_>,
    since_ms: i64,
    events: &mut RunEvents<'_>,
) -> Option<bool> {
    let backup = opts.backup?;
    result.tags.as_ref()?;
    let mut master = BTreeMap::new();
    let mut unread = None;
    for name in providers {
        match surfaces.gateway.tag_export(name).await {
            Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
                Ok(tree) => {
                    master.insert((*name).to_string(), tree);
                }
                Err(_) => {
                    unread = Some(format!("[{name}] is not JSON"));
                    break;
                }
            },
            Err(err) => {
                unread = Some(format!("[{name}] {}", err.code()));
                break;
            }
        }
    }
    let report = match unread {
        None => {
            let resync = surfaces.deploy.map(DeployResync);
            converge_tags(
                &master,
                backup,
                resync.as_ref().map(|r| r as &dyn MasterResync),
                opts.backup_policy,
                since_ms,
                &mut |event| events.emit_late(backup_event(event)),
            )
            .await
        }
        Some(why) => {
            result.warnings.push(format!(
                "the backup check could not read the master's export ({why}), so the backup's \
                 copy of the owned tags is not proven"
            ));
            let mut names: Vec<String> = providers.iter().map(|p| (*p).to_string()).collect();
            names.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            TagsBackupReport {
                converged: false,
                reason: Some(crate::deploy::pair::UnsyncedReason::Unreachable),
                budget_s: opts.backup_policy.budget.as_secs(),
                elapsed_s: 0,
                resync_sent: false,
                providers: names,
                differing: Vec::new(),
            }
        }
    };
    let converged = report.converged;
    if let Some(tags) = result.tags.as_mut() {
        tags.backup = Some(report);
    }
    Some(converged)
}

/// Warn when an event could not be logged after the gateway or host changed.
fn note_late_log_failure(result: &mut DeployProjectsResult, events: &RunEvents<'_>) {
    if events.late_failure {
        result
            .warnings
            .push("the run's events log could not be written to the end".to_string());
    }
}

/// A finished `record` result, for the unit tests of this crate (the
/// verdict exit test in `error.rs` reads it too).
#[cfg(test)]
pub(crate) fn sample_result() -> DeployProjectsResult {
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

    use super::sample_result;

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

        finish_recorded(&mut result, &mut events, false, true);

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
