---
phase: quick-261106-bt0
verified: 2026-11-06T11:45:00Z
status: passed
score: 10/10 must-haves verified
behavior_unverified: 0
overrides_applied: 0
re_verification: false
warnings:
  - id: W1
    summary: "prod_scale_missing_tags (3 s budget) failed once under extreme machine load (9.00 s at load average about 180 on 10 cores) and passed on every re-run (0.74 to 0.80 s)"
    severity: warning
    blocks_goal: false
---

# Quick 261106-bt0 Verification Report

**Goal:** `ign deploy projects --check` (and the real pre-import path) reports tags a project uses that the target gateway lacks, as a warning only. A failed tag read never reads as "no missing tags". Holds at production scale (43,000+ tags, 2,000+ files) with bounded gateway round trips.
**Branch / HEAD:** `feat/project-check-missing-tags` at `bbd983f`, base `50b4257`, 10 commits.
**Verified:** 2026-11-06
**Re-verification:** No, initial verification.

No source file was edited, nothing was staged or committed, `cargo install` was not run, and no production host, VM or ssh was touched. `git status --short` after all runs shows only the four paths that were already unstaged at the start plus this quick-task directory.

## Gates, run by the verifier on HEAD `bbd983f`

| # | Gate | Result |
|---|------|--------|
| 1a | `cargo test --workspace` (first run) | exit 101: 3 failures, all timing budgets in `deploy_prod_scale` under load (see W1) |
| 1b | `cargo test --workspace --no-fail-fast` (re-run) | exit 0, 3286 passed, 0 failed, 100 ignored |
| 1c | `prod_scale_missing_tags` alone, `--test-threads=1`, three runs | 0.78 / 0.74 / 0.80 s against the 3 s budget, all ok |
| 2 | `PATH=$HOME/.cargo/bin:$PATH cargo xwin clippy --target x86_64-pc-windows-msvc --workspace --all-targets -- -D warnings` | exit 0 |
| 3a | `cargo fmt --check` | pass |
| 3b | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| 4 | `git diff --quiet 50b4257 HEAD` on `Cargo.toml`, `Cargo.lock`, both crate manifests, `error.rs` | unchanged |
| 4 | same on the golden paths (`deploy_plan.rs`, `plan_fixture.rs`, `plan.rs`, `contract_deploy.rs`, `deploy/render.rs`, `deploy/tree.rs`, `fixtures/deploy/repos/tracer`, `apply/fingerprint.rs`) | unchanged |
| 5 | Deny-list over `git diff 50b4257 HEAD` (list non-blank, 15 entries, diff non-empty) | clean, 0 hits |
| 5 | Dotted-quad in added lines of `crates` and `README.md` | 0 |
| 5 | Deny-list and dotted-quad over the untracked PLAN.md and SUMMARY.md in this directory | 0 hits each |
| - | `exit_code_mapping_enumerated`, `readme_exit_table_agreement` | ok in both full runs |
| - | `grep -c "paths built at runtime aren't checked" README.md` | 1 |
| - | Added lines in non-test source with `todo!`, `unimplemented!`, `TODO`, `FIXME`, `XXX`, `TBD`, `HACK` | none |
| - | Added `#[ignore]`, `should_panic`, `assert!(true)` anywhere in the diff | none |

Test totals: the SUMMARY claims 3283 passed. The verifier's run shows 3286 passed. The three extra are the tests added in the last commit (`bbd983f`), which the SUMMARY says it did not re-run workspace-wide, so the figures are consistent.

## Observable truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `--check --json` reports `missing_tags` (byte-sorted, deduplicated, `{path, files}`, at most 3 files) and `missing_tags_count` | VERIFIED | `check_tag_references` (`actions/deploy_projects.rs`): `found` keyed by (folded provider, folded path), `files` trimmed to `FILES_PER_ENTRY` = 3, list sorted by bytes, `cap_list` to `LIST_CAP` = 200 with the full count beside it. Tests: `projects_check_reports_a_missing_tag` (binary, real client, wiremock), `missing_tags_names_five_files_as_three_and_two_spellings_as_one`. |
| 2 | Same fields ride the confirmed run's `plan.json` and `result.json`, and the confirmation refusal names the paths | VERIFIED | The check runs inside `deploy_projects_prepare`, which `--check` and the apply share. `execute` builds `result` from `plan.clone()` (line 1921) and `plan.json` is the plan (line 1897). `plan_prose` adds count lines, up to 20 paths, unknown providers and `NOT CHECKED`. Tests: `projects_apply_refusal_names_the_missing_tags`, `missing_tags_a_confirmed_import_still_runs_and_records_the_path`. |
| 3 | A provider the target lacks never passes as clean: `missing_tags_unknown_providers`, never in `missing_tags`, warning line, no export call | VERIFIED | D8 class 3 branch in `check_tag_references` (unlisted, `provider_shaped`) writes only to `unknown`, then `continue`s before `tag_export`. Tests: `missing_tags_a_provider_the_gateway_lacks_is_unknown_and_costs_no_export`, `projects_check_missing_tags_unknown_provider_is_reported_and_costs_no_export` (asserts no `tags/export` request), `missing_tags_bracket_text_that_cannot_be_a_provider_is_not_a_reference`. |
| 4 | Warning only: exit code, slugs, `action`, `would_change`, `artifact_sha256`, `members`, import identical | VERIFIED | `check_tag_references` is infallible by signature (returns `TagCheck`, no `Result`), so no tag failure can leave `prepare` as `Err`. `tag_warnings` only pushes strings. `error.rs` unchanged and both exit-table tests pass. `missing_tags_leaves_the_plan_untouched` compares gateway-has-tag, gateway-lacks-tag and gateway-lacks-provider on `action`, `would_change`, `artifact_sha256`, `members`, `ok`, `slug`, `outcome` with assertions that the three runs really differ in the tag fields (counts 0, 1 and unknown 1). The import itself still runs and records (`missing_tags_a_confirmed_import_still_runs_and_records_the_path`). |
| 5 | UDT inherited members resolve with or without stubs, nested instances, stub with no typeId, type inheritance, qualified typeId. Unresolvable type or cycle is never reported | VERIFIED | `TagTree::lookup` widens a frontier to each node plus its `typeId` chain on every segment (`expand`), guards repeats (`ptr::eq`) and 32 levels, and sets `unknown` on any unresolved link. `Missing` is returned only when the frontier empties with `unknown == false`; `Found` and `Unknown` both `continue` in `check_tag_references`. 12 `tree_*` tests build the captured shape plus the uncaptured shapes (bare instance, nested, stub without typeId, `Child` inheriting `T`, `[default]_types_/A/B/T`, other-provider typeId, unknown type, 2-cycle, 40-link chain). The multibyte `typeId` panic from the lead's report is fixed with `id.get(..)` (`type_node`) and pinned by two tests. The prod-scale export contains inherited members for half the instances with no stubs, and the exact-set assertion would fail if any were reported. |
| 6 | Comments, docstrings, runtime-built paths, `__tests__` members, excluded providers produce no entry | VERIFIED | `scan_python` skips `#` and triple-quoted literals, drops alone-on-line statement strings, `+ % .format` neighbours, adjacent strings and f/b prefixes. `parse_literal` refuses `{ } % $ [ ]`, backslash, control chars, empty/`.`/`..` segments. `scanned_member` skips any directory component exactly `__tests__` before reading bytes. Excluded providers (`exclude::effective`, `is_excluded`) are skipped before any export. Tests: `scan_python_docstring_example_is_not_a_reference` (the brief's false positive), `scan_python_runtime_built_paths_are_not_references`, `scan_members_below_a_tests_folder_are_skipped_by_path` (also proves `__tests__x` and `my__tests__` are scanned), `missing_tags_excluded_providers_are_not_exported_or_reported`, `missing_tags_members_below_a_tests_folder_are_not_checked`. |
| 7 | A failed read never reads as clean: `missing_tags_unavailable` with reason, check still succeeds, `missing_tags` null when nothing read, no output says "no missing" | VERIFIED | Four failure paths each return `unavailable` with fixed wording plus `err.code()`: scan error, provider-list error, per-provider export error, export not a tree. `missing = None` when `needed > 0 && read == 0`. Human renderer prints `NOT CHECKED` per entry and prints `tags: no missing tag paths found` only when unavailable, missing and unknown are all empty. Tests: `missing_tags_a_failed_provider_list_is_unavailable_and_still_ok`, `missing_tags_one_failed_export_gives_a_partial_list`, `missing_tags_every_failed_export_reads_nothing`, `missing_tags_an_export_that_is_not_a_tree_is_unavailable`, `projects_check_missing_tags_export_refused_exits_zero_and_says_not_checked` (403 through the real binary: exit 0, null fields, `NOT CHECKED`, no "no missing"), `missing_tags_failed_read_prints_not_checked_and_never_no_missing`, `missing_tags_no_missing_line_needs_every_list_empty`. |
| 8 | 43,000-tag provider vs 2,000-file project: exactly one list and one export per referenced provider, exact planted set, inside a measured budget | VERIFIED (see W1) | `prod_scale_missing_tags` (real `ReqwestGatewayApi`, wiremock): fixture 48,951 tags, 47,808,506 bytes, 2,000 code files plus 45 `__tests__` members. Asserts the unavailable list is empty, `missing == planted_missing` (exactly 12 paths), exactly one unknown provider with `path_count` 5, and `received_requests()` equals exactly `[GET provider-list, GET tags/export]`. `report()` does `assert!(elapsed <= budget)`, so the budget is enforced, not printed. The test can fail: a per-reference call changes the request list, a stub-only lookup reports inherited members, a missed docstring/comment/`__tests__` skip adds entries, a quadratic step exceeds the budget. Timing: 0.74 to 0.80 s in three isolated runs under load average about 170; see W1 for the one over-budget run. |
| 9 | Output and README state "paths built at runtime aren't checked; a tag project code creates at runtime can appear as a warning" | VERIFIED | `MISSING_TAGS_LIMIT` constant carries both phrases plus the other limits; it is in the JSON (`missing_tags_limit`), in every human run (`tags: ...`), and in the warning text. README has a `**Tags the project uses.**` section with the exact phrase (count 1), the null meaning, the round-trip bound, the token requirement, and the "deploy tags first" step. README states cap 200, matching `LIST_CAP`. |
| 10 | Gates and constraints: fmt, clippy, tests, xwin clippy pass. No Cargo/lockfile/golden/slug/exit-code/`error.rs` change. No dotted-quad or production name in committed lines | VERIFIED | See the gates table. Every item re-run by the verifier on HEAD except where noted as W1. |

Score: 10/10 truths verified (the nine behavioural truths from the PLAN frontmatter plus the gates truth).

## Required artifacts and wiring

| Artifact | Status | Details |
|----------|--------|---------|
| `crates/ignition-core/src/deploy/project_tags.rs` | VERIFIED | 1,152 lines, substantive: lexers, `parse_literal`, `TagTree`, 32 unit tests. Registered in `deploy/mod.rs`. Imported and used by `actions/deploy_projects.rs` and `ignition-cli/src/render.rs`. |
| `actions/deploy_projects.rs` (`check_tag_references`, two `ProjectGateway` reads, six result keys) | VERIFIED | Called from `deploy_projects_prepare` after the `first_deploy` gate and before the plan is built; shared by check and apply. |
| `client/mod.rs` `tags_export_provider` | VERIFIED | Default trait body through `api_call`; `ReqwestGatewayApi` override sends `GET` with `.query(provider, type=json)` and `TAGS_EXPORT_TIMEOUT` 300 s via `apply_auth` and `send_and_classify`. Wire pinned by `tags_export_provider_requests_the_native_export_and_returns_the_bytes` and `tags_export_provider_classifies_a_403`. |
| `tests/deploy_prod_scale.rs` `prod_scale_missing_tags` | VERIFIED | Enforced budget and request-list assertion, see truth 8. |
| `ignition-cli/tests/contract_deploy_projects.rs` | VERIFIED | Binary-level JSON, human and exit-code tests; 60 passed. |
| `README.md` | VERIFIED | See truth 9. |

Key links: `deploy_projects_prepare` -> `check_tag_references` (line 1225); `GatewayProjects::tag_export` -> `tags_export_provider`; real client -> `TAGS_EXPORT_TIMEOUT`; `render.rs::tag_lines` -> `MISSING_TAGS_LIMIT`. All WIRED.

Data flow (Level 4): the result fields are populated from `TagCheck`, which comes from the scan of `loaded.artifact.zip` and the gateway reads. No hardcoded or static source. The only static field is `missing_tags_limit`, which is intended.

## Behavioural spot checks (not presence only)

| Behaviour | Evidence | Status |
|-----------|----------|--------|
| Warning-only: three gateway states give equal plan fields | `missing_tags_leaves_the_plan_untouched` passes in the full run | PASS |
| Unreadable tree gives `missing_tags` null plus `missing_tags_unavailable` | `missing_tags_every_failed_export_reads_nothing`, 403 contract test | PASS |
| Round trips counted and bounded | `prod_scale_missing_tags` request-list equality; `missing_tags_500_references_across_two_providers_cost_one_list_and_two_exports` | PASS |
| Budget enforced | `report()` asserts `elapsed <= budget`; the first full run really failed on it (W1) | PASS (enforcement proven by the failure) |
| No panic on multibyte gateway or repo text | two `tree_*` tests and `scan_multibyte_text_around_literals_does_not_panic_and_keeps_the_path` | PASS |

## Probe execution

None declared by the plan and no `scripts/*/tests/probe-*.sh` applies. SKIPPED.

## Requirements coverage

`QUICK-bt0` is the only ID. Not defined in `.planning/REQUIREMENTS.md`; it is a quick-task token, satisfied by the truths above. No orphaned requirements.

## Anti-patterns

None found in changed non-test source: no TODO/FIXME/XXX/TBD/HACK, no `todo!`/`unimplemented!`, no empty handlers, no hardcoded empty data flowing to output. Tests: no `#[ignore]`, `should_panic`, skipped or tautological assertions in the added tests that were read (`project_tags` unit tests, the prod-scale row, `missing_tags_leaves_the_plan_untouched`, the 403 contract test).

## Warnings

### W1. The 3 s prod-scale budget is tight under extreme load (not a goal failure)

In the verifier's first `cargo test --workspace` run, `prod_scale_missing_tags` took 9.00 s against its 3 s budget. The same run also failed two rows this change did not touch (`prod_scale_local_read`: 47.64 s vs 25 s, `prod_scale_pwsh_bundle`: 85.85 s vs 40 s), so the whole `deploy_prod_scale` binary was starved: the machine's load average was in the 60s at the start and about 180 on 10 cores by the end. Re-running the single row three times at load about 170 gave 0.78 / 0.74 / 0.80 s, and a second full workspace run (`--no-fail-fast`) exited 0 with all 3286 tests passing, including all three timing rows.

This is a load-sensitivity warning, not a pass-by-luck: the isolated figures match the SUMMARY's 0.66 to 0.68 s, the code path is unchanged between runs, and the extra time sits in the same test binary as two older rows that failed for the same reason. The 3 s budget is about 4x the isolated time and one third of the 9 s seen when every core was contended. If CI runs `deploy_prod_scale` alongside other heavy tests, expect an occasional red on this row; the plan forbids raising it past 60 s, and a value between 3 s and about 15 s would still fail a per-tag or quadratic regression (which cost minutes). Not a blocker.

## Residual risks (informational, not blockers, no human verification gate)

- R1. No live gateway run was made (by design of this quick task and this verification). The export shapes for a nested-instance stub and for inherited types were not captured on a real 8.3.6 gateway; the lookup treats stubs as optional and falls back to `Unknown` (never reported), so the failure direction is a missed warning and not a false "missing". The lead's live rehearsal is the natural place to confirm.
- R2. Text that looks like a path inside an ordinary (non-comment, non-docstring) string, or a tag created at runtime, can appear as a warning. This is stated in `missing_tags_limit` and the README, as the goal requires.
- R3. The sorted `missing_tags` list is capped at 200 with `missing_tags_count` beside it, as every list in this result already is.

## Gaps summary

No gaps. Every must-have resolves to VERIFIED. The one over-budget timing result is reported as W1 with the re-runs that cleared it.

---

_Verified: 2026-11-06_
_Verifier: Claude (gsd-verifier)_
