//! Production-scale gate for deploy transport changes (quick 261104-s3n).
//!
//! `tests/common/prod_shape.rs` generates the shape of a production pair's
//! core tag folders (14,119 + 156 files, 7,500 + 85 folders, depth 14, fanout
//! 89, 7 paths of 260 characters or more). That fixture and the tests on it
//! are the gate for any deploy transport change:
//!
//! - the offline connection-count tests in `src/deploy/transport/sftp.rs`
//!   (`windows_prod_shape_*`): a Windows read of this tree opens a fixed
//!   number of ssh connections, the same as for a tiny tree; and since quick
//!   261105-dl1 the bundled-download tests there (`windows_bundled_*`): the
//!   read moves its files, about 48 MB with production bytes, with one sftp
//!   `get` of one archive, and refuses every hostile archive;
//! - this file: the generator holds the recorded numbers, and a full read of
//!   the materialized tree finishes inside a measured wall-clock budget
//!   (the local reader, the POSIX sftp reader over the local `sftp-server`,
//!   the Windows walk script under the local `pwsh`, and on Windows the
//!   Windows sftp reader over `sftp-server.exe`);
//! - the live `wincap_prod_shape_*` gate in `tests/live_transport_wincap.rs`.
//!
//! The file also holds `prod_scale_missing_tags` (quick 261106-bt0): the check
//! that reports the tags a project uses and the target lacks, over a
//! 43,000-tag provider and a 2,000-file project, through the real client
//! against a mock gateway. It costs one provider list and one export, returns
//! the exact planted set and finishes inside its budget.

mod common;

use std::collections::BTreeMap;
#[cfg(unix)]
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::native_shape;
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

/// Quick 261105-dl1: a Windows read now moves its files as one Deflate
/// archive, so the generated content must compress like real data, not like
/// a repeated id. The `Bytes::Prod` tag-definition files, zipped in memory
/// with the zip crate's default Deflate, must shrink between 2x and 6x: real
/// tag JSON (repeated keys) shrinks more, so a transfer measured on this
/// content is at or above production volume. Every content is valid JSON of
/// exactly its size and a function of its path.
#[test]
fn prod_shape_content_deflates_like_real_volume() {
    use std::io::Write as _;
    // about 10 s of Deflate work: never during a timed row
    let _guard = TIMED.blocking_lock();
    let td = prod_shape::tag_definition(Bytes::Prod);
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut raw = 0u64;
    for entry in td.iter().filter(|e| !e.dir) {
        let body = prod_shape::content(&entry.rel, entry.size);
        assert_eq!(body.len() as u64, entry.size, "{}", entry.rel);
        serde_json::from_slice::<serde_json::Value>(&body).expect(&entry.rel);
        assert_eq!(body, prod_shape::content(&entry.rel, entry.size));
        writer
            .start_file(entry.rel.as_str(), options)
            .expect("entry");
        writer.write_all(&body).expect("write");
        raw += entry.size;
    }
    let packed = writer.finish().expect("finish").into_inner().len() as u64;
    let ratio = raw as f64 / packed as f64;
    eprintln!(
        "prod_shape_content_deflates_like_real_volume: {raw} bytes deflate to {packed} bytes, ratio {ratio:.2}"
    );
    assert!(
        (2.0..=6.0).contains(&ratio),
        "the Prod content deflates {ratio:.2}x, outside 2x to 6x"
    );
    // the small profile is valid JSON of its size too
    for entry in prod_shape::tag_definition(Bytes::Small)
        .iter()
        .filter(|e| !e.dir)
        .step_by(53)
    {
        let body = prod_shape::content(&entry.rel, entry.size);
        assert_eq!(body.len() as u64, entry.size);
        serde_json::from_slice::<serde_json::Value>(&body).expect(&entry.rel);
    }
}

// ---- timed production-scale reads (BRIEF test 3) ----------------------------
//
// Each row materializes the two generator trees with production bytes into a
// fresh gateway data directory (`config/resources/production` plus
// `config/resources/core/ignition/<type>`), runs a full read through one
// host, asserts the result equals the generator, and asserts the wall-clock
// time against its budget. Materializing the tree is not timed. Each row,
// and the deflate-ratio test, holds `TIMED` for its whole run, materializing
// and checking included, so no row's timed region overlaps another test's
// 50 MB of work (quick 261105-dl1: with the bundle row and the ratio test
// added, a `cargo test --workspace` run timed `prod_scale_local_read`'s read
// at 80 s against 9 s alone).

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
    let _guard = TIMED.lock().await;
    let trees = prod_trees();
    let data = prod_data_dir(&trees);
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
    let _guard = TIMED.lock().await;
    let trees = prod_trees();
    let data = prod_data_dir(&trees);
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
    let _guard = TIMED.blocking_lock();
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

/// The bundled Windows download (quick 261105-dl1) at production scale: the
/// real `winps::BUNDLE` script under the local `pwsh` zips every generated
/// file of both trees from an uploaded list, then `bundle::extract` unpacks
/// the archive. Never skipped, like `prod_scale_pwsh_walk`.
///
/// Budgets: measured 2026-11-05 on an Apple M1 Max (10 cores) with
/// PowerShell 7, debug profile, three runs over the 14,275 files (51.5 MB,
/// a 20.2 MB archive, ratio 2.55): BUNDLE 13.35 / 8.59 / 2.98 s, extraction
/// 7.59 / 5.74 / 4.86 s. Each budget is about three times its slowest run.
/// The regression classes they catch cost far more: a per-file transfer
/// (about 0.37 s per file on a Windows server, 88 minutes here), a second
/// pass over the archive per entry, or a script loop that turns quadratic in
/// the file count (14,275 squared steps) all take minutes, while a CI runner
/// two to three times slower than this Mac still passes.
#[cfg(unix)]
const BUDGET_PWSH_BUNDLE: Duration = Duration::from_secs(40);
#[cfg(unix)]
const BUDGET_EXTRACT: Duration = Duration::from_secs(25);

#[cfg(unix)]
#[test]
fn prod_scale_pwsh_bundle() {
    use ignition_core::deploy::transport::bundle::{self, BundleFile};
    use ignition_core::deploy::transport::winps::{
        COMMAND_BUDGET, WinScript, encoded_command, parse_ign_result,
    };
    let _guard = TIMED.blocking_lock();
    let trees = prod_trees();
    let data = prod_data_dir(&trees);
    let resources = data.path().join("config").join("resources");
    let files: Vec<BundleFile> = TYPES
        .iter()
        .zip(&trees)
        .flat_map(|(ty, entries)| {
            entries.iter().filter(|e| !e.dir).map(move |e| BundleFile {
                collection: "core".to_string(),
                rel: format!("ignition/{ty}/{}", e.rel),
                size: e.size,
            })
        })
        .collect();
    let listed_bytes: u64 = files.iter().map(|f| f.size).sum();
    let stage = tempfile::tempdir().expect("stage");
    let list = bundle::render_list(&files);
    let list_path = stage.path().join(bundle::LIST_FILE);
    std::fs::write(&list_path, &list).expect("list");
    let archive = stage.path().join(bundle::ARCHIVE_FILE);
    let script = WinScript::bundle(
        "",
        &resources.display().to_string(),
        &list_path.display().to_string(),
        &archive.display().to_string(),
        &ignition_core::deploy::tree::sha256_hex(&list),
        bundle::archive_limit(&files) + bundle::STAGING_FREE_MARGIN,
    );
    let command = encoded_command(&script, COMMAND_BUDGET).expect("command");
    let encoded = command
        .as_str()
        .strip_prefix("powershell -NoProfile -NonInteractive -EncodedCommand ")
        .expect("prefix")
        .to_string();
    let pwsh = powershell();
    let started = Instant::now();
    let out = std::process::Command::new(pwsh)
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run pwsh");
    let build = started.elapsed();
    assert!(out.status.success(), "{out:?}");
    let value = parse_ign_result(&out.stdout).expect("sentinel");
    assert_eq!(value["ok"], 1, "{value}");
    assert_eq!(value["n"].as_u64(), Some(files.len() as u64), "{value}");
    let z = std::fs::metadata(&archive).expect("archive").len();
    assert_eq!(value["z"].as_u64(), Some(z), "{value}");
    assert!(z <= bundle::archive_limit(&files));
    assert!(!list_path.exists(), "BUNDLE removes the list");

    let dest = tempfile::tempdir().expect("dest");
    let started = Instant::now();
    let written = bundle::extract(&archive, &files, dest.path(), 1024 * 1024).expect("extract");
    let extract = started.elapsed();

    // identity, outside the timed regions
    assert_eq!(written, listed_bytes);
    for (ty, entries) in TYPES.iter().zip(&trees) {
        for entry in entries.iter().filter(|e| !e.dir) {
            let path = format!("core/ignition/{ty}/{}", entry.rel)
                .split('/')
                .fold(dest.path().to_path_buf(), |p, c| p.join(c));
            let got = std::fs::read(&path).expect(&entry.rel);
            assert!(
                got == prod_shape::content(&entry.rel, entry.size),
                "{}: the bytes differ",
                entry.rel
            );
        }
    }
    let ratio = listed_bytes as f64 / z as f64;
    let extra = format!(
        ", {} files, {listed_bytes} bytes in a {z} byte archive (ratio {ratio:.2})",
        files.len()
    );
    report(
        "prod_scale_pwsh_bundle",
        "BUNDLE of both trees",
        build,
        BUDGET_PWSH_BUNDLE,
        &extra,
    );
    report(
        "prod_scale_pwsh_bundle",
        "extract",
        extract,
        BUDGET_EXTRACT,
        &extra,
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
    let _guard = TIMED.lock().await;
    let trees = prod_trees();
    let data = prod_data_dir(&trees);
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
    // quick 261105-dl1: the `.ign-ctl-*` entries of the account home before
    // the read, which stages its archive there
    let home = reader.account_home().await.expect("account home");
    let home = home.trim_start_matches('/').replace('/', "\\");
    let ctl_entries = || -> std::collections::BTreeSet<String> {
        std::fs::read_dir(&home)
            .expect("the account home")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".ign-ctl-"))
            .collect()
    };
    let ctl_before = ctl_entries();
    let before_read = reader.sessions();
    let (observed, read_time) = timed(reader.read(&prod_scope())).await;
    let read_sessions = reader.sessions() - before_read;
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
    // the read fetched its files as one archive staged below the account
    // home: a fixed number of connections, nothing left behind
    assert!(read_sessions <= 8, "read opened {read_sessions}");
    assert_eq!(
        ctl_entries(),
        ctl_before,
        "the read left a staging dir in {home}"
    );
    let sessions = format!(", {list_sessions} connections for list_tree, {read_sessions} for read");
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

// ---- tags a project uses (quick 261106-bt0) ---------------------------------

/// The check of the tags a project uses against a production-size provider.
///
/// Budget: measured 2026-11-06 on an Apple M1 Max (10 cores), debug profile
/// (`cargo test`), three runs: 0.67 / 0.68 / 0.66 s for the scan of 2,000
/// files, one provider list, one 47.8 MB export (48,951 tags) and the lookup
/// of every reference. The machine was under very heavy load throughout (load
/// average 62 to 64 on 10 cores), so these are not best-case figures. Later
/// runs measured up to 0.70 s. 8 s leaves room for CI runners such as
/// windows-latest, which are much slower than this machine. A request per tag
/// or per reference (the row asserts exactly two requests), a second parse of
/// the export per provider reference, or a quadratic step at this shape
/// (48,951 nodes, about 10,000 references) still costs many multiples of it
/// and fails.
const BUDGET_MISSING_TAGS: Duration = Duration::from_secs(8);

fn missing_tags_gateway_paths() -> (&'static str, &'static str) {
    (
        "/data/api/v1/resources/list/ignition/tag-provider",
        "/data/api/v1/tags/export",
    )
}

/// A provider of 43,000 tags exported as one document and a project of 2,000
/// code files. The check must cost one provider list and one export, report
/// exactly the planted absent paths and the one planted absent provider, and
/// never report an inherited member, a runtime-built path, a comment, a
/// docstring or a `__tests__` member.
#[tokio::test]
async fn prod_scale_missing_tags() {
    use ignition_core::actions::deploy_projects::{GatewayProjects, check_tag_references};
    use ignition_core::client::ReqwestGatewayApi;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let _guard = TIMED.lock().await;
    let mut export = prod_shape::tag_export();
    let project = prod_shape::project_zip(&export);
    assert!(
        export.tags >= prod_shape::TAG_EXPORT_TAGS,
        "{} tags",
        export.tags
    );
    assert!(export.json.len() as u64 >= prod_shape::TAG_DEFINITION_BYTES);
    assert!(project.code_files >= prod_shape::PROJECT_CODE_FILES);
    let (tags, export_bytes) = (export.tags, export.json.len());
    let body = std::mem::take(&mut export.json);

    let (list_path, export_path) = missing_tags_gateway_paths();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(list_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [
                {"name": PROVIDER, "enabled": true},
                {"name": "System", "enabled": true}
            ],
            "metadata": {"total": 2, "matching": 2, "limit": -1, "offset": 0}
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(export_path))
        .and(query_param("provider", PROVIDER))
        .and(query_param("type", "json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
        .mount(&server)
        .await;

    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let excluded = vec!["System".to_string()];
    let (check, elapsed) = timed(check_tag_references(
        &project.zip,
        &excluded,
        &GatewayProjects(&api),
    ))
    .await;

    assert_eq!(check.unavailable, [], "every read was made");
    let missing: Vec<String> = check
        .missing
        .expect("the tags were read")
        .into_iter()
        .map(|m| m.path)
        .collect();
    assert_eq!(missing, project.planted_missing);
    let unknown = check.unknown_providers.expect("the providers were listed");
    assert_eq!(unknown.len(), 1, "{unknown:?}");
    assert_eq!(unknown[0].provider, prod_shape::ABSENT_PROVIDER);
    assert_eq!(unknown[0].path_count, prod_shape::PLANTED_PROVIDER_PATHS);

    let seen: Vec<(String, String)> = server
        .received_requests()
        .await
        .expect("request recording is on")
        .iter()
        .map(|r| (r.method.to_string(), r.url.path().to_string()))
        .collect();
    assert_eq!(
        seen,
        [
            ("GET".to_string(), list_path.to_string()),
            ("GET".to_string(), export_path.to_string()),
        ],
        "exactly one provider list and one export, and nothing else"
    );
    report(
        "prod_scale_missing_tags",
        "check_tag_references",
        elapsed,
        BUDGET_MISSING_TAGS,
        &format!(
            ", {tags} tags in {export_bytes} bytes, {} code files",
            project.code_files
        ),
    );
}

#[test]
fn prod_shape_tag_export_holds_the_recorded_numbers() {
    let _guard = TIMED.blocking_lock();
    let export = prod_shape::tag_export();
    assert!(
        export.tags >= prod_shape::TAG_EXPORT_TAGS,
        "{}",
        export.tags
    );
    let bytes = export.json.len() as u64;
    assert!(bytes >= prod_shape::TAG_DEFINITION_BYTES, "{bytes}");
    assert!(
        bytes <= prod_shape::TAG_DEFINITION_BYTES + prod_shape::TAG_DEFINITION_BYTES / 10,
        "{bytes}"
    );
    assert!(!export.present.is_empty() && !export.inherited.is_empty());
    // deterministic
    assert!(
        export == prod_shape::tag_export(),
        "the export differs between runs"
    );

    let project = prod_shape::project_zip(&export);
    assert!(project.code_files >= prod_shape::PROJECT_CODE_FILES);
    assert_eq!(project.planted_missing.len(), prod_shape::PLANTED_MISSING);
    assert!(project.test_files > 0);
    assert_eq!(project.zip, prod_shape::project_zip(&export).zip);

    // every generated view is valid JSON, so the scan never falls back to
    // reading its text
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(project.zip.as_slice())).expect("zip");
    let mut views = 0;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).expect("entry");
        if entry.name().ends_with("/view.json") {
            let mut text = String::new();
            std::io::Read::read_to_string(&mut entry, &mut text).expect("view text");
            serde_json::from_str::<serde_json::Value>(&text).expect("a view is JSON");
            views += 1;
        }
    }
    assert_eq!(views, prod_shape::PROJECT_CODE_FILES - 1_700);
}

// ---- the native export of a production provider (plan 26-14) ---------------
//
// `tests/common/native_shape.rs` generates an owned provider in the git
// module's individual-file form with the measured shape of the production
// provider (TAGDEP-13). These tests hold the generator to the recorded
// numbers and to neutral names; the timed `project_tags_*` rows below run
// every offline stage of a tag deploy, and the whole tag step against the
// fake gateway, on it.

/// The recorded shape numbers, within 2% where the production figure is a
/// measurement and exactly where the generator decides it.
#[test]
fn native_shape_matches_the_recorded_production_numbers() {
    let _guard = TIMED.blocking_lock();
    let files = native_shape::native_export(1);
    let shape = native_shape::shape_of(&files, native_shape::PROVIDER);
    eprintln!("native_shape: {shape:?}");
    let near = |got: usize, want: usize| within_2_percent(got as u64, want as u64);
    assert!(
        near(shape.provider_files, native_shape::PROVIDER_FILES),
        "{} provider files",
        shape.provider_files
    );
    assert!(
        near(shape.top_folder_files, native_shape::TOP_FOLDER_FILES),
        "{} files under the top folder",
        shape.top_folder_files
    );
    assert_eq!(shape.type_files, native_shape::TYPE_FILES);
    assert!(
        near(shape.member_stubs, native_shape::MEMBER_STUBS),
        "{} member stubs",
        shape.member_stubs
    );
    assert!(
        near(shape.instance_stubs, native_shape::INSTANCE_STUBS),
        "{} instance stubs",
        shape.instance_stubs
    );
    assert!(
        near(shape.folder_stubs, native_shape::FOLDER_STUBS),
        "{} folder stubs",
        shape.folder_stubs
    );
    // "about" figures: within 10%
    let about = |got: usize, want: usize| got.abs_diff(want) * 10 <= want;
    assert!(
        about(shape.type_nodes, native_shape::TYPE_NODES),
        "{} type nodes",
        shape.type_nodes
    );
    assert!(
        about(shape.definition_nodes, native_shape::DEFINITION_NODES),
        "{} definition nodes",
        shape.definition_nodes
    );
    assert_eq!(shape.max_depth, native_shape::MAX_DEPTH);
    assert!(
        shape.colon_names * 100 >= shape.provider_files,
        "{} %3A names in {} files",
        shape.colon_names,
        shape.provider_files
    );
    // one top folder, the types, the group summary and nothing else
    let below: std::collections::BTreeSet<&str> = files
        .iter()
        .filter_map(|(rel, _)| rel.strip_prefix(&format!("tags/{}/", native_shape::PROVIDER)))
        .filter_map(|rest| rest.split('/').next())
        .collect();
    assert_eq!(
        below.into_iter().collect::<Vec<_>>(),
        [".tag-groups.json", native_shape::TOP_FOLDER, "_types_"]
    );
    assert!(files.iter().any(|(rel, _)| rel == "tags/.tag-config.json"));
}

/// The memory AtomicTags of the definitions follow the measured mix, so the
/// R3-DV rules and the `value without defaultValue` check run at scale.
#[test]
fn native_shape_memory_tags_follow_the_measured_mix() {
    let _guard = TIMED.blocking_lock();
    let shape =
        native_shape::shape_of(&native_shape::native_export(1), native_shape::PROVIDER);
    assert_eq!(
        shape.memory_with_default,
        native_shape::MEMORY_WITH_DEFAULT
    );
    assert_eq!(
        shape.memory_default_differs,
        native_shape::MEMORY_DEFAULT_DIFFERS
    );
    assert_eq!(shape.memory_value_only, native_shape::MEMORY_VALUE_ONLY);
}

/// An allowlist, so no production name is ever written into a test: every
/// name below the provider (path segments, tag and member names, parameter
/// names, typeId segments, group names) is a word of the neutral vocabulary
/// followed by digits, optionally `:` and digits (the `%3A` names). The two
/// structural names `_types_` and Ignition's own `Default` group are the
/// only others.
#[test]
fn native_shape_names_are_neutral() {
    let _guard = TIMED.blocking_lock();
    let files = native_shape::native_export(1);
    let names = native_shape::names_of(&files, native_shape::PROVIDER);
    assert!(names.len() > 1_000, "{} names", names.len());
    let odd: Vec<&String> = names
        .iter()
        .filter(|n| !native_shape::neutral_name(n))
        .collect();
    assert!(odd.is_empty(), "names outside the vocabulary: {odd:?}");
    // the allowlist itself
    for good in ["Area01", "Line12", "Unit3", "Motor05:2", "Sensor900", "_types_"] {
        assert!(native_shape::neutral_name(good), "{good}");
    }
    for bad in ["Area", "Plant01", "Motor05:", "Motor05:x", "motor05", "Line01/Unit01", ""] {
        assert!(!native_shape::neutral_name(bad), "{bad}");
    }
    // the provider name is the plan's fixed `PlantA`; nothing else is named
    // outside the vocabulary
    assert_eq!(native_shape::PROVIDER, "PlantA");
}

/// Deterministic for a seed, the git module's exact byte form for every node
/// file, and the seed moves values only.
#[test]
fn native_shape_files_are_deterministic_and_in_the_module_form() {
    let _guard = TIMED.blocking_lock();
    let one = native_shape::native_export(1);
    assert!(one == native_shape::native_export(1), "not deterministic");
    let two = native_shape::native_export(2);
    let paths = |f: &[(String, Vec<u8>)]| f.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>();
    assert_eq!(paths(&one), paths(&two), "the seed must not move the shape");
    assert!(one != two, "the seed moves values");
    // byte-sorted, unique
    assert!(one.windows(2).all(|w| w[0].0.as_bytes() < w[1].0.as_bytes()));
    let provider = format!("tags/{}/", native_shape::PROVIDER);
    for (rel, bytes) in &one {
        let Some(rest) = rel.strip_prefix(&provider) else {
            continue;
        };
        if rest.starts_with('.') {
            continue;
        }
        let json: serde_json::Value = serde_json::from_slice(bytes).expect(rel);
        assert!(json.get("name").is_none(), "{rel} names itself");
        assert!(
            *bytes == ignition_core::deploy::tag_canon::canonical_file_bytes(&json),
            "{rel} is not in the module's byte form"
        );
    }
}

/// The live export is the gateway's form of the repo: a value-only memory tag
/// gains `defaultValue` equal to its `value` (C12), children are listed in
/// another order (F-P1), and each knob gives exactly its class in the plan.
#[test]
fn native_shape_live_export_follows_the_gateway_and_the_knobs() {
    use ignition_core::deploy::{native_tags, tag_canon, tag_plan};
    let _guard = TIMED.blocking_lock();
    let files = native_shape::native_export(1);
    let scratch = tempfile::tempdir().expect("tempdir");
    native_shape::materialize_repo(scratch.path(), &files);
    let repo = native_tags::read_provider(scratch.path(), native_shape::PROVIDER).expect("read");
    assert!(
        scratch.path().join("deploy/project.yaml").is_file()
            && scratch
                .path()
                .join(format!("deploy/tag-providers/{}.json", native_shape::PROVIDER))
                .is_file()
    );

    // no knob: every repo node is live, the value-only memory tags seeded
    let plain = native_shape::live_export_from(&files, &native_shape::Knobs::default());
    let bytes = serde_json::to_vec(&plain.json).expect("json");
    let live = tag_canon::flatten_export(&bytes, native_shape::PROVIDER).expect("a tag tree");
    assert_eq!(
        live.keys().collect::<Vec<_>>(),
        repo.nodes.keys().collect::<Vec<_>>()
    );
    let mut seeded = 0;
    for (path, node) in &repo.nodes {
        let json = &node.json;
        if json.get("valueSource").and_then(serde_json::Value::as_str) == Some("memory")
            && json.get("value").is_some()
            && json.get("defaultValue").is_none()
        {
            assert_eq!(live[path]["defaultValue"], json["value"], "{path}");
            seeded += 1;
        }
    }
    assert_eq!(seeded, native_shape::MEMORY_VALUE_ONLY);
    let names = |v: &serde_json::Value| -> Vec<String> {
        v["tags"]
            .as_array()
            .expect("tags")
            .iter()
            .map(|c| c["name"].as_str().expect("name").to_string())
            .collect()
    };
    let top = plain.json["tags"]
        .as_array()
        .expect("tags")
        .iter()
        .find(|c| c["name"] == native_shape::TOP_FOLDER)
        .expect("the top folder");
    let listed = names(top);
    let mut sorted = listed.clone();
    sorted.sort();
    assert_ne!(listed, sorted, "the gateway lists children in its own order");
    let plan = tag_plan::plan_provider(&repo, &live, None);
    assert!(plan.added.is_empty() && plan.changed.is_empty() && plan.drift.is_empty());
    assert!(!plan.kept_values.is_empty(), "runtime values differ on the gateway");

    // every knob gives exactly its class
    let knobs = native_shape::Knobs {
        added: 5,
        modified: 7,
        dropping: 3,
        adding: 4,
        types_changed: 2,
        drift: 2,
        removed: 3,
    };
    let shaped = native_shape::live_export_from(&files, &knobs);
    let bytes = serde_json::to_vec(&shaped.json).expect("json");
    let live = tag_canon::flatten_export(&bytes, native_shape::PROVIDER).expect("a tag tree");
    let record = native_shape::tag_record(&repo, &shaped, "shapegw", "ShapeApp");
    let plan = tag_plan::plan_provider(&repo, &live, Some(&record));
    assert_eq!(plan.added, shaped.added);
    let changed: Vec<String> = plan.changed.iter().map(|c| c.path.clone()).collect();
    assert_eq!(changed, shaped.changed());
    assert_eq!(plan.drift, shaped.drift);
    assert_eq!(plan.removed, shaped.removed);
    assert_eq!(
        (
            shaped.added.len(),
            shaped.modified.len(),
            shaped.dropping.len(),
            shaped.adding.len(),
            shaped.types_changed.len(),
            shaped.drift.len(),
            shaped.removed.len()
        ),
        (5, 7, 3, 4, 2, 2, 3)
    );
    // a dropping node is one the repo drops a property of
    for path in &shaped.dropping {
        let dropped = tag_canon::added_properties(&repo.nodes[path].json, &live[path], path);
        assert!(!dropped.is_empty(), "{path}");
    }
    // an adding node is one the repo adds a property to
    for path in &shaped.adding {
        let added = tag_canon::added_properties(&live[path], &repo.nodes[path].json, path);
        assert!(!added.is_empty(), "{path}");
    }
}
