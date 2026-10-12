---
phase: 20-config-api-client
verified: 2026-10-12T20:00:00Z
status: passed
score: 5/5 roadmap success criteria verified; 7/7 requirements satisfied (DAPI-07 at library level by user decision)
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: human_needed
  previous_score: 5/5 (SC1 carried live-coverage warnings)
  gaps_closed:
    - "Filtered resource_list with percent-encoded brackets: proven live (gate re-run d24e90b, section C)"
    - "resource_singleton, resource_list_in_collection, ResourceUpdate::from_record per-copy update, projects scan kind: proven live (section C)"
    - "UAT items 1 and 2: both passed (20-UAT.md, ddce322)"
  gaps_remaining: []
  regressions: []
gaps: []
deferred:
  - truth: "`ign redundancy status` CLI wired to the library `active_node` (activity level, Master/Backup vocabulary)"
    addressed_in: "Phase 24"
    evidence: "ROADMAP Phase 24 Constraints: 'Wire `ign redundancy status` to the Phase 20 library `active_node` ... carried from DAPI-07, which closed at library level in Phase 20.' REQUIREMENTS.md DAPI-07 row: 'Complete - library level'."
human_verification: []
---

# Phase 20: Config API Client Verification Report

**Phase Goal:** The core library speaks the 8.3.6 config-resource, scan-lock, scan and deployment-mode APIs with success predicates taken from live captures, so every later deploy verb builds on wire truth rather than on HTTP status codes.
**Verified:** 2026-10-12
**Status:** passed
**Re-verification:** Yes. The earlier report was stale: review fixes WR-01..WR-06, a live gate re-run, the security audit and the UAT all landed after it.

## Gates run in this verification (main checkout, HEAD 1928fc2)

| Gate | Command | Result |
| --- | --- | --- |
| Workspace tests | `cargo test --workspace` | Every `test result` line is `ok`; no failures anywhere (incl. 526-test and 274-test suites). Live tests are `#[ignore]`d by design (live gate used from the committed log, no gateway started). |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | Clean, no diagnostics. |
| Deploy contract suite | `cargo test -p ignition-core --test deploy_client_contract` | 64 passed, 0 failed. |
| Error-contract tests | `exit_code_mapping_enumerated`, `readme_exit_table_agreement`, `deploy_slugs_never_carry_signature_or_problem_text` | Pass (part of the workspace run). |

## Observable Truths (ROADMAP Success Criteria)

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | Library lists/reads config resources by type and name (effective view and named collection), creates by POST, updates with signature-checked array PUT; stale/missing signature is its own typed error | VERIFIED | `DEPLOY_ENDPOINTS` rows `resource_names/list/find/singleton/create/update` in `client/deploy_api.rs` (1195 lines) and `client/config_resources.rs` (488). Contract tests: `resource_find_effective_view_sends_no_collection`, `resource_find_named_collection_sends_collection_param`, `resource_list_in_collection_composes_names_and_find`, `resource_create_posts_an_array_body`, `resource_update_puts_an_array_with_signature`, `signature_stale_is_resource_signature_rejected`, `signature_missing_on_wire_maps_per_d3`, `signature_empty_is_refused_before_any_request`, `resource_duplicate_create_is_never_ok` (now `resource_exists`). Live gate section C (20-06-live-gate.log, d24e90b): create, effective/core/production reads, update, stale refusal leaves signature unchanged, per-copy `from_record` update, filtered list with percent-encoded brackets (one match / zero match), singleton reads. |
| 2 | Scan-lock acquire, remaining time while held, expiry detected from the response change alone (200 JSON held, 204 after); skewed local clock still decides correctly; foreign holder surfaces as `scan_lock_held` naming the holder | VERIFIED | `lock_verdict` (`client/scan.rs:128`) takes no clock and ignores `timestamp`; proptest `lock_verdict_ignores_timestamp` (512 cases, arbitrary i64 timestamp) is the skewed-clock test; `scan_rs_reads_no_clock` structurally pins no clock use. Contract tests `lock_held_names_the_holder_from_the_captured_body`, `lock_held_without_actor_is_enriched_from_lock_info`, `lock_held_survives_a_failed_enrichment`, `lock_info_held_then_free`, `shape_lock_info_200_empty_body_is_an_error_not_free`; 408 no longer maps to `scan_lock_held` (`lock_408_is_not_scan_lock_held`). Live gate A: held by cap-a, cap-b refused naming cap-a, Free after expiry (12 s hold). |
| 3 | Scan complete only when `scanActive` false and `lastScanTimestamp` advanced on the same gateway; trigger 200 with a non-advancing status reports not complete | VERIFIED | `scan_complete` = `!now.scan_active && now.last_scan_timestamp > before.last_scan_timestamp` (`scan.rs:171`); `ScanStatus` deciding fields are required (empty `{}` is a parse error). Tests `scan_trigger_200_but_status_never_advances_is_not_complete` (NotComplete), `scan_complete_requires_inactive_and_advanced`, `scan_foreign_scan_active_at_before_is_not_counted`. Live gate B: Complete with advanced timestamp. |
| 4 | Lists/creates deployment modes; `deploymentMode` reads `null` and `""` as "no mode"; redundancy names the active node by activity level with Master/Backup vocabulary; doc no longer says `Primary` | VERIFIED (library level, per user decision) | `normalize_deployment_mode` filters `None` and blank to `None` (`client/mode.rs:93`); tests `mode_deployment_mode_no_mode_is_none`, `mode_deployment_mode_null_absent_and_blank_are_none`, `mode_list_*`, `mode_create_*`. `active_node` reads only `activity_level == "Active"` (`client/redundancy.rs:165`), tested against the verbatim C-S3 pair pre- and post-failover (`active_node_follows_activity_not_role`). Zero `Primary` occurrences in `crates/**/*.rs` redundancy code. Live gate D: no active mode, production mode created and listed, still no mode; E trace includes `mode-duplicate:mode_exists`. CLI wiring of `ign redundancy status` to `active_node` is carried to Phase 24 (recorded in ROADMAP and REQUIREMENTS). |
| 5 | Every new endpoint has a declared response shape recorded in `20-LIVE-CAPTURES.md` before dependent code and pinned by a wiremock contract test; echoed gateway bodies are size-capped | VERIFIED | `DEPLOY_ENDPOINTS` carries `id`, `method`, `path_template`, `shape`, `capture_ref` per row; Endpoint shapes table in `20-LIVE-CAPTURES.md` matches. Capture commits (`69eca52`, `7da5730`, 20-01) precede the first client commit (`dec3935`, 20-03). `wire_matches_the_endpoint_table` ties verb and path to every row and requires each row exercised once. Size caps: `cap_422_body_is_truncated`, `cap_422_body_is_truncated_on_a_read`, `ACTOR_CAP`/`LOCK_HOLDER_CAP=256`, `deploy_slugs_never_carry_signature_or_problem_text`. |

**Score:** 5/5 truths verified (0 present-but-behavior-unverified).

## Behavior-dependent truths

The cancellation/expiry and completion invariants (SC2, SC3) are exercised by named passing tests (skewed-clock proptest, never-advancing scan contract test) and by the live gate, so none rest on symbol presence alone.

## Required Artifacts

| Artifact | Status | Details |
| --- | --- | --- |
| `crates/ignition-core/src/client/deploy_api.rs` | VERIFIED | `DeployApi` trait, `DEPLOY_ENDPOINTS`, `send_shaped`; re-exported from `client/mod.rs`; separate from `GatewayApi` per roadmap lock. |
| `crates/ignition-core/src/client/config_resources.rs` | VERIFIED | Resource types, path builders, `ResourceUpdate::from_record`, segment validation. |
| `crates/ignition-core/src/client/scan.rs` | VERIFIED | `LockObservation`, `lock_verdict`, `ScanStatus`, `scan_complete`, `ScanOutcome`. |
| `crates/ignition-core/src/client/mode.rs` | VERIFIED | `normalize_deployment_mode`, `ModeRecord`, `ModeCreate`. |
| `crates/ignition-core/src/client/redundancy.rs` | VERIFIED | `active_node`, `ActiveNode`, Master/Backup vocabulary in docs. |
| `crates/ignition-core/src/client/classify.rs` | VERIFIED | 409-only scan-lock arm with capped holder and enrichment read. |
| `crates/ignition-core/src/error.rs` | VERIFIED | Typed `scan_lock_held`, `resource_signature_rejected`, `resource_exists`, `mode_exists`; four-place rule enforced by tests. |
| `tests/deploy_client_contract.rs`, `tests/common/{fake_deploy,deploy_scenario}.rs`, `tests/live_deploy_api.rs` | VERIFIED | 64 contract tests pass; stateful fake plus live gate share one scenario; 14-step live trace equals fake trace. |
| `20-LIVE-CAPTURES.md`, `20-06-live-gate.log` | VERIFIED | Captures C1a-C1j, C3a, C-null, C-mode, C-list, C-S3 plus a dated gate-rerun entry; log shows PASS guard/A/B/C/D/E/Z. |

## Key Links

| From | To | Status |
| --- | --- | --- |
| `DeployApi` methods | `DEPLOY_ENDPOINTS` rows (shape enforcement in `send_shaped`) | WIRED (`wire_matches_the_endpoint_table`) |
| Scan POST 200 | Completion decision | WIRED correctly: the trigger body is ignored; completion comes only from `scan_status` + `scan_complete` |
| `classify` 409 on scan-lock route | `CoreError::ScanLockHeld` with holder | WIRED |
| PUT 500 "signature mismatch" / 422 signature field | `CoreError::ResourceSignatureRejected` | WIRED, no signature or problem text in payload |
| `DeployApi` | any verb/CLI | Intentionally none (library-only phase) |

## Anti-Patterns

No `TBD`/`FIXME`/`XXX` in `crates/ignition-core/src` or `tests`; no `TODO`/`HACK`/`todo!`/`unimplemented!` in the phase's source and test files. No stubs found.

## Requirements Coverage

| Requirement | Status | Evidence |
| --- | --- | --- |
| DAPI-01 | SATISFIED | SC1 |
| DAPI-02 | SATISFIED | SC1 (POST create, signature-checked PUT, typed refusals) |
| DAPI-03 | SATISFIED | SC2 |
| DAPI-04 | SATISFIED | SC3 |
| DAPI-05 | SATISFIED | SC4 |
| DAPI-06 | SATISFIED | SC5 |
| DAPI-07 | SATISFIED at library level | `scan_lock_held` naming holder and `active_node` by activity level verified; `ign redundancy status` wiring is deferred to Phase 24 by user decision (REQUIREMENTS.md row 125) |

No orphaned requirements: all of DAPI-01..07 appear in plan frontmatter and are mapped to Phase 20.

## Other evidence reviewed

- 20-REVIEW-FIX.md: WR-01..WR-06 all fixed; the items it listed as needing live confirmation (percent-encoded filter, singleton, per-collection list, `from_record`, projects scan kind, 12 s hold) are now proven by the d24e90b gate re-run.
- 20-SECURITY.md: 32/32 threats closed, `threats_open: 0`.
- 20-UAT.md: 2/2 passed, 0 issues.

## Human Verification Required

None. Both previously pending human items passed (20-UAT.md).

## Findings (non-blocking, no gaps)

- INFO: Review items IN-01..IN-07 were out of scope for the fix pass and remain open (control characters in the echoed lock holder, duplicated 256 cap, `Debug` on `ResourceUpdate`, `scan_lock_held` hint wording, mode-name error wording, an absolute local path in `redundancy.rs` doc comments, unvalidated token name in `mint-token.sh`). None affects a success criterion; worth sweeping in a later phase.
- INFO: `20-VALIDATION.md` still has `status: draft`, `nyquist_compliant: false`; bookkeeping only.
- INFO: ROADMAP.md line 86 still shows Phase 20 as `[ ]` although plans are 6/6; the orchestrator should tick it on phase completion.
- INFO: The live gate is `#[ignore]`d and not run here (no gateway started, per instruction); its evidence is the committed log from d24e90b and the matching capture entry. The contract suite and fake gateway run in CI-style `cargo test` and pass.

## Gaps Summary

None. The goal is achieved: the library speaks the config-resource, scan-lock, scan and deployment-mode wire with predicates drawn from live captures, each pinned by a passing contract test, and the live gate re-run after the review fixes matches the fake trace step for step.

---

_Verified: 2026-10-12_
_Verifier: Claude (gsd-verifier)_
