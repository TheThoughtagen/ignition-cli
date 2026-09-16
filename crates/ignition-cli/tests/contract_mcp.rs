//! 14-02 THE MCP contract wall (SC-1 / SC-5-MCP): a scripted JSON-RPC
//! client drives the REAL spawned `ign mcp serve` binary end to end.
//!
//! The harness is the byte-scan (research Pattern 5): a reader thread
//! reads stdout line-by-line and MUST parse every line as exactly one
//! JSON-RPC message before forwarding it — a single stray stdout byte
//! (a leaked tracing line, a panic, a partial write) fails the parse
//! and the test. `IGNITION_LOG=trace` plus a noisy unknown-key config
//! keep the diagnostics at maximum so the scan is proven under worst-
//! case noise (the 08 purity-harness recipe: tempdir config + ambient
//! `IGNITION_PROFILE` / `IGNITION_JSON` / `IGNITION_YES` stripped for
//! determinism).
//!
//! Every assertion is assert-based over PARSED values — never snapbox
//! goldens, so `SNAPSHOTS=overwrite` cannot sanitize a protocol
//! regression. `recv_timeout` on the reader channel means a protocol
//! death (dead writer, wedged loop) fails LOUDLY on timeout, never
//! hangs the suite.
//!
//! Request ids ride BOTH shapes the JSON-RPC spec allows — numeric and
//! string — and every test matches responses by id.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// How long a single response may take to arrive. The protocol loop
/// answers initialize/ping/tools/list inline; anything slower is a
/// starvation bug, and the budget converts it into a loud failure.
const RESPONSE_BUDGET_SECS: u64 = 15;

/// The quiet window used to prove a notification produced NO response
/// bytes (JSON-RPC: notifications are never answered).
const QUIET_WINDOW_MS: u64 = 800;

/// Isolated tempdir config (the purity-harness recipe): an unknown
/// top-level key fires `warn_unknown_keys` tracing on every config
/// load — maximum noise for the byte-scan. The file need not exist for
/// protocol-only tests; tools/call tests overwrite it with a real
/// profile.
fn isolated_config() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "bogus_key = 1\n").expect("write noisy config");
    (dir, path)
}

/// One clap argument of a catalog leaf, mirror-walked from the real
/// clap tree (the round-trip smoke needs definition order + the
/// positional/flag/boolean distinctions the bridge uses).
struct LeafArg {
    key: String,
    positional: bool,
    boolean: bool,
}

/// A catalog-eligible clap leaf found by walking the real tree.
struct LeafInfo {
    path: String,
    args: Vec<LeafArg>,
}

/// The scripted MCP client over the REAL binary. The reader thread is
/// THE BYTE-SCAN: one stray stdout byte fails the test.
struct ScriptedClient {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: std::sync::mpsc::Receiver<Value>,
    _config_dir: tempfile::TempDir,
}

impl ScriptedClient {
    /// Spawn with the noisy isolated config and the ambient IGNITION_*
    /// knobs stripped (determinism); `extra_env` rides ON TOP (used to
    /// prove IGNITION_YES cannot bypass the confirm gate).
    fn spawn(extra_env: &[(&str, &str)]) -> Self {
        let (dir, config) = isolated_config();
        Self::spawn_with_config(&config, dir, extra_env)
    }

    /// Spawn against a caller-written config file (wiremock-backed
    /// profiles); the tempdir is kept alive for the client's lifetime.
    fn spawn_with_config(
        config: &Path,
        config_dir: tempfile::TempDir,
        extra_env: &[(&str, &str)],
    ) -> Self {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_ign"));
        cmd.args(["mcp", "serve"])
            .env("IGNITION_CLI_CONFIG", config)
            .env("IGNITION_LOG", "trace")
            .env_remove("IGNITION_PROFILE")
            .env_remove("IGNITION_JSON")
            .env_remove("IGNITION_YES")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // stderr carries the trace noise; the protocol is stdout.
            .stderr(Stdio::null());
        for (key, value) in extra_env {
            cmd.env(key, value);
        }
        let mut child = cmd.spawn().expect("spawn ign mcp serve");
        let stdout = child.stdout.take().expect("piped stdout");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break, // server exited / stdout closed
                    Ok(_) => {}
                }
                // THE BYTE-SCAN: one stray byte fails this parse. A
                // panic here disconnects the channel and the test
                // fails loudly on its next recv.
                let parsed: Value = match serde_json::from_str(line.trim_end()) {
                    Ok(value) => value,
                    Err(error) => {
                        panic!("stdout carried a non-JSON-RPC byte (line {line:?}): {error}")
                    }
                };
                if tx.send(parsed).is_err() {
                    break; // test is done reading
                }
            }
        });
        Self {
            stdin: child.stdin.take(),
            child,
            lines: rx,
            _config_dir: config_dir,
        }
    }

    /// Write one JSON value as exactly one newline-terminated stdin
    /// frame. (Compact serialization — embedded newlines would be
    /// client-side framing bugs.)
    fn send(&mut self, message: &Value) {
        let stdin = self
            .stdin
            .as_mut()
            .expect("server stdin is open (a closed stdin fails every later step)");
        stdin
            .write_all(message.to_string().as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
            .expect("write a protocol frame to the server");
    }

    /// Write one raw (possibly non-JSON) stdin frame — the
    /// malformed-input survival probe.
    fn send_raw_line(&mut self, raw: &str) {
        let stdin = self
            .stdin
            .as_mut()
            .expect("server stdin is open (a closed stdin fails every later step)");
        stdin
            .write_all(raw.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
            .expect("write a raw frame to the server");
    }

    /// THE next message with a hard budget — no id matching, no
    /// skipping: the strict-ordering assertions need the FIRST arrival
    /// after a send, unfiltered.
    fn recv_next(&self, secs: u64) -> Value {
        match self.lines.recv_timeout(Duration::from_secs(secs)) {
            Ok(message) => message,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                panic!("no stdout message arrived within {secs}s — protocol death or starvation")
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => panic!(
                "the reader thread died — stdout closed mid-protocol or the byte-scan \
                 rejected a stray byte (see the panicked thread's message above)"
            ),
        }
    }

    /// Send a JSON-RPC request with the given id shape (numeric OR
    /// string — the spec allows both and both are exercised).
    fn request(&mut self, id: Value, method: &str, params: Value) {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
    }

    /// THE id-matched receive with a hard budget: waits for the next
    /// response whose id matches, failing LOUDLY on timeout (a wedged
    /// protocol loop must never hang the suite silently).
    fn expect_response(&self, id: &Value, secs: u64) -> Value {
        let deadline = Instant::now() + Duration::from_secs(secs);
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .unwrap_or_default();
            match self.lines.recv_timeout(remaining) {
                Ok(message) => {
                    assert_eq!(
                        message.get("jsonrpc").and_then(Value::as_str),
                        Some("2.0"),
                        "every response carries jsonrpc 2.0: {message}"
                    );
                    if message.get("id") == Some(id) {
                        return message;
                    }
                    // A different id's response: legal out-of-order
                    // delivery (JSON-RPC id matching) — keep waiting.
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!(
                    "no response with id {id} arrived within {secs}s — protocol death or \
                     starvation (see stderr for the server's trace log by rerunning with \
                     stderr inherited)"
                ),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => panic!(
                    "the reader thread died — stdout closed mid-protocol or the byte-scan \
                     rejected a stray byte (see the panicked thread's message above)"
                ),
            }
        }
    }

    /// THE quiet window: NO stdout message may arrive within `ms`
    /// milliseconds (proves notifications get no response).
    fn expect_silence(&self, ms: u64) {
        match self.lines.recv_timeout(Duration::from_millis(ms)) {
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {} // quiet, as contracted
            Ok(message) => {
                panic!("expected a quiet window ({ms}ms) but the server answered: {message}")
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                panic!("reader thread died during the quiet window")
            }
        }
    }

    /// The lifecycle opening every protocol test: initialize (numeric
    /// id) + notifications/initialized. Returns the initialize result.
    fn handshake(&mut self) -> Value {
        self.request(
            json!(1),
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "contract-mcp", "version": "0" },
            }),
        );
        let response = self.expect_response(&json!(1), RESPONSE_BUDGET_SECS);
        let result = response.get("result").expect("initialize has a result");
        self.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        result.clone()
    }

    /// All tool entries from tools/list, after a handshake.
    fn tools(&mut self) -> Vec<Value> {
        self.handshake();
        self.request(json!(2), "tools/list", json!({}));
        let response = self.expect_response(&json!(2), RESPONSE_BUDGET_SECS);
        response
            .pointer("/result/tools")
            .and_then(Value::as_array)
            .expect("tools/list returns tools[]")
            .clone()
    }
}

impl Drop for ScriptedClient {
    fn drop(&mut self) {
        // Close stdin (EOF ends the serve loop), then force-kill any
        // lingering process (a hung in-flight call holds the writer
        // open past the test's patience) and reap the zombie.
        self.stdin.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ---------------------------------------------------------------------------
// Lifecycle contract (SC-1 / SC-5-MCP)
// ---------------------------------------------------------------------------

/// initialize echoes the pinned 2025-06-18 revision EXACTLY, names the
/// server `ign`, and advertises tool capabilities — with BOTH id
/// shapes the spec allows (numeric and string), responses matched by
/// id.
#[test]
fn initialize_echoes_the_pinned_revision_with_both_id_shapes() {
    let mut client = ScriptedClient::spawn(&[]);

    // Numeric id.
    client.request(
        json!(1),
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "contract-mcp", "version": "0" },
        }),
    );
    let response = client.expect_response(&json!(1), RESPONSE_BUDGET_SECS);
    let result = response.get("result").expect("initialize answers a result");
    assert_eq!(
        result.get("protocolVersion").and_then(Value::as_str),
        Some("2025-06-18"),
        "the supported revision is echoed exactly"
    );
    assert_eq!(
        result.pointer("/serverInfo/name").and_then(Value::as_str),
        Some("ign"),
        "serverInfo.name is ign"
    );
    assert!(
        result.get("capabilities").map(|c| c.get("tools")).is_some(),
        "capabilities.tools is advertised: {result}"
    );

    // String id (spec-legal; the ping spec example itself uses "123").
    client.request(
        json!("str-id-9"),
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "contract-mcp", "version": "0" },
        }),
    );
    let response = client.expect_response(&json!("str-id-9"), RESPONSE_BUDGET_SECS);
    assert_eq!(
        response.get("id"),
        Some(&json!("str-id-9")),
        "string ids ride back verbatim"
    );
    assert!(response.get("result").is_some());
}

/// An OLDER requested revision negotiates to the server's single
/// supported revision (2025-06-18 spec negotiation rule) — the server
/// never echoes an unsupported version.
#[test]
fn initialize_older_version_negotiates_to_the_server_revision() {
    let mut client = ScriptedClient::spawn(&[]);
    client.request(
        json!(3),
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "contract-mcp", "version": "0" },
        }),
    );
    let response = client.expect_response(&json!(3), RESPONSE_BUDGET_SECS);
    assert_eq!(
        response
            .pointer("/result/protocolVersion")
            .and_then(Value::as_str),
        Some("2025-06-18"),
        "an older request negotiates to the server's revision"
    );
}

/// `notifications/initialized` is a NOTIFICATION: no response bytes
/// may ever arrive (JSON-RPC notifications are unanswered) — proven by
/// a quiet window on the byte-scanned stream.
#[test]
fn initialized_notification_produces_no_response_bytes() {
    let mut client = ScriptedClient::spawn(&[]);
    client.request(
        json!(1),
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "contract-mcp", "version": "0" },
        }),
    );
    let _ = client.expect_response(&json!(1), RESPONSE_BUDGET_SECS);

    client.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    client.expect_silence(QUIET_WINDOW_MS);
}

/// ping answers `result: {}` PROMPTLY (spec: "MUST respond promptly
/// with an empty response") — inline, not queued behind dispatch.
#[test]
fn ping_answers_promptly_with_an_empty_result() {
    let mut client = ScriptedClient::spawn(&[]);
    client.handshake();
    let start = Instant::now();
    client.request(json!(7), "ping", json!({}));
    let response = client.expect_response(&json!(7), 2);
    assert_eq!(
        response.get("result"),
        Some(&json!({})),
        "ping's result is exactly the empty object"
    );
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "ping answered within the 2s promptness contract (took {:?})",
        start.elapsed()
    );
}

/// tools/list returns a NON-EMPTY catalog where every entry carries
/// name/description/inputSchema — and the catalog spot-pins cannot
/// drift: `project_delete` is guarded (confirm in required +
/// properties), `status` is read-only (no confirm), and the
/// planner-locked exclusions (mcp/lsp/tui/completions/edit) are
/// absent.
#[test]
fn tools_list_carries_the_catalog_with_the_pinned_shape() {
    let mut client = ScriptedClient::spawn(&[]);
    let tools = client.tools();

    assert!(!tools.is_empty(), "the catalog is non-empty");
    let mut names = Vec::new();
    for tool in &tools {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .expect("every tool has a string name")
            .to_string();
        assert!(
            tool.get("description")
                .and_then(Value::as_str)
                .is_some_and(|d| !d.is_empty()),
            "every tool carries a non-empty description ({name})"
        );
        let schema = tool.get("inputSchema").expect("every tool has inputSchema");
        assert_eq!(
            schema.get("type").and_then(Value::as_str),
            Some("object"),
            "every inputSchema is an object schema ({name})"
        );
        assert!(
            schema.get("properties").is_some(),
            "every inputSchema has properties ({name})"
        );
        names.push(name);
    }

    fn required_of(tools: &[Value], name: &str) -> Vec<String> {
        tools
            .iter()
            .find(|t| t.get("name").and_then(Value::as_str) == Some(name))
            .map(|t| {
                t.pointer("/inputSchema/required")
                    .and_then(Value::as_array)
                    .expect("required array")
                    .iter()
                    .map(|v| v.as_str().expect("string name").to_string())
                    .collect()
            })
            .unwrap_or_else(|| panic!("tool {name} present in the catalog"))
    }

    // SC-2's catalog half over the wire: the guarded verb advertises a
    // REQUIRED boolean confirm; the read verb carries none.
    let delete_required = required_of(&tools, "project_delete");
    assert!(
        delete_required.iter().any(|r| r == "confirm"),
        "project_delete requires confirm on the wire: {delete_required:?}"
    );
    let delete_props = tools
        .iter()
        .find(|t| t.get("name").and_then(Value::as_str) == Some("project_delete"))
        .and_then(|t| t.pointer("/inputSchema/properties/confirm"))
        .expect("project_delete carries a confirm property");
    assert_eq!(
        delete_props.get("type").and_then(Value::as_str),
        Some("boolean"),
        "confirm is a boolean property on the wire"
    );

    let status = tools
        .iter()
        .find(|t| t.get("name").and_then(Value::as_str) == Some("status"))
        .expect("status present");
    assert!(
        status.pointer("/inputSchema/properties/confirm").is_none(),
        "the read-only status verb carries no confirm property"
    );

    // The planner-locked exclusions are absent as tools.
    for absent in ["mcp", "lsp", "tui", "completions", "edit"] {
        assert!(
            !names.iter().any(|n| n == absent),
            "excluded verb {absent} must not appear in the catalog: {names:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// SC-1 round-trip smoke: catalog names cannot drift from parseable argv
// ---------------------------------------------------------------------------

/// An INDEPENDENT walk of the real clap tree (the mcp.rs builder's
/// leaf rule) that keeps each eligible leaf's path + args in
/// definition order — the data the round-trip smoke needs to rebuild
/// the bridge's argv construction.
fn clap_leaf_map() -> std::collections::BTreeMap<String, LeafInfo> {
    use clap::CommandFactory;
    use ignition_cli::cli::Cli;

    fn walk(
        cmd: &clap::Command,
        prefix: &str,
        out: &mut std::collections::BTreeMap<String, LeafInfo>,
    ) {
        for sub in cmd.get_subcommands() {
            if sub.get_name() == "help" || sub.is_hide_set() {
                continue;
            }
            let path = if prefix.is_empty() {
                sub.get_name().to_string()
            } else {
                format!("{prefix} {}", sub.get_name())
            };
            let excluded = path
                .split(' ')
                .any(|segment| ["completions", "mcp", "lsp", "tui", "edit"].contains(&segment));
            let has_real_children = sub.get_subcommands().any(|c| c.get_name() != "help");
            if (!has_real_children || !sub.is_subcommand_required_set()) && !excluded {
                let mut args = Vec::new();
                for arg in sub.get_arguments() {
                    if arg.is_hide_set() || arg.is_global_set() {
                        continue;
                    }
                    let id = arg.get_id().as_str().to_string();
                    if id == "help" || id == "version" {
                        continue;
                    }
                    let key = if arg.is_positional() {
                        id.clone()
                    } else {
                        arg.get_long().unwrap_or(id.as_str()).to_string()
                    };
                    args.push(LeafArg {
                        key,
                        positional: arg.is_positional(),
                        boolean: matches!(arg.get_action(), clap::ArgAction::SetTrue),
                    });
                }
                out.insert(
                    path.replace(' ', "_"),
                    LeafInfo {
                        path: path.clone(),
                        args,
                    },
                );
            }
            walk(sub, &path, out);
        }
    }

    let mut out = std::collections::BTreeMap::new();
    walk(&Cli::command(), "", &mut out);
    out
}

/// A sample token for one required schema property: the first enum
/// value when the property enumerates, otherwise a plain "x".
fn sample_token(property: &Value) -> String {
    property
        .get("enum")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| "x".to_string())
}

/// SC-1 round-trip smoke: for a deterministic sample of catalog tools
/// — EVERY wire-guarded verb (required contains `confirm`) plus five
/// read verbs — the bridge's argv construction (`["ign"] + leaf path +
/// one token per schema-required arg`, confirm → `--yes`) must PARSE
/// against the real clap tree. Catalog names cannot drift from
/// parseable argv.
#[test]
fn catalog_sample_round_trips_through_clap_parse() {
    let mut client = ScriptedClient::spawn(&[]);
    let tools = client.tools();
    let leaves = clap_leaf_map();

    // The deterministic sample: every guarded-on-the-wire verb plus a
    // fixed set of read verbs spanning the family shapes.
    let mut sample: Vec<String> = tools
        .iter()
        .filter(|tool| {
            tool.pointer("/inputSchema/required")
                .and_then(Value::as_array)
                .is_some_and(|required| required.iter().any(|v| v.as_str() == Some("confirm")))
        })
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    for read in ["version", "status", "modules", "metrics", "profile_list"] {
        sample.push(read.to_string());
    }
    assert!(
        sample.len() >= 23 + 5,
        "the sample must cover all wire-guarded verbs + 5 reads (got {})",
        sample.len()
    );

    for name in &sample {
        let entry = tools
            .iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some(name.as_str()))
            .unwrap_or_else(|| panic!("sampled tool {name} missing from tools/list"));
        let leaf = leaves
            .get(name)
            .unwrap_or_else(|| panic!("tool {name} has no clap leaf — catalog drift"));

        let required: Vec<String> = entry
            .pointer("/inputSchema/required")
            .and_then(Value::as_array)
            .expect("required array")
            .iter()
            .map(|v| v.as_str().expect("string").to_string())
            .collect();
        let properties = entry
            .pointer("/inputSchema/properties")
            .and_then(Value::as_object)
            .expect("properties object");

        // The bridge's argv shape: leaf-path segments in order, then
        // one token per REQUIRED arg (positionals raw, value flags as
        // --long=value with the wire enum's first value, booleans as
        // bare flags), then --yes for the synthetic confirm.
        let mut argv: Vec<String> = vec!["ign".to_string()];
        argv.extend(leaf.path.split(' ').map(str::to_string));
        for arg in &leaf.args {
            if !required.contains(&arg.key) {
                continue;
            }
            if arg.positional {
                let property = properties.get(&arg.key).unwrap_or(&Value::Null);
                argv.push(sample_token(property));
            } else if arg.boolean {
                argv.push(format!("--{}", arg.key));
            } else {
                let property = properties.get(&arg.key).unwrap_or(&Value::Null);
                argv.push(format!("--{}={}", arg.key, sample_token(property)));
            }
        }
        if required.iter().any(|r| r == "confirm") {
            argv.push("--yes".to_string());
        }

        use clap::Parser;
        let parsed = ignition_cli::cli::Cli::try_parse_from(&argv);
        assert!(
            parsed.is_ok(),
            "catalog tool {name} (leaf {:?}) does not round-trip to parseable argv \
             {argv:?}: {:?}",
            leaf.path,
            parsed.err().map(|e| e.to_string())
        );
    }
}

// ---------------------------------------------------------------------------
// tools/call contract (SC-2 + frozen-envelope + error mapping + SC-5 pin)
// ---------------------------------------------------------------------------

use assert_cmd::Command as AssertCommand;

/// The live-captured gateway-info fixture (contract_status.rs verbatim).
async fn mount_gateway_info(server: &wiremock::MockServer) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/gateway-info"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": "ign-mock",
                "redundancyRole": "Independent",
                "edition": "standard",
                "ignitionVersion": "8.3.6 (b2026042713)",
                "jvmVersion": "17.0.11",
                "license": {"mode": "Trial", "expirationDate": "2026-08-24T19:00:00Z"}
            })),
        )
        .expect(1..)
        .mount(server)
        .await;
}

/// The live-captured overview fixture (contract_status.rs verbatim —
/// every number pinned, so the status envelope is fixture-stable).
async fn mount_overview(server: &wiremock::MockServer) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/overview"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "version": "8.3.6 (b2026042713)",
            "redundancy": {"role": "Independent", "activityLevel": "ACTIVE", "projectState": "RUNNING"},
            "java": {"version": "17.0.11", "vendor": "Azul Systems, Inc.", "name": "OpenJDK 64-Bit Server VM"},
            "os": {"name": "Linux", "arch": "amd64", "version": "5.15.0"},
            "uptime": 338137,
            "memory": [338137088i64, 1073741824i64],
            "cpu": 0.0031,
            "disk": {"total": 62661259264i64, "used": 12272824320i64},
            "license": {"state": "trial", "trialRemaining": 7017}
        })))
        .expect(1..)
        .mount(server)
        .await;
}

/// The readiness probe fixture.
async fn mount_status_ping(server: &wiremock::MockServer) {
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/StatusPing"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"state": "RUNNING"})),
        )
        .expect(1..)
        .mount(server)
        .await;
}

/// All three endpoints the `status` verb touches — the envelope is
/// fixture-stable, which is what makes byte-equality with a direct run
/// possible.
async fn mount_status_fixtures(server: &wiremock::MockServer) {
    mount_gateway_info(server).await;
    mount_overview(server).await;
    mount_status_ping(server).await;
}

/// A one-profile config pointing at `url` (the contract_status recipe)
/// plus the byte-scan's unknown-key noise. Auth rides IGNITION_TOKEN.
fn write_wiremock_profile_config(config: &Path, url: &str) {
    std::fs::write(
        config,
        format!(
            "bogus_key = 1\nactive = \"dev\"\n\n[profiles.dev]\nurl = \
             \"{url}\"\nauth = {{ token_env = \"IGNITION_TOKEN\" }}\n"
        ),
    )
    .expect("write wiremock profile config");
}

const MOCK_TOKEN: &str = "mock:name-key";

/// THE frozen-envelope verbatim pin (SC-2's wire half): a tools/call
/// over the real binary against a wiremock-backed profile returns the
/// tool-result text that is BYTE-FOR-BYTE the envelope the same
/// invocation prints directly (`ign --json --compact status`) — the
/// protocol path may never reshape an envelope.
#[tokio::test]
async fn tools_call_returns_the_frozen_envelope_verbatim() {
    let server = wiremock::MockServer::start().await;
    mount_status_fixtures(&server).await;

    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("config.toml");
    write_wiremock_profile_config(&config, &server.uri());

    let mut client =
        ScriptedClient::spawn_with_config(&config, dir, &[("IGNITION_TOKEN", MOCK_TOKEN)]);
    client.handshake();
    client.request(
        json!(10),
        "tools/call",
        json!({ "name": "status", "arguments": {} }),
    );
    let response = client.expect_response(&json!(10), RESPONSE_BUDGET_SECS);
    let result = response.get("result").expect("tools/call answers a result");
    assert_eq!(
        result.get("isError"),
        Some(&json!(false)),
        "a successful envelope is isError:false: {result}"
    );
    let text = result
        .pointer("/content/0/text")
        .and_then(Value::as_str)
        .expect("the tool result carries text content");
    let envelope: Value = serde_json::from_str(text).expect("the envelope text parses");
    assert_eq!(envelope.get("ok").and_then(Value::as_bool), Some(true));
    assert_eq!(
        envelope.get("profile").and_then(Value::as_str),
        Some("dev"),
        "the envelope echoes the resolved profile"
    );

    // The SAME invocation, run directly: byte-for-byte equality of the
    // two envelope strings (the direct stdout's single trailing
    // println! newline stripped).
    let direct = AssertCommand::cargo_bin("ign")
        .expect("binary 'ign' not found")
        .args(["--json", "--compact", "status"])
        .env("IGNITION_CLI_CONFIG", &config)
        .env("IGNITION_TOKEN", MOCK_TOKEN)
        .env("IGNITION_LOG", "trace")
        .env_remove("IGNITION_PROFILE")
        .env_remove("IGNITION_JSON")
        .env_remove("IGNITION_YES")
        .output()
        .expect("spawn ign status directly");
    assert!(
        direct.status.success(),
        "direct run must succeed: {}",
        String::from_utf8_lossy(&direct.stderr)
    );
    let direct_stdout = String::from_utf8(direct.stdout).expect("utf-8 stdout");
    assert_eq!(
        text,
        direct_stdout.strip_suffix('\n').unwrap_or(&direct_stdout),
        "MCP tool result must equal the direct CLI envelope byte-for-byte"
    );
}

/// SC-2, refusal half, ENV-PROOF: `tools/call project_delete` with
/// confirm OMITTED returns the frozen `confirmation_required` failure
/// envelope AS the tool result (isError:true, content = the envelope
/// JSON) — and `IGNITION_YES=1` exported into the spawned server's
/// environment changes NOTHING (the apply_env_defaults bypass is
/// behavioral, pinned here over the real binary).
#[tokio::test]
async fn confirm_omitted_refuses_the_frozen_envelope_even_with_ignition_yes() {
    let mut client = ScriptedClient::spawn(&[("IGNITION_YES", "1")]);
    client.handshake();
    client.request(
        json!(11),
        "tools/call",
        json!({ "name": "project_delete", "arguments": { "name": "x" } }),
    );
    let response = client.expect_response(&json!(11), RESPONSE_BUDGET_SECS);
    let result = response.get("result").expect("tools/call answers a result");
    assert_eq!(
        result.get("isError"),
        Some(&json!(true)),
        "the refusal rides isError:true: {result}"
    );
    let text = result
        .pointer("/content/0/text")
        .and_then(Value::as_str)
        .expect("the refusal rides as text content");
    let envelope: Value =
        serde_json::from_str(text).expect("the refusal text parses as the frozen envelope");
    assert_eq!(envelope.get("ok").and_then(Value::as_bool), Some(false));
    assert_eq!(
        envelope.pointer("/error/code").and_then(Value::as_str),
        Some("confirmation_required"),
        "the frozen refusal envelope rides verbatim: {envelope}"
    );
    assert!(
        envelope.pointer("/error/message").is_some(),
        "the refusal carries the LOCKED error envelope shape"
    );
}

/// SC-2, execution half: `confirm:true` is the ONLY way `--yes` is
/// reachable — the guarded verb executes against the gateway (the
/// wiremock records the DELETE carrying the server's own confirm=true
/// query param) and returns the success envelope.
#[tokio::test]
async fn confirm_true_executes_the_guarded_verb_on_the_wire() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path("/data/api/v1/projects/x"))
        .and(wiremock::matchers::query_param("confirm", "true"))
        .respond_with(wiremock::ResponseTemplate::new(200))
        .expect(1..)
        .mount_as_scoped(&server)
        .await;

    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("config.toml");
    write_wiremock_profile_config(&config, &server.uri());

    let mut client =
        ScriptedClient::spawn_with_config(&config, dir, &[("IGNITION_TOKEN", MOCK_TOKEN)]);
    client.handshake();
    client.request(
        json!(12),
        "tools/call",
        json!({ "name": "project_delete", "arguments": { "name": "x", "confirm": true } }),
    );
    let response = client.expect_response(&json!(12), RESPONSE_BUDGET_SECS);
    let result = response.get("result").expect("tools/call answers a result");
    assert_eq!(
        result.get("isError"),
        Some(&json!(false)),
        "confirm:true executes: {result}"
    );
    let text = result
        .pointer("/content/0/text")
        .and_then(Value::as_str)
        .expect("text content");
    let envelope: Value = serde_json::from_str(text).expect("envelope parses");
    assert_eq!(envelope.get("ok").and_then(Value::as_bool), Some(true));
    assert_eq!(
        envelope.pointer("/data/deleted").and_then(Value::as_str),
        Some("x"),
        "the frozen success envelope rides verbatim: {envelope}"
    );

    // The wire proof: exactly one DELETE, carrying confirm=true.
    let requests = guard.received_requests().await;
    assert_eq!(requests.len(), 1, "one DELETE per invocation");
    let query = requests[0].url.query().expect("query present");
    assert!(
        query.contains("confirm=true"),
        "the server's own guard rode the wire: {query}"
    );
}

/// Error mapping + survival: unknown tool → -32601; invalid params
/// (missing required arg, hostile unknown key) → -32602; a malformed
/// (non-JSON) stdin line produces NO stdout bytes and the server keeps
/// serving — a subsequent ping answers promptly.
#[tokio::test]
async fn protocol_errors_map_correctly_and_survive_malformed_input() {
    let mut client = ScriptedClient::spawn(&[]);
    client.handshake();

    // Unknown tool → -32601.
    client.request(
        json!(20),
        "tools/call",
        json!({ "name": "no_such_tool", "arguments": {} }),
    );
    let response = client.expect_response(&json!(20), RESPONSE_BUDGET_SECS);
    assert_eq!(
        response.pointer("/error/code").and_then(Value::as_i64),
        Some(-32601),
        "unknown tool is a protocol-level method error: {response}"
    );
    assert!(
        response
            .pointer("/error/message")
            .and_then(Value::as_str)
            .is_some_and(|m| m.contains("no_such_tool")),
        "the error names the offending tool"
    );

    // Missing required argument → -32602 (clap parse failure is NEVER
    // e.exit() — the process must survive the protocol).
    client.request(
        json!(21),
        "tools/call",
        json!({ "name": "tags_read", "arguments": {} }),
    );
    let response = client.expect_response(&json!(21), RESPONSE_BUDGET_SECS);
    assert_eq!(
        response.pointer("/error/code").and_then(Value::as_i64),
        Some(-32602),
        "invalid params map to -32602: {response}"
    );

    // THE SC-2 wire probe: a hostile `yes` property (not in any tool
    // schema — globals are excluded) refuses -32602 and can never
    // reach --yes.
    client.request(
        json!(22),
        "tools/call",
        json!({ "name": "status", "arguments": { "yes": true } }),
    );
    let response = client.expect_response(&json!(22), RESPONSE_BUDGET_SECS);
    assert_eq!(
        response.pointer("/error/code").and_then(Value::as_i64),
        Some(-32602),
        "the hostile-yes probe refuses as invalid params: {response}"
    );
    assert!(
        response
            .pointer("/error/message")
            .and_then(Value::as_str)
            .is_some_and(|m| m.contains("yes")),
        "the refusal names the offending key"
    );

    // A malformed stdin line: NO recoverable id → the server stays
    // QUIET (warns on stderr only), keeps serving, and answers the
    // next ping promptly.
    client.send_raw_line("this is not json at all");
    client.expect_silence(QUIET_WINDOW_MS);
    let start = Instant::now();
    client.request(json!(23), "ping", json!({}));
    let response = client.expect_response(&json!(23), 2);
    assert_eq!(
        response.get("result"),
        Some(&json!({})),
        "the protocol survived the malformed input"
    );
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "post-malformed ping still prompt: {:?}",
        start.elapsed()
    );
}

/// SC-5, THE PERMANENT STARVATION PIN: with an in-flight tools/call
/// hung on a dead gateway (a bound-but-never-accepting listener — the
/// TCP connect SUCCEEDS into the kernel backlog, so the reqwest client
/// is stuck waiting for a response that never comes until its own
/// 30s timeout), the ping response must arrive FIRST. Starvation is
/// structurally impossible: tools/call runs off the reader loop and
/// ping answers inline.
#[tokio::test]
async fn ping_never_starves_behind_an_in_flight_dead_gateway_call() {
    // The dead gateway: bound, never accepted. Held alive to the end
    // of the test — dropping it would RST the connection and could
    // error the call early.
    let dead_gateway = std::net::TcpListener::bind("127.0.0.1:0").expect("bind dead gateway");
    let dead_url = format!(
        "http://127.0.0.1:{}/",
        dead_gateway.local_addr().unwrap().port()
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("config.toml");
    write_wiremock_profile_config(&config, &dead_url);

    let mut client =
        ScriptedClient::spawn_with_config(&config, dir, &[("IGNITION_TOKEN", MOCK_TOKEN)]);
    client.handshake();

    // The call first (its gateway check hangs ≥ the client's own
    // 30s timeout), then IMMEDIATELY the ping.
    client.request(
        json!(30),
        "tools/call",
        json!({ "name": "version", "arguments": {} }),
    );
    client.request(json!(31), "ping", json!({}));

    // ORDERING: the FIRST message after the sends must be the ping.
    let first = client.recv_next(10);
    assert_eq!(
        first.get("id"),
        Some(&json!(31)),
        "ping must answer BEFORE the hung tool result: {first}"
    );
    assert_eq!(first.get("result"), Some(&json!({})), "ping's empty result");

    // Then the tool result lands (after the client's own gateway
    // timeout) — completing the ordering fact and proving the server
    // neither wedged nor dropped the call.
    let second = client.recv_next(75);
    assert_eq!(
        second.get("id"),
        Some(&json!(30)),
        "the in-flight call's result still arrives: {second}"
    );
    assert!(
        second.get("result").is_some() || second.get("error").is_some(),
        "the dead-gateway call resolves into a well-formed response: {second}"
    );
}

// ---------------------------------------------------------------------------
// Python mcp-SDK conformance oracle (optional gate, green-skips without uv)
// ---------------------------------------------------------------------------

/// The oracle's optional gate (the 10-USER-SETUP live-gate green-skip
/// genre): a REAL mcp-SDK Python client drives the full lifecycle over
/// the spawned binary against a wiremock-backed profile. Run
/// explicitly — NEVER a Cargo dependency on Python:
///
/// ```text
/// cargo test -p ignition-cli --test contract_mcp mcp_oracle -- --ignored
/// ```
///
/// Without `uv` on PATH this gate green-skips (prints the reason and
/// passes); with uv it runs `uv run --with mcp` — the mcp SDK is
/// fetched test-time only.
#[tokio::test]
#[ignore = "conformance oracle: needs uv + the mcp SDK — run with `cargo test -p \
            ignition-cli --test contract_mcp mcp_oracle -- --ignored`"]
async fn mcp_oracle_gate_drives_a_real_mcp_sdk_client() {
    let probe = Command::new("uv").arg("--version").output();
    let probe = match probe {
        Ok(output) if output.status.success() => output,
        _ => {
            eprintln!("green-skip: uv not available — the mcp SDK oracle gate is skipped");
            return;
        }
    };
    let _ = probe;

    let server = wiremock::MockServer::start().await;
    mount_status_fixtures(&server).await;

    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("config.toml");
    write_wiremock_profile_config(&config, &server.uri());

    let oracle = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/mcp_oracle.py");
    let output = Command::new("uv")
        .args([
            "run",
            "--with",
            "mcp",
            "python",
            oracle.to_string_lossy().as_ref(),
            "--",
            env!("CARGO_BIN_EXE_ign"),
        ])
        .env("IGNITION_CLI_CONFIG", &config)
        .env("IGNITION_TOKEN", MOCK_TOKEN)
        .output()
        .expect("run the oracle under uv");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the mcp SDK oracle failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("ORACLE OK"),
        "the oracle did not report success: {stdout}"
    );
}
