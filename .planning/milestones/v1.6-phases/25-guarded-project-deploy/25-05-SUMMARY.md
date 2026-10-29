---
phase: 25-guarded-project-deploy
plan: 05
subsystem: deploy
tags: [deploy-projects, engine-tests, fake-gateway, readme]

requires:
  - phase: 25-guarded-project-deploy
    provides: "25-04 deploy_projects_prepare over ProjectGateway (three read methods)"
provides:
  - "tests/common/fake_projects.rs: FakeProjects, an in-memory ProjectGateway whose export copies the C8 captures (checkout layout, no directory entries, fresh lastModification stamps, description and parent added to project.json)"
  - "tests/deploy_projects.rs: engine suite for the check (skip, import counts, create, unmanaged, targets root, no .ign)"
  - "README ### `deploy projects` section"
affects: [25-06, 25-09, 25-10, 25-11, 25-15]

actuals:
  tokens: 7800
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "test double with only the read methods of its trait, so the suite cannot pass on a write surface the verb must not have"
    - "fake stamps fresh volatile fields on every export so a passing comparison proves the digest ignores them"

key-files:
  created:
    - crates/ignition-core/tests/common/fake_projects.rs
    - crates/ignition-core/tests/deploy_projects.rs
  modified:
    - crates/ignition-core/tests/common/mod.rs
    - README.md

key-decisions:
  - "The fake lists projects in descending byte order so no test passes on a listing that arrives sorted"
  - "The fake leaves a descriptor without an attributes object unstamped and the suite never uses one; the captures are silent on it (see Open points)"

requirements-completed: []  # none marked: engine evidence for DPRJ-01, DPRJ-02, DPRJ-04, DPRJ-05, but the apply, guard and record plans finish them (25-04 left DPRJ-05 open for the same reason)

coverage:
  - id: E1
    description: "Equal content with fresh gateway stamps is skip; one changed, one added and one removed member give the counts and the removed key; an absent project is create with live_sha256 null"
    requirement: DPRJ-02
    verification:
      - kind: integration
        ref: "cargo test -p ignition-core --test deploy_projects (check_skip_when_equal, check_import_counts, check_create_when_absent)"
        status: pass
    human_judgment: false
  - id: E2
    description: "unmanaged holds the other names byte-sorted, capped at 200 with the full count, names only; the check reads only the named project"
    requirement: DPRJ-04
    verification:
      - kind: integration
        ref: "check_unmanaged_names_only (205 projects), check_unmanaged_is_byte_sorted, check_reads_only_the_named_project"
        status: pass
    human_judgment: false
  - id: E3
    description: "A target found under the targets root; the same layout with the repo as base refuses deploy_tree_invalid naming targets/dev.yaml"
    requirement: DPRJ-01
    verification:
      - kind: integration
        ref: "check_targets_root"
        status: pass
    human_judgment: false
  - id: E4
    description: "A check creates no .ign directory in the repo or the targets root for create, skip and import"
    requirement: DPRJ-05
    verification:
      - kind: integration
        ref: "check_never_creates_ign_dir"
        status: pass
    human_judgment: false
  - id: E5
    description: "README documents the verb: one project from its own repo, strict project.yaml with example, artifact rules K1-K4, --targets as a root, check keys, create for an absent project, Manifest.projects not read"
    requirement: DPRJ-05
    verification:
      - kind: integration
        ref: "cargo test -p ignition-cli --test deploy_family_coverage (8 pass); cargo test -p ignition-core --lib readme_exit_table_agreement; grep -c '^### `deploy projects`' README.md prints 1; grep -c '^## Exit codes' README.md prints 1"
        status: pass
    human_judgment: false
  - id: E6
    description: "Whether FakeProjects behaves like a real gateway beyond what C8a-C8e recorded"
    requirement: DPRJ-02
    verification: []
    human_judgment: true
    rationale: "The fake copies only recorded behaviour; the 25-19 live gates repeat the decisions on the rig"

duration: "about 1h wall time"
completed: 2026-10-22
status: complete
---

# Phase 25 Plan 05: Engine suite for the deploy projects check Summary

**`FakeProjects` is a capture-faithful in-memory project store behind `ProjectGateway`; nine engine tests pin skip, import counts, create for an absent project, unmanaged names and the targets root with no network, and the README now documents the verb.**

## Accomplishments

- **Task 1 (tracer, `753bd6d`).** `FakeProjects` implements `ProjectGateway` and nothing else: no delete, rename, copy, modify, create, import or reparent method (`grep -cE 'fn project_(delete|rename|copy|modify|create|import)'` prints 0). Its export is a zip of file entries only in the checkout layout (C8c, K1 a), stamps `attributes.lastModification` and `lastModificationSignature` with a new value on every export (C8b) and adds `description` and `parent` to `project.json` (C8b, C8c). The module doc cites the C8 block for each behaviour and names where the captures are silent. `tests/deploy_projects.rs` has the six required behaviours (`check_skip_when_equal`, `check_import_counts`, `check_create_when_absent`, `check_unmanaged_names_only`, `check_targets_root`, `check_never_creates_ign_dir`) plus three more: `fake_export_stamps_a_fresh_modification_on_every_export` (proves the skip is the digest ignoring stamps, not the fake repeating itself), `check_unmanaged_is_byte_sorted` (uppercase before lowercase, listing order reversed by the fake) and `check_reads_only_the_named_project` (one list, one find and one export, all for the named project).
- **Task 2 (`44bfeba`).** README `### \`deploy projects\`` after the rollback section (before `### Running deploy in CI`, the next `###` heading). It covers what the verb deploys (D-01, D-05), the strict `deploy/project.yaml` with an example (D-08), what enters the artifact (K1 to K4 as recorded, K2 directory refusal, K3 root-only `deploy` denial is implied by "every other root entry stays out"), `--targets` as a root (D-07, D-14), the check keys, `create` for an absent project (D-13) and that `Manifest.projects` is not read (D-04). The literal `## Exit codes` heading is untouched.

## Test results

- `cargo test -p ignition-core --test deploy_projects`: 9 passed (the `check_` filter selects 8; the ninth is the stamp test).
- `cargo test -p ignition-cli --test deploy_family_coverage`: 8 passed. `cargo test -p ignition-core --lib readme_exit_table_agreement`: passed.
- `cargo test -p ignition-core -p ignition-cli` (whole packages): 90 binaries, 2423 passed, 0 failed, 73 ignored (pre-existing live gates).
- `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --all --check` clean.
- Built into a private target dir under the session scratchpad; no cargo process of mine is left running.

## Deviations from Plan

### Contract departures applied (the Contract decisions section wins)

- **K2:** the README states that a directory inside a resource folder refuses `deploy_tree_invalid` (the plan text predates it). The engine behaviour was already in 25-04; this plan documents it.
- **K4 b:** the README documents the `files` sort and the `project.json` `description` and `parent` defaults as ignored fields. The fake adds `description` and `parent` to `project.json` so the suite exercises that row.
- **K3:** README says `deploy/` is left out as a root entry, not denied at any depth.

### Process deviation

- **Tracer gate.** Auto mode is off (`workflow._auto_chain_active` and `workflow.auto_advance` both `false`), so the tracer rule asks for a `checkpoint:human-verify` after committing Task 1. Your instruction was to run the whole plan and return commits and results, and the tracer's `<verify>` is an automated test run, so I re-ran it after the commit (8 passed) and continued to the README task, which layers nothing on the tracer. Say so if you want the stop honoured in future plans.

### Auto-fixed Issues

None.

## Open points

- **Empty `attributes` not dropped before hashing (flagged by 25-04).** This plan owns no hashing code (`files_modified` lists tests and README only), so it is left as is. Effect: a hand-authored `resource.json` without an `attributes` key hashes differently from the gateway's export of it, which gains an `attributes` object holding the two stamps; after `normalize_descriptor` removes the stamps an empty `attributes: {}` remains on the live side only, so such a descriptor reads `changed` forever and `skip` never fires for it. Every descriptor in the C8 captures carried `attributes`, so the captures never exercised it, and `FakeProjects` leaves such a descriptor unstamped rather than inventing the gateway's behaviour. The fix belongs in `canonical_resource_json` in `crates/ignition-core/src/deploy/project_artifact.rs` (25-06 owns the digest per the artifact table): drop an `attributes` key whose value is an empty object after normalisation, then confirm against a real export in 25-19.
- The README describes `--check` as the only mode ("required in this release") and says a later release creates the project on apply. `.ignexclude` and the `excluded` result key (K3, K2) are not mentioned because they are not built yet (25-06).

## Known Stubs

None.

## Threat Flags

None beyond the register. T-25-62 mitigated (module doc cites a capture for each behaviour and names the silent places; stamps are exported fresh so normalisation is exercised; 25-19 repeats the decisions live). T-25-63 mitigated (README uses `CapApp`, `dev` and `prod`; no hosts, tokens or file content).

## Self-Check: PASSED

- Files exist: `crates/ignition-core/tests/common/fake_projects.rs`, `crates/ignition-core/tests/deploy_projects.rs`, this SUMMARY; `crates/ignition-core/tests/common/mod.rs` has `pub mod fake_projects;`.
- Commits `753bd6d` and `44bfeba` exist on `main`.
- `grep -c 'pub struct FakeProjects'` prints 1; `grep -c '^### `deploy projects`' README.md` prints 1; `grep -c '^## Exit codes' README.md` prints 1; 0 write-method names in `fake_projects.rs`.
- `.planning/config.json` and `.planning/research/.cache/` were not staged.
