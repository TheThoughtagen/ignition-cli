---
phase: 25-guarded-project-deploy
plan: 21
subsystem: deploy-transport
tags: [windows, powershell, movefileex, add-type, release-record, error-cause, live-gate, gap-closure]

requires:
  - phase: 25-guarded-project-deploy
    provides: "atomic MoveFileEx record replace (25-20), review findings WR-01, WR-03, IN-01, IN-03 (25-20-REVIEW.md)"
provides:
  - "Every failure of winps::RECORD_REPLACE throws a bounded, single-line, path-free cause (IgnLine, IgnCause) that reaches the writer's transport_failed detail"
  - "Offline model of the PowerShell 5.1 error record (ps51_error_clixml) checked against records copied verbatim from 22-LIVE-CAPTURES.md"
  - "Live Add-Type cause test (passed) and live blocked-replace test (failed in its parser, G-25-6); preflight records the Add-Type compile cost"
  - "Doc fixes IN-01 and IN-03; README names the Windows record-write prerequisites (FullLanguage, csc.exe, writable %TEMP%)"
affects: [25-guarded-project-deploy verification, G-25-5, G-25-6]

actuals:
  tokens: 12100
  tasks: 4
  commits: 3

tech-stack:
  added: []
  patterns:
    - "Failure text of a fixed PowerShell script carries one bounded line of printable ASCII with drive-letter and UNC paths replaced by <path>"
    - "Offline error-record builder pinned to verbatim live captures, with its unproven part named as a model"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/deploy/transport/winps.rs
    - crates/ignition-core/src/deploy/transport/sftp.rs
    - crates/ignition-core/src/deploy/transport/projects.rs
    - crates/ignition-core/src/deploy/transport/sftp/project_ops.rs
    - crates/ignition-core/tests/live_transport_wflwvm01.rs
    - README.md
    - .planning/phases/25-guarded-project-deploy/25-LIVE-CAPTURES.md
    - .planning/phases/25-guarded-project-deploy/25-UAT.md

key-decisions:
  - "Add-Type is kept: a reflection-emit DefinePInvokeMethod declaration cannot be exercised offline and is the technique endpoint protection watches. Cost recorded: fresh-process baseline median 1.51 s, record_replace median 2.04 s, so Add-Type and the replace share 0.53 s"
  - "The plan ended on the 'new gap: fix failed' resume signal. No code was changed inside the checkpoint (plan rule); G-25-6 records the parser defect"

patterns-established:
  - "A live-test parser must be built from the real captured text, not a modelled one: the offline model had no source excerpt, the real PS 5.1 record has one"

requirements-completed: []

duration: "not timed end to end; first task commit 2026-10-27T04:01Z, live run ended 04:28Z"
completed: 2026-10-27
status: gaps_found
---

# Phase 25 Plan 21: Cause of a failed Windows record replace (G-25-5) Summary

**A failed Windows release-record replace now reports why (exception type, HResult and message, or the Win32 error and its system message) in the transport_failed detail, proven live for the Add-Type path and for the blocked-replace detail; but the live blocked-replace test failed in its own parser, so the plan ended with a new gap, G-25-6, and G-25-5 stays open.**

## Outcome

This plan ended with a new gap. All four tasks ran and three of the four live tests passed, but `wflwvm01_record_replace_blocked_by_reader` failed with `fix failed: expected Win32 error 5 or 32, got None`, so WR-03 (the blocked replace proven live: temp removed, old record intact, rerun after release) is not proven by the test. The product behaved correctly; the defect is in the test's parser. The checkpoint rule applies ("new gap: fix failed"): no code was changed inside the checkpoint, and G-25-5 stays `status: failed` with WR-01 proven live and WR-03 pending G-25-6.

## Performance

- **Tasks:** 4 of 4 executed (Task 1 tracer, Tasks 2 and 3 auto, Task 4 blocking human-verify checkpoint, run by the orchestrator; outcome: new gap)
- **Commits:** 3 code and test and doc commits, plus the docs commit for this summary
- **Duration:** first task commit 04:01Z, live run ended 04:28Z; earlier session time was not recorded

## Accomplishments

- **Task 1 (9522e29, tracer):** `RECORD_REPLACE` throws a bounded, single-line, path-free cause for the language mode, a failed Add-Type and a failed MoveFileEx (new helpers `IgnLine`, `IgnCause`). The MoveFileEx call, the flags, the type-exists guard, the retry on Win32 errors 32 and 5, and the fail-closed behaviour are unchanged. `ps51_error_clixml` reproduces PowerShell 5.1 error records pinned to lines copied verbatim from 22-LIVE-CAPTURES.md, and an offline test proves the cause survives the 5-line stderr tail into the writer's detail. The `RECORD_REPLACE` doc records the Add-Type evaluation and the NTFS guarantee.
- **Task 2 (9902370):** the live tests `wflwvm01_record_replace_names_the_add_type_cause` and `wflwvm01_record_replace_blocked_by_reader`, a preflight extended with three fresh-process baseline and three `record_replace` samples, offline `win32_code_in` and command-budget tests, and the pending capture section. The 25-20 race gate was not edited.
- **Task 3 (b8ea775):** the `ReleaseRecordWriter` doc is two sentences again (IN-01); the README, the `RECORD_REPLACE` doc and the `project_ops.rs` module doc state that the guarantee is the atomic rename of NTFS and of rename(2), not a durable write, that error 5 is retried as well as 32, and that a still-blocked replace ends in `transport_failed` with the previous record kept (IN-03). The README names FullLanguage, `csc.exe` and a writable `%TEMP%` as Windows record-write prerequisites (WR-01).
- **Task 4 (live, orchestrator, 2026-10-27T04:13Z to 04:28Z on ign-win-836):** see below.

## Live results (ign-win-836, IP 52.154.75.231)

The scanned ed25519 key equalled the pinned key before the session pin was written. After the run the session pin was removed and the VM was deallocated (`PowerState/deallocated`). Full numbers and verbatim CAPTURE lines are in 25-LIVE-CAPTURES.md under `### Windows record replace failure causes and blocked replace (G-25-5)`.

| Test | Result |
| --- | --- |
| a. `wflwvm01_record_replace_preflight` | PASS (41.56 s). FullLanguage on PowerShell 5.1.20348.5756. Baseline median 1.51 s, `record_replace` median 2.04 s, so Add-Type and the replace share 0.53 s |
| b. `wflwvm01_record_replace_race` (the unchanged 25-20 gate) | PASS (681.06 s). Calibration projected 96, so the fallback constants ran (300 s, 30 ms). Negative control missing 2 + 3 + 6 = 11. Shipped run: torn 0, missing 0, backwards 0; replace_ok 320, replace_failures 29; local reads 9387 + 9394; sftp reads 487; distinct 317, 317, 132. Verdicts: control Pass, shipped Pass |
| c. `wflwvm01_record_replace_names_the_add_type_cause` | PASS (21.09 s). The detail carries `System.IO.IOException 0x80131620: Access to the path '<path>' is denied (caf?)`, cut with `...`; target and temporary file untouched |
| d. `wflwvm01_record_replace_blocked_by_reader` | FAILED with `fix failed` at `live_transport_wflwvm01.rs:5856`: `expected Win32 error 5 or 32, got None`. The write took 16.44 s. The real detail ends `+ FullyQualifiedErrorId : the record replace failed with Win32 error 5: Access is denied`, so the product's cause text is correct |

On test d: the panic came before the test checked the temporary file and the old record, before its own cleanup and before the rerun-after-release step, which was not reached. The orchestrator checked the host by hand: `live-scratch\projects\CAP25-BLOCKED` held only `release.json` (262 bytes) with `run_id: G255-OLD`, so the old record is intact and there is no temporary file (a listing and a read of `run_id`, not a hash comparison). The orchestrator then removed `live-scratch` (`Test-Path` is False).

## Why the parser failed (G-25-6)

`win32_code_in` squashes whitespace and takes the digits after the first occurrence of `therecordreplacefailedwithWin32error`. The real PowerShell 5.1 error record repeats the failing source line before the `FullyQualifiedErrorId` line, so the first occurrence is the throw expression `throw ('the record replace failed with Win32 error '+$c+' ...`, followed by a quote, not a digit. The offline test `win32_code_in_reads_a_wrapped_detail` passed against a modelled detail with no source excerpt. This is exactly the model-versus-real difference the plan's Task 1 guarded against for the cause text (the builder checked against verbatim captures); the same discipline was not applied to the parser's input. Fix and rerun are recorded as the `missing` items of G-25-6 in 25-UAT.md.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] 300-character boundary case**
- **Found during:** Task 1
- **Issue:** the cause-survival test covered 60, 150 and 299 characters; the 5-line stderr tail boundary at exactly 300 had no case.
- **Fix:** added the 300-character boundary case next to the 299 and 150-with-special-characters cases (299 / 150-special / 300) in the winps.rs cause-survival test.
- **Files modified:** crates/ignition-core/src/deploy/transport/winps.rs
- **Commit:** 9522e29

**2. [Rule 2 - Missing critical] Extra test `cause_body_ends_with_the_shipped_text_and_fits_the_budget`**
- **Found during:** Task 2
- **Issue:** the Add-Type cause test runs a shadow function followed by the shipped text; nothing proved the body really ends with the exact `RECORD_REPLACE` text or that the encoded command fits the command budget.
- **Fix:** added the offline test `cause_body_ends_with_the_shipped_text_and_fits_the_budget`.
- **Files modified:** crates/ignition-core/tests/live_transport_wflwvm01.rs
- **Commit:** 9902370

**3. [Plan acceptance criterion off by one] `grep -c 'fn ps51_error_clixml'` prints 2, not 1**
- **Found during:** Task 1 verification
- **Issue:** the plan expected 1. The count is 2 because the test `fn ps51_error_clixml_matches_the_captured_records` also matches the pattern; there is one builder definition (winps.rs line 688).
- **Fix:** none needed; the criterion's intent (one builder, no duplicate) holds.

No other deviations in the executor's work. Task 4 ended with the failed-fix signal, which is the plan's defined path, not a deviation.

## Auth gates

None.

## Known Stubs

None. The `pending` cells the plan created in 25-LIVE-CAPTURES.md were all filled; the blocked-replace listing/sha256 and rerun-after-release rows record what was and was not reached.

## Threat Flags

None. No new network endpoint, auth path or schema; the new failure text is path-free and bounded by design.

## Deferred Issues

- **G-25-6** (25-UAT.md): fix `win32_code_in` against a fixture copied from the real captured detail, then rerun `wflwvm01_record_replace_blocked_by_reader` live on ign-win-836. Until then WR-03's temp-removed, old-record-intact (checked by hand once, not by the test) and rerun-after-release claims are unproven by the test.

## Self-Check: PASSED

The claims in this summary were checked: the modified files exist, commits 9522e29, 9902370 and b8ea775 exist, the G-25-5 capture section holds no `pending` (`grep -ci pending` prints 0), and the session pin `~/.cache/ign-wflwvm01/known_hosts.session` is gone. This check covers the accuracy of the record only. It does not mean the plan's Task 4 acceptance criteria were met: the blocked-replace live test failed, so the plan ended with G-25-6.
