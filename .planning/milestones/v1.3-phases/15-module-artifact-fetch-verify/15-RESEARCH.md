# Phase 15: Module Artifact — Fetch & Verify - Research

**Researched:** 2026-09-20
**Domain:** GitHub release-asset fetch, streaming SHA-256 verification, filesystem cache (Rust / reqwest 0.13 / tokio)
**Confidence:** HIGH

## Summary

This phase adds `ign`'s first dependency on a third-party artifact feed: pinned, signed
`.modl` releases published on `github.com/WhiskeyHouse/ignition-git-module`. Every fact
needed to de-risk this — repo visibility, asset URL shapes, redirect behavior, rate limits,
digest format — was verified live against the real GitHub API and CDN during this research
session (not assumed from training data), because the phase's whole point is "prove the
bytes are right before anything depends on them."

The repo is **public** (`"private": false`, confirmed live). `browser_download_url` is the
right asset URL to fetch: it 302-redirects once to `release-assets.githubusercontent.com`
and serves raw bytes with no `Accept` header required — reqwest 0.13's default
`ClientBuilder` (redirect limit 10) follows this automatically, no explicit redirect
configuration needed. This is a **different** client instance from the existing
`ReqwestGatewayApi`, whose `build_client()` deliberately sets
`redirect::Policy::none()` (`crates/ignition-core/src/client/mod.rs:919`) — that policy
must NOT be reused for artifact fetch, or the GitHub redirect will surface as an
unclassified 302.

Nearly everything this phase needs is already in the dependency tree: `reqwest` (workspace,
`stream` feature already enabled), `sha2` (workspace-pinned at `0.10`, but currently only a
**dev-dependency** in `ignition-cli` — it must be promoted into `ignition-core`'s production
`[dependencies]`, following the exact precedent `tempfile` set in 05-02), `tempfile`
(already promoted, already the crate's atomic-temp-file idiom), and `directories`
(`ProjectDirs::cache_dir()`, confirmed via docs.rs, unused so far — only `config_dir()` is
called today). **No new crates are required for this phase.**

**Primary recommendation:** Build a small `ignition-core::module` (or `rig_module`) module
housing a `fetch_and_verify(release_url_base, module, version) -> Result<PathBuf, CoreError>`
function that (1) GETs the tagged release from `api.github.com`, (2) reads the asset's
`browser_download_url` + `digest` fields, (3) on cache miss streams the download to a
`tempfile::NamedTempFile` inside the cache dir while hashing incrementally with
`sha2::Sha256`, (4) compares the finished digest against the release's published one, and
(5) only on match calls `.persist()` to atomically rename into
`<cache_dir>/modules/<module>/<version>-<digest12>.modl`. Add two or three new `CoreError`
variants (never new exit codes) for the two GitHub-specific failure shapes SC-1/SC-2 need
that the existing taxonomy's wording doesn't fit.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Release/asset resolution (GitHub API call) | API/Backend (ignition-core lib) | — | Same tier as the existing `GatewayApi` HTTP client — a plain async function over `reqwest`, no CLI/TUI concerns |
| Streaming download + incremental hash | API/Backend (ignition-core lib) | Filesystem/Storage | Byte handling must never surface to CLI/TUI layers; matches `download_to_file`'s existing streaming pipeline (`client/mod.rs:670-722`) |
| Cache read/write (version+digest keyed) | Filesystem/Storage | API/Backend | Physically a `directories::ProjectDirs::cache_dir()` subtree; logically owned by the same core module that fetches, so cache-hit short-circuits the network call entirely |
| Digest verification / refusal | API/Backend (ignition-core lib) | — | Pure computation + `CoreError` construction — no I/O beyond the already-streamed bytes |
| CLI/TUI surfacing of fetch progress or errors | Browser/Client analog: CLI binary + TUI | — | Out of scope for Phase 15 per the roadmap (no CLI verb is wired yet — `RMOD-01`/injection is Phase 16); this phase's surface is a **library function** exercised directly by tests |

## Package Legitimacy Audit

**No new external packages are proposed by this research.** Every crate needed is already
declared in the workspace (`Cargo.toml` root, lines 11-54):

| Package | Registry | Status in this repo | Verdict | Disposition |
|---------|----------|----------------------|---------|-------------|
| `reqwest` | crates.io | Already a workspace dep, `stream` feature already enabled | OK | Reuse — build a SEPARATE `Client` instance (see Q1) |
| `sha2` | crates.io | Already `workspace.dependencies` (`0.10`), but only wired into `ignition-cli`'s `[dev-dependencies]` today (`crates/ignition-cli/Cargo.toml:50`) | OK | **Promote** into `ignition-core`'s `[dependencies]` — same move `tempfile` made in 05-02 (`crates/ignition-core/Cargo.toml:28`), not a new dependency |
| `tempfile` | crates.io | Already `ignition-core` production dep (`Cargo.toml:28`) | OK | Reuse for the streamed-temp-file-then-persist cache write |
| `directories` | crates.io | Already `ignition-core` production dep, only `config_dir()` used so far | OK | Add `cache_dir()` call — same `ProjectDirs` handle |
| `thiserror` | crates.io | Already `ignition-core` production dep | OK | New `CoreError` variants only |

**Packages removed due to [SLOP] verdict:** none.
**Packages flagged as suspicious [SUS]:** none.

The Package Legitimacy Gate (`gsd_run query package-legitimacy check`) was not invoked
because there is nothing new to check — every dependency above already ships in the
workspace lockfile and was vetted in earlier phases (see the inline `Cargo.toml` comments
cited above, which are the project's own provenance notes).

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `reqwest` | 0.13 (workspace-pinned) [VERIFIED: root Cargo.toml:15] | HTTP client for both the GitHub API call and the CDN asset download | Already the project's one HTTP client; `stream` feature already enabled for `bytes_stream()` |
| `sha2` | 0.10 (workspace-pinned) [VERIFIED: root Cargo.toml:54] | Incremental SHA-256 over the streamed body | Already workspace-pinned; RustCrypto's `sha2` is the de facto standard pure-Rust SHA-2 implementation and is already used for the project's fidelity-oracle tests |
| `tempfile` | 3.27 (workspace-pinned) [VERIFIED: root Cargo.toml + ignition-core/Cargo.toml:28] | Streamed download target; atomic `.persist()` into the cache | Already promoted to production in `ignition-core` for exactly this download-then-atomically-commit shape (05-02 resource-surgery precedent) |
| `directories` | 6.0 (workspace-pinned) [VERIFIED: root Cargo.toml:16] | `ProjectDirs::cache_dir()` for the module cache root | Already the project's one cross-platform directory resolver; `cache_dir()` is a sibling getter to the already-used `config_dir()` [CITED: docs.rs/directories] |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `thiserror` | 2.0 (workspace-pinned) | New `CoreError` variants for GitHub-specific failure shapes | Every new failure class in this codebase is a `thiserror` variant, never a raw string |
| `serde` / `serde_json` | workspace-pinned | Deserialize the GitHub releases-by-tag JSON response | Already the project's one JSON stack |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Hand-rolled GitHub API client | `octocrab` | An entire GitHub API SDK for ONE endpoint (`releases/tags/{tag}`) — violates the lean-dependency-tree constraint; `reqwest` + a 5-field `#[derive(Deserialize)]` struct is ~20 lines |
| `sha2` streaming hash | `ring::digest` | `ring` is already in the tree transitively (TLS crypto provider), but its digest API is lower-level and the project has zero precedent for using `ring` outside TLS; `sha2` is already the project's chosen SHA library for hash work (11-06) |
| `directories::ProjectDirs::cache_dir()` | A hand-rolled `~/.cache/ignition-cli` path | Reinvents exactly what `directories` already solves correctly cross-platform (and the project already depends on it for `config_dir()`) |

**Installation:** No `cargo add` needed. Two `Cargo.toml` edits:
1. `crates/ignition-core/Cargo.toml`: add `sha2 = { workspace = true }` to `[dependencies]` (promoting it out of `ignition-cli`'s dev-only usage).
2. No other manifest changes — `reqwest`, `tempfile`, `directories`, `thiserror` are already `ignition-core` production deps.

**Version verification:** All four core crates are pinned in the workspace root
`Cargo.toml` (read directly, lines 11-54) — no registry lookup was needed since these
are not new dependencies. `sha2 = "0.10"` at root; current-tree resolution was not
re-verified against crates.io for this research (no version bump proposed).

## Architecture Patterns

### System Architecture Diagram

```
 caller (test / future Phase-16 `rig up`)
   │
   │ fetch_and_verify(github_base, "git", "2.3.4")
   ▼
 ┌─────────────────────────────────────────────────────────┐
 │ ignition-core::module (new)                              │
 │                                                           │
 │  1. cache lookup: <cache_dir>/modules/git/                │
 │     scan for an entry matching version "2.3.4"            │
 │     ├─ HIT  ───────────────────────────────► return path  │  (SC-3, SC-4: NO network)
 │     └─ MISS                                               │
 │         ▼                                                 │
 │  2. GET api.github.com/repos/OWNER/REPO/releases/tags/{v} │
 │     (separate reqwest::Client — default redirect policy)  │
 │     ├─ transport/DNS failure ──► CoreError::Network        │ (offline, exit 4)
 │     ├─ HTTP 404 ───────────────► CoreError::<ReleaseNotFound> │ (unknown version, exit 6)
 │     └─ HTTP 200 → parse assets[], find by name/pattern     │
 │         ▼                                                 │
 │  3. GET asset.browser_download_url                        │
 │     (302 → release-assets.githubusercontent.com → 200)    │
 │     stream response.bytes_stream() →                       │
 │       tokio::io::AsyncWriteExt into a tempfile::NamedTempFile │
 │       + sha2::Sha256::update() per chunk                   │
 │         ▼                                                 │
 │  4. finalize hash, compare to asset.digest ("sha256:…")    │
 │     ├─ MISMATCH ──► discard temp file, CoreError::<DigestMismatch> │ (exit 6, names both hashes)
 │     └─ MATCH ──► temp_file.persist(cache_dir/modules/git/2.3.4-<digest12>.modl) │
 │         ▼                                                 │
 │  5. return the persisted path                              │
 └─────────────────────────────────────────────────────────┘
```

### Recommended Project Structure
```
crates/ignition-core/src/
├── module/               # NEW — mirrors rig/, webdev/ as a sibling top-level domain
│   ├── mod.rs            # module registry types (id, pinned version) — Phase 16 extends this
│   └── fetch.rs           # fetch_and_verify(), release/asset structs, cache key logic
└── error.rs               # + 2-3 new CoreError variants (see Q5)
```
This mirrors the existing `client/`, `rig/`, `webdev/` sibling-module convention
[VERIFIED: crates/ignition-core/src/lib.rs:24-32 — `pub mod actions; pub mod client; pub mod
config; pub mod e2e; pub mod error; pub mod output; pub mod poll; pub mod rig; pub mod
session; pub mod webdev;`]. Naming it `module/` (not `rig_module/`) reads naturally with
Phase 16's "module registry" terminology from the design doc §4.1 ("It is 'rig modules'").
This is a naming **recommendation**, not a verified requirement — flagged for the planner.

### Pattern 1: Separate reqwest::Client for third-party fetch (never reuse the gateway client)
**What:** Build a fresh `reqwest::Client` for GitHub calls, distinct from
`ReqwestGatewayApi`'s client.
**When to use:** Any time code talks to something that is NOT the target Ignition gateway.
**Why:** `build_client()` (`crates/ignition-core/src/client/mod.rs:913-928`) hard-codes
`redirect::Policy::none()` specifically because an uncommissioned gateway 302s everything to
`/welcome` and following that would misrender the wizard HTML as a 200
[VERIFIED: crates/ignition-core/src/client/mod.rs:916-919 — `// Never follow redirects: an
uncommissioned gateway 302s everything // to /welcome and the follow would render the wizard
HTML as a 200 // (02-RESEARCH Pitfall 6). classify() maps the 3xx instead. .redirect(reqwest::redirect::Policy::none())`].
GitHub's asset download is the OPPOSITE case — the redirect to the CDN is mandatory and
benign. Reusing the gateway client's policy would break the fetch; building a second client
with reqwest's untouched default (`redirect::Policy::default()` = `limited(10)`) is correct
and requires zero explicit configuration [CITED: Context7 `/seanmonstar/reqwest`,
`api-reference/client-builder.md` — `redirect` default = `redirect::Policy::default()` (max
10); `api-reference/redirect.md` — "By default a `Client` follows redirects up to a maximum
chain of 10 hops."]. Both clients must still call the existing
`ignition_core::client::install_crypto_provider()` [VERIFIED: crates/ignition-core/src/client/mod.rs:903-911]
before their first request, since the workspace builds `reqwest` with `rustls-no-provider`
(root Cargo.toml:15) — a raw client without the provider installed panics on first TLS
handshake per that function's own doc comment.
**Example:**
```rust
// Pattern derived from crates/ignition-core/src/client/mod.rs:913-928 (build_client),
// adapted: default redirect policy instead of Policy::none().
crate::client::install_crypto_provider();
let client = reqwest::Client::builder()
    .connect_timeout(Duration::from_secs(10))
    .timeout(Duration::from_secs(30))
    .build()
    .map_err(|err| CoreError::Internal(format!("cannot build HTTP client: {err}")))?;
```

### Pattern 2: Streaming download + incremental hash + atomic persist
**What:** Never buffer the artifact in memory; hash while streaming; only make the cache
file visible after the digest checks out.
**When to use:** Any downloaded artifact whose integrity gates its later use — this is the
same shape `download_to_file` already uses for exports, extended with a hasher in the loop
and a temp-file-then-persist instead of a direct `File::create`.
**Example:**
```rust
// Streaming shape verified against crates/ignition-core/src/client/mod.rs:670-722
// (download_to_file's existing bytes_stream() loop). sha2 usage verified against
// crates/ignition-cli/tests/e2e_webdev.rs:1875-1876 (sha256_hex helper, same crate/version).
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

let mut temp = tempfile::NamedTempFile::new_in(&cache_root)
    .map_err(|err| CoreError::Internal(format!("cannot create temp file: {err}")))?;
let mut hasher = Sha256::new();
let mut stream = response.bytes_stream();
while let Some(chunk) = stream.next().await {
    let chunk = chunk.map_err(|err| CoreError::Network { url: url.clone(), source: Some(err), observation: None })?;
    hasher.update(&chunk);
    // tempfile::NamedTempFile is std::io::Write, not tokio::io::AsyncWrite —
    // either spawn_blocking the write or wrap with tokio::fs::File::from_std;
    // the planner must pick one (see Open Questions).
    temp.write_all(&chunk).map_err(|err| CoreError::Internal(format!("cannot write temp file: {err}")))?;
}
let actual = format!("{:x}", hasher.finalize());
if actual != expected_hex {
    return Err(CoreError::<DigestMismatch-variant TBD> { expected: expected_hex, actual, .. });
}
temp.persist(final_path).map_err(|err| CoreError::Internal(format!("cannot persist cache entry: {err}")))?;
```
**Caveat flagged for the planner:** `tempfile::NamedTempFile` implements `std::io::Write`
(blocking), while the download loop is async. `download_to_file` sidesteps this because it
uses `tokio::fs::File` directly. The planner should choose between (a) `tokio::task::spawn_blocking`
around each `write_all`, (b) `tokio::fs::File::from_std(temp.reopen()?)` for the async
writes and persisting the original `NamedTempFile` handle afterward, or (c) accumulating
into a `tokio::fs::File::create(&tmp_path)` (not `tempfile`) and doing the atomic step with
`tokio::fs::rename` instead of `.persist()`. All three are viable; this research does not
pick one because it depends on how the planner wants error cleanup to work on partial
writes — flagged as an open question, not asserted as fact.

### Anti-Patterns to Avoid
- **Reusing `ReqwestGatewayApi`'s client for GitHub calls:** its `redirect::Policy::none()`
  will turn the mandatory CDN redirect into an unclassified 3xx response instead of bytes.
- **Fetching `asset.url` (the `api.github.com/.../assets/{id}` form) without an explicit
  `Accept: application/octet-stream` header:** confirmed live this session — omitting the
  header returns a `200 application/json` metadata body instead of bytes (see Pitfalls).
  `browser_download_url` sidesteps this entirely since it always redirects to bytes
  regardless of `Accept`, and is the simpler of the two supported paths.
- **Buffering the whole 7.7 MB body into a `Vec<u8>` before hashing:** the project's own
  `download_to_file` doc comment calls this out as "Pitfall 2" for exactly this class of
  problem [VERIFIED: crates/ignition-core/src/client/mod.rs:658-659].

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| GitHub release/asset JSON parsing | A full GitHub API client | `reqwest` + 3-4 `#[derive(Deserialize)]` structs scoped to exactly the fields needed (`tag_name`, `assets[].name`, `assets[].browser_download_url`, `assets[].digest`, `assets[].size`) | One endpoint, one shape — a full SDK (`octocrab`) is framework creep for this |
| SHA-256 | Manual bit-twiddling or shelling out to `shasum`/`sha256sum` | `sha2::Sha256` (already workspace-pinned) | Already in the tree; shelling out adds a host-tool dependency the project doesn't need and can't `--json` cleanly |
| Cross-platform cache directory resolution | Hand-rolled `~/.cache` / `%LOCALAPPDATA%` branching | `directories::ProjectDirs::cache_dir()` (already a dependency, only unused so far) | Exactly the problem `directories` solves; the project already trusts it for `config_dir()` |
| Atomic "verify then commit to cache" | Direct `File::create` at the final path with a post-hoc integrity flag | `tempfile::NamedTempFile` + `.persist()` (rename-based atomic commit), matching the crate's own 05-02 precedent | A direct write-then-check leaves a window where a crash mid-download leaves a truncated file AT the final path — exactly the "partial download mistaken for complete" pitfall the roadmap flags |

**Key insight:** every piece of this phase already has a load-bearing precedent somewhere
else in this codebase (streaming download, atomic temp-file commit, sha2 hashing, redirect
policy discipline). The research risk here is not "what library" — it's "don't silently
diverge from the pattern the codebase already proved out," which is why this document cites
file:line for the existing patterns rather than proposing new idioms.

## Common Pitfalls

### Pitfall 1: `Accept` header omission on the `assets/{id}` API URL
**What goes wrong:** GETing `api.github.com/repos/OWNER/REPO/releases/assets/{id}` without
`Accept: application/octet-stream` returns `200 application/json` — the asset's *metadata*,
not its bytes. A naive implementation that doesn't check `Content-Type` before writing to
disk could persist a ~600-byte JSON blob into the module cache under a `.modl` filename.
**Why it happens:** GitHub's asset-by-id endpoint is content-negotiated; the JSON
representation is the default.
**How to avoid:** Prefer `browser_download_url` (confirmed this session to redirect straight
to bytes with NO `Accept` header needed, for both authenticated and unauthenticated
requests). If the `assets/{id}` + token path is ever used instead (e.g., for a future
private-repo case), always send `Accept: application/octet-stream` AND verify
`Content-Type` before treating the body as artifact bytes.
**Warning signs:** A "downloaded" file under ~1 KB, or a file whose first bytes are `{`.
**Evidence:** [VERIFIED this session] — live curl to
`api.github.com/repos/WhiskeyHouse/ignition-git-module/releases/assets/494472691` without
`Accept` returned `HTTP/2 200`, `content-type: application/json; charset=utf-8`, body
starting `{"url": "https://api.github.com/...", "id": 494472691, ...}`. The same URL WITH
`Accept: application/octet-stream` returned `HTTP/2 302` → CDN → bytes.

### Pitfall 2: Unknown version must fail loudly, not silently fall back
**What goes wrong:** A typo'd or unreleased version silently resolving to "latest" (or to
no asset match with a generic error) hides the real problem from the user.
**Why it happens:** GitHub's `releases/latest` endpoint exists and is tempting as a
fallback; the roadmap explicitly forbids this (SC-1: "an unknown version fails loudly naming
the version and the resolved URL, never falling back to 'latest'").
**How to avoid:** Only ever call `releases/tags/{tag}` (never `releases/latest`). A 404 from
that endpoint is the exact "unknown version" signal — surface the requested version AND the
URL that was queried in the error.
**Warning signs:** Any code path that references `/releases/latest`.
**Evidence:** [VERIFIED this session] — live curl to `releases/tags/v99.99.99` returned
`HTTP 404` with body `{"message": "Not Found", "documentation_url": "...", "status": "404"}`.

### Pitfall 3: Partial/interrupted download mistaken for a complete, verified cache entry
**What goes wrong:** If the download writes directly to the final cache path and the
process is killed mid-stream, a truncated `.modl` file sits at the path a later "cache hit"
check would find — and use.
**Why it happens:** Naive "write to final path, verify after" ordering.
**How to avoid:** ALWAYS write to a temp file (in the same filesystem as the cache, so
`.persist()`/rename is atomic — `tempfile::NamedTempFile::new_in(cache_root)`) and only
rename into the final, version+digest-keyed path after the hash check passes. A cache
lookup that finds a file at the final path can then trust it is complete and verified BY
CONSTRUCTION — no separate "is this complete" flag needed, because incomplete downloads can
never reach that path.
**Warning signs:** A cache "hit" that fails to load/mount later; file size smaller than the
release's published `size` field.

### Pitfall 4: Re-released version with different bytes (digest drift)
**What goes wrong:** If the cache key is version-only (`git/2.3.4.modl`), a re-tagged
release with different bytes (same version string, new upload) would either silently reuse
stale cached bytes forever, or silently overwrite them without re-verifying — both wrong.
**Why it happens:** Version strings are not immutable identifiers on GitHub; only the asset
bytes + digest are.
**How to avoid:** Key the cache by **version + digest** (roadmap SC-3 already specifies
this), e.g. `<cache_dir>/modules/<module>/<version>-<digest_prefix>.modl`. On every fetch
(cache miss OR explicit re-verify), compare the freshly-resolved release's digest against
what's cached for that version; a mismatch means the upstream release changed and the old
entry should not be silently reused. The exact "what does `ign` do when the SAME version now
has a DIFFERENT digest" behavior (auto-refetch vs. hard refuse) is a planner decision — see
Open Questions.
**Warning signs:** A gateway loading a module whose behavior doesn't match its documented
version.

### Pitfall 5: Rate-limit 403 misread as an auth failure
**What goes wrong:** GitHub's unauthenticated REST API returns HTTP `403` (not `429`) with
a JSON body when the rate limit is exhausted. A classify path that treats bare 403s as
`CoreError::Auth` (as the existing gateway classifier does for `/data` routes) would
misreport "credentials rejected" for what is actually "come back in an hour."
**Why it happens:** GitHub's rate-limit response status code overlaps with the conventional
"forbidden" status used for real auth failures.
**How to avoid:** Distinguish by response body/headers, not status code alone: a rate-limited
response carries `X-RateLimit-Remaining: 0` and a body containing `"rate limit exceeded"`.
Check `X-RateLimit-Remaining` before treating a 403 as anything else.
**Evidence:** [VERIFIED this session] unauthenticated requests to `api.github.com` carried
`x-ratelimit-limit: 60`, `x-ratelimit-remaining: 56` (decrementing per call) — confirming the
unauthenticated ceiling is 60 requests/hour per source IP, and that the header is present on
every successful response so a client CAN watch it decrease and warn before hitting the
wall. This was not exercised to actual exhaustion (60 real requests were not sent) — the
exact 403 rate-limit body shape is [CITED: GitHub REST API rate-limit docs describe this
shape but the literal exhausted-response body was not captured live this session].

## Code Examples

### Release lookup (tags endpoint, not `/latest`)
```rust
// Source: verified live against api.github.com this session.
// GET https://api.github.com/repos/{owner}/{repo}/releases/tags/{tag}
#[derive(serde::Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(serde::Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    digest: Option<String>, // "sha256:<hex>" — present on GitHub's modern API (verified live)
    size: u64,
}
```
Live-verified response shape (abbreviated, this session):
```json
{
  "tag_name": "v2.3.4",
  "assets": [
    {
      "name": "Git-2.3.4-signed.modl",
      "url": "https://api.github.com/repos/WhiskeyHouse/ignition-git-module/releases/assets/494472691",
      "browser_download_url": "https://github.com/WhiskeyHouse/ignition-git-module/releases/download/v2.3.4/Git-2.3.4-signed.modl",
      "digest": "sha256:b74070346e587b1e1c14ff5093cc70625f7bfa14c2d24b24e7a0888a619b33eb",
      "content_type": "application/octet-stream",
      "size": 7704578,
      "state": "uploaded"
    }
  ]
}
```
This matches the "known facts" given in the task brief byte-for-byte — re-confirmed live,
not re-derived from memory.

### Redirect chain the client must follow (verified live)
```
GET https://github.com/WhiskeyHouse/ignition-git-module/releases/download/v2.3.4/Git-2.3.4-signed.modl
→ HTTP/2 302, location: https://release-assets.githubusercontent.com/github-production-release-asset/...
→ GET (followed) → HTTP/2 200, content-disposition: attachment; filename=Git-2.3.4-signed.modl
   body: 7,704,578 bytes, sha256 b74070346e587b1e1c14ff5093cc70625f7bfa14c2d24b24e7a0888a619b33eb
```
Downloaded and hashed live this session with plain `curl -L`; the resulting sha256 matched
the release's published `digest` field exactly, confirming both the redirect mechanics and
the digest field's correctness end-to-end.

## Runtime State Inventory

Not applicable — this is a greenfield capability (first artifact-feed dependency), not a
rename/refactor/migration phase. Skipped per the trigger condition in the research protocol.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Module name for `crates/ignition-core/src/module/` (vs. `rig_module/` or another name) | Architecture Patterns → Recommended Project Structure | Low — pure naming/organization; a rename is a mechanical follow-up, not a redesign |
| A2 | Whether a same-version-different-digest re-release should auto-refetch or hard-refuse | Common Pitfalls → Pitfall 4 | Medium — affects error-taxonomy shape (one more variant if "hard refuse" is chosen) and UX; needs a planner/user decision, not inferable from RMOD-02/03 wording alone |
| A3 | GitHub's exhausted-rate-limit response body exact shape (403 + `X-RateLimit-Remaining: 0` + specific JSON message) | Common Pitfalls → Pitfall 5 | Low-Medium — only matters for the specific rate-limit-vs-auth-failure disambiguation logic; verified the header behavior live but not the exhausted-state body, since exhausting 60 requests would have been wasteful and this repo is unlikely to hit the ceiling in practice (module fetches are cache-first, rare) |
| A4 | Choice among the three async-write strategies for `tempfile::NamedTempFile` (spawn_blocking / from_std / plain rename) | Architecture Patterns → Pattern 2 caveat | Low — all three are correct; only affects code shape and error-cleanup ergonomics, not correctness of the verify-then-commit contract |

## Open Questions

1. **Does `ign` need a CLI-visible surface in Phase 15, or is this purely a library API
   exercised by tests?**
   - What we know: The roadmap's Phase 15 success criteria are all phrased as internal
     behavior ("`ign` resolves...", "a downloaded artifact...is REFUSED", "a second fetch
     reuses the cache") with no mention of a new subcommand; `RMOD-01` ("provision it on
     `rig up`") is explicitly Phase 16 scope, and the design doc's config surface (§4.3) is
     also Phase-16-adjacent.
   - What's unclear: Whether the plan should still add a thin `ign module fetch` (or
     similar) verb now for manual testing/demonstration, or defer any CLI surface entirely
     to Phase 16.
   - Recommendation: Treat Phase 15 as library-only (a `fetch_and_verify` function plus its
     supporting types), proven by `cargo test`, with zero new clap subcommands — consistent
     with "Depends on: Nothing" and "isolated so its failure modes are visible rather than
     buried inside a Docker flow" framing in the roadmap. Confirm with the user/planner if a
     debug-only CLI hook is wanted anyway.

2. **Cache staleness / re-verify policy for Pitfall 4 (re-released version, different bytes).**
   - What we know: The cache key must be version+digest (SC-3 explicit).
   - What's unclear: On a cache MISS for `version` because the cached entry's digest no
     longer matches the release's CURRENT published digest, should `ign` (a) silently fetch
     and cache the new bytes under a new digest-keyed path (leaving the stale entry orphaned
     but harmless), or (b) treat this as a hard error requiring explicit user action? Neither
     RMOD-02 nor the design doc states this.
   - Recommendation: (a) is more consistent with "digest mismatch is a hard failure" being
     scoped to *download-time* verification (the bytes you just fetched don't match what
     GitHub currently claims) rather than *cache-time* drift (what GitHub claims changed
     since you last cached it) — but this should be confirmed with the user, since it's a
     product decision about trust in the upstream release process, not a technical one.

3. **Exact new `CoreError` variant names/slugs and their exit codes** (see research question
   5 below for full reasoning) — drafted here as `<ReleaseNotFound>` and `<DigestMismatch>`
   placeholders; the planner should name them following the file's established
   `PascalCase` variant / `snake_case` slug convention (e.g. `ModuleReleaseNotFound` /
   `module_release_not_found`, `ModuleDigestMismatch` / `module_digest_mismatch`).

4. **Does the offline case (SC-4) need a NEW `CoreError` variant, or does the existing
   `Network` variant's wording ("gateway unreachable at {url}") read acceptably for a
   GitHub-fetch failure?**
   - What we know: `CoreError::Network`'s `Display` impl hard-codes "gateway unreachable"
     [VERIFIED: crates/ignition-core/src/error.rs:181-193].
   - What's unclear: Whether reusing this variant (technically correct: exit 4, `source` is
     the same `reqwest::Error` type, `url` is generic) with slightly-misleading wording is
     acceptable, or whether the file's own precedent (many comments noting "additive slug,
     never a new exit code" for narrowly-scoped accurate wording) argues for a new
     `ModuleNetwork`-style variant at the SAME exit code (4) with GitHub-accurate wording.
   - Recommendation: Add the narrow variant — it's a two-minute change and matches every
     other precedent in this file (the whole taxonomy is built from "same exit class, own
     slug" additions, never generic reuse with misleading text). Flagged as an open question
     rather than asserted because it is a judgment call the planner/user may weigh
     differently (fewer variants vs. more accurate messages).

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Network access to `api.github.com` / `github.com` / `*.githubusercontent.com` | Cache-miss fetch path (SC-1, SC-2) | ✓ (verified this session from the research environment) | — | Offline path (SC-4) is the designed fallback when network is unavailable |
| `gh` CLI | Not required by `ign` itself — used only as a research/verification convenience in this session | ✓ (used to cross-check `gh release view --json assets`) | — | `ign`'s own implementation must NOT shell out to `gh`; it uses `reqwest` directly (no `gh` dependency at runtime) |
| Rust toolchain (edition 2024, MSRV per workspace) | Building/testing this phase's code | Not probed in this session (out of scope — this is a research pass, not an execution pass) | — | N/A |

**Missing dependencies with no fallback:** none identified.
**Missing dependencies with fallback:** GitHub network access — SC-4 (offline-with-cache) is
the explicit fallback path this phase is required to prove.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | `cargo test` (workspace default) + `wiremock` 0.6 for HTTP mocking [VERIFIED: root Cargo.toml — `wiremock = "0.6"` under `# dev`] |
| Config file | none — standard `cargo test` / `[dev-dependencies]` per crate |
| Quick run command | `cargo test -p ignition-core --test module_fetch_contract` (new test file, name TBD by planner) |
| Full suite command | `cargo test --workspace` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| RMOD-02 (SC-1) | Pinned version resolves the signed asset and downloads; unknown version fails loudly naming version + URL | integration (wiremock) | `cargo test -p ignition-core --test module_fetch_contract -- resolves_pinned_version` / `-- unknown_version_names_url` | ❌ Wave 0 |
| RMOD-02 (SC-2) | Digest mismatch is REFUSED, nothing cached, error names both digests | integration (wiremock, tamper the mocked asset body) | `cargo test -p ignition-core --test module_fetch_contract -- digest_mismatch_refused` | ❌ Wave 0 |
| RMOD-02/RMOD-03 (SC-3) | Second fetch of same version+digest makes NO network request | integration (wiremock `mount_as_scoped` + `.expect(0)` on a SECOND mock scope) | `cargo test -p ignition-core --test module_fetch_contract -- second_fetch_no_network` | ❌ Wave 0 |
| RMOD-03 (SC-4) | Populated cache + no network ⇒ provisioning completes | integration (no mock server started at all for this test — point the fetch fn at an unroutable/absent base URL and assert the cache-hit path never attempts a connection) | `cargo test -p ignition-core --test module_fetch_contract -- offline_with_populated_cache` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p ignition-core --test module_fetch_contract`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full workspace suite green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `crates/ignition-core/tests/module_fetch_contract.rs` (name TBD) — covers RMOD-02, RMOD-03, all four success criteria
- [ ] `sha2` promoted to `ignition-core`'s `[dependencies]` (currently dev-only in `ignition-cli`) — a one-line `Cargo.toml` edit, not a framework gap, but blocks the first line of production code in this phase
- [ ] No shared test harness for GitHub-shaped wiremock responses exists yet (the project's `IgnitionMock` in `crates/ignition-core/tests/common/mod.rs` is gateway-fixture-shaped only) — the planner should decide whether to extend `common/mod.rs` with GitHub-release builders or keep them local to the new test file; given this is the ONLY phase touching this feed, a local helper (not shared infra) is likely sufficient

**Mechanism for SC-3's "no network request" assertion:** the existing wiremock convention
for exact-call-count assertions is `Mock::given(...).expect(N).mount_as_scoped(&server).await`
— dropping the returned `MockGuard` at scope end asserts the count
[VERIFIED: crates/ignition-core/tests/session_contract.rs:37-45, pattern also seen with
`.expect(0)` semantics documented in `crates/ignition-core/tests/edit_pipeline.rs:151` and
`crates/ignition-core/tests/workspace_status_push.rs:603` naming `expected_gets`/
`expected_exports` parameters]. For SC-3, mount a SECOND, freshly-scoped mock with
`.expect(0)` around the second `fetch_and_verify()` call (after the first call has already
populated the cache under its own `.expect(1)`-scoped mock) — if the second call issues any
HTTP request, the scoped guard's drop-time assertion fails the test.

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-------------------|
| V5 Input Validation | yes | Version string (`--version`/config value, not yet wired until Phase 16) must be validated before use in a filesystem path segment (cache key) — reject path-traversal characters (`/`, `..`, null bytes) before constructing `<cache_dir>/modules/<module>/<version>-...` |
| V6 Cryptography | partial | SHA-256 here is an **integrity check**, not a cryptographic signature verification — `ign` verifies "these are the bytes GitHub published," NOT "these bytes are validly signed by the module author." The actual code-signing check happens inside Ignition's module-scan at gateway load time (outside `ign`'s control). This distinction should be documented in user-facing docs/errors so nobody assumes `ign`'s sha256 check is a substitute for the module's own signature verification. [ASSUMED: exact ASVS 4.x category numbering for this project's chosen ASVS version was not independently re-verified this session — confirm the category list against the actual ASVS document version the team targets] |
| V12 File and Resources (or equivalent "untrusted download" category) | yes | Downloaded bytes are never executed or parsed by `ign` itself in this phase (no zip extraction, no script execution) — they are opaque bytes verified by digest and later bind-mounted (Phase 16) into a container `ign` does not control the contents of. This significantly limits the blast radius of a compromised artifact reaching `ign`'s own process. |

### Known Threat Patterns for this phase's stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|----------------------|
| MITM / tampered download between GitHub and `ign` | Tampering | TLS (rustls, enforced by `reqwest`'s default cert validation — this fetch client must NOT set `danger_accept_invalid_certs`, unlike the gateway client's opt-in insecure mode for self-signed rig certs) PLUS the sha256 digest check against GitHub's API-reported value (defense in depth: even a TLS-terminating MITM with a valid cert would still need to also forge the digest GitHub's API reports, which travels over a SEPARATE HTTPS connection to `api.github.com`) |
| Compromised upstream release (attacker uploads a malicious `.modl` AND updates the published digest to match) | Tampering (supply chain) | Out of scope for `ign`'s own verification — this is exactly what the module's own signature (loaded by Ignition, not `ign`) is meant to catch. `ign`'s sha256 check only proves byte-for-byte reproducibility of what GitHub currently serves, not upstream authorial integrity. Document this boundary explicitly (see V6 note above). |
| Path traversal via a malicious/malformed version string reaching the cache path | Tampering / Elevation | Validate the version string (expect a semver-like shape) before it becomes part of a filesystem path; never interpolate raw user/config input directly into `PathBuf::join()` without a character allowlist check |
| Disk exhaustion via an oversized or infinite-stream response | Denial of Service | The release API's `size` field is known BEFORE download (confirmed live: `"size": 7704578` present in the asset metadata) — the planner should consider capping the stream at the published size (+ small tolerance) and aborting if exceeded, rather than trusting `Content-Length` alone |
| Cache poisoned by a write that skipped verification (a bug, not an attacker) | Tampering (integrity bug) | Structural mitigation, not a runtime check: the atomic temp-file-then-persist pattern (Pitfall 3) makes it IMPOSSIBLE for an unverified file to ever occupy the final cache path — there is no code path that writes to the final path directly |
| GitHub rate-limit response misclassified as auth failure, leading to a confusing/incorrect error message to the user | (not a security threat per se, but an error-taxonomy correctness issue) | See Pitfall 5 — check `X-RateLimit-Remaining` before classifying a 403 |

## Sources

### Primary (HIGH confidence)
- Live requests to `https://api.github.com/repos/WhiskeyHouse/ignition-git-module` (repo
  visibility), `.../releases/tags/v2.3.4` (release/asset shape, digest field), `.../releases/tags/v99.99.99`
  (404 behavior for unknown version), `.../releases/assets/494472691` (with/without `Accept`
  header) — all executed live this session via `curl`, responses captured verbatim above.
- Live download + local sha256 verification of `Git-2.3.4-signed.modl` via
  `browser_download_url` — confirms redirect chain AND digest correctness end-to-end.
- Context7 `/seanmonstar/reqwest` — `ClientBuilder::redirect` default policy
  (`redirect::Policy::default()` = `limited(10)`); `reqwest::Error` predicate methods
  (`is_connect`, `is_dns`, `is_timeout`, `is_status`) for offline/not-found disambiguation.
- Context7 `/websites/rs_directories` — `ProjectDirs::cache_dir()` signature and
  per-platform path shape.
- This codebase, read directly: `crates/ignition-core/src/error.rs` (full `CoreError`
  taxonomy, exit-code table, slug list), `crates/ignition-core/src/client/mod.rs`
  (`build_client`, `download_to_file`, `install_crypto_provider`), `crates/ignition-core/src/config/mod.rs`
  (`config_path()`/`ProjectDirs` usage pattern), root `Cargo.toml` and both crates'
  `Cargo.toml` files (dependency provenance), `crates/ignition-core/tests/session_contract.rs`
  and `crates/ignition-core/tests/common/mod.rs` (wiremock `.expect(N)` / `mount_as_scoped`
  convention), `crates/ignition-core/src/lib.rs` (module layout convention).

### Secondary (MEDIUM confidence)
- GitHub REST API documentation's general description of unauthenticated rate limits (60
  req/hr) — corroborated by the live `X-RateLimit-Limit: 60` header observed this session,
  but the exact exhausted-state (`403` body) shape was not captured live.

### Tertiary (LOW confidence)
- None — every claim in this document is either live-verified this session, read directly
  from the codebase, or sourced from Context7 official docs. No claim rests on unverified
  training-data recall.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — every dependency is already in the workspace tree; no new-package
  risk exists.
- Architecture: HIGH — every pattern (streaming, atomic persist, separate-client redirect
  policy, module layout) is grounded in an existing, working precedent in this exact
  codebase, cited by file:line.
- Pitfalls: HIGH — the two most GitHub-specific pitfalls (Accept-header omission, unknown-tag
  404 shape) were reproduced live this session, not inferred.
- Security domain: MEDIUM — the STRIDE analysis and mitigations are sound, but the precise
  ASVS 4.x category numbers were not cross-checked against whichever exact ASVS revision
  this project's `security_asvs_level: 1` setting refers to.

**Research date:** 2026-09-20
**Valid until:** 30 days for the codebase-internal patterns (stable); the GitHub API/CDN
behavior (redirect shape, rate limits, digest field) is a third-party surface — re-verify
if implementation is deferred more than ~60 days, since GitHub's release-asset delivery
infrastructure has changed shape before (the CDN host `release-assets.githubusercontent.com`
observed this session is itself a relatively recent replacement for the older
`github-releases.githubusercontent.com`/`objects.githubusercontent.com` hosts GitHub has
used historically).
