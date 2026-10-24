---
phase: 25-guarded-project-deploy
plan: 19
subsystem: live-gates
tags: [deploy, projects, live-rig, installed-binary, phase-gate, dprj-01, dprj-02, dprj-03, dprj-04, dprj-05]

requires:
  - phase: 25-guarded-project-deploy
    provides: "25-04..25-18: the `deploy projects` engine, CLI verb, refusals, children report, records, runbook and CI workflow; the binding contract decisions in 25-LIVE-CAPTURES.md (`## Contract decisions (25-03)`)"
provides:
  - "crates/ignition-core/tests/live_deploy_projects.rs: five ignored live gates (`live_projects_create_then_import`, `live_projects_skip_and_record`, `live_projects_refusals`, `live_projects_children`, `live_projects_check_get_only`) with a loopback URL guard, a gateway-name guard, a method tap and a CAP25 sweep on entry and exit"
  - "crates/ignition-core/tests/fixtures/live-rig-deploy/compose.projects.yml and capture25.sh `up25`/`down25`: the rig override binding `data/projects` and `data/config/resources` onto one host data root outside the repo"
  - "25-LIVE-CAPTURES.md `## Live gates (25-19)`: one block per live test, `### Installed binary (25-19)`, `### Capture-host retry (25-19)`, `### Open points carried to the end of the phase (25-19)`, `### Rig teardown (25-19)`, `### Phase gate (25-19)`"
  - "DPRJ-01..05 marked complete, each resting on the live evidence listed under Requirement status"
affects: []

actuals:
  tokens: 17069   # chars/4 over the added lines of `git diff 252dbf8..HEAD` at the last task commit (68275 chars); excludes this SUMMARY, STATE.md and ROADMAP.md
  tasks: 3
  commits: 4      # 2c6f6cd (tracer, previous session), c982688, d355fed, 7ea3c91; plus the final docs commit

tech-stack:
  added: []
  patterns:
    - "live gates share one gateway through a process-wide lock and clean themselves: every test sweeps every CAP25 project, record and test directory on entry and again on exit (also from `Drop`, so a failed assertion still cleans), and ends with `assert_clean`"
    - "refusal and check claims are observed, not inferred: a local HTTP tap in front of the rig records the method of every request, and a refusal test asserts it saw no non-GET"

key-files:
  created:
    - crates/ignition-core/tests/live_deploy_projects.rs
    - crates/ignition-core/tests/fixtures/live-rig-deploy/compose.projects.yml
  modified:
    - crates/ignition-core/tests/fixtures/live-rig-deploy/capture25.sh
    - .planning/phases/25-guarded-project-deploy/25-LIVE-CAPTURES.md

key-decisions:
  - "No assertion was weakened or loosened: the four Task 2 tests passed on their first run on the rig, so no rig surprise needed a fix."
  - "DPRJ-01..05 are marked complete (see Requirement status); the GitHub-side behaviour of 25-18 is not what any of them rests on and stays unproven."

requirements-completed: [DPRJ-01, DPRJ-02, DPRJ-03, DPRJ-04, DPRJ-05]

duration: "tracer 2c6f6cd in the previous session; this continuation from about 03:35Z to 04:10Z on 2026-10-24 (the phase gate itself took 16 minutes)"
completed: 2026-10-24
status: complete
---

# Phase 25 Plan 19: Live gates on the 8.3.6 rig, installed-binary smoke and the phase gate Summary

**`ign deploy projects` is proven on a real Ignition 8.3.6 gateway and through the installed `ign` binary: the checkout-built artifact is created, then imported, and the gateway's own re-export has the same canonical digest in both runs; skip, record-only adoption, every refusal (zero writes), the children report and a GET-only check are each observed; the workspace gate is green.**

## Accomplishments

- **Task 1, tracer (`2c6f6cd`, previous session, accepted at the human-verify gate: "Accept, continue (Recommended)").** `compose.projects.yml`, `capture25.sh up25`/`down25`, the test scaffold and `live_projects_create_then_import`: `created` then `imported`, artifact digest equal to the gateway re-export digest in both runs (`6d0dc2d6...f422bfe2`, then `78fb96d9...4c730a`).
- **Task 2 (`c982688`, wording fix `d355fed`).** `MethodTap`, `Mode`, `raw_import`, `Repo::ign_dir_exists`, `Repo::run_count` and four tests, run on the rig at 2026-10-24T03:38:39Z to 03:39:15Z (`5 passed; 0 failed`, 13.97 s). All four passed on the first run.
  - `live_projects_skip_and_record`: an unchanged re-run is `skipped` with no `.ign/` directory, no new run directory and reads only; a project the rig already held, deployed for the first time with the same content, is `recorded` (the live digest equals the artifact digest before and after, the record holds `v1.0.0`); the same release again is `skipped`.
  - `live_projects_refusals`: `v1.0.0` over a recorded `v1.1.0` is `destructive_change_refused` gate `downgrade`; a first deploy that would change a project the tool never deployed is the same code with gate `first_deploy`; `parent: CAP25-NOPE` is `project_parent_invalid` reason `missing`; a `.git` directory made under `data/projects/<name>` (after the test proved the gateway writes the project there) is `project_git_managed` kind `directory`, under apply and under check. Zero non-GET requests in every case; the gateway copy kept its digest.
  - `live_projects_children`: a parent deploy reports its child `ok` (recorded digest equals live digest); after the child's record is edited the next parent deploy is `imported`, `ok` true, with the child `mismatch` and a warning; the child's live digest never changed.
  - `live_projects_check_get_only`: `--check` for create, import, record and skip plans sent only GET requests (2, 3, 3, 3), wrote no run directory, record or `.ign/`, and left the gateway's project list unchanged.
- **Task 3 (`7ea3c91`).**
  - `cargo install --path crates/ignition-cli --force` replaced `ignition-cli v1.5.0` with `v1.6.0-rc.1`; `command -v ign` printed `/Users/pmannion/.cargo/bin/ign`.
  - The installed binary against the rig (hand-made repo, targets root and a throwaway `IGNITION_CLI_CONFIG`, the real config untouched): `--check --json` reported `create` with the artifact digest; the confirmed `--artifact-sha256 <hash> --yes --json` apply gave `created`, a re-check gave `skip`; a changed library gave check `import` then apply `imported`, then `skip` (the installed binary's own verify and a fresh check agree on the digest); a downgrade attempt exited 6. The project and its record were removed afterwards.
  - Capture-host retry: no `pending` row exists in `## C-H after grant (25-08)` or `### Record write (25-13)`; recorded "no pending capture-host rows; both hosts passed in 25-08 and 25-13". The Azure VMs were not started.
  - Phase gate, on the final tree: `cargo test --workspace` exit 0 (2887 passed, 0 failed, 82 ignored live tests), `cargo clippy --workspace --all-targets -- -D warnings` exit 0, `cargo build --workspace --no-default-features` exit 0, and `cargo fmt --all -- --check` exit 0.
- **Teardown.** `capture25.sh down25` and the commands it printed (`docker compose -p ign-deploy-rig -f compose.yml -f compose.projects.yml down -v`, then `/bin/rm -rf` of the data root) ran at 2026-10-24T03:54:03Z. `docker ps -a` shows no `ign-deploy-rig` container, `docker volume ls` no `ign-deploy` volume, `docker network ls` no `deploy-rig` network. Before it, the rig's project list was `[]` and no `*cap25*` entry existed under the data root. Nothing is running in the background.

## Verification

- `cargo test -p ignition-core --test live_deploy_projects --no-run` exit 0; `grep -c '^### live_projects_' 25-LIVE-CAPTURES.md` is 5; `grep -c '^### Installed binary (25-19)'` is 1.
- The live commands and UTC times are recorded in the captures file; the rig was loopback only (`http://localhost:18290`), and no production host was touched.

## Deviations from Plan

None in the shipped code or the tests. Three notes on method, none of them a rule deviation:

1. **Task 3 step 2** used the plan's allowed alternative (the repo, targets and a throwaway config recreated by hand in a scratch directory) rather than a keep flag on the test helper. The scratch directory was removed after the run.
2. **The downgrade refusal through the installed binary was recorded by exit code (6) only.** The scratch directory, which held its stderr envelope, was removed in the same command before I read it; I did not re-run it. The gate `downgrade` itself is observed in `live_projects_refusals` (an error code plus the gate, asserted). The block says the stderr envelope was not kept.
3. **Extra command in the gate:** `cargo fmt --all -- --check` was run beside the three the plan names. It passed.

The tracer session (previous executor) found that a rig stopped with exit 137 holds a commissioned volume that faults with `Commissioning Failure` under an empty `config/resources` bind; `up25` now refuses while that volume exists. That is recorded under `## Live gates (25-19)`.

## Requirement status

`requirements-completed: [DPRJ-01, DPRJ-02, DPRJ-03, DPRJ-04, DPRJ-05]`. Every earlier Phase 25 plan left these open on purpose and named this plan as the one that proves them on a real gateway. Each now rests on the verb's own behaviour (engine and CLI suites against capture-faithful fakes, plus the live gates here). Nothing below rests on a GitHub run.

| Requirement | Rests on |
| --- | --- |
| DPRJ-01 (artifact verified against its pin before any upload; per D-01 the pin is `--artifact-sha256`, the artifact's canonical hash) | Live: the installed binary's `--check` printed `artifact_sha256`, and the confirmed apply with that hash created and imported the project; the artifact the tests build and the one the installed binary builds are byte-identical (same digests). The wrong-hash refusal `project_hash_mismatch` (exit 6) and "nothing uploaded" are proven by the engine and CLI suites (25-11) and the 25-18 harness against a mock gateway, not live. |
| DPRJ-02 (parent must exist, cycles and unknown parents refuse, unchanged projects skipped) | Live: `parent: CAP25-NOPE` refuses `project_parent_invalid` / `missing` with no write; an unchanged project is `skipped` with no `.ign/`, no run directory and no POST; a parent deploy reports its child `ok`, and `mismatch` with a warning (outcome still `imported`) after the child's record was edited. The `cycle` and `not_inheritable` reasons are proven by the 25-10 engine suite only, not live. |
| DPRJ-03 (export before overwrite, verify after import by canonical hash) | Live: both runs of `live_projects_create_then_import` show the gateway re-export's canonical digest equal to the artifact's; the pre-image `export/CAP25-LIVE.zip` equals the previously deployed digest; the installed binary's re-check agrees. The `imported_unverified` verdict is not induced live (the plan says so) and stays proven by the suites. |
| DPRJ-04 (projects not named are reported and never deleted; a `.git` project is refused) | Live: `project_git_managed` for a `.git` directory under the rig's bind-mounted project directory, apply and check, zero requests; the gateway's project list is unchanged by a check; the `.git` probe itself passed on both capture hosts in 25-08. The delete half is a negative claim: no code path in the verb deletes (25-11 confinement scans). |
| DPRJ-05 (`--check` reports with no writes) | Live: the tap saw only GET for create, import, record and skip plans, and for the refused `.git` and parent cases; no `.ign/`, run directory or record was written. |

**Open and not claimed by any requirement:** the GitHub-side behaviour of `deploy-projects.yml` (a real tag run, `dev`/`prod` approvals, the secret split reaching the `apply` job, the concurrency queue) is unproven from this machine. DPRJ-01 and DPRJ-05 are also listed by 25-18, but their text is about the verb, which the live gates prove; the CI wrapper is a consumer, not part of what the requirements say.

## Open points

- **Windows `Move-Item -Force` atomicity: still open (25-13).** The rig is Linux; no concurrent reader was provoked on the Windows capture host. Recorded in the captures file.
- **Size cap and memory (25-06):** the cap applies to the built zip, so a large checkout is built and held in memory before the cap is checked. The rig's projects were a few KB and say nothing about large ones.
- **Runbook steps 4 and 5 (25-17):** covered live by `CAP25-LCKR` (check says `record`), `CAP25-LFIRST` (different content without a record refuses `first_deploy`) and `CAP25-LREC` (apply gives `recorded`, project unchanged, record written). Not shown live: that the record-only apply sends no import request (that apply ran without a tap) and the `would_change` field of the record check.
- **25-18, GitHub only, owner follow-ups:** a first real run of `deploy-projects.yml` with a `v0.0.1-test` tag and a `dev` environment (the plan's `human-check`), the environment-secret premise, the concurrency queue, a self-hosted Windows runner, and `deploy-plan.yml`'s ungated plan job holding `IGN_SSH_KEY`.
- **Still OPEN (owner) from 25-17:** the long-term branch lifecycle of module-managed projects, and images and themes after WHK-Global's cutover.
- **Still unexercised by design:** `imported_unverified` live, the S5 residual (child session impact after a parent overwrite), the git module's Designer-side pull/save/hotfix paths and `tags_importOnStartup` (C10).
- `~/.cache/ign-deploy-rig/tokens.env` (0600) was left in place; its tokens belonged to the removed rig volume.

## Known Stubs

None. The changed files hold no placeholder, `todo!`, ignored-without-reason test or empty data path; the `#[ignore]` attributes on the live tests carry reasons and are the plan's design.

## Threat Flags

None. The only new surface is the test code's loopback HTTP tap (`127.0.0.1:0`, records method and path only, forwards to the loopback rig), covered by T-25-60 (URL guard, opt-in, CAP25 names, cleanup in every test). T-25-61: the captures hold outcomes, digests and counts only; the token was read from a 0600 env file and never printed.

## Self-Check: PASSED

- Files exist: `crates/ignition-core/tests/live_deploy_projects.rs`, `crates/ignition-core/tests/fixtures/live-rig-deploy/compose.projects.yml`, `crates/ignition-core/tests/fixtures/live-rig-deploy/capture25.sh`, `.planning/phases/25-guarded-project-deploy/25-LIVE-CAPTURES.md`: found.
- Commits exist: `2c6f6cd`, `c982688`, `d355fed`, `7ea3c91`: found in `git log`.
- `grep -c '^### live_projects_' 25-LIVE-CAPTURES.md` is 5 (one per test); `## Live gates (25-19)`, `### Installed binary (25-19)`, `### Capture-host retry (25-19)`, `### Rig teardown (25-19)` and `### Phase gate (25-19)` each occur once.
- No file was deleted by any of the plan's commits. `.planning/config.json` and `.planning/research/.cache/` were not staged. Nothing was pushed.
- The rig is gone (no container, volume, network or data root) and no CAP25 project or file remains.
