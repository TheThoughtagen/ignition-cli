---
status: partial
phase: 25-guarded-project-deploy
source: [25-VERIFICATION.md]
started: 2026-10-24T05:00:00Z
updated: 2026-10-27T04:00:00Z
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
result: blocked
blocked_by: other
reason: "needs a Windows Rust build environment"

## Summary

total: 6
passed: 2
issues: 0
pending: 0
skipped: 0
blocked: 4

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
  status: failed
  reason: "User chose to fix 25-20 review findings WR-01, WR-03, IN-01 and IN-03 (2026-10-27). WR-01: the Add-Type catch in RECORD_REPLACE rethrows without the original exception, so a missing csc.exe, unwritable TEMP or AV block are indistinguishable; every record write also pays the Add-Type compile (2.70 s in a fresh process, live preflight). WR-03: the blocked-replace path is only covered by an offline canned CLIXML stub; 34 of 352 live shipped replaces exhausted the retries under the race load. IN-01: projects.rs doc sentence garbled ('and nothing else' attaches to the wrong clause). IN-03: README and doc claims of 'never missing or half-written' are unconditional (NTFS-only; blocked replace becomes record_written:false; error 5 is also retried)."
  severity: minor
  source: 25-20-REVIEW.md (WR-01, WR-03, IN-01, IN-03)
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
