//! Wiremock contract for the script action (07-03, SCRPT-01) — the
//! probe-then-exec sequence over the REAL [`ReqwestGatewayApi`]
//! (trait level; the binary-level goldens live in
//! `ignition-cli/tests/contract_script.rs`).
//!
//! THE crown pins:
//! - **the structural gate**: no persisted `webdev_secret` → the
//!   additive `script_exec_not_configured` (exit 6) whose hint names
//!   `ign webdev deploy --with-script-exec` verbatim, with ZERO
//!   HTTP requests (the refusal costs nothing);
//! - **the sequence**: version probe then exec POST, BOTH carrying
//!   `X-Ignition-CLI-Secret` with the profile's stored secret, the
//!   exec JSON body's `action`/`code` asserted EXACTLY
//!   (recorded-request proofs);
//! - **denials surface honestly**: a probe `secret_mismatch` body
//!   maps onto the existing `webdev_route_error` family (no exec
//!   POST); an exec error body with a `traceback` surfaces it in
//!   the message (the 05-08 pattern);
//! - **redaction canary**: the secret hex appears in NO rendered
//!   output path — serialized JSON AND Debug of the result.

use ignition_core::actions::script::{read_script_input, script_run};
use ignition_core::client::ReqwestGatewayApi;

/// The scriptExec route path inside any wiremock server.
const ROUTE_PATH: &str = "/system/webdev/ign-cli/cli/scriptExec";

/// A temp config whose `dev` profile carries the given secret
/// (None = the not-configured state).
fn config_with(secret: Option<&str>) -> (tempfile::TempDir, ignition_core::config::Config) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");
    let secret_line = secret
        .map(|secret| format!("webdev_secret = \"{secret}\"\n"))
        .unwrap_or_default();
    std::fs::write(
        &path,
        format!(
            "active = \"dev\"\n\n[profiles.dev]\nurl = \"http://localhost:9088/\"\n{secret_line}"
        ),
    )
    .expect("write config");
    let config = ignition_core::config::load(&path).expect("config loads");
    (dir, config)
}

/// THE structural gate: no stored secret → the additive slug, exit 6,
/// the deploy-flag hint — and ZERO HTTP requests (an empty server:
/// any request would have hit an unmatched mock).
#[tokio::test]
async fn missing_secret_refuses_with_zero_requests() {
    let server = wiremock::MockServer::start().await;
    let (_dir, config) = config_with(None);
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);

    let err = script_run(&api, &config, "dev", "ign-cli", "2+2")
        .await
        .expect_err("not-configured refuses");
    assert_eq!(err.code(), "script_exec_not_configured");
    assert_eq!(err.exit_code(), 6);
    assert!(
        err.hint()
            .unwrap()
            .contains("ign webdev deploy --with-script-exec"),
        "hint names the deploy flag verbatim"
    );
    assert!(
        server
            .received_requests()
            .await
            .expect("received requests")
            .is_empty(),
        "the refusal performs ZERO HTTP requests"
    );
}

/// THE success round: probe → exec, both carrying the secret header,
/// the exec body's action/code EXACTLY as dispatched (recorded
/// requests), and the answer mapped under {stdout, result,
/// elapsedMs} — ALL keys always.
#[tokio::test]
async fn success_round_probes_then_execs_with_the_secret() {
    let server = wiremock::MockServer::start().await;
    // The version probe: body exactly {"action":"version"}, the
    // secret header, the handshake answer.
    let probe = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(ROUTE_PATH))
        .and(wiremock::matchers::body_json(
            serde_json::json!({"action": "version"}),
        ))
        .and(wiremock::matchers::header(
            "x-ignition-cli-secret",
            "cafebabe1234",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"routeVersion": "1.0.0", "minCli": "1.0"},
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;
    // The exec: body exactly {"action":"exec","code":...}, the
    // secret header again, the route's exec answer.
    let exec = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(ROUTE_PATH))
        .and(wiremock::matchers::body_json(
            serde_json::json!({"action": "exec", "code": "print 'hello'\n2+2"}),
        ))
        .and(wiremock::matchers::header(
            "x-ignition-cli-secret",
            "cafebabe1234",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"stdout": "hello\n", "result": 4, "elapsedMs": 12},
            })),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let (_dir, config) = config_with(Some("cafebabe1234"));
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let result = script_run(&api, &config, "dev", "ign-cli", "print 'hello'\n2+2")
        .await
        .expect("success round");
    assert_eq!(result.stdout, "hello\n");
    assert_eq!(result.result, serde_json::json!(4));
    assert_eq!(result.elapsed_ms, 12);

    // Recorded-request proofs: each mock saw exactly its one call,
    // and the exec JSON body's action/code are EXACT (the matcher
    // already gated the shape; the recorded body re-proves it).
    let probe_requests = probe.received_requests().await;
    assert_eq!(probe_requests.len(), 1);
    let probe_body: serde_json::Value =
        serde_json::from_slice(&probe_requests[0].body).expect("probe body parses");
    assert_eq!(probe_body["action"], "version");
    let exec_requests = exec.received_requests().await;
    assert_eq!(exec_requests.len(), 1, "exec fired exactly once");
    let exec_body: serde_json::Value =
        serde_json::from_slice(&exec_requests[0].body).expect("exec body parses");
    assert_eq!(exec_body["action"], "exec");
    assert_eq!(exec_body["code"], "print 'hello'\n2+2");

    // Serialized agent shape: {stdout, result, elapsedMs}, ALL keys
    // (a serde_json Value map is key-SORTED — the declaration order
    // shows in render_success's direct struct serialization, which
    // the CLI goldens pin).
    let serialized = serde_json::to_value(&result).expect("serializes");
    assert_eq!(serialized["stdout"], "hello\n");
    assert_eq!(serialized["result"], 4);
    assert_eq!(serialized["elapsedMs"], 12);
    let mut keys: Vec<&str> = serialized
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["elapsedMs", "result", "stdout"]);

    // Redaction canary: the secret appears in NEITHER the serialized
    // JSON NOR the Debug render of the result (every output path).
    let debug = format!("{result:?}");
    assert!(
        !serialized.to_string().contains("cafebabe1234") && !debug.contains("cafebabe1234"),
        "redaction: the secret never rides an output path"
    );
}

/// A probe denial (`secret_mismatch` at HTTP 200) surfaces honestly
/// through the existing webdev error family — and the exec POST
/// NEVER fires (the exec mock would have failed the test on any
/// unexpected hit via expect(0)-equivalent scoping: it is simply not
/// mounted).
#[tokio::test]
async fn probe_secret_mismatch_surfaces_honestly_without_exec() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(ROUTE_PATH))
        .and(wiremock::matchers::body_json(
            serde_json::json!({"action": "version"}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": false,
                "error": {"code": "secret_mismatch", "message": "scriptExec secret mismatch"},
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let (_dir, config) = config_with(Some("stale-secret"));
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = script_run(&api, &config, "dev", "ign-cli", "2+2")
        .await
        .expect_err("mismatch refuses");
    assert_eq!(err.code(), "webdev_route_error");
    assert_eq!(err.exit_code(), 6);
    assert!(
        err.to_string().contains("secret_mismatch"),
        "the route's machine code rides verbatim: {err}"
    );
    assert!(
        err.hint().unwrap().contains("--rotate-secret"),
        "the existing family hint carries the redeploy/rotate advice"
    );
    // Exactly ONE request (the probe); the exec never fired.
    assert_eq!(
        server
            .received_requests()
            .await
            .expect("received requests")
            .len(),
        1
    );
}

/// An exec error body with a traceback surfaces it in the message
/// (the 05-08 pattern — a route-side Python exception is not a black
/// box).
#[tokio::test]
async fn exec_route_error_surfaces_the_traceback() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(ROUTE_PATH))
        .and(wiremock::matchers::body_json(
            serde_json::json!({"action": "version"}),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": {"routeVersion": "1.0.0", "minCli": "1.0"},
            })),
        )
        .mount(&server)
        .await;
    // The exec denial with a traceback: the scoped guard stays
    // BOUND so the mock stays mounted through the action (the
    // drop-unmounts gotcha, STATE 02-01).
    let exec = wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(ROUTE_PATH))
        .and(wiremock::matchers::body_json(
            serde_json::json!({"action": "exec", "code": "1/0"}),
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "ok": false,
                "error": {
                    "code": "route_error",
                    "message": "scriptExec route error",
                    "traceback": "Traceback (most recent call last):\n  File \"<string>\", line 1, in ?\nZeroDivisionError: division by zero",
                },
            }),
        ))
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let (_dir, config) = config_with(Some("cafebabe1234"));
    let api = ReqwestGatewayApi::for_tests(&server.uri(), None);
    let err = script_run(&api, &config, "dev", "ign-cli", "1/0")
        .await
        .expect_err("the route's denial surfaces");
    assert_eq!(err.code(), "webdev_route_error");
    assert_eq!(err.exit_code(), 6);
    assert!(
        err.to_string().contains("ZeroDivisionError"),
        "the traceback rides the message: {err}"
    );
    assert!(
        err.to_string().contains("route traceback:"),
        "the 05-08 append marker: {err}"
    );
    let _ = exec; // the guard outlives the action (scoped-mock pin)
}

/// The pure three-form reader at the contract level: both inputs →
/// InvalidInput (exit 2), the reason naming the exclusivity.
#[test]
fn read_input_rejects_both_forms() {
    let err = read_script_input(Some("2+2"), Some("snippet.py")).expect_err("both refuse");
    assert_eq!(err.code(), "invalid_input");
    assert_eq!(err.exit_code(), 2);
}
