---
phase: 06-tui-cockpit
plan: 02
subsystem: ui
tags: [ratatui, tokio, dashboard, profile-switcher, elm-architecture, wiremock, poll-engine]

# Dependency graph
requires:
  - phase: 06-tui-cockpit plan 01
    provides: the cockpit shell (select loop, Elm state/update, modal infra, context resolution, worker conventions)
  - phase: 05-tags-webdev
    provides: the finished action surface (inspect/sessions/version/connections/doctor/restart/profile free fns) the workers compose
provides:
  - Live status dashboard: status/modules/metrics/sessions panels, 5s interval refresh worker, per-panel Loading/Loaded/Error degradation
  - The one-shot action pattern: spawn_action (in-flight busy guard, era-stamped, pretty-JSON result modal scrollable via PgUp/PgDn) — every future action verb copies it
  - Dashboard actions menu (a): version, connections, wait gateway/restart/module, doctor, restart — confirm-gated restart (modal accept ≡ --yes), Input-modal prompt for wait module's id
  - Session terminate off the sessions table (t/Enter → Confirm → worker)
  - Profile switcher modal (p, any screen): list + add form, ATOMIC switch (rebuild-first), era bump + worker re-target, stale-era drops, ProfileChanged banner
  - poll.rs Probe futures are now Send (the TUI can tokio::spawn whole waits)
affects: [06-03-logs, 06-04-tags, 06-05-alarms, 06-06-projects-rig]

# Tech tracking
tech-stack:
  added: [serde + serde_json as ignition-tui runtime deps (result-modal serialization), wiremock as ignition-tui dev-dep (fixture gateway)]
  patterns:
    - "Interval worker: snapshot() composes action fns AS-IS via future::join4, per-call degrade (Option<T> + error string per panel)"
    - "One-shot action worker: spawn_action(state, label, future) — Handle::try_current-guarded so update() never panics outside a runtime (tests) "
    - "Atomic profile switch: rebuild FIRST (failure keeps old world whole), persist second, adopt third — era bump retires in-flight results"
    - "PendingAction/PendingInput route modal accepts (Confirm y / Input Enter); Esc clears pending so a stale Confirm can never arm a later y"

key-files:
  created:
    - crates/ignition-tui/src/workers/refresh.rs
    - crates/ignition-tui/src/ui/profiles.rs
  modified:
    - crates/ignition-tui/src/event.rs
    - crates/ignition-tui/src/state.rs
    - crates/ignition-tui/src/update.rs
    - crates/ignition-tui/src/lib.rs
    - crates/ignition-tui/src/context.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-tui/src/ui/mod.rs
    - crates/ignition-tui/src/ui/dashboard.rs
    - crates/ignition-tui/src/workers/mod.rs
    - crates/ignition-tui/Cargo.toml
    - crates/ignition-core/src/poll.rs
    - crates/ignition-core/src/actions/restart.rs
    - crates/ignition-core/src/actions/logs.rs
    - crates/ignition-core/src/actions/rig.rs

key-decisions:
  - "Switch order inverted from the plan sketch (rebuild → use_profile → adopt, not use_profile → rebuild): a failed rebuild now leaves config.active UNWRITTEN — 'switch is atomic' must-have over the listed sequence (truths-over-sketches precedent)"
  - "poll.rs Probe gained + Send and the outer scratch Cells (restart/logs/rig) became Mutexes: the TUI tokio::spawns whole waits and &Cell is !Send; every locked contract intact (HRTB lifetime-carrying state, () payload, outer-owned terminal state surviving poll) — all 308 core tests green unchanged"
  - "doctor's credential_present is always true from the cockpit (the REQUIRED-credential context resolved it — the honest answer for this surface)"
  - "spawn helpers are Handle::try_current-guarded: outside a runtime (state-machine unit tests) the guard+label transition stands alone — update() can never panic by construction"
  - "ONE crate-wide #[cfg(test)] ENV_LOCK in lib.rs — per-module locks do not serialize cross-module and a racing teardown sent context tests at the real machine config"
  - "ProfileChanged banner is retired by the first refresh of the new world (the confirmation fulfilled its purpose); status line prefers banner over the persistent profile name"
  - "restart + sessions terminate join the CLI's --yes verbs via Modal::Confirm accept ('y'); the action fns themselves stay unguarded (the TUI owns confirmation — caller-owns-guard, same as main.rs)"

patterns-established:
  - "Panel tri-state render: PanelState Loading/Loaded/Error projected per panel — never blank, never frozen (the async-github LoadingState shape every 06-03..06-06 screen copies)"
  - "session_rows(state) is THE flatten contract (designers→perspective→vision) shared by render (table rows) and update (selection → terminate target)"
  - "Screen-owned modal rendering: ui/profiles.rs renders its variants; ui::render_modal delegates (per-screen modules own files)"

# Metrics
duration: 60min
completed: 2026-08-27
---

# Phase 6 Plan 2: Dashboard Summary

**Live four-panel dashboard (5s refresh worker composing the action fns unmodified), dashboard actions menu with confirm-gated verbs + scrollable pretty-JSON result modals, session terminate, and an atomic in-TUI profile switcher with era-based worker re-targeting**

## Performance

- **Duration:** 60 min
- **Started:** 2026-08-27T18:19:41Z
- **Completed:** 2026-08-27T19:19:26Z
- **Tasks:** 3
- **Files modified:** 16 (2 created, 14 modified)

## Accomplishments
- The dashboard is live: status/modules/metrics/sessions panels refresh every 5s with zero keystrokes; one failing endpoint degrades its panel to an honest error — a dead gateway shows four errors, never a frozen or blank UI
- Every global verb is reachable from the `a`ctions menu; long waits run in workers with only a "running: LABEL" status-line footprint; results land in the one-mechanism scrollable pretty-JSON modal
- Profile switching works end-to-end in state-machine terms: persisted active profile, rebuilt client, era bump, fresh shutdown rail, workers re-spawned, stale-era events dropped whole
- 28 new tests (542 → 570 workspace): wiremock composition proof, worker shutdown-termination proof, TestBackend panel/modal renders, confirm state machine, isolated-env switch tests

## Task Commits

Each task was committed atomically:

1. **Task 1: Refresh worker + dashboard panels with async states** - `5a8157f` (feat)
2. **Task 2: Dashboard actions menu + one-shot result modal + session terminate** - `bd69207` (feat)
3. **Task 3: Profile switcher modal (use_profile + client rebuild + era bump)** - `68718cc` (feat)

**Plan metadata:** (this commit)

## Files Created/Modified
- `crates/ignition-tui/src/workers/refresh.rs` - Snapshot (per-panel Option<T>+error), snapshot() over future::join4, the 5s interval worker (shutdown watch, era stamps, MissedTickBehavior::Skip), spawn_refresh/spawn_refresh_once
- `crates/ignition-tui/src/ui/profiles.rs` - switcher list + add-form modal rendering (screen-owned; ui/mod.rs delegates)
- `crates/ignition-tui/src/ui/dashboard.rs` - four-panel render (PanelState tri-state), sessions TableState selection, status line (profile/banner/freshness/in-flight/hints)
- `crates/ignition-tui/src/update.rs` - Refresh/ActionDone/ProfileChanged arms (era-gated), dashboard keymap (r/a/t/Enter/arrows), modal acceptors (Actions nav, Confirm y, Input Enter, Result_ paging, Profiles nav, ProfileAdd fields), switch_profile/submit_profile_add
- `crates/ignition-tui/src/state.rs` - DashboardData, ACTIONS, Modal::Actions/Profiles/ProfileAdd, Result_ scroll, PendingAction/PendingInput, SessionRow + session_rows, ClientHandle, profile/banner/profile_url fields
- `crates/ignition-tui/src/workers/mod.rs` - spawn_action (the one-shot pattern, in-flight guard, Handle::try_current-guarded)
- `crates/ignition-tui/src/event.rs` - Refresh{era, Box<Snapshot>}, ActionDone{era, label, result}, ProfileChanged{era, name}
- `crates/ignition-tui/src/lib.rs` - run_loop wiring (client/profile/url rails, spawn_refresh, shutdown on every exit), crate-wide test ENV_LOCK
- `crates/ignition-tui/src/context.rs` - resolve/rebuild return (name, url, api) — doctor's profile_url
- `crates/ignition-tui/src/routes.rs` - 15 rows (12 dashboard families + profile use/list/add; bare `sessions` = the list leaf)
- `crates/ignition-core/src/poll.rs` - Probe type gains + Send
- `crates/ignition-core/src/actions/{restart,logs,rig}.rs` - outer scratch Cells → Mutexes (Send probe futures; locked HRTB contracts intact)

## Decisions Made
- Switch sequence ordered rebuild→persist→adopt (atomicity must-have over the plan's listed order — a failed rebuild leaves config unwritten)
- poll.rs + Send via Mutex scratch instead of a TUI-side thread-with-own-runtime: the plan's tokio::spawn architecture works verbatim and every existing core test passes unchanged
- context::resolve/rebuild grew a url return (doctor); crate-wide ENV_LOCK replaces per-module test locks
- Box<Snapshot> in AppEvent::Refresh (clippy large_enum_variant); snapshot() takes &Arc<ReqwestGatewayApi> so the action calls carry the locked `(&**api)` shape the key_link greps

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] poll.rs futures were not Send — tokio::spawn of the wait actions did not compile**
- **Found during:** Task 2 (wiring the wait verbs into spawn_action)
- **Issue:** The poll engine's `Probe` box lacked `+ Send` and every probe captured `&Cell` (Cell is !Sync), so wait_gateway/wait_restart/wait_module (and tail/commissioned_wait) futures could not be tokio::spawn'd — the plan's stated one-shot-worker architecture
- **Fix:** `Probe` type gains `+ Send`; the outer scratch Cells in restart.rs (3 sites), logs.rs (streamed counter), rig.rs (commissioned flag) became Mutexes (clippy-suggested get_mut() on the exclusive probe refs). Every locked 02-04/02-05 contract intact: HRTB lifetime-carrying state, () payload, outer-owned terminal state surviving poll
- **Files modified:** crates/ignition-core/src/poll.rs, crates/ignition-core/src/actions/restart.rs, crates/ignition-core/src/actions/logs.rs, crates/ignition-core/src/actions/rig.rs
- **Verification:** all 308 ignition-core tests green unchanged; full workspace 570 green; clippy -D warnings clean
- **Committed in:** bd69207

**2. [Rule 3 - Blocking] Per-module test ENV_LOCKs raced cross-module (context tests resolved the real machine config)**
- **Found during:** Task 3 (isolated-env switch tests)
- **Issue:** update.rs's new env-mutating tests held only a local (useless, fresh-instance) mutex; a racing teardown dropped IGNITION_CLI_CONFIG mid-test and context::resolve(None) fell back to the machine's real config (active: "uat" — SecretUnavailable)
- **Fix:** ONE crate-wide `#[cfg(test)] pub(crate) static ENV_LOCK` in lib.rs; context.rs tests moved onto it; all env-mutating tests serialize
- **Files modified:** crates/ignition-tui/src/lib.rs, crates/ignition-tui/src/context.rs, crates/ignition-tui/src/update.rs
- **Verification:** the previously-racing tests pass repeatedly; full suite deterministic
- **Committed in:** 68718cc

**3. [Rule 2 - Missing Critical] Esc-cancel now clears armed modal payloads**
- **Found during:** Task 2 (Confirm-modal accept design)
- **Issue:** A canceled Confirm (Esc) that kept its pending action armed could fire on a LATER, unrelated Confirm's `y` — a stale-confirmation hazard the plan's "cancel spawns nothing" truth demanded be impossible
- **Fix:** Esc (and menu-Enter fresh starts) clear dashboard.pending/pending_input; pinned by confirm_cancel_spawns_nothing_and_clears_pending
- **Files modified:** crates/ignition-tui/src/update.rs
- **Verification:** update unit (cancel clears pending + nothing in flight)
- **Committed in:** bd69207

---

**Total deviations:** 3 auto-fixed (2 blocking, 1 missing critical)
**Impact on plan:** All three were required to deliver the plan's own must-have truths (tokio::spawn waits, deterministic tests, cancel-spawns-nothing). No scope creep; core contracts verified unchanged.

## Issues Encountered
- clippy borrow_deref_ref fought the key_link's literal `(&` call shape — resolved by snapshot() taking `&Arc<ReqwestGatewayApi>` so every action call is the honest `(&**api)` unsizing coercion (pattern greps 4/4)
- wiremock ListEnvelope fixtures need the `metadata` block present ( designers/clients `{"items": []}` alone fails decode) — noted for future fixtures

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- 06-03 (Logs) plugs in: the tail sink is already Send, the one-shot/interval worker patterns and PanelState render shape are established, and the era/shutdown conventions are proven by the switcher
- The live-rig smoke (`ign tui` against a real gateway, `p` switch re-populating panels) remains the phase-end human verification item (config: end-of-phase); all state machines are headless-proven
- routes.rs now carries 15 rows; 06-06's bidirectional clap-walk test inherits them

---
*Phase: 06-tui-cockpit*
*Completed: 2026-08-27*

## Self-Check: PASSED

All 7 key files exist on disk; all 3 task commits (5a8157f, bd69207, 68718cc) found in git history; key_link greps verified (4× action-fn calls, actions:: in refresh.rs, profile::use_profile + Modal::Confirm in update.rs, era in state.rs); dashboard.rs 377 lines (min 60 must-have).
