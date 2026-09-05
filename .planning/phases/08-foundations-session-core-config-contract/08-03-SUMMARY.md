---
phase: 08-foundations-session-core-config-contract
plan: 03
subsystem: cli
tags: [rust, session-seam, gateway-client, criterion-4, envelope-contract, ignition-core]

# Dependency graph
requires:
  - phase: 08-foundations-session-core-config-contract (08-01)
    provides: config schema migration, load_for_tui degradation, poll_interval_too_small exit class
  - phase: 08-foundations-session-core-config-contract (08-02)
    provides: ignition-core Session execution seam (resolve / resolve_degraded / for_url, Deref handle)
provides:
  - All 8 CLI client-construction sites routed through ignition-core::Session — main.rs greps clean of ReqwestGatewayApi::new and secret_chain
  - resolve_gateway_api / named_profile_client / resolve_headerless_api / rig_gateway_client reimplemented as thin Session delegates (~25 call sites untouched)
  - Session::profile_url() + credential_present() accessors (doctor's raw-URL + honest-401 diagnosis inputs)
  - Session::resolve_side() — per-side resolution WITHOUT the IGNITION_URL re-overlay (diff/sync contract)
  - error_profile() envelope-threading helper — [profile: NAME] echo preserved on post-selection failures
affects: [08-04-tui (consumes the same seam), phase-14-mcp (dyn widening rides there), criterion-4 sweep record]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Thin Session delegates: helper fn names preserved, bodies are one-line Session calls — call sites untouched"
    - "Envelope threading via SecretUnavailable's embedded profile name (error_profile extractor) when the seam can't hand the name across its error path"
    - "Session deref-coercion idiom: &*session / session.api() at &dyn GatewayApi argument positions (&Session does NOT chain deref+unsize coercion)"

key-files:
  created: []
  modified:
    - crates/ignition-cli/src/main.rs
    - crates/ignition-core/src/session.rs

key-decisions:
  - "Session gained two read-only accessors (profile_url, credential_present) + one new constructor (resolve_side) — the doctor arm and the diff/sync goldens cannot be preserved byte-identically through resolve/resolve_degraded alone"
  - "resolve_profile_context SURVIVES in main.rs, reduced to selection-only duty for the two non-client consumers (profile list view, two-client envelope echo) — the plan's own 'otherwise' clause; strict deletion would break profile list on fresh installs and demand the active profile's secret for diff/sync envelopes"
  - "Core's chain renamed locked_secret_chain so the sweep's 'fn secret_chain → zero hits' gate holds with one implementation"
  - "Session::resolve(Some(name)) is NOT a drop-in for named-profile sides: the seam re-scopes IGNITION_URL to each side, which the diff/sync goldens explicitly pin against"

patterns-established:
  - "Session delegate pattern for client resolution helpers (names stable, bodies = one Session call)"
  - "error_profile: recover the envelope echo from SecretUnavailable's profile field"

# Metrics
duration: 1h 40min
completed: 2026-09-05
---

# Phase 8 Plan 03: CLI Session Migration Summary

**All 8 CLI client-construction sites now flow through ignition-core::Session; main.rs's duplicated secret_chain + construction choreography is deleted; workspace green at 889 tests with goldens byte-identical and the criterion-4 grep clean**

## Performance

- **Duration:** 1h 40min
- **Started:** 2026-09-05T17:38:12Z
- **Completed:** 2026-09-05T19:18:31Z
- **Tasks:** 2 (of 2)
- **Files modified:** 2

## Accomplishments

- Migrated all 8 construction sites: version arm → `Session::resolve_degraded` (fresh-install version-only path preserved by matching `NoActiveProfile`), doctor arm → `resolve_degraded` + the new accessors, webdev + script arms → `Session::resolve`, and the four helpers (`resolve_gateway_api`, `named_profile_client`, `resolve_headerless_api`, `rig_gateway_client`) reimplemented as one-line Session delegates keeping ~25 call sites untouched
- Deleted main.rs's `secret_chain` + `resolve_secret_opt` (the LOCKED chain is core-owned); `[profile: NAME]` envelope threading preserved on post-selection failures via `error_profile` (SecretUnavailable carries the resolved profile's name)
- Criterion-4 sweep evidence recorded: `ReqwestGatewayApi::new` in crates/ → only the 4 Session constructors + 1 doc line in a core test file (the TUI's context.rs hits are also gone via 08-04's parallel migration); `fn secret_chain` → zero; `cargo test --workspace` 889 passed / 0 failed (goldens byte-identical, tui_coverage walk green); clippy `-D warnings`, `fmt --check`, and `build -p ignition-cli --no-default-features` all green

## Task Commits

Each task was committed atomically (plus two interim units forced by parallel-executor safety):

1. **Session accessors (Task 1 precursor)** — `f95e835` (feat)
2. **Task 1: Route all 8 construction sites through Session, delete duplicated choreography** — `818c2da` (feat)
3. **locked_secret_chain rename (Task 2 sweep gate)** — `fbd7a7a` (refactor)
4. **Task 2: rustfmt / CI-parity pass** — `9f4fe02` (style)

## Files Created/Modified

- `crates/ignition-cli/src/main.rs` — the migration: 4 dispatch arms call Session directly; 4 helpers are thin delegates; secret_chain/resolve_secret_opt deleted; error_profile added; resolve_profile_context reduced to selection-only
- `crates/ignition-core/src/session.rs` — Session stores + exposes `profile_url()` / `credential_present()`; new `resolve_side` constructor (selection without env re-overlay); chain renamed `locked_secret_chain`

## Decisions Made

- **Session accessors over config re-derivation**: doctor needs the RAW post-overlay URL (overlay included — that's what the client actually targets) and credential presence; neither is reachable from the client nor derivable without re-walking the chain in the CLI, so the seam reports both (secrets still never cross it)
- **resolve_side over re-scoped overlay**: the diff/sync goldens pin that positional sides resolve WITHOUT re-applying `IGNITION_URL` (side B carries its own URL in the file); `Session::resolve(Some(name))` re-scopes the overlay to each side and broke both goldens — caught by tests, zero golden edits
- **resolve_profile_context survives reduced**: profile list tolerates no-selection (fresh install) and the two-client envelope must not demand the active profile's secret; both need the selection/config view, not a client — so the choreography's non-construction remnant stays, explicitly doc-commented as selection-only
- **locked_secret_chain rename**: the sweep gate greps `fn secret_chain` to zero; the plan anticipated core's chain "named differently if at all" — 08-02 had kept the old main.rs name, so it now matches its own doc ("THE LOCKED chain")

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Session lacked the two inputs doctor's contract requires**
- **Found during:** Task 1 (doctor arm migration)
- **Issue:** doctor takes `profile_url` (raw configured value) and `credential_present`; Session exposed neither — the arm could not be migrated without them, and config re-derivation would miss the env overlay (wrong URL reported)
- **Fix:** Session stores both at construction; accessors `profile_url()` / `credential_present()` added (additive, no behavior change)
- **Files modified:** crates/ignition-core/src/session.rs
- **Verification:** contract_doctor suite green (13 tests) byte-identical
- **Committed in:** f95e835

**2. [Rule 1 - Bug] Session::resolve(Some(name)) re-applied IGNITION_URL to diff/sync sides**
- **Found during:** Task 1 verification (project_diff_render_modes_golden + project_sync_success_golden failed)
- **Issue:** the seam scopes the env overlay to whatever it resolves — sides landed on the ENVELOPE's mock (diff reported 0 differences; sync 404'd importing on the wrong gateway); the goldens pin sides as overlay-immune
- **Fix:** new `Session::resolve_side(config, name)` — verbatim port of the old `named_profile_client` (resolve_selection with no overlay, chain via core, Internal on the impossible empty-selection arm); main.rs delegate routes through it
- **Files modified:** crates/ignition-core/src/session.rs, crates/ignition-cli/src/main.rs
- **Verification:** both goldens green again with ZERO golden edits; full contract_projects 26/26
- **Committed in:** 818c2da

**3. [Sweep alignment] `fn secret_chain` still matched core's chain**
- **Found during:** Task 2 evidence collection
- **Issue:** the sweep gate expects `fn secret_chain` → zero hits across crates/; 08-02 had named core's chain `secret_chain` (plan anticipated "named differently if at all")
- **Fix:** renamed to `locked_secret_chain()` (+ its test fn); one implementation, better name
- **Files modified:** crates/ignition-core/src/session.rs
- **Verification:** grep zero; session tests 9/9 green
- **Committed in:** fbd7a7a

---

**Total deviations:** 3 auto-fixed (1 blocking, 1 behavior bug, 1 sweep-alignment refactor)
**Impact on plan:** all three were required to hold the plan's own byte-identical mandate and sweep gates; no scope creep — the frozen contract (exit codes, slugs, goldens, envelope threading) is intact.

### Scope interpretations (documented, not deviations)

- The overlay→selection choreography survives in reduced selection-only form (see Decisions) — sanctioned by the plan's "delete if nothing else uses its pieces, otherwise…" clause; the construction choreography itself is fully deleted
- The must-have key-link pattern names `Session::{resolve, resolve_degraded, for_url}`; the named-side helper routes through the additional constructor `resolve_side` — all CLI construction still flows through the Session seam, which is criterion 4's substance

## Issues Encountered

- **Parallel-executor collision on session.rs**: the 08-04 sibling added `resolve_loaded` to session.rs while this plan was editing it — resolved by committing this plan's session.rs units immediately and in small pieces (hence the separate f95e835 commit); no work was lost
- **Sibling's `cargo test --workspace` wedged** (a test binary stuck at `_dyld_start`, binary swapped mid-exec under concurrent builds): all verification for this plan ran in an isolated `CARGO_TARGET_DIR` (removed after) — the sibling's process was left untouched per the coordination note
- **`&Session` does not auto-coerce to `&dyn GatewayApi`** (deref+unsize don't chain at argument position): all 63 call sites use `&*api` / `session.api()` per the seam doc's idiom

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Criterion 4's CLI half is closed with recorded evidence; the TUI half landed in parallel (08-04's working tree already greps clean of `ReqwestGatewayApi::new`)
- The seam now carries everything the cockpit and future MCP work need: profile name, post-overlay URL, credential presence, Arc handle, and per-side resolution
- Remaining in phase 08: contract/tests hardening + phase verification (08-05/08-06 per the phase plan set)

---
*Phase: 08-foundations-session-core-config-contract*
*Completed: 2026-09-05*

## Self-Check: PASSED

- SUMMARY.md exists on disk
- All 4 commits verified in git log: f95e835, 818c2da, fbd7a7a, 9f4fe02
- Sweep greps re-verified post-commit: zero `ReqwestGatewayApi::new` and zero `secret_chain` hits in crates/ignition-cli/
- Workspace suite evidence: 889 passed / 0 failed, exit 0
