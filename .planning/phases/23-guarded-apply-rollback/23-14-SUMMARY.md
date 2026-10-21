---
phase: 23-guarded-apply-rollback
plan: 14
subsystem: deploy
tags: [cli, clap, deploy-rollback, journal-recovery, guarded-op, mcp-exclusion, write-half-guard, readme, wiremock, rust, tdd]
status: complete

requires:
  - phase: 23-guarded-apply-rollback
    provides: 23-11 run_rollback, RollbackContext, deploy_rollback_prepare/execute; 23-12 journal recovery via rollback --run <own id> (restored or cleared); 23-13 the apply verb, EXCLUDED_PATHS, StderrApplyObserver, mount_apply_gateway; 23-21 the README apply section and run_apply helpers; 23-10 the 176 s measured hold; 23-20 the Windows ApplyHost arms
provides:
  - "ign deploy rollback --env --run [--repo] [--discard-live-changes] [--designers] [--designers-timeout]" (DeployCommand::Rollback), guarded at "// guarded:deploy rollback", in GUARDED_OPS
  - actions::deploy_apply::deploy_rollback_prepare_for_env, deploy_rollback_run, RollbackRunOptions (core builds the host; main.rs names no write-half type)
  - ActionOutput::DeployRollback, render_deploy_rollback_human, RollbackKind::as_str
  - StderrApplyObserver with a prefix field (ROLLBACK_PROGRESS_PREFIX "ign deploy rollback:")
  - EXCLUDED_PATHS = [deploy apply, deploy rollback]; the guarded-and-excluded set is {edit, deploy apply, deploy rollback}; a TUI route row; a README Commands row
  - amended write_half_unreachable (ApplyHost and apply_host_for_node forbidden in the three read-only sources, plus a positive confinement walk)
  - README "### `deploy rollback`" section and a journal sentence in "### `deploy plan`"
affects: [23-15, 23-16]

actuals:
  tokens: 13800   # chars/4 over the realized diff (55,171 characters of added lines in README.md and crates/, 1,310 lines added, 22 removed)
  tasks: 2
  commits: 2      # two feat commits; the docs(23-14) completion commit follows

tech-stack:
  added: []
  patterns:
    - "a recovery of an interrupted run is tested through the binary by planting the journal a killed apply leaves, built with the core Journal type from the run's own on-box record (phase written or ready), not by killing a process"
    - "a source-confinement test has a positive half: it walks the source tree and asserts the forbidden names occur only in the named places AND occur in each of them, so a path typo cannot make it vacuous; mutation-checked by adding the name to render.rs"
    - "an MCP exclusion is tested with a slow call in flight: the write-verb tools/call arrive behind it and are answered at once as unknown tools"

key-files:
  created: []
  modified:
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/src/mcp.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-core/src/actions/deploy_apply.rs
    - crates/ignition-core/src/deploy/apply/rollback.rs
    - README.md
    - crates/ignition-cli/tests/contract_deploy.rs
    - crates/ignition-cli/tests/contract_mcp.rs
    - crates/ignition-cli/tests/deploy_family_coverage.rs
    - crates/ignition-core/tests/deploy_transport.rs

key-decisions:
  - "The rollback arm validates --run (RunId::parse) first, then reads only the target file (load_target), builds the read half with for_node, and calls deploy_rollback_prepare_for_env, which checks the id again, builds the apply host with apply_host_for_node and calls the existing deploy_rollback_prepare. The existing deploy_rollback_prepare (host as a parameter, used by the 23-11 core tests) is unchanged."
  - "deploy_rollback_run mirrors deploy_apply_run: it builds the host, takes HoldModel::MEASURED and SystemClock and the same scan wait (500 ms, 2 s ceiling, 120 s), now one scan_poll(subject) helper in deploy_apply.rs."
  - "No new error slug, no new outcome. A rollback has no verdict exit: restored and cleared exit 0, everything else is an error envelope, so the dated apply verdict exception is untouched and the new section says it does not apply."
  - "The positive half of the write-half guard runs inside write_half_unreachable (apply_host_is_confined is a plain helper it calls) so the plan's verify command `write_half_unreachable` exercises it."

patterns-established:
  - "the README section test normalizes whitespace before matching, so a sentence the README wraps still matches"

requirements-completed: []
requirements-advanced: [DAPL-08, DAPL-09, DSURF-02]

duration: ~2h (the workspace test and clippy runs and the release install dominate)
completed: 2026-10-18
---

# Phase 23 Plan 14: The `ign deploy rollback` verb, journal recovery through the binary, and the write-half guard Summary

**`ign deploy rollback --run <id> --yes` restores a past run from a fresh process, the recovery command a `deploy_journal_present` refusal prints recovers an interrupted run through the binary, both write verbs are guarded and outside MCP, and the apply host is provably confined to three places in the source.**

## What was built

### The verb

`ign deploy rollback --env <ENV> --run <ID> [--repo DIR] [--discard-live-changes] [--designers fail|wait|proceed] [--designers-timeout SECS]`, plus the global `--yes` and `--json`. There is no `--plan-sha256`, `--project` or `--skip-gwbk`: a rollback renders nothing and takes no backup. Order in the arm: `RunId::parse` (exit 2 before any contact), `load_target`, `for_node`, `deploy_rollback_prepare_for_env` (journal, record, run-directory cross-check, live changes), then `// guarded:deploy rollback` and `require_confirmation(cli.yes, &prepared.prose)`, then the master (and optional backup) sessions, then `deploy_rollback_run`. The refusal prose is the core's byte-stable text: run, gateway, mode, restore and delete counts, `plan_sha256`, and any live change it would overwrite.

Output: `data` is `DeployRollbackResult` (`env, gateway, mode, run_id, target_run_id, run_dir, outcome, ok, restored, deleted, discarded_live_changes, warnings, restart_pending, restart_pending_new`); outcomes `restored` and `cleared`, both exit 0. Human mode prints `rollback <gateway> (<env>, mode <mode>): <outcome>`, `rolled back run <id>` (or `cleared interrupted run <id>: it had changed nothing, so nothing was restored`), `run`, `run directory`, the counts, any discarded live change and restart lines. Progress lines go to stderr with the prefix `ign deploy rollback:`, human mode only (`--json` stderr stays silent until an error envelope).

### Core entries (main.rs names no write-half type)

`deploy_rollback_prepare_for_env(repo, env, run, reader, opts)` and `deploy_rollback_run(prepared, env, repo, reader, &ApplySurfaces, &RollbackRunOptions, observer)` in `actions/deploy_apply.rs`. `grep -c 'TreeMirror\|mirror_for_node\|apply_host_for_node\|ApplyHost' crates/ignition-cli/src/main.rs` prints 0.

### Surfaces

- `EXCLUDED_PATHS = ["deploy apply", "deploy rollback"]`; `GUARDED_AND_EXCLUDED = ["edit", "deploy apply", "deploy rollback"]`; `GUARDED_OPS` has `deploy rollback`; TUI row `deploy rollback` to `Screen::Projects`; README Commands row (flags, guard, exit behaviour, JSON keys, the 176 s hold, not in MCP).
- `StderrApplyObserver` now has a `prefix` field and `with_prefix`; `Default` keeps `ign deploy apply:`.

### README

`### \`deploy rollback\`` after the apply section: fresh-process rollback by run id, the order of refusals (journal of another run, record problems, live changes and `--discard-live-changes`, confirmation, preflights, the one 176 s hold), no `.gwbk`, journal recovery (the exact command `ign deploy rollback --run <id> --env <env>`, D-10 `cleared`, D-11 no flag and the `interrupted_run` event, D-12 and the manual `.gwbk` or run-directory-snapshot path with the journal deleted by hand only then), A9 (a good deploy restored after a kill past a green verify), A10 (no Ctrl-C handler in v1.6), the 176 s wait after an ssh drop, FA-23-05 (Linux ssh unproven against a live host: `commit.sh` proven only under a local `dash` with `shasum`; modes 0644/0755; staged-file move across file systems is a copy and delete), output, exit codes, and a statement that the dated apply verdict exception does not cover rollback. Facts the apply section already states are pointed to, not repeated: snapshot removal makes a run unrollbackable and manual cleanup ("Runs are never pruned"), when `journal.json` may be deleted by hand, the Windows notes, "After an ssh drop". `### \`deploy plan\`` gained "A journal blocks the plan".

## Tests

| Behaviour | Test |
|---|---|
| apply through the binary, delete `.ign/`, rollback: exit 0, `restored`, config tree equals pre-apply, new run dir `kind: rollback`, a scan | `contract_deploy::rollback_after_apply_restores` |
| no `--yes`: exit 2 `confirmation_required`, names the run, mode, restore count and delete wording; host and gateway untouched | `rollback_without_yes_prints_what_it_restores` |
| `../x`, empty, `a/b`: exit 2 `invalid_input`, zero requests, no `.ign/` | `rollback_bad_run_id_exit_2` |
| journal planted: `plan`, `apply` and `rollback --run OTHER` exit 6 `deploy_journal_present` with the command; nothing written or sent; the command recovers; the next plan exits 0 | `journal_blocks_cli_then_recovers` |
| journal at phase `ready` on a pre-apply tree: `cleared`, human line, journal removed, no scan | `journal_recovery_clears_a_run_that_changed_nothing` |
| later edit: exit 6 `deploy_live_changed` naming the file, nothing restored, no request; with `--discard-live-changes` exit 0 and the path in `discarded_live_changes` and the rollback's `run.json` | `rollback_live_changed_exit_6`, `rollback_discard_live_changes_exit_0` |
| human output and stderr prefix, no apply prefix, stdout free of progress | `rollback_human_output` |
| `deploy_rollback` and `deploy_apply` absent by full path; the guarded-and-excluded set; `deploy render/plan/pull/verify` stay | `mcp::tests::deploy_rollback_is_excluded_by_full_path` |
| `tools/call` of both write verbs while a slow call is in flight: -32601 naming the tool, answered under 2 s, the gateway sees no scan, lock, backup, restart, webdev or resource request | `contract_mcp::mcp_tools_call_write_verbs_unknown` |
| `ApplyHost` and `apply_host_for_node` absent from `actions/deploy.rs`, `main.rs`, `mcp.rs`; present only under `deploy/apply/`, `deploy/transport/` and `actions/deploy_apply.rs`, and present in each | `deploy_transport::write_half_unreachable` |
| the rollback section agrees with the apply section; `deploy plan` names the journal refusal | `deploy_family_coverage::rollback_section_agrees_with_apply_section` |

TDD: the three tracer tests were written first and failed with `unrecognized subcommand 'rollback'`, then the verb made them pass. Task 2's behaviours were already delivered by the verb, so those tests passed on first run; they are real tests, checked by mutation instead of RED: forcing `discard_live_changes: false` and `require_confirmation(true, ..)` in the arm failed `rollback_discard_live_changes_exit_0` and `rollback_without_yes_prints_what_it_restores`; removing `deploy rollback` from `EXCLUDED_PATHS` failed `mcp_tools_call_write_verbs_unknown`; adding `// ApplyHost` to `render.rs` failed `write_half_unreachable`. Each file was restored with `git checkout`/the saved copy.

## Verification

- `cargo test -p ignition-cli --test contract_deploy rollback_` and `journal_`: 8 passed; `mcp::tests`: 17 passed; `deploy_family_coverage`: 7; `tui_coverage`: 5; `contract_mcp`: 13; `deploy_transport write_half_unreachable`: 1.
- `cargo test --workspace --no-fail-fast`: 2459 passed, 0 failed, 56 ignored (live and Windows-host gates); 2448 before this plan. The known flake `live_gateway::guard_drop_during_unwind_inside_runtime_still_cleans_up` did not fail.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check`: clean.
- `grep -n '// guarded:deploy rollback' crates/ignition-cli/src/main.rs`: one line. `grep -c '^### `deploy rollback`' README.md`: 1.
- `cargo install --path crates/ignition-cli --force` was run and `ign deploy rollback --help` and a bad `--run` read back from the installed binary.
- No live step: this plan has none. The trial was not touched and no reset was made; no ssh or Windows host was used.

## Deviations from Plan

### Plan departures

1. **[Plan structure] Task 2's behaviours were not RED.** The verb from Task 1 already implemented the journal, live-change and human-output paths, so the Task 2 CLI tests passed when first written. They are mutation-checked (above). Test and implementation of Task 1 share a commit, as in 23-13 and 23-21.
2. **[Plan departure] The journal is planted, not left by a killed process.** The plan says "built with the core fixtures". The core fixtures (`FaultyHost`, `ApplyFault`) are in-process and not reachable through the binary, and a real kill mid-apply through the mock gateway would be a timing race. The CLI test applies for real, then writes a `journal.json` (phase `written`, or `ready` for the cleared case) with the core `Journal` type from the run's own on-box record. The real kill-then-recover is `journal_kill_mid_write_then_recover` in core and `live_kill_mid_write_then_recover` in 23-16.
3. **[Rule 2 - Missing critical functionality] New core entries instead of main.rs building the host.** `deploy_rollback_prepare` takes an `ApplyHost`, and the amended write-half guard forbids main.rs from naming one, so `deploy_rollback_prepare_for_env` and `deploy_rollback_run` were added (as the plan says "the host built inside the action"). The 23-11 signature and tests are untouched.
4. **[Rule 2 - Missing critical functionality] `RollbackKind::as_str`** added in core so the renderer prints the outcome from the type (the same reason 23-13 added `OutcomeKind::as_str`).
5. **[Rule 2] A `cleared` outcome has its own human line.** The plan's `render_deploy_rollback_human` covers it ("outcome `restored` or `cleared`"); saying "rolled back run" for a run that changed nothing would be wrong, so it prints `cleared interrupted run <id>: it had changed nothing, so nothing was restored`. Pinned by `journal_recovery_clears_a_run_that_changed_nothing`.
6. **[Scope] An extra test, `journal_recovery_clears_a_run_that_changed_nothing`,** and `rollback_section_agrees_with_apply_section`, beyond the plan's list, for the `cleared` rendering and the README agreement the carry-overs asked for.
7. **[Wording] The positive confinement walk lives inside `write_half_unreachable`** (via a helper) rather than as a second test, so the plan's `write_half_unreachable` verify command runs it.
8. **[Plan wording] The README names `Linux ssh unproven` once, in the rollback section.** The apply section's Commands row and section do not carry the note; the carry-over put it in the rollback section, so the apply section was left alone.

No auth gates. No Rule 1 bug found. No architectural change (Rule 4 not triggered). No new error slug, so the four-place rule was not triggered (`readme_exit_table_agreement` and `exit_code_mapping_enumerated` stay green).

## Tracer feedback gate

Task 1 is the tracer. Auto mode is off (`workflow.auto_advance` false) and `human_verify_mode` is `end-of-phase`, and the orchestrator directed a sequential unattended run, so execution continued after the tracer commit (`f322087`) instead of stopping for a `checkpoint:human-verify`, as in 23-10 to 23-13 and 23-21. The tracer's `<verify>` passed before the commit and the whole workspace passed after Task 2. For the end-of-phase list: the refusal prose from the binary (`ign --json deploy rollback --env dev --run <id>` without `--yes`) and an apply followed by a rollback against a stateful mock gateway.

## Known Stubs

None. A grep for `TODO`, `FIXME`, `todo!`, `unimplemented!`, `#[ignore]` and `skip` over the added lines finds nothing.

## Threat Flags

None beyond the plan's register. T-23-58 (MCP reaching a write verb): `deploy_rollback_is_excluded_by_full_path`, `mcp_tools_call_write_verbs_unknown`. T-23-59 (write half reachable from a read-only verb): the amended `write_half_unreachable`. T-23-60 (unconfirmed rollback): `rollback_without_yes_prints_what_it_restores`. T-23-61 (stuck behind a journal): `journal_blocks_cli_then_recovers` and the README recovery text.

## Requirements

`requirements-completed` is empty by instruction. Advanced: DAPL-08 (rollback from a fresh process through the binary), DAPL-09 (journal recovery through the binary), DSURF-02 (both write verbs guarded and outside MCP).

## Carry-overs

### 23-15 (canary, stdout purity)

- `apply_through_binary(server, repo, extra)`, `rollback_args(repo, run, json, yes)`, `config_tree`, `plant_journal`/`plant_journal_at`, `edit_live_unit` are in `contract_deploy.rs` next to the 23-13 and 23-21 helpers.
- The rollback `run.json` holds `discarded_live_changes` (paths), `target_run_id`, hold and preflight verdicts; its `events.ndjson` has `interrupted_run` (every touched path with its live hash) and `live_changes_discarded`. The canary scan should treat them as the apply's paths. `rollback_human_output` asserts progress is stderr-only; a `--json` rollback leaves stderr silent until an error envelope. The refusal prose (`confirmation_required`) names paths and counts only.
- A `--json` rollback writes exactly one envelope on success; the error cases are one JSON value on stderr with empty stdout (`rollback_live_changed_exit_6`, `rollback_bad_run_id_exit_2` assert it). The stdout-purity cases for `deploy rollback` (success, `cleared`, error) are still to add.

### 23-16 (live)

- `ign` on PATH was reinstalled at `9e2f676` (release). A live `ign deploy apply` then `ign deploy rollback --run <id> --yes` on `ign-win-836` is the end-to-end proof of the Windows arm; keep payloads small. Rollback takes its own 176 s hold, so a two-file apply plus rollback is about two minutes of lock time.
- Not yet exercised live: a rollback after a real kill (`live_kill_mid_write_then_recover`), and the A9 window. The README states both; the planted-journal CLI test is the only through-binary evidence now.
- The README says Linux ssh is unproven against a live host (FA-23-05); if a Linux host becomes available, that sentence and the `commit.sh` conformance note are the ones to update.

## Self-Check: PASSED

- Files exist: `crates/ignition-cli/src/{cli,main,render,mcp}.rs`, `crates/ignition-tui/src/routes.rs`, `crates/ignition-core/src/actions/deploy_apply.rs`, `crates/ignition-core/src/deploy/apply/rollback.rs`, `crates/ignition-cli/tests/{contract_deploy,contract_mcp,deploy_family_coverage}.rs`, `crates/ignition-core/tests/deploy_transport.rs`, `README.md`, this SUMMARY.
- Commits `f322087` and `9e2f676` exist in `git log`.
- `grep -c 'fn rollback_after_apply_restores' crates/ignition-cli/tests/contract_deploy.rs` prints 1; `grep -c 'fn mcp_tools_call_write_verbs_unknown' crates/ignition-cli/tests/contract_mcp.rs` prints 1.
