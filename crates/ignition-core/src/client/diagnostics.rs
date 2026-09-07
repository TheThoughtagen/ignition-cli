//! Diagnostics-bundle capability (09-05, EXT-02) — the support-bundle
//! slice of `GET/POST /data/api/v1/diagnostics/bundle/*`, wire-shaped
//! STRICTLY from the 09-02 live captures (09-LIVE-CAPTURES §5 — the
//! Pitfall-2 capture: observed, never guessed).
//!
//! **The state vocabulary is capture-locked (09-LIVE-CAPTURES §5 +
//! Decisions 1):** exactly `Generating` (mid-generation, both rigs) and
//! `Valid` (terminal/ready, both rigs) were observed — PascalCase;
//! lowercase guesses would have shipped wrong. Everything beyond these
//! two states (failure/cancel vocabularies) is NOT observable on a
//! healthy fresh rig and remains unenumerated — any other string must
//! ride through passthrough, never refuse, and is treated as UNKNOWN
//! (not terminal) by the wait action.
//!
//! **`fileSize` units (Decisions 2):** ABSENT while generating (not
//! null, not 0 — the key is missing), present integer **bytes** when
//! `Valid`, equal to the download's `Content-Length` exactly.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// POST path that starts bundle generation (no body).
pub const DIAGNOSTICS_GENERATE_PATH: &str = "/data/api/v1/diagnostics/bundle/generate";

/// GET path of the bundle status poll.
pub const DIAGNOSTICS_STATUS_PATH: &str = "/data/api/v1/diagnostics/bundle/status";

/// GET path of the bundle download (a ZIP, streamed).
pub const DIAGNOSTICS_DOWNLOAD_PATH: &str = "/data/api/v1/diagnostics/bundle/download";

/// The states that mean "still generating" — EXACTLY the captured
/// mid-generation strings, one member on the captured evidence
/// (09-LIVE-CAPTURES §5 Decisions 1). Case-SENSITIVE membership: the
/// gateway owns the vocabulary (PascalCase); this CLI never guesses.
pub const BUNDLE_GENERATING_STATES: &[&str] = &[
    // 8.3.6 rig A + 8.3.3 rig B: the POST generate answer and every
    // pre-completion status poll (09-LIVE-CAPTURES §5).
    "Generating",
];

/// The full CAPTURED state vocabulary — every state the two rigs ever
/// answered (09-LIVE-CAPTURES §5). A string OUTSIDE this set is
/// UNKNOWN: the wait action keeps polling (honest unknowns — a failure
/// state the captures never saw must not be declared terminal).
/// Unobserved members still round-trip through the wire model's
/// passthrough (version tolerance).
pub const BUNDLE_CAPTURED_STATES: &[&str] = &[
    // 8.3.6 rig A + 8.3.3 rig B (09-LIVE-CAPTURES §5).
    "Generating",
    // Terminal/ready on both rigs — `fileSize` present, download
    // repeatable, ready state persists after download.
    "Valid",
];

/// `true` when `state` is in the CAPTURED generating set — a
/// case-sensitive exact match (`BUNDLE_GENERATING_STATES` membership).
/// Unknown states are deliberately NOT generating (they are not
/// anything we have seen).
pub fn is_generating(state: &str) -> bool {
    BUNDLE_GENERATING_STATES.contains(&state)
}

/// Per-request timeout override for the bundle download (Pitfall 8):
/// the 30 s client default would TRUNCATE MB-sized bundles mid-stream.
/// Same class as the logs-download 120 s / project-export 120 s /
/// gwbk 300 s overrides — this ride goes through
/// `download_to_file`'s `RequestBuilder::timeout`, never a second
/// client. Pinned at 300 s by the unit test below (the REQUIRED
/// deterministic timeout-override check lives here, where the
/// constant is born).
pub const BUNDLE_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);

/// One bundle status body (`POST …/generate`'s 200 answer IS the same
/// shape — live capture: `{"state":"Generating"}`). `state` is a
/// String BY DECISION (Pitfall 2: never an enum from guessed values —
/// an unobserved future state must ride, never refuse the parse).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleStatusWire {
    /// The captured vocabulary is `Generating` (mid-generation) /
    /// `Valid` (terminal/ready) — PascalCase, both rigs
    /// (09-LIVE-CAPTURES §5). Anything else is an UNOBSERVED state
    /// riding passthrough: never an enum, never a refusal.
    #[serde(default)]
    pub state: String,
    /// Bundle size — **bytes** (Decisions 2: `fileSize` ==
    /// download `Content-Length` exactly). ABSENT while generating
    /// (the key is missing, not null) → `Option`; present when
    /// `Valid`.
    #[serde(
        rename = "fileSize",
        alias = "file_size",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub file_size: Option<u64>,
    /// Unknown keys round-trip (version tolerance).
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{BUNDLE_CAPTURED_STATES, BUNDLE_DOWNLOAD_TIMEOUT, BundleStatusWire, is_generating};

    /// THE 8.3.6 captures (rig A, 09-LIVE-CAPTURES §5): the generate
    /// answer and the generating poll (no fileSize key), then the
    /// terminal `Valid` poll with `fileSize` = 61053 (== the download
    /// Content-Length).
    #[test]
    fn bundle_status_parses_the_live_8_3_6_captures() {
        let generating: BundleStatusWire =
            serde_json::from_str(r#"{"state":"Generating"}"#).expect("generating capture parses");
        assert_eq!(generating.state, "Generating");
        assert_eq!(
            generating.file_size, None,
            "fileSize is ABSENT while generating (not null, not 0)"
        );

        let valid: BundleStatusWire = serde_json::from_str(r#"{"state":"Valid","fileSize":61053}"#)
            .expect("terminal capture parses");
        assert_eq!(valid.state, "Valid");
        assert_eq!(valid.file_size, Some(61_053), "bytes == Content-Length");

        // Wire-faithful round-trip: the gateway-native camelCase key on
        // the way out, absent while generating.
        let round = serde_json::to_value(&generating).expect("serialize");
        assert!(round.get("fileSize").is_none(), "absent stays absent");
        let round = serde_json::to_value(&valid).expect("serialize");
        assert_eq!(round["fileSize"], 61_053);
    }

    /// THE 8.3.3 capture (rig B, 09-LIVE-CAPTURES §5): same shape,
    /// `fileSize` = 55281 (no point-release drift).
    #[test]
    fn bundle_status_parses_the_live_8_3_3_capture() {
        let valid: BundleStatusWire = serde_json::from_str(r#"{"state":"Valid","fileSize":55281}"#)
            .expect("terminal capture parses");
        assert_eq!(valid.state, "Valid");
        assert_eq!(valid.file_size, Some(55_281));
    }

    /// The generating-set membership is exact and case-sensitive: the
    /// captured `Generating` is in; `Valid` (captured terminal) is
    /// NOT; an unknown state is NOT generating (it is not anything we
    /// have seen — honest unknowns).
    #[test]
    fn generating_membership_is_exact() {
        assert!(is_generating("Generating"));
        assert!(!is_generating("Valid"));
        // Pitfall 2 honesty: an unobserved state is not "generating"
        // either — the vocabulary is the gateway's, never ours.
        assert!(!is_generating("Failed"));
        assert!(!is_generating("generating"), "case-sensitive");
        assert!(!is_generating("RUNNING"), "no guessed vocabulary");
        assert!(!is_generating(""));
    }

    /// The captured vocabulary is exactly the two observed states and
    /// the generating set is its single-member subset (Decisions 1).
    #[test]
    fn captured_vocabulary_is_the_two_observed_states() {
        assert_eq!(BUNDLE_CAPTURED_STATES, &["Generating", "Valid"]);
        assert_eq!(super::BUNDLE_GENERATING_STATES, &["Generating"]);
    }

    /// THE deterministic timeout-override pin (Pitfall 8): the bundle
    /// download's per-request timeout is 300 s — the 30 s client
    /// default would truncate MB-sized bundles. This unit assertion is
    /// the REQUIRED check for the phase (it lives where the constant
    /// is born; the download ride through `download_to_file`'s
    /// `.timeout()` is pinned by construction + the rg review check).
    #[test]
    fn bundle_download_timeout_is_300_seconds() {
        assert_eq!(BUNDLE_DOWNLOAD_TIMEOUT, Duration::from_secs(300));
    }

    /// Unknown keys ride the flatten passthrough (version tolerance).
    #[test]
    fn unknown_keys_round_trip() {
        let wire: BundleStatusWire = serde_json::from_value(serde_json::json!({
            "state": "Valid",
            "fileSize": 61053,
            "zzFuturePointReleaseKey": {"future": true}
        }))
        .expect("unknown keys never refuse");
        assert_eq!(
            wire.extra.get("zzFuturePointReleaseKey"),
            Some(&serde_json::json!({"future": true})),
            "unknown keys ride passthrough"
        );
        assert_eq!(wire.state, "Valid");
    }

    /// BUNDLE_CAPTURED_STATES shapes the wait semantics: a captured
    /// NON-generating state is terminal; the two consts stay in sync
    /// by construction (generating ⊆ captured).
    #[test]
    fn generating_set_is_subset_of_captured_vocabulary() {
        for state in super::BUNDLE_GENERATING_STATES {
            assert!(
                BUNDLE_CAPTURED_STATES.contains(state),
                "every generating state is captured: {state}"
            );
        }
    }
}
