---
phase: 05-webdev-backend-tag-operations
verified: 2026-08-25T15:30:00Z
status: passed
score: 5/5 must-haves verified
re_verification: false
human_verification:
  - test: "Re-run the five live e2e gates against a commissioned 8.3.x rig"
    expected: "All five gates green (deploy/status/scriptExec loop, provider browse/read/write, config export/import round-trip, alarm lifecycle, historian spike). Executor ran them twice on a real 8.3.3 rig (port 9089, commit 054df4e), but a UAT re-run independently confirms."
    why_human: "Live-gateway behavior requires a real commissioned gateway + IGNITION_LIVE_* env vars + mutations opt-in; cannot be verified in a static pass"
  - test: "Spot-check human-facing CLI output quality (tree rendering for `ign tags browse`, alarm/history tables)"
    expected: "Readable aligned tables and indented trees; snapbox goldens pin the shapes but human legibility is a judgment call"
    why_human: "Visual output quality is subjective; goldens verify structure not legibility"
---

# Phase 5: WebDev Backend + Tag Operations Verification Report

**Phase Goal:** A user can deploy the CLI's own versioned WebDev routes to a gateway and then operate the complete tag lifecycle — providers, browse, read/write values, config CRUD, UDTs, alarms, history, bulk transfer — reaching the ignition-mcp replacement bar.
**Verified:** 2026-08-25T15:30:00Z
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `ign webdev deploy` installs versioned routes; `ign webdev status` verifies; WebDev-dependent commands refuse with actionable error on mismatch | ✓ VERIFIED | 5 route sources byte-0 contract (`def doPost` at byte 0, all confirmed on disk); 13-member `ROUTE_FILES` embedded via `include_str!` in `crates/ignition-core/src/webdev/mod.rs`; `ROUTE_BUNDLE_VERSION = "1.0.0"` == `webdev/routes/VERSION`; `webdev_deploy` → `project_import(project, zip, true)` (actions/webdev.rs:171); `webdev_status` probes every route; `webdev_precondition` refuses exit 6 with "run `ign webdev deploy`" (error.rs:455,479 — both `routes_not_deployed` and `route_version_mismatch` shapes). Live gate `live_webdev_deploy_status_scriptexec_loop` passed on 8.3.3 per 05-06 SUMMARY |
| 2 | Provider list/create/delete, browse as filtered tree, read (single/batch), write — through deployed routes | ✓ VERIFIED | `tag_provider_list/create/delete` on native `/data/api/v1/resources/**/ignition/tag-provider` paths (client/tags.rs:53-73); `tags_browse/read/write` actions (actions/tags.rs:292,321,354); 18 `webdev_route_call`/`webdev_route_probe` refs in actions/tags.rs; Property-children filter + substring filter in `TagsCommand::Browse` CLI arm; live gate `live_tags_provider_browse_read_write_loop` passed |
| 3 | Config CRUD (JSON in/out) + UDT types/definitions | ✓ VERIFIED | `tags_config_get/create/edit/delete` + `tags_udt_types/def` (actions/tags.rs:960-1127); tagConfig route dispatches all 7 actions (doPost.py lines 97-137); recursive `reparse_stringified` for stringified value/defaultValue (actions/tags.rs:851); live round-trip gate passed with read-back oracle (`Good/123`) |
| 4 | Active alarms, alarm history, acknowledge, tag history query | ✓ VERIFIED | `tags_alarms_active/history/ack` + `tags_history_query` (actions/tags.rs:464-636); 3-arg ack wire form with remainder-honest count; `no_alarm_journal` → `AlarmJournalMissing` exit-6 mapping at the single denial site (client/webdev.rs:173); `t_stamp` preserved verbatim; alarms route dispatches active/history/acknowledge (doPost.py:81-137); live gates: full alarm lifecycle (configure → trigger → poll to `Active, Unacknowledged` → ack → `Active, Acknowledged`) + InternalHistorian provisioning passed |
| 5 | Bulk export/import with collision policy defaulting to abort | ✓ VERIFIED (JSON-native; xml/csv documented deferral) | `tags_export`/`tags_import` (actions/tags.rs:1247,1372); `CollisionPolicy` reused from Phase 3; abort = browse pre-check → `tag_collision` exit 6 BEFORE any write; overwrite = `--yes` with no pre-check; live collision matrix green (abort clean, abort refusal, overwrite). **Deviation:** JSON only — xml/csv deferred to backlog as documented format-discretion (code comment actions/tags.rs:1245, README:157,653; pre-cleared per planner refinement). Export payload normalization handles all three live shapes (single subtree / `{tags:[...]}` wrapper / bare array) |

**Score:** 5/5 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `webdev/routes/**/tags/doPost.py` | version/browse/read/write dispatch | ✓ VERIFIED | 125 lines, byte-0 `def doPost`, all 4 actions dispatched |
| `webdev/routes/**/tagConfig/doPost.py` | 7-action config route | ✓ VERIFIED | 144 lines, byte-0, all 7 actions dispatched (lines 97-137) |
| `webdev/routes/**/alarms/doPost.py` | active/history/acknowledge | ✓ VERIFIED | 149 lines, byte-0, all actions + structured `no_alarm_journal` denial |
| `webdev/routes/**/tagHistory/doPost.py` | version/query | ✓ VERIFIED | 102 lines, byte-0, t_stamp verbatim passthrough |
| `webdev/routes/**/scriptExec/doPost.py` | secret-gated version/exec | ✓ VERIFIED | 186 lines, fail-closed (`SECRET = None or '__IGN_CLI_SECRET__'` + leading-underscore shape detector), SHA-256 constant-time compare via `java.security.MessageDigest` |
| `crates/ignition-core/src/webdev/mod.rs` | embedded bundle manifest | ✓ VERIFIED | `ROUTE_BUNDLE_VERSION`, 13-member `ROUTE_FILES`, `SCRIPT_EXEC_TEMPLATE`, all `include_str!`-wired; `pub mod webdev` registered in lib.rs:28 |
| `crates/ignition-core/src/client/webdev.rs` | route_call/probe/deploy zip | ✓ VERIFIED | `webdev_route_call`, `webdev_route_probe`, `build_deploy_zip`, `/system/webdev/{project}/cli/{route}` paths, `no_alarm_journal` → AlarmJournalMissing |
| `crates/ignition-core/src/actions/webdev.rs` | deploy/status/precondition | ✓ VERIFIED | All three actions + secret lifecycle + redaction unit tests |
| `crates/ignition-core/src/client/tags.rs` | native provider CRUD | ✓ VERIFIED | list/find/create/delete on `ignition/tag-provider` resource family, `BrowseEntry` model |
| `crates/ignition-core/src/actions/tags.rs` | full 17-action tag surface | ✓ VERIFIED | Provider CRUD, browse/read/write, alarms active/history/ack, history query, config CRUD, UDT types/def, export/import — all present (lines 120-1372) |
| `crates/ignition-core/src/error.rs` | exit-6 slugs | ✓ VERIFIED | `webdev_unlicensed`, `tag_collision`, `alarm_journal_missing`, `routes_not_deployed`, `route_version_mismatch` — both exit-table places |
| `crates/ignition-cli/src/cli.rs` | CLI command arms | ✓ VERIFIED | `WebdevCommand`, `TagsCommand` (Provider/Browse/Read/Write/Config/Udt/Export/Import + alarms/history arms), all documented subcommands |
| `crates/ignition-cli/tests/e2e_webdev.rs` | five live gates | ✓ VERIFIED | Exactly 5 `#[ignore]`-gated `tokio::test`s matching the claimed gate names; env-var opt-in (URL + TOKEN + MUTATIONS=1); LIVE_GATE serializer + pre-clean helpers present |
| `crates/ignition-core/tests/tags_contract.rs` | wiremock pins | ✓ VERIFIED | Passing in workspace suite |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|----|--------|---------|
| `webdev/mod.rs` | `webdev/routes/**` | `include_str!` | ✓ WIRED | 13 manifest entries + separate SCRIPT_EXEC_TEMPLATE |
| `client/webdev.rs` | `system/webdev/{project}/cli/{route}` | POST action dispatch | ✓ WIRED | Path builder + body-envelope parse; never branches on HTTP status |
| `actions/webdev.rs` | embedded bundle | `build_deploy_zip` over ROUTE_FILES | ✓ WIRED | Deploy zip built from embedded manifest, no filesystem dependency |
| `actions/webdev.rs` | `project_import` | overwrite=true import | ✓ WIRED | actions/webdev.rs:171 |
| `client/tags.rs` | `ignition/tag-provider` | native config-resource REST | ✓ WIRED | list/create/find/delete-by-signature paths (lines 53-73) |
| `actions/tags.rs` | `webdev_route_call` | tags/tagConfig/alarms/tagHistory routes | ✓ WIRED | 18 call/probe references across the file |
| `actions/tags.rs` | `webdev_route_probe` | version precondition | ✓ WIRED | Shared precondition refuses before every webdev-dependent action |
| `actions/tags.rs` | collision conventions | `CollisionPolicy` (Phase 3) | ✓ WIRED | Abort pre-check → `tag_collision` exit 6; overwrite `--yes` no-precheck |
| `actions/tags.rs` | `alarm_journal_missing` slug | `no_alarm_journal` route code | ✓ WIRED | Mapped at the single `denial_to_error` site (client/webdev.rs:173) |
| `cli.rs` → `main.rs` → actions | command dispatch | clap arms → actions | ✓ WIRED | All commands dispatch; workspace tests green |

### Requirements Coverage

| Requirement | Status | Blocking Issue |
|-------------|--------|----------------|
| WEB-01 (deploy + status) | ✓ SATISFIED | — |
| WEB-02 (version negotiation refusal) | ✓ SATISFIED | — |
| TAGS-01 (provider CRUD) | ✓ SATISFIED | — |
| TAGS-02 (browse tree + filter) | ✓ SATISFIED | — |
| TAGS-03 (read single/batch) | ✓ SATISFIED | — |
| TAGS-04 (write value) | ✓ SATISFIED | — |
| TAGS-05 (config CRUD JSON in/out) | ✓ SATISFIED | — |
| TAGS-06 (UDT types/definitions) | ✓ SATISFIED | — |
| TAGS-07 (alarms active/history/ack) | ✓ SATISFIED | — |
| TAGS-08 (tag history query) | ✓ SATISFIED | — |
| TAGS-09 (bulk export/import, json/xml/csv) | ✓ SATISFIED with documented deviation | JSON-native only; xml/csv deferred to backlog as format-discretion (pre-cleared, documented in code + README) |

**Housekeeping note:** REQUIREMENTS.md checkboxes still read "Pending" — the orchestrator/verify-work flow should flip WEB-01..TAGS-09 to complete.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| — | — | Zero TODO/FIXME/XXX/PLACEHOLDER across all phase key files | — | — |

No stub patterns, no console-log-only handlers, no empty implementations. All 18 task commits across the six plans verified present in git history. Full workspace test suite: **0 failures** (250+ unit/contract tests pass; 8 ignored = the env-gated live e2e gates, by design).

### Human Verification Required

### 1. Live e2e gate re-run

**Test:** Set `IGNITION_LIVE_URL` + `IGNITION_LIVE_TOKEN` + `IGNITION_LIVE_MUTATIONS=1` against a commissioned 8.3.x rig and run `cargo test -p ignition-cli --test e2e_webdev -- --ignored`.
**Expected:** All five gates green: deploy/status/scriptExec loop, provider browse/read/write, config export/import round-trip, alarm lifecycle, historian spike.
**Why human:** Requires a real gateway and mutation opt-in. The executor ran all five twice on a real commissioned 8.3.3 rig (05-06 SUMMARY, commit 054df4e) — including discovering and fixing the phase-old byte-0 loader bug — but an independent UAT re-run is the confirmatory check.

### 2. CLI output legibility spot-check

**Test:** Run `ign tags browse`, `ign tags alarms active`, `ign tags history query` against the live rig.
**Expected:** Readable aligned tables and indented trees.
**Why human:** Snapbox goldens pin structure; human legibility is a judgment call.

### Gaps Summary

No gaps. All five success criteria verified against the actual codebase:

- **Criterion 1:** Route bundle (byte-0 contract, embedded, version-pinned) + deploy/status/precondition machinery all wired; actionable `ign webdev deploy` refusal on both absent and version-mismatch states.
- **Criterion 2-4:** The full 17-action surface exists in actions/tags.rs, rides the deployed routes through the generic `webdev_route_call` seam with the shared version precondition, and is live-proven by the executor's five-gate run on a real 8.3.3 gateway.
- **Criterion 5:** Export/import with the locked collision matrix (abort default, `--yes` overwrite) live-proven round-trip. The xml/csv deferral is a documented, pre-cleared format-discretion deviation — JSON is the gateway's native interchange; xml/csv sit in backlog.

Notable strengths: the 05-06 live run functioned as designed verification — it caught and fixed five latent defects (byte-0 loader contract, scriptExec exec-form, export payload shapes, provider-shaped collision pre-check, e2e races), which is exactly what the live gates existed to do. The binding spike's tag↔historian data-flow limitation is honestly documented in the README with a named resolution path (Designer diff) and does not block the structural query capability.

---

_Verified: 2026-08-25T15:30:00Z_
_Verifier: Claude (gsd-verifier)_
