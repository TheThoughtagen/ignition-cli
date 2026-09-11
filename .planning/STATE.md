# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-09-04)

**Core value:** One binary that lets a developer (or an AI agent) fully operate and inspect an Ignition 8.3+ gateway — health, projects, tags, rigs — without opening the gateway webpage or Designer.
**Current Focus:** Milestone v1.1 Agent Surface & IDE Integration — Phase 10 EAM write operations: plans 01-05 + 07 executed (captures, client surface, action layer, CLI/TUI/README surface, live gate, UAT-test-10 modal-clip gap closure); 10-06 (live gate lifecycle) in flight with a parallel executor; SC-5 live run blocked on user-provisioned WHK controller env (10-USER-SETUP.md)

## Current Position

**Phase:** 10 of 14 (10-eam-write-operations)
**Current Plan:** 6 of 7 complete (01-05 + gap closure 10-07 — TUI Confirm modal wraps the blast-radius body, UAT test 10 pinned by buffer regression test); 10-06 in flight (parallel executor, code committed at 9f99ed5, SUMMARY pending)
**Total Plans in Phase:** 7 (5 original + 2 gap closures from 10-UAT)
**Status:** 10-07 complete — awaiting 10-06 SUMMARY, then `/gsd-verify-work` 10
**Last Activity:** 2026-09-11

**Progress:** [██████████] 98%

## Performance Metrics

**v1.0 baseline (for comparison):** 41 plans, 118 tasks, 9 days (2026-08-20 → 2026-08-29); avg ~38 min/plan; slowest plans were live-gate/WebDev phases (P03-P04 of Phase 5 at ~400+ min).

**v1.1 velocity:** Phase 8 complete (6/6 plans); Phase 9 complete (8/8 plans incl. 2 gap closures from the UAT, re-verified passed 2026-09-08) — next: `/gsd-verify-work` live re-test of UAT test 8, then Phase 10 planning.

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 8 | 1/6 | 200 min | 200 min |
| 9 | 0/TBD | - | - |
| 10 | 0/TBD | - | - |
| 11 | 0/TBD | - | - |
| 12 | 0/TBD | - | - |
| 13 | 0/TBD | - | - |
| 14 | 0/TBD | - | - |

*Updated after each plan completion*
| Phase 08 P02 | 200 min | 2 tasks | 3 files |
| Phase 08 P01 | 4h 58min | 3 tasks | 9 files |
| Phase 08 P03 | 1h 40min | 2 tasks | 2 files |
| Phase 08 P04 | 128 min | 2 tasks | 3 files |
| Phase 08 P06 | 189 min | 3 tasks | 5 files |
| Phase 08 P05 | 194 min | 3 tasks | 6 files |
| Phase 09 P02 | 62 min | 2 tasks | 2 files |
| Phase 09 P01 | 210min | 2 tasks | 5 files |
| Phase 09 P03 | 175min | 3 tasks | 20 files |
| Phase 09 P04 | 168min | 3 tasks | 22 files |
| Phase 09 P05 | 314 min | 3 tasks | 10 files |
| Phase 09 P06 | 131 min | 2 tasks | 2 files |
| Phase 09 P08 | 138 min | 2 tasks | 5 files |
| Phase 09 P07 | 167 min | 3 tasks | 9 files |
| Phase 10 P01 | 24 min | 2 tasks | 2 files |
| Phase 10 P02 | ~175 min (incl. infra outage) | 2 tasks | 16 files |
| Phase 10 P03 | 3h 23m | 3 tasks | 2 files |
| Phase 10 P04 | 3h 43m | 3 tasks | 15 files |
| Phase 10 P05 | 2h 30m | 2 tasks | 3 files |
| Phase 10 P07 | 24 min | 2 tasks | 1 files |

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [Roadmap]: Phase numbering continues v1.0 (8-14); research layer-2 split into four phases (9-12) — 15 reqs in one phase exceeded standard-depth manageability
- [Roadmap]: Transports (Phase 14) deliberately last — MCP catalog derives from the clap tree, so every command family must land first
- [Roadmap]: Historian binding (Phase 13) is spike-gated — licensed-Historian rig access must be confirmed BEFORE Phase 13 planning
- [Roadmap]: TUIX-05 fully delivered in Phase 8 (config plumbing + worker parameterization + clamp); TUIX-03/04 (rendering) in Phase 12
- [Phase 08]: Session seam is concrete-with-deref (Arc<ReqwestGatewayApi>, not Arc<dyn>) — dyn-widening deferred to Phase 14 where MCP needs dyn — TUI workers/ClientHandle are concretely typed; Phase 8 goal is construction-site unification, not handle-type churn
- [Phase 08]: Session::resolve takes the EFFECTIVE profile flag — IGNITION_PROFILE folding stays in the bin's apply_env_defaults (one env-to-flag home) — Seam must mirror main.rs resolve_profile_context verbatim; re-reading env in core would fork the precedence rule
- [Phase 08]: poll_interval_too_small is its own enum variant riding the config exit class (exit 3) — Phase-7 additive-slug mechanism; serde accepts 0, load-time validation refuses, load_for_tui degrades — keeps code() total, slug first-class in the envelope for agents, and the strict/degrading split is exactly what 08-04's TUI wiring needs
- [Phase 08]: New config keys follow the lenient-degradation pattern: deserialize_with warn+default on wrong types, warn-list entry, skip_serializing_if at defaults — legacy configs round-trip byte-identically — a typo in a NEW key must never fail the load; legacy on-disk shape and contract goldens stay frozen
- [Phase 08]: Plan 08-03: Session gained profile_url/credential_present accessors + resolve_side constructor — doctor's raw-URL/presence contract and the diff/sync no-re-overlay golden contract cannot hold through resolve/resolve_degraded alone — Byte-identical mandate; goldens pinned sides as overlay-immune
- [Phase 08]: Plan 08-03: resolve_profile_context survives reduced to selection-only (profile list view + two-client envelope echo) — envelope must not demand the active profile's secret and list must tolerate fresh installs — Plan's 'otherwise' clause; non-construction consumers keep main.rs selection-only
- [Phase 08]: TUI load pattern: config::load_for_tui then Session::resolve_loaded — caller owns the load policy, seam owns overlay/selection/LOCKED chain; Session::resolve_loaded added to core (returns session + selected POST-OVERLAY profile) — Session::resolve loads strict config internally, physically incompatible with the TUI degradation contract; constructor extension beats keeping a duplicated selection choreography in the TUI
- [Phase 08]: TUI rig helpers return Option<Arc<ReqwestGatewayApi>> via Session::for_url — Session hands out Arc handles (client not Clone); call sites deref with &*api / as_deref — Zero second-source construction in the TUI requires going through the seam; Arc is also the shape 08-05's ResolvedContext wants
- [Phase 08]: Three-Place slug rule now executable: readme_exit_table_agreement parses README exit table via include_str! (section-scoped) and cross-checks against literal (exit,slug) table both directions — nothing later mutates the frozen contract by accident
- [Phase 08]: mcp/lsp/edit pre-declared as reserved OutOfBand slugs (taxonomy + justification only, zero rows); rows land TOGETHER with their clap commands in P13/14 — orphan registry rows fail the clap walk by design; pinned test is the pre-declaration
- [Phase 08]: stdout purity harness is assert-based byte-exact over the real binary (NOT snapbox goldens) so SNAPSHOTS=overwrite cannot sanitize a leaked byte; ambient IGNITION_* env knobs stripped for determinism — single stray stdout byte under max diagnostics must fail CI, never be rewritten
- [Phase 08]: Plan 08-05: ResolvedContext struct replaces the positional (String, String, Arc) triple from resolve/rebuild — poll_interval rides a typed field so the profile-switch chain cannot silently drop it (the update.rs:545-587 trap)
- [Phase 08]: Plan 08-05: 5s poll default has ONE source (workers::refresh::REFRESH_PERIOD, imported by context.rs); only the dashboard refresh worker is parameterized — WATCH/ALARMS/TAIL periods + TICK parked for Phase 12; CI pins plumbing assertions (Duration values per hop), wall-clock cadence is checkpoint-only
- [Phase 09]: Bundle states captured PascalCase: Generating->Valid; BUNDLE_GENERATING_STATES=["Generating"]; unobserved states must passthrough — Live capture on both rigs; lowercase guesses would have shipped wrong (Pitfall 2)
- [Phase 09]: Units locked from captures: uptime=ms-since-gateway-start (wall-clock proven twice); lastSyncTimestamp=-1 never-synced sentinel (unit not capture-proven, model Option, ms flagged inference); fileSize=bytes, key absent until Valid — Magnitude cross-checks against wall clock; -1 sentinel rules out epoch units on fresh rigs
- [Phase 09]: Live 4xx partition: 404+HTML=unknown path, 404+EMPTY=wrong method on real path (NOT 405), 401+HTML=bad auth — DELETE /gateway-info answered 404-empty on both rigs - the plan's 405 hypothesis falsified by capture; evidence for 09-06 gates
- [Phase 09]: 8.3 headless commissioning rides the commissioner wire API (bootstrap -> eula-accept -> start-gateway); 8.3 image entrypoint ignores ACCEPT_EULA/GATEWAY_ADMIN_PASSWORD — Env vars are 8.1-era; wire recipe extracted from commissioner.js and replayed with curl on both rigs
- [Phase 09]: GatewayClientError rides exit 2 with slug gateway_client_error carrying the verbatim 4 KiB-capped gateway body — additive-slug on the frozen taxonomy, Three-Place rule landed atomically (README row + both CI agreement tests)
- [Phase 09]: The api-call catch-all is parameter-scoped (api_call: bool on classify, set only by pub send_and_classify_for_api) — curated pipeline 4xx/exit-1 semantics provably unchanged via pinned non-leak regression
- [Phase 09]: Gateway-verbatim = RawValue passthrough: capture text, from_string validates + embeds in ONE call (no parse-re-serialize); non-JSON 2xx is the honest exit-1 refusal naming the download pipelines
- [Phase 09]: api-call guards run PRE-resolve (auth-pattern header refusal + path validation: exit 2, envelope profile null, ZERO requests — binary-pinned against a bare mock) and the action re-checks for in-process callers
- [Phase 09]: api call maps OutOfBand in routes.rs (raw passthrough is not a cockpit verb — the envelope IS the product, completions genre); pinned OutOfBand set extended to [completions, api call] in the same task as the clap command
- [Phase 09]: User-supplied --header strings validated via HeaderName/HeaderValue::from_bytes before reqwest (whose .header() panics) — bad headers refuse exit 2, never crash; host-shaped --path check precedes the slash check (protocol-relative // refused explicitly)
- [Phase 09]: license action merges the trial companion — /licenses carries NO mode/edition keys on the captures (capture wins over the plan's field sketch); mode + countdown ride TrialWire — Live-capture ground truth over plan field sketch
- [Phase 09]: redundancy lastSyncTimestamp: -1 sentinel (never synced) normalized to None via last_sync_epoch_ms(); ms unit flagged INFERENCE in model docs — never presented as capture-proven — No sync ever happened on the fresh rigs; sibling epoch-ms pattern is the only evidence
- [Phase 09]: 09-04 wire models: partial-curated + flatten passthrough; role stays String (unknown future roles ride); GAN byte rates f64 (parse int+float wire forms); non-hardware license arrays stay Vec<Value> element passthrough — Version-tolerance directive; element shapes not capture-proven on fresh rigs
- [Phase 09]: 09-05: bundle state vocabulary stays String consts (BUNDLE_GENERATING_STATES=[Generating], BUNDLE_CAPTURED_STATES=[Generating,Valid]) with per-element rig provenance — never an enum (Pitfall 2); fileSize=Option<u64> bytes (capture Decision 2 over plan's i64 sketch) — Capture-locked vocabulary; unobserved states must ride passthrough and wait honestly
- [Phase 09]: 09-05: bundle download rides download_to_file (the ONE streaming site) with BUNDLE_DOWNLOAD_TIMEOUT=300s pinned by unit test at birth — no second hand-built request site, no sleep-based wiremock — Pitfall 8: the 30s client default would truncate MB-sized bundles; the parameter ride preserves the one-streaming-site invariant
- [Phase 09]: 09-05: bundle wait = captured non-generating terminal / Generating pending / UNKNOWN pending-until-deadline, exit 4 network_error (no new slug); envelope data IS the wire (data.state) for generate/status/wait — Honest unknowns keep polling with the final status on the deadline observation; data.state matches the contract-test spec
- [Phase 09]: 09-06 live gates: DELETE probe asserts the captured 404-empty answer (exit 6 not_found) — the 405/exit-2 hypothesis stays falsified; classify.rs maps every 404 before the api-call catch-all — Capture wins over plan hypothesis; gate encodes live truth
- [Phase 09]: 09-06 first-time capture: gateway-info body rides ignitionVersion (not version) — identical key shape on 8.3.3/8.3.6; wiremock mocks serve only the alias name — Wire truth recorded; gate spot-key corrected mid-run
- [Phase 09]: 09-08: the seven Phase 9 menu labels are clap-exact (menu_label seam stays minimal to the 06-10 wait trio); parity CI resolves labels through the seam in both directions
- [Phase 09]: 09-08: Dashboard routes↔menu parity is CI-enforced via a pinned 31-row count + bidirectional menu_label resolution — route-without-menu and menu-without-route both fail in the same change
- [Phase 09]: 09-08: bundle wait deadline is the 300s clap Wait default, deliberately NOT BUNDLE_DOWNLOAD_TIMEOUT (per-request download override ≠ poll deadline)
- [Phase 09]: 09-07: Invalid is a captured TERMINAL steady state (UAT TTL-probe truth — Valid decays to Invalid within ~2 min unprompted and stays); BUNDLE_UNAVAILABLE_STATES + is_bundle_unavailable; wait exits IMMEDIATELY exit 6 bundle_not_available (2-probe pin) — only a fresh generate changes the state — Capture-locked vocabulary extended with live 2026-09-07 provenance; polling a steady state is structurally futile
- [Phase 09]: 09-07: CoreError::Network gained observation: Option<String> — poll deadline() populates it; Display leads 'no terminal state' when the gateway answered (never 'unreachable' for an observed answer) and preserves 'gateway unreachable' byte-for-byte when None; exit 4/network_error unchanged — UAT Gap 3: the deadline message mislabeled an answered gateway as a network failure; dedicated field keeps transport-error wording untouched
- [Phase 09]: 09-07: bundle_not_available landed Three-Place ATOMIC in one commit (exit enum + literal (exit,slug) table + README exit-6 row) — readme_exit_table_agreement include_str!'s the README so the third place cannot lag a commit — Phase 08 pin makes the Three-Place rule executable; splitting the README row across tasks would commit a red suite
- [Phase 10]: Phase 10 capture decisions live in 10-LIVE-CAPTURES.md Decisions-locked-by-captures (12 items); headline: suspend/resume runtime verbs SYNC config.profile.isSuspended (bidirectional, both rigs) — Capture-first plan 10-01: later plans must cite, not re-derive, wire truth
- [Phase 10]: Signature mismatch (PUT+DELETE) = HTTP 500 + JSON problem{message,stacktrace}; classify on stable substring 'signature mismatch' — message prose + stack frames drift between 8.3.3/8.3.6 — Live capture falsified the 400/409 hypotheses; 8.3.3 message leaks the live signature
- [Phase 10]: Rename via PUT is NOT supported (404 empty, no create); delete succeeds WITHOUT ?confirm= for lone resources and ?collection= takes the COLLECTION name (core), not the type; runtime-verb unknown-task = 500-HTML (suspend/resume) or silent 204 (cancel), never 404 — All three plan hypotheses corrected by capture; locked for 10-02/10-03/10-04

- [Phase 10 / 10-02]: Zero new classify arms for the write verbs — runtime verbs ride the existing path-scoped controller-403/not_found arms, PROVEN per-verb by contract test (not assumed); 403-without-message stays Auth on the new paths
- [Phase 10 / 10-02]: Signature-mismatch 500 (captured) is a module-doc FINDING for the action layer — 10-03/10-04 classify on the stable 'signature mismatch' substring; the client stays classification-free
- [Phase 10 / 10-02]: eam_task_delete always sends collection=core; ?confirm= rides only on opt-in (capture Decision 3); modify PUT body is the full single-element array INCLUDING config.settings + original signature — 422-trap pinned at REQUEST level
- [Phase 10 / 10-02]: Contract tests are REQUEST-pinning (recorded request: method+path+query+body), not response-only — expect(1) + .mount() guard discipline; fixtures verbatim from 10-LIVE-CAPTURES.md
- [Phase 10]: 10-03: lifecycle re-checks are pure fns shared three ways (lifecycle_precheck / suspend_recheck / cancel_decision) — suspend refuses already-suspended exit 2 pre-write; resume fires unconditionally (§1b silent 204); cancel mirrors the gateway (no-pending no-op fired:false, canCancel=false reported)
- [Phase 10]: 10-03: modify is full-record RMW with rename+suspend_flag OMITTED from TaskChange per capture honesty (§5 rename-PUT=404; PUT-driven isSuspended unproven) — the create-new+delete-old composite is 10-04's documented workflow; delete derives signature from find, no-confirm by default with ONE confirm=true retry on the demand shape (Decision 3)
- [Phase 10]: 10-03: signature-mismatch 500s classify on EVIDENCE (post-failure find shows a changed signature ⇒ exit 2 re-run guidance) — the substring never reaches the action layer (classify's Internal fallback drops non-HTML bodies); zero new slugs/variants; BlastRadiusPreview composes find + BOTH scheduled segments (never short-circuited — Running rows live only in true) and force rides it (EAMW-04)
- [Phase 10]: 10-04: preview_then_confirm folds the two-tier gate — build_blast_radius → render_preview_line → require_confirmation — so all six guarded EAM verbs (incl. force) compose the identical refusal whose message IS the blast radius (golden-pinned: refusal traffic = exactly 3 GETs, zero mutations) — One gate function means the refusal shape cannot drift per verb; agents read the blast radius from stderr alone
- [Phase 10]: 10-04: TUI preview-gate — Confirm modals for the five EAM verbs arm ONLY after the read-only spawn_eam_preview fetch lands (EamPreview event carries the staged PendingAction); failed fetch opens the error modal and arms nothing; modify = ONE targeted change per cockpit fire (raw K=V rides to fire time, PendingAction stays Eq) — The TUI Confirm body IS the preview — same blast radius as the CLI refusal; gate-after-fetch means a bad name never reaches a gate
- [Phase 10]: 10-04: README reconciliation — no agent-level suspend/resume exists on the EAM wire (tasks are the unit; affected agents ride targetGateways in the preview); agent-level delete/approve/upgrade exist but are deliberately unexposed in v1.1; modify has NO --rename (PUT-rename=404 — create-new + delete-old composite is the documented workflow) — Wire honesty: the README is the agent contract and must state what the wire does NOT offer
- [Phase 10]: 10-05: SC-5 recorded as blocked-on-env — the live gate exists (compiles, clippy-clean, green-skip) but WHK controller access (IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN full name:key) is user-provisioned; 10-USER-SETUP.md + 10-LIVE-GATE.md §6 carry the unblock contract; no disposable-rig substitution permitted — roadmap names the WHK controller rig specifically; 10-01 ladder precedent: creds absent from environment => stop and record
- [Phase 10]: 10-05: the live gate flips its own scratch task to Scheduled+cron before suspend — capture §1a proves OnDemand suspend = 500 and §1c proves ~80s trigger-registration latency — retried 7x/30s on the Jetty message; pre-write name assertion (find + assert scratch name) runs before EVERY write, enforced in test code — captures are the locked wire truth; plan verify text defers to 10-LIVE-CAPTURES Decision 1
- [Phase 10]: 10-07: Confirm body split into real Lines before wrapping — ratatui renders embedded \n inside a Span as literal whitespace glyphs, not row breaks (reflow.rs); .wrap() alone would fold the three body lines into one flow with visible newline glyphs and row-split tokens — the height calc already counted body.lines(), proving multi-line intent
- [Phase 10]: 10-07: wrapped_row_count greedy estimator targets the Ratio(1,2) inner width (frame/2 − 2); over-estimate is the safe side under content-driven-height doctrine; the 80x24 buffer regression test is the arbiter if ratatui's WordWrapper disagrees

### Pending Todos

- [Phase 12 prerequisite — from 09 UAT test 10, minor]: TUI tab indicator is visually ambiguous — `render_tab_bar` (crates/ignition-tui/src/ui/mod.rs) signals the active tab with BOLD ONLY and the terminal cursor block parks on the tab bar reading as a stuck highlight. Logic is correct; the fix is visual/theming: `Tabs::select(idx)` + a visible `highlight_style` from the token palette + hide the terminal cursor in frame setup. Assigned to Phase 12 (TUIX-03/04) per the 09 UAT diagnosis — NOT a Phase 9 regression. Details: 09-UAT.md Gap 2.
- [Polish, v1.0 code — from 09 UAT re-verification round, minor]: `trial_reset`'s defensive tail (crates/ignition-core/src/actions/rig.rs:589) stuffs the rig URL into `CoreError::SecretUnavailable`'s `profile` slot, so the TUI modal renders `secret unavailable for profile "http://localhost:…"` — the profile NAME and the missing-credential path (IGNITION_USER/IGNITION_PASSWORD) should be named instead. Candidate to ride along with any later error-message/UX pass.

### Blockers/Concerns

- [Phase 13 prerequisite]: Confirm licensed-Historian rig access before starting Phase 13 planning — spike cannot proceed without it (documented-limitation fallback is legitimate, but access confirmation must happen first)
- [Phase 11 prerequisite]: Real multi-level UDT export needed for the derive-vs-Event-loop decision — requires a live rig during Phase 11 planning
- SC-5 (Phase 10) awaiting user-provisioned WHK controller env: IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN (EAM-rights token, full name:key) — then: cargo test -p ignition-core --test live_gateway live_eam_write_lifecycle -- --ignored --nocapture; append outcomes to 10-LIVE-GATE.md §5 (see 10-USER-SETUP.md)

## Session Continuity

**Last session:** 2026-09-11T11:49:54.036Z
**Resume file:** None
