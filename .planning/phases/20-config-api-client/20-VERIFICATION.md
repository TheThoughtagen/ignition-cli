---
phase: 20-config-api-client
verified: 2026-10-12T00:00:00Z
status: human_needed
score: 5/5 roadmap success criteria verified (SC1 carries live-coverage warnings); 7/7 requirements satisfied at library level
behavior_unverified: 0
overrides_applied: 0
gaps: []
human_verification:
  - test: "Run one live call: resource_list with ListQuery filter `name[eq]=<created name>` against the ephemeral 8.3.6 rig (and a non-matching name)"
    expected: "One match for the real name, zero for another. reqwest percent-encodes `name[eq]` to `name%5Beq%5D`; D7 was captured with literal brackets (curl -g). Silent zero matches is exactly the D7 failure mode."
    why_human: "Needs a live gateway; verifier was told not to start one. wiremock decodes the pair so it cannot show how the real Jetty reads the encoded form."
  - test: "Live-run resource_singleton, resource_list_in_collection(..., 'production'), and ResourceUpdate::from_record / with_collection (PUT carrying collection, enabled, backupConfig) with two copies (core + production) present"
    expected: "Singleton reads; the collection list returns the production copy; the from_record PUT is accepted and rotates the signature of the copy it targets"
    why_human: "Needs a live gateway. Only name/signature/description/config PUT bodies were ever captured."
---

# Phase 20: Config API Client Verification Report

**Phase Goal:** The core library speaks the 8.3.6 config-resource, scan-lock, scan and deployment-mode APIs with success predicates taken from live captures, so every later deploy verb builds on wire truth rather than on HTTP status codes.
**Verified:** 2026-10-12
**Status:** human_needed (no blockers; two live-coverage follow-ups)
**Re-verification:** No, initial verification

## Gates run by the verifier

| Gate | Command | Result |
|---|---|---|
| Tests | `cargo test --workspace` | exit 0, 72 result lines "ok", 0 failures (includes `deploy_client_contract`, `common/fake_deploy`, local_url_guard tests; the `#[ignore]` live gate is skipped as designed) |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| Live gate evidence | `20-06-live-gate.log` read (no gateway started) | `test result: ok. 1 passed`, PASS guard/A/B/C/D/E/Z, live trace == fake trace (13 steps) |

## Observable Truths (ROADMAP success criteria)

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | Library lists/reads resources by type+name from effective view and a named collection, creates by POST, updates by signature-checked array PUT; stale/missing signature is its own typed error | VERIFIED (with warnings W1-W3) | `DeployApi` in `client/deploy_api.rs` (names/list/find/singleton/create/update). Live gate section C: create, find effective/core/production, update, stale reuse -> `ResourceSignatureRejected{Stale}`, refused write leaves signature unchanged, list, names, duplicate refused. Typed error `resource_signature_rejected` (exit 6) in `error.rs`; D3 (stale 500 vs omitted/empty 422) captured. |
| 2 | Scan-lock acquire, remaining time, expiry detected from 200->204 with no clock compare (skew test); other actor's lock = `scan_lock_held` naming holder | VERIFIED | Live gate A (held by cap-a, cap-b refused naming cap-a, Free after expiry). `scan.rs` has a skewed-clock test and a "reads no clock" test. Route-scoped classify arm for 409. |
| 3 | Scan complete only when `scanActive` false AND `lastScanTimestamp` advanced on same gateway; 200 trigger with non-advancing status = not complete | VERIFIED | `scan_complete` truth-table test; contract test `scan_trigger_200_but_status_never_advances_is_not_complete` (NotComplete, not Err); live gate B (Complete, timestamp advanced). |
| 4 | Lists/creates modes, reports `deploymentMode` with `null` and `""` = no mode; redundancy names active node by activityLevel with Master/Backup vocabulary; `redundancy.rs` no longer says Primary | VERIFIED | Live gate D (None, create, list, still None). `active_node` keys on `activityLevel == "Active"`, tests use verbatim C-S3 pair incl. post-failover (role Master, activity Cold -> OnlyB). `grep -w Primary` finds nothing in redundancy/status/version (only an unrelated webdev Python comment). |
| 5 | Every endpoint has a declared shape recorded in `20-LIVE-CAPTURES.md` before dependent code; pinned by wiremock contract tests; echoed bodies size-capped | VERIFIED (one documented limit, W4) | Endpoint shapes table (13 ids) with capture refs; capture commits (69eca52, e415f2c) precede `dec3935`/`c15ec86` code commits. `cap_422_body_is_truncated` and `GATEWAY_CLIENT_BODY_CAP_BYTES` pin the cap; holder capped at 256. |

Constraints honored: no clap surface; new `DeployApi` trait (not `GatewayApi` methods); workspace `serde_json` features are only `raw_value` (no `preserve_order`/`arbitrary_precision`); slugs `scan_lock_held` and `resource_signature_rejected` are in `error.rs` (slug, exit 6, hint, endpoint) and the README exit-table row.

## WR-04 ruling: is SC1's "from a named collection" met?

**Met, not a gap. But the live gate over-claims, so two follow-ups are recorded.**

- Reading from a named collection is live-proven: gate section C runs `resource_find(..., Some("core"))` and `Some("production")` against the gateway, and the C-list capture records `find?collection=` for both copies with different signatures.
- Listing a named collection is, by D7, not a gateway capability: `list` is effective-view only. `resource_list_in_collection` composes `resource_names` (live) with `resource_find(collection)` (live). Both parts are live-proven; only the composition is untested live. It is pure logic covered by wiremock and `FakeDeploy`.
- Effective-view listing is live (unfiltered `resource_list`, `resource_names`).
- The genuinely unproven wire fact is the filtered list: client sends `name[eq]=v` via reqwest, which percent-encodes the brackets. D7's evidence used literal brackets. The SC does not require filtering, and Phase 22 plan/verify can use `names` + `find`, so this is a WARNING, not a BLOCKER. It is nonetheless a recorded human-verification item, because a silent zero-match is the failure D7 already documented.
- The claim "every `DeployApi` method" in `20-06-SUMMARY.md` and the live test header is false: `resource_singleton`, `resource_list_in_collection`, filtered list, `ScanKind::Projects` and `with_collection`/`from_record` updates were not run live. Reword the SUMMARY.

## Requirements Coverage

| Requirement | Source plans | Status | Evidence |
|---|---|---|---|
| DAPI-01 list/read by type+name, effective or named collection | 20-04, 20-06 | SATISFIED | names/list/find/singleton; live find per collection (see WR-04 ruling) |
| DAPI-02 POST create, signature-checked array PUT, typed signature error | 20-02, 20-04, 20-06 | SATISFIED | live create/update/stale; `resource_signature_rejected` |
| DAPI-03 scan-lock acquire, remaining, expiry w/o local clock | 20-03, 20-06 | SATISFIED | live A; skew test |
| DAPI-04 scan trigger, completion by scanActive + timestamp advance | 20-03, 20-06 | SATISFIED | contract + live B |
| DAPI-05 modes list/create, `null`/`""` -> no mode | 20-05, 20-06 | SATISFIED | live D; D5 captured `""` (null not observed on 8.3.6, normalised anyway) |
| DAPI-06 declared shape per endpoint proven by live capture; bodies capped | 20-01, 20-03, 20-04, 20-05, 20-06 | SATISFIED with limit | Ticked early by 20-01, but code and captures now exist for all 13 ids. Limit: `ScanKind::Projects` routes are wiremock-pinned only, recorded in "Drift and limits" (W4). |
| DAPI-07 held lock = `scan_lock_held` naming holder; active node by activityLevel, Master/Backup | 20-01, 20-02, 20-03, 20-06 | SATISFIED at library level | Full requirement text names no CLI surface. `active_node` + `scan_lock_held` exist and are tested. Deferral of `ign redundancy status` to Phase 24 is a separate user decision, not part of the requirement text. Bookkeeping: REQUIREMENTS.md leaves the box unticked and the row "Pending"; the orchestrator should tick it (or move the CLI-surface note to a Phase 24 requirement). |

No orphaned IDs: DAPI-01..07 are all claimed by plan frontmatter (20-06 claims all seven).

## Warnings (non-blocking; from the review and verifier checks)

| ID | Item | Severity |
|---|---|---|
| W1 | WR-04: live gate does not exercise singleton, `resource_list_in_collection`, filtered list (percent-encoded brackets), `with_collection` update. Human item 1 and 2 above. | Warning |
| W2 | WR-03: `ResourceUpdate::from_record` sends `collection`/`enabled`/`backupConfig`, never captured; doc comment wrongly says the gate settles it. Phase 23 will use this path. | Warning |
| W3 | WR-01: duplicate resource create (409) and duplicate/illegal mode (409/400) surface as `internal` (exit 1, "report a bug"), contradicting D6's "needs a typed outcome". Pinned by `create-duplicate:internal`. Not a DAPI requirement, but Phase 23 callers cannot distinguish "exists" from a CLI bug. | Warning |
| W4 | `projects` scan-lock/scan kinds not live-captured (documented limit). | Warning |
| W5 | WR-02: 408 on the scan-lock route is mapped to `scan_lock_held` although 408 was never observed (D1). Can misreport a proxy timeout as a held lock. | Warning |
| W6 | WR-05: live scenario uses `holdTimeout` 4 on a rig that stalled 1.92 s (C1g); flaky-test risk for re-runs. | Warning |
| W7 | WR-06, IN-01..07: endpoint table method/path columns are untested documentation; holder string not control-char filtered; `ResourceUpdate` derives Debug (leaks signature); hint omits D8's "no release route" wording; local absolute path in `redundancy.rs` doc. | Info |

## Anti-Patterns

Scanned phase source and test files for `TBD|FIXME|XXX`: none found. No stub implementations; every `DeployApi` method issues a real request through `send_shaped`/`read_resource_json` with shape checks. Level 4 data-flow: library-only phase, no rendered output; values originate from parsed gateway bodies and the live gate confirms real round-trip.

## Gaps Summary

No BLOCKER. The phase goal is achieved: the client's success predicates (409 holder, 200/204 lock, scanActive + advanced timestamp, activityLevel, array-PUT signature refusal) are taken from captures and were confirmed by one green live run whose trace matches the `FakeDeploy`. Status is `human_needed` solely because two recorded live-coverage items (percent-encoded filtered list; collection-bearing PUT and singleton) cannot be closed without starting a gateway. Recommended: either run them in a short extension of `live_deploy_api.rs` before Phase 22 depends on filtered list, or explicitly accept W1/W2 as deferred and correct the "every method" wording in `20-06-SUMMARY.md`.

---

_Verified: 2026-10-12_
_Verifier: Claude (gsd-verifier)_
