---
phase: 22-transport-plan-pull-verify
plan: 16
subsystem: deploy
tags: [ci, clippy, gap-closure, G-22-1, windows]
status: failed
requirements: [DPLN-01]
gap_closure: true
gap_ids: [G-22-1]

provides:
  - "ledger.rs and remote.rs no longer carry the two needless borrows (map_err(bad) / map_err(failure))"
  - "local run of every CI check step green under rustup stable 1.99.0"
affects: [22-17, phase-22-reverify]

actuals:
  tokens: 1000
  tasks: 2
  commits: 1

key-files:
  modified:
    - crates/ignition-core/src/deploy/ledger.rs
    - crates/ignition-core/src/deploy/transport/remote.rs
---

# Phase 22 Plan 16: CI clippy gap closure (G-22-1) Summary

**Status: FAILED at Task 3.** The two borrows named in G-22-1 are fixed and CI-equivalent checks pass locally on macOS, but the pushed head is red on `check (windows-latest)`: stable clippy `-D warnings` reports three further needless borrows in a `#[cfg(not(unix))]` body that no macOS or Linux build compiles. The plan's pass condition (all four jobs success) is NOT met.

## What was done

- Task 1 (commit `b06047e`): removed the needless borrows at `ledger.rs` (`node::classify(bytes).map_err(bad)?`) and `remote.rs` (`map_err(failure)?`). Ran every CI `check` step locally under rustup stable (rustc 1.99.0 b940084d7 2026-09-28, cargo 1.99.0, `CARGO_TARGET_DIR=$HOME/Library/Caches/cargo-target-ign-cli-stable`): style-token grep no offenders; `cargo fmt --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` exit 0; build exit 0; test exit 0 (2148 passed, 0 failed, 51 ignored); `--no-default-features` build exit 0. Homebrew 1.94.1 clippy also clean.
- Task 2: user approved; orchestrator ran exactly `git push origin main` (395f8ee..b06047e), no tags. origin/main == HEAD == `b06047ebdecc726b282c4804abb97e631c21cfb1`.
- Task 3: located and watched the CI run on that head. Result below.

## CI result (Task 3)

- Run id: **38816133586**
- URL: https://github.com/TheThoughtagen/ignition-cli/actions/runs/38816133586
- Head SHA: `b06047ebdecc726b282c4804abb97e631c21cfb1`
- Run conclusion: **failure**

| Job | Conclusion |
|-----|-----------|
| keyring-smoke | success |
| check (windows-latest) | **failure** |
| check (macos-latest) | cancelled (fail-fast after windows failed; counts as not verified) |
| check (ubuntu-latest) | cancelled (fail-fast after windows failed; counts as not verified) |

### Failing step and first errors

Job `check (windows-latest)`, step `Run cargo clippy --workspace --all-targets -- -D warnings`, `could not compile ignition-core (lib) due to 3 previous errors`, exit code 1:

1. `crates\ignition-core\src\deploy\transport\local.rs:1265:33` `clippy::needless_borrows_for_generic_args`: "the borrowed expression implements the required traits" at `std::fs::create_dir(&collection)`; fix is `collection`.
2. `local.rs:1278:49` `clippy::needless_borrow`: `self.ensure_dir_chain(OP, &collection, parents)?`; fix is `collection`.
3. `local.rs:1300:50` `clippy::needless_borrow`: `self.read_staged(OP, &source, path, sha)?`; fix is `source`.

## Root cause (diagnosis, not acted on)

These lines are in the `#[cfg(not(unix))] fn commit_apply` (local.rs:1245), the non-Unix commit path introduced for Windows by 22-13/22-07. It is only compiled on the Windows leg, so the macOS and Linux-equivalent local clippy run in Task 1 could not see it. `collection` and `source` are already references there, so `&collection` / `&source` are redundant. The plan's local verification was structurally unable to catch this; the plan's own key_link ("fail-fast cancels windows-latest") predicted the consequence.

## Open issue (blocks G-22-1 closure)

- OPEN: G-22-1 is only partially closed. Remove the three borrows in `crates/ignition-core/src/deploy/transport/local.rs` (lines 1265, 1278, 1300), verify the non-unix path compiles (e.g. `cargo clippy --target x86_64-pc-windows-msvc` via cargo-xwin, which the ci.yml comment says was used before), push, and re-confirm all four jobs. Also note ubuntu and macos legs have not yet run to completion on a stable-clippy head and could surface their own findings.
- Per the resume instructions, no further push, job re-run, or code edit was made by this executor.

## Deviations from Plan

None in execution. The plan's expectation (windows-latest green) was not met; see open issue.

## Known Stubs

None.

## Self-Check: FAILED

- FOUND: commit b06047e (origin/main == HEAD)
- FOUND: ledger.rs and remote.rs borrow fixes
- FAILED: CI run 38816133586 not green. `check (windows-latest)` failure (3 clippy errors in local.rs `commit_apply` non-unix); `check (macos-latest)` and `check (ubuntu-latest)` cancelled.
