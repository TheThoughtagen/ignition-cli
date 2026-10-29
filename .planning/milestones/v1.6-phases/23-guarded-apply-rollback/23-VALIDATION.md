---
phase: 23
slug: guarded-apply-rollback
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-10-15
---

# Phase 23 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution. Seeded from 23-RESEARCH.md `## Validation Architecture`.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (built-in harness) + wiremock, assert_cmd, predicates, proptest, tempfile |
| **Config file** | none beyond `Cargo.toml`; CI `.github/workflows/ci.yml` (fmt, clippy `-D warnings`, `cargo test --workspace`, `--no-default-features` build on ubuntu/macos/windows) |
| **Quick run command** | `cargo test -p ignition-core --test deploy_apply` |
| **Full suite command** | `cargo test --workspace` |
| **Live gate** | `IGNITION_LIVE_DEPLOY=1 cargo test -p ignition-core --test live_deploy_apply -- --ignored --nocapture` |
| **Estimated runtime** | ~30 s quick, several minutes full |

---

## Sampling Rate

- **After every task commit:** `cargo test -p ignition-core --test deploy_apply` plus the touched module's `--lib` filter
- **After every plan wave:** `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`
- **Before `/gsd-verify-work`:** full suite green on all three CI legs, live rig gate run once, Windows live proof recorded
- **Max feedback latency:** 60 seconds for the quick command

---

## Per-Task Verification Map

Task IDs are filled in by the planner. Requirement-level map:

| Requirement | Behavior | Test Type | Automated Command | File Exists | Status |
|-------------|----------|-----------|-------------------|-------------|--------|
| DAPL-01 | no `--yes` refuses with plan; stale hash `plan_stale`; gate precedence | unit + CLI | `cargo test -p ignition-core --lib deploy::apply::gates` ; `cargo test -p ignition-cli --test contract_deploy apply_gates` | ❌ W0 | ⬜ pending |
| DAPL-01 | `--plan-sha256` required when non-interactive | CLI | `cargo test -p ignition-cli --test contract_deploy apply_requires_plan_hash_when_noninteractive` | ❌ W0 | ⬜ pending |
| DAPL-02 | destructive / drift / `shadows_removal` gates; drift reason recorded | unit + engine | `cargo test -p ignition-core --test deploy_apply gates_` | ❌ W0 | ⬜ pending |
| DAPL-03 | empty plan exits 0, no lock, no run dir, no write | engine | `cargo test -p ignition-core --test deploy_apply empty_plan_is_a_noop` | ❌ W0 | ⬜ pending |
| DAPL-04 | all preflight failures in one refusal; designers fail/wait/proceed; journal; hold estimate | engine | `cargo test -p ignition-core --test deploy_apply preflight_` | ❌ W0 | ⬜ pending |
| DAPL-04 | git check-ignore guard | unit | `cargo test -p ignition-core --lib deploy::apply::run_store` | ❌ W0 | ⬜ pending |
| DAPL-05 | snapshot hashes on box + run dir; `.gwbk` 0600, truncation refused, `--skip-gwbk` recorded | engine | `cargo test -p ignition-core --test deploy_apply snapshot_` | ❌ W0 | ⬜ pending |
| DAPL-06 | lock expiry / mid-write failure restore snapshot then scan, never partial | engine fault injection | `cargo test -p ignition-core --test deploy_apply fault_` | ❌ W0 | ⬜ pending |
| DAPL-06 | hold constant from recorded p95s | unit | `cargo test -p ignition-core --lib deploy::apply::hold` | ❌ W0 | ⬜ pending |
| DAPL-07 | verify failure auto-rollback, exit 6 `deploy_rolled_back`; failed restore `rollback_failed` | engine + CLI | `cargo test -p ignition-core --test deploy_apply auto_rollback_` ; `cargo test -p ignition-cli --test contract_deploy apply_rolled_back_exit_6` | ❌ W0 | ⬜ pending |
| DAPL-08 | fresh-process rollback; `deploy_live_changed`; `--discard-live-changes` | engine + CLI | `cargo test -p ignition-core --test deploy_apply rollback_` | ❌ W0 | ⬜ pending |
| DAPL-09 | killed apply leaves journal; plan/apply/rollback refuse until recovery | engine crash sim | `cargo test -p ignition-core --test deploy_apply journal_` | ❌ W0 | ⬜ pending |
| DAPL-10 | run dir on every run incl. failures; 0700/0600; `restart_pending` reported, no restart call | engine | `cargo test -p ignition-core --test deploy_apply run_dir_` | ❌ W0 | ⬜ pending |
| DSURF-02 | apply/rollback in `GUARDED_OPS`, absent from MCP; render/plan/pull/verify present | CLI unit | `cargo test -p ignition-cli mcp::tests` ; `cargo test -p ignition-cli guarded_ops_registry_tracks_every_dispatch_site` | ✅ amend | ⬜ pending |
| DSURF-01 (keep green) | README rows, TUI routes, exit-table rows for new slugs | CLI | `cargo test -p ignition-cli --test deploy_family_coverage` | ✅ | ⬜ pending |
| DSURF-03 | secret canary absent from all outputs and run-dir files (snapshot/gwbk exempt, 0600) | CLI + core | `cargo test -p ignition-cli --test contract_deploy apply_canary` | ❌ W0 | ⬜ pending |
| guard | only `deploy/apply/` and `deploy/transport/` name the write half | source scan | `cargo test -p ignition-core --test deploy_transport write_half_unreachable` | ✅ amend | ⬜ pending |
| guard | `apply --json` / `rollback --json` stdout purity | CLI | `cargo test -p ignition-cli --test contract_stdout_purity` | ✅ add cases | ⬜ pending |
| parity | LocalHost / POSIX sh / PowerShell scenario table | conformance | `cargo test -p ignition-core --test deploy_host_conformance` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/ignition-core/tests/deploy_apply.rs` — engine suite
- [ ] `crates/ignition-core/tests/deploy_host_conformance.rs` — shared scenario table
- [ ] `crates/ignition-core/tests/common/` — FakeGateway loads mode collection on scan, restart_pending + gateway_info name, two-gateway setup, `FaultyHost`, git-repo tempdir fixture
- [ ] wiremock `GatewayApi` double via `ReqwestGatewayApi::for_tests` for designers, backup download, scan, logs
- [ ] `crates/ignition-core/tests/live_deploy_apply.rs` (`#[ignore]`, `IGNITION_LIVE_DEPLOY=1`)
- [ ] `crates/ignition-core/src/deploy/transport/scripts/commit.sh` and `commit.ps1`
- [ ] `23-LIVE-CAPTURES.md` with the Captures Owed table
- [ ] Migrate (not delete) the sftp commit tests pinning the per-file path

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Windows host available for D-02 re-measure, C7b, C-script-run | DAPL-06 | Azure resource group not found from this session | Confirm or re-provision `ign-win-836`, then run the Windows live gate |
| C2 scan-lock blocking behavior, C11 single-scan ordering | DAPL-06 | Needs live 8.3.6 gateway capture | Run captures, record in `23-LIVE-CAPTURES.md` |
| Live rig apply → kill → rollback | DAPL-08, DAPL-09 | Real gateway process | `IGNITION_LIVE_DEPLOY=1` live gate |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
