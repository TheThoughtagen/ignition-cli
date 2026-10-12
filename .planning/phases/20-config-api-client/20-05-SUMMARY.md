---
phase: 20-config-api-client
plan: 05
subsystem: api
tags: [deployment-mode, deploy-api, fake-gateway, differential-scenario, wiremock, ignition-8.3.6]

requires:
  - phase: 20-01
    provides: "20-LIVE-CAPTURES.md blocks C-null, C-mode and Decisions D1-D12, Endpoint shapes table"
  - phase: 20-03
    provides: "DeployApi, DEPLOY_ENDPOINTS, send_shaped, scan.rs lock_verdict and scan_complete"
  - phase: 20-04
    provides: "config_resources.rs validate_segment, ResourceRecord/Create/Update, MutationOutcome"
provides:
  - "client/mode.rs: MODE_PATH, ModeRecord, ModeCreate, GatewayModeProbe (pub(crate)), normalize_deployment_mode"
  - "DeployApi::{modes, mode_create, deployment_mode} at the end of the trait; DEPLOY_ENDPOINTS closed at 13 rows in trait order"
  - "tests/common/fake_deploy.rs: FakeGateway + FakeDeploy, one stateful in-memory gateway implementing every DeployApi method (reusable by Phase 23)"
  - "tests/common/deploy_scenario.rs: scenario(a, b, a_token_name, suffix, timing) -> Vec<String>, ScenarioTiming::{fake, live} for the 20-06 live diff"
affects: [20-06, 22, 23, 24]

tech-stack:
  added: []
  patterns:
    - "Private probe struct reads one key from a frozen wire type, so GatewayInfo and its --json output stay untouched"
    - "State-changing call confirmed by a follow-up read (mode_create then modes), never by the 2xx"
    - "Test double calls the library's own pure verdict functions instead of re-deriving them; every behaviour cites a capture Decision"
    - "Differential scenario returns tags and never panics, so one diff shows every fake-vs-live divergence"

key-files:
  created:
    - crates/ignition-core/src/client/mode.rs
    - crates/ignition-core/tests/common/fake_deploy.rs
    - crates/ignition-core/tests/common/deploy_scenario.rs
  modified:
    - crates/ignition-core/src/client/deploy_api.rs
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/tests/common/mod.rs
    - crates/ignition-core/tests/deploy_client_contract.rs

key-decisions:
  - "deployment_mode() returns Option<String>; null, empty, absent and whitespace-only all read as None, any other string verbatim (D5: 8.3.6 sends \"\" with no mode, \"production\" with one; null and absent unobserved)"
  - "GatewayInfo not edited; deploymentMode is read through the private GatewayModeProbe. The v1.0 golden body's STANDARD is not capture-backed and is treated as an ordinary mode name"
  - "mode_create validates the name with validate_segment before any request, POSTs once, ignores the 200 body (EmptyOk, D6) and returns the item found by a follow-up modes() read; absent from the list is internal naming mode_create and the mode"
  - "A duplicate mode (409 HTML) or illegal name (400 HTML) needs no new classify arm: both reach classify's fallback and are internal with the Jetty message, which is enough for never-Ok"
  - "FakeDeploy reports a duplicate create and a duplicate mode as internal, the code the real client yields for a 409 outside the scan-lock route"

patterns-established:
  - "Fake lock reads count down one gateway second per info read: hold 3 reads 3, 2, 1, then free (the live gateway reads hold-1 right after an acquire; the trace compares held-or-free only)"

requirements-completed: [DAPI-05, DAPI-06]

duration: 70min
completed: 2026-10-12
status: complete
actuals:
  tokens: 19200
  tasks: 2
  commits: 2
---

# Phase 20 Plan 05: Deployment modes, closed endpoint table and the stateful fake gateway Summary

**`DeployApi` now lists and creates deployment modes (create confirmed by read-back) and reads the active `deploymentMode` with null, `""`, absent and blank all meaning no mode; the endpoint table is closed at 13 capture-backed rows; and `FakeDeploy` is one stateful in-memory gateway built on the library's own verdict functions, with a shared scenario ready for the 20-06 live diff.**

## Task Commits

1. **Task 1: deployment modes, active deploymentMode, 13-row endpoint table** - `f990d2e` (feat)
2. **Task 2: stateful FakeDeploy gateway and shared differential scenario** - `af765e7` (test)

## What was built

- `mode.rs`: `ModeRecord` (lenient, `resourceCount` typed `Option<i64>`), `ModeCreate` (serialized `name, title, description`), `GatewayModeProbe`, `normalize_deployment_mode`. No D5 no-mode sentinel string exists, so none is special-cased.
- `deploy_api.rs`: `modes` (GET `/mode?limit=-1&offset=0`), `mode_create`, `deployment_mode`, three rows appended (C-mode, C-mode, C-null). `deploy_endpoints_cover_every_method` pins 13 rows in trait order, unique ids, non-empty capture refs, GET/POST/PUT only. The earlier row test and id test were extended to 13. Every `capture_ref` matches a `## ` heading in 20-LIVE-CAPTURES.md (grep check passes).
- `fake_deploy.rs`: `FakeGateway { new, client, tick, set_deployment_mode, seed_singleton }` and `FakeDeploy` (`impl DeployApi`). Calls `lock_verdict` (lock free or not), `scan_complete` (when a triggered scan ends), `normalize_deployment_mode` (active mode) and `validate_segment` / `resource_find_path` / `list_query_pairs` (segment and filter guards). Each behaviour cites its Decision (D1, D2, D3, D4, D5, D6, D7, D9, D10, D12).
- `deploy_scenario.rs`: the 13-step scenario on `ScanKind::Config`, `ScenarioTiming::fake()` (1 ms poll, hold 3, 1 s budget, scan 1 ms / 500 ms) and `live()` (1 s poll, hold 4, 15 s budget, scan 500 ms / 60 s). An unexpected Err becomes `<step>:err:<code>`; consecutive identical tags collapse.

## Expected scenario trace (for 20-06 to diff against)

`fake_scenario_trace_matches_the_capture_decisions` pins this literal. 20-06 runs `scenario(.., ScenarioTiming::live())` against the live rig with tokens `cap-a` and `cap-b` and compares.

| Tag | Fixed by |
|---|---|
| `lock:acquired` | D1 (acquire answers at once), D9 |
| `lock:held-by-a` | D9 / C1c: the info read names `api-token:cap-a:...` |
| `lock:refused-held-by-a` | D1 / C1e: immediate 409 naming the holder |
| `lock:free` | D9: 204 after `holdTimeout` lapses |
| `scan:complete` | D4: a no-op scan advances `lastScanTimestamp` |
| `create:ok` | D10: `[{name, config}]` answers 200 `success:true` |
| `find:ok` | C3a / C-list: the effective view carries a signature |
| `update:ok` | D3 / D10: the current signature is accepted |
| `update:stale` | D3: the pre-write signature is stale after a write |
| `create-duplicate:internal` | D10: 409 JSON problem, `internal` client-side |
| `mode:created` | D6: 200 and the list confirms it |
| `modes:listed` | D6 / C-mode |
| `deployment-mode:none` | D5 + T-20-26: a defined mode is not an active mode |

Divergences 20-06 should expect or settle: the scenario builds `ResourceUpdate::new`, so no `collection` goes out on the PUT, and the captures never show which copy a collection-less PUT targets; the fake updates the effective copy. If the live gateway refuses it, the trace shows `update:err:<code>` and 20-06 decides. The live rig runs with no mode unless 20-06 recreates it, so `deployment-mode:none` assumes the no-mode rig.

## Deviations from Plan

None - plan executed as written. Two choices inside the plan's discretion:

- The fake's lock countdown reads `holdTimeout` first (3, 2, 1) as the plan's behaviour block specifies, although the live gateway reads `holdTimeout - 1` right after an acquire (D9). The scenario never compares the number.
- The 409 (duplicate mode) and 400 (illegal name) answers got no new classify arm. Both land in classify's fallback as `internal` carrying the Jetty message, which satisfies "an error, never Ok" and the D6 note that a typed outcome is wanted only if cheap; a typed variant would touch `error.rs` and was left out of scope.

## Verification

- `cargo fmt --all --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo build --workspace` and `cargo build -p ignition-cli --no-default-features`: ok.
- `cargo test --workspace`: 1542 passed, 0 failed, 40 ignored (live-gated).
- Plan-scoped: `client::mode` 4 tests, `client::deploy_api` 4 tests, `deploy_client_contract mode_` 9 tests, `deploy_client_contract fake_` 10 tests, `gateway_info_contract` 12 tests, all green. `git log -- version.rs` shows no 20-05 commit. No `src/deploy` directory. No `preserve_order` or `arbitrary_precision` in any Cargo.toml.

## Known Stubs

None.

## Threat Flags

None. The new surface (mode POST, gateway-info read) is the one the plan's threat model covers (T-20-22 to T-20-26): name validated before any request, create confirmed by read-back, no-mode forms normalized, fake reuses production functions, no method activates a mode.

## Self-Check: PASSED

- FOUND: crates/ignition-core/src/client/mode.rs
- FOUND: crates/ignition-core/tests/common/fake_deploy.rs
- FOUND: crates/ignition-core/tests/common/deploy_scenario.rs
- FOUND commit f990d2e
- FOUND commit af765e7
