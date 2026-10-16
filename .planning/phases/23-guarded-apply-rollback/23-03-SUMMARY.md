---
phase: 23-guarded-apply-rollback
plan: 03
subsystem: deploy
tags: [run-store, journal, run-record, fingerprint, git-ignore-guard, rust, tdd]

requires:
  - phase: 23-guarded-apply-rollback
    provides: 23-02 checkpoint outcome items 4, 5 and 6 (touched set, on-box layouts, run-dir rule) and DeployRunInvalid, DeployPreflightFailed
  - phase: 22-transport-plan-pull-verify
    provides: Ledger strict-parse discipline, node::classify, mirror_path_problem, sha256 helpers
provides:
  - deploy::apply::run_store (RunId, RunDir, EventLog, IgnoreStatus, ignore_status, run_dir_rel, RUN_DIR_ROOT)
  - deploy::apply::fingerprint (fingerprint, FINGERPRINT_ABSENT, LEDGER_KEY) with a pinned golden
  - deploy::apply::journal (Journal v1, JournalPhase, JOURNAL_FILE, JOURNAL_MAX_BYTES)
  - deploy::apply::record (RunRecord v1, RecordState, TouchedEntry, RECORD_FILE, RECORD_MAX_BYTES)
  - actions::workspace::utc_parts (shared civil-from-days), now_unix_ms, rfc3339_now_utc made pub(crate)
affects: [23-18, 23-04, 23-05, 23-09, 23-11, 23-12, 23-13, 23-15]

actuals:
  tokens: 18700
  tasks: 2
  commits: 6

tech-stack:
  added: []
  patterns:
    - "RunDir::create runs the git-ignore guard itself, so no caller can create a run directory for secrets at a path git would commit"
    - "journal.rs holds the strict-parse helpers (parse_strict, check_header, check_touched, refuse) that record.rs reuses; both formats refuse the same way"
    - "a *_with(git, ...) doc-hidden seam names the git program, the same shape as the other transport test seams"

key-files:
  created:
    - crates/ignition-core/src/deploy/apply/run_store.rs
    - crates/ignition-core/src/deploy/apply/fingerprint.rs
    - crates/ignition-core/src/deploy/apply/journal.rs
    - crates/ignition-core/src/deploy/apply/record.rs
  modified:
    - crates/ignition-core/src/deploy/apply/mod.rs
    - crates/ignition-core/src/actions/workspace.rs

key-decisions:
  - "The git-ignore guard lives inside RunDir::create (not left to the caller): a NotIgnored or GitUnavailable answer is DeployPreflightFailed with the single check id run_dir_not_ignored, and nothing is created"
  - "git exit codes other than 0, 1 and 128 (and a spawn failure) all map to GitUnavailable, so the guard fails closed"
  - "RunDir::create returns deploy_run_invalid when the path exists; RunDir::create_fresh draws up to three ids, so an existing run directory is never written into"
  - "IgnoreStatus::as_record gives the run_dir_git strings (ignored, not_ignored, not_a_repo, git_unavailable) that 23-09 writes into run.json"
  - "A platform with neither unix modes nor icacls refuses to create a run directory rather than create an unprotected one"
  - "deploy_run_invalid from a journal or record parse carries the file's own run_id when it is a valid id, else `unknown`, and puts the location first in the problem text"
  - "Records and journals require every key present (an absent hash is an explicit null), as the Ledger does; post_fingerprint and rolled_back_by are always written, null when unset"

requirements-completed: []
requirements-advanced: [DAPL-05, DAPL-08, DAPL-09, DAPL-10]

coverage:
  - id: D1
    description: "A run id is a UTC timestamp plus six random hex, matches the staging rule, sorts by start time, and two ids in one second differ"
    requirement: "DAPL-10"
    verification:
      - kind: test
        ref: "deploy::apply::run_store::tests::{run_id_shape, run_ids_sort_by_time, run_ids_in_one_second_differ_by_suffix}"
        status: pass
    human_judgment: false
  - id: D2
    description: "The run directory is created exclusively at 0700 (parents too), files at 0600, run.json and events.ndjson once, events with seq 1, 2"
    requirement: "DAPL-10"
    verification:
      - kind: test
        ref: "deploy::apply::run_store::tests::{run_dir_created_private, event_log_refuses_reserved_keys_and_a_second_open, write_json_refuses_a_name_with_a_separator}"
        status: pass
    human_judgment: false
  - id: D3
    description: "The git-ignore guard: ignored proceeds, not ignored and git unavailable refuse with nothing written, not a repo proceeds and is recorded"
    requirement: "DAPL-10"
    verification:
      - kind: test
        ref: "deploy::apply::run_store::tests::{ignore_guard_matrix, a_run_dir_is_refused_with_nothing_written_unless_git_ignores_it, not_a_repo_is_allowed_and_recorded}"
        status: pass
    human_judgment: false
  - id: D4
    description: "An existing run directory is never reused; eight concurrent starts get eight directories; a traversing gateway name is refused before any I/O"
    requirement: "DAPL-10"
    verification:
      - kind: test
        ref: "deploy::apply::run_store::tests::{run_dir_exclusive, run_dir_exclusive_gives_up_after_three_attempts, two_runs_started_at_once_get_two_directories, a_traversing_gateway_is_refused_before_any_io}"
        status: pass
    human_judgment: false
  - id: D5
    description: "The fingerprint is byte-ordered, independent of insertion order, pinned by a golden derived in Python, and ledger.json sorts like any key"
    requirement: "DAPL-05"
    verification:
      - kind: test
        ref: "deploy::apply::fingerprint::tests::{fingerprint_golden, the_ledger_sorts_by_bytes_like_any_other_key, non_ascii_names_sort_by_utf8_bytes_not_utf16_units, an_empty_set_has_a_fingerprint}"
        status: pass
    human_judgment: false
  - id: D6
    description: "Journal v1 and RunRecord v1 round-trip byte-identically, have a pinned layout, and refuse every listed malformed class with deploy_run_invalid naming the location"
    requirement: "DAPL-09"
    verification:
      - kind: test
        ref: "deploy::apply::journal::tests::* and deploy::apply::record::tests::* (78 apply tests in all)"
        status: pass
    human_judgment: false
  - id: D7
    description: "The Windows owner-only ACL on the run directory (icacls /inheritance:r /grant:r USER:(OI)(CI)F)"
    requirement: "DAPL-10"
    verification:
      - kind: test
        ref: "deploy::apply::run_store::tests::run_dir_created_private_windows (cfg(windows), compiled and run only by CI)"
        status: unrun
    human_judgment: true
    human_judgment_reason: "This host is macOS and the x86_64-pc-windows-msvc standard library is not installed, so the Windows branch and its ACL read-back test were written but not compiled or run here"

duration: 55min
completed: 2026-10-16
status: complete
---

# Phase 23 Plan 03: Run Store, Journal and Run Record Formats Summary

**A private, git-ignored run directory with a time-sorted id and an append-only events log, plus strict, round-tripped v1 formats for the on-box journal and run record and a pinned touched-set fingerprint.**

## Performance

- **Duration:** about 55 min (most of it cargo builds)
- **Tasks:** 2 of 2
- **Files:** 4 created, 2 modified

## Accomplishments

- `RunId`: `YYYYMMDDTHHMMSSZ-xxxxxx` from `utc_parts` plus three `getrandom` bytes; `parse` accepts the whole staging rule and refuses empty, 129 characters, a leading dot, `/`, `\`, NUL, `..` and a space, so an id read from a file or a command line cannot become a traversal (T-23-10).
- `RunDir::create` at `<repo>/.ign/deploy/<gateway>/<run-id>/`: guard first, parents at 0700 when new, the directory itself created exclusively and set to 0700 explicitly. `create_private_file` and `write_json` create files exclusively at 0600, so `run.json` and `result.json` are written once. `create_fresh` draws up to three ids.
- `EventLog`: `events.ndjson`, one JSON object per line with `seq` 1, 2, 3, an RFC3339 `t`, the `event` name and the caller's fields; the reserved keys cannot be overridden.
- `fingerprint` over `path NUL (sha256 or "-") LF` in byte order. A separate test shows a UTF-8 byte order that differs from UTF-16 order, which is why scripts never compute it.
- `Journal` and `RunRecord` v1 on the 23-02 item 5 layout, with the `Ledger` discipline (see the verification list below), plus `JournalPhase::before_first_move`, `RunRecord::pre_map` and `post_map`.
- `actions::workspace::utc_parts` extracted from the RFC3339 formatter; the formatter's output is unchanged (its existing tests pass, and a new test pins `utc_parts`).

## Pinned fingerprint golden

Over the map `{ignition/tag-provider/default/tags.json: sha256("one"), ledger.json: sha256("ledger"), com.example/views/Main/view.json: absent}`:

```
ca50ebbf57b1933c2022ffd4b9ebb8ff8b0d8b10cedbcf98e1ea34f9e4b63e79
```

The digest was derived independently in Python from the listing format, then pinned in `fingerprint_golden`. An empty map gives the SHA-256 of the empty string.

## Layout difference from the checkpoint

None. Items 4 and 5 of the 23-02 `## Checkpoint outcome` are implemented as chosen. Two details the item left open, decided here and recorded so later plans do not guess:

- `post_fingerprint` and `rolled_back_by` are always present in `record.json`, written as `null` when unset; the parser requires every key.
- A `touched` entry always carries all of `path`, `pre_sha256` and `post_sha256`; an absent file is an explicit `null` hash.

## Task Commits

1. **Task 1 (tracer), RED:** `09a271b` (failing run-store tests, `utc_parts` extracted)
2. **Task 1 (tracer), GREEN:** `e8a7402` (run id, guarded private run directory, events log)
3. **Task 1, Windows branch:** `e06ba89` (owner-only ACL via `icacls`, its own small commit as the plan asked)
4. **Task 2, RED:** `90966c2` (failing journal, record and fingerprint tests)
5. **Task 2, GREEN:** `41f2c0f` (journal v1, run record v1, fingerprint)

**Plan metadata:** the final docs commit (SUMMARY, STATE, ROADMAP) follows this file.

## Tracer feedback gate

Auto mode is not active, so the interactive rule would stop after the tracer commit with a `checkpoint:human-verify`. This project runs `workflow.human_verify_mode = end-of-phase` and the orchestrator directed continuous sequential execution, the same call as 23-01 and 23-02. The tracer's own `<verify>` (`cargo test -p ignition-core --lib deploy::apply::run_store`, `actions::workspace`, clippy `-D warnings`) passed before the tracer commit `e8a7402` and again at the end of the plan. The slice has no human-verifiable surface beyond the tests, which use a real `git init` repo.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical functionality] The git-ignore guard is inside `RunDir::create`**
- **Found during:** Task 1 design
- **Issue:** the plan names `ignore_status` and `RunDir::create` separately and says the guard "runs first". Leaving the call to every caller would let a later engine plan skip it, which T-23-09 forbids.
- **Fix:** `RunDir::create` calls the guard itself and raises `deploy_preflight_failed` with check `run_dir_not_ignored` (23-02 item 6) for `NotIgnored` and `GitUnavailable`. `RunDir::git_status` returns what it saw for `run_dir_git` in `run.json`.
- **Files modified:** `crates/ignition-core/src/deploy/apply/run_store.rs`
- **Committed in:** e8a7402

**2. [Rule 2 - Missing critical functionality] A platform with no owner-only mechanism refuses**
- **Found during:** Task 1, Windows commit
- **Issue:** with the Windows branch split into its own commit, the non-unix path needed a defined behaviour; silently skipping the permission step would create a secrets directory anyone can read.
- **Fix:** `cfg(windows)` runs `icacls`; any other non-unix target returns `Unsupported`, which refuses the run.
- **Committed in:** e06ba89

**3. [Rule 3 - Blocking] `rfc3339_now_utc` made `pub(crate)` and `now_unix_ms` added**
- **Found during:** Task 1
- **Issue:** the events log and the run id need the same clock and formatter as the workspace manifest; both were private to `actions/workspace.rs`.
- **Fix:** `now_unix_ms` (new) and `rfc3339_now_utc` are `pub(crate)`; the existing workspace tests are unchanged and pass. No new dependency.
- **Committed in:** 09a271b

---

**Total deviations:** 3 auto-fixed (2 missing critical, 1 blocking). **Impact:** no scope change.

**Other plan departures, not rule-driven:**
- The plan lists `create(repo, gateway, run_id)` only. `create_fresh` (the three-attempt draw) and `create_private_file` were added because the plan's own truths (adjacency, 23-09's `.gwbk` write, the artifact inventory row naming `create_private_file`) need them.
- `requirements-completed` is empty: this plan lands formats and a store with no user-visible command, the same call as 23-01 and 23-02. DAPL-05, DAPL-08, DAPL-09 and DAPL-10 stay open until the engine, rollback and verb plans (23-04 to 23-21) deliver the behaviour.

## Auth Gates

None.

## Known Stubs

None. `grep` for `todo!`, `unimplemented!`, `TODO` and `FIXME` over `crates/ignition-core/src/deploy/apply/` finds nothing. The RED commits (`09a271b`, `90966c2`) contained stub bodies by design; both are replaced in the GREEN commits.

One unrun check is recorded: the Windows ACL read-back test (`run_dir_created_private_windows`) compiles and runs only on a Windows CI runner. See the broken-windows entry below.

## Threat Flags

None beyond the plan's register. T-23-09 (guard first, 0700/0600, owner-only ACL), T-23-10 (`RunId::parse`, `component_problem` on the gateway and on file names), T-23-11 and T-23-12 (strict parse, 8 MiB cap before parsing, `node::classify`) are mitigated and tested. T-23-13: `EventLog::append` takes caller-supplied fields; it cannot see what a field means, so the value-free rule is documented in its module doc and enforced by the 23-15 canary scan, not here.

## Verification

- `cargo test -p ignition-core --lib deploy::apply`: 78 passed (14 run_store, 4 fingerprint, 21 journal, 20 record, 19 gates)
- `cargo test -p ignition-core --lib actions::workspace`: 10 passed (the time helpers' output is unchanged)
- `cargo clippy --workspace --all-targets -- -D warnings`: clean
- Full `cargo test -p ignition-core -p ignition-cli --no-fail-fast`: 1960 passed, 0 failed, 83 test binaries
- `grep -n 'pub const RUN_DIR_ROOT: &str = ".ign/deploy"'`, `JOURNAL_FILE` and `RECORD_FILE` each print one line.
- Not verified here: the `cfg(windows)` code (no Windows standard library installed; `cargo check --target x86_64-pc-windows-msvc` fails with `can't find crate for core`). Research Pitfall 13 applies; CI compiles it.

## Next Phase Readiness

- 23-18 builds `journal_rel`, `snapshot_rel`, `record_rel` and `TreeReader::read_journal` on `JOURNAL_FILE`, `RECORD_FILE` and `Journal::parse`.
- 23-09 writes `run.json` with `RunDir::write_json` and records `IgnoreStatus::as_record` as `run_dir_git`; it streams the `.gwbk` through `create_private_file`.
- 23-12 adds `recovery_action` to `journal.rs` and reads `JournalPhase::before_first_move`.
- 23-04 and 23-05 take `fingerprint`, `LEDGER_KEY` and the `RunRecord` maps.

## Self-Check: PASSED

- Created files exist: `deploy/apply/{run_store,fingerprint,journal,record}.rs`, this SUMMARY.
- Commits `09a271b`, `e8a7402`, `e06ba89`, `90966c2`, `41f2c0f` exist in `git log`.
