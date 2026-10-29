---
phase: 25
slug: guarded-project-deploy
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-10-22
validated: 2026-10-29
---

# Phase 25 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust unit + integration, wiremock, assert_cmd) plus bash CI harnesses |
| **Config file** | `Cargo.toml` workspace; harnesses in `.planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/` |
| **Quick run command** | `cargo test -p ignition-core --test deploy_projects && cargo test -p ignition-cli --test contract_deploy_projects` |
| **Full suite command** | `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check` plus `bash $H/{plan,apply,projects,readme,release}-test.sh` |
| **Estimated runtime** | quick ~25 s, full ~7 min |

---

## Sampling Rate

- **After every task commit:** Run the task's `<automated>` command from its PLAN
- **After every plan wave:** Run the full suite command
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 25 seconds (quick command)

---

## Per-Task Verification Map

Threat refs and secure behaviors are in `25-SECURITY.md`. Every non-checkpoint task carries an `<automated>` command in its PLAN. The rows below group them by plan.

| Task IDs | Plan | Wave | Requirement | Test Type | Automated Command (abridged) | File Exists | Status |
|----------|------|------|-------------|-----------|------------------------------|-------------|--------|
| 25-01-01..03 | 01 | 1 | DPRJ-01, 02, 03 | capture + script | `bash -n capture25.sh`; grep C8a..C8e in 25-LIVE-CAPTURES.md | ✅ | ✅ green |
| 25-02-01..03 | 02 | 2 | DPRJ-02, 04 | capture + script | grep C9, C10, C-H before grant | ✅ | ✅ green |
| 25-03-01 | 03 | 3 | DPRJ-01..05 | doc | grep decided C8..C-H, K/V matrix, COVERAGE.md OPT-OUT | ✅ | ✅ green |
| 25-04-01..02 | 04 | 4 | DPRJ-05, 01, 02, 04 | contract + unit | `contract_deploy_projects projects_check_`, `tui_coverage`, `deploy::project`, `deploy_family_coverage` | ✅ | ✅ green |
| 25-05-01..02 | 05 | 5 | DPRJ-01, 02, 04, 05 | integration | `deploy_projects check_`, `readme_exit_table_agreement` | ✅ | ✅ green |
| 25-06-01..02 | 06 | 6 | DPRJ-01, 03 | unit + contract | `deploy::project_artifact`, `actions::projects`, `actions::deploy_projects` | ✅ | ✅ green |
| 25-07-01..02 | 07 | 7 | DPRJ-04 | contract + integration | `deploy::transport::local`, `deploy_projects git_` | ✅ | ✅ green |
| 25-08-01..03 | 08 | 8 | DPRJ-04 | unit + live (no-run) | `deploy::transport::sftp`, `deploy::transport::projects`, `live_project_host --no-run` | ✅ | ✅ green |
| 25-09-01..02 | 09 | 9 | DPRJ-02, 05 | contract + integration | `deploy::projects`, `deploy_projects`, `deploy::target` | ✅ | ✅ green |
| 25-10-01..02 | 10 | 10 | DPRJ-02 | contract + integration | `projects_check_parent`, `deploy_projects parent_`, `child_check_reads_parent_only` | ✅ | ✅ green |
| 25-11-01..02 | 11 | 11 | DPRJ-01, 03, 04, 05 | contract + integration | `projects_apply_`, `guarded_ops_registry_tracks_every_dispatch_site`, `write_half_unreachable` | ✅ | ✅ green |
| 25-12-01..02 | 12 | 12 | DPRJ-03, 05 | contract | `projects_apply_`, `deploy_family_coverage` | ✅ | ✅ green |
| 25-13-01..02 | 13 | 13 | DPRJ-03 | unit + live (no-run) | `deploy::transport::sftp`, `deploy::transport`, `write_half_unreachable` | ✅ | ✅ green |
| 25-14-01..02 | 14 | 14 | DPRJ-03 | contract + integration | `deploy::apply::preflight`, `deploy_apply designers`, `deploy_projects` | ✅ | ✅ green |
| 25-15-01..02 | 15 | 15 | DPRJ-02, 03 | contract + integration | `projects_apply_children`, `deploy_projects children_`, `sweep_only_reads` | ✅ | ✅ green |
| 25-16-01..02 | 16 | 16 | DPRJ-01 | contract + integration | `contract_deploy`, `deploy_plan`, `deploy_render/pull/verify/apply` | ✅ | ✅ green |
| 25-17-01..03 | 17 | 17 | DPRJ-03, 04, 05 | contract + integration + doc | `projects_canary`, `contract_stdout_purity`, `deploy_projects canary_`, README greps | ✅ | ✅ green |
| 25-18-01..02 | 18 | 18 | DPRJ-01, 05 | harness | `plan-test.sh`, `apply-test.sh`, `projects-test.sh`, `readme-test.sh` | ✅ | ✅ green |
| 25-19-01..03 | 19 | 19 | DPRJ-01..05 | live (no-run) + full suite | `live_deploy_projects --no-run`, capture greps, `cargo test --workspace`, clippy, no-default-features build | ✅ | ✅ green |
| 25-20-01..02 | 20 | 1 | DPRJ-03 | unit + live floors | `deploy::transport::winps`, `sftp_record_write_windows`, `live_transport_wflwvm01 race_` | ✅ | ✅ green |
| 25-21-01..03 | 21 | 1 | DPRJ-03 | unit + doc | `record_replace_`, `ps51_error_clixml_`, `release_writer_is_confined`, doc awk greps | ✅ | ✅ green |
| 25-22-01..02 | 22 | 1 | DPRJ-01, 05 | harness + doc | `release-test.sh`, RELEASING.md / installation.md greps | ✅ | ✅ green |
| 25-23-01..02 | 23 | 2 | DPRJ-03, 04, 05 | unit | `owner_acl`, `run_store` | ✅ | ✅ green |
| 25-24-01..03 | 24 | 3 | DPRJ-05 | unit + contract | `decide_follows_the_rule_order`, `adopt`, `contract_adopt`, `readme-test.sh` | ✅ | ✅ green |
| 25-25-01..02 | 25 | 4 | DPRJ-01, 05 | harness | `projects-test.sh`, `plan-test.sh`, `apply-test.sh` | ✅ | ✅ green |
| 25-26-01..02 | 26 | 5 | DPRJ-01, 05 | harness | `readme-test.sh`, `readme_exit_table_agreement` | ✅ | ✅ green |
| 25-27-01 | 27 | 6 | DPRJ-01..05 | harness + doc | `uat/*.yml` exist, G-25-13 capture heading, `readme-test.sh` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

### Requirement coverage

| Requirement | Automated evidence | Status |
|-------------|--------------------|--------|
| DPRJ-01 artifact verified by sha256 pin before upload | `deploy::project_artifact`, `projects_check_`, `deploy_plan`, harnesses | COVERED |
| DPRJ-02 parent-first import, cycles/unknown parents refuse, unchanged skipped | `deploy_projects parent_` / `check_` / `children_`, `projects_check_parent` | COVERED |
| DPRJ-03 export before overwrite, canonical-hash verify | `projects_apply_`, `deploy_projects canary_`, `deploy::transport` record write, `owner_acl` | COVERED |
| DPRJ-04 unlisted projects never deleted, `.git` refused | `deploy_projects git_`, `deploy::transport::local` / `sftp`, `projects_check_` | COVERED |
| DPRJ-05 `--check` makes no writes | `write_half_unreachable`, `sweep_only_reads`, `contract_stdout_purity`, `adopt`, `projects-test.sh` | COVERED |

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements.

---

## Manual-Only Verifications

These need a live gateway, ssh host or Windows VM. Their results are recorded in `25-LIVE-CAPTURES.md` and `25-UAT.md`. Offline, the suite compiles them (`--no-run`) and runs their floors.

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Live create/import/skip/refusal/children gates (`live_deploy_projects`) | DPRJ-01..05 | Needs a running 8.3 rig | `cargo test -p ignition-core --test live_deploy_projects -- --ignored` with the rig env set |
| `.git` probe and record write on ssh and Windows nodes (`live_project_host`) | DPRJ-03, 04 | Needs Linux ssh and Windows capture hosts | `cargo test -p ignition-core --test live_project_host -- --ignored` |
| Windows atomic record replace, race and blocked-replace (`live_transport_wflwvm01`) | DPRJ-03 | Needs ign-win-836 (IP changes on start) | 25-20 Task 3 and 25-21 Task 4 checkpoints |
| writePermissions admission on 8.3.6 / 8.3.3 (`live_adopt_write_admission`) | DPRJ-05 | Needs fresh gateways | `IGNITION_LIVE_ADOPT=1 cargo test -p ignition-core --test live_adopt_write_admission -- --ignored` |
| Host list grant and contract decisions | DPRJ-01..05 | Human decision checkpoints | 25-03 Tasks 2 and 3 |
| Cross-org GitHub Actions UAT | DPRJ-01..05 | Real GitHub runs and secrets | 25-27 Task 2; runs recorded in 25-UAT.md |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 25s (quick command)
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved 2026-10-29

---

## Validation Audit 2026-10-29

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 0 |
| Escalated | 0 |

Evidence at `ff841cc`: `cargo test --workspace` 2929 passed, 0 failed, 88 ignored (live). `cargo clippy --workspace --all-targets -- -D warnings` clean. `cargo fmt --all -- --check` clean. Harnesses `plan`, `apply`, `projects`, `readme`, `release` all exit 0. All capture and doc grep verifies match.

### Addendum 2026-10-29: gap-closure review fixes

The first audit missed that `25-REVIEW.md` (gap-closure review of 25-22..25-27) had 5 open warnings. `/gsd-code-review 25 --fix` closed WR-01, WR-02, WR-03 and WR-05 (`25-REVIEW-FIX.md`, status `partial`). WR-04 (release binary authenticity) is open pending an owner decision. Info findings IN-01..IN-06 are open.

| Finding | Fix commit | Automated evidence |
|---------|------------|--------------------|
| WR-01 adopt write probe orphans a minted key | 159d46a | `cargo test -p ignition-core --lib adopt` (403 retry, bounded give-up, persist-before-error) |
| WR-02 README remediation order | e3dd2b4 | `readme-test.sh` (docs-only change) |
| WR-03 artifact name collision | cf9e648 | `projects-test.sh` case 4i |
| WR-05 `--read-only` under default key name | 6786e9a, 71f11b9 | `adopt` unit test, `contract_adopt` (6 passed) |

Manual-only addition: the end-to-end `adopt()` persist-before-error path needs a live OIDC gateway (`live_adopt_write_admission`).

Evidence after the fixes at `71f11b9`: `cargo test --workspace` 2938 passed, 0 failed, 88 ignored. All five harnesses exit 0 with no FAIL lines.
