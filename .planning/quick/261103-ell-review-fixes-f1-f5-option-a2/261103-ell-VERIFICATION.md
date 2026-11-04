---
phase: quick-261103-ell
verified: 2026-11-03T20:15:00Z
re_verified: 2026-11-04
status: human_needed
score: 8/8 must-haves verified (F2 built and verified on the `core` branch; only the Windows `conformance_ps_*` rows remain unexecuted)
behavior_unverified: 0
overrides_applied: 0
human_verification:
  - test: "Run the Windows CI leg (conformance_ps_core_restore_created_only_without_core_root, conformance_ps_core_probe_refuses_unsafe_type_folder)"
    expected: "Both ps rows pass; commit.ps1 OpRestore / OpProbe match the sh and local hosts"
    why_human: "The rows are cfg(windows). Here they were only reviewed (diff read) and compiled under xwin clippy for windows-msvc; they were not executed."
---

# Quick 261103-ell: Verification Report

**Goal:** Fix code-review findings F1, F3, F4, F5 (+ F7) on `feat/deploy-tags-core`; F2 gated on a Docker pair-rig check.
**Base:** 9a2e268. **Commits:** a46488b (F1+F7), 6b39e5d (F4), 4e7428c (F5), 757a155 (F3), b5579ec (F2).
**Status:** human_needed. No gaps; only the Windows CI rows cannot be settled from this checkout.
**Re-verification:** Yes (2026-11-04, quick 261104-7cy C2): records the final state after the retried rig, the F2 commit, the live suite and the full gate.

## Must-have truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | F1: torn pair run recovers `.core/` keys, with and without record; reads `core/` slot | VERIFIED | `rollback.rs` `snapshot_key` now returns `super::snapshot_slot(path)` for non-ledger keys. Unit test `snapshot_key_core_key_reads_the_core_slot` asserts core -> `core/...`, mode -> `files/...`, ledger unchanged. Engine tests `pair_core_torn_run_recovers_core_keys` and `..._without_record` (shared `torn_core_run_recovers`) kill a real apply mid-write, assert `RollbackKind::Restored`, `core_state == before`, mode files and ledger restored, journal gone, `core/ignition/tag-definition` kept; the record-lost variant deletes `record.json`. Not trivially passing. All pass. |
| 2 | F7: sftp prepare uses `apply::snapshot_slot` | VERIFIED | `sftp/apply_ops.rs` diff replaces the inline `match split_key` with `crate::deploy::apply::snapshot_slot(op.path.as_str())`. Exempt from RED per plan. |
| 3 | F4: created-only `.core/` restore succeeds with core missing on local, sh, ps; pre-image record still fails; symlink/non-dir core still refused | VERIFIED (ps unexecuted) | Local `restore_blocking`: strict `core_for` only if a `.core/` record has a pre-image, else `core_collection_if_present` (missing -> None; non-dir/symlink still refused via `core_collection`). `commit.sh` `_needs_core` and `commit.ps1` `$needsCore` do the same, and fail "not a directory" when core exists but is not a directory. Conformance row `core_restore_created_only_without_core_root` asserts (a) no failure, core records in `deleted`, mode copy back, core never created, second restore idempotent, and (b) a pre-image record still fails naming core. local and sh rows pass; ps row cfg(windows). |
| 4 | F5: probe refuses symlinked / non-dir / unopenable `core/ignition/<type>` or `core/ignition`, names folder, never probes parent | VERIFIED (ps unexecuted) | Local `probe_paths_blocking` uses `kind()`: first existing level decides, Missing goes up, Dir must open and be writable, anything else -> `core_tags_unsafe:<ty>`. `commit.sh` and `commit.ps1` same logic (sh adds `-x`). `transport::core_probe_problem` has the text; sftp `probe_problem` test and `winps` literal pin cover it. Conformance row covers a file at the type folder, a file at `core/ignition`, a symlink (and asserts nothing written through the link), and modes 0o000/0o200 (skipped as root). local and sh pass. Quick 261104-7cy C5-C9 later widened this to a top-down walk of every folder on a touched path. |
| 5 | F3: empty/unseen-content mode type folder is residue at plan (warning), apply (refusal, gate and engine guard), verify (warning); standalone, `plan_sha256`, JSON key set unchanged | VERIFIED | `plan::pair_mode_tag_dirs` (pure), pair-only `list_types(mode)` in `deploy_plan` and `deploy_verify_planned`, `#[serde(skip)] DeployPlanResult.pair_mode_tag_dirs`, `pair_tags_refusal`/`pair_tags_warning`/`pair_mode_tags_warning` test the combined list. `run_apply` guard calls `gates::pair_tags_refusal`. Tests: `pair_core_empty_mode_type_folder_refused`, `..._plan_warns`, `standalone_empty_mode_type_folder_not_refused`, `pair_verify_warns_on_empty_mode_type_folder`, unit `pair_tags_refusal_names_mode_type_folders`, pure `pair_mode_tag_dirs_names_only_folders_with_no_read_file`. All pass. |
| 6 | F2: rig verdict recorded; on `core` a declared unit's mode copy fails the master `files` check | VERIFIED (`core` branch) | The retried rig linked and the SUMMARY carries `F2 rig verdict: core` (scratch log: the first attempt's `blocked` line, then `F2 rig verdict (retry): core`). The probe showed an edit of an existing core tag through `tags/import` landing in `core` on both nodes, with no mode path created. F2 fixed in b5579ec: `verify::ObservedState.mode_copies` and a `files_differ` finding in `check_files`. Tests: lib `files_check_fails_a_declared_unit_with_a_mode_copy`, `pair_verify_fails_on_mode_copy_of_declared_unit` (re-run 2026-11-04: ok), and `pair_verify_warns_on_mode_tag_files` retargeted to an undeclared unit's mode copy. |
| 7 | README gateway-tag-write sentence matches verdict (`core`) | VERIFIED | README carries "both a create and an edit of an existing core tag" (the gateway's own tag writes go to `core` even in a deployment mode), changed in b5579ec. |
| 8 | No slug/exit change, no Cargo change, goldens identical, gates green, SUMMARY hygiene | VERIFIED | See below. |

## Gates run

| Check | Result |
|-------|--------|
| `cargo fmt --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | Finished, no warnings |
| Named tests (F1-F5 and F2 rows) | all pass |
| `cargo test --workspace` | every `test result` line ok, 0 failed |
| xwin windows-msvc clippy | ran, clean (ps rows compile; not executed) |
| Live pair suite (eight `live_deploy_pair` gates, Docker 8.3.6 pair rig, loopback URLs) | `test result: ok. 8 passed`, each gate printed `LIVE-RAN <gate>` and `... ok` |
| Full gate | `FULL-GATE-OK` printed. The gate read the first verdict line and used a quiet grep on a pipe, so it was run as a corrected copy (recorded as deviation 5 in the SUMMARY); quick 261104-7cy C1 fixed the gate in the PLAN itself |

## Hygiene

- Golden check: `git diff -U0 9a2e268` on `tests/deploy_plan.rs`, `apply/fingerprint.rs`, `ignition-cli/tests/contract_deploy.rs`, filtered for GOLDEN / 64-hex lines: no match. Only `pair_mode_tag_dirs: Vec::new()` field initialisers and the new tests were added in `deploy_plan.rs`.
- No `Cargo.toml` / `Cargo.lock` in the diff stat.
- Commits since 9a2e268 touch no protected local file; config.json, `research/.cache/`, the `svc-ignition-deploy@...ps1` file and `tg-pg-table-check.md` are still working-tree only.
- No Co-Authored-By in any commit message. Author pmannion2.
- No IP-shaped literal or VM-style host name in added diff lines or in SUMMARY.md (regex for dotted quads, private-range prefixes and VM-style names returned nothing).
- Slugs and exit codes: `deploy_pair_tags_refused` / exit 6 unchanged; only its text, hint and README row wording changed.
- Stub scan on added lines (TODO, FIXME, XXX, TBD, HACK, todo!, unimplemented!, #[ignore]): none.

## Anti-patterns

None found. The `#[serde(skip)]` on `pair_mode_tag_dirs` is intentional (documented) and the field is not deserialised anywhere; both `DeployPlanResult` constructors outside tests (`deploy_plan`) populate it.

## Human verification required

1. **Windows CI leg.** `conformance_ps_core_restore_created_only_without_core_root` and `conformance_ps_core_probe_refuses_unsafe_type_folder` (F4, F5) and the `commit.ps1` changes were reviewed by diff and compiled under xwin clippy only. Logic mirrors the sh script, but it has not run here.

Note: the F3 open item (a type folder holding in-scope files the apply deletes plus content the plan did not read was not seen before the commit) is closed by quick 261104-7cy C3 (65b80b7): a full `TreeReader::list_tree` listing makes any unread file or folder in a pair's mode tag type folder residue, refused before any write.

## The `core` branch

The rig verdict was `core`, so Task 3c built F2 (b5579ec), set the README gateway-write sentence to the create and edit case, and ran the live pair suite, which passed eight of eight on HEAD b5579ec: the migration and core-unit gates verify green after their applies, so F2 does not false-fail and F3 does not false-refuse on a real pair. The F3, F4 and F5 README wording matches the code.

## Gaps

None.
