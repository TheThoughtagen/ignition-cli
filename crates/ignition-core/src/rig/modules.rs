//! The generated compose override (Phase 16, RMOD-04/05/06): a hand-rolled,
//! `ign`-owned `compose.ign-modules.yml` (D-06 — no YAML crate) carrying a
//! read-only bind mount of a verified cached module artifact plus the
//! gateway's acceptance environment variables, passed as a SECOND `-f` on
//! every project-scoped compose invocation.
//!
//! **The override is NEVER merged onto the user's compose file** — it is a
//! separate file `ign` writes only inside `plan.project_dir`, regenerated
//! whole on every provisioning run (D-08), and deleted entirely once no
//! module remains declared. The user's compose file is never touched by
//! any function in this module.
//!
//! **Ordering matters (D-11):** the override's `-f` is always the SECOND
//! one — [`crate::rig::compose::up_args`] (and, from Task 3, the other
//! three builders) append it after the base file. An override that landed
//! first would invert compose's merge precedence and shift
//! `--project-directory`/`.env` inference (research Pitfall 3).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::ModuleDeclaration;
use crate::error::CoreError;
use crate::module::ModuleSpec;
use crate::module::fetch::{ArtifactSource, FetchPolicy, ModuleFeed};

use super::RigPlan;

/// The `ID@VERSION` separator for `--with-module` (D-15): unambiguous
/// because neither [`crate::module::validate_module_id`]'s charset
/// (`[a-z0-9-]`) nor [`crate::module::validate_version`]'s
/// (`[A-Za-z0-9._+-]`) includes `@`.
const WITH_MODULE_SEPARATOR: char = '@';

/// Split one `--with-module` flag value into a validated
/// `(id, ModuleDeclaration)` pair (D-15): one `split_once` on
/// [`WITH_MODULE_SEPARATOR`], then BOTH halves through the EXISTING
/// validators — no third validator is written here. A value with no
/// separator, or with either half empty (which also catches a bare
/// `@`), is `CoreError::InvalidInput` naming the value received and the
/// expected shape.
fn parse_with_module_flag(raw: &str) -> Result<(String, ModuleDeclaration), CoreError> {
    let Some((id, version)) = raw.split_once(WITH_MODULE_SEPARATOR) else {
        return Err(CoreError::InvalidInput {
            reason: format!(
                "--with-module value {raw:?} is missing the '@' separator \
                 (expected ID@VERSION, e.g. git@2.3.4)"
            ),
        });
    };
    crate::module::validate_module_id(id)?;
    crate::module::validate_version(version)?;
    Ok((
        id.to_string(),
        ModuleDeclaration {
            version: version.to_string(),
        },
    ))
}

/// Overlay `--with-module ID@VERSION` flags onto `config`'s declared
/// modules (D-15/D-16), PURE — no filesystem or network access. A flag
/// entry replaces `config`'s entry for the same id (the flag wins,
/// D-16's "additive to config for this invocation only, no subtractive
/// form" contract) — `config` is never mutated in place, only cloned.
/// `flags` order never affects the result: the returned map is a
/// `BTreeMap`, so repeated flags land in `ModuleSpec::id` order
/// regardless of which order they were passed on the command line.
///
/// Every refusal is [`CoreError::InvalidInput`] (exit 2) naming the
/// offending value and the expected `ID@VERSION` shape — a REGISTRY
/// check (is the id actually registered?) is deliberately NOT performed
/// here; that stays [`provision_modules`]'s job (`CoreError::
/// ModuleNotRegistered`, exit 3), which applies identically to a
/// config-declared id and a flag-declared one.
pub fn merge_declarations(
    config: &BTreeMap<String, ModuleDeclaration>,
    flags: &[String],
) -> Result<BTreeMap<String, ModuleDeclaration>, CoreError> {
    let mut merged = config.clone();
    for raw in flags {
        let (id, declaration) = parse_with_module_flag(raw)?;
        merged.insert(id, declaration);
    }
    Ok(merged)
}

/// Pre-flight JUST the `--with-module` flag values — BEFORE
/// [`crate::rig::resolve_plan`] ever runs, so a malformed or
/// unregistered flag value refuses at exit 2/3 with ZERO Docker
/// invocation (Task 2, D-15/D-13). Runs [`merge_declarations`] against
/// an EMPTY config map purely to reuse its syntax validation (the
/// config half is irrelevant here — only the flags' own well-
/// formedness and registration matter), then checks every resulting id
/// against the registry the same way [`provision_modules`] does.
///
/// This is a pre-check, not a replacement: [`provision_modules`]
/// performs the AUTHORITATIVE registry check once the plan (and its
/// config-declared modules) exist — a config-declared unregistered id
/// is still caught there. This function only shortens the path for the
/// flag's own mistakes, which is what lets those two refusals be
/// proven without ever touching Docker.
pub fn preflight_with_module_flags(flags: &[String]) -> Result<(), CoreError> {
    let parsed = merge_declarations(&BTreeMap::new(), flags)?;
    for id in parsed.keys() {
        crate::module::spec_for(id).ok_or_else(|| {
            let known: Vec<String> = crate::module::MODULES
                .iter()
                .map(|spec| spec.id.to_string())
                .collect();
            CoreError::ModuleNotRegistered {
                id: id.clone(),
                known,
            }
        })?;
    }
    Ok(())
}

/// The override's filename — always at `std::path::absolute(plan.
/// project_dir.join(OVERRIDE_FILENAME))` (D-10). `std::path::absolute`
/// (stable 1.79), never `canonicalize`: the latter resolves symlinks and
/// would rewrite a path the user recognizes.
pub const OVERRIDE_FILENAME: &str = "compose.ign-modules.yml";

/// The container-side module-scan directory on the stock
/// `inductiveautomation/ignition` image (16-RESEARCH.md, live-verified) —
/// the mount target is this directory joined with `<ModuleSpec::id>.modl`.
const CONTAINER_MODULE_DIR: &str = "/usr/local/bin/ignition/user-lib/modules";

/// One already-fetched, already-verified module artifact ready to mount —
/// the input to [`generate_override`]. `source` MUST be absolute (Task 2's
/// pre-flight enforces this before generation ever sees it).
#[derive(Debug, Clone)]
pub struct MountedModule {
    /// The registry entry (carries `id` and `gateway_module_id`).
    pub spec: &'static ModuleSpec,
    /// The pinned version string.
    pub version: String,
    /// Absolute path to the verified cached artifact on the host.
    pub source: PathBuf,
    /// Where the bytes came from this call (cache hit vs. fresh
    /// download) — carried for [`ProvisionedModule`], not used by
    /// generation itself.
    pub source_kind: ArtifactSource,
}

/// Single-quote a YAML scalar, doubling any embedded apostrophe (D-07):
/// never double-quoted, because a double-quoted YAML scalar treats a
/// backslash as an escape character, which would mangle a Windows path
/// like `C:\Users\...` a second way.
fn single_quote_yaml(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Double-quote a YAML scalar for an environment VALUE (D-07): bare `Y`
/// parses as a YAML boolean, so every `ACCEPT_*` value is quoted.
fn double_quote_yaml(value: &str) -> String {
    format!("\"{value}\"")
}

/// Join every mount's [`ModuleSpec::gateway_module_id`] into the
/// comma-separated form the stock `inductiveautomation/ignition`
/// image's entrypoint is documented to expect for
/// `ACCEPT_MODULE_CERTS`/`ACCEPT_MODULE_LICENSES`. **This format is an
/// ASSUMPTION, not verified live in this session** — see
/// `two_modules_on_one_rig_golden`'s doc comment. A single named helper
/// exists so that if plan 16-03's live gate finds a different separator
/// or shape, fixing it is a one-line change here, never a scatter
/// through [`generate_override`].
fn join_gateway_module_ids(mounts: &[&MountedModule]) -> String {
    mounts
        .iter()
        .map(|mount| mount.spec.gateway_module_id)
        .collect::<Vec<_>>()
        .join(",")
}

/// The mount target inside the container for one mount: the fixed
/// [`CONTAINER_MODULE_DIR`] joined with `<ModuleSpec::id>.modl`.
/// `ModuleSpec::id` is already constrained to `[a-z0-9-]{1,32}` by
/// [`crate::module::validate_module_id`] — no new validation is invented
/// here, and no user-supplied segment can traverse out of the directory.
fn mount_target(id: &str) -> String {
    format!("{CONTAINER_MODULE_DIR}/{id}.modl")
}

/// Generate the override's full contents — PURE, no filesystem access,
/// deterministic (Phase 16, D-06/D-07). `mounts` need not arrive
/// pre-sorted: this function sorts a local copy by [`ModuleSpec::id`] so
/// regeneration is byte-stable regardless of caller iteration order.
///
/// Emits, in order: the generated-by header (states `ign` owns the file,
/// that it is regenerated whole rather than merged, and that deleting it
/// fully reverts provisioning — RMOD-06); `services:` → `service` →
/// `environment:` (`ACCEPT_IGNITION_EULA` then `ACCEPT_MODULE_CERTS` then
/// `ACCEPT_MODULE_LICENSES`, double-quoted values, the two accept lists
/// comma-joining every mount's `gateway_module_id`); `volumes:` (one
/// long-form mapping entry per mount, `source`/`target` single-quoted
/// with apostrophe doubling, `read_only` a bare boolean). `\n` line
/// endings only, exactly one trailing newline.
pub fn generate_override(service: &str, mounts: &[MountedModule]) -> String {
    let mut sorted: Vec<&MountedModule> = mounts.iter().collect();
    sorted.sort_by_key(|mount| mount.spec.id);

    let accept_ids = join_gateway_module_ids(&sorted);

    let mut out = String::new();
    out.push_str("# Generated by ign — DO NOT EDIT.\n");
    out.push_str(
        "# This file is regenerated WHOLE on every `ign rig up`/`rig reset` that \
         provisions a declared module — it is never hand-merged. Deleting it fully \
         reverts module provisioning for this rig without touching anything you \
         wrote in your own compose file.\n",
    );
    out.push_str("services:\n");
    out.push_str(&format!("  {service}:\n"));
    out.push_str("    environment:\n");
    out.push_str(&format!(
        "      ACCEPT_IGNITION_EULA: {}\n",
        double_quote_yaml("Y")
    ));
    out.push_str(&format!(
        "      ACCEPT_MODULE_CERTS: {}\n",
        double_quote_yaml(&accept_ids)
    ));
    out.push_str(&format!(
        "      ACCEPT_MODULE_LICENSES: {}\n",
        double_quote_yaml(&accept_ids)
    ));
    out.push_str("    volumes:\n");
    for mount in &sorted {
        let target = mount_target(mount.spec.id);
        out.push_str("      - type: bind\n");
        out.push_str(&format!(
            "        source: {}\n",
            single_quote_yaml(&mount.source.display().to_string())
        ));
        out.push_str(&format!("        target: {}\n", single_quote_yaml(&target)));
        out.push_str("        read_only: true\n");
    }
    out
}

/// The absolute override path for `plan` (D-10):
/// `std::path::absolute(plan.project_dir.join(OVERRIDE_FILENAME))`.
/// `std::path::absolute` neither requires the path to exist nor resolves
/// symlinks (unlike `canonicalize`) — the deliberate choice so a path the
/// user recognizes is never silently rewritten.
fn override_path(plan: &RigPlan) -> Result<PathBuf, CoreError> {
    let joined = plan.project_dir.join(OVERRIDE_FILENAME);
    std::path::absolute(&joined).map_err(|err| {
        CoreError::Rig(format!(
            "cannot compute absolute override path for {}: {err}",
            joined.display()
        ))
    })
}

/// The pre-existing override on disk for `plan` (D-10), for verbs that
/// NEVER provision — `rig down`, `rig status`, `rig logs` (Task 3). These
/// verbs never fetch and never write, so the file's presence on disk IS
/// the whole truth about what the last `up` used. Returns the absolute
/// path ONLY when it exists AND is a regular file; any other state
/// (absent, a directory, or a path-computation failure) is `None` —
/// never a refusal, because these verbs must keep working on a rig that
/// has never provisioned a module at all.
///
/// A stale override surviving into a `down` is harmless: it only adds a
/// read-only mount to a service that is about to be removed. The next
/// `up` (through [`provision_modules`]) deletes it per D-08's tri-state
/// if nothing is declared, or regenerates it whole if something still
/// is — so a file this function finds never accumulates stale state
/// across cycles.
pub fn existing_override(plan: &RigPlan) -> Option<PathBuf> {
    let path = override_path(plan).ok()?;
    let metadata = std::fs::metadata(&path).ok()?;
    if metadata.is_file() { Some(path) } else { None }
}

/// Write, overwrite whole, or remove the override for `plan` — D-08's
/// tri-state, completed in Task 2 (RMOD-06):
///
/// - `mounts` non-empty: computes the absolute override path (D-10),
///   refuses BEFORE writing if something other than a regular file
///   already occupies it (never a swallowed write error), then writes
///   [`generate_override`]'s output WHOLE — replacing anything a
///   previous run left behind, never merged or appended — and returns
///   `Some(path)`.
/// - `mounts` empty: removes a pre-existing override, tolerating
///   `NotFound` (a second run against an already-absent file is still
///   `Ok`), and returns `None`. This is the delete half of RMOD-06: an
///   undeclared rig must never leave a stale mount behind (research
///   Pitfall 4).
///
/// After this call returns, the override's `-f` is only ever passed for
/// a path this run either wrote or found (D-08) — never an empty-but-
/// present file.
pub fn write_override(
    plan: &RigPlan,
    service: &str,
    mounts: &[MountedModule],
) -> Result<Option<PathBuf>, CoreError> {
    let path = override_path(plan)?;

    if mounts.is_empty() {
        return match std::fs::remove_file(&path) {
            Ok(()) => Ok(None),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(CoreError::Rig(format!(
                "cannot remove stale module override at {}: {err}",
                path.display()
            ))),
        };
    }

    if let Ok(metadata) = std::fs::metadata(&path)
        && !metadata.is_file()
    {
        return Err(CoreError::Rig(format!(
            "cannot write module override at {} — something other than a \
             regular file already occupies that path; remove it by hand \
             and re-run `ign rig up`",
            path.display()
        )));
    }

    let contents = generate_override(service, mounts);
    std::fs::write(&path, contents).map_err(|err| {
        CoreError::Rig(format!(
            "cannot write module override at {}: {err}",
            path.display()
        ))
    })?;
    Ok(Some(path))
}

/// Pre-flight a declared module's mount source BEFORE it becomes a
/// [`MountedModule`] and before generation or the runner ever sees it
/// (Task 2; Pitfalls 1 and 7; T-16-02). Checked in THIS order — a
/// relative path that happens to resolve under the current directory
/// must be refused for BEING relative, not accepted because it
/// resolved:
///
/// 1. `source` is absolute.
/// 2. `std::fs::metadata(source)` succeeds (the path is accessible).
/// 3. the metadata reports a regular file, not a directory — a missing
///    bind-mount source silently becomes an empty DIRECTORY on both
///    host and container (reproduced live in 16-RESEARCH.md), leaving a
///    rig that boots healthy with no module ever loaded.
///
/// Every failure is [`CoreError::Rig`] (exit 7, D-13) naming the module
/// id and the offending path, with an actionable next step.
pub fn preflight_mount_source(id: &str, source: &Path) -> Result<(), CoreError> {
    if !source.is_absolute() {
        return Err(CoreError::Rig(format!(
            "module {id:?} artifact path {} is not absolute — ign refuses \
             to mount a relative path (it would resolve against compose's \
             own working directory, not this cached artifact)",
            source.display()
        )));
    }
    let metadata = std::fs::metadata(source).map_err(|err| {
        CoreError::Rig(format!(
            "module {id:?} artifact at {} is not accessible: {err} — the \
             cache entry is missing; re-run `ign rig up` to re-fetch it",
            source.display()
        ))
    })?;
    if !metadata.is_file() {
        return Err(CoreError::Rig(format!(
            "module {id:?} artifact path {} is not a regular file (found a \
             directory) — a missing or corrupted bind-mount source \
             silently becomes an empty directory on both host and \
             container; remove it and re-run `ign rig up` to re-fetch",
            source.display()
        )));
    }
    Ok(())
}

/// One provisioned module — the `ign rig up`/`reset` result-data shape
/// (all keys always present, the [`crate::actions::rig::RigUpResult`]
/// convention).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProvisionedModule {
    /// The [`ModuleSpec::id`].
    pub id: String,
    /// The pinned version string.
    pub version: String,
    /// The gateway's own acceptance-variable value.
    pub gateway_module_id: String,
    /// The verified artifact's sha256 digest.
    pub digest_sha256: String,
    /// Whether the artifact came from the cache or a fresh download.
    pub source: ArtifactSource,
    /// The container-side mount path (`CONTAINER_MODULE_DIR/<id>.modl`).
    pub mount_target: String,
}

/// The result of [`provision_modules`]: the override path (`None` for an
/// undeclared rig) plus the per-module outcome. [`Default`] is the
/// no-declaration case — constructible without touching the network or
/// the disk, which is what makes threading it through every `rig_up`/
/// `rig_reset` call site (including the SC-1 pinned-vector tests) a
/// zero-cost no-op.
#[derive(Debug, Clone, Default)]
pub struct ModuleProvisioning {
    /// The override's absolute path, when one was written or found.
    pub override_file: Option<PathBuf>,
    /// The per-module outcome, in [`ModuleSpec::id`] order.
    pub modules: Vec<ProvisionedModule>,
}

impl ModuleProvisioning {
    /// The override file paths as a slice suitable for
    /// [`crate::rig::compose::up_args`]'s `override_files` parameter —
    /// empty for the no-declaration case (SC-1).
    pub fn override_files(&self) -> &[PathBuf] {
        self.override_file.as_slice()
    }
}

/// Resolve, fetch, pre-flight, generate, and write the override for every
/// module `declared` on `plan` (D-14). Iterates `declared` in
/// `BTreeMap` (sorted-by-id) order, which is what keeps
/// [`generate_override`]'s `ModuleSpec::id` ordering deterministic end
/// to end without an extra sort at this layer.
///
/// The no-declaration case (`declared.is_empty()`) returns
/// [`ModuleProvisioning::default`] WITHOUT touching service resolution
/// at all — a rig that publishes no gateway port and declares no module
/// must still provision cleanly (SC-1). With at least one module
/// declared, the service is resolved FIRST (before any fetch): `plan.
/// gateway_service` already carries `RigEntry::module_service`'s
/// precedence over the derived value, folded in by `resolve_entry`
/// (Task 1, D-12) — `None` here is refused (Task 2), naming the rig's
/// services and the config key that fixes it. Each declared id is then
/// looked up ([`CoreError::ModuleNotRegistered`], Task 2, D-13, exit 3
/// — a well-formed id that simply isn't registered is a config problem,
/// never a silent skip), fetched through Phase 15's
/// [`crate::module::fetch::ModuleFeed::fetch_and_verify`] (never
/// re-chmodded, re-hashed, or re-verified here — D-04), and
/// pre-flighted ([`preflight_mount_source`], Task 2) before it ever
/// becomes a [`MountedModule`].
pub async fn provision_modules(
    feed: &ModuleFeed,
    plan: &RigPlan,
    declared: &BTreeMap<String, ModuleDeclaration>,
    cache_root: &Path,
    policy: FetchPolicy,
) -> Result<ModuleProvisioning, CoreError> {
    if declared.is_empty() {
        return Ok(ModuleProvisioning::default());
    }

    let service = plan.gateway_service.clone().ok_or_else(|| {
        CoreError::Rig(format!(
            "cannot derive a gateway service to mount modules into for rig {:?} \
                 (services: {:?}) — set module_service under [rigs.{}] to name one \
                 explicitly",
            plan.name, plan.services, plan.name
        ))
    })?;

    let mut mounts: Vec<MountedModule> = Vec::new();
    let mut provisioned: Vec<ProvisionedModule> = Vec::new();
    for (id, declaration) in declared {
        let spec = crate::module::spec_for(id).ok_or_else(|| {
            let known: Vec<String> = crate::module::MODULES
                .iter()
                .map(|s| s.id.to_string())
                .collect();
            CoreError::ModuleNotRegistered {
                id: id.clone(),
                known,
            }
        })?;
        let fetched = feed
            .fetch_and_verify(spec, &declaration.version, cache_root, policy)
            .await?;
        preflight_mount_source(spec.id, &fetched.path)?;
        mounts.push(MountedModule {
            spec,
            version: declaration.version.clone(),
            source: fetched.path.clone(),
            source_kind: fetched.source,
        });
        provisioned.push(ProvisionedModule {
            id: spec.id.to_string(),
            version: declaration.version.clone(),
            gateway_module_id: spec.gateway_module_id.to_string(),
            digest_sha256: fetched.digest_sha256,
            source: fetched.source,
            mount_target: mount_target(spec.id),
        });
    }

    let override_file = write_override(plan, &service, &mounts)?;

    Ok(ModuleProvisioning {
        override_file,
        modules: provisioned,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::fetch::{ArtifactSource, FetchPolicy, ModuleFeed};
    use crate::module::{GIT_MODULE, PROJECT_SCAN_ENDPOINT};
    use sha2::{Digest, Sha256};

    /// The golden-byte pin (T-16-01, plan Task 1 item 1). Every value
    /// `generate_override` interpolates is either charset-validated
    /// upstream (`ModuleSpec::id`/`gateway_module_id`) or a path `ign`
    /// itself constructed — pinning the exact bytes, not a set of
    /// `contains` checks, is what proves a future change can never let
    /// an interpolated value terminate its YAML scalar and inject a
    /// sibling key. If this test starts failing because the header
    /// wording, key order, quote style, or line endings legitimately
    /// changed, update the literal deliberately — a silent drift here
    /// is exactly the regression this test exists to catch.
    #[test]
    fn generate_override_pins_golden_bytes_and_quoting() {
        let mount = MountedModule {
            spec: &GIT_MODULE,
            version: "2.3.4".to_string(),
            source: PathBuf::from("/cache/modules/git/2.3.4/Git-2.3.4-signed.modl"),
            source_kind: ArtifactSource::Cache,
        };
        let expected = concat!(
            "# Generated by ign — DO NOT EDIT.\n",
            "# This file is regenerated WHOLE on every `ign rig up`/`rig reset` that provisions a declared module — it is never hand-merged. Deleting it fully reverts module provisioning for this rig without touching anything you wrote in your own compose file.\n",
            "services:\n",
            "  ignition:\n",
            "    environment:\n",
            "      ACCEPT_IGNITION_EULA: \"Y\"\n",
            "      ACCEPT_MODULE_CERTS: \"com.axone_io.ignition.git\"\n",
            "      ACCEPT_MODULE_LICENSES: \"com.axone_io.ignition.git\"\n",
            "    volumes:\n",
            "      - type: bind\n",
            "        source: '/cache/modules/git/2.3.4/Git-2.3.4-signed.modl'\n",
            "        target: '/usr/local/bin/ignition/user-lib/modules/git.modl'\n",
            "        read_only: true\n",
        );
        assert_eq!(
            generate_override("ignition", std::slice::from_ref(&mount)),
            expected,
            "golden bytes drifted from the pinned override shape"
        );

        // D-07's quoting decisions, exercised separately from the main
        // golden: paths are SINGLE-quoted with apostrophe doubling (the
        // YAML single-quoted-scalar escape — this is what "round-trip"
        // means here), never double-quoted (a double-quoted scalar
        // would treat `\` as an escape character and mangle a Windows
        // path a second way). Env VALUES are always double-quoted — the
        // main golden above already pins that half (`"Y"` would
        // otherwise parse as a YAML boolean if left bare).
        let quoted_mount = MountedModule {
            spec: &GIT_MODULE,
            version: "9.9.9".to_string(),
            source: PathBuf::from("/cache/O'Brien's Modules/git-9.9.9.modl"),
            source_kind: ArtifactSource::Download,
        };
        let out = generate_override("ignition", std::slice::from_ref(&quoted_mount));
        assert!(
            out.contains("        source: '/cache/O''Brien''s Modules/git-9.9.9.modl'\n"),
            "an embedded apostrophe must double, never vanish or backslash-escape: {out}"
        );
    }

    /// SC-5's provisioning-shape proof (16-02 plan Task 1 item 4): two
    /// modules declared on one rig produce ONE override with two
    /// long-form bind mounts, ordered by [`crate::module::ModuleSpec::
    /// id`] (`git` < `project-scan-endpoint` lexicographically) —
    /// proven here by passing them in the OPPOSITE order, so the
    /// ordering in the golden can only come from `generate_override`'s
    /// own sort, never from caller iteration order — and both
    /// acceptance-variable lists comma-joining both gateway ids in that
    /// same order.
    ///
    /// **The comma-joining format is an ASSUMPTION, not verified live in
    /// this session** (16-01-SUMMARY.md, "What This Plan Does NOT
    /// Prove"): it is the stock `inductiveautomation/ignition` image's
    /// documented entrypoint convention, but no live gateway in this
    /// session has confirmed it accepts a comma-separated list naming
    /// TWO different modules. Plan 16-03's live gate is what confirms or
    /// falsifies it; [`join_gateway_module_ids`] is the one place a
    /// format correction would land.
    #[test]
    fn two_modules_on_one_rig_golden() {
        let git_mount = MountedModule {
            spec: &GIT_MODULE,
            version: "2.3.4".to_string(),
            source: PathBuf::from("/cache/modules/git/2.3.4-deadbeef.modl"),
            source_kind: ArtifactSource::Cache,
        };
        let scan_mount = MountedModule {
            spec: &PROJECT_SCAN_ENDPOINT,
            version: "1.0.0".to_string(),
            source: PathBuf::from("/cache/modules/project-scan-endpoint/1.0.0-cafebabe.modl"),
            source_kind: ArtifactSource::Download,
        };

        // Reverse registry-id order on purpose (see doc comment above).
        let mounts = [scan_mount, git_mount];
        let expected = concat!(
            "# Generated by ign — DO NOT EDIT.\n",
            "# This file is regenerated WHOLE on every `ign rig up`/`rig reset` that provisions a declared module — it is never hand-merged. Deleting it fully reverts module provisioning for this rig without touching anything you wrote in your own compose file.\n",
            "services:\n",
            "  ignition:\n",
            "    environment:\n",
            "      ACCEPT_IGNITION_EULA: \"Y\"\n",
            "      ACCEPT_MODULE_CERTS: \"com.axone_io.ignition.git,project-scan-endpoint\"\n",
            "      ACCEPT_MODULE_LICENSES: \"com.axone_io.ignition.git,project-scan-endpoint\"\n",
            "    volumes:\n",
            "      - type: bind\n",
            "        source: '/cache/modules/git/2.3.4-deadbeef.modl'\n",
            "        target: '/usr/local/bin/ignition/user-lib/modules/git.modl'\n",
            "        read_only: true\n",
            "      - type: bind\n",
            "        source: '/cache/modules/project-scan-endpoint/1.0.0-cafebabe.modl'\n",
            "        target: '/usr/local/bin/ignition/user-lib/modules/project-scan-endpoint.modl'\n",
            "        read_only: true\n",
        );
        assert_eq!(
            generate_override("ignition", &mounts),
            expected,
            "two-module golden drifted — check mount ordering, the comma-join, \
             or the per-mount block shape"
        );
    }

    /// Research Pitfall 6: a source path shaped like a Windows absolute
    /// path must appear INTACT inside the generated `source` scalar —
    /// the falsifiable proof that the long bind-mount mapping syntax
    /// (`source:`/`target:` keys, each its own line) is what
    /// `generate_override` emits, never the short colon-separated form
    /// (`C:\path:/target:ro`) that a drive-letter colon would corrupt.
    #[test]
    fn windows_style_source_path_golden() {
        let mount = MountedModule {
            spec: &GIT_MODULE,
            version: "2.3.4".to_string(),
            source: PathBuf::from(
                r"C:\Users\dev\AppData\Local\ignition-cli\modules\git\2.3.4-deadbeef.modl",
            ),
            source_kind: ArtifactSource::Cache,
        };
        let out = generate_override("ignition", std::slice::from_ref(&mount));
        assert!(
            out.contains(
                "        source: 'C:\\Users\\dev\\AppData\\Local\\ignition-cli\\modules\\git\\2.3.4-deadbeef.modl'\n"
            ),
            "the Windows-style path must survive intact inside a single-quoted \
             long-form mapping — a short-form colon-separated mount would have \
             mistaken the drive-letter colon for the source:target separator: {out}"
        );
    }

    /// Plan Task 1 item 5 / the inherited test-matrix rule: a
    /// one-module override left on disk by a PREVIOUS run, re-provisioned
    /// with a SECOND module added — the file must name both, with
    /// nothing duplicated and nothing from the old one-module content
    /// orphaned.
    #[test]
    fn adding_a_second_module_to_a_pre_existing_override_regenerates_whole() {
        let project_dir = tempfile::tempdir().expect("project tempdir");
        let plan = RigPlan {
            name: "fixture-rig".to_string(),
            compose_file: project_dir.path().join("docker-compose.yml"),
            project_dir: project_dir.path().to_path_buf(),
            services: vec!["ignition".to_string()],
            host_ports: Vec::new(),
            port_mappings: Vec::new(),
            volumes: Vec::new(),
            gateway_service: Some("ignition".to_string()),
            modules: BTreeMap::new(),
        };

        let git_mount = MountedModule {
            spec: &GIT_MODULE,
            version: "2.3.4".to_string(),
            source: PathBuf::from("/cache/modules/git/2.3.4-deadbeef.modl"),
            source_kind: ArtifactSource::Cache,
        };
        let path = write_override(&plan, "ignition", std::slice::from_ref(&git_mount))
            .expect("seed a one-module override, as if a prior run provisioned only git")
            .expect("mounts were non-empty");
        let one_module_contents = std::fs::read_to_string(&path).expect("read one-module override");
        assert!(
            one_module_contents.contains("git.modl")
                && !one_module_contents.contains("project-scan-endpoint"),
            "precondition: the seeded override must name only the git module"
        );

        let scan_mount = MountedModule {
            spec: &PROJECT_SCAN_ENDPOINT,
            version: "1.0.0".to_string(),
            source: PathBuf::from("/cache/modules/project-scan-endpoint/1.0.0-cafebabe.modl"),
            source_kind: ArtifactSource::Cache,
        };
        write_override(&plan, "ignition", &[git_mount.clone(), scan_mount.clone()])
            .expect("regenerate the override with both modules")
            .expect("mounts were non-empty");

        let two_module_contents =
            std::fs::read_to_string(&path).expect("read regenerated override");
        assert_eq!(
            two_module_contents,
            generate_override("ignition", &[git_mount, scan_mount]),
            "the file must equal the fresh golden for BOTH mounts"
        );
        assert_eq!(
            two_module_contents.matches("git.modl").count(),
            1,
            "the git mount must appear exactly once, not duplicated: {two_module_contents}"
        );
        assert_eq!(
            two_module_contents
                .matches("project-scan-endpoint.modl")
                .count(),
            1,
            "the newly added module must appear exactly once: {two_module_contents}"
        );
    }

    /// T-16-03 / SC-4 (plan Task 1 item 2): a stronger gate than a grep
    /// for write calls — record the user's compose file's BYTES before
    /// provisioning and assert byte-identity afterward. `ign` must
    /// write ONLY `compose.ign-modules.yml`, never the file sitting
    /// next to it. Exercises the WHOLE `provision_modules` path (not
    /// just `write_override`) against a pure cache hit — mirrors the
    /// `offline_with_populated_cache_succeeds` idiom in
    /// `tests/module_fetch_contract.rs`: an unroutable feed base proves
    /// no network round-trip is even attempted.
    #[tokio::test]
    async fn user_compose_file_is_never_modified() {
        let project_dir = tempfile::tempdir().expect("project tempdir");
        let compose_path = project_dir.path().join("docker-compose.yml");
        let compose_bytes: &[u8] =
            b"services:\n  ignition:\n    image: inductiveautomation/ignition:8.3.6\n";
        std::fs::write(&compose_path, compose_bytes).expect("seed the user's compose file");
        let before = std::fs::read(&compose_path).expect("read compose before provisioning");

        // Pre-seed the cache so `provision_modules` is a pure cache hit
        // — no mock server, no network path exists to accidentally
        // touch anything.
        let cache_root = tempfile::tempdir().expect("cache tempdir");
        let payload = b"synthetic .modl payload for user_compose_file_is_never_modified".to_vec();
        let mut hasher = Sha256::new();
        hasher.update(&payload);
        let digest = format!("{:x}", hasher.finalize());
        let module_dir = cache_root.path().join("modules").join("git");
        std::fs::create_dir_all(&module_dir).expect("create module cache dir");
        std::fs::write(module_dir.join(format!("2.3.4-{digest}.modl")), &payload)
            .expect("seed cached artifact");

        let plan = RigPlan {
            name: "fixture-rig".to_string(),
            compose_file: compose_path.clone(),
            project_dir: project_dir.path().to_path_buf(),
            services: vec!["ignition".to_string()],
            host_ports: vec![9088],
            port_mappings: Vec::new(),
            volumes: Vec::new(),
            gateway_service: Some("ignition".to_string()),
            modules: BTreeMap::new(),
        };
        let mut declared: BTreeMap<String, ModuleDeclaration> = BTreeMap::new();
        declared.insert(
            "git".to_string(),
            ModuleDeclaration {
                version: "2.3.4".to_string(),
            },
        );

        let feed = ModuleFeed::for_base(
            url::Url::parse("http://127.0.0.1:1").expect("unroutable url parses"),
        )
        .expect("feed builds");

        let provisioning = provision_modules(
            &feed,
            &plan,
            &declared,
            cache_root.path(),
            FetchPolicy::CacheFirst,
        )
        .await
        .expect("cache-hit provisioning succeeds with nothing listening on the network");

        let expected_override = project_dir.path().join(OVERRIDE_FILENAME);
        assert_eq!(
            provisioning.override_file.as_deref(),
            Some(expected_override.as_path()),
            "the override is written into the project dir, never the user's compose path"
        );
        assert!(
            expected_override.exists(),
            "the override file itself must exist on disk after provisioning"
        );

        let after = std::fs::read(&compose_path).expect("read compose after provisioning");
        assert_eq!(
            before, after,
            "the user's compose file must be byte-identical before and after provisioning"
        );
        assert_eq!(
            after, compose_bytes,
            "and must equal exactly the bytes this test seeded"
        );
    }

    // -------------------------------------------------------------------
    // merge_declarations / preflight_with_module_flags (16-02 Task 2,
    // D-15/D-16)
    // -------------------------------------------------------------------

    /// D-16: a flag naming a NEW id adds to config's declarations; a
    /// flag naming the SAME id as config OVERRIDES that id's version
    /// for this invocation — the entry appears once, flag version wins.
    #[test]
    fn merge_declarations_overlays_flag_onto_config() {
        let mut config = BTreeMap::new();
        config.insert(
            "git".to_string(),
            ModuleDeclaration {
                version: "2.3.4".to_string(),
            },
        );

        // A flag naming a second id: both present afterward.
        let merged = merge_declarations(&config, &["project-scan-endpoint@1.0.0".to_string()])
            .expect("well-formed flag merges");
        assert_eq!(merged.len(), 2);
        assert_eq!(merged["git"].version, "2.3.4", "config entry untouched");
        assert_eq!(merged["project-scan-endpoint"].version, "1.0.0");

        // A flag naming the SAME id as config: the flag's version wins,
        // the entry appears exactly once.
        let merged =
            merge_declarations(&config, &["git@9.9.9".to_string()]).expect("same-id flag merges");
        assert_eq!(merged.len(), 1, "one entry, not two, for the same id");
        assert_eq!(
            merged["git"].version, "9.9.9",
            "the FLAG's version wins over config's"
        );

        // Untouched config is never mutated by the merge.
        assert_eq!(config["git"].version, "2.3.4");
    }

    /// Every malformed shape refuses `invalid_input` / exit 2: no
    /// separator, an empty id half, an empty version half (a bare `@`
    /// hits this same branch) — these three are refused directly by
    /// `parse_with_module_flag`, so the message names the FULL raw
    /// value received. An id failing `validate_module_id` and a
    /// version failing `validate_version` are refused by those
    /// EXISTING validators instead (no third validator is written, per
    /// D-15) — their messages name only the offending half, which the
    /// second half of this table checks for.
    #[test]
    fn merge_declarations_refuses_malformed_values() {
        let config = BTreeMap::new();

        // Refused by parse_with_module_flag ITSELF — only the
        // missing-separator case, which is the one failure the parser
        // detects before either validator runs, so it is the only one
        // whose message can name the whole raw value. A value that DOES
        // split (even into an empty half) reaches a validator, and those
        // belong in the table below.
        let bad = "git-no-separator";
        let err = merge_declarations(&config, &[bad.to_string()])
            .expect_err("a value with no '@' must be refused");
        assert_eq!(err.code(), "invalid_input");
        assert_eq!(err.exit_code(), 2);
        assert!(
            err.to_string().contains(bad),
            "message names the received value {bad:?}: {err}"
        );

        // Refused by the EXISTING validate_module_id/validate_version
        // (D-15: no third validator) — the message names the offending
        // half, not necessarily the whole ID@VERSION value.
        for (bad, expected_fragment) in [
            ("UPPER@2.3.4", "UPPER"),
            ("git@../escape", "../escape"),
            // These DO split on '@' into an empty half, so they are
            // refused by the validators, not by the parser — the message
            // names which half was empty. Asserting the validator's own
            // wording keeps the empty-string fragment from matching
            // everything vacuously.
            ("@2.3.4", "module id \"\" is not a safe identifier"),
            ("@", "module id \"\" is not a safe identifier"),
            ("git@", "module version \"\" is not a safe version string"),
        ] {
            let err = merge_declarations(&config, &[bad.to_string()])
                .expect_err(&format!("{bad:?} must be refused"));
            assert_eq!(err.code(), "invalid_input", "{bad:?}");
            assert_eq!(err.exit_code(), 2, "{bad:?}");
            assert!(
                err.to_string().contains(expected_fragment),
                "message names the offending half {expected_fragment:?}: {err}"
            );
        }
    }

    /// D-15: repeated `--with-module` flags produce a map ordered by
    /// id (BTreeMap's natural order), never by the order flags were
    /// passed on the command line.
    #[test]
    fn merge_declarations_is_order_independent() {
        let config = BTreeMap::new();
        let forward = merge_declarations(
            &config,
            &[
                "project-scan-endpoint@1.0.0".to_string(),
                "git@2.3.4".to_string(),
            ],
        )
        .expect("forward order merges");
        let reverse = merge_declarations(
            &config,
            &[
                "git@2.3.4".to_string(),
                "project-scan-endpoint@1.0.0".to_string(),
            ],
        )
        .expect("reverse order merges");
        assert_eq!(forward, reverse, "flag order must not affect the result");
        assert_eq!(
            forward.keys().collect::<Vec<_>>(),
            vec!["git", "project-scan-endpoint"],
            "iteration order is id order, not flag order"
        );
    }

    /// The dispatch-level guard (Task 2): a malformed flag value
    /// refuses via the SAME `invalid_input` path as
    /// `merge_declarations` — `preflight_with_module_flags` is a
    /// thin wrapper, not a second implementation.
    #[test]
    fn preflight_with_module_flags_refuses_malformed_value() {
        let err = preflight_with_module_flags(&["not-a-flag-value".to_string()])
            .expect_err("malformed value refused");
        assert_eq!(err.code(), "invalid_input");
        assert_eq!(err.exit_code(), 2);
    }

    /// The dispatch-level guard's other half: a well-formed but
    /// UNREGISTERED id refuses `module_not_registered` / exit 3,
    /// naming the unregistered id and every currently registered one —
    /// reachable with ZERO plan/Docker context, which is what lets the
    /// CLI catch it before `resolve_plan` ever runs.
    #[test]
    fn preflight_with_module_flags_refuses_unregistered_id() {
        let err = preflight_with_module_flags(&["nope@1.0.0".to_string()])
            .expect_err("unregistered id refused");
        assert_eq!(err.code(), "module_not_registered");
        assert_eq!(err.exit_code(), 3);
        let message = err.to_string();
        assert!(message.contains("nope"), "{message}");
        assert!(message.contains("git"), "known ids listed: {message}");
        assert!(
            message.contains("project-scan-endpoint"),
            "known ids listed: {message}"
        );
    }

    /// The guard's happy path: every currently registered id, and a
    /// well-formed unregistered-nowhere-near-registry id is refused —
    /// proven together so a future registry change can't silently
    /// widen or narrow acceptance.
    #[test]
    fn preflight_with_module_flags_accepts_every_registered_id() {
        for spec in crate::module::MODULES {
            let flag = format!("{}@1.0.0", spec.id);
            preflight_with_module_flags(std::slice::from_ref(&flag))
                .unwrap_or_else(|err| panic!("{} must preflight cleanly: {err}", spec.id));
        }
    }

    /// RMOD-01 / plan Task 2 item 4: `--with-module` against a rig with
    /// NO config-declared module — `merge_declarations` then
    /// `provision_modules` end to end — writes an override and reports
    /// the module in `ModuleProvisioning::modules` (the shape
    /// `RigUpResult.provisioned_modules` clones verbatim); the SAME rig
    /// with NO flag (`merge_declarations` against an empty flag slice)
    /// writes nothing and reports an empty list (SC-1, re-proven at
    /// this seam).
    #[tokio::test]
    async fn with_module_flag_alone_provisions_and_empty_flags_stay_sc1() {
        let project_dir = tempfile::tempdir().expect("project tempdir");
        let plan = RigPlan {
            name: "fixture-rig".to_string(),
            compose_file: project_dir.path().join("docker-compose.yml"),
            project_dir: project_dir.path().to_path_buf(),
            services: vec!["ignition".to_string()],
            host_ports: Vec::new(),
            port_mappings: Vec::new(),
            volumes: Vec::new(),
            gateway_service: Some("ignition".to_string()),
            modules: BTreeMap::new(),
        };

        // Pre-seed the cache so this is a pure cache hit — the flag
        // merge/provisioning path is what's under test, not the feed.
        let cache_root = tempfile::tempdir().expect("cache tempdir");
        let payload = b"synthetic .modl payload for with_module_flag_alone_provisions".to_vec();
        let mut hasher = Sha256::new();
        hasher.update(&payload);
        let digest = format!("{:x}", hasher.finalize());
        let module_dir = cache_root.path().join("modules").join("git");
        std::fs::create_dir_all(&module_dir).expect("create module cache dir");
        std::fs::write(module_dir.join(format!("2.3.4-{digest}.modl")), &payload)
            .expect("seed cached artifact");

        let feed = ModuleFeed::for_base(
            url::Url::parse("http://127.0.0.1:1").expect("unroutable url parses"),
        )
        .expect("feed builds");

        // No config declaration, no flag: SC-1 stays intact at this seam.
        let empty_declared = merge_declarations(&plan.modules, &[]).expect("empty merge");
        assert!(empty_declared.is_empty());
        let empty_provisioning = provision_modules(
            &feed,
            &plan,
            &empty_declared,
            cache_root.path(),
            FetchPolicy::CacheFirst,
        )
        .await
        .expect("no-declaration provisioning succeeds");
        assert!(empty_provisioning.modules.is_empty());
        assert!(empty_provisioning.override_file.is_none());
        assert!(
            !override_path(&plan)
                .expect("override path computes")
                .exists(),
            "no override file written when nothing is declared"
        );

        // The flag alone, no config declaration: provisions and reports.
        let flagged =
            merge_declarations(&plan.modules, &["git@2.3.4".to_string()]).expect("flag-only merge");
        assert_eq!(flagged.len(), 1);
        let provisioning = provision_modules(
            &feed,
            &plan,
            &flagged,
            cache_root.path(),
            FetchPolicy::CacheFirst,
        )
        .await
        .expect("flag-only provisioning succeeds with a cache hit");
        assert_eq!(provisioning.modules.len(), 1);
        assert_eq!(provisioning.modules[0].id, "git");
        assert_eq!(provisioning.modules[0].version, "2.3.4");
        assert_eq!(provisioning.modules[0].source, ArtifactSource::Cache);
        assert!(provisioning.override_file.is_some());
        assert!(
            override_path(&plan)
                .expect("override path computes")
                .exists(),
            "override file written once a module is provisioned via the flag"
        );
    }

    /// THE actual SC-1 mechanism (not just its OBSERVABLE effect): a
    /// rig with NO derivable gateway service (`gateway_service: None`
    /// — no port targeting 8088/443, no `module_service` override) and
    /// NO declared module (config empty, no `--with-module` flag) must
    /// still provision cleanly. `provision_modules` resolves the
    /// service ONLY when something is declared (`declared.is_empty()`
    /// short-circuits BEFORE that resolution) — a rig this bare would
    /// otherwise hit `CoreError::Rig` ("cannot derive a gateway service
    /// to mount modules into") for a rig that never asked to mount
    /// anything. This is what "prove the short-circuit, don't assert
    /// it" means: the two sibling tests above always seed
    /// `gateway_service: Some(..)`, which would mask exactly this
    /// regression.
    #[tokio::test]
    async fn no_declaration_provisions_cleanly_even_with_no_derivable_gateway_service() {
        let project_dir = tempfile::tempdir().expect("project tempdir");
        let plan = RigPlan {
            name: "bare-rig".to_string(),
            compose_file: project_dir.path().join("docker-compose.yml"),
            project_dir: project_dir.path().to_path_buf(),
            services: vec!["sidecar".to_string()],
            host_ports: Vec::new(),
            port_mappings: Vec::new(),
            volumes: Vec::new(),
            gateway_service: None,
            modules: BTreeMap::new(),
        };

        let feed = ModuleFeed::for_base(
            url::Url::parse("http://127.0.0.1:1").expect("unroutable url parses"),
        )
        .expect("feed builds");
        let cache_root = tempfile::tempdir().expect("cache tempdir");

        let declared = merge_declarations(&plan.modules, &[]).expect("empty merge");
        assert!(declared.is_empty());
        let provisioning = provision_modules(
            &feed,
            &plan,
            &declared,
            cache_root.path(),
            FetchPolicy::CacheFirst,
        )
        .await
        .expect(
            "a rig with no declared module must provision cleanly even when it has \
             NO derivable gateway service — service resolution must never run for it",
        );
        assert!(provisioning.modules.is_empty());
        assert!(provisioning.override_file.is_none());
    }

    /// Test-matrix rule (mandatory): a rig whose override was written
    /// by a PREVIOUS run declaring one module in CONFIG, now invoked
    /// with an ADDITIONAL `--with-module` flag naming a SECOND, already
    /// pre-seeded in the cache — ends with an override naming BOTH.
    /// (Task 1's `adding_a_second_module_to_a_pre_existing_override_
    /// regenerates_whole` proves this at `write_override`'s layer; this
    /// test proves it through the FULL `merge_declarations` →
    /// `provision_modules` seam a real `--with-module` invocation
    /// drives.)
    #[tokio::test]
    async fn pre_existing_override_plus_a_flag_module_regenerates() {
        let project_dir = tempfile::tempdir().expect("project tempdir");
        let mut config_modules = BTreeMap::new();
        config_modules.insert(
            "git".to_string(),
            ModuleDeclaration {
                version: "2.3.4".to_string(),
            },
        );
        let plan = RigPlan {
            name: "fixture-rig".to_string(),
            compose_file: project_dir.path().join("docker-compose.yml"),
            project_dir: project_dir.path().to_path_buf(),
            services: vec!["ignition".to_string()],
            host_ports: Vec::new(),
            port_mappings: Vec::new(),
            volumes: Vec::new(),
            gateway_service: Some("ignition".to_string()),
            modules: config_modules,
        };

        let cache_root = tempfile::tempdir().expect("cache tempdir");
        let seed = |module_id: &str, version: &str, payload: &[u8]| {
            let mut hasher = Sha256::new();
            hasher.update(payload);
            let digest = format!("{:x}", hasher.finalize());
            let dir = cache_root.path().join("modules").join(module_id);
            std::fs::create_dir_all(&dir).expect("create module cache dir");
            std::fs::write(dir.join(format!("{version}-{digest}.modl")), payload)
                .expect("seed cached artifact");
        };
        seed("git", "2.3.4", b"synthetic git payload for regenerate test");
        seed(
            "project-scan-endpoint",
            "1.0.0",
            b"synthetic project-scan-endpoint payload for regenerate test",
        );

        let feed = ModuleFeed::for_base(
            url::Url::parse("http://127.0.0.1:1").expect("unroutable url parses"),
        )
        .expect("feed builds");

        // Run 1: config alone (as a prior `rig up` with no flag would).
        let declared = merge_declarations(&plan.modules, &[]).expect("config-only merge");
        let first = provision_modules(
            &feed,
            &plan,
            &declared,
            cache_root.path(),
            FetchPolicy::CacheFirst,
        )
        .await
        .expect("config-only provisioning succeeds");
        assert_eq!(first.modules.len(), 1);
        let override_file = first.override_file.clone().expect("override written");
        let one_module_contents =
            std::fs::read_to_string(&override_file).expect("read one-module override");
        assert!(
            one_module_contents.contains("git.modl")
                && !one_module_contents.contains("project-scan-endpoint"),
            "precondition: the seeded override must name only git"
        );

        // Run 2: the SAME rig, now with an additional --with-module flag.
        let declared =
            merge_declarations(&plan.modules, &["project-scan-endpoint@1.0.0".to_string()])
                .expect("config-plus-flag merge");
        assert_eq!(declared.len(), 2);
        let second = provision_modules(
            &feed,
            &plan,
            &declared,
            cache_root.path(),
            FetchPolicy::CacheFirst,
        )
        .await
        .expect("config-plus-flag provisioning succeeds");
        assert_eq!(second.modules.len(), 2);

        let two_module_contents =
            std::fs::read_to_string(&override_file).expect("read regenerated override");
        assert!(
            two_module_contents.contains("git.modl"),
            "the config-declared module survives: {two_module_contents}"
        );
        assert!(
            two_module_contents.contains("project-scan-endpoint.modl"),
            "the flag-added module is present: {two_module_contents}"
        );
        assert_eq!(
            two_module_contents.matches("git.modl").count(),
            1,
            "git must appear exactly once, not duplicated"
        );
        assert_eq!(
            two_module_contents
                .matches("project-scan-endpoint.modl")
                .count(),
            1,
            "the flag-added module must appear exactly once"
        );
    }

    /// The test-matrix rule's cache-mix half: a cache that already
    /// holds ONE of two modules while the OTHER must be fetched over
    /// the network — a wiremock server stands in for the second
    /// module's feed, and its mock is asserted hit EXACTLY once, which
    /// is the falsifiable proof the cached entry was never re-fetched.
    #[tokio::test]
    async fn provisioning_fetches_only_the_missing_artifact_when_one_is_cached() {
        let project_dir = tempfile::tempdir().expect("project tempdir");
        let plan = RigPlan {
            name: "fixture-rig".to_string(),
            compose_file: project_dir.path().join("docker-compose.yml"),
            project_dir: project_dir.path().to_path_buf(),
            services: vec!["ignition".to_string()],
            host_ports: Vec::new(),
            port_mappings: Vec::new(),
            volumes: Vec::new(),
            gateway_service: Some("ignition".to_string()),
            modules: BTreeMap::new(),
        };

        let cache_root = tempfile::tempdir().expect("cache tempdir");
        let cached_payload = b"synthetic git payload already cached".to_vec();
        let mut hasher = Sha256::new();
        hasher.update(&cached_payload);
        let cached_digest = format!("{:x}", hasher.finalize());
        let git_dir = cache_root.path().join("modules").join("git");
        std::fs::create_dir_all(&git_dir).expect("create git cache dir");
        std::fs::write(
            git_dir.join(format!("2.3.4-{cached_digest}.modl")),
            &cached_payload,
        )
        .expect("seed cached git artifact");

        // The second module's release feed — a real wiremock server,
        // asserted hit exactly once (`.expect(1)` on the mount).
        let server = wiremock::MockServer::start().await;
        let scan_payload = b"synthetic project-scan-endpoint payload, freshly fetched".to_vec();
        let mut hasher = Sha256::new();
        hasher.update(&scan_payload);
        let scan_digest = format!("{:x}", hasher.finalize());
        let release_json = serde_json::json!({
            "tag_name": "v1.0.0",
            "assets": [{
                "name": "Project-Scan-Endpoint.modl",
                "url": format!("{}/assets/1", server.uri()),
                "browser_download_url": format!("{}/download/Project-Scan-Endpoint.modl", server.uri()),
                "digest": format!("sha256:{scan_digest}"),
                "content_type": "application/octet-stream",
                "size": scan_payload.len(),
                "state": "uploaded",
            }],
        });
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(
                "/repos/bw-design-group/ignition-project-scan-endpoint/releases/tags/v1.0.0",
            ))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(release_json))
            .expect(1)
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(
                "/download/Project-Scan-Endpoint.modl",
            ))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_bytes(scan_payload.clone()))
            .expect(1)
            .mount(&server)
            .await;
        // If `git` were ever re-fetched, THIS path would be hit — it
        // isn't mounted at all, so any request to it 404s and the test
        // would fail loudly rather than silently re-downloading.

        let feed = ModuleFeed::for_base(server.uri().parse().expect("server uri parses"))
            .expect("feed builds");

        let mut declared = BTreeMap::new();
        declared.insert(
            "git".to_string(),
            ModuleDeclaration {
                version: "2.3.4".to_string(),
            },
        );
        declared.insert(
            "project-scan-endpoint".to_string(),
            ModuleDeclaration {
                version: "1.0.0".to_string(),
            },
        );

        let provisioning = provision_modules(
            &feed,
            &plan,
            &declared,
            cache_root.path(),
            FetchPolicy::CacheFirst,
        )
        .await
        .expect("mixed cache/fetch provisioning succeeds");

        assert_eq!(provisioning.modules.len(), 2);
        let git_outcome = provisioning
            .modules
            .iter()
            .find(|m| m.id == "git")
            .expect("git provisioned");
        assert_eq!(
            git_outcome.source,
            ArtifactSource::Cache,
            "the pre-seeded module must be a cache hit, never re-fetched"
        );
        let scan_outcome = provisioning
            .modules
            .iter()
            .find(|m| m.id == "project-scan-endpoint")
            .expect("project-scan-endpoint provisioned");
        assert_eq!(
            scan_outcome.source,
            ArtifactSource::Download,
            "the missing module must be freshly downloaded"
        );

        let contents = std::fs::read_to_string(
            provisioning
                .override_file
                .as_ref()
                .expect("override written"),
        )
        .expect("read override");
        assert!(contents.contains("git.modl") && contents.contains("project-scan-endpoint.modl"));
        // wiremock's `.expect(1)` mounts assert on drop (server teardown
        // at end of scope) — an unmet or exceeded expectation panics.
    }
}
