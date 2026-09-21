---
phase: 06-tui-cockpit
plan: 05
subsystem: ui
tags: [ratatui, tui, projects, resources, webdev, confirm-gating]

# Dependency graph
requires:
  - phase: 06-tui-cockpit plans 01-04
    provides: the cockpit shell (event loop, Elm state/update, modal infrastructure, one-shot worker pattern), the Tags screen's object-browser/navigation pattern, and the actions::projects/resources/webdev layers re-pointed onto export-zip surgery
provides:
  - Projects screen with two-level browse (project list → detail + resources → resource get with scrollable content preview)
  - Project/resource/webdev action menus with CLI confirm-parity (project delete, project import-overwrite, resource put, resource delete Confirm-gated; webdev deploy deliberately ungated)
  - workers/ops.rs — the one-shot worker module for the project/resource/webdev families
  - All 14 project/resource/webdev clap leaves mapped in routes.rs → Screen(Projects)
  - gated_cli_verb — the exhaustive PendingAction confirm-parity classifier (06-06's tripwire input)
affects: [06-tui-cockpit plan 06 (rig screen + structural coverage proof)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Object-list → detail navigation stack with name/seq request-id stale gates (a popped pane's late result drops at the identity lookup)"
    - "Per-half tri-state panes (record + resources degrade independently)"
    - "Exhaustive PendingAction classification as a compile-time confirm-parity tripwire"

key-files:
  created:
    - crates/ignition-tui/src/workers/ops.rs
  modified:
    - crates/ignition-tui/src/ui/projects.rs
    - crates/ignition-tui/src/state.rs
    - crates/ignition-tui/src/update.rs
    - crates/ignition-tui/src/event.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-tui/src/ui/mod.rs
    - crates/ignition-tui/src/workers/mod.rs

key-decisions:
  - "Project detail rides client project_find (there is no `project get` leaf — the find IS the family's read-back source); the pane shows the record's full passthrough (defaultDb/tagProvider/userSource) beyond the six summary fields"
  - "One ProjectsActions menu hosts all three families (11 entries: new/copy/rename/set/delete/import/export/resource put/resource delete/webdev deploy/webdev status) — copy and rename included so every route row is honestly reachable"
  - "webdev deploy fires with CLI defaults (ign-cli, no scriptExec flags) and NO Confirm — the 05-03 CLI-owned-project decision; scriptExec's flags stay on the CLI form"
  - "project set parses exactly one FIELD=VALUE pair per prompt (values keep spaces after the first =); defaultDb/tagProvider/userSource stay CLI-form-only per the LOCKED modal-depth decision"
  - "Optional-title form step: empty SKIPS the field rather than canceling (special-cased ahead of the shared empty-cancel rule)"
  - "gated_cli_verb is #[cfg_attr(not(test), expect(dead_code))] — the Phase-01 pattern; 06-06's structural test graduates it"

patterns-established:
  - "Name-identity stale gates: ProjectGet/ResourcesList results drop unless the open pane still holds the same name (the Tags path-lookup shape, no per-pane era needed)"
  - "Menu fire helpers live in workers/ops.rs (fire_project_new/…/fire_webdev_status) so update.rs stays pure routing and the artifact's one-shot-verb contract holds"
  - "Content preview derivation is pure state (ResourceDetail::content_lines: JSON pretty, text raw) with render-side scroll clamping"

# Metrics
duration: 29min
completed: 2026-08-27
---

# Phase 6 Plan 5: Projects Screen Summary

**Project list → detail → resource drill-down with scrollable content preview, plus the full project/resource/webdev action surface with exact CLI confirm-parity (4 newly gated verbs, webdev deploy deliberately ungated) and all 14 family route rows registered**

## Performance

- **Duration:** 29 min
- **Started:** 2026-08-27T22:59:44Z
- **Completed:** 2026-08-27T23:28:49Z
- **Tasks:** 3
- **Files modified:** 8

## Accomplishments
- Two-level browse works headlessly: projects table → Enter → detail (record find + resources list, per-half degrade) → Enter → resource get with a scrollable content preview (JSON pretty/text raw; binary fencing surfaces the action's exit-6 verbatim)
- Esc pops exactly one level at every depth (resource → detail → list → quit); Up/Down move the owning cursor per depth and scroll the preview at the deepest level
- All project/resource/webdev verbs reachable: rich-arg actions chain Inputs with context prefills and the `?` CLI-form hint; guarded verbs (project delete, project import-overwrite, resource put, resource delete) arm Confirm modals — cancel spawns nothing, `y` fires the unguarded action
- webdev deploy/status fire directly from the menu (deploy UNGATED per the 05-03 CLI-owned ign-cli decision; status is a read whose sweep degradation lands in the result modal as data)
- Confirm-parity audit: the TUI's gated set is exactly main.rs's `require_confirmation` sites for every registry-mapped family (11 verbs / 6 families; the rig family's 3 guards remain 06-06's); secrets confinement grep-clean (Secret|Credential only in context.rs)

## Task Commits

Each task was committed atomically:

1. **Task 1: Project browser — list, detail, resource drill-down** - `c68008e` (feat)
2. **Task 2: Project + resource + webdev action menus with confirm gating** - `04e0abf` (feat)
3. **Task 3: Destructive-verb confirm-parity audit** - `332815a` (fix)

**Plan metadata:** (see final docs commit)

## Files Created/Modified
- `crates/ignition-tui/src/workers/ops.rs` - One-shot workers: project list/find, resources list, resource get (era+name/seq stamped) + menu fire helpers (new/copy/rename/set/delete/import/export/put/delete/webdev deploy/status) + the stdin-refusing byte-source helper
- `crates/ignition-tui/src/ui/projects.rs` - Full screen render: project table, record+resources split detail, scrollable resource preview with per-pane tri-states (+ TestBackend render tests)
- `crates/ignition-tui/src/state.rs` - ProjectsData (list/detail/resource stack, resource_seq request-id), ProjectsForm (18-step router), 4 new PendingAction variants, PROJECT_ACTIONS menu, ProjectsActions modal
- `crates/ignition-tui/src/update.rs` - Event handlers (name/seq gated), drill-down state machine, menu execution, form chains, parse_set_line, projects_cli_form, gated_cli_verb classifier + confirm-parity test
- `crates/ignition-tui/src/event.rs` - ProjectsList/ProjectGet/ResourcesList/ResourceGet variants
- `crates/ignition-tui/src/routes.rs` - All 14 project/resource/webdev leaves → Screen(Projects) + coverage test
- `crates/ignition-tui/src/ui/mod.rs` - ProjectsActions modal render + menu render test
- `crates/ignition-tui/src/workers/mod.rs` - ops module registration

## Decisions Made
- Project detail sources `client.project_find` (no `project get` action exists — the find is the family's read-back source); the detail pane uniquely shows the record's defaultDb/tagProvider/userSource passthrough
- Single ProjectsActions menu for all three families, including copy/rename (the plan listed five project verbs; the routes registry requires every leaf reachable, so the menu carries all seven project verbs — honest coverage over menu minimalism)
- webdev deploy fires with the CLI defaults only (target `ign-cli`, no scriptExec); the secret lifecycle stays inside the action, and the profile's stored webdev secret is loaded inside the status worker (a plain config String, not a Secret type — CLI-parity)
- `set` accepts one FIELD=VALUE pair per prompt (values may contain spaces after the first `=`); all other flags advertised via the `?` CLI form

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] ui/mod.rs placeholder test re-pinned**
- **Found during:** Task 1
- **Issue:** `screen_dispatch_renders_the_active_screen` asserted the old "Projects — not yet wired" placeholder; the real browser replaced it
- **Fix:** Re-pinned the test to the real list pane (bordered title + Loading state)
- **Files modified:** crates/ignition-tui/src/ui/mod.rs
- **Verification:** cargo test -p ignition-tui green
- **Committed in:** c68008e

**2. [Rule 1 - Bug] Optional-title form inverted its emptiness check**
- **Found during:** Task 2
- **Issue:** The first draft's `value.is_empty().then(|| value.clone())` produced Some("") for empty input (the opposite of skip), and the shared empty-cancel rule would have aborted the whole create
- **Fix:** NewTitle special-cased ahead of the shared empty-cancel: empty SKIPS the field, the create still fires (test-pinned both ways)
- **Files modified:** crates/ignition-tui/src/update.rs
- **Verification:** new_chain_prompts_title_and_empty_skips
- **Committed in:** 04e0abf

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Both fixes necessary for correctness. No scope creep.

## Issues Encountered
None - beyond the two auto-fixes above, all verifications passed first try (workspace 676 tests green, fmt clean, clippy -D warnings clean).

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Success Criterion 5 delivered (browse projects/resources + trigger project actions) and the project half of TUI-06: full project/resource browse + act surface with CLI confirm-parity
- 06-06 inherits: the routes registry (dashboard/profile/logs/tags/alarms/project/resource/webdev complete — only the rig family remains), the gated_cli_verb tripwire input, and the established object-browser/menu patterns
- Blockers: none

---
*Phase: 06-tui-cockpit*
*Completed: 2026-08-27*

## Self-Check: PASSED

- All 8 key files exist on disk (workers/ops.rs created; 7 modified)
- All 3 task commits verified in git history (c68008e, 04e0abf, 332815a)
- must_haves: ui/projects.rs 541 lines (≥80); workers/ops.rs contains actions::projects; all four truths test-pinned
- Verification: workspace 676 tests green, fmt clean, clippy -D warnings clean, Secret|Credential confined to context.rs, Esc pops exactly one level at every depth (test-pinned)
