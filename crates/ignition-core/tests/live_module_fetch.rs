//! Opt-in live fetch of the real `ignition-git-module` release feed —
//! proves the wiremock fixtures in `module_fetch_contract.rs` are not a
//! fiction. `#[ignore]`-gated, following the `live_gateway.rs` convention,
//! AND additionally gated on `IGNITION_LIVE_MODULE_FETCH` being set: CI
//! never runs this (network + a real, publicly-hosted third-party
//! artifact), and a bare `-- --ignored` run with the env var absent is a
//! green no-op rather than a failure.
//!
//! ```text
//! IGNITION_LIVE_MODULE_FETCH=1 cargo test -p ignition-core --test live_module_fetch -- --ignored --nocapture
//! ```
//!
//! Known identity, live-verified during 15-RESEARCH.md: asset
//! `Git-2.3.4-signed.modl`, 7,704,578 bytes, sha256
//! `b74070346e587b1e1c14ff5093cc70625f7bfa14c2d24b24e7a0888a619b33eb`.
//! Nothing this test fetches is committed to the repo — the artifact
//! lives only in a `tempfile::TempDir` discarded when the test process
//! exits; no binary fixture enters version control.

use ignition_core::module::GIT_MODULE;
use ignition_core::module::fetch::{ArtifactSource, FetchPolicy, ModuleFeed};

/// The known real artifact size in bytes (15-RESEARCH.md, live-verified).
const KNOWN_BYTES: u64 = 7_704_578;

/// The known real artifact's full lowercase hex sha256 digest
/// (15-RESEARCH.md, live-verified).
const KNOWN_DIGEST: &str = "b74070346e587b1e1c14ff5093cc70625f7bfa14c2d24b24e7a0888a619b33eb";

/// `true` only when `IGNITION_LIVE_MODULE_FETCH` is explicitly set to `1`.
fn live_enabled() -> bool {
    std::env::var("IGNITION_LIVE_MODULE_FETCH").ok().as_deref() == Some("1")
}

fn skip(message: &str) {
    eprintln!("skipping: {message}");
}

/// Fetches the REAL `Git-2.3.4-signed.modl` from the real
/// `WhiskeyHouse/ignition-git-module` GitHub release feed, confirms the
/// recorded size/digest identity, and proves the offline guarantee
/// against the real artifact (not just a mock) with a second call
/// against the same temp cache root.
#[tokio::test]
#[ignore = "opt-in: set IGNITION_LIVE_MODULE_FETCH=1 to fetch the real release artifact"]
async fn live_fetches_and_verifies_git_module() {
    if !live_enabled() {
        skip("IGNITION_LIVE_MODULE_FETCH not set to 1");
        return;
    }

    let cache_root = tempfile::tempdir().expect("tempdir");
    let feed = ModuleFeed::github().expect("feed builds");

    let fetched = feed
        .fetch_and_verify(
            &GIT_MODULE,
            "2.3.4",
            cache_root.path(),
            FetchPolicy::CacheFirst,
        )
        .await
        .expect("live fetch of the real Git-2.3.4-signed.modl must succeed");

    assert_eq!(
        fetched.source,
        ArtifactSource::Download,
        "the first call against an empty cache must download"
    );
    assert_eq!(
        fetched.bytes, KNOWN_BYTES,
        "artifact size disagrees with the recorded identity — the fixture may have drifted"
    );
    assert_eq!(
        fetched.digest_sha256, KNOWN_DIGEST,
        "digest disagrees with the recorded identity — the fixture may have drifted"
    );
    assert_eq!(
        fetched.path.file_name().and_then(|name| name.to_str()),
        Some(format!("2.3.4-{KNOWN_DIGEST}.modl").as_str()),
        "persisted filename must carry the digest"
    );
    assert!(fetched.path.exists(), "persisted file must exist");

    // A second call against the SAME temp cache root proves the
    // offline guarantee against the REAL artifact, not just a mock.
    let second = feed
        .fetch_and_verify(
            &GIT_MODULE,
            "2.3.4",
            cache_root.path(),
            FetchPolicy::CacheFirst,
        )
        .await
        .expect("second fetch must serve from cache");
    assert_eq!(second.source, ArtifactSource::Cache);
    assert_eq!(second.digest_sha256, KNOWN_DIGEST);
    assert_eq!(second.path, fetched.path);
}
