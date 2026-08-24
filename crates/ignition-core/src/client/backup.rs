//! Backup capability constants + query builders (04-04, RIG-04) — the
//! gwbk wire: **stream down, octet up** (04-RESEARCH §Backup
//! endpoints, 83-api postman primary).
//!
//! `GET /data/api/v1/backup?type=roaming` answers the portable `.gwbk`
//! byte stream (`Accept: application/octet-stream`); the download
//! rides the 03-02 `download_to_file` pipeline helper VERBATIM (the
//! ONE streaming body-consumption site — no `Vec<u8>` anywhere on the
//! down path; gwbks are tens of MB, Pitfall 2).
//!
//! `POST /data/api/v1/backup` is the RESTORE: the gwbk bytes as a RAW
//! `application/octet-stream` body — **NOT multipart** (the postman
//! collection's exact shape) — with the four scope params sent
//! EXPLICITLY (`restoreDisabled`, `disableTempProjectBackup`,
//! `renameEnabled`, `restoreLocal`): the server is the authority on
//! defaults, agents see what was sent. Restore is synchronous AND
//! blocks a gateway restart afterward (Pitfall 6), so BOTH directions
//! ride the 300 s per-request class — a short timeout kills mid-
//! restore into unknown state.
//!
//! Auth: 401 HTML unauthenticated (live-verified shape) — requires a
//! token like every `/data` route.

use std::time::Duration;

/// GET/POST path of the backup capability.
pub(crate) const BACKUP_PATH: &str = "/data/api/v1/backup";

/// The download path with its query — `type=roaming` = the PORTABLE
/// backup (cross-rig; `all` includes gateway-specific state). The
/// query rides the path string into `download_to_file`'s single
/// `path` parameter (the url join preserves it — no helper signature
/// churn for one fixed param).
pub(crate) const BACKUP_DOWNLOAD_PATH: &str = "/data/api/v1/backup?type=roaming";

/// The `Accept` header the download sends — the postman collection's
/// exact value (the server answers the gwbk bytes).
pub(crate) const BACKUP_ACCEPT: &str = "application/octet-stream";

/// Per-request class for BOTH backup directions (Pitfall 6): gwbk
/// generation is not instant, and a restore POST blocks while the
/// gateway restores — a short timeout kills mid-operation into
/// unknown state.
pub const BACKUP_TIMEOUT: Duration = Duration::from_secs(300);

/// The restore POST's query pairs — every scope param EXPLICIT
/// (all `false`: disabled restores and renames are the honest
/// round-trip defaults; agents see exactly what was sent, the server
/// stays the authority on what an omitted param would mean).
/// `newName` rides ONLY when `renameEnabled=true`, so it is absent
/// here by construction.
pub(crate) fn restore_query() -> [(String, String); 4] {
    [
        ("restoreDisabled".to_string(), "false".to_string()),
        ("disableTempProjectBackup".to_string(), "false".to_string()),
        ("renameEnabled".to_string(), "false".to_string()),
        ("restoreLocal".to_string(), "false".to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{BACKUP_DOWNLOAD_PATH, BACKUP_TIMEOUT, restore_query};

    /// Pitfall 6 pin: BOTH directions ride the 300 s per-request class
    /// (the same constant serves download and restore — one budget,
    /// one truth).
    #[test]
    fn backup_timeout_is_the_300s_class() {
        assert_eq!(BACKUP_TIMEOUT, Duration::from_secs(300));
    }

    /// The download query rides the path constant — pinned so the
    /// `type=roaming` half cannot silently drift out of the URL.
    #[test]
    fn download_path_carries_roaming_query() {
        assert_eq!(BACKUP_DOWNLOAD_PATH, "/data/api/v1/backup?type=roaming");
    }

    /// All four restore params, all explicit false, exactly once —
    /// the request-shape truth the wiremock pin asserts end-to-end.
    #[test]
    fn restore_query_is_four_explicit_falses() {
        let pairs = restore_query();
        assert_eq!(pairs.len(), 4);
        for (_, value) in &pairs {
            assert_eq!(value, "false", "every scope param is explicit false");
        }
        let names: Vec<&str> = pairs.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            [
                "restoreDisabled",
                "disableTempProjectBackup",
                "renameEnabled",
                "restoreLocal"
            ],
            "the postman param set, no newName (renameEnabled=false)"
        );
    }
}
