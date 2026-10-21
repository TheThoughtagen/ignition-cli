# Phase 24: Redundant Pair - Research

**Researched:** 2026-10-19
**Domain:** Rust CLI (ignition-cli workspace), Ignition 8.3.6 redundancy: read-only backup verification after a guarded master deploy, plus a two-node Docker pair rig as a live gate
**Confidence:** MEDIUM-HIGH (the code seams are fully read and HIGH; the gateway behaviors that decide D-06 and D-10 are not yet captured, which is exactly what plan 24-01 is for)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Backup file reads**
- **D-01:** The backup's files are read through the same transport declaration as the master (`nodes.backup.ssh` or `nodes.backup.local` in the target). `ign` builds only the read half for the backup (the phase 22 pull/verify reader). No write half (`mirror_for_node`, `ApplyHost`) is ever constructed for `nodes.backup`, so "no writes to the backup" holds by construction, not by convention. Reversibility: costly.
- **D-02:** When `nodes.backup` declares a file transport, apply checks before any write that the backup account can read the files it will compare (alongside the existing `mode_backup` preflight). A failure refuses before the lock. Phase 23 found exactly this gap on Linux (`data` / `data/config` list rights).
- **D-03:** The backup files compared are the same set master verify already hashes: the mode-collection tree the plan wrote plus the tag units. Each must be byte-equal to the master's live copy (not to the render).
- **D-04:** A `nodes.backup` with only a REST profile (no `ssh` or `local` block) is allowed. The backup checks then run REST-only, and the result records a warning check that the file-hash check was skipped. This is a deliberate relaxation of DPAIR-01's "file hashes" for pairs where the deploy account has no access to the backup host; the warning must be visible in human output and `--json`.

**Convergence wait, resync and verdict**
- **D-05:** After master verify and finalize, apply waits for the backup to converge, polling, for up to 300 s by default. `--backup-timeout <secs>` changes the budget (bounded like other timeouts). The wait runs outside the scan-lock and never spends hold budget.
- **D-06:** "Converged" means all of: the backup's `lastSyncTimestamp` advanced compared with the backup's own sample taken before the apply (`-1` = never synced), `syncPending=false`, `peerConnected=true`, and the content checks pass on the backup: same `deploymentMode`, effective view equal to the master's, and file hashes equal (D-03). A 200 or a timestamp alone is never proof (research PITFALLS Pitfall 9).
- **D-07:** If the wait times out, apply automatically sends one resync, `POST /data/api/v1/redundancy/gwaction/resync`, to the **master** (never to the backup), waits one more budget, and checks again. The resync and its response are recorded in events. At most one resync per run.
- **D-08:** A master that applied and verified with a backup that did not converge ends in a new outcome kind (for example `AppliedBackupUnsynced`): `result.json` `ok=false`, slug `deploy_backup_unsynced` carried in the payload (Decision A, the same pattern as `deploy_rolled_back`: not a `CoreError` variant, not in `DEPLOY_SLUGS`), exit 6. The master is never rolled back. The backup checks are listed with a reason. Reversibility: one-way.
- **D-09:** Every "backup not proven" case uses the one slug `deploy_backup_unsynced`, with a `reason` field: `unreachable` (REST down), `not_syncing` (peer disconnected or timestamp never advanced), `content_differs` (synced but content not equal), `sync_exhausted` (TEC-5105). Reversibility: costly.
- **D-10:** If the backup shows Ignition's sync-exhausted state ("sync failed 5 times", TEC-5105), apply ends the wait early, still sends the one resync, then reports `deploy_backup_unsynced` with reason `sync_exhausted`. How the state is detected is a C12 capture question (D-17).
- **D-11:** `ign redundancy status` prints the active node using the Phase 20 `active_node` library (activity level, live `Master`/`Backup` vocabulary). Apply's preflight refuses when the target's `nodes.master` is not the currently active node (for example after a failover), so `ign` never writes to the node that is now the backup.

**verify --node and secret names**
- **D-12:** `ign deploy verify --node master|backup|both`, default `master`. The default keeps today's verify behaviour and exit codes. `--node backup` runs the backup checks against the master's live copy; `--node both` runs master verify then the backup checks. `verify` does not wait or resync unless the planner finds a reason; it reports the current state. Reversibility: costly.
- **D-13:** The secret check collects every `{providerName, secretName}` the rendered tree references (the lint's `is_referenced` shape), lists `GET /data/api/v1/secret-providers/<provider>/secrets` on each checked node, and fails when any referenced name is missing. Names only, never values (ENT-3115, spec §9). The check runs on the nodes `--node` selects.
- **D-14:** Apply also runs the secret-name check as a preflight on the master and, when declared, the backup. A missing name refuses before any write.

**Pair rig and live gates**
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

### Deferred Ideas (OUT OF SCOPE)
- A redundant-pair topology in the shipped `ign rig` command (D-15 keeps the pair rig as a test fixture).
- A second unsynced live gate for reason `unreachable` (stopping the backup). D-17 proves `not_syncing` only; `unreachable` is unit-tested.
- ssh file access to a backup proven on a live host (the rig uses local transport, D-16).

Out of scope (carried forward, CONTEXT `<domain>`): any write to the backup node, restarts or failover inside apply (SF-3), `ign redundancy restart --rolling`.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| DPAIR-01 | Apply ends by verifying the backup through its own API and file hashes: same `deploymentMode`, effective view equal to master, redundancy healthy, after a bounded convergence wait and at most one resync | New backup phase in `drive` after finalize (Pattern 1); two-tier poll (Pattern 2); new `DeployApi` reads/one master-only POST (Standard Stack); backup `TreeReader` from the existing `for_node` (Pattern 3); reuse of `verify::evaluate` read_back (Pattern 4); captures C-P1..C-P4 decide the predicate |
| DPAIR-02 | A backup that stopped syncing or still differs reports `deploy_backup_unsynced`, exit 6, never rolls back the master | New `OutcomeKind` + payload slug + single exit point change (Pattern 5); reason precedence; README dated exception + pinned test; live gate D-17 (cut link by `toggleApproval` false, Pitfall 2) |
| DPAIR-03 | `ign deploy verify --node both` also checks every referenced secret name exists on both nodes | New `secret_names` read, reference collector over the rendered tree (Pattern 6); verify result gains an additive `backup` section; D-14 preflight must skip providers the tree itself creates (Pitfall 6) |
| DPAIR-04 | A two-node 8.3.6 rig with a trusted GAN link live-gates DPAIR-01..03 | Pair fixture from the spike (`docker-compose.yml`, `s3_redundancy.sh` `trust_gan`/`approve_pending`/`settled`); `live_deploy_pair.rs` behind `IGNITION_LIVE_PAIR=1`; Validation Architecture below |
</phase_requirements>

## Summary

Phase 24 adds a **read-only backup verification phase** to the apply engine that Phase 23 built, plus the `--node` flag on `verify`, a secret-name check, the `active_node` wiring, and a pair rig fixture. The engine already has every seam this needs: `drive` returns after finalize with the lock already released by the scan (so "outside the lock" is automatic); `transport::for_node` already returns a **read-only** `Box<dyn TreeReader>` for any node block (so D-01 needs no new transport code, only a call with the label `"nodes.backup"` and a source-scan pin that the backup path never names the write half); `verify::evaluate` already compares a declared tree against a REST effective view; `active_node` already exists and is tested. The new surface is small: three `DeployApi` methods (`redundancy_status`-like read is already on `GatewayApi`; the new ones are the master-only `redundancy_resync` POST and the names-only `secret_names` GET), one pure `deploy/backup.rs` evaluator, one engine phase, a new `OutcomeKind`, and CLI flags.

The real risk is not the Rust, it is **three unverified gateway behaviors** that CONTEXT locked decisions rest on. (1) Whether the backup's `lastSyncTimestamp` actually advances on an incremental config sync (D-06 requires it; the spike never isolated this, and if it does not advance, every healthy apply would end `deploy_backup_unsynced` after 600 s). (2) Whether the backup's on-disk mode collection is byte-identical to the master's (D-03 requires byte equality; the backup gateway writes the files itself during sync and may re-stamp `resource.json`). (3) How "sync exhausted" is observable (D-10): the spike only saw it in the backup's **log**, but the 8.3.6 OpenAPI also exposes `GET /data/api/v1/redundancy/providers` (`lastSyncError`, `status`) and `GET /data/api/v1/redundancy/events`, which are structured and should be tried first. All three are capture-first items for plan 24-01 (D-18), and the plans after it must read the capture, not this document.

One design collision the planner must resolve explicitly: D-11 (apply refuses unless `active_node` says the master alone is active) versus D-17 (the unsynced gate cuts the link before the apply). In Ignition a backup that loses its master for `pingMaxMissed` pings can promote itself, which would make `active_node` return `Both` and D-11 would refuse the very run the gate needs. The rig must keep the backup `Cold` for the gate window (tune `pingMaxMissed` in the setup script's redundancy config) or D-11 must be scoped; capture C-P6 measures it. Also cut the link with **`toggleApproval/.../false` on the master**, not by disconnecting gw-b from its only Docker network, because a disconnect also kills the published REST port and would test `unreachable`, not `not_syncing`.

**Primary recommendation:** Build in this order: 24-01 pair rig + captures (decide D-06 predicate, D-03 byte equality, D-10 detection, C12, split-brain timing) -> 24-02 client + pure evaluator + fakes -> 24-03 engine phase + new outcome + preflights -> 24-04 `verify --node` + secrets + `redundancy status` wiring + README/coverage/canary -> 24-05 live gates. Keep the backup handle a narrow read-only type so "no write to the backup" is a compile-time property of the new code, and pin it with `ForbidWrites` in engine tests and a backup-side zero-non-GET assertion in wiremock/live tests.

## Project Constraints (from CLAUDE.md)

Directives extracted from `/Users/pmannion/whiskeyhouse/ignition-cli/.claude/CLAUDE.md` and the user's global CLAUDE.md. Treated as locked.

- **Tech stack:** Rust, clap derive, tokio, reqwest 0.13 (rustls), serde/serde_json, thiserror. Keep the dependency tree lean: **this phase adds no new crate** (see Package Legitimacy Audit).
- **Agentic usage:** every subcommand non-interactive by default with `--json`; new flags (`--backup-timeout`, `--node`, `--peer`) must have defaults that keep today's behavior and exit codes.
- **Exit-code table:** the source of truth is the README `## Exit codes` table and `crates/ignition-core/src/error.rs`, pinned by `readme_exit_table_agreement` and `exit_code_mapping_enumerated`. `deploy_backup_unsynced` is a **verdict slug**, so it must NOT be added to `CoreError`, `DEPLOY_SLUGS` (length test pins 12) or the exit table; it is documented as a dated README verdict exception and pinned in `deploy_family_coverage.rs`.
- **README text rule:** the README must not repeat the literal `## Exit codes` before the real section (STATE 22-11).
- **Worktrees / CARGO_TARGET_DIR:** no git worktrees sharing the global `CARGO_TARGET_DIR`; run plans sequentially on the main checkout (`workflow.use_worktrees=false`). `rm` is wrapped and rejects `-rf`; use `/bin/rm` for artifact surgery.
- **Installed binary:** `ign` on PATH is `~/.cargo/bin/ign`, not the repo build. `cargo install --path crates/ignition-cli --force` before any manual smoke check of a changed verb.
- **GSD workflow:** all edits go through a GSD command (`/gsd-execute-phase`).
- **User global:** use `uv` not `pip` for any Python; no private WhiskeyHouse content to non-Claude/Codex backends (foundry rule). Rig is localhost-only and disposable.
- **Value-free output (DSURF-03):** no secret value in plan/apply/verify output, `--json`, run-directory files or events. Secret **names** are allowed; values never.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Convergence wait, resync decision, reason classification | `ignition-core` `deploy/backup.rs` (pure) + `deploy/apply` engine | CLI flag parse | Pure decision + engine orchestration is testable without a gateway; the CLI only supplies `--backup-timeout` |
| Backup REST reads (status, mode, effective view, secret names) | `ignition-core` client (`DeployApi` + existing `GatewayApi::redundancy_status`) | -- | Same per-endpoint-declared wire as Phase 20; keeps GatewayApi's 15 test doubles untouched |
| Resync POST | `ignition-core` client, **master session only** | -- | D-07: the only write this phase adds, and it never targets the backup |
| Backup file reads | `deploy::transport::for_node` read half (`TreeReader`) | -- | D-01: read half only; write half never constructed for `nodes.backup` |
| Active-node decision | `client::redundancy::active_node` (pure) | preflight + `redundancy status` action | Activity level, never role |
| Verdict envelope, exit 6 | `ignition-cli` `main.rs` single exit mapping | `render.rs` | Decision A: payload slug, exit after the envelope |
| Pair rig and live gates | test fixture (`tests/fixtures/live-rig-pair/`) + `live_deploy_pair.rs` | -- | D-15: not shipped product |

## Standard Stack

### Core

No new dependencies. Everything reuses crates already in the workspace graph.

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `reqwest` | 0.13.x (workspace) | New `DeployApi` methods go through the existing `ReqwestGatewayApi` | One client, one credential site `apply_auth` [VERIFIED: crates/ignition-core/src/client/deploy_api.rs:15-16] |
| `tokio` | 1.x (workspace) | `tokio::time::sleep` poll loop (the designers wait precedent) | Already used by `designers_finding` [VERIFIED: preflight.rs:552] |
| `serde` / `serde_json` | workspace | Wire structs, `result.json`, events | Existing |
| `async-trait` | workspace | Trait methods on `DeployApi` | Existing |
| `wiremock` | workspace dev-dep | Contract tests for the new endpoints | Existing |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `percent-encoding` | workspace | Encode `<provider-name>` in the secrets path | The provider name is user data (reuse `validate_segment` + the resource path helpers) |
| `sha2` | workspace | File-hash compare (already via `tree::sha256_hex`) | Backup vs master compare |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| New `DeployApi` methods | New methods on `GatewayApi` | Rejected: `GatewayApi` has 15 hand-written test doubles; the module doc of `deploy_api.rs` says "the deploy wire must not force a change on any of them" [VERIFIED: deploy_api.rs:4-7]. Reading the **backup's** redundancy status and logs can use the existing `GatewayApi::redundancy_status` / `logs` with zero trait change. |
| Log scraping for sync-exhausted | `GET /redundancy/providers` (`lastSyncError`, `status`) and `GET /redundancy/events` | Prefer the structured endpoints if the capture shows they carry the signal (Open Question 3); the log is the fallback |
| `docker network disconnect gw-b` to cut the link | `PUT /gateway-network-incoming/toggleApproval/<id>/false` on the master | See Pitfall 2: a disconnect from the only network also kills the published REST port |

**Installation:** none.

**Version verification:** n/a (no packages added). In-repo versions confirmed by reading `crates/ignition-core/Cargo.toml` this session.

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| (none added) | -- | -- | -- | -- | -- | Phase 24 installs no external packages; `gsd-tools package-legitimacy check` not applicable |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```
 ign deploy apply --env E [--backup-timeout S]
   |
   v
 deploy_apply_prepare (host read, re-plan, gates)             [unchanged, Phase 23]
   |  Ready
   v
 CLI builds: master Session, backup Session(REST), backup TreeReader(read half only, label "nodes.backup")
   |
   v
 run_apply -> drive
   |
   |-- preflights (existing 7) + NEW: master_active (D-11), secrets (D-14), backup_read (D-02)
   |        any Fail -> deploy_preflight_failed, nothing locked or written
   |-- NEW sample: backup GatewayApi.redundancy_status -> lastSyncTimestamp_before   (in memory)
   |-- gwbk, ledger, prepare, HeldLock::acquire, commit, scan, master verify GREEN, finalize,
   |   restart report, snapshot download                       [unchanged]  <-- lock already released by the scan
   |
   |-- NEW backup phase (outside the lock, after finalize):
   |      loop until budget:
   |        tick: backup status read  --Network err--------------------------> remember "unreachable"
   |              |-- sync_exhausted signal? ---------------------------------> break early (D-10)
   |              |-- signals ok? (peerConnected, !syncPending, ts advanced, mode == target)
   |              |        yes -> content checks (REST effective view vs render; file hashes vs MASTER live)
   |              |                  all ok -> CONVERGED
   |        sleep interval
   |      budget spent or early break, not converged, not yet resynced:
   |        POST master /redundancy/gwaction/resync (once; response -> event)
   |        wait one more budget (or short re-check after sync_exhausted), same loop
   |      still not converged -> final full evaluation -> reason
   |
   v
 ApplyOutcome { kind: Applied | AppliedBackupUnsynced, backup: {...} }
   |
   v
 run_apply single exit: result.json {ok, slug, backup}   ->  main.rs: render envelope, then exit 6 if unsynced
```

Master is never restored on this path: the record is already `applied`, the journal is gone, and no branch of the backup phase calls `roll_back` or `restore_snapshot`.

### Recommended Project Structure

```
crates/ignition-core/src/
├── client/
│   ├── deploy_api.rs        # + redundancy_resync (POST, master only), secret_names (GET); DEPLOY_ENDPOINTS 14 -> 16
│   └── redundancy.rs        # + (optional) providers wire if the capture proves it carries lastSyncError
├── deploy/
│   ├── backup.rs            # NEW pure: BackupSignals, Converged predicate, reason classifier, wait_step,
│   │                        #   file-compare, secret-name check, BackupVerdict/BackupReport (serde)
│   ├── lint.rs              # + pub fn referenced_secret_names(&Node) (names only; reuses is_referenced)
│   └── apply/
│       ├── mod.rs           # + OutcomeKind::AppliedBackupUnsynced, ApplyEvent::Backup*, drive backup phase,
│       │                    #   BACKUP_UNSYNCED_SLUG, run_apply exit-point change
│       └── preflight.rs     # + PreflightCheck::{MasterActive, Secrets, BackupRead}; rollback must not require declared tree
├── actions/
│   ├── deploy.rs            # deploy_verify -> node-aware (master unchanged); deploy_verify_backup
│   ├── deploy_apply.rs      # ApplySurfaces + ApplyRunOptions gain backup reader/gateway + backup_timeout
│   └── redundancy.rs        # + active_node field (additive)
crates/ignition-cli/src/
├── cli.rs                   # deploy apply --backup-timeout; deploy verify --node; redundancy status --peer
├── main.rs                  # backup Session + backup TreeReader wiring; exit 6 for AppliedBackupUnsynced
└── render.rs                # human lines for backup block (warn row for REST-only)
crates/ignition-core/tests/
├── fixtures/live-rig-pair/  # NEW compose.yml, setup-pair.sh, mint-token-pair.sh, capture24.sh
├── common/fake_deploy.rs    # FakeGateway: redundancy state, resync counter, secret lists, pair link
├── common/forbid_writes.rs  # + redundancy_resync refuses, secret_names forwards
└── live_deploy_pair.rs      # NEW, behind IGNITION_LIVE_PAIR=1
```

### Pattern 1: The backup phase sits at the end of `drive`, after the snapshot download

**What:** Insert after `fetch_snapshot` and before building `ApplyOutcome` in `drive` [VERIFIED: apply/mod.rs:1189-1223]. The scan already released the scan-lock (D12), so the wait is outside the lock without extra code. Do the snapshot download first so the run directory is complete if the operator interrupts a 5-minute wait. Take the backup's `lastSyncTimestamp` sample **after the preflights pass and before `prepare`/lock**, so a refused run never leaves a sample and the sample predates every master write.

**When to use:** only when `declared.target.nodes.backup.is_some()`. No backup node: the outcome is unchanged and `backup` is `null`.

**Example (shape, not final code):**
```rust
// Source: derived from crates/ignition-core/src/deploy/apply/mod.rs drive() (read this session)
let backup_before = match ctx.backup {
    Some(b) => Some(b.gateway.redundancy_status().await?),   // GatewayApi, existing method; a read
    None => None,
};
// ... existing steps through finalize, restart_report, fetch_snapshot ...
let backup = match ctx.backup {
    Some(b) => Some(converge_backup(ctx, opts, plan, b, backup_before.as_ref(), &post, steps).await?),
    None => None,
};
let kind = match &backup {
    Some(v) if !v.converged => OutcomeKind::AppliedBackupUnsynced,
    _ => OutcomeKind::Applied,
};
```

### Pattern 2: Two-tier poll (cheap signals every tick, content only when signals pass)

**What:** Each tick reads only the backup's `GET /data/api/v1/redundancy` (one request). Only when `peerConnected && !syncPending && timestamp advanced && deploymentMode == target` does it run the content checks (REST effective view, then the file read). A failing content check keeps the loop going. At timeout, do one final full evaluation so the report names the true reason.

**Why:** a full file read over ssh measured 155 to 221 s for 152 files on the Windows host (22-14, STATE decision 22-14) and the default op timeout is 1050 s [VERIFIED: target.rs:115-120]. Doing it every 5 s tick would blow the 300 s budget on its own. The pure decision function takes injected `elapsed` so tests need no real clock, mirroring `wait_step` [VERIFIED: preflight.rs:511-535].

### Pattern 3: Backup `TreeReader` comes from the existing `for_node`

**What:** `transport::for_node(repo, target_file, "nodes.backup", backup_node)` returns `Box<dyn TreeReader>` and is the read half only [VERIFIED: transport/mod.rs:279-293]. A node with neither `ssh` nor `local` is refused there as `deploy_tree_invalid` [VERIFIED: transport/mod.rs:250-255], so the CLI must **not** call it for a REST-only backup; it passes `None` and the backup report carries the D-04 warning check. D-01's "by construction" property is then: the backup handle type has no write method, and a source-scan test (the Phase 22 `write_half_unreachable` pattern [VERIFIED: transport/mod.rs:21-23, 298-302]) extended to the new backup code asserts it never mentions `mirror_for_node`, `apply_host_for_node`, `ApplyHost`, `TreeMirror`.

### Pattern 4: Reuse `verify::evaluate` for the backup's mode and read-back

**What:** `Expected::from_render` + `ObservedState` + `evaluate` already implement mode, read_back (with the N1/N2/N3 normalization table) and files [VERIFIED: verify.rs:301, 445-496]. For the backup, build an `ObservedState` from the backup's REST reads (gather via the same `gather_read_back` helper, currently private in `actions/deploy.rs:982`, make `pub(crate)`), run `evaluate`, and keep only the `mode` and `read_back` checks. This proves the backup's effective view equals the render; because the master was just verified equal to the same render, "equal to the master's" holds transitively. For the file comparison use master live hashes (Pattern 7), not the render, per D-03. Document this interpretation of D-06 ("effective view equal to the master's") in the plan; the stricter alternative (backup REST vs master REST directly) needs a second master read of every type.

### Pattern 5: New outcome kind through the single exit point

**What:** `OutcomeKind` gains one variant [VERIFIED: apply/mod.rs:148-157, quoted below]; `run_apply`'s exit doc is built from it [VERIFIED: apply/mod.rs:739-747]; `DeployApplyResult::failure_exit_code` and `base_result` key off it [VERIFIED: deploy_apply.rs:112-117, 130-131]; `main.rs` maps the exit after the envelope [VERIFIED: main.rs:596-599]. All four must change together. Slug constant lives next to `ROLLED_BACK_SLUG`:

```rust
// existing, apply/mod.rs:134
pub const ROLLED_BACK_SLUG: &str = "deploy_rolled_back";
// new
pub const BACKUP_UNSYNCED_SLUG: &str = "deploy_backup_unsynced";
```
Result doc rule becomes: `ok = !matches!(kind, RolledBack | AppliedBackupUnsynced)`, `slug` by kind. `cause` stays null for the new kind; the reason is `backup.reason` (D-09), not `cause` (the closed `cause` vocabulary of rollback is `lock_expired|write_failed|scan_incomplete|verify_failed` and must not grow).

### Pattern 6: Secret references are collected by walking the same `Node` tree the lint walks

**What:** `is_referenced` is private to `lint.rs` and requires exactly `{type:"Referenced", data:{providerName, secretName}}` [VERIFIED: lint.rs:42-54]. Add `pub fn referenced_secret_names(node: &Node, out: &mut BTreeSet<(String,String)>)` beside it, reusing `is_referenced` and walking objects/arrays; call it over every rendered `.json` file parsed with `node::classify`. Names only, so no value can enter the set. Group by provider, one `GET .../secrets` per provider per node, compare as sets.

### Pattern 7: Backup file compare set and expected hashes

**What:** Compare set = every path in the apply's `WriteSet.writes` + every declared tag-unit file + every `WriteSet.deletes` path (must be **absent** on the backup). Expected hash for a touched path = the master's post-run fingerprint, which the engine already computed (`ctx.host.fingerprint(&spec.touched_paths())`, assigned to `post` in `drive` [VERIFIED: apply/mod.rs:1169-1173]). Untouched tag units need one master `reader.read(&scope)`. Backup hashes come from one `backup_reader.read(&scope)`; compare `observed.mode` maps by `RelPath` -> `sha256`. Report paths and hashes only (value-free).

### Anti-Patterns to Avoid
- **Passing the backup as `&dyn DeployApi` into new code.** That type has `resource_create`, `scan_request`, `mode_create`, `scan_lock_acquire`. Wrap the backup in a small read-only struct (fields private, only read methods exposed) so a write cannot even be expressed. Today's `backup_deploy: Option<&dyn DeployApi>` is a type-level hole [VERIFIED: apply/mod.rs:614-616]; keep it only for the existing `mode_backup` preflight or migrate it.
- **Treating the resync 200 as success.** Re-verify content afterwards; PITFALLS Pitfall 9 and 27.
- **Comparing `lastSyncTimestamp` to the runner clock** or across nodes. Only against the same node's pre-apply sample.
- **Reading role to decide who is active.** `active_node` uses `activityLevel` alone [VERIFIED: redundancy.rs:165-173].
- **Adding `deploy_backup_unsynced` to `CoreError`/`DEPLOY_SLUGS`/the exit table.** Verdict slug only (Decision A).
- **Running new preflights from rollback without a declared tree.** `rollback` has no render; `secrets` and `backup_read` need one. See Pitfall 7.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Read-only file access to the backup | A new reader or ssh wrapper | `transport::for_node(..., "nodes.backup", node)` | Already hardened (symlink, size, entry caps, "a failed read is never an empty read") [VERIFIED: transport/mod.rs:10-17] |
| Which node is active | Role comparison | `redundancy::active_node` | Role is configuration; activity moves on failover [VERIFIED: redundancy.rs:23-31] |
| Effective-view comparison and null/subset normalization | A second JSON differ | `verify::evaluate` / `Expected::from_render` | CV2 table (N1/N2/N3) already encodes live 8.3.6 quirks [VERIFIED: verify.rs:25-49] |
| Poll timing decision | `loop { sleep }` with ad hoc math | A pure `wait_step`-style function + injected interval | Testable without a clock; the designers wait is the precedent |
| Path-segment encoding of a provider name | `format!` into a URL | `validate_segment` + `percent-encoding` as the resource routes do | Provider names are data; a `/` or `..` must not reshape the route |
| GAN trust between the two rig nodes | A Rust client for PKI/approval | The spike's `trust_gan()` shell recipe (file move) + one `PUT toggleApproval` via curl in `setup-pair.sh` | No API for the cert move; test fixture only (D-15) |
| Event/result writing | New files | `Steps::emit` + `result.json` single exit | One value-free log path (DSURF-03 canary already scans it) |
| Run-store / lock / snapshot | Anything | Phase 23 engine unchanged | The backup phase adds no master write |

**Key insight:** the phase is mostly *composition* of Phase 20-23 seams. The risk lives in gateway behavior (captures), not in missing Rust building blocks.

## Common Pitfalls

### Pitfall 1: `lastSyncTimestamp` may not advance on an incremental config sync (D-06)
**What goes wrong:** D-06 requires the backup's `lastSyncTimestamp` to advance past its pre-apply sample. If the gateway only bumps it on full syncs or on a timer, a healthy pair never "converges" and every apply ends `deploy_backup_unsynced` after 600 s (300 s wait, resync, 300 s).
**Why it happens:** the spike never isolated it. The S3 timestamps track restarts and failover, not individual config changes [VERIFIED: 20-LIVE-CAPTURES.md block C-S3: pair settled at `lastSyncTimestamp` 1791586091827/…875, after both restarts 1791586340823/…898, after failover 1791586358959/…359045]. Master `resync_settles_seconds=0` and `effective_change_sync_seconds=4` say content moved in about 4 s but say nothing about the timestamp.
**How to avoid:** capture C-P1: read the backup status before and after a master config change on a settled pair and record whether `lastSyncTimestamp` moves, and by when. Write the predicate as a parameter of one pure function so the capture can flip it (for example "advanced OR content-equal AND peerConnected AND !syncPending for two consecutive ticks").
**Warning signs:** a healthy rig run that waits the full 300 s and ends `content_differs=false, signals timestamp_not_advanced`.
**Decision flow:** if C-P1 shows no advance, raise it to the user before planning 24-03, because it contradicts a locked decision.

### Pitfall 2: Cutting the link by disconnecting gw-b from its only Docker network tests the wrong reason
**What goes wrong:** D-17 says "disconnect gw-b from the compose network or block its GAN port". `docker network disconnect` on the only network removes the container's route, so the published `127.0.0.1:<port>` REST port dies too; the verdict would be `unreachable`, not `not_syncing` (and `unreachable` is deliberately not live-gated).
**How to avoid:** cut the link with the Ignition API: `PUT /data/api/v1/gateway-network-incoming/toggleApproval/<remoteId>/false` on the **master** (the OpenAPI route exists; the 403 note says approval toggling is forbidden on the backup node) [VERIFIED: ~/.cache/ign-deploy-rig/openapi-8.3.6.json path `/data/api/v1/gateway-network-incoming/toggleApproval/{remoteId}/{approve}`, response `"403": "Approval status toggle forbidden on backup node."`]. Reconnect with `.../true`. Fallback: give the compose file two networks (`gan` for gw-a<->gw-b, default for the published ports) and disconnect only `gan`. Capture C-P5 decides which works and how fast `peerConnected` flips.

### Pitfall 3: A backup that loses its master may promote itself, tripping D-11
**What goes wrong:** D-11 refuses apply unless `active_node(master, backup) == OnlyA`. If the gate cuts the link and the backup reaches `pingMaxMissed` missed pings it can become `Active`; `active_node` then returns `Both` and the unsynced gate is refused before it starts.
**Why it happens:** redundancy config exposes `pingRate`, `pingTimeout`, `pingMaxMissed`, and `joinWaitTime` ("this node will become active regardless of node role") [VERIFIED: openapi-8.3.6.json `/data/api/v1/redundancy/config` description text]. The spike config sets `masterRecoveryMode: "Automatic"` and only sets host/port/ssl [VERIFIED: s3_redundancy.sh `cfg()`].
**How to avoid:** in `setup-pair.sh`, PUT a large `pingMaxMissed` (and keep the gate window short) so gw-b stays `Cold`. Capture C-P6 records the backup's `activityLevel` over time after the cut. Planner decision if it still flips: D-11 is scoped to refuse on `OnlyB` and `Neither` and to refuse on `Both` only when the master is not the sole writer; this needs the user's confirmation because it relaxes a locked decision (Assumption A3).

### Pitfall 4: Byte equality of the backup's mode collection is an assumption (D-03)
**What goes wrong:** the backup gateway writes the synced files itself. If it re-serializes `config.json` or re-stamps `resource.json` (`lastModification`), byte equality with the master fails on a perfectly synced pair.
**How to avoid:** capture C-P2: after a sync, `sha256` every file of one unit on both bind-mounted data dirs and diff. Master verify already proves post-scan bytes equal rendered bytes (CV3, STATE 22-09), so master live = render; the open question is only master live vs backup live. If they differ only in known members, add a normalization row (the Pitfall-6 rule of verify.rs: fix by adding a row, not by loosening equality).

### Pitfall 5: A `uptime: -1` / `Cold` backup is healthy
**What goes wrong:** a health check that wants `uptime > 0` or `Active` on the backup reports every good pair as broken.
**How to avoid:** the healthy-backup fixtures must be the verbatim S3 body (`"activityLevel":"Cold"`, `"uptime":-1`) [VERIFIED: redundancy.rs tests `pre_failover()`], and a test must assert `Converged` on it.

### Pitfall 6: D-14's secret preflight refuses a first deploy that creates the provider
**What goes wrong:** if the tree declares an `ignition/secret-provider/<P>` resource and also references `<P>`, the pre-apply listing on a gateway that has not got `<P>` yet is a 404, so the preflight blocks a legitimate deploy.
**How to avoid:** the preflight skips (as a `warn`/`skipped` detail) providers the render itself declares and that are not yet live; the full check runs again in verify/the backup phase after the apply. OpenAPI also defines 501 for a provider that does not support listing: treat as `unverifiable` (warn), not a fail (Assumption A4).

### Pitfall 7: New preflights break `rollback`
**What goes wrong:** `run_preflights` is shared by apply and rollback; rollback reads only the target file and has no rendered tree [VERIFIED: main.rs:3383-3388, rollback.rs:643]. `secrets` and `backup_read` cannot run there; `PreflightCheck::ALL` (7 ids) is pinned by `ids_are_the_contract_words_in_report_order` [VERIFIED: preflight.rs:597-612] and `run.json` records every id.
**How to avoid:** give `PreflightInput` optional inputs; with `None`, emit `Skipped` (never `Pass`). Keep `master_active` for both verbs (rollback also writes to the master). Update the pinned id list, the README check-id table and the contract tests in the same plan.

### Pitfall 8: Ctrl-C during the 300 s wait leaves no `result.json`
**What goes wrong:** `result.json` is written once at the single exit point [VERIFIED: apply/mod.rs:754]. A kill during the wait leaves `run.json`, `plan.json`, `events.ndjson` and an `applied` on-box record.
**How to avoid:** acceptable and safe (master is final), but emit `backup_wait_started` before the loop so the events show it; document in README that exit 130/killed after "applied" means "master applied, backup unproven: run `ign deploy verify --node backup`".

### Pitfall 9: `resync` response shape is documented two ways
**What goes wrong:** OpenAPI says 200 with `{"resync_state": string}`; PITFALLS and the spike say 200 with an empty body (`record … "$(api_status … resync)"` printed only `200`) [VERIFIED: openapi-8.3.6.json; s3.md `## resync` block `200`].
**How to avoid:** declare the endpoint `ResponseShape::EmptyOk` (body discarded) so either shape works, record the real body in captures, and never branch on it.

### Pitfall 10: The sync-exhausted state may only be visible in the backup log
**What goes wrong:** the verbatim line is `E [R.S.SyncManager] ... [config-resource] State sync request failed. Sync operation has failed 5 time(s), which exceeds the max attempt count. No further synchronization attempts will be made. redundant-provider=config-resource` [VERIFIED: spike results/RIG.md]. Log scraping has the CV1 pitfalls (logger filter is not a prefix filter, entries readable late).
**How to avoid:** try the structured routes first (`GET /redundancy/providers` -> `lastSyncError`, `status`; `GET /redundancy/events`) [VERIFIED: openapi-8.3.6.json response schemas: providers items have `name, friendlyName, pendingUpdates, queuedUpdatesPerSec, dispatchedUpdatesPerSec, lastPullTimestamp, lastPullDuration, lastApplyDuration, lastSyncError, status, hashcode`; events items have `timestamp, loggerName, level, message, marker, sourceLocation, properties`]. Fall back to `GatewayApi::logs` on the backup, matched client-side (`LogLine` withholds the message from `Debug`/serialization).

### Pitfall 11: Applies to a pair take up to 10 minutes on the unsynced path
**What goes wrong:** defaults give 300 s + resync + 300 s. Live gates and CLI tests must pass a tiny `--backup-timeout` (for example 20 s) or the suite crawls.
**How to avoid:** make poll interval an injected option (production 5 s) the way `DesignersPolicy.interval` is [VERIFIED: preflight.rs:160-169]; range-check `--backup-timeout` like `--designers-timeout` (`1..=86400`) [VERIFIED: cli.rs:762-768].

## Code Examples

### Existing outcome/exit pattern to extend (Decision A)
```rust
// Source: crates/ignition-core/src/deploy/apply/mod.rs:127-134, 148-157 (read this session)
/// The slug of the rolled-back apply verdict.
pub const ROLLED_BACK_SLUG: &str = "deploy_rolled_back";

pub enum OutcomeKind {
    /// The change is live and verified.
    Applied,
    /// The plan was empty; nothing was touched.
    Noop,
    /// The change was undone (23-17).
    RolledBack,
}
// Phase 24 adds: AppliedBackupUnsynced  (as_str -> "applied_backup_unsynced")
```

### Existing active-node API to wire
```rust
// Source: crates/ignition-core/src/client/redundancy.rs:145-173 (read this session)
pub enum ActiveNode { OnlyA, OnlyB, Both, Neither }
pub fn active_node(a: &RedundancyStatusWire, b: &RedundancyStatusWire) -> ActiveNode
// decides by activityLevel == "Active" alone; never reads role
```

### A new endpoint row (table drives shape enforcement)
```rust
// Source: pattern from crates/ignition-core/src/client/deploy_api.rs:85-184
EndpointDecl { id: "redundancy_resync", method: "POST",
    path_template: "/data/api/v1/redundancy/gwaction/resync",
    shape: ResponseShape::EmptyOk, capture_ref: "C-P3" },
EndpointDecl { id: "secret_names", method: "GET",
    path_template: "/data/api/v1/secret-providers/{provider}/secrets",
    shape: ResponseShape::Json, capture_ref: "C-P7" },
```
Adding two rows changes the pinned count `14` and the id list in three tests (`wire_matches_the_endpoint_table` expected table, `every_method_id_is_declared`, `deploy_endpoints_cover_every_method`) [VERIFIED: deploy_api.rs:1167, 1206, 1226]. The `{provider}` placeholder needs the same template handling as `{module}/{type}/{name}`. 404 = provider missing, 501 = cannot list, per the OpenAPI response table [VERIFIED: openapi-8.3.6.json `/data/api/v1/secret-providers/{provider-name}/secrets`]. 200 body `{"secrets": ["…"]}` is live-captured for a file provider [VERIFIED: 21-LIVE-CAPTURES.md `{"secrets":["cap21-secret"]}`]; the **internal** `default` provider listing is not yet captured (C-P7).

### Pure convergence decision (shape)
```rust
// Source: new, modeled on preflight.rs wait_step (read this session)
pub struct BackupSignals { pub peer_connected: bool, pub sync_pending: bool,
    pub last_sync_before: Option<i64>, pub last_sync_now: Option<i64>, pub mode: Option<String> }
pub enum Reason { Unreachable, NotSyncing, ContentDiffers, SyncExhausted }   // snake_case on the wire (D-09)
pub enum WaitStep { Converged, Exhausted, TimedOut, Sleep(Duration) }
pub fn wait_step(elapsed: Duration, budget: Duration, interval: Duration, state: &TickState) -> WaitStep
```

### The pair rig recipe (from the spike, to be adapted)
```bash
# Source: whk-environment-orchestration branch docs/prod-gateway-config-as-code,
#   spikes/gateway-modes/s3_redundancy.sh (read this session)
trust_gan() { for c in gw-a gw-b; do docker compose exec -u root $c sh -c '
  B=/usr/local/bin/ignition/data/config/local/ignition/gateway-network
  for side in server client; do R=$B/$side/security/pki/rejected; T=$B/$side/security/pki/trusted/certs
    mkdir -p "$T"; for f in "$R"/*; do [ -e "$f" ] && mv "$f" "$T/"; done; chown -R 2003:2003 "$T"; done'; done; }
# approve: PUT /data/api/v1/gateway-network-incoming/toggleApproval/<url-encoded connectionId>/true  (on gw-a)
# role:    PUT /data/api/v1/redundancy/config  role=Master|Backup, gatewayNetworkSetup.{host:"gw-a",port:8060,enableSsl:true}
# then restart both and loop trust_gan + approve_pending until gw-b peerConnected=true
```
Settle predicate from the spike: `peerConnected && !syncPending && projectState == "Good"` on both nodes.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| ARCHITECTURE.md slugs `backup_out_of_sync`, `applied_backup_stale`, PITFALLS `deploy_backup_lagging` | one slug `deploy_backup_unsynced` + `reason` | CONTEXT D-08/D-09 (SUMMARY.md already folded it) | Do not reintroduce the older names |
| Backup checked by status flags | Backup checked by its own content (REST + file hashes) | PITFALLS Pitfall 9 | A 200 or a timestamp alone is never proof |
| `redundancy.rs` role strings `Primary`/`Backup` | live `Master`/`Backup` | Phase 20 | Match `Master`; decide active by `activityLevel` |
| Failover scripted in verbs | `failover` library method may exist, no verb calls it | SF-3 | Not part of this phase's deliverables |

**Deprecated/outdated:** `plan_id` / `--expect-plan` naming (use `plan_sha256`); `deploy_backup_lagging`.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The backup's `lastSyncTimestamp` advances after an incremental config sync | Pitfall 1 / D-06 | Every healthy apply ends unsynced after 600 s; D-06 needs rewording (user decision) |
| A2 | The backup's on-disk mode collection bytes equal the master's after sync | Pitfall 4 / D-03 | False `content_differs`; needs normalization rows or a looser D-03 |
| A3 | A backup whose link is cut with a large `pingMaxMissed` stays `Cold` for the gate window | Pitfall 3 | D-11 refuses the unsynced gate; D-11/D-17 must be reconciled with the user |
| A4 | A secret provider that answers 501 should be a warning, not a failure | Pitfall 6 | Either false red on external-vault providers (if fail) or a silent gap (if warn); needs user confirmation |
| A5 | `toggleApproval/.../false` on the master drops an established GAN link promptly while both REST APIs stay up | Pitfall 2 | Gate cannot use it; fall back to the two-network compose layout |
| A6 | `GET /redundancy/providers` or `/events` carry a usable sync-exhausted signal | Pitfall 10 | Detection falls back to log scraping (CV1 caveats) |
| A7 | After the resync, a `sync_exhausted` backup still gets a full second budget (D-10 wording is ambiguous: "still sends the one resync, then reports") | D-10 | If C12 shows exhausted is not recoverable by resync, the second wait wastes 300 s; shorten to a short re-check |
| A8 | `projectState == "Good"` should be part of "redundancy healthy" (the spike's `settled()` uses it; D-06 omits it) | Open Question 4 | Either misses a project-sync failure (omit) or false-reds a transient state (include) |
| A9 | Ports 18300/18301 and compose project `ign-pair-rig` are free for the pair fixture | Environment | Trivial: pick others |
| A10 | The gateway API token minted for the master can be reused on the backup (spike reused one key on both nodes) | Pair rig | Setup script must mint per node |

## Open Questions

1. **Does the backup's `lastSyncTimestamp` move on a config-only change? (A1)**
   - Known: moves across restarts/failover (S3); content syncs in about 4 s.
   - Unclear: incremental behavior.
   - Recommendation: capture C-P1 in 24-01 before any predicate code; if not, bring D-06 back to the user.
2. **Is backup disk byte-equal to master disk? (A2)**
   - Recommendation: capture C-P2 with both data dirs bind-mounted (D-16 already requires gw-b's bind mount); diff per file.
3. **What is the best sync-exhausted signal and does resync alone clear it? (C12, A6, A7)**
   - Known: verbatim log line; `providers`/`events` routes exist in OpenAPI.
   - Unclear: reachability of the exhausted state on the rig. The spike reached it by wrong-order mode activation ("did not match remote system snapshot" x5, RIG.md). Recommendation: reproduce that order in 24-01 (start gw-b with the mode JVM arg before the mode exists) and capture `providers`, `events`, log and a single `resync` result; if unreachable on the rig, record "unreachable" and ship detection as unit-tested only (D-18 already allows this: "if it can be reached").
4. **Is `projectState` part of healthy? (A8)** Capture its value on the backup during sync and after the cut.
5. **How does `ign redundancy status` get the second node?** `active_node(a, b)` needs both statuses; one profile yields one. Recommendation: add `--peer <PROFILE>` (second named profile, resolved with `Session::resolve_side` like the diff/sync two-side shape [VERIFIED: main.rs:3935-3969]); output stays `{status: …}` plus an additive `active_node` object (`null` without `--peer`). Alternative: `--env` reading both profiles from the target. Planner decides; both are additive.
6. **Verify `--node` envelope shape.** Recommendation: keep `checks` (the six master checks) untouched for `--node master` so today's output is byte-identical; add `backup` (null for `master`). For `backup`/`both`, a red backup uses the existing verdict slug `deploy_verify_failed` (verify does not wait or resync, D-12), while `deploy_backup_unsynced` stays apply's slug. `data.node` carries the `--node` value; today it is always `"master"` and `contract_deploy.rs` pins it [VERIFIED: contract_deploy.rs:1388 `assert_eq!(data["node"], "master")`].
7. **Should a missing backup identity check exist (backup `gateway-info` name == master's)?** Optional hardening, not in the requirements; skip unless 24-01 shows the backup answers with a different name.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Docker + Compose | pair rig, live gates | yes (OrbStack context) | Docker 29.8.2, Compose v5.5.0 | none (live gates skip quietly without `IGNITION_LIVE_PAIR=1`) |
| `inductiveautomation/ignition:8.3.6` image | pair rig | yes, locally present | 8.3.6 (amd64, emulated on arm64) | pull |
| Rust toolchain | build/tests | yes | cargo/rustc 1.94.1 | none |
| `jq`, `curl`, `openssl` | setup/capture scripts | yes | present on PATH | none |
| Free host ports | pair rig | 18300, 18301 appear unused (a stopped `ign-deploy-rig-gw-1` container exists on 18290) | -- | choose others; use compose project name `ign-pair-rig`, not `ign-deploy-rig` |
| Local OpenAPI doc | route verification | yes, `~/.cache/ign-deploy-rig/openapi-8.3.6.json` | 8.3.6 | -- |
| Existing token dir | tokens | `~/.cache/ign-deploy-rig/tokens.env` exists for the single rig | -- | pair script writes its own `~/.cache/ign-pair-rig/` (0700, outside the repo, as `mint-token.sh` does) |

**Missing dependencies with no fallback:** none.
**Missing dependencies with fallback:** none. Note the pair needs a long first settle (the spike allowed 600 s) on an emulated amd64 image; budget the 24-01 capture window accordingly.

## Validation Architecture

> `workflow.nyquist_validation` is `true` in `.planning/config.json` [VERIFIED: config.json], so this section is required.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust `cargo test` (libtest + `tokio::test`), `wiremock`, `assert_cmd`-style CLI tests in `ignition-cli/tests`, `proptest` present |
| Config file | workspace `Cargo.toml`; no separate runner config |
| Quick run command | `cargo test -p ignition-core --lib deploy::backup deploy::apply::preflight client::deploy_api` |
| Full suite command | `cargo test --workspace` (plus the live gate commands below, never in CI) |
| Live gate | `IGNITION_LIVE_PAIR=1 … cargo test -p ignition-core --test live_deploy_pair -- --ignored --nocapture --test-threads=1` (mirrors `live_deploy_apply.rs` header) |

### Phase Requirements -> Test Map

| Req / SC | Behavior | Test Type | Automated Command | File Exists? |
|----------|----------|-----------|-------------------|-------------|
| DPAIR-01 / SC1 | Converged predicate: Cold + `uptime:-1` backup counts healthy; needs ts advanced, `peerConnected`, `!syncPending`, mode equal | unit (pure) | `cargo test -p ignition-core --lib deploy::backup` | Wave 0 (new file) |
| DPAIR-01 / SC1 | Two-tier poll, budget boundary inclusive, interval clamp, early-exit on sync-exhausted | unit (pure) | same | Wave 0 |
| DPAIR-01 / SC1 | `redundancy_resync` is POST to the master only; `secret_names` shapes (200, 404, 501, 401/403 Jetty HTML); path-encodes provider | wiremock contract | `cargo test -p ignition-core --test deploy_client_contract` | extend existing |
| DPAIR-01 / SC1 | Engine: converge on first tick; converge after resync; **zero requests to the backup other than GET** | engine (FakeGateway pair + `ForbidWrites`) | `cargo test -p ignition-core --test deploy_apply backup_` | extend existing |
| DPAIR-01 | `endpoint table` count/ids updated, `wire_matches_the_endpoint_table` | unit | `cargo test -p ignition-core --lib client::deploy_api` | extend existing |
| DPAIR-01 / D-02, D-14 | Preflights `backup_read`, `secrets`, `master_active` fail before any write; `Skipped` (never `Pass`) when not applicable; rollback skips declared-tree checks | engine | `cargo test -p ignition-core --test deploy_apply preflight_` | extend existing |
| DPAIR-02 / SC2 | Unsynced outcome: `ok=false`, slug, `backup.reason` for each of 4 reasons, master record stays `applied`, no `restore_started` event, no second restore | engine | `cargo test -p ignition-core --test deploy_apply backup_unsynced` | Wave 0 cases |
| DPAIR-02 / SC2 | CLI: envelope printed then exit 6; `data.slug`, `data.backup.reason`; `deploy_backup_unsynced` absent from `DEPLOY_SLUGS` and the exit table | CLI contract | `cargo test -p ignition-cli --test contract_deploy backup_` and `--test deploy_family_coverage` | extend existing |
| DPAIR-02 | README dated exception text pinned (mirror `rolled_back_verdict_exception_is_dated_and_documented`) | doc test | `cargo test -p ignition-cli --test deploy_family_coverage` | extend existing |
| DPAIR-03 / SC3 | Reference collector: only well-formed Referenced objects, names only, dedup, grouped by provider | unit | `cargo test -p ignition-core --lib deploy::lint` | extend existing |
| DPAIR-03 / SC3 | `verify --node master` output byte-identical to today (key set pin); `--node both` red when a name is missing on either node; exit 6, slug `deploy_verify_failed` | CLI contract | `cargo test -p ignition-cli --test contract_deploy verify_cli_` | extend existing |
| D-04 | REST-only backup: `files` check is a visible `warn` in human and `--json`, run still passes | engine + CLI | `cargo test -p ignition-core --test deploy_apply backup_rest_only` | Wave 0 |
| D-11 / DAPI-07 | `ign redundancy status` shows active node; apply refuses `OnlyB`, `Neither`, `Both` per decision | CLI + engine | `cargo test -p ignition-cli --test contract_diagnostics redundancy_` | extend existing |
| DSURF-01 | Coverage test stays green with `--node` flag (no new leaf, so existing rows suffice) and new README rows | doc test | `cargo test -p ignition-cli --test deploy_family_coverage` | exists |
| DSURF-03 | Canary: secret value seeded in backup resources, backup log, redundancy events, and secret listing never appears in output, JSON, run dir, events (extend `CANARY_*` scenario with a backup fake) | CLI contract | `cargo test -p ignition-cli --test contract_deploy canary` | extend existing (`contract_deploy.rs` ~2546-3096) and `crates/ignition-core/tests/deploy_apply.rs` ~4506-4684 |
| SC1 live | pair converges after apply; no non-GET to gw-b (audit log or counting proxy) | live | `IGNITION_LIVE_PAIR=1 cargo test … live_deploy_pair live_pair_converges` | Wave 0 (needs rig) |
| SC2 live | link cut (`toggleApproval` false), apply ends `deploy_backup_unsynced`/`not_syncing`, master not rolled back, then reconnect and re-converge | live | `… live_pair_unsynced_then_reconverge` | Wave 0 |
| SC3 live | secret name missing on one node -> `verify --node both` red; present on both -> green | live | `… live_pair_secret_names` | Wave 0 |
| C12 | resync alone vs sync-exhausted recorded (or recorded unreachable) | capture | `bash capture24.sh c12` | Wave 0 |

### Sampling Rate
- **Per task commit:** the single quick command above for the module touched (under 30 s once built).
- **Per wave merge:** `cargo test -p ignition-core` and `cargo test -p ignition-cli --test contract_deploy --test deploy_family_coverage`.
- **Phase gate:** `cargo test --workspace` green, clippy clean (`-D warnings` as the workspace does), then the live pair gate (manual, never CI), before `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] `crates/ignition-core/src/deploy/backup.rs` pure evaluator and its unit tests
- [ ] `FakeGateway` pair model in `tests/common/fake_deploy.rs`: per-node redundancy status script, resync counter, secret listings, a "link" switch that makes the backup apply the master's files on resync/after N ticks, and `with_data_dir` for a separate backup temp dir
- [ ] `ForbidWrites` extended: `redundancy_resync` refuses, `secret_names` forwards (and the new count/ids pinned)
- [ ] `FaultyDeploy` (tests/common/faulty_deploy.rs) and `LockGateway` (hold.rs test double) gain the two new `DeployApi` methods (these are the only `DeployApi` implementors besides `ReqwestGatewayApi`, `FakeDeploy`, `ForbidWrites`) [VERIFIED by grep this session]
- [ ] `tests/fixtures/live-rig-pair/` (compose, setup, token mint, `capture24.sh`) and `live_deploy_pair.rs`
- [ ] Captures document `24-LIVE-CAPTURES.md` with blocks C-P1..C-P8 (below) before plans 24-02+

**Captures to record in 24-01 (name them so later plans can cite them):**
- C-P1 backup `lastSyncTimestamp` before/after a master config change (decides D-06)
- C-P2 per-file sha256 of one unit on both data dirs (decides D-03)
- C-P3 resync: status, body, effect, time-to-settle, from the master
- C-P4 redundancy status of both nodes: settled, during sync, after the cut, after reconnect
- C-P5 `toggleApproval` false/true and the two-network fallback: `peerConnected` timing, REST reachability
- C-P6 backup `activityLevel` over 120 s after the cut with the tuned `pingMaxMissed` (A3)
- C-P7 secret listings on both nodes: `default` internal provider, a file provider, unknown provider (404), 501 candidate, token without permission
- C-P8 sync-exhausted: `providers`, `events`, log lines; C12 resync result (or "not reachable on the rig")

## Security Domain

> `security_enforcement` is `true`, `security_asvs_level` 1 [VERIFIED: config.json].

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes (backup profile credentials) | Existing secret chain via `Session::resolve_side`; token in `X-Ignition-API-Token` through the single `apply_auth` site; never printed |
| V3 Session Management | no (stateless token) | -- |
| V4 Access Control | yes | The backup is read-only by type; the one new write (resync) is sent to the master only; the deploy token's level is the gateway's gate |
| V5 Input Validation | yes | `validate_segment` + percent-encoding for provider names; `--backup-timeout` range-checked; gateway-supplied strings pass `printable()` before entering findings |
| V6 Cryptography | no new | No new crypto; sha256 via existing `tree::sha256_hex` |
| V7 Error handling / logging | yes | Value-free events and results (DSURF-03); log/redundancy-event messages matched, never serialized |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Accidental write to the backup (a "fix it" path creeps in) | Tampering | Narrow read-only backup type; no `ApplyHost`/`TreeMirror` for `nodes.backup`; source-scan pin; `ForbidWrites` in engine tests; live gate asserts no non-GET on gw-b (audit log: "POST, PUT and DELETE are audit-logged" [CITED: spec §3 table, Auth row]) |
| Secret value leakage via secret listing / backup logs / redundancy events | Information disclosure | Names only (`{"secrets":[…]}` carries `x-ignition-non-secret: true` [VERIFIED: openapi-8.3.6.json]); `LogLine` withholds messages; extend the canary to the backup surfaces |
| Provider name path injection (`../`, `?`) into the secrets URL | Tampering | `validate_segment` + encode; refuse before any request |
| Writing to the node that became active after a failover | Tampering / DoS | D-11 `master_active` preflight (activity level) in apply and rollback |
| Spoofed or stale "converged" (200 alone, timestamp alone) | Spoofing / Repudiation | Content-based proof (REST view + file hashes), pre-apply sample from the same node |
| Hostile file layout on the backup host | Tampering | The hardened read half already fails closed (symlink, special file, size, entry caps) |
| Rig credentials | Information disclosure | Localhost-only rig, token files 0600 in a 0700 dir outside the repo; refuse a state dir inside the repo (copy `mint-token.sh` guard); `assert_local` on every script and the loopback URL guard in the live test |

## Sources

### Primary (HIGH confidence, read this session)
- `/Users/pmannion/whiskeyhouse/ignition-cli/.planning/phases/24-redundant-pair/24-CONTEXT.md` and `24-DISCUSSION-LOG.md`
- `crates/ignition-core/src/client/redundancy.rs`, `actions/redundancy.rs`, `client/deploy_api.rs` (trait, endpoint table, tests), `deploy/target.rs`, `deploy/transport/mod.rs`, `deploy/verify.rs`, `deploy/lint.rs`, `deploy/apply/mod.rs`, `deploy/apply/preflight.rs`, `actions/deploy.rs` (verify), `actions/deploy_apply.rs`, `ignition-cli/src/main.rs` (apply/verify/rollback dispatch, exit mapping), `cli.rs`, `error.rs` (`DEPLOY_SLUGS`), `README.md` (exit table, dated exceptions)
- Tests: `tests/common/{forbid_writes,fake_deploy}.rs`, `tests/live_deploy_apply.rs` (gate pattern), `ignition-cli/tests/{deploy_family_coverage,contract_deploy,contract_diagnostics}.rs`
- `.planning/{ROADMAP,REQUIREMENTS,STATE}.md`, `.planning/research/{FEATURES §3.5/§4.4,PITFALLS 9/10/23/27/31/32/C12,ARCHITECTURE §5,§7,SUMMARY}.md`, `20-LIVE-CAPTURES.md` block C-S3, `20-config-api-client/COVERAGE.md`, `21-LIVE-CAPTURES.md` secret listing
- Local 8.3.6 OpenAPI: `~/.cache/ign-deploy-rig/openapi-8.3.6.json` (routes, schemas, response codes for `redundancy`, `redundancy/providers`, `redundancy/events`, `redundancy/gwaction/resync`, `redundancy/config`, `secret-providers/{provider-name}/secrets`, `gateway-network-incoming/toggleApproval`)
- Spike on branch `docs/prod-gateway-config-as-code` of `whk-environment-orchestration`: `spikes/gateway-modes/{docker-compose.yml,s3_redundancy.sh,mint-token.sh,lib.sh,results/{S3,RIG,SUMMARY}.md}`; spec §3, §6 step 5, §8, §9
- Environment probes: `docker`, `cargo`, `jq`, `curl`, `openssl`, image list, port scan

### Secondary (MEDIUM confidence)
- None used.

### Tertiary (LOW confidence)
- Behavioral claims about Ignition redundancy not seen in a capture (backup self-promotion on lost master, `lastSyncTimestamp` semantics, byte equality of synced files): logged as A1-A6 and gated by plan 24-01 captures. No web research was needed; the authoritative sources are the repo, the spike, and the cached 8.3.6 OpenAPI.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH, no new dependencies, every seam read.
- Architecture: HIGH for placement and reuse; MEDIUM for the predicate details until captures land.
- Pitfalls: HIGH for code-derived ones (preflight pins, endpoint-count pins, rollback sharing); MEDIUM for gateway-behavior ones (A1-A6).

**Research date:** 2026-10-19
**Valid until:** 2026-11-18 for the code seams (the repo is moving; re-grep the line numbers before editing); the gateway-behavior items stay open until `24-LIVE-CAPTURES.md` exists.
