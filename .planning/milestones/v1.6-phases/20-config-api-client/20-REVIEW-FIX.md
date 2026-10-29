---
phase: 20-config-api-client
fixed_at: 2026-10-12T00:00:00Z
review_path: /Users/pmannion/whiskeyhouse/ignition-cli/.planning/phases/20-config-api-client/20-REVIEW.md
iteration: 1
findings_in_scope: 6
fixed: 6
skipped: 0
status: all_fixed
---

# Phase 20: Code Review Fix Report

**Fixed at:** 2026-10-12
**Source review:** .planning/phases/20-config-api-client/20-REVIEW.md
**Iteration:** 1
**Scope:** critical_warning (6 warnings; the 7 Info findings IN-01..IN-07 are out of scope and untouched)

**Summary:**
- Findings in scope: 6
- Fixed: 6
- Skipped: 0

**Verification location:** all gates ran in the main checkout on branch `main` (no worktree, per the project's shared-`CARGO_TARGET_DIR` rule). No Docker gateway was started; nothing below was run against a live gateway.

## Fixed Issues

### WR-01: Duplicate create and duplicate or illegal mode surfaced as `internal`

**Files modified:** `README.md`, `crates/ignition-core/src/error.rs`, `crates/ignition-core/src/client/deploy_api.rs`, `crates/ignition-core/tests/common/fake_deploy.rs`, `crates/ignition-core/tests/common/deploy_scenario.rs`, `crates/ignition-core/tests/deploy_client_contract.rs`, `crates/ignition-core/tests/live_deploy_api.rs`
**Commit:** 32446f4
**Status:** fixed: requires human verification (classification logic)
**Applied fix:** Two new typed errors, both exit 6, following the four-place rule: `resource_exists` (`CoreError::ResourceExists`) for the 409 on `resource_create` (D10), and `mode_exists` (`CoreError::ModeExists`) for the 409 on `mode_create` (D6). Each has its header-table entry, `code()`, `exit_code()`, `hint()`, `endpoint()`, a case in `exit_code_mapping_enumerated`, an `EXIT_SLUG_LITERALS` row and the README exit-6 row (also reworded the meaning cell). The mode 400 (illegal name) now maps to `invalid_input` (exit 2) with no page echo. All three are deploy-scoped (in `send_resource_write` and `mode_create`), so `classify` and the EAM routes are unchanged. The signature-name resolution was factored into `resource_name_for`, shared with `signature_rejected`.
Trace tag change: `create-duplicate:internal` is now `create-duplicate:resource_exists`. `FakeDeploy` returns the same typed errors, and the shared scenario gained a `mode-duplicate:mode_exists` step after `mode:created`, so the fake trace and the (next) live trace both carry it. `fake_scenario_trace_matches_the_capture_decisions` and the live gate's section C assertion were updated consistently. New wiremock test `mode_create_gateway_400_is_invalid_input`; existing duplicate tests now assert the slug and exit code.
**Remaining:** the live gate has not been re-run, so the new `mode-duplicate` step and the typed duplicate-create assertion are unproven against a gateway (both outcomes are capture-backed: D6, D10).

### WR-02: Unobserved 408 mapped to `scan_lock_held`

**Files modified:** `crates/ignition-core/src/client/classify.rs`, `crates/ignition-core/tests/deploy_client_contract.rs`
**Commit:** 962b1bc
**Status:** fixed: requires human verification (classification logic)
**Applied fix:** The scan-lock arm now matches only `409` (the capture-proven status). A 408 falls to the generic unexpected-status error and triggers no enrichment read. Doc and comments updated. The two enrichment tests now use an empty 409 instead of a 408, and a new test `lock_408_is_not_scan_lock_held` pins the new behaviour (not `scan_lock_held`, exactly one request). Trade-off recorded: if a real gateway ever does answer 408 on a timed-out acquire (the OpenAPI declares it; D1 never saw it), it is reported as `internal` until a capture shows it.

### WR-03: `from_record` sends fields no capture shows, and the gate does not settle it

**Files modified:** `crates/ignition-core/src/client/config_resources.rs`, `crates/ignition-core/tests/common/fake_deploy.rs`, `crates/ignition-core/tests/live_deploy_api.rs`
**Commit:** 59a8e97
**Status:** fixed (code and docs); live confirmation remaining
**Applied fix:** Corrected the `from_record` doc comment and the `FakeDeploy` header, which both claimed the 20-06 gate settles `collection` on a PUT and which copy a collection-less PUT updates. They now say no capture shows either, and the gate has not exercised them. Added a live section in `live_deploy_api.rs` section C (two copies present): one `from_record` update per copy, asserting the targeted copy's signature rotates and the other copy's is untouched, and logging (not asserting) whether the earlier collection-less PUT also changed the production copy.
**Remaining (needs a live run):** whether the gateway accepts `collection`, `enabled` and `backupConfig` in a PUT body, and which copy a collection-less PUT updates. The new section is written but has not been run. If it fails, it will have answered the question.

### WR-04: Live gate claims outrun what it exercises

**Files modified:** `.planning/phases/20-config-api-client/20-06-SUMMARY.md`, `crates/ignition-core/tests/live_deploy_api.rs`
**Commit:** 4e0821f
**Status:** fixed (wording and test extension); live confirmation remaining
**Applied fix:** Reworded the gate's module header and the 20-06 summary (frontmatter and headline) so they no longer say "every `DeployApi` method"; they list what ran and what did not. Extended the live gate with: a `name[eq]=<name>` filtered list asserting exactly one match, and `name[eq]=IGN_LIVE_NO_SUCH_NAME` asserting none (exercises the percent-encoded brackets D7 never saw); `resource_list_in_collection` for `core` and `production`; `resource_singleton` for the effective view and `core` on `ignition/security-properties`; and read-only `scan_status` and `scan_lock_info` on `ScanKind::Projects`. The header notes the recorded green run (`20-06-live-gate.log`) predates these sections.
**Remaining (needs a live run):** the percent-encoded-bracket filter behaviour (the unproven wire fact), the singleton read through the client, the composed per-collection list, and the `projects` scan kind. The client was left unchanged: no capture shows how the gateway reads `name%5Beq%5D=`, so changing the encoding would be inventing behaviour.

### WR-05: Tight live timing on a gateway that can stall about two seconds

**Files modified:** `crates/ignition-core/tests/common/deploy_scenario.rs`, `crates/ignition-core/tests/live_deploy_api.rs`
**Commit:** fac5790
**Status:** fixed
**Applied fix:** Live section A now holds the lock for 12 s (was 4), asserts `remaining` in `1..=12`, and polls expiry for up to 30 s. `ScenarioTiming::live()` uses `lock_hold_s: 12` and `expiry_budget: 30 s`. The fake timing is unchanged. Adds roughly 20 s to a live run. Not exercised live.

### WR-06: Endpoint table's method and path columns not tied to the wire

**Files modified:** `crates/ignition-core/src/client/deploy_api.rs`, `crates/ignition-core/tests/deploy_client_contract.rs`
**Commit:** f4f5f54
**Status:** fixed
**Applied fix:** New contract test `wire_matches_the_endpoint_table` runs every `DeployApi` method once against a catch-all wiremock and asserts the first request's verb and path equal its `DEPLOY_ENDPOINTS` row (templates filled), and that every declared row is exercised exactly once. The module doc now states that only `id` and `shape` are read at runtime and the other columns are pinned by this test. The existing tautological unit tests were left as they are.

## Skipped Issues

None.

## Gate results (main checkout, after the last commit)

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test --workspace` | pass: 72 test binaries, all `0 failed` (the `#[ignore]` live gate is not run) |
| `cargo build -p ignition-cli --no-default-features` | pass |

## Live confirmation remaining (needs one run of the live gate against the disposable 8.3.6 rig)

1. `name[eq]` filtered list with percent-encoded brackets returns the match (WR-04).
2. `from_record` PUT bodies (`collection`, `enabled`, `backupConfig`) are accepted, and each updates only its own copy (WR-03); the collection-less PUT's effect on the production copy is logged as `OBSERVED C: ...`.
3. `resource_singleton`, `resource_list_in_collection` and the `projects` scan kind work live (WR-04).
4. The typed `resource_exists` and `mode_exists` outcomes and the extended 12 s lock hold run green, and the fake and live traces still agree with the new `mode-duplicate:mode_exists` step (WR-01, WR-05).

Run command is in the header of `crates/ignition-core/tests/live_deploy_api.rs`. After a green run, append the new trace and results to the Live gate entry in `20-LIVE-CAPTURES.md` and refresh `20-06-live-gate.log`.

---

_Fixed: 2026-10-12_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
