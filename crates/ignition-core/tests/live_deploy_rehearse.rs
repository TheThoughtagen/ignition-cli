//! The rig gate for `ign deploy projects --rehearse` (Phase 26 hardening): a
//! rehearsal of a shape-changing run whose rehearsal gateway is the real
//! Ignition 8.3.6 rig. The production side is `FakeProjects` behind
//! `ForbidProjectWrites` (it panics on any write); the rig takes the seed,
//! the run's prune batches, the forward verify and the forced rollback.
//!
//! ```text
//! cd crates/ignition-core/tests/fixtures/live-rig-deploy
//! bash capture25.sh up25     # the single-node rig with its data root bound
//! . ~/.cache/ign-deploy-rig/tokens.env
//! IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 \
//!   IGNITION_LIVE_TOKEN="$IGN_TOKEN_CAP_A" IGN_DEPLOY_RIG_DATA=<the data root> \
//!   cargo test -p ignition-core --test live_deploy_rehearse -- --ignored --nocapture \
//!   --test-threads=1
//! ```
//!
//! | var | meaning |
//! |---|---|
//! | `IGNITION_LIVE_DEPLOY` | must equal `1`; anything else is a quiet green skip |
//! | `IGNITION_LIVE_URL` | the rig's base URL; loopback hosts only |
//! | `IGNITION_LIVE_TOKEN` | cap-a's full `name:key` API token (never printed) |
//! | `IGN_DEPLOY_RIG_DATA` | the rig's data root (default `$HOME/.cache/ign-deploy-rig/data25`) |
//!
//! Safety: the URL guard refuses a non-loopback host before a request is
//! made, and the gateway must report `gateway-info` name `ign-deploy-rig`,
//! which the rehearsal target declares. The gate uses one disposable
//! provider, `P26Rehearse`, and deletes it on entry and on exit, even when
//! an assertion fails. The production tree is first written to the rig and
//! exported back, so the fake production gateway holds the gateway's own
//! form of it. Output carries names, counts and hashes, never a token.

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::fake_projects::{FakeProjects, ForbidProjectWrites};
use ignition_core::actions::deploy::load_target;
use ignition_core::actions::deploy_projects::{
    GatewayProjects, NoProjectsObserver, ProjectGateway, ProjectsInput, ProjectsSurfaces,
    RehearsalTarget, RehearseOptions, TAG_VERIFY_POLL_S, TAG_VERIFY_SETTLE_S, TagDeleteSurface,
    deploy_projects_rehearse, deploy_projects_rehearse_prepare, load_project,
};
use ignition_core::client::tags::{CollisionPolicy, TagProviderCreate};
use ignition_core::client::{GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};
use ignition_core::deploy::projects::ReleaseRecord;
use ignition_core::deploy::tag_record::TagRecord;
use ignition_core::deploy::transport::local::LocalFsTransport;
use ignition_core::error::CoreError;
use serde_json::{Value, json};

const RIG_GATEWAY_NAME: &str = "ign-deploy-rig";
const PROVIDER: &str = "P26Rehearse";
const PROJECT: &str = "P26RehearseApp";

const SCALE_TYPE: &str = r#"{"tagType":"UdtType","tags":[{"dataType":"Int4","name":"Net","tagType":"AtomicTag","value":0,"valueSource":"memory"}]}"#;
const SCANNER_FOLDER: &str = r#"{"tagType":"UdtType","tags":[{"dataType":"Int4","name":"Speed","tagType":"AtomicTag","value":0,"valueSource":"memory"},{"name":"Weigh","tagType":"Folder","tags":[{"dataType":"Int4","name":"Gross","tagType":"AtomicTag","value":0,"valueSource":"memory"}]}]}"#;
const SCANNER_INSTANCE: &str = r#"{"tagType":"UdtType","tags":[{"dataType":"Int4","name":"Speed","tagType":"AtomicTag","value":0,"valueSource":"memory"},{"name":"Weigh","tagType":"UdtInstance","typeId":"Scale"}]}"#;
const S1_FOLDER: &str = r#"{"tagType":"UdtInstance","tags":[{"name":"Weigh","tagType":"Folder","tags":[{"name":"Gross","tagType":"AtomicTag","tooltip":"g"}]}],"typeId":"Scanner"}"#;
const S1_INSTANCE: &str = r#"{"tagType":"UdtInstance","tags":[{"name":"Weigh","tagType":"UdtInstance","tooltip":"w","typeId":"Scale"}],"typeId":"Scanner"}"#;

fn skip(message: &str) {
    eprintln!("skipping: {message}");
}

/// Parse `raw` and accept it only when it points at this machine.
fn local_url_guard(raw: &str) -> Result<url::Url, String> {
    let parsed = url::Url::parse(raw).map_err(|err| format!("not a URL ({err})"))?;
    let loopback = match parsed.host() {
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(addr)) => addr == std::net::Ipv4Addr::LOCALHOST,
        Some(url::Host::Ipv6(addr)) => addr == std::net::Ipv6Addr::LOCALHOST,
        None => false,
    };
    if matches!(parsed.scheme(), "http" | "https") && loopback {
        Ok(parsed)
    } else {
        Err("not a loopback http(s) URL".to_string())
    }
}

fn require_env(name: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| panic!("{name} must be set to run the live rehearsal gate"))
}

#[test]
fn local_url_guard_refuses_non_loopback() {
    for raw in [
        "http://192.0.2.15:8088",
        "http://localhost.evil.example:18290",
        "not a url",
    ] {
        assert!(local_url_guard(raw).is_err(), "must refuse {raw}");
    }
    assert!(local_url_guard("http://localhost:18290").is_ok());
}

fn put(root: &Path, rel: &str, bytes: &[u8]) {
    let target = root.join(rel);
    std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
    std::fs::write(target, bytes).expect("write");
}

fn named(json: &str, name: &str) -> Value {
    let mut v: Value = serde_json::from_str(json).expect("json");
    v["name"] = json!(name);
    v
}

/// Delete the disposable provider on the rig, if it is there.
async fn remove_provider(api: &ReqwestGatewayApi) {
    match api.tag_provider_find(PROVIDER).await {
        Ok(record) => {
            let signature = record.signature.unwrap_or_default();
            if let Err(err) = api.tag_provider_delete(PROVIDER, &signature).await {
                eprintln!("cleanup: cannot delete provider {PROVIDER}: {}", err.code());
            }
        }
        Err(CoreError::NotFound { .. }) => {}
        Err(err) => eprintln!("cleanup: cannot find provider {PROVIDER}: {}", err.code()),
    }
}

/// Deletes the disposable provider when the test ends, even by a panic.
struct ProviderGuard {
    url: String,
    token: String,
}

impl Drop for ProviderGuard {
    fn drop(&mut self) {
        let (url, token) = (self.url.clone(), self.token.clone());
        let joined = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("cleanup runtime")
                .block_on(async {
                    let api = ReqwestGatewayApi::for_tests(
                        &url,
                        Some(Credential::Token(Secret::new(token))),
                    );
                    remove_provider(&api).await;
                });
        })
        .join();
        if joined.is_err() {
            eprintln!("cleanup: the provider delete thread panicked");
        }
    }
}

/// A shape-changing run (a type member turned from a folder into an
/// instance) rehearsed on the rig: green, prunes forward and back, the
/// record written, and the production side never written to.
#[tokio::test]
#[ignore = "needs the 8.3.6 rig: see the module doc"]
async fn rehearsal_on_the_rig() {
    if std::env::var("IGNITION_LIVE_DEPLOY").as_deref() != Ok("1") {
        skip("IGNITION_LIVE_DEPLOY is not 1");
        return;
    }
    let url = require_env("IGNITION_LIVE_URL");
    let token = require_env("IGNITION_LIVE_TOKEN");
    if let Err(why) = local_url_guard(&url) {
        panic!("IGNITION_LIVE_URL refused: {why}");
    }
    let api =
        ReqwestGatewayApi::for_tests(&url, Some(Credential::Token(Secret::new(token.clone()))));
    let info = GatewayApi::gateway_info(&api)
        .await
        .expect("gateway-info read for the name guard");
    assert_eq!(
        info.name.as_deref(),
        Some(RIG_GATEWAY_NAME),
        "refusing: this gateway is not the {RIG_GATEWAY_NAME} fixture"
    );
    eprintln!("LIVE-RAN rehearsal_on_the_rig");
    let rig_data = std::env::var("IGN_DEPLOY_RIG_DATA").map_or_else(
        |_| {
            PathBuf::from(std::env::var("HOME").expect("HOME")).join(".cache/ign-deploy-rig/data25")
        },
        PathBuf::from,
    );
    remove_provider(&api).await;
    let _guard = ProviderGuard {
        url: url.clone(),
        token,
    };

    // The production tree in the gateway's own form: written to the rig,
    // exported back, then the provider dropped again.
    let standard = json!({"profile": {"type": "STANDARD"}, "settings": {}});
    api.tag_provider_create(&[TagProviderCreate::standard(PROVIDER)])
        .await
        .expect("the provider is created");
    let rig = GatewayProjects(&api);
    let pre = json!({"tags": [
        {"name":"_types_","tagType":"Folder","tags":[
            named(SCALE_TYPE, "Scale"), named(SCANNER_FOLDER, "Scanner")
        ]},
        {"name":"Line1","tagType":"Folder","tags":[named(S1_FOLDER, "S1")]}
    ]});
    let answer = rig
        .tag_import(
            PROVIDER,
            "",
            CollisionPolicy::Overwrite,
            serde_json::to_vec(&pre).expect("json"),
        )
        .await
        .expect("the production tree is written");
    assert!(answer.is_clean(), "{answer:?}");
    tokio::time::sleep(Duration::from_secs(3)).await;
    let gateway_form: Value =
        serde_json::from_slice(&rig.tag_export(PROVIDER).await.expect("export")).expect("json");
    remove_provider(&api).await;

    let scratch = tempfile::tempdir().expect("tempdir");
    let repo = scratch.path().join("repo");
    let targets = scratch.path().join("targets-root");
    let data = scratch.path().join("data");
    std::fs::create_dir_all(data.join("projects")).expect("data");
    let members: Vec<(&str, Vec<u8>)> = vec![
        (
            "project.json",
            br#"{"title":"Rehearse","enabled":true}"#.to_vec(),
        ),
        (
            "ignition/script-python/r/resource.json",
            br#"{"scope":"A","version":1,"files":["code.py"],"attributes":{"enabled":true}}"#
                .to_vec(),
        ),
        ("ignition/script-python/r/code.py", b"x = 1\n".to_vec()),
    ];
    for (rel, bytes) in &members {
        put(&repo, rel, bytes);
    }
    put(
        &repo,
        "deploy/project.yaml",
        format!(
            "schema_version: 1\nproject: {PROJECT}\nenvironments: [prod]\ntag_providers: \
             [{PROVIDER}]\n"
        )
        .as_bytes(),
    );
    put(
        &repo,
        &format!("deploy/tag-providers/{PROVIDER}.json"),
        &serde_json::to_vec(&json!({"schema_version": 1, "enabled": true, "config": standard}))
            .expect("json"),
    );
    for (path, body) in [
        ("_types_/Scale", SCALE_TYPE),
        ("_types_/Scanner", SCANNER_INSTANCE),
        ("Line1/S1", S1_INSTANCE),
    ] {
        put(
            &repo,
            &format!("tags/{PROVIDER}/{path}.json"),
            body.as_bytes(),
        );
    }
    put(
        &data,
        "config/resources/core/ignition/system-properties/config.json",
        br#"{"systemName":"p26-prod"}"#,
    );
    put(
        &targets,
        "targets/prod.yaml",
        format!(
            "schema_version: 1\ngateway: p26-prod\nmode: dev\nnodes:\n  master:\n    profile: \
             p26-prod\n    local:\n      data_dir: '{}'\n",
            data.display()
        )
        .as_bytes(),
    );
    put(
        &targets,
        "targets/rig.yaml",
        format!(
            "schema_version: 1\ngateway: {RIG_GATEWAY_NAME}\nmode: production\nrehearsal: \
             true\nnodes:\n  master:\n    profile: rig\n    gateway_name: {RIG_GATEWAY_NAME}\n    \
             local:\n      data_dir: '{}'\n",
            rig_data.display()
        )
        .as_bytes(),
    );

    let prod = FakeProjects::new()
        .with_gateway_name("p26-prod")
        .with_project(PROJECT, None, false, &members)
        .with_tag_tree(PROVIDER, gateway_form);
    prod.set_provider_settings(
        PROVIDER,
        true,
        None,
        json!({"profile": {"type": "STANDARD"}, "settings": {}}),
    );
    let live = prod.export_bytes(PROJECT).await.expect("export");
    let digest = ignition_core::deploy::project_artifact::canonical_digest(&live).expect("digest");
    put(
        &data,
        &format!("ign-deploy/p26-prod/projects/{PROJECT}/release.json"),
        &ReleaseRecord {
            schema_version: 1,
            gateway: "p26-prod".to_string(),
            project: PROJECT.to_string(),
            release: "v0.9.0".to_string(),
            artifact_sha256: digest.sha256,
            run_id: "20261020T101500Z-ab12".to_string(),
            deployed_at: "2026-10-20T10:15:00Z".to_string(),
        }
        .to_bytes(),
    );
    put(
        &data,
        &format!("ign-deploy/p26-prod/tag-providers/{PROVIDER}/record.json"),
        &TagRecord {
            schema_version: 1,
            gateway: "p26-prod".to_string(),
            provider: PROVIDER.to_string(),
            project: PROJECT.to_string(),
            release: "v0.9.0".to_string(),
            run_id: "20261020T101500Z-ab12".to_string(),
            deployed_at: "2026-10-20T10:15:00Z".to_string(),
            tags_sha256: "ab".repeat(32),
            nodes: vec!["Line1".to_string()],
            groups: Vec::new(),
        }
        .to_bytes(),
    );

    let input = ProjectsInput {
        repo: repo.clone(),
        targets: targets.clone(),
        env: "prod".to_string(),
        release: "v1.0.0".to_string(),
        allow_downgrade: false,
        allow_first_change: None,
        allow_destructive: false,
        allow_manual_undo: true,
        artifact_sha256: None,
        tags_sha256: None,
    };
    let loaded = load_project(&input).expect("the repo loads");
    let forbid = ForbidProjectWrites(prod);
    let host = LocalFsTransport::new(data.clone(), "nodes.master");
    let rig_host = LocalFsTransport::new(rig_data.clone(), "nodes.master");
    let env = ProjectsSurfaces {
        gateway: &forbid as &dyn ProjectGateway,
        host: &host,
        tree: None,
        apply_host: None,
        deploy: None,
        tag_delete: TagDeleteSurface::NotConfigured { missing: vec![] },
    };
    let (target_file, target) = load_target(&targets, "rig").expect("the rig target");
    let rehearsal = RehearsalTarget {
        env: "rig".to_string(),
        target_file,
        target,
        surfaces: ProjectsSurfaces {
            gateway: &rig,
            host: &rig_host,
            tree: None,
            apply_host: None,
            deploy: None,
            tag_delete: TagDeleteSurface::NotConfigured { missing: vec![] },
        },
        url: url.clone(),
        env_urls: vec!["https://p26-prod.invalid:8043".to_string()],
    };
    let prepared = deploy_projects_rehearse_prepare(&loaded, &input, &env, &rehearsal)
        .await
        .expect("the rehearsal is prepared");
    let result = deploy_projects_rehearse(
        prepared,
        &loaded,
        &input,
        &env,
        &rehearsal,
        &RehearseOptions {
            repo: repo.clone(),
            targets_root: targets.clone(),
            ign_version: env!("CARGO_PKG_VERSION").to_string(),
            tag_verify_settle: Duration::from_secs(TAG_VERIFY_SETTLE_S),
            tag_verify_poll: Duration::from_secs(TAG_VERIFY_POLL_S),
            tag_delete_ready: Duration::from_secs(1),
            tag_delete_poll: Duration::from_millis(200),
        },
        &NoProjectsObserver,
    )
    .await
    .expect("a verdict");
    let data_json = serde_json::to_value(&result).expect("json");
    eprintln!(
        "rehearsal {}: {}",
        result.rehearsal.id,
        serde_json::to_string_pretty(&data_json["rehearsal"]).expect("json")
    );
    assert!(result.ok, "{data_json:#}");
    let pruned = json!([
        "[P26Rehearse]Line1/S1/Weigh",
        "[P26Rehearse]_types_/Scanner/Weigh"
    ]);
    assert_eq!(data_json["rehearsal"]["forward"]["pruned"], pruned);
    assert_eq!(data_json["rehearsal"]["rollback"]["green"], true);
    assert_eq!(data_json["rehearsal"]["rollback"]["pruned"], pruned);
    assert!(repo.join(&result.rehearsal.record).is_file());
}
