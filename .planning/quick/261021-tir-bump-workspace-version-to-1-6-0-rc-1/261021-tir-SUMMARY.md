---
phase: quick-261021-tir
plan: 01
subsystem: release
tags: [release, versioning, cargo, prerelease]
requires: []
provides:
  - workspace version 1.6.0-rc.1
affects: [release pipeline]
tech-stack:
  added: []
  patterns: [single-sourced workspace version]
key-files:
  created: []
  modified: [Cargo.toml, Cargo.lock]
decisions:
  - "Intra-workspace path-dependency version requirements set to the exact pre-release 1.6.0-rc.1 (caret on a stable version never matches a pre-release); also matches final 1.6.0"
metrics:
  duration: ~10m (dominated by a 6m20s cold cargo build)
  completed: 2026-10-21
status: complete
actuals:
  tokens: 1200
  tasks: 2
  commits: 1
---

# Phase quick-261021-tir Plan 01: Bump workspace version to 1.6.0-rc.1 Summary

Workspace bumped to 1.6.0-rc.1 in [workspace.package], both intra-workspace dependency requirements, and the three ignition-* lockfile entries; the built binary reports `ign 1.6.0-rc.1`.

## Tasks

| Task | Name | Commit | Files |
| ---- | ---- | ------ | ----- |
| 1 | Bump workspace to 1.6.0-rc.1 (tracer) | ede85e2 | Cargo.toml, Cargo.lock |
| 2 | Verify contract tests and no stale pins | (verification only, no commit) | none |

## Verification

- `cargo metadata` exit 0.
- Cargo.lock diff: exactly 3 insertions / 3 deletions (ignition-cli, ignition-core, ignition-tui); byteorder, http, lazy_static untouched.
- `cargo run -q -p ignition-cli -- --version` prints `ign 1.6.0-rc.1`.
- `cli_chassis` 13 passed; `contract_version` 6 passed, no test edits.
- `git grep '1\.5\.0'` outside .planning, Cargo.lock, website/package-lock.json: no matches.
- No tag at HEAD; nothing pushed or published. `.planning/config.json` (modified) and `.planning/research/.cache/` (untracked) left as found.

## Deviations from Plan

None - plan executed exactly as written. The Cargo.toml diff is 4 lines (version line, the two requirements, and the extended line-13 comment), as the plan specified the comment clause.

## Known Stubs

None.

## Self-Check: PASSED

- Commit ede85e2 `chore: release v1.6.0-rc.1` exists on release-1.6.0-rc.1, touching only Cargo.toml and Cargo.lock.
