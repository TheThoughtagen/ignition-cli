---
phase: 25-guarded-project-deploy
plan: 20
reviewed: 2026-10-26T00:00:00Z
depth: standard
files_reviewed: 6
files_reviewed_list:
  - crates/ignition-core/src/deploy/transport/winps.rs
  - crates/ignition-core/src/deploy/transport/sftp/project_ops.rs
  - crates/ignition-core/src/deploy/transport/sftp.rs
  - crates/ignition-core/src/deploy/transport/projects.rs
  - crates/ignition-core/tests/live_transport_wflwvm01.rs
  - README.md
findings:
  critical: 0
  warning: 4
  info: 4
  total: 8
status: issues_found
---

# Phase 25-20: Code Review Report (G-25-4, atomic Windows record replace)

**Reviewed:** 2026-10-26
**Depth:** standard
**Scope:** `git diff 8eaa99e 4d57b9f` on the six listed files
**Status:** issues_found

## Summary

The shipped change is sound. I traced these and found no BLOCKER in any of them:

- **Script text.** `RECORD_REPLACE` parses and flows correctly: the FullLanguage guard, the type-exists guard, then `try{Add-Type}catch{throw}`, `$f=[uint32](0x1 -bor 0x8)` and the retry loop. The `-or` precedence is correct. The `bool`/`uint` P/Invoke signature is right for `MoveFileExW` with `SetLastError=true`. `retries` is read as `[int]$p.retries`, and a missing value degrades to 0 retries rather than an error. `MoveFileEx` without `COPY_ALLOWED` keeps it same-volume and atomic, and it also works when the destination does not exist yet.
- **Injection.** `src`, `dst` and `retries` travel only in the base64 JSON blob. The script text is a fixed const, and the unit test pins that the text outside the blob does not depend on the count or the paths. The temp suffix is the run id, which was already validated.
- **Fail-closed behaviour.** Add-Type failure, constrained language, or an exhausted retry all become a thrown error, then `transport_failed` through `interpret_output`. No code path falls back to a delete-then-rename.
- **Windows `replace_record` branch.** It is unchanged in order. A failed replace goes through `remove_temp_after`, so the temp is removed and the old record is left in place, as the stub test shows.
- **Race harness.** The harness arithmetic is correct: the head is 19 bytes, the tail is 16, and `race_payload` agrees with the writer padding `size-35`. The HResult classification (`-band 65535`) is correct. The verdict logic cannot make the shipped run pass without 0 torn, 0 missing and 0 backwards plus the count floors. A broken setup would make the shipped run fail, not pass.

I did not run any tests. The cargo invocation was refused by the environment. This is a static review.

The findings below are robustness and coverage gaps, not wrong behaviour on the proven path.

## Warnings

### WR-01: `Add-Type` failure discards its cause, and the replace now depends on `csc.exe` plus a writable TEMP on every record write

**File:** `crates/ignition-core/src/deploy/transport/winps.rs:106` (the `RECORD_REPLACE` const)
**Issue:** `catch{throw 'the record replace cannot declare MoveFileEx'}` replaces the real exception with a fixed sentence. The operator cannot tell a missing compiler from an unwritable TEMP, an AppLocker or AV block, or a language-mode restriction. This path is likely to be hit in hardened Ignition hosts. `Add-Type -MemberDefinition` shells out to `csc.exe` and writes `.cs` and `.dll` files into the deploy account's `%TEMP%`. A session with no loaded profile, where TEMP is `C:\Windows\Temp`, will fail every time.

Failing closed is the intended behaviour. But the result is that, on such a host, every verified import ends `record_written:false` and the release record, which D-03/D-06/D-12 read, can never be written. Production also pays a compile in every fresh process, because the plan's guard only helps the in-process harness. The preflight only proves it on the capture host.

**Fix:** At minimum keep the cause in the thrown text, for example `catch{throw ('the record replace cannot declare MoveFileEx: '+$_.Exception.Message)}`. Cap its length and keep it ASCII, because stderr is scrubbed. Preferably avoid the compiler by declaring the P/Invoke with `[AppDomain]::CurrentDomain.DefineDynamicAssembly(...).DefineDynamicModule(...).DefinePInvokeMethod(...)`. That needs no csc and no TEMP file, but it must be re-verified under the same FullLanguage guard. If Add-Type is kept, document the csc and TEMP prerequisite in the README beside the "Windows ssh node needs FullLanguage" requirement.

### WR-02: The live gate has no `busy` ceiling, and the read floors count failed reads, so a reader that mostly fails can still reach its floor

**File:** `crates/ignition-core/tests/live_transport_wflwvm01.rs:4879` (`race_verdict`), `:4994` (`RACE_READER` catch), `:5062-5073` (`sftp_reader`)
**Issue:** Three things combine to weaken the proof.

1. In `RACE_READER`, the `catch` wraps the read and also the classification code. Anything that throws other than HResult 2 or 3, including a harness bug inside the `Substring` logic, counts as `busy`, and so does a genuine access violation.
2. The sftp reader counts every stopped batch as one `read` (busy or missing) and classifies "missing" only by `stderr` containing "no such file" or "not found". Any other wording for a vanished file lands in `busy`, which `race_verdict` ignores for the shipped run.
3. The floors `FLOOR_LOCAL_READS` and `FLOOR_SFTP_READS` are on `reads`, not on `good`. The distinct-version floors (100 and 20) make a total vacuous pass unlikely. But nothing bounds the fraction of reads that were busy, and a missing read on the sftp side could be reported as busy and so be invisible to the 0-missing assertion.

**Fix:** Require `good >= floor` for the read floors instead of `reads`. Add a shipped-run ceiling such as `busy <= reads / 4`, or record `busy` and fail on a ratio. Split the reader catch so that only IO exceptions are classified, and count anything else as a separate `err` bucket that fails the run. Print the first distinct `stderr` text of a stopped sftp batch in the `CAPTURE` output so a misclassified missing read is visible.

### WR-03: The blocked-replace path (retries exhausted, temp cleaned, old record kept) is never exercised with the real script

**File:** `crates/ignition-core/src/deploy/transport/sftp.rs:5545` (the failing `WinMoveRunner` case) and `crates/ignition-core/tests/live_transport_wflwvm01.rs:5290` (preflight)
**Issue:** The plan's must-have is that a replace blocked by a reader that holds the target without delete-share retries, then returns `transport_failed`, leaves the previous record, and removes the temp. The offline test covers this only with a canned CLIXML stub that fails on the first call and never runs the script. The live preflight covers only the unblocked success. The 32/5 retry branch, the `throw` after the retries, and the "old record intact" claim on a real blocked replace are covered by nothing. In the race gate a failed attempt is deliberately swallowed and only counted in `replace_failures`.

**Fix:** Add a live test, `#[ignore]` with the same quiet skip. It opens `release.json` with `FileShare.Read` for about `RENAME_RETRIES + 2` seconds in one exec, and runs `WinScript::record_replace` in another. Assert the replace errors, the temp file is still there for the caller to remove, and `release.json` still holds the old bytes. Then release the handle and assert that a rerun succeeds. A cheaper offline gain is to make the stub runner honour `retries` and fail with a sharing error N times.

### WR-04: The calibration gate checks fewer invariants than the shipped verdict

**File:** `crates/ignition-core/tests/live_transport_wflwvm01.rs:5340-5352`
**Issue:** The calibration run asserts only `torn + missing == 0`. It ignores `backwards`, which the shipped verdict treats as a failure. It does not check that `replace_ok` is above 0. If the guard throws on every iteration (constrained language, or Add-Type blocked in the harness), calibration passes, then `projected = 0 < PROJECTED_MIN` silently selects the 300 s fallback. Two more runs of up to 300 s each then end in `gate throughput:`. That reads as "gate too thin", not as "the shipped script cannot run", even though the preflight asserted FullLanguage only for the probe, not for the in-process harness.

**Fix:** Reuse `race_verdict(&counts, false)` for calibration. Treat `Failed` as `fix failed:`. If `replace_ok == 0 && replace_failures > 0`, panic immediately with a distinct `harness cannot run the shipped script` message that includes the failure count, before spending the long runs.

## Info

### IN-01: `ReleaseRecordWriter` doc sentence is now ungrammatical

**File:** `crates/ignition-core/src/deploy/transport/projects.rs:121-128`
**Issue:** The edit turned the parenthetical into a main clause: "...renamed over the record in one atomic step (Linux `mv -f`, ...): a reader sees the old record or the new one, never a missing or half-written one, and nothing else: no `.git`...". "and nothing else" now attaches to the reader sentence instead of to "writes exactly one file".
**Fix:** Restore two sentences: "An implementation writes exactly one file ... and nothing else (no `.git`, no project file, no other record). It lands as a private temporary sibling renamed over the record in one atomic step (...), so a reader sees the old record or the new one, never a missing or half-written one."

### IN-02: Stale `Move-Item` names left in the Windows mock test

**File:** `crates/ignition-core/src/deploy/transport/sftp.rs:5385-5420`, `:5441-5450`
**Issue:** The runner type is still called `WinMoveRunner`, `win_move_transport` and the test comment says "failed move". The failure stub's CLIXML still reads `Move-Item : Access to the path is denied.`. The code no longer uses Move-Item, so a failure message from the real script would read `the record replace failed with Win32 error 5`.
**Fix:** Rename to `WinReplaceRunner` and use the real thrown text in the stub, so the test pins the message the writer will surface.

### IN-03: README and doc claims are unconditional

**File:** `README.md:3374`, `README.md:3824`; `crates/ignition-core/src/deploy/transport/winps.rs:88`
**Issue:** The text says a reader sees "never a missing or half-written file". Two qualifications are missing.

- Atomic replace of the name is an NTFS guarantee. The temp's data is not flushed by the sftp `put`, and `MOVEFILE_WRITE_THROUGH` only flushes the rename.
- A reader that holds the target without delete-share makes the replace fail after the retries. Then `record_written:false` is the outcome, not a replaced record.

The module doc also says "a sharing violation is retried", while the script retries error 5 (access denied) too, which includes a permanent ACL denial. That costs about 3 s before the real error is shown.
**Fix:** Qualify the sentence ("on NTFS"; "or the write fails with `transport_failed` and the previous record stays"). State that error 5 is also retried.

### IN-04: Calibration projection is biased low and can force the fallback needlessly

**File:** `crates/ignition-core/tests/live_transport_wflwvm01.rs:5330-5338`
**Issue:** The 20 s calibration includes the one-off `Add-Type` compile (seconds), the ssh start, and the skew between the writer and reader sessions. Scaling it by `RACE_SECS / CALIBRATION_SECS` therefore under-projects the 120 s throughput. A host that would clear 300 replaces at 120 s can be pushed to the 300 s / 30 ms constants. That changes the reader cadence, and so what the gate measures, without a real throughput problem.
**Fix:** Have the writer harness report the elapsed time after the first successful replace, and project from `(secs - warmup)`. Or make the fallback decision on the control and shipped verdicts only, as the rerun rule already does.

---

_Reviewed: 2026-10-26_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
