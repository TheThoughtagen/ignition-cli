---
phase: quick/261018-nr2
plan: 01
subsystem: deploy-transport
tags: [live-gate, linux-ssh, sftp, commit.sh, FA-23-05]
status: incomplete
requirements: [QUICK-nr2, DAPL-04, DAPL-05, DAPL-06, DAPL-08, DAPL-09]
key-files:
  created:
    - crates/ignition-core/tests/live_transport_lnxvm.rs
  modified:
    - .planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md
    - .planning/phases/23-guarded-apply-rollback/23-UAT.md
decisions:
  - "Stopped on the first engine defect (phase 23 gap), as the brief requires; no src change."
  - "Build path for the conformance binary: local OrbStack docker (linux/amd64 ubuntu:22.04, rustc 1.99.1, git archive of 9062706, --locked); disk was sufficient, the host-side fallback was not used."
actuals:
  tokens: 60000
  tasks: 3
  commits: 4
---

# Quick 261018-nr2: live SSH gate for the Linux ApplyHost (FA-23-05) - INCOMPLETE (phase 23 gap)

One new live gate file (four live gates, six offline tests), run against `ign-lnx-836`. The first real Linux sftp listing fails, so no gate reached `commit.sh`. UAT test 2 is recorded as an issue with a Gaps entry.

## The gap (exact text)

`SftpTransport` lists POSIX hosts with `ListFormat::Prefixed` and requires the server to print `<dir>/<name>` in `ls -la` lines. Ubuntu 22.04's OpenSSH 8.9p1 `sftp-server` prints bare names (the Win32-OpenSSH shape that `ListFormat::Marked` already handles), so the first line (`.`) is refused. macOS's sftp-server (used by every local test) does prefix, which is why nothing caught this before.

- apply half: `prepare: TransportFailed { operation: "prepare", node: "nodes.master", detail: "cannot use the sftp listing: a listing line does not carry a listed directory prefix: \"drwxr-xr-x    4 root     root         4096 Oct 18 22:52 .\"" }`
- read half (`ign deploy plan`, exit 4): `transport failed (read) on nodes.master: cannot use the sftp listing: a listing line does not carry a listed directory prefix: "drwxr-x---    4 svc-ignition-deploy svc-ignition-deploy     4096 Oct 18 23:43 ."`

Suggested direction (not done): use the pointer-marker (`Marked`) listing for POSIX hosts too, or detect the format. Then rebuild the conformance binary and rerun the four gates.

## Gate results (all against ign-lnx-836)

| Gate | Result |
|---|---|
| `lnxvm_apply_script_run` | FAIL at first `prepare` (listing error). Captured before it: ssh session umask 0002, uid 1001 |
| `lnxvm_sh_commit_time_budget` | FAIL at first `prepare` (same error) |
| `lnxvm_conformance_table_on_host` | FAIL: `--list` 48, 28 passed, 20 failed, as uid 1001, binary exit 101. All 20 failures are the same listing error; the passing ones never list through sftp (17 `conformance_local_*`, `conformance_sh_fingerprint_is_read_only`, `conformance_sh_probe_reports_writable_...`, and the non-row tests) |
| `lnxvm_cli_apply_rollback` (opt-in) | FAIL at `ign deploy plan` (exit 4, `transport_failed`) before any write; production put back (listing equal), closing scan ok (0.56 s), deploy dir gone |

Host left clean after every run: home holds only `.bash_logout .bashrc .cache .profile .ssh`; production holds only `config-mode.json`. Trial was valid (4026 s left at 23:44:18Z), no reset.

## Regression results (main checkout, all green)

- `cargo test -p ignition-core --test deploy_host_conformance --test live_transport_lnxvm`: 48 passed; 6 passed, 4 ignored
- `cargo clippy --workspace --all-targets -- -D warnings`: exit 0
- `cargo fmt --all -- --check`: clean
- `cargo build -p ignition-cli --no-default-features`: exit 0
- `git diff --stat 9062706 -- crates/ignition-core/src crates/ignition-cli/src`: empty
- With no env, `-- --ignored` is a quiet green skip (no network)

## Commits (not pushed, no co-author trailer)

- `1372a55` test(23): live Linux ssh gate for the commit.sh apply host (FA-23-05)
- `c763478` test(23): Linux ssh deadline and production apply/rollback gates (FA-23-05)
- `eac67ab` test(23): run the conformance table on the live Linux host (FA-23-05)
- `bb5b1ca` docs(23): record the live Linux ssh gate (FA-23-05, UAT 2)

## Deviations from Plan

- The plan said to continue each task's live run to PASS; on the gap, per the brief, I stopped fixing and recorded. Gates 2 to 4 were still run once (not retried) to scope the defect: all fail on the same listing error, and the conformance run shows exactly which rows depend on sftp listing.
- `cargo install --path crates/ignition-cli --force` was run (plan step); `ign` on PATH is now the build of the current tree (still reports 1.5.0).
- Gates A, B and the conformance gate's pass paths (modes 644/755, deadline, apply/verify/rollback, 48/48) are written and compile/clippy clean but have never run past the listing step, so their post-listing assertions are unproven.

## Open items

- Phase 23 gap above (blocker for FA-23-05).
- Cross-filesystem `mv` still not exercised (home and production share a filesystem).
- README's FA-23-05 sentence is stale (says the Linux host is unproven); not edited, follow-up.
- Windows CI debug of `deploy_host_conformance.rs` untouched.

## Known Stubs

None.

## Self-Check: PASSED

Files exist (`live_transport_lnxvm.rs`, 23-LIVE-CAPTURES.md section, 23-UAT.md edits); commits 1372a55, c763478, eac67ab, bb5b1ca present in `git log`; SUMMARY.md, STATE.md and PLAN.md are not committed.
