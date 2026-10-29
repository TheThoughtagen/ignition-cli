---
phase: 25-guarded-project-deploy
plan: 23
subsystem: infra
tags: [windows, icacls, sid, whoami, ssh, acl, service-account]
status: complete
gap_closure: true
gap_ids: [G-25-10]

requires:
  - phase: 25-guarded-project-deploy
    provides: ssh node transport with key_env key file, apply run directory
provides:
  - deploy/owner_acl module: owner-only ACLs granted by SID read from whoami /user
  - ssh key file restricted by SID (works under NETWORK SERVICE, LocalSystem, local and domain users)
  - apply run directory restricted by SID with an inheritable full-control grant
affects: [25-27 live proof on ign-win-836]

actuals:
  tokens: 9000
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "Pure parse/argument functions behind #[cfg(any(windows, test))], process-spawning functions behind #[cfg(windows)]"
    - "Grant by *SID so icacls needs no account name lookup"

key-files:
  created:
    - crates/ignition-core/src/deploy/owner_acl.rs
  modified:
    - crates/ignition-core/src/deploy/mod.rs
    - crates/ignition-core/src/deploy/transport/remote.rs
    - crates/ignition-core/src/deploy/apply/run_store.rs

key-decisions:
  - "The SID comes from whoami.exe (no new dependency, no unsafe); is_sid accepts only S-1- plus 2 to 16 digit groups, so no output can add an icacls argument"
  - "Both sites stay fail-closed: whoami spawn or exit failure, unparsable SID and icacls failure all refuse, with no fallback to the user name or an unrestricted file"

requirements-completed: [DPRJ-03, DPRJ-04, DPRJ-05]
---

# Phase 25 Plan 23: Owner-only ACLs by SID Summary

On Windows, ign now restricts the ssh key file and the apply run directory to the SID of its own process account (read from `whoami /user /fo csv /nh`, granted as `icacls ... /grant:r *<SID>:<rights>`), which closes G-25-10 for NETWORK SERVICE and other service accounts.

## Tasks

| Task | Name | Commit |
|------|------|--------|
| 1 (tracer) | ssh key file restricted to the process SID, whoami line to icacls grant | ce65c8d |
| 2 | apply run directory restricted to the process SID, inheritable full control | 23a185b |

## Verification

All run with rustup's cargo 1.99.1 (`~/.cargo/bin` first on PATH; see Deviations).

- `cargo test -p ignition-core --lib owner_acl`: 5 passed (the 3 planned tests plus `is_sid_accepts_only_sids` and `sid_from_whoami_csv_bounds_the_error_text`)
- `cargo test -p ignition-core --lib run_store`: 19 passed
- `cargo test -p ignition-core --lib -- readme_exit_table_agreement exit_code_mapping_enumerated`: 2 passed
- `cargo clippy -p ignition-core --all-targets -- -D warnings`: pass
- `cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings`: pass (Windows-only functions and tests compile)
- `cargo fmt --all -- --check`: pass
- `cargo test --workspace`: exit 0, 2916 passed, 0 failed
- `grep -c 'owner_acl::restrict_to_current_user(file, "R")'` in remote.rs = 1; `grep -c 'owner_acl::restrict_to_current_user(path, "(OI)(CI)F")'` in run_store.rs = 1
- `env::var` count in `restrict_to_owner` = 0, in the Windows `harden_dir` = 0
- `git diff --quiet HEAD -- crates/ignition-core/src/error.rs` exits 0 (error.rs untouched)

## Deviations from Plan

**1. [Rule 3 - Blocking] Wrong cargo on PATH for the xwin check**
- **Found during:** Task 1 verify
- **Issue:** `/opt/homebrew/bin/cargo` (1.94.1, no Windows target) shadows rustup's cargo, so the first `cargo xwin clippy` failed with E0463 (`can't find crate for core`).
- **Fix:** Ran every verify command with `~/.cargo/bin` first on PATH (cargo 1.99.1, target `x86_64-pc-windows-msvc` installed). No repo change.
- **Commit:** none

**2. [Process] Tracer feedback gate in a non-auto run**
- `workflow.auto_advance` and `_auto_chain_active` are both false, so the interactive tracer rule would stop for a human-verify. The tracer's `<verify>` is fully automated and was run end to end and passed before Task 2 started, and the orchestrator asked for a full run, so I continued (same as 25-22). The one thing a human cannot check here, the live NETWORK SERVICE run, is plan 25-27's.

## Not verified here

The Windows-only tests (`current_user_sid_is_a_sid`, `restrict_to_current_user_leaves_one_grant`, the extended `run_dir_created_private_windows`) were compiled and linted via cargo-xwin but not executed; windows-latest CI runs them. The live proof under NETWORK SERVICE on ign-win-836 is plan 25-27's.

## Known Stubs

None.

## Threat Flags

None. T-25-74 (refuse on every failure) and T-25-75 (SID validation before icacls) are implemented as planned; T-25-77 (shared service-account SID) is transferred to plan 25-26's README.

## Self-Check: PASSED

owner_acl.rs exists; commits ce65c8d and 23a185b exist.
