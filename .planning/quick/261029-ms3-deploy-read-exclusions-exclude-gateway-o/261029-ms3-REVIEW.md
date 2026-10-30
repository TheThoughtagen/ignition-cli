---
phase: 261029-ms3-deploy-read-exclusions-exclude-gateway-o
reviewed: 2026-10-29T00:00:00Z
depth: standard
files_reviewed: 25
files_reviewed_list:
  - crates/ignition-core/src/deploy/exclude.rs
  - crates/ignition-core/src/deploy/target.rs
  - crates/ignition-core/src/deploy/manifest.rs
  - crates/ignition-core/src/deploy/render.rs
  - crates/ignition-core/src/deploy/plan.rs
  - crates/ignition-core/src/deploy/pull.rs
  - crates/ignition-core/src/deploy/mod.rs
  - crates/ignition-core/src/deploy/pair.rs
  - crates/ignition-core/src/deploy/verify.rs
  - crates/ignition-core/src/deploy/transport/mod.rs
  - crates/ignition-core/src/deploy/transport/sftp.rs
  - crates/ignition-core/src/deploy/transport/local.rs
  - crates/ignition-core/src/deploy/apply/gates.rs
  - crates/ignition-core/src/deploy/apply/preflight.rs
  - crates/ignition-core/src/deploy/apply/converge.rs
  - crates/ignition-core/src/actions/deploy.rs
  - crates/ignition-core/src/actions/deploy_apply.rs
  - crates/ignition-core/src/error.rs
  - crates/ignition-cli/src/render.rs
  - README.md
  - crates/ignition-core/tests/deploy_plan.rs
  - crates/ignition-core/tests/deploy_pull.rs
  - crates/ignition-core/tests/deploy_apply.rs
  - crates/ignition-core/tests/deploy_verify.rs
  - crates/ignition-cli/tests/contract_deploy.rs
findings:
  critical: 0
  warning: 5
  info: 4
  total: 9
status: issues_found
---

# Quick 261029-ms3: Code Review Report

**Reviewed:** 2026-10-29
**Depth:** standard (diff `8577e01..4615071`, read-only; no cargo run)
**Files Reviewed:** 25
**Status:** issues_found

## Summary

The core safety mechanics hold up under adversarial tracing:

- **Excluded providers** are filtered at every read scope (`Scope::from_manifest`), refused at render (manifest `tag_providers`, `tags/<P>/`, `modes/<env>/tags/<P>/`), and skipped in the ledger delete set *before* `in_scope`, so a ledger written before the exclusion never yields a delete. Fold-matching is consistent between `is_excluded` and `excluded_provider`.
- **Unreadable units** are removed from `desired/core/mode` groupings, never become entries, are checked in `delete_set` before the "absent" test, mark `in_ledger`, and force `has_changes`. `evaluate_gates` refuses with `deploy_unreadable` after the `plan_stale` check and before the no-op return, so an otherwise-empty plan cannot slip through.
- **`plan_sha256` stability**: `U` lines are emitted only when `unreadable` is non-empty and the header is unchanged. `group_keyed` and `unit_key_layer` reduce to the old `group`/`unit_key` when nothing is unreadable. The excluded set is not hashed. `PLAN_GOLDEN`/`PLAN_GOLDEN_LEDGER` are untouched by the diff (only the new `PLAN_GOLDEN_UNREADABLE` was added).
- **Strict parsing**: both `Target` and `Manifest` get `#[serde(default)]` plus `list_problems`; misspelled keys are covered by tests.
- **Secrets**: only unit keys, collection-relative paths, provider names, and lengths are echoed. No file bytes are added to any new output.

No Critical finding. Five Warnings are paths where an unreadable object can still be treated as absent, or where the pre-write refusal does not reach a node or read it should.

## Warnings

### WR-01: `wrong_target` gate falls back to the `core` system-properties when the mode copy is unreadable

**File:** `crates/ignition-core/src/deploy/apply/preflight.rs:387-420` (`host_system_name`)
**Issue:** `host_system_name` reads `observed.mode.get(&path).or_else(|| observed.core.get(&path))` and never looks at `observed.unreadable`. With this change a Windows host with an over-limit `<mode>/ignition/system-properties/config.json` no longer fails the read. The mode file lands in `observed.unreadable`, `observed.mode` lacks it, and the gate silently takes `systemName` from the `core` copy. The mode collection overrides `core` on the gateway, so the gate compares the REST name against a name the gateway is not using. This is exactly "unreadable treated as absent", on the gate that stops a write (and `deploy projects`, via `wrong_target_gate`) hitting the wrong gateway. A long mode name makes the mode path cross 240 characters while the `core` path does not. Even with no core fallback, the message would be the misleading "the host has no system-properties file".
**Fix:**
```rust
// after the local_units check
if observed.unreadable.iter().any(|u| {
    (u.collection == "mode" || u.collection == "core")
        && u.path.as_str().starts_with(&format!("{SYSTEM_PROPERTIES_UNIT}/"))
}) {
    return Ok(Err("its path is too long for the host's sftp server to read"));
}
```
Add a test with the mode file unreadable and a readable core copy; the gate must fail closed with the unreadable reason.

### WR-02: A declared or ledgered unit unreadable on the backup node is not refused before the first write

**File:** `crates/ignition-core/src/deploy/apply/preflight.rs:819-831` (`backup_read_finding`); `crates/ignition-core/src/deploy/apply/converge.rs:304-345`
**Issue:** The "refuse before any write" rule is enforced only against the master's plan (`evaluate_gates`). The backup preflight reads the backup's files and reports a pass with a file count, ignoring `observed.unreadable`. A rendered unit whose path is over-limit only on the backup (different install dir, or a different mode-name length) passes every preflight. The master is then written and scanned, and only afterwards does `converge` flag the unit as `unreadable`. The result is `applied_backup_unsynced` (exit 6) with the master already changed. That is a refusable condition known before the lock is taken.
**Fix:** In `backup_read_finding`, compute `unreadable_units(&observed)` and fail the finding when any unit is rendered (`declared.outcome.tree.files` has a path in `info.paths`, or the unit key matches). At minimum, emit a warning for the others. Mirror `exempt_unreadable`'s present/absent logic, or reuse it with a throwaway `FileComparison`.

### WR-03: `list_tag_providers` skips the Windows PowerShell cross-check, so an omitted provider directory is silently absent from a generated manifest

**File:** `crates/ignition-core/src/deploy/transport/sftp.rs:1612-1661` (`SftpTransport::list_tag_providers`)
**Issue:** Bootstrap pull used to discover providers by `read`ing `ignition/tag-provider` whole. On Windows that ran `cross_check`, which fails the read if the sftp listing omitted anything (the C7a-8 failure mode: the server drops over-limit entries from listings). The new names-only listing has no cross-check. A provider directory the server drops from `ls` is not named, not classified unreadable, and not reported. It is just missing from `tag_providers` in the generated manifest. The following `read` never scopes it, so nothing downstream can notice. A directory is shorter than the files in it, so the likelihood is low. The rule is still "an unreadable object is never treated as absent", and this is a new gap that did not exist before the change.
**Fix:** On `self.windows`, run one PowerShell enumeration of the `tag-provider` directory (depth 1) and compare names with the sftp listing, using the existing `cross_check` helpers. Fail `transport_failed` on any directory the listing omitted. Or surface omitted names as skipped `path_too_long` entries in `generate_manifest`'s `skipped`.

### WR-04: The apply engine itself does not refuse unreadable declared/ledgered units; the only gate is in `deploy_apply_prepare`

**File:** `crates/ignition-core/src/deploy/apply/mod.rs:798-826` (`run_apply`); `crates/ignition-core/src/actions/deploy_apply.rs:197-200`, `209-214`
**Issue:** `ApplyPrepared::Ready { plan, .. }` is a `pub` enum, and `run_apply` is `pub`. `run_apply` re-checks only `has_changes`. A plan carrying `unreadable` units with `declared || in_ledger` has `has_changes == true`. If it reaches `run_apply` by any route other than `evaluate_gates` (a future caller, a test harness, a TUI), the engine proceeds: it creates the run dir, takes the lock, and writes the readable entries. It would then write a new ledger from the rendered tree that includes the unreadable declared unit's files. The safety property rests on one call site.
**Fix:** Add a cheap invariant at the top of `run_apply`, before `RunDir::create_fresh`:
```rust
let blocked: Vec<String> = plan.unreadable.iter()
    .filter(|u| u.declared || u.in_ledger).map(|u| u.key.clone()).collect();
if !blocked.is_empty() { return Err(CoreError::DeployUnreadable { keys: blocked }); }
```
Share the key-collection with `evaluate_gates` so the two cannot drift.

### WR-05: `deploy pull --yes` silently deletes the repo's copy of a unit it then skips as `path_too_long`

**File:** `crates/ignition-core/src/deploy/pull.rs:285-289`; `crates/ignition-core/src/actions/deploy.rs:763-872` (`write_pulled_fs`)
**Issue:** Pull removes every file in `resources/`, `tags/`, and `modes/<env>/` and rewrites only what it pulled. An unreadable unit is dropped from the pull (`units.remove`), so a previously committed declared unit is removed from the working tree. The only trace is a `data.skipped[]` row. This is a stricter loss than the other skip reasons: the unit still exists live and is merely unreadable, and a later apply will refuse it only if it is in the ledger. The skip is by design. The unannounced removal of declared content is not.
**Fix:** In `deploy_pull_in`, when `existing` holds files for a key in `observed`'s unreadable set, either carry those repo files over into the written tree or add a per-unit warning ("kept the repo copy of X"). Carrying them over is the safer choice. Otherwise, name the removed repo files in `warnings`.

## Info

### IN-01: Existing manifests that declare `System` in `tag_providers` now fail render with no migration note

**File:** `crates/ignition-core/src/deploy/exclude.rs:20-21`; `crates/ignition-core/src/deploy/render.rs:96-103`; `README.md` (Excluded tag providers section)
**Issue:** `System` is excluded unconditionally and "cannot be turned off in this release". A repo whose `manifest.yaml` already lists `System` (previously valid) will fail every `render`/`plan`/`verify`/`apply`/`pull` with `deploy_tree_invalid`. The README documents the rule but not the upgrade consequence. Nothing in the error text says to remove the entry.
**Fix:** Add one sentence to the README, and consider a hint on the `contradictions` message ("remove it from tag_providers").

### IN-02: Exclusion matching is ASCII-only, and entries are not screened for control characters

**File:** `crates/ignition-core/src/deploy/exclude.rs:~40-55`, `~103`
**Issue:** `eq_ignore_ascii_case` is documented as an assumption. NTFS folds non-ASCII letters, so a provider `Énergie` is not matched by an excluded `énergie` on a Windows host, and the provider is then read and may be deleted. `entry_problem` builds on `component_problem`, which allows control characters (`\n`, ESC). Those are echoed raw by `eprint_excluded_tag_providers` and the finding messages. Both are low risk: the entries come from the operator's own repo.
**Fix:** Fold with `to_lowercase()` for the comparison (and in the dedupe key), or refuse non-ASCII entries. Reject `char::is_control` in `entry_problem`.

### IN-03: `deploy_unreadable` hint advises `exclude_tag_providers` even when the unit is not a tag unit

**File:** `crates/ignition-core/src/error.rs` (`DeployUnreadable` hint); `crates/ignition-core/src/deploy/apply/gates.rs:77-86`
**Issue:** The hint is generic. The error's own test uses `ignition/database-translator/OLD`, for which "list the provider in exclude_tag_providers" does not apply. The message also lists keys with `listed(keys, keys.len())`, so `APPLY_LIST_SHOWN` truncation never shows "and N more" (the count is passed as the shown length). The text could grow without bound.
**Fix:** Pass the true total and slice, as the other deploy errors do. Name the exclusion advice only when a key starts with `ignition/tag-`.

### IN-04: Minor duplication and indirection

**File:** `crates/ignition-core/src/deploy/pair.rs:682-690`; `crates/ignition-core/src/deploy/target.rs` and `manifest.rs` tests
**Issue:** `pair::unreadable_units` is a one-line passthrough to `plan::unreadable_unit_keys`, and `UnreadableUnits` is a plain alias of the same map. The "bad exclude list" table is duplicated verbatim in `target.rs` (`BAD_EXCLUDE_LISTS`) and `manifest.rs` tests. `unit_key_layer` is rebuilt three times per run (plan, pull, `unreadable_unit_keys`).
**Fix:** Re-export the plan function directly, and put the table in a shared `#[cfg(test)]` helper next to `exclude::list_problems`.

---

_Reviewed: 2026-10-29_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
