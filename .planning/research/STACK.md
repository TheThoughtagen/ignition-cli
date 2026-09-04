# Stack Research: ignition-cli v1.1 Milestone Additions

**Domain:** Incremental stack additions to an existing validated Rust CLI/TUI (ignition-cli v1.0 shipped 2026-08-30, 3-crate workspace, ~73,900 lines, lean-tree philosophy)
**Researched:** 2026-09-04
**Confidence:** HIGH on crate versions (all verified against crates.io API 2026-09-04, rmcp manifest verified on GitHub `main`, MCP spec fetched from modelcontextprotocol.io) · MEDIUM on the MCP hand-roll-vs-SDK verdict (spec facts HIGH; recommendation is judgment)
**Workspace floor:** `rust-version = 1.88`, `edition = 2024`, resolver 3

---

## Executive Verdict (read this first)

The v1.1 feature list needs only **three new direct dependencies**:

| Crate | Version | Feature | Serves |
|-------|---------|---------|--------|
| `quick-xml` | `"0.42"` | `["serialize"]` | Tag-provider bulk transfer XML (EXT bulk transfer) |
| `csv` | `"1.4"` | — | Tag-provider bulk transfer CSV (EXT bulk transfer) |
| `lsp-server` | `"0.10"` | — | IDE-02: LSP mode (`ign lsp`) |
| `lsp-types` | `"0.97"` | — | IDE-02: protocol types (pairs with lsp-server) |

Everything else is either **already in the workspace graph** (reqwest for EXT-01/02/03, toml/serde/directories for TUIX-01, zip/tempfile for IDE-03, tempfile+std for IDE-01, ratatui 0.30.2 `Style`/`Color` for theming) or — the headline decision — **hand-rolled for EXT-04 MCP: zero new crates**.

The big call: **MCP transport mode is a hand-rolled JSON-RPC 2.0 shim (~300–500 lines, `serde_json` + tokio only), NOT the official `rmcp` SDK.** The official SDK is excellent and active (rmcp 3.2.0, updated 2026-08-31, MSRV exactly 1.88), but its stdio-server feature set transitively drags in `chrono` (non-optional on native targets!), `schemars`, `uuid`, `pastey`, `indexmap`, `tokio-util`, and `futures` — 8–10 new crates, and `chrono` is on this project's own v1.0 What-NOT-Use list. The v1.0 precedent (docker compose shell-out over `bollard`; serde+toml over `config`/`figment`) and the milestone's own "thin shim over the stable JSON contract" language both point the same way. rmcp is documented below as the sanctioned escalation path if the MCP surface ever grows beyond tools-only.

---

## Recommended Stack

### New Direct Dependencies (complete list — nothing else)

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `quick-xml` | `"0.42"`, features `["serialize"]` | Read/write Ignition tag-provider export XML | The consensus Rust XML library (398M downloads, updated 2026-08-22, MSRV 1.86 ✓). Streaming pull-parser + Writer covers both directions of bulk transfer; serde bridge (`serialize` feature) maps tag structs. Only mandatory dep is `memchr` — the leanest real option by a wide margin |
| `csv` | `"1.4"` | Tag-provider bulk transfer CSV | BurntSushi's canonical CSV crate (238M downloads, 1.4.0 stable since 2025-10, MSRV 1.73 ✓). Deps: `csv-core`, `itoa`, `ryu`, `serde_core` — no tokio, no serde_json pull. Serde row-mapping is exactly the bulk-transfer model |
| `lsp-server` | `"0.10"` | IDE-02: LSP transport scaffold for `ign lsp` | rust-analyzer's own scaffold, maintained by the rust-lang team (updated 2026-07-16, 15M downloads). Sync crossbeam-channel design: `Connection::stdio()`, framing/handshake handled, dispatch loop owned by us. Total deps: `crossbeam-channel`, `log`, `serde`, `serde_derive`, `serde_json` — **no tokio, no tower** |
| `lsp-types` | `"0.97"` | IDE-02: LSP 3.17 protocol types | The standard types crate (343M downloads). "Stale" since 2024-06 but that's fine: LSP 3.17-era types cover completion/diagnostics/hover/didChange completely; ignition-nvim needs nothing newer |

### Already-in-Graph — No Action Required

| v1.1 Feature | Existing Crates That Cover It | Integration Note |
|--------------|-------------------------------|------------------|
| EXT-01 `ign api call` raw REST passthrough | `reqwest 0.13` (json, query, stream already enabled) | New clap verb → `ignition-core::GatewayApi` extension or a raw-request escape method; response JSON passes through the envelope untouched. Zero deps |
| EXT-02 diagnostics commands (license, bundle, redundancy, GAN) | `reqwest 0.13`, `serde_json` | All read-path REST + existing exit taxonomy. The bundle command assembles downloaded artifacts — `zip` (already direct in core) if the bundle is an archive |
| EXT-03 EAM write operations | `reqwest 0.13` | Same `GatewayApi` seam as v1.0's guarded EAM; extend with unguarded verbs + `--yes` guards. Zero deps |
| TUIX-01 per-profile polling cadence | `toml 1.1`, `serde`, `directories 6.0` | Add a field to the profile struct in `ignition-core`; TUI reads it. Zero deps |
| TUI theming | `ratatui 0.30.2` (`Style`, `Color`, `Stylize`) | Hand-rolled palette: a `Theme` struct in `ignition-tui` mapping semantic roles → ratatui styles, selected by config. All ratatui theme crates on crates.io are micro-projects (≤2.5k downloads — verified 2026-09-04); do not add one |
| IDE-01 `ign edit` round-trip | `std::process::Command`, `tempfile 3.27` (already direct in ignition-core) | Spawn `$VISUAL` → `$EDITOR` → `vi` fallback on a `tempfile::Builder` temp file **with a suffix** (`.py` for script resources, `.xml`/`.json` per resource type) so editors apply syntax highlighting. Compare mtime + size after editor exit to detect no-op. Zero deps |
| IDE-03 workspace checkout | `zip 8.6`, `tempfile`, `tokio::fs`, existing diff/sync actions | Pure fs orchestration of proven v1.0 machinery. **No file watching** — checkout/sync is command-driven by design; do not add `notify` |
| Tag↔historian binding closure | `reqwest 0.13`, `serde_json`, existing Designer-diff path | Gateway API work, not stack work |

### MCP (EXT-04): Hand-Rolled JSON-RPC Shim — Zero New Crates

**Recommendation:** implement `ign mcp serve` as a stdio MCP server hand-rolled on `serde_json` + tokio (io-std/io-util features already enabled in the workspace tokio).

**Why the official SDK is not the pick here (verified facts, not vibes):**

- `rmcp` **is** the official Rust MCP SDK (modelcontextprotocol/rust-sdk) — 3.2.0, updated 2026-08-31, 24.2M downloads, Apache-2.0, MSRV 1.88 (exactly matches our floor). It is *not* being rejected on quality grounds.
- Verified against its GitHub `main` manifest: the `server` feature (which the `default` feature set includes) **forcibly enables** `schemars` (+ its `chrono04` feature), `uuid`, `pastey`, and `transport-async-rw` (`tokio-util/codec`). Separately, `chrono` is a **non-optional** dependency on all native targets. Net new tree for a stdio server: `chrono`, `schemars`, `uuid`, `pastey`, `indexmap`, `tokio-util`, `futures`, `pin-project-lite` (± a few) — 8–10 crates, one of which (`chrono`) this project explicitly rejected in v1.0 ("prefer `jiff` if/when datetime is actually needed").
- rmcp's ergonomics are typed-parameter-first: `#[tool]` macros want `schemars`-derivable Rust parameter structs. Our tools are *passthrough wrappers* around the frozen `ign` CLI surface — a schema-from-derive model is an awkward fit; we'd fight the framework to express "arguments are the CLI's own arg model".
- rmcp has moved 0.x → 1.x → 2.x → 3.x within roughly 18 months; riding a fast-moving SDK inside a lean-tree project means recurring churn for capabilities (streamable HTTP, OAuth, elicitation, tasks) a tools-only stdio shim will never use.

**Why hand-rolling is safe here (spec-grounded, modelcontextprotocol.io, spec 2025-06-18):**

- stdio transport = newline-delimited JSON-RPC 2.0; messages MUST be UTF-8, no embedded newlines; stderr is free for logs; client launches the server as a subprocess. The full lifecycle for a tools-only server is: `initialize` → `notifications/initialized` → `tools/list` → `tools/call` (+ `ping`, and `notifications/cancelled` / `$/…` notifications which the spec says to ignore). That's ~4 methods and 2 notifications.
- Version negotiation is server-tolerant: respond with our pinned `protocolVersion` (e.g. `"2025-06-18"`); spec defines client-side downgrade behavior. No negotiation logic needed.
- Testing precedent exists: the project already ships a Python WebDev harness; the official Python `mcp` SDK (FastMCP client — the stack being *replaced*) makes a perfect conformance test oracle, alongside live tests against Claude Code / ignition-mcp consumers.

**The two rules that make-or-break the shim (write them into the plan):**

1. **stdout purity** — spec: server MUST NOT write anything to stdout that is not a valid MCP message. The child `ign` process's JSON must be *captured* (tokio `process` feature already enabled) and re-emitted inside the `tools/call` response payload, never piped through.
2. **Unknown-method tolerance** — any `$/`-prefixed or unrecognized notification gets a silent drop, not an error response.

**Sanctioned escalation path:** if the MCP surface ever needs prompts, resources, elicitation, sampling, or streamable-HTTP transport, switch to `rmcp = { version = "3.2", features = ["server", "transport-io", "macros"] }` (default already carries `server` + `macros` + `base64`; add `transport-io` for stdio). Re-evaluate then; the current milestone doesn't need any of it.

### LSP (IDE-02): `lsp-server` + `lsp-types` — Ecosystem State Late-2025/2026

The Rust LSP-server field, verified against crates.io 2026-09-04:

| Candidate | Version | Last updated | Verdict |
|-----------|---------|--------------|---------|
| `tower-lsp` | 0.20.0 | **2023-08-11** | **Dead.** Three years without a release. Ruled out |
| `tower-lsp-server` (community fork of tower-lsp) | 0.23.0 | 2025-12-07 | Alive but wrong shape: deps include `tower`, `dashmap`, `httparse`, `memchr`, `bytes` **plus its own `ls-types 0.0.6`** — a 0.0.x-versioned types crate with no semver guarantees. Heavier and riskier than the rust-lang option for zero capability gain |
| `async-lsp` | 0.2.4 | 2026-04-24 | Actively maintained, tower-middleware architecture, but pinned to `lsp-types ^0.95` (two majors old) and drags `tower-layer`/`tower-service`. Middleware layers are overkill for a single-purpose gateway-fed server |
| **`lsp-server`** | **0.10.0** | **2026-07-16** | **The pick.** rust-analyzer's scaffold, owned by the rust-lang/rust-analyzer team. 5 deps total, no tokio. Sync dispatch loop is a *feature* here: gateway calls via the existing reqwest-backed core run under a small in-process tokio runtime (`Runtime::block_on` — standard pattern, `rt` features already enabled), while the LSP loop stays on plain threads |

**Integration shape:** `ign lsp` subcommand in the `ignition-cli` crate (like `ign mcp serve`). Completion sources = gateway browse/UDT definitions via `ignition-core`; diagnostics = `ign lint` delegation + existing tag validation. Capability set needed is 3.16/3.17-era: `textDocument/completion`, `didOpen`/`didChange`, `publishDiagnostics`/`textDocument/diagnostic`. `lsp-types 0.97` covers all of it.

**Feature-gating:** add the two crates ungated to `ignition-cli` — combined tree cost is 3 crates (`lsp-server`, `lsp-types`, `crossbeam-channel`), trivially small next to `tui`'s ratatui/crossterm weight. If the default binary size matters later, gate behind `lsp = ["dep:lsp-server", "dep:lsp-types"]` mirroring the existing `tui` feature; not required up front.

### Bulk Transfer Serialization Notes

- **XML (tag provider format):** Ignition 8's tag XML is attribute-heavy and element-nested. `quick-xml` serde derive handles the regular parts (`#[serde(rename = "@attrName")]` for attributes); expect hand-rolled `Event`-loop code for the irregular tag-definition corners rather than forcing full derive coverage. Writing uses `quick_xml::Writer`. Enable exactly `features = ["serialize"]`.
- **CSV:** `csv 1.4` + serde structs per row. Flat tag export maps cleanly; UDT parameter tables need explicit column ordering — decide the column contract in requirements, not in the crate.

### Development Tools (no additions)

| Tool | Purpose | Notes |
|------|---------|-------|
| Existing `wiremock` / `snapbox` / `assert_cmd` / `predicates` / `tempfile` | Test stack unchanged | LSP + MCP handlers test at the message layer (feed framed JSON, assert framed responses) — no new test deps. MCP conformance oracle = Python `mcp` SDK invoked like the existing Python harness precedent |

---

## Installation

```toml
# Root Cargo.toml [workspace.dependencies] — the complete v1.1 delta
quick-xml = { version = "0.42", features = ["serialize"] } # tag-provider bulk transfer XML (read+write, serde bridge)
csv = "1.4"                                                 # tag-provider bulk transfer CSV
lsp-server = "0.10"                                         # IDE-02: LSP transport scaffold (ign lsp)
lsp-types = "0.97"                                          # IDE-02: LSP 3.17 protocol types
# NO MCP crate. EXT-04 is hand-rolled JSON-RPC 2.0 over stdio on serde_json + tokio (already in graph).
# Escalation path if the MCP surface grows beyond tools-only stdio:
# rmcp = { version = "3.2", features = ["server", "transport-io", "macros"] }
```

Placement: `quick-xml`/`csv` → `ignition-core` (serialization lives next to the gateway client, testable with wiremock fixtures). `lsp-server`/`lsp-types` → `ignition-cli` (the `ign lsp` subcommand). MCP shim → `ignition-cli` (`ign mcp serve`).

```bash
cargo add quick-xml --features serialize -p ignition-core
cargo add csv -p ignition-core
cargo add lsp-server lsp-types -p ignition-cli
```

---

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| Hand-rolled MCP shim | `rmcp 3.2.0` (official SDK) | If/when MCP mode needs prompts, resources, elicitation, sampling, streamable-HTTP transport, or the spec drifts in ways a shim can't track. Cost then: `chrono`, `schemars`, `uuid`, `pastey`, `indexmap`, `tokio-util`, `futures` enter the tree (chrono non-optional). Acceptable as a *deliberate later decision* |
| Hand-rolled MCP shim | `mcp-*` third-party crates | Never found a credible one worth listing — official SDK dominates; niche alternatives are less maintained than rmcp |
| `lsp-server 0.10` + `lsp-types 0.97` | `tower-lsp-server 0.23` | Only if tower middleware composition is genuinely wanted; the `ls-types 0.0.x` dependency makes it strictly worse for us today |
| `lsp-server 0.10` + `lsp-types 0.97` | `async-lsp 0.2.4` | Only in an already-async tower-native codebase; its `lsp-types ^0.95` pin is behind the curve |
| `lsp-server` | Hand-rolled LSP framing | Never. LSP's Content-Length framing + encoding edge cases are exactly what this crate exists to absorb; it costs 2 crates |
| `quick-xml 0.42` | `roxmltree` | Read-only (no writer) — wrong for a round-trip transfer feature |
| `quick-xml 0.42` | `hard-xml` / `xmltree` | Derive-first but heavier/pull-based-or-stale; quick-xml is the ecosystem default at 40× the downloads |
| `csv 1.4` | Hand-rolled CSV | Never. Quoting/escaping edge cases are a solved, subtle problem |

---

## What NOT to Add (lean-tree compliance)

| Rejected | Reason |
|----------|--------|
| `rmcp` (for now) | Official and healthy, but stdio-server profile forces `chrono` (on this project's own reject list), `schemars`, `uuid`, `pastey`, `indexmap`, `tokio-util` into the tree for capabilities a tools-only shim never uses. Documented escalation path, not a dead end |
| `tower-lsp 0.20` | Dead — last release 2023-08-11 (verified crates.io) |
| `tower-lsp-server 0.23` | Community fork is alive but depends on its own `ls-types 0.0.6` (0.0.x = no semver contract) plus tower/dashmap/httparse; heavier than lsp-server for nothing we need |
| `async-lsp 0.2.4` | Pin to `lsp-types ^0.95` and tower middleware weight; wrong fit |
| `notify` (file watching) | IDE-03 workspace checkout is command-driven fetch/sync by design — no watcher. 9.0.0-rc exists but adds an event-loop surface we don't want |
| ratatui theme crates (`ratatui-themes` et al.) | All are micro-projects (≤2.5k downloads, verified 2026-09-04); a ~50-line `Theme` struct over ratatui 0.30.2 `Style`/`Color` is the whole feature |
| `chrono` | v1.0 standing decision reaffirmed; if EXT-02 diagnostics needs client-side date math, `jiff 0.2` (17.9M downloads, active) is the sanctioned option — likely still unnecessary since gateway timestamps arrive as strings |
| `tokio-util` (direct) | Only needed by rmcp's codec; the hand-rolled MCP shim does newline framing with `tokio::io::BufReader::lines()` |
| `schemars` | MCP tool input schemas are hand-authored descriptive JSON for passthrough tools; derive-generated schemas would misdescribe CLI-arg semantics |
| `serde_yaml`, `toml_edit`, `config`, `figment` | v1.0 standing decisions; TUIX-01 is one TOML field, not a config-framework problem |

---

## Confidence Assessment

| Area | Level | Basis |
|------|-------|-------|
| Crate versions | HIGH | Every version/deps/MSRV figure pulled live from the crates.io API on 2026-09-04; rmcp feature graph read from its GitHub `main` manifest |
| LSP ecosystem verdict | HIGH | Maintenance dates verified; `lsp-server` API surface confirmed on docs.rs (0.10.0, Connection/IoThreads/dispatch-loop model) |
| MCP spec facts | HIGH | modelcontextprotocol.io 2025-06-18 transports page fetched directly (stdio framing, stdout purity, version negotiation) |
| MCP hand-roll recommendation | MEDIUM | Facts HIGH, but the verdict is an engineering judgment balancing lean-tree precedent vs. spec-drift ownership. Mitigated by the documented rmcp escalation path and Python-SDK conformance testing |
| Tag XML serde coverage | MEDIUM | quick-xml serde capability is HIGH-confidence; the *degree* to which Ignition's tag XML resists derive-mapping needs phase-specific research against real exports |

## Open Questions for Phase Research

1. **Ignition tag XML shape vs. serde derive** — pull a real multi-level UDT export and decide derive-vs-Event-loop split before writing code (phase research, wiremock fixtures can't provide this).
2. **MCP protocolVersion pin** — confirm "2025-06-18" acceptance across target clients (Claude Code, Claude Desktop) at implementation time; a live smoke test, not research.
3. **`ign lsp` + reqwest under a sync loop** — verify the thread-loop + in-process tokio `block_on` pattern under the existing `tracing-subscriber` (LSP servers must keep stdout clean — logs go to stderr or a file, same rule as MCP).

## Sources

- crates.io API (`/api/v1/crates/...`): rmcp, tower-lsp, tower-lsp-server, ls-types, async-lsp, lsp-server, lsp-types, quick-xml, csv, jiff, notify, schemars, ratatui theme search — fetched 2026-09-04
- rmcp 3.2.0 `Cargo.toml` — github.com/modelcontextprotocol/rust-sdk (`main`, fetched 2026-09-04): feature graph, chrono non-optionality, MSRV 1.88
- docs.rs/lsp-server/0.10.0 — crate API and rust-lang/rust-analyzer ownership
- Context7 `/websites/rs_rmcp_rmcp` — ServerHandler/`#[tool_router]`/stdio transport API surface
- Context7 `/tafia/quick-xml` — serde bridge capability
- modelcontextprotocol.io spec 2025-06-18, Basic/Transports — stdio framing, stdout purity rule, version negotiation
- `.planning/research/STACK.md` (v1.0) — What-NOT-Use list carried forward and extended
