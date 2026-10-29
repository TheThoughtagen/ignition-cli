---
phase: 22-transport-plan-pull-verify
plan: 16
subsystem: deploy
tags: [ci, clippy, gap-closure, G-22-1, windows]
status: complete
requirements: [DPLN-01]
gap_closure: true
gap_ids: [G-22-1]

provides:
  - "CI green on all four jobs (ubuntu, macos, windows, keyring-smoke) for the first time on a Phase 22 head"
  - "Phase 22 cfg(windows) code compiled, linted and tested on windows-latest"
  - "ENUMERATE builds relative paths from the walk, independent of how .NET spells the root"
affects: [phase-22-reverify]

actuals:
  tasks: 3
  commits: 7
  ci_rounds: 6

key-files:
  modified:
    - crates/ignition-core/src/deploy/ledger.rs
    - crates/ignition-core/src/deploy/transport/remote.rs
    - crates/ignition-core/src/deploy/transport/local.rs
    - crates/ignition-core/src/deploy/transport/winps.rs
    - crates/ignition-core/src/deploy/tree.rs
    - crates/ignition-core/tests/deploy_transport.rs
    - crates/ignition-cli/tests/deploy_family_coverage.rs
---

# Phase 22 Plan 16: CI gap closure (G-22-1) Summary

**Status: complete.** CI run [38838954722](https://github.com/TheThoughtagen/ignition-cli/actions/runs/38838954722) on head `ab8608a3d64229eb12cd25bcd5092cd76c23b94b` concluded success:

| Job | Conclusion |
|-----|-----------|
| check (ubuntu-latest) | success |
| check (macos-latest) | success |
| check (windows-latest) | success |
| keyring-smoke | success |

The plan expected one fix (two needless borrows). Removing it let each CI leg run further than ever before, and each round surfaced the next defect that fail-fast had been hiding. Six CI rounds were needed. Every push was approved by the user, `main` only, no tags.

## Rounds

| # | Commit | Change | CI run | Result |
|---|--------|--------|--------|--------|
| 1 | `b06047e` | Drop `map_err(&bad)` (ledger.rs) and `map_err(&failure)` (remote.rs), the plan's fix | 38816133586 | windows clippy: 3 more borrows in `#[cfg(not(unix))] commit_apply` |
| 2 | `d267b55` | Drop the 3 borrows in local.rs; gate `HOSTILE_ARGS` on unix; `cfg_attr(not(unix), allow(dead_code))` on the `Scratch.root` TempDir guard. Found with `cargo xwin clippy --target x86_64-pc-windows-msvc` | 38818262514 | ubuntu test `open_regular_file_rejects_a_swapped_inode` |
| 3 | `c888680` | Test: rename the checked file aside instead of deleting it, so a freed inode number cannot be reused by the replacement | 38821179408 | ubuntu green; windows test `verify_verdict_exception_is_dated_and_documented` |
| 4 | `04c3fc3` | Test: read the README heading with `lines()` (git for Windows checks out CRLF) | 38829453966 | windows lib: winps ENUMERATE, SftpOp local `\` |
| 5 | `770b38f`, `e254cbe` | ENUMERATE product fix; SftpOp tests cover both runner platforms (debug session) | 38836275411 | windows lib green (768); `deploy_transport` ssh -G tests |
| 6 | `ab8608a` | ssh -G tests expect `NUL` and `/` option paths on Windows (debug session) | 38838954722 | all green |

## Defects found

- **Product, F1 (`770b38f`):** `winps` ENUMERATE cut relative paths with `FullName.Substring(callerRoot.Length+1)`. When .NET spells the root differently from the caller (8.3 `RUNNER~1` vs `runneradmin`), every path is shifted. Production passes a `\\?\` prefix, which .NET does not rewrite, so live runs never showed it. Relative paths are now built from entry names during the walk.
- **Lint, Windows-only (`b06047e`, `d267b55`):** five clippy findings that only a Windows build compiles.
- **Test premises (`c888680`, `04c3fc3`, `e254cbe`, `ab8608a`):** inode reuse on ubuntu ext4, CRLF README on Windows, Windows-runner `\`→`/` normalisation of local SFTP paths (documented design, T-22-18 on remote paths unchanged), Windows ssh option spellings (`NUL`, `/` paths). Product behaviour unchanged in each.

The ubuntu inode failure did not reproduce locally on overlayfs, tmpfs or a fresh ext4 loop mount. The fix removes the dependency on inode allocation and CI ubuntu went green.

Debug session: `.planning/debug/resolved/windows-ci-winps-sftp-path.md`.

## Local verification per round

rustup stable rustc 1.99.0 (b940084d7 2026-09-28): style-token grep, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, build, `cargo test --workspace` (2151 passed, 0 failed at the final head), `--no-default-features` build, and `cargo xwin clippy --target x86_64-pc-windows-msvc --workspace --all-targets -- -D warnings` from round 2 on. Windows tests cannot run locally, so CI was the only Windows test runner.

## Deviations from Plan

- The plan named two borrows in two files. Closing G-22-1 took seven code commits across seven files and one product fix (ENUMERATE). Each was surfaced by CI and approved before push.
- Lesson: a `cfg(windows)` code path needs `cargo xwin clippy` locally before push, and fail-fast hides every defect behind the first.

## Known Stubs

None.

## Self-Check: PASSED

- FOUND: commits b06047e, d267b55, c888680, 04c3fc3, 770b38f, e254cbe, ab8608a on origin/main
- FOUND: CI run 38838954722 success on all four jobs
