---
phase: 21-deploy-render
plan: 08
subsystem: deploy
tags: [render, lint, secrets, windows, drnd-07, drnd-08]
status: complete

requires:
  - phase: 21-01
    provides: OpenAPI 8.3.6 extract (secret_fields, types) and the sanitized corpus
  - phase: 21-05
    provides: parsed-node side table (stage 5)
  - phase: 21-07
    provides: TYPE_RULES and the resource-identity stage 6
provides:
  - "deploy::lint: scan_embedded, scan_secrets, SecretScan, lint_files, lint_tree, windows_prefix_len, is_text_member and the Windows consts"
  - "deploy::rules: SecretFields, SECRET_FIELDS (10 types), secret_paths, OPENAPI_TYPES (56), openapi_describes"
  - "render stage 7 (secret half and tree half) and the single final raise"
  - "README Secrets and Windows safety and line endings"
affects: [21-09]

tech-stack:
  added: []
  patterns:
    - "SecretScan carries only location strings, so a value cannot reach any output"
    - "Secret table and type list pinned to the committed OpenAPI extract by tests"

key-files:
  created:
    - crates/ignition-core/src/deploy/lint.rs
  modified:
    - crates/ignition-core/src/deploy/mod.rs
    - crates/ignition-core/src/deploy/rules.rs
    - crates/ignition-core/src/deploy/render.rs
    - crates/ignition-core/tests/deploy_render.rs
    - crates/ignition-core/tests/deploy_corpus.rs
    - crates/ignition-cli/tests/contract_deploy.rs
    - README.md

key-decisions:
  - "Email-profile secret path is settings.settings.password (SECRET_FIELDS pinned to the extract)"
  - "Tag-origin files get the Embedded scan only; schema-path and key-name rules need a resource type and apply to resource config.json files only"
  - "The key-name rule is limited to config.json at the resource root, as the plan's must_haves say"
  - "A Referenced object needs an object data, like Embedded; Referenced data is exempt from the key-name rule (secretName is a name, CD7)"
  - "An empty string at an OpenAPI secret path is refused as plaintext (plan: string or any other value); an empty string under a key-name hit passes"
  - "CR findings carry line and column so DeployFinding Display prints file:line:column"

patterns-established:
  - "Findings and warnings name <source file>: <json path> (<kind>), never a value"
  - "One final raise: secret locations win and every other finding rides along in embedded_secret"

requirements-completed: [DRND-07, DRND-08]

actuals:
  tokens: 19200
  tasks: 2
  commits: 2

duration: 55min
completed: 2026-10-13
---

# Phase 21 Plan 08: Fail-closed lints (secrets, Windows safety, CR) Summary

**Render stage 7 refuses Embedded and plaintext secrets by JSON path (schema paths for 10 OpenAPI types, `password`/`secret` key names in undescribed module types), Windows-illegal names, case-fold collisions, over-budget paths and CR in text members, all in one pass and without ever printing a value.**

## Accomplishments

- `deploy/rules.rs`: `SECRET_FIELDS` (10 rows), `secret_paths`, `OPENAPI_TYPES` (56 pairs), `openapi_describes`; tests `secret_fields_agree_with_openapi_extract` and `openapi_types_equal_extract` compare against the extract.
- `deploy/lint.rs` secret half: `scan_embedded` (any object with `"type": "Embedded"` and an object `data`, any depth, any `.json`), `scan_secrets` (schema paths: absent, null or Referenced pass; key-name rule in `config.json`: refuse in undescribed types, warn in described types).
- `deploy/lint.rs` tree half: `lint_files`/`lint_tree`, `windows_prefix_len`, `is_text_member`, constants `WINDOWS_MAX_PATH` (248), `WINDOWS_MAX_COMPONENT` (255), `DEFAULT_WINDOWS_DATA_DIR`, `COLLECTION_SEGMENT`, `RESERVED_STEMS`, `TEXT_EXTENSIONS`. Component rules, sibling case-fold groups, the UTF-16 budget (247 passes, 248 refused), CR in text members only (jars exempt), and `target.mode` held to the component rules (finding on `targets/<env>.yaml`).
- `render.rs` stage 7: `scan_all_secrets` over the parsed-node table and `lint_files` over the final tree join the shared collector; the final raise is `embedded_secret(paths, other_findings)` when any secret exists, else `deploy_findings`, else Ok. Warnings travel into `RenderOutcome.warnings`.
- README `#### Secrets`, `#### Windows safety and line endings`, and three extra bullets in `#### What render refuses`.

## Task Commits

1. Task 1, secret lint: `ce6e617`
2. Task 2, Windows names, case folds, budget, CR: `7df5759`

## Test evidence

- Canary tests (`CANARY-EMBEDDED-21`, `CANARY-PLAINTEXT-21`, `CANARY-KEYNAME-21`, `CANARY-WARN-21`): absent from `err.to_string()`, `{err:?}`, warnings, CLI stdout and stderr in human and `--json` modes (`embedded_secret_names_path_not_value`, `possible_secret_warning_names_path_not_value`).
- `corpus_embedded_files_are_refused_by_path`: all 10 Embedded corpus resources refused, expected paths computed independently with serde_json, `SANITIZED` never in Display or Debug.
- `corpus_render_is_byte_identical` unchanged and now also asserts no warnings; `render_digest_golden` (`RENDER_GOLDEN`) unchanged.
- Boundaries: component 255/256 (non-BMP counts 2), path budget 247/248 with default prefix, with a longer Windows `data_dir`, with a trailing separator, with a backup node, and ignoring Linux nodes. Two proptests (`prop_illegal_char_names_refused`, `prop_legal_names_pass`).
- Full gates: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`, `cargo build -p ignition-cli --no-default-features`, `cargo test --workspace` all clean. Determinism grep over `deploy/`: only the 21-03 test-only guard at `yamlio.rs:448`.

## Requirement tracking

- **DRND-08** (case-fold collisions, Windows-illegal names, over-budget paths, CR bytes): fully delivered and tested. Marked complete.
- **DRND-07** (Embedded or plaintext secret field refused, naming the path, never the value): delivered for Embedded anywhere, plaintext at the exact OpenAPI path of the 10 schema-described types, and key-name `password`/`secret` strings in undescribed types. Marked complete, with the plan's flagged residual risk carried forward (below). The verifier should read that residual as a known limit, not a defect.

## Flagged assumption (DRND-07 residual risk, from the plan)

In module types the 8.3.6 OpenAPI does not describe, a plaintext secret under a key name other than `*password*`/`*secret*` (`apiKey`, `token`, `privateKey`, `credentials`) is not detected, and a non-secret string field whose name contains `password` or `secret` is refused with no Phase 21 override. The corpus has no such string value. Documented in README `#### Secrets`. Status: unresolved by design; `/gsd-verify-work` should surface it.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] Tag-origin files bypass resource-type rules**
- **Issue:** the plan names `scan_secrets(origin, key, file_name, node)` for every parsed node, but tag-origin files (CD5) have no resource type: `openapi_describes` is false for the tag types, so the key-name rule would refuse tag content that merely has a `password`-like key.
- **Fix:** added `scan_embedded` (generic Embedded rule only) and render uses it for tag-origin files; the name, path-budget, case-fold and CR lints do apply to tag files through their mapped collection path (tested by `lints_apply_to_tag_files_and_every_environment`).
- **Files:** `lint.rs`, `render.rs`. **Commit:** `ce6e617`.

**2. [Signature] `lint_files` and an `env` argument**
- The plan lists `lint_tree(&RenderedTree, &Target, origins)`. Render holds the files as a `BTreeMap` before the tree is built, so `lint_files(&BTreeMap, ...)` does the work and `lint_tree(&RenderedTree, ...)` delegates. Both take `env` (for the `targets/<env>.yaml` mode finding) and a `label` closure instead of the private `Origins` map. No behavior change.
- **Commit:** `7df5759`.

**3. [Rule 2 - Missing critical] CR findings carry line and column**
- `DeployFinding`'s Display prints a position only when both line and column are set, so a line-only finding would lose the line. The CR finding sets both (`file:line:column`) and the message repeats the line.

**4. TDD note.** Tests and implementation landed together per task (the tests need the new API to compile); each task's tests were run green against its implementation before the commit. No separate failing-test commits.

**5. Test-file placement.** The 13 DRND-08 unit tests and the two proptests live in `lint.rs` (the plan's target); `cr_in_jar_is_not_refused` and `all_lint_findings_reported_together` are in `deploy_render.rs` as planned. Extra render-level tests added: `cr_in_text_member_is_refused_through_render`, `empty_member_and_empty_tree_edges`, `lints_apply_to_tag_files_and_every_environment`, `windows_node_data_dir_lowers_the_render_budget`, `embedded_in_any_json_member_is_refused`, `key_name_rule_is_limited_to_config_json`, `secret_findings_win_and_carry_other_findings`.

## Issues Encountered

None blocking. A first draft of two lint unit tests mixed up YAML single-quote escaping and counted the path budget in a component-length test; both were test bugs and were corrected before commit.

## Known Stubs

None.

## Threat Flags

None. T-21-32 and T-21-33 are covered by the canary and corpus tests, T-21-34 by `plaintext_by_key_name_in_undescribed_type_is_refused` with the residual recorded above, T-21-35 by the boundary tests and proptests, T-21-36 by the CR tests with the jar exemption.

## Self-Check: PASSED

- FOUND: crates/ignition-core/src/deploy/lint.rs
- FOUND commits: ce6e617, 7df5759
