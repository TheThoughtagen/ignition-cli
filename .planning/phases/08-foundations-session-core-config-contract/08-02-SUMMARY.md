---
phase: 08-foundations-session-core-config-contract
plan: 02
subsystem: core
tags: [session, reqwest, credential-resolution, env-overlay, secret-chain, deref, wiremock, rust]

# Dependency graph
requires:
  - phase: 02-foundations-core
    provides: ReqwestGatewayApi + GatewayApi seam, config load/apply_env_overlay/resolve_selection/resolve_secret, CoreError taxonomy
  - phase: 01-cli-foundation
    provides: the duplicated resolution choreography in main.rs this seam ports verbatim
provides:
  - ignition_core::session::Session — the ONE execution seam: profile name + Arc<ReqwestGatewayApi> with resolve / resolve_degraded / for_url + api()/api_handle()/profile_name() + Deref
  - The three credential modes behind one type: REQUIRED (authed reads), degraded-to-None (version/waits/doctor), headerless-by-construction (rig family)
  - Behavior-parity test suite pinning the LOCKED choreography (overlay-before-selection, secret-chain order, headerless degraded mode)
affects: [08-03-cli-migration, 08-04-tui-migration, phase-14-mcp (dyn-widening), TUIX-05 call-site cleanup]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Concrete-with-deref session handle: Arc<ReqwestGatewayApi> + Deref so free-fn actions over &GatewayApi keep working unchanged (dyn-widening deferred to Phase 14)"
    - "Env-var mutation scoped to sync sections under ENV_LOCK in async tests (client snapshots URL+credential at construction, so guards drop before the awaits)"

key-files:
  created:
    - crates/ignition-core/src/session.rs
    - crates/ignition-core/tests/session_contract.rs
  modified:
    - crates/ignition-core/src/lib.rs

key-decisions:
  - "Concrete-with-deref (Arc<ReqwestGatewayApi>, not Arc<dyn GatewayApi>) — TUI workers/ClientHandle are concretely typed; Phase 8 is construction-site unification, dyn-widening rides Phase 14 where MCP needs dyn"
  - "IGNITION_PROFILE folding stays in the bin's apply_env_defaults — Session::resolve takes the EFFECTIVE flag and never re-reads the env (one env→flag home preserved)"
  - "for_url rig sessions carry an empty profile name (no profile exists); callers translate to None where output models want an echo"
  - "Secret::expose boundary unchanged — Session composes existing config fns and introduces no new exposure path (CORE-02 grep-auditable rule)"

patterns-established:
  - "Session construction trio: resolve() for authed commands, resolve_degraded() for version/waits/doctor, for_url() for rig clients — one ReqwestGatewayApi::new module, zero per-call-site construction"
  - "Async env tests: ENV_LOCK held only across sync sections; awaits run after the guard drops"

# Metrics
duration: 200min
completed: 2026-09-05
---

# Phase 8 Plan 2: Session Execution Seam Summary

**`ignition_core::session::Session` — the single construction site for gateway clients with all three credential modes (resolve / resolve_degraded / for_url), behavior-pinned against the duplicated main.rs/TUI choreography it replaces**

## Performance

- **Duration:** 200 min (dominated by cargo lock contention with the concurrently executing plan 08-01 in the same working tree)
- **Started:** 2026-09-05T12:36:05Z
- **Completed:** 2026-09-05T15:56:21Z
- **Tasks:** 2
- **Files modified:** 3 (2 created, 1 modified)

## Accomplishments
- Session type (`profile: String` + `Arc<ReqwestGatewayApi>`) with `resolve` / `resolve_degraded` / `for_url` constructors, `api()` / `api_handle()` / `profile_name()` accessors, and a `Deref` impl feeding every existing free-fn action over `&GatewayApi` unchanged
- The LOCKED choreography ported verbatim into core: env overlay scoped to the would-be selection FIRST → selection (flag > active, `ProfileNotFound` with knowns, `NoActiveProfile` on none) → LOCKED secret chain (env tokens → keyring → basic pair)
- Within `ignition-core/src`, the ONLY production `ReqwestGatewayApi::new` callers are session.rs's three constructors (grep-verified); `Secret::expose` boundary untouched
- Behavior-parity proofs: overlay-targets-selected-profile (wiremock dual-server), selection precedence + error taxonomy, env-token-beats-basic-pair + required-errors/degraded-headerless, for_url credential/URL carrying, and version-action reach through all three handle shapes

## Task Commits

Each task was committed atomically:

1. **Task 1: Session type with resolve / resolve_degraded / for_url** - `e3c01b3` (feat)
2. **Task 2: Session behavior-parity tests (precedence + secret chain + modes)** - `620972e` (test)

## Files Created/Modified
- `crates/ignition-core/src/session.rs` - The Session seam: three credential modes + rig constructor + Deref, with env-dependent behavior-parity unit tests
- `crates/ignition-core/tests/session_contract.rs` - Wiremock contract tests for for_url (URL/credential carrying, headerless tolerance) and Deref action reach
- `crates/ignition-core/src/lib.rs` - `pub mod session;` export + module-map doc update

## Decisions Made
- **Concrete-with-deref** (planner decision honored from the plan): `Arc<ReqwestGatewayApi>`, not `Arc<dyn GatewayApi>` — dyn-widening deferred to Phase 14
- **IGNITION_PROFILE is inert in the seam**: the bin's `apply_env_defaults` folds it into `--profile` (one env→flag home); the plan's "flag > IGNITION_PROFILE env > active" precedence holds at the bin level, and the core test pins the boundary (env alone does NOT drive selection)
- **for_url sessions have no profile** — `profile_name()` returns `""` (documented; callers translate)
- **Manual `Debug` for Session** — profile name only; the client is never rendered (redaction discipline)
- **ssl_verify wire-level assertion deferred**: the https-skip wire case needs wiremock's `tls` feature (not enabled in the workspace); the propagation path is exercised over http and the builder behavior stays pinned by the client's own tests

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Recreated work destroyed by the concurrent plan 08-01 executor**
- **Found during:** Task 1 (mid-verification)
- **Issue:** The parallel 08-01 executor working the same tree wiped this plan's uncommitted files (session.rs deleted, lib.rs edit reverted) between verification runs — confirmed by `rustfmt` reporting "session.rs does not exist" and `git status` no longer listing lib.rs
- **Fix:** Recreated both files from the authored content and committed Task 1 immediately after verification to close the race window
- **Files modified:** crates/ignition-core/src/session.rs, crates/ignition-core/src/lib.rs
- **Verification:** build + clippy + tests re-run after recreation; all green
- **Committed in:** e3c01b3

**2. [Rule 3 - Blocking] Profile gained `poll_interval_secs` mid-session (concurrent 08-01 work)**
- **Found during:** Task 1 (build failure E0063 — missing struct field)
- **Issue:** The parallel executor's TUIX-05 work added `poll_interval_secs: Option<u64>` to `config::Profile` after this session read the file
- **Fix:** Added `poll_interval_secs: None` to for_url's anonymous Profile (rig sessions have no poll cadence)
- **Files modified:** crates/ignition-core/src/session.rs
- **Verification:** `cargo build -p ignition-core` green
- **Committed in:** e3c01b3

**3. [Rule 1 - Bug] Async tests held ENV_LOCK across await points (clippy `await_holding_lock`)**
- **Found during:** Task 1 verification
- **Issue:** The std MutexGuard held across wiremock awaits — a lint error under `-D warnings` and a genuine hazard pattern
- **Fix:** Restructured so env mutation + sync `resolve*` calls hold the lock in scoped blocks; the `await`ed requests run after the guard drops (sound: the client snapshots URL + credential at construction)
- **Files modified:** crates/ignition-core/src/session.rs (test module)
- **Verification:** clippy clean; a lock-poison cascade exposed by this fix also surfaced and fixed a phase-2 test bug (missing token re-supply)
- **Committed in:** e3c01b3

**4. [Rule 3 - Blocking] wiremock `expect()` rejects `usize` in this version**
- **Found during:** Task 1 verification
- **Issue:** `Mock::expect(impl Into<Times>)` — `Times: From<usize>` unsatisfied; scoped-guard helpers needed `u64`
- **Fix:** mount helpers take `expected: u64`
- **Files modified:** crates/ignition-core/src/session.rs, crates/ignition-core/tests/session_contract.rs
- **Verification:** compile clean
- **Committed in:** e3c01b3 / 620972e

**5. [Rule 3 - Blocking] ENV_LOCK unreachable from integration tests**
- **Found during:** Task 2 (test placement)
- **Issue:** `ENV_LOCK` is `#[cfg(test)] pub(crate)` — integration test binaries cannot see it, but the precedence/secret-chain tests need serialized env mutation
- **Fix:** Env-dependent tests live as unit tests inside session.rs (per the plan's ENV_LOCK convention); the no-env tests (for_url, Deref reach) went to tests/session_contract.rs — the plan's `tests/` file surface honored where compatible
- **Files modified:** crates/ignition-core/src/session.rs, crates/ignition-core/tests/session_contract.rs
- **Verification:** full core suite green
- **Committed in:** e3c01b3 / 620972e

---

**Total deviations:** 5 auto-fixed (1 concurrent-executor wipe recovery, 2 concurrent-repo-evolution adaptations, 2 tooling/lint fixes)
**Impact on plan:** All fixes were forced by executing plan 08-02 concurrently with 08-01 in one working tree plus toolchain realities — zero scope creep into the plan's design.

## Issues Encountered
- **Cargo lock contention:** the parallel executor's continuous build/clippy loops starved this plan's full-suite runs repeatedly (three 10–15 min timeouts); resolved by running suites detached with `nohup` and polling
- **Workspace-wide fmt/clippy gates are temporarily contaminated** by 08-01's uncommitted WIP (`config/mod.rs`, `config/profile.rs`, `main.rs`, `context.rs`, `update.rs` show fmt diffs / carry in-flight changes): this plan's OWN files are fmt-clean and clippy-clean; the tree-wide gates should be re-run once 08-01 commits. The plan's tree-wide grep-clean sweep proof is — per the plan text — deferred to 08-03 Task 2 by design

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- The seam exists with all three credential modes and for_url; production construction is consolidated to one module — 08-03 (CLI migration) and 08-04 (TUI migration) can move call sites onto `Session::resolve` / `Session::resolve_degraded` / `Session::for_url` with the behavior-parity suite as the safety net
- 08-03 Task 2 will prove the tree-wide grep-clean criterion (`rg 'ReqwestGatewayApi::new' crates/` → only session.rs + for_tests)
- Watch-item: if 08-01's landing renames/moves `Profile` fields again, for_url's anonymous Profile literal is the one site to update

---
*Phase: 08-foundations-session-core-config-contract*
*Completed: 2026-09-05*

## Self-Check: PASSED

- crates/ignition-core/src/session.rs — FOUND (540 lines, exceeds the 80-line minimum)
- crates/ignition-core/tests/session_contract.rs — FOUND
- lib.rs contains `pub mod session;` — VERIFIED in commit e3c01b3
- Task commit e3c01b3 (feat) — FOUND in git log
- Task commit 620972e (test) — FOUND in git log
- Full-suite evidence: ignition-core 498 passed / 0 failed; ignition-tui + ignition-cli 381 passed / 0 failed
