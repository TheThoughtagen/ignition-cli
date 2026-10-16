//! The on-box run record, format v1 (Phase 23, D-04, D-07).
//!
//! One record per apply run, kept on the gateway host next to the snapshot
//! (`snapshots/<run_id>/record.json`). It is what `rollback --run` checks the
//! live files against, so it must be readable by any later runner: the
//! layout is one-way and strict (see [`super::journal`] for the rules the two
//! formats share: 8 MiB cap, strict UTF-8, no duplicate key, no unknown field,
//! every field present).
//!
//! ```json
//! {
//!   "schema_version": 1,
//!   "run_id": "<[A-Za-z0-9._-]{1,128}>",
//!   "gateway": "<target gateway>",
//!   "mode": "<target mode>",
//!   "render_sha256": "<64 lowercase hex>",
//!   "plan_sha256": "<64 lowercase hex>",
//!   "state": "applying | applied | rolled_back",
//!   "rolled_back_by": "<run id>" | null,
//!   "touched": [
//!     { "path": "<collection path or ledger.json>",
//!       "pre_sha256": "<64 hex>" | null, "post_sha256": "<64 hex>" | null }
//!   ],
//!   "pre_fingerprint": "<64 lowercase hex>",
//!   "post_fingerprint": "<64 lowercase hex>" | null
//! }
//! ```
//!
//! `null` hashes mean the path is absent before or after the run.
//! `touched` is byte-sorted by path and unique. `post_fingerprint` is null
//! while the state is `applying`; `rolled_back_by` is null unless the state
//! is `rolled_back`. [`RunRecord::pre_map`] and [`RunRecord::post_map`] give
//! the maps [`super::fingerprint::fingerprint`] takes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::journal::{
    canonical_bytes, check_header, check_touched, is_hex64, parse_strict, refuse,
};
use super::run_store::RunId;
use crate::deploy::node::Node;
use crate::error::CoreError;

/// File name of the run record.
pub const RECORD_FILE: &str = "record.json";

/// The largest record accepted (8 MiB), checked before any parse.
pub const RECORD_MAX_BYTES: usize = 8_388_608;

/// The record's top-level keys, in write order.
const FIELDS: [&str; 11] = [
    "schema_version",
    "run_id",
    "gateway",
    "mode",
    "render_sha256",
    "plan_sha256",
    "state",
    "rolled_back_by",
    "touched",
    "pre_fingerprint",
    "post_fingerprint",
];

/// The keys of one `touched` entry.
const ENTRY_FIELDS: [&str; 3] = ["path", "pre_sha256", "post_sha256"];

/// Where a run stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordState {
    /// Files are being written; the post fingerprint is not known yet.
    Applying,
    /// The run finished.
    Applied,
    /// A later rollback restored this run's pre-apply files.
    RolledBack,
}

/// One touched path with its hash before and after the run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TouchedEntry {
    /// A collection path, or `ledger.json`.
    pub path: String,
    /// SHA-256 before the run; `None` when the path did not exist.
    pub pre_sha256: Option<String>,
    /// SHA-256 after the run; `None` when the path does not exist.
    pub post_sha256: Option<String>,
}

/// The run record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRecord {
    /// Always 1.
    pub schema_version: u32,
    /// The run this record belongs to.
    pub run_id: String,
    /// The gateway directory name (the target's `gateway`).
    pub gateway: String,
    /// The deployment mode (the target's `mode`).
    pub mode: String,
    /// `render_sha256` of the render this run deployed.
    pub render_sha256: String,
    /// `plan_sha256` of the plan this run applied.
    pub plan_sha256: String,
    /// Where the run stands.
    pub state: RecordState,
    /// The rollback run that restored this one; only when `rolled_back`.
    pub rolled_back_by: Option<String>,
    /// The touched paths, byte-sorted and unique.
    pub touched: Vec<TouchedEntry>,
    /// The touched-set fingerprint before the run.
    pub pre_fingerprint: String,
    /// The touched-set fingerprint after the run; `None` while `applying`.
    pub post_fingerprint: Option<String>,
}

impl RunRecord {
    /// Parse and validate `bytes` read from `location`, for the target
    /// `gateway` and `mode`.
    pub fn parse(
        location: &str,
        bytes: &[u8],
        gateway: &str,
        mode: &str,
    ) -> Result<RunRecord, CoreError> {
        let bad = |problem: String| refuse(location, bytes, problem);
        let (record, root): (RunRecord, Node) = parse_strict(
            location,
            bytes,
            RECORD_MAX_BYTES,
            &FIELDS,
            &[("state", &["applying", "applied", "rolled_back"])],
        )?;
        // Each touched entry carries all three keys (a missing hash is an
        // explicit null) and no other.
        if let Some(Node::Arr(entries)) = root.get("touched") {
            for (index, entry) in entries.iter().enumerate() {
                let Node::Obj(members) = entry else {
                    return Err(bad(format!("touched[{index}] is not a JSON object")));
                };
                if let Some((key, _)) = members
                    .iter()
                    .find(|(k, _)| !ENTRY_FIELDS.contains(&k.as_str()))
                {
                    return Err(bad(format!(
                        "touched[{index}] has the unknown field `{}`",
                        super::journal::shown(key)
                    )));
                }
                if let Some(missing) = ENTRY_FIELDS.iter().find(|f| entry.get(f).is_none()) {
                    return Err(bad(format!("touched[{index}] is missing `{missing}`")));
                }
            }
        }
        check_header(
            record.schema_version,
            &record.run_id,
            (&record.gateway, gateway),
            (&record.mode, mode),
        )
        .map_err(bad)?;
        for (name, value) in [
            ("render_sha256", Some(&record.render_sha256)),
            ("plan_sha256", Some(&record.plan_sha256)),
            ("pre_fingerprint", Some(&record.pre_fingerprint)),
            ("post_fingerprint", record.post_fingerprint.as_ref()),
        ] {
            if value.is_some_and(|v| !is_hex64(v)) {
                return Err(bad(format!("{name} is not 64 lowercase hex digits")));
            }
        }
        if record.state == RecordState::Applying && record.post_fingerprint.is_some() {
            return Err(bad(
                "post_fingerprint is set while state is applying".to_string()
            ));
        }
        if let Some(by) = &record.rolled_back_by {
            if record.state != RecordState::RolledBack {
                return Err(bad(
                    "rolled_back_by is set while state is not rolled_back".to_string()
                ));
            }
            if RunId::parse(by).is_err() {
                return Err(bad("rolled_back_by is not a valid run id".to_string()));
            }
        }
        check_touched(record.touched.iter().map(|e| e.path.as_str())).map_err(bad)?;
        for (index, entry) in record.touched.iter().enumerate() {
            for (name, value) in [
                ("pre_sha256", &entry.pre_sha256),
                ("post_sha256", &entry.post_sha256),
            ] {
                if value.as_deref().is_some_and(|v| !is_hex64(v)) {
                    return Err(bad(format!(
                        "touched[{index}].{name} is not 64 lowercase hex digits"
                    )));
                }
            }
        }
        Ok(record)
    }

    /// The canonical bytes: pretty JSON in field order plus one LF.
    pub fn to_bytes(&self) -> Vec<u8> {
        canonical_bytes(self)
    }

    /// Path to pre-run hash, for the fingerprint.
    pub fn pre_map(&self) -> BTreeMap<String, Option<String>> {
        self.touched
            .iter()
            .map(|e| (e.path.clone(), e.pre_sha256.clone()))
            .collect()
    }

    /// Path to post-run hash, for the fingerprint.
    pub fn post_map(&self) -> BTreeMap<String, Option<String>> {
        self.touched
            .iter()
            .map(|e| (e.path.clone(), e.post_sha256.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deploy::apply::fingerprint::{LEDGER_KEY, fingerprint};
    use serde_json::{Value, json};

    const GW: &str = "tracer-gw";
    const MODE: &str = "dev";
    const LOC: &str = "~/ign-deploy/tracer-gw/dev/snapshots/20261015T142233Z-a1b2c3/record.json";
    const H1: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const H2: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

    fn entry(path: &str, pre: Option<&str>, post: Option<&str>) -> TouchedEntry {
        TouchedEntry {
            path: path.to_string(),
            pre_sha256: pre.map(str::to_string),
            post_sha256: post.map(str::to_string),
        }
    }

    fn good() -> RunRecord {
        RunRecord {
            schema_version: 1,
            run_id: "20261015T142233Z-a1b2c3".to_string(),
            gateway: GW.to_string(),
            mode: MODE.to_string(),
            render_sha256: H1.to_string(),
            plan_sha256: H2.to_string(),
            state: RecordState::Applied,
            rolled_back_by: None,
            touched: vec![
                entry("ledger.json", None, Some(H1)),
                entry("mod/type/a.json", Some(H1), Some(H2)),
                entry("mod/type/b.json", Some(H2), None),
            ],
            pre_fingerprint: H1.to_string(),
            post_fingerprint: Some(H2.to_string()),
        }
    }

    fn with(edit: impl FnOnce(&mut Value)) -> Vec<u8> {
        let mut value: Value = serde_json::from_slice(&good().to_bytes()).expect("good is JSON");
        edit(&mut value);
        serde_json::to_vec(&value).expect("serialize")
    }

    fn refusal(bytes: &[u8]) -> String {
        let err = RunRecord::parse(LOC, bytes, GW, MODE).expect_err("must be refused");
        assert_eq!(err.code(), "deploy_run_invalid", "{err}");
        let text = err.to_string();
        assert!(text.contains(LOC), "the refusal names the location: {text}");
        text
    }

    #[test]
    fn record_roundtrip() {
        let record = good();
        let bytes = record.to_bytes();
        assert!(bytes.ends_with(b"}\n"));
        let parsed = RunRecord::parse(LOC, &bytes, GW, MODE).expect("parses");
        assert_eq!(parsed, record);
        assert_eq!(parsed.to_bytes(), bytes, "byte-identical");

        let applying = RunRecord {
            state: RecordState::Applying,
            post_fingerprint: None,
            ..good()
        };
        assert_eq!(
            RunRecord::parse(LOC, &applying.to_bytes(), GW, MODE).expect("parses"),
            applying
        );
        let rolled = RunRecord {
            state: RecordState::RolledBack,
            rolled_back_by: Some("20261016T090000Z-ffffff".to_string()),
            ..good()
        };
        assert_eq!(
            RunRecord::parse(LOC, &rolled.to_bytes(), GW, MODE).expect("parses"),
            rolled
        );
    }

    #[test]
    fn the_layout_is_pinned() {
        let applying = RunRecord {
            state: RecordState::Applying,
            post_fingerprint: None,
            touched: vec![entry("mod/type/a.json", None, Some(H2))],
            ..good()
        };
        let text = String::from_utf8(applying.to_bytes()).expect("utf-8");
        let expected = format!(
            "{{\n  \"schema_version\": 1,\n  \"run_id\": \"20261015T142233Z-a1b2c3\",\n  \"gateway\": \"tracer-gw\",\n  \"mode\": \"dev\",\n  \"render_sha256\": \"{H1}\",\n  \"plan_sha256\": \"{H2}\",\n  \"state\": \"applying\",\n  \"rolled_back_by\": null,\n  \"touched\": [\n    {{\n      \"path\": \"mod/type/a.json\",\n      \"pre_sha256\": null,\n      \"post_sha256\": \"{H2}\"\n    }}\n  ],\n  \"pre_fingerprint\": \"{H1}\",\n  \"post_fingerprint\": null\n}}\n"
        );
        assert_eq!(text, expected);
    }

    #[test]
    fn the_maps_feed_the_fingerprint() {
        let record = good();
        let pre = record.pre_map();
        let post = record.post_map();
        assert_eq!(pre.len(), 3);
        assert_eq!(pre.get(LEDGER_KEY), Some(&None));
        assert_eq!(pre.get("mod/type/a.json"), Some(&Some(H1.to_string())));
        assert_eq!(post.get("mod/type/b.json"), Some(&None));
        assert_eq!(post.get(LEDGER_KEY), Some(&Some(H1.to_string())));
        assert_ne!(fingerprint(&pre), fingerprint(&post));
    }

    #[test]
    fn refuses_an_empty_file() {
        assert!(refusal(b"").contains("empty"));
    }

    #[test]
    fn refuses_a_file_over_the_cap() {
        assert!(refusal(&vec![b' '; RECORD_MAX_BYTES + 1]).contains("cap"));
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
    fn refuses_an_unknown_field_at_either_level() {
        let top = refusal(&with(|v| v["extra"] = json!(1)));
        assert!(top.contains("extra"), "{top}");
        refusal(&with(|v| v["touched"][0]["extra"] = json!(1)));
    }

    #[test]
    fn refuses_a_missing_field_at_either_level() {
        let top = refusal(&with(|v| {
            v.as_object_mut().expect("object").remove("plan_sha256");
        }));
        assert!(top.contains("plan_sha256"), "{top}");
        refusal(&with(|v| {
            v["touched"][1]
                .as_object_mut()
                .expect("object")
                .remove("post_sha256");
        }));
        refusal(&with(|v| {
            v.as_object_mut()
                .expect("object")
                .remove("post_fingerprint");
        }));
    }

    #[test]
    fn refuses_another_schema_version_gateway_or_mode() {
        refusal(&with(|v| v["schema_version"] = json!(2)));
        refusal(&with(|v| v["gateway"] = json!("other-gw")));
        refusal(&with(|v| v["mode"] = json!("prod")));
    }

    #[test]
    fn refuses_a_bad_run_id() {
        for bad in ["", "../x", ".hidden", "a b"] {
            refusal(&with(|v| v["run_id"] = json!(bad)));
        }
    }

    #[test]
    fn refuses_an_unknown_state() {
        let text = refusal(&with(|v| v["state"] = json!("halfway")));
        assert!(text.contains("state"), "{text}");
    }

    #[test]
    fn refuses_unsorted_or_duplicate_touched() {
        let text = refusal(&with(|v| {
            let t = v["touched"].as_array_mut().expect("array");
            t.swap(1, 2);
        }));
        assert!(text.contains("sorted"), "{text}");
        let text = refusal(&with(|v| {
            let first = v["touched"][1].clone();
            v["touched"][2] = first;
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
            "ledger.json.bak",
        ] {
            refusal(&with(|v| {
                v["touched"] = json!([{"path": bad, "pre_sha256": null, "post_sha256": null}])
            }));
        }
    }

    #[test]
    fn refuses_a_hash_that_is_not_hex() {
        for bad in ["", "abc", &H1.to_uppercase(), &format!("{H1}0")] {
            refusal(&with(|v| v["render_sha256"] = json!(bad)));
            refusal(&with(|v| v["plan_sha256"] = json!(bad)));
            refusal(&with(|v| v["pre_fingerprint"] = json!(bad)));
            refusal(&with(|v| v["post_fingerprint"] = json!(bad)));
            refusal(&with(|v| v["touched"][1]["pre_sha256"] = json!(bad)));
            refusal(&with(|v| v["touched"][1]["post_sha256"] = json!(bad)));
        }
    }

    #[test]
    fn refuses_a_post_fingerprint_while_applying() {
        let text = refusal(&with(|v| v["state"] = json!("applying")));
        assert!(text.contains("post_fingerprint"), "{text}");
    }

    #[test]
    fn refuses_rolled_back_by_unless_rolled_back() {
        for state in ["applying", "applied"] {
            let text = refusal(&with(|v| {
                v["state"] = json!(state);
                v["post_fingerprint"] = Value::Null;
                v["rolled_back_by"] = json!("20261016T090000Z-ffffff");
            }));
            assert!(text.contains("rolled_back_by"), "{state}: {text}");
        }
    }

    #[test]
    fn refuses_a_bad_rolled_back_by() {
        refusal(&with(|v| {
            v["state"] = json!("rolled_back");
            v["rolled_back_by"] = json!("../x");
        }));
    }

    #[test]
    fn refuses_a_wrong_typed_field() {
        refusal(&with(|v| v["touched"] = json!("mod/type/a.json")));
        refusal(&with(|v| v["touched"][1]["path"] = json!(7)));
    }

    #[test]
    fn refuses_a_top_level_that_is_not_an_object() {
        refusal(b"[]");
    }
}
