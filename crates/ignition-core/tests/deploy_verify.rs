//! `deploy verify` (22-10): the pure six-check engine (Task 1) and the
//! read-only `deploy_verify` action over fakes (Task 2).
//!
//! The engine tests build [`Expected`] and [`ObservedState`] directly, so
//! each check can be failed alone. The rules under test are the 22-09 rig
//! decisions CV1 (log match), CV2 (read-back table N1 to N3 and the two skip
//! classes) and CV3 (post-scan bytes), and the confirmed 22-03 vocabulary.

mod common;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

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
fn read_back_n1_null_equals_absent_on_both_sides() {
    let e = one_resource(Compare::Exact(
        json!({"keep": 1, "createAutoIncSequence": null, "nested": {"x": null, "y": 2}}),
    ));
    // File null, REST absent.
    let ok = evaluate(&e, &read_back_with(json!({"keep": 1, "nested": {"y": 2}})));
    assert_eq!(check(&ok, CheckId::ReadBack).status, CheckStatus::Pass);

    // File null, REST echoing the same null (G-22-2): equal.
    let echoed = evaluate(
        &e,
        &read_back_with(json!({"keep": 1, "nested": {"y": 2}, "createAutoIncSequence": null})),
    );
    assert_eq!(
        check(&echoed, CheckId::ReadBack).status,
        CheckStatus::Pass,
        "{echoed:#?}"
    );

    // File lacks the key, REST carries it as null at a nested depth: equal.
    let e = one_resource(Compare::Exact(json!({"keep": 1, "nested": {"y": 2}})));
    let nested = evaluate(
        &e,
        &read_back_with(json!({"keep": 1, "nested": {"y": 2, "x": null}})),
    );
    assert_eq!(
        check(&nested, CheckId::ReadBack).status,
        CheckStatus::Pass,
        "{nested:#?}"
    );
}

const COBRANDING: &str = "ignition/cobranding";

fn cobranding_with(logo_name: Value) -> Value {
    json!({
        "backgroundColor": "#ffffff",
        "buttonColor": "#0000ff",
        "buttonTextColor": "#ffffff",
        "textColor": "#000000",
        "enabled": false,
        "appIcon": null,
        "appIconName": null,
        "appIconSize": null,
        "favicon": null,
        "faviconName": null,
        "faviconSize": null,
        "logo": null,
        "logoName": logo_name,
        "logoType": null,
    })
}

fn cobranding_expected(file: Value) -> Expected {
    let mut e = expected();
    e.resources = vec![ExpectedResource::exact(
        "ignition",
        "cobranding",
        None,
        file,
    )];
    e
}

fn cobranding_observed(rest: Value) -> ObservedState {
    let mut o = observed();
    o.read_back
        .insert(COBRANDING.to_string(), Remote::Config(rest));
    o
}

#[test]
fn read_back_n1_cobranding_rest_nulls_matching_the_file_pass() {
    let file = cobranding_with(Value::Null);
    let report = evaluate(
        &cobranding_expected(file.clone()),
        &cobranding_observed(file),
    );
    let c = check(&report, CheckId::ReadBack);
    assert_eq!(c.status, CheckStatus::Pass, "{report:#?}");
    assert!(c.findings.is_empty(), "{c:?}");
    assert!(report.resources_checked >= 1, "{report:#?}");
}

#[test]
fn read_back_n1_rest_null_against_a_file_value_fails() {
    let file = cobranding_with(json!("brand-FILE-SECRET.png"));
    let report = evaluate(
        &cobranding_expected(file),
        &cobranding_observed(cobranding_with(Value::Null)),
    );
    let c = check(&report, CheckId::ReadBack);
    assert_eq!(c.status, CheckStatus::Fail, "{report:#?}");
    assert_eq!(c.findings.len(), 1);
    assert_eq!(c.findings[0].code, "resource_differs");
    assert_eq!(c.findings[0].subject, COBRANDING);
    assert!(c.findings[0].detail.contains("logoName"), "{c:?}");
    let text = serde_json::to_string(&report).unwrap();
    assert!(!text.contains("brand-FILE-SECRET.png"), "{text}");
    assert!(!format!("{report:?}").contains("brand-FILE-SECRET.png"));
}

#[test]
fn read_back_n1_file_null_against_a_rest_value_fails() {
    let report = evaluate(
        &cobranding_expected(cobranding_with(Value::Null)),
        &cobranding_observed(cobranding_with(json!("brand-REST-SECRET.png"))),
    );
    let c = check(&report, CheckId::ReadBack);
    assert_eq!(c.status, CheckStatus::Fail, "{report:#?}");
    assert_eq!(c.findings.len(), 1);
    assert_eq!(c.findings[0].code, "resource_differs");
    assert!(c.findings[0].detail.contains("logoName"), "{c:?}");
    let text = serde_json::to_string(&report).unwrap();
    assert!(!text.contains("brand-REST-SECRET.png"), "{text}");
    assert!(!format!("{report:?}").contains("brand-REST-SECRET.png"));
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
fn read_back_n2_rest_nulls_are_dropped() {
    let key = "ignition/security-levels";
    let rest_with_null = json!({"securityLevels": [
        {"name": "Authenticated", "description": null, "children": []}
    ]});
    let mut o = observed();
    o.read_back
        .insert(key.to_string(), Remote::Config(rest_with_null));
    let level = |file: Value| {
        let mut e = expected();
        e.resources = vec![ExpectedResource::new(
            "ignition",
            "security-levels",
            None,
            Compare::NameKeyedSubset(file),
        )];
        e
    };
    // The file declares no description, REST echoes `description: null`.
    let ok = evaluate(
        &level(json!({"securityLevels": [{"name": "Authenticated", "children": []}]})),
        &o,
    );
    assert_eq!(
        check(&ok, CheckId::ReadBack).status,
        CheckStatus::Pass,
        "{ok:#?}"
    );

    // The file declares a description, REST holds null: the level differs.
    let bad = evaluate(
        &level(json!({"securityLevels": [
            {"name": "Authenticated", "description": "FILE-DESC-SECRET", "children": []}
        ]})),
        &o,
    );
    let c = check(&bad, CheckId::ReadBack);
    assert_eq!(c.status, CheckStatus::Fail, "{bad:#?}");
    assert!(c.findings[0].detail.contains("description"), "{c:?}");
    assert!(
        !serde_json::to_string(&bad)
            .unwrap()
            .contains("FILE-DESC-SECRET")
    );
}

#[test]
fn read_back_n3_rest_nulls_are_dropped() {
    let key = "ignition/translations";
    let attrs = |value: Value| {
        let Value::Object(attrs) = value else {
            unreachable!()
        };
        let mut e = expected();
        e.resources = vec![ExpectedResource::new(
            "ignition",
            "translations",
            None,
            Compare::Attributes(attrs),
        )];
        e
    };
    let observe = |rest: Value| {
        let mut o = observed();
        o.read_back.insert(key.to_string(), Remote::Config(rest));
        o
    };

    // A nested object holding a null member, echoed by REST with that null.
    let ok = evaluate(
        &attrs(json!({"opts": {"a": 1, "b": null}})),
        &observe(json!({"opts": {"a": 1, "b": null}, "terms": {}})),
    );
    assert_eq!(
        check(&ok, CheckId::ReadBack).status,
        CheckStatus::Pass,
        "{ok:#?}"
    );
    // The file lacks the nested null, REST carries it.
    let ok = evaluate(
        &attrs(json!({"opts": {"a": 1}})),
        &observe(json!({"opts": {"a": 1, "b": null}})),
    );
    assert_eq!(
        check(&ok, CheckId::ReadBack).status,
        CheckStatus::Pass,
        "{ok:#?}"
    );

    // A REST null for an attribute the file declares as `true` still differs:
    // the key lookup is on the raw REST map, so the null key is present.
    let bad = evaluate(
        &attrs(json!({"ignoreTags": true})),
        &observe(json!({"ignoreTags": null})),
    );
    let c = check(&bad, CheckId::ReadBack);
    assert_eq!(c.status, CheckStatus::Fail, "{bad:#?}");
    assert!(c.findings[0].detail.contains("ignoreTags"), "{c:?}");
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

// =============================================================================
// Task 2: the read-only action over fakes
// =============================================================================

use common::fake_deploy::FakeGateway;
use common::forbid_writes::ForbidWrites;
use common::plan_fixture::{MODE, PlanFixture};
use ignition_core::actions::deploy::{Declared, VerifyOptions, deploy_verify};
use ignition_core::client::DeployApi;
use ignition_core::client::ReqwestGatewayApi;
use ignition_core::client::config_resources::{ResourceCreate, ResourceRecord};
use ignition_core::client::scan::{LockObservation, ScanKind};
use ignition_core::deploy::layout::RelPath;
use ignition_core::deploy::manifest::Verify;
use ignition_core::deploy::transport::{ObservedFiles, Scope, TreeReader};
use ignition_core::error::CoreError;

const NAMED: &str = "ignition/database-translator/POSTGRES";
const SINGLETON: &str = "ignition/general-alarm-settings";
const TAG_FILE: &str = "ignition/tag-definition/P/F/tags.json";
const TAG_MARKER: &str = "ignition/tag-definition/P/F/unary-resource.json";

/// Options with every wait shortened for tests.
fn fast() -> VerifyOptions {
    VerifyOptions {
        probe_deadline: Duration::from_millis(400),
        probe_interval: Duration::from_millis(20),
        log_settle: Duration::ZERO,
    }
}

/// The rendered `config.json` of `key` with JSON null members removed. Most
/// types' REST view omits null members, and this helper models that shape.
/// `ignition/cobranding` is the exception: it echoes unset members as explicit
/// `null`, which `rest_config_raw` models.
fn rest_config(declared: &Declared, key: &str) -> Value {
    fn strip(v: &Value) -> Value {
        match v {
            Value::Object(m) => Value::Object(
                m.iter()
                    .filter(|(_, v)| !v.is_null())
                    .map(|(k, v)| (k.clone(), strip(v)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
    let path = RelPath::new(&format!("{key}/config.json")).unwrap();
    let bytes = declared
        .outcome
        .tree
        .files
        .get(&path)
        .expect("rendered config");
    strip(&serde_json::from_slice(bytes).unwrap())
}

fn record(config: Value) -> ResourceRecord {
    ResourceRecord {
        resource_type: String::new(),
        name: None,
        description: None,
        enabled: Some(true),
        version: Some(1),
        collection: None,
        collections: Vec::new(),
        signature: None,
        config,
        backup_config: None,
        attributes: None,
        extra: BTreeMap::new(),
    }
}

/// A fake gateway in mode `dev` holding the tracer resources in the mode
/// collection (`named` controls whether the named one is present).
async fn seeded_fake(declared: &Declared, named: bool) -> FakeGateway {
    let fake = FakeGateway::new();
    fake.set_deployment_mode(Some(MODE.to_string()));
    let setup = fake.client("setup");
    if named {
        let mut create = ResourceCreate::new("POSTGRES", rest_config(declared, NAMED));
        create.collection = Some(MODE.to_string());
        setup
            .resource_create("ignition", "database-translator", &[create])
            .await
            .expect("seed named");
    }
    fake.seed_singleton(
        "ignition",
        "general-alarm-settings",
        MODE,
        record(rest_config(declared, SINGLETON)),
    );
    fake
}

struct Env {
    fx: PlanFixture,
    declared: Declared,
    reader: Box<dyn TreeReader>,
}

/// The tracer repo with its render written to the live mode collection.
fn env() -> Env {
    let fx = PlanFixture::new();
    let (declared, reader) = fx.load("dev");
    fx.live_render(MODE, &declared);
    Env {
        fx,
        declared,
        reader,
    }
}

fn logs_body(items: Vec<Value>) -> Value {
    json!({"items": items, "metadata": {"total": 0, "matching": 0, "limit": 200, "offset": 0}})
}

async fn mount_logs(server: &wiremock::MockServer, items: Vec<Value>) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/logs"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(logs_body(items)))
        .mount(server)
        .await;
}

async fn mount_tags_present(server: &wiremock::MockServer) {
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .and(wiremock::matchers::body_partial_json(
            json!({"action": "version"}),
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "ok": true,
            "data": {"routeVersion": ignition_core::webdev::ROUTE_BUNDLE_VERSION, "minCli": "1.0"},
        })))
        .mount(server)
        .await;
}

fn read_rows(rows: &[(&str, &str)]) -> Value {
    let results: Vec<Value> = rows
        .iter()
        .map(|(path, quality)| {
            json!({"path": path, "value": CANARY, "quality": quality, "timestamp": "t"})
        })
        .collect();
    json!({"ok": true, "data": {"results": results}})
}

/// Mount a tags `read` answer, used at most `times` times when given.
async fn mount_read(server: &wiremock::MockServer, rows: &[(&str, &str)], times: Option<u64>) {
    let mock = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .and(wiremock::matchers::body_partial_json(
            json!({"action": "read"}),
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(read_rows(rows)));
    match times {
        Some(n) => mock.up_to_n_times(n).mount(server).await,
        None => mock.mount(server).await,
    }
}

async fn run(
    env: &Env,
    fake: &FakeGateway,
    api: &ReqwestGatewayApi,
) -> Result<ignition_core::actions::deploy::DeployVerifyResult, CoreError> {
    deploy_verify(
        &env.declared,
        "ign-cli",
        &ForbidWrites(fake.client("verify")),
        api,
        env.reader.as_ref(),
        &fast(),
    )
    .await
}

fn probes(declared: &mut Declared, paths: &[&str]) {
    declared.manifest.verify = Some(Verify {
        tag_probes: paths.iter().map(|p| (*p).to_string()).collect(),
    });
}

#[tokio::test]
async fn verify_green_and_red_through_action() {
    let mut env = env();
    probes(&mut env.declared, &["[default]Heartbeat"]);
    let server = wiremock::MockServer::start().await;
    mount_logs(&server, Vec::new()).await;
    mount_tags_present(&server).await;
    mount_read(&server, &[("[default]Heartbeat", "Good")], None).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);

    let fake = seeded_fake(&env.declared, true).await;
    let green = run(&env, &fake, &api).await.expect("verdict");
    assert!(green.ok, "{green:#?}");
    assert_eq!(green.slug, None);
    assert_eq!(green.failure_exit_code(), None);
    assert_eq!(green.resources_checked, 2);
    assert_eq!(
        (
            green.env.as_str(),
            green.gateway.as_str(),
            green.mode.as_str()
        ),
        ("dev", "tracer-gw", "dev")
    );
    assert_eq!(
        (green.node.as_str(), green.project.as_str()),
        ("master", "ign-cli")
    );
    assert_eq!(green.checks.len(), 6);

    // Same everything, but the named resource is absent from the gateway.
    let fake = seeded_fake(&env.declared, false).await;
    let red = run(&env, &fake, &api).await.expect("verdict");
    assert!(!red.ok);
    assert_eq!(red.slug, Some(VERIFY_FAILED_SLUG));
    assert_eq!(red.failure_exit_code(), Some(6));
    let read_back = red
        .checks
        .iter()
        .find(|c| c.id == CheckId::ReadBack)
        .unwrap();
    assert_eq!(read_back.status, CheckStatus::Fail);
    assert_eq!(read_back.findings[0].code, "resource_missing");
    assert_eq!(read_back.findings[0].subject, NAMED);
}

/// The rendered `config.json` of `key` parsed with its null members KEPT:
/// the shape a type that echoes unset members as explicit `null` reports
/// (`ignition/cobranding`; the tracer translator's auto-increment members).
fn rest_config_raw(declared: &Declared, key: &str) -> Value {
    let path = RelPath::new(&format!("{key}/config.json")).unwrap();
    let bytes = declared
        .outcome
        .tree
        .files
        .get(&path)
        .expect("rendered config");
    serde_json::from_slice(bytes).unwrap()
}

/// Like `seeded_fake(declared, true)`, but the REST view echoes null members.
async fn seeded_fake_echoing_nulls(declared: &Declared) -> FakeGateway {
    let fake = FakeGateway::new();
    fake.set_deployment_mode(Some(MODE.to_string()));
    let setup = fake.client("setup");
    let mut create = ResourceCreate::new("POSTGRES", rest_config_raw(declared, NAMED));
    create.collection = Some(MODE.to_string());
    setup
        .resource_create("ignition", "database-translator", &[create])
        .await
        .expect("seed named");
    fake.seed_singleton(
        "ignition",
        "general-alarm-settings",
        MODE,
        record(rest_config_raw(declared, SINGLETON)),
    );
    fake
}

#[tokio::test]
async fn verify_green_when_rest_echoes_file_nulls() {
    let env = env();
    // The shape precondition: the rendered files really do hold null members.
    let has_null = |key: &str| {
        rest_config_raw(&env.declared, key)
            .as_object()
            .is_some_and(|m| m.values().any(Value::is_null))
    };
    assert!(
        has_null(NAMED) || has_null(SINGLETON),
        "the tracer render carries no null member; the test would pass vacuously"
    );
    let server = wiremock::MockServer::start().await;
    mount_logs(&server, Vec::new()).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let fake = seeded_fake_echoing_nulls(&env.declared).await;
    let result = run(&env, &fake, &api).await.expect("verdict");
    let read_back = result
        .checks
        .iter()
        .find(|c| c.id == CheckId::ReadBack)
        .unwrap();
    assert_eq!(read_back.status, CheckStatus::Pass, "{read_back:#?}");
    assert_eq!(result.resources_checked, 2);
    assert!(result.ok, "{result:#?}");
}

#[tokio::test]
async fn verify_result_keys_are_in_contract_order() {
    let env = env();
    let server = wiremock::MockServer::start().await;
    mount_logs(&server, Vec::new()).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let fake = seeded_fake(&env.declared, true).await;
    let result = run(&env, &fake, &api).await.expect("verdict");
    let text = serde_json::to_string(&result).unwrap();
    let keys = [
        "env",
        "gateway",
        "mode",
        "node",
        "project",
        "ok",
        "slug",
        "checks",
        "resources_checked",
        "warnings",
    ];
    let mut last = 0;
    for key in keys {
        let at = text
            .find(&format!("\"{key}\":"))
            .unwrap_or_else(|| panic!("{key} missing in {text}"));
        assert!(at >= last, "{key} out of order in {text}");
        last = at;
    }
    // No probes declared: a warning, not a failure.
    assert!(result.ok, "{result:#?}");
    assert!(result.warnings.iter().any(|w| w.starts_with("probes:")));
}

#[tokio::test]
async fn verify_makes_no_writes() {
    let env = env();
    let server = wiremock::MockServer::start().await;
    mount_logs(&server, Vec::new()).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let fake = seeded_fake(&env.declared, true).await;
    let before = fake
        .client("watcher")
        .scan_status(ScanKind::Config)
        .await
        .unwrap();
    run(&env, &fake, &api).await.expect("verdict");
    let after = fake
        .client("watcher")
        .scan_status(ScanKind::Config)
        .await
        .unwrap();
    assert_eq!(
        before.last_scan_timestamp, after.last_scan_timestamp,
        "no scan"
    );
    assert!(!after.scan_active);
    assert!(matches!(
        fake.client("watcher")
            .scan_lock_info(ScanKind::Config)
            .await
            .unwrap(),
        LockObservation::Free
    ));
    // Only gateway reads reached the mock: logs, no POST for the lock or scan.
    let requests = server.received_requests().await.unwrap();
    assert!(
        requests.iter().all(|r| r.method.as_str() == "GET"),
        "{:?}",
        requests
            .iter()
            .map(|r| (r.method.to_string(), r.url.path().to_string()))
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
#[should_panic(expected = "plan, pull and verify must not write")]
async fn forbid_writes_double_really_panics() {
    let fake = FakeGateway::new();
    let _ = ForbidWrites(fake.client("x"))
        .scan_request(ScanKind::Config)
        .await;
}

#[tokio::test]
async fn verify_unreachable_gateway_is_an_error() {
    let env = env();
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let dead = ReqwestGatewayApi::for_tests(&format!("http://127.0.0.1:{port}"), None);
    let err = deploy_verify(
        &env.declared,
        "ign-cli",
        &dead,
        &dead,
        env.reader.as_ref(),
        &fast(),
    )
    .await
    .expect_err("no verdict without a gateway");
    assert!(matches!(err, CoreError::Network { .. }), "{err:?}");
    assert_eq!(err.code(), "network_error");
}

struct DeadTransport;

#[async_trait::async_trait]
impl TreeReader for DeadTransport {
    async fn list_types(&self, _c: &str) -> Result<Vec<(String, String)>, CoreError> {
        unreachable!("verify reads through `read`")
    }

    async fn read(&self, _scope: &Scope) -> Result<ObservedFiles, CoreError> {
        Err(CoreError::TransportFailed {
            operation: "read".to_string(),
            node: "nodes.master".to_string(),
            detail: "connection refused".to_string(),
        })
    }

    async fn read_ledger(&self, _g: &str, _m: &str) -> Result<Option<Vec<u8>>, CoreError> {
        unreachable!("verify does not read the ledger")
    }

    async fn read_journal(&self, _g: &str, _m: &str) -> Result<Option<Vec<u8>>, CoreError> {
        unreachable!("verify does not read the journal")
    }
}

#[tokio::test]
async fn verify_transport_failure_is_an_error() {
    let env = env();
    let server = wiremock::MockServer::start().await;
    mount_logs(&server, Vec::new()).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let fake = seeded_fake(&env.declared, true).await;
    let err = deploy_verify(
        &env.declared,
        "ign-cli",
        &ForbidWrites(fake.client("v")),
        &api,
        &DeadTransport,
        &fast(),
    )
    .await
    .expect_err("no verdict without the files");
    assert_eq!(err.code(), "transport_failed");
}

#[tokio::test]
async fn verify_logs_refusal_is_unavailable_not_error() {
    let env = env();
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/logs"))
        .respond_with(
            wiremock::ResponseTemplate::new(403).set_body_json(json!({"message": CANARY})),
        )
        .mount(&server)
        .await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let fake = seeded_fake(&env.declared, true).await;
    let result = run(&env, &fake, &api)
        .await
        .expect("a verdict, not an error");
    assert!(!result.ok);
    let loaded = result
        .checks
        .iter()
        .find(|c| c.id == CheckId::Loaded)
        .unwrap();
    assert_eq!(loaded.status, CheckStatus::Fail);
    assert_eq!(loaded.findings[0].code, "logs_unavailable");
    assert!(!loaded.findings[0].detail.is_empty());
    assert!(
        !serde_json::to_string(&result).unwrap().contains(CANARY),
        "the refusal body must not be echoed"
    );
}

#[tokio::test]
async fn verify_log_hit_for_a_declared_resource_is_red_and_value_free() {
    let env = env();
    let server = wiremock::MockServer::start().await;
    mount_logs(
        &server,
        vec![json!({
            "timestamp": 1_792_000_000_050_i64,
            "loggerName": "NamedResourceHandler-ignition/database-translator",
            "level": "WARN",
            "message": format!("Resource {NAMED} does not have required UUID attribute, will not be loaded {CANARY}"),
        })],
    )
    .await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let fake = seeded_fake(&env.declared, true).await;
    let result = run(&env, &fake, &api).await.expect("verdict");
    assert!(!result.ok);
    assert_eq!(result.failure_exit_code(), Some(6));
    let loaded = result
        .checks
        .iter()
        .find(|c| c.id == CheckId::Loaded)
        .unwrap();
    assert_eq!(loaded.findings[0].code, "unloaded_resource");
    assert_eq!(loaded.findings[0].subject, NAMED);
    let text = serde_json::to_string(&result).unwrap();
    assert!(!text.contains(CANARY) && !format!("{result:?}").contains(CANARY));
    // The query was the CV1 one: WARN, an explicit window and limit.
    let requests = server.received_requests().await.unwrap();
    let query = requests[0].url.query().unwrap_or_default().to_string();
    assert!(query.contains("minLevel=WARN"), "{query}");
    assert!(query.contains("startTime="), "{query}");
    assert!(query.contains("limit=200"), "{query}");
    assert!(!query.contains("endTime"), "{query}");
    assert!(!query.contains("logger="), "{query}");
}

#[tokio::test]
async fn verify_log_window_starts_before_the_last_scan() {
    let env = env();
    let server = wiremock::MockServer::start().await;
    mount_logs(&server, Vec::new()).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let fake = seeded_fake(&env.declared, true).await;
    let status = fake
        .client("w")
        .scan_status(ScanKind::Config)
        .await
        .unwrap();
    let expected_start = ignition_core::deploy::verify::log_window_start(&status);
    run(&env, &fake, &api).await.expect("verdict");
    let requests = server.received_requests().await.unwrap();
    let query = requests[0].url.query().unwrap_or_default().to_string();
    assert!(
        query.contains(&format!("startTime={expected_start}")),
        "{query} vs {expected_start}"
    );
}

#[tokio::test]
async fn verify_log_settle_is_skipped_when_the_gateway_logged_since() {
    let env = env();
    let server = wiremock::MockServer::start().await;
    // The probe for "the gateway logged after the settle point" finds a line.
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/logs"))
        .and(wiremock::matchers::query_param("limit", "1"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(logs_body(vec![json!({
            "timestamp": 1_792_999_999_999_i64, "loggerName": "x", "level": "INFO", "message": "m"
        })])))
        .mount(&server)
        .await;
    mount_logs(&server, Vec::new()).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let fake = seeded_fake(&env.declared, true).await;
    let mut opts = fast();
    opts.log_settle = Duration::from_secs(60);
    let started = Instant::now();
    let result = deploy_verify(
        &env.declared,
        "ign-cli",
        &ForbidWrites(fake.client("v")),
        &api,
        env.reader.as_ref(),
        &opts,
    )
    .await
    .expect("verdict");
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "settle was not skipped"
    );
    assert!(result.ok, "{result:#?}");
}

#[tokio::test]
async fn verify_slug_is_not_an_error_slug() {
    assert!(!CoreError::DEPLOY_SLUGS.contains(&VERIFY_FAILED_SLUG));
    for code in [
        "transport_failed",
        "deploy_ledger_invalid",
        "deploy_tree_invalid",
    ] {
        assert_ne!(code, VERIFY_FAILED_SLUG);
    }
    // A red result is data, not an error: the exit-6 class is applied by the
    // binary through `failure_exit_code`.
    let env = env();
    let server = wiremock::MockServer::start().await;
    mount_logs(&server, Vec::new()).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let fake = seeded_fake(&env.declared, false).await;
    let red = run(&env, &fake, &api).await;
    assert!(red.is_ok());
}

// ---- files over the transport -----------------------------------------------------

fn with_tag_unit(env: &mut Env, live: Option<(&str, &str)>) {
    let tree = &mut env.declared.outcome.tree.files;
    tree.insert(RelPath::new(TAG_FILE).unwrap(), b"[]".to_vec());
    tree.insert(RelPath::new(TAG_MARKER).unwrap(), b"{}".to_vec());
    env.declared.manifest.tag_providers = vec!["P".to_string()];
    env.fx.mode(TAG_MARKER, b"{}");
    match live {
        Some((path, bytes)) => env.fx.mode(path, bytes.as_bytes()),
        None => env.fx.mode(TAG_FILE, b"[]"),
    }
}

#[tokio::test]
async fn verify_files_check_compares_tag_units_byte_for_byte() {
    let mut env = env();
    with_tag_unit(&mut env, None);
    let server = wiremock::MockServer::start().await;
    mount_logs(&server, Vec::new()).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let fake = seeded_fake(&env.declared, true).await;
    let green = run(&env, &fake, &api).await.expect("verdict");
    let files = green
        .checks
        .iter()
        .find(|c| c.id == CheckId::Files)
        .unwrap();
    assert_eq!(files.status, CheckStatus::Pass, "{green:#?}");
    assert!(
        files.detail.starts_with("1 declared tag unit"),
        "{}",
        files.detail
    );

    // One byte off on the host.
    env.fx.mode(TAG_FILE, b"[ ]");
    let red = run(&env, &fake, &api).await.expect("verdict");
    assert!(!red.ok);
    assert_eq!(red.failure_exit_code(), Some(6));
    let files = red.checks.iter().find(|c| c.id == CheckId::Files).unwrap();
    assert_eq!(files.findings[0].code, "files_differ");
    assert_eq!(files.findings[0].subject, "ignition/tag-definition/P/F");
    assert!(!serde_json::to_string(&red).unwrap().contains("[ ]"));
}

// ---- probes ------------------------------------------------------------------------

fn probe_check(result: &ignition_core::actions::deploy::DeployVerifyResult) -> &Check {
    result
        .checks
        .iter()
        .find(|c| c.id == CheckId::Probes)
        .unwrap()
}

#[tokio::test]
async fn verify_probe_that_turns_good_before_the_deadline_passes() {
    let mut env = env();
    probes(&mut env.declared, &["[default]Heartbeat"]);
    let server = wiremock::MockServer::start().await;
    mount_logs(&server, Vec::new()).await;
    mount_tags_present(&server).await;
    mount_read(&server, &[("[default]Heartbeat", "Bad_NotFound")], Some(3)).await;
    mount_read(&server, &[("[default]Heartbeat", "Good")], None).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let fake = seeded_fake(&env.declared, true).await;
    let result = run(&env, &fake, &api).await.expect("verdict");
    assert_eq!(
        probe_check(&result).status,
        CheckStatus::Pass,
        "{result:#?}"
    );
    assert!(result.ok);
    assert!(
        !serde_json::to_string(&result).unwrap().contains(CANARY),
        "tag value leaked"
    );
}

#[tokio::test]
async fn verify_probe_that_never_turns_good_fails_after_the_deadline() {
    let mut env = env();
    probes(&mut env.declared, &["[default]Heartbeat"]);
    let server = wiremock::MockServer::start().await;
    mount_logs(&server, Vec::new()).await;
    mount_tags_present(&server).await;
    mount_read(&server, &[("[default]Heartbeat", "Bad_NotFound")], None).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let fake = seeded_fake(&env.declared, true).await;
    let started = Instant::now();
    let result = run(&env, &fake, &api).await.expect("verdict");
    let waited = started.elapsed();
    assert!(
        waited >= Duration::from_millis(300),
        "gave up early: {waited:?}"
    );
    assert!(waited < Duration::from_secs(10), "unbounded: {waited:?}");
    let probes = probe_check(&result);
    assert_eq!(probes.status, CheckStatus::Fail);
    assert_eq!(probes.findings[0].code, "tag_probe_not_good");
    assert!(probes.findings[0].detail.contains("Bad_NotFound"));
    assert_eq!(result.failure_exit_code(), Some(6));
}

#[tokio::test]
async fn verify_probe_route_absent_is_unavailable() {
    let mut env = env();
    probes(&mut env.declared, &["[default]Heartbeat"]);
    let server = wiremock::MockServer::start().await;
    mount_logs(&server, Vec::new()).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/system/webdev/ign-cli/cli/tags"))
        .respond_with(wiremock::ResponseTemplate::new(405))
        .mount(&server)
        .await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let fake = seeded_fake(&env.declared, true).await;
    let result = run(&env, &fake, &api)
        .await
        .expect("a verdict, not an error");
    let probes = probe_check(&result);
    assert_eq!(probes.status, CheckStatus::Fail);
    assert_eq!(probes.findings[0].code, "tag_probes_unavailable");
    assert_eq!(probes.findings[0].detail, "routes_not_deployed");
}

/// Through the real client on the wire: a singleton whose effective copy is in
/// the `system` collection is skipped with a warning (CV2), never compared.
#[tokio::test]
async fn verify_system_collection_copy_is_skipped_with_a_warning() {
    let env = env();
    let server = wiremock::MockServer::start().await;
    mount_logs(&server, Vec::new()).await;
    let get = |p: &str| {
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(p.to_string()))
    };
    get("/data/api/v1/scan/config")
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "scanActive": false, "lastScanTimestamp": 1_792_000_000_000_i64, "lastScanDuration": 300
        })))
        .mount(&server)
        .await;
    get("/data/api/v1/gateway-info")
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(json!({"deploymentMode": MODE})),
        )
        .mount(&server)
        .await;
    get("/data/api/v1/resources/list/ignition/database-translator")
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "items": [{
                "type": "ignition/database-translator", "name": "POSTGRES", "collection": MODE,
                "config": rest_config(&env.declared, NAMED)
            }],
            "metadata": {"total": 1, "matching": 1, "limit": -1, "offset": 0}
        })))
        .mount(&server)
        .await;
    get("/data/api/v1/resources/singleton/ignition/general-alarm-settings")
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "type": "ignition/general-alarm-settings", "collection": "system",
            "config": {"anything": 1}
        })))
        .mount(&server)
        .await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = deploy_verify(
        &env.declared,
        "ign-cli",
        &api,
        &api,
        env.reader.as_ref(),
        &fast(),
    )
    .await
    .expect("verdict");
    assert!(result.ok, "{result:#?}");
    assert_eq!(result.resources_checked, 1);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains(SINGLETON) && w.contains("system")),
        "{:?}",
        result.warnings
    );
    // Nothing but GETs reached the gateway.
    let requests = server.received_requests().await.unwrap();
    assert!(requests.iter().all(|r| r.method.as_str() == "GET"));
}
