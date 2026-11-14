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
use std::path::Path;

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
                "the gateway repo holds {} loose objects, over the {GIT_OBJECTS_LOOSE_MAX} cap",
                self.loose.len()
            ));
        }
        if self.packs.len() > GIT_OBJECTS_PACKS_MAX {
            return Err(format!(
                "the gateway repo holds {} packs, over the {GIT_OBJECTS_PACKS_MAX} cap",
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
                 {GIT_OBJECTS_FETCH_MAX_BYTES} byte cap"
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
            "reading the gateway's objects would copy more than {GIT_OBJECTS_FETCH_MAX_BYTES} bytes"
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
        for i in chosen {
            let pack = &list.packs[i];
            reserve(stats, pack.pack_size)?;
            let file = GitObjectFile {
                rel: pack.pack_rel(),
                size: pack.pack_size,
            };
            read.fetch_git_objects(project, std::slice::from_ref(&file), scratch.git_dir())
                .await
                .map_err(|e| host("a gateway pack cannot be read", e))?;
            install_index(&stage, scratch.git_dir(), pack)?;
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
