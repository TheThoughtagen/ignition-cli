---
phase: 22-transport-plan-pull-verify
plan: 15
subsystem: deploy
tags: [verify, read-back, null-normalisation, gap-closure, G-22-2, live-rig]

requires:
  - phase: 22-transport-plan-pull-verify
    provides: deploy_verify / check_read_back (22-10), deploy_pull (22-08), CV2 decision table (22-09)
provides:
  - "verify.rs: symmetric N1 (JSON null equals absent on both the file and the REST side) in check_read_back and diff_attributes"
  - "regression tests at engine, action and binary level for REST-side nulls, plus guard tests that a one-sided null against a value still fails"
  - "live_deploy_plan.rs: pull_then_verify_is_green_live"
affects: [22-16, phase-22-reverify]

actuals:
  tokens: 34700
  tasks: 3
  commits: 5

tech-stack:
  added: []
  patterns:
    - "N1 is applied to both operands before diff_exact / diff_subset; the N3 key lookup stays on the raw REST map so a REST null cannot hide a declared attribute"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/deploy/verify.rs
    - crates/ignition-core/tests/deploy_verify.rs
    - crates/ignition-cli/tests/contract_deploy.rs
    - crates/ignition-core/tests/live_deploy_plan.rs
    - README.md
    - .planning/phases/22-transport-plan-pull-verify/22-LIVE-CAPTURES.md

key-decisions:
  - "N1 is symmetric: null members are dropped from the file value and from the REST value at every depth; no per-type exemption, no ignored-key list, diff_exact / diff_subset / same_scalar untouched"
  - "diff_attributes (N3) looks each key up in the RAW REST map and strips nulls only from the two values compared"

requirements-completed: [DPLN-07, DPLN-06]

duration: 2h 25min
completed: 2026-10-15
status: complete
---

# Phase 22 Plan 15: Symmetric N1 for verify read_back (G-22-2) Summary

**`deploy verify` now treats a JSON null as absent on both sides, so a tree from `deploy pull` verifies green against its own gateway; proven on the live 8.3.6 rig through the library and the installed `ign` binary.**

## What was done

- **Task 1 (tracer).** RED `90920e6` (cobranding-shape engine test, renamed N1 test, action-level `verify_green_when_rest_echoes_file_nulls`; three failed for the stated reason). GREEN `ba00845`: `check_read_back` null-strips the REST config before `diff_exact` and `diff_subset`; `diff_attributes` strips both compared values and keeps the key lookup on the raw map.
- **Task 2.** `6a60a05` guard tests: REST null vs file value fails, file null vs REST value fails (both name the path, never the value), N2 and N3 equivalents, plus `verify_cli_green_when_rest_echoes_file_nulls` (new `rest_keeps_nulls` parameter on `mount_verify_gateway`, 3 existing callers pass `false`). `4353824` docs: verify.rs module doc and `Compare::Exact` doc, README CV2 paragraph, 22-LIVE-CAPTURES CV2 table (own `ignition/cobranding` row, POSTGRES row reworded, dated amendment bullet).
- **Task 3.** `bc6bb24` `pull_then_verify_is_green_live`.

## Verification

- `cargo test --workspace`: 2148 passed, 0 failed. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` clean.
- Plan grep checks: the old one-sided phrases are gone from verify.rs, README and the CV2 table; `REST view reports it (CV2 N1)` is gone from both test files; README mentions cobranding; the cobranding row exists.

### Live gate (rig `ign-deploy-rig`, Ignition 8.3.6, port 18290; log `$HOME/.cache/ign-deploy-rig/g22-2-live.log`)

Preflight passed (docker up, port free, no rig, `cargo install --force` ok, tokens.env present, `IGN_TOKEN_CAP_A` non-empty, StatusPing RUNNING).

Library, run 1 (read-only, fresh rig), all PASS:
- `pull_then_verify_is_green_live`: REST cobranding had 9 null members; pull wrote 91 files, cobranding at `gateways/rig-gw/resources/ignition/cobranding/config.json`; `pulled: ok true slug None exit None resources_checked 28`; scan, mode, read_back, files, loaded pass; probes warn (`tag_probes_absent`, expected); no finding on `ignition/cobranding`. After the repo-side edit of `appIcon` to a string: `ok false slug deploy_verify_failed exit 6`, read_back `resource_differs:ignition/cobranding (differs at: appIcon)`.
- `pull_render_plan_is_empty_live`: 91 files written, 7 skipped, plan empty.
- `plan_takes_no_lock_live`: lock free before and after.

Library, run 2 (stated write exception, last and alone): `verify_green_then_red_live` PASS (green ok true, resources_checked 2; red exit 6 with `unloaded_resource` for VERIFY_NOUUID).

Installed binary (`ign 1.5.0`, fresh rig, scratch config outside the repo): `ign --json deploy pull` exit 0 (91 files); `ign --json deploy verify` exit 0, envelope ok true, `data.ok` true, `slug` null, `resources_checked` 28; scan pass, mode pass, read_back pass, files pass, loaded pass, probes warn. Setting one null cobranding member (`appIcon`) to a string in the temp repo: exit 6, `data.ok` false, `deploy_verify_failed`, read_back `resource_differs:ignition/cobranding (differs at: appIcon)`.

Rig torn down (`down -v`, no `ign-deploy-rig` containers left); scratch directory removed.

## Deviations from Plan

**1. [Rule 3 - Blocking] Binary gate re-run on a freshly rebuilt rig.**
- **Found during:** Task 3, step 4.
- **Issue:** The first binary verify ran after `verify_green_then_red_live` (run 2). That test writes the tracer `database-translator/POSTGRES` into the `production` collection and deletes it from disk, but the gateway keeps the override in memory until its next scan. REST then reported the tracer POSTGRES, so read_back reported `resource_differs` on `ignition/database-translator/POSTGRES` against the pulled core POSTGRES. This is stale live state left by the stated write exception, not a defect in the fix. A retry some minutes later showed the same.
- **Fix:** Tore the rig down (first attempt used `docker compose down -v` without the env that `capture22.sh` exports and failed; reran with `IGN_DEPLOY_RIG_RESOURCES` and `IGN_DEPLOY_RIG_STATE` set), ran `up22` and `setup22` again (stated writes), and ran the binary gate on the fresh rig before anything else wrote to it. The library runs were unaffected and had already passed on the first rig.
- **Follow-up for the plan/verifier:** a binary gate must run before `verify_green_then_red_live`, or the rig must be rescanned after it. Worth noting in the 22-16 or re-verify run order.

**2. Tracer feedback gate handled without a human checkpoint.** `AUTO_CHAIN` and `AUTO_CFG` are both false, so the interactive rule asks for a `checkpoint:human-verify` after the tracer. The plan is `autonomous: true`, `human_verify_mode` is `end-of-phase`, and the tracer's verify is fully automated and passed (49 passed, then 51 with the guard tests). I continued and recorded the verification here.

**3. Added `has_null` shape preconditions** to `verify_green_when_rest_echoes_file_nulls` and `verify_cli_green_when_rest_echoes_file_nulls` so they cannot pass vacuously if the tracer render stops carrying null members (not in the plan, same intent as the live precondition (a)).

## Auth gates

None.

## Known Stubs

None.

## Threat Flags

None. T-22-66 (guard tests pin one-sided nulls as failures; N3 lookup on the raw map), T-22-67 (findings carry paths only, asserted against the report and Debug output) and T-22-68 (the live red half is a repo-side edit; the only rig writes were `setup22` and `verify_green_then_red_live`) are mitigated as planned.

## Notes

- 22-UAT.md not edited (the re-verify pass owns it).
- `.planning/config.json` and `.planning/research/.cache/` were not staged.

## Self-Check: PASSED

Commits `90920e6`, `ba00845`, `6a60a05`, `4353824`, `bc6bb24` exist; modified files present; `pull_then_verify_is_green_live` is listed by `--ignored --list`.
