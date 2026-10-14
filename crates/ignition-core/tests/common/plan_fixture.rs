//! `PlanFixture`: a temp gateway data directory (core, mode and local
//! collection layers) plus a copy of the tracer repo whose master node is
//! `local: { data_dir }` pointing at it (22-01).
//!
//! Typical use:
//!
//! ```ignore
//! let fx = PlanFixture::new();
//! let (declared, reader) = fx.load("dev");
//! fx.live_render("dev", &declared);   // make live equal the render
//! let result = deploy_plan(&declared, reader.as_ref()).await?;
//! ```
//!
//! The reader reads lazily at `deploy_plan` time, so layers can be written
//! after `load`. `live_render` writes the ACTUAL render (it may mint a uuid),
//! never a hand-copied one. 22-03 adds `ledger(...)`.

use std::path::{Path, PathBuf};

use ignition_core::actions::deploy::{Declared, load_declared};
use ignition_core::deploy::transport::{self, TreeReader};
use tempfile::TempDir;

const TRACER_REPO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/deploy/repos/tracer"
);

/// The mode named by the tracer target; the mode collection is `dev`.
pub const MODE: &str = "dev";

pub struct PlanFixture {
    data: TempDir,
    repo: TempDir,
}

impl PlanFixture {
    /// An empty data directory (`config/resources/` present, no collections)
    /// and a fresh copy of the tracer repo.
    pub fn new() -> PlanFixture {
        let data = TempDir::new().expect("data dir");
        std::fs::create_dir_all(data.path().join("config").join("resources")).expect("resources");
        let repo = TempDir::new().expect("repo dir");
        copy_dir(Path::new(TRACER_REPO), repo.path());
        PlanFixture { data, repo }
    }

    /// The gateway data directory.
    pub fn data_dir(&self) -> &Path {
        self.data.path()
    }

    /// The directory of `collection` (`core`, the mode name or `local`).
    pub fn collection_dir(&self, collection: &str) -> PathBuf {
        self.data
            .path()
            .join("config")
            .join("resources")
            .join(collection)
    }

    fn put(&self, collection: &str, path: &str, bytes: &[u8]) {
        let dest = path
            .split('/')
            .fold(self.collection_dir(collection), |acc, c| acc.join(c));
        std::fs::create_dir_all(dest.parent().expect("parent")).expect("create dirs");
        std::fs::write(dest, bytes).expect("write file");
    }

    /// Write `bytes` at collection-relative `path` in the `core` collection.
    pub fn core(&self, path: &str, bytes: &[u8]) {
        self.put("core", path, bytes);
    }

    /// Write `bytes` at collection-relative `path` in the mode collection.
    pub fn mode(&self, path: &str, bytes: &[u8]) {
        self.put(MODE, path, bytes);
    }

    /// Write `bytes` at collection-relative `path` in the `local` collection.
    pub fn local(&self, path: &str, bytes: &[u8]) {
        self.put("local", path, bytes);
    }

    /// Write every rendered file of `declared` under `collection`.
    pub fn live_render(&self, collection: &str, declared: &Declared) {
        for (path, bytes) in &declared.outcome.tree.files {
            self.put(collection, path.as_str(), bytes);
        }
    }

    /// Read back a collection-relative file.
    pub fn read(&self, collection: &str, path: &str) -> Vec<u8> {
        let src = path
            .split('/')
            .fold(self.collection_dir(collection), |acc, c| acc.join(c));
        std::fs::read(src).expect("read file")
    }

    /// Remove a collection-relative file or directory.
    pub fn remove(&self, collection: &str, path: &str) {
        let target = path
            .split('/')
            .fold(self.collection_dir(collection), |acc, c| acc.join(c));
        if target.is_dir() {
            std::fs::remove_dir_all(target).expect("remove dir");
        } else {
            std::fs::remove_file(target).expect("remove file");
        }
    }

    /// The repo copy, with `targets/<env>.yaml` rewritten so the master node
    /// is `local: { data_dir }` over this fixture's data directory.
    pub fn repo(&self, env: &str) -> PathBuf {
        let block = format!(
            "    local:\n      data_dir: '{}'\n",
            self.data.path().display()
        );
        self.repo_with_master(env, &block)
    }

    /// The repo copy, with `targets/<env>.yaml` rewritten so the master node
    /// is `profile: dev` followed by `master_extra` (raw YAML lines indented
    /// four spaces; may be empty).
    pub fn repo_with_master(&self, env: &str, master_extra: &str) -> PathBuf {
        let yaml = format!(
            "schema_version: 1\ngateway: tracer-gw\nmode: {MODE}\nnodes:\n  master:\n    profile: dev\n{master_extra}"
        );
        let targets = self.repo.path().join("targets");
        std::fs::create_dir_all(&targets).expect("targets dir");
        std::fs::write(targets.join(format!("{env}.yaml")), yaml).expect("target file");
        self.repo.path().to_path_buf()
    }

    /// `load_declared` over the `local:` repo and the reader `for_node`
    /// builds for `nodes.master`: exactly the sequence the verb uses.
    pub fn load(&self, env: &str) -> (Declared, Box<dyn TreeReader>) {
        let repo = self.repo(env);
        let declared = load_declared(&repo, env).expect("declared tree loads");
        let reader = transport::for_node(
            &repo,
            &declared.target_file,
            "nodes.master",
            &declared.target.nodes.master,
        )
        .expect("master has host access");
        (declared, reader)
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create dir");
    for entry in std::fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("entry");
        let dest = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), dest).expect("copy file");
        }
    }
}
