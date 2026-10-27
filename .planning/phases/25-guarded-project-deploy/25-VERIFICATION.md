---
phase: 25-guarded-project-deploy
verified: 2026-10-27T12:00:00Z
status: human_needed
score: 5/5 roadmap success criteria verified; 20/20 plans' artifacts present and wired; 5/5 requirement IDs satisfied; G-25-4 closed
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: human_needed
  previous_score: "5/5 roadmap success criteria verified; 19/19 plans"
  gaps_closed:
    - "G-25-4: Windows release-record replace was not atomic (Move-Item -Force is delete-then-rename on PowerShell 5.1); closed by plan 25-20 (1f69b62, 4d57b9f, 7ff43b8)"
  gaps_remaining: []
  regressions: []
  environment_issues:
    - "Local macOS pwsh 7.5.4 crashes at startup for any command (FileLoadException on System.Collections.Specialized); 3 winps tests that spawn it fail. Host-tool breakage, not a code defect."
gaps: []
deferred: []
behavior_unverified_items: []
human_verification:
  - test: "First real run of .github/workflows/deploy-projects.yml on a tag (for example v0.0.1-test) with a dev environment, from a project repo caller"
    expected: "Sparse cross-repo targets checkout works with IGN_TARGETS_TOKEN and the token is masked in the log; IGN_MASTER_TOKEN / IGN_SSH_KEY reach the apply job only through its own environment (the secret split); check's artifact_sha256 and targets_sha reach apply through needs and workflow_call outputs; apply waits for approval and deploys exactly the checked hash"
    why_human: "GitHub-only behaviour (secret masking, environment secrets in reusable workflows, needs outputs). The local harness runs the extracted run/script bodies against a mock gateway and actionlint, nothing more."
  - test: "Concurrency queue of deploy-projects.yml: start two applies for the same gateway and project"
    expected: "One running, one pending; a newer pending apply replaces the older pending one (cancel-in-progress false); check what happens to a prod job that needs the replaced dev job"
    why_human: "GitHub concurrency semantics cannot be exercised from this machine. Accepted behaviour recorded in 25-18 deviation 7."
  - test: "Run the workflow on a self-hosted Windows runner (Git Bash, Win32-OpenSSH, cygpath)"
    expected: "Install, config, targets and run steps behave as on Linux"
    why_human: "Windows runner behaviour is unprovable locally"
  - test: "Compile and run the non-unix (portable) variant of local_fs.rs on a Windows target"
    expected: "Builds and the record writer's temp-file cleanup (WR-04) behaves as on unix"
    why_human: "Blocked in UAT (needs a Windows Rust build environment). The Windows std target is not installed for the toolchain in use (cargo check --target x86_64-pc-windows-msvc fails with E0463 on `core`). Verifier substitute: the portable module text was copied into a scratch crate and type-checks on macOS with no errors (warnings only: dead code). Behaviour was not run."
---

# Phase 25: Guarded Project Deploy Verification Report

**Phase Goal:** A user, or a project repo's CI, can deploy one Ignition project from its own repo to a gateway with `ign deploy projects`: the artifact built from the checkout is verified against its reviewed hash before any upload, the project's parent must already be on the gateway and is never imported or modified by the child's deploy, the live project is exported before it is overwritten and hash-verified after import, a project absent from the gateway is created, and no other gateway project is ever imported, modified or deleted.
**Verified:** 2026-10-27T12:00:00Z (initial verification 2026-10-24T14:50:35Z)
**Status:** human_needed (no gaps; the open items are GitHub-only or Windows-only behaviour that UAT could not reach)
**Re-verification:** Yes, after gap closure plan 25-20 (G-25-4)

## Re-verification: gap closure 25-20 (G-25-4)

**Gap:** UAT test 4 (live race on ign-win-836, 2026-10-26) showed that the Windows release-record replace, `Move-Item -Force` on PowerShell 5.1, is delete-then-rename. Concurrent readers saw `release.json` missing (175 and 55 of about 17k local reads, 1 of 381 sftp reads). `read_release_record` maps a missing file to `None`, which means "never deployed here". 0 torn reads.

**Verdict: CLOSED.** Evidence checked in the codebase, not from the SUMMARY:

| 25-20 must-have | Status | Evidence |
|-----------------|--------|----------|
| Windows replace is a single `MoveFileEx(MOVEFILE_REPLACE_EXISTING \| MOVEFILE_WRITE_THROUGH)` via Add-Type P/Invoke | VERIFIED (code) | `winps.rs` `RECORD_REPLACE`: `$f=[uint32](0x1 -bor 0x8)`, `MoveFileEx(string a,string b,uint c)` with `SetLastError=true`, `kernel32.dll`; the text contains no `Move-Item` (pinned by `record_replace_is_fixed_fail_closed_text`, which I ran: ok). `project_ops.rs` `replace_record` Windows arm now calls `WinScript::record_replace(..., RENAME_RETRIES)`; `grep Move-Item` in `project_ops.rs` finds only the doc comment that says it was retired. |
| Fails closed, never a delete-then-rename fallback | VERIFIED (code) | Script throws unless `LanguageMode -eq FullLanguage`; `Add-Type` is in `try{...}catch{throw ...}`; a non-sharing Win32 error or exhausted retries throws. The writer maps the thrown error to `transport_failed`; no other replace path exists in the arm. The live preflight asserts FullLanguage on the capture host. |
| Blocked replace retried a bounded number of times (`RENAME_RETRIES`=3, 1 s pauses, count only in the blob), then `transport_failed`, old record intact, temp removed | VERIFIED (code, offline stub) / PRESENT, live path not exercised | Retry loop in `RECORD_REPLACE`; the unit test pins that the text outside the blob is the same for retries 3 and 7. The sftp mock test (`deploy::transport::sftp` lib tests, 107 passed) shows temp removed and old record kept on a failed replace with a canned runner. Review WR-03: the real script's blocked path was never run live (see Warnings). |
| Live race cannot pass vacuously; control shows missing, shipped shows 0 torn/0 missing/0 backwards with floors | VERIFIED (recorded live evidence, not re-run by this verifier) | `25-LIVE-CAPTURES.md` `### Windows atomic record replace (G-25-4)`: preflight ok (PS 5.1.20348.5756, FullLanguage, `{"ok":1}`); negative control `MOVE_REPLACE` missing 3 + 1 + 1 = 5; shipped `RECORD_REPLACE` over 300 s fallback constants: torn 0, missing 0, backwards 0 on both local readers and the sftp reader; replace_ok 318 (floor 300), local reads 18754 (floor 10,000), sftp reads 527 (floor 300), distinct versions 317/314/144 (floors 100/100/20). `race test: 1 passed, 685.98 s`. Per instruction I did not run or start anything live. Calibration projected 90 per 120 s against an expected 500 to 3000 and went straight to the fallback constants as the plan's rule says; the replace_ok margin is thin (18). |
| `write_half_unreachable` and `release_writer_is_confined` green; no new slug, exit code or README exit-table row; Linux `mv -f` and `commit.ps1` unchanged | VERIFIED | I ran `write_half_unreachable` (it calls `release_writer_is_confined` at `deploy_transport.rs:2899`): ok. `readme_exit_table_agreement` and `exit_code_mapping_enumerated`: ok. `git diff 1f69b62~1 HEAD` over `crates/` touches exactly six files (`winps.rs`, `sftp.rs`, `project_ops.rs`, `projects.rs`, `live_transport_wflwvm01.rs`, README): no `error.rs`, no `commit.ps1`; `commit.ps1` still has its own `MoveReplace`/`Move-Item` (apply mirror move, out of scope by the prohibition). |

Prohibitions (`must_haves.prohibitions`, both `flagged`): the Linux/apply-mirror scope prohibition is verified by the diff above; the no-new-slug prohibition is verified by the diff and the passing exit-table tests. Neither is an unenforced test-tier item.

Truths 1 to 5 of the roadmap are unchanged by this plan except SC3 (record write after a verified import): the previously unproven Windows record-replace atomicity is now proven live, and the old open owner question is resolved. UAT test 5 (WR-02 judgment) passed in the owner's UAT, so that human item is removed.

### Test run and the host `pwsh` failure

The orchestrator's `cargo test --workspace --no-fail-fast` on 2026-10-27 gave 2901 passed and 3 failed (`winps_script_logic`, `winps_enumerate_reports_files_dirs_and_reparse_points`, `winps_enumerate_rel_paths_do_not_depend_on_the_root_spelling`). Classification: **environment issue, not a gap.** Independent evidence:

- On this machine `pwsh -NoProfile -Command '1+1'` crashes before running anything: `Unhandled exception. System.Collections.Specialized ... FileLoadException: The given assembly name was invalid`, with a corrupted `PublicKeyToken` in the file name. The interpreter cannot evaluate `1+1`, so no change to the crate's script text can be the cause. (`pwsh` is `/usr/local/bin/pwsh`, PowerShell 7.5.4, installed under `/usr/local/microsoft/powershell/7`.)
- The 25-20 diff to `winps.rs` has four hunks (lines 15, 72, 212, 694: doc, new const, new constructor, new unit test). The three failing tests (`winps_script_logic` at line 997 onward, and the two enumerate tests at about 1044 and 1114) are not touched, and they spawn the host `pwsh` through `std::process::Command` (`-NoProfile -NonInteractive -EncodedCommand`).
- The new offline test `record_replace_is_fixed_fail_closed_text` does not spawn `pwsh` and passes. The 2900 passes of the prior full run (before 25-20) plus 25-20's one added winps test and the live gate's offline tests account for 2901.
- Targeted runs by this verifier (own scratchpad `CARGO_TARGET_DIR`, no worktree): `record_replace_is_fixed_fail_closed_text`, `readme_exit_table_agreement`, `exit_code_mapping_enumerated`, `write_half_unreachable` (includes `release_writer_is_confined`), and the whole `deploy::transport::sftp` lib module (107 tests): all ok. I did not rerun the full workspace; a rerun on a host with a working `pwsh` (or after reinstalling PowerShell 7.5.4) should be green. This is recorded as an environment issue, to be fixed on the host.

### Review 25-20-REVIEW.md

0 critical, 4 warnings, 4 info, all advisory and none contradicts a must-have. WR-01 (Add-Type failure discards its cause; production pays an `Add-Type` compile, about 2.7 s, per replace and depends on `csc.exe` and a writable TEMP, so a hardened host will end with `record_written:false`, fail-closed by design) and WR-03 (blocked-replace path never run live with the real script) are the two worth a follow-up. WR-02 and WR-04 weaken the test gate's margins only. IN-03: the README states "never a missing or half-written file" unconditionally; it holds on NTFS, and a replace blocked past the retries yields `transport_failed` with the old record kept (about 10% of attempts under a constant two-reader load in the race, 34 of 352).

### Still open (UAT blocked on infrastructure, unchanged)

UAT tests 1, 2, 3 and 6 (GitHub first run, concurrency queue, self-hosted Windows runner, Windows build of the portable `local_fs.rs`) remain blocked, so the status stays `human_needed`. `25-UAT.md` still shows G-25-4 `status: failed` and test 4 `result: issue`; the orchestrator should mark both resolved by 25-20.

## Evidence base

- Code read directly: `crates/ignition-core/src/actions/deploy_projects.rs` (prepare, run, execute, sweep), `deploy/projects.rs` (project file, release, record, parent chain), `deploy/project_artifact.rs` (artifact and canonical digest), `deploy/transport/projects.rs` (read-only `ProjectHost`, confined `ReleaseRecordWriter`), `client/mod.rs` and `client/classify.rs` (import), `ignition-cli/src/main.rs` (dispatch, Decision C, `GUARDED_OPS`), `mcp.rs`, `routes.rs`, `error.rs`, README, `deploy-projects.yml`.
- `CARGO_TARGET_DIR=<scratchpad>/target cargo test --workspace`: exit 0, 94 result lines, 2900 passed, 0 failed, 82 ignored (live suites). `cargo clippy --workspace --all-targets -- -D warnings`: clean. `cargo fmt --all -- --check`: clean. This matches the 25-19 gate (2887) plus the WR fixes (2900 in the fix report).
- Quick-task harness (`.planning/quick/261021-m5y-.../harness`): `projects-test.sh`, `plan-test.sh`, `apply-test.sh`, `readme-test.sh` all ran to their last PASS line with no failure.
- Live 8.3.6 rig evidence read from `25-LIVE-CAPTURES.md` (`## Live gates (25-19)`); the rig was torn down and no rig or host was contacted by this verification.
- Debt-marker scan (`TBD|FIXME|XXX|todo!|unimplemented!`) over the 52 files changed since 2026-10-21: no hit (one doc comment in `cli.rs` that only mentions the word).
- `verify.artifacts` over all 19 plans: every artifact passes. `verify.key-links` reported 7 misses that were tool regex errors (`validate_import\(` and similar) or pattern-in-other-file; each link was confirmed by hand (all present).

## Goal Achievement

### Observable Truths (ROADMAP success criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `ign deploy projects` verifies the artifact's canonical hash against `--artifact-sha256` before any upload; mismatch refuses `project_hash_mismatch`, nothing uploaded | VERIFIED | `load_project` builds the artifact offline and compares `*expected != artifact.digest.sha256` before any host or gateway object exists (`main.rs` builds the host and session only after `load_project` returns). Format check refuses non-64-lowercase-hex as `invalid_input`. Decision C in `main.rs`: missing hash with `--json`, `--compact` or a non-terminal is `invalid_input` unless `--check`. Slug is exit 6 and in README exit table (pinned by `readme_exit_table_agreement`, passing). The workflow passes `needs.check.outputs.artifact_sha256` to apply. Live: installed binary check then apply with the printed hash gave `created`/`imported`. The wrong-hash refusal is proven by the CLI and engine suites and the harness against mock gateways, not live. |
| 2 | Parent read from `project.json`, never imported/exported/modified; missing parent or a cycle refuses before any import; a hash-equal project is skipped; after a parent deploys, children are hash-checked against their last `ign` deploy and a mismatch is a warning | VERIFIED | `read_project_meta` (non-string `parent` refused, WR-07). `parent_chain` is bounded by project count; `check_parent` refuses self, chain cycle, `link.parent == project`, missing, not inheritable as `project_parent_invalid`, from the single `projects/list` read (no per-parent export or write). The gateway trait has one write, `project_import`, called once with the declared project name. `ProjectAction::Skip` only when both member sets are non-empty and digests equal and the record names this release; equal content with an absent or different record is `record` (record only, K14, user-approved). `sweep_children` runs after `imported`/`imported_unverified`, only reads, compares each child with its own record, mismatch is a warning, never changes outcome or exit. Live: `parent: CAP25-NOPE` refused `project_parent_invalid`/`missing` with GET only; `skipped` with no `.ign/` and GET only; child `ok` then `mismatch` with a warning and outcome still `imported`. `cycle` and `not_inheritable` proven by engine tests only (`parent_cycle_self_refuses`, `parent_cycle_chain_refuses`, `parent_not_inheritable_refuses`, `parent_chain_is_bounded`), not live. |
| 3 | Each project is exported before it is overwritten and, after import, verified by canonical hash with `lastModification*` and signatures ignored | VERIFIED | `execute`: for `import`, `export_bytes` then 0600 `export/<project>.zip`, then K15 live-digest recheck (`plan_stale` if the live project moved), then `project_import`, then export again and `live.sha256 == artifact.sha256`. Equal writes the record; unequal is `imported_unverified`, `ok:false`, slug `deploy_project_unverified`, `restore_command`, exit 6 after the envelope, no record, no second import. `normalize_descriptor` drops `lastModification` and `lastModificationSignature`; `canonical_resource_json` also drops an empty `attributes` and sorts `files`; `canonical_project_json` defaults `description` and `parent` (K4 rows, user-approved). Live: gateway re-export canonical digest equals the artifact digest on create and on import; the pre-image digest equals the previous deploy's. Tests: `export_before_import_and_verify`, `live_changed_since_plan_is_stale`, the four WR-02 tests. `imported_unverified` was not induced live (stated by 25-19). |
| 4 | Absent project is created (`created`); unnamed gateway projects are reported, never imported/modified/deleted; a project with `.git` is refused; `--check` reports with zero writes | VERIFIED | `ProjectAction::Create` imports with `overwrite=false`, no pre-image, outcome `created` (D-13). `unmanaged` lists other names only (byte-sorted, capped 200 with count). No gateway or host method deletes, renames, copies or modifies (`ProjectGateway` has four reads and one write; `ProjectHost` has two reads; the record writer is a separate confined trait, pinned by the passing source scans `write_half_unreachable` and `release_writer_is_confined`). `.git` of any kind (directory, file, symlink, other) is `project_git_managed` before any gateway read; an unreadable probe is `transport_failed`, never absent. `--check` returns right after `deploy_projects_prepare`, which creates no run directory. Live: GET-only for create, import, record and skip plans (method tap), no `.ign/`, no record; `.git` directory under the rig data root refused in apply and check with zero requests; project list unchanged. |
| 5 | `projects` joins `GUARDED_OPS`, stays out of the MCP catalog, lands with TUI row, README entry and exit-table rows; output and run files pass the secret canary | VERIFIED (one documented exemption) | `GUARDED_OPS` has `deploy projects` with a `// guarded:deploy projects` marker (drift test passing). `mcp.rs` `EXCLUDED_PATHS` includes `"deploy projects"`; `deploy_projects_is_excluded_by_full_path` passes. `routes.rs` has the `deploy projects` row. README has the Commands row, a `### deploy projects` section, CI section and exit rows for the three new slugs. Canary suites (`projects_canary_apply_json_and_human`, `projects_canary_check_and_verdict`, `projects_stdout_one_envelope`, core `canary_*`) pass. The scan exempts exactly `PROJECTS_CANARY_EXEMPT = ["export"]`: the 0600 pre-image `export/<project>.zip` in a 0700 directory, which is project content by definition; the tests assert it exists, holds the canary and is private, so the exemption is not vacuous. This mirrors the Phase 23 snapshot exemption and was accepted by the user at the 25-17 human-verify gate. See Warnings for IN-08. |

**Score:** 5/5 truths verified, 0 present-but-behavior-unverified.

### Plan-level truths and CONTEXT decisions (spot-checked in code)

| Item | Status | Evidence |
|------|--------|----------|
| D-01 hash is the pin, no committed `ProjectPin` needed (D-04) | HONOURED | `ProjectsInput.artifact_sha256`; manifest `projects` field not read by the verb |
| D-02 release tag per environment job | HONOURED (code), human for GitHub | `--release` semver; workflow has check then environment-gated apply per call |
| D-03 downgrade refusal needs a durable record | HONOURED | `release.json` record, gate `downgrade`, `--allow-downgrade`; live: exit 6 for an older release |
| D-05 / D-06 parent never touched, children warned | HONOURED | see SC2 |
| D-07 / D-14 `--targets DIR` | HONOURED | 7 `--targets` args in `cli.rs` (projects plus render, plan, pull, verify, apply, rollback); `known_hosts` resolves against the targets root (`project_host_known_hosts_from_targets_root`) |
| D-08 / D-09 `deploy/project.yaml`, artifact rules | HONOURED (D-09 narrowed by approved K2/K3) | strict `deny_unknown_fields`, `schema_version` 1, name checked as a path component on unix and Windows |
| D-11 `ign` never deletes `.git` | HONOURED | `ProjectHost` has no write method |
| D-12 first deploy changes nothing | HONOURED | gate `first_deploy`, `--allow-first-change <REASON>` (1 to 500 chars) |
| D-13 create when absent | HONOURED | outcome `created`, no extra flag |
| D-15 reusable workflow in this phase | HONOURED (written and harness-tested; GitHub run is human) | `.github/workflows/deploy-projects.yml`; separate check secrets (`IGN_CHECK_TOKEN`, `IGN_CHECK_SSH_KEY`) so the ungated job holds no write secret; targets commit pinned from check to apply (WR-06) |
| D-16 ROADMAP goal and criteria reworded | HONOURED | ROADMAP text matches CONTEXT |
| Contract decisions (25-03) K1-K15, V1-V11 | HONOURED | each approved item that departs from plan text (K2, K3, K4, K6, K7 = 30 s, K9 `stat`) is what the code does |
| No write to the backup node | HONOURED | only `nodes.master` is used for import, host reads and the record; `backup` appears only in comments |
| No second overwrite after a failed verify | HONOURED | `execute` has one `project_import` call site; no retry path (WR-02 reads back, never re-sends) |
| Release record never read as absent when untrustworthy | HONOURED | `ReleaseRecord::parse` strict, size cap, symlink/non-regular refusal, `deploy_run_invalid` |

### Required Artifacts

All artifacts named by the 19 plans' `must_haves` exist and pass `verify.artifacts` (19/19 plans, no STUB or MISSING). Core ones: `actions/deploy_projects.rs` (1869 lines), `deploy/project_artifact.rs` (1813), `deploy/projects.rs` (910), `deploy/transport/projects.rs`, `transport/local/project_ops.rs`, `transport/sftp/project_ops.rs`, `tests/deploy_projects.rs` (3026), `tests/contract_deploy_projects.rs`, `tests/live_deploy_projects.rs` (1210), `.github/workflows/deploy-projects.yml`, `25-LIVE-CAPTURES.md` with `## Contract decisions (25-03)` and `## Live gates (25-19)`.

### Key Link Verification

| From | To | Status | Details |
|------|----|--------|---------|
| `main.rs` `DeployCommand::Projects` | `load_project`, `deploy_projects_prepare`, `deploy_projects_run` | WIRED | order: Decision C, offline build and hash compare, host, session, prepare, confirmation, run |
| `deploy_projects.rs` | `validate_import` (promoted), `canonical_digest`, `check_parent`, `designers_gate`, `read_release_record` | WIRED | confirmed by hand where the tool regex failed |
| `mcp.rs` `EXCLUDED_PATHS` | clap leaf `deploy projects` | WIRED | full-path exclusion, test passing |
| workflow `check` outputs | workflow `apply` | WIRED (static) | `artifact_sha256` and `targets_sha` via `needs`; runtime unproven |

### Data-Flow Trace (Level 4)

| Artifact | Data | Source | Real data | Status |
|----------|------|--------|-----------|--------|
| `DeployProjectsResult.live_sha256` / `members` / `removed` | live export | `GatewayProjects::export_bytes` to `canonical_digest` | yes (live rig equality) | FLOWING |
| `unmanaged` | `projects/list` | `GatewayApi::projects` | yes | FLOWING |
| `children[].live_sha256` | child exports | `export_bytes` per child | yes (live: ok and mismatch) | FLOWING |
| `previous_release` / `recorded_sha256` | host record | `read_release_record` | yes (live) | FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Workspace tests | `cargo test --workspace` (scratchpad target dir) | exit 0; 2900 passed, 0 failed, 82 ignored | PASS |
| Lint and format | `cargo clippy --workspace --all-targets -- -D warnings`; `cargo fmt --all -- --check` | clean | PASS |
| CI harness | `projects-test.sh`, `plan-test.sh`, `apply-test.sh`, `readme-test.sh` | all PASS lines, no failure | PASS |
| Exit-table agreement | `error::tests::readme_exit_table_agreement`, `exit_code_mapping_enumerated` | ok | PASS |
| Deferred-items test `timeout_kills_child` (flaky alone in the sandbox per 25-13) | in the full run | ok | PASS |
| Windows target compile | `cargo check --target x86_64-pc-windows-msvc -p ignition-core` | E0463 (std not installed for the toolchain) | SKIPPED, see Human Verification |
| Portable `local_fs.rs` module type-check | module text copied to a scratch crate, `cargo check --offline` | no errors, dead-code warnings only | PASS (compile only) |

### Probe Execution

No `scripts/*/tests/probe-*.sh` files exist and no plan declares a probe. Step 7c: not applicable.

### Requirements Coverage

Every ID declared in the PLAN frontmatter (`requirements:` of 25-01 to 25-19) is DPRJ-01 to DPRJ-05, and REQUIREMENTS.md maps exactly those five to Phase 25 (all marked Complete). No orphaned IDs.

| Requirement | Source plans | Status | Evidence |
|-------------|--------------|--------|----------|
| DPRJ-01 artifact verified by hash before any upload | 01, 03, 04, 05, 06, 11, 16, 18, 19 | SATISFIED | SC1. The pin is `--artifact-sha256` per D-01/D-04, not `ProjectPin.sha256` |
| DPRJ-02 parent rules, cycles/unknown parents refuse, unchanged skipped | 01, 02, 03, 04, 05, 09, 10, 15, 19 | SATISFIED | SC2. Parent-first ordering is replaced by "parent must exist, never imported" per D-05 |
| DPRJ-03 export before overwrite, canonical-hash verify | 01, 03, 06, 11-15, 17, 19, 20 | SATISFIED | SC3; 25-20 declares DPRJ-03 and closes the Windows record-replace atomicity gap |
| DPRJ-04 unnamed projects reported never deleted; `.git` refused | 02, 03, 04, 05, 07, 08, 11, 17, 19 | SATISFIED | SC4 |
| DPRJ-05 `--check` zero writes | 03, 04, 05, 09, 11, 12, 17, 18, 19 | SATISFIED | SC4/SC5, live GET-only tap |

### Anti-Patterns Found

None blocking. No TBD/FIXME/XXX/todo!/unimplemented! in changed files (re-checked over the added lines of the 25-20 diff); no stub return paths; `#[ignore]` only on live suites with reasons.

### Warnings (non-blocking, none contradicts a success criterion)

1. **README Commands row for `deploy projects` is stale** (`README.md:504`, review IN-02, not fixed). It says a deploy goes "for now, to a node declared with `local`" although ssh Linux and Windows nodes are implemented (25-08, 25-13, and the detailed sections say so), and its `JSON data` key list omits `children`. The README is the agent contract, so an agent reading only the table is misled. The `### deploy projects` section is correct. Suggested fix: drop the clause and add `children` before `outcome`.
2. **REQUIREMENTS.md wording lags D-04/D-05/D-16.** DPRJ-01 still says "manifest sha256 pin", DPRJ-02 "import parent-first", DPRJ-04 "not in the manifest". D-16 kept the IDs but only the ROADMAP text was reworded. Satisfied under the reworded intent; reword the three lines so the traceability table is not misleading.
3. **WR-02 mislabel risk.** `import_outcome_unknown` treats `CoreError::Internal` (what `classify` returns for an unclassified 4xx/5xx on a non-api-call route) as "outcome unknown". A deterministic gateway 4xx on import would then read back and report `imported_unverified` (exit 6) instead of a plain error. Fail-safe (no second import, no record) but possibly confusing. Flagged by the fixer for owner judgment (human item above).
4. **Interactive path without `--artifact-sha256`.** In a terminal the hash is optional (Decision C, CONTEXT D-01): the confirmation prose prints `artifact_sha256`, and `--yes` skips the prompt. The hash gate is enforced whenever the flag is given and is mandatory for `--json`, `--compact` and non-terminal use (CI). This is the design the user chose; noted so SC1 is not read as "always".
5. **Children sweep does not run after `created`, `recorded` or `skipped`** (documented in the result model). A newly created parent that already has orphaned children is not swept. Edge case; SC2 ("after a parent deploys") is met for the overwrite path.
6. **Unfixed review Info items:** IN-05 (`--check` runs the `.git` probe but not the wrong-target comparison; the apply does), IN-06 (mutable action tags, `IGN_TARGETS_TOKEN` reaches the ungated check job by design), IN-07 (canonical form relies on `serde_json` keeping `BTreeMap`), IN-08 (the 0600 pre-image stays in the workspace of a self-hosted runner after the apply job; add an `if: always()` cleanup step), IN-01/IN-03/IN-04. None blocks a criterion; IN-08 is the one worth acting on before unattended prod use on a shared runner.
7. **ROADMAP checklist line** `- [ ] **Phase 25: ...` (ROADMAP.md:91) and STATE.md ("Phase 25 execution started", 2026-10-22) have not been updated; the orchestrator owns that.
8. **Size cap and memory (25-06).** The cap applies to the built zip, so a large checkout is built in memory first. The rig projects were a few KB. Unverified for large projects (carried open point).

### Open owner questions (noted, not phase-25 criteria, not failed)

- Long-term branch lifecycle of module-managed projects (K8 follow-up).
- Images and themes after cutover (25-17 open question).
- S5 session residual (child session impact after a parent overwrite); no criterion claims it; the sweep reports hash drift only.
- Windows `Move-Item -Force` atomicity: resolved by 25-20 (G-25-4).
- `deploy-plan.yml`'s ungated plan job holds `IGN_SSH_KEY` (pre-existing; recorded as an owner follow-up in 25-18).
- Cutover prerequisite (spec step 8: remove `.git` or uninstall the git module on the target) is human work and was not performed here.

### Regression check

Phase 20 to 24 VERIFICATION.md files are all `passed`. Phase 25 touched shared code those phases own (`error.rs` slug table 12 to 15, `preflight.rs` designers gate extraction, `ledger.rs`, `run_store.rs`, `deploy_apply.rs` `--targets`, `deploy-plan.yml` and `deploy-apply.yml` inputs). The full workspace suite, which contains those phases' contract and engine tests, passes with 0 failures, and the plan and apply harnesses pass, so no regression was found.

### Human Verification Required

See the `human_verification` list in the frontmatter (GitHub first run and masking/secrets, concurrency queue, Windows runner, Windows compile of the portable `local_fs.rs`). All four are blocked in UAT on infrastructure. Windows record-replace atomicity (G-25-4) and the WR-02 judgment are no longer open. These are the only reason the status is `human_needed` rather than `passed`.

### Gaps Summary

No gaps. All five success criteria are met by code that was read, by a green full test run, and (for the gateway-facing parts) by recorded live runs on the 8.3.6 rig. What remains is verification that can only happen on GitHub, on Windows, or by the owner.

---

_Verified: 2026-10-27T12:00:00Z (re-verification; initial 2026-10-24T14:50:35Z)_
_Verifier: Claude (gsd-verifier)_
