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

    fn sftp_reader_with(data_dir: &Path, runner: Arc<dyn ProcessRunner>) -> SftpTransport {
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
        reader
            .read_ledger("gw", "dev")
            .await
            .expect_err("glob data dir");
        assert!(
            recording.calls.lock().expect("calls").is_empty(),
            "no process was spawned"
        );
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
    async fn for_node_builds_sftp_for_linux_and_refuses_windows() {
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

        let (windows, _) = load(ssh_yaml(
            "windows",
            "C:\\Program Files\\Inductive Automation\\Ignition\\data",
        ));
        let err = windows.err().expect("windows refused");
        assert_eq!(err.code(), "deploy_tree_invalid");
        assert!(err.to_string().contains("nodes.master.ssh.os"), "{err}");

        let (bad_dir, _) = load(ssh_yaml("linux", "relative/data"));
        let err = bad_dir.err().expect("relative refused");
        assert_eq!(err.code(), "deploy_tree_invalid");
        assert!(
            err.to_string().contains("nodes.master.ssh.data_dir"),
            "{err}"
        );
    }
}
