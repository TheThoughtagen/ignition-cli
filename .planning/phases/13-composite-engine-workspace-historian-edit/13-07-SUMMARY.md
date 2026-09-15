---
phase: 13-composite-engine-workspace-historian-edit
plan: 07
subsystem: cli-surface
tags: [workspace, clap, session-seam, render, routes-parity, stdout-purity, snapbox-goldens, traffic-pinning, readme-contract]

# Dependency graph
requires:
  - phase: 13-03
    provides: workspace_checkout + CheckoutOutcome + read_manifest's stable refusal prefixes (the golden anchors) + decode-scripts codec leg
  - phase: 13-06
    provides: workspace_status/workspace_push + WorkspaceStatus/StatusRow/StatusKind/PushPreview/PushOutcome envelopes + the one-gate refusal whose message IS the preview
  - phase: 13-02
    provides: the recorded mapping semantics the CLI rides without re-deriving
provides:
  - `ign workspace checkout|status|push` — three clap verbs, one family, dispatching through Session::resolve (the Phase-8 seam) with global flags honored (--yes never redeclared)
  - ActionOutput::Workspace{Checkout,Status,Push} + human render arms (PATH/STATE table with clean rows included + four-counter summary; push wrote/deleted/skipped lists) and JSON/compact envelope passthrough of the outcome structs
  - Pre-resolve usage guards on status/push: the manifest read needs no gateway, so missing/corrupt/foreign-schema refusals exit 2 with profile null and ZERO requests (api-call convention)
  - routes() rows for the three verbs (Projects screen) landed in the SAME commit as the clap commands — bidirectional clap walk green; OutOfBand set untouched ([api call, completions])
  - stdout-purity harness coverage for the three new verbs (missing-manifest and dead-port refusals leave stdout byte-empty under max diagnostics)
  - tests/contract_workspace.rs — 7-test binary suite: every-row-kind status goldens in all three modes, preview-refusal golden (13-06 text byte-for-byte), conflict refusal fires even with --yes, clobber ladder, decode-scripts happy path, foreign-schema prefix, guarded-splice with exactly-one-import scope pin
  - README workspace contract section (scope honesty, manifest commitment, push-relative state table, guarded push, decode round-trip)
affects: [13-08 (edit CLI edits the same CLI files next — sequential wave 5), README exit-table three-place discipline unaffected (no new slugs)]

# Tech tracking
tech-stack:
  added: [] # zero new dependencies
  patterns: [pre-resolve usage-refusal posture extended to manifest reads, routes-rows-land-with-clap atomicity, traffic-pinned binary guard tests via mount_as_scoped expect(0/1), snapbox goldens with random-path normalization, push-relative STATE labels in human render]

key-files:
  created:
    - crates/ignition-cli/tests/contract_workspace.rs
  modified:
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-cli/tests/tui_coverage.rs
    - crates/ignition-cli/tests/contract_stdout_purity.rs
    - README.md
    - crates/ignition-cli/tests/e2e_webdev.rs

key-decisions:
  - "Status/push derive the project from the RECORDED manifest and the manifest read happens PRE-resolution (read_manifest is pure fs) — the 13-03 stable refusal prefixes render as exit 2 / profile null / zero-request usage errors, exactly the api-call posture the plan prescribed"
  - "Workspace rows map Mapping::Screen(Screen::Projects) beside the project family (checkout rides the export, push rides the import) — normal envelope verbs, NOT OutOfBand; the pinned OutOfBand set stays exactly [completions, api call] and `edit` remains 13-08's atomic landing"
  - "Human status table uses the project-family two-space join (not computed-width alignment) — matches the codebase table convention and keeps snapbox goldens stable; STATE labels carry direction where it matters (deleted (local) vs deleted (gateway))"
  - "The checkout clobber/refusal ladder rides the action layer post-resolution (no CLI-side duplication of ensure_recheckout_safe) — one refusal shape, goldens render it with the profile echoed"

patterns-established:
  - "Random-path golden normalization: replace the tempdir prefix with <TARGET> before snapbox — goldens stay deterministic without a fixture-path escape hatch"
  - "Binary traffic pins: mount_as_scoped import guards with expect(0/1) fail loudly at scope drop — the zero-write and exactly-one-import honesty contracts now hold at the spawned-binary level, not just in core"

# Metrics
duration: 51min
completed: 2026-09-15
---

# Phase 13 Plan 07: Workspace CLI Family Summary

**`ign workspace checkout|status|push` live end to end: Session-seam dispatch with pre-resolve manifest guards, three-mode render (push-relative PATH/STATE table + guarded-push lists), routes↔clap atomicity, stdout-purity coverage, seven binary contract goldens whose push-refusal text is 13-06's preview byte-for-byte, and the README's honesty-contract section — SC-1 fully closed at the binary level.**

## Performance

- **Duration:** 51 min
- **Started:** 2026-09-15T19:21:23Z
- **Completed:** 2026-09-15T20:13:09Z
- **Tasks:** 3
- **Files modified:** 8 (1 created, 7 modified)

## Accomplishments
- The workspace family parses and dispatches exactly as planned: `Commands::Workspace(checkout|status|push)` with `--decode-scripts`/`--delete` and the global `--yes` (never redeclared); all three verbs resolve through `Session::resolve` — no second construction path — and status/push derive the project from the recorded manifest so the user never re-types it
- Pre-resolve usage guards: the manifest read (pure fs) runs BEFORE profile resolution on status/push, so the 13-03 stable refusal prefixes (missing/corrupt/foreign-schema) render as exit 2, `profile: null`, zero requests — pinned by the extended stdout-purity harness under `IGNITION_LOG=trace` + unknown-key noise
- Render arms for all three modes: human checkout line + decode note, the PATH/STATE table (clean rows included, deterministic core order, four-counter summary), push wrote/deleted/skipped lists; JSON/compact pass the outcome structs verbatim as `data` (additive-only — no pre-existing envelope touched)
- Coverage rituals landed atomically with the surface: three Projects-screen routes rows + clap-walk bidirectional green + the workspace family pin + OutOfBand set untouched, and the purity harness grew explicit workspace refusal cases
- contract_workspace.rs: seven binary tests over wiremock-served real exports, including the every-row-kind status golden in all three render modes, the guarded-push refusal golden whose message is 13-06's `render_push_preview` verbatim, conflict-refuses-even-with-`--yes`, the clobber ladder, the decode-scripts happy path, and the exactly-one-import splice proof (scope-verified traffic pins)
- README: the workspace contract section (resources-only scope with the tag-exclusion clause, committed manifest / gitignored artifacts, the push-relative state table, guarded push + conflict honesty, byte-exact decode round-trip) plus Commands-table rows and the destructive-operations list entry

## Task Commits

Each task was committed atomically:

1. **Task 1: clap family + Session-seam dispatch + three-mode render** - `30457b0` (feat)
2. **Task 2: TUI routes rows + clap-walk coverage + stdout purity coverage + status goldens** - `7cec57f` (test)
3. **Task 3: refusal goldens + checkout/push binary contract tests + README workspace section** - `09a3df1` (test)

## Files Created/Modified
- `crates/ignition-cli/src/cli.rs` — `Workspace(WorkspaceArgs)` on Commands; Checkout/Status/Push subcommands with docs
- `crates/ignition-cli/src/main.rs` — three ActionOutput variants, render_json arms, the dispatch arm (pre-resolve manifest guards; Session seam)
- `crates/ignition-cli/src/render.rs` — human render arms + `workspace_state_label` + status table/summary + push lists
- `crates/ignition-tui/src/routes.rs` — three Projects-screen rows with the family-atomicity comment
- `crates/ignition-cli/tests/tui_coverage.rs` — workspace family pin + required-subcommand-group entry
- `crates/ignition-cli/tests/contract_stdout_purity.rs` — two workspace purity tests (status/push missing-manifest, checkout dead-port)
- `crates/ignition-cli/tests/contract_workspace.rs` (created) — the seven-test binary contract suite
- `README.md` — Commands-table rows, the workspace contract section, destructive-ops entry
- `crates/ignition-cli/tests/e2e_webdev.rs` — two pre-existing clippy lints fixed (Rule 3, zero behavior change)

## Decisions Made
- Status/push read the manifest PRE-resolution for the project + the usage-refusal posture; the action layer re-reads it internally (idempotent, cheap) — no core change needed, honoring the "don't touch actions/workspace.rs cores" file-ownership note
- Workspace rows map to `Screen(Projects)` per the plan's "map them the way project diff/sync map" directive; no Dashboard row landed, so the pinned 36-row Dashboard menu count is untouched
- Human status table rides the two-space join convention (project-list style) instead of computed-width alignment — the plan's "stable two-column table" without golden-hostile dynamic padding
- The binary test fixture proves the manifest keys are USER paths (mapping-stripped), goldenned verbatim from the real binary rather than assumed from raw zip names

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Two pre-existing clippy -D warnings in e2e_webdev.rs**
- **Found during:** Task 3 (final sweep: `cargo clippy --workspace --all-targets -- -D warnings`)
- **Issue:** `&[&tag]` immediate-deref lint (line 1047) and `map_or(false, …)` → `is_some_and` (line 1316) failed the gate; the file predates this plan (13-04's live gate) and was not in this plan's file list
- **Fix:** applied the two mechanical lint suggestions; zero behavior change
- **Files modified:** crates/ignition-cli/tests/e2e_webdev.rs
- **Verification:** workspace clippy -D warnings green
- **Committed in:** 09a3df1 (Task 3 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** The lint fix was required for the plan's own verification gate and touched test scaffolding only. No scope creep.

## Issues Encountered
- The repo-local `target/debug/ign` is a stale pre-config artifact: the user's global `~/.cargo/config.toml` redirects `build.target-dir` to `~/Library/Caches/cargo-target`. Builds and `Command::cargo_bin` resolve correctly through the global config; manual smoke checks must use the cache-dir binary (not a code issue — recorded for future executors).
- The status-golden fixture initially asserted raw zip member names; the manifest keys (and status rows) are USER paths with the `resources/` segment stripped — the binary's actual output (which is the contract) was goldenned as-is.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness
- 13-08 (edit CLI, wave 5) now edits this exact surface sequentially: the workspace rows are in routes.rs with the OutOfBand set still `[completions, api call]` — `edit`'s OutOfBand row + clap command remain 13-08's atomic landing per the pre-declaration; ActionOutput's variant list ends at WorkspacePush before the TuiExited arm, so the sequential edit has a clean insertion point
- SC-1 (checkout/drift/guarded-push as commands) is fully closed with binary-level contract evidence; SC-2's tag-exclusion clause is pinned at the binary level by the exact-set checkout suite (13-03) and the resources-only README contract
- No new exit slugs, no route-bundle change, no new dependencies — the Three-Place discipline was untouched (all refusals ride existing invalid_input/confirmation_required classes)
- Full verification battery green at 09a3df1: `cargo test --workspace` (59/59 test binaries, 0 failures), `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`

---
*Phase: 13-composite-engine-workspace-historian-edit*
*Completed: 2026-09-15*

## Self-Check: PASSED

- Files verified on disk: all 8 key-files (1 created: contract_workspace.rs at 833 lines ≥ 60 min_lines; 7 modified) plus the SUMMARY itself
- Commits verified in history: 30457b0 (Task 1), 7cec57f (Task 2), 09a3df1 (Task 3)
- Purity coverage grep: 10 `workspace` mentions in contract_stdout_purity.rs (two dedicated tests)
- Final battery re-run green at 09a3df1: cargo test --workspace 59/59 binaries ok, clippy -D warnings clean, fmt --check clean
- Pinned contracts re-verified: OutOfBand set exactly [api call, completions]; Dashboard route count still 36; no new slugs/deps
