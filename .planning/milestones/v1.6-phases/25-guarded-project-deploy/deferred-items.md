# Phase 25 deferred items

Out-of-scope discoveries logged while executing the phase. None of these were fixed by the plan that found them.

## 25-13

- **`deploy::transport::process::tests::timeout_kills_child` fails when run alone in this sandbox and passes in the full run.** Seen by the first 25-13 executor (Task 1) when the test was run on its own; the continuation executor did not run it alone and saw it pass in the full run. The file (`crates/ignition-core/src/deploy/transport/process.rs`) is untouched by 25-13 and by every Phase 25 plan; the full `cargo test --workspace` run (2847 passed, 0 failed) includes it and passes. Treated as a sandbox timing or process-group issue, not a regression. Not investigated further; reopen if it fails in CI.
  status: resolved
  resolution: moved to backlog Phase 999.1 (ROADMAP.md ## Backlog) by quick 261029-fyc on 2026-10-29; the test is not fixed and stays timing-sensitive.
