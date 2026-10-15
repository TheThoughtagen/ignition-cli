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
//! - [`MOVE_REPLACE`]: `Move-Item -Force`, the documented fallback for
//!   replacing a file when sftp `rename` does not (C7a-6). On the captured host
//!   `rename` over an existing file replaces it (C7a-4), so the Windows commit
//!   uses `rename`; this script is the proven alternative, exercised live.
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
pub const MOVE_REPLACE: &str = "Move-Item -LiteralPath $p.src -Destination $p.dst -Force;E @{ok=1}";

/// Walk each `$p.r[i]` (with the prefix `$p.x` in front) and report every
/// entry as `[relative path, is_dir, size, is_reparse_point, absolute path
/// length]`. A reparse point is reported and not descended. A missing root is
/// `x=0`. An unreadable directory is a terminating error.
pub const ENUMERATE: &str = "$o=@();for($i=0;$i -lt $p.r.Count;$i++){$b=$p.x+$p.r[$i];$e=@();$x=[IO.Directory]::Exists($b);if($x){$l=$b.Length+1;$s=New-Object Collections.Stack;$s.Push($b);while($s.Count){foreach($f in ([IO.DirectoryInfo]$s.Pop()).EnumerateFileSystemInfos()){$a=[int]$f.Attributes;$d=[int](($a -band 16) -ne 0);$r=[int](($a -band 1024) -ne 0);$z=0;if(!$d){$z=$f.Length};$n=$f.FullName;$e+=,@($n.Substring($l),$d,$z,$r,$n.Length);if($d -and !$r){$s.Push($n)}}}};$o+=@{i=$i;x=[int]$x;e=$e}};E @{ok=1;res=$o}";

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

    /// The script that walks `roots` with `prefix` in front of each
    /// ([`ENUMERATE`]). `prefix` is [`LONG_PATH_PREFIX`] on a Windows host.
    pub fn enumerate(prefix: &str, roots: &[String]) -> WinScript {
        WinScript {
            name: "enumerate",
            body: ENUMERATE,
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
}
