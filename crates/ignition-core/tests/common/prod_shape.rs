//! A deterministic production-shape tag tree (quick 261104-s3n).
//!
//! The shape of the core tag folders of a production Windows pair, captured
//! read-only from its master (ign 1.6.6):
//!
//! | Tree | Files | Dirs | Max depth | Max fanout | Bytes | Paths >= 260 chars |
//! |---|---|---|---|---|---|---|
//! | `core/ignition/tag-definition` | 14,119 | 7,500 | 14 | 89 | 47.8 MB | 7 (max 271) |
//! | `core/ignition/tag-type-definition` | 156 | 85 | 5 | 15 | 3.7 MB | 0 |
//!
//! At that shape a read that lists one folder per sftp `ls` took about 100
//! minutes. This generator, and the tests built on it, are the gate for every
//! deploy transport change: the offline connection-count tests in
//! `src/deploy/transport/sftp.rs`, the timed rows of
//! `tests/deploy_prod_scale.rs` and the live `wflwvm01_prod_shape_*` gate of
//! `tests/live_transport_wflwvm01.rs`. A change that makes a read cost more
//! connections as the tree grows fails them.
//!
//! Everything is built from fixed rules: no RNG, no clock, no environment.
//! Names are generic (`Area01`, `Line03`, `Cell07`, `Device05`). Depth is the
//! number of components of an entry's path below the type folder, so a file
//! directly in `ProdShape` has depth 2. A folder's fanout is its number of
//! direct children, files and folders together.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::Path;

/// The one tag provider folder every generated tree lives in.
pub const PROVIDER: &str = "ProdShape";

/// The data directory of a default Windows install, which the path lengths
/// are measured under.
pub const TYPICAL_WINDOWS_DATA_DIR: &str = r"C:\Program Files\Inductive Automation\Ignition\data";

/// The recorded production numbers the generator reproduces.
pub const TAG_DEFINITION_FILES: usize = 14_119;
pub const TAG_DEFINITION_DIRS: usize = 7_500;
pub const TAG_DEFINITION_DEPTH: usize = 14;
pub const TAG_DEFINITION_FANOUT: usize = 89;
pub const TAG_DEFINITION_BYTES: u64 = 47_800_000;
pub const TAG_DEFINITION_LONG: usize = 7;
pub const TAG_DEFINITION_LONGEST: usize = 271;
pub const TAG_TYPE_FILES: usize = 156;
pub const TAG_TYPE_DIRS: usize = 85;
pub const TAG_TYPE_DEPTH: usize = 5;
pub const TAG_TYPE_FANOUT: usize = 15;
pub const TAG_TYPE_BYTES: u64 = 3_700_000;

/// One generated entry below a type folder (`ignition/<type>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// `/`-separated; the first component is always [`PROVIDER`].
    pub rel: String,
    pub dir: bool,
    /// 0 for a directory.
    pub size: u64,
}

/// How large the generated files are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bytes {
    /// 16 to 64 bytes per file: for tests that do not care about content.
    Small,
    /// Sizes whose tree totals land within 2% of the production bytes.
    Prod,
}

/// FNV-1a 64: a fixed hash so sizes and content follow from the path alone.
pub fn fnv(text: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// A folder name of exactly `len` characters: `stem` followed by a fixed
/// filler. Letters, digits and `_` only.
fn padded(stem: &str, len: usize) -> String {
    const FILL: &str = "_Historical_Equipment_Configuration_Archive";
    let mut name = stem.to_string();
    while name.len() < len {
        name.push_str(FILL);
    }
    name.truncate(len);
    if name.ends_with('_') {
        name.pop();
        name.push('X');
    }
    name
}

/// The folders of the production-shape `tag-definition` tree, the files of
/// each folder decided by [`with_files`].
fn tag_definition_dirs() -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut dirs: Vec<String> = vec![PROVIDER.to_string()];
    let mut devices: Vec<String> = Vec::new();
    for area in 1..=12 {
        let area = format!("{PROVIDER}/Area{area:02}");
        dirs.push(area.clone());
        for line in 1..=8 {
            let line = format!("{area}/Line{line:02}");
            dirs.push(line.clone());
            for cell in 1..=8 {
                let cell = format!("{line}/Cell{cell:02}");
                dirs.push(cell.clone());
                for device in 1..=7 {
                    let device = format!("{cell}/Device{device:02}");
                    dirs.push(device.clone());
                    devices.push(device);
                }
            }
        }
    }
    // the widest folder: 87 folders and 2 files, 89 children
    let library = format!("{PROVIDER}/Library");
    dirs.push(library.clone());
    for template in 1..=87 {
        dirs.push(format!("{library}/Template{template:03}"));
    }
    // the deepest branch: folders down to depth 13, files at depth 14
    let mut spine = format!("{PROVIDER}/Area01/Line01/Cell01/Device01");
    for level in 6..=13 {
        spine = format!("{spine}/Sub{level:02}");
        dirs.push(spine.clone());
    }
    // the long branch: two folders of 250 and 251 characters (over the
    // reader's 240 limit, under the server's 260) and the seven entries at
    // 260 to 271 characters below them
    let archive = format!("{PROVIDER}/Archive");
    dirs.push(archive.clone());
    let base = windows_full_len("core", "tag-definition", &archive);
    let first = format!("{archive}/{}", padded("Retired_Line_A", 250 - base - 1));
    let second = format!("{archive}/{}", padded("Retired_Line_B", 251 - base - 1));
    let first_sub = format!("{first}/Station01");
    let second_sub = format!("{second}/Station02");
    dirs.extend([
        first.clone(),
        first_sub.clone(),
        second.clone(),
        second_sub.clone(),
    ]);
    // filler folders spread evenly over the devices, to the recorded count
    let filler = TAG_DEFINITION_DIRS - dirs.len();
    for (index, device) in devices.iter().enumerate() {
        if spread(index, filler, devices.len()) {
            dirs.push(format!("{device}/Signals"));
        }
    }
    // folders that get `unary-resource.json` whatever the spread says, and
    // folders that never do (it would pass the recorded longest path)
    let forced = vec![library, first];
    let barred = vec![first_sub, second, second_sub];
    (dirs, forced, barred)
}

/// Whether item `index` of `of` is picked when `count` are spread evenly,
/// item 0 always included: exactly `count` are picked.
fn spread(index: usize, count: usize, of: usize) -> bool {
    let at = |i: usize| (i * count).div_ceil(of);
    at(index + 1) != at(index)
}

/// Every folder gets `tags.json`; `unary-resource.json` goes to the forced
/// folders and to an even spread of the others until there are `files`.
fn with_files(
    dirs: Vec<String>,
    forced: &[String],
    barred: &[String],
    files: usize,
    bytes: Bytes,
    total: u64,
) -> Vec<Entry> {
    let unary_needed = files - dirs.len();
    let eligible: Vec<&String> = dirs
        .iter()
        .filter(|d| !forced.contains(d) && !barred.contains(d))
        .collect();
    let spread_count = unary_needed - forced.len();
    let mut unary: Vec<&String> = forced.iter().collect();
    for (index, dir) in eligible.iter().enumerate() {
        if spread(index, spread_count, eligible.len()) {
            unary.push(dir);
        }
    }
    let mut paths: Vec<(String, bool)> = Vec::new();
    for dir in &dirs {
        paths.push((dir.clone(), true));
        paths.push((format!("{dir}/tags.json"), false));
    }
    for dir in unary {
        paths.push((format!("{dir}/unary-resource.json"), false));
    }
    sized(paths, bytes, total)
}

/// Give every file a size and sort by path bytes (parents before children).
fn sized(paths: Vec<(String, bool)>, bytes: Bytes, total: u64) -> Vec<Entry> {
    let mut entries: Vec<Entry> = match bytes {
        Bytes::Small => paths
            .into_iter()
            .map(|(rel, dir)| {
                let size = if dir { 0 } else { 16 + fnv(&rel) % 49 };
                Entry { rel, dir, size }
            })
            .collect(),
        Bytes::Prod => {
            // a marker file is small, as on a gateway; tags.json carries the
            // rest of the recorded bytes, weighted by a fixed hash
            let marker = |rel: &str| 64 + fnv(rel) % 128;
            let marker_bytes: u64 = paths
                .iter()
                .filter(|(rel, dir)| !dir && rel.ends_with("/unary-resource.json"))
                .map(|(rel, _)| marker(rel))
                .sum();
            let weight = |rel: &str| 1 + fnv(rel) % 9;
            let weights: u64 = paths
                .iter()
                .filter(|(rel, dir)| !dir && rel.ends_with("/tags.json"))
                .map(|(rel, _)| weight(rel))
                .sum();
            let left = total - marker_bytes;
            paths
                .into_iter()
                .map(|(rel, dir)| {
                    let size = if dir {
                        0
                    } else if rel.ends_with("/unary-resource.json") {
                        marker(&rel)
                    } else {
                        (left * weight(&rel) / weights).max(64)
                    };
                    Entry { rel, dir, size }
                })
                .collect()
        }
    };
    entries.sort_by(|a, b| a.rel.as_bytes().cmp(b.rel.as_bytes()));
    entries
}

/// The production-shape `tag-definition` tree (see the module table).
pub fn tag_definition(bytes: Bytes) -> Vec<Entry> {
    let (dirs, forced, barred) = tag_definition_dirs();
    with_files(
        dirs,
        &forced,
        &barred,
        TAG_DEFINITION_FILES,
        bytes,
        TAG_DEFINITION_BYTES,
    )
}

/// The production-shape `tag-type-definition` tree: 156 files, 85 folders,
/// depth 5, fanout 15.
pub fn tag_type_definition(bytes: Bytes) -> Vec<Entry> {
    let mut dirs: Vec<String> = vec![PROVIDER.to_string()];
    for family in 1..=13 {
        let family = format!("{PROVIDER}/Family{family:02}");
        dirs.push(family.clone());
        for member in 1..=5 {
            dirs.push(format!("{family}/Member{member:02}"));
        }
    }
    for family in 1..=6 {
        dirs.push(format!("{PROVIDER}/Family{family:02}/Member01/Variant01"));
    }
    with_files(dirs, &[], &[], TAG_TYPE_FILES, bytes, TAG_TYPE_BYTES)
}

/// A tiny `tag-definition` tree under the same provider: 6 entries, depth 3.
pub fn tiny_tag_definition() -> Vec<Entry> {
    let dirs = [PROVIDER.to_string(), format!("{PROVIDER}/Area01")];
    let mut paths: Vec<(String, bool)> = dirs
        .iter()
        .flat_map(|d| [(d.clone(), true), (format!("{d}/tags.json"), false)])
        .collect();
    paths.push((format!("{PROVIDER}/Area01/unary-resource.json"), false));
    paths.push((format!("{PROVIDER}/Area01/Line01"), true));
    sized(paths, Bytes::Small, 0)
}

/// xorshift64* (Vigna): a fixed pseudo-random stream, so the filler needs no
/// RNG crate, clock or environment. `state` must not be 0.
fn xorshift(state: &mut u64) -> u64 {
    *state ^= *state >> 12;
    *state ^= *state << 25;
    *state ^= *state >> 27;
    state.wrapping_mul(0x2545_f491_4f6c_dd1d)
}

/// How many distinct filler words there are.
const VOCABULARY_WORDS: usize = 128;

/// The filler vocabulary: [`VOCABULARY_WORDS`] lowercase words of 3 to 10
/// letters, drawn once from a fixed seed.
fn vocabulary() -> &'static [String] {
    static WORDS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    WORDS.get_or_init(|| {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        (0..VOCABULARY_WORDS)
            .map(|_| {
                let len = 3 + (xorshift(&mut state) % 8) as usize;
                (0..len)
                    .map(|_| char::from(b'a' + (xorshift(&mut state) % 26) as u8))
                    .collect()
            })
            .collect()
    })
}

/// Exactly `size` bytes of valid JSON derived only from `rel`. Panics below
/// 8 bytes (no generated file is that small).
///
/// The filler is lowercase words of a fixed vocabulary separated by single
/// spaces, picked by an xorshift64* stream seeded with [`fnv`] of `rel` (quick
/// 261105-dl1). A repeated id would deflate to well under 1% of its size, so a
/// bundled (zipped) transfer of it would not move real volume. Measured with
/// the zip crate's default Deflate on the `Bytes::Prod` tag-definition files:
/// about 2.6x (`prod_shape_content_deflates_like_real_volume` holds it
/// between 2x and 6x). Tag JSON with its repeated keys deflates better than
/// that, so every transfer the tests measure is at or above production volume.
pub fn content(rel: &str, size: u64) -> Vec<u8> {
    let size = size as usize;
    let seed = fnv(rel);
    let id = format!("{seed:016x}");
    let (head, tail) = if size >= 44 {
        (format!("{{\"tags\":[],\"id\":\"{id}\",\"pad\":\""), "\"}")
    } else {
        assert!(size >= 8, "{rel}: {size} bytes cannot hold a JSON object");
        ("{\"p\":\"".to_string(), "\"}")
    };
    let mut out = head.into_bytes();
    let fill = size - out.len() - tail.len();
    let end = out.len() + fill;
    let words = vocabulary();
    let mut state = seed | 1;
    while out.len() < end {
        if out.len() > end - fill {
            out.push(b' ');
        }
        let word = &words[(xorshift(&mut state) % VOCABULARY_WORDS as u64) as usize];
        out.extend_from_slice(word.as_bytes());
    }
    out.truncate(end);
    out.extend_from_slice(tail.as_bytes());
    debug_assert_eq!(out.len(), size);
    out
}

/// The character length of the Windows path of `rel` in the type folder
/// `ignition/<ty>` of `collection`, under [`TYPICAL_WINDOWS_DATA_DIR`].
pub fn windows_full_len(collection: &str, ty: &str, rel: &str) -> usize {
    format!(
        "{TYPICAL_WINDOWS_DATA_DIR}\\config\\resources\\{collection}\\ignition\\{ty}\\{}",
        rel.replace('/', "\\")
    )
    .chars()
    .count()
}

/// Create every directory and every file ([`content`]) of `entries` below
/// `type_dir`.
pub fn materialize(type_dir: &Path, entries: &[Entry]) {
    std::fs::create_dir_all(type_dir).expect("type dir");
    for entry in entries {
        let path = entry
            .rel
            .split('/')
            .fold(type_dir.to_path_buf(), |p, c| p.join(c));
        if entry.dir {
            std::fs::create_dir_all(&path).expect("generated dir");
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("generated parent");
            }
            std::fs::write(&path, content(&entry.rel, entry.size)).expect("generated file");
        }
    }
}

/// Per-folder direct child counts, by folder rel.
pub fn fanouts(entries: &[Entry]) -> BTreeMap<String, usize> {
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    for entry in entries {
        if let Some((parent, _)) = entry.rel.rsplit_once('/') {
            *out.entry(parent.to_string()).or_default() += 1;
        }
    }
    out
}

/// The number of components of the deepest entry.
pub fn depth(entries: &[Entry]) -> usize {
    entries
        .iter()
        .map(|e| e.rel.split('/').count())
        .max()
        .unwrap_or(0)
}
