//! The declared-names ledger, format v1 (Phase 22, D-05, D-07).
//!
//! The ledger is an on-box record of the mode-collection files the last
//! apply wrote for one gateway and mode. `deploy plan` reads it to learn which
//! names this tool previously declared: the delete set is exactly the
//! ledger's files minus the files being rendered, so a resource the tool
//! never wrote is never proposed for deletion. Nothing in Phase 22 writes a
//! ledger; [`Ledger::to_bytes`] is the writer Phase 23 and the tests use.
//!
//! ## Format v1
//!
//! One JSON object, strict: unknown fields, duplicate keys and nesting beyond
//! the shared depth cap are refused, and the file is at most
//! [`LEDGER_MAX_BYTES`] (8 MiB), strict UTF-8, never empty.
//!
//! ```json
//! {
//!   "schema_version": 1,
//!   "gateway": "<target gateway>",
//!   "mode": "<target mode>",
//!   "render_sha256": "<64 lowercase hex>",
//!   "run_id": "<[A-Za-z0-9._-]{1,128}>",
//!   "files": ["<module>/<type>/..."]
//! }
//! ```
//!
//! `files` holds collection-relative paths, byte-sorted and unique, with no
//! per-file hashes (the ledger is a name list, not a drift base). Every path
//! has at least three components, no dot-named component, is not
//! `config-mode.json`, is not a case-variant of another entry (a
//! case-insensitive host would treat the two as one file), and sits under a
//! type the gateway describes (the OpenAPI resource types, or the four tag
//! types under `ignition`).
//!
//! ## Location (Decision B, plan 22-12, applied by 22-13)
//!
//! The ledger is `ign-deploy/<gateway>/<mode>/ledger.json` ([`ledger_rel`]),
//! and what it is relative to depends on how the node is reached
//! ([`LedgerRoot`], [`ledger_location`]):
//!
//! | Node | Root | On the Windows capture host |
//! |---|---|---|
//! | `ssh` (Linux or Windows) | [`LedgerRoot::AccountHome`]: the service account's home directory | `C:\Users\svc-ignition-deploy\ign-deploy\<gateway>\<mode>\ledger.json` (sftp `/C:/Users/svc-ignition-deploy/ign-deploy/...`) |
//! | `local` | [`LedgerRoot::DataDir`]: the node's data directory | `<data_dir>/ign-deploy/<gateway>/<mode>/ledger.json` |
//!
//! Why: the C6 captures showed that the service account can write its own
//! home but nothing under the data directory, so Decision B(i) puts the
//! ledger, the journal and the staging area there
//! (`ign-deploy/<gateway>/<mode>/staging/<run_id>/`, [`staging_rel`]), outside
//! every collection the gateway scans. It is never inside `config/resources/`
//! and never dot-named. A ledger is per service account, not per gateway: a
//! second deploy account on one gateway does not see the first one's ledger.
//! Nothing writes a ledger before Phase 23, so nothing was migrated.
//!
//! ## Refusals
//!
//! A ledger that exists but cannot be trusted is `deploy_ledger_invalid`
//! (exit 6); plan never falls back to an empty delete set (D-07). The problem
//! text names fields and listed paths only, never other file content and never
//! the foreign gateway or mode a copied ledger names.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::apply::journal::JOURNAL_FILE;
use super::apply::record::RECORD_FILE;
use super::layout::{RelPath, TAG_TYPES};
use super::node::{self, Node};
use super::rules::{GATEWAY_OWNED_FILES, openapi_describes};
use crate::error::CoreError;

/// File name of the ledger.
pub const LEDGER_FILE: &str = "ledger.json";

/// The largest ledger accepted (8 MiB), checked before any parse.
pub const LEDGER_MAX_BYTES: usize = 8_388_608;

/// The only schema version this build reads and writes.
const SCHEMA_VERSION: u32 = 1;

/// The longest `run_id` accepted.
const RUN_ID_MAX: usize = 128;

/// The ledger's top-level keys, in write order.
const FIELDS: [&str; 6] = [
    "schema_version",
    "gateway",
    "mode",
    "render_sha256",
    "run_id",
    "files",
];

/// The declared-names ledger.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ledger {
    /// Always 1.
    pub schema_version: u32,
    /// The gateway directory name the ledger belongs to (the target's
    /// `gateway`).
    pub gateway: String,
    /// The deployment mode the ledger belongs to (the target's `mode`).
    pub mode: String,
    /// `render_sha256` of the render the writing apply deployed: 64
    /// lowercase hex digits.
    pub render_sha256: String,
    /// Identifier of the apply run that wrote it: `[A-Za-z0-9._-]{1,128}`.
    pub run_id: String,
    /// The mode-collection files that apply wrote, byte-sorted and unique.
    pub files: Vec<String>,
}

/// What the ledger location is relative to (module doc, Decision B).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerRoot {
    /// The node's data directory: `local` nodes.
    DataDir,
    /// The service account's home directory: `ssh` nodes.
    AccountHome,
}

impl LedgerRoot {
    /// The root for how `node` is reached: an `ssh` node keeps its ledger in
    /// the account home, a `local` node (or one with neither) in its data
    /// directory.
    pub fn for_node(node: &super::target::NodeTarget) -> LedgerRoot {
        if node.ssh.is_some() {
            LedgerRoot::AccountHome
        } else {
            LedgerRoot::DataDir
        }
    }
}

/// The ledger path below its root: `ign-deploy/<gateway>/<mode>/ledger.json`.
pub fn ledger_rel(gateway: &str, mode: &str) -> String {
    format!("ign-deploy/{gateway}/{mode}/{LEDGER_FILE}")
}

/// The staging directory of one apply run below its root:
/// `ign-deploy/<gateway>/<mode>/staging/<run_id>` (Decision B(i)).
pub fn staging_rel(gateway: &str, mode: &str, run_id: &str) -> String {
    format!("ign-deploy/{gateway}/{mode}/staging/{run_id}")
}

/// The on-box journal of the apply run in flight, below its root:
/// `ign-deploy/<gateway>/<mode>/journal.json` (D-04, D-09). Never dot-named
/// and never inside `config/resources/`; it resolves under the same
/// [`LedgerRoot`] as the ledger.
pub fn journal_rel(gateway: &str, mode: &str) -> String {
    format!("ign-deploy/{gateway}/{mode}/{JOURNAL_FILE}")
}

/// The pre-apply snapshot directory of one run below its root:
/// `ign-deploy/<gateway>/<mode>/snapshots/<run_id>` (D-04, D-07). `run_id`
/// must have passed `RunId::parse`.
pub fn snapshot_rel(gateway: &str, mode: &str, run_id: &str) -> String {
    format!("ign-deploy/{gateway}/{mode}/snapshots/{run_id}")
}

/// The run record of one run below its root:
/// `ign-deploy/<gateway>/<mode>/snapshots/<run_id>/record.json` (D-07).
/// `run_id` must have passed `RunId::parse`.
pub fn record_rel(gateway: &str, mode: &str, run_id: &str) -> String {
    format!("{}/{RECORD_FILE}", snapshot_rel(gateway, mode, run_id))
}

/// `rel` as an error and a log name it: the relative path for a
/// [`LedgerRoot::DataDir`] node, `~/` plus the relative path for a
/// [`LedgerRoot::AccountHome`] node.
pub fn location_of(root: LedgerRoot, rel: &str) -> String {
    match root {
        LedgerRoot::DataDir => rel.to_string(),
        LedgerRoot::AccountHome => format!("~/{rel}"),
    }
}

/// The ledger location as an error and a log name it: the relative path for a
/// [`LedgerRoot::DataDir`] node, `~/` plus the relative path for a
/// [`LedgerRoot::AccountHome`] node.
pub fn ledger_location(root: LedgerRoot, gateway: &str, mode: &str) -> String {
    location_of(root, &ledger_rel(gateway, mode))
}

/// The journal location as an error and a log name it (see
/// [`ledger_location`]).
pub fn journal_location(root: LedgerRoot, gateway: &str, mode: &str) -> String {
    location_of(root, &journal_rel(gateway, mode))
}

/// At most 64 printable characters of `text`, for a problem message.
fn shown(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(64).collect()
}

impl Ledger {
    /// Parse and validate `bytes` read from `location`, for the target
    /// `gateway` and `mode`. Every refusal is
    /// [`CoreError::DeployLedgerInvalid`] naming `location`.
    pub fn parse(
        location: &str,
        bytes: &[u8],
        gateway: &str,
        mode: &str,
    ) -> Result<Ledger, CoreError> {
        let bad = |problem: String| CoreError::DeployLedgerInvalid {
            location: location.to_string(),
            problem,
        };
        if bytes.is_empty() {
            return Err(bad("the file is empty".to_string()));
        }
        if bytes.len() > LEDGER_MAX_BYTES {
            return Err(bad(format!(
                "the file is {} bytes, over the {LEDGER_MAX_BYTES} byte cap",
                bytes.len()
            )));
        }
        // Strict UTF-8, no BOM, no repeated key, bounded depth.
        let (_, root) = node::classify(bytes).map_err(bad)?;
        let Node::Obj(members) = &root else {
            return Err(bad("the top level is not a JSON object".to_string()));
        };
        for (key, _) in members {
            if !FIELDS.contains(&key.as_str()) {
                return Err(bad(format!("unknown field `{}`", shown(key))));
            }
        }
        for field in FIELDS {
            if root.get(field).is_none() {
                return Err(bad(format!("missing field `{field}`")));
            }
        }
        let ledger: Ledger = serde_json::from_slice(bytes).map_err(|e| {
            bad(format!(
                "a field has the wrong type (line {} column {})",
                e.line(),
                e.column()
            ))
        })?;

        if ledger.schema_version != SCHEMA_VERSION {
            return Err(bad(format!(
                "schema_version is not {SCHEMA_VERSION}, the only version this build reads"
            )));
        }
        if ledger.gateway != gateway {
            return Err(bad(format!(
                "gateway names a different gateway than the target (`{}`)",
                shown(gateway)
            )));
        }
        if ledger.mode != mode {
            return Err(bad(format!(
                "mode names a different mode than the target (`{}`)",
                shown(mode)
            )));
        }
        let hex = ledger.render_sha256.len() == 64
            && ledger
                .render_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !hex {
            return Err(bad(
                "render_sha256 is not 64 lowercase hex digits".to_string()
            ));
        }
        let run_id_ok = (1..=RUN_ID_MAX).contains(&ledger.run_id.len())
            && ledger
                .run_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
        if !run_id_ok {
            return Err(bad(format!(
                "run_id is not 1 to {RUN_ID_MAX} characters of [A-Za-z0-9._-]"
            )));
        }
        check_files(&ledger.files).map_err(bad)?;
        Ok(ledger)
    }

    /// The canonical bytes: pretty JSON in field order plus one LF. The
    /// writer Phase 23 and the tests use; `parse(to_bytes(l)) == l`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = serde_json::to_vec_pretty(self).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }
}

/// Whether `module/ty` is a declared resource type.
fn declared_type(module: &str, ty: &str) -> bool {
    openapi_describes(module, ty) || (module == "ignition" && TAG_TYPES.contains(&ty))
}

/// The `files` rules (module doc); the problem text on refusal.
fn check_files(files: &[String]) -> Result<(), String> {
    let mut folded: BTreeSet<String> = BTreeSet::new();
    let mut previous: Option<&String> = None;
    for (index, path) in files.iter().enumerate() {
        let listed = shown(path);
        if let Err(why) = RelPath::new(path) {
            return Err(format!(
                "files[{index}] `{listed}` is not a safe path ({why})"
            ));
        }
        let parts: Vec<&str> = path.split('/').collect();
        if let Some(owned) = parts.iter().find(|p| GATEWAY_OWNED_FILES.contains(*p)) {
            return Err(format!(
                "files[{index}] `{listed}` lists the gateway-owned file `{owned}`, which is never deleted"
            ));
        }
        if parts.iter().any(|p| p.starts_with('.')) {
            return Err(format!(
                "files[{index}] `{listed}` has a dot-named component"
            ));
        }
        if parts.len() < 3 {
            return Err(format!(
                "files[{index}] `{listed}` has fewer than three components"
            ));
        }
        if !declared_type(parts[0], parts[1]) {
            return Err(format!(
                "files[{index}] `{listed}` is under type `{}/{}`, which is not a declared resource type",
                shown(parts[0]),
                shown(parts[1])
            ));
        }
        if let Some(prev) = previous {
            if path == prev {
                return Err(format!(
                    "files[{index}] `{listed}` is listed twice (duplicate)"
                ));
            }
            if path.as_bytes() < prev.as_bytes() {
                return Err(format!(
                    "files is not byte-sorted: `{listed}` follows `{}`",
                    shown(prev)
                ));
            }
        }
        if !folded.insert(path.to_ascii_lowercase()) {
            return Err(format!(
                "files[{index}] `{listed}` differs from another entry only by case, which a case-insensitive host treats as one file"
            ));
        }
        previous = Some(path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GW: &str = "tracer-gw";
    const MODE: &str = "dev";
    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn loc() -> String {
        ledger_location(LedgerRoot::DataDir, GW, MODE)
    }

    fn ledger(files: &[&str]) -> Ledger {
        Ledger {
            schema_version: 1,
            gateway: GW.to_string(),
            mode: MODE.to_string(),
            render_sha256: HASH.to_string(),
            run_id: "run-1.a_b".to_string(),
            files: files.iter().map(|f| (*f).to_string()).collect(),
        }
    }

    fn refused(bytes: &[u8]) -> String {
        let err = Ledger::parse(&loc(), bytes, GW, MODE).expect_err("must be refused");
        assert_eq!(err.code(), "deploy_ledger_invalid", "{err}");
        assert_eq!(err.exit_code(), 6);
        let text = err.to_string();
        assert!(text.contains(&loc()), "names the location: {text}");
        text
    }

    fn refused_ledger(l: &Ledger) -> String {
        refused(&l.to_bytes())
    }

    fn json_with(replace: (&str, &str)) -> Vec<u8> {
        let text =
            String::from_utf8(ledger(&["ignition/database-translator/A/config.json"]).to_bytes())
                .expect("utf8");
        assert!(text.contains(replace.0), "{text}");
        text.replace(replace.0, replace.1).into_bytes()
    }

    /// D-04, D-07: the journal, the snapshot and the run record sit below the
    /// ledger's directory, under the same root, never dot-named and never
    /// inside `config/resources/`.
    #[test]
    fn journal_snapshot_and_record_paths_are_pinned() {
        assert_eq!(journal_rel("gw", "prod"), "ign-deploy/gw/prod/journal.json");
        assert_eq!(
            snapshot_rel("gw", "prod", "R"),
            "ign-deploy/gw/prod/snapshots/R"
        );
        assert_eq!(
            record_rel("gw", "prod", "R"),
            "ign-deploy/gw/prod/snapshots/R/record.json"
        );
        for rel in [
            journal_rel("gw", "prod"),
            snapshot_rel("gw", "prod", "R"),
            record_rel("gw", "prod", "R"),
        ] {
            assert!(rel.starts_with("ign-deploy/gw/prod/"), "{rel}");
            assert!(!rel.contains("config/resources"), "{rel}");
            assert!(rel.split('/').all(|p| !p.starts_with('.')), "{rel}");
        }
        assert_eq!(
            journal_location(LedgerRoot::AccountHome, "gw", "prod"),
            "~/ign-deploy/gw/prod/journal.json"
        );
        assert_eq!(
            journal_location(LedgerRoot::DataDir, "gw", "prod"),
            "ign-deploy/gw/prod/journal.json"
        );
    }

    #[test]
    fn location_is_below_ign_deploy() {
        assert_eq!(loc(), "ign-deploy/tracer-gw/dev/ledger.json");
    }

    /// Decision B(i) (22-12): ssh nodes keep the ledger, the journal and the
    /// staging area in the service account's home; local nodes keep them in
    /// the data directory. The relative layout is the same.
    #[test]
    fn ledger_location_follows_decision_b() {
        use crate::deploy::target::{Local, NodeTarget, Ssh, TargetOs};
        assert_eq!(
            ledger_location(LedgerRoot::DataDir, GW, MODE),
            "ign-deploy/tracer-gw/dev/ledger.json"
        );
        assert_eq!(
            ledger_location(LedgerRoot::AccountHome, GW, MODE),
            "~/ign-deploy/tracer-gw/dev/ledger.json"
        );
        assert_eq!(ledger_rel(GW, MODE), "ign-deploy/tracer-gw/dev/ledger.json");
        assert_eq!(
            staging_rel(GW, MODE, "run-1"),
            "ign-deploy/tracer-gw/dev/staging/run-1"
        );
        // the staging area is below the same directory as the ledger and is
        // never inside config/resources or dot-named
        assert!(staging_rel(GW, MODE, "r").starts_with("ign-deploy/tracer-gw/dev/"));
        assert!(!ledger_rel(GW, MODE).contains("config/resources"));
        assert!(ledger_rel(GW, MODE).split('/').all(|p| !p.starts_with('.')));

        let ssh = NodeTarget {
            profile: "p".to_string(),
            ssh: Some(Ssh {
                host: "h".to_string(),
                user: "u".to_string(),
                port: None,
                key_env: None,
                key_file: None,
                os: TargetOs::Windows,
                data_dir: "C:\\ignition\\data".to_string(),
                known_hosts: "known_hosts".to_string(),
                connect_timeout_s: None,
                op_timeout_s: None,
            }),
            local: None,
        };
        let local = NodeTarget {
            profile: "p".to_string(),
            ssh: None,
            local: Some(Local {
                data_dir: "/data".to_string(),
            }),
        };
        assert_eq!(LedgerRoot::for_node(&ssh), LedgerRoot::AccountHome);
        assert_eq!(LedgerRoot::for_node(&local), LedgerRoot::DataDir);
    }

    #[test]
    fn parse_accepts_v1() {
        let l = ledger(&[
            "ignition/database-translator/A/config.json",
            "ignition/database-translator/A/resource.json",
            "ignition/tag-definition/P/tags.json",
        ]);
        let bytes = l.to_bytes();
        assert!(bytes.ends_with(b"\n"));
        let parsed = Ledger::parse(&loc(), &bytes, GW, MODE).expect("v1 parses");
        assert_eq!(parsed, l);
        assert_eq!(parsed.to_bytes(), bytes, "byte-identical round trip");
    }

    #[test]
    fn files_empty_is_valid() {
        let l = ledger(&[]);
        let parsed = Ledger::parse(&loc(), &l.to_bytes(), GW, MODE).expect("empty files is valid");
        assert!(parsed.files.is_empty());
    }

    #[test]
    fn refuses_empty_file() {
        assert!(refused(b"").contains("empty"));
    }

    #[test]
    fn refuses_over_the_cap() {
        let mut bytes = vec![b' '; LEDGER_MAX_BYTES + 1];
        bytes[0] = b'{';
        let text = refused(&bytes);
        assert!(text.contains("8388608"), "{text}");
    }

    #[test]
    fn refuses_invalid_utf8() {
        let mut bytes = ledger(&[]).to_bytes();
        bytes.insert(2, 0xff);
        assert!(refused(&bytes).contains("UTF-8"));
    }

    #[test]
    fn refuses_duplicate_key() {
        let bytes = json_with((
            "\"mode\": \"dev\"",
            "\"mode\": \"dev\",\n  \"mode\": \"dev\"",
        ));
        assert!(refused(&bytes).to_lowercase().contains("duplicate"));
    }

    #[test]
    fn refuses_unknown_field() {
        let bytes = json_with((
            "\"schema_version\": 1",
            "\"schema_version\": 1,\n  \"extra\": 1",
        ));
        let text = refused(&bytes);
        assert!(
            text.contains("unknown field") && text.contains("extra"),
            "{text}"
        );
    }

    #[test]
    fn refuses_a_missing_field() {
        let bytes = json_with(("\"run_id\": \"run-1.a_b\",", ""));
        assert!(refused(&bytes).contains("run_id"));
    }

    #[test]
    fn refuses_a_wrong_field_type_without_echoing_it() {
        let bytes = json_with((
            "\"schema_version\": 1",
            "\"schema_version\": \"leaky-value\"",
        ));
        let text = refused(&bytes);
        assert!(!text.contains("leaky-value"), "{text}");
    }

    #[test]
    fn refuses_a_non_object() {
        refused(b"[]\n");
    }

    #[test]
    fn refuses_another_schema_version() {
        let mut l = ledger(&[]);
        l.schema_version = 2;
        assert!(refused_ledger(&l).contains("schema_version"));
    }

    #[test]
    fn refuses_another_gateway_without_echoing_it() {
        let mut l = ledger(&[]);
        l.gateway = "other-gw".to_string();
        let text = refused_ledger(&l);
        assert!(text.contains("gateway"), "{text}");
        assert!(!text.contains("other-gw"), "{text}");
    }

    #[test]
    fn refuses_another_mode_without_echoing_it() {
        let mut l = ledger(&[]);
        l.mode = "prod".to_string();
        let text = refused_ledger(&l);
        assert!(text.contains("mode"), "{text}");
        assert!(!text.contains("prod"), "{text}");
    }

    #[test]
    fn refuses_a_bad_render_hash_and_run_id() {
        let mut l = ledger(&[]);
        l.render_sha256 = "ABC".to_string();
        assert!(refused_ledger(&l).contains("render_sha256"));
        let mut l = ledger(&[]);
        l.render_sha256 = HASH.to_uppercase();
        assert!(refused_ledger(&l).contains("render_sha256"));
        for bad in ["", "a b", "a/b", &"x".repeat(129)] {
            let mut l = ledger(&[]);
            l.run_id = bad.to_string();
            assert!(refused_ledger(&l).contains("run_id"), "{bad:?}");
        }
        let mut l = ledger(&[]);
        l.run_id = "x".repeat(128);
        Ledger::parse(&loc(), &l.to_bytes(), GW, MODE).expect("128 characters fit");
    }

    #[test]
    fn refuses_unsorted_files() {
        let l = ledger(&[
            "ignition/database-translator/B/config.json",
            "ignition/database-translator/A/config.json",
        ]);
        assert!(refused_ledger(&l).contains("sorted"));
    }

    #[test]
    fn refuses_duplicate_files() {
        let l = ledger(&[
            "ignition/database-translator/A/config.json",
            "ignition/database-translator/A/config.json",
        ]);
        assert!(refused_ledger(&l).contains("duplicate"));
    }

    #[test]
    fn refuses_case_variant_duplicates() {
        let l = ledger(&[
            "ignition/database-translator/X/config.json",
            "ignition/database-translator/x/config.json",
        ]);
        let text = refused_ledger(&l);
        assert!(text.contains("case"), "{text}");
    }

    #[test]
    fn refuses_config_mode_json() {
        let l = ledger(&["config-mode.json"]);
        assert!(refused_ledger(&l).contains("config-mode.json"));
        let l = ledger(&["ignition/database-translator/A/config-mode.json"]);
        assert!(refused_ledger(&l).contains("config-mode.json"));
    }

    #[test]
    fn refuses_dot_named_components() {
        for bad in [
            "ignition/database-translator/.hidden/config.json",
            "ignition/.git/config.json",
            ".x/database-translator/A/config.json",
        ] {
            let text = refused_ledger(&ledger(&[bad]));
            assert!(text.contains("dot"), "{bad}: {text}");
        }
    }

    #[test]
    fn refuses_fewer_than_three_components() {
        for bad in ["ignition", "ignition/database-translator"] {
            let text = refused_ledger(&ledger(&[bad]));
            assert!(text.contains("three"), "{bad}: {text}");
        }
    }

    #[test]
    fn refuses_unsafe_paths() {
        // sorted, so the path check (not the order check) is what fires
        for bad in ["../x/y/z", "ignition/database-translator//A", "a\\b/c/d"] {
            refused_ledger(&ledger(&[bad]));
        }
    }

    #[test]
    fn refuses_a_type_that_is_not_described() {
        let l = ledger(&["acme/widget/A/config.json"]);
        let text = refused_ledger(&l);
        assert!(text.contains("acme/widget/A/config.json"), "{text}");
        assert!(text.contains("type"), "{text}");
    }

    #[test]
    fn tag_types_are_declared_resource_paths() {
        let l = ledger(&[
            "ignition/tag-definition/P/tags.json",
            "ignition/tag-group/P/config.json",
            "ignition/tag-provider/P/config.json",
            "ignition/tag-type-definition/P/udts.json",
        ]);
        Ledger::parse(&loc(), &l.to_bytes(), GW, MODE).expect("tag types are described");
    }
}
