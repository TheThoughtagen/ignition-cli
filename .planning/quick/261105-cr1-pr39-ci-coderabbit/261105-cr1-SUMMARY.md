---
phase: quick-261105-cr1
plan: 01
quick_id: 261105-cr1
branch: feat/windows-fast-listing
status: complete
completed: 2026-11-05
key-files:
  modified:
    - crates/ignition-core/src/deploy/transport/bundle.rs
    - crates/ignition-core/src/deploy/transport/winps.rs
    - crates/ignition-core/src/deploy/transport/sftp.rs
    - crates/ignition-core/tests/live_transport_wflwvm01.rs
    - README.md
actuals:
  tasks: 6
  commits: 9
---

# Quick 261105-cr1: PR #39 Windows CI fix and CodeRabbit review

Fixed the Windows CI failure and all five CodeRabbit items. Each item was
checked against the code first. None was wrong, so nothing is pushed back.
C0 is solved in a different way from the working-tree draft.

## Items

| Item | Result | Commits |
|---|---|---|
| C0 CI | Fixed, done differently from the draft | `2e1246c` |
| C1 central walk | Fixed (RED, then GREEN) | `5e4e29f`, `3632e58`, `7f87ae1` |
| C2 BUNDLE copy | Fixed (RED, then GREEN) | `547c1b8`, `4e433ce` |
| C3 admin-route identity | Fixed, compile-checked only | `f0ecbf5` |
| C4 partial work dir | Fixed, compile-checked only | `f0ecbf5` |
| C5 README | Fixed | `55e6a61` |

- **C0.** The diagnosis was correct. zip 8.6 (`types.rs`) picks
  `System::Dos` under `cfg!(windows)` unless the options name a system. It
  still writes `0o100644 << 16` into the external attributes. The draft's
  cfg branch made the Windows half test the zip crate's platform default,
  not our code, and the test's own doc says "an honest Unix-made archive".
  The test now sets `.system(zip::System::Unix)` and keeps one assertion
  that is the same on every platform. A grep found no other test that
  depends on the writer's host. The sftp `zip_of` and `zip_set_mode`
  fixtures set the host byte explicitly. The other `ZipWriter` tests never
  check host or mode.
- **C1.** The finding was valid. The caller already rejects archives larger
  than `archive_limit`, so the memory was bounded by the listed bytes, but a
  crafted central directory at offset 0 still read the whole archive into
  memory. The walk now reads one record at a time through a 64 KiB
  `BufReader`. It keeps the `READ_MAX_ENTRIES + 1` stop and the cut-off and
  Zip64 checks. RED: a counting reader showed that the old walk read all
  8,388,779 bytes of the crafted file. The new walk reads at most the
  records plus 256 KiB, for both the 3-record case and the
  over-the-limit case.
- **C2.** The finding was valid. Each read now asks for no more than
  `listed - copied + 1` bytes, so the copy stops one byte past the listed
  size. The script uses only plain `while`/`if`/`break` and `[int]` casts:
  no `[Math]::Min` and no new overloads, so it is PS 5.1 safe. Since `n`
  is now a lower bound when the file grew, `sftp.rs` names that case
  "more than the N bytes it was listed as". The offline fake answers the
  same way. RED: the pwsh row answered `n = 300000` before the fix and
  answers `70001` after it.
- **C3.** The finding was valid: `IGN_WINVM_RG` and `IGN_WINVM_VM` are
  separate from the ssh host. `prod_shape_admin`, `_copy` and `_remove` now
  need an `AdminRoute` value. Only `prod_shape_admin_route` creates one, and
  it does so only after `[Environment]::MachineName` over ssh matches the
  name the az route prints (ASCII case ignored). The check runs lazily,
  right before the first admin write in seed and in restore. A run whose
  deploy account can write the folders needs no az call.
- **C4.** The finding was valid. Seed writes the manifest before it copies
  anything into the core folders. So when no manifest exists (and the
  existing ProdShape assertion holds), nothing was seeded, and restore now
  removes a leftover `ign-prod-shape` dir.
- **C5.** The finding was valid. The README now says that when BUNDLE_CLEAN
  also fails, the archive can stay in the staging dir and the read's error
  names that dir. Otherwise only a killed read leaves the archive behind.

C3 and C4 were not run live. No VM or host was contacted. They are checked
by clippy (`--all-targets`, macOS and the Windows MSVC target) only.

## Gates

All four were run with the private `CARGO_TARGET_DIR`:

- `cargo fmt --check`: clean
- `cargo clippy --workspace --all-targets -- -D warnings`: clean
- `cargo test --workspace`: 3,227 passed, 0 failed, 100 ignored (97 test
  binaries)
- `cargo xwin clippy --target x86_64-pc-windows-msvc --workspace
  --all-targets -- -D warnings`: clean

## Deviations

None from the plan. `7f87ae1` is a rustfmt-only follow-up to the RED test.
No version, golden, slug or exit-code changes.

## Self-Check: PASSED

All commits exist on `feat/windows-fast-listing`. Nothing is pushed.
