//! `ign lsp` — the LSP transport over stdio (14-03).
//!
//! Completes the transport pattern the MCP slice (14-01) proved, for the
//! second protocol consumer: OutOfBand seam, Session-only auth, cache-only
//! handlers.
//!
//! ## Ownership split (the Don't-Hand-Roll table)
//!
//! [`lsp_server`] owns the fiddly base protocol: Content-Length framing,
//! the initialize handshake, the shutdown handshake, and the stdio
//! reader/writer threads. WE own the sync dispatch loop on the main
//! thread — a `crossbeam_channel::select!` over the connection receiver
//! and the refresher's snapshot version channel with NO runtime, NO
//! block_on, and NO network anywhere on the request path (SC-3, both
//! halves: handlers are pure fns over a snapshot frame; the select arm
//! publishes diagnostics after every refresh cycle).
//!
//! ## The runtime story (the roadmap flag, resolved)
//!
//! `block_on` is legal on exactly ONE thread in this module: the
//! background refresher thread that owns the gateway cache (Task 2). The
//! LSP loop itself never touches a runtime — handlers read a TTL snapshot
//! behind an `Arc<RwLock<Arc<Snapshot>>>` and return.
//!
//! ## Capabilities: narrow BY DESIGN (composition, not replacement)
//!
//! Declared: `completion`, `hover`, full-text sync. NEVER claimed:
//! definition / codeAction / workspace symbols — Python ignition-lsp owns
//! those statics, and nvim merges completions across attached clients.
//! Doubling a capability makes nvim answer "go to definition" from two
//! servers and degrades the composition (research Anti-Pattern 5).
//!
//! ## Logging
//!
//! `init_tracing` (main.rs) already installed the stderr writer before
//! this seam runs — stderr doubles as the LSP log channel per the LSP
//! convention. No new logging surface; stdout carries ONLY framed
//! protocol bytes (a stray byte would kill the client).

use std::collections::{BTreeMap, HashMap};
use std::process::ExitCode;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, Diagnostic,
    DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, Hover, HoverContents, HoverProviderCapability, MarkupContent,
    MarkupKind, Position, PublishDiagnosticsParams, Range, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind,
};

use ignition_core::client::GatewayApi as _;
use ignition_core::client::query::ListQuery;
use ignition_core::session::Session;

/// Serve LSP over stdio. The OutOfBand exit seam: SUCCESS on a clean
/// client-driven shutdown (or protocol-loop end), FAILURE with the error
/// on stderr for any protocol death. Nothing on the request path reads
/// config or the network — the ambient profile flag rides to the
/// GatewayCache refresher thread, whose failures degrade the snapshot
/// instead of the server.
pub fn serve(profile_flag: Option<&str>) -> ExitCode {
    match run(profile_flag) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!("lsp: protocol error: {err}");
            ExitCode::FAILURE
        }
    }
}

/// The sync dispatch loop. 14-04's structural delta: the loop
/// `select!`s over the message receiver AND the refresher's snapshot
/// version channel, so diagnostics publish after EVERY refresh cycle —
/// handlers themselves stay pure snapshot reads (SC-3).
fn run(profile_flag: Option<&str>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (connection, io_threads) = Connection::stdio();
    let caps = serde_json::to_value(server_capabilities())?;
    let _init_params = connection.initialize(caps)?;
    tracing::info!("lsp: handshake complete — serving over stdio");

    // The gateway truth plane: a background refresher owns ALL network
    // and the module's only runtime; the loop below never blocks on
    // either. `kicks` is didSave's refresh request; `snapshots` carries
    // each published snapshot version for diagnostics publishing.
    let (kicks_tx, kicks_rx) = crossbeam_channel::unbounded::<()>();
    let (cache, snapshots_rx) = GatewayCache::spawn(profile_flag, kicks_rx);

    // The open-docs map: the ONLY record of what the client has open
    // (FULL sync ⇒ the loop owns the full text). It is the diagnostics
    // and completion input — the server never fetches file contents
    // anywhere else. Keyed by the URI STRING (exactly how lsp-types
    // compares Uris — and it keeps `Uri`'s interior-mutable innards out
    // of the key type). (uri, (text, version)).
    let mut docs: HashMap<String, (String, i32)> = HashMap::new();

    // The refresher may fail to spawn (or die mid-teardown); once its
    // sender is gone the version arm would spin, so it is swapped for a
    // receiver that never fires. The server keeps serving regardless.
    let mut versions: Receiver<Arc<Snapshot>> = snapshots_rx;

    loop {
        crossbeam_channel::select! {
            recv(&connection.receiver) -> msg => {
                let Ok(msg) = msg else {
                    break; // client closed the pipe — loop end is the exit handshake's sibling
                };
                match msg {
                    Message::Request(req) => {
                        // The shutdown handshake is lsp-server's too; `true` here
                        // means "client sent shutdown" — answer it, then exit the
                        // loop cleanly (the client follows with `exit`).
                        if connection.handle_shutdown(&req)? {
                            tracing::info!("lsp: shutdown handshake complete");
                            return Ok(());
                        }
                        let resp = handle_request(req, &cache, &docs);
                        connection.sender.send(Message::Response(resp))?;
                    }
                    Message::Notification(note) => {
                        // Spec: a bare `exit` notification terminates the server.
                        // (`initialized` is consumed by connection.initialize.)
                        if note.method == "exit" {
                            return Ok(());
                        }
                        handle_notification(note, &cache, &mut docs, &kicks_tx, &connection)?;
                    }
                    // We never call the client, so a Response cannot arrive; the
                    // arm exists for Message exhaustiveness.
                    Message::Response(_) => {}
                }
            }
            recv(&versions) -> snap => {
                match snap {
                    // A refresh cycle completed: republish diagnostics for
                    // every open doc from the NEW frame (staleness moves,
                    // verdicts re-derive — never silently stale).
                    Ok(published) => {
                        for (key, (text, version)) in &docs {
                            let uri = lsp_types::Uri::from_str(key)
                                .map_err(|err| format!("lsp: open-doc URI {key:?} stopped parsing: {err}"))?;
                            publish_diagnostics(&connection, &published, &uri, text, Some(*version))?;
                        }
                    }
                    // Refresher gone (spawn failure or teardown): stop
                    // selecting on it — `never()` never becomes ready.
                    Err(_) => versions = crossbeam_channel::never(),
                }
            }
        }
    }
    io_threads.join()?;
    Ok(())
}

/// THE narrow capability surface. Two providers + full-text sync, nothing
/// else: see the module docs for the composition contract (ignition-lsp
/// owns definition/codeAction/workspace symbols — never claimed here).
fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec!["/".into(), ".".into()]),
            ..Default::default()
        }),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        ..Default::default()
    }
}

/// Request dispatch. Cache-only (SC-3): the body reads the TTL snapshot
/// behind the read lock and returns — zero network, zero runtime.
fn handle_request(
    req: Request,
    cache: &GatewayCache,
    docs: &HashMap<String, (String, i32)>,
) -> Response {
    // THE read path (SC-3's structural half): ONE Arc clone under the
    // lock, then the frame is read lock-free. The health/age log IS the
    // request-path evidence: what a request can know is what this frame
    // carries, nothing else.
    let snap = cache.snapshot();
    tracing::debug!(
        healthy = snap.healthy,
        providers = snap.providers.len(),
        tags = snap.tag_trees.values().map(Vec::len).sum::<usize>(),
        queries = snap.named_queries.values().map(Vec::len).sum::<usize>(),
        projects = snap.projects.len(),
        age = ?snap.fetched_at.elapsed(),
        "lsp: answering from cache snapshot (zero network on the request path)"
    );
    match req.method.as_str() {
        "textDocument/completion" => {
            // Malformed params answer EMPTY (an honest empty beats a
            // protocol error for a hostile client — nvim stays alive).
            let items = serde_json::from_value::<CompletionParams>(req.params)
                .ok()
                .and_then(|params| {
                    let uri = params.text_document_position.text_document.uri;
                    let position = params.text_document_position.position;
                    docs.get(uri.as_str())
                        .map(|(text, _)| completions(&snap, text, position))
                })
                .unwrap_or_default();
            Response::new_ok(req.id, items)
        }
        "textDocument/hover" => {
            let hovered = serde_json::from_value::<lsp_types::HoverParams>(req.params)
                .ok()
                .and_then(|params| {
                    let uri = params.text_document_position_params.text_document.uri;
                    let position = params.text_document_position_params.position;
                    docs.get(uri.as_str())
                        .and_then(|(text, _)| hover(&snap, text, position))
                });
            Response::new_ok(req.id, hovered)
        }
        other => method_not_found(req.id, other),
    }
}

/// Notification dispatch: the open-docs map maintenance (didOpen/
/// didChange FULL/didClose), the didSave → refresh kick, and the
/// cache-derived publishDiagnostics after every doc-affecting event.
fn handle_notification(
    note: Notification,
    cache: &GatewayCache,
    docs: &mut HashMap<String, (String, i32)>,
    kicks: &Sender<()>,
    connection: &Connection,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match note.method.as_str() {
        "textDocument/didOpen" => {
            let params: DidOpenTextDocumentParams = serde_json::from_value(note.params)?;
            let key = params.text_document.uri.as_str().to_string();
            docs.insert(
                key.clone(),
                (params.text_document.text, params.text_document.version),
            );
            publish_for_doc(connection, cache, docs, &key)?;
        }
        "textDocument/didChange" => {
            let params: DidChangeTextDocumentParams = serde_json::from_value(note.params)?;
            let key = params.text_document.uri.as_str().to_string();
            // FULL sync: the ONE range-less change carries the whole
            // text — anything range-bearing is a client contract
            // violation we refuse to apply (it would corrupt the map).
            if let Some(change) = params
                .content_changes
                .iter()
                .rev()
                .find(|change| change.range.is_none())
            {
                docs.insert(
                    key.clone(),
                    (change.text.clone(), params.text_document.version),
                );
            }
            publish_for_doc(connection, cache, docs, &key)?;
        }
        "textDocument/didClose" => {
            let params: DidCloseTextDocumentParams = serde_json::from_value(note.params)?;
            let uri = params.text_document.uri.clone();
            docs.remove(uri.as_str());
            // Spec: publish an empty set so the client clears its view.
            let params = PublishDiagnosticsParams {
                uri,
                diagnostics: Vec::new(),
                version: None,
            };
            connection
                .sender
                .send(Message::Notification(Notification::new(
                    "textDocument/publishDiagnostics".into(),
                    params,
                )))?;
        }
        "textDocument/didSave" => {
            // Best-effort refresh kick — the request path NEVER awaits
            // it (SC-3); the refresher drains bursts into one cycle.
            let _ = kicks.send(());
        }
        _ => {}
    }
    Ok(())
}

/// Publish diagnostics for ONE open doc from the current snapshot.
fn publish_for_doc(
    connection: &Connection,
    cache: &GatewayCache,
    docs: &HashMap<String, (String, i32)>,
    key: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let Some((text, version)) = docs.get(key) else {
        return Ok(());
    };
    let snap = cache.snapshot();
    let uri = lsp_types::Uri::from_str(key)
        .map_err(|err| format!("lsp: open-doc URI {key:?} stopped parsing: {err}"))?;
    publish_diagnostics(connection, &snap, &uri, text, Some(*version))
}

/// THE publish site: one notification, derived from the snapshot frame
/// by [`diagnostics_for`] (pure) and sent through lsp-server's writer.
fn publish_diagnostics(
    connection: &Connection,
    snap: &Snapshot,
    uri: &lsp_types::Uri,
    text: &str,
    version: Option<i32>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let params = diagnostics_for(snap, uri, text, version);
    connection
        .sender
        .send(Message::Notification(Notification::new(
            "textDocument/publishDiagnostics".into(),
            params,
        )))?;
    Ok(())
}

/// LSP error code -32601 for anything the narrow surface never claimed —
/// including definition/codeAction/workspace symbols, which stay
/// unanswered so nvim's OTHER attached client owns them.
fn method_not_found(id: RequestId, method: &str) -> Response {
    Response::new_err(
        id,
        -32601, // MethodNotFound
        format!(
            "ign lsp does not provide {method} (narrow-by-design surface: \
             completion + hover; statics belong to ignition-lsp)"
        ),
    )
}

// ---------------------------------------------------------------------------
// The pure handler layer: everything below is a total function over
// (&Snapshot, text, position) — zero network, zero runtime, zero locks.
// The request path composes these with one cache.snapshot() Arc clone
// and nothing else (SC-3's behavioral half, unit-testable directly).
// ---------------------------------------------------------------------------

/// The diagnostics' source label (nvim shows it per diagnostic).
const LSP_SOURCE: &str = "ign lsp";

/// One `[provider]path` reference scanned out of an open document.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DocReference {
    /// The name between the brackets.
    provider: String,
    /// The path suffix immediately after `]` (empty for a bare
    /// provider mention like completion's `[def`).
    path: String,
    /// 0-based line.
    line: u32,
    /// Character span of `[provider]path` (code-point offsets — the
    /// surfaces here are ASCII paths; a non-ASCII prefix may shift the
    /// reported span, never the verdict).
    start: u32,
    end: u32,
}

/// One `runNamedQuery("project", "path", …)` call scanned out of a
/// document (the binding every Ignition scripting surface uses).
#[derive(Debug, Clone, PartialEq, Eq)]
struct NamedQueryRef {
    project: String,
    path: String,
    line: u32,
    start: u32,
    end: u32,
}

fn is_path_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '/' | '.' | '-')
}

/// The word (identifier-ish token including the bracket/path alphabet)
/// under the LSP position — completion's context and hover's lookup key.
fn word_at(text: &str, line: u32, character: u32) -> String {
    let Some(line_text) = text.lines().nth(line as usize) else {
        return String::new();
    };
    let chars: Vec<char> = line_text.chars().collect();
    let is_word = |c: char| is_path_char(c) || c == '[' || c == ']';
    let idx = (character as usize).min(chars.len());
    let mut start = idx;
    while start > 0 && is_word(chars[start - 1]) {
        start -= 1;
    }
    let mut end = idx;
    while end < chars.len() && is_word(chars[end]) {
        end += 1;
    }
    chars[start..end].iter().collect()
}

/// The provider a completion position is INSIDE, per the plan's honest
/// heuristic: a CLOSED `[name]` bracket before the cursor names the
/// provider (e.g. `[default]Motors/` → default); an unterminated
/// `[def` is treated as no context (all providers' paths flow — simple
/// and honest over clever partial matching).
fn provider_context(line_before: &str) -> Option<String> {
    let open = line_before.rfind('[')?;
    let rest = &line_before[open + 1..];
    let close = rest.find(']')?;
    let name = &rest[..close];
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// COMPLETION: the three-family merge over one snapshot frame —
/// (a) provider names (kind Module), (b) tag paths for the referenced
/// provider (kind File; all providers when no closed-bracket context),
/// (c) named-query paths for known projects (kind Function). Labels are
/// the raw snapshot paths. Empty (healthy:false) snapshot → empty list,
/// never an error.
fn completions(snap: &Snapshot, text: &str, position: Position) -> Vec<CompletionItem> {
    if !snap.healthy {
        return Vec::new();
    }
    let line = text.lines().nth(position.line as usize).unwrap_or("");
    let before: String = line.chars().take(position.character as usize).collect();
    let referenced = provider_context(&before);

    let mut items = Vec::new();
    for name in &snap.providers {
        items.push(CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::MODULE),
            detail: Some("tag provider".into()),
            ..Default::default()
        });
    }
    for (provider, paths) in &snap.tag_trees {
        if referenced.as_ref().is_some_and(|want| want != provider) {
            continue;
        }
        for path in paths {
            items.push(CompletionItem {
                label: path.clone(),
                kind: Some(CompletionItemKind::FILE),
                detail: Some(format!("tag path in [{provider}]")),
                ..Default::default()
            });
        }
    }
    for queries in snap.named_queries.values() {
        for query in queries {
            items.push(CompletionItem {
                label: query.clone(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some("named query".into()),
                ..Default::default()
            });
        }
    }
    items
}

/// The staleness stamp rendered into every hover and hint (Pitfall 4:
/// staleness is NEVER silent — the payload carries the frame's age and
/// the TTL so an editor can show the reader exactly how old its truth
/// is).
fn age_stamp(snap: &Snapshot) -> String {
    format!(
        "gateway snapshot {}s old (TTL {}s)",
        snap.fetched_at.elapsed().as_secs(),
        LSP_CACHE_TTL.as_secs()
    )
}

/// HOVER: word under the cursor → provider / tag-path / named-query
/// lookup in the snapshot → contents carrying the match description AND
/// the snapshot age stamp. No match → None (the client shows nothing).
/// A bracketed word (`[default]`) matches the provider family via its
/// bare inner name; paths keep their raw form (they ARE the keys).
fn hover(snap: &Snapshot, text: &str, position: Position) -> Option<Hover> {
    let word = word_at(text, position.line, position.character);
    if word.is_empty() {
        return None;
    }
    let bare = word.trim_start_matches('[').trim_end_matches(']');
    let (label, description) = if snap.providers.iter().any(|p| *p == bare) {
        (bare, "tag provider")
    } else if snap.tag_trees.values().flatten().any(|path| *path == word) {
        (word.as_str(), "tag path")
    } else if snap
        .named_queries
        .values()
        .flatten()
        .any(|query| *query == word)
    {
        (word.as_str(), "named query")
    } else {
        return None;
    };
    let value = format!("{label} — {description} — {}", age_stamp(snap));
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: None,
    })
}

/// Every `[provider]path` reference in the document, line by line
/// (lines never join — the scanner is deliberately line-local).
fn scan_references(text: &str) -> Vec<DocReference> {
    let mut refs = Vec::new();
    for (line_no, line) in text.lines().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] != '[' {
                i += 1;
                continue;
            }
            let Some(close) = (i + 1..chars.len()).find(|&j| chars[j] == ']') else {
                break; // unterminated bracket — nothing to judge this line
            };
            let provider: String = chars[i + 1..close].iter().collect();
            let mut end = close + 1;
            while end < chars.len() && is_path_char(chars[end]) {
                end += 1;
            }
            refs.push(DocReference {
                provider,
                path: chars[close + 1..end].iter().collect(),
                line: line_no as u32,
                start: i as u32,
                end: end as u32,
            });
            i = end;
        }
    }
    refs
}

/// Every `runNamedQuery("project", "path", …)` call — the first two
/// double-quoted literals after the paren, line-local, bounded scan.
fn scan_named_query_calls(text: &str) -> Vec<NamedQueryRef> {
    const NEEDLE: &str = "runNamedQuery(";
    const SCAN_BOUND: usize = 512;
    let mut refs = Vec::new();
    for (line_no, line) in text.lines().enumerate() {
        let mut from = 0usize;
        while let Some(found) = line[from..].find(NEEDLE) {
            let paren = from + found + NEEDLE.len();
            let rest = &line[paren..];
            let mut quoted = Vec::new();
            let mut consumed = 0usize;
            while quoted.len() < 2 && consumed < SCAN_BOUND {
                let Some(open) = rest[consumed..].find('"') else {
                    break;
                };
                let open = consumed + open + 1;
                let Some(close) = rest[open..].find('"') else {
                    break;
                };
                quoted.push(rest[open..open + close].to_string());
                consumed = open + close + 1;
            }
            if quoted.len() == 2 {
                let end_byte = paren + consumed;
                refs.push(NamedQueryRef {
                    project: quoted[0].clone(),
                    path: quoted[1].clone(),
                    line: line_no as u32,
                    start: line[..paren].chars().count() as u32,
                    end: line[..end_byte].chars().count() as u32,
                });
                from = end_byte;
            } else {
                // No judgable call here — advance past this needle.
                from = paren;
            }
        }
    }
    refs
}

/// DIAGNOSTICS for one open doc, derived ONLY from the snapshot frame:
/// unknown provider references warn; tag paths absent from the cached
/// tree hint (with the age stamp); named-query paths not in the cached
/// list hint. Known-but-unpopulated trees (a failed section) stay
/// SILENT — no cached oracle means no verdict, never a guess. An
/// unhealthy snapshot publishes an empty set: from no truth flows no
/// diagnostics (the dead-gateway honesty in the contract suite).
fn diagnostics_for(
    snap: &Snapshot,
    uri: &lsp_types::Uri,
    text: &str,
    version: Option<i32>,
) -> PublishDiagnosticsParams {
    let mut diagnostics = Vec::new();
    if snap.healthy {
        let stamp = age_stamp(snap);
        for r in scan_references(text) {
            let known = snap.providers.contains(&r.provider);
            if !known {
                diagnostics.push(Diagnostic {
                    range: range_of(r.line, r.start, r.end),
                    severity: Some(DiagnosticSeverity::WARNING),
                    source: Some(LSP_SOURCE.into()),
                    message: format!("[{}] is not a known tag provider — {stamp}", r.provider),
                    ..Default::default()
                });
            } else if !r.path.is_empty() {
                let full = format!("[{}]{}", r.provider, r.path);
                if let Some(paths) = snap.tag_trees.get(&r.provider)
                    && !paths.contains(&full)
                {
                    diagnostics.push(Diagnostic {
                        range: range_of(r.line, r.start, r.end),
                        severity: Some(DiagnosticSeverity::HINT),
                        source: Some(LSP_SOURCE.into()),
                        message: format!(
                            "{full} is absent from the cached tag tree — {stamp} — \
                             save to request a refresh"
                        ),
                        ..Default::default()
                    });
                }
            }
        }
        for q in scan_named_query_calls(text) {
            if let Some(queries) = snap.named_queries.get(&q.project)
                && !queries.contains(&q.path)
            {
                diagnostics.push(Diagnostic {
                    range: range_of(q.line, q.start, q.end),
                    severity: Some(DiagnosticSeverity::HINT),
                    source: Some(LSP_SOURCE.into()),
                    message: format!(
                        "named query \"{}\" is not in the cached list for project \
                         \"{}\" — {stamp}",
                        q.path, q.project
                    ),
                    ..Default::default()
                });
            }
        }
    }
    PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics,
        version,
    }
}

fn range_of(line: u32, start: u32, end: u32) -> Range {
    Range {
        start: Position {
            line,
            character: start,
        },
        end: Position {
            line,
            character: end,
        },
    }
}

/// The named-query path a project-export member normalizes to, or None
/// when the member is not a named query. Real exports arrive as
/// `{collection}/resources/named-query/{path}/resource.json`, which
/// `resource_members` maps to `{collection}/named-query/{path}/
/// resource.json` — the query path is what follows the `named-query/`
/// segment (the resource-type directory, per the sibling repo's
/// project_scanner convention), minus the resource.json leaf.
fn named_query_path(user_path: &str) -> Option<String> {
    let idx = user_path.find("named-query/")? + "named-query/".len();
    let rest = &user_path[idx..];
    let rest = rest.strip_suffix("/resource.json").unwrap_or(rest);
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

// ---------------------------------------------------------------------------
// The gateway truth plane: TTL snapshot cache + background refresher
// ---------------------------------------------------------------------------

/// The refresh cadence — gateway truth is AT LEAST this stale for every
/// consumer (hover/diagnostic text carries the fetched-at age so the
/// staleness is stamped into the payload, Pitfall 4). A documented const:
/// UAT may tune it; the test pins the current value.
const LSP_CACHE_TTL: Duration = Duration::from_secs(30);

/// The project hosting the CLI's deployed WebDev routes — the same default
/// every `ign tags` verb uses (the routes live in that project by design,
/// `ign webdev deploy` deploys there). One const, one reason.
const TAGS_ROUTE_PROJECT: &str = "ign-cli";

/// An upper bound on tags browsed per provider per cycle — a pathological
/// tree (or a runaway recursive provider) cannot turn the refresher into
/// an infinite walker. The cap is a floor for honesty: a tree this big is
/// reported truncated, never silently cut.
const TAG_BROWSE_NODE_CAP: usize = 5_000;

/// One published frame of gateway truth. The ONLY shape LSP handlers may
/// read (SC-3): if a fact is not in here, a request cannot reach for it.
#[derive(Debug, Clone)]
struct Snapshot {
    /// Tag provider names (`[default]`, …) — the completion root.
    providers: Vec<String>,
    /// Provider → flattened tag paths (recursively browsed, properties
    /// excluded) — the completion tree and the tag-path diagnostic oracle.
    tag_trees: BTreeMap<String, Vec<String>>,
    /// Project → named-query resource paths (from the project export's
    /// member list) — completion for query references.
    named_queries: BTreeMap<String, Vec<String>>,
    /// Runnable project names.
    projects: Vec<String>,
    /// When this frame was fetched — the staleness stamp consumers show.
    fetched_at: Instant,
    /// True only when EVERY section of this frame fetched cleanly. A
    /// failed section keeps its PREVIOUS frame's data out — a partial
    /// snapshot beats none, and `healthy` says exactly how partial.
    healthy: bool,
}

impl Snapshot {
    /// The pre-first-publish frame: empty and explicitly unhealthy — no
    /// handler can mistake it for gateway truth.
    fn empty() -> Self {
        Self {
            providers: Vec::new(),
            tag_trees: BTreeMap::new(),
            named_queries: BTreeMap::new(),
            projects: Vec::new(),
            fetched_at: Instant::now(),
            healthy: false,
        }
    }
}

/// The TTL cache: `Arc<RwLock<Arc<Snapshot>>>` where the INNER Arc is the
/// handed-out frame. Readers clone the Arc under the lock (microseconds)
/// and then read without the lock; the refresher is the only writer.
/// The outer Arc lets the loop and the refresher thread share ONE cache.
#[derive(Clone)]
struct GatewayCache {
    inner: Arc<RwLock<Arc<Snapshot>>>,
}

impl GatewayCache {
    /// A cache seeded with [`Snapshot::empty`] — nothing is claimed as
    /// truth until the first publish.
    fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Arc::new(Snapshot::empty()))),
        }
    }

    /// THE read path: clone the current frame's Arc (the lock is held for
    /// the clone only) — handlers read the returned frame lock-free.
    fn snapshot(&self) -> Arc<Snapshot> {
        self.inner.read().expect("cache lock poisoned").clone()
    }

    /// THE write path (refresher only): swap the frame atomically. Readers
    /// holding the previous Arc keep a consistent old frame until they
    /// re-read — no torn reads, ever.
    fn publish(&self, snap: Snapshot) -> Arc<Snapshot> {
        let arc = Arc::new(snap);
        *self.inner.write().expect("cache lock poisoned") = Arc::clone(&arc);
        arc
    }
}

/// One populate cycle's raw sections. Each section is individually
/// fallible: a failed section stays empty and raises `failed_sections`
/// (partial snapshot beats none; `healthy` requires zero failures).
#[derive(Debug, Default, Clone)]
struct Collected {
    providers: Option<Vec<String>>,
    tag_trees: BTreeMap<String, Vec<String>>,
    named_queries: BTreeMap<String, Vec<String>>,
    projects: Option<Vec<String>>,
    failed_sections: u32,
}

/// The refresher's data seam — sync ON PURPOSE so the state-machine tests
/// inject fakes with no gateway and no runtime. Implementations run ONLY
/// on the refresher thread (never an LSP request); blocking here is
/// legal and expected.
trait SnapshotSource: Send + Sync + 'static {
    fn collect(&self) -> Collected;
}

/// Collapse one cycle's sections into a publishable frame: empty slots
/// where sections failed, `healthy` only when none did.
fn ingest(collected: &Collected) -> Snapshot {
    Snapshot {
        providers: collected.providers.clone().unwrap_or_default(),
        tag_trees: collected.tag_trees.clone(),
        named_queries: collected.named_queries.clone(),
        projects: collected.projects.clone().unwrap_or_default(),
        fetched_at: Instant::now(),
        healthy: collected.failed_sections == 0,
    }
}

/// One refresh cycle, PURE: collect → ingest → publish → announce. The
/// refresher thread calls it per cycle; the unit tests call it with fake
/// sources — identical code path either way.
fn refresh_once(
    source: &dyn SnapshotSource,
    cache: &GatewayCache,
    versions: &Sender<Arc<Snapshot>>,
) -> Arc<Snapshot> {
    let snap = ingest(&source.collect());
    let published = cache.publish(snap);
    // The diagnostics consumer may be gone mid-teardown; a failed
    // announce is a no-op, never an error.
    let _ = versions.send(Arc::clone(&published));
    published
}

/// The live source: Session is the ONE client construction site (CORE-09)
/// and every section rides machinery the CLI already trusts — the native
/// provider/project lists, the deployed tags route's browse action, and
/// the project export zip's member list (research OQ3, planner-locked).
/// No new endpoint surface.
struct GatewaySource {
    /// `None` while the session has never resolved (or resolution keeps
    /// failing) — every cycle re-attempts resolution through the same
    /// construction site until it succeeds; a failed attempt publishes
    /// an unhealthy frame and the thread lives on.
    session: Option<Session>,
    /// The refresher thread's current-thread runtime, OWNED here so
    /// `Runtime::block_on` drives its reactor. (A cloned `Handle` would
    /// NOT: `Handle::block_on` into an idle current-thread runtime does
    /// not run its I/O driver — the connect future would hang forever.
    /// The runtime is moved back in on re-resolve; the live sections'
    /// futures are only ever polled on the refresher thread, never an
    /// LSP request — SC-3.)
    runtime: tokio::runtime::Runtime,
}

impl GatewaySource {
    /// Resolve the effective profile — ambient flag (the global
    /// `--profile`, IGNITION_PROFILE already folded by
    /// apply_env_defaults) — then config. Failure is DATA here (degrade
    /// soft), unlike the command chassis where it is exit 3.
    fn resolve(profile_flag: Option<&str>, runtime: tokio::runtime::Runtime) -> Self {
        let session = match Session::resolve(profile_flag) {
            Ok(session) => {
                tracing::info!(profile = %session.profile_name(), "lsp: gateway session resolved");
                Some(session)
            }
            Err(err) => {
                tracing::warn!("lsp: gateway session unresolved (editing degrades offline): {err}");
                None
            }
        };
        Self { session, runtime }
    }

    /// The async half: the four gateway reads, per-section fallible.
    /// Runs on the refresher thread's runtime ONLY.
    async fn fetch_sections(&self) -> Collected {
        let mut collected = Collected::default();
        let Some(session) = &self.session else {
            // Unresolved session: all four sections fail this cycle.
            collected.failed_sections = 4;
            return collected;
        };
        let api = session.api();

        // Section 1: providers (the native list).
        match api.tag_provider_list(&ListQuery::default()).await {
            Ok(page) => {
                collected.providers = Some(page.items.into_iter().map(|p| p.name).collect())
            }
            Err(err) => {
                tracing::warn!("lsp: provider section failed: {err}");
                collected.failed_sections += 1;
            }
        }

        // Section 2: tag trees (the deployed tags route's browse action,
        // recursed per provider — the SAME route `ign tags browse` uses;
        // one version precondition per cycle, not per node).
        if let Err(err) =
            ignition_core::actions::webdev::webdev_precondition(api, TAGS_ROUTE_PROJECT).await
        {
            tracing::warn!(
                "lsp: tags route precondition failed (deploy with `ign webdev deploy`?): {err}"
            );
            collected.failed_sections += 1;
        } else if let Some(providers) = &collected.providers {
            let mut tag_trees = BTreeMap::new();
            for provider in providers {
                let (paths, truncated) = browse_provider(api, provider, TAG_BROWSE_NODE_CAP).await;
                if truncated {
                    tracing::warn!(
                        "lsp: provider {provider} browse hit the {TAG_BROWSE_NODE_CAP}-node cap — tree truncated this cycle"
                    );
                }
                tag_trees.insert(provider.clone(), paths);
            }
            collected.tag_trees = tag_trees;
        } else {
            collected.failed_sections += 1;
        }

        // Section 3: projects (the native list) — needed by section 4.
        match api.projects(&ListQuery::default()).await {
            Ok(page) => collected.projects = Some(page.items.into_iter().map(|p| p.name).collect()),
            Err(err) => {
                tracing::warn!("lsp: projects section failed: {err}");
                collected.failed_sections += 1;
            }
        }

        // Section 4: named queries per project — the project export zip's
        // member list, normalized through [`named_query_path`] (OQ3,
        // planner-locked; the CLI's own export path, no new endpoint).
        // Skipped (and counted) when the project list failed — there is
        // nothing to enumerate.
        match &collected.projects {
            Some(projects) => {
                let mut named_queries = BTreeMap::new();
                for project in projects {
                    match ignition_core::actions::resources::export_zip_bytes(api, project).await {
                        Ok(zip) => {
                            let queries = ignition_core::client::resources::resource_members(&zip)
                                .unwrap_or_default()
                                .iter()
                                .filter_map(|path| named_query_path(path))
                                .collect::<Vec<_>>();
                            named_queries.insert(project.clone(), queries);
                        }
                        Err(err) => {
                            tracing::warn!("lsp: named-query export for {project} failed: {err}");
                            collected.failed_sections += 1;
                        }
                    }
                }
                collected.named_queries = named_queries;
            }
            None => collected.failed_sections += 1,
        }

        collected
    }
}

impl SnapshotSource for GatewaySource {
    fn collect(&self) -> Collected {
        // `block_on` is legal HERE and ONLY here: the refresher thread
        // driving its OWN current-thread runtime, never an LSP request.
        // The LSP loop never touches a runtime (SC-3's structural
        // property).
        self.runtime.block_on(self.fetch_sections())
    }
}

/// Recursively browse one provider's tree over the deployed tags route —
/// the same `{"action":"browse","path":…}` call `ign tags browse` makes.
/// Returns the flattened tag paths (properties excluded) and whether the
/// node cap truncated the walk.
async fn browse_provider(
    api: &dyn ignition_core::client::GatewayApi,
    provider: &str,
    cap: usize,
) -> (Vec<String>, bool) {
    let mut paths = Vec::new();
    let mut queue = vec![format!("[{provider}]")];
    let mut truncated = false;
    while let Some(node) = queue.pop() {
        if paths.len() >= cap {
            truncated = true;
            break;
        }
        let body = serde_json::json!({"action": "browse", "path": node});
        let data = match api
            .webdev_route_call(TAGS_ROUTE_PROJECT, "tags", &body, &[])
            .await
        {
            Ok(data) => data,
            Err(err) => {
                tracing::warn!("lsp: browse {node} failed: {err}");
                continue;
            }
        };
        let Ok(entries) = serde_json::from_value::<Vec<ignition_core::client::tags::BrowseEntry>>(
            data["results"].clone(),
        ) else {
            tracing::warn!("lsp: browse {node} returned an unexpected shape");
            continue;
        };
        for entry in entries {
            if entry.tag_type == "Property" {
                continue;
            }
            if entry.has_children {
                queue.push(entry.full_path.clone());
            }
            paths.push(entry.full_path);
        }
    }
    (paths, truncated)
}

impl GatewayCache {
    /// Spawn the refresher thread: its OWN small current-thread runtime is
    /// built INSIDE the thread (the roadmap flag's resolution — `block_on`
    /// is legal THERE because it is not an LSP request), resolution rides
    /// [`GatewaySource::resolve`], the first populate publishes
    /// immediately, then one cycle per [`LSP_CACHE_TTL`]. Kicks are
    /// drained each cycle so a burst collapses to one refresh; every
    /// publish announces the new frame on the returned `versions`
    /// receiver (the loop's diagnostics arm consumes it). The channels
    /// are crossbeam so the version receiver can share the loop's
    /// `select!` with lsp-server's message receiver. The returned cache
    /// handle and the thread's share the ONE RwLock.
    fn spawn(profile_flag: Option<&str>, kicks: Receiver<()>) -> (Self, Receiver<Arc<Snapshot>>) {
        let (versions_tx, versions_rx) = crossbeam_channel::unbounded::<Arc<Snapshot>>();
        let cache = Self::new();
        let thread_cache = cache.clone();
        let profile_flag = profile_flag.map(str::to_string);
        let spawned = thread::Builder::new()
            .name("ign-lsp-refresher".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("lsp refresher runtime");
                let mut source = GatewaySource::resolve(profile_flag.as_deref(), runtime);
                loop {
                    // Re-attempt resolution through the SAME construction
                    // site while it keeps failing (a gateway that comes
                    // back is seen next cycle; the happy path holds one
                    // client for the server's lifetime). The runtime moves
                    // back into the rebuilt source — the source owns it.
                    if source.session.is_none() {
                        source = GatewaySource::resolve(profile_flag.as_deref(), source.runtime);
                    }
                    // Drain kicks: a didSave burst collapses into THIS
                    // cycle's one refresh.
                    while kicks.try_recv().is_ok() {}
                    refresh_once(&source, &thread_cache, &versions_tx);
                    thread::sleep(LSP_CACHE_TTL);
                }
            });
        // The thread failing to spawn must not kill the server: an
        // unstarted refresher publishes nothing, the cache stays empty +
        // unhealthy, and handlers answer from that honestly.
        if let Err(err) = spawned {
            tracing::error!("lsp: refresher thread failed to spawn (cache stays unhealthy): {err}");
        }
        (cache, versions_rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_are_narrow_by_design() {
        let caps = server_capabilities();
        assert!(caps.completion_provider.is_some(), "completion claimed");
        assert!(caps.hover_provider.is_some(), "hover claimed");
        assert!(
            matches!(
                caps.text_document_sync,
                Some(lsp_types::TextDocumentSyncCapability::Kind(
                    lsp_types::TextDocumentSyncKind::FULL
                ))
            ),
            "full-text sync claimed"
        );
        // The composition anti-pattern is a CAPABILITY claim: these three
        // must stay absent so ignition-lsp's statics surface is untouched.
        assert!(
            caps.definition_provider.is_none(),
            "definition never claimed"
        );
        assert!(
            caps.code_action_provider.is_none(),
            "codeAction never claimed"
        );
        assert!(
            caps.workspace_symbol_provider.is_none(),
            "workspace symbols never claimed"
        );
    }

    // ------------------------------------------------------------------
    // The pure handler layer (14-04 Task 1): three-family completions,
    // TTL-stamped hover, cached diagnostics — all over hand-built fake
    // snapshots, zero network anywhere.
    // ------------------------------------------------------------------

    /// A healthy fake frame carrying ALL THREE families: one provider,
    /// two of its tag paths, and one named query.
    fn full_frame() -> Snapshot {
        Snapshot {
            providers: vec!["default".into()],
            tag_trees: BTreeMap::from([(
                "default".into(),
                vec!["[default]Motor".into(), "[default]Motors/Speed".into()],
            )]),
            named_queries: BTreeMap::from([(
                "proj".into(),
                vec!["MyQuery".into(), "Folder/DeepQuery".into()],
            )]),
            projects: vec!["proj".into()],
            fetched_at: Instant::now(),
            healthy: true,
        }
    }

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    /// The populate filter's normalization, pinned BOTH directions: the
    /// real export shape (`{collection}/resources/named-query/{path}/
    /// resource.json` → user path `{collection}/named-query/{path}/
    /// resource.json`) lands the bare query path, and non-NQ members
    /// never pass.
    #[test]
    fn named_query_normalization_handles_the_real_export_shape() {
        assert_eq!(
            named_query_path("ignition/resources/named-query/MyQuery/resource.json"),
            Some("MyQuery".into()),
            "real export shape (post user_path)"
        );
        assert_eq!(
            named_query_path("ignition/resources/named-query/Folder/DeepQuery/resource.json"),
            Some("Folder/DeepQuery".into()),
            "folders ride"
        );
        assert_eq!(named_query_path("named-query/q"), Some("q".into()));
        assert_eq!(
            named_query_path("ignition/resources/views/Main/view.json"),
            None
        );
        assert_eq!(named_query_path("project.json"), None);
    }

    fn labels(items: &[CompletionItem]) -> Vec<&str> {
        items.iter().map(|item| item.label.as_str()).collect()
    }

    #[test]
    fn completion_merges_all_three_families() {
        let snap = full_frame();
        let items = completions(&snap, "// any doc", pos(0, 0));
        let ls = labels(&items);
        // Family (a): provider names.
        assert!(ls.contains(&"default"), "provider family present: {ls:?}");
        // Family (b): tag paths from the cached tree.
        assert!(
            ls.contains(&"[default]Motor") && ls.contains(&"[default]Motors/Speed"),
            "tag-path family present: {ls:?}"
        );
        // Family (c): named-query paths for known projects.
        assert!(
            ls.contains(&"MyQuery") && ls.contains(&"Folder/DeepQuery"),
            "named-query family present: {ls:?}"
        );
        // The kinds carry the family semantics.
        let provider = items.iter().find(|i| i.label == "default").unwrap();
        assert_eq!(provider.kind, Some(CompletionItemKind::MODULE));
        let tag = items.iter().find(|i| i.label == "[default]Motor").unwrap();
        assert_eq!(tag.kind, Some(CompletionItemKind::FILE));
        let query = items.iter().find(|i| i.label == "MyQuery").unwrap();
        assert_eq!(query.kind, Some(CompletionItemKind::FUNCTION));
    }

    /// Anti-sabotage co-pin: the merge is exactly three sources — this
    /// test reds if any family drops out (it checks the PRESENCE of
    /// each source's shape, not just the labels).
    #[test]
    fn completion_family_kinds_are_distinguishable() {
        let snap = full_frame();
        let items = completions(&snap, "", pos(0, 0));
        let modules = items
            .iter()
            .filter(|i| i.kind == Some(CompletionItemKind::MODULE))
            .count();
        let files = items
            .iter()
            .filter(|i| i.kind == Some(CompletionItemKind::FILE))
            .count();
        let functions = items
            .iter()
            .filter(|i| i.kind == Some(CompletionItemKind::FUNCTION))
            .count();
        assert_eq!((modules, files, functions), (1, 2, 2), "3 families");
    }

    #[test]
    fn completion_on_an_unhealthy_snapshot_is_empty_never_an_error() {
        let mut snap = full_frame();
        snap.healthy = false;
        let items = completions(&snap, "[default]Motor", pos(0, 8));
        assert!(items.is_empty(), "no truth flows from a dead frame");
        // The seed frame (spawn-failure path) behaves identically.
        let items = completions(&Snapshot::empty(), "[default]Motor", pos(0, 8));
        assert!(items.is_empty());
    }

    /// A closed `[provider]` bracket scopes the tag-path family to that
    /// provider; with no provider context, all providers' paths flow.
    #[test]
    fn completion_provider_context_filters_tag_paths() {
        let mut snap = full_frame();
        snap.providers.push("other".into());
        snap.tag_trees
            .insert("other".into(), vec!["[other]X".into()]);

        // Inside "[default]" — the other provider's paths stay out.
        let items = completions(&snap, "[default]", pos(0, 9));
        let ls = labels(&items);
        assert!(ls.contains(&"[default]Motor"), "own family in: {ls:?}");
        assert!(!ls.contains(&"[other]X"), "other provider out: {ls:?}");

        // Unterminated "[def" — honest simplicity: all paths flow.
        let items = completions(&snap, "[def", pos(0, 4));
        let ls = labels(&items);
        assert!(
            ls.contains(&"[default]Motor") && ls.contains(&"[other]X"),
            "no closed bracket ⇒ no filter: {ls:?}"
        );
    }

    /// Pitfall 4: staleness is stamped into the payload — a hover's
    /// contents name the match AND the snapshot's age against the TTL.
    #[test]
    fn hover_carries_the_age_stamp() {
        let snap = full_frame();
        let text = "go [default]Motor now";
        let h = hover(&snap, text, pos(0, 10)).expect("known tag path hovers");
        let HoverContents::Markup(markup) = h.contents else {
            panic!("hover contents are markup");
        };
        assert!(markup.value.contains("[default]Motor"), "{markup:?}");
        assert!(
            markup.value.contains("tag path"),
            "match description present: {markup:?}"
        );
        let re = simple_contains(&markup.value, "snapshot ");
        assert!(
            re && markup.value.contains("s old"),
            "age stamp: {markup:?}"
        );
        assert!(
            markup
                .value
                .contains(&format!("TTL {}s", LSP_CACHE_TTL.as_secs())),
            "TTL present: {markup:?}"
        );
        // The provider family hovers too, with its own description.
        let h = hover(&snap, "see [default] here", pos(0, 7)).expect("provider hovers");
        let HoverContents::Markup(markup) = h.contents else {
            panic!("hover contents are markup");
        };
        assert!(markup.value.contains("tag provider"), "{markup:?}");
        // And the named-query family.
        let h = hover(&snap, "q = MyQuery", pos(0, 6)).expect("query hovers");
        let HoverContents::Markup(markup) = h.contents else {
            panic!("hover contents are markup");
        };
        assert!(markup.value.contains("named query"), "{markup:?}");
    }

    fn simple_contains(hay: &str, needle: &str) -> bool {
        hay.contains(needle)
    }

    #[test]
    fn hover_misses_are_none() {
        let snap = full_frame();
        assert!(hover(&snap, "nothing here", pos(0, 3)).is_none());
        assert!(hover(&snap, "[bogus]Nope", pos(0, 6)).is_none());
        // Empty word (cursor on whitespace) — no lookup, no hover.
        assert!(hover(&snap, "a b", pos(0, 1)).is_none());
    }

    /// The word extractor spans the bracket/path alphabet: hovering mid-
    /// path yields the FULL qualified path, which is the snapshot key.
    #[test]
    fn word_at_spans_brackets_and_paths() {
        let text = "go [default]Motors/Speed ok";
        assert_eq!(word_at(text, 0, 10), "[default]Motors/Speed");
        // A trailing word edge absorbs left (hovering a word's end is
        // hovering that word); empty only between whitespace.
        assert_eq!(word_at(text, 0, 2), "go");
        assert_eq!(word_at("x   y", 0, 2), "");
        assert_eq!(word_at(text, 0, 4), "[default]Motors/Speed"); // mid-word
        assert_eq!(word_at("second [default]Motor", 0, 15), "[default]Motor");
    }

    #[test]
    fn scan_finds_bracket_references_with_spans() {
        let refs = scan_references("x [bogus] nope\nand [default]Motor/Speed");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].provider, "bogus");
        assert_eq!(refs[0].path, "");
        assert_eq!(refs[0].line, 0);
        assert_eq!((refs[0].start, refs[0].end), (2, 9));
        assert_eq!(refs[1].provider, "default");
        assert_eq!(refs[1].path, "Motor/Speed");
        assert_eq!(refs[1].line, 1);
        // An unterminated bracket judges nothing (no ']').
        assert!(scan_references("just [bogus").is_empty());
    }

    #[test]
    fn scan_finds_named_query_calls() {
        let refs = scan_named_query_calls(
            "system.db.runNamedQuery(\"proj\", \"MyQuery\", {})\nnothing\nrunNamedQuery(\"p2\", \"x/y\")",
        );
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].project, "proj");
        assert_eq!(refs[0].path, "MyQuery");
        assert_eq!(refs[0].line, 0);
        assert_eq!(refs[1].project, "p2");
        assert_eq!(refs[1].path, "x/y");
        assert_eq!(refs[1].line, 2);
        // One argument (or none) is not a judgable call.
        assert!(scan_named_query_calls("runNamedQuery(\"only\")").is_empty());
        assert!(scan_named_query_calls("runNamedQuery()").is_empty());
    }

    #[test]
    fn diagnostics_unknown_provider_warns_with_age_stamp() {
        let snap = full_frame();
        let uri = "file:///tmp/doc.txt".parse().expect("uri");
        let params = diagnostics_for(&snap, &uri, "see [bogus] please", Some(3));
        assert_eq!(params.version, Some(3));
        assert_eq!(params.diagnostics.len(), 1, "{params:?}");
        let d = &params.diagnostics[0];
        assert_eq!(d.severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            d.message.contains("[bogus] is not a known tag provider"),
            "{:?}",
            d.message
        );
        assert!(
            d.message.contains("snapshot 0s old"),
            "age stamped: {:?}",
            d.message
        );
        assert_eq!(d.source.as_deref(), Some(LSP_SOURCE));
        assert_eq!(d.range.start.character, 4);
        assert_eq!(d.range.end.character, 11);
    }

    #[test]
    fn diagnostics_absent_tag_path_hints_and_known_path_is_silent() {
        let snap = full_frame();
        let uri = "file:///tmp/doc.txt".parse().expect("uri");
        // Known path: no diagnostic.
        let params = diagnostics_for(&snap, &uri, "[default]Motor", None);
        assert!(params.diagnostics.is_empty(), "{params:?}");
        // Absent path: HINT with the age stamp.
        let params = diagnostics_for(&snap, &uri, "[default]Nope", None);
        assert_eq!(params.diagnostics.len(), 1, "{params:?}");
        let d = &params.diagnostics[0];
        assert_eq!(d.severity, Some(DiagnosticSeverity::HINT));
        assert!(
            d.message.contains("absent from the cached tag tree"),
            "{:?}",
            d.message
        );
        assert!(d.message.contains("s old"), "age stamped: {:?}", d.message);
    }

    #[test]
    fn diagnostics_named_query_miss_hints() {
        let snap = full_frame();
        let uri = "file:///tmp/doc.txt".parse().expect("uri");
        // Known query: silent. Unknown query under a known project: hint.
        let params = diagnostics_for(&snap, &uri, "runNamedQuery(\"proj\", \"MyQuery\")", None);
        assert!(params.diagnostics.is_empty(), "{params:?}");
        let params = diagnostics_for(&snap, &uri, "runNamedQuery(\"proj\", \"Missing\")", None);
        assert_eq!(params.diagnostics.len(), 1, "{params:?}");
        let d = &params.diagnostics[0];
        assert_eq!(d.severity, Some(DiagnosticSeverity::HINT));
        assert!(
            d.message.contains("\"Missing\"")
                && d.message.contains("\"proj\"")
                && d.message.contains("s old"),
            "{:?}",
            d.message
        );
        // A project the cache has never seen: no oracle, silent.
        let params = diagnostics_for(&snap, &uri, "runNamedQuery(\"ghost\", \"q\")", None);
        assert!(params.diagnostics.is_empty(), "{params:?}");
    }

    /// From an UNHEALTHY frame flows NO diagnostics: the dead-gateway
    /// honesty — no cached oracle means no verdict, never a wall of
    /// false "unknown provider" spam. (The contract suite pins this at
    /// the binary level too.)
    #[test]
    fn diagnostics_from_an_unhealthy_snapshot_are_silent() {
        let mut snap = full_frame();
        snap.healthy = false;
        let uri = "file:///tmp/doc.txt".parse().expect("uri");
        let params = diagnostics_for(
            &snap,
            &uri,
            "[bogus] [default]Nope runNamedQuery(\"proj\", \"Missing\")",
            None,
        );
        assert!(params.diagnostics.is_empty(), "{params:?}");
    }

    /// THE didOpen → publishDiagnostics wiring, over lsp-server's
    /// in-memory connection pair (no stdio, no threads beyond the test).
    #[test]
    fn did_open_publishes_diagnostics_from_cache() {
        let (server_side, client_side) = Connection::memory();
        let cache = GatewayCache::new();
        cache.publish(full_frame());
        let docs = &mut HashMap::new();
        let (kicks_tx, kicks_rx) = crossbeam_channel::unbounded::<()>();
        let params = serde_json::json!({
            "textDocument": {
                "uri": "file:///tmp/doc.txt",
                "languageId": "plaintext",
                "version": 1,
                "text": "see [bogus] here",
            }
        });
        handle_notification(
            Notification {
                method: "textDocument/didOpen".into(),
                params,
            },
            &cache,
            docs,
            &kicks_tx,
            &server_side,
        )
        .expect("didOpen handled");

        let msg = client_side
            .receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("a publish notification arrived");
        let Message::Notification(note) = msg else {
            panic!("expected a notification, got {msg:?}");
        };
        assert_eq!(note.method, "textDocument/publishDiagnostics");
        let published: PublishDiagnosticsParams = serde_json::from_value(note.params).unwrap();
        assert_eq!(published.version, Some(1));
        assert_eq!(published.diagnostics.len(), 1, "{published:?}");
        assert_eq!(
            published.diagnostics[0].severity,
            Some(DiagnosticSeverity::WARNING)
        );

        // didSave kicks the refresher (best-effort, never awaited).
        handle_notification(
            Notification {
                method: "textDocument/didSave".into(),
                params: serde_json::json!({"textDocument": {"uri": "file:///tmp/doc.txt"}}),
            },
            &cache,
            docs,
            &kicks_tx,
            &server_side,
        )
        .expect("didSave handled");
        kicks_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("the kick reached the refresher channel");

        // didClose removes the doc AND clears the client's view.
        handle_notification(
            Notification {
                method: "textDocument/didClose".into(),
                params: serde_json::json!({"textDocument": {"uri": "file:///tmp/doc.txt"}}),
            },
            &cache,
            docs,
            &kicks_tx,
            &server_side,
        )
        .expect("didClose handled");
        assert!(docs.is_empty(), "the doc left the map");
        let msg = client_side
            .receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("a clearing publish arrived");
        let Message::Notification(note) = msg else {
            panic!("expected a notification, got {msg:?}");
        };
        let published: PublishDiagnosticsParams = serde_json::from_value(note.params).unwrap();
        assert!(published.diagnostics.is_empty(), "cleared: {published:?}");
    }

    /// The request path composes docs-map + snapshot WITHOUT network:
    /// an unknown doc (never opened) answers empty/null, never an error.
    #[test]
    fn requests_on_unopened_docs_answer_empty_honestly() {
        let cache = GatewayCache::new();
        cache.publish(full_frame());
        let docs = HashMap::new();
        let req = Request::new(
            RequestId::from(10),
            "textDocument/completion".into(),
            serde_json::json!({
                "textDocument": {"uri": "file:///tmp/never-opened.txt"},
                "position": {"line": 0, "character": 0},
            }),
        );
        let resp = handle_request(req, &cache, &docs);
        let result = resp.response_result.expect("completion answered");
        assert_eq!(result, serde_json::json!([]));
        let req = Request::new(
            RequestId::from(11),
            "textDocument/hover".into(),
            serde_json::json!({
                "textDocument": {"uri": "file:///tmp/never-opened.txt"},
                "position": {"line": 0, "character": 0},
            }),
        );
        let resp = handle_request(req, &cache, &docs);
        let result = resp.response_result.expect("hover answered");
        assert_eq!(result, serde_json::Value::Null);
    }

    /// End-to-end over the handler entry points: an OPEN doc with all
    /// three families completes from the map's text; the response shape
    /// round-trips through lsp-server's Response.
    #[test]
    fn completion_request_answers_from_the_docs_map() {
        let cache = GatewayCache::new();
        cache.publish(full_frame());
        let docs = HashMap::from([(
            "file:///tmp/doc.txt".to_string(),
            ("see [default] here".to_string(), 1i32),
        )]);
        let req = Request::new(
            RequestId::from(12),
            "textDocument/completion".into(),
            serde_json::json!({
                "textDocument": {"uri": "file:///tmp/doc.txt"},
                "position": {"line": 0, "character": 13},
            }),
        );
        let resp = handle_request(req, &cache, &docs);
        let items: Vec<CompletionItem> =
            serde_json::from_value(resp.response_result.expect("answered")).unwrap();
        let ls = labels(&items);
        assert!(ls.contains(&"default"), "{ls:?}");
        assert!(ls.contains(&"[default]Motor"), "{ls:?}");
        assert!(ls.contains(&"MyQuery"), "{ls:?}");
    }

    #[test]
    fn unclaimed_methods_refuse_method_not_found() {
        let cache = GatewayCache::new();
        for method in [
            "textDocument/definition",
            "textDocument/codeAction",
            "workspace/symbol",
            "totally/bogus",
        ] {
            let req = Request::new(RequestId::from(3), method.into(), serde_json::Value::Null);
            let resp = handle_request(req, &cache, &HashMap::new());
            let err = resp.response_result.expect_err("unclaimed method refuses");
            assert_eq!(err.code, -32601, "{method} refuses MethodNotFound");
        }
    }

    // ------------------------------------------------------------------
    // The gateway truth plane (Task 2): swap semantics, TTL pin,
    // degrade-soft state machine
    // ------------------------------------------------------------------

    /// A publishable frame with just the fields a test cares about.
    fn frame(providers: &[&str], healthy: bool) -> Snapshot {
        Snapshot {
            providers: providers.iter().map(|s| (*s).to_string()).collect(),
            tag_trees: BTreeMap::new(),
            named_queries: BTreeMap::new(),
            projects: Vec::new(),
            fetched_at: Instant::now(),
            healthy,
        }
    }

    /// A fake populate result: everything fetched cleanly.
    fn healthy_sections() -> Collected {
        Collected {
            providers: Some(vec!["default".into()]),
            tag_trees: BTreeMap::from([(
                "default".into(),
                vec!["[default]T1".into(), "[default]P5".into()],
            )]),
            named_queries: BTreeMap::from([("proj".into(), vec!["named-query/q".into()])]),
            projects: Some(vec!["proj".into()]),
            failed_sections: 0,
        }
    }

    /// A fake populate result: the gateway is unreachable — the live
    /// source's unresolved-session shape (all four sections failed).
    fn unreachable_sections() -> Collected {
        Collected {
            failed_sections: 4,
            ..Default::default()
        }
    }

    /// A fake populate result: ONE section failed (providers ok, tags
    /// not) — partial snapshot beats none, healthy says how partial.
    fn partial_sections() -> Collected {
        Collected {
            providers: Some(vec!["default".into()]),
            failed_sections: 1,
            ..Default::default()
        }
    }

    struct FakeSource(Collected);

    impl SnapshotSource for FakeSource {
        fn collect(&self) -> Collected {
            self.0.clone()
        }
    }

    #[test]
    fn ttl_const_is_pinned_at_thirty_seconds() {
        // The documented cadence — changing this needs a conscious edit
        // here (UAT may tune; the pin makes the change deliberate).
        assert_eq!(LSP_CACHE_TTL, Duration::from_secs(30));
    }

    #[test]
    fn snapshot_swap_semantics_readers_see_old_until_publish() {
        let cache = GatewayCache::new();

        // Before the first publish: empty + explicitly unhealthy — no
        // handler can mistake the seed for gateway truth.
        let seed = cache.snapshot();
        assert!(!seed.healthy);
        assert!(seed.providers.is_empty());

        // Publish frame A; a reader clones the current Arc.
        cache.publish(frame(&["default"], true));
        let held = cache.snapshot();
        assert_eq!(held.providers, ["default"]);
        assert!(held.healthy);

        // Publish frame B: the live view moves, the reader's clone does
        // not — no torn reads, ever.
        cache.publish(frame(&["other"], false));
        let now = cache.snapshot();
        assert_eq!(now.providers, ["other"]);
        assert!(!now.healthy);
        assert_eq!(held.providers, ["default"], "held readers keep frame A");
        // The staleness stamp is monotonic across publishes — 14-04's
        // hover text renders this age (Pitfall 4), so the pin guards it.
        assert!(held.fetched_at <= now.fetched_at);
    }

    #[test]
    fn refresh_once_announces_the_published_version() {
        let cache = GatewayCache::new();
        let (tx, rx) = crossbeam_channel::unbounded();
        let published = refresh_once(&FakeSource(healthy_sections()), &cache, &tx);
        assert_eq!(cache.snapshot().providers, published.providers);
        let announced = rx.recv().expect("publish announces its version");
        assert_eq!(announced.providers, published.providers);
    }

    /// THE anti-sabotage pin, both directions: a failing source publishes
    /// an UNHEALTHY frame (editing offline degrades, never lies), and the
    /// same machinery RECOVERS to healthy the cycle the source succeeds —
    /// the server survives the outage and returns to truth.
    #[test]
    fn degrade_soft_failure_then_recovery() {
        let cache = GatewayCache::new();
        let (tx, _rx) = crossbeam_channel::unbounded();

        // Sabotage: gateway unreachable → healthy:false, no fake data.
        let degraded = refresh_once(&FakeSource(unreachable_sections()), &cache, &tx);
        assert!(!degraded.healthy, "failure must mark the frame unhealthy");
        assert!(degraded.providers.is_empty());
        let live = cache.snapshot();
        assert!(!live.healthy);
        assert!(live.providers.is_empty());

        // Restore: the same cache, a succeeding source → healthy:true,
        // the sections land.
        let recovered = refresh_once(&FakeSource(healthy_sections()), &cache, &tx);
        assert!(recovered.healthy, "recovery must restore healthy");
        assert_eq!(recovered.providers, ["default"]);
        assert_eq!(
            recovered.tag_trees.get("default").map(Vec::as_slice),
            Some(&["[default]T1".to_string(), "[default]P5".to_string()][..])
        );
        assert_eq!(
            recovered.named_queries.get("proj").map(Vec::as_slice),
            Some(&["named-query/q".to_string()][..])
        );
        assert_eq!(recovered.projects, ["proj"]);
        let live = cache.snapshot();
        assert!(live.healthy);
        assert_eq!(live.providers, ["default"]);
    }

    /// One failing section never zeroes the others: providers survive,
    /// the failed section is empty, and healthy is false (all-sections-ok
    /// is the only healthy).
    #[test]
    fn partial_snapshot_keeps_the_surviving_sections() {
        let cache = GatewayCache::new();
        let (tx, _rx) = crossbeam_channel::unbounded();
        let partial = refresh_once(&FakeSource(partial_sections()), &cache, &tx);
        assert!(!partial.healthy, "one failed section ⇒ not healthy");
        assert_eq!(partial.providers, ["default"], "surviving section kept");
        assert!(partial.tag_trees.is_empty(), "failed section empty");
        assert!(partial.named_queries.is_empty());
        assert!(partial.projects.is_empty());
    }

    /// The pre-first-publish seed cannot masquerade as truth: empty and
    /// unhealthy by construction (the refresher-spawn failure path leans
    /// on this — an unstarted refresher leaves this frame in place).
    #[test]
    fn seed_frame_is_empty_and_unhealthy() {
        let snap = Snapshot::empty();
        assert!(!snap.healthy);
        assert!(snap.providers.is_empty());
        assert!(snap.tag_trees.is_empty());
        assert!(snap.named_queries.is_empty());
        assert!(snap.projects.is_empty());
    }
}
