//! `ign mcp serve` — the Model Context Protocol over newline-delimited
//! JSON-RPC 2.0 on stdio (Phase 14, 14-01).
//!
//! The protocol stream IS the stdout product (the `mcp` routes.rs row
//! is OutOfBand — completions/api-call genre): this module speaks JSON-RPC
//! 2.0 to an MCP client and never touches the envelope-render chassis.
//! Contracts this module owns:
//!
//! - **SC-1 (catalog)**: the tool catalog is DERIVED from
//!   `Cli::command()` at runtime — every non-hidden clap leaf except
//!   the exclusion set below, with clap-derived names/descriptions/
//!   inputSchemas. Nothing here is hand-written; the parity test in
//!   this file proves the derivation bidirectionally against a fresh
//!   independent walk, so catalog drift from the CLI surface is
//!   structurally impossible.
//! - **SC-2 (confirm gate)**: every leaf whose clap path appears in
//!   [`crate::GUARDED_OPS`] carries a synthetic `confirm: boolean`
//!   schema property that is deliberately ABSENT from `required`
//!   (schema-required would invite agents to auto-fill `confirm:true`);
//!   the dispatch bridge sets `--yes` ONLY from that field.
//!   `IGNITION_YES` (merged by `apply_env_defaults` for the human CLI)
//!   can never reach the protocol path — the bridge bypasses
//!   `apply_env_defaults` for the inner parse by construction. A
//!   confirmation_required refusal on a guarded verb rides as ONE text
//!   block whose envelope shape is byte-stable but whose message/hint
//!   PROSE is MCP-native (names `confirm: true`, marks `--yes`/
//!   `IGNITION_YES` CLI-only) — the second dated envelope-transport
//!   exception, alongside 09-03's api-call one (14-06 checkpoint round
//!   2). Every other envelope stays single-block verbatim.
//! - **Purity**: compact serialization only (`serde_json::to_string`)
//!   — one message per line, never pretty-printed (embedded newlines
//!   are a spec violation and a protocol death). Diagnostics ride the
//!   stderr-only tracing init in `main` — stdout carries protocol
//!   messages and nothing else.

use std::collections::HashSet;
use std::process::ExitCode;
use std::sync::Arc;

use clap::{CommandFactory, Parser};
use serde_json::{Value, json};
use tokio::io::AsyncBufReadExt;

use ignition_cli::cli::{Cli, McpArgs};
use ignition_core::error::CoreError;

use crate::GUARDED_OPS;
use crate::render::RenderMode;

/// The single served MCP protocol revision (2025-06-18 spec,
/// planner-locked pin; OQ5). Negotiation per spec: a requested version
/// EQUAL to this is echoed; anything else (older, newer, absent)
/// answers with the server's latest — this one revision.
pub(crate) const PROTOCOL_VERSION: &str = "2025-06-18";

/// Leaf paths EXCLUDED from the tool catalog (planner-locked set):
/// self-referential or non-agentic verbs whose product is not a tool
/// call — completions writes shell script, `mcp`/`lsp` ARE protocol
/// servers, `tui` owns the terminal, and `edit` hands the terminal to
/// a child $EDITOR (meaningless as a tool call). Matched against ANY
/// path segment so the set works at every depth.
pub(crate) const EXCLUDED_LEAVES: &[&str] = &["completions", "mcp", "lsp", "tui", "edit"];

/// One executable argument of a tool leaf, pre-resolved from clap
/// reflection so the dispatch bridge never touches the clap tree at
/// call time (owned data, `Arc`-shared into spawned tasks).
#[derive(Debug, Clone)]
pub(crate) struct ArgSpec {
    /// Token key: the positional's id (e.g. `name`) or the flag's long
    /// name with dashes preserved (e.g. `collision-policy`).
    pub key: String,
    /// Positional → raw tokens in definition order; flag → `--long=…`.
    pub positional: bool,
    /// `ArgAction::SetTrue` → boolean property, emitted as `--long`.
    pub boolean: bool,
}

/// One derived tool: the wire entry (name/description/schema) plus the
/// execution table the bridge needs. Both halves come from the SAME
/// clap walk — they cannot disagree.
#[derive(Debug, Clone)]
pub(crate) struct CatalogEntry {
    /// Tool name: leaf path with spaces → `_` (e.g. `tags_browse`).
    pub name: String,
    /// The clap leaf path (e.g. `tags browse`).
    pub path: String,
    /// The clap doc comment (`get_about`) — already written for agents.
    pub description: String,
    /// JSON Schema object derived from the leaf's clap args.
    pub schema: Value,
    pub args: Vec<ArgSpec>,
}

/// The full tool catalog. `Arc`-shared into spawned `tools/call`
/// tasks (clap `Command` borrows cannot cross `'static`).
pub(crate) type Catalog = Arc<Vec<CatalogEntry>>;

/// Tool-name encoding: leaf path with spaces replaced by underscores
/// (JSON-schema-safe, stable — the ONLY transformation between the
/// clap path and the wire name).
pub(crate) fn tool_name_for_path(path: &str) -> String {
    path.replace(' ', "_")
}

pub(crate) fn path_is_excluded(path: &str) -> bool {
    path.split(' ')
        .any(|segment| EXCLUDED_LEAVES.contains(&segment))
}

/// THE catalog builder: walk `Cli::command()` (the tui_coverage.rs
/// recursion, extended with the hidden/exclusion filters) and derive
/// every tool. The parity test runs an INDEPENDENT walk of the same
/// tree and pins bidirectional set equality.
pub(crate) fn build_catalog() -> Catalog {
    let mut entries = Vec::new();
    walk_catalog(&Cli::command(), "", &mut entries);
    Arc::new(entries)
}

/// The catalog walk: skip clap's auto `help` everywhere, skip hidden
/// commands (and their whole subtrees), and apply the leaf rule the
/// route registry uses — a node REQUIRES representation when it has no
/// real children OR its bare form is invocable (subcommand optional).
/// Row-requiring leaves become tools; required-subcommand groups
/// recurse only.
fn walk_catalog(cmd: &clap::Command, prefix: &str, out: &mut Vec<CatalogEntry>) {
    for sub in cmd.get_subcommands() {
        if sub.get_name() == "help" || sub.is_hide_set() {
            continue;
        }
        let path = if prefix.is_empty() {
            sub.get_name().to_string()
        } else {
            format!("{prefix} {}", sub.get_name())
        };
        let has_real_children = sub.get_subcommands().any(|c| c.get_name() != "help");
        if (!has_real_children || !sub.is_subcommand_required_set()) && !path_is_excluded(&path) {
            out.push(leaf_entry(&path, sub));
        }
        walk_catalog(sub, &path, out);
    }
}

/// Derive one leaf's tool entry. Schema rules (all clap-derived):
/// positionals → string property named by id (repeatable ones → arrays
/// of string); long flags → properties keyed by the long name with
/// dashes preserved; `ArgAction::SetTrue` → boolean;
/// `get_possible_values()` → enum; `is_required_set()` → required.
/// GLOBAL args (profile/json/compact/yes/verbose) are EXCLUDED — their
/// effect in the protocol path is either forced by the bridge
/// (json/compact), overridden by the ambient profile flag (profile),
/// confirm-gated (`yes` — replaced by the synthetic `confirm`
/// property), or stderr-only noise (verbose). A leaf present in
/// GUARDED_OPS gains the synthetic `confirm` boolean — the
/// catalog reads ONLY that const (single source, SC-2). `confirm` is
/// deliberately NOT in `required` (14-06 checkpoint round 2): a
/// schema-required confirm invites agents to auto-fill `confirm:true`
/// and defeat the gate — omission is legal and answered by the
/// refusal envelope.
fn leaf_entry(path: &str, cmd: &clap::Command) -> CatalogEntry {
    let guarded = GUARDED_OPS
        .iter()
        .any(|(guarded_path, _)| *guarded_path == path);
    let mut properties = serde_json::Map::new();
    let mut required: Vec<Value> = Vec::new();
    let mut args = Vec::new();

    for arg in cmd.get_arguments() {
        // Hidden (auto help/version) and GLOBAL args never become
        // per-call properties (see the fn doc for the globals rule).
        if arg.is_hide_set() || arg.is_global_set() {
            continue;
        }
        let id = arg.get_id().as_str().to_string();
        if id == "help" || id == "version" {
            continue;
        }
        let positional = arg.is_positional();
        let key = if positional {
            id.clone()
        } else {
            arg.get_long().unwrap_or(id.as_str()).to_string()
        };
        let action = arg.get_action();
        let boolean = matches!(action, clap::ArgAction::SetTrue);
        let repeatable = matches!(action, clap::ArgAction::Append);
        args.push(ArgSpec {
            key: key.clone(),
            positional,
            boolean,
        });

        let mut prop = serde_json::Map::new();
        let help = arg
            .get_long_help()
            .or_else(|| arg.get_help())
            .map(|styled| styled.to_string())
            .unwrap_or_default();
        if !help.is_empty() {
            prop.insert("description".into(), Value::String(help));
        }
        if boolean {
            prop.insert("type".into(), Value::String("boolean".into()));
        } else {
            let enum_values: Vec<Value> = arg
                .get_possible_values()
                .iter()
                .filter(|pv| !pv.is_hide_set())
                .map(|pv| Value::String(pv.get_name().to_string()))
                .collect();
            if repeatable {
                let mut item = serde_json::Map::new();
                item.insert("type".into(), Value::String("string".into()));
                if !enum_values.is_empty() {
                    item.insert("enum".into(), Value::Array(enum_values));
                }
                prop.insert("type".into(), Value::String("array".into()));
                prop.insert("items".into(), Value::Object(item));
            } else {
                prop.insert("type".into(), Value::String("string".into()));
                if !enum_values.is_empty() {
                    prop.insert("enum".into(), Value::Array(enum_values));
                }
            }
        }
        if arg.is_required_set() {
            required.push(Value::String(key.clone()));
        }
        properties.insert(key, Value::Object(prop));
    }

    if guarded {
        properties.insert(
            "confirm".into(),
            json!({
                "type": "boolean",
                "description": "Set true to execute this destructive operation; omit or \
                                false to receive the refusal."
            }),
        );
        // FIX B (14-06 checkpoint round 2): `confirm` stays OUT of
        // `required`. Schema-required advertises "you must confirm to
        // call this", which agents satisfy reflexively with
        // confirm:true — the optional form plus the refusal envelope
        // is the SC-2 behavior.
    }

    let mut schema = serde_json::Map::new();
    schema.insert("type".into(), Value::String("object".into()));
    schema.insert("properties".into(), Value::Object(properties));
    schema.insert("required".into(), Value::Array(required));

    CatalogEntry {
        name: tool_name_for_path(path),
        path: path.to_string(),
        description: cmd
            .get_about()
            .map(|styled| styled.to_string())
            .unwrap_or_default(),
        schema: Value::Object(schema),
        args,
    }
}

/// THE serve loop (`ign mcp serve`): newline-delimited JSON-RPC 2.0 on
/// stdio (research Pattern 2). Concurrency shape — ONE reader (this
/// task), ONE writer (the spawned task below is the sole stdout
/// owner), and every `tools/call` spawned OFF the reader path with its
/// response routed through an unbounded channel. Out-of-order
/// responses are legal (JSON-RPC id matching) and are what make ping
/// non-starving true by construction.
pub(crate) async fn serve(args: McpArgs, profile_flag: Option<&str>) -> ExitCode {
    // clap's value_parser=["serve"] means reaching this seam proves
    // the positional was exactly "serve" — nothing else parses.
    let McpArgs { serve } = args;
    debug_assert_eq!(serve, "serve");

    let catalog = build_catalog();
    let (resp_tx, resp_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let writer = tokio::spawn(writer_task(resp_rx));

    let mut stdin = tokio::io::BufReader::new(tokio::io::stdin());
    let mut line = String::new();
    loop {
        line.clear();
        match stdin.read_line(&mut line).await {
            Ok(0) | Err(_) => break, // stdin closed — the client is gone
            Ok(_) => {}
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(trimmed) {
            Ok(parsed) => parsed,
            Err(_) => {
                // Unparseable line WITHOUT a recoverable id: log to
                // stderr and keep serving — never std::process::exit,
                // never a non-message byte on stdout.
                tracing::warn!("mcp: dropped an unparseable stdin line");
                continue;
            }
        };
        let id = message.get("id").cloned();
        let Some(method) = message
            .get("method")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            // A Response (no method) has nothing to answer; ignore.
            tracing::warn!("mcp: ignored a message without a method");
            continue;
        };
        // Notifications (including lifecycle `notifications/initialized`)
        // never get a response — MCP/JSON-RPC rule.
        if method.starts_with("notifications/") {
            continue;
        }
        // A request-shaped message without an id IS a notification;
        // nothing to answer.
        let Some(id) = id else {
            continue;
        };
        match method.as_str() {
            "initialize" => {
                let requested = message
                    .pointer("/params/protocolVersion")
                    .and_then(Value::as_str);
                let result = json!({
                    "protocolVersion": negotiate(requested),
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "ign", "version": env!("CARGO_PKG_VERSION") },
                });
                let _ = resp_tx.send(response_message(Some(id), Ok(result)));
            }
            "ping" => {
                // INLINE on the reader path — never queued behind a
                // dispatch (SC-5): the empty response needs no gateway
                // work, so it is enqueued immediately, ahead of any
                // still-executing tools/call.
                let _ = resp_tx.send(response_message(Some(id), Ok(json!({}))));
            }
            "tools/list" => {
                // Full static catalog, no pagination — the cursor
                // param, if sent, is ignored gracefully (OQ2 pin).
                let tools: Vec<Value> = catalog.iter().map(tool_to_wire).collect();
                let _ = resp_tx.send(response_message(Some(id), Ok(json!({ "tools": tools }))));
            }
            "tools/call" => {
                let tx = resp_tx.clone();
                let catalog = Arc::clone(&catalog);
                let profile = profile_flag.map(str::to_string);
                let params = message.get("params").cloned().unwrap_or(Value::Null);
                // OFF the reader loop: a slow/dead gateway call must
                // never block ping (Pitfall 2).
                tokio::spawn(async move {
                    let answer = execute(&params, profile.as_deref(), &catalog).await;
                    let _ = tx.send(response_message(Some(id), answer));
                });
            }
            unknown => {
                // Unknown method WITH id → JSON-RPC protocol error.
                let _ = resp_tx.send(response_message(
                    Some(id),
                    Err((-32601, format!("Method not found: {unknown}"))),
                ));
            }
        }
    }
    // stdin closed: drop the sender so the writer drains and exits.
    drop(resp_tx);
    let _ = writer.await;
    ExitCode::SUCCESS
}

/// The ONE stdout owner: writes each pre-serialized response as
/// exactly one newline-terminated line. COMPACT ONLY — the lines
/// arrive from `serde_json::to_string`; embedded newlines inside JSON
/// strings are escaped by the serializer, so write+newline is exactly
/// one protocol frame (pretty-printing would be a protocol death).
async fn writer_task(mut resp_rx: tokio::sync::mpsc::UnboundedReceiver<String>) {
    use tokio::io::AsyncWriteExt;
    let mut stdout = tokio::io::stdout();
    while let Some(line) = resp_rx.recv().await {
        if stdout.write_all(line.as_bytes()).await.is_err()
            || stdout.write_all(b"\n").await.is_err()
        {
            break; // stdout closed — the client is gone
        }
        let _ = stdout.flush().await;
    }
}

/// Spec negotiation rule (2025-06-18 pin): echo the requested revision
/// when the server supports it; otherwise answer with the server's
/// latest — here the single supported revision either way.
fn negotiate(requested: Option<&str>) -> &'static str {
    match requested {
        Some(version) if version == PROTOCOL_VERSION => PROTOCOL_VERSION,
        _ => PROTOCOL_VERSION,
    }
}

/// One wire tool entry (name/title-less/description/inputSchema).
fn tool_to_wire(entry: &CatalogEntry) -> Value {
    json!({
        "name": entry.name,
        "description": entry.description,
        "inputSchema": entry.schema,
    })
}

/// Serialize one JSON-RPC response — compact, single line, id-matched.
fn response_message(id: Option<Value>, answer: Result<Value, (i64, String)>) -> String {
    let mut message = serde_json::Map::new();
    message.insert("jsonrpc".into(), Value::String("2.0".into()));
    if let Some(id) = id {
        message.insert("id".into(), id);
    }
    match answer {
        Ok(result) => {
            message.insert("result".into(), result);
        }
        Err((code, text)) => {
            message.insert("error".into(), json!({ "code": code, "message": text }));
        }
    }
    serde_json::to_string(&Value::Object(message)).expect("a JSON-RPC response serializes")
}

/// THE dispatch bridge (research Pattern 3, SC-1 + SC-2's execution
/// half): a tools/call becomes CLI tokens, parsed by the SAME clap
/// tree the catalog derives from, and dispatched IN-PROCESS through
/// the one `dispatch` seam (Session is the only auth path — CORE-09).
/// Returns the tool-result value: the frozen CLI envelope verbatim as
/// text content, `isError` true exactly when the envelope says
/// `ok:false` (business-logic errors ride isError, NOT protocol
/// errors).
async fn execute(
    params: &Value,
    profile_flag: Option<&str>,
    catalog: &Catalog,
) -> Result<Value, (i64, String)> {
    let tool_name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| (-32602, "tools/call requires a string tool name".to_string()))?;
    let entry = catalog
        .iter()
        .find(|entry| entry.name == tool_name)
        .ok_or_else(|| (-32601, format!("Unknown tool: {tool_name}")))?;
    let arguments = params.get("arguments").and_then(Value::as_object);

    // Purity guards (Pitfall 3): two dispatch arms print RAW lines to
    // stdout through in-dispatch sinks — `rig logs` (compose
    // passthrough in every mode) and `logs -f` (NDJSON). A stray
    // stdout byte is a protocol death for the MCP client, so the
    // bridge refuses these before any dispatch work. (The other
    // stdout exceptions never reach this path: completions is
    // catalog-excluded; `tags export -o -` carries its payload in the
    // result and prints only via render_ok, which the protocol path
    // never calls.)
    const SINK_STREAMING_TOOLS: &[&str] = &["rig_logs"];
    if SINK_STREAMING_TOOLS.contains(&tool_name) {
        return Err((
            -32602,
            format!(
                "tool {tool_name} streams raw lines to stdout and cannot ride the MCP \
                 protocol stream — run `ign {}` in a terminal instead",
                entry.path
            ),
        ));
    }
    let follow = arguments
        .and_then(|args| args.get("follow"))
        .and_then(Value::as_bool);
    if entry.path == "logs" && follow == Some(true) {
        return Err((
            -32602,
            "tool logs --follow streams raw lines to stdout and cannot ride the MCP \
             protocol stream — run `ign logs -f` in a terminal instead"
                .to_string(),
        ));
    }

    // Fail-closed argument validation: every key must be a known leaf
    // arg — or the synthetic `confirm` on guarded leaves. The schema
    // advertises exactly these, so a client that sends more is either
    // buggy or probing; in particular a hostile `yes` property can
    // NEVER ride through (SC-2 defense-in-depth — it is not a leaf
    // arg here because globals are excluded from every schema).
    let arguments = arguments.cloned().unwrap_or_default();
    let is_guarded = GUARDED_OPS.iter().any(|(path, _)| *path == entry.path);
    let mut known: HashSet<&str> = entry.args.iter().map(|spec| spec.key.as_str()).collect();
    if is_guarded {
        known.insert("confirm");
    }
    let unknown: Vec<&str> = arguments
        .keys()
        .map(String::as_str)
        .filter(|key| !known.contains(key))
        .collect();
    if !unknown.is_empty() {
        return Err((
            -32602,
            format!(
                "Unknown argument(s) for {tool_name}: {}",
                unknown.join(", ")
            ),
        ));
    }
    let confirm = arguments.get("confirm").and_then(Value::as_bool);

    // argv = ["ign"] + leaf-path segments + flag tokens — property
    // name IS the clap long name / positional id (1:1, no renaming).
    let mut argv: Vec<String> = vec!["ign".to_string()];
    argv.extend(entry.path.split(' ').map(str::to_string));
    for spec in &entry.args {
        let Some(value) = arguments.get(&spec.key) else {
            continue;
        };
        if spec.positional {
            // Positionals ride as raw tokens, in clap definition
            // order (arrays spread for repeatable positionals).
            match value {
                Value::Array(items) => {
                    for item in items {
                        if let Some(token) = scalar_token(item) {
                            argv.push(token);
                        }
                    }
                }
                other => {
                    if let Some(token) = scalar_token(other) {
                        argv.push(token);
                    }
                }
            }
        } else if spec.boolean {
            // SetTrue flags: only an explicit true emits the flag.
            if value.as_bool() == Some(true) {
                argv.push(format!("--{}", spec.key));
            }
        } else {
            // Value flags ride `--long=value` (the = form cannot be
            // mistaken for a flag even for dash-leading values);
            // arrays spread into repeated flags (Append args).
            match value {
                Value::Null => {}
                Value::Array(items) => {
                    for item in items {
                        if let Some(token) = scalar_token(item) {
                            argv.push(format!("--{}={}", spec.key, token));
                        }
                    }
                }
                other => {
                    if let Some(token) = scalar_token(other) {
                        argv.push(format!("--{}={}", spec.key, token));
                    }
                }
            }
        }
    }
    // THE confirm gate (SC-2): `--yes` is reachable ONLY from the
    // confirm field. No env merge exists on this path.
    if confirm == Some(true) {
        argv.push("--yes".to_string());
    }

    // NEVER `e.exit()` — clap would print usage to stdout and kill
    // the process mid-protocol. Parse failures are -32602 invalid
    // params carrying the message.
    let mut inner = Cli::try_parse_from(argv)
        .map_err(|err| (-32602, format!("Invalid arguments for {tool_name}: {err}")))?;
    // The bridge's fixed mutations (SC-2 + CORE-09): the ambient
    // profile (IGNITION_PROFILE was folded ONCE in main's
    // apply_env_defaults) rides in; json/compact are FORCED off (the
    // protocol path renders its own compact envelope below);
    // inner.yes stays EXACTLY as parsed — only confirm:true produced
    // --yes, so IGNITION_YES exported in an agent's shell can never
    // confirm a write.
    inner.profile = profile_flag.map(str::to_string);
    inner.json = false;
    inner.compact = false;

    // In-process dispatch in compact-json mode.
    let mode = RenderMode::resolve(true, true);
    let (profile, result) = crate::dispatch(inner, mode).await;

    // 14-06 checkpoint round 2 (FIX A): when a GUARDED verb is refused
    // on the confirmation_required path with confirm missing/false, the
    // envelope's message/hint prose is REWRITTEN to MCP-native advice —
    // the frozen CLI prose names `--yes`/IGNITION_YES, which 14-01
    // live-proved can never reach this protocol path, and an agent
    // that parses only the JSON must get the advice that works on its
    // transport. The rewrite rides the SAME locked envelope struct, so
    // shape/keys/field order/ok:false/code/profile:null/endpoint:null
    // are byte-stable; the CLI's own stdout/stderr prose NEVER changes
    // (second dated envelope-transport exception, alongside 09-03's
    // api-call one). Every other envelope stays verbatim, and every
    // result — refusal included — is ONE text block.
    let confirm_refusal = match &result {
        Err(err) => confirm != Some(true) && is_guarded && err.code() == "confirmation_required",
        Ok(_) => false,
    };
    let envelope = match &result {
        Ok(out) => success_envelope(out, profile.as_deref()),
        Err(err) if confirm_refusal => {
            mcp_confirm_refusal_envelope(err, profile.as_deref(), &entry.path)
        }
        Err(err) => failure_envelope(err, profile.as_deref()),
    };
    let is_error = envelope_reports_failure(&envelope);
    let content = vec![json!({ "type": "text", "text": envelope })];
    Ok(json!({
        "content": content,
        "isError": is_error,
    }))
}

/// The FROZEN success passthrough: EXACTLY the string `render_ok`
/// writes to stdout in CompactJson mode for the same inputs. The
/// tool result must never reshape the envelope (byte-equality pinned
/// by unit test).
fn success_envelope(out: &crate::ActionOutput, profile: Option<&str>) -> String {
    out.render_json(profile, true)
}

/// The FROZEN failure passthrough: EXACTLY the string `render_error`
/// writes to stderr in CompactJson mode (the compact serialization of
/// the LOCKED error envelope). Refusals ride this shape verbatim
/// everywhere EXCEPT the MCP confirmation_required one (see
/// [`mcp_confirm_refusal_envelope`]) — and even that exception only
/// swaps two prose fields inside the same struct.
fn failure_envelope(err: &CoreError, profile: Option<&str>) -> String {
    serde_json::to_string(&err.envelope(profile)).expect("envelope serialization cannot fail")
}

/// THE transport-aware refusal (14-06 checkpoint round 2 — the second
/// dated envelope-transport exception, alongside 09-03's api-call
/// one): the frozen envelope governs CLI stdout/stderr ONLY. Over MCP,
/// the confirmation_required refusal's message and hint PROSE are
/// MCP-native — an agent that parses only the JSON must get the advice
/// that works on its own transport (`--yes`/IGNITION_YES are
/// CLI-only and provably unreachable here), so the message names
/// `confirm: true` and the hint spells the argument form out. The
/// rewrite builds on the SAME locked envelope struct
/// ([`CoreError::envelope`]), so the shape/keys/field order and the
/// ok:false / code / profile:null / endpoint:null facts are
/// byte-stable by construction — only the two prose strings differ
/// from the CLI's refusal.
fn mcp_confirm_refusal_envelope(err: &CoreError, profile: Option<&str>, verb: &str) -> String {
    let mut env = err.envelope(profile);
    env.error.message = format!("{verb} is destructive; rerun with confirm: true to confirm");
    env.error.hint = Some(
        "Set {\"confirm\": true} in the tool arguments to execute it. (--yes and \
         IGNITION_YES=1 are CLI-only confirmations — they cannot confirm a call made \
         over MCP.)"
            .to_string(),
    );
    serde_json::to_string(&env).expect("envelope serialization cannot fail")
}

/// isError = the envelope says `ok:false`. A payload that is not a
/// parseable envelope (not produced by the paths above, but defensive)
/// counts as failure — fail-closed.
fn envelope_reports_failure(envelope: &str) -> bool {
    serde_json::from_str::<Value>(envelope)
        .ok()
        .and_then(|parsed| parsed.get("ok").and_then(Value::as_bool).map(|ok| !ok))
        .unwrap_or(true)
}

/// JSON scalar → CLI token text. Strings pass through; numbers/bools
/// stringify; null vanishes (the caller skips it); objects serialize
/// compact (the CLI's own validation refuses nonsense downstream);
/// arrays never reach here (the caller spreads them).
fn scalar_token(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::Bool(boolean) => Some(boolean.to_string()),
        Value::Number(number) => Some(number.to_string()),
        Value::String(text) => Some(text.clone()),
        Value::Array(_) => None,
        Value::Object(_) => Some(value.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::GUARDED_OPS;

    /// An INDEPENDENT leaf walk of the clap tree (the tui_coverage.rs
    /// recursion verbatim) with the SAME filters the builder applies —
    /// hidden-skip, help-skip, exclusion set, tool-name encoding. Any
    /// divergence between this walk and `build_catalog`'s is a bug in
    /// one of them.
    fn independent_leaf_names() -> BTreeSet<String> {
        fn walk(cmd: &clap::Command, prefix: &str, out: &mut BTreeSet<String>) {
            for sub in cmd.get_subcommands() {
                if sub.get_name() == "help" || sub.is_hide_set() {
                    continue;
                }
                let path = if prefix.is_empty() {
                    sub.get_name().to_string()
                } else {
                    format!("{prefix} {}", sub.get_name())
                };
                let has_real_children = sub.get_subcommands().any(|c| c.get_name() != "help");
                if (!has_real_children || !sub.is_subcommand_required_set())
                    && !path_is_excluded(&path)
                {
                    out.insert(tool_name_for_path(&path));
                }
                walk(sub, &path, out);
            }
        }
        let mut out = BTreeSet::new();
        walk(&Cli::command(), "", &mut out);
        out
    }

    /// SC-1's structural fact, machine-proven: the derived catalog and
    /// an independent clap-tree walk are the SAME set — a leaf without
    /// a tool and a tool without a leaf both fail, and a silently
    /// skipped subtree cannot hide (cardinality pin).
    #[test]
    fn catalog_names_match_the_clap_tree_bidirectionally() {
        let catalog: BTreeSet<String> = build_catalog().iter().map(|e| e.name.clone()).collect();
        let tree = independent_leaf_names();
        assert!(!catalog.is_empty(), "the catalog derived the tree");
        let missing: Vec<_> = tree.difference(&catalog).collect();
        assert!(
            missing.is_empty(),
            "clap leaves with no tool entry (builder filter bug): {missing:#?}"
        );
        let extra: Vec<_> = catalog.difference(&tree).collect();
        assert!(
            extra.is_empty(),
            "tool entries with no clap leaf (hand-written drift): {extra:#?}"
        );
        assert_eq!(catalog.len(), tree.len(), "bidirectional set equality");
    }

    /// SC-2's catalog half: every GUARDED_OPS leaf advertises an
    /// OPTIONAL `confirm` boolean (present in properties, ABSENT from
    /// `required` — a schema-required confirm would invite agents to
    /// auto-fill it and defeat the gate); a read-only verb advertises
    /// none.
    #[test]
    fn guarded_leaves_carry_optional_confirm_and_readonly_leaves_do_not() {
        let catalog = build_catalog();

        let delete = catalog
            .iter()
            .find(|e| e.name == "project_delete")
            .expect("project_delete derives from the clap tree");
        let delete_props = delete.schema.get("properties").expect("schema object");
        let confirm = delete_props
            .get("confirm")
            .expect("guarded leaf carries confirm");
        assert_eq!(
            confirm.get("type").and_then(Value::as_str),
            Some("boolean"),
            "confirm is a boolean property"
        );
        let delete_required = delete
            .schema
            .get("required")
            .and_then(Value::as_array)
            .expect("required array");
        assert!(
            !delete_required.iter().any(|v| v == "confirm"),
            "confirm must be OPTIONAL on project_delete \
             (schema-required invites reflexive auto-fill): {delete_required:?}"
        );

        let status = catalog
            .iter()
            .find(|e| e.name == "status")
            .expect("status derives from the clap tree");
        assert!(
            status
                .schema
                .get("properties")
                .and_then(|props| props.get("confirm"))
                .is_none(),
            "the read-only status verb carries no confirm property"
        );

        // And EVERY guarded leaf that is CATALOG-ELIGIBLE advertises
        // it (the registry → catalog direction; the drift test in
        // main.rs pins the dispatch-site → registry direction). One
        // GUARDED_OPS member is deliberately absent from the catalog:
        // `edit` — its guard is real (the push gate) but its product
        // is a child-$EDITOR terminal handoff, excluded as a tool call
        // by the planner-locked set. Pin that intersection so neither
        // half can drift silently.
        for (path, _) in GUARDED_OPS {
            let name = tool_name_for_path(path);
            if path_is_excluded(path) {
                assert_eq!(
                    *path, "edit",
                    "only edit may be both guarded and catalog-excluded"
                );
                assert!(
                    !catalog.iter().any(|e| e.name == name),
                    "excluded leaf {name} must not appear in the catalog"
                );
                continue;
            }
            let entry = catalog
                .iter()
                .find(|e| e.name == name)
                .unwrap_or_else(|| panic!("GUARDED_OPS leaf {path:?} missing from the catalog"));
            assert!(
                entry
                    .schema
                    .get("properties")
                    .and_then(|props| props.get("confirm"))
                    .is_some(),
                "guarded leaf {name} must carry the confirm property"
            );
        }
    }

    /// The catalog's name encoding is injective over the leaf paths —
    /// two distinct leaves mapping to one tool name would let
    /// tools/call dispatch the WRONG verb.
    #[test]
    fn tool_name_encoding_is_injective_over_the_clap_tree() {
        let catalog = build_catalog();
        let mut names: Vec<&str> = catalog.iter().map(|e| e.name.as_str()).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "tool names must be unique");
    }

    /// The negotiation rule (OQ5 pin): the requested 2025-06-18 is
    /// echoed EXACTLY; anything else (older, newer, absent, wrong
    /// type) answers with the server's single supported revision.
    #[test]
    fn initialize_negotiation_echoes_the_pinned_revision() {
        assert_eq!(negotiate(Some("2025-06-18")), "2025-06-18");
        assert_eq!(negotiate(Some("2024-11-05")), "2025-06-18");
        assert_eq!(negotiate(Some("2030-01-01")), "2025-06-18");
        assert_eq!(negotiate(None), "2025-06-18");
    }

    /// Framing: a response is ONE compact line — jsonrpc, id, and
    /// result/error — never pretty-printed (embedded newlines are a
    /// spec violation and a protocol death).
    #[test]
    fn response_message_is_single_line_json_rpc() {
        let ok = response_message(Some(json!(1)), Ok(json!({})));
        assert!(!ok.contains('\n'), "compact only: {ok}");
        let parsed: Value = serde_json::from_str(&ok).expect("valid JSON");
        assert_eq!(parsed.get("jsonrpc").and_then(Value::as_str), Some("2.0"));
        assert_eq!(parsed.get("id").and_then(Value::as_i64), Some(1));
        assert!(parsed.get("result").is_some());

        // String ids ride verbatim (the ping spec example uses "123").
        let string_id = response_message(Some(json!("123")), Ok(json!({})));
        let parsed: Value = serde_json::from_str(&string_id).expect("valid JSON");
        assert_eq!(parsed.get("id").and_then(Value::as_str), Some("123"));

        let err = response_message(Some(json!(7)), Err((-32601, "Method not found".into())));
        assert!(!err.contains('\n'), "compact only: {err}");
        let parsed: Value = serde_json::from_str(&err).expect("valid JSON");
        assert_eq!(
            parsed.pointer("/error/code").and_then(Value::as_i64),
            Some(-32601)
        );
        assert!(parsed.get("result").is_none());
    }

    /// THE envelope passthrough pin (SC-2's frozen-shape contract):
    /// the tool-result envelope strings are BYTE-EQUAL to what the
    /// chassis renders for the same inputs in CompactJson mode —
    /// success == render_ok's stdout line; failure == render_error's
    /// stderr line (the compact serialization of the LOCKED envelope).
    /// The MCP layer must never reshape an envelope.
    #[test]
    fn envelope_serializations_are_chassis_byte_identical() {
        use ignition_core::actions;
        use ignition_core::output::render_failure;

        let out = crate::ActionOutput::Version(actions::version::VersionResult {
            cli_version: env!("CARGO_PKG_VERSION"),
            gateway: None,
            warnings: vec![],
        });
        let success = success_envelope(&out, Some("dev"));
        assert_eq!(
            success,
            out.render_json(Some("dev"), true),
            "success envelope == render_ok's CompactJson stdout line"
        );
        assert!(!success.contains('\n'), "compact only: {success}");
        let parsed: Value = serde_json::from_str(&success).expect("valid envelope JSON");
        assert_eq!(parsed.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(parsed.get("profile").and_then(Value::as_str), Some("dev"));

        let err = CoreError::ConfirmationRequired {
            operation: "project delete".to_string(),
        };
        let failure = failure_envelope(&err, Some("dev"));
        assert_eq!(
            failure,
            render_failure(&err.envelope(Some("dev")), true),
            "failure envelope == render_error's CompactJson stderr line"
        );
        assert!(!failure.contains('\n'), "compact only: {failure}");
        let parsed: Value = serde_json::from_str(&failure).expect("valid envelope JSON");
        assert_eq!(parsed.get("ok").and_then(Value::as_bool), Some(false));
        assert_eq!(
            parsed.pointer("/error/code").and_then(Value::as_str),
            Some("confirmation_required"),
            "the refusal rides the frozen envelope verbatim"
        );
    }

    /// THE transport-aware refusal pin (14-06 checkpoint round 2): the
    /// MCP confirmation_required envelope carries MCP-native
    /// message/hint prose — and NOTHING else changes vs. the CLI
    /// refusal: same locked field order (byte-level prefix pin), same
    /// ok:false / code / profile:null / endpoint:null facts. Swapping
    /// the two prose strings back for the CLI's own turns the payload
    /// into a byte-equal twin of [`failure_envelope`]'s output —
    /// proving shape-preserved, prose-adapted.
    #[test]
    fn mcp_confirm_refusal_adapts_prose_only_shape_byte_stable() {
        let err = CoreError::ConfirmationRequired {
            operation: "project delete".to_string(),
        };
        let cli = failure_envelope(&err, Some("dev"));
        let mcp = mcp_confirm_refusal_envelope(&err, Some("dev"), "project delete");

        // The locked field order survives: ok, profile, error.code —
        // the serialized bytes start exactly where the CLI envelope
        // starts.
        let stable_prefix =
            r#"{"ok":false,"profile":"dev","error":{"code":"confirmation_required","#;
        assert!(
            mcp.starts_with(stable_prefix),
            "the MCP refusal must serialize in the LOCKED field order: {mcp}"
        );

        let mcp_parsed: Value = serde_json::from_str(&mcp).expect("valid envelope JSON");
        let cli_parsed: Value = serde_json::from_str(&cli).expect("valid envelope JSON");
        assert_eq!(
            mcp_parsed.get("ok"),
            cli_parsed.get("ok"),
            "ok:false both ways"
        );
        assert_eq!(
            mcp_parsed.get("profile"),
            cli_parsed.get("profile"),
            "profile echo unchanged"
        );
        assert_eq!(
            mcp_parsed.pointer("/error/code"),
            cli_parsed.pointer("/error/code"),
            "the stable slug is unchanged"
        );
        assert_eq!(
            mcp_parsed.pointer("/error/endpoint"),
            cli_parsed.pointer("/error/endpoint"),
            "endpoint is unchanged (null on the pre-network refusal)"
        );

        // The prose: MCP-native both fields.
        let message = mcp_parsed
            .pointer("/error/message")
            .and_then(Value::as_str)
            .expect("message present");
        assert_eq!(
            message, "project delete is destructive; rerun with confirm: true to confirm",
            "the message names the MCP-native confirmation: {message}"
        );
        let hint = mcp_parsed
            .pointer("/error/hint")
            .and_then(Value::as_str)
            .expect("hint present");
        assert!(hint.contains("confirm"), "hint names confirm: {hint}");
        assert!(
            hint.contains("CLI-only"),
            "hint marks the CLI paths: {hint}"
        );
        assert!(
            hint.contains("IGNITION_YES"),
            "hint marks the env path: {hint}"
        );

        // Shape-preserved, prose-adapted: restoring the CLI's two prose
        // strings makes the payloads EQUAL (parsed comparison is
        // key-order-insensitive; the byte prefix pin above covers
        // order).
        let mut repaired = mcp_parsed.clone();
        repaired["error"]["message"] = cli_parsed["error"]["message"].clone();
        repaired["error"]["hint"] = cli_parsed["error"]["hint"].clone();
        assert_eq!(
            repaired, cli_parsed,
            "only message/hint may differ from the frozen CLI envelope"
        );
    }

    /// isError semantics: true exactly when the envelope says ok:false;
    /// a non-envelope payload counts as failure (fail-closed).
    #[test]
    fn is_error_follows_the_envelope_ok_field() {
        assert!(!envelope_reports_failure(
            r#"{"ok":true,"profile":null,"data":{}}"#
        ));
        assert!(envelope_reports_failure(
            r#"{"ok":false,"profile":null,"error":{"code":"x","message":"m","endpoint":null,"hint":null}}"#
        ));
        assert!(envelope_reports_failure("not json at all"));
        assert!(envelope_reports_failure(r#"{"no_ok_field":1}"#));
    }

    /// JSON scalar → token rules: strings pass through, numbers/bools
    /// stringify, null vanishes, objects serialize compact.
    #[test]
    fn scalar_tokens_map_1_to_1_to_cli_tokens() {
        assert_eq!(scalar_token(&json!("hello")), Some("hello".to_string()));
        assert_eq!(scalar_token(&json!(42)), Some("42".to_string()));
        assert_eq!(scalar_token(&json!(1.5)), Some("1.5".to_string()));
        assert_eq!(scalar_token(&json!(true)), Some("true".to_string()));
        assert_eq!(scalar_token(&Value::Null), None);
        assert_eq!(
            scalar_token(&json!({"a":1})),
            Some(r#"{"a":1}"#.to_string())
        );
    }

    /// The bridge's refusals — all BEFORE any dispatch work (zero
    /// network): unknown tool → -32601; hostile/unknown argument keys
    /// → -32602 (the SC-2 defense-in-depth probe: a hostile `yes`
    /// property can never ride through); raw-stdout streaming verbs →
    /// -32602; missing required arguments → -32602 via clap WITHOUT
    /// e.exit().
    #[tokio::test]
    async fn execute_refuses_before_dispatch() {
        let catalog = build_catalog();

        // Unknown tool → -32601.
        let err = execute(&json!({ "name": "nonexistent_verb" }), None, &catalog)
            .await
            .expect_err("unknown tool");
        assert_eq!(err.0, -32601);

        // THE SC-2 probe: `yes` is not a leaf arg on ANY tool — a
        // hostile client cannot smuggle --yes through the arguments
        // object (it would bypass the confirm gate).
        let err = execute(
            &json!({ "name": "status", "arguments": { "yes": true } }),
            None,
            &catalog,
        )
        .await
        .expect_err("hostile yes must refuse");
        assert_eq!(err.0, -32602, "{err:?}");
        assert!(err.1.contains("yes"), "names the offending key: {err:?}");

        // Unknown argument (not in the leaf's schema) → -32602.
        let err = execute(
            &json!({ "name": "version", "arguments": { "bogus": 1 } }),
            None,
            &catalog,
        )
        .await
        .expect_err("unknown argument");
        assert_eq!(err.0, -32602);

        // Raw-stdout streaming verbs refuse (Pitfall 3): rig_logs in
        // every mode, logs with follow=true.
        let err = execute(&json!({ "name": "rig_logs" }), None, &catalog)
            .await
            .expect_err("raw-streaming verb");
        assert_eq!(err.0, -32602);
        let err = execute(
            &json!({ "name": "logs", "arguments": { "follow": true } }),
            None,
            &catalog,
        )
        .await
        .expect_err("follow streams raw lines");
        assert_eq!(err.0, -32602);

        // Missing required arguments → clap parse error → -32602
        // (NEVER e.exit(): the process must survive the protocol).
        let err = execute(
            &json!({ "name": "tags_read", "arguments": {} }),
            None,
            &catalog,
        )
        .await
        .expect_err("missing required positional");
        assert_eq!(err.0, -32602);
    }
}
