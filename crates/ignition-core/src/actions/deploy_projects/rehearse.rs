//! `ign deploy projects --rehearse <REHEARSAL_ENV>` (Phase 26 hardening):
//! the tag steps of a run and its rollback, proven on a disposable gateway
//! that holds the production environment's live tags, before the apply.
//!
//! Some gateway behaviour no static check predicts: what an import stores
//! in another form than it was sent, and an `Overwrite` that a gateway
//! refuses while instances inherit a member. A rehearsal runs the run's own
//! tag functions against a gateway seeded with `<ENV>`'s live tags, so that
//! behaviour shows up there. It never writes to `<ENV>`.
//!
//! [`deploy_projects_rehearse_prepare`] makes the reads, in this order, and
//! writes nothing:
//!
//! 1. the rehearsal target, offline ([`check_rehearsal_target`]): it declares
//!    `rehearsal: true`, has host access (`local` or `ssh`), and neither its
//!    gateway directory, its declared `gateway_name`, its profile nor its
//!    profile URL is one of `<ENV>`'s;
//! 2. both gateways' `gateway-info` names: the rehearsal gateway reports the
//!    name it declares, and `<ENV>`'s master reports another;
//! 3. `<ENV>`'s plan through [`deploy_projects_prepare`], the reads of
//!    `--check` (GET only), and the run's removal gates.
//!
//! After the confirmation [`deploy_projects_rehearse`] writes to the
//! rehearsal gateway only:
//!
//! 1. the run's route gate against the rehearsal profile's tag-delete route
//!    (`tags_undo_incomplete` without `--allow-manual-undo`, as the run);
//! 2. a run directory `.ign/deploy/<rehearsal gateway>/<id>/` (the id is the
//!    rehearsal id), holding the tag pre-images, `events.ndjson`,
//!    `plan.json` and `result.json`;
//! 3. the seed: `<ENV>`'s owned providers are exported again (GET) and must
//!    still hash as the plan's `live_sha256`; per provider the rehearsal
//!    gateway's one is deleted, created with `<ENV>`'s settings (only a
//!    `STANDARD` provider; anything else, a create the rehearsal gateway
//!    refuses or settings it stores otherwise is created `STANDARD` and
//!    reported `settings: not_rehearsed`, and the run's settings step then
//!    leaves it alone), and `<ENV>`'s export imported at the root with
//!    `Overwrite` (retried while the new provider still answers "provider not
//!    found", for up to 30 s). A provider `<ENV>` lacks is deleted and left for the run
//!    to create. The export read back must equal `<ENV>`'s, export against
//!    export, and hash as the plan's `live_sha256`; a difference ends the
//!    rehearsal red at `seed`, naming the nodes;
//! 4. the run's tag step ([`deploy_owned_tags`]) with the project step
//!    skipped: the pre-images, the re-plan (it must be `<ENV>`'s plan),
//!    the carry guard, the settings, the import batches with their prunes,
//!    the removals and the forward tag verify. The project is never
//!    imported to a rehearsal gateway;
//! 5. a forced rollback, the run's own [`roll_back`] (cause `rehearsal`),
//!    after a green tag step and after a red one. With
//!    `--allow-manual-undo` the created nodes of `undo_manual` are left by
//!    it as a run would leave them and reported, and the rehearsal then
//!    deletes and re-seeds their providers (cleanup);
//! 6. the record `.ign/deploy/<ENV gateway>/rehearsals/<id>.json` (0600):
//!    green only when the seed, the forward verify and the rollback are.
//!
//! Tag groups are not rehearsed: the rehearsal's run leaves them alone and
//! says so (`seed.groups`). A red rehearsal is a verdict: the success
//! envelope with `data.ok: false` and `data.slug` [`REHEARSAL_RED_SLUG`],
//! exit 6 after it.
//!
//! [`rehearsal_gate`] is the apply's side: a target that declares
//! `require_rehearsal: true` refuses a tag-owning apply without
//! `--rehearsal <ID>` (`rehearsal_required`), and a given record must be
//! green and hold this apply's environment, project, `tags_sha256`,
//! `live_tags_sha256` (the plan's `live_sha256`, which the run checks its
//! pre-images against), `plan_sha256`, `settings_sha256` and the running ign
//! version, and keep a forward verify that was equal and a green rollback
//! with an equal verify (`rehearsal_stale`).
//!
//! The live gate is `tests/live_deploy_rehearse.rs` (ignored): the 8.3.6 rig
//! at `http://localhost:18290` as the rehearsal gateway.

use serde::Deserialize;

use super::*;

/// The verdict slug of a red rehearsal: `data.slug` beside `data.ok: false`
/// on the success envelope, then exit 6. Not a [`CoreError`] and not in
/// [`CoreError::DEPLOY_SLUGS`].
pub const REHEARSAL_RED_SLUG: &str = "deploy_rehearsal_red";

/// The directory below `.ign/deploy/<gateway>/` that holds the rehearsal
/// records of that gateway's environment.
pub const REHEARSALS_DIR: &str = "rehearsals";

/// `.ign/deploy/<gateway>/rehearsals/<id>.json` below the repo.
pub fn rehearsal_record_rel(gateway: &str, id: &str) -> String {
    format!(
        "{}/{gateway}/{REHEARSALS_DIR}/{id}.json",
        crate::deploy::apply::run_store::RUN_DIR_ROOT
    )
}

/// The rehearsal gateway, as the caller resolved it.
pub struct RehearsalTarget<'a> {
    /// `--rehearse`: the environment name of the rehearsal target.
    pub env: String,
    /// Its target file below the targets root.
    pub target_file: String,
    /// The parsed target.
    pub target: Target,
    /// Its surfaces: the gateway, its host and its tag-delete route. The
    /// tree reader, apply host and deploy API are not used (tag groups are
    /// not rehearsed).
    pub surfaces: ProjectsSurfaces<'a>,
    /// The URL of its master profile.
    pub url: String,
    /// The URLs of the production target's node profiles.
    pub env_urls: Vec<String>,
}

/// What a rehearsal needs besides the targets.
#[derive(Debug, Clone)]
pub struct RehearseOptions {
    /// The project repo root: the run directory and the record live below
    /// it.
    pub repo: PathBuf,
    /// The targets root.
    pub targets_root: PathBuf,
    /// The version the running `ign` reports (`ign --version`); bound into
    /// the record and compared by [`rehearsal_gate`].
    pub ign_version: String,
    /// As [`ProjectsRunOptions::tag_verify_settle`].
    pub tag_verify_settle: Duration,
    /// As [`ProjectsRunOptions::tag_verify_poll`].
    pub tag_verify_poll: Duration,
    /// As [`ProjectsRunOptions::tag_delete_ready`].
    pub tag_delete_ready: Duration,
    /// As [`ProjectsRunOptions::tag_delete_poll`].
    pub tag_delete_poll: Duration,
}

/// What [`deploy_projects_rehearse_prepare`] read: the plan of `<ENV>` and
/// the confirmation prose.
#[derive(Debug, Clone)]
pub struct RehearsalPrepared {
    /// The confirmation prose: what is deleted and re-created where.
    pub prose: String,
    plan: DeployProjectsResult,
    tag_records: BTreeMap<String, TagRecord>,
    removals: bool,
}

/// One owned provider of the seed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SeedProvider {
    /// The provider.
    pub provider: String,
    /// `rehearsed`: created with `<ENV>`'s settings; `not_rehearsed`: created
    /// `STANDARD` and its settings left alone by the run; `run_creates`:
    /// `<ENV>` lacks it and the run creates it.
    pub settings: String,
    /// Why the settings are not rehearsed; null otherwise.
    pub reason: Option<String>,
    /// How many nodes `<ENV>`'s export holds.
    pub nodes: usize,
}

/// `data.rehearsal.seed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SeedReport {
    /// One entry per owned provider, byte-sorted.
    pub providers: Vec<SeedProvider>,
    /// `none` (the plan has no tag-group step) or `not_rehearsed`.
    pub groups: String,
    /// Nodes the seeded providers do not hold as `<ENV>` does,
    /// `[P]path (properties)`, byte-sorted and capped.
    pub mismatches: Vec<String>,
    /// Full count of `mismatches`.
    pub mismatches_count: usize,
}

/// `data.rehearsal.forward`: the run's tag step on the rehearsal gateway.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ForwardReport {
    /// The forward tag verify; null when the step stopped before it.
    pub verify: Option<TagVerify>,
    /// What stopped a red step; null when green or not reached.
    pub detail: Option<String>,
    /// Shape-changed members the prune batches left out, full paths.
    pub pruned: Vec<String>,
    /// Full count of `pruned`.
    pub pruned_count: usize,
    /// How many tag writes the step sent.
    pub writes: usize,
}

/// `data.rehearsal.rollback`: the forced rollback.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RollbackReport {
    /// It restored and verified everything it had to.
    pub green: bool,
    /// Its verify, when it ended green.
    pub verify: Option<TagVerify>,
    /// Shape-changed members its prune batches left out, full paths.
    pub pruned: Vec<String>,
    /// Full count of `pruned`.
    pub pruned_count: usize,
    /// Created nodes it left under `--allow-manual-undo` (removed by the
    /// cleanup), full paths.
    pub undo_manual: Vec<String>,
    /// Full count of `undo_manual`.
    pub undo_manual_count: usize,
    /// What is left after a red rollback; null when green or not reached.
    pub detail: Option<String>,
}

/// `data.rehearsal`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RehearsalReport {
    /// The rehearsal id (the run id of its run directory).
    pub id: String,
    /// `green` or `red`.
    pub verdict: String,
    /// `seed`, `forward` or `rollback` for a red rehearsal; null when green.
    pub failed_at: Option<String>,
    /// The rehearsal environment.
    pub env: String,
    /// The rehearsal target's gateway.
    pub gateway: String,
    /// `<ENV>`'s plan `tags_sha256`.
    pub tags_sha256: String,
    /// `<ENV>`'s plan `live_sha256`: its owned live tags, runtime values
    /// stripped.
    pub live_tags_sha256: String,
    /// `<ENV>`'s plan `plan_sha256`.
    pub plan_sha256: String,
    /// The provider settings the apply writes ([`settings_sha256`]).
    pub settings_sha256: String,
    /// The version of the `ign` that rehearsed.
    pub ign_version: String,
    /// The seed.
    pub seed: SeedReport,
    /// The tag step.
    pub forward: ForwardReport,
    /// The forced rollback.
    pub rollback: RollbackReport,
    /// Providers deleted and re-seeded after the rollback.
    pub cleanup: Vec<String>,
    /// The record, relative to the repo.
    pub record: String,
    /// The run directory, relative to the repo.
    pub run_dir: String,
    /// Seconds the rehearsal took.
    pub elapsed_s: f64,
}

/// `ign deploy projects --rehearse` output model.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeployRehearsalResult {
    /// The `--env` value: the production environment rehearsed.
    pub env: String,
    /// Its gateway.
    pub gateway: String,
    /// The project.
    pub project: String,
    /// The release, as given.
    pub release: String,
    /// The rehearsal.
    pub rehearsal: RehearsalReport,
    /// Warnings in order.
    pub warnings: Vec<String>,
    /// False for a red rehearsal.
    pub ok: bool,
    /// [`REHEARSAL_RED_SLUG`] when red; null otherwise.
    pub slug: Option<String>,
}

impl DeployRehearsalResult {
    /// `Some(6)` for a red rehearsal, after the envelope.
    pub fn failure_exit_code(&self) -> Option<u8> {
        (!self.ok).then_some(6)
    }
}

/// A forward or rollback verify as the record keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifySummary {
    /// The verify was green.
    pub equal: bool,
    /// Its mismatches, capped.
    pub mismatches: Vec<String>,
    /// Full count of `mismatches`.
    pub mismatches_count: usize,
}

impl From<&TagVerify> for VerifySummary {
    fn from(v: &TagVerify) -> Self {
        Self {
            equal: v.equal,
            mismatches: v.mismatches.clone(),
            mismatches_count: v.mismatches_count,
        }
    }
}

/// The rollback as the record keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RollbackSummary {
    /// It ended green.
    pub green: bool,
    /// Its verify, when green.
    pub verify: Option<VerifySummary>,
    /// The shape-changed members its prune batches left out.
    pub pruned: Vec<String>,
    /// Created nodes left under `--allow-manual-undo`.
    pub undo_manual: Vec<String>,
    /// What is left after a red rollback.
    pub detail: Option<String>,
}

/// `.ign/deploy/<ENV gateway>/rehearsals/<id>.json`: what a rehearsal
/// proved, bound to its inputs. Hashes, names and paths only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RehearsalRecord {
    /// Always 1.
    pub schema_version: u32,
    /// The rehearsal id.
    pub id: String,
    /// The production environment.
    pub env: String,
    /// Its gateway.
    pub gateway: String,
    /// The project.
    pub project: String,
    /// The release rehearsed.
    pub release: String,
    /// The rehearsal environment.
    pub rehearsal_env: String,
    /// The rehearsal gateway.
    pub rehearsal_gateway: String,
    /// `<ENV>`'s plan `tags_sha256`.
    pub tags_sha256: String,
    /// `<ENV>`'s plan `live_sha256`.
    pub live_tags_sha256: String,
    /// `<ENV>`'s plan `plan_sha256`.
    pub plan_sha256: String,
    /// The provider settings the apply writes ([`settings_sha256`]); empty
    /// in a record written before it was kept, which then never matches.
    #[serde(default)]
    pub settings_sha256: String,
    /// The `ign` version that rehearsed.
    pub ign_version: String,
    /// UTC start.
    pub started_at: String,
    /// UTC end.
    pub finished_at: String,
    /// `green` or `red`.
    pub verdict: String,
    /// Where a red rehearsal failed.
    pub failed_at: Option<String>,
    /// The forward tag verify, when it ran.
    pub forward: Option<VerifySummary>,
    /// The forced rollback, when it ran.
    pub rollback: Option<RollbackSummary>,
}

impl RehearsalRecord {
    /// Parse `bytes` strictly; `location` names the file in errors.
    pub fn parse(bytes: &[u8], location: &str, id: &str) -> Result<Self, CoreError> {
        let invalid = |problem: String| CoreError::DeployRunInvalid {
            run_id: id.to_string(),
            problem: format!("the rehearsal record {location} {problem}"),
        };
        let record: Self = serde_json::from_slice(bytes)
            .map_err(|e| invalid(format!("does not parse ({})", e.classify_name())))?;
        if record.schema_version != 1 {
            return Err(invalid(format!(
                "has schema_version {}; this ign reads 1",
                record.schema_version
            )));
        }
        if record.id != id {
            return Err(invalid(format!("names rehearsal {}", record.id)));
        }
        if !matches!(record.verdict.as_str(), "green" | "red") {
            return Err(invalid(
                "holds a verdict other than green or red".to_string(),
            ));
        }
        Ok(record)
    }
}

/// The serde_json error category, value-free.
trait ClassifyName {
    fn classify_name(&self) -> &'static str;
}

impl ClassifyName for serde_json::Error {
    fn classify_name(&self) -> &'static str {
        match self.classify() {
            serde_json::error::Category::Io => "io",
            serde_json::error::Category::Syntax => "syntax",
            serde_json::error::Category::Data => "data",
            serde_json::error::Category::Eof => "eof",
        }
    }
}

/// A URL compared as text: trailing `/` dropped, case folded.
fn url_key(url: &str) -> String {
    url.trim().trim_end_matches('/').to_ascii_lowercase()
}

/// The offline guard (Phase 26 hardening): `rehearsal` is a target that
/// declares `rehearsal: true` and host access, and none of its gateway
/// directory, declared gateway name, master profile or profile URL is one
/// of `env`'s (`env_urls` are the URLs of `env`'s node profiles).
pub fn check_rehearsal_target(
    env: &Target,
    env_urls: &[String],
    file: &str,
    rehearsal: &Target,
    url: &str,
) -> Result<(), CoreError> {
    let invalid = |reason: String| CoreError::RehearsalTargetInvalid {
        file: file.to_string(),
        reason,
    };
    if !rehearsal.rehearsal {
        return Err(invalid(
            "it does not declare `rehearsal: true`, so its tag providers may not be wiped"
                .to_string(),
        ));
    }
    let master = &rehearsal.nodes.master;
    if master.local.is_none() && master.ssh.is_none() {
        return Err(invalid(
            "nodes.master declares neither local nor ssh host access".to_string(),
        ));
    }
    let Some(name) = master.gateway_name.as_deref() else {
        return Err(invalid(
            "nodes.master declares no gateway_name to prove the gateway by".to_string(),
        ));
    };
    if rehearsal.gateway == env.gateway {
        return Err(invalid(format!(
            "its gateway {} is the production target's",
            rehearsal.gateway
        )));
    }
    let env_nodes = std::iter::once(("nodes.master", &env.nodes.master))
        .chain(env.nodes.backup.as_ref().map(|n| ("nodes.backup", n)));
    for (label, node) in env_nodes {
        let names = [Some(env.gateway.as_str()), node.gateway_name.as_deref()];
        if names.iter().flatten().any(|n| n.eq_ignore_ascii_case(name)) {
            return Err(invalid(format!(
                "its gateway_name {name} is a name of the production target's {label}"
            )));
        }
        if node.profile == master.profile {
            return Err(invalid(format!(
                "its profile {} is the production target's {label} profile",
                master.profile
            )));
        }
    }
    if env_urls.iter().any(|u| url_key(u) == url_key(url)) {
        return Err(invalid(format!(
            "its profile URL {url} is a production node's URL"
        )));
    }
    Ok(())
}

/// What both gateways report: the rehearsal gateway must report the name it
/// declares and the redundancy role `Independent`, and `<ENV>`'s master
/// another name. Reads only.
async fn check_reported_names(
    rehearsal: &RehearsalTarget<'_>,
    env_gateway: &dyn ProjectGateway,
) -> Result<(), CoreError> {
    let invalid = |reason: String| CoreError::RehearsalTargetInvalid {
        file: rehearsal.target_file.clone(),
        reason,
    };
    let declared = rehearsal
        .target
        .nodes
        .master
        .gateway_name
        .clone()
        .unwrap_or_default();
    let reported = rehearsal.surfaces.gateway.gateway_name().await?;
    if reported.as_deref() != Some(declared.as_str()) {
        return Err(invalid(format!(
            "the gateway its profile reaches reports the name {}, not the declared {declared}",
            reported.as_deref().unwrap_or("(none)")
        )));
    }
    // A node of a redundant pair (the production backup reached by another
    // address included) never reports `Independent`: only a standalone
    // gateway is seeded.
    let role = rehearsal.surfaces.gateway.redundancy_role().await?;
    if role.as_deref() != Some("Independent") {
        return Err(invalid(format!(
            "the gateway its profile reaches reports the redundancy role {}, not Independent; \
             a rehearsal gateway is a standalone gateway, never a node of a redundant pair",
            role.as_deref().unwrap_or("(none)")
        )));
    }
    let env_reported = env_gateway.gateway_name().await?;
    if env_reported
        .as_deref()
        .is_some_and(|n| n.eq_ignore_ascii_case(&declared))
    {
        return Err(invalid(format!(
            "the production master reports the same name {declared}"
        )));
    }
    Ok(())
}

/// `loaded` with the rehearsal target in place of `<ENV>`'s.
fn rehearsal_loaded(loaded: &LoadedProject, rehearsal: &RehearsalTarget<'_>) -> LoadedProject {
    let mut out = loaded.clone();
    out.target_file.clone_from(&rehearsal.target_file);
    out.target = rehearsal.target.clone();
    out
}

/// The reads and the refusals of a rehearsal, before any write (see the
/// module documentation).
pub async fn deploy_projects_rehearse_prepare(
    loaded: &LoadedProject,
    input: &ProjectsInput,
    surfaces: &ProjectsSurfaces<'_>,
    rehearsal: &RehearsalTarget<'_>,
) -> Result<RehearsalPrepared, CoreError> {
    if loaded.owned_tags.is_empty() {
        return Err(CoreError::InvalidInput {
            reason: "--rehearse: deploy/project.yaml lists no tag_providers, so there is no tag \
                     step to rehearse"
                .to_string(),
        });
    }
    check_rehearsal_target(
        &loaded.target,
        &rehearsal.env_urls,
        &rehearsal.target_file,
        &rehearsal.target,
        &rehearsal.url,
    )?;
    check_reported_names(rehearsal, surfaces.gateway).await?;
    let (plan, tag_records) = match deploy_projects_prepare(loaded, input, surfaces).await? {
        ProjectsPrepared::Ready {
            plan, tag_records, ..
        } => (plan, tag_records),
        ProjectsPrepared::Unchanged(_) => {
            return Err(CoreError::InvalidInput {
                reason: "--rehearse: the plan changes nothing, so there is nothing to rehearse"
                    .to_string(),
            });
        }
    };
    let rloaded = rehearsal_loaded(loaded, rehearsal);
    let removals = removal_gate(&plan, &rloaded, input, &rehearsal.surfaces)?;
    let prose = format!(
        "deploy projects will rehearse the tag steps of project {} release {} for environment \
         {} on gateway {} (env {}): it deletes and re-creates tag provider(s) {} there, seeds \
         them from {}'s live tags, runs the tag step and a forced rollback, and writes a \
         rehearsal record. {} is only read; the project is not imported",
        loaded.file.project,
        input.release,
        input.env,
        rehearsal.target.gateway,
        rehearsal.env,
        loaded.file.tag_providers.join(", "),
        input.env,
        input.env,
    );
    Ok(RehearsalPrepared {
        prose,
        plan,
        tag_records,
        removals,
    })
}

/// Whether `config` is a `STANDARD` provider's.
fn is_standard(config: &Value) -> bool {
    config.pointer("/profile/type").and_then(Value::as_str) == Some("STANDARD")
}

/// Delete `name` on `gateway` when it is there, with its own signature.
async fn drop_provider(gateway: &dyn ProjectGateway, name: &str) -> Result<(), CoreError> {
    if let Some(record) = gateway.tag_provider_record(name).await? {
        gateway
            .tag_provider_delete(name, record.signature.as_deref().unwrap_or_default())
            .await?;
    }
    Ok(())
}

/// What seeding one provider found.
struct Seeded {
    report: SeedProvider,
    /// The settings the run plans against instead of the repo's, when the
    /// provider's settings are not rehearsed.
    substitute: Option<(bool, Option<String>, Value)>,
}

/// Seed `name` on the rehearsal gateway `rig` from `<ENV>`'s `record` and
/// `export` (see the module documentation). `Err` is a gateway error.
async fn seed_provider(
    rig: &dyn ProjectGateway,
    name: &str,
    record: &TagProviderRecord,
    export: &[u8],
    wait: ProviderWait,
) -> Result<Seeded, CoreError> {
    drop_provider(rig, name).await?;
    let mut reason = None;
    if is_standard(&record.config) {
        let sent = TagProviderCreate::with_settings(
            name,
            record.enabled,
            record.description().map(str::to_string),
            record.config.clone(),
        );
        match rig.tag_provider_create(&[sent]).await {
            Ok(()) => match rig.tag_provider_record(name).await? {
                Some(back) if settings_equal(&back, record) => {}
                _ => {
                    reason = Some("the rehearsal gateway stores other settings".to_string());
                    drop_provider(rig, name).await?;
                }
            },
            Err(err) => {
                reason = Some(format!(
                    "the rehearsal gateway refused the create ({})",
                    err.code()
                ));
                drop_provider(rig, name).await?;
            }
        }
    } else {
        reason = Some("not a STANDARD provider".to_string());
    }
    if reason.is_some() {
        rig.tag_provider_create(&[TagProviderCreate::standard(name)])
            .await?;
    }
    let tree: Value = serde_json::from_slice(export).map_err(|_| {
        CoreError::Internal(format!(
            "the tag export of provider {name} is not a tag tree"
        ))
    })?;
    let body = serde_json::to_vec(&serde_json::json!({
        "tags": tree.get("tags").cloned().unwrap_or_else(|| Value::Array(Vec::new()))
    }))
    .map_err(|e| CoreError::Internal(format!("the seed of provider {name}: {e}")))?;
    let answer = import_when_ready(rig, name, "", CollisionPolicy::Overwrite, body, wait).await?;
    if !answer.is_clean() {
        return Err(CoreError::Internal(format!(
            "the rehearsal gateway did not take the seed of provider {name} cleanly ({} \
             failure(s))",
            answer.failure_count.unwrap_or(1)
        )));
    }
    let substitute = match &reason {
        None => None,
        Some(_) => {
            let back = rig.tag_provider_record(name).await?.ok_or_else(|| {
                CoreError::Internal(format!(
                    "the rehearsal gateway lists no provider {name} after its create"
                ))
            })?;
            Some((
                back.enabled,
                back.description().map(str::to_string),
                back.config.clone(),
            ))
        }
    };
    Ok(Seeded {
        report: SeedProvider {
            provider: name.to_string(),
            settings: if reason.is_some() {
                "not_rehearsed"
            } else {
                "rehearsed"
            }
            .to_string(),
            reason,
            nodes: 0,
        },
        substitute,
    })
}

/// `[P]path (properties)` for each node `seeded` does not hold as `env`
/// does, export against export.
fn seed_mismatches(
    name: &str,
    env: &BTreeMap<String, Value>,
    seeded: &BTreeMap<String, Value>,
) -> Vec<String> {
    let mut out = Vec::new();
    for (path, node) in env {
        match seeded.get(path) {
            None => out.push(format!("{} (absent)", full_tag_path(name, path))),
            Some(now) => {
                let properties = diff_properties(node, now, path, EXPORTS);
                if !properties.is_empty() {
                    out.push(format!(
                        "{} ({})",
                        full_tag_path(name, path),
                        properties.join(", ")
                    ));
                }
            }
        }
    }
    out.extend(
        seeded
            .keys()
            .filter(|path| !env.contains_key(*path))
            .map(|path| format!("{} (not in the export)", full_tag_path(name, path))),
    );
    out
}

/// The seed of every owned provider, the substitutions it made in `rloaded`
/// and `rplan`, and the mismatches of a seed that does not hold `<ENV>`'s
/// tags.
async fn seed_all(
    loaded: &LoadedProject,
    env_gateway: &dyn ProjectGateway,
    rig: &dyn ProjectGateway,
    rloaded: &mut LoadedProject,
    rplan: &mut DeployProjectsResult,
    tag_records: &BTreeMap<String, TagRecord>,
    groups_planned: bool,
) -> Result<SeedReport, CoreError> {
    let Some(tags) = rplan.tags.as_mut() else {
        return Err(CoreError::Internal(
            "a rehearsal plan without tags".to_string(),
        ));
    };
    let mut providers = Vec::new();
    let mut env_live = BTreeMap::new();
    let mut seeds = Vec::new();
    // `<ENV>` first, reads only, and it must still be the plan's.
    for provider in sorted(&loaded.owned_tags) {
        let name = provider.name.as_str();
        let creates = tags
            .provider_settings
            .iter()
            .any(|p| p.provider == name && p.action == ProviderSettingsAction::Create);
        if creates {
            seeds.push((name.to_string(), None));
            continue;
        }
        let record = env_gateway
            .tag_provider_record(name)
            .await?
            .ok_or_else(|| CoreError::PlanStale {
                expected: "the provider listed".to_string(),
                actual: "absent".to_string(),
                detail: format!("tag provider {name} of the production environment is gone"),
            })?;
        let bytes = env_gateway.tag_export(name).await?;
        env_live.insert(name.to_string(), flatten_export(&bytes, name)?);
        seeds.push((name.to_string(), Some((record, bytes))));
    }
    let now = plan_tags(&loaded.owned_tags, &env_live, tag_records).live_sha256;
    if now != tags.live_sha256 {
        return Err(CoreError::PlanStale {
            expected: tags.live_sha256.clone(),
            actual: now,
            detail: "the production environment's owned tags changed since the plan".to_string(),
        });
    }
    // Then the rehearsal gateway.
    for (name, env) in seeds {
        let Some((record, bytes)) = env else {
            drop_provider(rig, &name).await?;
            let repo = provider_settings_of(loaded, &name)?;
            let (settings, reason) = if is_standard(&repo.config) {
                ("run_creates", None)
            } else {
                // The run creates it STANDARD on the rehearsal gateway.
                let i = loaded
                    .file
                    .tag_providers
                    .iter()
                    .position(|p| *p == name)
                    .ok_or_else(|| CoreError::Internal(format!("no settings of {name}")))?;
                rloaded.provider_settings[i].config = TagProviderCreate::standard(&name).config;
                (
                    "not_rehearsed",
                    Some("not a STANDARD provider; the run creates it STANDARD".to_string()),
                )
            };
            providers.push(SeedProvider {
                provider: name,
                settings: settings.to_string(),
                reason,
                nodes: 0,
            });
            continue;
        };
        let mut seeded = seed_provider(rig, &name, &record, &bytes, PROVIDER_WAIT).await?;
        seeded.report.nodes = env_live.get(&name).map_or(0, BTreeMap::len);
        if let Some((enabled, description, config)) = seeded.substitute {
            let i = loaded
                .file
                .tag_providers
                .iter()
                .position(|p| *p == name)
                .ok_or_else(|| CoreError::Internal(format!("no settings of {name}")))?;
            let settings = &mut rloaded.provider_settings[i];
            settings.enabled = enabled;
            settings.description = description;
            settings.config = config;
            if let Some(entry) = tags
                .provider_settings
                .iter_mut()
                .find(|p| p.provider == name)
            {
                entry.action = ProviderSettingsAction::Same;
                entry.properties.clear();
            }
        }
        providers.push(seeded.report);
    }
    // The seeded providers must hold `<ENV>`'s tags.
    let mut mismatches = Vec::new();
    let mut rig_live = BTreeMap::new();
    for (name, env) in &env_live {
        let now = flatten_export(&rig.tag_export(name).await?, name)?;
        mismatches.extend(seed_mismatches(name, env, &now));
        rig_live.insert(name.clone(), now);
    }
    if mismatches.is_empty() {
        let seeded = plan_tags(&loaded.owned_tags, &rig_live, tag_records).live_sha256;
        if seeded != tags.live_sha256 {
            mismatches.push(format!(
                "the seeded providers hash {seeded}, the production environment's live tags {}",
                tags.live_sha256
            ));
        }
    }
    mismatches.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    let (mismatches, mismatches_count) = capped(mismatches);
    Ok(SeedReport {
        providers,
        // From `<ENV>`'s plan: the rehearsal's own leaves its group steps out.
        groups: if groups_planned {
            "not_rehearsed"
        } else {
            "none"
        }
        .to_string(),
        mismatches,
        mismatches_count,
    })
}

/// Write `record` to `.ign/deploy/<gateway>/rehearsals/<id>.json` below
/// `repo`, once, 0600 in 0700 directories on Unix.
fn write_record(repo: &std::path::Path, record: &RehearsalRecord) -> Result<String, CoreError> {
    let rel = rehearsal_record_rel(&record.gateway, &record.id);
    let path = repo.join(&rel);
    let fail = |what: &str, e: &std::io::Error| CoreError::DeployRunInvalid {
        run_id: record.id.clone(),
        problem: format!(
            "could not {what} the rehearsal record {rel} ({:?})",
            e.kind()
        ),
    };
    let dir = path.parent().expect("the record has a parent");
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(dir)
        .map_err(|e| fail("create the directory of", &e))?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut bytes = serde_json::to_vec_pretty(record)
        .map_err(|e| CoreError::Internal(format!("the rehearsal record: {e}")))?;
    bytes.push(b'\n');
    let mut file = options.open(&path).map_err(|e| fail("create", &e))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| fail("write", &e))?;
    Ok(rel)
}

/// The rehearsal's writes, all to the rehearsal gateway (see the module
/// documentation). `prepared` is what [`deploy_projects_rehearse_prepare`]
/// returned for the same inputs. A red rehearsal is `Ok` with `ok: false`;
/// an `Err` writes no record.
pub async fn deploy_projects_rehearse(
    prepared: RehearsalPrepared,
    loaded: &LoadedProject,
    input: &ProjectsInput,
    surfaces: &ProjectsSurfaces<'_>,
    rehearsal: &RehearsalTarget<'_>,
    opts: &RehearseOptions,
    observer: &dyn ProjectsObserver,
) -> Result<DeployRehearsalResult, CoreError> {
    let started = Instant::now();
    let started_at = rfc3339_now_utc();
    let RehearsalPrepared {
        plan,
        tag_records,
        removals,
        ..
    } = prepared;
    let mut rloaded = rehearsal_loaded(loaded, rehearsal);
    let mut rplan = plan.clone();
    let mut warnings = Vec::new();
    let Some(env_tags) = plan.tags.as_ref() else {
        return Err(CoreError::Internal(
            "a rehearsal plan without tags".to_string(),
        ));
    };
    // Tag groups are not rehearsed: the run's group step is left out.
    if let Some(tags) = rplan.tags.as_mut()
        && !tags.groups.is_empty()
    {
        tags.groups.clear();
        warnings.push(
            "the plan writes tag groups; the rehearsal leaves them alone (seed.groups: \
             not_rehearsed)"
                .to_string(),
        );
    }
    let ropts = ProjectsRunOptions {
        repo: opts.repo.clone(),
        targets_root: opts.targets_root.clone(),
        target_file: rehearsal.target_file.clone(),
        master: rehearsal.target.nodes.master.clone(),
        record_writer: None,
        tag_record_writer: None,
        run_writer: None,
        designers: DesignersPolicy::default(),
        children_settle: Duration::ZERO,
        children_poll: Duration::ZERO,
        tag_verify_settle: opts.tag_verify_settle,
        tag_verify_poll: opts.tag_verify_poll,
        backup: None,
        backup_policy: BackupPolicy::default(),
        tag_delete_ready: opts.tag_delete_ready,
        tag_delete_poll: opts.tag_delete_poll,
    };
    let rsurfaces = &rehearsal.surfaces;
    // The run's route gate, against the rehearsal gateway's route.
    let route = route_gate(
        &mut rplan,
        &rloaded,
        input,
        &rsurfaces.tag_delete,
        &ropts,
        removals,
    )
    .await?;
    let (run_id, run_dir) = RunDir::create_fresh(&opts.repo, &rehearsal.target.gateway)?;
    let rel = run_dir_rel(&rehearsal.target.gateway, &run_id);
    let outcome = rehearse_in(
        &plan,
        env_tags,
        rplan,
        &tag_records,
        loaded,
        &mut rloaded,
        input,
        surfaces,
        rehearsal,
        &ropts,
        opts,
        &run_dir,
        &run_id,
        &rel,
        &route,
        observer,
        warnings,
        (started, started_at),
    )
    .await;
    match outcome {
        Ok(result) => {
            let _ = run_dir.write_json("result.json", &result);
            Ok(result)
        }
        Err(err) => {
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

/// [`deploy_projects_rehearse`] once its run directory exists.
#[allow(clippy::too_many_arguments)]
async fn rehearse_in(
    plan: &DeployProjectsResult,
    env_tags: &TagsReport,
    mut rplan: DeployProjectsResult,
    tag_records: &BTreeMap<String, TagRecord>,
    loaded: &LoadedProject,
    rloaded: &mut LoadedProject,
    input: &ProjectsInput,
    surfaces: &ProjectsSurfaces<'_>,
    rehearsal: &RehearsalTarget<'_>,
    ropts: &ProjectsRunOptions<'_>,
    opts: &RehearseOptions,
    run_dir: &RunDir,
    run_id: &RunId,
    rel: &str,
    route: &RouteState<'_>,
    observer: &dyn ProjectsObserver,
    mut warnings: Vec<String>,
    (started, started_at): (Instant, String),
) -> Result<DeployRehearsalResult, CoreError> {
    let rsurfaces = &rehearsal.surfaces;
    run_dir.write_json(
        "run.json",
        &serde_json::json!({
            "run_id": run_id.as_str(),
            "started_at": started_at,
            "rehearsal": true,
            "env": input.env,
            "env_gateway": plan.gateway,
            "rehearsal_env": rehearsal.env,
            "gateway": rehearsal.target.gateway,
            "project": plan.project,
            "release": plan.release,
            "tags_sha256": env_tags.tags_sha256,
            "live_tags_sha256": env_tags.live_sha256,
            "plan_sha256": env_tags.plan_sha256,
            "allow_manual_undo": input.allow_manual_undo,
            "allow_destructive": input.allow_destructive,
            "ign_version": opts.ign_version,
            "tag_providers": loaded.file.tag_providers,
            "run_dir_git": run_dir.git_status().as_record(),
        }),
    )?;
    let mut events = RunEvents {
        log: EventLog::open(run_dir)?,
        observer,
        late_failure: false,
    };
    events.emit(ProjectsEvent::RunStarted {
        run_id: run_id.as_str().to_string(),
        action: plan.action,
    })?;

    let seed = seed_all(
        loaded,
        surfaces.gateway,
        rsurfaces.gateway,
        rloaded,
        &mut rplan,
        tag_records,
        !env_tags.groups.is_empty(),
    )
    .await?;
    run_dir.write_json("plan.json", &rplan)?;

    let mut forward = ForwardReport {
        verify: None,
        detail: None,
        pruned: Vec::new(),
        pruned_count: 0,
        writes: 0,
    };
    let mut rollback = RollbackReport {
        green: false,
        verify: None,
        pruned: Vec::new(),
        pruned_count: 0,
        undo_manual: Vec::new(),
        undo_manual_count: 0,
        detail: None,
    };
    let mut cleanup = Vec::new();
    let mut ran_forward = false;
    let mut ran_rollback = false;
    if seed.mismatches_count == 0 {
        ran_forward = true;
        let step = deploy_owned_tags(
            &rplan,
            tag_records,
            rloaded,
            input,
            rsurfaces,
            ropts,
            run_dir,
            run_id,
            HostWrites {
                writer: None,
                project_zip: None,
            },
            route,
            &mut events,
        )
        .await;
        let mut result = rplan.clone();
        result.run_id = Some(run_id.as_str().to_string());
        result.run_dir = Some(rel.to_string());
        let mut failure = match step {
            Ok(step) => {
                note_tag_step(&mut result, &step);
                forward.verify = step.verify.clone();
                forward.writes = step.writes.sent.len();
                forward.pruned_count = step.pruned.len();
                forward.pruned = step.pruned.iter().take(LIST_CAP).cloned().collect();
                TagFailure {
                    cause: RollbackCause::Rehearsal,
                    what: "the rehearsal rolls back its tag step".to_string(),
                    writes: step.writes,
                }
            }
            Err(TagStepError::Unwritten(err)) => return Err(err),
            Err(TagStepError::Written(failure)) => {
                forward.verify = failure.writes.verify.clone();
                forward.detail = Some(failure.what.clone());
                forward.writes = failure.writes.sent.len();
                let (pruned, count) = sorted_capped(failure.writes.pruned.clone());
                forward.pruned = pruned;
                forward.pruned_count = count;
                *failure
            }
        };
        // The created nodes `--allow-manual-undo` accepted stay, as a run's
        // rollback would leave them; the cleanup below removes them.
        if input.allow_manual_undo
            && let Some(tags) = rplan.tags.as_ref()
        {
            failure.writes.manual_undo = crate::deploy::tag_apply::undo_manual(
                &tags.undo_candidates,
                route.ready().map(|(_, h)| h),
            );
        }
        ran_rollback = true;
        match roll_back(
            failure,
            None,
            result,
            rloaded,
            input,
            rsurfaces,
            ropts,
            run_id,
            rel,
            UndoWriters {
                run: None,
                records: None,
                release: None,
            },
            route,
            &mut events,
        )
        .await
        {
            Ok(back) => {
                if let Some(done) = back.tags.as_ref().and_then(|t| t.rollback.as_ref()) {
                    rollback.green = true;
                    rollback.verify = done.verify.clone();
                    rollback.pruned = done.pruned.clone();
                    rollback.pruned_count = done.pruned_count;
                    rollback.undo_manual = done.undeletable.clone();
                    rollback.undo_manual_count = done.undeletable_count;
                }
                warnings.extend(back.warnings);
            }
            Err(CoreError::RollbackFailed(failure)) => rollback.detail = Some(failure.cause),
            Err(err) => rollback.detail = Some(format!("{} ({err})", err.code())),
        }
        // Cleanup: a provider holding a node the rollback left is deleted
        // and seeded again, never edited by hand.
        let left: BTreeSet<String> = rollback
            .undo_manual
            .iter()
            .filter_map(|p| p.strip_prefix('[').and_then(|r| r.split_once(']')))
            .map(|(provider, _)| provider.to_string())
            .collect();
        for name in left {
            let reseed = async {
                let record = surfaces
                    .gateway
                    .tag_provider_record(&name)
                    .await?
                    .ok_or_else(|| CoreError::Internal(format!("{name} is gone")))?;
                let bytes = surfaces.gateway.tag_export(&name).await?;
                seed_provider(rsurfaces.gateway, &name, &record, &bytes, PROVIDER_WAIT).await
            };
            match reseed.await {
                Ok(_) => cleanup.push(name),
                Err(err) => warnings.push(format!(
                    "the cleanup of provider {name} on the rehearsal gateway failed ({}); the \
                     next rehearsal seeds it again",
                    err.code()
                )),
            }
        }
    }
    let forward_green =
        forward.verify.as_ref().is_some_and(|v| v.equal) && forward.detail.is_none();
    let failed_at = if seed.mismatches_count > 0 {
        Some("seed")
    } else if !forward_green {
        Some("forward")
    } else if !rollback.green {
        Some("rollback")
    } else {
        None
    };
    let verdict = if failed_at.is_none() { "green" } else { "red" };
    let finished_at = rfc3339_now_utc();
    let record = RehearsalRecord {
        schema_version: 1,
        id: run_id.as_str().to_string(),
        env: input.env.clone(),
        gateway: plan.gateway.clone(),
        project: plan.project.clone(),
        release: plan.release.clone(),
        rehearsal_env: rehearsal.env.clone(),
        rehearsal_gateway: rehearsal.target.gateway.clone(),
        tags_sha256: env_tags.tags_sha256.clone(),
        live_tags_sha256: env_tags.live_sha256.clone(),
        plan_sha256: env_tags.plan_sha256.clone(),
        settings_sha256: settings_sha256(loaded, env_tags),
        ign_version: opts.ign_version.clone(),
        started_at,
        finished_at,
        verdict: verdict.to_string(),
        failed_at: failed_at.map(str::to_string),
        forward: ran_forward
            .then(|| forward.verify.as_ref().map(VerifySummary::from))
            .flatten(),
        rollback: ran_rollback.then(|| RollbackSummary {
            green: rollback.green,
            verify: rollback.verify.as_ref().map(VerifySummary::from),
            pruned: rollback.pruned.clone(),
            undo_manual: rollback.undo_manual.clone(),
            detail: rollback.detail.clone(),
        }),
    };
    let record_rel = write_record(&opts.repo, &record)?;
    if events.late_failure {
        warnings.push("the rehearsal's events log could not be written in full".to_string());
    }
    let ok = failed_at.is_none();
    Ok(DeployRehearsalResult {
        env: input.env.clone(),
        gateway: plan.gateway.clone(),
        project: plan.project.clone(),
        release: plan.release.clone(),
        rehearsal: RehearsalReport {
            id: run_id.as_str().to_string(),
            verdict: verdict.to_string(),
            failed_at: failed_at.map(str::to_string),
            env: rehearsal.env.clone(),
            gateway: rehearsal.target.gateway.clone(),
            tags_sha256: env_tags.tags_sha256.clone(),
            live_tags_sha256: env_tags.live_sha256.clone(),
            plan_sha256: env_tags.plan_sha256.clone(),
            settings_sha256: settings_sha256(loaded, env_tags),
            ign_version: opts.ign_version.clone(),
            seed,
            forward,
            rollback,
            cleanup,
            record: record_rel,
            run_dir: rel.to_string(),
            elapsed_s: (started.elapsed().as_millis() as f64) / 1000.0,
        },
        warnings,
        ok,
        slug: (!ok).then(|| REHEARSAL_RED_SLUG.to_string()),
    })
}

/// sha256 over the provider settings the apply writes: each owned
/// provider's repo settings (`deploy/tag-providers/<P>.json`), the settings
/// plan, and a digest of its live record's values. `plan_sha256` does not cover
/// them, so the gate binds them separately.
pub fn settings_sha256(loaded: &LoadedProject, tags: &TagsReport) -> String {
    let desired: Vec<Value> = loaded
        .owned_tags
        .iter()
        .zip(&loaded.provider_settings)
        .map(|(provider, settings)| {
            serde_json::json!({
                "provider": provider.name,
                "enabled": settings.enabled,
                "description": settings.description,
                "config": settings.config,
            })
        })
        .collect();
    // The plan names the differing properties only; each entry's live
    // record digest binds the live values too.
    let live: Vec<Value> = tags
        .provider_settings
        .iter()
        .map(|p| serde_json::json!({"provider": p.provider, "live_sha256": p.live_sha256}))
        .collect();
    let bytes = serde_json::to_vec(&serde_json::json!({
        "desired": desired,
        "plan": tags.provider_settings,
        "live": live,
    }))
    .unwrap_or_default();
    crate::deploy::tree::sha256_hex(&bytes)
}

/// The apply's rehearsal gate (Phase 26 hardening), before the confirmation
/// and any write: see the module documentation. `rehearsal` is
/// `--rehearsal <ID>`; `ign_version` the running binary's version. Returns
/// the record a given id named. An unchanged plan, and a plan of a project
/// that owns no tag provider, are not gated.
pub fn rehearsal_gate(
    loaded: &LoadedProject,
    input: &ProjectsInput,
    prepared: &ProjectsPrepared,
    rehearsal: Option<&str>,
    ign_version: &str,
) -> Result<Option<RehearsalRecord>, CoreError> {
    let ProjectsPrepared::Ready { plan, .. } = prepared else {
        return Ok(None);
    };
    let Some(tags) = plan.tags.as_ref() else {
        if rehearsal.is_some() {
            return Err(CoreError::InvalidInput {
                reason: "--rehearsal: deploy/project.yaml lists no tag_providers, so no \
                         rehearsal applies"
                    .to_string(),
            });
        }
        return Ok(None);
    };
    let Some(id) = rehearsal else {
        if loaded.target.require_rehearsal {
            return Err(CoreError::RehearsalRequired {
                env: input.env.clone(),
                detail: format!(
                    "{} declares require_rehearsal: true and no --rehearsal <ID> was given",
                    loaded.target_file
                ),
            });
        }
        return Ok(None);
    };
    let id = RunId::parse(id)?;
    let rel = rehearsal_record_rel(&loaded.target.gateway, id.as_str());
    let bytes = match std::fs::read(input.repo.join(&rel)) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(CoreError::RehearsalRequired {
                env: input.env.clone(),
                detail: format!("there is no rehearsal record {rel}"),
            });
        }
        Err(e) => {
            return Err(CoreError::DeployRunInvalid {
                run_id: id.as_str().to_string(),
                problem: format!("the rehearsal record {rel} cannot be read ({:?})", e.kind()),
            });
        }
    };
    let record = RehearsalRecord::parse(&bytes, &rel, id.as_str())?;
    let settings = settings_sha256(loaded, tags);
    // A green verdict must agree with the outcomes the record keeps: the
    // forward verify equal and the rollback green with an equal verify.
    let forward_equal = record.forward.as_ref().is_some_and(|f| f.equal);
    let rollback_green = record
        .rollback
        .as_ref()
        .is_some_and(|r| r.green && r.verify.as_ref().is_some_and(|v| v.equal));
    let outcome = |ok: bool| if ok { "green" } else { "red" };
    let checks: [(&str, &str, &str); 11] = [
        ("env", &record.env, &input.env),
        ("gateway", &record.gateway, &loaded.target.gateway),
        ("project", &record.project, &loaded.file.project),
        ("verdict", &record.verdict, "green"),
        ("tags_sha256", &record.tags_sha256, &tags.tags_sha256),
        (
            "live_tags_sha256",
            &record.live_tags_sha256,
            &tags.live_sha256,
        ),
        ("plan_sha256", &record.plan_sha256, &tags.plan_sha256),
        ("settings_sha256", &record.settings_sha256, &settings),
        ("ign_version", &record.ign_version, ign_version),
        ("forward", outcome(forward_equal), "green"),
        ("rollback", outcome(rollback_green), "green"),
    ];
    for (field, recorded, now) in checks {
        if recorded != now {
            return Err(CoreError::RehearsalStale {
                id: id.as_str().to_string(),
                field: field.to_string(),
                recorded: recorded.to_string(),
                now: now.to_string(),
            });
        }
    }
    Ok(Some(record))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(yaml: &str) -> Target {
        Target::parse("targets/x.yaml", yaml.as_bytes()).expect("a target")
    }

    const PROD: &str = "schema_version: 1\ngateway: plant\nmode: prod\nnodes:\n  master:\n    profile: plant-a\n    gateway_name: plant-a\n  backup:\n    profile: plant-b\n    gateway_name: plant-b\n";

    fn rig(extra: &str) -> String {
        format!(
            "schema_version: 1\ngateway: rig\nmode: dev\nrehearsal: true\nnodes:\n  master:\n    profile: rig\n    gateway_name: rig-gw\n    local:\n      data_dir: /tmp/rig\n{extra}"
        )
    }

    fn reason(result: Result<(), CoreError>) -> String {
        match result {
            Err(CoreError::RehearsalTargetInvalid { reason, .. }) => reason,
            other => panic!("expected rehearsal_target_invalid, got {other:?}"),
        }
    }

    const URLS: &[&str] = &[
        "https://plant-a.example:8043",
        "https://plant-b.example:8043",
    ];

    fn urls() -> Vec<String> {
        URLS.iter().map(|u| (*u).to_string()).collect()
    }

    /// The offline guard accepts a disposable gateway and refuses each way
    /// a rehearsal target can be the production one.
    #[test]
    fn the_guard_refuses_a_production_gateway() {
        let prod = target(PROD);
        let ok = target(&rig(""));
        check_rehearsal_target(
            &prod,
            &urls(),
            "targets/rig.yaml",
            &ok,
            "http://localhost:18290",
        )
        .expect("a disposable gateway");

        let not_declared = target(&rig("").replace("rehearsal: true\n", ""));
        assert!(
            reason(check_rehearsal_target(
                &prod,
                &urls(),
                "f",
                &not_declared,
                "http://localhost:1"
            ))
            .contains("rehearsal: true")
        );
        let no_host = target(&rig("").replace("    local:\n      data_dir: /tmp/rig\n", ""));
        assert!(
            reason(check_rehearsal_target(
                &prod,
                &urls(),
                "f",
                &no_host,
                "http://x"
            ))
            .contains("host access")
        );
        for (yaml, needle) in [
            (
                rig("").replace("gateway: rig\n", "gateway: plant\n"),
                "gateway plant",
            ),
            (rig("").replace("rig-gw", "PLANT-B"), "nodes.backup"),
            (rig("").replace("rig-gw", "plant"), "nodes.master"),
            (
                rig("").replace("profile: rig", "profile: plant-a"),
                "profile plant-a",
            ),
        ] {
            let found = reason(check_rehearsal_target(
                &prod,
                &urls(),
                "f",
                &target(&yaml),
                "http://localhost:1",
            ));
            assert!(found.contains(needle), "{needle}: {found}");
        }
        assert!(
            reason(check_rehearsal_target(
                &prod,
                &urls(),
                "f",
                &ok,
                "HTTPS://plant-b.example:8043/"
            ))
            .contains("URL")
        );
    }

    fn record() -> RehearsalRecord {
        RehearsalRecord {
            schema_version: 1,
            id: "20261111T120000Z-abcdef".into(),
            env: "prod".into(),
            gateway: "plant".into(),
            project: "CapApp".into(),
            release: "v1.0.0".into(),
            rehearsal_env: "rig".into(),
            rehearsal_gateway: "rig".into(),
            tags_sha256: "a".repeat(64),
            live_tags_sha256: "b".repeat(64),
            plan_sha256: "c".repeat(64),
            settings_sha256: "d".repeat(64),
            ign_version: "1.6.0".into(),
            started_at: "2026-11-11T12:00:00Z".into(),
            finished_at: "2026-11-11T12:01:00Z".into(),
            verdict: "green".into(),
            failed_at: None,
            forward: Some(VerifySummary {
                equal: true,
                mismatches: Vec::new(),
                mismatches_count: 0,
            }),
            rollback: Some(RollbackSummary {
                green: true,
                verify: None,
                pruned: vec!["[P]_types_/T/M".into()],
                undo_manual: Vec::new(),
                detail: None,
            }),
        }
    }

    /// The record round-trips, and a record that names another id, holds
    /// an unknown key or another schema version is refused.
    #[test]
    fn the_record_round_trips_strictly() {
        let rec = record();
        let bytes = serde_json::to_vec_pretty(&rec).expect("json");
        assert_eq!(
            RehearsalRecord::parse(&bytes, "r.json", &rec.id).expect("parses"),
            rec
        );
        let wrong = RehearsalRecord::parse(&bytes, "r.json", "20261111T120000Z-000000")
            .expect_err("another id");
        assert_eq!(wrong.code(), "deploy_run_invalid");
        let mut value: Value = serde_json::from_slice(&bytes).expect("json");
        value["extra"] = serde_json::json!(1);
        let extra = RehearsalRecord::parse(
            &serde_json::to_vec(&value).expect("json"),
            "r.json",
            &rec.id,
        )
        .expect_err("unknown key");
        assert_eq!(extra.code(), "deploy_run_invalid");
        let mut old = rec.clone();
        old.schema_version = 2;
        let old =
            RehearsalRecord::parse(&serde_json::to_vec(&old).expect("json"), "r.json", &rec.id)
                .expect_err("schema");
        assert_eq!(old.code(), "deploy_run_invalid");
        assert_eq!(
            rehearsal_record_rel("plant", &rec.id),
            ".ign/deploy/plant/rehearsals/20261111T120000Z-abcdef.json"
        );
    }

    /// A seeded node that differs, is absent, or is extra is named.
    #[test]
    fn seed_mismatches_name_the_nodes() {
        let env: BTreeMap<String, Value> = BTreeMap::from([
            (
                "A".to_string(),
                serde_json::json!({"name":"A","tagType":"AtomicTag","dataType":"Int4","value":1}),
            ),
            (
                "B".to_string(),
                serde_json::json!({"name":"B","tagType":"Folder"}),
            ),
        ]);
        let mut seeded = env.clone();
        // A runtime value is not a difference.
        seeded.get_mut("A").expect("A")["value"] = serde_json::json!(7);
        assert!(seed_mismatches("P", &env, &seeded).is_empty());
        seeded.get_mut("A").expect("A")["dataType"] = serde_json::json!("Float8");
        seeded.remove("B");
        seeded.insert(
            "C".to_string(),
            serde_json::json!({"name":"C","tagType":"Folder"}),
        );
        assert_eq!(
            seed_mismatches("P", &env, &seeded),
            [
                "[P]A (dataType)",
                "[P]B (absent)",
                "[P]C (not in the export)"
            ]
        );
    }

    #[test]
    fn only_a_standard_config_is_rehearsed() {
        assert!(is_standard(
            &serde_json::json!({"profile": {"type": "STANDARD"}, "settings": {}})
        ));
        assert!(!is_standard(
            &serde_json::json!({"profile": {"type": "DATASOURCE"}})
        ));
        assert!(!is_standard(&serde_json::json!({})));
    }

    // The seed waits for the provider it just created: a scripted rig that
    // answers 422 "provider not found" to the first imports after a create.

    type Answer = Box<dyn Fn() -> Result<TagImportAnswer, CoreError> + Send + Sync>;

    /// The catch-all shape of the gateway's 422 on a tag import.
    fn not_found(provider: &str) -> CoreError {
        CoreError::Internal(format!(
            "unexpected HTTP 422 Unprocessable Entity from http://rig.invalid/data/api/v1/tags/import \
             (gateway error page: Error 422: Failed to import tags: java.lang.Exception: provider \
             not found: {provider})"
        ))
    }

    #[derive(Default)]
    struct SeedState {
        providers: BTreeMap<String, TagProviderRecord>,
        attempts: usize,
        since_create: usize,
        log: Vec<String>,
    }

    /// A rehearsal gateway that keeps providers in memory and answers each
    /// import of a freshly created one with "provider not found"
    /// `not_ready` times (always when `None`), then with `then`.
    struct SeedRig {
        state: std::sync::Mutex<SeedState>,
        not_ready: Option<usize>,
        then: Answer,
    }

    impl SeedRig {
        fn new(not_ready: Option<usize>, then: Answer) -> Self {
            Self {
                state: std::sync::Mutex::new(SeedState::default()),
                not_ready,
                then,
            }
        }

        fn attempts(&self) -> usize {
            self.state.lock().expect("state").attempts
        }

        fn log(&self) -> Vec<String> {
            self.state.lock().expect("state").log.clone()
        }
    }

    fn unused<T>() -> Result<T, CoreError> {
        Err(CoreError::Internal(
            "SeedRig: not used by the seed".to_string(),
        ))
    }

    #[async_trait]
    impl ProjectGateway for SeedRig {
        async fn projects(&self) -> Result<Vec<ProjectRecord>, CoreError> {
            unused()
        }
        async fn project_find(&self, _name: &str) -> Result<Option<ProjectRecord>, CoreError> {
            unused()
        }
        async fn export_bytes(&self, _name: &str) -> Result<Vec<u8>, CoreError> {
            unused()
        }
        async fn designers(&self) -> Result<Vec<DesignerSeen>, CoreError> {
            unused()
        }
        async fn gateway_name(&self) -> Result<Option<String>, CoreError> {
            unused()
        }
        async fn redundancy_role(&self) -> Result<Option<String>, CoreError> {
            unused()
        }
        async fn tag_provider_names(&self) -> Result<Vec<String>, CoreError> {
            unused()
        }
        async fn tag_export(&self, _provider: &str) -> Result<Vec<u8>, CoreError> {
            unused()
        }
        async fn project_import(
            &self,
            _name: &str,
            _zip: Vec<u8>,
            _overwrite: bool,
        ) -> Result<ImportOutcome, CoreError> {
            unused()
        }
        async fn tag_import(
            &self,
            provider: &str,
            _path: &str,
            _policy: CollisionPolicy,
            _body: Vec<u8>,
        ) -> Result<TagImportAnswer, CoreError> {
            let refuse = {
                let mut state = self.state.lock().expect("state");
                state.attempts += 1;
                state.since_create += 1;
                state.log.push(format!("import:{provider}"));
                self.not_ready.is_none_or(|n| state.since_create <= n)
            };
            if refuse {
                Err(not_found(provider))
            } else {
                (self.then)()
            }
        }
        async fn tag_provider_record(
            &self,
            name: &str,
        ) -> Result<Option<TagProviderRecord>, CoreError> {
            let mut state = self.state.lock().expect("state");
            state.log.push(format!("record:{name}"));
            Ok(state.providers.get(name).cloned())
        }
        async fn tag_provider_create(&self, body: &[TagProviderCreate]) -> Result<(), CoreError> {
            let mut state = self.state.lock().expect("state");
            for sent in body {
                state.log.push(format!("create:{}", sent.name));
                state.since_create = 0;
                let mut value = serde_json::json!({
                    "name": sent.name,
                    "enabled": sent.enabled,
                    "config": sent.config,
                    "signature": format!("sig-{}", sent.name),
                    "collection": sent.collection,
                });
                if let Some(description) = &sent.description {
                    value["description"] = serde_json::json!(description);
                }
                let record = serde_json::from_value(value).expect("a provider record");
                state.providers.insert(sent.name.clone(), record);
            }
            Ok(())
        }
        async fn tag_provider_modify(&self, _body: &[TagProviderModify]) -> Result<(), CoreError> {
            unused()
        }
        async fn tag_provider_delete(&self, name: &str, _signature: &str) -> Result<(), CoreError> {
            let mut state = self.state.lock().expect("state");
            state.log.push(format!("delete:{name}"));
            state.providers.remove(name);
            Ok(())
        }
    }

    fn clean() -> Answer {
        Box::new(|| Ok(TagImportAnswer::default()))
    }

    fn env_record(config: Value) -> TagProviderRecord {
        serde_json::from_value(serde_json::json!({
            "name": "Edge",
            "enabled": true,
            "config": config,
            "signature": "sig-env",
            "collection": "core",
        }))
        .expect("a provider record")
    }

    fn standard_config() -> Value {
        serde_json::json!({"profile": {"type": "STANDARD"}, "settings": {}})
    }

    const EMPTY_EXPORT: &[u8] = br#"{"tags":[]}"#;

    fn quick() -> ProviderWait {
        ProviderWait {
            timeout: Duration::from_secs(2),
            first_pause: Duration::from_millis(1),
        }
    }

    fn position(log: &[String], entry: &str) -> usize {
        log.iter()
            .position(|e| e == entry)
            .unwrap_or_else(|| panic!("{entry} not in {log:?}"))
    }

    /// The with-settings create: the first two imports are refused, the
    /// third lands, and the seed is green with the settings rehearsed.
    #[tokio::test]
    async fn the_seed_waits_for_a_provider_created_with_its_settings() {
        let rig = SeedRig::new(Some(2), clean());
        let seeded = seed_provider(
            &rig,
            "Edge",
            &env_record(standard_config()),
            EMPTY_EXPORT,
            quick(),
        )
        .await
        .expect("a green seed");
        assert_eq!(seeded.report.settings, "rehearsed");
        assert_eq!(rig.attempts(), 3);
        let log = rig.log();
        assert!(
            position(&log, "create:Edge") < position(&log, "import:Edge"),
            "{log:?}"
        );
    }

    /// The STANDARD fallback create (a provider that is not STANDARD) waits
    /// the same way.
    #[tokio::test]
    async fn the_seed_waits_for_a_provider_created_standard() {
        let rig = SeedRig::new(Some(2), clean());
        let record = env_record(serde_json::json!({"profile": {"type": "DATASOURCE"}}));
        let seeded = seed_provider(&rig, "Edge", &record, EMPTY_EXPORT, quick())
            .await
            .expect("a green seed");
        assert_eq!(seeded.report.settings, "not_rehearsed");
        assert_eq!(rig.attempts(), 3);
        let log = rig.log();
        assert!(
            position(&log, "create:Edge") < position(&log, "import:Edge"),
            "{log:?}"
        );
    }

    /// A provider that never becomes ready fails within the bound, naming
    /// the provider.
    #[tokio::test]
    async fn a_provider_that_never_starts_fails_the_seed_within_the_bound() {
        let rig = SeedRig::new(None, clean());
        let wait = ProviderWait {
            timeout: Duration::from_millis(200),
            first_pause: Duration::from_millis(5),
        };
        let started = std::time::Instant::now();
        let err = seed_provider(
            &rig,
            "Edge",
            &env_record(standard_config()),
            EMPTY_EXPORT,
            wait,
        )
        .await
        .err()
        .expect("a failed seed");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(err.code(), "internal");
        let message = err.to_string();
        assert!(message.contains("Edge"), "{message}");
        assert!(message.contains("not ready"), "{message}");
        assert!(message.contains("provider not found"), "{message}");
        assert!(rig.attempts() >= 2, "{}", rig.attempts());
    }

    /// Nothing but the provider-not-found answer is retried: one attempt.
    #[tokio::test]
    async fn no_other_import_failure_is_retried() {
        let cases: Vec<(&str, Answer)> = vec![
            (
                "another status",
                Box::new(|| {
                    Err(CoreError::Internal(
                        "unexpected HTTP 500 Internal Server Error from http://rig.invalid/data/api/v1/tags/import"
                            .to_string(),
                    ))
                }),
            ),
            ("another provider", Box::new(|| Err(not_found("Edge2")))),
            (
                "an auth refusal",
                Box::new(|| {
                    Err(CoreError::Auth {
                        status: 401,
                        endpoint: None,
                    })
                }),
            ),
            (
                "an answer with failures",
                Box::new(|| {
                    Ok(TagImportAnswer {
                        failure_count: Some(1),
                        ..TagImportAnswer::default()
                    })
                }),
            ),
        ];
        for (what, first) in cases {
            let expected = first().err().map(|e| e.code());
            let rig = SeedRig::new(Some(0), first);
            let err = seed_provider(
                &rig,
                "Edge",
                &env_record(standard_config()),
                EMPTY_EXPORT,
                quick(),
            )
            .await
            .err()
            .unwrap_or_else(|| panic!("{what}: a failed seed"));
            assert_eq!(rig.attempts(), 1, "{what}");
            match expected {
                Some(code) => assert_eq!(err.code(), code, "{what}"),
                None => assert!(
                    err.to_string().contains("did not take the seed"),
                    "{what}: {err}"
                ),
            }
        }
    }

    #[test]
    fn provider_starting_matches_only_the_named_provider() {
        assert!(provider_starting(&not_found("Edge"), "Edge"));
        // Another provider, and one the seeded name is a prefix of.
        assert!(!provider_starting(&not_found("Edge2"), "Edge"));
        assert!(!provider_starting(&not_found("Edge"), "Edge2"));
        assert!(!provider_starting(
            &CoreError::Internal(
                "unexpected HTTP 500 Internal Server Error from http://rig.invalid/x (gateway error page: provider not found: Edge)"
                    .to_string()
            ),
            "Edge"
        ));
        assert!(!provider_starting(
            &CoreError::Auth {
                status: 401,
                endpoint: None
            },
            "Edge"
        ));
    }
}
