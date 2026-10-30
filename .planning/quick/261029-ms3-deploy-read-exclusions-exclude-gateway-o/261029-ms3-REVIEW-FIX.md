---
phase: 261029-ms3-deploy-read-exclusions-exclude-gateway-o
fixed_at: 2026-10-30T02:00:00Z
review_path: .planning/quick/261029-ms3-deploy-read-exclusions-exclude-gateway-o/261029-ms3-REVIEW.md
iteration: 1
findings_in_scope: 6
fixed: 6
skipped: 0
status: all_fixed
---

# Quick 261029-ms3: Code Review Fix Report

**Fixed at:** 2026-10-30
**Source review:** .planning/quick/261029-ms3-deploy-read-exclusions-exclude-gateway-o/261029-ms3-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 6 (WR-01 to WR-05, plus IN-01 by request)
- Fixed: 6
- Skipped: 0
- Out of scope, not touched: IN-02, IN-03, IN-04

All work ran in the main checkout on `fix/deploy-read-exclusions`. There was no worktree (`workflow.use_worktrees=false`, and `.claude/CLAUDE.md` has the shared `CARGO_TARGET_DIR` rule). Each fix was written test-first: the new test failed before the fix and passed after it.

## Fixed Issues

### WR-01: `wrong_target` fell back to the `core` system-properties when the mode copy was unreadable

**Files modified:** `crates/ignition-core/src/deploy/apply/preflight.rs`, `crates/ignition-core/tests/deploy_apply.rs`, `README.md`
**Commit:** 54073cd
**Applied fix:** `host_system_name` now checks `observed.unreadable` for the `ignition/system-properties` unit:
- If the mode copy is unreadable, the check fails closed with "the mode collection's system-properties file has a path too long for the host's sftp server to read". It no longer falls back to `core`.
- If there is no mode copy and the `core` copy is unreadable, it fails closed the same way.

`deploy apply` and `deploy projects` share this code through `wrong_target_gate`. Test: `preflight_wrong_target_unreadable_system_properties_fails_closed` runs both collections. Before the fix, the apply went ahead on the `core` name. The README preflight table and the `deploy projects` notes are updated.
**Status:** fixed: requires human verification (gate logic)

### WR-02: A declared or ledgered unit unreadable on the backup was not refused before the first write

**Files modified:** `crates/ignition-core/src/deploy/apply/preflight.rs`, `crates/ignition-core/src/deploy/apply/mod.rs`, `crates/ignition-core/src/deploy/apply/rollback.rs`, `crates/ignition-core/src/deploy/apply/gates.rs`, `crates/ignition-core/src/error.rs`, `crates/ignition-core/tests/deploy_apply.rs`, `README.md`
**Commit:** 0cf5ab7
**Applied fix:**
- **Detection.** `backup_read_finding` now takes the backup's `unreadable_units` from the read it already makes. It matches them the way the post-apply file check does: `exempt_unreadable` with a throwaway `FileComparison`, over the rendered paths plus this run's deletes. To supply the deletes, `PreflightInput` gets a new field, `deleted_paths`.
- **Refusal.** A match fails `backup_read` with the unit keys in the detail, and `PreflightReport` gets a new field, `backup_unreadable`. `drive` then returns `deploy_unreadable` (exit 6) before the backup sample, the lock and any write to the master. The run directory and `run.json` already exist at that point, with `backup_read: fail` recorded.
- **Error text.** `CoreError::DeployUnreadable` gets a new field, `node` (`master` or `backup`), so the message names whose sftp server cannot read the unit. The plan gate passes `master`. The `deploy_unreadable` four-place test now checks the node wording. The slug, exit code and hint are unchanged.

Tests:
- `backup_declared_unit_unreadable_on_the_backup_is_refused_before_any_write` replaces the old `..._is_not_converged`, which asserted the `applied_backup_unsynced` behaviour.
- `backup_deleted_unit_unreadable_on_the_backup_is_refused_before_any_write` covers a unit the apply deletes.

Both assert that the master's data directory is byte-identical, that no lock was taken, and, for the declared case, that only probe calls reached the host.

Notes:
- **Ledger coverage.** "Ledgered" is read the way the post-apply check reads it: a ledgered unit counts if the apply deletes it. A unit that is ledgered but neither rendered nor deleted is still only a warning from the file check, as before.
- **Precedence.** When other preflights also fail, `deploy_unreadable` wins, as it does in the plan gate. The other failures are still in `run.json` and in the `preflight_checks` event.

README: apply Order step 4 and the `backup_read` row are updated.
**Status:** fixed: requires human verification (refusal precedence and present/absent logic)

### WR-03: The Windows `list_tag_providers` listing skipped the PowerShell completeness check

**Files modified:** `crates/ignition-core/src/deploy/transport/sftp.rs`, `README.md`
**Commit:** fde8245
**Applied fix:**
- On a Windows host, `list_tag_providers` now runs `cross_check_names`: one PowerShell walk of the `tag-provider` directory, compared with the names sftp listed.
- `transport_failed` naming the path is raised for any of these:
  - a provider directory the listing omitted, whatever its length (a names-only listing has no unreadable list to put it in);
  - a name sftp listed that the host does not hold;
  - a reparse point directly below the directory.
- The walk code moved out of `cross_check` into `enumerate_roots` without change.

Test: `windows_list_tag_providers_fails_closed_when_the_listing_omits_a_provider` covers the complete case, an absent directory (no walk), an omitted provider and a reparse point. README Windows read notes are updated.
**Status:** fixed

### WR-04: The apply engine did not refuse unreadable declared or ledgered units itself

**Files modified:** `crates/ignition-core/src/deploy/apply/gates.rs`, `crates/ignition-core/src/deploy/apply/mod.rs`, `crates/ignition-core/tests/deploy_apply.rs`
**Commit:** 0963538
**Applied fix:** The key collection moved to a new function, `gates::unreadable_refusal(plan) -> Option<CoreError>`. `evaluate_gates` and a new first check in `run_apply` both call it, so the two cannot drift. The `run_apply` check runs before the no-op return, the run directory, the lock and any host call.

Test: `run_apply_refuses_a_plan_with_a_declared_unreadable_unit_that_skipped_the_gates` hand-builds `ApplyPrepared::Ready` over a plan with a declared unreadable unit and checks that nothing happens: no write, no `.ign/`, no host call and no lock. Before the fix, the engine applied. This is internal API only, so the README is unchanged.
**Status:** fixed

### WR-05: `deploy pull --yes` silently deleted the repo copy of a `path_too_long` unit

**Files modified:** `crates/ignition-core/src/actions/deploy.rs`, `crates/ignition-core/tests/deploy_pull.rs`, `README.md`
**Commit:** 5dfa186
**Applied fix:** The repo copy is not kept, and the behaviour is now documented and tested.

Why not keep it: a kept copy is not what the host serves. The written tree would then render to something other than the `render_sha256` pull reports, and render then plan would no longer be empty. That breaks the pull fixpoint that the production re-test in CONTEXT relies on.

The removal is now announced. `removed_unreadable_warnings` adds one `data.warnings` line per `path_too_long` unit whose repo files the pull removed. The line names the unit, the reason and each removed file (gateway-relative, up to 10 then "and N more"), and says git still holds them. A unit the repo did not hold gives no warning.

Test: `pull_yes_names_the_repo_files_of_a_unit_that_became_unreadable` runs a first pull, then a second pull with `--yes` over a host where the unit is now unreadable. A fresh bootstrap pull gives no warning. README: the pull Commands row and the `path_too_long` skip-reason row are updated.
**Status:** fixed

### IN-01: Manifests that declare `System` in `tag_providers` now fail render, with no migration note

**Files modified:** `README.md`
**Commit:** 10d6f58
**Applied fix:** Added an "Upgrading a repo that lists `System`" bullet to the Excluded tag providers section. It covers:
- **Symptom:** `deploy_tree_invalid`, exit 2, naming `tag_providers[i]`.
- **Migration:** remove `System` from `tag_providers`, and remove any `tags/System/` or `modes/<env>/tags/System/` directory.
- **Effect on the gateway:** none. An excluded provider is never written or deleted.

The suggested hint on the contradictions message was not added. Only the README note was requested.
**Status:** fixed

## Verification

All gates ran in the main checkout (`/Users/pmannion/whiskeyhouse/ignition-cli`), at HEAD 10d6f58, using the global `CARGO_TARGET_DIR` (`~/Library/Caches/cargo-target`) and `CARGO_INCREMENTAL=0`. Anyone can reproduce them from the main checkout.

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo test --workspace --no-fail-fast` | 96 suites, 3000 passed, 0 failed, 88 ignored (2995 before; +5 new tests, 1 replaced) |
| `cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings` | clean |
| `PLAN_GOLDEN`, `PLAN_GOLDEN_LEDGER`, `PLAN_GOLDEN_UNREADABLE` | unchanged (`git diff 4615071..HEAD` touches no `PLAN_GOLDEN` line) |
| `readme_exit_table_agreement`, `exit_code_mapping_enumerated`, `deploy_unreadable_four_places` | pass (part of the workspace run) |

Files left unstaged as instructed: `.planning/config.json`, `.planning/research/.cache/`, `svc-ignition-deploy@*.ps1`, `.planning/quick/261029-ms3-*`.

---

_Fixed: 2026-10-30_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
