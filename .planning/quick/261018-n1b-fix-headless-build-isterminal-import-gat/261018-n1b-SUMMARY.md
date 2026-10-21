---
phase: quick-261018-n1b
plan: 01
subsystem: ignition-cli
tags: [build, headless, ci, deploy-apply]
requires: [DAPL-01]
provides: [headless-build-green]
affects: [crates/ignition-cli/src/main.rs]
key-files:
  modified:
    - crates/ignition-cli/src/main.rs
decisions:
  - "Single unconditional `use std::io::IsTerminal;` import instead of per-call-site qualification or a second gated import"
metrics:
  tasks: 1
  commits: 1
status: complete
actuals:
  tokens: 50
  tasks: 1
  commits: 1
---

# Phase quick-261018-n1b Plan 01: Fix headless build IsTerminal import Summary

Removed the `#[cfg(feature = "tui")]` gate on `use std::io::IsTerminal;` in `crates/ignition-cli/src/main.rs`, so the `--no-default-features` build compiles again.

## What changed

The `ign deploy apply` Decision C check (DAPL-01) calls `stdin().is_terminal()` and `stdout().is_terminal()` in every build, but the trait import was gated on `tui`. Deleting the one attribute line fixes E0599 in the headless build. The check body is untouched. The import is used in both feature sets, so neither gets an unused-import warning.

Commit: `d55502d` `fix(23): import IsTerminal in the headless build` (1 deletion, only main.rs, no co-author trailer, not pushed).

## Verification

Red first: `cargo build -p ignition-cli --no-default-features` failed with E0599 at main.rs:3268/3269 before the edit.

| Step | Result |
|------|--------|
| `cargo build -p ignition-cli --no-default-features` | pass (was E0599) |
| `cargo clippy -p ignition-cli --no-default-features -- -D warnings` | pass (not a CI gate) |
| `cargo fmt --all --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo build --workspace` | pass |
| `cargo test -p ignition-cli --test contract_deploy` | pass (55 passed) |
| `cargo test --workspace` | pass (2487 passed, 0 failed; the known flake contract_tags::from_export_legacy_layout_and_filter did not trip) |
| `cargo build -p ignition-cli --no-default-features` (re-run after tests) | pass |

## Deviations from Plan

None - plan executed exactly as written.

## Known Stubs

None.

## Self-Check: PASSED

- main.rs has exactly one `use std::io::IsTerminal;` and no cfg attribute above it.
- Commit d55502d exists on main.
