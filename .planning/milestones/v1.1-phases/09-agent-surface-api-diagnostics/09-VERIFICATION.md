---
phase: 09-agent-surface-api-diagnostics
verified: 2026-09-08T00:15:00Z
status: passed
score: 4/4 success criteria verified (28/28 plan truths verified) + 2/2 UAT gaps closed
re_verification:
  previous_status: passed
  previous_score: 4/4
  verified_at: 2026-09-07T14:05:00Z
  uat_gaps_found: 2 (plus 1 minor deferred to Phase 12)
  gaps_closed:
    - "UAT test 8 (Gap 3): bundle wait Invalid-state honesty — immediate exit-6 bundle_not_available + honest deadline (09-07)"
    - "UAT test 10 (Gap 1): seven Phase 9 verbs reachable from the TUI Dashboard actions menu + routes↔menu parity CI (09-08)"
  gaps_remaining:
    - "UAT Gap 2 (tab-indicator visual ambiguity) — deliberately deferred to Phase 12 theming (TUIX-03/04), logic-verified correct, purely visual"
  regressions: []
---

# Phase 09: Agent Surface — API, Diagnostics Verification Report

**Phase Goal:** The daily gateway check needs no hand-crafted curl: users get `ign api call` as the escape hatch for anything uncurated, plus curated one-command reads (`license status`, `redundancy status`, `gan status`, diagnostics bundle) for the morning check — live-verified on both rigs.
**Verified:** 2026-09-08T00:15:00Z (initial: 2026-09-07T14:05:00Z; re-verified after UAT gap closure)
**Status:** passed
**Re-verification:** Yes — after UAT gap closure (plans 09-07 and 09-08)

**Method note:** gsd-tools frontmatter parser could not read the nested `must_haves` YAML in these plans ("No must_haves.artifacts found" despite them being present), so verification was performed manually per the fallback paths in the verification process — all greps, file checks, and test runs below were executed directly.

## Goal Achievement

### Success Criteria (observable truths)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `ign api call --method --path` reaches any gateway endpoint with body/header control; output is envelope-wrapped with gateway-verbatim `data`; contract exception documented | ✓ VERIFIED | `client/apicall.rs` (RawValue passthrough, `refuse_auth_headers`, `validate_path`); trait impl in `client/mod.rs:1428–1452` (`apply_auth` → `send_and_classify_for_api` → `RawValue::from_string`); `cli.rs` `ApiCallArgs` with `--method/--path/--data/--header/--query`; README:224 documents verbatim data, 4 KiB cap, exit partition; `contract_api.rs` 8/8 green (verbatim pin, truncation marker, refusal matrix, path validation) |
| 2 | Unclassified 4xx from `api call` → exit-2 class with verbatim body, never exit-1 internal storm; auth-pattern headers refused | ✓ VERIFIED | `classify.rs:199` `if api_call && status.is_client_error()` → `CoreError::GatewayClientError` with `truncate_api_body`; `error.rs:489/538/549` variant + slug `gateway_client_error` + exit 2; parameter-scoped (curated non-leak test passes); refusal runs pre-I/O in `actions/apicall.rs:35`; `api_classify_contract.rs` 5/5 green; `readme_exit_table_agreement` green |
| 3 | `license status`, `redundancy status`, `gan status` each one command to live truth | ✓ VERIFIED | `license.rs` (LicenseStatusWire, `#[serde(flatten)]` + `#[serde(default)]` throughout), `redundancy.rs` (RedundancyStatusWire, units derived from captures with wall-clock cross-check in LIVE-CAPTURES §3), `gan.rs` (GanStatusWire); routes.rs Dashboard rows lines 76/80/84; capture-backed fixtures in each file's tests; live gate 1–3 runs: **6/6 PASS on both rigs** (RIG-NOTES gate matrix) |
| 4 | Diagnostics bundle generate/download/wait as curated commands, live-verified on BOTH rigs (8.3.3 + 8.3.6) | ✓ VERIFIED | `client/diagnostics.rs`: `BUNDLE_GENERATING_STATES = ["Generating"]` citing 09-LIVE-CAPTURES §5, `BUNDLE_DOWNLOAD_TIMEOUT = 300s` unit-pinned, download rides `download_to_file`; `actions/diagnostics.rs`: `bundle_generate/bundle_status/bundle_wait (PollConfig)/bundle_download`; routes.rs 4 rows (93/97/101/105); `contract_diagnostics.rs` 9/9 green (generate envelope, wait flip, download bytes+magic, deadline convention); live: bundle round-trip **PASS on both rigs** under `IGNITION_LIVE_MUTATIONS=1` with ZIP magic (`50 4b 03 04`) and `fileSize` == downloaded bytes asserted live |

**Score:** 4/4 success criteria verified

### Required Artifacts

All 17 artifacts across 6 plans verified at all three levels (exists / substantive / wired):

| Artifact | Expected | Status | Details |
| -------- | -------- | ------ | ------- |
| `crates/ignition-core/src/error.rs` | GatewayClientError variant + slug/exit/hint arms + literal table + doc taxonomy | ✓ VERIFIED | 1627 lines; variant at 489, slug 538, exit-2 549, hint 803; `readme_exit_table_agreement` test passes |
| `crates/ignition-core/src/client/classify.rs` | api_call-scoped 4xx catch-all | ✓ VERIFIED | arm at 199, parameter-scoped, wired into GatewayClientError construction |
| `crates/ignition-core/src/client/mod.rs` | `send_and_classify_for_api` | ✓ VERIFIED | defined 759, called from api_call impl at 1448 |
| `crates/ignition-core/tests/api_classify_contract.rs` | wiremock exit-partition + non-leak tests (≥60 lines) | ✓ VERIFIED | 264 lines; **5/5 pass** (fresh run) |
| `README.md` | exit-2 row + gateway_client_error | ✓ VERIFIED | lines 57, 76, 224 |
| `09-LIVE-CAPTURES.md` | rig-labeled captures + capture-locked decisions | ✓ VERIFIED | both rigs, all endpoints, state vocabulary + units decided from captures (uptime=ms cross-checked against wall clock), 4xx probes (404/HTML vs 404/empty — honest hypothesis correction: DELETE answered 404 not 405, capture wins) |
| `09-RIG-NOTES.md` | ops log, ports, token names, PASS matrix, teardown | ✓ VERIFIED | ports 18188/19188, full gate matrix "ALL PASS on BOTH rigs", teardown record at line 66 |
| `crates/ignition-core/src/client/apicall.rs` | refuse_auth_headers + validate_path + ApiCallRequest | ✓ VERIFIED | 371 lines; RawValue data; refusal/validators unit-tested in-file |
| `crates/ignition-core/src/actions/apicall.rs` | refusal + path validation BEFORE I/O | ✓ VERIFIED | 433 lines; checks at 35–36 precede session/network |
| `crates/ignition-cli/src/cli.rs` | Commands::Api → ApiCallArgs | ✓ VERIFIED | 1548 lines; ApiCallArgs at 1173 |
| `crates/ignition-cli/tests/contract_api.rs` | binary contract tests (≥80 lines) | ✓ VERIFIED | 432 lines; **8/8 pass** (fresh run) |
| `crates/ignition-tui/src/routes.rs` | `api call` OutOfBand row + 3 read rows + 4 bundle rows | ✓ VERIFIED | api call row 66, license/redundancy/gan 76/80/84, bundle 93/97/101/105 |
| `crates/ignition-core/src/client/license.rs` | LicenseStatusWire partial-curated + flatten | ✓ VERIFIED | 269 lines; flatten at 84; capture fixtures at 173–265 |
| `crates/ignition-core/src/client/redundancy.rs` | RedundancyStatusWire flat, units from captures | ✓ VERIFIED | 204 lines; units doc references `last_sync_epoch_ms` derivation from captures |
| `crates/ignition-core/src/client/gan.rs` | GanStatusWire 5 fields | ✓ VERIFIED | 102 lines; zero-connection parse test present |
| `crates/ignition-cli/tests/contract_diagnostics.rs` | reads + bundle contract tests | ✓ VERIFIED | 659 lines; **9/9 pass** (fresh run) |
| `crates/ignition-cli/tests/e2e_api_diagnostics.rs` | env-gated live gates (≥80 lines) | ✓ VERIFIED | 563 lines; 6 gates; read-only by construction; mutations-gated round-trip; every assertion cites LIVE-CAPTURES |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | -- | --- | ------ | ------- |
| `classify.rs` | `error.rs` | fallback constructs GatewayClientError when `api_call && is_client_error` | ✓ WIRED | line 199–207 |
| `error.rs` | `README.md` | readme_exit_table_agreement cross-check | ✓ WIRED | test at error.rs:1282, **passes** |
| `client/apicall.rs` | `client/mod.rs` | api_call rides apply_auth + send_and_classify_for_api | ✓ WIRED | impl in mod.rs:1447–1448 |
| `main.rs` | `session.rs` | Api dispatch resolves through Session::resolve | ✓ WIRED | Commands::Api arm at main.rs:2025 |
| `routes.rs` / `tui_coverage.rs` | OutOfBand pin extended to [completions, api call] | pinned set + justification | ✓ WIRED | tui_coverage.rs:35,122–133; **3/3 pass** (fresh run) |
| LIVE-CAPTURES | license/redundancy/gan/diagnostics models | captures as parse fixtures + state vocabulary | ✓ WIRED | fixtures in each model's tests; BUNDLE_GENERATING_STATES cites §5 Decisions 1 |
| `actions/diagnostics.rs` | `poll.rs` | bundle_wait rides PollConfig/PollState, Network{source:None} deadline | ✓ WIRED | PollConfig import + use at actions/diagnostics.rs:39,87 |
| `client/diagnostics.rs` | `download_to_file` | streaming download with 300 s timeout override | ✓ WIRED | BUNDLE_DOWNLOAD_TIMEOUT unit-pinned; live download byte-verified on both rigs |
| e2e_api_diagnostics | LIVE-CAPTURES | live assertions cite captures | ✓ WIRED | header doc + per-gate citations (§1–§6) |
| RIG-NOTES | 04-VERIFICATION recipe | Phase-4 headless provisioning | ✓ WIRED | provisioning recipe cited verbatim at RIG-NOTES:56 |

### Requirements Coverage

| Requirement | Status | Blocking Issue |
| ----------- | ------ | -------------- |
| EXT-01 (api call escape hatch) | ✓ SATISFIED | — |
| EXT-02 (curated diagnostics/reads, live-verified) | ✓ SATISFIED | — |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| `crates/ignition-cli/src/cli.rs` | 815 | "unimplemented!() stubs" text | ℹ️ Info | False positive — comment asserting the *absence* of stubs; no actual TODO/FIXME/placeholder/unimplemented in any phase file |

### Test Evidence (fresh runs this verification)

- `ignition-core` exit-table agreement: 1/1 ok
- `ignition-core api_classify_contract` (wiremock): 5/5 ok (28.9s)
- `ignition-cli tui_coverage` (clap walk + OutOfBand pin): 3/3 ok
- `ignition-cli contract_api`: 8/8 ok
- `ignition-cli contract_diagnostics`: 9/9 ok
- Live both-rig runs (recorded, 2026-09-07): read-only gates **6/6 PASS on 8.3.6** (1.85s) and **6/6 PASS on 8.3.3** (2.05s); bundle round-trip **PASS on both** (3.82s / 4.00s) with ZIP magic + `fileSize` byte-equality asserted live. Rigs torn down (RIG-NOTES:66–72).

## Gap-Closure Verification (re-verification, 2026-09-08)

Two UAT gaps (09-UAT.md tests 8 and 10) were closed by plans 09-07 and 09-08, executed in parallel waves. All gap-closure must-haves verified against the actual codebase (SUMMARY claims cross-checked, not trusted).

### Gap 1 — UAT test 8 (09-07): bundle wait Invalid-state honesty

UAT-observed failure: wait polled a gateway-reported `Invalid` state to the 300 s deadline, then reported "gateway unreachable" — mislabeling an answered gateway as a network failure.

| Must-have | Status | Evidence |
| --------- | ------ | -------- |
| `Invalid` capture-encoded as TERMINAL steady state with 2026-09-07 UAT provenance | ✓ VERIFIED | `client/diagnostics.rs:71` in `BUNDLE_CAPTURED_STATES` with the ign-p9-836/TTL-probe provenance comment (67–70); `BUNDLE_UNAVAILABLE_STATES` (80–84) + `is_bundle_unavailable()` (97–99); falsified "exactly two states" docs rewritten; subset-by-construction unit test at 283–286 |
| `BundleNotAvailable` variant + `bundle_not_available` slug + exit 6, both pinned tables updated (Three-Place) | ✓ VERIFIED | `error.rs`: variant 533, slug 579, exit-6 arm 620, hint 860, enumerated case 1222–1226, literal `(6, "bundle_not_available")` table row 1288, exit-6 doc-taxonomy row 15; README exit-6 row includes it (README:61); `readme_exit_table_agreement` green in fresh run |
| Immediate-exit wait: unavailable probe arm ordered BEFORE captured-Done | ✓ VERIFIED | `actions/diagnostics.rs:113–131` — `is_generating`→Pending, `is_bundle_unavailable`→`Err(BundleNotAvailable)` immediate (117–122, ordering comment present), captured→Done, unknown→Pending; WaitRig test `invalid_state_exits_immediately_bundle_not_available` (620–646) pins **exactly 2 probes**, exit 6, slug, message names `Invalid` + `generate`, hint names the command verbatim |
| Honest deadline: `observation: Option<String>` on `CoreError::Network`; never "unreachable" when the gateway answered | ✓ VERIFIED | `error.rs:151–171` — thiserror named-arg lead switch (`Some`⇒"no terminal state", `None`⇒"gateway unreachable") + `; last observation: {obs}` note; `poll.rs:139` populates it; poll.rs tests cover BOTH branches (`deadline_expiry_is_network_class_with_observation` asserts "unreachable" ABSENT with observation; `deadline_without_observation_still_says_unreachable` preserves the no-observation wording); unknown-states-keep-polling-to-deadline exit-4 semantics preserved (test green) |
| Binary contract tests + README rows | ✓ VERIFIED | `contract_diagnostics.rs:557+` `bundle_wait_invalid_exits_immediately` (wiremock: `Invalid` ⇒ exit 6, slug, 1 status hit) + deadline contract upgraded for honest wording; README wait row (231) rewritten with terminal-Invalid/immediate-exit-6/honest-deadline semantics; generate row (228) vocabulary claim corrected (the falsified "EXACTLY two states" — 09-07 SUMMARY deviation 2) |

**Commits:** `f12de36` (T1), `4df872d` (T2, atomic Three-Place incl. README exit row), `1d9cc58` (T3) — all in history. Happy path unchanged: `bundle_wait_flips_to_terminal` (Generating→Valid exit 0) green.

### Gap 2 — UAT test 10 (09-08): TUI Dashboard actions menu + routes↔menu parity

UAT-observed failure: the Dashboard actions menu showed only the 15 v1.0 verbs — routes.rs rows existed but the hardcoded `ACTIONS: [&str; 15]` and `execute_menu_action` were never extended (registry rows without a reachable surface).

| Must-have | Status | Evidence |
| --------- | ------ | -------- |
| `ACTIONS [&str; 22]` with the 7 clap-exact verbs + 09 provenance comments | ✓ VERIFIED | `state.rs:411` const declared `22`; verbs at 434–442 (license/redundancy/gan status, bundle generate/status/download/wait); locked-list unit tests extended (1373–1380) and green |
| Seven `execute_menu_action` arms dispatching the SAME ignition-core actions the CLI uses | ✓ VERIFIED | `update.rs:2549–2612` — all 7 `Some(...)` arms, each spawning `ignition_core::actions::{license,redundancy,gan,diagnostics}::*` (no second construction); wait arm mirrors clap defaults (`DEFAULT_INTERVAL` + 300 s literal, explicitly NOT `BUNDLE_DOWNLOAD_TIMEOUT`); download rides `None` (.part-rename naming, documented) |
| `menu_label(path)` single alias seam | ✓ VERIFIED | `routes.rs:506` with doc comment pinning the wait-trio-only rule and the bidirectional CI contract |
| Bidirectional routes↔menu parity CI test | ✓ VERIFIED | `tui_coverage.rs:208` `dashboard_actions_menu_matches_registry`: `MENU_HOSTED[22]` (214–243), pinned 31-row Dashboard count (269–275), (b) MENU_HOSTED⊆Dashboard rows, (c)+(d) both directions through the `menu_label` seam with exactly-one cardinality, (e) `ACTIONS.len()==MENU_HOSTED.len()` (330–331). Negative proofs demonstrated live then reverted per 09-08 SUMMARY (lint drop fails (c); fake route fails pinned count 32≠31) |

**Commits:** `87d8672` (T1), `f98812b` (T2) — both in history. Deviations documented and sound: dead `lint` verb fixed (was unreachable since 07-04), Actions-modal fit test re-framed 80x24→80x30 (22 entries physically cannot fit 24 rows; test-module-only change).

### Gap-closure test evidence (fresh runs, 2026-09-08)

- `cargo test -p ignition-core --lib -- diagnostics exit_table`: **15/15 ok** — includes `readme_exit_table_agreement`, `invalid_is_a_captured_terminal_unavailable_state`, `invalid_state_exits_immediately_bundle_not_available`, `unknown_states_keep_polling_until_the_deadline`
- `cargo test -p ignition-cli --test contract_diagnostics`: **10/10 ok** (was 9/9 — the new Invalid binary pin brought it to 10); includes `bundle_wait_invalid_exits_immediately` and the honest-deadline `bundle_wait_deadline_is_network`
- `cargo test -p ignition-cli --test tui_coverage`: **4/4 ok** (was 3/3 — the new parity test added)

### Prior success criteria — regression spot-check

All four 2026-09-07 success criteria hold structurally (spot-check, not full re-verify; gap plans touched only diagnostics wait + the TUI menu surface):

- SC1/SC2 (api call + exit-2 4xx partition): `classify.rs:199` catch-all intact; `apicall.rs` `refuse_auth_headers`/`validate_path` intact; `contract_api.rs` 8/8 was green 2026-09-07 and its files untouched by 09-07/09-08
- SC3 (curated reads): routes.rs rows unchanged at 76/80/84; models untouched by gap plans
- SC4 (bundle family): routes.rs rows unchanged at 93/97/101/105; generate/status/download paths untouched — only wait semantics changed, and its happy path is re-proven green (`bundle_wait_flips_to_terminal`)

### Remaining gap (not this phase's closure scope)

UAT Gap 2 (tab-indicator visual ambiguity) is deliberately deferred to Phase 12 theming per the UAT diagnosis: the logic is correct (`active_tab_is_bolded` pins bold==state.screen; the 09 commits touched routes.rs rows only), the failure is purely visual (bold-only indication + parked cursor). Recorded here for the Phase 12 plan's intake, not as a Phase 9 failure.

### Human Verification Required

**Re-verification note (2026-09-08):** UAT test 8's LIVE re-test — Invalid-state decay behavior on a real rig (`ign diagnostics bundle wait` against a gateway whose bundle has decayed to Invalid, confirming exit 6 in seconds with the honest message) — still requires `/gsd-verify-work`. Automated checks confirm the code structure and test pins (WaitRig exactly-2-probes, wiremock 1-hit binary pin, honest-deadline wording), NOT live rig behavior; both rigs were torn down at RIG-NOTES:66–72.

The phase's own verification oracle (both-rig live runs) was executed and recorded, so no other gaps require human sign-off. Optional residual items:

### 1. Morning-check ergonomics
**Test:** From a clean shell, run the four curated reads + one `api call` against a freshly provisioned rig and judge whether the envelope/output is genuinely the "no hand-crafted curl" experience.
**Expected:** One command each; envelope data readable; exit codes match README table.
**Why human:** Output readability/feel is not programmatically judgeable; rigs are torn down so this needs a re-provision.

### Gaps Summary

Initial verification (2026-09-07): none — all four success criteria verified at artifact, wiring, and live-rig levels. Notable quality signals: the 4xx probe hypothesis (405) was corrected by capture evidence (gateway answers 404) and the e2e gate pins the captured truth rather than the plan's guess — exactly the capture-first discipline the roadmap mandated. State vocabulary ("Generating"/"Valid", PascalCase) and uptime units (ms) were locked from captures with wall-clock cross-checks, not guessed.

Re-verification (2026-09-08): none — both UAT gaps closed with verified must-haves (see Gap-Closure Verification above). The Invalid-state encoding is itself a capture-first success story: the UAT's TTL probe discovered a steady state the original 09-02 captures never showed, and the fix encodes the observed truth (terminal-unavailable) rather than the plan-era guess (unenumerated failure states). One honest-unknowns behavior was explicitly preserved: genuinely never-observed states still poll to the deadline with exit 4, so the vocabulary cannot silently declare an unknown state terminal. Live-rig re-test of UAT test 8 remains open for `/gsd-verify-work` (rigs torn down); UAT Gap 2 is deferred to Phase 12 by design.

---

_Verified: 2026-09-08T00:15:00Z (initial 2026-09-07T14:05:00Z; re-verified after UAT gap closure)_
_Verifier: Claude (gsd-verifier)_
