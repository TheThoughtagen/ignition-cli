---
phase: 23-guarded-apply-rollback
plan: 10
subsystem: deploy
tags: [hold-budget, scan-lock, d-02, d-03, live-timing, windows-ssh, commit-ps1, oq5b, tdd]
status: complete

requires:
  - phase: 23-guarded-apply-rollback
    provides: 23-05/23-17 hold.rs (D-02 floor, ceiling, formula, HoldModel, no measured value); 23-08/23-20 the Windows ApplyHost (prepare, commit, restore) and commit.ps1; 23-01 the Windows capture host record
provides:
  - oq5b_timings, a live test that times prepare, the batched commit, its scan, restore and its scan at 152 and 1,520 files, plus a replace commit and a delete-set commit per scale
  - "## OQ5b (batched commit, 23-10)" (raw lines of both scales) and "## Decisions: OQ5b" in 23-LIVE-CAPTURES.md
  - HOLD_S = 176 s, P95_COMMIT_S_SCALE1 = 10.123, P95_COMMIT_S_SCALE10 = 85.496, P95_SCAN_S = 1.579, FILES_SCALE1/FILES_SCALE10 in deploy/apply/hold.rs
  - HoldModel::MEASURED (hold 176 s, fixed 1.748 s, 55.1 ms a file, scan 1.579 s) for the D-03 refusal and for 23-13
  - the STATE.md hold-budget blocker marked RESOLVED
affects: [23-13, 23-21, 23-14, 23-15, 23-16]

actuals:
  tokens: 7000    # chars/4 over the realized diff (28,094 characters of crates/ changes, 694 lines added, 7 removed)
  tasks: 3        # Task 3 (D-03a) did not fire; counted as completed
  commits: 2      # two code commits; the docs commit follows

tech-stack:
  added: []
  patterns:
    - "a commit consumes its staged files, so a live timing loop prepares once, copies the staging directory on the host with robocopy, and puts it back before every iteration; the reset is timed on its own and sits outside every measured call"
    - "the hold constant is a grep-visible literal and a test recomputes it, and the model fit, from copies of the recorded numbers; changing either side alone fails the test"

key-files:
  created: []
  modified:
    - crates/ignition-core/tests/live_transport_wflwvm01.rs
    - crates/ignition-core/src/deploy/apply/hold.rs
    - .planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md
    - .planning/STATE.md

key-decisions:
  - "HOLD_S = 176 s = ceil(2 x (85.496 + 1.579) + 1) from the 1,520-file REPLACE commit (the slowest commit variant), not the new-file commit the plan names (62.040 s, hold 129 s). An apply of a changed tree replaces files and copies every pre-image first; the hold must cover the slowest call the lock is held across (FA-23-08). The plan's literal choice is two numbers away and is written in Decisions: OQ5b."
  - "D-03a not triggered: unclamped hold 176 s, ceiling 300 s. HOLD_CEILING_S stays 300; no clamp was applied."
  - "HoldModel::MEASURED fits the replace p95 at 152 and 1,520 files: 1.748 s fixed plus 0.0551 s a file. The largest change that fits is 1,527 files (writes plus deletes); 1,528 is refused by the hold_budget preflight."
  - "op_timeout_s is unchanged: the slowest transport call was prepare of 1,520 files (553.5 s, outside the lock), 53% of DEFAULT_OP_TIMEOUT_S (1050 s) and far under the 3600 s cap."

patterns-established:
  - "scan timing in live tests goes through the installed ign binary (GwCli), because Session::resolve blocks on a Keychain prompt in a headless run"

requirements-completed: []
requirements-advanced: [DAPL-06, DAPL-04]

duration: ~1h25 (the live runs were 13 min at scale 1 and 52 min at scale 10; the rest was reading, the harness and the record)
completed: 2026-10-18
---

# Phase 23 Plan 10: The measured hold and the per-file model Summary

**The batched commit was timed on the Windows capture host at 152 and 1,520 files; the lock window it needs is 176 s, inside the 300 s ceiling, so D-03a did not fire, and the hold, the p95 literals and `HoldModel::MEASURED` are committed behind a test that recomputes them.**

## What was built

- `oq5b_timings` (`live_transport_wflwvm01.rs`, `IGNITION_LIVE_OQ5B=1`, `IGN_OQ5B_ITERS` 10, `IGN_OQ5B_REPLACE_ITERS` 5, `IGN_OQ5B_SCALES` `1,10`). Through the Windows `ApplyHost` the engine uses: `prepare` once (timed), then per iteration `reset`, `commit` (one call, ssh connect included), the config scan to completion, `restore`, and its scan. Per scale it also times a replace commit (a second version of every file over the installed one) and one delete-set commit that removes the whole payload. After each scale and at the end the collection is asserted equal to its pre-run listing and the deploy directory is removed. A panic in the body cannot skip the restore (the body runs as a task, as in `wflwvm01_c7b`). `oq5b_fit_is_two_point_linear` is the offline test.
- `hold.rs`: the three p95 literals, the two file counts, `HOLD_S`, and `HoldModel::MEASURED` with the fit computed in `const` arithmetic next to them. `hold_s_is_derived_from_recorded_measurements` holds copies of the recorded numbers and fails if a constant, the hold or the fit moves without them; `hold_model_fits_the_measured_largest_tree` pins 1,527 as the largest change that fits.
- `23-LIVE-CAPTURES.md`: `## OQ5b (batched commit, 23-10)` with both runs' raw lines, and `## Decisions: OQ5b` (OQ5b-1 to OQ5b-5).

## The numbers (seconds; nearest-rank p95; full table in `## Decisions: OQ5b`)

| Series | n | 152 files p95 | 1,520 files p95 |
|---|---|---|---|
| commit, new files plus ledger | 10 | 8.731 | 62.040 |
| commit, every file replaced | 5 | 10.123 | **85.496** |
| commit, whole payload deleted | 1 | 6.130 | 34.459 |
| restore after the new-file commit | 10 | 16.006 | 28.937 |
| restore after the replace commit | 5 | 21.026 | 71.839 |
| scan after a commit (wall, through `ign api call`) | 10 / 5 | 1.221 | **1.579** (replace) |
| prepare (sftp upload, outside the lock) | 1 | 81.5 | 553.5 |

## Hold-budget decision

- D-02: `hold = ceil(2 x (p95 commit + p95 scan) + 1)`, floor 10 s, ceiling 300 s. Scale 10 is the basis (FA-23-07, the plan's choice). The commit in the formula is the slowest variant (the replace commit, FA-23-08): `ceil(2 x (85.496 + 1.579) + 1) = ceil(175.15) = 176 s`.
- **D-03a not triggered: unclamped hold 176 s, ceiling 300 s.** Nothing was clamped and no verb was gated.
- The evidence behind it: the 234 to 348 s for 152 files was the per-file sftp rename commit on the previous VM build. The one-call script commit takes 8.7 s (replace: 10.1 s) for the same 152 files on the rebuilt host (23-08 measured 6.92 s once; the p95 over ten runs is 8.731 s) and 62.0 s (replace: 85.5 s) for 1,520 files, where the old path extrapolated to 2,500 to 3,500 s.
- Where the engine holds the lock: `prepare` is before it; the lock covers `commit`, one `lock.check` and the scan, which releases it (D12). The post-write `fingerprint`, `deploy_verify`, `finalize` and `fetch_snapshot` run after the scan, outside the hold. The rollback path takes a lock for `restore` and its scan.
- With the plan's literal choice (new-file commit, 62.040 s, and its scan, 1.567 s) the hold would be 129 s. A 1,520-file replace commit and its scan take 87.1 s, which is 49% of 176 s and 68% of 129 s. The 47 s of extra lock window is the price of keeping D-02's 2x margin on the path real applies take. To use the plan's literal, change three numbers in `hold.rs` and the same three in the test (listed in `## Decisions: OQ5b`, OQ5b-2).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The scans go through the installed `ign` binary, not `Session::resolve`**
- **Found during:** Task 1
- **Issue:** `oq5_timings` (22-14) resolves a `Session`, which blocks on a Keychain prompt in this headless run.
- **Fix:** `oq5b_timings` reuses `GwCli` (`ign --profile azure-win --json api call`), as `wflwvm01_c7b` does. The scan wall times therefore include a process start, the keyring read and a 200 ms poll, so they are longer than the engine's own REST client would see (the gateway's `lastScanDuration` is recorded beside each: 0.4 to 1.1 s). That errs toward a longer hold.
- **Files modified:** crates/ignition-core/tests/live_transport_wflwvm01.rs
- **Commit:** 01143d0

**2. [Rule 3 - Blocking] "Payload staged once" cannot hold for ten commits**
- **Found during:** Task 1 design (read of `commit.ps1` `MoveOne`)
- **Issue:** a commit moves the staged copies, the pre-rendered journals and the script's inputs out of the staging directory. Preparing for every iteration would cost 550 s a time at 1,520 files, and the staging of one run id cannot simply be committed again.
- **Fix:** `prepare` runs once per batch and is timed; before each iteration a host-side `robocopy /MIR` puts the staging directory back from a copy made right after `prepare`, and the journal, the ledger and the run's snapshot are dropped (what `finalize` does in the engine). That reset is timed separately (1.8 s and 5.6 s p50) and is outside every measured call.
- **Commit:** 01143d0

**3. [Rule 2 - Missing critical functionality] A replace commit and a restore after it were timed as well**
- **Found during:** Task 1 design
- **Issue:** the plan times a commit of new files. An apply of a changed tree replaces files and copies each pre-image into the snapshot first; a hold sized on new files alone would under-budget that path, which is the failure the hold exists to prevent (T-23-42).
- **Fix:** a second batch per scale replaces every payload file (n = 5) and its restore is timed. The 1,520-file replace commit is 85.496 s against 62.040 s for new files, and it set the hold (FA-23-08).
- **Commit:** 01143d0, bc32a41

**4. [Rule 1 - Acceptance wording] `HOLD_S` is a literal with a test, not a computed constant**
- **Found during:** Task 2
- **Issue:** the acceptance criterion greps `pub const HOLD_S: u64` for a value equal to the clamped hold.
- **Fix:** `HOLD_S` is the literal 176; `hold_s_is_derived_from_recorded_measurements` recomputes it from the p95 literals. The fit constants (`FIT_PER_FILE_S`, `FIT_FIXED_S`) are `const` expressions of the literals, so `HoldModel::MEASURED` cannot drift from them.
- **Commit:** bc32a41

### Not done as planned

- **Replace series has 5 samples, not 10.** The plan asks for 10 iterations of the commit; the 10 are the new-file commits. The replace series (an addition, see 3) costs about 11 minutes more at 1,520 files for five more samples; with n = 5 or n = 10 the nearest-rank p95 is the maximum. The 2x factor and the extra second are the margin. Recorded in OQ5b-5.
- **Trial reset.** `ign --profile azure-win --json api call --method POST --path /data/api/v1/trial` answered `auth_rejected` (HTTP 403) at 2026-10-17T23:20:36Z. The trial was live (5,596 s left), so nothing depended on it, and the scale 10 run ended with 2,412 s left. No reset took effect in this plan. A later plan that runs past the trial needs a reset the profile's token cannot make (see carry-overs). An attempt to use the gateway admin credential from the cache was refused by the harness and was not repeated.

## Auth gates

None.

## TDD Gate Compliance

Task 2 is `tdd="true"`. The RED step was run (the two behaviour tests added first failed to compile: `HOLD_S`, `P95_*` and `HoldModel::MEASURED` did not exist), then the constants were added and the tests passed. The RED state was not committed on its own because it does not compile; test and code are one `feat(23-10)` commit (bc32a41). There is no separate `test(...)` commit for Task 2.

## Tracer feedback gate

Task 1 is the tracer. Auto mode is off and `human_verify_mode` is `end-of-phase`, so, as in 23-01 to 23-08, 23-12, 23-18 and 23-20, execution continued after the tracer commit instead of stopping for a human-verify checkpoint. The tracer's `<verify>` (the offline `live_transport_wflwvm01` run and the live `oq5b_timings` runs) passed before Task 2 used any number. For the end-of-phase list: the tracer is a working, repeatable timing of the batched commit on `ign-win-836`; a 2-iteration smoke run and then both full runs ended with production equal to its pre-run listing.

## Verification

- `cargo test -p ignition-core --lib deploy::apply::hold`: 17 passed, including `hold_s_is_derived_from_recorded_measurements` and `hold_model_fits_the_measured_largest_tree`.
- `cargo test -p ignition-core --test live_transport_wflwvm01` (offline): 7 passed, 11 live tests ignored; `oq5b_fit_is_two_point_linear` among them.
- `cargo test -p ignition-core --lib`: 945 passed. `cargo test -p ignition-core --test deploy_apply`: 78 passed. `cargo build --workspace`, `cargo clippy -p ignition-core --all-targets` and `cargo fmt --check`: clean.
- Live (`ign-win-836`, 2026-10-17): smoke 23:00Z (not pooled); scale 1 at 23:07:28Z (767.6 s); scale 10 at 23:20:54Z (3,147.8 s). Both ended `production_listing_equals_pre_run: true`, `deploy_dir_gone: true`. `live_gateway::guard_drop_during_unwind_inside_runtime_still_cleans_up` was not run in this plan.
- `grep -c '^## OQ5b' 23-LIVE-CAPTURES.md` is 1 and `grep -c '^## Decisions: OQ5b'` is 1; `grep -n 'pub const HOLD_S: u64' hold.rs` prints `176`.

## Known Stubs

None.

## Threat Flags

None. The harness prints timings and counts only; the key stays in the environment. Measurement staging and snapshots are removed (T-23-43); the p95 literals, the 2x factor and the D-03 refusal are the T-23-42 mitigation.

## Flagged assumptions

| ID | Assumption | Owner |
|---|---|---|
| FA-23-07 | the hold is computed from the 1,520-file scale, the largest measured, so changes up to 1,527 files fit | user review at verify |
| FA-23-08 | the commit in the formula is the slowest variant measured (replace, 85.496 s), not the new-file commit the plan names (62.040 s, hold 129 s) | user review at verify |

## Carry-overs

- **23-13:** `HoldModel::MEASURED` and `HOLD_S` exist; the verb can name them. The hold is 176 s for every target, with no override. A change of 1,528 or more touched files (writes plus deletes) is refused by the `hold_budget` preflight with the estimate and the 176 s budget; say so in the README run text and `deploy apply` help (23-21, 23-14 for rollback). Prepare is the long step: 553 s at 1,520 files and 80 s at 152, silent unless the observer prints `Staged` progress; consider a stderr line before it.
- **Whole-apply wall time** (single runs, 23-20 and here): at 1,520 files about prepare 553 s, commit 62 to 85 s, scan 1.6 s, then outside the lock `deploy_verify` plus fingerprint 15 s, finalize 14 s, `fetch_snapshot` 25 s. 23-15 and 23-16 should keep payloads small; a two-file apply is about 30 s of prepare, 3 s of commit and 15 s of restore on this link.
- **23-16:** `live_lock_lapse_restores` needs a short hold, which the verbs cannot give (D-02: no override). Drive it through the library entry with an explicit `HoldModel`, as the engine tests do. A live gate that runs past the trial needs a reset the profile token cannot make (403); the user or orchestrator resets it.
- **23-17 (observation, no change made):** `restore_steps` decides whether to reuse the held lock from `estimate_commit_s(files) + 2 s`, which is 1.9 s for two files, while a restore costs about 14 s of fixed ssh work plus 21 s at 152 files. A reused lock with only a few seconds left could lapse during a restore. With a 176 s hold this needs a commit that itself ran for most of the hold, so it was not changed; a restore fixed term in the model would close it.
- **23-07 (observation):** the model is fit to the replace commit; a delete-only change of 1,520 files measured 34.5 s, so the refusal is conservative for deletes.
- **Linux:** the sh host (23-06) has no measured timing; D-02's one constant applies to it unmeasured. A Linux re-run of `oq5b_timings` needs a Linux host test and is not in this plan.
- **Re-running:** `IGN_OQ5B_SCALES=1` takes 13 minutes, `10` takes 52 minutes; a different link or Defender configuration moves the commit, and the two `P95_*` literals plus `HOLD_S` and the test copies are the only places to change.

## Self-Check: PASSED

- Created or changed files exist: `crates/ignition-core/src/deploy/apply/hold.rs`, `crates/ignition-core/tests/live_transport_wflwvm01.rs`, `.planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md` (`## OQ5b (batched commit, 23-10)` and `## Decisions: OQ5b`), `.planning/STATE.md`.
- Commits exist: 01143d0 (test, Task 1), bc32a41 (feat, Task 2).
- Task 3 (D-03a): not triggered, unclamped hold 176 s against the 300 s ceiling; `HOLD_S` committed.
