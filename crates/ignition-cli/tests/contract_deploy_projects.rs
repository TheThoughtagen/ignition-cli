//! Binary-level contract for `ign deploy projects --check` (25-04, DPRJ-05).
//!
//! The check reads `deploy/project.yaml`, builds the artifact from the
//! checkout and compares it with the live project over GET requests only.
//! Every test here drives the real binary against a wiremock gateway and
//! asserts on what the gateway received as well as on the envelope.

mod common;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use common::verify_ign;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

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
    /// The master node's data directory; `projects/` exists and is empty.
    data: PathBuf,
}

fn fixture() -> Fixture {
    let scratch = tempfile::tempdir().expect("tempdir");
    let repo = scratch.path().join("repo");
    let targets = scratch.path().join("targets-root");
    let data = scratch.path().join("data");
    std::fs::create_dir_all(data.join("projects")).expect("data dir");
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
        data,
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

    // No release record: the first `ign` deploy of this project, which
    // differs from the gateway, needs its reason (D-12).
    let out = run(
        &server,
        &args_with(&fx, "v1.0.0", &["--allow-first-change", "adopt the repo"]),
    );
    let data = data_of(&out);
    assert_eq!(data["action"], "import");
    assert_eq!(data["allow_first_change"], "adopt the repo");
    assert!(data["previous_release"].is_null());
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

/// K14: an unchanged project with no record plans the record only; once the
/// record names this release and this content, nothing is planned. The record
/// is written the way the last deploy left it, with the digest the first check
/// reported.
#[tokio::test]
async fn projects_check_reports_skip() {
    let fx = fixture();
    let server = MockServer::start().await;
    mount_list(&server, &["CapApp"]).await;
    mount_find_found(&server).await;
    mount_export(&server, export_zip(CODE_V1, true)).await;

    let first = data_of(&run(&server, &check_args(&fx, "dev")));
    assert_eq!(first["action"], "record");
    let sha = first["artifact_sha256"].as_str().expect("sha").to_string();
    put_record_sha(&fx, "v1.0.0", &sha);

    let out = run(&server, &check_args(&fx, "dev"));
    let data = data_of(&out);
    assert_eq!(data["action"], "skip");
    assert_eq!(data["would_change"], false);
    assert_eq!(data["artifact_sha256"], data["live_sha256"]);
    assert_eq!(data["recorded_sha256"], sha.as_str());
    assert_eq!(data["live_matches_last_deploy"], true);
    assert_eq!(data["warnings"], json!([]));
    assert_eq!(data["members"]["changed"], 0);
    assert_eq!(data["members"]["same"], 3);
    assert_only_gets(&methods(&server).await);
}

/// K14, D-12: the git module left the project as the repo has it, no record:
/// only the record is planned, and it counts as a change.
#[tokio::test]
async fn projects_check_reports_record() {
    let fx = fixture();
    let server = MockServer::start().await;
    mount_list(&server, &["CapApp"]).await;
    mount_find_found(&server).await;
    mount_export(&server, export_zip(CODE_V1, true)).await;

    let data = data_of(&run(&server, &check_args(&fx, "dev")));
    assert_eq!(data["action"], "record");
    assert_eq!(data["would_change"], true);
    assert_eq!(data["artifact_sha256"], data["live_sha256"]);
    assert!(data["previous_release"].is_null());
    assert!(data["recorded_sha256"].is_null());
    assert!(data["live_matches_last_deploy"].is_null());
    assert!(data["allow_first_change"].is_null());
    assert_only_gets(&methods(&server).await);

    // A newer release over unchanged content records the release too.
    put_record(&fx, "v0.9.0");
    let data = data_of(&run(&server, &check_args(&fx, "dev")));
    assert_eq!(data["action"], "record");
    assert_eq!(data["previous_release"], "v0.9.0");
}

/// D-12, V3: no record and a gateway project that differs: the check refuses
/// like the apply would, naming the gate, the project and the flag.
#[tokio::test]
async fn projects_check_first_deploy_change_refuses() {
    let fx = fixture();
    let server = MockServer::start().await;
    mount_list(&server, &["CapApp"]).await;
    mount_find_found(&server).await;
    mount_export(&server, export_zip(CODE_V2, true)).await;

    let out = run(&server, &check_args(&fx, "dev"));
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = error_of(&out);
    assert_eq!(err["error"]["code"], "destructive_change_refused");
    let message = err["error"]["message"].as_str().expect("message");
    for needle in ["first_deploy", "CapApp", "--allow-first-change"] {
        assert!(message.contains(needle), "{needle}: {message}");
    }
    assert_only_gets(&methods(&server).await);
}

/// K13: the gateway no longer holds what `ign` last deployed. A warning in
/// the result, not a refusal.
#[tokio::test]
async fn projects_check_live_drift_warns() {
    let fx = fixture();
    put_record(&fx, "v0.9.0");
    let server = MockServer::start().await;
    mount_list(&server, &["CapApp"]).await;
    mount_find_found(&server).await;
    mount_export(&server, export_zip(CODE_V2, true)).await;

    let data = data_of(&run(&server, &check_args(&fx, "dev")));
    assert_eq!(data["action"], "import");
    assert_eq!(data["live_matches_last_deploy"], false);
    assert_eq!(data["recorded_sha256"], "a".repeat(64));
    let warnings = data["warnings"].as_array().expect("warnings");
    assert_eq!(warnings.len(), 1, "{data}");
    let text = warnings[0].as_str().expect("text");
    assert!(text.contains("CapApp") && text.contains("v0.9.0"), "{text}");
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

    let data = data_of(&run(
        &server,
        &args_with(&fx, "v1.0.0", &["--allow-first-change", "adopt"]),
    ));
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
async fn projects_check_creates_no_ign_dir() {
    let fx = fixture();
    let server = MockServer::start().await;
    mount_list(&server, &["CapApp"]).await;
    mount_find_found(&server).await;
    mount_export(&server, export_zip(CODE_V2, true)).await;

    let out = run(
        &server,
        &args_with(&fx, "v1.0.0", &["--allow-first-change", "adopt"]),
    );
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
    assert_eq!(data["action"], "record", "equal content, no record yet");
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
    let mut args = args_with(&fx, "v1.0.0", &["--allow-first-change", "adopt the repo"]);
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
    assert!(
        stdout.contains("release: v1.0.0 (last deployed by this tool: none)"),
        "{stdout}"
    );
    assert!(
        stdout.contains("first change allowed: adopt the repo"),
        "{stdout}"
    );
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

/// Put the noise a real checkout carries into the fixture repo, and an
/// `.ignexclude` that removes a scratch script package that would otherwise
/// ship.
fn add_noise(repo: &Path) {
    put(repo, "tags/t.json", b"{}");
    put(repo, "images/i.png", b"png");
    put(
        repo,
        "ignition/script-python/cap/__pycache__/code.cpython-311.pyc",
        b"pyc",
    );
    put(repo, "ignition/script-python/cap/notes.txt", b"unlisted");
    put(repo, "ignition/.git/HEAD", b"ref: refs/heads/main");
    put(repo, ".ign/deploy/gw/run/export/x.zip", b"a previous run");
    put(
        repo,
        "ignition/script-python/scratch/resource.json",
        br#"{"files":["s.py"]}"#,
    );
    put(repo, "ignition/script-python/scratch/s.py", b"pass");
    put(repo, ".ignexclude", b"ignition/script-python/scratch/\n");
}

#[tokio::test]
async fn projects_check_reports_excluded() {
    let fx = fixture();
    add_noise(&fx.repo);
    let server = MockServer::start().await;
    mount_list(&server, &[]).await;
    mount_find_missing(&server).await;

    let data = data_of(&run(&server, &check_args(&fx, "dev")));
    assert_eq!(
        data["excluded"],
        json!([
            ".ign/",
            ".ignexclude",
            "deploy/",
            "docs/",
            "ignition/.git/",
            "ignition/script-python/cap/__pycache__/",
            "ignition/script-python/cap/notes.txt",
            "ignition/script-python/scratch/",
            "images/",
            "tags/"
        ])
    );
    assert_eq!(
        data["excluded_count"],
        data["excluded"].as_array().expect("array").len()
    );
    // Only the project.json, the descriptor and the listed script ship.
    assert_eq!(data["members"]["added"], 3);
    assert_only_gets(&methods(&server).await);
    // The run directory of an earlier deploy stays where it was.
    assert!(fx.repo.join(".ign/deploy/gw/run/export/x.zip").exists());
}

#[tokio::test]
async fn projects_check_ignexclude_changes_digest() {
    let fx = fixture();
    add_noise(&fx.repo);
    let server = MockServer::start().await;
    mount_list(&server, &[]).await;
    mount_find_missing(&server).await;

    let excluding = data_of(&run(&server, &check_args(&fx, "dev")));
    // Removing the line puts the scratch package into the artifact.
    std::fs::remove_file(fx.repo.join(".ignexclude")).expect("rm");
    let including = data_of(&run(&server, &check_args(&fx, "dev")));

    assert_ne!(
        excluding["artifact_sha256"], including["artifact_sha256"],
        "the scratch package is part of the artifact once nothing excludes it"
    );
    assert_eq!(including["members"]["added"], 5);
    let listed: Vec<&str> = including["excluded"]
        .as_array()
        .expect("array")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(!listed.contains(&"ignition/script-python/scratch/"));
}

#[tokio::test]
async fn projects_check_bad_ignexclude_refuses_before_any_request() {
    let fx = fixture();
    put(&fx.repo, ".ignexclude", b"docs/\n**/x\n");
    let server = MockServer::start().await;

    let out = run(&server, &check_args(&fx, "dev"));
    assert_eq!(out.status.code(), Some(2));
    let err: Value = serde_json::from_slice(&out.stderr).expect("stderr envelope");
    assert_eq!(err["error"]["code"], "deploy_tree_invalid");
    let message = err["error"]["message"].as_str().expect("message");
    assert!(message.contains(".ignexclude:2:1"), "{message}");
    assert!(methods(&server).await.is_empty());
}

#[tokio::test]
async fn projects_check_human_prints_the_excluded_count() {
    let fx = fixture();
    add_noise(&fx.repo);
    let server = MockServer::start().await;
    mount_list(&server, &[]).await;
    mount_find_missing(&server).await;
    let mut args = check_args(&fx, "dev");
    args.remove(0); // human mode

    let out = run(&server, &args);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    assert!(
        stdout.contains("excluded: 10 path(s) left out of the artifact"),
        "{stdout}"
    );
    assert!(
        stdout.contains("  ignition/script-python/scratch/"),
        "{stdout}"
    );
}

/// A repo file name holding an ESC byte must not reach the terminal raw: the
/// human output escapes it, and the JSON output carries it as a JSON escape.
#[cfg(unix)]
#[tokio::test]
async fn projects_check_human_escapes_control_characters_in_excluded_names() {
    let fx = fixture();
    put(&fx.repo, "tags\u{1b}[2J/t.json", b"{}");
    let server = MockServer::start().await;
    mount_list(&server, &[]).await;
    mount_find_missing(&server).await;

    let mut human = check_args(&fx, "dev");
    human.remove(0);
    let out = run(&server, &human);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    assert!(stdout.contains("  tags\\u{1b}[2J/"), "{stdout:?}");
    assert!(!stdout.contains('\u{1b}'), "no raw ESC in {stdout:?}");

    let data = data_of(&run(&server, &check_args(&fx, "dev")));
    let listed: Vec<&str> = data["excluded"]
        .as_array()
        .expect("array")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(listed.contains(&"tags\u{1b}[2J/"), "{listed:?}");
}

/// The human error line names the refused path; an ESC byte in that name is
/// written out, not sent to the terminal.
#[cfg(unix)]
#[tokio::test]
async fn projects_check_human_error_escapes_control_characters_in_a_path() {
    let fx = fixture();
    let outside = tempfile::tempdir().expect("outside");
    std::fs::write(outside.path().join("secret.txt"), b"secret").expect("write");
    std::os::unix::fs::symlink(
        outside.path().join("secret.txt"),
        fx.repo.join("ignition/script-python/cap/l\u{1b}[2J.py"),
    )
    .expect("symlink");
    let server = MockServer::start().await;
    let mut args = check_args(&fx, "dev");
    args.remove(0); // human mode

    let out = run(&server, &args);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8(out.stderr).expect("utf-8");
    assert!(stderr.contains("l\\u{1b}[2J.py"), "{stderr:?}");
    assert!(!stderr.contains('\u{1b}'), "no raw ESC in {stderr:?}");
    assert!(methods(&server).await.is_empty());
}

fn error_of(out: &std::process::Output) -> Value {
    assert!(out.stdout.is_empty(), "a refusal leaves stdout empty");
    serde_json::from_slice(&out.stderr).expect("stderr envelope")
}

/// DPRJ-04, D-11: a project the git module still manages is refused before
/// the gateway is contacted. Any `.git` entry counts, whatever its kind.
#[tokio::test]
async fn projects_check_git_dir_refuses() {
    let fx = fixture();
    std::fs::create_dir_all(fx.data.join("projects/CapApp/.git")).expect("git dir");
    let server = MockServer::start().await;

    let out = run(&server, &check_args(&fx, "dev"));
    assert_eq!(
        out.status.code(),
        Some(6),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = error_of(&out);
    assert_eq!(err["error"]["code"], "project_git_managed");
    let message = err["error"]["message"].as_str().expect("message");
    for needle in ["CapApp", "projects/CapApp/.git", "directory"] {
        assert!(message.contains(needle), "{needle}: {message}");
    }
    assert!(
        methods(&server).await.is_empty(),
        "the refusal comes before any gateway request"
    );
}

#[tokio::test]
async fn projects_check_git_file_and_symlink_refuse_with_their_kind() {
    for (kind, make) in [("file", false), ("symlink", true)] {
        let fx = fixture();
        let project = fx.data.join("projects/CapApp");
        std::fs::create_dir_all(&project).expect("project dir");
        if make {
            #[cfg(unix)]
            std::os::unix::fs::symlink("/nonexistent", project.join(".git")).expect("symlink");
            #[cfg(not(unix))]
            continue;
        } else {
            std::fs::write(project.join(".git"), b"gitdir: ../x").expect("file");
        }
        let server = MockServer::start().await;
        let out = run(&server, &check_args(&fx, "dev"));
        assert_eq!(out.status.code(), Some(6), "{kind}");
        let err = error_of(&out);
        assert_eq!(err["error"]["code"], "project_git_managed", "{kind}");
        assert!(
            err["error"]["message"]
                .as_str()
                .expect("message")
                .contains(&format!("({kind})")),
            "{err}"
        );
        assert!(methods(&server).await.is_empty(), "{kind}");
    }
}

/// An existing project directory without `.git` proceeds to the gateway read.
#[tokio::test]
async fn projects_check_without_git_proceeds() {
    let fx = fixture();
    std::fs::create_dir_all(fx.data.join("projects/CapApp")).expect("project dir");
    let server = MockServer::start().await;
    mount_list(&server, &["CapApp"]).await;
    mount_find_found(&server).await;
    mount_export(&server, export_zip(CODE_V1, true)).await;
    let data = data_of(&run(&server, &check_args(&fx, "dev")));
    assert_eq!(data["action"], "record", "equal content, no record yet");
    assert_only_gets(&methods(&server).await);
}

/// A `data/projects` that does not exist is an unreadable host, not "no git".
#[tokio::test]
async fn projects_check_missing_projects_dir_is_transport_failed() {
    let fx = fixture();
    std::fs::remove_dir(fx.data.join("projects")).expect("rmdir");
    let server = MockServer::start().await;

    let out = run(&server, &check_args(&fx, "dev"));
    assert_eq!(out.status.code(), Some(4));
    let err = error_of(&out);
    assert_eq!(err["error"]["code"], "transport_failed");
    assert!(
        err["error"]["message"]
            .as_str()
            .expect("message")
            .contains("nodes.master"),
        "{err}"
    );
    assert!(methods(&server).await.is_empty());
}

/// Research F3: a master node that declares neither `ssh` nor `local` cannot
/// be probed, so the verb refuses before any gateway contact.
#[tokio::test]
async fn projects_check_no_host_access_refuses() {
    let fx = fixture();
    put(
        &fx.targets,
        "targets/dev.yaml",
        b"schema_version: 1\ngateway: capgw\nmode: dev\nnodes:\n  master:\n    profile: dev\n",
    );
    let server = MockServer::start().await;

    let out = run(&server, &check_args(&fx, "dev"));
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = error_of(&out);
    assert_eq!(err["error"]["code"], "deploy_tree_invalid");
    assert!(
        err["error"]["message"]
            .as_str()
            .expect("message")
            .contains("set ssh or local; this verb needs host access"),
        "{err}"
    );
    assert!(methods(&server).await.is_empty());
}

/// The human refusal names the entry and what to do about it. The project
/// name in it has passed the `deploy/project.yaml` name check, which refuses
/// control characters, so the line carries no raw terminal escape; the error
/// path escapes them in any case (25-06).
#[tokio::test]
async fn projects_check_human_git_refusal_names_the_entry() {
    let fx = fixture();
    std::fs::create_dir_all(fx.data.join("projects/CapApp/.git")).expect("git dir");
    let server = MockServer::start().await;
    let mut args = check_args(&fx, "dev");
    args.remove(0); // human mode

    let out = run(&server, &args);
    assert_eq!(out.status.code(), Some(6));
    let stderr = String::from_utf8(out.stderr).expect("utf-8");
    assert!(stderr.contains("projects/CapApp/.git"), "{stderr}");
    assert!(stderr.contains("administrator"), "{stderr}");
    assert!(stderr.contains("hint:"), "{stderr}");
    assert!(methods(&server).await.is_empty());
}

/// Write the release record `ign` left for `CapApp` on the fixture's master
/// node (a local node: below the data directory).
fn put_record(fx: &Fixture, release: &str) {
    put_record_sha(fx, release, &"a".repeat(64));
}

/// [`put_record`] naming the artifact digest the last deploy left.
fn put_record_sha(fx: &Fixture, release: &str, artifact_sha256: &str) {
    use ignition_core::deploy::projects::ReleaseRecord;
    let record = ReleaseRecord {
        schema_version: 1,
        gateway: "capgw".to_string(),
        project: "CapApp".to_string(),
        release: release.to_string(),
        artifact_sha256: artifact_sha256.to_string(),
        run_id: "20261020T101500Z-ab12".to_string(),
        deployed_at: "2026-10-20T10:15:00Z".to_string(),
    };
    put(
        &fx.data,
        "ign-deploy/capgw/projects/CapApp/release.json",
        &record.to_bytes(),
    );
}

/// `check_args` with another `--release` and extra flags.
fn args_with(fx: &Fixture, release: &str, extra: &[&str]) -> Vec<String> {
    let mut args = check_args(fx, "dev");
    let at = args.iter().position(|a| a == "--release").expect("flag") + 1;
    args[at] = release.to_string();
    args.extend(extra.iter().map(|s| (*s).to_string()));
    args
}

/// D-03, V3: a release older than the one the gateway last received refuses
/// in the check, before any gateway request.
#[tokio::test]
async fn projects_check_downgrade_refuses() {
    let fx = fixture();
    put_record(&fx, "v1.2.0");
    let server = MockServer::start().await;

    let out = run(&server, &args_with(&fx, "v1.1.0", &[]));
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = error_of(&out);
    assert_eq!(err["error"]["code"], "destructive_change_refused");
    let message = err["error"]["message"].as_str().expect("message");
    for needle in ["downgrade", "CapApp", "--allow-downgrade"] {
        assert!(message.contains(needle), "{needle}: {message}");
    }
    assert!(
        methods(&server).await.is_empty(),
        "the refusal comes before any gateway request"
    );
}

#[tokio::test]
async fn projects_check_downgrade_allowed() {
    let fx = fixture();
    put_record(&fx, "v1.2.0");
    let server = MockServer::start().await;
    mount_list(&server, &["CapApp"]).await;
    mount_find_found(&server).await;
    mount_export(&server, export_zip(CODE_V1, true)).await;

    let data = data_of(&run(
        &server,
        &args_with(&fx, "v1.1.0", &["--allow-downgrade"]),
    ));
    assert_eq!(data["allow_downgrade"], true);
    assert_eq!(data["previous_release"], "v1.2.0");
    assert_eq!(data["release"], "v1.1.0");
    assert_only_gets(&methods(&server).await);
}

#[tokio::test]
async fn projects_check_equal_release_allowed() {
    let fx = fixture();
    put_record(&fx, "v1.2.0");
    let server = MockServer::start().await;
    mount_list(&server, &["CapApp"]).await;
    mount_find_found(&server).await;
    mount_export(&server, export_zip(CODE_V1, true)).await;

    let data = data_of(&run(&server, &args_with(&fx, "v1.2.0", &[])));
    assert_eq!(data["allow_downgrade"], false);
    assert_eq!(data["previous_release"], "v1.2.0");
}

#[tokio::test]
async fn projects_check_newer_release_reports_the_previous_one() {
    let fx = fixture();
    put_record(&fx, "v1.2.0-rc.1");
    let server = MockServer::start().await;
    mount_list(&server, &["CapApp"]).await;
    mount_find_found(&server).await;
    mount_export(&server, export_zip(CODE_V1, true)).await;

    let data = data_of(&run(&server, &args_with(&fx, "v1.2.0", &[])));
    assert_eq!(data["previous_release"], "v1.2.0-rc.1");
}

/// V4: the tag is validated before any host or gateway contact.
#[tokio::test]
async fn projects_check_bad_release_refuses() {
    for bad in ["banana", "1.0", "vv1.0.0", ""] {
        let fx = fixture();
        // A host that would refuse if it were reached.
        std::fs::remove_dir(fx.data.join("projects")).expect("rmdir");
        let server = MockServer::start().await;
        let out = run(&server, &args_with(&fx, bad, &[]));
        assert_eq!(out.status.code(), Some(2), "{bad:?}");
        let err = error_of(&out);
        assert_eq!(err["error"]["code"], "invalid_input", "{bad:?}");
        assert!(
            err["error"]["message"]
                .as_str()
                .expect("message")
                .contains("--release"),
            "{err}"
        );
        assert!(methods(&server).await.is_empty(), "{bad:?}");
    }
}

/// A record that cannot be trusted refuses; it is never read as absent.
#[tokio::test]
async fn projects_check_corrupt_record_refuses() {
    let fx = fixture();
    put(
        &fx.data,
        "ign-deploy/capgw/projects/CapApp/release.json",
        b"{\"schema_version\":1}",
    );
    let server = MockServer::start().await;
    let out = run(&server, &args_with(&fx, "v1.1.0", &["--allow-downgrade"]));
    assert_eq!(out.status.code(), Some(6));
    let err = error_of(&out);
    assert_eq!(err["error"]["code"], "deploy_run_invalid");
    assert!(
        err["error"]["message"]
            .as_str()
            .expect("message")
            .contains("ign-deploy/capgw/projects/CapApp/release.json"),
        "{err}"
    );
    assert!(methods(&server).await.is_empty());
}

#[tokio::test]
async fn projects_check_overlong_first_change_reason_refuses() {
    let fx = fixture();
    let server = MockServer::start().await;
    let long = "x".repeat(501);
    for reason in [long.as_str(), ""] {
        let out = run(
            &server,
            &args_with(&fx, "v1.0.0", &["--allow-first-change", reason]),
        );
        assert_eq!(out.status.code(), Some(2), "{} chars", reason.len());
        assert_eq!(error_of(&out)["error"]["code"], "invalid_input");
    }
    assert!(methods(&server).await.is_empty());
}

/// The human check says what the apply would do for the record-only case, and
/// prints the drift warning.
#[tokio::test]
async fn projects_check_human_record_and_drift_lines() {
    let fx = fixture();
    let server = MockServer::start().await;
    mount_list(&server, &["CapApp"]).await;
    mount_find_found(&server).await;
    mount_export(&server, export_zip(CODE_V1, true)).await;
    let mut args = check_args(&fx, "dev");
    args.remove(0); // human mode

    let out = run(&server, &args);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    assert!(
        stdout.contains("check: record the release only (no project change)"),
        "{stdout}"
    );
    assert!(
        stdout.contains("release: v1.0.0 (last deployed by this tool: none)"),
        "{stdout}"
    );

    put_record(&fx, "v0.9.0");
    let out = run(&server, &args);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    assert!(
        stdout.contains("release: v1.0.0 (last deployed by this tool: v0.9.0)"),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "warning: project CapApp: the live project no longer matches the last release this tool deployed (v0.9.0)"
        ),
        "{stdout}"
    );
}

/// The repo's `project.json` with a `parent`.
fn set_repo_parent(fx: &Fixture, parent: &str) {
    put(
        &fx.repo,
        "project.json",
        format!(r#"{{"title":"Cap App","enabled":true,"parent":"{parent}"}}"#).as_bytes(),
    );
}

/// `projects/list` with full records (parent, inheritable, invalidParent).
async fn mount_list_records(server: &MockServer, items: Vec<Value>) {
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

/// K6 a, D-05: a parent the gateway does not list refuses
/// `project_parent_invalid` with reason `missing`, before any export and with
/// nothing but GET requests sent.
#[tokio::test]
async fn projects_check_parent_missing_refuses() {
    let fx = fixture();
    set_repo_parent(&fx, "WHK-Global");
    let server = MockServer::start().await;
    mount_list(&server, &["CapApp"]).await;
    mount_find_found(&server).await;
    mount_export(&server, export_zip(CODE_V1, true)).await;

    let out = run(&server, &check_args(&fx, "dev"));
    assert_eq!(
        out.status.code(),
        Some(6),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = error_of(&out);
    assert_eq!(err["error"]["code"], "project_parent_invalid");
    let message = err["error"]["message"].as_str().expect("message");
    for needle in ["CapApp", "WHK-Global", "missing"] {
        assert!(message.contains(needle), "{needle}: {message}");
    }
    let requests = methods(&server).await;
    assert!(
        requests.iter().all(|(_, url)| !url.contains("/export/")),
        "no project export before the refusal: {requests:?}"
    );
    assert_only_gets(&requests);
}

/// A present, inheritable parent passes and is reported.
#[tokio::test]
async fn projects_check_parent_present_passes() {
    let fx = fixture();
    set_repo_parent(&fx, "WHK-Global");
    let server = MockServer::start().await;
    mount_list_records(
        &server,
        vec![
            json!({"name": "WHK-Global", "enabled": true, "inheritable": true, "invalidParent": false}),
            json!({"name": "CapApp", "enabled": true}),
        ],
    )
    .await;
    mount_find_found(&server).await;
    mount_export(&server, export_zip(CODE_V1, true)).await;

    let out = run(
        &server,
        &args_with(&fx, "v1.0.0", &["--allow-first-change", "adopt the repo"]),
    );
    let data = data_of(&out);
    assert_eq!(data["parent"], "WHK-Global");
    assert_eq!(data["action"], "import");
    assert_only_gets(&methods(&server).await);
}

// ---------------------------------------------------------------------------
// The guarded apply (25-11): DPRJ-01, DPRJ-03.
// ---------------------------------------------------------------------------

/// The canonical digest of the artifact the fixture repo builds: what
/// `--check --json` reports as `artifact_sha256`.
fn artifact_hash(fx: &Fixture) -> String {
    ignition_core::deploy::project_artifact::build_artifact(&fx.repo)
        .expect("the fixture builds")
        .digest
        .sha256
}

/// Make the fixture repo a git work tree that ignores `.ign/`, as the run
/// directory's guard requires.
fn git_ignore_runs(fx: &Fixture) {
    let status = std::process::Command::new("git")
        .args(["init", "-q"])
        .arg(&fx.repo)
        .status()
        .expect("git runs");
    assert!(status.success());
    put(&fx.repo, ".gitignore", b".ign/\n");
}

/// `check_args` as a deploy: no `--check`, the reviewed hash, `--yes`.
fn apply_args(fx: &Fixture, release: &str, hash: Option<&str>, yes: bool) -> Vec<String> {
    let mut args = args_with(fx, release, &[]);
    args.retain(|a| a != "--check");
    if let Some(hash) = hash {
        args.push("--artifact-sha256".into());
        args.push(hash.into());
    }
    if yes {
        args.push("--yes".into());
    }
    args
}

/// The export answers `before` until the import was received and `after`
/// from then on: a gateway that holds the old project until it is replaced.
struct ExportAfterImport {
    imported: Arc<AtomicBool>,
    before: Vec<u8>,
    after: Vec<u8>,
}

impl Respond for ExportAfterImport {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let zip = if self.imported.load(Ordering::SeqCst) {
            self.after.clone()
        } else {
            self.before.clone()
        };
        ResponseTemplate::new(200)
            .set_body_raw(zip, "application/zip")
            .insert_header(
                "Content-Disposition",
                "attachment; filename=\"CapApp-export.zip\"",
            )
    }
}

/// The import answer of the real gateway (C8c); receiving it flips the
/// export.
struct ImportAnswer {
    imported: Arc<AtomicBool>,
}

impl Respond for ImportAnswer {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        self.imported.store(true, Ordering::SeqCst);
        ResponseTemplate::new(200)
            .set_body_json(json!({"success": true, "changes": [{"name": "CapApp"}]}))
    }
}

/// A gateway that holds `before` (or no project) until `import?overwrite=` is
/// posted, then holds `after`. `list` names the project when it exists.
async fn mount_deployable(server: &MockServer, before: Option<Vec<u8>>, after: Vec<u8>) {
    let imported = Arc::new(AtomicBool::new(false));
    let overwrite = before.is_some();
    match &before {
        Some(_) => {
            mount_list(server, &["CapApp"]).await;
            mount_find_found(server).await;
        }
        None => {
            mount_list(server, &[]).await;
            mount_find_missing(server).await;
        }
    }
    Mock::given(method("GET"))
        .and(path("/data/api/v1/projects/export/CapApp"))
        .respond_with(ExportAfterImport {
            imported: imported.clone(),
            before: before.unwrap_or_else(|| after.clone()),
            after,
        })
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/data/api/v1/projects/import/CapApp"))
        .and(query_param(
            "overwrite",
            if overwrite { "true" } else { "false" },
        ))
        .respond_with(ImportAnswer { imported })
        .mount(server)
        .await;
}

fn run_dirs(fx: &Fixture) -> Vec<PathBuf> {
    let root = fx.repo.join(".ign/deploy/capgw");
    match std::fs::read_dir(&root) {
        Ok(entries) => entries.map(|e| e.expect("entry").path()).collect(),
        Err(_) => Vec::new(),
    }
}

fn position(requests: &[(String, String)], verb: &str, needle: &str) -> Vec<usize> {
    requests
        .iter()
        .enumerate()
        .filter(|(_, (v, p))| v == verb && p.contains(needle))
        .map(|(i, _)| i)
        .collect()
}

/// DPRJ-03, K15, V5: the live project is saved before the import, the import
/// is verified by canonical hash and the release is recorded.
#[tokio::test]
async fn projects_apply_imports_and_records() {
    let fx = fixture();
    git_ignore_runs(&fx);
    put_record(&fx, "v1.0.0");
    let hash = artifact_hash(&fx);
    let server = MockServer::start().await;
    let old = export_zip(CODE_V2, true);
    mount_deployable(&server, Some(old.clone()), export_zip(CODE_V1, true)).await;

    let out = run(&server, &apply_args(&fx, "v1.1.0", Some(&hash), true));
    let data = data_of(&out);
    assert_eq!(data["outcome"], "imported");
    assert_eq!(data["ok"], true);
    assert_eq!(data["check"], false);
    assert_eq!(data["record_written"], true);
    assert_eq!(data["artifact_sha256"], hash.as_str());
    let run_id = data["run_id"].as_str().expect("run id").to_string();
    assert_eq!(data["run_dir"], format!(".ign/deploy/capgw/{run_id}"));

    // The run directory holds the audit files and the pre-image.
    let dir = fx.repo.join(".ign/deploy/capgw").join(&run_id);
    for file in ["run.json", "plan.json", "events.ndjson", "result.json"] {
        assert!(dir.join(file).is_file(), "{file}");
    }
    let saved = dir.join("export/CapApp.zip");
    assert_eq!(std::fs::read(&saved).expect("pre-image"), old);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode(&saved), 0o600);
        assert_eq!(mode(&dir.join("export")), 0o700);
        assert_eq!(mode(&dir), 0o700);
    }
    let run_json: Value =
        serde_json::from_slice(&std::fs::read(dir.join("run.json")).expect("run.json"))
            .expect("run.json is json");
    assert_eq!(run_json["release"], "v1.1.0");
    assert_eq!(run_json["allow_downgrade"], false);
    assert_eq!(run_json["artifact_sha256"], hash.as_str());

    // The record names this release, this artifact and this run.
    use ignition_core::deploy::projects::ReleaseRecord;
    let bytes = std::fs::read(
        fx.data
            .join("ign-deploy/capgw/projects/CapApp/release.json"),
    )
    .expect("record");
    let record = ReleaseRecord::parse(&bytes, "release.json", "capgw", "CapApp").expect("parses");
    assert_eq!(record.release, "v1.1.0");
    assert_eq!(record.artifact_sha256, hash);
    assert_eq!(record.run_id, run_id);

    // Export before import, import once, the read-back after it.
    let requests = methods(&server).await;
    let exports = position(&requests, "GET", "/export/");
    let imports = position(&requests, "POST", "/import/");
    assert_eq!(imports.len(), 1, "{requests:?}");
    assert!(
        exports.iter().filter(|i| **i < imports[0]).count() >= 2,
        "plan and pre-image exports precede the import: {requests:?}"
    );
    assert!(
        exports.iter().any(|i| *i > imports[0]),
        "the project is read back: {requests:?}"
    );
    assert_eq!(requests.iter().filter(|(v, _)| v == "POST").count(), 1);
}

/// D-13, K10: an absent project is created with `overwrite=false`, no
/// pre-image, through the same guard and hash gate.
#[tokio::test]
async fn projects_apply_creates_absent() {
    let fx = fixture();
    git_ignore_runs(&fx);
    let hash = artifact_hash(&fx);
    let server = MockServer::start().await;
    mount_deployable(&server, None, export_zip(CODE_V1, true)).await;

    let out = run(&server, &apply_args(&fx, "v1.0.0", Some(&hash), true));
    let data = data_of(&out);
    assert_eq!(data["outcome"], "created");
    assert_eq!(data["ok"], true);
    assert_eq!(data["record_written"], true);
    let dirs = run_dirs(&fx);
    assert_eq!(dirs.len(), 1);
    assert!(
        !dirs[0].join("export").exists(),
        "an absent project has no pre-image"
    );
    assert!(
        fx.data
            .join("ign-deploy/capgw/projects/CapApp/release.json")
            .is_file()
    );
    let requests = methods(&server).await;
    let imports = position(&requests, "POST", "/import/");
    assert_eq!(imports.len(), 1, "{requests:?}");
    assert!(
        position(&requests, "GET", "/export/")
            .iter()
            .all(|i| *i > imports[0]),
        "no export before the create: {requests:?}"
    );
}

/// SC-1, D-01: a hash that is not the artifact's is refused with nothing
/// contacted, not the gateway and not the host.
#[tokio::test]
async fn projects_apply_hash_mismatch_uploads_nothing() {
    let fx = fixture();
    git_ignore_runs(&fx);
    let server = MockServer::start().await;
    mount_deployable(
        &server,
        Some(export_zip(CODE_V2, true)),
        export_zip(CODE_V1, true),
    )
    .await;

    let zeros = "0".repeat(64);
    let out = run(&server, &apply_args(&fx, "v1.0.0", Some(&zeros), true));
    assert_eq!(
        out.status.code(),
        Some(6),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = error_of(&out);
    assert_eq!(err["error"]["code"], "project_hash_mismatch");
    let message = err["error"]["message"].as_str().expect("message");
    for needle in ["CapApp", zeros.as_str(), artifact_hash(&fx).as_str()] {
        assert!(message.contains(needle), "{needle}: {message}");
    }
    assert!(
        methods(&server).await.is_empty(),
        "nothing is requested before the hash passes"
    );
    assert!(run_dirs(&fx).is_empty());
    assert!(!fx.repo.join(".ign").exists());
}

/// Decision C: an unattended apply names the artifact it reviewed.
#[tokio::test]
async fn projects_apply_needs_hash_noninteractive() {
    let fx = fixture();
    git_ignore_runs(&fx);
    let server = MockServer::start().await;

    let out = run(&server, &apply_args(&fx, "v1.0.0", None, true));
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = error_of(&out);
    assert_eq!(err["error"]["code"], "invalid_input");
    let message = err["error"]["message"].as_str().expect("message");
    assert!(message.contains("--artifact-sha256"), "{message}");
    assert!(
        !err["error"]["hint"]
            .as_str()
            .unwrap_or_default()
            .contains("--file"),
        "a flag refusal does not point at --file"
    );
    assert!(methods(&server).await.is_empty());
    assert!(!fx.repo.join(".ign").exists());
}

/// The hash is 64 lowercase hex characters, or the input is refused before
/// anything is contacted.
#[tokio::test]
async fn projects_apply_bad_hash_format() {
    let fx = fixture();
    git_ignore_runs(&fx);
    let server = MockServer::start().await;
    let upper = artifact_hash(&fx).to_uppercase();

    for bad in [upper.as_str(), "abc", &"g".repeat(64)] {
        let out = run(&server, &apply_args(&fx, "v1.0.0", Some(bad), true));
        assert_eq!(
            out.status.code(),
            Some(2),
            "{bad}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(error_of(&out)["error"]["code"], "invalid_input", "{bad}");
    }
    assert!(methods(&server).await.is_empty());
}

/// The guard: without `--yes` the plan is the refusal, and nothing but reads
/// reached the gateway.
#[tokio::test]
async fn projects_apply_needs_yes() {
    let fx = fixture();
    git_ignore_runs(&fx);
    put_record(&fx, "v1.0.0");
    let hash = artifact_hash(&fx);
    let server = MockServer::start().await;
    mount_deployable(
        &server,
        Some(export_zip(CODE_V2, true)),
        export_zip(CODE_V1, true),
    )
    .await;

    let out = run(&server, &apply_args(&fx, "v1.1.0", Some(&hash), false));
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = error_of(&out);
    assert_eq!(err["error"]["code"], "confirmation_required");
    let message = err["error"]["message"].as_str().expect("message");
    for needle in ["CapApp", "import", hash.as_str(), "capgw", "v1.1.0"] {
        assert!(message.contains(needle), "{needle}: {message}");
    }
    assert_only_gets(&methods(&server).await);
    assert!(run_dirs(&fx).is_empty());
    assert!(!fx.repo.join(".ign").exists());
}

/// K14: this release is already recorded and the project is the artifact:
/// nothing is written, not even a run directory.
#[tokio::test]
async fn projects_apply_skip_writes_nothing() {
    let fx = fixture();
    git_ignore_runs(&fx);
    let hash = artifact_hash(&fx);
    put_record_sha(&fx, "v1.1.0", &hash);
    let server = MockServer::start().await;
    mount_deployable(
        &server,
        Some(export_zip(CODE_V1, true)),
        export_zip(CODE_V1, true),
    )
    .await;

    let out = run(&server, &apply_args(&fx, "v1.1.0", Some(&hash), true));
    let data = data_of(&out);
    assert_eq!(data["outcome"], "skipped");
    assert_eq!(data["ok"], true);
    assert!(data["run_id"].is_null());
    assert_eq!(data["record_written"], false);
    assert!(!fx.repo.join(".ign").exists());
    assert_only_gets(&methods(&server).await);
}

/// K14, D-12: the project already is the artifact and no record names this
/// release: only the record is written.
#[tokio::test]
async fn projects_apply_record_only() {
    let fx = fixture();
    git_ignore_runs(&fx);
    let hash = artifact_hash(&fx);
    let server = MockServer::start().await;
    mount_deployable(
        &server,
        Some(export_zip(CODE_V1, true)),
        export_zip(CODE_V1, true),
    )
    .await;

    let out = run(&server, &apply_args(&fx, "v1.1.0", Some(&hash), true));
    let data = data_of(&out);
    assert_eq!(data["outcome"], "recorded");
    assert_eq!(data["ok"], true);
    assert_eq!(data["record_written"], true);
    assert!(data["run_id"].is_string());
    assert!(
        fx.data
            .join("ign-deploy/capgw/projects/CapApp/release.json")
            .is_file()
    );
    assert_only_gets(&methods(&server).await);
    let dirs = run_dirs(&fx);
    assert_eq!(dirs.len(), 1);
    assert!(
        !dirs[0].join("export").exists(),
        "the project is not replaced, so nothing is saved"
    );
}

// ---------------------------------------------------------------------------
// The unverified verdict (25-12): DPRJ-03, V6, K11.
// ---------------------------------------------------------------------------

/// A gateway that replaces `CapApp` with something other than the artifact
/// (the read-back after the import differs), so the verify cannot pass.
async fn mount_unverifiable(server: &MockServer) {
    mount_deployable(
        server,
        Some(export_zip(CODE_V2, true)),
        export_zip(b"def f():\n    return 3\n", true),
    )
    .await;
}

fn record_path(fx: &Fixture) -> PathBuf {
    fx.data
        .join("ign-deploy/capgw/projects/CapApp/release.json")
}

/// V6, K11, T-25-64, T-25-66: a read-back that differs from the artifact is a
/// success envelope that says so, then exit 6. The live project stays as
/// imported, nothing is restored, no second import is sent and the release
/// record is not replaced.
#[tokio::test]
async fn projects_apply_unverified_exit_6() {
    let fx = fixture();
    git_ignore_runs(&fx);
    put_record(&fx, "v1.0.0");
    let record_before = std::fs::read(record_path(&fx)).expect("record");
    let hash = artifact_hash(&fx);
    let server = MockServer::start().await;
    mount_unverifiable(&server).await;

    let out = run(&server, &apply_args(&fx, "v1.1.0", Some(&hash), true));
    assert_eq!(
        out.status.code(),
        Some(6),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stderr.is_empty(),
        "under --json stderr is empty: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("one envelope");
    assert_eq!(
        envelope["ok"], true,
        "a success envelope, then the exit code"
    );
    let data = &envelope["data"];
    assert_eq!(data["outcome"], "imported_unverified");
    assert_eq!(data["ok"], false);
    assert_eq!(data["slug"], "deploy_project_unverified");
    assert_eq!(data["record_written"], false);
    assert_eq!(data["artifact_sha256"], hash.as_str());
    let run_id = data["run_id"].as_str().expect("run id");
    let run_dir = format!(".ign/deploy/capgw/{run_id}");
    assert_eq!(data["run_dir"], run_dir.as_str());
    assert_eq!(
        data["restore_command"],
        format!(
            "ign project import CapApp --file {run_dir}/export/CapApp.zip --collision-policy overwrite --yes"
        )
        .as_str()
    );

    // The file the command names is the saved pre-image.
    let saved = fx.repo.join(&run_dir).join("export/CapApp.zip");
    assert_eq!(
        std::fs::read(&saved).expect("pre-image"),
        export_zip(CODE_V2, true)
    );

    // result.json holds the same verdict.
    let result: Value = serde_json::from_slice(
        &std::fs::read(fx.repo.join(&run_dir).join("result.json")).expect("result.json"),
    )
    .expect("json");
    assert_eq!(result["outcome"], "imported_unverified");
    assert_eq!(result["ok"], false);
    assert_eq!(result["slug"], "deploy_project_unverified");
    assert_eq!(result["restore_command"], data["restore_command"]);

    // One import, no restore, and the earlier record is untouched.
    let requests = methods(&server).await;
    assert_eq!(
        position(&requests, "POST", "/import/").len(),
        1,
        "no second, unreviewed import: {requests:?}"
    );
    assert_eq!(requests.iter().filter(|(v, _)| v == "POST").count(), 1);
    assert_eq!(
        std::fs::read(record_path(&fx)).expect("record"),
        record_before,
        "no release record is written for an unverified import"
    );
}

/// A project that was created has no pre-image: the verdict still stands and
/// there is no restore command to print.
#[tokio::test]
async fn projects_apply_unverified_create_has_no_restore_command() {
    let fx = fixture();
    git_ignore_runs(&fx);
    let hash = artifact_hash(&fx);
    let server = MockServer::start().await;
    mount_deployable(&server, None, export_zip(b"def f():\n    return 3\n", true)).await;

    let out = run(&server, &apply_args(&fx, "v1.0.0", Some(&hash), true));
    assert_eq!(out.status.code(), Some(6));
    let envelope: Value = serde_json::from_slice(&out.stdout).expect("envelope");
    let data = &envelope["data"];
    assert_eq!(data["outcome"], "imported_unverified");
    assert_eq!(data["ok"], false);
    assert_eq!(data["slug"], "deploy_project_unverified");
    assert!(data["restore_command"].is_null(), "{data}");
    assert!(
        !record_path(&fx).exists(),
        "no record for an unverified create"
    );
}

/// Human mode: the outcome, the hashes, the run directory and the restore
/// command on stdout; one `ign deploy projects:` line per step on stderr.
#[tokio::test]
async fn projects_apply_human_output() {
    let fx = fixture();
    git_ignore_runs(&fx);
    put_record(&fx, "v1.0.0");
    let hash = artifact_hash(&fx);
    let server = MockServer::start().await;
    mount_unverifiable(&server).await;
    let mut args = apply_args(&fx, "v1.1.0", Some(&hash), true);
    args.remove(0); // human mode

    let out = run(&server, &args);
    assert_eq!(out.status.code(), Some(6));
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    let stderr = String::from_utf8(out.stderr).expect("utf-8");

    assert!(
        stdout.contains("project CapApp on capgw (env dev, release v1.1.0)"),
        "{stdout}"
    );
    assert!(
        stdout.contains("outcome: imported_unverified (NOT verified)"),
        "{stdout}"
    );
    assert!(
        stdout.contains("verdict: deploy_project_unverified"),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("artifact sha256 {hash}")),
        "{stdout}"
    );
    assert!(stdout.contains("live sha256     "), "{stdout}");
    assert!(
        stdout.contains("run directory .ign/deploy/capgw/"),
        "{stdout}"
    );
    assert!(
        stdout.contains("ign project import CapApp --file .ign/deploy/capgw/"),
        "{stdout}"
    );
    assert!(
        stdout.contains("/export/CapApp.zip --collision-policy overwrite --yes"),
        "{stdout}"
    );
    assert!(stdout.contains("release record: not written"), "{stdout}");

    // Progress: pre-image, import and verify, each on a prefixed line.
    let lines: Vec<&str> = stderr.lines().collect();
    for needle in ["pre-image saved", "import sent", "verify NOT green"] {
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("ign deploy projects: ") && l.contains(needle)),
            "{needle}: {stderr}"
        );
    }
    // Names and hashes only: the progress never prints file content.
    assert!(!stderr.contains("return 3"), "{stderr}");
    assert!(!stderr.contains("return 2"), "{stderr}");
}

/// A deploy that verifies is human-quiet about verdicts: no slug, no restore
/// line, exit 0.
#[tokio::test]
async fn projects_apply_verified_human_has_no_verdict_lines() {
    let fx = fixture();
    git_ignore_runs(&fx);
    put_record(&fx, "v1.0.0");
    let hash = artifact_hash(&fx);
    let server = MockServer::start().await;
    mount_deployable(
        &server,
        Some(export_zip(CODE_V2, true)),
        export_zip(CODE_V1, true),
    )
    .await;
    let mut args = apply_args(&fx, "v1.1.0", Some(&hash), true);
    args.remove(0);

    let out = run(&server, &args);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    assert!(stdout.contains("outcome: imported\n"), "{stdout}");
    assert!(!stdout.contains("verdict:"), "{stdout}");
    assert!(!stdout.contains("restore"), "{stdout}");
    let stderr = String::from_utf8(out.stderr).expect("utf-8");
    assert!(
        stderr.contains("ign deploy projects: verify green"),
        "{stderr}"
    );
}
