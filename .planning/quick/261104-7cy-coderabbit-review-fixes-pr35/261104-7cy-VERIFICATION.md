---
phase: quick-261104-7cy
verified: 2026-11-04T15:10:00Z
status: human_needed
score: 8/8 must-haves verified (C6, C7, C9 verified structurally and by xwin compile; their Windows behaviour is a human/CI item)
behavior_unverified: 0
overrides_applied: 0
human_verification:
  - test: "Run the Windows CI leg: conformance_ps_core_probe_walks_every_folder_on_a_touched_path (cases b, e junctioned core\\ignition, f, g), conformance_ps_core_restore_created_only_without_core_root, conformance_ps_core_probe_refuses_unsafe_type_folder"
    expected: "All three ps rows pass; commit.ps1 CoreWalk refuses a junctioned core\\ignition as core_tags_unsafe:tag-definition and a file below the type folder, and passes a writable path and a writable type folder with lower levels missing"
    why_human: "The rows are cfg(windows). On macOS they were compiled under xwin clippy for x86_64-pc-windows-msvc (clean) but not executed."
  - test: "On a Windows host, deny CreateDirectories (but allow CreateFiles) on core\\ignition\\tag-definition with default\\ missing, then run a pair apply touching .core/ignition/tag-definition/default/... (C7)"
    expected: "write_host probe reports core_tags_unwritable:tag-definition before any write"
    why_human: "CanAddDir (bit 4) is pinned only structurally by winps::tests::ps_core_probe_walks_top_down_and_never_recreates_core; no test, including the Windows CI row, exercises an ACL that allows files but denies folders."
  - test: "On a Windows host, remove config\\resources\\core between probe and commit (or force MoveOne on a .core/ key with core absent) (C9)"
    expected: "write fails 'the core collection does not exist or is not a directory'; core is not recreated"
    why_human: "MoveOne/MakeParents guards are pinned structurally only; commit.ps1 cannot run on macOS."
---

# Quick 261104-7cy: Verification Report

**Goal:** Fix CodeRabbit comments C1-C12 on PR #35 (branch `feat/deploy-tags-core`, base 579017a, HEAD e78856f).
**Verified:** 2026-11-04
**Status:** human_needed (Windows-only behaviour of the commit.ps1 changes; no gaps)
**Re-verification:** No, initial.

Commits 579017a..HEAD: c7526fc (C4), 65b80b7 (C3), af35aa4 (C12), b8f2ed5 (C5-C9), 8bac082 (C10), e78856f (C1, C2, C11).

## Observable truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | C4: a pair's `plan --diff` shows the mode copy of a routed tag unit as a removal; repo bytes appear once; standalone diffs, `plan_sha256`, goldens unchanged | VERIFIED | `plan.rs` `render_diffs`: `desired` now taken from the render only when `file.desired_sha256.is_some()`, so a mode-copy row (desired None) renders live lines as `-` and no `+`. Test `deploy_pair::pair_plan_diff_shows_mode_copy_as_removal` asserts `+"defaultValue":"ign"` exactly once, `-"defaultValue":"old"` once, `+...old` zero (passes). Golden lines unchanged (see Hygiene); `RENDER_GOLDEN`/`PLAN_GOLDEN*` tests pass in the workspace run. |
| 2 | C3: full listing via `TreeReader::list_tree` on local, sftp Linux, sftp Windows (held to the PS walk); unread file or folder-with-no-read-file is residue at plan (warn), apply (refuse `deploy_pair_tags_refused`/6 before any write), verify (warn), naming topmost paths; empty type folder still `ignition/<type>`; plain migration not refused; `unmanaged` read files stay out; slug unchanged | VERIFIED | `transport/mod.rs`: `TreeEntry`, `TreeListing`, `async fn list_tree` on the trait. Real impls: `local.rs` `list_tree_blocking` (probe + `walk_capped`, entry budget), `sftp.rs` `list_tree` (`discover` + `cross_check` on Windows, long entries kept). `plan::pair_mode_tag_dirs(listed: &[TreeListing], ...)` implements the read-set rule (folder when nothing read; else unread files and folders with no read path below; topmost only). `actions::deploy::pair_mode_tag_dirs_of` (now `pub(crate)`) calls `list_tree` over `PAIR_UNSYNCED_TAG_TYPES`; used by `deploy_plan` and `deploy_verify_planned`. Tests (all pass): `pair_core_migration_refused_with_unseen_content` (3 cases: empty folder, other provider's file, loose file; exit 6, names topmost path, not the type folder, data dir byte-unchanged, no run dir, no host calls), `pair_core_unseen_content_plan_warns` (one warning, `plan_sha256` equal with/without), `pair_verify_warns_on_unseen_content` (files pass, ids unchanged, one warning naming all paths, once under `--node both`), pure `pair_mode_tag_dirs_names_unseen_content_beside_read_files` and the rewritten F3 test (Unmanaged read file not residue), lib `list_tree_*` x3 (incl. symlink refusal, no downloads, Windows omitted entry `transport_failed`, reparse refused, over-limit entry listed). Existing `pair_core_migration_*` and `pair_core_empty_mode_type_folder_*` still pass. |
| 3 | C12: `run_apply` re-lists after staging, immediately before the scan lock; refuses residue made after the plan; nothing committed, staging dropped, no lock, `result.json` records it | VERIFIED | `apply/mod.rs` `drive`: after `ApplyEvent::Staged` and before `HeldLock::acquire`, on `Redundancy::Pair` calls `pair_mode_tag_dirs_of(ctx.reader, mode, &plan.entries)` then `gates::pair_tags_refusal_with`; listing errors also refuse (fail closed); `discard_interrupted` then return Err. `ctx.reader` is the master's reader. Test `pair_core_type_folder_made_after_the_plan_is_refused` (hook creates `ignition/tag-type-definition` on `Staged`): exit 6, keys == [that folder], no `commit` call, last call `discard_interrupted`, `lock_acquires()==0`, `scan_count()==0`, canary not in core, staging empty, `result.json` error code `deploy_pair_tags_refused`. Pin `pair_core_relist_keeps_a_clean_run` applies normally. Position before the lock is the accepted planner deviation (documented in SUMMARY and module docs). |
| 4 | C5-C8: on local, `commit.sh`, `commit.ps1`, the probe walks each touched core key's folders top-down from `core/ignition`; link/junction/non-dir at any level is `core_tags_unsafe:<type>` (linked `core/ignition` included); unwritable folder at/below type folder is `core_tags_unwritable:<type>`; deepest existing folder must take what the commit makes (Windows: CanAddDir); codes/slugs unchanged | VERIFIED (local, sh executed; ps structural + compile) | Local `apply_ops.rs` `core_walk` (memoized, top-down, stops at first missing level, checks deepest writable); `commit.sh` `core_walk` (same rule, `-L`/`-d`/`-e`, `-w`/`-x`, `_seen` memo); `commit.ps1` `CoreWalk` (`IsLink`/`IsDir`, `CanAddFile` at depth >= 1, `CanAddFile`+`CanAddDir` on deepest when a level is missing); the old candidate loops are gone in all three. Conformance row `core_probe_walks_every_folder_on_a_touched_path`: local and sh pass (cases a unwritable folder below type, b file below type, c symlinked folder below type, d symlinked `core/ignition`, f whole path writable, g type folder writable with lower levels missing; asserts nothing created and nothing written through links). ps row is cfg(windows) (human item). Structural pin `winps::tests::ps_core_probe_walks_top_down_and_never_recreates_core` passes. Codes unchanged; only `core_probe_problem` wording widened. |
| 5 | C9: `commit.ps1` never recreates `config/resources/core` in a write; `MoveOne` refuses a `.core/` key when core root is not an existing real dir, before `MakeParents`; `MakeParents` never creates its base; structural test pins both | VERIFIED (structural; Windows behaviour human) | `MoveOne` opens with `IsCoreKey $key` + `Attr $S.Cr` guard setting `$S.Fail` and returning `$false`; `MakeParents` checks `Attr $cur` of the base (`IsDir` and not `IsLink`) before its loop. `CreateDirectory((Xp $S.Cr))` absent from the script. The winps structural test asserts guard < MakeParents call and base check < loop (passes). |
| 6 | C10: `assert_non_tag_change_syncs` no longer asserts `waited_s == 0`; keeps `Applied`, `converged`, `resync == None`, `files` pass; doc says so | VERIFIED | Function body read at HEAD: `waited_s` only in the `eprintln!` diagnostic; asserts `OutcomeKind::Applied`, `report.converged`, `report.resync == None`, `Files == Pass`, reads only, settled, both `Good`. Doc updated. `--no-run` compile clean. |
| 7 | C1, C2, C11: 261103-ell PLAN gates read the last `F2 rig verdict` line (retry counts) with no quiet grep on a pipe; 261103-ell VERIFICATION records the final state; README pair verify paragraph corrected | VERIFIED | PLAN: quiet greps on pipes 13 at 579017a -> 0 at HEAD (`grep -oE '\| *grep -[A-Za-z]*q'`); Task 3c sets `V=$(grep -E '^F2 rig verdict( \(retry\))?: ...' | tail -n 1 | sed ...) && test -n "$V"`; sample `blocked` then `(retry): core` yields `core`; all five `<automated>` lines pass `bash -n`. VERIFICATION: commits list includes b5579ec (exists; touches `verify.rs`, `deploy_pair.rs`, README), truth 6 on `core` branch, 7 VERIFIED against README sentence (present, count 1), live 8 passed, xwin ran, FULL-GATE-OK, `human_needed` kept for the ps rows only, F3 open item closed by 65b80b7. README `**Redundant pairs in verify.**` now states the declared-unit mode copy fails `files` as `files_differ`, the warning names undeclared units' mode files and any unread file or folder (empty folder included), `--node both` once, `--node backup` neither; the "whatever its unit's status" clause is gone from that paragraph. |
| 8 | RED-first per code finding; no slug/exit/Cargo change; goldens byte-identical to 579017a; fmt, clippy, workspace tests, xwin clippy green | VERIFIED | RED evidence is recorded per finding in SUMMARY (C3 end-to-end REDs before code; the three `list_tree` unit tests' RED by emptied bodies, a disclosed deviation). I did not re-run the REDs (would require reverting committed code); each new test asserts exactly the behaviour its fix adds, so it could not have passed against the base code by construction for C3, C4, C12, C5 (old local/sh loops never inspected folders below the type folder) and C8 sh. Re-run here: `cargo fmt --all --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test --workspace` 96 binaries, 3189 passed, 0 failed, 96 ignored; `cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings` clean. |

**Score:** 8/8 truths verified (0 present-but-behavior-unverified; Windows-only behaviour of C6/C7/C9 routed to human verification).

## Required artifacts

| Artifact | Expected | Status |
|----------|----------|--------|
| `crates/ignition-core/src/deploy/transport/mod.rs` | `TreeEntry`, `TreeListing`, `async fn list_tree` | VERIFIED |
| `crates/ignition-core/src/deploy/plan.rs` | `pub fn pair_mode_tag_dirs(listed: &[TreeListing]`, C4 fix in `render_diffs` | VERIFIED |
| `crates/ignition-core/src/actions/deploy.rs` | `pub(crate) async fn pair_mode_tag_dirs_of` on `list_tree` | VERIFIED |
| `crates/ignition-core/src/deploy/apply/mod.rs` | C12 re-list using `pair_tags_refusal_with` | VERIFIED |
| `crates/ignition-core/src/deploy/apply/gates.rs` | `pub fn pair_tags_refusal_with`; `pair_tags_refusal` delegates to it | VERIFIED |
| `crates/ignition-core/tests/deploy_host_conformance.rs` | `core_probe_walks_every_folder_on_a_touched_path` row for local, sh, ps | VERIFIED (ps cfg(windows)) |
| `crates/ignition-core/src/deploy/transport/scripts/commit.ps1` | `function CanAddDir`, `CoreWalk`, MoveOne/MakeParents guards | VERIFIED |
| `.planning/quick/261104-7cy-.../261104-7cy-SUMMARY.md` | `## Findings` C1-C12 | VERIFIED |

## Key links

| From | To | Via | Status |
|------|----|-----|--------|
| `deploy_plan`, `deploy_verify_planned` (pair) | `list_tree` -> `pair_mode_tag_dirs` | `pair_mode_tag_dirs_of` | WIRED |
| `apply::drive` (pair, after prepare/Staged, before `HeldLock::acquire`) | `pair_mode_tag_dirs_of` -> `pair_tags_refusal_with` | fresh residue, same definition | WIRED |
| `gates::pair_tags_refusal` | `pair_tags_refusal_with(plan, redundancy, &plan.pair_mode_tag_dirs)` | one definition | WIRED |
| local `probe_paths_blocking`, `commit.sh` `op_probe`, `commit.ps1` `OpProbe` | `core_tags_unsafe` / `core_tags_unwritable` codes -> `core_probe_problem` | one top-down walk | WIRED |
| `plan::render_diffs` | `PlanFile::desired_sha256` | desired bytes only for rows with a desired sha | WIRED |

All five `TreeReader` test doubles implement `list_tree` (delegate, count or `unreachable!` in a verify-only double that never lists); workspace compiles.

## Behavioral spot-checks (targeted, run here)

| Behavior | Command | Result |
|----------|---------|--------|
| C3/C12 engine tests | `cargo test -p ignition-core --test deploy_apply pair_core_` | 15 passed |
| C3 verify, C4 diff | `cargo test -p ignition-core --test deploy_pair` | 14 passed |
| C3 pure rule | `cargo test -p ignition-core --test deploy_plan pair_mode_tag_dirs` | 2 passed |
| C5/C8 local + sh | `cargo test -p ignition-core --test deploy_host_conformance core_probe` | 6 passed |
| C3 `list_tree` units | `cargo test -p ignition-core --lib list_tree` | 3 passed |
| C6/C7/C9 structural | `cargo test -p ignition-core --lib ps_core_probe_walks_top_down` | 1 passed |
| C10 compile | `cargo test -p ignition-core --test live_deploy_pair --no-run` | built |
| Workspace | `cargo test --workspace` (once) | 3189 passed, 0 failed |

Live rig not run (instructed). SUMMARY's `## Live` (8/8 live pair gates on HEAD e78856f) is not re-verified here; no truth depends on it.

## Hygiene

- Goldens: `git diff -U0 579017a..HEAD` on `tests/deploy_plan.rs`, `apply/fingerprint.rs`, `ignition-cli/tests/contract_deploy.rs` filtered for `GOLDEN` or 64-hex lines: no match. `fingerprint.rs` and `crates/ignition-cli` untouched.
- Slug/exit: `error.rs` diff is doc and hint text of `DeployPairTagsRefused` only; `deploy_pair_tags_refused` / 6 asserted by tests.
- No `Cargo.toml` / `Cargo.lock` in the diff.
- No co-author line in any of the six commit messages.
- Protected files (`.planning/config.json`, `.planning/research/.cache/`, `svc-ignition-deploy@...ps1`, `.planning/debug/tg-pg-table-check.md`), STATE.md, and this task's PLAN/SUMMARY are not in any commit; they remain working-tree only.
- IP literals / host names: no dotted quad in added lines outside loopback; the only hostname-pattern hits are inside the 261103-ell PLAN gate's own grep patterns (`-uat4\.ps1$`, `#[ignore`) and a redacted `svc-ignition-deploy@...ps1` mention; no real address or VM name.
- Added lines contain no `TODO`/`FIXME`/`XXX`/`TBD`/`HACK`/`todo!`/`unimplemented!`/`#[ignore`/`.only` outside the PLAN gate's grep pattern text.

## Anti-patterns

None blocking. Info:
- C4 test asserts no `+` line carrying the old bytes and exactly one `+` with the repo bytes, not literally "no `+` line in the mode section"; the fix makes the stronger statement hold by construction (desired is `None` for that row).
- 261103-ell Task 3a's gate still counts exactly one plain `F2 rig verdict:` line; `(retry)` lines do not match that regex, so a retried log still passes. Consistent with C1's scope (Task 3c).

## Human verification required

1. **Windows CI leg.** `conformance_ps_core_probe_walks_every_folder_on_a_touched_path` (incl. junctioned `core\ignition`, C6), plus the 261103-ell ps rows. Compiled under xwin clippy only.
2. **C7 CanAddDir behaviour.** A folders-denied, files-allowed ACL on the deepest existing core folder with a missing level below should give `core_tags_unwritable`. No test exercises it, even on Windows CI.
3. **C9 behaviour.** `MoveOne`/`MakeParents` refusing to recreate a removed core on a Windows host. Structural pin only.

## Gaps

None.

---

_Verified: 2026-11-04_
_Verifier: Claude (gsd-verifier)_
