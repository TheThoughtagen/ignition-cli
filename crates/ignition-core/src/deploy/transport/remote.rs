//! The remote shell under the ssh transport (22-05).
//!
//! Pure path and stderr hygiene helpers live here first; the `RemoteShell`
//! argv builder follows in the same file.

use std::path::Path;

/// Lines of ssh or sftp stderr kept in an error detail.
pub const STDERR_TAIL_LINES: usize = 5;

/// An error detail never exceeds this many bytes.
const DETAIL_MAX_BYTES: usize = 1024;

/// Why `path` cannot be placed in an sftp batch line, or `Ok` when it can.
///
/// Remote paths reach sftp inside double quotes on a batch line, and the sftp
/// client globs what it is given, so these are refused outright:
///
/// | Refused | Why |
/// |---|---|
/// | empty | names the working directory, never a resource |
/// | control characters (newline, CR, tab, DEL, C1) | a newline ends the batch line and starts another command |
/// | `"` | closes the quoting |
/// | `\` | escapes inside the quoting (and is a separator on Windows hosts, normalised before this check) |
/// | `*` `?` `[` `]` `{` `}` | sftp glob characters: `ls "dir/odd[1]"` only worked because no `odd1` existed, a sibling would silently win |
///
/// Spaces, commas and other printable characters pass.
pub fn safe_remote_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("remote path must not be empty".to_string());
    }
    for c in path.chars() {
        let why = match c {
            c if c.is_control() => "a control character",
            '"' => "a double quote",
            '\\' => "a backslash",
            '*' | '?' | '[' | ']' | '{' | '}' => "an sftp glob character",
            _ => continue,
        };
        return Err(format!(
            "remote path {path:?} contains {why} ({c:?}), which cannot be placed safely in an sftp batch line"
        ));
    }
    Ok(())
}

/// Drop ANSI CSI sequences and every control character except `\n`.
fn strip_controls(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                // Parameter and intermediate bytes, then one final byte.
                for next in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&next) {
                        break;
                    }
                }
            }
            continue;
        }
        if c == '\n' || !c.is_control() {
            out.push(c);
        }
    }
    out
}

/// A bounded, scrubbed tail of `stderr` fit for an error detail.
///
/// Lossy UTF-8; ANSI sequences and control characters (newline excepted) are
/// removed; the native and forward-slash spellings of `temp_dir` become
/// `<tmp>`; only the last [`STDERR_TAIL_LINES`] lines are kept, and the
/// result is at most 1 KiB (cut from the front on a character boundary).
pub fn scrub_stderr(stderr: &[u8], temp_dir: Option<&Path>) -> String {
    let mut text = strip_controls(&String::from_utf8_lossy(stderr));
    if let Some(dir) = temp_dir {
        let native = dir.display().to_string();
        let slashed = native.replace('\\', "/");
        if !native.is_empty() {
            text = text.replace(&native, "<tmp>");
        }
        if !slashed.is_empty() && slashed != native {
            text = text.replace(&slashed, "<tmp>");
        }
    }
    let lines: Vec<&str> = text.lines().collect();
    let from = lines.len().saturating_sub(STDERR_TAIL_LINES);
    let mut out = lines[from..].join("\n");
    if out.len() > DETAIL_MAX_BYTES {
        let mut cut = out.len() - DETAIL_MAX_BYTES;
        while !out.is_char_boundary(cut) {
            cut += 1;
        }
        out.drain(..cut);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_remote_path_hostile_table() {
        for bad in [
            "", "a\nb", "a\rb", "a\tb", "a\"b", "a\\b", "a*b", "a?b", "a[1]", "a{b}", "a]b", "a}b",
        ] {
            assert!(safe_remote_path(bad).is_err(), "{bad:?} must be refused");
        }
        assert_eq!(
            safe_remote_path(
                "/opt/ignition/data/config/resources/production/ignition/database-connection/My DB,1"
            ),
            Ok(())
        );
    }

    #[test]
    fn scrub_stderr_tail() {
        let many: String = (1..=20).map(|i| format!("line {i}\n")).collect();
        let out = scrub_stderr(many.as_bytes(), None);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), STDERR_TAIL_LINES, "{out:?}");
        assert_eq!(lines[0], "line 16");
        assert_eq!(lines[STDERR_TAIL_LINES - 1], "line 20");

        let tmp = Path::new("/var/folders/xx/ign-ssh-AbC123");
        let text = format!(
            "Load key \"{0}/key\": invalid format\nWarning: {0}/known_hosts\n",
            tmp.display()
        );
        let out = scrub_stderr(text.as_bytes(), Some(tmp));
        assert!(!out.contains("ign-ssh-AbC123"), "{out}");
        assert!(out.contains("<tmp>/key"), "{out}");
        assert!(out.contains("<tmp>/known_hosts"), "{out}");

        let out = scrub_stderr(b"\x1b[31mred\x1b[0m\x07 bell\x00nul\r\nnext\n", None);
        assert!(out.chars().all(|c| c == '\n' || !c.is_control()), "{out:?}");
        assert!(out.contains("red") && out.contains("next"), "{out:?}");
        assert!(!out.contains("[31m"), "{out:?}");

        let big = "é".repeat(2000);
        let out = scrub_stderr(big.as_bytes(), None);
        assert!(out.len() <= 1024, "{}", out.len());
        assert!(!out.is_empty());
    }
}
