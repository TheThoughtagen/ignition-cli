---
phase: 25-guarded-project-deploy
plan: 11
subsystem: deploy
tags: [deploy-projects, apply, import, hash-gate, release-record, project_hash_mismatch, write-confinement]

requires:
  - phase: 25-guarded-project-deploy
    provides: "25-04 check and ProjectGateway; 25-06 artifact and canonical digest; 25-07 project host and release record path; 25-09 release gates; 25-10 parent check; 25-03 contract decisions V1, V2, V5, V6, K11, K14, K15"
provides:
  - "ign deploy projects without --check: Decision C and --artifact-sha256 before any contact, the confirmation guard, a run directory, the 0600 pre-image export, the import (create for an absent project, D-13), the canonical-hash verify and the release record"
  - "ProjectsPrepared, deploy_projects_run, ProjectsRunOptions, ProjectsEvent, ProjectsObserver, NoProjectsObserver, ProjectGateway::project_import"
  - "ReleaseRecordWriter and release_writer_for_node (local arm; the ssh arm refuses until 25-13)"
  - "CoreError::ProjectHashMismatch / project_hash_mismatch (exit 6), DEPLOY_SLUGS 15, README exit-6 row"
  - "imported_unverified fail-safe: ok false, no record, no second import, no further gateway call"
  - "ForbidProjectWrites, FailingRecordWriter and the import semantics of FakeProjects"
  - "write_half_unreachable forbids the writer in the read-only sources; release_writer_is_confined"
affects: [25-12, 25-13, 25-14, 25-15, 25-16, 25-17, 25-18, 25-19]

actuals:
  tokens: 38802   # chars/4 over `git diff 5146a44..HEAD -- crates README.md` (155211 chars)
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "the hash gate runs inside load_project, so a mismatch refuses before any host or gateway contact"
    - "after the gateway has changed, nothing returns an error: verify, record and report only, and every failure is a warning"
    - "the record writer is built inside the action from the master node, never in the CLI, and a source scan keeps it that way"
    - "result.json carries the slug only on an error return, never the message text, a hash or file content"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/actions/deploy_projects.rs
    - crates/ignition-core/src/deploy/transport/projects.rs
    - crates/ignition-core/src/deploy/transport/local/project_ops.rs
    - crates/ignition-core/src/deploy/apply/run_store.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/mcp.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/contract_deploy_projects.rs
    - crates/ignition-core/tests/common/fake_projects.rs
    - crates/ignition-core/tests/deploy_projects.rs
    - crates/ignition-core/tests/deploy_transport.rs
    - README.md

key-decisions:
  - "A failed read-back after the import is imported_unverified with a warning, not an error: the gateway has already changed, so the run reports and stops."
  - "A record-only plan re-checks the live digest before writing the record, because the record claims the live project is the artifact."
  - "The record-writer seam (ProjectsRunOptions.record_writer, #[doc(hidden)], None from the CLI) exists so the engine can prove a failing record write is a warning."

patterns-established:
  - "A confinement scan has a negative half (forbidden in read-only sources) and a positive half (the walk must find the allowed places), so a path typo cannot make it vacuous"

requirements-completed: []  # DPRJ-01, 03, 04 and 05 stay open: see Requirement status

duration: "tracer by the first executor, then a continuation of about 40 minutes"
completed: 2026-10-23
status: complete
---

# Phase 25 Plan 11: Guarded apply Summary

**`ign deploy projects` now deploys one project from its repo: the artifact hash the reviewer saw is checked before any contact, the live project is saved to a 0600 file, imported over (or created when absent), read back and compared by canonical hash, and the release recorded on a local node; a failed verify records nothing and sends no second import.**

## Accomplishments

- **Task 1, tracer (`4814da9`, accepted at the human-verify gate).**
  - Decision C: without `--check`, a missing `--artifact-sha256` with `--json`, `--compact` or a non-terminal stdin or stdout is `invalid_input` (exit 2) before any contact; a value that is not 64 lowercase hex is `invalid_input`.
  - The hash gate sits in `load_project`, so a differing hash is `project_hash_mismatch` (exit 6, project and two hashes only) after the local build and before any host or gateway request.
  - `// guarded:deploy projects` with `require_confirmation`, the plan as the prose; `deploy projects` is in `GUARDED_OPS` and `GUARDED_AND_EXCLUDED`; an unchanged project returns before any session write.
  - `deploy_projects_run`: fresh run directory (`run.json`, `plan.json`, `events.ndjson`, `result.json`), pre-image export to `export/<project>.zip` before the import, `plan_stale` when the live project moved (K15), import (`overwrite=false` for an absent project, D-13), read-back compare, release record through `ReleaseRecordWriter` (temp then rename), the K6 read-back parent warning, `imported_unverified`.
  - README: the exit-6 row lists `project_hash_mismatch`; a `Deploying` section; render changes.
- **Task 2 (`b692182`).** The engine cases and the two source scans below.
  - `export_before_import_and_verify`: after the plan, the call log is exactly `export`, `import:true`, `export`; the pre-image's digest equals the plan's `live_sha256`; `run.json`, `plan.json`, `events.ndjson` and `result.json` exist; the events are in order; the record names the artifact, the release and the run id.
  - `create_has_no_pre_image`: calls are `import:false`, `export`; no `export/` directory; outcome `created`.
  - `live_changed_since_plan_is_stale`: `plan_stale` with detail `live project changed since the plan`, no import call, the other administrator's edit survives, the earlier record is not replaced, `result.json` holds the slug and `ok: false`.
  - `empty_changes_warns`: the one warning `the gateway reported no changes for this import`, outcome `imported` (C8c recorded `changes: []` answers, so the case applies).
  - `check_is_read_only`: a full check of create, import, record and skip plans against `ForbidProjectWrites` completes; no `.ign/`, no record written or replaced.
  - `other_projects_untouched`: for import, create, record-only and stale runs, every other project is byte-identical, and the only import is the verb's own project with the right `overwrite`.
  - `record_write_failure_is_warning`: with `FailingRecordWriter`, outcome `imported`, `ok` true, `record_written` false, one warning carrying `transport_failed` and none of the error's text.
  - `write_half_unreachable` forbids `ReleaseRecordWriter` and `release_writer_for_node` in `actions/deploy.rs`, `main.rs` and `mcp.rs`; `release_writer_is_confined` proves only `deploy/transport/` and `actions/deploy_projects.rs` name them and that the walk finds both.

## Test results

- `cargo test --workspace`: 2836 passed, 0 failed, 75 ignored (the live gates), exit 0.
- `cargo test --workspace --no-default-features --no-run`: builds, exit 0.
- `cargo test -p ignition-core --test deploy_projects`: 34 passed (27 from earlier plans, 7 new). `--test deploy_transport write_half_unreachable`: passed and runs `release_writer_is_confined`.
- `cargo test -p ignition-cli --bin ign guarded_ops`: `guarded_ops_registry_tracks_every_dispatch_site` passed.
- `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --all --check` clean.
- Acceptance greps: `grep -c 'pub struct ForbidProjectWrites'` prints 1; `grep -c '// guarded:deploy projects' main.rs` prints 1; `grep -cE 'ReleaseRecordWriter|release_writer_for_node' main.rs` prints 0.
- Mutation runs, source restored after each. Dropping the stale comparison fails `live_changed_since_plan_is_stale` and `other_projects_untouched`. Dropping the empty-`changes` warning fails `empty_changes_warns`. Forcing `overwrite=true` fails `create_has_no_pre_image` and `other_projects_untouched`. A comment naming the writer in `main.rs` fails `write_half_unreachable`; one in `actions/deploy_apply.rs` fails `release_writer_is_confined`.

## Deviations from Plan

1. **[Rule 3 - Blocking] `RunDir::create_private_file_in` added in `run_store.rs`.** The pre-image goes to `export/<project>.zip`, a path below the run directory, and the existing `create_private_file` takes one plain name. The new method creates the subdirectory at 0700 and the file at 0600, with the same component checks. Commit `4814da9`.
2. **`deploy_projects_run` also takes `loaded` and `input`.** The run needs the built artifact zip and the gate flags written to `run.json`. Commit `4814da9`.
3. **The `record_writer` seam moved into Task 1.** The run could not build without deciding where the writer comes from. Task 2 uses it as planned. Commit `4814da9`.
4. **The fake gateway work moved into Task 1** (`project_import`, `answer_empty_changes`, `change_live`, `call_log`, `snapshot_except`, `ForbidProjectWrites`, `FailingRecordWriter`), because the tracer's own tests needed it. Task 2 only added tests on top of it. Commit `4814da9`.
5. **Hint wording changed** for `PlanStale` and `InvalidInput`, so the hints fit the project write path and contain no `ign ` command text. Commit `4814da9`.
6. **`projects_check_needs_check_flag` was removed** and replaced by the Decision C test, because `--check` is no longer `required`. Commit `4814da9`.
7. **Tests were written after the implementation in Task 1** (the plan says tests first). The mutation runs listed above show they are not vacuous. Task 2's tests were written against the finished code and checked by mutation the same way.
8. **Extra defences in `execute`.** A record-only run re-reads the live project and refuses `plan_stale` unless it still equals the artifact. A failed or unusable read-back after the import is `imported_unverified` plus a warning, not an error return. Commit `4814da9`.
9. **Open until 25-12:** `imported_unverified` exits 0 with a null slug and a null `restore_command`; the published verdict (V6) lands there. The plan records this as an assumption.
10. **Open until 25-13:** a target whose master is an ssh node refuses `deploy_tree_invalid` from `release_writer_for_node`, before anything is imported.
11. **Verify-command note.** The plan's `cargo test -p ignition-cli --lib ...` forms run 0 tests, because the registry and MCP tests live in the `ign` binary. They run under `--bin ign`; the full workspace run covers them.

## Requirement status

`requirements-completed` is empty on purpose. This plan delivers its share of DPRJ-01, 03, 04 and 05, but each is finished by a later plan, and a requirement is marked only when the plan finishes it.

- **DPRJ-01** (artifact verified against its pin before any upload): the gate is done here for the local build. 25-16 (`--targets`) and 25-18 (the CI workflow) carry the other entry points, and 25-19 proves it live.
- **DPRJ-03** (export before overwrite, verify after, by canonical hash): done for local nodes. The published unverified verdict and restore command are 25-12, ssh record writers are 25-13, and the Designer and wrong-target preflights are 25-14.
- **DPRJ-04** (unlisted projects reported and never deleted; git-managed refused): the isolation proof for the write path is here and the git refusal was earlier. 25-17 and 25-19 own the unmanaged-project reporting evidence and the live proof.
- **DPRJ-05** (`--check` writes nothing): proven at the engine and CLI levels. 25-12, 25-17, 25-18 and 25-19 each add to it (progress output, canary, CI check mode, live).
- **DPRJ-02** (parent-first, unchanged skip) is not in this plan's frontmatter and stays open.

## Known Stubs

None. The ssh refusal and the exit-0 `imported_unverified` are documented, tested fail-safe behaviours, with the plans that finish them named above, not stubs.

## Threat Flags

None beyond the plan's register. T-25-34 to T-25-39 are mitigated as planned: Decision C and hash gate before contact (`projects_apply_hash_mismatch_uploads_nothing`), pre-image before import with `plan_stale` and no second import, the 0600 pre-image in a git-ignored 0700 run directory, the writer confined by the two scans, per-run audit files, and master-only writes (`other_projects_untouched` and the single import call in each scenario; the action resolves no backup session).

## Self-Check: PASSED

- Files exist: `crates/ignition-core/tests/deploy_projects.rs`, `crates/ignition-core/tests/deploy_transport.rs`, `crates/ignition-core/tests/common/fake_projects.rs`, `crates/ignition-core/src/actions/deploy_projects.rs`, `crates/ignition-core/src/deploy/transport/projects.rs`: found.
- Commits exist: `4814da9`, `b692182`: found in `git log`.
- No `todo!`, `unimplemented!`, `#[ignore]` or skipped test was added by this plan; the 75 ignored tests are the earlier live gates.
- No background work is left running.
