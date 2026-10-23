---
phase: 25-guarded-project-deploy
plan: 08
subsystem: deploy
tags: [deploy-projects, project-host, ssh, sftp, windows, powershell, git-managed, fail-closed, known-hosts]

requires:
  - phase: 25-guarded-project-deploy
    provides: "25-07 GitEntry, ProjectHost, project_host_for_node (local arm), release_record_rel; 25-03 contract decision K9 and the host grant text"
provides:
  - "impl ProjectHost for SftpTransport on Linux (one fixed sh/stat script over the exec channel) and on Windows (winps::GIT_ENTRY, Get-Item -Force per level)"
  - "read_release_record on ssh nodes through read_home_file (account home, Windows component rules)"
  - "the Ssh arm of project_host_for_node, built exactly as for_node builds the sftp transport"
  - "tests/live_project_host.rs: live_project_host_lnxvm and live_project_host_winvm (ignored, quiet skip)"
  - "25-LIVE-CAPTURES.md ## C-H after grant (25-08): Linux and Windows rows, each with a denied row"
  - "README: Project deploys account-scope paragraph (list-only grant, setfacl and icacls lines) and the --targets known_hosts sentence"
affects: [25-09, 25-11, 25-13, 25-17, 25-19]

actuals:
  tokens: 20400   # chars/4 over `git diff d3d7bb0~1..HEAD -- crates README.md` (81714 chars)
  tasks: 3
  commits: 4

tech-stack:
  added: []
  patterns:
    - "K9 probe is a stat (Linux) or Get-Item -Force (Windows) per level, never an sftp listing, test -e, Test-Path or Exists: only a clean no-such-file answer is absent, anything else fails the command and is transport_failed"
    - "a Windows probe separates absent from denied by the error record category (ObjectNotFound is none, everything else is rethrown under the prelude's Stop)"
    - "fixed script text; host paths travel as separate quoted arguments (POSIX) or base64 JSON parameters (Windows)"

key-files:
  created:
    - crates/ignition-core/src/deploy/transport/sftp/project_ops.rs
    - crates/ignition-core/tests/live_project_host.rs
  modified:
    - crates/ignition-core/src/deploy/transport/sftp.rs
    - crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs
    - crates/ignition-core/src/deploy/transport/winps.rs
    - crates/ignition-core/src/deploy/transport/projects.rs
    - .planning/phases/25-guarded-project-deploy/25-LIVE-CAPTURES.md
    - README.md

key-decisions:
  - "K9 on Linux is stat-over-exec, not the marker-led sftp listing the plan text names: C-H showed the listing answers not found under denial."
  - "Windows probe is Get-Item -LiteralPath -Force at three levels; a reparse point (junction or symlink, dangling or not) is present and not followed; a Hidden entry is present."
  - "A denied probe is transport_failed on both OS families; the live run records it as a row, not just the found cases."
  - "No ACL change was applied for this plan: as provisioned, the capture hosts need no grant (25-03), so the denied row is produced by narrowing the test project directory only."

patterns-established:
  - "Live host tests set up shapes through the recorded admin route (ignadmin with sudo on Linux, az vm run-command as SYSTEM on Windows) and remove them in a Drop guard that reports how many CAP25- entries remain"

requirements-completed: []  # DPRJ-04 stays open: the apply-time refusal (25-11) and the 25-19 live gates remain, as left by 25-07

coverage:
  - id: E1
    description: "DPRJ-04 Linux ssh node: .git directory, file, dangling symlink and other are present; absent project directory or no .git is absent; denied is transport_failed"
    requirement: DPRJ-04
    verification:
      - kind: unit
        ref: "deploy::transport::sftp::project_ops (sftp_git_entry_posix, sftp_git_entry_posix_failures_are_transport_failed_never_absent, sftp_git_entry_refuses_unsafe_names_before_any_call, script::probe_script_classifies_every_shape, script::probe_script_denied_is_a_failure_never_none, script::probe_script_missing_projects_directory_is_none_at_level_one, git_probe_script_is_one_fixed_line)"
        status: pass
      - kind: live
        ref: "live_project_host_lnxvm on ign-lnx-836 (## C-H after grant (25-08), Linux)"
        status: pass
    human_judgment: false
  - id: E2
    description: "DPRJ-04 Windows ssh node: a Hidden .git directory or file is present, a junction or dangling symlink is a symlink, a missing project directory is absent, denied is transport_failed"
    requirement: DPRJ-04
    verification:
      - kind: unit
        ref: "deploy::transport::sftp::project_ops (win_git_entry_parse, win_git_entry_failures_are_transport_failed_never_absent, win_git_entry_script_is_fixed_text, sftp_git_entry_windows_name_rules_run_before_the_probe)"
        status: pass
      - kind: live
        ref: "live_project_host_winvm on ign-win-836 (## C-H after grant (25-08), Windows): absent, absent, directory (Hidden), directory, file (Hidden), symlink (junction), symlink (dangling), transport_failed (denied), absent"
        status: pass
    human_judgment: false
  - id: E3
    description: "Release record read on ssh nodes: absent at every level is None; symlink component or over-cap file is deploy_run_invalid"
    requirement: DPRJ-04
    verification:
      - kind: live
        ref: "missing record read returned None on both capture hosts"
        status: pass
      - kind: unit
        ref: "deploy::transport::sftp::tests::sftp_release_record_posix; the ledger reader tests that share read_home_file"
        status: pass
    human_judgment: false
  - id: E4
    description: "--targets roots the known_hosts of an ssh node; an escaping path is refused"
    requirement: DPRJ-04
    verification:
      - kind: unit
        ref: "deploy::transport::projects::tests::project_host_known_hosts_from_targets_root"
        status: pass
    human_judgment: false
  - id: E5
    description: "README states the list-only grant on data/projects for Linux and Windows"
    requirement: DPRJ-04
    verification:
      - kind: integration
        ref: "grep -c 'data/projects' README.md prints 3; grep -c setfacl README.md prints 2; cargo test -p ignition-cli --test deploy_family_coverage"
        status: pass
    human_judgment: false

duration: "first executor plus a continuation of about 1h55m, of which about 90 minutes were spent waiting for a stuck Azure run-command slot (see Deviations)"
completed: 2026-10-23
status: complete
---

# Phase 25 Plan 08: ssh project host Summary

**`deploy projects --check` now refuses a git-managed project on Linux and Windows gateway hosts reached over ssh: a fixed `stat` script on Linux and a fixed `Get-Item -Force` script on Windows classify `.git` (directory, file, symlink or junction, Hidden or not), a denied probe is `transport_failed` and never "absent", and both are proven on the capture VMs.**

## Accomplishments

- **Task 1, tracer (`d3d7bb0`).** `impl ProjectHost for SftpTransport` in `transport/sftp/project_ops.rs`. The Linux probe is one fixed `sh -c` script (`GIT_PROBE_SH`) over the hardened exec channel that `stat`s `data/projects`, the project directory and `.git` and reports a word per level; only `No such file or directory` is `none`, any other `stat` failure exits 3 with its message and becomes `transport_failed`. `read_release_record` goes through `read_home_file` (which now takes the two path component names). The Ssh arm of `project_host_for_node` builds the sftp transport as `for_node` does. The user accepted the tracer at the human-verify gate.
- **Task 2 (`e48108b`).** `winps::GIT_ENTRY` and `WinScript::git_entry`: three `Get-Item -LiteralPath ... -Force` calls (`data\projects`, the project directory, `.git`); only an `ObjectNotFound` error record is `none`, everything else is rethrown. The three native paths travel in the base64 JSON parameters. The Windows branch of `git_entry` maps the result (`kind` plus `hidden` and `reparse` flags) and refuses any word or field the script does not define. Unit tests `win_git_entry_parse`, `win_git_entry_failures_are_transport_failed_never_absent` and `win_git_entry_script_is_fixed_text`; ignored live test `live_project_host_winvm`.
- **Task 3 (`5228976`).** `project_host_known_hosts_from_targets_root` pins that an ssh node's `known_hosts` is read from the targets root passed as `base`, that another base is refused naming the path, and that `../x`, `hosts/../../x` and an absolute path are refused by the target parser. README: `**Project deploys (`deploy projects`).**` account-scope paragraph (list right only, the `setfacl` default-ACL and `icacls` lines from `## Host grant and prod git.yaml (25-03)`, no write right under `data/projects`, where the release record lives) and a `--targets` sentence for `known_hosts`.
- **Docs commit** carries this SUMMARY, STATE and ROADMAP.

## Live results (`## C-H after grant (25-08)`)

| Host | Shape | Classified | Probe wall time |
| --- | --- | --- | --- |
| ign-lnx-836 | no project directory | `absent` | 1091 ms |
| ign-lnx-836 | project directory, no `.git` | `absent` | 933 ms |
| ign-lnx-836 | `.git` directory | `directory` | 973 ms |
| ign-lnx-836 | `.git` file | `file` | 929 ms |
| ign-lnx-836 | `.git` dangling symlink | `symlink` | 906 ms |
| ign-lnx-836 | project directory mode 700, root-owned | `transport_failed` (`Permission denied`) | 905 ms |
| ign-lnx-836 | project directory removed | `absent` | 969 ms |
| ign-win-836 | no project directory | `absent` | 1570 ms |
| ign-win-836 | project directory, no `.git` | `absent` | 1566 ms |
| ign-win-836 | Hidden `.git` directory (`Attributes=18`) | `directory` | 1733 ms |
| ign-win-836 | visible `.git` directory | `directory` | 2074 ms |
| ign-win-836 | Hidden `.git` file | `file` | 2361 ms |
| ign-win-836 | `.git` junction | `symlink` | 1436 ms |
| ign-win-836 | `.git` dangling file symlink (`Attributes=1056`) | `symlink` | 1614 ms |
| ign-win-836 | no right for the deploy account on the project directory | `transport_failed` (`PermissionDenied`, `UnauthorizedAccessException`) | 1529 ms |
| ign-win-836 | project directory removed | `absent` | 1545 ms |

The release-record read of a missing record returned `None` on both hosts. Windows run: 2026-10-23T03:05:35Z, 345 s. Linux run: 2026-10-22T23:37:31Z, 17.75 s. Entries named `CAP25-` left in `data/projects` after the runs: 0 on both hosts (Windows read again as `svc-ignition-deploy` over ssh). Neither VM was deallocated or restarted. A4 (Windows Hidden `.git`) is closed by the live Windows rows.

## Test results

- `cargo test -p ignition-core`: 2036 passed, 0 failed, 53 ignored (the ignored tests are live gates, including `live_project_host_lnxvm` and `live_project_host_winvm`).
- `cargo test -p ignition-cli -p ignition-tui`: 739 passed, 0 failed, 22 ignored.
- `cargo test -p ignition-core --lib deploy::transport`: 196 passed, including `sftp_git_entry_posix`, `sftp_release_record_posix`, `win_git_entry_parse`, `win_git_entry_script_is_fixed_text` and `project_host_known_hosts_from_targets_root`.
- `cargo test -p ignition-core --test live_project_host --no-run` builds; `cargo test -p ignition-cli --test deploy_family_coverage` 8 passed.
- `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --all --check` clean.
- Acceptance greps: `grep -c '^## C-H after grant (25-08)'` prints 1, `grep -c 'pub const GIT_ENTRY' winps.rs` prints 1, `grep -c 'NodeAccess::Ssh' projects.rs` prints 1 or more, `grep -c 'data/projects' README.md` prints 3, `grep -c setfacl README.md` prints 2.
- The `GIT_ENTRY` text was also run under local `pwsh` 7 on unix against a directory, a file, a dangling symlink, a symlinked directory, a `chmod 000` directory, a missing project and a missing `projects` directory before the live run; the unreadable directory threw `UnauthorizedAccessException` instead of answering `none`.
- Built in a private target dir under the session scratchpad; no background work left running.

## Deviations from Plan

### Contract departures applied (approved in `## Contract decisions (25-03)`, K9)

- **Linux probe is stat-over-exec, not a marker-led sftp listing.** The plan text names a listing of `data/projects/<name>`; C-H showed the sftp listing answers `not found` under denial. One fixed `sh -c` script `stat`s three levels instead.
- **Windows probe is `Get-Item -LiteralPath -Force`** (K9), never `Test-Path` or `[IO.Directory]::Exists`.

### Plan behaviours changed

- **Unit tests split by layer.** The plan names `sftp_git_entry_posix` and `sftp_release_record_posix` against a fake shell. The `.git` tests live in `sftp/project_ops.rs` (a scripted `ProcessRunner`, plus the real probe text run under the local `sh` with a `stat` shim that answers like GNU `stat`); `sftp_release_record_posix` is in the `sftp.rs` tests with `FakeSftp`, through the helper that `read_ledger` also uses.
- **Live test setup differs.** The Linux probe directory is created as root through `ignadmin` with sudo, and the Windows one as SYSTEM through `az vm run-command` (the recorded admin routes), not through gateway REST. Each live test adds a denied row, not just the four plan shapes, and the Windows test adds a Hidden file, a junction and a dangling symlink.
- **Record-read name labels.** `read_home_file` takes `names: [(&str, &str); 2]` so a bad gateway or project name is reported as `gateway name` or `project name`. Confirmed by the user at the tracer gate.
- **Windows refused at probe time until Task 2.** Between Task 1 and Task 2 a Windows node answered `transport_failed` ("the Windows .git probe is not available in this build") rather than being probed without the Hidden-attribute form; Task 2 replaced it. Confirmed by the user at the tracer gate.
- **No grant applied.** The plan's `## C-H after grant` heading implies an after-grant state; as provisioned the capture hosts need no grant (25-03), so the section records the shipped probes on the hosts as they are, with a narrowed project directory standing in for the state the grant fixes.
- **Windows result shape.** The plan sketches `{ok, kind, hidden, reparse}`; the script also returns `projects` and `project` level words so a missing `data\projects` or a symlinked project directory is told apart from a missing `.git` (as on Linux). `kind` is `none | dir | file | skip`.
- **Tests and implementation were written in one pass** in each task, not red first; fail-closed cases were checked by assertion and, for the script text, by the local `pwsh` run.
- **No new human-output line** names a host- or repo-derived value, so no `render.rs` escaping was added; failures reach the user through the existing error renderer.

### Auto-fixed (Rule 1, bug in my own test script)

- **Hung `az vm run-command` from `cmd /c del`.** The live Windows test's setup preamble ran `cmd /c del /f /a "<.git>"`; given a directory, `del` prompts `Are you sure (Y/N)?` and waits on a stdin the run-command channel never closes. The fourth setup call of the first Windows run hung and I killed the test at about 01:44Z. The stuck script held the VM's legacy run-command slot (`Conflict: Run command extension execution is in progress`) from about 01:33Z until the extension's 90 minute limit expired at about 03:05Z. I did not force it (no VM restart, no managed run command). A retry loop ran the cleanup script (`REMAINING: 0`) as the first call that got through; the `del` line was removed from the setup script (a directory behind a junction is removed with `cmd /c rmdir`, everything else with `Remove-Item`), and the second run completed. Only `CAP25-GITPROBE` was touched. Commit `e48108b`.

### Process

- **Tracer gate:** honoured. The first executor stopped at `checkpoint:human-verify` after `d3d7bb0`; the user accepted.
- **Estimate:** the plan estimated 120k tokens on a low-confidence scale; the realized diff is about 20k tokens on the chars/4 scale (most of the cost was live host round trips and the 90 minute wait).

## Open points

- DPRJ-04 stays open: the apply-time refusal (25-11) and the 25-19 live gates remain. `requirements mark-complete` was not run, as in 25-07.
- A project directory the deploy account cannot traverse is `transport_failed` with the OS error text; the README grant lines are the remedy. The live Windows detail begins with an echo of the script line (PowerShell's error display), then `PermissionDenied` and `UnauthorizedAccessException`; it names the cause but is noisy. Trimming it belongs to the shared `scrub_stderr` and was left alone.
- Re-running the Windows live test needs the `az vm run-command` slot free; a hung earlier command blocks it for up to 90 minutes.

## Known Stubs

None.

## Threat Flags

None beyond the register. T-25-24 mitigated (`GIT_ENTRY` uses `Get-Item -Force`; unit tests for Hidden and reparse results; the live Windows rows). T-25-25 mitigated (fixed script text, paths as base64 JSON parameters or separate single-quoted arguments, `windows_component_problem` and `component_problem` before use; `win_git_entry_script_is_fixed_text`, `sftp_git_entry_refuses_unsafe_names_before_any_call`). T-25-26 mitigated (`project_host_known_hosts_from_targets_root`). T-25-27 mitigated (README documents list-only rights; no write right under `data/projects`).

## Self-Check: PASSED

- Files exist: `crates/ignition-core/src/deploy/transport/sftp/project_ops.rs`, `crates/ignition-core/tests/live_project_host.rs`, `crates/ignition-core/src/deploy/transport/winps.rs` (contains `pub const GIT_ENTRY`), `crates/ignition-core/src/deploy/transport/projects.rs`, `README.md`, `.planning/phases/25-guarded-project-deploy/25-LIVE-CAPTURES.md` (one `## C-H after grant (25-08)`).
- Commits found in `git log`: d3d7bb0, 5228976, e48108b.
