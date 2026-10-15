//! The ssh process layer (22-05): `RemoteShell` argv, host-key pin, key
//! handling and timeouts.
//!
//! `ssh_g_reports_pinned_options` runs the real `ssh -G` from PATH on the
//! generated argv; it fails with a clear message when `ssh` is absent and is
//! never skipped. `windows_key_acl` runs on the windows CI leg only.

mod common;

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ignition_core::deploy::target::{Ssh, TargetOs};
use ignition_core::deploy::transport::process::{ProcessOutput, ProcessRunner, TokioProcessRunner};
use ignition_core::deploy::transport::remote::{
    RemoteCommand, RemoteShell, SftpOp, hardened_options,
};
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
    let uname = RemoteCommand::posix(&["uname", "-a"]).expect("posix command");
    let with_command = argv_strings(&shell.ssh_argv(Some(&uname)));
    assert_eq!(
        &with_command[with_command.len() - 3..],
        ["--", "svc@gw.example.test", "'uname' '-a'"]
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
    let true_cmd = RemoteCommand::posix(&["true"]).expect("posix command");
    for argv in [shell.ssh_argv(Some(&true_cmd)), shell.sftp_argv()] {
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
        .sftp_batch(&[
            SftpOp::Ls {
                path: "/d".to_string(),
            },
            SftpOp::Ls {
                path: "/e".to_string(),
            },
        ])
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

    // a line break inside a path would start a second command: refused
    let err = shell
        .sftp_batch(&[SftpOp::Ls {
            path: "/d\"\nrm \"/x".to_string(),
        }])
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
    assert!(detail.contains("timed out after 1050s"), "{detail}");

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
    common::sftp_server::sftp_server_path()
}

#[cfg(unix)]
#[tokio::test]
async fn local_sftp_server_seam_runs_a_batch() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("alpha"), b"1").expect("alpha");
    let shell = RemoteShell::local_sftp_server(&sftp_server(), runner(), Duration::from_secs(30));
    assert_eq!(argv_strings(&shell.sftp_argv())[..3], ["-q", "-b", "-"]);
    let batch = SftpOp::Ls {
        path: dir.path().display().to_string(),
    };
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

#[cfg(unix)]
#[tokio::test]
async fn local_sftp_server_keeps_hostile_path_text_inert() {
    // a directory whose name carries quote, command-substitution, backtick and
    // `;` text, listed, then a directory named like a local-shell escape made
    // and removed: sftp's double quotes keep all of it a name
    let root = tempfile::tempdir().expect("dir");
    let hostile = root
        .path()
        .join("it's $(touch IGN_PWNED_MARKER) `id`; echo");
    std::fs::create_dir(&hostile).expect("hostile dir");
    std::fs::write(hostile.join("inner"), b"1").expect("inner");
    let shell = RemoteShell::local_sftp_server(&sftp_server(), runner(), Duration::from_secs(30));

    let out = shell
        .require_success(
            "ls",
            shell
                .sftp_batch(&[SftpOp::Ls {
                    path: hostile.display().to_string(),
                }])
                .await
                .expect("run"),
        )
        .expect("listing");
    assert!(String::from_utf8_lossy(&out.stdout).contains("inner"));

    let bang = root.path().join("!touch IGN_PWNED_MARKER");
    for op in [
        SftpOp::Mkdir {
            path: bang.display().to_string(),
        },
        SftpOp::Rmdir {
            path: bang.display().to_string(),
        },
    ] {
        let made = bang.exists();
        shell
            .require_success("op", shell.sftp_batch(&[op]).await.expect("run"))
            .expect("op succeeded");
        assert_ne!(
            made,
            bang.exists(),
            "mkdir then rmdir toggled the directory"
        );
    }
    for dir in [root.path(), Path::new(".")] {
        assert!(
            !dir.join("IGN_PWNED_MARKER").exists(),
            "a hostile path text ran as a command"
        );
    }
}

// ---- the ssh read half over a real sftp-server (22-06) ----------------------
//
// Every test below drives `sftp -D <sftp-server>`: the actual sftp protocol
// against a local server process, no sshd. `sftp_server_path()` panics, never
// skips, when no server binary is installed.

#[cfg(unix)]
mod sftp_read {
    use super::*;
    use common::plan_fixture::{GATEWAY, MODE, PlanFixture};
    use ignition_core::actions::deploy::{PlanOptions, deploy_plan, load_declared};
    use ignition_core::deploy::transport::local::LocalFsTransport;
    use ignition_core::deploy::transport::sftp::{SftpTransport, posix_root};
    use ignition_core::deploy::transport::{self, ObservedFiles, Scope, TreeReader};

    fn sftp_reader(data_dir: &Path) -> SftpTransport {
        sftp_reader_with(data_dir, runner())
    }

    /// Runs the real `sftp` with `home` as its working directory, which is
    /// what the server's `pwd` answers: the account home of Decision B(i).
    /// The tests use the fixture data directory as that home, so a ledger
    /// `PlanFixture::ledger` writes below the data directory is the ledger
    /// the ssh reader finds in "the home".
    struct Homed {
        home: PathBuf,
        inner: Arc<dyn ProcessRunner>,
    }

    #[async_trait::async_trait]
    impl ProcessRunner for Homed {
        async fn run(
            &self,
            program: &OsStr,
            args: &[OsString],
            stdin: Option<Vec<u8>>,
            timeout: Duration,
        ) -> ProcessOutput {
            let mut wrapped: Vec<OsString> = vec![
                OsString::from("-c"),
                OsString::from("cd \"$0\" && exec \"$@\""),
                self.home.clone().into_os_string(),
                program.to_os_string(),
            ];
            wrapped.extend(args.iter().cloned());
            self.inner
                .run(OsStr::new("sh"), &wrapped, stdin, timeout)
                .await
        }
    }

    fn sftp_reader_with(data_dir: &Path, runner: Arc<dyn ProcessRunner>) -> SftpTransport {
        let runner: Arc<dyn ProcessRunner> = Arc::new(Homed {
            home: data_dir.to_path_buf(),
            inner: runner,
        });
        let shell = RemoteShell::local_sftp_server(&sftp_server(), runner, Duration::from_secs(60));
        let root = posix_root(&data_dir.display().to_string()).expect("posix data dir");
        SftpTransport::new(shell, root, "nodes.master")
    }

    fn local_reader(data_dir: &Path) -> LocalFsTransport {
        LocalFsTransport::new(data_dir.to_path_buf(), "nodes.master")
    }

    fn scope_of(fx: &PlanFixture) -> Scope {
        let (declared, _) = fx.load("dev");
        Scope::from_manifest(&declared.manifest, MODE)
    }

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, bytes).expect("write");
    }

    fn failure_detail(err: CoreError) -> String {
        match err {
            CoreError::TransportFailed { detail, .. } => detail,
            other => panic!("expected transport_failed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn sftp_read_matches_local_read() {
        let fx = PlanFixture::new();
        let (declared, _) = fx.load("dev");
        fx.live_render("dev", &declared);
        fx.core(
            "ignition/database-translator/POSTGRES/config.json",
            b"{\"core\":true}",
        );
        fx.core("ignition/database-translator/POSTGRES/resource.json", b"{}");
        fx.core("ignition/general-alarm-settings/config.json", b"{}");
        fx.mode(
            "ignition/database-translator/POSTGRES/.dotfile",
            b"hidden but real",
        );
        fx.mode(
            "ignition/database-translator/Empty Dir,1/nested/deeper/file.bin",
            &[0u8, 255, 10, 13],
        );
        fx.local(
            "ignition/database-translator/POSTGRES/config.json",
            b"{\"local\":1}",
        );
        let scope = scope_of(&fx);

        let want = local_reader(fx.data_dir())
            .read(&scope)
            .await
            .expect("local read");
        assert!(!want.core.is_empty() && !want.mode.is_empty() && !want.local_units.is_empty());
        let reader = sftp_reader(fx.data_dir());
        let got = reader.read(&scope).await.expect("sftp read");
        assert_eq!(got, want);
        println!("sftp sessions for one read: {}", reader.sessions());

        for collection in ["core", MODE, "local", "nothing"] {
            assert_eq!(
                reader.list_types(collection).await.expect("sftp types"),
                local_reader(fx.data_dir())
                    .list_types(collection)
                    .await
                    .expect("local types"),
                "{collection}"
            );
        }
    }

    #[tokio::test]
    async fn sftp_plan_matches_local_plan() {
        let fx = PlanFixture::new();
        let (declared, local) = fx.load("dev");
        fx.live_render("dev", &declared);
        fx.core(
            "ignition/database-translator/POSTGRES/config.json",
            b"{\"core\":true}",
        );
        fx.mode("ignition/database-translator/OLD/config.json", b"{}");
        fx.mode("ignition/database-translator/OLD/resource.json", b"{}");
        fx.mode(
            "ignition/database-translator/POSTGRES/config.json",
            b"{\"drifted\":1}",
        );
        fx.local("ignition/general-alarm-settings/config.json", b"{}");
        fx.ledger(
            GATEWAY,
            MODE,
            &[
                "ignition/database-translator/OLD/config.json",
                "ignition/database-translator/OLD/resource.json",
            ],
        );
        let want = deploy_plan(&declared, local.as_ref(), &PlanOptions::default())
            .await
            .expect("local plan");
        assert_eq!(want.delete_set.files.len(), 2, "{:?}", want.delete_set);
        assert!(want.summary.has_changes);

        let sftp = sftp_reader(fx.data_dir());
        let got = deploy_plan(&declared, &sftp, &PlanOptions::default())
            .await
            .expect("sftp plan");
        assert_eq!(got.plan_sha256, want.plan_sha256);
        assert_eq!(got.delete_set, want.delete_set);
        assert_eq!(
            serde_json::to_string(&got).expect("json"),
            serde_json::to_string(&want).expect("json"),
            "same entries, delete set and hash"
        );
        // and the transport label is the only thing that differs once the
        // node is an ssh node
        let mut as_ssh = serde_json::to_value(&got).expect("json");
        as_ssh["transport"] = serde_json::json!("ssh");
        assert_eq!(as_ssh["plan_sha256"], serde_json::json!(want.plan_sha256));
    }

    #[tokio::test]
    async fn sftp_spaces_and_commas() {
        let root = tempfile::tempdir().expect("root");
        let data = root.path().join("gw data, 1");
        let res = data.join("config/resources");
        write(
            &res.join("dev/ignition/database-translator/My DB, the 1st/config.json"),
            b"{\"a\":1}",
        );
        write(
            &res.join("core/ignition/database-translator/Other, two/resource.json"),
            b"{}",
        );
        let scope = Scope {
            mode: "dev".to_string(),
            types: vec![("ignition".to_string(), "database-translator".to_string())],
            tag_providers: Vec::new(),
        };
        let got = sftp_reader(&data).read(&scope).await.expect("sftp read");
        let want = local_reader(&data).read(&scope).await.expect("local read");
        assert_eq!(got, want);
        assert_eq!(got.mode.len(), 1);
        assert_eq!(got.core.len(), 1);
    }

    #[tokio::test]
    async fn sftp_hostile_but_legal_names_are_inert() {
        let root = tempfile::tempdir().expect("root");
        let data = root.path().join("data");
        let res = data.join("config/resources");
        let name = "it's $(touch IGN_PWNED_MARKER) `id`; echo !x";
        write(
            &res.join(format!(
                "dev/ignition/database-translator/{name}/config.json"
            )),
            b"{}",
        );
        let scope = Scope {
            mode: "dev".to_string(),
            types: vec![("ignition".to_string(), "database-translator".to_string())],
            tag_providers: Vec::new(),
        };
        let got = sftp_reader(&data).read(&scope).await.expect("sftp read");
        assert_eq!(got, local_reader(&data).read(&scope).await.expect("local"));
        assert_eq!(got.mode.len(), 1);
        for dir in [root.path(), &data, Path::new(".")] {
            assert!(!dir.join("IGN_PWNED_MARKER").exists());
        }
    }

    #[tokio::test]
    async fn sftp_unsafe_names_are_refused() {
        for name in ["odd[1]", "back\\slash", "q\"uote", "g*lob"] {
            let root = tempfile::tempdir().expect("root");
            let data = root.path().join("data");
            write(
                &data.join(format!(
                    "config/resources/dev/ignition/database-translator/{name}/config.json"
                )),
                b"{}",
            );
            let scope = Scope {
                mode: "dev".to_string(),
                types: vec![("ignition".to_string(), "database-translator".to_string())],
                tag_providers: Vec::new(),
            };
            let why = failure_detail(sftp_reader(&data).read(&scope).await.expect_err(name));
            assert!(why.contains("unsafe"), "{name}: {why}");
        }
    }

    #[tokio::test]
    async fn sftp_absent_mode_collection_is_empty() {
        let fx = PlanFixture::new();
        // config/resources exists, no collection does
        let scope = scope_of(&fx);
        let got = sftp_reader(fx.data_dir()).read(&scope).await.expect("read");
        assert_eq!(got, ObservedFiles::default());
        // a core collection without the mode collection
        fx.core("ignition/database-translator/POSTGRES/config.json", b"{}");
        let got = sftp_reader(fx.data_dir()).read(&scope).await.expect("read");
        assert_eq!(got.core.len(), 1);
        assert!(got.mode.is_empty());
        // a data dir without config/resources is a failure, not emptiness
        let bare = tempfile::tempdir().expect("bare");
        let why = failure_detail(
            sftp_reader(bare.path())
                .read(&scope)
                .await
                .expect_err("bare"),
        );
        assert!(why.contains("gateway data directory"), "{why}");
    }

    #[tokio::test]
    async fn sftp_absent_ledger_is_none() {
        let fx = PlanFixture::new();
        let reader = sftp_reader(fx.data_dir());
        assert_eq!(reader.read_ledger(GATEWAY, MODE).await.expect("none"), None);
        std::fs::create_dir_all(fx.data_dir().join("ign-deploy")).expect("dir");
        assert_eq!(reader.read_ledger(GATEWAY, MODE).await.expect("none"), None);
        std::fs::create_dir_all(fx.data_dir().join("ign-deploy").join(GATEWAY).join(MODE))
            .expect("dir");
        assert_eq!(reader.read_ledger(GATEWAY, MODE).await.expect("none"), None);
        // an unreadable host is never an absent ledger
        let missing = fx.data_dir().join("nowhere");
        let err = sftp_reader(&missing)
            .read_ledger(GATEWAY, MODE)
            .await
            .expect_err("missing data dir");
        assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
    }

    #[tokio::test]
    async fn sftp_ledger_present_round_trips() {
        let fx = PlanFixture::new();
        fx.ledger(
            GATEWAY,
            MODE,
            &["ignition/database-translator/A/config.json"],
        );
        let want = local_reader(fx.data_dir())
            .read_ledger(GATEWAY, MODE)
            .await
            .expect("local")
            .expect("present");
        let got = sftp_reader(fx.data_dir())
            .read_ledger(GATEWAY, MODE)
            .await
            .expect("sftp")
            .expect("present");
        assert_eq!(got, want);
        // a symlinked ledger is a deploy_ledger_invalid, as locally
        let ledger = fx
            .data_dir()
            .join("ign-deploy")
            .join(GATEWAY)
            .join(MODE)
            .join("ledger.json");
        std::fs::rename(&ledger, ledger.with_file_name("real.json")).expect("rename");
        std::os::unix::fs::symlink("real.json", &ledger).expect("symlink");
        let err = sftp_reader(fx.data_dir())
            .read_ledger(GATEWAY, MODE)
            .await
            .expect_err("symlink");
        assert!(
            matches!(err, CoreError::DeployLedgerInvalid { .. }),
            "{err:?}"
        );
    }

    /// Restores permissions so the temp dir can be removed.
    struct Chmod(PathBuf, u32);
    impl Drop for Chmod {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(self.1));
        }
    }

    #[tokio::test]
    async fn sftp_permission_denied_is_transport_failed() {
        use std::os::unix::fs::PermissionsExt;
        let fx = PlanFixture::new();
        let (declared, _) = fx.load("dev");
        fx.live_render("dev", &declared);
        let scope = scope_of(&fx);
        let type_dir = fx.collection_dir(MODE).join("ignition/database-translator");
        let file = type_dir.join("POSTGRES/config.json");

        // a directory the server cannot open
        let _restore_dir = Chmod(type_dir.clone(), 0o755);
        std::fs::set_permissions(&type_dir, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        if std::fs::read_dir(&type_dir).is_ok() {
            println!("running with privileges that ignore mode bits; permission cases skipped");
            return;
        }
        let why = failure_detail(
            sftp_reader(fx.data_dir())
                .read(&scope)
                .await
                .expect_err("denied dir"),
        );
        assert!(why.contains("exit code 1"), "{why}");
        std::fs::set_permissions(&type_dir, std::fs::Permissions::from_mode(0o755)).expect("chmod");

        // a file the server cannot read
        let _restore_file = Chmod(file.clone(), 0o644);
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        let why = failure_detail(
            sftp_reader(fx.data_dir())
                .read(&scope)
                .await
                .expect_err("denied file"),
        );
        assert!(why.contains("exit code 1"), "{why}");
    }

    #[tokio::test]
    async fn sftp_glob_data_dir_refused_before_spawn() {
        let recording = Arc::new(Recording::default());
        let shell = RemoteShell::local_sftp_server(
            &sftp_server(),
            recording.clone(),
            Duration::from_secs(60),
        );
        // posix_root refuses it, and a transport built without it still
        // refuses before any process is spawned
        assert!(posix_root("/tmp/da[ta]").is_err());
        let reader = SftpTransport::new(shell, "/tmp/da[ta]".to_string(), "nodes.master");
        let scope = Scope {
            mode: "dev".to_string(),
            types: vec![("ignition".to_string(), "database-translator".to_string())],
            tag_providers: Vec::new(),
        };
        reader.read(&scope).await.expect_err("glob data dir");
        reader.list_types("core").await.expect_err("glob data dir");
        assert!(
            recording.calls.lock().expect("calls").is_empty(),
            "no process was spawned"
        );
        // the ledger is read from the account home (Decision B(i)), so the
        // data directory is not part of it
        let _ = reader.read_ledger("gw", "dev").await;
    }

    #[tokio::test]
    async fn sftp_get_skips_symlinks_documented() {
        // Pins the OpenSSH behaviour that makes the listing check necessary:
        // `get -R` of a tree holding a symlink exits 0 and silently skips it.
        // If this ever changes, the T-22-28 reasoning needs another look.
        let remote = tempfile::tempdir().expect("remote");
        let tree = remote.path().join("tree");
        write(&tree.join("real.txt"), b"x");
        std::os::unix::fs::symlink("real.txt", tree.join("link.txt")).expect("symlink");
        let local = tempfile::tempdir().expect("local");
        let shell =
            RemoteShell::local_sftp_server(&sftp_server(), runner(), Duration::from_secs(60));
        let out = shell
            .sftp_batch(&[SftpOp::GetRecursive {
                remote: tree.display().to_string(),
                local: local.path().to_path_buf(),
            }])
            .await
            .expect("batch");
        shell.require_success("get", out).expect("exit 0");
        assert!(local.path().join("tree/real.txt").exists());
        assert!(!local.path().join("tree/link.txt").exists(), "skipped");
    }

    #[tokio::test]
    async fn sftp_symlink_in_resource_is_refused() {
        let fx = PlanFixture::new();
        let (declared, _) = fx.load("dev");
        fx.live_render("dev", &declared);
        let scope = scope_of(&fx);
        let unit = fx
            .collection_dir(MODE)
            .join("ignition/database-translator/POSTGRES");
        std::os::unix::fs::symlink("config.json", unit.join("alias.json")).expect("symlink");
        let why = failure_detail(
            sftp_reader(fx.data_dir())
                .read(&scope)
                .await
                .expect_err("symlink"),
        );
        assert!(why.contains("alias.json"), "{why}");
        assert!(why.contains("is a symlink; symlinks are refused"), "{why}");
        // the local transport refuses the same tree for the same reason
        let local = failure_detail(
            local_reader(fx.data_dir())
                .read(&scope)
                .await
                .expect_err("symlink"),
        );
        assert!(local.contains("symlinks are refused"), "{local}");
    }

    #[tokio::test]
    async fn for_node_builds_sftp_for_linux_and_windows() {
        let fx = PlanFixture::new();
        let ssh_yaml = |os: &str, data_dir: &str| {
            format!(
                "    ssh:\n      host: gw.example.test\n      user: svc\n      os: {os}\n      data_dir: '{data_dir}'\n      known_hosts: ssh/prod.known_hosts\n"
            )
        };
        let load = |yaml: String| {
            let repo = fx.repo_with_master("dev", &yaml);
            std::fs::create_dir_all(repo.join("ssh")).expect("ssh dir");
            std::fs::write(repo.join("ssh/prod.known_hosts"), KNOWN_HOSTS).expect("known_hosts");
            let declared = load_declared(&repo, "dev").expect("declared");
            let node = declared.target.nodes.master.clone();
            (
                transport::for_node(&repo, &declared.target_file, "nodes.master", &node),
                declared.target_file,
            )
        };
        let (linux, _) = load(ssh_yaml("linux", "/usr/local/bin/ignition/data"));
        assert!(linux.is_ok(), "linux ssh node builds a reader");

        // a Windows OpenSSH node builds a reader from a drive-letter path (22-13)
        let (windows, _) = load(ssh_yaml(
            "windows",
            "C:\\Program Files\\Inductive Automation\\Ignition\\data",
        ));
        assert!(windows.is_ok(), "windows ssh node builds a reader");
        for bad in [
            "relative\\data",
            "\\\\server\\share\\data",
            "C:data",
            "/usr/local/ignition/data",
            "C:\\a\\..\\b",
        ] {
            let (refused, _) = load(ssh_yaml("windows", bad));
            let err = refused.err().expect(bad);
            assert_eq!(err.code(), "deploy_tree_invalid", "{bad}");
            assert!(
                err.to_string().contains("nodes.master.ssh.data_dir"),
                "{bad}: {err}"
            );
        }
        // and a POSIX path is refused for a Windows node and the reverse
        let (reverse, _) = load(ssh_yaml("linux", "C:\\Program Files\\Ignition\\data"));
        assert!(reverse.is_err(), "a drive-letter path for a linux node");

        let (bad_dir, _) = load(ssh_yaml("linux", "relative/data"));
        let err = bad_dir.err().expect("relative refused");
        assert_eq!(err.code(), "deploy_tree_invalid");
        assert!(
            err.to_string().contains("nodes.master.ssh.data_dir"),
            "{err}"
        );
    }
}

/// The write half (22-07): scratch collections only, no verb involved.
mod mirror {
    use super::*;
    use ignition_core::deploy::layout::RelPath;
    use ignition_core::deploy::target::{Local, NodeTarget};
    use ignition_core::deploy::transport::local::LocalFsTransport;
    use ignition_core::deploy::transport::{
        self, DeleteSet, MirrorReport, READ_MAX_FILE_BYTES, StagingMode, TreeMirror,
    };
    use ignition_core::deploy::tree::RenderedTree;
    use std::collections::BTreeMap;

    pub const MODE: &str = "dev";
    pub const DT: &str = "ignition/database-translator";

    /// A scratch gateway data directory plus a sibling staging root.
    pub struct Scratch {
        pub root: tempfile::TempDir,
        pub data: PathBuf,
        pub stage_root: PathBuf,
    }

    pub type Snapshot = BTreeMap<String, Option<Vec<u8>>>;

    impl Scratch {
        pub fn new() -> Scratch {
            let root = tempfile::tempdir().expect("scratch");
            let data = root.path().join("data");
            let stage_root = root.path().join("stage");
            let sc = Scratch {
                root,
                data,
                stage_root,
            };
            let dev = sc.resources().join(MODE);
            put(&dev.join("config-mode.json"), b"{\"mode\":\"dev\"}");
            put(&dev.join(format!("{DT}/KEEP/config.json")), b"{\"keep\":1}");
            put(&dev.join(format!("{DT}/KEEP/resource.json")), b"{}");
            put(&dev.join(format!("{DT}/OLD/config.json")), b"{\"old\":1}");
            put(&dev.join(format!("{DT}/OLD/resource.json")), b"{\"old\":2}");
            put(&dev.join(format!("{DT}/UPD/config.json")), b"{\"v\":1}");
            put(
                &dev.join("ignition/general-alarm-settings/config.json"),
                b"{\"alarm\":1}",
            );
            put(
                &sc.resources().join(format!("core/{DT}/CORE/config.json")),
                b"{\"core\":1}",
            );
            sc
        }

        pub fn resources(&self) -> PathBuf {
            self.data.join("config/resources")
        }

        pub fn collection(&self) -> PathBuf {
            self.resources().join(MODE)
        }

        pub fn separate(&self) -> StagingMode {
            StagingMode::Separate {
                root: self.stage_root.display().to_string(),
            }
        }
    }

    pub fn put(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, bytes).expect("write");
    }

    /// Every file (bytes) and directory (`None`) below `dir`, by relative path.
    pub fn snapshot(dir: &Path) -> Snapshot {
        fn walk(dir: &Path, rel: &str, out: &mut Snapshot) {
            let mut entries: Vec<_> = std::fs::read_dir(dir)
                .expect("read_dir")
                .map(|e| e.expect("entry"))
                .collect();
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                let name = entry.file_name().into_string().expect("utf8");
                let path = format!("{rel}{name}");
                let meta = std::fs::symlink_metadata(entry.path()).expect("meta");
                if meta.file_type().is_dir() {
                    out.insert(path.clone(), None);
                    walk(&entry.path(), &format!("{path}/"), out);
                } else if meta.file_type().is_symlink() {
                    out.insert(path, Some(b"<symlink>".to_vec()));
                } else {
                    out.insert(path, Some(std::fs::read(entry.path()).expect("read")));
                }
            }
        }
        let mut out = Snapshot::new();
        walk(dir, "", &mut out);
        out
    }

    pub fn rel(text: &str) -> RelPath {
        RelPath::new(text).expect("rel path")
    }

    pub fn tree(files: &[(&str, &[u8])]) -> RenderedTree {
        RenderedTree {
            files: files
                .iter()
                .map(|(path, bytes)| (rel(path), bytes.to_vec()))
                .collect(),
        }
    }

    pub fn deletes(paths: &[&str]) -> DeleteSet {
        DeleteSet {
            files: paths.iter().map(|p| rel(p)).collect(),
        }
    }

    pub fn scenario_tree() -> RenderedTree {
        tree(&[
            (&format!("{DT}/UPD/config.json"), b"{\"v\":2}"),
            (&format!("{DT}/NEW/config.json"), b"{\"new\":1}"),
            (&format!("{DT}/NEW/resource.json"), b"{\"new\":2}"),
        ])
    }

    pub fn scenario_deletes() -> DeleteSet {
        deletes(&[
            &format!("{DT}/OLD/config.json"),
            &format!("{DT}/OLD/resource.json"),
        ])
    }

    pub fn local_mirror(sc: &Scratch, staging: StagingMode) -> LocalFsTransport {
        LocalFsTransport::new(sc.data.clone(), "nodes.master")
            .with_mirror(MODE, staging)
            .expect("mirror")
    }

    /// A walk finds no dot-named entry and no temp-looking name.
    pub fn assert_nothing_foreign(snap: &Snapshot) {
        for path in snap.keys() {
            for part in path.split('/') {
                assert!(!part.starts_with('.'), "dot-named entry {path}");
                for suffix in [".tmp", ".bak", ".stage", ".part", "~", ".orig", ".new"] {
                    assert!(!part.ends_with(suffix), "temp-looking entry {path}");
                }
            }
        }
    }

    /// The scenario both transports run: update, add, delete, leave the rest.
    pub async fn run_scenario(m: &dyn TreeMirror, sc: &Scratch) -> (MirrorReport, Snapshot) {
        let before = snapshot(&sc.resources());
        let staged = m.stage(&scenario_tree()).await.expect("stage");
        assert_eq!(
            snapshot(&sc.resources()),
            before,
            "stage must not touch the collection"
        );
        let report = m
            .commit(&staged, &scenario_deletes())
            .await
            .expect("commit");
        let after = snapshot(&sc.resources());

        let mut expected = before.clone();
        for gone in [
            format!("{MODE}/{DT}/OLD"),
            format!("{MODE}/{DT}/OLD/config.json"),
            format!("{MODE}/{DT}/OLD/resource.json"),
        ] {
            assert!(expected.remove(&gone).is_some(), "{gone}");
        }
        expected.insert(
            format!("{MODE}/{DT}/UPD/config.json"),
            Some(b"{\"v\":2}".to_vec()),
        );
        expected.insert(format!("{MODE}/{DT}/NEW"), None);
        expected.insert(
            format!("{MODE}/{DT}/NEW/config.json"),
            Some(b"{\"new\":1}".to_vec()),
        );
        expected.insert(
            format!("{MODE}/{DT}/NEW/resource.json"),
            Some(b"{\"new\":2}".to_vec()),
        );
        assert_eq!(
            after, expected,
            "exactly the staged and deleted files changed"
        );
        assert_eq!(
            after[&format!("{MODE}/config-mode.json")],
            Some(b"{\"mode\":\"dev\"}".to_vec())
        );
        assert!(after.contains_key(&format!("{MODE}/{DT}/KEEP/config.json")));
        assert_eq!(
            report,
            MirrorReport {
                written: vec![
                    format!("{DT}/NEW/config.json"),
                    format!("{DT}/NEW/resource.json"),
                    format!("{DT}/UPD/config.json"),
                ],
                deleted: vec![
                    format!("{DT}/OLD/config.json"),
                    format!("{DT}/OLD/resource.json"),
                ],
                removed_dirs: vec![format!("{DT}/OLD")],
            }
        );
        assert_nothing_foreign(&after);
        (report, after)
    }

    pub fn code(err: &CoreError) -> &'static str {
        err.code()
    }

    /// Refusals that must leave the collection byte-identical.
    pub async fn assert_refusals_write_nothing(m: &dyn TreeMirror, sc: &Scratch) {
        let staged = m.stage(&scenario_tree()).await.expect("stage");
        let before = snapshot(&sc.resources());
        let cases: Vec<(&str, DeleteSet)> = vec![
            ("gateway-owned", deletes(&["config-mode.json"])),
            (
                "gateway-owned deep",
                deletes(&[&format!("{DT}/config-mode.json")]),
            ),
            ("dot-named", deletes(&["ignition/.x/a/b"])),
            ("too short", deletes(&["ignition/x"])),
            ("staged", deletes(&[&format!("{DT}/UPD/config.json")])),
            (
                "twice",
                deletes(&[
                    &format!("{DT}/OLD/config.json"),
                    &format!("{DT}/OLD/config.json"),
                ]),
            ),
            (
                "does not exist",
                deletes(&[
                    &format!("{DT}/OLD/config.json"),
                    &format!("{DT}/GONE/config.json"),
                ]),
            ),
            ("a directory", deletes(&[&format!("{DT}/KEEP")])),
            (
                "file and directory of another",
                deletes(&[
                    &format!("{DT}/OLD/config.json"),
                    &format!("{DT}/OLD/config.json/x"),
                ]),
            ),
        ];
        for (name, set) in cases {
            let err = m
                .commit(&staged, &set)
                .await
                .expect_err(&format!("{name} must be refused"));
            assert_eq!(code(&err), "transport_failed", "{name}: {err}");
            assert_eq!(snapshot(&sc.resources()), before, "{name} wrote something");
        }
    }

    #[tokio::test]
    async fn local_mirror_separate() {
        let sc = Scratch::new();
        let m = local_mirror(&sc, sc.separate());
        run_scenario(&m, &sc).await;
        // the staging root is outside the collection and its files were moved
        assert!(!sc.resources().join(".stage").exists());
        assert!(!snapshot(&sc.stage_root).values().any(Option::is_some));
    }

    #[tokio::test]
    async fn local_mirror_inplace() {
        let sc = Scratch::new();
        let m = local_mirror(&sc, StagingMode::InPlace);
        run_scenario(&m, &sc).await;
        assert!(
            !sc.stage_root.exists(),
            "inplace never makes a staging root"
        );
    }

    #[tokio::test]
    async fn no_foreign_entries_in_collection() {
        for separate in [true, false] {
            let sc = Scratch::new();
            let staging = if separate {
                sc.separate()
            } else {
                StagingMode::InPlace
            };
            let m = local_mirror(&sc, staging);
            let staged = m.stage(&scenario_tree()).await.expect("stage");
            assert_nothing_foreign(&snapshot(&sc.resources()));
            m.commit(&staged, &scenario_deletes())
                .await
                .expect("commit");
            assert_nothing_foreign(&snapshot(&sc.resources()));
            // the data dir holds only what it held
            assert_eq!(
                snapshot(&sc.data)
                    .keys()
                    .filter(|k| !k.starts_with("config/resources"))
                    .count(),
                1, // `config`
                "separate={separate}"
            );
        }
    }

    #[tokio::test]
    async fn delete_set_refusals_write_nothing() {
        for separate in [true, false] {
            let sc = Scratch::new();
            let staging = if separate {
                sc.separate()
            } else {
                StagingMode::InPlace
            };
            assert_refusals_write_nothing(&local_mirror(&sc, staging), &sc).await;
        }
    }

    #[tokio::test]
    async fn staging_root_inside_collection_refused() {
        let sc = Scratch::new();
        let inside = |tail: &str| StagingMode::Separate {
            root: sc.resources().join(tail).display().to_string(),
        };
        for staging in [
            inside("dev/stage"),
            inside("stage"),
            inside("core/ignition/stage"),
            StagingMode::Separate {
                root: sc.resources().display().to_string(),
            },
            // around it
            StagingMode::Separate {
                root: sc.data.display().to_string(),
            },
            StagingMode::Separate {
                root: "relative/stage".to_string(),
            },
            StagingMode::Separate {
                root: "/onlyone".to_string(),
            },
        ] {
            let why = LocalFsTransport::new(sc.data.clone(), "nodes.master")
                .with_mirror(MODE, staging.clone())
                .expect_err(&format!("{staging:?}"));
            assert!(why.contains("staging root"), "{why}");
        }
        assert!(!sc.resources().join("dev/stage").exists());
        assert!(!sc.resources().join("stage").exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn staging_root_through_a_symlink_into_the_collection_refused() {
        let sc = Scratch::new();
        let link = sc.root.path().join("sneaky");
        std::os::unix::fs::symlink(sc.resources().join("dev/ignition"), &link).expect("symlink");
        let m = local_mirror(
            &sc,
            StagingMode::Separate {
                root: link.join("stage").display().to_string(),
            },
        );
        let before = snapshot(&sc.resources());
        let err = m.stage(&scenario_tree()).await.expect_err("overlap");
        assert!(err.to_string().contains("overlaps"), "{err}");
        assert_eq!(snapshot(&sc.resources()), before);
    }

    #[tokio::test]
    async fn commit_is_idempotent() {
        for separate in [true, false] {
            let sc = Scratch::new();
            let staging = if separate {
                sc.separate()
            } else {
                StagingMode::InPlace
            };
            let m = local_mirror(&sc, staging);
            let rendered = scenario_tree();
            let staged = m.stage(&rendered).await.expect("stage");
            m.commit(&staged, &DeleteSet::default())
                .await
                .expect("first");
            let after_first = snapshot(&sc.resources());
            let staged = m.stage(&rendered).await.expect("stage again");
            let report = m
                .commit(&staged, &DeleteSet::default())
                .await
                .expect("second");
            assert_eq!(
                snapshot(&sc.resources()),
                after_first,
                "separate={separate}"
            );
            assert!(report.deleted.is_empty() && report.removed_dirs.is_empty());
        }
    }

    #[tokio::test]
    async fn module_type_dirs_survive() {
        let sc = Scratch::new();
        // the only resource of its type
        let m = local_mirror(&sc, StagingMode::InPlace);
        let staged = m.stage(&RenderedTree::default()).await.expect("stage");
        let report = m
            .commit(
                &staged,
                &deletes(&["ignition/general-alarm-settings/config.json"]),
            )
            .await
            .expect("commit");
        assert!(report.removed_dirs.is_empty(), "{report:?}");
        assert!(
            sc.collection()
                .join("ignition/general-alarm-settings")
                .is_dir()
        );
        // every unit of a type
        let report = m
            .commit(
                &staged,
                &deletes(&[
                    &format!("{DT}/KEEP/config.json"),
                    &format!("{DT}/KEEP/resource.json"),
                    &format!("{DT}/OLD/config.json"),
                    &format!("{DT}/OLD/resource.json"),
                    &format!("{DT}/UPD/config.json"),
                ]),
            )
            .await
            .expect("commit all");
        assert_eq!(
            report.removed_dirs,
            vec![
                format!("{DT}/KEEP"),
                format!("{DT}/OLD"),
                format!("{DT}/UPD")
            ]
        );
        assert!(
            sc.collection().join(DT).is_dir(),
            "<module>/<type> survives"
        );
        assert!(sc.collection().join("ignition").is_dir());
        assert!(sc.collection().join("config-mode.json").is_file());
    }

    #[tokio::test]
    async fn a_dir_with_other_files_is_not_removed() {
        let sc = Scratch::new();
        put(&sc.collection().join(format!("{DT}/OLD/extra.json")), b"{}");
        let m = local_mirror(&sc, StagingMode::InPlace);
        let staged = m.stage(&RenderedTree::default()).await.expect("stage");
        let report = m
            .commit(&staged, &scenario_deletes())
            .await
            .expect("commit");
        assert!(report.removed_dirs.is_empty(), "{report:?}");
        assert!(
            sc.collection()
                .join(format!("{DT}/OLD/extra.json"))
                .is_file()
        );
    }

    #[tokio::test]
    async fn oversize_file_is_refused_before_any_io() {
        let sc = Scratch::new();
        let m = local_mirror(&sc, sc.separate());
        let big = RenderedTree {
            files: [(
                rel(&format!("{DT}/BIG/config.json")),
                vec![0u8; READ_MAX_FILE_BYTES as usize + 1],
            )]
            .into_iter()
            .collect(),
        };
        let err = m.stage(&big).await.expect_err("over the file cap");
        assert!(err.to_string().contains("file cap"), "{err}");
        assert!(!sc.stage_root.exists(), "nothing was written");
    }

    #[tokio::test]
    async fn dot_named_and_gateway_owned_staged_paths_are_refused() {
        let sc = Scratch::new();
        let m = local_mirror(&sc, StagingMode::InPlace);
        for bad in [
            format!("{DT}/.hidden/config.json"),
            format!("{DT}/X/.config.json"),
            format!("{DT}/X/config-mode.json"),
            "ignition/short".to_string(),
        ] {
            let err = m.stage(&tree(&[(&bad, b"{}")])).await.expect_err(&bad);
            assert_eq!(code(&err), "transport_failed", "{bad}: {err}");
        }
        let err = m
            .stage(&tree(&[
                (&format!("{DT}/X"), b"{}"),
                (&format!("{DT}/X/config.json"), b"{}"),
            ]))
            .await
            .expect_err("file and directory");
        assert!(err.to_string().contains("directory of another"), "{err}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlinks_are_never_followed_or_created() {
        // a symlinked unit directory under a staged path
        for separate in [true, false] {
            let sc = Scratch::new();
            let outside = sc.root.path().join("outside");
            put(&outside.join("config.json"), b"{\"secret\":1}");
            std::os::unix::fs::symlink(&outside, sc.collection().join(format!("{DT}/LINK")))
                .expect("symlink");
            let staging = if separate {
                sc.separate()
            } else {
                StagingMode::InPlace
            };
            let m = local_mirror(&sc, staging);
            let staged = m
                .stage(&tree(&[(
                    &format!("{DT}/LINK/config.json"),
                    b"{\"pwn\":1}",
                )]))
                .await
                .expect("stage");
            let before = snapshot(&sc.resources());
            let err = m
                .commit(&staged, &DeleteSet::default())
                .await
                .expect_err("symlinked parent");
            assert!(err.to_string().contains("symlink"), "{err}");
            assert_eq!(snapshot(&sc.resources()), before);
            assert_eq!(
                std::fs::read(outside.join("config.json")).unwrap(),
                b"{\"secret\":1}"
            );

            // a symlink where a staged file goes
            std::os::unix::fs::symlink(
                outside.join("config.json"),
                sc.collection().join(format!("{DT}/KEEP/link.json")),
            )
            .expect("symlink file");
            let staged = m
                .stage(&tree(&[(&format!("{DT}/KEEP/link.json"), b"{\"pwn\":2}")]))
                .await
                .expect("stage");
            let err = m
                .commit(&staged, &DeleteSet::default())
                .await
                .expect_err("symlink target");
            assert!(err.to_string().contains("symlink"), "{err}");
            assert_eq!(
                std::fs::read(outside.join("config.json")).unwrap(),
                b"{\"secret\":1}"
            );

            // a symlink as a delete target is not deleted, its target neither
            let err = m
                .commit(
                    &m.stage(&RenderedTree::default()).await.expect("stage"),
                    &deletes(&[&format!("{DT}/KEEP/link.json")]),
                )
                .await
                .expect_err("symlink delete");
            assert!(err.to_string().contains("symlink"), "{err}");
            assert!(
                sc.collection()
                    .join(format!("{DT}/KEEP/link.json"))
                    .is_symlink()
            );
            assert!(outside.join("config.json").is_file());
        }
    }

    #[tokio::test]
    async fn tampered_staged_copy_is_refused() {
        let sc = Scratch::new();
        let m = local_mirror(&sc, sc.separate());
        let staged = m.stage(&scenario_tree()).await.expect("stage");
        let before = snapshot(&sc.resources());
        std::fs::write(
            sc.stage_root.join(format!("{DT}/NEW/config.json")),
            b"{\"evil\":1}",
        )
        .expect("tamper");
        let err = m
            .commit(&staged, &scenario_deletes())
            .await
            .expect_err("tampered");
        assert!(
            err.to_string().contains("changed after it was staged"),
            "{err}"
        );
        assert_eq!(snapshot(&sc.resources()), before, "nothing was written");
        // and a vanished one
        std::fs::remove_file(sc.stage_root.join(format!("{DT}/NEW/config.json"))).expect("rm");
        let err = m
            .commit(&staged, &scenario_deletes())
            .await
            .expect_err("gone");
        assert!(err.to_string().contains("gone"), "{err}");
        assert_eq!(snapshot(&sc.resources()), before);
    }

    #[tokio::test]
    async fn staged_files_belong_to_their_mirror() {
        let sc = Scratch::new();
        let dev = local_mirror(&sc, StagingMode::InPlace);
        let prod = LocalFsTransport::new(sc.data.clone(), "nodes.master")
            .with_mirror("prod", StagingMode::InPlace)
            .expect("prod");
        let staged = dev.stage(&scenario_tree()).await.expect("stage");
        let err = prod
            .commit(&staged, &DeleteSet::default())
            .await
            .expect_err("other mode");
        assert!(err.to_string().contains("different mirror"), "{err}");
        let separate = local_mirror(&sc, sc.separate());
        let err = separate
            .commit(&staged, &DeleteSet::default())
            .await
            .expect_err("other staging mode");
        assert!(err.to_string().contains("staging mode"), "{err}");
        assert!(!sc.resources().join("prod").exists());
    }

    #[tokio::test]
    async fn a_missing_mode_collection_is_created_and_core_is_never_written() {
        let sc = Scratch::new();
        let err = LocalFsTransport::new(sc.data.clone(), "nodes.master")
            .with_mirror("core", StagingMode::InPlace)
            .expect_err("core");
        assert!(err.contains("never writes"), "{err}");
        for bad in ["local", "", ".x", "a/b"] {
            assert!(
                LocalFsTransport::new(sc.data.clone(), "nodes.master")
                    .with_mirror(bad, StagingMode::InPlace)
                    .is_err(),
                "{bad}"
            );
        }
        let m = LocalFsTransport::new(sc.data.clone(), "nodes.master")
            .with_mirror("qa", StagingMode::InPlace)
            .expect("qa");
        let staged = m.stage(&scenario_tree()).await.expect("stage");
        // deleting from a collection that does not exist yet is refused
        let err = m
            .commit(&staged, &scenario_deletes())
            .await
            .expect_err("nothing to delete");
        assert_eq!(code(&err), "transport_failed");
        assert!(!sc.resources().join("qa").exists());
        m.commit(&staged, &DeleteSet::default())
            .await
            .expect("commit");
        assert!(
            sc.resources()
                .join(format!("qa/{DT}/NEW/config.json"))
                .is_file()
        );
    }

    #[tokio::test]
    async fn a_data_dir_without_config_resources_is_refused() {
        let bare = tempfile::tempdir().expect("bare");
        let m = LocalFsTransport::new(bare.path().to_path_buf(), "nodes.master")
            .with_mirror(MODE, StagingMode::InPlace)
            .expect("mirror");
        let err = m.stage(&scenario_tree()).await.expect_err("bare");
        assert!(err.to_string().contains("gateway data directory"), "{err}");
    }

    #[tokio::test]
    async fn mirror_for_node_builds_local_and_windows_and_refuses_core() {
        let sc = Scratch::new();
        let node = NodeTarget {
            profile: "p".to_string(),
            ssh: None,
            local: Some(Local {
                data_dir: sc.data.display().to_string(),
            }),
        };
        let m = transport::mirror_for_node(
            Path::new("."),
            "targets/dev.yaml",
            "nodes.master",
            &node,
            MODE,
            StagingMode::InPlace,
        )
        .expect("local mirror");
        run_scenario(m.as_ref(), &sc).await;

        let err = transport::mirror_for_node(
            Path::new("."),
            "targets/dev.yaml",
            "nodes.master",
            &node,
            "core",
            StagingMode::InPlace,
        )
        .err()
        .expect("core refused");
        assert_eq!(err.code(), "deploy_tree_invalid");

        // a Windows ssh node builds a mirror (22-13); a bad data dir and the
        // `core` collection are still refused
        let repo = repo_with_known_hosts(KNOWN_HOSTS);
        let mut ssh = ssh_block();
        ssh.os = TargetOs::Windows;
        ssh.data_dir = "C:\\Program Files\\Inductive Automation\\Ignition\\data".to_string();
        let windows_node = |ssh: Ssh| NodeTarget {
            profile: "p".to_string(),
            ssh: Some(ssh),
            local: None,
        };
        let built = transport::mirror_for_node(
            repo.path(),
            "targets/dev.yaml",
            "nodes.master",
            &windows_node(ssh.clone()),
            MODE,
            StagingMode::InPlace,
        );
        assert!(built.is_ok(), "windows mirror builds");
        let err = transport::mirror_for_node(
            repo.path(),
            "targets/dev.yaml",
            "nodes.master",
            &windows_node(ssh.clone()),
            "core",
            StagingMode::InPlace,
        )
        .err()
        .expect("core refused");
        assert_eq!(err.code(), "deploy_tree_invalid");
        // a staging root inside the collection is refused whatever its case
        // (the host is case-insensitive, whatever OS the runner is)
        let resources = "/c:/PROGRAM FILES/inductive automation/ignition/data/config/resources";
        for root in [
            format!("{resources}/staging"),
            "/C:/Program Files/Inductive Automation/Ignition/data/config/resources".to_string(),
            "/C:/Program Files/Inductive Automation".to_string(),
        ] {
            let err = transport::mirror_for_node(
                repo.path(),
                "targets/dev.yaml",
                "nodes.master",
                &windows_node(ssh.clone()),
                MODE,
                StagingMode::Separate { root: root.clone() },
            )
            .err()
            .unwrap_or_else(|| panic!("{root} refused"));
            assert_eq!(err.code(), "deploy_tree_invalid", "{root}");
        }
        ssh.data_dir = "relative".to_string();
        let err = transport::mirror_for_node(
            repo.path(),
            "targets/dev.yaml",
            "nodes.master",
            &windows_node(ssh),
            MODE,
            StagingMode::InPlace,
        )
        .err()
        .expect("relative refused");
        assert_eq!(err.code(), "deploy_tree_invalid");
        assert!(
            err.to_string().contains("nodes.master.ssh.data_dir"),
            "{err}"
        );
    }
}

/// Decision B(i): the default staging mode (22-13).
mod default_staging {
    use super::*;
    use ignition_core::deploy::target::{Local, NodeTarget};
    use ignition_core::deploy::transport::{self, StagingMode};

    fn local_node(data_dir: &Path) -> NodeTarget {
        NodeTarget {
            profile: "p".to_string(),
            ssh: None,
            local: Some(Local {
                data_dir: data_dir.display().to_string(),
            }),
        }
    }

    #[tokio::test]
    async fn a_relative_data_dir_stages_and_mirrors_with_a_relative_repo() {
        // WR-03: `local.data_dir` is relative to the repo and `--repo`
        // defaults to `.`; the staging root and the resources root are
        // validated as absolute paths, so both must be absolutized.
        let node = NodeTarget {
            profile: "p".to_string(),
            ssh: None,
            local: Some(Local {
                data_dir: "relative-data".to_string(),
            }),
        };
        let mode = transport::default_staging_mode(
            Path::new("."),
            "targets/dev.yaml",
            "nodes.master",
            &node,
            "gw",
            "production",
            "run-1",
        )
        .await
        .expect("a relative data_dir stages");
        let StagingMode::Separate { root } = &mode else {
            panic!("a separate staging root");
        };
        assert_eq!(
            Path::new(root),
            std::env::current_dir()
                .expect("cwd")
                .join("relative-data/ign-deploy/gw/production/staging/run-1")
        );
        let built = transport::mirror_for_node(
            Path::new("."),
            "targets/dev.yaml",
            "nodes.master",
            &node,
            "production",
            mode,
        );
        assert!(built.is_ok(), "a relative data_dir builds a mirror");
    }

    #[tokio::test]
    async fn local_nodes_stage_below_the_data_dir_and_inputs_are_checked() {
        let data = tempfile::tempdir().expect("data");
        let node = local_node(data.path());
        let mode = transport::default_staging_mode(
            Path::new("."),
            "targets/dev.yaml",
            "nodes.master",
            &node,
            "gw",
            "production",
            "run-1.a_b",
        )
        .await
        .expect("local staging");
        let StagingMode::Separate { root } = mode else {
            panic!("a separate staging root");
        };
        assert_eq!(
            Path::new(&root),
            data.path()
                .join("ign-deploy/gw/production/staging/run-1.a_b")
        );
        for (gateway, mode, run) in [
            ("../gw", "production", "r"),
            ("gw", "a/b", "r"),
            ("gw", "production", ""),
            ("gw", "production", ".hidden"),
            ("gw", "production", "has space"),
            ("gw", "production", "x/../y"),
        ] {
            let err = transport::default_staging_mode(
                Path::new("."),
                "targets/dev.yaml",
                "nodes.master",
                &node,
                gateway,
                mode,
                run,
            )
            .await
            .expect_err(&format!("{gateway}/{mode}/{run}"));
            assert_eq!(err.code(), "deploy_tree_invalid");
        }
        // a node with no host access has no staging
        let neither = NodeTarget {
            profile: "p".to_string(),
            ssh: None,
            local: None,
        };
        assert!(
            transport::default_staging_mode(
                Path::new("."),
                "targets/dev.yaml",
                "nodes.master",
                &neither,
                "gw",
                "production",
                "r",
            )
            .await
            .is_err()
        );
    }
}

/// The sftp write half through a real `sftp-server` (`sftp -D`).
#[cfg(unix)]
mod sftp_mirror {
    use super::mirror::{
        DT, MODE, Scratch, assert_nothing_foreign, assert_refusals_write_nothing, deletes, put,
        run_scenario, scenario_deletes, scenario_tree, snapshot, tree,
    };
    use super::*;
    use ignition_core::deploy::transport::sftp::{SftpTransport, posix_root};
    use ignition_core::deploy::transport::{DeleteSet, StagingMode, TreeMirror};
    use ignition_core::deploy::tree::RenderedTree;

    /// Runs the real `sftp` and records every batch it was fed; a hook may act
    /// on a batch just before it runs.
    type BatchHook = Box<dyn Fn(&str) + Send + Sync>;

    #[derive(Default)]
    struct Tap {
        batches: Mutex<Vec<String>>,
        before: Mutex<Option<BatchHook>>,
    }

    #[async_trait::async_trait]
    impl ProcessRunner for Tap {
        async fn run(
            &self,
            program: &OsStr,
            args: &[OsString],
            stdin: Option<Vec<u8>>,
            timeout: Duration,
        ) -> ProcessOutput {
            let text = stdin
                .as_deref()
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .unwrap_or_default();
            self.batches.lock().expect("batches").push(text.clone());
            if let Some(hook) = self.before.lock().expect("hook").as_ref() {
                hook(&text);
            }
            TokioProcessRunner.run(program, args, stdin, timeout).await
        }
    }

    impl Tap {
        fn lines(&self) -> Vec<Vec<String>> {
            self.batches
                .lock()
                .expect("batches")
                .iter()
                .map(|b| b.lines().map(str::to_string).collect())
                .collect()
        }
    }

    fn sftp_mirror(
        sc: &Scratch,
        staging: StagingMode,
        tap: Arc<dyn ProcessRunner>,
    ) -> SftpTransport {
        let shell = RemoteShell::local_sftp_server(&sftp_server(), tap, Duration::from_secs(60));
        let root = posix_root(&sc.data.display().to_string()).expect("posix root");
        SftpTransport::new(shell, root, "nodes.master")
            .with_mirror(MODE, staging)
            .expect("mirror")
    }

    #[tokio::test]
    async fn sftp_mirror_separate_and_inplace() {
        for separate in [true, false] {
            let local_sc = Scratch::new();
            let sftp_sc = Scratch::new();
            let tap = Arc::new(Tap::default());
            let (staging_l, staging_s) = if separate {
                (local_sc.separate(), sftp_sc.separate())
            } else {
                (StagingMode::InPlace, StagingMode::InPlace)
            };
            let local = super::mirror::local_mirror(&local_sc, staging_l);
            let (want_report, want) = run_scenario(&local, &local_sc).await;
            let sftp = sftp_mirror(&sftp_sc, staging_s, tap.clone());
            let (got_report, got) = run_scenario(&sftp, &sftp_sc).await;
            assert_eq!(
                got, want,
                "separate={separate}: same collection bytes as the local mirror"
            );
            assert_eq!(got_report, want_report);
            assert_nothing_foreign(&got);

            // the exact command sequence, for the C7a comparison (22-12)
            let root = sftp_sc.data.display().to_string();
            let stage_root = sftp_sc.stage_root.display().to_string();
            println!("--- sftp command sequence, separate={separate} ---");
            for (n, batch) in tap.lines().iter().enumerate() {
                for line in batch {
                    let shown = line
                        .replace(&stage_root, "<STAGE>")
                        .replace(&root, "<DATA>");
                    let shown = match shown.find("ign-stage-") {
                        Some(at) => {
                            let start = shown[..at].rfind('"').map_or(0, |i| i + 1);
                            let end = shown[at..].find('/').map_or(shown.len(), |i| at + i);
                            format!("{}<LOCAL>{}", &shown[..start], &shown[end..])
                        }
                        None => shown,
                    };
                    println!("session {}: {shown}", n + 1);
                }
            }
        }
    }

    #[tokio::test]
    async fn sftp_mirror_never_suppresses_errors() {
        // no batch line starts with `-`
        let sc = Scratch::new();
        let tap = Arc::new(Tap::default());
        let m = sftp_mirror(&sc, sc.separate(), tap.clone());
        run_scenario(&m, &sc).await;
        let all = tap.lines();
        assert!(!all.is_empty());
        for line in all.iter().flatten() {
            assert!(!line.starts_with('-'), "{line}");
            assert!(!line.starts_with("-@"), "{line}");
            assert!(!line.starts_with('!'), "{line}");
            assert!(!line.contains("get -R"), "{line}");
        }
        // an `rm` of a file that vanished is transport_failed
        let sc = Scratch::new();
        let tap = Arc::new(Tap::default());
        let victim = sc.collection().join(format!("{DT}/OLD/config.json"));
        let hook_victim = victim.clone();
        *tap.before.lock().expect("hook") = Some(Box::new(move |batch: &str| {
            if batch.contains("rm \"") {
                let _ = std::fs::remove_file(&hook_victim);
            }
        }));
        let m = sftp_mirror(&sc, StagingMode::InPlace, tap.clone());
        let staged = m.stage(&scenario_tree()).await.expect("stage");
        let err = m
            .commit(&staged, &scenario_deletes())
            .await
            .expect_err("vanished file");
        assert_eq!(err.code(), "transport_failed", "{err}");
        assert!(!victim.exists());
    }

    #[tokio::test]
    async fn sftp_mirror_existence_first_mkdir() {
        let sc = Scratch::new();
        let tap = Arc::new(Tap::default());
        let m = sftp_mirror(&sc, StagingMode::InPlace, tap.clone());
        let rendered = tree(&[
            (&format!("{DT}/UPD/config.json"), b"{\"v\":2}"),
            (&format!("{DT}/NEW/config.json"), b"{\"new\":1}"),
            ("ignition/brand-new-type/X/config.json", b"{}"),
        ]);
        let staged = m.stage(&rendered).await.expect("stage");
        assert!(tap.lines().is_empty(), "inplace staging runs no session");
        m.commit(&staged, &DeleteSet::default())
            .await
            .expect("commit");

        let data = posix_root(&sc.data.display().to_string()).expect("root");
        let dev = format!("{data}/config/resources/{MODE}");
        let batches = tap.lines();
        // (session, line within the session, path); mkdirs share the batch of
        // the writes, parents first
        let mut mkdirs: Vec<(usize, usize, String)> = Vec::new();
        for (index, batch) in batches.iter().enumerate() {
            for (at, line) in batch.iter().enumerate() {
                if let Some(rest) = line.strip_prefix("mkdir \"") {
                    mkdirs.push((index, at, rest.trim_end_matches('"').to_string()));
                }
            }
        }
        let made: std::collections::BTreeSet<&str> =
            mkdirs.iter().map(|(_, _, path)| path.as_str()).collect();
        let want = [
            format!("{dev}/ignition/brand-new-type"),
            format!("{dev}/ignition/brand-new-type/X"),
            format!("{dev}/{DT}/NEW"),
        ];
        assert_eq!(
            made,
            want.iter().map(String::as_str).collect(),
            "mkdir only for levels a listing showed missing (never `ignition` or `{DT}`)"
        );
        assert_eq!(mkdirs.len(), 3, "each exactly once: {mkdirs:?}");
        // every mkdir's parent was listed in an earlier session or created earlier
        for (index, at, path) in &mkdirs {
            let parent = path.rsplit_once('/').expect("parent").0;
            let listed_before = batches[..*index]
                .iter()
                .flatten()
                .any(|line| line == &format!("@ls -la \"{parent}\""));
            let made_before = mkdirs
                .iter()
                .any(|(i, a, other)| other == parent && (i, a) < (index, at));
            assert!(
                listed_before || made_before,
                "{path}: parent {parent} was never established"
            );
        }
        assert_eq!(
            std::fs::read(sc.collection().join(format!("{DT}/NEW/config.json"))).unwrap(),
            b"{\"new\":1}"
        );
    }
    #[tokio::test]
    async fn sftp_mirror_delete_set_refusals_write_nothing() {
        for separate in [true, false] {
            let sc = Scratch::new();
            let staging = if separate {
                sc.separate()
            } else {
                StagingMode::InPlace
            };
            let m = sftp_mirror(&sc, staging, Arc::new(TokioProcessRunner));
            assert_refusals_write_nothing(&m, &sc).await;
        }
    }

    #[tokio::test]
    async fn sftp_mirror_refuses_a_staging_root_inside_the_collection() {
        let sc = Scratch::new();
        let shell =
            RemoteShell::local_sftp_server(&sftp_server(), runner(), Duration::from_secs(60));
        let root = posix_root(&sc.data.display().to_string()).expect("root");
        for bad in [
            format!("{root}/config/resources/stage"),
            format!("{root}/config/resources/dev/stage"),
            format!("{root}/config/resources"),
            root.clone(),
            "relative/stage".to_string(),
            "/onlyone".to_string(),
        ] {
            let why = SftpTransport::new(
                RemoteShell::local_sftp_server(&sftp_server(), runner(), Duration::from_secs(60)),
                root.clone(),
                "nodes.master",
            )
            .with_mirror(MODE, StagingMode::Separate { root: bad.clone() })
            .err()
            .unwrap_or_else(|| panic!("{bad} must be refused"));
            assert!(why.contains("staging root"), "{why}");
        }
        drop(shell);
    }

    #[tokio::test]
    async fn sftp_mirror_symlinks_are_refused_and_never_followed() {
        for separate in [true, false] {
            let sc = Scratch::new();
            let outside = sc.root.path().join("outside");
            put(&outside.join("config.json"), b"{\"secret\":1}");
            std::os::unix::fs::symlink(&outside, sc.collection().join(format!("{DT}/LINK")))
                .expect("symlink");
            std::os::unix::fs::symlink(
                outside.join("config.json"),
                sc.collection().join(format!("{DT}/KEEP/link.json")),
            )
            .expect("symlink file");
            let staging = if separate {
                sc.separate()
            } else {
                StagingMode::InPlace
            };
            let m = sftp_mirror(&sc, staging, Arc::new(TokioProcessRunner));
            let before = snapshot(&sc.resources());
            for (name, rendered, set, needle) in [
                (
                    "symlinked parent",
                    tree(&[(&format!("{DT}/LINK/config.json"), b"{\"pwn\":1}")]),
                    DeleteSet::default(),
                    "symlink",
                ),
                (
                    "symlink as write target",
                    tree(&[(&format!("{DT}/KEEP/link.json"), b"{\"pwn\":2}")]),
                    DeleteSet::default(),
                    "symlink",
                ),
                (
                    "symlink as delete target",
                    RenderedTree::default(),
                    deletes(&[&format!("{DT}/KEEP/link.json")]),
                    "symlink",
                ),
            ] {
                let staged = m.stage(&rendered).await.expect("stage");
                let err = m.commit(&staged, &set).await.expect_err(name);
                assert!(err.to_string().contains(needle), "{name}: {err}");
                assert_eq!(snapshot(&sc.resources()), before, "{name}");
                assert_eq!(
                    std::fs::read(outside.join("config.json")).unwrap(),
                    b"{\"secret\":1}",
                    "{name}"
                );
            }
        }
    }

    #[tokio::test]
    async fn sftp_mirror_tampered_staged_copy_is_refused() {
        let sc = Scratch::new();
        let m = sftp_mirror(&sc, sc.separate(), Arc::new(TokioProcessRunner));
        let staged = m.stage(&scenario_tree()).await.expect("stage");
        let before = snapshot(&sc.resources());
        // wrong size on the host
        std::fs::write(sc.stage_root.join(format!("{DT}/NEW/config.json")), b"{}").expect("tamper");
        let err = m
            .commit(&staged, &scenario_deletes())
            .await
            .expect_err("wrong size");
        assert!(err.to_string().contains("wrong size"), "{err}");
        assert_eq!(snapshot(&sc.resources()), before);
        // missing on the host
        std::fs::remove_file(sc.stage_root.join(format!("{DT}/NEW/config.json"))).expect("rm");
        let err = m
            .commit(&staged, &scenario_deletes())
            .await
            .expect_err("missing");
        assert!(err.to_string().contains("missing"), "{err}");
        assert_eq!(snapshot(&sc.resources()), before);
    }

    /// No batch line of a tampered run renames anything or removes anything in
    /// the collection; the only `rm` allowed is the cleanup of the staged
    /// copies inside the staging root.
    fn assert_no_collection_mutation(tap: &Tap, sc: &Scratch) {
        let stage = format!("\"{}/", sc.stage_root.display());
        for line in tap.lines().iter().flatten() {
            assert!(!line.starts_with("rename "), "a rename ran: {line}");
            assert!(!line.starts_with("rmdir "), "an rmdir ran: {line}");
            if let Some(rest) = line.strip_prefix("rm ") {
                assert!(
                    rest.starts_with(&stage),
                    "an rm left the staging root: {line}"
                );
            }
        }
    }

    const NEW_HONEST: &[u8] = b"{\"new\":1}";
    const NEW_EVIL: &[u8] = b"{\"new\":9}";

    /// F6: a staged copy replaced on the host by one of the SAME size but
    /// different bytes is refused before any rename or delete.
    #[tokio::test]
    async fn sftp_mirror_same_size_tampered_staged_copy_is_refused() {
        let sc = Scratch::new();
        let tap = Arc::new(Tap::default());
        let m = sftp_mirror(&sc, sc.separate(), tap.clone());
        let staged = m.stage(&scenario_tree()).await.expect("stage");
        let before = snapshot(&sc.resources());
        let victim = sc.stage_root.join(format!("{DT}/NEW/config.json"));
        assert_eq!(std::fs::read(&victim).unwrap(), NEW_HONEST);
        assert_eq!(NEW_HONEST.len(), NEW_EVIL.len());
        std::fs::write(&victim, NEW_EVIL).expect("tamper");

        let err = m
            .commit(&staged, &scenario_deletes())
            .await
            .expect_err("same-size tamper");
        assert_eq!(err.code(), "transport_failed", "{err}");
        let text = err.to_string();
        assert!(text.contains("SHA-256"), "{text}");
        assert!(text.contains("NEW/config.json"), "{text}");
        assert_eq!(snapshot(&sc.resources()), before, "nothing was committed");
        assert_no_collection_mutation(&tap, &sc);
        // the staged copies were discarded, and the report says so
        assert!(text.contains("removed"), "{text}");
        assert!(
            !snapshot(&sc.stage_root).values().any(Option::is_some),
            "staged copies left in the staging root"
        );

        // an untampered stage commits
        let staged = m.stage(&scenario_tree()).await.expect("restage");
        m.commit(&staged, &scenario_deletes())
            .await
            .expect("an untampered copy commits");
        assert_eq!(
            std::fs::read(sc.collection().join(format!("{DT}/NEW/config.json"))).unwrap(),
            NEW_HONEST
        );
    }

    /// F6: the swap lands AFTER the size survey, just before the read-back.
    #[tokio::test]
    async fn sftp_mirror_tamper_after_the_size_survey_is_refused() {
        let sc = Scratch::new();
        let tap = Arc::new(Tap::default());
        let victim = sc.stage_root.join(format!("{DT}/NEW/config.json"));
        let hook_victim = victim.clone();
        *tap.before.lock().expect("hook") = Some(Box::new(move |batch: &str| {
            if batch.contains("get \"") {
                std::fs::write(&hook_victim, NEW_EVIL).expect("swap");
            }
        }));
        let m = sftp_mirror(&sc, sc.separate(), tap.clone());
        let staged = m.stage(&scenario_tree()).await.expect("stage");
        let before = snapshot(&sc.resources());
        let err = m
            .commit(&staged, &scenario_deletes())
            .await
            .expect_err("swapped after the survey");
        assert_eq!(err.code(), "transport_failed", "{err}");
        assert!(err.to_string().contains("SHA-256"), "{err}");
        assert_eq!(snapshot(&sc.resources()), before);
        assert_no_collection_mutation(&tap, &sc);
    }

    /// F6 follow-up: sftp has no atomic verify-then-rename. A swap that lands
    /// AFTER the pre-rename read-back (just before the rename batch runs) is
    /// detected by the post-commit read-back of the final paths, and no delete
    /// runs.
    #[tokio::test]
    async fn sftp_mirror_tamper_between_pre_verify_and_rename_is_caught_after_commit() {
        let sc = Scratch::new();
        let tap = Arc::new(Tap::default());
        let victim = sc.stage_root.join(format!("{DT}/NEW/config.json"));
        let hook_victim = victim.clone();
        *tap.before.lock().expect("hook") = Some(Box::new(move |batch: &str| {
            if batch.contains("rename \"") {
                std::fs::write(&hook_victim, NEW_EVIL).expect("swap");
            }
        }));
        let m = sftp_mirror(&sc, sc.separate(), tap.clone());
        let staged = m.stage(&scenario_tree()).await.expect("stage");
        let err = m
            .commit(&staged, &scenario_deletes())
            .await
            .expect_err("swapped between verify and rename");
        assert_eq!(err.code(), "transport_failed", "{err}");
        let text = err.to_string();
        assert!(
            text.contains("content changed on the host during commit"),
            "{text}"
        );
        assert!(text.contains("NEW/config.json"), "{text}");
        assert!(text.contains("no delete was run"), "{text}");
        // the renames did happen (that is what the post-commit check detects)...
        assert_eq!(
            std::fs::read(sc.collection().join(format!("{DT}/NEW/config.json"))).unwrap(),
            NEW_EVIL
        );
        // ...but no delete did
        for gone in ["OLD/config.json", "OLD/resource.json"] {
            assert!(
                sc.collection().join(format!("{DT}/{gone}")).is_file(),
                "{gone} was deleted after a failed post-commit verification"
            );
        }
        for line in tap.lines().iter().flatten() {
            assert!(!line.starts_with("rm "), "a delete ran: {line}");
            assert!(!line.starts_with("rmdir "), "an rmdir ran: {line}");
        }
        // verification and the renames are adjacent sessions: the session
        // after the pre-rename `get`s is the one that renames
        let batches = tap.lines();
        let first_get = batches
            .iter()
            .position(|b| b.iter().any(|l| l.starts_with("get \"")))
            .expect("a read-back session");
        assert!(
            batches[first_get + 1]
                .iter()
                .any(|l| l.starts_with("rename \"")),
            "something ran between the pre-rename verification and the renames: {:?}",
            batches[first_get + 1]
        );
    }

    /// F6: a staged copy swapped for a far larger file is bounded by the same
    /// listing-sized watchdog and caps as the read side and is refused.
    #[tokio::test]
    async fn sftp_mirror_oversize_swap_is_refused_before_any_rename() {
        let sc = Scratch::new();
        let tap = Arc::new(Tap::default());
        let victim = sc.stage_root.join(format!("{DT}/NEW/config.json"));
        let hook_victim = victim.clone();
        *tap.before.lock().expect("hook") = Some(Box::new(move |batch: &str| {
            if batch.contains("get \"") {
                std::fs::write(&hook_victim, vec![b'x'; 24 * 1024 * 1024]).expect("swap");
            }
        }));
        let m = sftp_mirror(&sc, sc.separate(), tap.clone());
        let staged = m.stage(&scenario_tree()).await.expect("stage");
        let before = snapshot(&sc.resources());
        let err = m
            .commit(&staged, &scenario_deletes())
            .await
            .expect_err("oversize swap");
        assert_eq!(err.code(), "transport_failed", "{err}");
        assert_eq!(snapshot(&sc.resources()), before);
        assert_no_collection_mutation(&tap, &sc);
    }

    /// F6: InPlace writes the final path directly, so the read-back is after
    /// the write; a mismatch stops before any delete and is reported honestly.
    #[tokio::test]
    async fn sftp_mirror_inplace_verifies_after_the_write_and_stops_before_deletes() {
        let sc = Scratch::new();
        let tap = Arc::new(Tap::default());
        let target = sc.collection().join(format!("{DT}/NEW/resource.json"));
        let hook_target = target.clone();
        *tap.before.lock().expect("hook") = Some(Box::new(move |batch: &str| {
            if batch.contains("get \"") {
                std::fs::write(&hook_target, NEW_EVIL).expect("swap");
            }
        }));
        let m = sftp_mirror(&sc, StagingMode::InPlace, tap.clone());
        let staged = m.stage(&scenario_tree()).await.expect("stage");
        let err = m
            .commit(&staged, &scenario_deletes())
            .await
            .expect_err("written file differs");
        assert_eq!(err.code(), "transport_failed", "{err}");
        let text = err.to_string();
        assert!(text.contains("SHA-256"), "{text}");
        assert!(text.contains("NEW/resource.json"), "{text}");
        assert!(text.contains("not rolled back"), "{text}");
        assert!(text.contains("no delete"), "{text}");
        for gone in ["OLD/config.json", "OLD/resource.json"] {
            assert!(
                sc.collection().join(format!("{DT}/{gone}")).is_file(),
                "{gone} was deleted after a failed verification"
            );
        }
        for line in tap.lines().iter().flatten() {
            assert!(!line.starts_with("rm "), "a delete ran: {line}");
        }

        // an honest InPlace commit reads back and still passes
        let sc = Scratch::new();
        let m = sftp_mirror(&sc, StagingMode::InPlace, Arc::new(TokioProcessRunner));
        run_scenario(&m, &sc).await;
    }

    #[tokio::test]
    async fn sftp_mirror_hostile_but_legal_names_are_inert() {
        let sc = Scratch::new();
        let name = "it's $(touch IGN_PWNED_MARKER) `id`; echo !x";
        let m = sftp_mirror(&sc, sc.separate(), Arc::new(TokioProcessRunner));
        let path = format!("{DT}/{name}/config.json");
        let staged = m.stage(&tree(&[(&path, b"{}")])).await.expect("stage");
        m.commit(&staged, &DeleteSet::default())
            .await
            .expect("commit");
        assert!(sc.collection().join(&path).is_file());
        let staged = m.stage(&RenderedTree::default()).await.expect("stage");
        let report = m.commit(&staged, &deletes(&[&path])).await.expect("delete");
        assert_eq!(report.removed_dirs, vec![format!("{DT}/{name}")]);
        for dir in [sc.root.path(), &sc.data, Path::new(".")] {
            assert!(!dir.join("IGN_PWNED_MARKER").exists());
        }
        // names the sftp client would glob or escape are refused up front
        for bad in ["odd[1]", "q\"uote", "g*lob"] {
            let err = m
                .stage(&tree(&[(&format!("{DT}/{bad}/config.json"), b"{}")]))
                .await;
            let staged = match err {
                Ok(staged) => staged,
                Err(_) => continue,
            };
            let err = m
                .commit(&staged, &DeleteSet::default())
                .await
                .expect_err(bad);
            assert_eq!(err.code(), "transport_failed", "{bad}");
        }
    }
}

/// No Phase 22 verb or surface can reach the write half (T-22-30): the three
/// sources that dispatch verbs name none of its identifiers. `deploy_plan`,
/// `deploy_pull` and `deploy_verify` live in `actions/deploy.rs`, receive a
/// `Box<dyn TreeReader>` from `for_node`, and the CLI and MCP surfaces only
/// call those actions.
#[test]
fn write_half_unreachable() {
    let sources: [(&str, &str); 3] = [
        (
            "crates/ignition-core/src/actions/deploy.rs",
            include_str!("../src/actions/deploy.rs"),
        ),
        (
            "crates/ignition-cli/src/main.rs",
            include_str!("../../ignition-cli/src/main.rs"),
        ),
        (
            "crates/ignition-cli/src/mcp.rs",
            include_str!("../../ignition-cli/src/mcp.rs"),
        ),
    ];
    let forbidden = [
        "TreeMirror",
        "mirror_for_node",
        "StagingMode",
        ".stage(",
        ".commit(",
    ];
    for (file, text) in sources {
        assert!(text.len() > 1000, "{file} was not read");
        for identifier in forbidden {
            assert!(
                !text.contains(identifier),
                "{file} names `{identifier}`: no Phase 22 verb or surface may reach the transport write half"
            );
        }
    }
}
