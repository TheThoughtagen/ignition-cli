---
phase: 22-transport-plan-pull-verify
plan: 09
subsystem: deploy
tags: [rig-captures, live-gate, ignition-8.3.6, log-window, read-back, capture-driver]

requires:
  - phase: 22-transport-plan-pull-verify
    provides: deploy_pull / load_declared / deploy_plan over the local transport (22-01, 22-08), skip-reason vocabulary (22-03, 22-08)
provides:
  - "capture22.sh: loopback-only rig driver (up22, setup22, header22, cv1, cv2, cv3) with the authority cut at the first / ? # or backslash"
  - "22-LIVE-CAPTURES.md: stamped blocks CV1-CV3 and Decisions CV1, CV2, CV3 that 22-10 follows"
  - "live_deploy_plan.rs: #[ignore] gate proving pull, render, plan is empty on a real 8.3.6 gateway, and that plan leaves the scan-lock alone"
affects: [22-10, 22-11, 22-14, 23]

actuals:
  tokens: 45970
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "Capture-before-code: every block is a script-stamped command line plus a script-taken UTC time, then the verbatim bytes; the token is written as the literal text $IGN_TOKEN_CAP_A"
    - "A comparison that must not print values is a driver subcommand (cv2cmp) run through shcap, so the evidence shows a short command and path-only output"

key-files:
  created:
    - crates/ignition-core/tests/fixtures/live-rig-deploy/capture22.sh
    - crates/ignition-core/tests/live_deploy_plan.rs
    - .planning/phases/22-transport-plan-pull-verify/22-LIVE-CAPTURES.md
  modified: []

key-decisions:
  - "CV1: the REST logs route returns the RIG.md line verbatim; match message containing 'will not be loaded' plus 'Resource <module>/<type>/<name> ', client-side (the logger query parameter is not a prefix filter); window start = lastScanTimestamp - lastScanDuration - 1000 ms on the gateway clock; wait 5 s after scan complete before reading; the line is logged once, and the REST view still lists the resource"
  - "CV2: REST config equals the file config.json for 35 of 46 REST items and for 1 more after N1; three normalisations (N1 null members, N2 security-levels name-keyed subset, N3 files:[] resource) and two skip classes (system collection, no REST read route)"
  - "CV3: a scan does not rewrite plain-file-write resource files; post-scan bytes equal rendered bytes over 30 s"

patterns-established:
  - "Rig live gates take the host data dir via IGN_DEPLOY_RIG_DATA and read it through LocalFsTransport; the REST token is used only for the name guard and the lock reads"

requirements-completed: []
requirements-advanced: [DPLN-06, DPLN-07]

duration: 95min
completed: 2026-10-14
status: complete
---

# Phase 22 Plan 09: Rig captures CV1-CV3 and the live pull/render/plan gate Summary

**Three rig facts that verify depends on (the "will not be loaded" log shape and window, per-type read-back equality, post-scan byte stability) are captured on a real 8.3.6 gateway with decisions, and `deploy pull` then `render` then `plan` reports `has_changes: false` against that gateway (48 units same).**

## Accomplishments

- **`capture22.sh`** (`crates/ignition-core/tests/fixtures/live-rig-deploy/`): copies the Phase 21 `cap`/`shcap` stamping style, but every block now has the command and a script-taken UTC time on a line before the fence. `assert_local` runs before any file is touched and cuts the authority at the first `/`, `?`, `#` or `\` before stripping userinfo; it also refuses whitespace and control characters. Probes: `BASE='http://evil.example#@localhost:18290'` exits 2, `http://user@evil.example/@localhost` exits 2, `http://localhost.evil.example:18290` exits 2. The bind directory is `$HOME/.cache/ign-deploy-rig/data/config/resources`, outside the repo, and `up22` refuses any other location.
- **`22-LIVE-CAPTURES.md`** (2784 lines): header, `## CV1` (a through e), `## CV2`, `## CV3`, `## Decisions`. Image `inductiveautomation/ignition:8.3.6`, gateway `ign-deploy-rig`, `deploymentMode` production. The token header appears only as the literal `$IGN_TOKEN_CAP_A`; `grep -cE 'X-Ignition-API-Token: [A-Za-z0-9]'` prints 0, and a grep for the token key value prints 0.
- **`live_deploy_plan.rs`**: `pull_render_plan_is_empty_live` and `plan_takes_no_lock_live`, both `#[ignore]` with a quiet skip unless `IGNITION_LIVE_DEPLOY=1`; a loopback URL guard (same shape as `live_deploy_api.rs`, with its own two offline guard tests) and the `ign-deploy-rig` name guard run before any other call. The test uses `deploy_pull` (no `--yes`, empty `gateways/`), `load_declared`, `deploy_plan` and `scan_lock_info(ScanKind::Config)`. It never touches `TreeMirror`, `mirror_for_node`, `stage` or `commit`; `write_half_unreachable` stays green.

## Decisions CV1 to CV3 (quoted from the captures)

### CV1: the "will not be loaded" line (research A6)

- Shape: `{"timestamp":1792008016254,"loggerName":"NamedResourceHandler-ignition/database-connection","level":"WARN","message":"Resource ignition/database-connection/CV1_NOUUID_DB does not have required UUID attribute, will not be loaded","mdc":{"collection":"production"}}`. Two identical entries per resource per scan, a few ms apart. No `stack`. The RIG.md text is returned verbatim, so A6 holds for the text.
- **Match rule**: `message` contains `will not be loaded` and `Resource <module>/<type>/<name> ` of a declared resource; match client-side on `minLevel=WARN`. The research's text is confirmed; filtering by logger is not possible: `logger=NamedResourceHandler` returned `items: []` in the window where `minLevel=WARN` returned both entries.
- **Window**: `startTime = lastScanTimestamp - lastScanDuration - LOG_WINDOW_SLACK_MS`, **`LOG_WINDOW_SLACK_MS = 1000`**, gateway clock only, no `endTime`. The entries are stamped 10 to 126 ms AFTER `lastScanTimestamp`. Narrower than the 30 s the research sketched, to avoid re-reading earlier scans.
- **Settle**: the entry became readable 516 to 1450 ms after the scan reported complete (516, 857, 644, 518, 1450). Verify must wait **`LOG_SETTLE_MS = 5000`** before reading; an earlier empty answer means nothing.
- **Logged once**: a second scan with the resource unchanged logs nothing, and a query spanning both scans returns only the first scan's two entries. A later `verify` cannot rediscover the line.
- **The REST view does not show the failure**: the uuid-less database-connection is in the REST list with its full config, so `read_back` does not subsume `loaded`.
- **Not every uuid-less resource logs**: a uuid-less `database-translator` produced no entry and is loaded. `loaded` must not be read as "all declared resources loaded" for types that do not need a uuid.

### CV2: REST effective view versus file `config.json` (research A7)

46 REST items compared (non-tag types on the rig after the tracer render). Result: 35 equal as JSON values, 1 equal after N1, 1 differs (N2), 1 has no config.json (N3), 6 are `system` collection, 2 have no read route; three normalisations for 22-10 `read_back`:

| Rule | Type | Normalisation |
|---|---|---|
| N1 | `database-translator/POSTGRES` (tracer render) | drop members whose file value is JSON `null` before comparing (`createAutoIncSequence`, `createAutoIncTrigger` are absent in REST). Stated for all types |
| N2 | `ignition/security-levels` | REST is the file tree merged with gateway-provided levels, array order differs; compare as a name-keyed subset (the file is a subset: true) |
| N3 | `ignition/translations` | `resource.json` has `files: []` and no `config.json`; REST config is built from the four `resource.json` attributes plus an empty `terms` |
| skip | `perspective/themes/{dark,light}`, `database-driver/{MariaDB,Microsoft SQLServer,PostgreSQL}`, `schedule/Always` | effective collection `system`: gateway-provided, not file-backed |
| skip | `opcua/one-time`, `ignition/images` | no REST read route (list and singleton both 404) |

Not exercised on this rig (the table says nothing about them): `database-connection` config, `oauth2-client`, `alarm-notification-profile`, `secret-provider`, `service-connector`, `roster-config`, `holiday` and any secret-typed field. The Pitfall 6 remedy stays a per-type row, not looser equality; 22-14 should extend the table on the prod-sized payload.

### CV3: post-scan byte stability

The four files of the tracer render placed with `cp -R` into `production/` have the same sha256 in the render directory, right after a scan and at every 5 s step to +30 s, and `cmp` reports all four identical. 22-10 `files` can compare post-scan bytes with the rendered bytes directly. Limits: four signature-bearing files, one scan; gateway-written tag files and gateway-created resources are not covered.

## Live run (Task 2)

Rig: image `inductiveautomation/ignition:8.3.6`, gateway `ign-deploy-rig`, mode `production`, `config/resources` bind-mounted from `$HOME/.cache/ign-deploy-rig/data/config/resources`, state at the end of CV3 (the four tracer files in `production/` beside the commissioned `core`). Command:

```text
IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 IGNITION_LIVE_TOKEN="$IGN_TOKEN_CAP_A" \
  IGN_DEPLOY_RIG_DATA="$HOME/.cache/ign-deploy-rig/data" IGN_DEPLOY_RIG_MODE=production \
  cargo test -p ignition-core --test live_deploy_plan -- --ignored --nocapture
```

Output (two runs, identical results):

```text
pull: 95 files written, manifest generated, 7 skipped
  skipped ignition/images (type_without_rule)
  skipped ignition/keyboard_layout (type_without_rule)
  skipped ignition/opc-connection/Ignition OPC UA Server (embedded_secret)
  skipped ignition/security-levels (denied_type)
  skipped ignition/security-properties (denied_type)
  skipped ignition/translations (type_without_rule)
  skipped ignition/user-source (denied_type)
plan: added 0 changed 0 removed 0 same 48 unmanaged 1 has_changes false
PASS pull_render_plan_is_empty_live
PASS plan_takes_no_lock_live: lock free before and after plan
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out
```

Success criterion 3 holds against a real gateway: 48 units `same`, the one `unmanaged` unit is the skipped `embedded_secret` OPC connection, and every unmanaged entry is asserted to be a unit pull reported as skipped. The seven skip reasons are all from the locked vocabulary.

## Teardown

`docker compose -p ign-deploy-rig down -v` exited 0 (container, volume `ign-deploy-rig_gw-data` and network removed; afterwards 0 containers, 0 volumes, 0 networks carry the project label and port 18290 refuses). The bind data directory `$HOME/.cache/ign-deploy-rig/data`, `render22`, `cv2-bodies` and the stale `tokens.env` were removed with `/bin/rm -rf`. The rig was NOT handed to 22-10. Rebuild with `capture22.sh up22` and `setup22`.

## Verification

- `bash -n capture22.sh`: 0. Loopback probes (above): exit 2 each.
- `cargo test -p ignition-core --test live_deploy_plan` (offline): 2 passed, 2 ignored. `cargo test --workspace`: 2027 passed, 0 failed, 43 ignored (2025 before this plan plus the 2 offline guard tests). `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check`: clean.
- `PLAN_GOLDEN` and `PLAN_GOLDEN_LEDGER` untouched; `write_half_unreachable` green; no product code changed.
- Build environment: private `CARGO_TARGET_DIR=~/Library/Caches/cargo-target-ign-cli` (as in 22-08).

## Task Commits

1. **Task 1: rig captures CV1-CV3, `capture22.sh`, Decisions:** `3acf680` (test)
2. **Task 2: live gate `live_deploy_plan.rs`:** `96d2b8c` (test)

## Deviations from Plan

**1. [Judgement] Window slack 1000 ms instead of a wide slack.** The plan's text leaves `LOG_WINDOW_SLACK_MS` to the capture. CV1b/CV1d show the entries sit after `lastScanTimestamp`, so a wide slack only adds stale lines; decision CV1 sets 1000 ms and adds the 5 s settle the plan did not anticipate.

**2. [Judgement] Added `lagprobe` and a second/clean-scan sequence to CV1.** The first CV1 attempt (translator, then database-connection) showed the log entries were not readable immediately after scan completion, so CV1 now records the lag over five scans and the "logged once" fact (a query spanning both scans). The first attempt's section was discarded and the whole capture re-run on the same rig; the committed file is one clean run (header to CV3).

**3. [Judgement] The uuid-less `database-translator` does not log.** The plan's step 3 starts with the translator and falls back to a database-connection "if the translator yields no such line". It yielded none, so CV1b uses the connection; both are in the evidence and in the decision.

**4. [Judgement] `cv2cmp` is a driver subcommand.** The comparison needs jq programs too long to show as a `shcap` command line, so `bash "$CAPTURE22" cv2cmp` runs it. The block still carries a command and a time.

**5. [Judgement] The live test uses `deploy_pull` for its input.** The plan has the same shape; the test additionally asserts that every `unmanaged` plan entry is a unit pull reported as skipped.

**6. [Process] The plan's `type: execute` tasks have no separate RED commit** (the artifacts are scripts and an ignored test; the offline test run is the check).

**Total deviations:** 5 judgement calls, 1 process note. No bugs, no architectural change, no new packages.

## Known limitations and residuals

- CV1 was captured with one type that needs a uuid (database-connection) and one that does not (database-translator). Which other types warn is unknown; the match rule does not depend on it.
- CV2 covers 46 items of the types present after commissioning plus the tracer render. Types listed under "Not exercised" are open for 22-14.
- CV3 covers four files and one scan.
- `plan_takes_no_lock_live` shows the lock free before and after; `plan` has no gateway session at all, so this is a regression guard for the lock rather than a discriminating test.
- The live gate was run once on the final rig state; the rig is gone, and no log of the run is committed beyond this summary.
- Windows paths of the transport were not run here (unchanged from 22-01 to 22-08).

## Requirement bookkeeping

No requirement is marked complete. DPLN-06 is **advanced** (the "from a live gateway" half is now proven for the local transport on a rig; 22-14 still captures WFLWVM01 and the prod-sized payload, per the override). DPLN-07 is **advanced** (the log, read-back and file facts verify rests on are decided; the verb is 22-10 and 22-11). DPLN-02 was already complete and stays so; this plan adds its live evidence.

## Known Stubs

None.

## Threat Flags

None beyond the plan's register. T-22-40 (BASE redirect): the `evil.example#@localhost` probe exits 2 before any file is touched, hardened to also cut at a backslash. T-22-41 (token in evidence): token only as a header and as the literal text `$IGN_TOKEN_CAP_A`; `shcap` refuses a token value in an argument or in output; both greps print 0. T-22-42 (rig config in evidence): CV2 records JSON paths and keys, not values (the committed REST list answers carry the rig's synthetic config, as in Phase 21). T-22-43 (leftover rig): `down -v` exited 0 and the bind directory was removed.

## Self-Check: PASSED

- Files present: `capture22.sh` (executable), `22-LIVE-CAPTURES.md` with `## CV1`, `## CV2`, `## CV3`, `## Decisions`; `live_deploy_plan.rs` with two `#[ignore]` tests.
- Commits `3acf680` and `96d2b8c` are in `git log`.
- `.planning/config.json` and `.planning/research/.cache/` untouched and uncommitted.
