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

// ---- tags a project uses (quick 261106-bt0) ---------------------------------
//
// The size of the tag side of a production deploy, from the 2026-11-06 brief:
// a provider of about 43,000 tags exported as one document, and a project of
// about 2,000 code files. `tag_export` generates the provider in the shape the
// native export has on 8.3.6 (a `Provider` root, a `_types_` folder, an
// instance carrying one stub per direct member) plus the shapes the capture
// did not hold (an instance with no stubs, an inherited type, a nested
// instance), and `project_zip` generates a project artifact that references it
// with present, inherited, property-suffixed, runtime-built, commented,
// docstring and `__tests__` references and plants the absent ones.
//
// Everything is deterministic: fixed rules, no RNG, no clock, no environment.

/// Tags in the generated provider export: every node below the root (folders,
/// atomic tags, instances, stubs and type definitions). The generator holds at
/// least this many. The figure is from the 2026-11-06 brief.
pub const TAG_EXPORT_TAGS: usize = 43_000;

/// Code files in the generated project artifact (Python scripts and Perspective
/// views, `__tests__` members not counted). The figure is from the 2026-11-06
/// brief.
pub const PROJECT_CODE_FILES: usize = 2_000;

/// A provider the generated project references that the generated gateway does
/// not list.
pub const ABSENT_PROVIDER: &str = "RemoteSite";

/// How many absent tag paths the project plants.
pub const PLANTED_MISSING: usize = 12;

/// How many distinct paths of [`ABSENT_PROVIDER`] the project references.
pub const PLANTED_PROVIDER_PATHS: usize = 5;

const EXPORT_AREAS: usize = 12;
const EXPORT_LINES: usize = 8;
const EXPORT_CELLS: usize = 7;
const EXPORT_DEVICES: usize = 7;
const TYPE_FAMILIES: usize = 13;
const TYPES_PER_FAMILY: usize = 12;
const TYPE_MEMBERS: usize = 8;

/// The generated provider export and what is known about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagExport {
    /// The export document (`GET tags/export?provider=...&type=json`).
    pub json: Vec<u8>,
    /// Nodes below the root.
    pub tags: usize,
    /// Atomic tags that carry the padded definition.
    pub atoms: usize,
    /// Sampled paths (below the provider) of tags the export holds outright.
    pub present: Vec<String>,
    /// Sampled paths of instance members that exist only through a type: an
    /// instance with no stub for them, or a type inherited by `typeId`.
    pub inherited: Vec<String>,
}

/// What a generated UDT type looks like.
struct TypeSpec {
    /// Inherits `Type01` of its family through `typeId` and declares only
    /// `Extra`.
    inherits: bool,
    /// Holds a nested instance `Sub` of `Type01`.
    sub: bool,
}

fn type_spec(type_no: usize) -> TypeSpec {
    TypeSpec {
        inherits: type_no > 1 && type_no % 3 == 0,
        sub: type_no % 4 == 0,
    }
}

fn type_id(family: usize, type_no: usize) -> String {
    format!("Family{family:02}/Type{type_no:02}")
}

/// A JSON tag node.
fn tag_node(name: &str, tag_type: &str, extra: &str, children: Option<Vec<String>>) -> String {
    let tags = children
        .map(|c| format!(r#","tags":[{}]"#, c.join(",")))
        .unwrap_or_default();
    format!(r#"{{"name":"{name}","tagType":"{tag_type}"{extra}{tags}}}"#)
}

/// A fixed block of filler text the padded definitions are cut from.
fn filler() -> &'static str {
    static TEXT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    TEXT.get_or_init(|| {
        let words = vocabulary();
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut text = String::with_capacity(32 * 1024);
        while text.len() < 32 * 1024 {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&words[(xorshift(&mut state) % VOCABULARY_WORDS as u64) as usize]);
        }
        text
    })
}

fn padding(path: &str, len: usize) -> &'static str {
    let text = filler();
    let at = (fnv(path) as usize) % (text.len() - len.max(1));
    &text[at..at + len]
}

/// The realistic definition of one atomic tag, `pad` bytes of documentation
/// included.
fn atom_json(name: &str, path: &str, pad: usize) -> String {
    let doc = padding(path, pad);
    let extra = format!(
        r#","dataType":"Float8","valueSource":"opc","opcServer":"Ignition OPC UA Server","opcItemPath":"ns=1;s=[Source]{path}","engUnit":"degC","engHigh":100.0,"engLow":0.0,"deadband":0.5,"historyEnabled":true,"historyProvider":"Historian","documentation":"{doc}""#
    );
    tag_node(name, "AtomicTag", &extra, None)
}

fn build_export(pad: usize) -> TagExport {
    let mut tags = 0usize;
    let mut atoms = 0usize;
    let mut present = Vec::new();
    let mut inherited = Vec::new();

    // the type definitions
    let mut families = Vec::new();
    for family in 1..=TYPE_FAMILIES {
        let mut types = Vec::new();
        for type_no in 1..=TYPES_PER_FAMILY {
            let spec = type_spec(type_no);
            let mut members = Vec::new();
            if spec.inherits {
                members.push(tag_node(
                    "Extra",
                    "AtomicTag",
                    r#","dataType":"Int4""#,
                    None,
                ));
            } else {
                for member in 1..=TYPE_MEMBERS {
                    members.push(tag_node(
                        &format!("M{member:02}"),
                        "AtomicTag",
                        r#","dataType":"Int4","valueSource":"memory""#,
                        None,
                    ));
                }
            }
            if spec.sub {
                members.push(tag_node(
                    "Sub",
                    "UdtInstance",
                    &format!(r#","typeId":"{}""#, type_id(family, 1)),
                    Some(Vec::new()),
                ));
            }
            tags += 1 + members.len();
            let extra = if spec.inherits {
                format!(r#","typeId":"{}""#, type_id(family, 1))
            } else {
                String::new()
            };
            types.push(tag_node(
                &format!("Type{type_no:02}"),
                "UdtType",
                &extra,
                Some(members),
            ));
        }
        tags += 1;
        families.push(tag_node(
            &format!("Family{family:02}"),
            "Folder",
            "",
            Some(types),
        ));
    }
    tags += 1;
    let types_folder = tag_node("_types_", "Folder", "", Some(families));

    // the equipment tree
    let mut areas = Vec::new();
    let mut index = 0usize;
    for area in 1..=EXPORT_AREAS {
        let mut lines = Vec::new();
        for line in 1..=EXPORT_LINES {
            let mut cells = Vec::new();
            for cell in 1..=EXPORT_CELLS {
                let mut devices = Vec::new();
                for device in 1..=EXPORT_DEVICES {
                    let path =
                        format!("Area{area:02}/Line{line:02}/Cell{cell:02}/Device{device:02}");
                    let hash = fnv(&path) as usize;
                    let family = 1 + hash % TYPE_FAMILIES;
                    let type_no = 1 + (hash / TYPE_FAMILIES) % TYPES_PER_FAMILY;
                    let spec = type_spec(type_no);
                    let mut children = Vec::new();
                    for atom in ["Pv", "Sp", "Mode", "Alarm", "Enabled"] {
                        children.push(atom_json(atom, &format!("{path}/{atom}"), pad));
                        atoms += 1;
                    }
                    // an even device carries stubs for the type's direct
                    // members, an odd one is the on-disk form with none
                    let mut stubs = Vec::new();
                    let with_stubs = index % 2 == 0;
                    if with_stubs {
                        if spec.inherits {
                            stubs.push(tag_node("Extra", "AtomicTag", "", None));
                        } else {
                            for member in 1..=TYPE_MEMBERS {
                                stubs.push(tag_node(
                                    &format!("M{member:02}"),
                                    "AtomicTag",
                                    "",
                                    None,
                                ));
                            }
                        }
                        if spec.sub {
                            stubs.push(tag_node("Sub", "UdtInstance", "", Some(Vec::new())));
                        }
                    }
                    tags += 1 + 1 + children.len() + stubs.len();
                    children.push(tag_node(
                        "Drive",
                        "UdtInstance",
                        &format!(r#","typeId":"{}""#, type_id(family, type_no)),
                        Some(stubs),
                    ));
                    if index % 5 == 0 {
                        present.push(format!("{path}/Pv"));
                        present.push(format!("{path}/Sp"));
                        inherited.push(format!("{path}/Drive/M03"));
                        if spec.sub {
                            inherited.push(format!("{path}/Drive/Sub/M02"));
                        }
                        if spec.inherits {
                            inherited.push(format!("{path}/Drive/Extra"));
                        }
                    }
                    index += 1;
                    devices.push(tag_node(
                        &format!("Device{device:02}"),
                        "Folder",
                        "",
                        Some(children),
                    ));
                }
                tags += 1;
                cells.push(tag_node(
                    &format!("Cell{cell:02}"),
                    "Folder",
                    "",
                    Some(devices),
                ));
            }
            tags += 1;
            lines.push(tag_node(
                &format!("Line{line:02}"),
                "Folder",
                "",
                Some(cells),
            ));
        }
        tags += 1;
        areas.push(tag_node(
            &format!("Area{area:02}"),
            "Folder",
            "",
            Some(lines),
        ));
    }
    areas.push(types_folder);
    let json = format!(
        r#"{{"name":"","tagType":"Provider","tags":[{}]}}"#,
        areas.join(",")
    )
    .into_bytes();
    TagExport {
        json,
        tags,
        atoms,
        present,
        inherited,
    }
}

/// The generated provider export, padded so the document reaches
/// [`TAG_DEFINITION_BYTES`].
pub fn tag_export() -> TagExport {
    let probe = build_export(0);
    let short = TAG_DEFINITION_BYTES.saturating_sub(probe.json.len() as u64) as usize;
    let pad = if short == 0 {
        0
    } else {
        short / probe.atoms + 1
    };
    build_export(pad)
}

/// The generated project artifact and what it plants.
#[derive(Debug, Clone)]
pub struct ProjectZip {
    pub zip: Vec<u8>,
    /// Python scripts plus Perspective views, `__tests__` members not counted.
    pub code_files: usize,
    /// Members below a `__tests__` folder.
    pub test_files: usize,
    /// The absent paths, as `[PROVIDER]path`, byte-sorted.
    pub planted_missing: Vec<String>,
}

/// A Python script with about five references of every kind.
fn script_text(n: usize, export: &TagExport, planted: &[String], ghost: Option<usize>) -> String {
    let pick =
        |list: &[String], salt: usize| list[(n * 7_919 + salt * 104_729) % list.len()].clone();
    let mut text = format!(
        "def run(x):\n    \"\"\"Docstring example: [{PROVIDER}]Area99/Docs/Example and [provider]Area/MES/SomeData\"\"\"\n"
    );
    text.push_str(&format!(
        "    paths = [\"[{PROVIDER}]{}\", \"[{PROVIDER}]{}.value\"]\n",
        pick(&export.present, 1),
        pick(&export.present, 2)
    ));
    text.push_str(&format!(
        "    inherited = '[{PROVIDER}]{}'\n",
        pick(&export.inherited, 3)
    ));
    text.push_str(&format!("    # [{PROVIDER}]Area99/Commented/Out\n"));
    text.push_str(&format!(
        "    built = \"[{PROVIDER}]Area01/Line%02d/Cell01/Device01/Pv\" % x\n"
    ));
    text.push_str(&format!("    other = \"[{PROVIDER}]Area01/\" + str(x)\n"));
    for path in planted {
        text.push_str(&format!("    missing = [\"[{PROVIDER}]{path}\"]\n"));
    }
    if let Some(k) = ghost {
        text.push_str(&format!(
            "    remote = \"[{ABSENT_PROVIDER}]Area01/Line01/Pv{k}\"\n"
        ));
    }
    text.push_str("    return system.tag.readBlocking(paths + [inherited])\n");
    text
}

/// A Perspective view with a tag binding, an expression binding, a script
/// transform, a label that only mentions a path and an indirect path.
fn view_text(n: usize, export: &TagExport, planted: &[String]) -> String {
    let pick =
        |salt: usize| export.present[(n * 6_007 + salt * 7_919) % export.present.len()].clone();
    let missing = planted
        .iter()
        .map(|path| {
            format!(
                r#",{{"type":"ia.display.label","propConfig":{{"props.text":{{"binding":{{"type":"tag","config":{{"tagPath":"[{PROVIDER}]{path}"}}}}}}}}}}"#
            )
        })
        .collect::<String>();
    format!(
        r#"{{"root":{{"type":"ia.container.flex","children":[{{"type":"ia.display.label","props":{{"text":"see [{PROVIDER}]Area99/Label/Only"}},"propConfig":{{"props.value":{{"binding":{{"type":"tag","config":{{"tagPath":"[{PROVIDER}]{}"}}}}}}}}}},{{"type":"ia.display.label","propConfig":{{"props.value":{{"binding":{{"type":"expr","config":{{"expression":"{{[{PROVIDER}]{}}} + 1"}}}}}}}}}},{{"type":"ia.display.label","propConfig":{{"props.value":{{"binding":{{"type":"tag","config":{{"tagPath":"[{PROVIDER}]Area01/{{1}}/Pv"}}}},"transforms":[{{"type":"script","code":"\tdef transform(self, value, quality, timestamp):\n\t\treturn system.tag.readBlocking(['[{PROVIDER}]{}'])"}}]}}}}}}{missing}]}}}}"#,
        pick(1),
        pick(2),
        pick(3),
    )
}

/// The generated project artifact. It references `export` (the provider
/// [`PROVIDER`]): present and inherited paths, `.value` suffixes, runtime-built
/// paths, comments and docstrings that must not be reported, plus
/// [`PLANTED_MISSING`] absent paths, [`PLANTED_PROVIDER_PATHS`] paths of
/// [`ABSENT_PROVIDER`] and members below `__tests__` folders holding absent
/// paths that must not be reported either.
pub fn project_zip(export: &TagExport) -> ProjectZip {
    use std::io::Write as _;
    let mut planted_missing: Vec<String> = (1..=PLANTED_MISSING)
        .map(|n| format!("[{PROVIDER}]Area99/Absent/P{n:02}"))
        .collect();
    planted_missing.sort();
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let mut add = |name: &str, bytes: &[u8]| {
        writer.start_file(name, options).expect("zip entry");
        writer.write_all(bytes).expect("zip bytes");
    };
    add("project.json", br#"{"title":"Prod Shape","enabled":true}"#);
    let resource = |file: &str| {
        format!(r#"{{"scope":"G","version":1,"files":["{file}"],"attributes":{{}}}}"#).into_bytes()
    };
    let scripts = 1_700usize;
    let views = PROJECT_CODE_FILES - scripts;
    for n in 0..scripts {
        // each absent path is planted in two scripts, 500 apart
        let planted: Vec<String> = (0..PLANTED_MISSING)
            .filter(|m| n == m * 37 || n == m * 37 + 500)
            .map(|m| format!("Area99/Absent/P{:02}", m + 1))
            .collect();
        let ghost = [3usize, 900, 1_001, 1_500, 1_601]
            .iter()
            .position(|at| *at == n);
        let text = script_text(n, export, &planted, ghost);
        let (dir, file) = match n % 17 {
            0..=2 => (format!("ignition/timer/T{n:04}"), "handleTimerEvent.py"),
            3..=7 => (format!("ignition/tag-change/C{n:04}"), "onTagChange.py"),
            _ => (format!("ignition/script-python/pkg{n:04}"), "code.py"),
        };
        add(&format!("{dir}/{file}"), text.as_bytes());
        add(&format!("{dir}/resource.json"), &resource(file));
    }
    for n in 0..views {
        let planted: Vec<String> = if n % 100 == 7 {
            vec![format!(
                "Area99/Absent/P{:02}",
                (n / 100) % PLANTED_MISSING + 1
            )]
        } else {
            Vec::new()
        };
        let dir = format!("com.inductiveautomation.perspective/views/V{n:03}");
        add(
            &format!("{dir}/view.json"),
            view_text(n, export, &planted).as_bytes(),
        );
        add(&format!("{dir}/resource.json"), &resource("view.json"));
    }
    let test_files = 45usize;
    for n in 0..test_files {
        let text = format!(
            "def test_case():\n    x = [\"[TEST]Fake/Case{n}\", \"[{PROVIDER}]Area99/Absent/Test{n}\"]\n"
        );
        let dir = format!("ignition/script-python/pkg{n:04}/__tests__/case");
        add(&format!("{dir}/test_case.py"), text.as_bytes());
        add(&format!("{dir}/resource.json"), &resource("test_case.py"));
    }
    let zip = writer.finish().expect("zip finish").into_inner();
    ProjectZip {
        zip,
        code_files: scripts + views,
        test_files,
        planted_missing,
    }
}
