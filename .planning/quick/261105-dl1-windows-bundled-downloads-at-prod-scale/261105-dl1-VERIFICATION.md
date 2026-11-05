---
phase: quick-261105-dl1
verified: 2026-11-05T11:50:00Z
status: passed
score: 9/9 must-haves verified (live-VM numbers carried from SUMMARY, not re-run)
behavior_unverified: 0
overrides_applied: 0
gaps: []
---

# Quick 261105-dl1 Verification: bundled Windows downloads at production scale

**Goal (BRIEF):** Windows deploy reads fetch their files with a constant number of transfers (one archive) instead of one sftp `get` per file, keep every safety property, and are guarded by production-scale tests.
**Verified:** 2026-11-05 on `feat/windows-fast-listing` (12 commits ahead of main). Re-verification: No.
**Not run, by instruction:** live tests, any VM or prod host. Live figures below come from SUMMARY and are marked as such.

## Observable truths

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | Windows read fetches files as one archive, O(1) connections | VERIFIED | `SftpTransport::new_windows` sets `bundle_reads = true` (sftp.rs:735). The read path dispatches to `fetch_bundled` (sftp.rs:2414). `bundle_into` does 1 mkdir+put batch, 1 PowerShell BUNDLE, 1 get+rm+rmdir download. Test `windows_bundled_prod_shape_read_uses_one_transfer` passes (1 `get`, 7 connections for 14,270 files). |
| 2 | Archive built from the exact listed set, not a re-walk; 240+ char files stay excluded | VERIFIED | `render_list` takes the `FoundFile` list; BUNDLE checks the list SHA-256 before reading a name. `windows_read_unreadable_*` tests (long path, over-limit) edited to also assert no bundle list contains them; all pass. |
| 3 | Extraction refuses absolute, `..`, drive, backslash, symlink/device, duplicate, unlisted and missing entries; extracts into private `download_dir` | VERIFIED | `bundle::extract` (bundle.rs:254-): name rules, `unix_mode` regular-file check, encryption, method, duplicate (central-directory walk), membership both ways, `create_new` writes under the listed name's own components. Tests `windows_bundled_read_refuses_unsafe_entry_names` (8 name cases plus non-UTF-8) and `..._unlisted_or_irregular_entries` (symlink, char/block/socket/fifo, dir bits, no type bits, encrypted, LZMA, unlisted, duplicate) read and passing. |
| 4 | Sizes: each file equals listed size and fits `READ_MAX_FILE_BYTES`; total checked before transfer; enforced during transfer | VERIFIED | Pre-transfer total check in the read path (sftp.rs ~2386). Declared-size check, per-entry `take(size+1)` limit, running total vs listed total plus slack in `extract`. Download killed past reported archive size plus `DOWNLOAD_SLACK_BYTES` (sftp.rs:1840). `windows_bundled_read_bounds_sizes` passes. Final `verify_download` unchanged. |
| 5 | Zip bomb bounded | VERIFIED | Per-entry limit is exact (listed size + 1). `windows_bundled_read_bounds_sizes` includes the bomb archive. SUMMARY deviation 9 admits the running-total check is unreachable defence in depth with no dedicated test (Info). |
| 6 | Staged under the account home, never the data dir; removed on success and failure; at most one copy | VERIFIED | `fetch_bundled` refuses a home at or below the data dir (sftp.rs:1720-1726); staging dir is `<home>/.ign-ctl-*`; `get` session runs Rm + Rmdir; failure path runs `BUNDLE_CLEAN`. `windows_bundled_read_stages_only_below_the_home` passes. BUNDLE refuses without `archive_limit` + 256 MiB free. |
| 7 | Op timeout not raised; same `ObservedFiles` and hashes as the per-file path | VERIFIED | No timeout constant changed in the diff. `windows_bundled_and_per_file_reads_are_identical` passes. The production-size A/B pass is `#[ignore = "opt-in: ..."]` with a named reason, accepted by amendment 6. `windows_prod_shape_read_matches_the_reference` passes. |
| 8 | Production-scale tests exist, are real, RED first | VERIFIED | RED commit `7f12ed6` (tests only: sftp.rs tests, prod_shape.rs, deploy_prod_scale.rs) precedes fix `95bca62`. `deploy_prod_scale.rs` has 6 tests with real assertions; generator emits about 48 MB (`prod_shape_content_deflates_like_real_volume`). `prod_scale_pwsh_bundle` runs the real BUNDLE script under local pwsh and asserts the archive, sizes and extraction. |
| 9 | Live: read under 2 min, plan and verify timed, restore verified | VERIFIED-FROM-SUMMARY (not re-run) | SUMMARY: read 48.14 s with 7 connections; plan exit 0 in 136 s; verify exit 0 in 55 s; restore hash and count equal; VM deallocated. Harness `live_transport_wflwvm01.rs` exists with 4 `#[ignore = "live: ..."]` tests, each with a named opt-in env var. This verification did not contact the VM. |

## Test runs (this verification)

| Command | Result |
|---|---|
| `cargo fmt --check` | pass (exit 0) |
| `cargo test -p ignition-core --lib windows_` | 57 passed, 0 failed, 1 ignored (the opt-in 48 MB A/B pass), 31.5 s |
| `cargo test -p ignition-core --test deploy_prod_scale` | 6 passed, 0 failed, 97.8 s (load average about 20) |

`CARGO_TARGET_DIR` was a private scratchpad dir. `cargo clippy` and `cargo xwin clippy` were not re-run (not requested); SUMMARY claims they passed.

## Constraints

| Constraint | Result |
|---|---|
| No Cargo.toml, Cargo.lock, golden, fixture, `error.rs`, `target.rs` change | PASS: `git diff main...HEAD --name-only` lists 10 files, none of these |
| No slug or exit-code change | PASS: `error.rs` untouched; new strings are `transport_failed` details only |
| No IPs or prod host names in added lines | PASS: no dotted-quad, production addresses or host names in `git diff main...HEAD`. The test VM name `ign-win-836` and its resource group appear in added lines but already appear in main's tracked files (Info) |
| No co-author lines | PASS |
| Never committed: `.planning/config.json`, `.planning/research/.cache/`, `the local deploy-account script`, `.planning/debug/tg-pg-table-check.md` | PASS for this branch: no commit in `main..HEAD` touches `.planning/` or the ps1. Note: `.planning/config.json` is tracked on main (commits 591a640, 66d5307) and is currently modified in the working tree; it must stay unstaged. The other three are untracked and in no commit |
| Debt markers and skips in added lines | None. `#[ignore]` appears 4 times, each with a reason string and a named opt-in |
| POSIX unchanged | PASS by inspection: the sftp.rs deletions are the Windows `cross_check`; POSIX `list_level` and `fetch_files` stay (`bundle_reads` false outside `new_windows`). The POSIX offline and local tests (`prod_scale_posix_sftp_read`) pass |

## Findings

No BLOCKER. No WARNING.

Info:
- A Windows read now writes a `.ign-ctl-*` dir under the deploy account's home. SUMMARY, README and module docs say so. A read killed from outside leaves the dir (T-dl1-08, documented residual).
- BUNDLE reads the host files by path after listing. A file swapped for a reparse point between listing and bundling would be followed. The old per-file `get` had the same window, and the size check still has to match. Not a regression.
- Cold first BUNDLE after a fresh seed ran over 80 s live (SUMMARY). Still about 13x under the op timeout.
- Windows-only rows (`prod_scale_windows_sftp_read`, `conformance_ps_*`) and PowerShell 7 on Windows zip attributes are compiled only; the BRIEF accepts Windows CI as the confirmer.
- `cargo install` was run with `--locked --offline` because of an Aikido block on a newer winnow; `Cargo.lock` is unchanged.

## Status: passed

Goal achieved. The live figures rest on SUMMARY evidence plus the live harness code; I did not re-run them.
