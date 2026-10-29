# Phase 23: Guarded Apply & Rollback - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-10-15
**Phase:** 23-guarded-apply-rollback
**Areas discussed:** Lock window / hold budget, Run directory & run IDs, Journal recovery flow, Snapshot/.gwbk/Designer wait

---

## Lock window / hold budget

| Option | Description | Selected |
|--------|-------------|----------|
| On-box batched commit | Check hashes before the lock. Under the lock, one remote script re-hashes and moves the files locally, then the scan runs. | ✓ |
| Keep sftp renames, hash check before lock | Removes about a third of the commit time but still renames one file at a time | |
| Raise the ceiling | Allow a hold of about 700 s or more | |

| Option | Description | Selected |
|--------|-------------|----------|
| Re-measure, fixed constant | Re-run OQ5 with the batched commit, then use one formula constant with floor 10 s and ceiling 300 s | ✓ |
| Per-run, scaled by file count | Base plus per-file cost, clamped | |
| Target-file override | Default plus a per-target `hold_timeout_s` | |

| Option | Description | Selected |
|--------|-------------|----------|
| Refuse pre-write | Name the estimate and the budget. Nothing is locked or written. | ✓ |
| Attempt it anyway | Rely on expiry detection and snapshot restore | |
| You decide | | |

| Option | Description | Selected |
|--------|-------------|----------|
| Inside lock, same script | Snapshot to `snapshots/<run_id>/`, downloaded after the scan | ✓ |
| Before lock via sftp | Shorter lock, but a change between the snapshot and the lock is missed | |

| Option | Description | Selected |
|--------|-------------|----------|
| Stop and ask me | Checkpoint if the re-measured hold is still above 300 s | ✓ |
| Raise ceiling to measured value | Pre-authorize up to 900 s | |

**Notes:** Facts shown before the decision: the lock cannot be extended (re-acquire gets 409), and the current commit fetches every staged file back and renames files one at a time at 1.6 to 2.3 s per file.

---

## Run directory & run IDs

| Option | Description | Selected |
|--------|-------------|----------|
| Repo-local .ign/deploy/ | 0700/0600, and apply refuses if git does not ignore the path | ✓ |
| User state dir | Through `directories` | |
| Repo-local plus --run-dir override | | |

| Option | Description | Selected |
|--------|-------------|----------|
| UTC stamp + random suffix | `20261015T142233Z-a1b2c3` | ✓ |
| ULID | | |

| Option | Description | Selected |
|--------|-------------|----------|
| On-box snapshot is enough | An on-box run record means rollback needs only the target and the run id | ✓ |
| Requires the local run dir | | |

| Option | Description | Selected |
|--------|-------------|----------|
| Keep all, no pruning | | ✓ |
| Keep last N on-box snapshots | | |

---

## Journal recovery flow

| Option | Description | Selected |
|--------|-------------|----------|
| rollback --run <id> | The journal names the run, and the refusal prints the command | ✓ |
| New `ign deploy recover` verb | | |
| Apply auto-recovers | | |

| Option | Description | Selected |
|--------|-------------|----------|
| Clear if live matches | Pre-write phase and live hash equals the pre-run fingerprint, so clear without restoring | ✓ |
| Always restore | | |

| Option | Description | Selected |
|--------|-------------|----------|
| No flag | Manual recovery through `.gwbk` or the snapshot, then delete the journal by hand | ✓ |
| --abandon-journal "<reason>" | | |

| Option | Description | Selected |
|--------|-------------|----------|
| Restore without the check | An interrupted run has no post-apply fingerprint | ✓ |
| Require --discard-live-changes | | |

---

## Snapshot, .gwbk, Designer wait

| Option | Description | Selected |
|--------|-------------|----------|
| Before lock, failure refuses | Only `--skip-gwbk` bypasses it | ✓ |
| Before lock, failure warns | | |

| Option | Description | Selected |
|--------|-------------|----------|
| Fixed default, flag override | Poll every 10 s up to 600 s, with `--designers-timeout` to change it | ✓ |
| Fixed only | | |
| You decide | | |

| Option | Description | Selected |
|--------|-------------|----------|
| Session list in run + events | No reason string | ✓ |
| Require a reason | | |

---

## Claude's Discretion

- The journal phase markers, how the per-file cost is derived for the size refusal, the on-box script structure and chunking, and the human output of apply and rollback.

## Deferred Ideas

- Run and snapshot pruning, per-target hold override, per-run budget scaling, a `recover` verb, and `--abandon-journal`.
