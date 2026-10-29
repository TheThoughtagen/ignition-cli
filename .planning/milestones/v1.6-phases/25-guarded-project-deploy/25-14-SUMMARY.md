---
phase: 25-guarded-project-deploy
plan: 14
subsystem: deploy
tags: [deploy-projects, preflight, designers, wrong-target, shared-gate]

requires:
  - phase: 25-guarded-project-deploy
    provides: "25-11 deploy_projects_run, ProjectsRunOptions, ProjectGateway, FakeProjects; 23 preflight vocabulary (DesignersPolicy, DesignersMode, read_designers, host_system_name)"
provides:
  - "designers_gate and DesignersVerdict in preflight.rs: the one read-poll-decide loop, used by deploy apply (designers_finding) and deploy projects"
  - "wrong_target_problems, wrong_target_failure and wrong_target_gate in preflight.rs: the one name comparison, used by deploy apply (wrong_target_finding) and deploy projects"
  - "ProjectGateway::designers and ProjectGateway::gateway_name; ProjectsRunOptions.designers"
  - "deploy projects --designers fail|wait|proceed and --designers-timeout SECS, with the apply vocabulary"
  - "A confirmed create/import refuses deploy_preflight_failed check designers while a Designer session is open; --designers proceed records the sessions (names only) in run.json and events.ndjson"
  - "A confirmed run of any action, record included, refuses deploy_preflight_failed check wrong_target unless the REST name equals the host systemName and nodes.master.gateway_name when declared"
  - "A refusal writes nothing: no run directory, no pre-image, no import, no record"
affects: [25-15, 25-17, 25-19]

actuals:
  tokens: 18355   # chars/4 over `git diff 54a2522~1..HEAD -- crates README.md` (73420 chars)
  tasks: 2
  commits: 4

tech-stack:
  added: []
  patterns:
    - "one shared preflight implementation, two callers (apply and projects), pinned by the unchanged apply tests"
    - "pre-checks read and write nothing, and run before RunDir::create_fresh"
    - "preflight problem text names no value the gateway or the host reported"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/deploy/apply/preflight.rs
    - crates/ignition-core/src/actions/deploy_projects.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-core/tests/common/fake_projects.rs
    - crates/ignition-core/tests/deploy_projects.rs
    - crates/ignition-cli/tests/contract_deploy_projects.rs
    - README.md

key-decisions:
  - "The Designer refusal covers any open session, not only sessions on this project (K12 a, assumption A8 kept)."
  - "A skipped Designer check for a record-only plan is recorded in run.json and events.ndjson only; the result carries no data.designers key (accepted at the tracer gate)."
  - "The wrong_target refusal text carries no name the gateway or the host reported; it says which comparison failed."
  - "FakeProjects reports the gateway name capgw by default, the gateway: of the engine suite's target file, so ordinary runs are aimed at the gateway they name."

patterns-established:
  - "wrong_target_gate(rest, reader, mode, declared) for a caller without a PreflightInput"

requirements-completed: []  # DPRJ-03 stays open: see Requirement status

duration: "about 1 h of commits (10:43 to 11:34 MDT) plus the planning reads before the first commit and the tracer gate wait"
completed: 2026-10-23
status: complete
---

# Phase 25 Plan 14: Designer and wrong-target pre-checks Summary

**`deploy projects` now carries the `deploy apply` pre-checks from one shared implementation: a confirmed create or import refuses while any Designer session is open (`--designers fail|wait|proceed`, `--designers-timeout`, sessions recorded when proceeding), and every confirmed run, `record` included, refuses `wrong_target` unless the gateway's REST name equals the host `systemName` and the declared `gateway_name`, all before any run directory, pre-image, import or record exists.**

## Accomplishments

- **Task 1, tracer (`54a2522` RED, `8ee2da1` GREEN, accepted at the human-verify gate: "Accept, continue (Recommended)").**
  - `designers_gate` is the only read-poll-decide loop; `designers_finding` is rebuilt on it with the same events and findings, and the `deploy_apply` designers tests are unchanged and green.
  - `ProjectGateway::designers()`, `ProjectsRunOptions.designers`, the check before `RunDir::create_fresh`, `--designers`/`--designers-timeout` on `DeployCommand::Projects`, `run.json` and event recording (`designers_poll`, `designers_proceed`, `designers_skipped`), and the `FakeProjects` designers knobs.
- **Task 2 (`70c2d3b`, `d0f8d3e`).**
  - `wrong_target_problems(rest, host, declared)` is the extracted comparison; `wrong_target_gate` adds the host read so `deploy projects` can call it without a `PreflightInput`; `wrong_target_finding` is rebuilt on it with unchanged behaviour.
  - `ProjectGateway::gateway_name()` (`gateway-info` name, blank is `None`); `deploy_projects_run` calls `wrong_target_check` for every plan, before the Designer check and before the run directory, reading the host `systemName` through `for_node(targets_root, target_file, "nodes.master", master)`.
  - Engine cases (all in `tests/deploy_projects.rs`): `wrong_target_name_mismatch_refuses`, `wrong_target_host_name_mismatch_refuses`, `wrong_target_applies_to_record_only`, `designers_wait_clears`, `designers_wait_times_out`, `preflight_read_error_is_not_preflight_failed`, plus `wrong_target_gateway_without_a_name_refuses`, `wrong_target_all_three_names_agree_proceeds` and `record_only_skips_the_designers_check`. One CLI contract case, `projects_apply_wrong_target_refuses`.
  - Existing engine and contract tests keep their intent: the `checkout()` and `fixture()` helpers now write the host `ignition/system-properties` file (`systemName` `capgw`) under the fixture data dir, `FakeProjects` reports `capgw`, and `mount_deployable` mounts a `gateway-info` answer `capgw`.
  - README: a `**Pre-checks.**` paragraph in `### deploy projects` and `--designers`/`--designers-timeout` in the Commands row and the section synopsis.

## Verification

Build environment: `CARGO_TARGET_DIR` in the session scratchpad.

- `cargo test -p ignition-core`: 2088 passed, 0 failed, 55 ignored (lib 1109, `deploy_apply` 134, `deploy_projects` 43, the `deploy_transport` source scans, the rest).
- `cargo test -p ignition-cli`: 492 passed, 0 failed, 22 ignored. This includes the registry tests under `--bin ign`, `contract_deploy_projects` (51 passed) and `deploy_family_coverage`.
- `cargo test -p ignition-tui`: 281 passed, 0 failed.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean. `cargo fmt --all --check`: clean.
- Acceptance: `grep -c 'designers_gate('` prints 3 (`preflight.rs`) and 1 (`deploy_projects.rs`); `grep -c 'designers-timeout' README.md` prints 10.
- Mutation check: with the `wrong_target_check` call commented out, four engine cases failed (`wrong_target_name_mismatch_refuses`, `wrong_target_host_name_mismatch_refuses`, `wrong_target_gateway_without_a_name_refuses`, `wrong_target_applies_to_record_only`); the call was restored.
- No live host was contacted. No background work is left running.

## Deviations from Plan

1. **[Rule 1 - Bug] `54a2522` alone fails `cargo fmt --check`.** The RED commit's test code was not formatted; the formatting was fixed in `8ee2da1`. Anyone bisecting should expect red on that one commit (its tests are meant to fail anyway).
2. **[Rule 3 - Blocking] `on_poll` needs `+ Send`.** `designers_gate`'s `on_poll: &mut dyn FnMut(usize) -> Result<(), CoreError>` had to carry `+ Send` so the future of the `mcp::spawn` call path stays `Send`. Commit `8ee2da1`.
3. **The skip note is in `run.json` and the events, not in `data`.** The plan says the result notes `designers: skipped (record only)`. The skip is recorded as `designers_skipped` in `events.ndjson` and `designers: skipped` in `run.json`, with no `data.designers` key (the V7 `data` key set is left as decided). The user accepted this at the tracer gate. Commit `8ee2da1`.
4. **[Rule 2 - Missing critical] The wrong-target comparison got a host-read wrapper.** The plan names only `wrong_target_problems`; `deploy projects` has no `PreflightInput`, so `wrong_target_gate` (host read plus comparison) and `wrong_target_failure` were added to keep one implementation. Commit `70c2d3b`.
5. **The gateway name is not in the refusal text.** A wrong-target refusal says which comparison failed and never repeats a name the gateway or the host reported (the message is the same as for `deploy apply`, which already named none). The human renderer therefore had no gateway-derived name to escape; `render.rs` escaping was not needed for this check. The new CLI test pins that the name is absent.
6. **`FakeProjects` defaults to a gateway name.** `gateway_name` answers `capgw` unless a test calls `with_gateway_name` or `with_no_gateway_name`. Without a default every existing engine test would have needed the call.
7. **Tests for Task 2 were written after the wiring, not strictly first.** The mutation check above showed they fail when the check is removed. (During the full run a stale mutated build briefly made four cases fail after the restore, because the restored file kept an older mtime; touching the file rebuilt it and all 43 passed. No code changed.)
8. **`designers_wait_times_out` runs for the real 1 s timeout** with a 1 ms interval, as the plan says, so it costs about 1 s.
9. **Extra cases beyond the six listed:** `wrong_target_gateway_without_a_name_refuses`, `wrong_target_all_three_names_agree_proceeds`, `record_only_skips_the_designers_check` and the CLI `projects_apply_wrong_target_refuses`.

No auth gates, package installs or checkpoints beyond the tracer gate.

## Requirement status

`requirements-completed` is empty on purpose and DPRJ-03 was not marked complete in REQUIREMENTS.md. DPRJ-03 is "each project is exported before it is overwritten and verified after import by canonical hash". This plan adds protections around the overwrite (no import under an open Designer session, no write to the wrong gateway) but does not finish the requirement. `requirements.ready-ids` for this plan returned `0/1 requirement(s) ready`, because three later plans in this phase still declare DPRJ-03 and have no SUMMARY:

- **25-15** (`[DPRJ-02, DPRJ-03]`): the children sweep.
- **25-17** (`[DPRJ-03, DPRJ-04, DPRJ-05]`): the secret canary and stdout purity for the verb.
- **25-19** (`[DPRJ-01 ... DPRJ-05]`): the live proof of export-before and verify-after on a real 8.3.6 rig.

A requirement is marked by the plan that finishes it; the last of those, 25-19, will.

## Known Stubs

None.

## Threat Flags

None beyond the plan's register. T-25-44 (wrong gateway) is mitigated by `wrong_target` comparing the REST name, the host `systemName` and the declared `gateway_name` before any write, `record` included; T-25-45 (import under an open Designer session) by `designers_gate` with `fail` as the default; T-25-46 (proceeding over sessions) by listing them by name in `run.json` and the events.

## Carried notes

- README still says "a node declared with `ssh` cannot take the record yet" in the `**Deploying.**` section; 25-13 made the ssh writer real. Not changed here (outside this plan's files).

## Self-Check: PASSED

- Files exist: `crates/ignition-core/src/deploy/apply/preflight.rs`, `crates/ignition-core/src/actions/deploy_projects.rs`, `crates/ignition-core/tests/common/fake_projects.rs`, `crates/ignition-core/tests/deploy_projects.rs`, `crates/ignition-cli/tests/contract_deploy_projects.rs`, `README.md`: found.
- Commits exist: `54a2522`, `8ee2da1`, `70c2d3b`, `d0f8d3e`: found in `git log`.
- No `todo!`, `unimplemented!`, `#[ignore]` or skipped test added.
- `.planning/config.json` and `.planning/research/.cache/` were not staged.
