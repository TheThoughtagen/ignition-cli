---
phase: 21
slug: deploy-render
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-10-12
---

# Phase 21 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` + `proptest` 1.11 + `snapbox`/`assert_cmd`/`predicates` |
| **Config file** | none (Cargo workspace); CI = `.github/workflows/ci.yml` (ubuntu, macos, windows-latest) |
| **Quick run command** | `cargo test -p ignition-core --lib deploy::` |
| **Full suite command** | `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo build --workspace && cargo build -p ignition-cli --no-default-features && cargo test --workspace` |
| **Estimated runtime** | ~180 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p ignition-core --lib deploy::` plus the touched integration test
- **After every plan wave:** Run `cargo test -p ignition-core -p ignition-cli` and `cargo clippy --workspace --all-targets -- -D warnings`
- **Before `/gsd-verify-work`:** Full suite must be green locally and on all three CI OS legs
- **Max feedback latency:** 180 seconds

---

## Per-Task Verification Map

Task IDs are filled in by the planner/executor. Requirement-level map:

| Requirement | Behavior | Test Type | Automated Command | File Exists | Status |
|-------------|----------|-----------|-------------------|-------------|--------|
| DRND-01 | same tree+env → same `render_sha256`, read-order independent | unit + CLI golden | `cargo test -p ignition-core --test deploy_render determinism` / `cargo test -p ignition-cli --test contract_deploy render_digest_golden` | ❌ W0 | ⬜ pending |
| DRND-02 | unknown field / wrong `schema_version` / syntax error → `deploy_tree_invalid` with file:line:col | unit | `cargo test -p ignition-core --lib deploy::yamlio` | ❌ W0 | ⬜ pending |
| DRND-03 | `modes/<env>/` replaces base file whole-file by path | unit | `cargo test -p ignition-core --test deploy_render overlay` | ❌ W0 | ⬜ pending |
| DRND-04 | un-allowlisted type refused; security-properties/levels/api-token/user-source refused | unit | `cargo test -p ignition-core --test deploy_render allowlist` | ❌ W0 | ⬜ pending |
| DRND-05 | corpus round-trips byte-identical; opaque copy for non-Gson files; writer idempotence proptest | integration + proptest (3 OSes) | `cargo test -p ignition-core --test deploy_corpus` | ❌ W0 | ⬜ pending |
| DRND-06 | uuid rule table (singleton / named+uuid / named-no-uuid); deterministic sha2→v8; existing kept | unit | `cargo test -p ignition-core --lib deploy::rules deploy::uuid` | ❌ W0 | ⬜ pending |
| DRND-07 | `Embedded` / plaintext secret → `embedded_secret`, path named, canary value never in output | unit + CLI | `cargo test -p ignition-core --test deploy_render secrets` / `cargo test -p ignition-cli --test contract_deploy embedded_secret_names_path_not_value` | ❌ W0 | ⬜ pending |
| DRND-08 | case-fold, illegal chars, trailing dot/space, reserved names, path budget, CR in text members (not jars) | unit + proptest | `cargo test -p ignition-core --lib deploy::lint` | ❌ W0 | ⬜ pending |
| DSURF-01 | every `deploy` leaf has TUI route row, README row, exit-table slugs | integration | `cargo test -p ignition-cli --test deploy_family_coverage` / `cargo test -p ignition-cli --test tui_coverage` | ❌ W0 | ⬜ pending |
| slugs | four-place rule for `deploy_tree_invalid`, `embedded_secret` | unit | `cargo test -p ignition-core --lib error::tests` | ✅ (edit) | ⬜ pending |
| MCP | `deploy_render` in catalog via clap derivation | unit | `cargo test -p ignition-cli mcp` | ✅ | ⬜ pending |
| stdout purity | `deploy render --json` writes only the envelope | CLI | `cargo test -p ignition-cli --test contract_stdout_purity` | ✅ (edit) | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/ignition-core/tests/fixtures/deploy/corpus/**` — sanitized corpus, `SANITIZE.md`, fixture-secret scan test
- [ ] `crates/ignition-core/tests/fixtures/deploy/openapi-8.3.6-resource-types.json` — extract + generating script
- [ ] `.gitattributes`: `crates/ignition-core/tests/fixtures/deploy/** -text`, `*.jar binary`
- [ ] `.planning/phases/21-deploy-render/21-LIVE-CAPTURES.md` + `capture.sh` subcommands `c3b c4 c5 oq1 oq4 oq8`
- [ ] `crates/ignition-core/tests/{deploy_corpus.rs,deploy_render.rs}`, `crates/ignition-cli/tests/{contract_deploy.rs,deploy_family_coverage.rs}`

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Live captures C3b/C4/C5/OQ1/OQ4/OQ8 and v8-uuid acceptance | DRND-05, DRND-06, DRND-07 | Needs live 8.3.6 rig | Run `capture.sh` on the Phase 20 live-rig-deploy rig; record in 21-LIVE-CAPTURES.md |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 180s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
