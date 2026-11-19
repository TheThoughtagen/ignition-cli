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
//! The reads run on a blocking thread. Every read of a project's `.git`, of a
//! project directory, of a save and of `git.yaml` goes through a [`GitDir`]
//! (27-43): the data directory is opened, then each level without following a
//! link, and every later read is relative to the opened handle, so a level or
//! a file swapped for a link after the walk is refused, never followed. See
//! [`ProjectHost`] for the fail-closed rule: only a clean "no such entry" is
//! absent. The writes (27-44) still check by path.
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
use std::path::Path;

use sha2::{Digest, Sha256};

use super::LocalFsTransport;
use super::git_dir::{GitDir, GitDirError, LockId, Open};
use crate::deploy::apply::run_store::RunId;
use crate::deploy::component_problem;
use crate::deploy::git_advance::{DiskFile, GitWrite, GitWriteName, OldValue, ProjectDisk};
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
    GIT_CHANGED_OPERATION, GitEntry, GitHostRead, GitManifest, GitSize, GitStateWriter, ImportDir,
    ProjectHost, ProjectRunWriter, ReleaseRecordWriter, TagRecordWriter, check_pre_image_write,
    git_entry_location, git_probe_name, project_run_names_problem_on, tag_record_names_problem_on,
};
use crate::deploy::yamlio::YAML_MAX_BYTES;
use crate::error::CoreError;

/// The part of a [`FsError`] a message may show: the I/O kind, or the
/// value-free description.
fn fs_kind(e: &FsError) -> String {
    match e {
        FsError::Io(e) => e.kind().to_string(),
        other => other.describe(),
    }
}

impl LocalFsTransport {
    /// What `<data_dir>/projects/<project>/.git` is, probed with
    /// `symlink_metadata` at every level (nothing is followed).
    fn git_entry_blocking(&self, project: &str) -> Result<GitEntry, CoreError> {
        self.entry_blocking(project, ".git")
    }

    /// What `<data_dir>/projects/<project>/<entry>` is. The data directory is
    /// opened, then `projects` and the project level by level without
    /// following a link, and the entry is classified relative to the opened
    /// project directory (27-43). `entry` is a fixed name (`.git`, or an
    /// [`ImportDir`] name), never outside input. Classification only: nothing
    /// is read through it.
    fn entry_blocking(&self, project: &str, entry: &str) -> Result<GitEntry, CoreError> {
        if let Some(why) = component_problem(project) {
            return Err(self.fail(format!("the project name {why}")));
        }
        // An unreadable host is not an absent `.git`.
        self.require_data_dir()?;
        let root = Dir::root(&self.data_dir).map_err(|e| {
            self.fail(format!(
                "data_dir {} cannot be opened ({})",
                self.data_dir.display(),
                e.describe()
            ))
        })?;

        let projects = match root.open(&["projects"]) {
            Ok(dir) => dir,
            Err(FsError::Changed) => {
                return Err(self.fail(match root.kind(&["projects"]) {
                    Ok(Kind::Symlink) => "projects is a symlink; symlinks are refused",
                    _ => "projects is not a directory",
                }));
            }
            Err(e) => {
                return Err(self.fail(format!(
                    "projects cannot be inspected ({}); is data_dir the gateway data directory?",
                    fs_kind(&e)
                )));
            }
        };

        let project_dir = match projects.open(&[project]) {
            Ok(dir) => dir,
            // No project directory: nothing to be git-managed (the `create`
            // case).
            Err(FsError::Missing) => return Ok(GitEntry::Absent),
            Err(FsError::Changed) => {
                return Err(self.fail(match projects.kind(&[project]) {
                    Ok(Kind::Symlink) => {
                        format!("projects/{project} is a symlink; symlinks are refused")
                    }
                    _ => format!("projects/{project} is not a directory"),
                }));
            }
            // Denied, I/O error and everything else: unknown, not absent. The
            // entry is looked up through the opened project directory, so a
            // project directory that cannot be opened leaves it unknown.
            Err(e) => {
                return Err(self.fail(format!(
                    "projects/{project}/{entry} cannot be inspected: projects/{project} cannot be opened ({})",
                    fs_kind(&e)
                )));
            }
        };

        let location = format!("projects/{project}/{entry}");
        match project_dir.kind(&[entry]) {
            Ok(Kind::Missing) => Ok(GitEntry::Absent),
            Ok(Kind::Symlink) => Ok(GitEntry::Symlink),
            Ok(Kind::Dir) => Ok(GitEntry::Directory),
            Ok(Kind::File) => Ok(GitEntry::File),
            Ok(Kind::Other) => Ok(GitEntry::Other),
            // Denied, I/O error and everything else: unknown, not absent.
            Err(e) => Err(self.fail(format!("{location} cannot be inspected ({})", fs_kind(&e)))),
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
    /// A directory (otherwise a regular file).
    is_dir: bool,
    /// Size of a regular file.
    size: u64,
    /// The source's Unix mode bits (`0` where the platform has none).
    mode: u32,
}

/// Give the open file exactly `mode` (Unix; elsewhere there are no modes to
/// set), through the handle, never a path.
fn set_file_mode(file: &std::fs::File, mode: u32) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = (file, mode);
        Ok(())
    }
}

/// The directory beside a save that a restore fills before it is published.
const RESTORE_STAGE: &str = "restoring";

/// The effective uid of this process, read from the owner of an anonymous
/// temporary file this process just created (no libc call, no extra rustix
/// feature).
#[cfg(unix)]
pub(super) fn current_uid() -> std::io::Result<u32> {
    use std::os::unix::fs::MetadataExt;
    static UID: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    if let Some(uid) = UID.get() {
        return Ok(*uid);
    }
    let uid = tempfile::tempfile()?.metadata()?.uid();
    Ok(*UID.get_or_init(|| uid))
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

    /// The directory chain of a save down to the run directory (every level
    /// above `repo`), opened level by level and none followed (27-43): a
    /// symlink or an entry that is not a directory refuses, naming the level.
    /// A level with nothing at it is the `Err` answer (its name); the restore
    /// and the manifest read it as an error (WR-B6). Every level is held by the returned handle too.
    fn open_save_run(
        &self,
        op: &str,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<Result<GitDir, String>, CoreError> {
        let rel = git_save_rel(gateway, project, run_id);
        let parts: Vec<&str> = rel.split('/').collect();
        match GitDir::open(&self.data_dir, &parts[..parts.len() - 1])
            .map_err(|e| self.git_fail(op, e.to_string()))?
        {
            Open::Found(run) => Ok(Ok(run)),
            Open::Absent { level, .. } => Ok(Err(level)),
        }
    }

    /// The run directory and the `repo` below it, for a caller that needs the
    /// save to be there: an absent level is an error naming it.
    fn open_save(
        &self,
        op: &str,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<(GitDir, GitDir), CoreError> {
        let run = match self.open_save_run(op, gateway, project, run_id)? {
            Ok(run) => run,
            Err(level) => {
                return Err(self.git_fail(
                    op,
                    format!("{level} does not exist; there is no save of run {run_id}"),
                ));
            }
        };
        let location = git_save_location(LedgerRoot::DataDir, gateway, project, run_id);
        match run
            .child("repo")
            .map_err(|e| self.git_fail(op, e.to_string()))?
        {
            Some(repo) => Ok((run, repo.into_root())),
            None => Err(self.git_fail(
                op,
                format!("{location} cannot be inspected (entity not found)"),
            )),
        }
    }

    #[cfg(test)]
    fn git_save_repo(&self, gateway: &str, project: &str, run_id: &str) -> std::path::PathBuf {
        let mut path = self.data_dir.clone();
        for part in git_save_rel(gateway, project, run_id).split('/') {
            path.push(part);
        }
        path
    }

    /// Require `path` to be a real directory: not missing, not a symlink,
    /// not another kind of entry. Advisory (a path check): the reads go
    /// through [`GitDir`], this is for the write side and for names of
    /// ign's own scratch space.
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

    /// The live `.git` of `project`, opened level by level from the data
    /// directory without following a link, and held (27-43). Everything read
    /// of the `.git` is relative to this handle. A missing level is an error.
    fn open_git(&self, op: &str, project: &str) -> Result<GitDir, CoreError> {
        if let Some(why) = component_problem(project) {
            return Err(self.git_fail(op, format!("the project name {why}")));
        }
        self.require_data_dir()?;
        self.open_below_data(op, &["projects", project, ".git"])
    }

    /// `parts` below the data directory as a held [`GitDir`]; a level with
    /// nothing at it is an error naming the path.
    fn open_below_data(&self, op: &str, parts: &[&str]) -> Result<GitDir, CoreError> {
        match GitDir::open(&self.data_dir, parts).map_err(|e| self.git_fail(op, e.to_string()))? {
            Open::Found(dir) => Ok(dir),
            Open::Absent { path, .. } => {
                Err(self.git_fail(op, format!("{path} cannot be inspected (entity not found)")))
            }
        }
    }

    /// A [`GitDirError`] as the transport's own failure for `op`.
    fn gd(&self, op: &str) -> impl Fn(GitDirError) -> CoreError + '_ {
        let op = op.to_string();
        move |e| self.git_fail(&op, e.to_string())
    }

    /// Walk `root` depth-first in name order. A symlink or any entry that is
    /// not a directory or a regular file refuses the walk. Every directory is
    /// opened from the handle of the one above it.
    fn walk_git(&self, op: &str, root: &GitDir) -> Result<Vec<WalkEntry>, CoreError> {
        let mut out = Vec::new();
        self.walk_git_into(op, root, None, &mut out)?;
        Ok(out)
    }

    /// [`walk_git`](Self::walk_git) of a project directory without its `.git`
    /// subtree, which is neither listed nor inspected (27-40).
    fn walk_project(&self, op: &str, root: &GitDir) -> Result<Vec<WalkEntry>, CoreError> {
        let mut out = Vec::new();
        self.walk_git_into(op, root, Some(".git"), &mut out)?;
        Ok(out)
    }

    /// `skip` names an entry of `dir` itself (the walk's root) that the walk
    /// leaves out. `rel` of an entry is its path below the walk's root: the
    /// handle's own `prefix` plus the name, so the root is walked as `dir`
    /// and a child as the handle opened from it.
    fn walk_git_into(
        &self,
        op: &str,
        dir: &GitDir,
        skip: Option<&str>,
        out: &mut Vec<WalkEntry>,
    ) -> Result<(), CoreError> {
        let prefix = dir.prefix();
        let entries = dir.list().map_err(self.gd(op))?;
        for entry in entries {
            let Some(text) = entry.name.to_str() else {
                return Err(self.git_fail(
                    op,
                    format!("{} holds a name that is not UTF-8", shown(prefix)),
                ));
            };
            if prefix.is_empty() && skip.is_some_and(|skipped| text == skipped) {
                continue;
            }
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
            match entry.kind {
                Kind::Symlink => {
                    return Err(self.git_fail(
                        op,
                        format!("{rel} is a symlink; a repo with a symlink is refused"),
                    ));
                }
                Kind::Dir => {
                    out.push(WalkEntry {
                        rel: rel.clone(),
                        is_dir: true,
                        size: 0,
                        mode: entry.mode,
                    });
                    let child = dir.child(text).map_err(self.gd(op))?.ok_or_else(|| {
                        self.git_fail(
                            op,
                            format!("{rel} changed while it was opened (it is gone)"),
                        )
                    })?;
                    self.walk_git_into(op, &child, None, out)?;
                }
                Kind::File => out.push(WalkEntry {
                    rel,
                    is_dir: false,
                    size: entry.size,
                    mode: entry.mode,
                }),
                Kind::Missing | Kind::Other => {
                    return Err(self.git_fail(
                        op,
                        format!("{rel} is neither a directory nor a regular file"),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Hash the regular file `rel` below `root` into a lowercase hex sha256
    /// and the number of bytes read, through a handle opened relative to the
    /// root (never a path the walk remembered).
    fn hash_file(&self, op: &str, root: &GitDir, rel: &str) -> Result<(String, u64), CoreError> {
        super::git_dir::fire(|| format!("hash:{rel}"));
        let Some((mut file, _)) = root.open_file(rel).map_err(self.gd(op))? else {
            return Err(self.git_fail(
                op,
                format!("{rel} changed while it was opened (it is gone)"),
            ));
        };
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 65_536];
        let mut total = 0u64;
        loop {
            let n = file
                .read(&mut buf)
                .map_err(|e| self.git_fail(op, format!("{rel} cannot be read ({})", e.kind())))?;
            if n == 0 {
                break;
            }
            total += n as u64;
            hasher.update(&buf[..n]);
        }
        Ok((format!("{:x}", hasher.finalize()), total))
    }

    /// The manifest of a walked tree, each file hashed through a handle
    /// opened by its path below `root`.
    fn manifest_of(
        &self,
        op: &str,
        root: &GitDir,
        entries: &[WalkEntry],
    ) -> Result<GitManifest, CoreError> {
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
                let (sha, _) = self.hash_file(op, root, &entry.rel)?;
                digest.update(sha.as_bytes());
            }
            digest.update(*b"\n");
        }
        Ok(GitManifest {
            files,
            bytes,
            digest: format!("{:x}", digest.finalize()),
        })
    }

    /// The size and sha256 of every regular file of the project directory but
    /// `.git` (27-40, [`GitHostRead::project_file_digests`]). The directory is
    /// walked, and each file hashed, through handles opened from the data
    /// directory (27-43).
    fn project_file_digests_blocking(&self, project: &str) -> Result<ProjectDisk, CoreError> {
        const OP: &str = "read";
        if let Some(why) = component_problem(project) {
            return Err(self.git_fail(OP, format!("the project name {why}")));
        }
        self.require_data_dir()?;
        let dir = self.open_below_data(OP, &["projects", project])?;
        let entries = self.walk_project(OP, &dir)?;
        let mut disk = ProjectDisk::new();
        for entry in entries.iter().filter(|e| !e.is_dir) {
            let (sha256, size) = self.hash_file(OP, &dir, &entry.rel)?;
            disk.insert(entry.rel.clone(), DiskFile { size, sha256 });
        }
        Ok(disk)
    }

    /// The manifest of the walked directory `root`.
    fn manifest_blocking(&self, op: &str, root: &GitDir) -> Result<GitManifest, CoreError> {
        let entries = self.walk_git(op, root)?;
        self.manifest_of(op, root, &entries)
    }

    /// The manifest of `parts` below the data directory.
    #[cfg(test)]
    fn manifest_at(&self, op: &str, parts: &[&str]) -> Result<GitManifest, CoreError> {
        let root = self.open_below_data(op, parts)?;
        self.manifest_blocking(op, &root)
    }

    /// Copy the walked `entries` of `src` below `to` (already created). The
    /// source side is read through handles opened relative to `src`; the
    /// destination is written through handles too (27-44): each file is
    /// created exclusively in the handle of its own directory, and each
    /// directory is made from the handle of the one above it. The walk is
    /// depth first, so the parent of the next entry is the last directory
    /// made or one above it.
    fn copy_git(
        &self,
        op: &str,
        src: &GitDir,
        entries: &[WalkEntry],
        to: &GitDir,
        mode: &CopyMode<'_>,
    ) -> Result<(), CoreError> {
        let mut stack: Vec<(String, GitDir)> = Vec::new();
        for entry in entries {
            let (parent_rel, name) = entry.rel.rsplit_once('/').unwrap_or(("", &entry.rel));
            while stack.last().is_some_and(|(rel, _)| rel != parent_rel) {
                stack.pop();
            }
            if !parent_rel.is_empty() && stack.is_empty() {
                return Err(
                    self.git_fail(op, format!("{} was walked before its directory", entry.rel))
                );
            }
            let parent: &GitDir = stack.last().map_or(to, |(_, dir)| dir);
            if entry.is_dir {
                let child = parent
                    .create_dir_child(name, mode.dir_mode())
                    .map_err(|e| {
                        self.git_fail(op, format!("{} cannot be created ({e})", entry.rel))
                    })?;
                stack.push((entry.rel.clone(), child));
                continue;
            }
            let Some((mut source, meta)) = src.open_file(&entry.rel).map_err(self.gd(op))? else {
                return Err(self.git_fail(
                    op,
                    format!("{} changed while it was copied; refused", entry.rel),
                ));
            };
            let mut dest = parent
                .create_file(name, mode.file_mode(&entry.rel, meta.mode))
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
            // The umask may have taken bits off at creation; the recorded
            // mode is applied exactly, on the open file.
            if let CopyMode::Restore(set) = mode {
                set_file_mode(&dest, set.file(&entry.rel)).map_err(|e| {
                    self.git_fail(
                        op,
                        format!("{} cannot be given its mode ({})", entry.rel, e.kind()),
                    )
                })?;
            }
        }
        Ok(())
    }

    fn save_git_blocking(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<GitManifest, CoreError> {
        const OP: &str = "write";
        self.git_save_names(gateway, project, run_id)?;
        let source = self.open_git(OP, project)?;
        // Walk first: a symlink anywhere refuses before a directory exists.
        let entries = self.walk_git(OP, &source)?;
        let before = self.manifest_of(OP, &source, &entries)?;

        let root_mode = source.mode().map_err(self.gd(OP))?;
        // Every level from the data directory down to `<run>` is opened from
        // the one above it, or made (`git-save` and below `0700`); the
        // handles are kept (27-44).
        let (mut levels, made) = GitDir::ensure_open(
            &self.data_dir,
            &[
                "ign-deploy",
                gateway,
                "projects",
                project,
                GIT_SAVE_DIR,
                run_id,
            ],
            |index| if index >= 4 { 0o700 } else { 0o755 },
        )
        .map_err(self.gd(OP))?;
        let created_run = made.last().copied().unwrap_or(false);
        #[cfg(test)]
        tests::AFTER_RUN_DIR.with(|hook| {
            if let Some(hook) = hook.borrow_mut().take() {
                hook();
            }
        });
        // `repo` and the `modes` record are removed only when this call made
        // them: a save that loses a race to another save of the run never
        // removes the winner's. The run directories go only when this call
        // made them and they are empty; their own handles are let go first
        // (a held directory cannot be removed on Windows).
        let made_modes: std::cell::Cell<Option<LockId>> = std::cell::Cell::new(None);
        let tidy = |levels: &mut Vec<GitDir>, made_repo: bool| {
            if let Some(run) = levels.last() {
                if made_repo {
                    let _ = run.remove_tree("repo");
                }
                if let Some(id) = made_modes.get() {
                    let _ = run.remove_own_lock(GIT_SAVE_MODES_FILE, id);
                }
            }
            if created_run {
                drop(levels.pop());
                if let Some(save_dir) = levels.last() {
                    let _ = save_dir.remove_empty_dir(run_id);
                }
                drop(levels.pop());
                // Only when this run made it and nothing else lives there.
                if let Some(project_dir) = levels.last() {
                    let _ = project_dir.remove_empty_dir(GIT_SAVE_DIR);
                }
            }
        };
        let exists = |this: &Self| {
            this.git_fail(
                OP,
                format!(
                    "{} already exists; a save is never overwritten",
                    git_save_location(LedgerRoot::DataDir, gateway, project, run_id)
                ),
            )
        };
        let Some(run) = levels.last() else {
            return Err(self.git_fail(OP, "the save directories cannot be opened"));
        };
        match run.kind("repo") {
            Ok(Kind::Missing) => {}
            Ok(_) => {
                tidy(&mut levels, false);
                return Err(exists(self));
            }
            Err(e) => {
                tidy(&mut levels, false);
                return Err(self.git_fail(OP, e.to_string()));
            }
        }
        let repo = match run.create_dir_child("repo", CopyMode::Private.dir_mode()) {
            Ok(repo) => repo.into_root(),
            Err(e) if e.already_exists() => {
                tidy(&mut levels, false);
                return Err(exists(self));
            }
            Err(e) => {
                tidy(&mut levels, false);
                return Err(
                    self.git_fail(OP, format!("the save directory cannot be created ({e})"))
                );
            }
        };
        let outcome = (|| {
            self.copy_git(OP, &source, &entries, &repo, &CopyMode::Private)?;
            let after = self.manifest_blocking(OP, &repo)?;
            if after != before {
                return Err(self.git_fail(
                    OP,
                    "the saved copy does not match the .git it was taken from",
                ));
            }
            // The original modes, beside the private copy.
            let (mut modes, id) = run.create_lock(GIT_SAVE_MODES_FILE, 0o600).map_err(|e| {
                self.git_fail(
                    OP,
                    format!("the modes record cannot be created ({})", e.kind()),
                )
            })?;
            made_modes.set(Some(id));
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
            // The copy's handles go first: a held directory cannot be removed
            // on Windows.
            drop(repo);
            tidy(&mut levels, true);
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
        let (_run, repo) = self.open_save("read", gateway, project, run_id)?;
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
        // The project directory is opened from the data directory and held:
        // the publish lands in this handle (27-44).
        let project_dir = self.open_below_data(OP, &["projects", project])?;
        // Never overwrite: anything at `.git`, a symlink included, refuses.
        // The cheap early refusal; the publish itself can never replace.
        match project_dir.kind(".git") {
            Ok(Kind::Missing) => {}
            Ok(_) => {
                return Err(self.git_fail(
                    OP,
                    format!("projects/{project}/.git exists; it is never replaced"),
                ));
            }
            Err(e) => {
                return Err(self.git_fail(
                    OP,
                    format!("projects/{project}/.git cannot be inspected ({e})"),
                ));
            }
        }
        let (run, saved_repo) = self.open_save(OP, gateway, project, run_id)?;
        let entries = self.walk_git(OP, &saved_repo)?;
        let saved = self.manifest_of(OP, &saved_repo, &entries)?;

        // Copy beside the save, then rename into place: `.git` is never seen
        // half written.
        match run.kind(RESTORE_STAGE).map_err(self.gd(OP))? {
            Kind::Missing => {}
            Kind::Dir => {
                let _ = run.remove_tree(RESTORE_STAGE);
            }
            _ => {
                return Err(self.git_fail(OP, "restoring is not a directory; it is left alone"));
            }
        }
        let modes = self.read_save_modes(&run);
        let mode = CopyMode::Restore(&modes);
        let outcome = (|| {
            // The stage's handles live only inside this block: a held
            // directory could not be renamed on Windows, and nothing may
            // refer to the stage by a name looked at earlier.
            let staged = {
                let stage = run
                    .create_dir_child(RESTORE_STAGE, mode.dir_mode())
                    .map_err(|e| {
                        self.git_fail(OP, format!("the staging directory cannot be created ({e})"))
                    })?
                    .into_root();
                self.copy_git(OP, &saved_repo, &entries, &stage, &mode)?;
                let staged = self.manifest_blocking(OP, &stage)?;
                if staged != saved {
                    return Err(self.git_fail(
                        OP,
                        "the restored copy does not match the save; nothing was restored",
                    ));
                }
                // Directories get their recorded modes now that they are
                // filled, deepest first, the root last, each on its own
                // handle.
                for entry in entries.iter().rev().filter(|e| e.is_dir) {
                    let dir = stage.at(&entry.rel).map_err(self.gd(OP))?.ok_or_else(|| {
                        self.git_fail(
                            OP,
                            format!("{} changed while it was opened (it is gone)", entry.rel),
                        )
                    })?;
                    dir.set_mode_self(modes.dir(&entry.rel)).map_err(|e| {
                        self.git_fail(OP, format!("{} cannot be given its mode ({e})", entry.rel))
                    })?;
                }
                stage.set_mode_self(modes.dir(".")).map_err(|e| {
                    self.git_fail(
                        OP,
                        format!("the restored .git cannot be given its mode ({e})"),
                    )
                })?;
                staged
            };
            let _ = staged;
            super::git_dir::fire(|| format!("publish:{project}"));
            // The publish never replaces (C12): a `.git` that appeared, an
            // empty directory included, is kept. A host that cannot refuse to
            // replace fails closed: "the host cannot publish without
            // replacing", and there is no plain-rename fallback.
            run.publish_noreplace(RESTORE_STAGE, &project_dir, ".git")
                .map_err(|e| {
                    if e.already_exists() {
                        self.git_fail(
                            OP,
                            format!("projects/{project}/.git appeared; it is never replaced"),
                        )
                    } else {
                        self.git_fail(OP, e.to_string())
                    }
                })?;
            let restored = self.open_git(OP, project)?;
            self.manifest_blocking(OP, &restored)
        })();
        if outcome.is_err() {
            // Only the stage goes: never `.git`, never the save.
            let _ = run.remove_tree(RESTORE_STAGE);
        }
        outcome
    }

    /// The modes recorded beside the save, in the run directory `run`.
    /// Anything unreadable, missing, a symlink or over the cap is "no
    /// record": owner-only. The file is read through the held run directory.
    fn read_save_modes(&self, run: &GitDir) -> ModeSet {
        let Ok(Some((mut file, meta))) = run.open_file(GIT_SAVE_MODES_FILE) else {
            return ModeSet::owner_only();
        };
        if meta.size > GIT_SAVE_MODES_MAX {
            return ModeSet::owner_only();
        }
        let mut text = String::new();
        match file.read_to_string(&mut text) {
            Ok(_) => ModeSet::parse(&text),
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
        // Every level above `repo` must be a real directory or absent; the
        // removal is made through the handles of the run directory and of
        // the one above it (27-44).
        let rel = git_save_rel(gateway, project, run_id);
        let parts: Vec<&str> = rel.split('/').collect();
        let Some(save_parts) = parts.len().checked_sub(2).map(|n| &parts[..n]) else {
            return Err(self.git_fail(OP, "the save has no run directory"));
        };
        let save_dir = match GitDir::open(&self.data_dir, save_parts).map_err(self.gd(OP))? {
            Open::Found(dir) => dir,
            Open::Absent { .. } => return Ok(()),
        };
        {
            let Some(run) = save_dir.child(run_id).map_err(self.gd(OP))? else {
                return Ok(());
            };
            match run
                .kind("repo")
                .map_err(|e| self.git_fail(OP, format!("the save cannot be inspected ({e})")))?
            {
                Kind::Missing => {}
                Kind::Dir => {
                    run.remove_tree("repo").map_err(|e| {
                        self.git_fail(OP, format!("the save cannot be removed ({e})"))
                    })?;
                }
                _ => {
                    return Err(self.git_fail(OP, "the save is not a directory; it is left alone"));
                }
            }
            // The modes record goes too.
            if run
                .kind(GIT_SAVE_MODES_FILE)
                .is_ok_and(|kind| kind == Kind::File)
            {
                let _ = run.unlink_file(GIT_SAVE_MODES_FILE);
            }
        }
        // The now-empty run directory goes too; a non-empty one stays.
        let _ = save_dir.remove_empty_dir(run_id);
        Ok(())
    }

    /// Write `writes` inside the project's existing `.git`, every one
    /// through handles opened from the held `.git` (27-44, CodeRabbit #56
    /// C2): each target's parent is opened (or made) level by level from that
    /// handle, the lock is created exclusively in the parent's handle without
    /// following a link, and the publish is a `renameat` inside that one
    /// handle. A parent swapped for a link after the preflight is refused and
    /// nothing is created behind it.
    fn write_git_files_blocking(
        &self,
        project: &str,
        writes: &[GitWrite],
    ) -> Result<(), CoreError> {
        const OP: &str = "write";
        let git = self.open_git(OP, project)?;
        // Every shape and every lock is checked before the first byte is
        // written, so a refusal leaves the repo as it was. The checks are
        // made on the held handle, the same anchor the writes use.
        struct Planned<'a> {
            rel: String,
            parents: Vec<String>,
            name: String,
            temp: String,
            write: &'a GitWrite,
        }
        let mut plan: Vec<Planned<'_>> = Vec::new();
        for write in writes {
            write
                .name
                .check_shape()
                .map_err(|why| self.git_fail(OP, format!("a .git write is refused: {why}")))?;
            let rel = write.name.rel();
            let mut parents: Vec<String> = rel.split('/').map(str::to_string).collect();
            let name = parents.pop().unwrap_or_default();
            let temp_suffix = match write.name {
                GitWriteName::PackFile(_) | GitWriteName::PackIndex(_) => ".ign-tmp",
                _ => ".lock",
            };
            let temp = format!("{name}{temp_suffix}");
            let temp_rel = if parents.is_empty() {
                temp.clone()
            } else {
                format!("{}/{temp}", parents.join("/"))
            };
            // Existing parents must be real directories (the kind lookup
            // opens each one without following it).
            match git.kind(&temp_rel).map_err(self.gd(OP))? {
                Kind::Missing => {}
                _ => {
                    return Err(self.git_fail(
                        OP,
                        format!("{rel}{temp_suffix} exists; another writer holds it, nothing was written"),
                    ));
                }
            }
            match git.kind(&rel).map_err(self.gd(OP))? {
                Kind::Missing | Kind::File => {}
                _ => {
                    return Err(
                        self.git_fail(OP, format!("{rel} is not a regular file; it is left alone"))
                    );
                }
            }
            plan.push(Planned {
                rel,
                parents,
                name,
                temp,
                write,
            });
        }
        // The levels this pass makes, so a failure can take the empty ones
        // away again.
        let mut made: Vec<String> = Vec::new();
        let outcome = (|| -> Result<(), CoreError> {
            for item in &plan {
                super::git_dir::fire(|| format!("write:{}", item.rel));
                let owned;
                let parent: &GitDir = if item.parents.is_empty() {
                    &git
                } else {
                    let parts: Vec<&str> = item.parents.iter().map(String::as_str).collect();
                    owned = git.ensure_parent(&parts, &mut made).map_err(self.gd(OP))?;
                    &owned
                };
                // WR-B1: the mode of the file this replaces, never wider (and
                // never an execute bit); a new `config` is owner-only (it can
                // hold a remote URL with userinfo), any other new file `0644`.
                let file_mode = match parent.stat(&item.name).map_err(self.gd(OP))? {
                    Some(found) if found.kind == Kind::File => found.mode & 0o666,
                    Some(_) => {
                        return Err(self.git_fail(
                            OP,
                            format!("{} is not a regular file; it is left alone", item.rel),
                        ));
                    }
                    None if matches!(item.write.name, GitWriteName::Config) => 0o600,
                    None => 0o644,
                };
                // A lock this call failed to create is another writer's: only
                // a failure after the create removes it, and then only the
                // file this call created.
                let (mut file, id) = match parent.create_lock(&item.temp, file_mode) {
                    Ok(created) => created,
                    Err(e) => {
                        return Err(self.git_fail(
                            OP,
                            format!("{} cannot be written ({})", item.rel, e.kind()),
                        ));
                    }
                };
                super::git_dir::fire(|| format!("locked:{}", item.rel));
                // 27-45 (F4): under the lock, the target must still be what
                // the run read. A mismatch removes only this call's lock
                // (the file is closed first) and keeps every earlier write.
                if let Err(changed) =
                    self.git_old_holds(&git, parent, &item.name, &item.rel, item.write)
                {
                    drop(file);
                    let _ = parent.remove_own_lock(&item.temp, id);
                    return Err(changed);
                }
                let result = (|| -> std::io::Result<()> {
                    // the umask may have narrowed the create; the file gets
                    // exactly the mode it replaces (an fchmod on the open file)
                    set_file_mode(&file, file_mode)?;
                    file.write_all(&item.write.bytes)?;
                    file.sync_all()?;
                    drop(file);
                    parent.publish(&item.temp, &item.name)
                })();
                if let Err(e) = result {
                    let _ = parent.remove_own_lock(&item.temp, id);
                    return Err(
                        self.git_fail(OP, format!("{} cannot be written ({})", item.rel, e.kind()))
                    );
                }
            }
            Ok(())
        })();
        if outcome.is_err() {
            for rel in made.iter().rev() {
                let _ = git.remove_empty_dir(rel);
            }
        }
        outcome
    }

    /// The sha256 of the regular file opened at `rel` below `dir`, streamed
    /// from the handle the open returned; `None` when it is absent.
    fn git_open_digest(&self, dir: &GitDir, rel: &str) -> Result<Option<String>, CoreError> {
        let Some((mut file, _)) = dir.open_file(rel).map_err(self.gd("write"))? else {
            return Ok(None);
        };
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 65_536];
        loop {
            let n = file
                .read(&mut buf)
                .map_err(|e| self.fail(format!("{rel} cannot be read ({})", e.kind())))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(Some(format!("{:x}", hasher.finalize())))
    }

    /// Whether `expected` holds for `found`, the digest of what is there now.
    fn old_holds(expected: &OldValue, found: Option<&str>) -> bool {
        match expected {
            OldValue::Unchecked => true,
            OldValue::Absent => found.is_none(),
            OldValue::Sha256(want) => found == Some(want.as_str()),
        }
    }

    /// The compare of 27-45 (F4), made while this call holds the target's
    /// lock: the target is reopened `O_NOFOLLOW` relative to `parent`, the
    /// handle its write goes through, and hashed; a branch that lived only in
    /// `packed-refs` also has `packed-refs` reopened relative to the held
    /// `.git`. A target that is no longer what the run read is a
    /// [`GIT_CHANGED_OPERATION`] failure naming the file and no value.
    fn git_old_holds(
        &self,
        git: &GitDir,
        parent: &GitDir,
        name: &str,
        rel: &str,
        write: &GitWrite,
    ) -> Result<(), CoreError> {
        let changed = |file: &str, extra: &str| {
            self.git_fail(
                GIT_CHANGED_OPERATION,
                format!("{file} changed after the run read it{extra}; it is left as it is"),
            )
        };
        if write.old != OldValue::Unchecked {
            let found = self.git_open_digest(parent, name)?;
            if !Self::old_holds(&write.old, found.as_deref()) {
                return Err(changed(rel, ""));
            }
        }
        if let Some(packed) = &write.packed_refs_old {
            let found = self.git_open_digest(git, "packed-refs")?;
            if !Self::old_holds(packed, found.as_deref()) {
                return Err(changed("packed-refs", &format!(" (it listed {rel})")));
            }
        }
        Ok(())
    }

    /// Every loose ref below `dir` (named `prefix`), at most one past the
    /// cap so the caller's cap check names the excess. Each level is opened
    /// from the one above it and each ref read through a handle opened from
    /// its directory (27-43).
    fn collect_loose_refs(
        &self,
        dir: &GitDir,
        prefix: &str,
        out: &mut Vec<(String, Vec<u8>)>,
    ) -> Result<(), CoreError> {
        for entry in dir.list().map_err(self.gd("read"))? {
            if out.len() > GIT_LOOSE_REFS_MAX {
                return Ok(());
            }
            let Some(text) = entry.name.to_str() else {
                return Err(self.fail(format!("{prefix} holds a name that is not UTF-8")));
            };
            let rel = format!("{prefix}/{text}");
            match entry.kind {
                Kind::Symlink => {
                    return Err(self.fail(format!("{rel} is a symlink; symlinks are refused")));
                }
                Kind::Dir => {
                    let child = dir.child(text).map_err(self.gd("read"))?.ok_or_else(|| {
                        self.fail(format!("{rel} changed while it was opened (it is gone)"))
                    })?;
                    self.collect_loose_refs(&child, &rel, out)?;
                }
                Kind::File => {
                    let bytes = dir
                        .read_small(text, GIT_SMALL_FILE_MAX)
                        .map_err(self.gd("read"))?
                        .ok_or_else(|| {
                            self.fail(format!("{rel} changed while it was opened (it is gone)"))
                        })?;
                    out.push((rel, bytes));
                }
                Kind::Missing | Kind::Other => {
                    return Err(self.fail(format!("{rel} is not a regular file")));
                }
            }
        }
        Ok(())
    }

    fn read_git_state_blocking(&self, project: &str) -> Result<GitFiles, CoreError> {
        let kind = self.git_entry_blocking(project)?;
        if kind != GitEntry::Directory {
            return Ok(GitFiles::without_contents(kind));
        }
        let git = self.open_git("read", project)?;
        let rd = self.gd("read");
        let mut files = GitFiles::without_contents(GitEntry::Directory);
        files.head = git.read_small("HEAD", GIT_SMALL_FILE_MAX).map_err(&rd)?;
        files.config = git.read_small("config", GIT_SMALL_FILE_MAX).map_err(&rd)?;
        files.packed_refs = git
            .read_small("packed-refs", GIT_PACKED_REFS_MAX)
            .map_err(&rd)?;
        files.index_present = git.present("index").map_err(&rd)?;
        for marker in GIT_MARKERS {
            if git.present(marker).map_err(&rd)? {
                files.markers.push((*marker).to_string());
            }
        }
        for prefix in ["refs/heads", "refs/remotes"] {
            if let Some(dir) = git.at(prefix).map_err(&rd)? {
                self.collect_loose_refs(&dir, prefix, &mut files.loose_refs)?;
            }
        }
        files.head_log_tail = self.read_log_tail(&git)?;
        Ok(files)
    }

    /// The last [`GIT_LOG_TAIL_MAX`] bytes of `logs/HEAD`, read from the
    /// handle the open returned.
    fn read_log_tail(&self, git: &GitDir) -> Result<Option<Vec<u8>>, CoreError> {
        let Some((mut file, meta)) = git.open_file("logs/HEAD").map_err(self.gd("read"))? else {
            return Ok(None);
        };
        let len = meta.size;
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

    /// `<data_dir>/git.yaml`, capped; `None` only when it does not exist. The
    /// data directory is opened first and the file read from that handle.
    fn read_git_yaml_blocking(&self) -> Result<Option<Vec<u8>>, CoreError> {
        let invalid = |problem: String| CoreError::DeployRunInvalid {
            run_id: "unknown".to_string(),
            problem: format!("git.yaml: {problem}"),
        };
        self.require_data_dir()?;
        let root = self.open_below_data("read", &[])?;
        match root.open_file("git.yaml") {
            Ok(None) => Ok(None),
            Err(e) if e.is_refusal() => Err(invalid(
                "is not a regular file, or changed while it was opened (symlinks are refused)"
                    .to_string(),
            )),
            Err(e) => Err(self.fail(e.to_string())),
            Ok(Some((_, meta))) if meta.size > YAML_MAX_BYTES as u64 => Err(invalid(format!(
                "is {} bytes, over the {YAML_MAX_BYTES} byte cap",
                meta.size
            ))),
            Ok(Some((file, _))) => {
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

    /// The manifest of the project's live `.git`, walked and hashed through
    /// the held handle.
    fn git_manifest_blocking(&self, project: &str) -> Result<GitManifest, CoreError> {
        let git = self.open_git("read", project)?;
        self.manifest_blocking("read", &git)
    }

    /// The sha256 of one `.git` file, streamed from a handle opened relative
    /// to the held `.git`; `None` when it, or a level above it, is absent.
    fn git_file_digest_blocking(
        &self,
        project: &str,
        name: &GitWriteName,
    ) -> Result<Option<String>, CoreError> {
        name.check_shape()
            .map_err(|why| self.fail(format!("a .git file name is refused: {why}")))?;
        let git = self.open_git("read", project)?;
        let rel = name.rel();
        let Some((mut file, _)) = git.open_file(&rel).map_err(self.gd("read"))? else {
            return Ok(None);
        };
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 65_536];
        loop {
            let n = file
                .read(&mut buf)
                .map_err(|e| self.fail(format!("{rel} cannot be read ({})", e.kind())))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(Some(format!("{:x}", hasher.finalize())))
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
        let git = self.open_git("read", project)?;
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
    /// reported as one (and [`GitObjectList::from_entries`] refuses it). Each
    /// level is opened from the one above it.
    fn list_git_objects_blocking(&self, project: &str) -> Result<GitObjectList, CoreError> {
        const OP: &str = "read";
        let git = self.open_git(OP, project)?;
        let objects = git.child("objects").map_err(self.gd(OP))?.ok_or_else(|| {
            self.git_fail(
                OP,
                format!("projects/{project}/.git/objects cannot be inspected (entity not found)"),
            )
        })?;
        let mut entries: Vec<GitObjectEntry> = Vec::new();
        let kind_of = |kind: Kind| match kind {
            Kind::Symlink => GitObjectEntryKind::Symlink,
            Kind::File => GitObjectEntryKind::File,
            _ => GitObjectEntryKind::Other,
        };
        for top in objects.list().map_err(self.gd(OP))? {
            // A name that is not UTF-8 cannot be one of the fixed shapes.
            let Some(top_name) = top.name.to_str() else {
                continue;
            };
            entries.push(GitObjectEntry {
                rel: format!("objects/{top_name}"),
                size: top.size,
                kind: kind_of(top.kind),
            });
            if top.kind != Kind::Dir {
                continue;
            }
            let sub = objects
                .child(top_name)
                .map_err(self.gd(OP))?
                .ok_or_else(|| {
                    self.git_fail(
                        OP,
                        format!("objects/{top_name} changed while it was opened (it is gone)"),
                    )
                })?;
            for inner in sub.list().map_err(self.gd(OP))? {
                let Some(name) = inner.name.to_str() else {
                    continue;
                };
                if entries.len() >= GIT_OBJECTS_LIST_MAX_ENTRIES {
                    return Err(self.git_fail(
                        OP,
                        format!("objects/ holds more than {GIT_OBJECTS_LIST_MAX_ENTRIES} entries"),
                    ));
                }
                entries.push(GitObjectEntry {
                    rel: format!("objects/{top_name}/{name}"),
                    size: inner.size,
                    kind: kind_of(inner.kind),
                });
            }
        }
        GitObjectList::from_entries(&entries)
            .map_err(|why| self.git_fail(OP, format!("the object listing is refused: {why}")))
    }

    /// Copy the listed object `files` into `dest` (see
    /// [`GitHostRead::fetch_git_objects`]). Each object is opened through the
    /// held `.git`, so a level or file swapped for a link after the listing
    /// fails the fetch; the destination is ign's own scratch directory.
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
        let git = self.open_git(OP, project)?;
        self.require_real_dir(OP, dest, "the destination")?;
        for file in files {
            let parts: Vec<&str> = file.rel.split('/').collect();
            // `check_object_rel` proved exactly `objects/<dir>/<name>`.
            let (dir, name) = (parts[1], parts[2]);
            let Some((mut source, meta)) = git.open_file(&file.rel).map_err(self.gd(OP))? else {
                return Err(self.git_fail(
                    OP,
                    format!("{} cannot be opened (entity not found)", file.rel),
                ));
            };
            if meta.size != file.size {
                return Err(self.git_fail(
                    OP,
                    format!(
                        "{} is {} bytes but the listing said {}",
                        file.rel, meta.size, file.size
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
        self.require_data_dir()?;
        // Both places are opened from the data directory without following a
        // link, and the probe is made and removed through those handles
        // (27-44).
        let project_dir = self.open_below_data(OP, &["projects", project])?;
        let git = self.open_below_data(OP, &["projects", project, ".git"])?;
        let name = git_probe_name(run_id);
        let places = [
            (&project_dir, format!("projects/{project}")),
            (&git, format!("projects/{project}/.git")),
        ];
        for (dir, shown) in places {
            let (file, id) = dir.create_lock(&name, 0o600).map_err(|e| {
                self.git_fail(
                    OP,
                    format!("a probe file cannot be created in {shown} ({})", e.kind()),
                )
            })?;
            drop(file);
            dir.remove_own_lock(&name, id).map_err(|e| {
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
        .env(FREE_PATH_ENV, path)
        // The git read token belongs to the two remote git reads only
        // (T-27-41 scope, 27-SECURITY.md auditor note 2).
        .env_remove(crate::deploy::git_remote::GIT_TOKEN_ENV);
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
        tokio::task::spawn_blocking(move || this.git_manifest_blocking(&project))
            .await
            .map_err(|e| CoreError::Internal(format!("local read task failed: {e}")))?
    }

    async fn project_file_digests(&self, project: &str) -> Result<ProjectDisk, CoreError> {
        let this = self.clone();
        let project = project.to_string();
        tokio::task::spawn_blocking(move || this.project_file_digests_blocking(&project))
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
        tokio::task::spawn_blocking(move || this.git_file_digest_blocking(&project, &name))
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

    thread_local! {
        /// Run once on this thread right after a save has its run directory
        /// (a test orders a second save into that point deterministically).
        pub(super) static AFTER_RUN_DIR: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
            std::cell::RefCell::new(None);
    }

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
    /// is changed. (27-44: the primitive is [`GitDir::set_mode_self`] on the
    /// handle opened from the held parent.)
    #[cfg(unix)]
    #[test]
    fn local_dir_mode_is_set_on_the_directory_itself() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).expect("metadata").permissions().mode() & 0o777;
        let dir = tempfile::tempdir().expect("dir");
        let Ok(Open::Found(parent)) = GitDir::open(dir.path(), &[]) else {
            panic!("the temporary directory opens");
        };
        let set = |name: &str, mode: u32| {
            parent.child(name).and_then(|found| match found {
                Some(child) => child.set_mode_self(mode),
                None => Ok(()),
            })
        };
        let real = dir.path().join("real");
        std::fs::create_dir(&real).expect("real");
        set("real", 0o750).expect("a directory of ours");
        assert_eq!(mode(&real), 0o750);

        // a regular file is not a directory: refused, mode untouched
        let file = dir.path().join("file");
        std::fs::write(&file, b"x").expect("file");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).expect("chmod");
        assert!(set("file", 0o777).is_err());
        assert_eq!(mode(&file), 0o600, "a file is never chmod-ed");

        // a symlink swapped in for the directory is not followed
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).expect("link");
        assert!(set("link", 0o777).is_err());
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
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::Head,
                bytes: b"ref: refs/heads/other\n".to_vec(),
            },
            GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::Config,
                bytes: b"[core]\n[remote \"origin\"]\n".to_vec(),
            },
            GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::BranchRef("production".to_string()),
                bytes: b"1111\n".to_vec(),
            },
            GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::BranchRef("feature/x".to_string()),
                bytes: b"2222\n".to_vec(),
            },
            GitWrite {
                old: Default::default(),
                packed_refs_old: None,
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
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::Config,
                bytes: b"[core]\n".to_vec(),
            }],
        )
        .expect("new config");
        assert_eq!(mode(&git.join("config")), 0o600, "a new config is 0600");
    }

    /// CodeRabbit #56 round 2: a save that made the run directory and then
    /// lost `repo` to another save leaves that save's `modes` record alone, so
    /// a restore of the winner puts back the recorded modes, not the
    /// owner-only fallback. The run directory is not made beforehand: save A
    /// makes it, save B runs whole at that point (it finds the run directory,
    /// makes `repo` and writes `modes`), then A finds `repo` and gives up.
    #[cfg(unix)]
    #[test]
    fn local_git_save_that_loses_repo_keeps_the_winners_modes() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).expect("metadata").permissions().mode() & 0o777;
        let dir = data_with_projects();
        let git = dir.path().join("projects/CapApp/.git");
        std::fs::create_dir_all(git.join("objects/ab")).expect("objects");
        std::fs::write(git.join("HEAD"), b"ref: refs/heads/main\n").expect("HEAD");
        std::fs::write(git.join("objects/ab/cdef"), b"o").expect("object");
        std::fs::set_permissions(git.join("HEAD"), std::fs::Permissions::from_mode(0o644))
            .expect("chmod");
        let t = host(dir.path());
        let winner = std::sync::Arc::new(std::sync::Mutex::new(None));
        let (b, seen) = (t.clone(), winner.clone());
        AFTER_RUN_DIR.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                let won = std::thread::spawn(move || b.save_git_blocking("gw", "CapApp", "r1"))
                    .join()
                    .expect("save B");
                *seen.lock().expect("winner") = Some(won);
            }));
        });
        let lost = t.save_git_blocking("gw", "CapApp", "r1");
        let won = winner.lock().expect("winner").take().expect("save B ran");
        let saved = won.expect("save B wins");
        assert!(lost.is_err(), "save A finds repo and gives up");
        let run = t.git_save_repo("gw", "CapApp", "r1");
        let modes = run.parent().expect("run dir").join(GIT_SAVE_MODES_FILE);
        assert!(modes.is_file(), "the winner's modes record is there");
        std::fs::rename(&git, dir.path().join("projects/CapApp/git-aside")).expect("aside");
        let restored = t
            .restore_git_blocking("gw", "CapApp", "r1")
            .expect("restore");
        assert_eq!(restored, saved);
        assert_eq!(
            mode(&git.join("HEAD")),
            0o644,
            "the recorded mode, not owner-only"
        );
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
                &t.manifest_at(
                    "read",
                    &[
                        "ign-deploy",
                        "gw",
                        "projects",
                        "CapApp",
                        "git-save",
                        &run,
                        "repo"
                    ]
                )
                .expect("manifest"),
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
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::Index,
                bytes: vec![1u8; 64 * 1024 * 1024],
            },
            GitWrite {
                old: Default::default(),
                packed_refs_old: None,
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
    async fn local_project_file_digests_skip_git_and_refuse_a_link() {
        let dir = data_with_git();
        let project = dir.path().join("projects/Plain");
        std::fs::create_dir_all(project.join("ignition/x")).expect("dirs");
        std::fs::write(project.join("project.json"), "{}\n").expect("file");
        std::fs::write(project.join("ignition/x/code.py"), "x = 1\n").expect("file");
        let t = host(dir.path());
        let disk = t.project_file_digests("Plain").await.expect("the read");
        let want: BTreeMap<String, (u64, String)> =
            [("ignition/x/code.py", "x = 1\n"), ("project.json", "{}\n")]
                .iter()
                .map(|(p, body)| ((*p).to_string(), (body.len() as u64, sha256_text(body))))
                .collect();
        let got: BTreeMap<String, (u64, String)> = disk
            .iter()
            .map(|(p, f)| (p.clone(), (f.size, f.sha256.clone())))
            .collect();
        assert_eq!(got, want, "everything but .git, with size and sha256");
        assert!(disk.keys().all(|p| !p.starts_with(".git")));
        // A missing project and a bad name refuse, never an empty answer.
        let err = t.project_file_digests("Nope").await.expect_err("missing");
        assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
        let err = t.project_file_digests("../x").await.expect_err("bad name");
        assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
        // A symlink anywhere in the project refuses the read; one named
        // `.git` is not inspected at all.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("code.py", project.join("ignition/x/link"))
                .expect("symlink");
            let err = t.project_file_digests("Plain").await.expect_err("symlink");
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
            // The path is the one value set; the git read token is the one
            // variable removed (27-56, T-27-197).
            let mut want = vec![
                (FREE_PATH_ENV.to_string(), Some(hostile.to_string())),
                (crate::deploy::git_remote::GIT_TOKEN_ENV.to_string(), None),
            ];
            want.sort();
            let mut env = env;
            env.sort();
            assert_eq!(env, want);
        }
        assert!(FREE_SPACE_SCRIPT.contains("$env:IGN_FREE_PATH"));
        assert!(!FREE_SPACE_SCRIPT.contains("$args"));
    }

    /// 27-56 (T-27-197): the PowerShell free-space child does not inherit the
    /// git read token either.
    #[test]
    fn free_space_command_removes_the_git_token() {
        let command = free_space_command(Path::new("C:\\data"));
        let removed = command.get_envs().any(|(key, value)| {
            key == std::ffi::OsStr::new(crate::deploy::git_remote::GIT_TOKEN_ENV) && value.is_none()
        });
        assert!(removed, "free_space_command must env_remove the git token");
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

    // ---- 27-43: handle-anchored reads --------------------------------

    /// Run `act` once, the first time the hook point named `label` is reached
    /// on this thread. The point is "just before a level or a file is opened
    /// for use": whatever the test changes there is a swap after the walk.
    fn at_point(label: &'static str, act: impl FnOnce() + 'static) {
        let act = std::cell::Cell::new(Some(act));
        crate::deploy::transport::local::git_dir::HOOK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move |seen: &str| {
                if seen == label
                    && let Some(act) = act.take()
                {
                    act();
                }
            }));
        });
    }

    fn no_point() {
        crate::deploy::transport::local::git_dir::HOOK.with(|hook| *hook.borrow_mut() = None);
    }

    /// Rename `at` aside and put a symlink to `target` where it was.
    #[cfg(unix)]
    fn swap_for_link(at: &Path, target: &Path) {
        let aside = at.with_file_name(format!(
            "{}.aside",
            at.file_name().expect("name").to_string_lossy()
        ));
        std::fs::rename(at, &aside).expect("rename aside");
        std::os::unix::fs::symlink(target, at).expect("plant link");
    }

    /// A repo `CapApp` with HEAD, a branch and a log, for the swap tests.
    fn data_with_repo() -> tempfile::TempDir {
        let dir = data_with_projects();
        let git = dir.path().join("projects/CapApp/.git");
        std::fs::create_dir_all(git.join("refs/heads")).expect("refs");
        std::fs::create_dir_all(git.join("logs")).expect("logs");
        std::fs::create_dir_all(git.join("objects/pack")).expect("objects");
        std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").expect("HEAD");
        std::fs::write(git.join("config"), "[core]\n").expect("config");
        std::fs::write(git.join("refs/heads/main"), "aaaa\n").expect("main");
        std::fs::write(git.join("logs/HEAD"), "0 1 inside\n").expect("log");
        dir
    }

    /// CodeRabbit #56 C1: a level swapped for a symlink after the walk, and
    /// before the read, fails the read naming the level. Nothing behind the
    /// link is read. Both `refs` (the level above the loose-ref walk) and
    /// `refs/heads` (the walk's own root) are swapped, one per scenario.
    #[cfg(unix)]
    #[test]
    fn local_git_state_refuses_a_level_swapped_after_the_open() {
        for (point, level) in [("level:refs", "refs"), ("level:refs/heads", "refs/heads")] {
            let dir = data_with_repo();
            let git = dir.path().join("projects/CapApp/.git");
            let outside = tempfile::tempdir().expect("outside");
            std::fs::create_dir_all(outside.path().join("heads")).expect("heads");
            std::fs::write(outside.path().join("heads/stolen"), "1111\n").expect("ref");
            let target = if level == "refs" {
                outside.path().to_path_buf()
            } else {
                outside.path().join("heads")
            };
            at_point(point, move || swap_for_link(&git.join(level), &target));
            let result = host(dir.path()).read_git_state_blocking("CapApp");
            no_point();
            let err = result.expect_err(level);
            assert_eq!(err.code(), "transport_failed", "{level}: {err}");
            let text = err.to_string();
            assert!(text.contains(level), "{level}: names the level: {text}");
            assert!(!text.contains("stolen"), "{level}: {text}");
        }
    }

    /// `.git` itself a link at open time: `GitDir::open` refuses with the
    /// 'is a symlink' wording, and `git_entry` still classifies it as a
    /// symlink (the classification did not change).
    #[cfg(unix)]
    #[test]
    fn local_git_dir_refuses_a_linked_git() {
        let dir = data_with_projects();
        let project = dir.path().join("projects/CapApp");
        std::fs::create_dir_all(&project).expect("project");
        let outside = tempfile::tempdir().expect("outside");
        std::os::unix::fs::symlink(outside.path(), project.join(".git")).expect("link");
        match GitDir::open(dir.path(), &["projects", "CapApp", ".git"]) {
            Err(e) => assert_eq!(
                e.to_string(),
                "projects/CapApp/.git is a symlink; symlinks are refused"
            ),
            Ok(_) => panic!("a linked .git must be refused"),
        }
        assert_eq!(
            host(dir.path())
                .git_entry_blocking("CapApp")
                .expect("entry"),
            GitEntry::Symlink
        );
    }

    #[cfg(unix)]
    #[test]
    fn local_git_state_refuses_a_file_swapped_after_the_open() {
        let dir = data_with_repo();
        let git = dir.path().join("projects/CapApp/.git");
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("HEAD"), "ref: refs/heads/stolen\n").expect("head");
        let target = outside.path().join("HEAD");
        at_point("file:HEAD", move || {
            swap_for_link(&git.join("HEAD"), &target)
        });
        let result = host(dir.path()).read_git_state_blocking("CapApp");
        no_point();
        let err = result.expect_err("the swapped file is refused");
        assert_eq!(err.code(), "transport_failed", "{err}");
        assert!(err.to_string().contains("HEAD"), "{err}");
    }

    /// Two swaps of the manifest: a directory swapped while the walk is
    /// entering it, and a directory swapped after the walk and before a file
    /// below it is hashed. The outside file is never hashed.
    #[cfg(unix)]
    #[test]
    fn local_git_manifest_refuses_a_directory_swapped_during_the_walk() {
        let outside = tempfile::tempdir().expect("outside");
        std::fs::create_dir_all(outside.path().join("heads")).expect("heads");
        std::fs::write(outside.path().join("heads/main"), "9999\n").expect("outside ref");
        std::fs::write(outside.path().join("p1"), "outside").expect("outside pack");

        // while the walk enters objects/pack
        let dir = data_with_repo();
        let git = dir.path().join("projects/CapApp/.git");
        std::fs::write(git.join("objects/pack/p1"), "inside").expect("pack");
        let (g, target) = (git.clone(), outside.path().to_path_buf());
        at_point("level:objects/pack", move || {
            swap_for_link(&g.join("objects/pack"), &target);
        });
        let result = host(dir.path()).git_manifest_blocking("CapApp");
        no_point();
        let err = result.expect_err("a level swapped during the walk");
        assert_eq!(err.code(), "transport_failed", "{err}");
        assert!(err.to_string().contains("objects/pack"), "{err}");

        // after the walk, before refs/heads/main is hashed
        let dir = data_with_repo();
        let git = dir.path().join("projects/CapApp/.git");
        let target = outside.path().to_path_buf();
        at_point("hash:refs/heads/main", move || {
            swap_for_link(&git.join("refs"), &target);
        });
        let result = host(dir.path()).git_manifest_blocking("CapApp");
        no_point();
        let err = result.expect_err("a level swapped before the hash");
        assert!(err.to_string().contains("refs"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn local_git_digest_refuses_a_swapped_file() {
        let dir = data_with_repo();
        let git = dir.path().join("projects/CapApp/.git");
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("main"), "9999\n").expect("outside");
        let target = outside.path().join("main");
        at_point("file:refs/heads/main", move || {
            swap_for_link(&git.join("refs/heads/main"), &target);
        });
        let result = host(dir.path())
            .git_file_digest_blocking("CapApp", &GitWriteName::BranchRef("main".into()));
        no_point();
        let err = result.expect_err("a swapped file never yields the outside digest");
        assert_eq!(err.code(), "transport_failed", "{err}");
        assert!(err.to_string().contains("refs/heads/main"), "{err}");
    }

    /// A pack directory swapped for a link, to a directory holding a file of
    /// the same name and size, after the listing: nothing is installed.
    #[cfg(unix)]
    #[test]
    fn local_git_objects_fetch_refuses_a_swapped_pack() {
        let dir = data_with_repo();
        let git = dir.path().join("projects/CapApp/.git");
        let pack = format!("pack-{}.pack", "a".repeat(40));
        std::fs::write(git.join("objects/pack").join(&pack), "PACKDATA").expect("pack");
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join(&pack), "OUTSIDE!").expect("outside pack");
        let dest = tempfile::tempdir().expect("dest");
        let target = outside.path().to_path_buf();
        at_point("level:objects/pack", move || {
            swap_for_link(&git.join("objects/pack"), &target);
        });
        let files = [GitObjectFile {
            rel: format!("objects/pack/{pack}"),
            size: 8,
        }];
        let result = host(dir.path()).fetch_git_objects_blocking("CapApp", &files, dest.path());
        no_point();
        let err = result.expect_err("a swapped pack directory is refused");
        assert_eq!(err.code(), "transport_failed", "{err}");
        assert!(err.to_string().contains("objects/pack"), "{err}");
        assert!(
            !dest.path().join("objects/pack").join(&pack).exists(),
            "nothing was installed"
        );
    }

    #[cfg(unix)]
    #[test]
    fn local_git_log_tail_refuses_a_swapped_logs_dir() {
        let dir = data_with_repo();
        let git = dir.path().join("projects/CapApp/.git");
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("HEAD"), "0 1 outside\n").expect("log");
        let target = outside.path().to_path_buf();
        at_point("level:logs", move || {
            swap_for_link(&git.join("logs"), &target)
        });
        let result = host(dir.path()).read_git_state_blocking("CapApp");
        no_point();
        let err = result.expect_err("a swapped logs level is refused");
        assert_eq!(err.code(), "transport_failed", "{err}");
        assert!(err.to_string().contains("logs"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn local_git_yaml_refuses_a_swapped_file() {
        let dir = data_with_projects();
        std::fs::write(dir.path().join("git.yaml"), "repos: {}\n").expect("yaml");
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("git.yaml"), "secret: 1\n").expect("outside");
        let (data, target) = (dir.path().to_path_buf(), outside.path().join("git.yaml"));
        at_point("file:git.yaml", move || {
            swap_for_link(&data.join("git.yaml"), &target)
        });
        let result = host(dir.path()).read_git_yaml_blocking();
        no_point();
        let err = result.expect_err("a swapped git.yaml is refused");
        assert!(err.to_string().contains("symlink"), "{err}");
    }

    /// 27-30 kept: every level of the save chain is checked, and now held
    /// too. `git-save` swapped for a link to a whole, well formed copy of
    /// itself is refused: only the link is wrong.
    #[cfg(unix)]
    #[test]
    fn local_save_manifest_refuses_a_swapped_save_level() {
        let dir = data_with_save();
        let level = dir.path().join("ign-deploy/gw/projects/CapApp/git-save");
        let real = level.with_file_name("git-save.real");
        let (at, copy) = (level.clone(), real.clone());
        at_point("level:ign-deploy/gw/projects/CapApp/git-save", move || {
            std::fs::rename(&at, &copy).expect("aside");
            std::os::unix::fs::symlink("git-save.real", &at).expect("link");
        });
        let result = host(dir.path()).save_manifest_blocking("gw", "CapApp", SAVE_RUN);
        no_point();
        let err = result.expect_err("a swapped save level is refused");
        assert_eq!(err.code(), "transport_failed", "{err}");
        assert!(err.to_string().contains("git-save"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn local_project_digests_refuse_a_level_swapped_during_the_walk() {
        let dir = data_with_git();
        let project = dir.path().join("projects/Plain");
        std::fs::create_dir_all(project.join("ignition/x")).expect("dirs");
        std::fs::write(project.join("ignition/x/code.py"), "x = 1\n").expect("file");
        let outside = tempfile::tempdir().expect("outside");
        std::fs::create_dir_all(outside.path().join("x")).expect("x");
        std::fs::write(outside.path().join("x/code.py"), "secret\n").expect("outside file");
        let target = outside.path().to_path_buf();
        at_point("hash:ignition/x/code.py", move || {
            swap_for_link(&project.join("ignition"), &target);
        });
        let result = host(dir.path()).project_file_digests_blocking("Plain");
        no_point();
        let err = result.expect_err("a swapped level is refused");
        assert_eq!(err.code(), "transport_failed", "{err}");
        assert!(err.to_string().contains("ignition"), "{err}");
    }

    // ---- 27-43, Windows: the same readers on held handles -----------------
    //
    // These run on the windows-latest CI job (they cannot run on the machines
    // the plan is executed on). Each uses the native paths `mklink` needs.

    /// `link` made a junction to `target` with `cmd /C mklink /J`, both paths
    /// native (quick 261104-ava, W1).
    #[cfg(windows)]
    fn junction(link: &Path, target: &Path) {
        let native = |p: &Path| p.display().to_string().replace('/', "\\");
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(native(link))
            .arg(native(target))
            .output()
            .expect("mklink");
        assert!(made.status.success(), "mklink /J: {made:?}");
    }

    /// Rename `at` aside and plant a junction to `target` where it was.
    #[cfg(windows)]
    fn swap_for_junction(at: &Path, target: &Path) {
        let aside = at.with_file_name(format!(
            "{}.aside",
            at.file_name().expect("name").to_string_lossy()
        ));
        std::fs::rename(at, &aside).expect("rename aside");
        junction(at, target);
    }

    /// `.git/refs` is a junction when the state is read: the read fails
    /// naming `refs`, and nothing behind the junction is read.
    #[cfg(windows)]
    #[test]
    fn local_git_dir_refuses_a_junction_level() {
        let dir = data_with_repo();
        let git = dir.path().join("projects/CapApp/.git");
        let outside = tempfile::tempdir().expect("outside");
        std::fs::create_dir_all(outside.path().join("heads")).expect("heads");
        std::fs::write(outside.path().join("heads/stolen"), "1111\n").expect("ref");
        std::fs::remove_dir_all(git.join("refs")).expect("rm refs");
        junction(&git.join("refs"), outside.path());
        let err = host(dir.path())
            .read_git_state_blocking("CapApp")
            .expect_err("a junction level is refused");
        assert_eq!(err.code(), "transport_failed", "{err}");
        let text = err.to_string();
        assert!(text.contains("refs"), "names the level: {text}");
        assert!(!text.contains("stolen"), "nothing outside is read: {text}");
    }

    /// While a `GitDir` is open its `.git` cannot be renamed (the level is
    /// held without delete sharing); once it is dropped the rename works.
    #[cfg(windows)]
    #[test]
    fn local_git_dir_holds_its_levels() {
        let dir = data_with_repo();
        let project = dir.path().join("projects/CapApp");
        let git = project.join(".git");
        let moved = project.join(".git-moved");
        let held = match GitDir::open(dir.path(), &["projects", "CapApp", ".git"]) {
            Ok(Open::Found(held)) => held,
            Ok(Open::Absent { path, .. }) => panic!("{path} is absent"),
            Err(e) => panic!("the open failed: {e}"),
        };
        std::fs::rename(&git, &moved).expect_err("a held level cannot be renamed");
        std::fs::rename(&project, dir.path().join("projects/Moved"))
            .expect_err("a level above a held one cannot be renamed");
        assert!(git.is_dir(), ".git is where it was");
        drop(held);
        std::fs::rename(&git, &moved).expect("released with the GitDir");
    }

    /// `HEAD` replaced by a file symlink after the open is refused on the
    /// handle. Where the runner may not create a symlink the test says so
    /// (`SYMLINK-UNAVAILABLE`) and asserts the junction case instead; it never
    /// passes without asserting a refusal.
    #[cfg(windows)]
    #[test]
    fn local_git_read_refuses_a_file_reparse_point() {
        let dir = data_with_repo();
        let git = dir.path().join("projects/CapApp/.git");
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("HEAD"), "ref: refs/heads/stolen\n").expect("head");
        let probe = dir.path().join("symlink-probe");
        let target = outside.path().join("HEAD");
        let can_link = std::os::windows::fs::symlink_file(&target, &probe).is_ok();
        if can_link {
            std::fs::remove_file(&probe).expect("rm probe");
            at_point("file:HEAD", move || {
                let aside = git.join("HEAD.aside");
                std::fs::rename(git.join("HEAD"), &aside).expect("aside");
                std::os::windows::fs::symlink_file(&target, git.join("HEAD")).expect("plant");
            });
            let result = host(dir.path()).read_git_state_blocking("CapApp");
            no_point();
            let err = result.expect_err("a file reparse point is refused");
            assert_eq!(err.code(), "transport_failed", "{err}");
            assert!(err.to_string().contains("HEAD"), "{err}");
        } else {
            eprintln!("SYMLINK-UNAVAILABLE: file symlinks are not permitted here; junction case");
            let outside_refs = outside.path().to_path_buf();
            std::fs::create_dir_all(outside_refs.join("heads")).expect("heads");
            std::fs::write(outside_refs.join("heads/stolen"), "1111\n").expect("ref");
            at_point("level:refs", move || {
                swap_for_junction(&git.join("refs"), &outside_refs);
            });
            let result = host(dir.path()).read_git_state_blocking("CapApp");
            no_point();
            let err = result.expect_err("a junction swapped in is refused");
            assert!(err.to_string().contains("refs"), "{err}");
        }
    }

    // ---- 27-44: handle-anchored writes ---------------------------------

    /// A repo `CapApp` with `refs/heads/sub` (an existing parent), for the
    /// write-swap tests.
    #[cfg(unix)]
    fn data_with_sub_ref() -> tempfile::TempDir {
        let dir = data_with_repo();
        std::fs::create_dir_all(dir.path().join("projects/CapApp/.git/refs/heads/sub"))
            .expect("sub");
        dir
    }

    /// Every file below `root`, as `(relative path, bytes)`, sorted.
    #[cfg(unix)]
    fn tree_snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
        fn visit(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
            for entry in std::fs::read_dir(dir).expect("read_dir") {
                let path = entry.expect("entry").path();
                let rel = path
                    .strip_prefix(root)
                    .expect("below root")
                    .to_string_lossy()
                    .into_owned();
                if path.is_dir() {
                    out.push((format!("{rel}/"), Vec::new()));
                    visit(root, &path, out);
                } else {
                    out.push((rel, std::fs::read(&path).expect("read")));
                }
            }
        }
        let mut out = Vec::new();
        visit(root, root, &mut out);
        out.sort();
        out
    }

    /// CodeRabbit #56 C2 (27-44): a parent of a write swapped for a symlink
    /// after the preflight and before the first create is refused, naming it.
    /// Nothing is created or written in the directory behind the link, and no
    /// lock is left in `.git`.
    #[cfg(unix)]
    #[test]
    fn local_git_write_refuses_a_parent_swapped_after_the_preflight() {
        let dir = data_with_sub_ref();
        let git = dir.path().join("projects/CapApp/.git");
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("keep"), "outside").expect("keep");
        let before = tree_snapshot(outside.path());
        let (g, target) = (git.clone(), outside.path().to_path_buf());
        at_point("write:refs/heads/sub/x", move || {
            swap_for_link(&g.join("refs/heads/sub"), &target)
        });
        let result = host(dir.path()).write_git_files_blocking(
            "CapApp",
            &[GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::BranchRef("sub/x".to_string()),
                bytes: b"1111\n".to_vec(),
            }],
        );
        no_point();
        let err = result.expect_err("a swapped parent is refused");
        assert_eq!(err.code(), "transport_failed", "{err}");
        assert!(err.to_string().contains("sub"), "names the level: {err}");
        assert_eq!(
            tree_snapshot(outside.path()),
            before,
            "nothing was created or written behind the link"
        );
        for lock in ["refs/heads/sub.aside/x.lock", "refs/heads/sub/x.lock"] {
            assert!(
                std::fs::symlink_metadata(git.join(lock)).is_err(),
                "no lock left: {lock}"
            );
        }
    }

    /// `.git` itself renamed aside and replaced with a symlink after the
    /// open: the write goes to the held `.git` (or fails) and is never made
    /// through the symlink.
    #[cfg(unix)]
    #[test]
    fn local_git_write_refuses_a_git_swapped_after_the_open() {
        let dir = data_with_repo();
        let project = dir.path().join("projects/CapApp");
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("HEAD"), "ref: refs/heads/outside\n").expect("HEAD");
        let before = tree_snapshot(outside.path());
        let (p, target) = (project.clone(), outside.path().to_path_buf());
        at_point("write:HEAD", move || {
            swap_for_link(&p.join(".git"), &target)
        });
        let result = host(dir.path()).write_git_files_blocking(
            "CapApp",
            &[GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::Head,
                bytes: b"ref: refs/heads/other\n".to_vec(),
            }],
        );
        no_point();
        assert_eq!(
            tree_snapshot(outside.path()),
            before,
            "nothing was written through the symlink ({result:?})"
        );
        if result.is_ok() {
            assert_eq!(
                std::fs::read(project.join(".git.aside/HEAD")).expect("held HEAD"),
                b"ref: refs/heads/other\n",
                "the write went to the .git it held"
            );
        }
    }

    /// Restore the save of `data_with_save` with `planted` files created in a
    /// `.git` that appears just before the publish; returns the error text.
    fn restore_with_git_planted(planted: &'static [(&'static str, &'static [u8])]) -> String {
        let dir = data_with_save();
        let t = host(dir.path());
        let before = t
            .save_manifest_blocking("gw", "CapApp", SAVE_RUN)
            .expect("save manifest");
        let git = dir.path().join("projects/CapApp/.git");
        let g = git.clone();
        at_point("publish:CapApp", move || {
            std::fs::create_dir(&g).expect("plant .git");
            for (name, bytes) in planted {
                std::fs::write(g.join(name), bytes).expect("plant file");
            }
        });
        let result = t.restore_git_blocking("gw", "CapApp", SAVE_RUN);
        no_point();
        let err = result.expect_err("a .git that appeared is never replaced");
        assert_eq!(err.code(), "transport_failed", "{err}");
        let text = err.to_string();
        assert!(
            text.contains("projects/CapApp/.git appeared; it is never replaced"),
            "{text}"
        );
        let mut names: Vec<String> = std::fs::read_dir(&git)
            .expect("the planted .git is still there")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        let mut want: Vec<String> = planted.iter().map(|(n, _)| (*n).to_string()).collect();
        want.sort();
        assert_eq!(names, want, "the planted .git is unchanged");
        for (name, bytes) in planted {
            assert_eq!(&std::fs::read(git.join(name)).expect("planted file"), bytes);
        }
        assert_eq!(
            t.save_manifest_blocking("gw", "CapApp", SAVE_RUN)
                .expect("the save is intact"),
            before
        );
        let run = dir
            .path()
            .join("ign-deploy/gw/projects/CapApp/git-save")
            .join(SAVE_RUN);
        assert!(
            std::fs::symlink_metadata(run.join("restoring")).is_err(),
            "the stage is removed"
        );
        text
    }

    /// CodeRabbit #56 C12 (27-44): rename(2) replaces an empty directory, so
    /// an empty `.git` that appeared after the last look was silently
    /// replaced. The publish never replaces.
    #[test]
    fn local_git_restore_keeps_an_empty_git_planted_before_the_publish() {
        restore_with_git_planted(&[]);
    }

    #[test]
    fn local_git_restore_keeps_a_full_git_planted_before_the_publish() {
        restore_with_git_planted(&[
            ("HEAD", b"ref: refs/heads/theirs\n"),
            ("config", b"[core]\n"),
        ]);
    }

    /// 27-44 (C12): a host whose file system does not know the no-replace
    /// flag fails closed. EINVAL, ENOSYS and EOPNOTSUPP are "the host cannot
    /// publish without replacing" (a `transport_failed`, never a plain
    /// rename); EEXIST is "a `.git` appeared"; no other errno is either.
    #[cfg(unix)]
    #[test]
    fn local_noreplace_unsupported_fails_closed() {
        use rustix::io::Errno;
        let dir = data_with_projects();
        let t = host(dir.path());
        let text = "the host cannot publish without replacing";
        for errno in [Errno::INVAL, Errno::NOSYS, Errno::OPNOTSUPP] {
            let e = super::super::git_dir::noreplace_error(errno);
            assert!(!e.already_exists(), "{errno:?}");
            let core = t.git_fail("write", e.to_string());
            assert_eq!(core.code(), "transport_failed", "{errno:?}");
            assert!(core.to_string().contains(text), "{errno:?}: {core}");
        }
        let appeared = super::super::git_dir::noreplace_error(Errno::EXIST);
        assert!(appeared.already_exists());
        assert!(!appeared.to_string().contains(text));
        for errno in [
            Errno::NOENT,
            Errno::ACCESS,
            Errno::NOTEMPTY,
            Errno::XDEV,
            Errno::NOTDIR,
            Errno::IO,
        ] {
            let e = super::super::git_dir::noreplace_error(errno);
            assert!(!e.already_exists(), "{errno:?}");
            assert!(
                !e.to_string().contains(text),
                "{errno:?} is not a missing flag: {e}"
            );
        }
    }

    /// 27-44 on Windows: an empty `.git` planted before the publish is kept
    /// and the restore refuses with the same wording (std's rename refuses a
    /// directory target; the publish also looks at the target on its parent's
    /// handle). Pending windows-latest CI.
    #[cfg(windows)]
    #[test]
    fn local_git_restore_refuses_an_existing_git_on_windows() {
        restore_with_git_planted(&[]);
    }

    /// The assumption the Windows publish relies on, proven directly: std's
    /// rename of a directory onto an existing empty directory fails and both
    /// survive. Pending windows-latest CI.
    #[cfg(windows)]
    #[test]
    fn local_noreplace_windows_rename_refuses_a_directory_target() {
        let dir = tempfile::tempdir().expect("dir");
        let (from, to) = (dir.path().join("from"), dir.path().join("to"));
        std::fs::create_dir(&from).expect("from");
        std::fs::write(from.join("f"), b"x").expect("f");
        std::fs::create_dir(&to).expect("to");
        std::fs::rename(&from, &to).expect_err("a directory target is not replaced");
        assert!(from.join("f").is_file(), "the source is untouched");
        assert!(to.is_dir(), "the target is untouched");
        assert_eq!(std::fs::read_dir(&to).expect("to").count(), 0);
    }

    /// C10/C11 kept (27-44): when a write fails after its lock was created,
    /// the lock is removed only if it is still the file this call created. A
    /// lock another writer put at the name meanwhile, with the same bytes or
    /// not, stays. The publish is made to fail by turning the target into a
    /// directory after the lock exists.
    #[cfg(unix)]
    #[test]
    fn local_git_write_failure_keeps_a_foreign_lock_at_the_same_name() {
        let dir = data_with_repo();
        let git = dir.path().join("projects/CapApp/.git");
        let g = git.clone();
        at_point("locked:HEAD", move || {
            // the other writer: takes the lock name with the same bytes this
            // call is about to write, and the target stops being a file
            std::fs::remove_file(g.join("HEAD.lock")).expect("drop this call's lock");
            std::fs::write(g.join("HEAD.lock"), b"ref: refs/heads/other\n").expect("foreign lock");
            std::fs::remove_file(g.join("HEAD")).expect("rm HEAD");
            std::fs::create_dir(g.join("HEAD")).expect("HEAD is a directory");
        });
        let result = host(dir.path()).write_git_files_blocking(
            "CapApp",
            &[GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::Head,
                bytes: b"ref: refs/heads/other\n".to_vec(),
            }],
        );
        no_point();
        let err = result.expect_err("the publish cannot replace a directory");
        assert!(err.to_string().contains("HEAD"), "{err}");
        assert_eq!(
            std::fs::read(git.join("HEAD.lock")).expect("the foreign lock stays"),
            b"ref: refs/heads/other\n"
        );
    }

    /// A failed write takes away the directories it made, when they are
    /// still empty (27-44). `refs/heads/new` is made by this pass and is
    /// closed to writes just before the next level is made, so the pass
    /// fails; `new` is removed again and `refs/heads`, which was there,
    /// stays.
    #[cfg(unix)]
    #[test]
    fn local_git_write_failure_removes_the_empty_levels_it_made() {
        use std::os::unix::fs::PermissionsExt;
        if current_uid().is_ok_and(|uid| uid == 0) {
            return; // root ignores the mode this test closes the level with
        }
        let dir = data_with_repo();
        let git = dir.path().join("projects/CapApp/.git");
        let g = git.clone();
        at_point("level:refs/heads/new/deep", move || {
            std::fs::set_permissions(
                g.join("refs/heads/new"),
                std::fs::Permissions::from_mode(0o500),
            )
            .expect("close new");
        });
        let result = host(dir.path()).write_git_files_blocking(
            "CapApp",
            &[GitWrite {
                old: Default::default(),
                packed_refs_old: None,
                name: GitWriteName::BranchRef("new/deep/x".to_string()),
                bytes: b"1111\n".to_vec(),
            }],
        );
        no_point();
        result.expect_err("the next level cannot be made");
        assert!(
            std::fs::symlink_metadata(git.join("refs/heads/new")).is_err(),
            "the level this pass made is removed again"
        );
        assert!(git.join("refs/heads").is_dir(), "an existing level stays");
    }

    fn sha(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    /// A write of `name` that expects `old` (and, for a packed branch,
    /// `packed`) to be what it replaces.
    fn guarded(
        name: GitWriteName,
        bytes: &[u8],
        old: OldValue,
        packed: Option<OldValue>,
    ) -> GitWrite {
        GitWrite {
            name,
            bytes: bytes.to_vec(),
            old,
            packed_refs_old: packed,
        }
    }

    /// Assert that `err` is the answer for a target that changed.
    fn assert_changed(err: &CoreError, what: &str) {
        match err {
            CoreError::TransportFailed {
                operation, detail, ..
            } => {
                assert_eq!(operation, GIT_CHANGED_OPERATION, "{err}");
                assert!(detail.contains(what), "names the file: {detail}");
            }
            other => panic!("not a transport failure: {other}"),
        }
    }

    /// 27-45 (F4): the writes of an unchanged repo go through when every old
    /// value holds: a digest that matches, an absent loose ref, and a packed
    /// ref whose `packed-refs` is as it was read.
    #[test]
    fn local_git_write_accepts_an_unchanged_target() {
        let dir = data_with_repo();
        let git = dir.path().join("projects/CapApp/.git");
        std::fs::write(git.join("packed-refs"), "p\n").expect("packed-refs");
        let result = host(dir.path()).write_git_files_blocking(
            "CapApp",
            &[
                guarded(
                    GitWriteName::Head,
                    b"ref: refs/heads/other\n",
                    OldValue::Sha256(sha(b"ref: refs/heads/main\n")),
                    None,
                ),
                guarded(
                    GitWriteName::BranchRef("main".to_string()),
                    b"bbbb\n",
                    OldValue::Sha256(sha(b"aaaa\n")),
                    None,
                ),
                guarded(
                    GitWriteName::BranchRef("fresh".to_string()),
                    b"cccc\n",
                    OldValue::Absent,
                    Some(OldValue::Sha256(sha(b"p\n"))),
                ),
            ],
        );
        result.expect("every expectation holds");
        assert_eq!(
            std::fs::read(git.join("refs/heads/main")).expect("main"),
            b"bbbb\n"
        );
        assert_eq!(
            std::fs::read(git.join("refs/heads/fresh")).expect("fresh"),
            b"cccc\n"
        );
    }

    /// 27-45 (F4): a ref rewritten after the engine read it is never
    /// overwritten. The refusal is the shared marker, the new value stays,
    /// this call's lock is gone, and a write that came before it in the pass
    /// stays published.
    #[test]
    fn local_git_write_refuses_a_changed_target() {
        let dir = data_with_repo();
        let git = dir.path().join("projects/CapApp/.git");
        // the Designer commits after the engine read the repository
        std::fs::write(git.join("refs/heads/main"), "dddd\n").expect("designer commit");
        let err = host(dir.path())
            .write_git_files_blocking(
                "CapApp",
                &[
                    guarded(
                        GitWriteName::Head,
                        b"ref: refs/heads/other\n",
                        OldValue::Sha256(sha(b"ref: refs/heads/main\n")),
                        None,
                    ),
                    guarded(
                        GitWriteName::BranchRef("main".to_string()),
                        b"bbbb\n",
                        OldValue::Sha256(sha(b"aaaa\n")),
                        None,
                    ),
                ],
            )
            .expect_err("a changed ref is refused");
        assert_changed(&err, "refs/heads/main");
        assert_eq!(
            std::fs::read(git.join("refs/heads/main")).expect("main"),
            b"dddd\n",
            "the Designer's value stays"
        );
        assert!(
            std::fs::symlink_metadata(git.join("refs/heads/main.lock")).is_err(),
            "this call's lock is removed"
        );
        assert_eq!(
            std::fs::read(git.join("HEAD")).expect("HEAD"),
            b"ref: refs/heads/other\n",
            "the write before it stays"
        );
    }

    /// 27-45 (F4): the comparison is made under the lock. A lock another
    /// writer holds at the name after this call's create (the call's own is
    /// gone) is not removed by the refusal.
    #[test]
    fn local_git_write_mismatch_keeps_a_foreign_lock() {
        let dir = data_with_repo();
        let git = dir.path().join("projects/CapApp/.git");
        let g = git.clone();
        at_point("locked:refs/heads/main", move || {
            std::fs::remove_file(g.join("refs/heads/main.lock")).expect("drop this call's lock");
            std::fs::write(g.join("refs/heads/main.lock"), b"foreign").expect("foreign lock");
            std::fs::write(g.join("refs/heads/main"), "dddd\n").expect("designer commit");
        });
        let result = host(dir.path()).write_git_files_blocking(
            "CapApp",
            &[guarded(
                GitWriteName::BranchRef("main".to_string()),
                b"bbbb\n",
                OldValue::Sha256(sha(b"aaaa\n")),
                None,
            )],
        );
        no_point();
        assert_changed(&result.expect_err("refused"), "refs/heads/main");
        assert_eq!(
            std::fs::read(git.join("refs/heads/main.lock")).expect("the foreign lock stays"),
            b"foreign"
        );
        assert_eq!(
            std::fs::read(git.join("refs/heads/main")).expect("main"),
            b"dddd\n"
        );
    }

    /// 27-45 (F4): an expected-absent loose ref that was created meanwhile is
    /// refused the same way.
    #[test]
    fn local_git_write_refuses_a_ref_that_appeared() {
        let dir = data_with_repo();
        let git = dir.path().join("projects/CapApp/.git");
        std::fs::write(git.join("refs/heads/fresh"), "eeee\n").expect("appeared");
        let err = host(dir.path())
            .write_git_files_blocking(
                "CapApp",
                &[guarded(
                    GitWriteName::BranchRef("fresh".to_string()),
                    b"cccc\n",
                    OldValue::Absent,
                    None,
                )],
            )
            .expect_err("a ref that appeared is refused");
        assert_changed(&err, "refs/heads/fresh");
        assert_eq!(
            std::fs::read(git.join("refs/heads/fresh")).expect("fresh"),
            b"eeee\n"
        );
        assert!(std::fs::symlink_metadata(git.join("refs/heads/fresh.lock")).is_err());
    }

    /// 27-45 (F4): a branch that lived only in `packed-refs` is refused when
    /// `packed-refs` changed meanwhile (a `pack-refs`, a deleted branch), and
    /// when it appeared after the engine saw none.
    #[test]
    fn local_git_write_refuses_a_changed_packed_refs() {
        let dir = data_with_repo();
        let git = dir.path().join("projects/CapApp/.git");
        std::fs::write(git.join("packed-refs"), "rewritten\n").expect("packed-refs");
        let err = host(dir.path())
            .write_git_files_blocking(
                "CapApp",
                &[guarded(
                    GitWriteName::BranchRef("packed".to_string()),
                    b"cccc\n",
                    OldValue::Absent,
                    Some(OldValue::Sha256(sha(b"as read\n"))),
                )],
            )
            .expect_err("a changed packed-refs is refused");
        assert_changed(&err, "packed-refs");
        assert!(std::fs::symlink_metadata(git.join("refs/heads/packed")).is_err());
        assert!(std::fs::symlink_metadata(git.join("refs/heads/packed.lock")).is_err());

        // `packed-refs` was absent when read and exists now
        let err = host(dir.path())
            .write_git_files_blocking(
                "CapApp",
                &[guarded(
                    GitWriteName::BranchRef("packed".to_string()),
                    b"cccc\n",
                    OldValue::Absent,
                    Some(OldValue::Absent),
                )],
            )
            .expect_err("a packed-refs that appeared is refused");
        assert_changed(&err, "packed-refs");
    }

    /// 27-44: a level of the save's directory chain swapped for a symlink
    /// after the walk, while the save opens its levels from the ledger root,
    /// is refused naming the level; nothing is created behind the link.
    #[cfg(unix)]
    #[test]
    fn local_git_save_refuses_a_save_level_swapped_after_the_open() {
        let dir = data_with_repo();
        let outside = tempfile::tempdir().expect("outside");
        std::fs::create_dir_all(outside.path().join("CapApp")).expect("outside project");
        let before = tree_snapshot(outside.path());
        let (d, target) = (dir.path().to_path_buf(), outside.path().to_path_buf());
        std::fs::create_dir_all(d.join("ign-deploy/gw/projects")).expect("chain");
        at_point("level:ign-deploy/gw/projects", move || {
            swap_for_link(&d.join("ign-deploy/gw/projects"), &target)
        });
        let result = host(dir.path()).save_git_blocking("gw", "CapApp", "run1");
        no_point();
        let err = result.expect_err("a swapped level is refused");
        assert_eq!(err.code(), "transport_failed", "{err}");
        assert!(
            err.to_string().contains("ign-deploy/gw/projects"),
            "names the level: {err}"
        );
        assert_eq!(
            tree_snapshot(outside.path()),
            before,
            "nothing behind the link"
        );
    }
}
