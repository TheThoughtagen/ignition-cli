//! The on-host tag record (26-06, TAGDEP-09, D-02, D-05).
//!
//! One record per gateway and owned provider, at
//! [`tag_record_rel`](super::ledger::tag_record_rel)
//! (`ign-deploy/<gateway>/tag-providers/<provider>/record.json`) below the
//! node's ledger root, beside the Phase 25 release records. It names the
//! project that deployed the provider and every node path and tag group that
//! project's deploys placed, so:
//!
//! - a later deploy deletes only what the record lists (D-02), and
//! - a second project claiming the provider is seen with one read (D-05).
//!
//! The record decides what may be deleted, so it is parsed as strictly as the
//! ledger and the release record: an unknown or missing field, another
//! gateway or provider, another schema, an unsorted, duplicated or unsafe
//! path, more than [`TAG_RECORD_MAX_BYTES`], invalid UTF-8 or a repeated key
//! is `deploy_run_invalid` naming the location. A record that exists is never
//! read as absent. The problem text names fields and list positions and never
//! echoes a value the file holds.

use serde::{Deserialize, Serialize};

use super::apply::run_store::RunId;
use super::component_problem;
use super::node::{self, Node};
use super::projects::Release;
use crate::error::CoreError;

/// File name of a tag record.
pub const TAG_RECORD_FILE: &str = "record.json";

/// The largest tag record accepted (8 MiB, the ledger cap), checked before any
/// parse. Production scale is about 3,500 file nodes plus their folders.
pub const TAG_RECORD_MAX_BYTES: usize = 8_388_608;

/// The only schema version this build reads and writes.
const TAG_RECORD_SCHEMA_VERSION: u32 = 1;

/// The record's keys, in write order.
const TAG_RECORD_FIELDS: [&str; 10] = [
    "schema_version",
    "gateway",
    "provider",
    "project",
    "release",
    "run_id",
    "deployed_at",
    "tags_sha256",
    "nodes",
    "groups",
];

/// What the last `ign` deploy of one owned provider on one gateway placed
/// there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TagRecord {
    /// Always 1.
    pub schema_version: u32,
    /// The gateway directory name the record belongs to (the target's
    /// `gateway`).
    pub gateway: String,
    /// The owned tag provider, exactly as the record path names it.
    pub provider: String,
    /// The gateway project whose deploys placed these nodes.
    pub project: String,
    /// The release tag of the run that wrote the record.
    pub release: String,
    /// The run that wrote the record: `[A-Za-z0-9._-]{1,128}`.
    pub run_id: String,
    /// When the record was written, UTC (informational; never compared).
    pub deployed_at: String,
    /// The reviewed owned tag content hash of that run: 64 lowercase hex.
    pub tags_sha256: String,
    /// Provider-relative native paths (`/`-joined decoded tag names) of the
    /// file nodes and folders the project's deploys placed; types sit under
    /// `_types_/`. Byte-sorted and unique.
    pub nodes: Vec<String>,
    /// The tag group names the project's deploys placed. Byte-sorted and
    /// unique.
    pub groups: Vec<String>,
}

/// At most 64 printable characters of `text`, for a problem message.
fn shown(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(64).collect()
}

/// Why `path` is not a provider-relative node path, or `None`. Every
/// `/`-separated segment must be one safe tag name: not empty, not `.` or
/// `..`, no `\` or NUL, not dot-leading and no control character. Never
/// quotes the path.
pub(crate) fn node_path_problem(path: &str) -> Option<String> {
    if path.is_empty() {
        return Some("is empty".to_string());
    }
    for (n, segment) in path.split('/').enumerate() {
        if segment.chars().any(char::is_control) {
            return Some(format!("segment {n} holds a control character"));
        }
        if let Some(why) = component_problem(segment) {
            return Some(format!("segment {n} {why}"));
        }
    }
    None
}

/// Why `list` is not byte-sorted and unique, as the position of the first
/// entry out of order, or `None`.
pub(crate) fn order_problem(list: &[String]) -> Option<usize> {
    list.windows(2)
        .position(|pair| pair[0].as_bytes() >= pair[1].as_bytes())
        .map(|i| i + 1)
}

impl TagRecord {
    /// Parse and validate `bytes` read from `location`, for `gateway` and
    /// `provider`, both compared exactly (a provider equal only under case
    /// folding is another record). Every refusal is
    /// [`CoreError::DeployRunInvalid`] naming `location`.
    pub fn parse(
        bytes: &[u8],
        location: &str,
        gateway: &str,
        provider: &str,
    ) -> Result<TagRecord, CoreError> {
        let bad = |problem: String| CoreError::DeployRunInvalid {
            run_id: "unknown".to_string(),
            problem: format!("{location}: {problem}"),
        };
        if bytes.is_empty() {
            return Err(bad("the file is empty".to_string()));
        }
        if bytes.len() > TAG_RECORD_MAX_BYTES {
            return Err(bad(format!(
                "the file is {} bytes, over the {TAG_RECORD_MAX_BYTES} byte cap",
                bytes.len()
            )));
        }
        // Strict UTF-8, no BOM, no repeated key, bounded depth.
        let (_, root) = node::classify(bytes).map_err(bad)?;
        let Node::Obj(members) = &root else {
            return Err(bad("the top level is not a JSON object".to_string()));
        };
        for (key, _) in members {
            if !TAG_RECORD_FIELDS.contains(&key.as_str()) {
                return Err(bad(format!("unknown field `{}`", shown(key))));
            }
        }
        for field in TAG_RECORD_FIELDS {
            if root.get(field).is_none() {
                return Err(bad(format!("missing field `{field}`")));
            }
        }
        let record: TagRecord = serde_json::from_slice(bytes).map_err(|e| {
            bad(format!(
                "a field has the wrong type (line {} column {})",
                e.line(),
                e.column()
            ))
        })?;

        if record.schema_version != TAG_RECORD_SCHEMA_VERSION {
            return Err(bad(format!(
                "schema_version is not {TAG_RECORD_SCHEMA_VERSION}, the only version this build reads"
            )));
        }
        if record.gateway != gateway {
            return Err(bad(format!(
                "gateway names a different gateway than the target (`{}`)",
                shown(gateway)
            )));
        }
        if record.provider != provider {
            return Err(bad(format!(
                "provider names a different tag provider than this record path (`{}`)",
                shown(provider)
            )));
        }
        if record.project.chars().any(char::is_control)
            || component_problem(&record.project).is_some()
        {
            return Err(bad(
                "project is not one safe gateway project name".to_string()
            ));
        }
        if Release::try_parse(&record.release).is_none() {
            return Err(bad("release is not a semver tag".to_string()));
        }
        if RunId::parse(&record.run_id).is_err() {
            return Err(bad(
                "run_id is not 1 to 128 characters of [A-Za-z0-9._-]".to_string()
            ));
        }
        if record.deployed_at.is_empty()
            || record.deployed_at.len() > 64
            || !record.deployed_at.bytes().all(|b| b.is_ascii_graphic())
        {
            return Err(bad(
                "deployed_at is not 1 to 64 printable ASCII characters".to_string()
            ));
        }
        let hex = record.tags_sha256.len() == 64
            && record
                .tags_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !hex {
            return Err(bad("tags_sha256 is not 64 lowercase hex digits".to_string()));
        }
        for (i, path) in record.nodes.iter().enumerate() {
            if let Some(why) = node_path_problem(path) {
                return Err(bad(format!(
                    "nodes[{i}] is not a provider-relative node path: it {why}"
                )));
            }
        }
        if let Some(i) = order_problem(&record.nodes) {
            return Err(bad(format!(
                "nodes[{i}] is out of byte order or repeats an entry; the list must be byte-sorted and unique"
            )));
        }
        for (i, group) in record.groups.iter().enumerate() {
            if group.is_empty() || group.chars().any(char::is_control) {
                return Err(bad(format!(
                    "groups[{i}] is empty or holds a control character"
                )));
            }
        }
        if let Some(i) = order_problem(&record.groups) {
            return Err(bad(format!(
                "groups[{i}] is out of byte order or repeats an entry; the list must be byte-sorted and unique"
            )));
        }
        Ok(record)
    }

    /// The canonical bytes: pretty JSON in field order, both lists
    /// byte-sorted and deduplicated, plus one LF. For any record `parse`
    /// accepted, `parse(to_bytes(r)) == r`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut canonical = self.clone();
        for list in [&mut canonical.nodes, &mut canonical.groups] {
            list.sort_unstable_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            list.dedup();
        }
        let mut bytes = serde_json::to_vec_pretty(&canonical).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CoreError;

    const LOC: &str = "~/ign-deploy/gw-a/tag-providers/PlantA/record.json";
    const GW: &str = "gw-a";
    const P: &str = "PlantA";

    fn record() -> TagRecord {
        TagRecord {
            schema_version: 1,
            gateway: GW.to_string(),
            provider: P.to_string(),
            project: "CapApp".to_string(),
            release: "v1.2.0".to_string(),
            run_id: "20261108T101500Z-ab12".to_string(),
            deployed_at: "2026-11-08T10:15:00Z".to_string(),
            tags_sha256: "0123456789abcdef".repeat(4),
            nodes: vec![
                "Area1".to_string(),
                "Area1/Motor 1".to_string(),
                "Area1/Pump".to_string(),
                "_types_".to_string(),
                "_types_/Motor".to_string(),
            ],
            groups: vec!["Default".to_string(), "Fast".to_string()],
        }
    }

    fn parse(bytes: &[u8]) -> Result<TagRecord, CoreError> {
        TagRecord::parse(bytes, LOC, GW, P)
    }

    /// The refusal of `bytes`: `deploy_run_invalid`, exit 6, naming the
    /// record location. Returns the message.
    fn refused(bytes: &[u8]) -> String {
        let err = parse(bytes).expect_err("must be refused");
        assert_eq!(err.code(), "deploy_run_invalid", "{err}");
        assert_eq!(err.exit_code(), 6, "{err}");
        let text = err.to_string();
        assert!(text.contains(LOC), "names the location: {text}");
        text
    }

    fn refused_record(r: &TagRecord) -> String {
        // Serialized as is, so an unsorted list reaches the parser unsorted.
        let mut bytes = serde_json::to_vec_pretty(r).expect("json");
        bytes.push(b'\n');
        refused(&bytes)
    }

    fn json_with(from: &str, to: &str) -> Vec<u8> {
        let text = String::from_utf8(record().to_bytes()).expect("utf-8");
        assert!(text.contains(from), "{from} in {text}");
        text.replacen(from, to, 1).into_bytes()
    }

    #[test]
    fn tag_record_round_trips() {
        let r = record();
        assert_eq!(parse(&r.to_bytes()).expect("parses"), r);
        // The canonical form is stable: pretty JSON in field order plus LF.
        let text = String::from_utf8(r.to_bytes()).expect("utf-8");
        assert!(text.ends_with("}\n"), "{text}");
        let order: Vec<usize> = TAG_RECORD_FIELDS
            .iter()
            .map(|f| text.find(&format!("\"{f}\"")).expect(f))
            .collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{text}");
        assert_eq!(TAG_RECORD_FILE, "record.json");
        assert_eq!(TAG_RECORD_MAX_BYTES, 8_388_608);
    }

    #[test]
    fn tag_record_to_bytes_sorts_and_dedups_its_lists() {
        let mut r = record();
        r.nodes = vec!["b".into(), "a".into(), "b".into(), "_types_".into()];
        r.groups = vec!["Slow".into(), "Default".into()];
        let parsed = parse(&r.to_bytes()).expect("parses");
        assert_eq!(parsed.nodes, ["_types_", "a", "b"]);
        assert_eq!(parsed.groups, ["Default", "Slow"]);
    }

    #[test]
    fn tag_record_empty_lists_are_valid() {
        let mut r = record();
        r.nodes.clear();
        r.groups.clear();
        assert_eq!(parse(&r.to_bytes()).expect("parses"), r);
    }

    #[test]
    fn tag_record_unknown_field_is_refused() {
        let text = refused(&json_with(
            "\"schema_version\": 1,",
            "\"schema_version\": 1,\n  \"extra\": true,",
        ));
        assert!(text.contains("unknown field `extra`"), "{text}");
    }

    #[test]
    fn tag_record_missing_field_is_refused() {
        for field in TAG_RECORD_FIELDS {
            let mut value: serde_json::Value =
                serde_json::from_slice(&record().to_bytes()).expect("json");
            value.as_object_mut().expect("object").remove(field);
            let text = refused(&serde_json::to_vec(&value).expect("json"));
            assert!(text.contains(&format!("missing field `{field}`")), "{text}");
        }
    }

    #[test]
    fn tag_record_wrong_schema_version_is_refused() {
        let text = refused(&json_with("\"schema_version\": 1", "\"schema_version\": 2"));
        assert!(text.contains("schema_version"), "{text}");
    }

    #[test]
    fn tag_record_wrong_type_is_refused() {
        let text = refused(&json_with(
            "\"schema_version\": 1",
            "\"schema_version\": \"1\"",
        ));
        assert!(text.contains("wrong type"), "{text}");
    }

    #[test]
    fn tag_record_other_gateway_is_refused() {
        let mut r = record();
        r.gateway = "gw-b".to_string();
        let text = refused(&r.to_bytes());
        assert!(text.contains("gateway"), "{text}");
        // the foreign name the file holds is never echoed
        assert!(!text.contains("gw-b"), "{text}");
    }

    #[test]
    fn tag_record_other_provider_is_refused_even_when_fold_equal() {
        for other in ["PlantB", "planta", "PLANTA", "PlantA "] {
            let mut r = record();
            r.provider = other.to_string();
            let text = refused(&r.to_bytes());
            assert!(text.contains("provider"), "{other}: {text}");
        }
    }

    #[test]
    fn tag_record_unsorted_or_duplicate_nodes_are_refused() {
        let mut r = record();
        r.nodes = vec!["b".into(), "a".into()];
        assert!(refused_record(&r).contains("nodes"));
        r.nodes = vec!["a".into(), "a".into()];
        assert!(refused_record(&r).contains("nodes"));
        // byte order, not case-folded order: `B` sorts before `a`
        r.nodes = vec!["a".into(), "B".into()];
        assert!(refused_record(&r).contains("nodes"));
        r.nodes = vec!["B".into(), "a".into()];
        let mut bytes = serde_json::to_vec_pretty(&r).expect("json");
        bytes.push(b'\n');
        assert!(parse(&bytes).is_ok());
    }

    #[test]
    fn tag_record_unsorted_or_duplicate_groups_are_refused() {
        let mut r = record();
        r.groups = vec!["Fast".into(), "Default".into()];
        assert!(refused_record(&r).contains("groups"));
        r.groups = vec!["Fast".into(), "Fast".into()];
        assert!(refused_record(&r).contains("groups"));
        for bad in ["", "Bad\ngroup"] {
            r.groups = vec![bad.to_string()];
            assert!(refused_record(&r).contains("groups"), "{bad:?}");
        }
    }

    #[test]
    fn tag_record_bad_node_paths_are_refused() {
        for bad in [
            "",
            "/Area1",
            "Area1/",
            "Area1//Pump",
            "..",
            "Area1/../Pump",
            ".",
            "Area1/./Pump",
            "Area1\\Pump",
            ".hidden",
            "Area1/Pu\0mp",
            "Area1/Pu\u{7}mp",
        ] {
            let mut r = record();
            r.nodes = vec![bad.to_string()];
            let text = refused_record(&r);
            assert!(text.contains("nodes[0]"), "{bad:?}: {text}");
            // the path itself is never echoed
            if !bad.is_empty() && bad != "." && bad != ".." {
                assert!(!text.contains(bad), "{bad:?}: {text}");
            }
        }
    }

    #[test]
    fn tag_record_bad_scalars_are_refused() {
        let cases: [(&str, &str, &str); 7] = [
            ("tags_sha256", "\"0123456789abcdef", "\"0123456789ABCDEF"),
            ("release", "\"v1.2.0\"", "\"latest\""),
            ("run_id", "\"20261108T101500Z-ab12\"", "\"../run\""),
            ("deployed_at", "\"2026-11-08T10:15:00Z\"", "\"\""),
            ("project", "\"CapApp\"", "\"\""),
            ("project", "\"CapApp\"", "\"../x\""),
            ("project", "\"CapApp\"", "\"a/b\""),
        ];
        for (field, from, to) in cases {
            let text = refused(&json_with(from, to));
            assert!(text.contains(field), "{field}: {text}");
        }
    }

    #[test]
    fn tag_record_over_the_cap_is_refused_before_any_parse() {
        let mut bytes = record().to_bytes();
        bytes.resize(TAG_RECORD_MAX_BYTES + 1, b' ');
        let text = refused(&bytes);
        assert!(text.contains("byte cap"), "{text}");
        // a record exactly at the cap is read
        let mut bytes = record().to_bytes();
        bytes.resize(TAG_RECORD_MAX_BYTES, b' ');
        assert!(parse(&bytes).is_ok());
    }

    #[test]
    fn tag_record_invalid_utf8_empty_and_non_object_are_refused() {
        let mut bytes = record().to_bytes();
        let at = bytes.iter().position(|b| *b == b'C').expect("a C");
        bytes[at] = 0xff;
        refused(&bytes);
        assert!(refused(b"").contains("empty"));
        refused(b"[]");
        refused(b"not json");
        // a repeated key is refused, never last-wins
        refused(&json_with(
            "\"gateway\": \"gw-a\",",
            "\"gateway\": \"gw-a\",\n  \"gateway\": \"gw-a\",",
        ));
    }
}
