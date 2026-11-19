//! Dirty paths of a gateway HEAD ign's store does not hold, from the gateway's
//! own git objects (Phase 27, 27-10, D-24).
//!
//! When an override passes a gateway HEAD G that ign's release checkout has
//! never seen (an unpushed Designer commit), the dirty paths cannot come from
//! ign's store (27-05 sets `needs_gateway_objects`). They come from the
//! gateway's `projects/<p>/.git/objects`, read over the host channel, with no
//! git on the gateway host:
//!
//! 1. the host lists the object files ([`GitObjectList`]);
//! 2. ign builds a private scratch git dir whose alternates point at its own
//!    checkout ([`HostGit::init_scratch`]) and copies every loose object into
//!    it;
//! 3. `rev-list --objects --no-walk --missing=print` names what G's tree still
//!    lacks; only the packs whose index holds one of those ids are copied, a
//!    round at a time;
//! 4. with the tree complete, the canonical digest of the tree at G is taken
//!    from the scratch dir ([`head_digest`](super::git_gate::head_digest)) and
//!    compared with the live export's.
//!
//! Only object files of the fixed shapes below are ever requested: the
//! gateway's `config`, hooks, `info/` and every other file are never read, so
//! no gateway setting or hook can run. No git runs with the gateway `.git` as
//! its git dir. Every cap is checked against the listed sizes before the
//! first fetch and the running total before each. A failure to obtain or
//! resolve G's objects is [`GitGateFailure::GatewayObjects`]: dirty is never
//! reported clean, and never unknown, for lack of objects.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::git_gate::{
    DirtySource, GitGateFailure, NodeGates, capped_dirty, detail_of, head_digest, short,
};
use super::git_remote::{GitDir, HostGit, is_object_id};
use super::project_artifact::CanonicalDigest;
use super::transport::projects::GitHostRead;

/// Most loose objects a gateway `.git` may hold for this read.
pub const GIT_OBJECTS_LOOSE_MAX: usize = 50_000;

/// Most packs a gateway `.git` may hold for this read.
pub const GIT_OBJECTS_PACKS_MAX: usize = 512;

/// Most bytes this read may copy, and the most a listing may add up to (every
/// loose object, every index and every pack: the worst case, in which every
/// pack is needed).
pub const GIT_OBJECTS_FETCH_MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// Most entries a host walks below `objects/` before it refuses.
pub const GIT_OBJECTS_LIST_MAX_ENTRIES: usize = 200_000;

/// What every cap refusal tells the operator to do (WR-A6): the caps are
/// counts and sizes of the gateway's object files, which `git gc` brings down.
const GC_REMEDY: &str =
    "an administrator runs `git gc` in the gateway's project repository, then deploys again";

/// One object file: its path below the `.git` (`objects/ab/cdef...`) and its
/// size, the exact number of bytes a fetch must deliver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitObjectFile {
    /// `/`-separated path below the project's `.git`.
    pub rel: String,
    /// Size in bytes.
    pub size: u64,
}

/// One pack: the 40 hex digits of its name and the sizes of its two files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitPackFiles {
    /// The hex digits in `pack-<name>.pack`.
    pub name: String,
    /// Size of the `.pack`.
    pub pack_size: u64,
    /// Size of the `.idx`.
    pub idx_size: u64,
}

impl GitPackFiles {
    /// `objects/pack/pack-<name>.pack`.
    pub fn pack_rel(&self) -> String {
        format!("objects/pack/pack-{}.pack", self.name)
    }

    /// `objects/pack/pack-<name>.idx`.
    pub fn idx_rel(&self) -> String {
        format!("objects/pack/pack-{}.idx", self.name)
    }
}

/// What a gateway `.git/objects` holds, restricted to the fixed shapes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitObjectList {
    /// Loose objects, rel-sorted.
    pub loose: Vec<GitObjectFile>,
    /// Packs that have both their `.pack` and their `.idx`, name-sorted.
    pub packs: Vec<GitPackFiles>,
}

/// What kind of entry a host listing found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitObjectEntryKind {
    /// A regular file.
    File,
    /// A symbolic link (never followed, and refused).
    Symlink,
    /// A directory, a device, a pipe or anything else (ignored).
    Other,
}

/// One entry a host listing reports, before the shapes are applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitObjectEntry {
    /// `/`-separated path below the project's `.git`.
    pub rel: String,
    /// Size of a regular file (0 otherwise).
    pub size: u64,
    /// What the entry is.
    pub kind: GitObjectEntryKind,
}

/// Which fixed shape an object path has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectRel {
    /// `objects/<2 hex>/<38 hex>`.
    Loose,
    /// `objects/pack/pack-<40 hex>.pack` (`idx` false) or `.idx`.
    Pack {
        /// The 40 hex digits.
        name: String,
        /// The `.idx` file rather than the `.pack`.
        idx: bool,
    },
}

fn is_hex(text: &str) -> bool {
    text.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Whether `rel` is a name no listing may carry: a NUL, a backslash, a line
/// break, a leading `/`, a drive, or an empty, `.` or `..` component.
fn hostile(rel: &str) -> bool {
    rel.is_empty()
        || rel.contains(['\0', '\\', '\n', '\r', ':'])
        || rel.starts_with('/')
        || rel
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
}

/// Check `rel` against the fixed shapes (`objects/<2 hex>/<38 hex>`,
/// `objects/pack/pack-<40 hex>.pack` and `.idx`). The one check a fetch runs
/// on every name before it builds a path.
pub fn check_object_rel(rel: &str) -> Result<ObjectRel, String> {
    if hostile(rel) {
        return Err("the name is not a plain relative object path".to_string());
    }
    let parts: Vec<&str> = rel.split('/').collect();
    match parts.as_slice() {
        ["objects", "pack", file] => {
            let named = file
                .strip_prefix("pack-")
                .and_then(|rest| {
                    rest.strip_suffix(".pack")
                        .map(|n| (n, false))
                        .or_else(|| rest.strip_suffix(".idx").map(|n| (n, true)))
                })
                .filter(|(name, _)| name.len() == 40 && is_hex(name));
            match named {
                Some((name, idx)) => Ok(ObjectRel::Pack {
                    name: name.to_string(),
                    idx,
                }),
                None => Err("the name is not a pack file of the fixed shape".to_string()),
            }
        }
        ["objects", fan, rest]
            if fan.len() == 2 && is_hex(fan) && rest.len() == 38 && is_hex(rest) =>
        {
            Ok(ObjectRel::Loose)
        }
        _ => Err("the name is not an object file of the fixed shapes".to_string()),
    }
}

impl GitObjectList {
    /// The list a host's entries make: only the fixed shapes are kept
    /// (`info/`, `*.keep`, `*.bitmap`, `*.rev`, temp files and a `.pack`
    /// without its `.idx` are ignored), and anything dangerous refuses the
    /// whole listing before any fetch: a symlink, a name with `..`, `\`, a NUL
    /// or a leading `/`, or the same path twice.
    pub fn from_entries(entries: &[GitObjectEntry]) -> Result<GitObjectList, String> {
        let mut seen = BTreeSet::new();
        let mut loose = Vec::new();
        let mut pack_files: BTreeMap<String, (Option<u64>, Option<u64>)> = BTreeMap::new();
        for entry in entries {
            if hostile(&entry.rel) {
                return Err(format!(
                    "the listing holds a name that is not a plain relative path ({})",
                    entry.rel.escape_debug()
                ));
            }
            if entry.kind == GitObjectEntryKind::Symlink {
                return Err(format!(
                    "{} is a symlink; a repo with a symlink is refused",
                    entry.rel.escape_debug()
                ));
            }
            if !seen.insert(entry.rel.as_str()) {
                return Err(format!(
                    "the listing names {} twice",
                    entry.rel.escape_debug()
                ));
            }
            if entry.kind != GitObjectEntryKind::File {
                continue;
            }
            match check_object_rel(&entry.rel) {
                Ok(ObjectRel::Loose) => loose.push(GitObjectFile {
                    rel: entry.rel.clone(),
                    size: entry.size,
                }),
                Ok(ObjectRel::Pack { name, idx }) => {
                    let slot = pack_files.entry(name).or_default();
                    if idx {
                        slot.1 = Some(entry.size);
                    } else {
                        slot.0 = Some(entry.size);
                    }
                }
                Err(_) => {}
            }
        }
        loose.sort_by(|a, b| a.rel.cmp(&b.rel));
        let packs = pack_files
            .into_iter()
            .filter_map(|(name, (pack, idx))| {
                Some(GitPackFiles {
                    name,
                    pack_size: pack?,
                    idx_size: idx?,
                })
            })
            .collect();
        Ok(GitObjectList { loose, packs })
    }

    /// The number of bytes of every file the list names.
    pub fn total_bytes(&self) -> u64 {
        let loose: u64 = self
            .loose
            .iter()
            .map(|f| f.size)
            .fold(0, u64::saturating_add);
        let packs: u64 = self
            .packs
            .iter()
            .map(|p| p.pack_size.saturating_add(p.idx_size))
            .fold(0, u64::saturating_add);
        loose.saturating_add(packs)
    }

    /// Re-check every name against the fixed shapes (a list built by hand, or
    /// by a transport, is not trusted) and the three caps. Run before the
    /// first fetch.
    pub fn validate(&self) -> Result<(), String> {
        if self.loose.len() > GIT_OBJECTS_LOOSE_MAX {
            return Err(format!(
                "the gateway repo holds {} loose objects, over the {GIT_OBJECTS_LOOSE_MAX} cap; {GC_REMEDY}",
                self.loose.len()
            ));
        }
        if self.packs.len() > GIT_OBJECTS_PACKS_MAX {
            return Err(format!(
                "the gateway repo holds {} packs, over the {GIT_OBJECTS_PACKS_MAX} cap; {GC_REMEDY}",
                self.packs.len()
            ));
        }
        for file in &self.loose {
            if check_object_rel(&file.rel) != Ok(ObjectRel::Loose) {
                return Err(format!(
                    "the listing names {}, which is not a loose object",
                    file.rel.escape_debug()
                ));
            }
        }
        for pack in &self.packs {
            let ok = |rel: String, idx: bool| {
                check_object_rel(&rel)
                    == Ok(ObjectRel::Pack {
                        name: pack.name.clone(),
                        idx,
                    })
            };
            if !(ok(pack.pack_rel(), false) && ok(pack.idx_rel(), true)) {
                return Err(format!(
                    "the listing names a pack `{}` that is not of the fixed shape",
                    pack.name.escape_debug()
                ));
            }
        }
        let total = self.total_bytes();
        if total > GIT_OBJECTS_FETCH_MAX_BYTES {
            return Err(format!(
                "the gateway repo's object files add up to {total} bytes, over the \
                 {GIT_OBJECTS_FETCH_MAX_BYTES} byte cap; {GC_REMEDY}"
            ));
        }
        Ok(())
    }
}

/// What one gateway-object read fetched.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FetchStats {
    /// Loose objects copied.
    pub loose: usize,
    /// Pack indexes copied (every listed one, once any pack is needed).
    pub idx: usize,
    /// Packs copied (only those holding an object still missing).
    pub packs: usize,
    /// Bytes copied, of all of the above.
    pub bytes: u64,
}

fn gateway(detail: impl Into<String>) -> GitGateFailure {
    GitGateFailure::GatewayObjects {
        detail: detail.into(),
    }
}

/// Add `more` to the running byte count, refusing before the fetch that would
/// pass [`GIT_OBJECTS_FETCH_MAX_BYTES`].
fn reserve(stats: &mut FetchStats, more: u64) -> Result<(), GitGateFailure> {
    let total = stats.bytes.saturating_add(more);
    if total > GIT_OBJECTS_FETCH_MAX_BYTES {
        return Err(gateway(format!(
            "reading the gateway's objects would copy more than {GIT_OBJECTS_FETCH_MAX_BYTES} bytes; {GC_REMEDY}"
        )));
    }
    stats.bytes = total;
    Ok(())
}

/// The canonical digest of the project tree at the gateway HEAD `head`, built
/// from the gateway's own objects.
///
/// Reads through `read` (the host channel of the node) and the scratch git dir
/// described in the module docs; `checkout` is ign's release checkout (its
/// object directory is the scratch dir's alternate and is never written).
/// Returns the digest with what was fetched.
pub async fn gateway_head_digest(
    git: &HostGit,
    checkout: &Path,
    read: &dyn GitHostRead,
    project: &str,
    head: &str,
) -> Result<(CanonicalDigest, FetchStats), GitGateFailure> {
    if !is_object_id(head) {
        return Err(gateway("the gateway HEAD is not a full lowercase hex id"));
    }
    let short_head = short(head).to_string();
    let host =
        |what: &str, err: crate::error::CoreError| gateway(format!("{what}: {}", detail_of(&err)));

    // (a) The listing, checked in full before anything is fetched.
    let list = read
        .list_git_objects(project)
        .await
        .map_err(|e| host("the gateway repo's objects cannot be listed", e))?;
    list.validate().map_err(gateway)?;

    // (b) The private scratch git dir; removed when this function returns,
    // whichever way it returns.
    let scratch = git
        .init_scratch(checkout)
        .await
        .map_err(|e| host("a scratch git dir could not be built", e))?;
    let mut stats = FetchStats::default();

    // (c) Every loose object.
    if !list.loose.is_empty() {
        let bytes: u64 = list.loose.iter().map(|f| f.size).sum();
        reserve(&mut stats, bytes)?;
        read.fetch_git_objects(project, &list.loose, scratch.git_dir())
            .await
            .map_err(|e| host("the gateway's loose objects cannot be read", e))?;
        stats.loose = list.loose.len();
    }

    // (d) What G's tree still lacks.
    let mut missing = git
        .missing_objects(&scratch, head)
        .await
        .map_err(|e| host("the gateway HEAD's objects cannot be checked", e))?;

    // (e) The pack round.
    if !missing.is_empty() {
        fetch_packs(
            git,
            read,
            project,
            &list,
            &scratch,
            head,
            &mut missing,
            &mut stats,
        )
        .await?;
    }
    if !missing.is_empty() {
        return Err(gateway(format!(
            "{} objects of {short_head} are in neither the gateway repo nor ign's store",
            missing.len()
        )));
    }

    // (f) The digest of the complete tree.
    let digest = head_digest(git, &GitDir::Bare(scratch.git_dir().to_path_buf()), head).await?;
    Ok((digest, stats))
}

/// Fetch every pack index, then, round after round, only the packs whose
/// index holds an id still missing, until nothing is missing or no pack not
/// yet fetched holds a missing id.
#[allow(clippy::too_many_arguments)]
async fn fetch_packs(
    git: &HostGit,
    read: &dyn GitHostRead,
    project: &str,
    list: &GitObjectList,
    scratch: &super::git_remote::ScratchGitDir,
    head: &str,
    missing: &mut BTreeSet<String>,
    stats: &mut FetchStats,
) -> Result<(), GitGateFailure> {
    if list.packs.is_empty() {
        return Ok(());
    }
    let host =
        |what: &str, err: crate::error::CoreError| gateway(format!("{what}: {}", detail_of(&err)));
    // The indexes go to a staging dir beside the scratch git dir; a pack's
    // index moves into the git dir with its pack.
    let stage = scratch.root().join("idx-stage");
    std::fs::create_dir(&stage)
        .map_err(|e| gateway(format!("a staging dir could not be made: {}", e.kind())))?;
    let idx_files: Vec<GitObjectFile> = list
        .packs
        .iter()
        .map(|p| GitObjectFile {
            rel: p.idx_rel(),
            size: p.idx_size,
        })
        .collect();
    reserve(stats, idx_files.iter().map(|f| f.size).sum())?;
    read.fetch_git_objects(project, &idx_files, &stage)
        .await
        .map_err(|e| host("the gateway's pack indexes cannot be read", e))?;
    stats.idx += idx_files.len();

    let mut held: Vec<BTreeSet<String>> = Vec::with_capacity(list.packs.len());
    for pack in &list.packs {
        let path = pack
            .idx_rel()
            .split('/')
            .fold(stage.clone(), |p, part| p.join(part));
        let ids = git
            .pack_ids(scratch, &path)
            .await
            .map_err(|e| host("a gateway pack index cannot be read", e))?;
        held.push(ids.into_iter().collect());
    }

    let mut fetched = vec![false; list.packs.len()];
    while !missing.is_empty() {
        let chosen: Vec<usize> = (0..list.packs.len())
            .filter(|i| !fetched[*i] && held[*i].iter().any(|id| missing.contains(id)))
            .collect();
        if chosen.is_empty() {
            break;
        }
        // One fetch for every pack of the round, so the number of host
        // connections follows the rounds and not the packs (27-11, D-24: a
        // production gateway holds dozens of packs, and a Windows host costs
        // a connection or more per fetch).
        let mut round: Vec<GitObjectFile> = Vec::with_capacity(chosen.len());
        for &i in &chosen {
            let pack = &list.packs[i];
            reserve(stats, pack.pack_size)?;
            round.push(GitObjectFile {
                rel: pack.pack_rel(),
                size: pack.pack_size,
            });
        }
        read.fetch_git_objects(project, &round, scratch.git_dir())
            .await
            .map_err(|e| host("a gateway pack cannot be read", e))?;
        for i in chosen {
            install_index(&stage, scratch.git_dir(), &list.packs[i])?;
            fetched[i] = true;
            stats.packs += 1;
        }
        *missing = git
            .missing_objects(scratch, head)
            .await
            .map_err(|e| host("the gateway HEAD's objects cannot be checked", e))?;
    }
    Ok(())
}

/// Move a pack's staged index next to its pack, after the pack (git uses a
/// pack only once its index exists). The copy is exclusive and its size is
/// the listed size.
fn install_index(stage: &Path, git_dir: &Path, pack: &GitPackFiles) -> Result<(), GitGateFailure> {
    let rel = pack.idx_rel();
    let from = rel
        .split('/')
        .fold(stage.to_path_buf(), |p, part| p.join(part));
    let to = rel
        .split('/')
        .fold(git_dir.to_path_buf(), |p, part| p.join(part));
    let failed =
        |e: std::io::Error| gateway(format!("a pack index could not be installed: {}", e.kind()));
    let mut source = std::fs::File::open(&from).map_err(failed)?;
    let mut target = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&to)
        .map_err(failed)?;
    let copied = std::io::copy(&mut source, &mut target).map_err(failed)?;
    if copied != pack.idx_size {
        return Err(gateway("a pack index changed size while it was copied"));
    }
    Ok(())
}

// ---- post-download verification (27-47, C3) -------------------------------

/// Most loose objects one hash batch holds.
const VERIFY_BATCH_FILES: usize = 4096;
/// Most inflated bytes one hash batch holds on disk at once.
const VERIFY_BATCH_BYTES: u64 = 256 * 1024 * 1024;
/// The longest `<type> <size>\0` header a loose object can have (`commit`
/// and a 20-digit size).
const LOOSE_HEADER_MAX: usize = 32;
/// The shortest pack index: the 256 four-byte fan-out entries (a version 1
/// index, no objects) and the two trailing checksums.
const IDX_MIN_BYTES: u64 = 1024 + 40;
/// The shortest pack: the 12-byte header and the trailing checksum.
const PACK_MIN_BYTES: u64 = 12 + 20;

/// The object types a loose object may have.
const OBJECT_KINDS: [&str; 4] = ["blob", "tree", "commit", "tag"];

/// The id a loose object's name says: `objects/<xx>/<38>` is `<xx><38>`.
fn loose_name_id(rel: &str) -> String {
    rel.strip_prefix("objects/").unwrap_or(rel).replace('/', "")
}

/// Check every file of a fetch against its own name and its partner, before
/// anything is installed into ign's scratch store (27-47, C3).
///
/// `dir` is the private download directory ign made; the files are in it by
/// their `rel`. The sftp protocol offers no no-follow open, so a link swapped
/// in between the listing and the `get` is followed by the `get`; this check
/// makes such a swap, and any other byte that is not the gateway's object,
/// detectable.
///
/// - A loose object is inflated (never past [`GIT_OBJECTS_FETCH_MAX_BYTES`]),
///   its `<type> <size>\0` header must name a type and the body's length, and
///   host git must hash that header and body to the id in the file's name.
/// - A pack needs its `.idx` in the same fetch. The idx must record the pack's
///   trailing checksum, and `git verify-pack` must accept the pair (the pack's
///   checksum over its own bytes, the idx's own trailing checksum, every
///   object). The trailer is never taken on trust.
/// - An `.idx` alone (the read that picks which packs to fetch) is checked
///   only for its shape: ign has no SHA-1, so its own checksum is checked
///   only when its pack arrives with it. A lone index is read for ids and is
///   never installed.
///
/// Any mismatch is an `Err` naming the object file and the check, never a
/// byte. Nothing is installed or left behind: the scratch files live in a
/// directory below `dir` that is removed on return.
///
/// Runs host git through [`HostGit`] (scrubbed environment), so it must be
/// called on a blocking thread of a tokio runtime (`spawn_blocking`).
pub fn verify_fetched_objects(dir: &Path, files: &[GitObjectFile]) -> Result<(), String> {
    verify_fetched_objects_capped(dir, files, GIT_OBJECTS_FETCH_MAX_BYTES)
}

/// A pack and its index as one fetch holds them.
#[derive(Default)]
struct PackPair {
    pack: Option<GitObjectFile>,
    idx: Option<GitObjectFile>,
}

fn verify_fetched_objects_capped(
    dir: &Path,
    files: &[GitObjectFile],
    inflate_cap: u64,
) -> Result<(), String> {
    if files.is_empty() {
        return Ok(());
    }
    let runtime = tokio::runtime::Handle::try_current()
        .map_err(|_| "the object check needs a runtime to run git".to_string())?;
    let git = HostGit::with_remote_token(None);

    // The shape of every file, before any hashing.
    let mut loose: Vec<&GitObjectFile> = Vec::new();
    let mut packs: BTreeMap<String, PackPair> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for file in files {
        let shape = check_object_rel(&file.rel)
            .map_err(|why| format!("{} is refused: {why}", file.rel.escape_debug()))?;
        if !seen.insert(file.rel.as_str()) {
            return Err(format!("{} is in the fetch twice", file.rel));
        }
        check_downloaded(dir, file)?;
        match shape {
            ObjectRel::Loose => loose.push(file),
            ObjectRel::Pack { name, idx } => {
                let slot = packs.entry(name).or_default();
                if idx {
                    slot.idx = Some(file.clone());
                } else {
                    slot.pack = Some(file.clone());
                }
            }
        }
    }
    for (name, pair) in &packs {
        match (&pair.pack, &pair.idx) {
            (Some(pack), None) => {
                return Err(format!(
                    "{} came without its idx (pack-{name}.idx); a pack is checked only with its index",
                    pack.rel
                ));
            }
            (None, Some(idx)) => {
                check_idx_shape(idx)?;
            }
            (Some(pack), Some(idx)) => {
                check_idx_shape(idx)?;
                check_pack_shape(pack)?;
                check_idx_records_pack(dir, pack, idx)?;
            }
            (None, None) => {}
        }
    }

    // The scratch files of the check: a directory below `dir`, removed on
    // return, and an empty bare git dir beside them for git to run in.
    let scratch = tempfile::Builder::new()
        .prefix("verify-")
        .tempdir_in(dir)
        .map_err(|e| format!("a scratch directory could not be made ({:?})", e.kind()))?;
    let git_dir = HostGit::empty_git_dir(scratch.path()).map_err(|e| detail_of(&e))?;

    verify_loose(
        &runtime,
        &git,
        scratch.path(),
        &git_dir,
        dir,
        &loose,
        inflate_cap,
    )?;

    // Whole packs last: `git verify-pack` resolves every delta, which is the
    // slow check; every cheap refusal has already come first.
    for (name, pair) in &packs {
        let (Some(pack), Some(idx)) = (&pair.pack, &pair.idx) else {
            continue;
        };
        let work = dir.join("objects").join("pack");
        let idx_name = format!("pack-{name}.idx");
        match runtime.block_on(git.verify_pack_files(&git_dir, &work, &idx_name)) {
            Ok(true) => {}
            Ok(false) => {
                return Err(format!(
                    "{} does not verify against {} (git verify-pack refused the pair)",
                    pack.rel, idx.rel
                ));
            }
            Err(e) => {
                return Err(format!(
                    "{} could not be checked: {}",
                    pack.rel,
                    detail_of(&e)
                ));
            }
        }
    }
    Ok(())
}

/// The downloaded file must be a regular file of exactly its listed size.
fn check_downloaded(dir: &Path, file: &GitObjectFile) -> Result<(), String> {
    let path = file
        .rel
        .split('/')
        .fold(dir.to_path_buf(), |p, c| p.join(c));
    match std::fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_file() && meta.len() == file.size => Ok(()),
        Ok(meta) if meta.file_type().is_file() => Err(format!(
            "{} is not the {} bytes it was listed as",
            file.rel, file.size
        )),
        Ok(_) => Err(format!("{} is not a regular file", file.rel)),
        Err(e) => Err(format!("{} cannot be inspected ({:?})", file.rel, e.kind())),
    }
}

fn downloaded_path(dir: &Path, rel: &str) -> PathBuf {
    rel.split('/').fold(dir.to_path_buf(), |p, c| p.join(c))
}

fn check_idx_shape(idx: &GitObjectFile) -> Result<(), String> {
    if idx.size < IDX_MIN_BYTES {
        return Err(format!(
            "{} is too short to be a pack index ({} bytes)",
            idx.rel, idx.size
        ));
    }
    Ok(())
}

fn check_pack_shape(pack: &GitObjectFile) -> Result<(), String> {
    if pack.size < PACK_MIN_BYTES {
        return Err(format!(
            "{} is too short to be a pack ({} bytes)",
            pack.rel, pack.size
        ));
    }
    Ok(())
}

/// The last `N` bytes of a file, from `back` bytes before its end.
fn tail_bytes<const N: usize>(path: &Path, back: u64, rel: &str) -> Result<[u8; N], String> {
    use std::io::{Read, Seek, SeekFrom};
    let failed = |e: std::io::Error| format!("{rel} cannot be read ({:?})", e.kind());
    let mut file = std::fs::File::open(path).map_err(failed)?;
    file.seek(SeekFrom::End(-(back as i64))).map_err(failed)?;
    let mut bytes = [0u8; N];
    file.read_exact(&mut bytes).map_err(failed)?;
    Ok(bytes)
}

/// The index's recorded pack checksum (the 20 bytes before its own checksum)
/// must be the pack's trailing 20 bytes.
fn check_idx_records_pack(
    dir: &Path,
    pack: &GitObjectFile,
    idx: &GitObjectFile,
) -> Result<(), String> {
    let recorded: [u8; 20] = tail_bytes(&downloaded_path(dir, &idx.rel), 40, &idx.rel)?;
    let trailer: [u8; 20] = tail_bytes(&downloaded_path(dir, &pack.rel), 20, &pack.rel)?;
    if recorded != trailer {
        return Err(format!(
            "{} records a different pack checksum than the trailer of {}",
            idx.rel, pack.rel
        ));
    }
    Ok(())
}

/// One batch of inflated loose-object bodies waiting for host git.
struct HashBatch {
    root: PathBuf,
    /// Per type: the body file names and the ids their object names say.
    by_kind: BTreeMap<&'static str, Vec<(String, String, String)>>,
    files: usize,
    bytes: u64,
}

impl HashBatch {
    fn new(root: PathBuf) -> Result<HashBatch, String> {
        std::fs::create_dir(&root)
            .map_err(|e| format!("a scratch directory could not be made ({:?})", e.kind()))?;
        Ok(HashBatch {
            root,
            by_kind: BTreeMap::new(),
            files: 0,
            bytes: 0,
        })
    }

    /// Hash every body with host git, compare with the names, empty the batch.
    fn flush(
        &mut self,
        runtime: &tokio::runtime::Handle,
        git: &HostGit,
        git_dir: &Path,
    ) -> Result<(), String> {
        for (kind, entries) in std::mem::take(&mut self.by_kind) {
            let names: Vec<String> = entries.iter().map(|(file, _, _)| file.clone()).collect();
            let ids = runtime
                .block_on(git.hash_object_files(git_dir, &self.root, kind, &names))
                .map_err(|e| format!("the objects could not be hashed: {}", detail_of(&e)))?;
            for ((_, want, rel), got) in entries.iter().zip(&ids) {
                if got != want {
                    return Err(format!("{rel} does not hash to its name"));
                }
            }
        }
        std::fs::remove_dir_all(&self.root)
            .and_then(|()| std::fs::create_dir(&self.root))
            .map_err(|e| format!("a scratch directory could not be cleaned ({:?})", e.kind()))?;
        self.files = 0;
        self.bytes = 0;
        Ok(())
    }
}

/// Inflate every loose object, check its header, and have host git hash the
/// header and body to the id in its name.
fn verify_loose(
    runtime: &tokio::runtime::Handle,
    git: &HostGit,
    scratch: &Path,
    git_dir: &Path,
    dir: &Path,
    loose: &[&GitObjectFile],
    inflate_cap: u64,
) -> Result<(), String> {
    if loose.is_empty() {
        return Ok(());
    }
    let mut batch = HashBatch::new(scratch.join("bodies"))?;
    let mut budget = inflate_cap;
    for (at, file) in loose.iter().enumerate() {
        let body = format!("b{at}");
        let kind = inflate_object(
            &downloaded_path(dir, &file.rel),
            &batch.root.join(&body),
            &mut budget,
            &file.rel,
        )?;
        let inflated = std::fs::metadata(batch.root.join(&body)).map_or(0, |m| m.len());
        batch.by_kind.entry(kind).or_default().push((
            body,
            loose_name_id(&file.rel),
            file.rel.clone(),
        ));
        batch.files += 1;
        batch.bytes = batch.bytes.saturating_add(inflated);
        if batch.files >= VERIFY_BATCH_FILES || batch.bytes >= VERIFY_BATCH_BYTES {
            batch.flush(runtime, git, git_dir)?;
        }
    }
    if batch.files > 0 {
        batch.flush(runtime, git, git_dir)?;
    }
    Ok(())
}

/// Inflate the loose object at `path` into `body`: its header must be
/// `<type> <size>` for a git object type, the body must be exactly `size`
/// bytes, and the zlib stream must end with the file. Returns the type.
/// `budget` is the inflated bytes still allowed; a header that claims more,
/// or a stream that yields more, is refused before it is written.
fn inflate_object(
    path: &Path,
    body: &Path,
    budget: &mut u64,
    rel: &str,
) -> Result<&'static str, String> {
    use std::io::{BufRead, BufReader, Read};
    let io = |what: &str, e: std::io::Error| format!("{rel} {what} ({:?})", e.kind());
    let file = std::fs::File::open(path).map_err(|e| io("cannot be opened", e))?;
    let mut reader = BufReader::with_capacity(64 * 1024, file);
    let mut stream = flate2::bufread::ZlibDecoder::new(&mut reader);
    let broken = |_: std::io::Error| format!("{rel} does not inflate");

    // `<type> <size>\0`
    let mut header = Vec::with_capacity(LOOSE_HEADER_MAX);
    loop {
        let mut byte = [0u8; 1];
        if stream.read(&mut byte).map_err(broken)? == 0 {
            return Err(format!("{rel} ends inside its header"));
        }
        if byte[0] == 0 {
            break;
        }
        header.push(byte[0]);
        if header.len() > LOOSE_HEADER_MAX {
            return Err(format!("{rel} has no `<type> <size>` header"));
        }
    }
    let header = std::str::from_utf8(&header)
        .ok()
        .and_then(|text| text.split_once(' '))
        .ok_or_else(|| format!("{rel} has no `<type> <size>` header"))?;
    let kind = OBJECT_KINDS
        .iter()
        .copied()
        .find(|kind| *kind == header.0)
        .ok_or_else(|| format!("{rel} has a type git does not know"))?;
    let digits = header.1;
    let canonical = !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'));
    let size: u64 = digits
        .parse()
        .ok()
        .filter(|_| canonical)
        .ok_or_else(|| format!("{rel} has a size in its header that is not a number"))?;
    if size > *budget {
        return Err(format!(
            "{rel} inflates past the {GIT_OBJECTS_FETCH_MAX_BYTES} byte cap"
        ));
    }

    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(body)
        .map_err(|e| io("cannot be unpacked", e))?;
    // one byte more than the header claims shows a body that is longer
    let mut limited = (&mut stream).take(size.saturating_add(1));
    let copied = std::io::copy(&mut limited, &mut out).map_err(broken)?;
    if copied != size {
        return Err(format!(
            "{rel} has a size in its header that is not the body's length"
        ));
    }
    drop(out);
    drop(stream);
    if !reader
        .fill_buf()
        .map_err(|e| io("cannot be read", e))?
        .is_empty()
    {
        return Err(format!("{rel} has bytes after its zlib stream"));
    }
    *budget -= size;
    Ok(kind)
}

/// Fill `gates.dirty` from the gateway's own objects when G is not in ign's
/// store.
///
/// A no-op (an empty [`FetchStats`]) unless `gates.needs_gateway_objects`.
/// Otherwise the canonical digest of the tree at G is compared with `live`:
/// `dirty` becomes the differing paths, relative to G (what the gateway's git
/// actually holds), capped at 200 with `dirty_count` the full number,
/// `dirty_source` becomes [`DirtySource::GatewayObjects`] and
/// `needs_gateway_objects` clears. A failure leaves `gates` untouched.
pub async fn dirty_from_gateway(
    git: &HostGit,
    checkout: &Path,
    read: &dyn GitHostRead,
    project: &str,
    gates: &mut NodeGates,
    live: &CanonicalDigest,
) -> Result<FetchStats, GitGateFailure> {
    if !gates.needs_gateway_objects {
        return Ok(FetchStats::default());
    }
    let head = gates
        .head
        .clone()
        .ok_or_else(|| gateway("the gateway repo has no HEAD commit to compare"))?;
    let (digest, stats) = gateway_head_digest(git, checkout, read, project, &head).await?;
    let (list, count) = capped_dirty(live, &digest);
    gates.dirty = Some(list);
    gates.dirty_count = count;
    gates.dirty_source = Some(DirtySource::GatewayObjects);
    gates.needs_gateway_objects = false;
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEX38: &str = "0123456789abcdef0123456789abcdef01234567";
    const NAME: &str = "0123456789abcdef0123456789abcdef01234567";

    fn file(rel: &str, size: u64) -> GitObjectEntry {
        GitObjectEntry {
            rel: rel.to_string(),
            size,
            kind: GitObjectEntryKind::File,
        }
    }

    fn loose(fan: &str) -> String {
        format!("objects/{fan}/{}", &HEX38[..38])
    }

    #[test]
    fn shapes_accept_only_the_fixed_names() {
        assert_eq!(check_object_rel(&loose("ab")), Ok(ObjectRel::Loose));
        assert_eq!(
            check_object_rel(&format!("objects/pack/pack-{NAME}.pack")),
            Ok(ObjectRel::Pack {
                name: NAME.to_string(),
                idx: false
            })
        );
        assert_eq!(
            check_object_rel(&format!("objects/pack/pack-{NAME}.idx")),
            Ok(ObjectRel::Pack {
                name: NAME.to_string(),
                idx: true
            })
        );
        for bad in [
            "config",
            "hooks/pre-commit",
            "objects/info/packs",
            "objects/info/alternates",
            "objects/ab/../../config",
            "objects/AB/0123456789abcdef0123456789abcdef01234567",
            "objects/ab/short",
            "objects/pack/tmp_pack_x",
            "objects/pack/pack-abc.pack",
            &format!("objects/pack/pack-{NAME}.keep"),
            &format!("objects/pack/pack-{NAME}.bitmap"),
            &format!("objects/pack/pack-{NAME}.rev"),
            &format!("/objects/pack/pack-{NAME}.pack"),
            &format!("objects\\pack\\pack-{NAME}.pack"),
            "objects/ab/0123456789abcdef0123456789abcdef0123456\0",
            "",
        ] {
            assert!(check_object_rel(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn from_entries_keeps_fixed_shapes_and_drops_the_rest() {
        let entries = vec![
            file(&loose("ab"), 10),
            file(&loose("cd"), 20),
            file("objects/info/packs", 5),
            file("objects/info/alternates", 5),
            file(&format!("objects/pack/pack-{NAME}.pack"), 100),
            file(&format!("objects/pack/pack-{NAME}.idx"), 7),
            file(&format!("objects/pack/pack-{NAME}.keep"), 0),
            file(&format!("objects/pack/pack-{NAME}.bitmap"), 9),
            file("objects/pack/tmp_pack_x", 3),
            // A pack without its index is dropped.
            file(&format!("objects/pack/pack-{}.pack", "1".repeat(40)), 50),
            GitObjectEntry {
                rel: "objects/ee".to_string(),
                size: 0,
                kind: GitObjectEntryKind::Other,
            },
        ];
        let list = GitObjectList::from_entries(&entries).expect("list");
        assert_eq!(list.loose.len(), 2);
        assert_eq!(
            list.packs,
            vec![GitPackFiles {
                name: NAME.to_string(),
                pack_size: 100,
                idx_size: 7
            }]
        );
        assert_eq!(list.total_bytes(), 10 + 20 + 100 + 7);
        list.validate().expect("valid");
    }

    #[test]
    fn from_entries_refuses_symlinks_and_hostile_names_anywhere() {
        let link = GitObjectEntry {
            rel: "objects/info/packs".to_string(),
            size: 0,
            kind: GitObjectEntryKind::Symlink,
        };
        assert!(GitObjectList::from_entries(&[link]).is_err());
        for bad in [
            "objects/ab/../../config",
            "/etc/passwd",
            "objects\\ab\\x",
            "objects/ab/x\0",
            "objects//ab",
            "objects/./ab",
        ] {
            assert!(
                GitObjectList::from_entries(&[file(bad, 1)]).is_err(),
                "{bad:?}"
            );
        }
        // The same path twice.
        let twice = vec![file(&loose("ab"), 1), file(&loose("ab"), 1)];
        assert!(GitObjectList::from_entries(&twice).is_err());
    }

    #[test]
    fn validate_enforces_the_caps_before_any_fetch() {
        let mut list = GitObjectList::default();
        for i in 0..=GIT_OBJECTS_LOOSE_MAX {
            list.loose.push(GitObjectFile {
                rel: format!("objects/{:02x}/{:038x}", i % 256, i),
                size: 1,
            });
        }
        assert!(list.validate().unwrap_err().contains("loose objects"));

        let packs = GitObjectList {
            loose: Vec::new(),
            packs: (0..=GIT_OBJECTS_PACKS_MAX)
                .map(|i| GitPackFiles {
                    name: format!("{i:040x}"),
                    pack_size: 1,
                    idx_size: 1,
                })
                .collect(),
        };
        assert!(packs.validate().unwrap_err().contains("packs"));

        let big = GitObjectList {
            loose: Vec::new(),
            packs: vec![
                GitPackFiles {
                    name: format!("{:040x}", 1),
                    pack_size: GIT_OBJECTS_FETCH_MAX_BYTES / 2 + 1,
                    idx_size: 1,
                },
                GitPackFiles {
                    name: format!("{:040x}", 2),
                    pack_size: GIT_OBJECTS_FETCH_MAX_BYTES / 2 + 1,
                    idx_size: 1,
                },
            ],
        };
        assert!(big.validate().unwrap_err().contains("byte cap"));
    }

    /// WR-A6 (27-27): a cap refusal says which cap and what to do.
    #[test]
    fn validate_cap_refusals_name_the_remedy() {
        let mut loose = GitObjectList::default();
        for i in 0..=GIT_OBJECTS_LOOSE_MAX {
            loose.loose.push(GitObjectFile {
                rel: format!("objects/{:02x}/{:038x}", i % 256, i),
                size: 1,
            });
        }
        let packs = GitObjectList {
            loose: Vec::new(),
            packs: (0..=GIT_OBJECTS_PACKS_MAX)
                .map(|i| GitPackFiles {
                    name: format!("{i:040x}"),
                    pack_size: 1,
                    idx_size: 1,
                })
                .collect(),
        };
        let big = GitObjectList {
            loose: Vec::new(),
            packs: (1..=2)
                .map(|i| GitPackFiles {
                    name: format!("{i:040x}"),
                    pack_size: GIT_OBJECTS_FETCH_MAX_BYTES / 2 + 1,
                    idx_size: 1,
                })
                .collect(),
        };
        let mut stats = FetchStats::default();
        reserve(&mut stats, GIT_OBJECTS_FETCH_MAX_BYTES).expect("exactly the cap");
        let reserved = reserve(&mut stats, 1)
            .expect_err("over the cap")
            .to_string();
        for (text, cap) in [
            (
                loose.validate().unwrap_err(),
                GIT_OBJECTS_LOOSE_MAX.to_string(),
            ),
            (
                packs.validate().unwrap_err(),
                GIT_OBJECTS_PACKS_MAX.to_string(),
            ),
            (
                big.validate().unwrap_err(),
                GIT_OBJECTS_FETCH_MAX_BYTES.to_string(),
            ),
            (reserved, GIT_OBJECTS_FETCH_MAX_BYTES.to_string()),
        ] {
            assert!(text.contains(&cap), "names its cap {cap}: {text}");
            assert!(text.contains("git gc"), "names the remedy: {text}");
            assert!(text.contains("administrator"), "{text}");
            assert!(text.contains("project repository"), "{text}");
        }
    }

    #[test]
    fn validate_rechecks_names_of_a_hand_built_list() {
        let list = GitObjectList {
            loose: vec![GitObjectFile {
                rel: "objects/ab/../../config".to_string(),
                size: 1,
            }],
            packs: Vec::new(),
        };
        assert!(list.validate().unwrap_err().contains("not a loose object"));
        let list = GitObjectList {
            loose: Vec::new(),
            packs: vec![GitPackFiles {
                name: "../config".to_string(),
                pack_size: 1,
                idx_size: 1,
            }],
        };
        assert!(list.validate().is_err());
    }

    #[test]
    fn reserve_refuses_before_the_fetch_that_passes_the_cap() {
        let mut stats = FetchStats::default();
        reserve(&mut stats, GIT_OBJECTS_FETCH_MAX_BYTES).expect("exactly the cap");
        assert!(reserve(&mut stats, 1).is_err());
        assert_eq!(stats.bytes, GIT_OBJECTS_FETCH_MAX_BYTES);
    }
}

/// 27-47: `verify_fetched_objects` against objects and packs made by host git.
#[cfg(test)]
mod verify_tests {
    use super::*;
    use std::io::Write as _;
    use std::path::PathBuf;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// A repo with a few commits over files that delta well.
    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        git(dir.path(), &["init", "-q", "-b", "production"]);
        for round in 0..4 {
            for file in 0..3 {
                let body: String = (0..200)
                    .map(|line| format!("file {file} line {line} round {round}\n"))
                    .collect();
                std::fs::write(dir.path().join(format!("f{file}.txt")), body).expect("write");
            }
            git(dir.path(), &["add", "-A"]);
            git(
                dir.path(),
                &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "round"],
            );
        }
        dir
    }

    /// Every object file below `repo/.git/objects` of the fixed shapes, as the
    /// listing reports them.
    fn listed(repo: &Path) -> Vec<GitObjectFile> {
        let objects = repo.join(".git").join("objects");
        let mut out = Vec::new();
        let mut stack = vec![objects.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("ls") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let rel = path
                    .strip_prefix(repo.join(".git"))
                    .expect("below")
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                if check_object_rel(&rel).is_ok() {
                    let size = std::fs::metadata(&path).expect("meta").len();
                    out.push(GitObjectFile { rel, size });
                }
            }
        }
        out.sort_by(|a, b| a.rel.cmp(&b.rel));
        out
    }

    /// Copy `files` of `repo` into a download directory laid out as the fetch
    /// lays it out (`<dir>/<rel>`).
    fn download(repo: &Path, files: &[GitObjectFile]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        for file in files {
            let target = dir.path().join(&file.rel);
            std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
            std::fs::copy(repo.join(".git").join(&file.rel), &target).expect("copy");
            // git writes its objects read-only; a download is ours to change
            let mut mode = std::fs::metadata(&target).expect("meta").permissions();
            mode.set_readonly(false);
            std::fs::set_permissions(&target, mode).expect("writable");
        }
        dir
    }

    fn only(files: &[GitObjectFile], keep: impl Fn(&GitObjectFile) -> bool) -> Vec<GitObjectFile> {
        files.iter().filter(|f| keep(f)).cloned().collect()
    }

    fn is_pack(file: &GitObjectFile) -> bool {
        file.rel.starts_with("objects/pack/")
    }

    /// The verifier on a blocking thread, as `fetch_git_objects` runs it.
    async fn verify(dir: &Path, files: &[GitObjectFile]) -> Result<(), String> {
        let (dir, files) = (dir.to_path_buf(), files.to_vec());
        tokio::task::spawn_blocking(move || verify_fetched_objects(&dir, &files))
            .await
            .expect("join")
    }

    async fn verify_capped(dir: &Path, files: &[GitObjectFile], cap: u64) -> Result<(), String> {
        let (dir, files) = (dir.to_path_buf(), files.to_vec());
        tokio::task::spawn_blocking(move || verify_fetched_objects_capped(&dir, &files, cap))
            .await
            .expect("join")
    }

    fn zlib(bytes: &[u8]) -> Vec<u8> {
        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(bytes).expect("deflate");
        enc.finish().expect("finish")
    }

    /// A repo packed into one pack: the pack and idx rels are the only object
    /// files.
    fn packed() -> (tempfile::TempDir, Vec<GitObjectFile>) {
        let repo = repo();
        git(repo.path(), &["repack", "-a", "-d", "-q", "--depth=20"]);
        git(repo.path(), &["prune-packed"]);
        let files = listed(repo.path());
        assert!(files.iter().all(is_pack) && files.len() == 2, "{files:?}");
        (repo, files)
    }

    fn flip(path: &Path, at: i64) {
        let mut bytes = std::fs::read(path).expect("read");
        let at = if at < 0 {
            bytes.len() - usize::try_from(-at).expect("small")
        } else {
            usize::try_from(at).expect("small")
        };
        bytes[at] ^= 0x01;
        std::fs::write(path, bytes).expect("write");
    }

    #[tokio::test]
    async fn verify_accepts_honest_objects() {
        let repo = repo();
        let loose = listed(repo.path());
        assert!(loose.len() > 10 && !loose.iter().any(is_pack));
        let dir = download(repo.path(), &loose);
        verify(dir.path(), &loose).await.expect("loose objects");

        let (packed, files) = packed();
        let dir = download(packed.path(), &files);
        verify(dir.path(), &files)
            .await
            .expect("a pack and its idx");
        // a pack with deltas in it was really checked
        let pack = files
            .iter()
            .find(|f| f.rel.ends_with(".pack"))
            .expect("pack");
        assert!(pack.size > 1000);
    }

    #[tokio::test]
    async fn verify_accepts_every_object_type_and_the_empty_blob() {
        let repo = repo();
        git(repo.path(), &["tag", "-a", "-m", "annotated", "v1"]);
        let empty = repo.path().join("empty.txt");
        std::fs::write(&empty, b"").expect("empty");
        git(repo.path(), &["add", "empty.txt"]);
        git(
            repo.path(),
            &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "empty"],
        );
        let files = listed(repo.path());
        let dir = download(repo.path(), &files);
        verify(dir.path(), &files)
            .await
            .expect("blob, tree, commit, tag");
    }

    #[tokio::test]
    async fn verify_refuses_a_loose_object_whose_bytes_do_not_hash_to_its_name() {
        let repo = repo();
        let loose = listed(repo.path());
        let dir = download(repo.path(), &loose);
        // another object's file under this object's name
        let (a, b) = (&loose[0], &loose[1]);
        std::fs::copy(dir.path().join(&b.rel), dir.path().join(&a.rel)).expect("swap");
        let files: Vec<GitObjectFile> = loose
            .iter()
            .map(|f| GitObjectFile {
                rel: f.rel.clone(),
                size: if f.rel == a.rel { b.size } else { f.size },
            })
            .collect();
        let why = verify(dir.path(), &files)
            .await
            .expect_err("a swapped file");
        assert!(why.contains(&a.rel), "names the object file: {why}");
        assert!(!why.contains(&b.rel), "names only the refused file: {why}");
    }

    #[tokio::test]
    async fn verify_refuses_a_loose_object_with_a_wrong_size_header() {
        let repo = repo();
        let loose = listed(repo.path());
        let dir = download(repo.path(), &loose);
        // the header claims 99 bytes, the body holds 5
        let bytes = zlib(b"blob 99\0hello");
        let target = &loose[0];
        std::fs::write(dir.path().join(&target.rel), &bytes).expect("write");
        let files: Vec<GitObjectFile> = loose
            .iter()
            .map(|f| GitObjectFile {
                rel: f.rel.clone(),
                size: if f.rel == target.rel {
                    bytes.len() as u64
                } else {
                    f.size
                },
            })
            .collect();
        let why = verify(dir.path(), &files)
            .await
            .expect_err("a lying header");
        assert!(why.contains(&target.rel), "{why}");
        assert!(why.contains("size"), "says which check failed: {why}");
        assert!(!why.contains("hello"), "never bytes: {why}");
    }

    #[tokio::test]
    async fn verify_refuses_a_loose_object_that_does_not_inflate() {
        let repo = repo();
        let loose = listed(repo.path());
        let dir = download(repo.path(), &loose);
        let target = &loose[0];
        let junk = b"this is not a zlib stream at all".to_vec();
        std::fs::write(dir.path().join(&target.rel), &junk).expect("write");
        let files: Vec<GitObjectFile> = loose
            .iter()
            .map(|f| GitObjectFile {
                rel: f.rel.clone(),
                size: if f.rel == target.rel {
                    junk.len() as u64
                } else {
                    f.size
                },
            })
            .collect();
        let why = verify(dir.path(), &files).await.expect_err("junk");
        assert!(why.contains(&target.rel), "{why}");
        assert!(why.contains("inflate"), "{why}");
        assert!(!why.contains("not a zlib"), "never bytes: {why}");
    }

    #[tokio::test]
    async fn verify_refuses_a_loose_object_with_bytes_after_its_stream() {
        let repo = repo();
        let loose = listed(repo.path());
        let dir = download(repo.path(), &loose);
        let target = &loose[0];
        let path = dir.path().join(&target.rel);
        let mut bytes = std::fs::read(&path).expect("read");
        bytes.extend_from_slice(b"trailing");
        std::fs::write(&path, &bytes).expect("write");
        let files: Vec<GitObjectFile> = loose
            .iter()
            .map(|f| GitObjectFile {
                rel: f.rel.clone(),
                size: if f.rel == target.rel {
                    bytes.len() as u64
                } else {
                    f.size
                },
            })
            .collect();
        let why = verify(dir.path(), &files)
            .await
            .expect_err("trailing bytes");
        assert!(why.contains(&target.rel), "{why}");
    }

    #[tokio::test]
    async fn verify_refuses_a_loose_object_of_an_unknown_type() {
        let repo = repo();
        let loose = listed(repo.path());
        let dir = download(repo.path(), &loose);
        let target = &loose[0];
        let bytes = zlib(b"widget 5\0hello");
        std::fs::write(dir.path().join(&target.rel), &bytes).expect("write");
        let files: Vec<GitObjectFile> = loose
            .iter()
            .map(|f| GitObjectFile {
                rel: f.rel.clone(),
                size: if f.rel == target.rel {
                    bytes.len() as u64
                } else {
                    f.size
                },
            })
            .collect();
        let why = verify(dir.path(), &files).await.expect_err("a widget");
        assert!(why.contains(&target.rel), "{why}");
        assert!(why.contains("type"), "{why}");
    }

    #[tokio::test]
    async fn verify_refuses_an_object_that_inflates_past_the_cap() {
        let repo = repo();
        let loose = listed(repo.path());
        let dir = download(repo.path(), &loose);
        // a body of 100 000 bytes compresses to a few hundred
        let target = &loose[0];
        let mut raw = b"blob 100000\0".to_vec();
        raw.extend(std::iter::repeat_n(b'a', 100_000));
        let bytes = zlib(&raw);
        assert!(bytes.len() < 1000);
        std::fs::write(dir.path().join(&target.rel), &bytes).expect("write");
        let files = [GitObjectFile {
            rel: target.rel.clone(),
            size: bytes.len() as u64,
        }];
        let why = verify_capped(dir.path(), &files, 1000)
            .await
            .expect_err("a bomb");
        assert!(why.contains(&target.rel), "{why}");
        assert!(why.contains("cap"), "{why}");
        // with room it is only a hash mismatch, not the cap
        let why = verify_capped(dir.path(), &files, 1_000_000)
            .await
            .expect_err("not this object's bytes");
        assert!(!why.contains("cap"), "{why}");
    }

    #[tokio::test]
    async fn verify_refuses_a_pack_with_a_bad_trailer() {
        let (repo, files) = packed();
        let dir = download(repo.path(), &files);
        let pack = files
            .iter()
            .find(|f| f.rel.ends_with(".pack"))
            .expect("pack");
        // one flipped byte in the body
        flip(&dir.path().join(&pack.rel), 100);
        let why = verify(dir.path(), &files)
            .await
            .expect_err("a flipped byte");
        assert!(why.contains(&pack.rel), "{why}");
    }

    #[tokio::test]
    async fn verify_refuses_a_pack_whose_last_bytes_are_not_the_ones_its_idx_records() {
        let (repo, files) = packed();
        let dir = download(repo.path(), &files);
        let pack = files
            .iter()
            .find(|f| f.rel.ends_with(".pack"))
            .expect("pack");
        flip(&dir.path().join(&pack.rel), -1);
        let why = verify(dir.path(), &files)
            .await
            .expect_err("a flipped trailer");
        assert!(why.contains(&pack.rel), "{why}");
    }

    #[tokio::test]
    async fn verify_refuses_an_idx_that_records_another_pack() {
        let (one, one_files) = packed();
        let other = tempfile::tempdir().expect("tempdir");
        git(other.path(), &["init", "-q", "-b", "production"]);
        for i in 0..40 {
            std::fs::write(other.path().join(format!("g{i}")), format!("another {i}\n"))
                .expect("write");
        }
        git(other.path(), &["add", "-A"]);
        git(
            other.path(),
            &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "other"],
        );
        git(other.path(), &["repack", "-a", "-d", "-q"]);
        git(other.path(), &["prune-packed"]);
        let other_files = listed(other.path());
        let other_idx = other_files
            .iter()
            .find(|f| f.rel.ends_with(".idx"))
            .expect("idx");

        let dir = download(one.path(), &one_files);
        let idx = one_files
            .iter()
            .find(|f| f.rel.ends_with(".idx"))
            .expect("idx");
        // the idx of a different pack beside this pack
        std::fs::copy(
            other.path().join(".git").join(&other_idx.rel),
            dir.path().join(&idx.rel),
        )
        .expect("swap");
        let files: Vec<GitObjectFile> = one_files
            .iter()
            .map(|f| GitObjectFile {
                rel: f.rel.clone(),
                size: if f.rel == idx.rel {
                    other_idx.size
                } else {
                    f.size
                },
            })
            .collect();
        let why = verify(dir.path(), &files).await.expect_err("another idx");
        assert!(why.contains(&idx.rel) || why.contains("pack-"), "{why}");
        assert!(why.contains("records"), "says which check failed: {why}");
    }

    #[tokio::test]
    async fn verify_refuses_an_idx_with_a_bad_trailer() {
        let (repo, files) = packed();
        let dir = download(repo.path(), &files);
        let idx = files.iter().find(|f| f.rel.ends_with(".idx")).expect("idx");
        // the idx's own checksum is its last 20 bytes
        flip(&dir.path().join(&idx.rel), -1);
        let why = verify(dir.path(), &files).await.expect_err("a bad idx");
        assert!(why.contains("pack-"), "names the pack: {why}");
    }

    #[tokio::test]
    async fn verify_refuses_an_idx_whose_body_was_changed() {
        let (repo, files) = packed();
        let dir = download(repo.path(), &files);
        let idx = files.iter().find(|f| f.rel.ends_with(".idx")).expect("idx");
        flip(&dir.path().join(&idx.rel), 1100);
        verify(dir.path(), &files).await.expect_err("a changed idx");
    }

    #[tokio::test]
    async fn verify_refuses_a_pack_without_its_idx_in_the_fetch() {
        let (repo, files) = packed();
        let dir = download(repo.path(), &files);
        let only_pack = only(&files, |f| f.rel.ends_with(".pack"));
        let why = verify(dir.path(), &only_pack)
            .await
            .expect_err("no idx beside the pack");
        assert!(why.contains(&only_pack[0].rel), "{why}");
        assert!(why.contains("idx"), "{why}");
    }

    #[tokio::test]
    async fn verify_takes_a_lone_index_only_as_far_as_its_shape() {
        let (repo, files) = packed();
        let dir = download(repo.path(), &files);
        let lone = only(&files, |f| f.rel.ends_with(".idx"));
        verify(dir.path(), &lone)
            .await
            .expect("an index to read ids from");
        // a file too short to hold its two checksums is no index
        let idx = dir.path().join(&lone[0].rel);
        std::fs::write(&idx, b"short").expect("write");
        let short = [GitObjectFile {
            rel: lone[0].rel.clone(),
            size: 5,
        }];
        let why = verify(dir.path(), &short).await.expect_err("too short");
        assert!(why.contains(&lone[0].rel), "{why}");
    }

    #[tokio::test]
    async fn verify_refuses_a_file_that_is_not_the_listed_size_or_not_a_regular_file() {
        let repo = repo();
        let loose = listed(repo.path());
        let dir = download(repo.path(), &loose);
        let mut files = loose.clone();
        files[0].size += 1;
        let why = verify(dir.path(), &files).await.expect_err("wrong size");
        assert!(why.contains(&files[0].rel), "{why}");

        #[cfg(unix)]
        {
            let outside = tempfile::tempdir().expect("tempdir");
            let target = dir.path().join(&loose[1].rel);
            std::fs::copy(&target, outside.path().join("o")).expect("copy");
            std::fs::remove_file(&target).expect("rm");
            std::os::unix::fs::symlink(outside.path().join("o"), &target).expect("link");
            let why = verify(dir.path(), &loose).await.expect_err("a link");
            assert!(why.contains(&loose[1].rel), "{why}");
        }
    }

    #[tokio::test]
    async fn verify_an_empty_fetch_is_fine_and_a_missing_file_is_refused() {
        let repo = repo();
        let loose = listed(repo.path());
        let dir = tempfile::tempdir().expect("tempdir");
        verify(dir.path(), &[]).await.expect("nothing to check");
        let why = verify(dir.path(), &loose[..1])
            .await
            .expect_err("not downloaded");
        assert!(why.contains(&loose[0].rel), "{why}");
    }

    #[tokio::test]
    async fn verify_leaves_the_download_as_it_was() {
        let (repo, mut files) = packed();
        let loose_repo = self::repo();
        files.extend(listed(loose_repo.path()));
        let dir = download(repo.path(), &only(&files, is_pack));
        let loose_files = only(&files, |f| !is_pack(f));
        for file in &loose_files {
            let target = dir.path().join(&file.rel);
            std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
            std::fs::copy(loose_repo.path().join(".git").join(&file.rel), target).expect("copy");
        }
        let names = |dir: &Path| -> Vec<PathBuf> {
            let mut out = Vec::new();
            let mut stack = vec![dir.to_path_buf()];
            while let Some(d) = stack.pop() {
                for entry in std::fs::read_dir(&d).expect("ls") {
                    let path = entry.expect("entry").path();
                    if path.is_dir() {
                        stack.push(path);
                    } else {
                        out.push(path);
                    }
                }
            }
            out.sort();
            out
        };
        let before = names(dir.path());
        verify(dir.path(), &files).await.expect("honest");
        assert_eq!(names(dir.path()), before, "no scratch file is left behind");
    }
}
