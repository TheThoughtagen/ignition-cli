//! Running the Windows git WinScripts of 27-09 under the local `pwsh`.
//!
//! The text a test runs is the text a Windows host receives: the same
//! `-EncodedCommand` that `encoded_command` builds, so the 8000-character
//! command budget is enforced here too. Two differences from a real host, both
//! stated where they matter:
//!
//! - the host runs Windows PowerShell 5.1, this runs PowerShell 7 (the live
//!   run on the Windows test VM covers 5.1);
//! - `robocopy` is a Windows program, so on a unix machine a stand-in named
//!   `robocopy` is put first on `PATH`: it copies `SRC/.` to `DST/` with `cp -R`
//!   and exits 1 ("files copied"), as the real tool does. The scripts still call
//!   `robocopy` with their real arguments; only the copy itself differs, so a
//!   timing taken here measures the script's own work (the walk, the hashing
//!   and the compare) and a local copy, not robocopy.
//!
//! A runner without `pwsh` prints `PWSH-SKIPPED <name> (no pwsh)` and the test
//! passes; a runner with it prints `PWSH-RAN <name>`. Never a silent skip.

#![cfg(unix)]

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use ignition_core::deploy::transport::process::ProcessOutput;
use ignition_core::deploy::transport::winps::{
    COMMAND_BUDGET, WinScript, encoded_command, interpret_output,
};

const ROBOCOPY_STANDIN: &str =
    "#!/bin/sh\nmkdir -p \"$2\" && cp -R \"$1\"/. \"$2\"/ && exit 1\nexit 16\n";

/// The PowerShell a test can run, if any.
pub fn pwsh() -> Option<PathBuf> {
    for name in ["pwsh", "powershell"] {
        if let Some(paths) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&paths) {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    [
        "/usr/local/bin/pwsh",
        "/opt/homebrew/bin/pwsh",
        "/usr/bin/pwsh",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|path| path.is_file())
}

/// Print the visible record of a row and whether it can run: `true` when
/// `pwsh` exists (and `PWSH-RAN <name>` is printed), `false` after
/// `PWSH-SKIPPED <name> (no pwsh)`.
pub fn gate(name: &str) -> bool {
    if pwsh().is_some() {
        println!("PWSH-RAN {name}");
        true
    } else {
        println!("PWSH-SKIPPED {name} (no pwsh)");
        false
    }
}

fn standin_dir() -> &'static PathBuf {
    static DIR: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    &DIR.get_or_init(|| {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("stand-in dir");
        let path = dir.path().join("robocopy");
        std::fs::write(&path, ROBOCOPY_STANDIN).expect("stand-in");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let shown = dir.path().to_path_buf();
        (dir, shown)
    })
    .1
}

/// The full process output of `script` under `pwsh` (the command as built for
/// a host, within the real budget).
pub fn output(script: &WinScript) -> Result<ProcessOutput, String> {
    let pwsh = pwsh().ok_or_else(|| "no pwsh".to_string())?;
    let command = encoded_command(script, COMMAND_BUDGET)?;
    let encoded = command
        .as_str()
        .strip_prefix("powershell -NoProfile -NonInteractive -EncodedCommand ")
        .ok_or_else(|| "not an encoded command".to_string())?;
    let path = format!(
        "{}:{}",
        standin_dir().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new(pwsh)
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", encoded])
        .env("PATH", path)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("pwsh did not start: {e}"))?;
    Ok(ProcessOutput {
        stdout: out.stdout,
        stderr: out.stderr,
        code: out.status.code().unwrap_or(-1),
        timed_out: false,
    })
}

/// The `IGN-RESULT` JSON of `script`, or the host's failure text.
pub fn run(script: &WinScript) -> Result<serde_json::Value, String> {
    interpret_output(&output(script)?, 600, None)
}

/// The paths a script's parameters hold as native Windows spellings
/// (`tmp\x\data`, from the sftp-style `/tmp/x/data`): rewritten to the local
/// unix path before the script runs, and the long path prefix dropped.
const PATH_KEYS: [&str; 5] = ["a", "b", "c", "d", "h"];

/// `rest` (the base64 of an `-EncodedCommand`) with its parameter blob mapped
/// from native Windows spellings to this machine's unix paths.
fn localize(encoded: &str) -> String {
    use base64::Engine as _;
    let engine = base64::engine::general_purpose::STANDARD;
    let bytes = engine.decode(encoded).expect("encoded command");
    let units: Vec<u16> = bytes
        .chunks(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let text = String::from_utf16(&units).expect("utf16");
    let open = "FromBase64String('";
    let Some(at) = text.find(open) else {
        return encoded.to_string();
    };
    let from = at + open.len();
    let to = from + text[from..].find('\'').expect("blob end");
    let mut params: serde_json::Value =
        serde_json::from_slice(&engine.decode(&text[from..to]).expect("blob")).expect("json");
    if let Some(map) = params.as_object_mut() {
        if map.contains_key("x") {
            map.insert("x".to_string(), serde_json::Value::String(String::new()));
        }
        for key in PATH_KEYS {
            if let Some(serde_json::Value::String(path)) = map.get_mut(key) {
                *path = format!("/{}", path.replace('\\', "/"));
            }
        }
    }
    let blob = engine.encode(serde_json::to_vec(&params).expect("json"));
    let rewritten = format!("{}{blob}{}", &text[..from], &text[to..]);
    let utf16: Vec<u8> = rewritten
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    engine.encode(utf16)
}

/// A process runner for the `local_exec` seam of an `SftpTransport` that
/// believes it talks to a Windows host: the `powershell -EncodedCommand ...`
/// the transport builds runs under the local `pwsh` with the parameters mapped
/// to local paths (see [`localize`]) and the `robocopy` stand-in first on
/// `PATH`; every other command (the sftp client) runs as it is, through the
/// `stat` shim of `posix_exec`.
pub struct PwshRunner;

#[async_trait::async_trait]
impl ignition_core::deploy::transport::process::ProcessRunner for PwshRunner {
    async fn run(
        &self,
        program: &std::ffi::OsStr,
        args: &[std::ffi::OsString],
        stdin: Option<Vec<u8>>,
        timeout: std::time::Duration,
    ) -> ProcessOutput {
        use ignition_core::deploy::transport::process::TokioProcessRunner;
        let prefix = "powershell -NoProfile -NonInteractive -EncodedCommand ";
        let command = args.last().and_then(|a| a.to_str()).unwrap_or_default();
        if program == "sh"
            && let Some(rest) = command.strip_prefix(prefix)
            && let Some(pwsh) = pwsh()
        {
            let path = format!(
                "PATH={}:{}",
                standin_dir().display(),
                std::env::var("PATH").unwrap_or_default()
            );
            let argv: Vec<std::ffi::OsString> = vec![
                path.into(),
                pwsh.into(),
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-EncodedCommand".into(),
                localize(rest).into(),
            ];
            return TokioProcessRunner
                .run(std::ffi::OsStr::new("env"), &argv, None, timeout)
                .await;
        }
        let (program, args) = super::posix_exec::shimmed(program, args);
        TokioProcessRunner
            .run(&program, &args, stdin, timeout)
            .await
    }
}
