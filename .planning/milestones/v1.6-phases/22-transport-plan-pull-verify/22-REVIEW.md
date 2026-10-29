---
phase: 22-transport-plan-pull-verify
reviewed: 2026-10-15T00:00:00Z
depth: standard
files_reviewed: 43
files_reviewed_list:
  - Cargo.toml
  - README.md
  - crates/ignition-cli/src/cli.rs
  - crates/ignition-cli/src/main.rs
  - crates/ignition-cli/src/mcp.rs
  - crates/ignition-cli/src/render.rs
  - crates/ignition-cli/tests/contract_deploy.rs
  - crates/ignition-cli/tests/contract_stdout_purity.rs
  - crates/ignition-cli/tests/deploy_family_coverage.rs
  - crates/ignition-core/Cargo.toml
  - crates/ignition-core/src/actions/deploy.rs
  - crates/ignition-core/src/deploy/layout.rs
  - crates/ignition-core/src/deploy/ledger.rs
  - crates/ignition-core/src/deploy/lint.rs
  - crates/ignition-core/src/deploy/mod.rs
  - crates/ignition-core/src/deploy/plan.rs
  - crates/ignition-core/src/deploy/pull.rs
  - crates/ignition-core/src/deploy/target.rs
  - crates/ignition-core/src/deploy/transport/local.rs
  - crates/ignition-core/src/deploy/transport/mod.rs
  - crates/ignition-core/src/deploy/transport/process.rs
  - crates/ignition-core/src/deploy/transport/remote.rs
  - crates/ignition-core/src/deploy/transport/sftp.rs
  - crates/ignition-core/src/deploy/transport/winps.rs
  - crates/ignition-core/src/deploy/tree.rs
  - crates/ignition-core/src/deploy/verify.rs
  - crates/ignition-core/src/error.rs
  - crates/ignition-core/tests/common/forbid_writes.rs
  - crates/ignition-core/tests/common/mod.rs
  - crates/ignition-core/tests/common/plan_fixture.rs
  - crates/ignition-core/tests/common/sftp_server.rs
  - crates/ignition-core/tests/deploy_plan.rs
  - crates/ignition-core/tests/deploy_pull.rs
  - crates/ignition-core/tests/deploy_render.rs
  - crates/ignition-core/tests/deploy_transport.rs
  - crates/ignition-core/tests/deploy_verify.rs
  - crates/ignition-core/tests/fixtures/live-rig-deploy/capture22.sh
  - crates/ignition-core/tests/fixtures/wflwvm01/capture22w.sh
  - crates/ignition-core/tests/live_deploy_plan.rs
  - crates/ignition-core/tests/live_transport_wflwvm01.rs
  - crates/ignition-tui/src/routes.rs
findings:
  critical: 1
  warning: 6
  info: 8
  total: 15
status: issues_found
---

# Phase 22: Code Review Report

**Reviewed:** 2026-10-15
**Depth:** standard
**Files Reviewed:** 43 (deep read of all non-test source in the transport, plan, pull, verify, ledger and action layers; test and fixture files were scanned for reliability problems only)

## Summary

The transport layer is carefully built. Path gating is layered: `safe_remote_path`, `safe_local_path`, the typed `SftpOp` set and a second `check_batch_line` guard. ssh options are hardened and the argv has `--` before `user@host`. Listings, downloads and writes are bounded by caps. A failed read is never an empty read, and the write half (`TreeMirror`) is not reachable from any verb. I found no way for a host-controlled value to reach an sftp batch line or an ssh argument unquoted, and no way for a verb to write to the host.

The defects below are concentrated outside the happy path of the transport:

- One data-loss ordering bug in `deploy pull`.
- One key-hygiene gap.
- A mode-name validation gap.
- A relative-path gap that breaks the Phase 23 mirror for local nodes.
- A few hardening and consistency items in the verify path.

No structural findings (fallow) were provided for this review.

## Critical Issues

### CR-01: `deploy pull` deletes the existing tree before validating or writing the new one

**File:** `crates/ignition-core/src/actions/deploy.rs:638-699` (unix `write_pulled`; the non-unix twin at `:721-757` has the same order)
**Issue:** `write_pulled` first unlinks every existing file and rmdirs every directory under `resources/`, `tags/` and `modes/<env>/` (lines 638-673). Only afterwards does it check each pulled path with `safe_component` (line 684) and create the new files with `create_new` (O_EXCL, line 696). Anything that fails after the deletes leaves the repo partially wiped, with the old tree gone and the new one half-written:

- An unsafe component is refused after the old tree is already gone.
- `create_new` returns `Exists` for two pulled names that differ only by case on a case-insensitive filesystem. This is the project owner's platform (macOS), and a Linux gateway can legitimately hold `Foo` and `foo`. It surfaces as `CoreError::Internal`, exit 1.
- ENOSPC, EIO, or a SIGINT or SIGTERM mid-write (there is no signal handling) have the same effect.

The user did pass `--yes`, but that confirms "replace with the live tree", not "end with a partial tree". Uncommitted local edits under those subtrees are unrecoverable.

**Fix:**
1. Validate every pulled path first, including a case-fold collision check, before the first delete. Move the `safe_component` loop above the removal block.
2. Write the new tree into a sibling staging directory inside the gateway directory, for example `gateways/<g>/.ign-pull-<pid>` (dot-named, so it is never rendered). Only then swap it in with `renameat`: remove the old subtrees and rename the new ones into place, so the destructive window is a few renames.
3. At minimum, run the validation (including case-fold collisions) before any delete.

```rust
// before the `if let Some(existing)` block
let mut folded = std::collections::BTreeSet::new();
for (path, _) in &source.files {
    if path.components().any(|c| !safe_component(c)) { /* return deploy_tree_invalid */ }
    if !folded.insert(path.as_str().to_ascii_lowercase()) { /* return deploy_tree_invalid: case collision */ }
}
```

## Warnings

### WR-01: A `key_env` private key, and every downloaded gateway file, survive on disk after SIGINT or SIGTERM

**File:** `crates/ignition-core/src/deploy/transport/remote.rs:619-678` (the key is written at `:669-672`); downloads at `sftp.rs:1234-1244`
**Issue:** The key text from `key_env` is written to `<tmp>/ign-ssh-XXXX/key` (0600, in a 0700 dir). The same directory is the parent of the `ign-read-*` download dirs, which hold the gateway's config files. Cleanup relies solely on `TempDir::drop`. The binary installs no signal handler (grep for `ctrl_c`, `signal::` and `SIGINT` finds nothing), so Ctrl-C, SIGTERM from CI cancellation, or a `kill_on_drop` race leaves the plaintext deploy key and gateway config in `$TMPDIR` indefinitely.

A plan or verify over sftp with a 1050 s default op timeout is exactly the kind of run people interrupt. The permissions are owner-only, but a leftover plaintext credential file is an insecure-storage defect. It undercuts the module's stated "deleted with the temp dir on drop" guarantee.

**Fix:** Install a `tokio::signal` (and Unix `SIGTERM`) handler around the deploy verbs that drops the `RemoteShell` (and thus the temp dir) before exiting. Alternatively:
- Unlink the key file as soon as it is no longer needed.
- Pass the key to ssh without a file, for example via an ssh-agent socket or `-i /dev/fd/N`.

### WR-02: Target `mode` may be `core` or `local`, which makes plan, pull and verify read the wrong collection

**File:** `crates/ignition-core/src/deploy/target.rs:150-153` (validation); consumers at `sftp.rs:1541-1545`, `sftp.rs:996`, `local.rs:317-321`
**Issue:** `Target::validate` only runs `component_problem` on `mode`. The write half refuses `core` and `local` (`mirror_mode_problem`, `transport/mod.rs:458`), but the read side does not.

- `mode: core`: `read()` returns the `core` collection as both `observed.core` and `observed.mode`. The plan then shows every unit as `live_in: ["core","mode"]`, and a ledger can propose `core` files in `delete_set`. Phase 23's mirror would refuse the delete, but the plan hash and the review are already wrong.
- `mode: local` over sftp: `local` is excluded from the download roots, so `observed.mode` is silently empty. Every unit plans as `added` and `has_changes` is true, with only a `local_units` warning.

**Fix:** Reject these in `Target::validate`:

```rust
if self.mode == "core" || self.mode == "local" {
    push(format!("mode {:?}: names a collection the gateway reserves", self.mode));
}
```

### WR-03: A relative `local.data_dir` (the default with `--repo .`) breaks `mirror_for_node` and `default_staging_mode`

**File:** `crates/ignition-core/src/deploy/transport/mod.rs:184-192, 435-443`; `mod.rs:582-605, 620-658`; `local.rs:111-122`
**Issue:** `node_access` resolves a relative `local.data_dir` with `repo.join(configured)`. It does not absolutize, and `--repo` defaults to `.`. For `StagingMode::Separate`, `validate_staging_root_for` calls `absolute_components` on both the staging root and `resources_display()`. `absolute_components` returns "must be an absolute path" for a relative string.

So with the documented relative `data_dir` and the default repo:
- `default_staging_mode` returns a relative root.
- `LocalFsTransport::with_mirror` then fails with "the staging root ... must be an absolute path".
- Even an absolute root fails with "the resources root ... must be an absolute path".

`deploy_transport.rs` only exercises absolute `data_dir` values (`sc.data.display()`, `data.path()`), so this is untested. It will surface the first time Phase 23 runs against a repo-relative local node. This is by code reading; I did not execute it.

**Fix:** Absolutize in `node_access`:

```rust
let data_dir = if configured.is_absolute() { configured.to_path_buf() }
               else { std::path::absolute(repo.join(configured)).map_err(...)? };
```

Add a test with `Path::new(".")` and a relative `data_dir` through `default_staging_mode` and `mirror_for_node(Separate)`.

### WR-04: `open_beneath` opens the final component without `O_NONBLOCK`, so a swapped-in FIFO hangs the runner

**File:** `crates/ignition-core/src/deploy/tree.rs:544-550`
**Issue:** The function exists to survive a file swapped after the `symlink_metadata` check. The final `openat` is `RDONLY | NOFOLLOW | CLOEXEC`. If the regular file is replaced by a FIFO between the check and the open, `open(2)` blocks until a writer appears. The later `fstat` mismatch check is never reached, so there is no timeout and no cancellation. The sibling helpers `dirfd::read_file` and `dirfd::open_overwrite` already use `NONBLOCK` for exactly this reason. Every local read (`read_blobs`, `read_ledger_blocking`, `read_repo_file`, `BaseTree`, staged-copy reads) goes through this path.

**Fix:** Add `OFlags::NONBLOCK` to the final `openat` flags. `fstat` then rejects the FIFO as `Changed`, and a regular file is unaffected by `O_NONBLOCK`.

### WR-05: Gateway-supplied JSON key names reach verify findings and the terminal unsanitised

**File:** `crates/ignition-core/src/deploy/verify.rs:655-661, 686-713, 763-777`; printed in `crates/ignition-cli/src/render.rs` (`render_deploy_verify_human`)
**Issue:** The module promises value-free, sanitised findings, and other gateway-sourced strings go through `printable()`. `diff_exact` and `diff_attributes` build the "differs at" paths from `r.keys()` of the REST body, and `child_path` applies neither `printable` nor a length cap. A gateway (or a resource with user-chosen map keys) can inject control characters or ANSI sequences, or arbitrarily long keys, into `data.checks[].findings[].detail` and the human output.

A related case: local-transport file names are not control-character-checked (`walk_capped`, `RelPath::new` refuse only NUL, backslash and empty/`.`/`..`). They are printed raw by `render_deploy_plan_human` (`entry.key`, `delete {path}`). The sftp path refuses control characters, so the two transports differ.

**Fix:** Run each key through `printable()` in `child_path` for REST-derived keys. Refuse control characters in `walk_capped`, or `printable()` the keys in the human renderers.

### WR-06: `same_scalar` treats distinct integers above 2^53 as equal, a false pass in `read_back`

**File:** `crates/ignition-core/src/deploy/verify.rs:672-683`
**Issue:** After `x == y` fails for two `serde_json::Number` values, the fallback compares `as_f64()`. Two different `i64` or `u64` values above 2^53 collapse to the same `f64` and compare equal, so a changed 64-bit id or timestamp reads back "green". The intent was only to equate `1` with `1.0`.

**Fix:** Take the float path only when at least one side is not an integer:

```rust
(Value::Number(x), Value::Number(y)) => {
    if x.is_f64() || y.is_f64() { x.as_f64() == y.as_f64() } else { x == y }
}
```

## Info

### IN-01: `TransportFailed` hint is wrong for every operation except plan reads

**File:** `crates/ignition-core/src/error.rs` (the `hint()` arm for `TransportFailed`)
**Issue:** The hint says "Nothing was read as absent, so no plan was made". The variant is also raised by `pull`, `verify`, `stage`, `commit` and `ssh-setup` (for example the missing `key_env` variable). For `stage` and `commit` the "no plan" sentence is misleading, because writes may have happened (see the commit error text "were not rolled back").
**Fix:** Key the hint off `operation`, or drop the plan sentence.

### IN-02: `log_window_start` does unchecked i64 arithmetic on gateway-supplied values

**File:** `crates/ignition-core/src/deploy/verify.rs:462-465`
**Issue:** `last_scan_timestamp - duration - LOG_WINDOW_SLACK_MS` can overflow when the gateway reports extreme values. That panics in debug builds and wraps in release. `settle_logs` correctly uses `saturating_add`.
**Fix:** Use `saturating_sub`.

### IN-03: Dead public transport API weakens the "never `get -R`" invariant

**File:** `crates/ignition-core/src/deploy/transport/remote.rs:401-403, 445-450`; `winps.rs:72, 168-174`; `remote.rs:703-721`
**Issue:**
- `SftpOp::GetRecursive`, `SftpOp::Ls`, `SftpOp::LsLong`, `winps::MOVE_REPLACE` and `WinScript::move_replace` are referenced only from tests.
- `GetRecursive` is an accepted `sftp_batch` op, while the sftp module doc says `get -R` is never issued because the server decides its contents.
- `RemoteShell::local_sftp_server` is `pub` (`#[doc(hidden)]`) and lets a caller point the transport at an arbitrary local binary via `-D`.

**Fix:** Delete the unused variants, or gate them and the test seam with `#[cfg(any(test, feature = "test-seam"))]`.

### IN-04: Dependency and cfg hygiene

**File:** `crates/ignition-core/Cargo.toml:32`; `crates/ignition-core/src/deploy/transport/local.rs:743-751`
**Issue:**
- `similar = "3"` is declared inline although the workspace pins versions in `[workspace.dependencies]` (project constraint).
- `write_in_place` is `#[cfg(not(unix))]` but contains a `#[cfg(unix)]` block that can never compile in.

**Fix:** Move `similar` to the workspace table and delete the dead block.

### IN-05: The ssh child inherits the `key_env` variable, and `-i` is not `%`-escaped

**File:** `crates/ignition-core/src/deploy/transport/process.rs:163-170`; `remote.rs:257-261, 770-773`
**Issue:**
- `run_capped` adds only `LC_ALL` and inherits the full environment. The variable named by `key_env` holds the private key text and is visible to ssh, sftp and any `ProxyCommand` or `ProxyJump` helper in the user's config.
- `hardened_options` validates the identity path with `option_path` but discards the `%`-escaped result. `-i <path>` is passed raw, and ssh percent-expands `-i` paths, so a `key_file` containing `%` resolves to a different file.

**Fix:** Pass an `env_remove` list to the runner containing the `key_env` name. Reject `%` in the identity path.

### IN-06: Commit creates an empty mode collection even when nothing is staged or deleted

**File:** `crates/ignition-core/src/deploy/transport/sftp.rs:2442-2444`; `local.rs:1090-1091`
**Issue:** The sftp commit inserts `mode_chain` into the mkdir set whenever it is missing. The unix local commit calls `ensure_dir(&resources, &setup.mode)` unconditionally. A commit with an empty staged set and an empty delete set therefore writes a directory on the host.
**Fix:** Return an empty `MirrorReport` early when `files` and `delete_set` are both empty.

### IN-07: `Ledger::to_bytes` swallows serialization failure

**File:** `crates/ignition-core/src/deploy/ledger.rs:250-254`
**Issue:** `serde_json::to_vec_pretty(self).unwrap_or_default()` would return a lone `"\n"` on error. That is a ledger Phase 23 would write and the reader would then refuse as "empty".
**Fix:** Return `Result<Vec<u8>, CoreError>`.

### IN-08: The local walk has no depth bound; the sftp walk caps it at 64

**File:** `crates/ignition-core/src/deploy/tree.rs:373-407`
**Issue:** `walk_capped` recurses without a depth limit and runs on a `spawn_blocking` thread with a small stack. A deeply nested hostile directory is limited only by `PATH_MAX`. The entry cap bounds breadth, not depth. This is the same hostile-tree threat model that `READ_MAX_DEPTH` addresses on the sftp side.
**Fix:** Thread a depth counter through the recursion and refuse beyond `READ_MAX_DEPTH`.

---

_Reviewed: 2026-10-15_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
