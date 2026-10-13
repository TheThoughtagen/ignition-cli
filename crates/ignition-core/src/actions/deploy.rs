//! Deploy actions (Phase 21): `ign deploy render`.
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

use std::path::Path;

use serde::Serialize;

use crate::deploy::component_problem;
use crate::deploy::manifest::Manifest;
use crate::deploy::render::{RenderInput, RenderedResource, render};
use crate::deploy::target::Target;
use crate::deploy::tree::{RenderMarker, SourceTree, write_out};
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

/// Render the declared tree of `env` under `repo`, optionally writing it
/// (and `render.json`) to `out`.
pub fn deploy_render(
    repo: &Path,
    env: &str,
    out: Option<&Path>,
) -> Result<DeployRenderResult, CoreError> {
    if let Some(problem) = component_problem(env) {
        return Err(CoreError::InvalidInput {
            reason: format!("--env {env}: {problem}"),
        });
    }

    let target_rel = format!("targets/{env}.yaml");
    let target_bytes = read_tree_file(
        repo,
        &target_rel,
        Some(&format!("--env {env} selects {target_rel}")),
    )?;
    let target = Target::parse(&target_rel, &target_bytes)?;

    let gateway_rel = format!("gateways/{}", target.gateway);
    let gateway_dir = repo.join("gateways").join(&target.gateway);
    require_real_dir(repo, "gateways")?;
    require_real_dir(repo, &gateway_rel)?;

    let manifest_rel = format!("{gateway_rel}/manifest.yaml");
    let manifest_bytes = read_tree_file(repo, &manifest_rel, None)?;
    let manifest = Manifest::parse(&manifest_rel, &manifest_bytes)?;

    let source = SourceTree::read_gateway(&gateway_dir, env)?;
    let outcome = render(&RenderInput {
        gateway: &target.gateway,
        env,
        manifest: &manifest,
        target: &target,
        source: &source,
    })?;

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

/// `symlink_metadata` of `repo/rel`, as a finding on failure.
fn tree_meta(repo: &Path, rel: &str, hint: Option<&str>) -> Result<std::fs::Metadata, CoreError> {
    match std::fs::symlink_metadata(repo.join(rel)) {
        Ok(meta) if meta.file_type().is_symlink() => Err(CoreError::deploy_tree_invalid(
            rel,
            None,
            None,
            "is a symlink; symlinks are refused",
        )),
        Ok(meta) => Ok(meta),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let message = match hint {
                Some(hint) => format!("does not exist ({hint})"),
                None => "does not exist".to_string(),
            };
            Err(CoreError::deploy_tree_invalid(rel, None, None, message))
        }
        Err(e) => Err(CoreError::deploy_tree_invalid(
            rel,
            None,
            None,
            format!("cannot inspect ({})", e.kind()),
        )),
    }
}

/// Require `repo/rel` to be a real directory (not a symlink, not a file).
fn require_real_dir(repo: &Path, rel: &str) -> Result<(), CoreError> {
    let meta = tree_meta(repo, rel, None)?;
    if meta.file_type().is_dir() {
        Ok(())
    } else {
        Err(CoreError::deploy_tree_invalid(
            rel,
            None,
            None,
            "must be a directory",
        ))
    }
}

/// Read `repo/rel` as a regular file; a symlink or non-file is refused.
/// Parent directories are checked too, so a symlinked parent cannot
/// redirect the read.
fn read_tree_file(repo: &Path, rel: &str, hint: Option<&str>) -> Result<Vec<u8>, CoreError> {
    if let Some((parent, _)) = rel.rsplit_once('/')
        && let Some(first) = parent.split('/').next()
        && repo.join(first).exists()
    {
        require_real_dir(repo, first)?;
    }
    let meta = tree_meta(repo, rel, hint)?;
    if !meta.file_type().is_file() {
        return Err(CoreError::deploy_tree_invalid(
            rel,
            None,
            None,
            "is not a regular file",
        ));
    }
    std::fs::read(repo.join(rel)).map_err(|e| {
        CoreError::deploy_tree_invalid(rel, None, None, format!("cannot read ({})", e.kind()))
    })
}
