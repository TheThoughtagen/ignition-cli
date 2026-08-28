---
status: complete
phase: 06-tui-cockpit
source: 06-01-SUMMARY.md, 06-02-SUMMARY.md, 06-03-SUMMARY.md, 06-04-SUMMARY.md, 06-05-SUMMARY.md, 06-06-SUMMARY.md
started: 2026-08-28T00:23:09Z
updated: 2026-08-28T02:25:00Z
---

## Current Test
<!-- OVERWRITE each test - shows where we are -->

[testing complete]

## Tests

### 1. TUI Launch & Quit
expected: With an active profile (live gateway), `ign tui` opens the full-screen cockpit: tab bar showing all six screens (Dashboard first, active tab highlighted bold), dashboard panels below. `q` (or Esc/Ctrl-C) quits back to the shell with exit 0 — terminal fully restored (no garbled output, cursor visible, prompt sane).
result: pass

### 2. Piped Stdout Refusal
expected: `ign tui | cat` (non-TTY stdout) refuses cleanly: exit 2, usage-class JSON error envelope on stderr, no alt-screen escape garbage in the pipe, no panic.
result: issue
reported: "error message correct + exit confirmed 2 by Claude re-run, but hint is misleading: 'fix the input source — a readable file path via --file, or `-` to pipe the content on stdin' is the generic resource-put hint, meaningless for ign tui"
severity: minor

### 3. Screen Navigation (Tab)
expected: Tab cycles forward through all six screens — Dashboard → Logs → Tags → Alarms → Projects → Rig → wraps to Dashboard — with the active tab bold/highlighted each step. Shift+Tab reverses.
result: pass

### 4. Dashboard Live Refresh & Degrade
expected: Status/modules/metrics/sessions panels populate within ~5s and refresh every 5s with zero keystrokes (freshness age visible). If a panel's endpoint fails, that panel shows an honest error while the others keep updating — never blank, never frozen.
result: issue
reported: "metrics panel shows: error / internal error: response from http://localhost:9088/data/api/v1/systemPerforman… (truncated)"
severity: major
note: degrade mechanics themselves worked (honest panel error, others refreshing) — failure is the metrics decode bug, gap logged separately

### 5. Dashboard Actions Menu & Result Modal
expected: `a` opens the actions menu (version, connections, wait gateway/restart/module, doctor, restart). Running `version` or `doctor` lands the result in a pretty-JSON modal scrollable via PgUp/PgDn; `restart` arms a Confirm modal first (y fires, Esc cancels and spawns nothing).
result: pass
note: menu opened and ran; restart Confirm gated correctly (user inspected modal, did not fire — gateway uptime confirms); UX roughness logged as gaps (labels, resize, vim motions)

### 6. Session Terminate
expected: On the dashboard sessions table, Up/Down selects a session; `t` (or Enter) opens a Confirm modal; `y` terminates it and the result modal confirms — the session disappears from the table on the next refresh.
result: skipped
reason: No terminatable session achievable on this rig: Designer-connected session prune -> gateway 409 (prune is for stale entries only, live Designer refused); Designer-embedded Perspective session -> 404 'no valid sessions found to close' (sessionScope=designer, not a valid terminate target); browser Perspective session unobtainable (module-resource layout for perspective general-properties unknown — see root-put gap). TUI mechanics VERIFIED via the honest-error path: selection -> Confirm gate -> y fires worker -> result modal relays the gateway's answer faithfully.

### 7. Profile Switcher
expected: `p` opens the profile switcher from any screen: existing profiles listed, selecting one switches (ProfileChanged banner appears, panels re-populate against the new gateway), and the add-form creates a new profile. Esc cancels without changing anything.
result: pass

### 8. Logs Tail, Filter & Scrollback
expected: Logs screen streams live gateway entries (level-coded colors). Changing the level filter applies immediately to already-retained lines (retroactive). PgUp breaks follow into scrollback; follow resumes at the newest. Tab away and back re-enters the tail without duplicate-flooding. `a` → loggers set/reset are Confirm-gated.
result: pass

### 9. Alarms Panel & Ack
expected: Alarms screen polls active alarms every 5s showing FULL 36-char UUIDs (never truncated at 80 cols). Ack form (`a` on a row) requires a username before Enter does anything; acking shows the result and refreshes the table promptly (not waiting for the 5s poll). History browse (24h) lands in the scrollable result modal.
result: skipped
reason: No configured alarms on this rig — ack loop not exercisable without building an alarm fixture; user opted to skip rather than fixture-quest (perspective fixture quest already consumed enough session)

### 10. Tags Browse & Live Watch
expected: Tags screen: provider list → Enter descends the tag tree (full-path rows); detail read shows value/quality/timestamp. Adding tags to watch opens the live table updating every ~2s. Esc ascends exactly one level at each depth (detail → tree → providers).
result: issue
reported: "tag isn't refreshing post reset"
severity: major

### 11. Tags Actions & Confirm Gates
expected: `a` on Tags opens the action menu (write, providers, config, export/import, UDT, history). Write modal accepts a JSON-scalar value. Provider delete, config delete, and import-overwrite arm Confirm modals; Esc cancels spawning nothing. `?` in a rich form shows the exact CLI synopsis.
result: pass

### 12. Projects Drill-Down & Actions
expected: Projects screen: list → Enter → detail (record fields + resources, degrading independently) → Enter → resource content preview (JSON pretty / raw text, scrollable). `a` menu carries all 11 verbs. Project delete, import-overwrite, resource put, resource delete are Confirm-gated; webdev deploy fires directly (ungated).
result: pass
note: user flags accumulated UX improvement desires (beyond test criteria) — captured in UX Feedback section

### 13. Rig Screen & Confirm Parity
expected: Tab to Rig: status summary renders (a DOWN rig shows as data, not an error). `l` opens the raw compose-logs pane streaming rig lines. `a` menu lists all nine rig verbs — reset/restore/trial-reset arm Confirm modals; down fires directly without a modal (volumes preserved).
result: issue
reported: "rig status doesn't show well. Otherwise pass"
severity: minor

## Summary

total: 13
passed: 7
issues: 5
pending: 0
skipped: 2

## Gaps

- truth: "Tags screen reflects the gateway's current state (recovers after transient conditions like trial-expired 402)"
  status: failed
  reason: "User reported: trial reset but the TUI is stale for tags — browse pane still shows the 402 webdev-unlicensed error earned while the trial was expired; screenshot shows 'tags — [default]' pane with 'browse error / the WebDev module is unlicensed on this gateway (HTTP 402 — trial-expired rigs cannot serve /system/webdev routes)'"
  severity: major
  test: 10
  root_cause: "One-shot browse results persist in the Tags navigation stack with no invalidation path: set_screen(Tags) re-fires only spawn_providers_once (the root provider list), never the in-stack browse pane; the Tags keymap (Up/Down/Enter/w/a) has NO refresh key; the 402 was honest when earned (trial-expired) but persists after the condition cleared — recovery requires non-obvious Esc+Enter re-navigation"
  artifacts:
    - path: "crates/ignition-tui/src/update.rs"
      issue: "set_screen Tags arm fires spawn_providers_once + spawn_tag_watch only; tags_keys has no refresh binding"
    - path: "crates/ignition-tui/src/workers/watch.rs"
      issue: "browse one-shot has no re-trigger path from the error state"
  missing:
    - "Add 'r' refresh on Tags screen re-firing the current-level read (provider list at root, current browse inside the stack)"
    - "OR re-fire the deepest visible one-shot on screen entry"
    - "AND/OR error panes should hint the recovery path (re-navigate to refresh)"
  debug_session: ""
  verified_gateway_healthy: "post-reset CLI check: RUNNING, trial 7080s, all 4 routes present, tags read round-trips — error state is stale, not live"

- truth: "Piped-stdout refusal hint is relevant to the TTY problem"
  status: failed
  reason: "User reported hint text: 'fix the input source — a readable file path via --file, or - to pipe the content on stdin' — the generic invalid_input hint from resource put, meaningless for ign tui; exit 2 + envelope + message all verified correct"
  severity: minor
  test: 2
  root_cause: "TTY guard rides CoreError::InvalidInput whose hint string is baked into the variant (06-01: no dedicated Usage variant in the frozen taxonomy) — the taxonomy forbids a new variant but the hint is a per-render string that can be contextual"
  artifacts:
    - path: "crates/ignition-cli/src/main.rs"
      issue: "Tui arm raises InvalidInput with the generic variant hint"
    - path: "crates/ignition-core/src/error.rs"
      issue: "InvalidInput variant carries the fixed --file/stdin hint text"
  missing:
    - "Contextual hint for the TUI TTY refusal (e.g. 'run ign tui in an interactive terminal') without adding a taxonomy variant — hint override at the raise site or a hint-parameterized constructor"
  debug_session: ""

- truth: "Metrics (gauges/charts/threads) render on 8.3.3 gateways"
  status: failed
  reason: "User reported: metrics panel shows internal error from /data/api/v1/systemPerformance (screenshot paste, truncated); CLI reproduces: 'internal error: response from http://localhost:9088/data/api/v1/systemPerformance/currentGauges did not match the expected shape: error decoding response body'"
  severity: major
  test: 4
  root_cause: "CurrentGauges model pins heap_memory/max_memory as i64 (from the 8.3.6 research-rig capture where both arrived integer-form); this 8.3.3 build (b2026012009) serializes the heap gauge as a Java double in SCIENTIFIC NOTATION — raw wire verified: {\"cpu\":1.27...,\"heapMemory\":2.85746728E8,\"maxMemory\":1073741824} — and serde_json refuses exponent/decimal forms for i64, failing decode. Charts (Datapoint.value already f64 — 2508 exponent-form values in the live body parse fine) and threads (plain integers) are safe; ONLY CurrentGauges breaks. Cross-surface: kills the TUI metrics panel AND the ign metrics CLI on 8.3.3."
  artifacts:
    - path: "crates/ignition-core/src/client/metrics.rs"
      issue: "CurrentGauges.heap_memory: i64 and max_memory: i64 reject exponent-form doubles (lines ~34-38)"
  missing:
    - "Change heap_memory/max_memory to f64 (byte counts <= ~9e15 are exact in f64) and update display formatting in actions/metrics + TUI dashboard panel render"
    - "Add wiremock fixture with exponent-form heapMemory (2.85746728E8) pinning the decode"
    - "Audit sibling models for integer-typed gauge fields that could arrive exponent-form on other gateway builds"
  debug_session: ""
  verified_wire: "curl raw body captured 2026-08-28: cpu=1.2755618546264424, heapMemory=2.85746728E8, maxMemory=1073741824"

- truth: "Action-menu modals show their footer hint (Enter to run · Esc to cancel)"
  status: failed
  reason: "Code-arithmetic verified during Test 5 investigation: Modal::Actions height = ACTIONS.len()+3 capped .min(11) = 10 rows, but content needs 11 (7 entries + blank + hint + 2 borders) — the 'Enter to run · Esc to cancel' line clips; LogsActions has the same off-by-one (3+3=6 vs 7 needed)"
  severity: cosmetic
  test: 5
  root_cause: "Height formulas in ui/mod.rs modal geometry: Actions uses .min(11) sized for 8 entries (ACTIONS has 7) and both formulas undercount by one row (content lines + borders vs len+3)"
  artifacts:
    - path: "crates/ignition-tui/src/ui/mod.rs"
      issue: "Modal::Actions => ACTIONS.len()+3.min(11) and Modal::LogsActions => LOG_ACTIONS.len()+3 — each one row short of content+borders"
  missing:
    - "Fix height formulas to entries + hint + blank + 2 borders (len + 4), or drop the separate min caps"
  debug_session: ""

- truth: "Menu labels read clearly as actions (wait-prefix labels confuse)"
  status: failed
  reason: "User reported: 'wait is a weird word to use for a comment to restart a gateway' — the CLI-parity labels 'wait gateway/wait restart/wait module' read as odd action prose in a menu; 'wait restart' scans like a restart variant"
  severity: minor
  test: 5
  root_cause: "ACTIONS const uses CLI-verb-parity labels verbatim (routes.rs needs clap spellings, but the MENU labels are display-side per the LOG_ACTIONS precedent: 'Labels are display-side; the route rows carry the clap-exact spellings')"
  artifacts:
    - path: "crates/ignition-tui/src/state.rs"
      issue: "ACTIONS labels are clap spellings, not display prose"
  missing:
    - "Display labels like 'wait for gateway up' / 'wait for restart complete' / 'wait for module ready' while routes.rs keeps clap-exact rows"
  debug_session: ""

- truth: "Modals resize dynamically with the terminal"
  status: failed
  reason: "User reported: 'this window needs to dynamically resize' — modals are fixed Ratio(1,2) x Length(height): clipped on small terminals, small on large ones"
  severity: minor
  test: 5
  root_cause: "ui/mod.rs modal geometry uses fixed Ratio(1,2)+Length caps with no clamp to frame area and no growth"
  artifacts:
    - path: "crates/ignition-tui/src/ui/mod.rs"
      issue: "centered(Ratio(1,2), Length(h)) fixed geometry for all modal kinds"
  missing:
    - "Clamp modal height to frame height (min(content, frame-2)) and/or let content-driven heights grow within terminal bounds"
  debug_session: ""

- truth: "Popups support full vim motions"
  status: failed
  reason: "User reported: 'Any popup like this should have full vim motions' — Actions menu navigates arrows-only (no j/k); Result modal has PgUp/PgDn but no Ctrl-d/Ctrl-u/g/G; screens have j/k but modals do not"
  severity: minor
  test: 5
  root_cause: "Modal key handling in update.rs: Actions modal matches KeyCode::Up/Down only (line ~2568); Result_ modal matches PageUp/PageDown only"
  artifacts:
    - path: "crates/ignition-tui/src/update.rs"
      issue: "modal nav arms lack j/k/g/G/Ctrl-d/Ctrl-u"
  missing:
    - "j/k + g/G + Ctrl-d/Ctrl-u (half-page) in every menu/list/result modal, matching the screen-level keymaps"
  debug_session: ""

- truth: "resource put writes root-level project files (e.g. perspective-properties.json)"
  status: failed
  reason: "During Test 6 fixture prep: ign resource put pterm views/root/view.json succeeded (nested member), but perspective-properties.json (project-root member) failed — HTTP 500 from projects/import with gateway log: 'Project import failed: module folder must have folder flag set' (IllegalArgumentException, gateway.ProjectRoutes)"
  severity: major
  test: 6
  root_cause: "Zip-surgery path for root-level file members: nested puts ride the existing parent-folder descriptor chain from the export zip, but a project-root file has no such chain — the synthesized descriptor/entry structure hits the gateway's module-folder flag validation (exact surgery mechanism TBD in debug)"
  artifacts:
    - path: "crates/ignition-core/src/client/resources.rs"
      issue: "member-surgery helpers: root-level file member path produces an import the gateway rejects with 'module folder must have folder flag set'"
  missing:
    - "Reproduce with wiremock/unit test; fix the synthesized structure for root-level members (folder flag or descriptor shape)"
  debug_session: ""
  repro: "ign resource put pterm perspective-properties.json --file <any json> — 500 + gateway log IllegalArgumentException"
  layout_matrix: "Manual surgery experiments on pterm.zip (import overwrite, live-store verified): A) com.inductiveautomation.perspective/resources/general-properties.json + synthesized resource.json -> 500 'resource already exists: resourcePath=com.inductiveautomation.perspective/resources' (implicit node collision); A2) same file bare, no descriptor -> import success, file SILENTLY not adopted (05-07 basename rule); B) module root + descriptor -> 500; C) project-root perspective-properties.json + root resource.json -> import success, silently no-ops. Working precedent: nested views/root/view.json + folder descriptor lands fine. PerspectiveProjectProps (jar-cache strings): module=com.inductiveautomation.perspective, RESOURCE_TYPE='general-properties', fields=updateMode(Notify)/updateMessage/updateTimeout/locale/timezone/desktopPageTimeoutSeconds/mobilePageTimeoutSeconds/hideFromLaunchListings/thumbnailPath/sessionClosedMessage/pageClosedMessage/loggedOutMessage"

- truth: "Designer-prune refusal (409) surfaces as target-state with actionable hint, not internal"
  status: failed
  reason: "User's TUI terminate (perspective row) showed 'resource not found on the gateway'; CLI designer prune against live session: 'internal error: unexpected HTTP 409 Conflict' exit 1 with 'internal errors are bugs' hint — 409 means prune-refused-for-live-designer (prune = stale entries only), a target-state condition"
  severity: minor
  test: 6
  root_cause: "classify() has no 409 mapping — designer-prune Conflict falls through to CoreError::Internal (exit 1) instead of a target-state exit 6 slug; Perspective terminate 404 'no valid sessions found to close' similarly could distinguish id-vs-scope mismatch"
  artifacts:
    - path: "crates/ignition-core/src/client/mod.rs"
      issue: "classify() status mapping lacks 409 handling for the designer prune route"
  missing:
    - "Map designer-prune 409 to an additive exit-6 slug (e.g. session_not_prunable) with a 'close the Designer first — prune removes stale entries only' hint"
    - "Consider distinguishing perspective-terminate 404 (no valid sessions) from generic not_found"
  debug_session: ""
  verified_wire: "raw curl: DELETE /data/api/v1/designer/{id} -> 409 empty body (GET same path 200); DELETE /data/perspective/api/v1/sessions?sessionId=<id> -> 404 per openapi 'No valid sessions found to close'; both sessions Designer-embedded (sessionScope=designer, userAgent=<designer>, shared id 10443A91)"

- truth: "Detail pane reflects a just-written value without manual refire"
  status: failed
  reason: "User reported 'tags aren't writing' then confirmed modal said success + detail did not change; CLI read verified 1234 LANDED (fresh timestamp) — the write path is correct, the displayed value is stale. Alarms screen has an ActionDone->re-poll trigger ('alarms ack'); tags write has no equivalent refresh of the open detail/read"
  severity: minor
  test: 11
  root_cause: "No ActionDone label trigger for 'tags write' in update.rs (the alarms ack trigger is the established pattern, test-pinned); the detail pane's read is purely on-demand (open + Enter refire)"
  artifacts:
    - path: "crates/ignition-tui/src/update.rs"
      issue: "ActionDone arm lacks a tags-write -> refire_detail_read (and/or watch table nudge) trigger"
  missing:
    - "On 'tags write' ActionDone success: refire the open detail read when the written path matches, mirroring the alarms ack-refresh trigger pattern"
  debug_session: ""
  verified: "TUI write of 1234 -> result modal 'success' -> CLI read shows 1234 [Good] fresh timestamp — write works, display stale"

## UX Feedback (captured verbatim — triage at completion)

- "Everything monochrome. No color or other indications."
- "vim motions everywhere, including vim motions and a better editor experience in the projects and tags tab."
- "better ux in popups with a lot of text."
- "Put and other things don't make a ton of sense"
- "the resource put is confusing. i dont get delete vs resource delete.

- truth: "Rig status summary renders readably"
  status: failed
  reason: "User reported: 'rig status doesn't show well' (display-quality issue; specifics TBD — possibly layout/truncation/readability, ties into the monochrome UX feedback)"
  severity: minor
  test: 13
  root_cause: ""
  artifacts:
    - path: "crates/ignition-tui/src/ui/rig.rs"
      issue: "status summary pane rendering (exact defect TBD at diagnosis)"
  missing: []
  debug_session: ""

- truth: "Destructive verb labels distinguish scope (project delete vs resource delete)"
  status: failed
  reason: "User UX feedback: 'the resource put is confusing. i dont get delete vs resource delete' — the Projects actions menu mixes project-scoped and resource-scoped verbs in one flat list with CLI jargon labels ('put', 'delete')"
  severity: minor
  test: 12
  root_cause: "PROJECT_ACTIONS is one flat 11-entry menu with clap-derived labels; no noun-grouping (Project: / Resource: / WebDev:) or human-readable descriptions; 'put' is CLI jargon for create-or-replace"
  artifacts:
    - path: "crates/ignition-tui/src/state.rs"
      issue: "PROJECT_ACTIONS flat list, CLI-parity labels"
  missing:
    - "Group the Projects menu by noun (project/resource/webdev sections) with human labels + one-line consequence descriptions"
  debug_session: ""
