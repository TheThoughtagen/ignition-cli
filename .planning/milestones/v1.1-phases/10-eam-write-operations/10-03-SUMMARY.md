---
phase: 10-eam-write-operations
plan: 03
subsystem: core-actions
tags: [eam, lifecycle, suspend, resume, cancel, modify, delete, blast-radius, rust, gateway-api]

# Dependency graph
requires:
  - phase: 10-eam-write-operations/02
    provides: "8 EAM write trait methods on GatewayApi (suspend/resume/cancel/scheduled/modify/delete) + capture-locked models (EamScheduledTask 13 keys, ModifyOutcome, DeleteOutcome) + REQUEST-pinning contract suite"
  - phase: 10-eam-write-operations/01
    provides: "10-LIVE-CAPTURES.md — 12 decisions locked by both-rig captures (isSuspended sync, can* truth table, delete-confirm policy, rename-404, full-record PUT echo, no-404-on-runtime-seam)"
provides:
  - "eam_task_suspend/resume/cancel actions with authoritative re-checks (suspend_recheck, cancel_decision — pure) and the all-keys EamLifecycleResult"
  - "eam_task_modify full-record RMW (TaskChange targeted keys, original-signature PUT, never-compose-from-scratch) + eam_task_delete signature-derived with the Decision-3 confirm-retry policy"
  - "BlastRadiusPreview + build_blast_radius composer (find + both scheduled segments, history deliberately excluded) + render_preview_line single-line format"
  - "eam_task_force rides the composer (EAMW-04) — preview rides EamTaskForceResult additively"
  - "lifecycle_precheck pure fn exported for the CLI/TUI three-place rule"
affects: [10-04 (CLI verbs render the preview line + precheck), 10-05 (gates), 12 (TUI Confirm modal body)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "double-check ladder: pure re-check fns (suspend_recheck, cancel_decision, lifecycle_precheck) shared by main.rs/TUI/action — the action is the correctness path"
    - "evidence-based mismatch classification: stale signature proven by post-failure find (client stays classification-free; no new slugs)"
    - "full-record clone via serde round-trip with null-placeholder stripping (the PUT carries only keys the wire answered)"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/actions/eam.rs
    - crates/ignition-core/tests/eam_contract.rs

key-decisions:
  - "Cancel no-op is representable in the all-keys model: fired=false + reason carries the plan's cancelled:false / reason semantics (the 5-key sketch alone was ambiguous for agents)"
  - "Rename omitted from TaskChange per capture §5 (PUT-rename is 404) and suspend flag omitted (PUT-driven isSuspended unproven — the verbs are the capture-locked path); the create-new+delete-old composite is documented for 10-04"
  - "Signature-mismatch 500s classify on EVIDENCE (post-failure find shows a changed signature ⇒ exit 2 re-run guidance) — the substring never reaches the action layer because classify's Internal fallback drops non-HTML bodies; no new slugs"
  - "Pending reads never short-circuit: a Running row lives only in scheduled/true (uncapturable on the 10-01 rigs) — a short-circuit could report nothing-pending while an execution is in flight"
  - "The modify clone drops the model's null scheduledTaskState placeholder so the PUT body is the find body + targeted keys only"
  - "Action-level wire composition pinned in tests/eam_contract.rs (find→write→read-back sequences) — the re-check branches are only provable end-to-end; unit tests stay wiremock-free per plan"

patterns-established:
  - "all-keys-always result models: nulls are honest absence, never omitted keys"
  - "pure-fn re-check extraction: decision logic testable without network doubles, async actions compose client calls + pure decisions"

# Metrics
duration: 3h 23m
completed: 2026-09-09
---

# Phase 10 Plan 03: EAM Action Layer Summary

**Suspend/resume/cancel/modify/delete actions with authoritative re-checks plus the blast-radius preview composer (find + both scheduled reads) feeding force and the future CLI/TUI confirm tiers — all wire-honest against the 12 capture-locked decisions.**

## Performance

- **Duration:** 3h 23m (host under heavy load — full-suite runs twice exceeded shell timeouts; recovered via per-binary incremental runs)
- **Started:** 2026-09-09T14:02:11Z
- **Completed:** 2026-09-09T17:25:21Z
- **Tasks:** 3
- **Files modified:** 2 (actions/eam.rs +1388/-26 lines, eam_contract.rs +665 lines)

## Accomplishments
- Five write verbs execute through actions/eam.rs, each carrying an authoritative re-check before the write fires (suspend refuses already-suspended exit 2; cancel mirrors the gateway's canCancel/no-pending truth; resume fires unconditionally per §1b)
- Blast-radius preview composes from read-only pre-flight reads only (find + scheduled/true + scheduled/false) into one all-keys struct — force now rides it (EAMW-04)
- Modify is full-record read-modify-write: clone every find key, mutate only targeted keys, PUT with the ORIGINAL signature + collection — the 422-trap invariant (config.settings required) pinned ON THE WIRE by contract test
- Delete derives the signature from find and implements Decision 3 exactly: no confirm by default, one sanctioned confirm=true retry on the (unobserved, spec-shaped) demand body
- Zero new CoreError variants (36 before == 36 after); the signature-mismatch FINDING handled by evidence-based staleness classification at the action layer

## Task Commits

Each task was committed atomically:

1. **Task 1: Runtime lifecycle actions — suspend/resume/cancel with authoritative re-checks** - `fc96016` (feat)
2. **Task 2: Config mutation actions — modify (full-record RMW) and delete (signature-keyed)** - `1a33a65` (feat)
3. **Task 3: Blast-radius preview — pure composer over read-only pre-flight reads** - `2e44005` (feat)

## Files Created/Modified
- `crates/ignition-core/src/actions/eam.rs` — the write surface: EamLifecycleResult, lifecycle_precheck, suspend_recheck, CancelDecision, eam_task_suspend/resume/cancel, TaskChange, apply_task_change, eam_task_modify, eam_task_delete, reclassify_stale_signature, affected_resources, BlastRadiusPreview, controller_impact, compose_blast_radius, build_blast_radius, render_preview_line, force-through-composer (+17 unit tests)
- `crates/ignition-core/tests/eam_contract.rs` — action-level wire pins: suspend find→POST→read-back, already-suspended pre-write refusal (expect(0) POST), cancel no-op and fire paths, modify never-compose-from-scratch body pin, delete no-confirm/confirm-retry/stale-signature sequences, force five-request sequence + preview composition (+8 tests, 2 updated)

## Decisions Made
- **EamLifecycleResult carries `fired` + `reason`** beyond the plan's 5-key sketch: the plan's own cancel paragraph demands `cancelled: false` + `reason: "no pending execution"` on the no-op, which the 5-key sketch couldn't represent unambiguously — all keys always serialize (nulls honest)
- **rename and suspend_flag omitted from TaskChange** exactly as the plan's capture-honesty clause directs (§5 proves rename-PUT is 404; PUT-driven isSuspended unproven) — composite workflow documented in the struct docs for 10-04
- **Signature-mismatch classified on evidence, not substring**: the client's Internal fallback drops the JSON problem body, so the substring never reaches the action layer — a post-failure find showing a changed signature (captures prove mismatches leave resources untouched) proves the conflict ⇒ exit 2 with re-run guidance; no evidence propagates the original error verbatim
- **Pending reads read BOTH scheduled segments always** (no short-circuit): Running rows live only in scheduled/true and were uncapturable — short-circuiting risked the one lie a cancel must never tell
- **The clone strips the null scheduledTaskState placeholder** the model serializes when the find answer carries the state under `healthchecks` — the PUT body carries only keys the wire answered
- **Action-level wire tests added to the contract suite** (beyond the plan's model+pure-fn unit scope): the re-check branches are "visible" only end-to-end; find→write→read-back sequences now pinned with expect(0)/expect(n) guards

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Existing force contract test updated for the composer sequence**
- **Found during:** Task 3
- **Issue:** `task_force_is_the_three_request_sequence` pinned force as exactly 3 requests; routing force through build_blast_radius makes it 5 (find + both scheduled segments + POST + history), and the missing scheduled mocks would 404
- **Fix:** Renamed to `task_force_is_the_five_request_sequence`, added both scheduled mocks, updated the sequence assertion; same treatment for the owner-fallback test; added the preview-composition pin
- **Files modified:** crates/ignition-core/tests/eam_contract.rs
- **Verification:** full eam_contract suite green (38 passed)
- **Committed in:** 2e44005 (Task 3 commit)

**2. [Rule 3 - Blocking] wiremock 0.6 API mismatch in new tests (Response → ResponseTemplate)**
- **Found during:** Task 1
- **Issue:** stateful test responder closures must return `wiremock::ResponseTemplate` in wiremock 0.6 (no `wiremock::Response` type exists)
- **Fix:** find_responder helper returns ResponseTemplate::new(200).set_body_json(body)
- **Files modified:** crates/ignition-core/tests/eam_contract.rs
- **Verification:** contract suite compiles + green
- **Committed in:** fc96016 (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (1 bug, 1 blocking)
**Impact on plan:** Both fixes confined to test infrastructure; no scope creep, no production-code deviation from the plan's capture-locked decisions.

## Issues Encountered
- Host load (up to ~57) twice pushed full-suite runs past shell timeouts — recovered by running test binaries incrementally with file-logged results (lib 379 + 18 integration suites, all green; final full `cargo build --workspace` and `--no-default-features` both clean)
- wiremock's default 404-on-no-match initially masqueraded as a "stale signature" in the modify test (the action's own diagnostic fired on the unmatched PUT) — switching the pin to the file's capture-and-assert convention surfaced the real cause (the null placeholder diff) and made the test stronger

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- 10-04 (CLI verbs) has everything it needs: `lifecycle_precheck` for pre-resolution guards, `build_blast_radius` + `render_preview_line` for preview-before-confirm, all five actions for the correctness path, and the documented create-new+delete-old rename composite
- The TUI tier (Phase 12) can reuse `BlastRadiusPreview` as the Confirm modal body — one composer, all caller tiers agree
- No blockers; the wire path is double-pinned (10-02 client REQUEST pins + 10-03 action sequence pins)

---
*Phase: 10-eam-write-operations*
*Completed: 2026-09-09*

## Self-Check: PASSED

- crates/ignition-core/src/actions/eam.rs — FOUND
- crates/ignition-core/tests/eam_contract.rs — FOUND
- Commits fc96016 / 1a33a65 / 2e44005 — all FOUND in git log
- Unit suite: 379 passed / 0 failed; eam_contract: 38 passed / 0 failed (file-logged full runs)
