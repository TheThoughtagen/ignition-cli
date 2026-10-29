---
phase: 20
slug: config-api-client
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-10-10
---

# Phase 20 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution. Source: `20-RESEARCH.md` § Validation Architecture.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (built-in) + wiremock 0.6 + proptest 1.11.0 |
| **Config file** | none (workspace `Cargo.toml`); live gate is env-gated (`IGNITION_LIVE_DEPLOY=1`) |
| **Quick run command** | `cargo test -p ignition-core --lib client:: && cargo test -p ignition-core --test deploy_client_contract` |
| **Full suite command** | The five CI gates: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo build --workspace`; `cargo test --workspace`; `cargo build -p ignition-cli --no-default-features` |
| **Estimated runtime** | ~60 s quick; full suite several minutes |

---

## Sampling Rate

- **After every task commit:** Run the quick run command
- **After every plan wave:** Run all five CI gates locally (WINDOWS.md entry 4)
- **Before `/gsd-verify-work`:** Five gates green on Linux/macOS/Windows CI plus one recorded live-gate run against the ephemeral 8.3.6 on localhost:18290
- **Max feedback latency:** 120 seconds (quick command)

---

## Per-Task Verification Map

Task IDs assigned by the planner (2026-10-11). Threat refs point at each plan's `<threat_model>` STRIDE register.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 20-01-T1, 20-01-T2 | 20-01 | 1 | DAPI-06 (capture-first, feeds all) | T-20-01, T-20-02 | Capture driver refuses non-loopback hosts; no token key in any repo file | live capture + doc grep | `test "$(grep -cE '^### D([1-9]\|1[0-2]) ' .planning/phases/20-config-api-client/20-LIVE-CAPTURES.md)" -eq 12` | ❌ W0 (20-01 creates) | ⬜ pending |
| 20-04-T1 | 20-04 | 3 | DAPI-01 | T-20-16 | Module/type charset refused; `{name}` percent-encoded | wiremock contract + unit | `cargo test -p ignition-core --test deploy_client_contract resource_` | ❌ W0 | ⬜ pending |
| 20-06-T2 | 20-06 | 5 | DAPI-01 | T-20-27, T-20-28, T-20-29 | Effective view vs named collection on a real gateway; the recorded run output is committed at `20-06-live-gate.log` (key-free, no 64-hex signature) before the phase can close | live gate + committed-log check | `IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 IGNITION_LIVE_TOKEN=… IGNITION_LIVE_TOKEN_B=… cargo test -p ignition-core --test live_deploy_api -- --ignored --nocapture`, then after the 20-06 commit: `git ls-files --error-unmatch .planning/phases/20-config-api-client/20-06-live-gate.log && git diff --quiet HEAD -- .planning/phases/20-config-api-client/20-06-live-gate.log && grep -q 'test result: ok. 1 passed' .planning/phases/20-config-api-client/20-06-live-gate.log && test "$(grep -cE '[0-9a-f]{64}' .planning/phases/20-config-api-client/20-06-live-gate.log)" -eq 0` | ❌ W0 | ⬜ pending |
| 20-04-T2 | 20-04 | 3 | DAPI-02 | T-20-17, T-20-20, T-20-21 | Stale signature → `resource_signature_rejected`; signature never echoed; empty signature refused pre-wire; v1.1 EAM pins still green | wiremock contract | `cargo test -p ignition-core --test deploy_client_contract -- signature_ resource_ && cargo test -p ignition-core --test eam_contract` | ❌ W0 / existing | ⬜ pending |
| 20-02-T1 | 20-02 | 1 | DAPI-02, DAPI-07 | T-20-06, T-20-07 | Four-place slug sync | unit | `cargo test -p ignition-core --lib error::tests` | existing (edit) | ⬜ pending |
| 20-03-T2 | 20-03 | 2 | DAPI-03 | T-20-14 | Verdict independent of local clock (proptest over `timestamp`); no clock read in `client/scan.rs` (structural pin `scan_rs_reads_no_clock`) | unit + proptest + contract | `cargo test -p ignition-core --lib client::scan` | ❌ W0 | ⬜ pending |
| 20-06-T2 | 20-06 | 5 | DAPI-03 | T-20-27 | Real hold then 204 | live gate | live command above | ❌ W0 | ⬜ pending |
| 20-03-T2 | 20-03 | 2 | DAPI-04 | T-20-11, T-20-12 | Trigger 200 but status never advances → `NotComplete`, never success | unit + wiremock contract | `cargo test -p ignition-core --test deploy_client_contract scan_` | ❌ W0 | ⬜ pending |
| 20-06-T2 | 20-06 | 5 | DAPI-04 | T-20-27 | Real scan advances the timestamp | live gate | live command above | ❌ W0 | ⬜ pending |
| 20-05-T1 | 20-05 | 4 | DAPI-05 | T-20-22, T-20-23, T-20-24 | `null`/`""`/absent → no mode; `GatewayInfo` envelope unchanged | unit + contract | `cargo test -p ignition-core --lib client::mode && cargo test -p ignition-core --test gateway_info_contract` | ❌ W0 / existing | ⬜ pending |
| 20-03-T1, 20-04-T2, 20-05-T1 | 20-03 / 20-04 / 20-05 | 2-4 | DAPI-06 | T-20-13, T-20-18 | Every endpoint shape enforced (200-empty is an error); 100 KB error body capped ≤ 4096 + marker; 13 DEPLOY_ENDPOINTS rows each tied to a capture block | unit + contract + doc grep | `cargo test -p ignition-core --test deploy_client_contract -- shape_ cap_ && cargo test -p ignition-core --lib client::deploy_api deploy_endpoints_cover_every_method` | ❌ W0 | ⬜ pending |
| 20-03-T1, 20-02-T2, 20-02-T3 | 20-03 / 20-02 | 1-2 | DAPI-07 | T-20-08, T-20-10 | 409/408 on `/scan-lock/` → `scan_lock_held` naming the holder; active node by `activityLevel`; no retired role name in redundancy.rs; `status`/`version`/`redundancy status` JSON keys and envelope unchanged, proven by the six contract test files passing unedited (`$BASE` = parent of the 20-02 Task 1 commit, resolved in the command by its exact subject; same definition as 20-02 Task 3) | unit + contract + grep + JSON contract + no-diff | `cargo test -p ignition-core --test deploy_client_contract lock_held && cargo test -p ignition-core --lib client::redundancy && test "$(grep -c Primary crates/ignition-core/src/client/redundancy.rs)" -eq 0 && cargo test -p ignition-core --test gateway_info_contract --test status_contract && cargo test -p ignition-cli --test contract_status --test contract_version --test version_gateway_contract --test contract_diagnostics && T1=$(git log -n1 --format=%H --fixed-strings --grep='feat(20-02): scan_lock_held and resource_signature_rejected slugs (four-place rule)') && test -n "$T1" && BASE=$(git rev-parse --verify "$T1^") && test -z "$(git diff --name-only "$BASE" HEAD -- crates/ignition-cli/tests/contract_status.rs crates/ignition-cli/tests/contract_version.rs crates/ignition-cli/tests/version_gateway_contract.rs crates/ignition-cli/tests/contract_diagnostics.rs crates/ignition-core/tests/status_contract.rs crates/ignition-core/tests/gateway_info_contract.rs)"` | ❌ W0 / existing | ⬜ pending |
| 20-05-T2, 20-06-T2 | 20-05 / 20-06 | 4-5 | (parity) | T-20-25, T-20-31 | Fake gateway honours the same scenario as the live gateway | differential | `cargo test -p ignition-core --test deploy_client_contract fake_` plus the live command | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `.planning/phases/20-config-api-client/20-LIVE-CAPTURES.md` — captures C1a–C1j, C3a, null-vs-`""`, mode, list (recorded before dependent code) — **20-01-T1/T2**
- [ ] `crates/ignition-core/tests/fixtures/live-rig-deploy/` — one-service 8.3.6 compose (127.0.0.1:18290), token mint script, capture driver, security fixtures — **20-01-T1**
- [ ] `crates/ignition-core/tests/deploy_client_contract.rs` — covers DAPI-01..07 — created by **20-03-T1** (tracer), grown by 20-03-T2, 20-04, 20-05
- [ ] `crates/ignition-core/tests/common/fake_deploy.rs` + `common/deploy_scenario.rs` plus `pub mod` lines in `common/mod.rs` — **20-05-T2**
- [ ] `crates/ignition-core/tests/live_deploy_api.rs` — `#[ignore]`, one sequential test, refuses any non-localhost `IGNITION_LIVE_URL` and any gateway not named `ign-deploy-rig` — **20-06-T1**, run in **20-06-T2**

Framework install: none.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Live gate against ephemeral 8.3.6 | DAPI-01, 03, 04 | Needs Docker + a running gateway; `#[ignore]` in CI | Bring up `tests/fixtures/live-rig-deploy/`, mint tokens, run the live command, commit the unedited output to `.planning/phases/20-config-api-client/20-06-live-gate.log` and cite it in the phase SUMMARY |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
