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
    assert_eq!(data["action"], "skip");
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
    use ignition_core::deploy::projects::ReleaseRecord;
    let record = ReleaseRecord {
        schema_version: 1,
        gateway: "capgw".to_string(),
        project: "CapApp".to_string(),
        release: release.to_string(),
        artifact_sha256: "a".repeat(64),
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
