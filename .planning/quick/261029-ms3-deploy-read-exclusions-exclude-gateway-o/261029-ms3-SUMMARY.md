---
phase: quick-261029-ms3
plan: 01
status: complete
subsystem: deploy
tags: [deploy, sftp, windows, tag-providers, exit-codes, redundant-pair]

requires:
  - phase: v1.6 (25)
    provides: deploy pull/plan/verify/apply, the redundant-pair backup checks, the exit-code four-place rule
provides:
  - exclude_tag_providers (target + manifest), built-in System default, never read and never deleted
  - over-long Windows live paths classified unreadable (ObservedFiles.unreadable), never absent
  - pull skip reason path_too_long; plan data.unreadable / summary.unreadable / U canonical line
  - exit-6 slug deploy_unreadable (apply gate before Noop)
  - verify `unreadable` finding (declared red, undeclared warning), master and backup
affects: [deploy apply, deploy verify --node, redundant-pair convergence, README exit table]

actuals:
  tokens: 612143   # chars/4 over the 30 files changed in 8577e01..4615071 (added-lines only: 37352)
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "Unreadable is a third state beside present and absent; every comparison (plan, verify, pair files) fails closed on it"
    - "One shared unit-key definition for unreadable units: plan::unreadable_unit_keys (pull, plan, verify, pair)"

key-files:
  created:
    - crates/ignition-core/src/deploy/exclude.rs
  modified:
    - crates/ignition-core/src/error.rs
    - crates/ignition-core/src/deploy/apply/gates.rs
    - crates/ignition-core/src/deploy/verify.rs
    - crates/ignition-core/src/deploy/pair.rs
    - crates/ignition-core/src/deploy/apply/converge.rs
    - crates/ignition-core/src/actions/deploy.rs
    - crates/ignition-core/src/deploy/plan.rs
    - crates/ignition-core/src/deploy/pull.rs
    - crates/ignition-core/src/deploy/transport/sftp.rs
    - crates/ignition-core/src/deploy/transport/mod.rs
    - README.md

key-decisions:
  - "L1: new exit-6 slug deploy_unreadable under the four-place rule; deploy_blocked not overloaded"
  - "L2: plan header stays ign-deploy:plan:v1; U lines only when unreadable units exist (PLAN_GOLDEN, PLAN_GOLDEN_LEDGER byte-identical)"
  - "L3: excluded providers are not hashed into plan_sha256"
  - "L4: contradiction checks are ASCII-case-insensitive"
  - "L5: System cannot be turned off in this release"
  - "P6: plan_sha256 covers every unreadable unit; has_changes only for declared or ledgered ones"
  - "F2 (orchestrator): the pair file comparison fails closed on unreadable units on either node"

requirements-completed: [QUICK-ms3]

duration: ~75 min (Task 3 run; Tasks 1-2 18:23 and 18:56 commits)
completed: 2026-10-29
---

# Phase quick-261029-ms3 Plan 01: Deploy read exclusions and over-long live paths Summary

Gateway-owned tag providers (`System` plus `exclude_tag_providers` from the target and the manifest) are never read or deleted. A Windows sftp live path of 240 characters or more makes its unit `unreadable`, which is never treated as absent: pull skips the unit, plan lists it, apply refuses `deploy_unreadable` (exit 6), and verify (master and backup) reports a declared one as red.

## Commits

| Task | Commit | Subject |
|------|--------|---------|
| 1 | 72f4b5e | feat(quick-261029-ms3): exclude gateway-owned tag providers from every deploy read |
| 2 | a96f9ce | feat(quick-261029-ms3): over-long Windows live paths are unreadable, never absent (pull, plan) |
| 3 | 4615071 | feat(quick-261029-ms3): apply refuses deploy_unreadable; verify reports declared unreadable units |

## RED evidence per task

- **Task 1:** `cargo test -p ignition-core exclude` failed to compile (exit 101): `error[E0425]: cannot find value DEFAULT_EXCLUDED_TAG_PROVIDERS in this scope`, followed by `cannot find function effective` and `no field exclude_tag_providers on type manifest::Manifest`.
- **Task 2:** `cargo test -p ignition-core unreadable` failed to compile (exit 101): `error[E0560]: struct ObservedFiles has no field named unreadable`, then `error[E0422]: cannot find struct, variant or union type Unreadable`. `cargo test -p ignition-core path_too_long` (separate run) failed the same way.
- **Task 3:** `cargo test -p ignition-core unreadable` failed to compile (exit 101): `error[E0599]: no variant named DeployUnreadable found for enum error::CoreError`. `cargo test -p ignition-core --test deploy_verify unreadable` failed with `error[E0560]: struct ObservedState has no field named unreadable`. For F2, `cargo test -p ignition-core --lib deploy::pair::tests::unreadable` failed with `error[E0425]: cannot find type UnreadableUnits in this scope`.

## Locked choices (L1-L5), as implemented

- **L1.** `CoreError::DeployUnreadable { keys }`, code `deploy_unreadable`, exit 6, no endpoint. The message reads `deploy refused: N unit(s) the deploy would write or delete have a live path the host's sftp server cannot read, so their live copy is unknown: <keys>; nothing was written`. The hint says to shorten the path on the gateway (rename the folder) or, when the gateway owns the provider, to list it in `exclude_tag_providers` and stop declaring it. It names no `ign` subcommand. The four-place rule pins are all updated: variant, `code()`, the exit-6 arm, `hint()`, `DEPLOY_SLUGS` (appended, now 16), `deploy_slugs_have_their_exit_codes` (same order), the slug pin, `exit_code_mapping_enumerated`, the README-agreement list, the module-doc exit-6 row, the README exit-table row, and the `deploy_family_coverage` count pin.
- **L2.** The header stays `ign-deploy:plan:v1`. `U NUL key NUL live_in NUL declared NUL in_ledger` lines come after the D lines and only when unreadable units exist. `PLAN_GOLDEN` and `PLAN_GOLDEN_LEDGER` are byte-identical.
- **L3.** Excluded providers are reported only in `data.excluded_tag_providers` and are not hashed into `plan_sha256`.
- **L4.** Contradiction checks match by ASCII case fold, so a declared `system` is refused. This assumes Ignition provider names and NTFS are case-insensitive.
- **L5.** `System` is always excluded and cannot be turned off in this release.

## Discretion choices (P1-P7)

- **P1.** A duplicate within one list (ASCII fold) is refused: `exclude_tag_providers[i] "X": duplicate tag provider`. Overlap across sources (target and manifest, or a list naming `System`) is allowed and unioned.
- **P2.** Every entry goes through `component_problem`. Blank entries and leading or trailing whitespace are also refused (`deploy_tree_invalid`, exit 2).
- **P3.** Effective set = default, then target, then manifest. Deduplicated by ASCII fold (keeping the byte-smallest spelling) and reported byte-sorted.
- **P4.** The unreadable threshold is the existing conservative `LONG_PATH_LIMIT`: a host path of **240 characters or more**, measured as `entry.full_len - LONG_PATH_PREFIX.len()` (characters from the drive letter, without the `\\?\` prefix). This is the same arithmetic the earlier refusal used. The production run of 2026-10-29 failed on a 240-character path. 260 is the sftp server's own limit and **not** the threshold. Only files define unreadable units. An over-limit directory is only left out of the completeness comparison.
- **P5.** The Linux sftp and local transports are unchanged: `unreadable` is always empty there.
- **P6, the `has_changes` reading (checker warning recorded).** The requirement "`has_changes` and `plan_sha256` must cover it" is read as two rules. `plan_sha256` covers **every** unreadable unit, one U line each. `has_changes` is set only by an unreadable unit the deploy must act on (declared, or listed in the ledger). An undeclared, unledgered unreadable unit is treated like `unmanaged` (D-06): it does not set `has_changes` and raises one warning. So pull, then render, then plan still reports `has_changes: false`, and an apply over such a unit is a no-op. The U line keeps that plan's hash distinct from a plan without the unit. Task 3's gate does not rely on `has_changes`: it refuses before the Noop return.
- **P7.** Names: `excluded_tag_providers`, `unreadable`, `summary.unreadable`, `path_too_long`, the verify finding `unreadable`, and the slug `deploy_unreadable`.

## Excluded-provider delete-set guard (Task 1)

A ledger file under an excluded provider's four tag type directories is skipped **before** the `in_scope` check and before the absent-file check. Each such provider gets one warning with its file count, and its files are never deleted or hashed (L3). This is proven at the plan engine (`excluded_provider_ledger_files_are_never_deleted`) and at apply (`apply_never_deletes_ledger_files_of_an_excluded_provider`).

## Golden value (Task 2)

`PLAN_GOLDEN_UNREADABLE = ef3832a4703a23afa5b7da611ab61662f665d8bd0513355bc1b4bee0bbbc36ae`. This is the PLAN_GOLDEN scenario plus a live-only mode unit `ignition/database-translator/OLD` whose `config.json` is unreadable at 262 characters. It was pinned once `recompute_plan_sha256` agreed.

## Semantic delta (Task 1)

Pull's provider discovery now lists directory names only, so an **empty provider directory (no `config.json`) is now named** in a generated manifest's `tag_providers`. v1.6.0 named only providers with at least one file. This is pinned by `pull_names_an_empty_provider_directory`.

## Task 3 implementation notes

- **Gate.** In `evaluate_gates`, after `plan_stale` and **before** `if !plan.summary.has_changes`, the byte-sorted keys of every `declared || in_ledger` unreadable unit become `DeployUnreadable`. Tests check that a declared unit is refused and that an in-ledger unit with `has_changes` forced false is still refused (never Noop). A wrong `--plan-sha256` still gives `plan_stale` first. An undeclared, unledgered unit gives Noop when nothing else changes and Ready otherwise. `apply_prepare_refuses_a_declared_unreadable_unit_and_writes_nothing` checks exit 6, an unchanged data directory and no `.ign/` (security finding F1).
- **Verify (master).** `ObservedState.unreadable: Vec<(key, declared)>` is filled from the plan's unreadable list. A declared tag unit gets one red `unreadable` finding, which replaces the `files_differ` "not found" one. A declared resource unit also gets a red `unreadable` finding. An undeclared unit adds the warning `files: <key> is unreadable on the host and is not declared; it was not compared`.
- **The backup files check is the pair comparison below**, not a separate code path. Before F2 it was fail-closed only for **rendered** paths (an unreadable path showed up as `file_missing_on_backup` or `file_differs`). It was **not** fail-closed for **deleted** paths. F2 fixes that.

## Deviations from Plan

### Orchestrator-requested (security review of a96f9ce)

**F2: the pair file comparison fails closed on unreadable units.**
- **Issue:** the sftp reader moves over-limit files out of `observed.files`. The redundant-pair comparison (`pair::compare_files` via `effective_files`) then read a deleted path the backup could not read as "gone". That is a false green on apply convergence. It also labelled rendered unreadable paths as missing or differing.
- **Fix:** `pair.rs` gains `UnreadableUnits`, `unreadable_units(observed)` (which is `plan::unreadable_unit_keys`), `exempt_unreadable(comparison, present, absent, [(node, units)])`, `unreadable_warnings`, and `FileComparison.{unreadable, unreadable_unchecked}`. A rendered or deleted unit that either node cannot read (same path, or the same unit key over the compared paths) leaves `differing`, `missing` and `unexpected`. It fails `files_check` with an `unreadable` finding naming the unit and the node, so the backup does not converge. Any other unreadable unit makes the check `warn` (which does not block convergence) and adds one warning. This is wired into `apply/converge.rs` (the master's unreadable units are cached with its read, and the warning is given once per run) and into `deploy verify --node backup|both` (`actions/deploy.rs` `verify_backup_files`, with warnings threaded through).
- **Tests:** 5 pure tests in `pair/tests.rs`: declared on backup is red not missing, deleted-and-unreadable is red not gone, master-unreadable is red not differing, outside both sets is a warning only, and no unreadable units leaves the comparison identical. `deploy_apply.rs` adds `backup_declared_unit_unreadable_on_the_backup_is_not_converged` and `backup_undeclared_unit_unreadable_on_the_backup_is_a_warning_only`. `deploy_verify.rs` adds `verify_node_backup_declared_unit_unreadable_on_the_backup_is_red` and `..._undeclared_..._is_a_warning`.
- **Files outside Task 3's `<files>`:** `crates/ignition-core/src/deploy/pair.rs`, `crates/ignition-core/src/deploy/pair/tests.rs`. Also `deploy/apply/converge.rs`, which is in the plan's `files_modified`.

### Auto-fixed

**1. [Rule 3 - Blocking] DEPLOY_SLUGS count pin in `crates/ignition-cli/tests/deploy_family_coverage.rs`.** `projects_unverified_verdict_exception_is_dated_and_documented` pinned `DEPLOY_SLUGS.len() == 15`. It is now 16, and the message names `deploy_unreadable`. The file is outside Task 3's `<files>` and is named in the commit body.

**2. [Rule 3 - Blocking] `Rig.backup_reader` in `tests/deploy_apply.rs`** changed from `Option<LocalFsTransport>` to `Option<Box<dyn TreeReader>>` so that `MarkUnreadable` can wrap the backup reader.

### Not done / notes

- `crates/ignition-cli/tests/contract_deploy.rs` (in Task 3's `<files>`) is **unchanged**. No binary-level path can produce an unreadable unit, because the local transport never reports one. The suite was run unchanged: 82 passed.
- The plan's literal Cargo check `git diff --quiet "$(git merge-base HEAD main)" ...` fails. The local `main` ref is stale at 708f1a0 (before rc.2 and v1.6.0), so it diffs the already-released `1.6.0-rc.1 -> 1.6.0` bump. Against the real base, 8577e01 (= `origin/main` = `merge-base HEAD origin/main`), Cargo.toml, Cargo.lock and every crate Cargo.toml are **unchanged**.

### Tasks 1 and 2 (from their commit bodies)

Neither commit touched a file outside the plan's `files_modified`, and neither records a deviation. Their recorded deltas are the empty-provider-directory semantic delta and the delete-set guard (Task 1), and the `MarkUnreadable` shared fixture, the `gates.rs plan_of` empty vec, and the CLI human `unreadable <key> live_in=.. declared=.. in_ledger=.. longest=..` stdout line (Task 2).

## Full gate (Task 3, on 4615071's tree)

| Step | Result |
|------|--------|
| `cargo fmt --all --check` | green |
| `cargo clippy --workspace --all-targets -- -D warnings` | green |
| `cargo test --workspace --no-fail-fast` | 96 suites, 2995 passed, 0 failed |
| `cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings` | green |
| `unreadable` tests listed in ignition-core | 43 (>= 10) |
| `deploy_unreadable_four_places`, `deploy::apply::gates` (25), `error::` (23), `deploy_family_coverage` (9), `contract_deploy` (82) | all green |
| Cargo.toml / Cargo.lock vs 8577e01 | unchanged |

- **Known flake:** `winps_script_logic` did not flake in this run, so no re-run was needed.
- **Disk:** the data volume is at 99% (about 15 GiB free). No ENOSPC occurred, `CARGO_INCREMENTAL=0` was not needed, and nothing was deleted.
- **First workspace run:** it used the default fail-fast and stopped at the `deploy_family_coverage` count pin (fixed above). The rerun used `--no-fail-fast` so every binary reported.

## Threat Flags

None beyond the plan's register. F2 closes T-ms3-03 (false green) for the redundant-pair comparison as well as for master verify.

## Hand-off (orchestrator, after merge)

1. `cargo install --path crates/ignition-cli --force`.
2. Add `exclude_tag_providers: ["MQTT Engine"]` to `~/whiskeyhouse/whk-onprem-infra-iac-ign-tree/ignition/targets/production.yaml`.
3. Run `ign deploy pull --env production`, then render, then plan. Expect a fast walk (minutes, not 53), `excluded_tag_providers: ["MQTT Engine", "System"]` and `has_changes: false`. Any `path_too_long` skips are listed by key.
4. Tag v1.6.1 per RELEASING.md (push the tag only).

STATE.md and ROADMAP.md were not updated or committed by this run. The orchestrator commits the planning files.

## Self-Check: PASSED

- FOUND: crates/ignition-core/src/deploy/exclude.rs, crates/ignition-core/src/deploy/pair.rs, crates/ignition-core/src/deploy/apply/gates.rs
- FOUND commits: 72f4b5e, a96f9ce, 4615071
