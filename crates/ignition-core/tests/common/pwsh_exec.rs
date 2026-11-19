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
//! - the scripts that read through the reviewed C# block (`IgnGit.N`, 27-48)
//!   call Win32 (`CreateFileW` and three more functions of kernel32), which a
//!   unix machine has not. A script whose body starts with the block's loader
//!   (`GIT_NATIVE_LOAD`) is run with a stand-in for `IgnGit.N` defined in the
//!   same `pwsh` process before the exact encoded text: the same type, the same
//!   seven methods and signatures, and managed semantics (the attributes of the
//!   path are read and a `ReparsePoint` is refused with the block's own text,
//!   then the file is opened as a `SafeFileHandle`). The loader's type guard
//!   sees the type and skips its own `Add-Type`, so the staged source is not
//!   read here. **The stand-in tests the scripts' logic. It does not test the
//!   Win32 anchoring**: a held level that cannot be renamed, `CreateFileW`'s
//!   flags, the compile under `csc` and Windows PowerShell 5.1. That is proved
//!   on windows-latest (`winps_git_native` in `deploy_transport.rs`, 27-49) and
//!   on the Windows test VM (27-59). The loader alone, which has no
//!   Win32 call until a method runs, is run without the stand-in by
//!   [`output_without_standin`].
//!
//! A runner without `pwsh` prints `PWSH-SKIPPED <name> (no pwsh)` and the test
//! passes; a runner with it prints `PWSH-RAN <name>`. Never a silent skip.

#![cfg(unix)]

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use ignition_core::deploy::transport::process::ProcessOutput;
use ignition_core::deploy::transport::winps::{
    COMMAND_BUDGET, GIT_NATIVE_CS, GIT_NATIVE_LOAD, GIT_NATIVE_STAGED_DIR, GIT_NATIVE_STAGED_FILE,
    WinScript, encoded_command, interpret_output, merge_git_state,
};

const ROBOCOPY_STANDIN: &str =
    "#!/bin/sh\nmkdir -p \"$2\" && cp -R \"$1\"/. \"$2\"/ && exit 1\nexit 16\n";

/// The stand-in for `IgnGit.N` (see the header): the block's seven public
/// methods with managed semantics. Its failure texts are the block's, with the
/// Win32 error numbers the scripts tell absent from refused by (2: the file is
/// not there, 3: a directory above it is not). `OpenDir` answers a handle that
/// owns nothing: nothing is held on a unix machine, which is the difference
/// the header names. `FileId` is a stand-in too (the handle's number, padded to
/// the block's 48 digits) and `DeleteOnClose` is not implemented: no script of
/// 27-49 calls either, and the plans that do extend this.
const GIT_NATIVE_STANDIN_CS: &str = r#"
using System;using System.IO;using Microsoft.Win32.SafeHandles;
namespace IgnGit{public static class N{
static IOException X(string w,int e){return new IOException(w+" (Win32 error "+e+")");}
static FileAttributes A(string p){
try{return File.GetAttributes(p);}
catch(FileNotFoundException){throw X("cannot open the path",2);}
catch(DirectoryNotFoundException){throw X("cannot open the path",3);}}
static void C(string p,int k){
FileAttributes t=A(p);
if((t&FileAttributes.ReparsePoint)!=0)throw new IOException("the path is a reparse point");
if(((t&FileAttributes.Directory)!=0)!=(k==1))throw new IOException(k==1?"the path is not a directory":"the path is not a regular file");}
public static SafeFileHandle OpenDir(string p){C(p,1);return new SafeFileHandle(IntPtr.Zero,false);}
public static SafeFileHandle OpenFile(string p){C(p,2);return File.OpenHandle(p,FileMode.Open,FileAccess.Read,FileShare.ReadWrite|FileShare.Delete);}
public static SafeFileHandle CreateNew(string p){
try{return File.OpenHandle(p,FileMode.CreateNew,FileAccess.Write,FileShare.None);}
catch(IOException){throw X("cannot open the path",80);}}
public static SafeFileHandle OpenForDelete(string p){C(p,2);return File.OpenHandle(p,FileMode.Open,FileAccess.Read,FileShare.ReadWrite|FileShare.Delete);}
public static string FileId(SafeFileHandle h){return BitConverter.ToString(BitConverter.GetBytes(h.DangerousGetHandle().ToInt64())).Replace("-","").PadRight(48,(char)48);}
public static void DeleteOnClose(SafeFileHandle h){throw new IOException("DeleteOnClose has no stand-in");}
public static void Move(string a,string b,bool replace){
if(!replace&&File.Exists(b))throw X("cannot move the file",183);
File.Move(a,b,replace);}
}}
"#;

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

/// The stand-in for `IgnGit.N` compiled once per test process into an assembly
/// (about a second with `Add-Type` inline, a tenth of that to load), in the
/// directory of the `robocopy` stand-in. Panics if `pwsh` cannot build it: a
/// harness that cannot define the type must not look like a script that failed.
fn standin_assembly() -> &'static PathBuf {
    static DLL: OnceLock<PathBuf> = OnceLock::new();
    DLL.get_or_init(|| {
        let dir = standin_dir();
        let source = dir.join("IgnGitStandIn.cs");
        let dll = dir.join("IgnGitStandIn.dll");
        std::fs::write(&source, GIT_NATIVE_STANDIN_CS).expect("stand-in source");
        let script = format!(
            "Add-Type -TypeDefinition (Get-Content -Raw -LiteralPath '{}') -OutputAssembly '{}'",
            source.display(),
            dll.display()
        );
        let out = Command::new(pwsh().expect("pwsh"))
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .stdin(Stdio::null())
            .output()
            .expect("pwsh did not start");
        assert!(
            out.status.success() && dll.is_file(),
            "the IgnGit.N stand-in did not compile: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        dll
    })
}

/// The `-EncodedCommand` payload `encoded` (base64 of UTF-16LE), with the
/// stand-in loaded in front of the exact text when that text starts the
/// reviewed block's loader. Any other script is returned as it is.
fn with_standin(encoded: &str) -> String {
    use base64::Engine as _;
    let engine = base64::engine::general_purpose::STANDARD;
    let bytes = engine.decode(encoded).expect("encoded command");
    let units: Vec<u16> = bytes
        .chunks(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let text = String::from_utf16(&units).expect("utf16");
    if !text.contains(GIT_NATIVE_LOAD) {
        return encoded.to_string();
    }
    let prefixed = format!("Add-Type -Path '{}';{text}", standin_assembly().display());
    let utf16: Vec<u8> = prefixed.encode_utf16().flat_map(u16::to_le_bytes).collect();
    engine.encode(utf16)
}

/// The full process output of `script` under `pwsh` (the command as built for
/// a host, within the real budget). A script that loads the reviewed block runs
/// with the `IgnGit.N` stand-in (see the header).
pub fn output(script: &WinScript) -> Result<ProcessOutput, String> {
    output_with(script, true)
}

/// [`output`] without the `IgnGit.N` stand-in: the exact encoded command of a
/// host. Only the loader can run this way on a unix machine (it compiles the
/// block, whose Win32 calls are never made).
pub fn output_without_standin(script: &WinScript) -> Result<ProcessOutput, String> {
    output_with(script, false)
}

fn output_with(script: &WinScript, standin: bool) -> Result<ProcessOutput, String> {
    let pwsh = pwsh().ok_or_else(|| "no pwsh".to_string())?;
    let command = encoded_command(script, COMMAND_BUDGET)?;
    let encoded = command
        .as_str()
        .strip_prefix("powershell -NoProfile -NonInteractive -EncodedCommand ")
        .ok_or_else(|| "not an encoded command".to_string())?;
    let encoded = if standin {
        with_standin(encoded)
    } else {
        encoded.to_string()
    };
    let encoded = encoded.as_str();
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

/// The `IGN-RESULT` JSON of `script` run without the `IgnGit.N` stand-in.
pub fn run_without_standin(script: &WinScript) -> Result<serde_json::Value, String> {
    interpret_output(&output_without_standin(script)?, 600, None)
}

/// The one answer of the three scripts of a state read
/// (`WinScript::git_state`), run in order and merged as the transport merges
/// them; the first failure is returned as it is.
pub fn run_state(git: &str, markers: &[&str], home: &str) -> Result<serde_json::Value, String> {
    let mut answers = Vec::new();
    for script in WinScript::git_state(git, markers, home) {
        answers.push(run(&script)?);
    }
    merge_git_state(&answers)
}

/// Write the pinned source where the loader reads it below `home`
/// (`ign-deploy/git-native/IgnGitNative-<pin12>.cs`), as the transport's upload
/// does, and return the file's path.
pub fn stage_native(home: &std::path::Path) -> PathBuf {
    let dir = home.join(GIT_NATIVE_STAGED_DIR);
    std::fs::create_dir_all(&dir).expect("staged dir");
    let file = dir.join(GIT_NATIVE_STAGED_FILE);
    std::fs::write(&file, GIT_NATIVE_CS).expect("staged source");
    file
}

/// The paths a script's parameters hold as native Windows spellings
/// (`tmp\x\data`, from the sftp-style `/tmp/x/data`): rewritten to the local
/// unix path before the script runs, and the long path prefix dropped. `l` and
/// `z` are BUNDLE's list and archive (27-11); in `GIT_OBJECTS` `l` is a number,
/// which is left alone (only strings are mapped).
const PATH_KEYS: [&str; 7] = ["a", "b", "c", "d", "h", "l", "z"];

/// The account home a Windows transport under test believes it has: a drive
/// letter in front, as a real Windows host spells it. A transport refuses to
/// stage a read below any other home, and a unix machine has no `/C:`, so the
/// runner below maps this one spelling to a real directory, in the sftp batch
/// and in the PowerShell parameters alike.
pub const SEAM_HOME: &str = "/C:/Users/ign-seam";

/// `text` with the seam home replaced by the real directory `home`, when there
/// is one.
fn at_home(text: &str, home: Option<&std::path::Path>) -> String {
    match home {
        Some(home) => text.replace(SEAM_HOME, &home.display().to_string()),
        None => text.to_string(),
    }
}

/// `rest` (the base64 of an `-EncodedCommand`) with its parameter blob mapped
/// from native Windows spellings to this machine's unix paths.
fn localize(encoded: &str, home: Option<&std::path::Path>) -> String {
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
            // `h` is the account home in the git scripts and the list's SHA-256
            // in BUNDLE: only a value that holds a separator is a path
            if let Some(serde_json::Value::String(path)) = map.get_mut(key)
                && path.contains(['\\', '/', ':'])
            {
                *path = at_home(&format!("/{}", path.replace('\\', "/")), home);
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

/// [`PwshRunner`] with the account home mapped ([`SEAM_HOME`] is `home`): what
/// a Windows read that stages an archive in the account home needs.
pub struct PwshHomeRunner {
    /// The real directory standing for [`SEAM_HOME`].
    pub home: PathBuf,
}

#[async_trait::async_trait]
impl ignition_core::deploy::transport::process::ProcessRunner for PwshHomeRunner {
    async fn run(
        &self,
        program: &std::ffi::OsStr,
        args: &[std::ffi::OsString],
        stdin: Option<Vec<u8>>,
        timeout: std::time::Duration,
    ) -> ProcessOutput {
        // an sftp batch names the home in its lines
        let stdin = stdin
            .map(|bytes| at_home(&String::from_utf8_lossy(&bytes), Some(&self.home)).into_bytes());
        run_pwsh_or_shim(program, args, stdin, timeout, Some(&self.home)).await
    }
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
        run_pwsh_or_shim(program, args, stdin, timeout, None).await
    }
}

/// Run `program` as the transport asked: a `powershell -EncodedCommand` under
/// the local `pwsh` (parameters mapped to local paths, with the seam home if
/// `home` is given), anything else as it is through the `stat` shim.
async fn run_pwsh_or_shim(
    program: &std::ffi::OsStr,
    args: &[std::ffi::OsString],
    stdin: Option<Vec<u8>>,
    timeout: std::time::Duration,
    home: Option<&std::path::Path>,
) -> ProcessOutput {
    use ignition_core::deploy::transport::process::{ProcessRunner, TokioProcessRunner};
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
            with_standin(&localize(rest, home)).into(),
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
