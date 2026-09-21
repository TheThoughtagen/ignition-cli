---
phase: 08-foundations-session-core-config-contract
plan: 04
subsystem: tui-session
tags: [session, config, tui, load_for_tui, degradation-contract, reqwest]

# Dependency graph
requires:
  - phase: 08-foundations-session-core-config-contract (08-01)
    provides: config::load_for_tui degradation entry point + [ui]/poll_interval_secs lenient schema
  - phase: 08-foundations-session-core-config-contract (08-02)
    provides: ignition-core::session::Session seam (resolve/resolve_degraded/for_url)
provides:
  - TUI profile→client resolution (resolve/rebuild) fully routed through ignition-core::Session
  - TUI rig clients constructed via Session::for_url (Arc handles)
  - TUI-side duplicated secret_chain + overlay→selection choreography DELETED
  - Degradation boundary pinned by 4 resolve-level tests (degrade new-surface / fatal resolution)
  - lib.rs pre-init contract doc stating the new degrade-vs-fatal split
affects: [08-05 (consumes the resolve/rebuild return for the ResolvedContext struct), 08-06, phase-12 TUI rendering]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Caller-owned load policy + seam-owned choreography: the TUI loads with load_for_tui, then hands the config to Session::resolve_loaded"
    - "Arc-handle rig clients: Session::for_url returns Arc<ReqwestGatewayApi>; call sites deref (&*api / as_deref)"

key-files:
  created: []
  modified:
    - crates/ignition-tui/src/context.rs
    - crates/ignition-tui/src/lib.rs
    - crates/ignition-tui/src/workers/rig_stream.rs
    - crates/ignition-core/src/session.rs (Session::resolve_loaded — landed via 08-03's f95e835, see deviations)

key-decisions:
  - "Session::resolve_loaded(config, flag) added to core: the TUI owns the load (load_for_tui degradation), the seam keeps overlay→selection→LOCKED chain→required-credential construction"
  - "rig helpers return Option<Arc<ReqwestGatewayApi>> (Session hands out Arc handles; the client is not Clone) — also the shape 08-05's ResolvedContext wants"
  - "Boundary test set pins BOTH sides at the resolve level, including a one-file/two-policy discriminator: poll_interval_secs = 0 degrades Ok via resolve while config::load refuses the same file with exit 3"

patterns-established:
  - "TUI load pattern: config::load_for_tui then Session::resolve_loaded — never config::load in the cockpit path"
  - "Boundary tests live at the resolve level (the seam startup actually goes through), raw-TOML fixtures so save() can't launder the typos"

# Metrics
duration: 128min
completed: 2026-09-05
---

# Phase 8 Plan 4: TUI Session Migration + Degradation Wiring Summary

**TUI client construction fully routed through ignition-core::Session (resolve_loaded / for_url) with the duplicated chain deleted, and the load_for_tui degradation contract wired in — new-surface config typos now warn-and-start while resolution failures still exit 3 pre-init, pinned by four boundary tests.**

## Performance

- **Duration:** 128 min (a large share spent polling for wave-2 sibling agents' WIP to settle on the shared worktree)
- **Started:** 2026-09-05T17:45:48Z
- **Completed:** 2026-09-05T19:54:42Z
- **Tasks:** 2
- **Files modified:** 3 (plus the core seam extension documented under Deviations)

## Accomplishments

- `context::resolve`/`rebuild` now resolve through `Session::resolve_loaded` — overlay, selection, and the LOCKED secret chain are core-owned; the TUI's byte-identical `secret_chain` and the duplicated `resolve_from` overlay→selection choreography are DELETED (research gotcha 9's supersession executed)
- Rig clients (`rig_client`/`rig_client_token`) construct via `Session::for_url(url, credential, false)` — zero `ReqwestGatewayApi::new` remains in the TUI crate (tree sweep: construction only in core client + session modules)
- `build_client` loads with `config::load_for_tui`: a config typo in the NEW schema surface degrades to defaults with a stderr tracing warning and the cockpit STARTS; raw TOML / profile-deserialize / selection / auth failures still hard-exit 3 BEFORE `ratatui::init`; the `NoActiveProfile` refusal stays LOCKED
- lib.rs `run()` pre-init doc-comment states the new contract precisely (degrade-vs-fatal split)
- Four boundary tests pin the line at the `resolve` level, including the one-file/two-policy discriminator test

## Task Commits

Each task was committed atomically:

1. **Task 1: TUI client construction onto Session; delete duplicated choreography** - `0780fcd` (feat)
2. **Task 2: Wire load_for_tui into the TUI path; pin the degradation boundary** - `2e87b65` (feat)

**Plan metadata:** (this commit)

## Files Created/Modified

- `crates/ignition-tui/src/context.rs` — resolve/rebuild via `Session::resolve_loaded`; rig helpers via `Session::for_url` returning `Option<Arc<ReqwestGatewayApi>>`; duplicated chain + `resolve_from` deleted; `load_for_tui` wired; 4 boundary tests added
- `crates/ignition-tui/src/lib.rs` — pre-init degradation-contract doc-comment on `run()`
- `crates/ignition-tui/src/workers/rig_stream.rs` — Arc-handle call-site fixes (`as_ref`→`as_deref` on the probe_dyn sites; `&api`→`&*api` at four `&dyn GatewayApi` action calls)
- `crates/ignition-core/src/session.rs` — `Session::resolve_loaded(config, flag) -> (Session, Profile)` added (see Deviations; landed in sibling 08-03's `f95e835` because that agent owned the file concurrently)

## Decisions Made

- **Core seam extension over duplicated choreography:** the plan required `load_for_tui` inside resolve/rebuild AND selection/secret "via Session", but `Session::resolve` loads config internally with the strict `config::load` — physically incompatible with degradation. Added `Session::resolve_loaded` (caller-loaded config in, session + selected POST-OVERLAY profile out) rather than keeping a TUI-side selection duplicate. This also gives 08-05 the `Profile` it needs for `poll_interval_secs`.
- **Arc handles for rig clients:** `Session::for_url` yields `Arc<ReqwestGatewayApi>` and the client is not `Clone`, so the rig helpers changed return type. Mechanical call-site fixes only; semantics unchanged, and the shape matches 08-05's `ResolvedContext.api`.
- **Boundary-test scope:** test (a) (`poll_interval_secs = "banana"`) pins that a wrong-TYPED new-surface key degrades and `resolve` succeeds; the plan's premise that this "previously would have been a type-error ConfigInvalid" doesn't hold (08-01's `lenient_u64` degrades wrong types on BOTH load paths), so the strict/degrading discriminator is test (b) (`poll_interval_secs = 0`), which additionally asserts `config::load` refuses the same file at exit 3. All four planned tests landed; only the rationale annotation differs.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Session lacked a caller-loaded-config constructor**
- **Found during:** Task 1
- **Issue:** `Session::resolve`/`resolve_degraded` load config internally with strict `config::load`; Task 2 requires the TUI to load with `load_for_tui` while selection/secret stay Session-owned. No existing constructor accepts a pre-loaded config — the planned wiring was impossible without a core change.
- **Fix:** Added `pub fn resolve_loaded(config: &mut Config, profile_flag: Option<&str>) -> Result<(Session, Profile)>` to `crates/ignition-core/src/session.rs`; `Session::resolve` now delegates to it (single source of the required-mode choreography). Returns the selected POST-OVERLAY `Profile` so the TUI's url string (and 08-05's interval) stays derivable without re-resolving.
- **Files modified:** crates/ignition-core/src/session.rs
- **Verification:** workspace tests + clippy green; existing Session tests unchanged and passing
- **Committed in:** `f95e835` — NOTE: the concurrent 08-03 executor owned session.rs at that moment (it was simultaneously extending `Session` with url/credential_present fields for its own migration); it integrated the `resolve_loaded` addition into its commit. This plan's own commits carry the TUI side.

**2. [Rule 1 - Bug] rig helper return type + call-site coercion fixes**
- **Found during:** Task 1
- **Issue:** Routing rig construction through `Session::for_url` necessarily yields an `Arc<ReqwestGatewayApi>` handle; `ReqwestGatewayApi` is not `Clone`, so the helpers cannot return an owned client as before. `&Arc<T>` does not coerce to `&dyn GatewayApi` at cast sites, breaking compilation at 6 rig_stream.rs sites.
- **Fix:** `rig_client`/`rig_client_token`/`rig_client_with` now return `Option<Arc<ReqwestGatewayApi>>`; two `probe_dyn` sites switched to `as_deref()`, four action calls to `&*api`. Behavior identical (deref semantics unchanged).
- **Files modified:** crates/ignition-tui/src/workers/rig_stream.rs
- **Verification:** cargo build + full ignition-tui suite (195 tests) + rig workers' own tests green
- **Committed in:** `0780fcd` (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking API gap, 1 mechanical consequence of the Session handle type). Plus one coordination note: Task 1's `tui_coverage` gate and Task 2's workspace gates could only run after sibling wave-2 agents' uncommitted WIP (08-03 main.rs, 08-06 error.rs) settled on the shared worktree — all gates re-run and green at settle time.
**Impact on plan:** Both fixes were necessary to satisfy the plan's own must-haves (load_for_tui + Session simultaneously; zero construction in the TUI). No scope creep — the core addition is a constructor on the seam 08-02 created, in the spirit of the plan's "selection, secret via Session".

## Issues Encountered

- **Shared-worktree contention (parallel executors):** cargo build/test lock contention repeatedly stalled verification runs (two 10–14 min timeouts); resolved by launching the workspace test as a background job and polling its log. All final gates green: `cargo test --workspace` (0 failures), `cargo clippy --all-targets -- -D warnings` (clean), `cargo fmt --check` (clean once 08-03 committed its formatting), `cargo build -p ignition-cli --no-default-features` (clean).
- No functional issues in the planned work itself — both tasks' verifications passed on first execution of their own code.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Ready for 08-05 (wave 3): `resolve`/`rebuild` still return the `(String, String, Arc<ReqwestGatewayApi>)` triple as planned, and `Session::resolve_loaded` already hands back the selected `Profile` — the `ResolvedContext` struct growth and the `poll_interval` consumer have everything they need.
- Criterion 1 (TUI half) and criterion 4 (TUI half) are satisfied: degrade-and-start for new-surface typos, exit-3 pre-init for resolution failures, and zero second-source client construction in the TUI crate.
- No blockers or concerns for the remaining phase plans.

---
*Phase: 08-foundations-session-core-config-contract*
*Completed: 2026-09-05*

## Self-Check: PASSED

- All modified/created files verified on disk (context.rs, lib.rs, rig_stream.rs, SUMMARY.md)
- All referenced commits verified in history: 0780fcd (Task 1), 2e87b65 (Task 2), f95e835 (core resolve_loaded via 08-03)
- must_have key_links verified in context.rs: `load_for_tui` (4 hits), `Session::resolve_loaded` (3 hits), `Session::for_url` (rig helpers)
- Verification gates at settle time: cargo test --workspace (0 failures), clippy --all-targets -D warnings (clean), fmt --check (clean), no-default-features build (clean), TUI grep sweep (zero ReqwestGatewayApi::new / zero secret_chain)
