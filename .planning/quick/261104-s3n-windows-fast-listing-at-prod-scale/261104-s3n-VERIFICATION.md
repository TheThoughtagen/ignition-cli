---
phase: quick-261104-s3n
verified: 2026-11-05T11:50:00Z
status: passed
score: 9/9 must-haves verified (live-VM numbers carried from SUMMARY, not re-run)
behavior_unverified: 0
overrides_applied: 0
gaps: []
deferred:
  - truth: "A full live read of the production-shape tree finishes in under 2 minutes (download cost)"
    addressed_in: "quick 261105-dl1"
    evidence: "dl1 SUMMARY: live read 48.14 s with 7 connections; dl1 verified in 261105-dl1-VERIFICATION.md"
---

# Quick 261104-s3n Verification: fast Windows listing at production scale

**Goal (BRIEF):** Windows deploy reads list the production-shape tree with an O(1) connection count, keep every safety property, and are guarded by production-scale tests. Downloads are out of scope for this task (measure and report).
**Verified:** 2026-11-05 on `feat/windows-fast-listing`. Re-verification: No.
**SUMMARY status `incomplete`:** the blocker was the download wall (about 88 min, exit 4 at 1,081 s). Task 261105-dl1 closed it (see its VERIFICATION.md). The live full-read-under-2-minutes criterion is therefore met at branch head, not at `3054a52`.
**Not run, by instruction:** live tests, any VM or prod host.

## Observable truths

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | Windows listing uses a fixed number of PowerShell calls, not per-level sftp | VERIFIED | `discover_windows` (sftp.rs:1013): Step A one batched `enumerate_children` over the path folders, then one batched `enumerate` over the requested roots. `windows_prod_shape_list_tree_uses_constant_connections` passes (2 connections, 0 `ls`). `windows_prod_shape_read_uses_constant_connections` passes. |
| 2 | RED first on pre-fix code | VERIFIED | Commit `5faea3c` (tests and generator only) precedes fix `23100e3`. SUMMARY records 20 connections and 7,590 `ls` lines on pre-fix code. I did not re-run the RED state. |
| 3 | Symlink and reparse points refused; non-file/non-dir refused | VERIFIED | `entry.reparse` refusals at sftp.rs:1109, 1179 and 1395 (path folders, walked entries, children). `windows_read_refuses_reparse_points` and `windows_read_refuses_a_bad_folder_on_its_path` pass. |
| 4 | Required `config` and `config/resources` check; N2 probe of requested folders kept | VERIFIED | Step A always includes `RESOURCES` prefixes. `windows_read_fails_when_a_listed_root_vanishes_before_the_walk` and `..._refuses_a_requested_root_made_while_it_is_read` pass. |
| 5 | `READ_MAX_ENTRIES` budget and its exact error detail kept | VERIFIED | `more than {READ_MAX_ENTRIES} entries under the data directory (at ...)` at sftp.rs:832, 1098, 1203. `windows_read_refuses_a_walked_tree_deeper_than_the_cap` passes. |
| 6 | Long (260+) entries listed, never dropped; 240+ not downloaded | VERIFIED | `LONG_PATH_LIMIT` classification at sftp.rs:1184 returns `LongEntry` items reported as unreadable. `windows_read_unreadable_core_and_local_entries` and the over-limit tests pass. The generator emits at least 7 paths of 260+ chars (`prod_shape_matches_the_recorded_production_numbers` passes). |
| 7 | Unrepresentable names refused (strict) or skipped (lenient); sizes reported; same `TreeListing`/`Found` output | VERIFIED | `windows_read_holds_walked_names_to_the_batch_line_gate`, `windows_discover_lenient_skips_an_unsafe_name_and_its_subtree`, `windows_prod_shape_read_matches_the_reference` pass. |
| 8 | Prod-shape generator is deterministic code, at the BRIEF numbers | VERIFIED | `tests/common/prod_shape.rs` (404 lines). Test asserts 14,119 files, 7,500 dirs, depth 14, fanout 89, 7+ long paths, type tree 156 and 85. No checked-in blobs; names generic. |
| 9 | Local conformance row with budget; live gate; test docs | VERIFIED (live from SUMMARY) | `prod_scale_local_read`, `prod_scale_posix_sftp_read`, `prod_scale_pwsh_walk` pass here with budgets justified in comments. README documents the prod-scale gate. Live gate (`live_transport_wflwvm01.rs`, 4 `#[ignore = "live: ..."]` tests with named env opt-ins): SUMMARY reports list_tree 21.75 s; I did not re-run it. |

## Test runs (this verification)

| Command | Result |
|---|---|
| `cargo fmt --check` | pass |
| `cargo test -p ignition-core --lib windows_` | 57 passed, 0 failed, 1 ignored (opt-in), 31.5 s |
| `cargo test -p ignition-core --test deploy_prod_scale` | 6 passed, 0 failed, 97.8 s |

Private `CARGO_TARGET_DIR` in the scratchpad. clippy and xwin clippy were not re-run; SUMMARY claims they passed.

## Constraints

| Constraint | Result |
|---|---|
| No Cargo.toml, Cargo.lock, goldens, fixtures, `error.rs`, slug or exit-code change | PASS (`git diff main...HEAD --name-only`: 10 files, none of these) |
| No IPs or prod host names in committed added lines | PASS (no dotted-quad, production addresses or host names) |
| No co-author lines | PASS |
| Never committed: `.planning/config.json`, `.planning/research/.cache/`, `the local deploy-account script`, `.planning/debug/tg-pg-table-check.md` | PASS for this branch (no branch commit touches `.planning/` or the ps1). `.planning/config.json` is already tracked on main from older commits and is modified in the working tree; keep it unstaged |
| No skips, `todo!`, `unimplemented!`, debt markers; `#[ignore]` only with a named opt-in | PASS: 4 `#[ignore = "live: ..."]` in `live_transport_wflwvm01.rs` plus 1 `#[ignore = "opt-in: ..."]` in sftp.rs, all with reasons |
| POSIX unchanged | PASS by inspection and test: removed code is the Windows `cross_check`; `prod_scale_posix_sftp_read` passes with 19 sessions, as before |

## Findings

No BLOCKER. No WARNING.

Info:
- Behaviour change (documented, new strictness): a reparse point on the path to a requested folder is now refused on Windows.
- The sftp-versus-walk `cross_check` was removed because the walk is now the listing. Its tests were rewritten (listed in SUMMARY) and pass.
- The 261104 SUMMARY frontmatter still says `status: incomplete`. That is historically accurate for `3054a52`, and the gap is closed by 261105-dl1. Consider adding a closing note.
- Windows-only rows compiled only (xwin clippy); Windows CI must confirm.

## Status: passed

The listing goal is achieved. The one open criterion from the SUMMARY, a full read under 2 minutes, is met by task 261105-dl1.
