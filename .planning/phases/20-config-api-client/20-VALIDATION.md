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

Task IDs are assigned by the planner; rows are keyed by requirement until then.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| TBD | — | — | DAPI-01 | — | Module/type charset refused; `{name}` percent-encoded | wiremock contract + unit | `cargo test -p ignition-core --test deploy_client_contract resource_` | ❌ W0 | ⬜ pending |
| TBD | — | — | DAPI-01 | — | Effective view vs named collection on a real gateway | live gate | `IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 IGNITION_LIVE_TOKEN=… cargo test -p ignition-core --test live_deploy_api -- --ignored --nocapture` | ❌ W0 | ⬜ pending |
| TBD | — | — | DAPI-02 | — | Stale signature → `resource_signature_rejected`; signature never echoed; empty signature refused pre-wire; v1.1 EAM pins still green | wiremock contract | `cargo test -p ignition-core --test deploy_client_contract signature_ && cargo test -p ignition-core --test eam_contract` | ❌ W0 / existing | ⬜ pending |
| TBD | — | — | DAPI-02 | — | Four-place slug sync | unit | `cargo test -p ignition-core --lib error::tests` | existing (edit) | ⬜ pending |
| TBD | — | — | DAPI-03 | — | Verdict independent of local clock (proptest over `timestamp`); no `SystemTime`/`now()` in `client/scan.rs` | unit + proptest + contract | `cargo test -p ignition-core --lib client::scan` | ❌ W0 | ⬜ pending |
| TBD | — | — | DAPI-03 | — | Real hold then 204 | live gate | live command above | ❌ W0 | ⬜ pending |
| TBD | — | — | DAPI-04 | — | Trigger 200 but status never advances → `NotComplete`, never success | unit + wiremock contract | `cargo test -p ignition-core --test deploy_client_contract scan_` | ❌ W0 | ⬜ pending |
| TBD | — | — | DAPI-04 | — | Real scan advances the timestamp | live gate | live command above | ❌ W0 | ⬜ pending |
| TBD | — | — | DAPI-05 | — | `null`/`""`/absent → no mode; `GatewayInfo` envelope unchanged | unit + contract | `cargo test -p ignition-core --lib client::mode && cargo test -p ignition-core --test gateway_info_contract` | ❌ W0 / existing | ⬜ pending |
| TBD | — | — | DAPI-06 | — | Every endpoint shape enforced; 100 KB error body capped ≤ 4096 + marker; capture block per endpoint | unit + contract + doc grep | `cargo test -p ignition-core --test deploy_client_contract shape_ cap_` and `grep -c '^## ' .planning/phases/20-config-api-client/20-LIVE-CAPTURES.md` | ❌ W0 | ⬜ pending |
| TBD | — | — | DAPI-07 | — | 409/408 on `/scan-lock/` → `scan_lock_held` naming the holder; active node by `activityLevel`; no `Primary` | unit + contract + grep | `cargo test -p ignition-core --test deploy_client_contract lock_held && cargo test -p ignition-core --lib client::redundancy && ! grep -n Primary crates/ignition-core/src/client/redundancy.rs` | ❌ W0 / existing | ⬜ pending |
| TBD | — | — | (parity) | — | Fake gateway honours the same scenario as the live gateway | differential | `cargo test -p ignition-core --test deploy_client_contract fake_` plus the live command | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `.planning/phases/20-config-api-client/20-LIVE-CAPTURES.md` — captures C1a–C1i, C3a, null-vs-`""`, mode, list (recorded before dependent code)
- [ ] `crates/ignition-core/tests/fixtures/live-rig-deploy/` — one-service 8.3.6 compose (port 18290), token mint script, security fixtures
- [ ] `crates/ignition-core/tests/deploy_client_contract.rs` — covers DAPI-01..07
- [ ] `crates/ignition-core/tests/common/fake_deploy.rs` plus `pub mod fake_deploy;` in `common/mod.rs`
- [ ] `crates/ignition-core/tests/live_deploy_api.rs` — `#[ignore]`, one sequential test, refuses any non-localhost `IGNITION_LIVE_URL`

Framework install: none.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Live gate against ephemeral 8.3.6 | DAPI-01, 03, 04 | Needs Docker + a running gateway; `#[ignore]` in CI | Bring up `tests/fixtures/live-rig-deploy/`, mint tokens, run the live command, record the output in the phase SUMMARY |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
