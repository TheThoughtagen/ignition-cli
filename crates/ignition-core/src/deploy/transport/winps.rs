//! PowerShell on a Windows OpenSSH host (22-13), built only from the C7a
//! captures in `22-LIVE-CAPTURES.md`.
//!
//! sftp cannot do three things the Windows read and write halves need, so a
//! small, fixed set of scripts runs through
//! [`RemoteShell::exec_powershell`](super::remote::RemoteShell::exec_powershell):
//!
//! - [`ENUMERATE`]: an independent, complete walk of a subtree with the
//!   `\\?\` prefix. The sftp-server resolves no path of 260 characters or
//!   more and silently omits such entries from listings (C7a-8), and `get -R`
//!   follows junctions (C7a-5), so an sftp listing is never proof of
//!   completeness on this host. The walk returns every entry with its size and
//!   reparse-point flag, and the read compares it with what sftp returned.
//! - [`ENUMERATE_CHILDREN`]: the same columns for the direct children of a
//!   directory only, never opening a child. The names-only provider listing
//!   uses it, so that a huge generated provider tree is never walked.
//! - [`MOVE_REPLACE`]: `Move-Item -Force`, the documented fallback for
//!   replacing a file when sftp `rename` does not (C7a-6). On the captured host
//!   `rename` over an existing file replaces it (C7a-4), so the Windows commit
//!   uses `rename`; this script is the proven alternative, exercised live.
//! - [`RECORD_REPLACE`]: the atomic replace of the release record (25-20,
//!   G-25-4), one `MoveFileEx(REPLACE_EXISTING | WRITE_THROUGH)` call.
//! - [`GIT_ENTRY`]: the `.git` probe of `deploy projects` (25-08, K9), one
//!   `Get-Item -Force` per level, so a Hidden entry is seen and a denied one is
//!   an error rather than "absent".
//!
//! ## How a command is built
//!
//! [`encoded_command`] assembles `prelude + parameter decode + body` and hands
//! the text to [`RemoteCommand::powershell_encoded`], the one UTF-16LE base64
//! encoder; there is no second encoder and no way to pass a raw command line.
//! The bodies are `const` strings. Parameters travel only as a base64 JSON
//! blob decoded inside the script (`$p`), never by string interpolation, so a
//! path with a quote, a `$` or a backtick is data. The whole remote command
//! is held to [`COMMAND_BUDGET`] characters before anything spawns (C7a-7: it
//! works to 8154, fails at 8162).
//!
//! ## What the output means
//!
//! - The prelude sets `$ErrorActionPreference='Stop'` (without it a failed
//!   step exits 0, C7a-6) and `$ProgressPreference='SilentlyContinue'`
//!   (without it progress CLIXML reaches stderr). Error CLIXML still reaches
//!   stderr on a failure even so, so **stderr is opaque**: success is decided
//!   from the exit code and exactly one [`IGN_RESULT_SENTINEL`] line on
//!   stdout, and stderr is decoded only to explain a failure.
//! - The sentinel line carries compact JSON, escaped to ASCII: PowerShell
//!   writes stdout in the OEM code page 437 (C7a-6), so non-ASCII text is
//!   `\uXXXX` before it leaves the host.
//! - An error record on stderr with exit 0 is a failure too: with `Stop` no
//!   error should survive, so one that does means a step did not do what the
//!   script says.

use serde_json::Value;

use super::process::ProcessOutput;
use super::remote::{RemoteCommand, scrub_stderr};

/// The marker that starts the one stdout line a script's result travels on.
pub const IGN_RESULT_SENTINEL: &str = "IGN-RESULT:";

/// The longest remote command, in characters, that is sent (C7a-7: 8154 works,
/// 8162 fails with "The command line is too long."; 8000 leaves a margin).
pub const COMMAND_BUDGET: usize = 8000;

/// `\\?\`: the extended-length path prefix the enumeration walks with.
pub const LONG_PATH_PREFIX: &str = "\\\\?\\";

/// Entries whose absolute path has this many characters or more are refused:
/// the sftp-server cannot resolve a path of 260 characters (C7a-8), and a
/// margin keeps a path that grows by a rename from crossing it unseen.
pub const LONG_PATH_LIMIT: usize = 240;

/// Run before every script. `Stop` makes a failed step exit non-zero; the
/// progress preference silences progress CLIXML; `E` writes the result line
/// with every non-ASCII character as a `\uXXXX` escape.
const PRELUDE: &str = "$ProgressPreference='SilentlyContinue';$ErrorActionPreference='Stop';function E($o){$j=ConvertTo-Json -InputObject $o -Compress -Depth 6;$j=[regex]::Replace($j,'[^\\x00-\\x7e]',{param($m)'\\u{0:x4}' -f [int][char]$m.Value});[Console]::Out.WriteLine('IGN-RESULT:'+$j)};";

/// `Move-Item -Force`: replace `dst` with `src`. A missing source or an
/// unwritable target is a terminating error (exit 1).
///
/// This is the apply-era fallback and is no longer the release-record replace
/// path: on Windows PowerShell 5.1 `Move-Item -Force` over an existing file is
/// delete-then-rename, so a concurrent reader sees the target missing for a
/// moment (G-25-4). The record uses [`RECORD_REPLACE`]. It is kept for the
/// generic exec test and as the negative control of the live race gate.
pub const MOVE_REPLACE: &str = "Move-Item -LiteralPath $p.src -Destination $p.dst -Force;E @{ok=1}";

/// The atomic release-record replace (25-20, G-25-4): `MoveFileEx(src, dst,
/// MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)` declared through
/// `Add-Type` P/Invoke. Win32 performs the rename-with-replace as one
/// operation, so a concurrent reader of `dst` sees the old file or the new one
/// on NTFS, never a missing or a half-written one. That is the guarantee of
/// the atomic rename of NTFS on Windows (and of `rename(2)` on Linux, which the
/// POSIX arm uses): it is atomic for readers and it is not a durable write
/// (the temporary file is not flushed first). `Move-Item -Force` was retired
/// for the record because on PowerShell 5.1 it deletes the target before the
/// rename.
///
/// It fails closed: unless the session is `FullLanguage` the script throws
/// naming the language mode, and a failing `Add-Type` throws naming the
/// innermost exception, so the writer returns `transport_failed` and never
/// falls back to a non-atomic replace. The type is compiled only when
/// `IgnRecord.Native` is not loaded yet (production runs one fresh process per
/// replace, so this matters only to the in-process race harness). A sharing or
/// access violation (Win32 error 32 or 5, a reader holding the target without
/// delete-share) is retried `$p.retries` times with one-second pauses, then the
/// script throws naming the last error and its system message. Error 5 also
/// covers a permanent ACL denial, which therefore surfaces after about
/// `retries` seconds. A replace still blocked after the retries throws and the
/// previous record stays. `$p.src`, `$p.dst` and `$p.retries` are data in the
/// base64 parameter blob, never text of the script.
///
/// Every failure says why (G-25-5, WR-01). The variable part of a thrown text
/// is one line of printable ASCII, at most 200 characters plus `...` when cut,
/// with every drive-letter or UNC path replaced by `<path>`. It is only
/// concatenated into the thrown string, never evaluated. Each thrown text
/// stays within 300 characters: PowerShell 5.1 wraps error lines at 119
/// columns and a thrown string ends its error record with
/// `FullyQualifiedErrorId : <thrown text>`, so at most 3 such chunks and the
/// CategoryInfo line must fit the 5-line stderr tail kept by `scrub_stderr`.
/// The helpers are named `IgnLine` and `IgnCause` and never with one letter,
/// because PowerShell resolves an alias before a function (`r` is the built-in
/// alias of `Invoke-History`). `IgnCause` has its own try and catch, so a
/// failure inside it can never stand in for the original failure.
///
/// `Add-Type` is kept (the G-25-5 evaluation): a `DefinePInvokeMethod`
/// declaration needs a dynamic assembly, a module, a type and a custom
/// attribute that sets `SetLastError`, none of it can be exercised offline,
/// and in-memory P/Invoke through Reflection.Emit is the technique of
/// offensive tooling that endpoint protection watches. The cost is one
/// `csc.exe` compile in every fresh process, inside a whole fresh-process
/// replace of 2.70 s on the capture host (25-20 preflight). The prerequisites
/// are a FullLanguage session, `csc.exe` and a writable `%TEMP%` for the
/// deploy account.
pub const RECORD_REPLACE: &str = concat!(
    r#"if($ExecutionContext.SessionState.LanguageMode -ne 'FullLanguage'){throw ('the record replace needs a FullLanguage PowerShell session, not '+$ExecutionContext.SessionState.LanguageMode)};"#,
    r#"function IgnLine($t){$s=[regex]::Replace([string]$t,'[\x00-\x1f]+',' ');$s=[regex]::Replace($s,'(?:[A-Za-z]:[\\/]|\\\\)[^''"]*','<path>');$s=[regex]::Replace($s,'[^\x20-\x7e]','?').Trim();if($s.Length -gt 200){$s=$s.Substring(0,200)+'...'};$s};"#,
    r#"function IgnCause($x){try{while($x.InnerException){$x=$x.InnerException};$m=[string]$x.Message;$k=$x.GetType().FullName+' '+('0x{0:X8}' -f $x.HResult);if($m){$k+=': '+$m};IgnLine $k}catch{'the cause could not be read'}};"#,
    "if(-not ('IgnRecord.Native' -as [type])){try{Add-Type -Namespace IgnRecord -Name Native -MemberDefinition '[DllImport(\"kernel32.dll\",SetLastError=true,CharSet=CharSet.Unicode)]public static extern bool MoveFileEx(string a,string b,uint c);'}catch{throw ('the record replace cannot declare MoveFileEx: '+(IgnCause $_.Exception))}};",
    "$f=[uint32](0x1 -bor 0x8);$n=0;",
    "while(-not [IgnRecord.Native]::MoveFileEx($p.src,$p.dst,$f)){$c=[Runtime.InteropServices.Marshal]::GetLastWin32Error();",
    r#"if(($c -ne 32 -and $c -ne 5) -or $n -ge [int]$p.retries){throw ('the record replace failed with Win32 error '+$c+': '+(IgnLine $(try{(New-Object ComponentModel.Win32Exception ([int]$c)).Message}catch{''})))};"#,
    "$n++;Start-Sleep -Seconds 1};E @{ok=1}"
);

/// Walk each `$p.r[i]` (with the prefix `$p.x` in front) and report every
/// entry as `[relative path, is_dir, size, is_reparse_point, absolute path
/// length]`. A reparse point is reported and not descended. A missing root is
/// `x=0`. An unreadable directory is a terminating error.
///
/// The relative path is built from entry names during the walk (`\`-joined),
/// never by cutting the root off `FullName`: .NET rewrites a root it
/// normalises (an 8.3 component such as `RUNNER~1` expanded, `.`, a doubled or
/// trailing separator dropped), so the caller's spelling and `FullName` need
/// not share a prefix length (G-22-1).
pub const ENUMERATE: &str = "$o=@();for($i=0;$i -lt $p.r.Count;$i++){$b=$p.x+$p.r[$i];$e=@();$x=[IO.Directory]::Exists($b);if($x){$s=New-Object Collections.Stack;$s.Push(@([IO.DirectoryInfo]$b,''));while($s.Count){$q=$s.Pop();foreach($f in $q[0].EnumerateFileSystemInfos()){$a=[int]$f.Attributes;$d=[int](($a -band 16) -ne 0);$r=[int](($a -band 1024) -ne 0);$z=0;if(!$d){$z=$f.Length};$m=$q[1]+$f.Name;$e+=,@($m,$d,$z,$r,$f.FullName.Length);if($d -and !$r){$s.Push(@($f,($m+'\\')))}}}};$o+=@{i=$i;x=[int]$x;e=$e}};E @{ok=1;res=$o}";

/// List the direct children of each `$p.r[i]` (with the prefix `$p.x` in
/// front), with the columns of [`ENUMERATE`]: `[name, is_dir, size,
/// is_reparse_point, absolute path length]`. No child is opened, so nothing
/// below a child directory is read (C6, PR #31 review: the names-only
/// `tag-provider` cross-check must not walk a provider's subtree). A missing
/// root is `x=0`. An unreadable root is a terminating error.
pub const ENUMERATE_CHILDREN: &str = "$o=@();for($i=0;$i -lt $p.r.Count;$i++){$b=$p.x+$p.r[$i];$e=@();$x=[IO.Directory]::Exists($b);if($x){foreach($f in ([IO.DirectoryInfo]$b).EnumerateFileSystemInfos()){$a=[int]$f.Attributes;$d=[int](($a -band 16) -ne 0);$r=[int](($a -band 1024) -ne 0);$z=0;if(!$d){$z=$f.Length};$e+=,@($f.Name,$d,$z,$r,$f.FullName.Length)}};$o+=@{i=$i;x=[int]$x;e=$e}};E @{ok=1;res=$o}";

/// The `.git` probe of `deploy projects` (Phase 25, 25-08, K9): what is at
/// `$p.c` (`<data>\projects\<name>\.git`), asked only when `$p.a`
/// (`<data>\projects`) and `$p.b` (the project directory) are real
/// directories.
///
/// Each level is one `Get-Item -LiteralPath ... -Force` and nothing else. The
/// 25-03 captures (`## C-H before grant`, `## Host grant and prod git.yaml
/// (25-03)`) showed why: `-Force` sees a Hidden entry, the .NET `Exists`
/// methods and `Test-Path` answer false (or, run to run, throw) for a
/// directory the account cannot traverse, and `Get-Item` throws
/// `UnauthorizedAccessException` there. Only an `ObjectNotFound` error record
/// is `none`; every other error is rethrown, which under the prelude's `Stop`
/// is a non-zero exit and so `transport_failed`, never "absent". A reparse
/// point (junction or symlink) is reported with `r=1` and never opened or
/// followed; a Hidden `.git` is reported with `h=1` and counts like any other.
///
/// Result: `projects` and `project` are `none | dir | file | symlink | skip`,
/// `kind` is `none | dir | file | skip` for the `.git` entry, `hidden` and
/// `reparse` are 0 or 1 for it.
pub const GIT_ENTRY: &str = concat!(
    "function T($x){try{$i=Get-Item -LiteralPath $x -Force -ErrorAction Stop}",
    "catch{if($_.CategoryInfo.Category -eq 'ObjectNotFound'){return @{k='none';h=0;r=0}};throw};",
    "$a=[int]$i.Attributes;",
    "return @{k=$(if($a -band 16){'dir'}else{'file'});h=[int](($a -band 2) -ne 0);r=[int](($a -band 1024) -ne 0)}};",
    "function W($t){if($t.r){'symlink'}else{$t.k}};",
    "$s=@{k='skip';h=0;r=0};$a=T $p.a;$b=$s;$c=$s;",
    "if($a.k -eq 'dir' -and !$a.r){$b=T $p.b;if($b.k -eq 'dir' -and !$b.r){$c=T $p.c}};",
    "E @{ok=1;projects=(W $a);project=(W $b);kind=$c.k;hidden=$c.h;reparse=$c.r}"
);

/// Why `name` cannot be ONE component of a path on a Windows host, or `None`.
///
/// This is the Windows half of the path gate. It refuses everything the POSIX
/// gate refuses (empty, `.`, `..`, a separator, NUL and other control
/// characters) and the ways Windows lets a name leave its directory or mean
/// something else: a `:` (a drive letter injected into a component, or an
/// alternate data stream `name:stream`), the characters NTFS forbids
/// (`< > " | ? *`), a trailing dot or space (Windows strips them, so two names
/// would be one entry) and the reserved device names (`CON`, `NUL`, `COM1`,
/// ... with or without an extension). `\\?\` and UNC prefixes cannot pass as a
/// component because they hold separators and a colon.
pub fn windows_component_problem(name: &str) -> Option<String> {
    let quoted = |why: &str| Some(format!("the name {name:?} {why}"));
    if name.is_empty() {
        return quoted("is empty");
    }
    if name == "." || name == ".." {
        return quoted("is `.` or `..`");
    }
    if let Some(bad) = name.chars().find(|c| {
        c.is_control() || matches!(c, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*')
    }) {
        return Some(format!(
            "the name {name:?} holds {bad:?}, which cannot be part of a Windows path component (separators, drive letters, alternate data streams and reserved characters)"
        ));
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return quoted("ends in a dot or a space, which Windows strips");
    }
    let stem = name.split('.').next().unwrap_or(name).trim_end();
    let upper = stem.to_ascii_uppercase();
    let device = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            upper
                .strip_prefix(prefix)
                .is_some_and(|n| matches!(n.as_bytes(), [b'1'..=b'9']))
        });
    if device {
        return quoted("is a reserved Windows device name");
    }
    None
}

/// Why `path` is not an absolute path in the sftp spelling of a Windows host
/// (`/C:/dir/...`), or `None`: a leading `/`, a drive letter component
/// (`C:`), at least one more component, and every component passing
/// [`windows_component_problem`]. Refuses `C:/...` (home-relative on this
/// server), UNC (`//host/...`), `\\?\` prefixes, `..`, empty components and a
/// second drive letter.
pub fn windows_absolute_path_problem(path: &str) -> Option<String> {
    let Some(rest) = path.strip_prefix('/') else {
        return Some(format!("{path:?} does not start with `/`"));
    };
    let mut parts = rest.split('/');
    let drive = parts.next().unwrap_or_default();
    let drive_ok = matches!(drive.as_bytes(), [letter, b':'] if letter.is_ascii_alphabetic());
    if !drive_ok {
        return Some(format!(
            "{path:?} does not start with a drive letter component such as `/C:/`"
        ));
    }
    let mut below = 0usize;
    for part in parts {
        if let Some(why) = windows_component_problem(part) {
            return Some(format!("{path:?}: {why}"));
        }
        below += 1;
    }
    if below == 0 {
        return Some(format!("{path:?} is a drive root"));
    }
    None
}

/// One fixed script and its parameters.
#[derive(Debug, Clone)]
pub struct WinScript {
    /// A short name for error messages.
    pub name: &'static str,
    /// The script body, a `const`. It reads its parameters from `$p` and ends
    /// by calling `E` with the result.
    pub body: &'static str,
    /// The parameters: a JSON object (or `Null` for none), decoded in-script.
    pub params: Value,
}

impl WinScript {
    /// The script that replaces `dst` with `src` ([`MOVE_REPLACE`]).
    pub fn move_replace(src: &str, dst: &str) -> WinScript {
        WinScript {
            name: "move_replace",
            body: MOVE_REPLACE,
            params: serde_json::json!({ "src": src, "dst": dst }),
        }
    }

    /// The script that atomically replaces the release record `dst` with
    /// `src` ([`RECORD_REPLACE`]). `retries` is how often a sharing violation
    /// is retried; like both paths it travels only in the JSON parameters.
    pub fn record_replace(src: &str, dst: &str, retries: u32) -> WinScript {
        WinScript {
            name: "record_replace",
            body: RECORD_REPLACE,
            params: serde_json::json!({ "src": src, "dst": dst, "retries": retries }),
        }
    }

    /// The script that probes the `.git` entry of one project ([`GIT_ENTRY`]).
    /// The three native paths are `<data>\projects`, the project directory and
    /// its `.git`; they travel only as parameters.
    pub fn git_entry(projects_dir: &str, project_dir: &str, git: &str) -> WinScript {
        WinScript {
            name: "git_entry",
            body: GIT_ENTRY,
            params: serde_json::json!({ "a": projects_dir, "b": project_dir, "c": git }),
        }
    }

    /// The script that walks `roots` with `prefix` in front of each
    /// ([`ENUMERATE`]). `prefix` is [`LONG_PATH_PREFIX`] on a Windows host.
    pub fn enumerate(prefix: &str, roots: &[String]) -> WinScript {
        WinScript {
            name: "enumerate",
            body: ENUMERATE,
            params: serde_json::json!({ "x": prefix, "r": roots }),
        }
    }

    /// The script that lists the direct children of `roots` with `prefix` in
    /// front of each ([`ENUMERATE_CHILDREN`]); its result parses with
    /// [`parse_enumeration`].
    pub fn enumerate_children(prefix: &str, roots: &[String]) -> WinScript {
        WinScript {
            name: "enumerate_children",
            body: ENUMERATE_CHILDREN,
            params: serde_json::json!({ "x": prefix, "r": roots }),
        }
    }
}

/// The remote command for `script`, or why it cannot be sent: the parameters
/// must be a JSON object (or `Null`), and the whole command must fit `budget`
/// characters ([`COMMAND_BUDGET`] in production). Built by
/// [`RemoteCommand::powershell_encoded`]; this function only composes the
/// script text.
pub fn encoded_command(script: &WinScript, budget: usize) -> Result<RemoteCommand, String> {
    use base64::Engine as _;
    let mut text = String::from(PRELUDE);
    match &script.params {
        Value::Null => {}
        Value::Object(map) if map.is_empty() => {}
        Value::Object(_) => {
            let json = serde_json::to_vec(&script.params).map_err(|e| {
                format!("the parameters of {} cannot be encoded ({e})", script.name)
            })?;
            let blob = base64::engine::general_purpose::STANDARD.encode(json);
            text.push_str("$p=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('");
            text.push_str(&blob);
            text.push_str("'))|ConvertFrom-Json;");
        }
        _ => {
            return Err(format!(
                "the parameters of {} must be a JSON object",
                script.name
            ));
        }
    }
    text.push_str(script.body);
    let command = RemoteCommand::powershell_encoded(&text);
    let length = command.as_str().chars().count();
    if length > budget {
        return Err(format!(
            "the remote command for {} is {length} characters, over the {budget} character budget",
            script.name
        ));
    }
    Ok(command)
}

/// The JSON of the single [`IGN_RESULT_SENTINEL`] line in `stdout`. Lines
/// without the sentinel are ignored; no such line, two or more, or text after
/// the sentinel that is not JSON is an error.
pub fn parse_ign_result(stdout: &[u8]) -> Result<Value, String> {
    let mut found: Option<&[u8]> = None;
    for line in stdout.split(|b| *b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if let Some(rest) = line.strip_prefix(IGN_RESULT_SENTINEL.as_bytes()) {
            if found.is_some() {
                return Err(format!(
                    "the output holds more than one {IGN_RESULT_SENTINEL} line"
                ));
            }
            found = Some(rest);
        }
    }
    let Some(json) = found else {
        return Err(format!("the output holds no {IGN_RESULT_SENTINEL} line"));
    };
    serde_json::from_slice(json)
        .map_err(|_| format!("the {IGN_RESULT_SENTINEL} line is not valid JSON"))
}

/// The messages of the `<S S="Error">` strings in a CLIXML stderr, decoded
/// for display only. Empty when `stderr` is not CLIXML.
pub fn clixml_errors(stderr: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(stderr);
    if !text.contains("CLIXML") {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut rest: &str = &text;
    while let Some(at) = rest.find("<S S=\"Error\">") {
        rest = &rest[at + "<S S=\"Error\">".len()..];
        let Some(end) = rest.find("</S>") else { break };
        let raw = &rest[..end];
        rest = &rest[end..];
        let decoded = raw
            .replace("_x000D__x000A_", "\n")
            .replace("_x000A_", "\n")
            .replace("_x000D_", "")
            .replace("_x0009_", "\t")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&");
        let decoded = decoded.trim();
        if !decoded.is_empty() {
            out.push(decoded.to_string());
        }
    }
    out
}

/// The result of one [`WinScript`] run, or the detail of the
/// `transport_failed` it is. Success needs exit 0 within the timeout, exactly
/// one sentinel line, and no error record on stderr; stderr is otherwise
/// opaque (see the module doc).
pub fn interpret_output(
    out: &ProcessOutput,
    timeout_s: u64,
    temp_dir: Option<&std::path::Path>,
) -> Result<Value, String> {
    if out.timed_out {
        return Err(format!("timed out after {timeout_s}s and was killed"));
    }
    let explain = || -> String {
        let decoded = clixml_errors(&out.stderr);
        if decoded.is_empty() {
            scrub_stderr(&out.stderr, temp_dir)
        } else {
            scrub_stderr(decoded.join("\n").as_bytes(), temp_dir)
        }
    };
    if out.code != 0 {
        let stderr_text = String::from_utf8_lossy(&out.stderr);
        if stderr_text.contains("The command line is too long") {
            return Err(format!(
                "exit code {}: the host refused the remote command as too long (budget {COMMAND_BUDGET} characters)",
                out.code
            ));
        }
        let tail = explain();
        return Err(if tail.is_empty() {
            format!("exit code {}", out.code)
        } else {
            format!("exit code {}: {tail}", out.code)
        });
    }
    let errors = clixml_errors(&out.stderr);
    if !errors.is_empty() {
        return Err(format!(
            "the script exited 0 but wrote an error record: {}",
            scrub_stderr(errors.join("\n").as_bytes(), temp_dir)
        ));
    }
    parse_ign_result(&out.stdout)
}

/// One entry of an [`ENUMERATE`] result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumEntry {
    /// Path below the walked root, `/`-separated.
    pub rel: String,
    /// A directory.
    pub dir: bool,
    /// File size (0 for a directory).
    pub size: u64,
    /// A reparse point (junction or symlink).
    pub reparse: bool,
    /// Length of the entry's absolute path, prefix included.
    pub full_len: usize,
}

/// The entries per root of an [`ENUMERATE`] result: `None` for a root that
/// does not exist. `expected_roots` roots must be reported, in order.
pub fn parse_enumeration(
    value: &Value,
    expected_roots: usize,
) -> Result<Vec<Option<Vec<EnumEntry>>>, String> {
    let bad = |why: &str| format!("the enumeration result is malformed ({why})");
    if value.get("ok").and_then(Value::as_i64) != Some(1) {
        return Err(bad("ok is not 1"));
    }
    let res = value
        .get("res")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("res is not an array"))?;
    if res.len() != expected_roots {
        return Err(bad("the number of roots differs"));
    }
    let mut out = Vec::with_capacity(res.len());
    for (index, root) in res.iter().enumerate() {
        if root.get("i").and_then(Value::as_u64) != Some(index as u64) {
            return Err(bad("roots are out of order"));
        }
        if root.get("x").and_then(Value::as_i64) == Some(0) {
            out.push(None);
            continue;
        }
        let entries = root
            .get("e")
            .and_then(Value::as_array)
            .ok_or_else(|| bad("e is not an array"))?;
        let mut list = Vec::with_capacity(entries.len());
        for entry in entries {
            let cols = entry
                .as_array()
                .ok_or_else(|| bad("an entry is not an array"))?;
            let (Some(rel), Some(dir), Some(size), Some(reparse), Some(full_len)) = (
                cols.first().and_then(Value::as_str),
                cols.get(1).and_then(Value::as_i64),
                cols.get(2).and_then(Value::as_u64),
                cols.get(3).and_then(Value::as_i64),
                cols.get(4).and_then(Value::as_u64),
            ) else {
                return Err(bad("an entry has the wrong columns"));
            };
            // every path the host reports passes the same gate as any other
            // path that can reach a batch line
            let rel = rel.replace('\\', "/");
            for component in rel.split('/') {
                if let Some(why) = windows_component_problem(component) {
                    return Err(bad(&format!("an entry path is refused: {why}")));
                }
            }
            list.push(EnumEntry {
                rel,
                dir: dir != 0,
                size,
                reparse: reparse != 0,
                full_len: full_len as usize,
            });
        }
        out.push(Some(list));
    }
    Ok(out)
}

/// Split `roots` into the fewest [`ENUMERATE`] commands that each fit
/// [`COMMAND_BUDGET`]: the index ranges of `roots` and the script for each. A
/// single root that does not fit alone is an error.
pub fn enumerate_chunks(
    prefix: &str,
    roots: &[String],
) -> Result<Vec<(std::ops::Range<usize>, WinScript)>, String> {
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < roots.len() {
        let mut end = start + 1;
        if encoded_command(
            &WinScript::enumerate(prefix, &roots[start..end]),
            COMMAND_BUDGET,
        )
        .is_err()
        {
            return Err(format!(
                "the path {:?} is too long to enumerate within the remote command budget",
                roots[start]
            ));
        }
        while end < roots.len()
            && encoded_command(
                &WinScript::enumerate(prefix, &roots[start..=end]),
                COMMAND_BUDGET,
            )
            .is_ok()
        {
            end += 1;
        }
        chunks.push((start..end, WinScript::enumerate(prefix, &roots[start..end])));
        start = end;
    }
    Ok(chunks)
}

/// The three native absolute roots the uploaded script works in (Phase 23,
/// 23-08): the run's staging directory (it holds `commit.ps1` and
/// `work.tsv`), the mode collection and the `ign-deploy/<gateway>/<mode>`
/// directory. Native form (`C:\Users\...`), not the sftp spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StubRoots {
    /// The directory that holds the script, the work list and the staged files.
    pub staging: String,
    /// `<data dir>\config\resources\<mode>`.
    pub collection: String,
    /// `<home>\ign-deploy\<gateway>\<mode>`.
    pub deploy: String,
}

/// The fixed body of the stub that runs the uploaded script (D-01, C-script-run).
///
/// It reads `commit.ps1` and `work.tsv` from the staging directory ONCE each,
/// hashes the bytes it read with SHA-256 and compares them with the values the
/// runner computed (`$p.ss`, `$p.ws`). On a mismatch it writes one fixed
/// sentence to stderr and exits 97
/// ([`STUB_MISMATCH_EXIT`](super::remote::STUB_MISMATCH_EXIT)) before it runs
/// anything. Otherwise it runs
/// the script text it hashed through `[ScriptBlock]::Create` (the execution
/// policy does not govern a script block, so the same stub works under
/// `Restricted` and `AllSigned`), passing the operation, the three roots and
/// the text of the work list it hashed, so that the list the script acts on is
/// the list that was checked. No path, hash or reason is part of the body:
/// they travel in the base64 JSON parameters.
pub const STUB: &str = concat!(
    "$q='\\\\?\\';",
    "$b=[IO.File]::ReadAllBytes($q+$p.t+'\\commit.ps1');",
    "$w=[IO.File]::ReadAllBytes($q+$p.t+'\\work.tsv');",
    "$a=[Security.Cryptography.SHA256]::Create();",
    "$x=[BitConverter]::ToString($a.ComputeHash($b)).Replace('-','').ToLower();",
    "$y=[BitConverter]::ToString($a.ComputeHash($w)).Replace('-','').ToLower();",
    "if($x -cne $p.ss -or $y -cne $p.ws){",
    "[Console]::Error.WriteLine('ign: the staged script or work list does not match its hash');exit 97};",
    "& ([ScriptBlock]::Create([Text.Encoding]::UTF8.GetString($b))) ",
    "$p.o $p.t $p.c $p.d ([Text.Encoding]::UTF8.GetString($w))"
);

/// The [`WinScript`] that verifies the uploaded `commit.ps1` and `work.tsv`
/// against `script_sha256` and `work_sha256` and then runs the script's `op`
/// over `roots` ([`STUB`]). The staging directory of `roots` must hold both
/// files. Paths travel only in the parameters; the command is held to
/// [`COMMAND_BUDGET`] by `exec_powershell`.
pub fn script_stub(
    script_sha256: &str,
    work_sha256: &str,
    op: &str,
    roots: &StubRoots,
) -> WinScript {
    WinScript {
        name: "script_stub",
        body: STUB,
        params: serde_json::json!({
            "t": roots.staging,
            "c": roots.collection,
            "d": roots.deploy,
            "o": op,
            "ss": script_sha256,
            "ws": work_sha256,
        }),
    }
}

/// Cuts `line` every 119 characters with no indent: how the capture host's
/// PowerShell 5.1 wraps an error line (22-LIVE-CAPTURES.md line 7223, chunks
/// of 119, 119, 119 and 1 character).
#[cfg(test)]
fn ps51_wrap(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    chars
        .chunks(119)
        .map(|piece| piece.iter().collect())
        .collect()
}

/// A `+ ...` line of an error record: the text is split into 114-character
/// pieces, each piece gets 4 spaces in front and the pieces are joined with
/// nothing between. Cut by [`ps51_wrap`], a 123-character CategoryInfo text
/// gives a 119-character chunk ending in a space and then a chunk of 3 spaces
/// and the rest (capture line 7223).
#[cfg(test)]
fn ps51_info_line(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    chars
        .chunks(114)
        .map(|piece| format!("    {}", piece.iter().collect::<String>()))
        .collect()
}

/// The CategoryInfo target: the whole text when it has 40 characters or
/// fewer, otherwise its first 15 characters, `...` and its last 15 (the 15/15
/// cut is on capture line 7223; the 40-character threshold is in no capture).
#[cfg(test)]
fn ps51_ellipsize(target: &str) -> String {
    let chars: Vec<char> = target.chars().collect();
    if chars.len() <= 40 {
        return target.to_string();
    }
    let head: String = chars[..15].iter().collect();
    let tail: String = chars[chars.len() - 15..].iter().collect();
    format!("{head}...{tail}")
}

/// The CLIXML stderr of Windows PowerShell 5.1 for `throw '<thrown>'`, in the
/// shape captured on the capture host: `#< CLIXML`, CRLF, the `<Objs>` frame
/// and one `<S S="Error">` chunk per output line, each ending
/// `_x000D__x000A_`. Checked byte for byte against the records of
/// 22-LIVE-CAPTURES.md lines 4751-4752 and 4767-4768 (a thrown string) and
/// line 7223 (the 119-column wrap, the wrapped `+` line and the target
/// ellipsis) by `ps51_error_clixml_matches_the_captured_records`.
///
/// One part stays a model: a FullyQualifiedErrorId line longer than one
/// 114-character piece has no capture. The live Add-Type cause test proves it,
/// because its thrown text spans three pieces on a real host.
#[cfg(test)]
pub(crate) fn ps51_error_clixml(thrown: &str) -> Vec<u8> {
    let mut lines = ps51_wrap(thrown);
    lines.push("At line:1 char:1200".to_string());
    lines.push("+ ... $n -ge [int]$p.retries){throw ('the record replace ...".to_string());
    lines.push(format!("+ {}{}", " ".repeat(30), "~".repeat(12)));
    lines.extend(ps51_wrap(&ps51_info_line(&format!(
        "+ CategoryInfo          : OperationStopped: ({}:String) [], RuntimeException",
        ps51_ellipsize(thrown)
    ))));
    lines.extend(ps51_wrap(&ps51_info_line(&format!(
        "+ FullyQualifiedErrorId : {thrown}"
    ))));
    lines.push(" ".to_string());
    let mut out = String::from(
        "#< CLIXML\r\n<Objs Version=\"1.1.0.1\" xmlns=\"http://schemas.microsoft.com/powershell/2004/04\">",
    );
    for line in &lines {
        let escaped = line
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        out.push_str(&format!("<S S=\"Error\">{escaped}_x000D__x000A_</S>"));
    }
    out.push_str("</Objs>");
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    fn decode_command(command: &RemoteCommand) -> String {
        let text = command.as_str();
        let b64 = text
            .strip_prefix("powershell -NoProfile -NonInteractive -EncodedCommand ")
            .expect("the fixed powershell prefix");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .expect("base64");
        assert_eq!(bytes.len() % 2, 0, "UTF-16LE has an even length");
        let units: Vec<u16> = bytes
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16(&units).expect("utf-16")
    }

    fn script(params: Value) -> WinScript {
        WinScript {
            name: "t",
            body: "E @{ok=1}",
            params,
        }
    }

    #[test]
    fn encoded_command_is_utf16le_base64() {
        let command = encoded_command(&script(Value::Null), COMMAND_BUDGET).expect("command");
        let text = decode_command(&command);
        assert_eq!(text, format!("{PRELUDE}E @{{ok=1}}"));
        assert!(
            text.starts_with(
                "$ProgressPreference='SilentlyContinue';$ErrorActionPreference='Stop';"
            )
        );
        // only base64 characters reach the shell after the fixed prefix
        let b64 = command
            .as_str()
            .strip_prefix("powershell -NoProfile -NonInteractive -EncodedCommand ")
            .expect("prefix");
        assert!(
            b64.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
        );
    }

    #[test]
    fn parameters_travel_as_a_base64_json_blob_never_interpolated() {
        let hostile = "C:\\a b\\'; Remove-Item -Recurse C:\\ ; '$(whoami)`\"";
        let command = encoded_command(
            &script(serde_json::json!({ "path": hostile })),
            COMMAND_BUDGET,
        )
        .expect("command");
        let text = decode_command(&command);
        assert!(
            !text.contains("Remove-Item"),
            "the hostile text must not appear in the script: {text}"
        );
        assert!(!text.contains("whoami"));
        let blob = text
            .split("FromBase64String('")
            .nth(1)
            .and_then(|rest| rest.split('\'').next())
            .expect("blob");
        let json = base64::engine::general_purpose::STANDARD
            .decode(blob)
            .expect("blob base64");
        let value: Value = serde_json::from_slice(&json).expect("json");
        assert_eq!(value["path"], hostile);
        // parameters must be an object
        for bad in [
            serde_json::json!([1]),
            serde_json::json!("x"),
            serde_json::json!(3),
        ] {
            assert!(encoded_command(&script(bad), COMMAND_BUDGET).is_err());
        }
    }

    #[test]
    fn encoded_command_budget() {
        let exact = encoded_command(&script(Value::Null), COMMAND_BUDGET).expect("fits");
        let length = exact.as_str().chars().count();
        assert!(encoded_command(&script(Value::Null), length).is_ok());
        let err = encoded_command(&script(Value::Null), length - 1).expect_err("one over");
        assert!(err.contains("over the") && err.contains("budget"), "{err}");

        // a parameter blob too big for the budget is refused before any spawn
        let big = "x".repeat(COMMAND_BUDGET);
        assert!(encoded_command(&script(serde_json::json!({ "p": big })), COMMAND_BUDGET).is_err());
        // the shipped scripts leave room for their parameters
        let enumerate = encoded_command(
            &WinScript::enumerate(
                LONG_PATH_PREFIX,
                &[
                    "C:\\Program Files\\Inductive Automation\\Ignition\\data\\config\\resources\\production\\ignition\\tag-definition\\provider".to_string(),
                ],
            ),
            COMMAND_BUDGET,
        )
        .expect("one realistic root fits");
        assert!(enumerate.as_str().len() < COMMAND_BUDGET);
    }

    #[test]
    fn enumerate_chunks_split_by_budget_and_cover_every_root() {
        let root = |i: usize| {
            format!(
                "C:\\Program Files\\Inductive Automation\\Ignition\\data\\config\\resources\\production\\ignition\\tag-definition\\provider-{i:04}"
            )
        };
        let roots: Vec<String> = (0..60).map(root).collect();
        let chunks = enumerate_chunks(LONG_PATH_PREFIX, &roots).expect("chunks");
        assert!(chunks.len() >= 2, "60 long roots cannot share one command");
        let mut next = 0;
        for (range, script) in &chunks {
            assert_eq!(range.start, next, "contiguous");
            next = range.end;
            assert!(
                encoded_command(script, COMMAND_BUDGET).is_ok(),
                "each chunk fits the budget"
            );
        }
        assert_eq!(next, roots.len(), "every root is covered");
        let huge = vec!["C:\\".to_string() + &"d".repeat(COMMAND_BUDGET)];
        assert!(enumerate_chunks(LONG_PATH_PREFIX, &huge).is_err());
        assert!(
            enumerate_chunks(LONG_PATH_PREFIX, &[])
                .expect("none")
                .is_empty()
        );
    }

    /// T-25-40, T-25-46, T-25-42: the record replace is fixed one-line ASCII
    /// text that asserts FullLanguage, declares the `MoveFileEx` P/Invoke once
    /// inside a try whose catch throws, never names `Move-Item` or a path, and
    /// takes its paths and retry count only from the JSON blob.
    #[test]
    fn record_replace_is_fixed_fail_closed_text() {
        let body = RECORD_REPLACE;
        assert!(!body.is_empty());
        assert!(
            body.bytes().all(|b| (0x20..=0x7e).contains(&b)),
            "one line of printable ASCII"
        );
        assert!(!body.contains("ign-deploy") && !body.contains("C:\\"));
        assert!(
            !body.contains("Move-Item"),
            "not the delete-then-rename cmdlet"
        );
        assert!(body.contains("$ExecutionContext.SessionState.LanguageMode -ne 'FullLanguage'"));
        assert!(body.contains("MoveFileEx(string a,string b,uint c)"));
        assert!(body.contains("kernel32.dll") && body.contains("SetLastError=true"));
        assert!(
            body.contains("0x1 -bor 0x8"),
            "REPLACE_EXISTING | WRITE_THROUGH"
        );
        assert!(body.contains("GetLastWin32Error"));
        assert!(body.contains("$p.src") && body.contains("$p.dst") && body.contains("$p.retries"));
        // the type-exists guard comes first and there is a single Add-Type
        let guard = body.find("'IgnRecord.Native' -as [type]").expect("guard");
        assert_eq!(body.matches("Add-Type").count(), 1);
        let add = body.find("Add-Type").expect("Add-Type");
        assert!(guard < add, "the guard precedes the declaration");
        let try_at = body[..add].rfind("try{").expect("try before Add-Type");
        let catch_at = body[add..].find("catch{throw ").expect("catch rethrows") + add;
        assert!(guard < try_at && try_at < add && add < catch_at);

        // sized for realistic account-home paths, plain and accented
        let long = format!("C:\\Users\\svc\\{}", "d".repeat(227));
        assert_eq!(long.len(), 240);
        let accented = format!("C:\\Users\\sv\u{e7}\\{}", "\u{e9}".repeat(110));
        for path in [&long, &accented] {
            assert!(
                encoded_command(
                    &WinScript::record_replace(
                        path,
                        path,
                        crate::deploy::transport::batch::RENAME_RETRIES
                    ),
                    COMMAND_BUDGET
                )
                .is_ok(),
                "fits the budget"
            );
        }

        // the paths and the count reach the script only through the blob
        let (src, dst) = ("C:\\h\\a.tmp-r1", "C:\\h\\a.json");
        let split = |retries: u32| {
            let text = decode_command(
                &encoded_command(
                    &WinScript::record_replace(src, dst, retries),
                    COMMAND_BUDGET,
                )
                .expect("command"),
            );
            let blob = text
                .split("FromBase64String('")
                .nth(1)
                .and_then(|rest| rest.split('\'').next())
                .expect("blob")
                .to_string();
            let json = base64::engine::general_purpose::STANDARD
                .decode(&blob)
                .expect("blob base64");
            let value: Value = serde_json::from_slice(&json).expect("json");
            (text.replace(&blob, "<blob>"), value)
        };
        let (text3, value3) = split(3);
        let (text7, value7) = split(7);
        assert_eq!(
            value3,
            serde_json::json!({ "src": src, "dst": dst, "retries": 3 })
        );
        assert_eq!(
            value7,
            serde_json::json!({ "src": src, "dst": dst, "retries": 7 })
        );
        assert!(value3["retries"].is_number());
        assert_eq!(
            text3, text7,
            "the text outside the blob does not depend on the count"
        );
        assert!(text3.contains("$p.retries"));
        assert!(!text3.contains(src) && !text3.contains(dst));
        assert!(text3.ends_with(RECORD_REPLACE));
    }

    /// Whitespace-free form: PowerShell's 119-column wrap can fall anywhere
    /// and `clixml_errors` trims each chunk, so texts are compared squashed.
    fn squash(text: &str) -> String {
        text.chars().filter(|c| !c.is_whitespace()).collect()
    }

    /// T-25-46, T-25-67, T-25-68: every thrown text is a bounded, single-line,
    /// printable-ASCII sentence built from the three fixed prefixes, and the
    /// 25-20 mechanism is still there byte for byte.
    #[test]
    fn record_replace_cause_is_bounded_single_line_ascii() {
        let body = RECORD_REPLACE;
        assert!(
            body.bytes().all(|b| (0x20..=0x7e).contains(&b)),
            "one line of printable ASCII"
        );
        let prefixes = [
            "the record replace needs a FullLanguage PowerShell session, not ",
            "the record replace cannot declare MoveFileEx: ",
            "the record replace failed with Win32 error ",
        ];
        for prefix in prefixes {
            assert!(body.contains(prefix), "{prefix}");
            // the cause is at most 200 characters + `...`, the Win32 number
            // at most 11 characters + `: ` (the language mode is at most 19)
            assert!(prefix.len() + 203 + 13 <= 300, "{prefix}");
        }
        let language = body
            .find("$ExecutionContext.SessionState.LanguageMode -ne 'FullLanguage'")
            .expect("language check");
        let line = body.find("function IgnLine").expect("IgnLine");
        let cause = body.find("function IgnCause").expect("IgnCause");
        let guard = body.find("'IgnRecord.Native' -as [type]").expect("guard");
        assert!(language < line && line < cause && cause < guard);
        let cause_body = &body[cause..guard];
        assert!(cause_body.contains("try{") && cause_body.contains("catch{"));
        assert!(
            cause_body.contains("catch{'the cause could not be read'}"),
            "a failing helper returns a fixed sentence"
        );
        assert!(!body[line..guard].contains("Add-Type"));
        // the cause is concatenated into a string and never evaluated
        let lower = body.to_ascii_lowercase();
        for banned in ["invoke-expression", "iex ", "scriptblock", "[scriptblock]"] {
            assert!(!lower.contains(banned), "{banned}");
        }
        // 25-20 mechanism segments, unchanged
        for segment in [
            "if(-not ('IgnRecord.Native' -as [type])){try{Add-Type -Namespace IgnRecord -Name Native -MemberDefinition '[DllImport(\"kernel32.dll\",SetLastError=true,CharSet=CharSet.Unicode)]public static extern bool MoveFileEx(string a,string b,uint c);'}catch{throw ",
            "$f=[uint32](0x1 -bor 0x8);$n=0;",
            "while(-not [IgnRecord.Native]::MoveFileEx($p.src,$p.dst,$f)){$c=[Runtime.InteropServices.Marshal]::GetLastWin32Error();",
            "if(($c -ne 32 -and $c -ne 5) -or $n -ge [int]$p.retries){throw ",
            "$n++;Start-Sleep -Seconds 1};E @{ok=1}",
        ] {
            assert!(body.contains(segment), "{segment}");
        }
        assert!(body.ends_with("$n++;Start-Sleep -Seconds 1};E @{ok=1}"));
        // no failure branch reaches the success line other than the loop exit
        assert_eq!(body.matches("E @{ok=1}").count(), 1);
    }

    /// A thrown text of exactly `len` characters, made of numbered words.
    fn thrown_text(len: usize, special: bool) -> String {
        let mut text = String::from("the record replace failed with Win32 error 32: ");
        if special {
            text.push_str("<path> it's a & b ");
        }
        let mut n = 0;
        while text.len() < len {
            text.push_str(&format!("w{n} "));
            n += 1;
        }
        text.truncate(len);
        text
    }

    /// WR-01: the cause survives the captured PowerShell 5.1 error record and
    /// the 5-line stderr tail, whole, up to the 300-character bound.
    #[test]
    fn record_replace_cause_survives_the_clixml_tail() {
        let failed = |thrown: &str| -> String {
            let stderr = ps51_error_clixml(thrown);
            let output = ProcessOutput {
                stdout: Vec::new(),
                stderr,
                code: 1,
                timed_out: false,
            };
            interpret_output(&output, 60, None).expect_err("a non-zero exit is an error")
        };
        for (len, special) in [(60, false), (150, false), (150, true), (299, true)] {
            let thrown = thrown_text(len, special);
            assert_eq!(thrown.len(), len);
            let detail = failed(&thrown);
            assert!(
                squash(&detail).contains(&squash(&thrown)),
                "{len}: {detail}"
            );
            assert!(
                !detail.contains("<S S=") && !detail.contains("&lt;"),
                "{detail}"
            );
            if special {
                assert!(
                    detail.contains("<path>") && detail.contains('&'),
                    "{detail}"
                );
            }
        }
        // the boundary: exactly 300 characters fills the 5-line tail and is whole
        let edge = thrown_text(300, false);
        assert_eq!(edge.chars().count(), 300);
        let detail = failed(&edge);
        assert!(squash(&detail).contains(&squash(&edge)), "{detail}");
        let tail = detail
            .strip_prefix("exit code 1: ")
            .expect("exit code prefix");
        assert_eq!(tail.lines().count(), 5, "{detail}");
        // control: past the bound the tail cuts, which is why the bound matters
        let long = thrown_text(700, false);
        let detail = failed(&long);
        assert!(!squash(&detail).contains(&squash(&long)), "{detail}");
    }

    // The records below are copied byte for byte from
    // .planning/phases/22-transport-plan-pull-verify/22-LIVE-CAPTURES.md. The
    // capture tool removed one CR from each, so the raw bytes are the
    // `#< CLIXML` line, CRLF, then the `<Objs>` line with no trailing newline.
    /// Lines 4751-4752: `throw 'boom'`, bare.
    const CAPTURED_BOOM_BARE: &str = concat!(
        "#< CLIXML",
        "\r\n",
        r#"<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04"><S S="Error">boom_x000D__x000A_</S><S S="Error">At line:1 char:1_x000D__x000A_</S><S S="Error">+ throw 'boom'_x000D__x000A_</S><S S="Error">+ ~~~~~~~~~~~~_x000D__x000A_</S><S S="Error">    + CategoryInfo          : OperationStopped: (boom:String) [], RuntimeException_x000D__x000A_</S><S S="Error">    + FullyQualifiedErrorId : boom_x000D__x000A_</S><S S="Error"> _x000D__x000A_</S></Objs>"#
    );
    /// Lines 4767-4768: `throw 'boom'` after the prelude.
    const CAPTURED_BOOM_PRELUDE: &str = concat!(
        "#< CLIXML",
        "\r\n",
        r#"<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04"><S S="Error">boom_x000D__x000A_</S><S S="Error">At line:1 char:72_x000D__x000A_</S><S S="Error">+ ... rence='SilentlyContinue'; $ErrorActionPreference='Stop'; throw 'boom'_x000D__x000A_</S><S S="Error">+                                                              ~~~~~~~~~~~~_x000D__x000A_</S><S S="Error">    + CategoryInfo          : OperationStopped: (boom:String) [], RuntimeException_x000D__x000A_</S><S S="Error">    + FullyQualifiedErrorId : boom_x000D__x000A_</S><S S="Error"> _x000D__x000A_</S></Objs>"#
    );
    /// Line 7223 alone: a Get-ChildItem error on a long path.
    const CAPTURED_WRAPPED: &str = r#"<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04"><S S="Error">Get-ChildItem : Could not find a part of the path 'C:\Program Files\Inductive Automation\Ignition\data\config\resources_x000D__x000A_</S><S S="Error">\core\ignition\tag-definition\CAP22H\LLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLL_x000D__x000A_</S><S S="Error">LLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLL'_x000D__x000A_</S><S S="Error">._x000D__x000A_</S><S S="Error">At line:1 char:72_x000D__x000A_</S><S S="Error">+ ... nce='Stop'; Get-ChildItem -LiteralPath (Join-Path 'C:\Program Files\I ..._x000D__x000A_</S><S S="Error">+                 ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~_x000D__x000A_</S><S S="Error">    + CategoryInfo          : ReadError: (C:\Program File...LLLLLLLLLLLLLLL:String) [Get-ChildItem], DirectoryNotFound _x000D__x000A_</S><S S="Error">   Exception_x000D__x000A_</S><S S="Error">    + FullyQualifiedErrorId : DirIOError,Microsoft.PowerShell.Commands.GetChildItemCommand_x000D__x000A_</S><S S="Error"> _x000D__x000A_</S></Objs>"#;

    /// The raw `<S S="Error">` chunk bodies of a CLIXML record, with the
    /// `_x000D__x000A_` ends left on.
    fn raw_chunks(clixml: &str) -> Vec<String> {
        clixml
            .split("<S S=\"Error\">")
            .skip(1)
            .map(|piece| piece.split("</S>").next().expect("chunk").to_string())
            .collect()
    }

    const CRLF_ENTITY: &str = "_x000D__x000A_";

    /// The offline proof that `ps51_error_clixml` is the PowerShell 5.1 shape
    /// and not only a model of it: the cause-survival test and the writer
    /// mock test rest on it (D-03, D-06, D-12).
    #[test]
    fn ps51_error_clixml_matches_the_captured_records() {
        for captured in [CAPTURED_BOOM_BARE, CAPTURED_BOOM_PRELUDE] {
            let built = String::from_utf8(ps51_error_clixml("boom")).expect("utf-8");
            let head = |text: &str| text.split("<S S=").next().expect("head").to_string();
            assert_eq!(head(&built), head(captured), "framing through <Objs>");
            assert!(built.ends_with("</Objs>") && captured.ends_with("</Objs>"));
            assert!(!built.ends_with('\n') && !captured.ends_with('\n'));
            let (mine, theirs) = (raw_chunks(&built), raw_chunks(captured));
            assert_eq!((mine.len(), theirs.len()), (7, 7));
            for at in [0, 4, 5, 6] {
                assert_eq!(mine[at], theirs[at], "chunk {}", at + 1);
                assert!(mine[at].ends_with(CRLF_ENTITY));
            }
            // chunks 2 to 4 depend on the script position: the form only
            for chunk in [&mine, &theirs] {
                let digits = chunk[1]
                    .strip_prefix("At line:1 char:")
                    .and_then(|rest| rest.strip_suffix(CRLF_ENTITY))
                    .expect("At line form");
                assert!(!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()));
                assert!(chunk[2].starts_with("+ ") && chunk[2].ends_with(CRLF_ENTITY));
                let marks = chunk[3]
                    .strip_prefix('+')
                    .and_then(|rest| rest.strip_suffix(CRLF_ENTITY))
                    .expect("underline form");
                assert!(marks.chars().all(|c| c == ' ' || c == '~'));
                assert!(marks.contains('~'));
            }
            let last_two = |text: &str| {
                let errors = clixml_errors(text.as_bytes());
                errors[errors.len() - 2..].to_vec()
            };
            assert_eq!(last_two(&built), last_two(captured));
            assert_eq!(
                last_two(captured),
                [
                    "+ CategoryInfo          : OperationStopped: (boom:String) [], RuntimeException",
                    "+ FullyQualifiedErrorId : boom",
                ]
            );
        }

        // line 7223: the message wrap, the wrapped + line, the ellipsis
        let captured = raw_chunks(CAPTURED_WRAPPED);
        assert_eq!(captured.len(), 11);
        let bare: Vec<String> = captured
            .iter()
            .map(|chunk| chunk.strip_suffix(CRLF_ENTITY).expect("end").to_string())
            .collect();
        let message = bare[..4].concat();
        assert_eq!(
            bare[..4]
                .iter()
                .map(|c| c.chars().count())
                .collect::<Vec<_>>(),
            [119, 119, 119, 1]
        );
        assert_eq!(ps51_wrap(&message), bare[..4]);
        let quoted = message
            .split_once("the path '")
            .and_then(|(_, rest)| rest.strip_suffix("'."))
            .expect("quoted path");
        let ellipsis = ps51_ellipsize(quoted);
        assert_eq!(ellipsis, "C:\\Program File...LLLLLLLLLLLLLLL");
        let category = format!(
            "+ CategoryInfo          : ReadError: ({ellipsis}:String) [Get-ChildItem], DirectoryNotFoundException"
        );
        assert_eq!(ps51_wrap(&ps51_info_line(&category)), bare[7..9]);
        assert_eq!(bare[7].chars().count(), 119);
        assert!(bare[7].ends_with(' ') && bare[8].starts_with("   Exception"));
        let id = "+ FullyQualifiedErrorId : DirIOError,Microsoft.PowerShell.Commands.GetChildItemCommand";
        assert_eq!(ps51_wrap(&ps51_info_line(id)), bare[9..10]);
        assert_eq!(bare[10], " ");
        // the short-target rule: nothing is cut at 40 characters or fewer
        assert_eq!(ps51_ellipsize("boom"), "boom");
    }

    #[test]
    fn parse_ign_result_cases() {
        let ok = parse_ign_result(b"noise\r\nIGN-RESULT:{\"ok\":1}\r\n").expect("one line");
        assert_eq!(ok["ok"], 1);
        // a result with no trailing newline
        assert!(parse_ign_result(b"IGN-RESULT:{\"ok\":1}").is_ok());
        for (bad, why) in [
            (&b""[..], "no line"),
            (b"hello\n", "sentinel absent"),
            (b"IGN-RESULT:{\"a\":1}\nIGN-RESULT:{\"a\":2}\n", "two lines"),
            (b"IGN-RESULT:{nope\n", "invalid JSON"),
            (b"IGN-RESULT:\n", "empty payload"),
            (
                b" IGN-RESULT:{\"ok\":1}\n",
                "indented sentinel is not a sentinel line",
            ),
        ] {
            assert!(parse_ign_result(bad).is_err(), "{why} must be an error");
        }
    }

    fn out(code: i32, stdout: &str, stderr: &str, timed_out: bool) -> ProcessOutput {
        ProcessOutput {
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
            code,
            timed_out,
        }
    }

    const CLIXML_ERROR: &str = "#< CLIXML\r\n<Objs Version=\"1.1.0.1\" xmlns=\"http://schemas.microsoft.com/powershell/2004/04\"><S S=\"Error\">Cannot find path 'C:\\x' because it does not exist._x000D__x000A_</S><S S=\"Error\">At line:1 char:1_x000D__x000A_</S></Objs>";
    const CLIXML_PROGRESS: &str = "#< CLIXML\r\n<Objs Version=\"1.1.0.1\" xmlns=\"http://schemas.microsoft.com/powershell/2004/04\"><Obj S=\"progress\" RefId=\"0\"><TN RefId=\"0\"><T>System.Management.Automation.PSCustomObject</T></TN><MS><I64 N=\"SourceId\">1</I64></MS></Obj></Objs>";

    #[test]
    fn interpret_output_decides_from_exit_code_and_the_sentinel() {
        let ok = out(0, "IGN-RESULT:{\"ok\":1}\r\n", "", false);
        assert_eq!(interpret_output(&ok, 5, None).expect("ok")["ok"], 1);
        // progress CLIXML on stderr does not make a success a failure
        let progress = out(0, "IGN-RESULT:{\"ok\":1}\r\n", CLIXML_PROGRESS, false);
        assert!(interpret_output(&progress, 5, None).is_ok());

        // non-zero exit: failure, with the CLIXML error text decoded for display
        let failed = out(1, "", CLIXML_ERROR, false);
        let detail = interpret_output(&failed, 5, None).expect_err("exit 1");
        assert!(detail.starts_with("exit code 1: "), "{detail}");
        assert!(detail.contains("Cannot find path"), "{detail}");
        assert!(
            !detail.contains("<S S="),
            "no raw XML in the detail: {detail}"
        );
        // exit 0 with an error record on stderr
        let recorded = out(0, "IGN-RESULT:{\"ok\":1}\r\n", CLIXML_ERROR, false);
        let detail = interpret_output(&recorded, 5, None).expect_err("error record");
        assert!(detail.contains("error record"), "{detail}");
        // a missing sentinel with exit 0
        let silent = out(0, "all good\r\n", "", false);
        assert!(interpret_output(&silent, 5, None).is_err());
        // a timeout
        let timed = out(-1, "", "", true);
        assert_eq!(
            interpret_output(&timed, 7, None).expect_err("timeout"),
            "timed out after 7s and was killed"
        );
        // the host refusing a long command line (C7a-7)
        let long = out(1, "", "The command line is too long.\r\n", false);
        let detail = interpret_output(&long, 5, None).expect_err("too long");
        assert!(detail.contains("too long"), "{detail}");
        // ssh itself failing
        let ssh = out(255, "", "exec request failed on channel 0", false);
        assert!(
            interpret_output(&ssh, 5, None)
                .expect_err("255")
                .contains("exit code 255")
        );
    }

    #[test]
    fn clixml_errors_decode_and_ignore_non_clixml() {
        let got = clixml_errors(CLIXML_ERROR.as_bytes());
        assert_eq!(got.len(), 2);
        assert_eq!(
            got[0],
            "Cannot find path 'C:\\x' because it does not exist."
        );
        assert!(clixml_errors(b"plain stderr").is_empty());
        assert!(clixml_errors(CLIXML_PROGRESS.as_bytes()).is_empty());
    }

    #[test]
    fn parse_enumeration_cases() {
        let value = serde_json::json!({
            "ok": 1,
            "res": [
                { "i": 0, "x": 1, "e": [["a\\b.json", 0, 12, 0, 90], ["a", 1, 0, 0, 80]] },
                { "i": 1, "x": 0, "e": [] }
            ]
        });
        let got = parse_enumeration(&value, 2).expect("parse");
        let first = got[0].as_ref().expect("root 0 exists");
        assert_eq!(first[0].rel, "a/b.json", "backslashes become slashes");
        assert!(!first[0].dir && first[0].size == 12 && !first[0].reparse);
        assert!(first[1].dir);
        assert!(got[1].is_none(), "a missing root");
        for bad in [
            serde_json::json!({ "ok": 0, "res": [] }),
            serde_json::json!({ "ok": 1 }),
            serde_json::json!({ "ok": 1, "res": [] }),
            serde_json::json!({ "ok": 1, "res": [{ "i": 5, "x": 1, "e": [] }, { "i": 1, "x": 1, "e": [] }] }),
            serde_json::json!({ "ok": 1, "res": [{ "i": 0, "x": 1, "e": [["a"]] }, { "i": 1, "x": 0 }] }),
        ] {
            assert!(parse_enumeration(&bad, 2).is_err(), "{bad}");
        }
    }

    /// The PowerShell the tests can run: `pwsh` on the dev Mac and CI,
    /// `powershell` on Windows. Fails loudly, never skips.
    fn powershell() -> std::path::PathBuf {
        for name in ["pwsh", "powershell", "powershell.exe"] {
            let Some(paths) = std::env::var_os("PATH") else {
                continue;
            };
            for dir in std::env::split_paths(&paths) {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
        for fixed in [
            "/usr/local/bin/pwsh",
            "/opt/homebrew/bin/pwsh",
            "/usr/bin/pwsh",
        ] {
            if std::path::Path::new(fixed).is_file() {
                return fixed.into();
            }
        }
        panic!(
            "no pwsh or powershell on PATH: install PowerShell 7 (macOS: `brew install --cask powershell`; Debian/Ubuntu: the Microsoft packages) so the script-logic tests can run. These tests are never skipped."
        );
    }

    /// Run `script` for real: the same text `-EncodedCommand` carries, as
    /// `-Command`. Returns the output.
    fn run_script(script: &WinScript) -> std::process::Output {
        let command = encoded_command(script, COMMAND_BUDGET).expect("command");
        let encoded = command
            .as_str()
            .strip_prefix("powershell -NoProfile -NonInteractive -EncodedCommand ")
            .expect("prefix");
        std::process::Command::new(powershell())
            .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", encoded])
            .stdin(std::process::Stdio::null())
            .output()
            .expect("run powershell")
    }

    #[test]
    fn winps_script_logic() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("src file.txt");
        let dst = dir.path().join("dst; echo 'hi' $(x).txt");
        std::fs::write(&src, b"new").expect("src");
        std::fs::write(&dst, b"old").expect("dst");

        // MOVE_REPLACE replaces an existing target and prints the sentinel
        let moved = run_script(&WinScript::move_replace(
            src.to_str().expect("utf8"),
            dst.to_str().expect("utf8"),
        ));
        assert!(moved.status.success(), "{moved:?}");
        let value = parse_ign_result(&moved.stdout).expect("sentinel");
        assert_eq!(value["ok"], 1);
        assert_eq!(std::fs::read(&dst).expect("dst"), b"new");
        assert!(!src.exists(), "the source was moved");

        // a missing source exits non-zero and prints no sentinel
        let missing = run_script(&WinScript::move_replace(
            dir.path().join("nope").to_str().expect("utf8"),
            dst.to_str().expect("utf8"),
        ));
        assert!(!missing.status.success(), "a missing source must fail");
        assert!(parse_ign_result(&missing.stdout).is_err());

        // a script that exits 7 propagates its code
        let seven = run_script(&WinScript {
            name: "seven",
            body: "exit 7",
            params: Value::Null,
        });
        assert_eq!(seven.status.code(), Some(7));

        // non-ASCII text leaves the host as \u escapes, ASCII only
        let unicode = run_script(&WinScript {
            name: "unicode",
            body: "E @{s=\"caf\u{e9} \u{1f600}\"}",
            params: Value::Null,
        });
        assert!(unicode.status.success(), "{unicode:?}");
        assert!(unicode.stdout.is_ascii(), "stdout must be ASCII");
        let value = parse_ign_result(&unicode.stdout).expect("sentinel");
        assert_eq!(value["s"], "caf\u{e9} \u{1f600}");
    }

    #[test]
    fn winps_enumerate_reports_files_dirs_and_reparse_points() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("root");
        std::fs::create_dir_all(root.join("a b").join("c")).expect("dirs");
        std::fs::write(root.join("top.json"), b"12345").expect("file");
        std::fs::write(root.join("a b").join("caf\u{e9}.json"), b"xyz").expect("file");
        std::fs::write(root.join("a b").join("c").join("deep.json"), b"").expect("file");
        let absent = dir.path().join("absent");
        let run = |roots: &[String]| {
            let out = run_script(&WinScript::enumerate("", roots));
            assert!(out.status.success(), "{out:?}");
            parse_enumeration(
                &parse_ign_result(&out.stdout).expect("sentinel"),
                roots.len(),
            )
            .expect("enumeration")
        };
        let roots = vec![
            root.to_str().expect("utf8").to_string(),
            absent.to_str().expect("utf8").to_string(),
        ];
        let got = run(&roots);
        let mut entries: Vec<(String, bool, u64, bool)> = got[0]
            .as_ref()
            .expect("root exists")
            .iter()
            .map(|e| (e.rel.clone(), e.dir, e.size, e.reparse))
            .collect();
        entries.sort();
        assert_eq!(
            entries,
            [
                ("a b".to_string(), true, 0, false),
                ("a b/c".to_string(), true, 0, false),
                ("a b/c/deep.json".to_string(), false, 0, false),
                ("a b/caf\u{e9}.json".to_string(), false, 3, false),
                ("top.json".to_string(), false, 5, false),
            ]
        );
        assert!(got[1].is_none(), "a missing root is reported as missing");

        // an empty root exists and has no entries
        let empty = dir.path().join("empty");
        std::fs::create_dir(&empty).expect("empty");
        let got = run(&[empty.to_str().expect("utf8").to_string()]);
        assert_eq!(got[0].as_ref().expect("exists").len(), 0);

        // a symlink is a reparse point on every platform pwsh runs on
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&root, root.join("link")).expect("symlink");
            let got = run(&[root.to_str().expect("utf8").to_string()]);
            let link = got[0]
                .as_ref()
                .expect("exists")
                .iter()
                .find(|e| e.rel == "link")
                .expect("the link is reported");
            // on macOS/Linux pwsh the ReparsePoint attribute is set for symlinks
            assert!(link.reparse, "a symlink is flagged: {link:?}");
        }
    }

    /// C6 (PR #31 review): the names-only listing reports the direct
    /// children of each root, with the same columns as [`ENUMERATE`], and
    /// never opens a child directory.
    #[test]
    fn winps_enumerate_children_reports_direct_children_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("root");
        std::fs::create_dir_all(root.join("a b").join("c")).expect("dirs");
        std::fs::write(root.join("top.json"), b"12345").expect("file");
        std::fs::write(root.join("a b").join("x.json"), b"xyz").expect("file");
        std::fs::write(root.join("a b").join("c").join("deep.json"), b"").expect("file");
        let absent = dir.path().join("absent");
        let roots = vec![
            root.to_str().expect("utf8").to_string(),
            absent.to_str().expect("utf8").to_string(),
        ];
        let run = |roots: &[String]| {
            let out = run_script(&WinScript::enumerate_children("", roots));
            assert!(out.status.success(), "{out:?}");
            parse_enumeration(
                &parse_ign_result(&out.stdout).expect("sentinel"),
                roots.len(),
            )
            .expect("enumeration")
        };
        let got = run(&roots);
        let listed = got[0].as_ref().expect("root exists");
        let mut entries: Vec<(String, bool, u64, bool)> = listed
            .iter()
            .map(|e| (e.rel.clone(), e.dir, e.size, e.reparse))
            .collect();
        entries.sort();
        assert_eq!(
            entries,
            [
                ("a b".to_string(), true, 0, false),
                ("top.json".to_string(), false, 5, false),
            ]
        );
        let top = listed.iter().find(|e| e.rel == "top.json").expect("top");
        assert!(top.full_len >= root.to_str().expect("utf8").len() + "/top.json".len());
        assert!(got[1].is_none(), "a missing root is reported as missing");

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("a b"), root.join("link")).expect("symlink");
            let got = run(&roots[..1]);
            let listed = got[0].as_ref().expect("exists");
            let link = listed
                .iter()
                .find(|e| e.rel == "link")
                .expect("the link is reported");
            assert!(link.reparse, "a symlink is flagged: {link:?}");
            assert!(
                listed.iter().all(|e| !e.rel.contains('/')),
                "a reparse point is not followed: {listed:?}"
            );
        }
    }

    /// C6: the names-only body has no walk (no stack, no push), is one line
    /// of printable ASCII and fits the budget with a realistic root; the
    /// read's [`ENUMERATE`] keeps its walk.
    #[test]
    fn enumerate_children_never_descends() {
        let body = ENUMERATE_CHILDREN;
        assert!(body.bytes().all(|b| (0x20..=0x7e).contains(&b)));
        assert!(!body.contains("Stack") && !body.contains("Push"), "{body}");
        assert!(ENUMERATE.contains("Collections.Stack") && ENUMERATE.contains(".Push("));
        let root = "C:\\Program Files\\Inductive Automation\\Ignition\\data\\config\\resources\\core\\ignition\\tag-provider".to_string();
        assert!(
            encoded_command(
                &WinScript::enumerate_children(LONG_PATH_PREFIX, &[root]),
                COMMAND_BUDGET
            )
            .is_ok()
        );
    }

    /// G-22-1: the relative paths come from the walk, not from cutting the
    /// caller's spelling of the root off each `FullName`. .NET rewrites a root
    /// it normalises: it expands an 8.3 component (`RUNNER~1` becomes
    /// `runneradmin` under the Windows runner's `%TEMP%`, 3 characters longer)
    /// and drops `.`, doubled separators and a trailing separator, so a length
    /// taken from the caller's string shifts every entry (`ot\a b` for `a b`).
    #[test]
    fn winps_enumerate_rel_paths_do_not_depend_on_the_root_spelling() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("root");
        std::fs::create_dir_all(root.join("a b").join("c")).expect("dirs");
        std::fs::write(root.join("top.json"), b"12345").expect("file");
        std::fs::write(root.join("a b").join("c").join("deep.json"), b"").expect("file");
        let base = dir.path().to_str().expect("utf8");
        let sep = std::path::MAIN_SEPARATOR;
        let expected = [
            ("a b".to_string(), true, 0, false),
            ("a b/c".to_string(), true, 0, false),
            ("a b/c/deep.json".to_string(), false, 0, false),
            ("top.json".to_string(), false, 5, false),
        ];
        for spelling in [
            // on the Windows runner this one is the 8.3 `RUNNER~1` path
            format!("{base}{sep}root"),
            format!("{base}{sep}.{sep}root"),
            format!("{base}{sep}{sep}root"),
            format!("{base}{sep}root{sep}"),
        ] {
            let roots = [spelling.clone()];
            let out = run_script(&WinScript::enumerate("", &roots));
            assert!(out.status.success(), "{spelling}: {out:?}");
            let got = parse_enumeration(&parse_ign_result(&out.stdout).expect("sentinel"), 1)
                .expect("enumeration");
            let mut entries: Vec<(String, bool, u64, bool)> = got[0]
                .as_ref()
                .expect("root exists")
                .iter()
                .map(|e| (e.rel.clone(), e.dir, e.size, e.reparse))
                .collect();
            entries.sort();
            assert_eq!(entries, expected, "root spelled {spelling:?}");
        }
    }

    /// Gate parity (post-review): everything the POSIX gate refuses, and the
    /// Windows-only ways to escape a directory, are refused as a component.
    #[test]
    fn windows_component_gate_refuses_what_posix_refuses_and_more() {
        for ok in [
            "plain",
            "MES db,1",
            "Caf\u{e9} DB",
            ".resources",
            "Program Files (x86)",
            "a.b",
            "%43%4f%4e",
            "CONSOLE",
            "NULL1",
            "COM10",
        ] {
            assert_eq!(windows_component_problem(ok), None, "{ok:?} is allowed");
        }
        for bad in [
            // the POSIX gate
            "",
            ".",
            "..",
            "a/b",
            "a\\b",
            "a\0b",
            "a\nb",
            // drive letters and alternate data streams
            "C:",
            "C:x",
            "a:b",
            "name:stream",
            "name::$DATA",
            // \\?\ and UNC prefixes arrive as separators and colons
            "\\\\?\\C:",
            "?",
            // Windows strips a trailing dot or space: two names, one entry
            "a.",
            "a ",
            "a..",
            // reserved device names, with or without an extension
            "CON",
            "con",
            "Nul",
            "aux.txt",
            "PRN.json",
            "COM1",
            "lpt9.cfg",
            // characters Windows forbids
            "a<b",
            "a>b",
            "a|b",
            "a\"b",
            "a*b",
        ] {
            assert!(
                windows_component_problem(bad).is_some(),
                "{bad:?} must be refused"
            );
        }
    }

    #[test]
    fn windows_absolute_path_gate() {
        for ok in [
            "/C:/Users/svc",
            "/c:/Users/svc/ign-deploy/gw/production/staging/run-1",
            "/D:/a b/c",
        ] {
            assert_eq!(windows_absolute_path_problem(ok), None, "{ok}");
        }
        for bad in [
            "",
            "/",
            "/C:",
            "C:/Users/svc",
            "/Users/svc",
            "/C:/Users/../x",
            "/C:/Users/./x",
            "/C:/Users//x",
            "/C:/Users/svc/D:/x",
            "/C:/Users/svc/x:stream",
            "/C:/Users/svc/x.",
            "/C:/Users/svc/NUL",
            "//server/share/x",
            "/C:\\Users",
            "/C:/Users/svc\0",
            "/\\\\?\\C:/x",
            "/CC:/x",
        ] {
            assert!(
                windows_absolute_path_problem(bad).is_some(),
                "{bad:?} must be refused"
            );
        }
    }

    #[test]
    fn parse_enumeration_applies_the_windows_gate_to_every_path() {
        let entry = |rel: &str| {
            serde_json::json!({
                "ok": 1,
                "res": [{ "i": 0, "x": 1, "e": [[rel, 0, 1, 0, 50]] }]
            })
        };
        assert!(parse_enumeration(&entry("a\\b.json"), 1).is_ok());
        for bad in [
            "..\\x",
            "a\\..\\b",
            "\\\\?\\C:\\x",
            "C:\\x",
            "/abs",
            "a\\\\b",
            "a:b\\c",
            "a\\b.",
            "a\\NUL",
            "",
            "a\0b",
        ] {
            assert!(
                parse_enumeration(&entry(bad), 1).is_err(),
                "{bad:?} must be refused"
            );
        }
    }

    // ---- the script stub (23-08) --------------------------------------------

    fn roots(staging: &str, collection: &str, deploy: &str) -> StubRoots {
        StubRoots {
            staging: staging.to_string(),
            collection: collection.to_string(),
            deploy: deploy.to_string(),
        }
    }

    fn sha(fill: char) -> String {
        std::iter::repeat_n(fill, 64).collect()
    }

    #[test]
    fn stub_fits_the_budget() {
        // the longest roots a legal node can make: the data directory is held to
        // WINDOWS_DATA_DIR_MAX (150) characters and everything below the account
        // home stays under LONG_PATH_LIMIT; 259-character roots are past both
        for len in [60usize, 150, 240, 259] {
            let root = |tag: char| format!("C:\\{}", tag.to_string().repeat(len - 3));
            let stub = script_stub(
                &sha('a'),
                &sha('b'),
                "restore",
                &roots(&root('s'), &root('c'), &root('d')),
            );
            let command = encoded_command(&stub, COMMAND_BUDGET)
                .unwrap_or_else(|why| panic!("{len}-character roots: {why}"));
            eprintln!(
                "stub with {len}-character roots: {} characters",
                command.as_str().chars().count()
            );
            assert!(
                command.as_str().chars().count() <= COMMAND_BUDGET,
                "{len}-character roots"
            );
        }
        // non-ASCII path characters travel as UTF-8 inside the JSON and still fit
        let accented = format!("C:\\Users\\{}", "\u{e9}".repeat(100));
        let stub = script_stub(
            &sha('a'),
            &sha('b'),
            "commit",
            &roots(&accented, &accented, &accented),
        );
        assert!(encoded_command(&stub, COMMAND_BUDGET).is_ok());
    }

    #[test]
    fn stub_body_is_fixed_text_and_carries_no_path_or_hash() {
        let stub = script_stub(
            &sha('a'),
            &sha('b'),
            "commit",
            &roots(
                "C:\\Users\\svc\\ign-deploy\\gw\\production\\staging\\run-1",
                "C:\\Data\\config\\resources\\production",
                "C:\\Users\\svc\\ign-deploy\\gw\\production",
            ),
        );
        assert_eq!(stub.body, STUB);
        // the body is one line of printable ASCII
        assert!(STUB.bytes().all(|b| (0x20..0x7f).contains(&b)), "{STUB}");
        assert!(!STUB.contains("ign-deploy") && !STUB.contains("Users"));
        // the parameters hold the roots, the operation and both hashes
        let text = decode_command(&encoded_command(&stub, COMMAND_BUDGET).expect("command"));
        let blob = text
            .split("FromBase64String('")
            .nth(1)
            .and_then(|rest| rest.split('\'').next())
            .expect("blob");
        let json = base64::engine::general_purpose::STANDARD
            .decode(blob)
            .expect("blob base64");
        let value: Value = serde_json::from_slice(&json).expect("json");
        assert_eq!(value["o"], "commit");
        assert_eq!(value["ss"], sha('a'));
        assert_eq!(value["ws"], sha('b'));
        assert_eq!(
            value["t"],
            "C:\\Users\\svc\\ign-deploy\\gw\\production\\staging\\run-1"
        );
        // the command text itself shows no path (the parameters are base64)
        assert!(!text.contains("staging"));
        assert!(text.ends_with(STUB));
    }

    #[test]
    fn stub_verifies_before_it_runs_and_hashes_the_bytes_it_runs() {
        // exit 97 comes before the script block is created, and the script and
        // the work list are each read once, hashed and then used from memory
        let verify = STUB.find("exit 97").expect("the mismatch exit");
        let run = STUB.find("[ScriptBlock]::Create").expect("the run");
        assert!(verify < run);
        assert_eq!(STUB.matches("ReadAllBytes").count(), 2);
        assert!(STUB.contains("GetString($b)") && STUB.contains("GetString($w)"));
        assert!(!STUB.contains("ReadAllText"));
        assert!(!STUB.contains("-File") && !STUB.contains("Invoke-Expression"));
    }

    #[test]
    fn the_shipped_script_is_ascii_text_the_stub_can_hash() {
        use crate::deploy::transport::batch::{SCRIPT_PS1, script_ps1_sha256};
        assert!(SCRIPT_PS1.is_ascii(), "the script is pure ASCII");
        assert!(!SCRIPT_PS1.starts_with('\u{feff}'), "no byte order mark");
        assert!(SCRIPT_PS1.contains("IGN-STOP") || SCRIPT_PS1.contains("E $S.Res"));
        assert_eq!(
            script_ps1_sha256(),
            crate::deploy::tree::sha256_hex(SCRIPT_PS1.as_bytes())
        );
        assert_eq!(script_ps1_sha256().len(), 64);
        // it starts with the parameter block after its comment header and uses
        // none of the things a script block run by the stub cannot rely on
        assert!(SCRIPT_PS1.contains("\nparam([string]$Op"));
        assert!(!SCRIPT_PS1.contains("$MyInvocation") && !SCRIPT_PS1.contains("$PSScriptRoot"));
    }

    /// Windows PowerShell 5.1 resolves an alias before a function, so a script
    /// function named like an alias is never called (`LP`, the alias of
    /// `Out-Printer`, made every hash of the first live run empty). The list is
    /// `Get-Alias` on the capture host (Windows PowerShell 5.1.20348, 2026-10-17).
    const ALIASES_5_1: &str = "% ? ac asnp cat cd cfs chdir clc clear clhy cli clp cls clv cnsn compare copy cp cpi cpp curl cvpa dbp del diff dir dnsn ebp echo epal epcsv epsn erase etsn exsn fc fhx fl foreach ft fw gal gbp gc gci gcm gcs gdr ghy gi gjb gl gm gmo gp gps gpv group gsn gsnp gsv gu gv gwmi h history icm iex ihy ii ipal ipcsv ipmo ipsn irm ise iwmi iwr kill lp ls man md measure mi mount move mp mv nal ndr ni nmo npssc nsn nv ogv oh popd ps pushd pwd r rbp rcjb rcsn rd rdr ren ri rjb rm rmdir rmo rni rnp rp rsn rsnp rujb rv rvpa rwmi sajb sal saps sasv sbp sc select set shcm si sl sleep sls sort sp spjb spps spsv start sujb sv swmi tee trcm type wget where wjb write";

    #[test]
    fn the_shipped_script_names_no_function_like_an_alias_or_a_cmdlet() {
        use crate::deploy::transport::batch::SCRIPT_PS1;
        let aliases: std::collections::BTreeSet<&str> = ALIASES_5_1.split(' ').collect();
        let mut seen = 0;
        for line in SCRIPT_PS1.lines() {
            let Some(rest) = line.strip_prefix("function ") else {
                continue;
            };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            seen += 1;
            assert!(
                !aliases.contains(name.to_ascii_lowercase().as_str()),
                "function {name} has the name of a Windows PowerShell alias, which wins over it"
            );
            assert!(!name.contains('-'), "function {name} reads like a cmdlet");
        }
        assert!(
            seen > 20,
            "the script defines its helpers as functions ({seen})"
        );
    }

    /// 23-20: the PowerShell script has every operation the POSIX script has,
    /// in the same dispatch, and the 23-08 hardening holds in the new ones: a
    /// file is moved only by `MoveReplace` (so `mirror` reuses `MoveOne` and
    /// gets the access-list reset), a tree is removed with .NET
    /// `Directory.Delete` and never with `Remove-Item -Recurse`, which in Windows
    /// PowerShell 5.1 can follow a junction.
    #[test]
    fn the_shipped_script_runs_every_operation_commit_sh_has_and_keeps_the_hardening() {
        use crate::deploy::transport::batch::{SCRIPT_PS1, SCRIPT_SH};
        let ops = [
            "commit",
            "restore",
            "mirror",
            "fingerprint",
            "probe",
            "finalize",
            "discard",
            "verify",
            "unstage",
        ];
        let mut sh_order: Vec<usize> = Vec::new();
        let mut ps_order: Vec<usize> = Vec::new();
        for op in ops {
            sh_order.push(
                SCRIPT_SH
                    .find(&format!("  {op}) op_{op} ;;"))
                    .unwrap_or_else(|| panic!("commit.sh has no {op}")),
            );
            let ps = SCRIPT_PS1
                .find(&format!("        '{op}' {{ "))
                .unwrap_or_else(|| panic!("commit.ps1 has no {op}"));
            ps_order.push(ps);
        }
        let ranks = |v: &[usize]| {
            let mut idx: Vec<usize> = (0..v.len()).collect();
            idx.sort_by_key(|i| v[*i]);
            idx
        };
        assert_eq!(
            ranks(&sh_order),
            ranks(&ps_order),
            "the same dispatch order"
        );
        let code: Vec<&str> = SCRIPT_PS1
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .collect();
        let uses = |needle: &str| code.iter().filter(|l| l.contains(needle)).count();
        assert_eq!(uses("Move-Item"), 1, "only MoveReplace moves a file");
        assert_eq!(uses("Remove-Item"), 0, "trees go with Directory.Delete");
        assert!(uses("[IO.Directory]::Delete((Xp $pth), $true)") >= 1);
        // mirror moves through MoveOne, which resets the access list
        let mirror = SCRIPT_PS1
            .split("function OpMirror")
            .nth(1)
            .and_then(|rest| rest.split("# ---- main").next())
            .expect("OpMirror");
        assert!(mirror.contains("Moves"), "{mirror}");
        let moves = SCRIPT_PS1
            .split("function Moves")
            .nth(1)
            .and_then(|rest| rest.split("\nfunction ").next())
            .expect("Moves");
        assert!(moves.contains("MoveOne"));
        let move_one = SCRIPT_PS1
            .split("function MoveOne")
            .nth(1)
            .and_then(|rest| rest.split("\nfunction ").next())
            .expect("MoveOne");
        assert!(move_one.contains("MoveWithRetries") && move_one.contains("ResetAcl"));
    }

    /// A tampered script or work list makes the stub exit 97 before the script
    /// runs. This needs Windows PowerShell (the stub reads `\\?\` paths), so it
    /// runs on the windows-latest CI leg; the same property is asserted against
    /// the real host in `wflwvm01_apply_script_run`.
    #[cfg(windows)]
    #[test]
    fn stub_refuses_hash_mismatch() {
        use crate::deploy::tree::sha256_hex;
        let dir = tempfile::tempdir().expect("temp dir");
        let script = "param($o,$t,$c,$d,$w) [Console]::Out.WriteLine('RAN '+$o)\n";
        let work = "ign-work 1\n";
        std::fs::write(dir.path().join("commit.ps1"), script).expect("script");
        std::fs::write(dir.path().join("work.tsv"), work).expect("work");
        let staging = dir.path().to_string_lossy().into_owned();
        let run = |script_sha: &str, work_sha: &str| {
            let stub = script_stub(
                script_sha,
                work_sha,
                "commit",
                &roots(&staging, &staging, &staging),
            );
            let command = encoded_command(&stub, COMMAND_BUDGET).expect("command");
            let text = command.as_str();
            let b64 = text
                .strip_prefix("powershell -NoProfile -NonInteractive -EncodedCommand ")
                .expect("prefix");
            std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", b64])
                .output()
                .expect("powershell")
        };
        let good = run(&sha256_hex(script.as_bytes()), &sha256_hex(work.as_bytes()));
        assert_eq!(good.status.code(), Some(0), "{good:?}");
        assert!(String::from_utf8_lossy(&good.stdout).contains("RAN commit"));
        for (script_sha, work_sha) in [
            (sha('0'), sha256_hex(work.as_bytes())),
            (sha256_hex(script.as_bytes()), sha('0')),
        ] {
            let bad = run(&script_sha, &work_sha);
            assert_eq!(bad.status.code(), Some(97), "{bad:?}");
            assert!(!String::from_utf8_lossy(&bad.stdout).contains("RAN"));
        }
    }
}
