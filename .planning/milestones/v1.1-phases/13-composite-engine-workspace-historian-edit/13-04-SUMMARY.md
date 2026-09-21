---
phase: 13-composite-engine-workspace-historian-edit
plan: 04
subsystem: historian
tags: [ignition, historian, tag-history, tagconfig, live-gate, trial-rigs, wiremock, render]

# Dependency graph
requires:
  - phase: 13-composite-engine-workspace-historian-edit (13-01)
    provides: SPIKE VERDICT closure + the pinned field-set table [historyEnabled, historyProvider, sampleMode] (13-LIVE-CAPTURES.md), the full-node-shape edit caveat, corrected provider-API find/delete routes, the trial-rig ops recipe
  - phase: 05-webdev-backend-tag-operations
    provides: tagConfig route, contract_tags.rs harness, the 05-06 live fixtures + LIVE_GATE serializer
  - phase: 11-tag-bulk-transfer-xml-csv
    provides: 11-RIG-NOTES rig ops discipline (unique names, 2h windows, mount-race tolerances)
provides:
  - TAGS-13: history bindings visible in `tags config get` human output — additive `history:` block, capture-locked field names, byte-identical absence pin
  - TAGS-14 closed per the closure branch: the working binding recipe documented in README + live gate `live_tags_history_bindings` re-proving bind → written-value-in-history on BOTH trial rigs
  - e2e harness historian delete FIXED (was a life-long silent no-op) — corrected resources-API find route + verified deletion (re-find 404)
  - 13-LIVE-GATE.md: both-rig pass record + the live-truth incidents (deploy mount race, deploy-tail bind orphaning, wedged-first-boot volume)
affects: [13-05..13-08 phase plans, phase verification (SC-3/SC-4), any future historian work]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "additive-only render contract: absent keys render byte-identically (pinned by golden); block appended only when present"
    - "self-healing live gates: deploy sweep-verify + one heal redeploy; bind cycles with delete→settle→recreate recovery"
    - "never trust a volume that survived an interrupted first boot — fresh-volume re-stage over deadline bumps"

key-files:
  created:
    - .planning/phases/13-composite-engine-workspace-historian-edit/13-LIVE-GATE.md
  modified:
    - crates/ignition-core/src/actions/tags.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/contract_tags.rs
    - crates/ignition-cli/tests/e2e_webdev.rs
    - README.md

key-decisions:
  - "Closure branch implemented exactly per the 13-01 spike record — field set [historyEnabled, historyProvider, sampleMode]; historical_group stays None (no captured key name; never guessed)"
  - "The `history:` block renders ONLY present fields; sampleMode rides the captured enum verbatim ('TagGroup', never re-cased); compact envelope untouched (config passthrough already carries the keys)"
  - "Gate proves the WRITTEN value (44) in history — stronger than any-non-null, which the binding's initial-0 row would satisfy"
  - "Self-healing bind cycles (3× settle→create→assert→write→bounded poll) after live truth showed the deploy model-rebuild tail can orphan history registration (config reads back, writes answer Good, rows stay empty from birth)"
  - "Rig B's dead historian storage was traced to a volume that survived a wedged first boot — down -v + fresh volume passed first-try; recorded as rig hygiene, not 8.3.3 wire truth"
  - "Harness fix folded in per 13-01's recorded follow-up: delete_internal_historian rides the corrected generic find route and VERIFIES gone (re-find 404)"

patterns-established:
  - "Unique names per run extend to TAG PATHS (11-06's provider discipline generalized after live proof)"
  - "Poll bounded sweeps after deploys — single-shot checks race first-activation (11-06 tolerance made structural)"

# Metrics
duration: 12h 24m wall-clock (multi-session; ~2h of it live-rig windows incl. the wedged-boot recovery)
completed: 2026-09-15
---

# Phase 13 Plan 04: Historian Binding Closure (TAGS-13/14 + Live Gate) Summary

**TAGS-13's additive `history:` block landed in `tags config get` (capture-locked names, byte-identical absence pin) and TAGS-14 closed per the spike's CLOSURE branch — the binding recipe in README and a self-healing live gate that re-proved the written-value-in-history chain on BOTH trial rigs (8.3.6 + 8.3.3), with the e2e harness's long-silent historian delete fixed along the way.**

## Performance

- **Duration:** 12h 24m wall-clock (02:25Z → 14:49Z; includes ~2h of trial-rig windows, the wedged-boot recovery saga, and waiting out a parallel plan's in-flight compile breaks)
- **Started:** 2026-09-15T02:25:20Z
- **Completed:** 2026-09-15T14:49:30Z
- **Tasks:** 3
- **Files modified:** 6 (5 code/docs + 1 gate record)

## Accomplishments

- **TAGS-13 shipped:** `history_summary` pure extraction (core, 4 unit tests) + additive `history:` render block (one line per present field; absence byte-identical) + wiremock contract pins for full/partial/absent shapes + README field-set table citing 13-LIVE-CAPTURES.md.
- **TAGS-14 closed (closure branch):** README carries the working recipe (provision InternalHistorian via native REST → complete-node write with the 3 captured keys → write → query at 40–50 s scan cadence → corrected resources-API cleanup) and the stale 05-06 "documented limitation" paragraph was replaced with the falsifying evidence (`historicalProvider` was the typo).
- **The live gate passed on BOTH rigs on the identical final binary:** bind → TAGS-13 live render assert → written 44 proven in history, teardown verified (re-find 404) — recorded verbatim in 13-LIVE-GATE.md (SC-3 ✅ SC-4 ✅).
- **Harness bug fixed:** `delete_internal_historian` used a malformed find URL and treated non-200 as "already gone" — its DELETE had been a silent no-op all along. Now rides the corrected generic find route and verifies deletion.
- **Four live-truth gate hardenings, each live-motivated and separately committed** (deploy mount race heal; unique tag paths; self-healing bind cycles; bounded sweep polls + 30 s settles).

## Task Commits

Each task was committed atomically (Task 2/3 grew hardening commits from live findings):

1. **Task 1: TAGS-13 — history summary + additive render + contract pins** — `2a03133` (feat)
2. **Task 2: TAGS-14 closure branch — recipe + gate + harness fix** — `a0457e5` (feat); gate hardenings from live findings: `0ab99d6`, `513ee87`, `76be84b`, `65456a9` (fix)
3. **Task 3: Live gate on both rigs → 13-LIVE-GATE.md** — `ebc375b` (docs)

## Files Created/Modified

- `crates/ignition-core/src/actions/tags.rs` — `HistorySummary` + `history_summary(config) -> Option<HistorySummary>` (pure view; absent → None) + 4 unit tests
- `crates/ignition-cli/src/render.rs` — `render_tags_config_get_human` appends the additive `history:` block (present fields only)
- `crates/ignition-cli/tests/contract_tags.rs` — `tags_config_get_history_render_contract`: full field set / byte-identity absence pin / partial-shape honesty
- `crates/ignition-cli/tests/e2e_webdev.rs` — harness `delete_internal_historian` fix (corrected route + verified deletion); `live_tags_history_bindings` gate (unique names, sweep-verified deploy, 3 self-healing bind cycles, FINALLY cleanup); `ensure_ok`/`history_rows_contain` helpers
- `README.md` — Tag↔historian bindings section (field-set table + the working recipe + wire truths + provenance); Tag history (TAGS-08) paragraph updated to closure
- `.planning/phases/13-.../13-LIVE-GATE.md` — both-rig pass record, per-step verbatim output, incident trail, teardown-clean verification

## Decisions Made

- **Implemented the CLOSURE branch only** — the plan's either-branch structure resolved by 13-01's verdict; no limitation-path code exists.
- **`historical_group` field exists but stays `None`** — no captured key name exists (the gateway default group binds); documented in the struct rather than guessed.
- **Gate asserts the WRITTEN value (44), not merely non-null rows** — matches 13-01's data-level proof; the binding's initial-0 row cannot false-positive it.
- **Self-healing cycles over deadline bumps** — the 11-06 doctrine applied to a new failure class (deploy-tail bind orphaning), with the recovery (delete→settle→recreate) proven live on rig A's passing run.
- **Fresh-volume re-stage over diagnosing a wedged volume** — rig B's storage was dead-on-arrival after an interrupted first boot; `down -v` → healthy boot → gate passed first-try. Recorded as rig hygiene, not wire truth.
- **Re-spun rig A for the final record** — its first pass predated two hardening commits; both recorded runs now used the identical final binary (`65456a9`).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] e2e harness historian delete was a silent no-op (13-01's recorded follow-up)**
- **Found during:** Task 2 (harness work)
- **Issue:** `delete_internal_historian` used the provider-prefixed find URL that 404s on every rig, treating non-200 as "already gone" — the DELETE never executed
- **Fix:** corrected find route `/data/api/v1/resources/find/{module}/{type}/{name}`, signature DELETE, plus verified deletion (re-find → 404 assert)
- **Files modified:** crates/ignition-cli/tests/e2e_webdev.rs
- **Verification:** live on both rigs — "deleted + verified gone (find → 404)"
- **Committed in:** a0457e5

**2. [Rule 1 - Bug] Gate deploy could half-land (mount race)**
- **Found during:** Task 3, rig A run 1 (three routes landed, `tags` 405)
- **Fix:** sweep-verify after deploy + one heal redeploy (`0ab99d6`), then bounded 30 s sweep polling (`65456a9`)
- **Verification:** heal fired live on rig A final run + rig B fresh run; sweeps passed

**3. [Rule 1 - Bug] Same-name tag paths degrade across runs (11-06 class, tag paths)**
- **Found during:** Task 3, rig A runs 2–3 (recreated P13H/T1 never re-registered; a fresh path flowed immediately)
- **Fix:** unique tag path per run alongside the unique historian name (`513ee87`)
- **Verification:** fresh paths flowed on every subsequent run

**4. [Rule 2 - Missing Critical] Bind sequence needed self-healing (deploy-tail orphaning)**
- **Found during:** Task 3, rig A runs 2–4 (config reads back, writes Good, rows empty from birth when created inside the deploy import's model-rebuild tail)
- **Fix:** 3 self-healing cycles (settle → create → TAGS-13 assert → writes → bounded 75 s poll) with delete→settle→recreate recovery (`76be84b`); settle raised 15 s→30 s after rig B evidence (`65456a9`)
- **Verification:** rig A final run passed; the recovery cycle proven live on rig A's earlier pass (cycle 2 hit)

---

**Total deviations:** 4 auto-fixed (4× Rule 1-class gate/harness bugs, one carrying a Rule-2 hardening). **Impact on plan:** all fixes were gate-robustness and harness-correctness work inside Task 2/3's declared files — no scope creep, no product-code surface changes beyond the planned history view.

## Issues Encountered

- **Wedged first boot on rig B** (~35 min stall; then a dead historian storage engine persisting across restarts on that volume): resolved by `down -v` + fresh volume — the gate passed first-try (54.6 s). Documented in 13-LIVE-GATE.md §Incidents as rig hygiene: never trust a volume that survived an interrupted first boot.
- **Ambient profile hijack during manual diagnosis:** the user's default CLI config pins profile `uat` to :18188, so two manual probes intended for rig B actually hit rig A (one landed at rig A's trial-expiry boundary). All gate runs were unaffected (the gate pins an isolated config); rig A was re-spun fresh afterward anyway. Lesson recorded: manual multi-rig work must use isolated configs, never the ambient default profile.
- **Parallel-plan (13-03) compile breaks** twice blocked `cargo` runs on `workspace.rs` WIP; verification was rerun in an isolated worktree at HEAD (the 11-02 precedent) and after 13-03's commits landed.

## User Setup Required

None — no external service configuration required. Trial rigs are ephemeral; token secrets ride /tmp outside the repo (0600); both rigs torn down (0 containers, 0 volumes).

## Next Phase Readiness

- **SC-3 and SC-4 are closed** with both-rig live evidence recorded in-phase (13-LIVE-GATE.md).
- The gate is re-runnable: `IGNITION_LIVE_URL/IGNITION_LIVE_TOKEN/IGNITION_LIVE_MUTATIONS=1 cargo test -p ignition-cli --test e2e_webdev live_tags_history_bindings -- --ignored --nocapture` against any licensed trial rig; rig re-stage recipe in 13-RIG-NOTES.md + 13-LIVE-GATE.md §Ops.
- Phase 13 continues with 13-05…13-08 (workspace/edit slices — untouched by this plan).

---
*Phase: 13-composite-engine-workspace-historian-edit*
*Completed: 2026-09-15*

## Self-Check: PASSED

- All 6 key files exist on disk (verified with `[ -f ]`)
- All 7 commits verified in git log (2a03133, a0457e5, 0ab99d6, 513ee87, 76be84b, 65456a9, ebc375b)
- `historicalScanclass` absent from README (no-guessed-names rule)
- Workspace suites: 56 ok / 0 failed at close (parallel 13-03 commits included)
