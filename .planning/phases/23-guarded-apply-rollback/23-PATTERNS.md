# Phase 23: Guarded Apply & Rollback - Pattern Map

**Mapped:** 2026-10-16
**Files analyzed:** 22 new/modified
**Analogs found:** 20 / 22

All paths below are relative to `crates/` unless noted. Line numbers were read on 2026-10-16.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `ignition-core/src/deploy/apply/mod.rs` (run_apply, run_rollback, restore_snapshot) | service/engine | request-response + file-I/O | `ignition-core/src/actions/deploy.rs` `deploy_pull` / `deploy_verify` | role-match |
| `.../apply/gates.rs` (pure flag x plan gates) | utility | transform | `ignition-core/src/deploy/plan.rs` (pure, hashed) | role-match |
| `.../apply/preflight.rs` | service | request-response | `ignition-core/src/actions/doctor.rs:393` `check_write`, `actions/sessions.rs:71` | role-match |
| `.../apply/hold.rs` (HOLD_S, HeldLock, Clock) | utility | event-driven (timer) | `ignition-core/src/client/scan.rs:98-143` `lock_verdict` | role-match |
| `.../apply/fingerprint.rs` | utility | transform | `deploy/tree.rs:62-71` `render_sha256`, `plan.rs` plan_sha256 | exact |
| `.../apply/journal.rs`, `record.rs` | model | file-I/O | `ignition-core/src/deploy/ledger.rs` `Ledger` | exact |
| `.../apply/run_store.rs` | utility | file-I/O | `deploy/pull.rs` (create_new), `remote.rs:380` `restrict_to_owner` | partial |
| `.../apply/host.rs` (ApplyHost, LocalHost, FaultyHost) | service/trait | file-I/O | `transport/mod.rs:320` `TreeMirror` + `transport/local.rs` | role-match |
| `transport/scripts/commit.sh`, `commit.ps1` | script | batch | `transport/winps.rs` `MOVE_REPLACE`, `ENUMERATE` | role-match |
| `transport/winps.rs` (modify: file-driven script runner) | utility | request-response | itself (`encoded_command`, `parse_ign_result`) | exact |
| `transport/mod.rs`, `sftp.rs`, `local.rs` (modify: reader gains `read_journal`; commit replaced for ssh) | service | file-I/O | `sftp.rs:2268` `commit_inner`, `TreeReader::read_ledger` | exact |
| `deploy/ledger.rs` (modify: snapshot, journal, record paths) | utility | transform | `ledger_rel`, `staging_rel` (ledger.rs:136-147) | exact |
| `ignition-core/src/actions/deploy_apply.rs` | service (thin verbs) | request-response | `actions/deploy.rs` `DeployVerifyResult::failure_exit_code` (:871) | exact |
| `ignition-core/src/error.rs` (new slugs) | model | n/a | `DEPLOY_SLUGS` (:1564) and pinned test (:2918) | exact |
| `ignition-cli/src/cli.rs` `DeployCommand` (+Apply, +Rollback) | config/controller | request-response | `DeployCommand::Plan/Pull` (cli.rs:622-) | exact |
| `ignition-cli/src/main.rs` (dispatch, GUARDED_OPS, verdict exit) | controller | request-response | `DeployCommand::Pull` arm (main.rs:3150-3178), verdict exit (:565-576), `GUARDED_OPS` (:3933, :4046) | exact |
| `ignition-cli/src/mcp.rs` (EXCLUDED_PATHS) | config | n/a | `EXCLUDED_LEAVES` (:62, :105-108) | role-match |
| `ignition-tui/src/routes.rs` rows | component | n/a | `deploy_rows_cover_the_family` | exact |
| `README.md` exit table + docs | doc | n/a | existing deploy rows (pinned by `readme_exit_table_agreement`) | exact |
| `ignition-core/tests/deploy_apply.rs` | test | request-response | `tests/deploy_verify.rs` + `tests/common/fake_deploy.rs` | role-match |
| `ignition-core/tests/deploy_host_conformance.rs` | test | batch | `tests/deploy_transport.rs` | role-match |
| `tests/deploy_transport.rs` `write_half_unreachable` (modify) | test | n/a | itself (:2811) | exact |

## Pattern Assignments

### `deploy/apply/journal.rs`, `record.rs` (model, file-I/O)

**Analog:** `ignition-core/src/deploy/ledger.rs` (strict versioned JSON, `deny_unknown_fields`, `Ledger::parse` / `to_bytes`, size cap, sorted unique paths).

**Path helpers to extend** (ledger.rs:136-147). Add `journal_rel`, `snapshot_rel(gateway, mode, run_id)`, `record_rel` in the same shape:
```rust
pub fn ledger_rel(gateway: &str, mode: &str) -> String {
    format!("ign-deploy/{gateway}/{mode}/{LEDGER_FILE}")
}
pub fn staging_rel(gateway: &str, mode: &str, run_id: &str) -> String {
    format!("ign-deploy/{gateway}/{mode}/staging/{run_id}")
}
```
**Root selection** (ledger.rs:126-134): `LedgerRoot::for_node(node)` is `AccountHome` for ssh, `DataDir` otherwise. `ledger_location` (:149) renders `~/...` for error text. Run ids must satisfy `[A-Za-z0-9._-]{1,128}`.

Round-trip rule: build, then `X::parse(to_bytes())` before upload (research Pattern 4).

---

### `deploy/apply/hold.rs` (utility, timer)

**Analog:** `ignition-core/src/client/scan.rs:98-143`. `HeldLock::check` must call `lock_verdict` and add the local monotonic clock as second opinion. Never read `timestamp`.
```rust
pub fn lock_verdict(obs: &LockObservation, margin_s: i64) -> LockVerdict {
    match obs {
        LockObservation::Free => LockVerdict::NotHeld,
        LockObservation::Held(info) if info.remaining >= margin_s => LockVerdict::Held { remaining_s: info.remaining },
        LockObservation::Held(info) => LockVerdict::Expiring { remaining_s: info.remaining },
    }
}
```
Wire calls: `DeployApi::scan_lock_acquire` (deploy_api.rs:293), `scan_lock_info` (:302), `deployment_mode` (:336), and `scan_and_wait(api, ScanKind::Config, PollConfig)` (deploy_api.rs:930). Tests control gateway time with `FakeGateway::tick(seconds)` (tests/common/fake_deploy.rs:207-219), which decrements `remaining` and clears the lock at <= 0. Use `std::time::Instant` through an injected `Clock` trait (no tokio test-util).

---

### `deploy/apply/host.rs`, `transport/scripts/*`, `winps.rs` (service/script, file-I/O + batch)

**Analog:** `transport/mod.rs:320` `TreeMirror` (async_trait, `Send + Sync`) for the trait shape, `transport/local.rs:50-97` (`LocalFsTransport::with_mirror`) as the fault-injection surface, `winps.rs:165-232` for PowerShell invocation.

**Script runner pattern** (winps.rs:198-232): parameters travel as base64 JSON inside `$p`, total command bounded by `COMMAND_BUDGET = 8000`:
```rust
pub fn encoded_command(script: &WinScript, budget: usize) -> Result<RemoteCommand, String> {
    let mut text = String::from(PRELUDE);
    ...
    text.push_str("$p=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('");
    ...
    let command = RemoteCommand::powershell_encoded(&text);
    if length > budget { return Err(...) }
```
Executor: `RemoteShell::exec_powershell(&self, script: &WinScript) -> Result<Value, CoreError>` (remote.rs:994); result is one `IGN-RESULT:` line via `parse_ign_result` (winps.rs:235). Existing scripts `WinScript::move_replace` (:174) and `enumerate` (:184) show the `name`/`body`/`params` struct. Per research Pattern 1, the 152-path work list does not fit the budget: upload `work.json` plus the script with the sftp `put` path used by `stage`, and run a short stub that checks the file hash.

**Replaced code:** the per-file sftp commit at `sftp.rs:2268` `commit_inner` (validate owner/staging root, `windows_paths`, `validate_mirror_paths`, read-back hash, rename batch with `RENAME_RETRIES = 3` at sftp.rs:1666). Keep its preflight validation calls, replace its under-lock part.

LocalFs implementation: reuse the descriptor-relative helpers in `local.rs` (`dirfd::rename_at`, `unlink_at`, `ensure_walk`, `read_file`, ~lines 1073-1239; path-based fallback ~1245).

**Write-half guard:** `GATEWAY_OWNED_FILES` / `mirror_path_problem` (transport/mod.rs ~489-508) must also gate the restore script.

---

### `deploy/apply/run_store.rs` (utility, file-I/O)

**Analog (ACL):** `transport/remote.rs:380-401`:
```rust
#[cfg(windows)]
fn restrict_to_owner(file: &Path, label: &str) -> Result<(), CoreError> {
    let user = std::env::var("USERNAME").map_err(|_| setup_failed(label, "USERNAME is not set; ..."))?;
    let status = std::process::Command::new("icacls").arg(file)
        .args(["/inheritance:r", "/grant:r"]).arg(format!("{user}:R")) ...
```
It is file-only and read-only (`:R`); the directory variant needs `(OI)(CI)F`. Unix side: 0700 dir, 0600 files. Timestamp for run id: reuse the civil-from-days code in `actions/workspace.rs:976-1000` (`unix_ms_to_rfc3339_utc`, `rfc3339_now_utc`), formatted `YYYYMMDDTHHMMSSZ`, plus `getrandom` suffix. Git guard: shell out to `git check-ignore -q -- <path>` (0 ignored, 1 not, 128 not a repo). `.gwbk` via `actions::backup::backup_download(api, Some(out), stem, BackupType)` (backup.rs:72); pre-create the file 0600 first.

---

### `deploy/apply/preflight.rs` (service, request-response)

**Analog:** `actions/doctor.rs:393-420` `check_write` for the write probe:
```rust
match api.scan_projects().await {
    Ok(()) => row("write", CheckStatus::Ok, ...),
    Err(CoreError::Auth { status: 403, .. }) => row("write", CheckStatus::Warn, "read-only token (HTTP 403 ...)", ...),
```
Map 403 to a preflight failure here. Designers: `GatewayApi::designers` (client/mod.rs:157, 1075) / `sessions()` (actions/sessions.rs:71), `DesignerInfo{user, project, address}`. Aggregate every failure into one `deploy_preflight_failed`.

---

### `actions/deploy_apply.rs` (thin verbs + verdict result)

**Analog:** `actions/deploy.rs:871` and its verdict contract:
```rust
impl DeployVerifyResult {
    pub fn failure_exit_code(&self) -> Option<u8> { (!self.ok).then_some(6) }
}
```
`DeployApplyResult` / `DeployRollbackResult` get the same method (`deploy_rolled_back` is `data.slug`, exit 6, not a `CoreError`). Reuse `load_target` (:127), `load_declared` (:160), `deploy_plan` (:276; `PlanOptions.base`), `deploy_verify` (:893). Actions never print; progress through a sink.

---

### `ignition-cli/src/main.rs` dispatch + GUARDED_OPS

**Analog:** the `Pull` arm (main.rs:3150-3178): load target, take `profile`, build prose, then the marker directly above the gate:
```rust
if let Some(prose) = prose {
    // guarded:deploy pull
    if let Err(err) = require_confirmation(cli.yes, &prose) { return (profile, Err(err)); }
}
```
Add `// guarded:deploy apply` and `// guarded:deploy rollback` markers and `GUARDED_OPS` entries shaped like main.rs:4046-4049 (`("deploy pull", "deploy pull would replace ...")`). Test `guarded_ops_registry_tracks_every_dispatch_site` (~:4606-4657) enforces both directions. Verdict exit after the envelope renders, extending main.rs:565-576:
```rust
if let ActionOutput::DeployVerify(verify) = &out && let Some(code) = verify.failure_exit_code() {
    return ExitCode::from(code);
}
```
Session for REST: `named_profile_client(&mut config, &master.profile)` then `session.api()` (as in the Verify arm). The apply module is the only one allowed to name `TreeMirror`; main.rs calls only `actions::deploy_apply::*`.

---

### `ignition-cli/src/cli.rs` `DeployCommand`

**Analog:** `Plan` variant (cli.rs:647-): `#[arg(long, value_name = "ENV")] env: String`, `#[arg(long, value_name = "DIR", default_value = ".")] repo: PathBuf`, `base: Option<PathBuf>` doc-commented. Add flags: `--plan-sha256`, `--allow-destructive`, `--allow-drift <reason>`, `--designers`, `--designers-timeout`, `--skip-gwbk`, `--discard-live-changes`, `--run`. `--yes` is global (`cli.yes`).

---

### `ignition-core/src/error.rs` (slugs)

**Analog:** `DEPLOY_SLUGS` (error.rs:1564-1569) and the pinned test at :2918-2955, which asserts `codes == CoreError::DEPLOY_SLUGS` in order and `!hint.contains("ign ")`. D-09 needs the rollback command in the refusal: put it in `Display` and carve the new variants out of the hint rule (research Pitfall 4). Four-place rule: variant, `code()` (:1020), `exit_code()` (:1080 area), `hint()` (:1144 area), plus README row and `exit_code_mapping_enumerated`.

---

### `ignition-cli/src/mcp.rs` exclusion

**Analog:** `EXCLUDED_LEAVES: &[&str] = &["completions","mcp","lsp","tui","edit"]` (mcp.rs:62) matched per segment at :105-108. Add a separate full-path list (`"deploy apply"`, `"deploy rollback"`); amend the test at mcp.rs:833-845.

---

### Tests

- **Engine tests:** `tests/common/fake_deploy.rs` (`FakeGateway::new` :187, `client(token)` :198, `tick` :207, `set_deployment_mode` :222, `seed_singleton` :227) plus `LocalFsTransport::with_mirror`. `FaultyHost<H>` wrapper delegates until nth op then errors or hangs for `JoinHandle::abort()`.
- **Canary:** extend `tests/deploy_verify.rs` pattern (`serde_json::to_string(&report)` then `assert!(!serialized.contains(CANARY))`) and `contract_deploy.rs:968-1042`; exempt `snapshot/**` and `*.gwbk` explicitly.
- **Write-half guard:** `tests/deploy_transport.rs:2811` `write_half_unreachable` scans `actions/deploy.rs`, `main.rs`, `mcp.rs` for `TreeMirror`, `mirror_for_node`, `StagingMode`, `.stage(`, `.commit(`; keep that and add a positive assertion for `deploy/apply/` and `deploy/transport/`.

## Shared Patterns

### Strict versioned JSON
**Source:** `deploy/ledger.rs` (`Ledger`). **Apply to:** journal, run record, `run.json`, `result.json`, work list.

### Verdict-in-data, exit after envelope
**Source:** `actions/deploy.rs:871`, `main.rs:565-576`. **Apply to:** `deploy_rolled_back`, red apply results.

### Windows-script result contract
**Source:** `winps.rs:235` `parse_ign_result` (one `IGN-RESULT:` line, ASCII-only JSON). **Apply to:** all three hosts' report format.

### Path safety
**Source:** `validate_mirror_paths`, `mirror_path_problem`, `windows_component_problem` (winps.rs:97), `windows_absolute_path_problem` (:135). **Apply to:** staging, snapshot and restore paths.

### Fingerprints
**Source:** `tree.rs:62-71` `render_sha256`; hash `path NUL (sha|"-") LF` in byte order, computed in Rust, never in a script.

## No Analog Found

| File | Role | Data Flow | Reason |
|---|---|---|---|
| `transport/scripts/commit.sh` / `commit.ps1` full commit/restore logic | script | batch | Only small PowerShell scripts exist (`MOVE_REPLACE`, `ENUMERATE`); no POSIX `sh` script and no file-uploaded script anywhere. Use RESEARCH.md Pattern 1 and the C7a-4 rules |
| `HeldLock` / `Clock` | utility | timer | `lock_verdict` is stateless; no holder type exists. Use RESEARCH.md Pattern 6 |
| Directory-variant ACL (`icacls` for a directory) | utility | file-I/O | Only the file variant exists (remote.rs:380) |

## Metadata

**Analog search scope:** `ignition-core/src/{deploy,deploy/transport,actions,client}`, `ignition-core/tests`, `ignition-cli/src/{main,cli,mcp}.rs`
**Pattern extraction date:** 2026-10-16
