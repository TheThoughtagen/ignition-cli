//! The read archive of a Windows deploy read (quick 261105-dl1).
//!
//! A Windows OpenSSH sftp server costs about 0.37 s per `get`, so a read
//! that fetched each listed file with its own `get` took about 88 minutes at
//! production shape (14,270 files). A Windows read therefore fetches its
//! files as one archive: [`render_list`] writes the exact list the listing
//! produced, the host zips exactly those files ([`winps::BUNDLE`]), one sftp
//! `get` downloads the archive, and [`extract`] unpacks it into the read's
//! private download dir. The listing is never redone on the host: the list is
//! uploaded, and BUNDLE checks its SHA-256 before it reads a name from it.
//!
//! ## Threat model
//!
//! The host controls every byte of the archive: entry names, types, flags,
//! declared sizes, the compressed data and the framing. Nothing in it is
//! trusted until [`extract`] has checked it, in this order:
//!
//! 1. Framing: the archive is a zip the crate can open, with no data before
//!    its first entry, and its central directory holds exactly as many
//!    records as the crate reports, no name twice (the crate keys entries by
//!    name and silently keeps one of two records that share it).
//! 2. Every central-directory record, before any byte is inflated: the name
//!    rules (UTF-8, no backslash, not absolute, not a directory entry, no
//!    drive or stream `:`, no empty, `.` or `..` component), a regular-file
//!    type, not encrypted, Stored or Deflated.
//! 3. Membership both ways: no entry that was not listed, then no listed file
//!    missing from the archive.
//! 4. Every declared size equals the listed size.
//! 5. Inflation: each entry is copied through a limit of its listed size plus
//!    one byte, and the running total may not pass the listed total plus the
//!    slack, so a zip bomb stops at the listed bytes.
//!
//! A file is created only below the destination, from the listed name's own
//! components (which passed the listing's name gates), with `create_new`. The
//! caller then runs the same final check as the per-file path
//! (`verify_download`: listed against actual, exact sizes, hashes).
//!
//! [`winps::BUNDLE`]: super::winps::BUNDLE

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use super::READ_MAX_ENTRIES;

/// The uploaded list's name in the staging dir.
pub const LIST_FILE: &str = "read.tsv";
/// The archive's name in the staging dir.
pub const ARCHIVE_FILE: &str = "read.zip";
/// Free bytes BUNDLE requires on the archive's drive beyond the archive's own
/// bound, so a read never fills the host's disk (a production master runs
/// near 88% full).
pub const STAGING_FREE_MARGIN: u64 = 256 * 1024 * 1024;

/// The `S_IFMT` mask and the regular-file type of a unix mode.
const S_IFMT: u32 = 0o170_000;
const S_IFREG: u32 = 0o100_000;
/// The "version made by" host of MS-DOS and Windows (APPNOTE 4.4.2).
const HOST_DOS: u8 = 0;

/// One file of a read archive: `{collection}/{rel}` with `/`, and its listed
/// size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleFile {
    pub collection: String,
    /// Collection-relative path, `/`-separated.
    pub rel: String,
    pub size: u64,
}

impl BundleFile {
    /// The archive entry name.
    pub fn name(&self) -> String {
        format!("{}/{}", self.collection, self.rel)
    }
}

/// The list BUNDLE reads: one line per file in fetch order, `{size}` TAB
/// `{collection}/{rel}` LF, UTF-8, no header. A listed name holds no TAB or
/// LF (it passed `safe_remote_path`).
pub fn render_list(files: &[BundleFile]) -> Vec<u8> {
    let mut out = Vec::new();
    for file in files {
        out.extend_from_slice(file.size.to_string().as_bytes());
        out.push(b'\t');
        out.extend_from_slice(file.name().as_bytes());
        out.push(b'\n');
    }
    out
}

/// The largest archive a faithful host can build of `files`.
///
/// - Data: Deflate never grows a block by more than its stored-block framing
///   (5 bytes per 65,535) and an empty file is 2 bytes; `total / 64` covers
///   that with room to spare.
/// - Per entry: a 30-byte local header and a 46-byte central record, each
///   with the name, plus a Zip64 extra field in each (up to 32 bytes) and a
///   data descriptor (up to 24 bytes): `2 x name + 192` covers them.
/// - Once: the end record (22), the Zip64 end record and locator (76) and a
///   comment: 4,096.
pub fn archive_limit(files: &[BundleFile]) -> u64 {
    let total: u64 = files.iter().map(|f| f.size).sum();
    let per_entry: u64 = files.iter().map(|f| 2 * f.name().len() as u64 + 192).sum();
    total + total / 64 + per_entry + 4096
}

/// What the central-directory walk saw of one record.
struct Record {
    name: Vec<u8>,
    /// The high byte of "version made by": the host that wrote the entry.
    host: u8,
    external: u32,
    local_offset: u64,
}

/// Why a name from the archive cannot be used, by the module's name rules.
fn name_problem(raw: &[u8]) -> Option<&'static str> {
    let Ok(name) = std::str::from_utf8(raw) else {
        return Some("is not UTF-8");
    };
    if name.contains('\\') {
        Some("holds a backslash")
    } else if name.starts_with('/') {
        Some("is an absolute path")
    } else if name.ends_with('/') {
        Some("is a directory entry")
    } else if name.contains(':') {
        Some("names a drive or a stream")
    } else if name
        .split('/')
        .any(|c| c.is_empty() || c == "." || c == "..")
    {
        Some("has an empty, `.` or `..` component")
    } else {
        None
    }
}

fn shown(raw: &[u8]) -> String {
    format!("{:?}", String::from_utf8_lossy(raw))
}

/// Walk the central directory from `start` to the end of `file` (`len`
/// bytes): every record's name, host, external attributes and local header
/// offset, at most `READ_MAX_ENTRIES + 1` records. Every length is checked
/// against the bytes present.
fn central_records(file: &mut std::fs::File, start: u64, len: u64) -> Result<Vec<Record>, String> {
    let unreadable = |e: std::io::Error| format!("the read archive is not a readable zip ({e})");
    if start > len {
        return Err(
            "the read archive is not a readable zip (its central directory is outside it)"
                .to_string(),
        );
    }
    file.seek(SeekFrom::Start(start)).map_err(unreadable)?;
    let mut bytes = Vec::new();
    file.take(len - start)
        .read_to_end(&mut bytes)
        .map_err(unreadable)?;
    let le16 = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]) as usize;
    let le32 =
        |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let mut records = Vec::new();
    let mut at = 0usize;
    while at + 4 <= bytes.len() && bytes[at..at + 4] == [0x50, 0x4b, 0x01, 0x02] {
        if records.len() > READ_MAX_ENTRIES {
            return Err(format!(
                "the read archive holds more than {READ_MAX_ENTRIES} entries"
            ));
        }
        if at + 46 > bytes.len() {
            return Err(
                "the read archive is not a readable zip (a central record is cut off)".to_string(),
            );
        }
        let name_len = le16(at + 28);
        let next = at + 46 + name_len + le16(at + 30) + le16(at + 32);
        if next > bytes.len() {
            return Err(
                "the read archive is not a readable zip (a central record is cut off)".to_string(),
            );
        }
        let mut local_offset = u64::from(le32(at + 42));
        if local_offset == u64::from(u32::MAX) {
            // the real offset is in the Zip64 extra field of the record
            local_offset = zip64_offset(
                &bytes[at + 46 + name_len..at + 46 + name_len + le16(at + 30)],
                le32(at + 24),
                le32(at + 20),
            )
            .ok_or("the read archive is not a readable zip (a Zip64 record is malformed)")?;
        }
        records.push(Record {
            name: bytes[at + 46..at + 46 + name_len].to_vec(),
            host: bytes[at + 5],
            external: le32(at + 38),
            local_offset,
        });
        at = next;
    }
    Ok(records)
}

/// The local header offset in a Zip64 extended-information extra field
/// (`extra` of a central record whose 32-bit offset is `0xFFFFFFFF`). The
/// field holds, in order, only the values whose 32-bit slots are
/// `0xFFFFFFFF`: uncompressed size, compressed size, then the offset.
fn zip64_offset(extra: &[u8], uncompressed: u32, compressed: u32) -> Option<u64> {
    let mut at = 0usize;
    while at + 4 <= extra.len() {
        let id = u16::from_le_bytes([extra[at], extra[at + 1]]);
        let size = usize::from(u16::from_le_bytes([extra[at + 2], extra[at + 3]]));
        let body = extra.get(at + 4..at + 4 + size)?;
        if id == 0x0001 {
            let mut skip = 0usize;
            if uncompressed == u32::MAX {
                skip += 8;
            }
            if compressed == u32::MAX {
                skip += 8;
            }
            let value = body.get(skip..skip + 8)?;
            return Some(u64::from_le_bytes(value.try_into().ok()?));
        }
        at += 4 + size;
    }
    None
}

/// Extract the read archive at `archive` into `dest/<collection>/<rel>` for
/// exactly `files`, under the module's checks, and return the bytes written.
/// `slack` is how far past the listed total the inflated bytes may run in
/// total before extraction stops (the per-entry limit is exact).
pub fn extract(
    archive: &Path,
    files: &[BundleFile],
    dest: &Path,
    slack: u64,
) -> Result<u64, String> {
    let opened = std::fs::File::open(archive)
        .map_err(|e| format!("the read archive cannot be opened ({:?})", e.kind()))?;
    let len = opened
        .metadata()
        .map_err(|e| format!("the read archive cannot be inspected ({:?})", e.kind()))?
        .len();
    let mut walk = opened
        .try_clone()
        .map_err(|e| format!("the read archive cannot be opened ({:?})", e.kind()))?;
    let mut zip = zip::ZipArchive::new(opened)
        .map_err(|e| format!("the read archive is not a readable zip ({e})"))?;

    // 1. framing
    if zip.offset() != 0 {
        return Err("the read archive has data before its first entry".to_string());
    }
    let records = central_records(&mut walk, zip.central_directory_start(), len)?;
    let mut seen: BTreeSet<&[u8]> = BTreeSet::new();
    for record in &records {
        if !seen.insert(&record.name) {
            return Err(format!(
                "the read archive names {} twice",
                shown(&record.name)
            ));
        }
    }
    if records.len() != zip.len() {
        return Err(format!(
            "the read archive's central directory holds {} records for {} entries; an entry is named twice",
            records.len(),
            zip.len()
        ));
    }
    if records.iter().map(|r| r.local_offset).min().unwrap_or(0) != 0 {
        return Err("the read archive has data before its first entry".to_string());
    }
    let by_name: BTreeMap<&[u8], &Record> =
        records.iter().map(|r| (r.name.as_slice(), r)).collect();

    // 2. every record: name, type, encryption, method; nothing inflated
    let mut names: Vec<Vec<u8>> = Vec::with_capacity(zip.len());
    for index in 0..zip.len() {
        let entry = zip
            .by_index_raw(index)
            .map_err(|e| format!("the read archive is not a readable zip ({e})"))?;
        let raw = entry.name_raw().to_vec();
        if let Some(why) = name_problem(&raw) {
            return Err(format!("the read archive entry {} {why}", shown(&raw)));
        }
        let record = by_name.get(raw.as_slice()).ok_or_else(|| {
            format!(
                "the read archive entry {} is not in its central directory",
                shown(&raw)
            )
        })?;
        let regular = match entry.unix_mode() {
            Some(mode) => mode & S_IFMT == S_IFREG,
            // .NET Framework (Windows PowerShell 5.1) writes MS-DOS-made
            // records with no attributes at all; the crate reports no mode
            None => record.host == HOST_DOS && record.external == 0,
        };
        if !regular {
            return Err(format!(
                "the read archive entry {} is not a regular file",
                shown(&raw)
            ));
        }
        if entry.encrypted() {
            return Err(format!(
                "the read archive entry {} is encrypted",
                shown(&raw)
            ));
        }
        if !matches!(
            entry.compression(),
            zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
        ) {
            return Err(format!(
                "the read archive entry {} uses an unsupported compression method",
                shown(&raw)
            ));
        }
        names.push(raw);
    }

    // 3. membership both ways
    let listed: BTreeMap<String, &BundleFile> = files.iter().map(|f| (f.name(), f)).collect();
    for raw in &names {
        let name = std::str::from_utf8(raw).unwrap_or_default();
        if !listed.contains_key(name) {
            return Err(format!(
                "the read archive holds {}, which was not listed",
                shown(raw)
            ));
        }
    }
    let present: BTreeSet<&str> = names
        .iter()
        .map(|raw| std::str::from_utf8(raw).unwrap_or_default())
        .collect();
    for file in files {
        if !present.contains(file.name().as_str()) {
            return Err(format!(
                "config/resources/{} was listed but is not in the read archive",
                file.name()
            ));
        }
    }

    // 4. declared sizes
    for (index, raw) in names.iter().enumerate() {
        let name = std::str::from_utf8(raw).unwrap_or_default();
        let file = listed[name];
        let declared = zip
            .by_index_raw(index)
            .map_err(|e| format!("the read archive is not a readable zip ({e})"))?
            .size();
        if declared != file.size {
            return Err(format!(
                "config/resources/{name} is {declared} bytes in the read archive but was listed as {}; it changed while it was read",
                file.size
            ));
        }
    }

    // 5. inflate, bounded per entry and in total
    let listed_total: u64 = files.iter().map(|f| f.size).sum();
    let limit = listed_total.saturating_add(slack);
    let mut written = 0u64;
    for (index, raw) in names.iter().enumerate() {
        let name = std::str::from_utf8(raw).unwrap_or_default();
        let file = listed[name];
        let target = file
            .rel
            .split('/')
            .fold(dest.join(&file.collection), |p, c| p.join(c));
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create a download dir ({:?})", e.kind()))?;
        }
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|e| format!("cannot write config/resources/{name} ({:?})", e.kind()))?;
        let entry = zip
            .by_index(index)
            .map_err(|e| format!("config/resources/{name} cannot be extracted ({e})"))?;
        let copied = copy_bounded(entry.take(file.size + 1), &mut out).map_err(|e| match e {
            Copy::Read(e) => format!("config/resources/{name} cannot be extracted ({e})"),
            Copy::Write(kind) => format!("cannot write config/resources/{name} ({kind:?})"),
        })?;
        if copied > file.size {
            return Err(format!(
                "config/resources/{name} inflates past its listed {} bytes",
                file.size
            ));
        }
        if copied != file.size {
            return Err(format!(
                "config/resources/{name} is {copied} bytes but was listed as {}; it changed while it was read",
                file.size
            ));
        }
        written += copied;
        if written > limit {
            return Err(format!(
                "the read archive inflates past the {limit} bytes listed for it"
            ));
        }
    }
    Ok(written)
}

/// Why a bounded copy stopped.
enum Copy {
    Read(std::io::Error),
    Write(std::io::ErrorKind),
}

/// Copy `from` into `to` until `from` ends; read and write errors apart.
fn copy_bounded(mut from: impl Read, to: &mut impl std::io::Write) -> Result<u64, Copy> {
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = match from.read(&mut buf) {
            Ok(0) => return Ok(total),
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(Copy::Read(e)),
        };
        to.write_all(&buf[..n]).map_err(|e| Copy::Write(e.kind()))?;
        total += n as u64;
    }
}

/// The "version made by" host and the external attributes of every
/// central-directory record of the archive at `archive`, by entry name, for
/// a live check of what a real host builds (quick 261105-dl1).
pub fn record_attributes(archive: &Path) -> Result<Vec<(String, u8, u32)>, String> {
    let opened = std::fs::File::open(archive)
        .map_err(|e| format!("the read archive cannot be opened ({:?})", e.kind()))?;
    let len = opened
        .metadata()
        .map_err(|e| format!("the read archive cannot be inspected ({:?})", e.kind()))?
        .len();
    let mut walk = opened
        .try_clone()
        .map_err(|e| format!("the read archive cannot be opened ({:?})", e.kind()))?;
    let zip = zip::ZipArchive::new(opened)
        .map_err(|e| format!("the read archive is not a readable zip ({e})"))?;
    Ok(
        central_records(&mut walk, zip.central_directory_start(), len)?
            .into_iter()
            .map(|r| {
                (
                    String::from_utf8_lossy(&r.name).into_owned(),
                    r.host,
                    r.external,
                )
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(collection: &str, rel: &str, size: u64) -> BundleFile {
        BundleFile {
            collection: collection.to_string(),
            rel: rel.to_string(),
            size,
        }
    }

    #[test]
    fn render_list_is_size_tab_name_lf_in_order() {
        let files = [
            file("core", "ignition/a b,1/config.json", 10),
            file("production", "ignition/caf\u{e9}.json", 0),
            file("core", "ignition/b.json", 12_345_678_901),
        ];
        assert_eq!(
            render_list(&files),
            "10\tcore/ignition/a b,1/config.json\n0\tproduction/ignition/caf\u{e9}.json\n12345678901\tcore/ignition/b.json\n"
                .as_bytes()
        );
        assert!(render_list(&[]).is_empty());
    }

    #[test]
    fn archive_limit_arithmetic() {
        assert_eq!(archive_limit(&[]), 4096);
        // `core/a/b.js` is 11 bytes: 6400 + 100 + (2 x 11 + 192) + 4096
        assert_eq!(archive_limit(&[file("core", "a/b.js", 6400)]), 10_810);
        let two = [file("core", "a", 64), file("mode", "bc", 0)];
        // 64 + 1 + (2 x 6 + 192) + (2 x 7 + 192) + 4096
        assert_eq!(archive_limit(&two), 64 + 1 + 204 + 206 + 4096);
    }

    #[test]
    fn name_rules() {
        for (name, want) in [
            (&b"core/a\\b"[..], Some("holds a backslash")),
            (b"/core/a", Some("is an absolute path")),
            (b"core/a/", Some("is a directory entry")),
            (b"C:/core/a", Some("names a drive or a stream")),
            (b"core/a:s", Some("names a drive or a stream")),
            (b"core//a", Some("has an empty, `.` or `..` component")),
            (b"core/./a", Some("has an empty, `.` or `..` component")),
            (b"core/../a", Some("has an empty, `.` or `..` component")),
            (b"", Some("has an empty, `.` or `..` component")),
            (b"core/\xff", Some("is not UTF-8")),
            (b"core/caf\xc3\xa9/a b,1.json", None),
        ] {
            assert_eq!(name_problem(name), want, "{}", shown(name));
        }
    }

    /// The central-directory walk sees a record the crate collapses: two
    /// records with one name.
    #[test]
    fn central_walk_counts_a_crafted_duplicate() {
        use std::io::Write as _;
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for name in ["core/a.json", "core/b.json"] {
            writer.start_file(name, options).expect("entry");
            writer.write_all(b"{}").expect("write");
        }
        let mut bytes = writer.finish().expect("finish").into_inner();
        // rename every `core/b.json` to `core/a.json` (local and central)
        let from = b"core/b.json";
        let mut at = 0;
        while let Some(found) = bytes[at..].windows(from.len()).position(|w| w == from) {
            bytes[at + found..at + found + from.len()].copy_from_slice(b"core/a.json");
            at += found + from.len();
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("dup.zip");
        std::fs::write(&path, &bytes).expect("write");
        let mut opened = std::fs::File::open(&path).expect("open");
        let zip =
            zip::ZipArchive::new(opened.try_clone().expect("clone")).expect("the crate opens it");
        let records = central_records(
            &mut opened,
            zip.central_directory_start(),
            bytes.len() as u64,
        )
        .expect("walk");
        assert_eq!(records.len(), 2, "both records are seen");
        assert_eq!(zip.len(), 1, "the crate keeps one of them");
        let why = extract(
            &path,
            &[file("core", "a.json", 2)],
            &dir.path().join("out"),
            1024,
        )
        .expect_err("refused");
        assert!(why.contains("twice"), "{why}");
    }

    /// An honest Unix-made archive (the shape PowerShell 7 builds on a Unix
    /// host) round-trips, and the destination holds exactly the files.
    #[test]
    fn extract_round_trips_an_honest_archive() {
        use std::io::Write as _;
        let files = [
            file("core", "ignition/a b,1/config.json", 5),
            file("production", "ignition/caf\u{e9}/x.json", 0),
        ];
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (f, body) in files.iter().zip([&b"hello"[..], b""]) {
            writer.start_file(f.name(), options).expect("entry");
            writer.write_all(body).expect("write");
        }
        let bytes = writer.finish().expect("finish").into_inner();
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("read.zip");
        std::fs::write(&path, &bytes).expect("write");
        assert!(bytes.len() as u64 <= archive_limit(&files));
        let out = dir.path().join("out");
        assert_eq!(extract(&path, &files, &out, 0).expect("extract"), 5);
        assert_eq!(
            std::fs::read(out.join("core/ignition/a b,1/config.json")).expect("read"),
            b"hello"
        );
        assert_eq!(
            std::fs::read(out.join("production/ignition/caf\u{e9}/x.json")).expect("read"),
            b""
        );
        let attributes = record_attributes(&path).expect("attributes");
        assert_eq!(attributes.len(), 2);
        assert!(
            attributes
                .iter()
                .all(|(_, host, ext)| *host == 3 && ext >> 16 == 0o100_644)
        );
    }
}
