---
phase: 09-agent-surface-api-diagnostics
verified: 2026-09-07T14:05:00Z
status: passed
score: 4/4 success criteria verified (28/28 plan truths verified)
---

# Phase 09: Agent Surface — API, Diagnostics Verification Report

**Phase Goal:** The daily gateway check needs no hand-crafted curl: users get `ign api call` as the escape hatch for anything uncurated, plus curated one-command reads (`license status`, `redundancy status`, `gan status`, diagnostics bundle) for the morning check — live-verified on both rigs.
**Verified:** 2026-09-07T14:05:00Z
**Status:** passed
**Re-verification:** No — initial verification

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

### Human Verification Required

The phase's own verification oracle (both-rig live runs) was executed and recorded, so no gaps require human sign-off. Optional residual items:

### 1. Morning-check ergonomics
**Test:** From a clean shell, run the four curated reads + one `api call` against a freshly provisioned rig and judge whether the envelope/output is genuinely the "no hand-crafted curl" experience.
**Expected:** One command each; envelope data readable; exit codes match README table.
**Why human:** Output readability/feel is not programmatically judgeable; rigs are torn down so this needs a re-provision.

### Gaps Summary

None. All four success criteria verified at artifact, wiring, and live-rig levels. Notable quality signals: the 4xx probe hypothesis (405) was corrected by capture evidence (gateway answers 404) and the e2e gate pins the captured truth rather than the plan's guess — exactly the capture-first discipline the roadmap mandated. State vocabulary ("Generating"/"Valid", PascalCase) and uptime units (ms) were locked from captures with wall-clock cross-checks, not guessed.

---

_Verified: 2026-09-07T14:05:00Z_
_Verifier: Claude (gsd-verifier)_
