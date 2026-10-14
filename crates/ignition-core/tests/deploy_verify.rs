//! `deploy verify` (22-10): the pure six-check engine (Task 1) and the
//! read-only `deploy_verify` action over fakes (Task 2).
//!
//! The engine tests build [`Expected`] and [`ObservedState`] directly, so
//! each check can be failed alone. The rules under test are the 22-09 rig
//! decisions CV1 (log match), CV2 (read-back table N1 to N3 and the two skip
//! classes) and CV3 (post-scan bytes), and the confirmed 22-03 vocabulary.

use std::collections::BTreeMap;

use ignition_core::client::scan::ScanStatus;
use ignition_core::deploy::plan::PlanStatus;
use ignition_core::deploy::verify::{
    Check, CheckId, CheckStatus, Compare, Expected, ExpectedResource, LogLine, ObservedState,
    ProbeReading, Remote, VERIFY_FAILED_SLUG, VerifyReport, evaluate, log_window_start,
};
use serde_json::{Value, json};

const CANARY: &str = "CANARY-7f3a9e-DO-NOT-ECHO";

const DB: &str = "ignition/database-translator/POSTGRES";
const TAG_UNIT: &str = "ignition/tag-definition/default/Plant";

fn scan(active: bool) -> ScanStatus {
    ScanStatus {
        scan_active: active,
        last_scan_timestamp: 1_792_000_000_000,
        last_scan_duration: Some(400),
        extra: BTreeMap::new(),
    }
}

fn expected() -> Expected {
    Expected {
        mode: "production".to_string(),
        resources: vec![ExpectedResource::exact(
            "ignition",
            "database-translator",
            Some("POSTGRES"),
            json!({"name": "POSTGRES", "nested": {"a": 1, "b": [1, 2]}}),
        )],
        not_checked: Vec::new(),
        declared_ids: vec![DB.to_string()],
        tag_units: vec![TAG_UNIT.to_string()],
        probes: vec![
            "[default]Heartbeat".to_string(),
            "[default]Pump".to_string(),
        ],
    }
}

fn observed() -> ObservedState {
    ObservedState {
        scan: scan(false),
        mode: Some("production".to_string()),
        read_back: BTreeMap::from([(
            DB.to_string(),
            Remote::Config(json!({"nested": {"b": [1, 2], "a": 1}, "name": "POSTGRES"})),
        )]),
        files: vec![(TAG_UNIT.to_string(), PlanStatus::Same)],
        logs: Ok(Vec::new()),
        probes: Some(Ok(vec![
            reading("[default]Heartbeat", "Good"),
            reading("[default]Pump", "Good_Provisional"),
        ])),
    }
}

fn reading(path: &str, quality: &str) -> ProbeReading {
    ProbeReading {
        path: path.to_string(),
        quality: quality.to_string(),
    }
}

fn check(report: &VerifyReport, id: CheckId) -> &Check {
    report
        .checks
        .iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("check {id:?} missing"))
}

fn statuses(report: &VerifyReport) -> Vec<(CheckId, CheckStatus)> {
    report.checks.iter().map(|c| (c.id, c.status)).collect()
}

fn codes(report: &VerifyReport, id: CheckId) -> Vec<&'static str> {
    check(report, id).findings.iter().map(|f| f.code).collect()
}

fn unloaded_line() -> LogLine {
    LogLine {
        timestamp: 1_792_000_000_050,
        logger: "NamedResourceHandler-ignition/database-translator".to_string(),
        message: format!("Resource {DB} does not have required UUID attribute, will not be loaded"),
    }
}

// ---- the six checks, alone and together -------------------------------------

#[test]
fn all_pass_together() {
    let report = evaluate(&expected(), &observed());
    assert!(report.ok, "{report:#?}");
    assert_eq!(report.slug, None);
    assert_eq!(report.resources_checked, 1);
    assert_eq!(
        statuses(&report),
        vec![
            (CheckId::Scan, CheckStatus::Pass),
            (CheckId::Mode, CheckStatus::Pass),
            (CheckId::ReadBack, CheckStatus::Pass),
            (CheckId::Files, CheckStatus::Pass),
            (CheckId::Loaded, CheckStatus::Pass),
            (CheckId::Probes, CheckStatus::Pass),
        ]
    );
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

#[test]
fn each_check_fails_alone() {
    type Break = fn(&mut ObservedState);
    let cases: [(CheckId, &str, Break); 6] = [
        (CheckId::Scan, "scan_active", |o| o.scan = scan(true)),
        (CheckId::Mode, "mode_mismatch", |o| {
            o.mode = Some("staging".to_string());
        }),
        (CheckId::ReadBack, "resource_missing", |o| {
            o.read_back.insert(DB.to_string(), Remote::Missing);
        }),
        (CheckId::Files, "files_differ", |o| {
            o.files = vec![(TAG_UNIT.to_string(), PlanStatus::Changed)];
        }),
        (CheckId::Loaded, "unloaded_resource", |o| {
            o.logs = Ok(vec![unloaded_line()]);
        }),
        (CheckId::Probes, "tag_probe_not_good", |o| {
            o.probes = Some(Ok(vec![
                reading("[default]Heartbeat", "Bad_NotFound"),
                reading("[default]Pump", "Good"),
            ]));
        }),
    ];
    for (id, code, breakage) in cases {
        let mut obs = observed();
        breakage(&mut obs);
        let report = evaluate(&expected(), &obs);
        assert!(!report.ok, "{id:?}: {report:#?}");
        assert_eq!(report.slug, Some(VERIFY_FAILED_SLUG), "{id:?}");
        for c in &report.checks {
            if c.id == id {
                assert_eq!(c.status, CheckStatus::Fail, "{id:?}");
                assert_eq!(codes(&report, id), vec![code], "{id:?}");
            } else {
                // The running scan makes the log read unobservable (warn); no
                // other check may fail because of this one.
                assert_ne!(c.status, CheckStatus::Fail, "{id:?} broke {:?}", c.id);
            }
        }
    }
}

#[test]
fn checks_are_always_six_in_fixed_order() {
    for obs in [observed(), {
        let mut o = observed();
        o.logs = Err("forbidden".to_string());
        o.probes = None;
        o
    }] {
        let ids: Vec<CheckId> = evaluate(&expected(), &obs)
            .checks
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(
            ids,
            vec![
                CheckId::Scan,
                CheckId::Mode,
                CheckId::ReadBack,
                CheckId::Files,
                CheckId::Loaded,
                CheckId::Probes
            ]
        );
    }
}

#[test]
fn mode_none_is_a_mismatch() {
    let mut obs = observed();
    obs.mode = None;
    let report = evaluate(&expected(), &obs);
    assert_eq!(codes(&report, CheckId::Mode), vec!["mode_mismatch"]);
}

// ---- read_back (CV2) ----------------------------------------------------------

fn one_resource(compare: Compare) -> Expected {
    let mut e = expected();
    e.resources = vec![ExpectedResource::new(
        "ignition",
        "database-translator",
        Some("POSTGRES"),
        compare,
    )];
    e
}

fn read_back_with(config: Value) -> ObservedState {
    let mut o = observed();
    o.read_back.insert(DB.to_string(), Remote::Config(config));
    o
}

#[test]
fn read_back_key_order_ignored() {
    let e = one_resource(Compare::Exact(
        json!({"a": 1, "b": {"c": [1, {"d": 2, "e": 3}]}}),
    ));
    let o = read_back_with(json!({"b": {"c": [1, {"e": 3, "d": 2}]}, "a": 1.0}));
    let report = evaluate(&e, &o);
    assert_eq!(
        check(&report, CheckId::ReadBack).status,
        CheckStatus::Pass,
        "{report:#?}"
    );
}

#[test]
fn read_back_reports_paths_not_values() {
    let e = one_resource(Compare::Exact(
        json!({"outer": {"inner": "expected-value-XYZ"}, "same": 1, "list": [1, 2]}),
    ));
    let o = read_back_with(
        json!({"outer": {"inner": "observed-value-QRS"}, "same": 1, "list": [1, 3]}),
    );
    let report = evaluate(&e, &o);
    let c = check(&report, CheckId::ReadBack);
    assert_eq!(c.status, CheckStatus::Fail);
    assert_eq!(c.findings.len(), 1);
    assert_eq!(c.findings[0].code, "resource_differs");
    assert_eq!(c.findings[0].subject, DB);
    assert!(c.findings[0].detail.contains("outer.inner"), "{c:?}");
    assert!(c.findings[0].detail.contains("list.1"), "{c:?}");
    let text = serde_json::to_string(&report).unwrap();
    for value in ["expected-value-XYZ", "observed-value-QRS"] {
        assert!(!text.contains(value), "{value} leaked: {text}");
        assert!(!format!("{report:?}").contains(value));
    }
}

#[test]
fn read_back_n1_null_members_of_the_file_are_dropped() {
    let e = one_resource(Compare::Exact(
        json!({"keep": 1, "createAutoIncSequence": null, "nested": {"x": null, "y": 2}}),
    ));
    let ok = evaluate(&e, &read_back_with(json!({"keep": 1, "nested": {"y": 2}})));
    assert_eq!(check(&ok, CheckId::ReadBack).status, CheckStatus::Pass);

    // A REST null member the file does not carry still differs.
    let bad = evaluate(
        &e,
        &read_back_with(json!({"keep": 1, "nested": {"y": 2}, "createAutoIncSequence": null})),
    );
    let c = check(&bad, CheckId::ReadBack);
    assert_eq!(c.status, CheckStatus::Fail);
    assert!(c.findings[0].detail.contains("createAutoIncSequence"));
}

#[test]
fn read_back_one_differing_value_fails() {
    let e = one_resource(Compare::Exact(json!({"a": {"b": {"c": 1}}})));
    let report = evaluate(&e, &read_back_with(json!({"a": {"b": {"c": 2}}})));
    let c = check(&report, CheckId::ReadBack);
    assert_eq!(c.status, CheckStatus::Fail);
    assert!(c.findings[0].detail.contains("a.b.c"));
    assert!(!report.ok);
}

#[test]
fn read_back_n2_security_levels_is_a_name_keyed_subset() {
    let file = json!({"securityLevels": [
        {"name": "Authenticated", "children": [{"name": "Roles", "description": "r"}]}
    ]});
    let rest = json!({"securityLevels": [
        {"name": "Public", "children": []},
        {"name": "Authenticated", "description": "", "children": [
            {"name": "Extra", "children": []},
            {"name": "Roles", "description": "r", "children": []}
        ]}
    ]});
    let mut e = expected();
    e.resources = vec![ExpectedResource::new(
        "ignition",
        "security-levels",
        None,
        Compare::NameKeyedSubset(file.clone()),
    )];
    let key = "ignition/security-levels";
    let mut o = observed();
    o.read_back
        .insert(key.to_string(), Remote::Config(rest.clone()));
    let report = evaluate(&e, &o);
    assert_eq!(
        check(&report, CheckId::ReadBack).status,
        CheckStatus::Pass,
        "{report:#?}"
    );

    // A file level the REST tree lacks fails, naming the level.
    let mut file_bad = file;
    file_bad["securityLevels"][0]["children"][0]["name"] = json!("Gone");
    e.resources[0].compare = Compare::NameKeyedSubset(file_bad);
    let report = evaluate(&e, &o);
    let c = check(&report, CheckId::ReadBack);
    assert_eq!(c.status, CheckStatus::Fail);
    assert!(c.findings[0].detail.contains("Gone"), "{c:?}");
}

#[test]
fn read_back_n3_attributes_only_unit_ignores_rest_only_keys() {
    let attrs =
        json!({"caseInsensitive": false, "ignoreTags": true, "lastModification": {"actor": "x"}});
    let Value::Object(attrs) = attrs else {
        unreachable!()
    };
    let mut e = expected();
    e.resources = vec![ExpectedResource::new(
        "ignition",
        "translations",
        None,
        Compare::Attributes(attrs),
    )];
    let key = "ignition/translations";
    let mut o = observed();
    o.read_back.insert(
        key.to_string(),
        Remote::Config(json!({"caseInsensitive": false, "ignoreTags": true, "terms": {}})),
    );
    let report = evaluate(&e, &o);
    assert_eq!(
        check(&report, CheckId::ReadBack).status,
        CheckStatus::Pass,
        "{report:#?}"
    );
    o.read_back.insert(
        key.to_string(),
        Remote::Config(json!({"caseInsensitive": false, "ignoreTags": false, "terms": {}})),
    );
    let report = evaluate(&e, &o);
    let c = check(&report, CheckId::ReadBack);
    assert_eq!(c.status, CheckStatus::Fail);
    assert!(c.findings[0].detail.contains("ignoreTags"));
}

#[test]
fn read_back_skips_system_collection_and_no_route_types_with_a_warning() {
    let mut e = expected();
    e.not_checked = vec![(
        "ignition/images".to_string(),
        "no REST read route".to_string(),
    )];
    let mut o = observed();
    o.read_back.insert(DB.to_string(), Remote::System);
    let report = evaluate(&e, &o);
    assert!(report.ok, "{report:#?}");
    assert_eq!(report.resources_checked, 0, "skipped items are not checked");
    assert!(
        report.warnings.iter().any(|w| w.contains(DB)),
        "{:?}",
        report.warnings
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("ignition/images")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn read_back_vacuous() {
    let mut e = expected();
    e.resources.clear();
    let mut o = observed();
    o.read_back.clear();
    let report = evaluate(&e, &o);
    let c = check(&report, CheckId::ReadBack);
    assert_eq!(c.status, CheckStatus::Pass);
    assert_eq!(report.resources_checked, 0);
    assert!(
        report.warnings.iter().any(|w| w.contains("read_back")),
        "{:?}",
        report.warnings
    );
    assert!(report.ok);
}

// ---- files (CV3) ----------------------------------------------------------------

#[test]
fn files_every_non_same_status_differs_and_names_the_unit() {
    for status in [PlanStatus::Changed, PlanStatus::Added, PlanStatus::Removed] {
        let mut o = observed();
        o.files = vec![(TAG_UNIT.to_string(), status)];
        let report = evaluate(&expected(), &o);
        let c = check(&report, CheckId::Files);
        assert_eq!(c.status, CheckStatus::Fail, "{status:?}");
        assert_eq!(c.findings[0].subject, TAG_UNIT);
        assert!(c.findings[0].detail.contains(status.word()));
    }
    let mut o = observed();
    o.files.clear();
    let report = evaluate(&expected(), &o);
    assert_eq!(codes(&report, CheckId::Files), vec!["files_differ"]);
}

#[test]
fn files_without_tag_units_passes() {
    let mut e = expected();
    e.tag_units.clear();
    let mut o = observed();
    o.files.clear();
    let report = evaluate(&e, &o);
    assert_eq!(check(&report, CheckId::Files).status, CheckStatus::Pass);
}

// ---- loaded (CV1) ----------------------------------------------------------------

#[test]
fn logs_unavailable_fails() {
    let mut o = observed();
    o.logs = Err("forbidden".to_string());
    let report = evaluate(&expected(), &o);
    let c = check(&report, CheckId::Loaded);
    assert_eq!(c.status, CheckStatus::Fail);
    assert_eq!(codes(&report, CheckId::Loaded), vec!["logs_unavailable"]);
    assert!(c.findings[0].detail.contains("forbidden"));
}

#[test]
fn log_hit_requires_declared_resource() {
    let mut o = observed();
    o.logs = Ok(vec![LogLine {
        timestamp: 1,
        logger: "NamedResourceHandler-ignition/database-connection".to_string(),
        message: "Resource ignition/database-connection/Other does not have required UUID attribute, will not be loaded".to_string(),
    }]);
    let report = evaluate(&expected(), &o);
    assert_eq!(check(&report, CheckId::Loaded).status, CheckStatus::Pass);
}

#[test]
fn log_hit_needs_the_phrase_and_the_whole_name() {
    // Right resource, but a different message: no hit.
    let mut o = observed();
    o.logs = Ok(vec![LogLine {
        timestamp: 1,
        logger: "x".to_string(),
        message: format!("Resource {DB} loaded"),
    }]);
    assert_eq!(
        check(&evaluate(&expected(), &o), CheckId::Loaded).status,
        CheckStatus::Pass
    );
    // A declared name that is a prefix of the logged name is not that resource.
    let mut e = expected();
    e.declared_ids = vec!["ignition/database-connection/Prod".to_string()];
    let mut o = observed();
    o.logs = Ok(vec![LogLine {
        timestamp: 1,
        logger: "x".to_string(),
        message: "Resource ignition/database-connection/Prod Backup does not have required UUID attribute, will not be loaded".to_string(),
    }]);
    // The phrase alone cannot tell "Prod" from "Prod Backup" when both are
    // declared: the longest declared id wins the line.
    e.declared_ids
        .push("ignition/database-connection/Prod Backup".to_string());
    e.declared_ids.sort();
    let report = evaluate(&e, &o);
    let c = check(&report, CheckId::Loaded);
    assert_eq!(c.findings.len(), 1, "{c:?}");
    assert_eq!(
        c.findings[0].subject,
        "ignition/database-connection/Prod Backup"
    );
}

#[test]
fn log_hit_is_reported_once_with_timestamp_and_logger_only() {
    let mut o = observed();
    let mut second = unloaded_line();
    second.timestamp += 9;
    second.message.push_str(CANARY);
    o.logs = Ok(vec![second, unloaded_line()]);
    let report = evaluate(&expected(), &o);
    let c = check(&report, CheckId::Loaded);
    assert_eq!(c.findings.len(), 1, "duplicates count once");
    assert_eq!(c.findings[0].code, "unloaded_resource");
    assert_eq!(c.findings[0].subject, DB);
    assert!(c.findings[0].detail.contains("1792000000050"), "{c:?}");
    assert!(c.findings[0].detail.contains("NamedResourceHandler"));
}

#[test]
fn loaded_is_unknown_while_a_scan_runs_and_never_pass() {
    let mut o = observed();
    o.scan = scan(true);
    let report = evaluate(&expected(), &o);
    let c = check(&report, CheckId::Loaded);
    assert_eq!(c.status, CheckStatus::Warn);
    assert!(c.findings.is_empty());
    assert!(!report.ok, "the scan check still fails");
    assert!(report.warnings.iter().any(|w| w.contains("loaded")));
}

#[test]
fn loaded_pass_states_its_limit() {
    let report = evaluate(&expected(), &observed());
    let c = check(&report, CheckId::Loaded);
    assert_eq!(c.status, CheckStatus::Pass);
    assert!(c.detail.contains("last scan"), "{}", c.detail);
}

#[test]
fn log_window_follows_cv1() {
    let s = scan(false);
    assert_eq!(
        log_window_start(&s),
        1_792_000_000_000 - 400 - ignition_core::deploy::verify::LOG_WINDOW_SLACK_MS
    );
    let mut s = scan(false);
    s.last_scan_duration = None;
    assert_eq!(
        log_window_start(&s),
        1_792_000_000_000
            - ignition_core::deploy::verify::LOG_WINDOW_FALLBACK_MS
            - ignition_core::deploy::verify::LOG_WINDOW_SLACK_MS
    );
    s.last_scan_timestamp = 10;
    assert_eq!(log_window_start(&s), 0, "never negative");
}

// ---- probes (DPLN-08) ------------------------------------------------------------

#[test]
fn probes_absent_is_warn_not_pass() {
    let mut e = expected();
    e.probes.clear();
    let mut o = observed();
    o.probes = None;
    let report = evaluate(&e, &o);
    let c = check(&report, CheckId::Probes);
    assert_eq!(c.status, CheckStatus::Warn);
    assert_eq!(codes(&report, CheckId::Probes), vec!["tag_probes_absent"]);
    assert!(report.ok, "a warning never fails the verdict");
    assert!(report.warnings.iter().any(|w| w.contains("probes")));
}

#[test]
fn probes_unavailable_fails() {
    let mut o = observed();
    o.probes = Some(Err("routes_not_deployed".to_string()));
    let report = evaluate(&expected(), &o);
    let c = check(&report, CheckId::Probes);
    assert_eq!(c.status, CheckStatus::Fail);
    assert_eq!(
        codes(&report, CheckId::Probes),
        vec!["tag_probes_unavailable"]
    );
    assert!(c.findings[0].detail.contains("routes_not_deployed"));
    // Declared probes that were never read are unavailable too, not a pass.
    o.probes = None;
    let report = evaluate(&expected(), &o);
    assert_eq!(check(&report, CheckId::Probes).status, CheckStatus::Fail);
}

#[test]
fn probe_quality_prefix() {
    for (quality, good) in [
        ("Good", true),
        ("Good_Provisional", true),
        ("Uncertain", false),
        ("Bad_Stale", false),
        ("Error_Configuration", false),
        ("", false),
    ] {
        let mut e = expected();
        e.probes = vec!["[default]T".to_string()];
        let mut o = observed();
        o.probes = Some(Ok(vec![reading("[default]T", quality)]));
        let report = evaluate(&e, &o);
        let c = check(&report, CheckId::Probes);
        assert_eq!(
            c.status,
            if good {
                CheckStatus::Pass
            } else {
                CheckStatus::Fail
            },
            "{quality:?}"
        );
        if !good {
            assert_eq!(c.findings[0].code, "tag_probe_not_good");
            assert_eq!(c.findings[0].subject, "[default]T");
        }
    }
}

#[test]
fn probe_missing_reading_fails() {
    let mut o = observed();
    o.probes = Some(Ok(vec![reading("[default]Heartbeat", "Good")]));
    let report = evaluate(&expected(), &o);
    let c = check(&report, CheckId::Probes);
    assert_eq!(c.status, CheckStatus::Fail);
    assert_eq!(c.findings.len(), 1);
    assert_eq!(c.findings[0].subject, "[default]Pump");
}

#[test]
fn probe_findings_in_manifest_order() {
    let mut e = expected();
    e.probes = vec![
        "[default]Zeta".to_string(),
        "[default]Alpha".to_string(),
        "[default]Mid".to_string(),
    ];
    let mut o = observed();
    o.probes = Some(Ok(vec![
        reading("[default]Alpha", "Bad_Stale"),
        reading("[default]Mid", "Bad_Stale"),
        reading("[default]Zeta", "Bad_Stale"),
    ]));
    let report = evaluate(&e, &o);
    let subjects: Vec<&str> = check(&report, CheckId::Probes)
        .findings
        .iter()
        .map(|f| f.subject.as_str())
        .collect();
    assert_eq!(
        subjects,
        vec!["[default]Zeta", "[default]Alpha", "[default]Mid"]
    );
}

// ---- determinism and value-freedom -------------------------------------------------

#[test]
fn deterministic_ordering() {
    let units = [
        "ignition/tag-definition/default/B",
        "ignition/tag-definition/default/A",
        "ignition/tag-definition/default/C",
    ];
    let mut e = expected();
    e.tag_units = units.iter().map(|s| s.to_string()).collect();
    let mut o = observed();
    o.files = units
        .iter()
        .map(|s| (s.to_string(), PlanStatus::Changed))
        .collect();
    let first = serde_json::to_string(&evaluate(&e, &o)).unwrap();

    let mut e2 = e.clone();
    e2.tag_units.reverse();
    let mut o2 = o.clone();
    o2.files.reverse();
    let second = serde_json::to_string(&evaluate(&e2, &o2)).unwrap();
    assert_eq!(first, second);

    let report: Value = serde_json::from_str(&first).unwrap();
    let subjects: Vec<&str> = report["checks"][3]["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["subject"].as_str().unwrap())
        .collect();
    let mut sorted = subjects.clone();
    sorted.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    assert_eq!(subjects, sorted);
}

#[test]
fn report_serializes_in_contract_order() {
    let text = serde_json::to_string(&evaluate(&expected(), &observed())).unwrap();
    let positions: Vec<usize> = [
        "\"ok\"",
        "\"slug\"",
        "\"checks\"",
        "\"resources_checked\"",
        "\"warnings\"",
    ]
    .iter()
    .map(|key| {
        text.find(key)
            .unwrap_or_else(|| panic!("{key} missing: {text}"))
    })
    .collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]), "{text}");
    let report: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(report["slug"], Value::Null);
    let ids: Vec<&str> = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        vec!["scan", "mode", "read_back", "files", "loaded", "probes"]
    );
    let red = serde_json::to_value({
        let mut o = observed();
        o.mode = None;
        evaluate(&expected(), &o)
    })
    .unwrap();
    assert_eq!(red["slug"], VERIFY_FAILED_SLUG);
    assert_eq!(red["ok"], false);
}

#[test]
fn no_message_or_value_echo() {
    let mut e = expected();
    e.resources = vec![ExpectedResource::exact(
        "ignition",
        "database-translator",
        Some("POSTGRES"),
        json!({"password_like": CANARY, "other": 1}),
    )];
    let mut o = observed();
    o.read_back.insert(
        DB.to_string(),
        Remote::Config(json!({"password_like": format!("{CANARY}-rest"), "other": 2})),
    );
    let mut line = unloaded_line();
    line.message.push_str(CANARY);
    o.logs = Ok(vec![line]);
    o.probes = Some(Ok(vec![
        reading("[default]Heartbeat", &format!("Bad_{CANARY}")),
        reading("[default]Pump", "Good"),
    ]));
    let report = evaluate(&e, &o);
    assert!(!report.ok);
    let serialized = serde_json::to_string(&report).unwrap();
    assert!(!serialized.contains(CANARY), "serialized: {serialized}");
    assert!(
        !format!("{report:?}").contains(CANARY),
        "report Debug leaked"
    );
    assert!(
        !format!("{o:?}").contains(CANARY),
        "ObservedState Debug leaked"
    );
    assert!(!format!("{e:?}").contains(CANARY), "Expected Debug leaked");
}
