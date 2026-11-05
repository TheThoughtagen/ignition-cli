//! Production-scale gate for deploy transport changes (quick 261104-s3n).
//!
//! `tests/common/prod_shape.rs` generates the shape of a production pair's
//! core tag folders (14,119 + 156 files, 7,500 + 85 folders, depth 14, fanout
//! 89, 7 paths of 260 characters or more). That fixture and the tests on it
//! are the gate for any deploy transport change:
//!
//! - the offline connection-count tests in `src/deploy/transport/sftp.rs`
//!   (`windows_prod_shape_*`): a Windows read of this tree opens a fixed
//!   number of ssh connections, the same as for a tiny tree;
//! - this file: the generator holds the recorded numbers, and a full read of
//!   the materialized tree finishes inside a measured wall-clock budget
//!   (the local reader, the POSIX sftp reader over the local `sftp-server`,
//!   the Windows walk script under the local `pwsh`, and on Windows the
//!   Windows sftp reader over `sftp-server.exe`);
//! - the live `wflwvm01_prod_shape_*` gate in `tests/live_transport_wflwvm01.rs`.

mod common;

use std::collections::BTreeMap;
#[cfg(unix)]
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::prod_shape::{self, Bytes, Entry, PROVIDER};
use ignition_core::deploy::layout::RelPath;
use ignition_core::deploy::transport::local::LocalFsTransport;
use ignition_core::deploy::transport::remote::safe_remote_path;
use ignition_core::deploy::transport::winps::windows_component_problem;
use ignition_core::deploy::transport::{ObservedFiles, Scope, TreeListing, TreeReader};

fn files(entries: &[Entry]) -> usize {
    entries.iter().filter(|e| !e.dir).count()
}

fn dirs(entries: &[Entry]) -> usize {
    entries.iter().filter(|e| e.dir).count()
}

fn bytes(entries: &[Entry]) -> u64 {
    entries.iter().map(|e| e.size).sum()
}

fn within_2_percent(got: u64, want: u64) -> bool {
    got.abs_diff(want) * 50 <= want
}

/// Every component passes both name gates, the first is the provider, and
/// no two siblings are equal ignoring case.
fn assert_names(entries: &[Entry]) {
    let mut folded = std::collections::BTreeSet::new();
    for entry in entries {
        assert_eq!(entry.rel.split('/').next(), Some(PROVIDER), "{}", entry.rel);
        for component in entry.rel.split('/') {
            assert!(component.len() <= 255, "{component}");
            assert_eq!(windows_component_problem(component), None, "{}", entry.rel);
            safe_remote_path(component).expect(&entry.rel);
        }
        assert!(
            folded.insert(entry.rel.to_lowercase()),
            "{} is listed twice ignoring case",
            entry.rel
        );
    }
    // sorted by path bytes, so a parent comes before its children
    assert!(
        entries
            .windows(2)
            .all(|w| w[0].rel.as_bytes() < w[1].rel.as_bytes())
    );
}

#[test]
fn prod_shape_matches_the_recorded_production_numbers() {
    let td = prod_shape::tag_definition(Bytes::Small);
    assert_eq!(files(&td), prod_shape::TAG_DEFINITION_FILES);
    assert_eq!(dirs(&td), prod_shape::TAG_DEFINITION_DIRS);
    assert_eq!(prod_shape::depth(&td), prod_shape::TAG_DEFINITION_DEPTH);
    let fan = prod_shape::fanouts(&td);
    let widest = fan.values().copied().max().unwrap_or(0);
    assert_eq!(widest, prod_shape::TAG_DEFINITION_FANOUT);
    assert_eq!(
        fan.values().filter(|n| **n == widest).count(),
        1,
        "one folder with the widest fanout"
    );
    let lengths: Vec<usize> = td
        .iter()
        .map(|e| prod_shape::windows_full_len("core", "tag-definition", &e.rel))
        .collect();
    let long = lengths.iter().filter(|n| **n >= 260).count();
    assert!(long >= prod_shape::TAG_DEFINITION_LONG, "{long}");
    assert_eq!(
        lengths.iter().copied().max(),
        Some(prod_shape::TAG_DEFINITION_LONGEST)
    );
    assert_names(&td);

    let tt = prod_shape::tag_type_definition(Bytes::Small);
    assert_eq!(files(&tt), prod_shape::TAG_TYPE_FILES);
    assert_eq!(dirs(&tt), prod_shape::TAG_TYPE_DIRS);
    assert_eq!(prod_shape::depth(&tt), prod_shape::TAG_TYPE_DEPTH);
    assert_eq!(
        prod_shape::fanouts(&tt).values().copied().max(),
        Some(prod_shape::TAG_TYPE_FANOUT)
    );
    assert!(
        tt.iter()
            .all(|e| { prod_shape::windows_full_len("core", "tag-type-definition", &e.rel) < 260 })
    );
    assert_names(&tt);

    // production bytes land within 2% of the recorded totals
    let td_prod = prod_shape::tag_definition(Bytes::Prod);
    let tt_prod = prod_shape::tag_type_definition(Bytes::Prod);
    assert!(
        within_2_percent(bytes(&td_prod), prod_shape::TAG_DEFINITION_BYTES),
        "{}",
        bytes(&td_prod)
    );
    assert!(
        within_2_percent(bytes(&tt_prod), prod_shape::TAG_TYPE_BYTES),
        "{}",
        bytes(&tt_prod)
    );
    // the same paths whatever the byte profile
    let paths = |e: &[Entry]| e.iter().map(|e| (e.rel.clone(), e.dir)).collect::<Vec<_>>();
    assert_eq!(paths(&td), paths(&td_prod));
    assert!(td.iter().all(|e| e.dir || (16..=64).contains(&e.size)));

    // deterministic
    assert_eq!(td, prod_shape::tag_definition(Bytes::Small));
    assert_eq!(tt_prod, prod_shape::tag_type_definition(Bytes::Prod));

    // content is exactly the size, valid JSON, and a function of the path
    for entry in td_prod.iter().chain(&td).filter(|e| !e.dir).step_by(97) {
        let body = prod_shape::content(&entry.rel, entry.size);
        assert_eq!(body.len() as u64, entry.size, "{}", entry.rel);
        serde_json::from_slice::<serde_json::Value>(&body).expect(&entry.rel);
        assert_eq!(body, prod_shape::content(&entry.rel, entry.size));
    }

    let tiny = prod_shape::tiny_tag_definition();
    assert!(tiny.len() <= 20 && prod_shape::depth(&tiny) <= 3);
    assert_names(&tiny);
}

// ---- timed production-scale reads (BRIEF test 3) ----------------------------
//
// Each row materializes the two generator trees with production bytes into a
// fresh gateway data directory (`config/resources/production` plus
// `config/resources/core/ignition/<type>`), runs a full read through one
// host, asserts the result equals the generator, and asserts the wall-clock
// time against its budget. The rows hold `TIMED` so they never time each
// other's load. Materializing the tree is not timed.

/// Serializes the timed rows (an async mutex: the rows hold it across their
/// reads).
static TIMED: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const TYPES: [&str; 2] = ["tag-definition", "tag-type-definition"];

/// The generator trees with production bytes, in [`TYPES`] order.
fn prod_trees() -> [Vec<Entry>; 2] {
    [
        prod_shape::tag_definition(Bytes::Prod),
        prod_shape::tag_type_definition(Bytes::Prod),
    ]
}

/// A gateway data directory holding the production-shape core tag trees.
fn prod_data_dir(trees: &[Vec<Entry>; 2]) -> tempfile::TempDir {
    let data = tempfile::tempdir().expect("tempdir");
    let resources = data.path().join("config").join("resources");
    std::fs::create_dir_all(resources.join("production")).expect("mode collection");
    for (ty, entries) in TYPES.iter().zip(trees) {
        prod_shape::materialize(&resources.join("core").join("ignition").join(ty), entries);
    }
    data
}

fn type_dirs() -> [(String, String); 2] {
    TYPES.map(|ty| ("ignition".to_string(), ty.to_string()))
}

fn prod_scope() -> Scope {
    Scope {
        mode: "production".to_string(),
        types: Vec::new(),
        tag_providers: vec![PROVIDER.to_string()],
    }
}

/// `list_tree` returned every generated entry of both types.
fn assert_listing(listed: &[TreeListing], trees: &[Vec<Entry>; 2]) {
    let types: Vec<&str> = listed.iter().map(|l| l.ty.as_str()).collect();
    assert_eq!(types, TYPES);
    for (listing, (ty, entries)) in listed.iter().zip(TYPES.iter().zip(trees)) {
        let got: Vec<(String, bool)> = listing
            .entries
            .iter()
            .map(|e| (e.path.as_str().to_string(), e.dir))
            .collect();
        let mut want: Vec<(String, bool)> = entries
            .iter()
            .map(|e| (format!("ignition/{ty}/{}", e.rel), e.dir))
            .collect();
        want.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        assert_eq!(got.len(), want.len(), "{ty}");
        assert!(got == want, "{ty}: the listing differs from the generator");
    }
}

/// `read` returned every generated file whose host path is under `long_at`
/// characters with its bytes, and every other one as unreadable `core` with
/// its length (`len_of` measures the host path). Nothing in the mode or
/// local collection.
fn assert_read(
    observed: &ObservedFiles,
    trees: &[Vec<Entry>; 2],
    long_at: usize,
    len_of: &dyn Fn(&str, &str) -> usize,
) {
    let mut want: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut want_unreadable: Vec<(String, usize)> = Vec::new();
    for (ty, entries) in TYPES.iter().zip(trees) {
        for entry in entries.iter().filter(|e| !e.dir) {
            let key = format!("ignition/{ty}/{}", entry.rel);
            let length = len_of(ty, &entry.rel);
            if length < long_at {
                want.insert(key, prod_shape::content(&entry.rel, entry.size));
            } else {
                want_unreadable.push((key, length));
            }
        }
    }
    assert_eq!(observed.core.len(), want.len());
    for (key, bytes) in &want {
        let got = observed
            .core
            .get(&RelPath::new(key).expect("rel"))
            .unwrap_or_else(|| panic!("{key} was not read"));
        assert!(got.bytes == *bytes, "{key}: the bytes differ");
    }
    let mut got_unreadable: Vec<(String, usize)> = observed
        .unreadable
        .iter()
        .map(|u| {
            assert_eq!(u.collection, "core");
            (u.path.as_str().to_string(), u.length)
        })
        .collect();
    got_unreadable.sort();
    want_unreadable.sort();
    assert_eq!(got_unreadable, want_unreadable);
    assert!(observed.mode.is_empty() && observed.local_units.is_empty());
}

/// Await `body` and return its result and how long it took.
async fn timed<T>(body: impl std::future::Future<Output = T>) -> (T, Duration) {
    let started = Instant::now();
    let out = body.await;
    (out, started.elapsed())
}

fn report(row: &str, what: &str, elapsed: Duration, budget: Duration, extra: &str) {
    eprintln!(
        "{row}: {what} took {:.2} s (budget {} s){extra}",
        elapsed.as_secs_f64(),
        budget.as_secs()
    );
    assert!(
        elapsed <= budget,
        "{row}: {what} took {:.2} s, over its {} s budget",
        elapsed.as_secs_f64(),
        budget.as_secs()
    );
}

/// The local reader at production scale.
///
/// Budget, per operation: measured 2026-11-04 on an Apple M1 Max (10 cores),
/// debug profile (`cargo test`), three runs: `list_tree` 0.48 / 0.36 / 0.49 s,
/// `read` 7.59 / 6.29 / 7.99 s. 25 s is about three times the slowest run:
/// a CI runner that is twice as slow still passes, while one extra pass over
/// the tree per folder, or a quadratic step at this shape (21,860 entries),
/// costs minutes and fails it.
const BUDGET_LOCAL: Duration = Duration::from_secs(25);

#[tokio::test]
async fn prod_scale_local_read() {
    let trees = prod_trees();
    let data = prod_data_dir(&trees);
    let _guard = TIMED.lock().await;
    let reader = LocalFsTransport::new(data.path().to_path_buf(), "nodes.master");
    let (listed, list_time) = timed(reader.list_tree("core", &type_dirs())).await;
    let (observed, read_time) = timed(reader.read(&prod_scope())).await;
    assert_listing(&listed.expect("list_tree"), &trees);
    assert_read(&observed.expect("read"), &trees, usize::MAX, &|_, _| 0);
    report(
        "prod_scale_local_read",
        "list_tree",
        list_time,
        BUDGET_LOCAL,
        "",
    );
    report("prod_scale_local_read", "read", read_time, BUDGET_LOCAL, "");
}

/// The POSIX sftp reader over the local `sftp-server` at production scale:
/// one `ls` session per tree level (POSIX hosts keep the per-level listing)
/// and one `get` per file, in chunks.
///
/// Budget, per operation: measured 2026-11-04 on an Apple M1 Max (10 cores),
/// debug profile, three runs: `list_tree` 2.06 / 2.63 / 2.20 s (19
/// sessions), `read` 19.30 / 17.24 / 19.39 s (21 more: the levels again and
/// two download chunks). 60 s is about three times the slowest run. A
/// listing that cost one session per folder (7,585 folders, about 20 ms each
/// even to a local server) or a quadratic parse of the 21,860 listing lines
/// fails it.
#[cfg(unix)]
const BUDGET_POSIX_SFTP: Duration = Duration::from_secs(60);

#[cfg(unix)]
#[tokio::test]
async fn prod_scale_posix_sftp_read() {
    use ignition_core::deploy::transport::process::TokioProcessRunner;
    use ignition_core::deploy::transport::remote::RemoteShell;
    use ignition_core::deploy::transport::sftp::{SftpTransport, posix_root};
    let trees = prod_trees();
    let data = prod_data_dir(&trees);
    let _guard = TIMED.lock().await;
    let shell = RemoteShell::local_sftp_server(
        &common::sftp_server::sftp_server_path(),
        Arc::new(TokioProcessRunner),
        Duration::from_secs(120),
    );
    let root = posix_root(&data.path().display().to_string()).expect("posix data dir");
    let reader = SftpTransport::new(shell, root, "nodes.master");
    let (listed, list_time) = timed(reader.list_tree("core", &type_dirs())).await;
    let list_sessions = reader.sessions();
    let (observed, read_time) = timed(reader.read(&prod_scope())).await;
    let observed = observed.expect("read");
    assert_listing(&listed.expect("list_tree"), &trees);
    assert_read(&observed, &trees, usize::MAX, &|_, _| 0);
    // the same files as the local reader
    let local = LocalFsTransport::new(data.path().to_path_buf(), "nodes.master")
        .read(&prod_scope())
        .await
        .expect("local read");
    assert!(
        observed == local,
        "the sftp read differs from the local read"
    );
    let sessions = format!(
        ", {list_sessions} sessions for list_tree, {} for list_tree + read",
        reader.sessions()
    );
    report(
        "prod_scale_posix_sftp_read",
        "list_tree",
        list_time,
        BUDGET_POSIX_SFTP,
        &sessions,
    );
    report(
        "prod_scale_posix_sftp_read",
        "read",
        read_time,
        BUDGET_POSIX_SFTP,
        &sessions,
    );
}

/// The Windows walk script (`winps::ENUMERATE`) run by the local `pwsh` over
/// the materialized `tag-definition` tree: the script logic and its speed at
/// this shape, never skipped (a runner without PowerShell fails here, as the
/// winps script-logic tests do).
///
/// Budget: measured 2026-11-04 on an Apple M1 Max with PowerShell 7, three
/// runs over the 21,619 `tag-definition` entries: 6.27 / 5.46 / 5.28 s
/// (1,511,045 bytes of stdout, about 70 bytes per entry against the 32 MiB
/// stdout cap). 20 s is about three times the slowest run. The script's own
/// cost is the part of a live Windows read that grows with the tree, so a
/// walk step that turned per-entry work quadratic, or a second walk per
/// folder, fails it.
#[cfg(unix)]
const BUDGET_PWSH: Duration = Duration::from_secs(20);

#[cfg(unix)]
fn powershell() -> std::path::PathBuf {
    for name in ["pwsh", "powershell"] {
        if let Some(paths) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&paths) {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }
    for fixed in [
        "/usr/local/bin/pwsh",
        "/opt/homebrew/bin/pwsh",
        "/usr/bin/pwsh",
    ] {
        if Path::new(fixed).is_file() {
            return fixed.into();
        }
    }
    panic!(
        "no pwsh on PATH: install PowerShell 7 (macOS: `brew install --cask powershell`; Debian/Ubuntu: the Microsoft packages) so the production-scale walk can run. This test is never skipped."
    );
}

#[cfg(unix)]
#[test]
fn prod_scale_pwsh_walk() {
    use ignition_core::deploy::transport::winps::{
        COMMAND_BUDGET, WinScript, encoded_command, parse_enumeration, parse_ign_result,
    };
    let trees = prod_trees();
    let data = prod_data_dir(&trees);
    let root = data
        .path()
        .join("config/resources/core/ignition/tag-definition")
        .display()
        .to_string();
    let command =
        encoded_command(&WinScript::enumerate("", &[root]), COMMAND_BUDGET).expect("command");
    let encoded = command
        .as_str()
        .strip_prefix("powershell -NoProfile -NonInteractive -EncodedCommand ")
        .expect("prefix")
        .to_string();
    let pwsh = powershell();
    let _guard = TIMED.blocking_lock();
    let started = Instant::now();
    let out = std::process::Command::new(pwsh)
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run pwsh");
    let elapsed = started.elapsed();
    assert!(out.status.success(), "{out:?}");
    let value = parse_ign_result(&out.stdout).expect("sentinel");
    let walked = parse_enumeration(&value, 1)
        .expect("enumeration")
        .pop()
        .flatten()
        .expect("the root exists");
    let mut got: Vec<(String, bool, u64)> =
        walked.into_iter().map(|e| (e.rel, e.dir, e.size)).collect();
    got.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let want: Vec<(String, bool, u64)> = trees[0]
        .iter()
        .map(|e| (e.rel.clone(), e.dir, e.size))
        .collect();
    assert_eq!(got.len(), want.len());
    assert!(got == want, "the walk differs from the generator");
    report(
        "prod_scale_pwsh_walk",
        "ENUMERATE of tag-definition",
        elapsed,
        BUDGET_PWSH,
        &format!(", {} bytes of stdout", out.stdout.len()),
    );
}

/// The Windows sftp reader over the runner's `sftp-server.exe` and the local
/// PowerShell (the conformance `ps_host` seam) at production scale: the
/// connection bounds of the offline `windows_prod_shape_*` tests and the
/// 120 s live target. Unmeasured until Windows CI runs it.
#[cfg(windows)]
const BUDGET_WINDOWS_SFTP: Duration = Duration::from_secs(120);

#[cfg(windows)]
#[tokio::test]
async fn prod_scale_windows_sftp_read() {
    use ignition_core::deploy::transport::process::TokioProcessRunner;
    use ignition_core::deploy::transport::remote::RemoteShell;
    use ignition_core::deploy::transport::sftp::{SftpTransport, windows_sftp_root};
    use ignition_core::deploy::transport::winps::LONG_PATH_LIMIT;
    let trees = prod_trees();
    let data = prod_data_dir(&trees);
    let _guard = TIMED.lock().await;
    let shell = RemoteShell::local_sftp_server(
        &common::sftp_server::sftp_server_path(),
        Arc::new(TokioProcessRunner),
        Duration::from_secs(120),
    )
    .local_exec();
    // the long name of the temp dir (`RUNNER~1` expanded), as the walk
    // measures it
    let canonical = std::fs::canonicalize(data.path()).expect("canonical data dir");
    let native = canonical.display().to_string();
    let native = native.strip_prefix(r"\\?\").unwrap_or(&native).to_string();
    let root = windows_sftp_root(&native).expect("windows sftp root");
    let reader = SftpTransport::new_windows(shell, root, "nodes.ps");
    let (listed, list_time) = timed(reader.list_tree("core", &type_dirs())).await;
    let list_sessions = reader.sessions();
    let (observed, read_time) = timed(reader.read(&prod_scope())).await;
    assert_listing(&listed.expect("list_tree"), &trees);
    let len_of = |ty: &str, rel: &str| {
        format!(
            "{native}\\config\\resources\\core\\ignition\\{ty}\\{}",
            rel.replace('/', "\\")
        )
        .chars()
        .count()
    };
    assert_read(&observed.expect("read"), &trees, LONG_PATH_LIMIT, &len_of);
    assert!(list_sessions <= 3, "list_tree opened {list_sessions}");
    let sessions = format!(", {list_sessions} connections for list_tree");
    report(
        "prod_scale_windows_sftp_read",
        "list_tree",
        list_time,
        BUDGET_WINDOWS_SFTP,
        &sessions,
    );
    report(
        "prod_scale_windows_sftp_read",
        "read",
        read_time,
        BUDGET_WINDOWS_SFTP,
        &sessions,
    );
}
