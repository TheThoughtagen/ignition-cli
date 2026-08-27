---
phase: 05-webdev-backend-tag-operations
verified: 2026-08-27T03:10:00Z
status: passed
score: 7/7 gap-closure must-haves verified (4/4 Gap-1 truths + 3/3 Gap-2 truths); prior 5/5 holds — no regressions
re_verification:
  previous_status: passed
  previous_score: 5/5
  gaps_closed:
    - "UAT Gap 1 (05-07): import-denial seam — success:false on HTTP 200 surfaces as exit 6 import_denied with the gateway's problem text at ALL four import callers (one seam), put-new lands via parent-folder resource.json descriptors, live e2e_projects gate green with the used-project witness"
    - "UAT Gap 2 (05-08): alarms view→ack loop — full event_id printed in the human table (copy-paste verbatim), short-prefix expansion in ack with honest exit-2 refusals, route traceback surfaced in webdev_route_error"
  gaps_remaining: []
  regressions: []
human_verification:
  - test: "UAT re-test (the 2 previously-failed items) against the live rig"
    expected: "10/12 → 12/12: resource put of a NEW resource followed by resource get returns the written content; the alarms view→ack copy-paste loop acks verbatim-printed ids and prefix ids"
    why_human: "Live-gateway behavior needs the commissioned rig + IGNITION_LIVE_* env + mutations opt-in; both executors ran the exact failure loops green live (05-07 gate transcript, 05-08 ack-loop transcript) but the UAT re-test is the confirmatory closure"
  - test: "Independent live-gate sweep (optional, regression belt)"
    expected: "cargo test -p ignition-cli --test e2e_projects --test e2e_webdev -- --ignored all green (rig left healthy at 1.0.0 per 05-08-SUMMARY, fresh trial from 02:05Z)"
    why_human: "Env-gated live tests cannot run in a static verification pass"
---

# Phase 5: Gap-Closure Re-Verification Addendum

**Phase Goal:** Ship the CLI's versioned WebDev routes, deploy them, and operate the full tag lifecycle (providers, browse, read/write, config CRUD, UDTs, alarms, history, bulk transfer) — the ignition-mcp replacement bar.

**Verified:** 2026-08-27T03:10:00Z
**Status:** passed
**Re-verification:** Yes — after gap closure (05-07 + 05-08, following UAT's 10-passed/2-gap outcome)

**Context:** The initial 05-VERIFICATION.md (2026-08-25) passed 5/5 must-haves. Subsequent `/gsd-verify-work` UAT found 2 gaps the static pass missed — exactly the class of live-behavior gaps static verification flags for human testing. Gap-closure plans 05-07 (import-denial seam + put-new landing) and 05-08 (alarms view→ack loop + traceback surfacing) were executed. This addendum verifies both closures against the actual codebase and confirms the prior 5/5 still holds.

## Gap 1 Closure (05-07): Import-Denial Seam + Put-New Landing

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | resource put of a NEW resource LANDS it — follow-up `resource get` returns the written content (fresh AND used project) | ✓ VERIFIED | Descriptor surgery in `client/resources.rs`: `FOLDER_DESCRIPTOR = "resource.json"`, `merge_descriptor_member` (existing parent → merge basename into `files`), `synthesized_descriptor` (absent → live-proven shape: scope G, version 1, restricted false, overridable true, `files:[basename]`), `DescriptorSurgery` resolved BEFORE the copy loop with descriptor-before-file ordering (lines 95–320). Unit pins: `replace_member_appends_when_absent`, `appends_merging_existing_descriptor`, `appending_a_descriptor_authors_it_explicitly`, `append_over_corrupt_descriptor_refuses` (internal, not silent), `remove_member_leaves_descriptor_to_gateway_reconciliation`. Live witness in `e2e_projects.rs` step 10b: `SCRATCH3_PATH = ignition/script-python/uat2/second.py` on the USED project, read-back landing assert + list-contains check incl. the synthesized `uat2/resource.json` (lines 426–486). Live gate first-ever run green (commit `8dab0a5`, transcript in 05-07-SUMMARY). Spike record honest: candidate 1 (dir-entry ancestors) DISPROVEN live — silent success:true no-op; candidate 3 (format inspection) won |
| 2 | gateway import denial ({success:false} on HTTP 200) surfaces as exit 6, slug `import_denied`, problem text verbatim — never ok:true | ✓ VERIFIED | Pure helper `projects::import_denied` (client/projects.rs:257) refuses ONLY on explicit bool `success:false` (missing key / true / string-"false" / status-fallback all stay opaque-success — never refuse on absence of proof); unit pins cover every family member. Seam at `ReqwestGatewayApi::project_import` (client/mod.rs:1095–1101) returns `CoreError::ImportDenied { project, problem, endpoint }`. error.rs: variant (line 51), slug `import_denied` (line 316), exit 6 (line 369), actionable hint naming `project export` as hand-edit baseline (line 393), endpoint thread (line 566), enumerated `exit_code_mapping_enumerated` case (lines 811–817). README exit table carries the row (error.rs doc table line 15). Wiremock goldens: `resource_put_import_denied_exits_6_golden` + `resource_delete_import_denied_exits_6` (contract_resources.rs:798–886, problem text `resource already exists: ResourceId{...}` verbatim in message + endpoint) and the project-import denial golden (contract_projects.rs:777–834) |
| 3 | resource put, resource delete, project import, and webdev deploy ALL inherit denial honesty via ONE seam | ✓ VERIFIED | Exactly one denial check exists — inside `project_import` (client/mod.rs:1095). All four callers route through it: resource put/delete (actions/resources.rs), `project import` verb (actions/projects.rs:541 `validate_import(&zip)?` → import), webdev deploy (actions/webdev.rs:171 → `project_import(project, zip, true)`, confirmed unchanged in initial verification and untouched by gap commits). No per-caller checks added — grep shows `import_denied` referenced only at the seam + helper + tests. The plan's forbidden-pattern rule (per-caller checks) respected |
| 4 | live e2e_projects loop runs green with IGNITION_LIVE_MUTATIONS=1 (put-new on fresh AND used project) | ✓ VERIFIED (code + commit evidence; live re-run stays the human confirmatory item) | `e2e_projects.rs`: `#[ignore]` gate requiring `IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN + IGNITION_LIVE_MUTATIONS=1` (lines 16–220, refuses otherwise), mutations helper (line 87–89). Step 10b is the UAT's exact failure shape — new member in new folder chain on a USED project — with landing read-back + synthesized-descriptor presence asserts. Commit `8dab0a5` (test) ran the full loop live green (5.6s, transcript highlights in 05-07-SUMMARY incl. two-sided honesty, replace-not-merge overwrite semantics, cleanup). Suite-inclusion verified: the gate compiles and is counted among the ignored-by-design env-gated tests |

### Bonus Fix Verified (05-07 Rule-2 deviation): Truncated-Zip Import Guard

The live spike discovered the gateway accepts truncated zips (valid PK magic, broken tail) with `success:true changes:[]` and OVERWRITE-WIPES the target — silent data loss. Fix verified: `validate_import` (actions/projects.rs:144) walks and decompresses every member via `zip::ZipArchive` before any network; corruption refuses `invalid_import_file` exit 2 pre-upload. Pinned three ways: unit `import_refuses_truncated_zip_before_any_network` (line 1057), binary golden `project_import_truncated_zip_exits_2_golden` (contract_projects.rs:943), README import row updated. This is a real critical-class fix, not scope creep.

## Gap 2 Closure (05-08): Alarms View→Ack Loop + Traceback

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | the eventId `tags alarms active` prints in human mode can be copied VERBATIM into `tags alarms ack` | ✓ VERIFIED | `render_tags_alarms_active_human` (render.rs:1062–1074) prints `alarm.event_id` verbatim in a `{:<38}` column — no truncation, short_id gone. Ack passthrough: `normalize_ack_ids` returns all-full-shape input unchanged (`is_full_uuid_shape`: len==36 + hyphens at 8/13/18/23 — shape check only, deliberately no uuid dep). Contract goldens moved to full ids; the loop works from the table to the wire unchanged. Live transcript in 05-08-SUMMARY shows the exact UAT-failure sequence green: printed full UUID → ack verbatim → `acknowledged 1 alarm(s)` exit 0 |
| 2 | short prefixes still work — expanded via the active list; ambiguous/unknown refuse invalid_input exit 2 naming candidates/miss | ✓ VERIFIED | `normalize_ack_ids` (actions/tags.rs:618–660): any short id triggers ONE `tags_alarms_active` lookup (its own precondition — the family's locked correctness-over-latency trade); `starts_with` prefix match; exactly-one → substitute full UUID; multiple → InvalidInput listing the FULL candidate ids; zero → InvalidInput naming the miss + "already acknowledged or cleared" + "full ids ride `tags alarms active --json`". Mixed short/full expand independently. Wire-level proof: `contract_tags.rs` expansion golden asserts the recorded acknowledge request body's `eventIds` carried the EXPANDED full uuid `3f2504e0-4f89-11d3-9a0c-0305e82c3301` from input `3f2504e0` (line 1603) — request-level, not just output-level. Refusal goldens: ambiguous (both full candidates in message) + unknown (`deadbeef` + hint), both exit 2 `invalid_input` (lines 1612–1657). Unit pins for both refusals at actions/tags.rs:3011–3048 |
| 3 | route denials carrying a traceback show it in `webdev_route_error` output | ✓ VERIFIED | `RouteBody::Denied` and `RouteProbe::Denied` grew `traceback: Option<String>`, parsed from the envelope's `/error/traceback` (client/webdev.rs:113–170, unit-pinned both directions: no-traceback → None, traceback → carried). `denial_to_error(..., traceback: Option<&str>, ..)` appends `\nroute traceback: {tb}` when present; absent → byte-identical message (unit pin "the message rides VERBATIM (no suffix)"). Threaded at BOTH call sites: the route-call path (client/mod.rs:901–909) and the probe/precondition's direct `WebdevRouteError` construction (actions/webdev.rs:250–258, identical append). Contract golden `tags_route_error_traceback_surfaces_in_the_message` green. Variant/slug/exit untouched — taxonomy frozen as planned. Live proof: the full-shaped INVALID uuid case in the 05-08 transcript surfaces `NumberFormatException ... route traceback:` at exit 6 — the former black box, now diagnosable |

## Regression Check: Prior 5/5 Must-Haves

The gap closures touched error.rs (additive variant only), actions/tags.rs, client/webdev.rs, client/mod.rs, actions/webdev.rs, render.rs, client/resources.rs, actions/projects.rs — all inside the original artifacts. Quick regression sweep:

| Prior truth | Status | Evidence |
|---|---|---|
| 1. Deploy/status/precondition machinery | ✓ HOLDS | All 5 route files untouched by gap commits (commit stats confirm) and still byte-0 `def doPost`; `ROUTE_BUNDLE_VERSION = "1.0.0"` == `webdev/routes/VERSION` 1.0.0; deploy still rides `project_import(project, zip, true)` — now additionally denial-honest (strictly stronger) |
| 2. Provider CRUD / browse / read / write | ✓ HOLDS | actions/tags.rs surface intact; suite green (all tags contract targets pass) |
| 3. Config CRUD + UDT | ✓ HOLDS | tags_config/udt actions untouched by gap commits; suite green |
| 4. Alarms + history | ✓ HOLDS + STRONGER | ack now normalizes ids before the wire (precondition still first); `alarm_journal_missing` slug mapping intact; traceback enrichment is additive (absent-traceback messages byte-identical — no-traceback goldens unmoved) |
| 5. Export/import collision matrix | ✓ HOLDS + STRONGER | Collision semantics unchanged; import family gains denial honesty + corrupt-zip refusal (both strictly safer) |

**Workspace suite: 523 passed, 0 failed** (env-gated live tests ignored by design). Zero TODO/FIXME/XXX/HACK/PLACEHOLDER/unimplemented across all 9 modified files.

## Commits Verified

| Commit | Claim | Status |
|---|---|---|
| `0834e81` | Task 1: denial seam (error variant, helper, seam, goldens, README) | ✓ Present, file list matches |
| `719924d` | Task 2: descriptor surgery (+289 lines client/resources.rs) | ✓ Present |
| `9fb9152` | Rule-2 fix: truncated-zip guard | ✓ Present |
| `8dab0a5` | Task 3: live e2e gate + used-project witness | ✓ Present (+100 lines e2e_projects.rs) |
| `0ff9302` | 05-08 Task 1: view→ack loop (render + normalization, 368 insertions) | ✓ Present |
| `b1932e8` | 05-08 Task 2: traceback surfacing | ✓ Present |

## Anti-Patterns Found

None. Zero stub markers across all modified files; every claimed behavior has a matching test pin (unit and/or wiremock golden) that passes in the suite.

## Human Verification Required

### 1. UAT re-test of the 2 previously-failed items

**Test:** Against the live rig (left healthy at 1.0.0, fresh trial per 05-08-SUMMARY): `resource put` a NEW resource → `resource get` reads it back; `tags alarms active` (human) → copy the printed eventId → `tags alarms ack <id>` exits 0; also ack an 8-char prefix.
**Expected:** 10/12 → 12/12 UAT. Both executors ran these exact loops green live (transcripts in both summaries) — the UAT re-test is the confirmatory closure.
**Why human:** Requires commissioned gateway + env opt-in; static verification cannot execute live mutations.

### 2. Optional: independent live-gate sweep

**Test:** `IGNITION_LIVE_URL=… IGNITION_LIVE_TOKEN=… IGNITION_LIVE_MUTATIONS=1 cargo test -p ignition-cli --test e2e_projects --test e2e_webdev -- --ignored`
**Expected:** All gates green (e2e_projects now includes the put-new-on-used-project witness).
**Why human:** Env-gated by design.

## Gaps Summary

No remaining gaps. Both UAT gaps are closed at all three levels (exists, substantive, wired) with wire-level and live proof:

- **Gap 1** closed by a single-seam denial fix (the architecturally correct shape — one client method, zero per-caller checks), a live-proven descriptor-landing rule that displaced the plan's own disproven primary candidate via its spike protocol, a bonus critical data-loss guard, and the previously-never-run live gate now green and permanently extended with the exact UAT failure shape as a witness.
- **Gap 2** closed both directions of the loop (print-what-ack-accepts AND accept-what-was-printed), with honest exit-2 refusals and request-level wire proof that expansion happened before the acknowledge call; the traceback fix opens the black box that hid the root cause during UAT.

Notable: the UAT→gap-closure cycle worked exactly as designed — the initial static verification's flagged human items were precisely where the 2 gaps surfaced, and both closure plans leave the phase stronger than the original 5/5 state (denial honesty + data-loss guard are net-new safety properties inherited by every import caller).

---

_Verified: 2026-08-27T03:10:00Z_
_Verifier: Claude (gsd-verifier) — gap-closure re-verification_
_Builds on: 05-VERIFICATION.md (2026-08-25, initial, passed 5/5)_
