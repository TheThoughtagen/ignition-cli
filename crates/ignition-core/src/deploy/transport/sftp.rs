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
//!    of that depth with `ls -lan` (`ls -la` on a Windows host), so every
//!    file, its size, and any symlink or special file is known BEFORE a byte
//!    is downloaded.
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
//! The host controls every listing line. A POSIX listing line is accepted
//! only when it carries the exact listed-directory prefix (a Windows line is
//! the server's bare longname), a recognised mode column and a numeric size;
//! the output is split on `\n` alone, so a `\r` inside a name stays in it.
//! A name that is empty, holds `/`, or fails
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
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

mod apply_ops;
mod project_ops;

use super::remote::{RemoteShell, SftpOp, safe_remote_path};
use super::winps::{
    self, LONG_PATH_LIMIT, LONG_PATH_PREFIX, windows_absolute_path_problem,
    windows_component_problem,
};
use super::{
    CORE_COLLECTION, DeleteSet, FileBlob, MirrorReport, ObservedFiles, READ_MAX_ENTRIES,
    READ_MAX_FILE_BYTES, READ_MAX_TOTAL_BYTES, Scope, Staged, StagingMode, TreeMirror, TreeReader,
    absolute_components, copy_to_temp, mirror_mode_problem, read_staged_copy,
    validate_mirror_inputs, validate_mirror_paths, validate_staging_root_for,
};
use crate::client::config_resources::validate_segment;
use crate::deploy::apply::journal::JOURNAL_MAX_BYTES;
use crate::deploy::component_problem;
use crate::deploy::layout::{RelPath, TAG_TYPES};
use crate::deploy::ledger::{
    LEDGER_MAX_BYTES, LedgerRoot, journal_location, journal_rel, ledger_location, ledger_rel,
};
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

/// The longest Windows `data_dir` accepted: the sftp-server resolves no path
/// of 260 characters (C7a-8), and `config/resources/<collection>/<module>/<type>`
/// below the data directory must always resolve, because a directory listing
/// that cannot see its own children cannot be told from an empty one.
pub const WINDOWS_DATA_DIR_MAX: usize = 150;

/// The sftp spelling of the Windows `data_dir` of an `ssh` node (C7a-1):
/// `/` + the drive letter, upper-cased, + `:` + the path with forward slashes
/// (`C:\Program Files\Inductive Automation\Ignition\data` becomes
/// `/C:/Program Files/Inductive Automation/Ignition/data`). `C:/...` without
/// the leading slash is relative to the account's home on this server, and a
/// backslash after the drive is not found, so neither is ever produced.
///
/// Refused: UNC and `\\?\` paths, relative and drive-relative paths (`C:foo`),
/// anything not starting with a drive letter and a separator, empty, `.` and
/// `..` components, a `:` after the drive (an alternate data stream), a
/// component ending in a dot or a space (Windows strips them, so two names
/// would mean one directory), a path over [`WINDOWS_DATA_DIR_MAX`], the bare
/// drive root, and whatever [`safe_remote_path`] refuses (quotes, control and
/// glob characters).
pub fn windows_sftp_root(data_dir: &str) -> Result<String, String> {
    let normal = data_dir.replace('\\', "/");
    if normal.starts_with("//") {
        return Err(format!(
            "{data_dir:?} is a UNC or extended-length path; use a drive letter path such as C:\\Program Files\\Inductive Automation\\Ignition\\data"
        ));
    }
    let bytes = normal.as_bytes();
    let drive_form =
        bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/';
    if !drive_form {
        return Err(format!(
            "{data_dir:?} is not an absolute Windows path (it must start with a drive letter, a colon and a separator, e.g. C:\\Program Files\\...)"
        ));
    }
    let drive = (bytes[0] as char).to_ascii_uppercase();
    let rest = normal[3..].trim_end_matches('/');
    if rest.is_empty() {
        return Err("a drive root cannot be a gateway data directory".to_string());
    }
    for part in rest.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err(format!(
                "{data_dir:?} must not contain empty, `.` or `..` components"
            ));
        }
        if part.contains(':') {
            return Err(format!(
                "{data_dir:?} has a `:` after the drive letter (an alternate data stream name)"
            ));
        }
        if part.ends_with('.') || part.ends_with(' ') {
            return Err(format!(
                "{data_dir:?} has a component ending in a dot or a space, which Windows strips"
            ));
        }
    }
    let root = format!("/{drive}:/{rest}");
    if root.chars().count() > WINDOWS_DATA_DIR_MAX {
        return Err(format!(
            "{data_dir:?} is longer than {WINDOWS_DATA_DIR_MAX} characters; the sftp server cannot resolve paths of 260 characters or more"
        ));
    }
    safe_remote_path(&root)?;
    Ok(root)
}

/// The native Windows spelling (`C:\a b\c`) of an sftp path made by
/// [`windows_sftp_root`] (`/C:/a b/c`), for the PowerShell enumeration.
fn windows_native_path(sftp_path: &str) -> String {
    sftp_path.trim_start_matches('/').replace('/', "\\")
}

/// Parse the stdout of one `@pwd`, `@ls -lan "<dir>"` batch for the single
/// POSIX directory `dir`. See [`parse_batch_listing`].
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

/// The host family a long listing comes from. In both families an `@pwd`
/// marker leads every listing, so a line is attributed to its directory by
/// the marker.
///
/// The OpenSSH client prints `\n`, `\r` and `\t` inside a name raw
/// (openssh-portable `utf8.c`), so a POSIX name holding `\n` reaches the
/// parser as two lines. For `ls -la` the client prints the server's bare
/// `longname` unless the server offers `users-groups-by-id@openssh.com` (the
/// macOS `sftp-server` does, Ubuntu's OpenSSH 8.9 does not), and a bare line
/// is byte-identical to the tail of a name that held a `\n`: no parser rule
/// can tell them apart. So a POSIX host is listed with `ls -lan`, which makes
/// every OpenSSH client format every line itself with the listed path in
/// front of the name, and a POSIX line without that prefix is refused (a
/// name fragment cannot hold `/`). This reverses the 23-22 (G1) choice of
/// `ls -la` with a bare fallback; the captured bare 8.9 listing is now refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListFormat {
    /// A POSIX host (Linux, macOS), listed with `ls -lan`: every entry line,
    /// `.` and `..` included, carries the exact `<dir>/` prefix. Names are
    /// ordinary POSIX names.
    Posix,
    /// A Windows host (Win32-OpenSSH), listed with `ls -la`: the server's
    /// bare names (a line holding the `<dir>/` prefix is read the same way as
    /// a POSIX one). A Win32 name cannot hold `\n` or `\r`; a name also passes
    /// the Windows component gate and no two names of a directory differ only
    /// by case. Reads are checked against a PowerShell walk as well.
    Windows,
}

/// What a listing does with a name that cannot be placed in a later batch line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameRule {
    /// The whole listing is refused. Used wherever every child of the
    /// directory is followed or read, so a skipped name would be an
    /// incomplete read.
    Refuse,
    /// The entry is left out. Used for directories where only fixed, safe
    /// names are ever looked up (an account home holds
    /// `NTUSER.DAT{<guid>}.TM.blf`, whose braces are glob characters).
    Skip,
}

/// The line `@pwd` prints before its path.
const PWD_MARKER: &str = "Remote working directory: ";

/// [`parse_batch_listing_as`] for a POSIX host.
pub fn parse_batch_listing_capped(
    dirs: &[String],
    stdout: &[u8],
    max_entries: usize,
) -> Result<Vec<Vec<ListedEntry>>, ListingError> {
    parse_batch_listing_as(
        ListFormat::Posix,
        NameRule::Refuse,
        dirs,
        stdout,
        max_entries,
    )
}

/// Parse the stdout of a batch that listed `dirs` in order, each listing led
/// by an `@pwd` marker, into one entry list per directory (byte-sorted by
/// name).
///
/// The output is split on `\n` alone, so a `\r` inside a name stays in the
/// name. One trailing `\r` is stripped from each line only when EVERY line,
/// the markers included, ends in `\r\n` (the Win32-OpenSSH client writes
/// CRLF, the other OpenSSH clients LF); in a listing whose line ends are
/// mixed every `\r` stays, so the names that hold one are refused.
///
/// A `Remote working directory:` line starts the next directory; the number
/// of markers must equal the number of directories. Every other line is `<mode>
/// <links> <owner> <group> <size> <month> <day> <time-or-year> <name>`. Its
/// name is `<dir>/<name>` (the client formatted the line; the exact prefix of
/// the CURRENT directory is found and ONE component is taken after it). For
/// [`ListFormat::Posix`] that is the only shape and a line without the prefix
/// is refused. For [`ListFormat::Windows`] a line without it is the server's
/// bare longname: the name is everything after the eighth column and the
/// single space that follows it, so a name with spaces survives, holding no
/// `/`. The day and time-or-year columns must read as a date, so a line whose
/// owner or group holds a space is refused rather than read under a wrong
/// name. A line before the first marker, an empty line, a line with an empty
/// or multi-component name, with an unrecognised mode or a non-numeric size,
/// a name that fails [`safe_remote_path`] (a client that did not run in a
/// UTF-8 locale prints `caf\303\251`, whose backslash is refused here), or a
/// name listed twice is an error. `.` and `..` are dropped. For
/// [`ListFormat::Windows`] a name also passes the Windows component gate and
/// no two names differ only by case.
///
/// Entries are counted AS EACH LINE IS PARSED: the line that makes the count
/// pass `max_entries` stops the parse with [`ListingError::TooMany`] before
/// it is stored and before any later line is parsed, so a huge or endless
/// listing is never materialised (the line-end check before the parse stores
/// nothing). The 32 MiB stdout cap of the process runner is a backstop, not
/// this check.
pub fn parse_batch_listing_as(
    format: ListFormat,
    names: NameRule,
    dirs: &[String],
    stdout: &[u8],
    max_entries: usize,
) -> Result<Vec<Vec<ListedEntry>>, ListingError> {
    let bad = |why: String| ListingError::Bad(why);
    let text = std::str::from_utf8(stdout)
        .map_err(|_| bad("the listing holds a name that is not valid UTF-8".to_string()))?;
    let prefixes: Vec<String> = dirs.iter().map(|d| format!("{d}/")).collect();
    let mut out: Vec<Vec<ListedEntry>> = vec![Vec::new(); dirs.len()];
    // `.` and `..` of every directory are lines too, and so is a marker
    let max_lines = max_entries.saturating_add(dirs.len().saturating_mul(3));
    let mut lines_seen = 0usize;
    let mut entries_seen = 0usize;
    let mut current = 0usize;
    let mut markers = 0usize;
    for line in output_lines(text) {
        lines_seen += 1;
        if lines_seen > max_lines {
            return Err(ListingError::TooMany);
        }
        if line.is_empty() {
            return Err(bad("the listing holds an empty line".to_string()));
        }
        if line.starts_with(PWD_MARKER) {
            markers += 1;
            if markers > dirs.len() {
                return Err(bad(
                    "the listing holds more directory markers than directories".to_string(),
                ));
            }
            current = markers - 1;
            continue;
        }
        if markers == 0 {
            return Err(bad(format!(
                "a listing line comes before the first directory marker: {line:?}"
            )));
        }
        let (columns, name) =
            line_columns_and_name(format, line, &prefixes[current]).map_err(bad)?;
        if name.contains('/') {
            return Err(bad(format!(
                "a listing line holds a multi-component name: {line:?}"
            )));
        }
        if name != "." && name != ".." {
            entries_seen += 1;
            if entries_seen > max_entries {
                return Err(ListingError::TooMany);
            }
        }
        if columns.len() < 8 {
            return Err(bad(format!("a listing line has too few columns: {line:?}")));
        }
        let mode = columns[0];
        if mode.len() < 10 || !mode.is_ascii() {
            return Err(bad(format!("a listing line has an unusable mode {mode:?}")));
        }
        let kind = match mode.as_bytes()[0] {
            b'-' => EntryKind::File,
            b'd' => EntryKind::Dir,
            b'l' => EntryKind::Symlink,
            _ => EntryKind::Other,
        };
        // month, day and time follow the size: it is the fourth from last
        let size_text = columns[columns.len() - 4];
        let size: u64 = size_text.parse().map_err(|_| {
            bad(format!(
                "a listing line has a non-numeric size {size_text:?}"
            ))
        })?;
        // A bare line is cut after its eighth column, so an owner or group
        // with a space shifts every column: the date columns then read
        // something else and the line is refused, never misread.
        if !reads_as_date(columns[columns.len() - 2], columns[columns.len() - 1]) {
            return Err(bad(format!(
                "a listing line does not carry a date after its size: {line:?}"
            )));
        }
        if name == "." || name == ".." {
            continue;
        }
        if name.is_empty() {
            return Err(bad("a listing line has an empty name".to_string()));
        }
        let problem = safe_remote_path(name).err().or_else(|| {
            // a Windows host's names also pass the Windows component gate
            (format == ListFormat::Windows)
                .then(|| windows_component_problem(name))
                .flatten()
        });
        if let Some(why) = problem {
            if names == NameRule::Skip {
                continue;
            }
            return Err(bad(format!("the name {name:?} is unsafe: {why}")));
        }
        out[current].push(ListedEntry {
            name: name.to_string(),
            kind,
            size,
        });
    }
    if markers != dirs.len() {
        return Err(bad(format!(
            "the listing holds {markers} directory markers for {} directories",
            dirs.len()
        )));
    }
    for entries in &mut out {
        entries.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
        if let Some(pair) = entries.windows(2).find(|w| w[0].name == w[1].name) {
            return Err(bad(format!("the name {:?} is listed twice", pair[0].name)));
        }
        if format == ListFormat::Windows {
            // NTFS is case-insensitive: two names that differ only by case
            // cannot be two entries of one directory
            let mut folded: BTreeSet<String> = BTreeSet::new();
            if let Some(dup) = entries
                .iter()
                .find(|e| !folded.insert(e.name.to_lowercase()))
            {
                return Err(bad(format!(
                    "the name {:?} is listed twice, ignoring case",
                    dup.name
                )));
            }
        }
    }
    Ok(out)
}

/// The lines of an sftp client's stdout. The text is split on `\n` alone (a
/// final `\n` ends the last line, it does not start an empty one), so a `\r`
/// inside a name stays in its line; `str::lines` would drop a `\r` before
/// each `\n` and read a name ending in `\r` as another name. One trailing
/// `\r` is stripped from each line only when EVERY line ends in `\r`: the
/// Win32-OpenSSH client ends every line it prints in CRLF (OpenSSH_9.5p2 for
/// Windows, measured on a pipe), the other OpenSSH clients in LF. When the
/// line ends are mixed, every `\r` stays.
fn output_lines(text: &str) -> impl Iterator<Item = &str> {
    let crlf = text.split_terminator('\n').all(|l| l.ends_with('\r'));
    text.split_terminator('\n').map(move |l| {
        if crlf {
            l.strip_suffix('\r').unwrap_or(l)
        } else {
            l
        }
    })
}

/// The columns before the name and the name of one listing line of the
/// directory whose prefix (`<dir>/`) is `prefix`. A line the client formatted
/// holds the exact prefix, preceded by a space, with one component after it.
/// For [`ListFormat::Posix`] (`ls -lan`) that is the only shape: a line
/// without it is refused, since it may be the tail of a name that held a raw
/// `\n`. For [`ListFormat::Windows`] (`ls -la`) a line without it is the
/// server's bare longname and the name is what follows the eighth column.
fn line_columns_and_name<'a>(
    format: ListFormat,
    line: &'a str,
    prefix: &str,
) -> Result<(Vec<&'a str>, &'a str), String> {
    if let Some(at) = line.rfind(prefix) {
        let name = &line[at + prefix.len()..];
        if at > 0 && line.as_bytes()[at - 1] == b' ' && !name.contains('/') {
            return Ok((line[..at].split_whitespace().collect(), name));
        }
    }
    match format {
        ListFormat::Posix => Err(format!(
            "a listing line does not carry the listed directory {prefix:?} and one name after it: {line:?}"
        )),
        ListFormat::Windows => split_columns(line, 8)
            .ok_or_else(|| format!("a listing line has too few columns: {line:?}")),
    }
}

/// True when `day` is 1 to 31 and `tail` is a time (`H:MM`, `HH:MM`) or a
/// four-digit year: the last two columns of an `ls -l` date.
fn reads_as_date(day: &str, tail: &str) -> bool {
    let digits = |t: &str| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit());
    let day_ok =
        digits(day) && day.len() <= 2 && (1..=31).contains(&day.parse::<u8>().unwrap_or(0));
    let tail_ok = match tail.split_once(':') {
        Some((hour, minute)) => {
            digits(hour) && hour.len() <= 2 && digits(minute) && minute.len() == 2
        }
        None => digits(tail) && tail.len() == 4,
    };
    day_ok && tail_ok
}

/// The first `count` whitespace-separated columns of `line` and the rest of
/// the line after the single space that follows the last of them (so a name
/// that starts or ends with spaces is kept). `None` when the line has fewer
/// columns or no name.
fn split_columns(line: &str, count: usize) -> Option<(Vec<&str>, &str)> {
    let mut columns = Vec::with_capacity(count);
    let mut rest = line.trim_start();
    for _ in 0..count {
        let end = rest.find(char::is_whitespace)?;
        columns.push(&rest[..end]);
        let after = &rest[end..];
        // the separator is one space between the last column and the name,
        // any run of spaces between the earlier columns
        rest = if columns.len() == count {
            after.strip_prefix(' ')?
        } else {
            after.trim_start()
        };
    }
    Some((columns, rest))
}

/// Running totals of one read.
#[derive(Default)]
struct Budget {
    entries: usize,
    /// Leave out names that are unsafe in a batch line instead of refusing
    /// the listing (see [`NameRule::Skip`]).
    lenient: bool,
}

impl Budget {
    /// A budget for directories where only fixed names are looked up.
    fn lenient() -> Budget {
        Budget {
            entries: 0,
            lenient: true,
        }
    }
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
    /// Every directory below a root (a root itself is not listed), as chains
    /// below `data_dir`.
    dirs: Vec<Vec<String>>,
}

/// Reads a Linux gateway data directory over system `sftp`.
pub struct SftpTransport {
    shell: RemoteShell,
    data_dir: String,
    label: String,
    sessions: AtomicUsize,
    mirror: Option<MirrorSetup>,
    /// A Windows OpenSSH host: listings are the marked format and a read is
    /// cross-checked against a PowerShell enumeration (22-13).
    windows: bool,
    /// The account's home directory as the server spells it, read once with
    /// `pwd`.
    home: std::sync::OnceLock<String>,
    /// The apply half (23-06): the target gateway and mode.
    apply: Option<apply_ops::ApplySetup>,
    /// The work lists `prepare` staged, by the SHA-256 of their bytes, so
    /// `commit` can name the entries a script result points at.
    lists: Mutex<BTreeMap<String, super::batch::WorkList>>,
    /// Numbers the throw-away control directories of this transport.
    ctl_seq: AtomicUsize,
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
            windows: false,
            home: std::sync::OnceLock::new(),
            apply: None,
            lists: Mutex::new(BTreeMap::new()),
            ctl_seq: AtomicUsize::new(0),
        }
    }

    /// A reader over a Windows OpenSSH host's `data_dir` (use
    /// [`windows_sftp_root`] to build it from a target). Listings use the
    /// marked format (C7a-2), paths compare case-insensitively where it
    /// matters, and a read is not reported complete until a PowerShell walk
    /// with the `\\?\` prefix agrees with what sftp returned (C7a-8).
    pub fn new_windows(shell: RemoteShell, data_dir: String, label: impl Into<String>) -> Self {
        let mut transport = Self::new(shell, data_dir, label);
        transport.windows = true;
        transport
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

    /// One sftp session: list every directory of `dirs` (`ls -lan` on a POSIX
    /// host, `ls -la` on a Windows host).
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
        // Each listing is led by a `pwd` marker that says whose lines follow
        // (C7a-2). A Windows host is listed with `ls -la` (the server's bare
        // names; a Win32 name holds no line break). A POSIX host is listed
        // with `ls -lan`, so the client formats every line with the listed
        // path in front of the name and a name split by a raw `\n` is refused
        // (see [`ListFormat`]).
        let mut ops: Vec<SftpOp> = Vec::with_capacity(dirs.len() * 2);
        for path in dirs {
            ops.push(SftpOp::Pwd);
            let path = path.clone();
            ops.push(if self.windows {
                SftpOp::LsLongAll { path }
            } else {
                SftpOp::LsLongAllNumeric { path }
            });
        }
        self.sessions.fetch_add(1, Ordering::Relaxed);
        let out = self.shell.sftp_batch(&ops).await?;
        let out = self.shell.require_success(op, out)?;
        let remaining = READ_MAX_ENTRIES.saturating_sub(budget.entries);
        let fail = |detail: String| CoreError::TransportFailed {
            operation: op.to_string(),
            node: self.label.clone(),
            detail,
        };
        let format = if self.windows {
            ListFormat::Windows
        } else {
            ListFormat::Posix
        };
        let names = if budget.lenient {
            NameRule::Skip
        } else {
            NameRule::Refuse
        };
        let listings = parse_batch_listing_as(format, names, dirs, &out.stdout, remaining)
            .map_err(|e| match e {
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
            // only a whole component of data_dir: `/data-evil` is not below `/data`
            .filter(|rest| rest.is_empty() || rest.starts_with('/'))
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
        self.descend_from(&self.data_dir, components, budget).await
    }

    /// [`descend`](Self::descend) starting at the absolute directory `base`.
    async fn descend_from(
        &self,
        base: &str,
        components: &[&str],
        budget: &mut Budget,
    ) -> Result<Option<(String, ListedEntry)>, Descend> {
        let mut current = base.to_string();
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

    /// The account's home directory as the server spells it (`/C:/Users/<user>`
    /// on Windows OpenSSH, `/home/<user>` on Linux), asked once with `pwd`.
    /// Ledgers and staging live below it (Decision B(i)); the answer is held to
    /// the same rules as any path that reaches a batch line.
    pub async fn account_home(&self) -> Result<String, CoreError> {
        if let Some(home) = self.home.get() {
            return Ok(home.clone());
        }
        self.sessions.fetch_add(1, Ordering::Relaxed);
        let out = self.shell.sftp_batch(&[SftpOp::Pwd]).await?;
        let out = self.shell.require_success("read", out)?;
        let text = std::str::from_utf8(&out.stdout)
            .map_err(|_| self.fail("the account home is not valid UTF-8"))?;
        // the batch is `@pwd` alone, so the output is that one line: any other
        // line is the tail of a home that held a raw `\n`, never ignored
        let mut lines = output_lines(text);
        let (Some(raw), None) = (
            lines.next().and_then(|l| l.strip_prefix(PWD_MARKER)),
            lines.next(),
        ) else {
            return Err(
                self.fail("the server did not report exactly one working directory, on one line")
            );
        };
        let trimmed = raw.trim_end_matches('/');
        let problem = if self.windows {
            windows_absolute_path_problem(trimmed)
        } else if !trimmed.starts_with('/') {
            Some("is not an absolute path".to_string())
        } else {
            safe_remote_path(trimmed).err().or_else(|| {
                trimmed
                    .split('/')
                    .skip(1)
                    .any(|part| part.is_empty() || part == "." || part == "..")
                    .then(|| "has an empty, `.` or `..` component".to_string())
            })
        };
        if let Some(why) = problem {
            return Err(self.fail(format!("the account home {raw:?} cannot be used: {why}")));
        }
        let _ = self.home.set(trimmed.to_string());
        Ok(trimmed.to_string())
    }

    /// The Decision B(i) staging mode: `Separate` at
    /// `<home>/ign-deploy/<gateway>/<mode>/staging/<run_id>`, where `<home>`
    /// is what the server reports for the account ([`account_home`](Self::account_home)).
    /// `gateway`, `mode` and `run_id` are checked by the caller
    /// ([`default_staging_mode`](super::default_staging_mode)).
    pub async fn default_staging(
        &self,
        gateway: &str,
        mode: &str,
        run_id: &str,
    ) -> Result<StagingMode, CoreError> {
        for (what, value) in [("gateway", gateway), ("mode", mode), ("run id", run_id)] {
            self.windows_gate(&format!("the staging {what}"), value)?;
        }
        let home = self.account_home().await?;
        let root = format!(
            "{home}/{}",
            crate::deploy::ledger::staging_rel(gateway, mode, run_id)
        );
        safe_remote_path(&root)
            .map_err(|why| self.fail(format!("the staging root is unsafe: {why}")))?;
        if self.windows
            && let Some(why) = windows_absolute_path_problem(&root)
        {
            return Err(self.fail(format!("the staging root is refused: {why}")));
        }
        Ok(StagingMode::Separate { root })
    }

    /// Windows only: the sftp listing is not proof of completeness on this
    /// host. The server resolves no path of 260 characters or more and drops
    /// such entries from a listing with exit 0 (C7a-8); a junction lists as a
    /// plain directory and would be followed (C7a-5). One PowerShell walk per
    /// batch of scoped roots, with the `\\?\` prefix, must agree with what
    /// sftp returned: the same files at the same sizes and the same
    /// directories, no reparse point, no entry whose path reaches
    /// [`LONG_PATH_LIMIT`]. Any difference is `transport_failed` naming the
    /// paths; a read is never reported complete without this check.
    async fn cross_check(&self, found: &Found) -> Result<(), CoreError> {
        if found.roots.is_empty() {
            return Ok(());
        }
        let roots: Vec<String> = found
            .roots
            .iter()
            .map(|chain| windows_native_path(&self.abs(chain)))
            .collect();
        let chunks = winps::enumerate_chunks(LONG_PATH_PREFIX, &roots).map_err(|why| {
            self.fail(format!("cannot check the listing for completeness: {why}"))
        })?;
        let mut enumerated: Vec<Option<Vec<winps::EnumEntry>>> = vec![None; roots.len()];
        for (range, script) in chunks {
            let value = self
                .shell
                .exec_powershell(&script)
                .await
                .map_err(|e| match e {
                    CoreError::TransportFailed { detail, .. } => self.fail(format!(
                        "cannot check the listing for completeness: {detail}"
                    )),
                    other => other,
                })?;
            let got = winps::parse_enumeration(&value, range.len()).map_err(|why| {
                self.fail(format!("cannot check the listing for completeness: {why}"))
            })?;
            for (slot, entries) in enumerated[range].iter_mut().zip(got) {
                *slot = entries;
            }
        }
        for (chain, entries) in found.roots.iter().zip(enumerated) {
            let shown_root = chain.join("/");
            let Some(entries) = entries else {
                return Err(self.fail(format!(
                    "{shown_root} was listed by sftp but the host enumeration does not find it; the tree changed while it was read"
                )));
            };
            let (collection, root_rel) = (&chain[2], chain[3..].join("/"));
            let mut sftp_files: BTreeMap<String, u64> = BTreeMap::new();
            let under = format!("{root_rel}/");
            for file in found.files.iter().filter(|f| f.collection == *collection) {
                if let Some(rest) = file.rel.strip_prefix(&under) {
                    sftp_files.insert(rest.to_string(), file.size);
                }
            }
            let sftp_dirs: BTreeSet<String> = found
                .dirs
                .iter()
                .filter(|d| d.len() > chain.len() && d.starts_with(chain.as_slice()))
                .map(|d| d[chain.len()..].join("/"))
                .collect();
            let mut host_files: BTreeMap<String, u64> = BTreeMap::new();
            let mut host_dirs: BTreeSet<String> = BTreeSet::new();
            for entry in &entries {
                let shown = format!("{shown_root}/{}", entry.rel);
                if entry.reparse {
                    return Err(self.fail(format!(
                        "{shown} is a junction or symlink (a reparse point); sftp would follow it, so a Windows read refuses it"
                    )));
                }
                let path_len = entry.full_len.saturating_sub(LONG_PATH_PREFIX.len());
                if path_len >= LONG_PATH_LIMIT {
                    return Err(self.fail(format!(
                        "{shown} has a path of {path_len} characters on the host; the sftp server cannot resolve paths of 260 characters or more, so it cannot be read"
                    )));
                }
                if entry.dir {
                    host_dirs.insert(entry.rel.clone());
                } else {
                    host_files.insert(entry.rel.clone(), entry.size);
                }
            }
            let mut missing: Vec<String> = host_files
                .keys()
                .filter(|k| !sftp_files.contains_key(*k))
                .chain(host_dirs.iter().filter(|d| !sftp_dirs.contains(*d)))
                .map(|k| format!("{shown_root}/{k}"))
                .collect();
            missing.sort();
            if !missing.is_empty() {
                return Err(self.fail(format!(
                    "the host holds entries that the sftp listing did not return (the sftp server drops paths of 260 characters or more): {}",
                    named(&missing)
                )));
            }
            let mut extra: Vec<String> = sftp_files
                .keys()
                .filter(|k| !host_files.contains_key(*k))
                .chain(sftp_dirs.iter().filter(|d| !host_dirs.contains(*d)))
                .map(|k| format!("{shown_root}/{k}"))
                .collect();
            extra.sort();
            if !extra.is_empty() {
                return Err(self.fail(format!(
                    "sftp listed entries that the host enumeration does not find; the tree changed while it was read: {}",
                    named(&extra)
                )));
            }
            let differing: Vec<String> = sftp_files
                .iter()
                .filter(|(k, size)| host_files.get(*k) != Some(*size))
                .map(|(k, _)| format!("{shown_root}/{k}"))
                .collect();
            if !differing.is_empty() {
                return Err(self.fail(format!(
                    "the host enumeration and the sftp listing disagree on the size of {}; the tree changed while it was read",
                    named(&differing)
                )));
            }
        }
        Ok(())
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
    /// Whether this is a Windows OpenSSH host (the factory test of
    /// `apply_host_for_node`).
    #[cfg(test)]
    pub(super) fn is_windows_for_tests(&self) -> bool {
        self.windows
    }

    /// The Windows half of the component gate: on a Windows host `name` must
    /// also pass [`windows_component_problem`]. `what` names it in the error.
    fn windows_gate(&self, what: &str, name: &str) -> Result<(), CoreError> {
        match self
            .windows
            .then(|| windows_component_problem(name))
            .flatten()
        {
            Some(why) => Err(self.fail(format!("{what}: {why}"))),
            None => Ok(()),
        }
    }

    fn target_chains(&self, scope: &Scope) -> Result<BTreeSet<Vec<String>>, CoreError> {
        if let Some(why) = component_problem(&scope.mode) {
            return Err(self.fail(format!("the target mode {why}")));
        }
        self.windows_gate("the target mode", &scope.mode)?;
        safe_remote_path(&scope.mode)
            .map_err(|why| self.fail(format!("the target mode is unsafe: {why}")))?;
        let mut dirs: Vec<Vec<String>> = Vec::new();
        for (module, ty) in &scope.types {
            validate_segment("module", module).map_err(|e| self.fail(e))?;
            validate_segment("type", ty).map_err(|e| self.fail(e))?;
            self.windows_gate("the module", module)?;
            self.windows_gate("the type", ty)?;
            dirs.push(vec![module.clone(), ty.clone()]);
        }
        for provider in &scope.tag_providers {
            if let Some(why) = component_problem(provider) {
                return Err(self.fail(format!("tag provider {provider:?} {why}")));
            }
            self.windows_gate("the tag provider", provider)?;
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
        let mut dirs_found: Vec<Vec<String>> = Vec::new();
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
                            EntryKind::Dir => {
                                dirs_found.push(child.clone());
                                next.push(child);
                            }
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
        // a root is not a directory "below" itself
        dirs_found.retain(|d| !roots.contains(d));
        dirs_found.sort();
        Ok(Found {
            roots,
            files,
            dirs: dirs_found,
        })
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
        self.windows_gate("the collection name", collection)?;
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
        if self.windows {
            // before any byte is downloaded: an incomplete listing fails here
            self.cross_check(&found).await?;
        }

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
        // Decision B(i): an ssh node keeps its ledger in the service account's
        // home, outside every collection and outside the data directory.
        let location = ledger_location(LedgerRoot::AccountHome, gateway, mode);
        self.read_home_file(
            &ledger_rel(gateway, mode),
            LEDGER_MAX_BYTES,
            "the ledger",
            [("target gateway", gateway), ("target mode", mode)],
            &|problem| CoreError::DeployLedgerInvalid {
                location: location.clone(),
                problem,
            },
        )
        .await
    }

    async fn read_journal(&self, gateway: &str, mode: &str) -> Result<Option<Vec<u8>>, CoreError> {
        // Under the same root as the ledger (D-04). A refusal is
        // `deploy_run_invalid` naming the location; the run id is not known
        // before the journal is parsed.
        let location = journal_location(LedgerRoot::AccountHome, gateway, mode);
        self.read_home_file(
            &journal_rel(gateway, mode),
            JOURNAL_MAX_BYTES,
            "the journal",
            [("target gateway", gateway), ("target mode", mode)],
            &|problem| CoreError::DeployRunInvalid {
                run_id: "unknown".to_string(),
                problem: format!("{location}: {problem}"),
            },
        )
        .await
    }
}

impl SftpTransport {
    /// The bytes of the regular file `rel` below the service account's home,
    /// or `None` when any level of its path is absent. Names that are not one
    /// safe component, a symlink or a non-directory on the way, a
    /// non-regular file, a file over `cap` (checked from the listing before
    /// any byte is fetched) and every read error are refused through `bad`,
    /// which builds the refusal from a value-free problem. The ledger, the
    /// journal and the release record (25-08) all read through here, so they
    /// fail closed the same way. `names` are the two path components the
    /// caller fills in (`kind` is how a refusal names each).
    async fn read_home_file(
        &self,
        rel: &str,
        cap: usize,
        what: &str,
        names: [(&str, &str); 2],
        bad: &(dyn Fn(String) -> CoreError + Sync),
    ) -> Result<Option<Vec<u8>>, CoreError> {
        for (kind, value) in names {
            if let Some(why) = component_problem(value) {
                return Err(bad(format!("the {kind} {why}")));
            }
            if let Some(why) = self
                .windows
                .then(|| windows_component_problem(value))
                .flatten()
            {
                return Err(bad(format!("the {kind}: {why}")));
            }
        }
        let parts: Vec<&str> = rel.split('/').collect();
        // An unreadable host is not an absent file: the home must answer
        // `pwd` and list.
        let home = self.account_home().await?;
        // Only the fixed names of the path are looked up in the home, so
        // names in it that cannot ride a batch line (a Windows profile holds
        // `NTUSER.DAT{<guid>}.TM.blf`) are skipped, not refused.
        let mut budget = Budget::lenient();
        let found = self
            .descend_from(&home, &parts, &mut budget)
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
            return Err(bad(format!("{what} is not a regular file")));
        }
        if entry.size > cap as u64 {
            return Err(bad(format!(
                "the file is {} bytes, over the {cap} byte cap",
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
            let result = read_downloaded(&root, Path::new("ledger"), size, cap as u64);
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
                "the file is {len} bytes, over the {cap} byte cap"
            ))),
            Err(Readback::Grew) => Err(bad(format!(
                "the file grew past the {cap} byte cap while it was read"
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
// `TreeMirror::commit` (D-01) runs the uploaded script's `mirror` op in one
// `ssh` call on either family (`sftp/apply_ops.rs`: POSIX `sh` since 23-06,
// Windows PowerShell since 23-20); there is no rename per file. `stage` still
// uploads the staged copies with sftp `put` (nothing under the scan lock), and
// the script, which checks the SHA-256 of every staged copy on the host before
// the first move, does every check, move, delete and emptied-directory removal.

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

impl SftpTransport {
    /// Bind the write half to the `mode` collection with `staging`. A
    /// `Separate` root is normalised to an absolute POSIX path here and must
    /// lie outside `config/resources`.
    pub fn with_mirror(mut self, mode: &str, staging: StagingMode) -> Result<Self, String> {
        if let Some(why) = mirror_mode_problem(mode) {
            return Err(why);
        }
        safe_remote_path(mode).map_err(|why| format!("the mode is unsafe: {why}"))?;
        if self.windows
            && let Some(why) = windows_component_problem(mode)
        {
            return Err(format!("the mode is refused: {why}"));
        }
        let resources = self.resources_display();
        validate_staging_root_for(&staging, &resources, self.windows)?;
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
                if self.windows
                    && let Some(why) = windows_absolute_path_problem(&normal)
                {
                    return Err(format!("the staging root is refused: {why}"));
                }
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

    /// On a Windows host every component of a staged or deleted path must
    /// also pass the Windows component gate; checked before any I/O.
    fn windows_paths<'a>(
        &self,
        op: &str,
        paths: impl Iterator<Item = &'a RelPath>,
    ) -> Result<(), CoreError> {
        if !self.windows {
            return Ok(());
        }
        for path in paths {
            for component in path.components() {
                if let Some(why) = windows_component_problem(component) {
                    return Err(
                        self.mirror_fail(op, format!("{path} cannot be a Windows path: {why}"))
                    );
                }
            }
        }
        Ok(())
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

    /// Run one mutating batch; an empty batch spawns nothing. Nothing is
    /// retried here: a held target is the script's business (D-01), and the
    /// uploads and directory makes of this batch have no such case.
    async fn run_batch(&self, op: &str, ops: Vec<SftpOp>) -> Result<(), CoreError> {
        if ops.is_empty() {
            return Ok(());
        }
        self.sessions.fetch_add(1, Ordering::Relaxed);
        let out = self.shell.sftp_batch(&ops).await?;
        self.shell.require_success(op, out)?;
        Ok(())
    }

    /// Where a survey of the staging `root` starts, and the components of
    /// `root` below that point.
    ///
    /// A root below the account home is surveyed from the home, which the
    /// server itself named (`pwd`) and which every read below the home
    /// already descends from. Nothing at or above the home is listed or
    /// created, so a home spelled with an 8.3 short name (the Windows runner's
    /// `C:\Users\RUNNER~1`, which the listing of `C:\Users` never shows) or one
    /// whose ancestors the account cannot list does not fail the survey. A root
    /// outside the home starts at `/<first component>` as before.
    ///
    /// The home is always asked first ([`account_home`](Self::account_home),
    /// one `@pwd` session unless cached), so the survey never depends on
    /// whether an earlier call on this instance happened to cache it (a
    /// transport fresh from `mirror_for_node` has not, WR-01). An unreadable
    /// home is `transport_failed`, before anything is listed.
    ///
    /// The home's components are compared by the host's case rule:
    /// ASCII-case-insensitively on a Windows host (as
    /// [`validate_staging_root_for`](super::validate_staging_root_for) does),
    /// exactly on a POSIX one. The base keeps the server's spelling of the
    /// home. An 8.3 short name and its long name are not resolved: that needs
    /// the host, so a root spelled with the other form than the home's (for
    /// a home `/C:/Users/RUNNER~1`, the root `/C:/Users/runneradmin/...`, or
    /// the reverse) still falls back to the `/<first component>` survey
    /// (WR-02). That fallback lists the drive root, and when the root names
    /// the short spelling it fails at `mkdir` exactly as G2 did.
    async fn survey_root(&self, op: &str, root: &str) -> Result<(String, Vec<String>), CoreError> {
        let parts = absolute_components(root).map_err(|why| self.mirror_fail(op, why))?;
        if parts.is_empty() {
            return Err(self.mirror_fail(op, "must name a directory below the filesystem root"));
        }
        let home = self.account_home().await?;
        let same = |a: &String, b: &String| {
            if self.windows {
                a.eq_ignore_ascii_case(b)
            } else {
                a == b
            }
        };
        if let Ok(home_parts) = absolute_components(&home)
            && !home_parts.is_empty()
            && parts.len() > home_parts.len()
            && parts.iter().zip(&home_parts).all(|(a, b)| same(a, b))
        {
            return Ok((home, parts[home_parts.len()..].to_vec()));
        }
        Ok((format!("/{}", parts[0]), parts[1..].to_vec()))
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
        self.windows_paths(OP, tree.files.keys())?;
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
                // the account home (asked, then listed first, so an unreadable
                // home is `transport_failed`), or `/<first>` for a root
                // outside it; either is assumed to exist and its listing
                // proves it
                let (base, rest) = self.survey_root(OP, root).await?;
                let mut targets: Vec<Vec<String>> = Vec::new();
                let mut file_chains: Vec<Vec<String>> = Vec::new();
                for (path, _) in &files {
                    let mut chain = rest.clone();
                    chain.extend(path.components().map(str::to_string));
                    file_chains.push(chain.clone());
                    targets.push(chain);
                }
                targets.push(rest.clone());
                // the staging root is below the account home, whose listing
                // may hold names a batch line cannot carry; only the fixed
                // names of the root are looked up
                let mut budget = Budget::lenient();
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

    /// Everything a mirror commit checks before any I/O on the host: the
    /// owner, the staging mode, every path (Windows gate, mirror rules, batch
    /// line) and the local staged copies, which are re-read and re-hashed.
    /// Returns the setup and each staged copy's size.
    async fn mirror_precheck(
        &self,
        local: &Path,
        files: &[(RelPath, String)],
        remote_root: Option<&str>,
        owner: &str,
        delete_set: &DeleteSet,
    ) -> Result<(&MirrorSetup, Vec<u64>), CoreError> {
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
        self.windows_paths(
            OP,
            staged_paths.iter().copied().chain(delete_set.files.iter()),
        )?;
        validate_mirror_paths(&staged_paths, delete_set)
            .and_then(|()| {
                validate_staging_root_for(&setup.staging, &self.resources_display(), self.windows)
            })
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
        Ok((setup, sizes))
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
        // D-01: a commit is one script call on either family, never a rename
        // per file.
        self.mirror_commit_script(
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
    use crate::deploy::apply::journal::JOURNAL_FILE;
    use crate::deploy::ledger::LEDGER_FILE;
    use crate::deploy::transport::batch::RENAME_RETRIES;
    use crate::deploy::transport::process::{ProcessOutput, ProcessRunner};
    use std::collections::HashSet;
    use std::ffi::{OsStr, OsString};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    const DATA: &str = "/gw/data";
    const RES: &str = "/gw/data/config/resources";
    /// The service account's home as the fake server reports it (`pwd`).
    const HOME: &str = "/home/svc";

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
        let out = format!("{PWD_MARKER}/home/u\n{out}\n");
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
            // each case sits under its marker, so it is refused for its own
            // reason and not for the missing marker
            let text = format!("{PWD_MARKER}/home/u\n{text}");
            assert!(
                parse_ls_listing(dir, text.as_bytes()).is_err(),
                "{text:?} must be refused"
            );
        }
        // the same name twice
        let twice = format!(
            "{PWD_MARKER}/home/u\n{}\n{}\n",
            line("-rw-r--r--", 1, "/d/a b,c/x"),
            line("-rw-r--r--", 2, "/d/a b,c/x")
        );
        assert!(parse_ls_listing(dir, twice.as_bytes()).is_err());
        // not UTF-8
        assert!(parse_ls_listing(dir, b"\xff\xfe").is_err());
        // an empty directory is a marker and no entry line (or only the dots)
        let marker = format!("{PWD_MARKER}/home/u\n");
        assert!(
            parse_ls_listing(dir, marker.as_bytes())
                .expect("empty")
                .is_empty()
        );
        let dots = format!(
            "{marker}{}\n{}\n",
            line("drwxr-xr-x", 1, "/d/a b,c/."),
            line("drwxr-xr-x", 1, "/d/a b,c/..")
        );
        assert!(
            parse_ls_listing(dir, dots.as_bytes())
                .expect("dots")
                .is_empty()
        );
        // a bare `.` or `..` carries no directory: not a POSIX `ls -lan` line
        for bare in [".", ".."] {
            let text = format!("{marker}{}\n", line("drwxr-xr-x", 1, bare));
            assert!(
                parse_ls_listing(dir, text.as_bytes()).is_err(),
                "a bare {bare:?} must be refused"
            );
        }
        // an output with no marker at all is not a listing (the batch always
        // leads with `@pwd`), as on Windows
        assert!(parse_ls_listing(dir, b"").is_err());
    }

    #[test]
    fn parse_batch_listing_attributes_lines_in_order() {
        let dirs = vec!["/d/a".to_string(), "/d/a/b".to_string(), "/d/c".to_string()];
        let marker = format!("{PWD_MARKER}/home/u");
        let out = [
            marker.clone(),
            line("drwxr-xr-x", 1, "/d/a/."),
            line("drwxr-xr-x", 1, "/d/a/b"),
            line("-rw-r--r--", 3, "/d/a/f"),
            marker.clone(),
            line("drwxr-xr-x", 1, "/d/a/b/."),
            line("-rw-r--r--", 4, "/d/a/b/x"),
            marker.clone(),
            line("-rw-r--r--", 5, "/d/c/y"),
        ]
        .join("\n");
        let got = parse_batch_listing(&dirs, out.as_bytes()).expect("batch");
        let names = |i: usize| -> Vec<&str> { got[i].iter().map(|e| e.name.as_str()).collect() };
        assert_eq!(names(0), ["b", "f"]);
        assert_eq!(names(1), ["x"]);
        assert_eq!(names(2), ["y"]);
        // a line under the marker of another directory is not valid
        let backwards = [
            marker.clone(),
            line("-rw-r--r--", 5, "/d/c/y"),
            marker.clone(),
            line("-rw-r--r--", 3, "/d/a/f"),
            marker,
        ]
        .join("\n");
        assert!(parse_batch_listing(&dirs, backwards.as_bytes()).is_err());
    }

    // ---- G1: the two shapes of a POSIX long listing -----------------------
    //
    // For `ls -la` the sftp CLIENT prints a line the server's way (a bare
    // name: OpenSSH 8.9 on Ubuntu 22.04) unless it formats the line itself,
    // which it does when the server offers `users-groups-by-id@openssh.com`
    // (the macOS `sftp-server`); then the listed path is in front of the
    // name. Both captures are verbatim, from `## G1: POSIX sftp listing
    // shapes` in 23-LIVE-CAPTURES.md. A bare line is byte-identical to the
    // tail of a name that held a raw `\n`, so a POSIX host is now listed with
    // `ls -lan` (the client formats every line) and the bare capture is
    // refused; the client-formatted one is still read.

    /// `pwd`, `ls -la` of the account home, `pwd`, `ls -la` of the production
    /// collection, through ign-lnx-836's own `sftp-server` (2026-10-19).
    const OPENSSH_8_9: &str = "\
Remote working directory: /home/svc-ignition-deploy\n\
drwxr-x---    4 svc-ignition-deploy svc-ignition-deploy     4096 Oct 18 23:43 .\n\
drwxr-xr-x    4 root     root         4096 Oct 18 22:52 ..\n\
-rw-r--r--    1 svc-ignition-deploy svc-ignition-deploy      220 Jan  6  2022 .bash_logout\n\
-rw-r--r--    1 svc-ignition-deploy svc-ignition-deploy     3771 Jan  6  2022 .bashrc\n\
drwx------    2 svc-ignition-deploy svc-ignition-deploy     4096 Oct 18 22:52 .cache\n\
-rw-r--r--    1 svc-ignition-deploy svc-ignition-deploy      807 Jan  6  2022 .profile\n\
drwx------    2 svc-ignition-deploy svc-ignition-deploy     4096 Oct 18 22:52 .ssh\n\
Remote working directory: /home/svc-ignition-deploy\n\
drwxrwxr-x    2 root     root         4096 Oct 18 22:59 .\n\
drwxr-xr-x    7 root     root         4096 Oct 18 22:50 ..\n\
-rw-rw-r--    1 root     root          140 Oct 18 22:59 config-mode.json\n\
";

    /// `pwd`, `ls -la` of a scratch directory, `pwd`, `ls -la` of its
    /// sub directory, through the macOS `/usr/libexec/sftp-server`.
    const MACOS_CLIENT_FORMATTED: &str = "\
Remote working directory: /private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/7dbe57f3-a4c4-4f47-88c4-a2249a4d2b80/scratchpad/g1mac\n\
drwxr-xr-x    ? pmannion wheel         128 Oct 18 18:27 /private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/7dbe57f3-a4c4-4f47-88c4-a2249a4d2b80/scratchpad/g1mac/scratch/.\n\
drwxr-xr-x    ? pmannion wheel          96 Oct 18 18:27 /private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/7dbe57f3-a4c4-4f47-88c4-a2249a4d2b80/scratchpad/g1mac/scratch/..\n\
-rw-r--r--    ? pmannion wheel           6 Oct 18 18:27 /private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/7dbe57f3-a4c4-4f47-88c4-a2249a4d2b80/scratchpad/g1mac/scratch/file.txt\n\
drwxr-xr-x    ? pmannion wheel          96 Oct 18 18:27 /private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/7dbe57f3-a4c4-4f47-88c4-a2249a4d2b80/scratchpad/g1mac/scratch/sub\n\
Remote working directory: /private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/7dbe57f3-a4c4-4f47-88c4-a2249a4d2b80/scratchpad/g1mac\n\
drwxr-xr-x    ? pmannion wheel          96 Oct 18 18:27 /private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/7dbe57f3-a4c4-4f47-88c4-a2249a4d2b80/scratchpad/g1mac/scratch/sub/.\n\
drwxr-xr-x    ? pmannion wheel         128 Oct 18 18:27 /private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/7dbe57f3-a4c4-4f47-88c4-a2249a4d2b80/scratchpad/g1mac/scratch/sub/..\n\
-rw-r--r--    ? pmannion wheel           1 Oct 18 18:27 /private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/7dbe57f3-a4c4-4f47-88c4-a2249a4d2b80/scratchpad/g1mac/scratch/sub/inner.json\n\
";

    const MAC_DIR: &str = "/private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/7dbe57f3-a4c4-4f47-88c4-a2249a4d2b80/scratchpad/g1mac/scratch";

    fn shown(entries: &[ListedEntry]) -> Vec<(&str, EntryKind, u64)> {
        entries
            .iter()
            .map(|e| (e.name.as_str(), e.kind, e.size))
            .collect()
    }

    #[test]
    fn posix_listing_refuses_captured_bare_openssh_8_9_output() {
        // the bare `ls -la` shape carries no directory on any line, so a line
        // cannot be told from the tail of a name split by a raw `\n`
        let dirs = vec![
            "/home/svc-ignition-deploy".to_string(),
            "/usr/local/bin/ignition/data/config/resources/production".to_string(),
        ];
        for rule in [NameRule::Refuse, NameRule::Skip] {
            let got =
                parse_batch_listing_as(ListFormat::Posix, rule, &dirs, OPENSSH_8_9.as_bytes(), 100);
            assert!(
                matches!(&got, Err(ListingError::Bad(why)) if why.contains("does not carry the listed directory")),
                "{rule:?}: {got:?}"
            );
        }
    }

    /// `pwd`, `ls -lan` of the same two directories through the same
    /// ign-lnx-836 OpenSSH 8.9 `sftp-server` (D3 of the raw-newline fix,
    /// 2026-10-19, `lnxvm_listing_capture`, macOS OpenSSH_10.3p1 client): with
    /// `-n` the client formats every line itself, numeric owner and group and
    /// the listed path in front of the name, though the server offers no
    /// users-groups extension.
    const OPENSSH_8_9_LAN: &str = "\
Remote working directory: /home/svc-ignition-deploy\n\
drwxr-x---    ? 1001     1001         4096 Oct 19 00:59 /home/svc-ignition-deploy/.\n\
drwxr-xr-x    ? 0        0            4096 Oct 18 16:52 /home/svc-ignition-deploy/..\n\
-rw-r--r--    ? 1001     1001          220 Jan  6  2022 /home/svc-ignition-deploy/.bash_logout\n\
-rw-r--r--    ? 1001     1001         3771 Jan  6  2022 /home/svc-ignition-deploy/.bashrc\n\
drwx------    ? 1001     1001         4096 Oct 18 16:52 /home/svc-ignition-deploy/.cache\n\
-rw-r--r--    ? 1001     1001          807 Jan  6  2022 /home/svc-ignition-deploy/.profile\n\
drwx------    ? 1001     1001         4096 Oct 18 16:52 /home/svc-ignition-deploy/.ssh\n\
Remote working directory: /home/svc-ignition-deploy\n\
drwxrwxr-x    ? 0        0            4096 Oct 18 20:40 /usr/local/bin/ignition/data/config/resources/production/.\n\
drwxr-xr-x    ? 0        0            4096 Oct 18 16:50 /usr/local/bin/ignition/data/config/resources/production/..\n\
-rw-rw-r--    ? 0        0             140 Oct 18 16:59 /usr/local/bin/ignition/data/config/resources/production/config-mode.json\n\
";

    #[test]
    fn posix_listing_parses_captured_ls_lan_output_from_openssh_8_9() {
        let dirs = vec![
            "/home/svc-ignition-deploy".to_string(),
            "/usr/local/bin/ignition/data/config/resources/production".to_string(),
        ];
        let got =
            parse_batch_listing(&dirs, OPENSSH_8_9_LAN.as_bytes()).expect("the 8.9 -lan capture");
        assert_eq!(
            shown(&got[0]),
            [
                (".bash_logout", EntryKind::File, 220),
                (".bashrc", EntryKind::File, 3771),
                (".cache", EntryKind::Dir, 4096),
                (".profile", EntryKind::File, 807),
                (".ssh", EntryKind::Dir, 4096),
            ],
            "the entries of the bare capture, dots dropped, a year column read as a date"
        );
        assert_eq!(shown(&got[1]), [("config-mode.json", EntryKind::File, 140)]);
    }

    #[test]
    fn posix_listing_parses_captured_client_formatted_output() {
        let dirs = vec![MAC_DIR.to_string(), format!("{MAC_DIR}/sub")];
        let got = parse_batch_listing(&dirs, MACOS_CLIENT_FORMATTED.as_bytes())
            .expect("the macOS capture");
        assert_eq!(
            shown(&got[0]),
            [
                ("file.txt", EntryKind::File, 6),
                ("sub", EntryKind::Dir, 96)
            ]
        );
        assert_eq!(shown(&got[1]), [("inner.json", EntryKind::File, 1)]);
    }

    #[test]
    fn posix_listing_refuses_a_bare_line_and_windows_takes_either_shape() {
        let dirs = vec!["/d/bare".to_string(), "/d/prefixed".to_string()];
        let text = format!(
            "{PWD_MARKER}/home/u\n{}\n{}\n{PWD_MARKER}/home/u\n{}\n{}\n",
            line("drwxr-xr-x", 1, "."),
            line("-rw-r--r--", 3, "plain name"),
            line("drwxr-xr-x", 1, "/d/prefixed/."),
            line("-rw-r--r--", 4, "/d/prefixed/other name"),
        );
        // POSIX (`ls -lan`): every entry line carries its directory
        for rule in [NameRule::Refuse, NameRule::Skip] {
            let got = parse_batch_listing_as(ListFormat::Posix, rule, &dirs, text.as_bytes(), 100);
            assert!(
                matches!(got, Err(ListingError::Bad(_))),
                "{rule:?}: {got:?}"
            );
        }
        // the control: the same entries, every line prefixed
        let prefixed = format!(
            "{PWD_MARKER}/home/u\n{}\n{}\n{PWD_MARKER}/home/u\n{}\n{}\n",
            line("drwxr-xr-x", 1, "/d/bare/."),
            line("-rw-r--r--", 3, "/d/bare/plain name"),
            line("drwxr-xr-x", 1, "/d/prefixed/."),
            line("-rw-r--r--", 4, "/d/prefixed/other name"),
        );
        let got = parse_batch_listing(&dirs, prefixed.as_bytes()).expect("prefixed");
        assert_eq!(shown(&got[0]), [("plain name", EntryKind::File, 3)]);
        assert_eq!(shown(&got[1]), [("other name", EntryKind::File, 4)]);
        // Windows (`ls -la`, bare server names) still reads either shape
        let got = parse_batch_listing_as(
            ListFormat::Windows,
            NameRule::Refuse,
            &dirs,
            text.as_bytes(),
            100,
        )
        .expect("mixed shapes on Windows");
        assert_eq!(shown(&got[0]), [("plain name", EntryKind::File, 3)]);
        assert_eq!(shown(&got[1]), [("other name", EntryKind::File, 4)]);
    }

    #[test]
    fn posix_listing_refuses_a_prefix_of_another_directory() {
        let dirs = vec!["/d/one".to_string(), "/d/two".to_string()];
        let marker = format!("{PWD_MARKER}/home/u\n");
        let one = line("-rw-r--r--", 1, "/d/one/x");
        let two = line("-rw-r--r--", 1, "/d/two/y");
        // the control: each line under its own marker
        let ok = format!("{marker}{one}\n{marker}{two}\n");
        assert!(parse_batch_listing(&dirs, ok.as_bytes()).is_ok());
        for (why, text) in [
            (
                "directory one's prefix under marker two",
                format!("{marker}{one}\n{marker}{one}\n"),
            ),
            (
                "directory two's prefix under marker one",
                format!("{marker}{two}\n{marker}{two}\n"),
            ),
            (
                "a name with a slash that is not the current prefix",
                format!(
                    "{marker}{one}\n{marker}{}\n",
                    line("-rw-r--r--", 1, "/elsewhere/y")
                ),
            ),
            (
                "a nested path under the parent's marker",
                format!(
                    "{marker}{}\n{marker}{two}\n",
                    line("-rw-r--r--", 1, "/d/one/sub/z")
                ),
            ),
            ("fewer markers than directories", format!("{marker}{one}\n")),
            (
                "an extra marker line",
                format!("{marker}{one}\n{marker}{two}\n{marker}"),
            ),
            (
                "a line before the first marker",
                format!("{one}\n{marker}{two}\n"),
            ),
            ("no marker at all", format!("{one}\n{two}\n")),
        ] {
            assert!(
                parse_batch_listing(&dirs, text.as_bytes()).is_err(),
                "{why}: must be refused"
            );
        }
    }

    #[test]
    fn bare_listing_with_a_spaced_owner_is_refused_not_misread() {
        // a bare line (the server's longname) is read only for a Windows host
        let dirs = vec!["/d".to_string()];
        let one = |text: &str| format!("{PWD_MARKER}/home/u\n{text}\n");
        let bare = |text: &str| {
            parse_batch_listing_as(
                ListFormat::Windows,
                NameRule::Refuse,
                &dirs,
                one(text).as_bytes(),
                100,
            )
        };
        // the control: a plain owner and group
        let control = "drwxr-x--- 4 svc users 4096 Oct 18 23:43 name";
        assert!(bare(control).is_ok());
        // the same bare line from a POSIX host is refused, not read
        assert!(parse_batch_listing(&dirs, one(control).as_bytes()).is_err());
        for text in [
            // an owner with a space: the day column would be a month
            "drwxr-x--- 4 svc domain users 4096 Oct 18 23:43 name",
            // a group with a space and a number in it
            "drwxr-x--- 4 svc domain 123 4096 Oct 18 23:43 name",
            // a day that is not a day, a time that is not a time
            "drwxr-x--- 4 svc users 4096 Oct 99 23:43 name",
            "drwxr-x--- 4 svc users 4096 Oct 18 noon name",
            "drwxr-x--- 4 svc users 4096 Oct 18 23:4 name",
            "drwxr-x--- 4 svc users 4096 Oct 18 202 name",
        ] {
            assert!(bare(text).is_err(), "{text:?} must be refused");
        }
        // both date forms of `ls` are read: a time, and a year, bare (Windows)
        // and with the directory in front of the name (POSIX)
        for text in [
            "-rw-r--r-- 1 svc users 4096 Oct  8 23:43 name",
            "-rw-r--r-- 1 svc users 4096 Jan  6  2022 name",
        ] {
            assert!(bare(text).is_ok(), "{text:?} must be read");
            let prefixed = text.replace(" name", " /d/name");
            assert!(
                parse_batch_listing(&dirs, one(&prefixed).as_bytes()).is_ok(),
                "{prefixed:?} must be read"
            );
        }
    }

    #[test]
    fn posix_listing_keeps_windows_only_rules_off() {
        let one = vec!["/d".to_string()];
        let posix = |name: &str| {
            format!("{PWD_MARKER}/home/u\n-rw-r--r--    ? 501 20   8 Oct 15 02:35 /d/{name}\n")
        };
        for name in ["a:b", "CON", "trail.", "trail ", "aux.txt"] {
            let got = parse_batch_listing(&one, posix(name).as_bytes())
                .unwrap_or_else(|e| panic!("{name:?} is an ordinary POSIX name: {e}"));
            assert_eq!(got[0].len(), 1, "{name:?}");
        }
        let both = format!(
            "{PWD_MARKER}/home/u\n-rw-r--r--    ? 501 20   8 Oct 15 02:35 /d/A\n-rw-r--r--    ? 501 20   8 Oct 15 02:35 /d/a\n"
        );
        assert_eq!(
            parse_batch_listing(&one, both.as_bytes()).expect("A and a")[0].len(),
            2,
            "POSIX names are case-sensitive"
        );
        // the same names under the Windows family are still refused
        for name in ["a:b", "CON", "trail."] {
            let text =
                format!("{PWD_MARKER}/C:/h\n-rw-******    1 -        -   8 Oct 15 02:35 {name}\n");
            assert!(
                parse_batch_listing_as(
                    ListFormat::Windows,
                    NameRule::Refuse,
                    &one,
                    text.as_bytes(),
                    100
                )
                .is_err(),
                "{name:?} must be refused on Windows"
            );
        }
        assert!(
            parse_batch_listing_as(
                ListFormat::Windows,
                NameRule::Refuse,
                &one,
                both.as_bytes(),
                100
            )
            .is_err(),
            "A and a are one name on Windows"
        );
    }

    // ---- a raw newline or carriage return inside a listed name --------------
    //
    // Every OpenSSH sftp client prints `\n`, `\r` and `\t` inside a name raw
    // (openssh-portable utf8.c, 8.9 through 10.3): a name holding `\n`
    // reaches the parser as two lines, and a name ending in `\r` as a line
    // ending in CRLF. The captures are from the OpenSSH_10.3p1 client over a
    // directory holding `plain` and `x\n<FORGED>`, and over one holding only
    // `y\r`, in three shapes: the server's bare longname, the line the client
    // formats, and `ls -lan`. The listed directory is normalised to `/d/nl`
    // and `/d/cr`, the `pwd` path to `/home/u`. A listing that cannot be read
    // exactly is refused (an unsafe but whole name is left out under Skip),
    // never read as `fake`, `x` or `y`.

    /// The text after the raw `\n` of the split name: a well-formed line.
    const FORGED: &str = "-rw-r--r--    1 root     root            5 Oct 18 22:52 fake";

    const NL_BARE: &str = "\
Remote working directory: /home/u\n\
drwxr-xr-x    4 pmannion wheel         128 Oct 19 04:26 .\n\
drwxr-xr-x    8 pmannion wheel         256 Oct 19 04:26 ..\n\
-rw-r--r--    1 pmannion wheel           2 Oct 19 04:26 plain\n\
-rw-r--r--    1 pmannion wheel           2 Oct 19 04:26 x\n\
-rw-r--r--    1 root     root            5 Oct 18 22:52 fake\n\
";

    const NL_CLIENT: &str = "\
Remote working directory: /home/u\n\
drwxr-xr-x    ? pmannion wheel         128 Oct 19 04:26 /d/nl/.\n\
drwxr-xr-x    ? pmannion wheel         288 Oct 19 04:26 /d/nl/..\n\
-rw-r--r--    ? pmannion wheel           2 Oct 19 04:26 /d/nl/plain\n\
-rw-r--r--    ? pmannion wheel           2 Oct 19 04:26 /d/nl/x\n\
-rw-r--r--    1 root     root            5 Oct 18 22:52 fake\n\
";

    const NL_LAN: &str = "\
Remote working directory: /home/u\n\
drwxr-xr-x    ? 501      0             128 Oct 19 04:26 /d/nl/.\n\
drwxr-xr-x    ? 501      0             320 Oct 19 04:26 /d/nl/..\n\
-rw-r--r--    ? 501      0               2 Oct 19 04:26 /d/nl/plain\n\
-rw-r--r--    ? 501      0               2 Oct 19 04:26 /d/nl/x\n\
-rw-r--r--    1 root     root            5 Oct 18 22:52 fake\n\
";

    const CR_BARE: &str = "\
Remote working directory: /home/u\n\
drwxr-xr-x    3 pmannion wheel          96 Oct 19 04:26 .\n\
drwxr-xr-x   13 pmannion wheel         416 Oct 19 04:26 ..\n\
-rw-r--r--    1 pmannion wheel           2 Oct 19 04:26 y\r\n\
";

    const CR_CLIENT: &str = "\
Remote working directory: /home/u\n\
drwxr-xr-x    ? pmannion wheel          96 Oct 19 04:26 /d/cr/.\n\
drwxr-xr-x    ? pmannion wheel         448 Oct 19 04:26 /d/cr/..\n\
-rw-r--r--    ? pmannion wheel           2 Oct 19 04:26 /d/cr/y\r\n\
";

    const CR_LAN: &str = "\
Remote working directory: /home/u\n\
drwxr-xr-x    ? 501      0              96 Oct 19 04:26 /d/cr/.\n\
drwxr-xr-x    ? 501      0             480 Oct 19 04:26 /d/cr/..\n\
-rw-r--r--    ? 501      0               2 Oct 19 04:26 /d/cr/y\r\n\
";

    /// One line the way the OpenSSH client formats `ls -lan`: numeric owner
    /// and group, the listed directory in front of the name.
    fn lan(mode: &str, size: u64, path: &str) -> String {
        format!("{mode}    ? 501      0      {size:>10} Oct 19 04:26 {path}")
    }

    fn names_of(entries: &[ListedEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn posix_listing_refuses_a_name_split_by_a_raw_newline() {
        let dirs = vec!["/d/nl".to_string()];
        // every case is tried, so a failure names each one that was misread
        let mut misread = Vec::new();
        for (shape, text) in [
            ("bare", NL_BARE),
            ("client-formatted", NL_CLIENT),
            ("ls -lan", NL_LAN),
        ] {
            assert!(
                text.ends_with(&format!("/x\n{FORGED}\n"))
                    || text.ends_with(&format!(" x\n{FORGED}\n"))
            );
            for rule in [NameRule::Refuse, NameRule::Skip] {
                let got =
                    parse_batch_listing_as(ListFormat::Posix, rule, &dirs, text.as_bytes(), 100);
                if !matches!(got, Err(ListingError::Bad(_))) {
                    misread.push(format!("{shape}, {rule:?}: {got:?}"));
                }
            }
        }
        assert!(
            misread.is_empty(),
            "the directory holds `plain` and `x\\n{FORGED}`, so every listing must be refused; these were not:\n{}",
            misread.join("\n")
        );
    }

    #[test]
    fn posix_listing_refuses_a_name_ending_in_a_carriage_return() {
        let dirs = vec!["/d/cr".to_string()];
        let mut misread = Vec::new();
        for (shape, text) in [
            ("bare", CR_BARE),
            ("client-formatted", CR_CLIENT),
            ("ls -lan", CR_LAN),
        ] {
            let got = parse_batch_listing_as(
                ListFormat::Posix,
                NameRule::Refuse,
                &dirs,
                text.as_bytes(),
                100,
            );
            if !matches!(got, Err(ListingError::Bad(_))) {
                misread.push(format!("{shape}, Refuse (must be refused): {got:?}"));
            }
        }
        // under Skip the unsafe name is left out, never read as `y`
        for (shape, text) in [("client-formatted", CR_CLIENT), ("ls -lan", CR_LAN)] {
            let got = parse_batch_listing_as(
                ListFormat::Posix,
                NameRule::Skip,
                &dirs,
                text.as_bytes(),
                100,
            );
            if got != Ok(vec![Vec::new()]) {
                misread.push(format!("{shape}, Skip (must leave `y\\r` out): {got:?}"));
            }
        }
        assert!(
            misread.is_empty(),
            "the only entry is `y\\r`, an unsafe name; these listings misread it:\n{}",
            misread.join("\n")
        );
    }

    #[test]
    fn posix_listing_refuses_names_next_to_a_raw_newline() {
        let dirs = vec!["/d/nl".to_string()];
        let head = format!(
            "{PWD_MARKER}/home/u\n{}\n{}\n",
            lan("drwxr-xr-x", 128, "/d/nl/."),
            lan("drwxr-xr-x", 320, "/d/nl/..")
        );
        let x = lan("-rw-r--r--", 2, "/d/nl/x");
        let z = lan("-rw-r--r--", 2, "/d/nl/z");
        let empty = lan("-rw-r--r--", 2, "/d/nl/");
        let mut misread = Vec::new();
        for (why, body) in [
            // `x\n` before another entry: an empty line between them
            ("`x\\n` before another entry", format!("{x}\n\n{z}\n")),
            // `x\n` as the last entry: the output ends in an empty line
            ("`x\\n` as the last entry", format!("{x}\n\n")),
            // `\nfake`: an empty name after the prefix, then a bare fragment
            ("`\\nfake`", format!("{empty}\nfake\n")),
            // `\n<FORGED>`: an empty name, then a well-formed line
            ("`\\n<forged line>`", format!("{empty}\n{FORGED}\n")),
            // `x\r\n<FORGED>`: a CRLF inside the name, then a well-formed line
            ("`x\\r\\n<forged line>`", format!("{x}\r\n{FORGED}\n")),
        ] {
            let text = format!("{head}{body}");
            for rule in [NameRule::Refuse, NameRule::Skip] {
                let got =
                    parse_batch_listing_as(ListFormat::Posix, rule, &dirs, text.as_bytes(), 100);
                if !matches!(got, Err(ListingError::Bad(_))) {
                    misread.push(format!("{why}, {rule:?}: {got:?}"));
                }
            }
        }
        assert!(
            misread.is_empty(),
            "every listing must be refused; these were not:\n{}",
            misread.join("\n")
        );
    }

    #[test]
    fn posix_listing_strips_a_carriage_return_only_when_every_line_ends_in_crlf() {
        let dirs = vec!["/d/nl".to_string()];
        let read = |rule: NameRule, text: &str| {
            parse_batch_listing_as(ListFormat::Posix, rule, &dirs, text.as_bytes(), 100)
        };
        let lines = [
            format!("{PWD_MARKER}/home/u"),
            lan("drwxr-xr-x", 128, "/d/nl/."),
            lan("drwxr-xr-x", 320, "/d/nl/.."),
            lan("-rw-r--r--", 2, "/d/nl/plain"),
            lan("-rw-r--r--", 2, "/d/nl/x"),
        ];
        // the controls: every line ends in LF, or every line (the client's
        // marker included) ends in CRLF and one CR is stripped from each
        let lf = format!("{}\n", lines.join("\n"));
        let crlf = format!("{}\r\n", lines.join("\r\n"));
        for (ends, text) in [("LF", &lf), ("CRLF", &crlf)] {
            let got = read(NameRule::Refuse, text)
                .unwrap_or_else(|e| panic!("a {ends}-uniform listing: {e}"));
            assert_eq!(names_of(&got[0]), ["plain", "x"], "{ends}");
        }
        // a CRLF-uniform listing keeps a name's own CR: refused, or left out
        let cr_name = format!("{crlf}{}\r\n", lan("-rw-r--r--", 2, "/d/nl/y\r"));
        let got = read(NameRule::Refuse, &cr_name);
        assert!(matches!(got, Err(ListingError::Bad(_))), "{got:?}");
        let got = read(NameRule::Skip, &cr_name).expect("skip");
        assert_eq!(names_of(&got[0]), ["plain", "x"]);
        let mut misread = Vec::new();
        // mixed: the marker ends in LF and every entry line in CRLF, so no CR
        // is stripped and every name keeps one
        let mixed = format!("{}\n{}\r\n", lines[0], lines[1..].join("\r\n"));
        let got = read(NameRule::Refuse, &mixed);
        if !matches!(got, Err(ListingError::Bad(_))) {
            misread.push(format!(
                "LF marker, CRLF entries, Refuse (must be refused): {got:?}"
            ));
        }
        let got = read(NameRule::Skip, &mixed);
        if got != Ok(vec![Vec::new()]) {
            misread.push(format!(
                "LF marker, CRLF entries, Skip (every name keeps its CR and is left out): {got:?}"
            ));
        }
        // a CRLF client listing a name split by a raw newline: the line before
        // the split ends in LF alone
        let split = format!(
            "{}\r\n{}\n{FORGED}\r\n",
            lines[..4].join("\r\n"),
            lan("-rw-r--r--", 2, "/d/nl/x")
        );
        for rule in [NameRule::Refuse, NameRule::Skip] {
            let got = read(rule, &split);
            if !matches!(got, Err(ListingError::Bad(_))) {
                misread.push(format!(
                    "CRLF listing of `x\\n<forged line>`, {rule:?} (must be refused): {got:?}"
                ));
            }
        }
        assert!(
            misread.is_empty(),
            "a listing whose line ends are not uniform was read:\n{}",
            misread.join("\n")
        );
    }

    #[test]
    fn windows_listing_strips_a_carriage_return_only_when_every_line_ends_in_crlf() {
        let dirs = vec!["/C:/d".to_string()];
        let read = |text: &str| {
            parse_batch_listing_as(
                ListFormat::Windows,
                NameRule::Refuse,
                &dirs,
                text.as_bytes(),
                100,
            )
        };
        let lines = [
            format!("{PWD_MARKER}/C:/Users/svc"),
            "-rw-******    1 -        -               8 Oct 15 02:35 a.json".to_string(),
            "drwx******    1 -        -               0 Oct 15 02:35 sub".to_string(),
        ];
        // the controls: LF-uniform and CRLF-uniform output
        for (ends, text) in [
            ("LF", format!("{}\n", lines.join("\n"))),
            ("CRLF", format!("{}\r\n", lines.join("\r\n"))),
        ] {
            let got = read(&text).unwrap_or_else(|e| panic!("a {ends}-uniform listing: {e}"));
            assert_eq!(names_of(&got[0]), ["a.json", "sub"], "{ends}");
        }
        // mixed: the marker ends in LF and the entry lines in CRLF
        let mixed = format!("{}\n{}\r\n", lines[0], lines[1..].join("\r\n"));
        let got = read(&mixed);
        assert!(
            matches!(got, Err(ListingError::Bad(_))),
            "a marker in LF and entries in CRLF must be refused, got {got:?}"
        );
    }

    #[tokio::test]
    async fn a_posix_host_is_listed_with_ls_lan_and_a_windows_host_with_ls_la() {
        // Windows keeps bare `ls -la` (a Win32 name cannot hold `\n` or `\r`)
        let wfake = windows_fake();
        windows_transport(&wfake)
            .list_level(&[WDATA.to_string()], &mut Budget::default())
            .await
            .expect("windows listing");
        assert_eq!(
            wfake.session_lines(),
            [vec!["@pwd".to_string(), format!("@ls -la \"{WDATA}\"")]]
        );
        // a POSIX host is listed with `ls -lan`: the client then formats every
        // line itself, with the listed directory in front of the name
        let fake = FakeSftp::new();
        transport(&fake)
            .list_level(&[DATA.to_string()], &mut Budget::default())
            .await
            .expect("posix listing");
        assert_eq!(
            fake.session_lines(),
            [vec!["@pwd".to_string(), format!("@ls -lan \"{DATA}\"")]]
        );
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
        /// What `pwd` answers (default [`HOME`]).
        home: Option<String>,
        /// Behave like Win32-OpenSSH: long listings print bare names (no
        /// `.`/`..`, `******` permission bits) and `ssh` runs PowerShell.
        windows: bool,
        /// Behave like OpenSSH 8.9's `sftp-server` (no users-groups
        /// extension): for `ls -la` the client prints the server's bare
        /// names, `.` and `..` included, with no directory in front of them
        /// (G1). It changes nothing for `ls -lan`, whose lines the client
        /// always formats itself.
        bare_posix: bool,
        /// Paths the listing omits though they exist (the 260 character
        /// silent omission, C7a-8).
        hidden: Vec<String>,
        /// Paths the enumeration reports as reparse points.
        reparse: Vec<String>,
        /// The enumeration fails with this exit code and CLIXML.
        enum_fail: Option<i32>,
        /// Extra entries the enumeration reports: (path, dir, size).
        enum_extra: Vec<(String, bool, u64)>,
        /// Report this absolute path length for every enumerated entry.
        enum_len: Option<usize>,
        /// A `rename` whose target ends with `.0` fails with `.1` this many
        /// more times: `Failure` or `Permission denied` per `.2`.
        rename_fails: Vec<(String, usize, &'static str)>,
        /// Every `ssh` argv seen.
        ssh_calls: usize,
        /// An 8.3 alias: `(short, long)`. Every path the client names under
        /// `short` is the same directory as under `long`, and a listing shows
        /// only the stored long names, as Win32-OpenSSH does (G2).
        alias: Option<(String, String)>,
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
                listed: Mutex::new(HashSet::from([DATA.to_string(), HOME.to_string()])),
                sessions: Mutex::new(Vec::new()),
                knobs: Mutex::new(Knobs::default()),
                downloads: Mutex::new(Vec::new()),
            };
            fake.mkdirs(DATA);
            fake.mkdirs(HOME);
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

        /// The stored path a client-typed `path` names (the 8.3 alias knob).
        fn resolve(&self, path: &str) -> String {
            if let Some((short, long)) = &self.knobs().alias
                && let Some(rest) = path.strip_prefix(short.as_str())
                && (rest.is_empty() || rest.starts_with('/'))
            {
                return format!("{long}{rest}");
            }
            path.to_string()
        }

        /// Apply one mutating batch line; `Err((class, stderr text))`.
        fn mutate(&self, l: &str, typed: &[String]) -> Result<(), (&'static str, String)> {
            let resolved: Vec<String> = typed.iter().map(|a| self.resolve(a)).collect();
            let args = &resolved[..];
            let parent_is_dir = |fs: &std::collections::BTreeMap<String, Node>, path: &str| {
                path.rsplit_once('/')
                    .is_some_and(|(parent, _)| matches!(fs.get(parent), Some(Node::Dir)))
            };
            let mut fs = self.fs.lock().expect("fs");
            if l.starts_with("mkdir ") {
                let path = &args[0];
                let shown = &typed[0];
                if !parent_is_dir(&fs, path) {
                    return Err((
                        "not_found",
                        format!("remote mkdir \"{shown}\": No such file or directory"),
                    ));
                }
                if fs.contains_key(path) {
                    return Err(("failure", format!("remote mkdir \"{shown}\": Failure")));
                }
                fs.insert(path.clone(), Node::Dir);
            } else if l.starts_with("put ") {
                let (local, path) = (&args[0], &args[1]);
                if !parent_is_dir(&fs, path) || matches!(fs.get(path), Some(Node::Dir)) {
                    return Err((
                        "not_found",
                        format!("dest open \"{path}\": No such file or directory"),
                    ));
                }
                let bytes = std::fs::read(local).expect("local put source");
                fs.insert(path.clone(), Node::File(bytes));
            } else if l.starts_with("rename ") {
                let (from, to) = (&args[0], &args[1]);
                assert_ne!(
                    from.to_ascii_lowercase(),
                    to.to_ascii_lowercase(),
                    "a rename onto itself reached the server: {l}"
                );
                {
                    let mut knobs = self.knobs();
                    if let Some(entry) = knobs
                        .rename_fails
                        .iter_mut()
                        .find(|(suffix, left, _)| *left > 0 && to.ends_with(suffix.as_str()))
                    {
                        entry.1 -= 1;
                        return Err((
                            "failure",
                            format!("remote rename \"{from}\" to \"{to}\": {}", entry.2),
                        ));
                    }
                }
                let Some(node) = fs.get(from).cloned() else {
                    return Err((
                        "not_found",
                        format!("remote rename \"{from}\" to \"{to}\": No such file or directory"),
                    ));
                };
                if !parent_is_dir(&fs, to) {
                    return Err((
                        "not_found",
                        format!("remote rename \"{from}\" to \"{to}\": No such file or directory"),
                    ));
                }
                if matches!(fs.get(to), Some(Node::Dir)) {
                    return Err((
                        "failure",
                        format!("remote rename \"{from}\" to \"{to}\": Failure"),
                    ));
                }
                fs.remove(from);
                fs.insert(to.clone(), node);
            } else if l.starts_with("rm ") {
                let path = &args[0];
                if !matches!(fs.get(path), Some(Node::File(_))) {
                    return Err((
                        "not_found",
                        format!("remote delete {path}: No such file or directory"),
                    ));
                }
                fs.remove(path);
            } else if l.starts_with("rmdir ") {
                let path = &args[0];
                let prefix = format!("{path}/");
                if fs.keys().any(|k| k.starts_with(&prefix)) {
                    return Err(("failure", format!("remote rmdir \"{path}\": Failure")));
                }
                fs.remove(path);
            } else {
                panic!("unexpected mutating line {l}");
            }
            Ok(())
        }

        /// `ssh <opts> -- user@host powershell -EncodedCommand <b64>`: decode
        /// the script, take the roots from its parameter blob and answer the
        /// ENUMERATE script from the tree (or fail as a PowerShell would).
        fn exec(&self, args: &[OsString]) -> ProcessOutput {
            use base64::Engine as _;
            self.knobs().ssh_calls += 1;
            if let Some(code) = self.knobs().enum_fail {
                return ProcessOutput {
                    stdout: Vec::new(),
                    stderr: b"#< CLIXML\r\n<Objs><S S=\"Error\">Access to the path is denied._x000D__x000A_</S></Objs>"
                        .to_vec(),
                    code,
                    timed_out: false,
                };
            }
            let last = args
                .last()
                .expect("remote command")
                .to_string_lossy()
                .into_owned();
            let b64 = last
                .strip_prefix("powershell -NoProfile -NonInteractive -EncodedCommand ")
                .expect("an encoded powershell command");
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .expect("base64");
            let units: Vec<u16> = bytes
                .chunks(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            let script = String::from_utf16(&units).expect("utf-16");
            let blob = script
                .split("FromBase64String('")
                .nth(1)
                .and_then(|r| r.split('\'').next())
                .expect("parameter blob");
            let params: serde_json::Value = serde_json::from_slice(
                &base64::engine::general_purpose::STANDARD
                    .decode(blob)
                    .expect("blob"),
            )
            .expect("params json");
            let prefix = params["x"].as_str().expect("prefix").to_string();
            assert_eq!(prefix, "\\\\?\\", "the long path prefix");
            let knobs_len = self.knobs().enum_len;
            let reparse: Vec<String> = self.knobs().reparse.clone();
            let extra = self.knobs().enum_extra.clone();
            let fs = self.fs.lock().expect("fs");
            let mut res = Vec::new();
            for (i, root) in params["r"].as_array().expect("roots").iter().enumerate() {
                let native = root.as_str().expect("root");
                assert!(
                    native.contains('\\') && !native.starts_with('/'),
                    "{native}"
                );
                let fake_root = format!("/{}", native.replace('\\', "/"));
                if !matches!(fs.get(&fake_root), Some(Node::Dir)) {
                    res.push(serde_json::json!({ "i": i, "x": 0, "e": [] }));
                    continue;
                }
                let below = format!("{fake_root}/");
                let mut entries = Vec::new();
                for (path, node) in fs.iter() {
                    let Some(rel) = path.strip_prefix(&below) else {
                        continue;
                    };
                    let (dir, size) = match node {
                        Node::Dir => (1, 0),
                        Node::File(b) => (0, b.len() as u64),
                        _ => (0, 0),
                    };
                    let flagged = i32::from(reparse.iter().any(|r| r == path));
                    let full = knobs_len.unwrap_or(native.len() + 1 + rel.len() + prefix.len());
                    entries.push(serde_json::json!([
                        rel.replace('/', "\\"),
                        dir,
                        size,
                        flagged,
                        full
                    ]));
                }
                for (path, dir, size) in &extra {
                    if let Some(rel) = path.strip_prefix(&below) {
                        entries.push(serde_json::json!([rel, i32::from(*dir), size, 0, 100]));
                    }
                }
                res.push(serde_json::json!({ "i": i, "x": 1, "e": entries }));
            }
            let json = serde_json::json!({ "ok": 1, "res": res });
            ProcessOutput {
                stdout: format!("IGN-RESULT:{json}\r\n").into_bytes(),
                stderr: Vec::new(),
                code: 0,
                timed_out: false,
            }
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
            if program == "ssh" {
                return self.exec(_args);
            }
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
                if l == "@pwd" {
                    let home = self
                        .knobs()
                        .home
                        .clone()
                        .unwrap_or_else(|| HOME.to_string());
                    stdout.push_str(&format!("Remote working directory: {home}\n"));
                    continue;
                }
                let args = quoted(l);
                let remote = args.first().expect("a path").clone();
                let mutating = ["mkdir ", "put ", "rename ", "rm ", "rmdir "]
                    .iter()
                    .any(|c| l.starts_with(c));
                if !mutating {
                    assert!(
                        self.listed.lock().expect("listed").contains(&remote),
                        "{remote} is referenced before an earlier session listed it"
                    );
                } else {
                    stdout.push_str(&format!("sftp> {l}\n"));
                    if let Err((class, text)) = self.mutate(l, &args) {
                        stderr.push_str(&format!("{text}\n"));
                        let _ = class;
                        code = 1;
                        break;
                    }
                    // a path the client just created is known to it
                    if let Some(made) = args.last() {
                        newly_listed.push(made.clone());
                    }
                    continue;
                }
                let numeric = l.starts_with("@ls -lan \"");
                if numeric || l.starts_with("@ls -la \"") {
                    let stored = self.resolve(&remote);
                    let node = self.fs.lock().expect("fs").get(&stored).cloned();
                    if !matches!(node, Some(Node::Dir)) {
                        stderr.push_str(&format!("Can't ls: \"{remote}\" not found\n"));
                        code = 1;
                        break;
                    }
                    let windows = self.knobs().windows;
                    // `-n` makes the client format every line itself, the
                    // listed path in front of the name and the owner and
                    // group numeric, whatever the server. Without it the
                    // client prints the server's bare longname when the
                    // server lacks the users-groups extension: Win32-OpenSSH,
                    // and OpenSSH 8.9 (`bare_posix`, G1).
                    let bare = !numeric && (windows || self.knobs().bare_posix);
                    let row = |mode: &str, size: u64, name: &str| -> String {
                        let mode = if windows {
                            format!("{}******", &mode[..4])
                        } else {
                            mode.to_string()
                        };
                        if bare && windows {
                            format!("{mode}    1 -        - {size:>15} Oct 15 02:27 {name}")
                        } else if bare {
                            line(&mode, size, name)
                        } else if numeric {
                            format!(
                                "{mode}    ? 501      20     {size:>10} Oct 15 02:27 {remote}/{name}"
                            )
                        } else {
                            line(&mode, size, &format!("{remote}/{name}"))
                        }
                    };
                    if !windows {
                        stdout.push_str(&format!("{}\n", row("drwxr-xr-x", 96, ".")));
                        stdout.push_str(&format!("{}\n", row("drwxr-xr-x", 96, "..")));
                    }
                    for (name, node) in self.children(&stored) {
                        let child = format!("{remote}/{name}");
                        if self.knobs().hidden.contains(&child) {
                            continue;
                        }
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
                        stdout.push_str(&format!("{}\n", row(mode, size, &name)));
                        newly_listed.push(child);
                    }
                } else if l.starts_with("get \"") {
                    if self.knobs().fail_get {
                        stderr.push_str(&format!("remote open(\"{remote}\"): No such file\n"));
                        code = 1;
                        break;
                    }
                    let node = self
                        .fs
                        .lock()
                        .expect("fs")
                        .get(&self.resolve(&remote))
                        .cloned();
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
    async fn read_over_a_bare_name_posix_server() {
        // a server without the users-groups extension (OpenSSH 8.9): `ls -lan`
        // makes the client format every line itself, so its bare `ls -la`
        // names never reach the parser
        let fake = FakeSftp::new();
        fake.knobs().bare_posix = true;
        standard_tree(&fake);
        let observed = transport(&fake).read(&scope()).await.expect("read");
        let mode: Vec<&str> = observed.mode.keys().map(RelPath::as_str).collect();
        assert_eq!(
            mode,
            [
                "ignition/database-connection/MES db,1/.dot",
                "ignition/database-connection/MES db,1/config.json",
                "ignition/database-connection/MES db,1/resource.json",
            ]
        );
        let lines: Vec<String> = fake.session_lines().into_iter().flatten().collect();
        let listings = lines.iter().filter(|l| l.starts_with("@ls -lan ")).count();
        let markers = lines.iter().filter(|l| *l == "@pwd").count();
        assert!(listings > 0 && markers == listings, "{lines:?}");
        assert!(
            !lines.iter().any(|l| l.starts_with("@ls -la ")),
            "a POSIX host is never listed with `ls -la`: {lines:?}"
        );
    }

    #[tokio::test]
    async fn prepare_over_a_bare_name_posix_server() {
        // the staging root below the account home is surveyed by listings,
        // the way `prepare` surveys it before it uploads
        let fake = FakeSftp::new();
        fake.knobs().bare_posix = true;
        fake.mkdirs("/home/svc/ign-deploy/gw/production");
        fake.listed
            .lock()
            .expect("listed")
            .extend(["/home".to_string(), "/home/svc".to_string()]);
        let shell = RemoteShell::local_sftp_server(
            Path::new("/unused/sftp-server"),
            fake.clone(),
            Duration::from_secs(30),
        );
        let m = SftpTransport::new(shell, DATA.to_string(), "nodes.master")
            .with_mirror(
                "production",
                StagingMode::Separate {
                    root: "/home/svc/ign-deploy/gw/production/staging/run-1".to_string(),
                },
            )
            .expect("mirror");
        m.stage(&win_tree_of(&[(
            &format!("{DC}/Keep/config.json"),
            "{\"keep\":2}",
        )]))
        .await
        .expect("stage over bare names");
        let fs = fake.fs.lock().expect("fs");
        assert!(
            fs.keys().any(
                |k| k.ends_with("/staging/run-1/ignition/database-connection/Keep/config.json")
            ),
            "{:?}",
            fs.keys().collect::<Vec<_>>()
        );
    }

    // ---- G2: the survey of a staging root starts at the account home ----------

    const RUN: &str = "20261019T030000Z-aaaaaa";

    /// A local directory holding `a.json`, and its `put_tree` entry list.
    fn one_local_file() -> (tempfile::TempDir, Vec<(String, u64)>) {
        let local = tempfile::tempdir().expect("local dir");
        std::fs::write(local.path().join("a.json"), b"{}").expect("local file");
        (local, vec![("a.json".to_string(), 2)])
    }

    fn apply_transport(base: SftpTransport) -> SftpTransport {
        base.with_apply("gw", "production").expect("apply half")
    }

    fn session_lines_flat(fake: &FakeSftp) -> Vec<String> {
        fake.session_lines().into_iter().flatten().collect()
    }

    /// The directories a session listed (`@ls -lan "<dir>"` on a POSIX host,
    /// `@ls -la "<dir>"` on a Windows host) or made.
    fn listed_dirs(lines: &[String]) -> Vec<String> {
        lines
            .iter()
            .filter(|l| l.starts_with("@ls -lan \"") || l.starts_with("@ls -la \""))
            .map(|l| quoted(l).remove(0))
            .collect()
    }

    fn made_dirs(lines: &[String]) -> Vec<String> {
        lines
            .iter()
            .filter(|l| l.starts_with("mkdir \""))
            .map(|l| quoted(l).remove(0))
            .collect()
    }

    #[tokio::test]
    async fn prepare_below_a_short_name_home_makes_nothing_at_or_above_it() {
        // the windows-latest shape: the account home is given as
        // `C:\Users\RUNNER~1`, which the listing of `C:\Users` never shows
        let short = "/C:/Users/RUNNER~1";
        let fake = windows_fake();
        fake.knobs().alias = Some((short.to_string(), WHOME.to_string()));
        fake.listed
            .lock()
            .expect("listed")
            .insert(short.to_string());
        let t = apply_transport(windows_transport(&fake).with_home(short));
        let home = t.account_home().await.expect("the cached home");
        assert_eq!(home, short);
        let root = t.staging_abs(&home, RUN).expect("staging root");
        let (local, entries) = one_local_file();
        let made = t
            .put_tree("prepare", &root, local.path(), &entries, true)
            .await
            .expect("prepare below a short-name home");
        assert!(made, "the staging root is new");
        let lines = session_lines_flat(&fake);
        for dir in made_dirs(&lines) {
            assert!(
                dir.starts_with(&format!("{short}/")),
                "mkdir {dir} is at or above the home"
            );
        }
        for dir in listed_dirs(&lines) {
            assert!(
                dir == short || dir.starts_with(&format!("{short}/")),
                "listed {dir}, which is above the home"
            );
        }
        let fs = fake.fs.lock().expect("fs");
        assert!(
            fs.contains_key(&format!(
                "{WHOME}/ign-deploy/gw/production/staging/{RUN}/a.json"
            )),
            "{:?}",
            fs.keys().collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn prepare_lists_nothing_above_the_home_posix() {
        let fake = FakeSftp::new();
        let t = apply_transport(transport(&fake));
        // the home is asked once, as `apply_roots` does
        let home = t.account_home().await.expect("home");
        assert_eq!(home, HOME);
        let root = t.staging_abs(&home, RUN).expect("staging root");
        let (local, entries) = one_local_file();
        t.put_tree("prepare", &root, local.path(), &entries, true)
            .await
            .expect("prepare");
        let lines = session_lines_flat(&fake);
        let dirs = listed_dirs(&lines);
        assert_eq!(dirs.first().map(String::as_str), Some(HOME), "{dirs:?}");
        assert!(
            dirs.iter().all(|d| d.starts_with(HOME)),
            "a directory above the home was listed: {dirs:?}"
        );
        assert!(made_dirs(&lines).iter().all(|d| d.starts_with(HOME)));
    }

    #[tokio::test]
    async fn a_staging_root_outside_the_home_keeps_the_root_survey() {
        let fake = FakeSftp::new();
        fake.listed
            .lock()
            .expect("listed")
            .insert("/gw".to_string());
        let t = apply_transport(transport(&fake).with_home(HOME));
        let root = "/gw/stage/gw/production/staging/run-1";
        let (local, entries) = one_local_file();
        t.put_tree("prepare", root, local.path(), &entries, true)
            .await
            .expect("a root outside the home");
        let lines = session_lines_flat(&fake);
        assert_eq!(
            listed_dirs(&lines).first().map(String::as_str),
            Some("/gw"),
            "{lines:?}"
        );
        assert!(
            !listed_dirs(&lines).iter().any(|d| d == HOME),
            "the home plays no part"
        );
        assert!(
            fake.fs
                .lock()
                .expect("fs")
                .contains_key("/gw/stage/gw/production/staging/run-1/a.json")
        );
    }

    #[tokio::test]
    async fn a_symlink_below_the_home_is_still_refused() {
        let fake = FakeSftp::new();
        fake.put(&format!("{HOME}/ign-deploy"), Node::Symlink);
        let t = apply_transport(transport(&fake).with_home(HOME));
        let root = t.staging_abs(HOME, RUN).expect("staging root");
        let (local, entries) = one_local_file();
        let err = t
            .put_tree("prepare", &root, local.path(), &entries, true)
            .await
            .expect_err("a symlink at the first component below the home");
        let text = err.to_string();
        assert!(
            text.contains("is a symlink; symlinks are refused"),
            "{text}"
        );
        assert!(made_dirs(&session_lines_flat(&fake)).is_empty());
    }

    #[tokio::test]
    async fn mirror_stage_below_the_home_surveys_from_the_home() {
        let fake = FakeSftp::new();
        fake.mkdirs("/home/svc/ign-deploy/gw/production");
        let shell = RemoteShell::local_sftp_server(
            Path::new("/unused/sftp-server"),
            fake.clone(),
            Duration::from_secs(30),
        );
        let m = SftpTransport::new(shell, DATA.to_string(), "nodes.master")
            .with_home(HOME)
            .with_mirror(
                "production",
                StagingMode::Separate {
                    root: "/home/svc/ign-deploy/gw/production/staging/run-1".to_string(),
                },
            )
            .expect("mirror");
        m.stage(&win_tree_of(&[(
            &format!("{DC}/Keep/config.json"),
            "{\"keep\":2}",
        )]))
        .await
        .expect("stage");
        let lines = session_lines_flat(&fake);
        let dirs = listed_dirs(&lines);
        assert_eq!(dirs.first().map(String::as_str), Some(HOME), "{dirs:?}");
        assert!(
            dirs.iter().all(|d| d.starts_with(HOME)),
            "a directory above the home was listed: {dirs:?}"
        );
    }

    // ---- WR-01: a fresh transport (as `mirror_for_node` builds) asks the home --

    /// `dir` is `home` or below it, on a component boundary.
    fn at_or_below(dir: &str, home: &str) -> bool {
        dir == home || dir.starts_with(&format!("{home}/"))
    }

    #[tokio::test]
    async fn mirror_stage_without_a_cached_home_surveys_from_the_short_name_home() {
        // `mirror_for_node` builds a fresh transport: nothing has asked the
        // home, and the server's `pwd` answers with the 8.3 alias, which the
        // listing of `C:\Users` never shows (G2 on the mirror path)
        let short = "/C:/Users/RUNNER~1";
        let fake = windows_fake();
        fake.knobs().alias = Some((short.to_string(), WHOME.to_string()));
        fake.knobs().home = Some(short.to_string());
        fake.listed
            .lock()
            .expect("listed")
            .insert(short.to_string());
        let m = windows_transport(&fake)
            .with_mirror(
                "production",
                StagingMode::Separate {
                    root: format!("{short}/ign-deploy/gw/production/staging/run-1"),
                },
            )
            .expect("mirror");
        m.stage(&win_tree_of(&[(
            &format!("{DC}/Keep/config.json"),
            "{\"keep\":2}",
        )]))
        .await
        .expect("stage below a short-name home");
        let lines = session_lines_flat(&fake);
        assert!(
            lines.iter().any(|l| l == "@pwd"),
            "the home is asked: {lines:?}"
        );
        for dir in made_dirs(&lines) {
            assert!(
                dir.starts_with(&format!("{short}/")),
                "mkdir {dir} is at or above the home"
            );
        }
        for dir in listed_dirs(&lines) {
            assert!(
                at_or_below(&dir, short),
                "listed {dir}, which is above the home"
            );
        }
        assert!(fake.fs.lock().expect("fs").contains_key(&format!(
            "{WHOME}/ign-deploy/gw/production/staging/run-1/{DC}/Keep/config.json"
        )));
    }

    #[tokio::test]
    async fn mirror_stage_without_a_cached_home_lists_nothing_above_the_home_posix() {
        let fake = FakeSftp::new();
        let shell = RemoteShell::local_sftp_server(
            Path::new("/unused/sftp-server"),
            fake.clone(),
            Duration::from_secs(30),
        );
        let m = SftpTransport::new(shell, DATA.to_string(), "nodes.master")
            .with_mirror(
                "production",
                StagingMode::Separate {
                    root: "/home/svc/ign-deploy/gw/production/staging/run-1".to_string(),
                },
            )
            .expect("mirror");
        m.stage(&win_tree_of(&[(
            &format!("{DC}/Keep/config.json"),
            "{\"keep\":2}",
        )]))
        .await
        .expect("stage");
        let lines = session_lines_flat(&fake);
        let dirs = listed_dirs(&lines);
        assert_eq!(dirs.first().map(String::as_str), Some(HOME), "{dirs:?}");
        assert!(
            dirs.iter().all(|d| at_or_below(d, HOME)),
            "a directory above the home was listed: {dirs:?}"
        );
        assert!(
            made_dirs(&lines)
                .iter()
                .all(|d| d.starts_with(&format!("{HOME}/"))),
            "{lines:?}"
        );
    }

    #[tokio::test]
    async fn put_tree_without_a_cached_home_surveys_from_the_short_name_home() {
        let short = "/C:/Users/RUNNER~1";
        let fake = windows_fake();
        fake.knobs().alias = Some((short.to_string(), WHOME.to_string()));
        fake.knobs().home = Some(short.to_string());
        fake.listed
            .lock()
            .expect("listed")
            .insert(short.to_string());
        let t = apply_transport(windows_transport(&fake));
        let root = t.staging_abs(short, RUN).expect("staging root");
        let (local, entries) = one_local_file();
        let made = t
            .put_tree("prepare", &root, local.path(), &entries, true)
            .await
            .expect("put_tree below a short-name home, nothing cached");
        assert!(made, "the staging root is new");
        let lines = session_lines_flat(&fake);
        for dir in listed_dirs(&lines) {
            assert!(
                at_or_below(&dir, short),
                "listed {dir}, which is above the home"
            );
        }
        for dir in made_dirs(&lines) {
            assert!(
                dir.starts_with(&format!("{short}/")),
                "mkdir {dir} is at or above the home"
            );
        }
    }

    #[tokio::test]
    async fn mirror_stage_with_an_unreadable_home_is_transport_failed() {
        // the home the server reports does not exist: its listing fails
        let fake = FakeSftp::new();
        fake.knobs().home = Some("/home/nope".to_string());
        fake.listed
            .lock()
            .expect("listed")
            .insert("/home/nope".to_string());
        let shell = RemoteShell::local_sftp_server(
            Path::new("/unused/sftp-server"),
            fake.clone(),
            Duration::from_secs(30),
        );
        let m = SftpTransport::new(shell, DATA.to_string(), "nodes.master")
            .with_mirror(
                "production",
                StagingMode::Separate {
                    root: "/home/nope/ign-deploy/gw/production/staging/run-1".to_string(),
                },
            )
            .expect("mirror");
        let err = m
            .stage(&win_tree_of(&[(
                &format!("{DC}/Keep/config.json"),
                "{\"keep\":2}",
            )]))
            .await
            .expect_err("an unreadable home");
        assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
        let lines = session_lines_flat(&fake);
        assert!(made_dirs(&lines).is_empty(), "{lines:?}");
        assert!(
            !listed_dirs(&lines).iter().any(|d| d == "/home"),
            "the home's parent is never listed: {lines:?}"
        );
    }

    // ---- WR-02: the home match follows the host's case rule -------------------

    #[tokio::test]
    async fn mirror_stage_below_a_windows_home_spelled_in_another_case() {
        // `/c:/users/SVC` is the home `/C:/Users/svc` on a Windows host: the
        // survey starts at the home, in the server's spelling
        let fake = windows_fake();
        // the fake's paths are case-sensitive; NTFS is not. The uploads name
        // the root as the operator spelled it, so fold that spelling onto the
        // stored one, as the host would
        fake.knobs().alias = Some(("/c:/users/SVC".to_string(), WHOME.to_string()));
        let m = windows_transport(&fake)
            .with_mirror(
                "production",
                StagingMode::Separate {
                    root: "/c:/users/SVC/ign-deploy/gw/production/staging/run-1".to_string(),
                },
            )
            .expect("mirror");
        m.stage(&win_tree_of(&[(
            &format!("{DC}/Keep/config.json"),
            "{\"keep\":2}",
        )]))
        .await
        .expect("stage below a home spelled in another case");
        let lines = session_lines_flat(&fake);
        let dirs = listed_dirs(&lines);
        assert_eq!(dirs.first().map(String::as_str), Some(WHOME), "{dirs:?}");
        for dir in &dirs {
            assert!(
                at_or_below(dir, WHOME),
                "listed {dir}, which is above the home"
            );
        }
        for dir in made_dirs(&lines) {
            assert!(
                dir.starts_with(&format!("{WHOME}/")),
                "mkdir {dir} is not below the home's own spelling"
            );
        }
        assert!(fake.fs.lock().expect("fs").contains_key(&format!(
            "{WHOME}/ign-deploy/gw/production/staging/run-1/{DC}/Keep/config.json"
        )));
    }

    #[tokio::test]
    async fn survey_root_home_match_follows_the_host_case_rule() {
        let fake = windows_fake();
        let t = windows_transport(&fake);
        let (base, rest) = t
            .survey_root("stage", "/c:/USERS/Svc/ign-deploy/run-1")
            .await
            .expect("windows root");
        assert_eq!(base, WHOME, "the base keeps the server's spelling");
        assert_eq!(rest, ["ign-deploy", "run-1"]);

        // a POSIX host is case-sensitive: `/home/SVC` is not the home
        let fake = FakeSftp::new();
        let t = transport(&fake);
        let (base, rest) = t
            .survey_root("stage", "/home/SVC/ign-deploy/run-1")
            .await
            .expect("posix root");
        assert_eq!(base, "/home");
        assert_eq!(rest, ["SVC", "ign-deploy", "run-1"]);
    }

    #[tokio::test]
    async fn survey_root_another_8dot3_spelling_of_the_home_falls_back() {
        // an 8.3 alias and its long name cannot be told apart offline: a root
        // in the other spelling than the home's is surveyed from the drive
        // root, as a root outside the home is (pinned, WR-02)
        let short = "/C:/Users/RUNNER~1";
        let fake = windows_fake();
        fake.knobs().home = Some(short.to_string());
        let t = windows_transport(&fake);
        let (base, rest) = t
            .survey_root("stage", "/C:/Users/svc/ign-deploy/run-1")
            .await
            .expect("long spelling, short home");
        assert_eq!(base, "/C:");
        assert_eq!(rest, ["Users", "svc", "ign-deploy", "run-1"]);

        let fake = windows_fake();
        let t = windows_transport(&fake);
        let (base, rest) = t
            .survey_root("stage", &format!("{short}/ign-deploy/run-1"))
            .await
            .expect("short spelling, long home");
        assert_eq!(base, "/C:");
        assert_eq!(rest, ["Users", "RUNNER~1", "ign-deploy", "run-1"]);
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
                l == "@pwd" || l.starts_with("@ls -lan \"") || l.starts_with("get \""),
                "only markers, listings and single-file gets, never `get -R`: {l}"
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
            // (a listing session is `@pwd` markers and `@ls -lan` lines)
            let last = fake.session_lines().pop().expect("session");
            assert!(
                last.iter()
                    .all(|l| l == "@pwd" || l.starts_with("@ls -lan \"")),
                "{last:?}"
            );
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
        assert!(fake.session_lines().is_empty(), "no process was spawned");
        // the ledger lives in the account home (Decision B(i)), not below
        // the data directory, so it does not depend on the data directory
        assert_eq!(
            t.read_ledger("gw", "production")
                .await
                .expect("home ledger"),
            None
        );
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

    /// Decision B(i): an ssh node's ledger is below the account home.
    fn ledger_path() -> String {
        format!("{HOME}/ign-deploy/gw/production/{LEDGER_FILE}")
    }

    #[tokio::test]
    async fn absent_ledger_at_any_level_is_none() {
        let fake = FakeSftp::new();
        let t = transport(&fake);
        assert_eq!(t.read_ledger("gw", "production").await.expect("none"), None);
        fake.mkdirs(&format!("{HOME}/ign-deploy"));
        assert_eq!(t.read_ledger("gw", "production").await.expect("none"), None);
        fake.mkdirs(&format!("{HOME}/ign-deploy/gw/production"));
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
        fake.put(&format!("{HOME}/ign-deploy/gw"), Node::Symlink);
        let p = invalid(
            transport(&fake)
                .read_ledger("gw", "production")
                .await
                .expect_err("link"),
        );
        assert!(p.contains("symlink"), "{p}");
        // a component that is a file
        let fake = FakeSftp::new();
        fake.file(&format!("{HOME}/ign-deploy/gw"), "x");
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
        // the home the server reports does not exist
        let fake = FakeSftp::new();
        let t = transport(&fake);
        fake.knobs().home = Some("/home/nope".to_string());
        fake.listed
            .lock()
            .expect("listed")
            .insert("/home/nope".to_string());
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

    // ---- read_release_record (25-08) -------------------------------------------

    /// The release record is below the account home, under `projects/<name>`.
    fn record_path() -> String {
        format!("{HOME}/ign-deploy/gw/projects/CapApp/release.json")
    }

    #[tokio::test]
    async fn sftp_release_record_posix() {
        use crate::deploy::ledger::RELEASE_RECORD_MAX_BYTES;
        use crate::deploy::transport::projects::ProjectHost;
        let invalid = |err: CoreError| match err {
            CoreError::DeployRunInvalid { run_id, problem } => {
                assert_eq!(run_id, "unknown");
                problem
            }
            other => panic!("expected deploy_run_invalid, got {other:?}"),
        };
        // absent at every level is None
        let fake = FakeSftp::new();
        let t = transport(&fake);
        assert_eq!(
            t.read_release_record("gw", "CapApp").await.expect("none"),
            None
        );
        fake.mkdirs(&format!("{HOME}/ign-deploy"));
        assert_eq!(
            t.read_release_record("gw", "CapApp").await.expect("none"),
            None
        );
        fake.mkdirs(&format!("{HOME}/ign-deploy/gw/projects/CapApp"));
        assert_eq!(
            t.read_release_record("gw", "CapApp").await.expect("none"),
            None
        );

        // a regular file returns its bytes, read below the account home
        let fake = FakeSftp::new();
        fake.file(&record_path(), "{\"schema_version\":1}");
        assert_eq!(
            transport(&fake)
                .read_release_record("gw", "CapApp")
                .await
                .expect("record"),
            Some(b"{\"schema_version\":1}".to_vec())
        );
        // and not from below the data directory
        let fake = FakeSftp::new();
        fake.file(
            &format!("{DATA}/ign-deploy/gw/projects/CapApp/release.json"),
            "{}",
        );
        assert_eq!(
            transport(&fake)
                .read_release_record("gw", "CapApp")
                .await
                .expect("none"),
            None
        );

        // the file is a symlink
        let fake = FakeSftp::new();
        fake.put(&record_path(), Node::Symlink);
        let p = invalid(
            transport(&fake)
                .read_release_record("gw", "CapApp")
                .await
                .expect_err("link"),
        );
        assert!(p.contains("symlink"), "{p}");
        assert!(
            p.starts_with("~/ign-deploy/gw/projects/CapApp/release.json"),
            "{p}"
        );
        // a component on the way is a symlink
        let fake = FakeSftp::new();
        fake.put(&format!("{HOME}/ign-deploy/gw/projects"), Node::Symlink);
        let p = invalid(
            transport(&fake)
                .read_release_record("gw", "CapApp")
                .await
                .expect_err("link"),
        );
        assert!(p.contains("symlink"), "{p}");
        // the record is a directory
        let fake = FakeSftp::new();
        fake.mkdirs(&record_path());
        let p = invalid(
            transport(&fake)
                .read_release_record("gw", "CapApp")
                .await
                .expect_err("dir"),
        );
        assert!(p.contains("not a regular file"), "{p}");
        // over the cap, from the listing, before any download
        let fake = FakeSftp::new();
        fake.file(&record_path(), "{}");
        fake.knobs().lie_size = Some((
            "release.json".to_string(),
            RELEASE_RECORD_MAX_BYTES as u64 + 1,
        ));
        let p = invalid(
            transport(&fake)
                .read_release_record("gw", "CapApp")
                .await
                .expect_err("cap"),
        );
        assert!(p.contains("byte cap"), "{p}");
        assert!(!fake.session_lines().pop().expect("last")[0].starts_with("get"));
        // hostile names are refused before any session
        let fake = FakeSftp::new();
        let t = transport(&fake);
        for (gateway, project, kind) in [
            ("../x", "CapApp", "gateway name"),
            ("gw", "../x", "project name"),
            ("gw", "a/b", "project name"),
            ("gw", "", "project name"),
        ] {
            let p = invalid(
                t.read_release_record(gateway, project)
                    .await
                    .expect_err(project),
            );
            assert!(p.contains(kind), "{p}");
        }
        assert!(fake.session_lines().is_empty());
    }

    #[tokio::test]
    async fn sftp_release_record_unreadable_host_is_not_an_absent_record() {
        use crate::deploy::transport::projects::ProjectHost;
        let fake = FakeSftp::new();
        let t = transport(&fake);
        fake.knobs().home = Some("/home/nope".to_string());
        fake.listed
            .lock()
            .expect("listed")
            .insert("/home/nope".to_string());
        let err = t
            .read_release_record("gw", "CapApp")
            .await
            .expect_err("host");
        assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
    }

    // ---- the release record write (25-13) -----------------------------------------

    /// Runs sftp against the scripted server and answers an `ssh` call as a
    /// POSIX host would answer the fixed `mv -f -- <tmp> <final>`: it records
    /// the command, then moves the file in the scripted tree over the target
    /// (or fails with `Permission denied` when told to).
    struct MvRunner {
        fake: Arc<FakeSftp>,
        execs: Mutex<Vec<String>>,
        fail: bool,
    }

    #[async_trait::async_trait]
    impl ProcessRunner for MvRunner {
        async fn run(
            &self,
            program: &OsStr,
            args: &[OsString],
            stdin: Option<Vec<u8>>,
            timeout: Duration,
        ) -> ProcessOutput {
            if program != "ssh" {
                return self.fake.run(program, args, stdin, timeout).await;
            }
            let command = args.last().expect("command").to_string_lossy().into_owned();
            self.execs.lock().expect("execs").push(command.clone());
            if self.fail {
                return ProcessOutput {
                    stdout: Vec::new(),
                    stderr: b"mv: cannot move: Permission denied\n".to_vec(),
                    code: 1,
                    timed_out: false,
                };
            }
            let inner = command
                .strip_prefix("'mv' '-f' '--' '")
                .and_then(|rest| rest.strip_suffix('\''))
                .expect("a fixed mv command");
            let (from, to) = inner.split_once("' '").expect("two paths");
            let mut fs = self.fake.fs.lock().expect("fs");
            let node = fs.remove(from).expect("the temporary file exists");
            fs.insert(to.to_string(), node);
            ProcessOutput {
                stdout: Vec::new(),
                stderr: Vec::new(),
                code: 0,
                timed_out: false,
            }
        }
    }

    fn mv_transport(fake: &Arc<FakeSftp>, fail: bool) -> (Arc<MvRunner>, SftpTransport) {
        let runner = Arc::new(MvRunner {
            fake: fake.clone(),
            execs: Mutex::new(Vec::new()),
            fail,
        });
        let shell = RemoteShell::local_sftp_server(
            Path::new("/unused/sftp-server"),
            runner.clone(),
            Duration::from_secs(30),
        );
        (
            runner,
            SftpTransport::new(shell, DATA.to_string(), "nodes.master"),
        )
    }

    fn record_for(gateway: &str, project: &str, release: &str) -> Vec<u8> {
        crate::deploy::projects::ReleaseRecord {
            schema_version: 1,
            gateway: gateway.to_string(),
            project: project.to_string(),
            release: release.to_string(),
            artifact_sha256: "ab".repeat(32),
            run_id: "20261020T101500Z-ab12".to_string(),
            deployed_at: "2026-10-20T10:15:00Z".to_string(),
        }
        .to_bytes()
    }

    const TEMP_NAME: &str = "release.json.tmp-20261020T101500Z-ab12";

    /// The mutating sftp lines of every session so far, as `(command, path)`.
    fn writes(fake: &FakeSftp) -> Vec<(String, String)> {
        fake.session_lines()
            .into_iter()
            .flatten()
            .filter(|l| {
                ["mkdir ", "put ", "rename ", "rm ", "rmdir "]
                    .iter()
                    .any(|c| l.starts_with(c))
            })
            .map(|l| {
                let q = quoted(&l);
                let command = l.split(' ').next().expect("command").to_string();
                (command, q.last().expect("a path").clone())
            })
            .collect()
    }

    fn tree_keys(fake: &FakeSftp) -> Vec<String> {
        fake.fs.lock().expect("fs").keys().cloned().collect()
    }

    /// Edge DPRJ-01 concurrency, T-25-40, T-25-43: missing levels are made in
    /// order, the bytes go to a temporary sibling, one fixed `mv -f --` puts
    /// them over the record, and nothing outside the account home's
    /// `ign-deploy` tree changes.
    #[tokio::test]
    async fn sftp_record_write_posix() {
        use crate::deploy::transport::projects::{ProjectHost, ReleaseRecordWriter};
        let fake = FakeSftp::new();
        let (runner, t) = mv_transport(&fake, false);
        let before = tree_keys(&fake);
        let first = record_for("gw", "CapApp", "v1.0.0");
        t.write_release_record("gw", "CapApp", &first)
            .await
            .expect("first write");

        let base = format!("{HOME}/ign-deploy");
        let dir = format!("{base}/gw/projects/CapApp");
        assert_eq!(
            writes(&fake),
            [
                ("mkdir".to_string(), base.clone()),
                ("mkdir".to_string(), format!("{base}/gw")),
                ("mkdir".to_string(), format!("{base}/gw/projects")),
                ("mkdir".to_string(), dir.clone()),
                ("put".to_string(), format!("{dir}/{TEMP_NAME}")),
            ],
            "levels in order, then one put to the temporary name, nothing else"
        );
        // one exec, exactly the fixed argv, both paths below the account home
        let execs = runner.execs.lock().expect("execs").clone();
        assert_eq!(
            execs,
            [format!(
                "'mv' '-f' '--' '{dir}/{TEMP_NAME}' '{dir}/release.json'"
            )]
        );
        // the record is in place, the temporary file is gone
        assert_eq!(
            t.read_release_record("gw", "CapApp").await.expect("read"),
            Some(first.clone())
        );
        let after = tree_keys(&fake);
        assert!(!after.iter().any(|k| k.contains(".tmp-")), "{after:?}");
        // nothing outside `<home>/ign-deploy` was created, and nothing at all
        // below the data directory
        for key in after.iter().filter(|k| !before.contains(k)) {
            assert!(key.starts_with(&base), "{key}");
        }
        assert!(!after.iter().any(|k| k.starts_with(&format!("{DATA}/"))));

        // a second write replaces it: no directory is made again, the old
        // bytes are never seen torn (the replace is the mv)
        let before_second = fake.session_lines().len();
        let second = record_for("gw", "CapApp", "v1.1.0");
        t.write_release_record("gw", "CapApp", &second)
            .await
            .expect("second write");
        let later: Vec<(String, String)> = writes(&fake).into_iter().skip(5).collect();
        assert_eq!(later, [("put".to_string(), format!("{dir}/{TEMP_NAME}"))]);
        assert!(fake.session_lines().len() > before_second);
        assert_eq!(
            t.read_release_record("gw", "CapApp").await.expect("read"),
            Some(second)
        );

        // an existing project directory under a new gateway level: only the
        // missing levels are made
        let fake = FakeSftp::new();
        fake.mkdirs(&format!("{HOME}/ign-deploy/gw"));
        let (_, t) = mv_transport(&fake, false);
        t.write_release_record("gw", "CapApp", &first)
            .await
            .expect("partial tree");
        let made: Vec<String> = writes(&fake)
            .into_iter()
            .filter(|(c, _)| c == "mkdir")
            .map(|(_, p)| p)
            .collect();
        assert_eq!(made, [format!("{base}/gw/projects"), dir.clone()]);
    }

    /// T-25-41: a link or a non-directory on the way, a record that is not a
    /// regular file and a temporary file already in the way are refused
    /// before anything is made or written; the names and the bytes are held
    /// to the read's rules first.
    #[tokio::test]
    async fn sftp_record_write_refusals() {
        use crate::deploy::transport::projects::ReleaseRecordWriter;
        let bytes = record_for("gw", "CapApp", "v1.0.0");
        let base = format!("{HOME}/ign-deploy");
        let record = format!("{base}/gw/projects/CapApp/release.json");
        type Setup<'a> = Box<dyn Fn(&FakeSftp) + 'a>;
        let cases: Vec<(&str, Setup<'_>, &str)> = vec![
            (
                "a link at ign-deploy",
                Box::new(|f| f.put(&base, Node::Symlink)),
                "symlink",
            ),
            (
                "a link at projects",
                Box::new(|f| f.put(&format!("{base}/gw/projects"), Node::Symlink)),
                "symlink",
            ),
            (
                "a file at projects",
                Box::new(|f| f.file(&format!("{base}/gw/projects"), "x")),
                "not a directory",
            ),
            (
                "a link at the project",
                Box::new(|f| f.put(&format!("{base}/gw/projects/CapApp"), Node::Symlink)),
                "symlink",
            ),
            (
                "a directory at the record",
                Box::new(|f| f.mkdirs(&record)),
                "a directory",
            ),
            (
                "a link at the record",
                Box::new(|f| f.put(&record, Node::Symlink)),
                "symlink",
            ),
            (
                "a special file at the record",
                Box::new(|f| f.put(&record, Node::Special)),
                "not a regular file",
            ),
            (
                "a temporary file in the way",
                Box::new(|f| f.file(&format!("{base}/gw/projects/CapApp/{TEMP_NAME}"), "x")),
                "temporary name",
            ),
            (
                "a link of the temporary name",
                Box::new(|f| {
                    f.put(
                        &format!("{base}/gw/projects/CapApp/{TEMP_NAME}"),
                        Node::Symlink,
                    )
                }),
                "temporary name",
            ),
        ];
        for (name, setup, needle) in cases {
            let fake = FakeSftp::new();
            fake.mkdirs(&format!("{base}/gw/projects/CapApp"));
            setup(&fake);
            let (runner, t) = mv_transport(&fake, false);
            let before = tree_keys(&fake);
            let err = t
                .write_release_record("gw", "CapApp", &bytes)
                .await
                .expect_err(name);
            let CoreError::TransportFailed { detail, .. } = &err else {
                panic!("{name}: expected transport_failed, got {err:?}");
            };
            assert!(
                detail.starts_with("~/ign-deploy/gw/projects/CapApp/release.json"),
                "{name}: {detail}"
            );
            assert!(detail.contains(needle), "{name}: {detail}");
            assert!(
                writes(&fake).is_empty(),
                "{name}: wrote {:?}",
                writes(&fake)
            );
            assert!(runner.execs.lock().expect("execs").is_empty(), "{name}");
            assert_eq!(tree_keys(&fake), before, "{name}");
        }

        // hostile names, a record for another project and an empty record are
        // refused before any session
        let fake = FakeSftp::new();
        let (runner, t) = mv_transport(&fake, false);
        for (gateway, project, payload) in [
            ("../x", "CapApp", bytes.clone()),
            ("gw", "../x", bytes.clone()),
            ("gw", "a/b", bytes.clone()),
            ("gw", "", bytes.clone()),
            ("gw", "CapApp", record_for("gw", "Other", "v1.0.0")),
            ("gw", "CapApp", record_for("other", "CapApp", "v1.0.0")),
            ("gw", "CapApp", Vec::new()),
            ("gw", "CapApp", b"{}".to_vec()),
        ] {
            let err = t
                .write_release_record(gateway, project, &payload)
                .await
                .expect_err(project);
            assert!(
                matches!(err, CoreError::DeployRunInvalid { .. }),
                "{gateway}/{project}: {err:?}"
            );
        }
        assert!(fake.session_lines().is_empty());
        assert!(runner.execs.lock().expect("execs").is_empty());
    }

    /// T-25-40: when the replace fails, the temporary file is removed and the
    /// previous record is still there, byte for byte.
    #[tokio::test]
    async fn sftp_record_write_failure_cleans_temp() {
        use crate::deploy::transport::projects::{ProjectHost, ReleaseRecordWriter};
        let fake = FakeSftp::new();
        let previous = record_for("gw", "CapApp", "v1.0.0");
        fake.file(
            &record_path(),
            std::str::from_utf8(&previous).expect("utf-8"),
        );
        let (runner, t) = mv_transport(&fake, true);
        let err = t
            .write_release_record("gw", "CapApp", &record_for("gw", "CapApp", "v1.1.0"))
            .await
            .expect_err("the mv fails");
        let CoreError::TransportFailed { detail, .. } = &err else {
            panic!("expected transport_failed, got {err:?}");
        };
        assert!(
            detail.starts_with("~/ign-deploy/gw/projects/CapApp/release.json"),
            "{detail}"
        );
        assert!(detail.contains("Permission denied"), "{detail}");
        assert!(!detail.contains("may still be there"), "{detail}");
        assert_eq!(runner.execs.lock().expect("execs").len(), 1);
        // put, then rm of the same temporary path
        let dir = format!("{HOME}/ign-deploy/gw/projects/CapApp");
        assert_eq!(
            writes(&fake),
            [
                ("put".to_string(), format!("{dir}/{TEMP_NAME}")),
                ("rm".to_string(), format!("{dir}/{TEMP_NAME}")),
            ]
        );
        assert!(!tree_keys(&fake).iter().any(|k| k.contains(".tmp-")));
        let (_, reader) = mv_transport(&fake, false);
        assert_eq!(
            reader
                .read_release_record("gw", "CapApp")
                .await
                .expect("read"),
            Some(previous)
        );
    }

    // ---- read_journal (23-18) --------------------------------------------------

    /// The journal is below the account home, beside the ledger (D-04).
    fn journal_path() -> String {
        format!("{HOME}/ign-deploy/gw/production/{JOURNAL_FILE}")
    }

    #[tokio::test]
    async fn read_journal_sftp() {
        let invalid = |err: CoreError| match err {
            CoreError::DeployRunInvalid { problem, .. } => problem,
            other => panic!("expected deploy_run_invalid, got {other:?}"),
        };
        let read = |fake: &Arc<FakeSftp>| {
            let t = transport(fake);
            async move { t.read_journal("gw", "production").await }
        };

        // absent at every level is None
        let fake = FakeSftp::new();
        assert_eq!(read(&fake).await.expect("none"), None);
        fake.mkdirs(&format!("{HOME}/ign-deploy"));
        assert_eq!(read(&fake).await.expect("none"), None);
        fake.mkdirs(&format!("{HOME}/ign-deploy/gw/production"));
        assert_eq!(read(&fake).await.expect("none"), None);

        // present: the bytes, unvalidated; a ledger beside it is not read
        fake.file(&journal_path(), "{\"schema_version\":1}");
        fake.file(&ledger_path(), "other");
        assert_eq!(
            read(&fake).await.expect("journal"),
            Some(b"{\"schema_version\":1}".to_vec())
        );

        // the journal is a symlink
        let fake = FakeSftp::new();
        fake.put(&journal_path(), Node::Symlink);
        let p = invalid(read(&fake).await.expect_err("link"));
        assert!(p.contains("symlink"), "{p}");
        assert!(p.contains("~/ign-deploy/gw/production/journal.json"), "{p}");
        // a directory on the way is a symlink
        let fake = FakeSftp::new();
        fake.put(&format!("{HOME}/ign-deploy/gw"), Node::Symlink);
        let p = invalid(read(&fake).await.expect_err("link"));
        assert!(p.contains("symlink"), "{p}");
        // a component that is a file
        let fake = FakeSftp::new();
        fake.file(&format!("{HOME}/ign-deploy/gw"), "x");
        let p = invalid(read(&fake).await.expect_err("file"));
        assert!(p.contains("not a directory"), "{p}");
        // the journal is a directory
        let fake = FakeSftp::new();
        fake.mkdirs(&journal_path());
        let p = invalid(read(&fake).await.expect_err("dir"));
        assert!(p.contains("not a regular file"), "{p}");
        // over the cap, from the listing, before a download
        let fake = FakeSftp::new();
        fake.file(&journal_path(), "{}");
        fake.knobs().lie_size = Some((JOURNAL_FILE.to_string(), JOURNAL_MAX_BYTES as u64 + 1));
        let p = invalid(read(&fake).await.expect_err("cap"));
        assert!(p.contains("byte cap"), "{p}");
        assert!(!fake.session_lines().pop().expect("last")[0].starts_with("get"));
        // a hostile gateway name
        let fake = FakeSftp::new();
        let err = transport(&fake)
            .read_journal("../x", "production")
            .await
            .expect_err("name");
        assert!(invalid(err).contains("gateway"));
        // an unreadable host is not an absent journal
        let fake = FakeSftp::new();
        fake.knobs().home = Some("/home/nope".to_string());
        fake.listed
            .lock()
            .expect("listed")
            .insert("/home/nope".to_string());
        let err = read(&fake).await.expect_err("host");
        assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
        // a failed download is not an absent journal
        let fake = FakeSftp::new();
        fake.file(&journal_path(), "{}");
        fake.knobs().stderr_noise = true;
        let err = read(&fake).await.expect_err("stderr");
        assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
    }

    // ---- resource caps (post-review) -------------------------------------------

    /// Answers every `@ls -lan` with a fixed directory chain and `filler`
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
                if l == "@pwd" {
                    out.push_str(&format!("Remote working directory: {DATA}\n"));
                    continue;
                }
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
            std::iter::once(format!("{PWD_MARKER}/home/u"))
                .chain((0..n).map(|i| line("-rw-r--r--", 1, &format!("/d/f{i}"))))
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
            ok(0),
            line("drwxr-xr-x", 1, "/d/."),
            line("drwxr-xr-x", 1, "/d/..")
        ) + &ok(5)["Remote working directory: /home/u".len()..];
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

    // ---- Windows OpenSSH (22-13), from the C7a captures -------------------------

    const WDATA: &str = "/C:/gw/data";
    const WRES: &str = "/C:/gw/data/config/resources";
    const WHOME: &str = "/C:/Users/svc";

    #[test]
    fn windows_sftp_root_vectors() {
        for (input, want) in [
            // the captured data dir (C7a-1)
            (
                "C:\\Program Files\\Inductive Automation\\Ignition\\data",
                "/C:/Program Files/Inductive Automation/Ignition/data",
            ),
            // forward slashes, a lowercase drive letter, a trailing separator
            (
                "c:/Program Files/Inductive Automation/Ignition/data/",
                "/C:/Program Files/Inductive Automation/Ignition/data",
            ),
            ("d:\\ign data, 1\\gw\\", "/D:/ign data, 1/gw"),
            ("C:\\a\\b\\\\", "/C:/a/b"),
        ] {
            assert_eq!(windows_sftp_root(input).as_deref(), Ok(want), "{input}");
        }
        for bad in [
            // UNC and extended-length paths
            "\\\\server\\share\\data",
            "//server/share/data",
            "\\\\?\\C:\\data",
            // relative, drive-relative, rooted without a drive, POSIX
            "data",
            "relative\\data",
            "C:data",
            "\\data",
            "/usr/local/ignition/data",
            "/C:/data",
            "",
            // the drive root
            "C:\\",
            "C:",
            // components
            "C:\\a\\..\\b",
            "C:\\a\\.\\b",
            "C:\\a\\\\b",
            "C:\\a.\\b",
            "C:\\a \\b",
            "C:\\a\\b:stream",
            // quotes, control characters, glob characters
            "C:\\a\"b",
            "C:\\a\nb",
            "C:\\a*b",
            "C:\\a[1]",
            "C:\\a{b}",
            // too long for the sftp server to resolve what is below it
            &format!("C:\\{}", "d".repeat(WINDOWS_DATA_DIR_MAX)),
        ] {
            assert!(windows_sftp_root(bad).is_err(), "{bad:?} must be refused");
        }
        assert_eq!(
            windows_native_path("/C:/Program Files/Ignition/data/config"),
            "C:\\Program Files\\Ignition\\data\\config"
        );
    }

    /// Verbatim from `## C7a` (C7a-2): `ls -la` of the real collection root
    /// and of the spaces-and-comma directory, in a UTF-8 client locale, each
    /// led by the `pwd` marker the transport puts before a listing.
    #[test]
    fn windows_listing_parses_captured_output() {
        const ROOT: &str = "Remote working directory: /C:/Users/svc-ignition-deploy\n\
drwx******    1 -        -          262144 Oct 15 02:10 .resources\n\
drwx******    1 -        -            4096 Oct 15 02:05 core\n\
drwx******    1 -        -               0 Oct 15 02:07 external\n\
drwx******    1 -        -               0 Oct 15 02:05 local\n\
-rw-******    1 -        -           26893 Oct 15 02:04 migration-log-202610150204.md\n\
drwx******    1 -        -               0 Oct 15 02:08 production\n";
        let dirs = vec!["/C:/Program Files/x/resources".to_string()];
        let got = parse_batch_listing_as(
            ListFormat::Windows,
            NameRule::Refuse,
            &dirs,
            ROOT.as_bytes(),
            100,
        )
        .expect("root listing")
        .remove(0);
        let shown: Vec<(&str, EntryKind, u64)> = got
            .iter()
            .map(|e| (e.name.as_str(), e.kind, e.size))
            .collect();
        assert_eq!(
            shown,
            [
                (".resources", EntryKind::Dir, 262144),
                ("core", EntryKind::Dir, 4096),
                ("external", EntryKind::Dir, 0),
                ("local", EntryKind::Dir, 0),
                ("migration-log-202610150204.md", EntryKind::File, 26893),
                ("production", EntryKind::Dir, 0),
            ]
        );

        // spaces, a comma, a non-ASCII name printed raw by a UTF-8 client, an
        // empty directory first and a listing after it
        const SPACES: &str = "Remote working directory: /C:/Users/u\n\
Remote working directory: /C:/Users/u\n\
-rw-******    1 -        -               8 Oct 15 02:35 café.txt\n\
-rw-******    1 -        -               6 Oct 15 02:35 f one.txt\n\
-rw-******    1 -        -               6 Oct 15 02:35 f,two.txt\n\
-rw-******    1 -        -               1 Oct 15 02:35 a  double  space\n\
drwx******    1 -        -               0 Oct 15 02:35 sub dir\n";
        let dirs = vec!["/C:/e".to_string(), "/C:/d".to_string()];
        let got = parse_batch_listing_as(
            ListFormat::Windows,
            NameRule::Refuse,
            &dirs,
            SPACES.as_bytes(),
            100,
        )
        .expect("spaces");
        assert!(
            got[0].is_empty(),
            "an empty directory prints nothing after its marker"
        );
        let names: Vec<&str> = got[1].iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "a  double  space",
                "café.txt",
                "f one.txt",
                "f,two.txt",
                "sub dir"
            ]
        );

        let one = vec!["/C:/d".to_string()];
        let parse = |text: &str| {
            parse_batch_listing_as(
                ListFormat::Windows,
                NameRule::Refuse,
                &one,
                text.as_bytes(),
                100,
            )
        };
        // a client without a UTF-8 locale prints the escaped name: refused,
        // never read as the real one
        assert!(
            parse("Remote working directory: /C:/h\n-rw-******    1 -        -   8 Oct 15 02:35 caf\\303\\251.txt\n")
                .is_err()
        );
        // lines before the first marker, too many markers, too few markers
        assert!(parse("-rw-******    1 -        -   8 Oct 15 02:35 f\n").is_err());
        assert!(
            parse("Remote working directory: /C:/h\nRemote working directory: /C:/h\n").is_err()
        );
        assert!(
            parse_batch_listing_as(
                ListFormat::Windows,
                NameRule::Refuse,
                &["/C:/a".to_string(), "/C:/b".to_string()],
                b"Remote working directory: /C:/h\n",
                100
            )
            .is_err()
        );
        // too few columns, a bad size, a bad mode, a duplicate
        for bad in [
            "-rw-******    1 -        - 8 Oct 15 02:35\n",
            "-rw-******    1 -        -  NaN Oct 15 02:35 f\n",
            "-rw    1 -        -   8 Oct 15 02:35 f\n",
            "-rw-******    1 -        -   8 Oct 15 02:35 f\n-rw-******    1 -        -   9 Oct 15 02:35 f\n",
            "-rw-******    1 -        -   8 Oct 15 02:35 a/b\n",
        ] {
            assert!(
                parse(&format!("Remote working directory: /C:/h\n{bad}")).is_err(),
                "{bad:?}"
            );
        }
        // the single-file shape (C7a-2: link count `?`, uid 0) is not a
        // directory listing line the transport ever asks for; with its full
        // path as the name it is refused as a multi-component name
        assert!(
            parse("Remote working directory: /C:/h\n-rw-------    ? 0        0               6 Oct 14 20:35 ign-c7a/d/f one.txt\n")
                .is_err()
        );
        // unsafe names: refused, or skipped where only fixed names are looked up
        let home = "Remote working directory: /C:/Users/u\n\
drwx******    1 -        -               0 Oct 15 01:55 AppData\n\
-rw-******    1 -        -           65536 Oct 15 01:54 NTUSER.DAT{6b3cbf8b-c6cb-11f1-8338-00155d013703}.TM.blf\n\
drwx******    1 -        -               0 Oct 15 01:54 ign-deploy\n";
        assert!(
            parse_batch_listing_as(
                ListFormat::Windows,
                NameRule::Refuse,
                &one,
                home.as_bytes(),
                100
            )
            .is_err()
        );
        let skipped = parse_batch_listing_as(
            ListFormat::Windows,
            NameRule::Skip,
            &one,
            home.as_bytes(),
            100,
        )
        .expect("skip")
        .remove(0);
        let names: Vec<&str> = skipped.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["AppData", "ign-deploy"]);
    }

    fn windows_fake() -> Arc<FakeSftp> {
        let fake = FakeSftp::new();
        fake.knobs().windows = true;
        fake.knobs().home = Some(WHOME.to_string());
        fake.mkdirs(WDATA);
        fake.mkdirs(WHOME);
        fake.listed.lock().expect("listed").extend([
            WDATA.to_string(),
            WHOME.to_string(),
            "/C:".to_string(),
        ]);
        fake
    }

    fn windows_transport(fake: &Arc<FakeSftp>) -> SftpTransport {
        let shell = RemoteShell::local_sftp_server(
            Path::new("/unused/sftp-server"),
            fake.clone(),
            Duration::from_secs(30),
        );
        SftpTransport::new_windows(shell, WDATA.to_string(), "nodes.master")
    }

    fn windows_tree(fake: &FakeSftp) {
        fake.file(
            &format!("{WRES}/core/ignition/database-connection/CoreDb/config.json"),
            "{\"core\":1}",
        );
        fake.file(
            &format!("{WRES}/production/ignition/database-connection/MES db,1/config.json"),
            "{\"mes\":1}",
        );
        fake.file(
            &format!("{WRES}/production/ignition/database-connection/MES db,1/resource.json"),
            "{\"scope\":\"A\"}",
        );
        fake.file(
            &format!("{WRES}/production/ignition/database-connection/MES db,1/.dot"),
            "d",
        );
        fake.file(
            &format!("{WRES}/local/ignition/database-connection/Override/config.json"),
            "{}",
        );
        fake.put(
            &format!("{WRES}/production/ignition/other-type"),
            Node::Symlink,
        );
    }

    #[tokio::test]
    async fn windows_read_collects_the_same_tree_and_checks_it_against_a_walk() {
        let fake = windows_fake();
        windows_tree(&fake);
        let got = windows_transport(&fake).read(&scope()).await.expect("read");
        assert_eq!(got.core.len(), 1);
        assert_eq!(
            got.mode.len(),
            3,
            "config.json, resource.json and the dot file"
        );
        assert_eq!(got.local_units.len(), 1);
        assert!(
            got.mode
                .contains_key(&rel("ignition/database-connection/MES db,1/.dot"))
        );
        // every session lists with a pwd marker; the walk ran at least once
        let all: Vec<String> = fake.session_lines().into_iter().flatten().collect();
        assert!(all.iter().any(|l| l == "@pwd"));
        assert!(fake.knobs().ssh_calls >= 1, "no walk, no completeness");
        // sizes and bytes survive
        assert_eq!(
            got.mode[&rel("ignition/database-connection/MES db,1/config.json")].bytes,
            b"{\"mes\":1}"
        );
    }

    fn windows_read_failure(fake: &Arc<FakeSftp>) -> String {
        let err = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("runtime")
            .block_on(windows_transport(fake).read(&scope()))
            .expect_err("the read must fail closed");
        detail(err)
    }

    #[test]
    fn windows_read_fails_closed_when_the_listing_omits_an_entry() {
        // C7a-8: the sftp server drops a path of 260 characters or more from a
        // listing with exit 0. The walk still sees it.
        let fake = windows_fake();
        windows_tree(&fake);
        let long = format!("{WRES}/production/ignition/database-connection/MES db,1/config.json");
        fake.knobs().hidden.push(long);
        let why = windows_read_failure(&fake);
        assert!(why.contains("did not return"), "{why}");
        assert!(why.contains("MES db,1/config.json"), "{why}");
        assert!(why.contains("260"), "{why}");
    }

    #[test]
    fn windows_read_refuses_reparse_points_and_long_paths() {
        let fake = windows_fake();
        windows_tree(&fake);
        fake.knobs().reparse.push(format!(
            "{WRES}/production/ignition/database-connection/MES db,1"
        ));
        let why = windows_read_failure(&fake);
        assert!(
            why.contains("reparse point") && why.contains("MES db,1"),
            "{why}"
        );

        let fake = windows_fake();
        windows_tree(&fake);
        fake.knobs().enum_len = Some(LONG_PATH_LIMIT + LONG_PATH_PREFIX.len());
        let why = windows_read_failure(&fake);
        assert!(
            why.contains(&format!(
                "a path of {LONG_PATH_LIMIT} characters on the host"
            )),
            "{why}"
        );
        assert!(why.contains("cannot be read"), "{why}");
    }

    #[test]
    fn windows_read_fails_closed_on_walk_failure_and_disagreement() {
        // the walk itself fails (access denied): never "complete"
        let fake = windows_fake();
        windows_tree(&fake);
        fake.knobs().enum_fail = Some(1);
        let why = windows_read_failure(&fake);
        assert!(
            why.contains("cannot check the listing for completeness"),
            "{why}"
        );
        assert!(why.contains("Access to the path is denied"), "{why}");
        assert!(!why.contains("<S S="), "{why}");

        // the host holds an entry sftp never returned
        let fake = windows_fake();
        windows_tree(&fake);
        fake.knobs().enum_extra.push((
            format!("{WRES}/production/ignition/database-connection/MES db,1/extra.json"),
            false,
            3,
        ));
        let why = windows_read_failure(&fake);
        assert!(why.contains("extra.json"), "{why}");

        // sftp and the walk disagree on a size
        let fake = windows_fake();
        windows_tree(&fake);
        fake.knobs().lie_size = Some(("MES db,1/config.json".to_string(), 99));
        let why = windows_read_failure(&fake);
        assert!(why.contains("disagree on the size"), "{why}");

        // an entry outside every scoped root is not part of the check
        let fake = windows_fake();
        windows_tree(&fake);
        fake.knobs().enum_extra.push((
            format!("{WRES}/production/ignition/other-type/ignored.json"),
            false,
            1,
        ));
        let got = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("runtime")
            .block_on(windows_transport(&fake).read(&scope()));
        assert!(got.is_ok(), "{:?}", got.err());
    }

    #[tokio::test]
    async fn windows_ledger_is_read_from_the_account_home_and_skips_unsafe_siblings() {
        let fake = windows_fake();
        // a real Windows profile holds files whose names carry braces
        fake.file(
            &format!("{WHOME}/NTUSER.DAT{{6b3cbf8b-c6cb-11f1-8338-00155d013703}}.TM.blf"),
            "x",
        );
        fake.file(
            &format!("{WHOME}/ign-deploy/gw/production/{LEDGER_FILE}"),
            "{\"schema_version\":1}",
        );
        // and none below the data directory
        let t = windows_transport(&fake);
        assert_eq!(
            t.read_ledger("gw", "production").await.expect("ledger"),
            Some(b"{\"schema_version\":1}".to_vec())
        );
        assert_eq!(t.account_home().await.expect("home"), WHOME);
        // the ledger is NOT read from below the data directory
        let fake = windows_fake();
        fake.file(
            &format!("{WDATA}/ign-deploy/gw/production/{LEDGER_FILE}"),
            "{\"schema_version\":1}",
        );
        assert_eq!(
            windows_transport(&fake)
                .read_ledger("gw", "production")
                .await
                .expect("none"),
            None
        );
    }

    #[test]
    fn account_home_answers_are_validated() {
        // a Windows host reports /X:/...; a POSIX path is not one
        for (answer, ok) in [
            ("/C:/Users/svc", true),
            ("/C:/Users/svc/", true),
            ("/home/svc", false),
            ("relative", false),
            ("/C:/Users/../x", false),
            ("/C:/Users/s*c", false),
            ("/", false),
        ] {
            let fake = windows_fake();
            fake.knobs().home = Some(answer.to_string());
            let got = block_on(windows_transport(&fake).account_home());
            assert_eq!(got.is_ok(), ok, "windows {answer}: {got:?}");
        }
        for (answer, ok) in [
            ("/home/svc", true),
            ("/home/svc/", true),
            ("relative", false),
            ("/home/../x", false),
            ("/home/s*c", false),
            ("/", false),
            // a raw `\n` in the home: the line after it is not ignored
            ("/home/s\nvc", false),
            ("/home/svc\nother", false),
        ] {
            let fake = FakeSftp::new();
            fake.knobs().home = Some(answer.to_string());
            let got = block_on(transport(&fake).account_home());
            assert_eq!(got.is_ok(), ok, "posix {answer}: {got:?}");
        }
    }

    #[tokio::test]
    async fn default_staging_follows_decision_b() {
        let fake = windows_fake();
        let staging = windows_transport(&fake)
            .default_staging("gw", "production", "run-1")
            .await
            .expect("staging");
        assert_eq!(
            staging,
            StagingMode::Separate {
                root: "/C:/Users/svc/ign-deploy/gw/production/staging/run-1".to_string()
            }
        );
        // and it is a root the mirror accepts: outside config/resources
        assert!(
            windows_transport(&fake)
                .with_mirror("production", staging)
                .is_ok()
        );
    }

    // ---- the Windows write half over the scripted server --------------------------

    fn win_tree_of(files: &[(&str, &str)]) -> RenderedTree {
        RenderedTree {
            files: files
                .iter()
                .map(|(p, b)| (rel(p), b.as_bytes().to_vec()))
                .collect(),
        }
    }

    const STAGING: &str = "/C:/Users/svc/ign-deploy/gw/production/staging/run-1";
    const DC: &str = "ignition/database-connection";

    fn mirror(fake: &Arc<FakeSftp>) -> SftpTransport {
        windows_transport(fake)
            .with_mirror(
                "production",
                StagingMode::Separate {
                    root: STAGING.to_string(),
                },
            )
            .expect("mirror")
    }

    fn fixture_collection(fake: &FakeSftp) {
        fake.file(
            &format!("{WRES}/production/config-mode.json"),
            "{\"mode\":1}",
        );
        fake.file(
            &format!("{WRES}/production/{DC}/Old/config.json"),
            "{\"old\":1}",
        );
        fake.file(
            &format!("{WRES}/production/{DC}/Keep/config.json"),
            "{\"keep\":1}",
        );
    }

    fn renames(fake: &FakeSftp) -> Vec<(String, String)> {
        fake.session_lines()
            .into_iter()
            .flatten()
            .filter(|l| l.starts_with("rename "))
            .map(|l| {
                let q = quoted(&l);
                (q[0].clone(), q[1].clone())
            })
            .collect()
    }

    /// Runs sftp against the scripted server and answers every `ssh` with one
    /// canned script result, recording the remote commands it was given.
    struct OneExec {
        fake: Arc<FakeSftp>,
        execs: Mutex<Vec<String>>,
        reply: String,
    }

    #[async_trait::async_trait]
    impl ProcessRunner for OneExec {
        async fn run(
            &self,
            program: &OsStr,
            args: &[OsString],
            stdin: Option<Vec<u8>>,
            timeout: Duration,
        ) -> ProcessOutput {
            if program == "ssh" {
                self.execs
                    .lock()
                    .expect("execs")
                    .push(args.last().expect("command").to_string_lossy().into_owned());
                return ProcessOutput {
                    stdout: format!("IGN-RESULT:{}\n", self.reply).into_bytes(),
                    stderr: Vec::new(),
                    code: 0,
                    timed_out: false,
                };
            }
            self.fake.run(program, args, stdin, timeout).await
        }
    }

    /// D-01: on a Linux node the whole commit is ONE `ssh` call that runs the
    /// uploaded script's `mirror` op; no per-file sftp `rename` is issued, and
    /// the result's indices are mapped back to the paths of the work list.
    #[tokio::test]
    async fn linux_mirror_commit_is_one_exec() {
        for separate in [true, false] {
            let fake = FakeSftp::new();
            fake.mkdirs("/home/svc");
            fake.listed
                .lock()
                .expect("listed")
                .extend(["/home".to_string(), "/home/svc".to_string()]);
            let runner = Arc::new(OneExec {
                fake: fake.clone(),
                execs: Mutex::new(Vec::new()),
                reply: r#"{"ok":1,"moved":[0,1],"deleted":[0],"dirs":[[0,3]],"fail":null}"#
                    .to_string(),
            });
            let shell = RemoteShell::local_sftp_server(
                Path::new("/unused/sftp-server"),
                runner.clone(),
                Duration::from_secs(30),
            );
            let staging = if separate {
                StagingMode::Separate {
                    root: "/home/svc/ign-deploy/gw/production/staging/run-1".to_string(),
                }
            } else {
                StagingMode::InPlace
            };
            let m = SftpTransport::new(shell, DATA.to_string(), "nodes.master")
                .with_mirror("production", staging)
                .expect("mirror");
            let tree = win_tree_of(&[
                (&format!("{DC}/Keep/config.json"), "{\"keep\":2}"),
                (&format!("{DC}/New/config.json"), "{\"new\":1}"),
            ]);
            let staged = m.stage(&tree).await.expect("stage");
            let before = fake.session_lines().len();
            let report = m
                .commit(
                    &staged,
                    &DeleteSet {
                        files: vec![rel(&format!("{DC}/Old/config.json"))],
                    },
                )
                .await
                .expect("commit");

            let execs = runner.execs.lock().expect("execs").clone();
            assert_eq!(
                execs.len(),
                1,
                "separate={separate}: one exec call: {execs:?}"
            );
            assert!(execs[0].contains("'mirror'"), "{}", execs[0]);
            assert!(execs[0].contains("commit.sh"), "{}", execs[0]);
            assert!(
                !execs[0].contains("Old/config.json"),
                "paths never reach the command line: {}",
                execs[0]
            );
            let lines: Vec<String> = fake.session_lines().into_iter().flatten().collect();
            assert!(
                !lines.iter().any(|l| l.starts_with("rename ")),
                "separate={separate}: no per-file rename: {lines:?}"
            );
            assert!(fake.session_lines().len() >= before);
            assert_eq!(
                report.written,
                [
                    format!("{DC}/Keep/config.json"),
                    format!("{DC}/New/config.json")
                ]
            );
            assert_eq!(report.deleted, [format!("{DC}/Old/config.json")]);
            assert_eq!(report.removed_dirs, [format!("{DC}/Old")]);
            // the script removes its own control directory when it exits (the
            // canned reply here never ran it; the integration tests check the
            // real thing), so only what was uploaded is asserted: the script
            // and the work list, below the account home
            let fs = fake.fs.lock().expect("fs");
            let ctl: Vec<&String> = fs.keys().filter(|k| k.contains("/.ign-ctl-")).collect();
            assert!(
                ctl.iter().any(|k| k.ends_with("/commit.sh"))
                    && ctl.iter().any(|k| k.ends_with("/work.tsv")),
                "{ctl:?}"
            );
        }
    }

    /// The decoded parameters of the one `powershell -EncodedCommand` an exec
    /// carried: the base64 JSON blob the fixed stub reads as `$p`.
    fn stub_params(exec: &str) -> (String, serde_json::Value) {
        use base64::Engine as _;
        let b64 = exec
            .strip_prefix("powershell -NoProfile -NonInteractive -EncodedCommand ")
            .or_else(|| {
                exec.split("powershell -NoProfile -NonInteractive -EncodedCommand ")
                    .nth(1)
            })
            .unwrap_or_else(|| panic!("not a powershell command: {exec}"))
            .trim();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .expect("outer base64");
        let units: Vec<u16> = bytes
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let text = String::from_utf16(&units).expect("utf-16");
        let blob = text
            .split("FromBase64String('")
            .nth(1)
            .and_then(|rest| rest.split('\'').next())
            .expect("parameter blob");
        let json = base64::engine::general_purpose::STANDARD
            .decode(blob)
            .expect("blob base64");
        (text, serde_json::from_slice(&json).expect("json"))
    }

    /// The text of the work list the transport uploaded below the account home.
    fn uploaded_work_list(fake: &FakeSftp) -> String {
        let fs = fake.fs.lock().expect("fs");
        let (_, node) = fs
            .iter()
            .find(|(k, _)| k.contains("/.ign-ctl-") && k.ends_with("/work.tsv"))
            .expect("a work list was uploaded");
        match node {
            Node::File(bytes) => String::from_utf8(bytes.clone()).expect("utf-8"),
            _ => panic!("work.tsv is not a file"),
        }
    }

    /// A Windows mirror over `OneExec` answering every `ssh` with `reply`.
    fn windows_mirror_with_reply(
        separate: bool,
        reply: &str,
    ) -> (Arc<FakeSftp>, Arc<OneExec>, SftpTransport) {
        let fake = windows_fake();
        fixture_collection(&fake);
        let runner = Arc::new(OneExec {
            fake: fake.clone(),
            execs: Mutex::new(Vec::new()),
            reply: reply.to_string(),
        });
        let shell = RemoteShell::local_sftp_server(
            Path::new("/unused/sftp-server"),
            runner.clone(),
            Duration::from_secs(30),
        );
        let staging = if separate {
            StagingMode::Separate {
                root: STAGING.to_string(),
            }
        } else {
            StagingMode::InPlace
        };
        let m = SftpTransport::new_windows(shell, WDATA.to_string(), "nodes.master")
            .with_mirror("production", staging)
            .expect("mirror");
        (fake, runner, m)
    }

    /// D-01 on Windows: the whole commit is ONE `ssh` call that runs the
    /// uploaded PowerShell script's `mirror` op; no per-file sftp `rename` is
    /// issued, the command line carries no path, the work list holds the
    /// native staged-files directory and the result's indices are mapped back
    /// to the paths of the work list.
    #[tokio::test]
    async fn windows_mirror_commit_is_one_exec() {
        for separate in [true, false] {
            let (fake, runner, m) = windows_mirror_with_reply(
                separate,
                r#"{"ok":1,"moved":[0,1],"deleted":[0],"dirs":[[0,3]],"fail":null}"#,
            );
            let tree = win_tree_of(&[
                (&format!("{DC}/Keep/config.json"), "{\"keep\":2}"),
                (&format!("{DC}/New One/config.json"), "{\"new\":1}"),
            ]);
            let staged = m.stage(&tree).await.expect("stage");
            let report = m
                .commit(
                    &staged,
                    &DeleteSet {
                        files: vec![rel(&format!("{DC}/Old/config.json"))],
                    },
                )
                .await
                .expect("commit");

            let execs = runner.execs.lock().expect("execs").clone();
            assert_eq!(
                execs.len(),
                1,
                "separate={separate}: one exec call: {execs:?}"
            );
            let (text, params) = stub_params(&execs[0]);
            assert_eq!(params["o"], "mirror");
            assert_eq!(params["c"], "C:\\gw\\data\\config\\resources\\production");
            assert!(
                params["t"]
                    .as_str()
                    .expect("t")
                    .starts_with("C:\\Users\\svc\\.ign-ctl-"),
                "{params}"
            );
            assert!(
                !text.contains("Old") && !text.contains("config.json"),
                "paths never reach the command line"
            );
            assert_eq!(
                params["ss"],
                crate::deploy::transport::batch::script_ps1_sha256()
            );
            let work = uploaded_work_list(&fake);
            let source = work
                .lines()
                .find_map(|l| l.strip_prefix("S\t"))
                .expect("an S record");
            if separate {
                assert_eq!(
                    source,
                    "C:\\Users\\svc\\ign-deploy\\gw\\production\\staging\\run-1"
                );
            } else {
                assert!(
                    source.starts_with("C:\\Users\\svc\\.ign-ctl-") && source.ends_with("\\files"),
                    "{source}"
                );
            }
            assert!(
                work.contains(&format!("\nX\t0\t{DC}/Old/config.json\n")),
                "{work}"
            );
            assert!(
                work.contains("\nT\t3\n"),
                "the retry bound travels in the list"
            );
            let lines: Vec<String> = fake.session_lines().into_iter().flatten().collect();
            assert!(
                !lines.iter().any(|l| l.starts_with("rename ")),
                "separate={separate}: no per-file rename: {lines:?}"
            );
            assert_eq!(
                report.written,
                [
                    format!("{DC}/Keep/config.json"),
                    format!("{DC}/New One/config.json")
                ]
            );
            assert_eq!(report.deleted, [format!("{DC}/Old/config.json")]);
            assert_eq!(report.removed_dirs, [format!("{DC}/Old")]);
            // the script (not this process) touches the collection: the canned
            // reply never ran it, so the fixture is as it was
            let fs = fake.fs.lock().expect("fs");
            assert!(matches!(
                fs.get(&format!("{WRES}/production/{DC}/Keep/config.json")),
                Some(Node::File(b)) if b == b"{\"keep\":1}"
            ));
            let ctl: Vec<&String> = fs.keys().filter(|k| k.contains("/.ign-ctl-")).collect();
            assert!(
                ctl.iter().any(|k| k.ends_with("/commit.ps1"))
                    && ctl.iter().any(|k| k.ends_with("/work.tsv")),
                "{ctl:?}"
            );
        }
    }

    /// Was `windows_commit_replaces_by_rename_and_never_renames_onto_itself`.
    /// A Windows commit no longer renames over sftp (D-01): the replaced file,
    /// the new one, the delete and the untouched `config-mode.json` are the
    /// script's business, which the live host proves; here the scenario is
    /// the report the script answers with and the two rules that were
    /// asserted of the renames: nothing is renamed onto itself (the script
    /// refuses it, and no sftp rename exists to try) and the staged copies
    /// move only from the staging root into the collection.
    #[tokio::test]
    async fn windows_commit_replaces_through_the_script_and_never_renames_over_sftp() {
        let (fake, runner, m) = windows_mirror_with_reply(
            true,
            r#"{"ok":1,"moved":[0,1],"deleted":[0],"dirs":[],"fail":null}"#,
        );
        let tree = win_tree_of(&[
            (&format!("{DC}/Keep/config.json"), "{\"keep\":2}"),
            (&format!("{DC}/New One/config.json"), "{\"new\":1}"),
        ]);
        let staged = m.stage(&tree).await.expect("stage");
        let report = m
            .commit(
                &staged,
                &DeleteSet {
                    files: vec![rel(&format!("{DC}/Old/config.json"))],
                },
            )
            .await
            .expect("commit");
        assert_eq!(report.written.len(), 2);
        assert_eq!(report.deleted, [format!("{DC}/Old/config.json")]);
        assert_eq!(runner.execs.lock().expect("execs").len(), 1);
        assert_eq!(renames(&fake).len(), 0, "no sftp rename on a Windows host");
        // the staged copies are on the host before the call, in the staging
        // root, and nowhere inside the collection
        let fs = fake.fs.lock().expect("fs");
        for p in ["Keep/config.json", "New One/config.json"] {
            assert!(
                matches!(fs.get(&format!("{STAGING}/{DC}/{p}")), Some(Node::File(_))),
                "{p} is staged"
            );
        }
        assert!(
            fs.keys()
                .filter(|k| k.starts_with(&format!("{WRES}/production/")))
                .all(|k| !k.rsplit('/').next().expect("name").starts_with('.')),
            "nothing dot-named in the collection"
        );
        // the script itself refuses a move onto itself and keeps the config
        // file of the gateway out of every list (the work list gate)
        assert!(
            crate::deploy::transport::batch::SCRIPT_PS1.contains("a file cannot move onto itself")
        );
    }

    /// Was `windows_commit_retries_a_held_rename_a_bounded_number_of_times`
    /// (a held rename retried by the transport, three segments). The retry is
    /// the script's now: the work list's `T` record carries the bound, a move
    /// that still fails comes back as the script's own result and is reported
    /// as `transport_failed` naming the entry by its path, saying nothing was
    /// rolled back, and the transport itself never repeats the call or issues
    /// an sftp `rename`.
    #[tokio::test]
    async fn windows_commit_holds_retry_in_the_script_and_reports_a_failed_move() {
        // segment 1 (held twice, then success): the script retried; the bound
        // is in the list, the call is made once
        let (fake, runner, m) = windows_mirror_with_reply(
            true,
            r#"{"ok":1,"moved":[0],"deleted":[],"dirs":[],"fail":null}"#,
        );
        let staged = m
            .stage(&win_tree_of(&[(
                &format!("{DC}/Keep/config.json"),
                "{\"keep\":2}",
            )]))
            .await
            .expect("stage");
        m.commit(&staged, &DeleteSet::default())
            .await
            .expect("commit");
        assert!(uploaded_work_list(&fake).contains(&format!("\nT\t{RENAME_RETRIES}\n")));
        assert_eq!(runner.execs.lock().expect("execs").len(), 1);
        assert_eq!(renames(&fake).len(), 0);

        // segment 2 (held for good) and 3 (permission denied): the script's
        // failure comes back as transport_failed naming the path; not
        // repeated by the transport
        for fail in [
            "write 0: cannot move the file into place",
            "write 0: cannot reset the permissions of the file",
        ] {
            let (fake, runner, m) = windows_mirror_with_reply(
                true,
                &format!(r#"{{"ok":1,"moved":[],"deleted":[],"dirs":[],"fail":"{fail}"}}"#),
            );
            let staged = m
                .stage(&win_tree_of(&[(
                    &format!("{DC}/Keep/config.json"),
                    "{\"keep\":2}",
                )]))
                .await
                .expect("stage");
            let err = m
                .commit(&staged, &DeleteSet::default())
                .await
                .expect_err("held for good");
            let CoreError::TransportFailed {
                operation, detail, ..
            } = err
            else {
                panic!("not transport_failed")
            };
            assert_eq!(operation, "commit");
            assert!(
                detail.contains(&format!("{DC}/Keep/config.json")),
                "{detail}"
            );
            assert!(detail.contains("nothing was rolled back"), "{detail}");
            assert_eq!(runner.execs.lock().expect("execs").len(), 1, "{fail}");
            assert_eq!(renames(&fake).len(), 0, "no sftp rename on a Windows host");
            // the old content is still there
            let fs = fake.fs.lock().expect("fs");
            assert!(matches!(
                fs.get(&format!("{WRES}/production/{DC}/Keep/config.json")),
                Some(Node::File(b)) if b == b"{\"keep\":1}"
            ));
        }
    }

    /// 23-20: every `ApplyHost` call of a Windows node runs the PowerShell
    /// script (`probe`, `fingerprint`, `finalize`, `discard_interrupted`) or
    /// reads over sftp outside the lock (`read_record`, `fetch_snapshot`); none
    /// is refused as unsupported any more.
    #[tokio::test]
    async fn windows_apply_calls_run_the_script_or_read_over_sftp() {
        use crate::deploy::apply::host::ApplyHost;
        use crate::deploy::apply::run_store::RunId;
        let run = RunId::parse("20261017T120000Z-aaaaaa").expect("run id");
        let host = |reply: &str| {
            let fake = windows_fake();
            let runner = Arc::new(OneExec {
                fake: fake.clone(),
                execs: Mutex::new(Vec::new()),
                reply: reply.to_string(),
            });
            let shell = RemoteShell::local_sftp_server(
                Path::new("/unused/sftp-server"),
                runner.clone(),
                Duration::from_secs(30),
            );
            let t = SftpTransport::new_windows(shell, WDATA.to_string(), "nodes.master")
                .with_apply("gw", "production")
                .expect("apply half");
            (fake, runner, t)
        };

        // probe: the script's problem words come back as sentences
        let (_, runner, t) = host(r#"{"ok":1,"problems":["coll_unwritable","deploy_unwritable"]}"#);
        let report = t.probe().await.expect("probe");
        assert!(!report.writable);
        assert_eq!(report.problems.len(), 2, "{:?}", report.problems);
        let execs = runner.execs.lock().expect("execs").clone();
        assert_eq!(execs.len(), 1);
        let (_, params) = stub_params(&execs[0]);
        assert_eq!(params["o"], "probe");
        assert_eq!(params["d"], "C:\\Users\\svc\\ign-deploy\\gw\\production");
        assert_eq!(params["c"], "C:\\gw\\data\\config\\resources\\production");
        let (_, runner, t) = host(r#"{"ok":1,"problems":[]}"#);
        assert!(t.probe().await.expect("probe").writable);
        assert_eq!(runner.execs.lock().expect("execs").len(), 1);

        // fingerprint: hashes by path; a held file is a refusal naming the path
        let hash = "ab".repeat(32);
        let (fake, runner, t) = host(&format!(r#"{{"ok":1,"hashes":["{hash}","-"]}}"#));
        let got = t
            .fingerprint(&[format!("{DC}/Keep/config.json"), "ledger.json".to_string()])
            .await
            .expect("fingerprint");
        assert_eq!(
            got.get(&format!("{DC}/Keep/config.json")),
            Some(&Some(hash))
        );
        assert_eq!(got.get("ledger.json"), Some(&None));
        assert_eq!(runner.execs.lock().expect("execs").len(), 1);
        assert!(uploaded_work_list(&fake).contains("\nO\tfingerprint\n"));
        let (_, _, t) = host(r#"{"ok":0,"err":"held","detail":"x","bad":["P0"]}"#);
        let err = t
            .fingerprint(&[format!("{DC}/Keep/config.json")])
            .await
            .expect_err("held");
        assert!(
            err.to_string().contains(&format!("{DC}/Keep/config.json"))
                && err.to_string().contains("holds it open"),
            "{err}"
        );

        // discard_interrupted: no journal on the host, the script drops the run
        let (fake, runner, t) = host(r#"{"ok":1}"#);
        t.discard_interrupted(&run).await.expect("discard");
        assert_eq!(runner.execs.lock().expect("execs").len(), 1);
        assert!(uploaded_work_list(&fake).contains("\nO\tdiscard\n"));

        // read_record and fetch_snapshot read over sftp and start no process
        let (fake, runner, t) = host(r#"{"ok":1}"#);
        assert!(t.read_record(&run).await.expect("read_record").is_none());
        let dest = tempfile::tempdir().expect("dest");
        let err = t
            .fetch_snapshot(&run, dest.path())
            .await
            .expect_err("no snapshot");
        assert!(
            err.to_string().contains("has no snapshot on this host"),
            "{err}"
        );
        assert!(!err.to_string().contains("not supported"), "{err}");
        assert!(t.sessions() > 0, "they read over sftp");
        assert_eq!(runner.execs.lock().expect("execs").len(), 0, "no ssh exec");
        let _ = fake;
        // the script the Windows family runs is the PowerShell one
        let (name, text, sha) = t.script_file_for_tests();
        assert_eq!(name, "commit.ps1");
        assert_eq!(text, crate::deploy::transport::batch::SCRIPT_PS1);
        assert_eq!(sha, crate::deploy::transport::batch::script_ps1_sha256());
    }

    /// The `write_host` preflight reaches the Windows host through
    /// `probe_paths`: a touched path whose copy below the account home the sftp
    /// server could not serve again is a refusal that names the path, decided
    /// in Rust from the account home alone, and writes nothing.
    #[tokio::test]
    async fn windows_probe_paths_refuses_a_path_the_sftp_server_could_not_reach() {
        use crate::deploy::apply::host::ApplyHost;
        let fake = windows_fake();
        let t = windows_transport(&fake)
            .with_apply("gw", "production")
            .expect("apply half");
        let long = format!(
            "ignition/database-connection/{}/config.json",
            "n".repeat(260)
        );
        let report = t
            .probe_paths(&[
                "ignition/database-connection/ok/config.json".to_string(),
                long.clone(),
            ])
            .await
            .expect("the gate answers");
        assert!(!report.writable);
        assert_eq!(report.problems.len(), 1, "{:?}", report.problems);
        assert!(
            report.problems[0].starts_with(&long),
            "{:?}",
            report.problems
        );
        // nothing was written: the only sftp work is the `pwd` for the home
        assert!(
            fake.sessions
                .lock()
                .expect("sessions")
                .iter()
                .flatten()
                .all(|c| ["put", "mkdir", "rename", "rm"]
                    .iter()
                    .all(|w| !c.trim_start_matches('@').starts_with(w))),
            "the gate writes nothing"
        );
        // a path that fits goes on to the script's own probe
        let runner = Arc::new(OneExec {
            fake: fake.clone(),
            execs: Mutex::new(Vec::new()),
            reply: r#"{"ok":1,"problems":[]}"#.to_string(),
        });
        let shell = RemoteShell::local_sftp_server(
            Path::new("/unused/sftp-server"),
            runner.clone(),
            Duration::from_secs(30),
        );
        let t = SftpTransport::new_windows(shell, WDATA.to_string(), "nodes.master")
            .with_apply("gw", "production")
            .expect("apply half");
        let report = t
            .probe_paths(&["ignition/database-connection/ok/config.json".to_string()])
            .await
            .expect("the script probe answers");
        assert!(report.writable, "{report:?}");
        assert_eq!(runner.execs.lock().expect("execs").len(), 1);
    }

    /// Was the POSIX segment of `windows_commit_retries_a_held_rename_a_bounded_number_of_times`
    /// (now `windows_commit_holds_retry_in_the_script_and_reports_a_failed_move`)
    /// ("a POSIX host does not retry at all", one sftp `rename` that failed).
    /// A Linux commit no longer renames over sftp at all (D-01): the script
    /// moves, retrying a held target as many times as the work list's `T`
    /// record says, and a move that still fails comes back as the script's
    /// own result, which the transport reports as `transport_failed` naming
    /// the entry by its path and saying nothing was rolled back.
    #[tokio::test]
    async fn linux_commit_never_renames_over_sftp_and_reports_a_failed_move() {
        let fake = FakeSftp::new();
        fake.mkdirs("/home/svc/ign-deploy/gw/production/staging");
        fake.listed
            .lock()
            .expect("listed")
            .extend(["/home".to_string(), "/home/svc".to_string()]);
        let runner = Arc::new(OneExec {
            fake: fake.clone(),
            execs: Mutex::new(Vec::new()),
            reply: r#"{"ok":1,"moved":[],"deleted":[],"dirs":[],"fail":"write 0: cannot move the file into place"}"#
                .to_string(),
        });
        let shell = RemoteShell::local_sftp_server(
            Path::new("/unused/sftp-server"),
            runner.clone(),
            Duration::from_secs(30),
        );
        let m = SftpTransport::new(shell, DATA.to_string(), "nodes.master")
            .with_mirror(
                "production",
                StagingMode::Separate {
                    root: "/home/svc/ign-deploy/gw/production/staging/run-1".to_string(),
                },
            )
            .expect("mirror");
        let staged = m
            .stage(&win_tree_of(&[(
                &format!("{DC}/Keep/config.json"),
                "{\"keep\":2}",
            )]))
            .await
            .expect("stage");
        let err = m
            .commit(&staged, &DeleteSet::default())
            .await
            .expect_err("held for good");
        let CoreError::TransportFailed {
            operation, detail, ..
        } = err
        else {
            panic!("not transport_failed")
        };
        assert_eq!(operation, "commit");
        assert!(
            detail.contains(&format!("{DC}/Keep/config.json")),
            "{detail}"
        );
        assert!(detail.contains("nothing was rolled back"), "{detail}");
        assert_eq!(renames(&fake).len(), 0, "no sftp rename on a POSIX host");
        assert_eq!(runner.execs.lock().expect("execs").len(), 1);
    }

    #[tokio::test]
    async fn windows_staging_inside_the_collection_is_refused_whatever_its_case() {
        let fake = windows_fake();
        for root in [
            "/c:/GW/Data/config/resources/staging/run-1",
            "/C:/gw/data/config/resources",
            "/C:/gw",
        ] {
            let err = windows_transport(&fake)
                .with_mirror(
                    "production",
                    StagingMode::Separate {
                        root: root.to_string(),
                    },
                )
                .err()
                .unwrap_or_else(|| panic!("{root} refused"));
            assert!(err.contains("staging root"), "{root}: {err}");
        }
    }

    // ---- post-review: path gate parity and component boundaries ----------------

    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("runtime")
            .block_on(future)
    }

    #[test]
    fn windows_listing_names_pass_the_windows_gate() {
        let one = vec!["/C:/d".to_string()];
        let listing = |name: &str| {
            format!(
                "Remote working directory: /C:/h\n-rw-******    1 -        -   8 Oct 15 02:35 {name}\n"
            )
        };
        for bad in [
            "a:b",
            "name:stream",
            "trail.",
            "trail ",
            "NUL",
            "aux.txt",
            "a<b",
            "a|b",
        ] {
            let text = listing(bad);
            assert!(
                parse_batch_listing_as(
                    ListFormat::Windows,
                    NameRule::Refuse,
                    &one,
                    text.as_bytes(),
                    100
                )
                .is_err(),
                "{bad:?} must be refused"
            );
            let kept = parse_batch_listing_as(
                ListFormat::Windows,
                NameRule::Skip,
                &one,
                text.as_bytes(),
                100,
            )
            .expect("skip")
            .remove(0);
            assert!(kept.is_empty(), "{bad:?} must be skipped, not kept");
        }
        // one directory cannot hold two names that differ only by case
        let twice = "Remote working directory: /C:/h\n\
-rw-******    1 -        -   8 Oct 15 02:35 Config.json\n\
-rw-******    1 -        -   9 Oct 15 02:35 config.JSON\n";
        assert!(
            parse_batch_listing_as(
                ListFormat::Windows,
                NameRule::Refuse,
                &one,
                twice.as_bytes(),
                100
            )
            .is_err()
        );
        // the POSIX format keeps its rules: a colon is an ordinary name there
        let posix =
            format!("{PWD_MARKER}/home/u\n-rw-r--r--    ? svc staff 8 Oct 14 10:21 /d/a:b\n");
        assert!(parse_batch_listing(&["/d".to_string()], posix.as_bytes()).is_ok());
    }

    #[test]
    fn windows_home_and_staging_roots_pass_the_windows_path_gate() {
        for answer in [
            "/C:/Users/svc/D:/x",
            "/Users/svc",
            "/C:/Users/svc.",
            "/C:/Users/svc/NUL",
            "/C:/Users/svc/a:stream",
            "/C:",
        ] {
            let fake = windows_fake();
            fake.knobs().home = Some(answer.to_string());
            let got = block_on(windows_transport(&fake).account_home());
            assert!(got.is_err(), "{answer}: {got:?}");
        }
        // the POSIX transport is untouched by the Windows gate
        let fake = FakeSftp::new();
        fake.knobs().home = Some("/home/svc:1".to_string());
        fake.listed
            .lock()
            .expect("listed")
            .insert("/home/svc:1".to_string());
        assert!(block_on(transport(&fake).account_home()).is_ok());

        let fake = windows_fake();
        for root in [
            "/C:/Users/svc/D:/x",
            "/C:/Users/svc/x:stream",
            "/C:/Users/svc/NUL",
            "/C:/Users/svc/staging.",
            "/C:/Users/../x",
            "/Users/svc/staging",
        ] {
            let got = windows_transport(&fake).with_mirror(
                "production",
                StagingMode::Separate {
                    root: root.to_string(),
                },
            );
            assert!(got.is_err(), "{root} must be refused");
        }
        // a sibling is not the thing it is next to, whatever its case
        for root in [
            "/C:/gw/data/config/resources2/staging",
            "/c:/GW/DATA/config/Resources-evil/staging",
            "/C:/gw/data2/staging",
            "/C:/gw/data-staging/x",
        ] {
            let got = windows_transport(&fake).with_mirror(
                "production",
                StagingMode::Separate {
                    root: root.to_string(),
                },
            );
            assert!(
                got.is_ok(),
                "{root} is a sibling, not inside: {:?}",
                got.err()
            );
        }
        // and the real thing is inside, whatever its case and separators
        for root in [
            "/c:/GW/Data/CONFIG/resources/x/y",
            "/C:/gw/data/config/resources/",
        ] {
            let got = windows_transport(&fake).with_mirror(
                "production",
                StagingMode::Separate {
                    root: root.to_string(),
                },
            );
            assert!(got.is_err(), "{root} is inside config/resources");
        }
    }

    #[test]
    fn windows_components_pass_the_windows_gate_before_any_session() {
        let fake = windows_fake();
        let t = windows_transport(&fake);
        for (mode, provider, module, ty) in [
            ("prod:x", None, "ignition", "database-connection"),
            ("production.", None, "ignition", "database-connection"),
            ("production", Some("p:q"), "ignition", "database-connection"),
            ("production", Some("NUL"), "ignition", "database-connection"),
            ("production", None, "ignition:x", "database-connection"),
            ("production", None, "ignition", "con"),
            ("production", None, "ignition", "type "),
        ] {
            let scope = Scope {
                mode: mode.to_string(),
                types: vec![(module.to_string(), ty.to_string())],
                tag_providers: provider.map(|p| vec![p.to_string()]).unwrap_or_default(),
            };
            assert!(
                block_on(t.read(&scope)).is_err(),
                "{mode}/{provider:?}/{module}/{ty}"
            );
        }
        assert!(block_on(t.list_types("core:x")).is_err());
        assert!(
            fake.session_lines().is_empty(),
            "refused before any process was spawned"
        );
        // the ledger: gateway and mode
        for (gateway, mode) in [("gw:x", "production"), ("gw", "NUL"), ("gw.", "production")] {
            let err = block_on(t.read_ledger(gateway, mode)).expect_err(gateway);
            assert!(
                matches!(err, CoreError::DeployLedgerInvalid { .. }),
                "{err:?}"
            );
        }
        assert!(fake.session_lines().is_empty());
        // the default staging: gateway, mode and run id
        for (gateway, mode, run) in [
            ("gw:x", "production", "r"),
            ("gw", "NUL", "r"),
            ("gw", "production", "r:1"),
            ("gw", "production", "run."),
            ("gw", "production", "CON"),
        ] {
            assert!(
                block_on(t.default_staging(gateway, mode, run)).is_err(),
                "{gateway}/{mode}/{run}"
            );
        }
        assert!(fake.session_lines().is_empty());
        // a mirror for a mode the host cannot name
        assert!(
            windows_transport(&fake)
                .with_mirror("production.", StagingMode::InPlace)
                .is_err()
        );
    }

    #[tokio::test]
    async fn windows_staged_and_deleted_paths_pass_the_windows_gate_and_write_nothing() {
        let fake = windows_fake();
        fixture_collection(&fake);
        let m = mirror(&fake);
        for bad in [
            format!("{DC}/NUL/config.json"),
            format!("{DC}/a:stream/config.json"),
            format!("{DC}/trail./config.json"),
            format!("{DC}/x/aux.json"),
        ] {
            let staged = m.stage(&win_tree_of(&[(&bad, "{}")])).await;
            assert!(staged.is_err(), "{bad} must not be staged");
        }
        let staged = m
            .stage(&win_tree_of(&[(
                &format!("{DC}/Keep/config.json"),
                "{\"k\":3}",
            )]))
            .await
            .expect("a clean path stages");
        let err = m
            .commit(
                &staged,
                &DeleteSet {
                    files: vec![rel(&format!("{DC}/NUL/config.json"))],
                },
            )
            .await
            .expect_err("a delete-set path the host cannot name");
        assert!(matches!(err, CoreError::TransportFailed { .. }));
        let fs = fake.fs.lock().expect("fs");
        assert!(matches!(
            fs.get(&format!("{WRES}/production/{DC}/Keep/config.json")),
            Some(Node::File(b)) if b == b"{\"keep\":1}"
        ));
    }

    #[test]
    fn windows_cross_check_does_not_let_a_sibling_satisfy_another() {
        let two_types = || Scope {
            mode: "production".to_string(),
            types: vec![
                ("ignition".to_string(), "database-connection".to_string()),
                ("ignition".to_string(), "database-connection2".to_string()),
            ],
            tag_providers: Vec::new(),
        };
        let fake = windows_fake();
        windows_tree(&fake);
        fake.file(
            &format!("{WRES}/production/ignition/database-connection2/Other/config.json"),
            "{\"o\":1}",
        );
        let ok = block_on(windows_transport(&fake).read(&two_types())).expect("siblings read");
        assert_eq!(
            ok.mode.len(),
            4,
            "3 files of the first type, 1 of its sibling"
        );

        // an entry the host holds in the SIBLING type that sftp dropped is not
        // covered by the first type's listing
        let fake = windows_fake();
        windows_tree(&fake);
        fake.file(
            &format!("{WRES}/production/ignition/database-connection2/Other/config.json"),
            "{\"o\":1}",
        );
        fake.knobs().hidden.push(format!(
            "{WRES}/production/ignition/database-connection2/Other/config.json"
        ));
        let err = block_on(windows_transport(&fake).read(&two_types())).expect_err("hidden");
        let why = detail(err);
        assert!(
            why.contains("database-connection2/Other/config.json"),
            "{why}"
        );

        // a host entry whose name differs only in case or by a suffix is a
        // different entry: it is reported, not matched
        for sibling in [
            "mes db,1/config.json",
            "MES db,10/config.json",
            "MES db,1/config.json2",
        ] {
            let fake = windows_fake();
            windows_tree(&fake);
            fake.knobs().enum_extra.push((
                format!("{WRES}/production/ignition/database-connection/{sibling}"),
                false,
                9,
            ));
            let why = windows_read_failure(&fake);
            assert!(why.contains(sibling), "{sibling}: {why}");
        }
    }

    #[test]
    fn shown_paths_respect_the_data_dir_component_boundary() {
        let fake = windows_fake();
        let t = windows_transport(&fake);
        assert_eq!(t.shown("/C:/gw/data/config"), "config");
        assert_eq!(t.shown("/C:/gw/data"), ".");
        assert_eq!(
            t.shown("/C:/gw/data-evil/config"),
            ".",
            "a sibling is not below data_dir"
        );
        assert_eq!(t.shown("/C:/gw/data2"), ".");
    }
}
