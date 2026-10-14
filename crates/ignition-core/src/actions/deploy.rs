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
//! warnings`. Later plans add keys additively (`delete_set`, `base`).
//! Entries carry paths and sha256 only, never file bytes.

use std::path::Path;

use serde::Serialize;

use crate::deploy::component_problem;
use crate::deploy::manifest::Manifest;
use crate::deploy::plan::{PlanEntry, PlanSummary, plan};
use crate::deploy::render::{RenderInput, RenderOutcome, RenderedResource, render};
use crate::deploy::target::Target;
use crate::deploy::transport::{Scope, TreeReader};
use crate::deploy::tree::{RenderMarker, SourceTree, read_repo_file, require_repo_dir, write_out};
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
    /// Render warnings, then plan warnings.
    pub warnings: Vec<String>,
}

/// Plan `declared` against the live files `reader` returns for the master
/// node. Reads once; never writes, never opens a gateway session, never
/// prints.
pub async fn deploy_plan(
    declared: &Declared,
    reader: &dyn TreeReader,
) -> Result<DeployPlanResult, CoreError> {
    let node = &declared.target.nodes.master;
    let transport = match (&node.ssh, &node.local) {
        (Some(_), _) => "ssh",
        (None, Some(_)) => "local",
        (None, None) => "none",
    };
    let scope = Scope::from_manifest(&declared.manifest, &declared.target.mode);
    let observed = reader.read(&scope).await?;
    let planned = plan(&declared.outcome.tree, &observed);

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
        warnings,
    })
}
