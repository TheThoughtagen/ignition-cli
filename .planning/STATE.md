# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-08-20)

**Core value:** One binary that lets a developer (or an AI agent) fully operate and inspect an Ignition 8.3+ gateway — health, projects, tags, rigs — without opening the gateway webpage or Designer.
**Current focus:** Phase 5 EXECUTING: 05-01..05-04 COMPLETE (route sources, resource re-point, webdev seam + deploy/status, tags provider CRUD + browse/read/write with the require_routes precondition). Next: 05-05 tagConfig + 05-06 alarms/tagHistory — both inherit the require_routes precondition template from 05-04

## Current Position

**Phase:** 5 of 7 (WebDev Backend & Tag Operations)
**Current Plan:** 5
**Total Plans in Phase:** 6
**Status:** Ready to execute
**Last Activity:** 2026-08-25

**Progress:** [█████████░] 91%

## Performance Metrics

**Velocity:**
- Total plans completed: 6
- Total execution time: 161min

| Plan | Duration | Tasks | Files |
|------|----------|-------|-------|
| Phase 01 P01 | 8min | 3 tasks | 13 files |
| Phase 01 P02 | 53min | 3 tasks | 9 files |
| Phase 01 P03 | 15min | 3 tasks | 15 files |
| Phase 01 P04 | 37min | 3 tasks | 18 files |
| Phase 02 P01 | 19min | 3 tasks | 14 files |

*Updated after each plan completion*
| Phase 02 P01 | 19min | 3 tasks | 14 files |
| Phase 02 P02 | 12min | 3 tasks | 14 files |
| Phase Phase 02 PP03 | 14min | 3 tasks | 15 files |
| Phase 02 P04 | 35min | 3 tasks | 18 files |
| Phase 02 P05 | 40min | 3 tasks | 20 files |
| Phase 03 P01 | 21min | 3 tasks | 13 files |
| Phase 03 P02 | 29min | 3 tasks | 21 files |
| Phase 03 P03 | 29min | 3 tasks | 12 files |
| Phase 04 P01 | 42min | 3 tasks | 14 files |
| Phase 04 P02 | 18min | 3 tasks | 8 files |
| Phase 04 P03 | 406min | 3 tasks | 14 files |
| Phase 04 PP04 | 37min | 3 tasks | 17 files |
| Phase 05 P01 | 24min | 3 tasks | 19 files |
| Phase 05 P02 | 38min | 3 tasks | 16 files |
| Phase 05 P03 | ~400min (2 sessions) | 3 tasks | 35 files |
| Phase 05 P04 | ~450min (3 sessions) | 3 tasks | 21 files |

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- Roadmap: 7-phase order follows research convergence — contracts → inspection → projects → rig → WebDev/tags → TUI → interop (rig before WebDev: rig is the self-managed fixture; TUI last: consumes finished action surface)
- Roadmap: `ign script run` command surfaces in Phase 7 per orchestrator call (scriptExec route ships in Phase 5's webdev/ sources; security posture is a Phase 5 spike)
- [Phase 01]: MSRV locked at 1.88 (keyring 4.1.x floor), correcting STACK.md's 1.85
- [Phase 01]: Workspace shape final from commit one: three crates (ignition-cli bin 'ign' / ignition-core lib / ignition-tui zero-dep stub), tui feature gate default-on, no Windows CI
- [Phase 01]: CLI contract chassis: five global clap args defined once, main() -> ExitCode single exit point, env defaults in exactly one function, stderr-only diagnostics
- [Phase 01]: Edition 2024 let-chains in apply_env_defaults keep clippy -D warnings clean without allows
- [Phase 01]: [Phase 01]: Agentic output contract FROZEN — envelope exactly {ok,profile,data}/{ok,profile,error}, exit taxonomy 1-7 with stable slugs, errors-on-stderr in all modes; changing shape/slugs/codes is a breaking change for agents
- [Phase 01]: [Phase 01]: Exit-code table lives in exactly two places (CoreError::exit_code() + README), enforced by the enumerated unit test and snapbox goldens; --compact implies --json, resolved once in RenderMode::resolve
- [Phase 01]: [Phase 01]: snapbox inline goldens: str! trims leading+trailing newlines, Cow<str> lacks IntoData — use stdout_for_golden (strip println's single trailing newline) and pass &str; isolate IGNITION_CLI_CONFIG per spawn; [..]-elide dynamic values
- [Phase 01]: Secret chain encodes the LOCKED order (env tokens → keyring → USER/PASSWORD) — BasicEnvStore is separate so keyring comes before basic; KeyringStore resolve fails soft, set/delete surface errors
- [Phase 01]: profile add skips pre-resolution (--profile naming a NEW profile must not fail); add's envelope echoes post-add active state; config save re-asserts 0600 on overwrite
- [Phase 01]: URL trailing-slash normalization pinned in goldens (typed url::Url beats storing strings); ActionOutput::render_json is monomorphic per-variant (Serialize not dyn-compatible)
- [Phase 01]: [Phase 01]: GatewayApi locked on async_trait, ONE coarse method (gateway_info) — Phase 2 grows it by capability; auth headers token-XOR-basic enforced by a match, Secret::expose() confined to the single header-construction site
- [Phase 01]: [Phase 01]: version behavior matrix LOCKED — unreachable gateway → exit 0 + warning inside data (never a top-level field); refusal (exit 6) only when the gateway ANSWERED <8.3.1/unparseable; SecretUnavailable degrades to header-less via resolve_secret_opt, never blocks version
- [Phase 01]: [Phase 01]: below_minimum compares against plain semver 8.3.1 (research sketch's 8.3.1.0 constant cannot parse — semver is strict); GatewayInfo carries a serde(skip) endpoint field so action-built GatewayTooOld populates CORE-05
- [Phase 01]: completions print RAW to stdout regardless of --json (the one sanctioned success-path exception, README-documented) and dispatch before config load; require_confirmation guard (exit 2, hint names --yes + IGNITION_YES=1) pinned in main.rs, #[cfg_attr(not(test), expect(dead_code))] until Phase 3's first destructive caller
- [Phase 01 CI bring-up]: #[expect(dead_code)] on test-exercised fns is unfulfilled in test-target clippy compiles → gate with #[cfg_attr(not(test), expect(...))]; keyring store-unavailable is debug (expected headless condition — warn noise preceded JSON envelopes on stderr); stderr-envelope tests parse from first '{' (log-tolerant)
- [Phase 02]: [Phase 02-01]: GatewayInfo serializes under gateway-native camelCase keys (rename=ignitionVersion + alias=version) — passthrough-shaped --json data; state/uptime dropped (not on the real payload; 02-02 sources them from /overview + /StatusPing)
- [Phase 02]: [Phase 02-01]: classify() is the single status→content-type→redirect mapping site running before every .json(); redirect(Policy::none()) pinned so uncommissioned 302s can never masquerade as 200; Basic demoted with a loud per-call warning (dead on 8.3 /data)
- [Phase 02]: [Phase 02-01]: three ADDITIVE exit-6 slugs (gateway_not_commissioned, gateway_restarting, not_found) + status-aware 401/403 auth hints; wiremock gotchas recorded: set_body_string forces text/plain (use set_body_raw), scoped MockGuard drop unmounts fixtures
- [Phase 02]: [Phase 02-01]: gateway-info REQUIRES auth under 8.3 default security (header-less → 401 Jetty HTML, re-verified live on the research rig) — 83-api's auth:none tag does not hold; live suite is skip-by-default green no-op
- [Phase 02]: [Phase 02-02]: Two-layer naming LOCKED — client models stay wire-faithful (gateway-native camelCase renames + flatten passthrough); the status action re-exposes selected fields under unit-explicit keys (uptime_ms, cpu_fraction, trial_remaining_s); overview cpu 0-1 fraction vs gauges percent documented at both fields, never converted
- [Phase 02]: [Phase 02-02]: Inspection commands (status/modules/metrics) REQUIRE a credential — resolve_secret → SecretUnavailable exit 3 (inverse of version's header-less degradation); LOCKED secret chain now built in exactly one place (secret_chain() in main.rs); status = read of a HEALTHY gateway, failed sub-calls exit per taxonomy
- [Phase 02]: [Phase 02-02]: ModuleInfo tolerates the quarantined reduced shape (state/licenseState/vendorName/startupTime Option per openapi — fully-loaded-only; startup_time is a String on the wire); PerformanceCharts parses the nested memoryChartDatapoints wire shape into a flat model; /StatusPing fetched header-less via auth=false (wiremock header-absence proof — the 02-05 wait anchor)
- [Phase Phase 02]: [Phase 02-03]: require_confirmation dead-code gate REMOVED in Phase 2/02-03 (sessions terminate = first destructive caller) — earlier than the logged 'until Phase 3'; the attribute's own reason string mandates removal at the first real caller. Guard fires BEFORE profile/secret/client resolution: refusal = exit 2 with null profile, zero config/network work; usage-class errors lead — Plan key_link + must_have truth #2; clippy -D warnings clean with the gate gone is the proof
- [Phase Phase 02]: [Phase 02-03]: sessions --type rides the SessionsArgs top level and terminate's id is --id <ID> — LOCKED must_have truths overrode the plan's nested-List/positional-id sketch; JSON data always carries ALL family keys (filtered-out = [], endpoints never called) — the stable agent shape all filtered list commands inherit — must_haves are the user contract; agents must never key-hunt
- [Phase Phase 02]: [Phase 02-03]: Perspective path discipline contract-pinned by recorded-request proofs — GET /data/perspective/api/v1/sessions/ EXACT trailing slash (Pitfall 8), DELETE no-trailing-slash with sessionId/message as QUERY params + empty body; connections ride resources/list/ignition/{database,opc}-connection with healthchecks RAW passthrough (LOW-confidence until live capture — live_connections hook + UAT open question) — Wire subtleties asserted on the REQUEST, not just response parsing
- [Phase 02-04]: [Phase 02-04]: CoreError::Network.source → Option<reqwest::Error> — source:None marks a poll deadline expiry (same network_error slug/exit 4, NO new variant per plan); the deadline message rides url (subject + waited + last observation); Some-source Display byte-identical, no golden moved
- [Phase 02-04]: [Phase 02-04]: poll.rs is THE wait engine (HRTB state-threading: for<'a> FnMut(&'a mut S) -> Probe<'a,T>) — ×1.5 backoff clamp [interval,30s], Network/GatewayRestarting retried, Auth never; 02-05's wait/restart --wait reuses it verbatim; tail maps the None-source deadline error to graceful Ok (exit 0)
- [Phase 02-04]: [Phase 02-04]: 'ign logs' shows the NEWEST entries via sortBy=desc(timestamp) + explicit limit (gateway's own openapi asc()/desc() syntax) — 'recent' without inventing a since-window policy; EVERY logs command sends an explicit limit (default 200, loggers included — Pitfall 9); logs -f --json streams NDJSON (one compact entry per line, no envelope) — the SECOND sanctioned stdout exception, README-documented
- [Phase 02]: [Phase 02-05]: poll HRTB constraint pinned — a probe whose state carries no lifetime or whose Done payload is non-() does not typecheck; every wait probe uses S=&mut Cell<String> + PollState::<()> with the terminal state riding the outer Cell (the tail pattern generalized) — Empirically bisected: identical closure shapes compile/fail on exactly these two properties
- [Phase 02]: [Phase 02-05]: wait gateway + wait restart dispatch HEADER-LESS (secret degrades to None — the unauth StatusPing anchor must work when auth is broken); wait module stays authed; doctor's credential degrades too, with a credential_present flag giving the honest 401 split (no-credential vs not-recognized)
- [Phase 02]: [Phase 02-05]: RESTART_FLOOR is ONE shared from_secs(5) const (restart_and_wait sleeps it post-POST; wait_restart requires it elapsed before all-RUNNING success, witnessing non-RUNNING short-circuits it); doctor exits 0 whenever the diagnosis completes — failing checks are data; webdev_route_status bypasses classify (404-vs-present IS the answer)
- [Phase 03-01]: ProjectModify.enabled is Option<bool> (skip-if-none): an always-sent enabled on modify would clobber the flag on single-field sets; the Task-2 unit pin ({"title":"T"} exactly) demanded it
- [Phase 03-01]: ProjectSetResult carries fields-touched as serde(skip) Vec<String> — the human 'set <fields> on <name>' line without deviating the flat six-key agent JSON
- [Phase 03-01]: Project action results keep ALL six keys present (null when absent) — stable agent shape; defaultDb/tagProvider/userSource passthrough stays at the client seam; encode_segment (NON_ALPHANUMERIC) is the one per-segment encoder 03-03 resource paths reuse keeping '/'
- [Phase 03-01]: Destructive delete pin (Pitfall 8 both layers): CLI guard refuses pre-resolution (exit 2, profile null — sessions-terminate shape verbatim) AND the wire DELETE always carries confirm=true QUERY param with empty body — wiremock recorded-request proven at both client and binary level
- [Phase 03-02]: ImportOutcome opaque-success normalizes non-object 2xx bodies (restart's literal true included) to {"status":"success"} — object JSON passes through verbatim; agents always see a stable object
- [Phase 03-02]: Export streams, import buffers BY DESIGN: download_to_file (classify→bytes_stream→tokio::fs, 120 s) never touches Vec<u8>; import rides a known-Content-Length application/zip body + overwrite QUERY param under a 300 s per-request timeout — Pitfall 2/3's structural answers, no second client
- [Phase 03-02]: Collision conventions LOCKED: abort (default) = action-layer find pre-check refusing project_exists (exit 6, replace-not-merge hint) BEFORE any upload; overwrite = --yes-guarded pre-resolution with NO pre-check (server is authority); merge is not a clap value (Designer-only, README documents)
- [Phase 03-02]: snapbox gotcha #3: str! normalizes backslashes in ACTUAL output to forward slashes — message text PK\x03\x04 goldens as PK//x03//x04 (recorded inline at the golden)
- [Phase 03-03]: Resource get returns the flat stable shape {project, path, content_kind, content} — family convention (identity fields, all keys always) over the plan's enum sketch; ResourceBinary (exit 6) fences data.bin-class resources out of BOTH get and put
- [Phase 03-03]: Added InvalidInput (exit 2, invalid_input) for put's unreadable --file/stdin — additive slug; import-specific invalid_import_file would mislabel; two-places exit-table rule synced
- [Phase 03-03]: Resource paths over-encode via the ONE locked encoder (dots %2E, hyphens %2D, '/' preserved) — over-encoding is safe, server decodes before matching; sniffer's 8-KiB NUL window boundary pinned honestly (late lone NUL in UTF-8 = Text)
- [Phase 03-03]: E2E harness convention LOCKED: #[ignore] tests, quiet skip without env, mutations need IGNITION_LIVE_MUTATIONS=1; openapi-capture gate writes the trimmed projects|scan|resources extract into the phase dir (phase2 precedent); replace-not-merge pinned TWO-SIDED in the loop (pre-export resource survives + post-export resource not_found)
- [Phase 04-01]: Rig discovery order LOCKED: --rig > IGNITION_RIG > [rig].default > cwd candidates > git-module > WHK-Global (both home roots, first hit wins) — must-have truth overrode the plan's inverted test blurb; stale [rig].default is a loud exit-7, never a silent scan
- [Phase 04-01]: Uncommissioned-as-data fires ONLY on the exact poll-deadline shape (Network{source:None}) + witnessed terminal-uncommissioned flag; still-STARTING at deadline stays rig_error. Research Open Question 4 RESOLVED live: --project-directory makes the rig's .env COMPOSE_PROJECT_NAME authoritative cwd-elsewhere
- [Phase 04-01]: ComposeRunner seam carries TWO program shapes: run (docker compose) + run_docker (plain docker for volume ls / docker ps attribution); tokio process is the ONLY Phase 4 dep change; LDJSON parsing is line-based (StreamDeserializer halts at first bad line); IGNITION_RIG_ROOTS env overrides convention home roots for test isolation
- [Phase 04]: [Phase 04-02]: rig reset cycle LOCKED: preview (volume-ls label + name-prefix filter) BEFORE version gate → down -v --remove-orphans → port preflight BETWEEN the halves (fresh eyes, torn-down-state hint) → up → commissioned_wait reused verbatim (poll.rs diff-empty); guard fires BEFORE discovery — binary-pinned by exit-2-not-exit-7 in a no-rig cwd (third destructive-verb instance)
- [Phase 04]: [Phase 04-02]: rig logs is the THIRD sanctioned stdout exception — RAW compose lines in EVERY mode including --json (compose lines are not gateway JSON; no NDJSON transform attempted); run_streaming seam = piped stdout + CONCURRENT stderr drain (pipe-deadlock-proof); streaming sinks need explicit dyn for<'a> FnMut(&'a str) + Send (elided FnMut(&str) mismatches across async_trait desugaring)
- [Phase 04]: [Phase 04-02]: LIVE-VERIFIED on ignition-devops — second reset previewed+removed ignition-devops_gateway_data exactly; preview's label-filter under-report on stale pre-label volumes is inherent (plan-LOCKED mechanism) and self-heals (compose-created volumes carry the label); test fixtures script OWN-PROJECT occupants so preflight tests never hit the advisory lsof path (determinism with a running rig — 04-01 machine-isolation lesson, lsof edition)
- [Phase 04-03]: Trial-reset spike resolved LIVE: tier 1 (native OIDC login) is the mechanism — full flip on the expired 8.3.3 rig (expired:true→false, 0→7199s); session cookie = webui-sid-<gatewayId>, CSRF field = csrfToken from /data/app/session (both research LOW-confidence items). Tier 0 ships as the ladder's first rung — formally open pending a provisioned token (no headless path; the api-token create's collection value is undiscovered)
- [Phase 04-03]: LIVE-DISCOVERED state gate: gateway 403s trial resets on NON-expired trials (proven from the browser page with exact UI headers) → additive slug trial_not_expired (exit 6) + the action's expiry pre-check — the honest target-state refusal over a misleading auth_rejected
- [Phase 04-03]: 8.3.3 rig creds discovered + verified (admin/password, resetter tst1.env); ign-research does NOT accept them. Trial endpoints verified unauth on BOTH minor versions → conditional-auth trait methods (cred rides when present, header-less otherwise). Trial verbs echo config.active as context; docker verbs stay profile:null
- [Phase 04-04]: Restore wait deadline MAX-clamps at 300s (Pitfall 6): an explicit short --timeout cannot buy an unknown-state mid-restart report — restore_deadline is the tested pure fn
- [Phase 04-04]: backup download rides download_to_file via ONE optional Accept param (not a forked chunk loop — the single streaming body-consumption site holds); roaming query rides the path constant; restore POST = raw octet-stream with 4 explicit-false params, NOT multipart
- [Phase 04-04]: Snapshot manifest is the honest composition contract: BOTH exclusion notes verbatim (trial clock NOT captured by gwbk; tag-provider bulk export = Phase 5); gateway_info failure degrades to ignition.version null in the artifact; project file names percent-encode injectively
- [Phase 04-04]: snapshot/restore creds = IGNITION_TOKEN only (backup route 401s unauth; Basic dead on 8.3 /data — no second rung), missing token = exit 3; restore is the 5th guarded destructive verb (binary-pinned before discovery)
- [Phase 05]: [Phase 05-01]: scriptExec secret = SECRET = None or '__IGN_CLI_SECRET__' template marker — deploy does ONE string substitution; None AND placeholder-shape (leading underscore) both fail-closed (version included), so the public template can never arm the gate; template excluded from ROUTE_FILES so unsubstituted deploy is impossible by construction
- [Phase 05]: [Phase 05-01]: scriptExec config.json stays require-auth FALSE / user-source '' — secret-only posture (API tokens 401 on WebDev require-auth, a Basic layer would lock the CLI's own token-authed calls out; research OQ3 resolved)
- [Phase 05]: [Phase 05-01]: route body envelope LOCKED — {ok,data}/{ok,error{code,message,traceback?}} at HTTP 200 (WebDev ignores 'status'); machine codes are the stable route contract: no_alarm_journal (structured journal-missing denial), secret_required, secret_mismatch, unknown_action, not_found, route_error
- [Phase 05]: [Phase 05-01]: route folders are SELF-CONTAINED by design (no cross-resource imports) — the ~25-line shared core (unicode re-parse, jv() depth-12 walker, bare-except traceback envelope) is duplicated across all five routes deliberately; ignition-core::webdev is pure data (include_str!), deploy orchestration lives in the actions layer (05-03)
- [Phase 05-02]: [Phase 05-02]: Resource family re-pointed onto export-zip surgery (zip 8.6, the only new dep) — UX contract unchanged, transport = export → pure member surgery (client/resources.rs helpers, unit-pinned) → import(overwrite=true); Phase 3 cross-phase blocker CLOSED — No per-resource REST routes exist on real 8.3 gateways (triple-verified: live openapi 575 paths + committed extract + EAM probe); export/import round-trip is native and the machinery shipped in 03-02
- [Phase 05-02]: [Phase 05-02]: resource put JOINED the --yes-guarded set (member surgery implicitly overwrite-imports the whole project; 03-03's unguarded put superseded) — put/delete refusal MESSAGES name the consequence via the operation string while the shared ConfirmationRequired hint stays frozen (no other verb's golden moved); prefix filter went client-side (starts_with on member paths) — Replace-not-merge wipes concurrent Designer edits — the plan's accepted-tradeoff language; consequence-at-refusal over generic hints
- [Phase 05-02]: [Phase 05-02]: Surgery contract pins — request-SEQUENCE wiremock proofs (reads = exactly one export GET + zero imports; writes = export then overwrite-import with content-type application/zip) asserted at MEMBER level by round-tripping the received import body through the same public helpers; missing member = not_found with endpoint:null (there was no 404 URL, there was a missing zip member); tempfile promoted from dev (already in workspace graph) for the temp export — Byte-exact zip equality is not deterministic across writers; member-level honesty is the contract
- [Phase 05-03]: WebDev seam LOCKED: POST /system/webdev/{project}/cli/{route} with the 200-BODY envelope as the only success oracle (denials ride 200); probe matrix 405=Absent (not 404), 402=Unlicensed, 401/403=AuthGated — doctor re-pinned at every layer incl. its CLI golden
- [Phase 05-03]: webdev deploy NOT --yes-guarded: the dedicated ign-cli project is CLI-OWNED (born from the deploy zip, overwrite-replaced every deploy — replace-not-merge IS the contract; user projects never touched); no pre-flight project create (Pitfall 10); status is a READ (exit 0 whenever the sweep completes, degradation is data — the exit-6 refusal matrix belongs to WebDev-DEPENDENT commands via webdev_precondition)
- [Phase 05-03]: scriptExec LOCKED posture shipped: /dev/urandom 32-byte hex secret persisted 0600 in the profile BEFORE upload and substituted into the template (placeholder excluded from the plain manifest — unsubstituted deploy impossible by construction); route fail-closes on every action incl. version; secret appears in exactly one place (the baked zip member) — redaction proven at action AND binary level; README documents the shared-secret threat-model honesty note
- [Phase 05]: [Phase 05-04]: Tag split LOCKED — providers ride NATIVE config-resource REST (list/find/create-array-body/delete-by-signature, tagCount+health metrics, no route dependency); browse/read/write ride the deployed tags route via webdev_route_call
- [Phase 05]: [Phase 05-04]: ONE shared require_routes precondition (405→routes_not_deployed, 402→webdev_unlicensed, mismatch→route_version_mismatch) runs before EVERY webdev-dependent action — one extra probe round trip, correctness over latency, no caching this phase; the template 05-05/06 inherit
- [Phase 05]: [Phase 05-04]: tags provider delete = 6th --yes-guarded destructive verb (binary-pinned zero-work refusal); find-miss = additive provider_not_found exit 6; write --value follows the JSON-scalar rule (bare string stays string, arrays/objects invalid_input exit 2 pre-resolution); browse human mode = indented tree from fullPath, JSON = flat agent shape; quality strings are DATA never parsed (Bad_NotFound reads exit 0)

### Pending Todos

None yet.

### Blockers/Concerns

- ~~**[CROSS-PHASE — routed to Phase 5 planning]** Phase 3 `resource` family defect: `ign resource` (client/resources.rs + cli arm + e2e witnesses) targets `/data/api/v1/projects/{name}/resources/**` routes that DO NOT EXIST on real 8.3 gateways~~ CLOSED by 05-02: the family re-pointed onto project-export ZIP surgery (export → pure member surgery → import overwrite=true; zip 8.6 the only new dep). UX contract unchanged, put/delete now --yes-guarded with consequence-naming refusals, prefix filter client-side, binary fence survives (member-bytes sniff), e2e witnesses live-runnable for the first time since Phase 3 (e2e_projects loop + e2e_rig pre-witness both re-pinned). See 05-02-SUMMARY.md.
- ~~Phase 4 live gates~~ CLOSED AUTONOMOUSLY post-verification (04-VERIFICATION.md addendum, commits bf51760/3d075ee): tier-1 trial reset live-proven on fresh 8.3.6 rig (0/expired → 7187s/active); snapshot→mutate→restore two-sided PASS via real CLI verbs on an 8.3.3 clone; lifecycle smoke passed. HEADLESS TOKEN PROVISIONING SOLVED (version-agnostic, proven both lines): OIDC login → `POST api-token/generate` → `POST resources/ignition/api-token` with `collection:"core"` → patch `security-properties` read/writePermissions to `AnyOf [Authenticated]` — full recipe in the addendum. Bonus findings: trial clock RIDES the restore (snapshot taken mid-trial restores the remaining clock); tokens+permissions inside a snapshot survive restore (Pitfall-5 warning not observed live). Tier-0-on-8.3.6 probe remains one natural-expiry away (non-blocking curiosity; tier-1 is the shipped mechanism). e2e_rig gate's resource-witness step blocked by the Phase 3 defect above.
- ~~Phase 4 spike pending: trial-reset mechanism (Playwright delegation vs native HTTP+CSRF)~~ RESOLVED at Phase 4 planning by 04-RESEARCH.md (live-probed on ign-research 8.3.6): native Rust HTTP ladder — tier 0 token-auth POST /data/api/v1/trial (one live call decides), tier 1 mapped OIDC challenge flow (client/idp.rs), tier 2 Playwright README-documented fallback only. Playwright delegation rejected (Node+chromium runtime, broke across 8.3.3 UI rewrite, DOM-text verification).
- ~~Phase 5 spike pending: WebDev deploy mechanism (per-resource vs project-zip import); script-exec security posture; tag-history route availability on default rigs~~ RESOLVED by 05-RESEARCH + 05-01/05-03: deploy = project-zip import overwrite=true into the CLI-owned ign-cli project; script-exec posture LOCKED (deploy-on-request, /dev/urandom secret 0600-in-profile, fail-closed route, redaction proven); tag-history availability rides the live e2e gate (opt-in)
- ~~Phase 2 gap: live-gateway auth verification (token header across /data + /webdev, Basic viability)~~ CLOSED by 02-01: claims verified empirically during research + wiremock-pinned; executable proof path = live_gateway.rs `-- --ignored` (needs IGNITION_LIVE_URL/IGNITION_LIVE_TOKEN per 02-USER-SETUP.md; research rig `ign-research` still up on port 18088 if a fresh token is created). /webdev half re-checks in Phase 5.
- ~~Phase 1: smoke-test keyring 4.1 on headless Linux CI~~ CLOSED & CI-CONFIRMED: keyring-smoke job green on ubuntu headless (run 32517734178, 2026-08-21)

## Session Continuity

**Last session:** 2026-08-25T03:59:03.375Z
**Stopped At:** Completed 05-04-PLAN.md (tags provider CRUD + browse/read/write; recovered Task 3 from cancelled sessions)
**Resume file:** None
