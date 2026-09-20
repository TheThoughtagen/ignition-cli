---
phase: 01-foundation-agentic-contracts
plan: 02
subsystem: api
tags: [rust, thiserror, serde-json, error-taxonomy, exit-codes, golden-tests, snapbox, assert-cmd, cli-contract]

# Dependency graph
requires:
  - phase: 01-foundation-agentic-contracts (plan 01)
    provides: three-crate workspace, clap chassis with 5 global args, main() -> ExitCode single exit point, stderr-only tracing
provides:
  - CoreError enum with the LOCKED full 1-7 exit-code taxonomy (internal / confirmation_required / config×4 / network / auth / gateway_too_old / rig — reserved variants included, complete on day one)
  - code() stable slugs + exit_code() single mapping + hint() for every class (CORE-05) + endpoint() for network/auth/target-state
  - ErrorEnvelope/ErrorBody serde structs in LOCKED field order (ok, profile, error / code, message, endpoint, hint)
  - JsonEnvelope success shape {ok, profile, data} with render_success/render_failure (pretty + compact) — core returns Strings, never prints
  - main.rs typed dispatch Result<ActionOutput, CoreError> → render_ok/render_error → ExitCode::from(err.exit_code())
  - RenderMode resolved exactly once (--compact implies --json — LOCKED precedence), errors on stderr in every mode, success on stdout
  - snapbox+assert_cmd golden harness (contract_version.rs) — every later subcommand inherits it
  - README agent contract: envelope shape + exit-code table (the second of its exactly two homes)
affects: [01-03, 01-04, phase-2-inspection, all-json-output-commands]

# Tech tracking
tech-stack:
  added: [serde + serde_json in ignition-cli (envelope data payloads + contract-test parsing)]
  patterns: [exit-code mapping lives ONLY in CoreError::exit_code() + README (unit-enumerated), errors-on-stderr in every mode, RenderMode::resolve as the single precedence decision, ActionOutput payload struct serializes as envelope data (declaration order = golden order), inline str![] goldens via stdout_for_golden helper (println newline vs macro trimming)]

key-files:
  created:
    - crates/ignition-core/src/error.rs
    - crates/ignition-core/src/output.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/contract_version.rs
    - README.md
  modified:
    - crates/ignition-core/src/lib.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/Cargo.toml
    - Cargo.lock

key-decisions:
  - "Envelope field set frozen exactly {ok,profile,data}/{ok,profile,error} — changing it later is a breaking change for agents"
  - "Auth/GatewayTooOld carry an endpoint field so ErrorBody.endpoint is populable for network/auth/target-state (Network's url is its endpoint)"
  - "Network's reqwest::Error constructed in tests via real request to unroutable loopback port (http://127.0.0.1:1 — instant refusal; no public constructor exists)"
  - "profile echoes null until 01-03 threads the resolved name through — field exists day one so goldens change value, never shape"
  - "Error envelope serialized field-order locked at string level in a core unit test (serde_json::Value maps are key-sorted and would hide ordering)"
  - "ActionOutput lives in the bin for now with a payload struct per command (VersionData); migrates to core when actions land"

patterns-established:
  - "Exit codes decided in exactly one function: CoreError::exit_code(); ExitCode::from(err.exit_code()) is its only caller"
  - "Golden tests: isolate IGNITION_CLI_CONFIG per spawn, [..] elide dynamic values, strip println's single trailing newline via stdout_for_golden (snapbox str! trims leading+trailing newlines from literals)"
  - "Render modes: RenderMode::resolve(json, compact) computed once in main; render.rs owns human mode, core::output owns JSON strings, bin owns streams"

# Metrics
duration: 53min
completed: 2026-08-21
---

# Phase 1 Plan 02: Agentic Output Contract Summary

**LOCKED output contract shipped: full 1-7 CoreError exit taxonomy with stable slugs + hints, {ok,profile,data}/{ok,profile,error} envelope shapes, three render modes with --compact-implies-json, errors-on-stderr, and the snapbox golden harness every later subcommand inherits**

## Performance

- **Duration:** 53 min (includes ~30 min snapbox inline-golden semantics investigation via vendored source)
- **Started:** 2026-08-21T15:40:17Z
- **Completed:** 2026-08-21T16:33:38Z
- **Tasks:** 3
- **Files modified:** 9 (5 created, 4 modified)

## Accomplishments
- Exit-code taxonomy complete and frozen on day one: 10 CoreError variants across 7 classes (including NoActiveProfile/SecretUnavailable/Rig which no CLI path constructs yet) — enumerated unit test pins every code and slug so no later phase can silently renumber
- Envelope contract live end-to-end: `ign version --json` → pretty `{ok,profile,data}`, `--compact` → one-line JSON, default → human line; errors structurally wired to stderr in all three modes with message+hint (human) or full envelope (JSON)
- Five golden/contract tests green: envelope shape with `[..]` elision, compact one-line + exact key set, compact-implies-json precedence, clap exit-2 documented exception, human-mode-not-JSON separation
- README documents the agent-facing contract: envelope shapes, stderr discipline, and the exit-code table — the second of its exactly two homes

## Task Commits

Each task was committed atomically:

1. **Task 1: CoreError full taxonomy + ErrorEnvelope + exit-mapping unit test** - `dc1cf61` (feat)
2. **Task 2: JsonEnvelope render + main.rs Result dispatch + three render modes** - `40ec392` (feat)
3. **Task 3: Golden-file harness + version envelope goldens + README taxonomy** - `1ee8d59` (test)

## Files Created/Modified
- `crates/ignition-core/src/error.rs` - CoreError taxonomy, code()/exit_code()/hint()/endpoint(), ErrorEnvelope/ErrorBody (locked field order), 3 unit tests
- `crates/ignition-core/src/output.rs` - JsonEnvelope success shape + render_success/render_failure (pretty/compact); core returns Strings only
- `crates/ignition-core/src/lib.rs` - registers error + output modules
- `crates/ignition-cli/src/main.rs` - ActionOutput + typed dispatch + single exit mapping point; RenderMode resolved once
- `crates/ignition-cli/src/render.rs` - bin-only: RenderMode, render_ok (stdout), render_error (stderr, human+hint or envelope)
- `crates/ignition-cli/tests/contract_version.rs` - 5 golden/contract tests, SNAPSHOTS=overwrite workflow documented inline
- `crates/ignition-cli/Cargo.toml` + `Cargo.lock` - serde/serde_json added to bin
- `README.md` - agent contract: envelope shapes + exit-code table

## Decisions Made
- Envelope field set is the Phase-1 API freeze — exactly `{ok,profile,data}` / `{ok,profile,error}`; documented as breaking-change territory for agents
- Auth/GatewayTooOld carry optional `endpoint` fields (Network's `url` doubles as its endpoint) so CORE-05's endpoint promise is populable per-class at the type level
- reqwest::Error obtained in tests by a real request to `http://127.0.0.1:1` (instant TCP refusal) — no public constructor exists; wrapped in a `#[cfg(test)]`-local helper
- Error-envelope field ORDER locked by a string-level unit test (parsed `serde_json::Value` uses key-sorted maps that would hide ordering)
- TUI stub now returns typed `CoreError::Internal` (exit 1, per 01-01's decision note) instead of a bare string

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added serde + serde_json to ignition-cli dependencies**
- **Found during:** Task 2 (ActionOutput envelope payloads)
- **Issue:** Plan requires dispatch to produce serializable `data` payloads (`{"cli_version": ...}`) and Task 3 tests parse stdout as JSON; the bin crate had neither serde nor serde_json
- **Fix:** Added `serde` + `serde_json` (workspace single-sourced) to ignition-cli `[dependencies]`
- **Files modified:** crates/ignition-cli/Cargo.toml, Cargo.lock
- **Verification:** cargo clippy --workspace --all-targets -D warnings clean; all tests green
- **Committed in:** 40ec392 (Task 2 commit)

**2. [Rule 1 - Bug] thiserror 2.x Display-bound compile error on `{known}`**
- **Found during:** Task 1 (first compile)
- **Issue:** `#[error("... {known}")]` on `Vec<String>` fails in thiserror 2.x (`AsDisplay` bound unsatisfied) — the research sketch predated this tightening
- **Fix:** Switched to `{known:?}` Debug interpolation
- **Files modified:** crates/ignition-core/src/error.rs
- **Verification:** cargo test -p ignition-core green
- **Committed in:** dc1cf61 (Task 1 commit)

**3. [Rule 1 - Bug] snapbox 1.2.2 inline-golden mechanics differed from the research example**
- **Found during:** Task 3 (first golden run)
- **Issue:** (a) `String::from_utf8_lossy` returns `Cow<str>`, which does not implement `IntoData`; (b) the `str![]` macro strips BOTH leading and trailing newlines from literals while `println!` appends one — goldens written per the research example mismatched on the trailing newline
- **Fix:** Pass `&str` to `Assert::eq`; added `stdout_for_golden` helper stripping exactly one trailing newline, documented inline
- **Files modified:** crates/ignition-cli/tests/contract_version.rs
- **Verification:** 5/5 contract tests green; `SNAPSHOTS=overwrite cargo test` is a clean no-op (workflow proven, no source drift)
- **Committed in:** 1ee8d59 (Task 3 commit)

---

**Total deviations:** 3 auto-fixed (1 blocking, 2 bug)
**Impact on plan:** All fixes required to make the plan's own verification pass. No scope creep; the snapbox findings are recorded as a pattern so 01-03/01-04 goldens avoid the same friction.

## Issues Encountered
None beyond the documented deviations — the `[..]` elision, compact/pretty rendering, exit-code mapping, and all verification greps held on first pass after the fixes above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Output contract FROZEN and golden-enforced; 01-03 (config/profiles) threads the resolved profile name into the envelope's `profile` echo and starts constructing config-class CoreErrors — the taxonomy, envelope, and golden harness are waiting
- 01-04 (gateway version check) will exercise Network/Auth/GatewayTooOld end-to-end against wiremock and grow `data` with gateway fields
- Blockers: none

## Self-Check: PASSED

Verified: all 5 created files exist on disk; commits dc1cf61 / 40ec392 / 1ee8d59 present on main; cargo test --workspace green (15 tests: 7 chassis + 5 contract + 3 core unit); SNAPSHOTS=overwrite no-op; `ign version --json` top-level keys exactly {ok, profile, data}; `process::exit` grep = 0; stream discipline grep clean (println only in render_ok paths, eprintln only in render_error, tracing writer = stderr); README table rows 0-7 match `CoreError::exit_code()` unit-test values; fmt + clippy -D warnings clean.

---
*Phase: 01-foundation-agentic-contracts*
*Completed: 2026-08-21*
