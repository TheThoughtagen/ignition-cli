//! A parameterised, deterministic generator for the `.git` handover at scale
//! (Phase 27, plan 27-06, D-18).
//!
//! [`GitShape`] says how big the repository is: bytes of the gateway's `.git`,
//! files in it, packs, loose objects, commits, tracked files (in scope and out
//! of scope of a project artifact) and how many commits the release is ahead.
//! [`GitShape::neutral`] holds round numbers chosen for CI speed; they are not
//! measurements of anything. The shape of a real repository is supplied
//! privately: `IGN_PROD_SHAPE_FILE` names a TOML file outside this repository
//! and [`GitShape::from_env`] reads it.
//!
//! [`ProdShapeGit::generate`] builds, with host git and `git fast-import`:
//!
//! - a bare remote on branch `production` with a release tag,
//! - a release checkout at the tag, `advance_commits` ahead of the gateway,
//! - a gateway data dir whose `projects/CapApp` is a module-style clone: remote
//!   named for the repo, branch tracking, `[http] sslVerify = false`, the
//!   remote-tracking ref in `packed-refs`, the in-scope files on disk.
//!
//! Content is a fixed-seed xorshift stream; most bytes are incompressible and
//! live in `images/`. The history is written in `packs` fast-import runs, so
//! the gateway holds that many packs; `loose_objects` unreferenced loose
//! objects absorb whatever byte count the history did not reach, and loose tag
//! refs fill the file count.
//!
//! The budgets of this phase are the constants below; no budget scales with
//! the shape and none is ignored.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::Deserialize;

use super::git_fixture::{BRANCH, GATEWAY, PROJECT, REMOTE, gateway_form, git, in_scope};
use ignition_core::deploy::git_advance::{AdvancePrep, prepare_advance};
use ignition_core::deploy::git_remote::HostGit;
use ignition_core::deploy::git_state::{GitRefs, GitState, GitStateKind, classify};
use ignition_core::deploy::project_artifact::build_artifact;
use ignition_core::deploy::transport::local::LocalFsTransport;
use ignition_core::deploy::transport::projects::GitHostRead;

// ---- the budgets (D-18); none scales with the shape --------------------------

/// Offline: read and parse the gateway `.git` state.
pub const OFFLINE_READ_STATE: Duration = Duration::from_secs(5);
/// Offline: host-local save of `.git`.
pub const OFFLINE_SAVE: Duration = Duration::from_secs(30);
/// Offline: restore plus manifest verify.
pub const OFFLINE_RESTORE_VERIFY: Duration = Duration::from_secs(30);
/// Offline: build the pack of the objects the gateway lacks.
pub const OFFLINE_PACK: Duration = Duration::from_secs(10);
/// Offline: build the index.
pub const OFFLINE_INDEX: Duration = Duration::from_secs(5);
/// Offline: predict the drift.
pub const OFFLINE_DRIFT: Duration = Duration::from_secs(10);
/// Offline: the write preflight (probe, size-only `.git` read, free space).
pub const OFFLINE_PREFLIGHT: Duration = Duration::from_secs(5);
/// Offline: the prepare's whole git step (read state, release checkout check,
/// remote view, ancestry, dirty, advance preparation), 27-13.
pub const OFFLINE_PREPARE: Duration = Duration::from_secs(30);
/// Offline: the gateway-object read for an unknown HEAD (D-24).
pub const OFFLINE_GATEWAY_OBJECTS: Duration = Duration::from_secs(15);
/// ign's machine: resolve the gateway objects and digest the head (D-24).
pub const OFFLINE_GATEWAY_DIGEST: Duration = Duration::from_secs(15);

/// Linux host: save.
pub const HOST_LINUX_SAVE: Duration = Duration::from_secs(60);
/// Linux host: restore plus verify.
pub const HOST_LINUX_RESTORE_VERIFY: Duration = Duration::from_secs(60);
/// Linux host: advance upload (pack, refs, index).
pub const HOST_LINUX_UPLOAD: Duration = Duration::from_secs(30);
/// Linux host: write preflight.
pub const HOST_LINUX_PREFLIGHT: Duration = Duration::from_secs(10);
/// Linux host: gateway-object read for an unknown HEAD.
pub const HOST_LINUX_GATEWAY_OBJECTS: Duration = Duration::from_secs(60);

/// Windows host: save.
pub const HOST_WINDOWS_SAVE: Duration = Duration::from_secs(120);
/// Windows host: restore plus verify.
pub const HOST_WINDOWS_RESTORE_VERIFY: Duration = Duration::from_secs(120);
/// Windows host: advance upload (pack, refs, index).
pub const HOST_WINDOWS_UPLOAD: Duration = Duration::from_secs(60);
/// Windows host: write preflight.
pub const HOST_WINDOWS_PREFLIGHT: Duration = Duration::from_secs(20);
/// Windows host: gateway-object read for an unknown HEAD.
pub const HOST_WINDOWS_GATEWAY_OBJECTS: Duration = Duration::from_secs(120);

// ---- the shape --------------------------------------------------------------

/// How big the repository is. The production shape is supplied privately
/// through `IGN_PROD_SHAPE_FILE`; the in-repo defaults are round numbers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitShape {
    /// Total bytes of the files below the gateway's `.git`.
    pub git_bytes: u64,
    /// Regular files below the gateway's `.git`.
    pub git_files: usize,
    /// `objects/pack/*.pack` files in the gateway's `.git`.
    pub packs: usize,
    /// Loose objects in the gateway's `.git`.
    pub loose_objects: usize,
    /// Commits reachable from the gateway's HEAD.
    pub commits: usize,
    /// Files in the gateway HEAD's tree (`in_scope` plus `out_of_scope`).
    pub tracked: usize,
    /// Tracked files an artifact carries.
    pub in_scope: usize,
    /// Tracked files an artifact does not carry.
    pub out_of_scope: usize,
    /// Commits the release is ahead of the gateway's HEAD.
    pub advance_commits: usize,
}

impl GitShape {
    /// Round numbers for CI speed; not a measurement of anything.
    pub fn neutral() -> GitShape {
        GitShape {
            git_bytes: 128 * 1024 * 1024,
            git_files: 600,
            packs: 24,
            loose_objects: 200,
            commits: 500,
            tracked: 3_000,
            in_scope: 1_000,
            out_of_scope: 2_000,
            advance_commits: 20,
        }
    }

    /// Parse and check a shape file's text. The error never repeats a value.
    pub fn from_toml(text: &str) -> Result<GitShape, String> {
        let shape: GitShape = toml::from_str(text).map_err(|e| e.message().to_string())?;
        shape.validate()?;
        Ok(shape)
    }

    /// The shape `IGN_PROD_SHAPE_FILE` names, else [`GitShape::neutral`]. A
    /// file inside this repository is refused: a real shape stays private.
    pub fn from_env() -> GitShape {
        let Some(path) = std::env::var_os("IGN_PROD_SHAPE_FILE").filter(|v| !v.is_empty()) else {
            return GitShape::neutral();
        };
        let path = outside_repository(Path::new(&path))
            .unwrap_or_else(|e| panic!("IGN_PROD_SHAPE_FILE: {e}"));
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("IGN_PROD_SHAPE_FILE cannot be read ({:?})", e.kind()));
        GitShape::from_toml(&text).unwrap_or_else(|e| panic!("IGN_PROD_SHAPE_FILE: {e}"))
    }

    /// The relations the generator needs.
    pub fn validate(&self) -> Result<(), String> {
        let need = |ok: bool, what: &str| {
            if ok { Ok(()) } else { Err(what.to_string()) }
        };
        need(self.git_bytes >= 1 << 20, "git_bytes is below 1 MiB")?;
        need(self.packs >= 1, "packs is zero")?;
        need(self.loose_objects >= 1, "loose_objects is zero")?;
        need(self.commits >= self.packs, "commits is below packs")?;
        need(self.advance_commits >= 1, "advance_commits is zero")?;
        need(self.in_scope >= 3, "in_scope is below 3")?;
        need(self.out_of_scope >= 8, "out_of_scope is below 8")?;
        need(
            self.tracked == self.in_scope + self.out_of_scope,
            "tracked is not in_scope plus out_of_scope",
        )?;
        Ok(())
    }
}

/// The root of this repository (the one the test sources live in).
fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root")
}

/// `path` made absolute with its existing part canonicalized; a `..` in the
/// part that does not exist yet is refused.
fn resolve(path: &Path) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| format!("the working directory is unreadable ({:?})", e.kind()))?
            .join(path)
    };
    let mut tail: Vec<OsString> = Vec::new();
    let mut current = absolute.as_path();
    loop {
        if let Ok(mut resolved) = current.canonicalize() {
            for part in tail.iter().rev() {
                resolved.push(part);
            }
            return Ok(resolved);
        }
        match (current.parent(), current.file_name()) {
            (Some(parent), Some(name)) => {
                tail.push(name.to_os_string());
                current = parent;
            }
            _ => return Err("the path does not resolve".to_string()),
        }
        if current
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        {
            return Err("the path holds a `..` that cannot be resolved".to_string());
        }
    }
}

/// `path` resolved, refused when it is inside this repository.
pub fn outside_repository(path: &Path) -> Result<PathBuf, String> {
    let resolved = resolve(path)?;
    if resolved.starts_with(repository_root()) {
        return Err("the path is inside this repository; it must stay outside".to_string());
    }
    Ok(resolved)
}

// ---- deterministic content --------------------------------------------------

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        Rng(z | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// `len` incompressible bytes from `seed`.
fn random_bytes(seed: u64, len: usize) -> Vec<u8> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::with_capacity(len + 8);
    while out.len() < len {
        out.extend_from_slice(&rng.next().to_le_bytes());
    }
    out.truncate(len);
    out
}

/// Python-looking text of about `len` bytes.
fn text_body(seed: u64, len: usize) -> Vec<u8> {
    let mut rng = Rng::new(seed);
    let mut out = String::with_capacity(len + 40);
    let mut n = 0u32;
    while out.len() < len {
        out.push_str(&format!("v{n} = 0x{:016x}\n", rng.next()));
        n += 1;
    }
    out.into_bytes()
}

/// A valid JSON document of about `len` bytes.
fn json_body(seed: u64, len: usize) -> Vec<u8> {
    let mut rng = Rng::new(seed);
    let mut out = String::from("{\"root\":{\"n\":[0");
    while out.len() < len {
        out.push_str(&format!(",{}", rng.next() % 1_000_000));
    }
    out.push_str("]}}\n");
    out.into_bytes()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Body {
    /// Exact bytes.
    Fixed,
    /// Script text (mutable).
    Code,
    /// A view document (mutable).
    View,
    /// Incompressible bytes.
    Binary,
    /// A tag export document.
    Tags,
    /// A stylesheet.
    Theme,
    /// A document.
    Doc,
}

#[derive(Debug, Clone)]
struct FileSpec {
    path: String,
    body: Body,
    seed: u64,
    size: usize,
    fixed: Vec<u8>,
}

impl FileSpec {
    fn content(&self, version: u32, size: usize) -> Vec<u8> {
        let seed = self.seed ^ (u64::from(version) + 1).wrapping_mul(0xA5A5_5A5A_1234_5679);
        match self.body {
            Body::Fixed => self.fixed.clone(),
            Body::Code | Body::Theme | Body::Doc => text_body(seed, size),
            Body::View | Body::Tags => json_body(seed, size),
            Body::Binary => random_bytes(seed, size),
        }
    }

    fn mutable(&self) -> bool {
        matches!(self.body, Body::Code | Body::View)
    }
}

/// Mutated in-scope files per history commit, and per advance commit.
const HISTORY_MODS: usize = 3;
const ADVANCE_MODS: usize = 4;
/// Bytes of the one out-of-scope file an advance commit replaces.
const ADVANCE_IMAGE_BYTES: usize = 24 * 1024;

fn mix(a: u64, b: u64) -> u64 {
    let mut rng = Rng::new(a.wrapping_mul(0x1000_0000_01B3) ^ b);
    rng.next()
}

/// The planned files, their grouping into units added together, and the
/// version of each mutable file as the commits are written in order.
struct History {
    files: Vec<FileSpec>,
    /// Half-open ranges of `files`, in the order the units are added.
    units: Vec<(usize, usize)>,
    versions: Vec<u32>,
    /// Indexes of mutable files already added.
    mutable_added: Vec<usize>,
    /// Indexes of binary files already added.
    images_added: Vec<usize>,
    next_unit: usize,
    commits: usize,
}

impl History {
    fn plan(shape: &GitShape) -> History {
        let resources = (shape.in_scope - 1) / 2;
        let extra = (shape.in_scope - 1) % 2;
        let mut in_units: Vec<Vec<FileSpec>> = vec![vec![FileSpec {
            path: "project.json".to_string(),
            body: Body::Fixed,
            seed: 0,
            size: 0,
            fixed: b"{\"title\":\"CapApp\"}\n".to_vec(),
        }]];
        for r in 0..resources {
            let code = r % 2 == 0;
            let (dir, member, scope, body) = if code {
                (
                    format!("ignition/script-python/pkg{:03}/mod{r:05}", r / 64),
                    "code.py",
                    "A",
                    Body::Code,
                )
            } else {
                (
                    format!(
                        "com.inductiveautomation.perspective/views/Folder{:03}/V{r:05}",
                        r / 64
                    ),
                    "view.json",
                    "G",
                    Body::View,
                )
            };
            let seed = mix(1, r as u64);
            let size = 300 + (mix(2, r as u64) % 1_700) as usize;
            let helper = extra == 1 && r == 0;
            let listing = if helper {
                format!("{{\"scope\":\"{scope}\",\"files\":[\"{member}\",\"helper.py\"]}}\n")
            } else {
                format!("{{\"scope\":\"{scope}\",\"files\":[\"{member}\"]}}\n")
            };
            let mut unit = vec![
                FileSpec {
                    path: format!("{dir}/{member}"),
                    body,
                    seed,
                    size,
                    fixed: Vec::new(),
                },
                FileSpec {
                    path: format!("{dir}/resource.json"),
                    body: Body::Fixed,
                    seed: 0,
                    size: 0,
                    fixed: listing.into_bytes(),
                },
            ];
            if helper {
                unit.push(FileSpec {
                    path: format!("{dir}/helper.py"),
                    body: Body::Doc,
                    seed: mix(3, r as u64),
                    size: 400,
                    fixed: Vec::new(),
                });
            }
            in_units.push(unit);
        }
        // Out-of-scope files: the image bytes are sized after the text is known.
        let mut out_units: Vec<Vec<FileSpec>> = Vec::new();
        let mut text_bytes: usize = in_units.iter().flatten().map(|f| f.size).sum();
        let mut images: Vec<usize> = Vec::new();
        for o in 0..shape.out_of_scope {
            let seed = mix(4, o as u64);
            let (path, body, size) = match o {
                0 => ("README.md".to_string(), Body::Doc, 900),
                1 => (".gitignore".to_string(), Body::Doc, 120),
                _ => match o % 100 {
                    0..=69 => (format!("images/img{o:05}.bin"), Body::Binary, 0),
                    70..=89 => (
                        format!("tags/default/tags{o:05}.json"),
                        Body::Tags,
                        200 + (mix(5, o as u64) % 1_500) as usize,
                    ),
                    90..=97 => (
                        format!("themes/theme{o:05}.css"),
                        Body::Theme,
                        300 + (mix(6, o as u64) % 1_200) as usize,
                    ),
                    _ => (
                        format!("docs/note{o:05}.md"),
                        Body::Doc,
                        500 + (mix(7, o as u64) % 2_000) as usize,
                    ),
                },
            };
            if body != Body::Binary {
                text_bytes += size;
            } else {
                images.push(o);
            }
            out_units.push(vec![FileSpec {
                path,
                body,
                seed,
                size,
                fixed: Vec::new(),
            }]);
        }

        // Size the images so the history lands a little under `git_bytes`;
        // the loose objects take up the rest.
        let churn = shape.commits * HISTORY_MODS * 1_150;
        let objects = shape.tracked + shape.commits * 8;
        let fixed_cost = (text_bytes + churn) * 6 / 10 + objects * 130;
        let target = (shape.git_bytes as f64 * 0.93) as usize;
        let image_budget = target.saturating_sub(fixed_cost);
        let base = image_budget / images.len().max(1);
        for o in images {
            let jitter = 80 + (mix(8, o as u64) % 41) as usize;
            out_units[o][0].size = (base * jitter / 100).max(64);
        }

        // Interleave the two lists by relative position.
        let mut files: Vec<FileSpec> = Vec::new();
        let mut units: Vec<(usize, usize)> = Vec::new();
        let (mut u, mut o) = (0usize, 0usize);
        let (nu, no) = (in_units.len(), out_units.len());
        while u < nu || o < no {
            let take_in = o >= no || (u < nu && u * no <= o * nu);
            let unit = if take_in {
                u += 1;
                &in_units[u - 1]
            } else {
                o += 1;
                &out_units[o - 1]
            };
            let start = files.len();
            files.extend(unit.iter().cloned());
            units.push((start, files.len()));
        }
        let versions = vec![0; files.len()];
        History {
            files,
            units,
            versions,
            mutable_added: Vec::new(),
            images_added: Vec::new(),
            next_unit: 0,
            commits: shape.commits,
        }
    }

    /// Write commit `c` of the stream to `w`; `from` continues the branch.
    /// Commits `0..commits` are the gateway's; later ones are the advance.
    fn emit_commit(&mut self, w: &mut dyn Write, c: usize, from: bool) -> io::Result<()> {
        let mut changed: BTreeMap<usize, usize> = BTreeMap::new(); // file -> size
        if c < self.commits {
            while self.next_unit < self.units.len()
                && self.next_unit * self.commits / self.units.len() <= c
            {
                let (start, end) = self.units[self.next_unit];
                for index in start..end {
                    let spec = &self.files[index];
                    changed.insert(index, spec.size);
                    if spec.mutable() {
                        self.mutable_added.push(index);
                    } else if spec.body == Body::Binary {
                        self.images_added.push(index);
                    }
                }
                self.next_unit += 1;
            }
        }
        let mods = if c < self.commits {
            HISTORY_MODS
        } else {
            ADVANCE_MODS
        };
        if !self.mutable_added.is_empty() {
            for m in 0..mods {
                let pick = (c * 7919 + m * 104_729 + 13) % self.mutable_added.len();
                let index = self.mutable_added[pick];
                if let std::collections::btree_map::Entry::Vacant(slot) = changed.entry(index) {
                    self.versions[index] += 1;
                    slot.insert(self.files[index].size);
                }
            }
        }
        if c >= self.commits && !self.images_added.is_empty() {
            let index = self.images_added[((c - self.commits) * 31 + 5) % self.images_added.len()];
            self.versions[index] = 1_000 + (c - self.commits) as u32;
            changed.insert(index, ADVANCE_IMAGE_BYTES);
        }

        let stamp = 1_700_000_000 + c as u64 * 3_600;
        let message = format!("change {c}\n");
        write!(
            w,
            "commit refs/heads/{BRANCH}\nauthor Test <test@example.invalid> {stamp} +0000\n\
             committer Test <test@example.invalid> {stamp} +0000\ndata {}\n{message}",
            message.len()
        )?;
        if from {
            writeln!(w, "from refs/heads/{BRANCH}^0")?;
        }
        for (index, size) in changed {
            let spec = &self.files[index];
            let bytes = spec.content(self.versions[index], size);
            writeln!(w, "M 100644 inline {}\ndata {}", spec.path, bytes.len())?;
            w.write_all(&bytes)?;
            w.write_all(b"\n")?;
        }
        w.write_all(b"\n")
    }
}

// ---- host git plumbing ------------------------------------------------------

fn git_command(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1");
    command
}

/// Run `git fast-import` in `repo` and feed it `feed`.
fn fast_import(
    repo: &Path,
    one_pack_per_run: bool,
    feed: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) {
    let mut errors = tempfile::tempfile().expect("a scratch file");
    let mut command = git_command(repo);
    if one_pack_per_run {
        command.args(["-c", "fastimport.unpackLimit=0"]);
    }
    let mut child = command
        .args(["fast-import", "--quiet"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::from(errors.try_clone().expect("clone")))
        .spawn()
        .expect("git fast-import starts");
    {
        let stdin = child.stdin.take().expect("stdin");
        let mut writer = io::BufWriter::with_capacity(4 << 20, stdin);
        let wrote = feed(&mut writer).and_then(|()| writer.flush());
        drop(writer);
        let status = child.wait().expect("git fast-import ends");
        if !status.success() || wrote.is_err() {
            let mut text = String::new();
            io::Seek::rewind(&mut errors).expect("rewind");
            errors.read_to_string(&mut text).expect("read");
            panic!("git fast-import failed ({wrote:?}): {text}");
        }
    }
}

/// Run git in `dir` with `input` on stdin; return stdout.
fn git_stdin(dir: &Path, args: &[&str], input: &[u8]) -> Vec<u8> {
    let mut child = git_command(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("git starts");
    let mut stdin = child.stdin.take().expect("stdin");
    let owned = input.to_vec();
    let feeder = std::thread::spawn(move || {
        let _ = stdin.write_all(&owned);
    });
    let out = child.wait_with_output().expect("git ends");
    feeder.join().expect("feeder");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

/// Put the index at HEAD and write the in-scope files of HEAD to the work
/// tree (the gateway keeps nothing else after an import).
fn checkout_in_scope(repo: &Path) {
    git(repo, &["read-tree", "HEAD"]);
    let listing = git_stdin(repo, &["ls-files", "-z"], b"");
    let mut wanted: Vec<u8> = Vec::new();
    for path in listing.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        if in_scope(std::str::from_utf8(path).expect("utf8 path")) {
            wanted.extend_from_slice(path);
            wanted.push(0);
        }
    }
    git_stdin(repo, &["checkout-index", "-f", "-z", "--stdin"], &wanted);
}

// ---- measuring ---------------------------------------------------------------

/// What a `.git` directory holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measured {
    /// Bytes of every regular file.
    pub bytes: u64,
    /// Regular files.
    pub files: usize,
    /// `objects/pack/*.pack` files.
    pub packs: usize,
    /// Files in `objects/<2 hex>/`.
    pub loose: usize,
}

/// Measure `git_dir` (no symlink is followed).
pub fn measure_git_dir(git_dir: &Path) -> Measured {
    let mut measured = Measured {
        bytes: 0,
        files: 0,
        packs: 0,
        loose: 0,
    };
    walk(git_dir, git_dir, &mut measured);
    measured
}

fn walk(root: &Path, dir: &Path, into: &mut Measured) {
    for entry in std::fs::read_dir(dir).expect("read_dir") {
        let entry = entry.expect("entry");
        let kind = entry.file_type().expect("file type");
        let path = entry.path();
        if kind.is_dir() {
            walk(root, &path, into);
        } else if kind.is_file() {
            into.bytes += entry.metadata().expect("metadata").len();
            into.files += 1;
            let rel = path.strip_prefix(root).expect("below root");
            let parts: Vec<&str> = rel.iter().filter_map(|p| p.to_str()).collect();
            if parts.len() == 3 && parts[0] == "objects" {
                if parts[1] == "pack" && parts[2].ends_with(".pack") {
                    into.packs += 1;
                } else if parts[1].len() == 2 && parts[1].bytes().all(|b| b.is_ascii_hexdigit()) {
                    into.loose += 1;
                }
            }
        }
    }
}

/// Copy a directory tree (regular files and directories; modes kept).
pub fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for entry in std::fs::read_dir(from).expect("read_dir") {
        let entry = entry.expect("entry");
        let kind = entry.file_type().expect("file type");
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target);
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &target).expect("copy");
        }
    }
}

// ---- the generated repository ------------------------------------------------

/// A generated remote, release checkout and gateway data dir.
pub struct ProdShapeGit {
    /// The shape it was built from.
    pub shape: GitShape,
    /// The bare remote.
    pub remote: PathBuf,
    /// The release checkout, at [`release_tag`](Self::release_tag).
    pub checkout: PathBuf,
    /// The gateway data dir.
    pub data_dir: PathBuf,
    /// `data/projects/CapApp`.
    pub project: PathBuf,
    /// The release tag.
    pub release_tag: String,
    /// The commit the gateway is at.
    pub gateway_head: String,
    /// The release commit.
    pub release_head: String,
}

impl ProdShapeGit {
    /// Build everything below `root` (which must exist and be empty).
    pub fn generate(root: &Path, shape: &GitShape) -> ProdShapeGit {
        shape.validate().expect("a usable shape");
        let remote = root.join("capapp.git");
        let checkout = root.join("release");
        let data_dir = root.join("data");
        let project = data_dir.join("projects").join(PROJECT);
        std::fs::create_dir_all(&project).expect("mkdir");

        let mut history = History::plan(shape);
        git(root, &["init", "-q", "-b", BRANCH, to_str(&project)]);
        for run in 0..shape.packs {
            let start = run * shape.commits / shape.packs;
            let end = (run + 1) * shape.commits / shape.packs;
            fast_import(&project, true, |w| {
                for c in start..end {
                    history.emit_commit(w, c, c == start && run > 0)?;
                }
                Ok(())
            });
        }
        let gateway_head = git(&project, &["rev-parse", &format!("refs/heads/{BRANCH}")]);

        // The remote, made before the gateway repo gets its module settings.
        git(
            root,
            &["clone", "-q", "--bare", to_str(&project), to_str(&remote)],
        );

        // The gateway's module-style clone.
        git(
            &project,
            &["remote", "add", "-t", BRANCH, REMOTE, to_str(&remote)],
        );
        git(
            &project,
            &["config", &format!("branch.{BRANCH}.remote"), REMOTE],
        );
        git(
            &project,
            &[
                "config",
                &format!("branch.{BRANCH}.merge"),
                &format!("refs/heads/{BRANCH}"),
            ],
        );
        git(&project, &["config", "http.sslVerify", "false"]);
        std::fs::write(
            project.join(".git/packed-refs"),
            format!(
                "# pack-refs with: peeled fully-peeled sorted \n{gateway_head} refs/remotes/{REMOTE}/{BRANCH}\n"
            ),
        )
        .expect("packed-refs");
        checkout_in_scope(&project);

        // The release checkout: the gateway's history plus the advance.
        let release_tag = "v1.1.0".to_string();
        git(
            root,
            &[
                "clone",
                "-q",
                "--no-checkout",
                "-b",
                BRANCH,
                to_str(&remote),
                to_str(&checkout),
            ],
        );
        fast_import(&checkout, false, |w| {
            for c in shape.commits..shape.commits + shape.advance_commits {
                history.emit_commit(w, c, c == shape.commits)?;
            }
            Ok(())
        });
        git(&checkout, &["tag", &release_tag]);
        git(&checkout, &["push", "-q", "origin", BRANCH, &release_tag]);
        let release_head = git(&checkout, &["rev-parse", "HEAD"]);
        checkout_in_scope(&checkout);

        let generated = ProdShapeGit {
            shape: shape.clone(),
            remote,
            checkout,
            data_dir,
            project,
            release_tag,
            gateway_head,
            release_head,
        };
        generated.fill_gateway_git(shape);
        generated
    }

    /// Bring the gateway's `.git` to the file and byte counts: loose tag refs
    /// for the files, unreferenced loose objects for the bytes.
    fn fill_gateway_git(&self, shape: &GitShape) {
        let git_dir = self.git_dir();
        let now = measure_git_dir(&git_dir);
        let pads = shape
            .git_files
            .saturating_sub(now.files + shape.loose_objects);
        let pad_bytes = pads as u64 * (self.gateway_head.len() as u64 + 1);
        let residual = shape.git_bytes as i64 - now.bytes as i64 - pad_bytes as i64;
        let each = (residual / shape.loose_objects as i64).max(64) as usize;
        // A loose object is zlib stored blocks plus a few header bytes.
        let content = each.saturating_sub(16 + 5 * each.div_ceil(65_535)).max(16);

        let scratch = tempfile::tempdir().expect("a scratch dir");
        let mut listing = String::new();
        for i in 0..shape.loose_objects {
            let path = scratch.path().join(format!("loose{i}"));
            std::fs::write(&path, random_bytes(mix(9, i as u64), content)).expect("write");
            listing.push_str(&format!("{}\n", path.display()));
        }
        git_stdin(
            &self.project,
            &["hash-object", "-w", "--stdin-paths"],
            listing.as_bytes(),
        );

        let tags = git_dir.join("refs/tags");
        std::fs::create_dir_all(&tags).expect("mkdir");
        for i in 0..pads {
            std::fs::write(
                tags.join(format!("pad-{i:05}")),
                format!("{}\n", self.gateway_head),
            )
            .expect("write ref");
        }
    }

    /// `data/projects/CapApp/.git`.
    pub fn git_dir(&self) -> PathBuf {
        self.project.join(".git")
    }

    /// The local-filesystem transport over the gateway data dir.
    pub fn node(&self) -> LocalFsTransport {
        LocalFsTransport::new(self.data_dir.clone(), "nodes.master")
    }

    /// The gateway's name in ledger paths.
    pub fn gateway(&self) -> &'static str {
        GATEWAY
    }

    /// The project's name.
    pub fn project_name(&self) -> &'static str {
        PROJECT
    }

    /// The gateway repo as a host read shows it, classified, with its refs.
    pub async fn gateway_state(&self, host: &HostGit) -> (GitState, GitRefs) {
        let files = self
            .node()
            .read_git_state(PROJECT)
            .await
            .expect("host read");
        files.check_caps().expect("caps");
        let config = host
            .parse_config(files.config.as_deref().expect("config"))
            .await
            .expect("config");
        let refs = host
            .parse_refs(&files)
            .await
            .expect("refs")
            .expect("some refs");
        let state = classify(&files, Some(&config), Some(&refs));
        (state, refs)
    }

    /// The artifact's members (the in-scope paths the import writes).
    pub fn artifact_members(&self) -> BTreeSet<String> {
        build_artifact(&self.checkout)
            .expect("artifact builds")
            .digest
            .members
            .keys()
            .cloned()
            .collect()
    }

    /// The managed state and the prepared advance to the release commit.
    pub async fn prepare(&self, host: &HostGit) -> (GitState, AdvancePrep) {
        let (state, refs) = self.gateway_state(host).await;
        assert_eq!(state.kind, GitStateKind::Managed, "{state:?}");
        let members = self.artifact_members();
        let prep = prepare_advance(
            host,
            &self.checkout,
            &state,
            &refs,
            &self.release_head,
            &members,
        )
        .await
        .expect("prepare");
        (state, prep)
    }

    /// The `resource.json` paths the simulated import re-serialises the way
    /// the gateway does (the first three script folders).
    pub fn gateway_form_targets(&self) -> Vec<String> {
        self.artifact_members()
            .into_iter()
            .filter(|p| p.starts_with("ignition/") && p.ends_with("/resource.json"))
            .take(3)
            .collect()
    }

    /// Simulate the module's import (R-3): the project directory is emptied
    /// (every name but `.git` when `keep_git`), then the artifact's files are
    /// written, `gateway_form` members re-serialised.
    pub fn simulate_import(&self, keep_git: bool, gateway_form_members: &[String]) {
        for entry in std::fs::read_dir(&self.project).expect("read_dir") {
            let path = entry.expect("entry").path();
            if keep_git && path.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            if path.is_dir() {
                std::fs::remove_dir_all(&path).expect("rm dir");
            } else {
                std::fs::remove_file(&path).expect("rm file");
            }
        }
        let artifact = build_artifact(&self.checkout).expect("artifact builds");
        let mut archive =
            zip::ZipArchive::new(io::Cursor::new(artifact.zip)).expect("artifact zip");
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).expect("entry");
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().to_string();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).expect("read member");
            if gateway_form_members.contains(&name) {
                bytes = gateway_form(&String::from_utf8(bytes).expect("utf8")).into_bytes();
            }
            let target = self.project.join(&name);
            std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
            std::fs::write(target, bytes).expect("write");
        }
    }

    /// The in-scope files of the project directory as the gateway's export.
    pub fn export_zip(&self) -> Vec<u8> {
        super::git_fixture::live_export_from(&self.project)
    }

    /// The export's members, by path.
    pub fn export_members(&self) -> BTreeMap<String, Vec<u8>> {
        let mut archive =
            zip::ZipArchive::new(io::Cursor::new(self.export_zip())).expect("export zip");
        let mut members = BTreeMap::new();
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).expect("entry");
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().to_string();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).expect("read member");
            members.insert(name, bytes);
        }
        members
    }

    /// Paths of the gateway HEAD's tree, split by artifact scope.
    pub fn tree_scope_counts(&self) -> (usize, usize) {
        let text = git(
            &self.project,
            &["ls-tree", "-r", "--name-only", &self.gateway_head],
        );
        let in_scope_count = text.lines().filter(|p| in_scope(p)).count();
        (in_scope_count, text.lines().count() - in_scope_count)
    }

    /// Commits reachable from the gateway's HEAD.
    pub fn gateway_commits(&self) -> usize {
        git(&self.project, &["rev-list", "--count", &self.gateway_head])
            .parse()
            .expect("a count")
    }

    /// Commits the release has that the gateway's HEAD lacks.
    pub fn advance_count(&self) -> usize {
        git(
            &self.checkout,
            &[
                "rev-list",
                "--count",
                &format!("{}..{}", self.gateway_head, self.release_head),
            ],
        )
        .parse()
        .expect("a count")
    }

    /// A Designer commit on top of the gateway HEAD that is never pushed
    /// (27-10): one in-scope script or JSON member gets a line appended and is
    /// committed. The work tree keeps the change, so the live export matches
    /// the new HEAD. With `packed` the commit's own objects (and no others)
    /// move into a pack of their own and out of the loose store, as after a
    /// gateway `git gc` of the new history. Returns the new commit id.
    pub fn designer_commit(&self, packed: bool) -> String {
        let member = self
            .artifact_members()
            .into_iter()
            .find(|m| {
                (m.ends_with(".py") || m.ends_with(".json"))
                    && !m.ends_with("/resource.json")
                    && m != "project.json"
            })
            .expect("an in-scope script or JSON member");
        let path = self.project.join(&member);
        let mut text = std::fs::read(&path).expect("read member");
        text.extend_from_slice(b"\n");
        std::fs::write(&path, text).expect("write member");
        git(&self.project, &["add", &member]);
        git(&self.project, &["commit", "-q", "-m", "designer"]);
        let head = git(&self.project, &["rev-parse", "HEAD"]);
        if packed {
            let listing = git(
                &self.project,
                &["rev-list", "--objects", &head, "--not", &self.gateway_head],
            );
            git_stdin(
                &self.project,
                &["pack-objects", "-q", ".git/objects/pack/pack"],
                format!("{listing}\n").as_bytes(),
            );
            git(&self.project, &["prune-packed", "-q"]);
        }
        head
    }

    /// Write the remote and the gateway data dir below `dir` for the rig and
    /// VM gates (a release checkout is a clone of the remote at the tag).
    /// Refused when `dir` is inside this repository.
    pub fn emit_to(&self, dir: &Path) -> PathBuf {
        let dir = outside_repository(dir).unwrap_or_else(|e| panic!("PRODSHAPE_GIT_OUT: {e}"));
        std::fs::create_dir_all(&dir).expect("mkdir");
        copy_tree(&self.remote, &dir.join("capapp.git"));
        copy_tree(&self.data_dir, &dir.join("data"));
        dir
    }
}

fn to_str(path: &Path) -> &str {
    path.to_str().expect("utf8 path")
}

// ---- timing the object read (27-10) ---------------------------------------------

/// A [`GitHostRead`] over a node that adds up the time spent inside
/// `list_git_objects` and `fetch_git_objects`, so a row can time the host-side
/// read apart from the ign-side resolve and digest.
pub struct TimedRead<R: GitHostRead> {
    inner: R,
    spent: std::sync::Mutex<Duration>,
}

impl<R: GitHostRead> TimedRead<R> {
    /// Wrap `inner`.
    pub fn new(inner: R) -> TimedRead<R> {
        TimedRead {
            inner,
            spent: std::sync::Mutex::new(Duration::ZERO),
        }
    }

    /// Time spent in the two object reads so far.
    pub fn spent(&self) -> Duration {
        *self.spent.lock().expect("lock")
    }

    fn add(&self, since: std::time::Instant) {
        *self.spent.lock().expect("lock") += since.elapsed();
    }
}

#[async_trait::async_trait]
impl<R: GitHostRead> GitHostRead for TimedRead<R> {
    async fn read_git_state(
        &self,
        project: &str,
    ) -> Result<ignition_core::deploy::git_state::GitFiles, ignition_core::error::CoreError> {
        self.inner.read_git_state(project).await
    }

    async fn git_manifest(
        &self,
        project: &str,
    ) -> Result<
        ignition_core::deploy::transport::projects::GitManifest,
        ignition_core::error::CoreError,
    > {
        self.inner.git_manifest(project).await
    }

    async fn save_manifest(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<
        ignition_core::deploy::transport::projects::GitManifest,
        ignition_core::error::CoreError,
    > {
        self.inner.save_manifest(gateway, project, run_id).await
    }

    async fn git_file_digest(
        &self,
        project: &str,
        name: &ignition_core::deploy::git_advance::GitWriteName,
    ) -> Result<Option<String>, ignition_core::error::CoreError> {
        self.inner.git_file_digest(project, name).await
    }

    async fn read_git_yaml(&self) -> Result<Option<Vec<u8>>, ignition_core::error::CoreError> {
        self.inner.read_git_yaml().await
    }

    async fn free_bytes(
        &self,
        gateway: &str,
        project: &str,
    ) -> Result<u64, ignition_core::error::CoreError> {
        self.inner.free_bytes(gateway, project).await
    }

    async fn git_size(
        &self,
        project: &str,
    ) -> Result<ignition_core::deploy::transport::projects::GitSize, ignition_core::error::CoreError>
    {
        self.inner.git_size(project).await
    }

    async fn list_git_objects(
        &self,
        project: &str,
    ) -> Result<ignition_core::deploy::git_objects::GitObjectList, ignition_core::error::CoreError>
    {
        let started = std::time::Instant::now();
        let out = self.inner.list_git_objects(project).await;
        self.add(started);
        out
    }

    async fn fetch_git_objects(
        &self,
        project: &str,
        files: &[ignition_core::deploy::git_objects::GitObjectFile],
        dest: &Path,
    ) -> Result<(), ignition_core::error::CoreError> {
        let started = std::time::Instant::now();
        let out = self.inner.fetch_git_objects(project, files, dest).await;
        self.add(started);
        out
    }
}
