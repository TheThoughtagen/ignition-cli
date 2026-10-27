---
phase: 25-guarded-project-deploy
plan: 06
subsystem: deploy
tags: [deploy-projects, artifact, ignexclude, denylist, caps, proptest, terminal-escape]

requires:
  - phase: 25-guarded-project-deploy
    provides: "25-04 build_artifact, canonical_digest, load_project; 25-05 engine suite and README section"
provides:
  - "IgnExclude (K3 grammar), IGNEXCLUDE_FILE, BUILTIN_DENY applied at any depth, Artifact.excluded"
  - "DeployProjectsResult.excluded / excluded_count; human output prints the count and the first 20, control characters escaped"
  - "Refusals before any gateway contact: listed-but-missing file, artifact holding only project.json, walk over ARTIFACT_MAX_ENTRIES, built zip over IMPORT_MAX_BYTES, non-UTF-8 name"
  - "validate_import as pub(crate); check_artifact_zip run by load_project on every build"
  - "proptest digest_invariants pinning the canonical digest"
  - "README: what goes into the artifact, .ignexclude grammar, caps, deploy/** -text"
affects: [25-07, 25-09, 25-10, 25-11, 25-15]

actuals:
  tokens: 21000   # chars/4 over `git diff f776846..HEAD -- crates README.md` (84124 chars before the last fix, ~88000 after)
  tasks: 2
  commits: 5

tech-stack:
  added: []
  patterns:
    - "build limits as a private struct (Limits) so a test reaches a cap refusal with a small value, not an environment variable"
    - "human renderers build a Vec<String> of lines so the escape rule is unit-testable without capturing stdout"
    - "name taken from a repo or gateway is escaped with char::escape_default before it reaches a terminal; JSON already escapes"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/deploy/project_artifact.rs
    - crates/ignition-core/src/actions/deploy_projects.rs
    - crates/ignition-core/src/actions/projects.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/contract_deploy_projects.rs
    - README.md

key-decisions:
  - "K3 wins over the plan text: `deploy` is not in BUILTIN_DENY. It is denied at the repo root only, by the root allowlist, so a script package or view named `deploy` below a module directory ships"
  - "An artifact with nothing besides project.json refuses at build time with deploy_tree_invalid (not left to canonical_digest, which only rejects a fully empty zip)"
  - "Size check is on the built zip, not the raw bytes read, so a compressible checkout is not refused for its uncompressed size"
  - "The human error path escapes control characters (newline kept) for all verbs, because error messages name refused repo paths"

patterns-established:
  - "Limits struct with REAL const for cap tests"

requirements-completed: []  # none marked, as in 25-05: this plan finishes the artifact rules (DPRJ-01 hash input, DPRJ-03 digest stability) but the import, verify and record plans finish both requirements

coverage:
  - id: E1
    description: "`.ignexclude` removes the paths it lists; BUILTIN_DENY applies at any depth; every path left out is in data.excluded, byte-sorted, with excluded_count; the human output prints the count"
    requirement: DPRJ-01
    verification:
      - kind: integration
        ref: "cargo test -p ignition-cli --test contract_deploy_projects (projects_check_reports_excluded, projects_check_ignexclude_changes_digest, projects_check_human_prints_the_excluded_count, projects_check_bad_ignexclude_refuses_before_any_request)"
        status: pass
      - kind: unit
        ref: "cargo test -p ignition-core --lib deploy::project_artifact (ignexclude_grammar, builtin_deny_any_depth, excluded_lists_everything_left_out_byte_sorted)"
        status: pass
    human_judgment: false
  - id: E2
    description: "Edge DPRJ-01 empty: an artifact holding only project.json refuses, and a listed file that is absent refuses naming it"
    requirement: DPRJ-01
    verification:
      - kind: unit
        ref: "only_project_json_refuses, listed_missing_refuses, listed_but_missing_file_refuses, an_ignexclude_that_removes_every_member_refuses"
        status: pass
    human_judgment: false
  - id: E3
    description: "Walk over ARTIFACT_MAX_ENTRIES, zip over IMPORT_MAX_BYTES and a non-UTF-8 name refuse deploy_tree_invalid before any gateway contact; the built zip passes validate_import, a truncated one is invalid_import_file"
    requirement: DPRJ-01
    verification:
      - kind: unit
        ref: "entry_cap_refuses, size_cap_refuses, the_real_limits_are_the_documented_ones, non_utf8_name_refuses, actions::deploy_projects::tests::check_artifact_zip_refuses_truncated, a_built_artifact_passes_the_check"
        status: pass
    human_judgment: false
  - id: E4
    description: "Edge DPRJ-03 ordering: the digest is the same for any entry order, either compression method, any lastModification/signature (or no attributes), any key order and files order; it changes for a one-byte change and for a rename"
    requirement: DPRJ-03
    verification:
      - kind: unit
        ref: "digest_invariants and digest_sees_a_byte_change_and_a_rename (proptest, 256 cases each); both fail when sort_files_array or drop_empty_attributes is disabled (checked by mutation, then reverted)"
        status: pass
    human_judgment: false
  - id: E5
    description: "A repo name with an ESC byte reaches the terminal escaped in the excluded list, the removed list, the project/env/gateway/release line, warnings and human error lines"
    requirement: DPRJ-01
    verification:
      - kind: unit
        ref: "render::tests (escape_control_writes_controls_out_and_keeps_the_rest, excluded_names_with_control_characters_print_escaped, removed_names_and_project_names_print_escaped, error_text_escapes_controls_but_keeps_line_breaks)"
        status: pass
      - kind: integration
        ref: "projects_check_human_escapes_control_characters_in_excluded_names, projects_check_human_error_escapes_control_characters_in_a_path"
        status: pass
    human_judgment: false
  - id: E6
    description: "Whether the gateway adopts exactly the artifact members under every K3 allowlist and denylist case on a real rig"
    requirement: DPRJ-03
    verification: []
    human_judgment: true
    rationale: "The rules come from the C8 captures; the 25-19 live gates repeat them on the rig"

duration: "about 1h30 wall time across the first executor and this continuation"
completed: 2026-10-22
status: complete
---

# Phase 25 Plan 06: Artifact rules, caps and digest property tests Summary

**The artifact is exactly the adoptable project content: `.ignexclude` and an any-depth denylist remove paths, every exclusion is listed in `data.excluded`, caps and the empty-artifact and listed-missing refusals fire before any gateway contact, the built zip passes `validate_import`, and a proptest pins the canonical digest.**

## Accomplishments

- **Pre-task fix (`b359441`).** `canonical_resource_json` drops an empty `attributes` object on both sides, so a hand-authored descriptor without `attributes` no longer reads `changed` forever against the gateway's export (the open point 25-05 handed to this plan). Test shown red first.
- **Task 1, tracer (`bccb775`).** `IgnExclude::parse` / `matches` (K3 grammar, refusals name `.ignexclude` and the line), `BUILTIN_DENY` at any depth, `Artifact.excluded` (directories once with a trailing `/`), `excluded` and `excluded_count` in the check result, the human count line. The user accepted the tracer at the human-verify gate.
- **Task 2 (`f04bc0d`).**
  - `build_artifact` refuses an artifact whose only member is `project.json`, a built zip over `IMPORT_MAX_BYTES`, and (since the tracer) a walk over `ARTIFACT_MAX_ENTRIES`. Limits sit in a private `Limits` struct so tests reach each cap with a small value.
  - `validate_import` is `pub(crate)`; `check_artifact_zip` wraps it and `load_project` calls it on every build.
  - Non-UTF-8 names go through `utf8_name`, which is unit-tested with a raw `OsStr` (APFS will not create such a file, so the pure function carries the coverage there; the walk test runs where the filesystem allows the name).
  - Proptest `digest_invariants` (entry order, Stored vs Deflated, random `lastModification` and signature or no `attributes`, shuffled `resource.json` key order and `files` order) and `digest_sees_a_byte_change_and_a_rename`.
  - README: the artifact section is now `**What goes into the artifact (D-09, K1 to K4).**` with the K2 refusals, the denylist, the `.ignexclude` grammar, `excluded`, the caps and the `deploy/** -text` advice; the Commands row and the key list gained `excluded` and `excluded_count`.
- **Security fix, terminal escape injection (`cdb0eb9`, `94674fa`).** The human renderer printed `result.excluded` raw. `render_deploy_projects_human` now builds its lines in `deploy_projects_human_lines` and writes every control character out with `char::escape_default` (`\u{1b}`) for the excluded and removed paths, the project, gateway, env and release names and warnings. Audit of other new human lines in this phase: those were the only repo-derived or gateway-derived strings. The human error path (`render_error`) printed the error message and hint raw for every verb, and this phase's refusals name repo paths (a symlink, a bad `resource.json`), so it now escapes control characters other than the newline. JSON output is untouched. Tests: the ESC-in-excluded-name test required by the finding (unit and CLI level), removed/project/warning names, the error path, and that a plain accented name passes through.

## Test results

- `cargo test -p ignition-core`: all binaries pass, 1062 unit tests (43 in `deploy::project_artifact`, 2 new in `actions::deploy_projects`), 0 failed; the ignored tests are the pre-existing live gates.
- `cargo test -p ignition-cli`: 452 passed, 0 failed, 22 ignored (pre-existing live gates). `contract_deploy_projects` has 18 tests, all pass.
- `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --all --check` clean.
- Acceptance greps: `pub(crate) fn validate_import` 1, `validate_import(` in `deploy_projects.rs` 1, `ignexclude` in README 5.
- Built in a private target dir under the session scratchpad; no cargo process is left running; the `proptest-regressions` file produced by the mutation checks was removed.

## Deviations from Plan

### Contract departures applied (the Contract decisions section wins)

- **K3:** `deploy` is not in `BUILTIN_DENY` (the plan text lists it). It is denied at the repo root only, by the root allowlist; below a module directory a `deploy` package ships. Test `a_deploy_directory_below_a_module_directory_is_kept`. README says the same.

### Auto-added (Rule 2)

- **A `files` entry removed by `.ignexclude` refuses `deploy_tree_invalid`** naming the descriptor and `.ignexclude`, instead of shipping a resource the gateway would drop. Tests `ignexclude_leaving_out_a_listed_file_refuses`.
- **A symlinked or non-regular `.ignexclude` refuses.** Test `a_symlinked_ignexclude_is_refused`.
- **Human error lines escape control characters** (all verbs), beyond the one renderer named in the finding, because the same raw-name path existed there.
- **An `.ignexclude` that removes every member refuses** (falls out of the empty-artifact rule; covered by `an_ignexclude_that_removes_every_member_refuses`).

### Unplanned behaviours (recorded, not in the plan)

- `.ignexclude` cannot remove `project.json` (it is read before the walk and is never a walked entry).
- A symlink that `.ignexclude` matches is left out and listed, not refused (`an_ignexcluded_symlink_is_left_out_not_refused`); an unmatched one still refuses.
- The ARTIFACT_MAX_ENTRIES walk cap was pulled forward into Task 1 (the tracer carried the `Limits`-style cap parameter); Task 2 added the zip-size cap and renamed the test to `entry_cap_refuses`.

### Process

- **Tracer tests were written after the implementation** in Task 1, not first; only the pre-task `attributes` fix was shown red. Task 2 tests were also written with the implementation in the same pass, except that `digest_invariants` was checked by mutation (disabling `sort_files_array` and `drop_empty_attributes` each made it fail) instead of a red run on absent code.
- **Tracer gate:** honoured. The first executor stopped at `checkpoint:human-verify`; the user answered "Accept, continue (Recommended)".

### Auto-fixed Issues

None beyond the above.

## Open points

- No new exit codes: every refusal here is an existing slug (`deploy_tree_invalid`, `invalid_import_file`). `readme_exit_table_agreement` and `exit_code_mapping_enumerated` pass unchanged.
- The `-text` advice relies on the 25-04 behaviour that `deploy/project.yaml` refuses CR line endings (research Pitfall 5); not re-tested here.
- The size cap is checked after the zip is built, so a checkout of many large files is held in memory (each file is capped at 64 MiB by `READ_MAX_FILE_BYTES`, the walk at 200,000 entries). A streaming build is not worth the code at this size; 25-19 live gates will show if it matters.

## Known Stubs

None.

## Threat Flags

None beyond the register. T-25-16 mitigated (`builtin_deny_any_depth`, `.ign` in the tracer CLI test with the earlier run directory left in place). T-25-17 mitigated (`entry_cap_refuses`, `size_cap_refuses`). T-25-18 mitigated (`listed_missing_refuses`, `only_project_json_refuses`, `check_artifact_zip_refuses_truncated`). T-25-19 mitigated (`ignexclude_unsupported_constructs_refuse_with_the_line`). Added by the review finding: terminal escape injection through repo-derived names in human output, mitigated in the renderer and the error path.

## Self-Check: PASSED

- Files: `crates/ignition-core/src/deploy/project_artifact.rs`, `crates/ignition-core/src/actions/deploy_projects.rs`, `crates/ignition-core/src/actions/projects.rs`, `crates/ignition-cli/src/render.rs`, `crates/ignition-cli/tests/contract_deploy_projects.rs`, `README.md` all exist and were committed.
- Commits found in `git log`: b359441, bccb775, cdb0eb9, f04bc0d, 94674fa.
