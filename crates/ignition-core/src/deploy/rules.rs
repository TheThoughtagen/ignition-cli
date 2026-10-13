//! Fixed render policy: which resource types and files a declared tree may
//! never ship, however the manifest allowlist reads (Phase 21, DRND-04).

/// `(module, type)` pairs that render refuses even when the manifest lists
/// them. Rendering these can lock an administrator out of the gateway
/// (replacing the security level or property singletons, the user sources, or
/// the API tokens that tooling authenticates with), so they stay
/// gateway-owned. Spellings are pinned to the OpenAPI extract by the
/// `denied_types_exist_in_openapi_extract` test.
pub const DENIED_TYPES: &[(&str, &str)] = &[
    ("ignition", "security-properties"),
    ("ignition", "security-levels"),
    ("ignition", "api-token"),
    ("ignition", "user-source"),
];

/// File names (final path component) the gateway writes itself, so a
/// declared tree must not carry them. `config-mode.json` is written by the
/// gateway's mode API; shipping one would overwrite the gateway's mode.
pub const GATEWAY_OWNED_FILES: &[&str] = &["config-mode.json"];

/// Whether `module/ty` is on the deny list.
pub fn is_denied(module: &str, ty: &str) -> bool {
    DENIED_TYPES.iter().any(|(m, t)| *m == module && *t == ty)
}

/// How a resource type carries its identity (`attributes.uuid`) and where its
/// files sit. The three classes come from the corpus census (RESEARCH
/// Pattern 4) and the 8.3.6 captures (decision CD3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UuidRule {
    /// One resource per type: files sit directly in `<module>/<type>/`, and
    /// its `resource.json` carries no uuid.
    Singleton,
    /// Many resources per type, one directory each, every one with an
    /// `attributes.uuid`. Render derives one for a resource that has none.
    Named,
    /// Named, but the corpus resources of this type carry no uuid
    /// (`ignition/store-and-forward-engine`,
    /// `com.inductiveautomation.perspective/themes`). Render never
    /// synthesizes one (RESEARCH Pitfall 5); an existing uuid is kept, because
    /// CD3 saw an 8.3.6 gateway write one for store-and-forward-engine.
    NamedNoUuid,
}

/// The render rule for one resource type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeRule {
    /// Module id.
    pub module: &'static str,
    /// Resource type.
    pub ty: &'static str,
    /// Identity class.
    pub uuid: UuidRule,
    /// The `scope` a synthesized `resource.json` carries.
    pub scope: &'static str,
}

const fn row(
    module: &'static str,
    ty: &'static str,
    uuid: UuidRule,
    scope: &'static str,
) -> TypeRule {
    TypeRule {
        module,
        ty,
        uuid,
        scope,
    }
}

/// Every resource type render accepts, sorted by (module, type) bytes. A type
/// without a row is refused (fail-closed): only evidence-backed types render.
///
/// Evidence per row is the comment above it: `corpus` is the sanitized
/// gateway corpus census (`tests/fixtures/deploy/corpus`, checked by
/// `type_rules_agree_with_corpus`), `CD3` the 8.3.6 live captures in
/// `21-LIVE-CAPTURES.md`. Denied types ([`DENIED_TYPES`]) have no row. The
/// four tag types are not here: they are governed by the `tags/` layout
/// (CD5), whose files use `unary-resource.json` and attribute shapes the
/// three classes do not describe.
pub const TYPE_RULES: &[TypeRule] = &[
    // corpus
    row(
        "com.cirruslink.mqtt.distributor.gateway",
        "general",
        UuidRule::Singleton,
        "A",
    ),
    // corpus
    row(
        "com.cirruslink.mqtt.distributor.gateway",
        "user",
        UuidRule::Named,
        "A",
    ),
    // corpus
    row(
        "com.cirruslink.mqtt.engine.gateway",
        "default-namespace",
        UuidRule::Named,
        "A",
    ),
    // corpus
    row(
        "com.cirruslink.mqtt.engine.gateway",
        "general",
        UuidRule::Singleton,
        "A",
    ),
    // corpus
    row(
        "com.cirruslink.mqtt.engine.gateway",
        "namespace-file",
        UuidRule::Singleton,
        "A",
    ),
    // corpus
    row(
        "com.cirruslink.mqtt.engine.gateway",
        "namespace-server-set",
        UuidRule::Named,
        "A",
    ),
    // corpus
    row(
        "com.cirruslink.mqtt.engine.gateway",
        "server",
        UuidRule::Named,
        "A",
    ),
    // corpus
    row(
        "com.cirruslink.mqtt.engine.gateway",
        "server-set",
        UuidRule::Named,
        "A",
    ),
    // corpus
    row(
        "com.cirruslink.mqtt.transmission.gateway",
        "general",
        UuidRule::Singleton,
        "A",
    ),
    // corpus
    row(
        "com.cirruslink.mqtt.transmission.gateway",
        "history-store",
        UuidRule::Named,
        "A",
    ),
    // corpus
    row(
        "com.cirruslink.mqtt.transmission.gateway",
        "server",
        UuidRule::Named,
        "A",
    ),
    // corpus
    row(
        "com.cirruslink.mqtt.transmission.gateway",
        "server-set",
        UuidRule::Named,
        "A",
    ),
    // corpus
    row(
        "com.cirruslink.mqtt.transmission.gateway",
        "transmitter",
        UuidRule::Named,
        "A",
    ),
    // CD3 (captured on 8.3.6)
    row(
        "com.inductiveautomation.alarm-notification",
        "alarm-notification-profile",
        UuidRule::Named,
        "A",
    ),
    // corpus
    row(
        "com.inductiveautomation.eam",
        "agent-group",
        UuidRule::Named,
        "A",
    ),
    // corpus
    row(
        "com.inductiveautomation.eam",
        "event-thresholds",
        UuidRule::Singleton,
        "A",
    ),
    // corpus
    row(
        "com.inductiveautomation.eam",
        "module-certificates",
        UuidRule::Named,
        "A",
    ),
    // corpus
    row(
        "com.inductiveautomation.eam",
        "module-eulas",
        UuidRule::Named,
        "A",
    ),
    // corpus
    row(
        "com.inductiveautomation.eam",
        "module-settings",
        UuidRule::Singleton,
        "A",
    ),
    // corpus
    row(
        "com.inductiveautomation.historian",
        "historian-provider",
        UuidRule::Named,
        "A",
    ),
    // corpus
    row(
        "com.inductiveautomation.opcua",
        "access-control",
        UuidRule::Singleton,
        "A",
    ),
    // corpus
    row(
        "com.inductiveautomation.opcua",
        "device",
        UuidRule::Named,
        "A",
    ),
    // corpus
    row(
        "com.inductiveautomation.opcua",
        "one-time",
        UuidRule::Singleton,
        "A",
    ),
    // corpus
    row(
        "com.inductiveautomation.opcua",
        "server-config",
        UuidRule::Singleton,
        "A",
    ),
    // corpus
    row(
        "com.inductiveautomation.perspective",
        "themes",
        UuidRule::NamedNoUuid,
        "G",
    ),
    // corpus
    row(
        "com.inductiveautomation.sfc",
        "chart-settings",
        UuidRule::Singleton,
        "A",
    ),
    // corpus
    row("ignition", "alarm-journal", UuidRule::Named, "A"),
    // corpus
    row("ignition", "audit-profile", UuidRule::Named, "A"),
    // corpus
    row("ignition", "cobranding", UuidRule::Singleton, "A"),
    // corpus
    row("ignition", "database-connection", UuidRule::Named, "A"),
    // corpus
    row("ignition", "database-driver", UuidRule::Named, "A"),
    // corpus; CD1 and CD3 (captured on 8.3.6)
    row("ignition", "database-translator", UuidRule::Named, "A"),
    // corpus
    row("ignition", "edge-sync-settings", UuidRule::Singleton, "A"),
    // corpus
    row(
        "ignition",
        "edge-system-properties",
        UuidRule::Singleton,
        "A",
    ),
    // corpus
    row("ignition", "email-profile", UuidRule::Named, "A"),
    // corpus
    row(
        "ignition",
        "gateway-network-proxy-rules",
        UuidRule::Singleton,
        "A",
    ),
    // corpus
    row(
        "ignition",
        "gateway-network-queue-settings",
        UuidRule::Singleton,
        "A",
    ),
    // corpus
    row(
        "ignition",
        "gateway-network-settings",
        UuidRule::Singleton,
        "A",
    ),
    // corpus
    row(
        "ignition",
        "general-alarm-settings",
        UuidRule::Singleton,
        "A",
    ),
    // CD3 (captured on 8.3.6)
    row("ignition", "holiday", UuidRule::Named, "A"),
    // corpus
    row("ignition", "identity-provider", UuidRule::Named, "A"),
    // CD3 (captured on 8.3.6)
    row("ignition", "oauth2-client", UuidRule::Named, "A"),
    // corpus
    row("ignition", "opc-connection", UuidRule::Named, "A"),
    // corpus
    row("ignition", "quickstart", UuidRule::Singleton, "A"),
    // CD3 (captured on 8.3.6)
    row("ignition", "roster-config", UuidRule::Named, "A"),
    // corpus
    row("ignition", "schedule", UuidRule::Named, "A"),
    // corpus
    row("ignition", "security-zone", UuidRule::Named, "A"),
    // CD3 (captured on 8.3.6)
    row("ignition", "service-connector", UuidRule::Named, "A"),
    // corpus
    row(
        "ignition",
        "store-and-forward-engine",
        UuidRule::NamedNoUuid,
        "G",
    ),
    // corpus
    row("ignition", "system-properties", UuidRule::Singleton, "A"),
];

/// The rule for `module/ty`, if the type has evidence.
pub fn type_rule(module: &str, ty: &str) -> Option<&'static TypeRule> {
    TYPE_RULES
        .binary_search_by(|r| {
            r.module
                .as_bytes()
                .cmp(module.as_bytes())
                .then_with(|| r.ty.as_bytes().cmp(ty.as_bytes()))
        })
        .ok()
        .map(|i| &TYPE_RULES[i])
}

/// The secret-typed config fields of one resource type, from the 8.3.6
/// OpenAPI (`tests/fixtures/deploy/openapi-8.3.6-resource-types.json`,
/// `secret_fields`; Phase 21, plan 21-01). Each path is dotted from the root
/// of the type's `config.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretFields {
    /// Module id.
    pub module: &'static str,
    /// Resource type.
    pub ty: &'static str,
    /// Dotted member paths whose value must be absent, null or Referenced.
    pub paths: &'static [&'static str],
}

/// The ten resource types whose 8.3.6 schema has secret-typed fields, sorted
/// by (module, type) bytes. `secret_fields_agree_with_openapi_extract` pins
/// the table to the extract. The email-profile path is
/// `settings.settings.password` (corrected from RESEARCH Pattern 7 in 21-01;
/// the corpus `WH_Classic_SMTP/config.json` has the nested layout).
pub const SECRET_FIELDS: &[SecretFields] = &[
    SecretFields {
        module: "com.inductiveautomation.alarm-notification",
        ty: "alarm-notification-profile",
        paths: &[
            "settings.settings.password",
            "settings.twoWayPop3.pop3Password",
            "settings.voipGateway.password",
        ],
    },
    SecretFields {
        module: "com.inductiveautomation.opcua",
        ty: "device",
        paths: &["settings.advanced.password"],
    },
    SecretFields {
        module: "ignition",
        ty: "database-connection",
        paths: &["password"],
    },
    SecretFields {
        module: "ignition",
        ty: "email-profile",
        paths: &["settings.settings.password"],
    },
    SecretFields {
        module: "ignition",
        ty: "identity-provider",
        paths: &["settings.clientSecret"],
    },
    SecretFields {
        module: "ignition",
        ty: "oauth2-client",
        paths: &["clientSecret"],
    },
    SecretFields {
        module: "ignition",
        ty: "opc-connection",
        paths: &[
            "settings.authentication.password",
            "settings.authentication.privateKeyPem",
            "settings.security.keyStoreAliasPassword",
        ],
    },
    SecretFields {
        module: "ignition",
        ty: "service-connector",
        paths: &[
            "settings.password",
            "settings.ssl.keyPassword",
            "settings.ssl.keystore.key",
            "settings.ssl.keystore.password",
            "settings.ssl.truststore.password",
        ],
    },
    SecretFields {
        module: "ignition",
        ty: "system-properties",
        paths: &["errorReportPassword"],
    },
    SecretFields {
        module: "ignition",
        ty: "user-source",
        paths: &[
            "settings.connectionPassword",
            "settings.ldap.connectionPassword",
        ],
    },
];

/// The secret-typed paths of `module/ty`; empty when the type has none (or is
/// not described by the OpenAPI at all).
pub fn secret_paths(module: &str, ty: &str) -> &'static [&'static str] {
    SECRET_FIELDS
        .iter()
        .find(|s| s.module == module && s.ty == ty)
        .map_or(&[], |s| s.paths)
}

/// Every (module, type) pair the 8.3.6 OpenAPI describes, sorted by bytes.
/// `openapi_types_equal_extract` pins it to the extract, so it cannot drift
/// from the evidence. A type outside this list (for example the Cirrus Link
/// MQTT types) has no schema, so secret detection there falls back to the
/// generic Embedded scan and the key-name rule (21-08).
pub const OPENAPI_TYPES: &[(&str, &str)] = &[
    (
        "com.inductiveautomation.alarm-notification",
        "alarm-notification-profile",
    ),
    ("com.inductiveautomation.eam", "agent-group"),
    ("com.inductiveautomation.eam", "agent-management"),
    ("com.inductiveautomation.eam", "eam-tasks"),
    ("com.inductiveautomation.eam", "event-thresholds"),
    ("com.inductiveautomation.eam", "hw-license-management"),
    ("com.inductiveautomation.eam", "leased-license-management"),
    ("com.inductiveautomation.eam", "module-certificates"),
    ("com.inductiveautomation.eam", "module-eulas"),
    ("com.inductiveautomation.eam", "module-settings"),
    ("com.inductiveautomation.eam", "remote-upgrade"),
    ("com.inductiveautomation.historian", "historian-provider"),
    ("com.inductiveautomation.opcua", "access-control"),
    ("com.inductiveautomation.opcua", "device"),
    ("com.inductiveautomation.opcua", "server-config"),
    (
        "com.inductiveautomation.opcua.drivers.bacnet",
        "BacnetIpLocalDeviceConfig",
    ),
    ("com.inductiveautomation.perspective", "fonts"),
    ("com.inductiveautomation.perspective", "icons"),
    ("com.inductiveautomation.perspective", "themes"),
    ("com.inductiveautomation.sfc", "chart-settings"),
    (
        "com.inductiveautomation.sip-notification",
        "script-settings",
    ),
    ("ignition", "alarm-journal"),
    ("ignition", "api-token"),
    ("ignition", "audit-profile"),
    ("ignition", "cobranding"),
    ("ignition", "database-connection"),
    ("ignition", "database-driver"),
    ("ignition", "database-translator"),
    ("ignition", "edge-system-properties"),
    ("ignition", "email-profile"),
    ("ignition", "gateway-network-incoming"),
    ("ignition", "gateway-network-outgoing"),
    ("ignition", "gateway-network-proxy-rules"),
    ("ignition", "gateway-network-queue-settings"),
    ("ignition", "gateway-network-settings"),
    ("ignition", "general-alarm-settings"),
    ("ignition", "holiday"),
    ("ignition", "identity-provider"),
    ("ignition", "keyboard_layout"),
    ("ignition", "local-system-properties"),
    ("ignition", "metrics-dashboard"),
    ("ignition", "oauth2-client"),
    ("ignition", "opc-connection"),
    ("ignition", "quickstart"),
    ("ignition", "roster-config"),
    ("ignition", "schedule"),
    ("ignition", "secret-provider"),
    ("ignition", "security-levels"),
    ("ignition", "security-properties"),
    ("ignition", "security-zone"),
    ("ignition", "service-connector"),
    ("ignition", "store-and-forward-engine"),
    ("ignition", "system-properties"),
    ("ignition", "tag-provider"),
    ("ignition", "translations"),
    ("ignition", "user-source"),
];

/// Whether the 8.3.6 OpenAPI describes `module/ty`: the schema is
/// authoritative for its secret-typed fields.
pub fn openapi_describes(module: &str, ty: &str) -> bool {
    OPENAPI_TYPES.binary_search(&(module, ty)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPENAPI: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/deploy/openapi-8.3.6-resource-types.json"
    ));

    #[test]
    fn deny_list_is_exact() {
        assert!(is_denied("ignition", "security-properties"));
        assert!(is_denied("ignition", "user-source"));
        assert!(!is_denied("ignition", "database-translator"));
        assert!(!is_denied("other", "api-token"));
    }

    #[test]
    fn type_rules_are_sorted_unique_and_exclude_denied() {
        for pair in TYPE_RULES.windows(2) {
            let a = (pair[0].module.as_bytes(), pair[0].ty.as_bytes());
            let b = (pair[1].module.as_bytes(), pair[1].ty.as_bytes());
            assert!(
                a < b,
                "not strictly sorted: {}/{} then {}/{}",
                pair[0].module,
                pair[0].ty,
                pair[1].module,
                pair[1].ty
            );
        }
        for r in TYPE_RULES {
            assert!(
                !is_denied(r.module, r.ty),
                "{}/{} is denied",
                r.module,
                r.ty
            );
            assert!(
                matches!(r.scope, "A" | "G"),
                "{}/{} scope {}",
                r.module,
                r.ty,
                r.scope
            );
            assert_eq!(type_rule(r.module, r.ty), Some(r));
            if r.uuid == UuidRule::NamedNoUuid {
                assert_eq!(r.scope, "G", "{}/{}", r.module, r.ty);
            }
        }
        assert_eq!(type_rule("ignition", "no-such-type"), None);
        assert_eq!(type_rule("ignition", "security-levels"), None);
        // the two no-uuid types, and no others
        let no_uuid: Vec<_> = TYPE_RULES
            .iter()
            .filter(|r| r.uuid == UuidRule::NamedNoUuid)
            .map(|r| (r.module, r.ty))
            .collect();
        assert_eq!(
            no_uuid,
            [
                ("com.inductiveautomation.perspective", "themes"),
                ("ignition", "store-and-forward-engine")
            ]
        );
    }

    #[test]
    fn type_rules_agree_with_openapi_extract() {
        let doc: serde_json::Value = serde_json::from_str(OPENAPI).expect("extract parses");
        let types = doc["types"].as_array().expect("types array");
        // RESEARCH was corrected from 87/49 to 56 types: 39 named, 17 singleton
        assert_eq!(types.len(), 56);
        assert_eq!(types.iter().filter(|t| t["named"] == true).count(), 39);
        let mut compared = 0;
        for t in types {
            let (module, ty) = (
                t["module"].as_str().expect("module"),
                t["type"].as_str().expect("type"),
            );
            let Some(rule) = type_rule(module, ty) else {
                continue;
            };
            compared += 1;
            let named = t["named"].as_bool().expect("named flag");
            assert_eq!(
                named,
                rule.uuid != UuidRule::Singleton,
                "{module}/{ty}: extract says named={named}, rule says {:?}",
                rule.uuid
            );
        }
        assert!(compared > 25, "only {compared} rows were comparable");
    }

    #[test]
    fn secret_fields_agree_with_openapi_extract() {
        let doc: serde_json::Value = serde_json::from_str(OPENAPI).expect("extract parses");
        let fields = doc["secret_fields"].as_array().expect("secret_fields");
        assert_eq!(fields.len(), SECRET_FIELDS.len());
        assert_eq!(SECRET_FIELDS.len(), 10);
        for (row, want) in SECRET_FIELDS.iter().zip(fields) {
            assert_eq!(row.module, want["module"].as_str().expect("module"));
            assert_eq!(row.ty, want["type"].as_str().expect("type"));
            let paths: Vec<&str> = want["paths"]
                .as_array()
                .expect("paths")
                .iter()
                .map(|p| p.as_str().expect("path"))
                .collect();
            assert_eq!(row.paths, paths.as_slice(), "{}/{}", row.module, row.ty);
        }
        for pair in SECRET_FIELDS.windows(2) {
            assert!(
                (pair[0].module.as_bytes(), pair[0].ty.as_bytes())
                    < (pair[1].module.as_bytes(), pair[1].ty.as_bytes())
            );
        }
        assert_eq!(
            secret_paths("ignition", "email-profile"),
            ["settings.settings.password"]
        );
        assert!(secret_paths("ignition", "schedule").is_empty());
        assert!(secret_paths("no.such", "type").is_empty());
        // every secret-bearing type is a described type
        for row in SECRET_FIELDS {
            assert!(openapi_describes(row.module, row.ty));
        }
    }

    #[test]
    fn openapi_types_equal_extract() {
        let doc: serde_json::Value = serde_json::from_str(OPENAPI).expect("extract parses");
        let mut want: Vec<(String, String)> = doc["types"]
            .as_array()
            .expect("types")
            .iter()
            .map(|t| {
                (
                    t["module"].as_str().expect("module").to_string(),
                    t["type"].as_str().expect("type").to_string(),
                )
            })
            .collect();
        want.sort_by(|a, b| {
            (a.0.as_bytes(), a.1.as_bytes()).cmp(&(b.0.as_bytes(), b.1.as_bytes()))
        });
        let got: Vec<(String, String)> = OPENAPI_TYPES
            .iter()
            .map(|(m, t)| ((*m).to_string(), (*t).to_string()))
            .collect();
        assert_eq!(got, want);
        assert_eq!(OPENAPI_TYPES.len(), 56);
        assert!(openapi_describes("ignition", "database-connection"));
        assert!(!openapi_describes(
            "com.cirruslink.mqtt.engine.gateway",
            "server"
        ));
    }
}
