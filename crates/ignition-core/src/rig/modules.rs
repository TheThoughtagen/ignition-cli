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
}
