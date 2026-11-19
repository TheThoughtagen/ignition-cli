//! [`GitDir`]: a directory the local transport opened level by level, without
//! following a link, and every later read relative to that handle (27-43,
//! CodeRabbit #56 C1, the first MEDIUM security finding of the Phase 27
//! review).
//!
//! The path-based readers checked a path with `symlink_metadata` and then
//! opened the same path again. A level or a file swapped for a link between
//! the check and the open was followed. Here each level is opened once, from
//! the handle of the level above it, and the next read is relative to that
//! handle, so there is no second look at a name that could be redirected.
//!
//! One type serves every local reader of a gateway's `.git`, a project
//! directory (27-40's `project_file_digests`), a save's `repo`, and the data
//! directory itself (`git.yaml`): the root differs, the opens do not.
//!
//! # Unix
//!
//! A `GitDir` holds a [`Dir`](super::super::local_fs::Dir) (an `O_DIRECTORY`
//! descriptor). Every level is `openat(O_DIRECTORY | O_NOFOLLOW)` from the
//! one before; a file is `openat(O_RDONLY | O_NOFOLLOW | O_NONBLOCK)` and
//! `fstat`ed on the descriptor it returned (a FIFO swapped in cannot block
//! the open). A symlink, or anything that is not what the walk saw, fails
//! the call.
//!
//! # Windows
//!
//! std has no handle-relative open, so the guarantee is built from what
//! Windows does give:
//!
//! - every level, from the data directory down, is opened with
//!   `FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT` (the first
//!   opens a directory, the second opens a link itself instead of its target)
//!   and held, with `FILE_SHARE_READ | FILE_SHARE_WRITE` and no
//!   `FILE_SHARE_DELETE`, for as long as the `GitDir` lives. A held level
//!   cannot be renamed or replaced, and neither can the levels above it, so
//!   the full path of a child is resolved through directories nobody can
//!   swap;
//! - the handle actually used is inspected: a reparse point (a symlink or a
//!   junction) is refused on the handle, not on a name looked at earlier;
//! - a file is opened the same way, with read and write sharing, and the
//!   handle must be a regular file and not a reparse point.
//!
//! The one remaining name is the final component of a file, opened by its full
//! path below held levels and checked on its handle; a link planted there is
//! refused by that check. A level is held only while its `GitDir` is alive,
//! which is one read call: a child `GitDir` is valid while the `GitDir` it was
//! taken from is alive.
//!
//! The error texts are value-free: level names only, never a path outside the
//! data directory.

use std::ffi::OsString;
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[cfg(windows)]
use std::path::PathBuf;

use crate::deploy::transport::local_fs::Kind;
#[cfg(unix)]
use crate::deploy::transport::local_fs::{Dir, FsError};

/// A test's hook: told which point was reached.
#[cfg(test)]
type Hook = Box<dyn FnMut(&str)>;

// The test hook (not compiled outside tests). `fire` runs it, if one is set,
// with a label naming the point just before a level or a file is opened for
// use (`level:<rel>`, `file:<rel>`) or before a file is hashed (`hash:<rel>`).
// A test swaps a level or a file there and checks that the read refuses it.
#[cfg(test)]
thread_local! {
    pub(super) static HOOK: std::cell::RefCell<Option<Hook>> = std::cell::RefCell::new(None);
}

#[cfg(test)]
pub(super) fn fire(label: impl FnOnce() -> String) {
    HOOK.with(|hook| {
        let taken = hook.borrow_mut().take();
        if let Some(mut f) = taken {
            f(&label());
            let mut slot = hook.borrow_mut();
            if slot.is_none() {
                *slot = Some(f);
            }
        }
    });
}

#[cfg(not(test))]
#[inline(always)]
pub(super) fn fire(_label: impl FnOnce() -> String) {}

/// Why a read through a [`GitDir`] failed, already worded (value-free).
#[derive(Debug)]
pub(super) struct GitDirError {
    text: String,
    /// The tree is not what a read may trust (a link, a swapped level or file,
    /// a non-regular file), as opposed to an I/O failure.
    refusal: bool,
}

impl GitDirError {
    fn new(text: impl Into<String>) -> GitDirError {
        GitDirError {
            text: text.into(),
            refusal: false,
        }
    }

    fn refused(text: impl Into<String>) -> GitDirError {
        GitDirError {
            text: text.into(),
            refusal: true,
        }
    }

    /// A refusal of the tree, not an I/O failure.
    pub(super) fn is_refusal(&self) -> bool {
        self.refusal
    }
}

impl std::fmt::Display for GitDirError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

/// What [`GitDir::open`] found.
pub(super) enum Open {
    Found(GitDir),
    /// Nothing at `level` (the component), `path` being it below the root.
    Absent {
        level: String,
        path: String,
    },
}

/// One entry of [`GitDir::list`]: what a name is, from the directory handle.
#[derive(Debug, Clone)]
pub(super) struct Entry {
    pub(super) name: OsString,
    pub(super) kind: Kind,
    /// The file's size (a link's own size for a symlink, `0` for a directory
    /// on platforms that do not report one).
    pub(super) size: u64,
    /// The Unix permission bits (`0` where the platform has none).
    pub(super) mode: u32,
}

/// What an opened file's handle says about it.
#[derive(Debug, Clone, Copy)]
pub(super) struct FileMeta {
    pub(super) size: u64,
    pub(super) mode: u32,
}

/// A directory held open (see the module doc).
pub(super) struct GitDir {
    #[cfg(unix)]
    dir: Dir,
    #[cfg(windows)]
    path: PathBuf,
    /// The handles of the levels held for this directory (Windows).
    #[cfg(windows)]
    held: Vec<File>,
    /// `/`-separated path of this directory below the opened root, for
    /// messages ("" for the root).
    prefix: String,
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

/// The text for a level that is not the directory it was.
fn level_error(rel: &str, found: Kind, at_open: bool) -> GitDirError {
    if at_open {
        return GitDirError::refused(match found {
            Kind::Symlink => format!("{rel} is a symlink; symlinks are refused"),
            Kind::Missing => format!("{rel} changed while it was opened (it is gone)"),
            _ => format!("{rel} is not a directory"),
        });
    }
    GitDirError::refused(match found {
        Kind::Symlink => {
            format!("{rel} changed while it was opened (it is a symlink; symlinks are refused)")
        }
        Kind::Missing => format!("{rel} changed while it was opened (it is gone)"),
        _ => format!("{rel} changed while it was opened (it is not a directory)"),
    })
}

/// The text for a file that is not the regular file it was.
fn file_error(rel: &str, found: Kind) -> GitDirError {
    GitDirError::refused(match found {
        Kind::Missing => format!("{rel} changed while it was opened (it is gone)"),
        _ => format!(
            "{rel} changed while it was opened (it is not a regular file; symlinks are refused)"
        ),
    })
}

fn io_text(rel: &str, what: &str, e: &std::io::Error) -> GitDirError {
    GitDirError::new(format!("{rel} {what} ({})", e.kind()))
}

#[cfg(unix)]
fn fs_text(rel: &str, what: &str, e: &FsError) -> GitDirError {
    match e {
        FsError::Io(e) => io_text(rel, what, e),
        other => GitDirError::new(format!("{rel} {what} ({})", other.describe())),
    }
}

#[cfg(unix)]
fn meta_mode(meta: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o777
}

#[cfg(windows)]
fn meta_mode(_meta: &std::fs::Metadata) -> u32 {
    0
}

impl GitDir {
    /// Open `parts` below `data_dir`, one level at a time, none followed; the
    /// data directory itself is opened without following a final link. A
    /// level with nothing at it answers [`Open::Absent`]; one that is a
    /// symlink or not a directory is "<level> is a symlink; symlinks are
    /// refused" or "<level> is not a directory", naming the path below
    /// `data_dir`.
    pub(super) fn open(data_dir: &Path, parts: &[&str]) -> Result<Open, GitDirError> {
        let mut dir = Self::data_root(data_dir)?;
        for part in parts {
            match dir.step(part, true)? {
                Some(mut next) => {
                    next.absorb(dir);
                    dir = next;
                }
                None => {
                    return Ok(Open::Absent {
                        level: (*part).to_string(),
                        path: join(&dir.prefix, part),
                    });
                }
            }
        }
        dir.prefix.clear();
        Ok(Open::Found(dir))
    }

    /// The child directory `name`, opened from this handle. `None` when it is
    /// absent; a symlink or a non-directory is refused, naming the level
    /// ("changed while it was opened").
    pub(super) fn child(&self, name: &str) -> Result<Option<GitDir>, GitDirError> {
        self.step(name, false)
    }

    /// The directory at `rel` below this one, one `child` per level.
    pub(super) fn at(&self, rel: &str) -> Result<Option<GitDir>, GitDirError> {
        let mut parts = rel.split('/');
        let first = parts.next().unwrap_or_default();
        let Some(mut current) = self.step(first, false)? else {
            return Ok(None);
        };
        for part in parts {
            let Some(mut next) = current.step(part, false)? else {
                return Ok(None);
            };
            next.absorb(current);
            current = next;
        }
        Ok(Some(current))
    }

    /// Run `f` on the directory that holds `rel` and the last component of
    /// `rel`; `absent` when a level above it is absent.
    fn in_parent<T>(
        &self,
        rel: &str,
        absent: T,
        f: impl FnOnce(&GitDir, &str) -> Result<T, GitDirError>,
    ) -> Result<T, GitDirError> {
        match rel.rsplit_once('/') {
            None => f(self, rel),
            Some((parents, name)) => match self.at(parents)? {
                None => Ok(absent),
                Some(parent) => f(&parent, name),
            },
        }
    }

    /// What `rel` below this directory is, never followed; `Missing` when a
    /// level above it is absent.
    pub(super) fn kind(&self, rel: &str) -> Result<Kind, GitDirError> {
        self.in_parent(rel, Kind::Missing, |dir, name| dir.kind_here(name))
    }

    /// Whether `rel` exists, any kind of entry.
    pub(super) fn present(&self, rel: &str) -> Result<bool, GitDirError> {
        Ok(self.kind(rel)? != Kind::Missing)
    }

    /// Open the regular file at `rel` for reading, and what its handle says.
    /// `None` when it, or a level above it, is absent. Never a symlink, never
    /// a file that is not a regular file; the metadata is the handle's.
    pub(super) fn open_file(&self, rel: &str) -> Result<Option<(File, FileMeta)>, GitDirError> {
        self.in_parent(rel, None, |dir, name| dir.open_file_here(name))
    }

    /// At most `cap + 1` bytes of the regular file at `rel` (one past the cap
    /// lets the caller name an excess); `None` when absent.
    pub(super) fn read_small(&self, rel: &str, cap: usize) -> Result<Option<Vec<u8>>, GitDirError> {
        let Some((file, _)) = self.open_file(rel)? else {
            return Ok(None);
        };
        let mut bytes = Vec::new();
        file.take(cap as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| io_text(&join(&self.prefix, rel), "cannot be read", &e))?;
        Ok(Some(bytes))
    }

    /// This directory as the root of what is read next: its messages and
    /// the paths a walk reports are relative to it from here on.
    pub(super) fn into_root(mut self) -> GitDir {
        self.prefix.clear();
        self
    }

    /// The path of this directory below the opened root ("" for the root),
    /// for a message.
    pub(super) fn prefix(&self) -> &str {
        &self.prefix
    }
}

#[cfg(unix)]
impl GitDir {
    fn data_root(path: &Path) -> Result<GitDir, GitDirError> {
        match Dir::root(path) {
            Ok(dir) => Ok(GitDir {
                dir,
                prefix: String::new(),
            }),
            Err(FsError::Changed) => Err(GitDirError::refused(
                "the data directory is not a real directory (symlinks are refused)",
            )),
            Err(e) => Err(fs_text("the data directory", "cannot be opened", &e)),
        }
    }

    fn absorb(&mut self, _parent: GitDir) {}

    fn step(&self, name: &str, at_open: bool) -> Result<Option<GitDir>, GitDirError> {
        let rel = join(&self.prefix, name);
        fire(|| format!("level:{rel}"));
        match self.dir.open(&[name]) {
            Ok(dir) => Ok(Some(GitDir { dir, prefix: rel })),
            Err(FsError::Missing) => Ok(None),
            Err(FsError::Changed) => {
                let found = self.dir.kind(&[name]).unwrap_or(Kind::Other);
                Err(level_error(&rel, found, at_open))
            }
            Err(e) => Err(fs_text(&rel, "cannot be inspected", &e)),
        }
    }

    fn kind_here(&self, name: &str) -> Result<Kind, GitDirError> {
        self.dir
            .kind(&[name])
            .map_err(|e| fs_text(&join(&self.prefix, name), "cannot be inspected", &e))
    }

    fn open_file_here(&self, name: &str) -> Result<Option<(File, FileMeta)>, GitDirError> {
        let rel = join(&self.prefix, name);
        fire(|| format!("file:{rel}"));
        match self.dir.open_file(&[name]) {
            Ok(None) => Ok(None),
            Ok(Some((file, meta))) => {
                let meta = FileMeta {
                    size: meta.len(),
                    mode: meta_mode(&meta),
                };
                Ok(Some((file, meta)))
            }
            Err(FsError::Changed) => {
                let found = self.dir.kind(&[name]).unwrap_or(Kind::Other);
                Err(file_error(&rel, found))
            }
            Err(e) => Err(fs_text(&rel, "cannot be read", &e)),
        }
    }

    /// Every entry of this directory, sorted by name, each statted relative
    /// to this handle.
    pub(super) fn list(&self) -> Result<Vec<Entry>, GitDirError> {
        let shown = if self.prefix.is_empty() {
            "the repo"
        } else {
            self.prefix.as_str()
        };
        match self.dir.list_stat(&[]) {
            Ok(raw) => Ok(raw
                .into_iter()
                .map(|e| Entry {
                    name: e.name,
                    kind: e.kind,
                    size: e.size,
                    mode: e.mode,
                })
                .collect()),
            Err(FsError::Missing) => Err(GitDirError::refused(format!(
                "{shown} changed while it was listed"
            ))),
            Err(e) => Err(fs_text(shown, "cannot be listed", &e)),
        }
    }

    /// The permission bits of this directory, from its handle.
    pub(super) fn mode(&self) -> Result<u32, GitDirError> {
        let shown = if self.prefix.is_empty() {
            "the repo"
        } else {
            self.prefix.as_str()
        };
        self.dir
            .metadata()
            .map(|m| meta_mode(&m))
            .map_err(|e| fs_text(shown, "cannot be inspected", &e))
    }
}

/// Win32 `FILE_FLAG_BACKUP_SEMANTICS`: needed to open a directory.
#[cfg(windows)]
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
/// Win32 `FILE_FLAG_OPEN_REPARSE_POINT`: open a link itself, not its target.
#[cfg(windows)]
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
/// Win32 `FILE_SHARE_READ` and `FILE_SHARE_WRITE`. `FILE_SHARE_DELETE` is
/// left out on purpose: a held level cannot be renamed or replaced.
#[cfg(windows)]
const SHARE_READ_WRITE: u32 = 0x1 | 0x2;
/// Win32 `FILE_ATTRIBUTE_REPARSE_POINT`.
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

/// Open `path` (a level when `dir`, a file otherwise) as a handle that does
/// not follow a reparse point and does not share delete.
#[cfg(windows)]
fn open_held(path: &Path, dir: bool) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    let flags = if dir {
        FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT
    } else {
        FILE_FLAG_OPEN_REPARSE_POINT
    };
    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(SHARE_READ_WRITE)
        .custom_flags(flags)
        .open(path)
}

/// Whether the handle's metadata says the object is a reparse point of any
/// kind (symlink, junction, or another).
#[cfg(windows)]
fn is_reparse(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    meta.file_type().is_symlink() || meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(windows)]
fn kind_of_meta(meta: &std::fs::Metadata) -> Kind {
    if is_reparse(meta) {
        Kind::Symlink
    } else if meta.is_dir() {
        Kind::Dir
    } else if meta.is_file() {
        Kind::File
    } else {
        Kind::Other
    }
}

#[cfg(windows)]
impl GitDir {
    fn data_root(path: &Path) -> Result<GitDir, GitDirError> {
        let file = open_held(path, true)
            .map_err(|e| io_text("the data directory", "cannot be opened", &e))?;
        let meta = file
            .metadata()
            .map_err(|e| io_text("the data directory", "cannot be inspected", &e))?;
        if is_reparse(&meta) || !meta.is_dir() {
            return Err(GitDirError::refused(
                "the data directory is not a real directory (symlinks are refused)",
            ));
        }
        Ok(GitDir {
            path: path.to_path_buf(),
            held: vec![file],
            prefix: String::new(),
        })
    }

    /// Keep the handles of `parent` for as long as this directory lives.
    fn absorb(&mut self, parent: GitDir) {
        self.held.extend(parent.held);
    }

    fn step(&self, name: &str, at_open: bool) -> Result<Option<GitDir>, GitDirError> {
        let rel = join(&self.prefix, name);
        fire(|| format!("level:{rel}"));
        let path = self.path.join(name);
        let file = match open_held(&path, true) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io_text(&rel, "cannot be inspected", &e)),
        };
        let meta = file
            .metadata()
            .map_err(|e| io_text(&rel, "cannot be inspected", &e))?;
        if is_reparse(&meta) {
            return Err(level_error(&rel, Kind::Symlink, at_open));
        }
        if !meta.is_dir() {
            return Err(level_error(&rel, Kind::File, at_open));
        }
        Ok(Some(GitDir {
            path,
            held: vec![file],
            prefix: rel,
        }))
    }

    fn kind_here(&self, name: &str) -> Result<Kind, GitDirError> {
        match std::fs::symlink_metadata(self.path.join(name)) {
            Ok(meta) => Ok(kind_of_meta(&meta)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Kind::Missing),
            Err(e) => Err(io_text(
                &join(&self.prefix, name),
                "cannot be inspected",
                &e,
            )),
        }
    }

    fn open_file_here(&self, name: &str) -> Result<Option<(File, FileMeta)>, GitDirError> {
        let rel = join(&self.prefix, name);
        fire(|| format!("file:{rel}"));
        let file = match open_held(&self.path.join(name), false) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io_text(&rel, "cannot be read", &e)),
        };
        let meta = file
            .metadata()
            .map_err(|e| io_text(&rel, "cannot be inspected", &e))?;
        if is_reparse(&meta) || !meta.is_file() {
            return Err(file_error(&rel, kind_of_meta(&meta)));
        }
        Ok(Some((
            file,
            FileMeta {
                size: meta.len(),
                mode: meta_mode(&meta),
            },
        )))
    }

    /// Every entry of this directory, sorted by name. The directory and every
    /// level above it are held, so the listing is of the directory that was
    /// opened.
    pub(super) fn list(&self) -> Result<Vec<Entry>, GitDirError> {
        let shown = if self.prefix.is_empty() {
            "the repo"
        } else {
            self.prefix.as_str()
        };
        let read =
            std::fs::read_dir(&self.path).map_err(|e| io_text(shown, "cannot be listed", &e))?;
        let mut out = Vec::new();
        for entry in read {
            let entry = entry.map_err(|e| io_text(shown, "cannot be listed", &e))?;
            let meta = entry
                .metadata()
                .map_err(|e| io_text(shown, "cannot be listed", &e))?;
            out.push(Entry {
                name: entry.file_name(),
                kind: kind_of_meta(&meta),
                size: meta.len(),
                mode: 0,
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    /// The permission bits of this directory: none on this platform.
    pub(super) fn mode(&self) -> Result<u32, GitDirError> {
        Ok(0)
    }
}
