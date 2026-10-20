//! Deploy actions: `ign deploy render` (Phase 21) and the `deploy plan`
//! engine entry point (Phase 22).
//!
//! An OFFLINE verb: it reads a declared repo from disk and returns a model.
//! There is no `Session`, no profile resolution, no network connection and
//! no credential read; the envelope's `profile` is null. Render reads the
//! repo and writes nothing, except into the optional `--out` directory, and
//! only after the whole render succeeded: a refused render leaves `--out`
//! untouched. The write goes through [`write_out`]'s clobber ladder, which
//! never deletes and only replaces its own unmodified prior output.
//!
//! The `data` keys, in order: `env, gateway, mode, out, files, bytes,
//! render_sha256, resources, tag_providers, uuids_minted, opaque_files,
//! warnings`. Later plans add keys additively (the `workspace.rs`
//! discipline); `out` is always present and is null without `--out`.
//!
//! Errors never echo file contents: findings name repo-relative paths and
//! key names only.
//!
//! ## `deploy plan` (22-01)
//!
//! [`load_declared`] is the shared front half of every verb that starts from
//! a declared repo (render, plan, later pull and verify): it reads
//! `targets/<env>.yaml`, the gateway manifest and the source tree and
//! renders them. [`deploy_plan`] then compares that render with a node's
//! live files, read through a [`TreeReader`]. It opens no gateway session,
//! sends no REST request, takes no scan-lock and writes nothing: the reader
//! has no write half to call.
//!
//! The [`DeployPlanResult`] `data` keys, in order: `env, gateway, mode,
//! node, transport, render_sha256, plan_sha256, summary, entries,
//! delete_set, base, warnings`. Later plans add keys additively.
//! Entries carry paths and sha256 only, never file bytes.
//!
//! ## `deploy pull` (22-08, DPLN-06)
//!
//! [`deploy_pull`] bootstraps `gateways/<name>/` from the live master through
//! the same read-only [`TreeReader`]: it reads, builds a tree that renders
//! back to the live effective view ([`crate::deploy::pull`]), and only then
//! writes the local repo. It never writes to the gateway host or the gateway
//! and resolves no session. Replacing a non-empty `resources/`, `tags/` or
//! `modes/<env>/` needs `yes` ([`deploy_pull_preview`] names what would be
//! replaced); `manifest.yaml` and other environments' overlays are never
//! touched. On unix every removal and write goes through descriptor-relative
//! calls rooted at the repo (`tree::dirfd`): a directory swapped for a
//! symlink mid-run is refused, never followed.
//!
//! The [`DeployPullResult`] `data` keys, in order: `env, gateway, mode, node,
//! transport, out, files_written, manifest, render_sha256, skipped,
//! warnings`.
//!
//! ## `deploy verify` (22-10, DPLN-07, DPLN-08, DPLN-09)
//!
//! [`deploy_verify`] gathers read-only evidence from the master gateway and
//! judges it with the pure [`verify::evaluate`] (six checks, fixed order; see
//! [`crate::deploy::verify`] for the rules). It takes a [`DeployApi`] for the
//! REST resource, scan and mode reads, a [`GatewayApi`] for the log and the
//! WebDev tag probes, and a [`TreeReader`] for the live files. It takes no
//! scan-lock, triggers no scan and writes nothing.
//!
//! A red verdict is a normal result: `ok: false`, `slug:
//! deploy_verify_failed`, and [`DeployVerifyResult::failure_exit_code`]
//! returns `Some(6)` for the binary to apply after rendering (Decision A).
//! An unreachable gateway, a refused resource, scan or mode read, and a
//! transport failure are [`CoreError`]s instead, because there is nothing to
//! judge. No log text, tag value or file byte reaches the result.
//!
//! The [`DeployVerifyResult`] `data` keys, in order: `env, gateway, mode,
//! node, project, ok, slug, checks, resources_checked, warnings`.
//!
//! ## `deploy verify --node` (24-09, DPAIR-01, DPAIR-03, D-12)
//!
//! [`deploy_verify_node`] adds the backup of a redundant pair. `master` is
//! [`deploy_verify`] unchanged; `backup` runs the backup checks only and
//! `both` runs the master's six and then the backup's. The result then also
//! carries `backup` (the [`crate::deploy::pair::BackupReport`] shape, with
//! `budget_s` 0, `waited_s` 0 and `resync` null: verify never waits, never
//! resyncs and never writes). Both keys are absent for `master`, so its
//! output is byte-identical to the verb before Phase 24.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::client::logs::{DEFAULT_LOG_LIMIT, LogQuery};
use crate::client::mode::normalize_deployment_mode;
use crate::client::query::ListQuery;
use crate::client::scan::{ScanKind, ScanStatus};
use crate::client::{DeployApi, GatewayApi};
use crate::deploy::apply::journal::{journal_on_box, journal_present};
use crate::deploy::component_problem;
use crate::deploy::ledger::{Ledger, LedgerRoot, ledger_location};
use crate::deploy::manifest::Manifest;
use crate::deploy::pair::{
    BackupCheck, BackupCheckId, BackupNode, BackupReport, BackupSignals, SecretNamesReport,
    TickOutcome, compare_files, effective_files, files_check, files_skipped_check,
    mode_backup_check, read_back_backup_check, unreachable_check, verify_report,
};
use crate::deploy::plan::{BaseTree, PlanEntry, PlanSummary, apply_base, plan, render_diffs};
use crate::deploy::pull::{PullInput, Skipped, generate_manifest, pull};
use crate::deploy::render::{RenderInput, RenderOutcome, RenderedResource, render};
use crate::deploy::target::{NodeTarget, Target};
use crate::deploy::transport::{CORE_COLLECTION, DeleteSet, READ_MAX_ENTRIES, Scope, TreeReader};
use crate::deploy::tree::{
    RenderMarker, SourceTree, read_repo_file, require_repo_dir, walk_capped, write_out,
};
use crate::deploy::verify;
use crate::error::CoreError;

/// The `ign deploy render` result (the envelope's `data`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeployRenderResult {
    /// The `--env` value.
    pub env: String,
    /// Gateway directory name from the target.
    pub gateway: String,
    /// Deployment mode from the target.
    pub mode: String,
    /// The `--out` directory as given; null without `--out`.
    pub out: Option<String>,
    /// Number of rendered files.
    pub files: usize,
    /// Total rendered bytes.
    pub bytes: usize,
    /// Lowercase hex SHA-256 of the rendered listing (README documents it).
    pub render_sha256: String,
    /// Rendered resources, byte-ordered, singletons first.
    pub resources: Vec<RenderedResource>,
    /// Tag providers rendered, byte-ordered.
    pub tag_providers: Vec<String>,
    /// Collection paths that received a minted uuid, byte-ordered.
    pub uuids_minted: Vec<String>,
    /// Collection paths passed through as opaque bytes, byte-ordered.
    pub opaque_files: Vec<String>,
    /// Warnings in pipeline order.
    pub warnings: Vec<String>,
}

/// Read and parse `targets/<env>.yaml`: the target file's repo-relative
/// name and the parsed target.
pub fn load_target(repo: &Path, env: &str) -> Result<(String, Target), CoreError> {
    if let Some(problem) = component_problem(env) {
        return Err(CoreError::InvalidInput {
            reason: format!("--env {env}: {problem}"),
        });
    }
    let target_rel = format!("targets/{env}.yaml");
    let target_bytes = read_repo_file(
        repo,
        &target_rel,
        Some(&format!("--env {env} selects {target_rel}")),
    )?;
    let target = Target::parse(&target_rel, &target_bytes)?;
    Ok((target_rel, target))
}

/// A declared repo, loaded and rendered for one environment.
#[derive(Debug, Clone)]
pub struct Declared {
    /// The `--env` value.
    pub env: String,
    /// Repo-relative path of the target file (`targets/<env>.yaml`).
    pub target_file: String,
    /// The parsed target.
    pub target: Target,
    /// The parsed gateway manifest.
    pub manifest: Manifest,
    /// The render of the declared tree.
    pub outcome: RenderOutcome,
}

/// Load the target, the manifest and the source tree of `env` under `repo`
/// and render them. Reads the repo and nothing else.
pub fn load_declared(repo: &Path, env: &str) -> Result<Declared, CoreError> {
    let (target_file, target) = load_target(repo, env)?;

    let gateway_rel = format!("gateways/{}", target.gateway);
    let gateway_dir = repo.join("gateways").join(&target.gateway);
    require_repo_dir(repo, "gateways")?;
    require_repo_dir(repo, &gateway_rel)?;

    let manifest_rel = format!("{gateway_rel}/manifest.yaml");
    let manifest_bytes = read_repo_file(repo, &manifest_rel, None)?;
    let manifest = Manifest::parse(&manifest_rel, &manifest_bytes)?;

    let source = SourceTree::read_gateway(&gateway_dir, env)?;
    let outcome = render(&RenderInput {
        gateway: &target.gateway,
        env,
        manifest: &manifest,
        target: &target,
        source: &source,
    })?;
    Ok(Declared {
        env: env.to_string(),
        target_file,
        target,
        manifest,
        outcome,
    })
}

/// Render the declared tree of `env` under `repo`, optionally writing it
/// (and `render.json`) to `out`.
pub fn deploy_render(
    repo: &Path,
    env: &str,
    out: Option<&Path>,
) -> Result<DeployRenderResult, CoreError> {
    let Declared {
        target, outcome, ..
    } = load_declared(repo, env)?;

    // Only a successful render reaches the filesystem.
    if let Some(out) = out {
        let marker = RenderMarker::for_tree(&target.gateway, env, &target.mode, &outcome.tree);
        write_out(&outcome.tree, out, &marker)?;
    }

    Ok(DeployRenderResult {
        env: env.to_string(),
        gateway: target.gateway.clone(),
        mode: target.mode.clone(),
        out: out.map(|p| p.to_string_lossy().into_owned()),
        files: outcome.tree.files.len(),
        bytes: outcome.tree.bytes(),
        render_sha256: outcome.tree.render_sha256(),
        resources: outcome.resources,
        tag_providers: outcome.tag_providers,
        uuids_minted: outcome.uuids_minted,
        opaque_files: outcome.opaque_files,
        warnings: outcome.warnings,
    })
}

/// The `ign deploy plan` result (the envelope's `data`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeployPlanResult {
    /// The `--env` value.
    pub env: String,
    /// Gateway directory name from the target.
    pub gateway: String,
    /// Deployment mode from the target.
    pub mode: String,
    /// The node that was read: always `master` in this phase.
    pub node: String,
    /// How the node's host was read: `local` or `ssh`.
    pub transport: String,
    /// Lowercase hex SHA-256 of the rendered listing.
    pub render_sha256: String,
    /// Lowercase hex SHA-256 of the canonical plan text (D-08).
    pub plan_sha256: String,
    /// Counts by status; `has_changes` ignores `unmanaged`.
    pub summary: PlanSummary,
    /// One entry per unit, in unit-key byte order.
    pub entries: Vec<PlanEntry>,
    /// The files plan proposes to delete: the declared-names ledger's files
    /// minus the rendered files (22-03). Empty without a ledger.
    pub delete_set: DeleteSet,
    /// The `--base` render the drift was computed against; null without it.
    pub base: Option<BaseSummary>,
    /// Render warnings, then plan warnings.
    pub warnings: Vec<String>,
}

/// Options of `deploy plan`.
#[derive(Debug, Clone, Default)]
pub struct PlanOptions {
    /// A directory written by `deploy render --out`, re-hashed and used as
    /// the base of the three-way drift check.
    pub base: Option<PathBuf>,
    /// Add redacted unified text diffs to the entries.
    pub diff: bool,
}

/// What `--base` was: the directory and its recomputed digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BaseSummary {
    /// The `--base` directory as given.
    pub dir: String,
    /// The base's recomputed `render_sha256`.
    pub render_sha256: String,
    /// Number of files in the base (`render.json` excluded).
    pub files: usize,
}

/// Plan `declared` against the live files `reader` returns for the master
/// node. Reads once; never writes, never opens a gateway session, never
/// prints.
pub async fn deploy_plan(
    declared: &Declared,
    reader: &dyn TreeReader,
    opts: &PlanOptions,
) -> Result<DeployPlanResult, CoreError> {
    let node = &declared.target.nodes.master;
    let transport = match (&node.ssh, &node.local) {
        (Some(_), _) => "ssh",
        (None, Some(_)) => "local",
        (None, None) => "none",
    };
    // A base that is not exactly a clean render is refused before any
    // transport work (22-04).
    let base = match &opts.base {
        Some(dir) => {
            let dir = dir.clone();
            let (gateway, env, mode) = (
                declared.target.gateway.clone(),
                declared.env.clone(),
                declared.target.mode.clone(),
            );
            let read =
                tokio::task::spawn_blocking(move || BaseTree::read(&dir, &gateway, &env, &mode))
                    .await
                    .map_err(|e| CoreError::Internal(format!("--base read task failed: {e}")))?;
            Some(read?)
        }
        None => None,
    };
    // An interrupted apply left its journal: the tree may be torn, so nothing
    // is read or planned until `rollback --run` recovers it (DAPL-09, D-09).
    // An unreadable journal blocks the same way. Checked after the base (a
    // bad one costs no transport work) and before the collections are read.
    if let Some(journal) = journal_on_box(
        reader,
        node,
        &declared.target.gateway,
        &declared.target.mode,
    )
    .await?
    {
        return Err(journal_present(
            &journal,
            &declared.env,
            node,
            &declared.target.gateway,
            &declared.target.mode,
        ));
    }
    let scope = Scope::from_manifest(&declared.manifest, &declared.target.mode);
    let observed = reader.read(&scope).await?;
    let gateway = &declared.target.gateway;
    let mode = &declared.target.mode;
    // An absent ledger is a first deploy (empty delete set); one that exists
    // but cannot be trusted is a refusal, never an empty delete set (D-07).
    let ledger = match reader.read_ledger(gateway, mode).await? {
        Some(bytes) => Some(Ledger::parse(
            &ledger_location(LedgerRoot::for_node(node), gateway, mode),
            &bytes,
            gateway,
            mode,
        )?),
        None => None,
    };
    let mut planned = plan(&declared.outcome.tree, &observed, ledger.as_ref(), &scope);
    if let Some(base) = &base {
        apply_base(&mut planned, base);
    }
    if opts.diff {
        render_diffs(&mut planned, &declared.outcome.tree, &observed);
    }

    let mut warnings = declared.outcome.warnings.clone();
    warnings.extend(planned.warnings);
    Ok(DeployPlanResult {
        env: declared.env.clone(),
        gateway: declared.target.gateway.clone(),
        mode: declared.target.mode.clone(),
        node: "master".to_string(),
        transport: transport.to_string(),
        render_sha256: declared.outcome.tree.render_sha256(),
        plan_sha256: planned.plan_sha256,
        summary: planned.summary,
        entries: planned.entries,
        delete_set: planned.delete_set,
        base: base.map(|b| BaseSummary {
            dir: b.dir,
            render_sha256: b.render_sha256,
            files: b.files.len(),
        }),
        warnings,
    })
}

/// The `ign deploy pull` result (the envelope's `data`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeployPullResult {
    /// The `--env` value.
    pub env: String,
    /// Gateway directory name from the target.
    pub gateway: String,
    /// Deployment mode from the target; its collection is the overlay.
    pub mode: String,
    /// The node that was read: always `master` in this phase.
    pub node: String,
    /// How the node's host was read: `local` or `ssh`.
    pub transport: String,
    /// The directory written, repo-relative: `gateways/<name>`.
    pub out: String,
    /// Source files written under `resources/`, `tags/` and `modes/<env>/`
    /// (the manifest is not counted).
    pub files_written: usize,
    /// `kept` when `manifest.yaml` already existed, `generated` otherwise.
    pub manifest: String,
    /// Lowercase hex SHA-256 of what `deploy render` produces from the
    /// pulled tree.
    pub render_sha256: String,
    /// Every live unit (or type) not written, byte-sorted by path, each with
    /// a reason. Never a value.
    pub skipped: Vec<Skipped>,
    /// Render warnings of the pulled tree, then pull warnings.
    pub warnings: Vec<String>,
}

/// What an existing `gateways/<name>/` holds in the three subtrees a pull
/// replaces. Gateway-relative paths, files in `resources`, `tags`,
/// `modes/<env>` order and byte order within each.
struct Existing {
    files: Vec<String>,
    /// Every directory below and including each subtree root.
    dirs: BTreeSet<String>,
}

/// Whether `repo/rel` exists, without following a symlink.
fn repo_entry_exists(repo: &Path, rel: &str) -> Result<bool, CoreError> {
    match std::fs::symlink_metadata(repo.join(rel)) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(CoreError::deploy_tree_invalid(
            rel,
            None,
            None,
            format!("cannot inspect ({})", e.kind()),
        )),
    }
}

/// Inspect the subtrees `deploy pull` would replace. A symlink at any level
/// of them, or anywhere under them, is `deploy_tree_invalid` and nothing is
/// changed. `None` when `gateways/<gateway>/` does not exist.
fn scan_existing(repo: &Path, gateway: &str, env: &str) -> Result<Option<Existing>, CoreError> {
    let gateway_rel = format!("gateways/{gateway}");
    for rel in ["gateways", gateway_rel.as_str()] {
        if !repo_entry_exists(repo, rel)? {
            return Ok(None);
        }
        require_repo_dir(repo, rel)?;
    }
    let mut existing = Existing {
        files: Vec::new(),
        dirs: BTreeSet::new(),
    };
    for sub in [
        "resources".to_string(),
        "tags".to_string(),
        format!("modes/{env}"),
    ] {
        if sub.starts_with("modes/") {
            let modes = format!("{gateway_rel}/modes");
            if !repo_entry_exists(repo, &modes)? {
                continue;
            }
            require_repo_dir(repo, &modes)?;
        }
        let rel = format!("{gateway_rel}/{sub}");
        if !repo_entry_exists(repo, &rel)? {
            continue;
        }
        require_repo_dir(repo, &rel)?;
        let mut files: BTreeMap<String, PathBuf> = BTreeMap::new();
        let mut dirs: BTreeSet<String> = BTreeSet::new();
        walk_capped(
            &repo.join(&rel),
            &sub,
            &mut files,
            &mut dirs,
            READ_MAX_ENTRIES,
        )
        .map_err(|why| CoreError::deploy_tree_invalid(&rel, None, None, why))?;
        existing.dirs.insert(sub.clone());
        existing.dirs.extend(dirs);
        existing.files.extend(files.into_keys());
    }
    Ok(Some(existing))
}

/// The refusal prose of a pull that would replace `existing`.
fn pull_prose(gateway: &str, env: &str, existing: &Existing) -> String {
    let shown: Vec<&str> = existing.files.iter().take(5).map(String::as_str).collect();
    let more = if existing.files.len() > shown.len() {
        ", ..."
    } else {
        ""
    };
    format!(
        "deploy pull would replace gateways/{gateway}/resources, tags and modes/{env} with the live tree of nodes.master ({} existing files, e.g. {}{more})",
        existing.files.len(),
        shown.join(", ")
    )
}

/// What `deploy pull` would replace: `Some(prose)` when the gateway
/// directory holds files in `resources/`, `tags/` or `modes/<env>/` (the
/// text the `--yes` guard refuses with), `None` when it is absent or holds
/// nothing there. Reads the local repo only: no gateway is contacted.
pub fn deploy_pull_preview(repo: &Path, env: &str) -> Result<Option<String>, CoreError> {
    let (_, target) = load_target(repo, env)?;
    Ok(scan_existing(repo, &target.gateway, env)?
        .filter(|existing| !existing.files.is_empty())
        .map(|existing| pull_prose(&target.gateway, env, &existing)))
}

fn transport_label(node: &NodeTarget) -> &'static str {
    match (&node.ssh, &node.local) {
        (Some(_), _) => "ssh",
        (None, Some(_)) => "local",
        (None, None) => "none",
    }
}

/// The tag provider names under `ignition/tag-provider/` in `observed`.
fn live_providers(observed: &crate::deploy::transport::ObservedFiles) -> Vec<String> {
    let names: BTreeSet<String> = observed
        .core
        .keys()
        .chain(observed.mode.keys())
        .filter_map(|path| {
            let parts: Vec<&str> = path.components().collect();
            // a provider is a directory: `ignition/tag-provider/<name>/<file>`
            (parts.len() >= 4 && parts[0] == "ignition" && parts[1] == "tag-provider")
                .then(|| parts[2].to_string())
        })
        .collect();
    names.into_iter().collect()
}

/// Bootstrap `gateways/<gateway>/` from the live master. See the module doc.
///
/// `yes` confirms replacing existing `resources/`, `tags/` and `modes/<env>/`
/// files; without it a non-empty tree is refused as `confirmation_required`
/// before the gateway is read and before any write.
pub async fn deploy_pull(
    repo: &Path,
    env: &str,
    reader: &dyn TreeReader,
    yes: bool,
) -> Result<DeployPullResult, CoreError> {
    let (_, target) = load_target(repo, env)?;
    let gateway = target.gateway.clone();
    let existing = scan_existing(repo, &gateway, env)?;
    if let Some(existing) = existing.as_ref().filter(|e| !e.files.is_empty())
        && !yes
    {
        return Err(CoreError::ConfirmationRequired {
            operation: pull_prose(&gateway, env, existing),
        });
    }

    // The manifest: an existing one is used and kept (D-10); otherwise one is
    // generated from the live types and tag providers.
    let manifest_rel = format!("gateways/{gateway}/manifest.yaml");
    let mut left_out: Vec<Skipped> = Vec::new();
    let (manifest, generated) = if repo_entry_exists(repo, &manifest_rel)? {
        let bytes = read_repo_file(repo, &manifest_rel, None)?;
        (Manifest::parse(&manifest_rel, &bytes)?, None)
    } else {
        let mut live_types = reader.list_types(CORE_COLLECTION).await?;
        live_types.extend(reader.list_types(&target.mode).await?);
        let discovery = Scope {
            mode: target.mode.clone(),
            types: vec![("ignition".to_string(), "tag-provider".to_string())],
            tag_providers: Vec::new(),
        };
        let providers = live_providers(&reader.read(&discovery).await?);
        let (yaml, skipped) = generate_manifest(&live_types, &providers);
        left_out = skipped;
        let manifest = Manifest::parse(&manifest_rel, yaml.as_bytes())?;
        (manifest, Some(yaml))
    };

    let scope = Scope::from_manifest(&manifest, &target.mode);
    let observed = reader.read(&scope).await?;
    let outcome = pull(&PullInput {
        gateway: &gateway,
        env,
        target: &target,
        manifest: &manifest,
        observed: &observed,
    })?;

    write_pulled(
        repo,
        &gateway,
        existing.as_ref(),
        &outcome.source,
        generated.as_deref(),
    )?;

    left_out.extend(outcome.skipped);
    left_out.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    let mut warnings = outcome.rendered.warnings.clone();
    warnings.extend(observed.local_units.iter().map(|unit| {
        format!(
            "{unit} also exists in the `local` collection, which the gateway loads over the mode collection; pull did not read the `local` copy"
        )
    }));
    Ok(DeployPullResult {
        env: env.to_string(),
        gateway: gateway.clone(),
        mode: target.mode.clone(),
        node: "master".to_string(),
        transport: transport_label(&target.nodes.master).to_string(),
        out: format!("gateways/{gateway}"),
        files_written: outcome.source.files.len(),
        manifest: if generated.is_some() {
            "generated"
        } else {
            "kept"
        }
        .to_string(),
        render_sha256: outcome.rendered.tree.render_sha256(),
        skipped: left_out,
        warnings,
    })
}

/// A name that reaches the repo from the gateway is checked once more here:
/// render already refused unsafe names, so this only fails closed.
fn safe_component(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', '\0'])
        && !name.chars().any(char::is_control)
}

/// Refuse a pulled tree that cannot be written whole. This runs before the
/// first delete, so a pull that would fail part-way fails with the existing
/// tree untouched (CR-01): an unsafe path component, two paths that collide
/// when case is folded (a case-insensitive filesystem keeps only one), and a
/// path that is both a file and a directory.
fn validate_pulled(gateway: &str, source: &SourceTree) -> Result<(), CoreError> {
    let refuse = |path: &str, why: String| {
        CoreError::deploy_tree_invalid(format!("gateways/{gateway}/{path}"), None, None, why)
    };
    let mut folded: BTreeMap<String, &str> = BTreeMap::new();
    let mut dir_folds: BTreeSet<String> = BTreeSet::new();
    for path in source.files.keys() {
        let parts: Vec<&str> = path.components().collect();
        if parts.iter().any(|c| !safe_component(c)) {
            return Err(refuse(
                path.as_str(),
                "has an unsafe path component; refused before anything was changed".to_string(),
            ));
        }
        let fold = path.as_str().to_lowercase();
        if let Some(other) = folded.insert(fold.clone(), path.as_str()) {
            return Err(refuse(
                path.as_str(),
                format!(
                    "collides with {other} when case is folded; a case-insensitive filesystem keeps only one. Refused before anything was changed"
                ),
            ));
        }
        for end in fold.match_indices('/').map(|(i, _)| i) {
            dir_folds.insert(fold[..end].to_string());
        }
    }
    for (fold, path) in &folded {
        if dir_folds.contains(fold) {
            return Err(refuse(
                path,
                "is both a file and a directory in the pulled tree; refused before anything was changed"
                    .to_string(),
            ));
        }
    }
    Ok(())
}

/// Validate the pulled tree, then replace what a pull replaces and write it
/// (see [`write_pulled_fs`]). The validation comes first so no destructive
/// step runs for a tree that is known to be unwritable.
fn write_pulled(
    repo: &Path,
    gateway: &str,
    existing: Option<&Existing>,
    source: &SourceTree,
    manifest: Option<&str>,
) -> Result<(), CoreError> {
    validate_pulled(gateway, source)?;
    write_pulled_fs(repo, gateway, existing, source, manifest)
}

/// Remove what a pull replaces, then write the pulled tree and, when one was
/// generated, the manifest. Unix: every step is descriptor-relative and
/// rooted at the repo, so no component can be redirected by a symlink.
#[cfg(unix)]
fn write_pulled_fs(
    repo: &Path,
    gateway: &str,
    existing: Option<&Existing>,
    source: &SourceTree,
    manifest: Option<&str>,
) -> Result<(), CoreError> {
    use crate::deploy::tree::dirfd::{self, FdError, Kind};
    use std::io::Write;

    let fail = |what: &str, e: FdError| match e {
        FdError::Changed => CoreError::deploy_tree_invalid(
            what,
            None,
            None,
            "changed while it was written (a symlink or a non-directory appeared); refused, nothing outside the gateway directory was touched",
        ),
        FdError::Io(io) => CoreError::Internal(format!("cannot write {what}: {io}")),
        other => CoreError::Internal(format!("cannot write {what}: {other:?}")),
    };

    // The repo root is the operator's own and may legitimately be reached
    // through a symlink (`--repo link`), so it is resolved once here; every
    // component below it is then opened without following symlinks.
    let repo_real = std::fs::canonicalize(repo).map_err(|e| {
        CoreError::Internal(format!("cannot resolve the repo {}: {e}", repo.display()))
    })?;
    let root = dirfd::open_root(&repo_real).map_err(|e| fail("the repo", e))?;
    let gateways = dirfd::ensure_dir(&root, "gateways").map_err(|e| fail("gateways", e))?;
    let gw_label = format!("gateways/{gateway}");
    let gw = dirfd::ensure_dir(&gateways, gateway).map_err(|e| fail(&gw_label, e))?;

    if let Some(existing) = existing {
        for file in &existing.files {
            let parts: Vec<&str> = file.split('/').collect();
            let (name, parents) = parts.split_last().ok_or_else(|| {
                CoreError::Internal("an existing file has an empty path".to_string())
            })?;
            let label = format!("{gw_label}/{file}");
            let dir = dirfd::walk(&gw, parents).map_err(|e| fail(&label, e))?;
            match dirfd::kind_at(&dir, name).map_err(|e| fail(&label, e))? {
                Kind::File => dirfd::unlink_at(&dir, name).map_err(|e| fail(&label, e))?,
                Kind::Missing => {}
                _ => return Err(fail(&label, FdError::Changed)),
            }
        }
        // children before parents: a child path sorts after its parent
        for dir_path in existing.dirs.iter().rev() {
            let parts: Vec<&str> = dir_path.split('/').collect();
            let Some((name, parents)) = parts.split_last() else {
                continue;
            };
            let label = format!("{gw_label}/{dir_path}");
            let parent = dirfd::walk(&gw, parents).map_err(|e| fail(&label, e))?;
            match dirfd::rmdir_at(&parent, name) {
                Ok(()) | Err(FdError::Missing) => {}
                Err(FdError::NotEmpty) => {
                    return Err(CoreError::deploy_tree_invalid(
                        label,
                        None,
                        None,
                        "is not empty after the pull removed its files (something wrote into it); nothing more was changed",
                    ));
                }
                Err(e) => return Err(fail(&label, e)),
            }
        }
    }

    if let Some(text) = manifest {
        let label = format!("{gw_label}/manifest.yaml");
        let mut file = dirfd::create_new(&gw, "manifest.yaml").map_err(|e| fail(&label, e))?;
        file.write_all(text.as_bytes())
            .map_err(|e| CoreError::Internal(format!("cannot write {label}: {e}")))?;
    }
    for (path, bytes) in &source.files {
        let label = format!("{gw_label}/{path}");
        let parts: Vec<&str> = path.components().collect();
        if parts.iter().any(|c| !safe_component(c)) {
            return Err(CoreError::deploy_tree_invalid(
                label,
                None,
                None,
                "has an unsafe path component; refused",
            ));
        }
        let (name, parents) = parts
            .split_last()
            .ok_or_else(|| CoreError::Internal("a pulled file has an empty path".to_string()))?;
        let dir = dirfd::ensure_walk(&gw, parents).map_err(|e| fail(&label, e))?;
        let mut file = dirfd::create_new(&dir, name).map_err(|e| fail(&label, e))?;
        file.write_all(bytes)
            .map_err(|e| CoreError::Internal(format!("cannot write {label}: {e}")))?;
    }
    Ok(())
}

/// Non-unix: std has no descriptor-relative primitives, so the replaced
/// subtrees were checked for symlinks by [`scan_existing`] just before and
/// the operations are path-based (the 22-01 residual for Windows).
#[cfg(not(unix))]
fn write_pulled_fs(
    repo: &Path,
    gateway: &str,
    existing: Option<&Existing>,
    source: &SourceTree,
    manifest: Option<&str>,
) -> Result<(), CoreError> {
    use std::io::Write;

    let io = |what: &Path, e: std::io::Error| {
        CoreError::Internal(format!("cannot write {}: {e}", what.display()))
    };
    let gw = repo.join("gateways").join(gateway);
    std::fs::create_dir_all(&gw).map_err(|e| io(&gw, e))?;
    if let Some(existing) = existing {
        for file in &existing.files {
            let path = file.split('/').fold(gw.clone(), |acc, c| acc.join(c));
            std::fs::remove_file(&path).map_err(|e| io(&path, e))?;
        }
        for dir_path in existing.dirs.iter().rev() {
            let path = dir_path.split('/').fold(gw.clone(), |acc, c| acc.join(c));
            std::fs::remove_dir(&path).map_err(|e| io(&path, e))?;
        }
    }
    let create = |path: &Path, bytes: &[u8]| -> Result<(), CoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| io(path, e))?;
        file.write_all(bytes).map_err(|e| io(path, e))
    };
    if let Some(text) = manifest {
        create(&gw.join("manifest.yaml"), text.as_bytes())?;
    }
    for (path, bytes) in &source.files {
        let parts: Vec<&str> = path.components().collect();
        if parts.iter().any(|c| !safe_component(c)) {
            return Err(CoreError::deploy_tree_invalid(
                format!("gateways/{gateway}/{path}"),
                None,
                None,
                "has an unsafe path component; refused",
            ));
        }
        let dest = parts.iter().fold(gw.clone(), |acc, c| acc.join(c));
        create(&dest, bytes)?;
    }
    Ok(())
}

/// Options of `deploy verify`.
#[derive(Debug, Clone)]
pub struct VerifyOptions {
    /// How long the probes are polled before the verdict.
    pub probe_deadline: Duration,
    /// Time between two probe polls.
    pub probe_interval: Duration,
    /// How long after a scan completed the gateway log is trusted to hold its
    /// lines (CV1). Skipped when the gateway has logged since.
    pub log_settle: Duration,
}

impl Default for VerifyOptions {
    fn default() -> Self {
        Self {
            probe_deadline: verify::PROBE_DEADLINE,
            probe_interval: verify::PROBE_INTERVAL,
            log_settle: Duration::from_millis(verify::LOG_SETTLE_MS.unsigned_abs()),
        }
    }
}

/// The `ign deploy verify` result (the envelope's `data`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeployVerifyResult {
    /// The `--env` value.
    pub env: String,
    /// Gateway directory name from the target.
    pub gateway: String,
    /// Deployment mode from the target.
    pub mode: String,
    /// The node that was verified: `master` (the default), `backup` or
    /// `both`.
    pub node: String,
    /// The WebDev project the tag probes were read through.
    pub project: String,
    /// True when no check failed.
    pub ok: bool,
    /// `deploy_verify_failed` when `ok` is false, else null.
    pub slug: Option<&'static str>,
    /// The six checks in fixed order.
    pub checks: Vec<verify::Check>,
    /// Declared resources `read_back` compared.
    pub resources_checked: usize,
    /// Non-failing notes.
    pub warnings: Vec<String>,
    /// The backup node's checks (`--node backup|both`, V2 shape with
    /// `budget_s` 0, `waited_s` 0 and `resync` null); absent for `master`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<BackupReport>,
    /// The secret-name check of each node `--node` ran (V7); absent for
    /// `master`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secrets: Option<SecretNamesReport>,
}

impl DeployVerifyResult {
    /// The red-verdict exit passthrough (Decision A), mirroring
    /// [`crate::actions::testing::TestingRunResult::failure_exit_code`]:
    /// `Some(6)` (target state) when a check failed, `None` otherwise. The
    /// envelope renders first; the binary decides the exit after.
    pub fn failure_exit_code(&self) -> Option<u8> {
        (!self.ok).then_some(6)
    }
}

/// Most pages of log lines read before the answer is called truncated.
const LOG_PAGES_MAX: usize = 5;

/// Verify the master gateway against `declared` (DPLN-07, DPLN-08, DPLN-09).
///
/// Reads only: `scan_status`, `deployment_mode`, `resource_list` or
/// `resource_singleton` per declared non-tag type, one read of the live files
/// through `reader`, the gateway log, and the tag probes through `project`'s
/// WebDev route. It acquires no scan-lock, triggers no scan and writes no
/// file. Evidence is gathered, then judged by [`verify::evaluate`].
///
/// A gateway that cannot be reached ([`CoreError::Network`]), any refusal of
/// the resource, scan or mode reads, and a transport failure return a
/// [`CoreError`]: there is nothing to judge. A refusal of the log or tag
/// source is evidence, not an error: that check fails with the refusal's slug
/// (never its text). A red verdict is a normal `Ok` result; the binary maps
/// it to exit 6 through [`DeployVerifyResult::failure_exit_code`].
pub async fn deploy_verify(
    declared: &Declared,
    project: &str,
    deploy: &dyn DeployApi,
    gateway: &dyn GatewayApi,
    reader: &dyn TreeReader,
    opts: &VerifyOptions,
) -> Result<DeployVerifyResult, CoreError> {
    let expected =
        verify::Expected::from_render(&declared.target.mode, &declared.outcome, &declared.manifest);

    let scan = deploy.scan_status(ScanKind::Config).await?;
    let scan_read_at = std::time::Instant::now();
    let mode = normalize_deployment_mode(deploy.deployment_mode().await?);
    let read_back = gather_read_back(deploy, &expected).await?;

    // The live files, once, over the read-only transport. Tag units are the
    // ones judged; reading is also what proves the host is reachable.
    let scope = Scope::from_manifest(&declared.manifest, &declared.target.mode);
    let live = reader.read(&scope).await?;
    let planned = plan(&declared.outcome.tree, &live, None, &scope);
    let files = planned
        .entries
        .iter()
        .filter(|entry| expected.tag_units.contains(&entry.key))
        .map(|entry| (entry.key.clone(), entry.status))
        .collect();

    let logs = if scan.scan_active {
        // Its lines are not readable yet; the check reports "not observed".
        Ok(Vec::new())
    } else {
        settle_logs(gateway, &scan, scan_read_at, opts.log_settle).await?;
        read_logs(gateway, verify::log_window_start(&scan)).await?
    };
    let probes = if expected.probes.is_empty() {
        None
    } else {
        Some(poll_probes(gateway, project, &expected.probes, opts).await?)
    };

    let observed = verify::ObservedState {
        scan,
        mode,
        read_back,
        files,
        logs,
        probes,
    };
    let report = verify::evaluate(&expected, &observed);
    Ok(DeployVerifyResult {
        env: declared.env.clone(),
        gateway: declared.target.gateway.clone(),
        mode: declared.target.mode.clone(),
        node: "master".to_string(),
        project: project.to_string(),
        ok: report.ok,
        slug: report.slug,
        checks: report.checks,
        resources_checked: report.resources_checked,
        warnings: report.warnings,
        backup: None,
        secrets: None,
    })
}

/// Which node(s) `deploy verify --node` looks at (D-12, V6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyNode {
    /// The six master checks; the default and byte-identical to the verb
    /// before Phase 24.
    Master,
    /// Only the backup checks.
    Backup,
    /// The six master checks, then the backup checks.
    Both,
}

impl VerifyNode {
    /// The word the flag and `data.node` use.
    pub fn as_str(self) -> &'static str {
        match self {
            VerifyNode::Master => "master",
            VerifyNode::Backup => "backup",
            VerifyNode::Both => "both",
        }
    }
}

/// The master's surfaces `deploy verify --node master|both` reads through.
pub type VerifyMasterSurfaces<'a> = (&'a dyn DeployApi, &'a dyn GatewayApi);

/// `deploy verify` over one node or both (D-12, V6).
///
/// `Master` is [`deploy_verify`] unchanged. `Backup` runs the backup checks
/// only: the redundancy check (`peerConnected` and not `syncPending`), the
/// mode, the REST read-back of the render, and the backup's files against the
/// master's live copy, read through `master_reader` (a `warn` when `backup`
/// has no file transport). `Both` runs the master's six checks, then the
/// backup's. Like `deploy_verify` it never waits, never resyncs and never
/// writes: the backup is reached through [`BackupNode`], which has no write
/// to express, and the master only through reads.
///
/// `master` is required for `Master` and `Both`, `backup` for `Backup` and
/// `Both`; a missing one is [`CoreError::InvalidInput`].
pub async fn deploy_verify_node(
    declared: &Declared,
    project: &str,
    node: VerifyNode,
    master: Option<VerifyMasterSurfaces<'_>>,
    master_reader: &dyn TreeReader,
    backup: Option<&BackupNode<'_>>,
    opts: &VerifyOptions,
) -> Result<DeployVerifyResult, CoreError> {
    let missing = |what: &str| CoreError::InvalidInput {
        reason: format!("deploy verify --node {} needs the {what}", node.as_str()),
    };
    let master_result = if node == VerifyNode::Backup {
        None
    } else {
        let (deploy, gateway) = master.ok_or_else(|| missing("master surfaces"))?;
        Some(deploy_verify(declared, project, deploy, gateway, master_reader, opts).await?)
    };
    if node == VerifyNode::Master {
        return master_result.ok_or_else(|| missing("master surfaces"));
    }
    let backup = backup.ok_or_else(|| missing("backup node"))?;
    let expected =
        verify::Expected::from_render(&declared.target.mode, &declared.outcome, &declared.manifest);
    let mut backup_warnings = Vec::new();
    let (tick, compared) = verify_backup_tick(
        declared,
        &expected,
        master_reader,
        backup,
        &mut backup_warnings,
    )
    .await;
    let report = verify_report(&tick);

    let mut result = match master_result {
        Some(result) => result,
        None => DeployVerifyResult {
            env: declared.env.clone(),
            gateway: declared.target.gateway.clone(),
            mode: declared.target.mode.clone(),
            node: String::new(),
            project: project.to_string(),
            ok: true,
            slug: None,
            checks: Vec::new(),
            resources_checked: compared,
            warnings: backup_warnings,
            backup: None,
            secrets: None,
        },
    };
    result.node = node.as_str().to_string();
    result.ok = result.ok && report.converged;
    result.slug = (!result.ok).then_some(verify::VERIFY_FAILED_SLUG);
    result.backup = Some(report);
    Ok(result)
}

/// One look at the backup for `deploy verify`: its redundancy status, mode,
/// REST read-back and files. Every read that cannot be answered becomes an
/// `unreachable` check carrying the error code (never the gateway's words);
/// verify has no pre-apply sample, so the sync timestamp is recorded, not
/// compared. Returns the tick and the number of resources the read-back
/// compared.
async fn verify_backup_tick(
    declared: &Declared,
    expected: &verify::Expected,
    master_reader: &dyn TreeReader,
    backup: &BackupNode<'_>,
    warnings: &mut Vec<String>,
) -> (TickOutcome, usize) {
    let signals = match backup.status().await {
        Ok(status) => Ok(BackupSignals::from_status(None, &status)),
        Err(error) => Err(error.code().to_string()),
    };
    let mode = match backup.deployment_mode().await {
        Ok(mode) => mode_backup_check(expected, mode.as_deref()),
        Err(error) => unreachable_check(BackupCheckId::Mode, error.code()),
    };
    let mut compared = 0;
    let read_back = match backup.read_back(expected).await {
        Ok(read_back) => {
            let (check, count) = read_back_backup_check(expected, &read_back, warnings);
            compared = count;
            check
        }
        Err(error) => unreachable_check(BackupCheckId::ReadBack, error.code()),
    };
    let (files, files_checked) = verify_backup_files(declared, master_reader, backup).await;
    (
        TickOutcome {
            signals,
            exhausted: false,
            content: Some(vec![mode, read_back, files]),
            files_checked,
            stable_ticks: 0,
        },
        compared,
    )
}

/// The backup's files against the master's live copy of every rendered path.
/// Nothing was deleted by verify, so no path is expected absent.
async fn verify_backup_files(
    declared: &Declared,
    master_reader: &dyn TreeReader,
    backup: &BackupNode<'_>,
) -> (BackupCheck, usize) {
    let scope = Scope::from_manifest(&declared.manifest, &declared.target.mode);
    let backup_files = match backup.read_files(&scope).await {
        // A backup with no file transport cannot be asked.
        None => return (files_skipped_check(), 0),
        Some(Ok(observed)) => effective_files(observed),
        Some(Err(error)) => {
            return (
                unreachable_check(BackupCheckId::Files, &format!("backup {}", error.code())),
                0,
            );
        }
    };
    let master_files = match master_reader.read(&scope).await {
        Ok(observed) => effective_files(observed),
        Err(error) => {
            return (
                unreachable_check(BackupCheckId::Files, &format!("master {}", error.code())),
                0,
            );
        }
    };
    let present: BTreeSet<_> = declared.outcome.tree.files.keys().cloned().collect();
    let comparison = compare_files(&master_files, &backup_files, &present, &BTreeSet::new());
    (files_check(&comparison), comparison.checked)
}

/// Read each declared, comparable resource from the REST effective view:
/// one list per named type, one singleton read per singleton. An absent
/// resource (or a type whose route answers 404) is `Missing`; any other
/// refusal is an error.
pub(crate) async fn gather_read_back(
    deploy: &dyn DeployApi,
    expected: &verify::Expected,
) -> Result<BTreeMap<String, verify::Remote>, CoreError> {
    let classify = |record: crate::client::config_resources::ResourceRecord| {
        if record.collection.as_deref() == Some(verify::SYSTEM_COLLECTION) {
            verify::Remote::System
        } else {
            verify::Remote::Config(record.config)
        }
    };
    let mut lists: BTreeMap<(String, String), Option<Vec<_>>> = BTreeMap::new();
    let mut out = BTreeMap::new();
    for res in &expected.resources {
        let remote = match &res.name {
            None => match deploy.resource_singleton(&res.module, &res.ty, None).await {
                Ok(record) => classify(record),
                Err(CoreError::NotFound { .. }) => verify::Remote::Missing,
                Err(e) => return Err(e),
            },
            Some(name) => {
                let type_key = (res.module.clone(), res.ty.clone());
                if !lists.contains_key(&type_key) {
                    let items = match deploy
                        .resource_list(&res.module, &res.ty, &ListQuery::default())
                        .await
                    {
                        Ok(envelope) => Some(envelope.items),
                        Err(CoreError::NotFound { .. }) => None,
                        Err(e) => return Err(e),
                    };
                    lists.insert(type_key.clone(), items);
                }
                lists
                    .get(&type_key)
                    .and_then(Option::as_ref)
                    .and_then(|items| items.iter().find(|r| r.name.as_deref() == Some(name)))
                    .map_or(verify::Remote::Missing, |r| classify(r.clone()))
            }
        };
        out.insert(res.key.clone(), remote);
    }
    Ok(out)
}

/// Wait until an empty answer from the log means something (CV1: a line is
/// readable up to about 1.5 s after the scan completed). The wait is skipped
/// when the gateway has logged anything stamped after the settle point.
async fn settle_logs(
    gateway: &dyn GatewayApi,
    scan: &ScanStatus,
    scan_read_at: std::time::Instant,
    settle: Duration,
) -> Result<(), CoreError> {
    if settle.is_zero() {
        return Ok(());
    }
    let settle_ms = i64::try_from(settle.as_millis()).unwrap_or(i64::MAX);
    let probe = LogQuery {
        start_time: Some(scan.last_scan_timestamp.saturating_add(settle_ms)),
        limit: 1,
        ..LogQuery::default()
    };
    match gateway.logs(&probe).await {
        Ok(envelope) if !envelope.items.is_empty() => return Ok(()),
        Err(e @ CoreError::Network { .. }) => return Err(e),
        // An unreadable log is reported by the real read that follows.
        Ok(_) | Err(_) => {}
    }
    tokio::time::sleep(settle.saturating_sub(scan_read_at.elapsed())).await;
    Ok(())
}

/// The WARN-or-higher log lines since `start_time` (CV1: no `logger` filter,
/// no `endTime`; matched client-side). The inner `Err` is the slug of why
/// they could not be read.
async fn read_logs(
    gateway: &dyn GatewayApi,
    start_time: i64,
) -> Result<Result<Vec<verify::LogLine>, String>, CoreError> {
    let mut lines = Vec::new();
    let mut offset = 0i64;
    for _ in 0..LOG_PAGES_MAX {
        let query = LogQuery {
            start_time: Some(start_time),
            min_level: Some("WARN".to_string()),
            limit: DEFAULT_LOG_LIMIT,
            offset,
            ..LogQuery::default()
        };
        match gateway.logs(&query).await {
            Ok(envelope) => {
                let count = i64::try_from(envelope.items.len()).unwrap_or(i64::MAX);
                lines.extend(envelope.items.into_iter().map(|e| verify::LogLine {
                    timestamp: e.timestamp,
                    logger: e.logger_name,
                    message: e.message,
                }));
                if count < DEFAULT_LOG_LIMIT {
                    return Ok(Ok(lines));
                }
                offset += count;
            }
            Err(e @ CoreError::Network { .. }) => return Err(e),
            Err(e) => return Ok(Err(e.code().to_string())),
        }
    }
    Ok(Err("logs_truncated".to_string()))
}

/// Read `paths` through the WebDev `tags` route every `probe_interval` until
/// every one is Good or `probe_deadline` passed; the last reading is judged.
/// Values are dropped at once. The inner `Err` is the slug of why the route
/// could not be read.
async fn poll_probes(
    gateway: &dyn GatewayApi,
    project: &str,
    paths: &[String],
    opts: &VerifyOptions,
) -> Result<Result<Vec<verify::ProbeReading>, String>, CoreError> {
    let interval = opts.probe_interval.max(Duration::from_millis(1));
    let started = tokio::time::Instant::now();
    loop {
        let readings: Vec<verify::ProbeReading> =
            match crate::actions::tags::tags_read(gateway, project, paths).await {
                Ok(result) => result
                    .results
                    .into_iter()
                    .enumerate()
                    .map(|(i, row)| verify::ProbeReading {
                        // A row that does not echo its path is the one asked for.
                        path: if row.path.is_empty() {
                            paths.get(i).cloned().unwrap_or_default()
                        } else {
                            row.path
                        },
                        quality: row.quality,
                    })
                    .collect(),
                Err(e @ CoreError::Network { .. }) => return Err(e),
                Err(e) => return Ok(Err(e.code().to_string())),
            };
        let all_good = paths.iter().all(|path| {
            readings
                .iter()
                .any(|r| &r.path == path && r.quality.starts_with("Good"))
        });
        if all_good || started.elapsed() + interval > opts.probe_deadline {
            return Ok(Ok(readings));
        }
        tokio::time::sleep(interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A repo whose `gateways/g/resources/` holds one file the pull replaces.
    fn repo_with_existing() -> (tempfile::TempDir, Existing) {
        let repo = tempfile::TempDir::new().expect("repo");
        let dir = repo.path().join("gateways/g/resources/ignition/x");
        std::fs::create_dir_all(&dir).expect("dirs");
        std::fs::write(dir.join("keep.json"), b"{\"keep\": true}").expect("file");
        let existing = Existing {
            files: vec!["resources/ignition/x/keep.json".to_string()],
            dirs: ["resources", "resources/ignition", "resources/ignition/x"]
                .iter()
                .map(|d| d.to_string())
                .collect(),
        };
        (repo, existing)
    }

    fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in std::fs::read_dir(dir).expect("read_dir") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    out.insert(path.clone(), Vec::new());
                    walk(&path, out);
                } else {
                    out.insert(path.clone(), std::fs::read(&path).expect("read"));
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(root, &mut out);
        out
    }

    /// CR-01: a pulled tree that cannot be written is refused before the first
    /// delete, so the existing tree is left exactly as it was.
    fn assert_refused_untouched(files: &[(&str, &str)], needle: &str) {
        let (repo, existing) = repo_with_existing();
        let before = snapshot(repo.path());
        let source = SourceTree::from_files(files.iter().copied()).expect("source");
        let err =
            write_pulled(repo.path(), "g", Some(&existing), &source, None).expect_err("refused");
        assert_eq!(err.code(), "deploy_tree_invalid", "{err}");
        assert!(err.to_string().contains(needle), "{needle}: {err}");
        assert_eq!(
            snapshot(repo.path()),
            before,
            "a refused pull must not delete the existing tree"
        );
    }

    #[test]
    fn unsafe_component_is_refused_before_any_delete() {
        assert_refused_untouched(
            &[
                ("resources/ignition/x/fine.json", "{}"),
                ("resources/ignition/x/bad\nname.json", "{}"),
            ],
            "unsafe path component",
        );
    }

    #[test]
    fn case_fold_collision_is_refused_before_any_delete() {
        assert_refused_untouched(
            &[
                ("resources/ignition/x/Foo.json", "{}"),
                ("resources/ignition/x/foo.json", "{}"),
            ],
            "case is folded",
        );
    }

    #[test]
    fn file_and_directory_clash_is_refused_before_any_delete() {
        assert_refused_untouched(
            &[
                ("resources/ignition/x", "{}"),
                ("resources/ignition/x/a.json", "{}"),
            ],
            "both a file and a directory",
        );
    }

    #[test]
    fn a_valid_tree_replaces_the_existing_one() {
        let (repo, existing) = repo_with_existing();
        let source =
            SourceTree::from_files([("resources/ignition/x/new.json", "{}")]).expect("source");
        write_pulled(repo.path(), "g", Some(&existing), &source, None).expect("written");
        let dir = repo.path().join("gateways/g/resources/ignition/x");
        assert!(!dir.join("keep.json").exists());
        assert!(dir.join("new.json").is_file());
    }
}
