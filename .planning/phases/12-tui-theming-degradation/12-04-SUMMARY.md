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
    - "crates/ignition-tui/src/ui/logs.rs"
    - "crates/ignition-tui/src/ui/rig.rs"
    - "crates/ignition-tui/src/ui/projects.rs"
    - "crates/ignition-tui/src/ui/tags.rs"
    - "crates/ignition-tui/src/ui/alarms.rs"
    - "crates/ignition-tui/src/ui/profiles.rs"
    - "crates/ignition-tui/src/ui/mod.rs"

key-decisions:
  - "12-04: dark/light hues HARDENED after UAT found the first palettes indistinguishable from default at a glance — dark border/title/header/emphasis now carry the accent blue family; hues were the plan's reserved post-UAT discretion and no test pinned them"
  - "12-04: [Rule 1] the border/title/header/accent slots were wired into the dashboard — they existed in the contract but NO consumer attached them (12-03 threaded only error/warning/selection + tab-bar emphasis), so panes rendered default-styled in every theme; wiring preserves default/mono (Reset slots) byte-identically"
  - "12-04: light c256 accent corrected from green Indexed(40) to blue Indexed(26) to match the theme's truecolor blue identity; light selection_bg is a LIGHT gray so black-text structure stays visible even on a dark terminal"
  - "12-04 round 2: DARK text/muted tinted (text Rgb(196,214,235)/Indexed(189) @ 14.2:1 AAA; muted Rgb(110,140,170)/Indexed(67) @ 6.0:1 AA) and EVERY screen's body content routed through theme::text/muted — the UAT verdict was 'most text is still just white everywhere' because no render site consumed the text/muted slots; c16 keeps White/DarkGray (readability-first step-down); DEFAULT/MONO untouched, byte-identical SGR sets re-proven pre/post"

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

---

# UAT Tuning Round 2 (2026-09-14, continuation)

**User verdict on the round-1 tuning: "nice, but most text is still just white everywhere. I see where the things that do change, do, its just not all that much overall."** Diagnosis: every theme's `text` slot was near-white (dark `Indexed(253)` ≈ terminal default), and NO render site consumed `theme::text`/`theme::muted` — body content (table rows, labels, hints, timestamps, modal text) rendered plain `Style::default()`. Round 1 tinted the chrome; round 2 tints the content.

## What Changed

1. **DARK `text`/`muted` slots tinted** (`theme.rs`, palette-only — structure untouched):
   - `text`: truecolor `Rgb(196,214,235)`, c256 `Indexed(189)` — a soft blue-tinted off-white that is clearly NOT default white but comfortable for long reading. c16 keeps `White` (readability wins at 16 colors — the honest step-down).
   - `muted`: truecolor `Rgb(110,140,170)`, c256 `Indexed(67)` — clearly dimmer and blue. c16 keeps `DarkGray`.
   - LIGHT palette untouched from `756e888` (its black text was already distinct); MONO untouched (all-Reset).
2. **Body-content sweep across every screen** — plain body text → `theme::text`, secondary text → `theme::muted`:
   - `dashboard.rs`: status/metrics panels split into the k9s two-tone field-row shape (`muted` label column + `text` value column, byte-identical row text); modules rows, session rows, "Loading…"/error-message lines ride `text`; the status line's prefix + tail (freshness/busy/hints) ride `muted` (profile name keeps `accent`).
   - `logs.rs`: timestamps + logger names ride `muted`, messages ride `text` (LEVEL spans keep their protected semantic slots); status row rides `muted`; the stream pane block wires `border`/`title` (still dormant after round 1).
   - `rig.rs`: field rows two-tone; services/volumes rows ride `text`; the "none running" hint rides `muted`+DIM; raw compose-log lines ride `text`; status row `muted`; both pane blocks themed.
   - `projects.rs`/`tags.rs`/`alarms.rs`: field rows two-tone, table rows ride `text`, table headers wired to the `header` slot + BOLD (matching the dashboard), error-message lines ride `text` (BOLD banners untouched), hints/footers/status rows ride `muted`; all pane blocks themed; the tags `refresh_hint` rides `muted`+DIM.
   - `mod.rs`: modal bodies ride `text`, footer hints ride `muted`, Input hints `muted`+DIM; every modal block (Confirm/Input/Result_/all five action menus/Projects menu) wires `border`/`title`; the Projects menu's descriptions ride `muted`+DIM, labels `text`; the tab bar's base style rides `muted` (inactive tab labels tint; active keeps emphasis+BOLD).
   - `profiles.rs`/`alarms.rs` modals: palette-threaded (`render_overlay`/`render_ack_overlay` signatures grew the palette param); names/fields ride `text`, hints/labels `muted`, blocks themed.

## Readability Evidence (TUIX-04, computed)

| Color | Relative luminance | Contrast vs black | WCAG verdict |
|---|---|---|---|
| `dark.text` Rgb(196,214,235) | 0.658 | **14.17:1** | AAA body text (≥7:1) |
| `dark.muted` Rgb(110,140,170) | 0.250 | **5.99:1** | AA body text (≥4.5:1) |
| (reference) pure white | 1.000 | 21.00:1 | — |
| (reference) old near-white Rgb(220,223,228) | — | 15.72:1 | barely-tinted predecessor |
| text ↔ muted separation | — | 2.36:1 | visible two-tone hierarchy |
| c256 Indexed(189) ≈ rgb(215,215,255) | — | 15.05:1 | mirrors truecolor |
| c256 Indexed(67) ≈ rgb(95,135,175) | — | 5.57:1 | mirrors truecolor |

The text slot sacrifices ~1.5 contrast points vs the old near-white (14.17 vs 15.72 — still deep in AAA territory) in exchange for an always-visible blue identity; muted is deliberately a full step down so labels/hints read as secondary.

## Wire-Level Re-Proof (fresh pty captures, `/tmp/ign-p12-rigs/`)

Pre-change binary built at HEAD `0175bdf`; post-change at `27df248`; C256 via `pty_capture.py` (COLORTERM popped), truecolor via `pty_capture_truecolor.py`.

| Comparison | Unique-SGR-set delta |
|---|---|
| **default pre vs post (C256 + truecolor)** | **IDENTICAL** — zero-regression re-proven |
| **mono pre vs post (C256)** | **IDENTICAL** — all-Reset contract intact |
| dark pre → post (C256) | +`38;5;189` (text) +`38;5;67` (muted) — **the tinted body code appears where default emits none** |
| dark pre → post (truecolor) | +`38;2;196;214;235` +`38;2;110;140;170` — authored Rgb values proven on the wire |
| light pre → post (C256) | +`38;5;8` only (DarkGray muted, from the body wiring — its palette is untouched) |
| dark vs default post (C256) | dark-only: `38;5;189` `38;5;67` `38;5;69` `38;5;75` `38;5;117` `38;5;203` — chrome AND body |
| dark vs default post (truecolor) | dark-only: `38;2;196;214;235` `38;2;110;140;170` `38;2;52;152;219` `38;2;120;200;255` `38;2;231;76;60` |

## Gates

- `cargo test -p ignition-tui`: **231 passed** (229 prior + 2 new render-site pins: `body_text_renders_from_the_text_and_muted_slots` [dashboard text/muted], `log_metadata_and_message_render_from_text_and_muted_slots` [logs message text / timestamp muted]); no existing test changed.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean (2 lints from new code fixed: `needless_option_as_deref`, `collapsible_if`).
- `cargo fmt --all --check`: clean.
- CI tokenization grep: clean (no `Color::` outside `theme.rs` — helpers only at render sites).

## Round-2 Commit

- `27df248` — feat(12-04): tint body content — text/muted slots wired across screens (UAT tuning round 2) (9 files: theme.rs + 8 render modules)

## Next Phase Readiness (round 2)

- Phase 12 remains 4/4 plans complete, PENDING the user's round-2 visual re-verification: `ign tui` with `[ui] theme = "dark"` should now read as a blue-tinted cockpit end to end (body text, labels, status lines, modals — not just borders), `light` as the inverted look with tinted metadata, `default`/`mono` pixel-identical to before Phase 12.
- After re-verification passes, the phase is ready for `/gsd-verify-work 12` / transition.

---

## Self-Check (round 2): PASSED

- [x] Commit `27df248` present (`feat(12-04): tint body content…`, 9 files)
- [x] Wire evidence: default/mono identical pre/post (C256 + truecolor); dark +`189`/`67`; light +`8`; truecolor `38;2` codes proven
- [x] 231 tests / clippy -D warnings / fmt / tokenization grep green at commit
- [x] HARD constraints honored: DEFAULT text stays Reset; MONO all-Reset; mono adaptation logic untouched; semantic level/error/selection styles untouched; LIGHT palette byte-unchanged from `756e888`
