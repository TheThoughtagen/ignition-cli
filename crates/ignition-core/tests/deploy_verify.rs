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
        unreadable: Vec::new(),
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

const DEEP: &str = "ignition/tag-definition/default/Deep";

/// quick-261029-ms3: a declared tag unit whose live path the host's sftp
/// server cannot read is one red `unreadable` finding, never the
/// `files_differ` "not found" one (never treated as absent).
#[test]
fn files_declared_unreadable_tag_unit_is_red_not_absent() {
    let mut e = expected();
    e.tag_units.push(DEEP.to_string());
    let mut o = observed();
    o.unreadable = vec![(DEEP.to_string(), true)];
    let report = evaluate(&e, &o);
    let c = check(&report, CheckId::Files);
    assert_eq!(c.status, CheckStatus::Fail, "{report:#?}");
    assert_eq!(c.findings.len(), 1, "{:?}", c.findings);
    assert_eq!(c.findings[0].code, "unreadable");
    assert_eq!(c.findings[0].subject, DEEP);
    assert!(
        c.findings[0].detail.contains("cannot read"),
        "{}",
        c.findings[0].detail
    );
    assert!(!report.ok);
    assert_eq!(report.slug, Some(VERIFY_FAILED_SLUG));
}

/// quick-261029-ms3: an undeclared unreadable unit keeps `files` green and
/// adds one warning naming it.
#[test]
fn files_undeclared_unreadable_unit_is_a_warning_only() {
    let key = "ignition/tag-definition/MQTT/Edge";
    let mut o = observed();
    o.unreadable = vec![(key.to_string(), false)];
    let report = evaluate(&expected(), &o);
    assert_eq!(check(&report, CheckId::Files).status, CheckStatus::Pass);
    assert!(report.ok, "{report:#?}");
    let hits: Vec<&String> = report.warnings.iter().filter(|w| w.contains(key)).collect();
    assert_eq!(hits.len(), 1, "{:?}", report.warnings);
    assert!(hits[0].contains("not declared"), "{}", hits[0]);
}

/// quick-261029-ms3: a declared non-tag resource that is unreadable is also
/// a red `unreadable` finding in the `files` check.
#[test]
fn files_declared_unreadable_resource_unit_is_red() {
    let mut o = observed();
    o.unreadable = vec![(DB.to_string(), true)];
    let report = evaluate(&expected(), &o);
    let c = check(&report, CheckId::Files);
    assert_eq!(c.status, CheckStatus::Fail);
    assert_eq!(codes(&report, CheckId::Files), vec!["unreadable"]);
    assert_eq!(c.findings[0].subject, DB);
    assert!(!report.ok);
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

/// A tag folder marker as the gateway writes it (the CD5 captures), listing
/// `files`; render refuses any other marker shape (261030-8m8 review WR-02).
fn tag_marker(files: &[&str]) -> String {
    let list: Vec<String> = files.iter().map(|f| format!("\"{f}\"")).collect();
    format!(
        "{{\"scope\":\"G\",\"version\":1,\"restricted\":false,\"overridable\":true,\"files\":[{}],\"attributes\":{{\"config\":{{}}}}}}",
        list.join(",")
    )
}

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

    // The named resource is absent from the gateway while the live files
    // equal the repo (plan `same`): a warning, still green (R2).
    let fake = seeded_fake(&env.declared, false).await;
    let warn = run(&env, &fake, &api).await.expect("verdict");
    assert!(warn.ok, "{warn:#?}");
    assert_eq!(warn.failure_exit_code(), None);
    let read_back = warn
        .checks
        .iter()
        .find(|c| c.id == CheckId::ReadBack)
        .unwrap();
    assert_eq!(read_back.status, CheckStatus::Warn);
    assert_eq!(read_back.findings[0].code, "resource_missing");
    assert_eq!(read_back.findings[0].subject, NAMED);
    assert_eq!(read_back.findings[0].status, Some(CheckStatus::Warn));

    // Same everything, but the live file drifted too (plan `changed`): red.
    drift_live_translator(&env);
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
async fn verify_reports_the_excluded_tag_providers() {
    let mut env = env();
    env.declared.manifest.exclude_tag_providers = vec!["MQTT Engine".to_string()];
    let server = wiremock::MockServer::start().await;
    mount_logs(&server, Vec::new()).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let fake = seeded_fake(&env.declared, true).await;
    let result = run(&env, &fake, &api).await.expect("verdict");
    assert_eq!(result.excluded_tag_providers, ["MQTT Engine", "System"]);
    let text = serde_json::to_string(&result).unwrap();
    assert!(
        text.contains("\"excluded_tag_providers\":[\"MQTT Engine\",\"System\"]"),
        "{text}"
    );
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

    async fn list_tag_providers(&self, _c: &str) -> Result<Vec<String>, CoreError> {
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
    let marker = tag_marker(&["tags.json"]);
    tree.insert(
        RelPath::new(TAG_MARKER).unwrap(),
        marker.clone().into_bytes(),
    );
    env.declared.manifest.tag_providers = vec!["P".to_string()];
    env.fx.mode(TAG_MARKER, marker.as_bytes());
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

// =============================================================================
// `deploy_verify_node`: the secret-name check on each node (24-09, DPAIR-03)
// =============================================================================

use ignition_core::actions::deploy::{DeployVerifyResult, VerifyNode, deploy_verify_node};
use ignition_core::deploy::pair::{BackupNode, SecretRef};

/// A secret reference as a rendered `.json` file carries it (names only).
fn referenced(provider: &str, name: &str) -> String {
    format!(
        r#"{{"cred":{{"type":"Referenced","data":{{"providerName":"{provider}","secretName":"{name}"}}}}}}"#
    )
}

/// The rendered tree gains one file per `(index, provider, name)` that
/// references a secret. The file sits under a path the engine does not judge
/// (it is not a tag unit and not a declared resource), so only the secret
/// check sees it.
fn with_secret_refs(env: &mut Env, refs: &[(&str, &str)]) {
    for (index, (provider, name)) in refs.iter().enumerate() {
        let path =
            RelPath::new(&format!("ignition/secret-ref-probe/R{index}/config.json")).expect("path");
        env.declared
            .outcome
            .tree
            .files
            .insert(path, referenced(provider, name).into_bytes());
    }
}

/// The render also declares `provider` (`ignition/secret-provider/<P>/...`).
fn declaring_provider(env: &mut Env, provider: &str) {
    let path =
        RelPath::new(&format!("ignition/secret-provider/{provider}/config.json")).expect("path");
    env.declared
        .outcome
        .tree
        .files
        .insert(path, br#"{"profile":{"type":"FILE"}}"#.to_vec());
}

/// A master and a backup gateway, both holding the tracer resources, the
/// backup answering a settled, linked redundancy status.
struct NodePair {
    master: FakeGateway,
    master_api: ReqwestGatewayApi,
    backup: FakeGateway,
    backup_api: ReqwestGatewayApi,
    servers: Vec<wiremock::MockServer>,
}

async fn node_pair(env: &Env) -> NodePair {
    let master_server = wiremock::MockServer::start().await;
    mount_logs(&master_server, Vec::new()).await;
    let backup_server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/redundancy"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "role": "Backup", "projectState": "Good", "activityLevel": "Cold",
            "localId": "10.0.0.2", "peerId": "dev", "peerConnected": true,
            "hasConfigAccess": true, "syncPending": false, "failoverPending": false,
            "uptime": -1, "lastSyncTimestamp": 1_792_000_000_000_i64
        })))
        .mount(&backup_server)
        .await;
    NodePair {
        master: seeded_fake(&env.declared, true).await,
        master_api: ReqwestGatewayApi::for_tests(&master_server.uri(), None),
        backup: seeded_fake(&env.declared, true).await,
        backup_api: ReqwestGatewayApi::for_tests(&backup_server.uri(), None),
        servers: vec![master_server, backup_server],
    }
}

async fn run_node(env: &Env, pair: &NodePair, node: VerifyNode) -> DeployVerifyResult {
    let master_deploy = ForbidWrites(pair.master.client("verify"));
    let backup_deploy = ForbidWrites(pair.backup.client("verify-backup"));
    let backup = BackupNode::new(&pair.backup_api, &backup_deploy, None);
    deploy_verify_node(
        &env.declared,
        "ign-cli",
        node,
        Some((&master_deploy, &pair.master_api)),
        env.reader.as_ref(),
        Some(&backup),
        &fast(),
    )
    .await
    .expect("verdict")
}

fn secret_statuses(result: &DeployVerifyResult) -> Vec<(String, CheckStatus)> {
    result
        .secrets
        .as_ref()
        .expect("a secrets report")
        .nodes
        .iter()
        .map(|n| (n.node.clone(), n.status))
        .collect()
}

fn pair_ref(provider: &str, name: &str) -> SecretRef {
    SecretRef {
        provider: provider.to_string(),
        name: name.to_string(),
    }
}

#[tokio::test]
async fn verify_node_both_green() {
    let mut env = env();
    with_secret_refs(
        &mut env,
        &[("cap24-file", "CAP24-Name"), ("cap24-file", "Other")],
    );
    let pair = node_pair(&env).await;
    for fake in [&pair.master, &pair.backup] {
        fake.seed_secret_names("cap24-file", &["CAP24-Name", "Other", "Unreferenced"]);
    }
    let result = run_node(&env, &pair, VerifyNode::Both).await;
    assert!(result.ok, "{result:#?}");
    assert_eq!(result.slug, None);
    assert_eq!(result.node, "both");
    assert_eq!(result.checks.len(), 6);
    assert_eq!(
        secret_statuses(&result),
        vec![
            ("master".to_string(), CheckStatus::Pass),
            ("backup".to_string(), CheckStatus::Pass)
        ]
    );
    let secrets = result.secrets.as_ref().unwrap();
    assert!(secrets.nodes.iter().all(|n| n.providers_checked == 1));
    assert!(secrets.nodes.iter().all(|n| n.missing.is_empty()));
    assert!(result.backup.as_ref().unwrap().converged);
    drop(pair.servers);
}

/// A backup answering over the wire whose `general-alarm-settings` copy is in
/// the `system` collection, so its read-back carries a warning that the master
/// (a fake holding the file-backed copy) does not.
async fn backup_with_system_singleton(env: &Env) -> (wiremock::MockServer, ReqwestGatewayApi) {
    let server = wiremock::MockServer::start().await;
    let get = |p: &str| {
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(p.to_string()))
    };
    get("/data/api/v1/redundancy")
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "role": "Backup", "projectState": "Good", "activityLevel": "Cold",
            "localId": "10.0.0.2", "peerId": "dev", "peerConnected": true,
            "hasConfigAccess": true, "syncPending": false, "failoverPending": false,
            "uptime": -1, "lastSyncTimestamp": 1_792_000_000_000_i64
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
    (server, api)
}

#[tokio::test]
async fn verify_node_both_carries_the_backup_read_back_warnings() {
    // WR-03: `--node both` reports the backup's read-back warnings exactly as
    // `--node backup` does; the master verify has not read the backup.
    let env = env();
    let pair = node_pair(&env).await;
    let (_server, backup_api) = backup_with_system_singleton(&env).await;
    let run = |node: VerifyNode| {
        let master_deploy = ForbidWrites(pair.master.client("verify"));
        let env = &env;
        let pair = &pair;
        let backup_api = &backup_api;
        async move {
            let backup = BackupNode::new(backup_api, backup_api, None);
            deploy_verify_node(
                &env.declared,
                "ign-cli",
                node,
                Some((&master_deploy, &pair.master_api)),
                env.reader.as_ref(),
                Some(&backup),
                &fast(),
            )
            .await
            .expect("verdict")
        }
    };

    let only = run(VerifyNode::Backup).await;
    let backup_warning = |w: &&String| w.contains(SINGLETON) && w.contains("system");
    assert_eq!(
        only.warnings.iter().filter(backup_warning).count(),
        1,
        "{:#?}",
        only.warnings
    );

    let both = run(VerifyNode::Both).await;
    for warning in &only.warnings {
        assert!(
            both.warnings.contains(warning),
            "`--node both` dropped {warning:?}: {:?}",
            both.warnings
        );
    }
    drop(pair.servers);
}

#[tokio::test]
async fn verify_node_both_missing_on_backup() {
    let mut env = env();
    with_secret_refs(
        &mut env,
        &[("cap24-file", "CAP24-Name"), ("cap24-file", "Other")],
    );
    let pair = node_pair(&env).await;
    pair.master
        .seed_secret_names("cap24-file", &["CAP24-Name", "Other"]);
    pair.backup.seed_secret_names("cap24-file", &["Other"]);
    let result = run_node(&env, &pair, VerifyNode::Both).await;
    assert!(!result.ok);
    assert_eq!(result.slug, Some(VERIFY_FAILED_SLUG));
    assert_eq!(result.failure_exit_code(), Some(6));
    let secrets = result.secrets.as_ref().unwrap();
    assert_eq!(secrets.nodes[0].node, "master");
    assert_eq!(secrets.nodes[0].status, CheckStatus::Pass);
    assert_eq!(secrets.nodes[1].node, "backup");
    assert_eq!(secrets.nodes[1].status, CheckStatus::Fail);
    assert_eq!(
        secrets.nodes[1].missing,
        vec![pair_ref("cap24-file", "CAP24-Name")]
    );
    // The master's six checks are untouched by the backup's secrets.
    assert!(result.checks.iter().all(|c| c.status != CheckStatus::Fail));
    assert!(result.backup.as_ref().unwrap().converged);
}

#[tokio::test]
async fn verify_node_both_missing_on_master() {
    let mut env = env();
    with_secret_refs(
        &mut env,
        &[("cap24-file", "Zeta"), ("cap24-file", "CAP24-Name")],
    );
    let pair = node_pair(&env).await;
    pair.master.seed_secret_names("cap24-file", &[]);
    pair.backup
        .seed_secret_names("cap24-file", &["CAP24-Name", "Zeta"]);
    let result = run_node(&env, &pair, VerifyNode::Both).await;
    assert!(!result.ok);
    assert_eq!(result.slug, Some(VERIFY_FAILED_SLUG));
    let secrets = result.secrets.as_ref().unwrap();
    assert_eq!(secrets.nodes[0].status, CheckStatus::Fail);
    // Byte-sorted pairs.
    assert_eq!(
        secrets.nodes[0].missing,
        vec![
            pair_ref("cap24-file", "CAP24-Name"),
            pair_ref("cap24-file", "Zeta")
        ]
    );
    assert_eq!(secrets.nodes[1].status, CheckStatus::Pass);
}

#[tokio::test]
async fn verify_node_backup_checks_backup_secrets_only() {
    let mut env = env();
    with_secret_refs(&mut env, &[("cap24-file", "CAP24-Name")]);
    let pair = node_pair(&env).await;
    // The master lacks the name; `--node backup` does not look.
    pair.master.seed_secret_names("cap24-file", &[]);
    pair.backup.seed_secret_names("cap24-file", &["CAP24-Name"]);
    let result = run_node(&env, &pair, VerifyNode::Backup).await;
    assert!(result.ok, "{result:#?}");
    assert_eq!(result.node, "backup");
    assert!(result.checks.is_empty());
    assert_eq!(
        secret_statuses(&result),
        vec![("backup".to_string(), CheckStatus::Pass)]
    );
}

/// K7: a provider that cannot list its names (501, or the 500 of a `remote`
/// provider; the client maps both to `CannotList`) is named, status `warn`,
/// and does not fail the verdict.
#[tokio::test]
async fn verify_node_secrets_unverifiable() {
    let mut env = env();
    with_secret_refs(&mut env, &[("cap24-remote", "Anything")]);
    let pair = node_pair(&env).await;
    pair.master.secret_provider_cannot_list("cap24-remote");
    pair.backup.secret_provider_cannot_list("cap24-remote");
    let result = run_node(&env, &pair, VerifyNode::Both).await;
    assert!(result.ok, "{result:#?}");
    assert_eq!(result.slug, None);
    let secrets = result.secrets.as_ref().unwrap();
    for node in &secrets.nodes {
        assert_eq!(node.status, CheckStatus::Warn, "{node:?}");
        assert_eq!(node.unverifiable, vec!["cap24-remote".to_string()]);
        assert!(node.missing.is_empty());
    }
}

#[tokio::test]
async fn verify_node_secrets_none_referenced() {
    let env = env();
    let pair = node_pair(&env).await;
    let result = run_node(&env, &pair, VerifyNode::Both).await;
    assert!(result.ok, "{result:#?}");
    let secrets = result.secrets.as_ref().unwrap();
    assert_eq!(secrets.nodes.len(), 2);
    for node in &secrets.nodes {
        assert_eq!(node.status, CheckStatus::Pass);
        assert_eq!(node.providers_checked, 0);
        assert!(node.missing.is_empty() && node.unverifiable.is_empty());
    }
}

/// K8, verify side: a provider the render declares that a node answers 404
/// for is `missing` after the apply (the preflight skips it, verify does not).
#[tokio::test]
async fn verify_node_declared_provider_without_a_node_is_missing() {
    let mut env = env();
    declaring_provider(&mut env, "cap24-file");
    with_secret_refs(&mut env, &[("cap24-file", "CAP24-Name")]);
    let pair = node_pair(&env).await;
    // Neither node lists `cap24-file`: both answer "no such provider".
    let result = run_node(&env, &pair, VerifyNode::Both).await;
    assert!(!result.ok);
    assert_eq!(result.slug, Some(VERIFY_FAILED_SLUG));
    let secrets = result.secrets.as_ref().unwrap();
    for node in &secrets.nodes {
        assert_eq!(node.status, CheckStatus::Fail, "{node:?}");
        assert_eq!(node.missing, vec![pair_ref("cap24-file", "CAP24-Name")]);
        assert!(node.skipped_providers.is_empty());
    }
}

/// K9: names compare exactly, so `cap24-name` does not satisfy `CAP24-Name`.
#[tokio::test]
async fn verify_node_secret_names_are_case_sensitive() {
    let mut env = env();
    with_secret_refs(&mut env, &[("cap24-file", "CAP24-Name")]);
    let pair = node_pair(&env).await;
    for fake in [&pair.master, &pair.backup] {
        fake.seed_secret_names("cap24-file", &["cap24-name"]);
    }
    let result = run_node(&env, &pair, VerifyNode::Both).await;
    assert!(!result.ok);
    for node in &result.secrets.as_ref().unwrap().nodes {
        assert_eq!(node.missing, vec![pair_ref("cap24-file", "CAP24-Name")]);
    }
}

/// The default `Master` keeps today's shape: no `backup`, no `secrets`.
#[tokio::test]
async fn verify_node_master_has_no_backup_or_secrets() {
    let mut env = env();
    with_secret_refs(&mut env, &[("cap24-file", "CAP24-Name")]);
    let pair = node_pair(&env).await;
    let result = run_node(&env, &pair, VerifyNode::Master).await;
    assert!(result.ok, "{result:#?}");
    assert!(result.backup.is_none() && result.secrets.is_none());
    let keys: Vec<String> = serde_json::to_value(&result)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert!(
        !keys.iter().any(|k| k == "backup" || k == "secrets"),
        "{keys:?}"
    );
}

// ---- quick-261029-ms3 (F2): unreadable units on the backup ----------------------

use common::plan_fixture::MarkUnreadable;

/// `--node backup` with the backup's files read through a second reader of
/// the same data directory, `path` (mode collection) unreadable on it.
async fn run_backup_unreadable(env: &Env, pair: &NodePair, path: &str) -> DeployVerifyResult {
    let (_, inner) = env.fx.load("dev");
    let backup_reader = MarkUnreadable::new(inner, "mode", path, 262);
    let master_deploy = ForbidWrites(pair.master.client("verify"));
    let backup_deploy = ForbidWrites(pair.backup.client("verify-backup"));
    let backup = BackupNode::new(&pair.backup_api, &backup_deploy, Some(&backup_reader));
    deploy_verify_node(
        &env.declared,
        "ign-cli",
        VerifyNode::Backup,
        Some((&master_deploy, &pair.master_api)),
        env.reader.as_ref(),
        Some(&backup),
        &fast(),
    )
    .await
    .expect("verdict")
}

fn backup_files_check(result: &DeployVerifyResult) -> &ignition_core::deploy::pair::BackupCheck {
    result
        .backup
        .as_ref()
        .expect("a backup report")
        .checks
        .iter()
        .find(|c| c.id == ignition_core::deploy::pair::BackupCheckId::Files)
        .expect("a files check")
}

#[tokio::test]
async fn verify_node_backup_declared_unit_unreadable_on_the_backup_is_red() {
    let env = env();
    let pair = node_pair(&env).await;
    let result = run_backup_unreadable(&env, &pair, &format!("{DB}/config.json")).await;
    assert!(!result.ok, "{result:#?}");
    assert_eq!(result.slug, Some(VERIFY_FAILED_SLUG));
    let backup = result.backup.as_ref().unwrap();
    assert!(!backup.converged, "{backup:?}");
    let files = backup_files_check(&result);
    assert_eq!(files.status, CheckStatus::Fail, "{files:?}");
    assert!(
        files
            .findings
            .iter()
            .any(|f| f.code == "unreadable" && f.subject == DB),
        "{files:?}"
    );
    assert!(
        !files
            .findings
            .iter()
            .any(|f| f.code == "file_missing_on_backup"),
        "never read as absent: {files:?}"
    );
}

#[tokio::test]
async fn verify_node_backup_undeclared_unit_unreadable_on_the_backup_is_a_warning() {
    const OLD: &str = "ignition/database-translator/OLD";
    let env = env();
    let pair = node_pair(&env).await;
    let result = run_backup_unreadable(&env, &pair, &format!("{OLD}/config.json")).await;
    assert!(result.ok, "{result:#?}");
    assert!(result.backup.as_ref().unwrap().converged);
    assert_eq!(backup_files_check(&result).status, CheckStatus::Warn);
    let hits: Vec<&String> = result.warnings.iter().filter(|w| w.contains(OLD)).collect();
    assert_eq!(hits.len(), 1, "{:?}", result.warnings);
}

// ---- quick-261030-8m8: root-level and nested tag units (CD5a, CD5b) -----------------

const CD5_ROOT: &str = "ignition/tag-definition/P";
const CD5_NESTED: &str = "ignition/tag-type-definition/P/A/B";

/// Declare (and make live) a root-level unit and a nested type unit.
fn with_cd5_units(env: &mut Env) {
    let tree = &mut env.declared.outcome.tree.files;
    let (tags, udts) = (tag_marker(&["tags.json"]), tag_marker(&["udts.json"]));
    for (path, bytes) in [
        (format!("{CD5_ROOT}/unary-resource.json"), tags.as_str()),
        (format!("{CD5_ROOT}/tags.json"), "[]"),
        (format!("{CD5_NESTED}/unary-resource.json"), udts.as_str()),
        (format!("{CD5_NESTED}/udts.json"), "[]"),
    ] {
        tree.insert(RelPath::new(&path).unwrap(), bytes.as_bytes().to_vec());
        env.fx.mode(&path, bytes.as_bytes());
    }
    env.declared.manifest.tag_providers = vec!["P".to_string()];
}

#[tokio::test]
async fn cd5_verify_reports_drift_on_root_and_nested_units() {
    let mut env = env();
    with_cd5_units(&mut env);
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
        files.detail.starts_with("2 declared tag unit"),
        "{}",
        files.detail
    );

    // one byte off in the live root tags.json, then in the nested udts.json
    for (unit, file, good) in [
        (CD5_ROOT, "tags.json", "[]"),
        (CD5_NESTED, "udts.json", "[]"),
    ] {
        let path = format!("{unit}/{file}");
        env.fx.mode(&path, b"[ ]");
        let red = run(&env, &fake, &api).await.expect("verdict");
        assert!(!red.ok, "{path}");
        assert_eq!(red.failure_exit_code(), Some(6));
        let files = red.checks.iter().find(|c| c.id == CheckId::Files).unwrap();
        assert_eq!(files.status, CheckStatus::Fail);
        assert_eq!(files.findings.len(), 1, "{:?}", files.findings);
        assert_eq!(files.findings[0].code, "files_differ");
        assert_eq!(files.findings[0].subject, unit);
        assert!(!serde_json::to_string(&red).unwrap().contains("[ ]"));
        env.fx.mode(&path, good.as_bytes());
    }
}

// ============================================================================
// quick 261031-bbs: read_back judged against a pre-apply REST baseline (R4)
// and, standalone, warned on plan-`same` resources (R2)
// ============================================================================

use std::collections::BTreeSet;

use ignition_core::deploy::verify::{
    BaselineBasis, ReadBackBaseline, ReadBackJudge, Written, evaluate_with, path_covered,
};

const ALARM: &str = "ignition/general-alarm-settings";

/// `expected()` declaring only POSTGRES, with `file` as its `config.json`.
fn rb_expected(file: Value) -> Expected {
    let mut e = expected();
    e.resources = vec![ExpectedResource::exact(
        "ignition",
        "database-translator",
        Some("POSTGRES"),
        file,
    )];
    e
}

/// `observed()` with POSTGRES read back as `r1`.
fn rb_observed(r1: Remote) -> ObservedState {
    let mut o = observed();
    o.read_back = BTreeMap::from([(DB.to_string(), r1)]);
    o
}

/// POSTGRES's baseline: read as `r0` before the apply, written as `written`.
fn rb_baseline(e: &Expected, r0: Remote, written: Option<Written>) -> ReadBackBaseline {
    let before = BTreeMap::from([(DB.to_string(), r0)]);
    let written: BTreeMap<String, Written> = written
        .map(|w| BTreeMap::from([(DB.to_string(), w)]))
        .unwrap_or_default();
    ReadBackBaseline::build("master", e, &before, &written)
}

/// Judge POSTGRES declared as `file`, read as `r0` before and `r1` after.
fn rb_judge(file: Value, r0: Remote, r1: Remote, written: Option<Written>) -> VerifyReport {
    let e = rb_expected(file);
    let baseline = rb_baseline(&e, r0, written);
    evaluate_with(&e, &rb_observed(r1), &ReadBackJudge::Baseline(&baseline))
}

fn cfg(v: Value) -> Remote {
    Remote::Config(v)
}

fn pg() -> Value {
    json!({"name": "POSTGRES", "v": 1})
}

fn pg_filled() -> Value {
    json!({"name": "POSTGRES", "v": 1, "filled": true})
}

#[test]
fn rb_judge_unwritten_preexisting_warns() {
    let report = rb_judge(pg(), cfg(pg_filled()), cfg(pg_filled()), None);
    assert!(report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Warn, "{rb:#?}");
    assert_eq!(rb.findings.len(), 1, "{rb:#?}");
    assert_eq!(rb.findings[0].code, "resource_differs");
    assert_eq!(rb.findings[0].subject, DB);
    assert_eq!(rb.findings[0].status, Some(CheckStatus::Warn));
    assert_eq!(
        rb.detail,
        "1 declared resource(s) compared with the REST effective view; 1 difference(s) reported as warnings"
    );
    let line = format!(
        "read_back: {DB} differs from its file at: filled (already different before this apply)"
    );
    assert!(report.warnings.contains(&line), "{:?}", report.warnings);
    assert_eq!(report.slug, None);
}

#[test]
fn rb_judge_unwritten_moved_and_differs_fails() {
    let report = rb_judge(pg(), cfg(pg()), cfg(pg_filled()), None);
    assert!(!report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Fail);
    assert_eq!(rb.findings.len(), 1);
    assert_eq!(rb.findings[0].code, "resource_differs");
    assert_eq!(rb.findings[0].status, None);
    assert!(rb.findings[0].detail.contains("filled"), "{rb:#?}");
    assert!(
        rb.findings[0].detail.contains("changed during this apply"),
        "{rb:#?}"
    );
}

#[test]
fn rb_judge_unwritten_moved_toward_file_passes() {
    let before = json!({"name": "POSTGRES", "v": 1, "x": 9});
    let report = rb_judge(pg(), cfg(before), cfg(pg()), None);
    assert!(report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Pass);
    assert!(rb.findings.is_empty());
}

#[test]
fn rb_judge_unwritten_config_then_missing_fails() {
    let report = rb_judge(pg(), cfg(pg_filled()), Remote::Missing, None);
    assert!(!report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Fail);
    assert_eq!(codes(&report, CheckId::ReadBack), vec!["resource_missing"]);
    assert_eq!(rb.findings[0].status, None);
}

#[test]
fn rb_judge_missing_both_sides_warns() {
    let report = rb_judge(pg(), Remote::Missing, Remote::Missing, None);
    assert!(report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Warn);
    assert_eq!(codes(&report, CheckId::ReadBack), vec!["resource_missing"]);
    assert_eq!(rb.findings[0].status, Some(CheckStatus::Warn));
    let line = format!(
        "read_back: {DB} is not in the gateway's effective view (it was not there before this apply either)"
    );
    assert!(report.warnings.contains(&line), "{:?}", report.warnings);
}

#[test]
fn rb_judge_missing_then_config_differs_fails() {
    let report = rb_judge(pg(), Remote::Missing, cfg(pg_filled()), None);
    assert!(!report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Fail);
    assert_eq!(codes(&report, CheckId::ReadBack), vec!["resource_differs"]);
    assert!(rb.findings[0].detail.contains("filled"), "{rb:#?}");

    // Appearing equal to the file is a pass.
    let report = rb_judge(pg(), Remote::Missing, cfg(pg()), None);
    assert!(report.ok, "{report:#?}");
    assert_eq!(check(&report, CheckId::ReadBack).status, CheckStatus::Pass);
}

#[test]
fn rb_judge_written_within_live_baseline_warns() {
    // B = {settings} (the live file has none, REST does); the write takes
    // effect (REST moves at `v` and `settings.a`); P = {settings.extra}, a
    // member REST already filled in before the apply.
    let live = json!({"v": 1});
    let desired = json!({"v": 2, "settings": {"a": 5}});
    let r0 = json!({"v": 1, "settings": {"a": 1, "extra": 0}});
    let r1 = json!({"v": 2, "settings": {"a": 5, "extra": 0}});
    let e = rb_expected(desired);
    let baseline = rb_baseline(
        &e,
        cfg(r0),
        Some(Written::Changed(Some(Compare::Exact(live)))),
    );
    let entry = baseline.entry(DB).expect("entry");
    assert_eq!(entry.basis(), BaselineBasis::LiveFile);
    assert_eq!(
        entry.differs_at(),
        &BTreeSet::from(["settings".to_string()])
    );
    let report = evaluate_with(
        &e,
        &rb_observed(cfg(r1)),
        &ReadBackJudge::Baseline(&baseline),
    );
    assert!(report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Warn, "{rb:#?}");
    assert_eq!(rb.findings[0].status, Some(CheckStatus::Warn));
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains(DB) && w.contains("settings.extra")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn rb_judge_written_new_path_fails() {
    let live = json!({"v": 1});
    let desired = json!({"v": 2});
    let r0 = json!({"v": 1, "settings": {"a": 1}});
    let r1 = json!({"v": 2, "settings": {"a": 1}, "other": 3});
    let report = rb_judge(
        desired,
        cfg(r0),
        cfg(r1),
        Some(Written::Changed(Some(Compare::Exact(live)))),
    );
    assert!(!report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Fail);
    assert_eq!(rb.findings.len(), 1);
    let detail = &rb.findings[0].detail;
    assert!(detail.contains("other"), "{detail}");
    assert!(
        !detail.contains("settings"),
        "names the new path only: {detail}"
    );
    assert!(
        detail.contains("not different before this apply"),
        "{detail}"
    );
}

#[test]
fn rb_judge_prefix_rule() {
    let a = BTreeSet::from(["a".to_string()]);
    assert!(path_covered("a", &a));
    assert!(path_covered("a.b", &a));
    assert!(path_covered("a[n]", &a));
    assert!(path_covered("a.b.c", &a));
    assert!(!path_covered("ab", &a));
    assert!(!path_covered("b.a", &a));
    let root = BTreeSet::from(["(root)".to_string()]);
    for p in ["a", "ab", "x.y[z]", "(root)"] {
        assert!(path_covered(p, &root), "{p}");
    }
    assert!(!path_covered("a", &BTreeSet::new()));

    // Through the judge: `ab` is not under `a`.
    let live = json!({"v": 1});
    let desired = json!({"v": 2});
    let r0 = json!({"v": 1, "a": 1});
    let r1 = json!({"v": 2, "a": 1, "ab": 1});
    let report = rb_judge(
        desired,
        cfg(r0),
        cfg(r1),
        Some(Written::Changed(Some(Compare::Exact(live)))),
    );
    assert!(!report.ok, "{report:#?}");
    let detail = &check(&report, CheckId::ReadBack).findings[0].detail;
    assert!(detail.contains("ab"), "{detail}");
}

#[test]
fn rb_judge_write_not_taken_effect_fails() {
    // The live file equals r0, the desired file changes `m`, REST did not move.
    let live = json!({"name": "POSTGRES", "m": 1});
    let desired = json!({"name": "POSTGRES", "m": 2});
    let report = rb_judge(
        desired,
        cfg(live.clone()),
        cfg(live.clone()),
        Some(Written::Changed(Some(Compare::Exact(live)))),
    );
    assert!(!report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Fail);
    assert_eq!(rb.findings[0].status, None);
    assert!(
        rb.findings[0].detail.starts_with("differs at: m"),
        "{rb:#?}"
    );
}

#[test]
fn rb_judge_write_not_taken_effect_root_covered_fails() {
    // A1: B = {(root)} covers every path, but the REST view did not move
    // although the desired file differs from the live one.
    let live = json!([1]);
    let desired = json!({"m": 2});
    let r = json!({"m": 1});
    let e = rb_expected(desired.clone());
    let baseline = rb_baseline(
        &e,
        cfg(r.clone()),
        Some(Written::Changed(Some(Compare::Exact(live.clone())))),
    );
    assert_eq!(
        baseline.entry(DB).expect("entry").differs_at(),
        &BTreeSet::from(["(root)".to_string()])
    );
    let report = evaluate_with(
        &e,
        &rb_observed(cfg(r)),
        &ReadBackJudge::Baseline(&baseline),
    );
    assert!(!report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Fail);
    assert_eq!(rb.findings[0].status, None);
    assert!(
        rb.findings[0].detail.starts_with("differs at: m"),
        "{rb:#?}"
    );
}

/// An OPC UA device whose REST view reports `settings.deviceSettings` as one
/// opaque value (a string here) while the file holds it structured, so before
/// the apply REST differs from the live file at `settings.deviceSettings` as
/// a whole. That one path covers every member under it. (Production's
/// Chiller1 is the other shape: REST has no `deviceSettings` key at all; see
/// `chiller_blob_file`.)
fn chiller(enabled: bool, host: &str) -> Value {
    json!({
        "name": "Chiller1",
        "settings": {"enabled": enabled, "deviceSettings": {"hostname": host, "port": 102}}
    })
}

fn chiller_rest(enabled: bool, host: &str) -> Value {
    json!({
        "name": "Chiller1",
        "settings": {"enabled": enabled, "deviceSettings": format!("hostname={host};port=102")}
    })
}

#[test]
fn rb_judge_partial_write_covered_path_fails() {
    // CR-01: the write changes `settings.deviceSettings.hostname` and
    // `settings.enabled`; the gateway applies `enabled` only. The hostname
    // still differs under the covered `settings.deviceSettings`, and REST did
    // not move there: that part of the write did not take effect. Red,
    // although another member moved.
    let live = chiller(false, "plc-old");
    let desired = chiller(true, "plc-new");
    let e = rb_expected(desired);
    let baseline = rb_baseline(
        &e,
        cfg(chiller_rest(false, "plc-old")),
        Some(Written::Changed(Some(Compare::Exact(live)))),
    );
    let entry = baseline.entry(DB).expect("entry");
    assert_eq!(
        entry.differs_at(),
        &BTreeSet::from(["settings.deviceSettings".to_string()])
    );
    assert_eq!(
        entry.writes_at(),
        &BTreeSet::from([
            "settings.deviceSettings.hostname".to_string(),
            "settings.enabled".to_string()
        ])
    );
    let report = evaluate_with(
        &e,
        &rb_observed(cfg(chiller_rest(true, "plc-old"))),
        &ReadBackJudge::Baseline(&baseline),
    );
    assert!(!report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Fail, "{rb:#?}");
    assert_eq!(rb.findings.len(), 1, "{rb:#?}");
    assert_eq!(rb.findings[0].code, "resource_differs");
    assert_eq!(rb.findings[0].status, None);
    let detail = &rb.findings[0].detail;
    assert!(
        detail.starts_with("differs at: settings.deviceSettings.hostname"),
        "{detail}"
    );
    assert!(
        !detail.contains("settings.enabled"),
        "the applied member is not named: {detail}"
    );
    assert!(detail.contains("did not take effect"), "{detail}");
}

#[test]
fn rb_judge_full_write_covered_path_passes() {
    // CR-01 control: the same write with both members applied. REST moved at
    // `settings.deviceSettings` (an ancestor of the hostname), and the
    // difference left is the covered pre-existing one: a warning, green.
    let live = chiller(false, "plc-old");
    let desired = chiller(true, "plc-new");
    let report = rb_judge(
        desired,
        cfg(chiller_rest(false, "plc-old")),
        cfg(chiller_rest(true, "plc-new")),
        Some(Written::Changed(Some(Compare::Exact(live)))),
    );
    assert!(report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Warn, "{rb:#?}");
    assert_eq!(rb.findings[0].status, Some(CheckStatus::Warn));
}

#[test]
fn rb_judge_partial_write_root_covered_fails() {
    // CR-01 under `(root)` coverage: REST lists a gateway-provided third
    // entry, so before the apply it differs from the live file at the root
    // and `(root)` covers every path. The write changes `0.host` and
    // `1.port`; the gateway applies `0.host` only. Red on `1.port`.
    let live = json!([{"host": "a", "port": 1}, {"host": "b", "port": 2}]);
    let desired = json!([{"host": "a2", "port": 1}, {"host": "b", "port": 3}]);
    let system = json!({"host": "sys", "port": 9});
    let r0 = json!([{"host": "a", "port": 1}, {"host": "b", "port": 2}, system.clone()]);
    let partial = json!([{"host": "a2", "port": 1}, {"host": "b", "port": 2}, system.clone()]);
    let full = json!([{"host": "a2", "port": 1}, {"host": "b", "port": 3}, system]);
    let written = || Some(Written::Changed(Some(Compare::Exact(live.clone()))));

    let e = rb_expected(desired.clone());
    let baseline = rb_baseline(&e, cfg(r0.clone()), written());
    assert_eq!(
        baseline.entry(DB).expect("entry").differs_at(),
        &BTreeSet::from(["(root)".to_string()])
    );
    let report = evaluate_with(
        &e,
        &rb_observed(cfg(partial)),
        &ReadBackJudge::Baseline(&baseline),
    );
    assert!(!report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Fail, "{rb:#?}");
    assert_eq!(rb.findings[0].status, None);
    let detail = &rb.findings[0].detail;
    assert!(detail.starts_with("differs at: 1.port"), "{detail}");
    assert!(!detail.contains("0.host"), "{detail}");
    assert!(detail.contains("did not take effect"), "{detail}");

    // Both members applied: a warning, green.
    let report = rb_judge(desired, cfg(r0), cfg(full), written());
    assert!(report.ok, "{report:#?}");
    assert_eq!(check(&report, CheckId::ReadBack).status, CheckStatus::Warn);
}

/// Production's Chiller1 shape (synthetic values): the file carries
/// `settings.deviceSettings` as a non-JSON string blob, and the REST view's
/// `settings` has no `deviceSettings` key at all; its other members match.
fn chiller_blob_file(enabled: bool, blob: &str) -> Value {
    json!({
        "name": "Chiller1",
        "settings": {"enabled": enabled, "deviceSettings": blob, "pollRate": 1000}
    })
}

fn chiller_blob_rest(enabled: bool) -> Value {
    json!({"name": "Chiller1", "settings": {"enabled": enabled, "pollRate": 1000}})
}

/// The detail of the warn finding for a written path REST does not show.
fn unobservable_detail(paths: &str) -> String {
    format!("{paths} is not visible in the gateway's REST view, so the write could not be verified")
}

#[test]
fn rb_judge_unobservable_blob_other_member_moving_warns() {
    // The write changes the blob and `settings.enabled`; `enabled` lands.
    // REST never shows `settings.deviceSettings`, before or after, so that
    // part cannot be verified through REST: a warning, not a red.
    let e = rb_expected(chiller_blob_file(true, "blob-new"));
    let baseline = rb_baseline(
        &e,
        cfg(chiller_blob_rest(false)),
        Some(Written::Changed(Some(Compare::Exact(chiller_blob_file(
            false, "blob-old",
        ))))),
    );
    assert_eq!(
        baseline.entry(DB).expect("entry").writes_at(),
        &BTreeSet::from([
            "settings.deviceSettings".to_string(),
            "settings.enabled".to_string()
        ])
    );
    let report = evaluate_with(
        &e,
        &rb_observed(cfg(chiller_blob_rest(true))),
        &ReadBackJudge::Baseline(&baseline),
    );
    assert!(report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Warn, "{rb:#?}");
    assert_eq!(rb.findings.len(), 1, "{rb:#?}");
    assert_eq!(rb.findings[0].code, "resource_differs");
    assert_eq!(rb.findings[0].subject, DB);
    assert_eq!(rb.findings[0].status, Some(CheckStatus::Warn));
    assert_eq!(
        rb.findings[0].detail,
        unobservable_detail("settings.deviceSettings")
    );
    let line = format!(
        "read_back: {DB}: {}",
        unobservable_detail("settings.deviceSettings")
    );
    assert!(report.warnings.contains(&line), "{:?}", report.warnings);
}

#[test]
fn rb_judge_unobservable_blob_alone_warns() {
    // Only the blob is written, so nothing REST shows moves. The whole-view
    // A1 rule must not read that as a write that did not take effect.
    let report = rb_judge(
        chiller_blob_file(true, "blob-new"),
        cfg(chiller_blob_rest(true)),
        cfg(chiller_blob_rest(true)),
        Some(Written::Changed(Some(Compare::Exact(chiller_blob_file(
            true, "blob-old",
        ))))),
    );
    assert!(report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Warn, "{rb:#?}");
    assert_eq!(rb.findings.len(), 1, "{rb:#?}");
    assert_eq!(rb.findings[0].status, Some(CheckStatus::Warn));
    assert_eq!(
        rb.findings[0].detail,
        unobservable_detail("settings.deviceSettings")
    );
}

#[test]
fn rb_judge_unobservable_null_config_warns() {
    // Production's Cirrus namespace-server-set shape (synthetic values): the
    // file is `{namespace, serverSet}` and the REST view carries no config
    // (`null`) before or after. Nothing written can be seen through REST.
    let live = json!({"namespace": "ns-old", "serverSet": "set-a"});
    let desired = json!({"namespace": "ns-new", "serverSet": "set-a"});
    let report = rb_judge(
        desired,
        cfg(Value::Null),
        cfg(Value::Null),
        Some(Written::Changed(Some(Compare::Exact(live)))),
    );
    assert!(report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Warn, "{rb:#?}");
    assert_eq!(rb.findings.len(), 1, "{rb:#?}");
    assert_eq!(rb.findings[0].status, Some(CheckStatus::Warn));
    assert_eq!(rb.findings[0].detail, unobservable_detail("namespace"));
}

#[test]
fn rb_judge_write_already_shown_root_covered_warns() {
    // REST lists a gateway-provided third entry, so `(root)` covers every
    // path. Before the apply REST already shows the desired `0.host` (the
    // live file was behind what the gateway reports): a write that changes
    // nothing REST shows. It must not read as a write that did not take
    // effect, alone (A1) or beside a member that does move (CR-01).
    let system = json!({"host": "sys", "port": 9});
    let live = json!([{"host": "a", "port": 1}, {"host": "b", "port": 2}]);
    let r0 = json!([{"host": "a2", "port": 1}, {"host": "b", "port": 2}, system.clone()]);

    // Alone: REST does not move at all.
    let desired = json!([{"host": "a2", "port": 1}, {"host": "b", "port": 2}]);
    let report = rb_judge(
        desired,
        cfg(r0.clone()),
        cfg(r0.clone()),
        Some(Written::Changed(Some(Compare::Exact(live.clone())))),
    );
    assert!(report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Warn, "{rb:#?}");
    assert_eq!(rb.findings.len(), 1, "{rb:#?}");
    assert_eq!(rb.findings[0].status, Some(CheckStatus::Warn));
    assert_eq!(
        rb.findings[0].detail,
        "differs at: (root) (already different before this apply)"
    );

    // Beside `1.port`, which lands.
    let desired = json!([{"host": "a2", "port": 1}, {"host": "b", "port": 3}]);
    let r1 = json!([{"host": "a2", "port": 1}, {"host": "b", "port": 3}, system]);
    let report = rb_judge(
        desired,
        cfg(r0),
        cfg(r1),
        Some(Written::Changed(Some(Compare::Exact(live)))),
    );
    assert!(report.ok, "{report:#?}");
    assert_eq!(check(&report, CheckId::ReadBack).status, CheckStatus::Warn);
}

#[test]
fn rb_judge_masked_partial_write_fails() {
    // The scenario `rb_judge_written_within_live_baseline_warns` held before
    // CR-01: desired `settings.a = 5`, REST at 1 before and after, while `v`
    // lands. `settings` is covered (the live file has none) and REST moved
    // elsewhere, so before CR-01 this unapplied write was a green warning.
    let live = json!({"v": 1});
    let desired = json!({"v": 2, "settings": {"a": 5}});
    let r0 = json!({"v": 1, "settings": {"a": 1, "extra": 0}});
    let r1 = json!({"v": 2, "settings": {"a": 1, "extra": 0}});
    let report = rb_judge(
        desired,
        cfg(r0),
        cfg(r1),
        Some(Written::Changed(Some(Compare::Exact(live)))),
    );
    assert!(!report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Fail, "{rb:#?}");
    assert_eq!(rb.findings.len(), 1, "{rb:#?}");
    assert_eq!(rb.findings[0].status, None);
    let detail = &rb.findings[0].detail;
    assert!(detail.starts_with("differs at: settings ("), "{detail}");
    assert!(detail.contains("did not take effect"), "{detail}");
}

#[test]
fn rb_judge_two_members_under_structured_ancestor_one_lands_fails() {
    // REST lists a gateway-provided entry in `s`, so `s` differed before and
    // covers both written members. The write changes `s.0.a` and `s.1.b`;
    // the gateway applies `s.0.a` only. Red on `s.1.b`; both landing warns.
    let live = json!({"s": [{"a": 1}, {"b": 1}]});
    let desired = json!({"s": [{"a": 2}, {"b": 2}]});
    let r0 = json!({"s": [{"a": 1}, {"b": 1}, {"sys": 9}]});
    let partial = json!({"s": [{"a": 2}, {"b": 1}, {"sys": 9}]});
    let full = json!({"s": [{"a": 2}, {"b": 2}, {"sys": 9}]});
    let written = || Some(Written::Changed(Some(Compare::Exact(live.clone()))));
    let e = rb_expected(desired.clone());
    let baseline = rb_baseline(&e, cfg(r0.clone()), written());
    let entry = baseline.entry(DB).expect("entry");
    assert_eq!(entry.differs_at(), &BTreeSet::from(["s".to_string()]));
    assert_eq!(
        entry.writes_at(),
        &BTreeSet::from(["s.0.a".to_string(), "s.1.b".to_string()])
    );
    let report = evaluate_with(
        &e,
        &rb_observed(cfg(partial)),
        &ReadBackJudge::Baseline(&baseline),
    );
    assert!(!report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Fail, "{rb:#?}");
    let detail = &rb.findings[0].detail;
    assert!(detail.starts_with("differs at: s.1.b ("), "{detail}");

    let report = rb_judge(desired, cfg(r0), cfg(full), written());
    assert!(report.ok, "{report:#?}");
    assert_eq!(check(&report, CheckId::ReadBack).status, CheckStatus::Warn);
}

#[test]
fn rb_judge_attributes_partial_write_fails() {
    // CR-01 on the `Attributes` compare kind (a unit with no config.json,
    // N3): REST normalises `b`, so `b` differed before. The write changes
    // `a` and `b`; the gateway applies `a` only. Red on `b`.
    let attrs = |a: i64, b: i64| {
        let Value::Object(map) = json!({"a": a, "b": b}) else {
            unreachable!()
        };
        map
    };
    let mut e = expected();
    e.resources = vec![ExpectedResource::new(
        "ignition",
        "database-translator",
        Some("POSTGRES"),
        Compare::Attributes(attrs(2, 2)),
    )];
    let r0 = json!({"a": 1, "b": "norm", "extra": 0});
    let baseline = rb_baseline(
        &e,
        cfg(r0),
        Some(Written::Changed(Some(Compare::Attributes(attrs(1, 1))))),
    );
    let entry = baseline.entry(DB).expect("entry");
    assert_eq!(entry.basis(), BaselineBasis::LiveFile);
    assert_eq!(entry.differs_at(), &BTreeSet::from(["b".to_string()]));
    assert_eq!(
        entry.writes_at(),
        &BTreeSet::from(["a".to_string(), "b".to_string()])
    );
    let judge = ReadBackJudge::Baseline(&baseline);

    let partial = json!({"a": 2, "b": "norm", "extra": 0});
    let report = evaluate_with(&e, &rb_observed(cfg(partial)), &judge);
    assert!(!report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Fail, "{rb:#?}");
    let detail = &rb.findings[0].detail;
    assert!(detail.starts_with("differs at: b ("), "{detail}");
    assert!(detail.contains("did not take effect"), "{detail}");

    let full = json!({"a": 2, "b": 2, "extra": 0});
    let report = evaluate_with(&e, &rb_observed(cfg(full)), &judge);
    assert!(report.ok, "{report:#?}");
    assert_eq!(check(&report, CheckId::ReadBack).status, CheckStatus::Pass);
}

// ---- accepted residual risk: path collapse ----------------------------------
//
// `writes_at` holds the shallowest path at which the desired file and the live
// one differ. Where a whole subtree differs (a key the live file lacks, an
// array whose length changes, a member REST reports as one opaque value) the
// written leaves collapse into one path, and any REST move under it counts
// for all of them. These tests pin today's verdict, a false green, so that a
// collapse-aware fix (segment-vector paths, IN-03) flips them deliberately.

#[test]
fn rb_judge_residual_collapse_key_absent_in_live_warns() {
    // The live file has no `s`, so `writes_at` is `{s}`. The gateway applies
    // `s.b` but not `s.a`; the move at `s.b` covers `s`. Today: a warning.
    let live = json!({"v": 1});
    let desired = json!({"v": 1, "s": {"a": 5, "b": 6}});
    let r0 = json!({"v": 1, "s": {"a": 1, "b": 1}});
    let r1 = json!({"v": 1, "s": {"a": 1, "b": 6}});
    let e = rb_expected(desired.clone());
    let written = || Some(Written::Changed(Some(Compare::Exact(live.clone()))));
    let baseline = rb_baseline(&e, cfg(r0.clone()), written());
    assert_eq!(
        baseline.entry(DB).expect("entry").writes_at(),
        &BTreeSet::from(["s".to_string()])
    );
    let report = rb_judge(desired, cfg(r0), cfg(r1), written());
    assert!(report.ok, "accepted residual risk: {report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Warn, "{rb:#?}");
    assert_eq!(
        rb.findings[0].detail,
        "differs at: s.a (already different before this apply)"
    );
}

#[test]
fn rb_judge_residual_collapse_array_length_warns() {
    // The write grows `l` from one item to three, so `writes_at` is `{l}`.
    // REST (which lists a gateway item) takes the second item but not the
    // third; the length change counts as a move at `l`. Today: a warning.
    let live = json!({"l": [1]});
    let desired = json!({"l": [1, 2, 3]});
    let r0 = json!({"l": [1, 9]});
    let r1 = json!({"l": [1, 2, 9]});
    let report = rb_judge(
        desired,
        cfg(r0),
        cfg(r1),
        Some(Written::Changed(Some(Compare::Exact(live)))),
    );
    assert!(report.ok, "accepted residual risk: {report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Warn, "{rb:#?}");
    assert_eq!(
        rb.findings[0].detail,
        "differs at: l.2 (already different before this apply)"
    );
}

#[test]
fn rb_judge_residual_opaque_rest_value_warns() {
    // REST reports `settings.deviceSettings` as one opaque string. The write
    // changes the hostname and the port under it; the gateway applies the
    // port only. The string moves, which covers both. Today: a warning.
    let file = |host: &str, port: i64| json!({"settings": {"deviceSettings": {"hostname": host, "port": port}}});
    let rest = |host: &str, port: i64| json!({"settings": {"deviceSettings": format!("hostname={host};port={port}")}});
    let report = rb_judge(
        file("plc-new", 103),
        cfg(rest("plc-old", 102)),
        cfg(rest("plc-old", 103)),
        Some(Written::Changed(Some(Compare::Exact(file("plc-old", 102))))),
    );
    assert!(report.ok, "accepted residual risk: {report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Warn, "{rb:#?}");
    assert_eq!(
        rb.findings[0].detail,
        "differs at: settings.deviceSettings (already different before this apply)"
    );
}

#[test]
fn rb_judge_written_now_system_fails() {
    // WR-01: after the apply the effective copy of a unit the run wrote is
    // in the `system` collection, so the written file is not what the
    // gateway runs: the write did not take effect. Red, and counted.
    let live = json!({"name": "POSTGRES", "v": 0});
    let cases = [
        (
            cfg(live.clone()),
            Written::Changed(Some(Compare::Exact(live))),
        ),
        (cfg(pg()), Written::Changed(None)),
        (Remote::Missing, Written::Added),
        (Remote::System, Written::Added),
    ];
    for (r0, written) in cases {
        let report = rb_judge(pg(), r0.clone(), Remote::System, Some(written.clone()));
        let what = format!("{r0:?} {written:?}");
        assert!(!report.ok, "{what}: {report:#?}");
        let rb = check(&report, CheckId::ReadBack);
        assert_eq!(rb.status, CheckStatus::Fail, "{what}");
        assert_eq!(rb.findings.len(), 1, "{what}");
        assert_eq!(rb.findings[0].code, "resource_missing", "{what}");
        assert_eq!(rb.findings[0].subject, DB, "{what}");
        assert_eq!(rb.findings[0].status, None, "{what}");
        assert!(
            rb.findings[0].detail.contains("`system` collection"),
            "{what}: {rb:#?}"
        );
        assert!(
            rb.findings[0].detail.contains("did not take effect"),
            "{what}: {rb:#?}"
        );
        assert_eq!(report.resources_checked, 1, "{what}");
    }

    // Unwritten and file-backed before: its file-backed copy dropped out of
    // the effective view during this apply. Red.
    let report = rb_judge(pg(), cfg(pg()), Remote::System, None);
    assert!(!report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(codes(&report, CheckId::ReadBack), vec!["resource_missing"]);
    assert!(
        rb.findings[0].detail.contains("during this apply"),
        "{rb:#?}"
    );

    // Unwritten and `system` before as well: still the not-compared warning.
    let report = rb_judge(pg(), Remote::System, Remote::System, None);
    assert!(report.ok, "{report:#?}");
    assert_eq!(check(&report, CheckId::ReadBack).status, CheckStatus::Pass);
    assert_eq!(report.resources_checked, 0);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.starts_with(&format!("read_back: {DB} not compared"))),
        "{:?}",
        report.warnings
    );
}

#[test]
fn rb_judge_added_is_strict() {
    for written in [Written::Added, Written::Changed(None)] {
        let report = rb_judge(
            pg(),
            cfg(pg_filled()),
            cfg(pg_filled()),
            Some(written.clone()),
        );
        assert!(!report.ok, "{written:?}: {report:#?}");
        let rb = check(&report, CheckId::ReadBack);
        assert_eq!(rb.status, CheckStatus::Fail, "{written:?}");
        assert_eq!(rb.findings[0].status, None, "{written:?}");
    }
    let e = rb_expected(pg());
    let baseline = rb_baseline(&e, cfg(pg_filled()), Some(Written::Added));
    assert_eq!(
        baseline.entry(DB).expect("entry").basis(),
        BaselineBasis::Strict
    );
}

#[test]
fn rb_judge_uncapped_paths() {
    // B holds 25 paths and P is those same 25: never derived from the
    // detail, which names 20.
    let mut r0 = serde_json::Map::new();
    let mut r1 = serde_json::Map::new();
    r0.insert("m".to_string(), json!(1));
    r1.insert("m".to_string(), json!(2));
    for i in 0..25 {
        r0.insert(format!("k{i:02}"), json!(0));
        r1.insert(format!("k{i:02}"), json!(0));
    }
    let live = json!({"m": 1});
    let desired = json!({"m": 2});
    let e = rb_expected(desired);
    let baseline = rb_baseline(
        &e,
        Remote::Config(Value::Object(r0)),
        Some(Written::Changed(Some(Compare::Exact(live)))),
    );
    assert_eq!(baseline.entry(DB).expect("entry").differs_at().len(), 25);
    let report = evaluate_with(
        &e,
        &rb_observed(Remote::Config(Value::Object(r1))),
        &ReadBackJudge::Baseline(&baseline),
    );
    assert!(report.ok, "{report:#?}");
    assert_eq!(check(&report, CheckId::ReadBack).status, CheckStatus::Warn);
}

#[test]
fn rb_judge_standalone_same_warns() {
    let file_status = BTreeMap::from([(DB.to_string(), PlanStatus::Same)]);
    let judge = ReadBackJudge::Standalone {
        file_status: &file_status,
    };
    let e = rb_expected(pg());

    let report = evaluate_with(&e, &rb_observed(cfg(pg_filled())), &judge);
    assert!(report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Warn);
    assert_eq!(rb.findings[0].code, "resource_differs");
    assert_eq!(rb.findings[0].status, Some(CheckStatus::Warn));
    let line = format!(
        "read_back: {DB} differs from its file at: filled (the live files equal the repo; the gateway's REST view differs)"
    );
    assert!(report.warnings.contains(&line), "{:?}", report.warnings);

    let report = evaluate_with(&e, &rb_observed(Remote::Missing), &judge);
    assert!(report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Warn);
    assert_eq!(rb.findings[0].code, "resource_missing");
    assert_eq!(rb.findings[0].status, Some(CheckStatus::Warn));
    let line = format!(
        "read_back: {DB} is not in the gateway's effective view (the live files equal the repo)"
    );
    assert!(report.warnings.contains(&line), "{:?}", report.warnings);
}

#[test]
fn rb_judge_standalone_changed_or_unknown_fails() {
    let e = rb_expected(pg());
    let statuses = [
        BTreeMap::from([(DB.to_string(), PlanStatus::Changed)]),
        BTreeMap::from([(DB.to_string(), PlanStatus::Added)]),
        BTreeMap::new(),
    ];
    for file_status in &statuses {
        let judge = ReadBackJudge::Standalone { file_status };
        for r1 in [cfg(pg_filled()), Remote::Missing] {
            let report = evaluate_with(&e, &rb_observed(r1.clone()), &judge);
            assert!(!report.ok, "{file_status:?} {r1:?}: {report:#?}");
            let rb = check(&report, CheckId::ReadBack);
            assert_eq!(rb.status, CheckStatus::Fail);
            assert_eq!(rb.findings[0].status, None);
        }
    }
}

#[test]
fn rb_judge_mixed_check_status() {
    // POSTGRES already differed (warn); the alarm settings moved and differ
    // (red).
    let mut e = rb_expected(pg());
    e.resources.push(ExpectedResource::exact(
        "ignition",
        "general-alarm-settings",
        None,
        json!({"enabled": true}),
    ));
    let before = BTreeMap::from([
        (DB.to_string(), cfg(pg_filled())),
        (ALARM.to_string(), cfg(json!({"enabled": true}))),
    ]);
    let baseline = ReadBackBaseline::build("master", &e, &before, &BTreeMap::new());
    let mut o = observed();
    o.read_back = BTreeMap::from([
        (DB.to_string(), cfg(pg_filled())),
        (ALARM.to_string(), cfg(json!({"enabled": true, "x": 1}))),
    ]);
    let report = evaluate_with(&e, &o, &ReadBackJudge::Baseline(&baseline));
    assert!(!report.ok, "{report:#?}");
    let rb = check(&report, CheckId::ReadBack);
    assert_eq!(rb.status, CheckStatus::Fail);
    assert_eq!(
        rb.detail,
        "2 declared resource(s) compared with the REST effective view"
    );
    let by_subject: BTreeMap<&str, Option<CheckStatus>> = rb
        .findings
        .iter()
        .map(|f| (f.subject.as_str(), f.status))
        .collect();
    assert_eq!(by_subject.get(DB), Some(&Some(CheckStatus::Warn)));
    assert_eq!(by_subject.get(ALARM), Some(&None));
}

#[test]
fn rb_judge_finding_status_serialization() {
    let red = rb_judge(pg(), cfg(pg()), cfg(pg_filled()), None);
    let red = serde_json::to_value(&check(&red, CheckId::ReadBack).findings[0]).unwrap();
    assert!(red.get("status").is_none(), "{red}");

    let warn = rb_judge(pg(), cfg(pg_filled()), cfg(pg_filled()), None);
    let warn = serde_json::to_value(&check(&warn, CheckId::ReadBack).findings[0]).unwrap();
    assert_eq!(warn["status"], "warn", "{warn}");

    // `evaluate` (Strict) of an existing CV2 case is unchanged, byte for byte.
    let mut obs = observed();
    obs.read_back.insert(DB.to_string(), Remote::Missing);
    let strict = evaluate(&expected(), &obs);
    assert_eq!(
        serde_json::to_string(check(&strict, CheckId::ReadBack)).unwrap(),
        format!(
            r#"{{"id":"read_back","status":"fail","detail":"1 declared resource(s) compared with the REST effective view","findings":[{{"code":"resource_missing","subject":"{DB}","detail":"not in the gateway's effective view"}}]}}"#
        )
    );
    let judged = evaluate_with(&expected(), &obs, &ReadBackJudge::Strict);
    assert_eq!(judged, strict);
}

#[test]
fn rb_judge_record_value_free() {
    // R5/P5: the record of a baseline holds the state, the basis and the
    // paths, never a value, and no hash of the values (IN-01).
    const RB_CANARY: &str = "CANARY-rb-0f3a";
    let e = rb_expected(pg());
    let r0 = json!({"name": "POSTGRES", "v": 1, "secretish": RB_CANARY, "nested": {"a": 1}});
    let record = serde_json::to_value(rb_baseline(&e, cfg(r0), None).record()).unwrap();
    let text = record.to_string();
    assert!(!text.contains(RB_CANARY), "{text}");
    assert_eq!(record["node"], "master");
    let entry = &record["resources"][DB];
    assert_eq!(entry["state"], "config", "{record}");
    assert_eq!(entry["basis"], "declared_file", "{record}");
    assert_eq!(
        entry["differs_at"],
        json!(["nested", "secretish"]),
        "{record}"
    );
    // Exactly these keys: no hash of the REST view, which with the repo's
    // file would be a guessing oracle for a low-entropy field.
    let keys: BTreeSet<&str> = entry
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        BTreeSet::from(["basis", "differs_at", "state"]),
        "{record}"
    );
    assert!(!text.contains("sha256"), "{text}");

    // Missing: no paths. A written `changed` unit with a live comparison is
    // on the live-file basis; an added one is strict.
    let missing = serde_json::to_value(rb_baseline(&e, Remote::Missing, None).record()).unwrap();
    assert_eq!(missing["resources"][DB]["state"], "missing");
    assert_eq!(missing["resources"][DB]["differs_at"], json!([]));
    let live = rb_baseline(
        &e,
        cfg(pg_filled()),
        Some(Written::Changed(Some(Compare::Exact(pg())))),
    );
    let live = serde_json::to_value(live.record()).unwrap();
    assert_eq!(live["resources"][DB]["basis"], "live_file");
    assert_eq!(live["resources"][DB]["differs_at"], json!(["filled"]));
    let added =
        serde_json::to_value(rb_baseline(&e, cfg(pg()), Some(Written::Added)).record()).unwrap();
    assert_eq!(added["resources"][DB]["basis"], "strict");
}

// ---- standalone verify through the action (R2) ------------------------------

/// The P4 standalone warning line for a missing resource.
fn standalone_missing_line(key: &str) -> String {
    format!(
        "read_back: {key} is not in the gateway's effective view (the live files equal the repo)"
    )
}

/// Drift the live copy of the tracer translator, so the plan says `changed`.
fn drift_live_translator(env: &Env) {
    let path = format!("{NAMED}/config.json");
    let mut config: Value = serde_json::from_slice(&env.fx.read(MODE, &path)).expect("json");
    config["driftedLive"] = json!(true);
    env.fx
        .mode(&path, &serde_json::to_vec_pretty(&config).expect("bytes"));
}

fn read_back_of(result: &DeployVerifyResult) -> &Check {
    result
        .checks
        .iter()
        .find(|c| c.id == CheckId::ReadBack)
        .expect("read_back")
}

#[tokio::test]
async fn rb_verify_standalone_same_warns_through_action() {
    let env = env();
    let server = wiremock::MockServer::start().await;
    mount_logs(&server, Vec::new()).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);

    // Missing from the REST view while the live files equal the repo.
    let fake = seeded_fake(&env.declared, false).await;
    let result = run(&env, &fake, &api).await.expect("verdict");
    assert!(result.ok, "{result:#?}");
    assert_eq!(result.slug, None);
    assert_eq!(result.failure_exit_code(), None);
    let rb = read_back_of(&result);
    assert_eq!(rb.status, CheckStatus::Warn);
    assert_eq!(rb.findings[0].code, "resource_missing");
    assert_eq!(rb.findings[0].subject, NAMED);
    assert_eq!(rb.findings[0].status, Some(CheckStatus::Warn));
    assert!(
        result.warnings.contains(&standalone_missing_line(NAMED)),
        "{:?}",
        result.warnings
    );

    // An extra member the gateway fills into its REST view.
    let fake = seeded_fake(&env.declared, true).await;
    fake.rest_overlay(NAMED, "filledByGateway", json!(7));
    let result = run(&env, &fake, &api).await.expect("verdict");
    assert!(result.ok, "{result:#?}");
    let rb = read_back_of(&result);
    assert_eq!(rb.status, CheckStatus::Warn);
    assert_eq!(rb.findings[0].code, "resource_differs");
    assert_eq!(rb.findings[0].status, Some(CheckStatus::Warn));
    let line = format!(
        "read_back: {NAMED} differs from its file at: filledByGateway (the live files equal the repo; the gateway's REST view differs)"
    );
    assert!(result.warnings.contains(&line), "{:?}", result.warnings);
}

#[tokio::test]
async fn rb_verify_standalone_changed_stays_red_through_action() {
    let env = env();
    drift_live_translator(&env);
    let server = wiremock::MockServer::start().await;
    mount_logs(&server, Vec::new()).await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);

    for named in [false, true] {
        let fake = seeded_fake(&env.declared, named).await;
        if named {
            fake.rest_overlay(NAMED, "filledByGateway", json!(7));
        }
        let result = run(&env, &fake, &api).await.expect("verdict");
        assert!(!result.ok, "{result:#?}");
        assert_eq!(result.slug, Some(VERIFY_FAILED_SLUG));
        assert_eq!(result.failure_exit_code(), Some(6));
        let rb = read_back_of(&result);
        assert_eq!(rb.status, CheckStatus::Fail);
        assert_eq!(rb.findings[0].subject, NAMED);
        assert_eq!(rb.findings[0].status, None);
        assert!(
            !result.warnings.iter().any(|w| w.starts_with("read_back: ")),
            "{:?}",
            result.warnings
        );
    }
}

#[tokio::test]
async fn rb_verify_backup_same_warns() {
    let env = env();
    let mut pair = node_pair(&env).await;
    pair.backup = seeded_fake(&env.declared, false).await;
    let result = run_node(&env, &pair, VerifyNode::Both).await;
    assert!(result.ok, "{result:#?}");
    assert_eq!(result.slug, None);
    let backup = result.backup.as_ref().expect("backup report");
    assert!(backup.converged, "{backup:#?}");
    let rb = backup
        .checks
        .iter()
        .find(|c| c.id == ignition_core::deploy::pair::BackupCheckId::ReadBack)
        .expect("backup read_back");
    assert_eq!(rb.status, CheckStatus::Warn);
    assert_eq!(rb.findings[0].code, "resource_missing");
    assert_eq!(rb.findings[0].subject, NAMED);
    assert_eq!(rb.findings[0].status, Some(CheckStatus::Warn));
    assert!(
        result.warnings.contains(&standalone_missing_line(NAMED)),
        "{:?}",
        result.warnings
    );
    // The master's read_back is untouched by the backup's difference.
    assert_eq!(read_back_of(&result).status, CheckStatus::Pass);
    drop(pair.servers);
}

#[tokio::test]
async fn rb_verify_backup_changed_stays_red() {
    // The same backup difference, with the master's live file drifted: the
    // plan says `changed`, so the backup's read_back stays red.
    let env = env();
    drift_live_translator(&env);
    let mut pair = node_pair(&env).await;
    pair.backup = seeded_fake(&env.declared, false).await;
    let result = run_node(&env, &pair, VerifyNode::Both).await;
    assert!(!result.ok, "{result:#?}");
    let backup = result.backup.as_ref().expect("backup report");
    assert!(!backup.converged, "{backup:#?}");
    let rb = backup
        .checks
        .iter()
        .find(|c| c.id == ignition_core::deploy::pair::BackupCheckId::ReadBack)
        .expect("backup read_back");
    assert_eq!(rb.status, CheckStatus::Fail);
    assert_eq!(rb.findings[0].status, None);
    drop(pair.servers);
}
