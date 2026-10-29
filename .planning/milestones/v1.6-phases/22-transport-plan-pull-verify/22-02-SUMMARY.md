---
phase: 22-transport-plan-pull-verify
plan: 02
subsystem: deploy
tags: [rust, clap, mcp, readme, plan-verb, envelope]

requires:
  - phase: 22-transport-plan-pull-verify
    provides: load_declared, transport::for_node, deploy_plan, DeployPlanResult, PlanFixture (22-01)
provides:
  - "ign deploy plan --env <ENV> [--repo DIR] verb (DeployCommand::Plan, ActionOutput::DeployPlan)"
  - "render_deploy_plan_human, Screen::Projects route row, unguarded MCP tool deploy_plan"
  - "README Commands row, ### deploy plan section with the canonical plan_sha256 text, nodes.*.local target row"
  - "REQUIREMENTS DPLN-02 note for the unmanaged status (D-06)"
affects: [22-03, 22-04, 22-06, 22-08, 22-11, 23]

actuals:
  tokens: 14500
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "Offline-style dispatch arm: load_declared then transport::for_node then deploy_plan, no Session::resolve; the envelope profile only labels the master node"
    - "CORE-01 human profile header is suppressed for verbs that resolve no session (deploy plan)"

key-files:
  created: []
  modified:
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/src/mcp.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-core/src/deploy/plan.rs
    - crates/ignition-cli/tests/contract_deploy.rs
    - crates/ignition-cli/tests/contract_stdout_purity.rs
    - README.md
    - .planning/REQUIREMENTS.md

key-decisions:
  - "The human CORE-01 header [profile: NAME] is not printed for deploy plan: no session resolves, and the documented first human line is the plan summary line"
  - "PlanStatus::word and CoreRelation::word became pub so the CLI renderer prints the same words the hash and JSON use"
  - "DPLN-02 and DPLN-03 stay unchecked: the removed status and the destructive/lockout flags arrive with 22-03"

patterns-established:
  - "contract_deploy helpers: tracer_with_master(master_extra) writes targets/dev.yaml; local_tracer() returns (scratch, repo, data_dir) with a live mode collection equal to the ACTUAL render (built via ign deploy render --out, render.json dropped); first_live_file(data); plan_json(repo, cmd). 22-03 reuses these."

requirements-completed: []
requirements-advanced: [DPLN-02, DPLN-03]

duration: 25min
completed: 2026-10-14
status: complete
---

# Phase 22 Plan 02: `ign deploy plan` verb surface Summary

**`ign deploy plan` wired onto the 22-01 engine: exit 0 always, no session and no secret, `transport_failed` (exit 4) on unreadable hosts, a byte-pure `--json` envelope, an unguarded MCP tool, and a README section that publishes the canonical `plan_sha256` text.**

## Accomplishments

- `DeployCommand::Plan { env, repo }`, `ActionOutput::DeployPlan`, and a dispatch arm that runs `load_declared`, `transport::for_node(.., "nodes.master", ..)` and `deploy_plan`. It never calls `Session::resolve`; the envelope `profile` is `nodes.master.profile`.
- Master with neither `ssh` nor `local` exits 2 `deploy_tree_invalid` naming `nodes.master`; a missing `data_dir` exits 4 `transport_failed` naming `nodes.master`; stdout stays empty on refusals.
- Human output: summary line, one line per non-`same` entry (`<status> <key> core=<core> live_in=<a,b>`), `plan_sha256 <hex>` last; warnings to stderr.
- Surface: route row `deploy plan` on `Screen::Projects`, README Commands row, MCP tool `deploy_plan` (unguarded, no `confirm`), `catalog_names_match_the_clap_tree_bidirectionally` and the family coverage tests stay green.
- README `### deploy plan` section (effective-view basis and `live_in`, five statuses, `core` relations, exit 0 always, zero writes, `transport_failed`, the `plan_sha256` text byte-exactly, the `local/` warning) and a `nodes.*.local` target-table row. REQUIREMENTS DPLN-02 carries the D-06 `unmanaged` note.

## Task Commits

1. **Task 1: verb, dispatch, envelope, human output, route row, Commands row** - `64b2e80` (feat; 6 tests RED first, then green)
2. **Task 2: MCP and stdout contracts, README section, target row, REQUIREMENTS note** - `9424b52` (docs)

## Verification

- `cargo test -p ignition-cli --test contract_deploy` 17 passed (6 new, `RENDER_GOLDEN` unchanged); `deploy_family_coverage` 4, `tui_coverage` 5, `contract_stdout_purity` 10, `deploy_plan_is_an_unguarded_catalog_tool` and `catalog_names_match_the_clap_tree_bidirectionally` pass.
- `cargo test --workspace` has no failures; `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` clean.
- README anchors: `### \`deploy plan\`` (1), `ign-deploy:plan:v1` (1), `nodes.*.local` row at the target table.

## Deviations from Plan

**1. [Rule 1 - Bug/spec conflict] Suppressed the CORE-01 `[profile: NAME]` header for `deploy plan`**
- **Found during:** Task 1, `plan_cli_human_output_lines`
- **Issue:** the generic human renderer prints `[profile: dev]` first whenever the envelope profile is set, contradicting the plan's "first line is `plan ...`", and implying a profile resolved when none did.
- **Fix:** `render_human` skips the header for `ActionOutput::DeployPlan`. JSON `profile` is unchanged. The error-path header (stderr) is untouched.
- **Files modified:** `crates/ignition-cli/src/render.rs`
- **Commit:** `64b2e80`

**2. [Rule 3 - Blocking] `PlanStatus::word` / `CoreRelation::word` made `pub`** so the CLI renderer prints the exact words used in JSON and the hash. Commit `64b2e80`.

**Test note:** the data-key test compares key sets, not order, because `serde_json` maps sort keys; the on-wire order is the 22-01 struct order only when `preserve_order` is on, and the README states the fields without promising JSON object order.

**Requirement bookkeeping:** `requirements.mark-complete` was not run. DPLN-02 lists the `removed` status and DPLN-03 the destructive/lockout flags and `shadows_removal`, which arrive with the 22-03 delete set; marking them now would overclaim.

## Known Stubs

None added. The 22-01 stubs (ssh arm of `for_node`, `removed` / `shadows_removal` producers) remain until 22-06 and 22-03. The README documents `removed` and `shadows_removal` as arriving with 22-03.

## Threat Flags

None. T-22-05 (read-only verb, no write path), T-22-06 (profile label authenticates nothing; `plan_cli_needs_no_secret`) and T-22-63 (paths and hashes only; `deploy_plan_json_stdout_is_the_envelope_only`) are covered by tests.

## Self-Check: PASSED

- Commits `64b2e80` and `9424b52` present in `git log`.
- Modified files present; README and REQUIREMENTS greps return the expected counts.
