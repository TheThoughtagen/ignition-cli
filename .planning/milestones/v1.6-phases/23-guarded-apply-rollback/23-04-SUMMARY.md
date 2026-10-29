---
phase: 23-guarded-apply-rollback
plan: 04
subsystem: deploy
tags: [apply-host, work-list, journal, snapshot, restore, conformance, descriptor-relative, rust, tdd]
status: complete

requires:
  - phase: 23-guarded-apply-rollback
    provides: 23-03 Journal, RunRecord, fingerprint, RunId; 23-18 journal_rel, snapshot_rel, record_rel; 23-02 checkpoint outcome item 5 (on-box layout)
  - phase: 22-transport-plan-pull-verify
    provides: TreeMirror guards (mirror_path_problem, mirror_mode_problem), tree::dirfd, the C7a-4 rename rules
provides:
  - ApplyHost, the one write seam the engine drives (nine methods), with BatchSpec, WriteOp, DeleteOp, Prepared, BatchReport, ProbeReport
  - WorkList (work.tsv, header `ign-work 1`), WORK_FILE, RENAME_RETRIES, journal_file, record_file
  - impl ApplyHost for LocalFsTransport (all nine methods native), LocalFsTransport::with_apply
  - apply_host_for_node (local arm; ssh refuses until 23-06 and 23-20)
  - tests/deploy_host_conformance.rs: scenarios() with seven local rows
affects: [23-19, 23-05, 23-06, 23-20, 23-11, 23-12, 23-17]

actuals:
  tokens: 35000
  tasks: 2
  commits: 4

tech-stack:
  added: []
  patterns:
    - "one conformance table of host-agnostic rows (name plus async body over a Ctx); a host supplies a Ctx and runs every row, and a guard test fails when a row is not wired to the local host"
    - "the local host does all file work through a Dir handle (local_fs.rs): descriptor-relative with O_NOFOLLOW on unix, path-based with a per-component symlink_metadata check elsewhere"
    - "journal and record bytes are rendered once in Rust, staged beside work.tsv, and installed by copy; the host re-parses each with the strict parsers before it uses them"

key-files:
  created:
    - crates/ignition-core/src/deploy/apply/host.rs
    - crates/ignition-core/src/deploy/transport/batch.rs
    - crates/ignition-core/src/deploy/transport/local_fs.rs
    - crates/ignition-core/src/deploy/transport/local/apply_ops.rs
    - crates/ignition-core/tests/deploy_host_conformance.rs
  modified:
    - crates/ignition-core/src/deploy/apply/mod.rs
    - crates/ignition-core/src/deploy/transport/mod.rs
    - crates/ignition-core/src/deploy/transport/local.rs
    - crates/ignition-core/src/deploy/transport/sftp.rs

key-decisions:
  - "commit moves the staged copy over the target with a rename, retried RENAME_RETRIES times on an i/o refusal only (a stale pre-image, a symlink or a non-regular file is never retried), and refuses when the staged copy and the target are the same inode (C7a-4: a self-rename deletes the file)"
  - "restore copies each pre-image from the snapshot to a staged restore copy under staging/<run>/restore and renames it over the live path, so the snapshot is never moved and a second restore finds the hash already in place and writes nothing"
  - "the journal is created with a temp file and a hard link, so it appears whole or not at all and a racing second run gets Exists rather than overwriting"
  - "the pre-image check reads and hashes each live file once; a path reached through a symlink or a non-regular file counts as a mismatch (stale), never as absent"

patterns-established:
  - "ApplyHost callers pass a BatchSpec whose journal and record describe exactly the batch; BatchSpec::problem() proves it (touched set, per-path hashes, strict reparse) before any I/O"
  - "a host reports per-entry progress by index into the spec (moved, deleted) and a value-free failure string, never file content"

requirements-completed: []
requirements-advanced: [DAPL-05, DAPL-06, DAPL-09]

duration: ~2h (most of it cargo builds)
completed: 2026-10-16
---

# Phase 23 Plan 04: The ApplyHost seam and its native local implementation Summary

**`ApplyHost` (nine methods) is the engine's one write seam; the local host implements all nine natively, with a journalled, snapshotted `commit` and an idempotent `restore`, and seven conformance rows now pin them for the script hosts to reuse.**

## The ApplyHost method signatures

```rust
#[async_trait::async_trait]
pub trait ApplyHost: Send + Sync {
    async fn probe(&self) -> Result<ProbeReport, CoreError>;
    async fn prepare(&self, spec: &BatchSpec, tree: &RenderedTree) -> Result<Prepared, CoreError>;
    async fn commit(&self, prepared: &Prepared) -> Result<BatchReport, CoreError>;
    async fn restore(&self, record: &RunRecord) -> Result<BatchReport, CoreError>;
    async fn fingerprint(&self, paths: &[String]) -> Result<BTreeMap<String, Option<String>>, CoreError>;
    async fn finalize(&self, record: &RunRecord) -> Result<(), CoreError>;
    async fn read_record(&self, run_id: &RunId) -> Result<Option<RunRecord>, CoreError>;
    async fn fetch_snapshot(&self, run_id: &RunId, dest: &Path) -> Result<BTreeMap<String, String>, CoreError>;
    async fn discard_interrupted(&self, run_id: &RunId) -> Result<(), CoreError>;
}
```

`BatchReport { phase, stale, moved, deleted, removed_dirs, failure }`: a refusal that wrote nothing is `Err`, a pre-image mismatch is `Ok` with `stale`, a failure after the first move is `Ok` with `failure` naming the entry by index.

Value types: `WriteOp { path, sha256, pre }`, `DeleteOp { path, pre }`, `BatchSpec { run_id, gateway, mode, writes, deletes, ledger_new, ledger_pre, journal, record }` (with `touched_paths`, `problem`, `journal_at`), `Prepared` (opaque: run id, staging location, work-list sha256), `ProbeReport { writable, problems }`. Also exported from `host.rs`: `PRERENDERED_PHASES` and `restoring_journal`.

## Conformance rows (local host)

`scenarios()` in `crates/ignition-core/tests/deploy_host_conformance.rs`, each run as `conformance_local_<row>`:

| Row | Method(s) it pins |
|---|---|
| `add_one_file` (tracer) | `prepare`, `commit`, `restore` |
| `fingerprint_is_read_only` | `fingerprint` |
| `finalize_removes_journal_and_staging_keeps_snapshot` | `finalize` |
| `discard_interrupted_touches_no_collection_file` | `discard_interrupted` (and its refusal past `ready`) |
| `fetch_snapshot_copies_with_hashes` | `fetch_snapshot` (0600 on unix) |
| `read_record_absent_is_none` | `read_record` |
| `probe_reports_writable_without_touching_the_collection` | `probe` (unwritable root via mode 0500, skipped as root) |

`every_row_runs_against_the_local_host` fails if a row is added to `scenarios()` without a local test. 23-19 adds the rule rows and fault rows; 23-06 and 23-20 add a `Ctx` for their host.

## Task Commits

1. **Task 1 (tracer), RED:** `f56bce0` (the seam, the work list, `apply_host_for_node`, a failing `conformance_local_add_one_file`; the nine methods returned `Internal`)
2. **Task 1 (tracer), GREEN:** `a0d43d0` (`prepare`, `commit`, `restore`, `local_fs.rs`)
3. **Task 2, RED:** `9d4fd96` (six failing rows)
4. **Task 2, GREEN:** `c3b2be3` (the other six methods)

**Plan metadata:** the final docs commit (SUMMARY, STATE, ROADMAP) follows this file.

## Tracer feedback gate

Auto mode is not active, so the interactive rule asks for a `checkpoint:human-verify` after the tracer commit. This project runs `workflow.human_verify_mode = end-of-phase` and the slice has no human-verifiable surface (a trait and a local-filesystem host), so, as in 23-01, 23-02 and 23-18, execution continued. The tracer's `<verify>` (`conformance_local_add_one_file` and the `deploy::transport::batch` lib tests) passed before the tracer GREEN commit `a0d43d0` and again after Task 2.

## Test results

- `cargo test -p ignition-core --test deploy_host_conformance`: 8 passed (seven rows plus the wiring guard)
- `cargo test -p ignition-core --lib deploy::transport`: 120 passed (6 new in `batch`, existing mirror tests unchanged)
- `cargo test --workspace`: 2264 passed, 0 failed
- `cargo clippy --workspace --all-targets -- -D warnings`: clean
- Acceptance greps: `pub trait ApplyHost` and `pub fn apply_host_for_node` each print one line; no `CoreError::Internal` and no `not_built` remains in `local.rs`

Checked ad hoc and then removed (23-19 owns these rows): a stale pre-image reports `stale` with the journal and partial snapshot gone and the collection unchanged; an existing journal of another run refuses with `deploy_journal_present`; a symlinked directory in the target path is stale and nothing is written through it; a delete plus an emptied directory removes only the directory below `<module>/<type>`; `restore` twice leaves the tree and the `ign-deploy` listing identical; `config-mode.json` is refused in `prepare` before any staging.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The new commit entry point could not be named `commit_blocking`**
- **Found during:** Task 1 (first build)
- **Issue:** `LocalFsTransport` already has a private `commit_blocking` for the Phase 22 `TreeMirror::commit`; a second definition is a duplicate-method error.
- **Fix:** the apply half's is `apply_commit_blocking`; the mirror's is untouched.
- **Files modified:** `crates/ignition-core/src/deploy/transport/local/apply_ops.rs`, `local.rs`
- **Committed in:** a0d43d0

**2. [Rule 3 - Blocking] Clippy `collapsible_if` on four nested `if let` blocks**
- **Found during:** Task 1 (clippy gate)
- **Fix:** let-chains (edition 2024); no behaviour change.
- **Committed in:** a0d43d0

**3. [Rule 2 - Missing critical functionality] `BatchSpec::problem()` and `Prepared` re-validation**
- **Found during:** Task 1 design
- **Issue:** the plan names the types but a host that trusts a spec whose journal, record and write set disagree could snapshot one set of paths and journal another, which breaks recovery.
- **Fix:** `BatchSpec::problem()` checks run id, gateway and mode agreement, sorted-unique disjoint writes and deletes, every path through `mirror_path_problem`, the journal and record touched sets and per-path hashes against the batch, and re-parses each pre-rendered journal and the record through the strict parsers; `commit` re-checks the staged copies of the same documents and the work list's sha256 before its first write.
- **Committed in:** f56bce0, a0d43d0

---

**Total deviations:** 3 auto-fixed (2 blocking, 1 missing critical). **Impact:** no scope change.

**Other plan departures, not rule-driven:**
- The plan puts the guards (pre-image abort, exclusive journal create, the C7a-4 rules, `mirror_path_problem` before any I/O) in this plan and the rows that pin them in 23-19. They are implemented here and spot-checked ad hoc (see Test results); no row for them is committed, which matches the plan's split.
- The plan names `ApplyFault` and `FaultyHost` as 23-19's. Neither exists here; the bounded retry is in place and exercised only by 23-19's fault rows.
- `local.rs` grew by the `ApplyHost` impl and `with_apply` only; the 1,365 lines of blocking work live in `transport/local/apply_ops.rs` and the handle layer in `transport/local_fs.rs`, to keep `local.rs` reviewable. Both are child modules and private to `transport`.
- `requirements-completed` is empty. DAPL-05, DAPL-06 and DAPL-09 describe user-visible apply, rollback and concurrency behaviour; this plan lands only the local host seam that the engine (23-05), the verbs (23-13) and the rule rows (23-19) build on, so no requirement is satisfied yet. `requirements mark-complete` was not run, as in 23-01, 23-02 and 23-18.

## Auth Gates

None.

## Known Stubs

None. `grep` for `todo!`, `unimplemented!`, `TODO` and `FIXME` over the new files finds nothing.

One documented placeholder, not a stub: `CoreError::DeployJournalPresent` raised by the host carries `env: "<env>"` because a host does not know the `--env` name. The engine (23-05) and the verbs (23-12, 23-13) own that value and must replace the field when they surface the error; the message otherwise carries the run id and the journal location.

## Threat Flags

None beyond the plan's register. T-23-14 (paths outside the declared set) is mitigated by `mirror_path_problem` in `BatchSpec::problem()` and `WorkList::validate` before any I/O; T-23-15 (staged copy altered) by `prepare`'s read-back hash and `commit`'s second hash check before the first move; T-23-16 (symlink swap) by `Dir` handles opened `O_DIRECTORY | O_NOFOLLOW` per component on unix, where a swapped component fails the call and a symlink or non-regular live file is a pre-image mismatch. On non-unix the same operations are path-based with a per-component `symlink_metadata` check, the documented residual of Phase 22. That path cannot be compiled or run on this host (the Windows host is `pending`, 23-01); it is recorded as unrun, not a blocker.

## Next Phase Readiness

- 23-19 adds `ApplyFault`, `with_apply_fault` and `FaultyHost`, and the commit and restore rule rows; the retry loop it exercises is `move_with_retry` in `apply_ops.rs`.
- 23-05 drives `ApplyHost` and must build `BatchSpec`s with matching journal and record documents (`BatchSpec::problem()` tells it when they are not) and must fill the `env` of `DeployJournalPresent`.
- 23-06 and 23-20 implement the same trait over `sh` and PowerShell and run `scenarios()` against a `Ctx` of their own; `WorkList` is their input and `journal_file` and `record_file` name the pre-rendered files they copy.

## Self-Check: PASSED

- Created files exist: `deploy/apply/host.rs`, `deploy/transport/batch.rs`, `deploy/transport/local_fs.rs`, `deploy/transport/local/apply_ops.rs`, `tests/deploy_host_conformance.rs`, this SUMMARY.
- Commits `f56bce0`, `a0d43d0`, `9d4fd96`, `c3b2be3` exist in `git log`.
