---
phase: 21-deploy-render
plan: 02
subsystem: testing
tags: [live-captures, ignition-8.3.6, config-api, tag-files, uuid, docker-rig, capture-driver]

requires:
  - phase: 20-config-api-client
    provides: loopback-only ign-deploy-rig compose rig, capture.sh driver, mint-token.sh, bind-mount viability (D11)
  - phase: 21-deploy-render (plan 01)
    provides: deploy_fixtures guard that every file under fixtures/deploy/ (including captured/) must pass
provides:
  - 21-LIVE-CAPTURES.md with seven C21 capture sections and Decisions CD1-CD7 (evidence only, no render code)
  - capture.sh Phase 21 subcommands (up21, setup21, header21, hdrrow, sect, shcap, v8uuid, check21, uuid8, c3b, c4, c4b, c4c, oq8, oq1, oq4, c5, c5log, c5x)
  - twelve gateway-written CAP21 tag files under fixtures/deploy/captured/ (byte copies)
affects: [21-05, 21-06, 21-07, 21-08]

actuals:
  tokens: 30600
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "Evidence blocks appended only by cap/shcap with the exact command and a script-taken UTC stamp; check21 gates Decisions"
    - "shcap runs commands with the bind dir as cwd and exported env vars so committed command lines carry no local paths or tokens"

key-files:
  created:
    - .planning/phases/21-deploy-render/21-LIVE-CAPTURES.md
    - crates/ignition-core/tests/fixtures/deploy/captured/ignition/tag-provider/CAP21/
    - crates/ignition-core/tests/fixtures/deploy/captured/ignition/tag-group/CAP21/
    - crates/ignition-core/tests/fixtures/deploy/captured/ignition/tag-type-definition/CAP21/
    - crates/ignition-core/tests/fixtures/deploy/captured/ignition/tag-definition/CAP21/
  modified:
    - crates/ignition-core/tests/fixtures/live-rig-deploy/capture.sh

key-decisions:
  - "CD1: version-8 sha2 uuid 4516ad9c-d6b0-8d86-89b9-802c96e33a9e loads on 8.3.6; D-03 can be frozen"
  - "CD2: omitted lastModification/signature loads; a stale signature loads, is recomputed in the API view, and nothing is rewritten on disk"
  - "CD3: every new named type (database-translator, oauth2-client, alarm-notification-profile, roster-config, holiday, service-connector) is scope A with a v4 uuid, attributes order uuid then enabled; a gateway-written store-and-forward-engine also has scope A and a uuid, contradicting the corpus"
  - "CD4: tag files are Gson pretty (2-space, no trailing newline, raw UTF-8, HTML-safe \\u00xx escapes, 1.0E10 float lexeme, exact big ints); all twelve are identity candidates"
  - "CD5: tags/<P>/<type>/<rest> maps to ignition/<type>/<P>/<rest> for all four tag types (A5 confirmed); tag-group are named resources, tag-type-definition and tag-definition are unary-resource.json"
  - "CD6: colon is written as %3a; ? trailing-dot comma * | quote < > are refused by the gateway as tag names; case is a collision; NTFS deferred to Phase 22"
  - "CD7: Referenced secret is written data{providerName, secretName} then type, regardless of request order"

patterns-established:
  - "check21 is the gate between captures and Decisions; re-runs append a new C21 section instead of editing blocks"

requirements-completed: []  # DRND-05 and DRND-06 are NOT marked complete: this plan delivers captures and fixtures only; implementation lands in 21-05/21-07

coverage:
  - id: D1
    description: "Seven capture ids (UUID8, C3b, C4, OQ8, OQ1, OQ4, C5) recorded verbatim against the loopback-only 8.3.6 rig and accepted by check21"
    requirement: "DRND-06"
    verification:
      - kind: other
        ref: "CAPTURE_OUT=.planning/phases/21-deploy-render/21-LIVE-CAPTURES.md bash crates/ignition-core/tests/fixtures/live-rig-deploy/capture.sh check21 --decisions"
        status: pass
  - id: D2
    description: "Gateway-written tag fixtures committed under fixtures/deploy/captured/ and passing the 21-01 guard"
    requirement: "DRND-05"
    verification:
      - kind: unit
        ref: "cargo test -p ignition-core --test deploy_fixtures"
        status: pass

duration: 25min
completed: 2026-10-13
status: complete
---

# Phase 21 Plan 02: Live Captures on the 8.3.6 Rig Summary

**Wire and disk truth for the render design from a loopback-only 8.3.6 gateway: a version-8 sha2 uuid loads, a stale signature is tolerated, every new named type is scope A with a uuid, tag files are Gson-written, and the gateway encodes `:` and refuses the other hostile tag names. Decisions CD1-CD7 are recorded behind a passing `check21`.**

## Performance

- **Duration:** about 25 min (rig boot and production mode about 3 min of that)
- **Started:** 2026-10-13T11:45Z (approximate; the rig's `Started (UTC)` is 11:50:17Z)
- **Completed:** 2026-10-13T12:10Z
- **Tasks:** 2
- **Files:** `capture.sh`, `21-LIVE-CAPTURES.md`, 12 fixture files

## Accomplishments

- Extended `capture.sh` (kept `assert_local` first) with `shcap`, `check21`, `v8uuid`, bind-write and scan helpers, and the Phase 21 subcommands. The new driver was exercised end to end on the rig.
- Task 1: rig up with the bind variant, tokens minted, production mode confirmed, header table written by the script; C21-UUID8, C21-C3b, C21-C4, C21-OQ8 captured.
- Task 2: C21-OQ1, C21-OQ4, C21-C5 captured; `check21` (all seven ids) passed; Decisions CD1-CD7 written; `check21 --decisions` passed; rig torn down with its volume.
- Twelve gateway-written tag files copied byte-for-byte into `fixtures/deploy/captured/` with collection-relative paths; `deploy_fixtures` still passes (9 of 9).

## Decisions (one line each; full text in 21-LIVE-CAPTURES.md `## Decisions`)

- **CD1** v8 uuid for `ign-deploy-rig` / `ignition` / `database-translator` / `CAP21_V8` is `4516ad9c-d6b0-8d86-89b9-802c96e33a9e`; 8.3.6 loaded it (find 200, names lists it, file untouched, no log line). D-03 is not blocked.
- **CD2** Omitted `lastModification` and signature load unchanged; a stale all-zero signature loads, the API view shows a recomputed `lastModification` (actor `external`) and signature, and neither file is rewritten across two scans 60 s apart.
- **CD3** `oauth2-client`, `alarm-notification-profile`, `roster-config`, `holiday`, `service-connector` are all named, scope `A`, v4 uuid, `attributes` = `uuid`, `enabled`; `oauth2-client` refuses `clientSecret: null` (422). Store-and-forward written as a side effect is scope `A` with uuid and `lastModification`, contradicting the corpus census.
- **CD4** Gateway tag files are Gson pretty-printed with no trailing newline, raw UTF-8, HTML-safe `\u00xx` escapes, `1.0E10` for a 10000000000.0 input, exact integers above 2^53, nulls kept, non-alphabetical key and tag order; all twelve are identity candidates for 21-05. Files appear 5-15 s after the import answers.
- **CD5** `tags/<P>/<type>/<rest>` maps to `ignition/<type>/<P>/<rest>` for tag-provider, tag-group, tag-type-definition and tag-definition (RESEARCH A5 confirmed); folders nest as directories holding `tags.json`/`udts.json`/`unary-resource.json`.
- **CD6** `a:b` becomes `a%3ab`; `has space` and `Case` are written raw; `case` collides with `Case`; `q?`, `trail.`, `com,ma` (and `* | " < >`, literal `%`) are refused as tag names; NTFS behaviour deferred to Phase 22 (D-04).
- **CD7** Referenced secrets are written `data` then `type`, `providerName` then `secretName`, identically for opposite request orders.

## check21 output

Before `## Decisions` was written (all seven ids; exit 0):

```
PASS header started-utc
PASS UUID8 sections=1 blocks=13
PASS C3b sections=1 blocks=14
PASS C4 sections=2 blocks=16
PASS OQ8 sections=1 blocks=8
PASS OQ1 sections=1 blocks=21
PASS OQ4 sections=1 blocks=3
PASS C5 sections=3 blocks=20
```

After Decisions (`--decisions`; exit 0; the C4 section count is 3 because the `c4c` side-effect capture was appended in between):

```
PASS header started-utc
PASS UUID8 sections=1 blocks=13
PASS C3b sections=1 blocks=14
PASS C4 sections=3 blocks=19
PASS OQ8 sections=1 blocks=8
PASS OQ1 sections=1 blocks=21
PASS OQ4 sections=1 blocks=3
PASS C5 sections=3 blocks=20
PASS CD1 cites-passing-block
PASS CD2 cites-passing-block
PASS CD3 cites-passing-block
PASS CD4 cites-passing-block
PASS CD5 cites-passing-block
PASS CD6 cites-passing-block
PASS CD7 cites-passing-block
PASS Decisions CD1-CD7-exactly-once
```

The `c4c` capture was taken after the first gate run and before Decisions were written; the gate was re-run on the final file (both runs exit 0).

## Task Commits

1. **Task 1: rig up in production mode and resource-side captures** - `5e247fd` (test)
2. **Task 2: tag captures, Decisions CD1-CD7, teardown** - `2c9b226` (test)

**Plan metadata:** committed separately with this SUMMARY.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] File secret provider instead of an internal secret (OQ8)**
- **Found during:** Task 1, step 6
- **Issue:** the plan asks for an internal secret provider holding `cap21-secret` with a run-time value "never written to any file". An internal secret needs JWE ciphertext and `/secret-providers/{name}/secrets` is GET-only in the live OpenAPI, so a throwaway value cannot be created through the REST API.
- **Fix:** created an internal provider with `secrets: {}` (accepted) and a file provider `CAP21_SP_FILE` pointing at a file inside the disposable container; the value was piped from `openssl rand` straight into the container and never printed, stored on the host or committed. The Referenced reference to it was created and its gateway-written bytes captured (CD7 states the difference).
- **Files modified:** capture.sh (oq8), 21-LIVE-CAPTURES.md
- **Commit:** 5e247fd

**2. [Rule 3 - Blocking] WebDev `tags` route needed a second deploy (Task 2)**
- **Found during:** Task 2 setup
- **Issue:** after `ign webdev deploy --yes` the `tags` route stayed unregistered (`ign webdev status`: absent, 405/404) so `ign tags import` kept answering `routes_not_deployed`.
- **Fix:** a second `ign webdev deploy --yes` registered all four routes; recorded as a header-table row.
- **Commit:** 2c9b226

**3. [Rule 2 - Missing critical] Extra captures beyond the plan**
- `c4b` (oauth2-client with a Referenced secret, because the null form was refused), `c4c` (store-and-forward side-effect resource, which surfaced the corpus contradiction in CD3), `c5log` (the gateway log and a second listing, because the import answered success while the gateway rejected names) and `c5x` (further Windows-illegal characters and a literal `a%3ab`). Each was appended through `cap`/`shcap` and passes `check21`.

**4. Verification-command false positive (documented, not fixed)**
- The plan's grep `cap-[ab]:[A-Za-z0-9_-]{16,}` over `.planning` and `crates` matches the dummy literal `cap-a:tokengeneratedlive` in the Phase 20 plan and `deploy_client_contract.rs`. Those are fake test credentials from earlier phases, not minted keys. The real check was a fixed-string search of every minted key value from `tokens.env` across `.planning`, `crates` and `.claude`: no match.

**Total deviations:** 2 blocking fixes, 1 scope addition, 1 verification note. No Blocker; the rig started, production mode was confirmed and CD1 records the v8 uuid as loaded.

## Issues Encountered

- A first insertion of the Task 2 functions landed inside `sect()` (the first `case "${1:-}"` in the file); caught on the first run (`oq1: command not found`) and moved before the main dispatch, before any capture was taken.
- The gateway flushes tag files 5-15 s after an import answers success; the capture waits 15-20 s.

## Authentication Gates

None. Tokens were minted by `mint-token.sh` into the 0600 state file outside the repo.

## Known Stubs

None.

## Threat Flags

None. The driver reaches only `localhost`/`127.0.0.1` (`BASE=http://example.com:18290 capture.sh uuid8` exits 2 before any request); no minted key is in the repository; the rig and its volume are gone; the bind directory (outside the repo) was removed; stale `tokens.env` was deleted.

## Verification

- `BASE=http://example.com:18290 bash capture.sh uuid8` exits 2.
- `check21` non-vacuity: removing the ` @ <UTC>` suffix from one block makes `UUID8` FAIL (exit 1); a `## C21-UUID8` heading with no fence also exits 1.
- `shcap` token probe exits 2 and leaves the scratch file unchanged.
- `check21 --decisions` exits 0 on the final file; `grep -c '^### CD[1-7]'` prints 7; `grep -c '^## C21-'` prints 11.
- `docker ps -a --filter label=com.docker.compose.project=ign-deploy-rig -q` and `docker volume ls -q | grep ign-deploy-rig` print nothing.
- `find fixtures/deploy/captured -type f | wc -l` is 12; no path component holds `< > : " | ? *`; no CR byte; `grep -rl Temp` finds `tag-definition/CAP21/FolderA/tags.json`.
- `cargo test -p ignition-core --test deploy_fixtures`: 9 passed.

## Next Phase Readiness

- 21-07 can pin the CD1 uuid literal and build TYPE_RULES rows from CD3 (including the store-and-forward and themes caveat).
- 21-05 gets twelve identity-candidate fixtures and the CD4 format facts; 21-06 gets the CD5 mapping with its not-exercised list; 21-08 gets CD6 and CD7.
- No Blocker. DRND-05 and DRND-06 stay open: they complete when 21-05 and 21-07 implement them.

## Self-Check: PASSED

- FOUND: `.planning/phases/21-deploy-render/21-LIVE-CAPTURES.md`
- FOUND: `crates/ignition-core/tests/fixtures/live-rig-deploy/capture.sh`
- FOUND: `crates/ignition-core/tests/fixtures/deploy/captured/ignition/tag-definition/CAP21/FolderA/tags.json` (and 11 sibling files)
- FOUND commits: `5e247fd`, `2c9b226`
