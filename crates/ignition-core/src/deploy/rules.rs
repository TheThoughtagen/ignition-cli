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
}
