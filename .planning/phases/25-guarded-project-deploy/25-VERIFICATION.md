---
phase: 25-guarded-project-deploy
verified: 2026-10-29T12:00:00Z
status: passed
score: 5/5 roadmap success criteria verified; 27/27 plans' artifacts present and wired; 5/5 requirement IDs satisfied; gaps G-25-4 to G-25-13 all closed
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: human_needed
  previous_score: "5/5 roadmap success criteria verified; 20/20 plans (plus UAT gaps G-25-7..G-25-13 open in 25-UAT.md)"
  gaps_closed:
    - "G-25-7: Windows release ign.exe needs the VC++ runtime; closed by 25-22 (static CRT, dumpbin import check), live-proven by 25-27"
    - "G-25-8: README caller examples used a reserved ssh-key-env name; closed by 25-25/25-26 (DEPLOY_SSH_KEY, reserved-name rule stated)"
    - "G-25-9: core.autocrlf=true on a Windows runner corrupted the targets checkout; closed by 25-25 (GIT_CONFIG_* on every checkout and git/ign step), live-proven by 25-27"
    - "G-25-10: icacls grant by %USERNAME% failed under NETWORK SERVICE; closed by 25-23 (grant by process SID), live-proven by 25-27"
    - "G-25-11: a read-only check token could write (bare Authenticated in writePermissions); closed by 25-24 (exact write levels, --read-only, --restrict-write), live end to end on 8.3.6 and 8.3.3"
    - "G-25-12: README did not tell callers how to keep the job on the intended runner; closed by 25-26 (unique label, Git\\bin on service PATH, real service account)"
    - "G-25-13: write secrets could not reach an approved apply job from a caller in another organization; closed by 25-25 (composite actions) and live-proven by 25-27 (cross-org WhiskeyHouse caller, TheThoughtagen actions)"
    - "G-25-4, G-25-5, G-25-6 (Windows record replace): already closed before this pass; unchanged, not regressed"
    - "UAT tests 1, 2 and 3 (first real GitHub run, concurrency queue, self-hosted Windows runner): reached and passed in the 25-27 live UAT"
  gaps_remaining: []
  regressions: []
gaps: []
deferred: []
behavior_unverified_items: []
human_verification: []
---

# Phase 25: Guarded Project Deploy Verification Report

**Phase Goal:** A user (or a project repo's CI) can deploy one Ignition project from its own repo to a gateway with `ign deploy projects`: the artifact built from the checkout is verified against its reviewed hash before any upload, the project's parent must already be on the gateway and is never imported or modified by the child's deploy, the live project is exported before it is overwritten and hash-verified after import, a project absent from the gateway is created, and no other gateway project is ever imported, modified or deleted.
**Verified:** 2026-10-29 (HEAD `a04a613`; initial verification 2026-10-24, first re-verification 2026-10-27)
**Status:** passed (no gaps; five advisory warnings, none contradicts a success criterion)
**Re-verification:** Yes, after gap closure plans 25-22 to 25-27 (G-25-7 to G-25-13)

## What changed since the last verification

The 2026-10-27 report was `human_needed` because four UAT items were blocked on infrastructure (a first real GitHub run, the concurrency queue, a self-hosted Windows runner, a Windows build of the portable `local_fs.rs`). The live UAT of 2026-10-28 then found seven new gaps (G-25-7 to G-25-13), and plans 25-22 to 25-27 closed them. Plan 25-27 ran the UAT again from `WhiskeyHouse/whk-ign-deploy-uat` on `ign-win-836` with the VM workarounds reverted (NETWORK SERVICE, `core.autocrlf=true`, no VC++ runtime).

I did not rely on the SUMMARY or the capture text for the live claims. I queried the GitHub runs directly (`gh run view`):

- run 40771157285 (tag v0.0.8-test): `check-dev` and `apply-dev` success; the log shows SID `S-1-5-20`, `system core.autocrlf: true`, `vcruntime140.dll present: False`, and the check job running `TheThoughtagen/ignition-cli/.github/actions/deploy-projects-check`.
- run 40775414088 (v0.0.9-test): both jobs success; the apply job's handoff step shows `IGN_ARTIFACT_SHA256: efdbd2e0...b944`, the same hash `25-LIVE-CAPTURES.md` records for the check.
- run 40794529806 (cross-org reusable workflow): `dev / check` and `dev / apply` success.

## Gap-by-gap check

| Gap | Closing plan | Verdict | Evidence checked |
|-----|--------------|---------|------------------|
| G-25-7 released `ign.exe` needs the VC++ runtime | 25-22 | CLOSED | `release.yml` diff: Windows matrix entry has `rustflags: '-D warnings -C target-feature=+crt-static'` passed through the `setup-rust-toolchain` `rustflags` input (a config-file flag would be overridden by the exported `RUSTFLAGS`, the file explains why); a `dumpbin /dependents` step runs after the build and before packaging, requires `KERNEL32.dll` in the parsed list (no vacuous pass) and throws on any `vcruntime`/`msvcp` import. `release-test.sh` (run by me): exit 0, 5 PASS lines. Live: release run 40730588305 import list has no vcruntime/msvcp; the static `ign.exe` ran on the VM with `vcruntime140.dll` absent. |
| G-25-10 `icacls` by user name fails under a service account | 25-23 | CLOSED | `deploy/owner_acl.rs`: SID from `whoami /user /fo csv /nh`, `is_sid` allows only `S-1-` plus 2 to 16 digit groups, grant is `*<SID>:<rights>`, every failure is an `Err` with no fallback to a name. `remote.rs::restrict_to_owner` and `run_store.rs::harden_dir` both call it; `env::var("USERNAME")` is gone from both (diff read). Unit tests (run by me, `owner_acl`): pass. Live: apply ran under `S-1-5-20` and deployed (runs A and B), so the key file and run directory were restricted by SID. |
| G-25-11 read-only token could write | 25-24 | CLOSED (see warnings WR-01, WR-02) | `actions/adopt.rs`: write levels resolved before login, a bare `Authenticated`/`Public` write level refused (`contract_adopt` tests, run by me: 4 passed); default write level `Authenticated/IgnCliWrite` chosen by a recorded live capture on 8.3.6 and 8.3.3 (`Decision: dedicated-nested`); `--read-only` pre-checks that the write list does not admit every key and proves a 403 on `scan/projects`; `--restrict-write` is opt-in. Live (25-24): the read-only key reads 200 and writes 403 on a fresh 8.3.6 rig. Live (25-27): `ign-ci-check` minted `--read-only`, `ign doctor --check-write` row `warn` "read-only token (HTTP 403 on scan/projects)". |
| G-25-9 autocrlf rewrote the checkouts | 25-25 | CLOSED | Every checkout and git/ign step in both actions carries `GIT_CONFIG_COUNT/KEY_0=core.autocrlf/VALUE_0=false` (harness case 3f, run by me via `projects-test.sh`: pass). Live: system `core.autocrlf: true` on the runner and every listed targets and project file reads `i/lf w/lf`; both runs succeeded. |
| G-25-13 write secrets in another organization | 25-25, 25-26, 25-27 | CLOSED | `.github/actions/deploy-projects-check` has no write-capable input and no `secrets.` reference; `deploy-projects-apply` requires `artifact-sha256` (64 hex), `targets-sha` (40 hex), `master-token`, and `ssh-key` when `ssh-key-env` is set, refusing before any checkout (`body:apply-handoff`), reads targets at exactly `targets-sha`, and passes `--artifact-sha256` to `ign deploy projects`. Live: a WhiskeyHouse caller with the actions from TheThoughtagen deployed through an approved dev environment (runs A and B above). See the correction note on the root cause below. |
| G-25-8 README examples rejected | 25-26 | CLOSED | `grep -c IGN_DEPLOY_KEY README.md` is 0; examples use `ssh-key-env: DEPLOY_SSH_KEY`; the reserved-name rule is in the input tables; `readme-test.sh` 6a to 6d run actionlint on the extracted template against the real `action.yml` files (run by me: exit 0). |
| G-25-12 runner guidance | 25-26 | CLOSED | README: unique `ign-deploy` label in both examples and the `runs-on` row, `C:\Program Files\Git\bin` on the service PATH, run the runner under a real account, note that NETWORK SERVICE also works because the key file is restricted by SID. Harness 6c asserts the labels. |
| G-25-4/5/6 Windows record replace | 25-20, 25-21, quick vc6 | CLOSED, unchanged | Not re-run. `winps.rs` and the sftp record path are outside the 25-22..27 diff (`git diff 7c7269e^..HEAD --stat` lists no `winps.rs`/`sftp` file). |

### Correction to the G-25-13 root cause (plan 25-27)

The UAT gap said an explicit secrets mapping delivers nothing to a reusable workflow's environment job. Plan 25-27 re-probed (uat-probe run 40803747460: no mapping and a literal mapping read empty; an explicit by-name mapping and `secrets: inherit` read present) and then ran the cross-org negative caller, which did not fail. I confirmed run 40794529806 succeeded in both jobs. The consequence is that the reusable workflow is usable cross-organization with the five secrets mapped by name; `secrets: inherit` is the form limited to one organization. The README Secrets section, the `deploy-projects.yml` header and its apply-secrets cause text now say this (commit d74a673). The gap stays closed, because the composite-action route is the one the live runs prove and the route works from any organization. One leftover is listed under Warnings (stale secret descriptions in `deploy-projects.yml`).

## The two unplanned 25-27 changes

**Commit 0ab4093, canonical digest defaults: VERIFIED.** `project_artifact.rs` adds `fill_defaults` and uses it for `project.json` (`description` "", `parent` "", `inheritable` false) and `resource.json` (`version` 1, `restricted` false, `overridable` true). A key that is absent or null reads as its default; any other value still changes the digest. Checks:

- The two new unit tests are in the 68 `project_artifact`/`owner_acl`/`adopt` lib tests I ran (all pass): `defaults_the_gateway_adds_on_create_do_not_make_a_project_changed` uses the exact bytes from the failed UAT run 40755290379, and `a_default_written_with_another_value_still_counts` asserts `inheritable:true`, `version:2`, `restricted:true`, `overridable:false` each change the digest. The existing `digest_invariants` and `digest_sees_a_byte_change_and_a_rename` still pass.
- It does not weaken success criterion 3. The post-import check is still a canonical-hash equality between the artifact and a fresh export; the only values treated as equal are the ones the gateway itself writes into a descriptor that lacks them. A repo that already carries the keys hashes as before (the diff only inserts when absent), so earlier release records and pins for such repos remain valid; a repo that lacks them could never have verified before this fix (it ended `imported_unverified`).
- Live: run A `create` -> `created` with the read-back equal, and run B `import` -> `imported` with the pre-import `live_sha256` equal to run A's digest, on the binary built from this commit (`v1.6.0-uat.2`).

**Commit d74a673, cross-org secret-delivery docs: VERIFIED with one leftover.** README (lines 4229 to 4247, 4494 to 4570) and the workflow header and apply-secrets text agree with the probe facts. The two `secrets:` descriptions for `IGN_MASTER_TOKEN` and `IGN_SSH_KEY` at `deploy-projects.yml:110,113` were not updated and still say the secret "reaches this workflow only through `secrets inherit` from a caller in the same organization". That contradicts the header of the same file (Warning W-1).

## Goal Achievement

### Observable Truths (ROADMAP success criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `ign deploy projects` verifies the artifact's canonical hash against `--artifact-sha256` before any upload; mismatch refuses `project_hash_mismatch`, nothing uploaded | VERIFIED | Unchanged code path (hash compare in `load_project` before any host or gateway object exists). The apply composite action passes `--artifact-sha256 "$IGN_ARTIFACT_SHA256"` and refuses a non-64-hex value first; the check output feeds the apply input (live: run B apply used `efdbd2e0...b944`, equal to the check's). `readme_exit_table_agreement` and `exit_code_mapping_enumerated` re-run: ok. |
| 2 | Parent read from `project.json`, never imported, exported or modified; missing parent or a cycle refuses before any import; a hash-equal project is skipped; children are hash-checked after a parent deploys and a mismatch is a warning | VERIFIED | Unchanged since the 2026-10-27 report (no gap-closure plan touches `deploy_projects.rs`, `deploy/projects.rs` or the parent logic; `git diff 7c7269e^..HEAD` over `crates/` lists only the files in the table above). Live evidence from 25-19 stands; `cycle` and `not_inheritable` remain engine-test-proven only. |
| 3 | Each project is exported before overwrite and hash-verified after import, ignoring `lastModification*` and signatures | VERIFIED | Same code; the only change is the K4 default set (0ab4093, above). Live (25-27) on a Windows ssh node: run B `import`, pre-image `live_sha256` equal to the previous deploy's, read-back equal, record written (`previous_release v0.0.8-test`). |
| 4 | Absent project created (`created`); unnamed gateway projects reported, never imported, modified or deleted; `.git` refused; `--check` reports with zero writes | VERIFIED | Run A: `create` -> `created`, `ok: true`, `record_written: true`. No gap-closure plan adds a gateway write: the check action runs only `ign deploy projects --check` (line 387) and holds no write secret. Prior live GET-only taps stand. |
| 5 | `projects` in `GUARDED_OPS`, out of the MCP catalog, TUI row, README entry and exit rows; output and run files pass the secret canary | VERIFIED | Unchanged. Exit-table agreement tests ok. Live logs of runs A and B and the negative and concurrency runs: 0 private-key headers and 0 check-token matches (recorded; I re-confirmed the secrets are masked as `***` and the key text is absent in the run A log grep for `BEGIN OPENSSH`, which returned no line). |

**Score:** 5/5 truths verified, 0 present-but-behavior-unverified.

### Plan-level truths for 25-22 to 25-27 (spot-checked in code)

| Item | Status | Evidence |
|------|--------|----------|
| Check job holds no write-capable input | VERIFIED | `deploy-projects-check/action.yml`: inputs `check-token`, `check-ssh-key`; no `master-token`, no `secrets.` reference; `readme-test.sh` 6b asserts no write secret and no `environment:` in check jobs. |
| Apply refuses before any checkout on a malformed handoff or empty write secret | VERIFIED | `body:apply-handoff` is the first step; harness `projects-test.sh` cases 3d2, 3d3, 4e to 4h pass (run by me). |
| `deploy-projects.yml` runs both actions from `job.workflow_sha` and verifies HEAD | VERIFIED (static + live) | Lines 149 to 194 and 246 to 291; live cross-org negative run printed `ign deploy actions: TheThoughtagen/ignition-cli@0ab4093...`. actionlint knows neither `job.workflow_repository` nor `job.workflow_sha`; the harness ignores exactly those two messages and the live run proves they populate. |
| Reusable workflow concurrency group, `cancel-in-progress: false` | VERIFIED | `deploy-projects.yml:216-217`; live: v0.0.11 replaced while pending, v0.0.10 and v0.0.12 ran in order. |
| Static CRT without a `.cargo/config.toml` | VERIFIED | none present; 25-22 key decision explains why. |
| No new exit code or slug in the gap-closure plans | VERIFIED | `git diff 7c7269e^ HEAD -- crates/ignition-core/src/error.rs` is empty; exit-table tests ok. |

### Required Artifacts

| Artifact | Status | Details |
|----------|--------|---------|
| `.github/actions/deploy-projects-check/action.yml` (506 lines) and `.../deploy-projects-apply/action.yml` (661) | VERIFIED | substantive, wired from `deploy-projects.yml` and from the README template; used live. |
| `.github/workflows/release.yml` static CRT + import check | VERIFIED | live release runs 40730588305 and 40770020656 succeeded on all 5 targets. |
| `crates/ignition-core/src/deploy/owner_acl.rs` (267 lines) | VERIFIED | called from `remote.rs` and `run_store.rs`. |
| `crates/ignition-core/src/client/adopt.rs`, `actions/adopt.rs`, `cli.rs`, `main.rs` | VERIFIED | `--write-level`, `--read-only`, `--restrict-write` wired; `contract_adopt.rs` 4 passed. |
| `project_artifact.rs` default-filling | VERIFIED | above. |
| `README.md` caller template, Secrets, runner requirements, read-only check token | VERIFIED | machine-checked by `readme-test.sh`. |
| `uat/deploy-composite.yml`, `uat/deploy-reusable-crossorg.yml` | PRESENT | UAT callers; they hold no gateway URL, host or token (per the capture section). |

### Key Link Verification

| From | To | Status | Details |
|------|----|--------|---------|
| check action output `artifact_sha256`/`targets_sha` | apply action inputs | WIRED | live: run B handoff shows the checked hash and `IGN_CHECKED_TARGETS_SHA` |
| `remote.rs::restrict_to_owner` | `owner_acl::restrict_to_current_user(file, "R")` | WIRED | diff read |
| `run_store.rs::harden_dir` | `owner_acl::restrict_to_current_user(path, "(OI)(CI)F")` | WIRED | diff read |
| `adopt` CLI flags | `AdoptOptions.write_levels/read_only/restrict_write` | WIRED | `contract_adopt` tests drive the binary |
| caller apply job `environment:` | `master-token`/`ssh-key` inputs | WIRED | live: apply under dev approval deployed with the environment secrets |

### Data-Flow Trace (Level 4)

| Artifact | Data | Source | Real data | Status |
|----------|------|--------|-----------|--------|
| apply `artifact-sha256` | `needs.check.outputs.artifact_sha256` | check action `report` step from `ign --check --json` | yes (live) | FLOWING |
| canonical digest of live export | `canonical_digest(export)` | gateway re-export | yes (live equality on create and import) | FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Gap-closure unit tests | `cargo test -p ignition-core --lib -- owner_acl project_artifact adopt readme_exit_table_agreement exit_code_mapping_enumerated` (rustup cargo first on PATH) | 68 passed, 0 failed | PASS |
| Adopt CLI contract | `cargo test -p ignition-cli --test contract_adopt` | 4 passed | PASS |
| Harness | `release-test.sh`, `plan-test.sh`, `apply-test.sh`, `readme-test.sh`, `projects-test.sh` | all exit 0, every PASS line | PASS |
| Full workspace, clippy | not re-run by me | orchestrator at HEAD: 2929 passed, 0 failed; clippy `-D warnings` clean | accepted from the orchestrator |
| Live GitHub runs | `gh run view` on 40771157285, 40775414088, 40794529806 | all jobs success; diagnostics as recorded | PASS |

### Probe Execution

No `scripts/*/tests/probe-*.sh` exist and no plan declares a probe. Step 7c: not applicable.

### Requirements Coverage

Every requirement ID in the PLAN frontmatter of 25-01 to 25-27 is DPRJ-01 to DPRJ-05. REQUIREMENTS.md maps exactly those five to Phase 25, all `[x]` and "Complete". No orphaned IDs.

| Requirement | Source plans | Status | Evidence |
|-------------|--------------|--------|----------|
| DPRJ-01 artifact verified by hash before any upload | 01, 03-06, 11, 16, 18, 19, 22, 25-27 | SATISFIED | SC1; the apply action deploys exactly the checked hash |
| DPRJ-02 parent rules, skip when unchanged | 01-05, 09, 10, 15, 19, 27 | SATISFIED | SC2 |
| DPRJ-03 export before overwrite, canonical-hash verify | 01, 03, 06, 11-15, 17, 19, 20, 23, 27 | SATISFIED | SC3; live import on Windows with read-back equality |
| DPRJ-04 unnamed projects never deleted; `.git` refused | 02-05, 07, 08, 11, 17, 19, 23, 27 | SATISFIED | SC4 |
| DPRJ-05 `--check` zero writes | 03-05, 09, 11, 12, 17-19, 22-27 | SATISFIED | SC4/SC5; the check job holds only a read-only token whose write is proven 403 |

### Anti-Patterns Found

None blocking. Debt-marker scan (`TBD|FIXME|XXX|TODO|todo!|unimplemented!`) over the added lines of the gap-closure diff (code, workflows, actions and README): no hit.

### Warnings (advisory, none contradicts a success criterion)

The code review `25-REVIEW.md` found 0 critical, 5 warnings, 6 info. I read the code behind each warning and agree with all five; none is fixed.

- **W-1 (new, from this pass): stale secret descriptions.** `deploy-projects.yml:110,113` still say the write secrets reach the workflow "only through `secrets inherit` from a caller in the same organization". After d74a673 the header, apply-secrets text and README say a by-name mapping works from any organization. Fix: reword the two descriptions. Documentation only.
- **WR-01 (review): adopt can orphan a key.** In `actions/adopt.rs` the write probe runs right after the permissions PUT, before the token is persisted; a stale 403 returns `Err` and the plaintext of the just-minted key is lost, so the key must be deleted by hand. The live test sleeps 3 s after every permissions PUT, which suggests the race is real; 25-24's live end to end passed without it. Fix: retry the probe on 403 and persist before returning the error.
- **WR-02 (review, confirmed in code): README remediation order is wrong for an old gateway.** The "A read-only check token" block tells an operator of a pre-release gateway to run `ign adopt --profile dev --restrict-write` first. On such a gateway the existing `ign-cli` key lacks `Authenticated/IgnCliWrite`, so `actions/adopt.rs` (found-key branch, the `requested`/`level_names` comparison) refuses with "exists at level ... was requested" before the permissions step. The next README subsection says to delete the key and re-run, so the two sections contradict each other. The mechanism is proven on fresh rigs; the old-key migration path was not exercised live. Fix: put "delete the old key" before the `--restrict-write` command, or make the level mismatch a warning when `--restrict-write` is set.
- **WR-03 (review): upload-artifact name collision** when one workflow run applies twice to the same environment. Fails after the deploy, so a successful deploy shows a red job.
- **WR-04 (review): installed `ign` is checked only against a `.sha256` fetched from the same release.** That is an integrity check, not authenticity, and the binary then holds the production write token and deploy key. Worth an owner decision (attestation or a caller-pinned digest) before unattended production use.
- **WR-05 (review): `--read-only` defaults `--key-name` to `ign-cli`**, so its failure text can tell the operator to delete the live write key.

Carried over from earlier reports, unchanged: README Commands row for `deploy projects` and REQUIREMENTS.md wording for DPRJ-01/02/04 lag the D-04/D-05/D-16 rewording; `imported_unverified` was induced live only by accident (run 40755290379, before the fix) and `cycle`/`not_inheritable` are engine-test-proven; the 0600 pre-image stays in a self-hosted runner workspace (IN-08 of the earlier review); S5 session residual has no criterion.

### Bookkeeping for the orchestrator (not code gaps)

- `25-UAT.md` still shows tests 1 and 3 as `issue`, test 2 as `blocked`, `status: diagnosed`, and G-25-7 to G-25-13 as `failed`. All are closed by the evidence above and should be marked resolved with 25-22 to 25-27 as the closer (test 1: runs A/B; test 2: runs 40804784192, 40804893236, 40805004997; test 3: the reverted-VM runs).
- `ROADMAP.md:91` still shows Phase 25 as `[ ]`, and its Plans line says "21 executed; gap closure ... planned". `STATE.md` still reads "Phase 25 execution started".
- Environment notes: `/opt/homebrew/bin/cargo` shadows rustup's; put `~/.cargo/bin` first on PATH. The host `pwsh` crash noted on 2026-10-27 does not affect this pass (not re-run).

### Human Verification Required

None. The four items the previous report listed as human-only (GitHub first run and secret masking, concurrency queue, self-hosted Windows runner, portable `local_fs.rs` on Windows) were each exercised: the first three by the 25-27 live UAT, which I corroborated with `gh run view`, and the fourth by PR #27 CI on windows-latest (recorded in 25-UAT.md test 6, passed). The remaining live-only gaps are the cutover prerequisite (spec step 8, human work, outside this phase) and the warnings above.

### Gaps Summary

No gaps. Success criteria 1 to 5 hold on code that was read and on live evidence; the seven UAT gaps found on 2026-10-28 are closed, each with a code-level check and, for the Windows, GitHub and cross-organization behaviour, a live run that I independently confirmed exists and succeeded. The five review warnings and the one stale-text finding are quality follow-ups that the owner can schedule.

---

_Verified: 2026-10-29T12:00:00Z (re-verification after 25-22..25-27)_
_Verifier: Claude (gsd-verifier)_
