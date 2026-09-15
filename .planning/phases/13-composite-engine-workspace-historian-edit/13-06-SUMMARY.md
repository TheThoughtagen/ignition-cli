---
phase: 13-composite-engine-workspace-historian-edit
plan: 06
subsystem: core-library
tags: [workspace, status, push, three-way-compare, conflict-refusal, preview_then_confirm, wiremock, traffic-pinning, fresh-export-splice]

# Dependency graph
requires:
  - phase: 13-03
    provides: WorkspaceManifest + read_manifest (strict read ownership, stable refusal prefixes) and checkout recording pairs + zip-side descriptor-normalized hashes
  - phase: 13-02
    provides: MemberSource (Zip::member_hashes descriptor-normalized) + fnv1a/normalize_descriptor/FOLDER_DESCRIPTOR primitives + replace_member/remove_member splice engine
provides:
  - actions/workspace.rs WorkspaceStatus/StatusRow/StatusKind — the push-relative three-way compare envelope (Clean/LocalEdit/GatewayDrift/Conflict + Deleted{local}/Added/Untracked)
  - classify() — pure total three-way matrix over all 8 Option combinations (pub; the verdict core unit-testable without IO)
  - workspace_status — reads the RECORDED manifest, gateway via MemberSource::Zip::member_hashes, local via Tree-equivalent per-member hash at recorded paths (one hash/normalize implementation, no byte-compare)
  - workspace_push — conflict refusal (NOT --yes-able, Pitfall W2), --delete opt-in, zero-write honesty, ONE require_confirmation gate whose message IS the preview, manifest-recorded splice of raw local bytes into a FRESH export, exactly ONE import
  - PushPreview/PushOutcome envelope models
  - tests/workspace_status_push.rs — 17-test matrix/guard suite (exhaustive classify matrix, one-row-of-every-kind, volatility-not-drift, traffic-pinned refusals, splice-into-fresh proof)
affects: [13-07 workspace CLI (envelope + refusal shapes are the golden anchors), 13-05 edit slice (staleness/conflict vocabulary shared)]

# Tech tracking
tech-stack:
  added: [] # zero new dependencies
  patterns: [push-relative status labels (one direction, never mixed), recorded-manifest reads (never re-derive), fresh-export splice base (staleness-safe at tree level), one-gate preview_then_confirm in core (refusal message IS the preview), wiremock mount-order sequencing for multi-export flows]

key-files:
  created:
    - crates/ignition-core/tests/workspace_status_push.rs
  modified:
    - crates/ignition-core/src/actions/workspace.rs

key-decisions:
  - "StatusKind semantics are PUSH-RELATIVE by planner lock — each row says what push does to the gateway (local_edit=would write, gateway_drift=untouched, conflict=refused); pinned by an include_str!-prose test so the doc direction is contract"
  - "classify() is total over all 8 Option combos: absent side never equals a present hash (counts as moved, refined into Deleted{local} by the caller), both-absent = Clean (both sides deleted it — nothing to reconcile), no-baseline agreeing sides Clean / disagreeing Conflict"
  - "Locally-deleted + gateway-moved and locally-edited + gateway-deleted both classify Conflict — deleting/writing against a side that moved on is exactly the W2 clobber class, so refusal (never --yes-able) is the honest verdict"
  - "Push gate lives IN CORE (require_confirmation(yes, preview) -> ConfirmationRequired) unlike EAM where the gate is CLI-side — the plan signature carries yes:bool down, and one core fn means the refusal shape cannot drift; the preview text rides the operation field verbatim"
  - "Untracked walk excludes workspace machinery (both manifests, .gitignore) and *.py codec artifacts (the checkout .gitignore's own convention) — an untracked row is still dirt (clean=false)"
  - "wiremock 0.6 matches mocks in MOUNT ORDER (first mounted wins), so multi-export flows sequence via up_to_n_times on the EARLY mock mounted FIRST — test-discovered, now the file's documented convention"

patterns-established:
  - "Traffic-pinned guard proof: a refused push asserts exactly the read GETs and zero imports via .expect() counts plus a scoped zero-expectation import mock"
  - "Splice-into-fresh proof: sequenced export mocks make the gateway diverge on an UNRELATED member between checkout and push; the recorded import body must carry the gateway's version of that member and the local bytes of the edited one"
  - "Doc-prose pinning: the StatusKind direction contract is include_str!'d, whitespace/doc-marker normalized, and asserted phrase-by-phrase (readme_exit_table_agreement pattern adapted to source docs)"

# Metrics
duration: 23min
completed: 2026-09-15
---

# Phase 13 Plan 06: Workspace Status + Push Summary

**Manifest three-way status (push-relative Clean/LocalEdit/GatewayDrift/Conflict + Deleted/Added/Untracked) and guarded push: conflicts refuse unconditionally, deletions are `--delete` opt-in, empty selections never mutate, and confirmed pushes splice manifest-recorded local bytes into a FRESH export behind the single preview_then_confirm gate — SC-1's drift + guard halves code-complete pending CLI.**

## Performance

- **Duration:** 23 min
- **Started:** 2026-09-15T15:04:10Z
- **Completed:** 2026-09-15T15:27:18Z
- **Tasks:** 2
- **Files modified:** 2 (1 created, 1 modified)

## Accomplishments
- The three-way compare exists as ONE pure verdict (`classify`, total over all 8 Option combinations) plus a tolerant action layer: status reads the RECORDED manifest, hashes the gateway side through `MemberSource::Zip::member_hashes` and the local side through the Tree-equivalent per-member hash — the same `fnv1a` + `normalize_descriptor` primitives throughout, so `lastModification` volatility provably never masquerades as drift (pinned live: a descriptor-noise-only re-export statuses CLEAN)
- Status reports every row kind in one fixture: clean, local_edit, gateway_drift, conflict, deleted{local:true}, deleted{local:false}, added{local:false} (including the synthesized descriptor an append produces), untracked — with PUSH-RELATIVE semantics pinned in source docs by an include_str!-prose test
- Push is guard-honest end to end: any conflict refuses BEFORE the gate with both diverged sides named and a resolution hint (fires even WITH `--yes` — Pitfall W2); locally-deleted members delete only under `--delete` and are reported as skipped otherwise; an empty selection returns Ok with ZERO mutation requests and never prompts (traffic-pinned: exactly 1 read GET, 0 imports)
- The splice rides the RECORDED mapping (never re-derived) into a FRESH export: the crown test sequences the wiremock so the gateway diverges on an UNRELATED member between checkout and push, then re-parses the recorded import body — the edited member carries LOCAL bytes, the unrelated member carries the GATEWAY's version, nothing resurrected or lost
- The one-gate property is structural: `require_confirmation` is defined once and called from exactly one push code path; the refusal message IS the deterministic preview (asserted verbatim in the error envelope)

## Task Commits

Each task was committed atomically:

1. **Task 1: status — the manifest three-way compare over MemberSource** - `52926a8` (feat)
2. **Task 2: push — manifest-recorded splice into a fresh export, conflicts refuse, one guard gate** - `8ae3694` (feat)

## Files Created/Modified
- `crates/ignition-core/src/actions/workspace.rs` — StatusKind/StatusRow/WorkspaceStatus envelope, classify matrix, workspace_status, PushPreview/PushOutcome, render_push_preview + require_confirmation (the one gate), workspace_push
- `crates/ignition-core/tests/workspace_status_push.rs` (created) — 17-test suite: exhaustive classify matrix, one-row-of-every-kind integration, volatility-not-drift, machinery-excluding untracked walk, envelope shapes, refusal prefixes, and the seven traffic-pinned push tests

## Decisions Made
- StatusKind semantics PUSH-RELATIVE per planner lock; the doc direction is load-bearing contract pinned by test (include_str!-prose, doc-marker-stripped normalization)
- `classify` both-absent combination (baseline member deleted locally AND absent from the fresh export) = Clean — nothing to reconcile; push has nothing to delete and a refresh re-checkout drops the stale entry
- Conflict refinement: local-deleted + gateway-moved AND local-edited + gateway-deleted both land Conflict (the write/delete would clobber a side that moved on); only gateway-at-baseline local deletions refine to Deleted{local:true}
- The push confirmation gate lives in CORE (unlike EAM's CLI-side gate) because the plan signature carries `yes: bool` down — `require_confirmation(yes, preview)` returns `ConfirmationRequired` whose `operation` IS the preview line-set; one fn, one call site
- Untracked reporting excludes `*.py` codec artifacts and workspace-owned files — consistent with the checkout `.gitignore` convention; an untracked row still makes `clean` false (all-rows-always semantics)
- A deletion whose target vanished from the fresh export between status and splice is a skipped no-op (remove_member NotFound tolerated), not an error — the race is reported honestly

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Wiremock sequencing mounted backwards (matching is mount-order, not reverse)**
- **Found during:** Task 2 (splice-into-fresh + already-gone-delete tests)
- **Issue:** The sequenced-export mocks assumed most-recent-first matching; wiremock 0.6 matches in MOUNT ORDER, so checkout silently consumed the "moved-on" zip and one test failed on a manifest key lookup
- **Fix:** mount the early mock (with `up_to_n_times`) FIRST, fallback second; documented as the file's sequencing convention in test comments
- **Files modified:** crates/ignition-core/tests/workspace_status_push.rs
- **Verification:** all three affected tests green; the splice-into-fresh assertions now genuinely compare zip0-vs-zip1 member bytes
- **Committed in:** 8ae3694 (Task 2 commit)

**2. [Rule 1 - Bug] Conflict fixture used lastModification-only descriptor changes (normalization sees through them)**
- **Found during:** Task 1 (every-kind status test)
- **Issue:** the gateway-side and local-side descriptor "edits" initially differed only in time/signature noise, which `normalize_descriptor` strips — the row classified LocalEdit/Clean instead of Conflict
- **Fix:** conflict fixture changes are SEMANTIC (`version` field 1→2 gateway-side, →3 locally); the volatility-only form is retained in its own dedicated clean-status test
- **Files modified:** crates/ignition-core/tests/workspace_status_push.rs
- **Verification:** every-kind test reports Conflict; volatility test reports Clean on the same fixture family
- **Committed in:** 52926a8 (Task 1 commit)

**3. [Rule 1 - Bug] Entry-count assertions forgot the export's `project.json`**
- **Found during:** Task 2 (import-body re-parse assertions)
- **Issue:** `body.len()` counts ALL zip entries including `project.json` (which checkout never writes into the tree but the export/splice always carries); two asserts under-counted by one
- **Fix:** corrected to 6 (five members + project.json) and 7 (six members + project.json) with the arithmetic named in the assertion message
- **Files modified:** crates/ignition-core/tests/workspace_status_push.rs
- **Verification:** body re-parse assertions green
- **Committed in:** 8ae3694 (Task 2 commit)

---

**Total deviations:** 3 auto-fixed (3 bugs, all test-fixture level; zero product-code deviations)
**Impact on plan:** All three were test-harness corrections discovered by the tests doing their job; the product surface landed exactly as planned. No scope creep.

## Issues Encountered
- The parallel 13-05 agent landed `a1892b5` (Editor seam) and uncommitted edit_pipeline.rs WIP mid-execution, making a whole-crate `cargo test`/`cargo fmt`/clippy --all-targets run red through no fault of this plan. Verification was scoped per the parallel-wave discipline: this plan's test binary (17/17), the 13-03 checkout suite (9/9), lib-only clippy `-D warnings` (clean), and rustfmt --check on exactly this plan's two files (clean). The 13-05 WIP failures belong to that plan's own loop.
- wiremock 0.6.5's `expect()` takes u64 (not usize) — test-side `as u64` casts at the two helper boundaries.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness
- 13-07 CLI wiring is unblocked: `WorkspaceStatus`/`StatusRow`/`PushPreview`/`PushOutcome` are the envelope shapes (additive discipline); the stable refusal texts (conflict refusal naming both sides, the gate's preview verbatim in `ConfirmationRequired.operation`) are the golden anchors; status is ONE read GET, push is 1 GET (no-op) or 2 GETs + 1 POST (work)
- 13-05's edit slice shares the conflict/staleness vocabulary: `classify` is pub and the Deleted/Conflict refinements are documented for reuse
- README coverage of push semantics (untracked ignored, conflicts not --yes-able, --delete opt-in) belongs to 13-07's CLI task per the planner locks — not touched here
- Parallel-wave note: 13-05 WIP was in flight during execution; no cross-plan file conflicts (this plan touched only actions/workspace.rs + its test file)

---
*Phase: 13-composite-engine-workspace-historian-edit*
*Completed: 2026-09-15*

## Self-Check: PASSED

- Files verified on disk: actions/workspace.rs, tests/workspace_status_push.rs (987 lines, ≥ 80 min_lines), 13-06-SUMMARY.md
- Commits verified in history: 52926a8 (Task 1), 8ae3694 (Task 2)
- Test binary re-run at self-check: 17 passed, 0 failed
- Structural checks re-verified: require_confirmation defined once / one call site in workspace_push; splice path handles member bytes raw (no serde on bytes); only the plan's two files touched
