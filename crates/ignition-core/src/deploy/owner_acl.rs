//! Windows owner-only ACLs, granted by SID (G-25-10).
//!
//! A file or directory that holds a secret (the ssh key written from a
//! target's `key_env`, the apply's run directory) is restricted to the account
//! the `ign` process runs as. That account is named by its SID, not by the
//! user-name environment variable: a service account such as NETWORK SERVICE
//! has a machine-account name (`host$`) that `icacls` cannot map (error 1332),
//! while the `*<SID>` form of a grant needs no name lookup and so works for a
//! local user, a domain user, NETWORK SERVICE (`S-1-5-20`) and LocalSystem
//! (`S-1-5-18`).
//!
//! The SID is read from the current process with `whoami /user /fo csv /nh`.
//! Every failure refuses: there is no fallback to a name or to an unrestricted
//! file. The SID text is validated before it reaches `icacls`, so no `whoami`
//! output can add an argument or a second grant.
//!
//! The parsing and the argument building are pure and compiled for tests on
//! every OS; the process-spawning functions exist on Windows only.

use std::ffi::OsString;
use std::path::Path;

/// Longest slice of a `whoami` line an error may carry.
#[cfg(any(windows, test))]
const ERROR_EXCERPT_CHARS: usize = 120;

/// Whether `text` is exactly a SID: `S-1-` followed by 2 to 16 dash-separated
/// groups of 1 to 15 ASCII digits, and nothing else.
#[cfg(any(windows, test))]
pub(crate) fn is_sid(text: &str) -> bool {
    let Some(rest) = text.strip_prefix("S-1-") else {
        return false;
    };
    let groups: Vec<&str> = rest.split('-').collect();
    (2..=16).contains(&groups.len())
        && groups
            .iter()
            .all(|g| (1..=15).contains(&g.len()) && g.bytes().all(|b| b.is_ascii_digit()))
}

/// The SID in the output of `whoami /user /fo csv /nh`: one CSV line,
/// `"<account>","<SID>"`, CRLF-terminated, in the OEM code page.
///
/// The error text starts `whoami /user printed no SID` and carries at most the
/// first 120 characters of the line.
#[cfg(any(windows, test))]
pub(crate) fn sid_from_whoami_csv(stdout: &[u8]) -> Result<String, String> {
    let text = String::from_utf8_lossy(stdout);
    let line = text
        .lines()
        .map(|l| l.trim_matches(['\r', '\n']))
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    let refuse = || {
        let excerpt: String = line.chars().take(ERROR_EXCERPT_CHARS).collect();
        format!("whoami /user printed no SID (first line: {excerpt:?})")
    };
    if !line.starts_with('"') || !line.ends_with('"') {
        return Err(refuse());
    }
    let Some((_, sid_quoted)) = line.rsplit_once("\",\"") else {
        return Err(refuse());
    };
    let sid = sid_quoted.strip_suffix('"').unwrap_or(sid_quoted);
    if sid.contains('"') || !is_sid(sid) {
        return Err(refuse());
    }
    Ok(sid.to_string())
}

/// The `icacls` arguments that remove inheritance from `path` and grant
/// `rights` to `sid` alone: the path, `/inheritance:r`, `/grant:r` and one
/// `*<SID>:<rights>` argument. Refuses a `sid` that is not a SID.
#[cfg(any(windows, test))]
pub(crate) fn icacls_owner_only_args(
    path: &Path,
    sid: &str,
    rights: &str,
) -> Result<Vec<OsString>, String> {
    if !is_sid(sid) {
        return Err("refusing to grant access to a value that is not a SID".to_string());
    }
    Ok(vec![
        path.as_os_str().to_os_string(),
        OsString::from("/inheritance:r"),
        OsString::from("/grant:r"),
        OsString::from(format!("*{sid}:{rights}")),
    ])
}

/// The SID of the account this process runs as, read from `whoami.exe`.
#[cfg(windows)]
pub(crate) fn current_user_sid() -> Result<String, String> {
    let output = std::process::Command::new("whoami")
        .args(["/user", "/fo", "csv", "/nh"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("could not run whoami ({:?})", e.kind()))?;
    if !output.status.success() {
        return Err(format!("whoami /user exited with {}", output.status));
    }
    sid_from_whoami_csv(&output.stdout)
}

/// Remove inheritance from `path` and grant `rights` (an `icacls` rights
/// string such as `R` or `(OI)(CI)F`) to the current process's SID alone.
/// Every failure is an `Err`; the caller refuses.
#[cfg(windows)]
pub(crate) fn restrict_to_current_user(path: &Path, rights: &str) -> Result<(), String> {
    let sid = current_user_sid()?;
    let args = icacls_owner_only_args(path, &sid, rights)?;
    let status = std::process::Command::new("icacls")
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("could not run icacls ({:?})", e.kind()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "icacls could not grant {rights} to the current user (SID {sid}) only ({status})"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCAL: &str = "\"ign-win-836\\ign-runner\",\"S-1-5-21-1111-2222-3333-1001\"\r\n";
    const DOMAIN: &str = "\"whk\\svc-ign\",\"S-1-5-21-1111-2222-3333-1105\"\r\n";
    const NETWORK_SERVICE: &str = "\"nt authority\\network service\",\"S-1-5-20\"\r\n";
    const SYSTEM: &str = "\"nt authority\\system\",\"S-1-5-18\"\r\n";

    #[test]
    fn sid_from_whoami_csv_reads_every_account_kind() {
        for (line, want) in [
            (LOCAL, "S-1-5-21-1111-2222-3333-1001"),
            (DOMAIN, "S-1-5-21-1111-2222-3333-1105"),
            (NETWORK_SERVICE, "S-1-5-20"),
            (SYSTEM, "S-1-5-18"),
        ] {
            assert_eq!(sid_from_whoami_csv(line.as_bytes()).as_deref(), Ok(want));
        }
        assert_eq!(
            sid_from_whoami_csv(b"\"ign-win-836\\a,b\",\"S-1-5-21-1-2-3-1001\"\r\n").as_deref(),
            Ok("S-1-5-21-1-2-3-1001")
        );
        // A name field holding 0xE9 (OEM code page, not UTF-8).
        let mut oem = b"\"ign-win-836\\caf".to_vec();
        oem.push(0xE9);
        oem.extend_from_slice(b"\",\"S-1-5-21-1-2-3-1001\"\r\n");
        assert_eq!(
            sid_from_whoami_csv(&oem).as_deref(),
            Ok("S-1-5-21-1-2-3-1001")
        );
        // Leading blank lines are skipped.
        assert_eq!(
            sid_from_whoami_csv(format!("\r\n\r\n{SYSTEM}").as_bytes()).as_deref(),
            Ok("S-1-5-18")
        );
    }

    #[test]
    fn sid_from_whoami_csv_refuses_malformed_output() {
        let seventeen = format!("\"x\",\"S-1-{}\"", vec!["1"; 17].join("-"));
        let sixteen_digits = "\"x\",\"S-1-5-1234567890123456\"";
        let bad: Vec<&str> = vec![
            "",
            "\r\n",
            "INFO: Could not find any user.",
            "\"x\"",
            "x,S-1-5-20",
            "\"x\",\"S-1-\"",
            "\"x\",\"S-1-5-x\"",
            "\"x\",\"*S-1-5-20\"",
            "\"x\",\"S-1-5-20 \"",
            "\"x\",\"S-1-5-20\",\"extra\"",
            &seventeen,
            sixteen_digits,
            "\"x\",\"S-1-5-20",
        ];
        for text in bad {
            let err = sid_from_whoami_csv(text.as_bytes())
                .expect_err(&format!("{text:?} must be refused"));
            assert!(err.starts_with("whoami /user printed no SID"), "{err}");
        }
    }

    #[test]
    fn sid_from_whoami_csv_bounds_the_error_text() {
        let long = format!("INFO: {}", "z".repeat(5000));
        let err = sid_from_whoami_csv(long.as_bytes()).expect_err("refused");
        assert!(err.len() < 300, "{}", err.len());
    }

    #[test]
    fn is_sid_accepts_only_sids() {
        for ok in ["S-1-5-20", "S-1-5-18", "S-1-5-21-1-2-3-1001"] {
            assert!(is_sid(ok), "{ok}");
        }
        let sixteen = format!("S-1-{}", vec!["1"; 16].join("-"));
        assert!(is_sid(&sixteen));
        for bad in [
            "",
            "S-1-",
            "S-1-5",
            "s-1-5-20",
            "S-2-5-20",
            "S-1-5-20\n",
            "S-1-5--20",
        ] {
            assert!(!is_sid(bad), "{bad:?}");
        }
    }

    #[test]
    fn icacls_owner_only_args_grants_by_sid() {
        let args = icacls_owner_only_args(Path::new("C:\\t\\key"), "S-1-5-20", "R").expect("args");
        assert_eq!(
            args,
            vec![
                OsString::from("C:\\t\\key"),
                OsString::from("/inheritance:r"),
                OsString::from("/grant:r"),
                OsString::from("*S-1-5-20:R"),
            ]
        );
        let dir =
            icacls_owner_only_args(Path::new("C:\\t\\run"), "S-1-5-18", "(OI)(CI)F").expect("args");
        assert_eq!(dir[3], OsString::from("*S-1-5-18:(OI)(CI)F"));
        for bad in ["ignwin836$", "S-1-5-20;x", ""] {
            icacls_owner_only_args(Path::new("C:\\t\\key"), bad, "R")
                .expect_err(&format!("{bad:?} must be refused"));
        }
    }

    #[cfg(windows)]
    #[test]
    fn current_user_sid_is_a_sid() {
        let sid = current_user_sid().expect("whoami runs");
        assert!(is_sid(&sid), "{sid}");
    }

    #[cfg(windows)]
    #[test]
    fn restrict_to_current_user_leaves_one_grant() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("key");
        std::fs::write(&file, b"secret").expect("write");
        restrict_to_current_user(&file, "R").expect("restrict");
        let out = std::process::Command::new("icacls")
            .arg(&file)
            .output()
            .expect("icacls runs");
        let acl = String::from_utf8_lossy(&out.stdout).to_string();
        let me = std::process::Command::new("whoami")
            .output()
            .expect("whoami runs");
        let me = String::from_utf8_lossy(&me.stdout).trim().to_lowercase();
        // An elevated process (GitHub's windows-latest runner) gives a new
        // file explicit SYSTEM and Administrators entries that
        // /inheritance:r does not remove. Win32-OpenSSH accepts both on a
        // key file, so they are allowed here; any other principal is not.
        let shown = file.display().to_string();
        let mut mine = 0;
        for line in acl.lines().filter(|l| l.contains(":(")) {
            let line = line
                .trim_start()
                .strip_prefix(shown.as_str())
                .unwrap_or(line);
            let principal = line.trim().split(":(").next().unwrap_or("").to_lowercase();
            if principal == me {
                mine += 1;
            } else {
                assert!(
                    principal == "nt authority\\system" || principal == "builtin\\administrators",
                    "{principal} has access: {acl}"
                );
            }
        }
        assert_eq!(mine, 1, "{acl}");
        for broad in ["Everyone", "BUILTIN\\Users", "Authenticated Users"] {
            assert!(!acl.contains(broad), "{broad} still has access: {acl}");
        }
        assert_eq!(std::fs::read(&file).expect("still readable"), b"secret");
    }
}
