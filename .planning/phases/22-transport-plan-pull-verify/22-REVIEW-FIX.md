---
phase: 22-transport-plan-pull-verify
fixed_at: 2026-10-15T00:00:00Z
review_path: /Users/pmannion/whiskeyhouse/ignition-cli/.planning/phases/22-transport-plan-pull-verify/22-REVIEW.md
iteration: 1
findings_in_scope: 7
fixed: 7
skipped: 0
status: all_fixed
---

# Phase 22: Code Review Fix Report

**Fixed at:** 2026-10-15
**Source review:** .planning/phases/22-transport-plan-pull-verify/22-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 7 (CR-01, WR-01..WR-06; Info excluded by `fix_scope`)
- Fixed: 7
- Skipped: 0

**Verification (ran in the main checkout, no worktree, `CARGO_TARGET_DIR=~/Library/Caches/cargo-target-ign-cli`):**
- `cargo test --workspace`: 2141 passed, 0 failed, 50 ignored (live-gated)
- `cargo clippy --workspace --all-targets -- -D warnings`: clean
- `cargo fmt --check`: clean
- `readme_exit_table_agreement` and `exit_code_mapping_enumerated` stay green; no slug or exit code was added or changed.
- `PLAN_GOLDEN` and `PLAN_GOLDEN_LEDGER` are untouched: no fix changes plan output.

## Fixed Issues

### CR-01: `deploy pull` deletes the existing tree before validating or writing the new one

**Files modified:** `crates/ignition-core/src/actions/deploy.rs`
**Commit:** 83a3998
**Applied fix:** New `validate_pulled` runs before the first delete. It refuses an unsafe path component, two paths that collide under `to_lowercase`, and a path that is both a file and a directory. `write_pulled` is now a thin wrapper (validate, then `write_pulled_fs`; the unix and non-unix bodies were renamed), so both platform twins are covered. Four unit tests (new `tests` module in `actions/deploy.rs`) drive `write_pulled` against an existing tree and assert the tree is byte-identical after each refusal. Three of them failed before the fix.

Review claim adjusted: the case-collision trigger named in the review is already caught upstream. `lint.rs` (sibling case-fold check in render) skips such units as `render_refused`, so a sibling collision never reaches `write_pulled` through the real action. I confirmed this with a failing integration probe, then removed it. The ordering defect was real for everything render does not catch, so the validate-first guard stays as defense in depth.

Residual (not fixed): the staging-directory-plus-rename swap from the review's step 2 was not done. ENOSPC, EIO or a signal mid-write can still leave a partial tree. That would be a larger restructure of the write path; the deterministic failure modes are now closed.

### WR-01: A `key_env` private key and downloaded gateway files survive on disk after SIGINT or SIGTERM

**Files modified:** `crates/ignition-core/src/deploy/transport/remote.rs`
**Commit:** c74a79b
**Applied fix:** Lean fix, no new dependency and no process-wide signal handler. The key was already 0600 in a 0700 dir. Each new `RemoteShell` now sweeps `ign-ssh-*` sibling temp dirs (`sweep_stale_temp_dirs`) that are real directories (never symlinks), owned by the same uid as the shell's fresh temp dir, and at least 6 hours old by mtime. All errors are ignored so a sweep can never fail a run. The module doc now states the residual. Unit test covers old, fresh, wrong-owner, non-prefix and symlinked dirs.

Residual: after a SIGINT or SIGTERM the key and downloads stay on disk (owner-only) until the next deploy verb runs on the same machine at least 6 hours later. A signal handler or an ssh-agent or `-i /dev/fd/N` approach would close it fully, but needs process-wide handlers or a different key-passing design, so it was left out.

### WR-02: Target `mode` may be `core` or `local`

**Files modified:** `crates/ignition-core/src/deploy/target.rs`
**Commit:** 4c7dda3
**Applied fix:** `Target::validate` adds a finding when `mode` is `core` or `local`, matching the write half's `mirror_mode_problem`. It surfaces as the existing `deploy_tree_invalid` slug (same path as other bad mode names), so the exit-code contract and README table are unchanged. Unit test `reserved_collection_modes_are_refused`.

### WR-03: A relative `local.data_dir` breaks `mirror_for_node` and `default_staging_mode`

**Files modified:** `crates/ignition-core/src/deploy/transport/mod.rs`, `crates/ignition-core/tests/deploy_transport.rs`
**Commit:** cc6f4c8
**Applied fix:** `node_access` now runs `std::path::absolute` on `repo.join(data_dir)` for a relative `data_dir`. Absolute values are unchanged. A `..` component in a relative path is still refused clearly by the existing validator (fail closed). New test `a_relative_data_dir_stages_and_mirrors_with_a_relative_repo` uses `Path::new(".")` and a relative `data_dir` through both `default_staging_mode` and `mirror_for_node(Separate)`. It failed before the fix with the relative root.

### WR-04: `open_beneath` opens the final component without `O_NONBLOCK`

**Files modified:** `crates/ignition-core/src/deploy/tree.rs`
**Commit:** b4108af
**Applied fix:** Added `OFlags::NONBLOCK` to the final `openat`. The later fstat check then returns `Changed` for a FIFO. New test creates a FIFO where a regular file was checked and asserts `open_regular_file` returns `Changed` within 5 s; it timed out (blocked) before the fix.

### WR-05: Gateway-supplied JSON key names reach verify findings and the terminal unsanitised

**Files modified:** `crates/ignition-core/src/deploy/verify.rs`, `crates/ignition-core/src/deploy/tree.rs`
**Commit:** a36c4cc
**Applied fix:** `child_path` now runs each key through `printable()` (control characters dropped, length capped). `collect_capped` (the local walk) refuses a control character in an entry name, aligning the local transport with sftp; the error message does not echo the name. Tests: `gateway_supplied_keys_are_sanitised_in_diff_paths` and `walk_capped_refuses_a_control_character_in_a_name`.

Behaviour note: because `walk_capped` also scans the existing repo subtrees in `deploy pull`, a repo file whose name contains a control character is now refused there too (previously it would have been deleted by the pull). This is consistent with `safe_component` and is fail-closed.

### WR-06: `same_scalar` treats distinct integers above 2^53 as equal

**Files modified:** `crates/ignition-core/src/deploy/verify.rs`
**Commit:** c497340
**Applied fix:** The f64 comparison is used only when at least one side is a float; two integers compare exactly. Test `large_integers_compare_exactly` covers 2^53 and 2^53+1 and `u64::MAX` vs `u64::MAX - 1`.

## Skipped Issues

None.

Info findings IN-01..IN-08 were out of scope (`fix_scope: critical_warning`) and are unchanged.

---

_Fixed: 2026-10-15_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
