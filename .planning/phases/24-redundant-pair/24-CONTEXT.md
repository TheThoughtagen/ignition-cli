# Phase 24: Redundant Pair - Context

**Gathered:** 2026-10-19
**Status:** Ready for planning

<domain>
## Phase Boundary

A deploy to a redundant pair ends with proof that the backup holds the same config as the master. `ign` checks the backup through its own REST API and its own files, never by writing to it. A backup that does not converge is reported as `deploy_backup_unsynced` (exit 6) and the master deploy stands. `ign deploy verify --node both` runs the backup checks on demand, including a check that every secret name the tree references exists on both nodes. A two-node 8.3.6 rig live-gates all of this (DPAIR-01..04).

Out of scope (carried forward): any write to the backup node, restarts or failover inside apply (SF-3), `ign redundancy restart --rolling`.

</domain>

<decisions>
## Implementation Decisions

### Backup file reads
- **D-01:** The backup's files are read through the same transport declaration as the master (`nodes.backup.ssh` or `nodes.backup.local` in the target). `ign` builds only the read half for the backup (the phase 22 pull/verify reader). No write half (`mirror_for_node`, `ApplyHost`) is ever constructed for `nodes.backup`, so "no writes to the backup" holds by construction, not by convention. — **Reversibility:** costly — the read-only backup transport becomes part of the target-file contract and the "no backup writes" guarantee rests on it.
- **D-02:** When `nodes.backup` declares a file transport, apply checks before any write that the backup account can read the files it will compare (alongside the existing `mode_backup` preflight). A failure refuses before the lock. Phase 23 found exactly this gap on Linux (`data` / `data/config` list rights).
- **D-03:** The backup files compared are the same set master verify already hashes: the mode-collection tree the plan wrote plus the tag units. Each must be byte-equal to the master's live copy (not to the render).
- **D-04:** A `nodes.backup` with only a REST profile (no `ssh` or `local` block) is allowed. The backup checks then run REST-only, and the result records a warning check that the file-hash check was skipped. This is a deliberate relaxation of DPAIR-01's "file hashes" for pairs where the deploy account has no access to the backup host; the warning must be visible in human output and `--json`.

### Convergence wait, resync and verdict
- **D-05:** After master verify and finalize, apply waits for the backup to converge, polling, for up to 300 s by default. `--backup-timeout <secs>` changes the budget (bounded like other timeouts). The wait runs outside the scan-lock and never spends hold budget.
- **D-06:** "Converged" means all of: the backup's `lastSyncTimestamp` advanced compared with the backup's own sample taken before the apply (`-1` = never synced), `syncPending=false`, `peerConnected=true`, and the content checks pass on the backup: same `deploymentMode`, effective view equal to the master's, and file hashes equal (D-03). A 200 or a timestamp alone is never proof (research PITFALLS Pitfall 9).
- **D-07:** If the wait times out, apply automatically sends one resync, `POST /data/api/v1/redundancy/gwaction/resync`, to the **master** (never to the backup), waits one more budget, and checks again. The resync and its response are recorded in events. At most one resync per run.
- **D-08:** A master that applied and verified with a backup that did not converge ends in a new outcome kind (for example `AppliedBackupUnsynced`): `result.json` `ok=false`, slug `deploy_backup_unsynced` carried in the payload (Decision A, the same pattern as `deploy_rolled_back`: not a `CoreError` variant, not in `DEPLOY_SLUGS`), exit 6. The master is never rolled back. The backup checks are listed with a reason. — **Reversibility:** one-way — the outcome kind, slug and exit code are a published scripting contract (README exit table exception, `--json` payload, `result.json`).
- **D-09:** Every "backup not proven" case uses the one slug `deploy_backup_unsynced`, with a `reason` field: `unreachable` (REST down), `not_syncing` (peer disconnected or timestamp never advanced), `content_differs` (synced but content not equal), `sync_exhausted` (TEC-5105). — **Reversibility:** costly — the reason values are part of the payload contract.
- **D-10:** If the backup shows Ignition's sync-exhausted state ("sync failed 5 times", TEC-5105), apply ends the wait early, still sends the one resync, then reports `deploy_backup_unsynced` with reason `sync_exhausted`. How the state is detected is a C12 capture question (D-17).
- **D-11:** `ign redundancy status` prints the active node using the Phase 20 `active_node` library (activity level, live `Master`/`Backup` vocabulary). Apply's preflight refuses when the target's `nodes.master` is not the currently active node (for example after a failover), so `ign` never writes to the node that is now the backup.

### verify --node and secret names
- **D-12:** `ign deploy verify --node master|backup|both`, default `master`. The default keeps today's verify behaviour and exit codes. `--node backup` runs the backup checks against the master's live copy; `--node both` runs master verify then the backup checks. `verify` does not wait or resync unless the planner finds a reason; it reports the current state. — **Reversibility:** costly — the flag and its default are a CLI contract.
- **D-13:** The secret check collects every `{providerName, secretName}` the rendered tree references (the lint's `is_referenced` shape), lists `GET /data/api/v1/secret-providers/<provider>/secrets` on each checked node, and fails when any referenced name is missing. Names only, never values (ENT-3115, spec §9). The check runs on the nodes `--node` selects.
- **D-14:** Apply also runs the secret-name check as a preflight on the master and, when declared, the backup. A missing name refuses before any write.

### Pair rig and live gates
- **D-15:** The two-node 8.3.6 rig is an in-repo test fixture, `crates/ignition-core/tests/fixtures/live-rig-pair/`: a compose file adapted from the spike (`gw-a` master, `gw-b` backup, new host ports), and a setup script that trusts the gateway-network certs, approves the incoming connection and mints tokens, following `live-rig-deploy`. The gates are behind `IGNITION_LIVE_PAIR=1`. It is not an `ign rig` topology.
- **D-16:** The backup on the rig gets a local file transport (gw-b's data dir bind-mounted to the host, declared as `nodes.backup.local`), so the backup file-hash path runs live. ssh to a backup is not proven on the rig.
- **D-17:** The gate that must end `deploy_backup_unsynced` cuts the gateway-network link before the apply (disconnect gw-b from the compose network or block its GAN port). The master applies and verifies, the backup never syncs, the one resync has no effect, and the verdict reason is `not_syncing`. The gate then reconnects and confirms the pair re-converges.
- **D-18:** Capture first. The first plan builds the pair rig and records live captures before any backup-check code: redundancy status fields during and after a sync, the resync response and its effect, the sync-exhausted signal (C12, whether resync alone clears it) if it can be reached, and secret-provider listings on both nodes. Later plans build against those captures, as phases 20-23 did.

### Claude's Discretion
- Polling interval for the convergence wait.
- The exact name of the new outcome kind and of the result's backup section.
- Whether `verify --node both` ever waits or resyncs (default: it does not; it reports the current state).
- How the read-half backup transport is wired into `deploy::transport` (a read-only constructor or a typed wrapper).
- The pair rig's port numbers and file layout.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase scope and requirements
- `.planning/ROADMAP.md` (Phase 24 section, about lines 470-485) — goal, success criteria, "Capture first" C12 plus the GAN trust recipe, constraints (no backup writes, active node by activity level, names-only secrets per ENT-3115, failover library method allowed but no verb calls it, no restart inside apply per SF-3, TEC-5105 own finding, Decision A slug).
- `.planning/REQUIREMENTS.md` lines 68-71 (DPAIR-01..04), 104 (no backup writes), 125 (DAPI-07 CLI wiring in Phase 24), 180-183 (DSURF carry-forward).
- `.planning/PROJECT.md` lines 31-40.

### Research
- `.planning/research/FEATURES.md` §3.5 (lines 203-235) and §4.4 (line 302) — backup check table, proposed `--node`, `--timeout`, `--resync`, sync-exhausted log detection.
- `.planning/research/PITFALLS.md` Pitfall 9 (lines 177-181), Pitfall 23, C12 (line 735), lines 477, 538, 600-601, 674 — verify by content, timestamp must advance against the same node, one resync, `uptime:-1`/`Cold` is normal.
- `.planning/research/ARCHITECTURE.md` §5.1-5.2 (lines 214-249: `redundancy_resync` sketch, a 200 is never proof) and §7.2-7.3 (lines 358-415: verdict shape, backup JSON around line 397). Older slug names there (`backup_out_of_sync`, `applied_backup_stale`) are superseded by `deploy_backup_unsynced`.
- `.planning/research/SUMMARY.md` lines 63, 171, 230.

### Binding spec (outside this repo)
- `docs/superpowers/specs/2026-10-09-prod-gateway-config-as-code-design.md` on branch `docs/prod-gateway-config-as-code` of `/Users/pmannion/whiskeyhouse/whk-environment-orchestration` (PR #60). Read with `git -C /Users/pmannion/whiskeyhouse/whk-environment-orchestration show "docs/prod-gateway-config-as-code:docs/superpowers/specs/2026-10-09-prod-gateway-config-as-code-design.md"`. §3 (resync row, about line 57), §6 step 5 "Backup verification" (about lines 193-196), §9 "Secrets and identities" (about line 226), §12 S3 row (line 297), TEC-5105 risk (line 335). The §9 B(i) amendment is still pending upstream (PR #63).
- Spike: `spikes/gateway-modes/docker-compose.yml`, `s3_redundancy.sh` (`trust_gan()`, `approve_pending()`, `settled()`, resync on the master at line 72) and `results/S3.md`, `results/RIG.md` on the same branch.

### Live captures and API references
- `.planning/phases/20-config-api-client/20-LIVE-CAPTURES.md` line 14 (block C-S3, live pair data) and `20-config-api-client/COVERAGE.md` line 28 (`redundancy.resync` opt-out to Phase 24).
- `.planning/phases/21-deploy-render/21-LIVE-CAPTURES.md` lines 936-952 and 2208-2215 — `GET /data/api/v1/secret-providers/<provider>/secrets` returns `{"secrets":[...]}`.
- `/Users/pmannion/whiskeyhouse/83-api/bruno/Ignition HTTP API/redundancy/` — `Re-Sync Configuration.bru`, Status, Force Failover.
- `/Users/pmannion/whiskeyhouse/83-api/postman/8.3.postman_collection_v2.json` about line 48849 (resync).

### Prior phase decisions
- `.planning/phases/23-guarded-apply-rollback/23-CONTEXT.md` — apply engine, run store, Decision A verdict slugs, scan-lock hold.
- `.planning/phases/22-transport-plan-pull-verify/22-CONTEXT.md` — transport read half, verify checks.
- `.planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md` — Linux account list-rights finding (relevant to D-02).

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/ignition-core/src/client/redundancy.rs`: `RedundancyStatusWire` (role, activityLevel, peerConnected, syncPending, lastSyncTimestamp, uptime...), `last_sync_epoch_ms()` (`-1` = never), `ActiveNode` and `active_node(a, b)` at about line 165 (tested, not yet called in production).
- `ign redundancy status`: `crates/ignition-cli/src/cli.rs:289-291, 2004-2014`, dispatch `crates/ignition-cli/src/main.rs:3549-3556`, action `crates/ignition-core/src/actions/redundancy.rs:21` (no active-node field yet).
- `crates/ignition-core/src/deploy/target.rs:36-64`: `Nodes { master, backup: Option<NodeTarget> }` already parsed and validated; `NodeTarget` has `ssh`/`local`.
- Preflight `mode_backup`: `crates/ignition-core/src/deploy/apply/preflight.rs:264-271, 315ff`.
- Verify evaluator: `crates/ignition-core/src/deploy/verify.rs` (checks scan, mode, read_back, files, loaded, probes; functions at about 543-968).
- Secret-reference detection: `crates/ignition-core/src/deploy/lint.rs:42-54, 184, 209` (`is_referenced`, `scan_secrets`).
- Single-node rig to copy: `crates/ignition-core/tests/fixtures/live-rig-deploy/` (compose.yml 8.3.6, compose.bind.yml, mint-token.sh, capture scripts).

### Established Patterns
- Verdict slugs ride the payload (Decision A): `ROLLED_BACK_SLUG` at `deploy/apply/mod.rs:134`, `failure_exit_code` at `actions/deploy_apply.rs:113-116`, CLI exit at `main.rs:596-597`, README dated exceptions at `README.md:102-117, 131-157`, pinned by `deploy_family_coverage.rs` tests.
- Live gates behind env switches (`IGNITION_LIVE_DEPLOY`, `IGNITION_LIVE_LNXVM`); captures recorded in `<NN>-LIVE-CAPTURES.md` before engine work.
- The backup today gets only a REST session (`named_profile_client`, `main.rs:3343-3358`, `:3452-3469`) passed as `ApplySurfaces.backup_deploy`; `transport::for_node` is only called for `"nodes.master"`.

### Integration Points
- Apply green path ends in `drive` (`deploy/apply/mod.rs` about 1167-1225): after finalize and the restart report, before the `Applied` return, outside the lock. The backup phase slots in there.
- `OutcomeKind` (`mod.rs:150-167`) and `run_apply`'s single exit (`:731-765`) need the new outcome.
- `CoreError::DEPLOY_SLUGS` (`error.rs:1839`, length test at `:3398`) must not gain `deploy_backup_unsynced` (Decision A).
- No client method exists yet for resync or for listing secret names.
- DSURF-01 coverage: `crates/ignition-cli/tests/deploy_family_coverage.rs`; DSURF-03 canaries: `crates/ignition-cli/tests/contract_deploy.rs` (about 2546-3096), `crates/ignition-core/tests/deploy_apply.rs` (about 4506-4684). Both must stay green with `--node both` and the backup verdict added.

</code_context>

<specifics>
## Specific Ideas

- The spike's `settled()` predicate (backup `peerConnected`, `syncPending=false`, both nodes `projectState=Good`) is a starting point for D-06, extended with the timestamp-advance and content checks.
- The unsynced live gate must reconnect the backup afterwards and confirm the pair re-converges, so the rig is left healthy.

</specifics>

<deferred>
## Deferred Ideas

- A redundant-pair topology in the shipped `ign rig` command (D-15 keeps the pair rig as a test fixture).
- A second unsynced gate for reason `unreachable` (stopping the backup). D-17 proves `not_syncing` only; `unreachable` is unit-tested.
- ssh file access to a backup proven on a live host (the rig uses local transport, D-16).

</deferred>

---

*Phase: 24-redundant-pair*
*Context gathered: 2026-10-19*
