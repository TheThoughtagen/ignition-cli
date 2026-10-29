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
- **Residual races.** SUPERSEDED for the local mirror on unix by F7 below (the sftp check-then-act window remains, see F6). Originally: intermediate directories are checked with `symlink_metadata` and then used; a component swapped for a symlink between the check and the use by a concurrent attacker with write access to the collection is not defended against (only the final component is `O_NOFOLLOW`; sftp has the same check-then-act window). The host's own gateway account is the only party able to do that.
- **sftp staging copies are size-checked, not hash-checked, on the host.** SUPERSEDED by F6 below: copies are now read back and hashed before the first rename.
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

## Post-review security fixes

Two confirmed findings and one follow-up from the coordinator. Commits: F6 `ab760dd`, F6 follow-up `120c518`, F7 `02707b9`.

### F6: integrity bypass in the sftp mirror (`ab760dd`, `120c518`)

**Finding.** Staged copies on the host were only size-checked before `rename`, so a same-size different-content file committed.

**Fix** (`transport/sftp.rs`):

- `verify_remote_copies` fetches every file back with typed `SftpOp::Get` into a per-operation temp dir (`download_dir(op)`), re-reads each through `read_downloaded` (which uses `open_regular_file`, the size equal to the expected size and `READ_MAX_FILE_BYTES`), and hashes it locally with `sha256_hex` against `Staged.files`. No remote hashing command (no Windows variant, command surface unchanged). The 22-06 watchdog is reused: `run_download` now takes the operation name, kills sftp when the temp dir outgrows expected bytes + 1 MiB, chunks are at most 256 MiB / 10 000 files, and the running total is checked against `READ_MAX_TOTAL_BYTES` before each chunk.
- `Separate`: after the size survey and before the first rename, any mismatch is `transport_failed` naming up to 8 files ("does not match its SHA-256 ... nothing was changed"). The staged copies are then discarded (`discard_staged`): a fresh listing must show each one is still a regular file with no symlink above it, then only those `rm`s run, and only inside the staging root. The error says what was removed, what was left, and that empty directories remain. If the re-check fails the copies are left and the error says so.
- Follow-up `120c518` (coordinator): sftp has no atomic verify-then-rename. (1) The missing-directory `mkdir`s now lead the SAME batch as the renames/puts (a sorted set lists parents first), so no session sits between the pre-rename read-back and the moves; a test asserts the session after the first `get` is the renaming one. (2) After the moves, in BOTH modes, the FINAL paths are read back and hashed again. A mismatch is `transport_failed` with "content changed on the host during commit", names the files, says the written files were not rolled back, and no delete runs. (3) Deletes and emptied-directory removal only run after that verification passes.
- Command sequence change for the 22-12 C7a comparison: `Separate` commit gains one `get` session (all staged copies) before the move batch and one after; `InPlace` gains one `get` session after the puts. The move batch now starts with the `mkdir` lines. No `get -R`, no line starting with `-` or `!`.

**What `InPlace` can and cannot guarantee.** `InPlace` writes the final path directly, so there is no staged copy on the host to check beforehand. The bytes sent come from the hash-pinned private local copy. After the write, the final paths are read back and hashed before the first delete. It can detect that the host holds different bytes (corruption, a racing writer) and stop before deleting anything. It cannot prevent the write, and it cannot roll back: the overwritten originals are gone, so a mismatch leaves the collection holding the unverified content (the error says so). Phase 23's snapshot owns recovery.

**Residual (documented honestly).** The remaining window is a host-side writer with write access to the staging root or the collection directory during commit. Such a writer can already alter the gateway directly, so this is detected after the fact, not prevented; sftp offers no atomic verify-then-rename and no handle to pin the verified inode. The read-backs are also sequential reads of paths (sftp has no fd semantics).

**Tests** (`tests/deploy_transport.rs`, real `sftp -D` with a `Tap` runner hook; red before the fix: all three committed the tampered bytes):
`sftp_mirror_same_size_tampered_staged_copy_is_refused` (nothing renamed or deleted, staged copies removed, an untampered restage commits), `sftp_mirror_tamper_after_the_size_survey_is_refused` (swap lands just before the read-back), `sftp_mirror_oversize_swap_is_refused_before_any_rename` (24 MiB swap), `sftp_mirror_inplace_verifies_after_the_write_and_stops_before_deletes` (honest InPlace run still passes; tamper stops before every delete), and for the follow-up `sftp_mirror_tamper_between_pre_verify_and_rename_is_caught_after_commit` (swap just before the rename batch; post-commit check catches it, no `rm` or `rmdir` line, OLD files still present, and the renaming session directly follows the pre-verify `get`). `sftp_mirror_existence_first_mkdir` was adjusted: the mkdirs now share a batch with the moves, so "parent made earlier" compares (session, line).

### F7: TOCTOU in the local mirror (`02707b9`)

**Finding.** Writes, creates, renames, deletes and rmdirs were path-based; only the final open used `O_NOFOLLOW`, so an intermediate directory swapped for a symlink redirected them outside the collection root.

**Fix** (unix; `deploy/tree.rs` `dirfd` module next to `open_regular_file`, used by `transport/local.rs`):

- `dirfd` helpers take a directory handle and ONE name: `open_root` / `open_dir` / `walk` (`O_RDONLY|O_DIRECTORY|O_NOFOLLOW|O_CLOEXEC`), `ensure_dir` / `ensure_walk` (`mkdirat` for a missing level, then open with `O_NOFOLLOW`), `kind_at` / `ident_at` (`statat` with `AT_SYMLINK_NOFOLLOW`), `create_new` (`O_CREAT|O_EXCL|O_NOFOLLOW|O_WRONLY`, mode 0666 under the umask like `File::create_new`), `open_overwrite` (`O_NOFOLLOW|O_NONBLOCK`, `fstat` must be a regular file BEFORE `ftruncate`), `read_file`, `rename_at` (`renameat` between dir fds), `unlink_at`, `rmdir_at` (`AT_REMOVEDIR`). `ELOOP`, `ENOTDIR` and `ENXIO` are `FdError::Changed` ("{what} changed while it was written ... refused, nothing outside it was touched"); the same wording as the read side's "changed while it was read".
- `commit_blocking` is split into `commit_preflight` (path-based, advisory: ownership, validation, whole expected state, staged sources hash-verified) and `commit_apply`. The unix `commit_apply` opens `data_dir` -> `config` -> `resources` -> the mode collection as a dir-fd chain and does every mutation `*at`. For `Separate` it opens the staged file by handle, hashes it, checks the name still has that dev+ino, `renameat`s, then checks the target has that dev+ino. `InPlace` re-reads the hash-pinned private copy and writes through `open_overwrite`. Deletes and emptied-directory removal re-walk the fd chain per file. The staging-root writes in `stage` (`fill_staging_root`) are descriptor-relative from the staging root's own handle too.
- Windows: the previous path-based `commit_apply` / `fill_staging_root` stay behind `#[cfg(not(unix))]`, as does the helpers' use (`ensure_dir_chain`, `write_new`, `write_in_place`). Not compiled or run here (no windows target installed), same status as the read side.

**Tests** (`local.rs` `tests::toctou`, red before the fix: the staging-directory swap moved a decoy into the collection, and the others failed on the old message): a stale plan where each of unit dir, delete target's dir, type dir, module dir, mode collection and `config` is replaced by a symlink to an outside tree AFTER `commit_preflight` and BEFORE `commit_apply`, in both staging modes, is refused with "changed while it was written" and the outside tree is byte-identical (names, contents, no new entries); a swapped staging directory is not renamed from; staging never writes through a symlink in the staging root; an honest apply still produces the same collection, report and a writable file mode. All existing `deploy_transport` local and sftp mirror tests pass unchanged.

**Residuals.**

- Windows keeps path-based mutations: an intermediate directory swapped between the preflight and a mutation is not defended against there (std has no descriptor-relative primitives).
- The roots of the chains (`data_dir`, the staging root) are opened by path with a final-component `O_NOFOLLOW`; ancestors above them are the operator's own and followed.
- Staged-file verify-then-rename on `Separate` is hash through the handle, then `renameat` by name: a swap in that instant is caught only by the dev+ino re-check before and after (detect, not prevent). The staging root is private to the deploy user.
- The preflight, `check_staging_real` and `canonical_with_tail` stay path-based by design (read-only, advisory).
- Not atomic across files (unchanged, Phase 23).

### Verification

- `cargo test --workspace`: 2001 passed, 0 failed. `deploy_transport` 59 passed (was 54: +5 sftp tests); `ignition-core --lib` 732 passed (includes the 4 new `toctou` tests); `deploy_plan` 57 passed, `PLAN_GOLDEN` and `PLAN_GOLDEN_LEDGER` unchanged (no plan code touched).
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` are clean.
- New packages: none. `Cargo.lock` is byte-identical to `d38f42b` (`git diff d38f42b HEAD -- Cargo.lock` is empty); `rustix` (`fs`) and `base64` were already direct dependencies.
- `.planning/config.json` and `.planning/research/.cache/` untouched (still the pre-existing uncommitted state).

### Accepted residual (orchestrator, 2026-10-14)

- A further review flagged a TOCTOU in `local.rs` after `02707b9`. No path-based mutation remains in the local mirror. What remains are the documented residuals: chain roots (`data_dir`, staging root) opened by path, and the detect-only hash-then-`renameat` window. Exploiting either needs local write access to the gateway data dir or staging root, which can already alter the gateway. The write half is unreachable until Phase 23. Re-audit when Phase 23 makes it reachable (`/gsd-secure-phase 23`).
