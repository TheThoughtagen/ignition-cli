---
phase: 24-redundant-pair
verified: 2026-10-21T00:00:00Z
status: passed
score: 7/7 must-haves verified
behavior_unverified: 0
overrides_applied: 0
re_verification: false
gaps: []
deferred: []
notes_nonblocking:
  - "Sync-exhausted (TEC-5105) early exit is unit-tested only: the state was not reachable on the rig (C-P8). Approved decision K5 = d. Matcher assumes ERROR level (review IN-03)."
  - "The post-review live run (6 passed, 206 s, after WR-01..07) is recorded in 24-REVIEW.md Fix status only; 24-LIVE-CAPTURES.md carries the earlier runs (the last block is the K2 gap-fix run, 4 live gates PASS in 258.9 s)."
  - "24-VALIDATION.md is still the unfilled template (status: draft, nyquist_compliant: false). Not a roadmap must-have."
  - "DPAIR-01..04 are intentionally still unchecked in REQUIREMENTS.md; phase completion marks them."
---

# Phase 24: Redundant Pair Verification Report

**Phase Goal:** A deploy to a redundant pair ends with proof that the backup holds the same config as the master, checked through the backup's own API and files and never by writing to it, and a backup that did not converge is reported without undoing a good master deploy.
**Verified:** 2026-10-21
**Status:** passed
**Re-verification:** No, initial verification

Method: goal-backward from ROADMAP success criteria and DPAIR-01..04. SUMMARY claims were checked against the code, targeted test runs and the recorded live captures. The full `cargo test --workspace` suite was NOT run here (the orchestrator owns it).

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | SC1 / DPAIR-01: after a master-verified apply, `ign` checks the backup through its own API and file hashes (same mode, effective view equal to master, redundancy healthy) after a bounded wait and at most one resync, and sends no write to the backup | VERIFIED | `apply/mod.rs` runs `converge::converge_backup` after finalize, restart report and snapshot (about line 1340). `converge.rs` `content()` reads the backup's `deployment_mode`, REST read-back and file hashes against the master's live copies; `redundancy_check` requires `peerConnected`, no `syncPending`, and `lastSyncTimestamp` advanced past a baseline re-sampled after `LockAcquired` (WR-02). `wait_budget` is bounded by `--backup-timeout` (default 300 s, 1 to 86400). `send_resync_once` guards on `self.resync.is_some()`; the only `redundancy_resync` call in the phase goes through `ctx.deploy` (the master session). `BackupNode` (`pair.rs`) has private fields and its only calls are `redundancy_status`, `deployment_mode`, `secret_names`, `logs` and read-back reads. Tests: `backup_phase_is_read_only`, `backup_converges_after_apply`, `backup_files_equal_master`, `backup_files_differ`, `backup_baseline_is_sampled_after_the_lock`, `pair_module_never_writes` all pass. Live: `live_pair_converges` PASS, tap recorded GET only (GET 12, POST/PUT/DELETE 0). |
| 2 | SC2 / DPAIR-02: a backup that stopped syncing or still differs reports `deploy_backup_unsynced`, exit 6, and the master is not rolled back | VERIFIED | `OutcomeKind::AppliedBackupUnsynced` is set when the report is not converged (`mod.rs` ~1357) and never enters `roll_back`. `DeployApplyResult::failure_exit_code()` returns `Some(6)`, `ok` false, `slug: deploy_backup_unsynced` (`BACKUP_UNSYNCED_SLUG`, not a `CoreError` slug); `main.rs` returns the exit through the sanctioned verdict hook. Tests pass: `apply_backup_unsynced_exit_6`, `backup_unsynced_not_syncing`, `backup_unsynced_content_differs`, `backup_cut_at_lock_acquired_ends_unsynced`, `backup_slug_is_not_a_core_error`, `backup_ignores_a_stale_sync_exhausted_line`. Live: `live_pair_unsynced_then_reconverge` PASS: link cut at `lock_acquired`, `applied_backup_unsynced`, slug `deploy_backup_unsynced`, exit 6, reason `not_syncing`, one resync, `restore_started: 0`, master record `applied`, then the pair re-converged. Live negative control for `content_differs` (K2 fix reverted): `applied_backup_unsynced` / `ContentDiffers` after 247 s. |
| 3 | SC3 / DPAIR-03: `ign deploy verify --node both` runs the backup checks on demand and fails when a referenced secret name is missing on either node | VERIFIED | `deploy_verify_node` (`actions/deploy.rs`) runs the master verify, the backup tick and `verify_secret_names` per node with `SecretCheckMode::Verify`; `result.ok` is false when any node's secret status is `Fail`, slug `deploy_verify_failed`, exit 6 through the existing verdict hook. `--node` is wired in `cli.rs` and `main.rs`; `backup`/`both` without `nodes.backup` is refused as `invalid_input` before any contact. Tests pass: `verify_node_both_missing_on_backup`, `verify_node_both_missing_on_master`, `verify_cli_node_both_red_exits_6`, `verify_node_secret_names_are_case_sensitive` (K9). Live: `live_pair_secret_names` PASS: green, then red naming `CAP24_GATE_SP/<name>` on both nodes after removal, then green; canary value absent. Installed `ign` run (`cargo install --path crates/ignition-cli --force`): `verify --node both` exit 0 JSON and human. |
| 4 | SC4 / DPAIR-04: a two-node 8.3.6 rig with a trusted gateway-network link live-gates the three behaviors, including one `deploy_backup_unsynced` run; DSURF-01 coverage and DSURF-03 canary stay green with `--node both` and backup verdicts added | VERIFIED | `crates/ignition-core/tests/live_deploy_pair.rs` plus `live-rig-pair/` fixture; rig confirmed still up (`ign-pair-rig-gw-a-1`, `ign-pair-rig-gw-b-1` healthy). Recorded live runs: converge, stale-signature converge, secret names, unsynced then re-converge (all PASS, `24-LIVE-CAPTURES.md` `## Live gates (24-10)` and the K2 gap-fix section). I re-ran: `deploy_family_coverage` 8/8 (including `backup_unsynced_verdict_exception_is_dated_and_documented` and `every_deploy_slug_is_in_the_readme_exit_table`); `contract_deploy` canary and backup filters 29/29 (`apply_canary_backup`, `apply_canary_backup_refused_listing`, `verify_canary_both`, `apply_canary_all_outcomes`); `deploy_pair` 7/7. |
| 5 | Roadmap constraint: `ign redundancy status` is wired to the library `active_node` (activity level, Master/Backup vocabulary) | VERIFIED | `actions/redundancy.rs` `redundancy_status_pair` calls `client::redundancy::active_node`; `main.rs:3700` wires `--peer`; render at `render.rs:2180`. Tests pass (8/8): `redundancy_status_peer_names_the_active_node`, `..._after_failover`, `..._split_brain`, `..._neither`, `..._same_profile_refused`, `..._without_peer_is_unchanged`. |
| 6 | Roadmap constraints: no write to the backup, no `failover` verb, nothing restarts in apply (SF-3), only secret names are compared, backup failure never triggers master rollback, Decision A (slug in payload, never in the exit table) | VERIFIED | grep finds no `failover` method or route anywhere in `src`; the single resync call is master-only; `secret_names` returns names only and the canary tests are green; `master_active` preflight runs for apply and rollback (activity level, not role; `preflight_master_active_*` pass); README exit table lacks the slug and `DEPLOY_SLUGS` excludes it (pinned by `backup_unsynced_verdict_exception_is_dated_and_documented`). Debt-marker scan (TBD/FIXME/XXX/TODO/todo!/unimplemented!) over 36 phase-touched files: no hits. |
| 7 | User-approved overrides K1..K9, the K2 revision and the K8 extension are reflected in the code | VERIFIED | K1 `SyncPredicate::TimestampAdvanced` (`SYNC_PREDICATE`); K2 revised: `FILE_NORMALIZATION` is a single `resource.json` row ignoring only `attributes.lastModification` and `attributes.lastModificationSignature` (`pair.rs` ~691; lib tests pass 25/25 incl. `resource_json_ignores_only_the_two_modification_stamp_members`); K3 b: `lock_acquired` is emitted after the preflights and before the first write, and the live gate cuts the link from an `ApplyObserver` at that event; K4 a: cut by `setup-pair.sh cut` (toggleApproval); K5 d: `is_sync_exhausted` is a log matcher only, no providers or events route (`COVERAGE.md` rows OPT-OUT); K6 a: `Exhausted` sends one resync and skips the second budget, `TimedOut` runs a second budget (`converge_backup`); K7: 500 and 501 both map to `CannotList` (contract tests pass); K8 and the WR-04 extension: `secrets_defined_by_render` filtered out in `secrets_finding`, `deploy verify` strict (`preflight_secrets_name_added_to_an_existing_provider_by_this_deploy` passes); K9: exact, case-sensitive comparison. |

**Score:** 7/7 truths verified (0 present, behavior-unverified).

Behavior-dependent truths (state transition and "master not rolled back" invariant, "backup never written") have behavioral evidence: the named tests above, and live records (`restore_started: 0`, master record `applied`, method tap with GET only).

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/ignition-core/src/deploy/pair.rs` | read-only `BackupNode`, pure decisions, file and secret checks | VERIFIED | 1061 lines, substantive, used by apply, verify and preflight |
| `crates/ignition-core/src/deploy/apply/converge.rs` | backup phase, one resync, no rollback | VERIFIED | 321 lines, called from `apply/mod.rs` |
| `crates/ignition-core/src/deploy/apply/preflight.rs` | `master_active`, `backup_read`, `secrets` | VERIFIED | findings registered in the preflight list |
| `crates/ignition-core/src/actions/deploy.rs` | `deploy_verify_node`, `verify_secret_names` | VERIFIED | wired from `main.rs` |
| `crates/ignition-core/src/actions/redundancy.rs` | `redundancy_status_pair` | VERIFIED | wired to `--peer` |
| `crates/ignition-core/src/client/deploy_api.rs` | `secret_names`, `redundancy_resync` | VERIFIED | contract tests pass (11/11 filtered) |
| `crates/ignition-core/tests/live_deploy_pair.rs` and `fixtures/live-rig-pair/` | live gates and rig | VERIFIED | live tests are `#[ignore]` by design, guards run offline (2 pass, 4 ignored) |
| `README.md` | verdict exception, `--backup-timeout`, `--node`, preflight rows | VERIFIED | coverage tests pass |

### Key Link Verification

| From | To | Via | Status |
|------|----|-----|--------|
| `deploy apply` (`main.rs`) | `converge_backup` | `ApplySurfaces.backup` built from the backup profile session and read-only `BackupNode` | WIRED |
| `converge_backup` | master `redundancy_resync` | `ctx.deploy` (master session), once | WIRED |
| `AppliedBackupUnsynced` | exit 6 | `failure_exit_code` and the `main.rs` verdict hook | WIRED |
| `deploy verify --node` | `deploy_verify_node` and `verify_secret_names` | `cli::VerifyNodeArg` to `VerifyNode` | WIRED |
| `redundancy status --peer` | `active_node` | `redundancy_status_pair` | WIRED |

### Data-Flow Trace (Level 4)

| Artifact | Data | Source | Real data | Status |
|----------|------|--------|-----------|--------|
| backup report | `data.backup` checks | backup status, mode, REST read-back and `TreeReader` file reads | Yes (live: `files_checked` 4) | FLOWING |
| secrets report | `data.secrets` | per-node `secret_names` listings | Yes (live red and green) | FLOWING |

### Behavioral Spot-Checks (targeted only)

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| pair pure logic | `cargo test -p ignition-core --lib deploy::pair` | 25 passed | PASS |
| engine backup phase and preflights | `cargo test -p ignition-core --test deploy_apply -- backup_ preflight_master preflight_secrets preflight_backup` | 45 passed | PASS |
| verify --node | `--test deploy_verify -- verify_node` | 10 passed | PASS |
| pair mock integration | `--test deploy_pair` | 7 passed | PASS |
| never-writes scan | `--test deploy_transport -- pair_module_never_writes` | 1 passed | PASS |
| REST client | `--test deploy_client_contract -- secret_names resync` | 11 passed | PASS |
| CLI contract and canary | `-p ignition-cli --test contract_deploy -- canary backup verify_cli` | 29 passed | PASS |
| DSURF-01 coverage | `-p ignition-cli --test deploy_family_coverage` | 8 passed | PASS |
| redundancy peer | `-p ignition-cli --test contract_diagnostics -- redundancy` | 8 passed | PASS |
| live guards | `--test live_deploy_pair` | 2 passed, 4 ignored (need the rig env) | PASS |

I did not re-run the live gates (the rig is up and I was told not to disturb it) nor the full workspace suite.

### Probe Execution

No `probe-*.sh` declared by the phase. SKIPPED.

### Requirements Coverage

| Requirement | Source plans | Status | Evidence |
|-------------|--------------|--------|----------|
| DPAIR-01 | 24-01..03, 24-05..10 | SATISFIED | Truths 1, 5, 6 |
| DPAIR-02 | 24-01, 24-02, 24-05, 24-06, 24-08, 24-10 | SATISFIED | Truth 2 |
| DPAIR-03 | 24-02, 24-03, 24-05, 24-07, 24-09, 24-10 | SATISFIED | Truth 3 |
| DPAIR-04 | 24-01, 24-02, 24-10 | SATISFIED | Truth 4 |

No orphaned requirements: REQUIREMENTS.md maps only DPAIR-01..04 to Phase 24 and all are claimed by plans. The checkboxes and traceability rows stay Pending by instruction; phase completion should flip them.

### Anti-Patterns Found

None blocking. Debt-marker scan of added lines in the 36 phase-touched files: none. No skipped or stubbed tests (the live tests are `#[ignore]` with a documented env gate).

### Non-Blocking Notes (warnings, no human decision required to close the phase)

1. Sync-exhausted (TEC-5105) detection is covered by fake-pair unit tests against the spike's recorded line only. The state could not be reproduced on the rig (C-P8), K5 = d and K6 = a were approved on that basis, and no roadmap criterion requires it live. Review IN-03 notes the matcher assumes ERROR log level; if the gateway logs it at WARN the early exit never fires and the wait simply ends on its budget with the one resync (still `deploy_backup_unsynced`). Candidate follow-up: drop `min_level` or document it as best-effort.
2. The post-review-fix live run (6 passed, 206 s) is cited in `24-REVIEW.md` Fix status but not appended to `24-LIVE-CAPTURES.md`; WR-01 and WR-02 changed engine sampling after the captured runs. The code is covered by `backup_baseline_is_sampled_after_the_lock` and `backup_ignores_a_stale_sync_exhausted_line` and the orchestrator reports the live rerun green. Recommend appending that run's record to the captures for audit.
3. Review Info items IN-01..IN-06 were out of scope and remain open (curl argv credentials in rig scripts, deny-list `pair_module_never_writes`, 404 maps to no-provider on a gateway lacking the route, "backup" wording in human output, small rig-script hardening). WR-05 is fixed for profile and location but not for two profiles that resolve to one URL (documented in the Fix status table).
4. `24-VALIDATION.md` is an unfilled template with `status: draft`; fill or drop it before milestone audit.
5. Known unrelated flake: `live_gateway::guard_drop_during_unwind_inside_runtime_still_cleans_up` (recorded in `deferred-items.md`, timing-sensitive under load).

### Human Verification Required

None.

### Gaps Summary

No gaps. Every roadmap success criterion and DPAIR-01..04 is backed by code reading, passing targeted tests, and live captures on the real 8.3.6 pair, including the required `deploy_backup_unsynced` run with the master left applied. The user-approved contract items (K1..K9, K2 revision, K8 extension) are implemented as approved and were not treated as plan deviations.

---

_Verified: 2026-10-21_
_Verifier: Claude (gsd-verifier)_
