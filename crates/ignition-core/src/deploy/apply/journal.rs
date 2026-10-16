//! The on-box apply journal, format v1 (Phase 23, D-04, D-07, D-09).
//!
//! Written on the gateway host before the first file moves and removed when
//! the run is finalized; a journal that is still there means an apply was
//! interrupted. It is read by a later runner (`rollback --run`, recovery, the
//! `deploy plan` refusal), so the layout is one-way and strict, in the
//! manner of the declared-names ledger ([`crate::deploy::ledger::Ledger`]):
//! at most [`JOURNAL_MAX_BYTES`] (8 MiB), strict UTF-8, no duplicate key,
//! bounded depth, no unknown field, every field present.
//!
//! ```json
//! {
//!   "schema_version": 1,
//!   "run_id": "<[A-Za-z0-9._-]{1,128}>",
//!   "gateway": "<target gateway>",
//!   "mode": "<target mode>",
//!   "phase": "snapshotting | ready | writing | written | restoring",
//!   "pre_fingerprint": "<64 lowercase hex>",
//!   "touched": ["<collection path or ledger.json>", "..."]
//! }
//! ```
//!
//! `touched` is every path apply writes or deletes plus the ledger (key
//! `ledger.json`), byte-sorted and unique; each path is a collection path
//! (`mirror_path_problem`) or exactly `ledger.json`. A refusal is
//! `deploy_run_invalid` naming the location and the problem, never a file
//! value beyond a listed path, and never a fallback to a default (T-23-11).

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::fingerprint::LEDGER_KEY;
use super::run_store::RunId;
use crate::deploy::layout::RelPath;
use crate::deploy::ledger::LedgerRoot;
use crate::deploy::node::{self, Node};
use crate::deploy::target::NodeTarget;
use crate::deploy::transport::{TreeReader, mirror_path_problem};
use crate::error::CoreError;

pub use crate::deploy::ledger::journal_location;

/// File name of the journal.
pub const JOURNAL_FILE: &str = "journal.json";

/// The largest journal accepted (8 MiB), checked before any parse.
pub const JOURNAL_MAX_BYTES: usize = 8_388_608;

/// How far an interrupted apply got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalPhase {
    /// Pre-images are being copied; no live file has moved.
    Snapshotting,
    /// Every pre-image is copied and the staging is complete; no live file
    /// has moved.
    Ready,
    /// Live files are being moved.
    Writing,
    /// Every move is done; the run is not finalized.
    Written,
    /// A restore is under way.
    Restoring,
}

impl JournalPhase {
    /// Whether no live file can have moved yet (D-10): `snapshotting` and
    /// `ready`.
    pub fn before_first_move(self) -> bool {
        matches!(self, JournalPhase::Snapshotting | JournalPhase::Ready)
    }

    /// The phase's name as the journal file spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            JournalPhase::Snapshotting => "snapshotting",
            JournalPhase::Ready => "ready",
            JournalPhase::Writing => "writing",
            JournalPhase::Written => "written",
            JournalPhase::Restoring => "restoring",
        }
    }
}

/// The only schema version this build reads and writes.
const SCHEMA_VERSION: u32 = 1;

/// The journal's top-level keys, in write order.
const FIELDS: [&str; 7] = [
    "schema_version",
    "run_id",
    "gateway",
    "mode",
    "phase",
    "pre_fingerprint",
    "touched",
];

/// The journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Journal {
    /// Always 1.
    pub schema_version: u32,
    /// The run this journal belongs to.
    pub run_id: String,
    /// The gateway directory name (the target's `gateway`).
    pub gateway: String,
    /// The deployment mode (the target's `mode`).
    pub mode: String,
    /// How far the run got.
    pub phase: JournalPhase,
    /// The touched-set fingerprint before the run, 64 lowercase hex digits.
    pub pre_fingerprint: String,
    /// The touched paths, byte-sorted and unique.
    pub touched: Vec<String>,
}

impl Journal {
    /// Parse and validate `bytes` read from `location`, for the target
    /// `gateway` and `mode`.
    pub fn parse(
        location: &str,
        bytes: &[u8],
        gateway: &str,
        mode: &str,
    ) -> Result<Journal, CoreError> {
        let bad = |problem: String| refuse(location, bytes, problem);
        let journal: Journal = parse_strict(
            location,
            bytes,
            JOURNAL_MAX_BYTES,
            &FIELDS,
            &[(
                "phase",
                &["snapshotting", "ready", "writing", "written", "restoring"],
            )],
        )?
        .0;
        check_header(
            journal.schema_version,
            &journal.run_id,
            (&journal.gateway, gateway),
            (&journal.mode, mode),
        )
        .map_err(bad)?;
        if !is_hex64(&journal.pre_fingerprint) {
            return Err(bad(
                "pre_fingerprint is not 64 lowercase hex digits".to_string()
            ));
        }
        check_touched(journal.touched.iter().map(String::as_str)).map_err(bad)?;
        Ok(journal)
    }

    /// The canonical bytes: pretty JSON in field order plus one LF.
    pub fn to_bytes(&self) -> Vec<u8> {
        canonical_bytes(self)
    }
}

/// The journal on the box of `gateway` and `mode`, read through `reader` and
/// parsed strictly (DAPL-09, D-09). `None` means no interrupted run. A journal
/// that cannot be read or parsed is `deploy_run_invalid` naming the location:
/// plan, apply and rollback never go on as if it were absent.
pub async fn journal_on_box(
    reader: &dyn TreeReader,
    node: &NodeTarget,
    gateway: &str,
    mode: &str,
) -> Result<Option<Journal>, CoreError> {
    let Some(bytes) = reader.read_journal(gateway, mode).await? else {
        return Ok(None);
    };
    let location = journal_location(LedgerRoot::for_node(node), gateway, mode);
    Journal::parse(&location, &bytes, gateway, mode).map(Some)
}

/// The refusal for `journal`: `deploy_journal_present` naming its run, the
/// location and (through its message) `ign deploy rollback --run <id> --env
/// <env>`.
pub fn journal_present(
    journal: &Journal,
    env: &str,
    node: &NodeTarget,
    gateway: &str,
    mode: &str,
) -> CoreError {
    CoreError::DeployJournalPresent {
        run_id: journal.run_id.clone(),
        env: env.to_string(),
        location: journal_location(LedgerRoot::for_node(node), gateway, mode),
    }
}

/// The canonical bytes of `value`: pretty JSON plus one LF.
pub(super) fn canonical_bytes(value: &impl Serialize) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(value).unwrap_or_default();
    bytes.push(b'\n');
    bytes
}

/// Whether `text` is 64 lowercase hex digits.
pub(super) fn is_hex64(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// At most 64 printable characters of `text`, for a problem message.
pub(super) fn shown(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(64).collect()
}

/// The refusal for a journal or record at `location`: `deploy_run_invalid`
/// naming the location and the problem. The run id is the file's own when it
/// is a valid id (the rule limits it to a safe charset), else `unknown`.
pub(super) fn refuse(location: &str, bytes: &[u8], problem: String) -> CoreError {
    let run_id = serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|v| v.get("run_id")?.as_str().map(str::to_string))
        .filter(|id| RunId::parse(id).is_ok())
        .unwrap_or_else(|| "unknown".to_string());
    CoreError::DeployRunInvalid {
        run_id,
        problem: format!("{location}: {problem}"),
    }
}

/// The checks every format applies before it deserializes: not empty, within
/// `cap`, strict UTF-8 without a repeated key or excess depth, a JSON object,
/// no unknown key, every key of `fields` present, and each of `enums` one of
/// its allowed strings. Then the typed read. Returns the value and the
/// order-preserving document for per-element checks.
pub(super) fn parse_strict<T: DeserializeOwned>(
    location: &str,
    bytes: &[u8],
    cap: usize,
    fields: &[&str],
    enums: &[(&str, &[&str])],
) -> Result<(T, Node), CoreError> {
    let bad = |problem: String| refuse(location, bytes, problem);
    if bytes.is_empty() {
        return Err(bad("the file is empty".to_string()));
    }
    if bytes.len() > cap {
        return Err(bad(format!(
            "the file is {} bytes, over the {cap} byte cap",
            bytes.len()
        )));
    }
    let (_, root) = node::classify(bytes).map_err(bad)?;
    let Node::Obj(members) = &root else {
        return Err(bad("the top level is not a JSON object".to_string()));
    };
    for (key, _) in members {
        if !fields.contains(&key.as_str()) {
            return Err(bad(format!("unknown field `{}`", shown(key))));
        }
    }
    for field in fields {
        if root.get(field).is_none() {
            return Err(bad(format!("missing field `{field}`")));
        }
    }
    for (field, allowed) in enums {
        let ok = root
            .get(field)
            .and_then(Node::as_str)
            .is_some_and(|v| allowed.contains(&v));
        if !ok {
            return Err(bad(format!(
                "`{field}` is not one of {}",
                allowed.join(", ")
            )));
        }
    }
    let value: T = serde_json::from_slice(bytes).map_err(|e| {
        bad(format!(
            "a field has the wrong type (line {} column {})",
            e.line(),
            e.column()
        ))
    })?;
    Ok((value, root))
}

/// The header checks both formats share: the schema version, the run id and
/// that the file names this target's gateway and mode. The problem text never
/// echoes the foreign names.
pub(super) fn check_header(
    schema_version: u32,
    run_id: &str,
    gateway: (&str, &str),
    mode: (&str, &str),
) -> Result<(), String> {
    if schema_version != SCHEMA_VERSION {
        return Err(format!(
            "schema_version is not {SCHEMA_VERSION}, the only version this build reads"
        ));
    }
    if RunId::parse(run_id).is_err() {
        return Err(
            "run_id is not 1 to 128 characters of [A-Za-z0-9._-] without a leading dot".to_string(),
        );
    }
    if gateway.0 != gateway.1 {
        return Err("gateway names a different gateway than the target".to_string());
    }
    if mode.0 != mode.1 {
        return Err("mode names a different mode than the target".to_string());
    }
    Ok(())
}

/// The `touched` rules: each path is a collection path or exactly
/// `ledger.json`, and the list is byte-sorted and unique.
pub(super) fn check_touched<'a>(paths: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let mut previous: Option<&str> = None;
    for (index, path) in paths.enumerate() {
        let listed = shown(path);
        if path != LEDGER_KEY {
            let collection = RelPath::new(path)
                .ok()
                .is_some_and(|rel| mirror_path_problem(&rel).is_none());
            if !collection {
                return Err(format!(
                    "touched[{index}] `{listed}` is neither a collection path nor `{LEDGER_KEY}`"
                ));
            }
        }
        if let Some(prev) = previous {
            if path == prev {
                return Err(format!(
                    "touched[{index}] `{listed}` is listed twice (duplicate)"
                ));
            }
            if path.as_bytes() < prev.as_bytes() {
                return Err(format!(
                    "touched is not byte-sorted: `{listed}` follows `{}`",
                    shown(prev)
                ));
            }
        }
        previous = Some(path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const GW: &str = "tracer-gw";
    const MODE: &str = "dev";
    const LOC: &str = "~/ign-deploy/tracer-gw/dev/journal.json";
    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn good() -> Journal {
        Journal {
            schema_version: 1,
            run_id: "20261015T142233Z-a1b2c3".to_string(),
            gateway: GW.to_string(),
            mode: MODE.to_string(),
            phase: JournalPhase::Writing,
            pre_fingerprint: HASH.to_string(),
            touched: vec![
                "ledger.json".to_string(),
                "mod/type/a.json".to_string(),
                "mod/type/b.json".to_string(),
            ],
        }
    }

    fn with(edit: impl FnOnce(&mut Value)) -> Vec<u8> {
        let mut value: Value = serde_json::from_slice(&good().to_bytes()).expect("good is JSON");
        edit(&mut value);
        serde_json::to_vec(&value).expect("serialize")
    }

    fn refusal(bytes: &[u8]) -> String {
        let err = Journal::parse(LOC, bytes, GW, MODE).expect_err("must be refused");
        assert_eq!(err.code(), "deploy_run_invalid", "{err}");
        let text = err.to_string();
        assert!(text.contains(LOC), "the refusal names the location: {text}");
        text
    }

    #[test]
    fn journal_roundtrip() {
        let journal = good();
        let bytes = journal.to_bytes();
        assert!(bytes.ends_with(b"}\n"));
        let parsed = Journal::parse(LOC, &bytes, GW, MODE).expect("parses");
        assert_eq!(parsed, journal);
        assert_eq!(parsed.to_bytes(), bytes, "byte-identical");
        for phase in [
            JournalPhase::Snapshotting,
            JournalPhase::Ready,
            JournalPhase::Writing,
            JournalPhase::Written,
            JournalPhase::Restoring,
        ] {
            let j = Journal { phase, ..good() };
            assert_eq!(
                Journal::parse(LOC, &j.to_bytes(), GW, MODE).expect("parses"),
                j
            );
        }
        let empty = Journal {
            touched: vec![],
            ..good()
        };
        assert_eq!(
            Journal::parse(LOC, &empty.to_bytes(), GW, MODE).expect("parses"),
            empty
        );
    }

    #[test]
    fn the_layout_is_pinned() {
        let text = String::from_utf8(good().to_bytes()).expect("utf-8");
        let expected = format!(
            "{{\n  \"schema_version\": 1,\n  \"run_id\": \"20261015T142233Z-a1b2c3\",\n  \"gateway\": \"tracer-gw\",\n  \"mode\": \"dev\",\n  \"phase\": \"writing\",\n  \"pre_fingerprint\": \"{HASH}\",\n  \"touched\": [\n    \"ledger.json\",\n    \"mod/type/a.json\",\n    \"mod/type/b.json\"\n  ]\n}}\n"
        );
        assert_eq!(text, expected);
    }

    #[test]
    fn phases_before_the_first_move() {
        assert!(JournalPhase::Snapshotting.before_first_move());
        assert!(JournalPhase::Ready.before_first_move());
        assert!(!JournalPhase::Writing.before_first_move());
        assert!(!JournalPhase::Written.before_first_move());
        assert!(!JournalPhase::Restoring.before_first_move());
    }

    fn live(pairs: &[(&str, Option<&str>)]) -> std::collections::BTreeMap<String, Option<String>> {
        pairs
            .iter()
            .map(|(path, hash)| ((*path).to_string(), hash.map(str::to_string)))
            .collect()
    }

    #[test]
    fn recovery_action_clears_only_an_untouched_run() {
        use super::super::fingerprint::fingerprint;
        let pre = live(&[("mod/type/a.json", Some(HASH)), ("mod/type/b.json", None)]);
        let journal = |phase| Journal {
            phase,
            pre_fingerprint: fingerprint(&pre),
            touched: pre.keys().cloned().collect(),
            ..good()
        };
        let other = live(&[
            ("mod/type/a.json", Some(HASH)),
            ("mod/type/b.json", Some(HASH)),
        ]);
        for (phase, cleared) in [
            (JournalPhase::Snapshotting, true),
            (JournalPhase::Ready, true),
            (JournalPhase::Writing, false),
            (JournalPhase::Written, false),
            (JournalPhase::Restoring, false),
        ] {
            let j = journal(phase);
            let same = recovery_action(&j, &pre);
            let changed = recovery_action(&j, &other);
            assert_eq!(
                same,
                if cleared {
                    RecoveryAction::ClearWithoutRestore
                } else {
                    RecoveryAction::Restore
                },
                "{phase:?}, live equals the pre-run state"
            );
            assert_eq!(
                changed,
                RecoveryAction::Restore,
                "{phase:?}, live differs from the pre-run state"
            );
        }
    }

    #[test]
    fn journal_location_follows_the_ledger_root() {
        use crate::deploy::ledger::LedgerRoot;
        assert_eq!(
            journal_location(LedgerRoot::AccountHome, GW, MODE),
            "~/ign-deploy/tracer-gw/dev/journal.json"
        );
        assert_eq!(
            journal_location(LedgerRoot::DataDir, GW, MODE),
            "ign-deploy/tracer-gw/dev/journal.json"
        );
    }

    #[test]
    fn refuses_an_empty_file() {
        assert!(refusal(b"").contains("empty"));
    }

    #[test]
    fn refuses_a_file_over_the_cap() {
        let big = vec![b' '; JOURNAL_MAX_BYTES + 1];
        assert!(refusal(&big).contains("cap"));
    }

    #[test]
    fn refuses_invalid_utf8() {
        refusal(b"{\"schema_version\": 1, \"run_id\": \"\xff\"}");
    }

    #[test]
    fn refuses_a_duplicate_key() {
        let text = String::from_utf8(good().to_bytes()).expect("utf-8");
        let dup = text.replacen(
            "\"mode\": \"dev\"",
            "\"mode\": \"dev\",\n  \"mode\": \"dev\"",
            1,
        );
        refusal(dup.as_bytes());
    }

    #[test]
    fn refuses_an_unknown_field() {
        let text = refusal(&with(|v| {
            v["extra"] = json!(1);
        }));
        assert!(text.contains("extra"), "{text}");
    }

    #[test]
    fn refuses_a_missing_field() {
        let text = refusal(&with(|v| {
            v.as_object_mut().expect("object").remove("phase");
        }));
        assert!(text.contains("phase"), "{text}");
    }

    #[test]
    fn refuses_another_schema_version() {
        refusal(&with(|v| v["schema_version"] = json!(2)));
    }

    #[test]
    fn refuses_another_gateway() {
        refusal(&with(|v| v["gateway"] = json!("other-gw")));
    }

    #[test]
    fn refuses_another_mode() {
        refusal(&with(|v| v["mode"] = json!("prod")));
    }

    #[test]
    fn refuses_a_bad_run_id() {
        for bad in ["", "../x", ".hidden", "a b"] {
            refusal(&with(|v| v["run_id"] = json!(bad)));
        }
    }

    #[test]
    fn refuses_an_unknown_phase() {
        let text = refusal(&with(|v| v["phase"] = json!("halfway")));
        assert!(text.contains("phase"), "{text}");
    }

    #[test]
    fn refuses_unsorted_touched() {
        let text = refusal(&with(|v| {
            v["touched"] = json!(["mod/type/b.json", "mod/type/a.json"]);
        }));
        assert!(text.contains("sorted"), "{text}");
    }

    #[test]
    fn refuses_duplicate_touched() {
        let text = refusal(&with(|v| {
            v["touched"] = json!(["mod/type/a.json", "mod/type/a.json"]);
        }));
        assert!(text.contains("duplicate"), "{text}");
    }

    #[test]
    fn refuses_a_touched_path_that_is_not_a_collection_path() {
        for bad in [
            "a/b",
            "mod/type/.hidden/x",
            "mod/type/../x",
            "/abs/x/y",
            "mod/type/resource.json/../../z",
            "mod/type/config-mode.json",
            "ledger.json.bak",
        ] {
            refusal(&with(|v| v["touched"] = json!([bad])));
        }
    }

    #[test]
    fn refuses_a_pre_fingerprint_that_is_not_hex() {
        for bad in [
            "",
            "abc",
            &HASH.to_uppercase(),
            &format!("{HASH}0"),
            "g".repeat(64).as_str(),
        ] {
            refusal(&with(|v| v["pre_fingerprint"] = json!(bad)));
        }
    }

    #[test]
    fn refuses_a_wrong_typed_field() {
        refusal(&with(|v| v["touched"] = json!("mod/type/a.json")));
        refusal(&with(|v| v["schema_version"] = json!("1")));
    }

    #[test]
    fn refuses_a_top_level_that_is_not_an_object() {
        refusal(b"[]");
    }

    #[test]
    fn the_refusal_never_echoes_foreign_values() {
        let text = refusal(&with(|v| v["gateway"] = json!("secret-gateway-name")));
        assert!(!text.contains("secret-gateway-name"), "{text}");
    }
}
