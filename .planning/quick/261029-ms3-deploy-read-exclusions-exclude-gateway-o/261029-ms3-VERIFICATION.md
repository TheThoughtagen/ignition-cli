---
phase: quick-261029-ms3
verified: 2026-10-29T00:00:00Z
status: passed
score: 9/9 must-haves verified
behavior_unverified: 0
overrides_applied: 0
re_verification: false
---

# Quick 261029-ms3: Deploy read exclusions and over-long live paths Verification Report

**Goal:** Exclude gateway-owned tag providers (target and manifest `exclude_tag_providers`, unioned with built-in `System`) from every `ign deploy` host read, never written or deleted, reported. Treat over-limit Windows live paths (>= 240, `LONG_PATH_LIMIT`) as unreadable: pull skips `path_too_long`, plan lists `unreadable`, apply refuses `deploy_unreadable` (exit 6) when declared or ledgered, verify is red for declared/ledgered unreadable units and a warning otherwise.
**Commits:** 72f4b5e, a96f9ce, 4615071 (base 8577e01)
**Status:** passed
**Re-verification:** No, initial verification

## Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Excluded providers (target, manifest, `System`) are never listed, read or downloaded by pull, plan, verify or apply | VERIFIED | `Scope::from_manifest(manifest, mode, excluded)` filters `tag_providers` (`deploy/transport/mod.rs:103`). All 6 production read sites pass `declared.excluded_tag_providers()` (`actions/deploy.rs:386,659,1038,1313`; `apply/converge.rs:108`; `apply/preflight.rs:814`). The one remaining literal `Scope {}` (preflight.rs:387, `host_system_name`) reads only `system-properties` with an empty `tag_providers`. `target_chains` (sftp.rs:1121) emits chains only for `scope.tag_providers`. Pull discovery uses names-only `list_tag_providers` (`deploy.rs:~640`), filtered by `is_excluded`. Test `pull_exclude_tracer_never_reads_an_excluded_provider` passes. |
| 2 | An excluded provider is never deleted; ledger files under it skip the delete set, with a per-provider warning | VERIFIED | `delete_set` (`plan.rs:~520`) checks `exclude::excluded_provider` before `in_scope` and the absent check. Tests `excluded_provider_ledger_files_are_never_deleted` and `apply_never_deletes_ledger_files_of_an_excluded_provider` pass. |
| 3 | `data.excluded_tag_providers` is in pull, plan, verify and apply data; one human stderr line | VERIFIED | Fields at `deploy.rs:304,470,981`, `deploy_apply.rs:105`. CLI `eprint_excluded_tag_providers` (`cli/src/render.rs:1492`) is called from 4 sites. Test `verify_reports_the_excluded_tag_providers` passes. |
| 4 | Strict parsing and contradictions: bad, blank, padded, unsafe or duplicate entries and unknown fields are refused; excluded-and-declared is `deploy_tree_invalid` | VERIFIED | `exclude.rs` `entry_problem`, `list_problems`, `contradictions`. The target and manifest tests (`exclude_tag_providers_*`, `exclude_tag_provider_misspelled_key_is_refused`) pass. Render tests `render_refuses_*excluded_provider*` pass. Pull refuses a kept manifest that contradicts, before any host read (`pull_exclude_refuses_a_kept_manifest_declaring_an_excluded_provider` passes). |
| 5 | On a Windows sftp host, entries with host path >= 240 are classified unreadable instead of failing the read; short missing entries remain `transport_failed`; Linux and local never report unreadable | VERIFIED | `cross_check` (sftp.rs:1000) classifies `path_len >= LONG_PATH_LIMIT` into `LongEntry` and `read` (sftp.rs:1667-1710) builds `ObservedFiles.unreadable` and drops those files from the download list. Missing short entries still hit the `missing` error branch. Tests `windows_read_unreadable_*` (4) pass. `local.rs` was +27 lines and its `unreadable` is empty. |
| 6 | Unreadable is never absent: pull skips `path_too_long`; plan lists `data.unreadable` and `summary.unreadable`, never in `delete_set`; `plan_sha256` gains `U` lines; `has_changes` for declared or ledgered | VERIFIED | `pull.rs:285-288`, `plan.rs:595-770`. `delete_set` records ledgered unreadable keys before the absent test. Tests `pull_path_too_long_unit_is_skipped_and_plans_unreadable`, `plan_sha256_golden_with_unreadable`, `unreadable_ledger_file_is_never_deleted` and the `unreadable_*` plan tests pass. The `PLAN_GOLDEN` tests are unchanged (the full suite was reported green). |
| 7 | Apply refuses `deploy_unreadable` (exit 6) before any write when an unreadable unit is declared or ledgered, even when the rest of the plan is a noop; undeclared and unledgered units proceed with a warning; four-place rule | VERIFIED | `gates.rs:77-86`, before the Noop return and after `plan_stale`. `error.rs` has the variant, `code()`, exit 6, `DEPLOY_SLUGS`, the enumerated exit tests and the README row. Tests `deploy_unreadable_four_places`, `readme_exit_table_agreement`, `exit_code_mapping_enumerated`, the 5 gate tests, `apply_prepare_refuses_a_declared_unreadable_unit_and_writes_nothing` and `deploy_family_coverage` (9) pass. |
| 8 | Verify: declared unreadable is red `unreadable`; undeclared is a warning; master and backup/pair convergence | VERIFIED | `verify.rs:833-880`, wired from `planned.unreadable` (`deploy.rs:1068`). The pair comparison is fail-closed through `exempt_unreadable` (`pair.rs`, `converge.rs`, `deploy.rs:1329-1354`), covering rendered and deleted paths. Tests `files_declared_unreadable_*`, `files_undeclared_*`, `verify_node_backup_*`, `backup_*unreadable*` and the pair tests pass. |
| 9 | Full gate green; no Cargo.toml or Cargo.lock change | VERIFIED | The caller reports the workspace suite green at 4615071. `git diff --stat 8577e01..4615071` lists no Cargo files. No debt markers (TBD/FIXME/XXX/TODO/ignore) appear in added lines. I did not re-run the full suite, as instructed. |

**Score:** 9/9 truths verified. The behavior-dependent invariants (never absent, never deleted, refuse before write) each have a passing named test.

## Required Artifacts

| Artifact | Status | Details |
|----------|--------|---------|
| `deploy/exclude.rs` | VERIFIED | 273 lines, `DEFAULT_EXCLUDED_TAG_PROVIDERS` present, wired from render, plan, pull, actions. |
| `transport/mod.rs` | VERIFIED | `list_tag_providers`, `Scope::from_manifest(.., excluded)`, `ObservedFiles.unreadable`. |
| `plan.rs` | VERIFIED | `UnreadableUnit`, `unreadable_unit_keys`, the `U` line, and the delete-set guards. |
| `pull.rs` | VERIFIED | `REASON_PATH_TOO_LONG`. |
| `error.rs` | VERIFIED | `DeployUnreadable` / `deploy_unreadable`. |
| `README.md` | VERIFIED | Target and manifest tables, "Excluded tag providers" section, `path_too_long` row, exit-table row, apply gate text. |

## Key Links

All key links from the plan are WIRED: render -> `exclude::contradictions`; the six read sites -> `Scope::from_manifest(.., excluded)`; pull -> `list_tag_providers`; sftp `cross_check` -> `ObservedFiles.unreadable`; `delete_set` -> `excluded_provider` and the unreadable guard; `evaluate_gates` -> `DeployUnreadable`.

## Requirements Coverage

QUICK-ms3: SATISFIED.

## Anti-Patterns

None found in the added lines (no debt markers, no ignored tests).

## Notes (non-blocking)

- **Verify and ledgered-only units.** The master `verify` check turns red only for *declared* unreadable units. A unit that is in the ledger but not declared gets a warning there. This matches CONTEXT ("a declared unit that is unreadable is a red finding; one that isn't declared is a warning"). The pair-convergence path does cover deleted/ledgered paths as red. The task's shorthand "declared/ledgered" is satisfied for apply and pair; master verify follows CONTEXT. Informational only.
- **Semantic delta (documented).** Pull now names empty provider directories in a generated manifest (`pull_names_an_empty_provider_directory`).
- **Unproven against a live host.** The behavior of Win32-OpenSSH 8.1 sftp listing deep directories can only be proven against a real host. The unit tests use a fake transport. That is the post-release re-test against WIGNVM01, not a gap.

## Targeted tests run (all passed)

- `ignition-core --lib`: `unreadable` (22), `exclude`, `readme_exit_table_agreement`, `exit_code_mapping_enumerated`
- `--test deploy_plan` (8), `deploy_pull` (4), `deploy_verify` (6), `deploy_apply` (8), `deploy_render` (2)
- `ignition-cli --test deploy_family_coverage` (9)

## Human Verification Required

None blocking. Post-release: run `ign deploy pull --env production` against WIGNVM01 with `exclude_tag_providers: ["MQTT Engine"]`, then render and plan, and expect `has_changes: false`.

---

_Verified: 2026-10-29_
_Verifier: Claude (gsd-verifier)_
