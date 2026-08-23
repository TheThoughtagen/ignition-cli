---
phase: 04-rig-lifecycle-trial-state
plan: 04
subsystem: rig
tags: [rig, gwbk, backup, restore, streaming, octet-stream, manifest, wiremock, guard, e2e-gate]

# Dependency graph
requires:
  - phase: 04-rig-lifecycle-trial-state
    plan: 01
    provides: RigPlan discovery, gateway_url_from, rig CLI tree + profile:null contract, commissioned_wait
  - phase: 04-rig-lifecycle-trial-state
    plan: 02
    provides: guard-before-discovery binary pin pattern, reset/README rig conventions
  - phase: 04-rig-lifecycle-trial-state
    plan: 03
    provides: rig_gateway_client (rig-URL client + IGNITION_TOKEN sourcing), config.active gateway-verb echo
  - phase: 03-project-operations
    plan: 02
    provides: download_to_file (the streaming pipeline), project_export_to_file, ExportMeta, encode_segment
provides:
  - client/backup.rs — the gwbk wire: roaming download (streamed, Accept octet-stream, 300s) + raw octet-stream restore POST (4 explicit params, NOT multipart, 300s)
  - backup_download/backup_restore trait methods (stubs added to all 9 test doubles)
  - rig_snapshot — timestamped dir (std-only stamp), gwbk first, per-project exports (injective encoded names), manifest.json with BOTH exclusion notes verbatim
  - rig_restore — pre-checked (exit 2), POST + witnessed RUNNING wait (300s floor), token-clobber warning first in data
  - `ign rig snapshot [-o DIR]` / `ign rig restore --file <gwbk>` (5th guarded destructive verb, binary-pinned ordering)
  - e2e_rig.rs — the TWO-SIDED round-trip gate (pre-witness survives + post-snapshot marker gone), env-gated quiet skip
affects: [phase-05 (gwbk as the tag-config carrier between TAGS work), phase-06 TUI, phase-07 interop]

# Tech tracking
tech-stack:
  added: [] # NO new crates — the std-only timestamp rule honored (civil-from-days)
  patterns:
    - "Minimal parameterization over forking: download_to_file gained an optional Accept header rather than a second chunk loop — the ONE streaming body-consumption site holds"
    - "Composition honesty in the artifact: the manifest carries BOTH exclusion notes verbatim, so no reader can mistake scope for a silent drop"
    - "Wait floors via max(): restore_deadline(timeout).max(300) — an explicit short --timeout cannot buy an unknown-state mid-restart report (RESTART_FLOOR precedent)"
    - "is_file() as the portable readability gate: File::open on a directory SUCCEEDS on macOS — only the read fails, which would be mid-network"
    - "Port-collision hazard documented: rig gateway verbs address derived localhost:<port> — another stack holding the port silently addresses the WRONG gateway (verify with rig status first)"

key-files:
  created:
    - crates/ignition-core/src/client/backup.rs
    - crates/ignition-core/tests/backup_contract.rs
    - crates/ignition-cli/tests/e2e_rig.rs
  modified:
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/src/actions/rig.rs
    - crates/ignition-core/src/actions/{projects,sessions,version,doctor,connections,inspect,logs}.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - README.md
    - crates/ignition-cli/tests/contract_rig.rs

key-decisions:
  - "Restore deadline floors at 300s (max clamp) — Pitfall 6's unknown-state risk outweighs an explicit short --timeout; pinned by restore_deadline unit test"
  - "The Accept header rides download_to_file via a minimal optional parameter rather than bending the helper or forking a second streaming site — one body-consumption site preserved"
  - "The roaming query rides the path constant into download_to_file's single path param (url join preserves it) — no helper signature churn for one fixed param"
  - "Project export file names percent-encode the project name (encode_segment — INJECTIVE and filesystem-safe): My Project → My%20Project.zip, no collisions"
  - "gateway_info failure during snapshot degrades to ignition.version: null IN THE MANIFEST (the snapshot already succeeded — the artifact carries the gap visibly)"
  - "Snapshot/restore source creds from IGNITION_TOKEN only (the backup route 401s unauth — unlike the trial endpoints); missing token = exit 3, the trial-reset refusal shape"
  - "rig_restore takes rig_url as a parameter (the trial_reset signature precedent — the shared wait's subject message needs it) over the plan's 3-param sketch"
  - "The live round-trip gate did NOT run: the ignition-devops rig is down and its ports (9088/9043) are held by another actively-used compose stack — stopping it is a human decision, not an auto-fix"

patterns-established:
  - "Snapshot-result verification via the manifest (the artifact IS the record) — e2e reads manifest.json for the gwbk name instead of re-running the command"
  - "Two-sided restore pins assert via list-absence when no find subcommand exists (the e2e_projects list-contains pin, inverted)"

# Metrics
duration: 37min
completed: 2026-08-23
---

# Phase 4 Plan 4: Snapshot & restore Summary

**`ign rig snapshot` (roaming gwbk streamed to disk + per-project exports + a manifest that names BOTH its exclusions verbatim) and the guarded `ign rig restore` (raw octet-stream POST → witnessed post-restore RUNNING with a 300s floor → the token-clobber warning first in data) — wire-pinned on the requests, round-trip pinned two-sided by an env-gated e2e gate**

## Performance

- **Duration:** 37 min
- **Started:** 2026-08-23T02:52:36Z
- **Completed:** 2026-08-23T03:29:47Z
- **Tasks:** 3
- **Files modified:** 17 (3 created, 14 modified)

## Accomplishments

- **The gwbk wire** (`client/backup.rs` + the two trait methods): the download rides the 03-02 `download_to_file` streaming pipeline VERBATIM (via one optional `Accept` parameter — the single body-consumption site preserved; `type=roaming` rides the path constant), 300 s per-request class, `Accept: application/octet-stream`. The restore POSTs the file bytes as a RAW `application/octet-stream` body — NOT multipart — with all four scope params (`restoreDisabled`, `disableTempProjectBackup`, `renameEnabled`, `restoreLocal`) EXPLICIT false on the query string (the server is the authority on defaults; agents see what was sent). Wiremock pins assert the REQUESTS: byte-identical read-back for the download (never a binary snapbox golden), content-type + all four params + body-equals-file for the restore, and the live-verified 401-HTML unauth shape classifying `auth_rejected` (exit 5).
- **`rig_snapshot`**: timestamped directory (`./ign-rig-snapshots/<rig>-<yyyyMMdd-HHmmss>/`, std-only civil-from-days stamp — no chrono), gwbk FIRST, per-project exports through the 03-02 machinery with INJECTIVE percent-encoded file names, and `manifest.json` asserted EXACTLY in tests — `{rig, taken_at, ignition: {version}, gwbk, projects: [{name, file}], notes}` with BOTH exclusion notes verbatim (trial clock NOT captured; tag-provider bulk export = Phase 5). A failed `gateway_info` degrades to `version: null` in the manifest — the artifact carries the gap.
- **`rig_restore`**: pre-checks (regular file + non-empty) refuse exit 2 `invalid_input` BEFORE any network work; the POST's 2xx is only ACCEPTANCE — success is a WITNESSED StatusPing→RUNNING via the shared `commissioned_wait` with the deadline floored at 300 s (`restore_deadline` max-clamp — an explicit `--timeout 30` cannot buy an unknown-state mid-restart report); the Pitfall-5 token warning (`API tokens may have been reset by restore — re-provision via gateway UI, then ign doctor`) rides DATA first, in every render mode.
- **CLI + gates**: `snapshot {-o}` / `restore {--file, --timeout}` wired on the trial precedent (rig-URL client, `IGNITION_TOKEN`-only cred sourcing — the backup route 401s unauth, missing token = exit 3; the `is_gateway_verb` generalization carries the `config.active` echo). Restore is the FIFTH guarded destructive verb: the guard fires before discovery, binary-pinned by exit-2-not-exit-7 in a no-rig cwd plus the `--yes` discovery-fallthrough proof. The round-trip e2e gate (`e2e_rig.rs`) pins TWO-SIDED — pre-witness survives, post-snapshot marker gone (the 03-03 replace-not-merge precedent, gwbk edition) — and observes (prints, never asserts) the trial clock across the restore.
- **Workspace state**: 30 suites green, clippy `-D warnings` clean, zero new dependencies.

## Task Commits

Each task was committed atomically:

1. **Task 1: Backup wire methods (streaming download + octet POST)** - `c202166` (feat)
2. **Task 2: snapshot + restore actions (manifest, wait, warning)** - `90afb73` (feat)
3. **Task 3: CLI wiring (restore guarded), goldens, README, e2e round-trip gate** - `a9caefc` (feat)
4. **Follow-up: README e2e port-collision warning** - `0ee5093` (docs)

**Plan metadata:** (see final docs commit)

## Files Created/Modified

- `crates/ignition-core/src/client/backup.rs` - path consts (roaming query on the path), the 300s class, the four-explicit-falses restore query builder
- `crates/ignition-core/src/client/mod.rs` - backup_download/backup_restore trait methods + impl bodies; download_to_file's optional `Accept` parameterization
- `crates/ignition-core/src/actions/rig.rs` - rig_snapshot/rig_restore + Snapshot/RestoreResult + RESTORE_WAIT_FLOOR_S/restore_deadline + civil-from-days stamp + the SnapshotRig fake + 8 new tests
- `crates/ignition-core/tests/backup_contract.rs` - the REQUEST-pinned wire contract (byte-identical read-back, octet POST, 401 classify)
- `crates/ignition-core/src/actions/{projects,sessions,version,doctor,connections,inspect,logs}.rs` - the 9-double stub chore
- `crates/ignition-cli/src/{cli,main,render}.rs` - Snapshot/Restore arms, guard (5th), is_gateway_verb echo, human renderers
- `crates/ignition-cli/tests/contract_rig.rs` - restore guard zero-work pin, --yes fallthrough, help surfaces
- `crates/ignition-cli/tests/e2e_rig.rs` - the two-sided round-trip gate (env-gated, quiet skip)
- `README.md` - snapshot/restore rows + the repeatable-state section + destructive-ops update + e2e runbook

## Decisions Made

- The restore wait deadline MAX-clamps to 300 s (plan: "deadline ≥300s") — a short explicit timeout is a footgun Pitfall 6 specifically warns about; `restore_deadline` is a tested pure function.
- `Accept: application/octet-stream` rides `download_to_file` via a minimal optional parameter (the plan asked for BOTH verbatim reuse AND the header — the parameterization keeps the one streaming site instead of a forked chunk loop).
- `rig_restore` grew a `rig_url` param over the plan's 3-param sketch — the shared wait's poll-subject message needs the URL, exactly the `trial_reset` signature precedent.
- Snapshot/restore creds are `IGNITION_TOKEN`-only with an up-front exit-3 refusal when absent (the trial-reset both-absent shape) — Basic cannot authenticate 8.3 `/data` routes, so there is no second rung.
- The key_link pattern `require_confirmation.*restore` is satisfied by the family's `guarded_operation` match (the 04-02/03 mechanism) rather than a direct call line; the ordering is proven by the stronger binary pin.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `download_to_file` needed an optional Accept header**
- **Found during:** Task 1 (backup_download wiring)
- **Issue:** The plan requires BOTH "rides download_to_file VERBATIM" and `Accept: application/octet-stream` — the 03-02 helper had no header parameter; the alternatives were forking a second chunk loop (violates the one-streaming-site invariant) or skipping the header (violates the postman contract).
- **Fix:** One minimal `accept: Option<&str>` parameter (export passes None, backup passes the octet-stream value) — the helper stays THE one streaming body-consumption site.
- **Files modified:** crates/ignition-core/src/client/mod.rs
- **Verification:** backup_contract.rs pins the header on the recorded request; all 30 suites green.
- **Committed in:** c202166 (part of task commit)

**2. [Rule 1 - Bug] Directory-as-gwbk passed the readability pre-check on macOS**
- **Found during:** Task 2 (pre-check tests — first run HUNG)
- **Issue:** The planned `File::open` readability probe SUCCEEDS on directories on macOS (only the later read fails) — a directory slipped past the pre-check into the network leg, and the first test run polled for the full 300 s restore floor.
- **Fix:** `metadata.is_file()` as the portable regular-file gate (checked before the empty check); the fake's Default ping state made explicit RUNNING so a misconfigured fake can never hang.
- **Files modified:** crates/ignition-core/src/actions/rig.rs
- **Verification:** `restore_prechecks_fail_before_any_network` covers missing/empty/directory with a zero-network-work assertion.
- **Committed in:** 90afb73 (part of task commit)

**3. [Rule 3 - Blocking] The never-RUNNING restore test was unrunnable under the 300 s floor**
- **Found during:** Task 2 (test design)
- **Issue:** A "STARTING until deadline" test through `rig_restore` would sit out the floor the plan itself mandates (300 s+ per run).
- **Fix:** The deadline-expiry→Rig-error mapping is already pinned by the up-cycle tests at a 1 s deadline; restore's own test uses a non-retryable probe error (immediate abort, same mapping), and the floor itself is pinned by the `restore_deadline` clamp test.
- **Files modified:** crates/ignition-core/src/actions/rig.rs
- **Verification:** `restore_wait_failure_is_a_rig_error` + `restore_wait_floor_is_300s_and_clamps` both green, full suite in 1.6 s.
- **Committed in:** 90afb73 (part of task commit)

---

**Total deviations:** 3 auto-fixed (2 blocking, 1 bug)
**Impact on plan:** All fixes were correctness/testability requirements. No scope creep — LOCKED contracts untouched (envelope, exit taxonomy additive-only via the existing InvalidInput slug, poll.rs diff-empty, no new dependencies).

## Issues Encountered

- **The live round-trip gate could not run (machine state, not a code issue):** the ignition-devops rig is DOWN and its port pair (9088/9043) is currently held by `whk-services-ignition-1` — a different compose stack that started up minutes into this execution (actively used; stopping it is a human decision, not a Rule 1-3 auto-fix). `ign-research` is a standalone container (not a compose rig; unknown admin creds). A live `rig status` smoke from `/tmp` confirmed discovery still resolves `ignition-devops` correctly (`services: []`, `ports_free: false` — exit-0 data). The gate ships as the plan's own #[ignore]/env-gated quiet-skip; the README runbook documents the precondition AND the newly-understood port-collision hazard (a derived `localhost:9088` URL silently addresses whichever stack holds the port — verify with `rig status` first). The gate's own env contract + instructions: `IGNITION_LIVE_URL` + `IGNITION_LIVE_TOKEN` + `IGNITION_LIVE_MUTATIONS=1`, run `cargo test -p ignition-cli --test e2e_rig -- --ignored`.
- One transient 300 s test hang during development (deviation #2's discovery) — killed, fixed, all subsequent runs green.

## Authentication Gates

None new — snapshot/restore use the documented `IGNITION_TOKEN` rig-family source; no token exists on either reachable rig right now (04-03's provisioning gap stands, 04-USER-SETUP.md), which is part of why the live gate is deferred.

## User Setup Required

No NEW setup beyond [04-USER-SETUP.md](./04-USER-SETUP.md) (still open from 04-03): provisioning an API token also unlocks this plan's live round-trip gate. To run it additionally ensure the rig is UP and its ports free (`ign rig status` — the dev machine's 9088/9043 are shared with the whk-services stack).

## Next Phase Readiness

- **RIG-04 complete — Phase 4 CLOSED.** All four requirements (RIG-01..04) stand: the project owns a self-managed gateway fixture (up/down/status/reset/logs/trial/snapshot/restore).
- The gwbk is now an `ign`-native artifact — Phase 5's tag work can treat it as the tag-config carrier between operations, and `manifest.json` is the machine-readable composition record.
- Open live items (documented, non-blocking): the two-sided round-trip gate awaits a reachable rig + provisioned token; the trial-clock-across-restore behavior is an observation point the gate prints but never asserts.
- Phase 5 (WebDev/tags) is next per the roadmap — its spikes (WebDev deploy mechanism, script-exec security posture, tag-history availability) are already logged in STATE.md.

---
*Phase: 04-rig-lifecycle-trial-state*
*Completed: 2026-08-23*

## Self-Check: PASSED

All key-files exist on disk; all 4 task commits (c202166, 90afb73, a9caefc, 0ee5093) verified in git log; must-have artifacts present (backup.rs, backup_contract.rs, e2e_rig.rs); full workspace green (30 suites) with clippy -D warnings clean; Cargo.toml untouched since 04-01 (no new dependencies).
