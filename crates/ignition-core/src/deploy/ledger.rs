//! The declared-names ledger, format v1 (Phase 22, D-05, D-07).
//!
//! RED-phase skeleton: the reader is implemented in the next commit.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// File name of the ledger.
pub const LEDGER_FILE: &str = "ledger.json";

/// The largest ledger accepted (8 MiB).
pub const LEDGER_MAX_BYTES: usize = 8_388_608;

/// The declared-names ledger.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ledger {
    /// Always 1.
    pub schema_version: u32,
    /// Gateway the ledger belongs to.
    pub gateway: String,
    /// Deployment mode the ledger belongs to.
    pub mode: String,
    /// Render hash of the apply that wrote it.
    pub render_sha256: String,
    /// Run identifier of the apply that wrote it.
    pub run_id: String,
    /// Mode-collection paths the apply wrote, byte-sorted and unique.
    pub files: Vec<String>,
}

/// The ledger location relative to the node's data directory.
pub fn ledger_location(gateway: &str, mode: &str) -> String {
    format!("ign-deploy/{gateway}/{mode}/{LEDGER_FILE}")
}

impl Ledger {
    /// Parse and validate `bytes` read from `location`.
    pub fn parse(
        location: &str,
        _bytes: &[u8],
        _gateway: &str,
        _mode: &str,
    ) -> Result<Ledger, CoreError> {
        Err(CoreError::DeployLedgerInvalid {
            location: location.to_string(),
            problem: "not implemented".to_string(),
        })
    }

    /// Canonical bytes: pretty JSON plus one LF.
    pub fn to_bytes(&self) -> Vec<u8> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GW: &str = "tracer-gw";
    const MODE: &str = "dev";
    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn loc() -> String {
        ledger_location(GW, MODE)
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

    #[test]
    fn location_is_below_ign_deploy() {
        assert_eq!(loc(), "ign-deploy/tracer-gw/dev/ledger.json");
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
