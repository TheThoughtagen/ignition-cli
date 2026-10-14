//! The ssh process layer (22-05): `RemoteShell` argv, host-key pin, key
//! handling and timeouts.
//!
//! `ssh_g_reports_pinned_options` runs the real `ssh -G` from PATH on the
//! generated argv; it fails with a clear message when `ssh` is absent and is
//! never skipped. `windows_key_acl` runs on the windows CI leg only.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ignition_core::deploy::target::{Ssh, TargetOs};
use ignition_core::deploy::transport::process::{ProcessOutput, ProcessRunner, TokioProcessRunner};
use ignition_core::deploy::transport::remote::{RemoteShell, hardened_options};
use ignition_core::error::CoreError;

const KEY_CANARY: &str = "KEYCANARY-b3a1c0de-do-not-leak";
const KNOWN_HOSTS: &str = "gw.example.test ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPinnedPinnedPinnedPinnedPinnedPinnedPinned\n";

fn ssh_block() -> Ssh {
    Ssh {
        host: "gw.example.test".to_string(),
        port: Some(2222),
        user: "svc".to_string(),
        key_file: None,
        key_env: None,
        os: TargetOs::Linux,
        data_dir: "/usr/local/bin/ignition/data".to_string(),
        known_hosts: "ssh/prod.known_hosts".to_string(),
        connect_timeout_s: None,
        op_timeout_s: None,
    }
}

fn repo_with_known_hosts(content: &str) -> tempfile::TempDir {
    let repo = tempfile::tempdir().expect("repo");
    std::fs::create_dir_all(repo.path().join("ssh")).expect("ssh dir");
    std::fs::write(repo.path().join("ssh/prod.known_hosts"), content).expect("known_hosts");
    repo
}

fn runner() -> Arc<dyn ProcessRunner> {
    Arc::new(TokioProcessRunner)
}

fn shell(repo: &Path, ssh: &Ssh, env: &dyn Fn(&str) -> Option<String>) -> RemoteShell {
    RemoteShell::from_ssh_with_env(
        repo,
        "targets/prod.yaml",
        "nodes.master",
        ssh,
        runner(),
        env,
    )
    .expect("shell")
}

fn no_env(_: &str) -> Option<String> {
    None
}

fn argv_strings(argv: &[OsString]) -> Vec<String> {
    argv.iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect()
}

/// `ssh -G` over `argv` (the part before any remote command): the effective
/// configuration as lowercase-key lines.
fn ssh_g(argv: &[OsString]) -> Vec<String> {
    let mut full = vec![OsString::from("-G")];
    full.extend(argv.iter().cloned());
    let out = Command::new("ssh")
        .args(&full)
        .output()
        .expect("`ssh` must be on PATH for the argv contract test (it is never skipped)");
    assert!(
        out.status.success(),
        "ssh -G failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

fn assert_reports(config: &[String], expected: &[&str]) {
    for want in expected {
        assert!(
            config.iter().any(|line| line == want),
            "ssh -G did not report `{want}`; got:\n{}",
            config.join("\n")
        );
    }
}

#[test]
fn ssh_g_reports_pinned_options() {
    let repo = repo_with_known_hosts(KNOWN_HOSTS);
    let shell = shell(repo.path(), &ssh_block(), &no_env);
    let config = ssh_g(&shell.ssh_argv(None));
    assert_reports(
        &config,
        &[
            "batchmode yes",
            "stricthostkeychecking true",
            "updatehostkeys false",
            "verifyhostkeydns false",
            "preferredauthentications publickey",
            "passwordauthentication no",
            "kbdinteractiveauthentication no",
            "numberofpasswordprompts 0",
            "forwardagent no",
            "forwardx11 no",
            "clearallforwardings yes",
            "permitlocalcommand no",
            "controlmaster false",
            "connecttimeout 10",
            "serveraliveinterval 5",
            "serveralivecountmax 3",
            "loglevel ERROR",
            "port 2222",
            "user svc",
            "hostname gw.example.test",
            "globalknownhostsfile /dev/null",
        ],
    );
    // `ControlPath=none` is reported as no `controlpath` line at all by
    // OpenSSH 10.3 (the value is unset); any other value would mean the
    // runner's own config re-enabled a mux socket.
    for line in &config {
        assert!(
            !line.starts_with("controlpath ") || line == "controlpath none",
            "a control socket is in play: {line}"
        );
    }
    let known_hosts = shell.known_hosts_copy().display().to_string();
    assert_reports(&config, &[&format!("userknownhostsfile {known_hosts}")]);
    for line in &config {
        if let Some(files) = line.strip_prefix("userknownhostsfile ") {
            assert!(
                !files.contains(".ssh/known_hosts"),
                "the runner's own known_hosts is in play: {line}"
            );
        }
    }
    assert!(
        !config.iter().any(|l| l.starts_with("identitiesonly yes")),
        "no identity configured, IdentitiesOnly must not be forced"
    );
    assert!(
        known_hosts.contains("ign-ssh-"),
        "known_hosts copy lives in the shell's temp dir: {known_hosts}"
    );
}

#[test]
fn ssh_g_reports_identity_and_configured_timeouts() {
    let repo = repo_with_known_hosts(KNOWN_HOSTS);
    let mut ssh = ssh_block();
    ssh.key_env = Some("PROD_KEY".to_string());
    ssh.connect_timeout_s = Some(37);
    let shell = shell(repo.path(), &ssh, &|name| {
        (name == "PROD_KEY").then(|| "-----BEGIN X-----\nabc\n-----END X-----".to_string())
    });
    let config = ssh_g(&shell.ssh_argv(None));
    let identity = shell.identity().expect("identity").display().to_string();
    assert_reports(
        &config,
        &[
            "identitiesonly yes",
            &format!("identityfile {identity}"),
            "connecttimeout 37",
        ],
    );
}

#[test]
fn ssh_g_keeps_odd_paths_literal() {
    let dir = tempfile::Builder::new()
        .prefix("ign odd %h ")
        .tempdir()
        .expect("odd dir");
    let known_hosts = dir.path().join("known hosts");
    let options = hardened_options(&known_hosts, None, 10).expect("options");
    let mut argv = vec![OsString::from("-T")];
    for (k, v) in &options {
        argv.push(OsString::from("-o"));
        argv.push(OsString::from(format!("{k}={v}")));
    }
    argv.push(OsString::from("--"));
    argv.push(OsString::from("svc@gw.example.test"));
    let config = ssh_g(&argv);
    assert_reports(
        &config,
        &[&format!("userknownhostsfile {}", known_hosts.display())],
    );
}

#[test]
fn sftp_argv_shape() {
    let repo = repo_with_known_hosts(KNOWN_HOSTS);
    let shell = shell(repo.path(), &ssh_block(), &no_env);
    let sftp = argv_strings(&shell.sftp_argv());
    assert_eq!(&sftp[..5], ["-q", "-b", "-", "-P", "2222"]);
    let ssh = argv_strings(&shell.ssh_argv(None));
    assert_eq!(&ssh[..3], ["-T", "-p", "2222"]);
    // the identical -o set, in the same order
    let options = |argv: &[String]| -> Vec<String> {
        argv.windows(2)
            .filter(|w| w[0] == "-o")
            .map(|w| w[1].clone())
            .collect()
    };
    assert_eq!(options(&sftp), options(&ssh));
    assert!(options(&sftp).len() >= 20);
    // `--` then user@host closes both argv
    assert_eq!(&sftp[sftp.len() - 2..], ["--", "svc@gw.example.test"]);
    assert_eq!(&ssh[ssh.len() - 2..], ["--", "svc@gw.example.test"]);
    // a remote command comes after the destination, never before `--`
    let with_command = argv_strings(&shell.ssh_argv(Some("uname -a")));
    assert_eq!(
        &with_command[with_command.len() - 3..],
        ["--", "svc@gw.example.test", "uname -a"]
    );
    // default port
    let mut ssh_default = ssh_block();
    ssh_default.port = None;
    let default_shell = self::shell(repo.path(), &ssh_default, &no_env);
    assert_eq!(argv_strings(&default_shell.ssh_argv(None))[2], "22");
}

#[cfg(unix)]
fn generated_key() -> String {
    let dir = tempfile::tempdir().expect("keydir");
    let path = dir.path().join("id");
    let status = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(&path)
        .status()
        .expect("`ssh-keygen` must be on PATH");
    assert!(status.success());
    std::fs::read_to_string(&path).expect("key text")
}

#[cfg(unix)]
#[test]
fn key_env_materialised() {
    use std::os::unix::fs::PermissionsExt;
    let original = generated_key();
    // a CI secret: CRLF line endings and no trailing newline
    let mangled = original.trim_end().replace('\n', "\r\n");
    assert!(mangled.contains('\r') && !mangled.ends_with('\n'));

    let repo = repo_with_known_hosts(KNOWN_HOSTS);
    let mut ssh = ssh_block();
    ssh.key_env = Some("PROD_KEY".to_string());
    let shell = shell(repo.path(), &ssh, &|name| {
        (name == "PROD_KEY").then(|| mangled.clone())
    });
    let key = shell.identity().expect("identity").to_path_buf();
    let dir = shell.temp_dir().expect("temp dir").to_path_buf();

    let written = std::fs::read_to_string(&key).expect("key file");
    assert!(!written.contains('\r'));
    assert!(written.ends_with('\n') && !written.ends_with("\n\n"));
    assert_eq!(written, original);
    assert_eq!(
        std::fs::metadata(&key)
            .expect("key meta")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(&dir)
            .expect("dir meta")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let kh = shell.known_hosts_copy();
    assert_eq!(std::fs::read_to_string(kh).expect("copy"), KNOWN_HOSTS);
    assert_eq!(
        std::fs::metadata(kh).expect("kh meta").permissions().mode() & 0o777,
        0o600
    );

    let keygen = Command::new("ssh-keygen")
        .arg("-y")
        .arg("-f")
        .arg(&key)
        .output()
        .expect("ssh-keygen");
    assert!(
        keygen.status.success(),
        "ssh-keygen refused the materialised key: {}",
        String::from_utf8_lossy(&keygen.stderr)
    );

    drop(shell);
    assert!(!dir.exists(), "temp dir must be removed on drop");
    assert!(!key.exists());
}

fn transport_failed_detail(err: CoreError) -> String {
    match err {
        CoreError::TransportFailed { node, detail, .. } => {
            assert_eq!(node, "nodes.master");
            detail
        }
        other => panic!("expected transport_failed, got {other}"),
    }
}

#[test]
fn key_env_unset_is_transport_failed() {
    let repo = repo_with_known_hosts(KNOWN_HOSTS);
    let mut ssh = ssh_block();
    ssh.key_env = Some("PROD_KEY_NOT_SET".to_string());
    for env in [
        Box::new(|_: &str| None) as Box<dyn Fn(&str) -> Option<String>>,
        Box::new(|_: &str| Some(String::new())),
        Box::new(|_: &str| Some("  \r\n ".to_string())),
    ] {
        let err = RemoteShell::from_ssh_with_env(
            repo.path(),
            "targets/prod.yaml",
            "nodes.master",
            &ssh,
            runner(),
            &*env,
        )
        .expect_err("unset key variable");
        let detail = transport_failed_detail(err);
        assert!(detail.contains("PROD_KEY_NOT_SET"), "{detail}");
    }
}

#[test]
fn key_file_must_be_a_regular_file() {
    let repo = repo_with_known_hosts(KNOWN_HOSTS);
    let mut ssh = ssh_block();
    ssh.key_file = Some(repo.path().join("missing-key").display().to_string());
    let err = RemoteShell::from_ssh_with_env(
        repo.path(),
        "targets/prod.yaml",
        "nodes.master",
        &ssh,
        runner(),
        &no_env,
    )
    .expect_err("missing key file");
    assert!(transport_failed_detail(err).contains("key_file"));
    // a directory is not a key file either
    ssh.key_file = Some(repo.path().display().to_string());
    assert!(
        RemoteShell::from_ssh_with_env(
            repo.path(),
            "targets/prod.yaml",
            "nodes.master",
            &ssh,
            runner(),
            &no_env,
        )
        .is_err()
    );
}

#[test]
fn key_never_echoed() {
    let repo = repo_with_known_hosts(KNOWN_HOSTS);
    let mut ssh = ssh_block();
    ssh.key_env = Some("PROD_KEY".to_string());
    let key_text = format!(
        "-----BEGIN OPENSSH PRIVATE KEY-----\n{KEY_CANARY}\n-----END OPENSSH PRIVATE KEY-----"
    );
    let shell = shell(repo.path(), &ssh, &|name| {
        (name == "PROD_KEY").then(|| key_text.clone())
    });
    assert!(!format!("{shell:?}").contains(KEY_CANARY));
    for argv in [shell.ssh_argv(Some("true")), shell.sftp_argv()] {
        for arg in argv_strings(&argv) {
            assert!(!arg.contains(KEY_CANARY), "argv leaks the key: {arg}");
            assert!(!arg.contains("PRIVATE KEY"), "argv leaks the key: {arg}");
        }
    }
    // The file holds it (that is its job); nothing else does. A failure's
    // detail carries only a scrubbed stderr tail, with the temp path hidden.
    let temp = shell.temp_dir().expect("temp").display().to_string();
    let err = shell
        .require_success(
            "ls",
            ProcessOutput {
                stdout: Vec::new(),
                stderr: format!("Load key \"{temp}/key\": invalid format\n").into_bytes(),
                code: 255,
                timed_out: false,
            },
        )
        .expect_err("failed");
    let text = format!("{err} {err:?}");
    assert!(!text.contains(KEY_CANARY), "{text}");
    assert!(!text.contains(&temp), "temp path must be hidden: {text}");
    assert!(text.contains("<tmp>/key"), "{text}");
}

#[test]
fn distinct_temp_dirs() {
    let repo = repo_with_known_hosts(KNOWN_HOSTS);
    let a = shell(repo.path(), &ssh_block(), &no_env);
    let b = shell(repo.path(), &ssh_block(), &no_env);
    assert_ne!(a.temp_dir(), b.temp_dir());
    assert_ne!(a.known_hosts_copy(), b.known_hosts_copy());
    let a_dir = a.temp_dir().expect("a").to_path_buf();
    drop(a);
    assert!(!a_dir.exists());
    assert!(
        b.known_hosts_copy().exists(),
        "dropping one shell leaves the other intact"
    );
}

#[test]
fn known_hosts_pin_is_validated() {
    // empty and comment-only pins are refused
    for content in ["", "\n  \n", "# only a comment\n"] {
        let repo = repo_with_known_hosts(content);
        let err = RemoteShell::from_ssh_with_env(
            repo.path(),
            "targets/prod.yaml",
            "nodes.master",
            &ssh_block(),
            runner(),
            &no_env,
        )
        .expect_err("empty pin");
        assert!(
            matches!(err, CoreError::DeployTreeInvalid(..)),
            "{content:?}: {err}"
        );
    }
    // a missing file is deploy_tree_invalid naming the file
    let repo = tempfile::tempdir().expect("repo");
    let err = RemoteShell::from_ssh_with_env(
        repo.path(),
        "targets/prod.yaml",
        "nodes.master",
        &ssh_block(),
        runner(),
        &no_env,
    )
    .expect_err("missing pin");
    assert!(matches!(err, CoreError::DeployTreeInvalid(..)), "{err}");
}

#[cfg(unix)]
#[test]
fn known_hosts_symlink_is_refused() {
    let repo = repo_with_known_hosts(KNOWN_HOSTS);
    let outside = tempfile::tempdir().expect("outside");
    let real = outside.path().join("kh");
    std::fs::write(&real, KNOWN_HOSTS).expect("real");
    // the file itself is a symlink
    let link = repo.path().join("ssh/linked.known_hosts");
    std::os::unix::fs::symlink(&real, &link).expect("symlink");
    let mut ssh = ssh_block();
    ssh.known_hosts = "ssh/linked.known_hosts".to_string();
    let err = RemoteShell::from_ssh_with_env(
        repo.path(),
        "targets/prod.yaml",
        "nodes.master",
        &ssh,
        runner(),
        &no_env,
    )
    .expect_err("symlinked file");
    assert!(matches!(err, CoreError::DeployTreeInvalid(..)), "{err}");
    // a symlinked parent directory
    let dir_link = repo.path().join("linked-dir");
    std::os::unix::fs::symlink(outside.path(), &dir_link).expect("dir symlink");
    ssh.known_hosts = "linked-dir/kh".to_string();
    let err = RemoteShell::from_ssh_with_env(
        repo.path(),
        "targets/prod.yaml",
        "nodes.master",
        &ssh,
        runner(),
        &no_env,
    )
    .expect_err("symlinked parent");
    assert!(matches!(err, CoreError::DeployTreeInvalid(..)), "{err}");
}

/// One recorded call: program, argv, stdin bytes, timeout.
type Call = (OsString, Vec<String>, Option<Vec<u8>>, Duration);

/// Records what the shell asked the runner to run.
#[derive(Default)]
struct Recording {
    calls: Mutex<Vec<Call>>,
}

#[async_trait::async_trait]
impl ProcessRunner for Recording {
    async fn run(
        &self,
        program: &OsStr,
        args: &[OsString],
        stdin: Option<Vec<u8>>,
        timeout: Duration,
    ) -> ProcessOutput {
        self.calls.lock().expect("calls").push((
            program.to_owned(),
            argv_strings(args),
            stdin,
            timeout,
        ));
        ProcessOutput {
            stdout: b"ok".to_vec(),
            stderr: Vec::new(),
            code: 0,
            timed_out: false,
        }
    }
}

#[tokio::test]
async fn sftp_batch_feeds_stdin_with_the_op_timeout() {
    let repo = repo_with_known_hosts(KNOWN_HOSTS);
    let mut ssh = ssh_block();
    ssh.op_timeout_s = Some(7);
    let recording = Arc::new(Recording::default());
    let shell = RemoteShell::from_ssh_with_env(
        repo.path(),
        "targets/prod.yaml",
        "nodes.master",
        &ssh,
        recording.clone(),
        &no_env,
    )
    .expect("shell");
    let out = shell
        .sftp_batch(&["@ls -1 \"/d\"".to_string(), "@ls -1 \"/e\"".to_string()])
        .await
        .expect("batch");
    assert_eq!(out.stdout, b"ok");
    {
        let calls = recording.calls.lock().expect("calls");
        assert_eq!(calls.len(), 1);
        let (program, args, stdin, timeout) = &calls[0];
        assert_eq!(program, "sftp");
        assert_eq!(args, &argv_strings(&shell.sftp_argv()));
        assert_eq!(
            stdin.as_deref(),
            Some(b"@ls -1 \"/d\"\n@ls -1 \"/e\"\n".as_slice())
        );
        assert_eq!(*timeout, Duration::from_secs(7));
    }

    // a line break inside a line would start a second command: refused
    let err = shell
        .sftp_batch(&["ls \"/d\"\nrm \"/x\"".to_string()])
        .await
        .expect_err("injected line break");
    assert!(matches!(err, CoreError::TransportFailed { .. }));
    assert_eq!(recording.calls.lock().expect("calls").len(), 1);
}

#[test]
fn require_success_maps_every_failure_shape() {
    let repo = repo_with_known_hosts(KNOWN_HOSTS);
    let shell = shell(repo.path(), &ssh_block(), &no_env);
    let out = |stderr: &str, code: i32, timed_out: bool| ProcessOutput {
        stdout: b"stdout-must-not-leak".to_vec(),
        stderr: stderr.as_bytes().to_vec(),
        code,
        timed_out,
    };
    assert!(shell.require_success("ls", out("", 0, false)).is_ok());

    let detail = transport_failed_detail(
        shell
            .require_success("ls", out("", -1, true))
            .expect_err("timeout"),
    );
    assert!(detail.contains("timed out after 120s"), "{detail}");

    let detail = transport_failed_detail(
        shell
            .require_success("ls", out("Permission denied (publickey).\n", 255, false))
            .expect_err("nonzero"),
    );
    assert!(detail.contains("exit code 255"), "{detail}");
    assert!(detail.contains("Permission denied"), "{detail}");
    assert!(!detail.contains("stdout-must-not-leak"), "{detail}");

    // exit 0 with any stderr byte is still a failure (the sftp `-` prefix
    // trap: an error must never hide behind a zero exit)
    let detail = transport_failed_detail(
        shell
            .require_success("ls", out("Couldn't stat remote file\n", 0, false))
            .expect_err("stderr"),
    );
    assert!(detail.contains("Couldn't stat"), "{detail}");

    let detail = transport_failed_detail(
        shell
            .require_success("ls", out("failed to spawn sftp: NotFound", 127, false))
            .expect_err("spawn"),
    );
    assert!(detail.contains("127"), "{detail}");
}

#[cfg(unix)]
fn sftp_server() -> PathBuf {
    [
        "/usr/libexec/sftp-server",
        "/usr/lib/openssh/sftp-server",
        "/usr/lib/ssh/sftp-server",
        "/usr/libexec/openssh/sftp-server",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.exists())
    .expect("an OpenSSH sftp-server binary is needed for the local sftp seam test")
}

#[cfg(unix)]
#[tokio::test]
async fn local_sftp_server_seam_runs_a_batch() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("alpha"), b"1").expect("alpha");
    let shell = RemoteShell::local_sftp_server(&sftp_server(), runner(), Duration::from_secs(30));
    assert_eq!(argv_strings(&shell.sftp_argv())[..3], ["-q", "-b", "-"]);
    let batch = format!("@ls -1 \"{}\"", dir.path().display());
    let out = shell
        .require_success("ls", shell.sftp_batch(&[batch]).await.expect("run"))
        .expect("listing");
    let listing = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(listing.contains("alpha"), "{listing}");
}

#[cfg(windows)]
#[test]
fn windows_key_acl() {
    let repo = repo_with_known_hosts(KNOWN_HOSTS);
    let mut ssh = ssh_block();
    ssh.key_env = Some("PROD_KEY".to_string());
    let shell = shell(repo.path(), &ssh, &|name| {
        (name == "PROD_KEY").then(|| "-----BEGIN X-----\nabc\n-----END X-----".to_string())
    });
    let key = shell.identity().expect("identity");
    let out = Command::new("icacls").arg(key).output().expect("icacls");
    let listing = String::from_utf8_lossy(&out.stdout).into_owned();
    let user = std::env::var("USERNAME").expect("USERNAME");
    assert!(
        listing
            .to_lowercase()
            .contains(&format!("\\{}:", user.to_lowercase())),
        "{listing}"
    );
    for other in ["Everyone", "BUILTIN\\Users", "Authenticated Users"] {
        assert!(
            !listing.contains(other),
            "{other} still has access:\n{listing}"
        );
    }
}
