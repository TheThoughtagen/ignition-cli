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

/// The null device `ssh -G` reports for `GlobalKnownHostsFile`: the product
/// passes `NUL` on a Windows runner and `/dev/null` elsewhere.
fn reported_null_device(windows_runner: bool) -> &'static str {
    if windows_runner { "NUL" } else { "/dev/null" }
}

/// How `ssh -G` reports a file option the product passed through
/// `hardened_options`: the path text, with every `\` written `/` on a
/// Windows runner (the product converts there; ssh echoes the value). A path
/// given with `-i` is not converted, so `identityfile` keeps the raw text.
fn reported_path_text(text: &str, windows_runner: bool) -> String {
    if windows_runner {
        text.replace('\\', "/")
    } else {
        text.to_string()
    }
}

fn reported_path(path: &Path) -> String {
    reported_path_text(&path.display().to_string(), cfg!(windows))
}

#[test]
fn reported_spellings_match_the_windows_runner_observation() {
    // Lines `ssh -G` printed on windows-latest (CI run 38836275411) for the
    // paths the tests built there.
    assert_eq!(reported_null_device(true), "NUL");
    assert_eq!(
        reported_path_text(
            r"C:\Users\RUNNER~1\AppData\Local\Temp\ign-ssh-8x7x7N\known_hosts",
            true
        ),
        "C:/Users/RUNNER~1/AppData/Local/Temp/ign-ssh-8x7x7N/known_hosts"
    );
    assert_eq!(
        reported_path_text(
            r"C:\Users\RUNNER~1\AppData\Local\Temp\ign odd %h RpQF5u\known hosts",
            true
        ),
        "C:/Users/RUNNER~1/AppData/Local/Temp/ign odd %h RpQF5u/known hosts"
    );
    // Unix runners report the path text as given.
    assert_eq!(reported_null_device(false), "/dev/null");
    assert_eq!(
        reported_path_text("/tmp/ign odd %h x/known hosts", false),
        "/tmp/ign odd %h x/known hosts"
    );
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
        ],
    );
    assert_reports(
        &config,
        &[&format!(
            "globalknownhostsfile {}",
            reported_null_device(cfg!(windows))
        )],
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
    assert_reports(
        &config,
        &[&format!(
            "userknownhostsfile {}",
            reported_path(shell.known_hosts_copy())
        )],
    );
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
        &[&format!(
            "userknownhostsfile {}",
            reported_path(&known_hosts)
        )],
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
        Scope::from_manifest(&declared.manifest, MODE, &declared.excluded_tag_providers())
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
            assert_eq!(
                reader
                    .list_tag_providers(collection)
                    .await
                    .expect("sftp providers"),
                local_reader(fx.data_dir())
                    .list_tag_providers(collection)
                    .await
                    .expect("local providers"),
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
        // Held for its drop. Only the unix-gated tests read it.
        #[cfg_attr(not(unix), allow(dead_code))]
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
            gateway_name: None,
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
            gateway_name: None,
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
            gateway_name: None,
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
            gateway_name: None,
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
            gateway_name: None,
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

/// The sftp write half through a real `sftp-server` (`sftp -D`) and the POSIX
/// script host over a local `sh` (23-06, D-01): uploads travel through sftp,
/// the whole commit is ONE script call, and nothing is renamed file by file.
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

    /// Runs the real `sftp` (and the local `sh` for the exec seam) and records
    /// every batch and every remote command it was fed; a hook may act on a
    /// batch, or on a remote command, just before it runs.
    type BatchHook = Box<dyn Fn(&str) + Send + Sync>;

    #[derive(Default)]
    struct Tap {
        batches: Mutex<Vec<String>>,
        execs: Mutex<Vec<String>>,
        before: Mutex<Option<BatchHook>>,
        before_exec: Mutex<Option<BatchHook>>,
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
            if program == "sh" {
                let command = args
                    .last()
                    .map(|a| a.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.execs.lock().expect("execs").push(command.clone());
                if let Some(hook) = self.before_exec.lock().expect("hook").as_ref() {
                    hook(&command);
                }
                return TokioProcessRunner.run(program, args, stdin, timeout).await;
            }
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

        fn execs(&self) -> Vec<String> {
            self.execs.lock().expect("execs").clone()
        }
    }

    /// The account home of the scratch host is the scratch root, so the
    /// control directories of the script calls land beside `data` and `stage`.
    fn sftp_mirror(
        sc: &Scratch,
        staging: StagingMode,
        tap: Arc<dyn ProcessRunner>,
    ) -> SftpTransport {
        let shell = RemoteShell::local_sftp_server(&sftp_server(), tap, Duration::from_secs(60))
            .local_exec();
        let root = posix_root(&sc.data.display().to_string()).expect("posix root");
        let home = posix_root(&sc.root.path().display().to_string()).expect("home");
        SftpTransport::new(shell, root, "nodes.master")
            .with_home(&home)
            .with_mirror(MODE, staging)
            .expect("mirror")
    }

    /// The control directories a script call leaves behind: none.
    fn assert_no_control_dirs(sc: &Scratch) {
        let leftovers: Vec<String> = std::fs::read_dir(sc.root.path())
            .expect("root")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(".ign-ctl-"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "control dirs left behind: {leftovers:?}"
        );
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
            assert_no_control_dirs(&sftp_sc);
            assert_eq!(tap.execs().len(), 1, "one script call for the commit");
        }
    }

    #[tokio::test]
    async fn sftp_mirror_never_suppresses_errors() {
        // no sftp batch line starts with `-`, `@-` or `!`
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
        // a file that vanished before the script ran is transport_failed, and
        // nothing else is changed
        for separate in [true, false] {
            let sc = Scratch::new();
            let tap = Arc::new(Tap::default());
            let victim = sc.collection().join(format!("{DT}/OLD/config.json"));
            let hook_victim = victim.clone();
            *tap.before_exec.lock().expect("hook") = Some(Box::new(move |_command: &str| {
                let _ = std::fs::remove_file(&hook_victim);
            }));
            let staging = if separate {
                sc.separate()
            } else {
                StagingMode::InPlace
            };
            let m = sftp_mirror(&sc, staging, tap.clone());
            let staged = m.stage(&scenario_tree()).await.expect("stage");
            let mut expected = snapshot(&sc.resources());
            let err = m
                .commit(&staged, &scenario_deletes())
                .await
                .expect_err("vanished file");
            assert_eq!(err.code(), "transport_failed", "{err}");
            assert!(!victim.exists());
            expected.remove(&format!("{MODE}/{DT}/OLD/config.json"));
            assert_eq!(
                snapshot(&sc.resources()),
                expected,
                "nothing but the vanished file differs"
            );
        }
    }

    /// Was `sftp_mirror_existence_first_mkdir`: the script, not an sftp
    /// `mkdir` per level, creates the directories, and only the ones that are
    /// missing, 0755. No `mkdir` line touches the collection.
    #[tokio::test]
    async fn sftp_mirror_script_creates_only_missing_directories() {
        use std::os::unix::fs::PermissionsExt;
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
        for line in tap.lines().iter().flatten() {
            if let Some(rest) = line.strip_prefix("mkdir \"") {
                assert!(
                    !rest.starts_with(&format!("{data}/config/resources")),
                    "an sftp mkdir touched the collection: {line}"
                );
            }
        }
        assert_eq!(tap.execs().len(), 1);
        assert_eq!(
            std::fs::read(sc.collection().join(format!("{DT}/NEW/config.json"))).unwrap(),
            b"{\"new\":1}"
        );
        let mode = |rel: &str| {
            std::fs::metadata(sc.collection().join(rel))
                .expect("meta")
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(mode("ignition/brand-new-type"), 0o755);
        assert_eq!(mode("ignition/brand-new-type/X"), 0o755);
        assert_eq!(mode("ignition/brand-new-type/X/config.json"), 0o644);
        assert_no_control_dirs(&sc);
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

    /// A staged copy wrong-sized or missing on the host is refused before any
    /// move (the script checks every staged copy's SHA-256 first).
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
        assert_eq!(err.code(), "transport_failed", "{err}");
        assert!(err.to_string().contains("NEW/config.json"), "{err}");
        assert!(err.to_string().contains("SHA-256"), "{err}");
        assert_eq!(snapshot(&sc.resources()), before);
        // missing on the host (the refused run discarded the staged copies)
        let err = m
            .commit(&staged, &scenario_deletes())
            .await
            .expect_err("missing");
        assert_eq!(err.code(), "transport_failed", "{err}");
        assert!(err.to_string().contains("SHA-256"), "{err}");
        assert_eq!(snapshot(&sc.resources()), before);
    }

    /// No sftp batch line of a tampered run renames anything, or removes
    /// anything in the collection; the only `rm` allowed is inside a control
    /// directory the script call made, and the staged copies are removed by
    /// the script itself.
    fn assert_no_collection_mutation(tap: &Tap, sc: &Scratch) {
        let data = format!("\"{}", sc.data.display());
        for line in tap.lines().iter().flatten() {
            assert!(!line.starts_with("rename "), "a rename ran: {line}");
            if let Some(rest) = line.strip_prefix("rm ") {
                assert!(
                    !rest.starts_with(&data),
                    "an sftp rm touched the data directory: {line}"
                );
            }
            if let Some(rest) = line.strip_prefix("rmdir ") {
                assert!(
                    !rest.starts_with(&data),
                    "an sftp rmdir touched the data directory: {line}"
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

    /// Was `sftp_mirror_tamper_after_the_size_survey_is_refused`: the swap
    /// lands just before the script call runs (the latest a runner can act);
    /// the script's SHA-256 check refuses it before any move.
    #[tokio::test]
    async fn sftp_mirror_tamper_before_the_script_runs_is_refused() {
        let sc = Scratch::new();
        let tap = Arc::new(Tap::default());
        let victim = sc.stage_root.join(format!("{DT}/NEW/config.json"));
        let hook_victim = victim.clone();
        *tap.before_exec.lock().expect("hook") = Some(Box::new(move |_command: &str| {
            std::fs::write(&hook_victim, NEW_EVIL).expect("swap");
        }));
        let m = sftp_mirror(&sc, sc.separate(), tap.clone());
        let staged = m.stage(&scenario_tree()).await.expect("stage");
        let before = snapshot(&sc.resources());
        let err = m
            .commit(&staged, &scenario_deletes())
            .await
            .expect_err("swapped before the script ran");
        assert_eq!(err.code(), "transport_failed", "{err}");
        assert!(err.to_string().contains("SHA-256"), "{err}");
        assert_eq!(snapshot(&sc.resources()), before);
        assert_no_collection_mutation(&tap, &sc);
    }

    /// Was `sftp_mirror_tamper_between_pre_verify_and_rename_is_caught_after_commit`.
    /// In the one-call model no runner step sits between the script's own
    /// verification and its moves, so that scenario cannot be staged; what
    /// remains reachable, and was not covered before, is a staged copy swapped
    /// for a symlink to a file outside the staging root: the script refuses it
    /// before any move and nothing is read through the link.
    #[tokio::test]
    async fn sftp_mirror_staged_copy_replaced_by_a_symlink_is_refused() {
        let sc = Scratch::new();
        let tap = Arc::new(Tap::default());
        let outside = sc.root.path().join("outside.json");
        std::fs::write(&outside, NEW_HONEST).expect("outside");
        let victim = sc.stage_root.join(format!("{DT}/NEW/config.json"));
        let hook_victim = victim.clone();
        let hook_outside = outside.clone();
        *tap.before_exec.lock().expect("hook") = Some(Box::new(move |_command: &str| {
            std::fs::remove_file(&hook_victim).expect("remove");
            std::os::unix::fs::symlink(&hook_outside, &hook_victim).expect("symlink");
        }));
        let m = sftp_mirror(&sc, sc.separate(), tap.clone());
        let staged = m.stage(&scenario_tree()).await.expect("stage");
        let before = snapshot(&sc.resources());
        let err = m
            .commit(&staged, &scenario_deletes())
            .await
            .expect_err("a symlinked staged copy");
        assert_eq!(err.code(), "transport_failed", "{err}");
        assert!(err.to_string().contains("NEW/config.json"), "{err}");
        assert_eq!(snapshot(&sc.resources()), before);
        assert_eq!(
            std::fs::read(&outside).unwrap(),
            NEW_HONEST,
            "the target of the link is intact"
        );
        for gone in ["OLD/config.json", "OLD/resource.json"] {
            assert!(sc.collection().join(format!("{DT}/{gone}")).is_file());
        }
    }

    /// F6: a staged copy swapped for a far larger file is refused by the
    /// script's hash check before any move.
    #[tokio::test]
    async fn sftp_mirror_oversize_swap_is_refused_before_any_rename() {
        let sc = Scratch::new();
        let tap = Arc::new(Tap::default());
        let victim = sc.stage_root.join(format!("{DT}/NEW/config.json"));
        let hook_victim = victim.clone();
        *tap.before_exec.lock().expect("hook") = Some(Box::new(move |_command: &str| {
            std::fs::write(&hook_victim, vec![b'x'; 24 * 1024 * 1024]).expect("swap");
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

    /// Was `sftp_mirror_inplace_verifies_after_the_write_and_stops_before_deletes`.
    /// An InPlace commit now uploads the staged copies to a control directory
    /// inside the call and moves them with the script, so a swap of an
    /// uploaded copy is refused BEFORE any write; an honest InPlace commit
    /// still passes.
    #[tokio::test]
    async fn sftp_mirror_inplace_swap_of_the_uploaded_copy_is_refused_before_any_write() {
        let sc = Scratch::new();
        let tap = Arc::new(Tap::default());
        let root = sc.root.path().to_path_buf();
        *tap.before_exec.lock().expect("hook") = Some(Box::new(move |_command: &str| {
            for entry in std::fs::read_dir(&root).expect("root").flatten() {
                if entry.file_name().to_string_lossy().starts_with(".ign-ctl-") {
                    let copy = entry
                        .path()
                        .join("files")
                        .join(format!("{DT}/NEW/resource.json"));
                    if copy.is_file() {
                        std::fs::write(&copy, NEW_EVIL).expect("swap");
                    }
                }
            }
        }));
        let m = sftp_mirror(&sc, StagingMode::InPlace, tap.clone());
        let staged = m.stage(&scenario_tree()).await.expect("stage");
        let before = snapshot(&sc.resources());
        let err = m
            .commit(&staged, &scenario_deletes())
            .await
            .expect_err("an uploaded copy was swapped");
        assert_eq!(err.code(), "transport_failed", "{err}");
        let text = err.to_string();
        assert!(text.contains("SHA-256"), "{text}");
        assert!(text.contains("NEW/resource.json"), "{text}");
        assert_eq!(
            snapshot(&sc.resources()),
            before,
            "nothing was written, nothing deleted"
        );
        assert_no_collection_mutation(&tap, &sc);
        assert_no_control_dirs(&sc);

        // an honest InPlace commit still passes
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

    /// D-01: a Linux commit is exactly one script call and no sftp `rename`.
    #[tokio::test]
    async fn sftp_mirror_commit_is_one_script_call_and_renames_nothing() {
        for separate in [true, false] {
            let sc = Scratch::new();
            let tap = Arc::new(Tap::default());
            let staging = if separate {
                sc.separate()
            } else {
                StagingMode::InPlace
            };
            let m = sftp_mirror(&sc, staging, tap.clone());
            run_scenario(&m, &sc).await;
            assert_eq!(
                tap.execs().len(),
                1,
                "separate={separate}: {:?}",
                tap.execs()
            );
            for line in tap.lines().iter().flatten() {
                assert!(!line.starts_with("rename "), "{line}");
            }
            // the remote command is fixed text, the script path, the op,
            // three roots and two hashes: no collection path
            let command = &tap.execs()[0];
            assert!(command.contains("'mirror'"), "{command}");
            assert!(!command.contains("OLD/config.json"), "{command}");
            assert!(!command.contains("UPD/config.json"), "{command}");
        }
    }
}

/// No read-only verb or surface can reach the write half (T-22-30, T-23-59):
/// the three sources that dispatch the read-only verbs name none of its
/// identifiers. `deploy_plan`, `deploy_pull` and `deploy_verify` live in
/// `actions/deploy.rs`, receive a `Box<dyn TreeReader>` from `for_node`, and
/// the CLI and MCP surfaces only call those actions. Phase 23 added the
/// apply host (`ApplyHost`, `apply_host_for_node`) to the forbidden names: the
/// write verbs reach it only through `actions/deploy_apply.rs`, which builds
/// it (`apply_host_is_confined`, run at the end of this test). Phase 25 adds
/// the project release record writer (`ReleaseRecordWriter`,
/// `release_writer_for_node`) the same way: only `actions/deploy_projects.rs`
/// builds it (`release_writer_is_confined`, also run at the end).
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
        "ApplyHost",
        "apply_host_for_node",
        "ReleaseRecordWriter",
        "release_writer_for_node",
        "TagRecordWriter",
        "tag_record_writer_for_node",
        "ProjectRunWriter",
        "project_run_writer_for_node",
        "GitStateWriter",
        "git_state_writer_for_node",
    ];
    for (file, text) in sources {
        assert!(text.len() > 1000, "{file} was not read");
        for identifier in forbidden {
            assert!(
                !text.contains(identifier),
                "{file} names `{identifier}`: no read-only verb or surface (plan, pull, verify, the CLI dispatch, the MCP bridge) may reach the transport write half or the apply host"
            );
        }
    }
    apply_host_is_confined();
    release_writer_is_confined();
    tag_record_writer_is_confined();
    git_state_writer_is_confined();
}

/// 24-05 (D-01, T-24-15): `deploy/pair.rs` is the only door to the backup
/// node and it only reads. The source names none of the gateway writes and
/// none of the transport's write half, so a write to the backup cannot be
/// added there without this test failing.
#[test]
fn pair_module_never_writes() {
    let text = include_str!("../src/deploy/pair.rs");
    assert!(text.len() > 1000, "deploy/pair.rs was not read");
    let forbidden = [
        "resource_create",
        "resource_update",
        "scan_request",
        "scan_lock_acquire",
        "mode_create",
        "redundancy_resync",
        "ApplyHost",
        "TreeMirror",
        "mirror_for_node",
        "apply_host_for_node",
        ".stage(",
        ".commit(",
    ];
    for identifier in forbidden {
        assert!(
            !text.contains(identifier),
            "deploy/pair.rs names `{identifier}`: the backup node is read, never written (D-01)"
        );
    }
}

/// Every `.rs` file below `dir`, depth first.
fn rust_sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|entry| entry.expect("dir entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The positive half of `write_half_unreachable` (T-23-59): the apply host is
/// built in exactly three places. Only files under `deploy/apply/`,
/// `deploy/transport/` and `actions/deploy_apply.rs` name `ApplyHost` or
/// `apply_host_for_node`; the CLI crate names neither, so a verb that wants
/// the write half must go through `actions/deploy_apply.rs`.
fn apply_host_is_confined() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_sources(&manifest.join("src"), &mut files);
    rust_sources(&manifest.join("../ignition-cli/src"), &mut files);
    assert!(
        files.len() > 50,
        "the walk found only {} files",
        files.len()
    );
    // 26-10: `deploy projects` commits a tag-owning project's tag-group units
    // through the same host, and builds it in its action like its record
    // writers; the CLI still names neither.
    let allowed = [
        "ignition-core/src/deploy/apply/",
        "ignition-core/src/deploy/transport/",
        "ignition-core/src/actions/deploy_apply.rs",
        "ignition-core/src/actions/deploy_projects.rs",
    ];
    let mut naming = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
        if !(text.contains("ApplyHost") || text.contains("apply_host_for_node")) {
            continue;
        }
        let shown = file
            .canonicalize()
            .unwrap_or_else(|_| file.clone())
            .to_string_lossy()
            .replace('\\', "/");
        assert!(
            allowed.iter().any(|ok| shown.contains(ok)),
            "{shown} names `ApplyHost` or `apply_host_for_node` but is outside deploy/apply/, \
             deploy/transport/, actions/deploy_apply.rs and actions/deploy_projects.rs"
        );
        naming.push(shown);
    }
    // The walk really saw the three places (a path typo would make the
    // confinement check vacuous).
    for expected in allowed {
        assert!(
            naming.iter().any(|shown| shown.contains(expected)),
            "no file under {expected} names the apply host: the walk is not looking where it should; saw {naming:?}"
        );
    }
}

/// The positive half for the project release record writer (T-25-37): the
/// writer is built in exactly two places. Only files under `deploy/transport/`
/// and `actions/deploy_projects.rs` name `ReleaseRecordWriter` or
/// `release_writer_for_node`; the CLI crate names neither, so a verb that wants
/// to write a release record must go through `actions/deploy_projects.rs`.
fn release_writer_is_confined() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_sources(&manifest.join("src"), &mut files);
    rust_sources(&manifest.join("../ignition-cli/src"), &mut files);
    assert!(
        files.len() > 50,
        "the walk found only {} files",
        files.len()
    );
    let allowed = [
        "ignition-core/src/deploy/transport/",
        "ignition-core/src/actions/deploy_projects.rs",
    ];
    let mut naming = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
        if !(text.contains("ReleaseRecordWriter") || text.contains("release_writer_for_node")) {
            continue;
        }
        let shown = file
            .canonicalize()
            .unwrap_or_else(|_| file.clone())
            .to_string_lossy()
            .replace('\\', "/");
        assert!(
            allowed.iter().any(|ok| shown.contains(ok)),
            "{shown} names `ReleaseRecordWriter` or `release_writer_for_node` but is outside \
             deploy/transport/ and actions/deploy_projects.rs"
        );
        naming.push(shown);
    }
    // The walk really saw both places (a path typo would make the confinement
    // check vacuous).
    for expected in allowed {
        assert!(
            naming.iter().any(|shown| shown.contains(expected)),
            "no file under {expected} names the release record writer: the walk is not looking where it should; saw {naming:?}"
        );
    }
}

/// The positive half for the tag record writer (26-06, T-26-20): only files
/// under `deploy/transport/` and `actions/deploy_projects.rs` name
/// `TagRecordWriter` or `tag_record_writer_for_node`; the CLI crate names
/// neither. Also run at the end of `write_half_unreachable`.
///
/// The project deploy writes the tag record (26-08), so the walk must see
/// both the trait's home under `deploy/transport/` and
/// `actions/deploy_projects.rs`.
#[test]
fn tag_record_writer_is_confined() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_sources(&manifest.join("src"), &mut files);
    rust_sources(&manifest.join("../ignition-cli/src"), &mut files);
    assert!(
        files.len() > 50,
        "the walk found only {} files",
        files.len()
    );
    let allowed = [
        "ignition-core/src/deploy/transport/",
        "ignition-core/src/actions/deploy_projects.rs",
    ];
    let mut naming = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
        if !(text.contains("TagRecordWriter") || text.contains("tag_record_writer_for_node")) {
            continue;
        }
        let shown = file
            .canonicalize()
            .unwrap_or_else(|_| file.clone())
            .to_string_lossy()
            .replace('\\', "/");
        assert!(
            allowed.iter().any(|ok| shown.contains(ok)),
            "{shown} names `TagRecordWriter` or `tag_record_writer_for_node` but is outside \
             deploy/transport/ and actions/deploy_projects.rs"
        );
        naming.push(shown);
    }
    // The walk really saw the trait's home (a path typo would make the
    // confinement check vacuous).
    for expected in [
        "ignition-core/src/deploy/transport/projects.rs",
        "ignition-core/src/deploy/transport/local/project_ops.rs",
        "ignition-core/src/deploy/transport/sftp/project_ops.rs",
        "ignition-core/src/actions/deploy_projects.rs",
    ] {
        assert!(
            naming.iter().any(|shown| shown.contains(expected)),
            "{expected} does not name the tag record writer: the walk is not looking where it should; saw {naming:?}"
        );
    }
}

/// The positive half for the `.git` writer (27-02, T-27-02, T-27-03): only
/// files under `deploy/transport/`, `deploy/git_handover.rs` and the project
/// deploy action (`actions/deploy_projects.rs` and `actions/deploy_projects/`)
/// name `GitStateWriter`; the CLI crate names nothing. Also run at the end of
/// `write_half_unreachable`.
///
/// The action does not name it yet (27-12 wires the verb), so the walk is
/// required to see the trait's home, the local implementation and the
/// handover, not the action.
#[test]
fn git_state_writer_is_confined() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_sources(&manifest.join("src"), &mut files);
    rust_sources(&manifest.join("../ignition-cli/src"), &mut files);
    assert!(
        files.len() > 50,
        "the walk found only {} files",
        files.len()
    );
    let allowed = [
        "ignition-core/src/deploy/transport/",
        "ignition-core/src/deploy/git_handover.rs",
        "ignition-core/src/actions/deploy_projects.rs",
        "ignition-core/src/actions/deploy_projects/",
    ];
    let mut naming = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
        if !text.contains("GitStateWriter") {
            continue;
        }
        let shown = file
            .canonicalize()
            .unwrap_or_else(|_| file.clone())
            .to_string_lossy()
            .replace('\\', "/");
        assert!(
            allowed.iter().any(|ok| shown.contains(ok)),
            "{shown} names `GitStateWriter` but is outside deploy/transport/, \
             deploy/git_handover.rs and the project deploy action"
        );
        naming.push(shown);
    }
    // The walk really saw the trait's home, its local implementation and the
    // handover (a path typo would make the confinement check vacuous).
    for expected in [
        "ignition-core/src/deploy/transport/projects.rs",
        "ignition-core/src/deploy/transport/local/project_ops.rs",
        "ignition-core/src/deploy/git_handover.rs",
    ] {
        assert!(
            naming.iter().any(|shown| shown.contains(expected)),
            "{expected} does not name the git state writer: the walk is not looking where it should; saw {naming:?}"
        );
    }
}

/// 26-06 (TAGDEP-09, T-26-18 to T-26-20): the tag record over the
/// local-filesystem transport. The read mirrors the release-record read
/// (`None` only when no level of the path exists, everything else that is not
/// a readable regular file under the cap refused), and the writer puts exactly
/// one file, atomically, and only bytes that are that gateway's and
/// provider's record.
mod tag_record_local {
    use std::path::Path;

    use ignition_core::deploy::ledger::{release_record_rel, tag_record_rel};
    use ignition_core::deploy::tag_record::{TAG_RECORD_MAX_BYTES, TagRecord};
    use ignition_core::deploy::transport::local::LocalFsTransport;
    use ignition_core::deploy::transport::projects::{ProjectHost, TagRecordWriter};

    const GW: &str = "gw-a";
    const P: &str = "PlantA";
    const LOC: &str = "ign-deploy/gw-a/tag-providers/PlantA/record.json";

    fn host(data: &Path) -> LocalFsTransport {
        LocalFsTransport::new(data.to_path_buf(), "nodes.master")
    }

    fn record(gateway: &str, provider: &str, release: &str) -> Vec<u8> {
        TagRecord {
            schema_version: 1,
            gateway: gateway.to_string(),
            provider: provider.to_string(),
            project: "CapApp".to_string(),
            release: release.to_string(),
            run_id: "20261108T101500Z-ab12".to_string(),
            deployed_at: "2026-11-08T10:15:00Z".to_string(),
            tags_sha256: "ab".repeat(32),
            nodes: vec!["Area1".to_string(), "Area1/Pump".to_string()],
            groups: vec!["Default".to_string()],
        }
        .to_bytes()
    }

    fn put(data: &Path, bytes: &[u8]) {
        let path = data.join(tag_record_rel(GW, P));
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, bytes).expect("write");
    }

    /// Every path below `root`, relative and sorted.
    fn tree(root: &Path) -> Vec<String> {
        fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries {
                let path = entry.expect("entry").path();
                out.push(
                    path.strip_prefix(root)
                        .expect("below root")
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
                if path.is_dir() {
                    walk(root, &path, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(root, root, &mut out);
        out.sort();
        out
    }

    #[tokio::test]
    async fn tag_record_read_is_none_only_when_no_level_exists() {
        let dir = tempfile::tempdir().expect("tempdir");
        let t = host(dir.path());
        assert_eq!(t.read_tag_record(GW, P).await.expect("no ign-deploy"), None);
        std::fs::create_dir_all(dir.path().join("ign-deploy/gw-a/tag-providers")).expect("mkdir");
        assert_eq!(t.read_tag_record(GW, P).await.expect("no provider"), None);
        std::fs::create_dir_all(dir.path().join("ign-deploy/gw-a/tag-providers/PlantA"))
            .expect("mkdir");
        assert_eq!(t.read_tag_record(GW, P).await.expect("no file"), None);

        // A regular file returns its bytes, unvalidated.
        put(dir.path(), b"{\"schema_version\":1}");
        assert_eq!(
            t.read_tag_record(GW, P).await.expect("file"),
            Some(b"{\"schema_version\":1}".to_vec())
        );
        // Another provider's record is not this one's.
        assert_eq!(t.read_tag_record(GW, "PlantB").await.expect("other"), None);
    }

    #[tokio::test]
    async fn tag_record_read_refuses_what_it_cannot_trust() {
        let dir = tempfile::tempdir().expect("tempdir");
        let t = host(dir.path());
        // Over the cap.
        put(dir.path(), &vec![b' '; TAG_RECORD_MAX_BYTES + 1]);
        let err = t.read_tag_record(GW, P).await.expect_err("over the cap");
        assert_eq!(err.code(), "deploy_run_invalid");
        assert!(err.to_string().contains(LOC), "{err}");
        assert!(err.to_string().contains("byte cap"), "{err}");
        // A file exactly at the cap is read.
        put(dir.path(), &vec![b' '; TAG_RECORD_MAX_BYTES]);
        assert_eq!(
            t.read_tag_record(GW, P)
                .await
                .expect("at the cap")
                .map(|b| b.len()),
            Some(TAG_RECORD_MAX_BYTES)
        );
        // A directory where the file belongs.
        let file = dir.path().join(tag_record_rel(GW, P));
        std::fs::remove_file(&file).expect("rm");
        std::fs::create_dir(&file).expect("dir");
        let err = t.read_tag_record(GW, P).await.expect_err("a directory");
        assert_eq!(err.code(), "deploy_run_invalid");
        assert!(err.to_string().contains(LOC), "{err}");
        // A file where a directory belongs.
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("ign-deploy/gw-a")).expect("mkdir");
        std::fs::write(dir.path().join("ign-deploy/gw-a/tag-providers"), b"x").expect("file");
        let err = host(dir.path())
            .read_tag_record(GW, P)
            .await
            .expect_err("not a directory");
        assert_eq!(err.code(), "deploy_run_invalid");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn tag_record_read_refuses_symlinks() {
        let dir = tempfile::tempdir().expect("tempdir");
        let t = host(dir.path());
        let outside = dir.path().join("outside.json");
        std::fs::write(&outside, record(GW, P, "v1.0.0")).expect("outside");
        let file = dir.path().join(tag_record_rel(GW, P));
        std::fs::create_dir_all(file.parent().expect("parent")).expect("mkdir");
        std::os::unix::fs::symlink(&outside, &file).expect("symlink");
        let err = t.read_tag_record(GW, P).await.expect_err("a linked record");
        assert_eq!(err.code(), "deploy_run_invalid");
        assert!(err.to_string().contains("symlink"), "{err}");

        // A linked directory above the record, dangling or not.
        std::fs::remove_file(&file).expect("rm");
        let real = dir.path().join("elsewhere");
        std::fs::create_dir_all(&real).expect("real");
        std::fs::remove_dir(file.parent().expect("parent")).expect("rmdir");
        std::os::unix::fs::symlink(&real, file.parent().expect("parent")).expect("dir link");
        let err = t
            .read_tag_record(GW, P)
            .await
            .expect_err("a linked directory");
        assert_eq!(err.code(), "deploy_run_invalid");
    }

    #[tokio::test]
    async fn tag_record_read_with_an_unreadable_host_is_not_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = host(&dir.path().join("gone"))
            .read_tag_record(GW, P)
            .await
            .expect_err("no data_dir");
        assert_eq!(err.code(), "transport_failed");
    }

    #[tokio::test]
    async fn tag_record_read_refuses_unsafe_names() {
        let dir = tempfile::tempdir().expect("tempdir");
        let t = host(dir.path());
        for (gateway, provider) in [("..", P), (GW, ".."), (GW, "a/b"), ("", P), (GW, "")] {
            let err = t
                .read_tag_record(gateway, provider)
                .await
                .expect_err("unsafe name");
            assert_eq!(err.code(), "deploy_run_invalid", "{gateway:?} {provider:?}");
        }
    }

    /// The record is written below the ledger root, read back, and replaced
    /// by the next write; no temporary file is left and nothing else is
    /// created (the release-record directory included).
    #[tokio::test]
    async fn tag_record_write_round_trips_and_writes_one_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let t = host(dir.path());
        let first = record(GW, P, "v1.0.0");
        t.write_tag_record(GW, P, &first).await.expect("write");
        assert_eq!(
            t.read_tag_record(GW, P).await.expect("read"),
            Some(first.clone())
        );
        assert_eq!(
            tree(dir.path()),
            [
                "ign-deploy",
                "ign-deploy/gw-a",
                "ign-deploy/gw-a/tag-providers",
                "ign-deploy/gw-a/tag-providers/PlantA",
                LOC,
            ],
            "exactly the record and its directories"
        );
        assert!(!dir.path().join(release_record_rel(GW, "CapApp")).exists());

        let second = record(GW, P, "v1.1.0");
        t.write_tag_record(GW, P, &second).await.expect("replace");
        assert_eq!(t.read_tag_record(GW, P).await.expect("read"), Some(second));
        let folder = dir.path().join("ign-deploy/gw-a/tag-providers/PlantA");
        let names: Vec<String> = std::fs::read_dir(&folder)
            .expect("folder")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["record.json"], "no temporary file is left");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(folder.join("record.json"))
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    /// The temporary sibling is named for the run: another run's leftover is
    /// neither read, removed nor renamed in.
    #[tokio::test]
    async fn tag_record_write_uses_a_per_run_temporary_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let folder = dir.path().join("ign-deploy/gw-a/tag-providers/PlantA");
        std::fs::create_dir_all(&folder).expect("folder");
        std::fs::write(folder.join("record.json.tmp-other-run"), b"another run").expect("other");
        let bytes = record(GW, P, "v1.0.0");
        host(dir.path())
            .write_tag_record(GW, P, &bytes)
            .await
            .expect("write");
        assert_eq!(
            std::fs::read(folder.join("record.json")).expect("record"),
            bytes
        );
        assert_eq!(
            std::fs::read(folder.join("record.json.tmp-other-run")).expect("kept"),
            b"another run"
        );
        let mut names: Vec<String> = std::fs::read_dir(&folder)
            .expect("folder")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["record.json", "record.json.tmp-other-run"]);
    }

    /// The writer applies the reader's name check and refuses bytes that are
    /// not this gateway's and provider's record; a refusal creates nothing.
    #[tokio::test]
    async fn tag_record_write_fails_closed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let t = host(dir.path());
        let good = record(GW, P, "v1.0.0");
        for (gateway, provider) in [("..", P), (GW, ".."), (GW, "a/b"), ("", P), (GW, ".x")] {
            let err = t
                .write_tag_record(gateway, provider, &good)
                .await
                .expect_err("unsafe name");
            assert_eq!(err.code(), "deploy_run_invalid", "{gateway:?} {provider:?}");
        }
        for bytes in [
            b"{}".to_vec(),
            b"not json".to_vec(),
            Vec::new(),
            record("gw-b", P, "v1.0.0"),
            record(GW, "PlantB", "v1.0.0"),
            record(GW, "planta", "v1.0.0"),
            // a release record is not a tag record
            ignition_core::deploy::projects::ReleaseRecord {
                schema_version: 1,
                gateway: GW.to_string(),
                project: "CapApp".to_string(),
                release: "v1.0.0".to_string(),
                artifact_sha256: "a".repeat(64),
                run_id: "20261108T101500Z-ab12".to_string(),
                deployed_at: "2026-11-08T10:15:00Z".to_string(),
            }
            .to_bytes(),
            vec![b' '; TAG_RECORD_MAX_BYTES + 1],
        ] {
            let err = t
                .write_tag_record(GW, P, &bytes)
                .await
                .expect_err("not this provider's record");
            assert_eq!(err.code(), "deploy_run_invalid");
            assert!(err.to_string().contains(LOC), "{err}");
        }
        assert!(
            !dir.path().join("ign-deploy").exists(),
            "a refused write creates nothing"
        );
        let err = host(&dir.path().join("gone"))
            .write_tag_record(GW, P, &good)
            .await
            .expect_err("no data_dir");
        assert_eq!(err.code(), "transport_failed");
    }

    /// A symlink on the record's path is never followed, at the file or at
    /// any directory above it.
    #[cfg(unix)]
    #[tokio::test]
    async fn tag_record_write_refuses_symlinks() {
        let dir = tempfile::tempdir().expect("tempdir");
        let t = host(dir.path());
        let bytes = record(GW, P, "v1.0.0");
        let outside = dir.path().join("outside.json");
        std::fs::write(&outside, b"keep").expect("outside");
        let file = dir.path().join(tag_record_rel(GW, P));
        std::fs::create_dir_all(file.parent().expect("parent")).expect("mkdir");
        std::os::unix::fs::symlink(&outside, &file).expect("symlink");
        let err = t
            .write_tag_record(GW, P, &bytes)
            .await
            .expect_err("a linked record");
        assert_eq!(err.code(), "deploy_run_invalid");
        assert_eq!(std::fs::read(&outside).expect("untouched"), b"keep");

        std::fs::remove_file(&file).expect("rm");
        let real = dir.path().join("elsewhere");
        std::fs::create_dir_all(&real).expect("real");
        std::fs::remove_dir(file.parent().expect("parent")).expect("rmdir");
        std::os::unix::fs::symlink(&real, file.parent().expect("parent")).expect("dir link");
        let err = t
            .write_tag_record(GW, P, &bytes)
            .await
            .expect_err("a linked directory");
        assert_eq!(err.code(), "deploy_run_invalid");
        assert_eq!(
            std::fs::read_dir(&real).expect("real").count(),
            0,
            "nothing was written through the link"
        );
    }

    /// Both traits reach the same file through `dyn`, as the engine uses them.
    #[tokio::test]
    async fn tag_record_through_the_traits() {
        let dir = tempfile::tempdir().expect("tempdir");
        let writer: Box<dyn TagRecordWriter> = Box::new(host(dir.path()));
        let reader: Box<dyn ProjectHost> = Box::new(host(dir.path()));
        let bytes = record(GW, P, "v1.0.0");
        writer.write_tag_record(GW, P, &bytes).await.expect("write");
        assert_eq!(
            reader.read_tag_record(GW, P).await.expect("read"),
            Some(bytes)
        );
    }
}

/// `list_tag_providers` over the local-filesystem transport (quick-261029-ms3).
mod list_tag_providers_local {
    use crate::common::plan_fixture::PlanFixture;
    use ignition_core::deploy::transport::TreeReader;
    use ignition_core::deploy::transport::local::LocalFsTransport;

    #[tokio::test]
    async fn list_tag_providers_lists_directory_names_only() {
        let fx = PlanFixture::new();
        fx.core("ignition/tag-provider/zeta/config.json", b"{}");
        fx.core("ignition/tag-provider/MQTT Engine/config.json", b"{}");
        fx.core("ignition/tag-provider/loose.json", b"{}");
        std::fs::create_dir_all(
            fx.collection_dir("core")
                .join("ignition/tag-provider/Empty"),
        )
        .expect("empty provider");
        let reader = LocalFsTransport::new(fx.data_dir().to_path_buf(), "nodes.master");
        assert_eq!(
            reader.list_tag_providers("core").await.expect("providers"),
            ["Empty", "MQTT Engine", "zeta"]
        );
        // no tag-provider directory, and no collection at all
        fx.core("ignition/database-translator/X/config.json", b"{}");
        assert!(
            reader
                .list_tag_providers("dev")
                .await
                .expect("absent")
                .is_empty()
        );
        assert!(
            reader
                .list_tag_providers("nothing")
                .await
                .expect("absent")
                .is_empty()
        );
    }

    /// C5 (PR #31 review): the collection name is one safe path component,
    /// checked before any path is built or probed, so `..` or a separator
    /// can never list a directory outside `config/resources/<collection>`.
    #[tokio::test]
    async fn list_tag_providers_refuses_an_unsafe_collection_name() {
        let fx = PlanFixture::new();
        fx.core("ignition/tag-provider/real/config.json", b"{}");
        // what `..` would reach: data/config/ignition/tag-provider
        std::fs::create_dir_all(fx.data_dir().join("config/ignition/tag-provider/Escaped"))
            .expect("outside dir");
        let reader = LocalFsTransport::new(fx.data_dir().to_path_buf(), "nodes.master");
        for bad in ["..", ".", "core/../core", "core\\x", "", ".hidden", "a\0b"] {
            let err = reader
                .list_tag_providers(bad)
                .await
                .expect_err(&format!("{bad:?} must be refused"));
            assert_eq!(err.code(), "transport_failed", "{bad:?}: {err}");
            assert!(
                err.to_string().contains("the collection name"),
                "{bad:?}: {err}"
            );
        }
        assert_eq!(
            reader.list_tag_providers("core").await.expect("safe"),
            ["real"]
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn list_tag_providers_refuses_a_symlink() {
        let fx = PlanFixture::new();
        fx.core("ignition/tag-provider/real/config.json", b"{}");
        let dir = fx.collection_dir("core").join("ignition/tag-provider");
        std::os::unix::fs::symlink(dir.join("real"), dir.join("link")).expect("symlink");
        let reader = LocalFsTransport::new(fx.data_dir().to_path_buf(), "nodes.master");
        let err = reader
            .list_tag_providers("core")
            .await
            .expect_err("symlink");
        assert_eq!(err.code(), "transport_failed", "{err}");
        assert!(err.to_string().contains("symlink"), "{err}");
    }
}

/// 26-12 (TAGDEP-10, T-26-43, T-26-44, T-26-46): the host journal and the
/// pre-image of a tag-owning project run, over the local-filesystem
/// transport. Writes are atomic, private, confined to the fixed names and
/// never follow a symlink; reads are `None` only when nothing is there and
/// refuse a symlink, a non-regular file and an over-cap file.
mod project_run_local {
    use std::path::Path;

    use ignition_core::deploy::ledger::{pre_image_rel, project_journal_rel, release_record_rel};
    use ignition_core::deploy::project_run::{
        JournalState, PRE_IMAGE_MAX_BYTES, PROJECT_JOURNAL_MAX_BYTES, ProjectJournal,
    };
    use ignition_core::deploy::transport::local::LocalFsTransport;
    use ignition_core::deploy::transport::projects::{ProjectHost, ProjectRunWriter};

    pub(crate) const GW: &str = "gw-a";
    pub(crate) const PROJECT: &str = "CapApp";
    pub(crate) const RUN: &str = "20261109T101500Z-ab12";

    fn host(data: &Path) -> LocalFsTransport {
        LocalFsTransport::new(data.to_path_buf(), "nodes.master")
    }

    pub(crate) fn journal_bytes(project: &str) -> Vec<u8> {
        ProjectJournal {
            schema_version: 1,
            gateway: GW.to_string(),
            project: project.to_string(),
            run_id: RUN.to_string(),
            state: JournalState::Applying,
            started_at: "2026-11-09T10:15:00Z".to_string(),
            providers: vec!["PlantA".to_string()],
        }
        .to_bytes()
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_dir(dir)
            .expect("dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        out.sort();
        out
    }

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).expect("meta").permissions().mode() & 0o777
    }

    /// The journal is written beside the release record, private, read back,
    /// and cleared; clearing removes only `journal.json` and succeeds when it
    /// is already gone.
    #[tokio::test]
    async fn project_run_journal_round_trips_and_clears_only_itself() {
        let dir = tempfile::tempdir().expect("tempdir");
        let t = host(dir.path());
        assert_eq!(t.read_journal(GW, PROJECT).await.expect("absent"), None);
        let bytes = journal_bytes(PROJECT);
        t.write_journal(GW, PROJECT, &bytes).await.expect("write");
        assert_eq!(
            t.read_journal(GW, PROJECT).await.expect("read"),
            Some(bytes)
        );
        let folder = dir.path().join("ign-deploy/gw-a/projects/CapApp");
        assert_eq!(names(&folder), ["journal.json"], "no temporary file");
        #[cfg(unix)]
        assert_eq!(mode(&folder.join("journal.json")), 0o600);

        std::fs::write(dir.path().join(release_record_rel(GW, PROJECT)), b"{}").expect("rec");
        t.write_pre_image(GW, PROJECT, RUN, "project.zip", b"PK")
            .await
            .expect("pre-image");
        t.clear_journal(GW, PROJECT).await.expect("clear");
        assert_eq!(names(&folder), ["pre-image", "release.json"]);
        assert_eq!(names(&folder.join("pre-image")), ["project.zip"]);
        t.clear_journal(GW, PROJECT).await.expect("clear again");
        // nothing at all below the home: still a success
        let empty = tempfile::tempdir().expect("tempdir");
        host(empty.path())
            .clear_journal(GW, PROJECT)
            .await
            .expect("no directory");
    }

    /// Every fixed pre-image name round-trips, private, with no temporary file
    /// left; a name outside the set, an unsafe provider or run id, or a
    /// `run.json` that is not this run's set is refused and creates nothing.
    #[tokio::test]
    async fn project_run_pre_image_fixed_names_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let t = host(dir.path());
        for name in [
            "project.zip",
            "tags-PlantA.json",
            "provider-PlantA.json",
            "record-PlantA.json",
        ] {
            assert_eq!(
                t.read_pre_image(GW, PROJECT, name).await.expect("absent"),
                None
            );
            let bytes = format!("bytes of {name}").into_bytes();
            t.write_pre_image(GW, PROJECT, RUN, name, &bytes)
                .await
                .expect(name);
            assert_eq!(
                t.read_pre_image(GW, PROJECT, name).await.expect("read"),
                Some(bytes)
            );
            #[cfg(unix)]
            assert_eq!(
                mode(&dir.path().join(pre_image_rel(GW, PROJECT, name))),
                0o600,
                "{name}"
            );
        }
        assert_eq!(
            names(&dir.path().join("ign-deploy/gw-a/projects/CapApp/pre-image")),
            [
                "project.zip",
                "provider-PlantA.json",
                "record-PlantA.json",
                "tags-PlantA.json"
            ]
        );

        let fresh = tempfile::tempdir().expect("tempdir");
        let f = host(fresh.path());
        for name in [
            "journal.json",
            "release.json",
            "other.json",
            "tags-.json",
            "tags-a/b.json",
            "../release.json",
            "tags-..json",
        ] {
            let err = f
                .write_pre_image(GW, PROJECT, RUN, name, b"x")
                .await
                .expect_err(name);
            assert_eq!(err.code(), "deploy_run_invalid", "{name}: {err}");
            let err = f.read_pre_image(GW, PROJECT, name).await.expect_err(name);
            assert_eq!(err.code(), "deploy_run_invalid", "{name}: {err}");
        }
        let err = f
            .write_pre_image(GW, PROJECT, "../x", "project.zip", b"x")
            .await
            .expect_err("unsafe run id");
        assert_eq!(err.code(), "deploy_run_invalid");
        for (gw, project) in [("..", PROJECT), (GW, ".."), (GW, "a/b"), ("", PROJECT)] {
            let err = f
                .write_pre_image(gw, project, RUN, "project.zip", b"x")
                .await
                .expect_err("unsafe name");
            assert_eq!(err.code(), "deploy_run_invalid", "{gw:?} {project:?}");
        }
        // run.json must be this gateway's, this project's and this run's set
        let err = f
            .write_pre_image(GW, PROJECT, RUN, "run.json", b"{}")
            .await
            .expect_err("not a set");
        assert_eq!(err.code(), "deploy_run_invalid");
        // the journal writer only takes this project's journal
        let err = f
            .write_journal(GW, PROJECT, &journal_bytes("Other"))
            .await
            .expect_err("another project's journal");
        assert_eq!(err.code(), "deploy_run_invalid");
        assert!(
            !fresh.path().join("ign-deploy").exists(),
            "a refused write creates nothing"
        );
    }

    /// A read refuses a directory, an over-cap file and an unreadable host;
    /// it never answers `None` for a file that is there.
    #[tokio::test]
    async fn project_run_reads_refuse_what_they_cannot_trust() {
        let dir = tempfile::tempdir().expect("tempdir");
        let t = host(dir.path());
        let journal = dir.path().join(project_journal_rel(GW, PROJECT));
        std::fs::create_dir_all(journal.parent().expect("parent")).expect("mkdir");
        std::fs::write(&journal, vec![b' '; PROJECT_JOURNAL_MAX_BYTES + 1]).expect("big");
        let err = t.read_journal(GW, PROJECT).await.expect_err("over the cap");
        assert_eq!(err.code(), "deploy_run_invalid", "{err}");
        assert!(
            err.to_string()
                .contains("ign-deploy/gw-a/projects/CapApp/journal.json"),
            "{err}"
        );
        std::fs::remove_file(&journal).expect("rm");
        std::fs::create_dir(&journal).expect("a directory");
        let err = t.read_journal(GW, PROJECT).await.expect_err("a directory");
        assert_eq!(err.code(), "deploy_run_invalid");

        let zip = dir.path().join(pre_image_rel(GW, PROJECT, "project.zip"));
        std::fs::create_dir_all(zip.parent().expect("parent")).expect("mkdir");
        let file = std::fs::File::create(&zip).expect("zip");
        file.set_len(PRE_IMAGE_MAX_BYTES as u64 + 1)
            .expect("sparse");
        let err = t
            .read_pre_image(GW, PROJECT, "project.zip")
            .await
            .expect_err("over the cap");
        assert_eq!(err.code(), "deploy_run_invalid", "{err}");

        let err = host(&dir.path().join("gone"))
            .read_journal(GW, PROJECT)
            .await
            .expect_err("no data_dir");
        assert_eq!(err.code(), "transport_failed");
    }

    /// A symlink is never followed: not as the journal, not as a pre-image
    /// file, not as the pre-image directory, for a read, a write or a clear.
    #[cfg(unix)]
    #[tokio::test]
    async fn project_run_never_follows_a_symlink() {
        let dir = tempfile::tempdir().expect("tempdir");
        let t = host(dir.path());
        let outside = dir.path().join("outside.json");
        std::fs::write(&outside, b"keep").expect("outside");
        let journal = dir.path().join(project_journal_rel(GW, PROJECT));
        std::fs::create_dir_all(journal.parent().expect("parent")).expect("mkdir");
        std::os::unix::fs::symlink(&outside, &journal).expect("link");
        let err = t.read_journal(GW, PROJECT).await.expect_err("linked");
        assert_eq!(err.code(), "deploy_run_invalid");
        let err = t
            .write_journal(GW, PROJECT, &journal_bytes(PROJECT))
            .await
            .expect_err("linked");
        assert_eq!(err.code(), "deploy_run_invalid");
        let err = t.clear_journal(GW, PROJECT).await.expect_err("linked");
        assert_eq!(err.code(), "deploy_run_invalid");
        assert_eq!(std::fs::read(&outside).expect("kept"), b"keep");
        assert!(std::fs::symlink_metadata(&journal).is_ok(), "link kept");

        let real = dir.path().join("elsewhere");
        std::fs::create_dir_all(&real).expect("real");
        let pre = journal.parent().expect("parent").join("pre-image");
        std::os::unix::fs::symlink(&real, &pre).expect("dir link");
        let err = t
            .write_pre_image(GW, PROJECT, RUN, "project.zip", b"PK")
            .await
            .expect_err("linked dir");
        assert_eq!(err.code(), "deploy_run_invalid");
        let err = t
            .read_pre_image(GW, PROJECT, "project.zip")
            .await
            .expect_err("linked dir");
        assert_eq!(err.code(), "deploy_run_invalid");
        assert_eq!(std::fs::read_dir(&real).expect("real").count(), 0);
    }

    /// The writer and the host reach the same files through `dyn`.
    #[tokio::test]
    async fn project_run_through_the_traits() {
        let dir = tempfile::tempdir().expect("tempdir");
        let writer: Box<dyn ProjectRunWriter> = Box::new(host(dir.path()));
        let reader: Box<dyn ProjectHost> = Box::new(host(dir.path()));
        let bytes = journal_bytes(PROJECT);
        writer.write_journal(GW, PROJECT, &bytes).await.expect("w");
        assert_eq!(
            reader.read_journal(GW, PROJECT).await.expect("r"),
            Some(bytes)
        );
        writer.clear_journal(GW, PROJECT).await.expect("clear");
        assert_eq!(reader.read_journal(GW, PROJECT).await.expect("r"), None);
    }
}

/// 26-12: the same journal and pre-image behaviour over the sftp transport,
/// against the in-repo local `sftp-server` harness (the account home is the
/// scratch root, the exec seam runs the local `sh`).
#[cfg(unix)]
mod project_run_sftp {
    use super::project_run_local::{GW, PROJECT, RUN, journal_bytes};
    use super::*;
    use ignition_core::deploy::ledger::{pre_image_rel, project_journal_rel};
    use ignition_core::deploy::project_run::PROJECT_JOURNAL_MAX_BYTES;
    use ignition_core::deploy::transport::projects::{ProjectHost, ProjectRunWriter};
    use ignition_core::deploy::transport::sftp::{SftpTransport, posix_root};

    fn transport(root: &Path) -> SftpTransport {
        let shell = RemoteShell::local_sftp_server(
            &sftp_server(),
            Arc::new(TokioProcessRunner),
            Duration::from_secs(60),
        )
        .local_exec();
        let data = root.join("data");
        std::fs::create_dir_all(&data).expect("data");
        let data = posix_root(&data.display().to_string()).expect("posix root");
        let home = posix_root(&root.display().to_string()).expect("home");
        SftpTransport::new(shell, data, "nodes.master").with_home(&home)
    }

    #[tokio::test]
    async fn project_run_sftp_round_trips_and_clears() {
        let root = tempfile::tempdir().expect("root");
        let t = transport(root.path());
        assert_eq!(t.read_journal(GW, PROJECT).await.expect("absent"), None);
        assert_eq!(
            t.read_pre_image(GW, PROJECT, "project.zip")
                .await
                .expect("absent"),
            None
        );
        let bytes = journal_bytes(PROJECT);
        t.write_journal(GW, PROJECT, &bytes).await.expect("write");
        assert_eq!(
            t.read_journal(GW, PROJECT).await.expect("read"),
            Some(bytes)
        );
        t.write_pre_image(GW, PROJECT, RUN, "tags-PlantA.json", b"{\"tags\":[]}")
            .await
            .expect("pre-image");
        assert_eq!(
            t.read_pre_image(GW, PROJECT, "tags-PlantA.json")
                .await
                .expect("read"),
            Some(b"{\"tags\":[]}".to_vec())
        );
        {
            use std::os::unix::fs::PermissionsExt;
            for rel in [
                project_journal_rel(GW, PROJECT),
                pre_image_rel(GW, PROJECT, "tags-PlantA.json"),
            ] {
                let mode = std::fs::metadata(root.path().join(&rel))
                    .expect("meta")
                    .permissions()
                    .mode();
                assert_eq!(mode & 0o777, 0o600, "{rel}");
            }
        }
        let folder = root.path().join("ign-deploy/gw-a/projects/CapApp");
        t.clear_journal(GW, PROJECT).await.expect("clear");
        assert!(!folder.join("journal.json").exists());
        assert!(folder.join("pre-image/tags-PlantA.json").exists());
        t.clear_journal(GW, PROJECT).await.expect("clear again");
        let leftovers: Vec<String> = std::fs::read_dir(folder.join("pre-image"))
            .expect("dir")
            .map(|e| e.expect("e").file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[tokio::test]
    async fn project_run_sftp_refuses_names_symlinks_and_oversize() {
        let root = tempfile::tempdir().expect("root");
        let t = transport(root.path());
        for name in ["journal.json", "release.json", "tags-a*b.json", "x.json"] {
            let err = t
                .write_pre_image(GW, PROJECT, RUN, name, b"x")
                .await
                .expect_err(name);
            assert_eq!(err.code(), "deploy_run_invalid", "{name}: {err}");
        }
        assert!(!root.path().join("ign-deploy").exists());

        let journal = root.path().join(project_journal_rel(GW, PROJECT));
        std::fs::create_dir_all(journal.parent().expect("parent")).expect("mkdir");
        std::fs::write(&journal, vec![b' '; PROJECT_JOURNAL_MAX_BYTES + 1]).expect("big");
        let err = t.read_journal(GW, PROJECT).await.expect_err("over the cap");
        assert_eq!(err.code(), "deploy_run_invalid", "{err}");
        std::fs::remove_file(&journal).expect("rm");

        let outside = root.path().join("outside.json");
        std::fs::write(&outside, b"keep").expect("outside");
        std::os::unix::fs::symlink(&outside, &journal).expect("link");
        let err = t.read_journal(GW, PROJECT).await.expect_err("linked");
        assert_eq!(err.code(), "deploy_run_invalid", "{err}");
        assert!(
            t.write_journal(GW, PROJECT, &journal_bytes(PROJECT))
                .await
                .is_err()
        );
        assert!(t.clear_journal(GW, PROJECT).await.is_err());
        assert_eq!(std::fs::read(&outside).expect("kept"), b"keep");
    }
}

/// The positive half for the project run writer (26-12, T-26-46): only files
/// under `deploy/transport/` and `actions/deploy_projects.rs` name
/// `ProjectRunWriter` or `project_run_writer_for_node`; the CLI crate names
/// neither.
#[test]
fn project_run_writer_is_confined() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_sources(&manifest.join("src"), &mut files);
    rust_sources(&manifest.join("../ignition-cli/src"), &mut files);
    assert!(
        files.len() > 50,
        "the walk found only {} files",
        files.len()
    );
    let allowed = [
        "ignition-core/src/deploy/transport/",
        "ignition-core/src/actions/deploy_projects.rs",
    ];
    let mut naming = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
        if !(text.contains("ProjectRunWriter") || text.contains("project_run_writer_for_node")) {
            continue;
        }
        let shown = file
            .canonicalize()
            .unwrap_or_else(|_| file.clone())
            .to_string_lossy()
            .replace('\\', "/");
        assert!(
            allowed.iter().any(|ok| shown.contains(ok)),
            "{shown} names `ProjectRunWriter` or `project_run_writer_for_node` but is outside \
             deploy/transport/ and actions/deploy_projects.rs"
        );
        naming.push(shown);
    }
    for expected in [
        "ignition-core/src/deploy/transport/projects.rs",
        "ignition-core/src/deploy/transport/local/project_ops.rs",
        "ignition-core/src/deploy/transport/sftp/project_ops.rs",
        "ignition-core/src/actions/deploy_projects.rs",
    ] {
        assert!(
            naming.iter().any(|shown| shown.contains(expected)),
            "{expected} does not name the project run writer: the walk is not looking where it should; saw {naming:?}"
        );
    }
}

/// 27-08: the git host channel over the sftp transport on a POSIX node, against
/// the in-repo local `sftp-server` and the local `sh` (the exec seam). The
/// scripts under test are the very text a Linux host would run, so each result
/// is also compared with the local-filesystem implementation on the same
/// `.git`.
#[cfg(unix)]
mod sftp_git {
    use super::*;
    use common::git_fixture::{GATEWAY, GitFixture, PROJECT};
    use ignition_core::deploy::git_advance::{GitWrite, GitWriteName, OldValue};
    use ignition_core::deploy::git_state::GitFiles;
    use ignition_core::deploy::transport::projects::{
        GIT_CHANGED_OPERATION, GitEntry, GitHostRead, GitStateWriter, ProjectHost,
    };
    use ignition_core::deploy::transport::sftp::{SftpTransport, posix_root};
    use ignition_core::deploy::tree::sha256_hex;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const RUN: &str = "20261114T101500Z-ab12";

    type BatchHook = Arc<dyn Fn(&str) + Send + Sync>;

    /// Counts the exec calls (`sh -c`) and the sftp sessions a transport makes
    /// and keeps the exec command texts.
    #[derive(Default)]
    struct Count {
        execs: Mutex<Vec<String>>,
        sftp: AtomicUsize,
        /// A per-test directory of shim programs put first on `sh`'s PATH.
        shims: Mutex<Option<PathBuf>>,
        /// A directory that is the whole PATH of `sh` (a host without a tool).
        only_path: Mutex<Option<PathBuf>>,
        /// Exec commands holding a needle that fail like a dead connection
        /// (exit 255, nothing run): the needle and how many calls still fail.
        dead: Mutex<Vec<(String, usize)>>,
        /// Every `put` of every sftp batch: the remote path and the mode of its
        /// parent directory at the moment the batch was sent.
        puts: Mutex<Vec<(String, Option<u32>)>>,
        /// Called with the batch text just before an sftp batch runs.
        before_batch: Mutex<Option<BatchHook>>,
    }

    #[async_trait::async_trait]
    impl ProcessRunner for Count {
        async fn run(
            &self,
            program: &OsStr,
            args: &[OsString],
            stdin: Option<Vec<u8>>,
            timeout: Duration,
        ) -> ProcessOutput {
            if program == "sh" {
                let command = args
                    .last()
                    .map(|a| a.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.execs.lock().expect("execs").push(command.clone());
                let dead = {
                    let mut dead = self.dead.lock().expect("dead");
                    dead.iter_mut()
                        .find(|(needle, left)| *left > 0 && command.contains(needle.as_str()))
                        .map(|(_, left)| *left -= 1)
                        .is_some()
                };
                if dead {
                    return ProcessOutput {
                        stdout: Vec::new(),
                        stderr: b"Connection reset by peer".to_vec(),
                        code: 255,
                        timed_out: false,
                    };
                }
            } else {
                self.sftp.fetch_add(1, Ordering::Relaxed);
                let batch =
                    String::from_utf8_lossy(stdin.as_deref().unwrap_or_default()).into_owned();
                let hook = self.before_batch.lock().expect("hook").clone();
                if let Some(hook) = hook {
                    hook(&batch);
                }
                for line in batch.lines().filter(|l| l.starts_with("put ")) {
                    if let Some(remote) = line.rsplit('"').nth(1) {
                        let parent = Path::new(remote).parent().and_then(|p| {
                            std::fs::metadata(p)
                                .ok()
                                .map(|m| m.permissions().mode() & 0o777)
                        });
                        self.puts
                            .lock()
                            .expect("puts")
                            .push((remote.to_string(), parent));
                    }
                }
            }
            let only = self.only_path.lock().expect("only").clone();
            if program == "sh"
                && let Some(only) = only
            {
                let mut wrapped = vec![
                    OsString::from(format!("PATH={}", only.display())),
                    OsString::from("/bin/sh"),
                ];
                wrapped.extend(args.iter().cloned());
                return TokioProcessRunner
                    .run(OsStr::new("env"), &wrapped, stdin, timeout)
                    .await;
            }
            let extra = self.shims.lock().expect("shims").clone();
            let (program, args) = common::posix_exec::shimmed_with(program, args, extra.as_deref());
            TokioProcessRunner
                .run(&program, &args, stdin, timeout)
                .await
        }
    }

    impl Count {
        fn use_shims(&self, shims: &common::posix_exec::Shims) {
            *self.shims.lock().expect("shims") = Some(shims.path().to_path_buf());
        }
        /// The next `times` exec commands holding `needle` die like a dropped
        /// connection.
        fn kill_exec(&self, needle: &str, times: usize) {
            self.dead
                .lock()
                .expect("dead")
                .push((needle.to_string(), times));
        }
        fn puts(&self) -> Vec<(String, Option<u32>)> {
            self.puts.lock().expect("puts").clone()
        }
        fn on_batch(&self, hook: impl Fn(&str) + Send + Sync + 'static) {
            *self.before_batch.lock().expect("hook") = Some(Arc::new(hook));
        }
        fn execs(&self) -> usize {
            self.execs.lock().expect("execs").len()
        }
        fn sftp(&self) -> usize {
            self.sftp.load(Ordering::Relaxed)
        }
    }

    /// The sftp transport over `data_dir`, with a fresh account home.
    struct Node {
        t: SftpTransport,
        home: tempfile::TempDir,
        count: Arc<Count>,
    }

    fn node(data_dir: &Path) -> Node {
        let count = Arc::new(Count::default());
        let shell =
            RemoteShell::local_sftp_server(&sftp_server(), count.clone(), Duration::from_secs(60))
                .local_exec();
        let home = tempfile::tempdir().expect("home");
        let data = posix_root(&data_dir.display().to_string()).expect("posix root");
        let home_root = posix_root(&home.path().display().to_string()).expect("home root");
        let t = SftpTransport::new(shell, data, "nodes.master").with_home(&home_root);
        Node { t, home, count }
    }

    /// Whether this process ignores mode bits (root), so denial cases cannot run.
    pub(super) fn ignores_modes(dir: &Path) -> bool {
        let probe = dir.join(".mode-probe");
        std::fs::create_dir_all(&probe).expect("probe dir");
        std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        let ignored = std::fs::read_dir(&probe).is_ok();
        std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        std::fs::remove_dir(&probe).expect("rm probe");
        ignored
    }

    fn detail(err: CoreError) -> String {
        match err {
            CoreError::TransportFailed { detail, .. } => detail,
            other => panic!("expected transport_failed, got {other:?}"),
        }
    }

    fn save_dir(n: &Node) -> PathBuf {
        n.home
            .path()
            .join("ign-deploy")
            .join(GATEWAY)
            .join("projects")
            .join(PROJECT)
            .join("git-save")
            .join(RUN)
    }

    #[tokio::test]
    async fn sftp_git_state_posix_managed() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        // markers the host may report, and a loose ref in a nested directory
        std::fs::write(fx.git_dir().join("index.lock"), "").expect("lock");
        std::fs::write(fx.git_dir().join("MERGE_HEAD"), "x\n").expect("merge");
        fx.git_in_project(&["branch", "feature/x"]);
        fx.git_in_project(&["branch", "feature-y"]);

        let local: GitFiles = fx.node().read_git_state(PROJECT).await.expect("local");
        let posix: GitFiles = n.t.read_git_state(PROJECT).await.expect("posix");
        assert_eq!(posix, local, "the two implementations read the same state");
        assert_eq!(posix.kind, GitEntry::Directory);
        assert!(posix.index_present);
        assert!(
            posix.markers.contains(&"index.lock".to_string()),
            "{posix:?}"
        );
        assert!(
            posix.markers.contains(&"MERGE_HEAD".to_string()),
            "{posix:?}"
        );
        assert!(
            posix
                .loose_refs
                .iter()
                .any(|(name, _)| name == "refs/heads/feature/x"),
            "{posix:?}"
        );
        // a probe and a state call: two exec calls, no sftp session
        assert_eq!(n.count.execs(), 2);
        assert_eq!(n.count.sftp(), 0);

        // not a directory, and no project: answered by kind, never read
        std::fs::remove_dir_all(fx.git_dir()).expect("rm .git");
        let absent = n.t.read_git_state(PROJECT).await.expect("absent");
        assert_eq!(absent, GitFiles::without_contents(GitEntry::Absent));
        std::fs::write(fx.git_dir(), "gitdir: elsewhere\n").expect("git file");
        let file = n.t.read_git_state(PROJECT).await.expect("file");
        assert_eq!(file, GitFiles::without_contents(GitEntry::File));
    }

    #[tokio::test]
    async fn sftp_git_state_posix_failure_not_absent() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        if ignores_modes(&fx.data_dir) {
            println!("running with privileges that ignore mode bits; denial cases skipped");
            return;
        }
        // a config the account cannot read is a failure, never an absent file
        let config = fx.git_dir().join("config");
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        let why = detail(
            n.t.read_git_state(PROJECT)
                .await
                .expect_err("denied config"),
        );
        assert!(why.contains("config"), "{why}");
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o644)).expect("chmod");

        // a refs directory it cannot list is a failure too
        let heads = fx.git_dir().join("refs/heads");
        std::fs::set_permissions(&heads, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        let err = n.t.read_git_state(PROJECT).await.expect_err("denied refs");
        std::fs::set_permissions(&heads, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");

        // a symlinked HEAD is refused
        let head = fx.git_dir().join("HEAD");
        std::fs::rename(&head, fx.git_dir().join("HEAD.real")).expect("rename");
        std::os::unix::fs::symlink("HEAD.real", &head).expect("symlink");
        let why = detail(n.t.read_git_state(PROJECT).await.expect_err("symlink"));
        assert!(why.contains("symlink"), "{why}");
        std::fs::remove_file(&head).expect("rm link");
        std::fs::rename(fx.git_dir().join("HEAD.real"), &head).expect("rename back");
        n.t.read_git_state(PROJECT).await.expect("readable again");
    }

    #[tokio::test]
    async fn sftp_git_save_restore_posix() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let local_manifest = fx
            .node()
            .git_manifest(PROJECT)
            .await
            .expect("local manifest");
        let before = n.t.git_manifest(PROJECT).await.expect("manifest");
        assert_eq!(before, local_manifest, "both implementations digest alike");

        let saved = n.t.save_git(GATEWAY, PROJECT, RUN).await.expect("save");
        assert_eq!(saved, before, "the save equals the gateway .git");
        assert_eq!(
            n.t.save_manifest(GATEWAY, PROJECT, RUN)
                .await
                .expect("save manifest"),
            before
        );
        let run_mode = std::fs::metadata(save_dir(&n))
            .expect("run dir")
            .permissions()
            .mode();
        assert_eq!(run_mode & 0o077, 0, "the save is private: {run_mode:o}");
        // a save is never overwritten
        let why = detail(
            n.t.save_git(GATEWAY, PROJECT, RUN)
                .await
                .expect_err("again"),
        );
        assert!(why.contains("never overwritten"), "{why}");

        // restore refuses while `.git` exists
        let why = detail(
            n.t.restore_git(GATEWAY, PROJECT, RUN)
                .await
                .expect_err("present"),
        );
        assert!(why.contains("never replaced"), "{why}");
        assert_eq!(n.t.git_manifest(PROJECT).await.expect("untouched"), before);

        // the import deletes `.git`; the restore puts it back byte for byte
        let config_mode = std::fs::metadata(fx.git_dir().join("config"))
            .expect("config")
            .permissions()
            .mode();
        std::fs::remove_dir_all(fx.git_dir()).expect("rm .git");
        let back =
            n.t.restore_git(GATEWAY, PROJECT, RUN)
                .await
                .expect("restore");
        assert_eq!(back, before);
        assert_eq!(n.t.git_manifest(PROJECT).await.expect("live"), before);
        let restored_mode = std::fs::metadata(fx.git_dir().join("config"))
            .expect("config")
            .permissions()
            .mode();
        assert_eq!(
            restored_mode, config_mode,
            "the modes survive the round trip"
        );
        assert!(
            !fx.project_dir()
                .join(".ign-restore-20261114T101500Z-ab12")
                .exists(),
            "no staging directory is left"
        );

        // removing the save takes the repo tree and its run directory
        n.t.remove_git_save(GATEWAY, PROJECT, RUN)
            .await
            .expect("remove");
        assert!(!save_dir(&n).exists());
        n.t.remove_git_save(GATEWAY, PROJECT, RUN)
            .await
            .expect("absent is fine");
    }

    /// 27-30 (CR-B1): the removal walks the save path by position, so a
    /// gateway or a project named `repo` never turns it into the removal of
    /// the whole ledger (the release record, the journals, the other runs).
    #[tokio::test]
    async fn sftp_git_save_rm_named_repo_components() {
        const OTHER: &str = "20261114T101600Z-cd34";
        for (gateway, project) in [("repo", "p1"), ("gw1", "repo"), ("repo", "repo")] {
            let fx = GitFixture::new();
            let n = node(&fx.data_dir);
            let proj = n
                .home
                .path()
                .join("ign-deploy")
                .join(gateway)
                .join("projects")
                .join(project);
            let save = |run: &str| proj.join("git-save").join(run).join("repo");
            for run in [RUN, OTHER] {
                std::fs::create_dir_all(save(run).join("objects")).expect("save");
                std::fs::write(save(run).join("HEAD"), format!("ref: {run}\n")).expect("head");
                std::fs::write(save(run).join("objects/o1"), run).expect("object");
            }
            std::fs::create_dir_all(proj.join("release")).expect("release");
            std::fs::write(proj.join("release/release.json"), "{\"keep\":1}").expect("record");
            std::fs::create_dir_all(proj.join("journals")).expect("journals");
            std::fs::write(proj.join("journals/j1.json"), "journal").expect("journal");
            let sibling = n.home.path().join("ign-deploy").join(gateway).join("note");
            std::fs::write(&sibling, "gateway-level file").expect("sibling");

            n.t.remove_git_save(gateway, project, RUN)
                .await
                .unwrap_or_else(|e| panic!("{gateway}/{project}: {e:?}"));

            let label = format!("{gateway}/{project}");
            assert!(
                !proj.join("git-save").join(RUN).exists(),
                "{label}: removed"
            );
            assert_eq!(
                std::fs::read_to_string(proj.join("release/release.json")).expect("record"),
                "{\"keep\":1}",
                "{label}: the release record survives"
            );
            assert_eq!(
                std::fs::read_to_string(proj.join("journals/j1.json")).expect("journal"),
                "journal",
                "{label}: the journals survive"
            );
            assert_eq!(
                std::fs::read_to_string(save(OTHER).join("objects/o1")).expect("other save"),
                OTHER,
                "{label}: another run's save survives"
            );
            assert_eq!(
                std::fs::read_to_string(save(OTHER).join("HEAD")).expect("other head"),
                format!("ref: {OTHER}\n")
            );
            assert!(
                sibling.is_file(),
                "{label}: the gateway-level file survives"
            );
        }
    }

    /// 27-30 (WR-B7): a `.git` that appears between the restore's last check
    /// and its move is never entered. The stage is taken back out, the restore
    /// refuses, and the gateway's new `.git` is left as it is.
    #[tokio::test]
    async fn sftp_git_restore_refuses_git_that_appears_before_the_move() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let before = n.t.save_git(GATEWAY, PROJECT, RUN).await.expect("save");
        std::fs::remove_dir_all(fx.git_dir()).expect("rm .git");

        // a designer's open creates `.git` right before the restore's `mv`
        let shims = common::posix_exec::Shims::new();
        shims.add(
            "mv",
            "case $3 in */.git) case $2 in */.ign-restore-*) mkdir -- \"$3\";; esac;; esac\n\
             exec \"$(real mv)\" \"$@\"",
        );
        n.count.use_shims(&shims);

        let why = detail(
            n.t.restore_git(GATEWAY, PROJECT, RUN)
                .await
                .expect_err("a .git appeared"),
        );
        assert!(why.contains(".git appeared"), "{why}");
        assert!(fx.git_dir().is_dir(), "the new .git is still there");
        assert_eq!(
            std::fs::read_dir(fx.git_dir()).expect("new .git").count(),
            0,
            "nothing was left inside the new .git"
        );
        let beside: Vec<_> = std::fs::read_dir(fx.project_dir())
            .expect("project")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".ign-restore-"))
            .collect();
        assert!(beside.is_empty(), "no stage is left beside it: {beside:?}");
        let saved = save_dir(&n).join("repo");
        assert!(saved.is_dir(), "the save is intact");
        assert_eq!(
            n.t.save_manifest(GATEWAY, PROJECT, RUN)
                .await
                .expect("save manifest"),
            before
        );
    }

    /// The shims that make the project directory belong to a third account
    /// (uid 4242), accept the ACL step and report what `getfacl` is told to.
    fn acl_shims(file: &str, dir: &str) -> common::posix_exec::Shims {
        let shims = common::posix_exec::Shims::new();
        shims.add(
            "ls",
            "case $1 in -ldn) printf 'drwxr-xr-x 2 4242 4242 64 Jan  1 00:00 %s\\n' \"$3\";; \
             *) exec \"$(real ls)\" \"$@\";; esac",
        );
        shims.add("setfacl", "exit 0");
        shims.add(
            "getfacl",
            "for a; do p=$a; done\n\
             if [ -d \"$p\" ]; then cat \"$(dirname \"$0\")/getfacl.dir\"; \
             else cat \"$(dirname \"$0\")/getfacl.file\"; fi",
        );
        shims.data("getfacl.file", file);
        shims.data("getfacl.dir", dir);
        shims
    }

    const RW: &str = "user:4242:rwx\t#effective:rw-\n";
    const RW_NO_SUFFIX: &str = "user:4242:rw-\n";
    const RX_TAB: &str = "user:4242:rw-\t#effective:r-x\n";
    const RX_SPACE: &str = "user:4242:rwx #effective:r-x\n";

    /// 27-30 (WR-B4): the probe asks whether the gateway account can write,
    /// so `#effective:r-x` refuses too, on a file and on a directory.
    #[tokio::test]
    async fn sftp_git_probe_refuses_effective_no_write() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        for (file, dir, label) in [
            (RX_TAB, RW, "file r-x with a tab"),
            (RX_SPACE, RW, "file r-x with a space"),
            (RW, RX_TAB, "directory r-x"),
            (RW, RX_SPACE, "directory r-x with a space"),
        ] {
            let shims = acl_shims(file, dir);
            n.count.use_shims(&shims);
            let why = detail(
                n.t.probe_write(PROJECT, RUN)
                    .await
                    .expect_err(&format!("{label} must refuse")),
            );
            assert!(why.contains("no effective write access"), "{label}: {why}");
        }
        // effective rw-, or no effective suffix at all, passes and cleans up
        for (file, dir) in [(RW, RW), (RW_NO_SUFFIX, RW_NO_SUFFIX)] {
            let shims = acl_shims(file, dir);
            n.count.use_shims(&shims);
            n.t.probe_write(PROJECT, RUN).await.expect("probe passes");
        }
        let probe = format!(".ign-probe-{RUN}");
        assert!(!fx.project_dir().join(&probe).exists());
        assert!(!fx.git_dir().join(&probe).exists());
        assert!(!fx.git_dir().join(format!("{probe}.acl")).exists());
    }

    /// 27-30 (WR-B4): the same on the restore. Refused, with no `.git`
    /// created and no stage left; with a writable effective right it lands.
    #[tokio::test]
    async fn sftp_git_restore_refuses_effective_no_write() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let before = n.t.save_git(GATEWAY, PROJECT, RUN).await.expect("save");
        std::fs::remove_dir_all(fx.git_dir()).expect("rm .git");
        for (file, dir, label) in [
            (RX_TAB, RW, "file r-x with a tab"),
            (RX_SPACE, RW, "file r-x with a space"),
            (RW, RX_TAB, "directory r-x"),
        ] {
            let shims = acl_shims(file, dir);
            n.count.use_shims(&shims);
            let why = detail(
                n.t.restore_git(GATEWAY, PROJECT, RUN)
                    .await
                    .expect_err(&format!("{label} must refuse")),
            );
            assert!(why.contains("no effective write access"), "{label}: {why}");
            assert!(!fx.git_dir().exists(), "{label}: no .git was created");
            let beside: Vec<_> = std::fs::read_dir(fx.project_dir())
                .expect("project")
                .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
                .filter(|name| name.starts_with(".ign-restore-"))
                .collect();
            assert!(beside.is_empty(), "{label}: no stage is left: {beside:?}");
        }
        let shims = acl_shims(RW, RW);
        n.count.use_shims(&shims);
        let back =
            n.t.restore_git(GATEWAY, PROJECT, RUN)
                .await
                .expect("writable restores");
        assert_eq!(back, before);
    }

    #[tokio::test]
    async fn sftp_git_save_refuses_a_symlink_and_leaves_no_save() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        std::os::unix::fs::symlink("/etc", fx.git_dir().join("hooks-link")).expect("symlink");
        let why = detail(
            n.t.save_git(GATEWAY, PROJECT, RUN)
                .await
                .expect_err("symlink"),
        );
        assert!(why.contains("symlink"), "{why}");
        assert!(!save_dir(&n).join("repo").exists(), "no half save");
        // the manifest refuses it as well
        let err = n.t.git_manifest(PROJECT).await.expect_err("symlink");
        assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
        // and a symlinked project directory is never walked through
        std::fs::remove_file(fx.git_dir().join("hooks-link")).expect("rm link");
        let real = fx.data_dir.join("projects").join("Real");
        std::fs::rename(fx.project_dir(), &real).expect("rename");
        std::os::unix::fs::symlink(&real, fx.project_dir()).expect("project link");
        let err =
            n.t.save_git(GATEWAY, PROJECT, RUN)
                .await
                .expect_err("linked project");
        assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
    }

    #[tokio::test]
    async fn sftp_git_write_lock_posix() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let pack = "ab".repeat(20);
        let writes = vec![
            GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::BranchRef("feature/new".to_string()),
                bytes: format!("{}\n", fx.c2).into_bytes(),
            },
            GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::PackFile(pack.clone()),
                bytes: vec![7u8; 5000],
            },
            GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::Head,
                bytes: b"ref: refs/heads/feature/new\n".to_vec(),
            },
        ];
        n.t.write_git_files(PROJECT, &writes).await.expect("write");
        for write in &writes {
            let path = fx.git_dir().join(write.name.rel());
            assert_eq!(std::fs::read(&path).expect("read back"), write.bytes);
            let digest =
                n.t.git_file_digest(PROJECT, &write.name)
                    .await
                    .expect("digest");
            assert_eq!(digest, Some(sha256_hex(&write.bytes)), "{:?}", write.name);
        }
        let leftovers: Vec<String> = walk(&fx.git_dir())
            .into_iter()
            .filter(|p| p.ends_with(".lock") || p.ends_with(".ign-tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        assert!(stages(&n).is_empty(), "no stage is left: {:?}", stages(&n));
        // the ref file is a regular readable file, not private to the writer
        let mode = std::fs::metadata(fx.git_dir().join("refs/heads/feature/new"))
            .expect("ref")
            .permissions()
            .mode();
        assert_eq!(mode & 0o044, 0o044, "{mode:o}");
        // an absent file has no digest
        assert_eq!(
            n.t.git_file_digest(PROJECT, &GitWriteName::BranchRef("nope".to_string()))
                .await
                .expect("absent"),
            None
        );

        // an existing lock refuses and nothing is written, no lock of this
        // call is left behind
        let head_before = std::fs::read(fx.git_dir().join("HEAD")).expect("head");
        std::fs::write(fx.git_dir().join("config.lock"), "held").expect("lock");
        let again = vec![
            GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::Head,
                bytes: b"ref: refs/heads/other\n".to_vec(),
            },
            GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::Config,
                bytes: b"[core]\n".to_vec(),
            },
        ];
        let why = detail(
            n.t.write_git_files(PROJECT, &again)
                .await
                .expect_err("held"),
        );
        assert!(why.contains("config.lock"), "{why}");
        assert_eq!(
            std::fs::read(fx.git_dir().join("HEAD")).expect("head"),
            head_before
        );
        assert!(
            !fx.git_dir().join("HEAD.lock").exists(),
            "no lock of the refused call"
        );
        assert_eq!(
            std::fs::read_to_string(fx.git_dir().join("config.lock")).expect("lock"),
            "held",
            "the other writer's lock is left alone"
        );
        assert!(
            stages(&n).is_empty(),
            "a refusal leaves no stage: {:?}",
            stages(&n)
        );
        // a pack name that is not a hex digest never reaches the host
        let calls = n.count.execs();
        let bad = vec![GitWrite {
            old: Default::default(),
            packed_refs_old: None,
            name: GitWriteName::PackFile("../x".to_string()),
            bytes: vec![1],
        }];
        n.t.write_git_files(PROJECT, &bad)
            .await
            .expect_err("bad shape");
        assert_eq!(n.count.execs(), calls, "refused before any remote call");
    }

    /// The `git-stage-*` directories in the account's ledger root.
    fn stages(n: &Node) -> Vec<String> {
        match std::fs::read_dir(n.home.path().join("ign-deploy")) {
            Ok(entries) => entries
                .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
                .filter(|name| name.starts_with("git-stage-"))
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    fn two_writes(fx: &GitFixture) -> Vec<GitWrite> {
        vec![
            GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::Head,
                bytes: b"ref: refs/heads/feature/new\n".to_vec(),
            },
            GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::BranchRef("feature/new".to_string()),
                bytes: format!("{}\n", fx.c2).into_bytes(),
            },
        ]
    }

    /// 27-31 (WR-B3): sftp puts the advance's bytes only into a private stage
    /// in the deploy account's ledger root, never into a path the gateway
    /// account can change.
    #[tokio::test]
    async fn sftp_git_write_stage_is_private() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let writes = two_writes(&fx);
        n.t.write_git_files(PROJECT, &writes).await.expect("write");
        for write in &writes {
            assert_eq!(
                std::fs::read(fx.git_dir().join(write.name.rel())).expect("read back"),
                write.bytes
            );
        }
        let puts = n.count.puts();
        assert_eq!(puts.len(), writes.len(), "one put per write: {puts:?}");
        let stage_root = format!("{}/ign-deploy/git-stage-", n.home.path().display());
        let data = fx.data_dir.display().to_string();
        for (remote, parent_mode) in &puts {
            assert!(
                remote.starts_with(&stage_root),
                "{remote} is not below a stage in the ledger root"
            );
            assert!(!remote.starts_with(&data), "{remote} is below the data dir");
            assert_eq!(*parent_mode, Some(0o700), "the stage is owner-only");
        }
        let ledger_mode = std::fs::metadata(n.home.path().join("ign-deploy"))
            .expect("ledger root")
            .permissions()
            .mode();
        assert_eq!(ledger_mode & 0o077, 0, "{ledger_mode:o}");
        assert!(stages(&n).is_empty(), "the stage is gone: {:?}", stages(&n));
    }

    /// 27-31 (WR-B3): a symlink at a lock path is never written through. The
    /// link appears after ign has looked, while the bytes are on their way.
    #[tokio::test]
    async fn sftp_git_write_never_follows_a_planted_link() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let outside = tempfile::tempdir().expect("outside");
        let victim = outside.path().join("victim");
        std::fs::write(&victim, "precious\n").expect("victim");
        let head_before = std::fs::read(fx.git_dir().join("HEAD")).expect("head");

        let git = fx.git_dir();
        let link_to = victim.clone();
        n.count.on_batch(move |_| {
            let lock = git.join("HEAD.lock");
            let _ = std::fs::remove_file(&lock);
            std::os::unix::fs::symlink(&link_to, &lock).expect("plant");
        });
        let why = detail(
            n.t.write_git_files(PROJECT, &two_writes(&fx))
                .await
                .expect_err("a link at the lock path"),
        );
        assert!(why.contains("exit code 4"), "refused as a refusal: {why}");
        assert!(why.contains("HEAD.lock"), "{why}");
        assert_eq!(
            std::fs::read_to_string(&victim).expect("victim"),
            "precious\n",
            "nothing was written through the link"
        );
        assert_eq!(
            std::fs::read(fx.git_dir().join("HEAD")).expect("head"),
            head_before,
            "no target changed"
        );
        assert!(
            !fx.git_dir().join("refs/heads/feature/new").exists(),
            "no target changed"
        );
        assert!(
            std::fs::symlink_metadata(fx.git_dir().join("HEAD.lock"))
                .expect("planted link")
                .file_type()
                .is_symlink(),
            "what ign did not create is left alone"
        );
        assert!(stages(&n).is_empty(), "{:?}", stages(&n));
    }

    /// 27-31 (WR-B3): the exclusive create itself does not follow a link that
    /// is planted after the checks and before the create.
    #[tokio::test]
    async fn sftp_git_write_create_refuses_a_link_planted_in_the_window() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let outside = tempfile::tempdir().expect("outside");
        let victim = outside.path().join("victim");
        std::fs::write(&victim, "precious\n").expect("victim");
        let head_before = std::fs::read(fx.git_dir().join("HEAD")).expect("head");

        // while the first lock is filled, the gateway account plants a link at
        // the second lock's path
        let shims = common::posix_exec::Shims::new();
        shims.add(
            "cat",
            &format!(
                "case $2 in */git-stage-*/0) ln -s {} {}/refs/heads/feature/new.lock;; esac\n\
                 exec \"$(real cat)\" \"$@\"",
                victim.display(),
                fx.git_dir().display()
            ),
        );
        n.count.use_shims(&shims);
        let why = detail(
            n.t.write_git_files(PROJECT, &two_writes(&fx))
                .await
                .expect_err("a link planted in the window"),
        );
        assert!(why.contains("exit code 4"), "{why}");
        assert_eq!(
            std::fs::read_to_string(&victim).expect("victim"),
            "precious\n",
            "the create did not follow the link"
        );
        assert!(
            !fx.git_dir().join("HEAD.lock").exists(),
            "the lock ign made is taken away again"
        );
        assert_eq!(
            std::fs::read(fx.git_dir().join("HEAD")).expect("head"),
            head_before
        );
        assert!(
            std::fs::symlink_metadata(fx.git_dir().join("refs/heads/feature/new.lock"))
                .expect("planted link")
                .file_type()
                .is_symlink()
        );
        assert!(stages(&n).is_empty(), "{:?}", stages(&n));
    }

    /// 27-31 (WR-B2): a connection that dies while the write script runs
    /// leaves an error that names every lock and the stage, and a cleanup that
    /// removes the locks the write script made (CodeRabbit #56: by identity
    /// with its private copies, not by their bytes).
    #[tokio::test]
    async fn sftp_git_write_transport_failure_names_locks() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let writes = two_writes(&fx);
        // the script dies after every lock was made and filled, at its first
        // move over a target (its shell is killed: no answer comes back)
        let shims = common::posix_exec::Shims::new();
        shims.add("mv", "kill -9 $PPID");
        n.count.use_shims(&shims);
        let why = detail(
            n.t.write_git_files(PROJECT, &writes)
                .await
                .expect_err("a dead connection"),
        );
        assert!(
            why.contains("may still be in .git"),
            "the leftovers are announced: {why}"
        );
        for lock in ["HEAD.lock", "refs/heads/feature/new.lock"] {
            assert!(why.contains(lock), "{lock} is named: {why}");
        }
        assert!(why.contains("git-stage-"), "the stage is named: {why}");
        assert!(
            why.contains("removed 2 lock(s) that ign made"),
            "both locks were there and were the script's: {why}"
        );
        for lock in ["HEAD.lock", "refs/heads/feature/new.lock"] {
            assert!(
                !fx.git_dir().join(lock).exists(),
                "{lock}, made by the write script, was removed by the cleanup"
            );
        }
        let private: Vec<_> = std::fs::read_dir(fx.git_dir())
            .expect("ls .git")
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with(".ign-write-"))
            .collect();
        assert!(private.is_empty(), "the private copies are gone");
        assert!(stages(&n).is_empty(), "the cleanup took the stage away");

        // a cleanup that dies as well is told in the same error, and the stage
        // it could not remove is still named
        let n = node(&fx.data_dir);
        n.count.kill_exec("HEAD.lock", 2);
        let why = detail(
            n.t.write_git_files(PROJECT, &writes)
                .await
                .expect_err("a dead connection twice"),
        );
        assert!(
            why.contains("cleanup"),
            "the failed cleanup is folded in: {why}"
        );
        assert!(why.contains("HEAD.lock"), "{why}");
        assert!(why.contains("git-stage-"), "{why}");
        assert_eq!(stages(&n).len(), 1, "the stage really is still there");
    }

    /// 27-31 (WR-B2): a lock that was there before the write and holds other
    /// bytes (a JGit `index.lock`) is never removed, by a refusal or by the
    /// cleanup after a dead connection.
    #[tokio::test]
    async fn sftp_git_write_cleanup_spares_a_foreign_lock() {
        let fx = GitFixture::new();
        let writes = vec![
            GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::Head,
                bytes: b"ref: refs/heads/feature/new\n".to_vec(),
            },
            GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::Index,
                bytes: vec![9u8; 64],
            },
        ];
        let foreign = fx.git_dir().join("index.lock");

        // refused: nothing written, the foreign lock stays
        let n = node(&fx.data_dir);
        std::fs::write(&foreign, "JGit holds this\n").expect("foreign lock");
        let why = detail(
            n.t.write_git_files(PROJECT, &writes)
                .await
                .expect_err("held"),
        );
        assert!(why.contains("index.lock"), "{why}");
        assert_eq!(
            std::fs::read_to_string(&foreign).expect("foreign"),
            "JGit holds this\n"
        );
        assert!(!fx.git_dir().join("HEAD.lock").exists());
        assert!(stages(&n).is_empty());

        // a dead connection: the cleanup runs and spares both foreign locks,
        // the one with other bytes and the one that holds exactly ign's bytes
        // (CodeRabbit #56: equal bytes are not ownership; ign did not make it)
        let n = node(&fx.data_dir);
        std::fs::write(fx.git_dir().join("HEAD.lock"), &writes[0].bytes).expect("same bytes");
        n.count.kill_exec("index.lock", 1);
        let why = detail(
            n.t.write_git_files(PROJECT, &writes)
                .await
                .expect_err("a dead connection"),
        );
        assert!(why.contains("index.lock"), "{why}");
        assert_eq!(
            std::fs::read_to_string(&foreign).expect("foreign"),
            "JGit holds this\n",
            "a lock with other bytes is never removed"
        );
        assert_eq!(
            std::fs::read(fx.git_dir().join("HEAD.lock")).expect("kept"),
            writes[0].bytes,
            "a lock ign did not make is never removed, whatever it holds"
        );
        assert!(stages(&n).is_empty());
    }

    /// 27-31 (security review 1): a lock path that holds a symlink at the
    /// moment the lock is created is refused whatever it points at, the shell
    /// does not matter: a dangling link, a link to a FIFO (a noclobber
    /// redirect opens a non-regular file without `O_EXCL`) and a link to a
    /// directory. Nothing is written through it.
    #[tokio::test]
    async fn sftp_git_write_create_refuses_a_planted_link_of_any_kind() {
        use std::io::Read as _;
        use std::os::unix::fs::OpenOptionsExt;
        for kind in ["dangling", "fifo", "dir"] {
            let fx = GitFixture::new();
            let n = node(&fx.data_dir);
            let outside = tempfile::tempdir().expect("outside");
            let aim = outside.path().join("aim");
            let mut reader = None;
            match kind {
                "dangling" => {}
                "fifo" => {
                    let made = std::process::Command::new("mkfifo")
                        .arg(&aim)
                        .status()
                        .expect("mkfifo");
                    assert!(made.success());
                    // a reader, so an open for writing would not block
                    reader = Some(
                        std::fs::OpenOptions::new()
                            .read(true)
                            .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
                            .open(&aim)
                            .expect("fifo reader"),
                    );
                }
                _ => std::fs::create_dir(&aim).expect("aim dir"),
            }
            let head_before = std::fs::read(fx.git_dir().join("HEAD")).expect("head");
            let shims = common::posix_exec::Shims::new();
            shims.add(
                "cat",
                &format!(
                    "case $2 in */git-stage-*/0) ln -s {} {}/refs/heads/feature/new.lock;; esac\n\
                     exec \"$(real cat)\" \"$@\"",
                    aim.display(),
                    fx.git_dir().display()
                ),
            );
            n.count.use_shims(&shims);
            let why = detail(
                n.t.write_git_files(PROJECT, &two_writes(&fx))
                    .await
                    .expect_err(kind),
            );
            assert!(why.contains("exit code 4"), "{kind}: {why}");
            if let Some(mut reader) = reader {
                let mut got = Vec::new();
                let _ = reader.read_to_end(&mut got);
                assert!(got.is_empty(), "{kind}: bytes reached the FIFO: {got:?}");
            }
            if kind == "dir" {
                assert_eq!(
                    std::fs::read_dir(&aim).expect("aim").count(),
                    0,
                    "nothing was put inside the directory the link points at"
                );
            } else if kind == "dangling" {
                assert!(!aim.exists(), "the link's target was never created");
            }
            assert_eq!(
                std::fs::read(fx.git_dir().join("HEAD")).expect("head"),
                head_before
            );
            assert!(
                !fx.git_dir().join("HEAD.lock").exists(),
                "{kind}: our lock is taken away"
            );
            assert!(
                std::fs::symlink_metadata(fx.git_dir().join("refs/heads/feature/new.lock"))
                    .expect("planted link")
                    .file_type()
                    .is_symlink(),
                "{kind}: what ign did not create is left alone"
            );
            assert!(stages(&n).is_empty(), "{kind}: {:?}", stages(&n));
        }
    }

    /// 27-31 (security review 2): a parent directory that is swapped for a
    /// symlink after it was checked is never written through, neither by the
    /// create nor by the final move.
    #[tokio::test]
    async fn sftp_git_write_parent_swapped_after_its_check_is_refused() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let git = fx.git_dir();
        std::fs::create_dir_all(git.join("refs/heads/sub")).expect("sub");
        let outside = tempfile::tempdir().expect("outside");
        let victim = outside.path().join("x");
        std::fs::write(&victim, "precious\n").expect("victim");
        let head_before = std::fs::read(git.join("HEAD")).expect("head");
        let shims = common::posix_exec::Shims::new();
        shims.add(
            "cat",
            &format!(
                "case $2 in */git-stage-*/0) cd {g}/refs/heads && mv sub sub.real && ln -s {o} sub;; esac\n\
                 exec \"$(real cat)\" \"$@\"",
                g = git.display(),
                o = outside.path().display()
            ),
        );
        n.count.use_shims(&shims);
        let writes = vec![
            GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::Head,
                bytes: b"ref: refs/heads/sub/x\n".to_vec(),
            },
            GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::BranchRef("sub/x".to_string()),
                bytes: format!("{}\n", fx.c2).into_bytes(),
            },
        ];
        let why = detail(
            n.t.write_git_files(PROJECT, &writes)
                .await
                .expect_err("a swapped parent"),
        );
        assert!(
            why.contains("exit code 4") || why.contains("exit code 3"),
            "{why}"
        );
        assert_eq!(
            std::fs::read_to_string(&victim).expect("victim"),
            "precious\n",
            "nothing was written through the swapped parent"
        );
        assert!(
            std::fs::read_dir(outside.path()).expect("outside").count() == 1,
            "no lock or temporary was made through it"
        );
        assert_eq!(std::fs::read(git.join("HEAD")).expect("head"), head_before);
        assert!(!git.join("HEAD.lock").exists());
        assert!(stages(&n).is_empty(), "{:?}", stages(&n));
    }

    /// The sha256 the engine would carry as the old value of `path`.
    fn digest_of(path: &Path) -> OldValue {
        OldValue::Sha256(sha256_hex(
            &std::fs::read(path).expect("read the old value"),
        ))
    }

    const MOVED: &[u8] = b"1111111111111111111111111111111111111111\n";
    const DESIGNER: &[u8] = b"2222222222222222222222222222222222222222\n";

    /// HEAD (rewritten with its own bytes) and the production branch (moved to
    /// `MOVED`), each carrying the value read now.
    fn compared_writes(fx: &GitFixture) -> Vec<GitWrite> {
        let git = fx.git_dir();
        vec![
            GitWrite {
                name: GitWriteName::Head,
                bytes: std::fs::read(git.join("HEAD")).expect("head"),
                old: digest_of(&git.join("HEAD")),
                packed_refs_old: None,
            },
            GitWrite {
                name: GitWriteName::BranchRef("production".to_string()),
                bytes: MOVED.to_vec(),
                old: digest_of(&git.join("refs/heads/production")),
                packed_refs_old: None,
            },
        ]
    }

    /// Nothing of the refused call is left in `.git` or in the account home.
    fn assert_no_leftovers(n: &Node, fx: &GitFixture, what: &str) {
        let left: Vec<String> = walk(&fx.git_dir())
            .into_iter()
            .filter(|p| p.ends_with(".lock") || p.contains(".ign-write-"))
            .collect();
        assert!(left.is_empty(), "{what}: {left:?}");
        assert!(stages(n).is_empty(), "{what}: {:?}", stages(n));
    }

    fn assert_changed(err: CoreError, names: &str) {
        match err {
            CoreError::TransportFailed {
                operation, detail, ..
            } => {
                assert_eq!(operation, GIT_CHANGED_OPERATION, "{detail}");
                assert!(detail.contains(names), "names the file: {detail}");
            }
            other => panic!("expected the changed-target failure, got {other:?}"),
        }
    }

    /// 27-46 (F4, POSIX arm): a ref rewritten after the run read it is never
    /// overwritten. A `ln` shim changes the ref after the plan and before the
    /// script compares (the lock of the branch is the call it waits for); the
    /// write fails as `GIT_CHANGED_OPERATION`, the writer's value stays, this
    /// script's locks and its stage are gone, and the write before it stays.
    #[tokio::test]
    async fn sftp_git_write_refuses_a_changed_target() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let git = fx.git_dir();
        let writes = compared_writes(&fx);
        let shims = common::posix_exec::Shims::new();
        shims.add(
            "ln",
            &format!(
                "case \"$*\" in *production.lock) printf '2222222222222222222222222222222222222222\\n' > {}/refs/heads/production;; esac\n\
                 exec \"$(real ln)\" \"$@\"",
                git.display()
            ),
        );
        n.count.use_shims(&shims);
        let err =
            n.t.write_git_files(PROJECT, &writes)
                .await
                .expect_err("a changed ref is refused");
        assert_changed(err, "refs/heads/production");
        assert_eq!(
            std::fs::read(git.join("refs/heads/production")).expect("ref"),
            DESIGNER,
            "the other writer's value stays"
        );
        assert_no_leftovers(&n, &fx, "a refused write");
    }

    /// 27-46: an old value of absent holds only while the ref is absent; a ref
    /// that appeared by the move step is refused the same way and kept.
    #[tokio::test]
    async fn sftp_git_write_refuses_a_ref_that_appeared() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let git = fx.git_dir();
        let writes = vec![GitWrite {
            name: GitWriteName::BranchRef("feature/new".to_string()),
            bytes: MOVED.to_vec(),
            old: OldValue::Absent,
            packed_refs_old: None,
        }];
        let shims = common::posix_exec::Shims::new();
        shims.add(
            "ln",
            &format!(
                "case \"$*\" in *new.lock) printf 'appeared\\n' > {}/refs/heads/feature/new;; esac\n\
                 exec \"$(real ln)\" \"$@\"",
                git.display()
            ),
        );
        n.count.use_shims(&shims);
        let err =
            n.t.write_git_files(PROJECT, &writes)
                .await
                .expect_err("a ref that appeared is refused");
        assert_changed(err, "refs/heads/feature/new");
        assert_eq!(
            std::fs::read_to_string(git.join("refs/heads/feature/new")).expect("ref"),
            "appeared\n"
        );
        assert_no_leftovers(&n, &fx, "a refused write");
    }

    /// 27-46: a branch that lives only in `packed-refs` carries the digest of
    /// `packed-refs`; a `packed-refs` that changed refuses, and nothing is
    /// moved.
    #[tokio::test]
    async fn sftp_git_write_refuses_a_changed_packed_refs() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let git = fx.git_dir();
        assert!(
            !git.join("refs/remotes/capapp/production").exists(),
            "the remote branch is packed only"
        );
        let writes = vec![GitWrite {
            name: GitWriteName::RemoteRef {
                remote: "capapp".to_string(),
                branch: "production".to_string(),
            },
            bytes: MOVED.to_vec(),
            old: OldValue::Absent,
            packed_refs_old: Some(digest_of(&git.join("packed-refs"))),
        }];
        let shims = common::posix_exec::Shims::new();
        shims.add(
            "ln",
            &format!(
                "case \"$*\" in *production.lock) printf '# gc ran\\n' >> {}/packed-refs;; esac\n\
                 exec \"$(real ln)\" \"$@\"",
                git.display()
            ),
        );
        n.count.use_shims(&shims);
        let err =
            n.t.write_git_files(PROJECT, &writes)
                .await
                .expect_err("a changed packed-refs is refused");
        assert_changed(err, "packed-refs");
        assert!(
            !git.join("refs/remotes/capapp/production").exists(),
            "nothing was moved"
        );
        assert_no_leftovers(&n, &fx, "a refused write");
    }

    /// 27-46: exit 7 of the write script is settled: the script cleaned up
    /// itself, so no cleanup command is sent after it.
    #[tokio::test]
    async fn sftp_git_write_exit_7_is_settled() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let git = fx.git_dir();
        let shims = common::posix_exec::Shims::new();
        shims.add(
            "ln",
            &format!(
                "case \"$*\" in *production.lock) printf '2222222222222222222222222222222222222222\\n' > {}/refs/heads/production;; esac\n\
                 exec \"$(real ln)\" \"$@\"",
                git.display()
            ),
        );
        n.count.use_shims(&shims);
        n.t.write_git_files(PROJECT, &compared_writes(&fx))
            .await
            .expect_err("a changed ref is refused");
        let sent = n.count.execs.lock().expect("execs").clone();
        assert!(
            sent.iter().any(|c| c.contains("changed after the plan")),
            "the write script ran: {sent:?}"
        );
        assert!(
            !sent
                .iter()
                .any(|c| c.contains("the locks could not be checked")),
            "no cleanup ran after exit 7: {sent:?}"
        );
    }

    /// 27-46: a write with no expectation (a pack) moves without a compare
    /// and without a hash tool; a write that carries a digest does call one.
    #[tokio::test]
    async fn sftp_git_write_unchecked_pack_is_not_hashed() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let log = fx.data_dir.join("hash-calls.log");
        let shims = common::posix_exec::Shims::new();
        shims.add(
            "sha256sum",
            &format!(
                "echo sha256sum >> {}\n\
                 if [ -n \"$(real sha256sum)\" ]; then exec \"$(real sha256sum)\" \"$@\"; fi\n\
                 exec \"$(real shasum)\" -a 256 \"$@\"",
                log.display()
            ),
        );
        shims.add(
            "shasum",
            &format!(
                "echo shasum >> {}\nexec \"$(real shasum)\" \"$@\"",
                log.display()
            ),
        );
        n.count.use_shims(&shims);
        let pack = GitWrite {
            name: GitWriteName::PackFile("ab".repeat(20)),
            bytes: vec![7u8; 4000],
            old: OldValue::Unchecked,
            packed_refs_old: None,
        };
        n.t.write_git_files(PROJECT, std::slice::from_ref(&pack))
            .await
            .expect("an unchecked pack moves");
        assert!(
            fx.git_dir().join(pack.name.rel()).is_file(),
            "the pack is in place"
        );
        assert!(!log.exists(), "no hash tool ran for an unchecked write");
        // a write that carries a digest does hash: the shim is wired
        n.t.write_git_files(PROJECT, &compared_writes(&fx))
            .await
            .expect("an unchanged repo is written");
        assert!(log.exists(), "a compared write hashes");
        assert_no_leftovers(&n, &fx, "the writes");
    }

    /// 27-46 (T-27-165): a malformed old-value field never reaches the host:
    /// the transport refuses a digest that is not 64 lowercase hex before the
    /// first remote call.
    #[tokio::test]
    async fn sftp_git_write_refuses_a_malformed_old_digest() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let calls = n.count.execs();
        for bad in [
            "",
            "AB",
            &"A".repeat(64),
            &"g".repeat(64),
            "$(touch x)",
            &"a".repeat(63),
        ] {
            let write = GitWrite {
                name: GitWriteName::Head,
                bytes: b"ref: refs/heads/production\n".to_vec(),
                old: OldValue::Sha256(bad.to_string()),
                packed_refs_old: None,
            };
            let why = detail(n.t.write_git_files(PROJECT, &[write]).await.expect_err(bad));
            assert!(why.contains("digest"), "{bad:?}: {why}");
        }
        assert_eq!(n.count.execs(), calls, "nothing reached the host");
    }

    fn mode_of(path: &Path) -> u32 {
        std::fs::metadata(path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777
    }

    /// 27-31 (WR-B1): an advanced file never gets a wider mode than the file it
    /// replaces, and a lock is never wider than its final mode.
    #[tokio::test]
    async fn sftp_git_write_keeps_target_modes() {
        use ignition_core::deploy::git_advance::GitWriteName::{
            BranchRef, Config, Head, RemoteRef,
        };
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let git = fx.git_dir();
        let set = |rel: &str, mode: u32| {
            std::fs::set_permissions(git.join(rel), std::fs::Permissions::from_mode(mode))
                .expect("chmod")
        };
        set("HEAD", 0o600);
        set("refs/heads/production", 0o600);
        set("config", 0o600);

        // every lock's mode, as it is when the script moves it
        let shims = common::posix_exec::Shims::new();
        shims.add(
            "mv",
            "printf '%s %s\\n' \"$3\" \"$(\"$(real ls)\" -l -- \"$3\" | cut -c1-10)\" \
             >> \"$(dirname \"$0\")/mv.log\"\n\
             exec \"$(real mv)\" \"$@\"",
        );
        n.count.use_shims(&shims);
        let writes = vec![
            GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: Head,
                bytes: b"ref: refs/heads/production\n".to_vec(),
            },
            GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: BranchRef("production".to_string()),
                bytes: format!("{}\n", fx.c2).into_bytes(),
            },
            GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: Config,
                bytes: b"[core]\n\trepositoryformatversion = 0\n".to_vec(),
            },
            GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: RemoteRef {
                    remote: "origin".to_string(),
                    branch: "feature".to_string(),
                },
                bytes: format!("{}\n", fx.c2).into_bytes(),
            },
        ];
        n.t.write_git_files(PROJECT, &writes).await.expect("write");
        for rel in ["HEAD", "refs/heads/production", "config"] {
            assert_eq!(mode_of(&git.join(rel)), 0o600, "{rel} is not widened");
        }
        assert_eq!(
            mode_of(&git.join("refs/remotes/origin/feature")),
            0o644,
            "a new ref is 0644"
        );
        for write in &writes {
            assert_eq!(
                std::fs::read(git.join(write.name.rel())).expect("read back"),
                write.bytes
            );
        }
        let log = std::fs::read_to_string(shims.path().join("mv.log")).expect("mv log");
        let lines: Vec<&str> = log.lines().collect();
        assert_eq!(lines.len(), writes.len(), "{log}");
        for (line, want) in
            lines
                .iter()
                .zip(["-rw-------", "-rw-------", "-rw-------", "-rw-r--r--"])
        {
            assert!(
                line.ends_with(want),
                "a lock is made with its final mode: {line}"
            );
        }

        // a config that is absent is created owner-only
        std::fs::remove_file(git.join("config")).expect("rm config");
        n.t.write_git_files(PROJECT, &writes[2..3])
            .await
            .expect("new config");
        assert_eq!(mode_of(&git.join("config")), 0o600, "a new config is 0600");
    }

    /// 27-31 (WR-B1): a mode the host cannot report refuses the write before
    /// anything is put in place.
    #[tokio::test]
    async fn sftp_git_write_refuses_an_unreadable_mode() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let head = std::fs::read(fx.git_dir().join("HEAD")).expect("head");
        let shims = common::posix_exec::Shims::new();
        shims.add("stat", "echo nonsense");
        n.count.use_shims(&shims);
        let why = detail(
            n.t.write_git_files(PROJECT, &two_writes(&fx))
                .await
                .expect_err("no mode"),
        );
        assert!(why.contains("exit code 3"), "{why}");
        assert!(why.contains("mode"), "{why}");
        assert_eq!(
            std::fs::read(fx.git_dir().join("HEAD")).expect("head"),
            head
        );
        assert!(!fx.git_dir().join("HEAD.lock").exists());
        assert!(stages(&n).is_empty());
    }

    /// The shims that make the project directory belong to `uid` and record
    /// every `setfacl` call.
    fn gateway_shims(uid: &str) -> common::posix_exec::Shims {
        let shims = common::posix_exec::Shims::new();
        shims.add(
            "ls",
            &format!(
                "case $1 in -ldn) printf 'drwxr-xr-x 2 {uid} {uid} 64 Jan  1 00:00 %s\\n' \"$3\";; \
                 *) exec \"$(real ls)\" \"$@\";; esac"
            ),
        );
        shims.add(
            "setfacl",
            "echo \"$*\" >> \"$(dirname \"$0\")/setfacl.log\"",
        );
        shims
    }

    /// Writes that make no directory: HEAD and the existing branch.
    fn in_place(fx: &GitFixture) -> Vec<GitWrite> {
        vec![
            GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::Head,
                bytes: b"ref: refs/heads/production\n".to_vec(),
            },
            GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::BranchRef("production".to_string()),
                bytes: format!("{}\n", fx.c2).into_bytes(),
            },
        ]
    }

    fn discard(run: &str) -> Vec<GitWrite> {
        vec![GitWrite {
            old: Default::default(),
            packed_refs_old: None,
            name: GitWriteName::DiscardRef(run.to_string()),
            bytes: b"0123456789012345678901234567890123456789\n".to_vec(),
        }]
    }

    /// 27-31 (WR-B1, D-22): a directory the advance creates gets the D-G1 named
    /// ACL entry for a gateway account that is neither root nor the deploy
    /// account, access and default; with no `setfacl` the write refuses before
    /// any lock exists.
    #[tokio::test]
    async fn sftp_git_write_new_dir_gets_gateway_acl() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let shims = gateway_shims("4242");
        n.count.use_shims(&shims);
        n.t.write_git_files(PROJECT, &discard("run1"))
            .await
            .expect("write");
        assert!(fx.git_dir().join("refs/ign-discarded/run1").is_file());
        let log = std::fs::read_to_string(shims.path().join("setfacl.log")).expect("setfacl log");
        let lines: Vec<&str> = log.lines().collect();
        assert!(
            lines.contains(&"-P -m u:4242:rwX ./ign-discarded"),
            "the access entry: {lines:?}"
        );
        assert!(
            lines.contains(&"-P -d -m u:4242:rwX ./ign-discarded"),
            "the default entry: {lines:?}"
        );
        assert_eq!(
            lines.len(),
            2,
            "only the directory the advance made: {lines:?}"
        );

        // a write that makes no directory sets no ACL (the file replaces one
        // the gateway already owns the right to)
        std::fs::remove_file(shims.path().join("setfacl.log")).expect("rm log");
        n.t.write_git_files(PROJECT, &in_place(&fx))
            .await
            .expect("no new directory");
        assert!(!shims.path().join("setfacl.log").exists());

        // a directory deeper down: every level the advance makes gets it
        n.t.write_git_files(
            PROJECT,
            &[GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::BranchRef("deep/er/x".to_string()),
                bytes: format!("{}\n", fx.c2).into_bytes(),
            }],
        )
        .await
        .expect("deep");
        let log = std::fs::read_to_string(shims.path().join("setfacl.log")).expect("setfacl log");
        for dir in ["./deep", "./er"] {
            for form in ["-P -m", "-P -d -m"] {
                assert!(
                    log.lines().any(|l| l == format!("{form} u:4242:rwX {dir}")),
                    "{form} {dir}: {log}"
                );
            }
        }
    }

    #[tokio::test]
    async fn sftp_git_write_new_dir_refuses_without_setfacl() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        // a PATH with everything the scripts use except setfacl
        let shims = gateway_shims("4242");
        let tools = shims.path();
        std::fs::remove_file(tools.join("setfacl")).expect("no setfacl");
        for tool in [
            "sh", "mkdir", "rmdir", "rm", "mv", "wc", "cat", "id", "stat", "cmp", "dirname", "cut",
            "ln",
        ] {
            let found = std::process::Command::new("sh")
                .args(["-c", &format!("command -v {tool}")])
                .output()
                .expect("sh");
            let target = String::from_utf8_lossy(&found.stdout).trim().to_string();
            if found.status.success() && target.starts_with('/') {
                std::os::unix::fs::symlink(&target, tools.join(tool)).expect("link");
            }
        }
        *n.count.only_path.lock().expect("only") = Some(tools.to_path_buf());
        let why = detail(
            n.t.write_git_files(PROJECT, &discard("run1"))
                .await
                .expect_err("no setfacl"),
        );
        assert!(why.contains("exit code 6"), "{why}");
        assert!(why.contains("setfacl"), "{why}");
        assert!(
            !fx.git_dir().join("refs/ign-discarded").exists(),
            "no directory was made"
        );
        assert!(stages(&n).is_empty());
        // a write that makes no directory does not need it
        n.t.write_git_files(PROJECT, &in_place(&fx))
            .await
            .expect("no new directory, no setfacl needed");
    }

    /// 27-31 (WR-B1): with a gateway account that is root, or the deploy
    /// account itself, no ACL is set.
    #[tokio::test]
    async fn sftp_git_write_new_dir_sets_no_acl_for_root_or_the_deploy_account() {
        let fx = GitFixture::new();
        for uid in ["0", ""] {
            let n = node(&fx.data_dir);
            // "" stands for the real owner: no `ls` shim, only the recorder
            let shims = if uid.is_empty() {
                let shims = common::posix_exec::Shims::new();
                shims.add(
                    "setfacl",
                    "echo \"$*\" >> \"$(dirname \"$0\")/setfacl.log\"",
                );
                shims
            } else {
                gateway_shims(uid)
            };
            n.count.use_shims(&shims);
            n.t.write_git_files(PROJECT, &discard(&format!("run{}", uid.len())))
                .await
                .expect("write");
            assert!(
                !shims.path().join("setfacl.log").exists(),
                "uid {uid:?}: no setfacl call"
            );
        }
    }

    /// Every file below `dir`, `/`-separated and relative.
    fn walk(dir: &Path) -> Vec<String> {
        fn go(base: &Path, dir: &Path, out: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).expect("read_dir") {
                let entry = entry.expect("entry");
                let path = entry.path();
                if entry.file_type().expect("type").is_dir() {
                    go(base, &path, out);
                } else {
                    out.push(
                        path.strip_prefix(base)
                            .expect("prefix")
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }
        let mut out = Vec::new();
        go(dir, dir, &mut out);
        out
    }

    #[tokio::test]
    async fn sftp_git_probe_posix() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        n.t.probe_write(PROJECT, RUN).await.expect("probe");
        let probe = format!(".ign-probe-{RUN}");
        assert!(!fx.project_dir().join(&probe).exists());
        assert!(!fx.git_dir().join(&probe).exists());
        if ignores_modes(&fx.data_dir) {
            println!("running with privileges that ignore mode bits; denial cases skipped");
            return;
        }
        // a read-only project directory refuses, naming it
        std::fs::set_permissions(fx.project_dir(), std::fs::Permissions::from_mode(0o555))
            .expect("chmod");
        let why = detail(
            n.t.probe_write(PROJECT, RUN)
                .await
                .expect_err("read-only project"),
        );
        std::fs::set_permissions(fx.project_dir(), std::fs::Permissions::from_mode(0o755))
            .expect("chmod");
        assert!(why.contains("projects/CapApp"), "{why}");
        assert!(!why.contains("projects/CapApp/.git"), "{why}");
        // a read-only `.git` refuses, naming it
        std::fs::set_permissions(fx.git_dir(), std::fs::Permissions::from_mode(0o555))
            .expect("chmod");
        let why = detail(
            n.t.probe_write(PROJECT, RUN)
                .await
                .expect_err("read-only .git"),
        );
        std::fs::set_permissions(fx.git_dir(), std::fs::Permissions::from_mode(0o755))
            .expect("chmod");
        assert!(why.contains("projects/CapApp/.git"), "{why}");
        // a probe file that is already there is refused and left alone
        std::fs::write(fx.git_dir().join(&probe), "mine").expect("existing");
        let why = detail(n.t.probe_write(PROJECT, RUN).await.expect_err("existing"));
        assert!(why.contains("already exists"), "{why}");
        assert_eq!(
            std::fs::read_to_string(fx.git_dir().join(&probe)).expect("kept"),
            "mine"
        );
    }

    #[tokio::test]
    async fn sftp_git_yaml_posix() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        assert_eq!(n.t.read_git_yaml().await.expect("absent"), None);
        let yaml = fx.data_dir.join("git.yaml");
        std::fs::write(&yaml, "repos: {}\n").expect("yaml");
        // CodeRabbit #56: the bytes may hold a credential, so they come back
        // in the answer of one host command, never as a file `get` into a
        // local download directory
        let batches = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = batches.clone();
        n.count
            .on_batch(move |batch| seen.lock().expect("batches").push(batch.to_string()));
        assert_eq!(
            n.t.read_git_yaml().await.expect("present"),
            Some(b"repos: {}\n".to_vec())
        );
        let batches = batches.lock().expect("batches").clone();
        assert!(
            batches.iter().all(|b| !b.contains("git.yaml")),
            "git.yaml went through sftp: {batches:?}"
        );
        // a symlink refuses, naming git.yaml
        std::fs::rename(&yaml, fx.data_dir.join("real.yaml")).expect("rename");
        std::os::unix::fs::symlink("real.yaml", &yaml).expect("symlink");
        let err = n.t.read_git_yaml().await.expect_err("symlink");
        assert_eq!(err.code(), "deploy_run_invalid", "{err}");
        assert!(err.to_string().contains("git.yaml"), "{err}");
        std::fs::remove_file(&yaml).expect("rm link");
        // over the YAML cap refuses
        std::fs::write(&yaml, vec![b'a'; 1_048_577]).expect("big");
        let err = n.t.read_git_yaml().await.expect_err("oversize");
        assert_eq!(err.code(), "deploy_run_invalid", "{err}");
        // a directory is not a yaml file
        std::fs::remove_file(&yaml).expect("rm");
        std::fs::create_dir(&yaml).expect("dir");
        let err = n.t.read_git_yaml().await.expect_err("directory");
        assert_eq!(err.code(), "deploy_run_invalid", "{err}");
    }

    #[tokio::test]
    async fn sftp_git_size_posix() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let manifest = fx.node().git_manifest(PROJECT).await.expect("manifest");
        let size = n.t.git_size(PROJECT).await.expect("size");
        assert_eq!(size.files, manifest.files);
        assert_eq!(size.bytes, manifest.bytes);
        // one exec call, no sftp session, and nothing was hashed or read
        assert_eq!(n.count.execs(), 1);
        assert_eq!(n.count.sftp(), 0);
        let command = n.count.execs.lock().expect("execs")[0].clone();
        for reader in [
            "sha256", "shasum", "cat ", "base64", "head ", "tail ", "wc ",
        ] {
            assert!(!command.contains(reader), "{reader} in the size script");
        }
        if !ignores_modes(&fx.data_dir) {
            // a file nobody may read still counts: its size comes from the listing
            let config = fx.git_dir().join("config");
            std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o000))
                .expect("chmod");
            let after = n.t.git_size(PROJECT).await;
            std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o644))
                .expect("chmod");
            assert_eq!(after.expect("size without reading"), size);
        }
        // a symlink inside refuses
        std::os::unix::fs::symlink("/etc", fx.git_dir().join("l")).expect("symlink");
        let err = n.t.git_size(PROJECT).await.expect_err("symlink");
        assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
        // a missing `.git` is a failure, not an empty size
        std::fs::remove_dir_all(fx.git_dir()).expect("rm .git");
        let err = n.t.git_size(PROJECT).await.expect_err("absent .git");
        assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
    }

    #[tokio::test]
    async fn sftp_git_free_bytes_posix() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let free = n.t.free_bytes(GATEWAY, PROJECT).await.expect("free");
        assert!(free > 0);
        let local = fx
            .node()
            .free_bytes(GATEWAY, PROJECT)
            .await
            .expect("local free");
        assert!(local > 0);
    }

    #[tokio::test]
    async fn sftp_git_names_checked_first() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let hostile = [
            "", ".", "..", "../x", "a/b", ".x", "a\nb", "a\u{7}b", "a\\b",
        ];
        for name in hostile {
            let write = vec![GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::Head,
                bytes: vec![1],
            }];
            let results = [
                n.t.read_git_state(name).await.map(|_| ()),
                n.t.git_manifest(name).await.map(|_| ()),
                n.t.git_size(name).await.map(|_| ()),
                n.t.save_git(GATEWAY, name, RUN).await.map(|_| ()),
                n.t.restore_git(GATEWAY, name, RUN).await.map(|_| ()),
                n.t.remove_git_save(GATEWAY, name, RUN).await,
                n.t.probe_write(name, RUN).await,
                n.t.write_git_files(name, &write).await,
                n.t.free_bytes(GATEWAY, name).await.map(|_| ()),
                n.t.save_manifest(GATEWAY, name, RUN).await.map(|_| ()),
                n.t.save_git(name, PROJECT, RUN).await.map(|_| ()),
                n.t.save_git(GATEWAY, PROJECT, name).await.map(|_| ()),
            ];
            for (index, result) in results.iter().enumerate() {
                assert!(result.is_err(), "{name:?} call {index} was accepted");
            }
        }
        assert_eq!(n.count.execs(), 0, "no remote command ran");
        assert_eq!(n.count.sftp(), 0, "no sftp session opened");
        // the host trait reaches the same reader
        let host: &dyn ProjectHost = &n.t;
        assert!(host.git().is_some());
    }

    /// Every file the list names, as the fetch takes them.
    fn all_files(
        list: &ignition_core::deploy::git_objects::GitObjectList,
    ) -> Vec<ignition_core::deploy::git_objects::GitObjectFile> {
        use ignition_core::deploy::git_objects::GitObjectFile;
        let mut files = list.loose.clone();
        for pack in &list.packs {
            files.push(GitObjectFile {
                rel: pack.idx_rel(),
                size: pack.idx_size,
            });
            files.push(GitObjectFile {
                rel: pack.pack_rel(),
                size: pack.pack_size,
            });
        }
        files
    }

    fn files_below(dir: &Path) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            for entry in std::fs::read_dir(&current).expect("ls") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    out.push(path.strip_prefix(dir).expect("below").display().to_string());
                }
            }
        }
        out.sort();
        out
    }

    /// 27-11: the POSIX arm of the gateway-object read, over the local
    /// sftp-server. The listing equals the local transport's for the same
    /// repo (loose objects, packs and both at once), is one exec and no sftp
    /// session, the fetch is a byte-identical round trip in a fixed number of
    /// sessions whatever the object count, a bad name refuses before any
    /// `get`, a size that differs from the listing refuses, and a target
    /// already in the destination is never overwritten.
    #[tokio::test]
    async fn sftp_git_objects_posix() {
        use ignition_core::deploy::git_objects::GitObjectFile;
        let fx = GitFixture::new();
        // loose only first
        let loose_only = fx.node().list_git_objects(PROJECT).await.expect("ref");
        assert!(!loose_only.loose.is_empty() && loose_only.packs.is_empty());
        let n = node(&fx.data_dir);
        assert_eq!(
            n.t.list_git_objects(PROJECT).await.expect("list"),
            loose_only
        );
        assert_eq!((n.count.execs(), n.count.sftp()), (1, 0), "one script");

        // packs and loose objects together
        fx.gateway_gc();
        fx.designer_commit(false);
        let reference = fx.node().list_git_objects(PROJECT).await.expect("ref");
        assert!(!reference.loose.is_empty() && !reference.packs.is_empty());
        let n = node(&fx.data_dir);
        let list = n.t.list_git_objects(PROJECT).await.expect("list");
        assert_eq!(list, reference);
        assert_eq!((n.count.execs(), n.count.sftp()), (1, 0));

        // a byte-identical round trip of every listed file
        let files = all_files(&list);
        let dest = tempfile::tempdir().expect("dest");
        let before = n.count.sftp();
        n.t.fetch_git_objects(PROJECT, &files, dest.path())
            .await
            .expect("fetch");
        let small_sessions = n.count.sftp() - before;
        assert!(small_sessions >= 1, "the download ran");
        let mut want: Vec<String> = files.iter().map(|f| f.rel.clone()).collect();
        want.sort();
        assert_eq!(files_below(dest.path()), want, "exactly the listed files");
        for file in &files {
            assert_eq!(
                std::fs::read(dest.path().join(&file.rel)).expect("copy"),
                std::fs::read(fx.git_dir().join(&file.rel)).expect("source"),
                "{}",
                file.rel
            );
        }

        // the number of sessions does not depend on the object count
        for i in 0..120u32 {
            let fan = format!("{:02x}", i % 7);
            let dir = fx.git_dir().join("objects").join(&fan);
            std::fs::create_dir_all(&dir).expect("fan");
            std::fs::write(dir.join(format!("{i:038x}")), vec![b'o'; 50 + i as usize])
                .expect("object");
        }
        let n = node(&fx.data_dir);
        let big = n.t.list_git_objects(PROJECT).await.expect("list");
        assert!(big.loose.len() >= files.len() + 100, "{}", big.loose.len());
        let more = all_files(&big);
        let dest_big = tempfile::tempdir().expect("dest");
        n.t.fetch_git_objects(PROJECT, &more, dest_big.path())
            .await
            .expect("fetch many");
        assert_eq!(
            n.count.sftp(),
            small_sessions,
            "{} files cost as many sessions as {}",
            more.len(),
            files.len()
        );
        assert_eq!(n.count.execs(), 1, "still one listing script");
        assert_eq!(files_below(dest_big.path()).len(), more.len());

        // a name outside the fixed shapes refuses before any `get`
        let n = node(&fx.data_dir);
        let empty = tempfile::tempdir().expect("dest");
        for rel in [
            "config",
            "objects/info/alternates",
            "hooks/pre-commit",
            "objects/../config",
            "objects/pack/pack-xyz.pack",
            "/etc/passwd",
        ] {
            let bad = GitObjectFile {
                rel: rel.to_string(),
                size: 1,
            };
            let got =
                n.t.fetch_git_objects(PROJECT, &[files[0].clone(), bad], empty.path())
                    .await;
            assert!(got.is_err(), "{rel}");
        }
        assert_eq!((n.count.execs(), n.count.sftp()), (0, 0), "no process ran");
        assert!(files_below(empty.path()).is_empty());

        // a size that is not the listed size refuses, and nothing is installed
        let wrong = GitObjectFile {
            rel: files[0].rel.clone(),
            size: files[0].size + 1,
        };
        let err =
            n.t.fetch_git_objects(PROJECT, &[wrong], empty.path())
                .await
                .expect_err("size");
        assert!(detail(err).contains("listed as"), "names the mismatch");
        assert!(files_below(empty.path()).is_empty(), "nothing installed");

        // a target already in the destination is never overwritten
        let taken = tempfile::tempdir().expect("dest");
        let first = &files[0];
        let target = taken.path().join(&first.rel);
        std::fs::create_dir_all(target.parent().expect("parent")).expect("dirs");
        std::fs::write(&target, "mine").expect("mine");
        let err =
            n.t.fetch_git_objects(PROJECT, std::slice::from_ref(first), taken.path())
                .await
                .expect_err("exists");
        assert!(detail(err).contains("never overwritten"));
        assert_eq!(std::fs::read(&target).expect("kept"), b"mine");

        // a destination that is a symlink, or a directory on the way that is
        // one, is not followed
        let elsewhere = tempfile::tempdir().expect("elsewhere");
        let linked = taken.path().join("link");
        std::os::unix::fs::symlink(elsewhere.path(), &linked).expect("link");
        assert!(
            n.t.fetch_git_objects(PROJECT, std::slice::from_ref(first), &linked)
                .await
                .is_err()
        );
        let trap = tempfile::tempdir().expect("trap");
        std::os::unix::fs::symlink(elsewhere.path(), trap.path().join("objects")).expect("link");
        assert!(
            n.t.fetch_git_objects(PROJECT, std::slice::from_ref(first), trap.path())
                .await
                .is_err()
        );
        assert!(
            files_below(elsewhere.path()).is_empty(),
            "nothing written through"
        );

        // a source that went away between the listing and the fetch
        let gone = &files[1];
        std::fs::remove_file(fx.git_dir().join(&gone.rel)).expect("rm");
        let fresh = tempfile::tempdir().expect("dest");
        assert!(
            n.t.fetch_git_objects(PROJECT, std::slice::from_ref(gone), fresh.path())
                .await
                .is_err()
        );
        assert!(files_below(fresh.path()).is_empty());
    }

    /// 27-11: the listing refuses a symlink and a repo whose `objects` is
    /// missing or a link, never an empty list (the transport arm of the
    /// refusals the script test pins).
    #[tokio::test]
    async fn sftp_git_objects_posix_refusals() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        std::os::unix::fs::symlink("/", fx.git_dir().join("objects/ab_link")).expect("link");
        let err = n.t.list_git_objects(PROJECT).await.expect_err("symlink");
        assert!(detail(err).contains("symlink"));
        std::fs::remove_file(fx.git_dir().join("objects/ab_link")).expect("rm");
        assert!(n.t.list_git_objects(PROJECT).await.is_ok());
        let err = n.t.list_git_objects("Nope").await.expect_err("missing");
        assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
        std::fs::write(fx.git_dir().join("objects/0a_bad:name"), "x").expect("colon");
        // an extra file of no fixed shape is ignored, a colon in a name refuses
        let err = n.t.list_git_objects(PROJECT).await.expect_err("colon");
        assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
    }

    /// 27-11: what is sent is fixed text of one line: no control character
    /// reaches the host, and the project name is data after the script.
    #[tokio::test]
    async fn sftp_git_objects_sh_one_line() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        n.t.list_git_objects(PROJECT).await.expect("list");
        let sent = n.count.execs.lock().expect("execs").clone();
        assert_eq!(sent.len(), 1);
        assert!(
            !sent[0].chars().any(char::is_control),
            "no control character: {:?}",
            sent[0]
        );
        assert!(sent[0].contains("IGN-RESULT"), "the answer sentinel");
        assert!(!sent[0].contains("git "), "no git runs on the host");
        assert!(!sent[0].contains("eval"), "nothing is evaluated");
    }

    /// 27-41: the POSIX read of the project's files after the import (D-25).
    /// Every regular file below `projects/<p>` but the `.git` subtree, with
    /// size and sha256, equal to the local reader's on the same tree, for names
    /// a shell or an awk could trip over.
    #[tokio::test]
    async fn sftp_project_digests_posix() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let project = fx.project_dir();
        // Apple's `wc` re-encodes a non-ASCII name it prints, so the size and
        // digest reads cannot be joined there (the read then refuses, as it
        // must); the production hosts are GNU, where the name is verbatim.
        #[cfg(target_os = "linux")]
        {
            let path = project.join("ignition/script-python/with space/caf\u{e9}.py");
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
            std::fs::write(&path, "accent\n").expect("file");
        }
        for rel in [
            "ignition/script-python/with space/plain.py",
            "ignition/script-python/-dash/-x.py",
            "ignition/script-python/q/it's \"here\".py",
            "ignition/script-python/q/$HOME `x` *.py",
            "ignition/script-python/d ./y.py",
            "ignition/script-python/empty.py",
        ] {
            let path = project.join(rel);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
            let body = if rel.ends_with("empty.py") {
                String::new()
            } else {
                format!("body of {rel}\n")
            };
            std::fs::write(&path, body).expect("file");
        }
        // below `.git` nothing is read, not even what the project walk refuses
        std::os::unix::fs::symlink("HEAD", fx.git_dir().join("a-link")).expect("link in .git");

        let local = fx
            .node()
            .project_file_digests(PROJECT)
            .await
            .expect("local");
        let posix = n.t.project_file_digests(PROJECT).await.expect("posix");
        assert_eq!(posix, local, "the two implementations read the same disk");
        assert!(posix.len() > 10, "{posix:?}");
        assert!(posix.contains_key("project.json"));
        assert!(
            posix.keys().all(|name| !name.starts_with(".git")),
            "{posix:?}"
        );
        let sizes: Vec<u64> = posix.values().map(|file| file.size).collect();
        assert!(sizes.contains(&0), "an empty file is listed with size 0");
        // one host command and no sftp session
        assert_eq!(n.count.execs(), 1);
        assert_eq!(n.count.sftp(), 0);
    }

    /// 27-41: a link, a special file, a name with a control character or a
    /// backslash anywhere below the project refuses with exit 4 naming the
    /// entry; the same entry below `.git` does not matter.
    #[tokio::test]
    async fn sftp_project_digests_posix_refuses_a_link() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let project = fx.project_dir();
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("secret"), "outside\n").expect("secret");
        let nested = project.join("ignition/script-python/cap");

        for (what, make) in [
            (
                "a symlink to a file",
                Box::new(|at: &Path| std::os::unix::fs::symlink(outside.path().join("secret"), at))
                    as Box<dyn Fn(&Path) -> std::io::Result<()>>,
            ),
            (
                "a symlink to a directory",
                Box::new(|at: &Path| std::os::unix::fs::symlink(outside.path(), at)),
            ),
            (
                "a dangling symlink",
                Box::new(|at: &Path| std::os::unix::fs::symlink("nowhere", at)),
            ),
        ] {
            let at = nested.join("planted");
            make(&at).unwrap_or_else(|e| panic!("{what}: {e}"));
            let why = detail(n.t.project_file_digests(PROJECT).await.expect_err(what));
            assert!(why.contains("exit code 4"), "{what}: {why}");
            assert!(why.contains("planted"), "{what} names the entry: {why}");
            std::fs::remove_file(&at).expect("rm link");
        }
        for name in ["bad\u{1}name", "back\\slash"] {
            let at = nested.join(name);
            std::fs::write(&at, "x").expect("odd name");
            let why = detail(n.t.project_file_digests(PROJECT).await.expect_err(name));
            assert!(why.contains("exit code 4"), "{name:?}: {why}");
            std::fs::remove_file(&at).expect("rm");
        }
        // the clean tree reads again, and a link in `.git` is not looked at
        std::os::unix::fs::symlink("HEAD", fx.git_dir().join("a-link")).expect("link in .git");
        n.t.project_file_digests(PROJECT)
            .await
            .expect("clean again");
        // the project directory itself may not be a link
        let real = fx.data_dir.join("projects/Real");
        std::fs::rename(&project, &real).expect("move");
        std::os::unix::fs::symlink(&real, &project).expect("link project");
        let why = detail(n.t.project_file_digests(PROJECT).await.expect_err("linked"));
        assert!(why.contains("exit code 4"), "{why}");
    }

    /// 27-41: a project directory swapped for a link while the read runs is
    /// refused. A `find` shim moves the verified directory away and puts a link
    /// to other content in its place after the walk entered it; the answer is
    /// never built from what the link shows.
    #[tokio::test]
    async fn sftp_project_digests_posix_refuses_a_level_swapped_during_the_walk() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let project = fx.project_dir();
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("project.json"), "{\"other\":1}\n").expect("other");
        let marker = outside.path().join("done");
        let shims = common::posix_exec::Shims::new();
        shims.add(
            "find",
            &format!(
                "if [ ! -e {m} ]; then : > {m}; mv {p} {p}.real && ln -s {o} {p}; fi\n\
                 exec \"$(real find)\" \"$@\"",
                m = marker.display(),
                p = project.display(),
                o = outside.path().display()
            ),
        );
        n.count.use_shims(&shims);
        let why = detail(
            n.t.project_file_digests(PROJECT)
                .await
                .expect_err("a swapped level"),
        );
        assert!(why.contains("exit code 4"), "{why}");
        assert!(why.contains("changed"), "{why}");
    }

    /// 27-41: what is sent is fixed text of one line, and no git runs there.
    #[tokio::test]
    async fn sftp_project_digests_sh_one_line() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        n.t.project_file_digests(PROJECT).await.expect("read");
        let sent = n.count.execs.lock().expect("execs").clone();
        assert_eq!(sent.len(), 1);
        assert!(!sent[0].chars().any(char::is_control), "{:?}", sent[0]);
        assert!(sent[0].contains("IGN-RESULT"), "the answer sentinel");
        assert!(
            !sent[0].contains(" git ") && !sent[0].contains("$(git"),
            "no git runs on the host: {:?}",
            sent[0]
        );
        assert!(!sent[0].contains("eval"), "nothing is evaluated");
    }

    /// A shim for `tool` that, the first time its `pwd -P` and arguments match
    /// the `case` pattern `when`, moves `victim` aside and puts a link to
    /// `outside` in its place (27-46): a level swapped while a script runs.
    fn swap_once(
        shims: &common::posix_exec::Shims,
        tool: &str,
        when: &str,
        victim: &Path,
        outside: &Path,
    ) {
        let marker = outside.join("swapped");
        shims.add(
            tool,
            &format!(
                "case \"$(pwd -P):$*\" in {when}) if [ ! -e {m} ]; then : > {m}; \
                 mv {v} {v}.real && ln -s {o} {v}; fi;; esac\n\
                 exec \"$(real {tool})\" \"$@\"",
                m = marker.display(),
                v = victim.display(),
                o = outside.display(),
            ),
        );
    }

    /// Like [`swap_once`], but at the `nth` call of `tool` (counted in a file),
    /// whatever its arguments.
    fn swap_at_call(
        shims: &common::posix_exec::Shims,
        tool: &str,
        nth: usize,
        victim: &Path,
        outside: &Path,
    ) {
        let count = outside.join("calls");
        shims.add(
            tool,
            &format!(
                "c=$(cat {c} 2>/dev/null || echo 0); c=$((c+1)); echo $c > {c}\n\
                 if [ $c = {nth} ]; then mv {v} {v}.real && ln -s {o} {v}; fi\n\
                 exec \"$(real {tool})\" \"$@\"",
                c = count.display(),
                v = victim.display(),
                o = outside.display(),
            ),
        );
    }

    /// 27-46 (C1, POSIX half): a level above the loose refs swapped for a link
    /// while the state read runs is refused (exit 4, naming the level), and no
    /// ref from outside `.git` is in the answer. A `wc` shim swaps
    /// `refs/remotes` at the last loose-ref read (the call after those of HEAD,
    /// config, packed-refs, the reflog and the one head), between the listing
    /// and the read, when every earlier check has passed: a script that reads
    /// `refs/remotes/<name>/<ref>` by its multi-level path reads the outside
    /// copy.
    #[tokio::test]
    async fn sftp_git_state_refuses_a_level_swapped_during_the_walk() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let git = fx.git_dir();
        std::fs::create_dir_all(git.join("refs/remotes/capapp")).expect("remote dir");
        std::fs::write(
            git.join("refs/remotes/capapp/production"),
            format!("{}\n", fx.c1),
        )
        .expect("loose remote ref");
        for name in [
            "HEAD",
            "config",
            "packed-refs",
            "logs/HEAD",
            "refs/heads/production",
            "refs/remotes/capapp/production",
        ] {
            assert!(git.join(name).is_file(), "the fixture holds {name}");
        }
        let outside = tempfile::tempdir().expect("outside");
        std::fs::create_dir_all(outside.path().join("capapp")).expect("capapp");
        std::fs::write(
            outside.path().join("capapp/production"),
            format!("{}\n", fx.c2),
        )
        .expect("outside copy");
        let shims = common::posix_exec::Shims::new();
        swap_at_call(&shims, "wc", 6, &git.join("refs/remotes"), outside.path());
        n.count.use_shims(&shims);
        let why = detail(
            n.t.read_git_state(PROJECT)
                .await
                .expect_err("a level swapped during the walk"),
        );
        assert!(why.contains("exit code 4"), "{why}");
        assert!(why.contains("refs/remotes"), "the level is named: {why}");
        assert!(
            git.join("refs/remotes.real").exists(),
            "the shim swapped the level"
        );
    }

    /// 27-46: every loose ref, nested or not, is read by its basename after a
    /// `cd` into its parent directory: the same bytes as the local read, and a
    /// `head` shim sees its working directory be the ref's parent.
    #[tokio::test]
    async fn sftp_git_state_reads_nested_refs_by_basename() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        fx.git_in_project(&["branch", "feature/a/b"]);
        fx.git_in_project(&["branch", "top"]);
        let log = fx.data_dir.join("head-cwd.log");
        let shims = common::posix_exec::Shims::new();
        shims.add(
            "head",
            &format!("pwd -P >> {}\nexec \"$(real head)\" \"$@\"", log.display()),
        );
        n.count.use_shims(&shims);
        let local: GitFiles = fx.node().read_git_state(PROJECT).await.expect("local");
        let posix: GitFiles = n.t.read_git_state(PROJECT).await.expect("posix");
        assert_eq!(posix, local, "the same bytes as before");
        assert!(
            posix
                .loose_refs
                .iter()
                .any(|(name, _)| name == "refs/heads/feature/a/b"),
            "{posix:?}"
        );
        let cwds = std::fs::read_to_string(&log).expect("head log");
        assert!(
            cwds.lines()
                .any(|dir| dir.ends_with("/refs/heads/feature/a")),
            "the nested ref is read from its parent directory: {cwds}"
        );
        assert!(
            cwds.lines().any(|dir| dir.ends_with("/refs/heads")),
            "{cwds}"
        );
    }

    /// 27-46: `logs` swapped for a link is refused, both when the swap lands
    /// before the walk enters it and when it lands after (the end check). The
    /// outside `HEAD` is never read.
    #[tokio::test]
    async fn sftp_git_log_refuses_a_swapped_logs_dir() {
        for (case, when) in [
            ("before the walk enters it", "*\"-- packed-refs\"*"),
            ("after the walk entered it", "*\"/logs:-ld -- HEAD\""),
        ] {
            let fx = GitFixture::new();
            let n = node(&fx.data_dir);
            let git = fx.git_dir();
            assert!(git.join("logs/HEAD").exists(), "the fixture keeps a reflog");
            let outside = tempfile::tempdir().expect("outside");
            std::fs::write(outside.path().join("HEAD"), "outside-reflog\n").expect("outside");
            let shims = common::posix_exec::Shims::new();
            swap_once(&shims, "ls", when, &git.join("logs"), outside.path());
            n.count.use_shims(&shims);
            let why = detail(n.t.read_git_state(PROJECT).await.expect_err(case));
            assert!(why.contains("exit code 4"), "{case}: {why}");
            assert!(why.contains("logs"), "{case}: {why}");
            assert!(
                git.join("logs.real").exists(),
                "{case}: the shim swapped it"
            );
        }
    }

    /// 27-46: the digest of a nested file is taken in the checked parent
    /// directory; a parent swapped for a link after the walk is refused and no
    /// digest of the outside file is returned.
    #[tokio::test]
    async fn sftp_git_digest_refuses_a_swapped_parent() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let git = fx.git_dir();
        std::fs::create_dir_all(git.join("refs/heads/sub")).expect("sub");
        std::fs::write(git.join("refs/heads/sub/x"), format!("{}\n", fx.c2)).expect("x");
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("x"), "outside\n").expect("outside x");
        let name = GitWriteName::BranchRef("sub/x".to_string());
        // honest first
        let honest = n.t.git_file_digest(PROJECT, &name).await.expect("honest");
        assert_eq!(
            honest.as_deref(),
            Some(sha256_hex(format!("{}\n", fx.c2).as_bytes()).as_str())
        );
        let shims = common::posix_exec::Shims::new();
        swap_once(
            &shims,
            "ls",
            "*",
            &git.join("refs/heads/sub"),
            outside.path(),
        );
        n.count.use_shims(&shims);
        let got = n.t.git_file_digest(PROJECT, &name).await;
        let why = detail(got.expect_err("a swapped parent"));
        assert!(why.contains("exit code 4"), "{why}");
        assert!(why.contains("sub"), "{why}");
    }

    /// 27-46: the manifest read refuses a level above `.git` that is swapped
    /// for a link after the walk (the end check); nothing is answered.
    #[tokio::test]
    async fn sftp_git_manifest_refuses_a_level_swapped_during_the_walk() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let outside = tempfile::tempdir().expect("outside");
        let shims = common::posix_exec::Shims::new();
        swap_once(&shims, "find", "*", &fx.project_dir(), outside.path());
        n.count.use_shims(&shims);
        let why = detail(
            n.t.git_manifest(PROJECT)
                .await
                .expect_err("a swapped level"),
        );
        assert!(why.contains("exit code 4"), "{why}");
        assert!(why.contains("changed"), "{why}");
    }

    /// 27-46: the size read, the same.
    #[tokio::test]
    async fn sftp_git_size_refuses_a_level_swapped_during_the_walk() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let outside = tempfile::tempdir().expect("outside");
        let shims = common::posix_exec::Shims::new();
        swap_once(&shims, "find", "*", &fx.git_dir(), outside.path());
        n.count.use_shims(&shims);
        let why = detail(n.t.git_size(PROJECT).await.expect_err("a swapped level"));
        assert!(why.contains("exit code 4"), "{why}");
        assert!(why.contains("changed"), "{why}");
    }

    /// 27-46: the object listing refuses `objects` swapped for a link after the
    /// walk reached it.
    #[tokio::test]
    async fn sftp_git_objects_refuses_a_swapped_objects_dir() {
        let fx = GitFixture::new();
        let n = node(&fx.data_dir);
        let outside = tempfile::tempdir().expect("outside");
        let shims = common::posix_exec::Shims::new();
        swap_once(
            &shims,
            "find",
            "*",
            &fx.git_dir().join("objects"),
            outside.path(),
        );
        n.count.use_shims(&shims);
        let why = detail(
            n.t.list_git_objects(PROJECT)
                .await
                .expect_err("a swapped objects dir"),
        );
        assert!(why.contains("exit code 4"), "{why}");
        assert!(why.contains("changed"), "{why}");
    }

    #[test]
    fn sftp_git_scripts_are_not_reachable_from_the_read_only_host() {
        // `ProjectHost::git` hands out the read half only; the writer trait is
        // a different trait object that this accessor cannot produce.
        let source = include_str!("../src/deploy/transport/projects.rs");
        let trait_start = source.find("pub trait ProjectHost").expect("trait");
        let trait_end = source[trait_start..].find("\n}\n").expect("end") + trait_start;
        assert!(
            !source[trait_start..trait_end].contains("GitStateWriter"),
            "ProjectHost must not name the writer"
        );
    }
}

/// The Windows git scripts of 27-09 (`winps::GIT_*`): their size against the
/// command budget, their parameter discipline, and their behaviour under the
/// local `pwsh` against the same fixture the POSIX and local implementations
/// are held to. Unix only: the `pwsh` driver and the mode checks are Unix.
#[cfg(unix)]
mod winps_git {
    use super::*;
    use common::git_fixture::{GATEWAY, GitFixture, PROJECT};
    use common::pwsh_exec;
    use ignition_core::deploy::git_state::{GIT_MARKERS, GitFiles};
    use ignition_core::deploy::transport::projects::{GitHostRead, GitManifest};
    use ignition_core::deploy::transport::winps::{self, COMMAND_BUDGET, WinScript};
    use ignition_core::deploy::tree::sha256_hex;
    use serde_json::Value;

    const RUN: &str = "20261114T101500Z-ab12";

    fn s(path: &Path) -> String {
        path.display().to_string()
    }

    /// Every constructor, with maximal realistic parameters: the Windows
    /// install's data directory, a 100-character project, 48-character gateway
    /// and run names, a long `.git` path.
    fn maximal() -> Vec<WinScript> {
        let prefix = winps::LONG_PATH_PREFIX;
        let data = "C:\\Program Files\\Inductive Automation\\Ignition\\data";
        let home = "C:\\Users\\svc-ignition-deploy";
        let project = "P".repeat(100);
        let (gateway, run) = ("G".repeat(48), "R".repeat(48));
        let git = format!("{data}\\projects\\{project}\\.git");
        let save_rel = format!("ign-deploy/{gateway}/projects/{project}/git-save/{run}");
        // twelve files named like SHA-1 packs: more than any advance writes
        // (one pack and its index, two refs, HEAD, index)
        let pack = format!("objects/pack/pack-{}.pack", "a".repeat(40));
        let writes: Vec<String> = (0..12).map(|_| pack.clone()).collect();
        let sized: Vec<(String, u64)> = writes.iter().map(|w| (w.clone(), 123_456_789)).collect();
        // one chunk of a write plan (the transport splits a longer plan) and the
        // longest stage name a run id can make
        let chunk: Vec<(String, u64)> =
            sized[..ignition_core::deploy::transport::sftp::GIT_WIN_CHUNK].to_vec();
        let stage = format!("git-stage-{}", "R".repeat(64));
        vec![
            WinScript::git_state(&git, GIT_MARKERS),
            WinScript::git_save(prefix, data, &project, home, &gateway, &run),
            WinScript::git_restore(prefix, data, &project, home, &gateway, &run),
            WinScript::git_manifest(prefix, &git),
            WinScript::git_size(prefix, &git),
            WinScript::git_objects(
                prefix,
                &git,
                ignition_core::deploy::git_objects::GIT_OBJECTS_LIST_MAX_ENTRIES,
                51_025,
            ),
            WinScript::git_digest(
                &git,
                &format!("refs/remotes/{}/{}", "o".repeat(40), "b".repeat(80)),
            ),
            WinScript::git_stage(home, &stage),
            WinScript::git_lock(data, &project, home, &stage, 8, &chunk),
            WinScript::git_unlock(data, &project, home, &stage, 8, &chunk, true),
            WinScript::git_replace(data, &project, home, &stage, &chunk, 3, true),
            WinScript::git_probe(data, &project, &format!(".ign-probe-{}", "R".repeat(128))),
            WinScript::git_save_rm(prefix, home, &format!("{save_rel}/repo"), &run),
            WinScript::git_free(home, &save_rel),
            WinScript::git_yaml(data, 1_048_576),
            WinScript::project_digests(prefix, data, &project, 100_000),
        ]
    }

    #[test]
    fn winps_git_scripts_fit_budget() {
        let scripts = maximal();
        assert_eq!(scripts.len(), 16);
        for script in &scripts {
            let command = winps::encoded_command(script, COMMAND_BUDGET)
                .unwrap_or_else(|why| panic!("{}: {why}", script.name));
            let used = command.as_str().chars().count();
            println!(
                "winps_git_scripts_fit_budget {}: {used} of {COMMAND_BUDGET}",
                script.name
            );
            assert!(used <= COMMAND_BUDGET, "{}", script.name);
            if script.name == "project_digests" {
                // 27-41: room for a longer script or parameter later
                assert!(used + 200 <= COMMAND_BUDGET, "{}", script.name);
            }
        }
        // a chunk of the write plan with the longest names a write can carry
        // (SHA-256 pack names, 10-digit sizes, a 100-character project, the
        // longest stage name) fits too, for each of the three scripts that take
        // a chunk; the transport splits a longer plan into chunks of this size
        let data = "C:\\Program Files\\Inductive Automation\\Ignition\\data";
        let home = "C:\\Users\\svc-ignition-deploy";
        let project = "P".repeat(100);
        let stage = format!("git-stage-{}", "R".repeat(64));
        let chunk_size = ignition_core::deploy::transport::sftp::GIT_WIN_CHUNK;
        let plan: Vec<(String, u64)> = (0..12)
            .map(|i| {
                let ext = if i % 2 == 0 { "pack" } else { "idx" };
                (
                    format!("objects/pack/pack-{}{i:02}.{ext}", "b".repeat(62)),
                    4_000_000_000 + i as u64,
                )
            })
            .collect();
        for (n, chunk) in plan.chunks(chunk_size).enumerate() {
            let offset = n * chunk_size;
            for script in [
                WinScript::git_lock(data, &project, home, &stage, offset, chunk),
                WinScript::git_unlock(data, &project, home, &stage, offset, chunk, true),
                WinScript::git_replace(data, &project, home, &stage, chunk, 3, true),
            ] {
                let used = winps::encoded_command(&script, COMMAND_BUDGET)
                    .unwrap_or_else(|why| panic!("{}: {why}", script.name))
                    .as_str()
                    .chars()
                    .count();
                println!(
                    "winps_git_scripts_fit_budget {} chunk of {}: {used} of {COMMAND_BUDGET}",
                    script.name,
                    chunk.len()
                );
            }
        }
        // a chunk that cannot fit is refused before anything is sent, never
        // truncated
        let many: Vec<(String, u64)> = (0..60)
            .map(|i| (format!("objects/pack/pack-{}-{i}.pack", "a".repeat(40)), 1))
            .collect();
        let script = WinScript::git_lock(data, &project, home, &stage, 0, &many);
        assert!(winps::encoded_command(&script, COMMAND_BUDGET).is_err());
    }

    /// WR-B5 (27-33, confirmed on the Windows test VM under 5.1): a raw
    /// `[IO.Directory]::Delete(path, $true)` stops on a ReadOnly file, so every
    /// recursive delete of the git scripts goes through the one helper `Dr`,
    /// which clears ReadOnly first without entering a reparse point.
    #[test]
    fn winps_git_recursive_deletes_clear_readonly_first() {
        for (name, body) in [
            ("GIT_SAVE", winps::GIT_SAVE),
            ("GIT_RESTORE", winps::GIT_RESTORE),
            ("GIT_SAVE_RM", winps::GIT_SAVE_RM),
        ] {
            assert!(body.contains("function Dr("), "{name} has the helper");
            assert_eq!(
                body.matches(",$true)").count(),
                1,
                "{name}: the only recursive delete is the one inside Dr"
            );
            assert!(
                body.contains("[IO.Directory]::Delete($d,$true)"),
                "{name}: Dr deletes"
            );
        }
        // security review of e0159fb: the ROOT of a delete is checked to be a real
        // directory first (a path swapped for a junction since the last check is
        // refused, never walked or deleted), and a ReadOnly file with more than one
        // hard link is left alone (clearing the bit would change the other link)
        for (name, body) in [
            ("GIT_SAVE", winps::GIT_SAVE),
            ("GIT_RESTORE", winps::GIT_RESTORE),
            ("GIT_SAVE_RM", winps::GIT_SAVE_RM),
        ] {
            assert!(
                body.contains("function Dr($d){$null=K (Z $d);"),
                "{name}: Dr checks its root"
            );
            assert!(
                body.contains("(Z $f.FullName).LinkType"),
                "{name}: a hard-linked file is not changed"
            );
        }
        for name in ["GIT_SAVE", "GIT_RESTORE"] {
            let body = if name == "GIT_SAVE" {
                winps::GIT_SAVE
            } else {
                winps::GIT_RESTORE
            };
            assert!(
                body.contains("could not be removed)"),
                "{name}: a delete that still fails names the leftover"
            );
        }
    }

    #[test]
    fn winps_git_params_only() {
        let bodies = [
            ("GIT_STATE", winps::GIT_STATE),
            ("GIT_SAVE", winps::GIT_SAVE),
            ("GIT_RESTORE", winps::GIT_RESTORE),
            ("GIT_MANIFEST", winps::GIT_MANIFEST),
            ("GIT_SIZE", winps::GIT_SIZE),
            ("GIT_OBJECTS", winps::GIT_OBJECTS),
            ("GIT_DIGEST", winps::GIT_DIGEST),
            ("GIT_LOCK", winps::GIT_LOCK),
            ("GIT_REPLACE", winps::GIT_REPLACE),
            ("GIT_PROBE", winps::GIT_PROBE),
            ("GIT_SAVE_RM", winps::GIT_SAVE_RM),
            ("GIT_FREE", winps::GIT_FREE),
            ("GIT_YAML", winps::GIT_YAML),
            ("PROJECT_DIGESTS", winps::PROJECT_DIGESTS),
        ];
        for (name, body) in bodies {
            assert!(!body.contains('\n'), "{name}: one line");
            // no host path or drive in the text: paths exist only as `$p.*`
            for drive in ["C:\\", "c:\\", "\\\\?\\", "/Users", "/home", "/tmp"] {
                assert!(!body.contains(drive), "{name} holds a path ({drive})");
            }
            // nothing that evaluates or re-parses text
            for forbidden in [
                "Invoke-Expression",
                "iex ",
                "Invoke-Command",
                "Start-Process",
                "-Command",
                "cmd ",
                "Add-Type",
                "[scriptblock]::Create",
            ] {
                assert!(!body.contains(forbidden), "{name} holds {forbidden}");
            }
            // a double-quoted string never carries a parameter: a parameter
            // value inside quotes would be parsed as script
            let mut quoted = false;
            let mut at = 0;
            let bytes: Vec<char> = body.chars().collect();
            let mut text = String::new();
            while at < bytes.len() {
                let c = bytes[at];
                if c == '\'' && !quoted {
                    // skip a single-quoted literal
                    at += 1;
                    while at < bytes.len() && bytes[at] != '\'' {
                        at += 1;
                    }
                } else if c == '"' {
                    if quoted {
                        assert!(
                            !text.contains("$p."),
                            "{name}: a parameter inside a double-quoted string: {text}"
                        );
                        text.clear();
                    }
                    quoted = !quoted;
                } else if quoted {
                    text.push(c);
                }
                at += 1;
            }
            assert!(!quoted, "{name}: balanced quotes");
            // the parameters are decoded from the base64 blob, never spliced
            assert!(
                !body.contains("FromBase64String"),
                "{name}: the blob is the command's"
            );
        }
        // a hostile value is data: it is inside the encoding and not in the
        // text of the command
        let odd = "it's$(x)`;y\"z";
        let script = WinScript::git_probe("C:\\gw\\data", odd, ".ign-probe-r1");
        let command = winps::encoded_command(&script, COMMAND_BUDGET).expect("fits");
        assert!(!command.as_str().contains("it's"));
        assert_eq!(script.params["j"], odd);
    }

    /// 27-11: the object listing fits the command budget with the longest
    /// realistic parameters (a 100-character project below the Windows data
    /// directory, the largest caps), with room to spare.
    #[test]
    fn winps_git_objects_fit_budget() {
        let data = "C:\\Program Files\\Inductive Automation\\Ignition\\data";
        let git = format!("{data}\\projects\\{}\\.git", "P".repeat(100));
        let script = WinScript::git_objects(winps::LONG_PATH_PREFIX, &git, usize::MAX, usize::MAX);
        let command = winps::encoded_command(&script, COMMAND_BUDGET).expect("fits");
        let used = command.as_str().chars().count();
        println!("winps_git_objects_fit_budget: {used} of {COMMAND_BUDGET}");
        assert!(used < COMMAND_BUDGET - 1_000, "{used} leaves no margin");
        // and a path that cannot fit is refused, never truncated
        let long = format!("C:\\{}", "d\\".repeat(3_000));
        let script = WinScript::git_objects("", &long, 1, 1);
        assert!(winps::encoded_command(&script, COMMAND_BUDGET).is_err());
    }

    /// 27-11: every value reaches the object listing as data in `$p`; the
    /// text holds no path, no evaluation and no `Add-Type`, and a hostile
    /// project is inside the encoding, not in the command text.
    #[test]
    fn winps_git_objects_params_only() {
        let body = winps::GIT_OBJECTS;
        assert!(!body.contains('\n'), "one line");
        for drive in ["C:\\", "c:\\", "\\\\?\\", "/Users", "/home", "/tmp"] {
            assert!(!body.contains(drive), "holds a path ({drive})");
        }
        for forbidden in [
            "Invoke-Expression",
            "iex ",
            "Invoke-Command",
            "Start-Process",
            "-Command",
            "cmd ",
            "Add-Type",
            "[scriptblock]::Create",
            "FromBase64String",
        ] {
            assert!(!body.contains(forbidden), "holds {forbidden}");
        }
        // the parameters it reads are exactly the ones the constructor sends
        let mut read: Vec<String> = body
            .match_indices("$p.")
            .map(|(at, _)| {
                body[at + 3..]
                    .chars()
                    .take_while(char::is_ascii_alphanumeric)
                    .collect()
            })
            .collect();
        read.sort();
        read.dedup();
        assert_eq!(read, ["a", "l", "m", "x"]);
        let odd = "it's$(x)`;y\"z";
        let script = WinScript::git_objects("", &format!("C:\\gw\\{odd}\\.git"), 5, 6);
        let command = winps::encoded_command(&script, COMMAND_BUDGET).expect("fits");
        assert!(!command.as_str().contains("it's"));
        let keys: Vec<&String> = script.params.as_object().expect("object").keys().collect();
        assert_eq!(keys, ["a", "l", "m", "x"]);
        assert_eq!(script.params["l"], 5);
        assert_eq!(script.params["m"], 6);
    }

    /// 27-11: the reads that existed before the object read keep their
    /// parameters. `bundle` sends no compression level, so the script keeps
    /// the `Optimal` of every resource read; the stored level is only ever
    /// asked for by `bundle_level`, with exactly one more key, and the body
    /// accepts nothing but that word.
    #[test]
    fn bundle_existing_reads_unchanged() {
        let old = WinScript::bundle("\\\\?\\", "C:\\d\\config\\resources", "L", "Z", "abc", 7);
        let keys: Vec<&String> = old.params.as_object().expect("object").keys().collect();
        assert_eq!(keys, ["b", "f", "h", "l", "x", "z"], "no `c` parameter");
        assert_eq!(
            old.params,
            serde_json::json!({
                "x": "\\\\?\\", "b": "C:\\d\\config\\resources", "l": "L", "z": "Z",
                "h": "abc", "f": 7,
            })
        );
        let same = WinScript::bundle_level(
            "\\\\?\\",
            "C:\\d\\config\\resources",
            "L",
            "Z",
            "abc",
            7,
            None,
        );
        assert_eq!(same.params, old.params);
        let stored = WinScript::bundle_level(
            "\\\\?\\",
            "C:\\d\\git",
            "L",
            "Z",
            "abc",
            7,
            Some("NoCompression"),
        );
        assert_eq!(stored.params["c"], "NoCompression");
        assert_eq!(stored.params.as_object().expect("object").len(), 7);
        assert_eq!(stored.body, old.body, "one script");
        let body = winps::BUNDLE;
        assert!(body.contains("CompressionLevel]::Optimal"), "the default");
        assert!(
            body.contains("$p.c -eq 'NoCompression'"),
            "one word accepted"
        );
        // the object listing and the archive both fit with their parameters
        for script in [&old, &stored] {
            let command = winps::encoded_command(script, COMMAND_BUDGET).expect("fits");
            assert!(command.as_str().chars().count() <= COMMAND_BUDGET);
        }
    }

    /// The manifest answer as the host-independent value.
    fn manifest_of(answer: &Value) -> GitManifest {
        GitManifest {
            files: answer["files"].as_u64().expect("files") as usize,
            bytes: answer["bytes"].as_u64().expect("bytes"),
            digest: answer["digest"].as_str().expect("digest").to_string(),
        }
    }

    fn b64(text: &str) -> Vec<u8> {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD
            .decode(text)
            .expect("base64")
    }

    fn file_of(value: &Value) -> Option<Vec<u8>> {
        if value.is_null() {
            None
        } else {
            Some(b64(value["b"].as_str().expect("bytes")))
        }
    }

    #[tokio::test]
    async fn pwsh_git_state() {
        if !pwsh_exec::gate("pwsh_git_state") {
            return;
        }
        let fx = GitFixture::new();
        std::fs::write(fx.git_dir().join("index.lock"), "").expect("lock");
        std::fs::write(fx.git_dir().join("MERGE_HEAD"), "x\n").expect("merge");
        fx.git_in_project(&["branch", "feature/x"]);
        fx.git_in_project(&["branch", "feature-y"]);
        fx.git_in_project(&["pack-refs", "--all"]);
        fx.git_in_project(&["branch", "late"]);
        let local: GitFiles = fx.node().read_git_state(PROJECT).await.expect("local");
        let markers: Vec<&str> = GIT_MARKERS.to_vec();
        let answer = pwsh_exec::run(&WinScript::git_state(&s(&fx.git_dir()), &markers))
            .expect("the state script");
        assert_eq!(answer["ok"], 1);
        assert_eq!(file_of(&answer["head"]), local.head);
        assert_eq!(file_of(&answer["config"]), local.config);
        assert_eq!(file_of(&answer["packed"]), local.packed_refs);
        assert_eq!(file_of(&answer["log"]), local.head_log_tail);
        assert_eq!(answer["index"], local.index_present);
        let mut marks: Vec<String> = answer["markers"]
            .as_array()
            .expect("markers")
            .iter()
            .map(|m| m.as_str().expect("marker").to_string())
            .collect();
        let mut want = local.markers.clone();
        marks.sort();
        want.sort();
        assert_eq!(marks, want);
        let mut refs: Vec<(String, Vec<u8>)> = answer["refs"]
            .as_array()
            .expect("refs")
            .iter()
            .map(|r| {
                (
                    String::from_utf8(b64(r["p"].as_str().expect("name"))).expect("utf8"),
                    file_of(r).expect("bytes"),
                )
            })
            .collect();
        let mut want = local.loose_refs.clone();
        refs.sort();
        want.sort();
        assert_eq!(refs, want);
        assert!(
            refs.iter().any(|(name, _)| name == "refs/heads/late"),
            "a loose ref after the pack: {refs:?}"
        );
        // a missing file is null, never an empty string
        std::fs::remove_file(fx.git_dir().join("packed-refs")).expect("rm");
        let again =
            pwsh_exec::run(&WinScript::git_state(&s(&fx.git_dir()), &markers)).expect("run");
        assert!(again["packed"].is_null());
        // a link in the way is a refusal, not an absent file
        let head = fx.git_dir().join("HEAD");
        std::fs::rename(&head, fx.git_dir().join("HEAD.real")).expect("rename");
        std::os::unix::fs::symlink("HEAD.real", &head).expect("symlink");
        let why = pwsh_exec::run(&WinScript::git_state(&s(&fx.git_dir()), &markers))
            .expect_err("a linked HEAD");
        assert!(why.contains("not a regular file"), "{why}");
        std::fs::remove_file(&head).expect("rm link");
        std::fs::rename(fx.git_dir().join("HEAD.real"), &head).expect("restore HEAD");
        // CodeRabbit #56: a linked directory above a file or a ref root is a
        // refusal too, never read through (`logs` above `logs/HEAD`, `refs`
        // above `refs/heads` and `refs/remotes`)
        let outside = tempfile::tempdir().expect("outside");
        std::fs::create_dir_all(outside.path().join("heads")).expect("heads");
        std::fs::write(outside.path().join("heads/stolen"), "1\n").expect("ref");
        std::fs::write(outside.path().join("HEAD"), "0 1 x\n").expect("log");
        for level in ["logs", "refs"] {
            let at = fx.git_dir().join(level);
            let aside = fx.git_dir().join(format!("{level}.real"));
            std::fs::rename(&at, &aside).expect("aside");
            std::os::unix::fs::symlink(outside.path(), &at).expect("link");
            let why = pwsh_exec::run(&WinScript::git_state(&s(&fx.git_dir()), &markers))
                .expect_err(level);
            assert!(why.contains("not a real directory"), "{level}: {why}");
            std::fs::remove_file(&at).expect("rm link");
            std::fs::rename(&aside, &at).expect("back");
        }
    }

    #[tokio::test]
    async fn pwsh_git_size() {
        if !pwsh_exec::gate("pwsh_git_size") {
            return;
        }
        let fx = GitFixture::new();
        let local = fx.node().git_size(PROJECT).await.expect("local size");
        let manifest = fx.node().git_manifest(PROJECT).await.expect("manifest");
        let answer = pwsh_exec::run(&WinScript::git_size("", &s(&fx.git_dir()))).expect("size");
        assert_eq!(answer["ok"], 1);
        assert_eq!(answer["files"].as_u64(), Some(local.files as u64));
        assert_eq!(answer["bytes"].as_u64(), Some(local.bytes));
        assert_eq!(local.files, manifest.files);
        assert_eq!(local.bytes, manifest.bytes);
        // a link anywhere inside refuses
        std::os::unix::fs::symlink(fx.data_dir.clone(), fx.git_dir().join("refs/loop"))
            .expect("link");
        let why = pwsh_exec::run(&WinScript::git_size("", &s(&fx.git_dir()))).expect_err("link");
        assert!(why.contains("link"), "{why}");
    }

    /// CodeRabbit #56: `git.yaml` comes back in the answer, as the local read
    /// gives it; absent, a link, a directory and an oversized file are told
    /// apart, never read through.
    #[tokio::test]
    async fn pwsh_git_yaml() {
        if !pwsh_exec::gate("pwsh_git_yaml") {
            return;
        }
        let fx = GitFixture::new();
        let data = s(&fx.data_dir);
        let run = |cap: u64| pwsh_exec::run(&WinScript::git_yaml(&data, cap)).expect("git_yaml");
        assert_eq!(run(64)["k"], "none");
        let yaml = fx.data_dir.join("git.yaml");
        std::fs::write(&yaml, "repos: {}\n").expect("yaml");
        let answer = run(64);
        assert_eq!(answer["k"], "file");
        assert_eq!(
            file_of(&answer),
            fx.node().read_git_yaml().await.expect("local")
        );
        assert_eq!(run(4)["k"], "big");
        std::fs::rename(&yaml, fx.data_dir.join("real.yaml")).expect("aside");
        std::os::unix::fs::symlink("real.yaml", &yaml).expect("link");
        assert_eq!(run(64)["k"], "symlink");
        std::fs::remove_file(&yaml).expect("rm");
        std::fs::create_dir(&yaml).expect("dir");
        assert_eq!(run(64)["k"], "other");
    }

    /// The manifest of a Windows script is the one the local walk computes,
    /// for names that sort differently by walk and by whole path.
    #[tokio::test]
    async fn pwsh_git_manifest_equals_local() {
        if !pwsh_exec::gate("pwsh_git_manifest_equals_local") {
            return;
        }
        let fx = GitFixture::new();
        let heads = fx.git_dir().join("refs/heads");
        std::fs::create_dir_all(heads.join("a")).expect("dir");
        std::fs::write(heads.join("a/b"), "1\n").expect("a/b");
        std::fs::write(heads.join("a-b"), "2\n").expect("a-b");
        std::fs::write(heads.join("caf\u{e9}"), "3\n").expect("accent");
        std::fs::write(heads.join("empty"), "").expect("empty");
        std::fs::write(fx.git_dir().join("objects/zero"), "").expect("zero");
        let local = fx.node().git_manifest(PROJECT).await.expect("local");
        let answer =
            pwsh_exec::run(&WinScript::git_manifest("", &s(&fx.git_dir()))).expect("manifest");
        assert_eq!(manifest_of(&answer), local);
        // an empty directory has the digest of nothing, like the other hosts
        let empty = fx.data_dir.join("empty-tree");
        std::fs::create_dir_all(&empty).expect("empty");
        let answer = pwsh_exec::run(&WinScript::git_manifest("", &s(&empty))).expect("empty");
        assert_eq!(answer["files"], 0);
        assert_eq!(answer["digest"], sha256_hex(b""));
        // a link or a name with a control character refuses
        std::os::unix::fs::symlink("HEAD", fx.git_dir().join("link")).expect("link");
        assert!(pwsh_exec::run(&WinScript::git_manifest("", &s(&fx.git_dir()))).is_err());
        std::fs::remove_file(fx.git_dir().join("link")).expect("rm");
        std::fs::write(fx.git_dir().join("bad\u{7}name"), "x").expect("bell");
        assert!(pwsh_exec::run(&WinScript::git_manifest("", &s(&fx.git_dir()))).is_err());
        std::fs::remove_file(fx.git_dir().join("bad\u{7}name")).expect("rm");
        // CodeRabbit #56: a linked root is refused, not walked as its target
        let linked = fx.data_dir.join("linked-git");
        std::os::unix::fs::symlink(fx.git_dir(), &linked).expect("link root");
        let why = pwsh_exec::run(&WinScript::git_manifest("", &s(&linked))).expect_err("root");
        assert!(why.contains("not a real directory"), "{why}");
    }

    /// The [`ProjectDisk`] of a project digests answer (`n` files in `t`).
    fn disk_of(answer: &Value) -> ignition_core::deploy::git_advance::ProjectDisk {
        use ignition_core::deploy::git_advance::DiskFile;
        let text = answer["t"].as_str().expect("t");
        let disk: ignition_core::deploy::git_advance::ProjectDisk = text
            .split('\n')
            .filter(|line| !line.is_empty())
            .map(|line| {
                let mut part = line.split('\t');
                let (rel, size, sha) = (
                    part.next().expect("name"),
                    part.next().expect("size"),
                    part.next().expect("sha"),
                );
                assert!(part.next().is_none(), "{line}");
                (
                    rel.to_string(),
                    DiskFile {
                        size: size.parse().expect("size"),
                        sha256: sha.to_string(),
                    },
                )
            })
            .collect();
        assert_eq!(disk.len() as u64, answer["n"].as_u64().expect("n"));
        disk
    }

    /// 27-41: the project digests of a Windows script are the local reader's,
    /// `.git` excluded, for names with spaces, an accent and an empty file; a
    /// tree over the cap refuses.
    #[tokio::test]
    async fn pwsh_project_digests() {
        if !pwsh_exec::gate("pwsh_project_digests") {
            return;
        }
        let fx = GitFixture::new();
        let project = fx.project_dir();
        for rel in [
            "ignition/script-python/with space/caf\u{e9}.py",
            "ignition/script-python/-dash/-x.py",
            "ignition/script-python/empty.py",
        ] {
            let path = project.join(rel);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
            let body = if rel.ends_with("empty.py") {
                ""
            } else {
                "body\n"
            };
            std::fs::write(&path, body).expect("file");
        }
        let local = fx
            .node()
            .project_file_digests(PROJECT)
            .await
            .expect("local");
        let data = s(&fx.data_dir);
        let answer =
            pwsh_exec::run(&WinScript::project_digests("", &data, PROJECT, 1000)).expect("digests");
        let got = disk_of(&answer);
        assert_eq!(got, local);
        assert!(got.len() > 10, "{got:?}");
        assert!(got.keys().all(|name| !name.starts_with(".git")), "{got:?}");
        assert!(got.values().any(|file| file.size == 0), "an empty file");
        // a `.git` that holds a link is not looked at
        std::os::unix::fs::symlink("HEAD", fx.git_dir().join("a-link")).expect("link in .git");
        let again = pwsh_exec::run(&WinScript::project_digests("", &data, PROJECT, 1000))
            .expect("digests again");
        assert_eq!(disk_of(&again), local);
        // over the cap: refused, never cut off
        let why = pwsh_exec::run(&WinScript::project_digests("", &data, PROJECT, 3))
            .expect_err("over the cap");
        assert!(why.contains("more files"), "{why}");
    }

    /// 27-41: a link anywhere below the project (not below `.git`), a name with
    /// a control character, a project that is itself a link and a missing
    /// project all refuse.
    #[tokio::test]
    async fn pwsh_project_digests_refuses_a_link() {
        if !pwsh_exec::gate("pwsh_project_digests_refuses_a_link") {
            return;
        }
        let fx = GitFixture::new();
        let data = s(&fx.data_dir);
        let run = || pwsh_exec::run(&WinScript::project_digests("", &data, PROJECT, 1000));
        let nested = fx.project_dir().join("ignition/script-python/cap");
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("secret"), "outside\n").expect("secret");
        for target in [outside.path().join("secret"), outside.path().to_path_buf()] {
            let at = nested.join("planted");
            std::os::unix::fs::symlink(&target, &at).expect("link");
            let why = run().expect_err("a link");
            assert!(why.contains("link"), "{why}");
            std::fs::remove_file(&at).expect("rm link");
        }
        let odd = nested.join("bad\u{7}name");
        std::fs::write(&odd, "x").expect("odd name");
        let why = run().expect_err("a control character");
        assert!(why.contains("control character"), "{why}");
        std::fs::remove_file(&odd).expect("rm");
        run().expect("clean again");
        // the project directory itself may not be a link, or missing
        let real = fx.data_dir.join("projects/Real");
        std::fs::rename(fx.project_dir(), &real).expect("move");
        let why = run().expect_err("a missing project");
        assert!(why.contains("not a real directory"), "{why}");
        std::os::unix::fs::symlink(&real, fx.project_dir()).expect("link project");
        let why = run().expect_err("a linked project");
        assert!(why.contains("not a real directory"), "{why}");
    }

    /// The listing of the object WinScript as the entries it reports.
    fn objects_entries(answer: &Value) -> Vec<ignition_core::deploy::git_objects::GitObjectEntry> {
        use ignition_core::deploy::git_objects::{GitObjectEntry, GitObjectEntryKind};
        answer["t"]
            .as_str()
            .expect("t")
            .split('\n')
            .filter(|line| !line.is_empty())
            .map(|line| {
                let (size, rel) = line.split_once(' ').expect("size name");
                GitObjectEntry {
                    rel: rel.to_string(),
                    size: size.parse().expect("size"),
                    kind: GitObjectEntryKind::File,
                }
            })
            .collect()
    }

    /// 27-11: the object listing under the local `pwsh` reports what the
    /// local transport lists (loose objects and packs together), stops at its
    /// report cap with `more`, refuses at its walk cap, and refuses a link, a
    /// name with a control character or a colon, and a missing or linked
    /// `objects` directory.
    #[tokio::test]
    async fn pwsh_git_objects() {
        use ignition_core::deploy::git_objects::GitObjectList;
        if !pwsh_exec::gate("pwsh_git_objects") {
            return;
        }
        let fx = GitFixture::new();
        fx.gateway_gc();
        fx.designer_commit(false);
        let local = fx.node().list_git_objects(PROJECT).await.expect("local");
        assert!(!local.loose.is_empty() && !local.packs.is_empty());
        let list = |walk: usize, report: usize| {
            pwsh_exec::run(&WinScript::git_objects("", &s(&fx.git_dir()), walk, report))
        };
        let answer = list(200_000, 51_025).expect("the listing");
        assert_eq!(
            (answer["ok"].clone(), answer["more"].clone()),
            (1.into(), 0.into())
        );
        let entries = objects_entries(&answer);
        assert_eq!(answer["n"].as_u64(), Some(entries.len() as u64));
        let got = GitObjectList::from_entries(&entries).expect("from entries");
        assert_eq!(got, local);

        // a report cap stops with `more`; a walk cap refuses
        let capped = list(200_000, 2).expect("capped");
        assert_eq!(
            (capped["more"].clone(), capped["n"].clone()),
            (1.into(), 2.into())
        );
        let why = list(3, 51_025).expect_err("walk cap");
        assert!(why.contains("more entries"), "{why}");

        // names of no fixed shape are never reported, hostile ones refuse
        let objects = fx.git_dir().join("objects");
        std::fs::write(objects.join("pack").join("pack-x.keep"), "").expect("keep");
        std::fs::write(objects.join("info").join("packs"), "x\n").expect("info");
        std::fs::write(objects.join("0a_short"), "x").expect("short");
        assert_eq!(
            GitObjectList::from_entries(&objects_entries(&list(200_000, 51_025).expect("again")))
                .expect("list"),
            local
        );
        std::fs::write(objects.join("pack/a:b"), "x").expect("colon");
        assert!(
            list(200_000, 51_025)
                .expect_err("colon")
                .contains("plain name")
        );
        std::fs::remove_file(objects.join("pack/a:b")).expect("rm");
        std::fs::write(objects.join("pack/bad\u{7}name"), "x").expect("bell");
        assert!(list(200_000, 51_025).is_err());
        std::fs::remove_file(objects.join("pack/bad\u{7}name")).expect("rm");
        std::os::unix::fs::symlink("/", objects.join("pack/link")).expect("link");
        assert!(list(200_000, 51_025).expect_err("link").contains("link"));
        std::fs::remove_file(objects.join("pack/link")).expect("rm");
        std::os::unix::fs::symlink("/", objects.join("lnk")).expect("dir link");
        assert!(
            list(200_000, 51_025)
                .expect_err("dir link")
                .contains("link")
        );
        std::fs::remove_file(objects.join("lnk")).expect("rm");
        assert!(list(200_000, 51_025).is_ok());

        // objects missing, or a link
        let moved = fx.git_dir().join("objects-real");
        std::fs::rename(&objects, &moved).expect("move");
        assert!(list(200_000, 51_025).is_err(), "missing objects");
        std::os::unix::fs::symlink(&moved, &objects).expect("link objects");
        let why = list(200_000, 51_025).expect_err("linked objects");
        assert!(why.contains("not a real directory"), "{why}");
    }

    #[tokio::test]
    async fn pwsh_git_save_restore() {
        if !pwsh_exec::gate("pwsh_git_save_restore") {
            return;
        }
        let fx = GitFixture::new();
        let home = tempfile::tempdir().expect("home");
        let (data, home_s) = (s(&fx.data_dir), s(home.path()));
        // a ReadOnly file, as git sets on its pack and object files (WR-B5):
        // the copy keeps the attribute, and the removal at the end must cope
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                fx.git_dir().join("HEAD"),
                std::fs::Permissions::from_mode(0o444),
            )
            .expect("read-only HEAD");
        }
        let local = fx.node().git_manifest(PROJECT).await.expect("local");
        let save = || WinScript::git_save("", &data, PROJECT, &home_s, GATEWAY, RUN);
        let restore = || WinScript::git_restore("", &data, PROJECT, &home_s, GATEWAY, RUN);

        let saved = pwsh_exec::run(&save()).expect("save");
        assert_eq!(manifest_of(&saved), local, "the save equals the source");
        let repo = home
            .path()
            .join("ign-deploy")
            .join(GATEWAY)
            .join("projects")
            .join(PROJECT)
            .join("git-save")
            .join(RUN)
            .join("repo");
        assert!(repo.join("HEAD").is_file());
        // a save is never overwritten
        let why = pwsh_exec::run(&save()).expect_err("second save");
        assert!(why.contains("already exists"), "{why}");
        // and the failed second save left the first alone
        assert_eq!(
            manifest_of(&pwsh_exec::run(&WinScript::git_manifest("", &s(&repo))).expect("m")),
            local
        );

        // restore refuses while `.git` exists
        let why = pwsh_exec::run(&restore()).expect_err("restore over .git");
        assert!(why.contains("never replaced"), "{why}");
        assert_eq!(
            fx.node().git_manifest(PROJECT).await.expect("intact"),
            local
        );

        // the import deletes `.git`; the restore brings it back, equal
        std::fs::remove_dir_all(fx.git_dir()).expect("rm .git");
        let back = pwsh_exec::run(&restore()).expect("restore");
        assert_eq!(manifest_of(&back), local, "the restore equals the save");
        assert_eq!(fx.node().git_manifest(PROJECT).await.expect("after"), local);
        let stage = fx.project_dir().join(format!(".ign-restore-{RUN}"));
        assert!(!stage.exists(), "no stage is left behind");

        // a planted stage (a link) is refused and its target is left alone
        std::fs::remove_dir_all(fx.git_dir()).expect("rm .git");
        let target = fx.data_dir.join("elsewhere");
        std::fs::create_dir_all(&target).expect("target");
        std::os::unix::fs::symlink(&target, &stage).expect("plant");
        let why = pwsh_exec::run(&restore()).expect_err("planted stage");
        assert!(why.contains("already exists"), "{why}");
        assert_eq!(
            std::fs::read_dir(&target).expect("ls").count(),
            0,
            "nothing written through"
        );
        assert!(!fx.git_dir().exists(), "nothing was restored");
        std::fs::remove_file(&stage).expect("unplant");

        // a restore proves the copy: a save changed behind the script's back
        // after the manifest cannot be seen from here, but a save that is not
        // a real directory is refused
        let moved = repo.with_file_name("repo-real");
        std::fs::rename(&repo, &moved).expect("move");
        std::os::unix::fs::symlink(&moved, &repo).expect("link repo");
        assert!(
            pwsh_exec::run(&restore()).is_err(),
            "a linked repo is refused"
        );
        assert!(!fx.git_dir().exists());
        // the save holds the ReadOnly file; its removal clears it and succeeds
        std::fs::remove_file(&repo).expect("unlink repo");
        std::fs::rename(&moved, &repo).expect("move back");
        let rel = format!("ign-deploy/{GATEWAY}/projects/{PROJECT}/git-save/{RUN}/repo");
        let removed =
            pwsh_exec::run(&WinScript::git_save_rm("", &home_s, &rel, RUN)).expect("remove");
        assert_eq!(removed["removed"], 1);
        assert!(!repo.exists(), "a save with a ReadOnly file is removed");
    }

    /// WR-B5: the removal of a save clears ReadOnly on the files first, never
    /// enters or changes what a link points at, and a delete that still fails
    /// names the leftover path with the real error.
    #[tokio::test]
    async fn pwsh_git_save_rm_clears_readonly_and_names_what_is_left() {
        use std::os::unix::fs::PermissionsExt;
        if !pwsh_exec::gate("pwsh_git_save_rm_clears_readonly_and_names_what_is_left") {
            return;
        }
        let fx = GitFixture::new();
        let home = tempfile::tempdir().expect("home");
        if super::sftp_git::ignores_modes(home.path()) {
            println!(
                "running with privileges that ignore mode bits; the 0555 denial case cannot run"
            );
            return;
        }
        let (data, home_s) = (s(&fx.data_dir), s(home.path()));
        pwsh_exec::run(&WinScript::git_save(
            "", &data, PROJECT, &home_s, GATEWAY, RUN,
        ))
        .expect("save");
        let rel = format!("ign-deploy/{GATEWAY}/projects/{PROJECT}/git-save/{RUN}");
        let repo = home.path().join(&rel).join("repo");
        let rm = || WinScript::git_save_rm("", &home_s, &format!("{rel}/repo"), RUN);
        // a ReadOnly file, and a link to a directory outside holding another
        let readonly = repo.join("HEAD");
        std::fs::set_permissions(&readonly, std::fs::Permissions::from_mode(0o444)).expect("ro");
        let outside = tempfile::tempdir().expect("outside");
        let precious = outside.path().join("precious");
        std::fs::write(&precious, "keep").expect("precious");
        std::fs::set_permissions(&precious, std::fs::Permissions::from_mode(0o444)).expect("ro");
        std::os::unix::fs::symlink(outside.path(), repo.join("link")).expect("link");
        // a directory that cannot be emptied: the delete fails, and says where
        let locked = repo.join("objects");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).expect("lock");
        let why = pwsh_exec::run(&rm()).expect_err("an undeletable tree");
        assert!(
            why.contains("could not be removed") && why.contains(&s(&repo)),
            "the leftover is named: {why}"
        );
        assert!(repo.is_dir(), "nothing else was touched");
        // a ReadOnly file that is also a hard link to a file outside the tree: its
        // attribute is shared with the other link, so it is not changed
        let shared = outside.path().join("shared");
        std::fs::write(&shared, "keep").expect("shared");
        std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o444)).expect("ro");
        std::fs::hard_link(&shared, repo.join("shared")).expect("hard link");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("unlock");
        let done = pwsh_exec::run(&rm()).expect("remove");
        assert_eq!(
            std::fs::metadata(&shared)
                .expect("meta")
                .permissions()
                .mode()
                & 0o222,
            0,
            "a hard-linked file was not changed"
        );
        assert_eq!(done["removed"], 1);
        assert!(!repo.exists(), "the save is gone, ReadOnly file included");
        assert_eq!(
            std::fs::read(&precious).expect("the link target is intact"),
            b"keep"
        );
        assert_eq!(
            std::fs::metadata(&precious)
                .expect("meta")
                .permissions()
                .mode()
                & 0o222,
            0,
            "a file behind a link was not changed"
        );
    }

    #[tokio::test]
    async fn pwsh_git_refuses_links_and_foreign_paths() {
        if !pwsh_exec::gate("pwsh_git_refuses_links_and_foreign_paths") {
            return;
        }
        let fx = GitFixture::new();
        let home = tempfile::tempdir().expect("home");
        let (data, home_s) = (s(&fx.data_dir), s(home.path()));
        // a link inside `.git` refuses the save and leaves no `repo`
        std::os::unix::fs::symlink("HEAD", fx.git_dir().join("link")).expect("link");
        let why = pwsh_exec::run(&WinScript::git_save(
            "", &data, PROJECT, &home_s, GATEWAY, RUN,
        ))
        .expect_err("save with a link");
        assert!(why.contains("link"), "{why}");
        let run_dir = home.path().join(format!(
            "ign-deploy/{GATEWAY}/projects/{PROJECT}/git-save/{RUN}"
        ));
        assert!(!run_dir.join("repo").exists(), "no partial save");
        std::fs::remove_file(fx.git_dir().join("link")).expect("rm link");

        // a link on the way into the save path refuses
        std::fs::create_dir_all(home.path().join("ign-deploy")).expect("dir");
        let elsewhere = tempfile::tempdir().expect("elsewhere");
        std::os::unix::fs::symlink(
            elsewhere.path(),
            home.path().join("ign-deploy").join(GATEWAY),
        )
        .expect("link");
        let why = pwsh_exec::run(&WinScript::git_save(
            "", &data, PROJECT, &home_s, GATEWAY, RUN,
        ))
        .expect_err("save through a link");
        assert!(why.contains("not a real directory"), "{why}");
        assert_eq!(std::fs::read_dir(elsewhere.path()).expect("ls").count(), 0);
        std::fs::remove_file(home.path().join("ign-deploy").join(GATEWAY)).expect("unlink");

        // remove_git_save: a foreign path is refused before anything is touched
        let saved = pwsh_exec::run(&WinScript::git_save(
            "", &data, PROJECT, &home_s, GATEWAY, RUN,
        ))
        .expect("save");
        assert_eq!(saved["ok"], 1);
        let rel = format!("ign-deploy/{GATEWAY}/projects/{PROJECT}/git-save/{RUN}");
        for foreign in [
            format!("{rel}/other"),
            format!("{rel}/repo/objects"),
            "ign-deploy/x/projects/y/git-save/other-run/repo".to_string(),
            format!("../{rel}/repo"),
            "repo".to_string(),
        ] {
            let why = pwsh_exec::run(&WinScript::git_save_rm("", &home_s, &foreign, RUN))
                .expect_err(&foreign);
            assert!(why.contains("refusing"), "{foreign}: {why}");
        }
        assert!(
            run_dir.join("repo").is_dir(),
            "the refusals removed nothing"
        );
        let removed = pwsh_exec::run(&WinScript::git_save_rm(
            "",
            &home_s,
            &format!("{rel}/repo"),
            RUN,
        ))
        .expect("remove");
        assert_eq!(removed["removed"], 1);
        assert!(!run_dir.exists(), "the empty run directory goes too");
        let again = pwsh_exec::run(&WinScript::git_save_rm(
            "",
            &home_s,
            &format!("{rel}/repo"),
            RUN,
        ))
        .expect("absent");
        assert_eq!(again["removed"], 0, "an absent save is a success");

        // the probe: one file in each directory, removed; an existing one is
        // refused and left
        let probe = ".ign-probe-r1";
        let ok = pwsh_exec::run(&WinScript::git_probe(&data, PROJECT, probe)).expect("probe");
        assert_eq!(ok["ok"], 1);
        assert!(!fx.project_dir().join(probe).exists());
        assert!(!fx.git_dir().join(probe).exists());
        std::fs::write(fx.git_dir().join(probe), "mine").expect("planted");
        let why = pwsh_exec::run(&WinScript::git_probe(&data, PROJECT, probe))
            .expect_err("planted probe");
        assert!(
            why.contains(".git"),
            "names the directory that refused: {why}"
        );
        assert_eq!(
            std::fs::read(fx.git_dir().join(probe)).expect("kept"),
            b"mine"
        );

        // free space of a save root that does not exist yet
        let free =
            pwsh_exec::run(&WinScript::git_free(&home_s, &format!("{rel}/free"))).expect("free");
        assert!(free["free"].as_u64().expect("free") > 0);
    }

    /// A project's `.git` and the private stage a Windows write fills its locks
    /// from (27-33): the stage lives in the account home, the bytes of plan
    /// index `n` are the file `n` in it.
    struct WriteRig {
        fx: GitFixture,
        home: tempfile::TempDir,
        data: String,
        home_s: String,
        stage: String,
    }

    impl WriteRig {
        fn new() -> WriteRig {
            let fx = GitFixture::new();
            let home = tempfile::tempdir().expect("home");
            let (data, home_s) = (s(&fx.data_dir), s(home.path()));
            WriteRig {
                fx,
                home,
                data,
                home_s,
                stage: "git-stage-w33a".to_string(),
            }
        }

        fn git(&self) -> PathBuf {
            self.fx.git_dir()
        }

        fn stage_dir(&self) -> PathBuf {
            self.home.path().join("ign-deploy").join(&self.stage)
        }

        fn make_stage(&self) -> Result<Value, String> {
            pwsh_exec::run(&WinScript::git_stage(&self.home_s, &self.stage))
        }

        /// The bytes of plan index `index`, as sftp would have put them.
        fn put(&self, index: usize, bytes: &[u8]) {
            std::fs::write(self.stage_dir().join(index.to_string()), bytes).expect("put");
        }

        fn lock(&self, offset: usize, writes: &[(&str, u64)]) -> Result<Value, String> {
            let writes: Vec<(String, u64)> =
                writes.iter().map(|(p, n)| ((*p).to_string(), *n)).collect();
            pwsh_exec::run(&WinScript::git_lock(
                &self.data,
                PROJECT,
                &self.home_s,
                &self.stage,
                offset,
                &writes,
            ))
        }

        fn unlock(&self, writes: &[(&str, u64)], remove_stage: bool) -> Result<Value, String> {
            let writes: Vec<(String, u64)> =
                writes.iter().map(|(p, n)| ((*p).to_string(), *n)).collect();
            pwsh_exec::run(&WinScript::git_unlock(
                &self.data,
                PROJECT,
                &self.home_s,
                &self.stage,
                0,
                &writes,
                remove_stage,
            ))
        }

        fn replace(&self, writes: &[(&str, u64)], remove_stage: bool) -> Result<Value, String> {
            let writes: Vec<(String, u64)> =
                writes.iter().map(|(p, n)| ((*p).to_string(), *n)).collect();
            pwsh_exec::run(&WinScript::git_replace(
                &self.data,
                PROJECT,
                &self.home_s,
                &self.stage,
                &writes,
                2,
                remove_stage,
            ))
        }
    }

    #[tokio::test]
    async fn pwsh_git_write_lock_replace_digest() {
        if !pwsh_exec::gate("pwsh_git_write_lock_replace_digest") {
            return;
        }
        let rig = WriteRig::new();
        let (fx, git) = (&rig.fx, s(&rig.fx.git_dir()));
        let pack = format!("objects/pack/pack-{}.pack", "a".repeat(40));
        let new_head = b"ref: refs/heads/feature/new\n".to_vec();
        let new_ref = b"0123456789abcdef0123456789abcdef01234567\n".to_vec();
        let new_pack = vec![b'P'; 4096];
        let plan = [
            ("refs/heads/feature/new", new_ref.len() as u64),
            ("HEAD", new_head.len() as u64),
            (pack.as_str(), new_pack.len() as u64),
        ];
        let head_before = std::fs::read(fx.git_dir().join("HEAD")).expect("head");

        // the stage is made once and never reused
        assert_eq!(rig.make_stage().expect("stage")["ok"], 1);
        assert!(rig.stage_dir().is_dir());
        let why = rig.make_stage().expect_err("a second stage of the name");
        assert!(why.contains("already exists"), "{why}");
        // a stage name that is not one component of git-stage-* is refused
        let bad = pwsh_exec::run(&WinScript::git_stage(&rig.home_s, "../escape"));
        assert!(bad.is_err());
        assert!(!rig.home.path().join("escape").exists());

        rig.put(0, &new_ref);
        rig.put(1, &new_head);
        rig.put(2, &new_pack);
        // the locks are created and filled from the stage, byte for byte
        assert_eq!(rig.lock(0, &plan).expect("lock")["ok"], 1);
        for (lock, bytes) in [
            ("refs/heads/feature/new.lock", &new_ref),
            ("HEAD.lock", &new_head),
            (&*format!("{pack}.ign-tmp"), &new_pack),
        ] {
            assert_eq!(&std::fs::read(fx.git_dir().join(lock)).expect(lock), bytes);
        }
        // a second writer is refused and the first one's locks stay
        let why = rig.lock(0, &plan).expect_err("held");
        assert!(why.contains("already exists"), "{why}");
        assert_eq!(
            std::fs::read(fx.git_dir().join("HEAD.lock")).expect("lock"),
            new_head
        );
        // a lock call that fails halfway takes back what it made this call, and
        // leaves the lock another writer holds
        let rig2 = WriteRig::new();
        rig2.make_stage().expect("stage");
        rig2.put(0, b"other\n");
        rig2.put(1, b"[core]\n");
        std::fs::write(rig2.git().join("config.lock"), "held").expect("foreign lock");
        let both = [("refs/heads/other", 6), ("config", 7)];
        assert!(rig2.lock(0, &both).is_err());
        assert!(!rig2.git().join("refs/heads/other.lock").exists());
        assert_eq!(
            std::fs::read_to_string(rig2.git().join("config.lock")).expect("kept"),
            "held",
            "a foreign lock is left"
        );
        // bytes in the stage that are not the size that was sent are refused before
        // any lock exists
        let rig3 = WriteRig::new();
        rig3.make_stage().expect("stage");
        rig3.put(0, b"abc");
        let why = rig3.lock(0, &[("refs/heads/x", 4)]).expect_err("size");
        assert!(why.contains("size"), "{why}");
        assert!(!rig3.git().join("refs/heads/x.lock").exists());

        // a size that is not the size sent is refused by the replace
        let wrong = [("HEAD", new_head.len() as u64 + 1)];
        let why = rig.replace(&wrong, false).expect_err("size");
        assert!(why.contains("size"), "{why}");
        assert_eq!(
            std::fs::read(fx.git_dir().join("HEAD")).expect("head"),
            head_before
        );
        // the replace puts every lock over its file and removes the stage
        let done = rig.replace(&plan, true).expect("replace");
        assert_eq!(done["ok"], 1);
        assert_eq!(
            std::fs::read(fx.git_dir().join("HEAD")).expect("head"),
            new_head
        );
        assert_eq!(
            std::fs::read(fx.git_dir().join("refs/heads/feature/new")).expect("ref"),
            new_ref
        );
        assert_eq!(
            std::fs::read(fx.git_dir().join(&pack)).expect("pack"),
            new_pack
        );
        for gone in ["HEAD.lock", "refs/heads/feature/new.lock"] {
            assert!(!fx.git_dir().join(gone).exists(), "{gone}");
        }
        assert!(!rig.stage_dir().exists(), "no stage remains");
        // the read-back digest is the sha256 of what was written
        let digest = pwsh_exec::run(&WinScript::git_digest(&git, "HEAD")).expect("digest");
        assert_eq!(digest["digest"], sha256_hex(&new_head));
        let absent =
            pwsh_exec::run(&WinScript::git_digest(&git, "refs/heads/none")).expect("absent");
        assert!(absent["digest"].is_null());
        let nodir = pwsh_exec::run(&WinScript::git_digest(&git, "no/dir/file")).expect("nodir");
        assert!(nodir["digest"].is_null());

        // the cleanup removes the locks that hold ign's bytes and the stage
        let rig4 = WriteRig::new();
        rig4.make_stage().expect("stage");
        rig4.put(0, b"again\n");
        rig4.lock(0, &[("refs/heads/again", 6)]).expect("lock");
        let cleaned = rig4
            .unlock(&[("refs/heads/again", 6)], true)
            .expect("unlock");
        assert_eq!(
            (cleaned["removed"].as_u64(), cleaned["kept"].as_u64()),
            (Some(1), Some(0))
        );
        assert!(!rig4.git().join("refs/heads/again.lock").exists());
        assert!(!rig4.stage_dir().exists());
        // a link as a lock target is refused
        let rig5 = WriteRig::new();
        rig5.make_stage().expect("stage");
        rig5.put(0, b"x");
        std::os::unix::fs::symlink("HEAD", rig5.git().join("refs/heads/linked")).expect("link");
        assert!(rig5.lock(0, &[("refs/heads/linked", 1)]).is_err());
    }

    /// 27-33 (WR-B3): a link planted where the lock goes, or in a level above
    /// it, or in the stage, is refused: nothing is written through it and no
    /// target in `.git` changes.
    #[tokio::test]
    async fn pwsh_git_write_refuses_a_planted_link_at_a_lock() {
        if !pwsh_exec::gate("pwsh_git_write_refuses_a_planted_link_at_a_lock") {
            return;
        }
        let outside = tempfile::tempdir().expect("outside");
        let victim = outside.path().join("victim");
        std::fs::write(&victim, "precious\n").expect("victim");
        let head = |rig: &WriteRig| std::fs::read(rig.git().join("HEAD")).expect("head");

        // a link at the lock path: to a file, and dangling
        for target in [victim.clone(), outside.path().join("not-there")] {
            let rig = WriteRig::new();
            rig.make_stage().expect("stage");
            rig.put(0, b"ref: refs/heads/x\n");
            let before = head(&rig);
            std::os::unix::fs::symlink(&target, rig.git().join("HEAD.lock")).expect("plant");
            let why = rig
                .lock(0, &[("HEAD", 18)])
                .expect_err("a link at the lock");
            assert!(why.contains("already exists"), "{why}");
            assert_eq!(std::fs::read(&victim).expect("victim"), b"precious\n");
            assert!(
                !outside.path().join("not-there").exists(),
                "nothing was created behind the link"
            );
            assert_eq!(head(&rig), before, "no target changed");
            assert!(
                std::fs::symlink_metadata(rig.git().join("HEAD.lock"))
                    .expect("still there")
                    .file_type()
                    .is_symlink(),
                "the planted link is not ours and stays"
            );
        }
        // a link in a level above the lock: refused, nothing created behind it
        let rig = WriteRig::new();
        rig.make_stage().expect("stage");
        rig.put(0, b"x");
        std::os::unix::fs::symlink(outside.path(), rig.git().join("refs/heads/feature"))
            .expect("plant");
        let why = rig
            .lock(0, &[("refs/heads/feature/new", 1)])
            .expect_err("a link above");
        assert!(why.contains("not a real directory"), "{why}");
        assert!(!outside.path().join("new.lock").exists());
        // a link as the project's .git is refused outright
        let rig = WriteRig::new();
        rig.make_stage().expect("stage");
        rig.put(0, b"x");
        let moved = rig.git().with_file_name("real-git");
        std::fs::rename(rig.git(), &moved).expect("move");
        std::os::unix::fs::symlink(&moved, rig.git()).expect("link .git");
        let why = rig.lock(0, &[("HEAD", 1)]).expect_err("a linked .git");
        assert!(why.contains("not a real directory"), "{why}");
        assert!(!moved.join("HEAD.lock").exists());
        // a link in the stage in place of the staged bytes is refused
        let rig = WriteRig::new();
        rig.make_stage().expect("stage");
        std::os::unix::fs::symlink(&victim, rig.stage_dir().join("0")).expect("plant");
        let why = rig
            .lock(0, &[("HEAD", 9)])
            .expect_err("a link in the stage");
        assert!(why.contains("staged bytes"), "{why}");
        assert!(!rig.git().join("HEAD.lock").exists());
        // the replace does not go through a link planted in a level above either
        let rig = WriteRig::new();
        rig.make_stage().expect("stage");
        rig.put(0, b"y");
        rig.lock(0, &[("refs/heads/main", 1)]).expect("lock");
        let heads = rig.git().join("refs/heads");
        let moved = rig.git().join("refs/heads-real");
        std::fs::rename(&heads, &moved).expect("move heads");
        std::os::unix::fs::symlink(&moved, &heads).expect("link heads");
        let why = rig
            .replace(&[("refs/heads/main", 1)], true)
            .expect_err("a link above");
        assert!(why.contains("not a real directory"), "{why}");
    }

    /// 27-33 (WR-B2): the cleanup removes a lock only when its bytes equal the
    /// staged bytes: a lock another writer holds (JGit's `index.lock`) stays.
    #[tokio::test]
    async fn pwsh_git_write_cleanup_spares_a_foreign_lock() {
        if !pwsh_exec::gate("pwsh_git_write_cleanup_spares_a_foreign_lock") {
            return;
        }
        let rig = WriteRig::new();
        rig.make_stage().expect("stage");
        rig.put(0, b"ref: refs/heads/x\n");
        rig.put(1, b"the staged index");
        let plan = [("HEAD", 18), ("index", 16)];
        // our lock holds the staged bytes; the other one is JGit's, with other
        // bytes of the same length, and a third entry has no lock at all
        std::fs::write(rig.git().join("HEAD.lock"), b"ref: refs/heads/x\n").expect("own");
        std::fs::write(rig.git().join("index.lock"), b"jgit's own index!").expect("foreign");
        let done = rig.unlock(&plan, true).expect("cleanup");
        assert_eq!(
            (done["removed"].as_u64(), done["kept"].as_u64()),
            (Some(1), Some(1))
        );
        assert!(!rig.git().join("HEAD.lock").exists(), "our lock went");
        assert_eq!(
            std::fs::read(rig.git().join("index.lock")).expect("foreign"),
            b"jgit's own index!",
            "the foreign lock stays"
        );
        assert!(!rig.stage_dir().exists(), "the stage went");
        // without a stage nothing can be proved ours: every lock stays, and
        // that is not an error
        let rig = WriteRig::new();
        std::fs::write(rig.git().join("HEAD.lock"), b"ref: refs/heads/x\n").expect("lock");
        let done = rig.unlock(&[("HEAD", 18)], true).expect("no stage");
        assert_eq!(
            (done["removed"].as_u64(), done["kept"].as_u64()),
            (Some(0), Some(1))
        );
        assert!(rig.git().join("HEAD.lock").exists());
        // a link as a lock is never removed (it is not a regular file)
        let rig = WriteRig::new();
        rig.make_stage().expect("stage");
        rig.put(0, b"x");
        std::os::unix::fs::symlink("HEAD", rig.git().join("config.lock")).expect("link");
        let done = rig.unlock(&[("config", 1)], false).expect("cleanup");
        assert_eq!(done["removed"], 0);
        assert!(rig.git().join("config.lock").symlink_metadata().is_ok());
    }
}
