---
phase: 04-rig-lifecycle-trial-state
plan: 01
subsystem: rig
tags: [docker, compose, rig, tokio-process, shell-out, discovery, ldjson]

# Dependency graph
requires:
  - phase: 03-project-operations
    provides: envelope chassis, ActionOutput machinery, contract-test harness conventions
provides:
  - ComposeRunner seam (run/run_docker) — the test seam every rig verb scripts
  - RigPlan model + 5-level discovery (resolve-then-act; .env identity truth)
  - LOCKED compose arg builders (config/up/down/ps/logs/volume-ls/docker-ps) exact-pinned
  - LDJSON + single-doc parsers (both Pitfall-1 conventions fixture-pinned)
  - rig_up / rig_down / rig_status actions with commissioned-wait (uncommissioned-as-data)
  - `ign rig [--rig] up|down|status` CLI tree, docker-only profile:null envelope
  - [rig]/[rigs.*] config surface + IGNITION_RIG/IGNITION_RIG_ROOTS env folds
affects: [04-02 rig reset/logs, 04-03 trial, 04-04 snapshot/restore, phase-06 TUI]

# Tech tracking
tech-stack:
  added: [] # tokio `process` feature ONLY — no new crates
  patterns:
    - "ComposeRunner trait seam: actions never spawn processes (the GatewayApi precedent, process edition)"
    - "Resolve-then-act: ONE config --format json run at discovery; resolved .name = -p identity for every op"
    - "Probe-side translation: GatewayNotCommissioned -> Pending (poll.rs retry set LOCKED, untouched)"
    - "Uncommissioned-as-data: only the locked source:None deadline error degrades to exit 0"

key-files:
  created:
    - crates/ignition-core/src/rig/mod.rs
    - crates/ignition-core/src/rig/compose.rs
    - crates/ignition-core/src/actions/rig.rs
    - crates/ignition-cli/tests/contract_rig.rs
  modified:
    - Cargo.toml
    - crates/ignition-core/src/lib.rs
    - crates/ignition-core/src/config/mod.rs
    - crates/ignition-core/src/config/profile.rs
    - crates/ignition-core/src/actions/mod.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - README.md

key-decisions:
  - "Discovery order LOCKED: --rig > IGNITION_RIG > [rig].default > cwd candidates > git-module > WHK-Global (both home roots, first hit wins) — the must-have truth overrode Task 3's inverted test blurb"
  - "Stale [rig].default is a LOUD exit-7 error naming the missing entry, never a silent scan"
  - "Uncommissioned degradation matches ONLY CoreError::Network{source:None} (the 02-04 deadline marker) with a terminal-uncommissioned flag — any other error class stays an error"
  - "`rig`/`project` output keys both carry the compose project name (identity truth); distinct keys reserved for future alias divergence"
  - "ports_free = no docker container publishes any host port (own project counts as occupied — a running rig holds its own ports)"
  - "Research Open Question 4 RESOLVED live: --project-directory makes the rig's .env COMPOSE_PROJECT_NAME authoritative even cwd-elsewhere (test runs real docker compose config, quiet-skips without docker)"
  - "IGNITION_RIG_ROOTS (path-separated) overrides the WHK convention home roots — binary-test isolation + a real agent affordance"

patterns-established:
  - "Plain-docker shapes (volume ls, docker ps) ride ComposeRunner::run_docker — the fake runner records which program shape each op used"
  - "Line-based LDJSON parsing (newline IS the delimiter): stray non-JSON lines warn+skip instead of halting the stream"

# Metrics
duration: 42min
completed: 2026-08-22
---

# Phase 4 Plan 1: Rig engine + lifecycle core Summary

**Compose shell-out engine (`ComposeRunner` seam, 5-level discovery resolving `.env`-true project names, port pre-flight) + `ign rig up/down/status` with commissioned-wait and uncommissioned-as-data — the first docker-only, `profile: null` commands**

## Performance

- **Duration:** 42 min
- **Started:** 2026-08-22T18:51:37Z
- **Completed:** 2026-08-22T19:33:47Z
- **Tasks:** 3
- **Files modified:** 14 (4 created, 10 modified)

## Accomplishments

- **The engine** (`rig/compose.rs`): runner seam (`run` = `docker compose`, `run_docker` = plain docker), version gate (≥ v2 + install hint), all LOCKED arg builders exact-vector-pinned, exit mapping with stderr tails, and both compose output conventions (single-doc config with object/array + string/numeric `published` tolerance; line-delimited ps/volume-ls/docker-ps with string/map `Labels` tolerance) fixture-pinned from the researcher's live captures.
- **Discovery** (`rig/mod.rs`): the 5-level order with both-home-roots convention probing (parameterized for tests — the checker-found dead-path bug class is now test-covered without touching real home dirs), always ending in the resolve-then-act config run whose `.name` is the explicit `-p` identity; search-trail errors for agents; port pre-flight with container/host-process attribution.
- **The actions** (`actions/rig.rs`): up (version gate → preflight → `up -d --wait` → commissioned wait via poll.rs verbatim), down (volumes kept), status (strict allowlist — exact `json!` shape comparisons prove no compose-config passthrough); uncommissioned degrades to exit-0 data with the wizard URL in `warnings`.
- **The CLI**: `rig` tree with only the wired verbs (extend-per-plan chore documented), docker-only dispatch (no profile/secret/client; `profile: null` on success AND error), `IGNITION_RIG` env fold in the one env→flag home, header-less probe client pointed at the rig's derived gateway URL (8088→http / 443→https heuristic).
- **Live-verified end-to-end** on this machine: from `/tmp` with zero config, discovery found the git-module convention rig, resolve honored the rig's own `.env` (`ignition-devops`), and the down rig reported as exit-0 data (`ports_free: true`, empty services, `profile: null`).

## Task Commits

Each task was committed atomically:

1. **Task 1: Compose engine — RigPlan, discovery, runner seam, parsers** - `661dce5` (feat)
2. **Task 2: Actions — rig_up / rig_down / rig_status + commissioned wait** - `7fc40b8` (feat)
3. **Task 3: CLI wiring — rig command tree, dispatch, goldens, README** - `dc7dc40` (feat)

**Plan metadata:** (see final docs commit)

## Files Created/Modified

- `crates/ignition-core/src/rig/compose.rs` - the engine: runner seam, version check, LOCKED arg builders, LDJSON/single-doc parsers, exit mapping
- `crates/ignition-core/src/rig/mod.rs` - RigPlan (+port mappings), 5-level discovery, path expansion, port pre-flight
- `crates/ignition-core/src/actions/rig.rs` - rig_up/rig_down/rig_status + commissioned_wait (poll.rs verbatim) + gateway_url_from
- `crates/ignition-cli/tests/contract_rig.rs` - binary contract tests (no-rig trail, unknown-rig knowns, precedence, env fold, help)
- `crates/ignition-core/src/config/profile.rs` + `config/mod.rs` - `[rig]`/`[rigs.*]` surface, unknown-key tolerance, byte-identical profile-only serialization
- `crates/ignition-cli/src/{cli,main,render}.rs` - rig tree, docker-only dispatch, human renderers
- `Cargo.toml` - tokio `process` feature (the ONLY Phase 4 dependency change)
- `README.md` - Rig section (discovery table, profile:null contract, uncommissioned-as-data, allowlist note), commands rows, exit-7 live

## Decisions Made

- Discovery precedence contradiction in the plan resolved in favor of the LOCKED must-have truth (`[rig].default` beats the cwd scan) — the Phase 02-03 precedent of must_haves overriding plan sketches, applied again; pinned at both unit and binary level.
- The uncommissioned degradation fires only on the exact poll-deadline error shape (`Network { source: None }`, the 02-04 marker) AND a witnessed terminal-uncommissioned observation — an unexpected error class mid-wait (e.g. a parse failure) stays a real error instead of being masked as data.
- Status `ports_free` is docker-occupancy only (lsof stays advisory, pre-flight-only) — a cheap, honest "is it down" signal.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] LDJSON stream parser halted on stray lines**
- **Found during:** Task 1 (parser unit tests)
- **Issue:** The researched `StreamDeserializer` approach stops iterating at the first unparseable line — the "skip warning lines" tolerance the plan's parser contract implied was false (0 rows survived a leading `[NOTE]` line in test).
- **Fix:** Line-based parsing (newline IS the LDJSON delimiter; per-line parse with warn+skip) via a shared `parse_ldjson` helper used by all three LDJSON parsers.
- **Files modified:** crates/ignition-core/src/rig/compose.rs
- **Verification:** `parse_ps_ldjson_empty_and_warning_lines` pins 1 row surviving a warning line; all parser fixtures green.
- **Committed in:** 661dce5 (part of task commit)

**2. [Rule 3 - Blocking] Plan-internal precedence contradiction (config default vs cwd)**
- **Found during:** Task 3 (contract test design)
- **Issue:** The plan's must-have truth #4 and BOTH body statements (main.rs + config sections) rank `[rig].default` above the cwd scan; Task 3's test blurb ("temp cwd with compose.yml beats config default") states the opposite.
- **Fix:** Followed the LOCKED must_have truth (the established override rule); the binary precedence test pins default-beats-cwd via the deterministic docker-less resolve failure, and a unit test pins it at engine level too.
- **Files modified:** crates/ignition-core/src/rig/mod.rs (ordering + tests), crates/ignition-cli/tests/contract_rig.rs
- **Verification:** `config_default_beats_cwd_candidates` (unit) + `discovery_precedence_config_default_beats_cwd` (binary) both green.
- **Committed in:** 661dce5 / dc7dc40 (part of task commits)

**3. [Rule 3 - Blocking] `gateway_url_from` required `pub`, not `pub(crate)`**
- **Found during:** Task 3 (dispatch wiring)
- **Issue:** The plan sketched `pub(crate)`, but the CLI dispatch must call it to build the header-less probe client (actions take `&dyn GatewayApi`; they cannot construct clients).
- **Fix:** `pub fn gateway_url_from` with the documented heuristic unchanged.
- **Files modified:** crates/ignition-core/src/actions/rig.rs
- **Verification:** Workspace compiles; heuristic unit-tested (8088-first, 443-fallback, none).
- **Committed in:** 7fc40b8 (part of task commit)

**4. [Rule 2 - Missing critical] Convention-root isolation for deterministic tests**
- **Found during:** Task 3 (binary test design)
- **Issue:** On machines WITH real WHK checkouts (this one), the convention discovery levels find a rig — "missing rig" and precedence tests become machine-dependent and fail.
- **Fix:** `IGNITION_RIG_ROOTS` (path-separated) env override for the convention home roots (documented in README); every binary test exports an empty temp dir. Also a genuine affordance for agents with rigs checked out elsewhere.
- **Files modified:** crates/ignition-core/src/rig/mod.rs, crates/ignition-cli/tests/contract_rig.rs, README.md
- **Verification:** All contract_rig tests deterministic; live smoke still finds the real rig without the override.
- **Committed in:** 661dce5 / dc7dc40 (part of task commits)

**5. [Rule 3 - Blocking] RigPlan needed port mappings (target→published) beyond Pattern 1's sketch**
- **Found during:** Task 1→2 hand-off
- **Issue:** The plan's own Task-2 helper `gateway_url_from` selects on TARGET ports (8088/443), but Pattern 1's model carries only published `host_ports` — the helper is uncomputable from the sketched shape.
- **Fix:** `RigPlan.port_mappings: Vec<PortMapping{target, published}>` populated by `parse_config`; `host_ports` remains the published half (contract unchanged).
- **Files modified:** crates/ignition-core/src/rig/compose.rs, crates/ignition-core/src/rig/mod.rs
- **Verification:** `parse_config_reads_name_services_ports_volumes` + `gateway_url_prefers_http_8088_then_https_443`.
- **Committed in:** 661dce5 / 7fc40b8 (part of task commits)

**6. [Rule 3 - Blocking] Plain-docker shapes needed a second runner method**
- **Found during:** Task 1 (engine design)
- **Issue:** The plan sketched volume-ls as "builder takes the full program prefix as parameter" — but the runner trait only had `run` (always `docker compose`), making `docker volume ls` / `docker ps` unspawnable as pinned.
- **Fix:** `ComposeRunner::run_docker` for the plain-docker shapes; builders stay pure arg vectors, exact-pinned; the fake runner records which program shape each op used (asserted in tests).
- **Files modified:** crates/ignition-core/src/rig/compose.rs, crates/ignition-core/src/rig/mod.rs
- **Verification:** `volume_ls_args_pinned_plain_docker_shape`, preflight/status tests assert the `docker` (not `docker compose`) program tag.
- **Committed in:** 661dce5 (part of task commit)

**7. [Rule 1 - Bug] snapbox inline golden mangles embedded quotes**
- **Found during:** Task 3 (unknown-rig golden)
- **Issue:** `snapbox::str!` normalizes `"` inside string VALUES (`/"nope/"`), so the exact-message golden can never match (the PK//x03//x04 gotcha's sibling).
- **Fix:** Exact-message pin via parsed-envelope field equality (`serde_json::Value` comparison) — same exactness, no literal normalization in the way.
- **Files modified:** crates/ignition-cli/tests/contract_rig.rs
- **Verification:** `unknown_rig_name_lists_knowns_golden` green with the full message pinned.
- **Committed in:** dc7dc40 (part of task commit)

---

**Total deviations:** 7 auto-fixed (3 blocking, 2 bugs, 1 missing critical, 1 blocking/plan-conflict)
**Impact on plan:** All fixes were required for correctness, determinism, or compilability. No scope creep — the LOCKED contracts (envelope, exit taxonomy, poll.rs retry set, invocation shapes) are untouched, and poll.rs is diff-empty.

## Issues Encountered

None beyond the deviations above.

## Authentication Gates

None — no authenticated surfaces in this plan (docker-only family, header-less probe by design).

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- RIG-01 core complete: every later rig verb (reset/logs in 04-02, trial in 04-03, snapshot/restore in 04-04) rides the `ComposeRunner` seam, `RigPlan` discovery, and the extend-per-plan `RigCommand` enum.
- `logs_args` + `down_args(volumes=true)` are already declared and exact-pinned for 04-02.
- The commissioned-wait probe pattern (wiremock StatusPing doubles) is reusable for restore's restart-wait in 04-04.
- Live gateway flows (trial reset) still need rig creds per 04-03's user_setup.

---
*Phase: 04-rig-lifecycle-trial-state*
*Completed: 2026-08-22*

## Self-Check: PASSED

All key-files exist on disk; all 3 task commits (661dce5, 7fc40b8, dc7dc40) verified in git log.
