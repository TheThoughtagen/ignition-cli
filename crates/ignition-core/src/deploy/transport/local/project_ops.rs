//! `impl ProjectHost for LocalFsTransport` (Phase 25, 25-07): the host half of
//! `deploy projects` on a data directory on the runner's own filesystem.
//!
//! Layout below `data_dir`:
//!
//! ```text
//! projects/<project>/.git                                   probed, never opened
//! ign-deploy/<gateway>/projects/<project>/release.json      the release record
//! ign-deploy/<gateway>/tag-providers/<provider>/record.json the tag record (26-06)
//! ign-deploy/<gateway>/projects/<project>/journal.json      the run journal (26-12)
//! ign-deploy/<gateway>/projects/<project>/pre-image/<name>  the run pre-image (26-12)
//! ```
//!
//! The reads run on a blocking thread and inspect every level with
//! `symlink_metadata`, so nothing is followed. See [`ProjectHost`] for the
//! fail-closed rule: only a clean "no such entry" is absent.
//!
//! The writes are the release record ([`ReleaseRecordWriter`], 25-11) and
//! the tag record ([`TagRecordWriter`], 26-06), each one file: a private
//! temporary sibling renamed over the record through a directory handle that
//! refuses a symlink at every level, exactly as the local ledger is replaced.
//! Both go through [`LocalFsTransport::replace_record_blocking`], and both
//! reads through the shared capped regular-file read.
//!
//! 26-12 adds the project run files beside the release record
//! (`journal.json`, `pre-image/<fixed name>`, [`ProjectRunWriter`]) through
//! the same replace and the same read.

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::LocalFsTransport;
use crate::deploy::apply::run_store::RunId;
use crate::deploy::component_problem;
use crate::deploy::git_advance::{GitWrite, GitWriteName};
use crate::deploy::git_objects::{
    GIT_OBJECTS_LIST_MAX_ENTRIES, GitObjectEntry, GitObjectEntryKind, GitObjectFile, GitObjectList,
    check_object_rel,
};
use crate::deploy::git_state::{
    GIT_LOG_TAIL_MAX, GIT_LOOSE_REFS_MAX, GIT_MARKERS, GIT_PACKED_REFS_MAX, GIT_SMALL_FILE_MAX,
    GitFiles,
};
use crate::deploy::ledger::{
    GIT_SAVE_DIR, LedgerRoot, RELEASE_RECORD_FILE, RELEASE_RECORD_MAX_BYTES, git_save_location,
    git_save_rel, pre_image_location, pre_image_rel, project_journal_location, project_journal_rel,
    release_record_location, release_record_rel, tag_record_location, tag_record_rel,
};
use crate::deploy::project_run::{
    PRE_IMAGE_MAX_BYTES, PROJECT_JOURNAL_FILE, PROJECT_JOURNAL_MAX_BYTES, ProjectJournal,
};
use crate::deploy::projects::ReleaseRecord;
use crate::deploy::tag_record::{TAG_RECORD_FILE, TAG_RECORD_MAX_BYTES, TagRecord};
use crate::deploy::transport::local_fs::{Dir, FsError, Kind};
use crate::deploy::transport::projects::{
    GitEntry, GitHostRead, GitManifest, GitSize, GitStateWriter, ImportDir, ProjectHost,
    ProjectRunWriter, ReleaseRecordWriter, TagRecordWriter, check_pre_image_write,
    git_entry_location, git_probe_name, project_run_names_problem_on, tag_record_names_problem_on,
};
use crate::deploy::yamlio::YAML_MAX_BYTES;
use crate::error::CoreError;

/// What [`LocalFsTransport::require_save_chain`] found above a save's `repo`.
enum SaveChain {
    /// Every level is a real directory.
    Complete,
    /// Nothing at the named level.
    Absent(String),
}

impl LocalFsTransport {
    /// What `<data_dir>/projects/<project>/.git` is, probed with
    /// `symlink_metadata` at every level (nothing is followed).
    fn git_entry_blocking(&self, project: &str) -> Result<GitEntry, CoreError> {
        self.entry_blocking(project, ".git")
    }

    /// What `<data_dir>/projects/<project>/<entry>` is, probed with
    /// `symlink_metadata` at every level (nothing is followed). `entry` is a
    /// fixed name (`.git`, or an [`ImportDir`] name), never outside input.
    fn entry_blocking(&self, project: &str, entry: &str) -> Result<GitEntry, CoreError> {
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

        let location = format!("projects/{project}/{entry}");
        match std::fs::symlink_metadata(project_dir.join(entry)) {
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

    /// The tag-record bytes, or `None` when any level of its path is absent.
    /// A refusal is `deploy_run_invalid` naming the location.
    fn read_tag_record_blocking(
        &self,
        gateway: &str,
        provider: &str,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        let location = tag_record_location(LedgerRoot::DataDir, gateway, provider);
        let bad = |problem: String| CoreError::DeployRunInvalid {
            run_id: "unknown".to_string(),
            problem: format!("{location}: {problem}"),
        };
        if let Some(why) = tag_record_names_problem_on(gateway, provider, None) {
            return Err(bad(why));
        }
        self.read_regular_blocking(
            &tag_record_rel(gateway, provider),
            TAG_RECORD_MAX_BYTES,
            "the tag record",
            (gateway, provider),
            &bad,
        )
    }
}

/// A `deploy_run_invalid` naming `location`.
fn run_invalid(location: &str, problem: impl std::fmt::Display) -> CoreError {
    CoreError::DeployRunInvalid {
        run_id: "unknown".to_string(),
        problem: format!("{location}: {problem}"),
    }
}

impl LocalFsTransport {
    /// The project journal bytes (26-12), or `None` when any level of its
    /// path is absent.
    fn read_project_journal_blocking(
        &self,
        gateway: &str,
        project: &str,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        let location = project_journal_location(LedgerRoot::DataDir, gateway, project);
        let bad = |problem: String| run_invalid(&location, problem);
        if let Some(why) = project_run_names_problem_on(gateway, project, None, None) {
            return Err(bad(why));
        }
        self.read_regular_blocking(
            &project_journal_rel(gateway, project),
            PROJECT_JOURNAL_MAX_BYTES,
            "the journal",
            (gateway, project),
            &bad,
        )
    }

    /// One pre-image file (26-12), or `None` when any level of its path is
    /// absent.
    fn read_pre_image_blocking(
        &self,
        gateway: &str,
        project: &str,
        name: &str,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        let location = pre_image_location(LedgerRoot::DataDir, gateway, project, name);
        let bad = |problem: String| run_invalid(&location, problem);
        if let Some(why) = project_run_names_problem_on(gateway, project, Some(name), None) {
            return Err(bad(why));
        }
        self.read_regular_blocking(
            &pre_image_rel(gateway, project, name),
            PRE_IMAGE_MAX_BYTES,
            "the pre-image file",
            (gateway, project),
            &bad,
        )
    }

    /// Replace the project journal with `bytes`, which must be this gateway's
    /// and project's journal.
    fn write_project_journal_blocking(
        &self,
        gateway: &str,
        project: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError> {
        let location = project_journal_location(LedgerRoot::DataDir, gateway, project);
        if let Some(why) = project_run_names_problem_on(gateway, project, None, None) {
            return Err(run_invalid(&location, why));
        }
        let journal = ProjectJournal::parse(bytes, &location, gateway, project)?;
        let temp_name = format!("{PROJECT_JOURNAL_FILE}.tmp-{}", journal.run_id);
        self.replace_record_blocking(
            &location,
            &project_journal_rel(gateway, project),
            &temp_name,
            bytes,
        )
    }

    /// Replace the pre-image file `name` of run `run_id` with `bytes`: a
    /// fixed name only, at most [`PRE_IMAGE_MAX_BYTES`], and a `run.json`
    /// only when it is this run's set.
    fn write_pre_image_blocking(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError> {
        let location = pre_image_location(LedgerRoot::DataDir, gateway, project, name);
        check_pre_image_write(&location, gateway, project, run_id, name, bytes, None)?;
        self.replace_record_blocking(
            &location,
            &pre_image_rel(gateway, project, name),
            &format!("{name}.tmp-{run_id}"),
            bytes,
        )
    }

    /// Remove `journal.json` and nothing else; absent is a success, a
    /// symlink or anything but a regular file is refused and left.
    fn clear_project_journal_blocking(
        &self,
        gateway: &str,
        project: &str,
    ) -> Result<(), CoreError> {
        let location = project_journal_location(LedgerRoot::DataDir, gateway, project);
        if let Some(why) = project_run_names_problem_on(gateway, project, None, None) {
            return Err(run_invalid(&location, why));
        }
        self.require_data_dir()?;
        let failed = |e: FsError| match e {
            FsError::Changed => run_invalid(
                &location,
                "a path component is, or became, a symlink or a non-directory; refused",
            ),
            other => self.mirror_fail(
                "write",
                format!("{location} cannot be removed: {}", other.describe()),
            ),
        };
        let root = Dir::root(&self.data_dir).map_err(failed)?;
        let rel = project_journal_rel(gateway, project);
        let parts: Vec<&str> = rel.split('/').collect();
        match root.kind(&parts).map_err(failed)? {
            Kind::Missing => Ok(()),
            Kind::File => match root.unlink(&parts) {
                Ok(()) | Err(FsError::Missing) => Ok(()),
                Err(e) => Err(failed(e)),
            },
            Kind::Symlink => Err(run_invalid(
                &location,
                "the journal is a symlink; symlinks are refused",
            )),
            Kind::Dir | Kind::Other => {
                Err(run_invalid(&location, "the journal is not a regular file"))
            }
        }
    }
}

impl LocalFsTransport {
    /// Replace the release record of `project` on `gateway` with `bytes`.
    ///
    /// The names get the check the read applies, and `bytes` must be a valid
    /// record for them, so a wrong caller cannot put anything else on the
    /// host. A symlink at any level is refused. The record is written to a
    /// private temporary sibling named for the run, flushed and renamed over
    /// the old one; the sibling is removed again when a step fails, and a
    /// removal that fails is named in the error.
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
        let record = ReleaseRecord::parse(bytes, &location, gateway, project)?;
        // The temporary sibling is named for the run, as the ssh writer's is,
        // so two runs on one data directory never write the same file. The
        // run id is already held to `[A-Za-z0-9._-]{1,128}`.
        let temp_name = format!("{RELEASE_RECORD_FILE}.tmp-{}", record.run_id);
        self.replace_record_blocking(
            &location,
            &release_record_rel(gateway, project),
            &temp_name,
            bytes,
        )
    }

    /// Replace the tag record of `provider` on `gateway` with `bytes`, under
    /// the same rules as the release record: the names are checked, `bytes`
    /// must be this gateway's and provider's [`TagRecord`], and the file is
    /// replaced atomically through a temporary sibling named for the run.
    fn write_tag_record_blocking(
        &self,
        gateway: &str,
        provider: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError> {
        let location = tag_record_location(LedgerRoot::DataDir, gateway, provider);
        if let Some(why) = tag_record_names_problem_on(gateway, provider, None) {
            return Err(CoreError::DeployRunInvalid {
                run_id: "unknown".to_string(),
                problem: format!("{location}: {why}"),
            });
        }
        let record = TagRecord::parse(bytes, &location, gateway, provider)?;
        let temp_name = format!("{TAG_RECORD_FILE}.tmp-{}", record.run_id);
        self.replace_record_blocking(
            &location,
            &tag_record_rel(gateway, provider),
            &temp_name,
            bytes,
        )
    }

    /// Replace the record file at `rel` below `data_dir` with `bytes`: write
    /// the private temporary sibling `temp_name`, flush it and rename it over
    /// the record through a directory handle that refuses a symlink at every
    /// level. The sibling is removed again when a step fails. The callers have
    /// already checked the names and parsed `bytes`.
    fn replace_record_blocking(
        &self,
        location: &str,
        rel: &str,
        temp_name: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError> {
        // An unwritable host is an error, not a skipped record.
        self.require_data_dir()?;
        let failed = |e: FsError| match e {
            FsError::Changed => CoreError::DeployRunInvalid {
                run_id: "unknown".to_string(),
                problem: format!(
                    "{location}: a path component is, or became, a symlink or a non-directory; refused"
                ),
            },
            other => self.mirror_fail(
                "write",
                format!("{location} cannot be written: {}", other.describe()),
            ),
        };
        let root = Dir::root(&self.data_dir).map_err(failed)?;
        let parts: Vec<&str> = rel.split('/').collect();
        root.replace_via(&parts, temp_name, bytes).map_err(failed)
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
impl TagRecordWriter for LocalFsTransport {
    async fn write_tag_record(
        &self,
        gateway: &str,
        provider: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError> {
        let this = self.clone();
        let (gateway, provider, bytes) =
            (gateway.to_string(), provider.to_string(), bytes.to_vec());
        tokio::task::spawn_blocking(move || {
            this.write_tag_record_blocking(&gateway, &provider, &bytes)
        })
        .await
        .map_err(|e| CoreError::Internal(format!("local write task failed: {e}")))?
    }
}

#[async_trait::async_trait]
impl ProjectRunWriter for LocalFsTransport {
    async fn write_journal(
        &self,
        gateway: &str,
        project: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError> {
        let this = self.clone();
        let (gateway, project, bytes) = (gateway.to_string(), project.to_string(), bytes.to_vec());
        tokio::task::spawn_blocking(move || {
            this.write_project_journal_blocking(&gateway, &project, &bytes)
        })
        .await
        .map_err(|e| CoreError::Internal(format!("local write task failed: {e}")))?
    }

    async fn clear_journal(&self, gateway: &str, project: &str) -> Result<(), CoreError> {
        let this = self.clone();
        let (gateway, project) = (gateway.to_string(), project.to_string());
        tokio::task::spawn_blocking(move || this.clear_project_journal_blocking(&gateway, &project))
            .await
            .map_err(|e| CoreError::Internal(format!("local write task failed: {e}")))?
    }

    async fn write_pre_image(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError> {
        let this = self.clone();
        let (gateway, project, run_id, name, bytes) = (
            gateway.to_string(),
            project.to_string(),
            run_id.to_string(),
            name.to_string(),
            bytes.to_vec(),
        );
        tokio::task::spawn_blocking(move || {
            this.write_pre_image_blocking(&gateway, &project, &run_id, &name, &bytes)
        })
        .await
        .map_err(|e| CoreError::Internal(format!("local write task failed: {e}")))?
    }
}

#[async_trait::async_trait]
impl ProjectHost for LocalFsTransport {
    fn git(&self) -> Option<&dyn GitHostRead> {
        Some(self)
    }

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

    async fn read_tag_record(
        &self,
        gateway: &str,
        provider: &str,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        let this = self.clone();
        let (gateway, provider) = (gateway.to_string(), provider.to_string());
        tokio::task::spawn_blocking(move || this.read_tag_record_blocking(&gateway, &provider))
            .await
            .map_err(|e| CoreError::Internal(format!("local read task failed: {e}")))?
    }

    async fn read_journal(
        &self,
        gateway: &str,
        project: &str,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        let this = self.clone();
        let (gateway, project) = (gateway.to_string(), project.to_string());
        tokio::task::spawn_blocking(move || this.read_project_journal_blocking(&gateway, &project))
            .await
            .map_err(|e| CoreError::Internal(format!("local read task failed: {e}")))?
    }

    async fn read_pre_image(
        &self,
        gateway: &str,
        project: &str,
        name: &str,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        let this = self.clone();
        let (gateway, project, name) = (gateway.to_string(), project.to_string(), name.to_string());
        tokio::task::spawn_blocking(move || this.read_pre_image_blocking(&gateway, &project, &name))
            .await
            .map_err(|e| CoreError::Internal(format!("local read task failed: {e}")))?
    }
}

/// Most entries a `.git` walk visits before it refuses (a real module repo
/// holds a few thousand loose objects; this bounds a hostile one).
const GIT_WALK_MAX_ENTRIES: usize = 2_000_000;

/// One entry of a walked `.git`.
struct WalkEntry {
    /// `/`-separated path below the walked root.
    rel: String,
    /// The path on disk.
    path: PathBuf,
    /// A directory (otherwise a regular file).
    is_dir: bool,
    /// Size of a regular file.
    size: u64,
    /// The source's Unix mode bits (`0` where the platform has none).
    mode: u32,
}

/// The Unix mode bits of `meta`, `0` where the platform has none.
#[cfg(unix)]
fn mode_of(meta: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o777
}

#[cfg(not(unix))]
fn mode_of(_meta: &std::fs::Metadata) -> u32 {
    0
}

/// Create the directory `path` (one level) with `mode` on Unix.
fn create_dir_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(mode).create(path)
    }
    #[cfg(not(unix))]
    {
        let _ = mode;
        std::fs::DirBuilder::new().create(path)
    }
}

/// Give the directory `path` exactly `mode` (27-31, security review 3).
///
/// Linux has no `lchmod` and `fchmodat(AT_SYMLINK_NOFOLLOW)` is unsupported for
/// a chmod, so a chmod by path could land on whatever was swapped in after the
/// `mkdir`. The directory is therefore opened itself, with `O_NOFOLLOW |
/// O_DIRECTORY` (a symlink or a file at the name fails the open), `fstat`-ed on
/// the descriptor (it must be a directory owned by this account), and the mode
/// is set with an `fchmod` on that descriptor.
fn set_dir_mode_no_follow(path: &Path, mode: u32) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
        let flags = rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::DIRECTORY;
        let dir = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(flags.bits() as i32)
            .open(path)?;
        let meta = dir.metadata()?;
        if !meta.is_dir() || meta.uid() != current_uid()? {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "not a directory owned by this account",
            ));
        }
        dir.set_permissions(std::fs::Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
        Ok(())
    }
}

/// The effective uid of this process, read from the owner of an anonymous
/// temporary file this process just created (no libc call, no extra rustix
/// feature).
#[cfg(unix)]
fn current_uid() -> std::io::Result<u32> {
    use std::os::unix::fs::MetadataExt;
    static UID: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    if let Some(uid) = UID.get() {
        return Ok(*uid);
    }
    let uid = tempfile::tempfile()?.metadata()?.uid();
    Ok(*UID.get_or_init(|| uid))
}

/// `path` opened for reading without following a symlink at the final
/// component (Unix; elsewhere the caller's `symlink_metadata` checks stand).
fn open_no_follow(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    }
    options.open(path)
}

/// A file created exclusively (never over an existing name, never through a
/// symlink at the final component).
fn create_new_file(path: &Path, mode: u32) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(mode);
    }
    #[cfg(not(unix))]
    let _ = mode;
    options.open(path)
}

/// The file beside a save that records the original mode of every entry of
/// the saved `.git`: `<octal mode> TAB <path>` per line, `.` for the root. The
/// save itself is private (`0700` directories, owner-only files) so it cannot
/// leak a credential in `config`; the restore puts the recorded modes back.
const GIT_SAVE_MODES_FILE: &str = "modes";

/// Largest modes file a restore reads.
const GIT_SAVE_MODES_MAX: u64 = 64 * 1024 * 1024;

/// The modes a restore applies. An entry with no record, or no record at all
/// (a save from a host with no modes, or a missing or unreadable file), is
/// owner-only: `0700` directories, `0600` files. A restore never grants a bit
/// the original lacked.
struct ModeSet {
    modes: BTreeMap<String, u32>,
}

impl ModeSet {
    /// Owner-only for everything.
    fn owner_only() -> ModeSet {
        ModeSet {
            modes: BTreeMap::new(),
        }
    }

    /// Parse the text of a modes file; a malformed line is skipped (its entry
    /// falls back to owner-only).
    fn parse(text: &str) -> ModeSet {
        let mut modes = BTreeMap::new();
        for line in text.lines() {
            let Some((mode, rel)) = line.split_once('\t') else {
                continue;
            };
            match u32::from_str_radix(mode, 8) {
                Ok(mode) if mode <= 0o777 && !rel.is_empty() => {
                    modes.insert(rel.to_string(), mode);
                }
                _ => {}
            }
        }
        ModeSet { modes }
    }

    fn file(&self, rel: &str) -> u32 {
        self.modes.get(rel).copied().unwrap_or(0o600)
    }

    fn dir(&self, rel: &str) -> u32 {
        self.modes.get(rel).copied().unwrap_or(0o700)
    }
}

/// The modes file text of walked `entries` below a root of mode `root_mode`.
fn modes_text(root_mode: u32, entries: &[WalkEntry]) -> String {
    let mut text = format!("{root_mode:04o}\t.\n");
    for entry in entries {
        text.push_str(&format!("{:04o}\t{}\n", entry.mode & 0o777, entry.rel));
    }
    text
}

/// What a copy does with the mode of each file it writes.
enum CopyMode<'a> {
    /// Into a save: directories `0700`, files keep their owner bits only.
    Private,
    /// Out of a save: files get their recorded mode; directories are made
    /// `0700` while they are filled and given their recorded mode afterwards.
    Restore(&'a ModeSet),
}

impl CopyMode<'_> {
    /// The mode a directory is created with.
    fn dir_mode(&self) -> u32 {
        0o700
    }

    fn file_mode(&self, rel: &str, source: u32) -> u32 {
        match self {
            CopyMode::Private => {
                let owner = source & 0o700;
                if owner == 0 { 0o600 } else { owner }
            }
            CopyMode::Restore(set) => set.file(rel),
        }
    }
}

/// Set `path` to exactly `mode` (Unix; elsewhere there are no modes to set).
fn set_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
        Ok(())
    }
}

impl LocalFsTransport {
    fn git_fail(&self, op: &str, detail: impl std::fmt::Display) -> CoreError {
        self.mirror_fail(op, detail)
    }

    /// The names a save, restore or save removal takes: gateway, project and
    /// run id, each a safe component, before any path is built.
    fn git_save_names(&self, gateway: &str, project: &str, run_id: &str) -> Result<(), CoreError> {
        let location = git_save_location(LedgerRoot::DataDir, gateway, project, "<run-id>");
        if let Some(why) = project_run_names_problem_on(gateway, project, None, None) {
            return Err(run_invalid(&location, why));
        }
        if RunId::parse(run_id).is_err() {
            return Err(run_invalid(
                &location,
                "the run id is not 1 to 128 characters of [A-Za-z0-9._-]",
            ));
        }
        Ok(())
    }

    fn project_git_dir(&self, project: &str) -> PathBuf {
        self.data_dir.join("projects").join(project).join(".git")
    }

    /// Walk every level above `repo` in the save path
    /// (`ign-deploy/<gateway>/projects/<project>/git-save/<run>`), none of
    /// them followed: a symlink or an entry that is not a directory refuses,
    /// naming the level. A level with nothing at it answers
    /// [`SaveChain::Absent`]; the remove reads that as "nothing to remove",
    /// the restore and the manifest read as an error (WR-B6).
    fn require_save_chain(
        &self,
        op: &str,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<SaveChain, CoreError> {
        let mut path = self.data_dir.clone();
        let rel = git_save_rel(gateway, project, run_id);
        let parts: Vec<&str> = rel.split('/').collect();
        for part in &parts[..parts.len() - 1] {
            path.push(part);
            match std::fs::symlink_metadata(&path) {
                Ok(m) if m.file_type().is_symlink() => {
                    return Err(
                        self.git_fail(op, format!("{part} is a symlink; symlinks are refused"))
                    );
                }
                Ok(m) if !m.file_type().is_dir() => {
                    return Err(self.git_fail(op, format!("{part} is not a directory")));
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(SaveChain::Absent((*part).to_string()));
                }
                Err(e) => {
                    return Err(
                        self.git_fail(op, format!("{part} cannot be inspected ({})", e.kind()))
                    );
                }
            }
        }
        Ok(SaveChain::Complete)
    }

    /// [`Self::require_save_chain`] for a caller that needs the save to be
    /// there: an absent level is an error naming it.
    fn save_chain_must_exist(
        &self,
        op: &str,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<(), CoreError> {
        match self.require_save_chain(op, gateway, project, run_id)? {
            SaveChain::Complete => Ok(()),
            SaveChain::Absent(level) => Err(self.git_fail(
                op,
                format!("{level} does not exist; there is no save of run {run_id}"),
            )),
        }
    }

    fn git_save_repo(&self, gateway: &str, project: &str, run_id: &str) -> PathBuf {
        let mut path = self.data_dir.clone();
        for part in git_save_rel(gateway, project, run_id).split('/') {
            path.push(part);
        }
        path
    }

    /// Require `path` to be a real directory: not missing, not a symlink,
    /// not another kind of entry.
    fn require_real_dir(&self, op: &str, path: &Path, what: &str) -> Result<(), CoreError> {
        match std::fs::symlink_metadata(path) {
            Ok(m) if m.file_type().is_symlink() => {
                Err(self.git_fail(op, format!("{what} is a symlink; symlinks are refused")))
            }
            Ok(m) if !m.file_type().is_dir() => {
                Err(self.git_fail(op, format!("{what} is not a directory")))
            }
            Ok(_) => Ok(()),
            Err(e) => Err(self.git_fail(op, format!("{what} cannot be inspected ({})", e.kind()))),
        }
    }

    /// The live `.git` of `project` as a verified real directory: every level
    /// from `data_dir` to `.git` is checked and none is followed.
    fn live_git_dir(&self, op: &str, project: &str) -> Result<PathBuf, CoreError> {
        if let Some(why) = component_problem(project) {
            return Err(self.git_fail(op, format!("the project name {why}")));
        }
        self.require_data_dir()?;
        self.require_real_dir(op, &self.data_dir.join("projects"), "projects")?;
        self.require_real_dir(
            op,
            &self.data_dir.join("projects").join(project),
            &format!("projects/{project}"),
        )?;
        let git = self.project_git_dir(project);
        self.require_real_dir(op, &git, &format!("projects/{project}/.git"))?;
        Ok(git)
    }

    /// Walk `root` depth-first in name order. A symlink or any entry that is
    /// not a directory or a regular file refuses the walk.
    fn walk_git(&self, op: &str, root: &Path) -> Result<Vec<WalkEntry>, CoreError> {
        let mut out = Vec::new();
        self.walk_git_into(op, root, "", &mut out)?;
        Ok(out)
    }

    fn walk_git_into(
        &self,
        op: &str,
        dir: &Path,
        prefix: &str,
        out: &mut Vec<WalkEntry>,
    ) -> Result<(), CoreError> {
        let read = std::fs::read_dir(dir).map_err(|e| {
            self.git_fail(
                op,
                format!("{} cannot be listed ({})", shown(prefix), e.kind()),
            )
        })?;
        let mut names = Vec::new();
        for entry in read {
            let entry = entry.map_err(|e| {
                self.git_fail(
                    op,
                    format!("{} cannot be listed ({})", shown(prefix), e.kind()),
                )
            })?;
            names.push(entry.file_name());
        }
        names.sort();
        for name in names {
            let Some(text) = name.to_str() else {
                return Err(self.git_fail(
                    op,
                    format!("{} holds a name that is not UTF-8", shown(prefix)),
                ));
            };
            if text.contains('\n') {
                return Err(self.git_fail(
                    op,
                    format!("{} holds a name with a line break", shown(prefix)),
                ));
            }
            let rel = if prefix.is_empty() {
                text.to_string()
            } else {
                format!("{prefix}/{text}")
            };
            if out.len() >= GIT_WALK_MAX_ENTRIES {
                return Err(self.git_fail(
                    op,
                    format!("the repo holds more than {GIT_WALK_MAX_ENTRIES} entries"),
                ));
            }
            let path = dir.join(&name);
            let meta = std::fs::symlink_metadata(&path).map_err(|e| {
                self.git_fail(op, format!("{rel} cannot be inspected ({})", e.kind()))
            })?;
            let kind = meta.file_type();
            if kind.is_symlink() {
                return Err(self.git_fail(
                    op,
                    format!("{rel} is a symlink; a repo with a symlink is refused"),
                ));
            }
            if kind.is_dir() {
                out.push(WalkEntry {
                    rel: rel.clone(),
                    path: path.clone(),
                    is_dir: true,
                    size: 0,
                    mode: mode_of(&meta),
                });
                self.walk_git_into(op, &path, &rel, out)?;
            } else if kind.is_file() {
                out.push(WalkEntry {
                    rel,
                    path,
                    is_dir: false,
                    size: meta.len(),
                    mode: mode_of(&meta),
                });
            } else {
                return Err(self.git_fail(
                    op,
                    format!("{rel} is neither a directory nor a regular file"),
                ));
            }
        }
        Ok(())
    }

    /// The manifest of a walked tree.
    fn manifest_of(&self, op: &str, entries: &[WalkEntry]) -> Result<GitManifest, CoreError> {
        let mut files = 0usize;
        let mut bytes = 0u64;
        let mut digest = Sha256::new();
        // The byte order of the whole path, the order `LC_ALL=C sort` gives on
        // a POSIX host, so every transport computes the same digest.
        let mut sorted: Vec<&WalkEntry> = entries.iter().filter(|e| !e.is_dir).collect();
        sorted.sort_by(|a, b| a.rel.as_bytes().cmp(b.rel.as_bytes()));
        for entry in sorted {
            files += 1;
            bytes += entry.size;
            digest.update(entry.rel.as_bytes());
            digest.update([0u8]);
            if entry.rel.starts_with("objects/") {
                digest.update(entry.size.to_string().as_bytes());
            } else {
                let mut file = std::fs::File::open(&entry.path).map_err(|e| {
                    self.git_fail(op, format!("{} cannot be read ({})", entry.rel, e.kind()))
                })?;
                let mut hasher = Sha256::new();
                let mut buf = [0u8; 65_536];
                loop {
                    let n = file.read(&mut buf).map_err(|e| {
                        self.git_fail(op, format!("{} cannot be read ({})", entry.rel, e.kind()))
                    })?;
                    if n == 0 {
                        break;
                    }
                    hasher.update(&buf[..n]);
                }
                digest.update(format!("{:x}", hasher.finalize()).as_bytes());
            }
            digest.update(*b"\n");
        }
        Ok(GitManifest {
            files,
            bytes,
            digest: format!("{:x}", digest.finalize()),
        })
    }

    fn manifest_blocking(&self, op: &str, root: &Path) -> Result<GitManifest, CoreError> {
        let entries = self.walk_git(op, root)?;
        self.manifest_of(op, &entries)
    }

    /// Copy the walked `entries` of `from` below the (already created) `to`.
    fn copy_git(
        &self,
        op: &str,
        entries: &[WalkEntry],
        to: &Path,
        mode: &CopyMode<'_>,
    ) -> Result<(), CoreError> {
        for entry in entries {
            let target = to.join(&entry.rel);
            if entry.is_dir {
                create_dir_mode(&target, mode.dir_mode()).map_err(|e| {
                    self.git_fail(
                        op,
                        format!("{} cannot be created ({})", entry.rel, e.kind()),
                    )
                })?;
                continue;
            }
            let meta = std::fs::symlink_metadata(&entry.path).map_err(|e| {
                self.git_fail(
                    op,
                    format!("{} cannot be inspected ({})", entry.rel, e.kind()),
                )
            })?;
            if !meta.file_type().is_file() {
                return Err(self.git_fail(
                    op,
                    format!("{} changed while it was copied; refused", entry.rel),
                ));
            }
            let mut source = std::fs::File::open(&entry.path).map_err(|e| {
                self.git_fail(op, format!("{} cannot be read ({})", entry.rel, e.kind()))
            })?;
            let mut dest = create_new_file(&target, mode.file_mode(&entry.rel, mode_of(&meta)))
                .map_err(|e| {
                    self.git_fail(
                        op,
                        format!("{} cannot be created ({})", entry.rel, e.kind()),
                    )
                })?;
            std::io::copy(&mut source, &mut dest).map_err(|e| {
                self.git_fail(op, format!("{} cannot be copied ({})", entry.rel, e.kind()))
            })?;
            dest.sync_all().map_err(|e| {
                self.git_fail(
                    op,
                    format!("{} cannot be flushed ({})", entry.rel, e.kind()),
                )
            })?;
            drop(dest);
            // The umask may have taken bits off at creation; the recorded
            // mode is applied exactly.
            if let CopyMode::Restore(set) = mode {
                set_mode(&target, set.file(&entry.rel)).map_err(|e| {
                    self.git_fail(
                        op,
                        format!("{} cannot be given its mode ({})", entry.rel, e.kind()),
                    )
                })?;
            }
        }
        Ok(())
    }

    /// Create the save directories down to `<run>` (not `repo`), each level a
    /// real directory or newly created: `git-save` and below `0700`. Returns
    /// whether `<run>` was created by this call.
    fn ensure_save_run_dir(
        &self,
        op: &str,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<bool, CoreError> {
        let mut path = self.data_dir.clone();
        let parts = [
            "ign-deploy",
            gateway,
            "projects",
            project,
            GIT_SAVE_DIR,
            run_id,
        ];
        let mut created_run = false;
        for (index, part) in parts.iter().enumerate() {
            path.push(part);
            match std::fs::symlink_metadata(&path) {
                Ok(m) if m.file_type().is_symlink() => {
                    return Err(
                        self.git_fail(op, format!("{part} is a symlink; symlinks are refused"))
                    );
                }
                Ok(m) if !m.file_type().is_dir() => {
                    return Err(self.git_fail(op, format!("{part} is not a directory")));
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    let mode = if index >= 4 { 0o700 } else { 0o755 };
                    create_dir_mode(&path, mode).map_err(|e| {
                        self.git_fail(op, format!("{part} cannot be created ({})", e.kind()))
                    })?;
                    created_run = index == parts.len() - 1;
                }
                Err(e) => {
                    return Err(
                        self.git_fail(op, format!("{part} cannot be inspected ({})", e.kind()))
                    );
                }
            }
        }
        Ok(created_run)
    }

    fn save_git_blocking(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<GitManifest, CoreError> {
        const OP: &str = "write";
        self.git_save_names(gateway, project, run_id)?;
        let source = self.live_git_dir(OP, project)?;
        // Walk first: a symlink anywhere refuses before a directory exists.
        let entries = self.walk_git(OP, &source)?;
        let before = self.manifest_of(OP, &entries)?;

        let root_mode = std::fs::symlink_metadata(&source)
            .map(|m| mode_of(&m))
            .map_err(|e| self.git_fail(OP, format!(".git cannot be inspected ({})", e.kind())))?;
        let created_run = self.ensure_save_run_dir(OP, gateway, project, run_id)?;
        let repo = self.git_save_repo(gateway, project, run_id);
        let run_dir = repo.parent().map(Path::to_path_buf).unwrap_or_default();
        let modes_path = run_dir.join(GIT_SAVE_MODES_FILE);
        // `repo` is removed only when this call made it: a save that loses a
        // race to another save of the run never removes the winner's.
        let cleanup = |made_repo: bool| {
            if made_repo {
                let _ = std::fs::remove_dir_all(&repo);
            }
            if created_run {
                let _ = std::fs::remove_file(&modes_path);
                let _ = std::fs::remove_dir(&run_dir);
                if let Some(save_dir) = run_dir.parent() {
                    // Only when this run made it and nothing else lives there.
                    let _ = std::fs::remove_dir(save_dir);
                }
            }
        };
        if std::fs::symlink_metadata(&repo).is_ok() {
            if created_run {
                cleanup(false);
            }
            return Err(self.git_fail(
                OP,
                format!(
                    "{} already exists; a save is never overwritten",
                    git_save_location(LedgerRoot::DataDir, gateway, project, run_id)
                ),
            ));
        }
        if let Err(e) = create_dir_mode(&repo, CopyMode::Private.dir_mode()) {
            cleanup(false);
            return Err(self.git_fail(
                OP,
                format!("the save directory cannot be created ({})", e.kind()),
            ));
        }
        let outcome = (|| {
            self.copy_git(OP, &entries, &repo, &CopyMode::Private)?;
            let after = self.manifest_blocking(OP, &repo)?;
            if after != before {
                return Err(self.git_fail(
                    OP,
                    "the saved copy does not match the .git it was taken from",
                ));
            }
            // The original modes, beside the private copy.
            let mut modes = create_new_file(&modes_path, 0o600).map_err(|e| {
                self.git_fail(
                    OP,
                    format!("the modes record cannot be created ({})", e.kind()),
                )
            })?;
            modes
                .write_all(modes_text(root_mode, &entries).as_bytes())
                .and_then(|()| modes.sync_all())
                .map_err(|e| {
                    self.git_fail(
                        OP,
                        format!("the modes record cannot be written ({})", e.kind()),
                    )
                })?;
            Ok(after)
        })();
        if outcome.is_err() {
            cleanup(true);
        }
        outcome
    }

    fn save_manifest_blocking(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<GitManifest, CoreError> {
        self.git_save_names(gateway, project, run_id)?;
        self.require_data_dir()?;
        self.save_chain_must_exist("read", gateway, project, run_id)?;
        let repo = self.git_save_repo(gateway, project, run_id);
        self.require_real_dir(
            "read",
            &repo,
            &git_save_location(LedgerRoot::DataDir, gateway, project, run_id),
        )?;
        self.manifest_blocking("read", &repo)
    }

    fn restore_git_blocking(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<GitManifest, CoreError> {
        const OP: &str = "write";
        self.git_save_names(gateway, project, run_id)?;
        self.require_data_dir()?;
        self.require_real_dir(OP, &self.data_dir.join("projects"), "projects")?;
        self.require_real_dir(
            OP,
            &self.data_dir.join("projects").join(project),
            &format!("projects/{project}"),
        )?;
        let git = self.project_git_dir(project);
        // Never overwrite: anything at `.git`, a symlink included, refuses.
        match std::fs::symlink_metadata(&git) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => {
                return Err(self.git_fail(
                    OP,
                    format!("projects/{project}/.git exists; it is never replaced"),
                ));
            }
            Err(e) => {
                return Err(self.git_fail(
                    OP,
                    format!("projects/{project}/.git cannot be inspected ({})", e.kind()),
                ));
            }
        }
        self.save_chain_must_exist(OP, gateway, project, run_id)?;
        let repo = self.git_save_repo(gateway, project, run_id);
        let location = git_save_location(LedgerRoot::DataDir, gateway, project, run_id);
        self.require_real_dir(OP, &repo, &location)?;
        let entries = self.walk_git(OP, &repo)?;
        let saved = self.manifest_of(OP, &entries)?;

        // Copy beside the save, then rename into place: `.git` is never seen
        // half written.
        let staging = repo
            .parent()
            .map(|run| run.join("restoring"))
            .ok_or_else(|| self.git_fail(OP, "the save has no parent directory"))?;
        match std::fs::symlink_metadata(&staging) {
            Ok(m) if m.file_type().is_dir() => {
                let _ = std::fs::remove_dir_all(&staging);
            }
            Ok(_) => {
                return Err(self.git_fail(OP, "restoring is not a directory; it is left alone"));
            }
            Err(_) => {}
        }
        let modes = self.read_save_modes(&repo);
        let mode = CopyMode::Restore(&modes);
        let outcome = (|| {
            create_dir_mode(&staging, mode.dir_mode()).map_err(|e| {
                self.git_fail(
                    OP,
                    format!("the staging directory cannot be created ({})", e.kind()),
                )
            })?;
            self.copy_git(OP, &entries, &staging, &mode)?;
            if self.manifest_blocking(OP, &staging)? != saved {
                return Err(self.git_fail(
                    OP,
                    "the restored copy does not match the save; nothing was restored",
                ));
            }
            // Directories get their recorded modes now that they are filled,
            // deepest first, the root last.
            for entry in entries.iter().rev().filter(|e| e.is_dir) {
                set_mode(&staging.join(&entry.rel), modes.dir(&entry.rel)).map_err(|e| {
                    self.git_fail(
                        OP,
                        format!("{} cannot be given its mode ({})", entry.rel, e.kind()),
                    )
                })?;
            }
            set_mode(&staging, modes.dir(".")).map_err(|e| {
                self.git_fail(
                    OP,
                    format!("the restored .git cannot be given its mode ({})", e.kind()),
                )
            })?;
            // The last look: `.git` appeared meanwhile, so do not rename.
            if std::fs::symlink_metadata(&git).is_ok() {
                return Err(self.git_fail(
                    OP,
                    format!("projects/{project}/.git appeared; it is never replaced"),
                ));
            }
            std::fs::rename(&staging, &git).map_err(|e| {
                self.git_fail(
                    OP,
                    format!("the restored .git cannot be put in place ({})", e.kind()),
                )
            })?;
            self.manifest_blocking(OP, &git)
        })();
        if outcome.is_err() {
            let _ = std::fs::remove_dir_all(&staging);
        }
        outcome
    }

    /// The modes recorded beside the save at `repo`. Anything unreadable,
    /// missing, a symlink or over the cap is "no record": owner-only.
    fn read_save_modes(&self, repo: &Path) -> ModeSet {
        let Some(path) = repo.parent().map(|run| run.join(GIT_SAVE_MODES_FILE)) else {
            return ModeSet::owner_only();
        };
        let regular = std::fs::symlink_metadata(&path)
            .is_ok_and(|m| m.file_type().is_file() && m.len() <= GIT_SAVE_MODES_MAX);
        if !regular {
            return ModeSet::owner_only();
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => ModeSet::parse(&text),
            Err(_) => ModeSet::owner_only(),
        }
    }

    fn remove_git_save_blocking(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<(), CoreError> {
        const OP: &str = "write";
        self.git_save_names(gateway, project, run_id)?;
        self.require_data_dir()?;
        let repo = self.git_save_repo(gateway, project, run_id);
        // Every level above `repo` must be a real directory or absent.
        if let SaveChain::Absent(_) = self.require_save_chain(OP, gateway, project, run_id)? {
            return Ok(());
        }
        match std::fs::symlink_metadata(&repo) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Ok(m) if m.file_type().is_dir() => {
                std::fs::remove_dir_all(&repo).map_err(|e| {
                    self.git_fail(OP, format!("the save cannot be removed ({})", e.kind()))
                })?;
            }
            Ok(_) => {
                return Err(self.git_fail(OP, "the save is not a directory; it is left alone"));
            }
            Err(e) => {
                return Err(
                    self.git_fail(OP, format!("the save cannot be inspected ({})", e.kind()))
                );
            }
        }
        // The modes record and the now-empty run directory go too; a
        // non-empty run directory stays.
        if let Some(run_dir) = repo.parent() {
            let modes = run_dir.join(GIT_SAVE_MODES_FILE);
            if std::fs::symlink_metadata(&modes).is_ok_and(|m| m.file_type().is_file()) {
                let _ = std::fs::remove_file(&modes);
            }
            let _ = std::fs::remove_dir(run_dir);
        }
        Ok(())
    }

    /// Write `writes` inside the project's existing `.git`.
    fn write_git_files_blocking(
        &self,
        project: &str,
        writes: &[GitWrite],
    ) -> Result<(), CoreError> {
        const OP: &str = "write";
        let git = self.live_git_dir(OP, project)?;
        // Every shape and every lock is checked before the first byte is
        // written, so a refusal leaves the repo as it was.
        let mut plan: Vec<(PathBuf, PathBuf, Vec<String>, &GitWrite)> = Vec::new();
        for write in writes {
            write
                .name
                .check_shape()
                .map_err(|why| self.git_fail(OP, format!("a .git write is refused: {why}")))?;
            let rel = write.name.rel();
            let parts: Vec<String> = rel.split('/').map(str::to_string).collect();
            let target = parts.iter().fold(git.clone(), |p, part| p.join(part));
            let temp_suffix = match write.name {
                GitWriteName::PackFile(_) | GitWriteName::PackIndex(_) => ".ign-tmp",
                _ => ".lock",
            };
            let mut temp_name = parts.last().cloned().unwrap_or_default();
            temp_name.push_str(temp_suffix);
            let temp = target.with_file_name(&temp_name);
            // Existing parents must be real directories.
            let mut dir = git.clone();
            for part in &parts[..parts.len() - 1] {
                dir.push(part);
                match std::fs::symlink_metadata(&dir) {
                    Ok(m) if m.file_type().is_dir() => {}
                    Ok(_) => {
                        return Err(self.git_fail(
                            OP,
                            format!("{part} below .git is a symlink or not a directory; refused"),
                        ));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
                    Err(e) => {
                        return Err(self.git_fail(
                            OP,
                            format!("{part} below .git cannot be inspected ({})", e.kind()),
                        ));
                    }
                }
            }
            match std::fs::symlink_metadata(&temp) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Ok(_) => {
                    return Err(self.git_fail(
                        OP,
                        format!("{rel}{temp_suffix} exists; another writer holds it, nothing was written"),
                    ));
                }
                Err(e) => {
                    return Err(self.git_fail(
                        OP,
                        format!("{rel}{temp_suffix} cannot be inspected ({})", e.kind()),
                    ));
                }
            }
            if let Ok(m) = std::fs::symlink_metadata(&target)
                && !m.file_type().is_file()
            {
                return Err(
                    self.git_fail(OP, format!("{rel} is not a regular file; it is left alone"))
                );
            }
            plan.push((target, temp, parts, write));
        }
        for (target, temp, parts, write) in plan {
            let mut dir = git.clone();
            for part in &parts[..parts.len() - 1] {
                dir.push(part);
                match std::fs::symlink_metadata(&dir) {
                    Ok(_) => {}
                    Err(_) => {
                        // WR-B1: a directory the advance makes takes its
                        // parent directory's permission bits.
                        let parent_mode = dir
                            .parent()
                            .and_then(|p| std::fs::symlink_metadata(p).ok())
                            .map_or(0o755, |m| mode_of(&m));
                        create_dir_mode(&dir, parent_mode)
                            .and_then(|()| set_dir_mode_no_follow(&dir, parent_mode))
                            .map_err(|e| {
                                self.git_fail(
                                    OP,
                                    format!("{part} cannot be created ({})", e.kind()),
                                )
                            })?;
                    }
                }
            }
            let rel = write.name.rel();
            // WR-B1: the mode of the file this replaces, never wider (and
            // never an execute bit); a new `config` is owner-only (it can hold
            // a remote URL with userinfo), any other new file `0644`.
            let file_mode = match std::fs::symlink_metadata(&target) {
                Ok(m) => mode_of(&m) & 0o666,
                Err(_) if matches!(write.name, GitWriteName::Config) => 0o600,
                Err(_) => 0o644,
            };
            // A lock this call failed to create is another writer's: only a
            // failure after the create removes it.
            let mut file = match create_new_file(&temp, file_mode) {
                Ok(file) => file,
                Err(e) => {
                    return Err(
                        self.git_fail(OP, format!("{rel} cannot be written ({})", e.kind()))
                    );
                }
            };
            let result = (|| -> std::io::Result<()> {
                #[cfg(unix)]
                {
                    // the umask may have narrowed the create; the file gets
                    // exactly the mode it replaces (an fchmod on the open file)
                    use std::os::unix::fs::PermissionsExt;
                    file.set_permissions(std::fs::Permissions::from_mode(file_mode))?;
                }
                file.write_all(&write.bytes)?;
                file.sync_all()?;
                drop(file);
                std::fs::rename(&temp, &target)
            })();
            if let Err(e) = result {
                let _ = std::fs::remove_file(&temp);
                return Err(self.git_fail(OP, format!("{rel} cannot be written ({})", e.kind())));
            }
        }
        Ok(())
    }

    /// `levels` below `git`, each a real directory, none followed: a symlink
    /// or another kind of entry refuses, naming the level; `None` when a level
    /// is absent (nothing below it exists). Path resolution follows a linked
    /// ancestor, so a check of the final entry alone does not see one.
    fn git_dirs_real(&self, git: &Path, levels: &[&str]) -> Result<Option<PathBuf>, CoreError> {
        let mut path = git.to_path_buf();
        for (index, part) in levels.iter().enumerate() {
            path.push(part);
            let shown = levels[..=index].join("/");
            match std::fs::symlink_metadata(&path) {
                Ok(m) if m.file_type().is_dir() => {}
                Ok(m) if m.file_type().is_symlink() => {
                    return Err(self.fail(format!("{shown} is a symlink; symlinks are refused")));
                }
                Ok(_) => return Err(self.fail(format!("{shown} is not a directory"))),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => {
                    return Err(self.fail(format!("{shown} cannot be inspected ({})", e.kind())));
                }
            }
        }
        Ok(Some(path))
    }

    /// `rel` below `git` once every directory above its last component passed
    /// [`Self::git_dirs_real`]; `None` when one of them is absent.
    fn git_file_path(&self, git: &Path, rel: &str) -> Result<Option<PathBuf>, CoreError> {
        let parts: Vec<&str> = rel.split('/').collect();
        let (last, parents) = parts.split_last().unwrap_or((&"", &[]));
        Ok(self.git_dirs_real(git, parents)?.map(|dir| dir.join(last)))
    }

    /// A capped read of a regular file; `None` when it does not exist.
    fn read_git_small(
        &self,
        git: &Path,
        rel: &str,
        cap: usize,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        let Some(path) = self.git_file_path(git, rel)? else {
            return Ok(None);
        };
        match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(self.fail(format!("{rel} cannot be inspected ({})", e.kind()))),
            Ok(m) if !m.file_type().is_file() => Err(self.fail(format!(
                "{rel} is not a regular file (symlinks are refused)"
            ))),
            Ok(_) => {
                let file = std::fs::File::open(&path)
                    .map_err(|e| self.fail(format!("{rel} cannot be read ({})", e.kind())))?;
                let mut bytes = Vec::new();
                file.take(cap as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| self.fail(format!("{rel} cannot be read ({})", e.kind())))?;
                Ok(Some(bytes))
            }
        }
    }

    /// Whether `rel` below `git` exists, any kind of entry.
    fn git_present(&self, git: &Path, rel: &str) -> Result<bool, CoreError> {
        let path = rel
            .split('/')
            .fold(git.to_path_buf(), |p, part| p.join(part));
        match std::fs::symlink_metadata(&path) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(self.fail(format!("{rel} cannot be inspected ({})", e.kind()))),
        }
    }

    /// Every loose ref below `dir` (named `prefix`), at most one past the
    /// cap so the caller's cap check names the excess.
    fn collect_loose_refs(
        &self,
        dir: &Path,
        prefix: &str,
        out: &mut Vec<(String, Vec<u8>)>,
    ) -> Result<(), CoreError> {
        let read = match std::fs::read_dir(dir) {
            Ok(read) => read,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(self.fail(format!("{prefix} cannot be listed ({})", e.kind()))),
        };
        let mut names = Vec::new();
        for entry in read {
            let entry = entry
                .map_err(|e| self.fail(format!("{prefix} cannot be listed ({})", e.kind())))?;
            names.push(entry.file_name());
        }
        names.sort();
        for name in names {
            if out.len() > GIT_LOOSE_REFS_MAX {
                return Ok(());
            }
            let Some(text) = name.to_str() else {
                return Err(self.fail(format!("{prefix} holds a name that is not UTF-8")));
            };
            let rel = format!("{prefix}/{text}");
            let path = dir.join(&name);
            let meta = std::fs::symlink_metadata(&path)
                .map_err(|e| self.fail(format!("{rel} cannot be inspected ({})", e.kind())))?;
            if meta.file_type().is_symlink() {
                return Err(self.fail(format!("{rel} is a symlink; symlinks are refused")));
            }
            if meta.file_type().is_dir() {
                self.collect_loose_refs(&path, &rel, out)?;
            } else if meta.file_type().is_file() {
                let file = std::fs::File::open(&path)
                    .map_err(|e| self.fail(format!("{rel} cannot be read ({})", e.kind())))?;
                let mut bytes = Vec::new();
                file.take(GIT_SMALL_FILE_MAX as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| self.fail(format!("{rel} cannot be read ({})", e.kind())))?;
                out.push((rel, bytes));
            } else {
                return Err(self.fail(format!("{rel} is not a regular file")));
            }
        }
        Ok(())
    }

    fn read_git_state_blocking(&self, project: &str) -> Result<GitFiles, CoreError> {
        let kind = self.git_entry_blocking(project)?;
        if kind != GitEntry::Directory {
            return Ok(GitFiles::without_contents(kind));
        }
        let git = self.project_git_dir(project);
        let mut files = GitFiles::without_contents(GitEntry::Directory);
        files.head = self.read_git_small(&git, "HEAD", GIT_SMALL_FILE_MAX)?;
        files.config = self.read_git_small(&git, "config", GIT_SMALL_FILE_MAX)?;
        files.packed_refs = self.read_git_small(&git, "packed-refs", GIT_PACKED_REFS_MAX)?;
        files.index_present = self.git_present(&git, "index")?;
        for marker in GIT_MARKERS {
            if self.git_present(&git, marker)? {
                files.markers.push((*marker).to_string());
            }
        }
        for prefix in ["refs/heads", "refs/remotes"] {
            let levels: Vec<&str> = prefix.split('/').collect();
            if let Some(dir) = self.git_dirs_real(&git, &levels)? {
                self.collect_loose_refs(&dir, prefix, &mut files.loose_refs)?;
            }
        }
        files.head_log_tail = self.read_log_tail(&git)?;
        Ok(files)
    }

    /// The last [`GIT_LOG_TAIL_MAX`] bytes of `logs/HEAD`.
    fn read_log_tail(&self, git: &Path) -> Result<Option<Vec<u8>>, CoreError> {
        let Some(path) = self.git_file_path(git, "logs/HEAD")? else {
            return Ok(None);
        };
        let meta = match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(self.fail(format!("logs/HEAD cannot be inspected ({})", e.kind())));
            }
            Ok(m) => m,
        };
        if !meta.file_type().is_file() {
            return Err(self.fail("logs/HEAD is not a regular file (symlinks are refused)"));
        }
        let mut file = std::fs::File::open(&path)
            .map_err(|e| self.fail(format!("logs/HEAD cannot be read ({})", e.kind())))?;
        let len = meta.len();
        if len > GIT_LOG_TAIL_MAX as u64 {
            file.seek(SeekFrom::Start(len - GIT_LOG_TAIL_MAX as u64))
                .map_err(|e| self.fail(format!("logs/HEAD cannot be read ({})", e.kind())))?;
        }
        let mut bytes = Vec::new();
        file.take(GIT_LOG_TAIL_MAX as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| self.fail(format!("logs/HEAD cannot be read ({})", e.kind())))?;
        Ok(Some(bytes))
    }

    /// `<data_dir>/git.yaml`, capped; `None` only when it does not exist.
    fn read_git_yaml_blocking(&self) -> Result<Option<Vec<u8>>, CoreError> {
        let invalid = |problem: String| CoreError::DeployRunInvalid {
            run_id: "unknown".to_string(),
            problem: format!("git.yaml: {problem}"),
        };
        self.require_data_dir()?;
        let path = self.data_dir.join("git.yaml");
        match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(self.fail(format!("git.yaml cannot be inspected ({})", e.kind()))),
            Ok(m) if !m.file_type().is_file() => Err(invalid(
                "is not a regular file (symlinks are refused)".to_string(),
            )),
            Ok(m) if m.len() > YAML_MAX_BYTES as u64 => Err(invalid(format!(
                "is {} bytes, over the {YAML_MAX_BYTES} byte cap",
                m.len()
            ))),
            Ok(_) => {
                let file = std::fs::File::open(&path)
                    .map_err(|e| self.fail(format!("git.yaml cannot be read ({})", e.kind())))?;
                let mut bytes = Vec::new();
                file.take(YAML_MAX_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| self.fail(format!("git.yaml cannot be read ({})", e.kind())))?;
                if bytes.len() > YAML_MAX_BYTES {
                    return Err(invalid(format!(
                        "grew past the {YAML_MAX_BYTES} byte cap while it was read"
                    )));
                }
                Ok(Some(bytes))
            }
        }
    }

    /// Free bytes on the file system holding the save root of `project`.
    fn free_bytes_blocking(&self, gateway: &str, project: &str) -> Result<u64, CoreError> {
        // Any valid run id: only the directory chain is used.
        self.git_save_names(gateway, project, "free")?;
        self.require_data_dir()?;
        let mut path = self.data_dir.clone();
        for part in git_save_rel(gateway, project, "free")
            .split('/')
            .take_while(|part| *part != "free")
        {
            path.push(part);
        }
        // The nearest ancestor that exists: the save root may not yet.
        while std::fs::symlink_metadata(&path).is_err() {
            if !path.pop() {
                return Err(self.fail("no ancestor of the save root can be inspected"));
            }
        }
        free_bytes_of(&path).map_err(|why| self.fail(format!("free space cannot be read ({why})")))
    }

    /// The count and total size of the project's live `.git` from the walk's
    /// metadata alone (no file is opened).
    fn git_size_blocking(&self, project: &str) -> Result<GitSize, CoreError> {
        let git = self.live_git_dir("read", project)?;
        let entries = self.walk_git("read", &git)?;
        let mut size = GitSize { files: 0, bytes: 0 };
        for entry in entries.iter().filter(|e| !e.is_dir) {
            size.files += 1;
            size.bytes += entry.size;
        }
        Ok(size)
    }

    /// The object files below the project's `.git/objects`: the two levels
    /// git uses (`objects/<fan>/<file>`, `objects/pack/<file>`,
    /// `objects/info/<file>`), nothing opened, nothing followed. A symlink is
    /// reported as one (and [`GitObjectList::from_entries`] refuses it).
    fn list_git_objects_blocking(&self, project: &str) -> Result<GitObjectList, CoreError> {
        const OP: &str = "read";
        let git = self.live_git_dir(OP, project)?;
        let objects = git.join("objects");
        self.require_real_dir(OP, &objects, &format!("projects/{project}/.git/objects"))?;
        let mut entries: Vec<GitObjectEntry> = Vec::new();
        let names_of = |dir: &Path, shown_dir: &str| -> Result<Vec<String>, CoreError> {
            let read = std::fs::read_dir(dir).map_err(|e| {
                self.git_fail(OP, format!("{shown_dir} cannot be listed ({})", e.kind()))
            })?;
            let mut names = Vec::new();
            for entry in read {
                let entry = entry.map_err(|e| {
                    self.git_fail(OP, format!("{shown_dir} cannot be listed ({})", e.kind()))
                })?;
                // A name that is not UTF-8 cannot be one of the fixed shapes.
                if let Some(name) = entry.file_name().to_str() {
                    names.push(name.to_string());
                }
            }
            names.sort();
            Ok(names)
        };
        let kind_of = |meta: &std::fs::Metadata| {
            let ft = meta.file_type();
            if ft.is_symlink() {
                GitObjectEntryKind::Symlink
            } else if ft.is_file() {
                GitObjectEntryKind::File
            } else {
                GitObjectEntryKind::Other
            }
        };
        for top in names_of(&objects, "objects")? {
            let top_path = objects.join(&top);
            let meta = std::fs::symlink_metadata(&top_path).map_err(|e| {
                self.git_fail(
                    OP,
                    format!("objects/{top} cannot be inspected ({})", e.kind()),
                )
            })?;
            entries.push(GitObjectEntry {
                rel: format!("objects/{top}"),
                size: meta.len(),
                kind: kind_of(&meta),
            });
            if !meta.file_type().is_dir() {
                continue;
            }
            for name in names_of(&top_path, &format!("objects/{top}"))? {
                if entries.len() >= GIT_OBJECTS_LIST_MAX_ENTRIES {
                    return Err(self.git_fail(
                        OP,
                        format!("objects/ holds more than {GIT_OBJECTS_LIST_MAX_ENTRIES} entries"),
                    ));
                }
                let path = top_path.join(&name);
                let meta = std::fs::symlink_metadata(&path).map_err(|e| {
                    self.git_fail(
                        OP,
                        format!("objects/{top}/{name} cannot be inspected ({})", e.kind()),
                    )
                })?;
                entries.push(GitObjectEntry {
                    rel: format!("objects/{top}/{name}"),
                    size: meta.len(),
                    kind: kind_of(&meta),
                });
            }
        }
        GitObjectList::from_entries(&entries)
            .map_err(|why| self.git_fail(OP, format!("the object listing is refused: {why}")))
    }

    /// Copy the listed object `files` into `dest` (see
    /// [`GitHostRead::fetch_git_objects`]).
    fn fetch_git_objects_blocking(
        &self,
        project: &str,
        files: &[GitObjectFile],
        dest: &Path,
    ) -> Result<(), CoreError> {
        const OP: &str = "read";
        // Every name first, before any path is built or file opened.
        for file in files {
            check_object_rel(&file.rel).map_err(|why| {
                self.git_fail(OP, format!("{} is refused: {why}", file.rel.escape_debug()))
            })?;
        }
        let git = self.live_git_dir(OP, project)?;
        self.require_real_dir(OP, &git.join("objects"), "objects")?;
        self.require_real_dir(OP, dest, "the destination")?;
        for file in files {
            let parts: Vec<&str> = file.rel.split('/').collect();
            // `check_object_rel` proved exactly `objects/<dir>/<name>`.
            let (dir, name) = (parts[1], parts[2]);
            let source_dir = git.join("objects").join(dir);
            self.require_real_dir(OP, &source_dir, &format!("objects/{dir}"))?;
            let source_path = source_dir.join(name);
            let mut source = open_no_follow(&source_path).map_err(|e| {
                self.git_fail(OP, format!("{} cannot be opened ({})", file.rel, e.kind()))
            })?;
            let meta = source.metadata().map_err(|e| {
                self.git_fail(
                    OP,
                    format!("{} cannot be inspected ({})", file.rel, e.kind()),
                )
            })?;
            if !meta.file_type().is_file() {
                return Err(self.git_fail(OP, format!("{} is not a regular file", file.rel)));
            }
            if meta.len() != file.size {
                return Err(self.git_fail(
                    OP,
                    format!(
                        "{} is {} bytes but the listing said {}",
                        file.rel,
                        meta.len(),
                        file.size
                    ),
                ));
            }
            let target_dir = dest.join("objects").join(dir);
            std::fs::create_dir_all(&target_dir).map_err(|e| {
                self.git_fail(
                    OP,
                    format!(
                        "objects/{dir} cannot be made in the destination ({})",
                        e.kind()
                    ),
                )
            })?;
            let mut target = create_new_file(&target_dir.join(name), 0o600).map_err(|e| {
                self.git_fail(
                    OP,
                    format!(
                        "{} cannot be created in the destination ({})",
                        file.rel,
                        e.kind()
                    ),
                )
            })?;
            // One byte more than listed would show a file that grew.
            let copied = std::io::copy(&mut (&mut source).take(file.size + 1), &mut target)
                .map_err(|e| {
                    self.git_fail(OP, format!("{} cannot be copied ({})", file.rel, e.kind()))
                })?;
            if copied != file.size {
                return Err(
                    self.git_fail(OP, format!("{} changed size while it was copied", file.rel))
                );
            }
        }
        Ok(())
    }

    /// Create then remove `.ign-probe-<run id>` in the project directory and
    /// in its `.git`.
    fn probe_write_blocking(&self, project: &str, run_id: &str) -> Result<(), CoreError> {
        const OP: &str = "write";
        if let Some(why) = component_problem(project) {
            return Err(self.git_fail(OP, format!("the project name {why}")));
        }
        if RunId::parse(run_id).is_err() {
            return Err(run_invalid(
                &git_entry_location(project),
                "the run id is not 1 to 128 characters of [A-Za-z0-9._-]",
            ));
        }
        let git = self.live_git_dir(OP, project)?;
        let name = git_probe_name(run_id);
        let places = [
            (
                self.data_dir.join("projects").join(project),
                format!("projects/{project}"),
            ),
            (git, format!("projects/{project}/.git")),
        ];
        for (dir, shown) in places {
            let probe = dir.join(&name);
            create_new_file(&probe, 0o600).map_err(|e| {
                self.git_fail(
                    OP,
                    format!("a probe file cannot be created in {shown} ({})", e.kind()),
                )
            })?;
            std::fs::remove_file(&probe).map_err(|e| {
                self.git_fail(
                    OP,
                    format!("the probe file in {shown} cannot be removed ({})", e.kind()),
                )
            })?;
        }
        Ok(())
    }
}

/// Free bytes (available to this account) on the file system holding `path`.
#[cfg(unix)]
fn free_bytes_of(path: &Path) -> Result<u64, String> {
    let stat = rustix::fs::statvfs(path).map_err(|e| e.to_string())?;
    Ok(stat.f_bavail.saturating_mul(stat.f_frsize))
}

/// The script of the Windows free-space query. The path is not in it: it is
/// read from the environment variable [`FREE_PATH_ENV`], so it is data and
/// never text a shell or PowerShell parses.
#[cfg(any(windows, test))]
const FREE_SPACE_SCRIPT: &str = "[System.IO.DriveInfo]::new([System.IO.Path]::GetPathRoot($env:IGN_FREE_PATH)).AvailableFreeSpace";

/// The environment variable that carries the path to the free-space script.
#[cfg(any(windows, test))]
const FREE_PATH_ENV: &str = "IGN_FREE_PATH";

/// The `powershell` command that reads the free bytes of the drive holding
/// `path`: a fixed `-Command` text, with `path` passed only through the
/// environment (a path such as `x'; whoami #` or `$(...)` stays a value).
#[cfg(any(windows, test))]
fn free_space_command(path: &Path) -> std::process::Command {
    let mut command = std::process::Command::new("powershell");
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            FREE_SPACE_SCRIPT,
        ])
        .env(FREE_PATH_ENV, path);
    command
}

/// Free bytes on the drive holding `path`, read with one `powershell`
/// subprocess (the precedent of `deploy/owner_acl.rs`).
#[cfg(windows)]
fn free_bytes_of(path: &Path) -> Result<u64, String> {
    let out = free_space_command(path)
        .output()
        .map_err(|e| e.kind().to_string())?;
    if !out.status.success() {
        return Err("the drive query failed".to_string());
    }
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<u64>()
        .map_err(|_| "the drive query answered something that is not a number".to_string())
}

#[cfg(not(any(unix, windows)))]
fn free_bytes_of(_path: &Path) -> Result<u64, String> {
    Err("this platform has no free-space query".to_string())
}

/// A prefix as a message names it (`the repo` for the root).
fn shown(prefix: &str) -> &str {
    if prefix.is_empty() {
        "the repo"
    } else {
        prefix
    }
}

#[async_trait::async_trait]
impl GitHostRead for LocalFsTransport {
    async fn read_git_state(&self, project: &str) -> Result<GitFiles, CoreError> {
        let this = self.clone();
        let project = project.to_string();
        tokio::task::spawn_blocking(move || this.read_git_state_blocking(&project))
            .await
            .map_err(|e| CoreError::Internal(format!("local read task failed: {e}")))?
    }

    async fn git_manifest(&self, project: &str) -> Result<GitManifest, CoreError> {
        let this = self.clone();
        let project = project.to_string();
        tokio::task::spawn_blocking(move || {
            let git = this.live_git_dir("read", &project)?;
            this.manifest_blocking("read", &git)
        })
        .await
        .map_err(|e| CoreError::Internal(format!("local read task failed: {e}")))?
    }

    async fn save_manifest(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<GitManifest, CoreError> {
        let this = self.clone();
        let (gateway, project, run_id) =
            (gateway.to_string(), project.to_string(), run_id.to_string());
        tokio::task::spawn_blocking(move || {
            this.save_manifest_blocking(&gateway, &project, &run_id)
        })
        .await
        .map_err(|e| CoreError::Internal(format!("local read task failed: {e}")))?
    }

    async fn git_file_digest(
        &self,
        project: &str,
        name: &GitWriteName,
    ) -> Result<Option<String>, CoreError> {
        let this = self.clone();
        let (project, name) = (project.to_string(), name.clone());
        tokio::task::spawn_blocking(move || {
            name.check_shape()
                .map_err(|why| this.fail(format!("a .git file name is refused: {why}")))?;
            let git = this.live_git_dir("read", &project)?;
            let rel = name.rel();
            let Some(path) = this.git_file_path(&git, &rel)? else {
                return Ok(None);
            };
            match std::fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(this.fail(format!("{rel} cannot be inspected ({})", e.kind()))),
                Ok(m) if !m.file_type().is_file() => Err(this.fail(format!(
                    "{rel} is not a regular file (symlinks are refused)"
                ))),
                Ok(_) => {
                    let mut file = std::fs::File::open(&path)
                        .map_err(|e| this.fail(format!("{rel} cannot be read ({})", e.kind())))?;
                    let mut hasher = Sha256::new();
                    let mut buf = [0u8; 65_536];
                    loop {
                        let n = file.read(&mut buf).map_err(|e| {
                            this.fail(format!("{rel} cannot be read ({})", e.kind()))
                        })?;
                        if n == 0 {
                            break;
                        }
                        hasher.update(&buf[..n]);
                    }
                    Ok(Some(format!("{:x}", hasher.finalize())))
                }
            }
        })
        .await
        .map_err(|e| CoreError::Internal(format!("local read task failed: {e}")))?
    }

    async fn read_git_yaml(&self) -> Result<Option<Vec<u8>>, CoreError> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.read_git_yaml_blocking())
            .await
            .map_err(|e| CoreError::Internal(format!("local read task failed: {e}")))?
    }

    async fn project_dir_entry(
        &self,
        project: &str,
        dir: ImportDir,
    ) -> Result<GitEntry, CoreError> {
        let this = self.clone();
        let project = project.to_string();
        tokio::task::spawn_blocking(move || this.entry_blocking(&project, dir.name()))
            .await
            .map_err(|e| CoreError::Internal(format!("local read task failed: {e}")))?
    }

    async fn free_bytes(&self, gateway: &str, project: &str) -> Result<u64, CoreError> {
        let this = self.clone();
        let (gateway, project) = (gateway.to_string(), project.to_string());
        tokio::task::spawn_blocking(move || this.free_bytes_blocking(&gateway, &project))
            .await
            .map_err(|e| CoreError::Internal(format!("local read task failed: {e}")))?
    }

    async fn git_size(&self, project: &str) -> Result<GitSize, CoreError> {
        let this = self.clone();
        let project = project.to_string();
        tokio::task::spawn_blocking(move || this.git_size_blocking(&project))
            .await
            .map_err(|e| CoreError::Internal(format!("local read task failed: {e}")))?
    }

    async fn list_git_objects(&self, project: &str) -> Result<GitObjectList, CoreError> {
        let this = self.clone();
        let project = project.to_string();
        tokio::task::spawn_blocking(move || this.list_git_objects_blocking(&project))
            .await
            .map_err(|e| CoreError::Internal(format!("local read task failed: {e}")))?
    }

    async fn fetch_git_objects(
        &self,
        project: &str,
        files: &[GitObjectFile],
        dest: &Path,
    ) -> Result<(), CoreError> {
        let this = self.clone();
        let (project, files, dest) = (project.to_string(), files.to_vec(), dest.to_path_buf());
        tokio::task::spawn_blocking(move || {
            this.fetch_git_objects_blocking(&project, &files, &dest)
        })
        .await
        .map_err(|e| CoreError::Internal(format!("local read task failed: {e}")))?
    }
}

#[async_trait::async_trait]
impl GitStateWriter for LocalFsTransport {
    async fn save_git(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<GitManifest, CoreError> {
        let this = self.clone();
        let (gateway, project, run_id) =
            (gateway.to_string(), project.to_string(), run_id.to_string());
        tokio::task::spawn_blocking(move || this.save_git_blocking(&gateway, &project, &run_id))
            .await
            .map_err(|e| CoreError::Internal(format!("local write task failed: {e}")))?
    }

    async fn restore_git(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<GitManifest, CoreError> {
        let this = self.clone();
        let (gateway, project, run_id) =
            (gateway.to_string(), project.to_string(), run_id.to_string());
        tokio::task::spawn_blocking(move || this.restore_git_blocking(&gateway, &project, &run_id))
            .await
            .map_err(|e| CoreError::Internal(format!("local write task failed: {e}")))?
    }

    async fn write_git_files(&self, project: &str, writes: &[GitWrite]) -> Result<(), CoreError> {
        let this = self.clone();
        let (project, writes) = (project.to_string(), writes.to_vec());
        tokio::task::spawn_blocking(move || this.write_git_files_blocking(&project, &writes))
            .await
            .map_err(|e| CoreError::Internal(format!("local write task failed: {e}")))?
    }

    async fn remove_git_save(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<(), CoreError> {
        let this = self.clone();
        let (gateway, project, run_id) =
            (gateway.to_string(), project.to_string(), run_id.to_string());
        tokio::task::spawn_blocking(move || {
            this.remove_git_save_blocking(&gateway, &project, &run_id)
        })
        .await
        .map_err(|e| CoreError::Internal(format!("local write task failed: {e}")))?
    }

    async fn probe_write(&self, project: &str, run_id: &str) -> Result<(), CoreError> {
        let this = self.clone();
        let (project, run_id) = (project.to_string(), run_id.to_string());
        tokio::task::spawn_blocking(move || this.probe_write_blocking(&project, &run_id))
            .await
            .map_err(|e| CoreError::Internal(format!("local write task failed: {e}")))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(data: &Path) -> LocalFsTransport {
        LocalFsTransport::new(data.to_path_buf(), "nodes.master")
    }

    fn data_with_projects() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("projects")).expect("projects");
        dir
    }

    /// A data dir holding a complete save of run `RUN` for `gw`/`CapApp`
    /// and a project directory with no `.git` (the state after an import).
    const SAVE_RUN: &str = "20261114T101500Z-ab12";
    #[cfg(unix)]
    const SAVE_LEVELS: [&str; 6] = [
        "ign-deploy",
        "gw",
        "projects",
        "CapApp",
        "git-save",
        SAVE_RUN,
    ];

    fn data_with_save() -> tempfile::TempDir {
        let dir = data_with_projects();
        std::fs::create_dir_all(dir.path().join("projects/CapApp")).expect("project");
        let repo = dir
            .path()
            .join("ign-deploy/gw/projects/CapApp/git-save")
            .join(SAVE_RUN)
            .join("repo");
        std::fs::create_dir_all(repo.join("objects")).expect("repo");
        std::fs::write(repo.join("HEAD"), "ref: refs/heads/main\n").expect("head");
        std::fs::write(repo.join("config"), "[core]\n").expect("config");
        std::fs::write(repo.join("objects/o1"), "object").expect("object");
        dir
    }

    /// 27-30 (WR-B6): a symlink at any level of the save path refuses the
    /// restore and the save-manifest read, the way the remove does.
    #[cfg(unix)]
    #[test]
    fn local_git_restore_refuses_a_symlinked_save_level() {
        for (index, level) in SAVE_LEVELS.iter().enumerate() {
            let dir = data_with_save();
            let t = host(dir.path());
            // the level becomes a link to a directory that holds the whole,
            // well formed tree: only the link itself is wrong
            let mut at = dir.path().to_path_buf();
            for part in &SAVE_LEVELS[..index] {
                at.push(part);
            }
            let real = at.join(format!("{level}.real"));
            std::fs::rename(at.join(level), &real).expect("rename");
            std::os::unix::fs::symlink(format!("{level}.real"), at.join(level)).expect("link");

            let err = t
                .restore_git_blocking("gw", "CapApp", SAVE_RUN)
                .expect_err(level);
            assert_eq!(err.code(), "transport_failed", "{level}");
            let text = err.to_string();
            assert!(
                text.contains(level) && text.contains("symlink"),
                "{level}: {text}"
            );
            assert!(
                !dir.path().join("projects/CapApp/.git").exists(),
                "{level}: no .git was created"
            );
            let err = t
                .save_manifest_blocking("gw", "CapApp", SAVE_RUN)
                .expect_err(level);
            let text = err.to_string();
            assert!(
                text.contains(level) && text.contains("symlink"),
                "{level}: {text}"
            );
        }
    }

    /// The restore and the manifest read of a chain with nothing at some
    /// level refuse (an absent level is an error there), and a complete
    /// chain reads and restores.
    #[test]
    fn local_git_restore_and_manifest_of_a_missing_save_refuse() {
        let dir = data_with_save();
        let t = host(dir.path());
        let manifest = t
            .save_manifest_blocking("gw", "CapApp", SAVE_RUN)
            .expect("complete chain");
        assert!(manifest.files >= 3, "{manifest:?}");
        std::fs::remove_dir_all(dir.path().join("ign-deploy/gw/projects/CapApp/git-save"))
            .expect("rm git-save");
        for err in [
            t.save_manifest_blocking("gw", "CapApp", SAVE_RUN)
                .expect_err("manifest"),
            t.restore_git_blocking("gw", "CapApp", SAVE_RUN)
                .expect_err("restore"),
        ] {
            assert_eq!(err.code(), "transport_failed");
            assert!(err.to_string().contains("git-save"), "{err}");
        }
        assert!(!dir.path().join("projects/CapApp/.git").exists());
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

    /// The restore puts back exactly the recorded modes and never grants a
    /// bit the original lacked: a `0600` config comes back `0600`, a `0700`
    /// directory `0700`, a `0644` file `0644`, a read-only object `0444`.
    /// With no modes record everything is owner-only.
    #[cfg(unix)]
    #[test]
    fn local_git_restore_puts_back_recorded_modes() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).expect("metadata").permissions().mode() & 0o777;
        let set = |p: &Path, m: u32| {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(m)).expect("chmod")
        };
        let dir = data_with_projects();
        let git = dir.path().join("projects/CapApp/.git");
        std::fs::create_dir_all(git.join("private")).expect("private");
        std::fs::create_dir_all(git.join("objects/ab")).expect("objects");
        std::fs::write(git.join("HEAD"), b"ref: refs/heads/production\n").expect("HEAD");
        std::fs::write(git.join("config"), b"[core]\n").expect("config");
        std::fs::write(git.join("private/secret"), b"s").expect("secret");
        std::fs::write(git.join("objects/ab/cdef"), b"o").expect("object");
        set(&git.join("HEAD"), 0o644);
        set(&git.join("config"), 0o600);
        set(&git.join("private/secret"), 0o600);
        set(&git.join("objects/ab/cdef"), 0o444);
        set(&git.join("private"), 0o700);
        set(&git.join("objects/ab"), 0o755);
        set(&git.join("objects"), 0o755);
        set(&git, 0o700);
        let t = host(dir.path());

        let saved = t.save_git_blocking("gw", "CapApp", "run1").expect("save");
        // The save is private: nothing in it grants group or other access.
        let repo = t.git_save_repo("gw", "CapApp", "run1");
        for rel in ["HEAD", "config", "private/secret", "objects/ab/cdef"] {
            assert_eq!(mode(&repo.join(rel)) & 0o077, 0, "{rel} in the save");
        }
        assert_eq!(mode(&repo.join("objects/ab")), 0o700);

        let restore = |t: &LocalFsTransport| {
            std::fs::remove_dir_all(&git).expect("the import deletes .git");
            t.restore_git_blocking("gw", "CapApp", "run1")
                .expect("restore")
        };
        assert_eq!(restore(&t), saved);
        assert_eq!(mode(&git.join("config")), 0o600);
        assert_eq!(mode(&git.join("HEAD")), 0o644);
        assert_eq!(mode(&git.join("private/secret")), 0o600);
        assert_eq!(mode(&git.join("objects/ab/cdef")), 0o444);
        assert_eq!(mode(&git.join("private")), 0o700);
        assert_eq!(mode(&git.join("objects/ab")), 0o755);
        assert_eq!(mode(&git.join("objects")), 0o755);
        assert_eq!(mode(&git), 0o700, "the root keeps its recorded mode");

        // No modes record: owner-only, never wider.
        std::fs::remove_file(repo.parent().expect("run dir").join("modes")).expect("rm modes");
        assert_eq!(restore(&t), saved);
        assert_eq!(mode(&git), 0o700);
        assert_eq!(mode(&git.join("private")), 0o700);
        assert_eq!(mode(&git.join("objects/ab")), 0o700);
        for rel in ["HEAD", "config", "private/secret", "objects/ab/cdef"] {
            assert_eq!(mode(&git.join(rel)), 0o600, "{rel} without a record");
        }
    }

    /// 27-31 (security review 3): a directory is given its mode through an
    /// open of the directory itself, never by path. A symlink, a regular file
    /// or a directory the current account does not own is refused and nothing
    /// is changed.
    #[cfg(unix)]
    #[test]
    fn local_dir_mode_is_set_on_the_directory_itself() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).expect("metadata").permissions().mode() & 0o777;
        let dir = tempfile::tempdir().expect("dir");
        let real = dir.path().join("real");
        std::fs::create_dir(&real).expect("real");
        set_dir_mode_no_follow(&real, 0o750).expect("a directory of ours");
        assert_eq!(mode(&real), 0o750);

        // a regular file is not a directory: refused, mode untouched
        let file = dir.path().join("file");
        std::fs::write(&file, b"x").expect("file");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).expect("chmod");
        assert!(set_dir_mode_no_follow(&file, 0o777).is_err());
        assert_eq!(mode(&file), 0o600, "a file is never chmod-ed");

        // a symlink swapped in for the directory is not followed
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).expect("link");
        assert!(set_dir_mode_no_follow(&link, 0o777).is_err());
        assert_eq!(mode(&real), 0o750, "the link's target is untouched");
    }

    /// 27-31 (WR-B1): an advanced file never gets a wider mode than the file
    /// it replaces; a new `config` is owner-only, any other new file `0644`,
    /// and a new directory takes its parent directory's mode.
    #[cfg(unix)]
    #[test]
    fn local_git_write_keeps_target_modes() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).expect("metadata").permissions().mode() & 0o777;
        let set = |p: &Path, m: u32| {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(m)).expect("chmod")
        };
        let dir = data_with_projects();
        let git = dir.path().join("projects/CapApp/.git");
        std::fs::create_dir_all(git.join("refs/heads")).expect("refs");
        std::fs::write(git.join("HEAD"), b"ref: refs/heads/production\n").expect("HEAD");
        std::fs::write(git.join("config"), b"[core]\n").expect("config");
        std::fs::write(git.join("refs/heads/production"), b"0000\n").expect("ref");
        set(&git.join("HEAD"), 0o600);
        set(&git.join("config"), 0o600);
        set(&git.join("refs/heads/production"), 0o640);
        set(&git.join("refs"), 0o750);
        set(&git.join("refs/heads"), 0o750);
        let t = host(dir.path());
        let hash = "a".repeat(40);
        let writes = vec![
            GitWrite {
                name: GitWriteName::Head,
                bytes: b"ref: refs/heads/other\n".to_vec(),
            },
            GitWrite {
                name: GitWriteName::Config,
                bytes: b"[core]\n[remote \"origin\"]\n".to_vec(),
            },
            GitWrite {
                name: GitWriteName::BranchRef("production".to_string()),
                bytes: b"1111\n".to_vec(),
            },
            GitWrite {
                name: GitWriteName::BranchRef("feature/x".to_string()),
                bytes: b"2222\n".to_vec(),
            },
            GitWrite {
                name: GitWriteName::PackFile(hash),
                bytes: vec![7u8; 16],
            },
        ];
        t.write_git_files_blocking("CapApp", &writes)
            .expect("write");
        assert_eq!(mode(&git.join("HEAD")), 0o600, "HEAD stays owner-only");
        assert_eq!(mode(&git.join("config")), 0o600, "config stays owner-only");
        assert_eq!(
            mode(&git.join("refs/heads/production")),
            0o640,
            "a 0640 ref is not widened to 0644"
        );
        assert_eq!(
            mode(&git.join("refs/heads/feature/x")),
            0o644,
            "a new ref is 0644"
        );
        assert_eq!(
            mode(&git.join("refs/heads/feature")),
            0o750,
            "a new directory takes its parent's mode"
        );
        assert_eq!(
            std::fs::read(git.join("refs/heads/production")).expect("ref"),
            b"1111\n"
        );

        // a config that is absent is created owner-only
        std::fs::remove_file(git.join("config")).expect("rm config");
        t.write_git_files_blocking(
            "CapApp",
            &[GitWrite {
                name: GitWriteName::Config,
                bytes: b"[core]\n".to_vec(),
            }],
        )
        .expect("new config");
        assert_eq!(mode(&git.join("config")), 0o600, "a new config is 0600");
    }

    /// CodeRabbit #56: two saves of one run into a run directory that already
    /// exists. When both pass the existence check, the one whose `mkdir` of
    /// `repo` fails must leave the other's save alone: exactly one succeeds
    /// and its save is there afterwards, whole.
    #[test]
    fn local_git_save_race_keeps_the_winners_save() {
        let dir = data_with_projects();
        let git = dir.path().join("projects/CapApp/.git");
        std::fs::create_dir_all(git.join("objects/ab")).expect("objects");
        std::fs::write(git.join("HEAD"), b"ref: refs/heads/main\n").expect("HEAD");
        std::fs::write(git.join("config"), b"[core]\n").expect("config");
        for i in 0..8 {
            std::fs::write(git.join("objects/ab").join(format!("o{i}")), [i as u8; 64])
                .expect("object");
        }
        let t = host(dir.path());
        for round in 0..100 {
            let run = format!("race-{round}");
            let repo = t.git_save_repo("gw", "CapApp", &run);
            std::fs::create_dir_all(repo.parent().expect("run dir")).expect("run dir");
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
            let racers: Vec<_> = (0..2)
                .map(|_| {
                    let (t, run, barrier) = (t.clone(), run.clone(), barrier.clone());
                    std::thread::spawn(move || {
                        barrier.wait();
                        t.save_git_blocking("gw", "CapApp", &run)
                    })
                })
                .collect();
            let results: Vec<_> = racers
                .into_iter()
                .map(|r| r.join().expect("racer"))
                .collect();
            let won: Vec<&GitManifest> = results.iter().filter_map(|r| r.as_ref().ok()).collect();
            assert_eq!(won.len(), 1, "round {round}: {results:?}");
            assert!(repo.is_dir(), "round {round}: the winner's save is gone");
            assert_eq!(
                &t.manifest_blocking("read", &repo).expect("manifest"),
                won[0],
                "round {round}"
            );
        }
    }

    /// CodeRabbit #56: a lock another writer takes after the preflight makes
    /// this write's exclusive create fail, and that writer's lock stays. The
    /// other writer here waits for `index.lock` (the first write of the pass,
    /// so the preflight is over) and then takes `HEAD.lock`.
    #[test]
    fn local_git_write_leaves_a_lock_it_failed_to_create() {
        let dir = data_with_projects();
        let git = dir.path().join("projects/CapApp/.git");
        std::fs::create_dir_all(&git).expect("git");
        std::fs::write(git.join("HEAD"), b"ref: refs/heads/main\n").expect("HEAD");
        let t = host(dir.path());
        let writes = vec![
            GitWrite {
                name: GitWriteName::Index,
                bytes: vec![1u8; 64 * 1024 * 1024],
            },
            GitWrite {
                name: GitWriteName::Head,
                bytes: b"ref: refs/heads/other\n".to_vec(),
            },
        ];
        let (index_lock, head_lock) = (git.join("index.lock"), git.join("HEAD.lock"));
        let index = git.join("index");
        let other = std::thread::spawn(move || {
            while std::fs::symlink_metadata(&index_lock).is_err()
                && std::fs::symlink_metadata(&index).is_err()
            {
                std::hint::spin_loop();
            }
            create_new_file(&head_lock, 0o644).expect("the other writer's lock");
        });
        let err = t
            .write_git_files_blocking("CapApp", &writes)
            .expect_err("HEAD.lock is held");
        other.join().expect("other writer");
        assert!(err.to_string().contains("HEAD"), "{err}");
        assert!(
            git.join("HEAD.lock").is_file(),
            "the other writer's lock is left alone"
        );
        assert_eq!(
            std::fs::read(git.join("HEAD")).expect("HEAD"),
            b"ref: refs/heads/main\n"
        );
    }

    /// CodeRabbit #56: a symlinked directory above a `.git` file the state
    /// read or the digest opens refuses, naming the level; nothing behind the
    /// link is read as the repo's. `refs` and `refs/heads` (the roots of the
    /// loose-ref walk), `logs` (above `logs/HEAD`) and a directory inside
    /// `refs/heads` above a branch ref.
    #[cfg(unix)]
    #[tokio::test]
    async fn local_git_reads_refuse_a_symlinked_parent_level() {
        let outside = tempfile::tempdir().expect("outside");
        std::fs::create_dir_all(outside.path().join("heads")).expect("heads");
        std::fs::write(outside.path().join("heads/stolen"), "1111\n").expect("ref");
        std::fs::write(outside.path().join("HEAD"), "0 1 x\n").expect("log");
        std::fs::write(outside.path().join("b"), "2222\n").expect("leaf");
        for (level, target) in [
            ("refs/heads", outside.path().join("heads")),
            ("refs", outside.path().to_path_buf()),
            ("logs", outside.path().to_path_buf()),
        ] {
            let dir = data_with_projects();
            let git = dir.path().join("projects/CapApp/.git");
            std::fs::create_dir_all(git.join("refs/heads")).expect("refs");
            std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").expect("HEAD");
            let at = git.join(level);
            if at.exists() {
                std::fs::remove_dir_all(&at).expect("rm level");
            }
            std::os::unix::fs::symlink(&target, &at).expect("link");
            let err = host(dir.path())
                .read_git_state("CapApp")
                .await
                .expect_err(level);
            assert_eq!(err.code(), "transport_failed", "{level}");
            assert!(err.to_string().contains("symlink"), "{level}: {err}");
        }
        for (level, name) in [
            ("refs/heads/a", GitWriteName::BranchRef("a/b".to_string())),
            ("refs", GitWriteName::BranchRef("b".to_string())),
        ] {
            let dir = data_with_projects();
            let git = dir.path().join("projects/CapApp/.git");
            std::fs::create_dir_all(git.join("refs/heads")).expect("refs");
            let at = git.join(level);
            if at.exists() {
                std::fs::remove_dir_all(&at).expect("rm level");
            }
            let target = if level == "refs" {
                std::fs::create_dir_all(outside.path().join("r/heads")).expect("r");
                std::fs::write(outside.path().join("r/heads/b"), "2222\n").expect("leaf");
                outside.path().join("r")
            } else {
                outside.path().to_path_buf()
            };
            std::os::unix::fs::symlink(&target, &at).expect("link");
            let err = host(dir.path())
                .git_file_digest("CapApp", &name)
                .await
                .expect_err(level);
            assert!(err.to_string().contains("symlink"), "{level}: {err}");
        }
    }

    /// A data dir whose `projects/Plain` holds a `.git` of three files.
    fn data_with_git() -> tempfile::TempDir {
        let dir = data_with_projects();
        let git = dir.path().join("projects/Plain/.git");
        std::fs::create_dir_all(git.join("objects/ab")).expect("dirs");
        std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").expect("head");
        std::fs::write(git.join("config"), "[core]\n").expect("config");
        std::fs::write(git.join("objects/ab/cdef"), "object").expect("object");
        dir
    }

    #[tokio::test]
    async fn local_git_size_counts_files_and_bytes_without_hashing() {
        let dir = data_with_git();
        let t = host(dir.path());
        let size = t.git_size("Plain").await.expect("size");
        assert_eq!(
            size,
            GitSize {
                files: 3,
                bytes: 21 + 7 + 6
            }
        );
        let manifest = t.git_manifest("Plain").await.expect("manifest");
        assert_eq!((size.files, size.bytes), (manifest.files, manifest.bytes));
        // a missing `.git` and a symlink inside both refuse
        let err = t.git_size("Nope").await.expect_err("missing project");
        assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("HEAD", dir.path().join("projects/Plain/.git/l"))
                .expect("symlink");
            let err = t.git_size("Plain").await.expect_err("symlink");
            assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
        }
    }

    #[tokio::test]
    async fn local_git_manifest_digest_is_in_path_byte_order() {
        // `objects-x` sorts before `objects/` bytewise ('-' < '/') although a
        // depth-first walk visits the `objects` directory first; the digest
        // must follow the whole-path order a POSIX `sort` gives.
        let dir = data_with_git();
        let git = dir.path().join("projects/Plain/.git");
        std::fs::write(git.join("objects-x"), "x").expect("file");
        let t = host(dir.path());
        let manifest = t.git_manifest("Plain").await.expect("manifest");
        let mut lines: Vec<(String, String)> = vec![
            ("HEAD".to_string(), sha256_text("ref: refs/heads/main\n")),
            ("config".to_string(), sha256_text("[core]\n")),
            ("objects-x".to_string(), sha256_text("x")),
            ("objects/ab/cdef".to_string(), "6".to_string()),
        ];
        lines.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        let mut digest = Sha256::new();
        for (path, value) in &lines {
            digest.update(path.as_bytes());
            digest.update([0u8]);
            digest.update(value.as_bytes());
            digest.update(*b"\n");
        }
        assert_eq!(manifest.digest, format!("{:x}", digest.finalize()));
    }

    fn sha256_text(text: &str) -> String {
        format!("{:x}", Sha256::digest(text.as_bytes()))
    }

    #[tokio::test]
    async fn local_git_yaml_reads_capped_and_refuses_odd_entries() {
        let dir = data_with_projects();
        let t = host(dir.path());
        assert_eq!(t.read_git_yaml().await.expect("absent"), None);
        let yaml = dir.path().join("git.yaml");
        std::fs::write(&yaml, "repos: {}\n").expect("yaml");
        assert_eq!(
            t.read_git_yaml().await.expect("present"),
            Some(b"repos: {}\n".to_vec())
        );
        std::fs::write(&yaml, vec![b'a'; YAML_MAX_BYTES + 1]).expect("big");
        let err = t.read_git_yaml().await.expect_err("over the cap");
        assert_eq!(err.code(), "deploy_run_invalid", "{err}");
        assert!(err.to_string().contains("git.yaml"), "{err}");
        std::fs::remove_file(&yaml).expect("rm");
        std::fs::create_dir(&yaml).expect("dir");
        let err = t.read_git_yaml().await.expect_err("directory");
        assert_eq!(err.code(), "deploy_run_invalid", "{err}");
        #[cfg(unix)]
        {
            std::fs::remove_dir(&yaml).expect("rm dir");
            std::fs::write(dir.path().join("real.yaml"), "a: 1\n").expect("real");
            std::os::unix::fs::symlink("real.yaml", &yaml).expect("symlink");
            let err = t.read_git_yaml().await.expect_err("symlink");
            assert_eq!(err.code(), "deploy_run_invalid", "{err}");
        }
    }

    #[tokio::test]
    async fn local_git_probe_leaves_nothing_and_refuses_an_existing_probe() {
        let dir = data_with_git();
        let t = host(dir.path());
        t.probe_write("Plain", "20261114T101500Z-ab12")
            .await
            .expect("probe");
        let name = ".ign-probe-20261114T101500Z-ab12";
        assert!(!dir.path().join("projects/Plain").join(name).exists());
        assert!(!dir.path().join("projects/Plain/.git").join(name).exists());
        // an existing probe file is refused and left alone
        std::fs::write(dir.path().join("projects/Plain/.git").join(name), "mine").expect("probe");
        let err = t
            .probe_write("Plain", "20261114T101500Z-ab12")
            .await
            .expect_err("existing probe");
        assert!(err.to_string().contains("projects/Plain/.git"), "{err}");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("projects/Plain/.git").join(name))
                .expect("kept"),
            "mine"
        );
        // names are checked first
        for bad in ["", "..", "a/b"] {
            assert!(
                t.probe_write(bad, "20261114T101500Z-ab12").await.is_err(),
                "{bad:?}"
            );
        }
        assert!(t.probe_write("Plain", "../x").await.is_err());
    }

    /// A path is passed to the Windows free-space query as data in the
    /// environment, never as an argument that `-Command` joins into script.
    #[test]
    fn windows_free_space_path_is_data_not_script() {
        for hostile in [
            "C:\\data'; whoami #",
            "C:\\data$(whoami)",
            "C:\\data`whoami`",
            "C:\\data\" ; calc; \"",
        ] {
            let command = free_space_command(Path::new(hostile));
            let args: Vec<String> = command
                .get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            assert_eq!(args.len(), 4, "{args:?}");
            assert_eq!(args[3], FREE_SPACE_SCRIPT, "the script is the fixed text");
            assert!(
                args.iter()
                    .all(|a| !a.contains("whoami") && !a.contains("calc")),
                "{hostile:?} reached the argv: {args:?}"
            );
            let env: Vec<(String, Option<String>)> = command
                .get_envs()
                .map(|(k, v)| {
                    (
                        k.to_string_lossy().into_owned(),
                        v.map(|v| v.to_string_lossy().into_owned()),
                    )
                })
                .collect();
            assert_eq!(
                env,
                vec![(FREE_PATH_ENV.to_string(), Some(hostile.to_string()))]
            );
        }
        assert!(FREE_SPACE_SCRIPT.contains("$env:IGN_FREE_PATH"));
        assert!(!FREE_SPACE_SCRIPT.contains("$args"));
    }

    #[tokio::test]
    async fn local_free_bytes_reads_the_file_system_of_the_save_root() {
        let dir = data_with_projects();
        let t = host(dir.path());
        // the save root does not exist yet: the nearest ancestor answers
        let free = t.free_bytes("gw-a", "Plain").await.expect("free");
        assert!(free > 0);
        assert!(t.free_bytes("..", "Plain").await.is_err());
        assert!(t.free_bytes("gw-a", "a/b").await.is_err());
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

    /// WR-04: the temporary sibling is named for the run. Another run's
    /// temporary file in the same folder (the old shared name, or one with
    /// another run id) is neither read, removed nor renamed in, and this
    /// run's own is gone afterwards.
    #[test]
    fn local_release_record_write_uses_a_per_run_temporary_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let t = host(dir.path());
        let folder = dir.path().join("ign-deploy/gw/projects/CapApp");
        std::fs::create_dir_all(&folder).expect("folder");
        std::fs::write(folder.join("release.json.ign-new"), b"the shared name").expect("a");
        std::fs::write(folder.join("release.json.tmp-other-run"), b"another run").expect("b");

        let record = record_bytes("gw", "CapApp", "v1.0.0");
        t.write_release_record_blocking("gw", "CapApp", &record)
            .expect("write");

        assert_eq!(
            std::fs::read(folder.join("release.json")).expect("record"),
            record
        );
        assert_eq!(
            std::fs::read(folder.join("release.json.ign-new")).expect("kept"),
            b"the shared name"
        );
        assert_eq!(
            std::fs::read(folder.join("release.json.tmp-other-run")).expect("kept"),
            b"another run"
        );
        let mut names: Vec<String> = std::fs::read_dir(&folder)
            .expect("folder")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "release.json",
                "release.json.ign-new",
                "release.json.tmp-other-run"
            ],
            "this run's temporary file is gone"
        );
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
