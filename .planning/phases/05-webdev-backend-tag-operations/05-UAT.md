---
status: diagnosed
phase: 05-webdev-backend-tag-operations
source: [05-01-SUMMARY.md, 05-02-SUMMARY.md, 05-03-SUMMARY.md, 05-04-SUMMARY.md, 05-05-SUMMARY.md, 05-06-SUMMARY.md]
started: 2026-08-26T19:20:18Z
updated: 2026-08-26T22:05:00Z
---

## Current Test

[testing complete]

## Tests

### 1. Resource list/get against a real gateway
expected: `ign resource list --project <p>` lists members; `ign resource get` returns {project, path, content_kind, content} with readable text (the Phase 3 blocker closed via zip surgery)
result: issue
reported: "get/list are honest, but put of a NEW resource claims success while nothing lands. `ign resource put uatproj2 ignition/script-python/uat/hello2.py --file ... --yes` exits 0 ok:true, then `resource get` of the same path exits 6 not_found and `resource list` stays empty (same for scripts/... paths). Wire-level repro (manual curl + python zip): the gateway's import answers HTTP 200 with {success:false, problem:'resource already exists: ResourceId{...}'}} — the CLI does not check the import body's success flag. REPLACE of an existing member and DELETE both work live."
severity: major

### 2. Resource put/delete destructive guards
expected: `ign resource put`/`delete` WITHOUT --yes exit 2 with profile:null, zero network work, and a refusal message naming the overwrite-import consequence; WITH --yes they succeed and a follow-up `resource get` reflects the change
result: pass

### 3. WebDev-dependent command refusal (routes not deployed)
expected: On a gateway WITHOUT the CLI routes deployed, `ign tags browse` exits 6 (routes_not_deployed) with an actionable hint naming `ign webdev deploy`. Skip if every gateway you have at hand already has routes deployed.
result: pass

### 4. `ign webdev deploy` + `ign webdev status`
expected: deploy exits 0 installing the ign-cli project routes; status exits 0 showing the route sweep with routes present at version 1.0.0; the scriptExec secret NEVER appears in any output (any mode)
result: pass

### 5. Tag provider list (native REST, no deploy needed)
expected: `ign tags provider list` exits 0 showing providers with tag counts/health (e.g. default provider, System-managed flagged); works even before any webdev deploy
result: pass

### 6. Provider create + guarded delete
expected: `ign tags provider create --name <test>` exits 0 and the provider appears in list; `provider delete` WITHOUT --yes refuses pre-resolution (exit 2, zero wire work); WITH --yes it deletes (find→signature chain)
result: pass

### 7. Tags browse tree + read quality-as-data
expected: `ign tags browse` renders an indented tree in human mode (flat list in --json); `ign tags read --path <nonexistent>` exits 0 returning quality Bad_NotFound as DATA (never an error)
result: pass

### 8. Tags write + read round-trip
expected: `ign tags write --path <existing-tag> --value 42` exits 0; a follow-up read returns the value 42 with Good quality; `--value '{bad json obj'`-style arrays/objects refuse invalid_input exit 2
result: pass

### 9. Tag config CRUD + UDTs
expected: `tags config create` then `config get` returns the config with stringified value/defaultValue re-parsed into real JSON (nested children too); `config edit` changes one node (no --yes needed); `config delete` refuses without --yes; `tags udt types`/`udt def` list and dump definitions
result: pass

### 10. Export/import collision matrix
expected: `tags export` with no -o writes <last-segment>.json in cwd; `-o -` prints raw payload (pipeable); `tags import` with default abort on existing tags refuses tag_collision exit 6 naming the collisions BEFORE any write; overwrite requires --yes
result: pass

### 11. Alarms active + journal-gated history
expected: `ign tags alarms active` exits 0 (empty table OK on a quiet rig); `tags alarms history` on a default rig (no alarm journal) exits 6 alarm_journal_missing with the provisioning-chain hint
result: pass

### 12. Alarm ack lifecycle + tag history query
expected: With a configured alarm tag: write past setpoint → `alarms active` shows Active/Unacknowledged → `alarms ack --username <you>` flips to Acknowledged (no --yes required, username mandatory). `tags history --path <hist-tag> --start ... --end ...` returns {columns, rows} with t_stamp preserved. Skip if no historian/alarm fixture at hand.
result: issue
reported: "The lifecycle WORKS with the full UUID (ack acknowledged:1, state flipped to Active, Acknowledged) and history query is structurally correct (t_stamp + provider-relative column, zero rows = the documented binding limitation). But the human-mode `alarms active` table prints SHORT eventIds (59c5b7e3) — copying that ID into `alarms ack` fails with generic webdev_route_error; the real cause ('Invalid UUID string') is only visible in the route's traceback via raw curl. The --json shape carries the full UUID (workaround)."
severity: minor

## Summary

total: 12
passed: 10
issues: 2
pending: 0
skipped: 0

## Gaps

- truth: "resource put of a NEW resource lands the resource in the project"
  status: failed
  reason: "User reported: put claims ok:true (exit 0) but resource get/list show nothing — gateway import answers HTTP 200 {success:false, problem:'resource already exists'} which the CLI does not surface; replace-existing and delete work"
  severity: major
  test: 1
  root_cause: "Two layers. (1) Client seam: ReqwestGatewayApi::project_import (crates/ignition-core/src/client/mod.rs:1043) parses the 200 body as an opaque object and NEVER checks its `success` flag — the 200-denial class handled for WebDev routes but missed here; ImportOutcome carries the body verbatim. (2) Actions layer: actions/resources.rs put/delete call `api.project_import(...).await?` and DISCARD the ImportOutcome entirely (lines 234/253) — even a checked seam wouldn't help today. Underlying gateway behavior (live-proven 8.3.3): overwrite-import of an export zip with an APPENDED member (new folder chain, e.g. ignition/resources/script-python/uat/hello.py — and even a brand-new top-level com.example/... on a fresh project) answers {success:false, problem:'resource already exists: ResourceId{...}'} riding HTTP 200; REPLACE of an existing member and member REMOVAL import cleanly. webdev deploy's import path DOES check success (its output carries import:{success:true}) — only the resource family misses it."
  artifacts:
    - path: "crates/ignition-core/src/client/mod.rs"
      issue: "project_import: 200 body parsed opaquely; success:false never mapped to an error"
    - path: "crates/ignition-core/src/actions/resources.rs"
      issue: "put/delete discard ImportOutcome; no post-write read-back verification"
    - path: "crates/ignition-cli/tests/contract_resources.rs"
      issue: "goldens pin the opaque-success behavior (no import-denial fixture)"
  missing:
    - "project_import (or the resources actions) must surface {success:false, problem} as an error (exit 6 family, e.g. import_failed / resource_write_failed with the problem message) — the WebDev 200-denial precedent"
    - "Decide the put-new path: either a working append strategy for new members (folder resource descriptors in the surgery zip, or another gateway-accepted shape) or an honest read-back verification after import that refuses when the member did not land; put-new must never report ok when get would say not_found"
    - "Wiremock fixture pinning the import success:false denial + contract golden for the surfaced error"
    - "The same unchecked-success audit for `project import` (03-02's user-facing import verb) — it shares the seam"
  debug_session: ""

- truth: "alarms ack accepts the eventId as printed by alarms active"
  status: failed
  reason: "User reported: human-mode alarms active prints short eventIds (59c5b7e3); ack with that ID fails generic webdev_route_error; real cause 'Invalid UUID string' only in route traceback; --json full UUID works"
  severity: minor
  test: 12
  root_cause: "Render/action split: render.rs's alarms-active table deliberately shortens event_id for column width, but tags_alarms_ack passes ids through verbatim and the gateway's system.alarm.acknowledge requires FULL UUIDs — the short form dies inside the route (IllegalArgumentException) surfacing as the generic webdev_route_error slug with no traceback in CLI output. The two halves of the same human workflow (view → copy → ack) were never connected; the e2e lifecycle gate used the full UUID from the JSON shape, so this shipped."
  artifacts:
    - path: "crates/ignition-cli/src/render.rs"
      issue: "alarms active table prints short eventId — the only ID a human sees"
    - path: "crates/ignition-core/src/actions/tags.rs"
      issue: "tags_alarms_ack does no id normalization/expansion (short form unrecoverable without a lookup)"
  missing:
    - "Pick one: print the FULL UUID in the alarms active table (widens the column), or expand short ids in ack via a fresh active-alarms lookup (prefix match), or accept both and document; the view→ack loop must work with what the table shows"
    - "Optional: surface route traceback in webdev_route_error diagnostics (-vv at minimum) so this class isn't a black box"
  debug_session: ""
