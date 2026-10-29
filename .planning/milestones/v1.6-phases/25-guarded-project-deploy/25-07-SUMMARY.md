---
phase: 25-guarded-project-deploy
plan: 07
subsystem: deploy
tags: [deploy-projects, project-host, git-managed, fail-closed, error-slug, local-fs]

requires:
  - phase: 25-guarded-project-deploy
    provides: "25-04 load_project and deploy_projects_prepare; 25-05 FakeProjects and the engine suite; 25-06 artifact rules; 25-03 contract decisions V2 and K9"
provides:
  - "GitEntry, ProjectHost (git_entry, read_release_record; no write or delete method) and project_host_for_node (local arm; ssh refuses until 25-08)"
  - "LocalFsTransport as a ProjectHost: symlink_metadata at every level, fail closed"
  - "RELEASE_RECORD_FILE, RELEASE_RECORD_MAX_BYTES, release_record_rel, release_record_location (next to ledger_rel)"
  - "CoreError::ProjectGitManaged (project_git_managed, exit 6) in all four places, DEPLOY_SLUGS (13) pinned exactly, README exit-6 row"
  - "ProjectsSurfaces { gateway, host } and the .git probe ahead of every gateway read in deploy_projects_prepare"
  - "README: Git-managed projects paragraph in ### deploy projects"
affects: [25-08, 25-09, 25-11, 25-13, 25-17]

actuals:
  tokens: 15100   # chars/4 over `git diff c64102f~1..HEAD -- crates README.md` (60370 chars)
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "read-only host trait with no write or delete method; the write trait is a separate trait in 25-11"
    - "host probe walks each level with symlink_metadata so a missing level and an unreadable level are different answers (absent vs transport_failed)"
    - "a host-resolution error returns before any gateway contact (project_host_for_node runs before the session)"

key-files:
  created:
    - crates/ignition-core/src/deploy/transport/projects.rs
    - crates/ignition-core/src/deploy/transport/local/project_ops.rs
  modified:
    - crates/ignition-core/src/deploy/transport/mod.rs
    - crates/ignition-core/src/deploy/transport/local.rs
    - crates/ignition-core/src/deploy/ledger.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-core/src/actions/deploy_projects.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/tests/contract_deploy_projects.rs
    - crates/ignition-core/tests/deploy_projects.rs
    - crates/ignition-core/tests/deploy_apply.rs
    - README.md

key-decisions:
  - "K9 local probe is symlink_metadata: it never follows, and any kind (directory, file, symlink, other) counts as git-managed. The Windows form (Get-Item -Force, for the Hidden attribute) is 25-08's."
  - "A missing projects directory exits 4 through transport_failed, not 'absent': an unreadable host must never pass as 'no git' (T-25-20)."
  - "deploy_projects_prepare takes ProjectsSurfaces so the host and the gateway are separate seams; the engine suite builds a LocalFsTransport host over a tempdir."

patterns-established:
  - "Engine tests assert 'zero gateway calls' through FakeProjects list_count, find_calls and export_calls"

requirements-completed: []  # DPRJ-04 is listed in the plan frontmatter but stays open: the ssh arm (25-08) and the apply-time refusal (25-11) finish it, as 25-05 and 25-06 left their requirements open

coverage:
  - id: E1
    description: "DPRJ-04 .git of any kind (directory, file, symlink) on a local node refuses project_git_managed naming project, location and kind, before any gateway request"
    requirement: DPRJ-04
    verification:
      - kind: integration
        ref: "cargo test -p ignition-cli --test contract_deploy_projects projects_check_ (projects_check_git_dir_refuses, projects_check_no_host_access_refuses, projects_check_missing_projects_dir_is_transport_failed)"
        status: pass
      - kind: integration
        ref: "cargo test -p ignition-core --test deploy_projects git_ (git_present_refuses_before_gateway: directory, file, symlink, zero fake calls)"
        status: pass
    human_judgment: false
  - id: E2
    description: "Fail closed: an unreadable project directory or a missing projects directory is transport_failed, never absent; an absent project directory or absent .git proceeds"
    requirement: DPRJ-04
    verification:
      - kind: unit
        ref: "deploy::transport::local::project_ops (local_git_entry_kinds, local_git_entry_missing_projects_dir_is_transport_failed, local_git_entry_unreadable_project_dir_is_transport_failed_not_absent)"
        status: pass
      - kind: integration
        ref: "git_absent_proceeds, git_probe_unreadable_is_transport_failed"
        status: pass
    human_judgment: false
  - id: E3
    description: "project_git_managed in the four places and the README exit-6 row; DEPLOY_SLUGS pinned at 13 and never holding a verdict slug"
    requirement: DPRJ-04
    verification:
      - kind: unit
        ref: "error::tests (project_git_managed_four_places, deploy_slugs_are_pinned_and_never_a_verdict, exit_code_mapping_enumerated, readme_exit_table_agreement); deploy_family_coverage every_deploy_slug_is_in_the_readme_exit_table"
        status: pass
    human_judgment: false
  - id: E4
    description: "Release record read: absent at every level is None; a symlink, a directory or an oversize file is deploy_run_invalid naming the location"
    requirement: DPRJ-04
    verification:
      - kind: unit
        ref: "local_release_record_read"
        status: pass
    human_judgment: false
  - id: E5
    description: "Whether a real gateway host (ssh, Linux and Windows, including a Hidden .git) answers the probe the same way"
    requirement: DPRJ-04
    verification: []
    human_judgment: true
    rationale: "Owned by 25-08 (ssh ProjectHost, C-H after grant) and the 25-19 live gates; this plan covers local nodes only"

duration: "about 1h20m across the first executor and this continuation"
completed: 2026-10-22
status: complete
---

# Phase 25 Plan 07: Git-managed project refusal Summary

**`ign deploy projects --check` reads `data/projects/<name>/.git` on a local master node through a new read-only `ProjectHost` and refuses `project_git_managed` (exit 6) before any gateway read; an unreadable or missing host path is `transport_failed`, never "no git".**

## Accomplishments

- **Task 1, tracer (`c64102f`).** `GitEntry`, the read-only `ProjectHost` trait and `project_host_for_node` (local arm; the ssh arm refuses `deploy_tree_invalid` until 25-08). The local probe walks `<data_dir>/projects`, `<project>` and `.git` with `symlink_metadata`. The release record helpers sit next to `ledger_rel`, and the local `read_release_record` copies the `read_ledger_blocking` rules. `project_git_managed` landed in `code()`, exit class 6, `hint()` (no `ign `), `DEPLOY_SLUGS` and the README exit-6 row. The count-only slug test became `deploy_slugs_are_pinned_and_never_a_verdict` (13 entries, exact order). `deploy_projects_prepare` probes first; `main.rs` resolves the host before the session. The user accepted the tracer at the human-verify gate ("Accept, continue (Recommended)").
- **Task 2 (`54affc7`).** Three engine cases in `tests/deploy_projects.rs`: `git_present_refuses_before_gateway` (directory, file, dangling symlink; each kind named; `list_count`, `find_calls` and `export_calls` all zero), `git_absent_proceeds` (absent project directory gives `create`; present directory without `.git` gives `skip`), `git_probe_unreadable_is_transport_failed` (mode 000 project directory, and a removed `projects` directory; the permission case is guarded for a privileged user). README `**Git-managed projects.**` paragraph in `### deploy projects`: the probe location, any kind counts, a symlink is never followed, fail closed, no delete or rename and no flag for it, cutover is per project by an administrator (D-11).
- `FakeProjects` already counted calls (`list_count`, `find_calls`, `export_calls`), so no fake change was needed. `deploy_family_coverage` iterates `DEPLOY_SLUGS`, so it covers the new slug unchanged and stays green.

## Test results

- `cargo test -p ignition-core`: all binaries pass, 1074 unit tests, `deploy_projects` 12 passed; 0 failed, ignored tests are the pre-existing live gates.
- `cargo test -p ignition-cli`: all binaries pass, 0 failed (`contract_deploy_projects`, `deploy_family_coverage`, `readme_exit_table_agreement` included).
- `cargo clippy --workspace --all-targets -- -D warnings` clean (one `type_complexity` lint in the new engine test fixed with a `GitShape` alias); `cargo fmt --check` clean.
- Acceptance: `grep -c project_git_managed README.md` prints 2, `grep -c '^## Exit codes' README.md` prints 1, `grep -cE 'async fn (delete|remove|rename|write)'` on `transport/projects.rs` prints 0.
- Built in a private target dir under the session scratchpad; no background work left running.

## Deviations from Plan

### Contract departures applied

- **K9 local probe is `symlink_metadata`** (no follow, any kind counts, including `Other`). The Windows `Get-Item -Force` form that sees a Hidden `.git` is 25-08's.

### Auto-fixed (Rule 3, blocking)

- **Fixtures now create `data/projects`.** The probe fails closed on a missing `projects` directory, so the engine `Checkout` and the CLI contract fixtures create `<data>/projects`.
- **Engine `prepare()` builds a `LocalFsTransport` host and `ProjectsSurfaces`** because `deploy_projects_prepare` now takes the surfaces struct.
- **Dropped the hard-coded `DEPLOY_SLUGS.len() == 12` in `tests/deploy_apply.rs`:** the list is pinned exactly in `error::tests::deploy_slugs_are_pinned_and_never_a_verdict`, so the count there was redundant and went stale on adding the slug.

### Plan behaviours changed

- **ESC human-output test dropped.** The plan family planned one for the new error text; the 25-04 name check refuses a control character in the project name before this error can be built, and `render_error` already escapes (25-06), so the test could not reach the case.
- **Host-resolution error shape:** `main.rs` returns `(Some(profile), Err)` when `project_host_for_node` fails, so the error envelope still names the profile.
- **Missing `projects` directory exits 4** through `transport_failed` (the plan said `transport_failed`; the exit class follows from it), pinned by `projects_check_missing_projects_dir_is_transport_failed`.

### Process

- **Tests and implementation were written in one pass** in both tasks, not red first. The fail-closed cases were checked against the implementation by assertion rather than a red run on absent code.
- **Tracer gate:** honoured. The first executor stopped at `checkpoint:human-verify` after `c64102f`; the user accepted.
- **README cutover sentence:** the plan names one paragraph; it states cutover is per project by an administrator and points at the runbook that 25-17 writes. The runbook itself is 25-17's.

### Auto-fixed Issues

None beyond the above.

## Open points

- DPRJ-04 stays open: ssh hosts (25-08) and the apply-time refusal (25-11) remain. `requirements mark-complete` was not run, as in 25-05 and 25-06.
- The ssh arm of `project_host_for_node` refuses `deploy_tree_invalid` for ssh masters until 25-08; a target with an ssh master cannot run `deploy projects --check` in the meantime. No release ships between the two plans (plan note).
- `read_release_record` has no caller yet; 25-09 adds the release guard that reads it.

## Known Stubs

None.

## Threat Flags

None beyond the register. T-25-20 mitigated (`local_git_entry_kinds`, `local_git_entry_missing_projects_dir_is_transport_failed`, `git_probe_unreadable_is_transport_failed`). T-25-21 mitigated (`local_git_entry_refuses_unsafe_project_names`, `component_problem` before any path is built). T-25-22 mitigated (`ProjectHost` has no write or delete method; grep prints 0). T-25-23 accepted (the message names project, location and kind only).

## Self-Check: PASSED

- Files: `crates/ignition-core/src/deploy/transport/projects.rs`, `crates/ignition-core/src/deploy/transport/local/project_ops.rs`, `crates/ignition-core/tests/deploy_projects.rs`, `README.md` exist and were committed.
- Commits found in `git log`: c64102f, 54affc7.
