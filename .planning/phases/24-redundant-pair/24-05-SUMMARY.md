---
phase: 24-redundant-pair
plan: 05
subsystem: core
tags: [ignition, redundancy, backup-node, deploy, pure-decisions, read-only-handle]
status: complete

requires:
  - phase: 24-redundant-pair
    provides: "24-03 secret_names / SecretNameListing / referenced_secret_names; the 24-02 contract decisions K1-K9, V1-V10"
provides:
  - "deploy::pair::BackupNode: read-only handle (private fields; status, deployment_mode, read_back, read_files, secret_names, sync_exhausted)"
  - "BackupSignals, BackupReport (V2 keys), BackupCheck/BackupCheckId, UnsyncedReason, ResyncRecord, redundancy_check"
  - "converged, classify (V2 precedence), wait_step (clock-free), report, SYNC_PREDICATE = TimestampAdvanced (K1 a)"
  - "compare_files, files_check, files_skipped_check, FILE_NORMALIZATION (one row, resource.json)"
  - "check_secret_names, collect_referenced_secrets, declared_secret_providers, SecretNamesReport::ordered"
  - "is_sync_exhausted (log matcher, bool only)"
  - "verify::{mode_check, read_back_check} (pub(crate)), actions::deploy::gather_read_back (pub(crate))"
  - "backup: Option<&BackupNode> on ApplyContext, RollbackContext, ApplySurfaces and PreflightInput; backup_deploy removed"
  - "pair_module_never_writes source scan"
affects: [24-06, 24-07, 24-08, 24-09, 24-10]

actuals:
  tokens: 20400
  tasks: 3
  commits: 5

tech-stack:
  added: []
  patterns:
    - "a node handle whose type cannot express a write (private fields, read methods) plus a source scan that pins it"
    - "pure decisions over value-free tick inputs with the clock injected"

key-files:
  created:
    - crates/ignition-core/src/deploy/pair.rs
    - crates/ignition-core/src/deploy/pair/tests.rs
    - crates/ignition-core/tests/deploy_pair.rs
  modified:
    - crates/ignition-core/src/deploy/mod.rs
    - crates/ignition-core/src/deploy/verify.rs
    - crates/ignition-core/src/actions/deploy.rs
    - crates/ignition-core/src/actions/deploy_apply.rs
    - crates/ignition-core/src/deploy/apply/mod.rs
    - crates/ignition-core/src/deploy/apply/rollback.rs
    - crates/ignition-core/src/deploy/apply/preflight.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-core/tests/deploy_transport.rs
    - crates/ignition-core/tests/deploy_apply.rs
    - crates/ignition-core/tests/live_deploy_apply.rs

key-decisions:
  - "K1 a: SYNC_PREDICATE is TimestampAdvanced; SyncPredicate::StableTicks(u8) exists and is tested but is not the one in force"
  - "A skipped provider (K8, preflight) alone leaves the node status pass; only a missing name fails and only an unverifiable provider warns"
  - "BackupSignals keeps project_state only when it is a short status word (at most 32 characters of letters, digits, space, - or _), else null, so gateway text cannot ride in it"
  - "sync_exhausted reads one page of ERROR-level log lines from the last ten minutes; the window uses the local wall clock, the decision functions use none"
  - "report() builds the redundancy check with require_advance from SYNC_PREDICATE; deploy verify (24-09) never waits, so it must build its own converged verdict or pass require_advance false itself"

patterns-established:
  - "Backup code takes BackupNode, never a DeployApi or an apply host for nodes.backup"

requirements-completed: []

duration: not timed
completed: 2026-10-19
---

# Phase 24 Plan 05: `deploy::pair` read-only backup handle and pure decisions Summary

**`BackupNode` is the only door to the backup and it only reads; every backup decision (converged, reason, wait step, file compare, secret names, sync-exhausted matcher) is a tested pure function; the old `backup_deploy` field is gone from the apply, rollback and preflight surfaces.**

## What was built

- **Task 1 (tracer).** `deploy/pair.rs` with `BackupNode` (private `gateway`, `deploy`, `reader`), `BackupSignals` (`-1` reads as never synced on either sample, via the same rule as `last_sync_epoch_ms`), the V2 `BackupReport` and `redundancy_check`. `tests/deploy_pair.rs` runs the handle over the verbatim C-P4 settled gw-b body with the deploy surface wrapped in `ForbidWrites`. `pair_module_never_writes` in `tests/deploy_transport.rs` fails if `pair.rs` names any of the twelve write identifiers.
- **Task 2.** `converged` (linked, predicate, no failed content check; a `warn` does not block), `classify` (`unreachable`, `sync_exhausted`, `not_syncing`, `content_differs`), `wait_step` (budget inclusive, interval clamped to the time left with a 1 ms floor), `report`, `compare_files` (sha256 of raw bytes; missing, differing and still-present deletes each listed with the path), `files_check` (at most `APPLY_LIST_SHOWN` findings plus counts), `files_skipped_check`, the secret-name functions and `is_sync_exhausted`. `verify::mode_check` and `verify::read_back_check` were split out of the private checks (which now call them), and `gather_read_back` became `pub(crate)`; all 51 `deploy_verify` tests and the verify unit tests pass unchanged.
- **Task 3.** `backup_deploy` (a `&dyn DeployApi`) replaced by `backup: Option<&BackupNode>` on `ApplyContext`, `RollbackContext`, `ApplySurfaces` and `PreflightInput`. `mode_finding` now takes the already-read answer (`Option<Option<String>>`); `run_preflights` reads the master and then the backup through `deployment_mode()`. Findings and statuses are byte-identical; no test changed beyond the struct-literal edits and the `Rig` plumbing.

## Deviations from Plan

### Contract overrides relayed by the orchestrator (24-02 decisions)

**1. [Override K2 = b] `FILE_NORMALIZATION` has one row and no excluded member**
- The plan's step describes re-comparing "without the named member". The row is `("resource.json", "")`: the empty member means nothing is removed, so the two files are compared as whole parsed JSON documents (member order and whitespace ignored, every value equal). Every other file stays sha256 byte-equal, and the match is on the file name, not a directory. The plan's "stop and ask if it differs" precondition was satisfied by the relayed decision.
- Tests: reordered and pretty-printed `resource.json` pass; a changed value, a missing member and non-JSON differ; a `config.json` byte difference fails; CRLF against LF differs.

**2. [Override K5 = d] `is_sync_exhausted` is the log matcher only**
- `is_sync_exhausted(message: &str) -> bool` matches `exceeds the max attempt count`. It is tested with the verbatim spike line (`E [R.S.SyncManager] [config-resource] State sync request failed. Sync operation has failed 5 time(s), which exceeds the max attempt count. ...`, from the 24-02 plan / RESEARCH, spike `results/RIG.md`), the message-only form and three non-matching lines. The K5 a/b body tests were dropped. There is no `DeployApi::redundancy_providers`; `BackupNode::sync_exhausted` reads `GatewayApi::logs`.

**3. [Override K7] `CannotList` is `unverifiable`, status `warn`**
- `SecretNameListing::CannotList` (501 or the 500 a `remote` provider answers, mapped in 24-03) puts the provider in `unverifiable`; the node is `warn` when nothing is missing and `fail` when a name is.

**4. [Override K8 = a] Preflight skips a render-declared absent provider**
- In `SecretCheckMode::Preflight` a provider the render declares that answers `NoProvider` (or has no listing) goes to `skipped_providers`; in `Verify` mode, and for a provider the render does not declare, its names are `missing`.

**5. [Override K9 = a] Exact, case-sensitive names** — plain string comparison; `Name` against a listing of `name` is missing.

**6. [Override K6 = a]** Applies to the engine (24-06). Nothing in this plan sends a resync or holds a second budget; `wait_step` returns `Exhausted` immediately so the engine can end the wait, send the one resync and report `sync_exhausted`.

### Plan departures

**7. [Test double] The `Rig` backup gateway is a dead address, not a second wiremock server**
- The plan asked for `backup_api` on its own wiremock server. `with_backup` is a synchronous helper, so the backup's `ReqwestGatewayApi` points at `http://127.0.0.1:1`. The preflight reads only the backup's deploy surface, so nothing connects; a call the handle should not make fails loudly and can never reach the master's server.

**8. [Plan wording] Unit tests live in `deploy/pair/tests.rs`**
- The plan says "unit tests in `pair.rs`". They are in the child module `deploy::pair::tests` (declared at the end of `pair.rs`), so `cargo test --lib deploy::pair` runs them and `pair_module_never_writes` scans only the production code. The acceptance command `--lib report_carries_no_gateway_text` matches by name.

**9. [Rule 2 - Missing critical functionality] `projectState` filtered to a short status word**
- The gateway's `projectState` is text that would otherwise land in `data.backup.project_state`. T-24-17 forbids gateway text in the report, so `BackupSignals::from_status` keeps it only when it is at most 32 letters, digits, spaces, `-` or `_` (`Good`, `Out of Date`); anything else becomes null. Test: `report_carries_no_gateway_text`.

**10. [Rule 1 - Bug in own test] `normalize_deployment_mode` does not trim**
- A first integration test assumed `" production "` reads as `production`. The function returns a real name verbatim (documented in `client/mode.rs`); the test now seeds `production`, a whitespace-only mode and no mode.

### Added beyond the plan

- `unreachable_check`, `mode_backup_check`, `read_back_backup_check`, `BackupCheckId::word` and `SecretNamesReport::ordered` (the master-before-backup ordering rule needed one owner), plus `SYNC_LOG_WINDOW_MS`.

## Verification

- `cargo test -p ignition-core --lib deploy::pair` 19 passed; `--lib deploy::verify` 7; `--test deploy_pair` 6; `--test deploy_verify` 51; `--test deploy_transport pair_module_never_writes` and `write_half_unreachable` pass; `--test deploy_apply` 91; `-p ignition-cli --test contract_deploy` 55; `live_deploy_apply --no-run` builds.
- `grep -rn backup_deploy crates` prints nothing; `grep -nE 'pub (gateway|deploy|reader):' deploy/pair.rs` prints nothing.
- `cargo clippy --workspace --all-targets -- -D warnings` exits 0.
- `cargo test --workspace --no-fail-fast`: every test binary passes except `live_gateway::guard_drop_during_unwind_inside_runtime_still_cleans_up` (the pre-existing load flake noted since Phase 23; see `deferred-items.md`). It failed in the workspace run and again when run alone, while the machine's load average was 36 to 44 from other processes; the test asserts that a best-effort Drop-path cleanup reached a wiremock server within 10 s, touches no redundancy, deploy or apply code, and no file of this plan is in its build graph beyond `ignition-core` as a whole. Not fixed here.

## Issues Encountered

- The first build after the tests landed took 13 minutes because an editor `cargo check --workspace` was running against the same target directory; later builds were minutes, not hours. No code consequence.

## Known Stubs

None.

## Threat Flags

None. T-24-15 (a backup write cannot be expressed: private fields, read methods, `pair_module_never_writes`, `ForbidWrites` in `tests/deploy_pair.rs`, `backup_deploy` removed), T-24-16 (`converged` needs content checks and the same node's own timestamps), T-24-17 (`is_sync_exhausted` returns a bool, the report types have no gateway-text field, `projectState` is filtered) and T-24-18 (the existing apply, rollback and CLI suites pass unchanged) are covered by tests.

## Requirements

DPAIR-01, DPAIR-02 and DPAIR-03 are not marked complete: this plan ships the handle and the pure decisions; the engine, preflights, CLI and live gates follow in 24-06 to 24-10.

## Self-Check: PASSED

Created files and the five task commits (ec5e701, d798bc0, 369829d, 9850788, 6c7cfec) exist.
