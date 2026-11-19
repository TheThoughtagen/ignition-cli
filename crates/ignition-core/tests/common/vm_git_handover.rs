//! The steps the two VM handover gates share (27-25, D-18, D-24, checker W-5):
//! `lnxvm_git_handover` and `wincap_git_handover` drive ign's own ssh
//! transports, built the way the CLI builds them from a target file's `ssh`
//! node, over the production-shape gateway `.git` of a disposable project.
//!
//! A gate has two halves with the platform's own host commands between them:
//!
//! 1. [`Handover::before_import`]: the host read, the write preflight as
//!    `git_preflight_on` runs it, the worst-case gateway-object read (D-24)
//!    and the save, each against its 27-06 host budget;
//! 2. the gate removes `.git` the way the import does (one host command);
//! 3. [`Handover::after_import`]: `restore_and_advance` with its writer timed,
//!    the restore plus one manifest compare against `HOST_*_RESTORE_VERIFY`,
//!    the advance upload against `HOST_*_UPLOAD`, and a read back of HEAD.
//!
//! Every step prints `<label> <step>: measured <ms> ms budget <ms> ms` and
//! fails over its budget. No budget scales with anything; nothing here prints a
//! value of the repository, only counts, times and a short digest prefix.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use ignition_core::actions::deploy_projects::git::git_preflight_on;
use ignition_core::deploy::git_advance::{AdvancePrep, GitWrite, GitWriteName, prepare_advance};
use ignition_core::deploy::git_handover::{
    GitRestoreReport, GitSaveReport, restore_and_advance, save,
};
use ignition_core::deploy::git_objects::{
    GitObjectFile, GitObjectList, check_object_rel, gateway_head_digest,
};
use ignition_core::deploy::git_remote::HostGit;
use ignition_core::deploy::git_state::{
    GitFiles, GitHead, GitRefs, GitState, GitStateKind, classify,
};
use ignition_core::deploy::ledger::LedgerRoot;
use ignition_core::deploy::project_artifact::build_artifact;
use ignition_core::deploy::transport::projects::{
    GitEntry, GitHostRead, GitManifest, GitSize, GitStateWriter, ImportDir, ProjectHost,
};
use ignition_core::error::CoreError;

use super::prod_shape_git::{
    HOST_LINUX_GATEWAY_OBJECTS, HOST_LINUX_PREFLIGHT, HOST_LINUX_READ, HOST_LINUX_RESTORE_VERIFY,
    HOST_LINUX_SAVE, HOST_LINUX_UPLOAD, HOST_WINDOWS_GATEWAY_OBJECTS, HOST_WINDOWS_PREFLIGHT,
    HOST_WINDOWS_READ, HOST_WINDOWS_RESTORE_VERIFY, HOST_WINDOWS_SAVE, HOST_WINDOWS_UPLOAD,
    OFFLINE_GATEWAY_DIGEST,
};

/// The gateway URL a VM gate may hand to `ign` (27-29, WR-D5, T-27-108): `https://` to any
/// host, `http://` only to a loopback host (`localhost`, `127.0.0.1`, `::1`). The VM
/// gateways serve plain HTTP, so the driver tunnels the port over its pinned ssh
/// connection and exports the loopback URL in `var`; an unset variable refuses too, so a
/// hand run without the driver cannot fall back to the VM's own address. The parse is a
/// URL parse, never a string match (`http://127.0.0.1@evil` has the host `evil`). The
/// refusal names the variable and the driver `steps` and never the URL.
pub fn vm_gateway_url_check(raw: Option<&str>, var: &str, steps: &str) -> Result<String, String> {
    let refuse = |why: &str| {
        format!(
            "REFUSING: {var} {why}; run this gate through `{steps}`, which reaches the gateway \
             only through a pinned ssh tunnel on 127.0.0.1"
        )
    };
    let raw = raw.ok_or_else(|| refuse("is not set"))?;
    if raw.is_empty() || raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(refuse("is empty or holds whitespace"));
    }
    let parsed = url::Url::parse(raw).map_err(|_| refuse("is not a URL"))?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(refuse("carries credentials"));
    }
    let loopback = match parsed.host() {
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(addr)) => addr == std::net::Ipv4Addr::LOCALHOST,
        Some(url::Host::Ipv6(addr)) => addr == std::net::Ipv6Addr::LOCALHOST,
        None => false,
    };
    match parsed.scheme() {
        "https" if parsed.host().is_some() => Ok(raw.to_string()),
        "http" if loopback => Ok(raw.to_string()),
        "http" => Err(refuse(
            "is a cleartext http:// URL to a host that is not loopback; the gateway token would cross the network",
        )),
        _ => Err(refuse("is not an http:// loopback or https:// URL")),
    }
}

/// [`vm_gateway_url_check`] on the environment variable `var`; a refusal panics before any
/// `ign` call.
pub fn vm_gateway_url(var: &str, steps: &str) -> String {
    let raw = std::env::var(var).ok();
    vm_gateway_url_check(raw.as_deref(), var, steps).unwrap_or_else(|refusal| panic!("{refusal}"))
}

/// The 27-06 host budgets of one host family.
#[derive(Clone, Copy)]
pub struct Budgets {
    pub read: Duration,
    pub save: Duration,
    pub restore_verify: Duration,
    pub upload: Duration,
    pub preflight: Duration,
    pub objects: Duration,
    /// The resolve and digest on ign's machine after the objects are fetched.
    pub digest: Duration,
}

impl Budgets {
    pub const LINUX: Budgets = Budgets {
        read: HOST_LINUX_READ,
        save: HOST_LINUX_SAVE,
        restore_verify: HOST_LINUX_RESTORE_VERIFY,
        upload: HOST_LINUX_UPLOAD,
        preflight: HOST_LINUX_PREFLIGHT,
        objects: HOST_LINUX_GATEWAY_OBJECTS,
        digest: OFFLINE_GATEWAY_DIGEST,
    };
    pub const WINDOWS: Budgets = Budgets {
        read: HOST_WINDOWS_READ,
        save: HOST_WINDOWS_SAVE,
        restore_verify: HOST_WINDOWS_RESTORE_VERIFY,
        upload: HOST_WINDOWS_UPLOAD,
        preflight: HOST_WINDOWS_PREFLIGHT,
        objects: HOST_WINDOWS_GATEWAY_OBJECTS,
        digest: OFFLINE_GATEWAY_DIGEST,
    };
}

/// Every row that ran over its budget, `<label> <row>: <ms> ms over <ms> ms`.
static OVER_BUDGET: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// One timed row: printed beside its budget. A row over its budget is
/// recorded and the gate goes on, so every row of a run is measured; the gate
/// fails at [`finish`] naming each one. No budget is scaled or skipped.
pub fn row(label: &str, name: &str, measured: Duration, budget: Duration) {
    let over = measured > budget;
    println!(
        "{label} {name}: measured {} ms budget {} ms{}",
        measured.as_millis(),
        budget.as_millis(),
        if over { " OVER BUDGET" } else { "" }
    );
    if over {
        OVER_BUDGET.lock().expect("rows").push(format!(
            "{label} {name}: measured {} ms is over the budget of {} ms",
            measured.as_millis(),
            budget.as_millis()
        ));
    }
}

/// Fail when any row ran over its budget.
pub fn finish() {
    let over = OVER_BUDGET.lock().expect("rows");
    assert!(over.is_empty(), "rows over their budget: {over:?}");
}

/// A line with no budget (a count, a shape).
pub fn note(label: &str, text: &str) {
    println!("{label} {text}");
}

/// A release checkout at tag `v1.1.0` of the emitted production-shape remote
/// (`PRODSHAPE_GIT_OUT/capapp.git`), outside every tracked tree.
pub struct Release {
    _dir: tempfile::TempDir,
    pub checkout: PathBuf,
    pub commit: String,
}

/// The fixture directory the 27-06 generator emitted, from `PRODSHAPE_GIT_OUT`.
pub fn fixture_dir() -> PathBuf {
    let out = PathBuf::from(
        std::env::var("PRODSHAPE_GIT_OUT")
            .expect("PRODSHAPE_GIT_OUT names the emitted production-shape fixture"),
    );
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root")
        .canonicalize()
        .expect("repo root resolves");
    assert!(
        !out.canonicalize()
            .expect("fixture resolves")
            .starts_with(&repo_root),
        "PRODSHAPE_GIT_OUT must be outside the repository"
    );
    assert!(
        out.join("capapp.git").is_dir(),
        "PRODSHAPE_GIT_OUT holds no capapp.git"
    );
    out
}

fn host_git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {} failed: {}",
        args.first().copied().unwrap_or(""),
        ignition_core::deploy::git_state::redact_text(&String::from_utf8_lossy(&out.stderr))
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

pub fn release_checkout(fixture: &Path) -> Release {
    let dir = tempfile::tempdir().expect("tempdir");
    let checkout = dir.path().join("release");
    host_git(
        dir.path(),
        &[
            "clone",
            "-q",
            "--no-hardlinks",
            fixture.join("capapp.git").to_str().expect("utf8"),
            checkout.to_str().expect("utf8"),
        ],
    );
    host_git(&checkout, &["checkout", "-q", "v1.1.0"]);
    let commit = host_git(&checkout, &["rev-parse", "v1.1.0^{commit}"]);
    Release {
        _dir: dir,
        checkout,
        commit,
    }
}

/// The commit a classified repo's HEAD names.
pub fn head_commit(refs: &GitRefs) -> String {
    match &refs.head {
        GitHead::Branch {
            commit: Some(commit),
            ..
        }
        | GitHead::Detached { commit } => commit.clone(),
        other => panic!("HEAD names no commit: {other:?}"),
    }
}

/// The writer of the run, timed: each save, restore and advance upload leaves
/// its duration.
pub struct TimedWriter<'a> {
    inner: &'a dyn GitStateWriter,
    spans: Mutex<Vec<(&'static str, Duration)>>,
}

impl<'a> TimedWriter<'a> {
    pub fn new(inner: &'a dyn GitStateWriter) -> TimedWriter<'a> {
        TimedWriter {
            inner,
            spans: Mutex::new(Vec::new()),
        }
    }

    pub fn span(&self, name: &'static str) -> Option<Duration> {
        let spans = self.spans.lock().expect("spans");
        let mut hits = spans.iter().filter(|(n, _)| *n == name).map(|(_, d)| *d);
        let first = hits.next()?;
        Some(hits.fold(first, |sum, next| sum + next))
    }

    fn record(&self, name: &'static str, started: Instant) {
        self.spans
            .lock()
            .expect("spans")
            .push((name, started.elapsed()));
    }
}

#[async_trait::async_trait]
impl GitStateWriter for TimedWriter<'_> {
    async fn save_git(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<GitManifest, CoreError> {
        let started = Instant::now();
        let out = self.inner.save_git(gateway, project, run_id).await;
        self.record("save", started);
        out
    }

    async fn restore_git(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<GitManifest, CoreError> {
        let started = Instant::now();
        let out = self.inner.restore_git(gateway, project, run_id).await;
        self.record("restore", started);
        out
    }

    async fn write_git_files(&self, project: &str, writes: &[GitWrite]) -> Result<(), CoreError> {
        let started = Instant::now();
        let out = self.inner.write_git_files(project, writes).await;
        self.record("upload", started);
        out
    }

    async fn remove_git_save(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<(), CoreError> {
        self.inner.remove_git_save(gateway, project, run_id).await
    }

    async fn probe_write(&self, project: &str, run_id: &str) -> Result<(), CoreError> {
        self.inner.probe_write(project, run_id).await
    }
}

/// A [`GitHostRead`] that adds up the time of the two object reads (the same
/// accounting the 27-11 rows use) and counts the fetches.
struct TimedRead<'a> {
    inner: &'a dyn GitHostRead,
    spent: Mutex<Duration>,
    fetches: Mutex<(usize, usize, u64)>,
}

#[async_trait::async_trait]
impl GitHostRead for TimedRead<'_> {
    async fn project_file_digests(
        &self,
        project: &str,
    ) -> Result<Option<ignition_core::deploy::git_advance::ProjectDisk>, CoreError> {
        self.inner.project_file_digests(project).await
    }
    async fn read_git_state(&self, project: &str) -> Result<GitFiles, CoreError> {
        self.inner.read_git_state(project).await
    }
    async fn git_manifest(&self, project: &str) -> Result<GitManifest, CoreError> {
        self.inner.git_manifest(project).await
    }
    async fn save_manifest(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<GitManifest, CoreError> {
        self.inner.save_manifest(gateway, project, run_id).await
    }
    async fn git_file_digest(
        &self,
        project: &str,
        name: &GitWriteName,
    ) -> Result<Option<String>, CoreError> {
        self.inner.git_file_digest(project, name).await
    }
    async fn read_git_yaml(&self) -> Result<Option<Vec<u8>>, CoreError> {
        self.inner.read_git_yaml().await
    }
    async fn project_dir_entry(
        &self,
        project: &str,
        dir: ImportDir,
    ) -> Result<GitEntry, CoreError> {
        self.inner.project_dir_entry(project, dir).await
    }
    async fn free_bytes(&self, gateway: &str, project: &str) -> Result<u64, CoreError> {
        self.inner.free_bytes(gateway, project).await
    }
    async fn git_size(&self, project: &str) -> Result<GitSize, CoreError> {
        self.inner.git_size(project).await
    }
    async fn list_git_objects(&self, project: &str) -> Result<GitObjectList, CoreError> {
        let started = Instant::now();
        let out = self.inner.list_git_objects(project).await;
        *self.spent.lock().expect("lock") += started.elapsed();
        out
    }
    async fn fetch_git_objects(
        &self,
        project: &str,
        files: &[GitObjectFile],
        dest: &Path,
    ) -> Result<(), CoreError> {
        for file in files {
            check_object_rel(&file.rel).expect("the listing names plain object paths");
        }
        let started = Instant::now();
        let out = self.inner.fetch_git_objects(project, files, dest).await;
        *self.spent.lock().expect("lock") += started.elapsed();
        let mut fetches = self.fetches.lock().expect("lock");
        fetches.0 += 1;
        fetches.1 += files.len();
        fetches.2 += files.iter().map(|f| f.size).sum::<u64>();
        out
    }
}

/// What the first half leaves for the second.
pub struct Before {
    pub state: GitState,
    pub saved: GitSaveReport,
    pub prep: AdvancePrep,
    pub artifact_zip: Vec<u8>,
    pub manifest: GitManifest,
    pub head_before: String,
}

/// One gate's context.
pub struct Handover<'a> {
    pub label: &'static str,
    pub host: &'a dyn ProjectHost,
    pub writer: &'a dyn GitStateWriter,
    pub gateway: &'a str,
    pub project: &'a str,
    pub run: &'a str,
    pub budgets: Budgets,
}

impl Handover<'_> {
    fn read(&self) -> &dyn GitHostRead {
        self.host
            .git()
            .expect("an ssh node's project host answers the git read")
    }

    /// `read_git_state`, the parse and the classification of the repository as
    /// the run's prepare does them.
    pub async fn classify_now(&self) -> (GitState, GitRefs, Duration) {
        let started = Instant::now();
        let files = self
            .read()
            .read_git_state(self.project)
            .await
            .expect("read_git_state");
        files.check_caps().expect("caps");
        let git = HostGit::new();
        let config = git
            .parse_config(
                files
                    .config
                    .as_deref()
                    .expect("the repository has a config"),
            )
            .await
            .expect("config parses");
        let refs = git
            .parse_refs(&files)
            .await
            .expect("refs parse")
            .expect("HEAD is on a branch");
        let state = classify(&files, Some(&config), Some(&refs));
        (state, refs, started.elapsed())
    }

    /// Steps 1 of the module doc, on a `.git` the gate has put into the
    /// project: read, preflight, the worst-case gateway-object read and save.
    pub async fn before_import(&self, release: &Release, run_prefix: &str) -> Before {
        let label = self.label;
        let b = &self.budgets;
        let read = self.read();

        // read_git_state (the host read budget of the 27-06 table)
        let (state, refs, took) = self.classify_now().await;
        row(label, "read_git_state", took, b.read);
        assert_eq!(
            state.kind,
            GitStateKind::Managed,
            "the production-shape .git classifies as managed: {state:?}"
        );
        let head_before = head_commit(&refs);
        assert_ne!(
            head_before, release.commit,
            "the gateway repository sits behind the release"
        );
        let size = read.git_size(self.project).await.expect("size");
        note(
            label,
            &format!("shape: {} files, {} bytes", size.files, size.bytes),
        );

        // The release artifact and the prepared advance (ign's machine).
        let artifact = build_artifact(&release.checkout).expect("artifact builds");
        let members = artifact.digest.members.keys().cloned().collect();
        let started = Instant::now();
        let prep = prepare_advance(
            &HostGit::new(),
            &release.checkout,
            &state,
            &refs,
            &release.commit,
            &members,
        )
        .await
        .expect("prepare_advance");
        note(
            label,
            &format!(
                "prepare_advance: {} ms on ign's machine (pack {} bytes)",
                started.elapsed().as_millis(),
                prep.pack
                    .as_ref()
                    .map(|p| p.pack.len() + p.idx.len())
                    .unwrap_or(0)
            ),
        );

        // The write preflight as the run's `git_preflight` runs it: the probe,
        // the size-only read and the free-space read (checker W-5).
        let started = Instant::now();
        git_preflight_on(
            "nodes.master",
            self.writer,
            read,
            self.gateway,
            self.project,
            Some(&prep),
            &format!("{run_prefix}p"),
        )
        .await
        .unwrap_or_else(|err| panic!("the write preflight refused: {} ({err})", err.code()));
        row(label, "write_preflight", started.elapsed(), b.preflight);
        let free = read
            .free_bytes(self.gateway, self.project)
            .await
            .expect("free_bytes");
        note(label, &format!("free_bytes: {free}"));

        // The worst case of D-24: ign's own store is empty, so every pack
        // holding an object of the HEAD's tree is fetched.
        let empty = tempfile::tempdir().expect("empty store");
        host_git(empty.path(), &["init", "-q"]);
        let timed = TimedRead {
            inner: read,
            spent: Mutex::new(Duration::ZERO),
            fetches: Mutex::new((0, 0, 0)),
        };
        let started = Instant::now();
        let (digest, stats) = gateway_head_digest(
            &HostGit::new(),
            empty.path(),
            &timed,
            self.project,
            &head_before,
        )
        .await
        .unwrap_or_else(|failure| panic!("the gateway's objects supply HEAD: {failure:?}"));
        let total = started.elapsed();
        let objects = *timed.spent.lock().expect("lock");
        let (fetches, files, bytes) = *timed.fetches.lock().expect("lock");
        note(
            label,
            &format!(
                "gateway objects: {} loose, {} idx, {} packs, {} bytes in {fetches} fetches of {files} files ({bytes} bytes), {} members",
                stats.loose,
                stats.idx,
                stats.packs,
                stats.bytes,
                digest.members.len()
            ),
        );
        row(
            label,
            "gateway_objects (list plus fetch)",
            objects,
            b.objects,
        );
        row(
            label,
            "gateway_objects (resolve and digest)",
            total.saturating_sub(objects),
            b.digest,
        );
        assert!(stats.loose + stats.packs > 0, "something was fetched");

        // save, and the save's manifest equals the repository's.
        let manifest = read.git_manifest(self.project).await.expect("manifest");
        let timed_writer = TimedWriter::new(self.writer);
        let saved = save(
            read,
            &timed_writer,
            LedgerRoot::AccountHome,
            self.gateway,
            self.project,
            self.run,
        )
        .await
        .unwrap_or_else(|err| panic!("save: {} ({err})", err.code()));
        row(
            label,
            "save_git",
            timed_writer.span("save").expect("saved"),
            b.save,
        );
        assert_eq!(saved.manifest, manifest, "the save equals the gateway .git");
        note(
            label,
            &format!(
                "save: {} files, {} bytes, digest {}",
                saved.manifest.files,
                saved.manifest.bytes,
                &saved.manifest.digest[..12.min(saved.manifest.digest.len())]
            ),
        );
        Before {
            state,
            saved,
            prep,
            artifact_zip: artifact.zip,
            manifest,
            head_before,
        }
    }

    /// Step 3: `.git` is gone (the gate removed it as the import does); the
    /// shipped `restore_and_advance` restores it, compares the manifest and
    /// advances it to the release, each against its budget.
    pub async fn after_import(&self, release: &Release, before: &Before) -> GitRestoreReport {
        let label = self.label;
        let b = &self.budgets;
        let read = self.read();
        let gone = read
            .read_git_state(self.project)
            .await
            .expect("state after the removal");
        assert_eq!(gone.kind, GitEntry::Absent, ".git is gone");

        let timed_writer = TimedWriter::new(self.writer);
        let report = restore_and_advance(
            read,
            &timed_writer,
            &HostGit::new(),
            &release.checkout,
            &before.state,
            &before.saved,
            &before.prep,
            &before.artifact_zip,
            None,
        )
        .await
        .unwrap_or_else(|failure| {
            panic!(
                "restore_and_advance failed: reason={} paths={} detail={}",
                failure.reason,
                failure.paths.len(),
                failure.detail
            )
        });
        assert!(report.restored, "the run restored .git");
        assert!(report.advanced, "the run advanced .git");
        assert!(report.verified, "every written file read back equal");

        // restore plus one manifest compare (the budget names both)
        let restore = timed_writer.span("restore").expect("restored");
        let started = Instant::now();
        // The advance has changed the repository since, so the compare is
        // against the save through the save's own manifest read.
        let saved_again = read
            .save_manifest(self.gateway, self.project, self.run)
            .await
            .expect("save manifest");
        let compare = started.elapsed();
        assert_eq!(
            saved_again, before.manifest,
            "the save still equals the manifest taken before the import"
        );
        row(
            label,
            "restore_git plus manifest compare",
            restore + compare,
            b.restore_verify,
        );
        row(
            label,
            "advance upload (write_git_files)",
            timed_writer.span("upload").expect("uploaded"),
            b.upload,
        );

        // read back HEAD and the branch: the advance is what the report says.
        let (state, refs, took) = self.classify_now().await;
        row(label, "read_git_state after the advance", took, b.read);
        assert_eq!(state.kind, GitStateKind::Managed, "{state:?}");
        assert_eq!(head_commit(&refs), release.commit, "HEAD is the release");
        note(
            label,
            &format!(
                "advanced: HEAD equals the release, restored={} advanced={} verified={} dropped={} expected_drift={}",
                report.restored,
                report.advanced,
                report.verified,
                report.dropped.len(),
                report.expected_drift.len()
            ),
        );
        report
    }

    /// The save is removed and gone.
    pub async fn remove_save(&self) {
        self.writer
            .remove_git_save(self.gateway, self.project, self.run)
            .await
            .expect("remove_git_save");
        self.writer
            .remove_git_save(self.gateway, self.project, self.run)
            .await
            .expect("an absent save is a success");
    }
}
