---
phase: 12-tui-theming-degradation
plan: 04
subsystem: ui
tags: [ratatui, tui, theming, palettes, ansi, sgr, truecolor, degradation, uat]

# Dependency graph
requires:
  - phase: 12-01
    provides: Palette/Tier/theme.rs token module — four authored tiers per theme, detect_tier, style helpers, mono adaptation
  - phase: 12-02
    provides: [ui].theme config wiring — ResolvedContext.palette + both adoption sites, unknown-theme warn+fallback
  - phase: 12-03
    provides: literal→token migration + CI tokenization grep + tab-bar emphasis token + cursor hide
provides:
  - UAT-tuned dark/light palettes that are visibly distinct from default at a glance
  - Dormant palette slots (border/title/header/accent) wired into the dashboard — the last dead consumers
  - Wire-level SGR proof of per-theme distinctness at C256 and Truecolor tiers (pty captures)
  - Negative-test evidence that the CI tokenization grep actually fails on a planted Color:: literal
affects: [phase-verification, uat, any future screen that consumes border/title/header/accent slots]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "wire-proof harness: pty_capture.py forces TERM=xterm-256color, pops COLORTERM for C256; a COLORTERM=truecolor copy proves the Rgb tiers"
    - "slot-wiring pattern: Block::bordered().border_style(theme::border(p)).title_style(theme::title(p)) — Reset slots keep default/mono byte-identical"

key-files:
  created:
    - ".planning/phases/12-tui-theming-degradation/12-04-SUMMARY.md"
  modified:
    - "crates/ignition-tui/src/ui/theme.rs"
    - "crates/ignition-tui/src/ui/dashboard.rs"

key-decisions:
  - "12-04: dark/light hues HARDENED after UAT found the first palettes indistinguishable from default at a glance — dark border/title/header/emphasis now carry the accent blue family; hues were the plan's reserved post-UAT discretion and no test pinned them"
  - "12-04: [Rule 1] the border/title/header/accent slots were wired into the dashboard — they existed in the contract but NO consumer attached them (12-03 threaded only error/warning/selection + tab-bar emphasis), so panes rendered default-styled in every theme; wiring preserves default/mono (Reset slots) byte-identically"
  - "12-04: light c256 accent corrected from green Indexed(40) to blue Indexed(26) to match the theme's truecolor blue identity; light selection_bg is a LIGHT gray so black-text structure stays visible even on a dark terminal"

patterns-established:
  - "Wire-proof capture: uniq'd SGR-set diffs vs default must show border/header/accent codes appearing — more than a red-shade swap proves theme distinctness"
  - "Zero-regression proof: default and mono pty captures must be identical SGR sets pre/post any theming change"

# Metrics
duration: ~50 min (Task 1 prior segment + UAT round-trip + tuning continuation)
completed: 2026-09-14
---

# Phase 12 Plan 04: UAT Verification + Palette Tuning Summary

**Negative-tested CI tokenization gate both directions, then UAT-tuned dark/light palettes into visibly distinct SGR sets (border/title/accent codes proven on the wire at C256 + truecolor) after the user found the first hues indistinguishable — plus wired the dormant border/title/header/accent slots that made every theme render default-styled panes.**

## Performance

- **Duration:** ~50 min active (Task 1 prior segment; Task 2 spans the UAT checkpoint round-trip)
- **Started:** 2026-09-14 (Task 1) / continuation 2026-09-14T18:1xZ
- **Completed:** 2026-09-14T19:05Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- **CI tokenization grep negative-tested (Task 1, by plan design — no commit, tree byte-identical):** planted `Color::Red` in `crates/ignition-core/src/actions/tags.rs` (line ~670) → the CI grep failed with exit 1 naming the violating file; reverted to the clean tree → exit 0. Enforcement proven in BOTH directions.
- **UAT executed (Task 2 checkpoint):** user ran all four themes live on a real terminal. Verdict: "nothing seems any different between the different ones." Wire-level diagnosis (orchestrator, pty captures): the mechanism worked end-to-end (config → resolve_palette → state.palette → render → ANSI), and each theme DID emit different SGRs — but the differences were palette timidity (near-white/near-gray shades) that no human can distinguish at a glance.
- **Palette hardening (the "tune it" fix):** dark's border/title/header/emphasis now carry the accent blue family (truecolor `Rgb(52,152,219)`/`Rgb(120,200,255)`; c256 `Indexed(69)`/`Indexed(117)`; c16 `Blue`/`LightBlue`); light's selection_bg is a light gray (`Rgb(208,214,220)`/`Indexed(252)`), its c256 accent corrected from green `Indexed(40)` to blue `Indexed(26)`.
- **Dormant slots wired ([Rule 1]):** `border`/`title`/`header`/`accent` were contract slots with zero consumers — dashboard panes rendered default-styled in EVERY theme (the true root cause of the UAT verdict). Now: four pane blocks use `border_style(theme::border(p))` + `title_style(theme::title(p))`; the sessions table header uses `theme::header(p)` under inline BOLD; the status-line profile name rides `theme::accent(p)`.
- **Wire-level re-proof (fresh pty captures against the tuned binary):**

  | Comparison | SGR delta (ESC-inclusive, sorted-unique sets) |
  |---|---|
  | default old vs new | **IDENTICAL** — zero-regression proven |
  | mono old vs new | **IDENTICAL** — zero-regression proven |
  | dark old → new (C256) | −`38;5;255` → +`38;5;117` (titles) +`38;5;69` (borders) +`38;5;75` (accent) |
  | light old → new (C256) | +`38;5;26` (accent); black `38;5;0` now also styles borders/titles |
  | dark vs default (C256) | adds `38;5;69`, `38;5;117`, `38;5;75`, `38;5;203` — far more than the old red-shade swap |
  | light vs default (C256) | adds `38;5;0` (black border/title), `38;5;26` (blue accent) |
  | dark vs default (TRUECOLOR) | adds `38;2;52;152;219`, `38;2;120;200;255`, `38;2;231;76;60` — Rgb tiers emit truecolor SGR proven |
  | light vs default (TRUECOLOR) | adds `38;2;0;90;170` (+`38;5;0` named-color forms) |
  | mono (C256 + truecolor) | zero color-set codes at both tiers — all-Reset contract intact |

  Harness: `/tmp/ign-p12-rigs/pty_capture.py` (TERM=xterm-256color, COLORTERM popped → C256) and `pty_capture_truecolor.py` (COLORTERM=truecolor forced → Truecolor tier).

## Task Commits

1. **Task 1: Negative-test the tokenization grep** — no commit (by plan design: plant → observe CI grep fail exit 1 → revert byte-identical; a commit would contradict the zero-residue purpose)
2. **Task 2: UAT verification + palette tuning** — `756e888` (feat(12-04): harden dark/light palettes for visual distinctness (UAT tuning))

## Files Created/Modified
- `crates/ignition-tui/src/ui/theme.rs` — DARK/LIGHT palettes hardened (structure untouched: slot set, tier purity, mono all-Reset, DEFAULT/MONO byte-untouched); module doc records the exercised post-UAT hue reservation
- `crates/ignition-tui/src/ui/dashboard.rs` — pane blocks + sessions header + status-line profile name wired to the border/title/header/accent slots

## Decisions Made
- **Hue hardening over acceptance:** the plan explicitly reserved hue tuning post-UAT ("tests pin structure, not hues") — the UAT verdict exercised exactly that reservation. Structural tests needed zero changes (they never pinned hues), which validated the doctrine.
- **Wire the dormant slots (deviation, see below):** hue tuning alone could not fix the UAT verdict — the biggest visual elements (pane borders/titles) never consumed the palette. Wiring `theme::border/title/header/accent` into the dashboard is the smallest fix that makes the themes actually visible, and Reset slots keep default/mono wire-identical (verified by capture).
- **Light theme degradation honesty:** light keeps black text (correct for its intended white-bg home) but black borders/titles + light-gray selection render as visible inverted structure on a dark terminal too — a mis-configured light theme degrades to "obviously inverted", never "invisible".

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] border/title/header/accent slots had no consumers — dashboard rendered default-styled in every theme**
- **Found during:** Task 2 (wire-level re-proof: pty captures showed NO `38;5;69`/border codes even pre-tuning — the tuning alone could not make border codes "appear" as the plan's proof required)
- **Issue:** 12-03 threaded the palette into dashboard render fns but only attached `error`/`warning`/`selection` + the tab-bar `emphasis`; the four pane `Block::bordered()` calls carried no `border_style`/`title_style`, the sessions table header was BOLD-only, and the status line was entirely raw. The theming contract's most visible slots were dead code — the direct cause of the UAT "nothing seems any different" verdict.
- **Fix:** `.border_style(theme::border(p))` + `.title_style(theme::title(p))` on the four pane blocks; sessions header `.style(theme::header(p).add_modifier(BOLD))`; status-line profile name `Span::styled(name, theme::accent(p))`. The plan's "don't touch screen files" constraint protected default/mono zero-regression and structure pins — both preserved: default and mono pty captures are identical SGR sets pre/post, and all 229 tests pass unchanged.
- **Files modified:** `crates/ignition-tui/src/ui/dashboard.rs`
- **Verification:** wire captures (delta table above), 229 tui tests green, clippy -D warnings clean, fmt clean, tokenization grep clean
- **Committed in:** `756e888`

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** The slot wiring was necessary — without it the UAT fix is unachievable and the wire-proof criterion (border/header/accent codes must appear) is unsatisfiable. No structural tests changed; no scope creep beyond the four dashboard consumers.

## Issues Encountered
- **Remaining unwired slots (follow-up, not a blocker):** `text`/`muted`/`indicator`/`success` slots still have no dashboard consumers (body lines like "Loading…", gauges, the `▸` marker render raw), and other screens (logs/tags/alarms/projects/rig) likely share the unstyled-block pattern. Scoped out deliberately — this plan tunes + proves the dashboard surface; a consumer-completion pass is a natural candidate for the phase verification round or a future polish plan.
- **Sessions-panel selection_bg not visible in the wire capture:** with the 401 gateway the sessions panel renders its Error state, so the table (and its `48;5;252`/`Rgb` selection fill) never draws in the pty window. The selection slot remains covered by the 12-03 buffer tests; border/title/header/accent codes all appear as required.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- **Phase 12 is 4/4 plans complete, PENDING final user re-verification of the tuned dark/light themes** (`ign tui` with `[ui] theme = "dark"` / `"light"` should now be obviously distinct: blue-chromed panes vs the inverted light look)
- After re-verification passes, the phase is ready for `/gsd-verify-work 12` / transition
- Phase 13 prerequisite still open: confirm licensed-Historian rig access before planning

---
*Phase: 12-tui-theming-degradation*
*Completed: 2026-09-14*

## Self-Check: PASSED

- [x] `crates/ignition-tui/src/ui/theme.rs` exists (tuned palettes committed)
- [x] `crates/ignition-tui/src/ui/dashboard.rs` exists (slot wiring committed)
- [x] Commit `756e888` present in history (`feat(12-04): harden dark/light palettes…`)
- [x] Prior plan commits intact (12-01/12-02/12-03 + docs commits)
- [x] Wire evidence: default/mono identical SGR sets pre/post; dark +`69`/`117`/`75`, light +`26`; truecolor `38;2;…` proven; mono zero color codes
- [x] 229 tui tests / clippy -D warnings / fmt / tokenization grep all green at commit
