---
phase: quick-261104-ava
plan: 01
status: complete
subsystem: deploy (Windows sftp completeness, conformance junction cases, pair refusal docs)
tags: [deploy, windows, sftp, redundancy, tags, conformance, coderabbit, pr35, ci]
requires:
  - 261104-7cy (C3 list_tree residue rule, C6 junction case, C12 re-list)
provides:
  - native_path / make_junction test helpers for mklink /J
  - cross_check(found, absent, budget): requested folders sftp omits are probed; unlisted long entries are charged to READ_MAX_ENTRIES
  - README, branch table, message and hint text for the unmanaged exception and the late re-list refusal
affects:
  - deploy plan / apply / verify on a Windows redundant pair (list_tree)
  - every Windows sftp read (long-entry budget)
  - Windows CI conformance leg
tech-stack:
  added: []
  patterns:
    - "a names-only listing has no unreadable list: anything the sftp listing omits that the host holds is transport_failed"
key-files:
  created: []
  modified:
    - crates/ignition-core/tests/deploy_host_conformance.rs
    - crates/ignition-core/src/deploy/transport/sftp.rs
    - README.md
    - crates/ignition-core/src/deploy/apply/mod.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-core/tests/deploy_apply.rs
decisions:
  - "N2: a requested folder the host holds but sftp did not return is transport_failed, not listed (planner decision): list_tree is names-only and has no unreadable list, the WR-03 rule of cross_check_names"
  - "N3: the long-entry charge sits in cross_check per root, so read and list_tree are both bounded"
  - "N4: the message and hint keep the substring 'nothing was written', qualified with the collections, which holds on every path that raises the slug"
metrics:
  completed: 2026-11-04
  tasks: 3
actuals:
  tokens: 10400
  tasks: 3
  commits: 3
---

# Quick 261104-ava Plan 01: PR #35 round 2 (Windows CI junction, sftp completeness, pair refusal docs) Summary

The Windows CI conformance leg now makes its junctions with backslash-only `mklink /J` arguments. On a Windows sftp host, `list_tree` probes every requested type folder the listing omits and refuses one the host holds. Long-path entries the listing omits count against `READ_MAX_ENTRIES`. The README, the apply branch table and the `deploy_pair_tags_refused` message and hint now cover the `unmanaged` exception and say what a late re-list refusal leaves behind.

## Findings

W1: fixed
- RED: Windows CI run 41848470336. `cmd /C mklink /J` failed with `Invalid switch - "resources"`, and both `conformance_local_core_probe_walks_every_folder_on_a_touched_path` and `conformance_ps_core_probe_walks_every_folder_on_a_touched_path` panicked at the `mklink /J` assert. Offline RED: `native_path_uses_backslashes_only` was written before the helper and failed to compile with `error[E0425]: cannot find function native_path in this scope`.
- Change: a file-level `native_path` (every `/` to `\`) and a `#[cfg(windows)] make_junction` (`cmd /C mklink /J` with native paths, asserting success). Case (e) and `ps_junction_target_dir_is_refused` both call it, and the local `native` closure is gone. Case (e)'s refusal and "nothing was written through the junction" assertions are unchanged.
- Other junction sites checked: `live_project_host.rs` (PowerShell `Join-Path`, native) and the live Windows transport test's `MAKE_JUNCTION` (backslash-built parameters). Neither has the bug, so neither changed.
- Behaviour only runs on the Windows CI leg after a push, which this task did not do. On macOS the helper test passes and xwin clippy compiles the cfg(windows) code for windows-msvc.
- Commit: ac2a6d6

N1: fixed
- Docs only. The behaviour was already pinned and those pins pass: `pair_tags_written_or_kept_in_the_mode_collection_are_refused_on_a_pair_only` (lib), `pair_mode_tag_dirs_names_only_folders_with_no_read_file` (deploy_plan) and `pair_verify_warns_on_mode_tag_files` (deploy_pair).
- Change: the README now names the `unmanaged` exception in four places: the `deploy apply` Commands row, the `What is still refused` bullet, the `deploy plan` `Redundant pairs` paragraph and apply step 5. Such a unit is not refused, `deploy plan` does not name it, its files are not folder-listing residue, its mode files still keep the backup from syncing, `deploy verify` warns about them, and they are removed by hand (runbook T6e).
- Commit: 6da56e2

N2: fixed
- RED: `windows_list_tree_refuses_a_type_folder_sftp_omits` case (b) panicked with `type folder omitted: []`, meaning `list_tree` returned `Ok` with no listing for a type folder the sftp listing dropped. Case (a) already passed before the fix.
- Change: `cross_check(found, absent, budget)`. `list_tree` passes the requested chains missing from `found.roots`. They are enumerated in the same `enumerate_roots` call as the found roots and split back by position. An absent chain the host holds is `transport_failed`: `<chain> exists on the host but the sftp listing did not return it (...)`. One the host does not hold is still left out, with no error. Case (a) confirms the enumeration is still one PowerShell call.
- **N2 decision:** fail with `transport_failed` rather than list the folder (planner decision). `list_tree` is names-only and has no unreadable list, which is the WR-03 rule `cross_check_names` already applies. Either way the deploy cannot proceed, but a transport failure names the real cause (the sftp server) instead of residue the operator cannot see over sftp.
- **read() observation:** `read` passes `&[]`, so its behaviour is unchanged. A scoped root that sftp omits is still read as absent there. That has the same shape as N2, is outside N2's scope, and is not fixed here.
- Commit: 7f2aa9d

N3: fixed
- RED: `windows_cross_check_counts_long_entries_sftp_omits` (a) failed `assertion left == right` (`left: 199997`, `right: 200000`). Without the charge, `budget.entries` did not move.
- Change: for each found root, `cross_check` builds the sftp file and folder sets before the long-path filter. It counts that root's long entries the listing did not return, adds the count to `budget.entries`, and past `READ_MAX_ENTRIES` returns `more than {READ_MAX_ENTRIES} entries under the data directory (at <root>)`, the same wording as `list_level`. A long entry sftp did return is not charged twice. Both `read` and `list_tree` pass their budgets. The existing missing, extra and size comparisons still exclude long paths, applied by `retain` after the count.
- Commit: 7f2aa9d

N4: fixed
- Docs plus one test pin. `drive` and `run_apply` confirm what a late re-list refusal leaves. On the host, `discard_interrupted` drops `journal.json`, `snapshots/<run>` and `staging/<run>`. Locally, the run directory keeps `run.json`, `plan.json`, `events.ndjson`, `read_back_baseline.json`, the `.gwbk` (unless `--skip-gwbk`) and the `result.json` that `run_apply` writes once. No collection file is written and no lock is taken.
- Change: the README C12 sentence (it no longer says "with nothing written") and apply step 10 (which now names the pair re-list) say this. The branch table's re-list row adds that the run directory, its `.gwbk` and `result.json` stay, and the lock-refused row now says "nothing was written into a collection". The message now ends `nothing was written to the {mode} or core collection` and the hint ends `nothing was written to the mode or core collection`. The substring `nothing was written` is kept, so the existing assertions in `deploy_apply.rs` and `live_deploy_pair.rs` still hold. Slug `deploy_pair_tags_refused` and exit 6 are unchanged. `deploy_pair_tags_refused_four_places` now checks both full phrases.
- Pin: `pair_core_type_folder_made_after_the_plan_is_refused` asserts that the one run directory holds `result.json` and exactly one `.gwbk`.
- Commit: 6da56e2

## Windows CI must confirm

- `conformance_local_core_probe_walks_every_folder_on_a_touched_path` and `conformance_ps_core_probe_walks_every_folder_on_a_touched_path`: case (e) now gets past `mklink /J` and must reach its refusal assertion (`a junctioned core\ignition`) with nothing written through the junction. If it fails at the refusal assertion, that is a real C6 finding for a follow-up, not a setup failure.
- `ps_junction_target_dir_is_refused` (a plain `#[tokio::test]`, not a `conformance_ps_` row) must still pass through `make_junction`.

## Deviations from Plan

- I edited `error.rs` while Task 2 was still uncommitted. To keep each commit green on its own, I saved that edit as a scratch patch, restored the file, committed Task 2, then applied the patch back in Task 3. Final content is unchanged.
- I reflowed two README lines after the scripted edit to keep the column width. Text unchanged.
- No existing test expectation changed. Every existing sftp, plan, apply, pair, verify and transport test passed unchanged.

## Hygiene

- No worktree. All cargo runs were sequential on the main checkout. No Cargo.toml or Cargo.lock change, no slug or exit-code change, goldens untouched.
- Staging was by explicit path only. The protected paths, STATE.md and this task's PLAN and SUMMARY were not committed. No co-author lines, no push, no PR reply, no production call, no host names or addresses.
- RED output: `261104-ava-t1-red.out` and `261104-ava-t2-red.out` in the session scratchpad.

## Gate tokens

- T1-OK
- T2-OK
- FULL-GATE-OK (fmt, workspace clippy, `cargo test --workspace` with 0 FAILED, windows-msvc xwin clippy, goldens unchanged, 3 commits since d5efc63). `wait_restart_witnessed_path_golden` did not trip.

## Self-Check: PASSED

The modified files exist, and commits ac2a6d6, 7f2aa9d and 6da56e2 are on feat/deploy-tags-core after d5efc63.
