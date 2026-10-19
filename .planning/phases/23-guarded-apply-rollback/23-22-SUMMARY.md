---
phase: 23-guarded-apply-rollback
plan: 22
subsystem: deploy-transport
tags: [sftp, openssh, listing-parser, linux-ssh, live-gate, fa-23-05]
gap_closure: true
gap_ids: [G1]
status: partial

requires:
  - phase: 23-guarded-apply-rollback
    provides: the POSIX ApplyHost (commit.sh), the Windows marker-led listing (22-13), the quick-task 261018-nr2 live gates
provides:
  - one listing format for every host, with an @pwd marker before every @ls -la
  - a per-line name shape (bare longname or client-formatted prefix) with a date-column check
  - two verbatim real-server captures pinned as regression tests
  - lnxvm_listing_capture, a read-only live capture gate
  - three of the four FA-23-05 gates passing on ign-lnx-836 (plus the capture gate)
affects: [23-23, 23-24, phase 23 verification, FA-23-05]

actuals:
  tokens: 16300
  tasks: 3
  commits: 5

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
  - "the README states only what passed live; plan/apply/verify/rollback against the real collection stays unproven"

requirements-completed: []

duration: 50min
completed: 2026-10-19
---

# Phase 23 Plan 22: POSIX sftp listing fix and the FA-23-05 rerun Summary

**Marker-led sftp listings for every host with a per-line name shape, pinned by a real OpenSSH 8.9 capture; three of four FA-23-05 gates now pass on ign-lnx-836, and the fourth is blocked by a data-directory read permission.**

## Status: PARTIAL (one blocker)

G1 itself is fixed: `prepare`, the read half's listings and the shipped `ign deploy plan` get past the first listing on an Ubuntu 22.04 OpenSSH 8.9p1 host, and `lnxvm_apply_script_run`, `lnxvm_sh_commit_time_budget` and `lnxvm_conformance_table_on_host` (48 of 48) pass. `lnxvm_cli_apply_rollback` does not: `ign deploy plan` stops with `transport_failed` (exit 4), `remote readdir("/usr/local/bin/ignition/data/"): Permission denied`. See Blockers. The must-have truth that names `lnxvm_cli_apply_rollback` is therefore NOT met. `requirements-completed` is left empty on purpose.

## What changed

- `crates/ignition-core/src/deploy/transport/sftp.rs`
  - `list_level_as` pushes `SftpOp::Pwd` before every `LsLongAll`, for every host.
  - `ListFormat::{Prefixed, Marked}` became `ListFormat::{Posix, Windows}` (the host family). The prefix-only attribution path is gone; the marker count must equal the directory count; a line before the first marker, a missing marker and an extra marker are refused.
  - Per line, inside the current marked directory: the name is the single component after the exact `<dir>/` prefix (the shape the macOS sftp client prints) or, otherwise, the text after the eighth column (the server's bare longname). A bare name holds no `/`. The last two columns (day, then time or four-digit year) must read as a date, so a line whose owner or group holds a space is refused, never read under a wrong name.
  - The Windows component gate and the case-folded duplicate check run for `Windows` only; a POSIX listing keeps `a:b`, `CON`, `trail.` and `A`/`a` as ordinary names.
  - `ListFormat` doc comment states the cause: the client prints the server's bare longname unless it formats the line itself (users-groups-by-id extension), and then the listed path is in front. `ls -lan` was not used.
- `crates/ignition-core/tests/live_transport_lnxvm.rs`: `lnxvm_listing_capture` (read only, asserts only that both markers appear).
- `23-LIVE-CAPTURES.md`: `## G1: POSIX sftp listing shapes` (both captures verbatim with commands and UTC times, plus the first gate pass) and `### Rerun after the listing fix (2026-10-19)` with the results table, the blocker, and a dated paragraph appended to `### FA-23-05 status`.
- `README.md` and `rollback_section_agrees_with_apply_section`: the FA-23-05 sentence (below) and its needles.

## The two captures

Both live in `23-LIVE-CAPTURES.md` under `## G1: POSIX sftp listing shapes` and as verbatim constants in the sftp.rs unit tests:

| Constant | Server | Shape | Test |
|---|---|---|---|
| `OPENSSH_8_9` | ign-lnx-836, OpenSSH 8.9p1 `sftp-server`, 2026-10-19T00:27:25Z | bare names, `.` and `..` bare, `Jan  6  2022` year dates | `posix_listing_parses_captured_openssh_8_9_output` |
| `MACOS_CLIENT_FORMATTED` | local `/usr/libexec/sftp-server` via `sftp -D`, 2026-10-19T00:27:41Z | `<dir>/<name>` | `posix_listing_parses_captured_client_formatted_output` |

New tests: `posix_listing_takes_either_shape_per_line`, `posix_listing_refuses_a_prefix_of_another_directory`, `posix_bare_listing_with_a_spaced_owner_is_refused_not_misread`, `posix_listing_keeps_windows_only_rules_off`, `read_over_a_bare_name_posix_server`, `prepare_over_a_bare_name_posix_server`, and a FakeSftp `bare_posix` knob (bare names, bare `.`/`..`). RED was seen first: all eight new tests failed on the old parser with `a listing line does not carry a listed directory prefix`.

## Gate results (ign-lnx-836, 2026-10-19)

| Gate | Result |
|---|---|
| `lnxvm_listing_capture` | PASS, 4.1 s |
| `lnxvm_apply_script_run` | PASS, 268.7 s (prepare, commit via `commit.sh`, both tamper refusals, journal refusal, restore, cleanup; modes 644 files, 755 new directory, umask 0002, uid 1001) |
| `lnxvm_sh_commit_time_budget` | PASS, 118.5 s (a 1 s budget stops the commit with `deadline: true`) |
| `lnxvm_conformance_table_on_host` | PASS, `48 passed; 0 failed` as uid 1001, 26.1 s |
| `lnxvm_cli_apply_rollback` (`IGNITION_LIVE_LNXVM_PROD=1`) | FAIL at `ign deploy plan` (exit 4, `transport_failed`): `remote readdir("/usr/local/bin/ignition/data/"): Permission denied`. Production put back (listing equal), closing scan ran, deploy dir gone. |

Host state after every gate: home holds only `.bash_logout .bashrc .cache .profile .ssh`; production holds only `config-mode.json` (22:59). VM power state found: `VM running`; left running. Trial: 426 s left at 00:44:18Z; `POST /data/api/v1/trial` while active returned 403 `auth_rejected`; the trial expired at about 00:51:20Z and the reset at 00:51:30Z returned 200 (7199 s). Resetting works only on an expired trial on this host.

## Regression results (macOS)

- `cargo test -p ignition-core --lib sftp::tests::` 66 passed; `deploy_host_conformance` 48 passed (local and `sh` rows); `deploy_transport` 63 passed (real-protocol tests against the local `sftp-server`).
- `cargo test --workspace`: 2501 passed, 0 failed (the known `contract_tags::from_export_legacy_layout_and_filter` flake did not fire). `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` clean.
- Install: `cargo install --path crates/ignition-cli --force` before the CLI gate.

## Tests that pinned the old batch lines (updated, reason)

- `parse_ls_listing_cases`, `parse_batch_listing_attributes_lines_in_order`, `entry_cap_stops_the_parse_at_the_offending_line`, `windows_listing_names_pass_the_windows_gate` (POSIX tail): inputs ported to marker-led batches; each bad case now sits under a marker so it is refused for its own reason. One meaning changed on purpose: an empty stdout has no marker and is refused; an empty directory is a marker with no entry lines (or only the dots).
- `batches_never_suppress_errors`: now allows the `@pwd` marker line.
- `symlinks_and_special_files_are_refused_not_skipped`: asserted the last session's first line was `@ls -la`; now asserts the last session holds only `@pwd` and `@ls -la` lines (refusal from the listing, before any download).
- Windows listing tests only had `ListFormat::Marked` renamed to `ListFormat::Windows`; their inputs and expectations are unchanged.

## README sentence as written

> **Linux ssh was run against a live host in part (FA-23-05, OQ8, 2026-10-19):** an Ubuntu 22.04 host with OpenSSH 8.9p1 and dash as `/bin/sh`, reached as a key-only account with no sudo. There `commit.sh` and its restore ran over ssh against a scratch collection (prepare, commit, the stub's tamper refusal, restore), the commit left 0644 files and 0755 directories under a session umask of 0002, a commit stopped at its time budget, and the shared host conformance table passed 48 of 48. **Still unproven against a live host:** `ign deploy plan`, `apply`, `verify` and `rollback --run` against the real gateway collection. The read half lists the gateway data directory, and the one account tried could only traverse it, so that run stopped at `ign deploy plan` before any write; the account needs read access to the data directory, or the read half needs a change. Also not captured: the move of staged files when the account home and the gateway data directory are on different file systems (a copy and delete, not an atomic rename). Run a Linux ssh apply or rollback against a disposable gateway first, and keep the `.gwbk`.

The test pins "FA-23-05", "Linux ssh was run against a live host", "48 of 48", "0644 files and 0755 directories", the plan/apply/verify/rollback unproven clause, "unproven against a live host", "different file systems", and the absence of the old conformance-only sentence. The plan's "old phrase absent" assertion was applied to the old conformance-only sentence, not to "unproven against a live host", because not all five gates passed and that phrase is still true.

## Blockers

**`lnxvm_cli_apply_rollback` cannot pass on this host as provisioned.** Exact error: `transport failed (read) on nodes.master: exit code 1 (permission denied): remote readdir("/usr/local/bin/ignition/data/"): Permission denied`. Cause: `getfacl` as the account shows `user:svc-ignition-deploy:--x` (traverse only) on `data` (mode `drwxr-x---+ root root`) and on `data/config`; `r-x` starts at `data/config/resources`. `SftpTransport::read` lists each parent from the data directory down to find the next component (so a symlink on the path is refused), so its first listing is denied. This is neither a listing-format nor a test-code defect: the failure moved from the missing-prefix error to a server permission error. Options for a decision: (a) grant the account read on `data` and `data/config` (host change; widens the account to the data directory's listing), or (b) start the read half at `config/resources` and give up the ancestor symlink check (a design change). Neither was made here (plan routing: a host fact contradicting phase 23 stops). Until decided, `ign deploy plan/apply/verify/rollback --run` against the real production collection over Linux ssh is unproven, and a Linux production deployment with a least-privilege account hits the same error. A move across file systems also remains open (home and production share one file system on this host).

## Deviations from Plan

**1. [Process] Tracer gate run in the autonomous form.** Auto mode was not active (`workflow.auto_advance` and `_auto_chain_active` false), which would have required a `checkpoint:human-verify` after the tracer. The caller asked for the whole plan in one run and the tracer's verify is a fully automated live test, so the tracer's end-to-end gate was re-run (`lnxvm_apply_script_run` PASS) and execution continued. Flagged here for the orchestrator.

**2. [Test shape] `prepare_over_a_bare_name_posix_server` drives `stage` (the survey and put-tree path `prepare` also uses), not `ApplyHost::prepare`.** There was no offline POSIX prepare scenario to extend; a full `prepare` needs a BatchSpec and a verify exec reply. The real `prepare` is covered live by `lnxvm_apply_script_run`.

**3. [Operations] Trial reset sequence.** The plan said to reset a trial with under 900 s left; the POST returned 403 while the trial was active, so the reset waited for expiry (about 7 minutes) and then succeeded. No code change.

No Rule 1 to 3 code fixes were forced by the live gates: no test-code or host-code defect surfaced in `lnxvm_apply_script_run`, `lnxvm_sh_commit_time_budget` or the conformance table. No assertion of a live gate was weakened.

## Commits

- c67e8bc fix(23-22): read bare-name sftp listings on POSIX hosts
- 1fbff5c test(23-22): add the read-only listing capture gate for the Linux host
- 9516ba7 docs(23-22): record the two POSIX sftp listing shapes and the first Linux gate pass
- 9a6beff docs(23-22): record the FA-23-05 rerun on ign-lnx-836 and the data-directory listing blocker
- 76000db docs(23-22): state what the live Linux host proved and what stays unproven

## Known Stubs

None.

## Threat Flags

None new. T-23-83 mitigations (marker count, current-directory prefix, no `/` in a bare name, date check, `safe_remote_path`, caps) are in place and tested; T-23-84 (Windows rules keyed to the family) tested both ways; T-23-85 (production write) held: the CLI gate stopped before any write and put production back; T-23-86: the captures hold names, modes, sizes and dates only (checked: no key, token, password or IP address in the added lines).

## Self-Check: PASSED

Created and modified files exist (sftp.rs, live_transport_lnxvm.rs, 23-LIVE-CAPTURES.md, README.md, deploy_family_coverage.rs); commits c67e8bc, 1fbff5c, 9516ba7, 9a6beff and 76000db exist; `## G1: POSIX sftp listing shapes` and `### Rerun after the listing fix` each appear once. This check covers the files and commits claimed here; it does not claim the blocked gate passed.
