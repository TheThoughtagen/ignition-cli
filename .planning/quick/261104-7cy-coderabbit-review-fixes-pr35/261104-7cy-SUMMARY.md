---
phase: quick-261104-7cy
plan: 01
status: complete
subsystem: deploy (pair tag residue, core probe, commit scripts, review records)
tags: [deploy, redundancy, tags, core, probe, commit.ps1, commit.sh, coderabbit, pr35]
requires:
  - 261103-ell (F1-F5, F2 on the core branch)
provides:
  - TreeReader::list_tree and the full-listing C3 residue rule
  - the C12 re-list before the scan lock in apply::drive
  - one top-down core probe walk on the local, sh and ps hosts
  - commit.ps1 never recreating config/resources/core
  - corrected 261103-ell gate and verification records, README pair verify paragraph
affects:
  - deploy plan / apply / verify on redundant pairs
  - write_host probe on every host
tech-stack:
  added: []
  patterns:
    - "one refusal definition (gates::pair_tags_refusal_with) for saved and fresh residue"
    - "probe walk top-down from core/ignition, memoized per folder, one code per type"
key-files:
  created: []
  modified:
    - crates/ignition-core/src/deploy/plan.rs
    - crates/ignition-core/src/deploy/transport/mod.rs
    - crates/ignition-core/src/deploy/transport/local.rs
    - crates/ignition-core/src/deploy/transport/sftp.rs
    - crates/ignition-core/src/deploy/transport/local/apply_ops.rs
    - crates/ignition-core/src/deploy/transport/scripts/commit.sh
    - crates/ignition-core/src/deploy/transport/scripts/commit.ps1
    - crates/ignition-core/src/deploy/transport/winps.rs
    - crates/ignition-core/src/actions/deploy.rs
    - crates/ignition-core/src/deploy/apply/gates.rs
    - crates/ignition-core/src/deploy/apply/mod.rs
    - crates/ignition-core/tests/deploy_apply.rs
    - crates/ignition-core/tests/deploy_pair.rs
    - crates/ignition-core/tests/deploy_host_conformance.rs
    - crates/ignition-core/tests/live_deploy_pair.rs
    - README.md
    - .planning/quick/261103-ell-review-fixes-f1-f5-option-a2/261103-ell-PLAN.md
    - .planning/quick/261103-ell-review-fixes-f1-f5-option-a2/261103-ell-VERIFICATION.md
decisions:
  - "C3: residue is any unread file, or any folder with no read file below it, in a pair's mode tag type folder, found through a names-and-kinds TreeReader::list_tree; widening read() and adding folders to ObservedFiles were rejected"
  - "C12: the re-list runs after staging and immediately before HeldLock::acquire, not under the lock (planner decision, accepted by the orchestrator)"
  - "C5-C8: one top-down probe walk rule on all hosts from core/ignition to each touched key's parent"
metrics:
  completed: 2026-11-04
  tasks: 5
actuals:
  tokens: 36500
  tasks: 5
  commits: 6
---

# Quick 261104-7cy Plan 01: CodeRabbit review fixes on PR #35 Summary

A pair deploy that would leave any unread file or folder in a mode tag type folder is refused before any write, also when the content appears after the plan; every host's core probe walks each touched folder from `core/ignition` down; `commit.ps1` never recreates `core`; a pair's `plan --diff` shows a mode copy as a removal; the live sync check drops its timing assertion; and the 261103-ell records and README say what shipped.

## Findings

C1: fixed
- 261103-ell PLAN Task 3c verify derives `V` from the last `F2 rig verdict` line (a `(retry)` line counts) via `grep -E ... | tail -n 1 | sed -E ...` and `test -n "$V"`, replacing the exactly-one count and the first-match `sed`. Task 3c's action preamble says the same. All 13 quiet greps reading a pipe (Task 2: 1, Task 3a: 1, Task 3b: 1, Task 3c: 10) became `grep <flags without q> ... >/dev/null`; quiet greps on named files stay. Every edited `<automated>` line passes `bash -n`. Checked against a sample log (`blocked` then `(retry): core` gives `core`) in the full gate.
- Commit: e78856f.

C2: fixed
- 261103-ell VERIFICATION records the final state after re-checking the facts (`git show --stat b5579ec`, the README sentence, `pair_verify_fails_on_mode_copy_of_declared_unit` re-run: ok, that SUMMARY's `## Live`): commits include b5579ec, truth 6 VERIFIED on the `core` branch with `F2 rig verdict: core`, truth 7 VERIFIED, eight live gates passed, xwin clippy ran, `FULL-GATE-OK`. `status: human_needed` stays for the two Windows `conformance_ps_*` rows only; `re_verified: 2026-11-04` and a `score` line added; the F3 open item is noted as closed by C3 (65b80b7).
- Commit: e78856f.

C3: fixed
- RED (before any code change):
  - `pair_core_migration_refused_with_unseen_content` (`deploy_apply.rs:8315`): "unread content beside the mode copy is residue", `prepare` returned `Ready { plan ... }`.
  - `pair_core_unseen_content_plan_warns` (`deploy_apply.rs:8350`): `assertion left == right failed: []`, `left: 0`, `right: 1`.
  - `pair_verify_warns_on_unseen_content` (`deploy_pair.rs:738`): "ignition/tag-definition/default/Empty: redundant pair: the master's dev collection holds 2 tag file(s) or folder(s) ...": the one pair warning named only the undeclared `Other` unit's two files, not the empty folder beside them.
  - `list_tree` unit tests `deploy::transport::local::tests::list_tree_lists_files_and_folders_of_a_type_folder`, `deploy::transport::sftp::tests::list_tree_lists_files_and_folders_of_a_type_folder`, `deploy::transport::sftp::tests::windows_list_tree_is_checked_against_a_walk`: RED by temporarily empty method bodies (see Deviations).
- Commit: 65b80b7.

C4: fixed
- RED: `pair_plan_diff_shows_mode_copy_as_removal` (`deploy_pair.rs:767`): `assertion left == right failed: --- live/ignition/tag-definition/default/Deploy/tags.json`, `left: 2`, `right: 1` (the repo bytes appeared twice, once as a replacement of the mode copy).
- Fix: `render_diffs` reads desired bytes only for a row with a desired sha. `plan_sha256` and every golden unchanged.
- Commit: c7526fc.

C5: fixed
- RED: `conformance_local_core_probe_walks_every_folder_on_a_touched_path` and `conformance_sh_core_probe_walks_every_folder_on_a_touched_path` (`deploy_host_conformance.rs:1682`): "a file below the type folder: ProbeReport { writable: true, problems: [] }". Per case on local and sh: (a) an unwritable folder below the type folder, (b) a file below the type folder and (c) a symlinked folder below the type folder all passed the probe.
- Commit: b8f2ed5.

C6: fixed
- RED: `deploy::transport::winps::tests::ps_core_probe_walks_top_down_and_never_recreates_core` (`winps.rs:2033`): "the candidate loop is gone"; against the unchanged `commit.ps1`, the C6 pins (candidate loop gone, `OpProbe` calls `CoreWalk` with `KeyRel`, `CoreWalk` splits the path and tests `IsLink` and `CanAddFile`) all failed. The junction case (e) of the ps conformance row runs only on Windows CI.
- Commit: b8f2ed5.

C7: fixed
- RED: the same structural test; the C7 pins (`function CanAddDir` tests bit 4, `CanAddFile` via `CanAdd 2`, the walk calls `CanAddDir $last`) failed against the unchanged script.
- Commit: b8f2ed5.

C8: fixed
- RED: sh case (d) a symlinked `core/ignition`: `refused_ok=false report=ProbeReport { writable: true, problems: [] }`. The local host already refused case (d) before the fix (`core_tags_unsafe:tag-definition` through its `O_NOFOLLOW` descriptor walk); the row keeps it as a pin.
- Commit: b8f2ed5.

C9: fixed
- RED: the same structural test; the C9 pins (`MoveOne` core-root guard before `MakeParents`, `MakeParents` base check before its loop) failed; the existing pin (no `CreateDirectory((Xp $S.Cr))`) passed. `commit.sh` `make_parents` uses plain `mkdir --` and cannot create a missing base, so sh is unchanged.
- Commit: b8f2ed5.

C10: fixed
- Verified by compile (`cargo test -p ignition-core --test live_deploy_pair --no-run`) and by reading (live test, no offline RED). `assert_non_tag_change_syncs` keeps `Applied`, `converged`, `resync == None`, `files` pass, reads only, settled and both nodes `Good`; `waited_s` stays in the printed diagnostic. Its doc no longer promises a zero-second first look. The kept-assertion greps were scoped to the function body (plan-checker W2). Exercised live by `live_pair_core_tag_units_converge` and `live_pair_core_migration_converges` (see `## Live`).
- Commit: 8bac082.

C11: fixed
- README `**Redundant pairs in verify.**`: a declared tag unit with a mode copy fails `files` with a `files_differ` finding naming the mode paths (first ten and count); other content (an undeclared unit's mode tag file, any unread file or folder in a mode tag type folder, an empty folder included) adds one `data.warnings` line; `--node both` carries the finding and the warning once, `--node backup` neither. The "whatever its unit's status" clause is gone. `readme_exit_table_agreement` (1 passed) and `deploy_family_coverage` (9 passed) green.
- Commit: e78856f.

C12: fixed
- RED: `pair_core_type_folder_made_after_the_plan_is_refused` (`deploy_apply.rs:8400`): "the late folder is residue", the apply returned `outcome: Applied, ok: true` and wrote both `.core/` keys although `ignition/tag-type-definition` was made in the master's mode collection after the plan. `pair_core_relist_keeps_a_clean_run` passes as the no-false-refusal pin.
- Commit: af35aa4.

## C3 design

- **Rule.** For each listed `ignition/<type>` folder of a pair-unsynced tag type in the master's mode collection, the read set is the bare (mode) rows with a live sha of entries of that type, whatever their status. No read path below the folder: the folder itself is residue, named `ignition/<type>` (the F3 output, unchanged). Otherwise any listed file not in the read set, and any listed folder with no read path below it, is residue, named by the topmost such paths. `unmanaged` read files stay out (cef P5); the delete set is not an input, so the engine can apply the rule to a saved plan.
- **`TreeReader::list_tree(collection, dirs)`** returns names and kinds (no bytes) of every entry below each requested `<module>/<type>` folder, one `TreeListing` per existing folder in request order, refusing symlinks, special files and (Windows) reparse points, counting against `READ_MAX_ENTRIES`, and on a Windows sftp host held to the PowerShell walk with long-path entries kept. Implemented on the local and sftp readers over their existing walks; the three test readers delegate, count or stay `unreachable!`.
- **Rejected.** Widening `read()` (downloads bytes of out-of-scope providers and still cannot see empty folders); adding folders to `ObservedFiles` (touches every literal and still misses out-of-scope providers).
- Slug `deploy_pair_tags_refused` and exit 6 unchanged; `DeployPlanResult.pair_mode_tag_dirs` stays `#[serde(skip)]`, outside `plan_sha256`.

## C12 position decision

Deviation from CodeRabbit's wording ("after the scan lock is taken and before commit"), a planner decision accepted by the orchestrator: the re-list runs in `apply::drive` after `ctx.host.prepare` and the `Staged` event, immediately before `HeldLock::acquire`. The scan lock stops only the gateway's scan, not a file being made, so listing under it adds no protection; time under the lock comes out of a hold whose `commit_budget_s` is fixed in the staged, hashed work list; a recursive sftp listing (one session per level, plus the PowerShell walk on Windows) under the lock could push a near-cap commit past the hold. The window left open is the lock request plus the at most 5 s backup sample. A refusal drops the run's staging (`discard_interrupted`), takes no lock, triggers no scan, commits nothing, and `result.json` records it.

## C5-C8 walk rule

For each touched `.core/` key, walk its collection path's folders top-down from `core/ignition` to the file's parent, stopping at the first missing level or the first problem:
- a symlink, reparse point or non-directory at any level: `core_tags_unsafe:<type>` (a linked `core/ignition` included);
- an existing directory at or below `core/ignition/<type>` that is not writable and searchable (Windows: not `CanAddFile`): `core_tags_unwritable:<type>`;
- `core/ignition` need only be a real, searchable directory when the type folder exists;
- the deepest existing folder must be writable when the walk stopped at a missing level, and on Windows also pass `CanAddDir` (CreateDirectories).

One code per type; each folder checked at most once per probe. Codes, slugs and the texts' leading folder names unchanged; only `core_probe_problem`'s wording widened. Same rule in local `probe_paths_blocking`, `commit.sh` `op_probe` and `commit.ps1` `OpProbe` (`CoreWalk`). sftp `apply_ops` unchanged (it runs the scripts).

## Live

- Docker (OrbStack) answered; no `ign-pair-rig` container existed beforehand.
- Rig, from `crates/ignition-core/tests/fixtures/live-rig-pair`, each step under `timeout 900 /bin/bash ./setup-pair.sh <step>` (the rig's own state directory; tokens never printed or logged): `up` 2026-11-04T13:48:15Z to 13:49:10Z exit 0, `mint` to 13:49:57Z exit 0, `pair` to 13:51:48Z exit 0 (linked on the first attempt; no link-race recovery needed), `mode` to 13:54:29Z exit 0.
- `status` at 13:54:39Z: gw-a Master Active, gw-b Backup Cold, both peerConnected true, no sync pending, Good, mode production.
- Command (from the repo root, `tokens.env` sourced, tokens never printed): `IGNITION_LIVE_PAIR=1 IGN_PAIR_URL_A=http://localhost:18300 IGN_PAIR_URL_B=http://localhost:18301 IGN_PAIR_DATA_A=$HOME/.cache/ign-pair-rig/data-a IGN_PAIR_DATA_B=$HOME/.cache/ign-pair-rig/data-b PATH=<scratchpad>/sysbash:$PATH timeout 5400 cargo test -p ignition-core --test live_deploy_pair -- --ignored --show-output --test-threads=1`, teed to `261104-7cy-live.log` in the scratchpad, on HEAD e78856f (C3, C12, C5-C9 and C10 all in).
- Began 2026-11-04T13:54:56Z, ended 14:01:11Z, exit 0.
- `test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 365.96s`
- Each of the eight gates printed `LIVE-RAN <gate>` and `test <gate> ... ok`: `live_pair_converges`, `live_pair_converges_with_a_stale_signature`, `live_pair_core_migration_converges`, `live_pair_core_tag_units_converge`, `live_pair_refuses_a_tag_unit`, `live_pair_secret_names`, `live_pair_tag_cleanup_converges`, `live_pair_unsynced_then_reconverge`. So on a real 8.3.6 pair the C3 full listing and the C12 re-list do not false-refuse a clean run or a migration, and the C5-C9 probe walk passes real core folders.
- The C10 diagnostic printed `waited_s 0` on both non-tag changes this run; the check no longer depends on it.
- A grep of the live log for the sourced token values found none.
- `status` at 14:01:20Z: gw-a Master Active, gw-b Backup Cold, both Good, mode production. Torn down at 14:01:44Z (`docker compose -p ign-pair-rig down -v`, exit 0); no `ign-pair-rig` container remains.

Live verdict: pass

## Commits

- c7526fc fix(261104-7cy): show a pair's mode copy of a routed tag unit as a removal in plan --diff (C4)
- 65b80b7 fix(261104-7cy): refuse any unread file or folder in a pair's mode tag type folder (C3)
- af35aa4 fix(261104-7cy): list a pair's mode tag type folders again before the lock and refuse new residue (C12)
- b8f2ed5 fix(261104-7cy): probe every existing core folder on a touched path and never recreate core in a write (C5-C9)
- 8bac082 test(261104-7cy): drop the zero-wait assertion from the live non-tag sync check (C10)
- e78856f docs(261104-7cy): correct the 261103-ell gate and verification and the README pair verify paragraph (C1, C2, C11)

## Deviations from Plan

**1. [Task 2, C3] `list_tree` unit tests written after the code**
- The three `list_tree` unit tests (local, sftp, Windows sftp walk) were written after the method; their RED was shown by temporarily emptying the method bodies, restored before the commit. The three end-to-end C3 REDs came before any code change.
- Commit: 65b80b7.

**2. [Task 3, C12] Re-list before the lock, not under it**
- CodeRabbit asked for the re-list after the scan lock is taken; it runs immediately before `HeldLock::acquire` (planner decision, accepted by the orchestrator). Reasons in `## C12 position decision`.
- Commit: af35aa4.

**3. [Task 3, C12] Doc-only touch of `actions/deploy.rs`**
- The `pair_mode_tag_dirs_of` helper's doc now names its third caller (`apply::drive`). No code change in that file in Task 3.
- Commit: af35aa4.

**4. [Task 4, C8] Local case (d) already refused**
- A symlinked `core/ignition` was already `core_tags_unsafe` on the local host (its `O_NOFOLLOW` descriptor walk); C8 reproduced on sh only. Case (d) stays in the row as a local pin. sftp `apply_ops` is unchanged (it runs `commit.sh` / `commit.ps1`).
- Commit: b8f2ed5.

**5. [Task 5, C10] Kept-assertion greps scoped to the function body**
- Per plan-checker W2, the C10 checks for `report.resync, None`, `report.converged`, `Applied`, `files` and the `waited_s` diagnostic were run on `sed -n '/fn assert_non_tag_change_syncs/,/^}/p'` output, not on the whole file.

## Windows CI items

These rows are `cfg(windows)`, compile under xwin clippy for windows-msvc and run only on the Windows CI leg:
- `conformance_ps_core_probe_walks_every_folder_on_a_touched_path` (cases (b), (e) a junctioned `core\ignition` for C6, (f) and (g)).
- `conformance_ps_core_restore_created_only_without_core_root` and `conformance_ps_core_probe_refuses_unsafe_type_folder` (261103-ell F4, F5).
- The `commit.ps1` C6, C7 and C9 changes are pinned offline by `winps::tests::ps_core_probe_walks_top_down_and_never_recreates_core`; their behaviour runs only on Windows.

## Hygiene

- No IP-shaped literal beyond loopback and documentation ranges, no private-range prefix, and no production or VM-style host name in this SUMMARY or the committed diff.
- No slug, exit code, Cargo.toml or Cargo.lock change. `PLAN_GOLDEN*`, `RENDER_GOLDEN` and the fingerprint `GOLDEN` are byte-identical to 579017a.
- No co-author line; nothing pushed, no PR reply, no worktree, no production call. The protected local files, STATE.md and this PLAN and SUMMARY were never staged.

## Gate tokens

- `T1-OK` (Task 1, C4)
- `T2-OK` (Task 2, C3)
- `T3-OK` (Task 3, C12)
- `T4-OK` (Task 4, C5-C9)
- `FULL-GATE-OK` (Task 5): the plan's verify command, run as written on HEAD e78856f after teardown, passed on the first run (2026-11-04, ended 14:21Z): the C1-C12 SUMMARY lines, hygiene greps, the 261103-ell gate and sample-log checks, VERIFICATION and README greps, C10 greps, the live log checks, the six commit markers, goldens byte-identical to 579017a, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (96 test binaries, 3189 passed, 0 failed, 96 ignored), `cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings`, no Cargo change, no protected or planning-state file in a commit, no IP literal, no co-author line, no new ignored test, nothing staged, no rig container. The known wall-clock flake `wait_restart_witnessed_path_golden` did not trip, so no rerun was needed.

## Self-Check: PASSED

- Commits c7526fc, 65b80b7, af35aa4, b8f2ed5, 8bac082 and e78856f exist on `feat/deploy-tags-core`; nothing pushed.
- Live log, workspace log and gate output are in the session scratchpad (`261104-7cy-live.log`, `261104-7cy-workspace.log`, `261104-7cy-fullgate.out`).
