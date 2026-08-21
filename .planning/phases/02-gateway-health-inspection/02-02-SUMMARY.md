---
phase: 02-gateway-health-inspection
plan: 02
subsystem: api
tags: [reqwest, wiremock, serde, ignition-rest, status, metrics, modules, statusping, snapbox]

# Dependency graph
requires:
  - phase: 02-gateway-health-inspection (02-01)
    provides: "classifier + get_json pipeline (auth=false seam ready), ListQuery/ListEnvelope, IgnitionMock harness, live suite skeleton, exit taxonomy (used exclusively, no additions)"
provides:
  - "Six new GatewayApi capabilities: overview(), status_ping() (header-less, wiremock-proven), modules(quarantined, query), metrics_current(), metrics_historic(), metrics_threads()"
  - "Overview/StatusPing/ModuleInfo + CurrentGauges/PerformanceCharts/ThreadCounts models with flatten passthrough and honest unit naming (uptime ms, cpu fraction vs gauges percent, trialRemaining s)"
  - "actions::inspect::{status,modules,metrics} — serde-model-out actions the Phase-6 TUI rides; status merges gateway_info + overview + status_ping into the documented data shape"
  - "`ign status` / `ign modules [--quarantined]` / `ign metrics [--history]` — three render modes, snapbox goldens, human banners incl. the trial-countdown line"
  - "The credential-REQUIRED dispatch pattern for authed reads (resolve_secret, exit 3 without a secret) + the shared secret_chain() extraction"
  - "Live-suite additions: unauthenticated StatusPing check (URL only) + the three authed inspection reads"
affects: [02-03, 02-04, 02-05, phase-06-tui]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Wire-faithful client models vs honest-keyed action models: status.rs/metrics.rs parse+serialize the gateway-native shapes; inspect.rs re-exposes selected fields under unit-explicit keys (uptime_ms, cpu_fraction, trial_remaining_s)"
    - "Nested-wire → flat-model serde: PerformanceCharts custom Deserialize walks memoryChartDatapoints; Serialize emits flat gateway-native series names"
    - "Header-absence proof: a credential-bearing client + recorded-request inspection proves auth=false fetches (the /StatusPing readiness anchor)"
    - "Error envelopes with dynamic endpoints use programmatic assertions (version_gateway_contract pattern) — SNAPSHOTS=overwrite would bake the random mock port into a golden"
    - "Empty human lists get a '(no … modules)' line; JSON stays {items: [], quarantined}"

key-files:
  created:
    - crates/ignition-core/src/client/status.rs
    - crates/ignition-core/src/client/metrics.rs
    - crates/ignition-core/src/actions/inspect.rs
    - crates/ignition-core/tests/status_contract.rs
    - crates/ignition-cli/tests/contract_status.rs
  modified:
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/src/actions/version.rs
    - crates/ignition-core/src/client/version.rs
    - crates/ignition-core/src/actions/mod.rs
    - crates/ignition-core/tests/live_gateway.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - README.md

key-decisions:
  - "Action-layer data keys are unit-explicit (uptime_ms, cpu_fraction, trial_remaining_s) while client models stay wire-faithful — honest naming instead of silent conversion; pinned string-level in the inspect unit test"
  - "status is a read of a HEALTHY gateway: a failed sub-call is an error (exit per taxonomy), never a degraded payload"
  - "Inspection commands REQUIRE a credential (resolve_secret → SecretUnavailable exit 3) — the inverse of version's header-less degradation; the LOCKED secret chain now builds in exactly one place (secret_chain())"
  - "ModuleInfo Option-fields for state/licenseState/vendorName/startupTime (openapi: fully-loaded-modules-only; quarantined items carry a reduced shape) and startup_time is a String on the wire"
  - "PerformanceCharts deserializes the nested memoryChartDatapoints wire shape into a flat model, serializing flat under gateway-native series names"

patterns-established:
  - "Growing GatewayApi: per-capability files own models + pub(crate) verified path consts; trait methods + one-line impl bodies live in the single impl block in mod.rs (E0119 forbids split impls)"
  - "Every new test double that implements GatewayApi stubs the not-served capabilities with unimplemented!() (version.rs pattern)"
  - "Binary goldens: fixture bodies pin every number, so goldens are exact; only genuinely dynamic values (mock URIs) avoid goldens entirely via programmatic envelope assertions"

# Metrics
duration: 12min
completed: 2026-08-21
---

# Phase 2 Plan 2: status / modules / metrics inspection surface Summary

**Six GatewayApi capabilities (overview, header-less StatusPing, modules, three systemPerformance metrics) feeding `ign status` / `ign modules --quarantined` / `ign metrics --history` with three-mode goldens — the webpage status pages replaced, no new error variants.**

## Performance

- **Duration:** 12 min
- **Started:** 2026-08-21T23:13:34Z
- **Completed:** 2026-08-21T23:25:54Z
- **Tasks:** 3
- **Files modified:** 14 (5 created, 9 modified)

## Accomplishments
- HLTH-01/02/07 shipped: three commands cover the gateway webpage's Status Overview, Config > Modules, and Performance pages — including the research-recommended license/trial-countdown banner (`license: trial, 1h 56m remaining`)
- The `/StatusPing` readiness anchor is header-less BY WIREMOCK PROOF: a credential-bearing client's recorded ping request carries no auth header at all — it will keep answering when auth is broken or mid-restart (the primitive 02-05's wait loops poll)
- All endpoints are the live-verified paths (`/data/api/v1/overview`, `/modules/{healthy,quarantined}`, `/systemPerformance/{currentGauges,charts,threads}`) with exact-capture wiremock fixtures — the invented `/system/metrics` path appears nowhere
- `ign status --json` emits exactly the documented data keys (`gateway {name, ignition_version, edition, license}`, `state`, `overview {java, os, uptime_ms, memory, cpu_fraction, disk, license {state, trial_remaining_s}}`) — pinned string-level in unit tests and by binary key assertions
- Inspection dispatch establishes the credential-REQUIRED pattern (exit 3 without a secret) and the shared `secret_chain()` helper; live suite gains an unauthenticated StatusPing check plus the three authed reads

## Task Commits

Each task was committed atomically:

1. **Task 1: overview / status_ping / modules capabilities** - `f22b186` (feat)
2. **Task 2: systemPerformance metrics capabilities** - `5a67795` (feat)
3. **Task 3: ign status/modules/metrics commands + goldens** - `1f646b3` (feat)

## Files Created/Modified
- `crates/ignition-core/src/client/status.rs` - Overview/StatusPing/ModuleInfo (+ redundancy/java/os/disk/license sub-models) with flatten passthrough; verified path consts
- `crates/ignition-core/src/client/metrics.rs` - CurrentGauges/ThreadCounts/Datapoint/PerformanceCharts (nested-wire custom Deserialize); path consts incl. the invented-path warning
- `crates/ignition-core/src/client/mod.rs` - trait +six methods; impl bodies (overview authed, status_ping auth=false, modules path-select + query)
- `crates/ignition-core/src/actions/inspect.rs` - status/modules/metrics actions + StatusResult documented-shape models + FakeApi unit tests
- `crates/ignition-core/src/actions/version.rs` - test double stubs for the grown trait
- `crates/ignition-core/src/client/version.rs` - LicenseInfo expirationDate rename fix (Rule 1)
- `crates/ignition-core/tests/status_contract.rs` - 9 wiremock scenarios incl. the header-absence proof and limit=-1 matcher proof
- `crates/ignition-core/tests/live_gateway.rs` - +2 opt-in live checks (now 5)
- `crates/ignition-cli/src/cli.rs` - Status / Modules{--quarantined} / Metrics{--history}
- `crates/ignition-cli/src/main.rs` - ActionOutput ×3, run_inspection + resolve_gateway_api (credential REQUIRED), secret_chain() extraction
- `crates/ignition-cli/src/render.rs` - human banners/rows + humanize_duration_ms/human_bytes
- `crates/ignition-cli/tests/contract_status.rs` - 6 golden/contract tests across all render modes
- `README.md` - Commands table + inspection-trio framing

## Decisions Made
- **Two-layer naming**: client models stay wire-faithful (gateway-native camelCase renames, round-trip passthrough); the status action re-exposes selected fields under unit-explicit keys — `uptime_ms`, `cpu_fraction`, `trial_remaining_s` — so agents never guess units. Overview's 0–1 fraction vs gauges' percent is documented at BOTH fields and never converted
- **status = healthy-gateway read**: no degradation semantics (a failed sub-call exits per taxonomy); contrast is deliberate with version's unreachable→warning
- **ModuleInfo shape from the openapi, not the research prose**: quarantined items carry a reduced shape, so state/licenseState/vendorName/startupTime are Option and `startup_time` is a String (the wire type) — the `--quarantined` list parses
- **Error-envelope tests with dynamic endpoints stay programmatic** (mock URI): SNAPSHOTS=overwrite rewrites `[..]` elisions with literal values, which would bake the random port into a golden — the exact-value goldens are safe because every number in them comes from a pinned fixture body

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Trait impl bodies centralized in mod.rs instead of status.rs/metrics.rs**
- **Found during:** Task 1 (trait growth)
- **Issue:** The plan placed overview/status_ping/modules impl bodies in status.rs and metrics impls in metrics.rs; Rust (E0119) rejects a second `impl GatewayApi for ReqwestGatewayApi` block — one trait impl per type, no splitting across files
- **Fix:** Per-capability files own the models + `pub(crate)` verified path constants; the single trait impl block in mod.rs holds all bodies as 1–3 line get_json delegations (the plan's key_links pattern — "impl bodies delegate to the 02-01 get_json pipeline" — holds exactly)
- **Files modified:** crates/ignition-core/src/client/mod.rs, status.rs, metrics.rs
- **Verification:** workspace builds, all contract tests green
- **Committed in:** f22b186 (Task 1 commit)

**2. [Rule 1 - Bug] LicenseInfo.expiration_date silently dropped the gateway's `expirationDate`**
- **Found during:** Task 3 (binary golden generation)
- **Issue:** 02-01's camelCase fix covered `ignitionVersion` only — `expiration_date` had no serde rename, so the gateway's `expirationDate` key was ignored on parse and the field serialized as None (visible in the first compact-golden run: `"license":{"mode":"Trial"}`)
- **Fix:** `#[serde(rename = "expirationDate", alias = "expiration_date")]` — gateway-native serialization like `ignitionVersion`, snake_case alias for tolerance; goldens regenerated
- **Files modified:** crates/ignition-core/src/client/version.rs (+ goldens in contract_status.rs / inspect.rs expectations)
- **Verification:** compact status golden now carries expirationDate; full workspace green
- **Committed in:** 1f646b3 (Task 3 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking/language constraint, 1 bug)
**Impact on plan:** Both were correctness necessities inside planned scope. The impl-placement change preserves every locked pattern (coarse trait, pipeline delegation, single expose site). No scope creep.

## Issues Encountered
- snapbox's single-line `str![]` macro must be single-line for SNAPSHOTS=overwrite's fixer, AND overwrite rewrites `[..]` elisions with literal captured values — dynamic-content goldens are un-maintainable under blanket overwrite runs; resolved by keeping only fixture-pinned exact goldens and asserting dynamic envelopes programmatically (documented in the test file)
- `serde_json::json!` integer literals beyond i32 need explicit `i64` suffixes (overflowing-literals deny) — fixtures use suffixed literals

## Authentication Gates

None — wiremock covers the contract; the live-suite additions are skip-by-default and inherit 02-01's env contract (`IGNITION_LIVE_URL`/`IGNITION_LIVE_TOKEN`, see [02-USER-SETUP.md](./02-USER-SETUP.md)). The unauthenticated StatusPing live check intentionally needs URL only.

## User Setup Required

None beyond 02-01's opt-in live suite (already documented in [02-USER-SETUP.md](./02-USER-SETUP.md)).

## Next Phase Readiness
- The full inspection surface (models + actions + commands) is live for 02-03 (sessions/terminate) to copy structurally: capability file → trait methods → action → subcommand → goldens
- `/StatusPing` header-less fetch + `GatewayRestarting` classification are ready for 02-04's restart and 02-05's wait loops (the research's wait design consumes exactly these primitives)
- status_ping's unknown-state passthrough (`STARTING`, commissioning-era states) is the not-ready semantics 02-05 treats
- No new CoreError variants were added (verification constraint held); error.rs untouched since 02-01

## Self-Check: PASSED
