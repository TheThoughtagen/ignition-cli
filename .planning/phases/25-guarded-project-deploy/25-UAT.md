---
status: testing
phase: 25-guarded-project-deploy
source: [25-VERIFICATION.md]
started: 2026-10-24T05:00:00Z
updated: 2026-10-24T05:00:00Z
---

## Current Test

number: 1
name: First real run of .github/workflows/deploy-projects.yml on a tag with a dev environment, from a project repo caller
expected: |
  Sparse cross-repo targets checkout works with IGN_TARGETS_TOKEN and the token is masked in the log; IGN_MASTER_TOKEN / IGN_SSH_KEY reach the apply job only through its own environment (the secret split); check's artifact_sha256 and targets_sha reach apply through needs and workflow_call outputs; apply waits for approval and deploys exactly the checked hash
awaiting: user response

## Tests

### 1. First real run of .github/workflows/deploy-projects.yml on a tag (for example v0.0.1-test) with a dev environment, from a project repo caller
expected: Sparse cross-repo targets checkout works with IGN_TARGETS_TOKEN and the token is masked in the log; IGN_MASTER_TOKEN / IGN_SSH_KEY reach the apply job only through its own environment (the secret split); check's artifact_sha256 and targets_sha reach apply through needs and workflow_call outputs; apply waits for approval and deploys exactly the checked hash
result: [pending]

### 2. Concurrency queue of deploy-projects.yml: start two applies for the same gateway and project
expected: One running, one pending; a newer pending apply replaces the older pending one (cancel-in-progress false); check what happens to a prod job that needs the replaced dev job
result: [pending]

### 3. Run the workflow on a self-hosted Windows runner (Git Bash, Win32-OpenSSH, cygpath)
expected: Install, config, targets and run steps behave as on Linux
result: [pending]

### 4. Windows ssh node: release-record replace with a concurrent reader (Move-Item -Force)
expected: A reader sees the old or the new record, never a torn file
result: [pending]

### 5. WR-02 judgment: an Internal (unclassified 4xx/5xx) answer to the import request is treated as outcome unknown and read back
expected: Owner confirms that a deterministic gateway 4xx on import reporting imported_unverified (exit 6) with no second import is acceptable, instead of a plain error
result: [pending]

### 6. Compile and run the non-unix (portable) variant of local_fs.rs on a Windows target
expected: Builds and the record writer's temp-file cleanup (WR-04) behaves as on unix
result: [pending]

## Summary

total: 6
passed: 0
issues: 0
pending: 6
skipped: 0
blocked: 0

## Gaps
