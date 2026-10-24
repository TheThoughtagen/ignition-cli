---
phase: 25-guarded-project-deploy
fixed_at: 2026-10-24T00:00:00Z
review_path: .planning/phases/25-guarded-project-deploy/25-REVIEW.md
iteration: 1
findings_in_scope: 8
fixed: 8
skipped: 0
status: all_fixed
---

# Phase 25: Code Review Fix Report

**Fixed at:** 2026-10-24
**Source review:** .planning/phases/25-guarded-project-deploy/25-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 8 (WR-01 to WR-08; the eight Info items are out of scope)
- Fixed: 8
- Skipped: 0

Verification ran in the main checkout (`workflow.use_worktrees=false`, no worktree), with
`CARGO_TARGET_DIR` set to the session scratchpad. `cargo test --workspace`: 94 result lines, all
`ok`, 2900 passed, 82 ignored (live-rig tests), 0 failed. `cargo clippy --workspace --all-targets
-- -D warnings` and `cargo fmt --all --check` are clean. The four harness scripts
(`projects-test.sh`, `plan-test.sh`, `apply-test.sh`, `readme-test.sh`) pass, and `actionlint`
is clean on all workflows. The source scans `write_half_unreachable` and
`release_writer_is_confined` ran as part of the workspace tests and are green. No new `CoreError`
slug was added.

## Fixed Issues

### WR-01: A UTF-8 BOM (or any non-control format character) in `.ignexclude` silently disables a rule

**Files modified:** `crates/ignition-core/src/deploy/project_artifact.rs`, `README.md`
**Commit:** 3cb8d6f
**Applied fix:** `IgnExclude::parse` now refuses a Unicode format, line or paragraph separator
character (BOM, zero-width space, bidi overrides and the like, via `is_format_char`) with
`deploy_tree_invalid` naming `.ignexclude`, the line and column 1, in the style of the existing
control-character refusal. Test: four new rows in
`ignexclude_unsupported_constructs_refuse_with_the_line` (BOM on line 1 and line 2, U+200B, U+202E).
README grammar sentence updated.

### WR-02: An ambiguous import failure leaves the operator no breadcrumb to the pre-image

**Files modified:** `crates/ignition-core/src/actions/deploy_projects.rs`,
`crates/ignition-core/tests/deploy_projects.rs`, `README.md`
**Commit:** 7a6118e
**Applied fix:** `import_outcome_unknown` classifies a failed import request. A `Network` error
that is not a connect or builder failure (timeout, reset), a `Network` error with no source, and
an unclassified `Internal` answer are "outcome unknown"; the run adds a warning with the error
code, reads the project back and compares, exactly as after a normal answer. Equal gives
`imported` (or `created`) with the record written. Unequal, or a failed read-back, gives
`imported_unverified` with `restore_command` for an overwrite. No second import is ever sent.
A definite refusal (`auth_failed`, `import_denied`, `not_found`, restarting or uncommissioned
gateway) and a connect failure still end the run with that error. Tests (four, in
`tests/deploy_projects.rs`, using a `FlakyImport` gateway wrapper): answer lost after the apply is
read back and `imported`; answer lost and not applied is `imported_unverified` with restore
command and old record kept; answer lost and read-back fails is `imported_unverified` with
restore command; a refusal stays an error with no read-back. Logic fix: requires human
verification, in particular the choice to treat an `Internal` (unclassified HTTP status) answer as
"outcome unknown".

### WR-03: Names the release-record writer cannot carry are only discovered after the import

**Files modified:** `crates/ignition-core/src/actions/deploy_projects.rs`,
`crates/ignition-core/tests/deploy_projects.rs`, `README.md`
**Commit:** 4314604
**Applied fix:** `load_project` (offline, before any gateway or host contact) now runs
`record_names_fit_the_host`: when the master node is `ssh`, the project name and the target
`gateway` must pass `safe_remote_path`, so sftp glob characters (`[ ] { } * ?`), quotes,
backslashes and control characters are refused as `deploy_tree_invalid` naming
`deploy/project.yaml` (project) or the target file (gateway). A `local` node is unaffected.
Test `names_the_ssh_record_writer_cannot_carry_are_refused_up_front` covers project names
(`[ ] { }`; `*` and `?` were already refused by the Windows rules), gateway names (`[ { * ?`), the
same names loading on a `local` node, and an ordinary name on `ssh`.

### WR-04: Release-record temp files are not reliably cleaned up in either writer

**Files modified:** `crates/ignition-core/src/deploy/transport/local_fs.rs`,
`crates/ignition-core/src/deploy/transport/local/project_ops.rs`,
`crates/ignition-core/src/deploy/transport/sftp.rs`,
`crates/ignition-core/src/deploy/transport/sftp/project_ops.rs`
**Commit:** 4ba2e9a
**Applied fix:**
- Local: `Dir::replace_via` writes through a caller-named sibling; the record writer names it
  `release.json.tmp-<run_id>`, as the sftp writer does. `replace` keeps the fixed name for the
  ledger and journal callers.
- Local: the temp file is removed when the write or sync fails (new `write_temp_with`) and when
  the rename fails. A failed removal yields a new `FsError::Leftover`, whose description says
  "the temporary file ... may still be there", so it reaches the `transport_failed` detail.
- sftp: the failed-`put` path now goes through the same helper as the failed-replace path
  (`remove_temp_after`); a failed `rm` is reported with the "may still be there" wording. A `rm`
  answering "not found" after a failed `put` is not a leftover, because the file usually never
  existed.
- Tests: local `local_release_record_write_uses_a_per_run_temporary_name`, `replace_via_uses_the_named_sibling`,
  `a_failed_temp_write_removes_the_temp_file`, `a_temp_file_that_cannot_be_removed_is_reported`;
  sftp `sftp_record_put_failure_removes_the_partial_temp` and
  `sftp_record_put_failure_with_failed_cleanup_is_reported` (the fake gained `put_fails` and
  `rm_denied` knobs). The non-unix (`portable`) variant of `local_fs.rs` was edited the same way
  but could not be compiled here (the Windows target is not installed for this toolchain); it was
  reviewed by eye only.

### WR-05: A log failure after the record is written turns a successful `record` run into an error

**Files modified:** `crates/ignition-core/src/actions/deploy_projects.rs`,
`crates/ignition-core/src/deploy/apply/run_store.rs`
**Commit:** 599d327
**Applied fix:** The record-only branch now ends in `finish_recorded`, which sets
`record_written` and the outcome first, then uses `emit_late` for `RecordWritten` and `Finished`,
and adds the "events log could not be written to the end" warning, matching the import branch.
Test `a_log_failure_after_the_record_is_written_is_a_warning` drives `finish_recorded` with an
event log over a read-only handle (a `#[cfg(test)]` `EventLog::from_file` constructor in
`run_store.rs`). The test exercises the extracted helper rather than a whole run, because no
portable way exists to make an open log fail mid-run.

### WR-06: The approval gate pins the artifact hash but not the targets checkout

**Files modified:** `.github/workflows/deploy-projects.yml`, `README.md`,
`.planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/projects-test.sh`
**Commit:** 78a9fb1
**Applied fix:** The `check` job has a new "Pin targets commit" step (`git -C targets rev-parse
HEAD`, 40 hex required) exposed as job output `targets_sha` and as a reusable-workflow output,
shown as a "targets commit" row in the check summary. The `apply` job checks out
`needs.check.outputs.targets_sha` instead of `inputs.targets-ref`, and a new "Verify targets
commit" step refuses an empty or non-40-hex value or a checkout that is at another commit, before
any secret is used. The `shared:` regions are untouched and byte-identical. Harness additions:
`3e` (static: output present, check reads `inputs.targets-ref`, apply does not, apply reads the
pinned output, both step bodies present), `4d` (runs both extracted bodies against a real git
checkout, including a moved branch and empty, abbreviated, uppercase and non-commit values), and
`report check (i-b)` (the summary names the commit). `deploy-plan.yml` and `deploy-apply.yml` are
separate single-job workflows, so no workflow other than `deploy-projects.yml` has a check and an
apply that both check out targets; nothing else needed the change.

### WR-07: A non-string `parent` in `project.json` is read as "no parent"

**Files modified:** `crates/ignition-core/src/deploy/projects.rs`
**Commit:** 15ff7a8
**Applied fix:** `read_project_meta` matches `parent` explicitly: absent, `null` and `""` are no
parent, a non-empty string is the parent, anything else (number, array, object, bool) is refused
`deploy_tree_invalid` naming `project.json`. Test
`project_meta_refuses_a_parent_that_is_not_a_string` (and the absent and null cases still read as
no parent). The K4 default-row behaviour for an absent `parent` is unchanged.

### WR-08: Flag errors of `deploy projects` get the "fix the input source, pass `--file`" hint

**Files modified:** `crates/ignition-core/src/error.rs`
**Commit:** 33d38c7
**Applied fix:** `--release ` and `--allow-first-change ` prefixes added to the flag branch of the
`InvalidInput` hint. Test: two new rows in `apply_gate_flag_refusals_do_not_get_the_file_hint`.

## Skipped Issues

None.

---

_Fixed: 2026-10-24_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
