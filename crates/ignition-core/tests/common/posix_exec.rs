//! Running the POSIX host scripts on a machine whose `stat` is not GNU's.
//!
//! The Phase 25 `.git` probe is a GNU `stat -c %F` script (it is what the
//! Linux gateway hosts run), so on a BSD developer machine the probe needs a
//! `stat` that speaks `-c %F`. [`shimmed`] wraps a `sh` invocation in `env
//! PATH=<shim dir>:$PATH` with a one-file shim that answers `stat -c %F -- P`
//! through the host's own `stat -f %HT -- P`, so the real error texts
//! (`No such file or directory`, `Permission denied`) still come from the
//! operating system. On a host whose `stat` already speaks `-c %F` (Linux) the
//! arguments are returned unchanged.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::sync::OnceLock;

const SHIM: &str = "#!/bin/sh\n\
    [ \"$1\" = \"-c\" ] && [ \"$2\" = \"%F\" ] && [ \"$3\" = \"--\" ] || exec /usr/bin/stat \"$@\"\n\
    o=$(/usr/bin/stat -f %HT -- \"$4\" 2>&1) || { printf '%s\\n' \"$o\" >&2; exit 1; }\n\
    case \"$o\" in\n\
    Directory) echo directory;;\n\
    'Regular File') if [ -s \"$4\" ]; then echo 'regular file'; else echo 'regular empty file'; fi;;\n\
    'Symbolic Link') echo 'symbolic link';;\n\
    *) echo \"$o\";;\n\
    esac\n";

/// The directory holding the `stat` shim, or `None` when this host's `stat`
/// already answers `-c %F`.
fn shim_dir() -> Option<&'static PathBuf> {
    static DIR: OnceLock<Option<(tempfile::TempDir, PathBuf)>> = OnceLock::new();
    DIR.get_or_init(|| {
        let gnu = std::process::Command::new("stat")
            .args(["-c", "%F", "--", "/"])
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false);
        if gnu || !cfg!(unix) {
            return None;
        }
        let dir = tempfile::tempdir().expect("shim dir");
        let path = dir.path().join("stat");
        std::fs::write(&path, SHIM).expect("shim");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        let shown = dir.path().to_path_buf();
        Some((dir, shown))
    })
    .as_ref()
    .map(|(_, path)| path)
}

/// `program` and `args` as they should run: unchanged for anything but `sh`,
/// and for `sh` wrapped in `env PATH=...` when the host needs the shim.
pub fn shimmed(program: &OsStr, args: &[OsString]) -> (OsString, Vec<OsString>) {
    match shim_dir() {
        Some(dir) if program == "sh" => {
            let path = format!(
                "PATH={}:{}",
                dir.display(),
                std::env::var("PATH").unwrap_or_default()
            );
            let mut wrapped = vec![OsString::from(path), OsString::from("sh")];
            wrapped.extend(args.iter().cloned());
            (OsString::from("env"), wrapped)
        }
        _ => (program.to_os_string(), args.to_vec()),
    }
}
