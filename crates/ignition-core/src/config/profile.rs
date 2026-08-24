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
}
