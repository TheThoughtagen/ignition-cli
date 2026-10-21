---
phase: 23-guarded-apply-rollback
plan: 02
subsystem: deploy
tags: [error-slugs, exit-codes, gates, plan-sha256, rust, tdd]

requires:
  - phase: 22-transport-plan-pull-verify
    provides: DeployPlanResult, plan_sha256, PlanEntry flags, the four-place slug rule and the DeployLedgerInvalid template
  - phase: 23-guarded-apply-rollback
    provides: 23-01 captures (C-sysname, C-restart, C2, C11, C-gwbk) that item 2 and item 9 rest on
provides:
  - the Phase 23 contract vocabulary, confirmed by the user (option-a) and recorded below
  - eight CoreError variants and slugs under the four-place rule (DEPLOY_SLUGS is 12), PreflightFailure and RollbackFailure
  - ROLLED_BACK_SLUG, the verdict-only slug of Decision A
  - deploy::apply::gates, the pure DAPL-01 and DAPL-02 gate layer, with tests over a real plan
affects: [23-03, 23-18, 23-04, 23-05, 23-17, 23-07, 23-09, 23-11, 23-12, 23-13, 23-21]

actuals:
  tokens: 14900
  tasks: 3
  commits: 4

tech-stack:
  added: []
  patterns:
    - "gate flags are validated inside evaluate_gates, so a malformed flag is invalid_input whatever the plan says"
    - "refusal payloads carry the first 10 keys plus the full count; Display collapses the rest into 'and N more'"

key-files:
  created:
    - crates/ignition-core/src/deploy/apply/mod.rs
    - crates/ignition-core/src/deploy/apply/gates.rs
    - crates/ignition-core/tests/deploy_apply.rs
  modified:
    - crates/ignition-core/src/deploy/mod.rs
    - crates/ignition-core/src/error.rs
    - README.md

key-decisions:
  - "Checkpoint option-a adopted unchanged: items 1 to 10 below, including the REST-to-host wrong-target binding (C-sysname showed the two names agree on a fresh rig)"
  - "DeployPreflightFailed.failures is Box<[PreflightFailure]>, not Box<Vec<_>>: clippy box_collection forbids the latter; construct with vec.into()"
  - "evaluate_gates validates --plan-sha256 and the drift reason first, so a malformed flag is invalid_input (exit 2), never plan_stale"
  - "Gates skip only unmanaged entries; has_changes already covers a delete-set file, because plan marks such a unit changed or removed"

requirements-completed: []
requirements-advanced: [DAPL-01, DAPL-02, DAPL-04, DAPL-07, DAPL-08, DAPL-09]

coverage:
  - id: D1
    description: "A stale --plan-sha256 over a real plan refuses as plan_stale (exit 6) through the gate layer"
    requirement: "DAPL-02"
    verification:
      - kind: test
        ref: "crates/ignition-core/tests/deploy_apply.rs::gates_stale_hash_refuses_over_a_real_plan"
        status: pass
    human_judgment: false
  - id: D2
    description: "Gate order: flags, stale, noop, shadows_removal, destructive, drift, ready; unmanaged and drift None never gate"
    requirement: "DAPL-01"
    verification:
      - kind: test
        ref: "crates/ignition-core/src/deploy/apply/gates.rs tests::order_* and unmanaged_entries_never_gate"
        status: pass
    human_judgment: false
  - id: D3
    description: "An extra delete-set file after review changes plan_sha256, so plan_stale precedes the destructive gate"
    requirement: "DAPL-02"
    verification:
      - kind: test
        ref: "crates/ignition-core/tests/deploy_apply.rs::gates_extra_delete_changes_hash_before_destructive"
        status: pass
    human_judgment: false
  - id: D4
    description: "The eight slugs exist under the four-place rule with README rows; deploy_rolled_back is not a CoreError or exit row"
    requirement: "DAPL-04"
    verification:
      - kind: test
        ref: "error::tests::{readme_exit_table_agreement, exit_code_mapping_enumerated, deploy_slugs_have_their_exit_codes} and ignition-cli deploy_family_coverage::every_deploy_slug_is_in_the_readme_exit_table"
        status: pass
    human_judgment: false
  - id: D5
    description: "DeployJournalPresent carries the exact recovery command in Display; RollbackFailed carries the D-12 guidance"
    requirement: "DAPL-08"
    verification:
      - kind: test
        ref: "error::tests::journal_present_message_carries_the_recovery_command and rollback_failed_message_names_run_dir_journal_and_the_guidance"
        status: pass
    human_judgment: false
  - id: D6
    description: "Same plan and flags give an equal outcome and byte-identical prose; drift reason validated on every evaluation"
    requirement: "DAPL-01"
    verification:
      - kind: test
        ref: "tests/deploy_apply.rs::gates_are_deterministic and gates.rs tests::drift_reason_is_validated_on_every_evaluation"
        status: pass
    human_judgment: false

duration: 70min
completed: 2026-10-16
status: complete
---

# Phase 23 Plan 02: Contract Vocabulary, Error Slugs and Gate Layer Summary

**The user confirmed Phase 23's public vocabulary (option-a, all ten items), then eight new error slugs under the four-place rule (`DEPLOY_SLUGS` now 12) and a pure `evaluate_gates` (flags, `plan_stale`, noop, `shadows_removal`, destructive, drift) proven over a real plan landed.**

## Checkpoint outcome

Task 1 was a `checkpoint:decision` (blocking), answered before this run by the user through the orchestrator: **"option-a"**. Every research proposal is adopted as written in the option-a `<pros>` of `23-02-PLAN.md` Task 1, items 1 to 10, unchanged. 23-01's C-sysname capture (`23-LIVE-CAPTURES.md`) showed REST `gateway-info.name` and on-disk `systemName` agree on a fresh rig, so item 2's REST-to-host binding stands and is not dropped. Items 1 to 10 as chosen, verbatim:

(1) `plan_stale` 6 (review hash or under-lock pre-image mismatch; expected and actual hashes), `destructive_change_refused` 2 (gate `destructive` or `drift`, first 10 entry keys, count), `deploy_blocked` 6 (`shadows_removal` entry keys), `deploy_preflight_failed` 6 (every failed check id with a value-free detail), `deploy_journal_present` 6 (run id, env, journal location; message carries `ign deploy rollback --run <id> --env <env>`), `deploy_live_changed` 6 (run id, first 10 changed paths, count), `deploy_run_invalid` 6 (run id, problem), `rollback_failed` 6 (run id, run dir, on-box journal path, cause, D-12 guidance).

(2) REST `gateway-info` `name` must equal the host's `systemName` from `ignition/system-properties` (one scoped host read), plus an OPTIONAL `nodes.<node>.gateway_name`; when set, the REST name must also equal it. If C-sysname showed the two names disagree on a fresh rig, the REST-to-host binding is dropped and only the optional field applies.

(3) `--plan-sha256` is required (`invalid_input`, exit 2, before any host or gateway contact) when `--json` is given or stdin or stdout is not a terminal (`std::io::IsTerminal`); optional otherwise.

(4) Touched set: every path apply writes or deletes plus the ledger (key `ledger.json`); unrelated live edits never block a rollback.

(5) Under `ign-deploy/<gateway>/<mode>/`: `journal.json` `{schema_version:1, run_id, gateway, mode, phase, pre_fingerprint, touched[]}` with phases `snapshotting`, `ready`, `writing`, `written`, `restoring`; `snapshots/<run_id>/files/<path>` pre-image copies, `snapshots/<run_id>/ledger.json` (prior ledger, when one existed), `snapshots/<run_id>/record.json` `{schema_version:1, run_id, gateway, mode, render_sha256, plan_sha256, state (applying, applied, rolled_back), rolled_back_by, touched[{path, pre_sha256, post_sha256}], pre_fingerprint, post_fingerprint}`; `staging/<run_id>/` holds the staged files, `work.tsv`, the script and pre-rendered journal and record files; all JSON strict (unknown fields refused, 8 MiB cap, byte-sorted unique paths); fingerprint = SHA-256 over `path NUL (sha256 or "-") LF` in byte order.

(6) The run dir is created after the gates and `--yes` and after the git-ignore guard, before the preflights; flag errors, a journal, `plan_stale`, noop, gate and confirmation refusals write none. `git check-ignore` exit 0 proceeds, 1 refuses (`deploy_preflight_failed` check `run_dir_not_ignored`, nothing written), 128 (not a work tree) proceeds and records `run_dir_git: "not_a_repo"`, git not found refuses (fail closed).

(7) Journal check first (before the plan), then plan, `plan_stale` before noop, shadows, destructive, drift, `--yes`, then the run dir and preflights. A pre-image mismatch under the lock aborts with nothing moved, issues one no-op scan to release the lock (D12), and raises `plan_stale`.

(8) `data.outcome`: apply `applied`, `noop`, `rolled_back`; rollback `restored`, `cleared` (D-10). `data.cause`: `lock_expired`, `write_failed`, `scan_incomplete`, `verify_failed`. Preflight check ids: `mode_master`, `mode_backup`, `wrong_target`, `write_rest`, `write_host`, `designers`, `hold_budget`, `gwbk`, `run_dir_not_ignored`.

(9) `restart_pending` (the post-apply list) and `restart_pending_new` (entries absent before apply); a 403 or unreadable endpoint is the warning `restart_pending_unavailable`, never a failure.

(10) A journal left between a green verify and finalize is recovered by restoring (the safe direction, documented, A9); no Ctrl-C handler in v1.6 (A10, documented).

Already decided and not asked (from the plan): Decision A (`deploy_rolled_back` in `data`, exit 6, never an exit-table row), Decision C's existence, D-01 to D-15, SF-3, SF-6, no `--force`, `--skip-verify` or `--no-rollback`, one `restore_snapshot`.

What this plan implements of the outcome: item 1 (the eight slugs, exits and payload fields), item 7's gate order up to the `--yes` step (the part that is a pure function of plan and flags), and the flag rules of item 3 (`validate_plan_sha256` and the empty `--plan-sha256` case being the CLI's concern, 23-13). Items 2, 4, 5, 6, 8, 9 and 10 are names and rules that later plans read from this section; no code for them lands here.

## Performance

- **Duration:** about 70 min (most of it cargo builds)
- **Tasks:** 3 of 3 (Task 1 was the decision checkpoint, answered before this run)
- **Files:** 3 created, 3 modified

## Accomplishments

- `deploy::apply` module with `ROLLED_BACK_SLUG = "deploy_rolled_back"` (verdict only; `grep -c '"deploy_rolled_back"' error.rs` prints 0).
- `evaluate_gates(plan, flags)` in the confirmed order, `confirmation_prose` (no file content, no diff, byte-stable), `validate_plan_sha256`, `validate_drift_reason` (non-empty, at most `DRIFT_REASON_MAX_CHARS` = 500 characters, no control characters).
- Eight `CoreError` variants: `PlanStale`, `DestructiveChangeRefused` (exit 2), `DeployBlocked`, `DeployPreflightFailed`, `DeployJournalPresent`, `DeployLiveChanged`, `DeployRunInvalid`, `RollbackFailed`, each in the four places (variant with `code()`, `exit_code()`, `hint()` and `endpoint()` None; the error.rs header table; the README `## Exit codes` rows 2 and 6; `EXIT_SLUG_LITERALS` and `exit_code_mapping_enumerated`), appended to `DEPLOY_SLUGS` in the confirmed order.
- Tests: 17 gate unit tests, 5 `deploy_apply` integration tests over a real plan, 7 new error tests.

## Task Commits

1. **Task 1 (checkpoint:decision):** no commit; the answer is recorded above.
2. **Task 2 (tracer), RED:** `17f3a3e` (failing gate tests over a real plan)
3. **Task 2 (tracer), GREEN:** `5ab4900` (`plan_stale` slug and the stale-hash gate)
4. **Task 3, RED:** `6e2db4c` (gate-order matrix, drift-reason, determinism and concurrency tests)
5. **Task 3, GREEN:** `067d5bd` (seven slugs, full gate order)

**Plan metadata:** the final docs commit (SUMMARY, STATE, ROADMAP) follows this file.

## Tracer feedback gate

Auto mode is not active (`workflow._auto_chain_active` and `workflow.auto_advance` are false), so the interactive rule would stop after the tracer commit with a `checkpoint:human-verify`. This project runs `workflow.human_verify_mode = end-of-phase` and the orchestrator directed Tasks 2 and 3 to continue (the same call as 23-01). The tracer's own `<verify>` (`cargo test -p ignition-core --test deploy_apply gates_`, the gates lib tests and the `error::` tests) passed before the tracer commit `5ab4900` and again after Task 3. The slice is a pure function with no human-verifiable surface.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `Box<Vec<PreflightFailure>>` trips clippy `box_collection`**
- **Found during:** Task 3 (design of the variant, before the clippy run)
- **Issue:** the plan specifies `failures: Box<Vec<PreflightFailure>>`; `clippy::box_collection` warns on that type and `-D warnings` is the verification gate.
- **Fix:** `failures: Box<[PreflightFailure]>`; callers build it with `vec.into()`. Same boxing effect, same payload.
- **Files modified:** `crates/ignition-core/src/error.rs`
- **Verification:** `cargo clippy --workspace --all-targets -- -D warnings` is clean.
- **Committed in:** 067d5bd

**2. [Rule 1 - Bug] The `invalid_input` hint for the gate flags pointed at `--file`**
- **Found during:** Task 3
- **Issue:** `InvalidInput`'s default hint says "a readable file path via --file, or `-` to pipe the content on stdin", meaningless for a bad `--plan-sha256` or `--allow-drift` value.
- **Fix:** a content-addressed branch (the 06-07 pattern) for reasons starting `--plan-sha256 ` or `--allow-drift `, with a test.
- **Files modified:** `crates/ignition-core/src/error.rs`
- **Committed in:** 067d5bd

**3. [Rule 2 - Missing critical functionality] `evaluate_gates` validates the flags itself**
- **Found during:** Task 3
- **Issue:** the truths require a malformed `--plan-sha256` or drift reason to be `invalid_input` (exit 2) and "validated the same way every time"; leaving that to the CLI alone would let a malformed hash surface as `plan_stale`.
- **Fix:** `evaluate_gates` validates both flags first (pure, no I/O). The CLI still validates before any host or gateway contact (23-13).
- **Committed in:** 067d5bd

---

**Total deviations:** 3 auto-fixed (1 blocking, 1 bug, 1 missing critical). **Impact:** no scope change.

**Other plan departures, not rule-driven:**
- The Task 3 acceptance line `grep -c '...' README.md prints at least 8` counts lines, and the eight slugs sit in two long table rows, so it prints 2. `grep -o ... | sort -u | wc -l` prints 8, which is the intent. The pinned tests (`every_deploy_slug_is_in_the_readme_exit_table`, `readme_exit_table_agreement`) check each slug exactly.
- `requirements-completed` is empty although the plan frontmatter lists DAPL-01, DAPL-02, DAPL-04, DAPL-07, DAPL-08 and DAPL-09: this plan lands the vocabulary and the pure gates only. The verb, preflights, engine, rollback and journal recovery that those requirements describe arrive in 23-04 to 23-21, so `requirements mark-complete` was run once and reverted (REQUIREMENTS.md is unchanged) rather than leave six requirements checked that no user-visible command satisfies yet. The same call was made in 23-01.
- The RED commit of Task 2 used `err.code()` assertions (the `PlanStale` variant did not exist yet); the GREEN commit added the `matches!(err, CoreError::PlanStale { .. })` assertion.

## Auth Gates

None.

## Known Stubs

None. `grep` for `todo!`, `unimplemented!`, `TODO` and `FIXME` over `deploy/apply` and `tests/deploy_apply.rs` finds nothing.

## Threat Flags

None. This plan adds no endpoint, no file access and no schema; the new surface is error text, which T-23-07 covers (keys, paths, counts and hashes only, lists capped at 10). T-23-05 (`shadows_removal` beats every flag), T-23-06 (stale hash first; `gates_extra_delete_changes_hash_before_destructive`) and T-23-08 (`validate_drift_reason`) are mitigated and tested.

## Verification

- `cargo test -p ignition-core --test deploy_apply`: 5 passed
- `cargo test -p ignition-core --lib -- deploy::apply error::`: 37 passed
- `cargo test -p ignition-cli --test deploy_family_coverage`: 5 passed
- `cargo clippy --workspace --all-targets -- -D warnings`: clean
- Full `cargo test -p ignition-core -p ignition-cli --no-fail-fast`: 1900 passed, 0 failed, 83 test binaries

## Next Phase Readiness

- 23-03 and 23-18 read items 4 and 5 (touched set, journal, record and staging layouts, fingerprint) from the Checkpoint outcome above.
- 23-07 reads item 2 (wrong-target rule and the optional `nodes.<node>.gateway_name`, which is a target-schema change owned there) and the preflight check ids of item 8.
- 23-13 reads item 3 (the Decision C rule), item 6 and item 7 for the verb, and wires `GateFlags` from the CLI flags.
- `PreflightFailure`, `RollbackFailure` and the eight variants are ready to be raised by the engine plans.

## Self-Check: PASSED

- Created files exist: `deploy/apply/mod.rs`, `deploy/apply/gates.rs`, `tests/deploy_apply.rs`, this SUMMARY.
- Commits `17f3a3e`, `5ab4900`, `6e2db4c`, `067d5bd` exist in `git log`.
- `## Checkpoint outcome` is present with items 1 to 10.
