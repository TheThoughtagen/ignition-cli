# Architecture Research: ignition-cli v1.1 Integration

**Domain:** Integration of 11 v1.1 features into an existing 3-crate Rust CLI/TUI workspace (Ignition 8.3+ gateway cockpit)
**Researched:** 2026-09-04
**Confidence:** HIGH (every integration point verified by reading the actual source; MCP/LSP library choices are MEDIUM with phase-research flags)

---

## System Overview — What Exists Today (verified)

```
┌───────────────────────────── ign (single binary) ─────────────────────────────┐
│                                                                               │
│  ENTRY POINTS (3 runtime modes exist today)                                   │
│  ┌──────────────────┐  ┌───────────────────┐  ┌────────────────────────────┐  │
│  │ clap tree        │  │ TUI loop          │  │ completions (stdout        │  │
│  │ (cli.rs,         │  │ (ignition-tui:    │  │  exception, render_ok      │  │
│  │  Commands enum)  │  │  Elm state/update,│  │  bypass precedent)         │  │
│  │       │          │  │  workers/, ui/)   │  └────────────────────────────┘  │
│  │       ▼          │  └────────┬──────────┘                                  │
│  │  main.rs dispatch│           │ calls actions directly                     │
│  │  → ActionOutput  │           │ (key_link contract: workers compose        │
│  │  → render.rs     │◄──────────┘  action fns AS-IS, never re-implement)      │
│  └───────┬──────────┘                                                         │
├──────────┼────────────────────────────────────────────────────────────────────┤
│          │        ignition-core (lib — core NEVER prints)                     │
│  ┌───────▼──────────┐  ┌──────────────┐  ┌───────────────┐  ┌─────────────┐  │
│  │ actions/         │  │ client/      │  │ scripts_codec │  │ webdev/     │  │
│  │ (shared verb     │─▶│ GatewayApi   │  │ flint encode/ │  │ embedded    │  │
│  │  layer, 1 file   │  │ trait (coarse│  │ decode, dedent│  │ Python      │  │
│  │  per family)     │  │ 1 mth/cap)   │  │ decode/encode │  │ bundle 1.1.0│  │
│  └──────────────────┘  │ Reqwest-     │  │ _export_tree  │  │ + version   │  │
│  ┌──────────────────┐  │ GatewayApi   │  └───────────────┘  │ handshake   │  │
│  │ config/ (Profile,│  └──────┬───────┘  ┌───────────────┐  └─────────────┘  │
│  │ AuthRef, secret  │         │          │ projects diff │  ┌─────────────┐  │
│  │ chain: profile→  │  ┌──────▼───────┐  │ /sync (cross- │  │ rig/ compose│  │
│  │ env → keyring)   │  │ classify.rs  │  │ gateway member│  │ ComposeRunner│ │
│  └──────────────────┘  │ status→err   │  │ normalized    │  │ seam (tokio │  │
│  ┌──────────────────┐  │ taxonomy     │  │ entries)      │  │ process     │  │
│  │ output.rs Json   │  │ + error.rs   │  └───────────────┘  │ precedent;  │  │
│  │ Envelope {ok,    │  │ exit codes   │  ┌───────────────┐  │ lint.rs also│  │
│  │ profile, data}   │  │ LOCKED 0–7   │  │ poll.rs       │  │ shells out) │  │
│  └──────────────────┘  └──────────────┘  │ PollConfig    │  └─────────────┘  │
│                                          └───────────────┘                   │
└───────────────────────────────────────┬───────────────────────────────────────┘
                                        │
                     ┌──────────────────▼──────────────────┐
                     │   Ignition 8.3+ Gateway             │
                     │   REST /data/api/v1/* + WebDev      │
                     │   /system/webdev/{project}/cli/*    │
                     └─────────────────────────────────────┘
```

**Locks the v1.1 work must respect (all verified in source):**

| Lock | Where enforced |
|------|----------------|
| JSON envelope `{ok, profile, data}` | `ignition-core/src/output.rs` — golden-file contract, field order matters |
| Exit codes 0–7 | `error.rs::exit_code()` — the ONLY mapping site, `exit_code_mapping_enumerated` test |
| Core never prints | `output.rs` doc comment + every action returns serde models |
| Actions = shared verb layer | `actions/mod.rs` — "CLI handlers and the TUI both call these" |
| GatewayApi coarse trait | `client/mod.rs` — one method per capability; ALL impl bodies in ONE impl block (Rust single-impl rule) |
| TUI↔CLI parity | `ignition-cli/tests/tui_coverage.rs` — walks live clap tree ↔ TUI route registry bidirectionally; new CLI commands FAIL CI without registry rows |
| Single binary, no daemon | workspace + release posture; TUI already proves an alternate main-loop fits in the binary |
| Stdout discipline | completions / `logs -f` NDJSON / `tags export -o -` / `rig logs` are the only sanctioned raw-stdout exceptions |
| Profile config shape | `config/profile.rs` — `Config {active, profiles, rig, rigs}`, `Profile {url, label, ssl_verify, auth, webdev_secret}`; unknown keys WARN, never deny |
| WebDev bundle version-locked | `webdev/mod.rs` — `ROUTE_BUNDLE_VERSION = "1.1.0"`, `MIN_CLI = "1.0"`, probe handshake |

**Cadence constants that are hardcoded today (TUIX-01's targets):**
- `ignition-tui/src/lib.rs`: `TICK = 250ms` (redraw floor, LOCKED)
- `workers/refresh.rs`: `REFRESH_PERIOD = 5s`
- `workers/tail.rs`: `TAIL_INTERVAL = 2s`
- `workers/watch.rs`: `ALARMS_PERIOD = 5s`, `WATCH_PERIOD = 2s`

---

## Feature Classification — The Answer to "What Kind of Change Is Each?"

| # | Feature | Classification | New crate? | New deps? | CI-gate impact |
|---|---------|---------------|-----------|-----------|----------------|
| 1 | EXT-01 `ign api call` | **New command family** | No | None | tui_coverage row |
| 2 | EXT-02 curated diagnostics | **New command family** | No | None | tui_coverage rows |
| 3 | EXT-03 EAM writes | **Family extension** | No | None | tui_coverage rows |
| 4 | EXT-04 MCP transport | **New runtime mode** (hidden `ign mcp`) | No (lives in ignition-cli, NOT core) | 1 (MCP protocol crate) | OutOfBand registry row |
| 5 | Tag xml/csv transfer | **Family extension** (edge format conversion) | No | 1–2 (csv + xml codec) | flag on existing leaf |
| 6 | Tag↔historian binding | **Family + diff-engine extension** | No | None likely | tui_coverage row |
| 7 | TUI theming | **Config extension + TUI change** | No | None | TUI render tests |
| 8 | TUIX-01 polling cadence | **Config extension + worker parameterization** | No | None | TUI worker tests |
| 9 | IDE-01 `ign edit` | **New command family** (pure orchestration of existing pieces) | No | None (tempfile already direct) | tui_coverage row + OutOfBand (EDITOR subprocess owns terminal) |
| 10 | IDE-02 LSP mode | **New runtime mode** (hidden `ign lsp`) | No (ignition-cli) | 1–2 (lsp-types + server framework) | OutOfBand registry row |
| 11 | IDE-03 workspace checkout | **New command family** over generalized diff/sync engine | No | None | tui_coverage rows |

**The pattern that falls out:** v1.1 adds ZERO new crates and at most ~4 small deps. Two features are new *runtime modes* (MCP, LSP) — both are hidden clap subcommands running their own stdio loops, exactly the way `ign tui` already runs its own terminal loop. Everything else grows the existing action/client/CLI trees additively.

---

## Per-Feature Integration Points & Data Flow

### 1. EXT-01 — `ign api call` (raw passthrough)

**Touches (new/modified explicit):**
- `client/mod.rs`: ONE new coarse method, e.g. `async fn raw_request(&self, method: &str, path: &str, body: Option<Vec<u8>>) -> Result<RawResponse, CoreError>` — endpoint-agnostic, so it satisfies the one-method-per-capability rule. It must reuse the existing pipeline (`url_for` → `apply_auth` → `send_and_classify`) — this is where redirect-policy-none, auth-header rules, and CSRF-free token mutations are already correct.
- `actions/api.rs` (NEW): policy decisions — which statuses classify vs pass through, response size cap, allowed method set (GET/POST/PUT/DELETE), body sniffing.
- `cli.rs`/`main.rs`: new `Commands::Api(ApiArgs)` + one `ActionOutput::ApiCall` variant.
- `error.rs`: likely ZERO new variants — reuse `classify` + `Network`/`Auth`; a `--raw-status` opt-out is data, not a new error class.

**Data flow:** clap → `resolve_gateway_api` (existing, main.rs:~472) → `api.raw_request` → classify/parse → envelope `{ok, profile, data:{status, headers_subset, body}}`. Exit code on non-2xx follows the existing taxonomy unless raw mode is requested.

**Why this integration is cheap:** auth chain, URL building, classify, and the envelope are all done — the feature is ~1 client method + 1 action + 1 CLI arm.

### 2. EXT-02 — curated diagnostics (license, bundle, redundancy, GAN)

**Touches:**
- `client/`: new capability files (`client/diagnostics.rs` NEW) with verified 8.3 path constants + wire models, mirroring the `status.rs`/`metrics.rs` precedent; ONE impl-block addition per method in `client/mod.rs`.
- `actions/diagnostics.rs` (NEW) or extensions to `actions/doctor.rs` — recommend a new action module; doctor is already 1157 lines and is a *pipeline* of probes, diagnostics are *endpoints*.
- `cli.rs`: new family (`DiagnosticsArgs`) with subcommands per curated read.
- `render.rs`: new ActionOutput variants (human tables).
- TUI: dashboard/route-registry rows (CI-enforced).

**Data flow:** identical to every existing read family: clap → resolve → action → client method → normalize → envelope.

**Note:** each endpoint's wire shape needs live-gateway verification (the codebase's established discipline — "passthrough shapes are LOW-confidence until live-proven"). Phase research needed per endpoint, not per architecture.

### 3. EXT-03 — EAM writes beyond guarded basics

**Touches:**
- `client/eam.rs`: new wire models + path constants for the additional write verbs.
- `actions/eam.rs` (695 lines today): extend with new verbs next to `eam_task_create`/`eam_task_force`. The controller-mode gate (`EamNotController`, exit 6) and in-flight/refused classes (`EamTaskTypeRefused`, `EamTaskInFlight`) already exist — new verbs MUST reuse them, not invent classes.
- `cli.rs` `EamArgs`: new subcommands; guard pattern: `require_confirmation` BEFORE client construction (the `sessions terminate` precedent, main.rs:486).
- `error.rs`: only if a genuinely new gateway state appears — prefer mapping onto existing Eam* variants.

**Data flow:** identical to existing EAM verbs.

### 4. EXT-04 — MCP transport (runtime mode #1)

**The pivotal design decision:** MCP is a **hidden subcommand** (`ign mcp`, `#[command(hide = true)]`) that runs a JSON-RPC-over-stdio loop, exactly parallel to how `ign tui` runs the ratatui loop. Single binary preserved; nothing daemonizes; the process lives only as long as the MCP client (Claude/Cursor) keeps it alive.

**Touches:**
- **The shared command-execution core (prerequisite refactor):** today, `dispatch(cli, mode) -> (Option<String>, Result<ActionOutput, CoreError>)` (main.rs:400) is the in-process invocation entry, but it is private to the binary and its output path assumes render-then-exit. Extract a library-callable execute seam so MCP tools and (later) LSP invoke operations in-process:
  - Option A (recommended): promote the profile→client resolution (currently `resolve_gateway_api`, private in main.rs) into `ignition-core` as a small `Session` type (`Session::resolve(profile_flag) -> Result<Session, CoreError>` holding `Arc<dyn GatewayApi>` + profile name), and expose a `run(op: Op) -> Result<impl Serialize, CoreError>` dispatch over a core-level op enum, or simply let MCP call `actions::*` fns directly. **Do NOT shell out to `ign` from the MCP handler** — process spawn per tool call would lose the auth-context reuse, complicate secrets, and multiply startup cost.
  - Option B: parse MCP tool args into clap's `Cli` via `Cli::try_parse_from` and call the extracted `dispatch`. Keeps one dispatch path but couples MCP surface to clap arg spelling. Acceptable fallback.
- `ignition-cli/src/mcp.rs` (NEW): stdio JSON-RPC framing + tool registry. Each MCP tool maps 1:1 onto an existing command family verb; tool output = the existing envelope JSON serialized into the MCP result content; `CoreError::exit_code()` maps to MCP `isError` + the rendered failure envelope (agents already speak this dialect).
- **Stdout discipline is critical:** in MCP mode the entire stdout pipe belongs to JSON-RPC. `init_tracing` already writes stderr only (verified main.rs contract) — but `render_ok`/`render_error` must never run in this mode. Precedent: `ActionOutput::TuiExited` prints nothing further. Enforce structurally: the `ign mcp` dispatch arm never returns through the render path.
- **Dependency choice:** RMCP (official Rust SDK, verified high-reputation, stdio transport supported) vs hand-rolled JSON-RPC. Hand-rolling MCP's framing is feasible (~300 lines) and honors lean-deps, but the protocol surface (initialize handshake, capabilities, tool schemas) argues for RMCP. **MEDIUM confidence, phase research flag** — verify RMCP's MSRV/dependency tree against the 1.88 floor and tokio 1.53 graph before committing.

**Data flow:**
```
MCP client (Claude/Cursor)
  └─ stdio JSON-RPC ──▶ ign mcp loop (mcp.rs)
                          ├─ tool call ─▶ execute core (Session + actions fns)
                          │                 └─ GatewayApi ─▶ gateway
                          └─ envelope JSON ─▶ MCP result content ─▶ client
```

**OutOfBand note:** `tui_coverage.rs`'s OutOfBand set grows from `["completions"]` to include `mcp` (and later `lsp`, `edit`).

### 5. Tag xml/csv transfer

**Touches:**
- `actions/tags.rs` or a NEW `actions/tags_format.rs`: the gateway interchange stays JSON list-of-subtrees (the route actions `exportTags`/`importTags` are unchanged — verified: `tags_export`/`tags_import` already parse/normalize the payload at the action edge). XML/CSV are **pure edge converters**: `normalize → render_format` on export, `parse_format → normalize → importTags` on import.
- `cli.rs`: `--format json|xml|csv` flag on `tags export`/`tags import` (flag value on an existing leaf — no new registry rows).
- New deps: a CSV writer (`csv` crate, tiny, ubiquitous) and an XML writer/parser (`quick-xml`, tiny). Both are serialized-data staples; alternatively hand-roll (Ignition's tag XML is a well-bounded dialect) — **phase research flag: sample a real export XML and decide hand-roll vs crate; the XML dialect may be narrow enough that a ~200-line serializer beats a dependency** (consistent with the repo's zip-surgery self-reliance).

**Data flow (export):** route `exportTags` → JSON payload → parse+normalize (EXISTING) → format converter (NEW) → file/stdout.
**Data flow (import):** file → parse format → normalize to list-of-subtrees (NEW) → route `importTags` (EXISTING).

**Why the code comment matters:** `actions/tags.rs` explicitly documents "JSON ONLY: the planner lock … xml/csv deferred to backlog" — v1.1 lifts the lock by converting at the edge so the planner lock narrows to the wire layer only.

### 6. Tag↔historian binding closure

**Touches:**
- Depends on the 05-06 documented Designer-diff path. Integration shape: either (a) extend `actions/projects.rs::project_diff` entries to carry historian-binding deltas (the diff engine already normalizes cross-gateway member trees — binding info rides the same member data), and/or (b) new tag verbs (`tags historian bind/unbind/list`) via the scriptExec route family.
- Potentially `webdev/`: if a new route action is required, bundle version bumps `1.1.0 → 1.2.0` with the MIN_CLI floor check (the version-locked deploy machinery handles this).

**Confidence:** LOW on the exact mechanism until the 05-06 Designer-diff discovery is re-read at phase research time. Architecture-wise it plugs into two existing seams (diff entries, scriptExec routes) — either way no new runtime structure.

### 7. TUI theming

**Touches:**
- `ignition-core/src/config/profile.rs`: new top-level `[ui]` table on `Config` (NOT per-profile — a human preference, like `rig`): `ui.theme = "mono" | "color"` (enum, serde default "mono" to keep goldens stable). Add `"ui"` to `KNOWN_TOP_LEVEL` in `config/mod.rs` (else every themed config logs a spurious warn).
- `ignition-tui/src/theme.rs` (NEW): a `Theme` struct — palette lookup consumed by `ui/*` render fns. ratatui 0.30 supports per-widget `Style` composition; the change is mechanical: replace inline `Color::X` with theme tokens.
- `ignition-tui/src/context.rs`: resolve theme alongside profile/client; thread through `AppState` (like `profile`/`profile_url` today, lib.rs:80-82).
- CLI ignores the key (TUI-only) — no clap surface, no envelope change.

**CI note:** TUI render tests assert styles today (implied by wiremock fixture tests); default-theme determinism must be a test invariant.

### 8. TUIX-01 — polling cadence

**Touches:**
- `config/profile.rs`: per-profile key — `poll_interval_secs: Option<u64>` on `Profile` (add to `KNOWN_PROFILE_KEYS`). Per-profile is right: cadence tracks gateway load/latency, which is a property of the gateway, not the operator's machine.
- `ignition-tui/src/context.rs`: return it from `resolve`.
- `workers/{refresh,tail,watch}.rs`: parameterize — the workers ALREADY take `period: Duration` as a function arg (verified: `refresh_worker(api, tx, shutdown_rx, era, period)`) — only the spawn sites hardcode the constants. Clamp with floors (e.g. min 1s refresh, 250ms tick stays LOCKED as the redraw floor) to prevent pathological configs from hammering the gateway.
- Optional: `restart --wait`/`wait` defaults could read the same key via `poll.rs::PollConfig` — cheap win, same plumbing.

**Data flow:** config.toml `[profiles.dev] poll_interval_secs = 2` → `context::resolve` → `spawn_refresh(state, period)` → `refresh_worker(..., period)`.

### 9. IDE-01 — `ign edit` round-trip

**Touches (pure orchestration — every primitive exists):**
- `ignition-core/src/actions/edit.rs` (NEW): the pipeline below. New pieces: temp-dir lifecycle (tempfile is already a DIRECT core dep, promoted in 05-02), change detection (re-encode + byte compare), and an **editor seam trait** — `trait Editor { async fn open(&self, path) -> Result<(), CoreError> }` with a `TokioEditor` impl using `tokio::process::Command` (the `lint.rs:121` precedent) and a no-op test impl (the `ComposeRunner` seam precedent, `rig/compose.rs:67`).
- `cli.rs`/`main.rs`: `Commands::Edit` + OutOfBand registry row (the child EDITOR owns the terminal while running).

**Data flow:**
```
ign edit <project> [resource-path]
  ├─ fetch:  export_zip_bytes(api, project)          [actions/resources.rs — EXISTS]
  ├─ decode: decode_export_tree(zip, tmpdir)          [scripts_codec — EXISTS]
  ├─ (flint_decode of script members — INSIDE decode_export_tree)
  ├─ editor: Editor.open(target file in tmpdir)       [NEW seam]
  ├─ detect: encode_export_tree(tmpdir) != original   [scripts_codec — EXISTS]
  └─ push:  project_import(name, new_zip, overwrite)  [actions/projects — EXISTS, guarded + denial-honest]
```
**Key observation:** `decode_export_tree(zip_bytes, out_dir)` and `encode_export_tree(dir)` (scripts_codec.rs:682/774) make this feature mostly wire-up. The one design decision: single-resource edit via `resource_get`/`resource_put` vs whole-project tree. Recommend v1.1 ship **whole-project tree edit** first (it's the decode/encode path that already exists end-to-end) and single-resource as a scoped variant — the round-trip oracle and collision policies are already project-scoped.

**Depends on:** nothing unbuilt — but benefits from IDE-03 landing first (workspace establishes the fs-tree layout conventions `ign edit` reuses).

### 10. IDE-02 — LSP mode (runtime mode #2)

**Touches:**
- `ign lsp` hidden subcommand (parity with `ign mcp`); own stdio loop; OutOfBand registry row.
- `ignition-cli/src/lsp.rs` (NEW): LSP server. **Structural rule: LSP must never enter the clap dispatch/render path** — it's a peer of the TUI loop, not a command. It consumes `ignition-core` directly: tag providers/UDT types (completions), resources list (hover/definition over resource paths), script lint delegation (publish-diagnostics), tag alarms/history (workspace symbols candidates).
- Shares the EXT-04 execute core: `Session` (profile→client) resolution is identical; LSP typically holds ONE long-lived session per editor window (resolved once at `initialize`), unlike MCP's per-call sessions.
- **Dependency choice:** `lsp-types` (data model, small) + a server framework. Candidates verified: `tower-lsp` (High reputation, but maintenance-mode — the community fork `tower-lsp-server` is the maintained line, MEDIUM confidence), `async-lsp` (High reputation, pluggable). **Phase research flag:** verify MSRV-1.88 + dep-tree fit; hand-rolling LSP framing (Content-Length headers + a JSON-RPC loop) is ~150 lines and defensible for the narrow surface ignition-nvim needs, but protocol breadth (didChange notifications, cancellation) favors a framework.

**Data flow:**
```
ignition-nvim ──stdio LSP──▶ ign lsp loop (lsp.rs)
   │                            ├─ initialize → Session::resolve (once)
   │                            ├─ textDocument/completion → tags/UDT reads via GatewayApi
   │                            ├─ publishDiagnostics → lint delegation (actions/lint)
   │                            └─ responses ─▶ nvim
```

### 11. IDE-03 — workspace checkout

**Touches:**
- `ignition-core/src/actions/workspace.rs` (NEW): three verbs.
- **The one real engineering item: generalize the diff engine.** `project_diff` (actions/projects.rs:426) compares gateway↔gateway member trees. Workspace needs member-tree sources that are "gateway zip" OR "local dir tree". Extract a `MemberSource` abstraction (zip bytes vs fs dir → normalized member list) — `decode_export_tree` already produces the fs tree and `encode_export_tree` already reads one, so the normalization seam is a refactor of ~100–200 lines, not a rewrite.
- Reused as-is: scripts_codec encode/decode (the whole fs-tree codec exists), guarded sync semantics (`SyncSelection`, removed-list, `--yes` guard from `project_sync`), import denial honesty (client-level, applies automatically).

**Data flows:**
```
checkout:  gateway export zip ─▶ decode_export_tree ─▶ fs tree (+ manifest)
status:    fs tree ─▶ normalize ┐
           gateway zip ─▶ normalize ┴─▶ diff entries (reuse ProjectDiffResult shape)
sync:      fs tree ─▶ encode_export_tree ─▶ project_import (guarded, --yes, denial-honest)
```
**Direction matters:** this is the diff/sync engine run in the gateway→fs direction for the first time; the guarded-promotion UX (show entries, require --yes) ports from `project sync` verbatim.

---

## Architectural Patterns for v1.1

### Pattern 1: Hidden-Subcommand Runtime Modes (MCP, LSP)

**What:** a second/third main loop inside the same binary, selected by a hidden clap subcommand, running its own stdio protocol loop and never touching the render/exit path.
**When:** protocol servers (MCP, LSP) that must ride the single binary.
**Why this shape:** `ign tui` already proves the pattern (alternate loop, `cfg(feature)` guard, TuiExited render bypass, OutOfBand registry). Reuse it.
**Trade-offs:** hidden subcommands are invisible in `--help` (intended); the OutOfBand coverage list must be maintained deliberately.

```rust
// cli.rs
/// Serve the Model Context Protocol over stdio (hidden; for MCP clients)
#[command(hide = true)]
Mcp,
/// Serve LSP over stdio (hidden; for ignition-nvim)
#[command(hide = true)]
Lsp,
```

### Pattern 2: Shared Command-Execution Core (`Session` + in-process invocation)

**What:** promote profile→client resolution out of `main.rs` into `ignition-core` (`Session::resolve(profile_flag) -> Session { profile_name, api: Arc<dyn GatewayApi>, ... }`) so MCP tools, LSP features, and CLI arms all resolve identically; invoke `actions::*` fns directly rather than shelling out or re-parsing through clap.
**When:** before either MCP or LSP lands.
**Trade-offs:** a small refactor of `resolve_gateway_api` (currently binary-private); alternatives (shell-out to `ign`, clap-reparse per tool call) both leak process boundaries or coupling into the agentic surface.

### Pattern 3: Injectable Subprocess Seams (editor, and everything like it)

**What:** any new subprocess dependency (the `$EDITOR` in IDE-01) gets a trait seam with a production `tokio::process` impl and a controllable test impl — the established `ComposeRunner` (rig/compose.rs:67) and lint patterns.
**Why:** subprocess behavior is untestable without the seam; every existing subprocess call in the repo already follows this.

### Pattern 4: Edge Format Conversion (xml/csv)

**What:** new wire formats convert at the action edge against the EXISTING normalized interchange (list-of-subtrees); the gateway route contract never changes.
**Why:** `tags_export` already parses-and-normalizes at exactly this seam; converters become pure functions with table-driven tests, and the "planner lock" narrows to the wire layer.

---

## Data-Flow Summary — The Three Flows That Change the Architecture

1. **Agentic flow (MCP):** client JSON-RPC → in-process execute core → actions → gateway → envelope JSON → MCP content. New: the in-process execute core (Pattern 2) is the ONLY structural addition; everything else is existing plumbing.
2. **Editor flow (IDE-01/03):** gateway → zip → decoded fs tree → (human edits) → re-encode → guarded import. New: nothing at the wire layer; new orchestration module + diff-engine source abstraction.
3. **Interactive flow (TUI):** config now feeds cadence + theme into existing worker/render structures. New: config keys only; workers already parameterized.

---

## Recommended Build Order (dependency-honoring)

```
Layer 0 — foundations (must land first)
  P1. Execute-core extraction (Session) + config schema extension
      (TUIX-01 poll key + theme key together — ONE config-surface change,
       one goldens update; parameterize TUI worker spawn sites)
        │
Layer 1 — independent command families (parallelizable, any order)
  P2. EXT-01 api call            P3. EXT-02 diagnostics       P4. EXT-03 EAM writes
  P5. tag xml/csv conversion     P6. TUI theming rendering (needs P1 theme key)
        │
Layer 2 — composite / engine work
  P7. IDE-03 workspace checkout (MemberSource diff generalization)
  P8. tag↔historian binding (rides P7's generalized diff OR scriptExec route;
      LOW-confidence pending 05-06 Designer-diff re-read → phase research)
  P9. IDE-01 ign edit (codec complete; reuses P7's tree conventions)
        │
Layer 3 — transports (wrap the now-complete surface)
  P10. EXT-04 MCP server (needs P1 Session; tool surface covers P2–P4 families)
  P11. IDE-02 LSP server (needs P1 Session + P3 diagnostics for publishDiagnostics)
```

**Ordering rationale:**
- **Config first, once:** TUIX-01 and theming share the config surface; landing both keys in one phase avoids two golden-config migrations.
- **Command families before transports:** MCP's tool surface mirrors command families; building MCP first would mean re-touching the tool registry as families land. LSP additionally consumes EXT-02 diagnostics.
- **Workspace before edit:** `ign edit` reuses the decoded-tree layout and conventions workspace checkout establishes; both consume the same codec, and hardening decode/encode at workspace scale de-risks edit.
- **MCP/LSP last:** they are consumer surfaces over everything else; landing them early would force protocol churn.
- **Historian binding is research-gated:** slot it wherever the 05-06 Designer-diff re-read places it; it shares the P7 diff generalization so P7 before P8 is the only hard edge.

**CI gates to plan for in every phase:** `tui_coverage.rs` rows for every new invocable command (P2–P4, P7, P9); OutOfBand additions (`mcp`, `lsp`, `edit`) when P9–P11 land; envelope goldens for every new ActionOutput variant; exit-code enumeration test stays green (no new codes needed — see taxonomy analysis: all planned error surfaces map onto existing classes; `ImportDenied`/`EamNotController`/`TagCollision` cover the risky cases already).

---

## Anti-Patterns (specific to this integration)

### Anti-Pattern 1: MCP/LSP re-implementing gateway access
**What people do:** the MCP handler builds its own reqwest client and speaks REST directly.
**Why wrong:** duplicates auth chain, classify, denial-honesty, redirect policy — five locks re-implemented badly.
**Do instead:** Session + `actions::*` fns; the protocol layer is a translator, never a second client.

### Anti-Pattern 2: Shelling out to `ign` from the MCP/LSP servers
**What people do:** `ign mcp` tool handler runs `ign status --json` as a child process.
**Why wrong:** per-call process+secret-resolution cost; secrets re-resolved per invocation; stdout/stderr mixing hazards; Windows spawn quirks.
**Do instead:** in-process invocation via the execute core (Pattern 2).

### Anti-Pattern 3: Letting MCP/LSP write through the render layer
**What people do:** reuse `render_ok` in protocol modes "for consistency".
**Why wrong:** the protocol owns the entire stdout pipe; one stray human-table print corrupts the JSON-RPC stream.
**Do instead:** structural bypass — the `ign mcp`/`ign lsp` dispatch arms never return through render (TuiExited precedent), enforced by a test that captures stdout in protocol mode.

### Anti-Pattern 4: Per-profile theme or global poll cadence
**What people do:** put theme in `[profiles.dev]` or cadence in a global `[ui]`.
**Why wrong:** theme is operator-machine preference (like editor), cadence is gateway-load preference (like ssl_verify). Cross-placing them makes shared configs churn and multi-gateway setups incoherent.
**Do instead:** `[ui].theme` top-level; `poll_interval_secs` per-profile.

### Anti-Pattern 5: Extending the Python bundle for anything solvable in Rust
**What people do:** add route actions in the embedded bundle for every new need (e.g. historian binding).
**Why wrong:** every bundle change is a version-locked deploy event (ROUTE_BUNDLE_VERSION bump, MIN_CLI negotiation, per-gateway deploy state).
**Do instead:** exhaust native REST + zip-member surgery + existing route actions first; bump the bundle only when the gateway must compute something Rust cannot (the scriptExec-justifying cases).

---

## Integration Points & Internal Boundaries

### Internal boundaries (new edges v1.1 creates)

| Boundary | Communication | Notes |
|----------|--------------|-------|
| MCP/LSP loops ↔ actions layer | direct (Session + fns) | THE new seam (Pattern 2); binary-private today |
| config `[ui]` ↔ ignition-tui | `context::resolve` output | theme + cadence ride the existing resolve tuple |
| tags format converters ↔ tags actions | pure functions over normalized interchange | table-driven tests, no gateway |
| workspace/edit ↔ projects diff engine | `MemberSource` abstraction | the P7 refactor; zip and fs tree as sources |
| `ign edit` ↔ child EDITOR | `Editor` trait seam | ComposeRunner precedent; tokio::process impl |

### External services (unchanged, with two notes)

| Service | Pattern | v1.1 note |
|---------|---------|-----------|
| Ignition REST `/data/api/v1/*` | GatewayApi coarse methods | EXT-01 passthrough + EXT-02 endpoints extend the trait per the one-method-per-capability rule |
| Ignition WebDev routes | route envelope {ok, data\|error} over HTTP 200 | unchanged unless historian binding needs a new action (then bundle 1.1.0→1.2.0) |
| Docker compose | ComposeRunner seam | untouched in v1.1 |
| keyring / env | secret chain | untouched — MCP/LSP inherit it through Session |
| $EDITOR | NEW subprocess via Editor seam | only new external integration |

---

## New Dependency Ledger (lean-deps audit)

| Dep | Feature | Size risk | Verdict |
|-----|---------|-----------|---------|
| RMCP (official MCP SDK) | EXT-04 | MEDIUM — verify MSRV 1.88 + tokio fit | Recommended, phase-research gate |
| `lsp-types` | IDE-02 | tiny (data model) | Recommended |
| tower-lsp-server OR async-lsp | IDE-02 | small–medium; fork-status question | Phase-research gate; hand-roll fallback documented |
| `csv` | tag xml/csv | tiny | Fine |
| `quick-xml` (or hand-roll) | tag xml/csv | tiny | Decide against a sampled real export (phase research) |

Everything else — tempfile, tokio `process` feature, zip, serde — is already in the graph (verified in workspace Cargo.toml comments).

## Scaling Considerations (n/a scale, but "what breaks first")

| Concern | First breakage | Mitigation |
|---------|---------------|------------|
| Gateway hammering by aggressive cadence | a user sets `poll_interval_secs = 0` | floor clamp in worker spawn (documented floor constants) |
| MCP tool-surface growth | 40+ tools confuse clients | registry stays 1:1 with command families; no invented tools (PROJECT.md anti-scope) |
| LSP diagnostics latency on huge projects | lint delegation on full project per keystroke | diagnostics on save/explicit trigger only (no file-watcher — PROJECT.md defers watch-mode) |
| Workspace sync on 100MB projects | encode/decode of full trees in memory | scripts_codec already streams zips via tempfile; keep tree ops disk-backed |

## Sources

**Codebase (all HIGH confidence — read directly):**
- `Cargo.toml` (workspace layout, dep ledger + feature-comment discipline)
- `crates/ignition-cli/src/main.rs` (dispatch chassis, ActionOutput, guards, stdout discipline)
- `crates/ignition-cli/src/cli.rs` (Commands tree, WaitArgs/SessionsArgs Option-subcommand shapes)
- `crates/ignition-cli/tests/tui_coverage.rs` (parity CI rule, OutOfBand set)
- `crates/ignition-core/src/client/mod.rs` (GatewayApi coarse trait, one-impl-block rule, auth/classify pipeline, import denial seam)
- `crates/ignition-core/src/actions/{mod,tags,projects,eam,resources,lint}.rs` (verb layer, interchange format + planner lock comment, diff/sync, EAM error classes, subprocess precedent)
- `crates/ignition-core/src/{output,error,config,poll}.rs` (envelope, exit taxonomy, profile schema + known-keys, PollConfig)
- `crates/ignition-core/src/{scripts_codec,webdev,rig/compose}.rs` (flint codec + tree encode/decode, bundle version lock, ComposeRunner seam)
- `crates/ignition-tui/src/{lib,context,workers/*}.rs` (Elm loop, resolve tuple, cadence constants + period-parameterized workers)
- `.planning/PROJECT.md` (v1.1 scope, explicit anti-scope: no daemon, no watch-mode, MCP = thin shim)
- `.planning/milestones/v1.0-REQUIREMENTS.md` (EXT-04 shim framing)

**Ecosystem (MEDIUM confidence — verified library existence/reputation, not versions):**
- RMCP — official Rust MCP SDK, stdio transport supported (Context7, high reputation)
- tower-lsp / tower-lsp-server (community fork) / async-lsp — Rust LSP server frameworks (Context7)

**Phase-research flags:** RMCP MSRV/dep-tree vs 1.88 floor; LSP framework fork status + MSRV; historian-binding mechanism (05-06 Designer-diff re-read); xml dialect hand-roll-vs-crate against a sampled export; EXT-02 endpoint wire shapes (live-gateway verification per the repo's established discipline).

---
*Architecture research for: ignition-cli v1.1 — Agent Surface & IDE Integration*
*Researched: 2026-09-04*
