//! Binary-level contract for `ign deploy projects --check` (25-04, DPRJ-05).
//!
//! The check reads `deploy/project.yaml`, builds the artifact from the
//! checkout and compares it with the live project over GET requests only.
//! Every test here drives the real binary against a wiremock gateway and
//! asserts on what the gateway received as well as on the envelope.

mod common;

use std::io::Write as _;
use std::path::{Path, PathBuf};

use common::verify_ign;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SCRIPT_RESOURCE: &str = r#"{"scope":"A","files":["code.py"],"attributes":{"enabled":true}}"#;
const CODE_V1: &[u8] = b"def f():\n    return 1\n";
const CODE_V2: &[u8] = b"def f():\n    return 2\n";

fn put(root: &Path, rel: &str, bytes: &[u8]) {
    let target = root.join(rel);
    std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
    std::fs::write(target, bytes).expect("write");
}

/// A project repo for project `CapApp`, deployable to `dev`, plus a targets
/// root holding `targets/dev.yaml` whose master profile is `dev`.
struct Fixture {
    _scratch: tempfile::TempDir,
    repo: PathBuf,
    targets: PathBuf,
}

fn fixture() -> Fixture {
    let scratch = tempfile::tempdir().expect("tempdir");
    let repo = scratch.path().join("repo");
    let targets = scratch.path().join("targets-root");
    let data = scratch.path().join("data");
    std::fs::create_dir_all(&data).expect("data dir");
    put(
        &repo,
        "project.json",
        br#"{"title":"Cap App","enabled":true}"#,
    );
    put(
        &repo,
        "deploy/project.yaml",
        b"schema_version: 1\nproject: CapApp\nenvironments: [dev]\n",
    );
    put(
        &repo,
        "ignition/script-python/cap/resource.json",
        SCRIPT_RESOURCE.as_bytes(),
    );
    put(&repo, "ignition/script-python/cap/code.py", CODE_V1);
    // Not part of the artifact.
    put(&repo, "docs/notes.md", b"not shipped");
    put(
        &targets,
        "targets/dev.yaml",
        format!(
            "schema_version: 1\ngateway: capgw\nmode: dev\nnodes:\n  master:\n    profile: dev\n    local:\n      data_dir: '{}'\n",
            data.display()
        )
        .as_bytes(),
    );
    Fixture {
        _scratch: scratch,
        repo,
        targets,
    }
}

/// An export zip of the project: the same members as the fixture repo with
/// `code` as the script body. `stamp` adds the volatile `lastModification`
/// attributes the gateway writes into `resource.json`, and the
/// `description` and `parent` keys it adds to `project.json`.
fn export_zip(code: &[u8], stamp: bool) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let project = if stamp {
        r#"{"title":"Cap App","enabled":true,"description":"","parent":""}"#
    } else {
        r#"{"title":"Cap App","enabled":true}"#
    };
    let resource = if stamp {
        r#"{"scope":"A","files":["code.py"],"attributes":{"enabled":true,"lastModification":{"actor":"admin","timestamp":"2026-10-22T10:00:00Z"},"lastModificationSignature":"abc"}}"#
    } else {
        SCRIPT_RESOURCE
    };
    for (name, bytes) in [
        ("project.json", project.as_bytes()),
        (
            "ignition/script-python/cap/resource.json",
            resource.as_bytes(),
        ),
        ("ignition/script-python/cap/code.py", code),
    ] {
        writer.start_file(name, opts).expect("start");
        writer.write_all(bytes).expect("write");
    }
    writer.finish().expect("finish").into_inner()
}

async fn mount_list(server: &MockServer, names: &[&str]) {
    let items: Vec<Value> = names
        .iter()
        .map(|n| json!({"name": n, "enabled": true}))
        .collect();
    let total = items.len();
    Mock::given(method("GET"))
        .and(path("/data/api/v1/projects/list"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": items,
            "metadata": {"total": total, "matching": total, "limit": -1, "offset": 0}
        })))
        .mount(server)
        .await;
}

async fn mount_find_found(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/data/api/v1/projects/find/CapApp"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"name": "CapApp", "title": "Cap App", "enabled": true})),
        )
        .mount(server)
        .await;
}

async fn mount_find_missing(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/data/api/v1/projects/find/CapApp"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json!({"message": "Project not found"})),
        )
        .mount(server)
        .await;
}

async fn mount_export(server: &MockServer, zip: Vec<u8>) {
    Mock::given(method("GET"))
        .and(path("/data/api/v1/projects/export/CapApp"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(zip, "application/zip")
                .insert_header(
                    "Content-Disposition",
                    "attachment; filename=\"CapApp-export.zip\"",
                ),
        )
        .mount(server)
        .await;
}

fn check_args(fx: &Fixture, env: &str) -> Vec<String> {
    vec![
        "--json".into(),
        "deploy".into(),
        "projects".into(),
        "--env".into(),
        env.into(),
        "--release".into(),
        "v1.0.0".into(),
        "--check".into(),
        "--repo".into(),
        fx.repo.display().to_string(),
        "--targets".into(),
        fx.targets.display().to_string(),
    ]
}

fn run(server: &MockServer, args: &[String]) -> std::process::Output {
    let (_dir, mut cmd) = verify_ign(&server.uri());
    cmd.args(args).output().expect("spawn ign")
}

fn data_of(out: &std::process::Output) -> Value {
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    assert_eq!(envelope["ok"], true);
    envelope["data"].clone()
}

async fn methods(server: &MockServer) -> Vec<(String, String)> {
    server
        .received_requests()
        .await
        .expect("request recording is on")
        .iter()
        .map(|r| (r.method.to_string(), r.url.path().to_string()))
        .collect()
}

fn assert_only_gets(requests: &[(String, String)]) {
    assert!(!requests.is_empty(), "the check must read the gateway");
    for (verb, url) in requests {
        assert_eq!(verb, "GET", "{url}: a check sends GET requests only");
    }
}

fn is_hex64(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
}

#[tokio::test]
async fn projects_check_reports_import() {
    let fx = fixture();
    let server = MockServer::start().await;
    mount_list(&server, &["CapApp"]).await;
    mount_find_found(&server).await;
    mount_export(&server, export_zip(CODE_V2, true)).await;

    let out = run(&server, &check_args(&fx, "dev"));
    let data = data_of(&out);
    assert_eq!(data["action"], "import");
    assert_eq!(data["would_change"], true);
    assert_eq!(data["members"]["changed"], 1);
    assert_eq!(data["members"]["same"], 2);
    assert_eq!(data["members"]["added"], 0);
    assert_eq!(data["members"]["removed"], 0);
    assert_eq!(data["check"], true);
    assert_eq!(data["outcome"], "checked");
    assert_eq!(data["project"], "CapApp");
    assert_eq!(data["gateway"], "capgw");
    assert_eq!(data["release"], "v1.0.0");
    assert_eq!(data["env"], "dev");
    assert!(is_hex64(&data["artifact_sha256"]), "{data}");
    assert!(is_hex64(&data["live_sha256"]), "{data}");
    assert_ne!(data["artifact_sha256"], data["live_sha256"]);
    assert_eq!(data["removed"], json!([]));
    assert_eq!(data["ok"], true);
    assert!(data["slug"].is_null());
    assert_only_gets(&methods(&server).await);
}

#[tokio::test]
async fn projects_check_reports_skip() {
    let fx = fixture();
    let server = MockServer::start().await;
    mount_list(&server, &["CapApp"]).await;
    mount_find_found(&server).await;
    mount_export(&server, export_zip(CODE_V1, true)).await;

    let out = run(&server, &check_args(&fx, "dev"));
    let data = data_of(&out);
    assert_eq!(data["action"], "skip");
    assert_eq!(data["would_change"], false);
    assert_eq!(data["artifact_sha256"], data["live_sha256"]);
    assert_eq!(data["members"]["changed"], 0);
    assert_eq!(data["members"]["same"], 3);
    assert_only_gets(&methods(&server).await);
}

#[tokio::test]
async fn projects_check_reports_create() {
    let fx = fixture();
    let server = MockServer::start().await;
    mount_list(&server, &[]).await;
    mount_find_missing(&server).await;

    let out = run(&server, &check_args(&fx, "dev"));
    let data = data_of(&out);
    assert_eq!(data["action"], "create");
    assert_eq!(data["would_change"], true);
    assert!(data["live_sha256"].is_null());
    assert!(is_hex64(&data["artifact_sha256"]));
    assert_eq!(data["members"]["added"], 3);
    let requests = methods(&server).await;
    assert_only_gets(&requests);
    assert!(
        requests.iter().all(|(_, url)| !url.contains("/export/")),
        "an absent project has nothing to export: {requests:?}"
    );
}

#[tokio::test]
async fn projects_check_lists_unmanaged() {
    let fx = fixture();
    let server = MockServer::start().await;
    mount_list(&server, &["CapApp", "Other", "Global"]).await;
    mount_find_found(&server).await;
    mount_export(&server, export_zip(CODE_V1, false)).await;

    let data = data_of(&run(&server, &check_args(&fx, "dev")));
    assert_eq!(data["unmanaged"], json!(["Global", "Other"]));
    assert_eq!(data["unmanaged_count"], 2);
    let requests = methods(&server).await;
    assert_only_gets(&requests);
    assert!(
        requests
            .iter()
            .all(|(_, url)| !url.contains("Other") && !url.contains("Global")),
        "other projects are listed by name and never read: {requests:?}"
    );
}

#[tokio::test]
async fn projects_check_reports_removed_members() {
    let fx = fixture();
    let server = MockServer::start().await;
    // The live project holds a member the repo lacks.
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    for (name, bytes) in [
        (
            "project.json",
            br#"{"title":"Cap App","enabled":true}"#.as_slice(),
        ),
        (
            "ignition/script-python/cap/resource.json",
            SCRIPT_RESOURCE.as_bytes(),
        ),
        ("ignition/script-python/cap/code.py", CODE_V1),
        ("ignition/script-python/old/code.py", b"gone"),
        (
            "ignition/script-python/old/resource.json",
            br#"{"files":["code.py"]}"#.as_slice(),
        ),
    ] {
        writer.start_file(name, opts).expect("start");
        writer.write_all(bytes).expect("write");
    }
    let live = writer.finish().expect("finish").into_inner();
    mount_list(&server, &["CapApp"]).await;
    mount_find_found(&server).await;
    mount_export(&server, live).await;

    let data = data_of(&run(&server, &check_args(&fx, "dev")));
    assert_eq!(data["action"], "import");
    assert_eq!(data["members"]["removed"], 2);
    assert_eq!(data["removed_count"], 2);
    assert_eq!(
        data["removed"],
        json!([
            "ignition/script-python/old/code.py",
            "ignition/script-python/old/resource.json"
        ])
    );
}

#[tokio::test]
async fn projects_check_env_not_listed() {
    let fx = fixture();
    // A target file for prod exists; the project file does not list it.
    put(
        &fx.targets,
        "targets/prod.yaml",
        std::fs::read(fx.targets.join("targets/dev.yaml"))
            .expect("dev target")
            .as_slice(),
    );
    let server = MockServer::start().await;

    let out = run(&server, &check_args(&fx, "prod"));
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty(), "a refusal leaves stdout empty");
    let err: Value = serde_json::from_slice(&out.stderr).expect("stderr envelope");
    assert_eq!(err["error"]["code"], "deploy_tree_invalid");
    assert!(
        err["error"]["message"]
            .as_str()
            .expect("message")
            .contains("deploy/project.yaml"),
        "{err}"
    );
    assert!(
        methods(&server).await.is_empty(),
        "the refusal comes before any request"
    );
}

#[tokio::test]
async fn projects_check_needs_check_flag() {
    let fx = fixture();
    let server = MockServer::start().await;
    let mut args = check_args(&fx, "dev");
    args.retain(|a| a != "--check");

    let out = run(&server, &args);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("--check"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(methods(&server).await.is_empty());
}

#[tokio::test]
async fn projects_check_creates_no_ign_dir() {
    let fx = fixture();
    let server = MockServer::start().await;
    mount_list(&server, &["CapApp"]).await;
    mount_find_found(&server).await;
    mount_export(&server, export_zip(CODE_V2, true)).await;

    let out = run(&server, &check_args(&fx, "dev"));
    data_of(&out);
    assert!(
        !fx.repo.join(".ign").exists(),
        "a check writes no run directory"
    );
    assert!(!fx.targets.join(".ign").exists());
    // The check also left the repo exactly as it was.
    assert!(!fx.repo.join("deploy/.ign").exists());
}

#[tokio::test]
async fn projects_check_targets_default_to_the_repo() {
    let fx = fixture();
    let server = MockServer::start().await;
    mount_list(&server, &["CapApp"]).await;
    mount_find_found(&server).await;
    mount_export(&server, export_zip(CODE_V1, true)).await;
    // Move the target file into the repo and drop `--targets`.
    let bytes = std::fs::read(fx.targets.join("targets/dev.yaml")).expect("target");
    put(&fx.repo, "targets/dev.yaml", &bytes);
    let mut args = check_args(&fx, "dev");
    let at = args.iter().position(|a| a == "--targets").expect("flag");
    args.drain(at..=at + 1);

    let data = data_of(&run(&server, &args));
    assert_eq!(data["action"], "skip");
}

#[tokio::test]
async fn projects_check_missing_project_json_refuses() {
    let fx = fixture();
    std::fs::remove_file(fx.repo.join("project.json")).expect("rm");
    let server = MockServer::start().await;

    let out = run(&server, &check_args(&fx, "dev"));
    assert_eq!(out.status.code(), Some(2));
    let err: Value = serde_json::from_slice(&out.stderr).expect("stderr envelope");
    assert_eq!(err["error"]["code"], "deploy_tree_invalid");
    assert!(
        err["error"]["message"]
            .as_str()
            .expect("message")
            .contains("a project repo holds project.json at its root"),
        "{err}"
    );
    assert!(methods(&server).await.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn projects_check_symlink_in_the_walk_refuses() {
    let fx = fixture();
    let outside = tempfile::tempdir().expect("outside");
    std::fs::write(outside.path().join("secret.txt"), b"secret").expect("write");
    std::os::unix::fs::symlink(
        outside.path().join("secret.txt"),
        fx.repo.join("ignition/script-python/cap/link.py"),
    )
    .expect("symlink");
    let server = MockServer::start().await;

    let out = run(&server, &check_args(&fx, "dev"));
    assert_eq!(out.status.code(), Some(2));
    let err: Value = serde_json::from_slice(&out.stderr).expect("stderr envelope");
    assert_eq!(err["error"]["code"], "deploy_tree_invalid");
    let message = err["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("ignition/script-python/cap/link.py"),
        "{message}"
    );
    assert!(!message.contains("secret"), "{message}");
    assert!(methods(&server).await.is_empty());
}

#[tokio::test]
async fn projects_check_human_lines() {
    let fx = fixture();
    let server = MockServer::start().await;
    mount_list(&server, &["CapApp", "Other"]).await;
    mount_find_found(&server).await;
    mount_export(&server, export_zip(CODE_V2, true)).await;
    let mut args = check_args(&fx, "dev");
    args.remove(0); // human mode

    let out = run(&server, &args);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    assert!(
        stdout.contains("project CapApp on capgw (env dev, release v1.0.0)"),
        "{stdout}"
    );
    assert!(stdout.contains("check: import (would change)"), "{stdout}");
    assert!(stdout.contains("artifact sha256 "), "{stdout}");
    assert!(
        stdout.contains("members: 0 added, 1 changed, 0 removed, 2 same"),
        "{stdout}"
    );
    assert!(
        stdout.contains("other projects on the gateway: 1"),
        "{stdout}"
    );
}
