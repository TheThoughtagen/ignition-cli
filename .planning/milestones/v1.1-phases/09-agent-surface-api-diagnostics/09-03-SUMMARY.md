---
phase: 09-agent-surface-api-diagnostics
plan: 03
subsystem: api
tags: [api-call, raw-passthrough, raw-value, escape-hatch, exit-partition, clap, wiremock, contract-tests]

# Dependency graph
requires:
  - phase: 09-agent-surface-api-diagnostics
    plan: 01
    provides: CoreError::GatewayClientError (exit-2 slug gateway_client_error, 4 KiB-capped verbatim body) and pub send_and_classify_for_api — the api-call pipeline entry
  - phase: 08-foundations-session-core-config-contract
    provides: Session::resolve construction seam (the ONE client path)
  - phase: 06-tui-cockpit
    provides: routes.rs registry + the CI-enforced clap-tree walk (tui_coverage)
provides:
  - `ign api call --method --path [--data --header --query]` — the EXT-01 raw passthrough escape hatch with envelope-wrapped gateway-verbatim output
  - GatewayApi::api_call trait method (client/apicall.rs): arbitrary verb, user headers, query pairs, GET/DELETE body passthrough, apply_auth + send_and_classify_for_api, RawValue verbatim success body
  - refuse_auth_headers + validate_path pre-resolve guards (auth-pattern header refusal; leading-slash / no-host / no-? path rules)
  - actions::apicall::api_call with action-level guard re-check + ApiCallOutcome envelope model
  - 8 binary contract tests pinning verbatim data, exit partition, truncation, refusal, path/query/body contracts
  - README-documented contract exception (gateway-verbatim data, non-JSON 2xx refusal, 4 KiB cap) + exit-partition note + command row
  - TUI OutOfBand row `api call` with written justification; pinned OutOfBand test extended to [completions, api call]
affects: [09-04, 09-05, 09-06, 14-transports-mcp, exit-code contract consumers, envelope goldens]

# Tech tracking
tech-stack:
  added:
    - serde_json raw_value feature (workspace — RawValue passthrough prerequisite; no new deps)
  patterns:
    - "RawValue passthrough: gateway-verbatim = no field dropped, no value coerced, key order preserved — validated + embedded in ONE from_string call, never parse-re-serialize"
    - "Pre-resolve guard convention: refuse_auth_headers/validate_path run BEFORE Session::resolve so the envelope profile is null and zero requests are constructed (sync-guard precedent)"
    - "Guard re-check layering: CLI guards (pre-resolve, exit contract) + action re-checks (in-process callers cannot skip)"

key-files:
  created:
    - crates/ignition-core/src/client/apicall.rs
    - crates/ignition-core/src/actions/apicall.rs
    - crates/ignition-cli/tests/contract_api.rs
  modified:
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/src/actions/mod.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-cli/tests/tui_coverage.rs
    - README.md
    - Cargo.toml
    - crates/ignition-core/src/actions/{doctor,inspect,version,connections,webdev,projects,logs,sessions,tags,script,rig}.rs (api_call unreachable stubs on the 14 GatewayApi test doubles)

key-decisions:
  - "Gateway-verbatim = RawValue passthrough (research OQ1): byte/order preservation + JSON validation in one call; non-JSON 2xx is the honest exit-1 refusal (README-documented)"
  - "Auth-pattern header refusal runs PRE-resolve (envelope profile null, zero mock hits — binary-pinned); the action re-runs the guards for in-process callers"
  - "Path rules pinned: leading / required, host-shaped URLs (absolute + protocol-relative //) refused naming the profile's gateway, embedded ? refused naming --query (ONE query mechanism)"
  - "User-supplied header names validated at the client (HeaderName/HeaderValue::from_bytes) — reqwest's .header() would PANIC on bad input; exit-2 refusal instead of a crash"
  - "GET/DELETE body passthrough allowed verbatim (curl parity, decision 6); arbitrary well-formed extension verbs accepted (reqwest Method parser is the validator)"
  - "api call maps OutOfBand in routes.rs — raw passthrough is not a cockpit verb, the envelope IS the product (completions genre); pinned test extended in the same task (Pitfall 5)"

patterns-established:
  - "Verbatim-passthrough capability: capture text -> RawValue::from_string — the template any future raw surface must follow"
  - "Pre-resolve usage guards + action re-check: the two-layer guard shape for commands with parse-time refusals"

# Metrics
duration: 175min
completed: 2026-09-07
---

# Phase 9 Plan 03: EXT-01 Raw Passthrough (`ign api call`) Summary

**`ign api call` escape hatch: gateway-verbatim RawValue output through apply_auth + the api-call classifier, auth-pattern header refusal and path/query guards pinned pre-resolve, 8 binary contract tests, README contract exception, and the TUI OutOfBand row**

## Performance

- **Duration:** 175 min
- **Started:** 2026-09-07T05:19:34Z
- **Completed:** 2026-09-07T08:14:52Z
- **Tasks:** 3
- **Files modified:** 20 (3 created + 17 modified)

## Accomplishments

- The EXT-01 escape hatch ships: `ign api call --method --path [--data --header --query]` covers the 80+ uncurated gateway endpoint families, with the envelope's `data.result.data` being the gateway's own JSON **verbatim** (no field dropped, no value coerced, key order preserved — RawValue passthrough, binary-pinned as the FIRST contract test)
- User-supplied auth-pattern headers (`authorization`/`x-ignition-api-token`/`cookie`, case-insensitive, trimmed) are refused pre-I/O with exit 2, `profile: null`, and ZERO gateway requests — contract-proven against a bare mock server
- The exit partition is binary-pinned as a table: 400+JSON → exit 2 `gateway_client_error` with the verbatim body, 401 → 5, 404 → 6, 503 → 6, 500 → 1; oversized bodies truncate at the 4 KiB cap with the `... [truncated]` marker
- The clap walk + pinned OutOfBand set stayed green in the same task as the clap command (Pitfall 5): `api call` maps OutOfBand with a written justification, and `out_of_band_rows_are_pinned` now pins exactly `[completions, api call]`
- README documents the contract exception (§Output contract), the api-call exit partition (§Exit codes), and the command row

## Task Commits

Each task was committed atomically:

1. **Task 1: core — ApiCallRequest/Data models, refusal + path validation, trait method api_call** - `a567da3` (feat)
2. **Task 2: CLI family + dispatch + render + TUI OutOfBand row** - `9b01f3f` (feat)
3. **Task 3: binary contract tests + README contract exception** - `b3be23d` (feat)

## Files Created/Modified

- `crates/ignition-core/src/client/apicall.rs` (NEW) — ApiCallRequest/ApiCallData models, REFUSED_AUTH_HEADERS + refuse_auth_headers, validate_path, 7 unit tests incl. wiremock verbatim/auth/query/GET-body/non-JSON proofs
- `crates/ignition-core/src/actions/apicall.rs` (NEW) — api_call action with guard re-check + ApiCallOutcome; ApiCallRig double + 3 tests
- `crates/ignition-cli/tests/contract_api.rs` (NEW) — 8 binary-level contract tests (verbatim pin, exit partition, truncation, refusal×2, path×3, query, GET body, non-JSON 2xx)
- `crates/ignition-core/src/client/mod.rs` — `pub mod apicall`, GatewayApi::api_call trait method, ReqwestGatewayApi impl (verb validation, header validation, query, body, apply_auth, send_and_classify_for_api, RawValue)
- `crates/ignition-cli/src/cli.rs` — Commands::Api(ApiArgs) → ApiCommand::Call(ApiCallArgs) with --method/--path/--data/--header/--query
- `crates/ignition-cli/src/main.rs` — dispatch arm (parse → pre-resolve guards → Session::resolve → action), build_api_call_request helper, ActionOutput::ApiCall + render_json arm
- `crates/ignition-cli/src/render.rs` — human-mode render_api_call_human (verdict line + pretty body)
- `crates/ignition-tui/src/routes.rs` — `api call` OutOfBand row + justification; OutOfBand prose updated in both doc comments
- `crates/ignition-cli/tests/tui_coverage.rs` — pinned test renamed `out_of_band_rows_are_pinned`, extended to [completions, api call] as an order-free set
- `README.md` — contract exception section, exit-partition note, command table row
- `Cargo.toml` — serde_json `raw_value` feature enabled
- 11 actions/*.rs — api_call `unreachable!` stubs on all 14 GatewayApi test doubles

## Decisions Made

- **RawValue for verbatim** (research OQ1): one `from_string` call both preserves bytes/order AND validates JSON; a non-JSON 2xx body is the honest internal-class refusal naming the download pipelines (README-documented)
- **Two-layer guards**: main.rs runs refuse_auth_headers + validate_path pre-resolve (exit 2, profile null, zero construction — the sync-guard convention); the action re-runs them so in-process callers cannot skip
- **Header validation at the client**: reqwest's `.header()` panics on invalid names/values — user-supplied strings go through `HeaderName/HeaderValue::from_bytes` first, mapping failures to exit-2 (see Deviations)
- **Host-shaped path check ordering**: the URL/host check runs BEFORE the leading-slash check so `http://other/x` gets the precise "carries a host" refusal; protocol-relative `//host/x` is refused explicitly (url::Url::parse alone fails it as RelativeUrlWithoutBase)
- **Extension verbs accepted**: `NOTAVERB` is a VALID HTTP token per reqwest's parser — the CLI accepts arbitrary well-formed verbs (curl parity); only malformed tokens refuse
- **OutOfBand justification written at the row + the pinned test**: raw passthrough is not a cockpit verb; the envelope IS the product (completions genre)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] serde_json lacked the raw_value feature**
- **Found during:** Task 1 (first compile)
- **Issue:** `serde_json::value::RawValue` is feature-gated; the workspace's plain `serde_json = "1.0"` did not enable it, so the verbatim model would not compile
- **Fix:** `serde_json = { version = "1.0", features = ["raw_value"] }` in the workspace Cargo.toml — a feature enable, not a new dependency (plan constraint "no new deps" holds)
- **Files modified:** Cargo.toml
- **Verification:** workspace compiles; contract pin passes
- **Committed in:** a567da3 (Task 1 commit)

**2. [Rule 2 - Missing Critical] Header name/value validation before reqwest's `.header()`**
- **Found during:** Task 1 (impl review)
- **Issue:** reqwest's `RequestBuilder::header` PANICS on invalid names/values; `api call` is the first pipeline carrying raw user header strings (webdev's extra_headers loop only ever carried internal constants) — a bad `--header "Foo Bar: x"` would crash the binary
- **Fix:** validate via `HeaderName::from_bytes` / `HeaderValue::from_str` in the client impl, mapping failures to `CoreError::InvalidInput` (exit 2) — plus empty-name rejection in main.rs's `--header` parser
- **Files modified:** crates/ignition-core/src/client/mod.rs, crates/ignition-cli/src/main.rs
- **Verification:** invalid_method/invalid header refusals unit-tested pre-wire; clippy -D warnings green
- **Committed in:** a567da3 (Task 1 commit)

**3. [Rule 1 - Bug] Path-check ordering + protocol-relative URL gap**
- **Found during:** Task 1 (test run)
- **Issue:** validate_path checked the leading slash FIRST, so `http://other/x` produced the generic slash refusal instead of the specific host diagnosis; additionally `url::Url::parse("//host/x")` fails as RelativeUrlWithoutBase, so the plan's `//host/x` case would slip through a parse-only check
- **Fix:** host-shaped check (explicit `//` prefix OR parse-with-host) runs first; then slash; then `?`. Test matrix updated to assert each specific reason
- **Files modified:** crates/ignition-core/src/client/apicall.rs
- **Verification:** path matrix green; binary path test green
- **Committed in:** a567da3 (Task 1 commit)

**4. [Rule 3 - Blocking] GatewayApi test doubles needed the new trait method**
- **Found during:** Task 1 (compile)
- **Issue:** adding `api_call` to the GatewayApi trait broke compilation of all 14 test doubles (the trait has no default bodies — deliberate: a missed impl must be a compile error, not a runtime panic)
- **Fix:** scripted insertion of the `unreachable!("not part of this action")` stub into all 14 doubles (the established pattern)
- **Files modified:** 11 files under crates/ignition-core/src/actions/
- **Verification:** cargo check -p ignition-core --all-targets green
- **Committed in:** a567da3 (Task 1 commit)

**5. [Rule 1 - Bug] Contract tests hit secret_unavailable (exit 3) instead of the gateway**
- **Found during:** Task 3 (test run)
- **Issue:** `api call` resolves through Session::resolve (as planned) which REQUIRES a credential; the test profile config had none and the spawn helper stripped IGNITION_TOKEN — 6 of 8 tests failed with the correct taxonomy
- **Fix:** spawn helper sets `IGNITION_TOKEN` (the generic EnvStore rung of the locked secret chain); ambient IGNITION_URL/PROFILE/JSON/YES still stripped; the refusal tests prove the guard beats resolution (profile null + zero hits even WITH a token present)
- **Files modified:** crates/ignition-cli/tests/contract_api.rs
- **Verification:** contract_api 8/8 green
- **Committed in:** b3be23d (Task 3 commit)

---

**Total deviations:** 5 auto-fixed (1 blocking dep-feature, 1 missing-critical, 2 bugs, 1 blocking test-double mechanical)
**Impact on plan:** All fixes forced by compile gates, the panic-on-bad-input hazard, or the plan's own test expectations. No scope creep; the shipped contract matches the plan's must-haves exactly.

## Issues Encountered

- Machine load made the workspace suite slow (~40 min wall under nice'd parallel load; 09-01 noted the same); verification ran via background polling — no functional impact. Full workspace green: WS-EXIT:0 across 52 test binaries.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- The EXT-01 happy path + contract surface is complete; 09-06's live gate proves criteria 1+2 against both rigs (read-only paths per Pitfall 7) using the binary surface landed here
- 09-04/09-05 (curated license/redundancy/gan/diagnostics families) reuse the same dispatch shape (pre-resolve guards → Session::resolve → action) and must add their routes.rs rows in the same task as their clap commands
- Blockers: none

---
*Phase: 09-agent-surface-api-diagnostics*
*Completed: 2026-09-07*

## Self-Check: PASSED

- All 10 key files exist on disk (3 created + 7 modified core/CLI/TUI/README)
- All 3 task commits verified in git history: a567da3, 9b01f3f, b3be23d
- Full workspace suite green at execution completion (WS-EXIT:0, 52 test binaries); contract_api 8/8; apicall unit suite 10/10; tui_coverage 3/3; clippy -D warnings + fmt clean
