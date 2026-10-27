---
phase: quick-261026-vc6
plan: 01
subsystem: testing
tags: [live-test, powershell, win32, gap-closure, G-25-6]
requires:
  - phase: 25-guarded-project-deploy
    provides: "25-21 live capture of the blocked record replace (G-25-5), which exposed G-25-6"
provides:
  - "win32_code_in reads the Win32 code from the real PS 5.1 error record"
  - "win32_cause_named, the shared code+message check used by the live blocked-replace test"
  - "CAPTURED_BLOCKED_DETAIL, a byte-verified offline fixture of the real record"
affects: [phase-25-reverify, G-25-5, G-25-6]
tech-stack:
  added: []
  patterns: ["offline fixture copied verbatim from a live capture, byte-checked with cmp"]
key-files:
  created: []
  modified:
    - crates/ignition-core/tests/live_transport_wflwvm01.rs
key-decisions:
  - "Parse the last marker followed by digits, not the FullyQualifiedErrorId label line (the label line is subject to the 119-column wrap and the existing modelled details have no label)"
  - "Message check no longer requires a trailing full stop: the real .NET message is `Access is denied`"
metrics:
  duration: ~20min
  completed: 2026-10-26
status: complete
actuals:
  tokens: 4500
  tasks: 2
  commits: 3
---

# Phase quick-261026-vc6 Plan 01: Close gap G-25-6 Summary

Test-only fix: `win32_code_in` now reads Win32 error 5 from the verbatim PowerShell 5.1 blocked-replace record, and the live test's cause check goes through a helper that accepts the real full-stop-free message.

## What was done

**Task 1 (RED then GREEN).** Added `WIN32_MARKER` and the const `CAPTURED_BLOCKED_DETAIL`, a verbatim 464-byte copy of the `CAPTURE blocked: detail:` block from 25-LIVE-CAPTURES.md, plus `win32_code_in_reads_the_captured_ps51_record`. The test asserts the premise (the first marker in the squashed record is followed by `'`, the repeated source line), `Some(5)` on the full record, `None` on the excerpt-only record, and `Some(32)` on a modelled wrapped code-32 variant. RED run failed on the `Some(5)` assertion with `left: None`. `win32_code_in` now walks `rmatch_indices(WIN32_MARKER)` and returns the first match from the end whose following digits parse as `u32`.

**Task 2.** Added `win32_cause_named(detail) -> Result<u32, String>` (code 5 needs `Accessisdenied`, code 32 needs `beingusedbyanotherprocess`, no full stop). `wflwvm01_record_replace_blocked_by_reader` keeps its `CAPTURE blocked: Win32 error {code:?}` line and now calls `win32_cause_named(&detail)` once, panicking with `fix failed: {why}: {detail}`. Added offline test `win32_code_in_feeds_the_blocked_cause_check` (Ok(5) on the real record, Ok(32) on the variant, Err `error 5 without its message`, Err `expected Win32 error 5 or 32, got Some(2)`, Err `got None` on the excerpt).

## Gate output

- Task 1 verify: `win32_code_in_reads_a_wrapped_detail ... ok`, `win32_code_in_reads_the_captured_ps51_record ... ok`; perl+cmp gate printed `VERBATIM-OK` (capture is 464 bytes).
- `cargo test -p ignition-core --test live_transport_wflwvm01 win32_code_in`: 3 passed, 0 failed, 30 filtered out.
- `cargo fmt --all --check`: exit 0.
- Homebrew `cargo clippy --workspace --all-targets -- -D warnings` (cargo 1.94.1 Homebrew): exit 0.
- rustup stable `cargo clippy --workspace --all-targets --keep-going -- -D warnings` (cargo 1.99.1, `CARGO_TARGET_DIR` in the scratchpad `target-1.99`): exit 0.
- `grep -c 'win32_cause_named(&detail)'`: 1. `--list --ignored` shows `wflwvm01_record_replace_blocked_by_reader: test` once.
- `git diff --name-only HEAD~3..HEAD`: only `crates/ignition-core/tests/live_transport_wflwvm01.rs`. No deletions.

## Commits

- 2300aac `test(quick-261026-vc6): failing fixture from the real PS 5.1 blocked-replace record` (RED)
- f56405a `fix(quick-261026-vc6): read the Win32 code from the real PS 5.1 error record (G-25-6)` (GREEN)
- 1510def `fix(quick-261026-vc6): blocked-replace live check accepts the real Win32 message text (G-25-6)`

## Deviations from Plan

None - plan executed exactly as written.

## Second finding (full-stop message check)

The planner found that, once the parser was fixed, the live test would fail next: it required `Accessisdenied.` / `beingusedbyanotherprocess.` with a trailing full stop, but the real record ends `Win32 error 5: Access is denied` with no full stop. Fixed in Task 2 via `win32_cause_named`, which the offline fixture test also runs, so the offline green proves the live check accepts the real record.

## Pending

The orchestrator still has to rerun `wflwvm01_record_replace_blocked_by_reader` live on ign-win-836 and record the result against G-25-6 / G-25-5 in 25-UAT.md and 25-LIVE-CAPTURES.md. No `--ignored` test and no pwsh was run here. The uncommitted `.planning/config.json` and the untracked `.planning/research/.cache/` and `svc-ignition-deploy@52.154.75ace-uat4.ps1` were left as found.

## Known Stubs

None.

## Threat Flags

None.

## Self-Check: PASSED

- crates/ignition-core/tests/live_transport_wflwvm01.rs contains `const CAPTURED_BLOCKED_DETAIL: &str` (FOUND)
- Commits 2300aac, f56405a, 1510def exist on main (FOUND)
