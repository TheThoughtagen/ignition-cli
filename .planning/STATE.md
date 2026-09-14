# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-09-04)

**Core value:** One binary that lets a developer (or an AI agent) fully operate and inspect an Ignition 8.3+ gateway — health, projects, tags, rigs — without opening the gateway webpage or Designer.
**Current Focus:** Milestone v1.1 Agent Surface & IDE Integration — Phase 12 TUI theming & degradation COMPLETE (verifier passed 3/3 success criteria — 12-VERIFICATION.md; named themes via [ui].theme live across all screens, authored 4-tier degradation palettes, tokenization CI grep negative-proven; two UAT-driven tuning rounds: palette hardening 756e888 + dormant-slot wiring, then body-content tint 27df248 — dark = blue-chromed cockpit, WCAG-AAA body contrast, default/mono byte-identical pre/post); next: Phase 13 (Historian rig access confirmation REQUIRED before planning). SC-5 (Phase 10) follow-up still open — capture scheduled/false post-suspend vanish behavior (fresh + long-lived rigs), re-size/re-shape the §2 check, gate re-run

## Current Position

**Phase:** 12 of 14 (12-tui-theming-degradation) — COMPLETE, verified passed 3/3
**Current Plan:** 4
**Total Plans in Phase:** 4 (12-01, 12-02, 12-03, 12-04 ALL DONE incl. 3-round human-verify checkpoint)
**Status:** Phase 12 verified passed (3/3 success criteria, 12-VERIFICATION.md) — user approved tuned themes 2026-09-14; ready for Phase 13 planning (after Historian rig access confirmation)
**Last Activity:** 2026-09-14

**Progress:** [██████████] 100%

## Performance Metrics

**v1.0 baseline (for comparison):** 41 plans, 118 tasks, 9 days (2026-08-20 → 2026-08-29); avg ~38 min/plan; slowest plans were live-gate/WebDev phases (P03-P04 of Phase 5 at ~400+ min).

**v1.1 velocity:** Phase 8 complete (6/6 plans); Phase 9 complete (8/8 incl. 2 gap closures); Phase 10 complete (7/7 incl. 2 gap closures); Phase 11 complete (6/6, verified passed 20/20 — capture-first, both-rig live gates, 8 live-truth deltas fixed in-phase) — next: Phase 12 planning.

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 8 | 1/6 | 200 min | 200 min |
| 9 | 0/TBD | - | - |
| 10 | 0/TBD | - | - |
| 11 | 6/6 | 334 min (excl. 11-03) | 67 min avg |
| 12 | 1/4 | 8 min | 8 min |
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
| Phase 10 P06 | 27 min | 3 tasks | 4 files |
| Phase 11 P01 | 56 min | 3 tasks | 4 files |
| Phase 11 P02 | 25min | 2 tasks | 9 files |
| Phase 11 P03 | 20h 32m (overnight gap) | 2 tasks | 7 files |
| Phase 11 P04 | 49 min | 3 tasks | 9 files |
| Phase 11 P05 | 33 min | 3 tasks | 9 files |
| Phase 11 P06 | 171 min | 3 tasks | 4 files |
| Phase 11 P07 | 12 min | 2 tasks | 3 files |
| Phase 11 P08 | 12 min | 2 tasks | 5 files |
| Phase 12 P01 | 8 min | 2 tasks | 2 files |
| Phase 12 P01 | 8 min | 2 tasks | 2 files |
| Phase 12 P02 | 14 min | 2 tasks | 5 files |
| Phase 12 P03 | 18 min | 3 tasks | 7 files |
| Phase 12 P04 | 50 min | 2 tasks | 2 files |

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
- [Phase 10]: 10-06: gate code stays EXACTLY as-run after the failed live runs — §5 verbatim records must match the committed binary; wording corrections live in the docs and the follow-up gap plan amends wording with its own re-run — Evidence-provenance over cosmetic retrofits
- [Phase 10]: 10-06: SC-5 live runs on the disposable controller rig both aborted at §2 — grace row persisted >90s twice, contradicting the fresh-rig <48s upper bound; retry budget spent, NO deadline bump — follow-up: capture scheduled/false post-suspend vanish behavior across fresh + long-lived rigs before re-sizing (gate §4 D1/§6) — Re-sizing without understanding the reconcile mechanics is guesswork, forbidden by the failing-step contract
- [Phase 11 / 11-01]: The fidelity oracle is THREE-tier (binds 11-06): transport sha256 always; byte-identity ONLY for unchanged-subtree re-export (3× byte-stable, both rigs); import→re-export rides ORDER-NORMALIZED structural identity because sibling order permutes on model rebuild (1128==1128 length, byte-different, structurally identical — captured pair dcf1c0aa/2d99ed58) — Byte-identity after an import is invalid by capture, not by tooling
- [Phase 11 / 11-01]: importTags REFUSES UDT type definitions (verbatim: "Udt definitions can only be imported in the UDT Definitions tab", both rigs) — UdtType-bearing XML cannot round-trip through importTags at all; loss-scan must surface type="UdtType"; instances DO import (UDTParentType → typeId, children ride from the type) **[CORRECTED 2026-09-14, 11-08]**: the refusal is FOLDER-basePath-only ([default]P11Roundtrip capture condition); the CLI's provider-root import (tags.rs:2210) accepts UdtType definitions and routes them to [provider]_types_/Name — live-proven ×3 on 8.3.6 + the UAT import itself (.planning/debug/udt-type-fact-falsified.md)
- [Phase 11 / 11-01]: exportTags kwargs+xml returns the full CRLF XML document string (NO <?xml declaration, 3-space indent, trailing CRLF) — positional form = filePath-writing fallback (gateway cwd-relative); mixed-parent export SILENTLY corrupts (type="Unknown" empty Tag) and PRE-EXISTS in the JSON baseline — mixed-parent refusal is a both-formats concern
- [Phase 11 / 11-01]: importTags collision 'a' NEVER throws — failures ride Bad_Failure("Tag 'X' already exists…") QualityCode elements; provider-root basePath WORKS from script threads (07-06 RpcContext constraint is getConfiguration/exportTags-specific) — route/action layer must inspect the QualityCode list, not catch exceptions
- [Phase 11 / 11-01]: Legacy CSV grammar locked by jar evidence (TagCSVImporter.PROP_COLUMNS = 51 names, identical both rigs): Path/Owner columns UNUSABLE (folder-branch NPE / Bad_Unsupported), CSV basePath IGNORED (tags land at provider root), alarms (AlarmStates) and Permissions parse Good but silently land NOTHING, unknown columns silently ignored, marker row required + version bounds-checked — docs' 47/48-column table is neither the vocabulary nor required
- [Phase 11 / 11-01]: scriptExec exec action runs code in a FRESH globals dict — `import system` is the bridge (first live proof of exec semantics; prior gates only hit `version`); applies to all later scriptExec probe/gate use
- [Phase 11 / 11-01]: Captured XML artifacts are committed `-text` via .gitattributes — core.autocrlf=input silently CRLF→LF-normalized the fixture blob on first commit (caught by sha mismatch); byte-faithful captures must assert blob sha after commit
- [Phase 11]: [Phase 11 / 11-02]: exportTags format=xml rides the kwargs+xml form Probe-1-proven on both rigs with the documented positional temp-file fallback retained defensively; importTagsFile has NO provider-root pre-flight (Probe 2: importTags is RpcContext-free, script-thread truth) with the No-RpcContext catch kept as honest WebDev-thread defense; XML crosses the envelope as payload_b64 base64 (CRLF document byte-exact); collisionPolicy LOCKED a/o with invalid_collision_policy refusal — 11-02 route design cites 11-LIVE-CAPTURES.md verbatim; WebDev-thread truth deferred to 11-06 live gate
- [Phase 11]: [Phase 11 / 11-02]: ROUTE_BUNDLE_VERSION 1.1.0 -> 1.2.0 landed ATOMICALLY in one seven-file commit (five route constants + mod.rs + routes/VERSION), MIN_CLI stays 1.0 — the three-way pin test is the enforcement; wiremock REQUEST pins for the new bodies landed at the raw webdev_route_call layer before their action-layer consumers exist (11-04) — Research Pattern 4: bump atomicity machine-enforced; pins precede consumers
- [Phase 11]: 11-03: TAGS-12 loss scans are pure advisory fns (scan_xml/scan_csv) that NEVER refuse — partial parses set partial=true + xml_parse_partial fact and report what was readable; refusals are the gateway's job — Planner advisory-posture lock; TDD'd over the real 11-01 UDT capture (8 Tags, P11UDT+MotorType top-level, alarms detected) — roadmap real-export validation flag closed
- [Phase 11]: 11-03: quick-xml 0.41 default features ONLY (serde-derive rejected, rationale in Cargo.toml); csv_no_alarms + csv_legacy_columns_only are UNCONDITIONAL facts for non-empty CSV scans; xml_udt_type_definition fact surfaces the capture-proven 'importTags refuses type definitions, lands nothing' for 11-04/11-05/11-06; codes are stable pub consts (codes module) — Report contract for the 11-04 import pre-check, 11-05 loss gate, and 11-06 oracle exclusions (types_seen UdtType) **[CORRECTED 2026-09-14, 11-08]**: the quoted 'refuses type definitions, lands nothing' claim is folder-basePath-only — provider-root imports land definitions in _types_ (see the 11-01 line correction above)
- [Phase 11]: 11-04: timeout override rides a DEFAULT trait method (webdev_route_call_with_timeout delegating to webdev_route_call) — only GatewayClient overrides; 17 test doubles inherit untouched — minimal blast radius on the 17-impl GatewayApi surface
- [Phase 11]: 11-04: the transfer seam is base64-only (decode payload_b64 out / encode file_b64 in) — grep-provable: zero XML/CSV Readers in tags.rs; the scan REPLACES a re-parse for names/tallies/facts — the byte-faithfulness invariant, planner lock
- [Phase 11]: 11-04: CAPTURE-WINS CSV generation — Path cells ALWAYS empty (non-empty Path NPEs the importer, probe 5); folder flattening + TagType-13 UDT-type placeholders REPORTED as coercions; TagsImportResult.failed records Bad_Failure elements verbatim (element-not-exception semantics) — plan reconciliation clause + probe-2d honesty
- [Phase 11]: TAGS-12 loss gate is ONE function in dispatch (loss_gate): pre-resolution scan, the prose report IS the exit-2 invalid_input refusal (profile null, zero requests), --yes attaches data.loss_report; imports only — csv generation warns-and-continues — 10-04 preview_then_confirm lesson: one gate means the refusal shape cannot drift
- [Phase 11]: data.loss_report is ONE additive envelope key with two shapes (import scan: facts+top_level_names; csv generation: dropped_keys+coerced+rows) on a LossReport struct skip-serialized at None — pre-11-05 envelopes byte-identical — envelope lock mandates the key; additive-only frozen contract
- [Phase 11]: Mixed-parent export gets NO gate — Probe 4 proves silent Unknown-corruption PRE-EXISTS in the JSON interchange (not a hard failure, not new-format); xml/csv stdout mode writes RAW bytes in every render mode (fourth exception extended), human summary on stderr — planner lock's own no-check branch + capture-first honesty; pipes stay pure
- [Phase 11]: 11-06: the fidelity oracle held live on both rigs (transport sha equality; order-normalized structural identity after import) — one Rig B run recorded a byte-identical pair, so capture (b)'s sibling permutation is probabilistic, not guaranteed — SC-1 closed on recorded live evidence
- [Phase 11]: 11-06: importTags silently drops UdtInstance parameter overrides when the referenced type is unresolvable in the TARGET provider (8.3.3; 8.3.6 preserves; overwrite re-import does not heal) — the fidelity gate transfers MotorType via the config surface — A UDT-instance transfer without its type definition is not faithful
- [Phase 11]: 11-06: 8.3.x async-mounting tolerances are bounded and measurement-backed (servlet first-activation 240s worst; import cleanPath 30s; provider resolution 30s; alarm registration 60s/cycle; landing verification after every import; unique provider names per run) — No blind deadline bumps; every window carries its measurement
- [Phase 11]: 11-06: generated CSV fills an absent dataType with Int4 (2) and reports it; the legacy-default sheet is PER-COLUMN (header-present columns materialize, absent ones do not) — probe-5's universal sheet claim corrected — Live-truth corrections to the interchange, gate + captures doc updated together
- [Phase 11]: 11-06 ops: image trial = 2h — budget a full live-suite run inside one window; commissioning a fresh rig costs ~4 min; provisioning script transcription typos surface as gateway 400 MalformedJson — Fifth rig-generation proof; recorded for the next live phase
- [Phase 11]: UDT fact correction (11-08): xml_udt_type_definition fires unconditionally with corrected scoped text — provider-root imports DO land UdtType definitions in [provider]_types_/Name; the verbatim refusal is folder-basePath-only; corrections dated across README/STATE/captures, originals preserved
- [Phase 11]: 11-07: loss-gate hint override rides a PREFIX sentinel (LOSS_GATE_REFUSAL_REASON_PREFIX = the render_loss_prose header literal) content-addressed in hint()'s InvalidInput arm — the 06-07 TTY exact-match pattern adapted for dynamic prose; same slug invalid_input, same exit 2, envelope shape + message prose byte-identical (hints are not slugs — not a Three-Place event); contract_tags drift-guard pins fail if the prose header and the sentinel ever diverge; the ~66 other InvalidInput sites keep the generic hint (deferred design question per the debug session) — UAT test 3: the refusal's trailing hint must match the failure; constructor variant rejected as extra API surface when the precedent already covers it
- [Phase 12]: 12-01: mono adaptation centralized in style helpers keyed on slot==Reset as the mono marker — the palette IS the resolved tier, so no second Tier parameter can drift; error→BOLD, selection→REVERSED at mono, everything else plain fg
- [Phase 12]: 12-01: degradation is AUTHORED not derived — four themes × four explicit tiers, Rgb confined to truecolor palettes, c16 = ANSI-16 names (+ intentional Reset in default), mono all-Reset at EVERY tier (tier-independent pin for 12-02's ambient-COLORTERM wiring tests)
- [Phase 12]: 12-01: detect_tier is pure over &dyn Fn(&str)->Option<String> — signature pinned so 12-02 wires it as detect_tier(&|k| std::env::var(k).ok()); priority NO_COLOR non-empty > COLORTERM truecolor|24bit > TERM *256color* > TERM missing/dumb→Mono > C16
- [Phase 12]: 12-01: palette HUES are planner-discretion and ADJUSTABLE POST-UAT — tests pin structure only (slot equality, containment, all-Reset mono), never literal colors
- [Phase 12]: 12-02: resolve_palette is private and pure over (name, tier) — build_context is the ONLY real-env detect_tier site; tests pin the helper at fixed tiers — honors 12-01's env-snapshot injection signature; keeps the wiring helper fully testable without ambient-env coupling
- [Phase 12]: 12-02: unknown [ui].theme warns via tracing::warn! and falls back to default at the same tier; wrong-TYPED values degrade earlier via the existing lenient_ui deserializer — the cockpit always starts — Phase-8 lenient-degradation contract decided at TUI-resolution time; tracing added as a direct ignition-tui dep (Rule 3)
- [Phase 12]: 12-02: AppState.palette defaults to default-theme @ Mono (all-Reset, no env reads at construction); run_loop + switch_profile adopt ctx.palette in the same block/group as poll_interval — the 08-05 ride-along pattern closes the silent-drop trap class — uniform adoption is the 08-05 lesson; deterministic construction keeps ~100 AppState::new() test callsites signature-stable
- [Phase 12]: 12-03: CI tokenization grep landed ATOMICALLY with the final literal migration (state.rs/context.rs test-code literals included in the same commit) — the grep goes red the moment it's added — grep-atomicity mandate; plan undercounted the tree's literals
- [Phase 12]: 12-03: all-Reset pins compare whole-palette equality against the mono-AUTHORED palette — strictly stronger than a slot walk (auto-covers future Palette fields) and tokenization-clean — slot-equality-only doctrine; CI grep forbids literals outside theme.rs
- [Phase 12]: 12-03: palette threaded into dashboard render fns as &theme::Palette (minimal pure dependency); hide_cursor is best-effort (let _ =) — restore path re-shows on every exit — research doctrine: helpers carry color only, modifiers inline; chrome must not kill the cockpit
- [Phase 12]: 12-04: dark/light hues HARDENED after UAT found the first palettes indistinguishable from default at a glance — dark border/title/header/emphasis carry the accent blue family, light keeps black-text + light-gray-fill structure visible on either terminal background; the reserved post-UAT hue discretion exercised with zero test changes (structure never pinned hues) — UAT verdict 'nothing seems any different' — wire diagnosis proved the mechanism worked end-to-end but palettes were timid; hues were planner-discretion by design
- [Phase 12]: 12-04: [Rule 1] dormant border/title/header/accent slots wired into the dashboard (pane blocks, sessions header, status-line profile name) — the slots existed in the contract but NO consumer attached them, so panes rendered default-styled in EVERY theme; default/mono proven byte-identical on the wire pre/post (Reset slots), 229 tests unchanged — The true root cause of the UAT verdict — hue tuning alone cannot make unwired slots visible; wiring is the smallest fix that preserves both protected guarantees
- [Phase 12]: 12-04 round 2: DARK text/muted tinted (text Rgb(196,214,235)/Indexed(189) — 14.2:1 vs black, WCAG-AAA body; muted Rgb(110,140,170)/Indexed(67) — 6.0:1, AA) and every screen's body content routed through theme::text/muted (dashboard/logs/rig/projects/tags/alarms/profiles field rows, table rows+headers, log timestamps/loggers, modal bodies + footer hints, tab-bar base) — UAT round-2 verdict "most text is still just white everywhere": no render site consumed text/muted and the dark text slot was near-white; c16 keeps White/DarkGray (readability-first step-down), LIGHT palette byte-unchanged, mono adaptation untouched; wire proof: default/mono SGR sets IDENTICAL pre/post at C256+truecolor, dark adds exactly 38;5;189+38;5;67 (and the truecolor 38;2 pair), light adds only 38;5;8; 231 tests (2 new render-site slot pins)

### Pending Todos

- [Phase 12 prerequisite — from 09 UAT test 10, minor — CODE HALF CLOSED by 12-03, 2026-09-14]: TUI tab indicator — `render_tab_bar` now uses `Tabs::select(active)` + `highlight_style` from the token palette (emphasis fg + inline BOLD, survives mono) and `run()` hides the terminal cursor after `ratatui::init()`; buffer tests pin active BOLD + emphasis slot at color tier and the mono tier. REMAINING: visual confirmation at phase verification/UAT (the original gap was a visual complaint). Details: 09-UAT.md Gap 2.
- [Polish, v1.0 code — from 09 UAT re-verification round, minor]: `trial_reset`'s defensive tail (crates/ignition-core/src/actions/rig.rs:589) stuffs the rig URL into `CoreError::SecretUnavailable`'s `profile` slot, so the TUI modal renders `secret unavailable for profile "http://localhost:…"` — the profile NAME and the missing-credential path (IGNITION_USER/IGNITION_PASSWORD) should be named instead. Candidate to ride along with any later error-message/UX pass.

### Blockers/Concerns

- [Phase 13 prerequisite]: Confirm licensed-Historian rig access before starting Phase 13 planning — spike cannot proceed without it (documented-limitation fallback is legitimate, but access confirmation must happen first)
- [Phase 11 prerequisite — RESOLVED by 11-01]: Real multi-level UDT export captured (artifacts/udt-multilevel.xml, sha-pinned) — the derive-vs-Event-loop decision now has its fixture
- SC-5 (Phase 10) NOT closed by a passing gate run — env blocker RESOLVED via the UAT-recorded disposable-rig substitution (10-06 ran twice on ign-uat-836, torn down clean), but the §2 vanish poll failed both times (grace row >90s ×2; drift recorded 10-LIVE-GATE.md §4 D1). Follow-up gap work: dedicated capture of scheduled/false post-suspend vanish behavior across fresh + long-lived rigs → re-size the 90s deadline (or re-shape the check) → gate re-run. Rig access is a documented recipe (10-RIG-NOTES + gate §5), not a user dependency.
- Parallel-wave note for 11-03 verify gates: pre-existing fmt drift (live_gateway.rs, ignition-tui/ui/mod.rs — Phase-10 commits) plus 11-03's own by-design RED tag_loss tests make workspace-wide cargo fmt --check / cargo test --workspace red independent of 11-02; 11-02 verified clean in an isolated worktree at 34d6594 (clippy -D warnings green, zero failures outside tag_loss RED tests)
- RESOLVED 2026-09-14: user re-verification of tuned dark/light themes — 12-04 UAT round 2 APPROVED by user ("approved") after the body-content tint pass (`27df248`); dark confirmed blue-tinted end to end; default/mono proven byte-identical on the wire pre/post

## Session Continuity

**Last session:** 2026-09-14T19:08:12.836Z
**Resume file:** None
