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
use crate::module::{ModuleSpec, cached_entry, module_cache_dir};

/// How a fetch treats an existing cache hit (D-02, D-09). All three
/// behave IDENTICALLY on a cache MISS — this plan (15-01) implements the
/// miss path once; plan 15-02 differentiates the HIT path (a pinned
/// version re-released upstream with a different digest) without
/// changing this signature. Phase 16 wires `Refresh` and
/// `AcceptUpstreamChange` to CLI flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FetchPolicy {
    /// Trust an existing cache hit outright — never re-contact the feed
    /// for a version already cached. The default.
    #[default]
    CacheFirst,
    /// Re-verify a cache hit against the feed's current digest before
    /// trusting it (15-02).
    Refresh,
    /// Accept a digest change the feed now reports for an already-cached
    /// version, re-downloading and re-caching under the new digest
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
    /// The release's published size in bytes. Unused in this plan;
    /// 15-02 uses it to cap the download stream against a DoS body
    /// (T-15-04) — kept here now so that plan needs no signature churn.
    #[allow(dead_code)]
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
        _policy: FetchPolicy,
    ) -> Result<FetchedModule, CoreError> {
        // (a) Cache lookup — keyed by version alone, no feed contact.
        if let Some(cached) = cached_entry(cache_root, spec.id, version) {
            return Ok(FetchedModule {
                module_id: spec.id.to_string(),
                version: version.to_string(),
                digest_sha256: cached.digest_sha256,
                path: cached.path,
                bytes: cached.bytes,
                source: ArtifactSource::Cache,
            });
        }

        // (b) Resolve ONLY through the tag-pinned releases endpoint —
        // the floating `releases/latest` endpoint is never called.
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
            return Err(CoreError::ModuleFeedUnusable {
                url: release_url.to_string(),
                detail: format!("release lookup answered HTTP {}", response.status()),
            });
        }
        let release: Release = response.json().await.map_err(|err| {
            CoreError::ModuleFeedUnusable {
                url: release_url.to_string(),
                detail: format!("release body did not match the expected shape: {err}"),
            }
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
            hasher.update(&chunk);
            async_file.write_all(&chunk).await.map_err(|err| {
                CoreError::Internal(format!("cannot write temp file: {err}"))
            })?;
            bytes += chunk.len() as u64;
        }
        async_file
            .flush()
            .await
            .map_err(|err| CoreError::Internal(format!("cannot flush temp file: {err}")))?;

        // (g) Verify. A mismatch returns before `.persist()` — the
        // NamedTempFile's Drop discards the bytes structurally.
        let actual_digest = format!("{:x}", hasher.finalize());
        if actual_digest != expected_digest {
            return Err(CoreError::ModuleDigestMismatch {
                module: spec.id.to_string(),
                version: version.to_string(),
                url: download_url.to_string(),
                expected: expected_digest,
                actual: actual_digest,
            });
        }

        // (h) Persist — the ONLY way bytes reach the final cache path.
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
