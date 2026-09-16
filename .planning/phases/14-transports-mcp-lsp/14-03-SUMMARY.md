---
phase: 14-transports-mcp-lsp
plan: 03
subsystem: transports
tags: [lsp, lsp-server, lsp-types, stdio, json-rpc, ttl-cache, refresher-thread, soft-degrade, out-of-band]

# Dependency graph
requires:
  - phase: 08-session-seam
    provides: Session::resolve (the one auth/config seam — CORE-09), the `lsp` OutOfBand slug reservation (08-06)
  - phase: 14-transports-mcp-lsp (14-01)
    provides: the protocol-mode pattern (hidden clap command → main seam → OutOfBand row atomicity) and the mcp pin precedent in tui_coverage.rs
provides:
  - "`ign lsp` — lsp-server 0.10 stdio LSP with a sync dispatch loop on the main thread (framing/handshake/shutdown owned by the crate)"
  - "Narrow-by-design ServerCapabilities: completion (triggers [/ .]) + hover + FULL sync — definition/codeAction/workspace symbols never claimed (ignition-lsp composition)"
  - "GatewayCache: Arc<RwLock<Arc<Snapshot>>> TTL snapshot (providers/tag_trees/named_queries/projects/fetched_at/healthy) — the SC-3 structural half: zero network on any request path"
  - "Background refresher thread owning the module's ONLY runtime (current-thread, Runtime::block_on) — Session::resolve construction site, per-section fallible populate, soft-degrade healthy:false on failure"
  - "Kick channel (didSave→refresh, burst-collapsing) + snapshot version channel (diagnostics publishing) defined for 14-04"
  - "`lsp` OutOfBand row + pinned set exactly [api call, completions, edit, mcp, lsp] landed atomically — the 08-06 reservation set is CLOSED"
  - "The phase's ONLY new dependencies: lsp-server 0.10 + lsp-types 0.97"
affects: [14-04 (handlers fill the loop bodies + consume the channels; contract suite proves SC-3 behaviorally), IDE-03/ignition-nvim (the cmd = ign lsp client)]

# Tech tracking
tech-stack:
  added: [lsp-server 0.10, lsp-types 0.97]
  patterns:
    - "Protocol-mode pattern (second instance): hidden clap unit variant + plain-sync main seam + OutOfBand row + pinned-set extension in ONE commit"
    - "Cache-only request path: Arc<RwLock<Arc<Snapshot>>> read-path clones the inner Arc (lock held microseconds); single-writer refresher"
    - "SnapshotSource trait seam: sync collect() so state-machine tests inject fakes with no gateway and no runtime; the live impl block_on's its OWN runtime"
    - "Test-shaped smoke: python scripted LSP client over the real binary with byte-exact Content-Length framing (both happy and unreachable-gateway paths)"

key-files:
  created:
    - crates/ignition-cli/src/lsp.rs
  modified:
    - Cargo.toml
    - Cargo.lock
    - crates/ignition-cli/Cargo.toml
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-cli/tests/tui_coverage.rs

key-decisions:
  - "GatewaySource OWNS its current-thread Runtime and uses Runtime::block_on — a cloned Handle::block_on into an idle current-thread runtime does NOT drive its reactor (the connect future hung forever; caught by the unreachable-gateway live smoke, not by unit tests)"
  - "Session::resolve re-attempts per cycle ONLY while session is None — reconciles 'resolve once at startup' (one construction SITE, happy path holds one client for the server's lifetime) with the degrade-soft recovery truth (a gateway that comes back is seen next cycle)"
  - "Tag populate runs the webdev version precondition ONCE per cycle and then calls the deployed tags route's browse action directly per node (the SAME route/action `ign tags browse` uses) — a per-node precondition would triple the wire cost of a refresh; node cap 5000/provider bounds a pathological tree"
  - "Refresher thread-spawn failure is non-fatal: the cache stays seed-empty/unhealthy and the server serves honestly from it (degrade-soft all the way down)"
  - "Refresher spawn returns (cache, versions_rx) and run() creates the kick channel — both channels exist NOW with the final loop shape so 14-04 only fills handler bodies"

patterns-established:
  - "SC-3 evidence logging: every request logs the snapshot frame it answered from (health/section counts/age) — the contract suite can read protocol-adjacent truth from stderr without touching the wire"
  - "Offline-smoke shape: valid TOML config + token_env + dead URL — resolve succeeds, sections fail, server serves (the truest unreachable-gateway test; auth must ride token_env + IGNITION_TOKEN because BasicEnvStore reads the GLOBAL IGNITION_USER/PASSWORD, not per-profile env names)"

# Metrics
duration: 46 min
completed: 2026-09-16
---

# Phase 14 Plan 03: LSP Transport Summary

**`ign lsp` — lsp-server 0.10 stdio transport with a sync dispatch loop, narrow-by-design capabilities, and a TTL GatewayCache refreshed by a soft-degrading Session-only background thread; the `lsp` OutOfBand row closes the 08-06 reservation set at exactly [api call, completions, edit, mcp, lsp]**

## Performance

- **Duration:** 46 min
- **Started:** 2026-09-16T02:37:17Z
- **Completed:** 2026-09-16T03:23:43Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- `ign lsp` speaks the LSP base protocol end to end over the real binary: Content-Length-framed initialize response with the narrow capability surface, silent notifications, -32601 for unclaimed methods, clean shutdown handshake (exit 0)
- Capabilities are narrow BY DESIGN and unit-pinned: completion + hover + FULL sync, with definition/codeAction/workspace-symbol absence asserted so nvim's other attached client (ignition-lsp) owns the statics
- GatewayCache is live: the refresher thread owns the module's only runtime, publishes per-section-fallible snapshots every 30 s (kick-draining, version-announcing), and degrades to healthy:false on any failure — handlers' data path is a microsecond read-lock Arc clone with zero network
- Offline live-proven: with an unreachable gateway the session resolves, every section warns and fails, the frame publishes healthy:false, and the server still initializes, answers completion from the cache, and shuts down cleanly (exit 0)
- OutOfBand atomicity held for the last time: `lsp` row + hidden command + pinned set exactly [api call, completions, edit, mcp, lsp] in one commit; the bidirectional clap walk stayed green

## Task Commits

Each task was committed atomically:

1. **Task 1: Cargo deps + hidden `ign lsp` command + OutOfBand row + lsp-server sync loop scaffold** - `fb23e6a` (feat)
2. **Task 1 follow-up: lock the new deps into Cargo.lock** - `ee283b8` (chore)
3. **Task 2: GatewayCache TTL snapshot + background refresher thread (soft-degrade, Session-only)** - `b3352f1` (feat)

## Files Created/Modified
- `crates/ignition-cli/src/lsp.rs` (NEW, 802 lines) — sync dispatch loop, narrow ServerCapabilities, Snapshot/GatewayCache, SnapshotSource trait + GatewaySource (Runtime-owning), per-section populate (providers/tags/named-queries/projects), 10 unit tests
- `Cargo.toml` — workspace entries for lsp-server 0.10 + lsp-types 0.97 (the phase's only new deps)
- `Cargo.lock` — the dep pair locked
- `crates/ignition-cli/Cargo.toml` — the two workspace-carried dependencies
- `crates/ignition-cli/src/cli.rs` — hidden `Commands::Lsp` unit variant (leaf path exactly `lsp`)
- `crates/ignition-cli/src/main.rs` — `mod lsp`, the plain-sync Lsp seam beside Mcp's (no block_on at main level), dispatch exhaustiveness arm
- `crates/ignition-tui/src/routes.rs` — OutOfBand row for `lsp` with written justification; 08-06 reservation comments closed
- `crates/ignition-cli/tests/tui_coverage.rs` — pinned OutOfBand set extended to exactly [api call, completions, edit, mcp, lsp]

## Decisions Made
- Runtime ownership in GatewaySource (Runtime::block_on, not Handle::block_on) — see key-decisions; the smoke caught what unit tests could not
- Per-cycle re-resolve only while unresolved (one construction site, recovery preserved)
- One precondition per cycle + direct browse calls for tag trees (same trusted route/action, node-capped)
- Thread-spawn failure degrades rather than dies (cache stays unhealthy; server serves)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Workspace-root Cargo.toml gained the dependency entries**
- **Found during:** Task 1 (step 1)
- **Issue:** the plan lists only `crates/ignition-cli/Cargo.toml` as modified, but every dep in this crate is workspace-carried (`workspace = true`) — the two new deps cannot resolve without `[workspace.dependencies]` entries
- **Fix:** added `lsp-server = "0.10"` + `lsp-types = "0.97"` to the workspace root with the phase comment; the crate references them workspace-style. Cargo.lock followed in `ee283b8`
- **Files modified:** Cargo.toml, Cargo.lock
- **Verification:** `cargo build -p ignition-cli` resolves both; the phase's dep delta is still exactly these two
- **Committed in:** fb23e6a / ee283b8

**2. [Rule 1 - Bug] lsp-types 0.97 renamed the sync enum**
- **Found during:** Task 1 (scaffold compile)
- **Issue:** the research Pattern 4 sketch spells `TextDocumentSyncType` — lsp-types 0.97 exports `TextDocumentSyncKind` (E0432)
- **Fix:** import/`match` updated to `TextDocumentSyncKind::FULL` (capability + pin test)
- **Files modified:** crates/ignition-cli/src/lsp.rs
- **Verification:** clean compile; narrow-caps unit test green
- **Committed in:** fb23e6a

**3. [Rule 1 - Bug] lsp-server 0.10 Response/RequestId API differs from the sketch**
- **Found during:** Task 1 (unit tests compile)
- **Issue:** the sketch-era API (`RequestId::Number`, `Response.result`/`Response.error` fields) does not exist in 0.10 — `RequestId` is a newtype with `From<i32>`, and `Response` carries `response_result: Result<Value, ResponseError>` (`ResponseError` not `PartialEq`)
- **Fix:** tests rebuilt on the real surface (`RequestId::from(n)`, `response_result.expect…`, `err.code`)
- **Files modified:** crates/ignition-cli/src/lsp.rs
- **Verification:** 4 scaffold tests green against 0.10 semantics
- **Committed in:** fb23e6a

**4. [Rule 1 - Bug] Handle::block_on into an idle current-thread runtime never drives its reactor**
- **Found during:** Task 2 (unreachable-gateway live smoke)
- **Issue:** the refresher first stored a cloned `Handle` and `collect()` called `handle.block_on` — for a current_thread runtime that is not being driven, the reactor never polls: the connect future hung forever, the first cycle never completed, and the server answered from the SEED frame (healthy:false for the wrong reason — no section warn ever fired)
- **Fix:** GatewaySource OWNS the `Runtime`; `collect()` uses `Runtime::block_on`, which drives the current-thread reactor; the runtime moves back in on re-resolve. Live smoke now shows session resolved → three section-failure warns in milliseconds → real published frame answering
- **Files modified:** crates/ignition-cli/src/lsp.rs
- **Verification:** offline smoke ×3 green (resolve + section warns + completion answered from the published frame); unit tests untouched
- **Committed in:** b3352f1

---

**Total deviations:** 4 auto-fixed (1 blocking, 3 bug)
**Impact on plan:** All four were necessary for the plan's own verification gates to pass (dep resolution + two stale-API spellings from the research sketch + a reactor subtlety only observable live). No scope creep; every must_have truth/artifact/key_link is satisfied.

## Issues Encountered
- The parallel 14-02 agent's in-flight `tests/contract_mcp.rs` made workspace-wide clippy transiently red during this plan's verification windows (it changed between runs and stabilized green by plan close). All 14-03-owned targets were verified clippy-clean independently throughout; the final workspace-wide clippy is green.
- The offline smoke's config debugging surfaced a documented-but-easy-to-miss auth fact: `BasicEnvStore` reads the GLOBAL `IGNITION_USER`/`IGNITION_PASSWORD`, not the per-profile `user_env`/`password_env` names — smoke configs here use `token_env` + `IGNITION_TOKEN` (the contract tests' own recipe). No product code change.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- The loop shape is FINAL: 14-04 fills `handle_request`/`handle_notification` bodies against `cache.snapshot()` (read-lock only) and wires didSave → the kick channel + diagnostics publishing → the snapshot version channel (both channels already exist in `run()`)
- The seed-frame honesty test guarantees handlers never mistake pre-first-publish emptiness for truth; the per-request SC-3 evidence log gives 14-04's contract suite a stderr-side oracle for the zero-network property
- `ign lsp` is already consumable by the nvim patch (`cmd = { 'ign', 'lsp' }`) for IDE-03's end-to-end verification in the sibling repo
- 14-02's contract suite is landing in parallel; both contract suites share the purity-harness recipe (tempdir config + IGNITION_* stripping + IGNITION_LOG noise)

---
*Phase: 14-transports-mcp-lsp*
*Completed: 2026-09-16*

## Self-Check: PASSED

- Files: all 8 key-files exist on disk (lsp.rs NEW at 802 lines — artifact min_lines 200 exceeded)
- Commits: fb23e6a (Task 1), ee283b8 (lockfile), b3352f1 (Task 2) all present in git log
- must_have greps: `lsp-server` in crates/ignition-cli/Cargo.toml ✓; hidden `Lsp` variant in cli.rs ✓; `"lsp"` row in routes.rs ✓; `lsp` pin in tui_coverage.rs ✓; `lsp::serve` seam in main.rs ✓; `Session::resolve` link in lsp.rs ✓
- Verification suite at close: `cargo test -p ignition-cli` 34/34 test targets green (10 lsp unit tests incl. swap/degrade/TTL pins; tui_coverage 5/5 with the five-member pin); `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --check` clean; live smokes green (happy-path handshake + unreachable-gateway degrade, ×3 stable)
