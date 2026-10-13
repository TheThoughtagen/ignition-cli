---
phase: 21-deploy-render
plan: 07
subsystem: deploy
tags: [render, uuid, sha2, rule-table, resource-identity, cd1, cd3]

requires:
  - phase: 21-02
    provides: decisions CD1 (live v8 uuid vector), CD2 (omitted lastModification loads), CD3 (per-type facts)
  - phase: 21-05
    provides: node.rs Gson writer and the stage 5 parsed-node side table
  - phase: 21-06
    provides: deploy/rules.rs (DENIED_TYPES, is_denied), tags layout, policy stage
provides:
  - "deploy::uuid: UUID_DOMAIN, name_based_uuid (sha2, RFC 9562 version 8), is_canonical_uuid"
  - "deploy::rules: UuidRule, TypeRule, TYPE_RULES (50 rows), type_rule"
  - "render stage 4 rule-table checks (no row, wrong structure) and stage 6 uuid keep / inject / synthesize"
  - "README Resource identity (attributes.uuid)"
affects: [21-08, 21-09]

tech-stack:
  added: []
  patterns:
    - "Rule table sorted by (module, type) bytes, binary-searched, each row commented with its evidence"
    - "Stage 6 edits the parsed Node and re-emits through node::write only for GatewayFormat files"

key-files:
  created:
    - crates/ignition-core/src/deploy/uuid.rs
  modified:
    - crates/ignition-core/src/deploy/rules.rs
    - crates/ignition-core/src/deploy/render.rs
    - crates/ignition-core/src/deploy/mod.rs
    - crates/ignition-core/tests/deploy_corpus.rs
    - crates/ignition-core/tests/deploy_render.rs
    - README.md

key-decisions:
  - "name_based_uuid reproduces the CD1 live vector 4516ad9c-d6b0-8d86-89b9-802c96e33a9e (and the CAP21_STALE vector), so the D-03 derivation is frozen"
  - "TYPE_RULES has 50 rows: 19 Singleton, 29 Named, 2 NamedNoUuid"
  - "NamedNoUuid keeps an existing valid uuid instead of refusing it (CD3: an 8.3.6 gateway wrote a uuid on store-and-forward-engine)"
  - "Tag types (CD5) are not in TYPE_RULES and are exempt from stage 4 rule checks and stage 6"
  - "A rule-less type is reported once, and only when it is allowlisted, not denied and not a tag type, so existing findings do not double up"

patterns-established:
  - "Findings name paths and the defect, never the uuid value found"

requirements-completed: [DRND-06]
requirements-partial: "DRND-05 stays Pending: see Requirement tracking"

actuals:
  tokens: 36000
  tasks: 2
  commits: 2

duration: 75min
completed: 2026-10-13
status: complete
---

# Phase 21 Plan 07: Resource identity (uuid) and the type rule table Summary

**A sha2 version-8 name-based `attributes.uuid` (matching the live-verified CD1 vector) and a 50-row evidence-backed `TYPE_RULES` table drive render stage 6: new named resources get a synthesized `resource.json`, missing uuids are injected into gateway-format files, existing uuids are kept byte for byte, and types without evidence are refused.**

## Accomplishments

- `deploy/uuid.rs`: SHA-256 over `ign-deploy:uuid:v1` NUL gateway NUL `module/type/name`, first 16 bytes, version nibble 8, RFC variant. Environment excluded, gateway included, exact UTF-8 bytes (NFC and NFD differ). Pinned to CD1 by `name_based_uuid_matches_live_vector`. No `sha1` or `uuid` crate (`cargo tree` count 0).
- `deploy/rules.rs`: `UuidRule`, `TypeRule`, `TYPE_RULES`, `type_rule`.
- `render.rs` stage 4 `enforce_rules`: manifest `types` entries and rendered types with no row are findings; a Singleton type with a name directory and a Named/NamedNoUuid type with a loose file under the type directory are findings.
- `render.rs` stage 6 `assign_uuids`: Named keep / invalid-refuse / inject (after `lastModification` and `legacyId`, GatewayFormat only, opaque refused) / synthesize (`scope`, `version` 1, `restricted` false, `overridable` true, `files` byte-ordered, `attributes {uuid, enabled true}` via `node::write`); Singleton and NamedNoUuid never get a uuid and need a `resource.json`; `uuids_minted` carries `module/type/name`, byte-sorted.
- README `#### Resource identity (attributes.uuid)` and three new "What render refuses" bullets.

## TYPE_RULES by class and evidence

| Class | Rows | Evidence |
| --- | --- | --- |
| Singleton | 19 | corpus census (all scope `A`; every one the OpenAPI extract lists is `named: false`, the others are Cirrus Link and edge types absent from the extract) |
| Named | 29 | 24 from the corpus census (scope `A`, uuid on every resource); 5 from CD3 only (`oauth2-client`, `alarm-notification-profile`, `roster-config`, `holiday`, `service-connector`, all scope `A`, v4 uuid); `database-translator` is corpus plus CD1 and CD3 |
| NamedNoUuid | 2 | corpus: `ignition/store-and-forward-engine` and `com.inductiveautomation.perspective/themes`, scope `G`, 0 of 5 and 0 of 4 carry a uuid |

No row for the four denied types or the four tag types. `type_rules_agree_with_corpus` checks presence, placement, uuid presence and scope for every corpus `resource.json`, and that every row is backed by corpus or the five CD3 types. `type_rules_agree_with_openapi_extract` checks named versus singleton against the extract (56 types, 39 named) wherever the extract lists the type.

## Task Commits

1. Task 1, `deploy::uuid` and `TYPE_RULES`: `8cb3b5f`
2. Task 2, render stage 6 and README: `a2460b9`

## Requirement tracking

- **DRND-06** (new named resources get a deterministic name-based `attributes.uuid` from a per-type rule table; singleton types get none; existing uuids are kept): fully delivered and tested (`new_named_resource_gets_resource_json_with_uuid`, `missing_uuid_is_injected_in_gateway_format_resource_json`, `existing_uuid_is_kept_byte_for_byte`, `singleton_and_named_no_uuid_get_none`, `uuid_is_stable_across_envs_and_differs_across_gateways`, the rule-table tests). Marked complete.
- **DRND-05** stays Pending. The synthesized-file half (files `ign` synthesizes through the Gson writer) is now delivered and the injected/synthesized bytes are the writer's output, but the requirement's "byte-identical on Linux, macOS and Windows" proof still needs the green three-OS CI run recorded by 21-09.

## Deviations from Plan

### Auto-fixed Issues

**1. [Orchestrator direction + CD3 contradiction] NamedNoUuid keeps an existing valid uuid**
- **Plan said:** a `resource.json` of a Singleton or NamedNoUuid type that carries `attributes.uuid` is refused.
- **Done instead for NamedNoUuid:** a valid existing uuid is kept (never synthesized, never injected); a malformed one is still refused. The Singleton refusal is as planned.
- **Why:** CD3 recorded an 8.3.6 gateway writing a uuid on `store-and-forward-engine`, so refusing it would refuse trees pulled from a current gateway. The no-uuid claim is a corpus fact, not gateway truth. Test: `singleton_and_named_no_uuid_get_none`.
- **Files:** `render.rs`, `deploy_render.rs`. **Commit:** `a2460b9`.

**2. [Rule 2 - Missing critical] Tag types are outside the rule table**
- **Issue:** the plan says "tag types included" in TYPE_RULES, but CD5 shows they do not fit the three classes (`tag-group` has `attributes: {}` with no uuid, `tag-type-definition` and `tag-definition` use `unary-resource.json` scope `G`, and `resource_key` misreads the tag-group name). Giving them rows would make stage 6 inject uuids into files that must not carry one, or refuse every tag render.
- **Fix:** tag-origin files (`Mapped.tags`) are skipped by stage 4 rule checks and by stage 6; the `tags/` layout (CD5) and provider allowlist (D-02) keep governing them. Documented in the `TYPE_RULES` doc comment and the README.
- **Files:** `render.rs`, `rules.rs`, `README.md`. **Commit:** `a2460b9`.

**3. [Rule 1 - Test breakage from fail-closed] Existing render tests migrated to real types**
- **Issue:** 14 pre-21-07 tests used invented types (`m/a`, `n/b`, `m/t`) and mixed singleton plus named files in one type; both are now refused by design.
- **Fix:** moved to `ignition/schedule` (Named), `ignition/system-properties` (Singleton), `ignition/email-profile`, `com.inductiveautomation.historian/historian-provider` and themes (NamedNoUuid) with valid gateway-format `resource.json` fixtures; expectations that counted files or named exact bytes were updated for the synthesized `resource.json`. The "singletons first" ordering assertion became a (module, type, name) byte-order assertion, since a singleton and named resources no longer share a type. `each_denied_type_is_refused_even_when_allowlisted` now asserts `user-source-x` is refused only for lacking a rule (not as denied). Intent of each test is kept. `render_digest_golden` (`RENDER_GOLDEN`), `corpus_render_is_byte_identical` (`uuids_minted` empty) and the tracer render are unchanged.
- **Files:** `deploy_render.rs`, `render.rs` (unit tests). **Commit:** `a2460b9`.

**4. TDD note.** Tests and implementation landed together per task (the tests need the new API to compile); each was run against the implementation before its commit. No separate failing-test commits.

**5. Rule-less type reporting scope.** The plan's rule-less finding is emitted only for allowlisted, non-denied, non-tag types, so the D-02, deny-list and tag-type findings of 21-06 are not doubled (`unlisted_types_are_refused_together` still counts one finding per type directory).

## Issues Encountered

- An unquoted shell heredoc swallowed backticks in a first generation of `rules.rs` doc comments; one doc line was repaired by hand and the file reviewed.
- One stray `rm` of a nonexistent `/tmp_rows.txt` in a scratch command was denied by the safety check and was not retried or worked around (nothing needed removing).

## Known Stubs

None.

## Threat Flags

None. T-21-28 (domain-separated SHA-256, NUL separators, `uuid_fields_are_separated`, input sensitivity tests), T-21-29 (CD1 vector pinned, three-class table checked against corpus, OpenAPI and captures, fail-closed on rule-less types), T-21-30 (opaque `resource.json` refused, `opaque_resource_json_without_uuid_is_refused`) and T-21-31 (malformed uuid refused, never replaced, value never echoed, `invalid_existing_uuid_is_refused`) are covered.

## Verification

- `cargo test -p ignition-core --lib deploy::`: all passed (uuid 5, rules 3, render unit 4)
- `cargo test -p ignition-core --test deploy_render`: 32 passed (23 pre-existing, 9 new stage-6 tests)
- `cargo test -p ignition-core --test deploy_corpus`: 5 passed (includes `type_rules_agree_with_corpus`)
- `cargo test -p ignition-cli --test contract_deploy`: 5 passed, `RENDER_GOLDEN` untouched
- `cargo test --workspace`: no failures
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`, `cargo build -p ignition-cli --no-default-features`: clean
- `cargo tree -p ignition-core | grep -cE '^.* (sha1|uuid) v'`: 0
- `grep -n CD1 crates/ignition-core/src/deploy/uuid.rs`: present; `grep -c 'Resource identity' README.md`: 3
- Determinism grep over `deploy/`: only `yamlio.rs:448` (21-03 test-only guard)

## Self-Check: PASSED

- FOUND: crates/ignition-core/src/deploy/uuid.rs, crates/ignition-core/src/deploy/rules.rs
- FOUND commits: 8cb3b5f, a2460b9
