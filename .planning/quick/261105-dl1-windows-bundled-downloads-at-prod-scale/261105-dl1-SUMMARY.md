---
phase: quick-261105-dl1
plan: 01
subsystem: deploy-transport
tags: [windows, sftp, powershell, zip, prod-scale, testing]
status: complete
requires: [quick-261104-s3n]
provides:
  - "Windows deploy reads fetch their files as one archive (fetch_bundled): 1 sftp get, 7 connections at production shape"
  - "bundle.rs: read-list rendering, archive bound, hostile-archive-safe extractor"
  - "winps BUNDLE and BUNDLE_CLEAN scripts"
  - "production-scale gate now covers downloads: offline transfer/hostile/equivalence tests, timed pwsh row, live read gate"
affects: [deploy plan, deploy verify, deploy pull, deploy apply (Windows ssh nodes)]
tech-stack:
  added: []
  patterns:
    - "Windows read download: upload exact list, one PowerShell zip into <home>/.ign-ctl-*, one get + rm + rmdir, local extraction under hostile-archive rules, then the unchanged verify_download"
    - "extractor order: framing, every central record (name, type, encryption, method) with nothing inflated, membership both ways, declared sizes, then bounded inflation"
key-files:
  created:
    - crates/ignition-core/src/deploy/transport/bundle.rs
  modified:
    - crates/ignition-core/src/deploy/transport/sftp.rs
    - crates/ignition-core/src/deploy/transport/winps.rs
    - crates/ignition-core/src/deploy/transport/mod.rs
    - crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs
    - crates/ignition-core/tests/common/prod_shape.rs
    - crates/ignition-core/tests/deploy_prod_scale.rs
    - crates/ignition-core/tests/live_transport_wflwvm01.rs
    - README.md
decisions:
  - "Container: zip through System.IO.Compression.ZipArchive (no new crate, no probe, in-process API), not tar.exe"
  - "Transfer: one sftp get of a staged archive, not an uncapped ssh stream (process seam keeps file content off stdout)"
  - "Staging: a fresh <home>/.ign-ctl-<nonce> dir (the existing ctl_name), refused when the home is at or below the data dir"
  - "Always bundle on Windows when a read has a file to fetch; per-file stays for POSIX and as a test-only switch"
  - "A DOS-made record with no unix mode (external attributes 0) counts as a regular file: that is exactly what Windows PowerShell 5.1 writes (confirmed live on all 14,114 records)"
  - "Amendment 6: the production pass of the per-file/bundled equivalence test is opt-in (--ignored); the lib suite grew by about 150 s with it"
metrics:
  duration: "about 13 h 30 min wall (from 05:00 to 18:40 UTC), much of it waiting on a heavily loaded machine and the keychain checkpoint"
  completed: 2026-11-05
actuals:
  tokens: 72000
  tasks: 5
  commits: 7
---

# Phase quick-261105-dl1 Plan 01: Windows bundled downloads at production scale Summary

On a Windows ssh host, a deploy read now moves its files as one zip archive. ign uploads the exact file list, one PowerShell call zips those files into a temporary `.ign-ctl-*` directory below the deploy account's home, and one sftp `get` downloads the archive and removes it. Extraction runs under hostile-archive checks. A read used to need one `get` per file: about 14,270 of them, taking about 88 minutes. On the test VM, the production-shape read now takes 48 s with 7 connections. `ign deploy plan` went from exit 4 after 1,081 s to exit 0 in 136 s, and `ign deploy verify` ran for the first time: green in 55 s.

## Commits

| Task | Commit | What |
|---|---|---|
| 1 (RED) | `7f12ed6` | `test(261105-dl1): realistic prod-shape content and bundled-download tests (RED)` |
| 2 (GREEN) | `95bca62` | `fix(261105-dl1): fetch Windows deploy reads as one archive instead of one sftp get per file` |
| 3a-i A | `7fcaf40` | `test(261105-dl1): production-scale timed row for the bundled Windows download` |
| 3a-i B | `42b9ba2` | `docs(261105-dl1): Windows reads fetch one archive; where a Windows read stages` |
| 3a-i fix | `0b16d0a` | `test(261105-dl1): hold the production-scale lock for each row's whole run` |
| 3a-ii fix | `47d195f` | `fix(261105-dl1): split the read list with an overload Windows PowerShell 5.1 has` |
| 3a-ii E | `803d5a3` | `test(261105-dl1): live Windows bundled-read gate` |

## Before and after

| | before (`3054a52`, per-file `get`) | after |
|---|---|---|
| Offline production-shape read | 14,270 `get` lines, 5 connections (2 sftp + 3 PowerShell), `sessions()` = 5 | 1 `get`, 7 connections (3 sftp + 4 PowerShell), `sessions()` = 7; the tiny tree is also 1 and 7 |
| Live `read` of the seeded tree | cut off at 120 s; projected about 88 min | **48.14 s, 7 connections**, 14,270 files as one archive, 5 unreadable |
| Live `ign deploy plan` | exit 4 `transport_failed` at 1,081 s (download chunk passed the 1,050 s op timeout) | **exit 0 in 136 s**: 6,688 ProdShape units (`unmanaged`), 2 unreadable |
| Live `ign deploy verify` | could not run (keychain) | **exit 0 in 55 s**: green; every check passed except `probes`, which warned that the manifest declares none (see Live) |

The RED output is in the session scratch file `dl1-t1-red.out`. At `7f12ed6` the transfer test failed on the `get` count (`left: 14270, right: 1`) after its identity check passed. Every other `windows_bundled_*` test failed because `read()` returned Ok. The reference and non-ASCII tests passed on the per-file path.

A small Windows read now costs more connections: `windows_tree` takes 6 bundled against 3 per-file. That is the accepted cost of decision D-dl1-4.

## Content compressibility

The generator's filler is now words from a fixed 128-word vocabulary, picked by xorshift64* seeded with `fnv(rel)`. The plan suggested about 4,096 words, but that gave only 1.60x; 128 words gives 2.60x.

| Archive | Raw | Packed | Ratio |
|---|---|---|---|
| zip crate default Deflate (`prod_shape_content_deflates_like_real_volume`) | 47.8 MB | 18.4 MB | 2.60 |
| real BUNDLE under local pwsh 7, both trees | 51.5 MB | 20.2 MB | 2.55 |
| real BUNDLE under Windows PowerShell 5.1, live, tag-definition | 47.8 MB | 19.3 MB | 2.48 |

## Local budgets (`tests/deploy_prod_scale.rs`)

`prod_scale_pwsh_bundle` was measured on 2026-11-05 on an Apple M1 Max, debug profile, while the machine carried other work (see Environment).

| Run | BUNDLE | extract |
|---|---|---|
| 1 | 13.35 s | 7.59 s |
| 2 | 8.59 s | 5.74 s |
| 3 | 2.98 s | 4.86 s |
| later passing runs | 3.00 / 5.55 s | 4.60 / 3.96 s |
| **budget** | **40 s** | **25 s** |

Each budget is about three times its slowest run. The other rows are unchanged. In the final gate run they took: local read 5.87 s, POSIX sftp read 29.49 s, pwsh walk 6.57 s.

`prod_scale_windows_sftp_read` (cfg(windows)) now also asserts at most 8 read connections and no new `.ign-ctl-*` entry in the account home. It has only been compiled, with xwin clippy.

## Amendment 6: wall time of the 48 MB fake tests

| Test (debug, standalone) | Measured |
|---|---|
| `windows_bundled_prod_shape_read_uses_one_transfer` (prod and tiny tree) | 113 s; 66 to 414 s inside the parallel transport suite |
| `windows_prod_shape_read_matches_the_reference` | 43 s; 66 to 413 s in the suite |
| per-file plus bundled production passes of the A/B test | 147 s |

- **Growth:** the ignition-core lib suite took 102.4 s without the three tests and 250.8 s with them, back to back at load 136 to 189. That is about 148 s of growth, over the 60 s threshold.
- **Split:** the production pass of the equivalence test is now opt-in as `windows_bundled_and_per_file_prod_reads_are_identical` (`#[ignore]`; run it with `--ignored`). It passed when run. The always-on `windows_bundled_and_per_file_reads_are_identical` covers the tiny production-shape tree and a tree with mode, local and unreadable entries.
- **Fake speed-up:** the fake's BUNDLE zips at Deflate level 1.
- **Ceiling:** each production-scale fake test asserts `PROD_FAKE_CEILING` = 900 s, a hang detector set at about twice the slowest loaded run. The transfer bound is the `get` and connection counts, not the clock.

## Live, ign-win-836

**Setup**
- The VM was deallocated. I started it at 14:09 UTC.
- Its current address matched the env file and the pinned `known_hosts`, so no session copy was made. StrictHostKeyChecking stayed `yes`, and the address was exported in the shell only.
- The daily 03:00 UTC auto-shutdown left about 13 h.

**Original core tag folders** (from the seed's manifest; identical to 261104-s3n):
- `tag-definition`: 14 files, 14 dirs, sha256 `5ce9970dd095f09db32b6188eeffc4b4cf5af8778cb3c0532dcbf8e6c09ff0dc`
- `tag-type-definition`: 2 files, 2 dirs, sha256 `489082e41ac7cab78981bb6b2a65f4c0b2387256554fb6f4fa34eb37376a3255`

Each step ran as its own command, and no exit code was masked.

| UTC | Step | Exit | Wall | Result |
|---|---|---|---|---|
| 14:48 | seed | 0 | 159 s | admin route; 14,119 files / 7,500 dirs and 156 / 85 seeded |
| 14:51 | `reads_are_fast` (1) | 101 | 165 s | `list_tree` 20.71 s, 2 connections; walk alone 18.06 s; **read cut off at 120.02 s with 6 connections** (BUNDLE still running) |
| 14:55 | `reads_are_fast` (2), bundle alone moved ahead of the read | 101 | 62 s | BUNDLE failed on PS 5.1: `ArgumentOutOfRangeException` at `Substring(0,$j)` (the split bug, Deviation 2); BUNDLE_CLEAN answered ok |
| 14:57 | `reads_are_fast` (3), after the fix | **0** | 154 s | PASS: `list_tree` 18.85 s, 2 connections; walk alone 17.39 s; **read 48.14 s, 7 connections**; home `.ign-ctl-*` entries 0 before, 0 after |
| 18:27 | `ign deploy plan --env live --json` | **0** | **136 s** | 6,688 units, all `unmanaged` (the manifest declares no units), 2 unreadable, `has_changes` false |
| 18:30 | `ign deploy verify --env live --json` | **0** | **55 s** | `ok: true`. scan, mode, read_back, `files`, loaded: pass. `probes`: warn (no `verify.tag_probes`). `files` compared 0 declared units; its warnings name the unreadable ProdShape `Archive` units, so the read ran |
| 18:31 | restore | **0** | 181 s | `PASS wflwvm01_prod_shape_restore: manifest hash and counts equal`; both folders equal the manifest above; `ign-prod-shape` removed |
| 18:35 | `ign --profile azure-win status --json` | 0 | | `state: RUNNING` |
| 18:35 | `az vm deallocate` | 0 | 35 s | `VM deallocated` |

**Bundle alone (run 3), the share of each step** (14,114 tag-definition files, 47.8 MB, a 19.3 MB archive):

| Step | Time |
|---|---|
| upload | 2.09 s |
| BUNDLE | 6.84 s |
| get | 4.68 s |
| extract | 4.16 s |

All 14,114 central records are (made-by host 0, external attributes 0). That is the .NET Framework shape that `zip_of` reproduces and the extractor accepts.

**Op-timeout headroom.** The largest single call was the read's listing walk, about 18 s, followed by BUNDLE at 6.8 s warm. That leaves more than 50x headroom under the 1,050 s default op timeout, which was not raised. One caveat: the first BUNDLE after the seed (run 1) was still running more than 80 s after it started. The likely cause is first access to freshly written files (Defender scanning or a cold file cache); this was not measured further. Even 80 s is about 13x under the op timeout, but a production read of freshly applied files may run slower than the warm 48 s.

The installed binary is `~/.cargo/bin/ign` (`ign 1.6.0`), built from `803d5a3` (Deviation 4). The keychain prompt was approved with Always Allow before the timed verbs ran.

## Edited tests and why

| Test | Change |
|---|---|
| `windows_read_holds_walked_names_to_the_batch_line_gate` | "never fetched" now also checks every uploaded read list (`never_fetched` helper) |
| `windows_read_unreadable_over_limit_hidden_mode_file` | same |
| `windows_read_unreadable_over_limit_listed_file_is_not_fetched` | same |
| `windows_read_unreadable_when_every_entry_reaches_the_limit` | also asserts no bundle list, no mutating line, nothing staged |
| `windows_read_fails_closed_on_walk_failure_and_disagreement` | the extra entry now fails as BUNDLE `missing` ("... extra.json is not on the host any more"); the size lie as `changed` ("was listed as 99"); both leave nothing staged |
| `windows_prod_shape_read_uses_constant_connections` | listing connections are counted as enumeration calls plus `ls` sessions; the per-chunk download equality is dropped (the transfer test owns the download bound) |
| `windows_bundled_and_per_file_reads_are_identical` | split per amendment 6 (above) |
| `prod_scale_*` rows and `prod_shape_content_deflates_like_real_volume` | hold `TIMED` for their whole run, materializing included |

The last change came from the workspace gate. With the new bundle row and the ratio test running beside it, `prod_scale_local_read` timed its read at 80 s against a 25 s budget. On its own it takes 9 s.

New tests:
- sftp.rs: `windows_bundled_prod_shape_read_uses_one_transfer`, `windows_bundled_read_refuses_unsafe_entry_names`, `windows_bundled_read_refuses_unlisted_or_irregular_entries`, `windows_bundled_read_bounds_sizes`, `windows_bundled_read_refuses_bad_host_answers`, `windows_bundled_read_stages_only_below_the_home`, `windows_prod_shape_read_matches_the_reference`, `windows_read_reads_a_non_ascii_name_identically`, and the two A/B tests
- winps.rs: `winps_bundle_zips_exactly_the_listed_files` (cases a to f), `winps_bundle_clean_removes_only_the_staging_files`, `bundle_commands_fit_the_budget` (7,398 and 2,994 characters with 150-character paths, against 8,000), `bundle_splits_its_list_with_an_overload_ps51_has`
- bundle.rs: `render_list_is_size_tab_name_lf_in_order`, `archive_limit_arithmetic`, `name_rules`, `central_walk_counts_a_crafted_duplicate`, `extract_round_trips_an_honest_archive`
- deploy_prod_scale.rs: `prod_shape_content_deflates_like_real_volume`, `prod_scale_pwsh_bundle`

The hostile-name tests assert the name-rule message and that it is never "which was not listed" (amendment 3).

## Behaviour differences from `3054a52`

- **A Windows read writes to the host.** It creates `<home>/.ign-ctl-<pid>-<seq>-<nanos>/` with `read.tsv` and `read.zip`, and removes them before it returns: the `get` session removes the archive and the dir, and after any failure one BUNDLE_CLEAN call removes what is left. The gateway and its data directory are never written. README and the module docs say so.
- **A read needs a readable account home outside the data directory.** A home at or below the data dir (ASCII case ignored) is refused before any write.
- **A read needs free space** on the home's drive for `archive_limit` plus 256 MiB. BUNDLE refuses otherwise with "bytes free".
- **Windows no longer uses per-file `get`.** POSIX hosts do, byte for byte as before.
- **Small Windows reads cost about three more connections.**
- **New transport_failed details** (slug and exit code unchanged): the P8 table in the PLAN, plus "cannot bundle the listed files on the host", "the bundle result is malformed", "is inside the data directory", and the cleanup suffix "; the read staging directory ... could not be removed (...)".
- **Residual (T-dl1-08).** A read killed from outside, for example by an outer timeout, leaves its staging dir. The live run 1 did exactly that: it left one empty dir, because the script's own `finally` had removed both files. The next run removed it with the new opt-in `IGN_WFLWVM01_REMOVE_CTL`.

## Deviations from Plan

1. **[Rule 1] Windows PowerShell 5.1 list split** (`47d195f`, found live).
   - `.Split([char]10,[StringSplitOptions]::RemoveEmptyEntries)` has no matching overload on .NET Framework, so PS 5.1 bound it to `Split(params char[])`. The empty entry after the last LF survived, and the script failed after zipping every file.
   - The fix uses `Split(char[], StringSplitOptions)`, and a test pins it.
   - pwsh 7 cannot reproduce the bug.
2. **[Rule 1] Live harness order.** The bundle-alone timing runs before the read, like the walk alone, so a read that runs out of time still reports each step's share. A cut-off read also names the staging dirs it left.
3. **[Rule 1] Timed rows serialized whole** (`0b16d0a`, see Edited tests).
4. **[Rule 3] Install.**
   - `cargo install --path crates/ignition-cli --force` re-resolved dependencies and was blocked by Aikido Device Protection on winnow 1.0.5 ("published too recently to be vetted").
   - The rerun was `cargo install --path crates/ignition-cli --force --locked --offline`: the repo's pinned `Cargo.lock` (winnow 1.0.4) and already-downloaded crates only. No package was substituted and `Cargo.lock` is unchanged.
   - The coordinator accepted this as a pass of step F.
5. **[Rule 3] Private target dir.**
   - One workspace `cargo clippy` stalled with no CPU progress for 20 minutes while the machine was swapping, holding the shared target lock. Killing its children was denied by the permission classifier.
   - Every later gate, live test and the install used a private `CARGO_TARGET_DIR` in the scratchpad, as `.claude/CLAUDE.md` prescribes. The stalled chain later finished on its own.
6. **Amendment 4 detail.** The zip crate reports `unix_mode() == None` when external attributes are 0, which is what .NET Framework writes; it does not synthesize a mode in that case. The extractor therefore accepts `None` only for a DOS-made record (host byte 0) with external attributes 0. Any `Some(mode)` must have `S_IFREG` type bits, so a Unix-made record with type bits 0 is refused (tested). The live archive confirmed the shape.
7. **Vocabulary size.** 128 words, not the 4,096 first tried (see Content compressibility).
8. **Fake zip level.** The fake's BUNDLE uses Deflate level 1. The hostile tests keep the default level, so the bomb archive (4,890 bytes) stays inside `archive_limit`.
9. **Unreachable check.** The "running total past listed total plus slack" check in `extract` cannot fire while the per-entry limit holds. It is kept as defence in depth and has no dedicated test.
10. **STATE.md and ROADMAP.md** were not touched; the orchestrator owns them for this quick task.

## Gates (final code)

| Gate | Result |
|---|---|
| `cargo fmt --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass (after `47d195f`, live harness included) |
| `cargo xwin clippy --target x86_64-pc-windows-msvc --workspace --all-targets -- -D warnings` | pass |
| `cargo test --workspace --no-fail-fast` at `0b16d0a` | 97 binaries, 3,224 passed, 0 failed, 100 ignored |
| same after `47d195f` | 3,222 passed; the only failures were 3 timed rows at load average 200 to 490 (pwsh BUNDLE 774 s, POSIX sftp op timeout, local read 27 s) |
| `deploy_prod_scale --test-threads=1` rerun right after | 6/6 pass |
| Hygiene | no Cargo.toml, Cargo.lock, golden, fixture, error.rs or target.rs change since `3054a52`; no dotted-quad address in added lines; no co-author lines; explicit paths only; none of the four never-stage paths staged; nothing pushed; no worktree |

**Environment.** The dev Mac carried other workloads throughout this run: load average 25 to 490, and 27.8 of 29.7 GB of swap in use. Wall-clock numbers measured under that load are marked as such. CI on a quiet runner should be faster.

## What Windows CI must confirm

- `prod_scale_windows_sftp_read`: real `sftp-server.exe` plus local PowerShell, now through the bundled path. It must show identity, at most 3 `list_tree` connections, at most 8 read connections, no new `.ign-ctl-*` in the runner's home, and each operation under 120 s.
- The `conformance_ps_*` rows that read through the Windows transport.
- The FileShare `ReadWrite|Delete` source open under PowerShell 5.1 against a file the gateway holds open (live, the gateway was running, but nothing held a seeded file).
- The archive shape: .NET Framework writes made-by host 0 with external attributes 0, which the fake's `zip_of` reproduces and the extractor accepts (`unix_mode() == None` only for that shape). This is confirmed live on PS 5.1. A PowerShell 7 Windows runner writes different attributes; confirm it extracts.

## Known Stubs

None.

## Threat Flags

| Flag | File | Description |
|---|---|---|
| threat_flag: host-write-on-read | crates/ignition-core/src/deploy/transport/sftp.rs | read-only verbs on a Windows ssh host now write one `.ign-ctl-*` dir below the deploy account's home (T-dl1-07; confined, refused inside the data dir, removed) |
| threat_flag: untrusted-archive | crates/ignition-core/src/deploy/transport/bundle.rs | host-controlled zip parsed locally (T-dl1-01..03; every P8 refusal tested end to end) |
| threat_flag: admin-route | crates/ignition-core/tests/live_transport_wflwvm01.rs | unchanged from 261104-s3n; the new `IGN_WFLWVM01_REMOVE_CTL` only runs BUNDLE_CLEAN on validated `.ign-ctl-*` names in the home |

## Self-Check: PASSED

- Files exist: `crates/ignition-core/src/deploy/transport/bundle.rs` and the modified files above.
- Commits exist on `feat/windows-fast-listing`: `7f12ed6`, `95bca62`, `7fcaf40`, `42b9ba2`, `0b16d0a`, `47d195f`, `803d5a3`.
- The restore printed `PASS ... manifest hash and counts equal`, and the VM is deallocated.
