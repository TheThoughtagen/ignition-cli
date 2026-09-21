---
phase: 05-webdev-backend-tag-operations
plan: 01
subsystem: api
tags: [webdev, jython, ignition, tags, alarms, tag-history, script-exec, include_str, embedded-assets]

# Dependency graph
requires:
  - phase: 03-project-operations
    provides: project-zip import machinery (05-03 deploy rides it verbatim)
  - phase: 05-webdev-backend-tag-operations/05-RESEARCH
    provides: live-proven WebDev wire protocol, corrected scripting call forms, WHK-Global auth pattern
provides:
  - Five self-contained action-dispatch WebDev route sources under webdev/routes/ (tags, tagConfig, alarms, tagHistory, scriptExec)
  - Embedded always-on route bundle (ignition-core::webdev::ROUTE_FILES, 13 members) zippable from the binary with no source checkout
  - SCRIPT_EXEC_TEMPLATE with the single __IGN_CLI_SECRET__ substitution marker (fail-closed until substituted)
  - Version handshake contract: {routeVersion: 1.0.0, minCli: 1.0} in every route, pinned to Rust consts + VERSION file
affects: [05-02 resource e2e witnesses, 05-03 webdev deploy/status, 05-04 tags CLI family, 05-05 alarms/history CLI, webdev status doctor probe]

# Tech tracking
tech-stack:
  added: [] # no new crates — include_str! embeds the bundle; zip dep belongs to 05-02
  patterns:
    - "Action-dispatch doPost routes with a uniform body envelope {ok,data}/{ok,error{code,message,traceback?}} — WebDev denials ride HTTP 200"
    - "Self-contained route folders: ~25-line shared core (unicode re-parse, jv() depth-capped walker, bare-except) duplicated per route by design — no cross-resource imports"
    - "Template-with-marker secret baking: SECRET = None or '<marker>' — deploy substitutes the marker once; placeholder-shape (leading underscore) detector fail-closes both unconfigured states"
    - "Compile-time asset embedding pinned by string-containment contract tests (Jython not parseable in Rust)"

key-files:
  created:
    - webdev/routes/project.json
    - webdev/routes/VERSION
    - webdev/routes/com.inductiveautomation.webdev/resources/cli/tags/doPost.py
    - webdev/routes/com.inductiveautomation.webdev/resources/cli/tagConfig/doPost.py
    - webdev/routes/com.inductiveautomation.webdev/resources/cli/alarms/doPost.py
    - webdev/routes/com.inductiveautomation.webdev/resources/cli/tagHistory/doPost.py
    - webdev/routes/com.inductiveautomation.webdev/resources/cli/scriptExec/doPost.py
    - crates/ignition-core/src/webdev/mod.rs
  modified:
    - webdev/README.md
    - crates/ignition-core/src/lib.rs

key-decisions:
  - "scriptExec secret mechanism: SECRET = None or '__IGN_CLI_SECRET__' — single-marker string substitution at deploy; None AND placeholder-shape (leading underscore) both fail-closed, so the publicly-visible template can never arm the gate"
  - "scriptExec config.json stays require-auth FALSE / user-source '' (secret-only posture): API tokens 401 on require-auth routes, so a Basic layer would lock the CLI's own calls out (research OQ3 resolved)"
  - "alarm history denial is a structured machine code (no_alarm_journal) detected from the raised error text — the CLI maps it to an actionable slug naming the missing journal chain"
  - "scriptExec exec: eval-first (single expressions return a value) with exec fallback (statements, optional _result global) + StringIO stdout capture/restore + per-invocation audit log (sha256[:12] + elapsedMs)"
  - "browse passes ALL entries through including Property children — filtering stays the CLI's display decision"

patterns-established:
  - "WebDev route envelope: every response is {'json': {...}} with ok/error semantics; never branch on HTTP status"
  - "ROUTE_FILES manifest layout: (zip member path, include_str! contents) pairs with forward-slash Designer-native member names"

# Metrics
duration: 24min
completed: 2026-08-24
---

# Phase 5 Plan 1: WebDev Route Sources + Embedded Bundle Summary

**Five live-proven Jython action-dispatch routes (21 actions) under webdev/routes/, embedded into ignition-core as a 13-member always-on bundle + separate fail-closed scriptExec template, with zero new dependencies**

## Performance

- **Duration:** 24 min
- **Started:** 2026-08-24T14:25:04Z
- **Completed:** 2026-08-24T14:49:18Z
- **Tasks:** 3
- **Files modified:** 19 (17 created, 2 modified)

## Accomplishments
- tags (version/browse/read/write) and tagConfig (version/getConfig/configure/deleteTags/listUDTTypes/getUDTDefinition/exportTags) routes encoding all six prior-art defect corrections (string-arg getConfiguration, basePath configure, kwargs-only exportTags, alarms-as-list implied by configure docs, tagType discriminator, t_stamp column)
- alarms (version/active/history/acknowledge) with the gateway-scope 3-arg acknowledge and structured no_alarm_journal denial; tagHistory (version/query) with mandatory Date(long()) wraps and verbatim t_stamp passthrough
- scriptExec template with the WHK-Global-ported gate: dual-header case-insensitive extract (x-ignition-cli-secret / Authorization: Bearer), sha256-both-sides constant-time compare via java.security.MessageDigest, fail-closed on None-or-placeholder — version action gated too
- ignition-core::webdev embeds the always-on bundle (ROUTE_FILES) + SCRIPT_EXEC_TEMPLATE separately; five static contract tests pin handshake constants, placeholder isolation, fail-closed default, forward-slash member names, and exact 13-member manifest size

## Task Commits

Each task was committed atomically:

1. **Task 1: tags + tagConfig route sources** - `ff9bb3e` (feat)
2. **Task 2: alarms + tagHistory + secret-gated scriptExec routes** - `d7ddb53` (feat)
3. **Task 3: Embedded bundle module + static contract tests** - `372cd6c` (feat)

**Plan metadata:** `pending` (docs: complete plan)

## Files Created/Modified
- `webdev/routes/project.json` — deploy project manifest (title ign-cli, born from the first deploy zip)
- `webdev/routes/VERSION` — route bundle version handshake anchor (1.0.0)
- `webdev/routes/com.inductiveautomation.webdev/resources/cli/{tags,tagConfig,alarms,tagHistory,scriptExec}/{resource.json,config.json,doPost.py}` — the five self-contained routes (probe-verbatim gates: doPost enabled, require-auth false)
- `crates/ignition-core/src/webdev/mod.rs` — ROUTE_BUNDLE_VERSION/MIN_CLI consts, 13-member ROUTE_FILES manifest, SCRIPT_EXEC_TEMPLATE, five contract tests
- `crates/ignition-core/src/lib.rs` — pub mod webdev registered + module map updated
- `webdev/README.md` — layout, handshake, and security-posture documentation

## Decisions Made
- Secret-template mechanics (`SECRET = None or '__IGN_CLI_SECRET__'`): satisfies the plan's twin constraints (containment test for `SECRET = None`, exactly-once marker) while making deploy a single `str.replace` and keeping BOTH unconfigured states (None, un-substituted placeholder) reject-everything via the leading-underscore shape detector
- eval-then-exec execution contract for scriptExec (plan offered "plain eval/exec"): single-expression code returns its value as `result`; statement code runs via exec with an optional `_result` global surfaced — both shapes documented in the route header
- alarm history rows are defensively shaped (per-field getattr with null fallbacks) since journal-entry fields beyond eventId/source/priority were not live-captured; the denial path (the one default rigs always hit) is exact

## Deviations from Plan

None - plan executed exactly as written.

## Authentication Gates

None — no external auth required (pure source/embedding work; live verification belongs to later plans' e2e gates).

## Issues Encountered
- **Concurrent sibling executor (05-02) in the same working tree** — wave-1 parallelization. File sets were disjoint per plan frontmatter; discipline held: staged only plan-scoped files, commits interleaved cleanly (05-02's `4506e07` landed between mine). One 3-minute cargo lock wait absorbed into duration.
- **cargo fmt churn outside plan scope**: `cargo fmt -p ignition-core` reflowed pre-existing drift in `error.rs` + `rig/{mod,compose}.rs` (committed code, not sibling edits — verified token-identical rewraps). Reverted to keep the parallel tree uncontaminated; the drift is a pre-existing repo condition worth a future style pass, not this plan's scope.
- One `&&str` vs `str` compile fix in a test comparison during Task 3 (caught by cargo test, fixed before commit).

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- 05-03 (deploy) can zip ROUTE_FILES + substituted SCRIPT_EXEC_TEMPLATE entirely from the binary — no filesystem dependency
- 05-04/05-05 CLI families map their verbs onto the shipped action inventory; machine error codes (no_alarm_journal, secret_required, secret_mismatch, unknown_action, not_found, route_error) are the stable route-side contract
- Live-route e2e (deploy → handshake → round-trip) belongs to this phase's later plans against a rig

---
*Phase: 05-webdev-backend-tag-operations*
*Completed: 2026-08-24*

## Self-Check: PASSED
