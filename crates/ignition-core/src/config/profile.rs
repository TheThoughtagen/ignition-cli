//! Config + Profile serde structs — the on-disk shape of `config.toml`.
//!
//! Secrets are REFERENCES (env var names, keyring user strings), never
//! values: no field on [`Profile`] or [`AuthRef`] ever holds a secret string
//! (CORE-02). No `deny_unknown_fields` anywhere (research Pitfall 7 —
//! forward compat; unknown keys warn at load time instead, see
//! [`super::load`]).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The whole `config.toml`. `profiles` is a `BTreeMap` so `profile list`
/// output — and therefore every golden — is deterministic by name.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// The profile commands operate on by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
    /// Named gateway profiles (`[profiles.NAME]` tables).
    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,
    /// Rig family defaults (`[rig]` table — just the default rig name
    /// today; 04-01).
    #[serde(default, skip_serializing_if = "rig_config_is_empty")]
    pub rig: RigConfig,
    /// Named compose rigs (`[rigs.NAME]` tables — 04-01). Omitted from
    /// serialization when empty so profile-only configs keep their exact
    /// on-disk shape (the save goldens).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rigs: BTreeMap<String, RigEntry>,
    /// UI preferences (`[ui]` table — TUIX-05 plumbing). PLUMBING ONLY:
    /// `theme` is carried on the config, never rendered (rendering is
    /// Phase 12, TUIX-03/04). Omitted from serialization at defaults so
    /// legacy configs round-trip byte-identically; a wrong-shaped `[ui]`
    /// degrades to the default with a warning (see [`lenient_ui`]).
    #[serde(
        default,
        skip_serializing_if = "UiConfig::is_default",
        deserialize_with = "lenient_ui"
    )]
    pub ui: UiConfig,
}

/// The `[ui]` table: TUI preference keys (TUIX-05 plumbing).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UiConfig {
    /// Preferred UI theme name (e.g. `"dark"`). `None` = the built-in
    /// default. Carried only — nothing consumes it until Phase 12.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
}

impl UiConfig {
    /// True when nothing is set — keeps `[ui]` out of configs that never
    /// carried one (the `rig_config_is_empty` precedent).
    pub fn is_default(ui: &UiConfig) -> bool {
        *ui == UiConfig::default()
    }
}

/// Lenient `poll_interval_secs` extraction (TUIX-05): an integer loads
/// verbatim — INCLUDING 0, which the load-time clamp (see
/// [`super::load`]) refuses; anything else (string, float, negative,
/// table) warns and degrades to `None` (default cadence). A typo in the
/// NEW key never fails the load.
fn lenient_u64<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    let value = toml::Value::deserialize(d)?;
    match value {
        toml::Value::Integer(v) if v >= 0 => Ok(Some(v as u64)),
        other => {
            tracing::warn!(
                value = %other,
                "poll_interval_secs must be a non-negative integer — ignoring (default cadence in use)"
            );
            Ok(None)
        }
    }
}

/// Lenient `[ui]` extraction (TUIX-05): a well-shaped `[ui]` table loads;
/// anything else (a bare scalar, a mistyped member) warns and degrades to
/// the default. Unknown keys INSIDE a valid table are simply ignored (no
/// `deny_unknown_fields`, ever — Pitfall 7 forward compat).
fn lenient_ui<'de, D: serde::Deserializer<'de>>(d: D) -> Result<UiConfig, D::Error> {
    let value = toml::Value::deserialize(d)?;
    match UiConfig::deserialize(value) {
        Ok(ui) => Ok(ui),
        Err(_) => {
            tracing::warn!("[ui] is not a valid table — using defaults");
            Ok(UiConfig::default())
        }
    }
}

/// True when the `[rig]` block carries nothing worth serializing —
/// keeps `[rig]` out of profile-only configs entirely.
fn rig_config_is_empty(rig: &RigConfig) -> bool {
    rig.default.is_none()
}

/// The `[rig]` table: rig-family defaults (04-01).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RigConfig {
    /// Name of the `[rigs.*]` entry `ign rig` targets when no
    /// `--rig`/`IGNITION_RIG` names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

/// One named compose rig (`[rigs.NAME]`, 04-01). References only —
/// never secrets (the compose file itself owns those).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigEntry {
    /// Path to the rig's compose file (`~` and `${VAR}` expanded at use
    /// time — manual expansion, no new dependency).
    pub compose_file: String,
    /// Explicit compose project name (`-p`). OPTIONAL — omit to honor
    /// the rig's own `.env` `COMPOSE_PROJECT_NAME` (the identity truth,
    /// research Pattern 1); set only to override it deliberately.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
}

/// One gateway profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// Gateway base URL.
    pub url: url::Url,
    /// Optional display label (CORE-01) — absent from TOML and JSON when
    /// unset so existing configs and goldens don't churn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Verify TLS certificates (self-signed dev rigs turn this off).
    #[serde(default = "default_ssl_verify")]
    pub ssl_verify: bool,
    /// HOW to find the credential — a reference, never a value.
    #[serde(default)]
    pub auth: AuthRef,
    /// The CLI-GENERATED webdev scriptExec shared secret (05-03) — the
    /// ONE deliberate value-carrying exception to the references-only
    /// rule: this secret is not a user credential but a token the CLI
    /// itself mints at deploy time, must round-trip verbatim (the
    /// deployed route compares it byte-for-byte), and cannot live in
    /// an env var another tool owns. It rides ONLY in this 0600 config
    /// store and the baked route zip member — never in any action
    /// result, log, or JSON envelope (the redaction discipline).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webdev_secret: Option<String>,
    /// TUI background-refresh poll interval in seconds (TUIX-05
    /// plumbing). `None` = the default cadence. Sub-second values
    /// (`Some(0)`) are REFUSED by load-time validation on the normal CLI
    /// path (exit 3, `poll_interval_too_small`) and degraded to `None`
    /// on the TUI path ([`super::load_for_tui`]); a wrong-TYPED value
    /// degrades to `None` with a warning (see [`lenient_u64`]) — a typo
    /// in the new key never bricks the load.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_u64"
    )]
    pub poll_interval_secs: Option<u64>,
}

fn default_ssl_verify() -> bool {
    true
}

/// Credential reference — the three supported forms, untagged so the TOML
/// stays flat: `auth = { token_env = "X" }`, `auth = { keyring =
/// "profile:prod" }`, or `auth = { user_env = "U", password_env = "P" }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AuthRef {
    /// Token lives in this env var.
    TokenEnv {
        /// Env var NAME (never the token itself).
        token_env: String,
    },
    /// Token lives in the OS keyring (service `ignition-cli`) under this
    /// user string, e.g. `"profile:prod"`.
    Keyring {
        /// Keyring user string.
        keyring: String,
    },
    /// Basic pair from two env vars.
    Basic {
        /// Env var NAME for the user.
        user_env: String,
        /// Env var NAME for the password.
        password_env: String,
    },
}

impl AuthRef {
    /// Safe kind string for output models — never a secret or env value.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::TokenEnv { .. } => "token_env",
            Self::Keyring { .. } => "keyring",
            Self::Basic { .. } => "basic",
        }
    }
}

impl Default for AuthRef {
    /// A profile without an `auth` key resolves through the generic env
    /// token path (`IGNITION_TOKEN`) — the last env-token step of the LOCKED
    /// resolution order.
    fn default() -> Self {
        Self::TokenEnv {
            token_env: "IGNITION_TOKEN".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AuthRef, Config};

    /// AuthRef untagged round-trip across all three reference forms, and
    /// `kind()` never leaks a value.
    #[test]
    fn auth_ref_untagged_round_trip() {
        let toml = r#"
active = "dev"

[profiles.dev]
url = "http://localhost:9088/"
label = "Dev rig"
auth = { token_env = "IGNITION_TOKEN" }

[profiles.prod]
url = "https://gw.example.com:8443/"
auth = { keyring = "profile:prod" }

[profiles.rig]
url = "http://10.0.0.5:9088/"
ssl_verify = false
auth = { user_env = "IGNITION_USER", password_env = "IGNITION_PASSWORD" }
"#;
        let config: Config = toml::from_str(toml).expect("parse");
        assert_eq!(config.active.as_deref(), Some("dev"));
        assert_eq!(config.profiles.len(), 3);

        let dev = &config.profiles["dev"];
        assert_eq!(dev.label.as_deref(), Some("Dev rig"));
        assert!(dev.ssl_verify, "ssl_verify defaults true");
        assert_eq!(
            dev.auth,
            AuthRef::TokenEnv {
                token_env: "IGNITION_TOKEN".into()
            }
        );
        assert_eq!(dev.auth.kind(), "token_env");

        let prod = &config.profiles["prod"];
        assert_eq!(prod.label, None, "label absent when unset");
        assert_eq!(
            prod.auth,
            AuthRef::Keyring {
                keyring: "profile:prod".into()
            }
        );
        assert_eq!(prod.auth.kind(), "keyring");

        let rig = &config.profiles["rig"];
        assert!(!rig.ssl_verify, "ssl_verify = false honored");
        assert_eq!(
            rig.auth,
            AuthRef::Basic {
                user_env: "IGNITION_USER".into(),
                password_env: "IGNITION_PASSWORD".into(),
            }
        );
        assert_eq!(rig.auth.kind(), "basic");

        // Serialize round-trip: label omitted when None, ssl_verify omitted
        // when default (serde defaults skip nothing here except via attrs).
        let reserialized = toml::to_string(&config).expect("serialize");
        assert!(reserialized.contains("label = \"Dev rig\""));
        assert!(!reserialized.contains("[profiles.prod]\nlabel"));
        let back: Config = toml::from_str(&reserialized).expect("re-parse");
        assert_eq!(back, config);
    }

    /// Missing `auth` falls back to the generic env token reference.
    #[test]
    fn auth_defaults_to_generic_token_env() {
        let toml = "[profiles.dev]\nurl = \"http://localhost:9088/\"\n";
        let config: Config = toml::from_str(toml).expect("parse");
        assert_eq!(
            config.profiles["dev"].auth,
            AuthRef::TokenEnv {
                token_env: "IGNITION_TOKEN".into()
            }
        );
    }

    /// New-schema round trip (TUIX-05): `[ui]` + `poll_interval_secs`
    /// survive load → serialize → load losslessly.
    #[test]
    fn ui_and_poll_interval_round_trip() {
        let toml = r#"
active = "dev"

[ui]
theme = "dark"

[profiles.dev]
url = "http://localhost:9088/"
poll_interval_secs = 10
"#;
        let config: Config = toml::from_str(toml).expect("parse");
        assert_eq!(config.ui.theme.as_deref(), Some("dark"));
        assert_eq!(config.profiles["dev"].poll_interval_secs, Some(10));

        let reserialized = toml::to_string(&config).expect("serialize");
        let back: Config = toml::from_str(&reserialized).expect("re-parse");
        assert_eq!(back, config, "round trip must be lossless");
    }

    /// Legacy-shape pin: a config without the new keys serializes WITHOUT
    /// them — existing configs keep their exact on-disk shape (goldens).
    #[test]
    fn legacy_config_serializes_without_new_keys() {
        let toml = "[profiles.dev]\nurl = \"http://localhost:9088/\"\n";
        let config: Config = toml::from_str(toml).expect("parse");
        let out = toml::to_string(&config).expect("serialize");
        assert!(
            !out.contains("ui") && !out.contains("poll_interval_secs"),
            "new keys must stay off legacy-shaped configs: {out}"
        );
    }

    /// Lenient degrade (TUIX-05): a wrong-TYPED value in the NEW surface
    /// never fails the load — it warns and falls back to defaults.
    #[test]
    fn lenient_degrade_on_bad_typed_new_keys() {
        let toml = r#"
[ui]
theme = "dark"

[profiles.dev]
url = "http://localhost:9088/"
poll_interval_secs = "banana"
"#;
        let config: Config = toml::from_str(toml).expect("typo'd new key must not fail the load");
        assert_eq!(
            config.profiles["dev"].poll_interval_secs, None,
            "bad poll_interval_secs degrades to None"
        );

        let toml = "ui = 42\n[profiles.dev]\nurl = \"http://localhost:9088/\"\n";
        let config: Config = toml::from_str(toml).expect("bad [ui] shape must not fail the load");
        assert_eq!(
            config.ui,
            super::UiConfig::default(),
            "non-table [ui] degrades to the default"
        );
    }

    /// A well-shaped `[ui]` table with FUTURE keys inside still loads —
    /// unknown keys inside are ignored, never denied (Pitfall 7).
    #[test]
    fn ui_table_with_unknown_keys_still_loads() {
        let toml = r#"
[ui]
theme = "dark"
future_key = "whatever"

[profiles.dev]
url = "http://localhost:9088/"
"#;
        let config: Config = toml::from_str(toml).expect("parse");
        assert_eq!(config.ui.theme.as_deref(), Some("dark"));
    }

    /// The clamp's floor belongs to validation, not serde: 0 parses fine
    /// here (permissive deserializer) and is refused by `load` (tested in
    /// `config::tests`).
    #[test]
    fn poll_interval_zero_parses_leniently() {
        let toml = "[profiles.dev]\nurl = \"http://localhost:9088/\"\npoll_interval_secs = 0\n";
        let config: Config = toml::from_str(toml).expect("parse");
        assert_eq!(config.profiles["dev"].poll_interval_secs, Some(0));
    }
}
