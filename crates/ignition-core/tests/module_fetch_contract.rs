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

// ---------------------------------------------------------------------
// Task 3: loud refusals on input (SC-1's negative half) — an unknown
// version, a missing asset, a rate-limited feed, and a hostile version/
// module-id string all fail loudly, name what the caller must fix, and
// leave the cache untouched.
// ---------------------------------------------------------------------

/// Shared refusal-hygiene helper (Task 1, 15-02): asserts `module_id`'s
/// cache directory either does not exist or holds ZERO entries — every
/// refusal test in this file calls this, so a refusal that leaves debris
/// fails HERE rather than passing on an incomplete ad-hoc check.
fn assert_cache_empty(cache_root: &std::path::Path, module_id: &str) {
    let dir = cache_root.join("modules").join(module_id);
    // A missing directory counts as empty.
    if let Ok(mut entries) = std::fs::read_dir(&dir) {
        assert!(
            entries.next().is_none(),
            "cache dir {} must be empty after a refusal — a refusal that leaves debris \
             is a failure even when the error itself is right",
            dir.display()
        );
    }
}

/// SC-1: an unknown version fails loudly naming BOTH the requested
/// version and the resolved tags URL — never a silent fallback to
/// "latest". Nothing is written to the cache root.
#[tokio::test]
async fn unknown_version_names_version_and_url() {
    let mock = FeedMock::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/repos/WhiskeyHouse/ignition-git-module/releases/tags/v2.3.4",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({"message": "Not Found"})),
        )
        .mount(&mock.server)
        .await;

    let cache_root = tempfile::tempdir().expect("tempdir");
    let feed =
        ModuleFeed::for_base(mock.uri().parse().expect("uri parses")).expect("feed builds");

    let err = feed
        .fetch_and_verify(&GIT_MODULE, "2.3.4", cache_root.path(), FetchPolicy::CacheFirst)
        .await
        .expect_err("unknown version must fail loudly");

    assert_eq!(err.code(), "module_release_not_found");
    assert_eq!(err.exit_code(), 6);
    let message = err.to_string();
    assert!(message.contains("2.3.4"), "message must name the version: {message}");
    let expected_url = format!(
        "{}/repos/WhiskeyHouse/ignition-git-module/releases/tags/v2.3.4",
        mock.uri()
    );
    assert_eq!(
        err.endpoint().as_deref(),
        Some(expected_url.as_str()),
        "endpoint must name the resolved URL"
    );
    assert_cache_empty(cache_root.path(), "git");
}

/// SC-1: a release with no matching asset fails loudly naming BOTH the
/// expected asset name and the names actually present.
#[tokio::test]
async fn missing_asset_names_expected_and_present() {
    let mock = FeedMock::start().await;
    let release_json = serde_json::json!({
        "tag_name": "v2.3.4",
        "assets": [{
            "name": "Other-2.3.4-signed.modl",
            "url": format!("{}/assets/1", mock.uri()),
            "browser_download_url": format!("{}/download/other.modl", mock.uri()),
            "digest": format!("sha256:{}", mock.digest),
            "content_type": "application/octet-stream",
            "size": mock.payload.len(),
            "state": "uploaded",
        }],
    });
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/repos/WhiskeyHouse/ignition-git-module/releases/tags/v2.3.4",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(release_json))
        .mount(&mock.server)
        .await;

    let cache_root = tempfile::tempdir().expect("tempdir");
    let feed =
        ModuleFeed::for_base(mock.uri().parse().expect("uri parses")).expect("feed builds");

    let err = feed
        .fetch_and_verify(&GIT_MODULE, "2.3.4", cache_root.path(), FetchPolicy::CacheFirst)
        .await
        .expect_err("missing asset must fail loudly");

    assert_eq!(err.code(), "module_release_not_found");
    let message = err.to_string();
    assert!(
        message.contains("Git-2.3.4-signed.modl") && message.contains("Other-2.3.4-signed.modl"),
        "message must name both expected and present asset names: {message}"
    );
    assert_cache_empty(cache_root.path(), "git");
}

/// SC-2's precondition: `ign` never caches bytes it cannot verify — an
/// asset published with `digest: null` is refused before any download.
#[tokio::test]
async fn asset_without_digest_is_refused() {
    let mock = FeedMock::start().await;
    let release_json = serde_json::json!({
        "tag_name": "v2.3.4",
        "assets": [{
            "name": "Git-2.3.4-signed.modl",
            "url": format!("{}/assets/1", mock.uri()),
            "browser_download_url": format!("{}/download/Git-2.3.4-signed.modl", mock.uri()),
            "digest": serde_json::Value::Null,
            "content_type": "application/octet-stream",
            "size": mock.payload.len(),
            "state": "uploaded",
        }],
    });
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/repos/WhiskeyHouse/ignition-git-module/releases/tags/v2.3.4",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(release_json))
        .mount(&mock.server)
        .await;

    let cache_root = tempfile::tempdir().expect("tempdir");
    let feed =
        ModuleFeed::for_base(mock.uri().parse().expect("uri parses")).expect("feed builds");

    let err = feed
        .fetch_and_verify(&GIT_MODULE, "2.3.4", cache_root.path(), FetchPolicy::CacheFirst)
        .await
        .expect_err("digest-less asset must be refused");

    assert_eq!(err.code(), "module_feed_unusable");
    assert_eq!(err.exit_code(), 6);
    assert_cache_empty(cache_root.path(), "git");
}

/// Pitfall 5: a rate-limited 403 is NEVER reported as an auth failure —
/// distinguished by the `x-ratelimit-remaining` header, not status code
/// alone. A 403 WITHOUT the header is still `module_feed_unusable`, with
/// a detail that does not claim a rate limit.
#[tokio::test]
async fn rate_limited_feed_is_not_reported_as_auth() {
    let limited = FeedMock::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/repos/WhiskeyHouse/ignition-git-module/releases/tags/v2.3.4",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", "1234567890")
                .set_body_json(serde_json::json!({"message": "API rate limit exceeded"})),
        )
        .mount(&limited.server)
        .await;
    let cache_root = tempfile::tempdir().expect("tempdir");
    let feed =
        ModuleFeed::for_base(limited.uri().parse().expect("uri parses")).expect("feed builds");

    let err = feed
        .fetch_and_verify(&GIT_MODULE, "2.3.4", cache_root.path(), FetchPolicy::CacheFirst)
        .await
        .expect_err("rate-limited feed must be refused");
    assert_eq!(err.code(), "module_feed_unusable");
    assert_eq!(err.exit_code(), 6);
    let message = err.to_string().to_lowercase();
    assert!(message.contains("rate limit"), "message must name the rate limit: {message}");
    assert!(
        !message.contains("credential"),
        "message must not claim a credential problem: {message}"
    );

    let forbidden = FeedMock::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/repos/WhiskeyHouse/ignition-git-module/releases/tags/v2.3.4",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(403)
                .set_body_json(serde_json::json!({"message": "Forbidden"})),
        )
        .mount(&forbidden.server)
        .await;
    let cache_root2 = tempfile::tempdir().expect("tempdir");
    let feed2 =
        ModuleFeed::for_base(forbidden.uri().parse().expect("uri parses")).expect("feed builds");

    let err2 = feed2
        .fetch_and_verify(&GIT_MODULE, "2.3.4", cache_root2.path(), FetchPolicy::CacheFirst)
        .await
        .expect_err("plain 403 must still be refused as unusable, not auth");
    assert_eq!(err2.code(), "module_feed_unusable");
    let message2 = err2.to_string().to_lowercase();
    assert!(
        !message2.contains("rate limit"),
        "message must not claim a rate limit when the header is absent: {message2}"
    );
}

/// T-15-03: a hostile version or module id is refused BEFORE any HTTP
/// request is issued or any path is joined — asserted with a
/// `.expect(0)`-scoped mock over the whole server.
#[tokio::test]
async fn unsafe_version_is_refused_before_any_request() {
    let mock = FeedMock::start().await;
    let no_requests = wiremock::Mock::given(wiremock::matchers::any())
        .respond_with(wiremock::ResponseTemplate::new(500))
        .expect(0)
        .mount_as_scoped(&mock.server)
        .await;

    let cache_root = tempfile::tempdir().expect("tempdir");
    let feed =
        ModuleFeed::for_base(mock.uri().parse().expect("uri parses")).expect("feed builds");

    let long_version = "x".repeat(65);
    for unsafe_version in ["../escape", "a/b", "with\0null", "back\\slash", long_version.as_str()] {
        let err = feed
            .fetch_and_verify(&GIT_MODULE, unsafe_version, cache_root.path(), FetchPolicy::CacheFirst)
            .await
            .unwrap_err();
        assert_eq!(err.code(), "invalid_input", "version {unsafe_version:?} must be refused");
        assert_eq!(err.exit_code(), 2);
    }

    let unsafe_module = ignition_core::module::ModuleSpec {
        id: "../nope",
        repo: "WhiskeyHouse/ignition-git-module",
        tag_template: "v{version}",
        asset_template: "Git-{version}-signed.modl",
    };
    let err = feed
        .fetch_and_verify(&unsafe_module, "2.3.4", cache_root.path(), FetchPolicy::CacheFirst)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input", "unsafe module id must be refused");
    assert_eq!(err.exit_code(), 2);

    drop(no_requests);
}

// ---------------------------------------------------------------------
// 15-02 Task 1: digest mismatch is a hard refusal that caches nothing
// (SC-2), and an oversized body is aborted mid-stream. Three of the four
// tests below exercise the mismatch arm 15-01 already wrote; only
// `oversized_body_is_aborted_and_refused` drives genuinely new behavior
// (the streaming size cap) — see the plan's Task 1 action note.
// ---------------------------------------------------------------------

/// Mount a full happy-shaped release+download flow where the CALLER
/// controls the declared digest/size independently of what bytes are
/// actually served — the refusal tests below deliberately make these
/// disagree.
async fn mount_release_and_download(
    server: &wiremock::MockServer,
    declared_digest: &str,
    declared_size: u64,
    served_body: Vec<u8>,
) {
    let release_json = serde_json::json!({
        "tag_name": "v2.3.4",
        "assets": [{
            "name": "Git-2.3.4-signed.modl",
            "url": format!("{}/assets/1", server.uri()),
            "browser_download_url": format!("{}/download/Git-2.3.4-signed.modl", server.uri()),
            "digest": format!("sha256:{declared_digest}"),
            "content_type": "application/octet-stream",
            "size": declared_size,
            "state": "uploaded",
        }],
    });
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/repos/WhiskeyHouse/ignition-git-module/releases/tags/v2.3.4",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(release_json))
        .mount(server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/download/Git-2.3.4-signed.modl"))
        .respond_with(
            wiremock::ResponseTemplate::new(302)
                .insert_header("Location", "/cdn/Git-2.3.4-signed.modl"),
        )
        .mount(server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/cdn/Git-2.3.4-signed.modl"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_bytes(served_body))
        .mount(server)
        .await;
}

/// SC-2: the release publishes digest A, the CDN serves bytes hashing to
/// B. Refused, both full 64-char digests named, cache left empty.
#[tokio::test]
async fn digest_mismatch_refuses_and_caches_nothing() {
    let server = wiremock::MockServer::start().await;
    let good_payload = payload();
    let expected_digest = sha256_hex(&good_payload);
    let served_payload =
        b"entirely different bytes than what the release claims to publish here".to_vec();
    let actual_digest = sha256_hex(&served_payload);
    assert_ne!(expected_digest, actual_digest);

    mount_release_and_download(
        &server,
        &expected_digest,
        served_payload.len() as u64,
        served_payload.clone(),
    )
    .await;

    let cache_root = tempfile::tempdir().expect("tempdir");
    let feed =
        ModuleFeed::for_base(server.uri().parse().expect("uri parses")).expect("feed builds");

    let err = feed
        .fetch_and_verify(&GIT_MODULE, "2.3.4", cache_root.path(), FetchPolicy::CacheFirst)
        .await
        .expect_err("digest mismatch must refuse");

    assert_eq!(err.code(), "module_digest_mismatch");
    assert_eq!(err.exit_code(), 6);
    let message = err.to_string();
    assert!(
        message.contains(&expected_digest),
        "message must name the expected digest in full: {message}"
    );
    assert!(
        message.contains(&actual_digest),
        "message must name the actual digest in full: {message}"
    );
    assert_cache_empty(cache_root.path(), "git");
}

/// A PREFIX of the correct payload (the interrupted-download shape) is
/// the same mismatch refusal, not a partial cache entry.
#[tokio::test]
async fn truncated_body_is_a_mismatch_not_a_cache_entry() {
    let server = wiremock::MockServer::start().await;
    let full_payload = payload();
    let expected_digest = sha256_hex(&full_payload);
    let truncated = full_payload[..full_payload.len() / 2].to_vec();

    mount_release_and_download(
        &server,
        &expected_digest,
        full_payload.len() as u64,
        truncated,
    )
    .await;

    let cache_root = tempfile::tempdir().expect("tempdir");
    let feed =
        ModuleFeed::for_base(server.uri().parse().expect("uri parses")).expect("feed builds");

    let err = feed
        .fetch_and_verify(&GIT_MODULE, "2.3.4", cache_root.path(), FetchPolicy::CacheFirst)
        .await
        .expect_err("a truncated body must refuse as a mismatch");

    assert_eq!(err.code(), "module_digest_mismatch");
    assert_eq!(err.exit_code(), 6);
    assert_cache_empty(cache_root.path(), "git");
}

/// T-15-10: a body exceeding the release's published `size` is aborted
/// mid-stream and refused `module_feed_unusable`, never trusting
/// `Content-Length` in the published size's place. The declared digest
/// here IS the hash of the oversized bytes — isolating the size check
/// from the digest check: if the size cap did not fire first, the bytes
/// would otherwise verify cleanly and get cached.
#[tokio::test]
async fn oversized_body_is_aborted_and_refused() {
    let server = wiremock::MockServer::start().await;
    let base_payload = payload();
    let mut oversized = base_payload.clone();
    oversized
        .extend_from_slice(b"extra bytes the CDN should never have served past the published size");
    let digest_of_oversized = sha256_hex(&oversized);

    mount_release_and_download(
        &server,
        &digest_of_oversized,
        base_payload.len() as u64,
        oversized,
    )
    .await;

    let cache_root = tempfile::tempdir().expect("tempdir");
    let feed =
        ModuleFeed::for_base(server.uri().parse().expect("uri parses")).expect("feed builds");

    let err = feed
        .fetch_and_verify(&GIT_MODULE, "2.3.4", cache_root.path(), FetchPolicy::CacheFirst)
        .await
        .expect_err("an oversized body must be aborted and refused");

    assert_eq!(err.code(), "module_feed_unusable");
    assert_eq!(err.exit_code(), 6);
    let message = err.to_string();
    assert!(
        message.contains(&base_payload.len().to_string()),
        "message must name the published size: {message}"
    );
    assert_cache_empty(cache_root.path(), "git");
}

/// D-02/D-09: there is no policy, flag, or parameter that turns a
/// digest mismatch into a success — exercised under all three
/// `FetchPolicy` variants against the same mismatching mock.
#[tokio::test]
async fn mismatch_error_is_not_downgradable() {
    let server = wiremock::MockServer::start().await;
    let good_payload = payload();
    let expected_digest = sha256_hex(&good_payload);
    let served_payload = b"still the wrong bytes no matter which policy asks for them".to_vec();

    mount_release_and_download(
        &server,
        &expected_digest,
        served_payload.len() as u64,
        served_payload,
    )
    .await;

    let cache_root = tempfile::tempdir().expect("tempdir");
    let feed =
        ModuleFeed::for_base(server.uri().parse().expect("uri parses")).expect("feed builds");

    for policy in [
        FetchPolicy::CacheFirst,
        FetchPolicy::Refresh,
        FetchPolicy::AcceptUpstreamChange,
    ] {
        let err = feed
            .fetch_and_verify(&GIT_MODULE, "2.3.4", cache_root.path(), policy)
            .await
            .expect_err("a digest mismatch must refuse under every FetchPolicy");
        assert_eq!(
            err.code(),
            "module_digest_mismatch",
            "policy {policy:?} must still refuse"
        );
    }
    assert_cache_empty(cache_root.path(), "git");
}
