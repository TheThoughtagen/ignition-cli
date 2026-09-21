---
phase: 12-tui-theming-degradation
plan: 02
subsystem: ui
tags: [ratatui, tui, theming, config-resolution, lenient-degradation, detect-tier, palette]

# Dependency graph
requires:
  - phase: 12-tui-theming-degradation
    plan: 01
    provides: "ui/theme.rs token surface — Theme::by_name/resolve, Tier (Default=Mono), detect_tier over &dyn Fn(&str)->Option<String>, all-Reset mono at every tier"
  - phase: 08-config-profiles-and-session-seam
    provides: "[ui].theme: Option<String> on Config (lenient_ui deserializer) + the ResolvedContext carrier → one-block adoption → switch-chain adoption pattern this plan re-rides"
provides:
  - "ResolvedContext.palette: the resolved theme palette ([ui].theme at the env-detected tier) — built in build_context, the ONLY real-env detect_tier site"
  - "resolve_palette(name, tier): pure warn+default fallback helper (unknown name NEVER fails the load)"
  - "AppState.palette: defaulted to default-theme @ Mono (deterministic, no env reads at construction), adopted by run_loop and switch_profile"
  - "Resolution test suite: 7 new tests pinning known-name equality, unknown-name fallback, wrong-typed degradation, mono tier-independence, and the all-Reset state default"
affects: [12-03 (token migration of dashboard/logs renders against state.palette), 12-04 (CI Color:: grep + render consumption)]

# Tech tracking
tech-stack:
  added: [tracing (workspace graph, now a direct ignition-tui dep — warn for unknown theme names)]
  patterns:
    - "Ride-along adoption (08-05 poll_interval pattern): ResolvedContext carrier → run_loop one-block adoption → switch_profile same-statement-group adoption"
    - "Computed-equality env-coupled assertion: fallback tests compare resolve() output against resolve_palette(None, detect_tier(&env)) computed in-process under the same ambient env"
    - "Slot-by-slot Reset pin (12 named slots) — stays honest if Palette grows a field"

key-files:
  created: []
  modified:
    - crates/ignition-tui/src/context.rs
    - crates/ignition-tui/src/state.rs
    - crates/ignition-tui/src/lib.rs
    - crates/ignition-tui/src/update.rs
    - crates/ignition-tui/Cargo.toml

key-decisions:
  - "resolve_palette is private and pure over (name, tier) — build_context is the single real-environment detection site (detect_tier(&|k| std::env::var(k).ok())); tests pin the helper at fixed tiers"
  - "Unknown theme name warns via tracing::warn! and falls back to default — the lenient-degradation contract decided at TUI-resolution time; wrong-TYPED values degrade earlier via the existing lenient_ui deserializer"
  - "AppState default palette = default theme @ Tier::Mono (all-Reset) — deterministic construction, zero env reads; run_loop overwrites with real detection"
  - "Profile switch adopts ctx.palette in the same statement group as poll_interval — uniform adoption closes the silent-drop trap class even though theme is global config"

patterns-established:
  - "Palette rides ResolvedContext like poll_interval — every future per-context TUI surface value follows the same carrier/adoption choreography"
  - "Mono-theme wiring tests are tier-independent by construction (authored all-Reset at every tier) — no ambient COLORTERM/TERM coupling in CI"

# Metrics
duration: 14 min
completed: 2026-09-14
---

# Phase 12 Plan 02: Theme Wiring Summary

**[ui].theme resolved to a concrete Palette at the env-detected tier on ResolvedContext, carried onto AppState through both adoption sites (run_loop + profile switch), with 7 new tests pinning known/unknown/wrong-typed/mono-tier-independent behavior — 225/225 green, clippy/fmt clean, lean build unaffected**

## Performance

- **Duration:** 14 min
- **Started:** 2026-09-14T16:04:21Z
- **Completed:** 2026-09-14T16:18:33Z
- **Tasks:** 2
- **Files modified:** 5 (4 source, 1 Cargo manifest + Cargo.lock)

## Accomplishments
- `resolve_palette(name, tier)` — the pure warn+default fallback helper in context.rs: unknown `[ui].theme` names warn via `tracing::warn!` and land the default theme at the same tier; the cockpit always starts
- `ResolvedContext.palette` — resolved in `build_context` from `config.ui.theme` at `detect_tier(&|k| std::env::var(k).ok())`, the module's ONLY real-environment call site
- `AppState.palette` — defaulted to default-theme @ Mono (deterministic, no env reads at construction; `AppState::new()` signatures untouched, all ~100 existing callsites stay valid)
- Both adoption sites landed: run_loop's one-block adoption and switch_profile's poll_interval statement group (the 08-05 ride-along pattern, closing the silent-drop trap class)
- 7 new tests: 3 pure-helper (known-name equality at fixed tiers, unknown == None == default, none-is-default), 3 resolve-level TOML fixtures (mono all-Reset tier-independent pin, unknown-name computed-equality fallback, wrong-typed `theme = 42` degradation), 1 state default (slot-by-slot all-Reset)

## Task Commits

Each task was committed atomically:

1. **Task 1: resolve_palette + ResolvedContext.palette + AppState.palette + both adoption sites** - `8a7bd50` (feat)
2. **Task 2: Resolution tests — known name, unknown-name fallback, wrong-typed degradation, mono tier-independence** - `9518428` (test)

## Files Created/Modified
- `crates/ignition-tui/src/context.rs` — `resolve_palette` helper, `ResolvedContext.palette` field, `build_context` wiring (detect_tier over real env), 6 new tests
- `crates/ignition-tui/src/state.rs` — `AppState.palette` field + deterministic Default entry, all-Reset default test
- `crates/ignition-tui/src/lib.rs` — run_loop adoption block gains `state.palette = ctx.palette;`
- `crates/ignition-tui/src/update.rs` — switch_profile adopts `ctx.palette` beside the poll_interval line
- `crates/ignition-tui/Cargo.toml` (+ `Cargo.lock`) — `tracing = { workspace = true }` added for the unknown-theme warning

## Decisions Made
- **resolve_palette is private + pure:** only build_context touches the real environment — the helper stays fixed-tier testable and the env-snapshot injection (12-01's pinned signature) is honored exactly
- **Fallback equality as computed-equality:** the banana/42 fixture tests compare `ctx.palette` against `resolve_palette(None, detect_tier(&|k| std::env::var(k).ok()))` computed in-process under the same ambient env — proves the fallback flowed through build_context, not just the helper, without depending on CI terminal type
- **Mono fixture pin is slot-by-slot Reset:** the mono theme is authored all-Reset at every tier, so the config→palette wiring test holds under ANY ambient COLORTERM/TERM
- **State default = default @ Mono:** the strictest tier at construction means anything reading wrong at Mono is a bug, not a cosmetic nit — matches Tier::default() doctrine from 12-01

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added `tracing` as a direct ignition-tui dependency**
- **Found during:** Task 1 (resolve_palette implementation)
- **Issue:** the plan's `tracing::warn!` for unknown theme names has no dependency declared in ignition-tui (tracing was only in core and the bin); first build failed with `unresolved module or unlinked crate tracing`
- **Fix:** `tracing = { workspace = true }` added to `[dependencies]` (first attempt landed in dev-dependencies and was corrected immediately)
- **Files modified:** crates/ignition-tui/Cargo.toml, Cargo.lock
- **Verification:** `cargo build -p ignition-tui` clean; workspace clippy `-D warnings` clean
- **Committed in:** 8a7bd50 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking dependency)
**Impact on plan:** The dependency was required by the plan's own warn-and-fallback design; zero scope change.

## Issues Encountered
None — both tasks verified first-pass after the dependency fix (one misplaced Cargo.toml section caught and corrected before the first successful build).

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- 12-03/12-04 (token migration of dashboard/logs renders) can consume `state.palette` directly — it is defaulted, adopted, and switch-safe; renders read the palette slots instead of literal Colors
- The plan's structural pin (`state.palette = ctx.palette` in both lib.rs and update.rs) is in place for any later adoption audit
- Nothing renders differently yet — this plan ships wiring only; visual behavior changes land with the screen token migration
- Lean build unaffected: theme code lives entirely in ignition-tui (`cargo build -p ignition-cli --no-default-features` clean)

---
*Phase: 12-tui-theming-degradation*
*Completed: 2026-09-14*

## Self-Check: PASSED

- All 4 modified source files exist on disk ✓
- Commits 8a7bd50 + 9518428 present in git log ✓
- `pub palette` present on ResolvedContext (context.rs) and AppState (state.rs) ✓
- `cargo test -p ignition-tui`: 225/225 passed ✓
- `cargo clippy --workspace --all-targets -- -D warnings`: clean ✓
- `cargo fmt --all --check`: clean ✓
- `cargo build -p ignition-cli --no-default-features`: clean ✓
