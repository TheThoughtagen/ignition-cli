---
phase: 25-guarded-project-deploy
reviewed: 2026-10-23T22:30:00Z
depth: standard
files_reviewed: 38
files_reviewed_list:
  - .github/workflows/deploy-apply.yml
  - .github/workflows/deploy-plan.yml
  - .github/workflows/deploy-projects.yml
  - README.md
  - crates/ignition-cli/src/cli.rs
  - crates/ignition-cli/src/main.rs
  - crates/ignition-cli/src/mcp.rs
  - crates/ignition-cli/src/render.rs
  - crates/ignition-core/src/actions/deploy.rs
  - crates/ignition-core/src/actions/deploy_apply.rs
  - crates/ignition-core/src/actions/deploy_projects.rs
  - crates/ignition-core/src/actions/mod.rs
  - crates/ignition-core/src/actions/projects.rs
  - crates/ignition-core/src/actions/rig.rs
  - crates/ignition-core/src/client/projects.rs
  - crates/ignition-core/src/deploy/apply/preflight.rs
  - crates/ignition-core/src/deploy/apply/run_store.rs
  - crates/ignition-core/src/deploy/ledger.rs
  - crates/ignition-core/src/deploy/mod.rs
  - crates/ignition-core/src/deploy/project_artifact.rs
  - crates/ignition-core/src/deploy/projects.rs
  - crates/ignition-core/src/deploy/target.rs
  - crates/ignition-core/src/deploy/transport/local.rs
  - crates/ignition-core/src/deploy/transport/local/project_ops.rs
  - crates/ignition-core/src/deploy/transport/local_fs.rs
  - crates/ignition-core/src/deploy/transport/mod.rs
  - crates/ignition-core/src/deploy/transport/projects.rs
  - crates/ignition-core/src/deploy/transport/remote.rs
  - crates/ignition-core/src/deploy/transport/sftp.rs
  - crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs
  - crates/ignition-core/src/deploy/transport/sftp/project_ops.rs
  - crates/ignition-core/src/deploy/transport/winps.rs
  - crates/ignition-core/src/error.rs
  - crates/ignition-core/tests/fixtures/live-rig-deploy/capture25.sh
  - crates/ignition-core/tests/fixtures/live-rig-deploy/compose.projects.yml
  - crates/ignition-core/tests/fixtures/live-rig-deploy/mint-token.sh
  - crates/ignition-tui/src/routes.rs
  - crates/ignition-tui/src/update.rs
findings:
  critical: 0
  warning: 8
  info: 8
  total: 16
status: issues_found
---

# Phase 25: Code Review Report

**Reviewed:** 2026-10-23
**Depth:** standard
**Files Reviewed:** 38 (test files under `crates/*/tests/*.rs` were not reviewed; the in-source unit tests were read where they pin behaviour)
**Status:** issues_found

## Summary

Phase 25 is, on the whole, carefully built. The fail-closed `.git` probe holds on all three paths I traced (local `symlink_metadata`, POSIX `stat` script, Windows `Get-Item -Force`): only a clean "no such entry" is `Absent`, every other failure is `transport_failed`, a symlink or junction is reported and never followed, and project/gateway names pass `component_problem` (plus the Windows rules) before any path, argv or PowerShell parameter is built. Artifact building refuses symlinks at every level of the walk and re-checks them in `read_repo_file`. The human renderer escapes every repo-, gateway- and host-derived string it prints. The CI split holds: the `check` job references only `IGN_CHECK_TOKEN`, `IGN_CHECK_SSH_KEY` and `IGN_TARGETS_TOKEN`, never `IGN_MASTER_TOKEN` or `IGN_SSH_KEY`. The exit-code and slug contract is consistent across `error.rs`, `DEPLOY_SLUGS`, the README exit table and the workflow classifier. The 86 phase 25 unit tests in `ignition-core` pass (`cargo test -p ignition-core --lib`, filtered to the phase 25 modules).

No BLOCKER was found. The warnings below are real defects in error paths, validation ordering and one silently ignored operator control. The accepted items listed in the review brief (Windows `Move-Item -Force` atomicity, in-memory zip, verbatim `import_denied` text, ungated plan job mapping `IGN_SSH_KEY`) are not re-raised.

## Warnings

### WR-01: A UTF-8 BOM (or any non-control format character) in `.ignexclude` silently disables a rule

**File:** `crates/ignition-core/src/deploy/project_artifact.rs:181-232`
**Issue:** `IgnExclude::parse` decodes with `str::from_utf8`, which keeps a leading U+FEFF, then trims only space and tab and rejects only `char::is_control()` characters. U+FEFF (and U+200B, U+202E and the like) are category Cf, not Cc, so they pass. A `.ignexclude` saved by a Windows editor with a BOM therefore parses "successfully", but its first rule has the pattern `"\u{feff}ignition"` as its first segment, which can never match. The documented grammar only promises CRLF tolerance, so this is an input the parser neither supports nor refuses. The failure mode is fail-open: a path the author listed to keep out of the artifact is deployed (provided a `resource.json` lists it), and nothing in `excluded` or the warnings says a rule matched nothing. A zero-match rule is not reported at all.
**Fix:** Strip one leading BOM before splitting, and refuse any Unicode format/separator character in a rule the way control characters are refused:
```rust
let text = std::str::from_utf8(bytes).map_err(|_| invalid(IGNEXCLUDE_FILE, "is not valid UTF-8"))?;
let text = text.strip_prefix('\u{feff}').unwrap_or(text);
// ...
if line.chars().any(|c| c.is_control() || is_format_or_separator(c)) {
    return Err(at("holds a control or format character"));
}
```
Consider also adding a warning to the result for a rule that matched nothing, so a typo is visible in `--check`.

### WR-02: An ambiguous import failure leaves the operator no breadcrumb to the pre-image

**File:** `crates/ignition-core/src/actions/deploy_projects.rs:1529-1532`
**Issue:** `gateway.project_import(...).await?` returns `Err` for any transport-level failure. The import is synchronous and has a 300 s client budget (`PROJECT_IMPORT_TIMEOUT`). A timeout or connection reset after the gateway has begun applying the zip is indistinguishable from a refusal, but the code treats it as "nothing happened": no read-back, no `imported_unverified` verdict, and no `restore_command`. `result.json` records only the slug and run id, and the CLI's error (`network_error`, exit 4) names neither the run directory nor the saved pre-image. The module's own rule (K11) is that after the gateway has changed, the work that is left (verify, report) must still happen. The pre-image exists on disk in `export/<project>.zip`, but nothing points to it, and in CI the apply job's uploaded records exclude it by design.
**Fix:** Distinguish "request definitely not applied" (the gateway answered an error, `import_denied`) from "outcome unknown" (timeout, transport error, 5xx). For the unknown class, fall through to the read-back and the existing `imported_unverified` path (which already builds `restore_command`), or at minimum attach `run_id` and the restore command to the error:
```rust
let answer = match gateway.project_import(project, zip, overwrite).await {
    Ok(a) => a,
    Err(e) if e.is_outcome_unknown() => { /* verify by read-back, then unverified verdict */ }
    Err(e) => return Err(e),
};
```

### WR-03: Names the release-record writer cannot carry are only discovered after the import

**File:** `crates/ignition-core/src/deploy/projects.rs:69-73`, `crates/ignition-core/src/deploy/transport/sftp/project_ops.rs:462-471`, `crates/ignition-core/src/actions/deploy_projects.rs:1203-1215`
**Issue:** `run()` builds the record writer first "so a node that cannot take a record refuses before anything is imported", but building does not validate the project or gateway name. `ProjectFile::validate` applies `component_problem` and `windows_component_problem`, which allow `[`, `]`, `{` and `}`. The ssh writer's `SftpOp::Mkdir/Put/Rm` lines are built through `safe_remote_path`, which refuses exactly those characters as sftp globs (`remote.rs:68-91`). For a project that has no directory in the home yet (the first deploy), the read in `prepare` lists only the parent, succeeds with `None`, the import runs, and the record write then fails with `transport_failed`. The run ends `imported` with `record_written: false` and only a warning. Every later deploy of that project then hits the `first_deploy` gate again, because no record can ever be written. The same applies to a target `gateway` containing those characters.
**Fix:** Reject, in `ProjectFile::validate` and in the target's gateway check, names that fail `safe_remote_path` for any ssh master, or run the writer's path/name check in `release_writer_for_node` / at the top of `deploy_projects_run` (before `wrong_target_check`), for example by exposing a `ReleaseRecordWriter::preflight(gateway, project)`.

### WR-04: Release-record temp files are not reliably cleaned up in either writer

**File:** `crates/ignition-core/src/deploy/transport/sftp/project_ops.rs:509-514`, `crates/ignition-core/src/deploy/transport/local_fs.rs:270-279` and `:540-549` (used by `local/project_ops.rs:169`)
**Issue:** Three related gaps.
1. sftp, failed `put`: the cleanup is `let _ = self.run_batch(... Rm temp ...)`. If the `put` left a partial file and the `rm` also fails, the leftover is neither reported nor retried. The temp name carries the run id, so every run uses a fresh name and no later run ever removes it (the walk only refuses when the same name exists). Files accumulate silently in the deploy account's home. The replace-failure branch does report "may still be there"; the put-failure branch does not.
2. Local: `write_temp` creates the temp and then `write_all` / `sync_all` can fail (disk full), returning with the temp file left behind. Only the later `rename_at` failure unlinks it. The next write removes a file of that name, so it is bounded, but it is not cleaned up on the failing run.
3. Local: the temp name is the fixed `release.json` + `TEMP_SUFFIX`, not per run. Two concurrent runs for one project on the same data directory can have run B unlink run A's temp and create its own while A still holds the old descriptor, so `release.json` can briefly be renamed in while B is still writing it. This contradicts the `ReleaseRecordWriter` doc claim that a reader never sees half a record. (The sftp writer is unaffected, its temp names are per run.)
**Fix:** Report a failed cleanup after a failed `put` the same way the replace branch does. In `write_temp`, unlink the temp on any error after it is created. For the local writer, derive the temp name from the record's `run_id`, as the sftp writer does.

### WR-05: A log failure after the record is written turns a successful `record` run into an error

**File:** `crates/ignition-core/src/actions/deploy_projects.rs:1484-1499`
**Issue:** In the `ProjectAction::Record` branch the record is written and then `events.emit(ProjectsEvent::RecordWritten)?` propagates a log failure as an error. The module states that once the gateway/host has changed, a log failure must only be remembered and surfaced as a warning (`emit_late`, `RunEvents` docs, and the import branch does exactly that). Here the record exists on the host, but `result.record_written` is never set and the process reports failure, so the operator is told a deploy failed that changed state.
**Fix:** Use `events.emit_late(ProjectsEvent::RecordWritten)` and set `result.record_written = true` before emitting, as the import branch does.

### WR-06: The approval gate pins the artifact hash but not the targets checkout

**File:** `.github/workflows/deploy-projects.yml:69-72`, `:170-178`, `:587-595`
**Issue:** `targets-ref` defaults to `main`, and the `check` and `apply` jobs each check it out independently. The reviewer approves a check that read `targets/<env>.yaml` at time T1; the apply job, possibly hours later, reads whatever `main` is at T2. The target file decides the ssh host, `data_dir`, `known_hosts` pin, `gateway` directory name and `gateway_name` used by the wrong-target check, so a change between check and approval changes where the approved artifact lands, while the approver sees an unchanged `artifact_sha256`. The description says "Pin a tag or commit for a reproducible deploy", but the safe behaviour is opt-in.
**Fix:** Resolve `targets-ref` to a commit SHA once in the `check` job (`git -C targets rev-parse HEAD`), expose it as a job output, show it in the check summary, and make the apply job check out that SHA. Alternatively default `targets-ref` to a required input with no default.

### WR-07: A non-string `parent` in `project.json` is read as "no parent"

**File:** `crates/ignition-core/src/deploy/projects.rs:152-156`
**Issue:** `object.get("parent").and_then(|v| v.as_str()).filter(|s| !s.is_empty())` turns `"parent": 5`, `"parent": []` or `"parent": {}` into `None`, so the K6 parent checks (missing, cycle, not inheritable) are skipped. The artifact still ships that `project.json` verbatim, and `canonical_project_json` only normalises an absent or null `parent`. The deploy then proceeds past a gate it was meant to enforce, and what the gateway does with a malformed parent is not captured.
**Fix:** Refuse a present `parent` that is not a string or null:
```rust
let parent = match object.get("parent") {
    None | Some(serde_json::Value::Null) => None,
    Some(serde_json::Value::String(s)) if s.is_empty() => None,
    Some(serde_json::Value::String(s)) => Some(s.clone()),
    Some(_) => return Err(CoreError::deploy_tree_invalid(PROJECT_JSON_REL, None, None, "`parent` must be a string")),
};
```

### WR-08: Flag errors of `deploy projects` get the "fix the input source, pass `--file`" hint

**File:** `crates/ignition-core/src/error.rs:1449-1486`, with the errors raised at `crates/ignition-core/src/deploy/projects.rs:213-216` and `crates/ignition-core/src/actions/deploy_projects.rs:292-301`
**Issue:** The `InvalidInput` hint has an allow-list of reason prefixes (`--plan-sha256 `, `--allow-drift `, `--artifact-sha256 `, `deploy projects needs --artifact-sha256`) for which the default "pass a readable file via `--file`" hint is replaced. Two flag errors this phase adds are not in the list: `--release <tag>: not a semver tag ...` and `--allow-first-change <reason>: the reason is 1 to 500 characters`. Both therefore carry the `--file`/stdin hint, which is wrong for a flag value and is exactly the case the comment at that site says the list exists to avoid. The hint is part of the agent-facing JSON error envelope.
**Fix:** Add `reason.starts_with("--release ") || reason.starts_with("--allow-first-change ")` to the flag branch (and a test row like the existing ones).

## Info

### IN-01: Escaping covers Unicode category Cc only

**File:** `crates/ignition-cli/src/render.rs:1564-1587`
**Issue:** `escape_control` and `escape_error_text` use `char::is_control()`, which is Cc only. Format and separator characters (bidi overrides U+202A-U+202E and U+2066-U+2069, U+2028/U+2029, U+200B/U+200D, U+FEFF) pass through. A repo path or gateway name can use U+202E to make an `excluded` or `removed` entry read as a different name. This is spoofing rather than cursor control, but the stated purpose is that host- and repo-derived names cannot mislead the operator.
**Fix:** Also escape `c.is_control()` or any char whose general category is Cf, Zl or Zp (for example via the `unicode-general-category`-free check `matches!(c, '\u{200b}'..='\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}')`).

### IN-02: README table row for `deploy projects` is stale

**File:** `README.md:504`
**Issue:** The row says a deploy goes to the master node only "and, for now, to a node declared with `local`". The ssh writer and probe shipped in 25-08/25-13, and the README's own sections (around lines 3374 and 3847) describe ssh as supported. The row's `JSON data {...}` key list also omits `children`, which the module doc, the sections around line 3892 and the result struct all carry. The README is the agent contract, so the row misleads on both points.
**Fix:** Drop the "for now ... `local`" clause and add `children` between `restore_command` and `outcome`.

### IN-03: The reserved mode name `projects` is matched case-sensitively

**File:** `crates/ignition-core/src/deploy/target.rs:172`
**Issue:** `self.mode == "projects"` reserves the directory `ign-deploy/<gateway>/projects/`. On a Windows ssh node, and on a `local` node on macOS or Windows, `Projects` names the same directory, so a mode called `Projects` would put its ledger next to the release records the reservation exists to keep apart.
**Fix:** Compare with `eq_ignore_ascii_case("projects")`.

### IN-04: Test seam on the production options struct

**File:** `crates/ignition-core/src/actions/deploy_projects.rs:1016-1018`
**Issue:** `ProjectsRunOptions::record_writer` is `pub` with only `#[doc(hidden)]`. Any library caller can substitute the writer that the module's own doc says "the CLI always passes `None`" for, and `release_writer_is_confined` does not see it (it scans for the type name, not field use).
**Fix:** Gate the field behind `#[cfg(any(test, feature = "test-support"))]` or pass it through a separate `run_with_writer` entry used only by tests.

### IN-05: Wrong-target and `.git` consistency are weaker in `--check` than in the apply

**File:** `crates/ignition-core/src/actions/deploy_projects.rs:679-686` and `:1135-1160`
**Issue:** `--check` runs the `.git` probe and the record read against `nodes.master` but not the wrong-target comparison (REST name against the host's `systemName`), which only the apply runs, after the probe, the record read and the Skip/Record/Import decision. The CI check job's "no `.git`" and `action` therefore come from a host that was never tied to the gateway it reads. A cheap extra guard: a project the gateway lists whose directory is `Absent` on the probed host (`project: "none"`) is evidence of a wrong `data_dir`, and is currently reported as a clean "no git". Also, nothing re-probes between the Designer wait (up to 86400 s) and the import.
**Fix:** In `prepare`, when `project_find` returns a record and `git_entry` returned `Absent` because the project directory itself is missing, add a warning (or refuse under apply). Optionally re-run `git_entry` immediately before `project_import`.

### IN-06: Shared and mutable-tag CI inputs

**File:** `.github/workflows/deploy-projects.yml:136`, `:175`; `.github/workflows/deploy-plan.yml` (targets steps); all three workflows (`actions/checkout@v7`, `actions/github-script@v9`, `actions/upload-artifact@v7`)
**Issue:** The `check` job (and the ungated plan job) now receive `IGN_TARGETS_TOKEN`, a secret that is not one of the "check" secrets; it is read-only and scoped to the targets repository, so this is by design, but it widens what an ungated pre-approval job can read (host names, known_hosts pins, data_dir paths). Third-party actions are pinned to mutable major tags, not commit SHAs, in a workflow that handles gateway write tokens. The install step verifies the release checksum fetched from the same release as the archive, which detects corruption but not a tampered release.
**Fix:** Document `IGN_TARGETS_TOKEN` as part of the check surface, or split it as `IGN_CHECK_TARGETS_TOKEN`. Pin actions by SHA. Consider a detached signature or a pinned checksum in the caller for `ign-version`.

### IN-07: Digest correctness depends on `serde_json` keeping its `BTreeMap` map

**File:** `crates/ignition-core/src/deploy/project_artifact.rs:328`
**Issue:** The canonical form relies on the comment "`serde_json::Map` is a BTreeMap here, so keys stay sorted". Today no crate in the tree enables `preserve_order` (checked with `cargo tree -e features -i serde_json`), but one transitive dependency enabling it would change key order, the live and artifact digests would diverge for any descriptor whose keys are ordered differently, and every deploy would plan `import` and verify as `imported_unverified`, with no code failing at build time.
**Fix:** Add a unit test that parses two descriptors with the same keys in a different order and asserts equal canonical bytes, so a feature change fails CI.

### IN-08: The secret-bearing pre-image persists on self-hosted runners

**File:** `crates/ignition-core/src/actions/deploy_projects.rs:1507-1516`, `.github/workflows/deploy-projects.yml:851-890`
**Issue:** The workflow carefully refuses to upload `export/` and zips, which is right. On a self-hosted runner the 0600 pre-image `project/.ign/deploy/<gw>/<id>/export/<project>.zip` (which can contain script-embedded credentials) stays in the workspace after the job until a later checkout cleans it, and the `apply` job has no post-step that removes it, so on a shared runner it outlives the approval.
**Fix:** After the upload step add an `if: always()` step that removes `project/$IGN_REPO_DIR/.ign/deploy/*/*/export` (or the whole run directory once `result.json` is staged).

---

_Reviewed: 2026-10-23_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
