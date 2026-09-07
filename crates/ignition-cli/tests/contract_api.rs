//! Binary-level contract tests for `ign api call` (09-03, EXT-01) —
//! the FULL passthrough contract pinned through the real binary over
//! a wiremock gateway:
//!
//! 1. THE verbatim pin: the envelope's `data.result.data` carries the
//!    gateway's JSON byte-verbatim (unusual key order + unknown
//!    fields preserved) — the RawValue passthrough decision as
//!    executable contract, and the README's documented contract
//!    exception.
//! 2. The exit partition table: 400+JSON → exit 2
//!    `gateway_client_error` (verbatim body in the message);
//!    401 → 5 `auth_rejected`; 404 → 6 `not_found`; 503 → 6
//!    `gateway_restarting`; 500 → 1 `internal`.
//! 3. The 4 KiB truncation cap with the explicit `... [truncated]`
//!    marker on oversized error bodies.
//! 4. The refusal matrix: auth-pattern headers (`Authorization`,
//!    case-variant `X-IGNITION-API-TOKEN`) refuse exit 2
//!    `invalid_input` with `profile: null` and ZERO gateway requests
//!    (the guard runs pre-resolve).
//! 5. Path validation: missing leading slash, absolute/foreign-host
//!    URLs, and an embedded `?` (which names `--query`) all refuse
//!    exit 2 pre-resolution.
//! 6. The ONE query mechanism: repeatable `--query k=v` pairs ride
//!    the request.
//! 7. GET body passthrough (curl parity): `--data` rides ANY method.
//! 8. A non-JSON 2xx body refuses exit 1 `internal` with the
//!    explanatory message.
//!
//! Isolation is the version_gateway_contract pattern:
//! `IGNITION_CLI_CONFIG` → tempfile whose one profile points at the
//! mock server. wiremock is an existing ignition-cli dev-dep — NO new
//! deps (plan constraint).

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;

fn isolated_config() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");
    (dir, path)
}

/// Write a one-profile config (`active = "dev"`) whose URL points at `url`.
fn write_profile_config(config: &Path, url: &str) {
    std::fs::write(
        config,
        format!("active = \"dev\"\n\n[profiles.dev]\nurl = \"{url}\"\n"),
    )
    .expect("write config");
}

/// Spawn `ign api call --json …` with an isolated config. Ambient
/// `IGNITION_*` knobs are stripped (the stdout-purity-harness rule):
/// a developer shell's IGNITION_URL/PROFILE/YES/JSON must never
/// redirect a contract test's request. `IGNITION_TOKEN` is SET (the
/// generic EnvStore rung of the locked secret chain): `api call`
/// resolves through `Session::resolve` like every authed command, so
/// a credential must exist — the mock never validates it, only its
/// presence matters.
fn ign_api_call(config: &Path, args: &[&str]) -> std::process::Output {
    let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
    command.env("IGNITION_CLI_CONFIG", config);
    command.env("IGNITION_TOKEN", "ign-contract-test:0123456789abcdef");
    for knob in [
        "IGNITION_URL",
        "IGNITION_PROFILE",
        "IGNITION_JSON",
        "IGNITION_YES",
    ] {
        command.env_remove(knob);
    }
    command
        .args(["api", "call", "--json"])
        .args(args)
        .output()
        .expect("spawn ign")
}

/// The error envelope is the JSON object starting at the first `{` on
/// stderr (tracing log lines share stderr by design — see
/// version_gateway_contract.rs).
fn stderr_envelope(out: &std::process::Output) -> Value {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let start = stderr.find('{').unwrap_or(0);
    serde_json::from_str(&stderr[start..])
        .unwrap_or_else(|err| panic!("stderr envelope parses (full stderr {stderr:?}): {err}"))
}

/// A mock that answers `status` with `body` on GET /data/api/v1/x.
async fn mount_status(server: &wiremock::MockServer, status: u16, body: &str) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/x"))
        .respond_with(wiremock::ResponseTemplate::new(status).set_body_string(body))
        .expect(1)
        .mount(server)
        .await;
}

/// THE verbatim pin (first contract test): unusual key order AND
/// unknown fields ride the envelope's `data.result.data` EXACTLY —
/// `zz_last` before `alpha_first`, `b` before `a` — with no new
/// top-level envelope fields. The RawValue passthrough decision is
/// now executable contract (README's documented exception).
#[tokio::test]
async fn data_is_gateway_verbatim_key_order_and_unknown_fields_preserved() {
    let server = wiremock::MockServer::start().await;
    // set_body_string (NOT set_body_json): the WIRE bytes must carry
    // the unusual order — a serde_json Value would re-sort keys and
    // neuter this pin.
    mount_status(
        &server,
        200,
        r#"{"zz_last": 1, "alpha_first": {"b": 2, "a": 1}}"#,
    )
    .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign_api_call(&config, &["--method", "GET", "--path", "/data/api/v1/x"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let body: Value = serde_json::from_str(&stdout).expect("success envelope parses");
    let zz = stdout
        .find("\"zz_last\"")
        .expect("zz_last key present verbatim");
    let alpha = stdout
        .find("\"alpha_first\"")
        .expect("alpha_first key present verbatim");
    let b = stdout.find("\"b\"").expect("b key present verbatim");
    let a = stdout.find("\"a\"").expect("a key present verbatim");
    assert!(zz < alpha, "original key order preserved: {stdout}");
    assert!(b < a, "nested key order preserved: {stdout}");

    // The outcome shape: method/path echo + status, data inline.
    assert_eq!(body["ok"], Value::Bool(true));
    assert_eq!(body["profile"], Value::String("dev".into()));
    assert_eq!(body["data"]["method"], Value::String("GET".into()));
    assert_eq!(body["data"]["path"], Value::String("/data/api/v1/x".into()));
    assert_eq!(body["data"]["result"]["status"], Value::Number(200.into()));
    assert_eq!(
        body["data"]["result"]["data"]["zz_last"],
        Value::Number(1.into()),
        "unknown fields survive untouched"
    );

    // The LOCKED envelope never grows top-level fields.
    let mut keys: Vec<&str> = body
        .as_object()
        .expect("envelope is an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["data", "ok", "profile"]);
}

/// The exit partition table (research OQ7) — each row: (status, exit
/// code, slug). The 400 row additionally pins the verbatim body in
/// the message; every error row keeps stdout empty.
#[tokio::test]
async fn exit_partition_table() {
    let cases: [(u16, i32, &str, bool); 5] = [
        (400, 2, "gateway_client_error", true),
        (401, 5, "auth_rejected", false),
        (404, 6, "not_found", false),
        (503, 6, "gateway_restarting", false),
        (500, 1, "internal", false),
    ];
    for (status, exit, slug, body_verbatim) in cases {
        let server = wiremock::MockServer::start().await;
        let marker = format!("case-{status}-gateway-body");
        let body = format!(r#"{{"detail":"{marker}"}}"#);
        mount_status(&server, status, &body).await;

        let (_dir, config) = isolated_config();
        write_profile_config(&config, &server.uri());
        let out = ign_api_call(&config, &["--method", "GET", "--path", "/data/api/v1/x"]);
        assert_eq!(
            out.status.code(),
            Some(exit),
            "status {status} must exit {exit} ({slug}); stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty(), "errors never touch stdout");

        let envelope = stderr_envelope(&out);
        assert_eq!(envelope["ok"], Value::Bool(false), "status {status}");
        assert_eq!(
            envelope["error"]["code"],
            Value::String(slug.to_string()),
            "status {status}"
        );
        let message = envelope["error"]["message"].as_str().expect("message");
        if body_verbatim {
            assert!(
                message.contains(&marker),
                "the 4xx body rides the message verbatim: {message}"
            );
        }
    }
}

/// Pitfall 6's cap: a >4 KiB error body truncates at 4 KiB with the
/// explicit `... [truncated]` marker in the envelope message.
#[tokio::test]
async fn oversized_error_body_truncates_at_cap_with_marker() {
    let server = wiremock::MockServer::start().await;
    let big = "A".repeat(5000);
    mount_status(&server, 400, &format!(r#"{{"pad":"{big}"}}"#)).await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign_api_call(&config, &["--method", "GET", "--path", "/data/api/v1/x"]);
    assert_eq!(out.status.code(), Some(2), "gateway_client_error class");

    let envelope = stderr_envelope(&out);
    assert_eq!(
        envelope["error"]["code"],
        Value::String("gateway_client_error".into())
    );
    let message = envelope["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("... [truncated]"),
        "the cap carries an explicit marker: {message}"
    );
    assert!(
        message.len() < 5000 + 200,
        "the message is capped near the 4 KiB body cap, not the full body: {}",
        message.len()
    );
}

/// THE refusal matrix (binary): auth-pattern headers refuse exit 2
/// `invalid_input` with `profile: null` — and the mock server
/// received ZERO requests (the guard runs pre-resolve; nothing is
/// ever constructed or sent).
#[tokio::test]
async fn auth_pattern_headers_refused_with_zero_requests() {
    for header in ["Authorization: Bearer x", "X-IGNITION-API-TOKEN: zzz"] {
        let server = wiremock::MockServer::start().await; // bare — no mounts
        let (_dir, config) = isolated_config();
        write_profile_config(&config, &server.uri());
        let out = ign_api_call(
            &config,
            &[
                "--method",
                "GET",
                "--path",
                "/data/api/v1/x",
                "--header",
                header,
            ],
        );
        assert_eq!(
            out.status.code(),
            Some(2),
            "{header} must refuse exit 2; stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty(), "errors never touch stdout");

        let envelope = stderr_envelope(&out);
        assert_eq!(
            envelope["error"]["code"],
            Value::String("invalid_input".into())
        );
        assert_eq!(
            envelope["profile"],
            Value::Null,
            "the refusal fired pre-resolve — the envelope profile stays null"
        );
        let received = server.received_requests().await.unwrap_or_default();
        assert!(
            received.is_empty(),
            "ZERO requests may hit the gateway on a refusal ({header})"
        );
    }
}

/// Path validation (binary): missing leading slash, an absolute
/// foreign-host URL, and an embedded `?` all refuse exit 2
/// pre-resolution — the `?` case names `--query` (the ONE query
/// mechanism).
#[tokio::test]
async fn path_validation_refuses_bad_shapes_with_specific_reasons() {
    let server = wiremock::MockServer::start().await;
    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());

    let cases = [
        ("data/api/v1/x", None),
        ("http://other-host/x", None),
        ("/data/api/v1/x?embed=1", Some("--query")),
    ];
    for (path, expected_hint) in cases {
        let out = ign_api_call(&config, &["--method", "GET", "--path", path]);
        assert_eq!(
            out.status.code(),
            Some(2),
            "{path} must refuse exit 2; stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let envelope = stderr_envelope(&out);
        assert_eq!(
            envelope["error"]["code"],
            Value::String("invalid_input".into()),
            "{path}"
        );
        assert_eq!(envelope["profile"], Value::Null, "{path}: pre-resolve");
        if let Some(expected_hint) = expected_hint {
            let message = envelope["error"]["message"].as_str().expect("message");
            assert!(
                message.contains(expected_hint),
                "the ? refusal names the ONE query mechanism: {message}"
            );
        }
    }
    // Zero requests: every case refused pre-resolution.
    let received = server.received_requests().await.unwrap_or_default();
    assert!(received.is_empty(), "no case may hit the gateway");
}

/// The ONE query mechanism: repeatable `--query k=v` — both pairs
/// arrive on the request (the mock only answers when BOTH match).
#[tokio::test]
async fn query_pairs_ride_the_request() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/x"))
        .and(wiremock::matchers::query_param("a", "1"))
        .and(wiremock::matchers::query_param("b", "2"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign_api_call(
        &config,
        &[
            "--method",
            "GET",
            "--path",
            "/data/api/v1/x",
            "--query",
            "a=1",
            "--query",
            "b=2",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let requests = server.received_requests().await.expect("requests recorded");
    assert_eq!(requests.len(), 1);
    let url = requests[0].url.clone();
    assert!(url.query().unwrap_or_default().contains("a=1"), "{url}");
    assert!(url.query().unwrap_or_default().contains("b=2"), "{url}");
}

/// GET body passthrough (curl parity — decision 6): `--data` rides
/// ANY method, byte-verbatim.
#[tokio::test]
async fn get_method_carries_the_body() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/x"))
        .and(wiremock::matchers::body_bytes(br#"{"x":1}"#.to_vec()))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&server)
        .await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign_api_call(
        &config,
        &[
            "--method",
            "GET",
            "--path",
            "/data/api/v1/x",
            "--data",
            r#"{"x":1}"#,
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let requests = server.received_requests().await.expect("requests recorded");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        String::from_utf8_lossy(&requests[0].body),
        r#"{"x":1}"#,
        "the GET carries the raw body verbatim"
    );
}

/// A non-JSON 2xx body refuses exit 1 `internal` with the
/// explanatory message (the README's documented honesty refusal —
/// binary endpoints belong to the download pipelines).
#[tokio::test]
async fn non_json_2xx_refuses_internal_with_explanation() {
    let server = wiremock::MockServer::start().await;
    mount_status(&server, 200, "this is not json").await;

    let (_dir, config) = isolated_config();
    write_profile_config(&config, &server.uri());
    let out = ign_api_call(&config, &["--method", "GET", "--path", "/data/api/v1/x"]);
    assert_eq!(out.status.code(), Some(1), "internal class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");

    let envelope = stderr_envelope(&out);
    assert_eq!(envelope["error"]["code"], Value::String("internal".into()));
    let message = envelope["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("non-JSON"),
        "the message explains the contract: {message}"
    );
}
