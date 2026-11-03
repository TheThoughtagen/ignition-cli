---
phase: quick-261103-ell
verified: 2026-11-03T20:15:00Z
status: human_needed
score: 8/8 must-haves verified in code (F2 and the README sentence verified as the `blocked` branch)
behavior_unverified: 0
overrides_applied: 0
human_verification:
  - test: "Run the Windows CI leg (conformance_ps_core_restore_created_only_without_core_root, conformance_ps_core_probe_refuses_unsafe_type_folder)"
    expected: "Both ps rows pass; commit.ps1 OpRestore / OpProbe match the sh and local hosts"
    why_human: "The rows are cfg(windows). Here they were only reviewed (diff read) and the SUMMARY says they compile under xwin clippy; they were not executed."
  - test: "Rebuild the Docker pair rig until the pair links, then run the F2 edit-location check and the eight live_deploy_pair gates"
    expected: "A rig verdict of core|mode|inconclusive; F3 and the F4/F5 changes behave against a real 8.3.6 pair"
    why_human: "Rig verdict was `blocked` (pair not linked within 300 s); live suite skipped. Where the gateway writes an edit of an existing core tag is still unknown."
  - test: "Decide whether the F3 open item matters: an apply that deletes in-scope files from a type folder that also holds out-of-scope content is not refused before the commit"
    expected: "Accept as documented (verify names the folder after the apply) or schedule a recursive TreeReader listing"
    why_human: "Design call; documented in SUMMARY and README, not a defect against the plan."
---

# Quick 261103-ell: Verification Report

**Goal:** Fix code-review findings F1, F3, F4, F5 (+ F7) on `feat/deploy-tags-core`; F2 gated on a Docker pair-rig check.
**Base:** 9a2e268. **Commits:** a46488b (F1+F7), 6b39e5d (F4), 4e7428c (F5), 757a155 (F3).
**Status:** human_needed. No gaps; the remaining items cannot be settled from this checkout.
**Re-verification:** No, initial.

## Must-have truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | F1: torn pair run recovers `.core/` keys, with and without record; reads `core/` slot | VERIFIED | `rollback.rs` `snapshot_key` now returns `super::snapshot_slot(path)` for non-ledger keys. Unit test `snapshot_key_core_key_reads_the_core_slot` asserts core -> `core/...`, mode -> `files/...`, ledger unchanged. Engine tests `pair_core_torn_run_recovers_core_keys` and `..._without_record` (shared `torn_core_run_recovers`) kill a real apply mid-write, assert `RollbackKind::Restored`, `core_state == before`, mode files and ledger restored, journal gone, `core/ignition/tag-definition` kept; the record-lost variant deletes `record.json`. Not trivially passing. All pass. |
| 2 | F7: sftp prepare uses `apply::snapshot_slot` | VERIFIED | `sftp/apply_ops.rs` diff replaces the inline `match split_key` with `crate::deploy::apply::snapshot_slot(op.path.as_str())`. Exempt from RED per plan. |
| 3 | F4: created-only `.core/` restore succeeds with core missing on local, sh, ps; pre-image record still fails; symlink/non-dir core still refused | VERIFIED (ps unexecuted) | Local `restore_blocking`: strict `core_for` only if a `.core/` record has a pre-image, else `core_collection_if_present` (missing -> None; non-dir/symlink still refused via `core_collection`). `commit.sh` `_needs_core` and `commit.ps1` `$needsCore` do the same, and fail "not a directory" when core exists but is not a directory. Conformance row `core_restore_created_only_without_core_root` asserts (a) no failure, core records in `deleted`, mode copy back, core never created, second restore idempotent, and (b) a pre-image record still fails naming core. local and sh rows pass; ps row cfg(windows). |
| 4 | F5: probe refuses symlinked / non-dir / unopenable `core/ignition/<type>` or `core/ignition`, names folder, never probes parent | VERIFIED (ps unexecuted) | Local `probe_paths_blocking` uses `kind()`: first existing level decides, Missing goes up, Dir must open and be writable, anything else -> `core_tags_unsafe:<ty>`. `commit.sh` and `commit.ps1` same logic (sh adds `-x`). `transport::core_probe_problem` has the text; sftp `probe_problem` test and `winps` literal pin cover it. Conformance row covers a file at the type folder, a file at `core/ignition`, a symlink (and asserts nothing written through the link), and modes 0o000/0o200 (skipped as root). local and sh pass. |
| 5 | F3: empty/unseen-content mode type folder is residue at plan (warning), apply (refusal, gate and engine guard), verify (warning); standalone, `plan_sha256`, JSON key set unchanged | VERIFIED | `plan::pair_mode_tag_dirs` (pure), pair-only `list_types(mode)` in `deploy_plan` and `deploy_verify_planned`, `#[serde(skip)] DeployPlanResult.pair_mode_tag_dirs`, `pair_tags_refusal`/`pair_tags_warning`/`pair_mode_tags_warning` test the combined list. `run_apply` guard calls `gates::pair_tags_refusal` (apply/mod.rs:844). Tests: `pair_core_empty_mode_type_folder_refused` (gate: exit 6, names folder, data dir byte-unchanged, no run dir, no host calls; engine guard via hand-built `Ready` plan, no lock, no calls), `..._plan_warns` (one warning, `plan_sha256` equal with/without), `standalone_empty_mode_type_folder_not_refused` (same warnings and hash), `pair_verify_warns_on_empty_mode_type_folder` (check ids and ok unchanged, one warning, once under `--node both`), unit `pair_tags_refusal_names_mode_type_folders`, pure `pair_mode_tag_dirs_names_only_folders_with_no_read_file`. All pass. |
| 6 | F2: rig verdict recorded as exactly one line; on non-`core` no F2 code lands | VERIFIED (blocked branch) | SUMMARY has `F2 rig verdict: blocked` with timeline (up ok, mint ok, pair exit 1 "not linked within 300 s"), teardown done. No change to `verify.rs` or any verify check; `pair_verify_warns_on_mode_tag_files` untouched. Open item recorded for the user. |
| 7 | README gateway-tag-write sentence matches verdict (`blocked` -> unchanged) | VERIFIED | README.md:2643 sentence ("The gateway's own tag writes (Designer, `tags/import`) go to ...") is absent from the README diff. README changes are limited to F3 (folder residue, exit-6 row), F4 (rollback bullet) and F5 (host access bullet), each consistent with the code. |
| 8 | No slug/exit change, no Cargo change, goldens identical, gates green, SUMMARY hygiene | VERIFIED | See below. |

## Gates run (this verification)

| Check | Result |
|-------|--------|
| `cargo fmt --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | Finished, no warnings |
| Named tests (5 lib, 5 deploy_apply, 4 conformance local+sh, 1 deploy_plan, 1 deploy_pair) | all pass |
| `cargo test --workspace` | every `test result` line ok, 0 failed, no panics |
| xwin windows-msvc clippy | not re-run (SUMMARY claims it; ps rows unexecuted) |

## Hygiene

- Golden check: `git diff -U0 9a2e268` on `tests/deploy_plan.rs`, `apply/fingerprint.rs`, `ignition-cli/tests/contract_deploy.rs`, filtered for GOLDEN / 64-hex lines: no match. Only `pair_mode_tag_dirs: Vec::new()` field initialisers and the new tests were added in `deploy_plan.rs`.
- No `Cargo.toml` / `Cargo.lock` in the diff stat.
- Commits since 9a2e268 touch no `.planning/` path; config.json, `research/.cache/`, the `svc-ignition-deploy@...ps1` file and `tg-pg-table-check.md` are still working-tree only (`git status`: M / ?? as before).
- No Co-Authored-By in any of the four commit messages. Author pmannion2.
- No IP-shaped literal or VM host name in added diff lines or in SUMMARY.md (regex for dotted quads, wignvm, ldkrvm, ign-win, 10.4., 192.168, 52.154 returned nothing).
- Slugs and exit codes: `deploy_pair_tags_refused` / exit 6 unchanged; only its text, hint and README row wording changed.
- Stub scan on added lines (TODO, FIXME, XXX, TBD, HACK, todo!, unimplemented!, #[ignore]): none.

## Anti-patterns

None found. The `#[serde(skip)]` on `pair_mode_tag_dirs` is intentional (documented) and the field is not deserialised anywhere; both `DeployPlanResult` constructors outside tests (`deploy_plan`) populate it.

## Human verification required

1. **Windows CI leg.** `conformance_ps_*` rows (F4, F5) and the `commit.ps1` changes were reviewed by diff only. Logic mirrors the sh script, but it has not run here.
2. **Pair rig and live suite.** Verdict `blocked`; F2 is neither built nor ruled out, and the eight `live_deploy_pair` gates were skipped. The README claim that gateway tag edits go to `core` still rests on the create case only. A fresh rig run is needed to settle F2.
3. **F3 limitation (accepted, documented).** A type folder holding in-scope files the apply deletes plus out-of-scope content is not seen before the commit (needs a recursive listing); the post-apply verify names it. README says so.

## Verdict on `blocked` handling and README wording

Consistent with the plan: the plan says on `blocked` verify keeps today's warning, no F2 code lands, the README gateway-write sentence is unchanged, and the live suite may be skipped. All four hold. The F3/F4/F5 README wording matches the code (the F4 sentence correctly states a pre-image record still needs core; the F5 sentence matches the unsafe/unwritable split).

## Gaps

None.
