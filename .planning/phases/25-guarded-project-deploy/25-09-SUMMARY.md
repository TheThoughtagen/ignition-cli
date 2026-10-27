---
phase: 25-guarded-project-deploy
plan: 09
subsystem: deploy
tags: [deploy-projects, release-order, semver, release-record, first-deploy, downgrade, git-handover, check]

requires:
  - phase: 25-guarded-project-deploy
    provides: "25-07 ProjectHost::read_release_record and release_record_rel; 25-08 ssh read side; 25-03 contract decisions V3, V4, V5, K13, K14"
provides:
  - "Release::parse / Release::order_against / ReleaseOrder: semver after one leading v, precedence ordering, build metadata ignored"
  - "ReleaseRecord (strict parse, to_bytes, release()): schema 1, unknown fields refused, size cap, gateway/project/hash/run id/release validated; any anomaly is deploy_run_invalid naming the location"
  - "ProjectAction::Record and the K14 decision (record / skip / create / import)"
  - "destructive_change_refused gates downgrade and first_deploy with a flag and a hint each"
  - "--allow-downgrade and --allow-first-change <REASON> on deploy projects"
  - "result keys previous_release, recorded_sha256, live_matches_last_deploy, allow_downgrade, allow_first_change; K13 drift warning"
  - "Target::validate refuses mode projects"
  - "README: Release order and the first deploy paragraph; Commands row"
affects: [25-10, 25-11, 25-13, 25-17, 25-19]

actuals:
  tokens: 21500   # chars/4 over `git diff cdf900e..HEAD -- crates README.md` (85899 chars)
  tasks: 2
  commits: 3

tech-stack:
  added: []
  patterns:
    - "a record read from a host is parsed strictly and a failure is never read as absent; the same Ledger::parse rules apply (size cap first, deny_unknown_fields, foreign gateway/project refused, problem text names fields only)"
    - "an override flag is reported in the result (allow_downgrade, allow_first_change) so it is auditable"
    - "the check applies the same refusals as the apply will, with the same flags (V10)"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/deploy/projects.rs
    - crates/ignition-core/src/deploy/target.rs
    - crates/ignition-core/src/actions/deploy_projects.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/contract_deploy_projects.rs
    - crates/ignition-core/tests/deploy_projects.rs
    - README.md

key-decisions:
  - "The downgrade gate runs before any gateway read (right after the .git probe and the record read); the first_deploy gate needs the live digest, so it runs after the gateway reads. The user accepted the downgrade placement at the tracer gate."
  - "Same release means equal semver version including build metadata, so 1.0.0 and v1.0.0 are the same release and skip, while v1.0.0+a and v1.0.0+b are different releases and record."
  - "The --release refusal text is literal and never echoes the input."
  - "No new CoreError slug: the gates ride destructive_change_refused and a bad record rides deploy_run_invalid, so no README exit row was added."
  - "Mode projects gets its own message ('names a directory this tool reserves for project release records') because it is not a gateway collection; the test still asserts the word reserves."

patterns-established:
  - "Human lines that print a value derived from the host, the repo or the user (release, previous release, first-change reason, warnings) go through render.rs escape_control"

requirements-completed: []  # DPRJ-02 and DPRJ-05 stay open: the apply, parent order (25-10), verify and live gates (25-11 to 25-19) remain, as 25-07 and 25-08 left DPRJ-04

duration: "tracer by the first executor, then a continuation of about 40 minutes"
completed: 2026-10-23
status: complete
---

# Phase 25 Plan 09: Release order and the git-module handover Summary

**`deploy projects --check` now orders releases by semver, refuses an older release than the one the gateway last received (`downgrade`) and the first `ign` deploy of a project that would change what the gateway runs (`first_deploy`) unless the reported override flags are given, and plans a record-only adoption when the live project already equals the repo.**

## Accomplishments

- **Task 1, tracer (`eb89774`, accepted at the human-verify gate).** `Release::parse` and `order_against`, strict `ReleaseRecord::parse` and `to_bytes`, the `ProjectAction::Record` variant, the `--allow-downgrade` and `--allow-first-change <REASON>` flags, the downgrade gate placed before any gateway read, per-gate refusal text and hints in `error.rs`.
- **Task 2 (`926eaaf`).**
  - `first_deploy` gate: no record, a live project, and a digest that differs from the artifact refuses `destructive_change_refused` gate `first_deploy` unless `--allow-first-change` is given; the reason is reported in `data.allow_first_change`. A project absent from the gateway is `create` and needs no reason (D-13).
  - K14: live equals artifact gives `record` when the record is absent or names another release (`would_change` true) and `skip` when it names this release (`would_change` false).
  - K13: `recorded_sha256` and `live_matches_last_deploy` are reported; a live project that no longer matches the recorded digest adds one warning, "project <name>: the live project no longer matches the last release this tool deployed (<release>)", and does not refuse.
  - `Target::validate` refuses mode `projects`.
  - Human output: `release: <new> (last deployed by this tool: <previous|none>)`, `check: record the release only (no project change)`, `first change allowed: <reason>`, `downgrade allowed (--allow-downgrade)` and every warning, all through `escape_control`.
  - README: the `**Release order and the first deploy.**` paragraph, the Commands row synopsis and action list, the data key lists.

## Test results

- `cargo test -p ignition-core --tests` (lib 1091 passed, 0 failed; all integration targets passed, live gates ignored).
- `cargo test -p ignition-cli` all targets passed, including `contract_deploy_projects` (35 passed) and `deploy_family_coverage`.
- New engine tests in `crates/ignition-core/tests/deploy_projects.rs`: `first_deploy_change_refuses`, `recorded_project_imports_without_the_first_change_reason`, `first_deploy_equal_records`, `same_release_skips`, `new_release_same_content_records`, `create_needs_no_first_change_flag`, `live_drift_warns`, `live_matching_the_record_is_silent`; `check_never_creates_ign_dir` now also covers `record` and asserts the host gained nothing.
- New CLI tests: `projects_check_reports_record`, `projects_check_first_deploy_change_refuses`, `projects_check_live_drift_warns`, `projects_check_human_record_and_drift_lines`; new render tests `release_line_names_the_previous_release_or_none`, `record_action_says_only_the_record_is_written`, `first_change_reason_and_warnings_print_escaped`.
- `reserved_collection_modes_are_refused` covers `core`, `local` and `projects`.
- `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --all --check` clean.
- Acceptance greps: `grep -c 'allow-first-change' README.md` prints 3; `grep -c '"downgrade"' crates/ignition-core/src/error.rs` prints 4.
- Built in a private target dir under the session scratchpad; no background work left running.

## Deviations from Plan

### Existing tests changed deliberately (intent kept)

With no release record, an unchanged project now plans `record` and a differing one needs `--allow-first-change`, so these tests changed meaning:

- `projects_check_reports_import`, `projects_check_reports_removed_members`, `projects_check_creates_no_ign_dir`, `projects_check_human_lines`: pass `--allow-first-change` so the import (counts, removals, no `.ign/`, human lines) is still what is tested; `reports_import` also asserts the reason is reported and `previous_release` is null.
- `projects_check_reports_skip`: the first check now reports `record`; the test then writes a record naming `v1.0.0` and the digest the check reported and asserts `skip`, `recorded_sha256` and `live_matches_last_deploy` true. This keeps the "unchanged is a skip" intent and exercises the record round trip.
- `projects_check_without_git_proceeds` and `projects_check_targets_default_to_the_repo`: the action they reach is now `record` (equal content, no record).
- Engine: `check_skip_when_equal` records the matching release first; `check_import_counts` records a release so the member counts stay the subject; `git_absent_proceeds` expects `record`; `check_never_creates_ign_dir` runs create, skip, record and import.

### Process

- **Tests first.** Task 1: the contract file was red on the missing `ReleaseRecord` before implementation. Task 2: the engine tests were written before the action change. The `projects` mode test change and its implementation were made in one step, so that red was not observed separately.
- **Downgrade gate before any gateway read** (Task 1, accepted by the user at the tracer gate): the plan places it in prepare after the `.git` probe; it sits there, ahead of `projects/list`.
- **`--release` refusal text is literal** and never echoes the input.
- **No new CoreError slug**, so no README exit-table row was added.
- **Same release definition** (full semver equality, build metadata included) is mine; the plan says "names the same release" without defining it.
- **`projects` mode message** differs from `core` and `local` because it is not a gateway collection; the plan asked for "the same message shape".
- **`requirements mark-complete` not run:** DPRJ-02 (parent order, 25-10, 25-15, 25-19) and DPRJ-05 (apply-side, 25-11, 25-12, 25-17, 25-18, 25-19) have later plans, as 25-07 and 25-08 left DPRJ-04.
- **Estimate:** the plan estimated 115k tokens on a low-confidence scale; the realized diff is about 21.5k on the chars/4 scale.

## Open points

- The apply must take the same two gates with the same flags; 25-11 owns that, and 25-11 records `allow_downgrade` and the `allow_first_change` reason in `run.json` (T-25-29 is fully closed there).
- The Commands row and the new paragraph say "a deploy will" write the record; nothing writes it yet (25-11, 25-13).

## Known Stubs

None.

## Threat Flags

None beyond the register. T-25-28 mitigated (`ReleaseRecord::parse` strict, `release_record_parse` tests, `projects_check_corrupt_record_refuses`). T-25-29 mitigated for the check (flags reported in `data`); the `run.json` half lands in 25-11. T-25-30 mitigated (`reserved_collection_modes_are_refused`).

## Self-Check: PASSED

- Files exist: `crates/ignition-core/src/deploy/projects.rs` (contains `pub struct ReleaseRecord`), `crates/ignition-core/src/deploy/target.rs` (contains `"projects"`), `crates/ignition-core/src/actions/deploy_projects.rs` (contains `"first_deploy"` and `read_release_record(`), `README.md`.
- Commits found in `git log`: eb89774, 926eaaf.
