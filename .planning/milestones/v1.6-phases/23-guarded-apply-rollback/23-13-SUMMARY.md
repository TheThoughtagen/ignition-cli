---
phase: 23-guarded-apply-rollback
plan: 13
subsystem: deploy
tags: [cli, clap, deploy-apply, guarded-op, decision-c, mcp-exclusion, verdict-exit, wiremock, rust, tdd]
status: complete

requires:
  - phase: 23-guarded-apply-rollback
    provides: 23-05/23-17 run_apply and deploy_apply_prepare/execute; 23-07 preflights and DesignersPolicy; 23-09 gwbk and restart-pending fields; 23-10 HoldModel::MEASURED and HOLD_S (176 s); 23-11/23-12 rollback and journal-recovery events; 23-08/23-20 the Windows apply host behind apply_host_for_node; 23-02 gates, GateFlags and the Decision C rule
provides:
  - "ign deploy apply" (DeployCommand::Apply, DesignersArg), guarded at "// guarded:deploy apply", in GUARDED_OPS
  - actions::deploy_apply::deploy_apply_run, ApplySurfaces, ApplyRunOptions (the core entry that builds the host and takes HoldModel::MEASURED)
  - ActionOutput::DeployApply, render_deploy_apply_human, the verdict exit hook (rolled_back exits 6 after the envelope)
  - render::StderrApplyObserver and describe_apply_event (a progress line for every ApplyEvent, rollback and journal-recovery events included)
  - mcp::EXCLUDED_PATHS (full-path exclusion), path_is_excluded on both lists, guarded-and-excluded set {edit, deploy apply}
  - TUI route row and README Commands row for the leaf
  - mount_apply_gateway and seven apply_* CLI contract tests (plus one render unit test)
affects: [23-21, 23-14, 23-15, 23-16]

actuals:
  tokens: 13800   # chars/4 over the realized diff (55,133 characters of crates/ and README.md changes, 1,275 lines added, 7 removed)
  tasks: 2
  commits: 3      # two feat commits and one docs commit; the docs(23-13) completion commit follows

tech-stack:
  added: []
  patterns:
    - "a stateful wiremock gateway built from Respond closures over one Arc<Mutex<..>>, reading the REST view from the local data directory at request time, so a restore is seen by the next read"
    - "progress lines are human-mode stderr only; under --json stderr stays silent until an error envelope (precedent: project export, webdev deploy)"
    - "the staging notice rides the step that always precedes prepare (gwbk_downloaded or gwbk_skipped) because the engine emits no event before the upload"

key-files:
  created: []
  modified:
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/src/mcp.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-core/src/actions/deploy_apply.rs
    - crates/ignition-core/src/deploy/apply/mod.rs
    - README.md
    - crates/ignition-cli/tests/contract_deploy.rs

key-decisions:
  - "The verb names no write-half type: main.rs calls deploy_apply_prepare, the guarded confirmation, then deploy_apply_run; the core entry builds the host with apply_host_for_node and runs the engine with HoldModel::MEASURED and SystemClock. The grep for TreeMirror, mirror_for_node, apply_host_for_node and ApplyHost in main.rs prints 0."
  - "A no-op resolves no session and touches neither host nor gateway (DAPL-03): main.rs returns the noop result before named_profile_client. apply_noop_exit_0 asserts the mock gateway saw zero requests."
  - "Decision C is the first check in the arm (before load_declared): --plan-sha256 absent and (--json or --compact or stdin not a terminal or stdout not a terminal) is invalid_input naming Decision C, with no host or gateway contact."
  - "Progress is human mode only. Under --json or --compact the observer is NoObserver, so stderr is still one JSON value on an error."
  - "Scan wait for the production run: 500 ms interval, 2 s backoff ceiling, 120 s budget (the longest measured scan in 23-10 was under 1.6 s); a scan that does not complete rolls back as scan_incomplete."

patterns-established:
  - "EXCLUDED_PATHS beside EXCLUDED_LEAVES: a full-path list for a verb whose last word would be a bad segment rule (research Pitfall 5); 23-14 adds deploy rollback."
  - "A write verb is wired in two commits when the surface rows must land with the clap leaf: the leaf, the guard, the rows and the MCP exclusion first, the confirmed run second."

requirements-completed: []
requirements-advanced: [DAPL-01, DAPL-03, DAPL-07, DSURF-02]

duration: ~3h (the workspace test and clippy runs and the release install dominate)
completed: 2026-10-18
---

# Phase 23 Plan 13: The `ign deploy apply` verb Summary

**`ign deploy apply` is the first user-visible write verb: guarded (the refusal IS the plan), Decision C enforced before any contact, a no-op that sends nothing, a rolled-back verdict that exits 6 after its envelope, and absent from MCP through a full-path exclusion list.**

## What was built

### Final flag list

`ign deploy apply --env <ENV> [--repo DIR] [--base DIR] [--project NAME] [--plan-sha256 HASH] [--allow-destructive] [--allow-drift REASON] [--designers fail|wait|proceed] [--designers-timeout SECS] [--skip-gwbk]`, plus the global `--yes` and `--json`. `--repo` defaults to `.`, `--project` to `ign-cli` (as in `deploy verify`), `--designers` to `fail`, `--designers-timeout` to 600 (range 1..=86400). `apply_flags_are_exactly_these` pins the ten own long flags; there is no flag that skips the plan hash, the verify or the rollback.

### Behaviour

- **Guard.** Without `--yes`, `confirmation_required` (exit 2) with the plan prose from `deploy_apply_prepare` (env, gateway, mode, each changing entry with its flags, delete-set count, `plan_sha256`), at `// guarded:deploy apply`; `GUARDED_OPS` has the entry.
- **Decision C.** Missing `--plan-sha256` with `--json`, or no terminal on stdin or stdout, is `invalid_input` (exit 2) naming Decision C, before `load_declared`. A stale hash is the library's `plan_stale` (exit 6).
- **Gates.** `--allow-destructive`, `--allow-drift` and `--base` reach `GateFlags` unchanged; `--designers` and `--designers-timeout` reach `DesignersPolicy` (`DESIGNERS_POLL_S` 10 s interval kept). The gate-exit matrix through the binary is 23-21's.
- **Run.** After the confirmation the master session (and the backup node's, when the target has one, for `mode_backup`) resolve with `named_profile_client`; `deploy_apply_run` builds the host, takes the one measured hold (176 s, no override) and runs the engine.
- **Verdict exit.** `ActionOutput::DeployApply` with `outcome` `rolled_back` renders the success envelope (`data.ok` false, `data.slug` `deploy_rolled_back`, `data.cause`), then exits 6 through a hook beside the `deploy verify` one. `deploy_rolled_back` is in no exit table.
- **Human output.** `apply <gateway> (<env>, mode <mode>) via <transport>: <outcome>`, `run`, `run directory`, `N written, M deleted`, for a rollback the cause and that the snapshot was restored, `backup <name>` or its skip, `restart pending` lines, `plan_sha256`; warnings on stderr.
- **Progress.** Human mode only: `ign deploy apply: ...` lines on stderr for every engine event. The long silent step is `prepare` (up to about 9 minutes at 1,520 files, 23-10), so the observer announces `staging N file(s) on the host before the lock is taken: this step prints nothing until it ends and can take several minutes for a large tree` right after the backup step, which always precedes it.
- **MCP.** `EXCLUDED_PATHS = ["deploy apply"]`; `path_is_excluded` is true for a segment in `EXCLUDED_LEAVES` or the whole path in `EXCLUDED_PATHS`. `deploy render`, `plan`, `pull` and `verify` stay in the catalog. The registry-to-catalog test allows exactly `{edit, deploy apply}`; building the catalog twice gives the same tools.
- **Rows.** A `deploy apply` TUI route to `Screen::Projects` and a README Commands row, in the same commit as the clap leaf.

### The size limit, stated where users read it

The `deploy apply` help text and the README Commands row say: one fixed lock hold (176 s) serves every target; a change of **1,528 or more files** (writes plus deletes) is refused by the `hold_budget` preflight (`deploy_preflight_failed`, exit 6); staging a large tree is silent on the wire.

### `mount_apply_gateway` (23-21 reuses it)

```rust
// crates/ignition-cli/tests/contract_deploy.rs
async fn mount_apply_gateway(server: &wiremock::MockServer, data: &Path, log_items: Vec<Value>)
```

It mounts: scan-lock acquire/info on `/data/api/v1/scan-lock/config` (remaining 170, free again after a scan request), scan status/request on `/data/api/v1/scan/config` (inactive, active for one read after a trigger, inactive with an advanced timestamp), `scan/projects`, `gateway-info` (`name` `apply-gw`, mode `dev`), an empty `designers` list, a small valid `.gwbk` at `/data/api/v1/backup`, `restart-tasks/pending` `[]`, the REST list of `database-translator` and the singleton `general-alarm-settings` (read from `data` at request time), and logs (`log_items`, plus the one-line probe). Companions in the same file: `changed_tracer()` (local tracer with POSTGRES changed by a trailing space), `apply_tracer()` (adds `core/ignition/system-properties` with `systemName` `apply-gw`, a `.gitignore` holding `.ign/` and `git init`), `plan_hash(repo)`, `apply_args(repo, hash, json, yes)`, `seen_requests(server)`, `verify_ign(url)`. A red verify is `log_items` carrying a "will not be loaded" WARN for `ignition/database-translator/POSTGRES` with a timestamp after the scan (`1_792_999_999_000`).

## Tests

| Behaviour | Test |
|---|---|
| no `--yes`: exit 2 `confirmation_required`, message carries `plan_sha256`, the hash, the changed unit and the gateway; no `.ign/`; host unchanged | `apply_without_yes_prints_plan` |
| exactly the ten own flags; `--yes` global | `apply_flags_are_exactly_these` |
| applied: exit 0, one envelope on stdout, `.gwbk` in the run dir, second plan empty | `apply_applied_exit_0` |
| red verify: exit 6, verdict envelope with `deploy_rolled_back` and `verify_failed`, config tree equals its pre-apply listing, host record says `rolled_back`, no log text in output | `apply_rolled_back_exit_6` |
| empty plan: exit 0, `noop`, no `.ign/`, the gateway saw no request | `apply_noop_exit_0` |
| Decision C under `--json` and without it (no TTY): exit 2 `invalid_input`, zero requests, nothing written | `apply_requires_plan_hash_when_noninteractive` |
| human lines on stdout, `ign deploy apply:` progress and the staging notice on stderr, `scan-lock taken for 176 s` | `apply_human_output` |
| `deploy_apply` absent by full path, other deploy leaves present, catalog idempotent, `{edit, deploy apply}` | `mcp::tests::deploy_apply_is_excluded_by_full_path_not_by_segment` and the amended registry test |
| a progress line per event, the new 23-07/09/11/12 events included | `render::tests::apply_events_render_progress_lines` |
| marker and registry agree; README row; TUI row | `guarded_ops_registry_tracks_every_dispatch_site`, `every_deploy_leaf_has_a_readme_commands_row`, `every_deploy_leaf_has_a_projects_route_row`, `tui_coverage` |

## Verification

- `cargo test -p ignition-cli --test contract_deploy apply_`: 7 passed.
- `cargo test --workspace --no-fail-fast`: 2443 passed, 0 failed, 56 ignored (live and Windows-host gates); 2414 before this plan. The known load flake `live_gateway::guard_drop_during_unwind_inside_runtime_still_cleans_up` did not fail.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check`: clean.
- `grep -n '// guarded:deploy apply' crates/ignition-cli/src/main.rs`: one line. `grep -c 'TreeMirror\|mirror_for_node\|apply_host_for_node\|ApplyHost' crates/ignition-cli/src/main.rs`: 0.
- `cargo install --path crates/ignition-cli --force` was run and `ign deploy apply --help` read back (the help names the 1,528-file refusal).
- No live step: this plan has none. The trial was not touched and no reset was made.

## Deviations from Plan

### Plan departures

1. **[Plan structure] Task 1 shipped with the run not yet wired.** The plan has the confirmed run in Task 2. Task 1's commit (`fd0b0aa`) therefore returned an internal error after a confirmed `Ready` plan, and Task 2 (`1ce974f`) replaced it. The two commits are one release; no state between them was shipped. The interim code is gone (grep finds no `next commit` or `unreachable!` in the diff).
2. **[Rule 2 - Missing critical functionality] `OutcomeKind::as_str` added to core** (`deploy/apply/mod.rs`) so the human renderer prints `applied`, `noop` and `rolled_back` from the type, not by re-spelling the words.
3. **Progress lines are human mode only.** The plan's truth says "without `--json`"; the code matches that and also silences them for `--compact`. Under `--json` a long apply is silent on stderr until it ends (the project-export and webdev-deploy precedent, and the contract that a failing `--json` run leaves exactly one JSON value on stderr).
4. **The staging notice cannot come "before `prepare`" from an engine event.** The engine emits nothing between `gwbk_*` and `prepare` and no event was added (the events are pinned in `events.ndjson` and by tests). The observer remembers `run_started`'s file count and prints the notice on `gwbk_downloaded` or `gwbk_skipped`, which fires exactly once in every apply, directly before `prepare`.
5. **`--project` is optional (default `ign-cli`)**, as in `deploy verify`. The plan's truth lists it without brackets, but its own tests call `apply` without it.
6. **The rolled-back test compares the `config/` tree, not the whole data directory.** After a rollback the host keeps the run's record and snapshot below `ign-deploy/` (the record says `rolled_back`); that is the design, so the test asserts the config tree equals its pre-apply listing and that the record is `rolled_back`.
7. **TDD.** Both tasks are `tdd="true"`. Tests were written before the implementation in each task, but the RED state was not committed on its own: the tests and the code share a commit per task (`fd0b0aa`, `1ce974f`). There is no separate `test(...)` commit.
8. **Docs touch-up.** `f7d99bf` changes one help sentence to say the progress lines are human-mode stderr.

No auth gates. No architectural change (Rule 4 not triggered). No new error slug, so the four-place rule was not triggered.

## Carry-overs

### 23-21 (README section, verdict exception, gate-exit matrix)

- Reuse `mount_apply_gateway`, `apply_tracer`, `plan_hash`, `apply_args`, `seen_requests` (above). A stale hash: take `plan_hash`, change the live file again, run `apply --yes --plan-sha256 <old>` (exit 6 `plan_stale`). Drift needs `--base` from a `deploy render --out`; drift without `--allow-drift` and an un-allowed destructive entry are both `destructive_change_refused` (exit 2); a `core` shadow is `deploy_blocked` (exit 6) whatever the flags.
- The code comment in `main.rs` calls the apply hook the FOURTH sanctioned success-path exit exception; the README's dated section `### The \`deploy apply\` rolled-back verdict exception (<date>)` and `rolled_back_verdict_exception_is_dated_and_documented` are 23-21's.
- The README `### \`deploy apply\`` section should carry: the seven preflight check ids (23-07), the `gwbk` check, `type=all`, the 0600 file, `restart_pending_unavailable` and `snapshot_copy_differs` (23-09), the Windows notes (deploy account needs Modify on the mode collection and ownership of the files it moves; a collection path whose copy below the account home would pass 259 characters is refused by `write_host`; a `held` refusal or fingerprint surfaces as `transport_failed` naming the path, 23-08/23-20), and that a Windows `deploy apply` through `apply_host_for_node` is now reachable. The Commands row already states the 1,528-file refusal and the 176 s hold.
- Residual worth one README line (23-12): a recovery right after a dropped ssh connection should wait for the dead apply's hold (up to 176 s) to lapse.

### 23-14 (rollback verb)

- Add `"deploy rollback"` to `EXCLUDED_PATHS` and to the test constant `GUARDED_AND_EXCLUDED` in `mcp.rs`; add its route row.
- `StderrApplyObserver` hardcodes `APPLY_PROGRESS_PREFIX` (`ign deploy apply:`); a rollback needs `ign deploy rollback:`, so give the observer a prefix field. `describe_apply_event` already has a line for `restore_started` (cause optional), `live_changes_discarded`, `restored`, `cleared`, `interrupted_run` and `commit_interrupted`.
- `deploy_apply_run` is the model for a rollback entry: build the host inside core, no write-half type in `main.rs`. `RollbackPrepared.target` and `run_id` are there for the confirmation prose.

### 23-15 (canary, stdout purity)

- Progress is stderr and human only; in `--json` stderr is silent unless an error envelope. `LiveChangesDiscarded` and `InterruptedRun` lines carry paths and counts, the same class as the plan output. `apply_rolled_back_exit_6` already asserts the log canary is in neither stream; a `CANARY_APPLY` case can copy it.

### 23-16 (live)

- `ign` on PATH was reinstalled at `1ce974f` (release); `f7d99bf` only changes a help sentence. A live apply through the binary goes through `apply_host_for_node`, so the Windows arm is reachable; keep payloads small (a two-file apply is about 50 s end to end). `live_lock_lapse_restores` still needs the library entry with an explicit `HoldModel` (the verb has one hold, no override).

## Known Stubs

None. A grep for `TODO`, `FIXME`, `todo!`, `unimplemented!`, `#[ignore]`, `unreachable!` and "next commit" over the added lines finds nothing.

## Threat Flags

None beyond the plan's register. T-23-53 (agent applying over MCP): `deploy_apply_is_excluded_by_full_path_not_by_segment` and the amended registry test. T-23-54 (unreviewed plan unattended): `apply_requires_plan_hash_when_noninteractive`, `apply_without_yes_prints_plan`. T-23-55 (bypass flag added later): `apply_flags_are_exactly_these`. T-23-56 (stdout purity): the observer writes stderr only, silent under `--json`; 23-15 adds the purity cases. T-23-57 (a red apply exiting 0): `apply_rolled_back_exit_6`.

## Tracer feedback gate

Task 1 is the tracer. Auto mode is off and `human_verify_mode` is `end-of-phase`, and the orchestrator directed a sequential run, so execution continued after the tracer commit (`fd0b0aa`) instead of stopping for a human-verify checkpoint, as in 23-10 and 23-12. The tracer's `<verify>` (`apply_`, `mcp::tests`, the registry test, `deploy_family_coverage`, `tui_coverage`) passed before the commit; the tracer re-ran green with Task 2's work in place. For the end-of-phase list: the refusal that prints the plan from the binary, and the confirmed apply against a stateful mock gateway.

## Requirements

`requirements-completed` is empty by instruction. Advanced: DAPL-01 (guard and plan binding), DAPL-03 (noop), DAPL-07 (rolled-back verdict exit 6, through the binary), DSURF-02 (MCP exclusion). DAPL-02's gate-exit matrix through the binary, the README section and the dated exception are 23-21.

## Self-Check: PASSED

- Files exist: `crates/ignition-core/src/actions/deploy_apply.rs`, `crates/ignition-cli/src/{cli,main,render,mcp}.rs`, `crates/ignition-tui/src/routes.rs`, `crates/ignition-cli/tests/contract_deploy.rs`, `README.md`, this SUMMARY.
- Commits `fd0b0aa`, `1ce974f`, `f7d99bf` exist in `git log`.
