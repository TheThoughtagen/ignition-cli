---
phase: 22-transport-plan-pull-verify
reviewed: 2026-10-16T00:00:00Z
depth: standard
scope: gap closure plans 22-15 and 22-16 (git diff 395f8ee..HEAD)
files_reviewed: 12
files_reviewed_list:
  - README.md
  - crates/ignition-cli/tests/contract_deploy.rs
  - crates/ignition-cli/tests/deploy_family_coverage.rs
  - crates/ignition-core/src/deploy/ledger.rs
  - crates/ignition-core/src/deploy/transport/local.rs
  - crates/ignition-core/src/deploy/transport/remote.rs
  - crates/ignition-core/src/deploy/transport/winps.rs
  - crates/ignition-core/src/deploy/tree.rs
  - crates/ignition-core/src/deploy/verify.rs
  - crates/ignition-core/tests/deploy_transport.rs
  - crates/ignition-core/tests/deploy_verify.rs
  - crates/ignition-core/tests/live_deploy_plan.rs
findings:
  critical: 0
  warning: 1
  info: 3
  total: 4
status: issues_found
---

# Phase 22 gap closure: Code Review Report

**Depth:** standard. **Files reviewed:** 12 (changed hunks plus direct interactions). The rest of phase 22 is covered by 22-REVIEW.md.

## Summary

Product-code changes are correct. No blocker.

**verify.rs, symmetric N1.** One-sided nulls still fail. `diff_exact` and `diff_subset` traced for file-null vs REST-value, REST-null vs file-value and REST-null vs file-absent: a value on one side and a null on the other leaves the key on one side only, which is reported. `diff_attributes` keeps the key lookup on the raw REST map, so a REST `null` for a declared key is compared and differs. New tests pin all three cases (cobranding both ways, N2, N3) and assert secret values never appear in the serialized report.

**winps.rs, ENUMERATE rewrite.** The relative path is built from `$f.Name` joined with `'\'`, a valid single-quoted PowerShell literal. The root arrives base64-wrapped through `$p` and entry names are data only, so there is no injection surface. `full_len` stays `FullName.Length`, consistent with `sftp.rs:889`; a `\\?\` root is not normalised by .NET. Reparse points are reported and not descended. The `$q=$s.Pop()` / `$q[0]` / `$q[1]` pattern uses only PowerShell 5.1 constructs. `cargo test -p ignition-core --lib winps_enumerate` passes under pwsh 7, and the new test discriminates on macOS because `.`, doubled-separator and trailing-separator spellings each shifted the old `Substring($l)`.

**remote.rs, `render_on`.** The `cfg!(windows)` conversion already existed and is now parameterised. The converted text is both what `safe_local_path` checks and what is placed in the line. On a Windows runner `\` becomes `/` and the full refusal set still runs: `\\?\C:\x` becomes `//?/C:/x`, refused for `?`. Quote, glob, control-character and leading-dash inputs are refused on both settings. Remote paths are never rewritten, and `Rm { "/r\\x" }` is refused on both.

**Test-only changes.** None weaken product assertions. The `lines()` change in `deploy_family_coverage.rs` is correct and slightly stricter. The `rename` change in `tree.rs` removes a real inode-reuse flake.

## Warnings

### WR-01: `find_cobranding` picks the first match in directory-iteration order

**File:** `crates/ignition-core/tests/live_deploy_plan.rs:508-524` (used at ~560 and ~616)

The helper returns the first `ignition/cobranding/config.json` from recursive `read_dir`, whose order is unspecified. Its doc says the file may be in core `resources/` or the mode collection. If a pull writes both, step (d) may edit the non-effective copy and fail to produce the expected `resource_differs`, making the live gate flaky for reasons unrelated to G-22-2. The test is `#[ignore]` and live-only, so CI will not show it.

**Fix:** collect all matches and assert exactly one, or prefer the mode-collection copy explicitly.

## Info

### IN-01: Doc and test comments disagree about which 8.3.6 types echo null members

**Files:** `crates/ignition-core/src/deploy/verify.rs:29-35`, `crates/ignition-core/tests/deploy_verify.rs` (~1203-1205, `rest_config_raw` doc), `crates/ignition-cli/tests/contract_deploy.rs` (~1386, `rest_config_raw` doc)

The `verify.rs` module doc says `database-translator` omits null members from REST. The test helper docs cite the tracer translator's auto-increment members as an example of a type that echoes nulls. The README names only `ignition/cobranding`. **Fix:** settle against live evidence and make the three agree, e.g. "which types echo nulls varies by type; cobranding is the confirmed one".

### IN-02: `reported_spellings_match_the_windows_runner_observation` tests only its own helpers

**File:** `crates/ignition-core/tests/deploy_transport.rs:~118-140`

It maps literal strings to literal strings and never exercises the product. Useful as a record of the CI observation, no regression protection; the `ssh -G` tests provide that. **Fix (optional):** reword the doc as a helper spec, or drop it.

### IN-03: `verify_verdict_exception_is_dated_and_documented` fails opaquely on a trailing space after `)`

**File:** `crates/ignition-cli/tests/deploy_family_coverage.rs:189-195`

`strip_suffix(')')` on the first line panics with "the heading closes with `)`" if the heading has trailing whitespace. **Fix (optional):** `line.trim_end().strip_suffix(')')`.

---
_Reviewer: Claude (gsd-code-reviewer), depth: standard_
