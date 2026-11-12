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
//!
//! The `project_tags_*` rows (plan 26-14, TAGDEP-13) run the project-owned
//! tag deploy on a generated native export with the production provider's
//! shape (`tests/common/native_shape.rs`): the read and form check, the plan,
//! the import batches, the rollback plan and the whole tag step against the
//! fake gateway, each inside a measured budget. `project_tags_real_corpus`
//! does the read and plan on a real export, opt-in through
//! `IGN_NATIVE_CORPUS`, and prints counts and times only.

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
    let shape = native_shape::shape_of(&native_shape::native_export(1), native_shape::PROVIDER);
    assert_eq!(shape.memory_with_default, native_shape::MEMORY_WITH_DEFAULT);
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
    // distinct names: the folders, the numbered tags and members, the types
    assert!(names.len() > 300, "{} distinct names", names.len());
    let odd: Vec<&String> = names
        .iter()
        .filter(|n| !native_shape::neutral_name(n))
        .collect();
    assert!(odd.is_empty(), "names outside the vocabulary: {odd:?}");
    // the allowlist itself
    for good in [
        "Area01",
        "Line12",
        "Unit3",
        "Motor05:2",
        "Sensor900",
        "_types_",
    ] {
        assert!(native_shape::neutral_name(good), "{good}");
    }
    for bad in [
        "Area",
        "Plant01",
        "Motor05:",
        "Motor05:x",
        "motor05",
        "Line01/Unit01",
        "",
    ] {
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
    assert!(
        one.windows(2)
            .all(|w| w[0].0.as_bytes() < w[1].0.as_bytes())
    );
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
                .join(format!(
                    "deploy/tag-providers/{}.json",
                    native_shape::PROVIDER
                ))
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
    assert_ne!(
        listed, sorted,
        "the gateway lists children in its own order"
    );
    let plan = tag_plan::plan_provider(&repo, &live, None);
    assert!(plan.added.is_empty() && plan.changed.is_empty() && plan.drift.is_empty());
    assert!(
        !plan.kept_values.is_empty(),
        "runtime values differ on the gateway"
    );

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

// ---- the project-owned tag deploy at production scale (plan 26-14) --------
//
// Each row materializes the generated native export into a fresh repo (not
// timed), runs one stage of the tag deploy on it and asserts the result and
// the wall-clock time against its budget. The plan the rows share is the
// shape of a production release on that provider: 100 added nodes, 200
// changed (128 modified, 20 that drop a property and go `Overwrite` with
// carry, 50 that add one, 2 types), 20 drift and, where a removal is
// planned, 20 removed. No row times a delete: no tag delete exists (R3-DEL),
// so a removal is planned and refused (`tag_removal`, 26-07).
//
// Budgets are about four times the slowest local run, rounded up to whole
// seconds, so a windows-latest runner several times slower than the
// measuring machine still passes, while a step that turns quadratic in the
// node count (about 45,000 nodes) costs many multiples of it and fails.

use ignition_core::deploy::native_tags::{self, NativeProvider};
use ignition_core::deploy::tag_apply::{self, AppliedSteps, Carried, ImportBatch};
use ignition_core::deploy::{tag_canon, tag_plan};

/// The production release shape the rows plan, with `removed` planned
/// removals.
fn release_knobs(removed: usize) -> native_shape::Knobs {
    native_shape::Knobs {
        added: 100,
        modified: 128,
        dropping: 20,
        adding: 50,
        types_changed: 2,
        drift: 20,
        removed,
    }
}

/// The generated export and the owned provider read from it, materialized
/// into a fresh repo.
struct NativeRepo {
    files: Vec<(String, Vec<u8>)>,
    provider: NativeProvider,
}

impl NativeRepo {
    fn new() -> Self {
        let files = native_shape::native_export(1);
        let scratch = tempfile::tempdir().expect("tempdir");
        native_shape::materialize_repo(scratch.path(), &files);
        let provider =
            native_tags::read_provider(scratch.path(), native_shape::PROVIDER).expect("read");
        NativeRepo { files, provider }
    }
}

/// The export bytes of `live` and their flattened nodes.
fn flattened(live: &serde_json::Value) -> (Vec<u8>, BTreeMap<String, serde_json::Value>) {
    let bytes = serde_json::to_vec(live).expect("json");
    let flat = tag_canon::flatten_export(&bytes, native_shape::PROVIDER).expect("a tag tree");
    (bytes, flat)
}

/// The bodies `batches` are sent as: a carried batch with the values of
/// `fresh` carried in, split again while it outgrows the cap (as the run
/// does, each half with its own fresh export), every other batch as built.
fn sent_bodies(
    batches: &[ImportBatch],
    fresh: &BTreeMap<String, serde_json::Value>,
) -> Vec<ImportBatch> {
    let mut out = Vec::new();
    let mut queue: std::collections::VecDeque<ImportBatch> = batches.iter().cloned().collect();
    while let Some(batch) = queue.pop_front() {
        if !batch.carry {
            out.push(batch);
            continue;
        }
        match tag_apply::carried(&batch, fresh).expect("carry") {
            Carried::Body(body) => out.push(ImportBatch { body, ..batch }),
            Carried::Split(halves) => {
                for half in halves.into_iter().rev() {
                    queue.push_front(half);
                }
            }
        }
    }
    out
}

/// No body over the cap unless it carries a single node (R3-BATCH).
fn assert_capped(row: &str, bodies: &[ImportBatch]) {
    for batch in bodies {
        assert!(
            batch.body.len() <= tag_apply::IMPORT_BATCH_MAX_BYTES || batch.nodes.len() == 1,
            "{row}: a {} byte body at [{}]{} holds {} nodes",
            batch.body.len(),
            native_shape::PROVIDER,
            batch.path,
            batch.nodes.len()
        );
    }
}

/// How many batches `batches` would have been with a `cap` byte body limit:
/// each type alone, every other run of batches with the same path, policy
/// and carry repacked greedily.
fn batches_at_cap(batches: &[ImportBatch], cap: usize) -> usize {
    let mut count = 0;
    let mut group: Option<(String, String, bool)> = None;
    let mut size = 0usize;
    for batch in batches {
        let types = batch.path.is_empty() && batch.nodes.iter().any(|n| n.starts_with("_types_/"));
        let body: serde_json::Value = serde_json::from_slice(&batch.body).expect("body");
        let items: Vec<usize> = body["tags"]
            .as_array()
            .expect("tags")
            .iter()
            .map(|v| serde_json::to_vec(v).expect("item").len())
            .collect();
        let key = (
            batch.path.clone(),
            format!("{:?}", batch.policy),
            batch.carry,
        );
        if types || group.as_ref() != Some(&key) {
            group = (!types).then_some(key);
            count += 1;
            size = 11;
            if types {
                continue;
            }
        }
        for item in items {
            if size > 11 && size + item + 1 > cap {
                count += 1;
                size = 11;
            }
            size += item + usize::from(size > 11);
        }
    }
    count
}

/// Every planned node path the batches carry.
fn sent_paths(batches: &[ImportBatch]) -> std::collections::BTreeSet<String> {
    batches
        .iter()
        .flat_map(|b| b.nodes.iter().cloned())
        .collect()
}

/// The read of the owned provider and the form check of every file.
///
/// Budget: see [`BUDGET_TAGS_READ`].
#[test]
fn project_tags_read_at_prod_scale() {
    let _guard = TIMED.blocking_lock();
    let files = native_shape::native_export(1);
    let scratch = tempfile::tempdir().expect("tempdir");
    native_shape::materialize_repo(scratch.path(), &files);
    let started = Instant::now();
    let (provider, raw) =
        native_tags::read_provider_with_raw(scratch.path(), native_shape::PROVIDER).expect("read");
    let non_canonical = native_tags::check_canonical_form(&provider, &raw);
    let elapsed = started.elapsed();
    let want = native_shape::nodes_from_files(&files, native_shape::PROVIDER);
    assert_eq!(
        provider.nodes.keys().collect::<Vec<_>>(),
        want.keys().collect::<Vec<_>>()
    );
    assert!(provider.groups.is_some(), "the group summary is read");
    // the value-only memory tags are hand-made; every other file is the
    // module's own form
    assert_eq!(non_canonical.len(), native_shape::MEMORY_VALUE_ONLY);
    assert!(
        non_canonical
            .iter()
            .all(|n| n.reason == "value without defaultValue")
    );
    report(
        "project_tags_read_at_prod_scale",
        "read_provider + check_canonical_form",
        elapsed,
        BUDGET_TAGS_READ,
        &format!(
            ", {} nodes from {} files, {} non-canonical",
            provider.nodes.len(),
            raw.len(),
            non_canonical.len()
        ),
    );
}

/// The plan of the release against the live export.
#[test]
fn project_tags_plan_at_prod_scale() {
    let _guard = TIMED.blocking_lock();
    let repo = NativeRepo::new();
    let live = native_shape::live_export_from(&repo.files, &release_knobs(20));
    let record = native_shape::tag_record(&repo.provider, &live, "capgw", native_shape::PROJECT);
    let bytes = serde_json::to_vec(&live.json).expect("json");
    let started = Instant::now();
    let flat = tag_canon::flatten_export(&bytes, native_shape::PROVIDER).expect("a tag tree");
    let plan = tag_plan::plan_provider(&repo.provider, &flat, Some(&record));
    let elapsed = started.elapsed();
    assert_eq!(plan.added, live.added);
    let changed: Vec<String> = plan.changed.iter().map(|c| c.path.clone()).collect();
    assert_eq!(changed, live.changed());
    assert_eq!(changed.len(), 200);
    assert_eq!(plan.drift, live.drift);
    // planned, and refused by the run before any write (`tag_removal`)
    assert_eq!(plan.removed, live.removed);
    assert!(!plan.kept_values.is_empty());
    report(
        "project_tags_plan_at_prod_scale",
        "flatten_export + plan_provider",
        elapsed,
        BUDGET_TAGS_PLAN,
        &format!(
            ", {} live nodes in {} bytes: {} added, {} changed, {} drift, {} removed, {} kept values, {} same",
            flat.len(),
            bytes.len(),
            plan.added.len(),
            plan.changed.len(),
            plan.drift.len(),
            plan.removed.len(),
            plan.kept_values.len(),
            plan.same
        ),
    );
}

/// The import batches of the release, the carried ones with their live
/// values carried in.
#[test]
fn project_tags_batches_at_prod_scale() {
    let _guard = TIMED.blocking_lock();
    let repo = NativeRepo::new();
    let live = native_shape::live_export_from(&repo.files, &release_knobs(20));
    let record = native_shape::tag_record(&repo.provider, &live, "capgw", native_shape::PROJECT);
    let (_, flat) = flattened(&live.json);
    let plan = tag_plan::plan_provider(&repo.provider, &flat, Some(&record));
    let started = Instant::now();
    let batches = tag_apply::import_batches(
        &plan,
        &repo.provider,
        &flat,
        tag_apply::REMOVED_PROPERTY_POLICY,
    )
    .expect("batches");
    let bodies = sent_bodies(&batches, &flat);
    let elapsed = started.elapsed();

    assert_capped("project_tags_batches_at_prod_scale", &bodies);
    let mut want: Vec<String> = live.added.iter().chain(&live.changed()).cloned().collect();
    want.sort();
    assert_eq!(
        sent_paths(&bodies).into_iter().collect::<Vec<_>>(),
        want,
        "every added and changed node is sent, and nothing else"
    );
    let carried: std::collections::BTreeSet<String> = bodies
        .iter()
        .filter(|b| b.carry)
        .flat_map(|b| b.nodes.iter().cloned())
        .collect();
    assert_eq!(
        carried.into_iter().collect::<Vec<_>>(),
        live.dropping,
        "the nodes that drop a property, and only they, go Overwrite with carry"
    );
    let types = bodies
        .iter()
        .filter(|b| b.path.is_empty() && b.nodes.iter().any(|n| n.starts_with("_types_/")))
        .count();
    assert_eq!(types, live.types_changed.len());
    let largest = bodies.iter().map(|b| b.body.len()).max().unwrap_or(0);
    let total: usize = bodies.iter().map(|b| b.body.len()).sum();
    let folders: std::collections::BTreeSet<&str> =
        bodies.iter().map(|b| b.path.as_str()).collect();
    report(
        "project_tags_batches_at_prod_scale",
        "import_batches + carry",
        elapsed,
        BUDGET_TAGS_BATCHES,
        &format!(
            ", {} batches ({} types, {} carried) into {} folders, {total} bytes, largest body {largest} bytes, {} batches at a 2 MiB cap",
            bodies.len(),
            types,
            bodies.iter().filter(|b| b.carry).count(),
            folders.len(),
            batches_at_cap(&bodies, 2 * 1024 * 1024)
        ),
    );
}

/// The first deploy into an empty provider (adoption, 26-14 F1): every
/// node is added. No body is over the cap: a new folder over it goes alone,
/// then its children below it. Every batch goes below the root or a folder an
/// earlier batch created, and a rollback names exactly the created nodes.
#[test]
fn project_tags_adoption_batches_at_prod_scale() {
    let _guard = TIMED.blocking_lock();
    let repo = NativeRepo::new();
    let empty = serde_json::json!({"name": "", "tagType": "Provider",
        "tags": [{"name": "_types_", "tagType": "Folder", "tags": []}]});
    let (_, flat) = flattened(&empty);
    let plan = tag_plan::plan_provider(&repo.provider, &flat, None);
    let started = Instant::now();
    let batches = tag_apply::import_batches(
        &plan,
        &repo.provider,
        &flat,
        tag_apply::REMOVED_PROPERTY_POLICY,
    )
    .expect("batches");
    let elapsed = started.elapsed();

    assert_eq!(plan.added.len(), repo.provider.nodes.len());
    assert!(batches.iter().all(|b| !b.carry));
    let largest = batches.iter().map(|b| b.body.len()).max().unwrap_or(0);
    assert!(
        largest <= tag_apply::IMPORT_BATCH_MAX_BYTES,
        "a {largest} byte body"
    );
    let mut created: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    let mut named = 0;
    for batch in &batches {
        assert!(
            batch.path.is_empty() || created.contains(batch.path.as_str()),
            "{} is sent before its folder",
            batch.path
        );
        created.extend(batch.nodes.iter().map(String::as_str));
        // A type batch also names each new folder of its `_types_` chain.
        named += batch
            .nodes
            .iter()
            .filter(|n| !n.starts_with("_types_/"))
            .count();
    }
    assert_eq!(
        named,
        plan.added
            .iter()
            .filter(|n| !n.starts_with("_types_/"))
            .count(),
        "each tag and folder in exactly one batch"
    );
    let mut want = plan.added.clone();
    want.sort();
    assert_eq!(sent_paths(&batches).into_iter().collect::<Vec<_>>(), want);

    let mut applied = AppliedSteps::default();
    for batch in &batches {
        applied.sent(batch);
        // What the run sent at each path (26-REVIEW CR-01).
        for path in &batch.nodes {
            if let Some(node) = repo.provider.nodes.get(path) {
                applied.bodies.insert(path.clone(), node.json.clone());
            }
        }
    }
    let all = native_shape::gateway_export(&native_shape::nodes_from_files(
        &repo.files,
        native_shape::PROVIDER,
    ));
    let (_, current) = flattened(&all);
    for created_provider in [false, true] {
        let undo = tag_apply::rollback_plan(
            &plan,
            &flat,
            &current,
            &applied,
            created_provider,
            &std::collections::BTreeSet::new(),
            None,
            tag_apply::TAG_DELETE_DESCENDANTS,
        )
        .expect("rollback plan");
        let listed = if created_provider {
            &undo.provider_deletes
        } else {
            &undo.undeletable
        };
        assert_eq!(listed, &want, "created provider {created_provider}");
        assert!(undo.imports.is_empty());
    }
    let total: usize = batches.iter().map(|b| b.body.len()).sum();
    report(
        "project_tags_adoption_batches_at_prod_scale",
        "import_batches",
        elapsed,
        BUDGET_TAGS_BATCHES,
        &format!(
            ", {} added in {} batches, {total} bytes, largest body {largest} bytes",
            plan.added.len(),
            batches.len()
        ),
    );
}

/// The undo of the release from the pre-image: changed types and tags go
/// back, the property-adding ones by carried `Overwrite` with the values of
/// a fresh export, and the created nodes are named undeletable (R3-DEL).
#[test]
fn project_tags_rollback_plan_at_prod_scale() {
    let _guard = TIMED.blocking_lock();
    let repo = NativeRepo::new();
    let knobs = release_knobs(0);
    let before = native_shape::live_export_from(&repo.files, &knobs);
    let record = native_shape::tag_record(&repo.provider, &before, "capgw", native_shape::PROJECT);
    let (_, pre_image) = flattened(&before.json);
    let plan = tag_plan::plan_provider(&repo.provider, &pre_image, Some(&record));
    let batches = tag_apply::import_batches(
        &plan,
        &repo.provider,
        &pre_image,
        tag_apply::REMOVED_PROPERTY_POLICY,
    )
    .expect("batches");
    let mut applied = AppliedSteps::default();
    for batch in &batches {
        applied.sent(batch);
        // What the run sent at each path (26-REVIEW CR-01).
        for path in &batch.nodes {
            if let Some(node) = repo.provider.nodes.get(path) {
                applied.bodies.insert(path.clone(), node.json.clone());
            }
        }
    }
    // the provider after the run: every repo node live, the drift untouched
    let after = native_shape::live_export_from(
        &repo.files,
        &native_shape::Knobs {
            drift: knobs.drift,
            ..native_shape::Knobs::default()
        },
    );
    let (fresh_bytes, current) = flattened(&after.json);

    let started = Instant::now();
    let undo = tag_apply::rollback_plan(
        &plan,
        &pre_image,
        &current,
        &applied,
        false,
        &std::collections::BTreeSet::new(),
        None,
        tag_apply::TAG_DELETE_DESCENDANTS,
    )
    .expect("rollback plan");
    let fresh = tag_canon::flatten_export(&fresh_bytes, native_shape::PROVIDER).expect("fresh");
    let bodies = sent_bodies(&undo.imports, &fresh);
    let elapsed = started.elapsed();

    assert_capped("project_tags_rollback_plan_at_prod_scale", &bodies);
    assert_eq!(
        undo.undeletable, before.added,
        "no tag delete exists (R3-DEL)"
    );
    assert!(undo.provider_deletes.is_empty() && undo.uncarriable.is_empty());
    assert_eq!(undo.carried, before.adding);
    let mut restored: Vec<String> = before.changed();
    restored.sort();
    assert_eq!(
        sent_paths(&bodies).into_iter().collect::<Vec<_>>(),
        restored,
        "every changed node goes back, nothing the run did not change"
    );
    // no pre-image value is sent: a carried body holds the fresh values only
    let carried_bodies = bodies.iter().filter(|b| b.carry).count();
    assert!(carried_bodies > 0);
    report(
        "project_tags_rollback_plan_at_prod_scale",
        "rollback_plan + carry from a fresh export",
        elapsed,
        BUDGET_TAGS_ROLLBACK,
        &format!(
            ", {} restore batches ({carried_bodies} carried), {} undeletable",
            bodies.len(),
            undo.undeletable.len()
        ),
    );
}

// Budgets of the `project_tags_*` rows. Measured 2026-11-09 on an Apple M1
// Max (10 cores), debug profile, rows run one at a time, six to nine runs
// each while the machine ran other builds (load average 45 to 293 on 10
// cores, so the slowest runs are up to ten times the fastest). Each budget
// is about four times the slowest run, rounded up to whole seconds: a
// windows-latest runner several times slower than this machine unloaded
// still passes, while a step that turns quadratic in the node count (about
// 45,000 nodes with the stubs) costs minutes and fails.

/// `read_provider` + `check_canonical_form` of 3,266 files: 2.11 s fastest,
/// 16.03 s slowest.
const BUDGET_TAGS_READ: Duration = Duration::from_secs(65);
/// `flatten_export` of the 3.1 MB export + `plan_provider`: 0.97 s fastest,
/// 3.75 s slowest.
const BUDGET_TAGS_PLAN: Duration = Duration::from_secs(15);
/// `import_batches` + the carry of the 15 carried batches: 0.05 s fastest,
/// 0.59 s slowest.
const BUDGET_TAGS_BATCHES: Duration = Duration::from_secs(3);
/// `rollback_plan` + a fresh export flattened + the carry of 25 carried
/// restores: 0.28 s fastest, 2.12 s slowest.
const BUDGET_TAGS_ROLLBACK: Duration = Duration::from_secs(9);
/// `deploy_projects_prepare` (provider list and settings, the export, the
/// group read, the plan, the check of the project's tag references): 2.25 s
/// fastest, 17.85 s slowest.
const BUDGET_TAGS_PREPARE: Duration = Duration::from_secs(72);
/// `deploy_projects_run`, the time the fake spent answering left out (82
/// imports, 18 exports, the host copy, the verify, the records): 9.05 s
/// fastest, 59.75 s slowest.
const BUDGET_TAGS_RUN_OWN: Duration = Duration::from_secs(240);
/// `deploy_projects_run` with the fake's time: 32.40 s fastest, 133.77 s
/// slowest, the fake 24 to 85 s of it (it re-parses and re-serializes its
/// whole 3 MB tree on every import).
const BUDGET_TAGS_RUN: Duration = Duration::from_secs(540);

/// The gateway's own `Default` group unit (C14), which the summary names.
const DEFAULT_GROUP_UNIT: &str = r#"{
  "config": {
    "mode": "Direct",
    "oneShot": false,
    "opcDataMode": "Subscribed",
    "optWriteTimeout": 60000,
    "optWrites": false,
    "rate": 1000,
    "readAfterWrite": false
  }
}"#;
const GROUP_UNIT_RESOURCE: &str = r#"{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["config.json"],"attributes":{}}"#;

/// The members of the generated repo's project: one script.
fn shape_project_members() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        (
            "project.json",
            br#"{"title":"Shape App","enabled":true}"#.to_vec(),
        ),
        (
            "ignition/script-python/shape/resource.json",
            br#"{"scope":"A","version":1,"files":["code.py"],"attributes":{"enabled":true}}"#
                .to_vec(),
        ),
        (
            "ignition/script-python/shape/code.py",
            b"def f():\n    return 1\n".to_vec(),
        ),
    ]
}

/// A [`ProjectGateway`] that forwards every call to the fake and adds up how
/// long the fake spent answering them. The fake re-parses and re-serializes
/// its whole tag tree on every tag export and import (about 3 MB here), a
/// cost a gateway pays in its own process, so the row budgets ign's own
/// share apart from the fake's.
///
/// [`ProjectGateway`]: ignition_core::actions::deploy_projects::ProjectGateway
struct TimedGateway<'a> {
    inner: &'a common::fake_projects::FakeProjects,
    spent: std::sync::Mutex<Duration>,
}

impl TimedGateway<'_> {
    async fn time<T>(&self, call: impl std::future::Future<Output = T>) -> T {
        let (out, took) = timed(call).await;
        *self.spent.lock().expect("spent") += took;
        out
    }

    /// The time the fake spent since the last call, and reset it.
    fn take(&self) -> Duration {
        std::mem::take(&mut *self.spent.lock().expect("spent"))
    }
}

#[async_trait::async_trait]
impl ignition_core::actions::deploy_projects::ProjectGateway for TimedGateway<'_> {
    async fn projects(
        &self,
    ) -> Result<Vec<ignition_core::client::projects::ProjectRecord>, ignition_core::error::CoreError>
    {
        self.time(self.inner.projects()).await
    }

    async fn project_find(
        &self,
        name: &str,
    ) -> Result<
        Option<ignition_core::client::projects::ProjectRecord>,
        ignition_core::error::CoreError,
    > {
        self.time(self.inner.project_find(name)).await
    }

    async fn export_bytes(&self, name: &str) -> Result<Vec<u8>, ignition_core::error::CoreError> {
        self.time(self.inner.export_bytes(name)).await
    }

    async fn designers(
        &self,
    ) -> Result<
        Vec<ignition_core::deploy::apply::preflight::DesignerSeen>,
        ignition_core::error::CoreError,
    > {
        self.time(self.inner.designers()).await
    }

    async fn gateway_name(&self) -> Result<Option<String>, ignition_core::error::CoreError> {
        self.time(self.inner.gateway_name()).await
    }

    async fn tag_provider_names(&self) -> Result<Vec<String>, ignition_core::error::CoreError> {
        self.time(self.inner.tag_provider_names()).await
    }

    async fn tag_export(&self, provider: &str) -> Result<Vec<u8>, ignition_core::error::CoreError> {
        self.time(self.inner.tag_export(provider)).await
    }

    async fn project_import(
        &self,
        name: &str,
        zip: Vec<u8>,
        overwrite: bool,
    ) -> Result<ignition_core::client::projects::ImportOutcome, ignition_core::error::CoreError>
    {
        self.time(self.inner.project_import(name, zip, overwrite))
            .await
    }

    async fn tag_import(
        &self,
        provider: &str,
        path: &str,
        policy: ignition_core::client::tags::CollisionPolicy,
        body: Vec<u8>,
    ) -> Result<ignition_core::client::tags::TagImportAnswer, ignition_core::error::CoreError> {
        self.time(self.inner.tag_import(provider, path, policy, body))
            .await
    }

    async fn tag_provider_record(
        &self,
        name: &str,
    ) -> Result<
        Option<ignition_core::client::tags::TagProviderRecord>,
        ignition_core::error::CoreError,
    > {
        self.time(self.inner.tag_provider_record(name)).await
    }

    async fn tag_provider_create(
        &self,
        body: &[ignition_core::client::tags::TagProviderCreate],
    ) -> Result<(), ignition_core::error::CoreError> {
        self.time(self.inner.tag_provider_create(body)).await
    }

    async fn tag_provider_modify(
        &self,
        body: &[ignition_core::client::tags::TagProviderModify],
    ) -> Result<(), ignition_core::error::CoreError> {
        self.time(self.inner.tag_provider_modify(body)).await
    }

    async fn tag_provider_delete(
        &self,
        name: &str,
        signature: &str,
    ) -> Result<(), ignition_core::error::CoreError> {
        self.time(self.inner.tag_provider_delete(name, signature))
            .await
    }
}

/// The whole tag step of `ign deploy projects` against the fake gateway
/// holding the generated provider: the prepare (provider list, settings,
/// export, group read, plan) and the run (`plan_stale` re-export, host copy,
/// every batch, a fresh export before each carried batch, the verify reads,
/// the tag record). The project is already this release, so only the tags
/// change. No removal is planned: a removal refuses the run before any write.
#[tokio::test]
async fn project_tags_run_through_fake_at_prod_scale() {
    use common::fake_projects::FakeProjects;
    use ignition_core::actions::deploy_projects::{
        NoProjectsObserver, ProjectOutcome, ProjectsInput, ProjectsPrepared, ProjectsRunOptions,
        ProjectsSurfaces, deploy_projects_prepare, deploy_projects_run, load_project,
    };
    use ignition_core::deploy::apply::preflight::DesignersPolicy;
    use ignition_core::deploy::projects::{ProjectAction, ReleaseRecord};

    let _guard = TIMED.lock().await;
    let gateway_dir = common::fake_projects::FAKE_GATEWAY_NAME;
    let files = native_shape::native_export(1);
    let scratch = tempfile::tempdir().expect("tempdir");
    let repo = scratch.path().join("repo");
    let targets = scratch.path().join("targets-root");
    let data = scratch.path().join("data");
    let put = |root: &std::path::Path, rel: &str, bytes: &[u8]| {
        let target = root.join(rel);
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(target, bytes).expect("write");
    };
    native_shape::materialize_repo(&repo, &files);
    let members = shape_project_members();
    for (rel, bytes) in &members {
        put(&repo, rel, bytes);
    }
    std::fs::create_dir_all(data.join("projects")).expect("data dir");
    put(
        &data,
        "config/resources/core/ignition/system-properties/config.json",
        format!(r#"{{"systemName":"{gateway_dir}"}}"#).as_bytes(),
    );
    let group = format!(
        "config/resources/core/ignition/tag-group/{}/Default",
        native_shape::PROVIDER
    );
    put(
        &data,
        &format!("{group}/config.json"),
        DEFAULT_GROUP_UNIT.as_bytes(),
    );
    put(
        &data,
        &format!("{group}/resource.json"),
        GROUP_UNIT_RESOURCE.as_bytes(),
    );
    put(
        &targets,
        "targets/dev.yaml",
        format!(
            "schema_version: 1\ngateway: {gateway_dir}\nmode: dev\nnodes:\n  master:\n    profile: dev\n    local:\n      data_dir: '{}'\n",
            data.display()
        )
        .as_bytes(),
    );

    let knobs = release_knobs(0);
    let live = native_shape::live_export_from(&files, &knobs);
    let provider = native_tags::read_provider(&repo, native_shape::PROVIDER).expect("read");
    let record = native_shape::tag_record(&provider, &live, gateway_dir, native_shape::PROJECT);
    put(
        &data,
        &format!(
            "ign-deploy/{gateway_dir}/tag-providers/{}/record.json",
            native_shape::PROVIDER
        ),
        &record.to_bytes(),
    );
    let gateway = FakeProjects::new()
        .with_project(native_shape::PROJECT, None, false, &members)
        .with_tag_tree(native_shape::PROVIDER, live.json.clone());
    let input = ProjectsInput {
        repo: repo.clone(),
        targets: targets.clone(),
        env: "dev".to_string(),
        release: "v1.0.0".to_string(),
        allow_downgrade: false,
        allow_first_change: None,
        allow_destructive: false,
        // No tag-delete route here: the run accepts that a rollback would
        // leave the tags it creates.
        allow_manual_undo: true,
        artifact_sha256: None,
        tags_sha256: None,
    };
    let loaded = load_project(&input).expect("the project loads");
    let release = ReleaseRecord {
        schema_version: 1,
        gateway: gateway_dir.to_string(),
        project: native_shape::PROJECT.to_string(),
        release: "v1.0.0".to_string(),
        artifact_sha256: loaded.artifact.digest.sha256.clone(),
        run_id: "20261020T101500Z-ab12".to_string(),
        deployed_at: "2026-10-20T10:15:00Z".to_string(),
    };
    put(
        &data,
        &format!(
            "ign-deploy/{gateway_dir}/projects/{}/release.json",
            native_shape::PROJECT
        ),
        &release.to_bytes(),
    );
    let host = LocalFsTransport::new(data.clone(), "nodes.master");
    let reader = LocalFsTransport::new(data.clone(), "nodes.master");
    let timed_gateway = TimedGateway {
        inner: &gateway,
        spent: std::sync::Mutex::new(Duration::ZERO),
    };
    let surfaces = ProjectsSurfaces {
        gateway: &timed_gateway,
        host: &host,
        tree: Some(&reader),
        apply_host: None,
        deploy: None,
        tag_delete: ignition_core::actions::deploy_projects::TagDeleteSurface::NotConfigured {
            missing: vec![],
        },
    };
    let options = ProjectsRunOptions {
        repo: input.repo.clone(),
        targets_root: input.targets.clone(),
        target_file: loaded.target_file.clone(),
        master: loaded.target.nodes.master.clone(),
        record_writer: None,
        tag_record_writer: None,
        run_writer: None,
        designers: DesignersPolicy::default(),
        children_settle: Duration::from_millis(50),
        children_poll: Duration::from_millis(5),
        tag_verify_settle: Duration::from_secs(30),
        tag_verify_poll: Duration::from_millis(5),
        backup: None,
        backup_policy: ignition_core::deploy::pair::BackupPolicy::default(),
        tag_delete_ready: std::time::Duration::from_millis(50),
        tag_delete_poll: std::time::Duration::from_millis(5),
    };

    let (prepared, prepare_time) = timed(deploy_projects_prepare(&loaded, &input, &surfaces)).await;
    let prepare_fake = timed_gateway.take();
    let prepared = prepared.expect("the plan completes");
    let ProjectsPrepared::Ready { plan, .. } = &prepared else {
        panic!("a ready plan");
    };
    assert_eq!(plan.action, ProjectAction::Skip, "only the tags change");
    let tags = plan.tags.as_ref().expect("data.tags");
    assert_eq!(
        (
            tags.added_count,
            tags.changed_count,
            tags.drift_count,
            tags.removed_count
        ),
        (100, 200, 20, 0)
    );
    let exports_before = gateway.tag_export_calls().len();
    let (result, run_time) = timed(deploy_projects_run(
        prepared,
        &loaded,
        &input,
        &surfaces,
        &options,
        &NoProjectsObserver,
    ))
    .await;
    let run_fake = timed_gateway.take();
    let result = result.expect("the run completes");
    assert!(result.ok, "{result:?}");
    assert_eq!(result.outcome, ProjectOutcome::Applied);
    let tags = result.tags.as_ref().expect("data.tags");
    let verify = tags.verify.as_ref().expect("the verify ran");
    assert!(verify.equal && verify.settled, "{verify:?}");
    assert_eq!(tags.carried_count, knobs.dropping);
    let imports = gateway.tag_import_calls();
    assert!(
        gateway.import_calls().is_empty(),
        "the project is not imported"
    );
    let run_exports = gateway.tag_export_calls().len() - exports_before;

    // the gateway now holds the repo: nothing left to add or change, the
    // drift untouched
    let again = deploy_projects_prepare(&loaded, &input, &surfaces)
        .await
        .expect("the second plan")
        .into_check_result();
    let tags_again = again.tags.as_ref().expect("data.tags");
    assert_eq!(
        (
            tags_again.added_count,
            tags_again.changed_count,
            tags_again.drift_count
        ),
        (0, 0, 20)
    );

    let row = "project_tags_run_through_fake_at_prod_scale";
    report(
        row,
        "deploy_projects_prepare",
        prepare_time,
        BUDGET_TAGS_PREPARE,
        &format!(", the fake {:.2} s of it", prepare_fake.as_secs_f64()),
    );
    let calls = format!(
        ", {} tag imports, {run_exports} tag exports, {} verify reads in {:.2} s",
        imports.len(),
        verify.reads,
        verify.elapsed_s
    );
    report(
        row,
        "deploy_projects_run without the fake",
        run_time.saturating_sub(run_fake),
        BUDGET_TAGS_RUN_OWN,
        &calls,
    );
    report(
        row,
        "deploy_projects_run",
        run_time,
        BUDGET_TAGS_RUN,
        &format!(", the fake {:.2} s of it{calls}", run_fake.as_secs_f64()),
    );
}

// ---- forward deletes at production scale (plan 26-20) -----------------------

/// How many nodes the delete row removes, and how: 930 recorded leaf tags
/// the repo dropped (in the unit folders), 50 recorded folders dropped whole
/// with 20 leaf tags each (1,050 nodes), and 20 recorded UDT instances.
const DELETE_LEAVES: usize = 930;
const DELETE_FOLDERS: usize = 50;
const DELETE_FOLDER_CHILDREN: usize = 20;
const DELETE_INSTANCES: usize = 20;
const DELETE_REMOVED: usize =
    DELETE_LEAVES + DELETE_FOLDERS * (1 + DELETE_FOLDER_CHILDREN) + DELETE_INSTANCES;

/// The generated provider as the gateway holds it before a release that
/// removes [`DELETE_REMOVED`] nodes this project's record lists, beside 20
/// drift nodes; the record; and the removed paths.
fn deleting_release(
    repo: &NativeProvider,
    files: &[(String, Vec<u8>)],
    gateway: &str,
) -> (
    serde_json::Value,
    ignition_core::deploy::tag_record::TagRecord,
    Vec<String>,
) {
    let mut live = native_shape::live_export_from(
        files,
        &native_shape::Knobs {
            drift: 20,
            removed: DELETE_LEAVES,
            ..native_shape::Knobs::default()
        },
    );
    let (_, mut nodes) = flattened(&live.json);
    let type_id = nodes
        .iter()
        .find(|(path, node)| path.starts_with("_types_/") && node["tagType"] == "UdtType")
        .map(|(path, _)| path.trim_start_matches("_types_/").to_string())
        .expect("a generated type");
    let top = native_shape::TOP_FOLDER;
    let mut extra = Vec::new();
    for k in 0..DELETE_FOLDERS {
        let folder = format!("{top}/Line{}", 800 + k);
        nodes.insert(folder.clone(), serde_json::json!({"tagType": "Folder"}));
        extra.push(folder.clone());
        for c in 0..DELETE_FOLDER_CHILDREN {
            let child = format!("{folder}/Valve{c:02}");
            nodes.insert(
                child.clone(),
                serde_json::json!({"tagType": "AtomicTag", "dataType": "Int4",
                    "valueSource": "memory", "value": 5, "defaultValue": 1}),
            );
            extra.push(child);
        }
    }
    for k in 0..DELETE_INSTANCES {
        let instance = format!("{top}/Line01/Motor{}", 950 + k);
        nodes.insert(
            instance.clone(),
            serde_json::json!({"tagType": "UdtInstance", "typeId": type_id}),
        );
        extra.push(instance);
    }
    live.removed.extend(extra);
    live.removed.sort();
    assert_eq!(live.removed.len(), DELETE_REMOVED);
    let json = native_shape::gateway_export(&nodes);
    let record = native_shape::tag_record(repo, &live, gateway, native_shape::PROJECT);
    (json, record, live.removed)
}

/// A [`TagDeleteRoute`] that forwards to the fake route and adds up the time
/// the fake spent: it re-parses and re-serializes its whole tag tree for
/// every path it deletes.
///
/// [`TagDeleteRoute`]: ignition_core::deploy::tag_delete::TagDeleteRoute
struct TimedRoute<'a> {
    inner: &'a common::fake_projects::FakeTagDeleteRoute<'a>,
    spent: std::sync::Mutex<Duration>,
}

#[async_trait::async_trait]
impl ignition_core::deploy::tag_delete::TagDeleteRoute for TimedRoute<'_> {
    async fn handshake(
        &self,
    ) -> Result<ignition_core::deploy::tag_delete::RouteHandshake, ignition_core::error::CoreError>
    {
        let (out, took) = timed(self.inner.handshake()).await;
        *self.spent.lock().expect("spent") += took;
        out
    }

    async fn delete(
        &self,
        provider: &str,
        paths: &[String],
    ) -> Result<ignition_core::deploy::tag_delete::DeleteAnswer, ignition_core::error::CoreError>
    {
        let (out, took) = timed(self.inner.delete(provider, paths)).await;
        *self.spent.lock().expect("spent") += took;
        out
    }
}

/// D-02 at production size (TAGDEP-13): a release that removes 2,000 nodes
/// of the generated provider (930 tags, 50 folders whole, 20 instances).
///
/// 1. Offline: the plan, [`tag_apply::delete_calls`] at the route's 500-path
///    cap in both delete modes, and the [`tag_apply::rollback_plan`]
///    re-create of every deleted path from the pre-image, each body within
///    512 KiB.
/// 2. The whole tag step through the fake gateway and the fake tag-delete
///    route with `--allow-destructive`: the prepare, the run (re-export, host
///    copy, the delete calls, the verify, the record), then a second plan
///    that removes nothing and keeps the drift.
///
/// Budget: see [`BUDGET_TAGS_DELETES_OFFLINE`] and the two run budgets.
#[tokio::test]
async fn project_tags_deletes_at_prod_scale() {
    use common::fake_projects::{FakeProjects, FakeTagDeleteRoute};
    use ignition_core::actions::deploy_projects::{
        NoProjectsObserver, ProjectOutcome, ProjectsInput, ProjectsPrepared, ProjectsRunOptions,
        ProjectsSurfaces, TagDeleteSurface, deploy_projects_prepare, deploy_projects_run,
        load_project,
    };
    use ignition_core::deploy::apply::preflight::DesignersPolicy;
    use ignition_core::deploy::projects::ReleaseRecord;
    use ignition_core::deploy::tag_delete::RouteHandshake;

    let _guard = TIMED.lock().await;
    let row = "project_tags_deletes_at_prod_scale";
    let gateway_dir = common::fake_projects::FAKE_GATEWAY_NAME;
    let files = native_shape::native_export(1);
    let scratch = tempfile::tempdir().expect("tempdir");
    let repo = scratch.path().join("repo");
    let targets = scratch.path().join("targets-root");
    let data = scratch.path().join("data");
    let put = |root: &std::path::Path, rel: &str, bytes: &[u8]| {
        let target = root.join(rel);
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(target, bytes).expect("write");
    };
    native_shape::materialize_repo(&repo, &files);
    let provider = native_tags::read_provider(&repo, native_shape::PROVIDER).expect("read");
    let (live_json, record, removed) = deleting_release(&provider, &files, gateway_dir);

    // 1. Offline.
    let handshake = RouteHandshake {
        route_version: "1".to_string(),
        min_cli: "0".to_string(),
        providers: vec![native_shape::PROVIDER.to_string()],
        max_paths: 500,
        max_path_chars: 1000,
    };
    let started = Instant::now();
    let (_, pre_image) = flattened(&live_json);
    let plan = tag_plan::plan_provider(&provider, &pre_image, Some(&record));
    let calls = tag_apply::delete_calls(
        &plan,
        &pre_image,
        &handshake,
        tag_apply::TAG_DELETE_DESCENDANTS,
    )
    .expect("a delete set");
    let every = tag_apply::delete_calls(
        &plan,
        &pre_image,
        &handshake,
        tag_apply::DeleteDescendants {
            folders: false,
            instances: false,
        },
    )
    .expect("a delete set");
    let sent: Vec<String> = calls.iter().flat_map(|c| c.paths.clone()).collect();
    let mut current = pre_image.clone();
    for path in &removed {
        current.remove(path);
    }
    let applied = AppliedSteps {
        deleted: sent.clone(),
        ..AppliedSteps::default()
    };
    let undo = tag_apply::rollback_plan(
        &plan,
        &pre_image,
        &current,
        &applied,
        false,
        &std::collections::BTreeSet::new(),
        Some(&handshake),
        tag_apply::TAG_DELETE_DESCENDANTS,
    )
    .expect("a rollback plan");
    let offline = started.elapsed();

    assert_eq!(plan.removed, removed, "every recorded removal is planned");
    assert_eq!(plan.drift.len(), 20);
    let removed_set: std::collections::BTreeSet<&String> = removed.iter().collect();
    for call in calls.iter().chain(&every) {
        assert!(!call.paths.is_empty() && call.paths.len() <= 500);
        assert!(call.paths.iter().all(|p| removed_set.contains(p)));
    }
    let top_level = DELETE_LEAVES + DELETE_FOLDERS + DELETE_INSTANCES;
    assert_eq!(sent.len(), top_level, "a folder goes alone (26-17 record)");
    assert_eq!(calls.len(), top_level.div_ceil(500));
    assert_eq!(
        every.iter().map(|c| c.paths.len()).sum::<usize>(),
        DELETE_REMOVED
    );
    assert_eq!(every.len(), DELETE_REMOVED.div_ceil(500));
    assert_capped(row, &undo.imports);
    assert_eq!(
        sent_paths(&undo.imports).into_iter().collect::<Vec<_>>(),
        removed,
        "every deleted node comes back, nothing else"
    );
    let largest_body = undo.imports.iter().map(|b| b.body.len()).max().unwrap_or(0);

    // 2. The whole tag step through the fakes.
    let members = shape_project_members();
    for (rel, bytes) in &members {
        put(&repo, rel, bytes);
    }
    std::fs::create_dir_all(data.join("projects")).expect("data dir");
    put(
        &data,
        "config/resources/core/ignition/system-properties/config.json",
        format!(r#"{{"systemName":"{gateway_dir}"}}"#).as_bytes(),
    );
    let group = format!(
        "config/resources/core/ignition/tag-group/{}/Default",
        native_shape::PROVIDER
    );
    put(
        &data,
        &format!("{group}/config.json"),
        DEFAULT_GROUP_UNIT.as_bytes(),
    );
    put(
        &data,
        &format!("{group}/resource.json"),
        GROUP_UNIT_RESOURCE.as_bytes(),
    );
    put(
        &targets,
        "targets/dev.yaml",
        format!(
            "schema_version: 1\ngateway: {gateway_dir}\nmode: dev\nnodes:\n  master:\n    profile: dev\n    local:\n      data_dir: '{}'\n",
            data.display()
        )
        .as_bytes(),
    );
    put(
        &data,
        &format!(
            "ign-deploy/{gateway_dir}/tag-providers/{}/record.json",
            native_shape::PROVIDER
        ),
        &record.to_bytes(),
    );
    let gateway = FakeProjects::new()
        .with_project(native_shape::PROJECT, None, false, &members)
        .with_tag_tree(native_shape::PROVIDER, live_json.clone());
    let input = ProjectsInput {
        repo: repo.clone(),
        targets: targets.clone(),
        env: "dev".to_string(),
        release: "v1.0.0".to_string(),
        allow_downgrade: false,
        allow_first_change: None,
        allow_destructive: true,
        allow_manual_undo: false,
        artifact_sha256: None,
        tags_sha256: None,
    };
    let loaded = load_project(&input).expect("the project loads");
    let release = ReleaseRecord {
        schema_version: 1,
        gateway: gateway_dir.to_string(),
        project: native_shape::PROJECT.to_string(),
        release: "v1.0.0".to_string(),
        artifact_sha256: loaded.artifact.digest.sha256.clone(),
        run_id: "20261020T101500Z-ab12".to_string(),
        deployed_at: "2026-10-20T10:15:00Z".to_string(),
    };
    put(
        &data,
        &format!(
            "ign-deploy/{gateway_dir}/projects/{}/release.json",
            native_shape::PROJECT
        ),
        &release.to_bytes(),
    );
    let host = LocalFsTransport::new(data.clone(), "nodes.master");
    let reader = LocalFsTransport::new(data.clone(), "nodes.master");
    let timed_gateway = TimedGateway {
        inner: &gateway,
        spent: std::sync::Mutex::new(Duration::ZERO),
    };
    let fake_route = FakeTagDeleteRoute::new(&gateway, &[native_shape::PROVIDER]);
    let route = TimedRoute {
        inner: &fake_route,
        spent: std::sync::Mutex::new(Duration::ZERO),
    };
    let surfaces = ProjectsSurfaces {
        gateway: &timed_gateway,
        host: &host,
        tree: Some(&reader),
        apply_host: None,
        deploy: None,
        tag_delete: TagDeleteSurface::Configured(&route),
    };
    let options = ProjectsRunOptions {
        repo: input.repo.clone(),
        targets_root: input.targets.clone(),
        target_file: loaded.target_file.clone(),
        master: loaded.target.nodes.master.clone(),
        record_writer: None,
        tag_record_writer: None,
        run_writer: None,
        designers: DesignersPolicy::default(),
        children_settle: Duration::from_millis(50),
        children_poll: Duration::from_millis(5),
        tag_verify_settle: Duration::from_secs(30),
        tag_verify_poll: Duration::from_millis(5),
        backup: None,
        backup_policy: ignition_core::deploy::pair::BackupPolicy::default(),
        tag_delete_ready: Duration::from_millis(50),
        tag_delete_poll: Duration::from_millis(5),
    };
    let prepared = deploy_projects_prepare(&loaded, &input, &surfaces)
        .await
        .expect("the plan completes");
    let ProjectsPrepared::Ready { plan: planned, .. } = &prepared else {
        panic!("a ready plan");
    };
    assert_eq!(
        planned.tags.as_ref().expect("data.tags").removed_count,
        DELETE_REMOVED
    );
    timed_gateway.take();
    *route.spent.lock().expect("spent") = Duration::ZERO;
    let (result, run_time) = timed(deploy_projects_run(
        prepared,
        &loaded,
        &input,
        &surfaces,
        &options,
        &NoProjectsObserver,
    ))
    .await;
    let run_fake = timed_gateway.take() + *route.spent.lock().expect("spent");
    let result = result.expect("the run completes");
    assert!(result.ok, "{result:?}");
    assert_eq!(result.outcome, ProjectOutcome::Applied);
    let tags = result.tags.as_ref().expect("data.tags");
    assert_eq!(tags.deleted_count, top_level);
    let verify = tags.verify.as_ref().expect("the verify ran");
    assert!(verify.equal, "{verify:?}");
    let route_calls = fake_route.delete_calls();
    assert_eq!(route_calls.len(), top_level.div_ceil(500));
    let largest_call = route_calls.iter().map(|(_, p)| p.len()).max().unwrap_or(0);
    assert!(largest_call <= 500);
    assert!(gateway.tag_import_calls().is_empty(), "only deletes");
    let again = deploy_projects_prepare(&loaded, &input, &surfaces)
        .await
        .expect("the second plan")
        .into_check_result();
    let tags_again = again.tags.as_ref().expect("data.tags");
    assert_eq!((tags_again.removed_count, tags_again.drift_count), (0, 20));

    report(
        row,
        "plan + delete_calls (both modes) + rollback_plan re-create",
        offline,
        BUDGET_TAGS_DELETES_OFFLINE,
        &format!(
            ", {} removed, {} call(s) of {} path(s) collapsed, {} of {} every path, {} re-create \
             batches, largest body {largest_body} bytes",
            plan.removed.len(),
            calls.len(),
            sent.len(),
            every.len(),
            DELETE_REMOVED,
            undo.imports.len()
        ),
    );
    let calls_line = format!(
        ", {} delete call(s), largest {largest_call} path(s), {} verify reads",
        route_calls.len(),
        verify.reads
    );
    report(
        row,
        "deploy_projects_run without the fakes",
        run_time.saturating_sub(run_fake),
        BUDGET_TAGS_DELETES_RUN_OWN,
        &calls_line,
    );
    report(
        row,
        "deploy_projects_run",
        run_time,
        BUDGET_TAGS_DELETES_RUN,
        &format!(
            ", the fakes {:.2} s of it{calls_line}",
            run_fake.as_secs_f64()
        ),
    );
}

// Budgets of `project_tags_deletes_at_prod_scale`. Measured 2026-11-10 on an
// Apple M1 Max (10 cores), debug profile, the row alone, five runs while the
// machine ran other builds (load average 34 to 163 on 10 cores). Each budget
// is about four times the slowest run, rounded up to whole seconds (the
// 26-14 rule).

/// `plan_provider` + `delete_calls` in both modes + the `rollback_plan`
/// re-create of every deleted path: 1.92 s fastest, 3.56 s slowest.
const BUDGET_TAGS_DELETES_OFFLINE: Duration = Duration::from_secs(15);
/// `deploy_projects_run` with 2 delete calls, the fakes' time left out (the
/// re-export, the host copy, the fresh export before the deletes, the
/// verify reads, the record): 4.41 s fastest, 5.16 s slowest.
const BUDGET_TAGS_DELETES_RUN_OWN: Duration = Duration::from_secs(21);
/// `deploy_projects_run` with the fakes' time: 4.92 s fastest, 6.16 s
/// slowest.
const BUDGET_TAGS_DELETES_RUN: Duration = Duration::from_secs(25);

// ---- a real export, opt-in (plan 26-14) -------------------------------------

/// The repository root this test file belongs to.
fn repository_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root")
}

/// `path` canonicalized, refused when it is inside `repo_root`: a real export
/// is private and must never be read from (or land in) this public repo.
fn corpus_path(
    path: &std::path::Path,
    repo_root: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    let path = path
        .canonicalize()
        .map_err(|e| format!("IGN_NATIVE_CORPUS does not resolve ({:?})", e.kind()))?;
    if path.starts_with(repo_root) {
        return Err(
            "IGN_NATIVE_CORPUS is inside this repository; a real export stays outside it"
                .to_string(),
        );
    }
    Ok(path)
}

#[test]
fn project_tags_real_corpus_refuses_a_path_inside_the_repository() {
    let root = repository_root();
    assert!(corpus_path(&root, &root).is_err());
    assert!(corpus_path(&root.join("crates"), &root).is_err());
    let outside = tempfile::tempdir().expect("tempdir");
    assert!(corpus_path(outside.path(), &root).is_ok());
}

/// Every file below `dir` as `(path relative to root, bytes)`, symlinks
/// skipped. In memory only.
fn files_below(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<(String, Vec<u8>)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            files_below(root, &path, out);
        } else if meta.is_file()
            && let (Ok(rel), Ok(bytes)) = (path.strip_prefix(root), std::fs::read(&path))
        {
            let rel: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            out.push((rel.join("/"), bytes));
        }
    }
}

/// Opt-in: `IGN_NATIVE_CORPUS=<a repo checkout outside this repository>`
/// reads every owned provider of a real native export, checks its form, and
/// plans it against its own gateway-form export (C12, F-P1), timed. It
/// prints counts and times only: never a provider, file or tag name, never
/// content, and a refusal prints its slug only. Quietly does nothing when the
/// variable is unset.
#[test]
fn project_tags_real_corpus() {
    let Some(path) = std::env::var_os("IGN_NATIVE_CORPUS") else {
        return;
    };
    let _guard = TIMED.blocking_lock();
    let root = match corpus_path(std::path::Path::new(&path), &repository_root()) {
        Ok(root) => root,
        Err(why) => panic!("{why}"),
    };
    let config = native_tags::read_tag_config(&root)
        .unwrap_or_else(|e| panic!("the tag settings refuse: {}", e.code()));
    let mut providers: Vec<String> = std::fs::read_dir(root.join(native_tags::TAGS_DIR))
        .expect("a tags directory")
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.') && config.includes(name))
        .collect();
    providers.sort();
    eprintln!("project_tags_real_corpus: {} provider(s)", providers.len());
    for (index, name) in providers.iter().enumerate() {
        let started = Instant::now();
        let read = native_tags::read_provider_with_raw(&root, name);
        let read_time = started.elapsed();
        let (provider, raw) = match read {
            Ok(read) => read,
            Err(err) => {
                eprintln!(
                    "project_tags_real_corpus: provider #{}: read refused ({})",
                    index + 1,
                    err.code()
                );
                continue;
            }
        };
        let non_canonical = native_tags::check_canonical_form(&provider, &raw).len();
        let mut files = Vec::new();
        files_below(
            &root,
            &root.join(native_tags::TAGS_DIR).join(name),
            &mut files,
        );
        let shape = native_shape::shape_of(&files, name);
        let nodes: BTreeMap<String, serde_json::Value> = provider
            .nodes
            .iter()
            .map(|(p, n)| (p.clone(), n.json.clone()))
            .collect();
        let bytes = serde_json::to_vec(&native_shape::gateway_export(&nodes)).expect("json");
        let started = Instant::now();
        let planned = tag_canon::flatten_export(&bytes, name)
            .map(|live| tag_plan::plan_provider(&provider, &live, None));
        let plan_time = started.elapsed();
        match planned {
            Ok(plan) => eprintln!(
                "project_tags_real_corpus: provider #{}: {} files, {} nodes ({} member stubs, {} type nodes, depth {}), {} non-canonical, read {:.2} s; plan against itself {:.2} s: {} added, {} changed, {} drift, {} same",
                index + 1,
                shape.provider_files,
                provider.nodes.len(),
                shape.member_stubs,
                shape.type_nodes,
                shape.max_depth,
                non_canonical,
                read_time.as_secs_f64(),
                plan_time.as_secs_f64(),
                plan.added.len(),
                plan.changed.len(),
                plan.drift.len(),
                plan.same
            ),
            Err(err) => eprintln!(
                "project_tags_real_corpus: provider #{}: its own export refuses ({})",
                index + 1,
                err.code()
            ),
        }
    }
}
