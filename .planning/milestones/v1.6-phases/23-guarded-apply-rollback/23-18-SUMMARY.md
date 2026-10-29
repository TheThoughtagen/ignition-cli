---
phase: 23-guarded-apply-rollback
plan: 18
subsystem: deploy
tags: [ledger-paths, journal, transport, tree-reader, fail-closed, rust, tdd]
status: complete

requires:
  - phase: 23-guarded-apply-rollback
    provides: 23-03 Journal v1, JOURNAL_FILE, JOURNAL_MAX_BYTES, RECORD_FILE; 23-02 checkpoint outcome item 5 (on-box layout)
  - phase: 22-transport-plan-pull-verify
    provides: TreeReader::read_ledger, LedgerRoot, open_regular_file, the sftp descend walk
provides:
  - journal_rel, snapshot_rel, record_rel, journal_location, location_of (deploy/ledger.rs)
  - TreeReader::read_journal on LocalFsTransport and SftpTransport, and on the DeadTransport and Counting test doubles
affects: [23-04, 23-05, 23-06, 23-11, 23-12, 23-20]

actuals:
  tokens: 6000
  tasks: 2
  commits: 3

tech-stack:
  added: []
  patterns:
    - "read_ledger and read_journal share one private helper per transport (read_regular_blocking on local, read_home_file on sftp); a refusal is built by a caller-supplied closure, so the two files fail closed identically and differ only in error variant"
    - "the size cap is checked from metadata/listing before any byte is read or downloaded, and again after the descriptor-relative open and during the bounded read"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/deploy/ledger.rs
    - crates/ignition-core/src/deploy/transport/mod.rs
    - crates/ignition-core/src/deploy/transport/local.rs
    - crates/ignition-core/src/deploy/transport/sftp.rs
    - crates/ignition-core/tests/deploy_verify.rs
    - crates/ignition-core/tests/deploy_pull.rs

key-decisions:
  - "A journal refusal is deploy_run_invalid with run_id `unknown` and problem `<location>: <problem>`, the same shape journal.rs `refuse` uses when the file is unparsable (the run id is not known before the journal is parsed)"
  - "location_of(root, rel) is the one place a location is rendered (`~/` prefix for AccountHome); ledger_location and journal_location call it"
  - "journal_location was added in ledger.rs because it did not yet exist in deploy/apply/journal.rs; 23-12 may re-export or reuse it"

patterns-established:
  - "ledger.rs owns every on-box path under ign-deploy/<gateway>/<mode>/; formats live in deploy/apply/"

requirements-completed: [DAPL-09, DAPL-08]

duration: ~25 min (most of it cargo builds)
completed: 2026-10-16
---

# Phase 23 Plan 18: Journal paths and the journal read Summary

**Fixed on-box paths for the journal, snapshot and run record under the ledger root, and `TreeReader::read_journal` on the local and sftp transports sharing the ledger's fail-closed read code path.**

## The three path strings

| Helper | Value |
|---|---|
| `journal_rel(g, m)` | `ign-deploy/<g>/<m>/journal.json` |
| `snapshot_rel(g, m, run)` | `ign-deploy/<g>/<m>/snapshots/<run>` |
| `record_rel(g, m, run)` | `ign-deploy/<g>/<m>/snapshots/<run>/record.json` |

None is dot-named or inside `config/resources/`. Each resolves under the same `LedgerRoot` as the ledger (account home on ssh, `<data_dir>` on local). `journal_location(root, g, m)` renders the journal name for an error (`~/` prefix on ssh). A test pins all of it (`journal_snapshot_and_record_paths_are_pinned`).

## Refusal cases per transport

`read_journal` returns `Ok(None)` only when the file is absent at some level (no `ign-deploy/`, no gateway dir, no mode dir, no file). Everything else is `deploy_run_invalid` (exit 6) naming the location, never file content:

| Case | local (`read_journal_local`, `read_journal_local_refuses_symlinks`) | sftp (`read_journal_sftp`) |
|---|---|---|
| journal is a symlink | refused (unix) | refused |
| a directory on the way is a symlink | refused (unix) | refused |
| a path component is a file | refused | refused |
| journal path is a directory | refused | refused |
| over `JOURNAL_MAX_BYTES` | refused from metadata | refused from the listing, before any `get` |
| unsafe gateway or mode name | refused | refused (plus the Windows component gate) |
| unreadable host (data dir missing / home not listable) | `transport_failed` | `transport_failed` |
| failed download / read error | refused | `transport_failed` (the transport's own failure, as for the ledger) |

## Accomplishments

- Path helpers in `ledger.rs` beside `ledger_rel` and `staging_rel`; `ledger_location` now delegates to the shared `location_of`.
- `TreeReader::read_journal` documented as the fail-closed read (DAPL-09).
- The ledger read on both transports was factored into one private helper each, and the journal read calls the same helper with `journal_rel`, `JOURNAL_MAX_BYTES` and a `deploy_run_invalid` builder. The existing ledger tests pass unchanged.
- The local helper gained an up-front metadata size check (before `open_regular_file`); the descriptor-relative check and bounded read remain as defence against a swap or growth.
- `DeadTransport` (`unreachable!`, verify never reads the journal) and `Counting` (counts the call) implement the method; the workspace builds.

## Task Commits

1. **Task 1 (tracer), RED:** `addebb1` (path helpers and tests, plus the failing `read_journal_local_roundtrip`)
2. **Task 1 (tracer), GREEN:** `fea0909` (trait method, local and sftp implementations, test doubles)
3. **Task 2, tests:** `5082211` (the refusal matrix on both transports). The tests passed against the Task 1 shared helpers, so no implementation change was needed in this commit.

**Plan metadata:** the final docs commit (SUMMARY, STATE, ROADMAP) follows this file.

## Tracer feedback gate

Auto mode is not active, so the interactive rule asks for a `checkpoint:human-verify` after the tracer commit. This project runs `workflow.human_verify_mode = end-of-phase` and the slice has no human-verifiable surface (a pure read seam), so, as in 23-01 and 23-02, execution continued. The tracer's `<verify>` (ledger lib tests, the local `read_journal` tests, `deploy_verify` and `deploy_pull`) passed before the tracer commit `fea0909` and the lib suites were re-run after Task 2.

## Test results

- `cargo test --workspace`: 2250 passed, 0 failed.
- `cargo test -p ignition-core --lib -- deploy::transport`: 114 passed, including `read_journal_local_roundtrip`, `read_journal_local`, `read_journal_local_refuses_symlinks`, `read_journal_sftp`.
- `cargo test -p ignition-core --test deploy_verify --test deploy_pull`: 18 + 51 passed (counts as printed per binary).
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The plan's verify commands use two filters after `--lib`**
- **Found during:** Task 1 verify
- **Issue:** `cargo test -p ignition-core --lib deploy::transport::local read_journal` is rejected by cargo (`unexpected argument 'read_journal'`); `cargo test` accepts one filter before `--`.
- **Fix:** ran `cargo test -p ignition-core --lib -- deploy::transport::local::tests::read_journal` (and the `deploy::ledger deploy::transport::local` pair after `--`). Same tests, same scope.
- **Files modified:** none
- **Commit:** n/a

**2. [Rule 3 - Blocking] `journal_location` did not exist**
- **Found during:** Task 1
- **Issue:** the artifact inventory lists `journal_location` under `deploy/apply/journal.rs` (23-03, 23-12), but 23-03 did not add it, and the refusal needs a rendered location.
- **Fix:** added `journal_location` (and `location_of`) to `ledger.rs`, next to `ledger_location`, since it is the path-rendering module. 23-12 can reuse it.
- **Files modified:** `crates/ignition-core/src/deploy/ledger.rs`
- **Commit:** `addebb1`

**3. [Plan note] Task 2 produced no separate implementation commit**
- The Task 1 shared helpers already refused every Task 2 case, so the Task 2 RED step was green on first run. The tests are committed as a `test(23-18)` commit.

## Threat model

T-23-71 (symlink or non-regular journal), T-23-72 (oversized journal, checked from metadata or the listing first), T-23-73 (only "not found at any level" is `None`; every other error is a refusal or `transport_failed`) and T-23-74 (paths share `ledger_rel`'s shape and root; tests pin the strings) are mitigated and covered by the tests above. Symlink and traversal discipline matches the 23-03 run store fix: every level is `symlink_metadata`-checked, names pass `component_problem`, and the read is descriptor-relative on local.

## Known Stubs

None.

## Threat Flags

None. No new network endpoint, auth path or schema; the new reads use the existing walks.

## Issues Encountered

None beyond the deviations above.

## Next Phase Readiness

- 23-12 (recovery) and the `deploy plan` refusal can call `read_journal` through `TreeReader` and parse with `Journal::parse`.
- 23-04 and 23-06 (`ApplyHost`) write the files at `journal_rel`, `snapshot_rel` and `record_rel`.
- The Windows host stays `pending` (23-01); nothing here is Windows-only, and the sftp tests run over the fake, so no unrun Windows verification is owed by this plan.

## Self-Check: PASSED

- Modified files exist (the six files in `key-files.modified`).
- Commits `addebb1`, `fea0909`, `5082211` exist in `git log`.
- `grep -n 'pub fn journal_rel\|pub fn snapshot_rel\|pub fn record_rel' crates/ignition-core/src/deploy/ledger.rs` prints three lines; `grep -c 'async fn read_journal'` over `mod.rs`, `local.rs`, `sftp.rs` totals 3.
