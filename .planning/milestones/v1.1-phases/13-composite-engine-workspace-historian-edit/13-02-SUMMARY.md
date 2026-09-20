---
phase: 13-composite-engine-workspace-historian-edit
plan: 02
subsystem: core-library
tags: [proptest, path-mapping, percent-encoding, fnv1a, zip-surgery, injectivity, workspace]

# Dependency graph
requires:
  - phase: 05-02 (member surgery) / 07-01 (diff engine)
    provides: the proven v1.0 member engine in client/resources.rs — resource_members/read_member/member_hashes/normalize_descriptor/fnv1a — that MemberSource::Zip delegates to verbatim
provides:
  - client/workspace.rs: segment_escape/segment_unescape/local_path_for — the injective hostile-name-safe user-path→fs-path mapping (per-segment %XX escaping, fail-closed refusals)
  - build_mapping — set-level total mapping with ASCII case-fold collision refusal naming BOTH members + exact-duplicate refusal, deterministic (order-stable)
  - MemberSource enum (Zip | Tree) — one member engine over gateway export zip AND checked-out directory tree, identical members()/member_hashes()/read semantics, equivalence test-pinned
  - crates/ignition-core/tests/workspace_path_mapping.rs — proptest suite P1–P6 (round-trip bijection, byte-wise injectivity, refusals naming the member, safe-domain idempotence, set-level totality+injectivity, adversarial fold-collision refusals)
  - proptest as a DEV-dependency of ignition-core ONLY (never ships in the binary)
affects: [13-03 workspace checkout (consumes build_mapping + records manifest), 13-06 status three-way compare (compares Tree-vs-Zip member_hashes), 13-04/13-05 historian, 13-07/13-08 edit/push]

# Tech tracking
tech-stack:
  added: [proptest 1.11 (dev-dep only)]
  patterns: [per-segment percent-escaping with %XX-only escape form, fail-closed mapping refusals riding CoreError::InvalidInput, recorded-not-recomputed checkout mapping, manifest-scoped strict tree source]

key-files:
  created:
    - crates/ignition-core/src/client/workspace.rs
    - crates/ignition-core/tests/workspace_path_mapping.rs
  modified:
    - crates/ignition-core/src/client/resources.rs
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/Cargo.toml

key-decisions:
  - "Escaping scheme per planner lock: per-segment percent-encoding, safe bytes [A-Za-z0-9._-], %XX-only escape form (so no double-decode trap — literal %2e never collapses to .)"
  - "Empty segment refuses — same refusal class as ./.. (planner's decide-and-pin, pinned in segment_escape + property tests)"
  - "255-byte cap checked on the ESCAPED segment (that is the on-disk name; escaping can inflate 1 byte to 3)"
  - "Unicode case NOT folded beyond ASCII — NFC/NFD variants stay distinct files; only ASCII fold-collisions refuse at build_mapping level"
  - "Visibility widening kept minimal: only fnv1a + FOLDER_DESCRIPTOR went pub(crate) — normalize_descriptor/resource_members/read_member/member_hashes were already pub; member_path/user_path stay private (not needed by the delegation)"
  - "Tree unknown-file policy pinned at the source: manifest-scoped strict (invalid_input) — 13-06's status handles unknown files at the action layer"

patterns-established:
  - "Property tests as the driving spec: RED run against declared-but-empty API first, implement to green — the failing properties ARE the spec"
  - "Sabotage-check discipline: deliberately disabling a guard (case-fold check) must turn its property test red — proves the test bites"
  - "Single-implementation delegation: workspace.rs defines NO engine functions — Zip branch calls resources.rs verbatim; grep-verifiable"

# Metrics
duration: 19min
completed: 2026-09-14
---

# Phase 13 Plan 02: Workspace Path Mapping + MemberSource Summary

**Injective hostile-name-safe path mapping machine-proven by proptest over hostile corpora (P1–P6), plus MemberSource exposing the proven zip member engine over checked-out trees with equivalence pinned — zero product actions, the pure foundation 13-03/13-06 build on.**

## Performance

- **Duration:** 19 min
- **Started:** 2026-09-14T21:44:01Z
- **Completed:** 2026-09-14T22:03:48Z
- **Tasks:** 3
- **Files modified:** 5 (2 created, 3 modified)

## Accomplishments
- The SC-2 core exists in machine-proven form: `segment_escape`/`segment_unescape`/`local_path_for` with proptest properties P1–P4 (exact round-trip bijection, byte-wise injectivity, fail-closed refusals naming the member, safe-domain idempotence) over 512 randomized cases each — case pairs, percent traps (`%2e` never double-decodes), control bytes, NFC/NFD unicode pairs, deep nesting
- Set-level injectivity: `build_mapping` refuses ASCII fold collisions naming BOTH members (`P13/A`/`p13/a`, `Foo`/`foo`) with order-stable messages, refuses exact duplicates, and the P5/P6 properties plus a live sabotage check (fold-guard disabled → P6 red → reverted) prove the tests bite — Pitfall W1's silent-loss class is closed at the mapping layer
- `MemberSource` (Zip | Tree): Zip delegates VERBATIM to resources.rs; Tree reads through the recorded mapping with the SAME descriptor-normalized hash semantics — equivalence pinned by fixtures including a descriptor hashed under DIFFERENT lastModification values on each side and a hostile-name member (`my file%20`) round-tripped byte-exact through `my%20file%2520`
- Full ignition-core suite green: 391 lib tests (all pre-existing resources.rs pins untouched) + 21 property/equivalence tests; lean `--no-default-features` build compiles (proptest is dev-only)

## Task Commits

Each task was committed atomically:

1. **Task 1: proptest dev-dep + path mapping with property tests as the driving spec** - `92e46ab` (feat)
2. **Task 2: set-level injectivity — case-collision detection across the whole member set** - `00504d8` (feat)
3. **Task 3: MemberSource extraction — Zip delegates verbatim, Tree over the mapping, equivalence pinned** - `d3c7c16` (feat)

_Note: Task 1 ran test-first (RED: all 13 tests failed `not yet implemented` against the declared-but-empty API, proptest shrinking named minimal inputs; GREEN: implement to 13/13). `build_mapping`'s implementation + module docs rode Task 1's GREEN commit (same mapping implementation), so Task 2's commit carries the P5/P6 tests + sabotage evidence; Task 3's commit carries the visibility widening + MemberSource + equivalence tests._

## Files Created/Modified
- `crates/ignition-core/src/client/workspace.rs` (created) — mapping + build_mapping + MemberSource; module docs pin the full scheme for 13-03's README work
- `crates/ignition-core/tests/workspace_path_mapping.rs` (created) — proptest suite P1–P6 + zip/tree equivalence + manifest-scoped strictness pins
- `crates/ignition-core/src/client/resources.rs` — ONLY `fnv1a` + `FOLDER_DESCRIPTOR` widened to `pub(crate)` with delegation-boundary comments; zero behavior change
- `crates/ignition-core/src/client/mod.rs` — `pub mod workspace;`
- `crates/ignition-core/Cargo.toml` — `proptest = "1.11.0"` under `[dev-dependencies]` only

## Decisions Made
- Empty-segment refusal pinned inside `segment_escape` (same class as `.`/`..`), per the planner's decide-and-pin
- 255-byte cap enforced on the ESCAPED segment (the on-disk name) — documented, property-tested (85 spaces = 255 escaped passes; 86 = 258 refuses)
- `segment_unescape` enforces the strict image of escape: malformed `%`, decoded NUL/`/`/`.`/`..`/empty all refuse — a hand-written local path cannot smuggle structure back through decode
- Minimal visibility widening: `member_path`/`user_path` stay private — the Zip delegation needs only the already-pub primitives; Tree needed only `fnv1a` + `FOLDER_DESCRIPTOR`
- No new exit slugs: mapping refusals ride `CoreError::InvalidInput` (exit 2) with the member path in the reason; the tree-walk I/O failure rides the existing `Internal` (resources.rs precedent)

## Deviations from Plan

None — plan executed exactly as written. (File-placement note, not a deviation: `build_mapping`'s code + module docs landed in Task 1's commit alongside the mapping they compose; Task 2's commit carries its tests as planned.)

## Issues Encountered
- proptest assertion macros reject inline format-string capture (`{a:?}`) — switched to positional format args (mechanical, caught at first compile)
- proptest's RED run persisted a regressions file from the intentional stub panics — deleted before GREEN (the failures were the spec, not mapping bugs)

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness
- 13-03 checkout consumes `build_mapping` (call once, record gateway↔local pairs in the manifest — never re-derive) and writes files through `local_path_for`'s outputs
- 13-06 status compares `MemberSource::Tree` vs `MemberSource::Zip` `member_hashes()` — one diff/normalization implementation serves both sources
- Readiness verified: equivalence (zip↔tree), strictness (manifest-scoped tree), and the refusal classes (traversal/NUL/case-fold/duplicate/length) are all machine-pinned; no blockers

---
*Phase: 13-composite-engine-workspace-historian-edit*
*Completed: 2026-09-14*

## Self-Check: PASSED

- Files created verified on disk: workspace.rs, tests/workspace_path_mapping.rs (684 lines, proptest-driven)
- Commits verified in history: 92e46ab (Task 1), 00504d8 (Task 2), d3c7c16 (Task 3)
- Final verification battery re-run green: cargo test -p ignition-core (391 lib + 21 suite), workspace clippy -D warnings clean, fmt --check clean, proptest under [dev-dependencies] only, --no-default-features build compiles
