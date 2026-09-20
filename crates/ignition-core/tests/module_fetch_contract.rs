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
