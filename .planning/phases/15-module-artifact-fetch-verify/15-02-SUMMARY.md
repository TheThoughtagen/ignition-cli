---
phase: 15-module-artifact-fetch-verify
plan: 02
subsystem: infra
tags: [reqwest, sha2, wiremock, github-releases, cache, clippy]

requires:
  - phase: 15-01
    provides: "ModuleSpec/GIT_MODULE/MODULES/spec_for registry, cache_root/module_cache_dir/cached_entry, ModuleFeed::fetch_and_verify's resolve/download/verify/persist miss path, FetchPolicy enum (CacheFirst/Refresh/AcceptUpstreamChange, hit path not yet differentiated), four CoreError module_* variants"
provides:
  - "Streaming size cap in fetch_and_verify: a body exceeding the release's published `size` is aborted mid-stream and refused module_feed_unusable — Content-Length is never trusted in its place"
  - "FetchPolicy hit-path branching: CacheFirst still short-circuits with zero requests; Refresh/AcceptUpstreamChange resolve the release and compare digests before deciding"
  - "CoreError::ModuleDigestChanged (slug module_digest_changed, exit 6) — the fifth and final module error variant, wired through code()/exit_code()/hint()/the module-doc header/EXIT_SLUG_LITERALS/README (Three-Place rule, readme_exit_table_agreement passes)"
  - "assert_cache_empty(root, module_id) shared test helper, replacing the prior ad-hoc cache_is_empty check at all call sites"
  - "crates/ignition-core/tests/live_module_fetch.rs — opt-in, #[ignore]+env-gated live proof that the real Git-2.3.4-signed.modl matches the wiremock fixtures' assumed shape"
affects: ["16"]

actuals:
  tokens: 11094
  tasks: 3
  commits: 5

tech-stack:
  added: []
  patterns:
    - "Cache-hit policy branching resolves the release ONCE and reuses the same expected_digest for both the drift comparison and (on AcceptUpstreamChange) the download-verify-persist path — never two separate verification code paths"
    - "Streaming size cap compares the running byte total against the API-published size field, never Content-Length, after every chunk — returning early drops the stream and the NamedTempFile together"

key-files:
  created:
    - crates/ignition-core/tests/live_module_fetch.rs
  modified:
    - crates/ignition-core/src/module/fetch.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-core/tests/module_fetch_contract.rs
    - README.md

key-decisions:
  - "D-02/D-09 kept as two genuinely separate questions in code: the byte-verification comparison (no policy parameter, no override, ever) is untouched by the new hit-path branch; the upstream-drift comparison is a SEPARATE comparison reachable only from a cache hit under Refresh/AcceptUpstreamChange"
  - "AcceptUpstreamChange's drift-accepted case falls through to the SAME shared download-verify-persist code the miss path uses (not a parallel implementation) — the newly downloaded bytes are verified against the newly published digest exactly as a miss would be"
  - "A pre-existing, out-of-scope clippy::result_large_err failure across the whole workspace (7 sites in ignition-tui) was discovered by the plan's mandated 'cargo clippy --workspace --all-targets' gate. Root-caused via a size probe: CoreError was ALREADY exactly 128 bytes from 15-01's ModuleDigestMismatch (5 Strings); this plan's ModuleDigestChanged is the identical shape and does not increase the enum's max-variant size. Not silently fixed — logged to .planning/WINDOWS.md (entry #3, kind=deviation, status=open) rather than unilaterally boxing CoreError or blanket-allowing the lint, since that is a workspace lint-policy decision outside this plan's file scope"

patterns-established:
  - "Pattern: a fetch/cache seam's FetchPolicy hit-path differentiates by resolving the feed ONCE per policy branch and reusing that single resolution for both a drift comparison and (conditionally) the miss path's download — avoids a second, divergent verification implementation"

requirements-completed: [RMOD-02]

coverage:
  - id: D1
    description: "A downloaded artifact whose sha256 does not match the release's published digest is refused, the bytes are discarded, nothing is cached, and the error names both digests in full (SC-2)"
    requirement: "RMOD-02"
    verification:
      - kind: integration
        ref: "crates/ignition-core/tests/module_fetch_contract.rs#digest_mismatch_refuses_and_caches_nothing"
        status: pass
      - kind: integration
        ref: "crates/ignition-core/tests/module_fetch_contract.rs#truncated_body_is_a_mismatch_not_a_cache_entry"
        status: pass
      - kind: integration
        ref: "crates/ignition-core/tests/module_fetch_contract.rs#mismatch_error_is_not_downgradable"
        status: pass
    human_judgment: false
  - id: D2
    description: "A response body exceeding the release's published size is aborted mid-stream and refused — Content-Length is never trusted in its place"
    requirement: "RMOD-02"
    verification:
      - kind: integration
        ref: "crates/ignition-core/tests/module_fetch_contract.rs#oversized_body_is_aborted_and_refused"
        status: pass
    human_judgment: false
  - id: D3
    description: "A pinned version re-released upstream with a different digest is refused by default under Refresh, naming both digests, with the cached artifact left on disk untouched"
    requirement: "RMOD-02"
    verification:
      - kind: integration
        ref: "crates/ignition-core/tests/module_fetch_contract.rs#refresh_with_changed_digest_refuses_and_keeps_the_cached_artifact"
        status: pass
      - kind: integration
        ref: "crates/ignition-core/tests/module_fetch_contract.rs#refresh_with_unchanged_digest_reuses_cache_without_redownloading"
        status: pass
    human_judgment: false
  - id: D4
    description: "FetchPolicy::AcceptUpstreamChange deliberately accepts a digest change, downloads and verifies the new bytes against the newly published digest, and keeps both the old and new cache entries"
    requirement: "RMOD-02"
    verification:
      - kind: integration
        ref: "crates/ignition-core/tests/module_fetch_contract.rs#accept_upstream_change_downloads_verifies_and_keeps_both"
        status: pass
      - kind: integration
        ref: "crates/ignition-core/tests/module_fetch_contract.rs#accept_upstream_change_still_verifies_the_new_bytes"
        status: pass
    human_judgment: false
  - id: D5
    description: "The default policy still makes zero feed requests for a cached version even after a hypothetical upstream re-release (SC-3 regression guard) — and cached_entry's newest-by-mtime selection stays deterministic across repeated calls once two digests are cached"
    requirement: "RMOD-02"
    verification:
      - kind: integration
        ref: "crates/ignition-core/tests/module_fetch_contract.rs#cache_first_never_consults_the_feed_after_a_rerelease"
        status: pass
      - kind: unit
        ref: "crates/ignition-core/tests/module_fetch_contract.rs#cached_entry_picks_the_newest_deterministically"
        status: pass
    human_judgment: false
  - id: D6
    description: "The real Git-2.3.4-signed.modl release matches the wiremock fixtures' assumed shape — proven by an opt-in, network-gated live fetch that CI never runs and that commits no binary to the repo"
    requirement: "RMOD-02"
    verification:
      - kind: integration
        ref: "crates/ignition-core/tests/live_module_fetch.rs#live_fetches_and_verifies_git_module (run manually with IGNITION_LIVE_MODULE_FETCH=1)"
        status: pass
    human_judgment: false

duration: 48min
completed: 2026-09-20
status: complete
---

# Phase 15 Plan 2: Digest-Mismatch Refusal, Upstream Drift Policy & Live Fixture Proof Summary

**Streaming size cap plus FetchPolicy-driven upstream-drift refusal (module_digest_changed, exit 6) close out the module fetch/verify integrity contract, and an opt-in live fetch confirms the real Git-2.3.4-signed.modl matches the wiremock fixtures exactly.**

## Performance

- **Duration:** 48 min
- **Started:** 2026-09-20T19:08:00Z (approx, first Read of the plan)
- **Completed:** 2026-09-20T19:56:00Z
- **Tasks:** 3
- **Files modified:** 5 (1 created)

## Accomplishments
- A response body exceeding the release's published `size` is aborted mid-stream (never draining the remainder) and refused `module_feed_unusable`, naming the published size — `Content-Length` is deliberately never trusted in its place
- The digest-mismatch refusal (`module_digest_mismatch`) already written in 15-01 is now proven under all three `FetchPolicy` variants and re-proven for a truncated (short) body — there is no policy, flag, or parameter that turns it into a success
- `FetchPolicy::Refresh`/`AcceptUpstreamChange` now differentiate the cache-hit path: an unchanged upstream digest reuses the cache without re-downloading; a changed digest under `Refresh` refuses (`CoreError::ModuleDigestChanged`, exit 6) naming both full digests with the cached artifact left byte-identical; `AcceptUpstreamChange` downloads the new bytes and verifies them against the newly published digest through the SAME shared download-verify-persist path a cache miss uses, then persists them alongside (never over) the old entry
- `CacheFirst` still makes zero feed requests for a cached version — proven directly against a mock server with every route `.expect(0)`-scoped (SC-3 regression guard, since drift detection now exists)
- `cached_entry`'s newest-by-mtime selection is proven deterministic across repeated calls once a version legitimately owns two cache entries (post-override)
- An opt-in, `#[ignore]` + `IGNITION_LIVE_MODULE_FETCH`-gated live test fetched the REAL `Git-2.3.4-signed.modl` from the real GitHub release feed and confirmed it is exactly 7,704,578 bytes with sha256 `b74070346e587b1e1c14ff5093cc70625f7bfa14c2d24b24e7a0888a619b33eb` — the mocked fixture shape in `module_fetch_contract.rs` needed NO correction

## Task Commits

Each task was committed atomically (Tasks 1 and 2 carried `tdd="true"`, so each has a test-first commit followed by an implementation commit):

1. **Task 1 (RED): digest mismatch + size-cap tests** — `4fa85f9` (test)
2. **Task 1 (GREEN): streaming size cap (T-15-10)** — `1d4050f` (feat)
3. **Task 2 (RED): upstream digest drift tests** — `ddd047c` (test)
4. **Task 2 (GREEN): ModuleDigestChanged + FetchPolicy hit-path branching (D-02)** — `e16f080` (feat)
5. **Task 3: opt-in live fetch test** — `5c5321c` (test)

**Plan metadata:** left to the orchestrator's final commit (per the environment constraints in this plan's prompt — no docs commits from this executor).

_Note: an unrelated commit `d4d9ce6` (`docs(quick-260920-iti): plan credential precedence fix`) landed from a concurrent session between the Task 1 RED and GREEN commits — see Deviations._

## Files Created/Modified
- `crates/ignition-core/src/module/fetch.rs` — streaming size cap (Task 1); `FetchPolicy` hit-path branching, doc comment rewritten to state the D-02 policy in user terms, digest-comparison site comment (Task 2)
- `crates/ignition-core/src/error.rs` — `CoreError::ModuleDigestChanged` wired through `code()`/`exit_code()`/`hint()`/the module-doc header table/`exit_code_mapping_enumerated`/`EXIT_SLUG_LITERALS` (Task 2)
- `crates/ignition-core/tests/module_fetch_contract.rs` — `assert_cache_empty` helper (replacing `cache_is_empty`), `mount_release_and_download` helper, 4 Task 1 tests, 6 Task 2 tests (10 new tests total; suite is now 21)
- `README.md` — exit-6 row gains `module_digest_changed`
- `crates/ignition-core/tests/live_module_fetch.rs` — new opt-in live test (Task 3)

## Decisions Made
- D-02/D-09 conflation guard held: the byte-verification comparison (`actual_digest != expected_digest`) takes no policy parameter and has no override, confirmed by `mismatch_error_is_not_downgradable` running the mismatch case under all three policies
- `AcceptUpstreamChange`'s accepted-drift case falls through to the exact same download-verify-persist code a cache miss uses (not a duplicated implementation) — `accept_upstream_change_still_verifies_the_new_bytes` proves the override still refuses when served bytes don't match the newly published digest
- The cache is never destructive on any path — confirmed by the Task 2 structural gate (`remove_file`/`remove_dir`/`truncate(true)` grep is 0 in `src/module`) and by `refresh_with_changed_digest_refuses_and_keeps_the_cached_artifact` reading the old file back byte-identical after a refusal
- A pre-existing, workspace-wide `clippy::result_large_err` failure (7 sites in `ignition-tui`, none in this plan's file scope) was discovered by the mandated `cargo clippy --workspace --all-targets` gate. Root-caused with a temporary size probe (removed before committing): `CoreError` was already exactly 128 bytes because of 15-01's `ModuleDigestMismatch` (5 `String` fields); this plan's `ModuleDigestChanged` is the identical shape and does not increase the enum's size. Logged to `.planning/WINDOWS.md` as an open deviation rather than silently boxing `CoreError` or blanket-allowing the lint — that is a workspace lint-policy decision outside this plan's scope (see Deviations)

## Deviations from Plan

### Concurrent-session collision (not a code deviation, but a real environmental event)

While executing Task 2, a sibling session working in the SAME non-worktree checkout committed `d4d9ce6` (`docs(quick-260920-iti): plan credential precedence fix`) mid-task and, in the process, reverted my in-progress (uncommitted) edits to `crates/ignition-core/src/error.rs` and `README.md` back to their pre-edit state — confirmed via `git diff HEAD` showing zero delta immediately after four `Edit` calls that should have applied. No committed work was lost (my Task 1 commits were already on disk); I re-applied all four `error.rs` edits and the `README.md` edit, verified each with `grep` immediately after writing, and committed promptly to close the collision window. That session's own in-progress work (`crates/ignition-core/src/config/secret.rs`, 221 uncommitted lines) was left untouched throughout — it is not part of this plan and I did not stage or examine it beyond confirming it wasn't mine.

**Recommendation:** this repo is not using worktree isolation (`use_worktrees: false` in `.planning/config.json`) and two sessions were writing to the same checkout concurrently. This is an environment/process concern for the user, not something this executor can resolve.

### Auto-fixed Issues

None — Tasks 1–3 executed exactly as the plan specified. The concurrent-session collision above required re-applying (not fixing) already-designed edits; no Rule 1–4 deviation was needed for the plan's own code.

### Discovered, deliberately NOT auto-fixed

**1. [Rule 4 - Architectural, deferred] Pre-existing `clippy::result_large_err` across the workspace**
- **Found during:** final verification (`cargo clippy --workspace --all-targets -- -D warnings`, required by this plan's `<verification>` section)
- **Issue:** `CoreError` is exactly 128 bytes (clippy's `result_large_err` threshold), driven by `ModuleDigestMismatch` (5 `String` fields, added in 15-01) — the workspace-wide lint check fires at 7 call sites in `ignition-tui` (`context.rs` ×4, `workers/rig_stream.rs`, `lib.rs`) that this plan never touches
- **Why not fixed:** confirmed via a size probe that this plan's own new variant (`ModuleDigestChanged`, also 5 `String`s) does NOT increase `CoreError`'s size — the enum was already at 128 bytes before this plan touched it. The fix (boxing large variant payloads, or a workspace-level `#[allow(clippy::result_large_err)]`) is a lint-policy/representation decision spanning every consumer of `CoreError`, squarely Rule 4 ("Fix requires significant structural modification") rather than something in this plan's declared file scope
- **Action taken:** logged to `.planning/WINDOWS.md` (entry id 3, kind `deviation`, phase `15`, status `open`) with full root-cause detail; task-scoped `cargo clippy -p ignition-core --all-targets -- -D warnings` (this plan's actual per-task `<verify>` command) is clean
- **Files:** none modified for this item (deliberately out of scope)

---

**Total deviations:** 1 discovered-and-deferred (Rule 4, logged not fixed), 0 auto-fixed.
**Impact on plan:** None on this plan's own deliverables — all Task 1–3 `<verify>` commands and structural gates pass exactly as specified. The workspace-wide clippy gate is a pre-existing condition surfaced by this plan's stricter verification scope, not caused by it.

## Issues Encountered
- `cargo test --workspace` (without `--no-fail-fast`) stops at the first failing test BINARY — the `ignition-core` lib unittest binary currently has 18 failures, ALL inside `config::secret`/`config`/`rig`/`session` modules, ALL traced to the concurrent session's in-progress, uncommitted `crates/ignition-core/src/config/secret.rs` edit (root panic: `basic_env_store_prefers_profile_named_vars_then_generic_pair` asserts `"some via named vars"` and fails; the other 17 are `env lock: PoisonError` cascades from that first panic poisoning a shared test mutex). Re-ran with `--no-fail-fast` to get the complete picture: every OTHER binary in the workspace (all of `ignition-cli`'s ~40 test files, all of `ignition-core`'s other ~30 integration test files including `module_fetch_contract` at 21/21 and `live_module_fetch`, and `ignition-tui`'s 231-test suite) reports `ok`. None of the 18 failures are in files this plan touches or in modules this plan's tests exercise. Not fixed (out of scope, actively being edited by another session) — reported here for the record.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Phase 15's integrity contract (SC-1 through SC-4, plus the digest-drift addition) is fully closed: `crates/ignition-core/src/module/fetch.rs` and `crates/ignition-core/src/error.rs` now carry all five module `CoreError` variants, and `module_fetch_contract.rs` has 21 tests covering the happy path, cache authority, loud refusals, digest-mismatch/size-cap refusals, and upstream-drift policy behavior
- `FetchPolicy` (`CacheFirst`/`Refresh`/`AcceptUpstreamChange`) is fully implemented at the library layer and documented for Phase 16 to wire onto a CLI flag — no CLI surface exists yet (D-01 honored: `git diff --stat crates/ignition-cli` is empty across every commit in this plan)
- Two items need attention before/alongside Phase 16: (1) the open `.planning/WINDOWS.md` entry (workspace-wide `clippy::result_large_err`) should be triaged — likely a workspace `[lints]` policy decision — before it accumulates further with Phase 16's own module additions; (2) confirm the concurrent session's `config/secret.rs` work lands cleanly and re-run `cargo test --workspace` clean before shipping this milestone
- The live-gate run's output (7,704,578 bytes / sha256 `b74070346e587b1e1c14ff5093cc70625f7bfa14c2d24b24e7a0888a619b33eb`) matches the plan's recorded identity exactly — nothing for the phase-end human check to reconcile on that point

---
*Phase: 15-module-artifact-fetch-verify*
*Completed: 2026-09-20*

## Self-Check: PASSED

All 6 declared files found on disk; all 5 task commits (`4fa85f9`, `1d4050f`, `ddd047c`, `e16f080`, `5c5321c`) found in git history.
