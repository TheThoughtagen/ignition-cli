# Quick 261103-ell: interim progress (Tasks 1 and 2)

Interim note for the Task 3c executor, which builds the SUMMARY from it. Not the SUMMARY.
Base 9a2e268, branch feat/deploy-tags-core. Commits are code + tests (+ README for F4/F5) only.

## F1: reproduced, fixed
- RED (before any code change):
  - `deploy::apply::rollback::tests::snapshot_key_core_key_reads_the_core_slot` failed:
    `left: "files/.core/ignition/tag-definition/default/Deploy/tags.json"`,
    `right: "core/ignition/tag-definition/default/Deploy/tags.json"`.
  - `pair_core_torn_run_recovers_core_keys` failed: `rollback_failed`, "the snapshot is missing or damaged
    for 4 path(s): .core/ignition/tag-definition/default/Deploy/tags.json, ...Deploy/unary-resource.json,
    ...Old/tags.json, ...Old/unary-resource.json".
  - `pair_core_torn_run_recovers_core_keys_without_record` failed: `rollback_failed`, "the run's record is missing
    and the snapshot is incomplete: its files do not match the journal's pre-run fingerprint".
- Fix: `rollback.rs` `snapshot_key` maps every non-ledger key through `apply::snapshot_slot`.
- Test notes: both engine tests share `torn_core_run_recovers(lose_record)`; `kill_apply` now delegates to a new
  `kill_apply_with(rig, notify, flags)` (behaviour unchanged for existing callers).
- Commit: a46488b (shared with F7).
- Gate: `T1-OK`.

## F7: fixed, refactor, exempt from RED
`F7: fixed, refactor, exempt from RED (orchestrator decision; behaviour already correct, covered by existing sftp staging tests)`
- sftp prepare's staging loop takes its slot from `crate::deploy::apply::snapshot_slot`; the inline match is gone.
  `split_key` / `KeyCollection` imports are still used elsewhere in the file, so nothing was left unused.
- Commit: a46488b (shared with F1).

## F4: reproduced, fixed
- RED: `conformance_local_core_restore_created_only_without_core_root` failed with
  `failure: Some("transport failed (restore) on nodes.conf: config/resources/core does not exist; ...")`;
  `conformance_sh_core_restore_created_only_without_core_root` failed with `failure: Some("the core collection does not exist")`
  (assertion `restored.failure.is_none()` in part (a), after `config/resources/core` was removed).
  The ps row compiles under xwin clippy and runs only on the Windows CI leg.
- Fix: local `restore_blocking` opens core strictly only when a `.core/` restore carries a pre-image, otherwise
  `core_collection_if_present` (missing -> `None`, symlink/non-dir still refused); `commit.sh` `op_restore`
  (`_needs_core`) and `commit.ps1` `OpRestore` (`$needsCore`) fail only for a `.core/` pre-image record, and for a
  core that exists but is not a directory.
- Interpretation: the plan says "missing or non-directory" for the scripts but "refuse a non-directory" for local;
  all three hosts now fail a core that exists and is not a real directory, and tolerate only a missing one.
- Test note: part (b) removes the first run's restoring journal (`journal.json`) before applying the second run.
- README: Deletes and rollbacks bullet.
- Commit: 6b39e5d.
- Gate: F4 subset green (conformance 68 passed, lib `transport::` 232 passed, `sh -n`, pwsh parse OK, workspace clippy).

## F5: reproduced, fixed
- RED:
  - `conformance_local_core_probe_refuses_unsafe_type_folder` and `conformance_sh_...`:
    `a file at the type folder: ProbeReport { writable: true, problems: [] }`.
  - `deploy::transport::tests::core_probe_problem_names_an_unsafe_type_folder` failed (`expect("a text")` on `None`).
  - `deploy::transport::sftp::apply_ops::tests::core_key_probe_codes_name_the_folder` failed:
    `core_tags_unsafe:tag-definition: the host reported a problem this build does not know`.
  - `deploy::transport::winps::tests::both_scripts_name_the_core_root_and_its_floor` failed on the new
    `core_tags_unsafe:` pin.
- Fix: probe walks `core/ignition/<type>`, then `core/ignition`, stopping at the first level that exists
  (missing: up; dir: writable and searchable else `core_tags_unwritable:<type>`; symlink/other: `core_tags_unsafe:<type>`,
  never the parent; neither: the core root decides). Local `probe_paths_blocking`, `commit.sh` `op_probe`,
  `commit.ps1` `OpProbe`; `transport::core_probe_problem` has the new text and updated doc.
- Test notes: the sh script must contain the literal `core_tags_unsafe:` (structural test), so `op_probe` adds it
  by literal, not via a variable. Row cases (a) file at type folder, (b) file at `core/ignition`, (c) symlink,
  (d) modes 0o000 and 0o200 (skipped as root).
- README: Host access bullet.
- Commit: 4e7428c.
- Gate: `T2-OK` (full verbatim run after the F5 commit; includes xwin clippy for windows-msvc).

## Hygiene
- No Cargo.toml / Cargo.lock change; no golden changed; no 64-hex or IP-shaped literal added in the diff
  `9a2e268..HEAD`.
- `.planning/config.json`, `.planning/research/.cache/`, the `svc-ignition-deploy@...ps1` file and
  `.planning/debug/tg-pg-table-check.md` were never staged. No docs artifacts committed.
- Not pushed; no PR; no production calls.
- Open items for Task 3: none from Tasks 1-2. Findings that did not reproduce: none.
