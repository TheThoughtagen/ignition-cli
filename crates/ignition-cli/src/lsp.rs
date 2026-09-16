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
//! thread — a plain `for msg in &connection.receiver` with NO runtime, NO
//! block_on, and NO network anywhere on the request path (SC-3's
//! structural half).
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

use std::process::ExitCode;

use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::{
    CompletionOptions, HoverProviderCapability, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind,
};

/// Serve LSP over stdio. The OutOfBand exit seam: SUCCESS on a clean
/// client-driven shutdown (or protocol-loop end), FAILURE with the error
/// on stderr for any protocol death. Nothing here reads config or the
/// network on the request path — the ambient profile flag rides to the
/// GatewayCache refresher thread (the TTL snapshot, next task), whose
/// failures degrade the snapshot instead of the server.
pub fn serve(profile_flag: Option<&str>) -> ExitCode {
    match run(profile_flag) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!("lsp: protocol error: {err}");
            ExitCode::FAILURE
        }
    }
}

/// The sync dispatch loop. Shape is FINAL — 14-04's handlers only fill
/// function bodies.
fn run(profile_flag: Option<&str>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // The flag feeds the refresher thread (GatewayCache, next task);
    // the protocol scaffold itself touches no config and no network.
    let _ = profile_flag;

    let (connection, io_threads) = Connection::stdio();
    let caps = serde_json::to_value(server_capabilities())?;
    let _init_params = connection.initialize(caps)?;
    tracing::info!("lsp: handshake complete — serving over stdio");

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                // The shutdown handshake is lsp-server's too; `true` here
                // means "client sent shutdown" — answer it, then exit the
                // loop cleanly (the client follows with `exit`).
                if connection.handle_shutdown(&req)? {
                    tracing::info!("lsp: shutdown handshake complete");
                    return Ok(());
                }
                let resp = handle_request(req);
                connection.sender.send(Message::Response(resp))?;
            }
            Message::Notification(note) => {
                // Spec: a bare `exit` notification terminates the server.
                // (`initialized` is consumed by connection.initialize.)
                if note.method == "exit" {
                    return Ok(());
                }
                handle_notification(note);
            }
            // We never call the client, so a Response cannot arrive; the
            // arm exists for Message exhaustiveness.
            Message::Response(_) => {}
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

/// Request dispatch. Cache-only (SC-3): the bodies below read the TTL
/// snapshot and return — zero network, zero runtime. Stubs in 14-03;
/// 14-04 fills the bodies against the GatewayCache.
fn handle_request(req: Request) -> Response {
    match req.method.as_str() {
        "textDocument/completion" => {
            Response::new_ok(req.id, Vec::<lsp_types::CompletionItem>::new())
        }
        "textDocument/hover" => Response::new_ok(req.id, serde_json::Value::Null),
        other => method_not_found(req.id, other),
    }
}

/// Notification dispatch — no-ops in 14-03 (the GatewayCache's kick
/// channel arrives with the refresher; 14-04 wires didSave to it).
fn handle_notification(_note: Notification) {}

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

    #[test]
    fn completion_stub_returns_an_empty_list() {
        let req = Request::new(
            RequestId::from(1),
            "textDocument/completion".into(),
            serde_json::Value::Null,
        );
        let resp = handle_request(req);
        assert_eq!(resp.id, RequestId::from(1));
        let result = resp
            .response_result
            .expect("completion stub answers without error");
        assert_eq!(result, serde_json::Value::Array(vec![]));
    }

    #[test]
    fn hover_stub_answers_null() {
        let req = Request::new(
            RequestId::from(2),
            "textDocument/hover".into(),
            serde_json::Value::Null,
        );
        let resp = handle_request(req);
        let result = resp
            .response_result
            .expect("hover stub answers without error");
        assert_eq!(result, serde_json::Value::Null);
    }

    #[test]
    fn unclaimed_methods_refuse_method_not_found() {
        for method in [
            "textDocument/definition",
            "textDocument/codeAction",
            "workspace/symbol",
            "totally/bogus",
        ] {
            let req = Request::new(RequestId::from(3), method.into(), serde_json::Value::Null);
            let resp = handle_request(req);
            let err = resp.response_result.expect_err("unclaimed method refuses");
            assert_eq!(err.code, -32601, "{method} refuses MethodNotFound");
        }
    }
}
