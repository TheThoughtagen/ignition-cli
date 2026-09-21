---
phase: 12-tui-theming-degradation
plan: 01
subsystem: ui
tags: [ratatui, tui, theming, color-tiers, capability-degradation, style-tokens, no-color, colorterm]

# Dependency graph
requires:
  - phase: 08-config-profiles-and-session-seam
    provides: `[ui].theme: Option<String>` config plumbing (lenient_ui warn+default deserializer, load_for_tui degradation contract) that 12-02 will resolve through
provides:
  - "ui/theme.rs: 12-slot semantic Palette — the style-token contract (screens name slots, never Colors)"
  - "Tier enum (Truecolor/C256/C16/Mono, Default=Mono) + pure detect_tier over an injected env snapshot"
  - "Theme registry: default/mono/dark/light, each AUTHORED at all four tiers (no runtime quantization)"
  - "Style helpers with centralized mono adaptation (error→BOLD, selection→REVERSED)"
  - "Structural test suite: registry, tier resolution, mono all-Reset contract, Rgb containment, c16 purity"
affects: [12-02 (theme wiring into context::resolve + AppState), 12-03 (CI Color:: grep — theme.rs is the exempt file), 12-04 (token migration of dashboard/logs)]

# Tech tracking
tech-stack:
  added: [] # zero new dependencies — Palette is a struct of ratatui::style::Color variants
  patterns:
    - "Authored per-tier palettes (never runtime RGB→256→16 quantization — CrosstermBackend passes 1:1)"
    - "Mono marker pattern: helpers detect mono tier by slot==Reset (a Reset error slot IS mono) and substitute modifiers"
    - "Pure env-snapshot injection (&dyn Fn(&str) -> Option<String>) for capability detection — no real-env test access"

key-files:
  created:
    - crates/ignition-tui/src/ui/theme.rs
  modified:
    - crates/ignition-tui/src/ui/mod.rs

key-decisions:
  - "Mono adaptation centralized in style helpers, keyed on slot==Reset as the mono marker — screen code composes tokens with inline modifiers and cannot accidentally ship a color-only distinction at Mono tier"
  - "by_name is a find over const THEMES registry (case-sensitive), returning Option — caller warns + falls back per the lenient-degradation contract"
  - "default theme: truecolor/c256/c16 IDENTICAL (named ANSI + Reset slots) — zero visual regression by construction"
  - "dark c256 approximations hand-picked (Indexed 253/244/255/60/238/75/203/220/41); light truecolor darkens warning/accent via Rgb(176,128,0)/Rgb(0,90,170) for white-bg contrast — hues explicitly adjustable post-UAT, tests pin structure only"

patterns-established:
  - "Style-token contract: screens never name a Color, they name a Palette slot via theme:: helpers"
  - "Tier detection is a pure function — 12-02 wires it as detect_tier(&|k| std::env::var(k).ok())"
  - "C16 purity = {Reset} ∪ 16 ANSI named variants (Reset allowed only where intentional, documented in-test)"

# Metrics
duration: 8 min
completed: 2026-09-14
---

# Phase 12 Plan 01: Theme Token Module Summary

**Style-token module with 12-slot Palette, four themes authored at four capability tiers (no runtime quantization), centralized mono adaptation, and a pure env-snapshot detect_tier — 16 structural tests green, clippy/fmt clean**

## Performance

- **Duration:** 8 min
- **Started:** 2026-09-14T15:52:29Z
- **Completed:** 2026-09-14T16:00:49Z
- **Tasks:** 2
- **Files modified:** 2 (1 created, 1 modified)

## Accomplishments
- `ui/theme.rs` — the cockpit's single style-token module: 12-slot `Palette`, `Tier` (Truecolor/C256/C16/Mono, Default=Mono), `Theme` registry (`default`/`mono`/`dark`/`light`) with `by_name`/`resolve`, per-slot style helpers, pure `detect_tier`
- Four themes AUTHORED at all four tiers — degradation is an authoring fact, not runtime behavior; mono = all-Reset at EVERY tier (tier-independent, pin-ready for 12-02's ambient-COLORTERM wiring tests)
- Mono adaptation centralized: `error()` → BOLD and `selection()` → REVERSED when the slot is the Reset mono marker; all other slots map to plain fg
- 16 structural tests: registry (case-sensitive None on unknowns), tier resolution equality, mono all-Reset contract, Rgb confined to truecolor, c16 ANSI-16 purity, helper mono mapping, full detect_tier env matrix

## Task Commits

Each task was committed atomically:

1. **Task 1: Palette, Tier, Theme registry, style helpers — four themes authored at four tiers** - `70941cd` (feat)
2. **Task 2: detect_tier — pure capability detection over an env snapshot** - `af301f4` (feat)

## Files Created/Modified
- `crates/ignition-tui/src/ui/theme.rs` (created) — the ONLY module allowed to name `Color::`; tokens, tiers, registry, helpers, detect_tier, 16 tests
- `crates/ignition-tui/src/ui/mod.rs` (modified) — `pub mod theme;` added alphabetically alongside the screen modules

## Decisions Made
- **Mono marker = slot==Reset:** helpers detect the mono tier from the palette value itself rather than passing Tier everywhere — the palette IS the resolved tier, so no second parameter can drift out of sync
- **`by_name` over const THEMES with `find`:** registry-add only; case-sensitive per plan ("DARK" → None)
- **default theme carries Reset slots at color tiers deliberately** (documented in-test): they render as terminal defaults — that IS the zero-regression look; c16-purity test allows {Reset} ∪ ANSI-16 and documents why
- **dark c256 values are nearest-cube approximations** of the truecolor hues (hand-picked Indexed values); light truecolor darkens warning/accent with Rgb per plan example — all hues flagged ADJUSTABLE POST-UAT, tests never pin them

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None — both tasks verified first-pass (two fmt reflows on the new file were applied immediately; no logic changes).

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- 12-02 (theme wiring) can consume exactly what it needs: `Theme::by_name` + `resolve` + `detect_tier(&|k| std::env::var(k).ok())` — the detect_tier signature was pinned to that closure coercion
- 12-02's wiring tests can pin the mono theme's tier-independence against ambient COLORTERM (structurally guaranteed now)
- 12-03's CI grep has its single exempt file (`src/ui/theme.rs`); grep-verified only the 10 known dashboard/logs literals remain outside it
- Nothing consumes the module yet — zero behavioral change shipped in this plan

---
*Phase: 12-tui-theming-degradation*
*Completed: 2026-09-14*

## Self-Check: PASSED

- `crates/ignition-tui/src/ui/theme.rs` exists on disk ✓
- `pub mod theme;` present in ui/mod.rs ✓
- Commits 70941cd + af301f4 present in git log ✓
- `cargo test -p ignition-tui`: 218/218 passed ✓
- `cargo clippy -p ignition-tui --all-targets -- -D warnings`: clean ✓
- `cargo fmt --all --check`: clean ✓
