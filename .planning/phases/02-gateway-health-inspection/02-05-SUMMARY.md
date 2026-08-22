---
phase: 02-gateway-health-inspection
plan: 05
subsystem: api
tags: [reqwest, wiremock, polling, restart, statusping, readiness, floor-race, doctor, diagnostics, security-properties, webdev, tcp-dial, snapbox, confirmation-guard, hrtb]

# Dependency graph
requires:
  - phase: 02-gateway-health-inspection (02-01)
    provides: "classifier + pipeline helpers (post_empty already carried query params), IgnitionMock harness, exit taxonomy (reused exclusively — error.rs untouched all plan)"
  - phase: 02-gateway-health-inspection (02-02)
    provides: "status_ping (auth=false — THE unauth readiness anchor), modules capability, credential-REQUIRED dispatch + resolve_gateway_api"
  - phase: 02-gateway-health-inspection (02-03)
    provides: "guard-before-construction destructive dispatch (restart inherits it), 302→/welcome classification"
  - phase: 02-gateway-health-inspection (02-04)
    provides: "poll.rs — the ONE wait engine both restart-aware waits reuse verbatim; post_empty with query params; the tail's outer-Cell HRTB pattern"
provides:
  - "Two new GatewayApi methods for the red button: restart (POST restart-tasks/restart?confirm=true, literal-true 200 with drift-tolerant warn) + scan_projects (the harmless write probe)"
  - "actions::restart — restart / restart_and_wait (POST → 5s floor → poll to RUNNING) / wait_gateway (immediate-success-correct) / wait_restart (witnessed non-RUNNING→RUNNING short-circuits the floor; all-RUNNING accepted only past it) / wait_module (modules?search= to ACTIVE); ONE shared RESTART_FLOOR const, injectable"
  - "actions::doctor — the 8-check structured preflight (url TCP dial, unauth liveness, commissioned, 401-vs-403-vs-no-credential auth, security-properties permissions deep-dive, --check-write probe, --webdev-route presence, rig) — exits 0 whenever the diagnosis completes"
  - "security_properties + webdev_route_status client capabilities (raw-status probe, never classified)"
  - "The `ign restart [--wait]` + `ign wait gateway|restart|module` + `ign doctor` command surface with header-less wait dispatch (secret degrades to None)"
  - "Live-suite additions: live_doctor_end_to_end (read-only, now 10 ignored checks)"
affects: [phase-03-projects, phase-04-rig, phase-05-webdev-tags, phase-06-tui]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Restart-aware floor (Open Question 4): ONE shared `Duration::from_secs(5)` const — restart_and_wait sleeps it after the POST; wait_restart requires it elapsed before an all-RUNNING sequence may report success (observing non-RUNNING→RUNNING short-circuits it) — never restart-tasks/pending"
    - "Header-less wait dispatch: resolve_headerless_api degrades credential resolution to None so `wait gateway`/`wait restart` work when auth is broken (the whole point of the unauth StatusPing anchor)"
    - "Doctor exit contract: exit 0 whenever the diagnosis COMPLETES — failing checks are data (checks[] for agents, table for humans); only config-class errors exit through the normal path"
    - "Raw-status probe: webdev_route_status returns the HTTP status without classifying — presence (404 vs anything) IS the answer"
    - "Machine-dependent golden rows: the doctor's rig row + summary counts are `[..]`-elided (Docker presence varies by machine) — the snapbox glob keeps the golden deterministic everywhere"

key-files:
  created:
    - crates/ignition-core/src/client/restart.rs
    - crates/ignition-core/src/actions/restart.rs
    - crates/ignition-core/src/actions/doctor.rs
    - crates/ignition-core/tests/restart_wait_contract.rs
    - crates/ignition-cli/tests/contract_restart_wait.rs
    - crates/ignition-cli/tests/contract_doctor.rs
  modified:
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/src/actions/mod.rs
    - crates/ignition-core/tests/live_gateway.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - README.md
    - Test-double stubs for the 4 new trait methods: actions/{version,inspect,sessions,connections,logs}.rs

key-decisions:
  - "The poll HRTB constraint, empirically pinned: a probe whose poll-owned state carries NO lifetime (owned value) or whose Done payload is non-() does NOT typecheck under `for<'a> FnMut(&'a mut S) -> Probe<'a, T>` — every wait probe therefore uses S = `&mut Cell<String>` (or a borrowing struct) + PollState::<()>, with the terminal state riding the Cell the state mutably borrows (the tail's outer-Cell pattern generalized)"
  - "wait gateway + wait restart dispatch HEADER-LESS (resolve_headerless_api: SecretUnavailable → None): StatusPing needs no credential, so waiting must never be blocked by broken/absent auth — wait module stays authed (modules requires a token)"
  - "Doctor's 403 permissions deep-dive runs even when auth read failed with 403 (the read failing too CONFIRMS part 2 of the three-part diagnosis — warn, not skip); a no-credential 401 is diagnosed as 'no credential resolved' rather than 'bad token' (the honest split, via a credential_present flag on the action)"
  - "webdev_route_status deliberately bypasses classify(): the doctor needs the RAW status (404 = absent, 200/401/403 = exists) — classifying would erase the answer; only transport failures error"
  - "restart's non-`true` 2xx body is success-shape drift (warn, still Ok) — the POST was accepted; the wait half reports what happens next"
  - "Doctor's machine-dependent rows (rig + summary counts) are [..]-elided in goldens rather than made injectable — the plan's 'no other options' constraint ruled out a --no-rig flag"

patterns-established:
  - "RESTART_FLOOR is THE one shared floor constant; any future restart-aware wait imports it (never rewrites from_secs(5))"
  - "Doctor's checks[] shape {name, status, detail, hint} is agent contract — hint serializes null when absent so agents can key on it unconditionally"
  - "Phased trait growth: 4 new GatewayApi methods this plan, stubbed into all 7 existing test doubles (the established chore)"

# Metrics
duration: 40min
completed: 2026-08-22
---

# Phase 2 Plan 5: careful restart + waits + doctor Summary

**The careful-restart story (`ign restart [--wait]`, the three `ign wait` targets sharing ONE 5s floor constant against the fast-flip race) and the full `ign doctor` preflight (url/liveness/commissioned/auth-401-vs-403/security-properties/write/webdev/rig as structured checks[] that exits 0 on completion) — all lifecycle- and taxonomy-pinned against wiremock.**

## Performance

- **Duration:** 40 min
- **Started:** 2026-08-22T03:49:30Z
- **Completed:** 2026-08-22T04:29:51Z
- **Tasks:** 3
- **Files modified:** 20 (6 created, 14 modified)

## Accomplishments
- HLTH-09 shipped end-to-end: `ign restart` is ALWAYS --yes-guarded before any API construction (exit-2 golden with no server mounted — a refusal costs nothing), the POST pins `confirm=true` as a QUERY param on the exact path with an empty body (recorded-request proofs at both the core and CLI layers), and without --wait the human line carries the research-mandated "READY in ~1 min; consider `ign restart --wait`" advisory
- HLTH-11 shipped: `restart --wait` completes the POST → 5s floor → STARTING → RUNNING lifecycle (golden-pinned, elapsed [..]-elided with the ≥5s floor asserted); `wait gateway` polls the unauth StatusPing header-less (proven by running the golden with NO token in the env — the secret degrades to None); `wait restart` shares restart --wait's exact semantics — the WITNESSED path (non-RUNNING observed once → RUNNING) returns with NO floor wait (<5s asserted in the golden) while an all-RUNNING sequence succeeds only past the floor (core-tested with an injected floor, timing-proven); `wait module` polls modules?search= to ACTIVE
- HLTH-10 shipped: `ign doctor` walks the research's empirically-verified failure taxonomy — url parse + 3s TCP dial (DNS/firewall split), unauth liveness (down-ness never confused with bad auth BY CONSTRUCTION), commissioned 302→/welcome, the 401 (name:key format) vs 403 (three-part setup) vs no-credential honest split, the security-properties permissions deep-dive (on 403, the read failing too CONFIRMS the wiring diagnosis), the --check-write scan/projects probe (403 = read-only token), --webdev-route presence (404 = absent), and the local-only rig row — with the four scenario goldens (healthy / 401 / 403 / uncommissioned) plus the JSON shape golden
- The doctor exit contract is README-documented and test-pinned: exit 0 whenever the diagnosis COMPLETES — all four failure-scenario goldens assert success exit; only config errors (no profile) exit 3 through the normal path
- Phase 2 finish line reached: every health surface is inspectable, sessions terminable, logs tail-able, the gateway carefully restartable, and the setup diagnosable — with zero gateway-side CLI setup beyond one API token

## Task Commits

Each task was committed atomically:

1. **Task 1: restart + scan_projects capabilities + wait primitives** - `0525fdf` (feat)
2. **Task 2: `ign restart` + `ign wait` commands + goldens** - `d38e067` (feat)
3. **Task 3: `ign doctor` — the preflight report** - `0f738e4` (feat)

## Files Created/Modified
- `crates/ignition-core/src/client/restart.rs` - restart/scan_posts paths + literal-true drift guard; security-properties singleton model (passthrough); webdev route path helper; path-literal pins
- `crates/ignition-core/src/actions/restart.rs` - restart / restart_and_wait / wait_gateway / wait_restart / wait_module over 02-04's poll engine; RESTART_FLOOR (from_secs(5)) + default interval/timeout constants
- `crates/ignition-core/src/actions/doctor.rs` - the 8-check sequence, CheckResult/CheckStatus/DoctorOptions, per-check constructors + 9 unit tests (order contract, 403 wiring confirmation, no-credential honesty, dead-port dial, read-only write probe, serialization pins)
- `crates/ignition-core/src/client/mod.rs` - +4 trait methods (restart, scan_projects, security_properties, webdev_route_status) with the raw-status probe helper
- `crates/ignition-core/tests/restart_wait_contract.rs` - 13 wiremock lifecycle scenarios incl. recorded-request POST proof, fast-flip floor timing, witnessed-restart floor short-circuit, timeout taxonomy
- `crates/ignition-cli/src/cli.rs` - Restart {--wait,--timeout,--interval}, Wait{Gateway,Restart,Module}, Doctor {--check-write,--webdev-route}
- `crates/ignition-cli/src/main.rs` - restart/wait/doctor dispatch; resolve_headerless_api (secret degrades to None); guard-before-construction for restart
- `crates/ignition-cli/src/render.rs` - restart advisory, RUNNING-after-Ns lines, wait final-state line, doctor table + hint rows + count summary
- `crates/ignition-cli/tests/contract_restart_wait.rs` - 7 goldens (guard, --yes POST proof, full lifecycle, headerless gateway wait, witnessed restart, module, exit-4 timeout)
- `crates/ignition-cli/tests/contract_doctor.rs` - 4 scenario goldens + JSON shape golden + flags flip test (rig row [..]-elided for machine independence)
- `crates/ignition-core/tests/live_gateway.rs` - +live_doctor_end_to_end (read-only; 10 ignored checks total)
- `README.md` - restart/wait/doctor rows, the wait-restart floor-semantics note, restart in destructive ops, doctor exit-0 contract, the three-part token-setup troubleshooting section
- Test doubles stubbed ×7 for the 4 new trait methods

## Decisions Made
- **The HRTB probe shape (empirically derived, the plan's hardest deviation):** poll's `for<'a> FnMut(&'a mut S) -> Probe<'a, T>` bound rejects probes whose state carries no lifetime or whose Done payload is non-() — the 02-04 tail worked because TailState borrows its sink and returns PollState::<()>. Every wait probe therefore threads S = `&mut Cell<String>` (or the borrowing Witness struct) and parks the terminal state in the outer Cell the state mutably borrows — the established pattern, now documented at the module header for Phase 3+
- **Header-less waits:** `wait gateway` and `wait restart` construct the client with degraded credential resolution (resolve_headerless_api) — the unauth StatusPing anchor means waiting works during broken auth, which is precisely when you wait; `wait module` requires a token (modules is an authed read)
- **Doctor's permissions deep-dive on 403:** the plan sketched "only when auth ok", but running the read on a 403 is itself diagnostic (the token cannot read the security config either ⇒ the permission wiring is the culprit — part 2 of the three-part setup); it reports warn with the specific fix hint
- **credential_present flag:** a 401 with no configured credential is "no credential resolved" (fix: set IGNITION_TOKEN), distinct from a 401 with a credential ("token not recognized" — fix: the name:key format); doctor is the preflight for BOTH mistakes
- **Raw-status webdev probe:** classify() would map 404→NotFound and erase the presence answer; the probe returns the status code untouched

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The plan's wait-probe sketch did not typecheck under poll's HRTB bound**
- **Found during:** Task 1 (actions/restart.rs)
- **Issue:** Straightforward closures (`poll(cfg, (), |()| Box::pin(async {... Ok(PollState::Done(state))}))`) fail with E0597/E0309 — the probe's poll-owned state must carry a lifetime and the Done payload must be `()` (bisected empirically: identical shapes compile with S = `&mut Cell` + `PollState::<()>` and fail with owned states or `Done(String)`; 02-04's tail survived for exactly these two properties)
- **Fix:** Every probe threads S = `&mut Cell<String>` (wait_restart uses a borrowing Witness struct holding the same `&mut Cell`); the terminal state string rides the outer Cell via `take()` after poll consumes the state; module header documents the constraint
- **Files modified:** crates/ignition-core/src/actions/restart.rs
- **Verification:** All 13 lifecycle wiremock scenarios + 9 CLI goldens green; the floor timing proofs pin the semantics
- **Committed in:** 0525fdf (Task 1 commit)

**2. [Rule 3 - Blocking] Restart flag names mismatched the research-documented surface**
- **Found during:** Task 2 (CLI goldens)
- **Issue:** The plan's field sketch (`timeout_secs`/`interval_secs`) renders as `--timeout-secs`/`--interval-secs` under clap's kebab casing — inconsistent with the research's `ign restart [--wait [--timeout S]]` and with the `--interval`/`--timeout` flags every other poll command (logs -f, wait) already uses
- **Fix:** Renamed the fields to `timeout`/`interval` (flags `--timeout`/`--interval`, value_name SECS) — one consistent flag vocabulary across the whole wait family
- **Files modified:** crates/ignition-cli/src/cli.rs, crates/ignition-cli/src/main.rs
- **Verification:** `ign restart --wait --interval 1` lifecycle golden passes
- **Committed in:** d38e067 (Task 2 commit)

**3. [Rule 1 - Bug] Trait-growth stub script mis-nested one double's stubs**
- **Found during:** Task 1/3 (stub insertions into the 7 test doubles)
- **Issue:** The scripted insertion assumed 8-space method indent; logs.rs's AuthRig lives INSIDE a test fn at 12-space depth — the stubs landed outside the impl block
- **Fix:** Repaired the nesting manually; the Task-3 insertion script derives the indent from the anchor line
- **Files modified:** crates/ignition-core/src/actions/logs.rs
- **Verification:** cargo test --workspace green, clippy -D warnings clean
- **Committed in:** 0525fdf / 0f738e4

---

**Total deviations:** 3 auto-fixed (2 blocking, 1 bug)
**Impact on plan:** All three were correctness-preserving refinements inside the plan's own contracts — no new error variant, no envelope change, no exit-code change (error.rs untouched all plan; the enumerated test == README table consistency check holds). The doctor signature gained `profile_url: &str` + `credential_present: bool` beyond the plan's `doctor(api_or_none, opts)` sketch (the TCP-dial check needs the URL and the 401 diagnosis needs to distinguish unconfigured from unrecognized) — additive at the action seam only.

## Issues Encountered
- wiremock's scoped MockGuard gotcha bit once more (an unbound guard UNMOUNTS the fixture at statement end) — the lifecycle golden now binds `_post_guard` with a comment citing the 02-01 note
- serde_json::Value maps are key-sorted on round-trip — the checks[]-keys assertion compares a sorted set, while the real serialization order (struct declaration order: name, status, detail, hint) is pinned by the golden
- The doctor's rig row and summary counts vary by machine (Docker present or not) — solved with [..] glob elision in goldens rather than an injectable flag (the plan's "no other options" constraint)

## Authentication Gates

None — wiremock covers the contract; the new live check is skip-by-default and inherits the env contract (IGNITION_LIVE_URL [+ IGNITION_LIVE_TOKEN]; see [02-USER-SETUP.md](./02-USER-SETUP.md)).

## User Setup Required

None beyond 02-01's opt-in live suite.

## Next Phase Readiness
- Phase 2 is COMPLETE (5/5 plans): every HLTH requirement shipped — status/modules/metrics/sessions/connections/logs/loggers inspectable, sessions terminable, logs tail-able/downloadable, the gateway carefully restartable with race-free waits, and doctor diagnosing the whole setup
- Exit taxonomy unchanged since 02-01's additions (error.rs untouched in 02-04 AND 02-05); the enumerated test + README table stay in sync
- For Phase 3 (projects): restart --wait is the post-import readiness primitive; doctor's scan/projects probe already exercises the project-rescan surface
- For Phase 4 (rig): the doctor's rig row is the seam (currently a docker --version presence check; Phase 4 replaces the detail with real rig detection)
- For Phase 5 (WebDev): webdev_route_status is the presence probe; route creation/management extends the same path family

## Self-Check: PASSED
