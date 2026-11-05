---
phase: quick-261104-s3n
plan: 01
subsystem: deploy-transport
tags: [windows, sftp, powershell, prod-scale, testing]
status: incomplete
blocker: "The live full read of the production-shape tree does not finish in 2 minutes after the fix: the listing does (list_tree 21.75 s), but downloads cost about 0.37 s per sftp get on the test VM, 14,270 files is about 88 minutes, and the 10,000-file download chunk passes the 1,050 s op timeout, so `ign deploy plan` fails exit 4. The BRIEF puts downloads out of scope (measure, do not fix silently), so this needs a decision. `ign deploy verify` could not run at all: the test profile's token is in the macOS keychain and every new ign build blocks on an interactive keychain approval."
requires: []
provides:
  - "Windows deploy reads list with a fixed number of PowerShell calls (discover_windows)"
  - "tests/common/prod_shape.rs production-shape generator"
  - "offline connection-count, local timed and live production-scale gates"
affects: [deploy plan, deploy verify, deploy apply, deploy pull (Windows ssh nodes)]
tech-stack:
  added: []
  patterns:
    - "Windows read listing: one ENUMERATE_CHILDREN pass over the path folders, one ENUMERATE pass over the requested roots"
    - "test fixtures generated deterministically at production shape, never checked-in blobs"
key-files:
  created:
    - crates/ignition-core/tests/common/prod_shape.rs
    - crates/ignition-core/tests/deploy_prod_scale.rs
  modified:
    - crates/ignition-core/src/deploy/transport/sftp.rs
    - crates/ignition-core/src/deploy/transport/winps.rs
    - crates/ignition-core/tests/common/mod.rs
    - crates/ignition-core/tests/live_transport_wflwvm01.rs
    - README.md
decisions:
  - "Windows reads list the folders on the path with one batched ENUMERATE_CHILDREN call and walk the requested trees with one batched ENUMERATE call; the sftp-versus-walk cross_check is removed because the walk is now the listing"
  - "A reparse point on the path to a requested folder is now refused on Windows (new strictness)"
  - "Long entries (240+) are held to the Windows component gate only; short entries to safe_remote_path"
  - "The live gate's identity guard reads the gateway's own systemName from its system-properties resource instead of calling gateway-info, so it needs no API token"
  - "The live seed and restore write through the az run-command admin route when the deploy account cannot write the core tag folders (it cannot on ign-win-836)"
metrics:
  duration: "about 2 h 10 min"
  completed: 2026-11-05
actuals:
  tokens: 34946
  tasks: 3
  commits: 5
---

# Phase quick-261104-s3n Plan 01: Windows fast listing at production scale Summary

On a Windows host a deploy read now lists the production tag tree with 2 PowerShell calls (`list_tree`) or 3 (`read`) instead of 20 connections and 7,590 sftp `ls` lines. A deterministic production-shape generator and three test layers (offline connection count, local timed rows, live VM gate) now guard every deploy transport change. The live gate shows the listing at 21.75 s on the test VM. The full read still takes far longer than 2 minutes because of downloads, which the BRIEF puts out of scope. That is the open blocker.

## Commits

| Task | Commit | What |
|---|---|---|
| 1 (RED) | `5faea3c` | `test(261104-s3n): production-shape fixture and offline connection-count tests (RED)` |
| 2 (GREEN) | `23100e3` | `fix(261104-s3n): list Windows deploy reads with one PowerShell walk instead of per-level sftp` |
| 3A | `53733b0` | `test(261104-s3n): production-scale timed rows` |
| 3D | `5b726b4` | `docs(261104-s3n): Windows reads list with one PowerShell walk; production-scale gate` |
| 3C | `3054a52` | `test(261104-s3n): live Windows production-shape gate` |

No `perf` commit: the walk script stayed under its trigger (local pwsh 5.3 to 6.3 s; the live listing was 21.75 s, under the 60 s trigger).

## RED on the pre-fix code (Task 1)

The generator test passed. Both offline tests passed identity on the pre-fix code, then failed on the connection bound. Output is in the session scratch file `s3n-t1-red.out`:

```text
list_tree of 21619 entries: 20 connections (19 sftp sessions, 1 PowerShell calls, 7590 `ls` lines), sessions() = 19
thread '...windows_prod_shape_list_tree_uses_constant_connections' panicked at crates/ignition-core/src/deploy/transport/sftp.rs:7447:13:
list_tree opened 20 connections
read of 21619 entries: 20 listing connections (19 sftp sessions, 1 PowerShell calls, 7591 `ls` lines), 2 download sessions for 14270 files, sessions() = 21
thread '...windows_prod_shape_read_uses_constant_connections' panicked at crates/ignition-core/src/deploy/transport/sftp.rs:7526:13:
read opened 20 listing connections
test result: FAILED. 0 passed; 2 failed
```

Pre-fix `sessions()` also left out the PowerShell call: 19 reported against 20 real connections.

## Connection counts, offline, production shape against the tiny tree

| | pre-fix (`5faea3c`) | fixed |
|---|---|---|
| `list_tree` (both core tag types) | 20 (19 sftp + 1 PowerShell), 7,590 `ls` | 2 PowerShell calls, 0 `ls`; tiny tree also 2 |
| `read` listing (one provider) | 20 (19 + 1), 7,591 `ls` | 3 PowerShell calls, 0 `ls`; tiny tree also 3 |
| `read` downloads | 2 sessions for 14,270 files | unchanged: 2 sessions for 14,270 files (1 for the tiny tree's 159) |
| `sessions()` | sftp only | every connection (asserted equal to the fake's count) |

The read takes 3 calls because the 21 path folders of 3 collections times 4 tag types split into 2 `ENUMERATE_CHILDREN` chunks under the 8,000-character command budget, plus 1 walk.

## Local production-scale budgets (Task 3A, `tests/deploy_prod_scale.rs`)

Measured 2026-11-04 on an Apple M1 Max (10 cores), debug profile, three runs:

| Row | Runs | Budget |
|---|---|---|
| `prod_scale_local_read` | list_tree 0.48 / 0.36 / 0.49 s; read 7.59 / 6.29 / 7.99 s | 25 s |
| `prod_scale_posix_sftp_read` (local `sftp-server`) | list_tree 2.06 / 2.63 / 2.20 s (19 sessions); read 19.30 / 17.24 / 19.39 s | 60 s |
| `prod_scale_pwsh_walk` (`ENUMERATE`, PowerShell 7) | 6.27 / 5.46 / 5.28 s | 20 s |
| `prod_scale_windows_sftp_read` (cfg(windows)) | not measured: no Windows runner here | 120 s |

Each budget is about 3 times the slowest run. Risk: a CI runner more than about 3 times slower than this Mac in debug would be flaky. Windows CI must confirm `prod_scale_windows_sftp_read` (identity, at most 3 connections for `list_tree`, under 120 s); it has only been compiled (xwin clippy).

## Stdout headroom

The walk of the 21,619-entry tag-definition tree printed 1,511,045 bytes, about 70 bytes per entry. Against the 32 MiB stdout cap that is room for about 480,000 entries, so the `READ_MAX_ENTRIES` cap (200,000) trips first. Non-ASCII names grow as `\uXXXX` escapes and shrink that margin.

## Live, ign-win-836 (Task 3C)

Setup:
- The VM was deallocated. I started it, read its current address from `az` and exported it in the shell only. The env file's address matched, and the pinned `known_hosts` already held the address. No known_hosts copy was needed. StrictHostKeyChecking stayed `yes`.
- I deallocated the VM again at the end.
- Before binary: `5faea3c` built under the scratch root (`ign 1.6.0`). After binary: `cargo install --path crates/ignition-cli --force` at `5b726b4` plus the uncommitted live test.
- `~/.cargo/bin/ign` is now this branch build. It reports `1.6.0` because that is the workspace version; the binary it replaced reported `1.6.6`.

Original core tag folders, from the manifest:
- `tag-definition`: 14 files, 14 dirs, sha256 `5ce9970dd095f09db32b6188eeffc4b4cf5af8778cb3c0532dcbf8e6c09ff0dc`
- `tag-type-definition`: 2 files, 2 dirs, sha256 `489082e41ac7cab78981bb6b2a65f4c0b2387256554fb6f4fa34eb37376a3255`

Each step ran as a separate command. Exit codes and wall times (wall includes cargo build time):

| UTC start | Step | Exit | Wall | Result |
|---|---|---|---|---|
| 05:03:32 | seed (1) | 101 | 184 s | robocopy exit 16 into `core\...\ProdShape`: the deploy account has only ReadAndExecute on both core tag folders on this VM (icacls read back), so the README's Modify grant is not on this host. Nothing was written to core. |
| 05:07:11 | restore (1) | 0 | 39 s | manifest hash and counts equal; `ign-prod-shape` removed |
| 05:14:01 | seed (2), admin route added | 101 | 115 s | test-helper bug (the plain-path check refused the drive colon) before any core write |
| 05:16:32 | restore (2) | 0 | 244 s | equal, through the admin route |
| 05:20:54 | seed (3) | 0 | 137 s | admin route (`az vm run-command` as SYSTEM); seeded 14,119 files / 7,500 dirs and 156 / 85, identical to the generator |
| 05:23:38 | `ign deploy plan`, after binary | 4 | 1,081 s | `transport_failed`: "timed out after 1050s and was killed". The listing finished in about 30 s. The first 10,000-file download session ran into the op timeout. |
| 05:42:00 | `wflwvm01_prod_shape_reads_are_fast` | 101 | 188 s | `list_tree` 21.75 s, 2 connections, identical to the generator (21,647 + 245 entries, 28 + 4 outside ProdShape). Walk alone of tag-definition/ProdShape: 21,618 entries in 20.21 s. `read` was cut off at 120.45 s with 4 connections (3 listing calls plus 1 download session): **not under 2 minutes**. |
| 05:45:25 | `ign deploy plan`, before binary | 142 (SIGALRM) | 1,200 s | > 20 min, killed at cap, still in per-level sftp listing (one session open for minutes on a large level) |
| 06:05:5x | restore (3) | 0 | 177 s | admin route; both folders equal the stored manifest: same exists flag, files, dirs and combined sha256 (values above); `ign-prod-shape` removed |
| 06:09:26 | gateway status | - | - | `StatusPing` `{"state":"RUNNING"}`, HTTP 200 |

Before and after:

| | before (`5faea3c`) | after |
|---|---|---|
| listing of the production-shape core tag trees | > 20 min (killed at cap; about 100 min projected from 0.8 s per folder) | 21.75 s (`list_tree`), about 30 s inside `plan` |
| `ign deploy plan` | > 20 min, killed at cap | exit 4 at 1,081 s: download op timeout |
| `ign deploy verify` | not run | not run |

Download cost, measured live (for the BRIEF's "measure, do not fix"):
- The temp dir of the after-binary plan held 1,807 files after about 710 s of its first download session and 2,294 about 180 s later. That is about 0.37 to 0.39 s per `get`.
- The link round trip is 60 to 90 ms (TCP connect to the VM), so a `get` costs several round trips.
- At that rate the read's 14,270 files take about 88 minutes. One 10,000-file chunk (about 3,900 s) is far over the 1,050 s default op timeout.
- Offline session count at production scale: 2 download sessions. Live: the first session never finished.

`ign deploy verify` could not run, before or after:
- The `azure-win` profile's token lives in the macOS keychain. A freshly built binary blocks on an interactive keychain approval.
- Observed: `ign --profile azure-win api call ... gateway-info` hung for more than 170 s, and I killed it (my own process). The same call with `IGNITION_TOKEN_AZURE_WIN` set to a dummy value returned `auth_rejected` at once.
- `deploy_verify` asks `scan_status` before it reads, so it cannot get past the credential.
- I did not try to read the token out of the keychain.

## Behaviour differences from the pre-fix code

- **A reparse point on the path is now refused.** A junction or symlink at `config`, `config/resources`, a collection, `ignition`, or a type folder on the way to a requested folder is now exit 4 naming it. The pre-fix sftp listing showed a junction there as a directory and followed it.
- **Long entries are held to the Windows gate only.** An entry at or over 240 characters is never downloaded, so only `parse_enumeration`'s Windows component gate and `RelPath::new` apply. The pre-fix code also held a 240 to 259 entry to the batch-line gate, because sftp listed it.
- **The sftp-versus-walk comparison is removed.** `cross_check` is deleted. The walk is the listing, and `get` plus `verify_download` (exact listed size) check the downloads.
- **The path listing applies `safe_remote_path` to every name it lists**, as the pre-fix `NameRule::Refuse` listing did. Duplicate and case-twin names are refused, with the path named.
- **`sessions()` now counts the read path's PowerShell calls.** `cross_check_names` (`list_tag_providers`) is counted too.
- **Error wording.** The path listing and walk failures keep the "cannot check the listing for completeness" wrapper, and the N2 detail keeps "did not return". A missing data dir is now "the data directory cannot be read (not found); ...". The pre-fix code showed the sftp `ls` failure text. Slugs and exit codes are unchanged.
- **Unchanged:** `list_types`, `list_tag_providers` and `descend` still list per level over sftp. Their cost does not grow with the tag tree. POSIX `discover` is byte-for-byte unchanged.

## Edited tests and why

| Test | Change |
|---|---|
| `windows_read_collects_the_same_tree_and_checks_it_against_a_walk` | asserts no `@ls` line instead of an `@pwd` line (no sftp listing any more) |
| `windows_list_tag_providers_cross_check_lists_direct_children_only` | second half: the read keeps a recursive walk; its non-recursive scripts list only path folders, none at or below a requested root (new `script_roots` helper) |
| `windows_list_tree_is_checked_against_a_walk` | the short entry hidden from sftp is now listed; the long and reparse cases are kept |
| `windows_list_tree_refuses_a_type_folder_sftp_omits` -> `..._made_while_it_is_read` | (a) +2 calls; (b) and (c) now list; (d) N2 via the new appear knob |
| `windows_read_refuses_a_requested_root_sftp_omits` -> `..._made_while_it_is_read` | (b) and (c) now read; (d) N2 via the appear knob |
| `windows_cross_check_counts_long_entries_sftp_omits` -> `windows_discover_charges_every_walked_entry_once` | against `discover_windows`: every entry charged once, at the cap Ok, one more is the exact detail |
| `windows_read_fails_closed_when_the_listing_omits_an_entry` -> `windows_read_reads_an_entry_the_sftp_listing_would_omit` | the file is read with identical bytes |
| `windows_read_fails_closed_on_walk_failure_and_disagreement` | the walk failure keeps the wrapper; the extra entry fails at `get` naming it; the size case uses the new walk size lie and fails at the download size check; the out-of-scope case is unchanged |
| `windows_cross_check_does_not_let_a_sibling_satisfy_another` -> `windows_read_does_not_let_a_sibling_satisfy_another` | a sibling's sftp-hidden file is read under the sibling; case and suffix twins still fail naming the entry |

New tests:
- `windows_read_refuses_a_bad_folder_on_its_path`: reparse point on the path, file on the path, missing config, missing resources, missing data dir
- `windows_read_fails_when_a_listed_root_vanishes_before_the_walk`
- `windows_read_holds_walked_names_to_the_batch_line_gate`: `[` refused when short and listed unreadable when long, case twins
- `windows_read_refuses_a_walked_tree_deeper_than_the_cap`
- `windows_discover_lenient_skips_an_unsafe_name_and_its_subtree`
- `enumerate_children_chunks_split_by_budget_and_cover_every_root`

FakeSftp changes, all test-only:
- range scans
- the walk adds what it reports to `listed`
- a symlink is reported as a reparse point
- new knobs for appear, vanish and walk size lie

Every other existing test, POSIX included, passes unedited.

## Deviations from Plan

1. **[Rule 3] Generator include path.** The plan's `#[path]` inside `mod tests` resolves through the nonexistent `sftp/tests/` directory and fails. It is now a top-level `#[cfg(test)] #[path = "../../../tests/common/prod_shape.rs"] mod prod_shape;`.
2. **[Rule 1] The timed rows use a tokio mutex.** A std `Mutex` held across `.await` fails clippy `await_holding_lock`.
3. **[Rule 3] Live identity guard.** I replaced gateway-info with the gateway's own `systemName` read from `core\ignition\system-properties\config.json` over the pinned-key ssh session. gateway-info needs the keychain token (see above). The on-disk name is the same value, and no token leaves the machine. `IGNITION_URL` is therefore not needed by the gate.
4. **[Plan-sanctioned fallback] Admin route.** The deploy account cannot write the core tag folders on ign-win-836. The seed copy and the restore removal and mirror run as SYSTEM through `az vm run-command`, chosen by a write probe. Backup, manifest and reads stay as the deploy account.
5. **[Rule 1] `reads_are_fast` restructure.** It holds `read` to 120 s with `tokio::time::timeout` and asserts `list_tree` before the read, so a slow download still reports the listing verdict. The walk-alone timing now runs before the read.
6. **xwin gate invocation.** Homebrew's `cargo` comes first on PATH and has no Windows target, so the gate ran as `PATH=$HOME/.cargo/bin:$PATH cargo xwin clippy ...` (the rustup toolchain, which has `x86_64-pc-windows-msvc`).
7. **PLAN `</output>`.** The PLAN file already had a single trailing `</output>`, so nothing was removed.
8. **STATE.md / ROADMAP.md** were not touched. The orchestrator owns them for this quick task.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass, run again after the live test landed |
| `cargo test --workspace` | 3,205 passed, 0 failed (before the live test commit); the live test file's offline tests afterwards: 15 passed, 22 ignored |
| `cargo xwin clippy --workspace --all-targets --target x86_64-pc-windows-msvc -- -D warnings` | pass, run again after the live test landed |
| Hygiene | `git diff b24fb19..HEAD` touches no Cargo version, golden, fixture or `error.rs`; added lines hold no dotted-quad address or production host name; no co-author lines; only explicit paths staged; none of the four never-stage paths staged; nothing pushed |

## What Windows CI must confirm

- `prod_scale_windows_sftp_read`: real `sftp-server.exe` plus local PowerShell at production shape. Identity, at most 3 `list_tree` connections, each operation under 120 s. Note the local temp path makes some entries long.
- The Windows conformance rows (`conformance_ps_*`) that read through the changed Windows listing.

## Known Stubs

None.

## Threat Flags

| Flag | File | Description |
|---|---|---|
| threat_flag: admin-route | crates/ignition-core/tests/live_transport_wflwvm01.rs | The live gate can run PowerShell as SYSTEM on the test VM through `az vm run-command` (test-only, behind two opt-ins, plain-path checked; the same route `live_project_host.rs` uses) |

## Self-Check: PASSED

- Files exist: `crates/ignition-core/tests/common/prod_shape.rs`, `crates/ignition-core/tests/deploy_prod_scale.rs`, and the modified `sftp.rs`, `winps.rs`, `live_transport_wflwvm01.rs`, `README.md`.
- Commits exist on `feat/windows-fast-listing`: `5faea3c`, `23100e3`, `53733b0`, `5b726b4`, `3054a52`.
- The plan is not complete: the live under-2-minute full read is not met (see blocker), and `verify` could not run.

## Closed by 261105-dl1

The download gap that left this task incomplete was closed by quick task 261105-dl1 (bundled Windows downloads): the live production-shape read now takes 48 s, `plan` 136 s and `verify` 55 s.
