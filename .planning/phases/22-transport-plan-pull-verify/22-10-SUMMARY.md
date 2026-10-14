---
phase: 22-transport-plan-pull-verify
plan: 10
subsystem: deploy
tags: [verify, read-back, log-scrape, tag-probes, verdict-envelope, forbid-writes, live-rig]

requires:
  - phase: 22-transport-plan-pull-verify
    provides: Declared/load_declared, TreeReader and LocalFsTransport, plan(), rig decisions CV1-CV3 (22-01, 22-08, 22-09)
provides:
  - "deploy/verify.rs: pure six-check engine (evaluate, Expected, ObservedState, VerifyReport, Check, Finding)"
  - "actions/deploy.rs: VerifyOptions, DeployVerifyResult with failure_exit_code (Some(6) when red), deploy_verify"
  - "tests/common/forbid_writes.rs: ForbidWrites<T: DeployApi>"
  - "live_deploy_plan.rs: verify_green_then_red_live on the 8.3.6 rig"
affects: [22-11, 22-14, 23]

actuals:
  tokens: 28300
  tasks: 3
  commits: 5

tech-stack:
  added: []
  patterns:
    - "Pure evaluate over an ObservedState the action gathers; the action reads, never judges"
    - "Value-free by construction: LogLine, ProbeReading, Remote and Compare have hand-written Debug that withholds message, quality text and config"
    - "A refusal of an optional evidence source (log, tags) is evidence carrying only error.code(); a refusal of the primary reads is a CoreError"

key-files:
  created:
    - crates/ignition-core/src/deploy/verify.rs
    - crates/ignition-core/tests/deploy_verify.rs
    - crates/ignition-core/tests/common/forbid_writes.rs
  modified:
    - crates/ignition-core/src/deploy/mod.rs
    - crates/ignition-core/src/actions/deploy.rs
    - crates/ignition-core/tests/common/mod.rs
    - crates/ignition-core/tests/live_deploy_plan.rs

key-decisions:
  - "CV1 window as decided by the rig: startTime = lastScanTimestamp - lastScanDuration - 1000 ms (LOG_WINDOW_SLACK_MS = 1000, not the plan's 30 s), WARN, limit 200, no endTime, no logger filter, matched client-side; LOG_SETTLE_MS = 5000"
  - "CV2 normalisation N1, N2, N3 plus the system-collection and no-read-route skips implemented as listed in the captures"
  - "CV3: tag-unit files compared post-scan directly with the rendered bytes through plan()"
  - "loaded is best effort: warn (not observed) while a scan runs, fail logs_unavailable when the log cannot be read or is truncated, and the pass detail states it covers the last scan only"
  - "deploy_verify_failed is a constant in verify.rs, not a CoreError variant; DEPLOY_SLUGS and the exit table are unchanged"

patterns-established:
  - "Quality text is reduced to a code (class plus a letters-only suffix) before it reaches a finding or Debug"
  - "Gateway log settle wait is skipped when the gateway has logged anything after the settle point"

requirements-completed: []
requirements-advanced: [DPLN-07, DPLN-08, DPLN-09]

duration: 70min
completed: 2026-10-14
status: complete
---

# Phase 22 Plan 10: Verify core (six checks, verdict, live rig) Summary

**`deploy_verify` reads scan, mode, REST read-back, tag-unit files, the WARN log window and tag probes on the master without writing, and returns a value-free `DeployVerifyResult` whose red form carries `deploy_verify_failed` and `failure_exit_code() == Some(6)`; on the 8.3.6 rig a scanned tree verifies green and a declared connection with no live uuid verifies red with `unloaded_resource`.**

## Performance

- **Duration:** about 70 min (14:20 to 15:31 MDT)
- **Tasks:** 3
- **Files:** 7 (3 created, 4 modified)

## Accomplishments

- **Pure engine** (`deploy/verify.rs`). `evaluate(&Expected, &ObservedState) -> VerifyReport` always returns six checks in the fixed order `scan, mode, read_back, files, loaded, probes`. `ok` is false exactly when a check is `fail`; `warn` never fails the verdict. Findings are byte-sorted by subject (probes keep manifest order), so identical inputs give byte-identical JSON.
- **Action** (`actions/deploy.rs`). `deploy_verify(declared, project, deploy, gateway, reader, opts)` takes a `DeployApi`, a `GatewayApi` and a `TreeReader`. It calls only read methods. A red result is `Ok(..)` with `ok: false`; the binary maps it to exit 6 through `failure_exit_code()` in 22-11.
- **`ForbidWrites<T>`** panics on `scan_lock_acquire`, `scan_request`, `resource_create`, `resource_update`, `mode_create`; every action test runs through it, and a `should_panic` test proves the double fires.
- **Live proof** on `inductiveautomation/ignition:8.3.6`: green, then red (output below).

## The check design, per check

| # | id | source | passes when | finding codes |
|---|----|--------|-------------|---------------|
| 1 | `scan` | `scan_status(Config)` | `scanActive` is false | `scan_active` |
| 2 | `mode` | `deployment_mode()`, normalised | equals `target.mode` exactly (no mode is a mismatch) | `mode_mismatch` |
| 3 | `read_back` | one `resource_list(limit -1)` per declared named type, `resource_singleton` per singleton | every declared, comparable resource is present with an equal `config` (CV2 table); a 404 on a singleton or on a type's list route is "missing" | `resource_missing`, `resource_differs` (JSON paths only) |
| 4 | `files` | `reader.read(scope)` then `plan(tree, live, None, scope)` | every declared tag unit has plan status `same` (live effective copy byte-equal, CV3) | `files_differ` (unit key and status word) |
| 5 | `loaded` | `logs()` with `minLevel=WARN`, `startTime` per CV1, `limit=200`, paged up to 5 pages | no line contains `will not be loaded` and `Resource <declared id> ` | `unloaded_resource` (resource id, timestamp, logger), `logs_unavailable` |
| 6 | `probes` | `tags_read` polled every `probe_interval` to `probe_deadline` | every `verify.tag_probes[]` path reads quality starting `Good` | `tag_probe_not_good`, `tag_probes_unavailable`, `tag_probes_absent` (warn) |

Details that are not obvious from the table:

- **scan.** Only idleness is checked. A standalone verify cannot trigger a scan.
- **read_back vacuous.** No comparable resource gives `pass`, `resources_checked: 0` and the warning `read_back: no declared non-tag resource was compared`.
- **read_back skips.** A REST item whose effective collection is `system`, and the types in `NO_READ_ROUTE` (`com.inductiveautomation.opcua/one-time`, `ignition/images`, not even queried), are warnings and are not counted in `resources_checked`.
- **files.** The transport is read on every run, also when no tag unit is declared, so an unreachable host is an error and not a silent green. No tag units gives `pass`.
- **loaded.** Duplicate lines for one resource count once (earliest timestamp). When two declared ids both prefix a line (`Prod`, `Prod Backup`), the longest wins it. While a scan runs the check is `warn` (not observed) and the log is not read. A truncated log (5 pages full) is `logs_unavailable` with detail `logs_truncated`.
- **probes.** Absent or empty list: `warn` with `tag_probes_absent`, never `pass`, and `ok` is unaffected. A declared probe with no reading is `tag_probe_not_good` (`no reading`). An unreadable route is `tag_probes_unavailable` carrying only the slug (for example `routes_not_deployed`). The last poll is judged: a probe that turns Good on the last poll passes.

## CV1 window and CV2 table as implemented

**CV1.** `log_window_start(scan) = max(0, lastScanTimestamp - lastScanDuration - LOG_WINDOW_SLACK_MS)` with `LOG_WINDOW_SLACK_MS = 1000`, gateway clock only. A missing `lastScanDuration` falls back to `LOG_WINDOW_FALLBACK_MS = 30000` (wider, never narrower). Before the WARN read the action waits `log_settle` (default `LOG_SETTLE_MS = 5000`, measured from the scan-status read) unless one cheap `limit=1` query finds a log entry stamped after `lastScanTimestamp + settle`, which proves the gateway has logged since. The query never sends `endTime` or `logger` (the route's `logger` is not a prefix filter, CV1b).

**CV2.** Equality is JSON value equality (key order ignored, `1` equals `1.0`). Paths are dotted (`a.b.0.c`); arrays of named objects in N2 are `path[name]`; up to 20 paths per finding, sorted.

| Rule | Implementation |
|---|---|
| N1 | `strip_nulls` drops every object member whose file value is `null`, at every depth, before an exact or subset comparison. A REST-only `null` member still differs |
| N2 | `ignition/security-levels`: `Compare::NameKeyedSubset`; every object of the file matched by `name` must appear in REST with its members; extra REST levels are fine |
| N3 | unit with no `config.json`: `Compare::Attributes(resource.json attributes)`; only the attribute keys the REST `config` also carries are compared, REST-only keys (`terms`) ignored |
| skip | effective collection `system`: warning, not compared |
| skip | `NO_READ_ROUTE` types: warning, not queried |

## `DeployVerifyResult` key order

`env, gateway, mode, node, project, ok, slug, checks, resources_checked, warnings`. `slug` is `deploy_verify_failed` only when `ok` is false, else null. Each check serializes as `{id, status, detail, findings:[{code, subject, detail}]}`. `failure_exit_code()` returns `Some(6)` when `!ok`.

## Live rig run (Task 3)

Rig: `capture22.sh up22` then `setup22` (image `inductiveautomation/ignition:8.3.6`, gateway `ign-deploy-rig`, mode `production`, bind-mounted `config/resources`), then:

```text
IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 IGNITION_LIVE_TOKEN="$IGN_TOKEN_CAP_A" \
  IGN_DEPLOY_RIG_DATA="$HOME/.cache/ign-deploy-rig/data" IGN_DEPLOY_RIG_MODE=production \
  cargo test -p ignition-core --test live_deploy_plan -- --ignored --nocapture
```

```text
pull: 95 files written, manifest generated, 7 skipped
plan: added 0 changed 0 removed 0 same 48 unmanaged 1 has_changes false
PASS pull_render_plan_is_empty_live
PASS plan_takes_no_lock_live: lock free before and after plan
green: ok true slug None exit None resources_checked 2
  Scan Pass []   Mode Pass []   ReadBack Pass []   Files Pass []   Loaded Pass []
  Probes Warn [tag_probes_absent:verify.tag_probes (...)]
red: ok false slug Some("deploy_verify_failed") exit Some(6) resources_checked 3
  Scan Pass []   Mode Pass []
  ReadBack Fail [resource_differs:ignition/database-connection/VERIFY_NOUUID (differs at: connectionProps, connectionResetParams, defaultTransactionLevel, evictionRate, evictionTests, evictionTime, failoverMode, includeSchemaInTableName, poolInitSize, poolMaxIdle, poolMaxWait, poolMinIdle, slowQueryLogThreshold, testOnBorrow, testOnReturn, testWhileIdle, validationSleepTime)]
  Files Pass []
  Loaded Fail [unloaded_resource:ignition/database-connection/VERIFY_NOUUID (logged at 1792012964219 by `NamedResourceHandler-ignition/database-connection`)]
  Probes Warn [tag_probes_absent:...]
PASS verify_green_then_red_live
test result: ok. 3 passed; 0 failed; 0 ignored
```

Two full runs passed (the first one with only the verify test, the second with all three). The test places the rendered tree with plain `std::fs` writes (world-readable, no `chown` was needed), scans as test code, and removes what it wrote when it ends.

**Teardown:** `docker compose -p ign-deploy-rig down -v` exited 0 (container, volume and network removed; afterwards 0 containers, 0 volumes, 0 networks carry the project label and port 18290 refuses). `$HOME/.cache/ign-deploy-rig/{data,render22,cv2-bodies,tokens.env}` were removed with `/bin/rm -rf`. The rig is not handed to 22-11.

## Task Commits

1. **Task 1 RED:** `378ff88` (test: failing engine tests over a stub `evaluate`)
2. **Task 1 GREEN:** `62d278e` (feat: six-check engine, plus 4 lib unit tests for the private helpers)
3. **Task 2 RED:** `0702fea` (test: `ForbidWrites`, `VerifyOptions`/`DeployVerifyResult` and 16 action tests over a stub `deploy_verify`)
4. **Task 2 GREEN:** `7dbecf1` (feat: read-only `deploy_verify`)
5. **Task 3:** `d6ef90f` (test: `verify_green_then_red_live`, `#[ignore]`)

## Verification

- `cargo test -p ignition-core --test deploy_verify`: 45 passed (29 engine, 16 action). `cargo test -p ignition-core --lib deploy::verify`: 4 passed. `cargo test -p ignition-core --lib deploy::`: 201 passed. `cargo test -p ignition-core --lib error::`: 12 passed (the exit tables are unchanged).
- `cargo test --workspace`: 2076 passed, 0 failed, 44 ignored (2027 before, plus 4 lib, 45 `deploy_verify`; the 44th ignored test is the new live one).
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check`: clean.
- `PLAN_GOLDEN` and `PLAN_GOLDEN_LEDGER` untouched (`deploy_plan` 57 passed); `write_half_unreachable` green (the new code in `actions/deploy.rs` names no `TreeMirror`, `mirror_for_node`, `StagingMode`, `.stage(` or `.commit(`).
- Build environment: private `CARGO_TARGET_DIR=~/Library/Caches/cargo-target-ign-cli`; no worktree.

## Edge coverage

| Edge | Evidence |
|---|---|
| DPLN-07 adjacency | `read_back_key_order_ignored`, `read_back_reports_paths_not_values`, `read_back_n1_null_members_of_the_file_are_dropped` |
| DPLN-07 empty | `read_back_vacuous`, `logs_unavailable_fails`, `verify_logs_refusal_is_unavailable_not_error` |
| DPLN-07 ordering | `deterministic_ordering`, `checks_are_always_six_in_fixed_order`, `report_serializes_in_contract_order` |
| DPLN-08 adjacency | `probe_quality_prefix`, `verify_probe_that_turns_good_before_the_deadline_passes` |
| DPLN-08 empty | `probes_absent_is_warn_not_pass`, `probes_unavailable_fails`, `verify_probe_route_absent_is_unavailable` |
| DPLN-08 ordering | `probe_findings_in_manifest_order` |
| DPLN-09 (flagged assumption) | `verify_unreachable_gateway_is_an_error` and `verify_transport_failure_is_an_error` pin that an unreachable gateway or host is a `CoreError`, not a verdict |
| Secret canary (SC-5, verify half) | `no_message_or_value_echo` (engine, serialized and Debug), `verify_log_hit_for_a_declared_resource_is_red_and_value_free`, `verify_logs_refusal_is_unavailable_not_error` (403 body not echoed), probe tag value never serialized |

## Deviations from Plan

None of these needed a checkpoint; all follow "the code and the captured decisions win over the plan sketch".

**1. [Judgement - CV1] `LOG_WINDOW_SLACK_MS = 1000`, not `30_000`.** The plan allowed "or the CV1 decision's value". Added `LOG_WINDOW_FALLBACK_MS = 30000` (used only when the gateway omits `lastScanDuration`) and `LOG_SETTLE_MS = 5000`. `VerifyOptions` gained a third field, `log_settle`, so tests can set it to zero; the plan had two fields.

**2. [Judgement - CV1] `loaded` can be `warn`.** While a scan runs the check answers `warn` (not observed) and the log is not read; the plan's six tests only fail one check each, so this does not disturb them (`each_check_fails_alone` asserts the other checks are not `fail`). A clean log only proves the last scan did not warn, so the pass detail says so. No new finding code was added: a log that cannot be read (refusal or truncation) is `logs_unavailable`, with `logs_truncated` as the detail.

**3. [Judgement] `ObservedState.read_back` is `BTreeMap<String, Remote>`** (`Missing`, `Config(Value)`, `System`), not `Option<Value>`, so the CV2 `system` skip is representable. `Expected` is richer than the sketch: `resources: Vec<ExpectedResource>` (with `Compare::{Exact, NameKeyedSubset, Attributes}`), `not_checked`, `declared_ids`, `tag_units`, `probes`. `ObservedState.scan` is a `ScanStatus` as planned; `deploy/verify.rs` is the second `deploy` module to import from `client` (`sftp.rs` already does).

**4. [Judgement] Which refusals are verdict input.** Only the log and tag sources downgrade a non-network error to `*_unavailable`. A refusal of `scan_status`, `deployment_mode`, `resource_list` or `resource_singleton` (other than 404) propagates as a `CoreError`, because there is no evidence to judge. The plan's text says "any other refusal from the logs or tags source"; this makes the converse explicit.

**5. [Judgement] The transport is read even without tag units** (so a dead host is an error, which `verify_transport_failure_is_an_error` needs because the tracer repo has no tag units).

**6. [Judgement] Quality text never reaches a finding.** `quality_code` keeps `Good|Bad|Uncertain|Error` plus a letters-only suffix (`Bad_NotFound`); anything else is `unrecognised`. `ProbeReading`'s `Debug` uses the same reduction.

**7. [Judgement] Four unit tests in `verify.rs`** for the private helpers, so `cargo test --lib deploy::verify` is not empty.

**Total deviations:** 7 judgement calls, 0 bugs, 0 architectural changes.

## Issues Encountered

- The fake gateway's effective view only knows the active mode and `core`, so a `system`-collection singleton could not be seeded through it. The `system` skip is therefore tested at the action level through the real `ReqwestGatewayApi` against wiremock (which also exercises the real DeployApi wire for scan, mode, list and singleton), and at the engine level directly.
- `serde_json::Value` objects sort keys, so the contract-order test checks the raw serialized string, not a parsed value.

## Known limitations and residuals

- **`loaded` is bounded evidence.** The gateway logs the line once per scan that finds the resource (CV1c) and not for every type (a uuid-less `database-translator` is silent). A later verify after another scan cannot see an earlier line. Render mints uuids, so the line is belt and braces; `read_back` cannot catch it either (the REST view lists an unloaded resource, CV1b).
- **Prefix ambiguity.** A line for an undeclared `Prod Backup` can be attributed to a declared `Prod` (the match is `Resource <id> `). The error is on the safe side (false red) and needs a declared name that is a prefix of another resource's name.
- **`database-connection` read-back (new fact for 22-14).** A hand-written, minimal `database-connection` `config.json` differs from the REST view in 17 members (`connectionProps`, `poolInitSize`, `evictionRate`, and so on): the gateway fills defaults for members the file omits. A pulled tree carries the full REST config, so it compares equal. This is the Pitfall 6 case CV2 said was not exercised; the remedy is a per-type row in the table (for example "REST-only members are defaults"), decided on the prod-sized payload in 22-14, not looser equality here.
- Tag probes were exercised through wiremock only; the rig has no `ign-cli` WebDev bundle deployed (the live run shows `tag_probes_absent`). A live probe needs the bundle (research A8).
- `place_render`'s `strip_uuid` empty string means "strip nothing"; test-only.
- A periodic gateway scan between the test's scan and the verify would move `lastScanTimestamp` and hide the line; none was seen in the two runs.

## Requirement bookkeeping

No requirement is marked complete. DPLN-07, DPLN-08 and DPLN-09 are **advanced**: the checks, the probe semantics and the verdict (`ok:false`, slug, exit-6 passthrough) are delivered and proven, but the user-facing verb `ign deploy verify`, the `main.rs` exit-6 hook, the README exception and the surfaces land in 22-11.

## Known Stubs

None.

## Threat Flags

None. The register's mitigations are in place: T-22-44 (`no_message_or_value_echo` and the log-hit and 403 tests), T-22-45 (`ForbidWrites`, a `TreeReader` only, wiremock sees only GETs in `verify_makes_no_writes`), T-22-46 (unavailable sources fail, absent probes warn, an unreachable gateway is an error), T-22-48 (probe polling bounded by `probe_deadline`).

## Self-Check: PASSED

- Files present: `crates/ignition-core/src/deploy/verify.rs`, `tests/deploy_verify.rs`, `tests/common/forbid_writes.rs`, and `verify_green_then_red_live` in `tests/live_deploy_plan.rs`.
- Commits `378ff88`, `62d278e`, `0702fea`, `7dbecf1`, `d6ef90f` are in `git log`.
- `.planning/config.json` and `.planning/research/.cache/` untouched and uncommitted.
