---
phase: 21-deploy-render
plan: 09
subsystem: deploy
tags: [render, out, clobber-ladder, render-json, readme, contract, installed-binary]
status: complete

requires:
  - phase: 21-04
    provides: deploy_render action, RenderedTree, render_sha256, tracer repo and RENDER_GOLDEN
  - phase: 21-08
    provides: the complete fail-closed render pipeline (lints, secrets, identity)
provides:
  - "ign deploy render --out DIR: writes the rendered tree byte for byte plus render.json"
  - "deploy::tree::{RENDER_MARKER, RenderMarker, MarkerFile, write_out}: check-then-write clobber ladder"
  - "README --out section and Commands row; contract and stdout-purity tests for --out"
  - "Phase 21 gate evidence: five CI gates and installed-binary smoke"
affects: [22]

tech-stack:
  added: []
  patterns:
    - "Check phase with no writes, then write phase with create_new / temp-plus-rename; no remove calls anywhere"
    - "Write only after render() returned Ok, so every refusal leaves --out untouched"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/deploy/tree.rs
    - crates/ignition-core/src/actions/deploy.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-core/tests/deploy_render.rs
    - crates/ignition-cli/tests/contract_deploy.rs
    - crates/ignition-cli/tests/contract_stdout_purity.rs
    - README.md

key-decisions:
  - "render.json is `{schema_version: 1, gateway, env, mode, render_sha256, files: [{path, sha256, bytes}]}`, pretty JSON plus one LF, deny_unknown_fields, files byte-sorted. Phase 22 (`plan --base`) reads it; a shape change needs a schema_version bump and a reader for both"
  - "The ladder accepts only: absent path, empty directory, or a directory that is exactly a previous render of the same gateway and env (all regular files listed with unchanged sha256, all listed files present, no directory without a listed file, every listed path still produced). Anything else is invalid_input (exit 2) naming the reason and first offending path"
  - "Local I/O failures during the write map to CoreError::Internal, as actions/workspace.rs does; AlreadyExists (a racing writer) maps to invalid_input"
  - "A new path that collides with an existing file or directory, or sits under an existing file, is refused in the check phase (not in the plan text; avoids a half-written refresh)"

requirements-completed: [DRND-01]
requirements-partial: "DRND-05 stays Pending: its three-OS proof is outstanding (see Cross-OS)"

actuals:
  tokens: 14750
  tasks: 2
  commits: 2

duration: 75min
completed: 2026-10-13
---

# Phase 21 Plan 09: `--out` with the clobber ladder, and phase closure Summary

**`ign deploy render --out DIR` writes the exact rendered tree plus a `render.json` marker, refreshes only its own unmodified prior output, refuses everything else before writing a byte, and never deletes; the phase's five CI gates pass locally and the installed binary reproduces `RENDER_GOLDEN`.**

## Accomplishments

- `deploy/tree.rs`: `RENDER_MARKER`, `RenderMarker` (with `for_tree` and `to_bytes`), `MarkerFile`, and `write_out` in two phases. The check phase uses `symlink_metadata` on the out path and every entry (sorted, never followed), then validates the marker (schema_version 1, same gateway and env, sha256 of every file, listed files present and still produced, no stray directory, no collision for new paths). The write phase creates the leaf with `create_dir` (a racing creator gets `AlreadyExists`, refused), opens new files with `create_new`, replaces listed files and finally `render.json` through `<name>.ign-render-tmp` plus rename.
- `actions/deploy.rs`: `deploy_render(repo, env, out)`; `write_out` runs only after `render` returned Ok; `data.out` is the path as given.
- CLI: `--out DIR` on `deploy render`; human mode prints `wrote <files> files to <out>` as the third stdout line.
- README: Commands row now `ign deploy render --env <ENV> [--repo DIR] [--out DIR]`; new `#### Writing the rendered tree (--out)` (ladder, marker fields, never deletes, interrupted writes, MCP safety). Re-read the whole deploy section: fixed three statements that went stale ("writes nothing to disk", `out` "null until `--out` lands", MCP tool described as read-only).
- Tests: 13 `out_*` / `refused_render_writes_nothing` tests in `deploy_render.rs` (tempdirs only, symlink test `#[cfg(unix)]`), 2 in `contract_deploy.rs`, 1 in `contract_stdout_purity.rs`.

## Task Commits

1. Task 1, `--out` ladder, marker, action, CLI: `2647bb2`
2. Task 2, README, CLI contracts, stdout purity: `80c8c8f`

## Gate results (main checkout, no worktree)

| Gate | Result |
|------|--------|
| `cargo fmt --all --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo build --workspace` | clean |
| `cargo build -p ignition-cli --no-default-features` | clean |
| `cargo test --workspace` | exit 0, 1724 passed, 0 failed |

Plan acceptance greps: `grep -nE 'remove_file|remove_dir' crates/ignition-core/src/deploy/tree.rs crates/ignition-core/src/actions/deploy.rs` prints nothing; the README row grep prints 2 (Commands row and section lead); `tui_coverage`, the MCP `catalog_names_match_the_clap_tree_bidirectionally` test and `deploy_family_coverage` (4 tests) pass; `RENDER_GOLDEN` is unchanged.

## Installed-binary smoke

After `cargo install --path crates/ignition-cli --force` (replaced `~/.cargo/bin/ign`):

- `ign deploy render --env dev --repo crates/ignition-core/tests/fixtures/deploy/repos/tracer --json`: `render_sha256` `7c8b41b16aa7680fac4e3cc1b4ac0ba3031973a720f33a625079e408f5b376a1` = `RENDER_GOLDEN`; 4 files, 1763 bytes; `out` null.
- Same with `--out <scratchpad>/smoke/out`: exit 0, human output ended `wrote 4 files to <that path>`; files written: `ignition/database-translator/POSTGRES/{config,resource}.json`, `ignition/general-alarm-settings/{config,resource}.json`, `render.json`; `render.json.render_sha256` = `7c8b41b1...b376a1` (same digest). A second run into the same directory (refresh) exited 0. The scratch directory was removed afterwards.
- `git check-attr text -- crates/ignition-core/tests/fixtures/deploy/repos/tracer/targets/dev.yaml` prints `text: unset`.
- `.github/workflows/ci.yml`: `git status --porcelain` empty; `git log --format=%s -- .github/workflows/ci.yml | grep -c '21-0'` prints 0.

## Cross-OS

cross-OS: pending CI

Reason: not pushed. Local `main` is 81 commits ahead of `origin/main` (head `0bebfa9`), and `git log origin/main -- crates/ignition-core/tests/deploy_corpus.rs` is empty, so no remote commit carries `deploy_corpus.rs`. `gh run list --workflow ci.yml` shows runs (latest `38289285633` on `44b1bf3`, success) but those commits are not in this clone and cannot carry the Phase 21 tests. Pushing is the user's call and was not done; `ci.yml` was not changed. The existing matrix (`.github/workflows/ci.yml` lines 11-17, `os: [ubuntu-latest, macos-latest, windows-latest]`, `cargo test --workspace` at line 42) runs `deploy_fixtures`, `deploy_corpus`, `deploy_render` and `contract_deploy` on every leg, and `.gitattributes` marks `fixtures/deploy/**` as `-text`. `/gsd-verify-work` must treat success criterion 3 (DRND-05 cross-OS byte identity) as unproven until a green run on all three legs for a commit carrying `deploy_corpus.rs` is recorded here.

## Requirement tracking

- **DRND-01: complete.** `ign deploy render` on a `gateways/<name>/` tree for an environment gives a rendered tree (now also writable with `--out`) whose `render_sha256` is identical for identical inputs; verified locally and through the installed binary.
- **DRND-05: stays Pending.** Byte pass-through, the Gson-compatible writer and the corpus round-trip are green locally, but the requirement says the corpus round-trips byte-identical on Linux, macOS and Windows, and no three-leg CI run exists yet.
- DRND-04 and DSURF-01 were already complete (21-04, 21-06); this plan keeps their tests green.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] New paths colliding with existing entries refused in the check phase**
- **Issue:** the plan's check list did not cover a new rendered path equal to, or beneath, an existing unlisted file or directory; the write phase would fail midway with a partial refresh.
- **Fix:** `check_out` refuses it (naming the path) before any write.
- **Files:** `crates/ignition-core/src/deploy/tree.rs`. **Commit:** `2647bb2`.

**2. [Rule 2 - Missing critical] A render holding a path named `render.json` is refused**
- Collection paths always start with `<module>/<type>`, so this cannot occur today; the guard keeps the marker from ever being overwritten by rendered content.

**3. Plan-text stale statements (README)**
- Three README sentences from 21-04 ("writes nothing to disk", `out` "null until `--out` lands", MCP tool "read-only") would have been false after this plan; corrected as the plan's re-read step requires.

**4. TDD note.** As in 21-08, tests and implementation landed in one commit per task (the tests need the new signature to compile); tests were run green against the implementation before commit. One test bug was fixed on the first run (the "drops a file" test must also remove the source directory, because un-allowlisted type directories are refused by D-02).

**5. Test count.** The plan names ten ladder tests; thirteen landed because the extra-file / missing-file / stray-directory cases and the second-writer case got their own tests. `out_second_writer_is_refused_and_nothing_is_replaced` exercises the check-phase refusal of a competitor's unmarked output; the `create_dir` / `create_new` race itself cannot be forced deterministically in a test and rests on the std semantics (T-21-39 is accepted in the threat model).

## Known Stubs

None.

## Threat Flags

None beyond the plan's register. T-21-37 and T-21-38 are covered by the ladder tests, the symlink test and the no-remove grep; T-21-41 by `refused_render_writes_nothing` (secret and policy refusals, absent / empty / deep-parent targets); T-21-39 and T-21-40 are accepted as planned.

## Phase-level notes for the verifier

- DRND-07 residual (plaintext secrets under non password/secret key names in undescribed module types are not detected) is recorded in the 21-08 SUMMARY and README Secrets; treat as a known limit.
- Disk space on the build volume was at 100% (513 MiB free) after the release install; nothing was deleted other than the smoke scratch directory.

## Self-Check: PASSED

- FOUND: crates/ignition-core/src/deploy/tree.rs (write_out, RenderMarker)
- FOUND: commits `2647bb2` and `80c8c8f` in `git log`
