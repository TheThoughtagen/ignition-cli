---
phase: 23-guarded-apply-rollback
plan: 16
subsystem: deploy
tags: [live-gate, apply, rollback, scan-lock, windows-ssh, spec-amendment, rust]
status: complete

requires:
  - phase: 23-guarded-apply-rollback
    provides: 23-05..23-21 the engine, hosts, verbs and README; 23-10 HOLD_S and HoldModel::MEASURED; 23-20 the Windows ApplyHost arms and the factory arm; 23-15 the canary and the README secrets paragraph
provides:
  - "live_deploy_apply.rs: live_apply_then_rollback, live_kill_mid_write_then_recover, live_kill_after_green_verify_then_recover (A9), live_lock_lapse_restores, all green on a fresh 8.3.6 rig"
  - "wflwvm01_cli_apply_rollback: the installed ign applies and rolls back on the real Windows host; green"
  - "ACQUIRE_WAIT_S = 1 (hold.rs): the scan-lock acquire no longer sends acquireTimeout 0, which a real gateway refuses"
  - "23-LIVE-CAPTURES.md: three '## Live gate:' sections and '## Spec §9 amendment draft (23-16)'"
  - "whk-environment-orchestration PR #63 (stacked on #60): the section 9 amendment"
affects: []

actuals:
  tokens: 15300   # chars/4 over the added lines in crates/ and README.md (61,333 characters); 20,000 more characters are the .planning capture text
  tasks: 3
  commits: 5      # fix, three test, docs; the docs(23-16) completion commit follows

tech-stack:
  added: []
  patterns:
    - "a live test of the write flow takes a process-wide async lock and a Clean guard that removes what it wrote at the start and on drop; the library entries are driven with an explicit short HoldModel where the verbs have one fixed hold"
    - "a fake gateway reproduces every refusal a live gate has found (here: a non-positive lock timeout), so the offline suite catches the regression"
    - "a Windows live test of a verb runs the installed binary as a child process (stdin null, kill_on_drop, a time limit), reads the JSON envelope, and measures the lock window from the run's events.ndjson"

key-files:
  created:
    - crates/ignition-core/tests/live_deploy_apply.rs
  modified:
    - crates/ignition-core/src/deploy/apply/hold.rs
    - crates/ignition-core/tests/common/fake_deploy.rs
    - crates/ignition-core/tests/deploy_apply.rs
    - crates/ignition-core/tests/live_transport_wflwvm01.rs
    - README.md
    - .planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md

key-decisions:
  - "The engine asks the gateway for a lock wait of 1 s (ACQUIRE_WAIT_S), not 0: the real gateway answers 400 'Timeouts must be positive.' to 0 and never waits out a positive timeout (a held lock is an immediate 409, D1), so 1 is 'no wait'."
  - "Spec section 9 (Task 3 decision): user chose a PR for that repo, stacked on #60. Landed as WhiskeyHouse/whk-environment-orchestration PR #63."
  - "A rollback leaves an empty <module>/<type> directory when it removes the first resource of a type: the host ops never remove <module>/<type> or above, by design. The live tests compare files plus directories below <module>/<type>, and the README now says so."

patterns-established:
  - "recover_after_kill: try the recovery at once, print what the gateway does, and on scan_lock_held wait for the lock to read free and retry"

requirements-completed: []
requirements-advanced: [DAPL-05, DAPL-06, DAPL-08, DAPL-09]

duration: ~6h elapsed across two sessions (the rig bring-up, a Keychain halt of about 9 hours, and the 395 s Windows run dominate)
completed: 2026-10-18
---

# Phase 23 Plan 16: Live gates for apply, rollback, kill and lapse, and the section 9 spec PR Summary

**The whole write flow ran green on a fresh 8.3.6 rig (apply, rollback by run id from a fresh context, kill mid-write, kill past a green verify, lapsed lock) and through the installed `ign` on the real Windows host, the first live run found and fixed a lock-acquire bug that would have failed every real apply, and the spec section 9 amendment is a stacked PR (#63).**

## What was built

### The bug the live gate found (Rule 1)

The first live apply stopped at the scan-lock acquire: `400 Bad Request ... Error 400: Timeouts must be positive.` `HeldLock::acquire` sent `acquireTimeout: 0`, which every offline gateway accepted. The real gateway refuses a non-positive timeout and never waits out a positive one (20-LIVE-CAPTURES D1: a held lock is an immediate 409), so the engine now sends `ACQUIRE_WAIT_S = 1`. The fake gateway refuses a non-positive timeout as the real one does. Mutation check: with the constant at 0, `apply_happy_path_local` fails with that message. Before `84dca10`, every `ign deploy apply` and `ign deploy rollback` would have failed at the lock on a real gateway. No new error slug (the four-place rule was not triggered).

### The rig gate (`live_deploy_apply.rs`)

`IGNITION_LIVE_DEPLOY=1`, loopback guard, gateway-name guard (`ign-deploy-rig`), a `local:` node on the bind mount, the tracer's two units, a real `ReqwestGatewayApi`.

| Test | Result on the rig (2026-10-18) |
|---|---|
| `live_apply_then_rollback` | `deploy_apply_run` (measured hold): `applied`, 4 files, `.gwbk` 3.67 MB, events through `verify_done` ok and `finalized`. After dropping every handle and `.ign/`, `deploy_rollback_run` by run id: `restored`, a completed restore scan, tree and REST view as before, no "will not be loaded" line since the restore scan, plan equal to the first plan. 13.3 s |
| `live_kill_mid_write_then_recover` | task aborted after the first move: journal at `writing`; plan, apply and a rollback of another run refuse `deploy_journal_present`; **recovery at once failed `scan_lock_held`** (the dead apply's lock), the lock read free 30.6 s after the kill (the 30 s hold), recovery then restored everything. 35.6 s |
| `live_kill_after_green_verify_then_recover` (A9) | aborted at `finalize`: journal at `written`, the gateway serves the unit; recovery at once (0.6 s) restores the pre-apply tree (the scan already released the lock) |
| `live_lock_lapse_restores` | hold 10 s, sleep past it after `commit`: C2e as predicted (scan timestamp unchanged across the lapse, lock free, the unscanned file not loaded); then `rolled_back`, cause `lock_expired`, exit 6 verdict; REST view, log rule and plan all back |

### The Windows gate (`wflwvm01_cli_apply_rollback`)

The installed `ign` (built at the `84dca10` state), against `ign-win-836` (Windows Server 2022, real `production` collection): `deploy plan`, `deploy apply --yes --plan-sha256`, `deploy rollback --yes --run`. Apply `applied` (166.0 s wall, 4 files, `.gwbk` 3,710,688 bytes, `snapshot/record.json`, nine events); **lock window 3.808 s against `HOLD_S` 176 s**. Rollback `restored` (125.7 s wall, 4 deleted), lock window 13.232 s. Plan after equals the first plan; production equals its pre-run listing (checked over ssh and again by hand: `config-mode.json` only, no `ign-deploy` directory). The trial had expired (`trialSecondsLeft` 0); **reset at 2026-10-18T13:55:14Z** (HTTP 200, 7199 s left). Recorded in `## Live gate: Windows binary apply and rollback (23-16)`.

### The spec section 9 decision

**User chose: a PR for that repo (stacked on #60).** In a fresh clone (the user's local clone and PR #60's branch were not touched), branch `docs/spec9-deploy-account-scope` from `docs/prod-gateway-config-as-code`, one commit (`675e583`, no co-author line) replacing the §9 SSH-account bullet with the drafted text, pushed alone, and opened as **<https://github.com/WhiskeyHouse/whk-environment-orchestration/pull/63>** (base `docs/prod-gateway-config-as-code`). The body summarises the change and evidence, lists the three open points, and carries no secrets or hostnames. #60's head is still `5260842`. The decision and the URL are also under the draft in `23-LIVE-CAPTURES.md`.

## Verification

- `cargo test --workspace --no-fail-fast` (after the last code commit `6258fc6`; later commits are docs only): **2471 passed, 0 failed, 61 ignored** (live and Windows-host gates); 2468 passed and 56 ignored before this plan. The known flake `live_gateway::guard_drop_during_unwind_inside_runtime_still_cleans_up` did not fail.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check`: clean.
- Offline: `live_deploy_apply` 2 passed (the URL guards), 5 ignored; `live_transport_wflwvm01` 8 passed (including `event_ms_reads_the_run_store_timestamp`), 12 ignored.
- Live: the rig and Windows runs above. `ign` on PATH was reinstalled with `cargo install --path crates/ignition-cli --force` before the Windows run (the reinstall changed the binary, so macOS asked for Keychain access again; the user allowed it).
- The coordinator's rust-analyzer report (E0277 at `live_deploy_apply.rs` lines 365 and 910, `Vec<String>` from `Option<String>`) was **stale**: it described the first compile; `.filter_map(|r| r.name)` replaced `.map` in the same session, and `cargo test -p ignition-core --test live_deploy_apply --no-run` compiles clean (re-run 2026-10-18T13:54Z).

## Deviations from Plan

1. **[Rule 1 - Bug] Lock acquire timeout 0** (above), `84dca10`.
2. **[Scope] Extra test `live_kill_after_green_verify_then_recover`** (A9, the README and 23-14 carry-over), and a directory-level check in the tree comparison.
3. **[Observation recorded in the README] Empty `<module>/<type>` directories remain after a rollback** that removed the first resource of a type (by design of the host ops). The README rollback section now says so, and that a recovery of a run killed during the write is refused `scan_lock_held` until the dead hold lapses while one killed after the scan recovers at once.
4. **[Environment] The first rig start faulted** ("Commissioning Failure") because `up22` reused a 36-hour-old `gw-data` volume; `down -v` and a fresh `up22`/`setup22` fixed it.
5. **[Plan structure] Task 1's commit holds the whole rig test file**, with the Task 2 fault tests, because they share one file and were verified together.
6. **[Tracer gate]** Auto mode is off and the orchestrator directed a sequential unattended run, so execution continued after the tracer commit (`3262a46`) instead of stopping for a `checkpoint:human-verify`, as in 23-10 to 23-15. The tracer's verify passed before expansion.
7. **[Keychain halt]** After the `cargo install` the new binary's first keyring read blocked; the run halted ("Blocked by: Keychain") and resumed after the user allowed it. No retry loop.
8. **Not done:** the optional canary scan of live run directories (no way to set a secret field without printing it). The Linux ssh host is unproven live: **FA-23-05 stays owed**, no Linux host exists, and none was invented; the README and the PR text say so.
9. **[Wording] The Windows test calls the binary with a 600 s limit** (not 120 s) for apply and rollback, which take 125 to 166 s on this link; the plan and status calls use 120 s.

## Known Stubs

None. A grep for `TODO`, `FIXME`, `todo!`, `unimplemented!` and `skip` over the added lines finds only the quiet-skip helper of the live gates (`skip("IGNITION_LIVE_DEPLOY is not 1")`), which is the documented gate behaviour; the `#[ignore]` tests carry their reason.

## Threat Flags

None beyond the plan's register. T-23-65 (live gate against a non-test gateway): loopback and gateway-name guards, the pinned host key, env opt-in; the URL guards have offline tests. T-23-66 (host left changed): each live test restores and asserts the pre-run state; the VM was checked by hand afterwards. T-23-67 (shared spec PR): a new branch stacked on #60, nothing pushed to #60's branch or to the user's clone. T-23-68 (live output in the capture doc): envelopes, counts and run ids only; no token, key or file body.

## Requirements

`requirements-completed` is empty by instruction. Advanced: DAPL-05 (the Windows apply through the binary), DAPL-06 (kill, lapse), DAPL-08 (rollback by run id from a fresh context, live), DAPL-09 (journal recovery, live).

## Carry-overs (phase end)

- FA-23-05: Linux ssh host unproven live; update the README sentence and the `commit.sh` conformance note if a host appears.
- windows-latest CI has not run `deploy_host_conformance` with the PowerShell rows (23-20 owed): push the branch when you choose and open the CI run.
- `snapshot/` subdirectories are created 0755 inside the 0700 run directory (23-15, no leak); transport stderr tails are echoed by design (22-05). Both are user decisions.
- PR #63 awaits review and merge in whk-environment-orchestration.

## Self-Check: PASSED

- Files exist: `crates/ignition-core/tests/live_deploy_apply.rs`, `crates/ignition-core/src/deploy/apply/hold.rs`, `crates/ignition-core/tests/live_transport_wflwvm01.rs`, `README.md`, this SUMMARY; `23-LIVE-CAPTURES.md` has three `## Live gate:` sections (`grep -c '^## Live gate:'` = 3) and `## Spec §9 amendment draft (23-16)`.
- Commits `84dca10`, `3262a46`, `6258fc6`, `62583b2`, `972443a` exist in `git log`; PR #63 exists and targets `docs/prod-gateway-config-as-code`.
