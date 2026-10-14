---
phase: 22
slug: transport-plan-pull-verify
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-10-14
---

# Phase 22 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (libtest) + `proptest` 1.11 + `wiremock` 0.6 + `assert_cmd` 2.2 |
| **Config file** | none (workspace `Cargo.toml`; CI matrix ubuntu/macos/windows) |
| **Quick run command** | `cargo test -p ignition-core --lib deploy::` |
| **Full suite command** | `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo build -p ignition-cli --no-default-features` |
| **Estimated runtime** | ~40 s compile + <1 s quick run; full suite several minutes |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p ignition-core --lib deploy::` plus the touched integration test file
- **After every plan wave:** Run the full suite command
- **Before `/gsd-verify-work`:** Full suite must be green; live rig gate and WFLWVM01 captures recorded in `22-LIVE-CAPTURES.md`
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

Filled by the planner per task. Requirement-level map (from 22-RESEARCH.md § Validation Architecture):

| Requirement | Behavior | Test Type | Automated Command | File Exists | Status |
|-------------|----------|-----------|-------------------|-------------|--------|
| DPLN-01 | ssh argv hardening, known_hosts, key normalisation, timeouts, `safe_remote_path` | unit + process | `cargo test -p ignition-core --lib deploy::transport` | ❌ W0 | ⬜ pending |
| DPLN-01 | real sftp protocol (existence-first read, error = failure, glob refusal) | integration `cfg(unix)` | `cargo test -p ignition-core --test deploy_transport` | ❌ W0 | ⬜ pending |
| DPLN-01 | write half stage/commit with explicit delete set against scratch dir | integration | `cargo test -p ignition-core --test deploy_transport mirror` | ❌ W0 | ⬜ pending |
| DPLN-01 | no verb names the write half | source-grep | `cargo test -p ignition-core --test deploy_transport write_half_unreachable` | ❌ W0 | ⬜ pending |
| DPLN-01 | proven on real Windows OpenSSH host | live `#[ignore]` + human | `cargo test -p ignition-core --test live_transport_wflwvm01 -- --ignored --nocapture` | ❌ late wave | ⬜ pending |
| DPLN-02 | statuses incl. `unmanaged`, zero writes, exit 0, `has_changes`, golden `plan_sha256`, `live_in` | integration | `cargo test -p ignition-core --test deploy_plan` | ❌ W0 | ⬜ pending |
| DPLN-03 | core relation + destructive/lockout flags | integration | `cargo test -p ignition-core --test deploy_plan core_relation` | ❌ W0 | ⬜ pending |
| DPLN-04 | delete set = ledger − rendered, filters, ledger refusals | unit + integration | `cargo test -p ignition-core --lib deploy::ledger` | ❌ W0 | ⬜ pending |
| DPLN-05 | three-way drift with `--base` | integration | `cargo test -p ignition-core --test deploy_plan drift` | ❌ W0 | ⬜ pending |
| DPLN-06 | pull → render → plan empty; Embedded skipped; `--yes` overwrite guard | integration | `cargo test -p ignition-core --test deploy_pull` | ❌ W0 | ⬜ pending |
| DPLN-07 | each verify check fails independently, passes together | integration | `cargo test -p ignition-core --test deploy_verify` | ❌ W0 | ⬜ pending |
| DPLN-08 | tag probes Good / not Good / absent-list warning | integration | `cargo test -p ignition-core --test deploy_verify probes` | ❌ W0 | ⬜ pending |
| DPLN-09 | red verify envelope, exit 6, slug only in `data` | CLI contract | `cargo test -p ignition-cli --test contract_deploy verify` | extend | ⬜ pending |
| DSURF-04 | dated README verdict exception; verdict slug absent from exit table | test | `cargo test -p ignition-cli --test deploy_family_coverage` | extend | ⬜ pending |
| surface | TUI rows, README rows, exit rows, MCP catalog | existing + new | `cargo test -p ignition-cli --test deploy_family_coverage --test tui_coverage` | partial | ⬜ pending |
| canary | seeded secret absent from plan, `--diff`, verify, pull, stderr | integration | `cargo test -p ignition-core --test deploy_plan canary` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

### Per-task map (filled by the planner, 2026-10-14)

| Task | Wave | Requirement(s) | Automated command | Notes |
|------|------|----------------|-------------------|-------|
| 22-01 T1 (tracer) | 1 | DPLN-01 local, DPLN-02 | `cargo test -p ignition-cli --test contract_deploy && cargo test -p ignition-cli --test deploy_family_coverage && cargo test -p ignition-cli --test tui_coverage && cargo test -p ignition-core --lib error::` | creates `transport_failed`, `deploy plan` surface |
| 22-01 T2 | 1 | DPLN-02, DPLN-03 | `cargo test -p ignition-core --test deploy_plan && cargo test -p ignition-core --lib deploy::` | `PLAN_GOLDEN`, `PlanFixture` (Wave 0 harness) |
| 22-02 T1 | 2 | DPLN-04, DPLN-03 | checkpoint:decision (contract vocabulary) | manual |
| 22-02 T2 | 2 | DPLN-04 | `cargo test -p ignition-core --lib deploy::ledger && cargo test -p ignition-core --lib deploy::transport && cargo test -p ignition-core --lib error::` | ledger slug four-place |
| 22-02 T3 | 2 | DPLN-03, DPLN-04 | `cargo test -p ignition-core --test deploy_plan && cargo test -p ignition-cli --test contract_deploy` | `PLAN_GOLDEN_LEDGER` |
| 22-03 T1 | 3 | DPLN-05 | `cargo test -p ignition-core --test deploy_plan drift` | re-hash closes R-03 |
| 22-03 T2 | 3 | DPLN-02, canary | `cargo test -p ignition-core --test deploy_plan && cargo test -p ignition-cli --test contract_deploy` | `similar` promoted |
| 22-04 T1 | 4 | DPLN-01 | `cargo test -p ignition-core --lib deploy::transport::process && cargo test -p ignition-core --lib deploy::transport::remote` | `ProcessRunner` (Wave 0) |
| 22-04 T2 | 4 | DPLN-01 | `cargo test -p ignition-core --test deploy_transport` | `ssh -G` argv contract |
| 22-05 T1 | 5 | DPLN-01 | `cargo test -p ignition-core --lib deploy::transport::sftp` | fake runner |
| 22-05 T2 | 5 | DPLN-01, DPLN-02 | `cargo test -p ignition-core --test deploy_transport` | real `sftp -D` (unix) |
| 22-06 T1 | 6 | DPLN-01 | `cargo test -p ignition-core --test deploy_transport mirror` | scratch dir |
| 22-06 T2 | 6 | DPLN-01 | `cargo test -p ignition-core --test deploy_transport` | `write_half_unreachable` |
| 22-07 T1 | 6 | DPLN-06 | `cargo test -p ignition-core --test deploy_pull` | fixpoint |
| 22-07 T2 | 6 | DPLN-06 | `cargo test -p ignition-core --test deploy_pull && cargo test -p ignition-cli --test contract_deploy` | guarded, round trip |
| 22-08 T1 | 7 | DPLN-07 (A6, A7) | `bash -n capture22.sh` plus the loopback refusal probe | live rig captures |
| 22-08 T2 | 7 | DPLN-06, DPLN-02 | `cargo test -p ignition-core --test live_deploy_plan` (+ `--ignored` on the rig) | live |
| 22-09 T1 | 8 | DPLN-07, DPLN-08 | `cargo test -p ignition-core --test deploy_verify` | pure engine |
| 22-09 T2 | 8 | DPLN-09, DSURF-04 | `cargo test -p ignition-cli --test contract_deploy && cargo test -p ignition-cli --test deploy_family_coverage` | exit 6, dated README exception, `ForbidWrites` (Wave 0) |
| 22-09 T3 | 8 | DPLN-07 | full suite command (five gates) + `--ignored` rig run | installed-binary smoke |
| 22-10 T1 | 9 | DPLN-01 | checkpoint:human-action (WFLWVM01) | manual |
| 22-10 T2 | 9 | DPLN-01 (C7a, C6, C5-NTFS) | `bash -n capture22w.sh` | live captures |
| 22-10 T3 | 9 | exit gate | checkpoint:decision (Decision B) | manual |
| 22-11 T1 | 10 | DPLN-01 | `cargo test -p ignition-core --lib deploy::transport && cargo test -p ignition-core --lib deploy::ledger` | Windows code from C7a |
| 22-11 T2 | 10 | DPLN-01 (SC-1) | `cargo test -p ignition-core --test live_transport_wflwvm01` (+ `--ignored` on WFLWVM01) | live |
| 22-12 T1 | 11 | exit gate (OQ5) | `cargo test -p ignition-core --test live_transport_wflwvm01` (+ `--ignored` OQ5 run A) | live |
| 22-12 T2 | 11 | exit gate | checkpoint:human-action (Defender exclusion) | manual |
| 22-12 T3 | 11 | exit gate | `cargo test -p ignition-core --lib deploy::target` | hold budget, STATE/ROADMAP |

Sampling continuity: no three consecutive tasks lack an automated command (checkpoints are isolated between automated tasks).

---

## Wave 0 Requirements

- [ ] `tests/common`: `ForbidWrites<T>` wrapper, `GatewayApi` double with logs + WebDev tags rows, `deploy_plan` fixture builder (core/mode/ledger layers)
- [ ] `crates/ignition-core/tests/{deploy_transport,deploy_plan,deploy_pull,deploy_verify}.rs`, `live_deploy_plan.rs`, `live_transport_wflwvm01.rs`
- [ ] `ProcessRunner` fake (records argv/stdin/timeouts, scripted outputs)
- [ ] `similar` promoted to `ignition-core` dependencies
- [ ] `tree::walk_out` promoted to `pub(crate)`; load+render step of `deploy_render` extracted to a shared function

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| WFLWVM01 provisioning (Ignition 8.3.6, OpenSSH Server, `svc-ignition-deploy`) | DPLN-01 | Human infra setup | Checkpoint task; agent runs C6/C7a/OQ5 afterwards |
| C6 / C7a / OQ5 captures | exit gate | Real Windows host | Record raw output in `22-LIVE-CAPTURES.md`; Decision B and hold budget recorded |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
