---
phase: 25-guarded-project-deploy
plan: 20
subsystem: deploy-transport
tags: [windows, powershell, movefileex, atomic-replace, release-record, concurrency, live-gate, gap-closure]

requires:
  - phase: 25-guarded-project-deploy
    provides: "release-record writer (25-13), UAT race that exposed G-25-4"
provides:
  - "Windows release-record replace is one atomic MoveFileEx(MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) call (winps::RECORD_REPLACE), fail closed outside FullLanguage or when Add-Type fails, bounded retry on sharing/access violations"
  - "Live preflight, MOVE_REPLACE negative control and shipped-code concurrent-reader race gate with floors, calibration and a single fixed fallback"
  - "Corrected docs for the per-family record-replace mechanism"
affects: [25-guarded-project-deploy verification, any later reader-side change to read_release_record]

actuals:
  tokens: 11600
  tasks: 3
  commits: 2

tech-stack:
  added: []
  patterns:
    - "Fixed-text PowerShell script with all parameters (src, dst, retries) in the base64 JSON blob"
    - "Live race gate that cannot pass vacuously: negative control must show missing reads, shipped run must meet count floors"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/deploy/transport/winps.rs
    - crates/ignition-core/src/deploy/transport/sftp/project_ops.rs
    - crates/ignition-core/src/deploy/transport/sftp.rs
    - crates/ignition-core/src/deploy/transport/projects.rs
    - crates/ignition-core/tests/live_transport_wflwvm01.rs
    - README.md
    - .planning/phases/25-guarded-project-deploy/25-LIVE-CAPTURES.md

key-decisions:
  - "Mechanism fixed as MoveFileEx(REPLACE_EXISTING | WRITE_THROUGH) via Add-Type P/Invoke; the live gate proved it rather than choosing it"
  - "No reader-side retry added; the atomic rename removes the missing window at the source"
  - "MOVE_REPLACE and WinScript::move_replace retained for the generic exec test and the apply mirror move; only the record path moved"

patterns-established:
  - "Fail closed: a constrained language mode or a failing Add-Type throws and the writer returns transport_failed, never a non-atomic fallback"

requirements-completed: [DPRJ-03]

duration: ~2h30m wall (includes the operator-side live run)
completed: 2026-10-27
status: complete
---

# Phase 25 Plan 20: Atomic Windows record replace (G-25-4) Summary

**Windows release-record replace is now a single MoveFileEx(REPLACE_EXISTING | WRITE_THROUGH) call that fails closed, and a live concurrent-reader race on ign-win-836 shows 0 torn, 0 missing, 0 backwards reads for the shipped script while the MOVE_REPLACE negative control showed 5 missing reads.**

## Performance

- **Duration:** about 2h30m wall time including the operator-side live run (race test itself 685.98 s)
- **Tasks:** 3 of 3 (2 auto, 1 blocking human-verify checkpoint, approved)
- **Commits:** 2 code commits plus the docs commit for this summary

## Accomplishments

- **Task 1 (1f69b62):** added `RECORD_REPLACE` and `WinScript::record_replace(src, dst, retries)` in `winps.rs`. The script asserts FullLanguage, compiles the `IgnRecord.Native` P/Invoke inside a try/catch that rethrows (guarded by a type-exists check so the in-process harness compiles once), calls `MoveFileEx($p.src, $p.dst, 0x9)`, and retries sharing/access violations up to `$p.retries` times with one-second pauses before throwing. `replace_record`'s Windows arm now calls it with `RENAME_RETRIES`. The POSIX `mv -f --` arm and `commit.ps1` are unchanged. Unit and mock tests cover the fixed text, the guard order, no `Move-Item` token, the command budget with long and accented paths, and that the retry count travels only in the blob.
- **Task 2 (4d57b9f):** added the offline `race_payload` / `race_version` / `race_verdict` helpers and tests, and the two ignored live tests `wflwvm01_record_replace_preflight` and `wflwvm01_record_replace_race` (20 s calibration, standard constants, one fixed fallback of 300 s and 30 ms, `MOVE_REPLACE` negative control, floors). Corrected the Windows claim in the `project_ops.rs` module doc, the `ReleaseRecordWriter` doc, two README sites, and the 25-LIVE-CAPTURES open-point and 25-13 notes.
- **Task 3 (live, run by the orchestrator/operator, approved):** preflight and race gate both passed on ign-win-836. No code commit.

## Live results (ign-win-836, 2026-10-27)

| Check | Result |
| --- | --- |
| Preflight | ok, 27.26 s; PowerShell 5.1.20348.5756, FullLanguage, fresh-process `record_replace` 2.70 s, answer `{"ok":1}`; scratch and staging cleaned |
| Calibration (RECORD_REPLACE, 20 s, 10 ms) | 19 attempts, replace_ok 15, replace_failures 4; projected 90 in 120 s, below the 450 threshold, so the gate went straight to the fallback constants (300 s, 30 ms). No ThroughputOnly rerun |
| Negative control (MOVE_REPLACE, 300 s, 30 ms) | replace_ok 2600, replace_failures 4415; missing 3 + 1 + 1 (sftp), torn 0: control is sensitive, verdict Pass |
| Shipped (RECORD_REPLACE, 300 s, 30 ms) | replace_ok 318 (floor 300), replace_failures 34; local reads 9378 + 9376 = 18754 (floor 10,000); sftp reads 527 (floor 300); distinct versions 317 / 314 / 144 (floors 100 / 100 / 20); torn 0, missing 0, backwards 0 on every reader; verdict Pass |
| Race test | `1 passed`, 685.98 s, window 02:37:35Z to 02:48:52Z; scratch cleaned on the host |

All CAPTURE lines are recorded verbatim in `25-LIVE-CAPTURES.md` under `### Windows atomic record replace (G-25-4)`.

## Deviations from Plan

None in code: the plan was executed as written.

### Observations (not failures)

- **Calibration throughput far below the plan's expectation.** About 0.75 replaces/s (15 in 20 s, projected 90 in 120 s) against the expected 500 to 3,000 per 120 s. The plan's own rule sent the gate to the fallback constants, and the shipped run then cleared the replace_ok floor of 300 by only 18 (318). Every correctness count was 0 and every floor was met, so the gate passed legitimately, but the margin on replace_ok is thin. The plan's throughput model (about 25 ms per unblocked iteration) was optimistic for this host: roughly 10% of shipped attempts (34 of 352) exhausted the bounded retry while readers held the file. A future re-run could miss the replace_ok floor on throughput alone; the plan classifies that as `ThroughputOnly`, not a failed fix.
- **Control writer behaviour.** `Move-Item -Force` failed 4415 of 7015 attempts under the same readers (it does not wait), yet still showed 5 missing reads, confirming the original G-25-4 window.

## Issues for the operator

- **VM public IP changed on restart.** ign-win-836 now answers at 52.154.75.231, but `~/.config/ign-azure-win/env` still says 40.114.71.134 (`IGN_WINVM_HOST`). The live run overrode `IGN_WFLWVM01_HOST` for the session only. The config file was not edited (out of scope and outside the repo); update it, or use a static IP/DNS label, before the next capture run. The VM is deallocated and was not touched by this closing pass.

## Known Stubs

None.

## Threat Flags

None. The plan's threat register (T-25-40 atomic replace, T-25-46 fail-closed guard, T-25-42 fixed-text script with blob parameters) is covered by the implementation and the live result.

## Self-Check: PASSED

- Commits 1f69b62 and 4d57b9f exist in `git log`.
- `25-LIVE-CAPTURES.md` G-25-4 section contains no `pending` row.
- `.planning/config.json`, `.planning/research/.cache/` and `svc-ignition-deploy@52.154.75ace-uat4.ps1` were not staged.
