# Phase 14: 14-transports-mcp-lsp - Research

**Researched:** 2026-09-15
**Domain:** Protocol transports over stdio — hand-rolled JSON-RPC 2.0 MCP server (`ign mcp serve`) + LSP server (`ign lsp`) over the stable command surface, fed by the Phase-8 `Session` core
**Confidence:** HIGH (every code anchor below was read out of the tree on this date; every protocol fact cites the official MCP 2025-06-18 spec or docs.rs; stack choices extend the milestone-level STACK.md research, re-verified today)

No CONTEXT.md exists (no `/gsd-discuss-phase`). There are no locked user decisions. Per the 09-phase precedent, the **roadmap's own flags are treated as directives** and are quoted verbatim where they constrain design. The two directives that shape everything:

> "MCP slice: transport decision settled by stack research (hand-rolled, not rmcp); protocolVersion pin '2025-06-18' is an implementation-time live smoke test against Claude Code/Claude Desktop, not research-phase."

> "LSP slice needs verification of the sync dispatch loop + in-process tokio `block_on` pattern under the existing tracing setup, plus scripted-client harness design."

---

## Summary

Phase 14 adds two **hidden clap subcommands that are OutOfBand runtime modes** — `ign mcp serve` and `ign lsp` — each owning stdout completely, each never touching the `render_ok`/`render_error` stdout path. The pattern already exists twice in the tree: `ign tui` (the `TuiExited` intercept, main.rs:401-404) and `ign edit` (the `dispatch_edit` main-level seam, main.rs:424-437). The 08-06 reservation pre-declared the `mcp`/`lsp` OutOfBand rows and the contract is exact: **their registry rows land atomically with their clap commands** (adding rows early = orphans; the clap walk refuses them by design — routes.rs:434-445, tui_coverage.rs:44-49). Neither transport builds a second client: both resolve auth through `ignition-core::Session` (08-02; `resolve`/`for_url`, session.rs:98-217) and invoke the existing `dispatch(cli, mode)` in-process.

**MCP** is hand-rolled JSON-RPC 2.0 on `serde_json` + tokio (the STACK.md decision, re-verified: rmcp is now **3.4.0** — it moved 3.2 → 3.4 in the eleven days since the stack research, demonstrating exactly the churn the hand-roll avoids). The protocol surface is small and fully specified: `initialize` → `notifications/initialized` → `ping`/`tools/list`/`tools/call` over **newline-delimited JSON on stdio** (spec: messages "MUST NOT contain embedded newlines"; server "MUST NOT write anything to stdout that is not a valid MCP message" — the existing stderr-only tracing invariant, main.rs:3284-3297, is the load-bearing precondition). The tool catalog derives from a `Cli::command()` walk — the exact walk already shipped in `tui_coverage.rs:64-82` — with tool schemas from clap's Arg reflection, so catalog drift is structurally impossible. The one piece the clap tree **cannot** supply is which verbs are `--yes`-guarded (`--yes` is a global arg; the guards live as `require_confirmation(yes, "<operation>")` literals in dispatch arms) — that set needs a single-source const registry, and this is the phase's main new design obligation (§ Architecture, Pattern 3).

**LSP** rides `lsp-server` 0.10 + `lsp-types` 0.97 (both re-verified current today; lsp-server is rust-analyzer's own scaffold, 5 deps, no tokio, sync crossbeam dispatch loop — we own the loop). Gateway truth enters through a **TTL-cached snapshot** written by a background thread and read lock-only inside LSP requests — "no blocking network call inside an LSP request" becomes a structural property (requests never touch the runtime; only the refresher does). Composition with Python `ignition-lsp` is nvim-side dual-client: the verified `find_lsp_server()` chain (lsp.lua:93-130) returns one command for one client today; the IDE-03 patch registers a second client (`ign lsp`) beside the statics server — nvim merges completions across attached clients and aggregates diagnostics, so both servers coexist by design. The roadmap's "one-line sibling-repo patch" is optimistic against the verified anatomy (~5-8 lines for a second `vim.lsp.config` + start path, or one line if the setup is first refactored to loop over a config list); plan honestly for the small patch, not the literal one-liner.

**Primary recommendation:** Land MCP first as the pattern-prover (framing, catalog, confirm gate, concurrency, byte-scan harness), then have LSP reuse the conventions — shared scripted-binary test chassis, shared Session/cache resolution, shared OutOfBand row landing — with the two loops intentionally different (MCP async/tokio for non-starving ping; LSP sync/crossbeam with a block_on-free cache).

---

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `serde_json` | workspace (1.0, `raw_value` feature on) | All JSON-RPC message + envelope serialization | Already the wire format owner; compact `to_string` is the MCP framing primitive |
| `tokio` | workspace 1.53 (io-std/io-util/sync/rt-multi-thread already enabled) | MCP server runtime: stdin reader task, per-call task spawn, mpsc writer | Features already declared for exactly these purposes (Cargo.toml comments) |
| `lsp-server` | **0.10.0** (2026-07-16, rust-lang/rust-analyzer) | LSP base protocol: Content-Length framing, handshake, IO threads; we own the dispatch loop | rust-analyzer's own scaffold; 5 deps, no tokio, sync loop is the verified pattern for cache-only handlers (STACK.md pick, re-verified on crates.io today) |
| `lsp-types` | **0.97.0** (current newest = default) | LSP data model: ServerCapabilities, CompletionItem, Hover, Diagnostic, PublishDiagnosticsParams | The ecosystem types crate lsp-server is designed to pair with; capability set needed is 3.16/3.17-era, fully covered |
| `clap` (derive) | workspace 4.6 line (4.6.7 latest) | Catalog derivation via `Cli::command()` reflection | `CommandFactory::command()` already used by completions + tui_coverage walk; reflection API (`get_subcommands`/`get_arguments`/`is_hide_set`/`is_subcommand_required_set`) verified on 4.6.7 docs |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `crossbeam-channel` | 0.5.15 (transitive via lsp-server) | lsp-server's internal message channels | Never directly — the Connection channels suffice |
| `assert_cmd` / `predicates` / `tempfile` | workspace dev | Scripted-client harnesses spawning the REAL binaries | Both transport contract suites (precedent: contract_stdout_purity.rs, contract_edit.rs) |
| Python `mcp` SDK (test-time only, via `uv run --with mcp`) | current PyPI | MCP conformance oracle — a real FastMCP **client** drives `ign mcp serve` | Optional live/e2e gate; same genre as the existing Python WebDev harness. NOT a code dependency |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Hand-rolled JSON-RPC (MCP) | `rmcp` 3.4.0 (official Rust SDK) | Rejected (STACK.md, re-verified): rmcp moved 3.2.0 → 3.4.0 between 2026-09-04 and today — recurring-churn SDK; stdio-server feature set drags chrono/schemars/uuid/indexmap/tokio-util/futures (~8-10 crates; chrono is on the v1.0 what-NOT-use list); typed-parameter-first ergonomics fight a passthrough-catalog model. **Sanctioned escalation path if the surface ever needs prompts/resources/sampling/streamable-HTTP** (STACK.md:73) |
| `lsp-server` (LSP) | `tower-lsp-server` 0.23 fork / `async-lsp` | Rejected: tower-lsp dead (no release since 2023); the fork drags tower/dashmap/httparse plus a 0.0.x-versioned `ls-types` with no semver guarantees; async-lsp is fine but async machinery buys nothing when handlers are cache-only (STACK.md:81-84). Hand-rolling LSP framing (~150 lines) was judged defensible but strictly worse than the maintained rust-lang scaffold |
| `Cli::try_parse_from` dispatch (recommended) | Direct `actions::*` invocation | ARCHITECTURE.md offered both; `try_parse_from` wins here because the catalog AND schemas derive from the same clap tree, making "no hand-written catalog" a structural fact rather than a discipline. Falls out of "MCP catalog derives from the clap tree and structurally cannot drift" (roadmap Depends-on note) |
| nvim dual-client composition | `ign lsp` proxying/statics-delegating to ignition-lsp | Proxy would merge two servers' responses inside `ign` — enormous complexity, duplicates nvim's native multi-client merge. nvim attaches both clients and merges (completions concat, diagnostics aggregate) |

**Installation:**

```toml
# crates/ignition-cli/Cargo.toml — the ONLY additions (ungated per STACK.md:88;
# combined tree cost is lsp-server + lsp-types + transitive crossbeam-channel)
lsp-server = "0.10"
lsp-types = "0.97"
# MCP: zero new dependencies (serde_json + tokio already in the tree)
```

---

## Architecture Patterns

### Recommended Project Structure

```
crates/ignition-cli/src/
├── mcp.rs              # NEW — `ign mcp serve`: JSON-RPC framing, catalog builder,
│                       #   dispatch bridge, concurrent loop (tokio)
├── lsp.rs              # NEW — `ign lsp`: lsp-server loop, GatewayCache (TTL
│                       #   snapshot + background refresher thread), handlers
├── cli.rs              # MCommand additions: Mcp (hide = true), Lsp (hide = true)
└── main.rs             # Two new dispatch seams beside Edit's (before the chassis);
                        # GUARDED_OPS const if the confirm registry lands here
crates/ignition-cli/tests/
├── contract_mcp.rs     # NEW — scripted MCP client over the real spawned binary
└── contract_lsp.rs     # NEW — scripted LSP client over the real spawned binary
~/whiskeyhouse/ignition-nvim/
└── lua/ignition/lsp.lua        # IDE-03: second client registration (sibling repo)
```

### Pattern 1: OutOfBand runtime mode (the `tui`/`edit` precedent, third application)

**What:** a hidden clap subcommand that never returns through the envelope-render chassis; its clap `routes()` row lands in the SAME commit as the command (08-06 contract, fulfilled for `edit` in 13-08 — the pinned set becomes `["api call", "completions", "edit", "mcp", "lsp"]`).
**When to use:** both new verbs. Important nuance: unlike `edit` (zero stdout), these modes' stdout IS the product (the protocol stream) — OutOfBand means "not the envelope render path," not "silent."
**Example (verified, main.rs:424-437):**

```rust
// main() — the Edit seam precedent; mcp/lsp seams sit beside it:
if let Commands::Edit(args) = cli.command {
    let profile_flag = cli.profile.clone();
    let yes = cli.yes;
    return runtime.block_on(dispatch_edit(args, profile_flag.as_deref(), yes, mode));
}
// 14: same shape for Commands::Mcp(_) and Commands::Lsp(_) —
// each returns its own ExitCode and never reaches render_ok/render_error.
```

Clap declaration (hidden, mirroring the roadmap's "hidden `ign mcp`" intent):

```rust
/// Serve the Model Context Protocol over stdio (hidden; for MCP clients)
#[command(subcommand)]
Mcp(McpArgs),          // McpArgs { serve } — `ign mcp serve`

/// Serve LSP over stdio (hidden; for ignition-nvim)
Lsp,
```

### Pattern 2: MCP message surface and concurrency (spec-verified)

**What:** newline-delimited JSON-RPC 2.0 on stdio; the reader loop answers `initialize`/`ping` inline and **spawns every `tools/call` onto the tokio runtime**, routing responses through an mpsc to a single writer task. Responses may arrive out of order — JSON-RPC id matching makes that legal, and it is what makes SC-5's "ping never starves behind a gateway call" true by construction.
**Wire facts (all from the official 2025-06-18 spec, fetched 2026-09-15):**

```jsonc
// 1. initialize request → server MUST respond with same protocolVersion
//    if supported (else its latest); then client sends notifications/initialized
{ "jsonrpc": "2.0", "id": 1, "method": "initialize",
  "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "...", "version": "..." } } }
// server result: { protocolVersion, capabilities: { tools: {} }, serverInfo: { name, version }, instructions? }

// 2. ping — "MUST respond promptly with an empty response" (the starvation constraint)
{ "jsonrpc": "2.0", "id": "123", "method": "ping" }
// → { "jsonrpc": "2.0", "id": "123", "result": {} }

// 3. tools/list → { tools: [ { name, title?, description, inputSchema, annotations? } ], nextCursor? }
//    (static catalog: return everything, ignore the cursor param gracefully)

// 4. tools/call → result { content: [ { type: "text", text: <ENVELOPE JSON VERBATIM> } ], isError }
```

**Error mapping:** unknown tool / invalid params → JSON-RPC **protocol errors** (`-32601`, `-32602`); everything a dispatched command produces — success envelope OR failure envelope — is a **tool result**, with `isError: true` exactly when the envelope says `ok: false` (spec: tool execution errors incl. "business logic errors" ride `isError`, not JSON-RPC errors). The confirm refusal is therefore `isError: true` + the `confirmation_required` failure envelope as content — "returns the refusal envelope as the tool result" (SC-2).

**Concurrency skeleton:**

```rust
// mcp.rs — shape only; framing helpers + catalog feed this loop
let (resp_tx, resp_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
// writer task: sole owner of stdout; one serde_json::to_string line per message
tokio::spawn(async move { while let Some(line) = resp_rx.recv().await { writeln!(out, "{line}") } });
loop {
    let line = reader.next_line().await?;          // stdin, newline-delimited
    match parse_message(&line)? {
        Msg::Ping(id)        => respond_empty(id), // inline — never queued
        Msg::Initialize(id, p) => negotiate_and_respond(id, p),
        Msg::ToolsList(id)   => respond_catalog(id),
        Msg::ToolsCall(id, call) => {
            let tx = resp_tx.clone();
            tokio::spawn(async move {                 // off the reader path
                tx.send(render_tool_result(execute(call)).await);
            });
        }
        Msg::Notification(m) if m == "notifications/initialized" => {}
        Msg::UnknownMethod(id) => protocol_error(id, -32601),
    }
}
```

### Pattern 3: Catalog + confirm-field derivation (the phase's one genuinely new mechanism)

**What:** the tool catalog is computed by walking `Cli::command()`; schemas come from clap Arg reflection; the catalog builder and the parity test share one function, so the CI test is trivially green when nothing drifts and red the moment clap and catalog diverge.

**Verified walk (already in-tree, tui_coverage.rs:64-82) — reuse this exact recursion:**

```rust
// Source: crates/ignition-cli/tests/tui_coverage.rs (the compiled-truth walk)
fn walk(cmd: &clap::Command, prefix: &str, out: &mut Vec<String>) {
    for sub in cmd.get_subcommands() {
        if sub.get_name() == "help" { continue; }
        let path = if prefix.is_empty() { sub.get_name().to_string() }
                   else { format!("{prefix} {}", sub.get_name()) };
        let has_real_children = sub.get_subcommands().any(|c| c.get_name() != "help");
        if !has_real_children || !sub.is_subcommand_required_set() {
            out.push(path.clone());
        }
        walk(sub, &path, out);
    }
}
```

The MCP builder extends it: skip `is_hide_set()` commands, skip the curated-exclusion set (`completions`, `mcp`, `lsp`, `tui` — self-referential or non-agentic; recommend also excluding `edit`, whose product is a child-$EDITOR terminal handoff and is meaningless as a tool call), and for each leaf emit:

- **name:** the path with spaces → `_` (e.g. `tags_browse`, `project_delete`) — JSON-schema-safe, stable;
- **description:** the clap doc-comment (`get_about()`) — already written for agents;
- **inputSchema:** `get_arguments()` → properties (positional → `string` property named by id; long flags → properties; `SetTrue` flags → `boolean`; help text → `description`; `get_possible_values()` → `enum`; `is_required_set()` → `required` array);
- **`confirm`:** for every guarded verb, a synthetic `"confirm": {"type": "boolean"}` property appended to `required`.

**The gap the clap tree cannot fill:** `--yes` is a GLOBAL arg (cli.rs:38-40), so no per-verb clap attribute marks guardedness — the guards are `require_confirmation(yes, "project delete")`-style literals scattered through dispatch (main.rs:2972+; ~10 call sites). The catalog therefore needs a **single-source registry**, and the anti-drift discipline must cover it:

```rust
// ONE home, referenced by BOTH the catalog builder and (ideally, after a small
// mechanical refactor) the dispatch arms' require_confirmation calls:
pub(crate) const GUARDED_OPS: &[(&str, &str)] = &[
    // (clap leaf path, operation prose used in the refusal)
    ("project delete", "project delete"),
    ("gateway restart", "gateway restart"),
    // … the full --yes set, enumerated once …
];
// Drift pin: a test walks require_confirmation call sites' literals (or the
// refactor makes that vacuous) and asserts set equality with GUARDED_OPS.
```

**Execution bridge (SC-1's "structurally cannot drift" chain):**

```rust
// tools/call handler — tool args become CLI tokens, one dispatch path:
fn execute(call: ToolCall) -> (String /*envelope*/, bool /*isError*/) {
    let mut argv = vec!["ign".into()];
    argv.extend(call.path.split(' ').map(str::to_string));
    if call.confirm == Some(true) { argv.push("--yes".into()); }
    for (k, v) in call.arguments { argv.extend(flag_tokens(k, v)); }
    let mut cli = match Cli::try_parse_from(argv) {
        Ok(c) => c,
        Err(e) => return (invalid_params_envelope(e), true), // NEVER e.exit():
    };                                                       // clap prints + exits mid-protocol
    // SC-2 hard gate BEFORE anything else: confirm is the ONLY --yes source here.
    // Do NOT run apply_env_defaults — IGNITION_YES=1 in the agent's shell would
    // otherwise bypass the confirm field and violate the success criterion verbatim.
    let mode = RenderMode::CompactJson;
    let (profile, result) = rt.block_on(dispatch(cli, mode));
    let envelope = match result {
        Ok(out)  => out.render_json(profile.as_deref(), true),
        Err(err) => err.envelope(profile.as_deref()).serialized_compact(),
    };
    (envelope, !envelope_starts_with_ok_true(&envelope))
}
```

### Pattern 4: LSP sync loop + TTL cache (the roadmap-flag verification, resolved)

**What:** `lsp-server` hands back a `Connection` (pair of crossbeam channels) after handling framing + handshake; **we** write the sync dispatch loop on the main thread. Gateway calls never happen inside a handler: a background thread refreshes a `GatewayCache` snapshot every TTL period; handlers take a read lock over an `Arc` snapshot and return.

**Verified API surface (docs.rs, lsp-server 0.10.0 — official example, abridged):**

```rust
// Source: https://docs.rs/lsp-server/latest/lsp_server/struct.Connection.html
let (connection, io_threads) = Connection::stdio();

let server_capabilities = serde_json::to_value(&ServerCapabilities {
    completion_provider: Some(CompletionOptions { trigger_characters: Some(vec!["/".into(), ".".into()]), ..Default::default() }),
    hover_provider: Some(HoverProviderCapability::Simple(true)),
    text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncType::FULL)),
    // Narrow surface BY DESIGN (composition): Python ignition-lsp owns
    // definition/codeAction/workspace symbols — never claim them here.
    ..Default::default()
})?;
let _init_params = connection.initialize(server_capabilities)?;  // handshake done

// The tokio runtime exists for the REFINER (and MCP share); the LSP loop
// itself never block_on's inside a request — the cache is the interface:
let cache = GatewayCache::spawn(profile_flag, ttl);   // own thread; Session::resolve once
for msg in &connection.receiver {
    match msg {
        Message::Request(req) => {
            if connection.handle_shutdown(&req)? { return Ok(()); }
            let resp = handle_request(req, &cache);   // RwLock read ONLY — zero network
            connection.sender.send(Message::Response(resp))?;
        }
        Message::Notification(n) => handle_notification(n, &cache, &connection),
        Message::Response(_) => {}
    }
}
io_threads.join()?;
```

**GatewayCache shape:** `{ providers: Vec<ProviderSummary>, tag_trees: BTreeMap<String, TagTree>, named_queries: BTreeMap<String /*project*/, Vec<String>>, projects: Vec<String>, fetched_at: Instant, healthy: bool }` behind `Arc<RwLock<Arc<Snapshot>>>` (read path clones the `Arc` snapshot — lock held microseconds). The refresher resolves `Session::resolve` once at startup (degrade-soft on failure: cache reports `healthy = false` and keeps retrying — editing offline must not kill the server) and then loops `interval.tick() → populate → publish`. After each refresh (and on `didOpen`/`didChange`), diagnostics publish **from cache** for open documents (e.g. unknown provider, tag path absent from the cached browse tree, named-query path not in the cached list).

**In-process runtime note (the flag's `block_on` question):** the LSP loop needs the tokio runtime ONLY inside the refresher thread (which is free to `block_on` — it is not an LSP request). Handlers stay pure-sync. This is cleaner than block_on-per-request and satisfies "no blocking network call inside an LSP request" structurally. The existing `init_tracing` (stderr writer) is reused verbatim — stderr is also the LSP convention for logs, and `RUST_LOG=lsp_server=debug` gives message-level tracing for free (crate docs).

### Pattern 5: Byte-scan purity for both protocols (SC-5)

**What:** extends `contract_stdout_purity.rs`'s assert-based (never snapbox — overwrite-proof) discipline to the framing layer:
- **MCP:** every stdout byte must form lines that each parse as exactly one JSON-RPC message (newline-delimited, no embedded newlines). The scripted client's `readline → serde_json::from_slice` IS the byte-scan: one stray byte fails the parse.
- **LSP:** every stdout byte must be valid `Content-Length: N\r\n\r\n`-framed payload of exactly N bytes. The scripted client implements the base-protocol reader and asserts the byte count exactly — a stray `tracing` byte fails the length check.
Both harnesses run under `IGNITION_LOG=trace` + unknown-key config noise (the purity-harness noise recipe, contract_stdout_purity.rs:10-26) and strip ambient `IGNITION_*` knobs.

### Anti-Patterns to Avoid

- **`Cli::try_parse_from` error → `e.exit()`**: clap prints usage and terminates the process — a protocol death. Always translate parse errors to JSON-RPC `-32602` responses.
- **Pretty JSON on the MCP wire**: `serde_json::to_string_pretty` embeds newlines — spec forbids embedded newlines in stdio messages. Compact serialization only (`to_string`). (LSP Content-Length framing tolerates newlines inside payloads; the constraint is MCP-specific.)
- **`apply_env_defaults` in the MCP path**: it merges `IGNITION_YES` (main.rs:2948-2950); an agent shell with that exported would turn a missing `confirm` field into a confirmed write — violating SC-2's "omitting it returns the refusal envelope" verbatim. Protocol dispatch sets `cli.yes` solely from the confirm field.
- **Blocking gateway call inside an LSP handler**: violates SC-3 and freezes nvim's UI on a dead gateway. Cache read-only handlers; the refresher owns the network.
- **Claiming definition/codeAction/workspace-symbols capabilities in `ign lsp`**: those are ignition-lsp's statics (verified server.py registers TEXT_DOCUMENT_DEFINITION, CODE_ACTION, WORKSPACE_SYMBOL). Doubling them makes nvim's "go to definition" answer from two servers and degrades the composition.
- **Out-of-band rows before clap commands exist**: `routes()` rows for `mcp`/`lsp` added before their subcommands would fail the bidirectional clap walk as orphans (the 08-06 design). Rows land in the same commit as the commands.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| LSP base protocol (Content-Length framing, handshake, shutdown) | A hand-written framing loop | `lsp-server` 0.10 | 808 lines of battle-tested rust-lang code owning exactly the fiddly part; `Connection::memory()` even gives an in-memory connection pair for unit tests |
| LSP data model | Ad-hoc serde structs for LSP messages | `lsp-types` 0.97 | The spec's type surface is enormous (CompletionItem, Hover, Diagnostic…); hand-rolling any of it is silent-incompatibility bait |
| Tool JSON Schemas | Hand-written schema per tool | clap Arg reflection in the catalog builder | Schemas from the tree cannot drift from the tree — the entire SC-1 mechanism |
| MCP conformance testing | A bespoke protocol test matrix only | Python `mcp` SDK client as an oracle (test-time, `uv run --with mcp`) | A real client (the FastMCP stack ignition-mcp already used) exercises initialize/negotiation/cancellation like production consumers will |
| Config→client resolution | A new profile path in either mode | `ignition-core::Session` (shipped 08) | Both modes get identical auth/overlay semantics; a second path violates CORE-09 |
| Confirmation semantics | A new MCP-specific refusal path | `require_confirmation` + the frozen failure envelope | The refusal envelope as tool result IS the spec of SC-2; no new error type, no new slug |
| Diagnostics timing | A per-request network fetch | The TTL snapshot + post-refresh publish | SC-3's constraint; also the only design that survives a dead gateway gracefully |

**Key insight:** this phase's genuinely custom code is small — MCP framing (~300-500 lines per STACK.md) and two dispatch loops. Everything else is composition: the catalog is the clap tree, the tool results are the frozen envelopes, the LSP data is the Session's client, the nvim integration is nvim's own multi-client merge.

---

## Common Pitfalls

### Pitfall 1: The confirm-field set can drift from the `--yes` guards
**What goes wrong:** the catalog advertises `confirm` on the wrong verbs (or misses one) — an agent either refuses to perform a guarded write or, worse, performs an unguarded one thinking it's guarded.
**Why it happens:** `--yes` is global; guardedness lives in dispatch-arm literals, invisible to the clap walk.
**How to avoid:** `GUARDED_OPS` const (Pattern 3) as the single source; dispatch arms consult it (or a test pins the literal sites against it); the catalog builder reads only the const.
**Warning signs:** a catalog/contract test comparing `GUARDED_OPS` against the ~10 `require_confirmation` call sites failing after any new guarded verb.

### Pitfall 2: Ping starvation behind a slow gateway call
**What goes wrong:** a tools/call against a dead/slow gateway blocks the reader loop; the MCP client times the connection out and kills the server ("MAY terminate the connection" on ping timeout — spec).
**Why it happens:** naive single-threaded dispatch executes tools inline.
**How to avoid:** tools/call spawns onto the runtime; ping/initialize answered on the reader path (Pattern 2); single writer task serializes stdout.
**Warning signs:** scripted-client test that pings DURING an in-flight dead-port tools/call and expects the ping reply before the tool result — keep this test in contract_mcp.rs permanently.

### Pitfall 3: A stray stdout byte kills the MCP client
**What goes wrong:** Claude Code / any client sees a non-message line and aborts the connection (spec: server MUST NOT write non-MCP bytes to stdout).
**Why it happens:** pretty-printed envelope (embedded newlines), a `println!` debug, or a crate writing to stdout; clap's parse-error printer.
**How to avoid:** compact serialization only; never `e.exit()`; byte-scan harness under max noise (Pattern 5); existing stderr-only tracing invariant is the base.
**Warning signs:** the framing validator failing on `IGNITION_LOG=trace` runs.

### Pitfall 4: Gateway-truth staleness surprises (LSP)
**What goes wrong:** completions/diagnostics disagree with the gateway after edits made elsewhere (Designer, another `ign` session).
**Why it happens:** TTL cache is doing its job; consumers weren't told.
**How to avoid:** stamp staleness into the payload — hover/diagnostic text carries the fetched-at age (roadmap: "TTL-stamped"); refresh cadence documented; didSave can trigger a best-effort refresh kick (still never awaited by a request).
**Warning signs:** UAT complaint of "ghost" tag paths; missing age stamp in hover text.

### Pitfall 5: nvim composition regressions
**What goes wrong:** the second client (ign lsp) fails to start when `ign` isn't on PATH, or doubles capabilities and breaks go-to-definition.
**Why it happens:** detection-order patch assumes `ign` exists; capability overlap.
**How to avoid:** the nvim patch checks `vim.fn.executable('ign')` before registering (mirroring the existing venv/exepath checks, lsp.lua:97-114); server caps limited to completion/hover/diagnostics (Pattern 4).
**Warning signs:** `:LspInfo` showing two clients both answering hover; IDE-03's end-to-end verification in the sibling repo is the gate.

### Pitfall 6: Golden/registry blast radius
**What goes wrong:** adding two clap leaves breaks the TUI-coverage walk and the pinned OutOfBand test in the same CI run as everything else.
**Why it happens:** the walk is bidirectional by design.
**How to avoid:** land `cli.rs` command + `routes.rs` rows + pinned-test entries as ONE atomic change (the 13-07/13-08 precedent: "routes-rows-land-with-clap atomicity"); expect `gui`-unaffected goldens — mcp/lsp have no render arms.
**Warning signs:** `every_row_requiring_cli_node_is_mapped_and_no_orphans` red.

---

## Code Examples

### MCP catalog entry (derived)

```jsonc
// tools/list entry for `ign project delete` — everything but "confirm"
// is clap-derived at runtime; nothing here is hand-written:
{
  "name": "project_delete",
  "description": "Delete a project — destructive, refused without --yes",
  "inputSchema": {
    "type": "object",
    "properties": {
      "name":    { "type": "string", "description": "Project name" },
      "confirm": { "type": "boolean", "description": "Explicit confirmation for this destructive operation (the --yes translation)" }
    },
    "required": ["name", "confirm"]
  }
}
```

### Envelope-as-tool-result (SC-2, both outcomes)

```jsonc
// confirm omitted on project_delete → isError result carrying the FROZEN refusal envelope:
{ "jsonrpc": "2.0", "id": 7,
  "result": { "isError": true, "content": [{ "type": "text", "text":
    "{\"ok\":false,\"profile\":\"dev\",\"error\":{\"code\":\"confirmation_required\",\"message\":\"project delete requires confirmation\",\"endpoint\":null,\"hint\":\"re-run with --yes (or confirm: true) to proceed\"}}" }] } }

// success → the success envelope, verbatim, isError absent/false:
{ "jsonrpc": "2.0", "id": 8,
  "result": { "content": [{ "type": "text", "text":
    "{\"ok\":true,\"profile\":\"dev\",\"data\":{…}}" }] } }
```

### Scripted-client harness core (both contract suites)

```rust
// tests/contract_mcp.rs — shape; lsp variant swaps readline for Content-Length framing
struct ScriptedClient { child: std::process::Child, lines: Receiver<String> }

impl ScriptedClient {
    fn spawn() -> Self {
        let (_dir, config) = isolated_config();           // purity-harness recipe:
        let mut cmd = Command::cargo_bin("ign").unwrap(); //   tempdir config + ambient
        cmd.env("IGNITION_CLI_CONFIG", &config)           //   IGNITION_* stripping +
        cmd.env("IGNITION_LOG", "trace");                 //   IGNITION_LOG=trace noise
        let mut child = cmd.args(["mcp", "serve"]).stdin(piped()).stdout(piped()).spawn().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {                      // reader thread; recv_timeout
            let mut line = String::new();                 // prevents hangs on protocol death
            let mut out = std::io::BufReader::new(child.stdout.take().unwrap());
            while out.read_line(&mut line).unwrap_or(0) > 0 {
                // THE BYTE-SCAN: one stray byte fails this parse
                serde_json::from_str::<serde_json::Value>(&line).expect("stdout carried a non-JSON-RPC byte");
                tx.send(line.clone()).unwrap(); line.clear();
            }
        });
        Self { child, lines: rx }
    }
    fn expect_response(&self, id: i64, secs: u64) -> serde_json::Value { /* recv_timeout loop */ }
}
```

### The nvim-side patch (verified anatomy to modify)

```lua
-- ~/whiskeyhouse/ignition-nvim/lua/ignition/lsp.lua — verified anchors:
--   find_lsp_server() (lsp.lua:93-130): returns ONE cmd — venv → exepath → dev venv
--   M.setup (lsp.lua:16-91): registers ONE vim.lsp.config('ignition_lsp') + ONE FileType autocmd
-- Composition: register a second config and start both in the autocmd, e.g.:
if vim.fn.executable('ign') == 1 then
  vim.lsp.config('ignition_live', {
    cmd = { 'ign', 'lsp' },
    root_markers = { 'project.json' },
    filetypes = { 'ignition', 'python', 'ignition_expr' },
  })
end
-- …and start 'ignition_live' beside 'ignition_lsp' in the FileType callback.
-- nvim merges completions across attached clients and aggregates per-client
-- diagnostics, so statics (ignition-lsp) and live truth (ign lsp) compose
-- without either server knowing about the other.
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `ignition-mcp` (Python FastMCP, 37 tools, own REST client) | `ign mcp serve` as thin shim over the frozen CLI envelope | This phase | One gateway client (Session), one auth chain, zero second surface to maintain; the Python tool catalog was the coverage reference, not the shape |
| tower-lsp as the default Rust LSP choice | tower-lsp **dead** (0.20.0, 2023-08-11); `lsp-server` (rust-lang) is the maintained narrow scaffold | Verified 2026-07-16 release; fork `tower-lsp-server` exists but drags a 0.0.x types crate | lsp-server 0.10 + own sync loop is the current lean path |
| MCP spec revision pinning | **2025-06-18** is the current served revision (modelcontextprotocol.io, fetched 2026-09-15) | roadmap pre-pinned it | Server responds with the pinned version string; negotiation rule (same-if-supported) is one `if` |
| rmcp 3.2.0 (stack-research baseline) | rmcp **3.4.0** today | moved within 11 days | Confirms the fast-moving-SDK assessment; hand-roll decision holds all the more |

**Deprecated/outdated:**
- `tower-lsp`: unmaintained since 2023 — do not adopt even for familiarity's sake.
- MCP `HTTP+SSE` transport (2024-11-05): replaced by Streamable HTTP — irrelevant here (stdio only), noted to prevent cargo-culting old examples.
- `apply_env_defaults` in protocol paths: fine for the human CLI; must NOT gate confirm in MCP mode (Pitfall 1/anti-pattern).

---

## Open Questions

1. **Guarded-op registry shape** — a const table vs. extending `require_confirmation` call sites to consult it.
   - What we know: ~10 literal call sites (main.rs), `--yes` global arg hides guardedness from clap.
   - Recommendation: const `GUARDED_OPS` + a drift test pinning it against the call-site literals (or the small mechanical refactor of call sites reading the const — cleaner, slightly wider blast radius). Planner's call.
2. **Tool surface size and `--json`-mode verbs** — roughly 90+ row-requiring leaves exist today; the catalog will be large for tools/list.
   - What we know: spec allows returning the full list without pagination (cursor is optional); envelopes are compact JSON.
   - Recommendation: full static catalog, no cursor support; verify tools/list latency is fine (it's a serde serialize of ~100 entries). Planner may curate further (e.g. drop `profile` family?) — default to include-everything-uncurated-except the exclusion list for structural honesty.
3. **Named-query enumeration path** — export-based (`project_export` → `resource_members`, the `resource list` machinery) vs. the REST resources-list route.
   - What we know: named queries are project resources under `named-query/` prefixes (verified in the sibling repo's `project_scanner.py`); the workspace/13-03 machinery already lists project resources.
   - Recommendation: ride the export/`resource_members` path the CLI already trusts (no new endpoint surface); measure cost on the rig at implementation time.
4. **`ign lsp` profile selection UX** — nvim's `cmd = {'ign','lsp'}` carries no `--profile`.
   - Recommendation: ambient resolution (IGNITION_PROFILE env → config `active`), plus an optional `--profile` global that nvim power users can append in their config. No new mechanism.
5. **protocolVersion live smoke** (roadmap-flagged as implementation-time): Claude Code / Claude Desktop against the real server. Not resolvable in research; the negotiation code must handle "client requests an older version" by echoing it (spec rule), and the live gate records the result.

---

## Sources

### Primary (HIGH confidence)
- Official MCP spec, revision 2025-06-18 (fetched 2026-09-15): `/basic/lifecycle` (initialize shape, version negotiation, shutdown), `/basic/transports` (stdio framing rules: newline-delimited, no embedded newlines, stdout purity MUSTs, stderr logging), `/server/tools` (tools/list, tools/call, isError vs protocol errors, inputSchema, structured-content guidance), `/basic/utilities/ping` (prompt empty response; timeout ⇒ stale)
- docs.rs `lsp-server` 0.10.0 — crate page, `Connection` page incl. the official `initialize_start`/`initialize` examples (API verified verbatim; 5-dep tree verified in the docs header)
- crates.io API: `lsp-server` (0.10.0, 2026-07-16, rust-lang owner team), `lsp-types` (0.97.0 current), `rmcp` (3.4.0 current — alternatives table)
- docs.rs `clap::Command` 4.6.7 — reflection methods verified (`get_subcommands`, `get_arguments`, `is_hide_set`, `is_subcommand_required_set`, `get_name`)
- In-tree (read 2026-09-15): `cli.rs` (Commands enum, global args), `main.rs` (main/seams/dispatch/init_tracing/apply_env_defaults/require_confirmation), `render.rs` (render_ok/render_error + sanctioned exceptions), `ignition-core/src/session.rs`, `error.rs` (ErrorEnvelope LOCKED shape), `ignition-tui/src/routes.rs` (OutOfBand reservation), `tests/tui_coverage.rs` (the walk), `tests/contract_stdout_purity.rs` (the harness), `tests/cli_chassis.rs` (spawn conventions), Cargo.toml (tokio features, rust-version 1.88, edition 2024)

### Secondary (MEDIUM confidence)
- `~/whiskeyhouse/ignition-nvim` — `lua/ignition/lsp.lua` (find_lsp_server chain + single-client setup, read in full), `lsp/ignition_lsp/server.py` (pygls 2.0 capability surface: definition/codeAction/workspace symbols owned by statics), `lsp/ignition_lsp/project_scanner.py` (named-query path convention), `UPGRADE_LSP.md` — sibling-repo facts as of today; the repo may move independently
- nvim 0.11+ multi-client completion merge / per-client diagnostics aggregation — standard documented behavior, not re-verified against the nvim manual in this session (IDE-03's end-to-end verification in the sibling repo is the binding gate)

### Tertiary (LOW confidence / implementation-time)
- Claude Code / Claude Desktop interop behavior with a hand-rolled stdio server — roadmap explicitly defers this to the implementation-time live smoke; negotiation code is written spec-first
- Exact rmcp escalation-path ergonomics if the surface ever grows — STACK.md's pointer suffices; not re-researched

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — every version verified against crates.io/docs.rs today; hand-roll decisions extend the already-researched STACK.md with fresh confirmation
- Architecture: HIGH — patterns are compositions of verified in-tree precedents (tui/edit OutOfBand seams, tui_coverage walk, Session seam, purity harness) plus spec-quoted protocol shapes
- Pitfalls: HIGH for code-grounded ones (env-yes bypass, orphan rows, framing), MEDIUM for nvim composition details (sibling-repo facts; live gate covers)
- Open questions: 5, all with recommendations; none block planning

**Research date:** 2026-09-15
**Valid until:** ~2026-10-15 (protocol specs and chosen crates are slow-moving; rmcp version noted only as an alternative. Re-verify lsp-server/lsp-types if planning slips past a month.)
