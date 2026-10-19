# Phase 24: Redundant Pair - Pattern Map

**Mapped:** 2026-10-19
**Files analyzed:** 22 new/modified
**Analogs found:** 21 / 22 (line numbers are from a read on 2026-10-19; re-grep before editing, the repo moves)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `crates/ignition-core/src/deploy/backup.rs` (new) | service (pure evaluator) | transform / poll decision | `deploy/apply/preflight.rs` `wait_step` (L511-535) + `deploy/verify.rs` `evaluate` | role-match |
| `crates/ignition-core/src/client/deploy_api.rs` (+`redundancy_resync`, `secret_names`) | client | request-response | same file, `EndpointDecl` rows (L85-184) | exact |
| `crates/ignition-core/src/client/redundancy.rs` (optional providers wire) | model | request-response | same file (`RedundancyStatusWire`, `active_node` L145-173) | exact |
| `crates/ignition-core/src/deploy/lint.rs` (+`referenced_secret_names`) | utility | transform | same file `is_referenced` (L42-54), `scan_secrets` | exact |
| `crates/ignition-core/src/deploy/apply/mod.rs` (new `OutcomeKind`, `BACKUP_UNSYNCED_SLUG`, backup phase in `drive`, exit point) | service (engine) | event-driven / batch | same file: `ROLLED_BACK_SLUG` L134, `OutcomeKind` L150-167, `drive` ~L1167-1225, `run_apply` L731-765 | exact |
| `crates/ignition-core/src/deploy/apply/preflight.rs` (+`MasterActive`, `Secrets`, `BackupRead`) | service | request-response | same file `mode_finding`, `PreflightCheck::ALL`, ids test L597-612 | exact |
| `crates/ignition-core/src/actions/deploy_apply.rs` (`ApplySurfaces`, `ApplyRunOptions`, `failure_exit_code`, `base_result`) | action | request-response | same file L112-131 | exact |
| `crates/ignition-core/src/actions/deploy.rs` (`deploy_verify` node-aware, `gather_read_back` to `pub(crate)`) | action | request-response | same file `deploy_verify` ~L982 | exact |
| `crates/ignition-core/src/actions/redundancy.rs` (+`active_node`) | action | request-response | same file (whole, 29 lines) | exact |
| `crates/ignition-core/src/deploy/transport/mod.rs` (backup read-only handle) | utility | file-I/O | same file `for_node` L279-293 | exact |
| `crates/ignition-cli/src/cli.rs` (`--backup-timeout`, `--node`, `--peer`) | config | request-response | `--designers-timeout` L760-768 | exact |
| `crates/ignition-cli/src/main.rs` (backup reader wiring, exit 6, redundancy status) | controller | request-response | L590-599, L3343-3358, L3549-3556 | exact |
| `crates/ignition-cli/src/render.rs` (backup block, D-04 warn row) | component | transform | existing `StderrApplyObserver` / apply render arms | role-match |
| `README.md` (dated verdict exception, check-id table, `--node`) | docs | n/a | README L102-117, L131-157 | exact |
| `crates/ignition-core/tests/common/fake_deploy.rs` (pair model) | test | event-driven | same file | exact |
| `crates/ignition-core/tests/common/forbid_writes.rs`, `faulty_deploy.rs`, hold.rs `LockGateway` | test | request-response | `forbid_writes.rs` L1-50 | exact |
| `crates/ignition-core/tests/deploy_apply.rs` (backup_*) | test | request-response | same file ~L4506-4684 canary | exact |
| `crates/ignition-cli/tests/{contract_deploy,deploy_family_coverage,contract_diagnostics}.rs` | test | request-response | same files | exact |
| `crates/ignition-core/tests/live_deploy_pair.rs` (new) | test (live gate) | request-response | `tests/live_deploy_apply.rs` L1-50 header + guards | exact |
| `crates/ignition-core/tests/fixtures/live-rig-pair/{compose.yml,setup-pair.sh,mint-token-pair.sh,capture24.sh}` (new) | config / fixture | batch | `tests/fixtures/live-rig-deploy/{compose.yml,mint-token.sh,capture23.sh}` + spike `s3_redundancy.sh` | role-match |
| `.planning/phases/24-redundant-pair/24-LIVE-CAPTURES.md` (new) | doc | n/a | `23-LIVE-CAPTURES.md`, `20-LIVE-CAPTURES.md` | exact |

## Pattern Assignments

### `deploy/apply/mod.rs` (engine, new outcome kind and backup phase)

**Analog:** itself.

**Slug constant** (L127-134), Decision A, add `BACKUP_UNSYNCED_SLUG` beside it:
```rust
pub const ROLLED_BACK_SLUG: &str = "deploy_rolled_back";
```

**OutcomeKind** (L148-167): add `AppliedBackupUnsynced` and `as_str` -> `"applied_backup_unsynced"`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeKind { Applied, Noop, RolledBack }
impl OutcomeKind { pub fn as_str(self) -> &'static str { match self {
    OutcomeKind::Applied => "applied", OutcomeKind::Noop => "noop", OutcomeKind::RolledBack => "rolled_back" } } }
```

**Insertion point** (`drive`, after `fetch_snapshot`, before `Ok(ApplyOutcome {...})`, ~L1189-1223). Lock already released by the scan. `post` (master post fingerprint) is the expected master live hash set (Pattern 7). Build `kind` from backup verdict instead of the literal `OutcomeKind::Applied`. Keep warnings via `warnings.push(format!("... ({})", e.code()))`. Emit events with `steps.emit(ApplyEvent::...)`; a failed emit is a warning (`EVENTS_UNWRITABLE`), never an error.

**Single exit** (`run_apply`, L731-765). Replace the `ok`/`slug` logic:
```rust
let mut doc = serde_json::to_value(outcome).unwrap_or(Value::Null);
doc["ok"] = Value::Bool(outcome.kind != OutcomeKind::RolledBack);
if outcome.kind == OutcomeKind::RolledBack {
    doc["slug"] = Value::String(ROLLED_BACK_SLUG.to_string());
}
```
New rule: `ok = !matches!(kind, RolledBack | AppliedBackupUnsynced)`; slug by kind; `cause` stays null; reason lives in `backup.reason`.

### `deploy/apply/preflight.rs` (new checks `master_active`, `secrets`, `backup_read`)

**Analog:** itself, `mode_backup` finding (L264-271): skip with a detail when not applicable.
```rust
findings.push(if input.target.nodes.backup.is_some() {
    mode_finding(PreflightCheck::ModeBackup, "backup", input.backup, mode).await?
} else {
    PreflightFinding { check: PreflightCheck::ModeBackup, status: FindingStatus::Skipped,
        detail: "the target has no backup node".to_string() }
});
```
Rules to copy: `Skipped` never `Pass` when inputs absent (rollback has no render); update the pinned list in `ids_are_the_contract_words_in_report_order` (L597-612; currently `mode_master, mode_backup, wrong_target, write_rest, write_host, designers, hold_budget`), the README check-id table, and `run.json` expectations in the same plan.

**Injected-clock poll step** (L511-535), the model for `backup::wait_step`:
```rust
pub(crate) fn wait_step(open: usize, elapsed: Duration, policy: &DesignersPolicy) -> WaitStep {
    if open == 0 { WaitStep::Clear }
    else if elapsed >= policy.timeout { WaitStep::TimedOut }
    else { WaitStep::Sleep(policy.interval.max(Duration::from_millis(1)).min(policy.timeout - elapsed)) }
}
```
Poll interval is an injected option (like `DesignersPolicy.interval`, L160-169) so tests use tiny values.

### `deploy/backup.rs` (new, pure)

**Analog:** `wait_step` above plus `verify::evaluate` / `Expected::from_render` (verify.rs L301, 445-496). Reuse `evaluate` for the backup `mode` and `read_back` checks via `ObservedState` built from backup REST reads (make `gather_read_back` in `actions/deploy.rs` `pub(crate)`). Serde reason enum, snake_case on the wire (D-09): `unreachable | not_syncing | content_differs | sync_exhausted`. File compare uses `tree::sha256_hex`, report paths and hashes only. Healthy fixture must be the verbatim S3 body (`activityLevel:"Cold"`, `uptime:-1`); see `redundancy.rs` tests `pre_failover()`.

### `client/deploy_api.rs` (two new endpoint rows and trait methods)

**Analog:** itself, `EndpointDecl` table (L85-184):
```rust
EndpointDecl { id: "resource_create", method: "POST",
    path_template: "/data/api/v1/resources/{module}/{type}",
    shape: ResponseShape::Json, capture_ref: "C3a" },
```
Add (shape per research; resync is `EmptyOk`, response documented two ways):
```rust
EndpointDecl { id: "redundancy_resync", method: "POST",
    path_template: "/data/api/v1/redundancy/gwaction/resync",
    shape: ResponseShape::EmptyOk, capture_ref: "C-P3" },
EndpointDecl { id: "secret_names", method: "GET",
    path_template: "/data/api/v1/secret-providers/{provider}/secrets",
    shape: ResponseShape::Json, capture_ref: "C-P7" },
```
Pinned tests to update: count 14 -> 16 and id lists (`wire_matches_the_endpoint_table` ~L1167, `every_method_id_is_declared` ~L1206, `deploy_endpoints_cover_every_method` ~L1226). Provider segment: `validate_segment` + percent-encoding like the resource routes. Implementors that must gain both methods: `ReqwestGatewayApi`, `FakeDeploy`, `ForbidWrites`, `FaultyDeploy`, hold.rs `LockGateway`.

### `deploy/lint.rs` (`referenced_secret_names`)

**Analog:** `is_referenced` (L42-54), private; add a `pub fn` beside it that walks `Node::Obj`/arrays and, when `is_referenced(node)`, reads `data.providerName` and `data.secretName` into a `BTreeSet<(String,String)>`. Names only (ENT-3115).
```rust
fn is_referenced(node: &Node) -> bool {
    let Node::Obj(members) = node else { return false; };
    if members.len() != 2 || !matches!(node.get("type"), Some(Node::Str(t)) if t == "Referenced") { return false; }
    ...
```

### `actions/deploy_apply.rs`

**Analog:** itself L112-131. Extend both functions with the new kind:
```rust
pub fn failure_exit_code(&self) -> Option<u8> {
    (self.outcome == OutcomeKind::RolledBack).then_some(6)
}
...
ok: outcome != OutcomeKind::RolledBack,
slug: (outcome == OutcomeKind::RolledBack).then_some(ROLLED_BACK_SLUG),
```
Add an additive `backup` section to `DeployApplyResult`. `ApplySurfaces` already carries `backup_deploy: Option<&dyn DeployApi>` (type-level hole: it can express writes). Add a narrow read-only backup struct (private fields, read methods only) and a backup `TreeReader` instead of passing `&dyn DeployApi` to new code.

### `transport/mod.rs` (backup read handle)

**Analog:** `for_node` (L279-293), already returns the read half only:
```rust
pub fn for_node(repo: &Path, target_file: &str, label: &str, node: &NodeTarget)
    -> Result<Box<dyn TreeReader>, CoreError> {
    Ok(match node_access(repo, target_file, label, node)? {
        NodeAccess::Local(data_dir) => Box::new(local::LocalFsTransport::new(data_dir, label)),
        NodeAccess::Ssh { shell, data_dir, windows } => Box::new(sftp_transport(*shell, data_dir, label, windows)),
    })
}
```
Call with label `"nodes.backup"`. A node with neither `ssh` nor `local` is refused (`deploy_tree_invalid`, L250-255), so the CLI must pass `None` for a REST-only backup (D-04 warning check). Add `"nodes.backup"` path to the `write_half_unreachable` source scan (L21-23, 298-302): new backup code must not mention `mirror_for_node`, `apply_host_for_node`, `ApplyHost`, `TreeMirror`.

### `actions/redundancy.rs` (+`active_node`)

**Analog:** itself (whole file). Current shape `RedundancyStatusResult { status: RedundancyStatusWire }`; add `active_node: Option<...>` (null without `--peer`), additive only. Library to call, `client/redundancy.rs` L145-173:
```rust
pub fn active_node(a: &RedundancyStatusWire, b: &RedundancyStatusWire) -> ActiveNode
// decides by activity_level == ACTIVITY_ACTIVE alone, never role
```
Second node via `--peer <PROFILE>` resolved like the diff/sync two-side shape (`Session::resolve_side`, main.rs ~L3935-3969).

### `ignition-cli/src/cli.rs` and `main.rs`

**Flag analog** (cli.rs L760-768):
```rust
/// Seconds `--designers wait` polls before it refuses
#[arg(long, value_name = "SECS", default_value_t = 600,
      value_parser = clap::value_parser!(u64).range(1..=86400))]
designers_timeout: u64,
```
Use the same for `--backup-timeout` (default 300). `--node master|backup|both` on `deploy verify` as a clap `ValueEnum`, default `master` (today's output byte-identical; `data.node` pinned `"master"` at contract_deploy.rs:1388).

**Exit mapping** (main.rs L590-599): the envelope renders first, then `if let ActionOutput::DeployApply(apply) = &out && let Some(code) = apply.failure_exit_code() { return ExitCode::from(code); }`. No change needed beyond `failure_exit_code`.

**Backup wiring** (main.rs L3343-3358): backup REST session via `named_profile_client(&mut config, &backup.profile)`, passed as `backup_deploy`. Add the backup `TreeReader` here from `transport::for_node(..., "nodes.backup", backup)` only when `ssh`/`local` is declared.

**Redundancy dispatch** (main.rs L3549-3556): `Commands::Redundancy(... Status)` with `Session::resolve(cli.profile.as_deref())`; add `peer: Option<String>` and a second `Session::resolve_side`.

### `tests/live_deploy_pair.rs` (live gate)

**Analog:** `tests/live_deploy_apply.rs` header (L1-50): env switch quiet-skip, loopback-only URL guard, gateway-name guard, process-wide lock because tests share the gateways, cleanup even on assertion failure, output carries counts/run ids/timings only, never token or bytes. Copy structure with `IGNITION_LIVE_PAIR=1`, `IGNITION_LIVE_URL` (master), a backup URL var, tokens per node (A10), and `IGN_PAIR_RIG_DATA_A/B` for the bind mounts. Pass a tiny `--backup-timeout` (e.g. 20 s) in every gate.

### `tests/common/forbid_writes.rs` and fakes

**Analog:** `ForbidWrites<T>` (forbid_writes.rs L1-50): forwards reads, `refuse("method")` panics on writes. `redundancy_resync` -> `refuse`; `secret_names` -> forward. FakeGateway pair model: per-node redundancy status script, resync counter, secret lists, link switch making the backup adopt master files on resync or after N ticks, separate backup data dir. Backup-side assertion: zero non-GET requests.

### `tests/fixtures/live-rig-pair/`

**Analog:** `live-rig-deploy/compose.yml` (project name header, `platform: linux/amd64`, `127.0.0.1:<port>:8088`, env `ACCEPT_IGNITION_EULA`, `GATEWAY_ADMIN_*`, `IGNITION_UID/GID 2003`, `command: ["-n", "<name>", "--", "${MODE_ARG}"]`, data volume) and `mint-token.sh` (state dir `~/.cache/<rig>` 0700, 0600 `tokens.env`, refuses a state dir inside the repo, never prints keys, no `set -x`, `set -euo pipefail`). Pair specifics from spike `s3_redundancy.sh`: `trust_gan()` moves `gateway-network/{server,client}/security/pki/rejected/*` to `trusted/certs`, approve via `PUT /data/api/v1/gateway-network-incoming/toggleApproval/<url-encoded id>/true` on gw-a, `PUT /data/api/v1/redundancy/config` (`role`, `gatewayNetworkSetup.{host:"gw-a",port:8060,enableSsl:true}`), settle predicate `peerConnected && !syncPending && projectState=="Good"`. Use project `ign-pair-rig`, ports 18300/18301, per-node tokens, a bind mount for gw-b data dir (D-16). Cut the link with `toggleApproval/.../false` on the master (not a Docker network disconnect, Pitfall 2) and tune `pingMaxMissed` so gw-b stays `Cold` (Pitfall 3).

## Shared Patterns

### Decision A verdict slug
**Source:** `deploy/apply/mod.rs` L127-134, `actions/deploy_apply.rs` L112-131, `main.rs` L590-599.
**Apply to:** the new outcome, `result.json`, CLI exit 6. `deploy_backup_unsynced` is never a `CoreError`, never in `CoreError::DEPLOY_SLUGS` (length test ~error.rs:3398 stays 12), never an exit-table row; add a dated README verdict exception (README L102-117, 131-157) and pin it in `deploy_family_coverage.rs` (mirror `rolled_back_verdict_exception_is_dated_and_documented`). Do not repeat the literal `## Exit codes` in README text before the real section.

### Read-only backup by construction
**Source:** `transport::for_node` (read half), `write_half_unreachable` scan, `ForbidWrites`.
**Apply to:** all new backup code; the one new write (`redundancy_resync`) goes to the master session only.

### Value-free output (DSURF-03)
**Source:** `LogLine` withholds messages from `Debug`/serialization; `Steps::emit` event log; canary tests (`contract_deploy.rs` ~2546-3096, `tests/deploy_apply.rs` ~4506-4684).
**Apply to:** secret names only (never values), redundancy events and backup log messages matched client-side, never serialized. Extend the canary with a backup fake.

### Error handling
**Source:** `CoreError` with `e.code()`; non-fatal steps become `warnings.push(format!("name ({})", e.code()))` in `drive`.
**Apply to:** backup phase: a backup REST error is a reason (`unreachable`), not an `Err` out of `drive`; the master has already been finalized.

### Pure decision plus injected clock
**Source:** `wait_step` / `DesignersPolicy`.
**Apply to:** `backup::wait_step`, poll interval option, `--backup-timeout` range-checked.

## No Analog Found

| File | Role | Data Flow | Reason |
|---|---|---|---|
| Redundancy pair compose with two services and GAN trust setup | fixture | batch | Only a single-gateway rig exists in-repo; use the spike on branch `docs/prod-gateway-config-as-code` of `whk-environment-orchestration` (`spikes/gateway-modes/docker-compose.yml`, `s3_redundancy.sh`) read via `git show` as in CONTEXT canonical refs |

Partial matches only: sync-exhausted detection (no existing code reads `redundancy/providers`, `redundancy/events`; fall back to `GatewayApi::logs`), and the poll loop with real `tokio::time::sleep` (precedent: `designers_finding`, preflight.rs ~L552).

## Planner flags (from the research, affect plan shape)

- Plan 24-01 (rig + captures C-P1..C-P8) must land before backup-check code; D-06 (timestamp advance), D-03 (byte equality) and D-10 detection rest on unverified gateway behavior.
- D-11 versus D-17 collision: a cut-off backup may promote itself and make `active_node` return `Both`, refusing the gate; capture C-P6 and tune `pingMaxMissed`.
- New preflights must `Skipped` (not `Pass`) for rollback, which has no rendered tree (Pitfall 7).
- D-14 secret preflight must skip providers the render itself creates (Pitfall 6).

## Metadata

**Analog search scope:** `crates/ignition-core/src/{deploy,client,actions}`, `crates/ignition-core/tests/{common,fixtures}`, `crates/ignition-cli/src/{cli,main}.rs`
**Files scanned:** about 20 read or excerpted
**Pattern extraction date:** 2026-10-19
