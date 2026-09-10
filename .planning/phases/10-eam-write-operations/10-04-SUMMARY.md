---
phase: 10-eam-write-operations
plan: 04
subsystem: cli
tags: [eam, blast-radius, guard, clap, tui, wiremock, snapbox, readme]

# Dependency graph
requires:
  - phase: 10-eam-write-operations (10-03)
    provides: lifecycle actions (suspend/resume/cancel with authoritative re-checks), full-record RMW modify + signature-keyed delete, BlastRadiusPreview composer (build_blast_radius / compose_blast_radius / render_preview_line), lifecycle_precheck
provides:
  - Five guarded `ign eam task` verbs (suspend/resume/cancel/modify/delete) with the two-tier blast-radius guard (pure precheck → preview fetch → confirm-with-preview → action)
  - Force composed onto the same preview gate (refusal message IS the blast-radius line)
  - ActionOutput::EamTaskLifecycle/Modify/Delete + human render arms in all three render modes
  - TUI routes/PendingActions for all five verbs with Confirm modal body = blast-radius preview (EamPreview worker gate) and parity pins at 36 rows / 27 verbs / 23-verb confirm tripwire
  - README contract: command-table rows, two-tier guard walkthrough, agent-vs-task reconciliation note, stale deferred-claim correction
affects: [10-05 live gates, phase-11 tags, phase-14 MCP catalog (new clap leaves ride the catalog)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Two-tier guarded-verb dispatch: pure Tier-0 precheck (zero network) → resolve → preview fetch (bad name refuses not_found BEFORE any prompt) → require_confirmation whose operation string IS render_preview_line output → action with core's authoritative re-checks"
    - "TUI preview-gate: staged PendingAction rides a read-only spawn_eam_preview worker; only the EamPreview event arms the gate and opens Confirm with the preview body; a failed fetch arms NOTHING"
    - "Refusal-as-blast-radius: the CLI refusal message names task + agents + impact so agents read the blast radius from stderr alone (exit 2 pre-write, zero mutations — request-counting golden proves exactly 3 GETs)"

key-files:
  created: []
  modified:
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-core/src/actions/eam.rs
    - crates/ignition-cli/tests/contract_eam.rs
    - crates/ignition-core/tests/eam_contract.rs
    - crates/ignition-tui/src/event.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-tui/src/state.rs
    - crates/ignition-tui/src/ui/mod.rs
    - crates/ignition-tui/src/update.rs
    - crates/ignition-tui/src/workers/mod.rs
    - crates/ignition-tui/src/workers/ops.rs
    - crates/ignition-cli/tests/tui_coverage.rs
    - README.md

key-decisions:
  - "preview_then_confirm helper in main.rs folds the Tier-2 gate (build_blast_radius → render_preview_line → require_confirmation) so every guarded verb + force composes the identical refusal shape"
  - "EamTaskLifecycle is ONE ActionOutput variant keyed by the result's own action string (the lifecycle verbs share EamLifecycleResult); modify/delete get their own variants matching the result-model granularity"
  - "TUI eam task modify: ONE targeted change per cockpit fire (modal-depth decision) — multi-flag modifies stay CLI-only; raw K=V rides to fire time keeping PendingAction Eq"
  - "README reconciliation note: no agent-level suspend/resume on the wire — tasks are the unit, affected agents ride targetGateways in the preview; agent-level delete/approve/upgrade deliberately unexposed v1.1"
  - "Recovery decision: prior executor died mid-plan with all work uncommitted — reconciled the full working tree against the plan (Tasks 1-2 complete, Task 3 README missing), verified per task, committed per task boundary (files split cleanly across tasks)"

patterns-established:
  - "Preview-in-refusal: every guarded EAM write refusal embeds the render_preview_line output — golden-pinned with request counting (refusal traffic = reads only)"
  - "TUI gate-after-fetch: Confirm modals for blast-radius verbs open only AFTER the preview read lands; the fetch failure path opens the error modal and never arms a gate"

# Metrics
duration: 3h 43m (recovery session; incl. ~15 min build + ~40 min full suite under heavy host load)
completed: 2026-09-10
---

# Phase 10 Plan 04: CLI + TUI surface for guarded EAM task verbs Summary

**Five guarded `ign eam task` verbs (suspend/resume/cancel/modify/delete) with the two-tier blast-radius guard whose refusal message IS the preview, force composed onto the same gate, TUI preview-gated Confirm modals with parity at 36/27/23, and the README contract incl. the no-agent-level-suspend reconciliation.**

## Performance

- **Duration:** 3h 43m (recovery session — see Deviations; wall time dominated by a heavily loaded host: workspace build 14m46s, full test suite ~50 min, lean build cached)
- **Started:** 2026-09-10T02:52:15Z (recovery start; prior executor began earlier and died uncommitted)
- **Completed:** 2026-09-10T06:35:28Z
- **Tasks:** 3 of 3
- **Files modified:** 15 (14 from the prior executor's uncommitted tree + README written this session)

## Accomplishments

- Five new CLI verbs run the two-tier guard: Tier-0 pure precheck (exit 2, zero network), preview fetch (bad name = exit 6 `not_found` BEFORE any prompt), confirm gate whose refusal message embeds the blast-radius preview line (task + agents + impact + pending), then the action with core's authoritative re-checks — zero writes fire without `--yes` (golden proves exactly 3 GETs on refusal)
- `force` composes onto the same preview gate (EAMW-04) — its refusal now also names the blast radius
- Modify's Tier-0 rejects malformed `--setting` K=V and no-target-keys modifies pre-network; `--rename` deliberately absent (wire answers PUT-rename 404 — create-new + delete-old composite documented in README)
- TUI: five Dashboard routes + menu entries; Confirm modal body IS the preview (render_preview_line + agents/pending fact lines) fetched BEFORE the gate arms; a failed fetch opens the error modal and arms nothing; Session-typed dispatch only
- Parity pins moved atomically: 36 Dashboard rows / 27 menu verbs / 23-verb confirm-parity tripwire (the TUI Confirm set exactly equals the CLI `--yes` set)
- README: command-table rows for all five verbs, the two-tier guard walkthrough, THE reconciliation note (no agent-level suspend/resume — tasks are the unit; agent-level writes exist on the wire but are unexposed v1.1), stale deferred-claim corrected

## Task Commits

1. **Task 1: CLI verbs + two-tier guard dispatch + force preview composition** - `caaeb27` (feat)
2. **Task 2: TUI surface — routes rows, PendingAction variants, Confirm body = preview, parity pins** - `efe0ad0` (feat)
3. **Task 3: README agent contract + reconciliation note + full verification pass** - `e208f83` (docs)

## Files Created/Modified

- `crates/ignition-cli/src/cli.rs` — EamTaskCommand::Suspend/Resume/Cancel/Modify/Delete with controller-gate-honesty doc-comments
- `crates/ignition-cli/src/main.rs` — two-tier dispatch arms, `preview_then_confirm` gate, ActionOutput variants
- `crates/ignition-cli/src/render.rs` — human renderers: lifecycle read-back proofs, modify changed-keys + new signature, delete affected names
- `crates/ignition-core/src/actions/eam.rs` — formatting pass (clippy/rustfmt alignment; actions unchanged — built in 10-03)
- `crates/ignition-cli/tests/contract_eam.rs` — goldens: suspend refusal (snapbox-pinned preview message, profile envelope, 3-GET zero-mutation proof), suspend success (JSON + human, stateful persistence), delete pair, modify RMW + flag refusals, resume/cancel envelopes
- `crates/ignition-core/tests/eam_contract.rs` — formatting pass (fmt from the prior executor's tree-wide `cargo fmt`)
- `crates/ignition-tui/src/event.rs` — EamPreview event (era + staged action + preview result)
- `crates/ignition-tui/src/routes.rs` — five CliRoute rows (Screen::Dashboard)
- `crates/ignition-tui/src/state.rs` — PendingAction variants + EamTaskModifyChange + input-modal states + ACTIONS registry 22→27
- `crates/ignition-tui/src/update.rs` — EamPreview handler (gate arming), modify-line grammar, execute/fire arms, gated_cli_verb titles, parity tripwire at 23, grammar + preview-flow tests
- `crates/ignition-tui/src/workers/mod.rs` — spawn_eam_preview + eam_preview_body (read-only gate fetch)
- `crates/ignition-tui/src/workers/ops.rs` — fire_eam_task_lifecycle / fire_eam_task_modify (confirmed arms, Session-typed)
- `crates/ignition-tui/src/ui/mod.rs` — Actions-menu fit assertion 80x30→80x35 (27 entries)
- `crates/ignition-cli/tests/tui_coverage.rs` — menu-registry parity pin 31→36 rows / 22→27 verbs
- `README.md` — EAM write-lifecycle contract (Task 3)

## Decisions Made

- `preview_then_confirm` helper keeps the two-tier gate ONE function so all six guarded verbs compose identically (see key-decisions)
- One `EamTaskLifecycle` ActionOutput variant (keyed by the result's action string) rather than three — matches the shared EamLifecycleResult shape; modify/delete get distinct variants per their result models
- TUI modify = one targeted change per fire (modal-depth decision); raw K=V string rides to fire time so PendingAction stays Eq and parse_setting remains the single typing authority
- README reconciliation note locks the agent-vs-task story: no agent-level suspend/resume exists; affected agents ride targetGateways; agent-level delete/approve/upgrade noted as existing-but-unexposed (deferred scope, nothing planned)
- Recovery decision (this session): reconciled the prior executor's uncommitted tree against the plan rather than reimplementing — Tasks 1-2 were complete on disk, Task 3's README was missing and was written fresh

## Deviations from Plan

### Process Recovery (not a code deviation)

**Prior executor outage — second in this phase (10-02 had the same failure mode)**
- **Found during:** recovery session start
- **Issue:** the prior executor implemented Tasks 1-2 (and tree-wide `cargo fmt`) but died from an API connection failure BEFORE any verification, commit, SUMMARY, or STATE update; the working tree held +1824/-171 uncommitted; Task 3's README was never touched
- **Fix:** reconciled the diff task-by-task against the plan (all Task 1/2 surface present incl. goldens + tests), ran the full verification ladder incrementally (build → contract_eam → eam_contract → tui_coverage → tui lib → fmt → clippy → full workspace suite → lean build), then committed at task boundaries (files split cleanly across the three tasks — no interleaving)
- **Verification:** all green — 17 CLI goldens, 38 core seam tests, 201 TUI lib tests, 53 total workspace test binaries with zero failures, clippy `-D warnings` clean, fmt clean, lean build clean
- **Committed in:** caaeb27 / efe0ad0 / e208f83

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Stale git index.lock from the crashed session**
- **Found during:** Task 1 commit
- **Issue:** `.git/index.lock` left by the killed prior session blocked all commits
- **Fix:** verified no git process was running, removed the stale lock
- **Files modified:** none (repo housekeeping)
- **Verification:** commits proceeded normally
- **Committed in:** caaeb27 (unblocked the Task 1 commit)

**2. [Rule 3 - Blocking] Hung `docker version` probe wedged the full-suite run (environment, not code)**
- **Found during:** Task 3 full-workspace verification
- **Issue:** `run_streaming_forwards_lines_via_piped_stdout` (rig compose, untouched by this plan) hung >15 min because its Docker-daemon probe child never returned — the host's Docker daemon was unresponsive
- **Fix:** killed the hung probe child — the test's own design (nonzero probe → quiet skip) took over; no code change
- **Files modified:** none
- **Verification:** the test skipped exactly as designed; suite resumed and finished EXIT=0
- **Committed in:** n/a (test-run intervention only)

---

**Total deviations:** 2 auto-fixed (2 blocking) + 1 process recovery (executor outage, prior-session work adopted rather than reimplemented)
**Impact on plan:** No scope creep — the adopted tree matched the plan's surface exactly; both auto-fixes were unblocking housekeeping. All plan verification criteria met.

## Issues Encountered

- Host was under extreme load for the first half of the session (load avg 26-57; workspace cold build 14m46s, first test-binary compile passes slow) — mitigated with the incremental file-output test strategy (one `--test` binary per detached command, logs to /tmp) per the phase's environment notes; load dropped to ~4 late in the session and the remainder ran fast
- The full-suite rig/docker test hang documented as deviation 2 (environment)
- Untracked non-plan files in the repo root (a .gwbk backup, a zip, uattest.json) were left alone — not plan surface

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- 10-05 (live gates) is the only remaining plan in Phase 10: the guarded verbs are mock-proven end-to-end and ready for live-gateway verification against the controller-configured rig
- The capture-locked behaviors (sync `isSuspended` persistence, cancel no-op honesty, signature-keyed delete) are now golden-pinned at the binary level — 10-05's live gates assert against pinned expectations
- Parity pins (36/27/23) are the new baseline; any future guarded verb must move all three in the same change (the tripwires enforce it)
- Watch item: the `readme_exit_table_agreement` contract remains green untouched (no new exit slugs landed — as the plan predicted)

---
*Phase: 10-eam-write-operations*
*Completed: 2026-09-10*

## Self-Check: PASSED

All key-files verified on disk; all three task commits (caaeb27, efe0ad0, e208f83) verified in git log; full workspace suite EXIT=0 (53 binaries, 0 failures); clippy -D warnings clean; fmt clean; lean build clean.
