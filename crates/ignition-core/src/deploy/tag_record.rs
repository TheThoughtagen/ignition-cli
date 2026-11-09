//! The on-host tag record (26-06, TAGDEP-09, D-02, D-05).

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
        let text = refused(&json_with("\"schema_version\": 1", "\"schema_version\": \"1\""));
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
