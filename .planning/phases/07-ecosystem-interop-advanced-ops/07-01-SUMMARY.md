---
phase: 07-ecosystem-interop-advanced-ops
plan: "01"
subsystem: api
tags: [ignition, cross-gateway, diff, sync, zip-surgery, wiremock, rust, promotion]

# Dependency graph
requires:
  - phase: 05-webdev-backend-tag-operations
    provides: export-zip member surgery helpers (replace_member/remove_member/read_member), the --yes guard-before-resolution pattern with consequence-naming operation strings, member-level (not byte-level) contract-test honesty
  - phase: 06-tui-cockpit
    provides: tui_coverage clap-walk CI gate, PROJECT_ACTIONS menu + ProjectsForm chained-input patterns, gated_cli_verb confirm-parity tripwire
provides:
  - "`ign project diff <A> <B> --project <NAME>` — cross-gateway normalized member compare (B-relative-to-A statuses, wiremock + golden pinned)"
  - "`ign project sync <A> <B> --project <NAME> --resource/--all-changed [--delete] --yes` — guarded selective A→B promotion via two-client surgery"
  - Pure diff engine in client/resources.rs (normalize_descriptor with key-order-independent canonicalization, member_hashes FNV-1a, diff_members, project_meta_delta) — zero new dependencies
  - The two-client resolution shape (resolve_two_clients: envelope-active profile unchanged, per-side resolve_selection + independent secret chains)
  - e2e sync witness runnable against two live gateways (IGNITION_LIVE_URL_B)
affects: [07-02 (backup/EAM — envelope + guard conventions), 07-03 (script run — TUI same-plan rule), 07-04 (interop trio — pure-module pattern)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Two-client resolution: envelope profile = the ACTIVE profile (resolution unchanged); data carries profile_a/profile_b; each side builds its own client through the ONE locked secret chain (env tokens apply to BOTH sides — README caveat)"
    - "Descriptor normalization for cross-gateway compare: strip exactly attributes.lastModification + lastModificationSignature, recursively key-sort, re-serialize — output depends only on content, never input key order or serde_json ambient map behavior"
    - "Diff/sync label reconciliation: the diff speaks B-relative-to-A; sync speaks A→B promotion — upserts = diff's removed+changed, --delete removals = diff's added"
    - "TUI two-profile verbs rebuild per-side clients inside spawned workers via context::rebuild (the cockpit's single-client world untouched)"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/client/resources.rs
    - crates/ignition-core/src/actions/projects.rs
    - crates/ignition-core/src/actions/resources.rs
    - crates/ignition-core/tests/projects_contract.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/contract_projects.rs
    - crates/ignition-cli/tests/e2e_projects.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-tui/src/state.rs
    - crates/ignition-tui/src/update.rs
    - crates/ignition-tui/src/workers/ops.rs
    - README.md

key-decisions:
  - "Direction semantics LOCKED as planned: diff output is B-relative-to-A (added = in B only, removed = in A only); sync direction is always explicit A→B"
  - "LABEL RECONCILIATION (must_haves over plan sketch): the plan's Task-3 prose 'take added+changed paths' used sync-direction labels, contradicting the locked diff labels its own parenthetical '(in B not A)' confirms — the action maps upserts = diff's removed+changed and --delete removals = diff's added, because pushing the diff's added set would read members A does not have"
  - "member_hashes rides a 64-bit FNV-1a u64 (the plan's prose override of its [u8;32] sketch) — no sha2 dependency; collision risk acceptable for diff UX"
  - "Sync guard order: selection validation (usage errors lead) → --yes guard (exit 2, profile null, zero requests — operation string names the whole-project overwrite-import on the actual profile B) → resolve_two_clients"
  - "Zero-write honesty: --all-changed with nothing changed performs NO import (an overwrite-import of an unchanged zip is not a gateway no-op) — empty synced/removed lists, exit 0"
  - "Same-profile diff check fires in the ACTION (post-resolution, per plan) so the envelope honestly echoes the active profile — unlike sync's guard which is pre-resolution profile-null"
  - "TUI sync form: four chained inputs (profiles A/B, project, space/comma-separated resource paths) + Confirm gate; --all-changed/--delete stay CLI forms via the ? hatch (the LOCKED modal-depth decision)"

patterns-established:
  - "Cross-gateway verbs resolve two clients through resolve_selection per side with the envelope keeping the active profile — the shape every future multi-profile verb (if any) inherits"
  - "wiremock first-mounted mock wins: a bundled zero-expect import guard SHADOWS a later scoped guard on the same server — sync tests mount export-only on the import side"
  - "TUI menu indices in tests are positional — inserting a PROJECT_ACTIONS entry bumps every later run_projects_menu index (kept honest by the pinned-order unit test)"

# Metrics
duration: 79min
completed: 2026-08-28
---

# Phase 7 Plan 1: Cross-Gateway Project Diff & Selective Sync Summary

**`ign project diff` + `ign project sync` over a pure normalized-compare engine (lastModification/Signature volatility stripped, key-order-independent canonicalization, 64-bit FNV-1a) with a two-client resolution shape and a zero-request `--yes` guard — zero new dependencies**

## Performance

- **Duration:** 79 min
- **Started:** 2026-08-28T15:30:49Z
- **Completed:** 2026-08-28T16:50:08Z
- **Tasks:** 3
- **Files modified:** 14

## Accomplishments
- Cross-gateway diff with honest normalization: identical resources exported from two different gateways report `same` (the live-evidenced resource.json volatility normalized away; key-order independence pinned so a future serde_json `preserve_order` flip cannot change canonical output)
- Selective guarded sync: A's changed/missing resources land in B via the existing surgery helpers (descriptor-merge landing rules ride free), default upsert-only with `--delete` opt-in, one overwrite-import into B proven at member level on the wire
- Two-client resolution shape landed end-to-end: envelope keeps the active profile, data carries profile_a/profile_b, each side's secret chain resolves independently — plus the README two-sided-secret caveat and tag-promotion pipe for scope honesty
- TUI parity: both verbs reachable from the Projects actions menu (diff ungated read with a three-input chain; sync Confirm-gated with four), confirm-parity tripwire extended to the 15-verb guard set

## Task Commits

Each task was committed atomically:

1. **Task 1: Pure diff engine — normalized member compare** - `5550a04` (feat)
2. **Task 2: `ign project diff` verb — two-client resolution + action + contract + goldens + TUI** - `e2b95c3` (feat)
3. **Task 3: `ign project sync` verb — guarded cross-client surgery + e2e witness + TUI** - `c9a233e` (feat)

**Plan metadata:** (this commit)

## Files Created/Modified
- `crates/ignition-core/src/client/resources.rs` — the pure diff engine: normalize_descriptor, member_hashes, diff_members, project_meta_delta + 8 new unit tests (volatility, direction, exclusion, key-order independence)
- `crates/ignition-core/src/actions/projects.rs` — project_diff + project_sync orchestration, ProjectDiffResult/ProjectSyncResult/SyncSelection/ProjectMetaDelta, action-level refusals (same-profile, selection-less)
- `crates/ignition-core/src/actions/resources.rs` — export_zip_bytes promoted pub (the shared export-to-bytes seam)
- `crates/ignition-core/tests/projects_contract.rs` — two-server wiremock pins: one-export-per-side/zero-imports for diff; reads-then-one-import for sync; --delete opt-in; missing-side not_found; zero-import-on-empty
- `crates/ignition-cli/src/cli.rs` — ProjectCommand::Diff + Sync subcommands
- `crates/ignition-cli/src/main.rs` — ActionOutput variants, resolve_two_clients + named_profile_client, guarded sync dispatch (selection validation → guard → resolution)
- `crates/ignition-cli/src/render.rs` — grouped-sections diff renderer + sync renderer
- `crates/ignition-cli/tests/contract_projects.rs` — two-profile-config goldens: diff both modes, same-profile/unknown-side/selection-less/sync-refusal (zero requests pinned) + sync success both modes with member-level import-body honesty
- `crates/ignition-cli/tests/e2e_projects.rs` — project_sync_two_gateways_witness (#[ignore], IGNITION_LIVE_URL_B): put→diff→sync→re-read adoption oracle
- `crates/ignition-tui/src/routes.rs` — "project diff"/"project sync" rows + leaf-coverage test updates
- `crates/ignition-tui/src/state.rs` — PROJECT_ACTIONS (13), ProjectsForm diff/sync chains, PendingAction::ProjectSync
- `crates/ignition-tui/src/update.rs` — menu arms, form routing + path-list splitting, ? synopses, execute_pending arm, gated_cli_verb + 15-verb parity tripwire
- `crates/ignition-tui/src/workers/ops.rs` — fire_project_diff/fire_project_sync (per-side clients via context::rebuild inside workers)
- `README.md` — Cross-gateway diff & sync section (direction, normalization, scope honesty + tag pipe, two-sided secrets, sync semantics + guard + e2e recipe), commands table rows, destructive-ops list

## Decisions Made
- The label reconciliation (see key-decisions): the must-have truth "selected resources from A land in B" resolved the plan's internal diff-label/sync-direction inconsistency — documented loudly at the action's mapping site
- `--yes` rides the existing global flag (the plan's per-subcommand `yes` field sketch would collide with the CLI chassis's global `--yes`/`-y`)
- Interpolated the actual profile-B name into the sync guard's operation string ("…overwrite-import the whole project on other — …") rather than the plan's literal `<profile-b>` placeholder — more informative, golden-pinned
- wiremock mock-shadowing discovered and worked around (first-mounted wins) — recorded as a pattern so 07-02's EAM two-mock tests don't trip on it

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Sync's all-changed label mapping corrected to the locked diff semantics**
- **Found during:** Task 3 (core contract tests failed with NotFound)
- **Issue:** The plan's Task-3 action spec ("take `added`+`changed` paths"; "`removed` paths (in B not A)") used sync-direction labels that contradict the LOCKED B-relative-to-A diff semantics defined by the plan's own objective and must_haves — implemented verbatim, `--all-changed` would try to read members that exist only in B from A's zip (NotFound on any project where B has extras)
- **Fix:** Mapped upserts = diff's `removed`+`changed` (everything A has that B lacks/differs) and `--delete` removals = diff's `added` (B-only extras), with a loud LABEL RECONCILIATION comment at the mapping site; the contract tests pin the corrected semantics both directions
- **Files modified:** crates/ignition-core/src/actions/projects.rs
- **Verification:** project_sync_all_changed_delete_removes_b_only_members + project_sync_reads_then_overwrite_imports_into_b green; the e2e witness's diff assertion (`removed` for A-only) aligns
- **Committed in:** c9a233e

**2. [Rule 2 - Missing Critical] Zero-write honesty for empty sync selections**
- **Found during:** Task 3 (action implementation)
- **Issue:** The plan never addressed `--all-changed` resolving to an empty set — performing the whole-project overwrite-import anyway would ship an unmodified-but-rewritten zip to B (byte-differing zips, descriptor churn) with zero promotion value
- **Fix:** An empty effective selection (no upserts, no removals) performs NO import — empty synced/removed lists, exit 0; contract-pinned
- **Files modified:** crates/ignition-core/src/actions/projects.rs
- **Verification:** project_sync_all_changed_with_nothing_changed_imports_nothing green (export mocks consumed, import never fires)
- **Committed in:** c9a233e

---

**Total deviations:** 2 auto-fixed (1 bug, 1 missing critical)
**Impact on plan:** Both fixes are correctness requirements flowing directly from the plan's own must-have truths. No scope creep.

## Issues Encountered
- wiremock matches the FIRST-mounted mock on a server — a bundled zero-expect import guard shadowed the later scoped import guard in the sync tests; resolved by mounting export-only on the import side (pattern recorded above)
- `MockServer::received_requests()` returns `Option<Vec>` (vs the scoped guard's bare `Vec`) — unwrap_or_default at the binary-level refusal assertion
- The InvalidInput shared hint (--file/stdin text) rides the diff same-profile golden — the existing slug's frozen hint, unchanged

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- SYNC-01 and SYNC-02 delivered; the two-client resolution shape and the diff engine are available for any future multi-profile verb
- 07-02 (backup standalone + EAM) proceeds on the untouched seams; the guard-before-resolution and additive-slug conventions carry over
- The e2e sync witness needs a second live gateway (IGNITION_LIVE_URL_B) to actually run — the opt-in env-skip keeps CI green without it

---
*Phase: 07-ecosystem-interop-advanced-ops*
*Completed: 2026-08-28*

## Self-Check: PASSED
