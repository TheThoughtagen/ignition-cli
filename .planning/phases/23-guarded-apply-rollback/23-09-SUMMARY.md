---
phase: 23-guarded-apply-rollback
plan: 09
subsystem: deploy
tags: [apply-engine, gwbk, restart-pending, run-directory, snapshot, deploy_preflight_failed, rust, tdd]
status: complete

requires:
  - phase: 23-guarded-apply-rollback
    provides: 23-07 run_apply order, run.json written once, events, preflight check ids; 23-05 and 23-17 the single exit point and restore path; 23-03 RunDir and create_private_file; 23-01 C-gwbk and C-restart captures; 23-02 vocabulary (option-a)
provides:
  - the .gwbk step of run_apply, between the preflights and the ledger, staging and lock
  - DeployApi::restart_pending and the DEPLOY_ENDPOINTS row restart_pending (14 rows)
  - restart_pending and restart_pending_new on the apply result and result.json
  - snapshot_sha256 in result.json and a pre-image check of the run-directory snapshot copy
  - the run-directory file matrix pinned by tests
affects: [23-10, 23-11, 23-12, 23-13, 23-15, 23-16, 23-21]

actuals:
  tokens: 15800
  tasks: 3
  commits: 6

tech-stack:
  added: []
  patterns:
    - "an unanswerable or failed .gwbk is a preflight finding with the check id gwbk, raised after run.json and plan.json are written and before anything is staged, so the one exit point writes result.json"
    - "a pending-restart read that fails is a warning once per run, never an error; an unreadable 'before' is treated as empty so nothing is hidden"
    - "a source-level guard walks deploy/ for the restart route text, comments stripped"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/deploy/apply/mod.rs
    - crates/ignition-core/src/actions/deploy_apply.rs
    - crates/ignition-core/src/client/deploy_api.rs
    - crates/ignition-core/src/client/restart.rs
    - crates/ignition-core/src/deploy/apply/hold.rs
    - crates/ignition-core/tests/deploy_apply.rs
    - crates/ignition-core/tests/deploy_client_contract.rs
    - crates/ignition-core/tests/restart_wait_contract.rs
    - crates/ignition-core/tests/common/fake_deploy.rs
    - crates/ignition-core/tests/common/forbid_writes.rs

key-decisions:
  - "The .gwbk completeness check is the ZIP central directory (opens, at least one entry, non-empty file). Content-Length is not compared by the engine: C-gwbk shows the gateway sends none, and where a length is sent hyper already fails a short body, which arrives as a failed download and is the same gwbk refusal"
  - "A refused .gwbk is deleted from the run directory (it is useless and holds secrets); the other four run-directory files stay"
  - "restart_pending_new with an unreadable 'before' reports everything now pending as new, plus the warning, rather than hiding entries (over-report, not under-report)"
  - "restart_pending is read before staging (after the .gwbk) and after finalize on the applied path, and after the restore on the rolled-back path; restore_snapshot itself does not read it, so 23-11's manual rollback can choose"

patterns-established:
  - "FakeGateway::set_restart_pending(before, after_scan) switches the list at the first config scan trigger"
  - "snapshot_warnings(record, copied) is the one comparison used by apply and by restore_snapshot"

requirements-completed: []
requirements-advanced: [DAPL-05, DAPL-10]

duration: about 2h (most of it cargo builds)
completed: 2026-10-16
---

# Phase 23 Plan 09: The .gwbk, restart-pending and the run directory Summary

**Apply now downloads a private, ZIP-validated `.gwbk` into its run directory before it stages or locks anything (or records `--skip-gwbk`), reads `restart-tasks/pending` before and after the run without ever restarting, and proves its run-directory snapshot copy equals the on-box pre-images.**

## What changed

- **`.gwbk` (Task 1, tracer).** `take_gwbk` in `deploy/apply/mod.rs`: unless `ApplyOptions.skip_gwbk`, create `<run id>.gwbk` exclusively at 0600 with the existing `RunDir::create_private_file` (it already returned a handle and does not need a second helper), download through `backup_download(.., Some(path), .., BackupType::All)`, then require a non-empty file that opens as a ZIP with at least one entry. Any failure removes the file and is `deploy_preflight_failed` with check `gwbk` (exit 6, value-free detail: "is empty", "is not a readable ZIP (no central directory), so the download is incomplete", "was not downloaded (<code>)"). Events `gwbk_downloaded {bytes}` and `gwbk_skipped`. `run.json` and `result.json` carry `skip_gwbk`; `result.json` and the result carry `gwbk` (the file name). Event order: `run_started`, `gwbk_downloaded`, `staged`, `lock_acquired`, ... so the download never spends hold budget.
- **Endpoint (Task 2).** `DEPLOY_ENDPOINTS` row `restart_pending` (`GET /data/api/v1/restart-tasks/pending`, shape Json, capture `C-restart`), `DeployApi::restart_pending() -> Vec<String>` through `send_shaped`, a body without `pending` or with `null` is an `internal` shape error, 401/403 are `Auth`. The pinned id list and count moved from 13 to 14 deliberately. The `restart.rs` module doc now says the endpoint is read by `DeployApi::restart_pending` only and the restart route stays out of the deploy path. `FakeGateway::set_restart_pending(before, after_scan)` and `deny_restart_pending()`; `ForbidWrites` and the `hold.rs` test gateway implement the read.
- **Reporting and run directory (Task 3).** `restart_pending` (list after the run) and `restart_pending_new` (entries absent before) are `Vec<String>` and serialize as `[]`, never null, in the result and in `result.json`. An unreadable list (403, shape, network) is the single warning `restart_pending_unavailable`, outcome unchanged. `fetch_snapshot` hashes are compared with the record's pre-images; a mismatch or missing copy is the warning `snapshot_copy_differs (<path>)`; `result.json` lists `snapshot_sha256` (keys `files/<path>`, `ledger.json`, `record.json`). The restore path (`restore_snapshot`) does the same comparison and returns the hashes in `RestoreReport`.

## Tests

| Behaviour | Test |
|---|---|
| `.gwbk` private (0600), `type=all`, one request, before the lock | `snapshot_gwbk_downloaded_private` |
| half a ZIP refused, nothing staged or locked, file removed | `snapshot_gwbk_truncated_is_refused` |
| zero-byte (edge DAPL-05 empty) | `snapshot_gwbk_empty_is_refused` |
| download 500 | `snapshot_gwbk_download_failure_is_refused` |
| skip recorded, no backup request | `snapshot_skip_gwbk_is_recorded` |
| row parses `{"pending":[...]}`, `[]`, 401/403, bad shape | `restart_pending_parses_the_captured_shape`, `restart_pending_403_is_auth_with_the_status` |
| 14 rows, wire matches the table | `deploy_endpoints_cover_every_method`, `endpoint_rows_match_the_capture_table`, `wire_matches_the_endpoint_table` |
| fields, new vs old, empty is `[]` (edge DAPL-10 empty) | `run_dir_restart_pending_reported` |
| 403 is a warning | `run_dir_restart_pending_unavailable_warns` |
| no restart request across an applied and a rolled-back run, and no route text under `src/deploy/` | `run_dir_never_restarts`, `apply_source_never_names_the_restart_route` |
| snapshot hashes equal the record, created file has no copy (edge DAPL-05 adjacency) | `snapshot_run_dir_hashes_equal_record` |
| create-only run: empty `snapshot/files/`, complete record (edge DAPL-05 empty) | `snapshot_create_only_run_has_an_empty_files_dir` |
| two runs, two box snapshots, two run dirs (edge DAPL-05 idempotency) | `snapshot_two_runs_two_dirs` |
| four files and 0700/0600 after success, preflight failure, restore, rollback_failed; no `.ign` after a flag error, `plan_stale`, destructive refusal, no-op, `run_dir_not_ignored` | `run_dir_matrix` |
| copy-differs warning logic | `snapshot_warnings_name_a_differing_or_missing_copy_and_skip_created_paths` (unit) |

## Deviations from Plan

**1. [Plan departure] No new `RunDir::create_private_file`.** The plan asks for `create_private_file(name) -> Result<PathBuf>` in `run_store.rs`. 23-03 already shipped `create_private_file(name) -> Result<File>` (create-new, 0600, used by `write_json` and `EventLog`). The engine drops the handle and passes the path, so `run_store.rs` is unchanged.

**2. [Plan departure] Content-Length is not compared.** The plan says to compare the byte count with Content-Length when the result reports one. `ExportMeta` does not report one, C-gwbk shows the gateway sends none, and a body shorter than a sent length already fails inside the HTTP stack as a download error, which is the same `gwbk` refusal. The ZIP central directory is the completeness check, as `### C-gwbk` decided. A CRC walk was not added (backups can be large; the plan marks it optional).

**3. [Rule 3 - Blocking] `deploy_client_contract.rs` and `restart_wait_contract.rs` touched.** `wire_matches_the_endpoint_table` requires every declared row to be exercised, so it gained a `restart_pending` row. `restart_wait_contract.rs` had a doc comment saying the trait has no pending method; it now says the `GatewayApi` trait. Neither file is in the plan's `files_modified`.

**4. [Rule 3 - Blocking] Existing 23-05/23-17 assertions shifted.** The rig defaults now mount a backup route at priority 250 (like the other preflight answers), so every run includes the `.gwbk`: the exact event lists (`apply_happy_path_local`, the restore and verify-red lists, `LAPSED_EVENTS`), one event index in `fault_write_failed_mid_commit_restores`, and the run-directory file list gained `gwbk_downloaded` and `<run id>.gwbk`. No behaviour of those tests changed.

**5. [Test shape] RED commits do not compile.** The tracer RED needs `ApplyOptions.skip_gwbk` and the Task 2 and 3 REDs need `restart_pending` and the result fields, so each RED commit fails to build, the same way the 23-02 tracer RED did. Each GREEN commit makes its tests pass.

**6. [Scope] `deploy_blocked` and the missing-confirmation refusals are not in `run_dir_matrix`.** The matrix covers the gate refusals that `deploy_apply_prepare` raises before `run_apply` (flag error, `plan_stale`, destructive) plus no-op and `run_dir_not_ignored`. `deploy_blocked` is raised by the same function before any run directory by construction (and has gate unit tests in 23-02); the missing `--yes` confirmation is the CLI's check (23-13), which owns that no-directory test.

No auth gates.

## Plan edge coverage

| Edge | Where |
|---|---|
| DAPL-05 adjacency | `snapshot_run_dir_hashes_equal_record` |
| DAPL-05 empty | `snapshot_gwbk_empty_is_refused`, `snapshot_create_only_run_has_an_empty_files_dir` |
| DAPL-05 idempotency | `snapshot_two_runs_two_dirs` |
| DAPL-10 empty | `run_dir_restart_pending_reported` (both empty gives `[]` and `[]`), the no-dir refusals of `run_dir_matrix` |

## Tracer feedback gate

Auto mode is not active and `human_verify_mode=end-of-phase` applies, so no mid-plan human-verify stop. The tracer's `<verify>` (`cargo test -p ignition-core --test deploy_apply -- snapshot_gwbk snapshot_skip_gwbk`) passed before the tracer GREEN commit `5fff00b` and the whole `deploy_apply` suite passed again after Task 3 (55 passed).

## Human-verify checkpoints

None in this plan. Owed to the end-of-phase pass: a real gateway `.gwbk` (the plan's captures used the local rig; the engine path is exercised against wiremock only) and C2b (a Designer save under a held lock) remain with 23-16 and the user.

## Verification

- `cargo test -p ignition-core --test deploy_apply`: 55 passed
- `cargo test -p ignition-core --lib client::deploy_api`: 6 passed (14-row pin included)
- `cargo test -p ignition-core --test deploy_client_contract`: 64 passed
- `cargo test --workspace --no-fail-fast`: 2390 passed, 0 failed, 51 ignored (live and Windows-host gates) across 86 binaries; the `contract_rig.rs` tests ran green (Docker daemon up)
- `cargo clippy --workspace --all-targets -- -D warnings`: clean
- `cargo fmt --all -- --check`: clean
- `grep -rn 'RESTART_PATH\|restart-tasks/restart' crates/ignition-core/src/deploy/apply` prints nothing

## Known Stubs

None. `grep` for `TODO`, `FIXME`, `todo!`, `unimplemented!`, `#[ignore]` over the changed engine and test files finds nothing.

## Threat Flags

None beyond the plan's register. T-23-38: the `.gwbk` is created exclusively at 0600 inside the 0700 run directory (`snapshot_gwbk_downloaded_private`, `assert_private` in `run_dir_matrix`), never uploaded (it is written only to the run directory path), and a refused copy is deleted. T-23-39: only the read of `restart-tasks/pending` exists in the deploy client; the zero-call wiremock expectation and the source walk pin it. T-23-40: `run_dir_matrix`. T-23-41: the ZIP central directory check refuses before any write.

## Next Phase Readiness

- **23-13 (verb):** add `--skip-gwbk` to `ApplyOptions.skip_gwbk`; render `gwbk`, `skip_gwbk`, `restart_pending` and `restart_pending_new` in the human output; the README `### deploy apply` section should name the `gwbk` check id, `type=all`, the 0600 file, `restart_pending_unavailable` and `snapshot_copy_differs`. The stderr observer needs arms for the two new events (`gwbk_downloaded`, `gwbk_skipped`).
- **23-11 / 23-12 (rollback):** `restore_snapshot` already returns `snapshot_sha256` and the copy warnings; it does not read the pending list, so manual rollback can call the private `restart_report` equivalent (or the engine can expose it) if the rollback result should carry the fields.
- **23-15 (canary):** the `.gwbk` is a whole-gateway secret archive in `.ign/deploy/<gateway>/<run id>/`; the canary exemption must stay permission-pinned (0600) as T-23-38 says.
- **FA-23-06** (`BackupType::All`) is still the flagged assumption for user review.

## Self-Check: PASSED

- Modified files exist: `crates/ignition-core/src/deploy/apply/mod.rs`, `crates/ignition-core/src/actions/deploy_apply.rs`, `crates/ignition-core/src/client/deploy_api.rs`, `crates/ignition-core/tests/deploy_apply.rs`, `crates/ignition-core/tests/common/fake_deploy.rs`, this SUMMARY.
- Commits `9d13040`, `5fff00b`, `606f3ff`, `8f33158`, `f36c9e8`, `bf5f93d` exist in `git log`.
- `grep -n 'backup_download' crates/ignition-core/src/deploy/apply/mod.rs` prints the import and the call.
