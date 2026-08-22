---
phase: 02-gateway-health-inspection
plan: 04
subsystem: api
tags: [reqwest, wiremock, serde, polling, logs, ndjson, sqlite-idb, loggers, backoff, snapbox, confirmation-guard, streaming]

# Dependency graph
requires:
  - phase: 02-gateway-health-inspection (02-01)
    provides: "classifier + get_json pipeline, ListQuery/ListEnvelope, IgnitionMock harness, exit taxonomy (reused exclusively — error.rs gained NO new variant, only an Option relaxation)"
  - phase: 02-gateway-health-inspection (02-02)
    provides: "credential-REQUIRED dispatch pattern (resolve_gateway_api), three-mode golden harness, capability-file → trait-method → action → subcommand growth pattern"
  - phase: 02-gateway-health-inspection (02-03)
    provides: "guard-before-construction destructive dispatch, exact-path wiremock pins + recorded-request proofs, top-level-flags Args shape (SessionsArgs)"
provides:
  - "Five new GatewayApi capabilities: logs (LogQuery — startTime tail cursor), logs_download (per-request 120s timeout, bytes + Content-Disposition), loggers, set_logger_level (POST {logger}?level=X empty body), reset_logger_levels (POST levelreset)"
  - "poll.rs — the ONE wait/retry engine (×1.5 adaptive backoff clamp [interval,30s], Network/GatewayRestarting retried, Auth NEVER, deadline → Network-class timeout with last observation) — 02-05's wait/restart --wait reuses it verbatim"
  - "actions::logs::{tail, list_logs, loggers, set_logger_level, reset_logger_levels, download, parse_since} — the tail loop is printer-free via a sink; cursor = max(timestamp)+1 with client-side timestamp sort"
  - "The `ign logs` command tree: list flags hoisted top-level (`ign logs [-f]` parses bare), download [-o], loggers [--search] / set <NAME> <LEVEL> / reset — set/reset are the second/third --yes-guarded mutations"
  - "The SECOND sanctioned stdout exception: `logs -f` under --json/--compact streams NDJSON (one compact entry per line, no envelope), README-documented alongside completions"
  - "LogQuery sortBy support (gateway-native asc()/desc() syntax) — list sends desc(timestamp) so `ign logs` shows the NEWEST 200, not the oldest"
  - "Live-suite additions: live_logs_and_loggers (read-only) + live_logger_level_set_and_reset (behind IGNITION_LIVE_MUTATIONS=1)"
affects: [02-05, phase-03-projects, phase-06-tui]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "State-threading poll signature: poll(cfg, state, FnMut(&'a mut S) -> Probe<'a, T>) — the HRTB pattern that lets a borrowing async closure carry a cursor/sink across iterations; wait-style callers pass S = ()"
    - "Poll deadline → Network with source: None (same slug/exit, no new variant): the ONLY None-source Network is a poll timeout; genuine transport errors stay Some(source) with byte-identical Display"
    - "Streaming dispatch: tail entries print to stdout DURING execution via a mode-aware sink; render_ok intercepts the LogsTail variant (completions precedent) — one envelope contract preserved everywhere else"
    - "Zero-dep ISO-8601 UTC rendering: civil-from-days over epoch-ms (deterministic across machines, no chrono/tz machinery); raw epoch-ms stays the --json value"
    - "Recent-without-a-window: explicit limit + sortBy=desc(timestamp) yields the newest N entries — no default since-window policy to invent or maintain"

key-files:
  created:
    - crates/ignition-core/src/client/logs.rs
    - crates/ignition-core/src/poll.rs
    - crates/ignition-core/src/actions/logs.rs
    - crates/ignition-core/tests/logs_contract.rs
    - crates/ignition-cli/tests/contract_logs.rs
  modified:
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-core/src/lib.rs
    - crates/ignition-core/src/actions/mod.rs
    - crates/ignition-core/src/actions/version.rs
    - crates/ignition-core/src/actions/inspect.rs
    - crates/ignition-core/src/actions/sessions.rs
    - crates/ignition-core/src/actions/connections.rs
    - crates/ignition-core/tests/live_gateway.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - README.md

key-decisions:
  - "CoreError::Network.source relaxed to Option<reqwest::Error> (NOT a new variant — plan-mandated 'reuse the network_error slug'): source: None marks a poll deadline expiry whose url carries subject + waited + last observation; Some-source Display is byte-identical, so no golden moved"
  - "The probe signature threads &mut State through an HRTB closure (for<'a> FnMut(&'a mut S) -> Pin<Box<dyn Future + 'a>>) — the standard escape from Rust's closure-returning-borrowed-future limitation; the count/streamed bookkeeping rides an outer Cell so results survive poll consuming the state"
  - "logs list sends sortBy=desc(timestamp) + explicit limit (gateway's own openapi asc()/desc() syntax) — 'recent log entries' (must-have truth #1) without inventing a default since-window; the plan's LogQuery grew one additive optional field to carry it"
  - "loggers list sends explicit limit=200 too — must-have truth #5 says EVERY logs command, the registry included (Pitfall 9's rationale applied strictly)"
  - "Tail maps the poll's None-source Network deadline error to Ok (graceful --timeout, exit 0) — sound because poll retries genuine Network errors until the deadline, so None-source is unambiguously the timeout"
  - "post_empty's dead-code gate came off with its first production callers (set/reset level) and grew query params — 02-05's restart POST confirm=true rides the same helper"
  - "Download filename precedence: -o FILE > Content-Disposition > <profile>-logs-<unix_ts>.idb — the profile name is the user's own gateway identifier (no extra gateway-info round-trip); never .zip (Pitfall 7)"
  - "Human timestamps render ISO-8601 UTC (zero-dep civil-from-days) with the epoch-ms value always available in --json — deterministic across machines, no tz database"

patterns-established:
  - "poll.rs is THE wait engine for 02-05 (wait gateway/restart/module): probe = StatusPing/modules checks, deadline defaults 120s (restart 300s), Ctrl-C = default process kill documented in README"
  - "Streaming command shape: dispatch-owned sink printing during execution + render_ok interception — the template for any future streaming output"
  - "The guard family grows: 'logs loggers set' / 'logs loggers reset' join 'sessions terminate' — require_confirmation fires before resolve_gateway_api, envelope profile null on refusal"

# Metrics
duration: 35min
completed: 2026-08-22
---

# Phase 2 Plan 4: gateway logs (query, tail, download, logger levels) Summary

**Five logs capabilities + the shared poll engine feeding a full `ign logs` tree — query (newest-first via the gateway's own desc() sort), poll-based live tail with NDJSON streaming (the second sanctioned stdout exception), byte-exact SQLite `.idb` download, and the --yes-guarded logger level mutations — all contract-pinned with recorded-request proofs.**

## Performance

- **Duration:** 35 min
- **Started:** 2026-08-22T03:07:40Z
- **Completed:** 2026-08-22T03:42:19Z
- **Tasks:** 3
- **Files modified:** 18 (5 created, 13 modified)

## Accomplishments
- HLTH-03 shipped end-to-end: `ign logs` queries (filters --logger/--min-level/--since EPOCH-MS-or-relative, explicit --limit always) and shows the NEWEST entries (desc(timestamp) + limit 200 — never the server's unlimited default, Pitfall 9 pinned by mock matchers); `ign logs -f` tails via pure polling with ×1.5 adaptive backoff, streaming human lines or NDJSON per line; `ign logs download` writes the archive byte-for-byte as an `.idb` (Content-Disposition naming, 120s per-request timeout override, never .zip — Pitfall 7)
- HLTH-04 shipped: `ign logs loggers` (registry, explicit limit) plus `loggers set <NAME> <LEVEL>` / `loggers reset` as the CLI's second and third --yes-guarded mutations — exit 2 golden-pinned refusals firing BEFORE any API construction, --yes success proven by a recorded POST carrying `level=DEBUG` on the query string with an empty body
- The ONE wait/retry engine exists (poll.rs) and is test-proven: retry matrix (Network/GatewayRestarting retried, Auth fails at exactly one call, other classes abort), the exact 2s→3s→4.5s→…→30s backoff sequence, and deadline expiry as a Network-class timeout (exit 4, network_error slug, NO new variant) carrying subject + last observation — 02-05's wait inherits it verbatim
- The tail semantics are unit-proven before any CLI wiring: two scripted pages then silence deliver entries in timestamp order (client-side sort beats server page ordering), cursor advances max+1 per page, and the --timeout deadline ends cleanly Ok (exit 0); Ctrl-C stays the documented default process kill
- The agentic streaming story is documented reality: NDJSON tail exception in README next to completions, one-envelope contract explicitly preserved for every other command

## Task Commits

Each task was committed atomically:

1. **Task 1: logs query + download capabilities** - `a6e6a3b` (feat)
2. **Task 2: poll helper + tail loop** - `3727d48` (feat)
3. **Task 3: `ign logs` command tree + guards + streaming goldens** - `84e8f96` (feat)
4. **Key-link literal alignment** - `843463d` (refactor)

## Files Created/Modified
- `crates/ignition-core/src/client/logs.rs` - LogEntry (camelCase renames, stack/mdc), LogQuery (explicit limit 200, startTime cursor, sortBy), LoggerInfo, LogDownload + Content-Disposition filename extraction
- `crates/ignition-core/src/poll.rs` - PollConfig/PollState/poll + backoff math + retry matrix + 6 scripted-FakeProbe unit tests
- `crates/ignition-core/src/actions/logs.rs` - tail (sink design, cursor discipline), list_logs (desc sort), loggers, set/reset, download (filename precedence), parse_since grammar — each unit-tested
- `crates/ignition-core/src/client/mod.rs` - +5 trait methods; get_bytes pipeline (per-request from_secs(120)); get_json generalized to query pairs; post_empty grows query params + gate off
- `crates/ignition-core/src/error.rs` - Network.source → Option (None = poll deadline; Display unchanged for Some)
- `crates/ignition-core/tests/logs_contract.rs` - 8 wiremock scenarios: explicit-limit + startTime recorded proofs, stack-trace parse, .idb download, exact set-level path with empty body, all 7 levels, levelreset, registry shape, HTML 401 → Auth
- `crates/ignition-cli/src/cli.rs` - Logs tree (top-level flags + download/loggers subcommands), LogLevel ValueEnum, --since value_parser
- `crates/ignition-cli/src/main.rs` - Logs dispatch (tail sink streaming, guards, download stem), mode threaded into dispatch
- `crates/ignition-cli/src/render.rs` - log entry line (ISO-UTC/LEVEL/logger/message), loggers rows, set/reset/download lines, civil_from_days + tests
- `crates/ignition-cli/tests/contract_logs.rs` - 8 goldens incl. 3-mode list, param pins, exit-2 guards, --yes recorded proof, byte-exact download, and the -f --timeout 2 streaming pin (human + NDJSON, exit 0)
- `crates/ignition-core/tests/live_gateway.rs` - +2 opt-in checks (now 9): read-only logs/loggers; set/reset behind IGNITION_LIVE_MUTATIONS=1
- `README.md` - 6 command rows, "Streaming output (the second stdout exception)" section, .idb note, logs mutations in destructive ops
- Test doubles stubbed for the grown trait: `actions/version.rs`, `actions/inspect.rs`, `actions/sessions.rs`, `actions/connections.rs`

## Decisions Made
- **Network.source Option relaxation (plan-conforming):** the plan mandates deadline expiry as "Network-class error … reuse the network_error slug, no new variant" — a poll timeout has no reqwest::Error to show, so source became Option with None marking the deadline; the deadline message lives in `url` (subject + waited + last observation); every existing construction site wraps in Some, Display for Some-source is byte-identical, and no golden moved
- **HRTB state-threading probe:** `FnMut() -> Fut` cannot express a closure whose future borrows its own captured cursor/sink; `for<'a> FnMut(&'a mut S) -> Probe<'a, T>` (boxed future) is the standard escape — poll owns the state, lends it per call; TailState { cursor, sink } for the tail, S = () for wait-style callers
- **Newest-first without a window policy:** the gateway's own openapi documents `sortBy=asc(field)/desc(field)` — `desc(timestamp)` + the explicit limit satisfies "recent log entries" deterministically; LogQuery gained one additive optional field (sort_by) rather than inventing a default since-window
- **Tail graceful end:** poll's None-source Network error maps to Ok — unambiguous because genuine transport errors are retried until deadline, so None-source can ONLY be the timeout
- **--since validates at clap parse time** via a value_parser delegating to the core grammar (relative spans resolve against now at parse); junk → clap's own exit-2 usage error, consistent with the taxonomy's "clap renders its own usage errors"
- **post_empty consolidation:** query params added (set-level's `level=`, future restart's `confirm=true`) and the dead-code gate removed in the same commit its first production callers landed — the gate's own doc comment anticipated 02-04

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Trait-growth stub script dropped a closing brace**
- **Found during:** Task 1 (trait method stubbing across the four existing test doubles)
- **Issue:** The scripted insertion of the five new trait-method stubs omitted the final method's closing brace, breaking compilation in inspect.rs
- **Fix:** Re-ran a corrected patch adding the brace across all four files
- **Files modified:** crates/ignition-core/src/actions/{version,inspect,sessions,connections}.rs
- **Verification:** cargo test --workspace green (65 core lib tests), clippy -D warnings clean
- **Committed in:** a6e6a3b (Task 1 commit)

**2. [Rule 3 - Blocking] Closure-returning-borrowed-future did not typecheck as planned**
- **Found during:** Task 2 (poll helper)
- **Issue:** The plan's `F: FnMut() -> Fut` sketch cannot express a probe whose future borrows closure-captured state (Rust's classic HRTB limitation) — E0582/E0597 on first compile
- **Fix:** Restructured poll to thread a `state: S` it owns and lend it per call: `for<'a> FnMut(&'a mut S) -> Probe<'a, T>` (boxed future); the tail's cursor/sink ride TailState, the stream count rides an outer Cell
- **Files modified:** crates/ignition-core/src/poll.rs, crates/ignition-core/src/actions/logs.rs
- **Verification:** poll + tail unit tests green (retry matrix, backoff sequence, deadline message, order/cursor/graceful-end)
- **Committed in:** 3727d48 (Task 2 commit)

**3. [Rule 2 - Missing Critical] List ordering would have shown the OLDEST entries**
- **Found during:** Task 3 (list action)
- **Issue:** The plan's LogQuery carries no sort; without it, `ign logs --limit 200` fetches the server's default order (oldest-first page) — violating must-have truth #1 ("sees RECENT log entries")
- **Fix:** Added additive `sort_by: Option<String>` to LogQuery (gateway-native `asc()/desc()` syntax from its own openapi) and list_logs sends `desc(timestamp)` — newest 200, matcher-pinned in the goldens
- **Files modified:** crates/ignition-core/src/client/logs.rs, crates/ignition-core/src/actions/logs.rs, crates/ignition-cli/tests/contract_logs.rs
- **Verification:** `logs_list_render_modes_golden` pins sortBy=desc(timestamp) + limit=200 on the request; entries render newest-first
- **Committed in:** 84e8f96 (Task 3 commit)

---

**Total deviations:** 3 auto-fixed (1 bug, 1 blocking, 1 missing critical)
**Impact on plan:** All three were correctness-preserving refinements inside the plan's own contracts (no new error variant, no envelope change, no scope creep). The must-have key_link `from_secs(120)` was also aligned literally (843463d) after the constant-indirect form would not have matched the pattern check.

## Issues Encountered
- snapbox goldens cannot contain a fixture tab character (`json!` turns `\t` into a real tab whose re-escaped round-trip differs from the raw-string golden) — stack-trace fixtures use "at …" without the leading tab
- The stateful wiremock fixture (page-then-silence) serves its entry only to the first request a server ever sees — the NDJSON run needed a fresh server instance (recorded-request statefulness is per-fixture, a wiremock behavior worth remembering)
- iso_utc hand-verification caught two wrong test EXPECTATIONS (not algorithm bugs): 19723 days = 2024-01-01 (not 02-29) and the research timestamp renders 21:12:27Z — the civil-from-days implementation was correct

## Authentication Gates

None — wiremock covers the contract; the two new live checks are skip-by-default and inherit the env contract (`IGNITION_LIVE_URL`/`IGNITION_LIVE_TOKEN`, plus `IGNITION_LIVE_MUTATIONS=1` for the level mutations; see [02-USER-SETUP.md](./02-USER-SETUP.md)).

## User Setup Required

None beyond 02-01's opt-in live suite.

## Next Phase Readiness
- poll.rs is 02-05's engine: `wait gateway` (StatusPing probe, Done on RUNNING), `restart --wait` (the 300s deadline + POST via post_empty's confirm=true query param), `wait module` (modules/healthy?search=) — all documented in poll.rs's module header
- The streaming-dispatch shape (sink during execution + render_ok interception) is the template if 02-05's wait gains watch-style output
- All 18 test suites green; exit taxonomy unchanged (no new variant — verification constraint held); clippy -D warnings + fmt clean
- Remaining for Phase 2: 02-05 (wait + doctor + restart)

## Self-Check: PASSED
