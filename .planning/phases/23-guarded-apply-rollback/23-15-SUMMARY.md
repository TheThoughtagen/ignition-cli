---
phase: 23-guarded-apply-rollback
plan: 15
subsystem: deploy
tags: [canary, secrets, dsurf-03, stdout-purity, tests, wiremock, readme, rust, tdd]
status: complete

requires:
  - phase: 23-guarded-apply-rollback
    provides: 23-13/23-21 the apply verb, mount_apply_gateway and the run_apply helpers; 23-14 the rollback verb, apply_through_binary, plant_journal_at, edit_live_unit; 23-12 journal recovery; 23-19 ApplyFault::HangAfterMove and FaultyHost
provides:
  - "CANARY_APPLY, CANARY_EXEMPT (snapshot/ and *.gwbk, D-05), scan_run_dir_for, assert_exempt_private and the CLI canary tests apply_canary, apply_canary_all_outcomes, rollback_canary, canary_repeat_runs_clean, apply_canary_drift_reason"
  - "core canary_on_box_files_clean and canary_aborted_run_clean (journal.json, record.json, ledger.json, partial events.ndjson)"
  - "stdout-purity cases deploy_apply_json_stdout_is_the_envelope_only and deploy_rollback_json_stdout_is_the_envelope_only under max diagnostics"
  - "crates/ignition-cli/tests/common/mod.rs: the apply/rollback harness shared by contract_deploy.rs and contract_stdout_purity.rs"
  - "mount_apply_gateway_seeded(server, data, logs, canary): the mock gateway with the canary in the .gwbk bytes and the restart route's 500 body"
  - "README paragraph 'Where secrets can be, and where they cannot'"
affects: [23-16]

actuals:
  tokens: 16300   # chars/4 over the added lines in crates/ and README.md (65,133 characters); about 570 lines of that are the harness moved to tests/common/mod.rs, not new
  tasks: 2
  commits: 2      # two test commits; the docs(23-15) completion commit follows

tech-stack:
  added: []
  patterns:
    - "a canary scan has a named, asserted exemption: the exempt entries must exist, hold the canary (so the skip is real), be 0600 under a 0700 run directory and pass git check-ignore; mutation-checked by emptying CANARY_EXEMPT"
    - "a test harness shared by two integration-test binaries lives in tests/common/mod.rs with #![allow(dead_code)] and is imported by name or glob; the move was done with a script and the 51 existing contract_deploy tests passed unchanged"
    - "an on-box file that exists only mid-run is read from an after-hook on the FaultyHost commit call"

key-files:
  created:
    - crates/ignition-cli/tests/common/mod.rs
  modified:
    - crates/ignition-cli/tests/contract_deploy.rs
    - crates/ignition-cli/tests/contract_stdout_purity.rs
    - crates/ignition-core/tests/deploy_apply.rs
    - README.md

key-decisions:
  - "No source change was needed: the canary found no leak in any scanned channel on the first run of every test, so there was nothing to fix at a source. The tests were checked by mutation instead (below)."
  - "The exemption is one named constant, CANARY_EXEMPT, citing D-05, applied by is_canary_exempt to the first path component (snapshot) and to top-level files by suffix (*.gwbk). Anything else a run writes is scanned."
  - "The apply harness moved to tests/common/mod.rs so the purity file drives the same fixture as the contract file, instead of copying 350 lines of mock gateway."

patterns-established:
  - "a free-text flag value (--allow-drift) is asserted to be recorded in exactly one file (run.json) and absent from both streams and every other run file"

requirements-completed: []
requirements-advanced: [DSURF-03]

duration: ~1h30m
completed: 2026-10-18
---

# Phase 23 Plan 15: The secret canary over apply and rollback, and stdout purity for both write verbs Summary

**A canary seeded in the live resources apply replaces, in the `.gwbk` bytes, in gateway log lines and in an error body reaches no stream and no run-directory file except the two named, 0600, git-ignored artefacts (`snapshot/` and `*.gwbk`), for every apply and rollback outcome, the on-box journal, record and ledger, and an apply killed mid-write.**

## What was built

### The canary and its named exemption (Task 1, tracer)

`CANARY_APPLY = "IGN-CANARY-23-15"` is seeded in four places by `canary_tracer()` and `mount_apply_gateway_seeded`:

| Where | How |
|---|---|
| live mode-collection secret fields | the live `POSTGRES/config.json` holds an `Embedded` ciphertext and the alarm settings a `password`; the apply replaces both, so only the pre-images carry it afterwards |
| the `.gwbk` the gateway serves | a stored ZIP member holds the canary |
| gateway log entries | an unrelated logger's WARN line names it (and, in the rolled-back case, so does the "will not be loaded" line) |
| an optional route's error body | `GET /restart-tasks/pending` answers 500 with `password=<canary>` in the body |

`scan_run_dir_for` walks the run directory and skips exactly `CANARY_EXEMPT = ["snapshot", "*.gwbk"]` (the comment cites D-05). `assert_exempt_private` asserts the exempt entries exist, every exempt file is 0600, the run directory is 0700 and `git check-ignore -q` exits 0 for `snapshot/`, the `.gwbk` and each exempt file. `apply_canary` also asserts the seeds are real (the `.gwbk` and the snapshot pre-image contain the canary, the live file no longer does), that the three seeded routes were served, and that a `--json` apply leaves stderr empty.

### The canary across every outcome (Task 2)

| Test | Outcomes and checks |
|---|---|
| `apply_canary_all_outcomes` | `noop` (no request, no run directory), `rolled_back` (red verify, the red log line carries the canary, exit 6), `confirmation_required`, `plan_stale`, `deploy_preflight_failed` (an open Designer; the run directory exists and is scanned) |
| `rollback_canary` | `deploy_live_changed` refusal; `restored` over a later edit with `--discard-live-changes` (`discarded_live_changes` is paths only); an interrupted-run recovery whose `events.ndjson` has `interrupted_run`; no `.gwbk` in a rollback directory; the apply's directory still holds the exempt files |
| `canary_repeat_runs_clean` (edge idempotency) | apply, rollback, a second apply in human mode; three run directories scan clean, and human-mode stderr (progress) is scanned |
| `apply_canary_drift_reason` | the `--allow-drift` reason (with the canary in it) is in `run.json` and in no other file, not on stdout or stderr, not in events (the 23-21 carry-over) |
| core `canary_on_box_files_clean` | `journal.json` (read in an after-hook on the commit, before finalize), `record.json` and `ledger.json` for an applied run, for the fresh-process rollback of it, and for a rolled-back run; the `DeployApplyResult` and rollback result serialization; the observer events; the run directory |
| core `canary_aborted_run_clean` (edge concurrency) | an apply aborted with `HangAfterMove`; the on-box journal, record, ledger and the partial `events.ndjson` are clean, and the on-box pre-image holds the secret the move replaced |
| purity `deploy_apply_json_stdout_is_the_envelope_only` | applied (one envelope), error (no `--yes`, stdout byte-empty), rolled back (one verdict envelope, exit 6), all under unknown-key warnings and `IGNITION_LOG=trace` |
| purity `deploy_rollback_json_stdout_is_the_envelope_only` | restored, `cleared`, and a `deploy_live_changed` error (stdout byte-empty), same noise |

### README

`### \`deploy apply\``: "Where secrets can be, and where they cannot" states the two exempt artefacts (and why they must be git-ignored and owner-only), lists every other file and stream as secret-free, describes the canary tests, and says an `--allow-drift` reason is recorded once, in `run.json`, as typed.

## Verification

- Tracer verify: `cargo test -p ignition-cli --test contract_deploy apply_canary` passed before the Task 1 commit.
- `cargo test -p ignition-cli --test contract_deploy canary`: 6 passed (the 22-03 `plan_diff_canary_cli` and the five new). `--test contract_stdout_purity`: 13 passed (11 before). `-p ignition-core --test deploy_apply canary_`: 2 passed.
- `cargo test --workspace --no-fail-fast`: 2468 passed, 0 failed, 56 ignored (live and Windows-host gates); 2459 before this plan. The known flake `live_gateway::guard_drop_during_unwind_inside_runtime_still_cleans_up` did not fail.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check`: clean.
- Mutation checks (each reverted): emptying `CANARY_EXEMPT` made `apply_canary` fail naming the `.gwbk` and both snapshot files; adding the on-box pre-image path to `assert_on_box_clean` made both core tests fail naming it. So the scans can fail, and the exempt skip is not vacuous.
- No live step: this plan has none. The trial was not touched; no ssh or Windows host was used. No `cargo install` was needed: no verb changed.

## Deviations from Plan

### Plan departures

1. **[Plan structure] RED did not occur.** The plan expects the tests to pass against the shipped code ("it should pass against the shipped code"); they did, with no leak to fix. Verified by mutation instead of RED, as in 23-14.
2. **[Rule 3 - Blocking] The apply harness moved to `tests/common/mod.rs`.** The plan puts the purity cases in `contract_stdout_purity.rs`, a separate test binary that cannot see `contract_deploy.rs`'s `mount_apply_gateway` and `apply_tracer`. Copying them would have duplicated about 350 lines of mock gateway; `#[path]`-including `contract_deploy.rs` would have run its 50 tests twice. 28 items moved unchanged by script (`TRACER_REPO`, `ign`, `copy_dir`, `stderr_json`, `read_tree`, `rest_config`, `local_tracer`, `plan_json`, `plan_hash`, `changed_tracer`, `apply_tracer`, `ApplyGwState`, `apply_gwbk_zip`, `mount_apply_gateway`, `seen_requests`, `apply_args`, `run_apply`, `verify_ign`, `rollback_args`, `config_tree`, `apply_through_binary`, `plant_journal`, `plant_journal_at`, `edit_live_unit`, and the consts `NAMED_TRANSLATOR`, `APPLY_UNIT`, `APPLY_SYSTEM_NAME`). `contract_deploy.rs` imports them with `use common::*`. All 51 existing contract tests passed unchanged after the move. `tests/common/mod.rs` is not a test target.
3. **[Plan wording] `snapshot/` is 0600 files under a 0700 run directory, not 0700 subdirectories.** The plan says "0600 under a 0700 directory". The directories created inside `snapshot/` by `fetch_snapshot` use `std::fs::create_dir` and keep the umask's mode (0755 here). The 0700 run directory above them is the barrier, so no one else can enter, and every file in them is 0600. The test asserts the run directory 0700 and every exempt file 0600, and says so in a comment. Not changed in the source (see Carry-overs).
4. **[Scope] "Transport stderr" is covered by construction, not by a seeded test.** See "Transport stderr" below.
5. **[Scope] Extra test `apply_canary_drift_reason`,** beyond the plan's list, because the 23-21 carry-over asked for the `--allow-drift` reason to be checked against both streams and the events. Finding: the reason is in `run.json` and nowhere else, as designed; the README says so.
6. **[Plan wording] The noop case seeds its canary in an unmanaged unit.** A noop means the live tree equals the render, so no replaced secret field exists; the only secrets in reach are an unmanaged unit's and the gateway's. A noop sends no request at all (asserted), so the `.gwbk` is never fetched.

No auth gates. No Rule 1 bug found: no leak in any scanned channel. No architectural change (Rule 4 not triggered). No new error slug, so the four-place rule was not triggered.

### Transport stderr

The truth lists "transport stderr" as a seeded source. `scrub_stderr` (22-05) puts a bounded, control-stripped tail of ssh/sftp stderr into `transport_failed` details by design; it filters control characters and the temp directory, not secret values. I did not seed a canary there, because there is no path by which a file body reaches that stream: `commit.sh` redirects every tool's stderr to `/dev/null` (`2>/dev/null` at each mv, ln, rmdir and mkdir) and reports structured verdicts on stdout, `commit.ps1` has no stderr writes, and file bodies travel over the sftp data channel and stdin. So the stream carries tool diagnostics (paths, "Permission denied"), not content. A seeded test would only assert that a synthetic value placed on stderr is echoed, which it is, by design, as the diagnostics tail. If a stricter rule is wanted ("no stderr tail at all in a `--json` envelope"), that is a design change to 22-05, not a leak fix, and is left for the user.

## Tracer feedback gate

Task 1 is the tracer. Auto mode is off (`workflow.auto_advance` false) and the orchestrator directed a sequential unattended run, so execution continued after the tracer commit (`8069672`) instead of stopping for a `checkpoint:human-verify`, as in 23-10 to 23-14 and 23-21. The tracer's `<verify>` passed before the commit and the whole workspace passed after Task 2.

## Known Stubs

None. A grep for `TODO`, `FIXME`, `todo!`, `unimplemented!`, `#[ignore]` and `skip` over the added lines finds nothing.

## Threat Flags

None beyond the plan's register. T-23-62 (secret in any channel): the canary tests above. T-23-63 (the exempted snapshot and `.gwbk`): `assert_exempt_private` plus the README paragraph. T-23-64 (stray stdout): the two purity cases.

Two observations, not new flags: the `snapshot/` subdirectories are 0755 inside the 0700 run directory (defence in depth only), and transport stderr is echoed as a scrubbed tail by design.

## Requirements

`requirements-completed` is empty by instruction. Advanced: DSURF-03 (the secret canary now covers apply and rollback output, run-directory files, events and the on-box journal and record).

## Carry-overs

### 23-16 (live)

- Nothing in this plan needs a live host. For the live gates, the harness in `tests/common/mod.rs` is not usable (it mocks the gateway); `live_deploy_apply.rs` should reuse the existing live-gate patterns from 23-08/23-10.
- A cheap live addition: after `wflwvm01_cli_apply_rollback`, scan the run directories the live run left with the same exemption rule (`snapshot/`, `*.gwbk`) for a value known to be in the live config. Only worth doing if a real secret field can be set up on the rig without printing it.
- The spec section 9 amendment text can cite D-05 and this plan's README paragraph for "where secrets can be".

### Open, for the user

- `snapshot/` subdirectories are created with default permissions (0755) in `fetch_snapshot` (`transport/local/apply_ops.rs` and `transport/sftp/apply_ops.rs`). The run directory is 0700 so nothing is exposed; a one-line `harden_dir` call per `create_dir` would make it uniform. Not done: outside this plan's scope and no leak.
- Transport stderr tails are surfaced in `transport_failed` details (22-05 design). Decide whether that stays.

## Self-Check: PASSED

- Files exist: `crates/ignition-cli/tests/common/mod.rs`, `crates/ignition-cli/tests/contract_deploy.rs`, `crates/ignition-cli/tests/contract_stdout_purity.rs`, `crates/ignition-core/tests/deploy_apply.rs`, `README.md`, this SUMMARY.
- Commits `8069672` and `16b72f4` exist in `git log`.
- `grep -c 'const CANARY_EXEMPT' crates/ignition-cli/tests/contract_deploy.rs` prints 1; `grep -c 'fn apply_canary' crates/ignition-cli/tests/contract_deploy.rs` prints 3 (`apply_canary`, `apply_canary_all_outcomes`, `apply_canary_drift_reason`); `grep -c 'fn canary_' crates/ignition-core/tests/deploy_apply.rs` prints 3 (the two tests and the `canary_rig` helper).
