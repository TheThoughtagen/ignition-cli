---
phase: 07-ecosystem-interop-advanced-ops
plan: 06
subsystem: api
tags: [eam, tags, webdev-routes, error-classification, route-versioning, ignition, wiremock]

# Dependency graph
requires:
  - phase: 07-ecosystem-interop-advanced-ops (plan 05)
    provides: EAM family (history/tasks/guarded create/force) on the corrected wire model, classify 422 arm, the live devops rig gateways
provides:
  - force-route 409 → additive exit-6 eam_task_in_flight carrying the gateway's Jetty page text (gap 4 closed)
  - provider-ROOT tag paths refuse honestly as provider_root_unsupported (gap 5 closed, route-level option c: pre-call bracket detection + RpcContext translation for the bare form)
  - route bundle version-locked at 1.1.0 — stale 1.0.0 deployments refuse route_version_mismatch (live-proven staleness protection)
affects: [milestone UAT close-out, 07-VERIFICATION, any future route change (bundle version must move in lockstep)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Path-scoped 409 classify arms (the session_not_prunable precedent generalized: force-URL match + Jetty page sniff → verbatim detail, '(forced)' fallback when the page is absent)"
    - "Route-level honest refusal for platform limitations: pre-call shape detection where detectable, targeted try/except translation of a known gateway exception where not (everything else re-raises into the unchanged bare-exect)"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/error.rs
    - crates/ignition-core/src/client/classify.rs
    - crates/ignition-core/src/client/webdev.rs
    - crates/ignition-core/src/webdev/mod.rs
    - crates/ignition-core/src/actions/webdev.rs
    - webdev/routes/com.inductiveautomation.webdev/resources/cli/tagConfig/doPost.py
    - webdev/routes/VERSION (+ the other four route doPost.py files, version-only)
    - crates/ignition-cli/tests/contract_eam.rs
    - crates/ignition-cli/tests/contract_tags.rs
    - crates/ignition-cli/tests/contract_webdev.rs
    - README.md

key-decisions:
  - "EamTaskInFlight { task, detail, endpoint } — detail rides the gateway's Jetty page message verbatim (HTML-escaped apostrophes included); classify falls back to the '(forced)' text when the page is absent; hint names the EAM console (no ign verb deletes runs)"
  - "Provider-root fix is ROUTE-level (option c): CLI-side refusal would hide the limitation from every future consumer; bracket form is pre-detected (zero gateway work), the bare form is translated from the gateway's own 'No RpcContext' throw — every other exception re-raises into the unchanged route_error bare-except"
  - "Route bundle 1.0.0 → 1.1.0 (all five doPost.py + VERSION + ROUTE_BUNDLE_VERSION in lockstep): the route source changed, so the equality-locked bundle must move — a stale 1.0.0 deployment refuses route_version_mismatch until ign webdev deploy runs (live-proven on A)"
  - "Route-source pin in webdev/mod.rs tests: wiremock cannot execute the route's Python, so the tagConfig doPost source is assert-scanned for the refusal (detector + translation + denial code) — the route-side fix cannot silently regress"
  - "The 409 contract fixture is byte-faithful to the live page (set_body_raw HTML, &apos;-escaped apostrophes) — the 07-05 raw-capture convention"

patterns-established:
  - "Known-gateway-exception translation: wrap the specific system.* call, match the exception's signature text in the formatted traceback, return the dedicated denial, re-raise everything else — the honest-refusal pattern for platform limitations that cannot be shape-detected pre-call"

# Metrics
duration: 96min
completed: 2026-08-29
---

# Phase 7 Plan 6: UAT Gap Closure (gaps 4+5) Summary

**Both remaining 07-UAT gaps closed as honest additive exit-6 refusals and live-proven against the real 8.3.3 controller: the EAM force 409 surfaces as `eam_task_in_flight` carrying the gateway's own "already exists" page text, and provider-ROOT tag paths (bracket and bare forms) refuse as `provider_root_unsupported` naming the subtree workaround — via a route-level fix shipped in a version-locked 1.1.0 bundle whose stale-deployment refusal was itself live-proven.**

## Performance

- **Duration:** 96 min (21:42–23:18 UTC)
- **Started:** 2026-08-29T21:42:34Z
- **Completed:** 2026-08-29T23:18:26Z
- **Tasks:** 2
- **Files modified:** 15

## Accomplishments
- **Gap 4 (force 409):** path-scoped 409 classify arm on `/data/eam/api/v1/eam-tasks/force/` constructs the additive `EamTaskInFlight` — the gateway's Jetty page MESSAGE rides the detail verbatim, the task name extracts from the force URL's last segment, and the `'(forced)'` fallback covers a body-less 409. Contract-pinned against the captured live page (raw HTML wiremock body) and live-proven on gateway A (both human and compact renders).
- **Gap 5 (provider-root):** the tagConfig route detects the bracket provider-root pre-call (zero gateway work) and translates the bare form's `No RpcContext` throw into the same `provider_root_unsupported` denial; the Rust `denial_to_error` seam maps it to the new additive slug whose fixed Display names the `[provider]folder` subtree workaround. Live-proven on A in both forms after a routine redeploy; subtree paths regression-free.
- **Route bundle 1.1.0:** all seven version copies moved in lockstep; the staleness refusal (deployed 1.0.0 vs expected 1.1.0 → exit 6 `route_version_mismatch` with the redeploy hint) captured live BEFORE the redeploy — the designed protection doing exactly its job.
- Full workspace suite green: **863 passed, 0 failed** (26 opt-in live tests ignored); `cargo fmt --all` + `cargo clippy --all-targets -- -D warnings` clean; exactly two new slugs, both additive, both in the enumerated test AND the README table (two-place rule).

## Task Commits

Each task was committed atomically:

1. **Task 1: force-route 409 → additive eam_task_in_flight refusal (gap 4)** - `055def2` (feat)
2. **Task 2: provider-ROOT tag paths refuse honestly — provider_root_unsupported (gap 5, option c)** - `ab9e909` (feat)

## Live Verification (gateway A, profile `uat`, http://localhost:9088, 8.3.3 b2026012009)

All probes run with `IGNITION_TOKEN=uatdiff:…` via the debug binary. Gateway A was NOT reset (volume state untouched); its trial had re-expired since the UAT session's reset (~1h59m burn-down elapsed), so the routine `rig trial reset --yes` re-licensed the WebDev module first — the same operation the UAT session performed on A, and the `(forced)` run is EAM history state that survives it (verified below, after all probes).

**Gap 4 — the force 409 refusal (the leftover `cli-research-backup (forced)` run IS the fixture):**
```
$ ign --profile uat eam task force cli-research-backup --yes --compact
{"ok":false,"profile":"uat","error":{"code":"eam_task_in_flight","message":"EAM task cli-research-backup has a run in flight — the gateway refused the force: Task &apos;cli-research-backup (forced)&apos; already exists! It must be completed or deleted before another task of this type can be force executed.","endpoint":"http://localhost:9088/data/eam/api/v1/eam-tasks/force/eam/cli-research-backup","hint":"complete or delete the leftover '(forced)' run from the EAM console — no ign verb deletes runs; the slot frees once the run is resolved"}}
EXIT=6
```
Human mode renders the same refusal message + hint under the `[profile: uat]` header. The gateway's page text (incl. its `&apos;` HTML escaping) rides verbatim.

**Staleness proof (BEFORE redeploy — A still served the 1.0.0 bundle):**
```
$ ign --profile uat tags browse --compact
{"ok":false,"profile":"uat","error":{"code":"route_version_mismatch","message":"route \"tags\" version mismatch: deployed 1.0.0, this CLI expects 1.1.0","endpoint":"/system/webdev/ign-cli/cli/tags","hint":"run `ign webdev deploy` to redeploy the route version this CLI expects"}}
EXIT=6
```

**Redeploy (the ign-cli project is CLI-owned; routine and unguarded by design):**
```
$ ign --profile uat webdev deploy
deployed 4 routes to project ign-cli (overwrite import)
routes: tags, tagConfig, alarms, tagHistory
import: {"changes":[{"name":"ign-cli"}],"success":true}
EXIT=0
$ ign --profile uat webdev status
tags         present          1.1.0
tagConfig    present          1.1.0
alarms       present          1.1.0
tagHistory   present          1.1.0
ok: all always-on routes present with matching versions
```

**Gap 5 — bare-form provider root (the RpcContext translation path):**
```
$ ign --profile uat tags export default --compact
{"ok":false,"profile":"uat","error":{"code":"provider_root_unsupported","message":"provider-root tag paths are not supported by the deployed route (the gateway needs an RPC context WebDev threads don't carry) — target a subtree like [provider]folder","endpoint":"http://localhost:9088/system/webdev/ign-cli/cli/tagConfig","hint":"target a subtree path like [provider]folder — provider-ROOT forms ([default] alone, or a bare provider name) need an RPC context WebDev threads don't carry (8.3.3); subtree paths are the supported form"}}
EXIT=6
```

**Gap 5 — bracket-form provider root (the pre-call detection path):**
```
$ ign --profile uat tags config get [default] --compact
{"ok":false,"profile":"uat","error":{"code":"provider_root_unsupported","message":"provider-root tag paths are not supported by the deployed route (the gateway needs an RPC context WebDev threads don't carry) — target a subtree like [provider]folder","endpoint":"http://localhost:9088/system/webdev/ign-cli/cli/tagConfig","hint":"target a subtree path like [provider]folder — provider-ROOT forms ([default] alone, or a bare provider name) need an RPC context WebDev threads don't carry (8.3.3); subtree paths are the supported form"}}
EXIT=6
```

**Subtree regression probe (no regression after the redeploy):**
```
$ ign --profile uat tags browse [default]
  _types_  Folder
  uattest  AtomicTag Int4
$ ign --profile uat tags export [default]uattest --compact
{"ok":true,"profile":"uat","data":{"project":"ign-cli","paths":["[default]uattest"],"file":"uattest.json","stdout":false,"tag_count":1}}
EXIT=0
$ ign --profile uat tags config get [default]uattest --compact
{"ok":true,"profile":"uat","data":{"project":"ign-cli","path":"[default]uattest","tag_type":"AtomicTag","config":{"dataType":"Int4","defaultValue":42,"enabled":true,"name":"uattest","path":"[default]uattest","tagType":"AtomicTag","value":1234,"valueSource":"memory"}}}
EXIT=0
```

**Gateway A state PRESERVED (post-probe check):**
```
$ ign --profile uat eam history
2026-08-28T13:48:16.509Z  cli-research-backup (forced)  [Failed]  target=_controller  Attempt 1: Gateway network for agent '_controller' is currently not connected, the connection status is 'NotDefined'
(1 run(s))
```

## Files Created/Modified
- `crates/ignition-core/src/error.rs` — `EamTaskInFlight` + `ProviderRootUnsupported` variants (additive, exit 6, endpoint plumb-through, hints) + enumerated/hint test extensions
- `crates/ignition-core/src/client/classify.rs` — path-scoped 409 arm on the force route + `is_eam_force_url`/`eam_force_task_name` pure helpers with unit tests
- `crates/ignition-core/src/client/webdev.rs` — `provider_root_unsupported` → `ProviderRootUnsupported` denial arm + mapping test extension
- `crates/ignition-core/src/webdev/mod.rs` — `ROUTE_BUNDLE_VERSION` 1.1.0 + the tagConfig route-source refusal pin (test 6)
- `crates/ignition-core/src/actions/webdev.rs` — version-bump re-pins (precondition assert → BUNDLE_VERSION; degraded-status fixture alarms)
- `webdev/routes/.../tagConfig/doPost.py` — `is_provider_root` detector (nested, byte-0 rule), getConfig/exportTags pre-call + pre-flight refusals, RpcContext translation; ROUTE_VERSION 1.1.0
- `webdev/routes/VERSION` + the other four route doPost.py files — bundle version 1.1.0 in lockstep
- `crates/ignition-cli/tests/contract_eam.rs` — `task_force_conflict_refusal_golden` (the captured 409 Jetty page, raw HTML)
- `crates/ignition-cli/tests/contract_tags.rs` — both provider-root denial→slug contracts (getConfig + exportTags, refusal precedes any file write)
- `crates/ignition-cli/tests/contract_webdev.rs` — version re-pins (matching probes via the constant; goldens at 1.1.0)
- `README.md` — both exit-table rows + `eam task force` semantics note + the provider-root limitation docs (tags export / config get rows + bulk-export section)

## Decisions Made
- The provider-root fix lives in the ROUTE (option c, planner-locked): pre-call bracket detection + targeted RpcContext translation; the CLI-side refusal (b) would hide the limitation from every future route consumer.
- The bundle version moved 1.0.0 → 1.1.0 in the same commit as the route change — the equality lock (all five doPost.py + VERSION + ROUTE_BUNDLE_VERSION + the drift tests) is the staleness protection; proven live in both directions (mismatch refusal, then matching after redeploy).
- The 409 golden mounts the live page image byte-faithfully (HTML-escaped apostrophes) rather than a sanitized form — the 07-05 raw-capture convention.
- A's trial re-expiry (the UAT session's reset had ~2h) was handled with the routine `rig trial reset --yes` — the same operation the UAT session ran on A; the `(forced)` run is EAM history state and survives it (verified before, during, and after all probes).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Version-bump re-pins beyond the named drift tests**
- **Found during:** Task 2 (workspace suite after the 1.1.0 bump)
- **Issue:** The plan named only the webdev/mod.rs drift tests as the version pin, but four more fixtures hard-pinned the old version as "matching": `contract_webdev.rs`'s all-present status golden (mounts + human/compact goldens), its absent-rows and mismatch-rows expected_version asserts, and `actions/webdev.rs`'s precondition test (`contains("1.0.0")` — the error now says 1.1.0) plus the degraded-status fixture's alarms row.
- **Fix:** Re-pinned all to the new truth — matching probes now use `ignition_core::webdev::ROUTE_BUNDLE_VERSION` (future-proof), goldens carry literal 1.1.0, the precondition assert uses `BUNDLE_VERSION`; the degraded fixture's alarms row keeps its "present + matching" intent via the constant.
- **Files modified:** crates/ignition-cli/tests/contract_webdev.rs, crates/ignition-core/src/actions/webdev.rs
- **Verification:** Full workspace suite green (863/0).
- **Committed in:** ab9e909 (Task 2 commit)

**2. [Rule 1 - Bug] Plan's 409-golden "profile null" assertion contradicted the frozen envelope contract**
- **Found during:** Task 1 (contract golden authoring)
- **Issue:** The plan said to assert "profile null in the envelope" for the force-conflict refusal, but the refusal occurs AFTER profile resolution — the frozen one-field envelope carries the resolved profile (the `controller_refusal_golden` precedent asserts `"dev"`).
- **Fix:** Asserted `profile == "dev"` per the precedent; documented the envelope shape rather than the plan's slip.
- **Files modified:** crates/ignition-cli/tests/contract_eam.rs
- **Verification:** Golden passes; live envelope on A confirms `profile: "uat"` for the same refusal.
- **Committed in:** 055def2 (Task 1 commit)

**3. [Rule 3 - Blocking] Gateway A's trial had re-expired before the live probes**
- **Found during:** Task 2 verification step 1 (the staleness probe answered `webdev_unlicensed` 402, not the version handshake)
- **Issue:** The UAT session's trial reset (~2h window) had burned down; the 402 fires before the version compare, so the staleness/redeploy/provider-root sequence could not run.
- **Fix:** The routine `rig trial reset --yes` (the exact operation the UAT session performed on A; the 04-VERIFICATION recipe) re-licensed the module — `expired true → false, 1h 59m remaining`. The `(forced)` run is EAM history state and survives it (verified post-probe).
- **Files modified:** none (gateway state only)
- **Verification:** All five live probes then ran; `eam history` still shows the run.
- **Committed in:** n/a (live rig operation, no code)

---

**Total deviations:** 3 auto-fixed (1 bug, 2 blocking)
**Impact on plan:** All fixes were direct consequences of the plan's own version bump and live-rig timing; the envelope-shape correction follows the frozen contract. No scope creep.

## Issues Encountered
- None beyond the deviations above. One cosmetic observation: the live 409 page text arrives with `&apos;` HTML entities (the Jetty page escapes apostrophes) — the message carries it verbatim by design (wire honesty over cosmetics); the contract fixture matches the wire image exactly.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- **Phase 7 is COMPLETE (6/6 plans).** All five 07-UAT gaps (1–5) closed and live-verified; taxonomy additive-only; both new slugs live in the two-place exit table; guard ladder + TUI Confirm parity intact (tui_coverage green — no CLI command/flag shape changed this plan).
- Gateway A remains the standing devops rig: UP, trial active (~2h from 23:00 UTC), routes at 1.1.0, the `cli-research-backup (forced)` run and `uat-backup-demo` definition preserved as verification assets.
- Next: 07-VERIFICATION / re-verify, then `/gsd-complete-milestone`.

---
*Phase: 07-ecosystem-interop-advanced-ops*
*Completed: 2026-08-29*

## Self-Check: PASSED

All key files exist on disk; both task commits (055def2, ab9e909) verified in git log; must-have artifact spot-checks (EamTaskInFlight in error.rs, provider_root_unsupported in the tagConfig route source, 1.1.0 in webdev/routes/VERSION) pass.
