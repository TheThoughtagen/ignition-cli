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
  - "on a pair, a mode-collection copy of a declared tag unit fails the master verify files check (F2)"
affects:
  - crates/ignition-core/src/deploy/apply/rollback.rs
  - crates/ignition-core/src/deploy/transport/
  - crates/ignition-core/src/deploy/plan.rs
  - crates/ignition-core/src/actions/deploy.rs
  - crates/ignition-core/src/deploy/apply/gates.rs
  - crates/ignition-core/src/deploy/verify.rs
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
    - crates/ignition-core/src/deploy/verify.rs
    - crates/ignition-core/tests/deploy_verify.rs
    - README.md
status: complete
actuals:
  tokens: 22568
  tasks: 5
  commits: 5
---

# Quick 261103-ell: review fixes F1-F5 (option A2)

Five code commits on `feat/deploy-tags-core` fix the code-review findings F1, F2, F3, F4, F5 and the refactor F7: a torn pair run is recoverable with `ign deploy rollback --run`, a restore no longer strands the mode copies when `core` is gone, preflight refuses an unsafe core tag folder, a pair whose mode collection holds an empty tag type folder is warned at plan and verify and refused at apply, and a pair whose master holds a mode-collection copy of a declared tag unit now fails the `files` check of `deploy verify`. F2 was built after a retried Docker 8.3.6 pair rig linked and showed that a gateway edit of an existing core tag stays in `core` (verdict `core`); the eight live pair gates then passed on that rig.

## Findings

F1: fixed
F2: fixed
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

- Built on the retried rig check's `core` verdict (see "F2 rig check" below).
- RED (the new `ObservedState.mode_copies` field was added and filled empty first, so the tests compiled and failed at run time):
  - `deploy::verify::tests::files_check_fails_a_declared_unit_with_a_mode_copy` (lib): `assertion left == right failed: Check { id: Files, status: Pass, detail: "1 declared tag unit(s) compared byte for byte with the live files", findings: [] }`, `left: Pass`, `right: Fail`.
  - `pair_verify_fails_on_mode_copy_of_declared_unit` (`deploy_pair.rs`): panicked at `assert!(!master.ok)`: the master verify was green with the canary's mode copy present.
  - `pair_verify_warns_on_mode_tag_files`, retargeted to an undeclared unit's mode copy (`ignition/tag-definition/default/Other/`), passed before and after (its `files` check stays `pass` and its one warning names the `Other` paths).
- Fix: `verify::ObservedState` gains `mode_copies` (declared tag unit key to its mode-collection paths, filled only on a pair). `check_files` turns a non-empty mode copy into one `files_differ` finding whose detail names the mode collection, the count and the first ten paths and says the copy overrides `core` and blocks the backup's configuration sync; it replaces the unit's status line. The module table row 4 and the `check_files` doc say so. `deploy_verify_planned` builds the map on a pair from the declared entries' bare rows with a live sha; `pair_mode_tags_warning` now takes only the other mode tag files plus the F3 folders, and its empty-return guard still tests the combined list. Standalone passes an empty map. The `deploy_verify` and `deploy_verify_node` docs and the README `deploy verify` bullet describe the finding; `--node both` fails the master `files` check the same way and leaves the backup checks as they were.
- README: the gateway-tag-write sentence now says the gateway's own tag writes go to `core` even in a deployment mode, both a create and an edit of an existing core tag (debug C3 and this rig check), so they are not affected.
- GREEN: the lib test and `--test deploy_pair --test deploy_verify --test deploy_apply --test deploy_plan` (168, 12, 87 and 107 passed) and workspace clippy `-D warnings` were green before the commit.
- Commit: b5579ec.

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
- b5579ec fix(261103-ell): fail verify on a mode-collection copy of a declared tag unit on a pair (F2)

## F3 scope and open item

`TreeReader::read` returns files only, and `list_types(mode)` lists `<module>/<type>` directories and nothing below the type folder. The F3 rule therefore covers a tag type folder that is listed while the plan read no file under it: empty, holding only empty folders, or holding only files of tag providers outside the read scope. It does **not** cover a type folder that holds in-scope files the migration deletes **plus** such other content: seeing that before the commit needs a recursive directory listing in every reader (a new `TreeReader` method on the local and sftp readers and three test readers). That case is an open item, not built. After such an apply the folder holds no read file, so the post-apply `deploy verify` reports it in its pair warning.

## F2 rig check

### First attempt (blocked)

- Docker: orbstack, answered at 2026-11-03T18:29:48Z. No containers of compose project `ign-pair-rig` existed beforehand.
- Each step ran under `timeout 900 /bin/bash ./setup-pair.sh <step>` from `crates/ignition-core/tests/fixtures/live-rig-pair` (environment: the rig's own state directory; tokens were never printed or logged): `up` 18:29:53Z to 18:30:48Z exit 0, `mint` to 18:31:36Z exit 0, `pair` to 18:37:24Z exit 1 with `pair not linked within 300 s`.
- `status` at 18:37:43Z: gw-a Master Active, gw-b Backup **Active**, both peerConnected false. Torn down at 18:37:48Z (`docker compose -p ign-pair-rig down -v`, exit 0). That attempt recorded a `blocked` verdict in the scratch log; no probe folder was created.

### Rig diagnosis

- The retry used the same invocation (`/bin/bash`, `timeout 900`, a fresh state directory after `down -v`, no stale volume, no leftover `setup-pair.sh` process) and `pair` linked after 67 s. No repo file was changed for the rig.
- The gateway logs of the linking run show the sequence `pair` depends on: gw-b's first GAN connect fails `SSLHandshakeException: No approved certificate could be found` for gw-a's GAN certificate; once `trust_gan` moves the rejected certificates to trusted, gw-b logs "Restarting outgoing TLS client connections since issuer / trusted certs have changed" and "Channel connection ... temporarily going down. After 30 seconds, the connection will be re-established. If the connection fails, this node will become active"; the next attempts fail `certificate_unknown`, then `403 Forbidden` on the websocket upgrade (pending approval); gw-a's incoming connection is then listed and approved, gw-b goes Backup Cold, logs "Connection established - requesting peer certificate sync", and peerConnected turns true.
- The failed attempt's end state (gw-b Backup **Active**, peerConnected false) is the "this node will become active" branch of that 30 s window: the backup's restarted channel did not get a connection in time and the backup promoted itself, and the trust/approve loop then never relinked it within 300 s. It is a timing race in the gateway's trust-change restart, not a licence, image or script error: no licence, certificate-store or redundancy-config error was logged beyond the expected handshake steps, and the cef session's recovery from a stuck `pair` was the same (`docker compose -p ign-pair-rig restart gw-b` and `setup-pair.sh link`). The race did not recur in this session, so it was not reproduced; if it does, that recovery is the fix.
- Not related to the bash 5.3 deadlock (cef): every step ran under `/bin/bash` 3.2 and exited on its own.

### Retry (verdict core)

- 2026-11-03T19:34:47Z `up` (exit 0, 19:35:54Z), `mint` (exit 0, 19:36:54Z), `pair` (exit 0, 19:39:34Z, linked after 67 s), `mode` (exit 0, 19:42:32Z; mode definition synced in 3 s, relinked 39 s after the gw-b recreate and 12 s after the gw-a recreate, settled at once), each under `timeout 900 /bin/bash ./setup-pair.sh <step>`.
- `status` at 19:43:03Z: gw-a Master Active, gw-b Backup Cold, both peerConnected true, syncPending false, Good, mode production.
- Probe (the env file `tokens.env` sourced; the token went only into a request header):
  - 19:43:03Z wrote `core/ignition/tag-definition/default/F2Probe/tags.json` (the live gate's canary tag, named `Probe`) and `unary-resource.json` (the canary marker) into gw-a's data dir; `POST /data/api/v1/scan/config` HTTP 200 (19:43:04Z). The export showed `F2Probe` at 19:43:07Z and gw-b's core held `F2Probe/tags.json` by then.
  - Pre-state: `production/ignition/tag-definition` absent on gw-a; the listing held only `core/ignition/tag-definition/{default,System}` paths plus `F2Probe/tags.json` and `F2Probe/unary-resource.json`. Hashes: `tags.json` 072dbc3b3e60... on both nodes; `unary-resource.json` e7b0843bee83... on gw-a, 8bf62a66d6a0... on gw-b (the backup pretty-prints the marker, debug C1).
  - 19:43:07Z `POST /data/api/v1/tags/import?provider=default&type=json&collisionPolicy=Overwrite` (`application/octet-stream`, `Probe` with `documentation` changed): HTTP 200, `successCount` 2, `failureCount` 0. The export showed the edit at once.
  - Post-state at 19:43:12Z: the same listing on both nodes (no new path), gw-a core `F2Probe/tags.json` 0a439fb39306... (changed), gw-b core `F2Probe/tags.json` 0a439fb39306... (synced), both markers 8bf62a66d6a0... (the gateway rewrote gw-a's marker in `core` too), and `production/ignition/tag-definition` still absent.
  - The first round decided, so the `F2Probe2` control round (C3) was not needed.
- Cleanup: 19:43:24Z removed `F2Probe` from gw-a's core and scanned (HTTP 200); no mode path existed, so no resync. By 19:43:29Z no `F2Probe` path remained on either node and the pair read settled. The rig stayed up for the live suite.
- Evidence log: the scratch rig log in the session scratchpad (`261103-ell-f2-rig.log`). It holds the first attempt's verdict line and, appended after it, the retry's line `F2 rig verdict (retry): core`.

F2 rig verdict: core

## Live

- Rig: the retried rig, settled at 19:43:29Z and again before the run.
- Command (from the repo root, `tokens.env` sourced, tokens never printed): `IGNITION_LIVE_PAIR=1 IGN_PAIR_URL_A=http://localhost:18300 IGN_PAIR_URL_B=http://localhost:18301 IGN_PAIR_DATA_A=$HOME/.cache/ign-pair-rig/data-a IGN_PAIR_DATA_B=$HOME/.cache/ign-pair-rig/data-b PATH=<scratchpad>/sysbash:$PATH cargo test -p ignition-core --test live_deploy_pair -- --ignored --show-output --test-threads=1`, under a `timeout 5400` watchdog, teed to `261103-ell-live.log` in the scratchpad.
- Began 2026-11-03T20:00:02Z, ended 20:06:42Z, exit 0, on HEAD b5579ec (F2 built).
- `test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 389.29s`
- Each of the eight gates printed `LIVE-RAN <gate>` and `test <gate> ... ok`: `live_pair_converges`, `live_pair_converges_with_a_stale_signature`, `live_pair_core_migration_converges`, `live_pair_core_tag_units_converge`, `live_pair_refuses_a_tag_unit`, `live_pair_secret_names`, `live_pair_tag_cleanup_converges`, `live_pair_unsynced_then_reconverge`. So on a real 8.3.6 pair F3 does not false-refuse and F2 does not false-fail (the migration and core-unit gates verify green after their applies).
- `status` before teardown: gw-a Master Active, gw-b Backup Cold, both peerConnected true, Good, mode production. Torn down with `docker compose -p ign-pair-rig down -v` (exit 0); no `ign-pair-rig` container remains.

Live verdict: pass

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

**2. [Task 3a] First rig attempt did not link; retried on the orchestrator's instruction**
- The first `pair` step exited 1 (`pair not linked within 300 s`) and was handled as the plan directs (verdict `blocked`, teardown). The orchestrator then relayed the user's choice to retry the rig, settle F2 and run the live gates; the retry linked with the same invocation (diagnosis above), the verdict was `core`, and Task 3c's `core` branch was carried out.
- The Task 3a rule "never retry under another shell" was not broken: the retry used the same `/bin/bash` invocation, on the relayed user decision. Three rig attempts were allowed; one was needed.

**5. [Task 3c] Verdict line form and the full gate**
- On the orchestrator's instruction the retry verdict was appended to the scratch log as `F2 rig verdict (retry): core`, below the first attempt's line, rather than replacing it. The plan's full-gate command reads the verdict from the `F2 rig verdict: ` line, which is still the first attempt's `blocked`; it was therefore run as a copy that reads `F2 rig verdict (retry): ` instead (and tees the workspace log to a new file). Every other check of the command is unchanged but one: on the `core` branch, `git log --format=%s 9a2e268..HEAD | grep -qE '^fix\(261103-ell\): .*\(F2\)$'` exits 141 under `set -o pipefail` because the F2 commit is HEAD, `grep -q` stops at the first line and `git log` takes SIGPIPE; the copy uses `grep -E ... > /dev/null` there (same match, reads all input). This SUMMARY carries the exact line `F2 rig verdict: core` that the gate matches.
- `FULL-GATE-OK` was printed on the second full run (2026-11-03, workspace: 96 test binaries, 3176 passed, 0 failed, 96 ignored; fmt, workspace clippy and windows-msvc xwin clippy clean). The first full run failed only `wait_restart_witnessed_path_golden` (`crates/ignition-cli/tests/contract_restart_wait.rs`, a wall-clock assertion `< 5 s` that read 7.54 s under the parallel workspace load). That file is untouched since phase 02; the test passed alone (7 of 7) and in the second full run. A pre-existing timing flake, not caused by this task.

**6. [Task 3c] F2 RED by scaffolding**
- The `mode_copies` field was added to `ObservedState` and filled with an empty map at every literal before the tests ran, so the RED failures are run-time assertion failures rather than a compile error. The scaffolding and the fix are in the one F2 commit, as the plan specifies.

**3. [Task 3b] Commit before the gate**
- The T3B gate also checks that the F3 commit exists in the log, so the F3 commit was made first and the gate run on the committed tree (as for Task 2). Nothing changed between the two.

**4. [Task 3b] Gates unit test written after the gate change**
- `pair_tags_refusal_names_mode_type_folders` is fed the field directly, so it does not depend on the plan-side rule. Its RED was shown by temporarily removing the combined-list line (see F3 detail).

## Notes

- The `conformance_ps_*` rows compile under xwin clippy for windows-msvc and run only on the Windows CI leg.
- Out of scope, seen in the rig logs: gw-a logs `Audit profile ... not found` for each API call, because the security-properties fixture that `setup-pair.sh mint` copies from `live-rig-deploy` names an audit profile after a production host. It is harmless on the rig (pre-existing, not changed here), but the fixture carries a production host name; worth a follow-up.
- No Cargo.toml or Cargo.lock change, no slug or exit-code change, and no golden line changed (diff-based check against the base commit 9a2e268).
- Nothing pushed, no PR, no worktree, no production call. The protected local files were never staged.

## Hygiene

No IP-shaped literal beyond loopback, no private-range prefix and no production host name appear in this file or in the committed diff; hosts are described by role only.
