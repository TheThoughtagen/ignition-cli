---
phase: 13-composite-engine-workspace-historian-edit
plan: 05
subsystem: edit-pipeline
tags: [edit, editor-seam, tokio-process, tempdir, staleness, diff-members, real-process-tests, fail-closed]

# Dependency graph
requires:
  - phase: 13-composite-engine-workspace-historian-edit (13-03)
    provides: scripts_codec pub primitives (decode_export_tree / encode_export_tree / encode_member / MANIFEST_NAME) proven byte-exact at tree scale, and the checkout-era fixture style
  - phase: 13-composite-engine-workspace-historian-edit (13-02)
    provides: client/resources.rs mapping + member_hashes / resource_members / diff_members / MemberStatus (the snapshot, membership, and blast-radius machinery)
provides:
  - Editor seam trait + TokioEditor (VISUAL→EDITOR resolution, whitespace-split arg vector with target LAST, advisory exit)
  - EditTempDir (0700 private per-invocation tree, Drop guard, keep() for the fail-closed recovery path)
  - edit_pipeline / StagedEdit / EditStatus — fetch→snapshot→decode→content-decided no-op→fail-closed encode→staleness gate→staged push payload
  - Five-archetype adversarial-editor harness as REAL spawned processes (the roadmap's plan-level edit-slice verification)
affects: [13-07 (CLI rendering: golden-anchors on the stable refusal prefixes), 13-08 (README/edit UX: EDITOR='code --wait' doc, guard ladder consumes StagedEdit)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Content-decided no-op: byte-compare encode-vs-encode of identical trees (raw gateway container bytes are never byte-stable across writers)"
    - "Exit-status-is-advisory editor contract (Pitfall E1) — daemon writes cannot flip a verdict landed at encode time"
    - "Push stays OUT of core: the pipeline ends at the staged payload; gate composition is the dispatch layer's job (10-04 lesson)"
    - "Real-process test doubles: test-authored #!/bin/sh scripts spawned through the PRODUCTION arg-vector spawn site — nothing about the process layer mocked"

key-files:
  created:
    - crates/ignition-core/tests/edit_pipeline.rs
  modified:
    - crates/ignition-core/src/actions/edit.rs
    - crates/ignition-core/src/actions/mod.rs

key-decisions:
  - "13-05: no-op comparator = the ORIGINAL export's RE-ENCODE (second untouched decode + encode), not raw gateway zip bytes — our zip writer sorts and re-compresses, so container bytes differ from the gateway's even with identical member content; encode-vs-encode of identical trees is the byte-stable invariant 13-03 proved at member scale. Planner lock's own wording ('the ORIGINAL export zip's re-encode')"
  - "13-05: baseline re-encode computed BEFORE the editor runs — a codec failure on an unedited tree refuses before burning the user's editing session"
  - "13-05: push deliberately OUT of edit_pipeline's signature — StagedEdit{status, import_zip: Option<Vec<u8>>, project} ends at the staged payload; NoOp ⇒ import_zip None makes the no-push no-prompt rule STRUCTURAL (the caller cannot push bytes that do not exist) — spy-push fn made unnecessary by the corrected plan signature"
  - "13-05: staleness gate reuses member_hashes unchanged (invents NO etag), compares TARGET-member hash only, refuses InvalidInput 'changed on gateway since fetch', NOT --yes-able (Pitfall E2 — forcing would clobber a concurrent Designer edit)"
  - "13-05: fail-closed re-encode rides encode_member's InvalidInput VERBATIM (bare reason, not re-wrapped) with EditTempDir::keep() preserving the tree; the kept path rides the message"
  - "13-05: editor archetypes are real #!/bin/sh scripts spawned through run_editor_argv (the production arg-vector site) — the editor's IDENTITY is the test double, the spawn mechanics/arg-vector discipline/advisory-exit contract are the production path; daemon pinned BOTH content directions (unchanged-at-encode → NoOp; garbage landing before encode → fail-closed despite exit 0)"

patterns-established:
  - "Content-decides pattern: whether work happened is decided by comparing content at a fixed point in time, never by a process's exit code (daemon writes after the verdict point cannot flip it)"
  - "Structural-refusal pattern: when a rule must be unbreakable, shape the data so the forbidden action is impossible (import_zip: None) rather than asking the caller to remember"

# Metrics
duration: ~10 min continuation session (Task 1 committed 15:12Z; WIP resumed + completed after sibling 13-06 landed — full plan spanned 15:12Z→19:15Z Sep-15 across the interleave)
completed: 2026-09-15
---

# Phase 13 Plan 05: Edit Pipeline Summary

**`ign edit`'s core loop proven end-to-end: content-decided no-op, fail-closed re-encode with kept-tree recovery, NOT---yes-able staleness gate, and the five-archetype adversarial-editor harness as real spawned processes**

## Performance

- **Duration:** ~10 min (continuation session completing interrupted Task 2; Task 1 landed earlier at `a1892b5`)
- **Started:** 2026-09-15T19:05Z (continuation; Task 1 committed 15:12Z)
- **Completed:** 2026-09-15T19:15Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- The Editor seam: `Editor` trait + `TokioEditor` (VISUAL→EDITOR, trimmed, empty=unset; whitespace-split ARG VECTOR with target appended LAST — never a shell string; spawn failure = usage-class refusal naming the editor; exit status advisory by contract)
- `EditTempDir`: 0700-private unique per-invocation tree, Drop-guard removal, `keep()` recovery path — mode pinned by test
- `edit_pipeline`: fetch → `member_hashes` snapshot → whole-tree decode → pre-edit baseline re-encode → target resolution (member list refusals) → edit → fail-closed re-encode → content-decided no-op → staleness gate (fresh export, target-member hash) → `StagedEdit::Ready` with the member-level blast radius (`diff_members` B-relative)
- The FIVE-ARCHETYPE harness as REAL spawned processes: blocking vim-style → Ready; `--wait` deferred-exit → Ready (pipeline waits, exit advisory); daemon-style pinned BOTH ways (unchanged-at-encode → NoOp, landed garbage → fail-closed despite exit 0, post-encode write cannot flip); no-change → structural NoOp; JSON breaker → codec-verbatim fail-closed with the kept tree proven on disk
- Staleness gate both ways: drift refuses with the stable `changed on gateway since fetch` prefix (traffic-pinned: no-op edits never re-check — 1 GET); fresh export allows Ready (2 GETs)
- Real `TokioEditor` end-to-end test through env resolution (VISUAL) driving the actual pipeline

## Task Commits

Each task was committed atomically:

1. **Task 1: Editor seam trait + TokioEditor + private temp-dir guard** - `a1892b5` (feat)
2. **Task 2: edit_pipeline — fetch/decode/snapshot/open/compare/encode/staleness/diff-summary** - `45ddc7a` (feat)

## Files Created/Modified
- `crates/ignition-core/src/actions/edit.rs` - Editor/TokioEditor/EditTempDir + the pipeline (StagedEdit/EditStatus) + resolution/argvector/lifecycle unit tests
- `crates/ignition-core/src/actions/mod.rs` - `pub mod edit` declaration (Task 1)
- `crates/ignition-core/tests/edit_pipeline.rs` - The five-archetype real-process harness + staleness/no-op/fail-closed/target-resolution pins (11 tests)

## Decisions Made
- **No-op comparator = the original export's re-encode, not raw gateway bytes.** The plan body's literal "byte-compare vs original export bytes" is unworkable: our zip writer sorts and re-compresses, so container bytes differ from the gateway's even with identical member content — the raw comparison would never fire NoOp. The planner lock's own wording ("the ORIGINAL export zip's re-encode") is implemented: a second untouched decode + encode is the baseline, byte-stable by 13-03's proven round-trip invariant. Computed BEFORE the editor runs so a codec failure on an unedited tree refuses before burning the editing session.
- **Push stays out of core.** Per the plan's corrected signature: the pipeline ends at `StagedEdit { status, import_zip, project }`. The no-push rule on NoOp is structural (`import_zip: None` — the caller cannot push bytes that don't exist), replacing the plan's original spy-push fn.
- **Daemon archetype pinned both content directions** (deterministic `wait`-based child): unchanged-at-encode → NoOp; garbage landing before encode → fail-closed despite exit 0. Together with the post-encode-write test, "content decides, never exit code" is closed from every side.
- **Staleness reuses member_hashes unchanged** (no invented etag), target-member hash only, refusal is InvalidInput (not a confirmation gate — no flag shape satisfies it).

## Deviations from Plan

None - plan executed exactly as written (Task 2's plan text self-corrected mid-spec — StagedEdit signature replacing EditOutcome+push-fn — and the corrected contract is what shipped).

### WIP reconciliation (continuation note)

The interrupted prior executor left Task 2 ~95% complete (pipeline + 10 tests, compiling). This session diffed the WIP against the plan, found ONE genuine gap (archetype (c)'s "pinned both ways" — only the NoOp leg existed), added `daemon_child_write_landing_before_encode_fails_closed`, fixed a clippy `useless_format` + a fmt drift in the new test code, and re-verified everything fresh. No test intent was weakened; the WIP's no-op-comparator divergence from the plan's literal wording matched the planner lock and was confirmed correct.

## Issues Encountered
- The daemon test emits one benign stderr line (`sh: ...: No such file or directory`) — the background child's post-encode write landing after the tree's Drop. That is the test's thesis (a late write goes nowhere), not a failure.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- 13-07 (CLI edit command + guard goldens) anchors on the stable refusal prefixes landed here: `no $EDITOR set`, `the edited member no longer parses as JSON` (codec-verbatim), `changed on gateway since fetch`, and the `preserved at <path>` kept-tree clause
- 13-08 consumes `StagedEdit` (Serialize-ready: `status: no_op|ready`, `changed` list) for the guard ladder and documents `EDITOR="code --wait"` for IDE users
- Wave 3 fully complete (13-05 + 13-06); remaining: 13-07, 13-08
- `cargo test -p ignition-core`: 692 passed / 0 failed / 16 ignored (live-gate skips) — 13-02/13-03/13-06 suites confirmed green post-WIP; clippy -D warnings + fmt clean; scripts_codec.rs untouched (byte-faithfulness invariant intact)

---
*Phase: 13-composite-engine-workspace-historian-edit*
*Completed: 2026-09-15*

## Self-Check: PASSED
- `crates/ignition-core/src/actions/edit.rs` FOUND on disk
- `crates/ignition-core/tests/edit_pipeline.rs` FOUND on disk
- Task 1 commit `a1892b5` FOUND in git log
- Task 2 commit `45ddc7a` FOUND in git log
- Full crate suite: 692 passed / 0 failed / 16 ignored (live-gate skips); clippy -D warnings clean; fmt clean
