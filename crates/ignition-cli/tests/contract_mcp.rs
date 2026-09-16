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

    /// Write one raw JSON value as exactly one newline-terminated
    /// stdin frame. (Compact serialization — embedded newlines would
    /// be client-side framing bugs.)
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

    fn required_of<'a>(tools: &'a [Value], name: &str) -> Vec<String> {
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
