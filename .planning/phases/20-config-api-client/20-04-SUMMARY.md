---
phase: 20-config-api-client
plan: 04
subsystem: api
tags: [config-resources, signature, deploy-api, wiremock, classify, ignition-8.3.6]

requires:
  - phase: 20-01
    provides: "20-LIVE-CAPTURES.md blocks C3a and C-list, decisions D3, D7, D10, assumptions A4 and A9, Endpoint shapes table"
  - phase: 20-02
    provides: "CoreError::ResourceSignatureRejected { name, reason: SignatureReason { Stale, Missing }, endpoint } (exit 6)"
  - phase: 20-03
    provides: "DeployApi trait, DEPLOY_ENDPOINTS, send_shaped, parse_json"
provides:
  - "client/config_resources.rs: validate_segment, resource_*_path builders, list_query_pairs, ResourceRecord, ResourceName, ResourceCreate, ResourceUpdate (private signature), MutationOutcome"
  - "DeployApi::{resource_names, resource_list, resource_find, resource_singleton, resource_create, resource_update} at the head of the trait, with six DEPLOY_ENDPOINTS rows"
  - "resource_list_in_collection (names, then find?collection=) because D7 says list sees the effective view only"
  - "Deploy-scoped write guard ReqwestGatewayApi::send_resource_write: the only place a signature refusal becomes ResourceSignatureRejected"
  - "classify.rs resource 422 arm extracted to resource_body_rejected, both echoes capped through truncate_api_body (DAPI-06)"
affects: [20-05, 20-06, 21, 22, 23, 24, 25]

tech-stack:
  added: []
  patterns:
    - "Typed refusal built in a deploy-scoped helper on the raw response, before classify, so v1.1 EAM pins are untouched"
    - "Unconstructible-without-a-signature write item: private field plus Result constructors"
    - "Errors built from the gateway's refusal carry the status, URL and a fixed phrase, never the body"

key-files:
  created:
    - crates/ignition-core/src/client/config_resources.rs
  modified:
    - crates/ignition-core/src/client/deploy_api.rs
    - crates/ignition-core/src/client/classify.rs
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/tests/deploy_client_contract.rs

key-decisions:
  - "D7 consequence: resource_list takes no collection; resource_list_in_collection composes names and find?collection=, skipping a name by its modes, by a not_found, or when the returned copy is stamped with another collection"
  - "D7 consequence: ListQuery.filter is sent object-style (name[eq]=v as its own parameter) and a filter not of that form is refused invalid_input, since filter=field[op]=v silently matches nothing on 8.3.6"
  - "D3: guard keys on 500 plus the lowercase marker 'signature mismatch' in problem.message (Stale) and on 422 with fieldMessages fieldName 'signature' (Missing); error.rs not touched"
  - "ResourceRecord.name is Option because a singleton has no name key (C-list singleton blocks)"

patterns-established:
  - "Mutation check on the guard: changing the marker or removing the cap turns six contract tests red; restored"

requirements-completed: [DAPI-01, DAPI-02]

duration: 55min
completed: 2026-10-12
status: complete
actuals:
  tokens: 21400
  tasks: 2
  commits: 2
---

# Phase 20 Plan 04: Config resource reads and signature-checked writes Summary

**`DeployApi` now reads config resources by type and name from the effective view or a named collection, creates by POST and updates by the signature-checked array PUT, and turns a stale or missing signature into exit-6 `resource_signature_rejected` without echoing the signature or the gateway's problem text.**

## Task Commits

1. **Task 1: read config resources by type and name** - `c15ec86` (feat)
2. **Task 2: signature-checked writes, typed refusal, capped 422 echo** - `cf244fe` (feat)

## What was built

- `config_resources.rs`: `validate_segment` (non-empty ASCII `[A-Za-z0-9._-]`, not `.`/`..`), five path builders (name segment through `encode_segment`, module and type never encoded), lenient `ResourceRecord` and `ResourceName` with a flattened `extra`, and the write items. `ResourceUpdate` keeps `signature` private and always serializes it; `new` and `from_record` refuse an empty signature as Missing, so an unsigned update cannot be built.
- `deploy_api.rs`: six methods declared at the head of the trait in the research order (names, list, find, singleton, create, update, then scan-lock/scan), six DEPLOY_ENDPOINTS rows with capture refs, and `send_resource_write`.
- The guard reads the raw response before `classify`. 500: parse `problem.message`, marker present gives Stale, otherwise `internal` naming only status and URL. 422: `fieldMessages[].fieldName == "signature"` gives Missing, otherwise the shared `classify::resource_body_rejected`. 2xx: parse the envelope, `success:false` is an error (Stale on the marker, `internal` otherwise). Nothing is retried.
- `classify.rs`: the resource 422 arm moved into `resource_body_rejected`; `truncate_api_body` wraps both the joined `messages` and the raw body. No signature handling was added there.

## Facts recorded for later plans

- **D3 statuses and marker the guard keys on:** stale is HTTP 500 with `problem.message` containing `signature mismatch`; omitted and empty signature are one byte-identical HTTP 422 whose `fieldMessages[0].fieldName` is `signature`, mapped to `SignatureReason::Missing`.
- **D3 consumption case:** D3 fits Stale and Missing exactly, so `error.rs` and `README.md` were not touched. The extend-only check passes (no line naming the slug or variant removed or rewritten).
- **D7 / `resource_list_in_collection`: needed.** `list` sees the effective view only and `filter=collection[eq]=c` matches nothing, so listing a named collection is `names` then `find?collection=` per name. The contract test pins that composition.
- **A9 holds, `ResourceCreate` has no `attributes` field.** `[{name, config}]` is the whole create body; the contract test asserts that exact body.
- **8.3.6 confirmation of amended DAPI-02:** C3a / D10 recorded the PUT of an unknown name with a valid current signature as a bare HTTP 404 with `Content-Length: 0`. `resource_update_unknown_name_is_not_found` pins it as `not_found` exit 6. Update is the signature-checked PUT and cannot create; create is the POST (200 with `changes[].newSignature`, duplicate 409 JSON problem).

## Deviations from Plan

**1. [Plan vs capture - D7] `resource_list` filter and collection handling**
- **Found during:** Task 1
- **Issue:** the plan's `resource_list` sends `q.to_query_pairs()`, which emits `filter=field[op]=v`. D7 and A4 show that form matches nothing on 8.3.6 (`matching:0` even for the effective item), so a caller using `ListQuery.filter` would silently get an empty list.
- **Fix:** `list_query_pairs` sends the filter as its own `field[op]=value` parameter and refuses any other form with `invalid_input`. `ListQuery` itself is unchanged. Pinned by `resource_list_filter_is_sent_object_style` (C-list object-style probes).
- **Files modified:** `config_resources.rs`, `deploy_api.rs`
- **Commit:** `c15ec86`

**2. [Rule 2 - safety] Names `.` and `..` refused**
- **Found during:** Task 1
- **Issue:** the plan validates module and type only. A name of `..` percent-encodes to `%2E%2E`, an ambiguous dot segment for a servlet container.
- **Fix:** `resource_find_path` refuses `.` and `..` names with `invalid_input`, alongside the empty-name refusal.
- **Commit:** `c15ec86`

**3. [Rule 2 - correctness] `resource_list_in_collection` guards against a fallback copy**
- **Found during:** Task 1
- **Issue:** the captures never show what `find?collection=X` answers for a name X does not hold. If it fell back to the effective view, a collection listing would return another collection's copy as X's.
- **Fix:** skip a name whose non-empty `modes` lacks the collection, and skip a record stamped with a different collection. Pinned in the composition test.
- **Commit:** `c15ec86`

**4. [Plan wording] `ResourceRecord.name` is `Option<String>`**
- C-list's singleton bodies have no `name` key. `from_record` returns `invalid_input` for a nameless record.

**5. [Plan vs fixtures] Contract-test bodies are trimmed, not byte-whole**
- Where a captured body is large (find bodies with `metrics` and `healthchecks`, the roughly 55-frame stacktraces of the 409 and 500), the fixture keeps every typed key verbatim and elides `metrics`/`healthchecks` to `{}` and the stacktrace to its first frames. Each fixture says so. The small bodies (create 200, PUT 200, names, 422) are verbatim.

**6. [Process - TDD] RED not committed separately**
- Tests were written with each implementation and committed together; no failing-test commit exists. Compensated by a mutation check: changing `SIGNATURE_MARKER` and removing the two `truncate_api_body` calls turned six contract tests red (`signature_stale_*`, `resource_update_2xx_success_false_*`, `resource_stale_after_successful_write`, both `cap_422_*`); restored afterwards.

**7. [Process - tracer] none.** This plan has no `type="tracer"` task. The `checkpoint:human-verify` rule does not apply; no human-verify checkpoint exists in the plan.

## Verification

- All five CI gates green locally: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`, `cargo test --workspace` (71 test binaries, 1518 tests passed, 0 failed), `cargo build -p ignition-cli --no-default-features`.
- Plan-scoped: `deploy_client_contract` 42 passed (11 `resource_*` reads plus writes, `signature_*`, `cap_*`); `eam_contract` 38, `resources_contract` 15, `api_classify_contract` 5 passed unedited, including `task_delete_signature_mismatch_500_is_the_recorded_finding` (still exit 1 `internal`).
- Acceptance greps: `truncate_api_body` in `classify.rs` 5 (needs 3 or more); `ResourceSignatureRejected` in `classify.rs` 0; in `deploy_api.rs` 3 (needs 2 or more); `C3a` in the contract test 11; `test ! -d crates/ignition-core/src/deploy` holds; no `preserve_order` or `arbitrary_precision`.
- The Task 2 BASE (`9c1d4b1`, parent of the Task 1 commit) resolves; the extend-only diff of `error.rs` and `README.md` is empty.

## Requirements

- **DAPI-01 and DAPI-02 marked complete:** their full text is satisfied by code that now exists (list/names/find/singleton from the effective view or a named collection; create by POST, update by the signature-checked array PUT, stale or missing signature as its own error).
- **DAPI-06** was already complete; this plan adds the 422 echo cap and the six `resource_*` endpoint rows.

## Known Stubs

None.

## Threat Flags

None. No endpoint, auth path or schema beyond the planned resource routes. T-20-16 to T-20-21 are mitigated as planned: segment validation and name encoding with zero-request tests, no signature or problem text in any error (asserted on Display, hint and endpoint), capped 422 echoes, `success:false` is an error, `.expect(1)` on every write mock, and `classify.rs` has no signature arm.

## Self-Check: PASSED

Commits `c15ec86` and `cf244fe` exist; `config_resources.rs` exists; no TODO, skip or ignore markers in the changed files; no co-author trailer on either commit; `.planning/config.json` and `.planning/research/.cache/` left unstaged.
