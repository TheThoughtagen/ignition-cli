//! Contract tests for the compose-override module-injection mechanism
//! (Phase 16, `ignition-core::rig::modules`).
//!
//! Task 2 (this file): every way a bind mount can silently do nothing —
//! a missing artifact, a directory sitting where a file should be, a
//! relative source, a stale override — is a LOUD refusal, proven
//! against PRE-EXISTING on-disk state as well as fresh state (the
//! Phase-15-inherited test-matrix rule: 16-01-PLAN.md's `planner_locks`).
//! Task 3 extends this same file with the `-f` durability tests.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;
use ignition_core::actions::rig::{rig_down, rig_up};
use ignition_core::config::ModuleDeclaration;
use ignition_core::module::GIT_MODULE;
use ignition_core::module::fetch::{ArtifactSource, FetchPolicy, ModuleFeed};
use ignition_core::rig::modules::preflight_mount_source;
use ignition_core::rig::{
    ComposeOutput, ComposeRunner, ModuleProvisioning, MountedModule, OVERRIDE_FILENAME, RigPlan,
    existing_override, generate_override, provision_modules, write_override,
};

use sha2::{Digest, Sha256};

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// A minimal `RigPlan` fixture: a project directory, the derived
/// `gateway_service` (or `None` to exercise the underivable-service
/// refusal), and the rig's own service list (named in that refusal's
/// message). `modules` is always empty on the returned plan — every
/// test threads its own `declared` map through [`provision_modules`]
/// directly, mirroring how `resolve_entry` (Task 1) is the only path
/// that ever populates `RigPlan::modules`.
fn base_plan(project_dir: &Path, gateway_service: Option<&str>, services: &[&str]) -> RigPlan {
    RigPlan {
        name: "fixture-rig".to_string(),
        compose_file: project_dir.join("docker-compose.yml"),
        project_dir: project_dir.to_path_buf(),
        services: services.iter().map(|s| s.to_string()).collect(),
        host_ports: Vec::new(),
        port_mappings: Vec::new(),
        volumes: Vec::new(),
        gateway_service: gateway_service.map(str::to_string),
        modules: BTreeMap::new(),
    }
}

/// An unroutable feed base — every test in this file is either a pure
/// cache hit or refuses before the feed is ever contacted; an unroutable
/// address is what proves that no test accidentally depends on network
/// access.
fn unroutable_feed() -> ModuleFeed {
    ModuleFeed::for_base(url::Url::parse("http://127.0.0.1:1").expect("literal URL parses"))
        .expect("feed builds")
}

/// Seed a real cache entry (a FILE) at `<cache_root>/modules/<id>/
/// <version>-<sha256(contents)>.modl`, matching exactly what Phase 15's
/// fetcher itself would have written — returns the seeded path.
fn seed_cache_file(cache_root: &Path, module_id: &str, version: &str, contents: &[u8]) -> PathBuf {
    let dir = cache_root.join("modules").join(module_id);
    std::fs::create_dir_all(&dir).expect("create module cache dir");
    let digest = sha256_hex(contents);
    let path = dir.join(format!("{version}-{digest}.modl"));
    std::fs::write(&path, contents).expect("seed cached artifact");
    path
}

/// Seed a cache "entry" that is a DIRECTORY rather than a file, at the
/// exact path [`ignition_core::module::cached_entry`] would report as a
/// cache hit — `cached_entry` matches purely on filename SHAPE
/// (`<version>-<64 lowercase hex chars>.modl`), never on file type or
/// content hash, so a directory with the right name is indistinguishable
/// from a real artifact until something actually opens it. This is the
/// on-disk state Pitfall 1 describes: a bind-mount source that silently
/// became an empty directory.
fn seed_cache_directory_occupant(cache_root: &Path, module_id: &str, version: &str) -> PathBuf {
    let dir = cache_root.join("modules").join(module_id);
    std::fs::create_dir_all(&dir).expect("create module cache dir");
    let fake_digest = "a".repeat(64);
    let path = dir.join(format!("{version}-{fake_digest}.modl"));
    std::fs::create_dir_all(&path).expect("seed a directory occupying the artifact's path");
    path
}

/// Pitfall 1/7: a declared module whose cached path does not exist must
/// be refused BEFORE any generation or write — exercised directly
/// against [`preflight_mount_source`], the single guard every declared
/// module passes through. The path IS absolute (so this pins the
/// metadata-failure branch specifically, not the absoluteness check).
#[test]
fn missing_artifact_is_refused_before_compose() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir
        .path()
        .join("2.3.4-0000000000000000000000000000000000000000000000000000000000000000.modl");
    assert!(!missing.exists(), "precondition: the path must not exist");

    let err = preflight_mount_source("git", &missing)
        .expect_err("a missing artifact path must be refused");
    assert_eq!(err.code(), "rig_error");
    assert_eq!(err.exit_code(), 7);
    let message = err.to_string();
    assert!(
        message.contains("git"),
        "message must name the module id: {message}"
    );
    assert!(
        message.contains(&missing.display().to_string()),
        "message must name the offending path: {message}"
    );
}

/// The guard is `is_file`, not `exists` — proven end to end through
/// [`provision_modules`] with a cache "hit" that is actually a
/// directory (Pitfall 1's exact on-disk shape). No override file may
/// exist afterward: the refusal fires before `write_override` is ever
/// reached.
#[tokio::test]
async fn artifact_that_is_a_directory_is_refused() {
    let project_dir = tempfile::tempdir().expect("project tempdir");
    let cache_root = tempfile::tempdir().expect("cache tempdir");
    let occupant = seed_cache_directory_occupant(cache_root.path(), "git", "2.3.4");

    let plan = base_plan(project_dir.path(), Some("ignition"), &["ignition"]);
    let mut declared: BTreeMap<String, ModuleDeclaration> = BTreeMap::new();
    declared.insert(
        "git".to_string(),
        ModuleDeclaration {
            version: "2.3.4".to_string(),
        },
    );

    let err = provision_modules(
        &unroutable_feed(),
        &plan,
        &declared,
        cache_root.path(),
        FetchPolicy::CacheFirst,
    )
    .await
    .expect_err("a directory occupying the cached artifact's path must be refused");

    assert_eq!(err.code(), "rig_error");
    assert_eq!(err.exit_code(), 7);
    let message = err.to_string();
    assert!(
        message.contains("git"),
        "message must name the module id: {message}"
    );
    assert!(
        message.contains(&occupant.display().to_string()),
        "message must name the offending path: {message}"
    );

    assert!(
        !project_dir.path().join(OVERRIDE_FILENAME).exists(),
        "a refused provisioning run must never write the override"
    );
}

/// Pitfall 7: a `MountedModule` source that is not absolute must be
/// refused — and refused for BEING relative, not for happening to be
/// missing. The message content pins the ordering: it names the
/// absoluteness failure specifically (`"is not absolute"`), which would
/// change to the metadata-failure wording if the checks were ever
/// reordered to check existence first.
#[test]
fn relative_artifact_path_is_refused() {
    let relative = PathBuf::from("relative/2.3.4-deadbeef.modl");
    assert!(
        !relative.is_absolute(),
        "precondition: the fixture path must be relative"
    );

    let err =
        preflight_mount_source("git", &relative).expect_err("a relative source must be refused");
    assert_eq!(err.code(), "rig_error");
    assert_eq!(err.exit_code(), 7);
    let message = err.to_string();
    assert!(
        message.contains("git"),
        "message must name the module id: {message}"
    );
    assert!(
        message.contains("relative/2.3.4-deadbeef.modl"),
        "message must name the offending path: {message}"
    );
    assert!(
        message.contains("is not absolute"),
        "the relative-path check must fire BEFORE the existence check — a \
         reordering would surface the metadata-failure wording instead: {message}"
    );
}

/// The project directory already holds a DIRECTORY at the override file
/// name — a loud refusal via [`write_override`], never a swallowed
/// write error. The occupant is left untouched (`write_override` never
/// deletes what it did not create).
#[test]
fn override_path_that_is_a_directory_is_refused() {
    let project_dir = tempfile::tempdir().expect("project tempdir");
    let occupant = project_dir.path().join(OVERRIDE_FILENAME);
    std::fs::create_dir_all(&occupant).expect("seed a directory at the override's path");

    let plan = base_plan(project_dir.path(), Some("ignition"), &["ignition"]);
    let mount = MountedModule {
        spec: &GIT_MODULE,
        version: "2.3.4".to_string(),
        source: PathBuf::from("/cache/modules/git/2.3.4-deadbeef.modl"),
        source_kind: ArtifactSource::Cache,
    };

    let err = write_override(&plan, "ignition", std::slice::from_ref(&mount))
        .expect_err("a directory occupying the override's path must be refused");
    assert_eq!(err.code(), "rig_error");
    assert_eq!(err.exit_code(), 7);
    assert!(
        err.to_string().contains(&occupant.display().to_string()),
        "message must name the offending path: {err}"
    );
    // Pin the GUARD's own wording specifically — `std::fs::write` on a
    // directory also fails and would produce a superficially similar
    // "cannot write module override" message, which would let a
    // deleted pre-write guard hide behind that fallback I/O error.
    // Asserting the guard's exact phrasing is what makes this
    // falsifiable against that regression.
    assert!(
        err.to_string().contains("already occupies that path"),
        "message must be the PRE-WRITE guard's own wording, not a bare \
         std::fs::write I/O failure: {err}"
    );
    assert!(
        occupant.is_dir(),
        "the refusal must never touch the pre-existing occupant"
    );
}

/// D-13: a declared id with no registry entry is a CONFIG problem
/// (`module_not_registered`, exit 3) — never `rig_error`, never a
/// silent skip. The message names both the bad id and every id that IS
/// registered. No override may be written.
#[tokio::test]
async fn unknown_module_id_lists_known_ids() {
    let project_dir = tempfile::tempdir().expect("project tempdir");
    let cache_root = tempfile::tempdir().expect("cache tempdir");
    let plan = base_plan(project_dir.path(), Some("ignition"), &["ignition"]);

    let mut declared: BTreeMap<String, ModuleDeclaration> = BTreeMap::new();
    declared.insert(
        "nonexistent-module".to_string(),
        ModuleDeclaration {
            version: "1.0.0".to_string(),
        },
    );

    let err = provision_modules(
        &unroutable_feed(),
        &plan,
        &declared,
        cache_root.path(),
        FetchPolicy::CacheFirst,
    )
    .await
    .expect_err("an unregistered module id must be refused");

    assert_eq!(err.code(), "module_not_registered");
    assert_eq!(err.exit_code(), 3);
    let message = err.to_string();
    assert!(
        message.contains("nonexistent-module"),
        "message must name the bad id: {message}"
    );
    assert!(
        message.contains("git"),
        "message must list known ids: {message}"
    );

    assert!(!project_dir.path().join(OVERRIDE_FILENAME).exists());
}

/// D-12: `None` is only fatal when service resolution is actually
/// needed. A plan with no derivable gateway service and no
/// `module_service` override provisions FINE with nothing declared, and
/// refuses — naming the rig's services and the config key that fixes it
/// — the moment at least one module is declared.
#[tokio::test]
async fn service_underivable_with_modules_declared_is_refused() {
    let project_dir = tempfile::tempdir().expect("project tempdir");
    let cache_root = tempfile::tempdir().expect("cache tempdir");
    let plan = base_plan(project_dir.path(), None, &["worker"]);

    let empty: BTreeMap<String, ModuleDeclaration> = BTreeMap::new();
    let ok = provision_modules(
        &unroutable_feed(),
        &plan,
        &empty,
        cache_root.path(),
        FetchPolicy::CacheFirst,
    )
    .await
    .expect("an undeclared rig must provision fine even with no derivable service (SC-1)");
    assert!(ok.override_file.is_none());
    assert!(ok.modules.is_empty());

    let mut declared: BTreeMap<String, ModuleDeclaration> = BTreeMap::new();
    declared.insert(
        "git".to_string(),
        ModuleDeclaration {
            version: "2.3.4".to_string(),
        },
    );
    let err = provision_modules(
        &unroutable_feed(),
        &plan,
        &declared,
        cache_root.path(),
        FetchPolicy::CacheFirst,
    )
    .await
    .expect_err("the same underivable-service plan must refuse once a module is declared");

    assert_eq!(err.code(), "rig_error");
    assert_eq!(err.exit_code(), 7);
    let message = err.to_string();
    assert!(
        message.contains("worker"),
        "message must name the rig's services: {message}"
    );
    assert!(
        message.contains("module_service"),
        "message must name the config key that fixes it: {message}"
    );

    assert!(!project_dir.path().join(OVERRIDE_FILENAME).exists());
}

/// RMOD-06 / Pitfall 4: a `compose.ign-modules.yml` left on disk by a
/// PREVIOUS run, with the config now declaring nothing, must be gone
/// afterward — and a SECOND run against an already-absent file is still
/// `Ok` (`NotFound` tolerated).
#[test]
fn undeclaring_the_last_module_deletes_a_pre_existing_override() {
    let project_dir = tempfile::tempdir().expect("project tempdir");
    let plan = base_plan(project_dir.path(), Some("ignition"), &["ignition"]);

    let mount = MountedModule {
        spec: &GIT_MODULE,
        version: "2.3.4".to_string(),
        source: PathBuf::from("/cache/modules/git/2.3.4-deadbeef.modl"),
        source_kind: ArtifactSource::Cache,
    };
    let written = write_override(&plan, "ignition", std::slice::from_ref(&mount))
        .expect("seed a pre-existing override")
        .expect("mounts were non-empty, so a path must come back");
    assert!(
        written.exists(),
        "precondition: the stale override must exist on disk"
    );

    let result = write_override(&plan, "ignition", &[])
        .expect("deleting a pre-existing override must succeed");
    assert_eq!(
        result, None,
        "override_file must be None once nothing is declared"
    );
    assert!(!written.exists(), "the stale override must be gone");

    // Running it again with the file already absent is still Ok.
    let result_again = write_override(&plan, "ignition", &[])
        .expect("a second delete over an already-absent file must still be Ok");
    assert_eq!(result_again, None);
    assert!(!written.exists());
}

/// A pre-existing override whose content names an OLD version:
/// regeneration overwrites WHOLE, never appends — nothing from the old
/// content survives.
#[test]
fn regeneration_overwrites_a_stale_override_whole() {
    let project_dir = tempfile::tempdir().expect("project tempdir");
    let plan = base_plan(project_dir.path(), Some("ignition"), &["ignition"]);

    let old_mount = MountedModule {
        spec: &GIT_MODULE,
        version: "1.0.0".to_string(),
        source: PathBuf::from("/cache/modules/git/1.0.0-oldold.modl"),
        source_kind: ArtifactSource::Cache,
    };
    let path = write_override(&plan, "ignition", std::slice::from_ref(&old_mount))
        .expect("seed the stale override")
        .expect("mounts were non-empty");
    let old_contents = std::fs::read_to_string(&path).expect("read stale override");
    assert!(
        old_contents.contains("1.0.0-oldold.modl"),
        "precondition: the stale content must name the old version"
    );

    let new_mount = MountedModule {
        spec: &GIT_MODULE,
        version: "2.3.4".to_string(),
        source: PathBuf::from("/cache/modules/git/2.3.4-newnew.modl"),
        source_kind: ArtifactSource::Cache,
    };
    write_override(&plan, "ignition", std::slice::from_ref(&new_mount))
        .expect("regenerate the override")
        .expect("mounts were non-empty");

    let new_contents = std::fs::read_to_string(&path).expect("read regenerated override");
    assert_eq!(
        new_contents,
        generate_override("ignition", std::slice::from_ref(&new_mount)),
        "the file must equal the fresh golden for the NEW mount only"
    );
    assert!(
        !new_contents.contains("1.0.0-oldold.modl") && !new_contents.contains("oldold"),
        "nothing from the old content may survive regeneration: {new_contents}"
    );
    assert!(
        new_contents.contains("2.3.4-newnew.modl"),
        "the new mount must be present: {new_contents}"
    );
}

/// Two calls with identical inputs must produce identical strings —
/// `generate_override` is PURE and deterministic.
#[test]
fn generation_is_byte_stable() {
    let mount = MountedModule {
        spec: &GIT_MODULE,
        version: "2.3.4".to_string(),
        source: PathBuf::from("/cache/modules/git/2.3.4-deadbeef.modl"),
        source_kind: ArtifactSource::Cache,
    };
    let first = generate_override("ignition", std::slice::from_ref(&mount));
    let second = generate_override("ignition", std::slice::from_ref(&mount));
    assert_eq!(
        first, second,
        "two calls with identical inputs must produce identical bytes"
    );
}

/// A standing regression guard on the already-merged `ensure_readable`
/// (D-04, PR #13): seeded from a PRE-EXISTING cache entry at owner-only
/// mode — exactly the state the original defect left behind — then
/// provisioned through the NORMAL cache-hit path (not `fetch_and_verify`
/// called in isolation). Asserts the widened mode on the path the fetch
/// returns.
#[cfg(unix)]
#[tokio::test]
async fn provisioned_artifact_is_readable_by_a_non_root_container_user() {
    use std::os::unix::fs::PermissionsExt;

    let project_dir = tempfile::tempdir().expect("project tempdir");
    let cache_root = tempfile::tempdir().expect("cache tempdir");

    let body = b"synthetic .modl payload for the readability regression".to_vec();
    let seeded = seed_cache_file(cache_root.path(), "git", "2.3.4", &body);
    std::fs::set_permissions(&seeded, std::fs::Permissions::from_mode(0o600))
        .expect("chmod the seeded entry to owner-only");
    assert_eq!(
        std::fs::metadata(&seeded)
            .expect("seeded entry exists")
            .permissions()
            .mode()
            & 0o044,
        0,
        "precondition: the seeded entry must start owner-only"
    );

    let plan = base_plan(project_dir.path(), Some("ignition"), &["ignition"]);
    let mut declared: BTreeMap<String, ModuleDeclaration> = BTreeMap::new();
    declared.insert(
        "git".to_string(),
        ModuleDeclaration {
            version: "2.3.4".to_string(),
        },
    );

    let provisioning = provision_modules(
        &unroutable_feed(),
        &plan,
        &declared,
        cache_root.path(),
        FetchPolicy::CacheFirst,
    )
    .await
    .expect("a pre-existing owner-only cache hit must still provision (readability is repaired in place)");
    assert_eq!(provisioning.modules.len(), 1);

    let mode = std::fs::metadata(&seeded)
        .expect("entry still exists")
        .permissions()
        .mode();
    assert_eq!(
        mode & 0o044,
        0o044,
        "provisioning through the normal cache-hit path must leave the artifact \
         group+world readable (mode {mode:o})"
    );
}

// ---------------------------------------------------------------------
// Task 3: one `-f` helper for all four builders, and the down-then-up
// durability proof (SC-1, SC-3 mechanism half).
// ---------------------------------------------------------------------

/// Records every `(program, args)` call, in order; every call succeeds
/// with an empty (or, for `version`, parseable) stdout — sufficient for
/// the durability test below, where only the ARGUMENT VECTORS
/// `rig_up`/`rig_down` build matter, never response parsing.
#[derive(Default)]
struct RecordingRunner {
    calls: Mutex<Vec<(&'static str, Vec<String>)>>,
}

impl RecordingRunner {
    fn calls(&self) -> Vec<(&'static str, Vec<String>)> {
        self.calls.lock().expect("call log lock").clone()
    }
}

fn version_output() -> ComposeOutput {
    ComposeOutput {
        stdout: "Docker Compose version v2.24.0\n".to_string(),
        stderr: String::new(),
        code: 0,
    }
}

fn ok_output() -> ComposeOutput {
    ComposeOutput {
        stdout: String::new(),
        stderr: String::new(),
        code: 0,
    }
}

#[async_trait]
impl ComposeRunner for RecordingRunner {
    async fn run(&self, args: &[String]) -> ComposeOutput {
        let is_version = args.first().map(String::as_str) == Some("version");
        self.calls
            .lock()
            .expect("call log lock")
            .push(("docker compose", args.to_vec()));
        if is_version {
            version_output()
        } else {
            ok_output()
        }
    }

    async fn run_docker(&self, args: &[String]) -> ComposeOutput {
        self.calls
            .lock()
            .expect("call log lock")
            .push(("docker", args.to_vec()));
        ok_output()
    }

    async fn run_streaming(
        &self,
        args: &[String],
        _line_sink: &mut (dyn for<'a> FnMut(&'a str) + Send),
    ) -> ComposeOutput {
        self.calls
            .lock()
            .expect("call log lock")
            .push(("docker compose", args.to_vec()));
        ok_output()
    }
}

/// `rig_down`/`rig_status`/`rig_logs` never provision — they derive
/// their override purely from [`existing_override`]'s read of the
/// project directory (Task 3). Exercised directly against the function
/// itself: absent → `None`; a PRE-EXISTING file left by a run this test
/// process never made → found; a DIRECTORY occupying the override's
/// path (the same on-disk shape Pitfall 1 describes on the write side)
/// → never mistaken for a real override.
#[test]
fn down_uses_the_override_found_on_disk() {
    let project_dir = tempfile::tempdir().expect("project tempdir");
    let plan = base_plan(project_dir.path(), Some("ignition"), &["ignition"]);

    assert_eq!(
        existing_override(&plan),
        None,
        "no override file on disk → None"
    );

    // A PRE-EXISTING override — `write_override` here stands in for a
    // PRIOR run's `up` that this test process never itself drove
    // through `rig_up` (the Phase-15-inherited test-matrix rule:
    // pre-existing on-disk state, not only freshly created state).
    let mount = MountedModule {
        spec: &GIT_MODULE,
        version: "2.3.4".to_string(),
        source: PathBuf::from("/cache/modules/git/2.3.4-deadbeef.modl"),
        source_kind: ArtifactSource::Cache,
    };
    let written = write_override(&plan, "ignition", std::slice::from_ref(&mount))
        .expect("seed a pre-existing override")
        .expect("mounts were non-empty");

    assert_eq!(
        existing_override(&plan),
        Some(written.clone()),
        "a pre-existing override on disk must be found"
    );

    // A directory at the override's path must never be mistaken for a
    // real override — the same is_file discipline `write_override`'s
    // own pre-write guard already uses, applied here to the read side.
    std::fs::remove_file(&written).expect("remove the file override");
    std::fs::create_dir_all(&written).expect("seed a directory at the override's path");
    assert_eq!(
        existing_override(&plan),
        None,
        "a directory occupying the override's path must never be mistaken for a real override"
    );
}

/// SC-3's mechanism half: a `rig_up` / `rig_down` / `rig_up` sequence
/// records the SAME override path in all three compose invocations, and
/// the override file is still on disk BETWEEN them — provable without
/// Docker. `rig_up` threads the override through the `ModuleProvisioning`
/// the caller already resolved; `rig_down` never provisions and instead
/// derives it from what [`existing_override`] finds on disk — which is
/// exactly the override `write_override` seeds BEFORE this test's first
/// `rig_up` even runs (a PRE-EXISTING file, never one freshly written
/// mid-sequence by this test).
///
/// The assertion after the `down` call is the one that would fail if
/// anything ever deleted the override as a side effect of teardown —
/// `rig_down` only reads the file's presence; it must never write or
/// remove it.
#[tokio::test]
async fn down_then_up_passes_the_same_override_file() {
    let project_dir = tempfile::tempdir().expect("project tempdir");
    let plan = base_plan(project_dir.path(), Some("ignition"), &["ignition"]);

    let mount = MountedModule {
        spec: &GIT_MODULE,
        version: "2.3.4".to_string(),
        source: PathBuf::from("/cache/modules/git/2.3.4-deadbeef.modl"),
        source_kind: ArtifactSource::Cache,
    };
    let override_path = write_override(&plan, "ignition", std::slice::from_ref(&mount))
        .expect("seed a pre-existing override, as if a prior `up` provisioned it")
        .expect("mounts were non-empty, so a path must come back");
    assert!(
        override_path.exists(),
        "precondition: the override must exist before the sequence starts"
    );

    let provisioning = ModuleProvisioning {
        override_file: Some(override_path.clone()),
        modules: Vec::new(),
    };

    let runner = RecordingRunner::default();

    rig_up(&runner, &plan, 300, None, &provisioning)
        .await
        .expect("first up succeeds");
    assert!(
        override_path.exists(),
        "the override must still be on disk immediately after the first up"
    );

    rig_down(&runner, &plan).await.expect("down succeeds");
    assert!(
        override_path.exists(),
        "the override must survive `rig_down` — teardown must never delete it \
         (this is the assertion that would fail if teardown ever deleted it \
         as a side effect)"
    );

    rig_up(&runner, &plan, 300, None, &provisioning)
        .await
        .expect("second up succeeds");
    assert!(
        override_path.exists(),
        "the override must still be on disk after the second up"
    );

    let calls = runner.calls();
    // version, up, version, down, version, up — base_plan's fixture
    // declares no host_ports, so port_preflight makes zero calls on
    // either side of the cycle.
    assert_eq!(calls.len(), 6, "exactly the six scripted calls: {calls:?}");

    let override_str = override_path.display().to_string();
    let base_str = plan.compose_file.display().to_string();

    for (label, args) in [
        ("first up", &calls[1].1),
        ("down", &calls[3].1),
        ("second up", &calls[5].1),
    ] {
        let override_index = args
            .iter()
            .position(|arg| arg == &override_str)
            .unwrap_or_else(|| panic!("{label} call is missing the override -f arg: {args:?}"));
        let base_index = args
            .iter()
            .position(|arg| arg == &base_str)
            .unwrap_or_else(|| panic!("{label} call is missing the base -f arg: {args:?}"));
        assert!(
            override_index > base_index,
            "{label}: the override's -f must come AFTER the base file's -f \
             (D-11 — an override landing first would invert compose's merge \
             precedence): {args:?}"
        );
        assert_eq!(
            args[override_index - 1],
            "-f",
            "{label}: the override path must be immediately preceded by its own -f flag: {args:?}"
        );
    }
}
