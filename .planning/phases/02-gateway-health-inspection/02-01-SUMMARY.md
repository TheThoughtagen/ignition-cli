---
phase: 02-gateway-health-inspection
plan: 01
subsystem: api
tags: [reqwest, wiremock, serde, ignition-rest, error-taxonomy, jetty, classifier]

# Dependency graph
requires:
  - phase: 01-foundation-agentic-contracts
    provides: "GatewayApi seam (async_trait, gateway_info), LOCKED exit taxonomy 1-7, envelope contract, snapbox/wiremock harnesses"
provides:
  - "Response classifier (classify.rs): status → content-type → redirect dispatch into the LOCKED taxonomy, pre-.json() on every pipeline call"
  - "Three additive exit-6 slugs: gateway_not_commissioned, gateway_restarting, not_found (+ status-aware 401/403 auth hints)"
  - "Corrected GatewayInfo (ignitionVersion + name/license/redundancyRole/jvmVersion; state/uptime removed)"
  - "ListQuery/ListEnvelope<T>/ListMetadata — the standard 8.3 {items, metadata} envelope"
  - "IgnitionMock wiremock harness (list_json/html_error/redirect/status_json/literal_true)"
  - "#[ignore] live-gateway suite (env-gated skip-by-default) + rig recreation recipe"
  - "get_json/post_empty pipeline helpers with single apply_auth()/Secret::expose() site + Basic demotion warning + redirect(Policy::none())"
affects: [02-02, 02-03, 02-04, 02-05, phase-05-webdev, phase-07]

# Tech tracking
tech-stack:
  added: ["reqwest 'query' feature (.query() is feature-gated in 0.13)"]
  patterns:
    - "Classifier-before-parse: every response passes classify() before .json(); HTML bodies are sniffed (substring, no HTML crate), never parsed"
    - "redirect(Policy::none()) so uncommissioned 302s can never masquerade as 200"
    - "Single auth site: apply_auth() owns the only Secret::expose() calls outside secret.rs"
    - "IgnitionMock shared test module (tests/common/mod.rs) — 3-liner scenarios for later plans"
    - "wiremock gotcha: set_body_string forces text/plain; use set_body_raw(body, mime) when Content-Type matters"

key-files:
  created:
    - crates/ignition-core/src/client/classify.rs
    - crates/ignition-core/src/client/query.rs
    - crates/ignition-core/tests/common/mod.rs
    - crates/ignition-core/tests/live_gateway.rs
  modified:
    - crates/ignition-core/src/error.rs
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/src/client/version.rs
    - crates/ignition-core/src/actions/version.rs
    - crates/ignition-core/tests/gateway_info_contract.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/version_gateway_contract.rs
    - README.md
    - Cargo.toml

key-decisions:
  - "GatewayInfo serializes under the gateway-native camelCase keys (rename=ignitionVersion, passthrough-shaped --json data); alias=version keeps old-shape 8.3.x parsing"
  - "Classifier dispatch order pinned: 2xx → 3xx(/welcome vs other) → 401/403 → 503 → 404 → Internal+sniff; classified variants keep fixed Display, sniffing only enriches Internal"
  - "post_empty extracted now (#[cfg_attr(not(test), expect(dead_code))] until 02-04's restart) with a unit test so clippy stays clean"
  - "Live suite is skip-by-default green no-op; gateway-info requires auth under 8.3 default security (header-less → 401, re-verified live) — the 83-api auth:none tag does not hold"
  - "State/uptime removed from GatewayInfo rather than Optional-kept: the model stays truthful; 02-02 sources them from /overview + /StatusPing"

patterns-established:
  - "classify() is the only status→error mapping site; pipeline helpers (get_json/post_empty) are the only .json() call sites"
  - "IgnitionMock builders register with expect(1) + .mount() (guard-drop unmounting is a footgun); html fixtures via set_body_raw"
  - "Exit-taxonomy additions always touch both homes (error.rs + README) in the same commit, guarded by exit_code_mapping_enumerated"

# Metrics
duration: 19min
completed: 2026-08-21
---

# Phase 2 Plan 1: Gateway client truth-fix + error classification foundation Summary

**Live-shape GatewayInfo fix (ignitionVersion), a status→content-type→redirect classifier with three additive exit-6 slugs, the ListEnvelope/IgnitionMock harnesses, and an opt-in live-gateway suite — the seam every Phase-2 capability rides on.**

## Performance

- **Duration:** 19 min
- **Started:** 2026-08-21T22:46:25Z
- **Completed:** 2026-08-21T23:06:00Z
- **Tasks:** 3
- **Files modified:** 14 (4 created, 10 modified)

## Accomplishments
- Fixed the Phase-1 deserialization bug: real 8.3 gateways return `ignitionVersion`, not `version` — model renamed (serde alias keeps legacy parsing), `name`/`license`/`redundancyRole`/`jvmVersion` added, phantom `state`/`uptime` removed, and the exact live-capture body is a wiremock golden
- Built the response classifier mapping every observed gateway error shape into the LOCKED taxonomy: HTML 401/403 → Auth (exit 5, status-aware hints), 302→/welcome → gateway_not_commissioned, other 3xx → Auth, 503 → gateway_restarting, 404 → not_found, unclassifiable → Internal with the Jetty-sniffed title/message — all pinned by a 7-scenario wiremock matrix
- Added three ADDITIVE exit-6 slugs to both taxonomy homes (error.rs + README) in the same commits, with the enumerated unit test extended
- Locked `redirect(Policy::none())` (the welcome wizard can never masquerade as 200), moved `Secret::expose()` into the single `apply_auth()` helper, and demoted Basic auth with a loud per-call warning (it cannot authenticate 8.3 /data routes)
- Shipped ListQuery/ListEnvelope + the IgnitionMock harness so later plans' scenarios stay 3-liners, and the `#[ignore]` live-gateway suite (skip-by-default) with the rig-recreation recipe

## Task Commits

Each task was committed atomically:

1. **Task 1: Classifier + additive taxonomy + GatewayInfo live-shape fix** - `d4d5a22` (feat)
2. **Task 2: ListQuery/ListEnvelope + IgnitionMock harness + full classifier matrix** - `198c2ee` (feat)
3. **Task 3: Live-gateway #[ignore] suite + Basic demotion docs** - `8ac4e51` (feat)

## Files Created/Modified
- `crates/ignition-core/src/client/classify.rs` - response classifier + Jetty HTML sniffer (+ raw-capture golden tests)
- `crates/ignition-core/src/client/query.rs` - ListQuery / ListEnvelope<T> / ListMetadata (serde-tolerant)
- `crates/ignition-core/src/client/mod.rs` - Policy::none(), apply_auth() single expose() site + Basic warn, get_json/post_empty pipeline
- `crates/ignition-core/src/client/version.rs` - corrected GatewayInfo + LicenseInfo + live-string rows
- `crates/ignition-core/src/error.rs` - 3 new variants, status-aware Auth hints, both homes updated
- `crates/ignition-core/src/actions/version.rs` - field-rename fallout
- `crates/ignition-core/tests/common/mod.rs` - IgnitionMock harness
- `crates/ignition-core/tests/gateway_info_contract.rs` - live-capture golden + 7-scenario classifier matrix
- `crates/ignition-core/tests/live_gateway.rs` - env-gated live suite + rig recipe
- `crates/ignition-cli/src/render.rs` - version human line drops state
- `crates/ignition-cli/tests/version_gateway_contract.rs` - ignitionVersion key assertions
- `README.md` - exit-6 slug row + Phase-2 auth section (token-only, 401/403 table)
- `Cargo.toml` / `Cargo.lock` - reqwest `query` feature

## Decisions Made
- **Gateway-native JSON keys**: GatewayInfo serializes as `ignitionVersion` (camelCase, passthrough-shaped `--json` data) — decided when the plain alias failed to parse the live camelCase payload; `alias = "version"` keeps old-shape gateways working
- **Sniffing scope**: `html_error_parts` enriches ONLY the Internal fallback; classified variants keep fixed Display strings (stable messages beat dynamic ones for agents)
- **post_empty landed now** (restart caller arrives in 02-04) with `#[cfg_attr(not(test), expect(dead_code))]` + a unit test — the Phase-1 CI-learned pattern
- **Live parse test requires the token** — header-less gateway-info 401s under 8.3 default security (re-verified against the still-running research rig during execution)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Enabled reqwest's `query` feature**
- **Found during:** Task 2 (get_json ListQuery param)
- **Issue:** reqwest 0.13 gates `RequestBuilder::query()` behind a non-default `query` feature — compile error on first use
- **Fix:** Added `"query"` to the workspace reqwest features
- **Files modified:** Cargo.toml, Cargo.lock
- **Verification:** workspace builds + tests green
- **Committed in:** 198c2ee (Task 2 commit)

**2. [Rule 1 - Bug] wiremock html_error fixtures carried text/plain, defeating the content-type check**
- **Found during:** Task 2 (classifier matrix 500-HTML scenario)
- **Issue:** wiremock's `set_body_string` forces mime text/plain and its mime field overrides inserted Content-Type headers regardless of order
- **Fix:** Switched the harness to `set_body_raw(body, "text/html;charset=iso-8859-1")`
- **Files modified:** crates/ignition-core/tests/common/mod.rs
- **Verification:** internal_500_html_sniffs_detail passes (sniffer engages)
- **Committed in:** 198c2ee (Task 2 commit)

**3. [Rule 1 - Bug] Scoped-mock guards unmounted fixtures before any request**
- **Found during:** Task 2 (matrix tests: "Number of matched incoming requests: 0")
- **Issue:** IgnitionMock builders returned `MockGuard`s that tests dropped immediately — guard drop unmounts the mock
- **Fix:** Builders now use `.mount()` with `expect(1)` (verification happens on server drop)
- **Files modified:** crates/ignition-core/tests/common/mod.rs
- **Verification:** all matrix scenarios match and pass
- **Committed in:** 198c2ee (Task 2 commit)

**4. [Rule 1 - Bug] Stale "auth: none" claim for gateway-info**
- **Found during:** Task 3 (live verification against the still-running research rig)
- **Issue:** Header-less gateway-info answers 401 on a commissioned 8.3.6 gateway — the 83-api collection's tag (repeated in Phase-1 comments and research) does not hold under default security
- **Fix:** Corrected the mod.rs module comment, made live_gateway_info_parses require the token, and pinned the RAW re-captured 401 page (with its inter-row newlines) as a second sniffer golden
- **Files modified:** crates/ignition-core/src/client/mod.rs, crates/ignition-core/tests/live_gateway.rs, crates/ignition-core/src/client/classify.rs
- **Verification:** sniffer test passes against the raw wire capture; live suite skips cleanly with no envs
- **Committed in:** 8ac4e51 (Task 3 commit)

---

**Total deviations:** 4 auto-fixed (2 blocking/feature, 2 bug)
**Impact on plan:** All fixes were correctness necessities inside planned scope. No scope creep.

## Authentication Gates

None — no live credentials were required for execution (wiremock covers the contract). The research rig (`ign-research`, port 18088) was still running during execution and provided one bonus live verification (the header-less 401 + raw Jetty capture), but the research session's API token was a secret that was never persisted, so the two token-bearing live tests remain skip-path until a user supplies `IGNITION_LIVE_TOKEN` (see [02-USER-SETUP.md](./02-USER-SETUP.md)).

## Issues Encountered
- serde alias alone cannot parse the gateway's camelCase `ignitionVersion` key — resolved by `rename = "ignitionVersion"` + `alias = "version"` (decision documented above)
- `Internal` carries no endpoint field by design; the matrix's shared endpoint assertion now excludes Internal (its URL lives in the message text)

## User Setup Required

**Opt-in live-gateway verification requires manual env/rig setup (NOT required for CI).** See [02-USER-SETUP.md](./02-USER-SETUP.md) for:
- `IGNITION_LIVE_URL` / `IGNITION_LIVE_TOKEN` env vars
- Docker rig + API-token creation recipe (with the "Require secure connections" trap)
- Verification commands

## Next Phase Readiness
- Classifier, ListEnvelope, IgnitionMock, and the live suite are in place — 02-02 (status/overview) grows `GatewayApi` with `overview()` + `status_ping()` (auth=false pipeline ready) and reuses the harness directly
- **STATE.md's flagged "Phase 2 gap: live-gateway auth verification" now has an executable closure path** (live_token_auth_works); the research session itself already verified the claims empirically, and the remaining step is a user-supplied token run — mark CLOSED with that caveat
- The research rig is still up (`ign-research`, port 18088, trial mode) if the user wants to run the live suite immediately after creating a fresh token

## Self-Check: PASSED
