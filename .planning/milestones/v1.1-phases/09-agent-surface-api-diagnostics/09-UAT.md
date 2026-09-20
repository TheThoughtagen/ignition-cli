---
status: complete
phase: 09-agent-surface-api-diagnostics
source: 09-01-SUMMARY.md, 09-03-SUMMARY.md, 09-04-SUMMARY.md, 09-05-SUMMARY.md, 09-06-SUMMARY.md, 09-07-SUMMARY.md, 09-08-SUMMARY.md
started: 2026-09-07T19:10:00Z
updated: 2026-09-08T11:30:07Z
---

## Current Test

[testing complete]

## Tests

### 1. ign api call — verbatim envelope
expected: Run `ign api call --method GET --path /data/api/v1/gateway-info` — envelope output where data.result.data is the gateway's own JSON verbatim (ignitionVersion/redundancyRole/edition keys as-is, nothing dropped), exit 0
result: pass

### 2. Auth-pattern header refusal
expected: Run `ign api call --method GET --path /data/api/v1/gateway-info --header "Authorization: Bearer fake"` — refused BEFORE any gateway request (exit 2, profile shows null, error names the refused auth header); no request ever leaves the machine
result: pass

### 3. Unclassified 4xx → exit 2 with verbatim body
expected: Trigger a 400-class gateway rejection — exit code is 2 (NOT 1), slug gateway_client_error, and the error shows the gateway's own verbatim response body, never an "internal error" storm. Note: original example (POST gateway-info) hit a classified 404 → exit 6, which is correct partition behavior; verified with POST /StatusPing (405 → exit 2 verbatim Jetty body) and POST api-token bad-body (400 → exit 2 verbatim body)
result: pass

### 4. license status — one command
expected: Run `ign license status` — one command returns live gateway license truth: trial mode/countdown line (or licensed mode) plus per-hardware-key item rows; exit 0
result: pass

### 5. redundancy status — one command
expected: Run `ign redundancy status` — one command returns the flat redundancy model (role, sync state, uptime etc.); uptime is milliseconds since gateway start; a never-synced node shows no bogus epoch date (-1 sentinel handled honestly); exit 0
result: pass

### 6. gan status — one command
expected: Run `ign gan status` — one command returns the 5-field gateway-area-network overview (including connection counts/rates); on a standalone gateway the zero-connection shape renders cleanly; exit 0
result: [pending]

### 7. diagnostics bundle generate + status
expected: Run `ign diagnostics bundle generate` — kicks off bundle generation and reports the state; running `ign diagnostics bundle status` immediately after shows the state (Generating, or Valid if fast) without any error about unknown state values
result: [pending]

### 8. diagnostics bundle wait
expected: Run `ign diagnostics bundle wait` — polls until the bundle reaches a terminal state and exits 0 once Valid; does not hang forever and does not falsely claim completion while still Generating
result: issue
reported: "error: gateway unreachable at diagnostics bundle generation — timed out after 300.016257917s; last observation: unknown state \"Invalid\" — still waiting; exit 4, took 5m 0s"
severity: major
note: "retest (happy path): generate→wait on a fresh bundle exits 0 in seconds ('bundle wait complete: Valid (73889 bytes)') — the gap is exclusively Invalid-state handling, not the polling machinery"

### 9. diagnostics bundle download — byte-faithful ZIP
expected: Run `ign diagnostics bundle download --output /tmp/uat-bundle.zip` — a file lands on disk that IS a valid ZIP (`file /tmp/uat-bundle.zip` reports Zip archive data, or unzip -l lists entries); no truncation, no size mismatch
result: pass
note: "full chain generate→wait→download exit 0; downloaded 73889 bytes == wait-observed fileSize; `file` reports Zip archive. Observation (gateway-side, not CLI): the same Valid bundle reported fileSize 64708 on one status poll and 73889 on others — the bundle file appears non-static while Valid; CLI reported faithfully at every hop"

### 10. TUI Dashboard rows for the new families
expected: Run `ign tui` — the new families reachable from the Dashboard actions menu (`a`): license status, redundancy status, gan status, diagnostics bundle generate/status/download/wait
result: issue
reported: "actions menu shows only the 15 v1.0 verbs (version, connections, waits, doctor, restart, backup, eam, script run, lint) — none of the Phase 9 verbs appear"
severity: major
note: "routes.rs rows EXIST (license/redundancy/gan/bundle mapped Screen::Dashboard, clap-walk green) but the menu is a hardcoded const ACTIONS [&str; 15] in state.rs that 09-04/09-05 never extended; update.rs executor arms also absent. Tab-indicator issue found during this test recorded as its own gap below"

---

## Re-verification Round (2026-09-08)
<!-- Gap closures 09-07 and 09-08 executed; this round live re-tests the failed/pending items only -->

### 11. [RE-TEST of #6] gan status
expected: Run `ign gan status` — one command returns the 5-field gateway-area-network overview (including connection counts/rates); on a standalone gateway the zero-connection shape renders cleanly; exit 0
result: pass

note: "setup interlude: uat profile had been repointed at the whk-services container (9088) — repointed uat→18188 (8.3.6 rig) / uat-b→19188 (8.3.3 rig) with token_env IGNITION_TOKEN_836/833 sourced from ~/.config/ignition-cli/rig-tokens.env; old 9088 setup preserved as whk-local profile. Aside observed (not a gap): missing token env var surfaces as gateway 401 rather than a local env-var-missing error — pre-existing v1.0 auth behavior, out of Phase 9 scope"

### 12. [RE-TEST of #8] diagnostics bundle wait — Invalid terminal state (09-07 fix)
expected: The old bundle from 2026-09-07 has long decayed to Invalid (steady state). Run `ign diagnostics bundle status` first (expect Invalid), then `ign diagnostics bundle wait` — wait must exit **6 bundle_not_available IMMEDIATELY** (seconds, not 300s), reporting the observed state Invalid and that a fresh generate is required. Old broken behavior: polls the full 300s deadline and calls a gateway that answered "gateway unreachable"
result: pass

note: "wait exited in 0.548s with 'no diagnostics bundle available (gateway reports state \"Invalid\") — run ign diagnostics bundle generate first; polling cannot change this state' + generate hint; status independently confirmed Invalid; exit code 6 pinned by Claude re-run (recorded evidence). No 'gateway unreachable' wording anywhere — the round-1 mislabel is gone"

### 13. [RE-TEST of #10] TUI Dashboard actions menu — Phase 9 verbs (09-08 fix)
expected: Run `ign tui`, press `a` on the Dashboard — the actions menu now includes the seven Phase 9 verbs alongside the 15 v1.0 ones: license status, redundancy status, gan status, diagnostics bundle generate, diagnostics bundle status, diagnostics bundle download, diagnostics bundle wait. Each new verb dispatches through the menu like the old ones do
result: pass

note: "screenshot evidence: actions menu shows all 22 verbs (15 v1.0 + the 7 Phase 9 verbs in clap-exact labels); gan status dispatched from the menu rendered the live JSON payload (totalConnections 0, runningConnections 0, byte rates 0.0, remoteGateways 0) in the scrollable viewer — dispatch chain works, not just menu presence"

## Summary

total: 10
passed: 7
issues: 2
pending: 1
skipped: 0

### Re-verification Round (2026-09-08)

total: 3
passed: 3
issues: 0
pending: 0
skipped: 0

**Round verdict:** both gap closures verified live — 09-07 (bundle wait Invalid terminal state) and 09-08 (Dashboard menu + dispatch) confirmed working on the 8.3.6 rig. No open gaps remain from this UAT except the tab-indicator item deliberately deferred to Phase 12 (below).

**Out-of-scope observations (2026-09-08, post-round):**
1. Tab indicator confirmed still bold-only in the user's TUI session — consistent with the OPEN-by-design deferral to Phase 12, not a regression.
2. `trial reset` (v1.0 rig verb, not a Phase 9 test) stopped at the designed ladder tail: tier-0 token POST refused 401 (endpoint expects the login rung) and no IGNITION_USER/IGNITION_PASSWORD in shell. Root-caused while triaging: the defensive tail (rig.rs:589) puts the rig URL in the error's `profile` slot — message-honesty wart recorded in STATE.md pending todos (minor, v1.0 code).

(note: 1 pending = test 10's OutOfBand completions sub-check folded into the pass/issue call above; tab-indicator finding recorded as its own gap, not a test slot)

## Gaps

<!-- Round-1 gaps: two FIXED + re-verified 2026-09-08; one OPEN (deferred Phase 12 by design) -->

- truth: "The new Phase 9 morning-check verbs are runnable from the TUI Dashboard"
  status: failed
  resolution: "FIXED by 09-08 (ACTIONS const + executor arms + routes↔menu parity CI) — re-verified pass 2026-09-08, re-test 13 (screenshot evidence)"
  reason: "User reported: actions menu shows only the 15 v1.0 verbs — none of the Phase 9 verbs appear"
  severity: major
  test: 10
  root_cause: "The Dashboard actions menu is a hardcoded const ACTIONS: [&str; 15] (crates/ignition-tui/src/state.rs:409) that update.rs's executor and the modal renderer key off. 09-04/09-05 added routes.rs CliRoute rows (satisfying the clap-walk coverage contract, tui_coverage green) but never extended ACTIONS nor added executor dispatch arms — registry rows without a reachable surface. Systemic blind spot: the coverage contract enforces routes↔clap parity but nothing enforces routes↔menu parity"
  artifacts:
    - path: "crates/ignition-tui/src/state.rs"
      issue: "ACTIONS const missing license status / redundancy status / gan status / diagnostics bundle generate|status|download|wait"
    - path: "crates/ignition-tui/src/update.rs"
      issue: "no executor dispatch arms for the seven new verbs (modal Enter does nothing for them)"
  missing:
    - "Extend ACTIONS const with the seven new verbs (license status, redundancy status, gan status, diagnostics bundle generate/status/download/wait)"
    - "Add update.rs executor arms dispatching each verb through Session::resolve like the CLI actions"
    - "Close the systemic blind spot: CI test enforcing every Screen-mapped route path appears in its screen's actions const (routes↔menu parity, same spirit as the clap walk)"
  debug_session: ""

- truth: "The active tab is visually unambiguous when switching screens"
  status: failed
  resolution: "OPEN by design — deferred to Phase 12 theming (TUIX-03/04); tracked in STATE.md pending todos. Not a Phase 9 regression; visual fix (Tabs::select + highlight_style + cursor hide) belongs to the token-palette work"
  reason: "User reported: the tab indicator doesn't change — the tab (content) changes but not the indicator"
  severity: minor
  test: 10
  root_cause: "render_tab_bar (crates/ignition-tui/src/ui/mod.rs) signals the active tab with BOLD ONLY — indistinguishable in the user's terminal font/theme — and the terminal cursor block sits parked on the first tab (Dashboard) at the bar origin, reading as a permanent selection highlight that never moves. Logic is correct (body and bold share state.screen; active_tab_is_bolded pins it) — the failure is purely visual. Fix belongs to Phase 12 theming (TUIX-03/04): Tabs::select(idx) + real highlight_style + hide cursor, not a Phase 9 regression (09 commits touched routes.rs rows only)"
  artifacts:
    - path: "crates/ignition-tui/src/ui/mod.rs"
      issue: "render_tab_bar: no Tabs::select, no highlight_style, cursor not hidden — bold-only indication + stray cursor block masquerades as a stuck indicator"
  missing:
    - "Phase 12 (theming): select the active tab index on the ratatui Tabs widget with a visible highlight_style from the token palette"
    - "Hide the terminal cursor in the TUI frame setup so it cannot park on the tab bar"
  debug_session: ""

- truth: "bundle wait reaches a terminal state and reports it honestly — does not hang forever on a failed generation"
  status: failed
  resolution: "FIXED by 09-07 (Invalid terminal steady state + immediate exit-6 bundle_not_available + honest deadline message) — re-verified pass 2026-09-08, re-test 12 (0.548s exit, exit code 6 pinned, no 'unreachable' mislabel)"
  reason: "User reported: wait timed out after 300s with 'gateway unreachable... unknown state Invalid — still waiting', exit 4 — the gateway had answered (bundle generation FAILED, state Invalid) but wait kept polling and the final message implied a network problem"
  severity: major
  test: 8
  root_cause: "BUNDLE_CAPTURED_STATES = [Generating, Valid] from 09-02 captures never observed Invalid (first live observation during this UAT). TTL-probe evidence: a Valid bundle decays to Invalid within ~2 minutes UNPROMPTED (probe: Valid at min4-5, Invalid at min6, passive status-only polling) and stays Invalid — Invalid is the gateway's steady-state 'no current bundle' answer, not a transient generation failure. Secondary wire quirk: the same Valid bundle reported different fileSize across polls (64708 vs 73889) — bundle file appears non-static while Valid. Therefore: (a) wait treating Invalid as pending-until-deadline is structurally futile — only a new generate changes it; (b) the deadline message ('gateway unreachable') mislabels an observed gateway answer as a network failure"
  artifacts:
    - path: "crates/ignition-core/src/client/diagnostics.rs"
      issue: "BUNDLE_GENERATING_STATES/BUNDLE_CAPTURED_STATES lack Invalid; is_generating/poll classification has no terminal-failure arm"
    - path: "crates/ignition-core/src/actions/diagnostics.rs"
      issue: "wait treats any non-generating unknown state as pending-until-deadline; deadline exit message says 'gateway unreachable' even when the last observation was a concrete gateway-reported state"
  missing:
    - "Add Invalid to the captured vocabulary with live provenance (observed 2026-09-07 UAT on 8.3.6 rig ign-p9-836: Valid decays to Invalid within ~minutes unprompted, probe log) as a TERMINAL steady state — the 'no current bundle' answer"
    - "wait: on observing Invalid, exit immediately (non-zero, bundle-not-available semantics) reporting the observed state and that a fresh generate is required — polling cannot change this state"
    - "Deadline message must distinguish 'no answer' from 'answered with observed state X' — never say unreachable when last_observation is Some"
  debug_session: ""

