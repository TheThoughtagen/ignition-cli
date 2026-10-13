---
phase: 21-deploy-render
plan: 01
subsystem: testing
tags: [fixtures, sanitizer, corpus, openapi, deploy, gitattributes]
status: complete

requires:
  - phase: 20
    provides: workspace conventions and CI matrix (ubuntu/macos/windows) that run the byte-identity gates
provides:
  - Sanitized 183-file gateway corpus (183 json, 8 css, 4 stand-in jars) under tests/fixtures/deploy/corpus
  - Re-runnable pattern-based sanitizer (sanitize.py) and SANITIZE.md rules S1-S6 with counts
  - Committed 8.3.6 OpenAPI resource-type extract (56 types, secret field paths)
  - deploy_fixtures.rs fixture guard (9 tests) with scanners proven against synthetic leaks
affects: [21-05 corpus gate, 21-06 deny table, 21-07 TYPE_RULES, 21-08 secret lint]

actuals:
  tokens: 20000   # chars/4 over the non-corpus files changed (tests, scripts, extract, docs); the 195 corpus fixtures are excluded
  tasks: 2
  commits: 4

tech-stack:
  added: []
  patterns:
    - "Fixtures under fixtures/deploy/** are -text so CRLF conversion never touches byte-identity gates"
    - "Corpus and extract are committed evidence; later tests do not need the external source"

key-files:
  created:
    - crates/ignition-core/tests/fixtures/deploy/sanitize.py
    - crates/ignition-core/tests/fixtures/deploy/SANITIZE.md
    - crates/ignition-core/tests/fixtures/deploy/corpus/
    - crates/ignition-core/tests/fixtures/deploy/extract_openapi.py
    - crates/ignition-core/tests/fixtures/deploy/openapi-8.3.6-resource-types.json
    - crates/ignition-core/tests/deploy_fixtures.rs
  modified:
    - .gitattributes
    - .planning/phases/21-deploy-render/21-RESEARCH.md
    - .planning/phases/21-deploy-render/21-01-PLAN.md

key-decisions:
  - "ignition/email-profile secret path is settings.settings.password (RESEARCH Pattern 7 had a typo); user approved at checkpoint"
  - "OpenAPI taxonomy is 56 types (39 named, 17 singleton), not RESEARCH's 87/49; user accepted"

requirements-completed: [DRND-05, DRND-06, DRND-07]

coverage:
  - id: D1
    description: "Sanitized 183-file corpus plus 8 css and 4 stand-in jars, no real secret/GUID/IP/mail host committed"
    requirement: "DRND-05"
    verification:
      - kind: unit
        ref: "cargo test -p ignition-core --test deploy_fixtures"
        status: pass
  - id: D2
    description: "OpenAPI 8.3.6 resource-type extract (named vs singleton, secret field paths)"
    requirement: "DRND-06"
    verification:
      - kind: unit
        ref: "crates/ignition-core/tests/deploy_fixtures.rs#openapi_extract_shape"
        status: pass
  - id: D3
    description: "CR-bearing binary stand-in jars and -text gitattributes so render fixtures survive Windows checkout"
    requirement: "DRND-07"
    verification:
      - kind: unit
        ref: "crates/ignition-core/tests/deploy_fixtures.rs#text_fixtures_have_no_carriage_return"
        status: pass
---

# Phase 21 Plan 01: Sanitized deploy corpus, OpenAPI extract and fixture guard Summary

Pattern-sanitized 183-file gateway corpus, a 56-type OpenAPI resource taxonomy extract and a 9-test fixture guard, committed before any render code exists so later plans test against public-safe evidence.

## Source identity gate

Observed on the source snapshot before sanitizing: 195 regular files (183 json, 8 css, 4 jar), 0 symlinks, aggregate SHA-256 manifest digest `6477b7e5e010cbe56f1ded82b080108c31763e357d5c604d91bca5f5b8f0bfcf`. Both match the plan's expected values, so the gate passed. After this plan the committed corpus plus SANITIZE.md is the source of truth.

## Sanitizer counts (user-run)

| Rule | Count |
|------|-------|
| S1 Embedded `data` strings (files touched) | 10 |
| S2 distinct Entra GUIDs | 2 |
| S3 IPv4 occurrences | 5 |
| S4 tenant mail hosts | 1 |
| S5 jar stand-ins | 4 |
| Verbatim copies | 175 |

The sanitizer was run by the user rather than the executor. No sanitized-away value is recorded here.

## Accomplishments

- Task 1 (05f155a): corpus, `sanitize.py`, `SANITIZE.md`, `.gitattributes` (`fixtures/deploy/** -text`, `*.jar binary`), and the `deploy_fixtures.rs` guard with secret, IPv4, Entra GUID, mail-host and CR scanners plus inventory checks.
- Task 2 (174d35c): `extract_openapi.py` walks the live 8.3.6 OpenAPI file into `openapi-8.3.6-resource-types.json` (56 types: 39 named, 17 singleton, with secret field paths) and `openapi_extract_shape` pins its shape.
- Docs correction (0b684f8): RESEARCH Pattern 7 and the type-count claims aligned with the evidence.

## Task Commits

1. Task 1: `05f155a` test(21-01): add sanitized deploy corpus, sanitizer and fixture guard
2. Task 2: `174d35c` test(21-01): add OpenAPI resource-type extract and shape guard
3. Docs correction: `0b684f8` docs(21-01): correct RESEARCH email-profile secret path and OpenAPI type counts

## Deviations from Plan

### User-approved at the decision checkpoint

**1. [Checkpoint - secret path] email-profile secret path is `settings.settings.password`**
- **Found during:** Task 2
- **Issue:** The plan required `secret_fields` to equal RESEARCH Pattern 7 exactly, which lists `ignition/email-profile` as `settings.password`. The OpenAPI walk truthfully yields `settings.settings.password`, and `corpus/ignition/email-profile/WH_Classic_SMTP/config.json` confirms the `settings.settings.*` layout. The RESEARCH row was a typo.
- **Fix:** User approved the walk's output. RESEARCH Pattern 7 row and the 21-01-PLAN truth line were corrected. 21-07-PLAN.md and 21-08-PLAN.md contain no hard-coded email-profile path or type counts, so they needed no change.
- **Commit:** 0b684f8

**2. [Checkpoint - type count] 56 types (39 named, 17 singleton) instead of 87 (49 named)**
- **Found during:** Task 2
- **Issue:** RESEARCH measured 87 types (49 named). The extract yields 56 (39 named, 17 singleton) because additional verb path segments are excluded as non-types: datafile, delete, rename, type, copy, move. The plan said to keep the script's truthful output and record the reason; the user accepted it.
- **Fix:** RESEARCH lines for the C4 census, the DRND-06 row and OQ-G now read 56/39/17, with a note citing the 21-01 extract. The 21-01-PLAN line was annotated.
- **Commit:** 0b684f8

### Auto-fixed Issues

**3. [Rule 1 - Lint] clippy `collapsible_if` in Task 1 guard code**
- **Found during:** Task 2 verification (`cargo clippy -D warnings`)
- **Issue:** Nested `if` blocks in `deploy_fixtures.rs` introduced in Task 1 tripped `clippy::collapsible_if`.
- **Fix:** Collapsed them and ran rustfmt; the fix ships in the Task 2 commit alongside `openapi_extract_shape`.
- **Files modified:** crates/ignition-core/tests/deploy_fixtures.rs
- **Commit:** 174d35c

## Skipped acceptance checks

Two acceptance checks need the external source directory and were skipped after a permission denial; the executor was told not to touch that path:

- Determinism re-run: re-running `sanitize.py` and diffing the output against the committed corpus.
- `cmp` of an untouched (S6) corpus file against its source.

The `.planning/WINDOWS.md` ledger append was attempted but failed on a pre-existing invalid entry (`kind: "process"`, entry 3), so these are recorded here only. The committed guard still proves the output properties (scanners, inventory, CR-free text, -text attributes), and the user ran the sanitizer and reported the counts above.

## Verification

- `cargo test -p ignition-core --test deploy_fixtures`: 9 passed, 0 failed.
- `cargo clippy -p ignition-core --all-targets -- -D warnings`: clean.
- `cargo fmt --all -- --check`: clean.

## Platform note

Verified locally on macOS only. Linux and Windows byte-identity of the corpus is exercised by the existing CI matrix (`cargo test --workspace` on ubuntu, macos, windows legs); `.gitattributes` `-text` prevents CRLF conversion on the Windows leg.

## Known Stubs

None. The jars are intentional stand-ins (D-01 fixture), not stubs blocking any goal.

## Threat Flags

None. No new network endpoints or trust boundaries; the work reduces exposure by keeping real secrets out of a public repo.

## Issues Encountered

None beyond the deviations above. Pre-existing uncommitted `.planning/config.json` and `.planning/research/.cache/` were intentionally left unstaged.

## Next Phase Readiness

Plans 21-05 (corpus gate), 21-06 (deny table), 21-07 (TYPE_RULES) and 21-08 (SECRET_FIELDS) can `include_str!` the extract and read the corpus path. They should use 56/39/17 and `settings.settings.password` for email-profile.

## Self-Check: PASSED
