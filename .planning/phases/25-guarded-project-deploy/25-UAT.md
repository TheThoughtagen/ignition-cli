---
status: partial
phase: 25-guarded-project-deploy
source: [25-VERIFICATION.md]
started: 2026-10-24T05:00:00Z
updated: 2026-10-27T04:50:00Z
---

## Current Test

[testing complete]

## Tests

### 1. First real run of .github/workflows/deploy-projects.yml on a tag (for example v0.0.1-test) with a dev environment, from a project repo caller
expected: Sparse cross-repo targets checkout works with IGN_TARGETS_TOKEN and the token is masked in the log; IGN_MASTER_TOKEN / IGN_SSH_KEY reach the apply job only through its own environment (the secret split); check's artifact_sha256 and targets_sha reach apply through needs and workflow_call outputs; apply waits for approval and deploys exactly the checked hash
result: blocked
blocked_by: server
reason: "needs a pushed branch, a WHK caller repo and a dev environment (LDKRVM02 candidate)"

### 2. Concurrency queue of deploy-projects.yml: start two applies for the same gateway and project
expected: One running, one pending; a newer pending apply replaces the older pending one (cancel-in-progress false); check what happens to a prod job that needs the replaced dev job
result: blocked
blocked_by: server
reason: "needs the same CI caller as test 1"

### 3. Run the workflow on a self-hosted Windows runner (Git Bash, Win32-OpenSSH, cygpath)
expected: Install, config, targets and run steps behave as on Linux
result: blocked
blocked_by: other
reason: "needs a self-hosted Windows runner"

### 4. Windows ssh node: release-record replace with a concurrent reader (Move-Item -Force)
expected: A reader sees the old or the new record, never a torn file
result: issue
reported: "issue" (live race on ign-win-836, 2026-10-26: 0 torn reads, but readers saw the record missing 175/55 times locally and once over sftp; Move-Item -Force on PS 5.1 deletes then moves; 92% of replaces failed while a reader held the file; File.Replace also leaves a missing window)
severity: major
retest: pass
retested: "2026-10-27 after 25-20 (MoveFileEx REPLACE_EXISTING|WRITE_THROUGH): live race on ign-win-836 at 300 s / 30 ms, shipped RECORD_REPLACE torn 0, missing 0, backwards 0 on all readers, replace_ok 318; MOVE_REPLACE negative control missing 5. See 25-LIVE-CAPTURES.md ### Windows atomic record replace (G-25-4)"

### 5. WR-02 judgment: an Internal (unclassified 4xx/5xx) answer to the import request is treated as outcome unknown and read back
expected: Owner confirms that a deterministic gateway 4xx on import reporting imported_unverified (exit 6) with no second import is acceptable, instead of a plain error
result: pass

### 6. Compile and run the non-unix (portable) variant of local_fs.rs on a Windows target
expected: Builds and the record writer's temp-file cleanup (WR-04) behaves as on unix
result: pass
retested: "2026-10-27 PR #27 CI run 40446661810, check (windows-latest), rustc 1.99.1: cargo clippy -D warnings, cargo build and cargo test --workspace pass (2788 tests). The portable local_fs.rs variant compiles and the local record-writer paths pass on Windows (record_write_failure_is_warning, first_deploy_equal_records, new_release_same_content_records, write_half_unreachable, distinct_temp_dirs). Caveat: the direct temp-cleanup unit tests in local_fs.rs (a_failed_temp_write_removes_the_temp_file and siblings) are cfg(unix)-only, so the non-unix cleanup branch is exercised only through the integration tests."

## Summary

total: 6
passed: 3
issues: 0
pending: 0
skipped: 0
blocked: 3

## Gaps

- gap_id: G-25-4
  truth: "On a Windows ssh node, a reader of the release record sees the old or the new record, never a torn or missing file"
  status: resolved
  resolved_by: 25-20 (commits 1f69b62, 4d57b9f, 7ff43b8; live race passed 2026-10-27)
  reason: "User reported: issue. Live race on ign-win-836: Move-Item -Force (PowerShell 5.1) deletes the target then moves the staged file, so concurrent readers saw release.json missing (175 and 55 of ~17k local reads, 1 of 381 sftp reads). No torn reads. Replaces also fail with IOException while any reader holds the file without FileShare.Delete (6087 of 6647 attempts under a constant read loop). [IO.File]::Replace shows the same missing window (902/836)."
  severity: major
  test: 4
  root_cause: "winps.rs MOVE_REPLACE uses Move-Item -Force, which on Windows PowerShell 5.1 is delete-then-rename rather than an atomic replace. read_release_record maps a missing file to None, which deploy_projects treats as 'never deployed here' (and the children sweep as Unverified)."
  artifacts:
    - crates/ignition-core/src/deploy/transport/winps.rs (MOVE_REPLACE)
    - crates/ignition-core/src/deploy/transport/sftp/project_ops.rs (replace_record, read_release_record)
    - crates/ignition-core/src/actions/deploy_projects.rs (record None handling)
  missing:
    - "An atomic replace on Windows (for example a rename with replace-if-exists semantics via SetFileInformationByHandle/FILE_RENAME_INFO, or MoveFileEx MOVEFILE_REPLACE_EXISTING via P/Invoke), proven by the same live race"
    - "Or a reader that does not read a transiently missing record as absent (retry before None)"
    - "A defined outcome when the replace is blocked by a reader after a successful import"

- gap_id: G-25-5
  truth: "A failed Windows record replace reports why it failed, and the blocked-replace outcome (retries exhausted, temp removed, previous record kept) is proven against the real script"
  status: resolved
  resolved_by: 25-21 (9522e29, 9902370, b8ea775) plus quick 261026-vc6 (2300aac, f56405a, 1510def); blocked-replace live rerun passed 2026-10-27T04:46Z
  reason: "User chose to fix 25-20 review findings WR-01, WR-03, IN-01 and IN-03 (2026-10-27). WR-01: the Add-Type catch in RECORD_REPLACE rethrows without the original exception, so a missing csc.exe, unwritable TEMP or AV block are indistinguishable; every record write also pays the Add-Type compile (2.70 s in a fresh process, live preflight). WR-03: the blocked-replace path is only covered by an offline canned CLIXML stub; 34 of 352 live shipped replaces exhausted the retries under the race load. IN-01: projects.rs doc sentence garbled ('and nothing else' attaches to the wrong clause). IN-03: README and doc claims of 'never missing or half-written' are unconditional (NTFS-only; blocked replace becomes record_written:false; error 5 is also retried)."
  severity: minor
  source: 25-20-REVIEW.md (WR-01, WR-03, IN-01, IN-03)
  progress: "25-21 live run on ign-win-836 (2026-10-27): WR-01 cause reporting is proven live (the Add-Type cause test passed and the blocked-replace detail names Win32 error 5 with its message); IN-01 and IN-03 doc fixes landed (b8ea775); the Add-Type cost is recorded (baseline median 1.51 s, record_replace median 2.04 s, share 0.53 s); the unchanged race gate passed against the changed text. WR-03 (blocked replace proven live: transport_failed, temp removed, old record intact, rerun after release) is pending G-25-6. G-25-5 stays failed until G-25-6 is resolved."
  artifacts:
    - crates/ignition-core/src/deploy/transport/winps.rs (RECORD_REPLACE Add-Type guard)
    - crates/ignition-core/tests/live_transport_wflwvm01.rs (no live blocked-replace test)
    - crates/ignition-core/src/deploy/transport/projects.rs (doc)
    - README.md (record-replace guarantee wording)
  missing:
    - "The thrown text of a failed Add-Type/MoveFileEx carries a bounded, single-line cause (exception type and message, or the Win32 error code), surfaced in the transport_failed detail; still fail-closed, no fallback"
    - "Evaluate removing the per-write compile cost (for example reflection-emit DefinePInvokeMethod instead of Add-Type) only if it keeps the same atomic MoveFileEx call and the fail-closed guard; otherwise keep Add-Type and record the cost"
    - "An ignored live test on ign-win-836 that holds release.json open without FileShare.Delete while record_replace runs, and asserts transport_failed, temp removed, old record intact"
    - "Doc fixes for IN-01 and IN-03"

- gap_id: G-25-6
  truth: "The live blocked-replace test reads the Win32 code from the real PS 5.1 error record and proves transport_failed, temp removed, old record intact and a successful rerun after release"
  status: resolved
  resolved_by: quick 261026-vc6 (2300aac, f56405a, 1510def); live rerun of wflwvm01_record_replace_blocked_by_reader PASSED 2026-10-27T04:43Z-04:46Z (Win32 error Some(5), no temp left, old record sha256 unchanged, rerun after release ok); see 25-LIVE-CAPTURES.md ### Blocked replace rerun after G-25-6
  reason: "25-21 Task 4 live run on ign-win-836 (2026-10-27): wflwvm01_record_replace_blocked_by_reader FAILED with 'fix failed: expected Win32 error 5 or 32, got None' at live_transport_wflwvm01.rs:5856. The product behaved correctly: after 16.44 s of retries the writer returned transport_failed naming Win32 error 5 and 'Access is denied', and a manual host check afterwards found only release.json (262 bytes, run_id G255-OLD) in the scratch project directory, so the old record was intact and no temp file was left. The test's parser read None, so the test panicked before its temp-file and old-record checks, before its own cleanup and before the rerun-after-release step. Those three claims of WR-03 are therefore not proven by the test."
  severity: minor
  source: 25-21 Task 4 live run
  test: 4
  root_cause: "win32_code_in squashes whitespace and takes the digits after the FIRST occurrence of 'therecordreplacefailedwithWin32error'. The real PowerShell 5.1 error record repeats the failing source line before the FullyQualifiedErrorId line, so the first occurrence is the throw expression in the excerpt, followed by a quote and not a digit. Real lines: '+ ... $p.retries){throw ('the record replace failed with Win32 error '+$c+' ...' and, later, '+ FullyQualifiedErrorId : the record replace failed with Win32 error 5: Access is denied'. The offline test win32_code_in_reads_a_wrapped_detail passed because its modelled detail has no source excerpt; the model and the real record differ."
  artifacts:
    - crates/ignition-core/tests/live_transport_wflwvm01.rs (win32_code_in, wflwvm01_record_replace_blocked_by_reader)
  missing:
    - "A parser fixture copied from the real captured detail (the blocked-replace CAPTURE in 25-LIVE-CAPTURES.md ### Windows record replace failure causes and blocked replace (G-25-5)), with win32_code_in fixed to read the code from the FullyQualifiedErrorId line (or the last occurrence) and the offline test asserting Some(5) on that fixture"
    - "A rerun of wflwvm01_record_replace_blocked_by_reader live on ign-win-836, passing through transport_failed, temp removed, old record intact and the successful rerun after the holder is released"
