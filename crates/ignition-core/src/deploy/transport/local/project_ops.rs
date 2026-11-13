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
    GitEntry, GitHostRead, GitManifest, GitStateWriter, ProjectHost, ProjectRunWriter,
    ReleaseRecordWriter, TagRecordWriter, check_pre_image_write, git_entry_location,
    project_run_names_problem_on, tag_record_names_problem_on,
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
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(mode);
    }
    #[cfg(not(unix))]
    let _ = mode;
    builder.create(path)
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
        for entry in entries.iter().filter(|e| !e.is_dir) {
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
            digest.update([b'\n']);
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
        let cleanup = || {
            let _ = std::fs::remove_dir_all(&repo);
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
                cleanup();
            }
            return Err(self.git_fail(
                OP,
                format!(
                    "{} already exists; a save is never overwritten",
                    git_save_location(LedgerRoot::DataDir, gateway, project, run_id)
                ),
            ));
        }
        let outcome = (|| {
            create_dir_mode(&repo, CopyMode::Private.dir_mode()).map_err(|e| {
                self.git_fail(
                    OP,
                    format!("the save directory cannot be created ({})", e.kind()),
                )
            })?;
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
            cleanup();
        }
        outcome
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
        let mut path = self.data_dir.clone();
        let rel = git_save_rel(gateway, project, run_id);
        let parts: Vec<&str> = rel.split('/').collect();
        for part in &parts[..parts.len() - 1] {
            path.push(part);
            match std::fs::symlink_metadata(&path) {
                Ok(m) if m.file_type().is_symlink() => {
                    return Err(
                        self.git_fail(OP, format!("{part} is a symlink; symlinks are refused"))
                    );
                }
                Ok(m) if !m.file_type().is_dir() => {
                    return Err(self.git_fail(OP, format!("{part} is not a directory")));
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(e) => {
                    return Err(
                        self.git_fail(OP, format!("{part} cannot be inspected ({})", e.kind()))
                    );
                }
            }
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
                    Err(_) => create_dir_mode(&dir, 0o755).map_err(|e| {
                        self.git_fail(OP, format!("{part} cannot be created ({})", e.kind()))
                    })?,
                }
            }
            let rel = write.name.rel();
            let result = (|| -> std::io::Result<()> {
                let mut file = create_new_file(&temp, 0o644)?;
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

    /// A capped read of a regular file; `None` when it does not exist.
    fn read_git_small(
        &self,
        git: &Path,
        rel: &str,
        cap: usize,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        let path = rel
            .split('/')
            .fold(git.to_path_buf(), |p, part| p.join(part));
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
            let dir = prefix.split('/').fold(git.clone(), |p, part| p.join(part));
            self.collect_loose_refs(&dir, prefix, &mut files.loose_refs)?;
        }
        files.head_log_tail = self.read_log_tail(&git)?;
        Ok(files)
    }

    /// The last [`GIT_LOG_TAIL_MAX`] bytes of `logs/HEAD`.
    fn read_log_tail(&self, git: &Path) -> Result<Option<Vec<u8>>, CoreError> {
        let path = git.join("logs").join("HEAD");
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
            this.git_save_names(&gateway, &project, &run_id)?;
            this.require_data_dir()?;
            let repo = this.git_save_repo(&gateway, &project, &run_id);
            this.require_real_dir(
                "read",
                &repo,
                &git_save_location(LedgerRoot::DataDir, &gateway, &project, &run_id),
            )?;
            this.manifest_blocking("read", &repo)
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
            let path = rel.split('/').fold(git, |p, part| p.join(part));
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
