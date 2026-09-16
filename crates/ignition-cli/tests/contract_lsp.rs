//! 14-04 THE LSP contract wall (SC-3 handler half + SC-5 LSP half): a
//! Content-Length scripted client drives the REAL spawned `ign lsp`
//! binary end to end.
//!
//! The harness IS the byte-scan (research Pattern 5, LSP variant): the
//! reader thread implements the base protocol — read header lines to
//! the blank line, parse `Content-Length: N`, read EXACTLY N bytes —
//! and every stdout byte must land inside a valid framed payload. One
//! stray byte (a leaked `println!`, a panic, a partial write) breaks
//! either the header parse or the exact-count read and the test fails.
//! The harness runs under `IGNITION_LOG=trace` plus an unknown-key
//! config (the purity-harness noise recipe, contract_stdout_purity.rs),
//! with ambient `IGNITION_PROFILE` / `IGNITION_JSON` / `IGNITION_YES`
//! stripped for determinism.
//!
//! Every assertion is assert-based over PARSED values — never snapbox
//! goldens, so `SNAPSHOTS=overwrite` cannot sanitize a protocol
//! regression. `recv_timeout` on the reader channel means a protocol
//! death fails LOUDLY on timeout, never hangs the suite.
//!
//! The dead-gateway test is the PERMANENT cache-only proof (SC-3):
//! with a dead gateway configured, didOpen + completion + hover +
//! publish all answer within the budget — a handler that ever touches
//! the Session/runtime on the request path fails by timeout.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// How long a single response may take to arrive. Handlers are pure
/// snapshot reads; anything slower is either the dead-gateway
/// no-blocking-network property regressing (SC-3) or a wedged loop —
/// either way the budget converts it into a loud failure.
const RESPONSE_BUDGET_SECS: u64 = 15;

/// The cache's first publish wait: the refresher populates immediately
/// at startup, but the wire walk takes real time — the poller re-asks
/// until the snapshot goes healthy (bounded, never unbounded).
const FIRST_PUBLISH_WAIT_SECS: u64 = 60;

/// Isolated tempdir config (the purity-harness recipe): an unknown
/// top-level key fires `warn_unknown_keys` tracing on every config
/// load — maximum stderr noise for the byte-scan. `url` points the
/// single `dev` profile at the wiremock server (or a dead port).
fn write_config(config: &Path, url: &str) {
    std::fs::write(
        config,
        format!(
            "bogus_key = 1\nactive = \"dev\"\n\n[profiles.dev]\nurl = \
             \"{url}\"\nauth = {{ token_env = \"IGNITION_TOKEN\" }}\n"
        ),
    )
    .expect("write lsp contract config");
}

fn isolated_config_dir() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.toml").to_path_buf();
    (dir, path)
}

/// The scripted LSP client over the REAL binary. The reader thread is
/// THE BYTE-SCAN: every stdout byte must be a framed payload byte.
struct LspClient {
    child: Child,
    stdin: Option<ChildStdin>,
    messages: Receiver<Value>,
    _config_dir: tempfile::TempDir,
}

impl LspClient {
    /// Spawn `ign lsp` against a caller-written config file with the
    /// purity recipe. `token` rides IGNITION_TOKEN (the config's
    /// token_env); a dead-gateway spawn passes a dummy token — the
    /// session resolves offline and every section fails by design.
    fn spawn(config: &Path, config_dir: tempfile::TempDir, token: &str) -> Self {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_ign"));
        cmd.args(["lsp"])
            .env("IGNITION_CLI_CONFIG", config)
            .env("IGNITION_LOG", "trace")
            .env("IGNITION_TOKEN", token)
            .env_remove("IGNITION_PROFILE")
            .env_remove("IGNITION_JSON")
            .env_remove("IGNITION_YES")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // stderr carries the trace noise; the protocol is stdout.
            .stderr(Stdio::null());
        let mut child = cmd.spawn().expect("spawn ign lsp");
        let stdout: ChildStdout = child.stdout.take().expect("piped stdout");
        let stdin = child.stdin.take().expect("piped stdin");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || reader_thread(stdout, tx));
        Self {
            child,
            stdin: Some(stdin),
            messages: rx,
            _config_dir: config_dir,
        }
    }

    /// Write one message as exactly one Content-Length frame.
    fn send(&mut self, message: &Value) {
        let stdin = self
            .stdin
            .as_mut()
            .expect("server stdin is open (a closed stdin fails every later step)");
        let body = message.to_string();
        let frame = format!("Content-Length: {}\r\n\r\n{body}", body.len());
        stdin
            .write_all(frame.as_bytes())
            .and_then(|_| stdin.flush())
            .expect("write a framed protocol message");
    }

    fn request(&mut self, id: i64, method: &str, params: Value) {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }));
    }

    /// THE id-matched receive with a hard budget: waits for the next
    /// response whose id matches, failing LOUDLY on timeout (a wedged
    /// loop or a blocked-on-network handler must never hang silently).
    /// Notifications that arrive first are skipped (LSP lets them
    /// interleave freely; id matching is the client contract).
    fn expect_response(&self, id: i64, secs: u64) -> Value {
        let deadline = Instant::now() + Duration::from_secs(secs);
        loop {
            let now = Instant::now();
            if now >= deadline {
                panic!(
                    "no response for id {id} within {secs}s — protocol death, \
                        starvation, or a handler blocked on the network"
                );
            }
            match self.messages.recv_timeout(deadline - now) {
                Ok(msg) if msg.get("id").and_then(Value::as_i64) == Some(id) => return msg,
                Ok(_) => continue, // a notification (or stray id) — keep waiting
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!(
                    "no response for id {id} within {secs}s — protocol death, \
                     starvation, or a handler blocked on the network"
                ),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => panic!(
                    "the reader thread died — stdout closed mid-protocol or the \
                     byte-scan rejected a stray byte (see the panicked thread's \
                     message above)"
                ),
            }
        }
    }

    /// THE next publishDiagnostics notification with a hard budget.
    fn expect_publish(&self, secs: u64) -> Value {
        let deadline = Instant::now() + Duration::from_secs(secs);
        loop {
            let now = Instant::now();
            if now >= deadline {
                panic!(
                    "no publishDiagnostics within {secs}s — the loop's \
                        notification arm is wedged or dead"
                );
            }
            match self.messages.recv_timeout(deadline - now) {
                Ok(msg)
                    if msg.get("method").and_then(Value::as_str)
                        == Some("textDocument/publishDiagnostics") =>
                {
                    return msg;
                }
                Ok(_) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!(
                    "no publishDiagnostics within {secs}s — the loop's \
                     notification arm is wedged or dead"
                ),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("the reader thread died — see the panicked thread's message")
                }
            }
        }
    }

    /// The initialize handshake: request → response → `initialized`
    /// notification. Returns the response result (capabilities live
    /// there).
    fn initialize(&mut self) -> Value {
        self.request(
            1,
            "initialize",
            json!({
                "processId": null,
                "rootUri": null,
                "capabilities": {},
            }),
        );
        let response = self.expect_response(1, RESPONSE_BUDGET_SECS);
        self.notify("initialized", json!({}));
        response["result"].clone()
    }

    /// The shutdown handshake + the exit notification, then wait for
    /// the process to terminate. Returns the exit code (clean = 0).
    fn shutdown_and_wait(&mut self) -> i32 {
        self.request(999, "shutdown", Value::Null);
        let response = self.expect_response(999, RESPONSE_BUDGET_SECS);
        assert!(
            response.get("error").is_none(),
            "shutdown must not error: {response:?}"
        );
        self.notify("exit", Value::Null);
        let deadline = Instant::now() + Duration::from_secs(RESPONSE_BUDGET_SECS);
        while Instant::now() < deadline {
            if let Some(status) = self.child.try_wait().expect("poll child") {
                return status.code().unwrap_or(-1);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!(
            "the server did not exit within {RESPONSE_BUDGET_SECS}s of the \
                shutdown handshake"
        );
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        // Best-effort cleanup — tests that completed the handshake
        // already reaped the child; everything else gets killed so a
        // failure never wedges the suite.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// THE BYTE-SCAN: the base-protocol reader. Header lines to the blank
/// line, `Content-Length: N`, then EXACTLY N bytes — any stray stdout
/// byte fails either the header parse or the exact-count read.
fn reader_thread(stdout: ChildStdout, tx: std::sync::mpsc::Sender<Value>) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => return, // server exited / stdout closed
                Ok(_) => {}
            }
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                break; // blank line: headers done
            }
            if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
                match rest.trim().parse::<usize>() {
                    Ok(len) => content_length = Some(len),
                    Err(_) => panic!("unparseable Content-Length header {trimmed:?}"),
                }
            } else if trimmed.starts_with("Content-Type:") {
                // legal base-protocol header; ignore
            } else {
                panic!(
                    "stdout carried bytes outside Content-Length framing \
                     ({trimmed:?}) — the byte-scan reds on any stray byte"
                );
            }
        }
        let Some(len) = content_length else {
            panic!(
                "stdout carried a blank line with no Content-Length header — \
                 a stray byte killed the framing"
            );
        };
        let mut payload = vec![0u8; len];
        if reader.read_exact(&mut payload).is_err() {
            return; // stdout closed mid-payload
        }
        let parsed: Value = match serde_json::from_slice(&payload) {
            Ok(value) => value,
            Err(error) => panic!("a framed payload was not valid JSON: {error}"),
        };
        if tx.send(parsed).is_err() {
            return; // the test is done reading
        }
    }
}

/// Advisory-hint helper without a regex dependency: does the text
/// carry "…snapshot <digits>s old…"?
fn contains_age_stamp(text: &str) -> bool {
    let Some(idx) = text.find("snapshot ") else {
        return false;
    };
    let rest = &text[idx + "snapshot ".len()..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    !digits.is_empty() && rest[digits.len()..].starts_with("s old")
}

const DOC_URI: &str = "file:///contract/lsp-doc.txt";

fn did_open_params(text: &str, version: i64) -> Value {
    json!({
        "textDocument": {
            "uri": DOC_URI,
            "languageId": "python",
            "version": version,
            "text": text,
        }
    })
}

fn did_change_params(text: &str, version: i64) -> Value {
    json!({
        "textDocument": {"uri": DOC_URI, "version": version},
        "contentChanges": [{"text": text}],
    })
}

fn completion_request() -> (&'static str, Value) {
    (
        "textDocument/completion",
        json!({
            "textDocument": {"uri": DOC_URI},
            "position": {"line": 0, "character": 0},
        }),
    )
}

// ---------------------------------------------------------------------------
// Lifecycle: the narrow capabilities are CONTRACT (composition posture)
// ---------------------------------------------------------------------------

/// SC-5 pin at the handshake level: initialize answers the narrow
/// capability surface — completion (trigger chars ["/","."]), hover,
/// FULL sync — and NEVER claims definition/codeAction/workspace
/// symbols (ignition-lsp owns the statics; doubling them degrades
/// nvim's composition). Then shutdown → exit → clean process exit 0.
#[test]
fn lifecycle_initializes_with_narrow_capabilities_and_exits_cleanly() {
    let (dir, config) = isolated_config_dir();
    // A dead URL is fine here: the handshake and shutdown never touch
    // the gateway (and that is itself part of the contract).
    write_config(&config, "http://127.0.0.1:1");
    let mut client = LspClient::spawn(&config, dir, "mock:name-key");

    let result = client.initialize();
    let caps = &result["capabilities"];
    assert_eq!(
        caps["completionProvider"]["triggerCharacters"],
        json!(["/", "."]),
        "the declared trigger characters are contract: {caps:?}"
    );
    assert_eq!(
        caps["hoverProvider"],
        json!(true),
        "hover claimed: {caps:?}"
    );
    assert_eq!(
        caps["textDocumentSync"],
        json!(1),
        "FULL sync claimed: {caps:?}"
    );
    for never_claimed in [
        "definitionProvider",
        "codeActionProvider",
        "workspaceSymbolProvider",
    ] {
        let value = caps.get(never_claimed);
        assert!(
            value.is_none_or(Value::is_null),
            "{never_claimed} must never be claimed (composition posture): \
             got {value:?} in {caps:?}"
        );
    }

    assert_eq!(
        client.shutdown_and_wait(),
        0,
        "the shutdown handshake ends in a clean exit"
    );
}

// ---------------------------------------------------------------------------
// The data plane: a wiremock-fed cache, the three families, and the
// cached-diagnostics lifecycle
// ---------------------------------------------------------------------------

/// The export zip the named-query section reads: one real-shaped
/// member (`ignition/resources/named-query/MyQuery/resource.json` —
/// the `{collection}/resources/{type}/{name}` export convention).
fn export_zip() -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    writer
        .start_file("project.json", options)
        .expect("project.json starts");
    writer
        .write_all(br#"{"title":"T","enabled":true}"#)
        .expect("project.json writes");
    writer
        .start_file(
            "ignition/resources/named-query/MyQuery/resource.json",
            options,
        )
        .expect("named-query member starts");
    writer
        .write_all(br#"{"name":"MyQuery"}"#)
        .expect("named-query member writes");
    writer.finish().expect("zip finalizes").into_inner()
}

/// Everything the GatewayCache populate cycle touches (the 14-03
/// populate path): the native provider/project lists, the deployed
/// tags route's version precondition + browse action, and the project
/// export zip carrying the named query.
async fn mount_lsp_fixtures(server: &wiremock::MockServer) {
    use wiremock::matchers::{body_json, method, path};

    // Section 1: tag providers.
    wiremock::Mock::given(method("GET"))
        .and(path("/data/api/v1/resources/list/ignition/tag-provider"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "items": [{"name": "default", "enabled": true}],
            "metadata": {}
        })))
        .expect(1..)
        .mount(server)
        .await;

    // Section 3: projects.
    wiremock::Mock::given(method("GET"))
        .and(path("/data/api/v1/projects/list"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "items": [{"name": "proj"}],
            "metadata": {}
        })))
        .expect(1..)
        .mount(server)
        .await;

    // Section 2's precondition: the tags route version handshake
    // (ROUTE_BUNDLE_VERSION — a mismatch would fail the section).
    wiremock::Mock::given(method("POST"))
        .and(path("/system/webdev/ign-cli/cli/tags"))
        .and(body_json(json!({"action": "version"})))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "ok": true,
            "data": {"routeVersion": "1.3.0"}
        })))
        .expect(1..)
        .mount(server)
        .await;

    // Section 2's browse walk: the provider root (with one folder to
    // recurse) and the folder's leaf.
    wiremock::Mock::given(method("POST"))
        .and(path("/system/webdev/ign-cli/cli/tags"))
        .and(body_json(json!({"action": "browse", "path": "[default]"})))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "ok": true,
            "data": {"results": [
                {"fullPath": "[default]Motor", "name": "Motor",
                 "tagType": "AtomicTag", "hasChildren": false,
                 "dataType": "Int4"},
                {"fullPath": "[default]Motors", "name": "Motors",
                 "tagType": "Folder", "hasChildren": true}
            ]}
        })))
        .expect(1..)
        .mount(server)
        .await;
    wiremock::Mock::given(method("POST"))
        .and(path("/system/webdev/ign-cli/cli/tags"))
        .and(body_json(
            json!({"action": "browse", "path": "[default]Motors"}),
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "ok": true,
            "data": {"results": [
                {"fullPath": "[default]Motors/Speed", "name": "Speed",
                 "tagType": "AtomicTag", "hasChildren": false,
                 "dataType": "Int4"}
            ]}
        })))
        .expect(1..)
        .mount(server)
        .await;

    // Section 4: the project export zip (the named-query source).
    wiremock::Mock::given(method("GET"))
        .and(path("/data/api/v1/projects/export/proj"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_raw(export_zip(), "application/zip"),
        )
        .expect(1..)
        .mount(server)
        .await;
}

/// Re-send a completion until the snapshot goes healthy and the three
/// families flow — the "wait for first publish" poll the plan pins.
/// Returns the last (non-empty) result.
fn poll_completion_until_non_empty(client: &mut LspClient) -> Vec<Value> {
    let deadline = Instant::now() + Duration::from_secs(FIRST_PUBLISH_WAIT_SECS);
    let mut id = 100;
    loop {
        assert!(
            Instant::now() < deadline,
            "the cache never went healthy within {FIRST_PUBLISH_WAIT_SECS}s — \
             the wiremock fixtures or the populate path broke"
        );
        let (method, params) = completion_request();
        client.request(id, method, params);
        let response = client.expect_response(id, RESPONSE_BUDGET_SECS);
        let items: Vec<Value> =
            serde_json::from_value(response["result"].clone()).expect("completion is a list");
        if !items.is_empty() {
            return items;
        }
        id += 1;
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// THE data-plane contract: three-family completions, cached
/// diagnostics on didOpen/didChange, the TTL-stamped hover, and the
/// empty-publish clear — all against the REAL binary over a wiremock-
/// fed cache.
#[tokio::test]
async fn data_plane_completions_diagnostics_and_hover_from_a_wiremock_fed_cache() {
    let server = wiremock::MockServer::start().await;
    mount_lsp_fixtures(&server).await;
    let (dir, config) = isolated_config_dir();
    write_config(&config, &server.uri());
    let mut client = LspClient::spawn(&config, dir, "mock:name-key");

    client.initialize();

    // Open a doc BEFORE the first publish lands: an early publish (seed
    // frame, healthy:false) may arrive and MUST be an empty set — from
    // no truth flows no diagnostics.
    client.notify(
        "textDocument/didOpen",
        did_open_params("see [bogus] please", 1),
    );

    // (a) THE THREE FAMILIES: provider name, tag path, named-query path
    // — completions are sourced from the cached snapshot only.
    let items = poll_completion_until_non_empty(&mut client);
    let labels: Vec<&str> = items
        .iter()
        .filter_map(|item| item["label"].as_str())
        .collect();
    assert!(labels.contains(&"default"), "provider family: {labels:?}");
    assert!(
        labels.contains(&"[default]Motor") || labels.contains(&"[default]Motors/Speed"),
        "tag-path family: {labels:?}"
    );
    assert!(
        labels.contains(&"MyQuery"),
        "named-query family: {labels:?}"
    );

    // Drain the publishes queued during the poll window (version
    // arrivals republish the open doc; their verdicts were derived
    // before this next edit).
    let drain_deadline = Instant::now() + Duration::from_millis(800);
    while let Ok(msg) = client
        .messages
        .recv_timeout(drain_deadline.saturating_duration_since(Instant::now()))
    {
        let _ = msg;
    }

    // (b) didChange to a doc referencing an UNKNOWN provider → a
    // publishDiagnostics arrives carrying a WARNING (severity 2), from
    // the cache (zero network — the wiremock never sees a request for
    // verdicts).
    client.notify(
        "textDocument/didChange",
        did_change_params("x [bogus] y [default]Motors/Speed", 2),
    );
    let publish = client.expect_publish(RESPONSE_BUDGET_SECS);
    assert_eq!(
        publish["params"]["uri"].as_str(),
        Some(DOC_URI),
        "the publish names the edited doc: {publish:?}"
    );
    let diagnostics = publish["params"]["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert_eq!(
        diagnostics.len(),
        1,
        "exactly the unknown provider: {publish:?}"
    );
    assert_eq!(
        diagnostics[0]["severity"],
        json!(2),
        "unknown provider warns: {publish:?}"
    );
    assert!(
        diagnostics[0]["message"]
            .as_str()
            .is_some_and(|m| m.contains("[bogus] is not a known tag provider")),
        "message names the offender: {publish:?}"
    );
    assert!(
        diagnostics[0]["message"]
            .as_str()
            .is_some_and(contains_age_stamp),
        "the hint carries the age stamp: {publish:?}"
    );

    // (c) Hover on a known tag path → contents contain the path AND an
    // age stamp matching snapshot <n>s old (Pitfall 4).
    client.request(
        200,
        "textDocument/hover",
        json!({
            "textDocument": {"uri": DOC_URI},
            // char 20 sits mid "[default]Motors/Speed" in the current text.
            "position": {"line": 0, "character": 20},
        }),
    );
    let response = client.expect_response(200, RESPONSE_BUDGET_SECS);
    let contents = &response["result"]["contents"]["value"];
    assert!(
        contents
            .as_str()
            .is_some_and(|v| v.contains("[default]Motors/Speed")),
        "hover names the match: {response:?}"
    );
    assert!(
        contents.as_str().is_some_and(contains_age_stamp),
        "hover carries the TTL-stamped snapshot age: {response:?}"
    );

    // (d) didChange to a fully valid doc → an EMPTY publish (the fix
    // clears the advisory view).
    client.notify(
        "textDocument/didChange",
        did_change_params("[default]Motors/Speed", 3),
    );
    let publish = client.expect_publish(RESPONSE_BUDGET_SECS);
    assert_eq!(publish["params"]["uri"].as_str(), Some(DOC_URI));
    assert!(
        publish["params"]["diagnostics"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "a valid doc publishes empty: {publish:?}"
    );

    // Clean exit — the data plane runs through a full shutdown too.
    assert_eq!(client.shutdown_and_wait(), 0);
}

// ---------------------------------------------------------------------------
// The dead-gateway cache-only proof (SC-3, permanent)
// ---------------------------------------------------------------------------

/// THE permanent no-blocking-network property: with a DEAD gateway
/// configured, didOpen + completion + hover + publish ALL answer within
/// the budget. With a healthy:false empty snapshot the RESULTS are
/// empty (honest — completions [], hover null, diagnostics []) but the
/// REQUESTS complete. A regression that makes any handler touch the
/// Session/runtime on the request path fails this test by timeout.
#[test]
fn dead_gateway_requests_all_answer_within_budget_from_cache_only() {
    // A guaranteed-closed port: bind, read it, drop the listener.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("probe bind");
    let dead_port = listener.local_addr().expect("addr").port();
    drop(listener);

    let (dir, config) = isolated_config_dir();
    write_config(&config, &format!("http://127.0.0.1:{dead_port}"));
    let mut client = LspClient::spawn(&config, dir, "mock:name-key");

    // The handshake itself must never touch the gateway.
    client.initialize();

    // didOpen on the dead cache: the publish that arrives (whether from
    // the seed frame or the first failed cycle) must be EMPTY — no
    // truth, no verdicts, no spam — and arrive within the budget.
    client.notify(
        "textDocument/didOpen",
        did_open_params("see [bogus] and [default]Motor", 1),
    );
    let publish = client.expect_publish(RESPONSE_BUDGET_SECS);
    assert!(
        publish["params"]["diagnostics"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "an unhealthy frame publishes empty, never guessed verdicts: {publish:?}"
    );

    // completion answers empty within the budget — and the latency is
    // measured per REQUEST (send → answer), which is the actual SC-3
    // property; process startup is not a request and cannot block on
    // the network.
    let (method, params) = completion_request();
    let completion_started = Instant::now();
    client.request(300, method, params);
    let response = client.expect_response(300, RESPONSE_BUDGET_SECS);
    let completion_latency = completion_started.elapsed();
    assert_eq!(
        response["result"],
        json!([]),
        "an unhealthy snapshot completes empty, never an error: {response:?}"
    );

    // hover answers null within the budget.
    let hover_started = Instant::now();
    client.request(
        301,
        "textDocument/hover",
        json!({
            "textDocument": {"uri": DOC_URI},
            "position": {"line": 0, "character": 8},
        }),
    );
    let response = client.expect_response(301, RESPONSE_BUDGET_SECS);
    let hover_latency = hover_started.elapsed();
    assert_eq!(
        response["result"],
        json!(null),
        "an unhealthy snapshot hovers null: {response:?}"
    );

    // THE latency assertions: each REQUEST answered far inside its
    // budget — the budget exists to catch a handler that ever blocks
    // on the network (SC-3 regression = timeout), and the explicit
    // per-request pins keep a contended CI honest about WHICH hop
    // slowed.
    for (name, latency) in [("completion", completion_latency), ("hover", hover_latency)] {
        assert!(
            latency < Duration::from_secs(RESPONSE_BUDGET_SECS),
            "{name} answered in {latency:?} — a request blocked on the \
             network (SC-3 regression)"
        );
    }

    assert_eq!(
        client.shutdown_and_wait(),
        0,
        "the server survives a dead gateway through a clean shutdown"
    );
}
