---
phase: 05-webdev-backend-tag-operations
plan: "03"
subsystem: api
tags: [webdev, ignition, zip, wiremock, snapbox, e2e, secrets]

# Dependency graph
requires:
  - phase: 05-webdev-backend-tag-operations/05-01
    provides: embedded WebDev route bundle (ROUTE_FILES, SCRIPT_EXEC_TEMPLATE, ROUTE_BUNDLE_VERSION) + the machine-code envelope contract
  - phase: 05-webdev-backend-tag-operations/05-02
    provides: zip 8.6 workspace dependency + member-surgery helper precedents
  - phase: 03-project-operations/03-02
    provides: project_import machinery (application/zip + overwrite query, 300 s timeout) that deploy rides
provides:
  - client/webdev.rs seam — webdev_route_call (POST action dispatch, 200-body envelope oracle) + webdev_route_probe (405/402/401/200-discrimination) + build_deploy_zip
  - actions/webdev.rs — webdev_deploy (secret lifecycle, overwrite-import, no pre-flight create), webdev_status (per-route matrix), webdev_precondition (the exit-6 refusal every tag command in 05-04+ runs first)
  - Profile.webdev_secret (0600 config store, skip-when-none — the one value-carrying profile field)
  - `ign webdev deploy` / `ign webdev status` CLI family with goldens + live e2e gate
  - 4 additive exit-6 slugs (routes_not_deployed, webdev_unlicensed, route_version_mismatch, webdev_route_error) in both exit-table places
  - doctor re-pinned to 405=absent semantics (research Pitfall 1 closed)
affects: [05-04, 05-05, 05-06, tags-family, tag-commands]

# Tech tracking
tech-stack:
  added: []  # no new deps — zip came in 05-02; secret generation is /dev/urandom zero-dep
  patterns:
    - "route envelope oracle: HTTP 200 is NEVER a success verdict — only {ok,data|error} body decides (denials ride 200)"
    - "probe discrimination: the status code IS the answer (405=absent, 402=unlicensed, 401=auth-gated) — bypasses classify"
    - "fail-closed secret template: placeholder excluded from the plain manifest so unsubstituted deploy is impossible by construction"
    - "status-is-a-read: degradation is data, exit 0 whenever the sweep completes (the doctor precedent) — the exit-6 refusal belongs to DEPENDENT commands"

key-files:
  created:
    - crates/ignition-core/src/client/webdev.rs
    - crates/ignition-core/src/actions/webdev.rs
    - crates/ignition-core/tests/webdev_contract.rs
    - crates/ignition-cli/tests/contract_webdev.rs
    - crates/ignition-cli/tests/e2e_webdev.rs
  modified:
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-core/src/actions/doctor.rs
    - crates/ignition-core/src/config/profile.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/contract_doctor.rs
    - README.md

key-decisions:
  - "WebDev wire protocol LOCKED at /system/webdev/{project}/cli/{route} POST action dispatch — denials ride HTTP 200, so the body envelope {ok,data|error} is the ONLY success oracle; classify handles transport/status but never the 200 verdict"
  - "Probe matrix: 405=Absent (the live-proven 8.3 marker — NOT 404), 402=Unlicensed (trial-expired module), 401/403=AuthGated, 200-body=Present|Denied — doctor's Phase-2 404 assumption re-pinned everywhere including its CLI golden"
  - "Deploy = project zip import with overwrite=true, NO pre-flight create (Pitfall 10); NOT --yes-guarded — the dedicated ign-cli project is CLI-OWNED (born from the deploy zip, wholesale-replaced every deploy); user projects are never touched"
  - "scriptExec LOCKED posture: secret generated from /dev/urandom (32-byte hex, zero-dep), persisted 0600 in the profile config BEFORE upload, substituted into the template (never ships unsubstituted — excluded from the plain manifest); the route fail-closes on every action incl. version; secret appears in exactly one place (the baked zip member) — action-level AND binary-level redaction proofs"
  - "webdev status is a READ: exits 0 whenever the sweep completes; per-route degradation {absent,unlicensed,auth_gated,secret_mismatch,version_mismatch} is data; scriptExec probed only when a secret is configured and never gates ok"
  - "webdev_precondition (tags-route handshake probe) is the cheap exit-6 refusal every WebDev-DEPENDENT command runs: routes_not_deployed / route_version_mismatch (direction-aware hint: redeploy vs update ign) / webdev_unlicensed — no auto-upgrade"
  - "Profile gains webdev_secret — the ONE value-carrying profile exception (skip-when-none so existing configs/goldens do not churn)"

patterns-established:
  - "Route-call seam: extra_headers param carries per-route auth (the X-Ignition-CLI-Secret) without a second trait method per header need"
  - "always_on_routes() DERIVED from ROUTE_FILES so the deploy set, the status sweep, and the manifest can never drift apart"
  - "Direction-aware version-mismatch hints via semver compare — same slug, two message variants (redeploy vs update ign)"

# Metrics
duration: ~400min (across two executor sessions — the first died on a rate limit after Tasks 1-2; this session recovered, finished Task 3)
completed: 2026-08-24
---

# Phase 5 Plan 03: WebDev Client Seam + Deploy/Status Summary

**WebDev route seam (POST action dispatch with 200-body envelope oracle, 405/402/401 probe matrix, deploy zip builder over the embedded bundle) + `ign webdev deploy`/`status` with the version-negotiation refusal matrix, scriptExec secret lifecycle, and doctor's 405 re-pin**

## Performance

- **Duration:** ~400min total (two sessions: Tasks 1–2 committed 09:25–09:38; recovery session completed Task 3 by 16:08)
- **Started:** 2026-08-24T15:25:16Z (first task commit)
- **Completed:** 2026-08-24T16:08:01Z
- **Tasks:** 3
- **Files modified:** 35 (across the whole plan)

## Accomplishments
- The hinge seam every tag command in 05-04..06 rides: `webdev_route_call` + `webdev_route_probe` + `build_deploy_zip`, wiremock-pinned across the full discrimination matrix (13 core contract tests)
- Deploy/status actions with the secret lifecycle (generate → persist 0600 BEFORE upload → substitute → overwrite-import; rotate on demand) and redaction proofs at both the action and binary level
- `ign webdev` CLI family with snapbox goldens and the opt-in live e2e gate (deploy → status → redeploy clean-replace → scriptExec security posture probed RAW on the wire)
- doctor re-pinned to 405=absent (research Pitfall 1 closed at every layer: action, core fixtures, CLI golden)

## Task Commits

Each task was committed atomically (Tasks 1–2 by the prior executor session; Task 3 + recovery by this one):

1. **Task 1: client seam — route_call, probe, deploy zip builder, error slugs** — `eabb061` (feat)
2. **Task 2: deploy + status actions, secret lifecycle, doctor re-pin** — `fbe5e9e` (feat)
3. **Task 3: CLI wiring, goldens, live e2e gate, README** — `1c1ddb8` (feat) + `1f71c53` (style: workspace fmt normalization)

**Plan metadata:** (this commit)

## Files Created/Modified
- `crates/ignition-core/src/client/webdev.rs` — route_url, RouteProbe enum, parse_route_body, denial_to_error, build_deploy_zip (scriptExec fail-closed)
- `crates/ignition-core/src/client/mod.rs` — trait methods webdev_route_call/webdev_route_probe + ReqwestGatewayApi impl
- `crates/ignition-core/src/error.rs` — 4 additive exit-6 slugs, both exit-table places
- `crates/ignition-core/src/actions/webdev.rs` — webdev_deploy/webdev_status/webdev_precondition + secret lifecycle + unit matrix (incl. redaction proof)
- `crates/ignition-core/src/actions/doctor.rs` — check_webdev re-pinned to webdev_route_probe (405 semantics), fixtures moved to 405-shape
- `crates/ignition-core/src/config/profile.rs` — webdev_secret field (skip-when-none)
- `crates/ignition-core/tests/webdev_contract.rs` — wiremock matrix + deploy-zip member/secret pins + import-request proof
- `crates/ignition-cli/src/{cli,main,render}.rs` — webdev command family, dispatch (profile-resolved, doctor precedent), human renderers (secret never prints)
- `crates/ignition-cli/tests/contract_webdev.rs` — binary goldens: deploy ×2 shapes, status all-present/absent-as-data/mismatch, redaction proof
- `crates/ignition-cli/tests/contract_doctor.rs` — webdev fixture re-pinned to POST ign-cli probe, 405
- `crates/ignition-cli/tests/e2e_webdev.rs` — live gate (#[ignore], quiet-skip, mutations opt-in)
- `README.md` — webdev command rows, refusal-matrix table, scriptExec security section, exit-table rows, doctor 405 note

## Decisions Made
See key-decisions in frontmatter — the load-bearing ones: the 200-body envelope oracle, the CLI-owned-project no-guard deploy contract, the status-is-a-read lock, and the shared-secret honesty posture.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] contract_doctor.rs golden left on the retired 404 wire shape**
- **Found during:** Task 3 recovery (full-suite verification)
- **Issue:** fbe5e9e re-pinned doctor's core fixtures to the 405/POST probe but missed the CLI-level golden (`doctor_json_shape_and_flags` still mocked GET 404 on the old path → webdev check flipped warn→fail, suite red)
- **Fix:** re-pinned the fixture to POST `/system/webdev/ign-cli/cli/stacked` answering 405; assertion comment updated to match
- **Files modified:** crates/ignition-cli/tests/contract_doctor.rs
- **Verification:** full `cargo test -p ignition-cli` green
- **Committed in:** 1c1ddb8

**2. [Rule 3 - Blocking] prior session's contract_webdev.rs had a borrow compile error and unverified goldens**
- **Found during:** Task 3 recovery (the rate-limit death left the test file unwritten-to-disk-state: one `server` vs `&server` compile error, two goldens with wrong column widths)
- **Fix:** borrow fix + `SNAPSHOTS=overwrite` regenerated the two table goldens from actual (correct) render output
- **Files modified:** crates/ignition-cli/tests/contract_webdev.rs
- **Verification:** all 5 contract tests green
- **Committed in:** 1c1ddb8

**3. [Rule 3 - Blocking] workspace rustfmt drift**
- **Found during:** Task 3 recovery (`cargo fmt --check` failed on HEAD)
- **Issue:** the committed 05-03 files and several unrelated modules had drifted from canonical rustfmt layout
- **Fix:** one zero-behavior-change `cargo fmt` pass over the workspace, committed separately as `1f71c53` so the feature commits stay scoped
- **Verification:** full workspace suite green post-format (33 suites); `cargo fmt --check` clean
- **Committed in:** 1f71c53

---

**Total deviations:** 3 auto-fixed (1 bug, 2 blocking)
**Impact on plan:** All recovery work — no scope creep; the plan itself executed exactly as written.

## Issues Encountered
- The prior executor session died on a rate limit after committing Tasks 1–2 but mid-Task-3 (uncommitted cli/render/main wiring + a non-compiling test). This session mapped the commits against the plan, verified Tasks 1–2 green, fixed the in-flight Task 3 remnants, and completed the remaining work (e2e gate, README, doctor golden).

## Authentication Gates

None — the live e2e gate is opt-in (`IGNITION_LIVE_URL`/`IGNITION_LIVE_TOKEN`/`IGNITION_LIVE_MUTATIONS`) and quiet-skips without them; the plan's user_setup documents the env recipe.

## User Setup Required

Optional only — live gates are `#[ignore]` and quiet-skip. See the plan frontmatter (`IGNITION_LIVE_URL`, `IGNITION_LIVE_TOKEN`, `IGNITION_LIVE_MUTATIONS=1`) to run `cargo test -p ignition-cli --test e2e_webdev -- --ignored` against a commissioned 8.3.x rig.

## Next Phase Readiness
- The seam 05-04..06 depend on is DONE: `webdev_precondition` is the one-call refusal every tags-family command runs; `webdev_route_call` is the transport; `ROUTE_BUNDLE_VERSION` negotiation is pinned end-to-end
- scriptExec's LOCKED posture shipped (deploy-on-request, secret-gated, fail-closed) — `ign script run` (Phase 7) rides the stored-secret call path
- The live deploy gate remains unrun against a real rig this session (env vars unset) — same opt-in status as the other live suites; contract coverage is complete without it

## Self-Check: PASSED
