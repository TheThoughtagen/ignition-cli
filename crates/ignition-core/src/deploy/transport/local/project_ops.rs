//! `impl ProjectHost for LocalFsTransport` (Phase 25, 25-07): the host half of
//! `deploy projects` on a data directory on the runner's own filesystem.
//!
//! Layout below `data_dir`:
//!
//! ```text
//! projects/<project>/.git                                   probed, never opened
//! ign-deploy/<gateway>/projects/<project>/release.json      the release record
//! ```
//!
//! The reads run on a blocking thread and inspect every level with
//! `symlink_metadata`, so nothing is followed. See [`ProjectHost`] for the
//! fail-closed rule: only a clean "no such entry" is absent.
//!
//! The one write is the release record ([`ReleaseRecordWriter`], 25-11): a
//! private temporary sibling renamed over `release.json` through a directory
//! handle that refuses a symlink at every level, exactly as the local ledger
//! is replaced.

use super::LocalFsTransport;
use crate::deploy::component_problem;
use crate::deploy::ledger::{
    LedgerRoot, RELEASE_RECORD_MAX_BYTES, release_record_location, release_record_rel,
};
use crate::deploy::projects::ReleaseRecord;
use crate::deploy::transport::local_fs::{Dir, FsError};
use crate::deploy::transport::projects::{
    GitEntry, ProjectHost, ReleaseRecordWriter, git_entry_location,
};
use crate::error::CoreError;

impl LocalFsTransport {
    /// What `<data_dir>/projects/<project>/.git` is, probed with
    /// `symlink_metadata` at every level (nothing is followed).
    fn git_entry_blocking(&self, project: &str) -> Result<GitEntry, CoreError> {
        if let Some(why) = component_problem(project) {
            return Err(self.fail(format!("the project name {why}")));
        }
        // An unreadable host is not an absent `.git`.
        self.require_data_dir()?;

        let projects = self.data_dir.join("projects");
        match std::fs::symlink_metadata(&projects) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(self.fail("projects is a symlink; symlinks are refused"));
            }
            Ok(m) if !m.file_type().is_dir() => {
                return Err(self.fail("projects is not a directory"));
            }
            Ok(_) => {}
            Err(e) => {
                return Err(self.fail(format!(
                    "projects cannot be inspected ({}); is data_dir the gateway data directory?",
                    e.kind()
                )));
            }
        }

        let project_dir = projects.join(project);
        match std::fs::symlink_metadata(&project_dir) {
            // No project directory: nothing to be git-managed (the `create`
            // case).
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(GitEntry::Absent),
            Err(e) => {
                return Err(self.fail(format!(
                    "projects/{project} cannot be inspected ({})",
                    e.kind()
                )));
            }
            Ok(m) if m.file_type().is_symlink() => {
                return Err(self.fail(format!(
                    "projects/{project} is a symlink; symlinks are refused"
                )));
            }
            Ok(m) if !m.file_type().is_dir() => {
                return Err(self.fail(format!("projects/{project} is not a directory")));
            }
            Ok(_) => {}
        }

        let location = git_entry_location(project);
        match std::fs::symlink_metadata(project_dir.join(".git")) {
            Ok(m) => {
                let kind = m.file_type();
                Ok(if kind.is_symlink() {
                    GitEntry::Symlink
                } else if kind.is_dir() {
                    GitEntry::Directory
                } else if kind.is_file() {
                    GitEntry::File
                } else {
                    GitEntry::Other
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(GitEntry::Absent),
            // Denied, I/O error and everything else: unknown, not absent.
            Err(e) => Err(self.fail(format!("{location} cannot be inspected ({})", e.kind()))),
        }
    }

    /// The release-record bytes, or `None` when any level of its path is
    /// absent. A refusal is `deploy_run_invalid` naming the location.
    fn read_release_record_blocking(
        &self,
        gateway: &str,
        project: &str,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        let location = release_record_location(LedgerRoot::DataDir, gateway, project);
        let bad = |problem: String| CoreError::DeployRunInvalid {
            run_id: "unknown".to_string(),
            problem: format!("{location}: {problem}"),
        };
        for (kind, value) in [("gateway", gateway), ("project", project)] {
            if let Some(why) = component_problem(value) {
                return Err(bad(format!("the {kind} name {why}")));
            }
        }
        self.read_regular_blocking(
            &release_record_rel(gateway, project),
            RELEASE_RECORD_MAX_BYTES,
            "the release record",
            (gateway, project),
            &bad,
        )
    }
}

impl LocalFsTransport {
    /// Replace the release record of `project` on `gateway` with `bytes`.
    ///
    /// The names get the check the read applies, and `bytes` must be a valid
    /// record for them, so a wrong caller cannot put anything else on the
    /// host. A symlink at any level is refused. The record is written to a
    /// private temporary sibling, flushed and renamed over the old one.
    fn write_release_record_blocking(
        &self,
        gateway: &str,
        project: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError> {
        let location = release_record_location(LedgerRoot::DataDir, gateway, project);
        let bad = |problem: String| CoreError::DeployRunInvalid {
            run_id: "unknown".to_string(),
            problem: format!("{location}: {problem}"),
        };
        for (kind, value) in [("gateway", gateway), ("project", project)] {
            if let Some(why) = component_problem(value) {
                return Err(bad(format!("the {kind} name {why}")));
            }
        }
        // The same parse the read applies: a record that would be refused on
        // the way back is never put there.
        ReleaseRecord::parse(bytes, &location, gateway, project)?;

        // An unwritable host is an error, not a skipped record.
        self.require_data_dir()?;
        let failed = |e: FsError| match e {
            FsError::Changed => bad(
                "a path component is, or became, a symlink or a non-directory; refused".to_string(),
            ),
            other => self.mirror_fail(
                "write",
                format!("{location} cannot be written: {}", other.describe()),
            ),
        };
        let root = Dir::root(&self.data_dir).map_err(failed)?;
        let rel = release_record_rel(gateway, project);
        let parts: Vec<&str> = rel.split('/').collect();
        root.replace(&parts, bytes).map_err(failed)
    }
}

#[async_trait::async_trait]
impl ReleaseRecordWriter for LocalFsTransport {
    async fn write_release_record(
        &self,
        gateway: &str,
        project: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError> {
        let this = self.clone();
        let (gateway, project, bytes) = (gateway.to_string(), project.to_string(), bytes.to_vec());
        tokio::task::spawn_blocking(move || {
            this.write_release_record_blocking(&gateway, &project, &bytes)
        })
        .await
        .map_err(|e| CoreError::Internal(format!("local write task failed: {e}")))?
    }
}

#[async_trait::async_trait]
impl ProjectHost for LocalFsTransport {
    async fn git_entry(&self, project: &str) -> Result<GitEntry, CoreError> {
        let this = self.clone();
        let project = project.to_string();
        tokio::task::spawn_blocking(move || this.git_entry_blocking(&project))
            .await
            .map_err(|e| CoreError::Internal(format!("local read task failed: {e}")))?
    }

    async fn read_release_record(
        &self,
        gateway: &str,
        project: &str,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        let this = self.clone();
        let (gateway, project) = (gateway.to_string(), project.to_string());
        tokio::task::spawn_blocking(move || this.read_release_record_blocking(&gateway, &project))
            .await
            .map_err(|e| CoreError::Internal(format!("local read task failed: {e}")))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn host(data: &Path) -> LocalFsTransport {
        LocalFsTransport::new(data.to_path_buf(), "nodes.master")
    }

    fn data_with_projects() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("projects")).expect("projects");
        dir
    }

    #[test]
    fn local_git_entry_kinds() {
        let dir = data_with_projects();
        let projects = dir.path().join("projects");
        let t = host(dir.path());

        // No project directory: absent.
        assert_eq!(
            t.git_entry_blocking("Nope").expect("absent"),
            GitEntry::Absent
        );
        // A project directory without .git: absent.
        std::fs::create_dir_all(projects.join("Plain")).expect("plain");
        assert_eq!(
            t.git_entry_blocking("Plain").expect("plain"),
            GitEntry::Absent
        );
        // .git as a directory.
        std::fs::create_dir_all(projects.join("Dir/.git")).expect("dir");
        assert_eq!(
            t.git_entry_blocking("Dir").expect("dir"),
            GitEntry::Directory
        );
        // .git as a file (a worktree or submodule pointer).
        std::fs::create_dir_all(projects.join("File")).expect("file dir");
        std::fs::write(projects.join("File/.git"), b"gitdir: ../x").expect("file");
        assert_eq!(t.git_entry_blocking("File").expect("file"), GitEntry::File);
        // .git as a symlink, dangling or not: reported, never followed.
        #[cfg(unix)]
        {
            std::fs::create_dir_all(projects.join("Link")).expect("link dir");
            std::os::unix::fs::symlink("/nonexistent/target", projects.join("Link/.git"))
                .expect("symlink");
            assert_eq!(
                t.git_entry_blocking("Link").expect("link"),
                GitEntry::Symlink
            );
        }
    }

    #[test]
    fn local_git_entry_missing_projects_dir_is_transport_failed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = host(dir.path())
            .git_entry_blocking("CapApp")
            .expect_err("no data/projects");
        assert_eq!(err.code(), "transport_failed");
        assert!(err.to_string().contains("projects"), "{err}");

        // A data_dir that does not exist at all fails the same way.
        let err = host(&dir.path().join("gone"))
            .git_entry_blocking("CapApp")
            .expect_err("no data_dir");
        assert_eq!(err.code(), "transport_failed");
    }

    #[test]
    fn local_git_entry_refuses_a_project_that_is_not_a_directory() {
        let dir = data_with_projects();
        std::fs::write(dir.path().join("projects/CapApp"), b"x").expect("file");
        let err = host(dir.path())
            .git_entry_blocking("CapApp")
            .expect_err("a file where the project directory belongs");
        assert_eq!(err.code(), "transport_failed");
    }

    #[test]
    fn local_git_entry_refuses_unsafe_project_names() {
        let dir = data_with_projects();
        for bad in ["", ".", "..", "a/b", "a\\b", ".git"] {
            let err = host(dir.path())
                .git_entry_blocking(bad)
                .expect_err("unsafe name");
            assert_eq!(err.code(), "transport_failed", "{bad:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn local_git_entry_unreadable_project_dir_is_transport_failed_not_absent() {
        use std::os::unix::fs::PermissionsExt;
        let dir = data_with_projects();
        let project = dir.path().join("projects/CapApp");
        std::fs::create_dir_all(project.join(".git")).expect("git dir");
        std::fs::set_permissions(&project, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        // A privileged user (root) ignores the mode; the probe cannot be
        // denied there, and the assertion below would not hold.
        let privileged = std::fs::read_dir(&project).is_ok();
        let result = host(dir.path()).git_entry_blocking("CapApp");
        std::fs::set_permissions(&project, std::fs::Permissions::from_mode(0o755))
            .expect("restore");
        if privileged {
            assert_eq!(result.expect("privileged probe"), GitEntry::Directory);
        } else {
            let err = result.expect_err("a denied probe is not an answer");
            assert_eq!(err.code(), "transport_failed");
            assert!(err.to_string().contains("projects/CapApp/.git"), "{err}");
        }
    }

    fn write_record(data: &Path, gateway: &str, project: &str, bytes: &[u8]) {
        let path = data.join(release_record_rel(gateway, project));
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, bytes).expect("write");
    }

    #[test]
    fn local_release_record_read() {
        let dir = tempfile::tempdir().expect("tempdir");
        let t = host(dir.path());
        let read = || t.read_release_record_blocking("gw", "CapApp");

        // Absent at every level.
        assert_eq!(read().expect("no ign-deploy"), None);
        std::fs::create_dir_all(dir.path().join("ign-deploy/gw/projects")).expect("mkdir");
        assert_eq!(read().expect("no project dir"), None);
        std::fs::create_dir_all(dir.path().join("ign-deploy/gw/projects/CapApp")).expect("mkdir");
        assert_eq!(read().expect("no file"), None);

        // A regular file returns its bytes.
        write_record(dir.path(), "gw", "CapApp", b"{\"schema_version\":1}");
        assert_eq!(
            read().expect("file"),
            Some(b"{\"schema_version\":1}".to_vec())
        );

        // Over the cap.
        write_record(
            dir.path(),
            "gw",
            "CapApp",
            &vec![b'x'; RELEASE_RECORD_MAX_BYTES + 1],
        );
        let err = read().expect_err("over the cap");
        assert_eq!(err.code(), "deploy_run_invalid");
        assert!(
            err.to_string()
                .contains("ign-deploy/gw/projects/CapApp/release.json"),
            "{err}"
        );

        // A directory where the file belongs.
        let file = dir.path().join(release_record_rel("gw", "CapApp"));
        std::fs::remove_file(&file).expect("rm");
        std::fs::create_dir(&file).expect("dir");
        let err = read().expect_err("a directory");
        assert_eq!(err.code(), "deploy_run_invalid");
    }

    #[cfg(unix)]
    #[test]
    fn local_release_record_refuses_symlinks() {
        let dir = tempfile::tempdir().expect("tempdir");
        let t = host(dir.path());
        let outside = dir.path().join("outside.json");
        std::fs::write(&outside, b"{}").expect("outside");
        let file = dir.path().join(release_record_rel("gw", "CapApp"));
        std::fs::create_dir_all(file.parent().expect("parent")).expect("mkdir");
        std::os::unix::fs::symlink(&outside, &file).expect("symlink");
        let err = t
            .read_release_record_blocking("gw", "CapApp")
            .expect_err("a symlinked record");
        assert_eq!(err.code(), "deploy_run_invalid");

        // A symlinked directory above the record is refused the same way.
        std::fs::remove_file(&file).expect("rm");
        let real = dir.path().join("elsewhere");
        std::fs::create_dir_all(&real).expect("real");
        std::fs::remove_dir(file.parent().expect("parent")).expect("rmdir");
        std::os::unix::fs::symlink(&real, file.parent().expect("parent")).expect("dir link");
        let err = t
            .read_release_record_blocking("gw", "CapApp")
            .expect_err("a symlinked directory");
        assert_eq!(err.code(), "deploy_run_invalid");
    }

    #[test]
    fn local_release_record_with_an_unreadable_host_is_not_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = host(&dir.path().join("gone"))
            .read_release_record_blocking("gw", "CapApp")
            .expect_err("no data_dir");
        assert_eq!(err.code(), "transport_failed");
    }

    #[test]
    fn local_release_record_refuses_unsafe_names() {
        let dir = tempfile::tempdir().expect("tempdir");
        for (gateway, project) in [
            ("..", "CapApp"),
            ("gw", ".."),
            ("gw", "a/b"),
            ("", "CapApp"),
        ] {
            let err = host(dir.path())
                .read_release_record_blocking(gateway, project)
                .expect_err("unsafe name");
            assert_eq!(err.code(), "deploy_run_invalid", "{gateway:?} {project:?}");
        }
    }

    fn record_bytes(gateway: &str, project: &str, release: &str) -> Vec<u8> {
        ReleaseRecord {
            schema_version: 1,
            gateway: gateway.to_string(),
            project: project.to_string(),
            release: release.to_string(),
            artifact_sha256: "a".repeat(64),
            run_id: "20261020T101500Z-ab12".to_string(),
            deployed_at: "2026-10-20T10:15:00Z".to_string(),
        }
        .to_bytes()
    }

    /// V5: the record is written below the ledger root, read back by the
    /// reader that gates the next deploy, and replaced by the next one. No
    /// temporary file is left behind.
    #[test]
    fn local_release_record_write_round_trips_and_replaces() {
        let dir = tempfile::tempdir().expect("tempdir");
        let t = host(dir.path());
        let first = record_bytes("gw", "CapApp", "v1.0.0");
        t.write_release_record_blocking("gw", "CapApp", &first)
            .expect("write");
        assert_eq!(
            t.read_release_record_blocking("gw", "CapApp")
                .expect("read"),
            Some(first)
        );
        let second = record_bytes("gw", "CapApp", "v1.1.0");
        t.write_release_record_blocking("gw", "CapApp", &second)
            .expect("replace");
        assert_eq!(
            t.read_release_record_blocking("gw", "CapApp")
                .expect("read"),
            Some(second)
        );
        let folder = dir.path().join("ign-deploy/gw/projects/CapApp");
        let names: Vec<String> = std::fs::read_dir(&folder)
            .expect("folder")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["release.json"], "no temporary file is left");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(folder.join("release.json"))
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    /// The writer applies the reader's name check and refuses bytes that are
    /// not a record for those names; nothing is created for a refusal.
    #[test]
    fn local_release_record_write_fails_closed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let t = host(dir.path());
        let good = record_bytes("gw", "CapApp", "v1.0.0");
        for (gateway, project) in [
            ("..", "CapApp"),
            ("gw", ".."),
            ("gw", "a/b"),
            ("", "CapApp"),
            ("gw", ".git"),
        ] {
            let err = t
                .write_release_record_blocking(gateway, project, &good)
                .expect_err("unsafe name");
            assert_eq!(err.code(), "deploy_run_invalid", "{gateway:?} {project:?}");
        }
        // Not a record, a record for another project, an over-cap blob.
        for bytes in [
            b"{}".to_vec(),
            b"not json".to_vec(),
            record_bytes("gw", "Other", "v1.0.0"),
            vec![b'x'; RELEASE_RECORD_MAX_BYTES + 1],
        ] {
            let err = t
                .write_release_record_blocking("gw", "CapApp", &bytes)
                .expect_err("not this project's record");
            assert_eq!(err.code(), "deploy_run_invalid");
        }
        assert!(
            !dir.path().join("ign-deploy").exists(),
            "a refused write creates nothing"
        );
        // An unwritable host is an error, not a skipped record.
        let err = host(&dir.path().join("gone"))
            .write_release_record_blocking("gw", "CapApp", &good)
            .expect_err("no data_dir");
        assert_eq!(err.code(), "transport_failed");
    }

    /// A symlink on the record's path is never followed, at the file or at
    /// any directory above it.
    #[cfg(unix)]
    #[test]
    fn local_release_record_write_refuses_symlinks() {
        let dir = tempfile::tempdir().expect("tempdir");
        let t = host(dir.path());
        let bytes = record_bytes("gw", "CapApp", "v1.0.0");
        let outside = dir.path().join("outside.json");
        std::fs::write(&outside, b"keep").expect("outside");
        let file = dir.path().join(release_record_rel("gw", "CapApp"));
        std::fs::create_dir_all(file.parent().expect("parent")).expect("mkdir");
        std::os::unix::fs::symlink(&outside, &file).expect("symlink");
        let err = t
            .write_release_record_blocking("gw", "CapApp", &bytes)
            .expect_err("a symlinked record");
        assert_eq!(err.code(), "deploy_run_invalid");
        assert_eq!(std::fs::read(&outside).expect("untouched"), b"keep");

        std::fs::remove_file(&file).expect("rm");
        let real = dir.path().join("elsewhere");
        std::fs::create_dir_all(&real).expect("real");
        std::fs::remove_dir(file.parent().expect("parent")).expect("rmdir");
        std::os::unix::fs::symlink(&real, file.parent().expect("parent")).expect("dir link");
        let err = t
            .write_release_record_blocking("gw", "CapApp", &bytes)
            .expect_err("a symlinked directory");
        assert_eq!(err.code(), "deploy_run_invalid");
        assert_eq!(
            std::fs::read_dir(&real).expect("real").count(),
            0,
            "nothing was written through the link"
        );
    }

    #[tokio::test]
    async fn the_writer_works_through_the_trait() {
        let dir = tempfile::tempdir().expect("tempdir");
        let w: Box<dyn ReleaseRecordWriter> = Box::new(host(dir.path()));
        let bytes = record_bytes("gw", "CapApp", "v1.0.0");
        w.write_release_record("gw", "CapApp", &bytes)
            .await
            .expect("write");
        let t = host(dir.path());
        assert_eq!(
            t.read_release_record("gw", "CapApp").await.expect("read"),
            Some(bytes)
        );
    }

    #[tokio::test]
    async fn the_host_works_through_the_trait() {
        let dir = data_with_projects();
        std::fs::create_dir_all(dir.path().join("projects/CapApp/.git")).expect("git");
        let t: Box<dyn ProjectHost> = Box::new(host(dir.path()));
        assert_eq!(
            t.git_entry("CapApp").await.expect("probe"),
            GitEntry::Directory
        );
        assert_eq!(
            t.read_release_record("gw", "CapApp").await.expect("record"),
            None
        );
    }
}
