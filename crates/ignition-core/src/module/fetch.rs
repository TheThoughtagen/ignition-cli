//! Release resolution, streamed download, and atomic cache-verify for
//! third-party `.modl` module artifacts (Phase 15, RMOD-02/03).
//!
//! [`ModuleFeed`] talks to a module's GitHub release feed over a
//! SEPARATE `reqwest::Client` from [`crate::client::ReqwestGatewayApi`]
//! (D-03): the gateway client suppresses ALL redirects for the
//! uncommissioned-gateway wizard 302 (see `crate::client::build_client`'s
//! redirect policy), which would swallow the mandatory release-CDN
//! redirect this fetch depends on. Redirects here are left at reqwest's
//! untouched default (follows up to 10 hops).
//!
//! [`ModuleFeed::fetch_and_verify`] resolves ONLY through the tag-pinned
//! releases endpoint (`/repos/{repo}/releases/tags/{tag}`) — the floating
//! `releases/latest` endpoint is never called, for any reason, including
//! as a fallback (SC-1). A cache hit ([`super::cached_entry`], keyed by
//! version alone) short-circuits before any request is built, which is
//! what makes SC-3 (no network on a repeat fetch) and SC-4
//! (offline-with-cache succeeds) possible.

use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::error::CoreError;
use crate::module::{
    ModuleSpec, cached_entry, module_cache_dir, validate_module_id, validate_version,
};

/// Hard byte budget read from a non-success feed response before it is
/// used in an error message — see [`bounded_diagnostic_body`].
const DIAGNOSTIC_BODY_CAP_BYTES: usize = 8 * 1024;
/// Characters of that budget actually kept in the error message.
const DIAGNOSTIC_BODY_CHARS: usize = 200;
/// Appended when the diagnostic body was cut short.
const DIAGNOSTIC_TRUNCATION_MARKER: &str = "… (truncated)";

/// How a fetch treats an existing cache hit (D-02, D-09). All three
/// behave IDENTICALLY on a cache MISS — the miss path (resolve,
/// download, verify, persist) is implemented exactly once and is
/// reached by every policy alike.
///
/// On a cache HIT, a re-released version upstream is refused BY
/// DEFAULT: if the digest the feed publishes today differs from what's
/// cached for this version, the fetch refuses naming both digests and
/// leaves the cached artifact usable and untouched.
/// `AcceptUpstreamChange` is the deliberate, explicit way to accept
/// that change — it downloads the new bytes and verifies them against
/// the NEWLY published digest before caching them ALONGSIDE (never
/// over) the old entry. Phase 16 wires these to a CLI flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FetchPolicy {
    /// Trust an existing cache hit outright — never re-contact the feed
    /// for a version already cached. The default.
    #[default]
    CacheFirst,
    /// Re-verify a cache hit against the feed's current digest before
    /// trusting it: an unchanged digest reuses the cache (no
    /// re-download); a changed digest is REFUSED
    /// (`module_digest_changed`, exit 6), naming both digests, with
    /// the cached artifact left exactly as it was (15-02).
    Refresh,
    /// Accept a digest change the feed now reports for an already-cached
    /// version: downloads the new bytes and verifies them against the
    /// NEWLY published digest — never trusts them unverified — then
    /// persists them alongside (not over) the existing cache entry
    /// (15-02, D-02's explicit override).
    AcceptUpstreamChange,
}

/// Where a [`FetchedModule`]'s bytes came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactSource {
    /// Served from the local cache — no HTTP request was made.
    Cache,
    /// Freshly resolved, downloaded, and verified this call.
    Download,
}

/// The result of a successful [`ModuleFeed::fetch_and_verify`] call —
/// core returns models, never prints (Phase 16's `--json` renders this).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct FetchedModule {
    /// The [`ModuleSpec::id`] this artifact belongs to.
    pub module_id: String,
    /// The requested version string.
    pub version: String,
    /// Full lowercase hex sha256 digest of the artifact bytes.
    pub digest_sha256: String,
    /// Path to the verified `.modl` file on disk.
    pub path: std::path::PathBuf,
    /// Artifact size in bytes.
    pub bytes: u64,
    /// Whether this call hit the cache or downloaded fresh.
    pub source: ArtifactSource,
}

/// The GitHub releases-by-tag response, scoped to exactly the fields
/// this module needs — no GitHub SDK (15-RESEARCH.md "Don't Hand-Roll").
#[derive(Debug, serde::Deserialize)]
struct Release {
    assets: Vec<Asset>,
}

/// One release asset, scoped to exactly the fields this module needs.
#[derive(Debug, serde::Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    /// `"sha256:<64-hex>"` on GitHub's modern API — `None`/malformed is
    /// refused (`ign` never caches bytes it cannot verify).
    digest: Option<String>,
    /// The release's published size in bytes — caps the download
    /// stream against an oversized/endless body (T-15-10, 15-02).
    /// `Content-Length` is deliberately NOT trusted in its place: a
    /// hostile or broken origin controls that header too, while this
    /// value arrived over the separate release-metadata request.
    size: u64,
}

/// A third-party module release feed — one `reqwest::Client`, kept
/// SEPARATE from [`crate::client::ReqwestGatewayApi`] (D-03).
pub struct ModuleFeed {
    api_base: url::Url,
    client: reqwest::Client,
}

impl ModuleFeed {
    /// The production feed: `https://api.github.com`.
    pub fn github() -> Result<Self, CoreError> {
        Self::for_base(
            url::Url::parse("https://api.github.com").expect("literal GitHub API URL parses"),
        )
    }

    /// Build against an arbitrary base — the test seam (wiremock servers,
    /// and the unroutable-address offline fixtures).
    pub fn for_base(api_base: url::Url) -> Result<Self, CoreError> {
        // Both this client and ReqwestGatewayApi's must install the
        // process-wide TLS provider before their first request — the
        // workspace builds reqwest with rustls-no-provider (D-03).
        crate::client::install_crypto_provider();
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|err| {
                CoreError::Internal(format!("cannot build module feed HTTP client: {err}"))
            })?;
        Ok(Self { api_base, client })
    }

    /// Resolve, download, verify, and cache `spec`'s `version` — or
    /// return it straight from the cache with zero network requests
    /// (see the module doc for the SC-3/SC-4 mechanism).
    pub async fn fetch_and_verify(
        &self,
        spec: &ModuleSpec,
        version: &str,
        cache_root: &Path,
        policy: FetchPolicy,
    ) -> Result<FetchedModule, CoreError> {
        // Refuse a hostile module id / version BEFORE it reaches a
        // PathBuf::join or a URL (T-15-03) — before even the cache
        // lookup, so a traversal attempt never touches the filesystem.
        validate_module_id(spec.id)?;
        validate_version(version)?;

        // (a) Cache lookup — keyed by version alone. `CacheFirst` (the
        // default) short-circuits HERE with ZERO requests of any kind
        // — that's the whole SC-3/SC-4 mechanism, and adding drift
        // detection below must never weaken it.
        // `Refresh`/`AcceptUpstreamChange` fall through to resolve the
        // release below and compare digests before deciding (D-02/D-09).
        let cached = cached_entry(cache_root, spec.id, version);
        if let Some(cached) = &cached
            && policy == FetchPolicy::CacheFirst
        {
            // An entry cached before readability was enforced is still
            // 0600 on disk; a hit must repair it, not hand back an
            // artifact the gateway cannot read.
            ensure_readable(&cached.path)?;
            return Ok(FetchedModule {
                module_id: spec.id.to_string(),
                version: version.to_string(),
                digest_sha256: cached.digest_sha256.clone(),
                path: cached.path.clone(),
                bytes: cached.bytes,
                source: ArtifactSource::Cache,
            });
        }

        // (b) Resolve ONLY through the tag-pinned releases endpoint —
        // the floating `releases/latest` endpoint is never called. Runs
        // for a cache MISS (any policy) AND for a cache HIT under
        // Refresh/AcceptUpstreamChange (the drift check needs the
        // feed's CURRENT digest).
        let tag = spec.tag(version);
        let release_url = self
            .api_base
            .join(&format!("/repos/{}/releases/tags/{tag}", spec.repo))
            .map_err(|err| CoreError::Internal(format!("cannot build release URL: {err}")))?;

        let response = self
            .client
            .get(release_url.clone())
            .header(reqwest::header::USER_AGENT, "ignition-cli")
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .send()
            .await
            .map_err(|err| CoreError::ModuleFeedUnreachable {
                url: release_url.to_string(),
                source: Some(err),
            })?;

        // (c) Classify the resolve response.
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(CoreError::ModuleReleaseNotFound {
                module: spec.id.to_string(),
                version: version.to_string(),
                url: release_url.to_string(),
                detail: format!("no release exists for tag {tag:?}"),
            });
        }
        if !response.status().is_success() {
            let status = response.status();
            // Rate-limit detail BEFORE anything else (15-RESEARCH
            // Pitfall 5): GitHub's unauthenticated API answers an
            // exhausted rate limit as a bare 403, which the gateway
            // classifier's convention would misreport as a credential
            // rejection. Distinguish by header, not status code alone —
            // a 403 WITHOUT the header is a plain unusable answer, never
            // presumed to be a rate limit.
            let rate_limit_remaining = response
                .headers()
                .get("x-ratelimit-remaining")
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            let rate_limit_reset = response
                .headers()
                .get("x-ratelimit-reset")
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            let capped_body = bounded_diagnostic_body(response).await;
            let detail = if rate_limit_remaining.as_deref() == Some("0") {
                match rate_limit_reset {
                    Some(reset) => format!(
                        "feed rate limit exhausted (x-ratelimit-remaining: 0, resets at {reset})"
                    ),
                    None => "feed rate limit exhausted (x-ratelimit-remaining: 0)".to_string(),
                }
            } else {
                format!("release lookup answered HTTP {status}: {capped_body}")
            };
            return Err(CoreError::ModuleFeedUnusable {
                url: release_url.to_string(),
                detail,
            });
        }
        let release: Release =
            response
                .json()
                .await
                .map_err(|err| CoreError::ModuleFeedUnusable {
                    url: release_url.to_string(),
                    detail: format!("release body did not match the expected shape: {err}"),
                })?;

        // (d) Select the asset; verify it carries a usable digest.
        let expected_asset_name = spec.asset_name(version);
        let Some(asset) = release
            .assets
            .iter()
            .find(|asset| asset.name == expected_asset_name)
        else {
            let present: Vec<&str> = release.assets.iter().map(|a| a.name.as_str()).collect();
            return Err(CoreError::ModuleReleaseNotFound {
                module: spec.id.to_string(),
                version: version.to_string(),
                url: release_url.to_string(),
                detail: format!(
                    "release has no asset named {expected_asset_name:?} (present: [{}])",
                    present.join(", ")
                ),
            });
        };

        let expected_digest = match asset
            .digest
            .as_deref()
            .and_then(|digest| digest.strip_prefix("sha256:"))
        {
            Some(hex) if hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()) => {
                hex.to_ascii_lowercase()
            }
            _ => {
                return Err(CoreError::ModuleFeedUnusable {
                    url: release_url.to_string(),
                    detail: format!(
                        "asset {:?} has no usable sha256 digest (published: {:?})",
                        asset.name, asset.digest
                    ),
                });
            }
        };

        // (a-continued) A cache HIT under Refresh/AcceptUpstreamChange:
        // decide from the digest comparison BEFORE touching the
        // network again for bytes — D-02's two separate questions
        // (see the module doc: this is the "did upstream change?"
        // question, never the "do the bytes match?" one below, which
        // has no override).
        if let Some(cached) = cached {
            if expected_digest == cached.digest_sha256 {
                // Upstream still publishes the digest already cached —
                // bytes already proven are not re-fetched.
                ensure_readable(&cached.path)?;
                return Ok(FetchedModule {
                    module_id: spec.id.to_string(),
                    version: version.to_string(),
                    digest_sha256: cached.digest_sha256,
                    path: cached.path,
                    bytes: cached.bytes,
                    source: ArtifactSource::Cache,
                });
            }
            if policy == FetchPolicy::Refresh {
                // D-02 default: refuse, name both digests, leave the
                // cached artifact exactly as it was — no write, no
                // delete, no overwrite.
                return Err(CoreError::ModuleDigestChanged(Box::new(
                    crate::error::ModuleDigestChangedDetails {
                        module: spec.id.to_string(),
                        version: version.to_string(),
                        cached_digest: cached.digest_sha256,
                        upstream_digest: expected_digest.clone(),
                        cached_path: cached.path.display().to_string(),
                    },
                )));
            }
            // AcceptUpstreamChange: fall through to the SHARED
            // download-verify-persist path below, which verifies the
            // NEW bytes against `expected_digest` exactly as a cache
            // miss would — the override never skips verification
            // (D-02/D-09).
        }

        // (e) Download through the redirect (D-03: default policy).
        let download_url = url::Url::parse(&asset.browser_download_url).map_err(|err| {
            CoreError::ModuleFeedUnusable {
                url: release_url.to_string(),
                detail: format!(
                    "asset download URL {:?} did not parse: {err}",
                    asset.browser_download_url
                ),
            }
        })?;

        // Asset-URL scheme guard (T-15-05): an `https` feed may only
        // hand back an `https` asset URL — refuse BEFORE attempting the
        // download, never silently follow a downgrade. Keeps the
        // mock-server (http base -> http asset) path working while
        // making a production downgrade impossible.
        if is_insecure_downgrade(self.api_base.scheme(), download_url.scheme()) {
            return Err(CoreError::ModuleFeedUnusable {
                url: download_url.to_string(),
                detail: format!(
                    "asset download URL uses {} while the feed itself is https — \
                     refusing the insecure downgrade",
                    download_url.scheme()
                ),
            });
        }

        let response = self
            .client
            .get(download_url.clone())
            .send()
            .await
            .map_err(|err| CoreError::ModuleFeedUnreachable {
                url: download_url.to_string(),
                source: Some(err),
            })?;
        if !response.status().is_success() {
            return Err(CoreError::ModuleFeedUnusable {
                url: download_url.to_string(),
                detail: format!("asset download answered HTTP {}", response.status()),
            });
        }

        // (f) Stream to a temp file in the cache dir, hashing as we go
        // (D-07): tempfile::NamedTempFile for RAII cleanup on any early
        // return, tokio::fs::File::from_std(temp.reopen()?) for the
        // async chunk writes.
        let cache_dir = module_cache_dir(cache_root, spec.id);
        tokio::fs::create_dir_all(&cache_dir).await.map_err(|err| {
            CoreError::Internal(format!("cannot create {}: {err}", cache_dir.display()))
        })?;

        let temp = tempfile::NamedTempFile::new_in(&cache_dir).map_err(|err| {
            CoreError::Internal(format!(
                "cannot create temp file in {}: {err}",
                cache_dir.display()
            ))
        })?;
        let std_file = temp
            .reopen()
            .map_err(|err| CoreError::Internal(format!("cannot reopen temp file: {err}")))?;
        let mut async_file = tokio::fs::File::from_std(std_file);

        let mut hasher = Sha256::new();
        let mut stream = response.bytes_stream();
        let mut bytes: u64 = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|err| CoreError::ModuleFeedUnreachable {
                url: download_url.to_string(),
                source: Some(err),
            })?;
            bytes += chunk.len() as u64;
            // Size cap (T-15-10): compare the running total against the
            // release's PUBLISHED size — never Content-Length, which a
            // hostile or broken origin controls just as easily.
            // Returning here stops polling `stream` immediately (the
            // remaining body is never drained) and drops `temp`, whose
            // Drop discards whatever partial bytes were written so far.
            if bytes > asset.size {
                return Err(CoreError::ModuleFeedUnusable {
                    url: download_url.to_string(),
                    detail: format!(
                        "asset body exceeded the published size ({} bytes) — aborted mid-stream",
                        asset.size
                    ),
                });
            }
            hasher.update(&chunk);
            async_file
                .write_all(&chunk)
                .await
                .map_err(|err| CoreError::Internal(format!("cannot write temp file: {err}")))?;
        }
        async_file
            .flush()
            .await
            .map_err(|err| CoreError::Internal(format!("cannot flush temp file: {err}")))?;

        // (g) Verify. This comparison runs unconditionally, ahead of
        // any policy branch, and takes no policy parameter — there is
        // no FetchPolicy value that changes what happens here. A
        // mismatch returns before `.persist()` — the NamedTempFile's
        // Drop discards the bytes structurally.
        let actual_digest = format!("{:x}", hasher.finalize());
        if actual_digest != expected_digest {
            return Err(CoreError::ModuleDigestMismatch(Box::new(
                crate::error::ModuleDigestMismatchDetails {
                    module: spec.id.to_string(),
                    version: version.to_string(),
                    url: download_url.to_string(),
                    expected: expected_digest,
                    actual: actual_digest,
                },
            )));
        }

        // (h) Persist — the ONLY way bytes reach the final cache path.
        //
        // Readability is set on the TEMP file, before publication. Doing it
        // after `persist` would expose the final path at the temp mode for a
        // window, and a failure there would leave an unreadable artifact
        // already published; failing here instead lets the NamedTempFile's
        // Drop discard it, exactly as a digest mismatch does.
        ensure_readable(temp.path())?;
        let final_path = cache_dir.join(format!("{version}-{actual_digest}.modl"));
        temp.persist(&final_path).map_err(|err| {
            CoreError::Internal(format!(
                "cannot persist cache entry at {}: {err}",
                final_path.display()
            ))
        })?;

        Ok(FetchedModule {
            module_id: spec.id.to_string(),
            version: version.to_string(),
            digest_sha256: actual_digest,
            path: final_path,
            bytes,
            source: ArtifactSource::Download,
        })
    }
}

/// `true` when handing back `asset_scheme` would downgrade below the
/// feed's own `feed_scheme` — an `https` feed serving a non-`https`
/// asset URL (T-15-05). Pure and network-free by design: this workspace
/// has no TLS-serving wiremock (the same limitation
/// `tests/session_contract.rs`'s module doc already documents for the
/// `ssl_verify` builder behavior — "the WIRE-level https-skip proof
/// needs wiremock's optional `tls` feature, which this workspace does
/// not enable"), so the guard's logic is pinned here directly rather
/// than through a live round trip.
fn is_insecure_downgrade(feed_scheme: &str, asset_scheme: &str) -> bool {
    feed_scheme == "https" && asset_scheme != "https"
}

/// Read at most [`DIAGNOSTIC_BODY_CAP_BYTES`] from a non-success feed
/// response for use in an error message.
///
/// `Response::text()` buffers the WHOLE remote body before any
/// truncation, so a hostile or compromised feed could answer an error
/// with an arbitrarily large payload and push `ign` into memory
/// pressure — the request timeout bounds duration, not size. The
/// artifact path already streams under a published-size cap; this is
/// the same discipline applied to the diagnostic path, which is the
/// one place a feed's bytes are read without one.
///
/// Whatever is read is decoded lossily (an error body is diagnostic
/// text, never trusted structured data) and truncated to
/// [`DIAGNOSTIC_BODY_CHARS`], with a marker when anything was dropped.
async fn bounded_diagnostic_body(response: reqwest::Response) -> String {
    let mut buf: Vec<u8> = Vec::new();
    let mut truncated = false;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else { break };
        let remaining = DIAGNOSTIC_BODY_CAP_BYTES.saturating_sub(buf.len());
        if chunk.len() >= remaining {
            buf.extend_from_slice(&chunk[..remaining]);
            truncated = true;
            break;
        }
        buf.extend_from_slice(&chunk);
    }
    let text = String::from_utf8_lossy(&buf);
    let mut capped: String = text.chars().take(DIAGNOSTIC_BODY_CHARS).collect();
    if truncated || capped.chars().count() < text.chars().count() {
        capped.push_str(DIAGNOSTIC_TRUNCATION_MARKER);
    }
    capped
}

/// Make a cache entry readable by any uid — the gateway runs as its own
/// user, not ours.
///
/// A cache entry is an artifact to be MOUNTED, not a secret.
/// `NamedTempFile` creates at 0600 and `persist` preserves that mode, so
/// without this the stock `inductiveautomation/ignition` image (uid
/// 2003:2003) cannot read a bind-mounted `.modl` and the module is
/// silently absent — no error, nothing in the gateway log.
///
/// Idempotent, and applied on cache HITS as well as fresh downloads:
/// entries written before this existed are still 0600 on disk, and a hit
/// must repair one rather than hand back an unreadable artifact.
///
/// Unix-only. Windows has no equivalent mode bit and its default ACL on a
/// user-owned file is already readable.
fn ensure_readable(path: &Path) -> Result<(), CoreError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let current = std::fs::metadata(path)
            .map_err(|err| {
                CoreError::Internal(format!(
                    "cannot read permissions of cache entry at {}: {err}",
                    path.display()
                ))
            })?
            .permissions()
            .mode();
        if current & 0o044 == 0o044 {
            return Ok(());
        }
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(current | 0o044)).map_err(
            |err| {
                CoreError::Internal(format!(
                    "cannot make cache entry readable at {}: {err}",
                    path.display()
                ))
            },
        )?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_insecure_downgrade;

    #[test]
    fn https_feed_refuses_non_https_asset() {
        assert!(is_insecure_downgrade("https", "http"));
    }

    #[test]
    fn https_feed_allows_https_asset() {
        assert!(!is_insecure_downgrade("https", "https"));
    }

    #[test]
    fn http_feed_allows_any_asset_scheme() {
        assert!(!is_insecure_downgrade("http", "http"));
        assert!(!is_insecure_downgrade("http", "https"));
    }
}
