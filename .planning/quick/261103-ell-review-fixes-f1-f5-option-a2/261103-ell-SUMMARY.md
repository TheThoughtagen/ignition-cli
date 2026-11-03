---
phase: quick-261103-ell
plan: 01
subsystem: deploy
tags: [deploy, redundancy, rollback, probe, pair]
requires: [261102-cef, 261102-kus]
provides:
  - "torn pair run recovery reads .core/ pre-images from the snapshot core/ slot (F1, F7)"
  - "restore of created-only .core/ records without the core collection on every host (F4)"
  - "write_host probe refuses a symlinked, non-directory or unopenable core tag folder on every host (F5)"
  - "an empty (or unseen-content) mode tag type folder on a pair is residue at plan, apply and verify (F3)"
affects:
  - crates/ignition-core/src/deploy/apply/rollback.rs
  - crates/ignition-core/src/deploy/transport/
  - crates/ignition-core/src/deploy/plan.rs
  - crates/ignition-core/src/actions/deploy.rs
  - crates/ignition-core/src/deploy/apply/gates.rs
  - README.md
key-files:
  modified:
    - crates/ignition-core/src/deploy/apply/rollback.rs
    - crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs
    - crates/ignition-core/src/deploy/transport/local/apply_ops.rs
    - crates/ignition-core/src/deploy/transport/mod.rs
    - crates/ignition-core/src/deploy/transport/winps.rs
    - crates/ignition-core/src/deploy/transport/scripts/commit.sh
    - crates/ignition-core/src/deploy/transport/scripts/commit.ps1
    - crates/ignition-core/src/deploy/plan.rs
    - crates/ignition-core/src/deploy/apply/gates.rs
    - crates/ignition-core/src/deploy/apply/mod.rs
    - crates/ignition-core/src/actions/deploy.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-core/tests/deploy_apply.rs
    - crates/ignition-core/tests/deploy_host_conformance.rs
    - crates/ignition-core/tests/deploy_plan.rs
    - crates/ignition-core/tests/deploy_pair.rs
    - README.md
status: complete
actuals:
  tokens: 18064
  tasks: 5
  commits: 4
---

# Quick 261103-ell: review fixes F1-F5 (option A2)

Four code commits on `feat/deploy-tags-core` fix the code-review findings F1, F3, F4, F5 and the refactor F7: a torn pair run is recoverable with `ign deploy rollback --run`, a restore no longer strands the mode copies when `core` is gone, preflight refuses an unsafe core tag folder, and a pair whose mode collection holds an empty tag type folder is warned at plan and verify and refused at apply. F2 (verify fails on a mode copy of a declared unit) is **blocked**: the Docker pair rig did not link, so where the gateway writes an edit of an existing core tag was not established, and no F2 code landed.

## Findings

F1: fixed
F2: blocked
F3: fixed
F4: fixed
F5: fixed
F7: fixed, refactor, exempt from RED (orchestrator decision; behaviour already correct, covered by existing sftp staging tests)

### F1 detail

- RED (before any code change):
  - `deploy::apply::rollback::tests::snapshot_key_core_key_reads_the_core_slot`: `left: "files/.core/ignition/tag-definition/default/Deploy/tags.json"`, `right: "core/ignition/tag-definition/default/Deploy/tags.json"`.
  - `pair_core_torn_run_recovers_core_keys`: `rollback_failed`, "the snapshot is missing or damaged for 4 path(s): .core/ignition/tag-definition/default/Deploy/tags.json, ...Deploy/unary-resource.json, ...Old/tags.json, ...Old/unary-resource.json".
  - `pair_core_torn_run_recovers_core_keys_without_record`: `rollback_failed`, "the run's record is missing and the snapshot is incomplete: its files do not match the journal's pre-run fingerprint".
- Fix: `rollback.rs` `snapshot_key` maps every non-ledger key through `apply::snapshot_slot`, so a `.core/` pre-image is read from the snapshot's `core/` slot.
- Commit: a46488b (shared with F7).

### F7 detail

- sftp prepare's staging loop takes its slot from `crate::deploy::apply::snapshot_slot`; the inline match is gone. One slot rule in the crate.
- Commit: a46488b (the F1 commit).

### F2 detail

- Blocked, not built. See "F2 rig check" below. `pair_verify_warns_on_mode_tag_files` is unchanged, the README sentence on gateway tag writes is unchanged, and verify keeps today's warning for a mode copy of a declared unit.

### F3 detail

- RED:
  - `core_route::pair_mode_tag_dirs_names_only_folders_with_no_read_file` (`deploy_plan.rs`, pure): `left: []`, `right: ["ignition/tag-definition"]`.
  - `pair_tags_refusal_names_mode_type_folders` (lib, `gates.rs`): passes once the field is set and the combined list is used; RED shown by temporarily dropping the `keys.extend(plan.pair_mode_tag_dirs...)` line: it panicked at `.expect("refused")` (the old early return gave `None`). The line was restored before the commit.
  - `pair_core_empty_mode_type_folder_refused` (`deploy_apply.rs`): "the empty type folder is residue", with `prepare` returning `Ready { plan ... }` instead of the `deploy_pair_tags_refused` error.
  - `pair_core_empty_mode_type_folder_plan_warns`: `assertion left == right failed: []`, `left: 0`, `right: 1` (no pair warning naming the folder).
  - `pair_verify_warns_on_empty_mode_type_folder` (`deploy_pair.rs`): `left: 0`, `right: 1` (`pair_warnings` empty; the other warning present was the unrelated probes line).
  - `standalone_empty_mode_type_folder_not_refused` passed before the change and stays as a pin.
- Fix: `plan::pair_mode_tag_dirs` (pure rule), a pair-only `list_types(mode)` read in `deploy_plan` and `deploy_verify_planned`, a `#[serde(skip)]` `DeployPlanResult.pair_mode_tag_dirs` (not in `plan_sha256`, plan JSON key set unchanged), and `pair_tags_refusal`, `pair_tags_warning` and `pair_mode_tags_warning` testing the combined list (units or files plus folders) so folder-only residue refuses and warns. The `run_apply` guard reaches the refusal only through `pair_tags_refusal`. The `deploy_pair_tags_refused` text and hint, README and exit-table row 6 name the folder case. Slug and exit code unchanged.
- Commit: 757a155.

### F4 detail

- RED:
  - `conformance_local_core_restore_created_only_without_core_root`: `failure: Some("transport failed (restore) on nodes.conf: config/resources/core does not exist; ...")`.
  - `conformance_sh_core_restore_created_only_without_core_root`: `failure: Some("the core collection does not exist")` (assertion `restored.failure.is_none()` in part (a), after `config/resources/core` was removed).
  - The ps row compiles under xwin clippy and runs only on the Windows CI leg.
- Fix: local `restore_blocking` opens core strictly only when a `.core/` restore carries a pre-image, otherwise `core_collection_if_present` (missing gives `None`, symlink or non-directory still refused). `commit.sh` `op_restore` and `commit.ps1` `OpRestore` fail only for a `.core/` pre-image record, and for a core that exists but is not a directory. Created-only records are `Removed`, the mode pre-images are put back, no host creates `core`.
- Commit: 6b39e5d.

### F5 detail

- RED:
  - `conformance_local_core_probe_refuses_unsafe_type_folder` and the sh row: `a file at the type folder: ProbeReport { writable: true, problems: [] }`.
  - `deploy::transport::tests::core_probe_problem_names_an_unsafe_type_folder`: `expect("a text")` on `None`.
  - `deploy::transport::sftp::apply_ops::tests::core_key_probe_codes_name_the_folder`: `core_tags_unsafe:tag-definition: the host reported a problem this build does not know`.
  - `deploy::transport::winps::tests::both_scripts_name_the_core_root_and_its_floor`: failed on the new `core_tags_unsafe:` pin.
- Fix: the probe walks `core/ignition/<type>`, then `core/ignition`, stopping at the first level that exists (a directory: writable and searchable, else `core_tags_unwritable:<type>`; a symlink or other: `core_tags_unsafe:<type>`, never the parent). Local `probe_paths_blocking`, `commit.sh` `op_probe`, `commit.ps1` `OpProbe`, and `transport::core_probe_problem` carries the new text.
- Commit: 4e7428c.

## Commits

- a46488b fix(261103-ell): recover a torn pair run's core keys from the core snapshot slot (F1, F7)
- 6b39e5d fix(261103-ell): restore created-only core records without the core collection (F4)
- 4e7428c fix(261103-ell): refuse a symlinked or non-directory core tag folder at the write_host probe (F5)
- 757a155 fix(261103-ell): report an empty mode tag type folder on a pair as residue (F3)

## F3 scope and open item

`TreeReader::read` returns files only, and `list_types(mode)` lists `<module>/<type>` directories and nothing below the type folder. The F3 rule therefore covers a tag type folder that is listed while the plan read no file under it: empty, holding only empty folders, or holding only files of tag providers outside the read scope. It does **not** cover a type folder that holds in-scope files the migration deletes **plus** such other content: seeing that before the commit needs a recursive directory listing in every reader (a new `TreeReader` method on the local and sftp readers and three test readers). That case is an open item, not built. After such an apply the folder holds no read file, so the post-apply `deploy verify` reports it in its pair warning.

## F2 rig check

- Docker: orbstack, answered at 2026-11-03T18:29:48Z. No containers of compose project `ign-pair-rig` existed beforehand.
- Commands, each under `timeout 900 /bin/bash ./setup-pair.sh <step>` run from `crates/ignition-core/tests/fixtures/live-rig-pair` (environment: the rig's own state directory; tokens were never printed or logged):
  - `up`: begun 2026-11-03T18:29:53Z, exit 0 at 18:30:48Z. Both gateway containers healthy.
  - `mint`: begun 18:30:48Z, exit 0 at 18:31:36Z.
  - `pair`: begun 18:31:36Z, exit 1 at 18:37:24Z with `pair not linked within 300 s`.
- `status` at 18:37:43Z: the first gateway read Master, Active, peerConnected false; the second read Backup, Active, peerConnected false. The pair never linked, so no settled pair existed for the probe tags (steps 3-7 of the check) and the live suite.
- Per the Task 3a rule the verdict is `blocked`: no retry under another shell, no `mode` step, and no kill of any process. Teardown ran at 18:37:48Z with `docker compose -p ign-pair-rig down -v` (exit 0); no `ign-pair-rig` container remains and no `F2Probe` folder was ever created.
- Evidence log: the scratch rig log in the session scratchpad (`261103-ell-f2-rig.log`), one verdict line.

F2 rig verdict: blocked

Open item for the user: where the gateway writes an edit of an existing core tag was not established. Today's README sentence ("The gateway's own tag writes (Designer, `tags/import`) go to `core` even in a deployment mode, so they are not affected.") rests on the create case only (debug control C3). F2 is therefore neither built nor ruled out: verify keeps its warning for a mode copy of a declared tag unit. If a later rig run shows the edit lands in the mode collection, a Designer edit on a pair would re-wedge the backup and conflict with cef D2 ("Designer edits to ign-managed core tag folders are drift"); if it shows `core`, F2 can be built as specified. A fresh pair rig run (the `pair` step linking) is needed either way.

## Live

Skipped, as the plan allows on a `blocked` verdict: no settled 8.3.6 pair was available (the `pair` step did not link), so the eight `live_deploy_pair` gates were not run and F3 and the other changes were not proven against a real pair in this task. The offline suites (engine, pair verify and conformance rows) cover them. The orchestrator decides whether to rerun the live suite on a rebuilt rig.

## Gate tokens

- `T1-OK` (Task 1, F1 + F7)
- `T2-OK` (Task 2, F4 + F5, includes xwin clippy for windows-msvc)
- `T3A-OK` (Task 3a, no repo file changed, no probe folder, rig torn down)
- `T3B-OK` (Task 3b, F3)
- `FULL-GATE-OK` (Task 3c)

## Deviations from Plan

### Accepted by the orchestrator

**1. [Task 2, F4] Restore tolerates a missing core collection on all three hosts**
- **Found during:** Task 2.
- **Issue:** the plan words the script rule as "missing or non-directory" but the local rule as "refuse a non-directory".
- **Decision (orchestrator, accepted):** restore on the local host, `commit.sh` and `commit.ps1` tolerates a missing core collection for created-only `.core/` records and still fails when core exists but is not a real directory; a `.core/` record with a pre-image still needs core.
- **Commit:** 6b39e5d.

### Other

**2. [Task 3a] Rig did not link; verdict `blocked`**
- The `pair` step exited 1 (`pair not linked within 300 s`). Handled exactly as the plan directs: verdict `blocked`, teardown, no F2 code, README sentence unchanged, live suite skipped. F2 and the live run are the open items above.

**3. [Task 3b] Commit before the gate**
- The T3B gate also checks that the F3 commit exists in the log, so the F3 commit was made first and the gate run on the committed tree (as for Task 2). Nothing changed between the two.

**4. [Task 3b] Gates unit test written after the gate change**
- `pair_tags_refusal_names_mode_type_folders` is fed the field directly, so it does not depend on the plan-side rule. Its RED was shown by temporarily removing the combined-list line (see F3 detail).

## Notes

- The `conformance_ps_*` rows compile under xwin clippy for windows-msvc and run only on the Windows CI leg.
- No Cargo.toml or Cargo.lock change, no slug or exit-code change, and no golden line changed (diff-based check against the base commit 9a2e268).
- Nothing pushed, no PR, no worktree, no production call. The protected local files were never staged.

## Hygiene

No IP-shaped literal beyond loopback, no private-range prefix and no production host name appear in this file or in the committed diff; hosts are described by role only.
