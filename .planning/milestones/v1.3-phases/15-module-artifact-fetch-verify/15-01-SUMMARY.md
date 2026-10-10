---
phase: 15-module-artifact-fetch-verify
plan: 01
subsystem: infra
tags: [reqwest, sha2, tempfile, wiremock, github-releases, cache]

requires: []
provides:
  - "ignition-core::module — ModuleSpec/GIT_MODULE/MODULES/spec_for registry seam"
  - "ignition-core::module::fetch::ModuleFeed::fetch_and_verify — resolve/download/hash/verify/persist pipeline"
  - "Four new CoreError variants (module_feed_unreachable, module_release_not_found, module_digest_mismatch, module_feed_unusable) at existing exit codes 4/6"
  - "Version-keyed on-disk module cache at <cache_root>/modules/<id>/<version>-<sha256hex>.modl"
affects: ["15-02", "16"]

actuals:
  tokens: 18043
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "Separate reqwest::Client per third-party feed (never reuse ReqwestGatewayApi — its redirect::Policy::none() would swallow a mandatory CDN redirect)"
    - "tempfile::NamedTempFile + tokio::fs::File::from_std(temp.reopen()?) as the async-write bridge for a std-only crate, with .persist() as the ONLY path to the final cache location (verify-then-commit, never write-then-check)"
    - "Streamed download with incremental sha2::Sha256::update() in the same chunk loop as the write — never response.bytes() / Vec<u8> accumulation"

key-files:
  created:
    - crates/ignition-core/src/module/mod.rs
    - crates/ignition-core/src/module/fetch.rs
    - crates/ignition-core/tests/module_fetch_contract.rs
  modified:
    - crates/ignition-core/src/error.rs
    - crates/ignition-core/src/lib.rs
    - crates/ignition-core/Cargo.toml
    - Cargo.toml
    - Cargo.lock
    - README.md

key-decisions:
  - "sha2 promoted from ignition-cli's dev-dependencies into ignition-core's [dependencies] (D-04) — Cargo.lock gains only the dependency edge (1 line), not a new package"
  - "Cache lookup is keyed by version alone (D-08) so a hit never contacts the feed — this is the single mechanism behind both SC-3 (no network on repeat fetch) and SC-4 (offline-with-cache succeeds)"
  - "The insecure-asset-URL-downgrade guard (T-15-05) is unit-tested directly in module/fetch.rs rather than through a live wiremock round trip — this workspace's wiremock has no TLS feature enabled, the same documented limitation already noted in tests/session_contract.rs for ssl_verify"

patterns-established:
  - "Pattern: third-party artifact fetch modules live under ignition-core/src/<domain>/ with a registry seam (spec_for) + a fetch.rs implementing the resolve/verify/cache pipeline, following the client/ sibling-module convention"

requirements-completed: [RMOD-02, RMOD-03]

coverage:
  - id: D1
    description: "A pinned module version resolves its signed .modl asset through the tag-pinned releases endpoint and downloads it through the mandatory release-CDN redirect"
    requirement: "RMOD-02"
    verification:
      - kind: integration
        ref: "crates/ignition-core/tests/module_fetch_contract.rs#tracer_fetches_verifies_and_caches"
        status: pass
    human_judgment: false
  - id: D2
    description: "A verified artifact lands at <cache_root>/modules/<id>/<version>-<sha256hex>.modl by atomic rename after the digest check"
    requirement: "RMOD-02"
    verification:
      - kind: integration
        ref: "crates/ignition-core/tests/module_fetch_contract.rs#tracer_fetches_verifies_and_caches"
        status: pass
    human_judgment: false
  - id: D3
    description: "A second fetch of the same version reuses the cache and issues zero HTTP requests"
    requirement: "RMOD-03"
    verification:
      - kind: integration
        ref: "crates/ignition-core/tests/module_fetch_contract.rs#second_fetch_makes_no_network_request"
        status: pass
    human_judgment: false
  - id: D4
    description: "A populated cache with no reachable feed completes successfully; an empty cache with no reachable feed fails exit 4 module_feed_unreachable naming the feed URL"
    requirement: "RMOD-03"
    verification:
      - kind: integration
        ref: "crates/ignition-core/tests/module_fetch_contract.rs#offline_with_populated_cache_succeeds"
        status: pass
      - kind: integration
        ref: "crates/ignition-core/tests/module_fetch_contract.rs#offline_with_empty_cache_names_the_feed"
        status: pass
    human_judgment: false
  - id: D5
    description: "An unknown version, a missing asset, a digest-less asset, a rate-limited feed, and a hostile version/module-id string all fail loudly and leave the cache untouched"
    requirement: "RMOD-02"
    verification:
      - kind: integration
        ref: "crates/ignition-core/tests/module_fetch_contract.rs#unknown_version_names_version_and_url"
        status: pass
      - kind: integration
        ref: "crates/ignition-core/tests/module_fetch_contract.rs#missing_asset_names_expected_and_present"
        status: pass
      - kind: integration
        ref: "crates/ignition-core/tests/module_fetch_contract.rs#asset_without_digest_is_refused"
        status: pass
      - kind: integration
        ref: "crates/ignition-core/tests/module_fetch_contract.rs#rate_limited_feed_is_not_reported_as_auth"
        status: pass
      - kind: integration
        ref: "crates/ignition-core/tests/module_fetch_contract.rs#unsafe_version_is_refused_before_any_request"
        status: pass
    human_judgment: false
  - id: D6
    description: "An https feed refuses a non-https asset URL before attempting the download (insecure downgrade guard, T-15-05)"
    verification:
      - kind: unit
        ref: "crates/ignition-core/src/module/fetch.rs#tests::https_feed_refuses_non_https_asset"
        status: pass
    human_judgment: true
    rationale: "Could not be wire-tested end-to-end: this workspace's wiremock has no TLS feature enabled, so an https round trip against the mock server isn't possible without either a real TLS listener or danger_accept_invalid_certs, which D-03 forbids for this client. The pure comparison logic is unit-tested directly, but a human should confirm the guard is wired at the correct call site (before the download .send(), verified by code review) since no live-wire proof exists."

duration: 26min
completed: 2026-09-20
status: complete
---

# Phase 15 Plan 1: Module Artifact Fetch & Verify Summary

**`ignition-core::module` fetches a pinned, signed `.modl` release artifact from GitHub, verifies its sha256 against the release's published digest, and atomically caches it — proven end-to-end, cache-authority, and loud-refusal by 11 wiremock/unit tests, with zero clap/CLI surface.**

## Performance

- **Duration:** 26 min
- **Started:** 2026-09-20T13:01Z (approx, first exploration after prior commit)
- **Completed:** 2026-09-20T13:27Z
- **Tasks:** 3
- **Files modified:** 11 (3 created, 8 modified)

## Accomplishments

- Built `ignition-core::module` — a registry seam (`ModuleSpec`, `GIT_MODULE`, `MODULES`, `spec_for`) plus a version-keyed, deterministic on-disk cache (`cache_root`, `module_cache_dir`, `cached_entry`)
- Built `ModuleFeed::fetch_and_verify` — resolves ONLY through the tag-pinned GitHub releases endpoint (never `releases/latest`), follows the mandatory release-CDN redirect on a SEPARATE `reqwest::Client` from `ReqwestGatewayApi`, streams + incrementally hashes the download, and atomically persists via `tempfile::NamedTempFile::persist()` only after the digest check passes
- Added four `CoreError` variants (`module_feed_unreachable` exit 4, `module_release_not_found`/`module_digest_mismatch`/`module_feed_unusable` exit 6), wired through all Three-Place-rule locations (`code()`, `exit_code()`, `hint()`, `endpoint()`, `exit_code_mapping_enumerated`, `EXIT_SLUG_LITERALS`, README exit table) in the same commit that introduced them
- Proved SC-1 (unknown version / missing asset fail loudly naming both requested and resolved values, never falling back to "latest"), SC-3 (a repeat fetch of the same version issues zero HTTP requests, proven by a `MockGuard`'s drop-time `.expect(0)` assertion), and SC-4 (a populated cache completes successfully with nothing listening on the network; an empty cache fails exit 4 naming the feed URL)
- Added input validation (`validate_version`, `validate_module_id`) refusing hostile strings (`/`, `..`, NUL, backslash, over-length) BEFORE any path join or HTTP request — verified by a `.expect(0)`-scoped catch-all mock
- Added rate-limit-aware classification so a GitHub 403 with `x-ratelimit-remaining: 0` is never reported as a credential failure

## Task Commits

Each task was committed atomically:

1. **Task 1: End-to-end "fetch a pinned module and prove its bytes" — one path only (tracer)** - `6242ada` (feat)
2. **Task 2: Cache authority — second fetch makes no network request, offline with a populated cache succeeds (SC-3, SC-4)** - `82db51d` (test)
3. **Task 3: Loud refusals on input — unknown version, missing asset, rate-limited feed, unsafe version string (SC-1)** - `558d2d2` (test)

## Files Created/Modified

- `crates/ignition-core/src/module/mod.rs` - `ModuleSpec`/`GIT_MODULE`/`MODULES`/`spec_for` registry, `cache_root`/`module_cache_dir`/`cached_entry`/`CachedArtifact` cache primitives, `validate_version`/`validate_module_id` input guards
- `crates/ignition-core/src/module/fetch.rs` - `FetchPolicy`, `ArtifactSource`, `FetchedModule`, `ModuleFeed::fetch_and_verify` (the full resolve/download/hash/verify/persist pipeline), `is_insecure_downgrade`
- `crates/ignition-core/tests/module_fetch_contract.rs` - 11 wiremock/env-based integration tests covering the tracer, cache authority, and loud refusals
- `crates/ignition-core/src/error.rs` - four new `CoreError` variants wired through every Three-Place-rule location
- `crates/ignition-core/src/lib.rs` - `pub mod module;` added alphabetically, module-map doc comment extended
- `crates/ignition-core/Cargo.toml` - `sha2 = { workspace = true }` promoted into `[dependencies]`
- `Cargo.toml` - `sha2` workspace-dependency comment updated to reflect the new production consumer
- `README.md` - exit-4/exit-6 rows widened with the new slugs, prose Meaning cells (no backticks, keeps `parse_readme_exit_table` happy)

## Decisions Made

- **`fetch_and_verify` signature (binding for Phase 16):** `async fn fetch_and_verify(&self, spec: &ModuleSpec, version: &str, cache_root: &Path, policy: FetchPolicy) -> Result<FetchedModule, CoreError>` on `ModuleFeed`, built via `ModuleFeed::github()` (production) or `ModuleFeed::for_base(url)` (test seam).
- **Cache path shape:** `<cache_root>/modules/<module_id>/<version>-<sha256hex-64-lowercase>.modl`, exactly matching D-08.
- **`NamedTempFile::reopen()` behaved correctly** — no `spawn_blocking` fallback was needed. `tokio::fs::File::from_std(temp.reopen()?)` worked cleanly for the async chunked write loop on macOS; 15-02 can inherit this choice without re-litigating it.
- **`sha2` promotion** left `Cargo.lock` with exactly a 1-line diff (the new dependency EDGE from `ignition-core` to the already-in-tree `sha2` package) — not a new package. The plan's literal "shows no change" wording for `git diff --stat Cargo.lock` doesn't quite hold for a dependency-table promotion (the edge itself IS the diff); documented here as an expected, minimal, benign consequence rather than a genuine deviation from D-04's intent.
- **`insecure_asset_url_is_refused` test relocated** from the integration test file (as literally named in the plan) to three unit tests inside `module/fetch.rs`'s own `#[cfg(test)]` module, testing the extracted pure function `is_insecure_downgrade` directly — see Deviations below.

## Deviations from Plan

### Auto-fixed Issues (Rule 1/3 — structural gate false positives)

**1. [Rule 3 - blocking issue] `Policy::none` structural gate had no comment exclusion**
- **Found during:** Task 1 verification
- **Issue:** The plan's structural gate `grep -rn "Policy::none" crates/ignition-core/src/module | wc -l` (must print 0) has no `grep -v` comment exclusion, unlike the other two gates in the same block. My `fetch.rs` module doc comment explained D-03 by literally quoting `Policy::none()`, which the gate then flagged even though it was prose, not code.
- **Fix:** Reworded the doc comment to describe the gateway client's redirect suppression without using the literal substring `Policy::none`.
- **Files modified:** `crates/ignition-core/src/module/fetch.rs`
- **Verification:** `grep -rn "Policy::none" crates/ignition-core/src/module | wc -l` → `0`
- **Committed in:** `6242ada`

**2. [Rule 3 - blocking issue] `.bytes()` structural gate collided with `str::bytes()`**
- **Found during:** Task 1 verification
- **Issue:** The gate `grep -rn "fs::File::create\|fs::write\|\.bytes()" crates/ignition-core/src/module | grep -v '^[^:]*:[0-9]*: *//' | wc -l` (must print 0, meant to catch `response.bytes()` whole-body buffering, Pitfall 2) also matched two unrelated `hex.bytes()`/`s.bytes()` calls — `str::bytes()` iterators used for hex-digit validation, nothing to do with HTTP response buffering.
- **Fix:** Rewrote both hex-digit checks to use `.chars()` instead of `.bytes()`, which is semantically equivalent for ASCII hex validation and doesn't collide with the gate's literal-substring match.
- **Files modified:** `crates/ignition-core/src/module/fetch.rs`, `crates/ignition-core/src/module/mod.rs`
- **Verification:** the same grep now prints `0`; `cargo test -p ignition-core --lib module::` still green
- **Committed in:** `6242ada`

---

**Total deviations:** 2 auto-fixed (Rule 3), plus 1 documented test-relocation adaptation (see below) and 1 documented benign lockfile-diff clarification (see Decisions).
**Impact on plan:** Both auto-fixes were pure test-infrastructure/wording corrections with zero behavioral change to the shipped code — no scope creep.

### Test relocation: `insecure_asset_url_is_refused`

The plan names this test in `tests/module_fetch_contract.rs`'s Task 3 `<behavior>` block, driven end-to-end via wiremock ("when the feed base is https, an asset whose `browser_download_url` is plain http → `module_feed_unusable`, no download attempted"). This cannot be exercised as a live wiremock round trip in this workspace: `wiremock` here has no `tls` feature enabled (confirmed absent from the workspace `Cargo.toml`), and the guard by design only fires when the feed's `api_base` scheme is actually `https` — which means the RELEASE-LOOKUP request itself (the one step before the guard's asset check) would need to succeed over a real TLS connection to trigger the code path at all. A plain-HTTP wiremock server cannot serve that, and this client (D-03) never sets `danger_accept_invalid_certs`, so a self-signed workaround was also not viable.

This is the exact same limitation this codebase already documents for TLS-adjacent behavior it cannot wire-test: `tests/session_contract.rs`'s module doc states "the WIRE-level https-skip proof needs wiremock's optional `tls` feature, which this workspace does not enable — the builder behavior itself is pinned by the client's own tests." Following that precedent, I extracted the guard's comparison into a small pure function (`is_insecure_downgrade(feed_scheme, asset_scheme) -> bool`) and unit-tested it directly inside `module/fetch.rs`'s own `#[cfg(test)]` module (3 tests: https-refuses-http, https-allows-https, http-allows-any), rather than inventing a flaky or misleading integration test. The guard's call site (before the download request's `.send()`, so "no download attempted" holds structurally) is verifiable by direct code review of `fetch_and_verify`.

## Issues Encountered

None beyond the two structural-gate false positives documented above.

## User Setup Required

None — no external service configuration required. `ModuleFeed::github()` talks to the public, unauthenticated GitHub API; no token or secret is involved in this plan.

## Next Phase Readiness

- **Ready for 15-02:** `FetchPolicy` (`CacheFirst`/`Refresh`/`AcceptUpstreamChange`) is already defined with its miss-path behavior implemented identically across all three variants, per D-09 — 15-02 only needs to differentiate the HIT path (digest-drift re-verify/accept) without touching this plan's signature. The `ModuleDigestMismatch` refusal branch exists and is reachable (`temp` drops on early return before persist), ready for 15-02's drift test to exercise it further.
- **Ready for Phase 16:** `fetch_and_verify`'s signature, the cache path shape, and all four slugs are stable and documented above for direct consumption by the CLI wiring phase.
- **No blockers.**

---
*Phase: 15-module-artifact-fetch-verify*
*Completed: 2026-09-20*

## Self-Check: PASSED

All created files verified present on disk; all three task commit hashes (`6242ada`, `82db51d`, `558d2d2`) verified present in `git log`.
