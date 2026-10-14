//! The remote shell under the ssh transport (22-05).
//!
//! Pure path and stderr hygiene helpers live here first; the `RemoteShell`
//! argv builder follows in the same file.

use std::path::Path;

/// Lines of ssh or sftp stderr kept in an error detail.
pub const STDERR_TAIL_LINES: usize = 5;

/// Why `path` cannot be placed in an sftp batch line, or `Ok` when it can.
pub fn safe_remote_path(_path: &str) -> Result<(), String> {
    Ok(())
}

/// A bounded, scrubbed tail of `stderr` fit for an error detail.
pub fn scrub_stderr(_stderr: &[u8], _temp_dir: Option<&Path>) -> String {
    String::new()
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
