# Phase 24: Redundant Pair - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-10-19
**Phase:** 24-redundant-pair
**Areas discussed:** Backup file reads, Wait/resync/verdict, verify --node + secrets, Pair rig + unsynced test

---

## Backup file reads

| Option | Description | Selected |
|--------|-------------|----------|
| Read-only transport | Same ssh/local declaration as master; only the read half is built | ✓ |
| REST only | Compare REST effective views, skip file hashes | |
| Optional transport | Files when declared, else REST-only with a warning | |

| Option | Description | Selected |
|--------|-------------|----------|
| Fail preflight | Check backup read access before any write | ✓ |
| Report at the end | Apply master, report backup failure afterwards | |

| Option | Description | Selected |
|--------|-------------|----------|
| Same set as master verify | Plan's tree plus tag units, byte-equal to master's live copy | ✓ |
| Whole mode collection | Hash everything under resources/<mode> | |
| You decide | Planner chooses | |

| Option | Description | Selected |
|--------|-------------|----------|
| Refuse | A declared backup needs a file transport | |
| REST-only + warning | Run REST checks, record that the file check was skipped | ✓ |

**User's choice:** Read-only transport, fail preflight on unreadable files, same compare set as master verify, REST-only backups allowed with a recorded warning.

---

## Wait, resync, verdict

| Option | Description | Selected |
|--------|-------------|----------|
| 120 s default + flag | Poll 5 s up to 120 s, `--backup-timeout` | |
| Measured constant | Measure on the rig, no flag | |
| 300 s default + flag | More margin, slower failure | ✓ |

| Option | Description | Selected |
|--------|-------------|----------|
| Sync moved + content equal | Timestamp advanced, syncPending=false, peerConnected, content equal | ✓ |
| Content equal only | Ignore sync timestamps | |

| Option | Description | Selected |
|--------|-------------|----------|
| Automatic, once | One resync to the master after the wait times out | ✓ |
| Opt-in --resync | Resync only with the flag | |
| Opt-out --no-resync | Automatic, flag turns it off | |

| Option | Description | Selected |
|--------|-------------|----------|
| New outcome, exit 6 | New OutcomeKind, payload slug deploy_backup_unsynced | ✓ |
| Applied + backup block | Outcome stays Applied, exit 6 when the block is red | |

| Option | Description | Selected |
|--------|-------------|----------|
| deploy_backup_unsynced | One slug with a reason field | ✓ |
| Separate slug | deploy_backup_unreachable for a down backup | |

| Option | Description | Selected |
|--------|-------------|----------|
| End early, own reason | Sync exhausted ends the wait, one resync, reason sync_exhausted | ✓ |
| No special case | Full wait runs out | |

| Option | Description | Selected |
|--------|-------------|----------|
| Both | Status prints active node; apply refuses when master is not active | ✓ |
| Status only | Wire the status command only | |

**User's choice:** 300 s default with `--backup-timeout`; converged = sync advanced plus content equal; one automatic resync to the master; new outcome kind with payload slug and exit 6; one slug with reasons; sync-exhausted ends the wait early; active node in status and in apply preflight.

---

## verify --node + secrets

| Option | Description | Selected |
|--------|-------------|----------|
| master\|backup\|both, default master | Keeps today's verify behaviour | ✓ |
| Default both when a backup is declared | Verify follows the target | |

| Option | Description | Selected |
|--------|-------------|----------|
| Every Referenced name in the tree | All providers the tree references | ✓ |
| Default provider only | Only `default` | |

| Option | Description | Selected |
|--------|-------------|----------|
| Apply preflight, both nodes | Missing names refuse before any write | ✓ |
| Verify only | Matches DPAIR-03 wording exactly | |

**User's choice:** `--node master|backup|both` default master; check every referenced secret name; apply preflight runs the check on both nodes.

---

## Pair rig + unsynced test

| Option | Description | Selected |
|--------|-------------|----------|
| In-repo test fixture | tests/fixtures/live-rig-pair, IGNITION_LIVE_PAIR=1 | ✓ |
| `ign rig` topology | Shipped product feature | |

| Option | Description | Selected |
|--------|-------------|----------|
| Cut the GAN link | Backup reachable but not syncing, reason not_syncing | ✓ |
| Stop the backup | Reason unreachable | |
| Both cases | One gate per reason | |

| Option | Description | Selected |
|--------|-------------|----------|
| Capture before engine work | First plan builds the rig and records captures | ✓ |
| Capture alongside | Capture at the gate stage | |

| Option | Description | Selected |
|--------|-------------|----------|
| Local transport | Bind-mount gw-b data, nodes.backup.local | ✓ |
| REST-only backup | Gate REST checks and the warning only | |
| Both | One gate each | |

**User's choice:** In-repo pair rig fixture; cut the GAN link for the unsynced gate; capture first; local file transport for the rig backup.

---

## Claude's Discretion

- Convergence polling interval.
- Name of the new outcome kind and the result's backup section.
- Whether `verify --node both` waits or resyncs (default: no).
- How the read-only backup transport is wired.
- Pair rig ports and file layout.

## Deferred Ideas

- Redundant-pair topology in `ign rig`.
- A live gate for reason `unreachable`.
- ssh file access to a backup proven live.
