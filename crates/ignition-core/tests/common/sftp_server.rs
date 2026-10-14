//! Discovery of an OpenSSH `sftp-server` binary for the real-protocol tests
//! (22-06). The tests drive `sftp -D <server>`, which speaks the actual sftp
//! protocol to a local server process without an sshd.

use std::path::PathBuf;

/// Where OpenSSH installs `sftp-server` on the supported runner platforms.
pub const SFTP_SERVER_PROBES: [&str; 4] = [
    // macOS
    "/usr/libexec/sftp-server",
    // Debian and Ubuntu (openssh-sftp-server)
    "/usr/lib/openssh/sftp-server",
    // Arch and some BSDs
    "/usr/lib/ssh/sftp-server",
    // Fedora and RHEL (openssh-server)
    "/usr/libexec/openssh/sftp-server",
];

/// The first `sftp-server` that exists. Panics when none does, naming every
/// probed path and the package to install: a runner without one must fail the
/// real-protocol tests loudly, never skip them.
pub fn sftp_server_path() -> PathBuf {
    SFTP_SERVER_PROBES
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists())
        .unwrap_or_else(|| {
            panic!(
                "no OpenSSH sftp-server found (probed {}); install openssh-sftp-server \
                 (Debian/Ubuntu: `apt-get install openssh-sftp-server`) so the real-protocol \
                 sftp tests can run. These tests are never skipped.",
                SFTP_SERVER_PROBES.join(", ")
            )
        })
}
