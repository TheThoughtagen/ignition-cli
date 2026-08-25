---
phase: 05-webdev-backend-tag-operations
plan: 04
subsystem: api
tags: [ignition, tags, tag-provider, webdev, rest, clap, wiremock, snapbox]

# Dependency graph
requires:
  - phase: 05-03
    provides: webdev_route_call generic seam + probe matrix (405/402/401) + deploy/status + route-version slugs
provides:
  - Native tag-provider CRUD (list/find/create/delete-by-signature) via config-resource REST — client/tags.rs
  - tags browse/read/write actions riding webdev_route_call with the shared require_routes version precondition
  - provider_not_found additive exit-6 slug
  - ign tags CLI family (provider list|create|delete guarded, browse tree, read batch, write JSON-scalar)
  - The precondition-helper template every 05-05/06 webdev-dependent action inherits
affects: [05-05 tag-config, 05-06 alarms/tag-history, MCP parity surface]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "require_routes(api, project) precondition helper — one probe round trip before EVERY webdev-dependent action (correctness over latency, no caching this phase)"
    - "Native config-resource REST seam: list/find/create(ARRAY body)/delete(name+signature path) with wire-faithful camelCase passthrough models"
    - "Tree rendering at the render layer only — action data stays a flat ordered list with fullPath so nesting is derivable (agents get the flat shape)"

key-files:
  created:
    - crates/ignition-core/src/client/tags.rs
    - crates/ignition-core/src/actions/tags.rs
    - crates/ignition-core/tests/tags_contract.rs
    - crates/ignition-cli/tests/contract_tags.rs
  modified:
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/e2e_webdev.rs
    - crates/ignition-core/src/error.rs
    - README.md

key-decisions:
  - "Providers ride NATIVE config-resource REST (no route dependency); browse/read/write ride the deployed tags route — the healthier seam per research (tagCount metrics, healthchecks)"
  - "ONE shared require_routes precondition: 405→routes_not_deployed, 402→webdev_unlicensed, version mismatch→route_version_mismatch — one extra round trip per command, documented, no caching this phase"
  - "tags provider delete = 6th --yes-guarded destructive verb, binary-pinned pre-resolution (zero wire work); find-miss = additive provider_not_found exit 6"
  - "Write-scalar-is-JSON rule: --value parses as JSON scalar (42/1.5/true/null); unparseable text rides as the bare string; arrays/objects refuse invalid_input exit 2 pre-resolution"
  - "Browse human mode renders an indented tree derived from fullPath nesting (planner discretion: tree > table for a tag hierarchy); JSON mode stays the flat agent shape"
  - "Quality strings are DATA, never parsed: a missing tag reads back Bad_NotFound at exit 0 — the honest oracle the live gate exploits (verifiable without mutation side effects)"

patterns-established:
  - "Precondition-helper template: every future webdev-dependent action (05-05/06) calls require_routes first"
  - "Grouped-subfamily CLI pattern: tags provider … nests one level; browse/read/write ride the top level"

# Metrics
duration: ~450min (3 sessions, 2 cancelled + recovery)
completed: 2026-08-24
---

# Phase 5 Plan 4: Tag Operations (Provider CRUD + Browse/Read/Write) Summary

**Native tag-provider CRUD (signature-chained, array-body POST) plus the first WebDev-route consumers — browse-as-filtered-tree, batch read, JSON-scalar write — every route-dependent command gated by the version precondition that refuses exit 6 naming `ign webdev deploy`.**

## Performance

- **Duration:** ~450min across 3 executor sessions (2 cancelled mid-Task-3, this session recovered the in-flight tree)
- **Started:** 2026-08-24 (first session)
- **Completed:** 2026-08-24T17:45:00Z (recovery session)
- **Tasks:** 3
- **Files modified:** 21 (2867 insertions)

## Accomplishments
- Provider CRUD on the NATIVE config-resource seam: list (tagCount/health metrics, System flagged managed), create (fixed STANDARD array-body POST), delete (find→signature→DELETE chain) — all wire-pinned at the wiremock layer
- browse/read/write as the FIRST route consumers: one shared `require_routes` precondition (405/402/version-mismatch matrix), Property children filtered by default, case-insensitive substring filter, verbatim quality passthrough
- `ign tags` CLI family with the 6th destructive-verb binary pin, indented-tree human rendering, flat JSON agent shape, JSON-scalar write rule, and a self-contained live e2e loop (deploy → create → browse → Bad_NotFound passthrough → guarded delete)

## Task Commits

Each task was committed atomically:

1. **Task 1: native provider CRUD client + actions** — `8afbf9d` (feat)
2. **Task 2: browse/read/write actions with the version precondition** — `28a522a` (feat)
3. **Task 3: CLI family, tree renderer, goldens, live gate** — `f55616b` (feat, recovered from cancelled sessions' uncommitted tree)
4. **Formatting normalization** — `1b5ecc6` (style: fmt drift in ignition-core trait-double stubs + test rigs)

**Plan metadata:** (this commit)

## Files Created/Modified
- `crates/ignition-core/src/client/tags.rs` — native provider wire methods (list/find/create/delete) + BrowseEntry wire model
- `crates/ignition-core/src/actions/tags.rs` — provider/browse/read/write actions + require_routes precondition + filter logic
- `crates/ignition-core/tests/tags_contract.rs` — 9 wiremock pins: array-body POST, signature-chained DELETE path, refusal matrix, filter behavior, read/write body shapes
- `crates/ignition-cli/src/cli.rs` — TagsArgs/TagsCommand/TagsProviderCommand clap arms
- `crates/ignition-cli/src/main.rs` — dispatch, provider-delete guard, parse_write_scalar
- `crates/ignition-cli/src/render.rs` — indented-tree browse renderer, provider table, read rows
- `crates/ignition-cli/tests/contract_tags.rs` — 7 binary goldens over wiremock
- `crates/ignition-cli/tests/e2e_webdev.rs` — live_tags_provider_browse_read_write_loop (#[ignore] gate)
- `crates/ignition-core/src/error.rs` — provider_not_found slug + tests
- `README.md` — six command rows, JSON-scalar rule, exit-table sync

## Decisions Made
- Providers on native REST, tag ops on the deployed route — the research-recommended split (native seam has healthier data; no route needed for provider verbs)
- `require_routes` shared helper with one extra probe round trip per command — correctness over latency, explicitly documented, caching deferred
- Browse tree rendering lives in render.rs only; the action's flat list with fullPath keeps the agent shape stable
- Live gate exploits quality-as-data: reading/writing a nonexistent path asserts Bad_NotFound/Bad quality at exit 0 — an honest oracle with zero mutation side effects

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Recovered uncommitted Task 3 tree from two cancelled executor sessions**
- **Found during:** Task 3 (recovery session start)
- **Issue:** Two prior sessions were cancelled mid-Task-3, leaving the CLI family, goldens, e2e extension, and README rows uncommitted alongside unrelated fmt drift in 11 ignition-core files
- **Fix:** Verified the in-flight tree against the plan (all six verbs, guard pin, tree render, goldens, live loop, README), ran the full verification suite (workspace tests 449 passed / 0 failed, clippy -D warnings clean, fmt clean), then committed the Task-3 files atomically and the pure-formatting core drift as a separate style commit (05-03 precedent)
- **Files modified:** all Task 3 files + 11 fmt-only ignition-core files
- **Verification:** `cargo test --workspace` green; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --check` clean; help output lists all six verbs
- **Committed in:** f55616b + 1b5ecc6

---

**Total deviations:** 1 auto-fixed (1 blocking/recovery)
**Impact on plan:** Recovery only — no scope change; the in-flight work matched the plan exactly.

## Issues Encountered
- One environmental test failure at session start: `rig::compose::tests::run_streaming_forwards_lines_via_piped_stdout` needed the Docker daemon (OrbStack not running) — started OrbStack, suite green. Unrelated to this plan's changes.

## User Setup Required

None - no external service configuration required. (Live e2e gates remain opt-in via IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN + IGNITION_LIVE_MUTATIONS=1, unchanged from prior plans.)

## Next Phase Readiness
- The require_routes precondition template is ready for 05-05 (tagConfig) and 05-06 (alarms/tagHistory) — every webdev-dependent action calls it first
- Parity note: browse_tags / read_tags / write_tag / list-get-create-delete_tag_provider map 1:1 onto the MCP 21-tool surface (intent-level)
- Provider verbs are usable on any 8.3 gateway immediately (no deploy needed)

---
*Phase: 05-webdev-backend-tag-operations*
*Completed: 2026-08-24*

## Self-Check: PASSED
