//! The ssh read half: [`SftpTransport`] reads a Linux gateway host's
//! `data/config/resources/<collection>/...` trees and the declared-names
//! ledger over system `sftp` (22-06).
//!
//! ## The protocol cannot mistake a failure for an absence
//!
//! One sftp batch aborts on its first failing command, and a `-` prefix on a
//! command would hide that failure. So no batch line ever starts with `-`
//! (the typed [`SftpOp`] cannot render one), and a path appears in a batch
//! only after an EARLIER session's listing showed it. Every session lists one
//! tree level of every directory that level needs, at once:
//!
//! 1. `data_dir`, then `config`, then `config/resources`: three sessions that
//!    prove each level is a real directory (not a symlink).
//! 2. The collections (`core`, the mode, `local`), then their modules, then
//!    the scoped types (and for tags the four tag types, then the declared
//!    providers): one session per level.
//! 3. Inside a scoped directory, one session per depth lists every directory
//!    of that depth with `ls -la`, so every file, its size, and any symlink or
//!    special file is known BEFORE a byte is downloaded.
//! 4. Download sessions fetch each listed file with its own `get` (never
//!    `get -R`, whose contents the server would decide) into a private temp
//!    dir, in chunks; the running total is checked before each chunk, the
//!    temp dir is measured while sftp runs and sftp is killed if it outgrows
//!    what the listing allowed, and afterwards the local walk must find
//!    exactly the listed files with exactly the listed sizes.
//!
//! Any non-zero sftp exit, any stderr byte or a timeout is `transport_failed`
//! (see [`RemoteShell::require_success`]); a directory that is permission
//! denied, vanishes between sessions, or whose download is missing locally is
//! `transport_failed`, never an absent or empty resource. Only a name missing
//! from its parent's listing reads as absent.
//!
//! ## Untrusted input
//!
//! The host controls every listing line. A listing line is accepted only when
//! it carries the exact listed-directory prefix, a recognised mode column and
//! a numeric size; a name that is empty, holds `/`, or fails
//! [`safe_remote_path`] (control characters, quotes, backslash, glob
//! characters) is refused. A name that passed those checks is the only
//! remote-derived text that ever reaches a later batch line, always inside the
//! double quotes of a typed op. Symlinks and special files are refused (the
//! sftp client's `get -R` would otherwise skip a symlink silently), the three
//! `READ_MAX_*` caps are enforced from the listing before the download and
//! again on the downloaded bytes (entries while the listing is parsed), and
//! the depth of the walk is bounded by
//! [`READ_MAX_DEPTH`]. Entries are counted while a listing is parsed, so an
//! oversized listing stops at the line that passes the cap. File hashes are computed here on the downloaded bytes,
//! never by a remote tool, and the downloads live in a per-read temp dir that
//! is deleted on drop.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::remote::{RemoteShell, SftpOp, safe_remote_path};
use super::{
    CORE_COLLECTION, DeleteSet, FileBlob, MIRROR_MIN_COMPONENTS, MirrorReport, ObservedFiles,
    READ_MAX_ENTRIES, READ_MAX_FILE_BYTES, READ_MAX_TOTAL_BYTES, Scope, Staged, StagingMode,
    TreeMirror, TreeReader, absolute_components, copy_to_temp, mirror_mode_problem,
    read_staged_copy, validate_mirror_inputs, validate_mirror_paths, validate_staging_root,
};
use crate::client::config_resources::validate_segment;
use crate::deploy::component_problem;
use crate::deploy::layout::{RelPath, TAG_TYPES};
use crate::deploy::ledger::{LEDGER_MAX_BYTES, ledger_location};
use crate::deploy::plan::unit_key;
use crate::deploy::tree::{OpenError, RenderedTree, open_regular_file, sha256_hex, walk_capped};
use crate::error::CoreError;

/// The collection that overrides the mode collection on the gateway.
const LOCAL_COLLECTION: &str = "local";

/// Path components from `data_dir` to the collections.
const RESOURCES: [&str; 2] = ["config", "resources"];

/// A walk deeper than this many components below `data_dir` is refused. Each
/// level costs one sftp session, so a hostile deep tree cannot drive an
/// unbounded number of them.
pub const READ_MAX_DEPTH: usize = 64;

/// One download session carries at most this many listed bytes. The total cap
/// is checked before each chunk, so a tree near [`READ_MAX_TOTAL_BYTES`] is
/// fetched in several sessions, each verified before the next starts.
const DOWNLOAD_CHUNK_BYTES: u64 = 256 * 1024 * 1024;
/// One download session carries at most this many `get` lines.
const DOWNLOAD_CHUNK_FILES: usize = 10_000;
/// How far past the listed bytes the download dir may grow before the sftp
/// process is killed (a server may legitimately differ by a rounding error,
/// not by this much).
const DOWNLOAD_SLACK_BYTES: u64 = 1024 * 1024;
/// How often the download dir is measured while sftp runs.
const WATCH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);

/// What kind of filesystem object a listing line describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A regular file (`-`).
    File,
    /// A directory (`d`).
    Dir,
    /// A symbolic link (`l`).
    Symlink,
    /// Anything else (device, socket, fifo, unknown).
    Other,
}

/// One entry of a directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedEntry {
    /// The entry's name: one path component, already checked by
    /// [`safe_remote_path`].
    pub name: String,
    /// What the listing says it is.
    pub kind: EntryKind,
    /// The size column (a directory's size is not meaningful).
    pub size: u64,
}

/// The POSIX data directory of an `ssh` node, ready to be used as a path
/// prefix: absolute, checked by [`safe_remote_path`], free of empty, `.` and
/// `..` components, with the trailing `/` trimmed.
pub fn posix_root(data_dir: &str) -> Result<String, String> {
    if !data_dir.starts_with('/') {
        return Err(format!(
            "{data_dir:?} is not an absolute POSIX path (it must start with `/`)"
        ));
    }
    safe_remote_path(data_dir)?;
    let trimmed = data_dir.trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("the filesystem root cannot be a gateway data directory".to_string());
    }
    if trimmed
        .split('/')
        .skip(1)
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(format!(
            "{data_dir:?} must not contain empty, `.` or `..` components"
        ));
    }
    Ok(trimmed.to_string())
}

/// Parse the stdout of one `@ls -la "<dir>"` batch for the single directory
/// `dir`. See [`parse_batch_listing`].
pub fn parse_ls_listing(dir: &str, stdout: &[u8]) -> Result<Vec<ListedEntry>, String> {
    let mut all = parse_batch_listing(&[dir.to_string()], stdout)?;
    Ok(all.pop().unwrap_or_default())
}

/// Why a listing was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListingError {
    /// More entries than the budget allows. Raised while the output is being
    /// parsed, so nothing after the offending line is looked at or stored.
    TooMany,
    /// A line, name or mode that cannot be trusted.
    Bad(String),
}

impl std::fmt::Display for ListingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ListingError::TooMany => write!(f, "more than {READ_MAX_ENTRIES} entries"),
            ListingError::Bad(why) => f.write_str(why),
        }
    }
}

/// [`parse_batch_listing_capped`] with the whole [`READ_MAX_ENTRIES`] budget.
pub fn parse_batch_listing(
    dirs: &[String],
    stdout: &[u8],
) -> Result<Vec<Vec<ListedEntry>>, String> {
    parse_batch_listing_capped(dirs, stdout, READ_MAX_ENTRIES).map_err(|e| e.to_string())
}

/// Parse the stdout of a batch that listed `dirs` in order, one
/// `ls -la` each, into one entry list per directory (byte-sorted by name).
///
/// Every line is `<mode> <links> <owner> <group> <size> <month> <day>
/// <time-or-year> <dir>/<name>`: the listing prints the listed directory
/// before each name, so the exact prefix is found and ONE component is taken
/// after it. A line without a prefix of a listed directory, with an empty or
/// multi-component name, with an unrecognised mode or a non-numeric size, a
/// name that fails [`safe_remote_path`], or a name listed twice is an error.
/// `.` and `..` (present because the listing includes dot-named entries) are
/// dropped.
///
/// Entries are counted AS EACH LINE IS PARSED: the line that makes the count
/// pass `max_entries` stops the parse with [`ListingError::TooMany`] before
/// it is stored and before any later line is read, so a huge or endless
/// listing is never materialised. The 32 MiB stdout cap of the process runner
/// is a backstop, not this check.
pub fn parse_batch_listing_capped(
    dirs: &[String],
    stdout: &[u8],
    max_entries: usize,
) -> Result<Vec<Vec<ListedEntry>>, ListingError> {
    let bad = |why: String| ListingError::Bad(why);
    let text = std::str::from_utf8(stdout)
        .map_err(|_| bad("the listing holds a name that is not valid UTF-8".to_string()))?;
    let prefixes: Vec<String> = dirs.iter().map(|d| format!("{d}/")).collect();
    let mut out: Vec<Vec<ListedEntry>> = vec![Vec::new(); dirs.len()];
    // `.` and `..` of every directory are lines too
    let max_lines = max_entries.saturating_add(dirs.len().saturating_mul(2));
    let mut lines_seen = 0usize;
    let mut entries_seen = 0usize;
    // Output follows the order of the listing commands, so a line belongs to
    // the current directory or a later one.
    let mut current = 0usize;
    for line in text.lines() {
        lines_seen += 1;
        if lines_seen > max_lines {
            return Err(ListingError::TooMany);
        }
        if line.is_empty() {
            return Err(bad("the listing holds an empty line".to_string()));
        }
        let mut hit = None;
        for (index, prefix) in prefixes.iter().enumerate().skip(current) {
            let Some(at) = line.rfind(prefix.as_str()) else {
                continue;
            };
            let name = &line[at + prefix.len()..];
            if at == 0 || line.as_bytes()[at - 1] != b' ' || name.contains('/') {
                continue;
            }
            hit = Some((index, at, name));
            break;
        }
        let Some((index, at, name)) = hit else {
            return Err(bad(format!(
                "a listing line does not carry a listed directory prefix: {line:?}"
            )));
        };
        current = index;
        if name != "." && name != ".." {
            entries_seen += 1;
            if entries_seen > max_entries {
                return Err(ListingError::TooMany);
            }
        }
        let tokens: Vec<&str> = line[..at].split_whitespace().collect();
        if tokens.len() < 8 {
            return Err(bad(format!("a listing line has too few columns: {line:?}")));
        }
        let mode = tokens[0];
        if mode.len() < 10 || !mode.is_ascii() {
            return Err(bad(format!("a listing line has an unusable mode {mode:?}")));
        }
        let kind = match mode.as_bytes()[0] {
            b'-' => EntryKind::File,
            b'd' => EntryKind::Dir,
            b'l' => EntryKind::Symlink,
            _ => EntryKind::Other,
        };
        let size_text = tokens[tokens.len() - 4];
        let size: u64 = size_text.parse().map_err(|_| {
            bad(format!(
                "a listing line has a non-numeric size {size_text:?}"
            ))
        })?;
        if name == "." || name == ".." {
            continue;
        }
        if name.is_empty() {
            return Err(bad("a listing line has an empty name".to_string()));
        }
        safe_remote_path(name).map_err(|why| bad(format!("the name {name:?} is unsafe: {why}")))?;
        out[index].push(ListedEntry {
            name: name.to_string(),
            kind,
            size,
        });
    }
    for entries in &mut out {
        entries.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
        if let Some(pair) = entries.windows(2).find(|w| w[0].name == w[1].name) {
            return Err(bad(format!("the name {:?} is listed twice", pair[0].name)));
        }
    }
    Ok(out)
}

/// Running totals of one read.
#[derive(Default)]
struct Budget {
    entries: usize,
}

/// Why walking down a fixed path stopped.
enum Descend {
    /// An sftp session failed.
    Failed(CoreError),
    /// A component (display path relative to `data_dir`) is a symlink.
    Symlink(String),
    /// A component (display path relative to `data_dir`) is not a directory.
    NotDir(String),
}

impl From<CoreError> for Descend {
    fn from(err: CoreError) -> Self {
        Descend::Failed(err)
    }
}

/// A file the listing found inside a scoped directory.
#[derive(Debug, Clone)]
struct FoundFile {
    collection: String,
    /// Collection-relative path.
    rel: String,
    size: u64,
}

/// What discovery found.
struct Found {
    /// Scoped directories that exist (chains below `data_dir`).
    roots: Vec<Vec<String>>,
    /// Every file below a root.
    files: Vec<FoundFile>,
}

/// Reads a Linux gateway data directory over system `sftp`.
pub struct SftpTransport {
    shell: RemoteShell,
    data_dir: String,
    label: String,
    sessions: AtomicUsize,
    mirror: Option<MirrorSetup>,
}

fn transport_failed(label: &str, detail: impl std::fmt::Display) -> CoreError {
    CoreError::TransportFailed {
        operation: "read".to_string(),
        node: label.to_string(),
        detail: detail.to_string(),
    }
}

impl SftpTransport {
    /// A reader over `data_dir` (use [`posix_root`] to build it from a
    /// target); `label` (e.g. `nodes.master`) names the node in errors. A
    /// `data_dir` that cannot be placed in a batch line is refused when it
    /// is first used, before any process is spawned.
    pub fn new(shell: RemoteShell, data_dir: String, label: impl Into<String>) -> Self {
        SftpTransport {
            shell,
            data_dir,
            label: label.into(),
            sessions: AtomicUsize::new(0),
            mirror: None,
        }
    }

    /// How many sftp sessions (ssh handshakes) this reader has run so far.
    pub fn sessions(&self) -> usize {
        self.sessions.load(Ordering::Relaxed)
    }

    fn fail(&self, detail: impl std::fmt::Display) -> CoreError {
        transport_failed(&self.label, detail)
    }

    /// The absolute remote path of `chain` below `data_dir`.
    fn abs(&self, chain: &[String]) -> String {
        let mut path = self.data_dir.clone();
        for part in chain {
            path.push('/');
            path.push_str(part);
        }
        path
    }

    /// One sftp session: list every directory of `dirs` (`ls -la`).
    ///
    /// Every parsed entry counts against `budget`, which is shared by all
    /// listings of one read, so [`READ_MAX_ENTRIES`] is cumulative across
    /// directories, levels and collections.
    async fn list_level(
        &self,
        dirs: &[String],
        budget: &mut Budget,
    ) -> Result<Vec<Vec<ListedEntry>>, CoreError> {
        self.list_level_as("read", dirs, budget).await
    }

    /// [`list_level`](Self::list_level) reporting failures as operation `op`.
    async fn list_level_as(
        &self,
        op: &str,
        dirs: &[String],
        budget: &mut Budget,
    ) -> Result<Vec<Vec<ListedEntry>>, CoreError> {
        if dirs.is_empty() {
            return Ok(Vec::new());
        }
        let ops: Vec<SftpOp> = dirs
            .iter()
            .map(|path| SftpOp::LsLongAll { path: path.clone() })
            .collect();
        self.sessions.fetch_add(1, Ordering::Relaxed);
        let out = self.shell.sftp_batch(&ops).await?;
        let out = self.shell.require_success(op, out)?;
        let remaining = READ_MAX_ENTRIES.saturating_sub(budget.entries);
        let fail = |detail: String| CoreError::TransportFailed {
            operation: op.to_string(),
            node: self.label.clone(),
            detail,
        };
        let listings =
            parse_batch_listing_capped(dirs, &out.stdout, remaining).map_err(|e| match e {
                ListingError::TooMany => fail(format!(
                    "more than {READ_MAX_ENTRIES} entries under the data directory (at {})",
                    self.shown(&dirs[0])
                )),
                ListingError::Bad(why) => fail(format!("cannot use the sftp listing: {why}")),
            })?;
        budget.entries += listings.iter().map(Vec::len).sum::<usize>();
        Ok(listings)
    }

    /// `abs` relative to `data_dir`, for messages.
    fn shown(&self, abs: &str) -> String {
        abs.strip_prefix(&self.data_dir)
            .map(|rest| rest.trim_start_matches('/'))
            .filter(|rest| !rest.is_empty())
            .unwrap_or(".")
            .to_string()
    }

    /// Walk the fixed `components` below `data_dir`, one listing session per
    /// level. `Ok(None)` when a component is missing from its parent's
    /// listing; otherwise the absolute path and listing entry of the last
    /// component, whatever its kind (a symlink anywhere is refused).
    async fn descend(
        &self,
        components: &[&str],
        budget: &mut Budget,
    ) -> Result<Option<(String, ListedEntry)>, Descend> {
        let mut current = self.data_dir.clone();
        let mut shown = String::new();
        for (index, component) in components.iter().enumerate() {
            let last = index + 1 == components.len();
            let mut listed = self
                .list_level(std::slice::from_ref(&current), budget)
                .await?;
            let entries = listed.pop().unwrap_or_default();
            let Some(entry) = entries.into_iter().find(|e| e.name == *component) else {
                return Ok(None);
            };
            if !shown.is_empty() {
                shown.push('/');
            }
            shown.push_str(component);
            current = format!("{current}/{component}");
            match entry.kind {
                EntryKind::Symlink => return Err(Descend::Symlink(shown)),
                EntryKind::Dir => {}
                _ if last => {}
                _ => return Err(Descend::NotDir(shown)),
            }
            if last {
                return Ok(Some((current, entry)));
            }
        }
        Ok(None)
    }

    /// `descend`'s refusal as a read failure.
    fn read_problem(&self, problem: Descend) -> CoreError {
        match problem {
            Descend::Failed(err) => err,
            Descend::Symlink(rel) => self.fail(format!("{rel} is a symlink; symlinks are refused")),
            Descend::NotDir(rel) => self.fail(format!("{rel} is not a directory")),
        }
    }

    /// The directory chains a read of `scope` opens, each starting with
    /// `config/resources/<collection>`.
    fn target_chains(&self, scope: &Scope) -> Result<BTreeSet<Vec<String>>, CoreError> {
        if let Some(why) = component_problem(&scope.mode) {
            return Err(self.fail(format!("the target mode {why}")));
        }
        safe_remote_path(&scope.mode)
            .map_err(|why| self.fail(format!("the target mode is unsafe: {why}")))?;
        let mut dirs: Vec<Vec<String>> = Vec::new();
        for (module, ty) in &scope.types {
            validate_segment("module", module).map_err(|e| self.fail(e))?;
            validate_segment("type", ty).map_err(|e| self.fail(e))?;
            dirs.push(vec![module.clone(), ty.clone()]);
        }
        for provider in &scope.tag_providers {
            if let Some(why) = component_problem(provider) {
                return Err(self.fail(format!("tag provider {provider:?} {why}")));
            }
            safe_remote_path(provider)
                .map_err(|why| self.fail(format!("tag provider {provider:?} is unsafe: {why}")))?;
            for tag_type in TAG_TYPES {
                dirs.push(vec![
                    "ignition".to_string(),
                    (*tag_type).to_string(),
                    provider.clone(),
                ]);
            }
        }
        let collections: BTreeSet<&str> = [CORE_COLLECTION, scope.mode.as_str(), LOCAL_COLLECTION]
            .into_iter()
            .collect();
        let mut chains = BTreeSet::new();
        for collection in collections {
            for dir in &dirs {
                let mut chain: Vec<String> = RESOURCES.iter().map(|c| (*c).to_string()).collect();
                chain.push(collection.to_string());
                chain.extend(dir.iter().cloned());
                chains.insert(chain);
            }
        }
        Ok(chains)
    }

    /// List every directory the read needs, level by level, and return the
    /// scoped directories that exist and every file below them.
    async fn discover(
        &self,
        chains: &BTreeSet<Vec<String>>,
        budget: &mut Budget,
    ) -> Result<Found, CoreError> {
        let mut roots = Vec::new();
        let mut files = Vec::new();
        let mut frontier: Vec<Vec<String>> = vec![Vec::new()];
        while !frontier.is_empty() {
            let dirs: Vec<String> = frontier.iter().map(|c| self.abs(c)).collect();
            let listings = self.list_level(&dirs, budget).await?;
            let mut next: Vec<Vec<String>> = Vec::new();
            for (chain, entries) in frontier.iter().zip(listings) {
                let inside = chains.iter().any(|t| chain.starts_with(t.as_slice()));
                if chains.contains(chain) {
                    roots.push(chain.clone());
                }
                // `config` and `config/resources` must exist: a data directory
                // without them is not a gateway data directory, which is a
                // failure and never an empty tree.
                let required = RESOURCES.get(chain.len()).copied();
                if let Some(need) = required
                    && !entries.iter().any(|e| e.name == need)
                {
                    return Err(self.fail(format!(
                        "{} cannot be read (not found); is data_dir a gateway data directory?",
                        RESOURCES[..=chain.len()].join("/")
                    )));
                }
                for entry in entries {
                    let mut child = chain.clone();
                    child.push(entry.name.clone());
                    let shown = child.join("/");
                    if inside {
                        match entry.kind {
                            EntryKind::Dir => next.push(child),
                            EntryKind::File => {
                                if let (Some(collection), true) = (chain.get(2), chain.len() >= 3) {
                                    let mut rel = chain[3..].join("/");
                                    if !rel.is_empty() {
                                        rel.push('/');
                                    }
                                    rel.push_str(&entry.name);
                                    files.push(FoundFile {
                                        collection: collection.clone(),
                                        rel,
                                        size: entry.size,
                                    });
                                }
                            }
                            EntryKind::Symlink => {
                                return Err(self
                                    .fail(format!("{shown} is a symlink; symlinks are refused")));
                            }
                            EntryKind::Other => {
                                return Err(self
                                    .fail(format!("{shown} is not a regular file or directory")));
                            }
                        }
                    } else if required == Some(entry.name.as_str())
                        || chains.iter().any(|t| t.starts_with(child.as_slice()))
                    {
                        match entry.kind {
                            EntryKind::Dir => next.push(child),
                            EntryKind::Symlink => {
                                return Err(self
                                    .fail(format!("{shown} is a symlink; symlinks are refused")));
                            }
                            _ => return Err(self.fail(format!("{shown} is not a directory"))),
                        }
                    }
                }
            }
            if next.iter().any(|c| c.len() > READ_MAX_DEPTH) {
                return Err(self.fail(format!(
                    "the tree under config/resources is deeper than {READ_MAX_DEPTH} levels"
                )));
            }
            frontier = next;
        }
        roots.sort();
        Ok(Found { roots, files })
    }

    /// Run one download session and kill it if `dir` grows past `limit`
    /// bytes. The listing said how much each `get` may deliver, so a server
    /// that sends more is aborted mid-transfer (the dropped batch future kills
    /// the sftp child) instead of filling the runner's disk; the caller's
    /// temp dir is then removed on drop.
    async fn run_download(
        &self,
        op: &str,
        ops: &[SftpOp],
        dir: &Path,
        limit: u64,
    ) -> Result<(), CoreError> {
        self.sessions.fetch_add(1, Ordering::Relaxed);
        let batch = self.shell.sftp_batch(ops);
        let watch = async {
            loop {
                tokio::time::sleep(WATCH_INTERVAL).await;
                let target = dir.to_path_buf();
                let used = tokio::task::spawn_blocking(move || dir_bytes(&target))
                    .await
                    .unwrap_or(u64::MAX);
                if used > limit {
                    return used;
                }
            }
        };
        tokio::pin!(batch);
        tokio::pin!(watch);
        tokio::select! {
            out = &mut batch => {
                let out = out?;
                self.shell.require_success(op, out)?;
                Ok(())
            }
            used = &mut watch => Err(self.mirror_fail(op, format!(
                "the download wrote {used} bytes, more than the {limit} bytes the listing allowed; it was aborted"
            ))),
        }
    }

    /// Fetch the listed `files` into `tmp` with one `get` per file, in chunks.
    /// Before each chunk the running total (actual bytes already on disk plus
    /// the chunk's listed bytes) must fit [`READ_MAX_TOTAL_BYTES`]; after each
    /// chunk every delivered file must be exactly its listed size.
    async fn fetch_files(&self, files: &[&FoundFile], tmp: &Path) -> Result<(), CoreError> {
        let mut downloaded = 0u64;
        let mut next = 0usize;
        while next < files.len() {
            let start = next;
            let mut ops = Vec::new();
            let mut chunk_listed = 0u64;
            while next < files.len() && ops.len() < DOWNLOAD_CHUNK_FILES {
                let file = files[next];
                if !ops.is_empty() && chunk_listed + file.size > DOWNLOAD_CHUNK_BYTES {
                    break;
                }
                if downloaded + chunk_listed + file.size > READ_MAX_TOTAL_BYTES {
                    return Err(self.fail(format!(
                        "the files under config/resources exceed {READ_MAX_TOTAL_BYTES} bytes in total (at config/resources/{}/{})",
                        file.collection, file.rel
                    )));
                }
                let local = file
                    .rel
                    .split('/')
                    .fold(tmp.join(&file.collection), |p, c| p.join(c));
                if let Some(parent) = local.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        self.fail(format!("cannot create a download dir ({:?})", e.kind()))
                    })?;
                }
                ops.push(SftpOp::Get {
                    remote: format!(
                        "{}/config/resources/{}/{}",
                        self.data_dir, file.collection, file.rel
                    ),
                    local,
                });
                chunk_listed += file.size;
                next += 1;
            }
            let limit = downloaded + chunk_listed + DOWNLOAD_SLACK_BYTES;
            self.run_download("read", &ops, tmp, limit).await?;
            downloaded += self.check_chunk(&files[start..next], tmp)?;
        }
        Ok(())
    }

    /// The delivered size of each file of a finished chunk must be exactly
    /// the listed size (and within the per-file cap); returns the bytes.
    fn check_chunk(&self, chunk: &[&FoundFile], tmp: &Path) -> Result<u64, CoreError> {
        let mut bytes = 0u64;
        for file in chunk {
            let local = file
                .rel
                .split('/')
                .fold(tmp.join(&file.collection), |p, c| p.join(c));
            let shown = format!("config/resources/{}/{}", file.collection, file.rel);
            let meta = match std::fs::symlink_metadata(&local) {
                Ok(meta) => meta,
                // reported with the other missing files by the final check
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(self.fail(format!("cannot inspect {shown} ({:?})", e.kind()))),
            };
            if !meta.file_type().is_file() {
                return Err(self.fail(format!("{shown} was downloaded as a non-regular file")));
            }
            let len = meta.len();
            if len > READ_MAX_FILE_BYTES {
                return Err(self.fail(format!(
                    "{shown} is {len} bytes, over the {READ_MAX_FILE_BYTES} byte file cap"
                )));
            }
            if len != file.size {
                return Err(self.fail(format!(
                    "{shown} is {len} bytes but was listed as {}; it changed while it was read",
                    file.size
                )));
            }
            bytes += len;
        }
        Ok(bytes)
    }

    /// A fresh private directory for downloads, inside the shell's own temp
    /// dir when it has one. Deleted when dropped.
    fn download_dir(&self, op: &str) -> Result<tempfile::TempDir, CoreError> {
        let mut builder = tempfile::Builder::new();
        builder.prefix("ign-read-");
        let made = match self.shell.temp_dir() {
            Some(parent) => builder.tempdir_in(parent),
            None => builder.tempdir(),
        };
        made.map_err(|e| {
            self.mirror_fail(op, format!("cannot create a download dir ({:?})", e.kind()))
        })
    }
}

/// Bytes held by regular files under `dir`, without following symlinks. A
/// tree with more than [`READ_MAX_ENTRIES`] entries, or one that cannot be
/// measured, counts as unbounded so the caller aborts.
fn dir_bytes(dir: &Path) -> u64 {
    let mut total = 0u64;
    let mut entries = 0usize;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in read.flatten() {
            entries += 1;
            if entries > READ_MAX_ENTRIES {
                return u64::MAX;
            }
            let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                total = total.saturating_add(meta.len());
            }
        }
    }
    total
}

/// Why reading a downloaded file back was refused.
enum Readback {
    Io(std::io::ErrorKind),
    Changed,
    Capped(u64),
    Grew,
    SizeMismatch { listed: u64, actual: u64 },
}

/// Read one downloaded file: open it descriptor-relative from `root`
/// (a swapped-in symlink is not followed), require the size to equal what
/// the listing said and to be within `cap`, and return its bytes.
fn read_downloaded(root: &Path, rel: &Path, listed: u64, cap: u64) -> Result<Vec<u8>, Readback> {
    let path = root.join(rel);
    let before = std::fs::symlink_metadata(&path).map_err(|e| Readback::Io(e.kind()))?;
    if !before.file_type().is_file() {
        return Err(Readback::Changed);
    }
    let (file, held) = open_regular_file(root, rel, &before).map_err(|e| match e {
        OpenError::Changed => Readback::Changed,
        OpenError::Io(e) => Readback::Io(e.kind()),
    })?;
    let len = held.len();
    if len > cap {
        return Err(Readback::Capped(len));
    }
    if len != listed {
        return Err(Readback::SizeMismatch {
            listed,
            actual: len,
        });
    }
    let mut bytes = Vec::with_capacity(len as usize);
    file.take(cap + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| Readback::Io(e.kind()))?;
    if bytes.len() as u64 > cap {
        return Err(Readback::Grew);
    }
    Ok(bytes)
}

/// Check the downloads of `roots` under `tmp_root` against the listing and
/// return the hashed blobs per collection. Runs on a blocking thread.
fn verify_download(
    label: &str,
    tmp_root: &Path,
    roots: &[Vec<String>],
    listed: &BTreeMap<(String, String), u64>,
) -> Result<BTreeMap<String, BTreeMap<RelPath, FileBlob>>, CoreError> {
    let fail = |detail: String| transport_failed(label, detail);
    let mut actual: BTreeMap<(String, String), PathBuf> = BTreeMap::new();
    for root in roots {
        let below: Vec<&str> = root.iter().skip(2).map(String::as_str).collect();
        let shown = format!("config/resources/{}", below.join("/"));
        let dir = below.iter().fold(tmp_root.to_path_buf(), |p, c| p.join(c));
        match std::fs::symlink_metadata(&dir) {
            Ok(meta) if meta.file_type().is_dir() => {}
            Ok(_) => {
                return Err(fail(format!(
                    "{shown} was downloaded as something other than a directory"
                )));
            }
            // nothing was fetched into it (an empty directory); a listed file
            // that is missing is reported below
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(fail(format!(
                    "{shown} cannot be inspected after the download ({:?})",
                    e.kind()
                )));
            }
        }
        let mut files: BTreeMap<String, PathBuf> = BTreeMap::new();
        let mut dirs: BTreeSet<String> = BTreeSet::new();
        walk_capped(&dir, &shown, &mut files, &mut dirs, READ_MAX_ENTRIES).map_err(fail)?;
        let collection = below.first().copied().unwrap_or_default();
        let strip = format!("config/resources/{collection}/");
        for (full, path) in files {
            let rel = full.strip_prefix(&strip).unwrap_or(&full).to_string();
            actual.insert((collection.to_string(), rel), path);
        }
    }
    if let Some(key) = listed.keys().find(|k| !actual.contains_key(*k)) {
        return Err(fail(format!(
            "config/resources/{}/{} was listed but not downloaded; the tree changed while it was read",
            key.0, key.1
        )));
    }
    if let Some(key) = actual.keys().find(|k| !listed.contains_key(*k)) {
        return Err(fail(format!(
            "config/resources/{}/{} appeared in the download but was not listed; the tree changed while it was read",
            key.0, key.1
        )));
    }
    let mut out: BTreeMap<String, BTreeMap<RelPath, FileBlob>> = BTreeMap::new();
    let mut total = 0u64;
    for ((collection, rel), path) in actual {
        let shown = format!("config/resources/{collection}/{rel}");
        let key = RelPath::new(&rel)
            .map_err(|why| fail(format!("{shown} has an unusable name ({why})")))?;
        let size = listed.get(&(collection.clone(), rel.clone())).copied();
        let Some(size) = size else { continue };
        let Ok(under) = path.strip_prefix(tmp_root) else {
            return Err(fail(format!("{shown} is outside the download dir")));
        };
        let bytes = read_downloaded(tmp_root, under, size, READ_MAX_FILE_BYTES).map_err(|e| {
            fail(match e {
                Readback::Io(kind) => format!("cannot read {shown} ({kind:?})"),
                Readback::Changed => format!("{shown} changed while it was read"),
                Readback::Capped(len) => format!(
                    "{shown} is {len} bytes, over the {READ_MAX_FILE_BYTES} byte file cap"
                ),
                Readback::Grew => format!(
                    "{shown} grew past the {READ_MAX_FILE_BYTES} byte file cap while it was read"
                ),
                Readback::SizeMismatch { listed, actual } => format!(
                    "{shown} is {actual} bytes but was listed as {listed}; it changed while it was read"
                ),
            })
        })?;
        total += bytes.len() as u64;
        if total > READ_MAX_TOTAL_BYTES {
            return Err(fail(format!(
                "the files under config/resources exceed {READ_MAX_TOTAL_BYTES} bytes in total (at {shown})"
            )));
        }
        let sha256 = sha256_hex(&bytes);
        out.entry(collection)
            .or_default()
            .insert(key, FileBlob { bytes, sha256 });
    }
    Ok(out)
}

#[async_trait::async_trait]
impl TreeReader for SftpTransport {
    async fn list_types(&self, collection: &str) -> Result<Vec<(String, String)>, CoreError> {
        if let Some(why) = component_problem(collection) {
            return Err(self.fail(format!("the collection name {why}")));
        }
        safe_remote_path(collection)
            .map_err(|why| self.fail(format!("the collection name is unsafe: {why}")))?;
        let mut budget = Budget::default();
        let found = self
            .descend(&[RESOURCES[0], RESOURCES[1], collection], &mut budget)
            .await
            .map_err(|p| self.read_problem(p))?;
        let Some((collection_dir, entry)) = found else {
            return Ok(Vec::new());
        };
        if entry.kind != EntryKind::Dir {
            return Err(self.fail(format!("config/resources/{collection} is not a directory")));
        }
        // every parsed entry already counted against the budget
        let child_dirs = |entries: Vec<ListedEntry>, rel: &str| -> Result<Vec<String>, CoreError> {
            let mut names = Vec::new();
            for entry in entries {
                match entry.kind {
                    EntryKind::Symlink => {
                        return Err(self.fail(format!(
                            "{rel}/{} is a symlink; symlinks are refused",
                            entry.name
                        )));
                    }
                    EntryKind::Dir => names.push(entry.name),
                    _ => {}
                }
            }
            Ok(names)
        };
        let coll_rel = format!("config/resources/{collection}");
        let top = self
            .list_level(std::slice::from_ref(&collection_dir), &mut budget)
            .await?
            .pop()
            .unwrap_or_default();
        let modules = child_dirs(top, &coll_rel)?;
        let module_dirs: Vec<String> = modules
            .iter()
            .map(|m| format!("{collection_dir}/{m}"))
            .collect();
        let listings = self.list_level(&module_dirs, &mut budget).await?;
        let mut pairs = Vec::new();
        for (module, entries) in modules.iter().zip(listings) {
            for ty in child_dirs(entries, &format!("{coll_rel}/{module}"))? {
                pairs.push((module.clone(), ty));
            }
        }
        pairs.sort_by(|a, b| {
            (a.0.as_bytes(), a.1.as_bytes()).cmp(&(b.0.as_bytes(), b.1.as_bytes()))
        });
        Ok(pairs)
    }

    async fn read(&self, scope: &Scope) -> Result<ObservedFiles, CoreError> {
        let chains = self.target_chains(scope)?;
        let mut budget = Budget::default();
        let found = self.discover(&chains, &mut budget).await?;

        // Caps from the listing, before a byte is downloaded (local files are
        // listed only for their names and are not read).
        let mut total = 0u64;
        let mut listed: BTreeMap<(String, String), u64> = BTreeMap::new();
        let mut local_keys: BTreeMap<RelPath, ()> = BTreeMap::new();
        for file in &found.files {
            let shown = format!("config/resources/{}/{}", file.collection, file.rel);
            if file.collection == LOCAL_COLLECTION {
                let key = RelPath::new(&file.rel)
                    .map_err(|why| self.fail(format!("{shown} has an unusable name ({why})")))?;
                local_keys.insert(key, ());
                continue;
            }
            if file.size > READ_MAX_FILE_BYTES {
                return Err(self.fail(format!(
                    "{shown} is {} bytes, over the {READ_MAX_FILE_BYTES} byte file cap",
                    file.size
                )));
            }
            total += file.size;
            if total > READ_MAX_TOTAL_BYTES {
                return Err(self.fail(format!(
                    "the files under config/resources exceed {READ_MAX_TOTAL_BYTES} bytes in total (at {shown})"
                )));
            }
            listed.insert((file.collection.clone(), file.rel.clone()), file.size);
        }

        let download_roots: Vec<Vec<String>> = found
            .roots
            .iter()
            .filter(|root| root.get(2).map(String::as_str) != Some(LOCAL_COLLECTION))
            .cloned()
            .collect();
        let mut by_collection: BTreeMap<String, BTreeMap<RelPath, FileBlob>> = BTreeMap::new();
        if !download_roots.is_empty() {
            // One `get` per listed file, never `get -R`: the server decides
            // what a recursive get delivers, so a recursive get would bypass
            // every per-file check made on the listing.
            let tmp = self.download_dir("read")?;
            let fetch: Vec<&FoundFile> = found
                .files
                .iter()
                .filter(|f| f.collection != LOCAL_COLLECTION)
                .collect();
            self.fetch_files(&fetch, tmp.path()).await?;

            let label = self.label.clone();
            by_collection = tokio::task::spawn_blocking(move || {
                verify_download(&label, tmp.path(), &download_roots, &listed)
            })
            .await
            .map_err(|e| CoreError::Internal(format!("sftp read task failed: {e}")))??;
        }

        let local_units: BTreeSet<String> = local_keys
            .keys()
            .map(|k| unit_key(k, &local_keys))
            .collect();
        Ok(ObservedFiles {
            core: by_collection
                .get(CORE_COLLECTION)
                .cloned()
                .unwrap_or_default(),
            mode: by_collection.get(&scope.mode).cloned().unwrap_or_default(),
            local_units: local_units.into_iter().collect(),
        })
    }

    async fn read_ledger(&self, gateway: &str, mode: &str) -> Result<Option<Vec<u8>>, CoreError> {
        let location = ledger_location(gateway, mode);
        let bad = |problem: String| CoreError::DeployLedgerInvalid {
            location: location.clone(),
            problem,
        };
        for (what, value) in [("gateway", gateway), ("mode", mode)] {
            if let Some(why) = component_problem(value) {
                return Err(bad(format!("the target {what} {why}")));
            }
        }
        let parts: Vec<&str> = location.split('/').collect();
        let mut budget = Budget::default();
        let found = self
            .descend(&parts, &mut budget)
            .await
            .map_err(|problem| match problem {
                Descend::Failed(err) => err,
                Descend::Symlink(_) => {
                    bad("a path component is a symlink; symlinks are refused".to_string())
                }
                Descend::NotDir(_) => bad("a path component is not a directory".to_string()),
            })?;
        let Some((remote, entry)) = found else {
            return Ok(None);
        };
        if entry.kind != EntryKind::File {
            return Err(bad("the ledger is not a regular file".to_string()));
        }
        if entry.size > LEDGER_MAX_BYTES as u64 {
            return Err(bad(format!(
                "the file is {} bytes, over the {LEDGER_MAX_BYTES} byte cap",
                entry.size
            )));
        }
        let tmp = self.download_dir("read")?;
        self.run_download(
            "read",
            &[SftpOp::Get {
                remote,
                local: tmp.path().join("ledger"),
            }],
            tmp.path(),
            entry.size + DOWNLOAD_SLACK_BYTES,
        )
        .await?;
        let size = entry.size;
        let root = tmp.path().to_path_buf();
        let read = tokio::task::spawn_blocking(move || {
            let result = read_downloaded(&root, Path::new("ledger"), size, LEDGER_MAX_BYTES as u64);
            drop(tmp);
            result
        })
        .await
        .map_err(|e| CoreError::Internal(format!("sftp read task failed: {e}")))?;
        match read {
            Ok(bytes) => Ok(Some(bytes)),
            Err(Readback::Io(kind)) => Err(bad(format!("cannot be read ({kind:?})"))),
            Err(Readback::Changed) => Err(bad(
                "changed while it was read; symlinks are refused".to_string()
            )),
            Err(Readback::Capped(len)) => Err(bad(format!(
                "the file is {len} bytes, over the {LEDGER_MAX_BYTES} byte cap"
            ))),
            Err(Readback::Grew) => Err(bad(format!(
                "the file grew past the {LEDGER_MAX_BYTES} byte cap while it was read"
            ))),
            Err(Readback::SizeMismatch { listed, actual }) => Err(bad(format!(
                "the file is {actual} bytes but was listed as {listed}; it changed while it was read"
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// The write half (22-07)
// ---------------------------------------------------------------------------
//
// Integrity: a listing size says nothing about content, so before the first
// rename (`Separate`) every staged copy on the host is fetched back with a
// typed `get` into a per-operation temp dir and hashed HERE against the
// SHA-256 the stage recorded (no remote hash command). The directories to
// create lead the same batch as the renames, so nothing sits between that
// verification and the moves. sftp has no atomic verify-then-rename, so after
// the moves (and for `InPlace`, which has no staged copy on the host) the
// FINAL paths are read back and hashed again before the first delete; a
// mismatch is `transport_failed` ("content changed on the host during
// commit") and no delete runs. The read-backs use the read side's watchdog
// and caps and `open_regular_file`. What remains is a host-side writer with
// write access to the staging or collection directory.

/// What the write half needs: the mode collection and the staging mode (the
/// staging root normalised to an absolute POSIX path).
#[derive(Debug, Clone)]
struct MirrorSetup {
    mode: String,
    staging: StagingMode,
}

/// What a survey listing says about one path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seen {
    Missing,
    Dir,
    File(u64),
}

/// A file on the host to read back and compare with its staged SHA-256.
struct RemoteCopy {
    rel: RelPath,
    remote: String,
    sha: String,
    size: u64,
}

/// At most this many mismatches are named in an error.
const MISMATCH_NAMED: usize = 8;

/// `base` plus `chain`, joined with `/`.
fn join_abs(base: &str, chain: &[String]) -> String {
    let mut path = base.to_string();
    for part in chain {
        path.push('/');
        path.push_str(part);
    }
    path
}

/// The first [`MISMATCH_NAMED`] entries of `items`, joined for an error.
fn named(items: &[String]) -> String {
    let mut text = items
        .iter()
        .take(MISMATCH_NAMED)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > MISMATCH_NAMED {
        text.push_str(&format!(" and {} more", items.len() - MISMATCH_NAMED));
    }
    text
}

fn chain_of(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|p| (*p).to_string()).collect()
}

impl SftpTransport {
    /// Bind the write half to the `mode` collection with `staging`. A
    /// `Separate` root is normalised to an absolute POSIX path here and must
    /// lie outside `config/resources`.
    pub fn with_mirror(mut self, mode: &str, staging: StagingMode) -> Result<Self, String> {
        if let Some(why) = mirror_mode_problem(mode) {
            return Err(why);
        }
        safe_remote_path(mode).map_err(|why| format!("the mode is unsafe: {why}"))?;
        let resources = self.resources_display();
        validate_staging_root(&staging, &resources)?;
        let staging = match staging {
            StagingMode::Separate { root } => {
                let parts = absolute_components(&root)
                    .map_err(|why| format!("the staging root {root:?} {why}"))?;
                if !root.starts_with('/') {
                    return Err(format!(
                        "the staging root {root:?} must be an absolute POSIX path on the host"
                    ));
                }
                let normal = format!("/{}", parts.join("/"));
                safe_remote_path(&normal)
                    .map_err(|why| format!("the staging root is unsafe: {why}"))?;
                StagingMode::Separate { root: normal }
            }
            StagingMode::InPlace => StagingMode::InPlace,
        };
        self.mirror = Some(MirrorSetup {
            mode: mode.to_string(),
            staging,
        });
        Ok(self)
    }

    /// `<data_dir>/config/resources`, for the staging-root containment check.
    fn resources_display(&self) -> String {
        format!("{}/config/resources", self.data_dir)
    }

    fn mirror_fail(&self, op: &str, detail: impl std::fmt::Display) -> CoreError {
        CoreError::TransportFailed {
            operation: op.to_string(),
            node: self.label.clone(),
            detail: detail.to_string(),
        }
    }

    fn mirror_setup(&self, op: &str) -> Result<&MirrorSetup, CoreError> {
        self.mirror.as_ref().ok_or_else(|| {
            self.mirror_fail(
                op,
                "this transport has no write half; build it with mirror_for_node",
            )
        })
    }

    /// Which node and mode a [`Staged`] belongs to.
    fn mirror_owner(&self, setup: &MirrorSetup) -> String {
        format!("sftp:{}:{}:{}", self.label, self.data_dir, setup.mode)
    }

    /// Run one mutating batch; an empty batch spawns nothing.
    async fn run_batch(&self, op: &str, ops: Vec<SftpOp>) -> Result<(), CoreError> {
        if ops.is_empty() {
            return Ok(());
        }
        self.sessions.fetch_add(1, Ordering::Relaxed);
        let out = self.shell.sftp_batch(&ops).await?;
        self.shell.require_success(op, out)?;
        Ok(())
    }

    /// Read-only: what every path in `targets` (chains below the existing
    /// directory `base`) is on the host, one listing session per depth.
    ///
    /// A directory is listed only when it is known to exist and has a needed
    /// child; a child of a missing directory is missing without a listing. A
    /// symlink or special file anywhere, or a file where a directory is
    /// needed, is refused. `base` itself is listed first, so a missing or
    /// unreadable `base` is `transport_failed`.
    async fn survey(
        &self,
        op: &str,
        base: &str,
        targets: &[Vec<String>],
        budget: &mut Budget,
    ) -> Result<BTreeMap<Vec<String>, Seen>, CoreError> {
        let mut needed: BTreeSet<Vec<String>> = BTreeSet::new();
        for target in targets {
            for len in 1..=target.len() {
                needed.insert(target[..len].to_vec());
            }
        }
        let mut has_child: BTreeSet<Vec<String>> = BTreeSet::new();
        for chain in &needed {
            if chain.len() > 1 {
                has_child.insert(chain[..chain.len() - 1].to_vec());
            }
        }
        let max_depth = needed.iter().map(Vec::len).max().unwrap_or(0);
        if max_depth > READ_MAX_DEPTH {
            return Err(self.mirror_fail(op, "a path is deeper than the walk bound"));
        }
        let mut seen: BTreeMap<Vec<String>, Seen> = BTreeMap::new();
        for depth in 0..max_depth {
            let children: Vec<&Vec<String>> =
                needed.iter().filter(|c| c.len() == depth + 1).collect();
            let mut parents: BTreeSet<Vec<String>> = BTreeSet::new();
            for child in &children {
                let parent = child[..depth].to_vec();
                let parent_state = if depth == 0 {
                    Seen::Dir
                } else {
                    seen.get(&parent).copied().unwrap_or(Seen::Missing)
                };
                if parent_state == Seen::Dir {
                    parents.insert(parent);
                }
            }
            let parents: Vec<Vec<String>> = parents.into_iter().collect();
            let dirs: Vec<String> = parents.iter().map(|p| join_abs(base, p)).collect();
            let listings = self.list_level_as(op, &dirs, budget).await?;
            for child in children {
                let parent = child[..depth].to_vec();
                let name = &child[depth];
                let state = match parents.iter().position(|p| *p == parent) {
                    None => Seen::Missing,
                    Some(index) => match listings[index].iter().find(|e| e.name == *name) {
                        None => Seen::Missing,
                        Some(entry) => {
                            let shown = self.shown(&join_abs(base, child));
                            match entry.kind {
                                EntryKind::Symlink => {
                                    return Err(self.mirror_fail(
                                        op,
                                        format!("{shown} is a symlink; symlinks are refused"),
                                    ));
                                }
                                EntryKind::Other => {
                                    return Err(self.mirror_fail(
                                        op,
                                        format!("{shown} is not a regular file or directory"),
                                    ));
                                }
                                EntryKind::File if has_child.contains(child) => {
                                    return Err(
                                        self.mirror_fail(op, format!("{shown} is not a directory"))
                                    );
                                }
                                EntryKind::File => Seen::File(entry.size),
                                EntryKind::Dir => Seen::Dir,
                            }
                        }
                    },
                };
                seen.insert(child.clone(), state);
            }
        }
        Ok(seen)
    }

    /// Create every directory in `missing` (chains below `base`), one `mkdir`
    /// session per depth, shallowest first. Only chains a survey showed
    /// missing are passed, so `mkdir` is never issued for an existing level.
    async fn make_dirs(
        &self,
        op: &str,
        base: &str,
        missing: &BTreeSet<Vec<String>>,
    ) -> Result<(), CoreError> {
        let max_depth = missing.iter().map(Vec::len).max().unwrap_or(0);
        for depth in 1..=max_depth {
            let ops: Vec<SftpOp> = missing
                .iter()
                .filter(|chain| chain.len() == depth)
                .map(|chain| SftpOp::Mkdir {
                    path: join_abs(base, chain),
                })
                .collect();
            self.run_batch(op, ops).await?;
        }
        Ok(())
    }

    /// The directories above `files` that a survey showed missing.
    fn dirs_to_make(
        seen: &BTreeMap<Vec<String>, Seen>,
        files: &[Vec<String>],
    ) -> BTreeSet<Vec<String>> {
        let mut missing = BTreeSet::new();
        for file in files {
            for len in 1..file.len() {
                let chain = file[..len].to_vec();
                if seen.get(&chain) == Some(&Seen::Missing) {
                    missing.insert(chain);
                }
            }
        }
        missing
    }

    /// Read every file of `copies` back from the host with typed `get` ops
    /// into a private per-operation temp dir and hash it here (never with a
    /// remote command). Downloads are chunked, the running total is checked
    /// before each chunk and the temp dir is measured while sftp runs, as on
    /// the read side; each downloaded file is re-read through
    /// [`open_regular_file`] with the byte caps.
    ///
    /// Returns the files whose size or SHA-256 differs from what was expected
    /// (empty when every copy is intact). A transfer or cap failure is an
    /// `Err`.
    async fn verify_remote_copies(
        &self,
        op: &str,
        copies: &[RemoteCopy],
    ) -> Result<Vec<String>, CoreError> {
        if copies.is_empty() {
            return Ok(Vec::new());
        }
        let tmp = self.download_dir(op)?;
        let mut downloaded = 0u64;
        let mut next = 0usize;
        while next < copies.len() {
            let mut ops = Vec::new();
            let mut chunk = 0u64;
            while next < copies.len() && ops.len() < DOWNLOAD_CHUNK_FILES {
                let copy = &copies[next];
                if !ops.is_empty() && chunk + copy.size > DOWNLOAD_CHUNK_BYTES {
                    break;
                }
                if downloaded + chunk + copy.size > READ_MAX_TOTAL_BYTES {
                    return Err(self.mirror_fail(
                        op,
                        format!(
                            "reading the files back would exceed {READ_MAX_TOTAL_BYTES} bytes in total (at {})",
                            copy.rel
                        ),
                    ));
                }
                let local = copy
                    .rel
                    .components()
                    .fold(tmp.path().to_path_buf(), |p, c| p.join(c));
                if let Some(parent) = local.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        self.mirror_fail(
                            op,
                            format!("cannot create a download dir ({:?})", e.kind()),
                        )
                    })?;
                }
                ops.push(SftpOp::Get {
                    remote: copy.remote.clone(),
                    local,
                });
                chunk += copy.size;
                next += 1;
            }
            let limit = downloaded + chunk + DOWNLOAD_SLACK_BYTES;
            self.run_download(op, &ops, tmp.path(), limit).await?;
            let root = tmp.path().to_path_buf();
            downloaded = tokio::task::spawn_blocking(move || dir_bytes(&root))
                .await
                .map_err(|e| CoreError::Internal(format!("sftp verify task failed: {e}")))?;
        }

        let root = tmp.path().to_path_buf();
        let wanted: Vec<(RelPath, String, u64)> = copies
            .iter()
            .map(|c| (c.rel.clone(), c.sha.clone(), c.size))
            .collect();
        let outcome = tokio::task::spawn_blocking(move || -> Result<Vec<String>, String> {
            let mut bad = Vec::new();
            for (rel, sha, size) in &wanted {
                match read_downloaded(&root, Path::new(rel.as_str()), *size, READ_MAX_FILE_BYTES)
                {
                    Ok(bytes) => {
                        if sha256_hex(&bytes) != *sha {
                            bad.push(format!("{rel} (SHA-256 differs)"));
                        }
                    }
                    Err(Readback::SizeMismatch { listed, actual }) => {
                        bad.push(format!(
                            "{rel} (SHA-256 not matched: {actual} bytes, expected {listed})"
                        ));
                    }
                    Err(Readback::Io(kind)) => {
                        return Err(format!("cannot read {rel} back ({kind:?})"));
                    }
                    Err(Readback::Changed) => {
                        return Err(format!("{rel} changed while it was read back"));
                    }
                    Err(Readback::Capped(len)) => {
                        return Err(format!(
                            "{rel} is {len} bytes, over the {READ_MAX_FILE_BYTES} byte file cap"
                        ));
                    }
                    Err(Readback::Grew) => {
                        return Err(format!(
                            "{rel} grew past the {READ_MAX_FILE_BYTES} byte file cap while it was read back"
                        ));
                    }
                }
            }
            Ok(bad)
        })
        .await
        .map_err(|e| CoreError::Internal(format!("sftp verify task failed: {e}")))?
        .map_err(|why| self.mirror_fail(op, why))?;
        Ok(outcome)
    }

    /// After a staged copy failed verification: remove the staged files from
    /// the staging root, but only after a fresh listing shows each one is
    /// still a regular file with no symlink above it (an `rm` follows
    /// directory symlinks). Never touches the collection. Returns a sentence
    /// saying what was removed and what was left.
    async fn discard_staged(&self, op: &str, root: &str, files: &[(RelPath, String)]) -> String {
        let Ok(parts) = absolute_components(root) else {
            return format!("The staged copies were left under {root}.");
        };
        let base = format!("/{}", parts[0]);
        let rest: Vec<String> = parts[1..].to_vec();
        let chains: Vec<Vec<String>> = files
            .iter()
            .map(|(path, _)| {
                let mut chain = rest.clone();
                chain.extend(path.components().map(str::to_string));
                chain
            })
            .collect();
        let mut budget = Budget::default();
        let seen = match self.survey(op, &base, &chains, &mut budget).await {
            Ok(seen) => seen,
            Err(_) => {
                return format!(
                    "The staging root {root} could not be re-checked as a plain directory tree, so the staged copies were left in place under it."
                );
            }
        };
        let mut rms = Vec::new();
        let mut kept = Vec::new();
        for ((path, _), chain) in files.iter().zip(&chains) {
            if matches!(seen.get(chain), Some(Seen::File(_))) {
                rms.push((
                    SftpOp::Rm {
                        path: format!("{root}/{path}"),
                    },
                    0,
                ));
            } else if !matches!(seen.get(chain), Some(Seen::Missing) | None) {
                kept.push(path.to_string());
            }
        }
        let count = rms.len();
        match self.run_chunked(op, rms).await {
            Ok(()) if kept.is_empty() => format!(
                "{count} staged copies were removed from {root} (empty directories remain)."
            ),
            Ok(()) => format!(
                "{count} staged copies were removed from {root}; left in place: {}.",
                kept.join(", ")
            ),
            Err(_) => {
                format!("Removing the staged copies from {root} failed part-way; some may remain.")
            }
        }
    }

    /// Sessions of at most [`DOWNLOAD_CHUNK_FILES`] lines and, for `bytes`
    /// per op, at most [`DOWNLOAD_CHUNK_BYTES`] bytes.
    async fn run_chunked(&self, op: &str, ops: Vec<(SftpOp, u64)>) -> Result<(), CoreError> {
        let mut batch: Vec<SftpOp> = Vec::new();
        let mut bytes = 0u64;
        for (line, size) in ops {
            if !batch.is_empty()
                && (batch.len() >= DOWNLOAD_CHUNK_FILES || bytes + size > DOWNLOAD_CHUNK_BYTES)
            {
                self.run_batch(op, std::mem::take(&mut batch)).await?;
                bytes = 0;
            }
            bytes += size;
            batch.push(line);
        }
        self.run_batch(op, batch).await
    }

    async fn stage_inner(&self, tree: &RenderedTree) -> Result<Staged, CoreError> {
        const OP: &str = "stage";
        let setup = self.mirror_setup(OP)?;
        validate_mirror_inputs(
            tree,
            &DeleteSet::default(),
            &setup.staging,
            &self.resources_display(),
        )
        .map_err(|why| self.mirror_fail(OP, why))?;
        let owned = tree.clone();
        let (local, files) = tokio::task::spawn_blocking(move || copy_to_temp(&owned))
            .await
            .map_err(|e| CoreError::Internal(format!("sftp stage task failed: {e}")))?
            .map_err(|why| self.mirror_fail(OP, why))?;

        let remote_root = match &setup.staging {
            StagingMode::InPlace => None,
            StagingMode::Separate { root } => {
                let parts = absolute_components(root).map_err(|why| self.mirror_fail(OP, why))?;
                // `/<first>` is assumed to exist; its listing proves it.
                let base = format!("/{}", parts[0]);
                let rest: Vec<String> = parts[1..].to_vec();
                let mut targets: Vec<Vec<String>> = Vec::new();
                let mut file_chains: Vec<Vec<String>> = Vec::new();
                for (path, _) in &files {
                    let mut chain = rest.clone();
                    chain.extend(path.components().map(str::to_string));
                    file_chains.push(chain.clone());
                    targets.push(chain);
                }
                targets.push(rest.clone());
                let mut budget = Budget::default();
                let seen = self.survey(OP, &base, &targets, &mut budget).await?;
                for chain in &file_chains {
                    if matches!(seen.get(chain), Some(Seen::Dir)) {
                        return Err(self.mirror_fail(
                            OP,
                            format!(
                                "{} exists in the staging root and is a directory",
                                self.shown(&join_abs(&base, chain))
                            ),
                        ));
                    }
                }
                if matches!(seen.get(&rest), Some(Seen::File(_))) {
                    return Err(self.mirror_fail(
                        OP,
                        format!("the staging root {root:?} is a file, not a directory"),
                    ));
                }
                let mut missing = Self::dirs_to_make(&seen, &file_chains);
                // the root itself and its missing ancestors
                for len in 1..=rest.len() {
                    let chain = rest[..len].to_vec();
                    if seen.get(&chain) == Some(&Seen::Missing) {
                        missing.insert(chain);
                    }
                }
                self.make_dirs(OP, &base, &missing).await?;
                let puts: Vec<(SftpOp, u64)> = tree
                    .files
                    .iter()
                    .map(|(path, bytes)| {
                        (
                            SftpOp::Put {
                                local: local.path().join(path.as_str()),
                                remote: format!("{root}/{path}"),
                            },
                            bytes.len() as u64,
                        )
                    })
                    .collect();
                self.run_chunked(OP, puts).await?;
                Some(root.clone())
            }
        };
        Ok(Staged {
            files,
            local,
            remote_root,
            owner: self.mirror_owner(setup),
        })
    }

    async fn commit_inner(
        &self,
        local: &Path,
        files: &[(RelPath, String)],
        remote_root: Option<&str>,
        owner: &str,
        delete_set: &DeleteSet,
    ) -> Result<MirrorReport, CoreError> {
        const OP: &str = "commit";
        let setup = self.mirror_setup(OP)?;
        if owner != self.mirror_owner(setup) {
            return Err(self.mirror_fail(
                OP,
                "these files were staged by a different mirror (another node or mode)",
            ));
        }
        let expected_root = match &setup.staging {
            StagingMode::Separate { root } => Some(root.as_str()),
            StagingMode::InPlace => None,
        };
        if remote_root != expected_root {
            return Err(self.mirror_fail(OP, "these files were staged in a different staging mode"));
        }
        let staged_paths: Vec<&RelPath> = files.iter().map(|(path, _)| path).collect();
        validate_mirror_paths(&staged_paths, delete_set)
            .and_then(|()| validate_staging_root(&setup.staging, &self.resources_display()))
            .map_err(|why| self.mirror_fail(OP, why))?;
        for path in staged_paths.iter().copied().chain(delete_set.files.iter()) {
            safe_remote_path(path.as_str()).map_err(|why| {
                self.mirror_fail(
                    OP,
                    format!("{path} cannot be placed in an sftp batch: {why}"),
                )
            })?;
        }

        // ---- expected state: everything is checked before the first write.
        // The local copies first, through `open_regular_file`.
        let sizes: Vec<u64> = {
            let (base, copies) = (local.to_path_buf(), files.to_vec());
            tokio::task::spawn_blocking(move || -> Result<Vec<u64>, String> {
                copies
                    .iter()
                    .map(|(path, sha)| read_staged_copy(&base, path, sha).map(|b| b.len() as u64))
                    .collect()
            })
            .await
            .map_err(|e| CoreError::Internal(format!("sftp commit task failed: {e}")))?
            .map_err(|why| self.mirror_fail(OP, why))?
        };

        let mode_chain = chain_of(&[RESOURCES[0], RESOURCES[1], setup.mode.as_str()]);
        let in_collection = |path: &RelPath| -> Vec<String> {
            let mut chain = mode_chain.clone();
            chain.extend(path.components().map(str::to_string));
            chain
        };
        let mut deletes: Vec<&RelPath> = delete_set.files.iter().collect();
        deletes.sort();
        let mut targets: Vec<Vec<String>> = vec![
            chain_of(&[RESOURCES[0]]),
            chain_of(&[RESOURCES[0], RESOURCES[1]]),
            mode_chain.clone(),
        ];
        let staged_chains: Vec<Vec<String>> = files.iter().map(|(p, _)| in_collection(p)).collect();
        let delete_chains: Vec<Vec<String>> = deletes.iter().map(|p| in_collection(p)).collect();
        targets.extend(staged_chains.iter().cloned());
        targets.extend(delete_chains.iter().cloned());
        let mut budget = Budget::default();
        let seen = self
            .survey(OP, &self.data_dir, &targets, &mut budget)
            .await?;
        for required in [
            chain_of(&[RESOURCES[0]]),
            chain_of(&[RESOURCES[0], RESOURCES[1]]),
        ] {
            if seen.get(&required) != Some(&Seen::Dir) {
                return Err(self.mirror_fail(
                    OP,
                    format!(
                        "{} is not a directory; is data_dir a gateway data directory?",
                        required.join("/")
                    ),
                ));
            }
        }
        if let Some(Seen::File(_)) = seen.get(&mode_chain) {
            return Err(self.mirror_fail(
                OP,
                format!("the {} collection is not a directory", setup.mode),
            ));
        }
        for (path, chain) in deletes.iter().zip(&delete_chains) {
            if !matches!(seen.get(chain), Some(Seen::File(_))) {
                return Err(self.mirror_fail(
                    OP,
                    format!(
                        "{path} is not a regular file in the {} collection; nothing was changed",
                        setup.mode
                    ),
                ));
            }
        }
        for ((path, _), chain) in files.iter().zip(&staged_chains) {
            if matches!(seen.get(chain), Some(Seen::Dir)) {
                return Err(self.mirror_fail(
                    OP,
                    format!(
                        "{path} is a directory in the {} collection; nothing was changed",
                        setup.mode
                    ),
                ));
            }
        }
        if let Some(root) = remote_root {
            let parts = absolute_components(root).map_err(|why| self.mirror_fail(OP, why))?;
            let base = format!("/{}", parts[0]);
            let rest: Vec<String> = parts[1..].to_vec();
            let chains: Vec<Vec<String>> = files
                .iter()
                .map(|(path, _)| {
                    let mut chain = rest.clone();
                    chain.extend(path.components().map(str::to_string));
                    chain
                })
                .collect();
            let mut staging_budget = Budget::default();
            let staging_seen = self.survey(OP, &base, &chains, &mut staging_budget).await?;
            for (((path, _), chain), size) in files.iter().zip(&chains).zip(&sizes) {
                match staging_seen.get(chain) {
                    Some(Seen::File(found)) if found == size => {}
                    other => {
                        return Err(self.mirror_fail(
                            OP,
                            format!(
                                "the staged copy of {path} on the host is missing or the wrong size ({other:?}, expected {size} bytes); nothing was changed"
                            ),
                        ));
                    }
                }
            }
            // The size survey cannot tell same-size different bytes apart: read
            // every staged copy back and hash it here before the first rename.
            let copies: Vec<RemoteCopy> = files
                .iter()
                .zip(&sizes)
                .map(|((path, sha), size)| RemoteCopy {
                    rel: path.clone(),
                    remote: format!("{root}/{path}"),
                    sha: sha.clone(),
                    size: *size,
                })
                .collect();
            let bad = self.verify_remote_copies(OP, &copies).await?;
            if !bad.is_empty() {
                let cleanup = self.discard_staged(OP, root, files).await;
                return Err(self.mirror_fail(
                    OP,
                    format!(
                        "the staged copy on the host does not match its SHA-256 for {}; nothing was changed. {cleanup}",
                        named(&bad)
                    ),
                ));
            }
        }

        // ---- writes
        let mut missing = Self::dirs_to_make(&seen, &staged_chains);
        if seen.get(&mode_chain) == Some(&Seen::Missing) {
            missing.insert(mode_chain.clone());
        }
        // The missing directories lead the SAME batch as the renames (a sorted
        // set lists a parent before its children), so nothing but the batch
        // itself sits between the pre-rename verification above and the moves.
        let mut moves: Vec<(SftpOp, u64)> = Vec::with_capacity(missing.len() + files.len());
        for chain in &missing {
            moves.push((
                SftpOp::Mkdir {
                    path: join_abs(&self.data_dir, chain),
                },
                0,
            ));
        }
        for (((path, _), chain), size) in files.iter().zip(&staged_chains).zip(&sizes) {
            let target = join_abs(&self.data_dir, chain);
            let op = match remote_root {
                Some(root) => SftpOp::Rename {
                    from: format!("{root}/{path}"),
                    to: target,
                },
                None => SftpOp::Put {
                    local: local.join(path.as_str()),
                    remote: target,
                },
            };
            moves.push((op, *size));
        }
        self.run_chunked(OP, moves).await?;

        // ---- post-commit verification, before any delete. sftp has no atomic
        // verify-then-rename, so a host-side writer can still change a staged
        // copy between the read-back above and the rename (and `InPlace` has no
        // pre-write check at all). Read the FINAL paths back and hash them: a
        // mismatch is detected, reported, and stops before the first delete.
        let copies: Vec<RemoteCopy> = files
            .iter()
            .zip(&staged_chains)
            .zip(&sizes)
            .map(|(((path, sha), chain), size)| RemoteCopy {
                rel: path.clone(),
                remote: join_abs(&self.data_dir, chain),
                sha: sha.clone(),
                size: *size,
            })
            .collect();
        let bad = self.verify_remote_copies(OP, &copies).await?;
        if !bad.is_empty() {
            return Err(self.mirror_fail(
                OP,
                format!(
                    "content changed on the host during commit: the {} collection holds content that does not match the staged SHA-256 for {}; the {} written files were not rolled back and no delete was run",
                    setup.mode,
                    named(&bad),
                    files.len()
                ),
            ));
        }
        let written: Vec<String> = files.iter().map(|(path, _)| path.to_string()).collect();

        // ---- deletes
        let rms: Vec<(SftpOp, u64)> = delete_chains
            .iter()
            .map(|chain| {
                (
                    SftpOp::Rm {
                        path: join_abs(&self.data_dir, chain),
                    },
                    0,
                )
            })
            .collect();
        self.run_chunked(OP, rms).await?;
        let deleted: Vec<String> = deletes.iter().map(|path| path.to_string()).collect();

        // ---- directories a delete emptied, never `<module>/<type>` or above
        let removed = self
            .remove_emptied(OP, &deletes, &mode_chain, &mut budget)
            .await?;
        Ok(MirrorReport {
            written,
            deleted,
            removed_dirs: removed,
        })
    }

    /// Remove the directories above `deletes` that are now empty, deepest
    /// first, listing each candidate level in one session. A directory that is
    /// not empty keeps its ancestors. Stops before `<module>/<type>`.
    async fn remove_emptied(
        &self,
        op: &str,
        deletes: &[&RelPath],
        mode_chain: &[String],
        budget: &mut Budget,
    ) -> Result<Vec<String>, CoreError> {
        let mut pending: BTreeMap<usize, BTreeSet<Vec<String>>> = BTreeMap::new();
        for path in deletes {
            let parts: Vec<&str> = path.components().collect();
            for depth in MIRROR_MIN_COMPONENTS..parts.len() {
                pending
                    .entry(depth)
                    .or_default()
                    .insert(chain_of(&parts[..depth]));
            }
        }
        let mut blocked: BTreeSet<Vec<String>> = BTreeSet::new();
        let mut removed: Vec<String> = Vec::new();
        let depths: Vec<usize> = pending.keys().rev().copied().collect();
        for depth in depths {
            let candidates: Vec<Vec<String>> = pending[&depth]
                .iter()
                .filter(|chain| !blocked.contains(*chain))
                .cloned()
                .collect();
            let dirs: Vec<String> = candidates
                .iter()
                .map(|chain| join_abs(&join_abs(&self.data_dir, mode_chain), chain))
                .collect();
            let listings = self.list_level_as(op, &dirs, budget).await?;
            let mut empty: Vec<SftpOp> = Vec::new();
            for ((chain, dir), listing) in candidates.iter().zip(&dirs).zip(&listings) {
                if listing.is_empty() {
                    empty.push(SftpOp::Rmdir { path: dir.clone() });
                    removed.push(chain.join("/"));
                } else {
                    for len in MIRROR_MIN_COMPONENTS..chain.len() {
                        blocked.insert(chain[..len].to_vec());
                    }
                }
            }
            self.run_batch(op, empty).await?;
        }
        removed.sort();
        Ok(removed)
    }
}

#[async_trait::async_trait]
impl TreeMirror for SftpTransport {
    async fn stage(&self, tree: &RenderedTree) -> Result<Staged, CoreError> {
        self.stage_inner(tree).await
    }

    async fn commit(
        &self,
        staged: &Staged,
        delete_set: &DeleteSet,
    ) -> Result<MirrorReport, CoreError> {
        self.commit_inner(
            staged.local.path(),
            &staged.files,
            staged.remote_root.as_deref(),
            &staged.owner,
            delete_set,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deploy::ledger::LEDGER_FILE;
    use crate::deploy::transport::process::{ProcessOutput, ProcessRunner};
    use std::collections::HashSet;
    use std::ffi::{OsStr, OsString};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    const DATA: &str = "/gw/data";
    const RES: &str = "/gw/data/config/resources";

    // ---- parse_ls_listing / posix_root ------------------------------------

    fn line(mode: &str, size: u64, path: &str) -> String {
        format!("{mode}    ? svc staff {size:>10} Oct 14 10:21 {path}")
    }

    #[test]
    fn parse_ls_listing_cases() {
        let dir = "/d/a b,c";
        let out = [
            line("drwxr-xr-x", 96, "/d/a b,c/."),
            line("drwxr-xr-x", 96, "/d/a b,c/.."),
            line("-rw-r--r--", 12, "/d/a b,c/zeta file"),
            line("drwxr-xr-x", 64, "/d/a b,c/alpha"),
            line("lrwxr-xr-x", 5, "/d/a b,c/lnk"),
            line("crw-r--r--", 0, "/d/a b,c/dev"),
            line("-rw-r--r--", 7, "/d/a b,c/.hidden"),
        ]
        .join("\n");
        let got = parse_ls_listing(dir, out.as_bytes()).expect("listing");
        let names: Vec<(&str, EntryKind, u64)> = got
            .iter()
            .map(|e| (e.name.as_str(), e.kind, e.size))
            .collect();
        assert_eq!(
            names,
            [
                (".hidden", EntryKind::File, 7),
                ("alpha", EntryKind::Dir, 64),
                ("dev", EntryKind::Other, 0),
                ("lnk", EntryKind::Symlink, 5),
                ("zeta file", EntryKind::File, 12),
            ],
            "byte-sorted, dot entries dropped, kinds from the mode column"
        );

        let bad = [
            // no prefix of a listed directory
            line("-rw-r--r--", 1, "/other/x"),
            // a name holding a slash is two components
            line("-rw-r--r--", 1, "/d/a b,c/x/y"),
            // empty name
            line("-rw-r--r--", 1, "/d/a b,c/"),
            // unsafe names
            line("-rw-r--r--", 1, "/d/a b,c/odd[1]"),
            line("-rw-r--r--", 1, "/d/a b,c/odd\"q"),
            line("-rw-r--r--", 1, "/d/a b,c/back\\slash"),
            // too few columns, bad mode, bad size
            "/d/a b,c/x".to_string(),
            line("-rw", 1, "/d/a b,c/x"),
            "-rw-r--r--    ? svc staff NaN Oct 14 10:21 /d/a b,c/x".to_string(),
            // an empty line
            "\n".to_string(),
        ];
        for text in bad {
            assert!(
                parse_ls_listing(dir, text.as_bytes()).is_err(),
                "{text:?} must be refused"
            );
        }
        // the same name twice
        let twice = [
            line("-rw-r--r--", 1, "/d/a b,c/x"),
            line("-rw-r--r--", 2, "/d/a b,c/x"),
        ]
        .join("\n");
        assert!(parse_ls_listing(dir, twice.as_bytes()).is_err());
        // not UTF-8
        assert!(parse_ls_listing(dir, b"\xff\xfe").is_err());
        // an empty output is an empty directory
        assert!(parse_ls_listing(dir, b"").expect("empty").is_empty());
    }

    #[test]
    fn parse_batch_listing_attributes_lines_in_order() {
        let dirs = vec!["/d/a".to_string(), "/d/a/b".to_string(), "/d/c".to_string()];
        let out = [
            line("drwxr-xr-x", 1, "/d/a/."),
            line("drwxr-xr-x", 1, "/d/a/b"),
            line("-rw-r--r--", 3, "/d/a/f"),
            line("drwxr-xr-x", 1, "/d/a/b/."),
            line("-rw-r--r--", 4, "/d/a/b/x"),
            line("-rw-r--r--", 5, "/d/c/y"),
        ]
        .join("\n");
        let got = parse_batch_listing(&dirs, out.as_bytes()).expect("batch");
        let names = |i: usize| -> Vec<&str> { got[i].iter().map(|e| e.name.as_str()).collect() };
        assert_eq!(names(0), ["b", "f"]);
        assert_eq!(names(1), ["x"]);
        assert_eq!(names(2), ["y"]);
        // going back to an earlier directory is not a valid order
        let backwards = [
            line("-rw-r--r--", 5, "/d/c/y"),
            line("-rw-r--r--", 3, "/d/a/f"),
        ]
        .join("\n");
        assert!(parse_batch_listing(&dirs, backwards.as_bytes()).is_err());
    }

    #[test]
    fn posix_root_cases() {
        assert_eq!(
            posix_root("/usr/local/bin/ignition/data"),
            Ok("/usr/local/bin/ignition/data".to_string())
        );
        assert_eq!(
            posix_root("/opt/ign data/"),
            Ok("/opt/ign data".to_string())
        );
        for bad in [
            "",
            "relative/dir",
            "C:\\Program Files\\Inductive",
            "/",
            "///",
            "/a//b",
            "/a/./b",
            "/a/../b",
            "/a/b*",
            "/a/[b]",
            "/a/{b}",
            "/a/b?",
            "/a/\"b",
            "/a/b\\c",
            "/a/b\nc",
        ] {
            assert!(posix_root(bad).is_err(), "{bad:?} must be refused");
        }
    }

    // ---- a scripted sftp server --------------------------------------------

    #[derive(Clone)]
    enum Node {
        Dir,
        File(Vec<u8>),
        Symlink,
        Special,
    }

    #[derive(Default)]
    struct Knobs {
        timeout: bool,
        stderr_noise: bool,
        fail_get: bool,
        /// Do not materialise a file whose path ends with this.
        skip_download: Option<String>,
        /// Write this extra file into every downloaded directory.
        extra_download: Option<String>,
        /// Report a different size for the path.
        lie_size: Option<(String, u64)>,
        /// Remove this path from the tree once the first session ended.
        vanish_after_first_session: Option<String>,
        /// A `get` of a path ending with `.0` writes a sparse file of `.1`
        /// bytes regardless of the listing.
        overshoot: Option<(String, u64)>,
    }

    struct FakeSftp {
        fs: Mutex<std::collections::BTreeMap<String, Node>>,
        listed: Mutex<HashSet<String>>,
        sessions: Mutex<Vec<Vec<String>>>,
        knobs: Mutex<Knobs>,
        /// Local files the server wrote.
        downloads: Mutex<Vec<PathBuf>>,
    }

    impl FakeSftp {
        fn new() -> Arc<FakeSftp> {
            let fake = FakeSftp {
                fs: Mutex::new(std::collections::BTreeMap::new()),
                listed: Mutex::new(HashSet::from([DATA.to_string()])),
                sessions: Mutex::new(Vec::new()),
                knobs: Mutex::new(Knobs::default()),
                downloads: Mutex::new(Vec::new()),
            };
            fake.mkdirs(DATA);
            Arc::new(fake)
        }

        fn mkdirs(&self, path: &str) {
            let mut fs = self.fs.lock().expect("fs");
            let mut cur = String::new();
            for part in path.split('/').skip(1) {
                cur.push('/');
                cur.push_str(part);
                fs.entry(cur.clone()).or_insert(Node::Dir);
            }
        }

        fn file(&self, path: &str, bytes: &str) {
            self.mkdirs(path.rsplit_once('/').expect("parent").0);
            self.fs
                .lock()
                .expect("fs")
                .insert(path.to_string(), Node::File(bytes.as_bytes().to_vec()));
        }

        fn put(&self, path: &str, node: Node) {
            self.mkdirs(path.rsplit_once('/').expect("parent").0);
            self.fs.lock().expect("fs").insert(path.to_string(), node);
        }

        fn knobs(&self) -> std::sync::MutexGuard<'_, Knobs> {
            self.knobs.lock().expect("knobs")
        }

        fn session_lines(&self) -> Vec<Vec<String>> {
            self.sessions.lock().expect("sessions").clone()
        }

        fn children(&self, dir: &str) -> Vec<(String, Node)> {
            let prefix = format!("{dir}/");
            self.fs
                .lock()
                .expect("fs")
                .iter()
                .filter_map(|(path, node)| {
                    let rest = path.strip_prefix(&prefix)?;
                    (!rest.contains('/')).then(|| (rest.to_string(), node.clone()))
                })
                .collect()
        }
    }

    fn quoted(text: &str) -> Vec<String> {
        // the arguments of an sftp line: each is wrapped in double quotes
        text.split('"')
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, s)| s.to_string())
            .collect()
    }

    #[async_trait::async_trait]
    impl ProcessRunner for FakeSftp {
        async fn run(
            &self,
            program: &OsStr,
            _args: &[OsString],
            stdin: Option<Vec<u8>>,
            _timeout: Duration,
        ) -> ProcessOutput {
            assert_eq!(program, "sftp");
            let batch = String::from_utf8(stdin.expect("batch on stdin")).expect("utf8");
            let lines: Vec<String> = batch.lines().map(str::to_string).collect();
            self.sessions.lock().expect("sessions").push(lines.clone());
            if self.knobs().timeout {
                return ProcessOutput {
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    code: -1,
                    timed_out: true,
                };
            }
            let mut stdout = String::new();
            let mut stderr = String::new();
            let mut code = 0;
            let mut newly_listed: Vec<String> = Vec::new();
            for l in &lines {
                assert!(!l.starts_with('-'), "an error-suppressing line: {l}");
                let args = quoted(l);
                let remote = args.first().expect("a path").clone();
                assert!(
                    self.listed.lock().expect("listed").contains(&remote),
                    "{remote} is referenced before an earlier session listed it"
                );
                if l.starts_with("@ls -la \"") {
                    let node = self.fs.lock().expect("fs").get(&remote).cloned();
                    if !matches!(node, Some(Node::Dir)) {
                        stderr.push_str(&format!("Can't ls: \"{remote}\" not found\n"));
                        code = 1;
                        break;
                    }
                    stdout.push_str(&format!(
                        "{}\n",
                        line("drwxr-xr-x", 96, &format!("{remote}/."))
                    ));
                    stdout.push_str(&format!(
                        "{}\n",
                        line("drwxr-xr-x", 96, &format!("{remote}/.."))
                    ));
                    for (name, node) in self.children(&remote) {
                        let child = format!("{remote}/{name}");
                        let (mode, mut size) = match &node {
                            Node::Dir => ("drwxr-xr-x", 64),
                            Node::File(b) => ("-rw-r--r--", b.len() as u64),
                            Node::Symlink => ("lrwxr-xr-x", 5),
                            Node::Special => ("crw-r--r--", 0),
                        };
                        if let Some((path, lie)) = &self.knobs().lie_size
                            && child.ends_with(path.as_str())
                        {
                            size = *lie;
                        }
                        stdout.push_str(&format!("{}\n", line(mode, size, &child)));
                        newly_listed.push(child);
                    }
                } else if l.starts_with("get \"") {
                    if self.knobs().fail_get {
                        stderr.push_str(&format!("remote open(\"{remote}\"): No such file\n"));
                        code = 1;
                        break;
                    }
                    let node = self.fs.lock().expect("fs").get(&remote).cloned();
                    let Some(Node::File(bytes)) = node else {
                        stderr.push_str(&format!("File \"{remote}\" not found.\n"));
                        code = 1;
                        break;
                    };
                    let skip = self.knobs().skip_download.clone();
                    if skip.as_deref().is_some_and(|s| remote.ends_with(s)) {
                        continue;
                    }
                    let dst = PathBuf::from(&args[1]);
                    std::fs::write(&dst, bytes).expect("write");
                    self.downloads.lock().expect("downloads").push(dst.clone());
                    let extra = self.knobs().extra_download.clone();
                    if let Some(extra) = extra {
                        std::fs::write(dst.parent().expect("parent").join(extra), b"surprise")
                            .expect("extra");
                    }
                    let overshoot = self.knobs().overshoot.clone();
                    if let Some((path, len)) = overshoot
                        && remote.ends_with(&path)
                    {
                        // a server that sends far more than it listed, then stalls
                        let file = std::fs::File::create(&dst).expect("create");
                        file.set_len(len).expect("sparse");
                        drop(file);
                        tokio::time::sleep(Duration::from_secs(30)).await;
                    }
                } else {
                    panic!("unexpected batch line {l}");
                }
            }
            self.listed.lock().expect("listed").extend(newly_listed);
            if self.knobs().stderr_noise {
                stderr.push_str("warning: something odd\n");
            }
            let vanish = self.knobs().vanish_after_first_session.take();
            if let Some(path) = vanish {
                self.fs
                    .lock()
                    .expect("fs")
                    .retain(|k, _| k != &path && !k.starts_with(&format!("{path}/")));
            }
            ProcessOutput {
                stdout: stdout.into_bytes(),
                stderr: stderr.into_bytes(),
                code,
                timed_out: false,
            }
        }
    }

    fn transport(fake: &Arc<FakeSftp>) -> SftpTransport {
        transport_at(fake, DATA)
    }

    fn transport_at(fake: &Arc<FakeSftp>, data_dir: &str) -> SftpTransport {
        let shell = RemoteShell::local_sftp_server(
            Path::new("/unused/sftp-server"),
            fake.clone(),
            Duration::from_secs(30),
        );
        SftpTransport::new(shell, data_dir.to_string(), "nodes.master")
    }

    fn scope() -> Scope {
        Scope {
            mode: "production".to_string(),
            types: vec![("ignition".to_string(), "database-connection".to_string())],
            tag_providers: Vec::new(),
        }
    }

    fn standard_tree(fake: &FakeSftp) {
        fake.file(
            &format!("{RES}/core/ignition/database-connection/CoreDb/config.json"),
            "{\"core\":1}",
        );
        fake.file(
            &format!("{RES}/production/ignition/database-connection/MES db,1/config.json"),
            "{\"mes\":1}",
        );
        fake.file(
            &format!("{RES}/production/ignition/database-connection/MES db,1/resource.json"),
            "{\"scope\":\"A\"}",
        );
        fake.file(
            &format!("{RES}/production/ignition/database-connection/MES db,1/.dot"),
            "d",
        );
        fake.file(
            &format!("{RES}/local/ignition/database-connection/Override/config.json"),
            "{}",
        );
        // outside the scope: never inspected
        fake.put(
            &format!("{RES}/production/ignition/other-type"),
            Node::Symlink,
        );
    }

    fn rel(text: &str) -> RelPath {
        RelPath::new(text).expect("rel path")
    }

    fn detail(err: CoreError) -> String {
        match err {
            CoreError::TransportFailed {
                operation,
                node,
                detail,
            } => {
                // the transport's own failures say `read`; a batch the shell
                // refuses to render says `sftp`
                assert!(operation == "read" || operation == "sftp", "{operation}");
                assert!(node.starts_with("nodes."), "{node}");
                detail
            }
            other => panic!("expected transport_failed, got {other:?}"),
        }
    }

    // ---- read ----------------------------------------------------------------

    #[tokio::test]
    async fn read_collects_core_mode_and_local_units() {
        let fake = FakeSftp::new();
        standard_tree(&fake);
        let t = transport(&fake);
        let observed = t.read(&scope()).await.expect("read");

        let core: Vec<&str> = observed.core.keys().map(RelPath::as_str).collect();
        assert_eq!(core, ["ignition/database-connection/CoreDb/config.json"]);
        let mode: Vec<&str> = observed.mode.keys().map(RelPath::as_str).collect();
        assert_eq!(
            mode,
            [
                "ignition/database-connection/MES db,1/.dot",
                "ignition/database-connection/MES db,1/config.json",
                "ignition/database-connection/MES db,1/resource.json",
            ]
        );
        let blob = &observed.mode[&rel("ignition/database-connection/MES db,1/config.json")];
        assert_eq!(blob.bytes, b"{\"mes\":1}");
        assert_eq!(blob.sha256, sha256_hex(b"{\"mes\":1}"));
        assert_eq!(
            observed.local_units,
            ["ignition/database-connection/Override"]
        );
        // data_dir, config, resources, collections, modules, types, the type
        // contents, the unit contents, then one download
        assert_eq!(t.sessions(), 8);
    }

    #[tokio::test]
    async fn batches_never_suppress_errors() {
        let fake = FakeSftp::new();
        standard_tree(&fake);
        transport(&fake).read(&scope()).await.expect("read");
        let lines: Vec<String> = fake.session_lines().into_iter().flatten().collect();
        assert!(!lines.is_empty());
        for l in &lines {
            assert!(
                l.starts_with("@ls -la \"") || l.starts_with("get \""),
                "only listings and single-file gets, never `get -R`: {l}"
            );
            assert!(!l.starts_with('-'), "{l}");
        }
        // the fake itself asserts that no batch line was sent before an earlier
        // session listed its path (batches_only_reference_listed_paths)
    }

    #[tokio::test]
    async fn batches_only_reference_listed_paths() {
        // The fake panics when a batch names a path no earlier session listed.
        // A scope with tag providers exercises the longest descent.
        let fake = FakeSftp::new();
        standard_tree(&fake);
        fake.file(
            &format!("{RES}/production/ignition/tag-definition/default/Folder/tags.json"),
            "{}",
        );
        fake.file(
            &format!("{RES}/core/ignition/tag-definition/default/tags.json"),
            "{}",
        );
        let mut s = scope();
        s.tag_providers = vec!["default".to_string(), "absent".to_string()];
        let observed = transport(&fake).read(&s).await.expect("read");
        assert!(
            observed
                .mode
                .contains_key(&rel("ignition/tag-definition/default/Folder/tags.json"))
        );
        assert!(
            observed
                .core
                .contains_key(&rel("ignition/tag-definition/default/tags.json"))
        );
        // the absent provider and the absent tag types are simply not read
        let everything: String = fake.session_lines().into_iter().flatten().collect();
        assert!(!everything.contains("absent"), "{everything}");
    }

    #[tokio::test]
    async fn absent_collection_and_type_read_as_empty() {
        let fake = FakeSftp::new();
        fake.mkdirs(RES);
        let observed = transport(&fake).read(&scope()).await.expect("read");
        assert!(observed.core.is_empty() && observed.mode.is_empty());
        assert!(observed.local_units.is_empty());

        // a mode collection that exists without the scoped type
        fake.mkdirs(&format!("{RES}/production/ignition"));
        let observed = transport(&fake).read(&scope()).await.expect("read");
        assert!(observed.mode.is_empty());
    }

    #[tokio::test]
    async fn fake_timeout_is_transport_failed() {
        let fake = FakeSftp::new();
        standard_tree(&fake);
        fake.knobs().timeout = true;
        let why = detail(transport(&fake).read(&scope()).await.expect_err("timeout"));
        assert!(why.contains("timed out"), "{why}");
        assert_eq!(fake.session_lines().len(), 1, "nothing ran after a timeout");
    }

    #[tokio::test]
    async fn fake_stderr_is_transport_failed() {
        let fake = FakeSftp::new();
        standard_tree(&fake);
        fake.knobs().stderr_noise = true;
        let why = detail(transport(&fake).read(&scope()).await.expect_err("stderr"));
        assert!(why.contains("unexpected stderr output"), "{why}");
    }

    #[tokio::test]
    async fn missing_data_dir_is_transport_failed_not_empty() {
        let fake = FakeSftp::new();
        let t = transport_at(&fake, "/gw/nope");
        // the fake panics on an unlisted path, so give it the path as listed
        fake.listed
            .lock()
            .expect("listed")
            .insert("/gw/nope".to_string());
        let why = detail(t.read(&scope()).await.expect_err("missing data dir"));
        assert!(why.contains("exit code 1"), "{why}");
    }

    #[tokio::test]
    async fn vanished_dir_is_transport_failed() {
        // the type directory is listed, then it disappears before the next
        // session touches it
        let fake = FakeSftp::new();
        standard_tree(&fake);
        let target = format!("{RES}/production/ignition/database-connection");
        // sessions: 1 data, 2 config, 3 resources, 4 collections, 5 modules,
        // 6 types. The type dir vanishes after session 6, before session 7
        // lists its contents.
        // script the vanish: remove the dir once the type level was listed
        struct Vanisher {
            inner: Arc<FakeSftp>,
            target: String,
            after: usize,
        }
        #[async_trait::async_trait]
        impl ProcessRunner for Vanisher {
            async fn run(
                &self,
                program: &OsStr,
                args: &[OsString],
                stdin: Option<Vec<u8>>,
                timeout: Duration,
            ) -> ProcessOutput {
                let out = self.inner.run(program, args, stdin, timeout).await;
                if self.inner.session_lines().len() == self.after {
                    let t = &self.target;
                    self.inner
                        .fs
                        .lock()
                        .expect("fs")
                        .retain(|k, _| k != t && !k.starts_with(&format!("{t}/")));
                }
                out
            }
        }
        let shell = RemoteShell::local_sftp_server(
            Path::new("/unused"),
            Arc::new(Vanisher {
                inner: fake.clone(),
                target,
                after: 6,
            }),
            Duration::from_secs(30),
        );
        let t = SftpTransport::new(shell, DATA.to_string(), "nodes.master");
        let why = detail(t.read(&scope()).await.expect_err("vanished"));
        assert!(why.contains("exit code 1"), "{why}");
        assert!(why.contains("not found"), "{why}");
    }

    #[tokio::test]
    async fn symlinks_and_special_files_are_refused_not_skipped() {
        for node in [Node::Symlink, Node::Special] {
            let fake = FakeSftp::new();
            standard_tree(&fake);
            fake.put(
                &format!("{RES}/production/ignition/database-connection/MES db,1/lnk"),
                node.clone(),
            );
            let why = detail(transport(&fake).read(&scope()).await.expect_err("refused"));
            assert!(
                why.contains(
                    "config/resources/production/ignition/database-connection/MES db,1/lnk"
                ),
                "{why}"
            );
            if matches!(node, Node::Symlink) {
                assert!(why.contains("is a symlink; symlinks are refused"), "{why}");
            } else {
                assert!(why.contains("is not a regular file or directory"), "{why}");
            }
            // refused from the listing, before any download
            let last = fake.session_lines().pop().expect("session");
            assert!(last[0].starts_with("@ls -la"), "{last:?}");
        }
    }

    #[tokio::test]
    async fn symlinked_scope_path_components_are_refused() {
        // a symlink where config, resources, a collection, a module or a type
        // should be a directory
        for path in [
            "config",
            "config/resources",
            "config/resources/production",
            "config/resources/production/ignition",
            "config/resources/production/ignition/database-connection",
        ] {
            let fake = FakeSftp::new();
            standard_tree(&fake);
            let full = format!("{DATA}/{path}");
            fake.fs
                .lock()
                .expect("fs")
                .retain(|k, _| k != &full && !k.starts_with(&format!("{full}/")));
            fake.put(&full, Node::Symlink);
            let why = detail(transport(&fake).read(&scope()).await.expect_err(path));
            assert!(why.contains(path), "{path}: {why}");
            assert!(why.contains("symlink"), "{path}: {why}");
        }
        // a file where a directory belongs
        let fake = FakeSftp::new();
        standard_tree(&fake);
        let full = format!("{RES}/production/ignition");
        fake.fs
            .lock()
            .expect("fs")
            .retain(|k, _| k != &full && !k.starts_with(&format!("{full}/")));
        fake.file(&full, "x");
        let why = detail(transport(&fake).read(&scope()).await.expect_err("file"));
        assert!(why.contains("is not a directory"), "{why}");
    }

    #[tokio::test]
    async fn unsafe_names_are_refused() {
        for name in ["odd[1]", "a\\b", "q\"uote", "g*lob"] {
            let fake = FakeSftp::new();
            standard_tree(&fake);
            fake.file(
                &format!("{RES}/production/ignition/database-connection/{name}/config.json"),
                "{}",
            );
            let why = detail(transport(&fake).read(&scope()).await.expect_err(name));
            assert!(why.contains("unsafe"), "{name}: {why}");
        }
    }

    #[tokio::test]
    async fn caps_are_enforced_from_the_listing_before_any_download() {
        let fake = FakeSftp::new();
        standard_tree(&fake);
        fake.knobs().lie_size = Some(("MES db,1/config.json".to_string(), READ_MAX_FILE_BYTES + 1));
        let why = detail(transport(&fake).read(&scope()).await.expect_err("file cap"));
        assert!(why.contains("byte file cap"), "{why}");
        let last = fake.session_lines().pop().expect("session");
        assert!(
            !last[0].starts_with("get"),
            "no download happened: {last:?}"
        );

        // the total cap: 17 files that each fit the file cap (64 MiB), together
        // over 1 GiB
        let fake = FakeSftp::new();
        for n in 0..17 {
            fake.file(
                &format!("{RES}/production/ignition/database-connection/D{n:02}/config.json"),
                "{}",
            );
        }
        fake.knobs().lie_size = Some(("config.json".to_string(), READ_MAX_FILE_BYTES));
        let why = detail(
            transport(&fake)
                .read(&scope())
                .await
                .expect_err("total cap"),
        );
        assert!(why.contains("in total"), "{why}");
        let last = fake.session_lines().pop().expect("session");
        assert!(
            !last[0].starts_with("get"),
            "no download happened: {last:?}"
        );
    }

    #[tokio::test]
    async fn depth_is_bounded() {
        let fake = FakeSftp::new();
        standard_tree(&fake);
        let mut deep = format!("{RES}/production/ignition/database-connection/MES db,1");
        for _ in 0..READ_MAX_DEPTH {
            deep.push_str("/d");
        }
        fake.file(&format!("{deep}/f"), "x");
        let why = detail(transport(&fake).read(&scope()).await.expect_err("deep"));
        assert!(why.contains("deeper than"), "{why}");
    }

    #[tokio::test]
    async fn download_that_disagrees_with_the_listing_is_transport_failed() {
        // a listed type directory is missing locally after the download
        let fake = FakeSftp::new();
        standard_tree(&fake);
        let t = transport(&fake);
        // a file that listed but did not come down
        fake.knobs().skip_download = Some("MES db,1/resource.json".to_string());
        let why = detail(t.read(&scope()).await.expect_err("missing file"));
        assert!(why.contains("was listed but not downloaded"), "{why}");
        fake.knobs().skip_download = None;

        // a file that came down but was never listed
        fake.knobs().extra_download = Some("surprise.json".to_string());
        let why = detail(t.read(&scope()).await.expect_err("extra file"));
        assert!(why.contains("appeared in the download"), "{why}");
        fake.knobs().extra_download = None;

        // a file whose size differs from its listing
        fake.knobs().lie_size = Some(("MES db,1/config.json".to_string(), 3));
        let why = detail(t.read(&scope()).await.expect_err("size"));
        assert!(why.contains("was listed as 3"), "{why}");
    }

    #[tokio::test]
    async fn downloaded_files_missing_locally_are_transport_failed() {
        // the sftp client exits 0 but wrote nothing for the listed files
        struct Silent(Arc<FakeSftp>);
        #[async_trait::async_trait]
        impl ProcessRunner for Silent {
            async fn run(
                &self,
                program: &OsStr,
                args: &[OsString],
                stdin: Option<Vec<u8>>,
                timeout: Duration,
            ) -> ProcessOutput {
                let batch =
                    String::from_utf8_lossy(stdin.as_deref().unwrap_or_default()).to_string();
                if batch.contains("get \"") {
                    return ProcessOutput {
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                        code: 0,
                        timed_out: false,
                    };
                }
                self.0.run(program, args, stdin, timeout).await
            }
        }
        let fake = FakeSftp::new();
        standard_tree(&fake);
        let shell = RemoteShell::local_sftp_server(
            Path::new("/unused"),
            Arc::new(Silent(fake.clone())),
            Duration::from_secs(30),
        );
        let t = SftpTransport::new(shell, DATA.to_string(), "nodes.master");
        let why = detail(t.read(&scope()).await.expect_err("missing files"));
        assert!(why.contains("was listed but not downloaded"), "{why}");
    }

    #[tokio::test]
    async fn get_failure_is_transport_failed() {
        let fake = FakeSftp::new();
        standard_tree(&fake);
        fake.knobs().fail_get = true;
        let why = detail(transport(&fake).read(&scope()).await.expect_err("get"));
        assert!(why.contains("exit code 1"), "{why}");
    }

    #[tokio::test]
    async fn hostile_scope_values_are_refused_before_any_session() {
        let fake = FakeSftp::new();
        standard_tree(&fake);
        let t = transport(&fake);
        let mut bad_type = scope();
        bad_type.types = vec![("ignition".to_string(), "x\"; rm".to_string())];
        let mut bad_module = scope();
        bad_module.types = vec![("../etc".to_string(), "t".to_string())];
        let mut bad_provider = scope();
        bad_provider.tag_providers = vec!["p[1]".to_string()];
        let mut bad_mode = scope();
        bad_mode.mode = "prod/../x".to_string();
        for s in [bad_type, bad_module, bad_provider, bad_mode] {
            detail(t.read(&s).await.expect_err("refused"));
        }
        assert!(fake.session_lines().is_empty(), "no process was spawned");
    }

    #[tokio::test]
    async fn glob_data_dir_is_refused_before_any_session() {
        let fake = FakeSftp::new();
        let t = transport_at(&fake, "/gw/da[ta]");
        detail(t.read(&scope()).await.expect_err("glob"));
        detail(t.list_types("core").await.expect_err("glob"));
        let ledger = t.read_ledger("gw", "production").await;
        assert!(ledger.is_err());
        assert!(fake.session_lines().is_empty(), "no process was spawned");
    }

    // ---- list_types ----------------------------------------------------------

    #[tokio::test]
    async fn list_types_lists_module_type_pairs() {
        let fake = FakeSftp::new();
        standard_tree(&fake);
        fake.file(
            &format!("{RES}/core/ignition/alarm-journal/a/config.json"),
            "{}",
        );
        fake.file(&format!("{RES}/core/ignition/loose.json"), "{}");
        fake.file(&format!("{RES}/core/zeta/thing/x"), "{}");
        let t = transport(&fake);
        assert_eq!(
            t.list_types("core").await.expect("types"),
            [
                ("ignition".to_string(), "alarm-journal".to_string()),
                ("ignition".to_string(), "database-connection".to_string()),
                ("zeta".to_string(), "thing".to_string()),
            ]
        );
        assert!(t.list_types("nothing").await.expect("absent").is_empty());
    }

    // ---- read_ledger -----------------------------------------------------------

    fn ledger_path() -> String {
        format!("{DATA}/ign-deploy/gw/production/{LEDGER_FILE}")
    }

    #[tokio::test]
    async fn absent_ledger_at_any_level_is_none() {
        let fake = FakeSftp::new();
        let t = transport(&fake);
        assert_eq!(t.read_ledger("gw", "production").await.expect("none"), None);
        fake.mkdirs(&format!("{DATA}/ign-deploy"));
        assert_eq!(t.read_ledger("gw", "production").await.expect("none"), None);
        fake.mkdirs(&format!("{DATA}/ign-deploy/gw/production"));
        assert_eq!(t.read_ledger("gw", "production").await.expect("none"), None);
    }

    #[tokio::test]
    async fn ledger_present_round_trips() {
        let fake = FakeSftp::new();
        fake.file(&ledger_path(), "{\"schema_version\":1}");
        let t = transport(&fake);
        assert_eq!(
            t.read_ledger("gw", "production").await.expect("ledger"),
            Some(b"{\"schema_version\":1}".to_vec())
        );
    }

    #[tokio::test]
    async fn ledger_anomalies_are_deploy_ledger_invalid() {
        let invalid = |err: CoreError| match err {
            CoreError::DeployLedgerInvalid { problem, .. } => problem,
            other => panic!("expected deploy_ledger_invalid, got {other:?}"),
        };
        // the file is a symlink
        let fake = FakeSftp::new();
        fake.put(&ledger_path(), Node::Symlink);
        let p = invalid(
            transport(&fake)
                .read_ledger("gw", "production")
                .await
                .expect_err("link"),
        );
        assert!(p.contains("symlink"), "{p}");
        // a directory in the middle is a symlink
        let fake = FakeSftp::new();
        fake.put(&format!("{DATA}/ign-deploy/gw"), Node::Symlink);
        let p = invalid(
            transport(&fake)
                .read_ledger("gw", "production")
                .await
                .expect_err("link"),
        );
        assert!(p.contains("symlink"), "{p}");
        // a component that is a file
        let fake = FakeSftp::new();
        fake.file(&format!("{DATA}/ign-deploy/gw"), "x");
        let p = invalid(
            transport(&fake)
                .read_ledger("gw", "production")
                .await
                .expect_err("file"),
        );
        assert!(p.contains("not a directory"), "{p}");
        // the ledger is a directory
        let fake = FakeSftp::new();
        fake.mkdirs(&ledger_path());
        let p = invalid(
            transport(&fake)
                .read_ledger("gw", "production")
                .await
                .expect_err("dir"),
        );
        assert!(p.contains("not a regular file"), "{p}");
        // over the cap, from the listing, before a download
        let fake = FakeSftp::new();
        fake.file(&ledger_path(), "{}");
        fake.knobs().lie_size = Some((LEDGER_FILE.to_string(), LEDGER_MAX_BYTES as u64 + 1));
        let p = invalid(
            transport(&fake)
                .read_ledger("gw", "production")
                .await
                .expect_err("cap"),
        );
        assert!(p.contains("byte cap"), "{p}");
        assert!(!fake.session_lines().pop().expect("last")[0].starts_with("get"));
        // a hostile gateway name
        let fake = FakeSftp::new();
        let p = invalid(
            transport(&fake)
                .read_ledger("../x", "production")
                .await
                .expect_err("name"),
        );
        assert!(p.contains("gateway"), "{p}");
    }

    #[tokio::test]
    async fn unreadable_host_is_not_an_absent_ledger() {
        let fake = FakeSftp::new();
        let t = transport_at(&fake, "/gw/nope");
        fake.listed
            .lock()
            .expect("listed")
            .insert("/gw/nope".to_string());
        let err = t.read_ledger("gw", "production").await.expect_err("host");
        assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");

        let fake = FakeSftp::new();
        fake.file(&ledger_path(), "{}");
        fake.knobs().fail_get = false;
        fake.knobs().stderr_noise = true;
        let err = transport(&fake)
            .read_ledger("gw", "production")
            .await
            .expect_err("stderr");
        assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
    }

    // ---- resource caps (post-review) -------------------------------------------

    /// Answers every `@ls -la` with a fixed directory chain and `filler`
    /// extra regular files per directory, and counts the sessions it served.
    struct FloodRunner {
        filler: usize,
        sessions: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl ProcessRunner for FloodRunner {
        async fn run(
            &self,
            _program: &OsStr,
            _args: &[OsString],
            stdin: Option<Vec<u8>>,
            _timeout: Duration,
        ) -> ProcessOutput {
            *self.sessions.lock().expect("sessions") += 1;
            let batch = String::from_utf8(stdin.expect("stdin")).expect("utf8");
            let mut out = String::new();
            for l in batch.lines() {
                let dir = quoted(l).remove(0);
                out.push_str(&format!("{}\n", line("drwxr-xr-x", 1, &format!("{dir}/."))));
                let child = match dir.as_str() {
                    DATA => Some("config"),
                    d if d == format!("{DATA}/config") => Some("resources"),
                    d if d == RES => Some("core"),
                    _ => None,
                };
                if let Some(child) = child {
                    out.push_str(&format!(
                        "{}\n",
                        line("drwxr-xr-x", 1, &format!("{dir}/{child}"))
                    ));
                }
                for n in 0..self.filler {
                    out.push_str(&format!(
                        "{}\n",
                        line("-rw-r--r--", 1, &format!("{dir}/f{n}"))
                    ));
                }
            }
            ProcessOutput {
                stdout: out.into_bytes(),
                stderr: Vec::new(),
                code: 0,
                timed_out: false,
            }
        }
    }

    fn flood_transport(filler: usize) -> (Arc<FloodRunner>, SftpTransport) {
        let runner = Arc::new(FloodRunner {
            filler,
            sessions: Mutex::new(0),
        });
        let shell = RemoteShell::local_sftp_server(
            Path::new("/unused"),
            runner.clone(),
            Duration::from_secs(30),
        );
        (
            runner,
            SftpTransport::new(shell, DATA.to_string(), "nodes.master"),
        )
    }

    #[test]
    fn entry_cap_stops_the_parse_at_the_offending_line() {
        let dir = "/d".to_string();
        let ok = |n: usize| -> String {
            (0..n)
                .map(|i| line("-rw-r--r--", 1, &format!("/d/f{i}")))
                .collect::<Vec<_>>()
                .join("\n")
        };
        // exactly at the cap is fine; one more is not
        assert_eq!(
            parse_batch_listing_capped(std::slice::from_ref(&dir), ok(5).as_bytes(), 5)
                .expect("at the cap")[0]
                .len(),
            5
        );
        assert_eq!(
            parse_batch_listing_capped(std::slice::from_ref(&dir), ok(6).as_bytes(), 5),
            Err(ListingError::TooMany)
        );
        // the line that passes the cap ends the parse: a hostile line after it
        // is never looked at (a TooMany, not a Bad)
        let hostile = format!("{}\n{}", ok(6), "garbage that would be a parse error");
        assert_eq!(
            parse_batch_listing_capped(std::slice::from_ref(&dir), hostile.as_bytes(), 5),
            Err(ListingError::TooMany)
        );
        // `.` and `..` do not count as entries
        let dots = format!(
            "{}\n{}\n{}",
            line("drwxr-xr-x", 1, "/d/."),
            line("drwxr-xr-x", 1, "/d/.."),
            ok(5)
        );
        assert!(parse_batch_listing_capped(std::slice::from_ref(&dir), dots.as_bytes(), 5).is_ok());
    }

    #[tokio::test]
    async fn entry_cap_rejects_one_huge_listing_in_its_first_session() {
        let (runner, t) = flood_transport(READ_MAX_ENTRIES + 1);
        let why = detail(t.read(&scope()).await.expect_err("flood"));
        assert!(
            why.contains(&format!("more than {READ_MAX_ENTRIES} entries")),
            "{why}"
        );
        assert_eq!(*runner.sessions.lock().expect("sessions"), 1);
    }

    #[tokio::test]
    async fn entry_cap_is_cumulative_across_levels() {
        // each directory is far under the cap; three levels together are not
        let (runner, t) = flood_transport(READ_MAX_ENTRIES / 3 + 100);
        let why = detail(t.read(&scope()).await.expect_err("cumulative"));
        assert!(
            why.contains(&format!("more than {READ_MAX_ENTRIES} entries")),
            "{why}"
        );
        assert_eq!(*runner.sessions.lock().expect("sessions"), 3, "{why}");

        // list_types and the ledger walk keep their own cumulative budget
        let (_, t) = flood_transport(READ_MAX_ENTRIES + 1);
        assert!(t.list_types("core").await.is_err());
        let (_, t) = flood_transport(READ_MAX_ENTRIES + 1);
        assert!(t.read_ledger("gw", "production").await.is_err());
    }

    #[test]
    fn file_cap_is_rechecked_through_the_handle() {
        // an on-disk file over the cap is refused even if the listing agrees
        let dir = tempfile::tempdir().expect("dir");
        let file = std::fs::File::create(dir.path().join("big")).expect("create");
        file.set_len(READ_MAX_FILE_BYTES + 1).expect("sparse");
        drop(file);
        assert!(matches!(
            read_downloaded(
                dir.path(),
                Path::new("big"),
                READ_MAX_FILE_BYTES + 1,
                READ_MAX_FILE_BYTES
            ),
            Err(Readback::Capped(_))
        ));
        // a file that does not match its listing is refused
        std::fs::write(dir.path().join("small"), b"12345").expect("write");
        assert!(matches!(
            read_downloaded(dir.path(), Path::new("small"), 3, READ_MAX_FILE_BYTES),
            Err(Readback::SizeMismatch {
                listed: 3,
                actual: 5
            })
        ));
        assert_eq!(
            read_downloaded(dir.path(), Path::new("small"), 5, READ_MAX_FILE_BYTES)
                .ok()
                .as_deref(),
            Some(&b"12345"[..])
        );
    }

    #[tokio::test]
    async fn oversize_download_is_aborted_and_cleaned_up() {
        // the listing says 10 bytes; the server delivers far more
        let fake = FakeSftp::new();
        fake.file(
            &format!("{RES}/production/ignition/database-connection/A/config.json"),
            "0123456789",
        );
        fake.knobs().overshoot = Some(("A/config.json".to_string(), 8 * 1024 * 1024));
        let t = transport(&fake);
        let started = std::time::Instant::now();
        let why = detail(t.read(&scope()).await.expect_err("overshoot"));
        assert!(why.contains("it was aborted"), "{why}");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the stalled transfer was killed, not waited for"
        );
        let written = fake.downloads.lock().expect("downloads").clone();
        assert!(!written.is_empty());
        for path in written {
            assert!(!path.exists(), "{} was cleaned up", path.display());
        }
    }

    #[tokio::test]
    async fn total_cap_is_checked_before_each_chunk() {
        // 8 files listed at 64 MiB: 512 MiB, under the total cap, so they are
        // fetched in chunks of 256 MiB (4 files). The first chunk delivers the
        // real (tiny) bytes, fails its size check, and the second never starts.
        let fake = FakeSftp::new();
        for n in 0..8 {
            fake.file(
                &format!("{RES}/production/ignition/database-connection/D{n}/config.json"),
                "{}",
            );
        }
        fake.knobs().lie_size = Some(("config.json".to_string(), READ_MAX_FILE_BYTES));
        let why = detail(transport(&fake).read(&scope()).await.expect_err("chunk"));
        assert!(why.contains("was listed as"), "{why}");
        let get_sessions = fake
            .session_lines()
            .iter()
            .filter(|lines| lines[0].starts_with("get \""))
            .count();
        assert_eq!(get_sessions, 1, "the second chunk never started");
        let first = fake
            .session_lines()
            .into_iter()
            .find(|lines| lines[0].starts_with("get \""))
            .expect("get session");
        assert_eq!(first.len(), 4, "a chunk is bounded by listed bytes");
    }

    #[tokio::test]
    async fn recursive_get_is_never_issued() {
        let fake = FakeSftp::new();
        standard_tree(&fake);
        transport(&fake).read(&scope()).await.expect("read");
        transport(&fake)
            .read_ledger("gw", "production")
            .await
            .expect("ledger");
        let all: Vec<String> = fake.session_lines().into_iter().flatten().collect();
        assert!(all.iter().any(|l| l.starts_with("get \"")));
        assert!(all.iter().all(|l| !l.contains("get -R")), "{all:?}");
    }

    #[tokio::test]
    async fn data_dir_without_config_resources_is_transport_failed() {
        for missing in ["config", "config/resources"] {
            let fake = FakeSftp::new();
            fake.mkdirs(RES);
            let full = format!("{DATA}/{missing}");
            fake.fs
                .lock()
                .expect("fs")
                .retain(|k, _| k != &full && !k.starts_with(&format!("{full}/")));
            let why = detail(transport(&fake).read(&scope()).await.expect_err(missing));
            assert!(why.contains(missing), "{missing}: {why}");
            assert!(why.contains("gateway data directory"), "{why}");
        }
        // also with an empty scope
        let fake = FakeSftp::new();
        let empty = Scope {
            mode: "production".to_string(),
            types: Vec::new(),
            tag_providers: Vec::new(),
        };
        detail(
            transport(&fake)
                .read(&empty)
                .await
                .expect_err("bare data dir"),
        );
    }
}
