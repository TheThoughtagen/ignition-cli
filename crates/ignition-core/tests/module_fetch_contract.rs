//! Wiremock contract tests for the module artifact fetch/verify seam
//! (Phase 15, `ignition-core::module`). Task 1 (this commit) covers the
//! end-to-end tracer slice (SC-1's happy path) plus D-08's env-override
//! seam. Cache-authority (SC-3/SC-4) and loud-refusal (SC-1's negative
//! half) coverage land in later commits of this same file.
//!
//! Kept LOCAL to this file rather than extending
//! `tests/common/mod.rs` — that harness is gateway-fixture-shaped, and
//! this is the only phase that speaks to the module release feed.

use ignition_core::module::GIT_MODULE;
use ignition_core::module::fetch::{ArtifactSource, FetchPolicy, ModuleFeed};

use sha2::{Digest, Sha256};

/// A small, deterministic synthetic payload — never a committed
/// multi-megabyte real `.modl` binary.
fn payload() -> Vec<u8> {
    b"synthetic .modl payload for the 15-01 tracer test - deterministic, tiny, never the real binary; "
        .repeat(4)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// The whole path, end to end: tag-pinned resolve → 302 redirect →
/// stream + incremental hash → verify → atomic persist. The 302 hop is
/// not decoration — it proves the SEPARATE client (D-03) follows the
/// release-CDN redirect `ReqwestGatewayApi`'s `Policy::none()` would
/// have swallowed.
#[tokio::test]
async fn tracer_fetches_verifies_and_caches() {
    let server = wiremock::MockServer::start().await;
    let body = payload();
    let digest = sha256_hex(&body);

    let release_json = serde_json::json!({
        "tag_name": "v2.3.4",
        "assets": [{
            "name": "Git-2.3.4-signed.modl",
            "url": format!("{}/assets/1", server.uri()),
            "browser_download_url": format!("{}/download/Git-2.3.4-signed.modl", server.uri()),
            "digest": format!("sha256:{digest}"),
            "content_type": "application/octet-stream",
            "size": body.len(),
            "state": "uploaded",
        }],
    });

    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/repos/WhiskeyHouse/ignition-git-module/releases/tags/v2.3.4",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(release_json))
        .mount(&server)
        .await;

    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/download/Git-2.3.4-signed.modl"))
        .respond_with(
            wiremock::ResponseTemplate::new(302)
                .insert_header("Location", "/cdn/Git-2.3.4-signed.modl"),
        )
        .mount(&server)
        .await;

    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/cdn/Git-2.3.4-signed.modl"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_bytes(body.clone()))
        .mount(&server)
        .await;

    let cache_root = tempfile::tempdir().expect("tempdir");
    let feed =
        ModuleFeed::for_base(server.uri().parse().expect("server uri parses")).expect("feed builds");

    let fetched = feed
        .fetch_and_verify(
            &GIT_MODULE,
            "2.3.4",
            cache_root.path(),
            FetchPolicy::CacheFirst,
        )
        .await
        .expect("tracer fetch succeeds");

    assert_eq!(fetched.source, ArtifactSource::Download);
    assert_eq!(fetched.digest_sha256, digest);
    assert_eq!(fetched.bytes, body.len() as u64);
    assert!(fetched.path.exists(), "persisted file must exist");
    assert_eq!(
        fetched.path.file_name().and_then(|name| name.to_str()),
        Some(format!("2.3.4-{digest}.modl").as_str())
    );
    let expected_parent = cache_root.path().join("modules").join("git");
    assert_eq!(fetched.path.parent(), Some(expected_parent.as_path()));
    let on_disk = std::fs::read(&fetched.path).expect("read persisted file");
    assert_eq!(
        on_disk, body,
        "persisted bytes must be byte-identical to the served payload"
    );
}

/// D-08's env seam: `IGNITION_CLI_CACHE` wins over the platform path.
#[test]
fn cache_root_honours_env_override() {
    let dir = tempfile::tempdir().expect("tempdir");
    // SAFETY: no other test in this binary touches IGNITION_CLI_CACHE;
    // set/removed within this one test, no cross-test interleaving risk.
    unsafe { std::env::set_var("IGNITION_CLI_CACHE", dir.path()) };
    assert_eq!(ignition_core::module::cache_root(), dir.path());
    unsafe { std::env::remove_var("IGNITION_CLI_CACHE") };
}

// ---------------------------------------------------------------------
// Task 2: cache authority (SC-3, SC-4) — a wiremock server standing in
// for the release feed, scoped per-route so a test can assert EXACTLY
// how many requests a phase of the test made (`mount_release`/
// `mount_download`'s `expect` parameter). Kept local to this file (the
// `mount_info` shape at tests/session_contract.rs:37-45) — this is the
// only phase that speaks to this feed.
// ---------------------------------------------------------------------

/// Wraps a [`wiremock::MockServer`] standing in for the `git` module's
/// release feed, plus the payload/digest it serves — so each test names
/// only how many requests it expects per route.
struct FeedMock {
    server: wiremock::MockServer,
    payload: Vec<u8>,
    digest: String,
}

impl FeedMock {
    async fn start() -> Self {
        let server = wiremock::MockServer::start().await;
        let payload = payload();
        let digest = sha256_hex(&payload);
        Self {
            server,
            payload,
            digest,
        }
    }

    fn uri(&self) -> String {
        self.server.uri()
    }

    /// Scope the tags-endpoint mock, asserting it is hit EXACTLY
    /// `expect` times by the time the returned guard drops.
    async fn mount_release(&self, expect: u64) -> wiremock::MockGuard {
        let release_json = serde_json::json!({
            "tag_name": "v2.3.4",
            "assets": [{
                "name": "Git-2.3.4-signed.modl",
                "url": format!("{}/assets/1", self.server.uri()),
                "browser_download_url": format!("{}/download/Git-2.3.4-signed.modl", self.server.uri()),
                "digest": format!("sha256:{}", self.digest),
                "content_type": "application/octet-stream",
                "size": self.payload.len(),
                "state": "uploaded",
            }],
        });
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(
                "/repos/WhiskeyHouse/ignition-git-module/releases/tags/v2.3.4",
            ))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(release_json))
            .expect(expect)
            .mount_as_scoped(&self.server)
            .await
    }

    /// Scope BOTH download-leg mocks (the redirect hop and the CDN
    /// bytes), each asserting it is hit EXACTLY `expect` times.
    async fn mount_download(&self, expect: u64) -> (wiremock::MockGuard, wiremock::MockGuard) {
        let redirect_guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/download/Git-2.3.4-signed.modl"))
            .respond_with(
                wiremock::ResponseTemplate::new(302)
                    .insert_header("Location", "/cdn/Git-2.3.4-signed.modl"),
            )
            .expect(expect)
            .mount_as_scoped(&self.server)
            .await;
        let cdn_guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/cdn/Git-2.3.4-signed.modl"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_bytes(self.payload.clone()))
            .expect(expect)
            .mount_as_scoped(&self.server)
            .await;
        (redirect_guard, cdn_guard)
    }
}

/// SC-3: a second fetch of the same version reuses the cache and makes
/// ZERO HTTP requests. The falsifiable mechanism is the SECOND scope's
/// `.expect(0)` catch-all — its drop-time assertion fails the test if a
/// single request lands, not a log inspection.
#[tokio::test]
async fn second_fetch_makes_no_network_request() {
    let mock = FeedMock::start().await;
    let release_guard = mock.mount_release(1).await;
    let (redirect_guard, cdn_guard) = mock.mount_download(1).await;

    let cache_root = tempfile::tempdir().expect("tempdir");
    let feed =
        ModuleFeed::for_base(mock.uri().parse().expect("uri parses")).expect("feed builds");

    let first = feed
        .fetch_and_verify(
            &GIT_MODULE,
            "2.3.4",
            cache_root.path(),
            FetchPolicy::CacheFirst,
        )
        .await
        .expect("first fetch succeeds");
    assert_eq!(first.source, ArtifactSource::Download);

    // Drop now: the first phase's exact-one-hit expectations are
    // verified HERE, before the second phase's catch-all is mounted.
    drop(release_guard);
    drop(redirect_guard);
    drop(cdn_guard);

    let no_requests = wiremock::Mock::given(wiremock::matchers::any())
        .respond_with(wiremock::ResponseTemplate::new(500))
        .expect(0)
        .mount_as_scoped(&mock.server)
        .await;

    let second = feed
        .fetch_and_verify(
            &GIT_MODULE,
            "2.3.4",
            cache_root.path(),
            FetchPolicy::CacheFirst,
        )
        .await
        .expect("second fetch succeeds from cache");
    assert_eq!(second.source, ArtifactSource::Cache);
    assert_eq!(second.path, first.path);
    assert_eq!(second.digest_sha256, first.digest_sha256);

    drop(no_requests);
}

/// SC-4: a populated cache with NOTHING listening on the network still
/// completes successfully — offline is a supported state, not a
/// degraded one. No mock server is started at all.
#[tokio::test]
async fn offline_with_populated_cache_succeeds() {
    let cache_root = tempfile::tempdir().expect("tempdir");
    let body = payload();
    let digest = sha256_hex(&body);
    let module_dir = cache_root.path().join("modules").join("git");
    std::fs::create_dir_all(&module_dir).expect("create module cache dir");
    std::fs::write(module_dir.join(format!("2.3.4-{digest}.modl")), &body)
        .expect("seed cache entry");

    let feed = ModuleFeed::for_base(
        url::Url::parse("http://127.0.0.1:1").expect("unroutable url parses"),
    )
    .expect("feed builds");

    let fetched = feed
        .fetch_and_verify(&GIT_MODULE, "2.3.4", cache_root.path(), FetchPolicy::CacheFirst)
        .await
        .expect("offline cache hit succeeds with nothing listening");

    assert_eq!(fetched.source, ArtifactSource::Cache);
    assert_eq!(fetched.digest_sha256, digest);
}

/// SC-4's negative half: an EMPTY cache with nothing listening fails
/// exit 4 `module_feed_unreachable`, naming the feed URL that was
/// tried.
#[tokio::test]
async fn offline_with_empty_cache_names_the_feed() {
    let cache_root = tempfile::tempdir().expect("tempdir");
    let feed = ModuleFeed::for_base(
        url::Url::parse("http://127.0.0.1:1").expect("unroutable url parses"),
    )
    .expect("feed builds");

    let err = feed
        .fetch_and_verify(&GIT_MODULE, "2.3.4", cache_root.path(), FetchPolicy::CacheFirst)
        .await
        .expect_err("empty cache + unreachable feed must fail");

    assert_eq!(err.code(), "module_feed_unreachable");
    assert_eq!(err.exit_code(), 4);
    let endpoint = err.endpoint().unwrap_or_default();
    assert!(
        endpoint.contains("127.0.0.1:1"),
        "endpoint must name the feed URL that was tried: {endpoint}"
    );
}

/// `cached_entry` treats only EXACT `<version>-` + 64 lowercase hex +
/// `.modl` filenames as hits — a truncated digest, an uppercase digest,
/// a bare `2.3.4.modl`, and a prerelease `2.3.4-rc1-<hex>.modl` (when
/// plain `2.3.4` was requested) are all rejected.
#[test]
fn cache_entry_rejects_malformed_filenames() {
    let cache_root = tempfile::tempdir().expect("tempdir");
    let module_dir = cache_root.path().join("modules").join("git");
    std::fs::create_dir_all(&module_dir).expect("create module cache dir");
    let digest = sha256_hex(b"whatever");

    std::fs::write(
        module_dir.join(format!("2.3.4-{}.modl", &digest[..16])),
        b"truncated digest",
    )
    .expect("write truncated");
    std::fs::write(
        module_dir.join(format!("2.3.4-{}.modl", digest.to_ascii_uppercase())),
        b"uppercase digest",
    )
    .expect("write uppercase");
    std::fs::write(module_dir.join("2.3.4.modl"), b"no digest at all").expect("write bare");
    std::fs::write(
        module_dir.join(format!("2.3.4-rc1-{digest}.modl")),
        b"prerelease version string",
    )
    .expect("write prerelease");

    assert!(
        ignition_core::module::cached_entry(cache_root.path(), "git", "2.3.4").is_none(),
        "no malformed filename may be treated as a cache hit"
    );
}
