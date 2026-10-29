---
phase: 22-transport-plan-pull-verify
plan: 06
subsystem: deploy
tags: [rust, ssh, sftp, transport, read-half, resource-caps, security]

requires:
  - phase: 22-transport-plan-pull-verify
    provides: TreeReader seam and LocalFsTransport (22-01), ledger location (22-03), ProcessRunner and hardened RemoteShell with typed SftpOp (22-05)
provides:
  - "SftpTransport: TreeReader over system sftp for Linux ssh nodes (read, list_types, read_ledger)"
  - "parse_ls_listing / parse_batch_listing / parse_batch_listing_capped, posix_root, EntryKind, ListedEntry, ListingError, READ_MAX_DEPTH"
  - "for_node ssh arm: os linux builds RemoteShell + SftpTransport, os windows is deploy_tree_invalid naming <label>.ssh.os"
  - "SftpOp::LsLongAll (ls -la), typed and validated like the other ops"
  - "tests/common/sftp_server.rs: sftp-server discovery that panics when absent"
affects: [22-07, 22-08, 22-10, 22-13, 22-14]

actuals:
  tokens: 31000
  tasks: 2
  commits: 5

tech-stack:
  added: []
  patterns:
    - "Existence-first read: one ls -la tree level per sftp session, a path enters a batch only after an earlier listing showed it"
    - "Listings are untrusted: entries are counted while parsing, names are gated by safe_remote_path, symlinks and special files are refused from the mode column"
    - "Downloads are one get per listed file in size-bounded chunks with a disk watchdog that kills sftp, then a local walk must equal the listing exactly"
    - "A scripted in-process sftp server (FakeSftp) implements ProcessRunner for protocol-level unit tests; a real sftp-server through sftp -D backs the integration tests"

key-files:
  created:
    - crates/ignition-core/src/deploy/transport/sftp.rs
    - crates/ignition-core/tests/common/sftp_server.rs
  modified:
    - crates/ignition-core/src/deploy/transport/mod.rs
    - crates/ignition-core/src/deploy/transport/remote.rs
    - crates/ignition-core/tests/common/mod.rs
    - crates/ignition-core/tests/deploy_transport.rs

key-decisions:
  - "Lists with ls -la for every level (not ls -1): it exposes the mode column (symlink and special-file refusal, matching the local transport), sizes (caps before any download) and dot-named entries that ls -1 hides. Needs the new typed SftpOp::LsLongAll"
  - "Per-file get instead of get -R (post-review): the server decides what a recursive get delivers, so every per-file pre-check would be bypassed"
  - "config and config/resources are required levels; a data dir missing either is transport_failed (the local transport's behaviour), not an empty tree"
  - "DPLN-01 stays pending: the ssh read half for Linux hosts has landed, but the requirement names a Windows OpenSSH host and the write half"

patterns-established:
  - "The read path never spawns for a hostile value: scope values, provider names and the data dir are checked, and SftpOp refuses any unsafe path before sftp starts"

requirements-completed: []
requirements-advanced: [DPLN-01]

duration: 150min
completed: 2026-10-14
status: complete
---

# Phase 22 Plan 06: SftpTransport read half Summary

**`SftpTransport` reads a Linux gateway data directory and the declared-names ledger over system `sftp` with an existence-first, one-level-per-session protocol, and gives byte-identical plans to the local transport, proven against a real `sftp-server` through `sftp -D`.**

## Accomplishments

- **Read protocol** (`transport/sftp.rs`): sessions list `data_dir`, `config`, `config/resources`, the collections (`core`, the mode, `local`), modules, types (and for tags the four tag types, then providers), then one session per depth inside each scoped directory. All of it uses `@ls -la`. No batch line starts with `-`; a path is in a batch only after an earlier session listed it. Then chunked `get` sessions fetch the listed files into a per-read temp dir (under the shell's own 0700 dir when it has one), and a local walk (`walk_capped`) must find exactly the listed files at exactly the listed sizes. Hashing is `sha256_hex` on the downloaded bytes.
- **Failure model:** any non-zero exit, stderr byte or timeout is `transport_failed` (via `require_success`). Permission denied, a directory vanishing between sessions, a file missing locally after download, a size that differs from its listing, a symlink or special file, an unsafe or non-UTF-8 name, and a data dir without `config/resources` are all `transport_failed`, never an absent or empty resource. Only a name missing from its parent's listing reads as absent.
- **Ledger:** `read_ledger` descends `ign-deploy/<gateway>/<mode>/ledger.json` one listing per level, `None` when a level is absent, `deploy_ledger_invalid` for a symlink, a non-directory component, a non-regular ledger or one over `LEDGER_MAX_BYTES` (mirroring `LocalFsTransport`), and `transport_failed` when sftp itself fails.
- **`for_node`:** Linux ssh node builds `RemoteShell::from_ssh` + `SftpTransport` with `posix_root(ssh.data_dir)`; Windows is `deploy_tree_invalid` naming `<label>.ssh.os` until 22-13; a relative or glob `data_dir` is `deploy_tree_invalid` naming `<label>.ssh.data_dir`.
- **Sessions per read** (counted by `SftpTransport::sessions()`): 3 (data_dir, config, resources) + 1 (collections) + 1 (modules) + 1 (types) + one per depth below the type + one per download chunk. The fixture tree in `sftp_read_matches_local_read` takes 10; the minimal unit-test tree takes 8. A ledger read is one session per path component (4) plus one for the `get`.

## Task Commits

1. **Task 1: SftpTransport read half:** `6de5a8c` (feat)
2. **Post-review: enforce every resource cap:** `e8150bb` (fix)
3. **Required levels found by the real-protocol test:** `9524f58` (fix)
4. **Task 2: real-protocol tests and plan parity:** `b739633` (test)

(Plus the final docs commit with this summary.)

## Post-review fix: resource caps (coordinator security review)

The review flagged a cap bypass. Audit and fix of every cap in the read path (`e8150bb`):

| Cap | Before | After |
|---|---|---|
| Entries (`READ_MAX_ENTRIES`) | counted after the whole listing was parsed and stored, only for entries inside a scoped dir | counted while parsing (`ListingError::TooMany` at the offending line, nothing after it read), for every entry at every level, cumulative across directories, levels and collections of one read (`Budget` threaded through `list_level`, `descend`, `discover`, `list_types`, `read_ledger`). `.`/`..` lines are bounded separately |
| Per-file (`READ_MAX_FILE_BYTES`) | listing size checked before download; re-check after | unchanged before download; after each chunk the delivered size must equal the listed size; final read-back goes through `open_regular_file` and `take(cap + 1)` and refuses `len > cap`, a size that differs from the listing, and growth while reading |
| Total (`READ_MAX_TOTAL_BYTES`) | listing sum checked once before the single download | listing sum checked up front, then `downloaded + chunk_listed + file.size` is checked before each `get` is added to a chunk; chunks are at most 256 MiB / 10 000 files and each is verified before the next; the final read-back re-sums the real bytes |
| Recursive get | one `get -R` per scoped directory | removed from the read path: one `get` per listed file. A test pins that no batch contains `get -R` |
| Disk use | unbounded during a transfer | `run_download` measures the download dir every 50 ms while sftp runs and drops the batch future (kill on drop) when it exceeds listed bytes + 1 MiB; the temp dir is removed on drop |
| Stdout cap | implicit backstop | not relied on: entry counting is independent of it |

Tests added with mocked runners (each cap): `entry_cap_stops_the_parse_at_the_offending_line`, `entry_cap_rejects_one_huge_listing_in_its_first_session` (one session, a 200 001-entry listing), `entry_cap_is_cumulative_across_levels` (three levels each under the cap, fails in session 3; also `list_types` and `read_ledger` budgets), `caps_are_enforced_from_the_listing_before_any_download` (file cap and total cap, no `get` session issued), `file_cap_is_rechecked_through_the_handle` (sparse file of cap + 1 bytes), `oversize_download_is_aborted_and_cleaned_up` (listing says 10 bytes, server writes 8 MiB and stalls; aborted well under 10 s and the written file is gone), `total_cap_is_checked_before_each_chunk` (second chunk never starts), `recursive_get_is_never_issued`.

## Test results

- `cargo test -p ignition-core --lib deploy::transport::sftp`: 34 passed (parse, `posix_root`, protocol invariants against the scripted server, every failure shape, symlinks and special files, unsafe names, depth bound, required levels, ledger anomalies, caps).
- `cargo test -p ignition-core --test deploy_transport`: 28 passed (13 new in `sftp_read`): `sftp_read_matches_local_read`, `sftp_plan_matches_local_plan`, `sftp_spaces_and_commas`, `sftp_hostile_but_legal_names_are_inert`, `sftp_unsafe_names_are_refused`, `sftp_absent_mode_collection_is_empty`, `sftp_absent_ledger_is_none`, `sftp_ledger_present_round_trips`, `sftp_permission_denied_is_transport_failed`, `sftp_glob_data_dir_refused_before_spawn`, `sftp_get_skips_symlinks_documented`, `sftp_symlink_in_resource_is_refused`, `for_node_builds_sftp_for_linux_and_refuses_windows`.
- `cargo test -p ignition-core --test deploy_plan`: 57 passed; `PLAN_GOLDEN` and `PLAN_GOLDEN_LEDGER` unchanged.
- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` are clean.
- **Where the real-protocol tests ran:** the dev Mac (Darwin 25.6, OpenSSH 10.3p1, `/usr/libexec/sftp-server`). **CI status:** not yet run. Whether the ubuntu-latest leg has an `sftp-server` at one of the probed paths (RESEARCH A13) is confirmed only by the next CI run. If discovery fails there, `sftp_server_path()` panics with the probed paths and the package name (`openssh-sftp-server`); the fix is a CI install step, which is the user's push decision (`ci.yml` was not edited). The `ls -la` line layout was verified only against the macOS client and server; the Linux layout is assumed identical (same OpenSSH code) and will be confirmed by that run. The windows leg compiles the sftp tests out (`#[cfg(unix)]`).

## Deviations from Plan

**1. [Override] ops are typed `SftpOp`, not raw lines.** Followed the 22-05 post-review API; no raw-line escape hatch added.

**2. [Rule 2 - Missing critical functionality] `ls -la` everywhere instead of `ls -1`.** The override requires refusing symlinks and enforcing caps; `ls -1` shows neither the type nor the size, and hides dot-named entries that `get` would still deliver and the local transport reads. Added the typed variant `SftpOp::LsLongAll { path }` (same `safe_remote_path` validation, `@` quiet prefix, never a leading `-`) with a render test and a hostile-path case. `parse_ls_listing` therefore returns `ListedEntry` (name, kind, size) rather than names, and parses the long format; the exact listed-directory prefix is still required, and `.`/`..` are dropped.

**3. [Rule 2] Remote-derived names reach later batch lines.** The plan truth says no remote-derived name is ever interpolated. Refusing symlinks inside a resource tree (override) requires listing subdirectories by name, and per-file `get` requires file names. Every such name first passes `safe_remote_path` (control characters, quotes, backslash, glob characters), contains no `/`, is never `.`/`..`, and is quoted by the typed op, which re-checks it. `sftp_hostile_but_legal_names_are_inert` runs a name with `'`, `$(...)`, backticks, `;` and `!` against the real server.

**4. [Rule 1/2] `get -R` removed from the read path** (coordinator post-review item 4), so the plan's "session 5 downloads with `get -R`" became chunked per-file `get`. The OpenSSH behaviour that motivated the plan's T-22-28 acceptance (`get -R` silently skips symlinks) is still pinned by `sftp_get_skips_symlinks_documented`, but the risk is now closed by the listing check (`sftp_symlink_in_resource_is_refused`), not accepted.

**5. [Rule 1 - Bug] Missing `config` / `config/resources` read as empty.** Found by `sftp_absent_mode_collection_is_empty`: the level walk followed only names present in a listing. They are now required levels (`9524f58`), matching `LocalFsTransport::resources_root`.

**6. [Judgement] Session shape.** The plan listed `data_dir` and `config/resources` in one session and cached `ign-deploy` for `read_ledger`. Instead `data_dir -> config -> resources` are three single-level sessions (proving each is a real directory, not a symlink, like the local transport) and `read_ledger` does its own descent. Cost: two more handshakes per read than the plan's count, plus one per depth for the symlink and size listing. Added `READ_MAX_DEPTH = 64` so a hostile deep tree cannot drive unbounded sessions.

**7. [Judgement] Downloads go to a per-read `TempDir` created inside the shell's temp dir** (override: "from the RemoteShell temp dir") when it has one (the `-D` test seam has none and uses the system temp dir). Deleted on drop (T-22-29).

**8. [Process] TDD.** As in 22-05, the scripted-server tests and the implementation landed together in `6de5a8c`; there is no separate RED commit. The post-review caps and the required-levels bug were found by tests and fixed in separate `fix` commits.

**Total deviations:** 2 Rule 2 additions, 1 Rule 1 bug, 1 post-review change, 3 judgement calls. No architectural change.

## Known limitations

- **Dot-named entries and names the client escapes:** a name that `safe_remote_path` refuses (for example `odd[1]` or a backslash) makes the whole read `transport_failed` rather than being skipped. The local transport would read such a name.
- **Linux server only.** The Windows sftp path form and the `ls -la` layout on Win32-OpenSSH are 22-13 / C7a captures.
- **Listing columns:** the size is taken as the 4th token from the end of the pre-path columns, so owner and group names with spaces are tolerated; a server that prints a different column layout fails closed (`transport_failed`).
- DPLN-01 is not marked complete: Windows hosts and the write half remain.

## New packages

None. `Cargo.lock` unchanged (`tempfile`, `tokio` with `time`, `async-trait` were already direct dependencies).

## Known Stubs

None.

## Threat Flags

None beyond the plan's register. T-22-24 (no `-` prefix, existence-first, listing equals download), T-22-25 (names validated, typed ops), T-22-26 (listing parser, `walk_capped` on the local side), T-22-27 (now mitigated rather than accepted: caps checked before and during download), T-22-28 (now mitigated: symlinks refused from the listing), T-22-29 (per-read temp dir removed on drop) all have tests.

## Self-Check: PASSED

- Files present: `transport/sftp.rs`, `tests/common/sftp_server.rs`; `grep -n 'SftpTransport::new' transport/mod.rs` prints the `for_node` arm; `grep -n 'panic!' tests/common/sftp_server.rs` prints the loud-failure branch.
- Commits `6de5a8c`, `e8150bb`, `9524f58`, `b739633` present in `git log`.
- No `unwrap`, TODO or placeholder in the new non-test code; `.planning/config.json` and `.planning/research/.cache/` untouched.
