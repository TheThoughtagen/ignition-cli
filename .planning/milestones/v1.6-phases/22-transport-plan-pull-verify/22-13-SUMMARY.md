---
phase: 22-transport-plan-pull-verify
plan: 13
subsystem: deploy
tags: [windows-openssh, sftp, powershell, decision-b, ledger-location, live-proof, success-criterion-1]

requires:
  - phase: 22-transport-plan-pull-verify
    provides: RemoteShell, SftpOp, RemoteCommand::powershell_encoded (22-05); sftp read and write halves (22-06, 22-07); C7a, C6 and Decision B (22-12)
provides:
  - "Windows ssh nodes are read by SftpTransport and mirrored by its TreeMirror (for_node and mirror_for_node no longer refuse os: windows)"
  - "winps.rs: fixed ENUMERATE and MOVE_REPLACE scripts, budgeted encoded_command over the typed powershell_encoded, IGN-RESULT parser, CLIXML decode, the Windows path gate"
  - "RemoteShell::exec_powershell, SftpOp::Pwd, classify_sftp_stderr, same-path rename refusal"
  - "Decision B(i) applied: LedgerRoot, ledger_location(root, ..), ledger_rel, staging_rel, default_staging_mode"
  - "live_transport_wflwvm01.rs: five #[ignore] proofs against the Windows capture host, run green, host left clean"
affects: [22-14, 23]

actuals:
  tokens: 62000
  tasks: 2
  commits: 6

key-files:
  created:
    - crates/ignition-core/src/deploy/transport/winps.rs
    - crates/ignition-core/tests/live_transport_wflwvm01.rs
  modified:
    - crates/ignition-core/src/deploy/transport/{mod,remote,sftp,process,local}.rs
    - crates/ignition-core/src/deploy/ledger.rs
    - crates/ignition-core/src/actions/deploy.rs
    - crates/ignition-core/tests/{deploy_transport.rs,common/plan_fixture.rs}
    - crates/ignition-core/tests/fixtures/wflwvm01/capture22w.sh
    - .planning/phases/22-transport-plan-pull-verify/22-LIVE-CAPTURES.md
    - README.md

key-decisions:
  - "A Windows listing is never trusted alone: every Windows read is compared with a PowerShell walk (the \\\\?\\ prefix) before a byte is downloaded"
  - "Windows long listings print bare names, so each ls -la is led by a pwd marker (SftpOp::Pwd); the POSIX prefixed format is unchanged"
  - "The Windows commit replaces by sftp rename (C7a-4 proves it); MOVE_REPLACE ships as the proven, live-exercised fallback and is not wired into the commit"
  - "ssh nodes keep the ledger and staging in the account home (B(i)); the home is read with pwd and its listing skips names a batch line cannot carry"
  - "Windows component and path gate (windows_component_problem, windows_absolute_path_problem) applied everywhere a path from the host or the target reaches a batch line"

patterns-established:
  - "Budget::lenient() / NameRule::Skip: directories where only fixed, safe names are looked up may hold names a batch line cannot carry"
  - "FakeSftp models a Windows host (bare names, pwd, writes, rename failures, PowerShell walk) for unit tests"

requirements-completed: [DPLN-01]

duration: 3h
completed: 2026-10-15
status: complete
---

# Phase 22 Plan 13: Windows OpenSSH transport, Decision B and the live proof Summary

**A Windows OpenSSH host is read and mirrored over hardened sftp from the C7a captures, with an independent PowerShell completeness walk, the Decision B(i) ledger location, and the whole thing proven against the real Azure Windows host.**

## Commits

| Commit | What |
|---|---|
| f12df14 | docs: the post-C6 Modify grant on `production`, verified as the account (new `c6post` driver subcommand and a `C6-post` section marked post-C6; the original C6 answers are untouched) |
| 70a10fd | feat: `winps.rs`, `exec_powershell`, `SftpOp::Pwd`, the same-path rename refusal, stderr classification, UTF-8 locale for children |
| 72b2d34 | feat: Windows `for_node` / `mirror_for_node`, `windows_sftp_root`, marked listings, the cross-check, rename retry, Decision B(i) ledger and staging |
| 738a9fd | fix: post-review security fixes (below) |
| bf6a692 | docs + test: README (Windows notes, Decision B) and the live proof file |

## How each C7a decision was implemented

- **C7a-1, `/C:/` form.** `windows_sftp_root` makes `/` + upper-cased drive + `:` + forward-slash path (vectors from the captured data dir, spaces, lowercase drive, trailing separator; UNC, `\\?\`, relative, drive-relative, ADS, trailing dot or space, quotes, over 150 characters refused). `C:/...` and backslash forms are never produced.
- **C7a-2, listings.** The Windows server prints long names bare, so `parse_batch_listing_as(Marked, ..)` splits on `Remote working directory:` markers (`SftpOp::Pwd` before every `ls -la`), keeps names with spaces and commas (everything after the eighth column and one space), requires marker count = directory count. Verbatim captured output is a unit-test fixture. The sftp client runs with `LC_ALL` set to a UTF-8 locale (`process::utf8_locale`); an escaped `caf\303\251` is refused (backslash), never read as real.
- **C7a-3, errors by text.** `classify_sftp_stderr` (not found, permission denied, failure, other) for exit 1 only; `require_success` names the class. A not-found answer for a name the listing returned reads "not found or unreadable" (a denied read reports not found).
- **C7a-4, rename.** Replace-by-rename is used. `SftpOp::Rename` refuses a same-path rename (case-insensitive, repeated slashes, trailing slash ignored) for every host, so the write half cannot issue one; unit tests at `SftpOp::render` and in a Windows commit (the fake server asserts it never sees one). A rename answered `Failure` is retried 3 times (growing pause, resumed from the failing op using the client's echoed commands), then classified; permission denied and everything else is not retried; POSIX hosts never retry.
- **C7a-5, junctions.** The read already used per-file `get`, never `get -R`. The walk reports reparse points and any one fails the read closed; verified live with a real junction.
- **C7a-6, PowerShell.** Prelude `$ErrorActionPreference='Stop'` and `$ProgressPreference='SilentlyContinue'`; success needs exit 0 and exactly one `IGN-RESULT:` line; stderr is opaque, CLIXML `<S S="Error">` text is decoded for display only; an error record on stderr with exit 0 is a failure; progress CLIXML with exit 0 is tolerated; output is ASCII (`\uXXXX`) because PowerShell writes code page 437.
- **C7a-7, budget.** `COMMAND_BUDGET = 8000` characters on the whole remote command, checked before spawn; `enumerate_chunks` splits roots across commands; a host "command line is too long" answer is named.
- **C7a-8, long paths.** `cross_check` (Windows only, between discovery and download): the walk's files, directories and sizes must equal sftp's per root, no reparse point, no path of 240 characters or more. Any difference is `transport_failed` naming the paths. Proven live with a 313-character path and a junction.

Typed API only: `encoded_command` composes the script text and calls `RemoteCommand::powershell_encoded`; there is no raw-line or raw-command API and no second encoder.

## Decision B(i)

`LedgerRoot::{DataDir, AccountHome}`; `ledger_location(root, gateway, mode)` is `ign-deploy/<gw>/<mode>/ledger.json` for `local` nodes and `~/ign-deploy/...` (the account home, as `pwd` reports it) for `ssh` nodes. `staging_rel` and `default_staging_mode` give `Separate` at `<home>/ign-deploy/<gw>/<mode>/staging/<run_id>` (ssh) or `<data_dir>/ign-deploy/...` (local). Nothing was migrated. The README documents it. Staging containment compares case-insensitively for a Windows host whatever OS the runner is.

## Live proof (SC-1) against the Azure Windows host, as `svc-ignition-deploy`

Windows Server 2022, Win32-OpenSSH 8.1, PowerShell 5.1, pinned ED25519 key (fingerprint re-checked by `ssh-keyscan` before the run). Command: `IGNITION_LIVE_WFLWVM01=1 ... cargo test -p ignition-core --test live_transport_wflwvm01 -- --ignored --nocapture --test-threads=1` -> **5 passed, 0 failed, 494.9 s**.

| Test | Result | Elapsed |
|---|---|---|
| `wflwvm01_pull_render_plan_is_empty` | pull 91 files (7 skipped, manifest generated), render, plan over ssh: added 0 changed 0 removed 0 same 48 unmanaged 1, `has_changes: false` | pull 125.4 s, render 0.02 s, plan 103.5 s, total 228.9 s |
| `wflwvm01_write_half_scratch` | Separate (at the B(i) default root) then InPlace commit into a scratch collection under the account home; replaced a file by rename, wrote a `Café DB,1` unit, deleted `Doomed` and removed its emptied directory; read back through the Windows reader (listing + walk + download) and compared; `config-mode.json` sha256 identical and the pre-existing `Existing/config.json` untouched after both rounds; no dot-named entry | separate stage+commit 69.1 s, read back 21.2 s, in-place 19.1 s, total 156.5 s |
| `wflwvm01_exec_powershell` | `{"major":5,"ok":1,"s":"café","user":"svc-ignition-deploy"}` (non-ASCII intact over code page 437); `exit 7` -> `transport_failed` "exit code 7"; `throw` -> CLIXML decoded ("boom from the script"); `MOVE_REPLACE` replaced an existing target with a hostile-looking name; missing source -> `transport_failed` | read-only exec 2.7 s, MOVE_REPLACE 2.1 s, total 21.6 s |
| `wflwvm01_timeout_is_transport_failed` | `op_timeout_s: 1` with a remote `Start-Sleep 10`: "timed out after 1s and was killed" | 1.00 s |
| `wflwvm01_long_paths_and_junctions_fail_closed` | a 313-character path: read refused ("has a path of 313 characters on the host"); a junction: refused ("reparse point"); both read again once removed | refused read 12.3 s, total 85.8 s |

Timing is input to OQ5 / 22-14: one ssh session costs about 2 to 3 s here, so reads are session-bound (`pull` 125 s and `plan` 103 s for a gateway with roughly 100 files and a deep `core`). The 120 s default `op_timeout_s` is per call, not per read, and no single call came close; the totals are what 22-14 should weigh.

**Cleanup evidence.** Every test removes what it wrote and asserts the host reports it gone (`remove_tree` returns `gone=1`, then a fresh enumeration returns "root missing"). After the run, as the account over sftp: `ls -la` of `production` shows `config-mode.json` (133 bytes) only, the account home has no `ign-*` entry (`dir /b %USERPROFILE% | findstr /i ign-` exits 1), and `ign-deploy` was removed. Nothing was written to the production collection by the tests; the only writes there are the C6-post probes, removed in the same session.

## Post-C6 provisioning grant

The user's `icacls ... production /grant ...:(OI)(CI)(M)` of 2026-10-15 was verified as the account (commit f12df14, `## C6-post`): put, rename, rename-over-existing from the home, mkdir/rmdir and rm in `production\` all succeed; `resources` still shows read-only for the account; the scan state is identical before and after. No ACL change was made by me. The original C6 section is unchanged.

## Post-review security fixes

Raised against commit 72b2d34 (path-traversal / sibling-path gate parity); fixed in 738a9fd with tests written first (five failed before the fix).

1. **Component boundary.** Every containment check was audited: `validate_staging_root_for` compares component vectors; `cross_check` uses `root_rel + "/"` and `Vec::starts_with` on chains; `verify_download` strips `config/resources/<coll>/`; `RelPath::strip_prefix` requires the separator. One real gap: `SftpTransport::shown` used a string prefix (`/data-evil` read as below `/data`), message-only but wrong; fixed. Pinned by `shown_paths_respect_the_data_dir_component_boundary`, and by `windows_home_and_staging_roots_pass_the_windows_path_gate` (`resources2`, `data2`, `data-staging` are outside; any case or separator of the real thing is inside).
2. **Windows gate parity.** Real gap: `component_problem` is the POSIX gate and nothing refused `C:` in a component, ADS (`name:stream`), trailing dot or space, device names or `<>|` on a Windows node, and the paths returned by the PowerShell walk were unchecked. Added `windows_component_problem` and `windows_absolute_path_problem` (winps.rs) and applied them to: Windows listing names (refused, or skipped in the lenient home listing), scope mode/module/type/provider, `list_types`, ledger gateway/mode, staging gateway/mode/run id, `account_home`, staging roots, `with_mirror` mode, staged and delete-set paths, and every path in a walk result (`parse_enumeration`). Pinned by `windows_component_gate_refuses_what_posix_refuses_and_more`, `windows_absolute_path_gate`, `parse_enumeration_applies_the_windows_gate_to_every_path`, `windows_listing_names_pass_the_windows_gate`, `windows_components_pass_the_windows_gate_before_any_session`, `windows_staged_and_deleted_paths_pass_the_windows_gate_and_write_nothing`. Listings also refuse two names that differ only by case. The POSIX path is untouched (a test pins `a:b` and `/home/svc:1` as still valid there).
3. **Cross-check normalisation.** Not a gap: both sides are compared as exact `/`-separated strings (the walk's `\` becomes `/` in `parse_enumeration`), per root, keyed by the full relative path; a sibling cannot satisfy another. Pinned by `windows_cross_check_does_not_let_a_sibling_satisfy_another` (sibling type roots, and host entries differing by case, suffix or trailing digit are reported).

## Deviations from Plan

**1. [Override] Plan text vs code.** The plan's raw `exec_powershell`/`encoded_command(String)` and raw sftp lines were replaced by `WinScript` + the typed `powershell_encoded` and `SftpOp` (new typed `SftpOp::Pwd`). `encoded_command` returns a `RemoteCommand`.

**2. [Rule 1/2] Windows listings are not prefixed.** The C7a-2 blocks show bare names in `-l`/`-la`, which the existing parser (prefix attribution) cannot attribute across several directories; added the `pwd`-marked format.

**3. [Rule 2] The home listing holds names a batch line cannot carry** (`NTUSER.DAT{guid}.TM.blf`, found live before writing code). The ledger and staging surveys use `Budget::lenient()` (skip, do not refuse); scoped trees stay strict.

**4. [Judgement] MOVE_REPLACE is not used by the commit.** C7a-4 proves sftp `rename` replaces; the script ships, is unit-tested under pwsh and run live, and is the documented fallback.

**5. [Rule 2] Rename retry, case-insensitive staging containment for a Windows host, Windows gate** (above), none in the plan text.

**6. [Rule 1] ssh ledger tests moved to the account home.** The ledger of an ssh node is no longer below `data_dir`; unit and integration tests were updated (integration tests run the real `sftp` with the fixture data dir as its working directory, i.e. as "the home").

**7. [Process] Tests and implementation landed together** for winps/remote/sftp (no separate RED commit) except the post-review fixes, which were written first and failed.

**8. [Not done] Gateway trial reset.** The gateway reports "trial, 0s remaining". The transport proof is filesystem-level and needed no licensed gateway, and reading the gateway-admin credential file for `ign rig trial reset` was refused by the permission system, so it was not attempted again. The trial is still expired.

## What stayed uncompiled / unproven

- The Windows `cfg(windows)` branches (key ACL via `icacls`, `NUL` device, backslash normalisation of local paths) cannot be built on this Mac (no Windows std target). All Windows-HOST logic added here is ordinary code compiled and tested on unix.
- `list_types` on a Windows node is not cross-checked (it lists only `<module>/<type>` directory names; `windows_sftp_root` caps `data_dir` at 150 characters so those always resolve). `read_ledger` and the account home are short fixed paths.
- The write half is only exercised by the `#[ignore]` live test and the unit/integration tests; no verb, action or MCP tool reaches it (`write_half_unreachable` is green).
- The Windows-client `ssh.exe` stdin item stays with the windows-latest CI leg.
- Design-spec section 9 amendment (whk-environment-orchestration) is still to do, outside this repository.

## Requirements

**DPLN-01 marked complete.** The Windows half is delivered and proven live: read and write over system OpenSSH to a real Windows OpenSSH host with batch mode, a pinned host key, per-call timeouts (forced timeout proven live), argv-only invocation and client-side hashes, plus the local filesystem. Plan 22-14 only revisits the timeout defaults from OQ5 and does not change the requirement.

## Known Stubs

None. `MOVE_REPLACE` is intentionally not wired into the commit (deviation 4).

## Threat Flags

| Flag | File | Description |
|------|------|-------------|
| threat_flag: ssh-remote-exec | crates/ignition-core/src/deploy/transport/winps.rs | runs fixed PowerShell scripts on the gateway host as the service account; the plan's T-22-55 mitigations hold (const bodies, base64 JSON parameters, budget before spawn) and are unit-tested |

## Test counts

- `cargo test --workspace`: all green, 0 failed. `ignition-core` lib: 778 passed (was about 739 before this plan); `deploy_transport`: 60; `deploy_plan`: 57; `deploy_pull`: 18; `deploy_verify`: 45; `live_transport_wflwvm01`: 2 run offline, 5 `#[ignore]` live (run green).
- `cargo clippy --workspace --all-targets -- -D warnings`: clean. `cargo fmt --all -- --check`: clean.
- New tests (approximate): winps 13, remote 5 plus 3 exec tests, process 1, ledger 1, sftp about 24, integration 4 (the renamed for_node and mirror_for_node tests, default staging), live file 7 (2 offline, 5 live).

## Self-Check: PASSED

- Commits f12df14, 70a10fd, 72b2d34, 738a9fd, bf6a692 exist on main.
- `winps.rs`, `live_transport_wflwvm01.rs` and this file exist; no key text in the captures file (`grep -c 'BEGIN OPENSSH PRIVATE KEY'` prints 0).
- `.planning/config.json` and `.planning/research/.cache/` were never staged.
