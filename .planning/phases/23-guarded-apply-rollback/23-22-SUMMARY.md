---
phase: 23-guarded-apply-rollback
plan: 22
subsystem: deploy-transport
tags: [sftp, openssh, listing-parser, linux-ssh, live-gate, fa-23-05]
gap_closure: true
gap_ids: [G1]
status: complete

requires:
  - phase: 23-guarded-apply-rollback
    provides: the POSIX ApplyHost (commit.sh), the Windows marker-led listing (22-13), the quick-task 261018-nr2 live gates
provides:
  - one listing format for every host, with an @pwd marker before every @ls -la
  - a per-line name shape (bare longname or client-formatted prefix) with a date-column check
  - two verbatim real-server captures pinned as regression tests
  - lnxvm_listing_capture, a read-only live capture gate
  - all five FA-23-05 gates passing in one sequential run on ign-lnx-836
affects: [23-23, 23-24, phase 23 verification, FA-23-05]

actuals:
  tokens: 21000
  tasks: 3
  commits: 9

tech-stack:
  added: []
  patterns:
    - "marker-attributed listings for every host family; the name shape is decided per line"

key-files:
  created:
    - .planning/phases/23-guarded-apply-rollback/23-22-SUMMARY.md
  modified:
    - crates/ignition-core/src/deploy/transport/sftp.rs
    - crates/ignition-core/tests/live_transport_lnxvm.rs
    - .planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md
    - README.md
    - crates/ignition-cli/tests/deploy_family_coverage.rs

key-decisions:
  - "ListFormat names the host family (Posix, Windows), not the attribution; both are marker-led"
  - "the Windows component gate and the case-folded duplicate check key off ListFormat::Windows only"
  - "the deploy account on a Linux ssh node needs read and execute (list) on data and data/config (user decision, granted non-recursively, no default ACL, write still denied)"

requirements-completed: []

duration: ~2h50m
completed: 2026-10-19
---

# Phase 23 Plan 22: POSIX sftp listing fix and the FA-23-05 rerun Summary

**Marker-led sftp listings for every host with a per-line name shape, pinned by a real OpenSSH 8.9 capture; all five FA-23-05 gates pass in one sequential run on ign-lnx-836, after the user granted the deploy account list rights on data and data/config.**

## Status: COMPLETE

G1 is fixed and FA-23-05 is closed on a live Linux host except for a move across file systems (home and production share one file system on this host). `requirements-completed` is left empty (human_verify_mode end-of-phase).

## What changed

- `crates/ignition-core/src/deploy/transport/sftp.rs`
  - `list_level_as` pushes `SftpOp::Pwd` before every `LsLongAll`, for every host.
  - `ListFormat::{Prefixed, Marked}` became `ListFormat::{Posix, Windows}` (the host family). The prefix-only attribution path is gone; the marker count must equal the directory count; a line before the first marker, a missing marker and an extra marker are refused.
  - Per line, inside the current marked directory: the name is the single component after the exact `<dir>/` prefix (the shape the macOS sftp client prints) or, otherwise, the text after the eighth column (the server's bare longname). A bare name holds no `/`. The last two columns (day, then time or four-digit year) must read as a date, so a line whose owner or group holds a space is refused, never read under a wrong name.
  - The Windows component gate and the case-folded duplicate check run for `Windows` only; a POSIX listing keeps `a:b`, `CON`, `trail.` and `A`/`a` as ordinary names.
  - `ListFormat` doc comment states the cause (the client prints the server's bare longname unless it formats the line itself). `ls -lan` was not used.
- `crates/ignition-core/tests/live_transport_lnxvm.rs`: `lnxvm_listing_capture` (read only).
- `23-LIVE-CAPTURES.md`: `## G1: POSIX sftp listing shapes`, `### Rerun after the listing fix (2026-10-19)`, `### Grant of list rights on data and data/config, and the rerun of all five gates (2026-10-19)`, two dated paragraphs under `### FA-23-05 status`, and a follow-up note for spec section 9 PR #63 (account scope should add read and execute (list) on `data` and `data/config` for Linux nodes; the other repo was not touched).
- `README.md` and `rollback_section_agrees_with_apply_section`: the FA-23-05 sentence (below), a new "Linux hosts" paragraph under `deploy apply` listing the account's rights, and the test asserting the old "unproven" claim is absent.

## The two captures

Both live in `23-LIVE-CAPTURES.md` under `## G1: POSIX sftp listing shapes` and as verbatim constants in the sftp.rs unit tests:

| Constant | Server | Shape | Test |
|---|---|---|---|
| `OPENSSH_8_9` | ign-lnx-836, OpenSSH 8.9p1 `sftp-server`, 2026-10-19T00:27:25Z | bare names, `.` and `..` bare, `Jan  6  2022` year dates | `posix_listing_parses_captured_openssh_8_9_output` |
| `MACOS_CLIENT_FORMATTED` | local `/usr/libexec/sftp-server` via `sftp -D`, 2026-10-19T00:27:41Z | `<dir>/<name>` | `posix_listing_parses_captured_client_formatted_output` |

New tests: `posix_listing_takes_either_shape_per_line`, `posix_listing_refuses_a_prefix_of_another_directory`, `posix_bare_listing_with_a_spaced_owner_is_refused_not_misread`, `posix_listing_keeps_windows_only_rules_off`, `read_over_a_bare_name_posix_server`, `prepare_over_a_bare_name_posix_server`, and a FakeSftp `bare_posix` knob. RED was seen first: all eight new tests failed on the old parser with `a listing line does not carry a listed directory prefix`.

## Gate results (ign-lnx-836, 2026-10-19)

First pass (before the grant): `lnxvm_listing_capture`, `lnxvm_apply_script_run`, `lnxvm_sh_commit_time_budget`, `lnxvm_conformance_table_on_host` (48 of 48) PASS; `lnxvm_cli_apply_rollback` FAIL at `ign deploy plan`: `transport failed (read) on nodes.master: exit code 1 (permission denied): remote readdir("/usr/local/bin/ignition/data/"): Permission denied`. Cause: `getfacl` showed `user:svc-ignition-deploy:--x` on `data` and `data/config`; the read half lists every ancestor from the data dir for the symlink check.

The decision (option a) and grant: the user ran `sudo -n setfacl -m u:svc-ignition-deploy:r-x /usr/local/bin/ignition/data /usr/local/bin/ignition/data/config` at their own prompt (non-recursive, no default ACL). Verified read-only as the account: both directories list, write is denied on both, `config/resources` and `production/config-mode.json` still read. The provisioning script `sh/30-svc-account.sh` (outside the repo) now sets `r-x` with a comment on why.

| Run | Result |
|---|---|
| `lnxvm_cli_apply_rollback` alone @ 01:30:42Z | PASS, 571.6 s: plan exit 0, apply `applied` with `.gwbk`, modes 644/775, verify ok, rollback `restored`, plan after equals first, listing equals pre-run, closing scan ran, deploy dir gone |
| five-gate run #1 @ 01:51:58Z | 4 of 5 PASS. `lnxvm_cli_apply_rollback` FAIL in `ign deploy rollback` (exit 4): `network_error` on `POST /data/api/v1/scan/projects`, "error sending request", after 161.7 s. plan, apply and verify had passed; production put back, closing scan ran, host clean; gateway reachable minutes later. Treated as a transient network error on the public http endpoint; not reproduced. |
| CLI-gate rerun (not counted) | cut off by the executor's own 595 s tool timeout after `rollback` returned `restored`; left two empty directories under `production/ignition` and a run dir under `ign-deploy`; removed by hand as the account (`rmdir`/`rm` of exactly those paths) and the host checked clean. |
| **five-gate run #2 @ 02:22:49Z (one sequential invocation)** | **5 of 5 PASS** (`5 passed; 0 failed`): `lnxvm_apply_script_run` 459.6 s, `lnxvm_cli_apply_rollback` 608.5 s, `lnxvm_conformance_table_on_host` 48 passed; 0 failed (29.3 s), `lnxvm_listing_capture`, `lnxvm_sh_commit_time_budget` 124.3 s |

Host state after run #2: home holds only `.bash_logout .bashrc .cache .profile .ssh`; production holds only `config-mode.json` (22:59). VM power state found `VM running`; left running. Trial: reset once earlier (expired at about 00:51:20Z, `POST` at 00:51:30Z returned 200; `POST` while active returns 403); 4865 s left at 01:30Z and 1736 s at 02:22Z, no further reset.

## Regression results

- After the final change: `cargo test --workspace` 2501 passed, 0 failed (the known `contract_tags` flake did not fire); `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --all --check` clean; headless build (`cargo build -p ignition-cli --no-default-features`) clean. Earlier checks: sftp unit tests 66 passed, `deploy_host_conformance` 48, `deploy_transport` 63, `deploy_family_coverage` 7.
- `cargo install --path crates/ignition-cli --force` was run before the CLI gate (source unchanged since).

## Tests that pinned the old batch lines (updated, reason)

- `parse_ls_listing_cases`, `parse_batch_listing_attributes_lines_in_order`, `entry_cap_stops_the_parse_at_the_offending_line`, `windows_listing_names_pass_the_windows_gate` (POSIX tail): inputs ported to marker-led batches; each bad case sits under a marker so it is refused for its own reason. One meaning changed on purpose: an empty stdout has no marker and is refused; an empty directory is a marker with no entry lines (or only the dots).
- `batches_never_suppress_errors`: now allows the `@pwd` marker line.
- `symlinks_and_special_files_are_refused_not_skipped`: now asserts the last session holds only `@pwd` and `@ls -la` lines (refusal from the listing, before any download).
- Windows listing tests only had `Marked` renamed to `Windows`.

## README sentence as written

> **Linux ssh was run against a live host (FA-23-05, OQ8, 2026-10-19):** an Ubuntu 22.04 host with OpenSSH 8.9p1 and dash as `/bin/sh`, reached as a key-only account with no sudo. There `commit.sh` and its restore ran over ssh (prepare, commit, the stub's tamper refusal, restore), the commit left 0644 files and 0755 directories under a session umask of 0002, a commit stopped at its time budget, the shared host conformance table passed 48 of 48, and `ign deploy plan`, `apply`, `verify` and `rollback --run` against the real `production` collection restored it exactly. The account needs read and execute (list) on `data` and `data/config` as well as write on the mode collection: the read half lists every ancestor from the data directory to refuse a symlink on the path. Not captured: the move of staged files when the account home and the gateway data directory are on different file systems (a copy and delete, not an atomic rename). Run a first Linux ssh apply or rollback against a disposable gateway, and keep the `.gwbk`.

## Deviations from Plan

1. **[Process] Tracer gate run in the autonomous form.** Auto mode was off; the tracer's verify is a fully automated live test, so it was re-run (PASS) and execution continued instead of returning a `checkpoint:human-verify`.
2. **[Test shape] `prepare_over_a_bare_name_posix_server` drives `stage`** (the survey and put-tree path `prepare` also uses), not `ApplyHost::prepare`; the real `prepare` is covered live by `lnxvm_apply_script_run`.
3. **[Decision] Rule 4 stop and resume.** The data-directory read denial was a host fact contradicting the plan's assumption; the plan stopped, the user chose to grant list rights on the two directories and applied the grant themselves. No transport change was made.
4. **[Operations] Trial reset** works only on an expired trial on this host. **Executor mistake:** a 595 s tool timeout killed one CLI-gate run mid-test and left test leftovers on the host; they were removed by hand and the run is not counted.

No test-code or host-code fix was forced by the gates; no live assertion was weakened.

## Commits

- c67e8bc fix(23-22): read bare-name sftp listings on POSIX hosts
- 1fbff5c test(23-22): add the read-only listing capture gate for the Linux host
- 9516ba7 docs(23-22): record the two POSIX sftp listing shapes and the first Linux gate pass
- 9a6beff docs(23-22): record the FA-23-05 rerun on ign-lnx-836 and the data-directory listing blocker
- 76000db docs(23-22): state what the live Linux host proved and what stays unproven (superseded by 8d44fc5)
- 5173ede docs(23-22): summary for the POSIX listing fix and FA-23-05 rerun
- f32b02e docs(23-22): complete POSIX sftp listing fix plan (partial, CLI gate blocked)
- c89389c docs(23-22): record the data/config list grant and the five-gate rerun on ign-lnx-836
- 8d44fc5 docs(23-22): README states the Linux host passed all five gates and the account's list right on data

## Known Stubs

None.

## Threat Flags

None new. T-23-83 mitigations (marker count, current-directory prefix, no `/` in a bare name, date check, `safe_remote_path`, caps) are in place and tested; T-23-84 tested both ways; T-23-85 held (production put back after every CLI-gate run); T-23-86: the captures hold names, modes, sizes, dates and ACL listings only (checked: no key, token, password or IP address in the added lines). The deploy account's added read right on `data` and `data/config` widens it to listing the gateway data directory (names only; it still cannot write there or read below `config` except `resources`); recorded for the spec section 9 account-scope text.

## Self-Check: PASSED

Created and modified files exist (sftp.rs, live_transport_lnxvm.rs, 23-LIVE-CAPTURES.md, README.md, deploy_family_coverage.rs); the commits listed above exist; `## G1: POSIX sftp listing shapes` and `### Rerun after the listing fix` each appear once; five-gate run #2 recorded 5 of 5 PASS.
