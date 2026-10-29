---
phase: 20-config-api-client
reviewed: 2026-10-12T00:00:00Z
depth: standard
files_reviewed: 27
files_reviewed_list:
  - README.md
  - crates/ignition-cli/src/render.rs
  - crates/ignition-cli/tests/e2e_api_diagnostics.rs
  - crates/ignition-core/src/client/classify.rs
  - crates/ignition-core/src/client/config_resources.rs
  - crates/ignition-core/src/client/deploy_api.rs
  - crates/ignition-core/src/client/mod.rs
  - crates/ignition-core/src/client/mode.rs
  - crates/ignition-core/src/client/redundancy.rs
  - crates/ignition-core/src/client/scan.rs
  - crates/ignition-core/src/client/status.rs
  - crates/ignition-core/src/client/version.rs
  - crates/ignition-core/src/error.rs
  - crates/ignition-core/tests/common/deploy_scenario.rs
  - crates/ignition-core/tests/common/fake_deploy.rs
  - crates/ignition-core/tests/common/mod.rs
  - crates/ignition-core/tests/deploy_client_contract.rs
  - crates/ignition-core/tests/fixtures/live-rig-deploy/capture.sh
  - crates/ignition-core/tests/fixtures/live-rig-deploy/compose.bind.yml
  - crates/ignition-core/tests/fixtures/live-rig-deploy/compose.yml
  - crates/ignition-core/tests/fixtures/live-rig-deploy/db-connection-config.json
  - crates/ignition-core/tests/fixtures/live-rig-deploy/mint-token.sh
  - crates/ignition-core/tests/fixtures/live-rig-deploy/security-levels/config.json
  - crates/ignition-core/tests/fixtures/live-rig-deploy/security-levels/resource.json
  - crates/ignition-core/tests/fixtures/live-rig-deploy/security-properties/config.json
  - crates/ignition-core/tests/fixtures/live-rig-deploy/security-properties/resource.json
  - crates/ignition-core/tests/live_deploy_api.rs
findings:
  critical: 0
  warning: 6
  info: 7
  total: 13
status: issues_found
---

# Phase 20: Code Review Report

**Reviewed:** 2026-10-12
**Depth:** standard
**Files Reviewed:** 27
**Status:** issues_found

## Summary

Phase 20 adds the `DeployApi` trait (resource, scan-lock, scan, mode calls), the lenient wire models, a route-scoped `scan_lock_held` classify arm, a deploy-scoped signature-refusal helper, a stateful `FakeDeploy`, wiremock contract tests, and a live gate against a disposable 8.3.6 rig. The core client is careful: request paths are validated before any request, signatures and problem text are withheld from every error, neither POST nor any write is retried, completion of a scan is judged by a polled status and never by the trigger's 200, and the live gate has a loopback URL guard plus a gateway-name guard before any write.

I found no data-loss or security BLOCKER. The defects are in classification fidelity (two 409 outcomes land in the `internal` bug bucket, an unobserved 408 is mapped to a typed state), claims that the live gate does not back (it says it settles questions it never asks, and says "every method" while skipping two), and a timing-sensitive live scenario on a rig the captures show can stall for about two seconds. The rest is maintainability.

No structural findings (fallow) were supplied for this review.

## Narrative Findings (AI reviewer)

## Warnings

### WR-01: A duplicate create and a duplicate or illegal mode surface as `internal` (exit 1, "report as a bug")

**File:** `crates/ignition-core/src/client/deploy_api.rs:615-637` (`resource_create`), `:754-774` (`mode_create`); `crates/ignition-core/src/client/classify.rs:189-229` (fallthrough arm)
**Issue:** Capture D10 records the duplicate create as a clean, typed refusal (409 with a JSON `problem`), D6 records a duplicate mode as 409 and an illegal mode name as 400, and D6 says explicitly "409 and 400 need a typed outcome in 20-05, not a body echo". Nothing in 20-05 delivers that. Both 409s and the 400 fall into classify's catch-all and become `CoreError::Internal("unexpected HTTP 409 ...")`, exit 1. The project's own contract says exit 1 means "report as a bug". A scripted or agentic caller (Phase 23's apply engine) cannot tell "the resource already exists" from a genuine CLI fault without string-matching the message. The trace tag `create-duplicate:internal` is pinned in `fake_deploy.rs`, `deploy_scenario.rs`, `deploy_client_contract.rs` and the live log, so this is enshrined as intended behavior, but it contradicts the decision it cites. An illegal mode name that passes `validate_segment` but is rejected by the gateway (400) is also an input error reported as a bug.
**Fix:** Add typed outcomes now, while the surface is new and nothing consumes it: for example `CoreError::ResourceExists { name, endpoint }` and `ModeExists { name, endpoint }` (exit 6, target state) for the two 409s, and map the mode 400 to `InvalidInput`. Scope each in `send_resource_write` / `mode_create` (the deploy-scoped helper already owns the 500/422 decisions) rather than in classify, so the EAM routes keep their behavior. Update the pinned `create-duplicate:*` tag and README exit table.

### WR-02: An unobserved 408 is mapped to `scan_lock_held` on every method of the scan-lock route

**File:** `crates/ignition-core/src/client/classify.rs:125-132`
**Issue:** The arm `S::CONFLICT | S::REQUEST_TIMEOUT if is_scan_lock_url(url)` matches any verb. D1 records that 408 "never appeared"; only 409 is capture-proven. A 408 is also what a reverse proxy, load balancer or Jetty idle-timeout produces, and it can arrive on the `GET` lock-info call. Such a response would be reported as `scan_lock_held` with a "wait for the holder, then retry" hint when no lock is held. In `scan_lock_acquire` the enrichment read can then return `Free` (204) and the code still returns `ScanLockHeld { actor: None }` (pinned by `lock_held_survives_a_failed_enrichment`), so the caller is told the lock is held after the gateway has just said it is free.
**Fix:** Map only 409 here. Keep 408 on the acquire POST only if the OpenAPI's "Unable to acquire lock within timeout" is worth honoring, and in that case decide from the enrichment read: `Free` means the original 408 is a transport-class timeout, not a held lock.

### WR-03: `ResourceUpdate::from_record` sends fields no live capture has shown the gateway accepting, and the gate that was meant to settle it does not

**File:** `crates/ignition-core/src/client/config_resources.rs:370-390`; `crates/ignition-core/tests/live_deploy_api.rs:315-340`
**Issue:** The doc comment says "Whether the gateway wants `collection` on a PUT is not shown by the captures; the 20-06 live gate settles it." It does not. The live gate and the shared scenario build every update with `ResourceUpdate::new` (name, signature, config) and never call `from_record`, `with_collection`, `with_enabled` or `with_backup_config`. The only PUT bodies ever captured carry `name`, `signature`, `description` and `config`. `from_record` always adds `collection` and also `enabled` and `backupConfig`. If the PUT endpoint rejects an unknown or unexpected key (the gateway answers a 422 on shape problems elsewhere), the recommended "read, then update" path fails on first live use. The 20-06 summary and `20-LIVE-CAPTURES.md` also record the collection-less PUT question as unresolved ("which copy it updates is still not shown"), while the live run happened to hit a case with a single core copy at that point in the scenario.
**Fix:** Add a live section that updates from `resource_find(...)` via `from_record` for both a core copy and a production copy with two copies present, and asserts which copy's signature changed. Until that is captured, correct the doc comment so it no longer claims the gate settles the question.

### WR-04: The live gate's claims outrun what it exercises

**File:** `crates/ignition-core/tests/live_deploy_api.rs:1-8, 267-380`; `.planning/phases/20-config-api-client/20-06-SUMMARY.md:13,48`
**Issue:** The summary and frontmatter say the gate runs "every `DeployApi` method" live. It never calls `resource_singleton`, `resource_list_in_collection` (the D7 workaround), `ScanKind::Projects`, a `resource_list` with a filter, or `ResourceUpdate::with_collection`. The filter path matters most: D7 was learned from curl with literal brackets (`-g`), while the client sends `name[eq]` through `reqwest::RequestBuilder::query`, which percent-encodes the brackets. The wiremock test decodes the pair, so it cannot show how the real gateway reads `name%5Beq%5D=`. A filter that silently matches nothing is exactly the failure D7 recorded ("matches nothing"), and the client's own guard exists to prevent it.
**Fix:** Add live assertions for a `name[eq]` filtered list (expect one match for the created name and zero for a different name), `resource_singleton("ignition", "security-properties", Some("core"))`, and `resource_list_in_collection(..., "production")`. Reword the summary to list what was not exercised.

### WR-05: Live scenario timing is tight for a gateway the captures show can stall for about two seconds

**File:** `crates/ignition-core/tests/live_deploy_api.rs:171-201`; `crates/ignition-core/tests/common/deploy_scenario.rs:57-61, 97-128`
**Issue:** The gate acquires a lock with `holdTimeout` 4, so `remaining` reads 3 immediately, then asserts the lock is still `Held` with `remaining` in `1..=4` on the next read and that a second token's acquire is refused. C1g records an acquire taking 1.92 s on this emulated (amd64 on arm64) rig. A stall of that size between the acquire and the next call lets the hold lapse: the info read then returns `Free` (panic "reads Free right after the acquire"), or in the scenario `b` acquires successfully and the trace records `lock:b-acquired-unexpected`, leaving `b` holding a lock into the later steps. This is a flaky-test pattern, and the scenario's divergence assertion would report it as a fake-versus-gateway drift.
**Fix:** Use a longer hold for the held-then-refused steps (for example 15 to 20 s) and wait for expiry with the existing polling loop, which already has a 15 s budget; or set `lock_hold_s` to at least 10 in `ScenarioTiming::live()`. Widen the `remaining` range accordingly.

### WR-06: The deploy endpoint table's method and path columns are not tied to what the client sends

**File:** `crates/ignition-core/src/client/deploy_api.rs:79-173, 974-1151`
**Issue:** `DEPLOY_ENDPOINTS` carries `method` and `path_template` for every endpoint, and the module doc says every method "maps to exactly one `EndpointDecl`". Only `id` and `shape` are used at runtime. Each method builds its own path and verb from `resource_*_path`, `scan_*_path` and `MODE_PATH`, so the table's `method` and `path_template` columns are documentation that no test ties to the wire. The unit tests `endpoint_rows_match_the_capture_table` and `deploy_endpoints_cover_every_method` re-state the same literals the table holds (tautological), and `every_method_id_is_declared` only checks that the ids exist, not that each method uses its own. A future edit can change a path or verb in a method, leave the table untouched, and every unit test stays green (the wiremock tests would catch the wire change but not the table drift).
**Fix:** Either drop `method` and `path_template` from `EndpointDecl` (keep id, shape, capture_ref), or have each method derive its request from the declaration, or add a contract test that sends each method against wiremock and asserts the received verb and path equal the row.

## Info

### IN-01: Gateway-supplied lock holder is echoed without stripping control characters

**File:** `crates/ignition-core/src/client/classify.rs:289-299`; `crates/ignition-core/src/client/deploy_api.rs:685-688`
**Issue:** The holder string (scraped from an HTML body, or taken from the JSON `actor`) is capped at 256 characters but is otherwise verbatim and is shown in `Display` and the JSON envelope. The scrape stops at whitespace, quotes, `<` and `,`, but an ESC (0x1b) or other control byte passes through, so a hostile or buggy gateway can write terminal control sequences into CLI output.
**Fix:** Filter to printable characters when building the holder (`.chars().filter(|c| !c.is_control())`) in both places, and share one cap constant (see IN-02).

### IN-02: The 256-character holder cap is defined twice

**File:** `crates/ignition-core/src/client/classify.rs:265` (`LOCK_HOLDER_CAP`) and `crates/ignition-core/src/client/deploy_api.rs:48` (`ACTOR_CAP`)
**Issue:** Two constants with the same value and purpose in two files; they can drift.
**Fix:** Keep one `pub(crate)` constant (for example in `scan.rs`) and use it from both.

### IN-03: `ResourceUpdate` derives `Debug`, which prints the signature the rest of the design goes to lengths to withhold

**File:** `crates/ignition-core/src/client/config_resources.rs:313-332`
**Issue:** The signature field is private so it cannot be built empty, but `#[derive(Debug)]` renders it. Any `tracing::debug!(?items)` or `{:?}` in a future caller (Phase 23) leaks a live signature. The tests only check error text, not Debug output.
**Fix:** Implement `Debug` by hand and print `signature: "<redacted>"`.

### IN-04: The `scan_lock_held` hint drops the wording Decision D8 asked for

**File:** `crates/ignition-core/src/error.rs:1273-1277`
**Issue:** D8 says the hint "must keep saying 'no release route: wait for the holder (holdTimeout) or let a scan release it'". The shipped hint says the lock "is released when the holder's scan runs or its holdTimeout expires". A caller reading it does not learn that there is no release call to try; the test asserts only `holdTimeout` and `scan runs`.
**Fix:** Lead the hint with "there is no release route" and add that to the assertion.

### IN-05: Mode-name validation reuses the resource-segment validator, so messages read "resource mode name ..."

**File:** `crates/ignition-core/src/client/deploy_api.rs:755`; `crates/ignition-core/src/client/config_resources.rs:51-68`
**Issue:** `validate_segment("mode name", ...)` formats `resource {label} ...`, giving "resource mode name must not be empty". Also, the allowed character set for a mode name is the path-segment set, not anything the gateway documents (D6 only shows `a b/` rejected). A legal gateway mode name outside `[A-Za-z0-9._-]` is refused client-side, with no capture backing the restriction.
**Fix:** Take the full label from the caller (`"resource module"`, `"mode name"`), and note in the doc that the restriction is a client-side choice.

### IN-06: Source documentation carries a developer's absolute local path

**File:** `crates/ignition-core/src/client/redundancy.rs:15-18`
**Issue:** The module doc cites `/Users/pmannion/data/worktrees/whk-environment-orchestration/gateway-config-spec/spikes/gateway-modes/results/S3.md`. That path does not exist for anyone else, and it is in a shipped source file.
**Fix:** Cite the capture block (`20-LIVE-CAPTURES.md` C-S3, which already carries the verbatim bodies) and drop the local path.

### IN-07: `mint-token.sh` interpolates caller-supplied token names into a path and a sourced shell file without validation

**File:** `crates/ignition-core/tests/fixtures/live-rig-deploy/mint-token.sh:45-57`
**Issue:** `NAMES` come from the command line and are used in `$tmp/ignition/api-token/$name` and, after `tr`, as part of an `export IGN_TOKEN_<VAR>=` line in `tokens.env`, which the capture driver and the live-gate recipe `source`. A name with `/`, `..`, `;` or spaces escapes the directory or injects shell. The input is operator-supplied and the rig is disposable, so impact is limited to a self-inflicted foot-gun.
**Fix:** Reject any name that is not `^[a-z0-9-]+$` before the loop.

---

_Reviewed: 2026-10-12_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
