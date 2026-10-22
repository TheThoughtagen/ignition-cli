---
phase: 25-guarded-project-deploy
plan: 04
subsystem: deploy
tags: [deploy-projects, check, canonical-digest, artifact-zip, mcp-exclusion, tui-route]

requires:
  - phase: 25-guarded-project-deploy
    provides: "25-03 contract decisions K1-K15 and V1-V11 (approved 2026-10-22)"
provides:
  - "deploy::projects: ProjectFile (strict deploy/project.yaml), read_project_meta, ProjectAction, MemberDelta, member_delta"
  - "deploy::project_artifact: build_artifact (deterministic checkout-layout zip), canonical_digest, is_module_dir_name"
  - "actions::deploy_projects: ProjectGateway (three read methods), GatewayProjects, load_project, deploy_projects_prepare, DeployProjectsResult"
  - "ign deploy projects --env --release --check [--repo] [--targets] (GET only, no .ign/), absent from MCP, TUI row, README Commands row"
affects: [25-05, 25-06, 25-07, 25-09, 25-10, 25-11]

actuals:
  tokens: 23500
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "read-only gateway trait with no write method as the structural zero-write proof"
    - "canonical digest over key NUL sha256 LF lines, independent of zip entry order"

key-files:
  created:
    - crates/ignition-core/src/deploy/projects.rs
    - crates/ignition-core/src/deploy/project_artifact.rs
    - crates/ignition-core/src/actions/deploy_projects.rs
    - crates/ignition-cli/tests/contract_deploy_projects.rs
  modified:
    - crates/ignition-core/src/deploy/mod.rs
    - crates/ignition-core/src/actions/mod.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/src/mcp.rs
    - crates/ignition-tui/src/routes.rs
    - README.md

key-decisions:
  - "Canonical key is the zip path verbatim (K1 a); no resources segment is stripped by name"
  - "A root file whose name matches the module-id pattern (project.json, tags.json) is skipped as a top-level candidate, never refused"
  - "An empty attributes object is NOT dropped from resource.json before hashing: a hand-authored descriptor without attributes compares as changed against the gateway's stamped one"

requirements-completed: [DPRJ-05, DPRJ-01, DPRJ-02, DPRJ-04]  # partial: the apply, guard and record plans finish these

coverage:
  - id: D1
    description: "deploy projects --check reports artifact_sha256, live_sha256, action create/skip/import, would_change, member counts, removed paths, unmanaged"
    requirement: DPRJ-05
    verification:
      - kind: integration
        ref: "cargo test -p ignition-cli --test contract_deploy_projects (12 tests: import, skip, create, unmanaged, removed members, env not listed, needs --check, no .ign, default targets, missing project.json, symlink, human lines)"
        status: pass
    human_judgment: false
  - id: D2
    description: "The check sends GET requests only and the gateway surface has no write method"
    requirement: DPRJ-04
    verification:
      - kind: integration
        ref: "assert_only_gets in contract_deploy_projects; grep -cE 'fn (project_delete|project_rename|project_copy|project_modify|project_create)' deploy_projects.rs prints 0"
        status: pass
    human_judgment: false
  - id: D3
    description: "Artifact is deterministic and canonical digest ignores entry order and volatile fields"
    requirement: DPRJ-01
    verification:
      - kind: unit
        ref: "cargo test -p ignition-core --lib deploy::project (26 tests incl. build_is_byte_identical_twice, digest_ignores_entry_order_directories_and_volatile_fields, digest_counts_checkout_layout_members)"
        status: pass
    human_judgment: false
  - id: D4
    description: "deploy projects is off the MCP catalog by full path and has a TUI row and README row"
    requirement: DPRJ-05
    verification:
      - kind: unit
        ref: "mcp::deploy_projects_is_excluded_by_full_path; tui_coverage; deploy_family_coverage"
        status: pass
    human_judgment: false
  - id: D5
    description: "Whether skip is correct for real gateway-stamped projects (resource.json without an attributes object, Perspective view loading)"
    requirement: DPRJ-02
    verification: []
    human_judgment: true
    rationale: "Proved against synthetic fixtures only; the live gates in 25-19 compare a real gateway export"

duration: "about 4h wall time, most of it waiting on a machine under load 100 to 270 and a reboot that wiped the build cache"
completed: 2026-10-22
status: complete
---

# Phase 25 Plan 04: deploy projects --check Summary

**`ign deploy projects --env --release --check` builds a deterministic artifact from the project checkout, reads the live project through GET-only calls and reports create, skip or import with canonical sha256 digests, member counts, the members an overwrite would delete and the other projects on the gateway.**

## Accomplishments

- **Task 1 (tracer, `eac00cd`).** `deploy/project.yaml` is strict (`schema_version: 1`, `project`, `environments`; project name checked for POSIX and Windows safety; an unlisted `--env` is `deploy_tree_invalid` naming the file, before any request). `build_artifact` zips the root `project.json` plus the `ignition` and module-id top-level directories, each `resource.json` and exactly the files it lists, Deflate, one fixed timestamp, entries in byte order, directory entries for every ancestor; two builds are byte-identical. `canonical_digest` hashes every file entry under its zip path (`resource.json` through `normalize_descriptor` plus a sorted `files` array, `project.json` with absent `description`/`parent` read as "", entry order and directory entries ignored, empty member set refused). `deploy_projects_prepare` lists, finds, exports and digests the live project and decides `create` (absent, D-13), `skip` (both digests non-empty and equal) or `import`. `ProjectGateway` has `projects`, `project_find`, `export_bytes` and nothing else. The leaf is in `EXCLUDED_PATHS`, has its `Screen::Projects` row and a human renderer.
- **Task 2 (`5d0e6cc`).** README Commands row for the check, listing the reads, the actions, the GET-only guarantee, the MCP exclusion and the JSON data keys.

## Test results

- `cargo test -p ignition-core --lib deploy::project`: 26 passed.
- `cargo test -p ignition-cli --test contract_deploy_projects`: 12 passed. `--test tui_coverage` 5 passed, `--test deploy_family_coverage` 8 passed.
- `cargo test -p ignition-cli -p ignition-core -p ignition-tui` (whole packages, includes `readme_exit_table_agreement`, the mcp lib test `deploy_projects_is_excluded_by_full_path` and the MCP contract suites): all green, 91 test binaries ok, 0 failed.
- `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --all --check` clean.
- Acceptance greps: `"deploy projects"` appears 3 times in `mcp.rs`, `path: "deploy projects"` once in `routes.rs`, 0 write-method names in `deploy_projects.rs`, the README row once, `^## Exit codes` once.

## Deviations from Plan

### Contract departures applied (the Contract decisions section wins)

- **K1 a:** the canonical key is the zip path verbatim. The plan text says "map `<collection>/resources/` back to `<collection>/`" (the K1 b adapter), which would mis-key `com.inductiveautomation.webdev/resources`. Pinned by `a_webdev_resources_directory_keeps_its_name`.
- **K2:** a directory inside a resource folder (after the denylist) refuses `deploy_tree_invalid` naming its path; unlisted files, dotfiles and folders without `resource.json` are left out.
- **K3:** `deploy` is denied at the repo root only (it never matches the allowlist, and nothing named `deploy` below a module directory is dropped).
- **K4 b:** `files` array sorted, `project.json` defaults for `description` and `parent`.
- **K6:** parent facts from `projects/list` and `invalid_parent` belong to 25-10. This slice reads `parent` from `project.json` into `ProjectMeta` and checks nothing yet.

### Auto-fixed Issues

**1. [Rule 1 - Bug] Module-id pattern matched root files**
- **Found during:** Task 1 unit tests
- **Issue:** `project.json` matches `^[a-z][a-z0-9_-]*(\.[a-z0-9_-]+)+$`, so the root-entry loop refused it as "must be a directory".
- **Fix:** top-level candidates that are files are skipped; only a symlink or a directory is examined. Root entries outside the allowlist (any kind, including symlinks and non-UTF-8 names) are never examined (`symlink_among_excluded_root_entries_is_left_out`).
- **Files modified:** `crates/ignition-core/src/deploy/project_artifact.rs`
- **Commit:** `eac00cd`

### Plan scope notes

- `--release` is passed through unvalidated and echoed. The semver rule (V4) lands with `Release::parse` in 25-09, which the plan assigns to that slice.
- `GUARDED_AND_EXCLUDED` and `GUARDED_OPS` are not touched: this slice has no guarded path (`--check` is required, clap refuses without it). 25-11 adds them.
- `LoadedProject.meta` (parent) and `target_file` are loaded but not yet consumed; 25-10 and 25-07 use them.

**Total deviations:** 1 auto-fixed bug, plus the five contract departures above. **Impact:** none on scope.

## Build-environment incident

The shared `CARGO_TARGET_DIR` (`~/Library/Caches/cargo-target`) was wiped mid-build twice (disk full at 100 percent, then a machine reboot) while a rust-analyzer `cargo check --workspace` and other agents built concurrently, with load averages up to 270. I built into a private target directory under the session scratchpad instead; nothing from it is left behind that matters and no cargo process of mine is running. No source was lost.

## Known Stubs

None.

## Threat Flags

None beyond the plan's register. T-25-11 (symlinks refused, allowlisted content only), T-25-12 (project name component checks, REST segment encoded by the existing client), T-25-13 (MCP full-path exclusion with test), T-25-14 (read-only gateway trait, GET-only assertion in every check test) are mitigated; T-25-15 accepted (names, hashes and counts only).

## Next

Ready for 25-05 (engine suite over `FakeProjects` and the README section).

## Self-Check: PASSED

- Files exist: `deploy/projects.rs`, `deploy/project_artifact.rs`, `actions/deploy_projects.rs`, `tests/contract_deploy_projects.rs`, this SUMMARY.
- Commits `eac00cd` and `5d0e6cc` exist on `main`.
- All plan acceptance criteria re-run and green (see Test results).
