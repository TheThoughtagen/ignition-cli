---
phase: 02-gateway-health-inspection
plan: 03
subsystem: api
tags: [reqwest, wiremock, serde, ignition-rest, sessions, perspective, vision, connections, terminate, confirmation-guard, snapbox]

# Dependency graph
requires:
  - phase: 02-gateway-health-inspection (02-01)
    provides: "classifier + get_json pipeline, ListQuery/ListEnvelope, IgnitionMock harness, delete-ready pipeline shape, exit taxonomy (used exclusively, no additions)"
  - phase: 02-gateway-health-inspection (02-02)
    provides: "credential-REQUIRED dispatch pattern (resolve_gateway_api + secret_chain), three-mode golden harness, capability-file → trait-method → action → subcommand growth pattern"
provides:
  - "Eight new GatewayApi capabilities: designers, perspective_sessions (trailing-slash pinned), vision_clients, terminate_perspective_session (query-param DELETE), terminate_vision_client, prune_designer, database_connections, opc_connections"
  - "actions::sessions::{sessions, terminate_session} + actions::connections::{connections} with the stable all-family-keys JSON shape (filtered-out families = [], never called)"
  - "`ign sessions [--type designer|perspective|vision]`, `ign sessions terminate --type <T> --id <ID> [--message MSG]` (THE first --yes-guarded destructive command), `ign connections [--type database|opc]`"
  - "require_confirmation's dead-code gate REMOVED — the Phase-1 confirmation-guard pattern proven by a production caller (sessions terminate fires it before any API construction)"
  - "delete_with_query pipeline variant (token-auth DELETE, no CSRF, query params never a body)"
  - "Live-suite additions: live_session_families_parse + live_connections (healthchecks capture hook)"
affects: [02-04, 02-05, phase-03-projects, phase-06-tui]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Query-param DELETE pipeline: terminate routes carry their payload as QUERY params with an empty body (recorded-request proofs), classified Ok = success — response bodies ({terminated: N}, {message}) stay advisory"
    - "Guard-before-construction: destructive dispatch arms run require_confirmation BEFORE profile/secret/client resolution — usage-class errors lead (like clap), a refusal never touches config state or the gateway, envelope profile stays null"
    - "Filter-with-stable-shape actions: --type filters call ONLY the requested endpoint while every family key stays present-but-empty in the data shape (agents never key-hunt)"
    - "clap value-enum mirrors: core action enums (SessionType/ConnectionType, kebab-case Serialize) stay clap-free; cli.rs owns deriving mirrors with From conversions at the dispatch seam"

key-files:
  created:
    - crates/ignition-core/src/client/sessions.rs
    - crates/ignition-core/src/client/connections.rs
    - crates/ignition-core/src/actions/sessions.rs
    - crates/ignition-core/src/actions/connections.rs
    - crates/ignition-core/tests/sessions_contract.rs
    - crates/ignition-cli/tests/contract_sessions.rs
  modified:
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/src/actions/mod.rs
    - crates/ignition-core/src/actions/version.rs
    - crates/ignition-core/src/actions/inspect.rs
    - crates/ignition-core/tests/live_gateway.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - README.md

key-decisions:
  - "Dead-code gate came off in Phase 2 / 02-03 — EARLIER than STATE.md's logged 'until Phase 3's first destructive caller' — a sanctioned roadmap-planner refinement: the attribute's own reason string mandates removal at the first real caller, which is sessions terminate"
  - "Sessions --type rides the SessionsArgs TOP level (no nested List subcommand) and terminate's id is a --id OPTION — both resolve the plan's cli.rs sketch in favor of the LOCKED must_have truths ('ign sessions --type X' and 'terminate --type <t> --id <ID>')"
  - "The confirmation guard fires BEFORE profile/secret/client resolution: usage-class refusal (exit 2) leads over config errors, a refusal costs nothing, envelope profile is null by construction"
  - "healthchecks stays a RAW serde_json::Value passthrough with the LOW-confidence flag in code + tests + README (research Open Question 1); live_connections is the capture hook for the populated shape"
  - "Designer/Vision numeric fields are i64 epoch-ms (openapi 'number' + codebase precedent); Perspective's unmodeled known keys (sessionScope, pageIds, byte counters) deliberately ride the flatten passthrough"

patterns-established:
  - "Destructive-command dispatch shape (guard → resolve_gateway_api → action) that Phase 3's project delete and Phase 4's rig reset inherit verbatim"
  - "Exact-path wiremock pins for path-suffix subtleties (trailing slash in/out) + recorded-request proofs for query-param DELETEs — assertions on the REQUEST, not just the response"

# Metrics
duration: 14min
completed: 2026-08-21
---

# Phase 2 Plan 3: sessions + termination + DB/OPC connections Summary

**Eight session/connection GatewayApi capabilities (incl. the trailing-slash-pinned Perspective list and query-param DELETE terminate) feeding `ign sessions` / `ign connections`, with `ign sessions terminate` as the CLI's first --yes-guarded destructive command — the Phase-1 confirmation guard's dead-code scaffolding removed the moment it gained its first production caller.**

## Performance

- **Duration:** 14 min
- **Started:** 2026-08-21T23:27:41Z
- **Completed:** 2026-08-21T23:41:39Z
- **Tasks:** 3
- **Files modified:** 15 (6 created, 9 modified)

## Accomplishments
- HLTH-08 shipped: all three session families (designers / Perspective / Vision) listable in one merged output, filterable by `--type`, and terminable through kind→endpoint mapping (designer→prune, perspective→terminate with `--message`, vision→terminate) — the webpage's Sessions pages replaced
- THE destructive-command convention is proven in production: `ign sessions terminate` refuses with exit 2 + `confirmation_required` (hint names `--yes` AND `IGNITION_YES=1`) BEFORE any API construction; with `--yes` the gateway-side DELETE succeeds; a nonexistent id exits 6 `not_found` — all three paths golden-pinned
- The dead-code gate came off `require_confirmation` in the same commit its first caller landed (clippy -D warnings clean WITH it removed — that clean run IS the proof); STATE.md's "until Phase 3" note was a roadmap-planner approximation the attribute's own reason string overrides
- Path subtleties are contract-pinned by recorded-request proofs, not just response parsing: Perspective GET hits the EXACT trailing-slash `/data/perspective/api/v1/sessions/` (Pitfall 8); the DELETE carries `sessionId` (+optional `message`) as QUERY params with an EMPTY body against the no-trailing-slash path; vision/designer terminate hit their singular `/client/{id}` / `/designer/{id}` routes
- HLTH-05/06 shipped honestly: connections ride the network-captured `resources/list/ignition/{database,opc}-connection` mechanism (the ignition-mcp `/connections/*` inventions appear nowhere) with `healthchecks` as raw passthrough and the LOW-confidence flag documented in code, tests, README, plus a `live_connections` capture hook

## Task Commits

Each task was committed atomically:

1. **Task 1: session list + terminate capabilities (wiremock-proven)** - `a66d90a` (feat)
2. **Task 2: DB/OPC connections via resources/list + healthchecks verification** - `df00ab0` (feat)
3. **Task 3: `ign sessions` (+ terminate, first --yes caller) + `ign connections` + goldens** - `80239ed` (feat)

## Files Created/Modified
- `crates/ignition-core/src/client/sessions.rs` - DesignerInfo/PerspectiveSession/VisionClient (openapi shapes, camelCase renames, memory-is-an-object passthrough) + verified path consts incl. the trailing-slash pin
- `crates/ignition-core/src/client/connections.rs` - GatewayConnection with RAW healthchecks Value + LOW-confidence flag; resource-list path consts
- `crates/ignition-core/src/client/mod.rs` - trait +8 methods; delete_with_query pipeline (token-auth DELETE, no CSRF); impl bodies in the single impl block
- `crates/ignition-core/src/actions/sessions.rs` - SessionType (kebab Serialize) + sessions/terminate_session + recording-double unit tests (filter selectivity, kind→endpoint mapping)
- `crates/ignition-core/src/actions/connections.rs` - ConnectionType + connections action + unit tests
- `crates/ignition-core/src/actions/mod.rs` - modules registered
- `crates/ignition-core/src/actions/version.rs`, `inspect.rs` - existing test doubles stubbed for the grown trait (8 methods)
- `crates/ignition-core/tests/sessions_contract.rs` - 12 wiremock scenarios: three list families, trailing-slash pin, query-param+no-body DELETE proofs, 404→NotFound, HTML 403→Auth, connections trio
- `crates/ignition-core/tests/live_gateway.rs` - +2 opt-in live checks (now 7): session families parse; live_connections healthchecks capture hook
- `crates/ignition-cli/src/cli.rs` - Sessions(SessionsArgs) with top-level --type + Option<SessionsCmd::Terminate{--type,--id,--message}>; Connections{--type}; clap value-enum mirrors + From conversions
- `crates/ignition-cli/src/main.rs` - ActionOutput ×3; terminate arm guard-first then resolve_gateway_api; **require_confirmation cfg_attr(not(test), expect(dead_code)) DELETED** + doc updated
- `crates/ignition-cli/src/render.rs` - family sections with counts (`designers (1)`), terminate line, connection rows with compact-JSON healthchecks
- `crates/ignition-cli/tests/contract_sessions.rs` - 6 golden/contract tests: three-mode sessions goldens, --type selectivity (unmounted families prove no call), exit-2 refusal golden, --yes success + IGNITION_YES probe, 404 exit-6 envelope, connections goldens
- `README.md` - three command rows + the "Destructive operations" section

## Decisions Made
- **Gate removal timing (plan-mandated note):** the dead-code gate came off in 02-03, earlier than STATE.md's logged "until Phase 3's first destructive caller" — the attribute's own reason string says "flags removal when it gains a caller", and sessions terminate IS that caller; the plan itself instructs recording this refinement
- **must_haves beat the plan's cli sketch** where they conflicted: `--type` sits on `SessionsArgs` (bare `ign sessions --type X` parses — truth #1) instead of a nested `List` subcommand, and terminate's id is `--id <ID>` (truth #2) instead of a positional
- **Guard-before-construction ordering:** the refusal envelope's `profile` is `null` by construction — usage-class errors lead over config/profile errors (mirroring clap's exit-2 precedence), and a refusal performs zero config or network work
- **Goldens stay exact** (02-02 policy): every value comes from pinned fixtures; the exit-2 golden is fully static; only the 404 envelope (dynamic mock URI) is programmatic — SNAPSHOTS=overwrite verified a no-op
- **i64 epoch-ms numerics** for designer/vision fields following the Overview.uptime precedent; Perspective's non-listed known keys ride extras rather than being modeled (wire-faithful + plan's field list honored)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Patch script dropped prune_designer from version.rs FakeApi**
- **Found during:** Task 2 (workspace clippy run)
- **Issue:** The scripted stub-append to the version.rs test double replaced the prune_designer block instead of appending after it, breaking the trait impl (E0046)
- **Fix:** Re-inserted the prune_designer stub alongside the two connection stubs
- **Files modified:** crates/ignition-core/src/actions/version.rs
- **Verification:** cargo test --workspace green (49 lib tests incl. the version matrix), clippy -D warnings clean
- **Committed in:** df00ab0 (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug introduced-and-fixed within the same task)
**Impact on plan:** No scope change — the fix restored the planned stub set. The two plan-internal cli-surface conflicts were resolved in favor of the LOCKED must_haves (documented under Decisions Made, not deviations).

## Issues Encountered
- reqwest form-encodes query-param spaces as `+` not `%20` — the message-param assertion accepts both encodings (the gateway decodes either)
- The exit-2 golden initially captured via SNAPSHOTS=overwrite during the `--id`-as-flag fixup contained clap usage text; after making `--id` an option per must-have truth #2, regeneration produced the true confirmation_required envelope (which also revealed the envelope's `endpoint: null` field the hand-written golden had missed)

## Authentication Gates

None — wiremock covers the contract; the two new live checks are skip-by-default and inherit 02-01's env contract (`IGNITION_LIVE_URL`/`IGNITION_LIVE_TOKEN`, see [02-USER-SETUP.md](./02-USER-SETUP.md)).

## User Setup Required

None beyond 02-01's opt-in live suite. OPEN QUESTION carried for UAT: `live_connections` should be run ONCE against a gateway with a configured DB/OPC connection to capture the populated `healthchecks` shape (then the passthrough can become a typed model); the capture hook prints the shape on stderr.

## Next Phase Readiness
- The destructive-command dispatch shape (guard → resolve_gateway_api → action) is proven and ready for 02-04's `ign restart` (--yes guarded) and Phase 3's project mutations
- The trailing-slash/query-param path-discipline pattern (exact-path matchers + recorded-request proofs) is the template for 02-04's restart POST and 02-05's wait polls
- All 16 test suites green; no new CoreError variants (verification constraint held); error.rs untouched since 02-01
- Remaining for Phase 2: 02-04 (logs + restart) and 02-05 (wait + doctor)

## Self-Check: PASSED
