//! Unit tests of the pure backup decisions (24-05 Task 2).

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use super::*;
use crate::client::deploy_api::SecretNameListing;
use crate::deploy::layout::RelPath;
use crate::deploy::transport::FileBlob;
use crate::deploy::tree::{RenderedTree, sha256_hex};
use crate::deploy::verify::{Expected, ExpectedResource, Remote};

const CANARY: &str = "PAIR-CANARY-TEXT";

fn signals(connected: bool, pending: bool, before: Option<i64>, now: Option<i64>) -> BackupSignals {
    BackupSignals {
        peer_connected: connected,
        sync_pending: pending,
        last_sync_before: before,
        last_sync_now: now,
        project_state: Some("Good".to_string()),
    }
}

fn check(id: BackupCheckId, status: CheckStatus, detail: &str) -> BackupCheck {
    BackupCheck {
        id,
        status,
        detail: detail.to_string(),
        findings: Vec::new(),
    }
}

fn content(status_mode: CheckStatus, status_rb: CheckStatus, status_files: CheckStatus) -> Vec<BackupCheck> {
    vec![
        check(BackupCheckId::Mode, status_mode, "mode"),
        check(BackupCheckId::ReadBack, status_rb, "read back"),
        check(BackupCheckId::Files, status_files, "files"),
    ]
}

/// A tick on which the backup has converged.
fn good() -> TickOutcome {
    TickOutcome {
        signals: Ok(signals(true, false, Some(100), Some(200))),
        exhausted: false,
        content: Some(content(CheckStatus::Pass, CheckStatus::Pass, CheckStatus::Pass)),
        files_checked: 4,
        stable_ticks: 0,
    }
}

fn policy() -> BackupPolicy {
    BackupPolicy {
        budget: Duration::from_secs(10),
        interval: Duration::from_secs(5),
    }
}

// ---- wait_step ---------------------------------------------------------------

#[test]
fn wait_step_budget_is_inclusive() {
    let mut open = good();
    open.content = None;
    let policy = policy();
    // Elapsed equal to the budget has passed.
    assert_eq!(
        wait_step(&open, Duration::from_secs(10), &policy),
        PairWaitStep::TimedOut
    );
    // One millisecond less sleeps for that millisecond.
    assert_eq!(
        wait_step(&open, Duration::from_millis(9_999), &policy),
        PairWaitStep::Sleep(Duration::from_millis(1))
    );
    // A converged tick is Converged even at the budget.
    assert_eq!(
        wait_step(&good(), Duration::from_secs(10), &policy),
        PairWaitStep::Converged
    );
    // The sync-exhausted signal ends the wait at any time (D-10).
    let mut exhausted = open.clone();
    exhausted.exhausted = true;
    assert_eq!(
        wait_step(&exhausted, Duration::ZERO, &policy),
        PairWaitStep::Exhausted
    );
    assert_eq!(
        wait_step(&exhausted, Duration::from_secs(10), &policy),
        PairWaitStep::Exhausted
    );
}

#[test]
fn wait_step_sleep_is_clamped() {
    let mut open = good();
    open.content = None;
    // 2 s left of a 10 s budget, interval 5 s: sleep 2 s.
    assert_eq!(
        wait_step(&open, Duration::from_secs(8), &policy()),
        PairWaitStep::Sleep(Duration::from_secs(2))
    );
    // Plenty left: the interval.
    assert_eq!(
        wait_step(&open, Duration::ZERO, &policy()),
        PairWaitStep::Sleep(Duration::from_secs(5))
    );
    // An interval of zero sleeps 1 ms, never spins.
    let zero = BackupPolicy {
        budget: Duration::from_secs(10),
        interval: Duration::ZERO,
    };
    assert_eq!(
        wait_step(&open, Duration::from_secs(1), &zero),
        PairWaitStep::Sleep(Duration::from_millis(1))
    );
}

#[test]
fn the_default_policy_is_the_published_one() {
    let policy = BackupPolicy::default();
    assert_eq!(policy.budget, Duration::from_secs(BACKUP_TIMEOUT_DEFAULT_S));
    assert_eq!(policy.interval, Duration::from_secs(BACKUP_POLL_S));
    assert_eq!((BACKUP_TIMEOUT_DEFAULT_S, BACKUP_POLL_S), (300, 5));
}

// ---- converged ---------------------------------------------------------------

#[test]
fn converged_needs_every_clause() {
    // K1 a: the timestamp advance is a clause.
    assert_eq!(SYNC_PREDICATE, SyncPredicate::TimestampAdvanced);
    let p = SYNC_PREDICATE;
    assert!(converged(&good(), p));

    let mut t = good();
    t.signals = Ok(signals(false, false, Some(100), Some(200)));
    assert!(!converged(&t, p), "peer not connected");

    let mut t = good();
    t.signals = Ok(signals(true, true, Some(100), Some(200)));
    assert!(!converged(&t, p), "sync still pending");

    let mut t = good();
    t.signals = Ok(signals(true, false, Some(200), Some(200)));
    assert!(!converged(&t, p), "timestamp did not advance");

    let mut t = good();
    t.signals = Ok(signals(true, false, None, None));
    assert!(!converged(&t, p), "never synced is not an advance");

    for (mode, rb, files) in [
        (CheckStatus::Fail, CheckStatus::Pass, CheckStatus::Pass),
        (CheckStatus::Pass, CheckStatus::Fail, CheckStatus::Pass),
        (CheckStatus::Pass, CheckStatus::Pass, CheckStatus::Fail),
    ] {
        let mut t = good();
        t.content = Some(content(mode, rb, files));
        assert!(!converged(&t, p), "{mode:?} {rb:?} {files:?}");
    }

    let mut t = good();
    t.content = None;
    assert!(!converged(&t, p), "content not read yet");

    let mut t = good();
    t.signals = Err("network_error".to_string());
    assert!(!converged(&t, p), "status unreadable");

    // A warn (a REST-only backup's skipped file check) does not block.
    let mut t = good();
    t.content = Some(content(CheckStatus::Pass, CheckStatus::Pass, CheckStatus::Warn));
    assert!(converged(&t, p));
}

#[test]
fn a_stable_tick_count_is_the_other_predicate() {
    let mut t = good();
    // StableTicks ignores the timestamp and counts ticks (K1 b, not chosen).
    t.signals = Ok(signals(true, false, None, None));
    t.stable_ticks = 2;
    assert!(converged(&t, SyncPredicate::StableTicks(2)));
    t.stable_ticks = 1;
    assert!(!converged(&t, SyncPredicate::StableTicks(2)));
}

// ---- classify ----------------------------------------------------------------

#[test]
fn classify_precedence() {
    assert_eq!(classify(&good()), None);

    // unreadable status
    let mut t = good();
    t.signals = Err("network_error".to_string());
    t.exhausted = true;
    assert_eq!(classify(&t), Some(UnsyncedReason::Unreachable));

    // readable status, a file read that could not be answered
    let mut t = good();
    t.content = Some(vec![
        check(BackupCheckId::Mode, CheckStatus::Pass, "mode"),
        check(BackupCheckId::ReadBack, CheckStatus::Pass, "read back"),
        check(
            BackupCheckId::Files,
            CheckStatus::Fail,
            "unreachable: transport_failed",
        ),
    ]);
    assert_eq!(classify(&t), Some(UnsyncedReason::Unreachable));

    // exhausted beats disconnected
    let mut t = good();
    t.signals = Ok(signals(false, false, Some(100), Some(100)));
    t.exhausted = true;
    assert_eq!(classify(&t), Some(UnsyncedReason::SyncExhausted));

    // disconnected beats differing content
    let mut t = good();
    t.signals = Ok(signals(false, false, Some(100), Some(200)));
    t.content = Some(content(CheckStatus::Pass, CheckStatus::Pass, CheckStatus::Fail));
    assert_eq!(classify(&t), Some(UnsyncedReason::NotSyncing));

    // pending and a timestamp that did not move are both not syncing
    let mut t = good();
    t.signals = Ok(signals(true, true, Some(100), Some(200)));
    assert_eq!(classify(&t), Some(UnsyncedReason::NotSyncing));
    let mut t = good();
    t.signals = Ok(signals(true, false, Some(100), Some(100)));
    assert_eq!(classify(&t), Some(UnsyncedReason::NotSyncing));

    // linked, advanced, content differing
    let mut t = good();
    t.content = Some(content(CheckStatus::Pass, CheckStatus::Fail, CheckStatus::Pass));
    assert_eq!(classify(&t), Some(UnsyncedReason::ContentDiffers));
}

#[test]
fn the_report_follows_the_tick() {
    let mut t = good();
    t.content = Some(content(CheckStatus::Pass, CheckStatus::Pass, CheckStatus::Fail));
    let r = report(
        &t,
        Some(100),
        300,
        45,
        Some(ResyncRecord {
            ok: true,
            code: None,
        }),
    );
    assert!(!r.converged);
    assert_eq!(r.reason, Some(UnsyncedReason::ContentDiffers));
    assert_eq!((r.budget_s, r.waited_s, r.files_checked), (300, 45, 4));
    assert_eq!((r.last_sync_before, r.last_sync_after), (Some(100), Some(200)));
    assert_eq!(r.project_state.as_deref(), Some("Good"));
    let ids: Vec<BackupCheckId> = r.checks.iter().map(|c| c.id).collect();
    assert_eq!(
        ids,
        [
            BackupCheckId::Redundancy,
            BackupCheckId::Mode,
            BackupCheckId::ReadBack,
            BackupCheckId::Files
        ]
    );

    let ok = report(&good(), Some(100), 300, 5, None);
    assert!(ok.converged);
    assert_eq!(ok.reason, None);

    // An unreadable status keeps the earlier sample and fails the redundancy check.
    let mut t = good();
    t.signals = Err("network_error".to_string());
    t.content = None;
    let r = report(&t, Some(100), 300, 300, None);
    assert_eq!(r.reason, Some(UnsyncedReason::Unreachable));
    assert_eq!((r.last_sync_before, r.last_sync_after), (Some(100), None));
    assert_eq!(r.checks.len(), 1);
    assert_eq!(r.checks[0].status, CheckStatus::Fail);
}

// ---- content checks ----------------------------------------------------------

fn expected(mode: &str) -> Expected {
    Expected {
        mode: mode.to_string(),
        resources: vec![ExpectedResource::exact(
            "ignition",
            "tag-provider",
            Some("default"),
            serde_json::json!({"a": 1}),
        )],
        not_checked: Vec::new(),
        declared_ids: Vec::new(),
        tag_units: Vec::new(),
        probes: Vec::new(),
    }
}

#[test]
fn the_mode_and_read_back_checks_reuse_the_verify_evaluation() {
    let mode = mode_backup_check(&expected("prod"), Some("prod"));
    assert_eq!((mode.id, mode.status), (BackupCheckId::Mode, CheckStatus::Pass));
    let mode = mode_backup_check(&expected("prod"), None);
    assert_eq!(mode.status, CheckStatus::Fail);

    let mut warnings = Vec::new();
    let mut seen = BTreeMap::new();
    seen.insert(
        "ignition/tag-provider/default".to_string(),
        Remote::Config(serde_json::json!({"a": 1})),
    );
    let (rb, checked) = read_back_backup_check(&expected("prod"), &seen, &mut warnings);
    assert_eq!((rb.id, rb.status, checked), (BackupCheckId::ReadBack, CheckStatus::Pass, 1));
    seen.insert(
        "ignition/tag-provider/default".to_string(),
        Remote::Config(serde_json::json!({"a": 2})),
    );
    let (rb, _) = read_back_backup_check(&expected("prod"), &seen, &mut warnings);
    assert_eq!(rb.status, CheckStatus::Fail);
    assert_eq!(rb.findings.len(), 1);
}

#[test]
fn an_unanswered_read_is_an_unreachable_check() {
    let c = unreachable_check(BackupCheckId::Files, "transport_failed");
    assert_eq!((c.id, c.status), (BackupCheckId::Files, CheckStatus::Fail));
    assert_eq!(c.detail, "unreachable: transport_failed");
}

// ---- compare_files -----------------------------------------------------------

fn path(s: &str) -> RelPath {
    RelPath::new(s).expect("a path")
}

fn blob(bytes: &[u8]) -> FileBlob {
    FileBlob {
        bytes: bytes.to_vec(),
        sha256: sha256_hex(bytes),
    }
}

fn files(entries: &[(&str, &[u8])]) -> BTreeMap<RelPath, FileBlob> {
    entries.iter().map(|(p, b)| (path(p), blob(b))).collect()
}

fn set(paths: &[&str]) -> BTreeSet<RelPath> {
    paths.iter().map(|p| path(p)).collect()
}

#[test]
fn compare_files_bytes() {
    let master = files(&[
        ("m/t/a/config.json", b"{\"x\":1}"),
        ("m/t/b/config.json", b"{\"y\":2}"),
        ("m/t/c/config.json", b"line\n"),
        ("m/t/d/config.json", b"{}"),
    ]);
    let present = set(&[
        "m/t/a/config.json",
        "m/t/b/config.json",
        "m/t/c/config.json",
        "m/t/d/config.json",
    ]);

    // identical: clean, `checked` is the present-set size
    let same = compare_files(&master, &master.clone(), &present, &BTreeSet::new());
    assert_eq!(same.checked, 4);
    assert!(same.differing.is_empty() && same.missing.is_empty() && same.unexpected.is_empty());
    assert_eq!(files_check(&same).status, CheckStatus::Pass);

    // one byte differing, one file missing on the backup, one delete still there
    let backup = files(&[
        ("m/t/a/config.json", b"{\"x\":2}"),
        ("m/t/c/config.json", b"line\n"),
        ("m/t/d/config.json", b"{}"),
        ("m/t/gone/config.json", b"{}"),
    ]);
    let cmp = compare_files(&master, &backup, &present, &set(&["m/t/gone/config.json"]));
    assert_eq!(cmp.checked, 4);
    assert_eq!(cmp.differing, ["m/t/a/config.json"]);
    assert_eq!(cmp.missing, ["m/t/b/config.json"]);
    assert_eq!(cmp.unexpected, ["m/t/gone/config.json"]);
    let check = files_check(&cmp);
    assert_eq!(check.status, CheckStatus::Fail);
    let subjects: Vec<&str> = check.findings.iter().map(|f| f.subject.as_str()).collect();
    assert_eq!(
        subjects,
        ["m/t/a/config.json", "m/t/b/config.json", "m/t/gone/config.json"]
    );

    // CRLF against LF differs: no newline normalization (edge DPAIR-03 encoding)
    let crlf = files(&[("m/t/c/config.json", b"line\r\n")]);
    let cmp = compare_files(
        &files(&[("m/t/c/config.json", b"line\n")]),
        &crlf,
        &set(&["m/t/c/config.json"]),
        &BTreeSet::new(),
    );
    assert_eq!(cmp.differing, ["m/t/c/config.json"]);

    // paths come out byte-sorted whatever the map order
    let master = files(&[("z/a", b"1"), ("a/z", b"1"), ("m/m", b"1")]);
    let backup = files(&[("z/a", b"2"), ("a/z", b"2"), ("m/m", b"2")]);
    let cmp = compare_files(&master, &backup, &set(&["z/a", "a/z", "m/m"]), &BTreeSet::new());
    assert_eq!(cmp.differing, ["a/z", "m/m", "z/a"]);
}

#[test]
fn compare_files_lists_a_bounded_number_of_paths() {
    let many: Vec<String> = (0..25).map(|i| format!("m/t/u{i:02}/config.json")).collect();
    let master: BTreeMap<RelPath, FileBlob> = many.iter().map(|p| (path(p), blob(b"1"))).collect();
    let cmp = compare_files(
        &master,
        &BTreeMap::new(),
        &many.iter().map(|p| path(p)).collect(),
        &BTreeSet::new(),
    );
    assert_eq!(cmp.missing.len(), 25);
    let check = files_check(&cmp);
    assert!(check.findings.len() <= crate::error::APPLY_LIST_SHOWN);
    assert!(check.detail.contains("25"), "{}", check.detail);
}

#[test]
fn only_resource_json_is_compared_as_a_document() {
    // K2 b: exactly one row, `resource.json`, no member excluded.
    assert_eq!(FILE_NORMALIZATION, &[("resource.json", "")]);

    let compact = br#"{"uuid":"u","lastModificationSignature":"s","attributes":{"a":1,"b":[1,2]}}"#;
    let reordered = br#"{"lastModificationSignature":"s","uuid":"u","attributes":{"b":[1,2],"a":1}}"#;
    let pretty = b"{\n  \"uuid\": \"u\",\n  \"lastModificationSignature\": \"s\",\n  \"attributes\": {\n    \"a\": 1,\n    \"b\": [1, 2]\n  }\n}\n";
    let changed = br#"{"uuid":"u","lastModificationSignature":"T","attributes":{"a":1,"b":[1,2]}}"#;
    let p = "ignition/tag-provider/default/resource.json";
    let one = |master: &[u8], backup: &[u8]| {
        compare_files(
            &files(&[(p, master)]),
            &files(&[(p, backup)]),
            &set(&[p]),
            &BTreeSet::new(),
        )
    };

    assert!(one(compact, reordered).differing.is_empty(), "reordered");
    assert!(one(compact, pretty).differing.is_empty(), "pretty-printed");
    assert_eq!(one(compact, changed).differing, [p], "a changed value");
    assert_eq!(one(compact, b"not json").differing, [p], "not a document");
    assert_eq!(one(b"not json", b"not json ").differing, [p]);
    // a member missing is a different document, not an excluded member
    assert_eq!(one(compact, br#"{"uuid":"u"}"#).differing, [p]);

    // every other file stays byte-equal, even when the JSON is equal
    let c = "ignition/tag-provider/default/config.json";
    let cmp = compare_files(
        &files(&[(c, br#"{"a":1,"b":2}"#)]),
        &files(&[(c, br#"{"b":2,"a":1}"#)]),
        &set(&[c]),
        &BTreeSet::new(),
    );
    assert_eq!(cmp.differing, [c], "config.json is byte-equal only");

    // only the file NAME is a row: a directory named resource.json is not
    let d = "ignition/tag-provider/resource.json/config.json";
    let cmp = compare_files(
        &files(&[(d, br#"{"a":1,"b":2}"#)]),
        &files(&[(d, br#"{"b":2,"a":1}"#)]),
        &set(&[d]),
        &BTreeSet::new(),
    );
    assert_eq!(cmp.differing, [d]);
}

#[test]
fn a_rest_only_backup_skips_the_file_check_with_a_warning() {
    let c = files_skipped_check();
    assert_eq!((c.id, c.status), (BackupCheckId::Files, CheckStatus::Warn));
    assert!(c.detail.contains("nodes.backup"), "{}", c.detail);
    assert!(!c.detail.contains("D-0"), "no planning ids in user text");
}

// ---- secrets -----------------------------------------------------------------

fn refs(pairs: &[(&str, &str)]) -> BTreeSet<(String, String)> {
    pairs
        .iter()
        .map(|(p, n)| ((*p).to_string(), (*n).to_string()))
        .collect()
}

fn names(list: &[&str]) -> SecretNameListing {
    SecretNameListing::Listed(list.iter().map(|n| (*n).to_string()).collect())
}

fn listings(entries: Vec<(&str, SecretNameListing)>) -> BTreeMap<String, SecretNameListing> {
    entries
        .into_iter()
        .map(|(p, l)| (p.to_string(), l))
        .collect()
}

fn missing(report: &SecretNodeReport) -> Vec<(&str, &str)> {
    report
        .missing
        .iter()
        .map(|m| (m.provider.as_str(), m.name.as_str()))
        .collect()
}

#[test]
fn check_secret_names_cases() {
    let none = BTreeSet::new();
    let referenced = refs(&[("vault", "b"), ("file", "z"), ("file", "a"), ("vault", "a")]);

    // all present
    let all = listings(vec![("file", names(&["a", "z"])), ("vault", names(&["a", "b"]))]);
    let r = check_secret_names("master", &referenced, &all, &none, SecretCheckMode::Verify);
    assert_eq!(r.status, CheckStatus::Pass);
    assert_eq!((r.providers_checked, r.node.as_str()), (2, "master"));
    assert!(r.missing.is_empty() && r.unverifiable.is_empty());

    // missing names are sorted by provider then name
    let some = listings(vec![("file", names(&["a"])), ("vault", names(&[]))]);
    let r = check_secret_names("backup", &referenced, &some, &none, SecretCheckMode::Verify);
    assert_eq!(r.status, CheckStatus::Fail);
    assert_eq!(missing(&r), [("file", "z"), ("vault", "a"), ("vault", "b")]);

    // K7: a provider that cannot list is unverifiable, a warn when nothing is missing
    let cannot = listings(vec![("file", names(&["a", "z"])), ("vault", SecretNameListing::CannotList)]);
    let r = check_secret_names("master", &referenced, &cannot, &none, SecretCheckMode::Verify);
    assert_eq!(r.status, CheckStatus::Warn);
    assert_eq!(r.unverifiable, ["vault"]);
    assert!(r.missing.is_empty());
    // ... and a failure when something else is missing
    let both = listings(vec![("file", names(&["a"])), ("vault", SecretNameListing::CannotList)]);
    let r = check_secret_names("master", &referenced, &both, &none, SecretCheckMode::Verify);
    assert_eq!(r.status, CheckStatus::Fail);
    assert_eq!((missing(&r), r.unverifiable.as_slice()), (vec![("file", "z")], ["vault".to_string()].as_slice()));

    // K8: a provider the render declares that answers 404 is skipped in a preflight ...
    let declared: BTreeSet<String> = ["vault".to_string()].into();
    let absent = listings(vec![("file", names(&["a", "z"])), ("vault", SecretNameListing::NoProvider)]);
    let r = check_secret_names("backup", &referenced, &absent, &declared, SecretCheckMode::Preflight);
    assert!(r.missing.is_empty(), "{:?}", r.missing);
    assert_eq!(r.skipped_providers, ["vault"]);
    assert_eq!(r.status, CheckStatus::Pass);
    // ... but verify requires it
    let r = check_secret_names("backup", &referenced, &absent, &declared, SecretCheckMode::Verify);
    assert_eq!(missing(&r), [("vault", "a"), ("vault", "b")]);
    assert!(r.skipped_providers.is_empty());
    assert_eq!(r.status, CheckStatus::Fail);
    // a provider the render does NOT declare that is absent is missing in a preflight
    let r = check_secret_names("backup", &referenced, &absent, &none, SecretCheckMode::Preflight);
    assert_eq!(missing(&r), [("vault", "a"), ("vault", "b")]);
    // a provider with no listing at all reads as absent
    let r = check_secret_names("backup", &refs(&[("ghost", "x")]), &BTreeMap::new(), &none, SecretCheckMode::Verify);
    assert_eq!(missing(&r), [("ghost", "x")]);

    // no reference at all: pass, nothing checked
    let r = check_secret_names("master", &BTreeSet::new(), &BTreeMap::new(), &none, SecretCheckMode::Verify);
    assert_eq!((r.status, r.providers_checked), (CheckStatus::Pass, 0));

    // a provider listing {"secrets":[]} makes every name referenced in it missing
    let empty = listings(vec![("file", names(&[]))]);
    let r = check_secret_names("master", &refs(&[("file", "a"), ("file", "b")]), &empty, &none, SecretCheckMode::Verify);
    assert_eq!(missing(&r), [("file", "a"), ("file", "b")]);
    assert_eq!(r.providers_checked, 1);

    // K9: exact, case-sensitive
    let cased = listings(vec![("file", names(&["name"]))]);
    let r = check_secret_names("master", &refs(&[("file", "Name")]), &cased, &none, SecretCheckMode::Verify);
    assert_eq!(missing(&r), [("file", "Name")]);
}

#[test]
fn secret_reports_list_the_master_before_the_backup() {
    let none = BTreeSet::new();
    let a = check_secret_names("backup", &BTreeSet::new(), &BTreeMap::new(), &none, SecretCheckMode::Verify);
    let b = check_secret_names("master", &BTreeSet::new(), &BTreeMap::new(), &none, SecretCheckMode::Verify);
    let report = SecretNamesReport::ordered(vec![a, b]);
    let nodes: Vec<&str> = report.nodes.iter().map(|n| n.node.as_str()).collect();
    assert_eq!(nodes, ["master", "backup"]);
}

#[test]
fn a_secret_report_is_names_only_in_the_published_shape() {
    let none = BTreeSet::new();
    let some = listings(vec![("file", names(&["a"])), ("vault", SecretNameListing::CannotList)]);
    let r = check_secret_names(
        "master",
        &refs(&[("file", "z"), ("vault", "q")]),
        &some,
        &none,
        SecretCheckMode::Verify,
    );
    let value = serde_json::to_value(&r).expect("serializes");
    let mut keys: Vec<&str> = value.as_object().expect("object").keys().map(String::as_str).collect();
    keys.sort_unstable();
    // V7: skipped_providers is omitted when empty
    assert_eq!(keys, ["missing", "node", "providers_checked", "status", "unverifiable"]);
    assert_eq!(value["missing"][0], serde_json::json!({"provider": "file", "name": "z"}));
    assert_eq!(value["unverifiable"], serde_json::json!(["vault"]));
    assert_eq!(value["status"], "fail");
}

fn tree(entries: &[(&str, &str)]) -> RenderedTree {
    RenderedTree {
        files: entries
            .iter()
            .map(|(p, b)| (path(p), b.as_bytes().to_vec()))
            .collect(),
    }
}

#[test]
fn collect_referenced_secrets_and_declared_providers() {
    let reference = |p: &str, n: &str| {
        format!(r#"{{"type":"Referenced","data":{{"providerName":"{p}","secretName":"{n}"}}}}"#)
    };
    let t = tree(&[
        (
            "ignition/database-connection/main/config.json",
            &format!(r#"{{"password":{}}}"#, reference("file", "db-pass")),
        ),
        (
            "ignition/smtp/mail/config.json",
            &format!(r#"{{"a":[{},{}]}}"#, reference("vault", "k"), reference("file", "db-pass")),
        ),
        (
            "ignition/tag-provider/default/tags.json",
            &format!(r#"{{"tags":[{{"p":{}}}]}}"#, reference("file", "tag-secret")),
        ),
        ("ignition/secret-provider/file/config.json", r#"{"profile":{"type":"INTERNAL"}}"#),
        ("ignition/secret-provider/file/resource.json", "{}"),
        ("ignition/secret-provider/vault/config.json", "{}"),
        ("ignition/secret-provider/vault/secret", "not json at all"),
        ("ignition/secret-provider", "{}"),
        ("ignition/other/x/notes.txt", "Referenced providerName"),
        ("ignition/other/x/broken.json", "{not json"),
        ("ignition/other/x/lookalike.json", r#"{"type":"referenced","data":{"providerName":"p","secretName":"n"}}"#),
    ]);
    assert_eq!(
        collect_referenced_secrets(&t),
        refs(&[("file", "db-pass"), ("file", "tag-secret"), ("vault", "k")])
    );
    let providers: Vec<String> = declared_secret_providers(&t).into_iter().collect();
    assert_eq!(providers, ["file", "vault"]);
}

// ---- gateway text ------------------------------------------------------------

#[test]
fn report_carries_no_gateway_text() {
    // A status whose members carry gateway text: a `lastSyncError`, an unknown
    // member and a hostile `projectState`.
    let status: RedundancyStatusWire = serde_json::from_value(serde_json::json!({
        "role": "Backup", "projectState": format!("Good\n{CANARY}{}", "x".repeat(200)),
        "activityLevel": "Cold", "peerConnected": true, "syncPending": false,
        "uptime": -1, "lastSyncTimestamp": 200,
        "lastSyncError": CANARY, "somethingNew": {"message": CANARY}
    }))
    .expect("parses");
    let signals = BackupSignals::from_status(Some(100), &status);
    assert!(
        signals.project_state.as_deref().is_some_and(|s| s.chars().count() <= 64
            && !s.chars().any(char::is_control)),
        "{:?}",
        signals.project_state
    );

    // The log line that signals exhaustion carries the canary: the matcher
    // answers a bool and nothing else.
    let line = format!(
        "[config-resource] State sync request failed. Sync operation has failed 5 time(s), which exceeds the max attempt count. No further synchronization attempts will be made. redundant-provider=config-resource {CANARY}"
    );
    assert!(is_sync_exhausted(&line));

    let tick = TickOutcome {
        signals: Ok(signals),
        exhausted: is_sync_exhausted(&line),
        content: Some(vec![
            unreachable_check(BackupCheckId::Files, "transport_failed"),
        ]),
        files_checked: 0,
        stable_ticks: 0,
    };
    let rendered = serde_json::to_string(&report(&tick, Some(100), 300, 10, None)).expect("json");
    // the status word itself is kept, truncated and stripped of controls
    assert!(rendered.contains("Good"), "{rendered}");
    assert!(!rendered.contains(CANARY), "{rendered}");

    // the secret-name report holds names and providers only
    let r = check_secret_names(
        "master",
        &refs(&[("file", "a")]),
        &listings(vec![("file", names(&[]))]),
        &BTreeSet::new(),
        SecretCheckMode::Verify,
    );
    assert!(!serde_json::to_string(&SecretNamesReport { nodes: vec![r] })
        .expect("json")
        .contains(CANARY));
}

// ---- is_sync_exhausted -------------------------------------------------------

#[test]
fn is_sync_exhausted_matches_the_spike_line() {
    // The verbatim line of the spike's results/RIG.md (24-RESEARCH, 24-02 plan).
    let spike = "E [R.S.SyncManager] [config-resource] State sync request failed. Sync operation has failed 5 time(s), which exceeds the max attempt count. No further synchronization attempts will be made. redundant-provider=config-resource";
    assert!(is_sync_exhausted(spike));
    // the message alone, as the logs route returns it
    assert!(is_sync_exhausted(
        "State sync request failed. Sync operation has failed 5 time(s), which exceeds the max attempt count. No further synchronization attempts will be made."
    ));
    // the other lines of the same incident do not match
    assert!(!is_sync_exhausted("Active deployment mode differs from peer node!"));
    assert!(!is_sync_exhausted(
        "[config-resource] After applying diff, new local system snapshot did not match remote system snapshot"
    ));
    assert!(!is_sync_exhausted(""));
}
