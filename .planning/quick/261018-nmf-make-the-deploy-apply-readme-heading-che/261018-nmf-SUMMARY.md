---
phase: quick-261018-nmf
plan: 01
subsystem: tests
tags: [windows, crlf, readme-contract, deploy-apply]
status: complete
requirements: [QUICK-nmf, DAPL-07]
key-files:
  modified:
    - crates/ignition-cli/tests/deploy_family_coverage.rs
metrics:
  tasks: 1
  commits: 1
actuals:
  tokens: 300
  tasks: 1
  commits: 1
---

# Quick 261018-nmf: deploy apply README heading check under CRLF

Whole-line `README.lines().any(|line| line == "### `deploy apply`")` replaces the LF-wrapped substring search, so `rolled_back_verdict_exception_is_dated_and_documented` passes on both LF and CRLF (windows-latest) checkouts.

## Result

- Commit: 17b1730 `fix(23): match the deploy apply README heading under CRLF` (one file, 3 insertions, 2 deletions; not pushed, no co-author trailer).
- Reproduced before the fix on a CRLF copy of README.md: only `rolled_back_verdict_exception_is_dated_and_documented` failed (6 passed, 1 failed), panicking at deploy_family_coverage.rs:208 with the "README has a `### `deploy apply`` section" message. No other test failed under CRLF.
- After the fix: 7/7 pass with README in LF, and 7/7 pass with README converted to CRLF.
- README.md restored via `git checkout -- README.md` after each simulation; `git status --short README.md` is empty.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo build -p ignition-cli --no-default-features` all pass.

## Deviations from Plan

None - plan executed exactly as written.

## Self-Check: PASSED

Commit 17b1730 exists; the edited file contains the new assertion; README.md is clean.
