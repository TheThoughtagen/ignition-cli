---
phase: 22-transport-plan-pull-verify
plan: 05
subsystem: deploy
tags: [rust, ssh, sftp, process-runner, host-key-pin, tokio, security]

requires:
  - phase: 22-transport-plan-pull-verify
    provides: read_repo_file and open_regular_file (22-01), transport_failed (22-01), TreeReader seam (22-01)
provides:
  - "ProcessRunner / ProcessOutput / TokioProcessRunner: argv-only spawn, piped-and-closed stdin, per-call timeout with kill and reap, bounded capture, spawn failure as code 127"
  - "RemoteShell: hardened ssh and sftp argv, repo-pinned host key, safely materialised key, timeouts, sftp_batch, require_success"
  - "safe_remote_path, scrub_stderr, normalize_private_key, hardened_options, STDERR_TAIL_LINES"
  - "Target ssh.known_hosts (required), ssh.connect_timeout_s, ssh.op_timeout_s; host and user option-injection refusal"
affects: [22-06, 22-07, 22-13, 22-14]

actuals:
  tokens: 20650
  tasks: 2
  commits: 4

tech-stack:
  added: []
  patterns:
    - "Every ssh/sftp call is an argv vector through one Arc<dyn ProcessRunner>; fakes script the trait"
    - "Per-shell private temp dir (0700) owns the known_hosts copy and the key file and is removed on drop"
    - "Environment lookup is injectable (from_ssh_with_env) so key_env tests never mutate the process environment"

key-files:
  created:
    - crates/ignition-core/src/deploy/transport/process.rs
    - crates/ignition-core/src/deploy/transport/remote.rs
    - crates/ignition-core/tests/deploy_transport.rs
  modified:
    - crates/ignition-core/src/deploy/target.rs
    - crates/ignition-core/src/deploy/transport/mod.rs
    - crates/ignition-core/src/deploy/lint.rs
    - crates/ignition-core/tests/deploy_plan.rs
    - crates/ignition-core/tests/deploy_render.rs
    - README.md

key-decisions:
  - "Stdout capture is bounded (32 MiB); going over kills the child and reports code -2, so a truncated listing can never read as a complete one. Stderr keeps its last 64 KiB. ProcessOutput stays four fields"
  - "A timed-out call kills AND reaps the child before returning (kill then wait), so the process is gone, not a zombie"
  - "Option paths: whitespace is quoted, % is doubled (ssh expands %h-style tokens), a Windows runner gets forward slashes, and double quote, $ and control characters are refused"
  - "host and user also refuse @ in host (plan named it for user only): user@host splitting is on the last @, so an @ in either moves the boundary"

patterns-established:
  - "RemoteShell::local_sftp_server(server, runner, timeout) runs sftp -D against a local sftp-server binary: the sftp protocol layer is testable without an sshd"

requirements-completed: []
requirements-advanced: [DPLN-01]  # process and shell layer only; the ssh transport that uses it lands in 22-06, so DPLN-01 stays unchecked

duration: 55min
completed: 2026-10-14
status: complete
---

# Phase 22 Plan 05: Process seam and hardened RemoteShell Summary

**A mockable `ProcessRunner` (argv only, closed stdin, hard timeout that kills and reaps, bounded capture) under a `RemoteShell` that builds `ssh -G`-proven hardened ssh and sftp argv with a repo-pinned host key and a key file that never reaches argv, logs or errors.**

## Accomplishments

- **`ProcessRunner` / `TokioProcessRunner`** (`transport/process.rs`): `kill_on_drop(true)`, stdin piped, the optional bytes written, then the handle dropped (EOF). `tokio::time::timeout` wraps the I/O; on elapse the child is killed and awaited, so the call returns `timed_out: true` with no process left (`timeout_kills_child` checks `kill -0` fails). Stdout over `STDOUT_CAP_BYTES` (32 MiB) kills the child and returns code `-2`; stderr keeps its last `STDERR_CAP_BYTES` (64 KiB). A spawn failure is code 127 with `failed to spawn <program>: <kind>`.
- **`RemoteShell`** (`transport/remote.rs`): `from_ssh` (and `from_ssh_with_env` for an injected environment) reads `ssh.known_hosts` through `read_repo_file` (symlink-refusing, empty or comment-only is `deploy_tree_invalid`), copies it 0600 into a fresh 0700 `ign-ssh-*` temp dir, materialises `key_env` (CRLF to LF, one trailing LF, 0600) or checks `key_file` is a regular file, and builds the option list once. `ssh_argv`, `sftp_argv`, `sftp_batch` (rejects a line holding CR or LF, runs `sftp` with the batch on stdin under `op_timeout`), `require_success` (timeout, non-zero exit, or any stderr byte becomes `transport_failed` with a scrubbed tail), `local_sftp_server` (the `-D <server>` test seam).
- **Hygiene helpers:** `safe_remote_path` (empty, control characters, `"`, `\`, `* ? [ ] { }`), `scrub_stderr` (last 5 lines, at most 1 KiB, ANSI and control characters removed, temp dir replaced by `<tmp>`), `normalize_private_key`.
- **Target schema:** `ssh.known_hosts` (required, repo-relative, no `/` prefix, drive, backslash, `..`, empty or dot-leading component), `connect_timeout_s` (1..=120, default 10), `op_timeout_s` (1..=3600, default 120); `host` and `user` refuse a leading `-`, whitespace, control characters and `@`.

## Exact `-o` order

Every ssh and sftp argv carries these, in this order (`hardened_options`); the 20 pairs become 21 when an identity is configured (`IdentitiesOnly=yes` is item 11):

1. `BatchMode=yes`
2. `StrictHostKeyChecking=yes`
3. `UserKnownHostsFile=<temp copy>`
4. `GlobalKnownHostsFile=/dev/null` (`NUL` on a Windows runner)
5. `UpdateHostKeys=no`
6. `VerifyHostKeyDNS=no`
7. `PreferredAuthentications=publickey`
8. `PasswordAuthentication=no`
9. `KbdInteractiveAuthentication=no`
10. `NumberOfPasswordPrompts=0`
11. `IdentitiesOnly=yes` (only with `key_file` or `key_env`)
12. `ForwardAgent=no`
13. `ForwardX11=no`
14. `ClearAllForwardings=yes`
15. `PermitLocalCommand=no`
16. `ControlMaster=no`
17. `ControlPath=none`
18. `ConnectTimeout=<connect_timeout_s>`
19. `ServerAliveInterval=5`
20. `ServerAliveCountMax=3`
21. `LogLevel=ERROR`

Argv shapes: ssh `-T -p <port> [-i <key>] -o k=v ... -- user@host [command]`; sftp `-q -b - -P <port> [-i <key>] -o k=v ... -- user@host`.

## Task Commits

1. **Task 1 RED:** `249c0d5` (test; 8 failing tests against skeletons)
2. **Task 1 GREEN: ProcessRunner, safe_remote_path, scrub_stderr:** `427ad24` (feat)
3. **Task 2a: ssh target fields and option-injection refusal:** `72e65cc` (feat)
4. **Task 2b: RemoteShell, tests/deploy_transport.rs, README:** `5dcfcc2` (feat)

## Test results

- `cargo test -p ignition-core --lib deploy::` 147 passed (includes `process` 6, `remote` 5 plus target `target_ssh_fields`, `ssh_host_and_user_cannot_be_read_as_options`).
- `cargo test -p ignition-core --test deploy_transport` 14 passed: `ssh_g_reports_pinned_options`, `ssh_g_reports_identity_and_configured_timeouts`, `ssh_g_keeps_odd_paths_literal`, `sftp_argv_shape`, `key_env_materialised`, `key_env_unset_is_transport_failed`, `key_file_must_be_a_regular_file`, `key_never_echoed`, `distinct_temp_dirs`, `known_hosts_pin_is_validated`, `known_hosts_symlink_is_refused`, `sftp_batch_feeds_stdin_with_the_op_timeout`, `require_success_maps_every_failure_shape`, `local_sftp_server_seam_runs_a_batch`.
- `cargo test --workspace` all green (including `deploy_render` 63 and `deploy_plan` 19, unaffected by the new required field once their ssh fixtures gained `known_hosts`); `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` clean.
- **CI legs:** everything above ran on the local macOS (OpenSSH 10.3p1) leg. The unix-only tests (`timeout_kills_child` unix variant, `stdin_is_closed`, `stdout_over_the_cap_kills_and_fails`, `stderr_keeps_only_its_tail`, `nonzero_exit_code_is_reported`, `key_env_materialised`, `known_hosts_symlink_is_refused`, `local_sftp_server_seam_runs_a_batch`) need a unix runner with `ssh-keygen` and an OpenSSH `sftp-server`. **`windows_key_acl` and the windows `timeout_kills_child` variant were written but NOT run**: there is no Windows leg here, so the `icacls` ACL, the `NUL` global-known-hosts value and the quoted-path parse on Win32-OpenSSH (RESEARCH A11) remain unverified until a Windows runner (22-13, C7a).

## Deviations from Plan

**1. [Rule 2 - Missing critical functionality] Bounded stdout/stderr capture**
- **Found during:** Task 1 (orchestrator security expectation: bound stdout and stderr capture; the plan's `wait_with_output` is unbounded)
- **Fix:** replaced `wait_with_output` with capped readers (stdout fails the call over 32 MiB, stderr keeps a 64 KiB tail), kill plus reap on overflow. `ProcessOutput` keeps its four planned fields; overflow is code `-2`.
- **Files:** `transport/process.rs` - **Commit:** `427ad24`

**2. [Rule 1 - Plan/environment mismatch] `ssh -G` does not print `controlpath none`**
- **Found during:** Task 2 (`ssh_g_reports_pinned_options`)
- **Issue:** the plan truth lists `controlpath none`, but OpenSSH 10.3p1 omits the `controlpath` line entirely when `ControlPath=none`; `controlmaster false` is reported. The option is still passed and effective.
- **Fix:** the test asserts that no `controlpath` line other than `none` appears (a user config re-enabling a mux socket would fail it) instead of expecting the literal line.
- **Files:** `tests/deploy_transport.rs` - **Commit:** `5dcfcc2`

**3. [Rule 2 - Missing critical functionality] Extra hardening beyond the plan text**
- `@` is refused in `host` as well as `user` (boundary shift in `user@host`).
- Option paths: `%` is doubled (ssh expands `%h`-style tokens in file options), `$` is refused (environment expansion), a Windows runner gets forward slashes.
- `sftp_batch` refuses a line containing CR or LF as a second guard behind `safe_remote_path`.
- `hardened_options` returns `Result<_, String>` (the plan's prose required an error for an unsafe path; its signature line showed a bare `Vec`).

**4. [Judgement] `from_ssh` is synchronous; the Windows ACL uses a blocking `std::process::Command`**
- The plan has the `icacls` step run "through the runner", but `for_node` (sync, 22-01) will call `from_ssh`, and the runner is async. The `icacls` call is a short local command compiled only on Windows (`#[cfg(windows)]`), so it uses `std::process::Command` directly. Revisit in 22-13 if an async constructor is wanted.

**5. [Process] Task 2 had no separate RED commit.** The `RemoteShell` API did not exist when the tests were written, so the test file could not compile as a RED commit; tests and implementation landed together in `5dcfcc2`. The target half and Task 1 did follow test-first.

**Total deviations:** 2 auto-fixes (Rules 1 and 2), 1 hardening addition, 2 judgement calls. No architectural change.

## New packages

None. `Cargo.lock` is unchanged; `tempfile`, `tokio` (`process`, `time`, `io-util`, `macros`), `async-trait` and `rustix` were already direct dependencies of `ignition-core`.

## Known Stubs

None added. `transport::for_node` still returns `deploy_tree_invalid` for `ssh` nodes (22-01's stub, replaced by 22-06 which will build `RemoteShell::from_ssh`). `RemoteShell` has no production caller in this plan by design.

## Threat Flags

None beyond the plan's register. T-22-17 (option injection: target schema plus `--` in every argv), T-22-18 (`safe_remote_path`, `sftp_batch` line-break guard), T-22-19 (pinned known_hosts, `ssh -G` contract test), T-22-20 (0600 key in a 0700 dir, deleted on drop, manual `Debug`, `key_never_echoed`), T-22-21 (closed stdin, `ConnectTimeout`, `ServerAlive*`, timeout with kill), T-22-22 (`scrub_stderr`), T-22-23 (`read_repo_file`, `known_hosts_symlink_is_refused`) are all mitigated and tested.

## Notes for 22-06

- Build the shell with `RemoteShell::from_ssh(repo, target_file, label, ssh, Arc::new(TokioProcessRunner))`; call `shell.require_success(op, shell.sftp_batch(&lines).await?)`.
- `STDOUT_CAP_BYTES` (32 MiB) bounds listings; file bytes must be downloaded with `get` to a temp dir, not read from stdout.
- The shell's temp dir (`temp_dir()`) is scrubbed to `<tmp>` in error details; do not put a download target outside it into a message.

## Self-Check: PASSED

- Files: `process.rs`, `remote.rs`, `tests/deploy_transport.rs` present; `grep -c 'kill_on_drop(true)'` in `process.rs` is 1; `grep -n 'pub known_hosts: String'` in `target.rs` prints one line; `grep -c known_hosts README.md` is 3 or more.
- Commits `249c0d5`, `427ad24`, `72e65cc`, `5dcfcc2` present in `git log`.
- No `unwrap`, TODO or placeholder patterns in the new non-test code; `.planning/config.json` and `.planning/research/.cache/` untouched.
