---
phase: 22-transport-plan-pull-verify
plan: 07
subsystem: deploy
tags: [rust, transport, write-half, sftp, symlink-safety, delete-set, security]

requires:
  - phase: 22-transport-plan-pull-verify
    provides: TreeReader seam, LocalFsTransport, DeleteSet (22-01, 22-03), typed SftpOp and RemoteShell (22-05), SftpTransport read half (22-06)
provides:
  - "TreeMirror (stage, commit), StagingMode::{Separate{root}, InPlace}, Staged, MirrorReport, mirror_for_node"
  - "impl TreeMirror for LocalFsTransport and SftpTransport, each bound to <data_dir>/config/resources/<mode> through with_mirror(mode, staging)"
  - "Shared pre-I/O validation: validate_mirror_inputs, validate_mirror_paths, validate_staging_root, mirror_path_problem, mirror_mode_problem, copy_to_temp, read_staged_copy"
  - "write_half_unreachable source scan over actions/deploy.rs, ignition-cli main.rs and mcp.rs"
affects: [22-12, 22-13, 22-14, 23]

actuals:
  tokens: 30200
  tasks: 2
  commits: 1

tech-stack:
  added: []
  patterns:
    - "Check the whole expected state first (survey / inspect), write second; any mismatch is transport_failed with nothing written"
    - "sftp state is learned by listings only: one ls -la session per depth, mkdir only for levels a listing showed missing"
    - "The staged copy is a hash-pinned private temp dir re-read through open_regular_file at commit"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/deploy/transport/mod.rs
    - crates/ignition-core/src/deploy/transport/local.rs
    - crates/ignition-core/src/deploy/transport/sftp.rs
    - crates/ignition-core/tests/deploy_transport.rs

key-decisions:
  - "A Staged belongs to the mirror and mode that made it (owner string) and to its staging mode; commit refuses a mismatch"
  - "Separate commit consumes the staged files on the staging root (they are renamed away); stage again to commit again. Idempotence is stage+commit twice"
  - "Commit order is writes, then deletes, then emptied-directory removal, so a partial failure leaves more rather than less"
  - "Mode 'core' and 'local' can never be mirrored; a delete-set path or staged path needs 3+ components, no dot-named or gateway-owned component, no control character, and may not be a file and the directory of another path"

patterns-established:
  - "sftp survey: list only directories known to exist that have a needed child; a child of a missing directory is missing without a listing"

requirements-completed: []
requirements-advanced: [DPLN-01]

duration: 80min
completed: 2026-10-14
status: complete
---

# Phase 22 Plan 07: TreeMirror write half Summary

**`TreeMirror` stages a rendered tree and commits it with an explicit delete set on both the local and the sftp transport, never follows or creates a symlink, never writes a foreign entry into the collection, and is unreachable from every Phase 22 verb (pinned by a source scan).**

## Accomplishments

- **`transport/mod.rs`**: `TreeMirror`, `StagingMode`, `Staged`, `MirrorReport`, `mirror_for_node`. `for_node` and `mirror_for_node` share one `node_access` resolver (Windows ssh stays refused, with "written" in the message for the mirror). The trait doc states the contract (bounded writes, nothing foreign, fail closed, caps, atomicity) and that no verb may import it.
- **Validation before any I/O** (shared by both transports): every staged and delete path needs 3+ components, no dot-named component, no `config-mode.json`, no control character; no path both staged and deleted or listed twice; no path that is a file and the directory of another; per-file, total and entry caps equal to the read side; a `Separate` root must be absolute, at least two directories deep, and neither inside nor around `config/resources`. The local mirror also compares canonical paths so a root reached through a symlink into the collection is refused.
- **Local mirror** (`local.rs`): `stage` writes a hash-pinned private temp copy (and, in `Separate`, copies into the root with `create_new`, replacing only a stale regular file). `commit` first checks the full expected state (every delete target a regular file, every write target absent or a regular file, no symlink at any level, the staged source re-read through `open_regular_file` and hash-verified), then creates directories one `create_dir` at a time, then `rename`s (`Separate`) or writes with `O_NOFOLLOW` (`InPlace`), then `remove_file`s the delete set, then `remove_dir`s emptied directories from the deepest up, stopping at a non-empty one and before `<module>/<type>`.
- **sftp mirror** (`sftp.rs`): the same contract through typed `SftpOp` only (`LsLongAll`, `Mkdir`, `Put`, `Rename`, `Rm`, `Rmdir`). A `survey` learns the state of every needed path from listings (one session per depth), refusing a symlink or special file anywhere. `Separate` stage uploads into the root; commit verifies each staged file exists on the host with the size of the hash-pinned local copy, then renames into place. All sessions go through `require_success("stage" | "commit", ..)`.
- **`write_half_unreachable`** scans the three verb-dispatch sources for `TreeMirror`, `mirror_for_node`, `StagingMode`, `.stage(`, `.commit(`.

## Exact sftp command sequence (for the C7a comparison in 22-12)

Captured with `cargo test -p ignition-core --test deploy_transport sftp_mirror_separate_and_inplace -- --nocapture` (OpenSSH 10.3p1, `sftp -D /usr/libexec/sftp-server`, macOS). `<DATA>` is the data dir, `<STAGE>` the staging root, `<LOCAL>` the private temp copy, `D` = `<DATA>/config/resources/dev/ignition/database-translator`. The scenario updates `UPD`, adds `NEW` and deletes `OLD`. Each line is one batch line, grouped by session.

`Separate { root }` (staging root's ancestors are listed one level per session, the test tempdir is 5 deep so sessions 1-6 and 19-24 are ancestor listings; a real root such as `/var/tmp/ign-stage` costs one or two):

| Phase | Sessions | Batch lines |
|---|---|---|
| stage: survey the root | 1..n | `@ls -la "/<c1>"`, `@ls -la "/<c1>/<c2>"`, ... one per level of the root |
| stage: mkdir missing | one per depth | `mkdir "<STAGE>"`, `mkdir "<STAGE>/ignition"`, `mkdir "<STAGE>/ignition/database-translator"`, then `mkdir "<STAGE>/.../NEW"` and `.../UPD` in one session |
| stage: upload | 1 | `put "<LOCAL>/ignition/database-translator/NEW/config.json" "<STAGE>/ignition/database-translator/NEW/config.json"` (and `NEW/resource.json`, `UPD/config.json`) |
| commit: survey collection | 7 | `@ls -la "<DATA>"`, `.../config`, `.../config/resources`, `.../dev`, `.../dev/ignition`, `D`, then `D/OLD` and `D/UPD` together |
| commit: survey staging copies | n + 4 | `@ls -la` per root level, then `<STAGE>/ignition`, `<STAGE>/ignition/database-translator`, `.../NEW` and `.../UPD` together (sizes compared to the local copy) |
| commit: mkdir | 1 | `mkdir "D/NEW"` |
| commit: move | 1 | `rename "<STAGE>/ignition/database-translator/NEW/config.json" "D/NEW/config.json"` (and `NEW/resource.json`, `UPD/config.json`; `UPD/config.json` replaces an existing file) |
| commit: delete | 1 | `rm "D/OLD/config.json"`, `rm "D/OLD/resource.json"` |
| commit: emptied dirs | 2 | `@ls -la "D/OLD"` then `rmdir "D/OLD"` |

`InPlace`: stage runs no session. Commit is `@ls -la` x6 (`<DATA>` ... `D`) plus `D/OLD` and `D/UPD` together, `mkdir "D/NEW"`, `put "<LOCAL>/..." "D/.../config.json"` (3 lines, one session, `UPD/config.json` replaces an existing file), `rm` x2, `@ls -la "D/OLD"`, `rmdir "D/OLD"`: 12 sessions. `Separate` took 33 sessions on the 5-deep tempdir root; a shallow root is about 20.

No line starts with `-`, `!`, or `get -R` (asserted by `sftp_mirror_never_suppresses_errors`).

## Test results

- `cargo test -p ignition-core --test deploy_transport`: 54 passed (25 new): local `local_mirror_separate`, `local_mirror_inplace`, `no_foreign_entries_in_collection`, `delete_set_refusals_write_nothing`, `staging_root_inside_collection_refused`, `staging_root_through_a_symlink_into_the_collection_refused`, `commit_is_idempotent`, `module_type_dirs_survive`, `a_dir_with_other_files_is_not_removed`, `oversize_file_is_refused_before_any_io`, `dot_named_and_gateway_owned_staged_paths_are_refused`, `symlinks_are_never_followed_or_created`, `tampered_staged_copy_is_refused`, `staged_files_belong_to_their_mirror`, `a_missing_mode_collection_is_created_and_core_is_never_written`, `a_data_dir_without_config_resources_is_refused`, `mirror_for_node_builds_local_and_refuses_windows_and_core`; sftp `sftp_mirror_separate_and_inplace` (collection bytes and report equal to the local mirror), `sftp_mirror_never_suppresses_errors`, `sftp_mirror_existence_first_mkdir`, `sftp_mirror_delete_set_refusals_write_nothing`, `sftp_mirror_refuses_a_staging_root_inside_the_collection`, `sftp_mirror_symlinks_are_refused_and_never_followed`, `sftp_mirror_tampered_staged_copy_is_refused`, `sftp_mirror_hostile_but_legal_names_are_inert`; plus `write_half_unreachable`.
- `cargo test -p ignition-core --test deploy_plan`: 57 passed; `PLAN_GOLDEN` and `PLAN_GOLDEN_LEDGER` unchanged.
- `cargo test -p ignition-core --lib deploy::`: 191 passed. `cargo test --workspace` has no failure. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` are clean.
- **Where the real-protocol tests ran:** the dev Mac only (OpenSSH 10.3p1). The sftp mirror tests are `#[cfg(unix)]` and use the 22-06 `sftp_server_path()` discovery; the ubuntu CI leg is unconfirmed, as for 22-06. Windows is untested (`std::fs::rename` replace semantics, `symlink_metadata`, the no-`O_NOFOLLOW` path).

## Deviations from Plan

**1. [Override] Typed `SftpOp` and `RemoteCommand`.** No raw lines; `Rename`, `Put`, `Rm`, `Mkdir`, `Rmdir`, `LsLongAll` cover the mirror, so no new op variant was needed.

**2. [Rule 2 - Missing critical functionality] Write-half safety from the override.** Symlinks are refused at every level (local `symlink_metadata` walk plus `O_NOFOLLOW` on the final open; sftp listing kinds), every remote target goes through `safe_remote_path` and stays under `config/resources/<mode>`, the expected state is checked before the first write and fails closed, bytes and entries are capped with the read-side constants, and local staging reads use `open_regular_file` with a SHA-256 pin. None of this was in the plan text.

**3. [Rule 2] `Staged` additions.** `Staged.files` carries the SHA-256 per file and `Staged` gained a `pub(crate) owner` string; commit refuses a `Staged` from another node, mode or staging mode.

**4. [Judgement] `with_mirror(mode, staging)` on the two transports.** The plan says `impl TreeMirror for LocalFsTransport`; the mode and staging mode have to live in the struct, so the constructors gained an optional mirror setup. `for_node` readers have none and `stage` on them is an error. The `Separate` root of an sftp mirror is normalised to an absolute POSIX path.

**5. [Judgement] `mirror_for_node` returns `deploy_tree_invalid` for a bad mode** (`core`, `local`, dot-named, separator) and a bad staging root, naming the label, before any I/O.

**6. [Judgement] Staged files stay in the private temp dir in both modes**, and in `Separate` mode a second copy goes to the root. `commit` verifies the source it will use (root copy for local `Separate`, temp copy for `InPlace`) and, for sftp, the local copy's size against the host's staged copy.

**7. [Process] One commit, not two.** `mirror_for_node` and its ssh arm live in the same file as the local arm, so task 1 cannot compile without task 2's `with_mirror`. Tests and implementation landed together (no separate RED commit), as in 22-05 and 22-06.

**Total deviations:** 2 Rule 2 additions, 2 override applications, 3 judgement calls, 1 process note. No architectural change.

## Known limitations

- **Not atomic across files.** A failure part-way leaves the files already moved in place; Phase 23's snapshot and `restore_snapshot` own that (T-22-34, transferred). `InPlace` is not atomic per file either (open with truncate); `Separate` is a rename per file.
- **Residual races.** Intermediate directories are checked with `symlink_metadata` and then used; a component swapped for a symlink between the check and the use by a concurrent attacker with write access to the collection is not defended against (only the final component is `O_NOFOLLOW`; sftp has the same check-then-act window). The host's own gateway account is the only party able to do that.
- **sftp staging copies are size-checked, not hash-checked, on the host.** A file replaced by one of the same size in the staging root between `stage` and `commit` would be moved in. The staging root must be private to the deploy user.
- **Listing the ancestors of the staging root** requires read access to each. A root under `/home/<user>` where `/home` is not listable fails closed.
- **`Separate` needs one filesystem.** A rename across filesystems fails with `transport_failed` (local) rather than falling back to copy.
- Staging directories left empty under the root after a `Separate` commit are not removed.
- Windows ssh nodes stay refused (22-13). The Windows rename-over-existing behaviour is a C7a item.
- DPLN-01 is not marked complete: the Windows OpenSSH host remains.

## New packages

None. `Cargo.lock` unchanged (`rustix`, `tempfile`, `async-trait` were already direct dependencies).

## Known Stubs

None.

## Threat Flags

None beyond the plan's register. T-22-30 (trait split plus `write_half_unreachable`), T-22-31 (bounded delete set, pre-I/O refusals, emptied-dir stop at `<module>/<type>`), T-22-32 (no temp or dot-named entry in the collection, staging root outside it, walk tests in both modes), T-22-33 (`symlink_metadata` and listing-kind checks, `rm` never on a symlink) are mitigated and tested. T-22-34 is transferred to Phase 23 as planned.

## Self-Check: PASSED

- Files present: `grep -n 'pub trait TreeMirror' crates/ignition-core/src/deploy/transport/mod.rs` prints one line; `grep -n 'impl TreeMirror for' crates/ignition-core/src/deploy/transport/local.rs crates/ignition-core/src/deploy/transport/sftp.rs` prints one line each; `grep -n 'fn write_half_unreachable' crates/ignition-core/tests/deploy_transport.rs` prints one line.
- Commit `183344a` present in `git log`.
- No `unwrap`, TODO or placeholder in the new non-test code; `.planning/config.json` and `.planning/research/.cache/` untouched.
