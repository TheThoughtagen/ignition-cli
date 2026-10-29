---
phase: 22-transport-plan-pull-verify
plan: 01
subsystem: deploy
tags: [rust, plan-engine, transport, local-fs, sha256, thiserror]

requires:
  - phase: 21-deploy-render
    provides: RenderedTree, render_sha256, Target/Manifest parsing, walk_out, sha256_hex
provides:
  - "local: { data_dir } node block, exclusive with ssh (D-02)"
  - "TreeReader read seam, Scope, ObservedFiles, for_node, LocalFsTransport (caps, hostile-tree refusal)"
  - "Pure plan engine: units, added/changed/same/unmanaged statuses, live_in, core relation, canonical plan_sha256"
  - "load_target / load_declared (extracted from deploy_render), DeployPlanResult, deploy_plan"
  - "transport_failed (exit 4) through the four-place rule"
  - "PlanFixture test harness and tests/deploy_plan.rs"
affects: [22-02, 22-03, 22-04, 22-05, 22-06, 22-07, 22-08, 22-10, 23]

actuals:
  tokens: 21159
  tasks: 2
  commits: 3

tech-stack:
  added: []
  patterns:
    - "Read half of a transport is a trait the verbs receive; the write half is a separate trait no verb can name"
    - "A failed read is never an empty read: only an absent collection directory is empty"
    - "Canonical hash text documented byte-exactly in the module doc, pinned by a golden and an independent in-test recomputation"

key-files:
  created:
    - crates/ignition-core/src/deploy/plan.rs
    - crates/ignition-core/src/deploy/transport/mod.rs
    - crates/ignition-core/src/deploy/transport/local.rs
    - crates/ignition-core/tests/deploy_plan.rs
    - crates/ignition-core/tests/common/plan_fixture.rs
  modified:
    - crates/ignition-core/src/actions/deploy.rs
    - crates/ignition-core/src/deploy/target.rs
    - crates/ignition-core/src/deploy/tree.rs
    - crates/ignition-core/src/deploy/mod.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-core/tests/common/mod.rs
    - README.md

key-decisions:
  - "unit_key never climbs above <module>/<type>; a file with no marker in its own directory joins the nearest ancestor that has one (a tag folder without unary-resource.json joins its provider)"
  - "name is the unit key minus module and type (P/FolderA for tag folders); null for a singleton"
  - "hidden applies to unmanaged entries too (same function), so an unmanaged tag folder with core-only files reports hidden"
  - "deploy_plan.transport is none when a caller supplies a reader for a node with neither block; for_node refuses that case first"
  - "deploy_plan.warnings is the render warnings followed by the plan warnings"

patterns-established:
  - "PlanFixture: new() -> load(env) -> live_render(collection, &declared) -> deploy_plan; layers are written after load because the reader is lazy"
  - "Tag and edge-case engine tests build RenderedTree/ObservedFiles directly against the pure plan() function"

requirements-completed: []
requirements-advanced: [DPLN-01, DPLN-02, DPLN-03]  # partial: library-level engine only; not checked off in REQUIREMENTS.md (see Deviations)

duration: 30min
completed: 2026-10-14
status: complete
---

# Phase 22 Plan 01: Local-transport deploy plan engine Summary

**Read-only `deploy_plan` over a `local: { data_dir }` master: scoped no-follow reads feed a pure plan engine that reports added/changed/same/unmanaged per unit on the effective view, with `live_in`, `core` and a golden-pinned `plan_sha256`; unreadable hosts are `transport_failed` (exit 4).**

## Performance

- **Duration:** about 30 min
- **Started:** 2026-10-14T12:40Z
- **Completed:** 2026-10-14T13:07Z
- **Tasks:** 2 (1 tracer, 1 tdd)
- **Files modified:** 12

## Accomplishments

- Tracer: `load_declared`, `transport::for_node` for `nodes.master`, `LocalFsTransport`, `plan::plan` and `deploy_plan` run end to end over a temp data dir; the render-equals-live case plans empty with `live_in: ["mode"]`.
- Engine semantics pinned: tag folders are units, byte-exact comparison, `unmanaged` never counts, `core` relations incl. `hidden`, `local/` collection warning, deterministic ordering, empty-tree hash.
- Hostile live trees (symlink, special file, non-UTF-8 name, over-cap file, entry flood) are `transport_failed`, never an absent resource. The walk stops at the entry cap rather than enumerating first.
- `transport_failed` landed in all four places in the same commit as the variant, and `DEPLOY_SLUGS` now carries its own exit code per slug.

## Reference values

- **`PLAN_GOLDEN`** = `a89de5043275c1b057289e89cdd78e05bc3d32680ce169e1d885e1a22708c473` (tracer fixture, `dev` mode: POSTGRES config.json one byte changed, `general-alarm-settings` not live; `tests/deploy_plan.rs`). Cross-checked by an in-test recomputation from the documented line format.
- **Moved helpers:** `tree_meta` (private), `require_real_dir` -> `tree::require_repo_dir(repo, rel)`, `read_tree_file` -> `tree::read_repo_file(repo, rel, hint)`, both `pub(crate)` in `deploy/tree.rs`; `walk_out` is `pub(crate)` and now wraps new `walk_capped(.., max_entries)`.
- **`PlanFixture` API** (`tests/common/plan_fixture.rs`): `new()`, `data_dir()`, `collection_dir(c)`, `core(path, bytes)`, `mode(path, bytes)`, `local(path, bytes)`, `live_render(collection, &Declared)`, `read(c, path)`, `remove(c, path)`, `repo(env)`, `repo_with_master(env, master_extra)`, `load(env) -> (Declared, Box<dyn TreeReader>)`; constant `MODE = "dev"`.
- **`DeployPlanResult` key order:** `env, gateway, mode, node, transport, render_sha256, plan_sha256, summary, entries, warnings` (22-03 adds `delete_set`, 22-04 adds `base`).
- **Entry key order:** `key, module, type, name, status, live_in, core, files[{path, live_sha256, desired_sha256}]`.
- **Summary key order:** `added, changed, removed, same, unmanaged, has_changes`.

## Task Commits

1. **Task 1 (tracer): `deploy_plan` end to end over a `local:` master** - `66b8f2c` (feat)
2. **Task 2 RED: pin plan engine semantics** - `db2c894` (test; `hidden_tag_folder` failed as intended)
3. **Task 2 GREEN: hidden relation and capped walk** - `e7fc465` (feat)

## Tracer gate

Auto mode is off and `workflow.human_verify_mode` is `end-of-phase`, so no mid-plan human checkpoint was raised. The tracer's `<verify>` was re-run end to end after the commit (deploy_plan 9/9, error:: 12/12, deploy:: lib, contract_deploy 11/11, deploy_family_coverage 4/4) before the expansion task started. The human check for the working slice is deferred to the phase's end-of-phase verification.

## Files Created/Modified

- `crates/ignition-core/src/deploy/plan.rs` - pure engine, canonical text documented in the module doc
- `crates/ignition-core/src/deploy/transport/{mod,local}.rs` - read seam and local transport
- `crates/ignition-core/src/actions/deploy.rs` - `load_target`, `Declared`, `load_declared`, `DeployPlanResult`, `deploy_plan`
- `crates/ignition-core/src/deploy/target.rs` - `Local`, ssh-xor-local validation
- `crates/ignition-core/src/error.rs`, `README.md` - `transport_failed`, exit 4
- `crates/ignition-core/tests/{deploy_plan.rs,common/plan_fixture.rs}` - 19 tests and the harness

## Decisions Made

See `key-decisions` above. No decision beyond the plan's pre-decided D-02, D-06, D-08, D-17 was taken at a gate.

## Deviations from Plan

None - plan executed as written. Notes on choices the plan left open:

- `Scope::from_manifest` drops manifest `types` entries that name one of the four tag types, because `tag_providers` already covers them restricted to the declared providers.
- Extra tests beyond the plan's list: `plan_missing_config_resources_is_transport_failed`, `plan_empty_data_dir_collections_are_empty_not_errors`, target tests (`local_node_parses`, `ssh_and_local_together_are_refused`, `empty_local_data_dir_is_refused`), `walk_capped_stops_at_the_entry_cap`, and `unit_key` unit tests in `plan.rs`.

**Requirement bookkeeping (judgement call):** `requirements.mark-complete` was run and then reverted. DPLN-01 (ssh half), DPLN-02 (the `ign deploy plan` verb, wired in 22-02) and DPLN-03 (destructive/lockout flags and `shadows_removal`, 22-03) are not satisfied by this library-level plan alone, so REQUIREMENTS.md keeps them unchecked and the later plans that finish them should mark them.

**Total deviations:** 0 code deviations.

## Issues Encountered

None. A first clippy pass flagged `err().expect()` in the new test file (`clippy::err_expect`); switched to `expect_err`.

## Known Stubs

| File | Line | Reason |
|------|------|--------|
| `crates/ignition-core/src/deploy/transport/mod.rs` | `for_node`, `(Some(_), None)` arm | An `ssh` node returns `deploy_tree_invalid` ("the ssh transport is not available in this build yet") until plan 22-06 replaces the arm with `SftpTransport`. Intentional, in-phase sequencing named by the plan. |
| `crates/ignition-core/src/deploy/plan.rs` | `PlanStatus::Removed`, `CoreRelation::ShadowsRemoval` | Variants exist for the serialized vocabulary but nothing produces them until the 22-03 delete set. |

Known stubs are resolved by 22-06 and 22-03; they do not prevent this plan's goal (the local-transport plan). The broken-windows ledger (`.planning/WINDOWS.md`) was not appended: the file does not exist in this repo, so `windows append` is a no-op by contract.

## Threat Flags

None. No new network endpoint or auth path; the only new filesystem surface is the read-only scoped walk covered by T-22-01 to T-22-04 (symlink and special-file refusal, three caps enforced during the walk, read errors never read as absent, entries carry paths and sha256 only).

## Next Phase Readiness

22-02 can wire the verb: `load_declared`, `transport::for_node(.., "nodes.master", ..)`, `deploy_plan` is the exact sequence, and `README`/exit-table agreement for `transport_failed` is already in. `RENDER_GOLDEN` is unchanged.

## Self-Check: PASSED

- Files: `plan.rs`, `transport/mod.rs`, `transport/local.rs`, `tests/deploy_plan.rs`, `tests/common/plan_fixture.rs` all present.
- Commits `66b8f2c`, `db2c894`, `e7fc465` present in `git log`.
- `cargo test -p ignition-core --lib` 632 passed; `--test deploy_plan` 19 passed; `contract_deploy` 11 passed; `deploy_family_coverage` 4 passed; `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` clean.

## Post-review security fixes

Three confirmed findings in this plan's code were fixed: F1 and F2 in `6852f49`, F3 in `1a242dc`.

- **F1, resource-cap bypass.** `walk_capped` (`deploy/tree.rs`) collected every `read_dir` entry of a directory before any cap check, and the local transport's `child_dirs` had no cap. Both now stop as soon as the cap is crossed: `walk_capped` counts entries already held plus entries collected in this directory against `max_entries` while iterating (same "more than N entries (at ...)" message, entries that fit stay sorted by name bytes). `child_dirs` counts every entry read against a counter shared across one `list_types` call, bounded by `READ_MAX_ENTRIES`, and fails with `transport_failed`.
- **F2, TOCTOU symlink follow.** `read_blobs` and `read_repo_file` checked `symlink_metadata` and then opened the path, which follows symlinks. They now open first and validate the handle through the shared `open_regular_file`: the handle must be a regular file and, on unix, share dev+ino with the inspected metadata (mismatch is "{path} changed while it was read"). On windows the open uses `FILE_FLAG_OPEN_REPARSE_POINT` and rejects a symlink or non-file handle. Size caps come from the handle metadata; the `take(READ_MAX_FILE_BYTES + 1)` guard is kept. `read_repo_file` additionally checks every parent component (not only the first) and now caps bytes read at `READ_MAX_FILE_BYTES`.

Tests added (all in `ignition-core --lib`): lazy-iterator test proving the collector stops pulling at the cap, an on-disk flat-directory cap test, `child_dirs` cap test, a deterministic unix inode-mismatch test of `open_regular_file` (stale metadata plus a symlink or replacement file at the path), the `read_blobs` swap path via `open_checked`, and a deep symlinked-parent test for `read_repo_file`.

Verification: `--lib` 638 passed, `--test deploy_plan` 19 passed, `contract_deploy` 11 passed, clippy `-D warnings` and `fmt --check` clean.

- **F3, parent-directory symlink-swap race (closes the F2 residual on unix).** `read_blobs` re-statted with `symlink_metadata(path)` and opened the same full path, and both resolve every intermediate directory, so a directory under `data_dir` swapped for a symlink after the walk made the stat, the open and the dev+ino comparison all agree on a file outside `data_dir`. `open_regular_file` now takes `(root, rel, before)` and on unix opens `root` once as a directory fd, then `openat`s each component of `rel` relative to the previous fd: `O_NOFOLLOW | O_DIRECTORY | O_CLOEXEC` for intermediate directories, `O_NOFOLLOW | O_CLOEXEC` read-only for the final file. `ELOOP` or `ENOTDIR` on any component is "{shown} changed while it was read" (the wording the existing tests expect); other errnos stay I/O errors. `.`, `..`, absolute or empty relative paths are refused the same way. The final fd is `fstat`ed (via `File::metadata`): it must be a regular file, and the existing dev+ino comparison with the inspected metadata is kept; size caps come from that handle, and the `take(READ_MAX_FILE_BYTES + 1)` guard and messages are unchanged. Both callers use it: `read_blobs` through `open_checked`, rooted at `data_dir`, and `read_repo_file`, rooted at the repo. `rustix` 1.1 (`fs`, `std`, no default features) is a `[workspace.dependencies]` entry and a `[target.'cfg(unix)'.dependencies]` entry of `ignition-core`. It was already in `Cargo.lock` through `tempfile`: the `[[package]]` name+version set is identical before and after (220 packages), the only lock change is the `rustix` edge in the `ignition-core` dependency list.

Tests added (all in `ignition-core --lib`): lazy-iterator test proving the collector stops pulling at the cap, an on-disk flat-directory cap test, `child_dirs` cap test, a deterministic unix inode-mismatch test of `open_regular_file` (stale metadata plus a symlink or replacement file at the path), the `read_blobs` swap path via `open_checked`, and a deep symlinked-parent test for `read_repo_file`. F3 adds `read_blobs_refuses_a_parent_directory_swapped_for_a_symlink` (stale path list, parent replaced by a symlink to an outside directory holding a same-named file; it returned the outside bytes before the fix and is refused after) and `open_regular_file_does_not_follow_a_swapped_intermediate_directory` (helper level: swapped deepest and shallower directories with fresh post-swap metadata, symlinked final component, path escapes, absent file stays an I/O error).

Verification: `--lib` 640 passed, `--test deploy_plan` 19 passed, `contract_deploy` 17 passed, `cargo test --workspace` green, clippy `-D warnings` and `fmt --check` clean.

**Not covered:**
- Windows: the intermediate-directory swap race is closed on unix only. The windows branch opens the full path with `FILE_FLAG_OPEN_REPARSE_POINT`, which protects the final component but not intermediate ones, because std has no descriptor-relative open there. It stays a documented residual, and the branch is compiled only on windows (the windows target is not installed for the local toolchain, so it was not even cross-checked) and has no test.
- The directory walk and listings stay path-based: `probe`, `walk_capped` and `child_dirs` still use `symlink_metadata` and `read_dir` on full paths. They only produce names, and every byte read goes through the fd chain, so a swap during the walk cannot return outside bytes. It can still list names from an outside directory: for the names-only `local` collection that means `local_units` could contain outside names, and a swapped directory in the core/mode collections yields paths that the fd-relative read then refuses. Converting the walk to `openat`/`getdents` by fd is the remaining step if that is wanted.
- `root` itself (`data_dir`, or the repo) is opened following a final symlink. `data_dir` being a symlink is refused earlier by `resources_root` (a check-time test); the repo may legitimately be reached through a symlink.
- The swap itself (the race between walk and open) cannot be forced; the unix tests recreate its end state deterministically by passing a stale path list or stale metadata.
