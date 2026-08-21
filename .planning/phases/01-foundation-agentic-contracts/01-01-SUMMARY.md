---
phase: 01-foundation-agentic-contracts
plan: 01
subsystem: infra
tags: [rust, cargo-workspace, clap, tokio, tracing, ci, github-actions, msrv]

# Dependency graph
requires:
  - phase: none (first plan of the project)
    provides: greenfield repository
provides:
  - Three-crate Cargo workspace (ignition-cli bin `ign` / ignition-core lib / ignition-tui stub) — shape is FINAL
  - workspace.dependencies single-sourcing (clap 4.6, serde, tokio 1.53, reqwest 0.13, toml 1.1, directories 6.0, keyring 4.1, thiserror 2.0, semver, tracing, url; dev: wiremock, snapbox, assert_cmd, predicates, tempfile)
  - MSRV 1.88 locked in [workspace.package] (keyring 4.1.x floor — corrects STACK.md's 1.85)
  - Clap chassis: five global args (--profile/--json/--compact/--yes/--verbose) defined once with global=true
  - Single-exit-point main() -> ExitCode: try_parse → apply_env_defaults → init_tracing → tokio runtime → dispatch
  - apply_env_defaults as the ONLY env→flag precedence point (IGNITION_PROFILE/JSON/YES)
  - stderr-only tracing (verbosity 0=warn/1=info/2=debug/3+=trace, IGNITION_LOG override); stdout stays data-only
  - tui feature gate (default on; --no-default-features lean agent build compiles both ways)
  - CI check workflow on ubuntu-latest + macos-latest (fmt/clippy -D warnings/build/test/lean build)
  - webdev/ scaffold for Phase 5 routes
affects: [01-02, 01-03, 01-04, phase-2-inspection, phase-6-tui]

# Tech tracking
tech-stack:
  added: [clap 4.6 (derive), tokio 1.53 (rt-multi-thread), tracing + tracing-subscriber 0.3 (env-filter), serde/serde_json, reqwest 0.13, toml 1.1, directories 6.0, keyring 4.1, thiserror 2.0, semver 1.0, url 2.5, clap_complete 4.6 (declared, unused until 01-02), wiremock 0.6 (dev), snapbox 1.2 (dev), assert_cmd 2.2 (dev), predicates 3.1 (dev), tempfile 3.27 (dev)]
  patterns: [virtual workspace + workspace.dependencies single-sourcing, clap global args defined once at root, main() -> ExitCode single-exit-point, env→flag in exactly one function, stderr-only diagnostics, feature-gated optional crate dep (dep:ignition-tui), Cargo.lock committed for binary crate]

key-files:
  created:
    - Cargo.toml
    - Cargo.lock
    - .gitignore
    - crates/ignition-cli/Cargo.toml
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/tests/cli_chassis.rs
    - crates/ignition-core/Cargo.toml
    - crates/ignition-core/src/lib.rs
    - crates/ignition-tui/Cargo.toml
    - crates/ignition-tui/src/lib.rs
    - webdev/README.md
    - .github/workflows/ci.yml
  modified: []

key-decisions:
  - "MSRV 1.88 (not 1.85): keyring 4.1.x declares rust_version 1.88.0 — every other core crate sits at 1.85, keyring alone sets the floor"
  - "Minimal placeholder main.rs shipped in Task 1 (bin crate cannot build without it) and replaced by the real chassis in Task 2"
  - "Doc comments worded so contract greps hold exactly: `global = true` matches = 5, `process::exit` matches = 0 in crates/"
  - "Edition 2024 let-chains used in apply_env_defaults (collapsible_if-clean under clippy -D warnings)"
  - "Tui subcommand stub errors with exit 1 + stderr message for now; typed CoreError mapping lands in 01-02"

patterns-established:
  - "Globals-once: the five global flags live only in cli.rs; subcommand structs never redeclare them"
  - "Single-exit-point: main() -> ExitCode; no direct exit calls outside clap's Error::exit"
  - "Env precedence: flags > IGNITION_* env > defaults, resolved in apply_env_defaults only"
  - "stdout is data-only: all tracing/diagnostics render to stderr"
  - "Test isolation: every binary test sets IGNITION_CLI_CONFIG to an isolated path (directories ignores XDG on macOS)"

# Metrics
duration: 8min
completed: 2026-08-21
---

# Phase 1 Plan 01: Workspace Skeleton & CLI Chassis Summary

**Three-crate Cargo workspace (ign / ignition-core / ignition-tui) with MSRV 1.88, five global clap args defined once, single-exit-point main() -> ExitCode, env defaults in one function, stderr-only tracing, tui feature gate, and day-one ubuntu+macos CI**

## Performance

- **Duration:** 8 min
- **Started:** 2026-08-21T15:22:38Z
- **Completed:** 2026-08-21T15:30:48Z
- **Tasks:** 3
- **Files modified:** 13 (created; 0 pre-existing modified)

## Accomplishments
- Workspace shape is FINAL from commit one: virtual manifest (resolver 3, edition 2024, rust-version 1.88), single-sourced dependency versions, zero-dependency ignition-tui stub proving the Phase-6 feature-gate structure
- `ign` binary chassis: `--help` lists all five global flags, `version` subcommand works on a fresh install with no config, unknown flags exit 2 from clap's own renderer, env (`IGNITION_JSON=1`/`IGNITION_YES=1`/`IGNITION_PROFILE`) applies in exactly one function
- Diagnostics contract enforced by test: `ign version -v` keeps stdout exactly the version line while tracing is wired to stderr
- CI check job committed green-shaped (fmt / clippy -D warnings / build / test / lean build on ubuntu+macos); fmt+clippy verified locally before commit

## Task Commits

Each task was committed atomically:

1. **Task 1: Workspace manifests + three crates + tui feature gate + webdev/ scaffold** - `b9872c4` (feat)
2. **Task 2: Clap chassis — 5 global args, single-exit-point main, env defaults, stderr tracing, version stub** - `d286e93` (feat) + `e911f02` (fix: doc-comment wording so contract greps hold exactly)
3. **Task 3: CI workflow — check job on ubuntu+macos** - `f1cbe8c` (chore)

## Files Created/Modified
- `Cargo.toml` - Virtual workspace manifest; MSRV 1.88; workspace.dependencies single-sourcing
- `Cargo.lock` - Committed on purpose (binary crate → reproducible agent installs)
- `.gitignore` - /target, .DS_Store, /tmp
- `crates/ignition-cli/Cargo.toml` - Bin crate `ign`; features default=["tui"], tui=["dep:ignition-tui"]
- `crates/ignition-cli/src/cli.rs` - Cli derive: five global args (global=true) + Version/Tui (cfg-gated) subcommands
- `crates/ignition-cli/src/main.rs` - main() -> ExitCode; try_parse→e.exit(); apply_env_defaults; init_tracing (stderr-only, IGNITION_LOG override); tokio runtime + async dispatch
- `crates/ignition-cli/tests/cli_chassis.rs` - 7 binary-level tests (help/version/fresh-install/env×2/exit-2/stdout-clean)
- `crates/ignition-core/Cargo.toml` - Lib crate with full Phase-1 dependency surface
- `crates/ignition-core/src/lib.rs` - Crate docs + layering invariants; modules land in 01-02+
- `crates/ignition-tui/Cargo.toml` + `crates/ignition-tui/src/lib.rs` - Zero-dependency stub (no ratatui until Phase 6)
- `webdev/README.md` - Phase 5 WebDev routes scaffold
- `.github/workflows/ci.yml` - Check job matrix ubuntu+macos, six steps, no Windows

## Decisions Made
- MSRV 1.88 per research correction (keyring 4.1.x floor) — encoded in [workspace.package] and inherited by all three crates
- Task 1 shipped a minimal placeholder main.rs (bin crate cannot compile without one); Task 2 replaced it with the real chassis — no wasted restructure
- Doc comments deliberately avoid the literal contract strings (`global = true`, `process::exit`) so the verification greps hold at exactly 5 and 0
- Used edition-2024 let-chains in apply_env_defaults, keeping clippy -D warnings clean rather than adding an allow

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added placeholder main.rs to Task 1**
- **Found during:** Task 1 (workspace skeleton)
- **Issue:** Plan's Task 1 file list omitted main.rs, but `cargo build --workspace` (Task 1's own verify) cannot succeed for a bin crate with no src/main.rs
- **Fix:** Created a minimal version-printing placeholder that compiles in both feature configurations; replaced by the real chassis in Task 2
- **Files modified:** crates/ignition-cli/src/main.rs
- **Verification:** cargo build --workspace + no-default-features both green at Task 1 commit
- **Committed in:** b9872c4 (Task 1 commit)

**2. [Rule 1 - Bug] Reworded doc comments that broke contract greps**
- **Found during:** Task 2 verification
- **Issue:** Plan verification requires `grep -c "global = true" cli.rs` = 5 and `grep -rn "process::exit" crates/` = 0; doc comments contained those literal strings as prose (counts were 6 and 1)
- **Fix:** Reworded the doc comments to describe the contracts without the literal patterns
- **Files modified:** crates/ignition-cli/src/cli.rs, crates/ignition-cli/src/main.rs
- **Verification:** Counts now exactly 5 and 0; full test suite still green
- **Committed in:** e911f02

**3. [Rule 1 - Bug] Collapsed nested if to satisfy clippy -D warnings**
- **Found during:** Task 3 pre-commit verification
- **Issue:** clippy::collapsible_if fired on apply_env_defaults' nested if/let-chain (CI would have been red on first push)
- **Fix:** Collapsed to a single edition-2024 let-chain condition
- **Files modified:** crates/ignition-cli/src/main.rs
- **Verification:** cargo clippy --workspace --all-targets -- -D warnings clean in both feature configurations; all tests pass
- **Committed in:** f1cbe8c (Task 3 commit)

---

**Total deviations:** 3 auto-fixed (2 bug, 1 blocking)
**Impact on plan:** All fixes necessary for the plan's own verification criteria and first-push-green CI. No scope creep.

## Issues Encountered
None beyond the documented deviations — dependency resolution, MSRV, edition-2024, and assert_cmd test wiring all worked on first pass.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Workspace shape final; plan 01-02 (config/profiles + CoreError envelope) adds modules to ignition-core and the full exit-code mapping to main — no restructuring needed
- Blockers: none. Note (per STATE.md): keyring smoke-test on headless Linux CI remains a 01-03 concern; this plan's CI deliberately omits the keyring-smoke job

## Self-Check: PASSED

Verified: all 13 created files exist on disk; commits b9872c4 / d286e93 / e911f02 / f1cbe8c present on main; both feature configs build; cargo test --workspace green (7 chassis tests); globals grep = 5; exit grep = 0; fmt + clippy -D warnings clean.

---
*Phase: 01-foundation-agentic-contracts*
*Completed: 2026-08-21*
