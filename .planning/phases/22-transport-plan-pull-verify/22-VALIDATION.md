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
