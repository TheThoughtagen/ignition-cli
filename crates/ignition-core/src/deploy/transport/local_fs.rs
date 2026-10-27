//! Directory-handle file operations for the local `ApplyHost` (Phase 23).
//!
//! A [`Dir`] is a directory the runner opened itself. Every operation takes
//! the components below it, never a path, and never follows a symlink: on unix
//! each component is opened `O_DIRECTORY | O_NOFOLLOW` from the handle before
//! it (descriptor-relative, like `tree::dirfd`), so a component swapped for a
//! symlink after an earlier check fails the call instead of redirecting it. On
//! other platforms the same operations are path-based with a
//! `symlink_metadata` check per component, a documented residual (std has no
//! descriptor-relative calls there).
//!
//! The private files (`journal.json`, snapshots, the record, the ledger) are
//! created `0600` on unix. Files that will land in the collection (staged
//! copies) are created with the default mode, so the gateway can read them.

/// What a name is (never followed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Missing,
    Dir,
    File,
    Symlink,
    Other,
}

/// Why an operation was refused.
#[derive(Debug)]
pub(super) enum FsError {
    /// A component is, or became, a symlink or a non-directory, or a handle
    /// is not what was inspected.
    Changed,
    Missing,
    Exists,
    NotEmpty,
    Io(std::io::Error),
    /// `cause` ended an operation that had written the temporary file `temp`,
    /// and taking that file away failed too: it may still be there.
    Leftover {
        cause: Box<FsError>,
        temp: String,
    },
}

impl FsError {
    /// A value-free description.
    pub(super) fn describe(&self) -> String {
        match self {
            FsError::Changed => {
                "a path component is, or became, a symlink or a non-directory; refused".to_string()
            }
            FsError::Missing => "it does not exist".to_string(),
            FsError::Exists => "it already exists".to_string(),
            FsError::NotEmpty => "it is not empty".to_string(),
            FsError::Io(e) => format!("i/o error ({})", e.kind()),
            FsError::Leftover { cause, temp } => format!(
                "{}; the temporary file {temp} may still be there",
                cause.describe()
            ),
        }
    }
}

/// How a file is created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Visibility {
    /// The default mode: the file will be read by another account.
    Shared,
    /// Owner only (`0600`).
    Private,
}

pub(super) type FsResult<T> = Result<T, FsError>;

/// `cause`, with the note that the temporary file `temp` could not be removed.
fn leftover(cause: FsError, temp: &str) -> FsError {
    FsError::Leftover {
        cause: Box::new(cause),
        temp: temp.to_string(),
    }
}

/// The suffix of the sibling a replace or an exclusive publish writes first.
pub(super) const TEMP_SUFFIX: &str = ".ign-new";

#[cfg(not(unix))]
pub(super) use portable::Dir;
#[cfg(unix)]
pub(super) use unix::Dir;

#[cfg(unix)]
mod unix {
    use std::fs::File;
    use std::io::Write;
    use std::path::Path;

    use rustix::fs::{AtFlags, Mode, OFlags, linkat, openat};

    use super::{FsError, FsResult, Kind, TEMP_SUFFIX, Visibility, leftover};
    use crate::deploy::tree::dirfd::{self, FdError};

    fn lift(e: FdError) -> FsError {
        match e {
            FdError::Changed => FsError::Changed,
            FdError::Missing => FsError::Missing,
            FdError::Exists => FsError::Exists,
            FdError::NotEmpty => FsError::NotEmpty,
            FdError::Io(e) => FsError::Io(e),
        }
    }

    fn lift_errno(e: rustix::io::Errno) -> FsError {
        if e == rustix::io::Errno::LOOP
            || e == rustix::io::Errno::NOTDIR
            || e == rustix::io::Errno::NXIO
        {
            FsError::Changed
        } else if e == rustix::io::Errno::NOENT {
            FsError::Missing
        } else if e == rustix::io::Errno::EXIST {
            FsError::Exists
        } else {
            FsError::Io(e.into())
        }
    }

    fn kind_of(k: dirfd::Kind) -> Kind {
        match k {
            dirfd::Kind::Missing => Kind::Missing,
            dirfd::Kind::Dir => Kind::Dir,
            dirfd::Kind::File => Kind::File,
            dirfd::Kind::Symlink => Kind::Symlink,
            dirfd::Kind::Other => Kind::Other,
        }
    }

    /// An opened directory.
    #[derive(Debug)]
    pub(in crate::deploy::transport) struct Dir {
        fd: File,
    }

    impl Dir {
        /// Open `path` as a directory, not following a final symlink.
        pub(in crate::deploy::transport) fn root(path: &Path) -> FsResult<Dir> {
            dirfd::open_root(path).map(|fd| Dir { fd }).map_err(lift)
        }

        /// Open the existing directory at `parts` below this one.
        pub(in crate::deploy::transport) fn open(&self, parts: &[&str]) -> FsResult<Dir> {
            dirfd::walk(&self.fd, parts)
                .map(|fd| Dir { fd })
                .map_err(lift)
        }

        /// Open `parts` below this directory, creating each missing level.
        pub(in crate::deploy::transport) fn ensure(&self, parts: &[&str]) -> FsResult<Dir> {
            dirfd::ensure_walk(&self.fd, parts)
                .map(|fd| Dir { fd })
                .map_err(lift)
        }

        fn parent<'a>(&self, parts: &'a [&'a str]) -> FsResult<Option<(File, &'a str)>> {
            let Some((name, parents)) = parts.split_last() else {
                return Err(FsError::Changed);
            };
            match dirfd::walk(&self.fd, parents) {
                Ok(dir) => Ok(Some((dir, name))),
                Err(FdError::Missing) => Ok(None),
                Err(e) => Err(lift(e)),
            }
        }

        /// `(device, inode)` of the name at `parts`; `None` when absent.
        pub(in crate::deploy::transport) fn ident(
            &self,
            parts: &[&str],
        ) -> FsResult<Option<(u64, u64)>> {
            match self.parent(parts)? {
                None => Ok(None),
                Some((dir, name)) => dirfd::ident_at(&dir, name).map_err(lift),
            }
        }

        /// What the last component of `parts` is; `Missing` when any level is
        /// absent.
        pub(in crate::deploy::transport) fn kind(&self, parts: &[&str]) -> FsResult<Kind> {
            match self.parent(parts)? {
                None => Ok(Kind::Missing),
                Some((dir, name)) => dirfd::kind_at(&dir, name).map(kind_of).map_err(lift),
            }
        }

        /// The bytes of the regular file at `parts`, at most `cap` of them;
        /// `None` when absent. A non-regular file or a file over `cap` is an
        /// error.
        pub(in crate::deploy::transport) fn read(
            &self,
            parts: &[&str],
            cap: u64,
        ) -> FsResult<Option<Vec<u8>>> {
            let Some((dir, name)) = self.parent(parts)? else {
                return Ok(None);
            };
            match dirfd::read_file(&dir, name, cap) {
                Ok((_, bytes)) if bytes.len() as u64 > cap => Err(FsError::Io(
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "over the size cap"),
                )),
                Ok((_, bytes)) => Ok(Some(bytes)),
                Err(FdError::Missing) => Ok(None),
                Err(e) => Err(lift(e)),
            }
        }

        fn create_in(dir: &File, name: &str, vis: Visibility) -> FsResult<File> {
            let mode = match vis {
                Visibility::Shared => 0o666,
                Visibility::Private => 0o600,
            };
            openat(
                dir,
                name,
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(mode),
            )
            .map(File::from)
            .map_err(lift_errno)
        }

        /// Create the regular file at `parts` (parents created), which must
        /// not exist.
        pub(in crate::deploy::transport) fn create_new(
            &self,
            parts: &[&str],
            bytes: &[u8],
            vis: Visibility,
        ) -> FsResult<()> {
            let Some((name, parents)) = parts.split_last() else {
                return Err(FsError::Changed);
            };
            let dir = dirfd::ensure_walk(&self.fd, parents).map_err(lift)?;
            let mut file = Self::create_in(&dir, name, vis)?;
            file.write_all(bytes).map_err(FsError::Io)?;
            file.sync_all().map_err(FsError::Io)
        }

        /// Write `bytes` to a temp sibling and link it to `parts`, so the
        /// file appears whole or not at all and an existing one is never
        /// replaced (`Exists`). Private.
        pub(in crate::deploy::transport) fn publish_new(
            &self,
            parts: &[&str],
            bytes: &[u8],
        ) -> FsResult<()> {
            let Some((name, parents)) = parts.split_last() else {
                return Err(FsError::Changed);
            };
            let dir = dirfd::ensure_walk(&self.fd, parents).map_err(lift)?;
            let temp = format!("{name}{TEMP_SUFFIX}");
            self.write_temp(&dir, &temp, bytes)?;
            let linked = linkat(&dir, temp.as_str(), &dir, *name, AtFlags::empty());
            let _ = dirfd::unlink_at(&dir, &temp);
            linked.map_err(lift_errno)
        }

        /// Write `bytes` to a temp sibling and rename it over `parts`
        /// (replacing a file, atomically). Private.
        pub(in crate::deploy::transport) fn replace(
            &self,
            parts: &[&str],
            bytes: &[u8],
        ) -> FsResult<()> {
            let Some((name, _)) = parts.split_last() else {
                return Err(FsError::Changed);
            };
            self.replace_via(parts, &format!("{name}{TEMP_SUFFIX}"), bytes)
        }

        /// [`Self::replace`] through the sibling named `temp`, for a caller
        /// whose writes must not share a temporary name with another run's.
        /// The temporary file is removed when the write, the flush or the
        /// rename fails; a removal that fails is reported in the error.
        pub(in crate::deploy::transport) fn replace_via(
            &self,
            parts: &[&str],
            temp: &str,
            bytes: &[u8],
        ) -> FsResult<()> {
            let Some((name, parents)) = parts.split_last() else {
                return Err(FsError::Changed);
            };
            let dir = dirfd::ensure_walk(&self.fd, parents).map_err(lift)?;
            match dirfd::kind_at(&dir, name).map_err(lift)? {
                dirfd::Kind::Missing | dirfd::Kind::File => {}
                _ => return Err(FsError::Changed),
            }
            self.write_temp(&dir, temp, bytes)?;
            dirfd::rename_at(&dir, temp, &dir, name).map_err(|e| {
                let cause = lift(e);
                match dirfd::unlink_at(&dir, temp) {
                    Ok(()) | Err(FdError::Missing) => cause,
                    Err(_) => leftover(cause, temp),
                }
            })
        }

        fn write_temp(&self, dir: &File, temp: &str, bytes: &[u8]) -> FsResult<()> {
            self.write_temp_with(dir, temp, |file| {
                file.write_all(bytes)?;
                file.sync_all()
            })
        }

        /// Create `temp` (replacing a plain file of that name) and fill it
        /// through `fill`. When `fill` fails the file is removed again; a
        /// removal that fails is reported in the error.
        pub(super) fn write_temp_with(
            &self,
            dir: &File,
            temp: &str,
            fill: impl FnOnce(&mut File) -> std::io::Result<()>,
        ) -> FsResult<()> {
            match dirfd::kind_at(dir, temp).map_err(lift)? {
                dirfd::Kind::Missing => {}
                dirfd::Kind::File => dirfd::unlink_at(dir, temp).map_err(lift)?,
                _ => return Err(FsError::Changed),
            }
            let mut file = Self::create_in(dir, temp, Visibility::Private)?;
            match fill(&mut file) {
                Ok(()) => Ok(()),
                Err(e) => {
                    drop(file);
                    let cause = FsError::Io(e);
                    Err(match dirfd::unlink_at(dir, temp) {
                        Ok(()) | Err(FdError::Missing) => cause,
                        Err(_) => leftover(cause, temp),
                    })
                }
            }
        }

        /// Move `from` (below this directory) to `to` below `to_dir`,
        /// replacing an existing file. The target's parents are created.
        pub(in crate::deploy::transport) fn rename(
            &self,
            from: &[&str],
            to_dir: &Dir,
            to: &[&str],
        ) -> FsResult<()> {
            let (Some((fname, fparents)), Some((tname, tparents))) =
                (from.split_last(), to.split_last())
            else {
                return Err(FsError::Changed);
            };
            let src = dirfd::walk(&self.fd, fparents).map_err(lift)?;
            let dst = dirfd::ensure_walk(&to_dir.fd, tparents).map_err(lift)?;
            dirfd::rename_at(&src, fname, &dst, tname).map_err(lift)
        }

        /// Remove the non-directory at `parts`.
        pub(in crate::deploy::transport) fn unlink(&self, parts: &[&str]) -> FsResult<()> {
            match self.parent(parts)? {
                None => Err(FsError::Missing),
                Some((dir, name)) => dirfd::unlink_at(&dir, name).map_err(lift),
            }
        }

        /// Remove the empty directory at `parts`.
        pub(in crate::deploy::transport) fn rmdir(&self, parts: &[&str]) -> FsResult<()> {
            match self.parent(parts)? {
                None => Err(FsError::Missing),
                Some((dir, name)) => dirfd::rmdir_at(&dir, name).map_err(lift),
            }
        }

        /// The names in the directory at `parts` with what each is, sorted
        /// by byte.
        pub(in crate::deploy::transport) fn list(
            &self,
            parts: &[&str],
        ) -> FsResult<Vec<(String, Kind)>> {
            let dir = dirfd::walk(&self.fd, parts).map_err(lift)?;
            let mut names = Vec::new();
            for entry in rustix::fs::Dir::read_from(&dir).map_err(|e| FsError::Io(e.into()))? {
                let entry = entry.map_err(|e| FsError::Io(e.into()))?;
                let raw = entry.file_name().to_bytes();
                if raw == b"." || raw == b".." {
                    continue;
                }
                let name = std::str::from_utf8(raw)
                    .map_err(|_| FsError::Changed)?
                    .to_string();
                let kind = dirfd::kind_at(&dir, &name).map(kind_of).map_err(lift)?;
                names.push((name, kind));
            }
            names.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            Ok(names)
        }

        /// Remove the directory at `parts` and everything below it. Nothing
        /// is followed: a symlink inside is unlinked, never entered. Absent
        /// is success.
        pub(in crate::deploy::transport) fn remove_tree(&self, parts: &[&str]) -> FsResult<()> {
            let Some((name, _)) = parts.split_last() else {
                return Err(FsError::Changed);
            };
            match self.parent(parts)? {
                None => return Ok(()),
                Some((parent, _)) => match dirfd::kind_at(&parent, name).map_err(lift)? {
                    dirfd::Kind::Missing => return Ok(()),
                    dirfd::Kind::Dir => {}
                    _ => return Err(FsError::Changed),
                },
            }
            let dir = Dir::open(self, parts)?;
            dir.empty_out()?;
            let (parent, name) = self.parent(parts)?.ok_or(FsError::Missing)?;
            dirfd::rmdir_at(&parent, name).map_err(lift)
        }

        /// Remove every entry of this directory, recursively.
        fn empty_out(&self) -> FsResult<()> {
            for (name, kind) in self.list(&[])? {
                match kind {
                    Kind::Dir => {
                        let child = self.open(&[name.as_str()])?;
                        child.empty_out()?;
                        dirfd::rmdir_at(&self.fd, &name).map_err(lift)?;
                    }
                    _ => dirfd::unlink_at(&self.fd, &name).map_err(lift)?,
                }
            }
            Ok(())
        }

        /// Whether this process may write into the directory (`access(2)`).
        pub(in crate::deploy::transport) fn writable(&self) -> bool {
            rustix::fs::accessat(
                &self.fd,
                ".",
                rustix::fs::Access::WRITE_OK | rustix::fs::Access::EXEC_OK,
                AtFlags::empty(),
            )
            .is_ok()
        }
    }

    #[cfg(test)]
    mod tests {
        use std::io::Write as _;

        use super::{Dir, FsError};

        fn names(dir: &std::path::Path) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(dir)
                .expect("read_dir")
                .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }

        /// WR-04: a temporary file whose write or flush fails (a full disk) is
        /// removed again, and the cause is the error.
        #[test]
        fn a_failed_temp_write_removes_the_temp_file() {
            let tmp = tempfile::tempdir().expect("tempdir");
            let root = Dir::root(tmp.path()).expect("root");
            let err = root
                .write_temp_with(&root.fd, "rec.tmp-r1", |file| {
                    file.write_all(b"half")?;
                    Err(std::io::Error::other("disk full"))
                })
                .expect_err("the fill fails");
            assert!(matches!(err, FsError::Io(_)), "{err:?}");
            assert!(names(tmp.path()).is_empty(), "{:?}", names(tmp.path()));
        }

        /// WR-04: when the temporary file cannot be removed either, the error
        /// says it may still be there.
        #[test]
        fn a_temp_file_that_cannot_be_removed_is_reported() {
            let tmp = tempfile::tempdir().expect("tempdir");
            let root = Dir::root(tmp.path()).expect("root");
            let path = tmp.path().join("rec.tmp-r1");
            let err = root
                .write_temp_with(&root.fd, "rec.tmp-r1", |_| {
                    // The name now belongs to a directory, which `unlink` refuses.
                    std::fs::remove_file(&path)?;
                    std::fs::create_dir(&path)?;
                    Err(std::io::Error::other("disk full"))
                })
                .expect_err("the fill fails");
            assert!(matches!(err, FsError::Leftover { .. }), "{err:?}");
            let shown = err.describe();
            assert!(
                shown.contains("the temporary file rec.tmp-r1 may still be there"),
                "{shown}"
            );
            assert!(shown.contains("i/o error"), "the cause is kept: {shown}");
        }

        /// `replace_via` writes through the named sibling and leaves nothing of it.
        #[test]
        fn replace_via_uses_the_named_sibling() {
            let tmp = tempfile::tempdir().expect("tempdir");
            let root = Dir::root(tmp.path()).expect("root");
            std::fs::write(tmp.path().join("rec.ign-new"), b"stranger").expect("seed");
            root.replace_via(&["rec"], "rec.tmp-r1", b"new")
                .expect("replace");
            assert_eq!(std::fs::read(tmp.path().join("rec")).expect("rec"), b"new");
            assert_eq!(
                std::fs::read(tmp.path().join("rec.ign-new")).expect("stranger"),
                b"stranger"
            );
            assert_eq!(names(tmp.path()), ["rec", "rec.ign-new"]);
        }
    }
}

#[cfg(not(unix))]
mod portable {
    use std::io::Write;
    use std::path::{Path, PathBuf};

    use super::{FsError, FsResult, Kind, TEMP_SUFFIX, Visibility, leftover};

    fn kind_of(path: &Path) -> FsResult<Kind> {
        match std::fs::symlink_metadata(path) {
            Ok(m) => {
                let t = m.file_type();
                Ok(if t.is_symlink() {
                    Kind::Symlink
                } else if t.is_dir() {
                    Kind::Dir
                } else if t.is_file() {
                    Kind::File
                } else {
                    Kind::Other
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Kind::Missing),
            Err(e) => Err(FsError::Io(e)),
        }
    }

    fn io(e: std::io::Error) -> FsError {
        match e.kind() {
            std::io::ErrorKind::NotFound => FsError::Missing,
            std::io::ErrorKind::AlreadyExists => FsError::Exists,
            _ => FsError::Io(e),
        }
    }

    /// An opened directory (path-based residual on this platform).
    #[derive(Debug)]
    pub(in crate::deploy::transport) struct Dir {
        path: PathBuf,
    }

    impl Dir {
        pub(in crate::deploy::transport) fn root(path: &Path) -> FsResult<Dir> {
            match kind_of(path)? {
                Kind::Dir => Ok(Dir {
                    path: path.to_path_buf(),
                }),
                Kind::Missing => Err(FsError::Missing),
                _ => Err(FsError::Changed),
            }
        }

        /// The path of `parts`, every directory level checked.
        fn walk(&self, parts: &[&str], create: bool) -> FsResult<PathBuf> {
            let mut path = self.path.clone();
            for part in parts {
                path.push(part);
                match kind_of(&path)? {
                    Kind::Dir => {}
                    Kind::Missing if create => std::fs::create_dir(&path).map_err(io)?,
                    Kind::Missing => return Err(FsError::Missing),
                    _ => return Err(FsError::Changed),
                }
            }
            Ok(path)
        }

        pub(in crate::deploy::transport) fn open(&self, parts: &[&str]) -> FsResult<Dir> {
            Ok(Dir {
                path: self.walk(parts, false)?,
            })
        }

        pub(in crate::deploy::transport) fn ensure(&self, parts: &[&str]) -> FsResult<Dir> {
            Ok(Dir {
                path: self.walk(parts, true)?,
            })
        }

        fn parent_path(&self, parts: &[&str], create: bool) -> FsResult<Option<(PathBuf, String)>> {
            let Some((name, parents)) = parts.split_last() else {
                return Err(FsError::Changed);
            };
            match self.walk(parents, create) {
                Ok(dir) => Ok(Some((dir, (*name).to_string()))),
                Err(FsError::Missing) => Ok(None),
                Err(e) => Err(e),
            }
        }

        pub(in crate::deploy::transport) fn ident(
            &self,
            _parts: &[&str],
        ) -> FsResult<Option<(u64, u64)>> {
            Ok(None)
        }

        pub(in crate::deploy::transport) fn kind(&self, parts: &[&str]) -> FsResult<Kind> {
            match self.parent_path(parts, false)? {
                None => Ok(Kind::Missing),
                Some((dir, name)) => kind_of(&dir.join(name)),
            }
        }

        pub(in crate::deploy::transport) fn read(
            &self,
            parts: &[&str],
            cap: u64,
        ) -> FsResult<Option<Vec<u8>>> {
            use std::io::Read;
            let Some((dir, name)) = self.parent_path(parts, false)? else {
                return Ok(None);
            };
            let path = dir.join(name);
            match kind_of(&path)? {
                Kind::Missing => return Ok(None),
                Kind::File => {}
                _ => return Err(FsError::Changed),
            }
            let file = std::fs::File::open(&path).map_err(io)?;
            let mut bytes = Vec::new();
            file.take(cap + 1)
                .read_to_end(&mut bytes)
                .map_err(FsError::Io)?;
            if bytes.len() as u64 > cap {
                return Err(FsError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "over the size cap",
                )));
            }
            Ok(Some(bytes))
        }

        fn create_file(path: &Path, bytes: &[u8]) -> FsResult<()> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(io)?;
            file.write_all(bytes).map_err(FsError::Io)?;
            file.sync_all().map_err(FsError::Io)
        }

        pub(in crate::deploy::transport) fn create_new(
            &self,
            parts: &[&str],
            bytes: &[u8],
            _vis: Visibility,
        ) -> FsResult<()> {
            let (dir, name) = self.parent_path(parts, true)?.ok_or(FsError::Missing)?;
            Self::create_file(&dir.join(name), bytes)
        }

        fn write_temp(dir: &Path, temp: &str, bytes: &[u8]) -> FsResult<PathBuf> {
            let path = dir.join(temp);
            match kind_of(&path)? {
                Kind::Missing => {}
                Kind::File => std::fs::remove_file(&path).map_err(io)?,
                _ => return Err(FsError::Changed),
            }
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(io)?;
            if let Err(e) = file.write_all(bytes).and_then(|()| file.sync_all()) {
                // The file is ours (`create_new`): take it away, and say so
                // when that fails.
                drop(file);
                let cause = FsError::Io(e);
                return Err(match std::fs::remove_file(&path) {
                    Ok(()) => cause,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => cause,
                    Err(_) => leftover(cause, temp),
                });
            }
            Ok(path)
        }

        pub(in crate::deploy::transport) fn publish_new(
            &self,
            parts: &[&str],
            bytes: &[u8],
        ) -> FsResult<()> {
            let (dir, name) = self.parent_path(parts, true)?.ok_or(FsError::Missing)?;
            let temp = Self::write_temp(&dir, &format!("{name}{TEMP_SUFFIX}"), bytes)?;
            let linked = std::fs::hard_link(&temp, dir.join(&name));
            let _ = std::fs::remove_file(&temp);
            linked.map_err(io)
        }

        pub(in crate::deploy::transport) fn replace(
            &self,
            parts: &[&str],
            bytes: &[u8],
        ) -> FsResult<()> {
            let Some((name, _)) = parts.split_last() else {
                return Err(FsError::Changed);
            };
            self.replace_via(parts, &format!("{name}{TEMP_SUFFIX}"), bytes)
        }

        /// [`Self::replace`] through the sibling named `temp`, for a caller
        /// whose writes must not share a temporary name with another run's.
        pub(in crate::deploy::transport) fn replace_via(
            &self,
            parts: &[&str],
            temp: &str,
            bytes: &[u8],
        ) -> FsResult<()> {
            let (dir, name) = self.parent_path(parts, true)?.ok_or(FsError::Missing)?;
            match kind_of(&dir.join(&name))? {
                Kind::Missing | Kind::File => {}
                _ => return Err(FsError::Changed),
            }
            let temp_path = Self::write_temp(&dir, temp, bytes)?;
            std::fs::rename(&temp_path, dir.join(&name)).map_err(|e| {
                let cause = io(e);
                match std::fs::remove_file(&temp_path) {
                    Ok(()) => cause,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => cause,
                    Err(_) => leftover(cause, temp),
                }
            })
        }

        pub(in crate::deploy::transport) fn rename(
            &self,
            from: &[&str],
            to_dir: &Dir,
            to: &[&str],
        ) -> FsResult<()> {
            let (src_dir, src_name) = self.parent_path(from, false)?.ok_or(FsError::Missing)?;
            let (dst_dir, dst_name) = to_dir.parent_path(to, true)?.ok_or(FsError::Missing)?;
            std::fs::rename(src_dir.join(src_name), dst_dir.join(dst_name)).map_err(io)
        }

        pub(in crate::deploy::transport) fn unlink(&self, parts: &[&str]) -> FsResult<()> {
            let (dir, name) = self.parent_path(parts, false)?.ok_or(FsError::Missing)?;
            std::fs::remove_file(dir.join(name)).map_err(io)
        }

        pub(in crate::deploy::transport) fn rmdir(&self, parts: &[&str]) -> FsResult<()> {
            let (dir, name) = self.parent_path(parts, false)?.ok_or(FsError::Missing)?;
            std::fs::remove_dir(dir.join(name)).map_err(|e| {
                if std::fs::read_dir(dir_join_name(&self.path, parts))
                    .map(|mut it| it.next().is_some())
                    .unwrap_or(false)
                {
                    FsError::NotEmpty
                } else {
                    io(e)
                }
            })
        }

        pub(in crate::deploy::transport) fn list(
            &self,
            parts: &[&str],
        ) -> FsResult<Vec<(String, Kind)>> {
            let dir = self.walk(parts, false)?;
            let mut names = Vec::new();
            for entry in std::fs::read_dir(&dir).map_err(io)? {
                let entry = entry.map_err(FsError::Io)?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| FsError::Changed)?;
                let kind = kind_of(&entry.path())?;
                names.push((name, kind));
            }
            names.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            Ok(names)
        }

        pub(in crate::deploy::transport) fn remove_tree(&self, parts: &[&str]) -> FsResult<()> {
            let Some((dir, name)) = self.parent_path(parts, false)? else {
                return Ok(());
            };
            let path = dir.join(name);
            match kind_of(&path)? {
                Kind::Missing => Ok(()),
                Kind::Dir => {
                    Self::empty_path(&path).and_then(|()| std::fs::remove_dir(&path).map_err(io))
                }
                _ => Err(FsError::Changed),
            }
        }

        fn empty_path(path: &Path) -> FsResult<()> {
            for entry in std::fs::read_dir(path).map_err(io)? {
                let entry = entry.map_err(FsError::Io)?;
                let child = entry.path();
                match kind_of(&child)? {
                    Kind::Dir => {
                        Self::empty_path(&child)?;
                        std::fs::remove_dir(&child).map_err(io)?;
                    }
                    _ => std::fs::remove_file(&child).map_err(io)?,
                }
            }
            Ok(())
        }

        pub(in crate::deploy::transport) fn writable(&self) -> bool {
            std::fs::metadata(&self.path)
                .map(|m| !m.permissions().readonly())
                .unwrap_or(false)
        }
    }

    fn dir_join_name(base: &Path, parts: &[&str]) -> PathBuf {
        let mut path = base.to_path_buf();
        for part in parts {
            path.push(part);
        }
        path
    }
}

/// Join `parts` for a message.
pub(super) fn shown(parts: &[&str]) -> String {
    parts.join("/")
}
