---
phase: 06-tui-cockpit
verified: 2026-08-28T07:15:00Z
status: passed
score: 18/18 must-haves verified (5 original truths regression-clean + 13 UAT gap closures)
re_verification:
  previous_status: passed
  previous_score: 5/5
  gaps_closed:
    - "Metrics decode on 8.3.3 exponent-form doubles (UAT test 4, major) — 06-07"
    - "Designer-prune 409 → exit-6 session_not_prunable (UAT test 6, minor) — 06-07"
    - "Contextual ign tui TTY hint (UAT test 2, minor) — 06-07"
    - "Root-level resource put member shape (UAT test 6 fixture blocker, major) — 06-08"
    - "Tags 'r' refresh + stale-error invalidation (UAT test 10, major) — 06-09"
    - "Tags write→detail refire (UAT test 11, minor) — 06-09"
    - "Error-pane recovery hints on Tags — 06-09"
    - "Modal footer-hint clipping (cosmetic) — 06-10"
    - "Frame-clamped modal geometry (minor) — 06-10"
    - "Vim motions in all modals (minor) — 06-10"
    - "Prose wait-prefix menu labels (minor) — 06-10"
    - "Noun-grouped Projects menu with consequence descriptions (minor) — 06-10"
    - "Readable rig status summary (UAT test 13, minor) — 06-11"
    - "README TUI keymap synchronized with gap-closure keys — 06-11"
  gaps_remaining: []
  regressions: []
human_verification_anticipated:
  - test: "Re-drive the previously-failing UAT scenarios against the live 8.3.3 rig (metrics panel populates, stale tags error clears via r, modal footer visible, rig summary readable)"
    expected: "Each former UAT issue now passes in the cockpit; handled by the configured end-of-phase /gsd-verify-work step"
    why_human: "Visual feel and live-gateway interactivity cannot be asserted programmatically"
---

# Phase 6: TUI Cockpit Verification Report

**Phase Goal:** A user can open `ign tui` and drive every CLI capability through a k9s/lazygit-style cockpit — the primary human interface, structurally complete because TUI and CLI share the same actions layer.
**Verified:** 2026-08-28T07:15:00Z
**Status:** passed
**Re-verification:** Yes — after UAT gap closure (06-UAT.md found 13 gaps; plans 06-07→06-11 closed them)

## Goal Achievement

This verification confirms two layers: (A) the original 5 ROADMAP truths remain regression-clean, and (B) all 13 UAT-discovered gaps are genuinely closed in the codebase — not merely claimed in summaries.

### A. Original Observable Truths (regression check)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `ign tui` cockpit with full CLI-action coverage, CI-enforced | ✓ VERIFIED | `tui_coverage.rs` clap-tree walk **re-ran: 3/3 passed** after the prose-label change — routes.rs keeps clap-exact rows (`wait gateway` etc. at routes.rs:80-88) while display labels went prose |
| 2 | Live status dashboard w/ periodic refresh + profile switch | ✓ VERIFIED | refresh worker tests green; metrics panel now f64-fed (gap 1); profile switch tests green (186 TUI tests) |
| 3 | Tail logs w/ level filtering, non-blocking | ✓ VERIFIED | tail/watch worker + ring tests green in the workspace sweep |
| 4 | Browse tags, live-watch, alarms ack | ✓ VERIFIED | Tags screen tests green + new freshness tests (gap 5-6); alarms ack trigger remains the pinned pattern |
| 5 | Browse projects/resources, project actions | ✓ VERIFIED | Projects tests green; menu regrouped (gap 11) with `.verb` dispatch at update.rs:1864; gated-verb parity tests **re-ran: 8+1 passed** |

### B. UAT Gap Closures (full 3-level verification)

| # | Gap (UAT test) | Status | Code Evidence (all three levels: exists / substantive / wired) |
|---|----------------|--------|----------------------------------------------------------------|
| 1 | Metrics decode on 8.3.3 exponent-form doubles (test 4, major) | ✓ VERIFIED | `metrics.rs:49-53` heap/max `f64` + `serialize_bytes_f64` (whole→integer JSON, 2^53 guard); fixture test `current_gauges_decodes_exponent_form_java_doubles` pins the EXACT raw 8.3.3 body `2.85746728E8` via `from_str` (macro-proof), + integer/decimal/round-trip forms — **ran: passing**; display consumers wired (`dashboard.rs:288 fmt_mib(f64)`, `render.rs:365-366 as i64` casts keep goldens byte-identical); sibling wire audit comment in-model |
| 2 | Designer-prune 409 → exit-6 session_not_prunable (test 6, minor) | ✓ VERIFIED | `classify.rs:88` route-scoped arm `S::CONFLICT if is_designer_prune_url(url)` with exact URL-detection unit tests (singular vs plural `/designers`); `error.rs:323` SessionNotPrunable in the exit-6 group (`:397`), slug `:365`, hint "close the Designer first — prune removes stale entries only" `:562-563`; wiremock pair — 409-empty-body→slug assertion AND off-route-409-stays-Internal — **ran: 2/2 passing**; README exit table row 6 updated |
| 3 | Contextual `ign tui` TTY hint (test 2, minor) | ✓ VERIFIED | `error.rs:30` `TUI_TTY_REFUSAL_REASON` const + `tui_tty_refusal()` constructor (:616) + content-addressed hint branch (:428-432) "run `ign tui` in an interactive terminal…"; raise site wired (`main.rs:1645`); snapbox golden `tui_under_a_pipe_refuses_with_the_interactive_terminal_hint` pins exit-2 envelope + new hint — **ran: passing** |
| 4 | Root-level resource put member shape (test 6 fixture blocker, major) | ✓ VERIFIED | `resources.rs:70-76` `member_path` no-slash → `<X>/resources/<X>` (module named after the file) with symmetric `user_path` inverse + documented alias; 3 structure pins in `resources_contract.rs` (:431, :495, :528 — member shape, delete removal, wiremock import-body crown) — **ran: 3/3 passing**; nested-member tests unregressed; live rig round-trip evidence pasted in 06-08-SUMMARY (put→get→list→delete on gap08cli, gateway re-export adoption oracle) |
| 5 | Tags 'r' refresh, deepest-visible refire (test 10, major) | ✓ VERIFIED | `update.rs:816` `Char('r')` in tags_keys → `refire_tags_current_level` (:947) — substantive deepest-first logic: detail read > stack-top browse (entries+error cleared, re-spawn) > providers; screen re-entry (:487) and profile-switch re-entry (:591) route through the same helper — stale 402-class errors invalidate without key discovery; 4 refresh tests green |
| 6 | Tags write→detail refire (test 11, minor) | ✓ VERIFIED | `state.rs:872` `last_write_path` armed at accept (:1318), consumed on ANY landing (:240); ActionDone trigger (:239-252) refires detail read only on SUCCESS + matching path (alarms-ack pattern's twin); watch table left to its 2s poll (commented); matching + non-matching + failed-write tests green |
| 7 | Error-pane recovery hints (test 10 missing-item) | ✓ VERIFIED | `tags.rs:30` `refresh_hint()` DIM line on all three error renders (:118, :194, :260); 3 render-test assertions "press r to refresh" green |
| 8 | Modal footer-hint clipping (test 5, cosmetic) | ✓ VERIFIED | `ui/mod.rs:103-126` content-driven formulas: menus = `len + 4` (entries + hint + blank + 2 borders), Confirm/Input/Result exact row counts, ProjectsActions = shared `projects_action_lines` builder length + 2 — the builder renders the footer hint as its last line, so geometry can never undercount it again |
| 9 | Frame-clamped modal geometry (test 5, minor) | ✓ VERIFIED | `ui/mod.rs:132` `height.clamp(5, frame.area().height - 2)` — never clips chrome on small terminals (5-row floor), grows on large; small-frame render tests green |
| 10 | Vim motions in every modal (test 5, minor) | ✓ VERIFIED | `update.rs:2434` shared `menu_nav` (Up/k, Down/j, g, G) wired into ALL six list modals (:2477, :2557, :2574, :2591, :2608, :2626); Result-modal arm (:2760-2795) adds j/k line, Ctrl-d/Ctrl-u `RESULT_HALF_PAGE`=10, g/G with plain/ctrl modifier guards; arrows + PgUp/PgDn byte-identical behavior preserved |
| 11 | Prose menu labels (test 5, minor) | ✓ VERIFIED | `state.rs:343-345` "wait for gateway up / wait for restart complete / wait for module ready"; executor arms match on the same strings (update.rs:2226/2238/2251); **routes.rs NOT modified** — coverage test 3/3 green proves clap parity intact; structure-pin tests at state.rs:1188-1190 |
| 12 | Noun-grouped Projects menu (test 12, minor) | ✓ VERIFIED | `state.rs` `ProjectAction{group, verb, label, description}` const with project/resource/webdev groups (contiguity/order/uniqueness test-pinned); `ui/mod.rs:31` `projects_action_lines` renders bold group headers, blank-separated sections, dim `label — description` rows (descriptions budgeted to the 38-col interior); dispatch decoupled via `.verb` (update.rs:1864); gated-verb parity tests green — confirm gating unchanged |
| 13 | Rig status readable + README keymap sync (test 13, minor) | ✓ VERIFIED | `ui/rig.rs:153` `summary_lines` — bold `STATE {UP/DOWN} · PORTS {free/held}` headline (:163), blank-separated bold sections identity/services/volumes (:170/:188/:211), width-aware `fit_tail` for the compose path (:61), render tests for UP + DOWN shapes at 80×24; README keymap documents Tags `r` deepest-visible refire, modal vim motions (j/k/g/G menus; j/k + Ctrl-d/Ctrl-u Result), grouped prose menu; exit-table `session_not_prunable` row cross-checked (:36) |

**Score:** 18/18 (5 original truths + 13 gap closures)

### Key Link Verification (gap-closure-critical wiring)

| From | To | Via | Status |
|------|----|----|--------|
| metrics.rs f64 wire | exponent JSON | serde decode + fixture | ✓ WIRED — test passing |
| classify.rs 409 arm | CoreError exit-6 slug | route-scoped guard | ✓ WIRED — wiremock pair passing |
| main.rs TTY raise | tui_tty_refusal hint | constructor + const | ✓ WIRED — golden passing |
| resources.rs member_path | zip import | `<X>/resources/<X>` shape | ✓ WIRED — 3 structure pins passing |
| tags_keys 'r' | refire_tags_current_level | deepest-first helper | ✓ WIRED |
| set_screen/profile re-entry | same helper | invalidation on entry | ✓ WIRED (:487, :591) |
| ActionDone "tags write" | detail read refire | last_write_path match+consume | ✓ WIRED — tests passing |
| ui/mod.rs modal_height | frame area | clamp(5, frame-2) | ✓ WIRED |
| menu_nav | 6 modal handlers | j/k/g/G | ✓ WIRED |
| state.rs prose labels | update.rs executor arms | identical strings both sides | ✓ WIRED — coverage 3/3 |
| state.rs PROJECT_ACTIONS .verb | update dispatch (:1864) | decoupled dispatch key | ✓ WIRED — parity tests green |
| rig.rs summary_lines | render_summary | sectioned layout + width | ✓ WIRED — render tests passing |
| README | shipped keys | keymap bullets | ✓ WIRED — verified against code |

### Dynamic Evidence (executed during this verification)

- `cargo test --workspace --features tui` → **38 suites, ~721 tests, 0 failures** (698 at initial verification + gap-closure tests)
- Gap tests by name: exponent fixture 1/1, designer-prune 2/2, TTY golden 1/1, root-level 3/3, tags refresh+write 12/12 (filtered)
- `cargo test -p ignition-cli --features tui --test tui_coverage` → **3/3** (prose-label change did not break clap parity)
- Gated-verb parity → **8+1 passing** (confirm gating unchanged through the menu regroup)
- `cargo fmt --all --check` → clean; `cargo clippy --workspace --features tui --all-targets -- -D warnings` → clean

### Requirements Coverage

| Requirement | Status | Blocking Issue |
|-------------|--------|----------------|
| TUI-01 — cockpit exposing every CLI action | ✓ SATISFIED | coverage test green post-refactor |
| TUI-02 — live dashboard w/ refresh | ✓ SATISFIED | + 8.3.3 metrics fix (gap 1) |
| TUI-03 — tail logs w/ filtering | ✓ SATISFIED | regression-clean |
| TUI-04 — browse tags + live watch | ✓ SATISFIED | + 'r' refresh + write-refire (gaps 5-6) |
| TUI-05 — view + ack alarms | ✓ SATISFIED | regression-clean (ack loop itself UAT-skipped — no alarm fixture on rig; mechanics test-pinned) |
| TUI-06 — projects/resources browse + profile switch | ✓ SATISFIED | + root-level put (gap 4) + grouped menu (gap 12) |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| — | — | none | — | Zero TODO/FIXME/XXX/HACK/unimplemented!/todo! across all TUI sources + the 4 touched core client/error files |

### UAT Skips & Backlog (not gaps — explicitly owned)

- UAT tests 6 (session terminate) and 9 (alarms ack) were **skipped by user choice** — no fixture achievable on the rig; mechanics verified via honest-error paths and remain test-pinned
- UX feedback themes (monochrome color, richer editor experience) triaged as **backlog** per UAT triage — out of gap-closure scope by design

### Human Verification Required

The UAT cycle already provided the human pass for the original build. The gap closures warrant a short human re-confirm of the formerly-failing scenarios (routed to the configured end-of-phase `/gsd-verify-work 6` step):

### 1. Former UAT failures now pass in the cockpit
**Test:** Against the live 8.3.3 rig: watch the metrics panel populate; on Tags, earn an error then press `r`; put a root-level file via the Projects screen; open any Actions menu; view the Rig summary.
**Expected:** Metrics render (no internal error); stale error visibly reloads; root-level put lands; "Enter to run · Esc to cancel" footer fully visible at any terminal size; rig summary reads as grouped sections.
**Why human:** Visual rendering, live-gateway behavior, terminal-size feel.

### 2. Vim motions feel right in modals
**Test:** j/k/g/G in menus; j/k + Ctrl-d/Ctrl-u in a long Result modal.
**Expected:** Selection/scroll moves as on the screens; arrows/PgUp/PgDn unchanged.
**Why human:** Key-feel and scroll behavior across modal kinds.

### Gaps Summary

None. All 13 UAT gaps are closed with real, wired, test-pinned code — verified at all three levels (exists, substantive, wired) with dynamic evidence (38 green suites, ~721 tests, fmt/clippy clean, coverage + parity tripwires green). The structural-completeness guarantee survived the label refactor because routes.rs kept clap-exact rows while display labels went prose — the exact design the coverage test enforces. Remaining uncertainty is confined to visual/live feel of the closed gaps, owned by the end-of-phase UAT step.

---

_Verified: 2026-08-28T07:15:00Z_
_Verifier: Claude (gsd-verifier)_
