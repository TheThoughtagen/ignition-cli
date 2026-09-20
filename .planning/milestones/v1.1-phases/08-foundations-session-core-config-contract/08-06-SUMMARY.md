---
phase: 08-foundations-session-core-config-contract
plan: 06
subsystem: contract-testing
tags: [exit-codes, error-taxonomy, readme-table, include-str, out-of-band, clap-walk, stdout-purity, assert-cmd, byte-exact]

# Dependency graph
requires:
  - phase: 08-foundations-session-core-config-contract (08-01)
    provides: poll_interval_too_small slug in the exit-3 class and the doc-comment taxonomy row
  - phase: 06-foundations (TUI)
    provides: routes() registry and the clap-tree-walk coverage tests the taxonomy extension pins
provides:
  - readme_exit_table_agreement test: machine-enforced Three-Place slug rule (README table ↔ literal triples, both directions)
  - Pre-declared reserved OutOfBand slugs (mcp/lsp/edit) with written justification in pinned test + routes.rs docs, zero registry rows
  - contract_stdout_purity.rs: byte-exact stdout harness over the real spawned binary under max diagnostics (assert-based, overwrite-proof)
affects: [13-mcp, 14-transports (reserved slugs land with clap commands), any future exit-code/slug change (agreement test enforces README sync)]

# Tech tracking
tech-stack:
  added: [] # no new dependencies (assert_cmd/predicates/tempfile/serde_json already present)
  patterns: [include_str! doc-sync test (README parsed as contract data), reserved-slug pre-declaration (taxonomy before rows), byte-exact stdout purity via raw output.stdout comparison]

key-files:
  created:
    - crates/ignition-cli/tests/contract_stdout_purity.rs
  modified:
    - crates/ignition-core/src/error.rs
    - README.md
    - crates/ignition-cli/tests/tui_coverage.rs
    - crates/ignition-tui/src/routes.rs

key-decisions:
  - "README parser scoped to the '## Exit codes' section — the README's unrelated tables contain rows coincidentally beginning '| 5 |' that polluted a whole-file parse (caught by the parser's own failure on first run)"
  - "Literal (exit, slug) table duplicated beside the enumerated test rather than shared: the enumerated test stays the literal source of truth, drift between the two surfaces as a failure instead of silent co-shuffling"
  - "Purity harness is assert-based on raw output.stdout, deliberately not snapbox goldens, so SNAPSHOTS=overwrite cannot sanitize a leaked byte"
  - "Purity harness strips ambient IGNITION_PROFILE/IGNITION_JSON/IGNITION_YES — a developer shell with one exported would turn byte-exact assertions into shell-dependent flakes"

patterns-established:
  - "Three-Place rule is now executable: enum test (enum ↔ literals) + README agreement test (README ↔ enum); a slug change must touch code, literals, and README or CI fails"
  - "Reserved OutOfBand slugs: pre-declare taxonomy + justification BEFORE the clap commands exist; rows land together with commands (orphan rows fail the walk by design)"

# Metrics
duration: 189min
completed: 2026-09-05
---

# Phase 08 Plan 06: Contract Enforcement Rituals Summary

**Three-Place slug rule machine-enforced via a README-parsing agreement test, mcp/lsp/edit pre-declared as reserved OutOfBand slugs with written justification, and a byte-exact stdout-purity harness over the real spawned binary under maximum diagnostics**

## Performance

- **Duration:** 189 min (dominated by sibling-agent cargo lock contention on the shared worktree; hands-on work ~45 min)
- **Started:** 2026-09-05T17:38:17Z
- **Completed:** 2026-09-05T20:47:19Z
- **Tasks:** 3
- **Files modified:** 5 (1 created, 4 modified)

## Accomplishments
- `readme_exit_table_agreement` test parses the README exit-code table via `include_str!` and cross-checks every slug ↔ exit-code row against the literal (exit, slug) table in BOTH directions — a missing slug fails direction (a), a stale/deleted row fails direction (b). The Three-Place rule (enum + literals + README) is now executable.
- OutOfBand taxonomy extended WITHOUT rows: `mcp`/`lsp`/`edit` pre-declared as reserved slugs with justification in the pinned test's doc-comment, the module-level exceptions doc, and routes.rs (variant doc + out-of-band prose). Set stays exactly `["completions"]`; both clap-walk tests green.
- `contract_stdout_purity.rs` enforces byte-exact stdout over the REAL binary under unknown-key warnings + `IGNITION_LOG=trace`: JSON mode byte-equals the empty-list envelope, human mode byte-equals zero bytes. Assert-based, overwrite-proof.

## Task Commits

Each task was committed atomically:

1. **Task 1: Three-Place agreement test** - `1c44644` (test)
2. **Task 2: OutOfBand taxonomy extension (reserved slugs, no rows)** - `28f2782` (docs)
3. **Task 3: stdout-purity byte-scan harness** - `7b01728` (test)

## Files Created/Modified
- `crates/ignition-core/src/error.rs` - `readme_exit_table_agreement` + `parse_readme_exit_table` + duplicated `EXIT_SLUG_LITERALS` table (parser + tests module)
- `README.md` - exit-code-table prose now names BOTH sync tests (enumerated + agreement)
- `crates/ignition-cli/tests/tui_coverage.rs` - reserved-slug justification in pinned OutOfBand test + module docs
- `crates/ignition-tui/src/routes.rs` - reserved-slug pre-declaration in `Mapping::OutOfBand` doc + out-of-band prose note
- `crates/ignition-cli/tests/contract_stdout_purity.rs` - CREATED: two purity tests (JSON byte-exact, human zero-byte) over `ign profile list` under max diagnostics

## Negative Proofs (recorded per plan)

- **Task 1, direction (a):** removed `tag_collision` from the README table → test FAILED: "README exit-6 row is missing slug \"tag_collision\"" (run in an isolated worktree). Restored → green.
- **Task 1, direction (b):** added `bogus_stale_slug` to the README table → test FAILED: "README exit-6 row carries slug \"bogus_stale_slug\" that no CoreError variant emits". Restored → green.
- **Task 3:** off-by-one expected string (stray `x` prefix) → purity test FAILED at "first differing byte at offset 0" with diff-friendly context. Reverted → 2/2 green.

## Decisions Made
- Scoped the README parser to the `## Exit codes` section after the first run caught an unrelated README table's `| 5 |` row leaking a docker-compose path into the slug set (the whole-file parse failed on real drift — proof the test bites).
- Duplicated the (exit, slug) literals beside the enumerated test instead of sharing a helper: the enumerated test remains the literal source of truth; the duplication is documented at both sites so drift surfaces as a loud failure.
- Hard-coded the expected stdout bytes once (derived from the real binary at HEAD): JSON `{"ok":true,...,"profiles":[]}` + trailing newline; human mode with no profiles and no active profile prints NOTHING (header suppressed) — pinned as zero bytes.
- Added `env_remove` guards for ambient `IGNITION_PROFILE`/`IGNITION_JSON`/`IGNITION_YES` so byte-exact assertions are deterministic regardless of the invoking shell.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] README exit-table parser scoped to its section**
- **Found during:** Task 1 (agreement test first run)
- **Issue:** Whole-file row matching picked up an unrelated README table whose rows begin `| 5 |` / `| 3 |` (line ~285), injecting `ignition-git-module/docker/docker-compose.yml` as a "slug" and failing the test
- **Fix:** Parse only after the `## Exit codes` heading, stop at the first non-table line; `rows.len() >= 7` guard keeps a renamed heading a loud failure
- **Files modified:** crates/ignition-core/src/error.rs
- **Verification:** agreement test green; negative proofs still fail correctly
- **Committed in:** 1c44644 (Task 1 commit)

**2. [Rule 3 - Blocking] Post-commit `cargo fmt` collapse of the Task 1 parser**
- **Found during:** Task 3 verification (fmt --check)
- **Issue:** Task 1 was committed before running `cargo fmt`; two multi-line expressions needed collapsing
- **Fix:** `cargo fmt -p ignition-core`; folded into the Task 3 commit (no amend — sibling agents build on main)
- **Files modified:** crates/ignition-core/src/error.rs
- **Verification:** tree-wide `cargo fmt --check` clean
- **Committed in:** 7b01728 (Task 3 commit)

---

**Total deviations:** 2 auto-fixed (2 blocking)
**Impact on plan:** Both fixes were required to satisfy the plan's own verify gates. No scope creep: no new clap commands, no registry rows, no new exit codes, no new dependencies.

## Issues Encountered
- **Sibling-agent contention (expected per coordination note):** 08-03/08-04 migrated CLI/TUI call sites concurrently on the same worktree. Their mid-flight `session.rs` WIP briefly broke `ignition-core` compilation; Task 1 verification was completed in an isolated `git worktree` at HEAD with only my two modified files copied in (siblings' files untouched). The full `cargo test --workspace` run required ~30 min wall time across contentions and one transient failure (`tui_under_a_pipe_refuses_with_the_interactive_terminal_hint`) that passed on immediate re-run — the predicted mid-flight flake.
- Final full-suite result: **889 passed, 0 failed across 50 test binaries**; `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean tree-wide.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Phase 08 Success Criterion 3 (contract enforcement) is complete: the frozen contract cannot be mutated by accident — slug changes must land in code + literals + README simultaneously, OutOfBand futures are documented where implementers will look, and stdout purity is byte-enforced.
- Phases 13/14 must extend the OutOfBand set (`mcp`/`lsp`/`edit`) TOGETHER with their clap commands; the pinned test's doc-comment states exactly where.
- Phase 08's remaining plans: none for this executor — 08-03/08-04/08-05 are the siblings' responsibility.

---
*Phase: 08-foundations-session-core-config-contract*
*Completed: 2026-09-05*

## Self-Check: PASSED
- crates/ignition-cli/tests/contract_stdout_purity.rs exists on disk
- Commits 1c44644, 28f2782, 7b01728 all present in git log
- readme_exit_table_agreement present in error.rs; mcp pre-declaration present in routes.rs
- Full workspace suite: 889 passed / 0 failed (50 binaries); clippy -D warnings clean; fmt clean
