//! Render stage 7: the fail-closed lints (Phase 21, plan 21-08, DRND-07 and
//! DRND-08).
//!
//! - **Secrets** ([`scan_embedded`], [`scan_secrets`]): an `Embedded` secret
//!   object anywhere in a `.json` member; a non-null, non-Referenced value at
//!   an OpenAPI secret-typed path ([`secret_paths`]); and, in a module type
//!   the 8.3.6 OpenAPI does not describe, a non-empty string under a key whose
//!   name contains `password` or `secret`.
//! - **Windows safety** (`lint_tree`): illegal names, case-fold collisions
//!   and the path budget (D-04).
//! - **Line endings**: a CR byte in a text member (D-01).
//!
//! Every output names a location, never a value. [`SecretScan`] has no field
//! that can hold a JSON string, so a secret cannot reach an error, a warning,
//! the `--json` envelope or a log through this module (T-21-33).

use super::layout::ResourceKey;
use super::node::Node;
use super::rules::{openapi_describes, secret_paths};

/// What a secret scan of one parsed file found. Locations only.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SecretScan {
    /// `<origin>: <json path> (<kind>)` for every refused location, in
    /// document order.
    pub refused: Vec<String>,
    /// Warning texts naming a path, in document order.
    pub warnings: Vec<String>,
}

/// The root of a document, as a JSON path.
const ROOT_PATH: &str = "$";

/// Whether `node` is an `Embedded` secret object: `"type": "Embedded"` with an
/// object `data`.
fn is_embedded(node: &Node) -> bool {
    matches!(node.get("type"), Some(Node::Str(t)) if t == "Embedded")
        && matches!(node.get("data"), Some(Node::Obj(_)))
}

/// Whether `node` is a `Referenced` secret object: `"type": "Referenced"`
/// with an object `data` (CD7: `data {providerName, secretName}`).
fn is_referenced(node: &Node) -> bool {
    matches!(node.get("type"), Some(Node::Str(t)) if t == "Referenced")
        && matches!(node.get("data"), Some(Node::Obj(_)))
}

/// Whether a member named `key` looks like it holds a secret: its name
/// contains `password` or `secret` (ASCII case-insensitive).
fn key_looks_secret(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("password") || lower.contains("secret")
}

/// Join a member name onto a JSON path.
fn member_path(parent: &str, key: &str) -> String {
    if parent.is_empty() {
        key.to_string()
    } else {
        format!("{parent}.{key}")
    }
}

/// What a scan applies besides the generic Embedded rule.
struct Rules<'a> {
    /// OpenAPI secret-typed paths of the resource type (config.json only).
    schema: &'a [&'static str],
    /// Apply the key-name rule (config.json of a resource type).
    key_names: bool,
    /// The 8.3.6 OpenAPI describes the type: key-name hits warn, not refuse.
    described: bool,
}

struct Walker<'a> {
    origin: &'a str,
    rules: Option<Rules<'a>>,
    scan: SecretScan,
}

impl Walker<'_> {
    fn refuse(&mut self, path: &str, kind: &str) {
        let shown = if path.is_empty() { ROOT_PATH } else { path };
        self.scan
            .refused
            .push(format!("{}: {shown} ({kind})", self.origin));
    }

    /// Visit `node` at `path`. `key_hit` is true when `node` is a string (or
    /// an array of them) held by a secret-looking key; `in_ref_data` is true
    /// inside the `data` of a Referenced object, where `secretName` is a name,
    /// not a secret.
    fn walk(&mut self, node: &Node, path: &str, key_hit: bool, in_ref_data: bool) {
        match node {
            Node::Obj(members) => {
                if is_embedded(node) {
                    self.refuse(path, "embedded");
                    return;
                }
                let referenced = is_referenced(node);
                for (key, value) in members {
                    let child = member_path(path, key);
                    let at_schema = self
                        .rules
                        .as_ref()
                        .is_some_and(|r| r.schema.contains(&child.as_str()));
                    if at_schema {
                        self.schema_value(value, &child, in_ref_data);
                        continue;
                    }
                    let hit = self.rules.as_ref().is_some_and(|r| r.key_names)
                        && !in_ref_data
                        && key_looks_secret(key);
                    let skip = in_ref_data || (referenced && key == "data");
                    self.walk(value, &child, hit, skip);
                }
            }
            Node::Arr(items) => {
                for (i, item) in items.iter().enumerate() {
                    self.walk(item, &format!("{path}[{i}]"), key_hit, in_ref_data);
                }
            }
            Node::Str(s) => {
                if key_hit && !s.is_empty() {
                    self.key_name_hit(path);
                }
            }
            Node::Lit(_) => {}
        }
    }

    /// A value at an OpenAPI secret-typed path: null and Referenced pass, an
    /// Embedded object is refused as embedded, anything else is plaintext.
    fn schema_value(&mut self, value: &Node, path: &str, in_ref_data: bool) {
        match value {
            Node::Lit(l) if l == "null" => {}
            Node::Obj(_) if is_embedded(value) => self.refuse(path, "embedded"),
            Node::Obj(_) if is_referenced(value) => self.walk(value, path, false, in_ref_data),
            _ => self.refuse(path, "plaintext"),
        }
    }

    /// A non-empty string under a secret-looking key outside the schema paths.
    fn key_name_hit(&mut self, path: &str) {
        let described = self.rules.as_ref().is_some_and(|r| r.described);
        if described {
            self.scan.warnings.push(format!(
                "possible plaintext secret at {}: {path}; this field is not secret-typed in the 8.3.6 OpenAPI, check it is not a secret",
                self.origin
            ));
        } else {
            self.refuse(path, "plaintext, by key name");
        }
    }
}

/// The generic Embedded scan: every object in `node` that is an Embedded
/// secret, as `<origin>: <json path> (embedded)`. Used for every `.json`
/// member, including tag files, which have no resource type.
pub fn scan_embedded(origin: &str, node: &Node) -> SecretScan {
    let mut walker = Walker {
        origin,
        rules: None,
        scan: SecretScan::default(),
    };
    walker.walk(node, "", false, false);
    walker.scan
}

/// Scan one parsed resource file.
///
/// Always the generic Embedded rule. When `file_name` (the file's path
/// relative to the resource root) is `config.json`, also:
///
/// - each [`secret_paths`] entry must be absent, `null` or Referenced; an
///   Embedded object there is reported once, as embedded; a string or any
///   other value is plaintext;
/// - a non-empty string under a key containing `password` or `secret`
///   (outside the schema paths and outside a Referenced object's `data`) is
///   refused as plaintext by key name when the 8.3.6 OpenAPI does not
///   describe the type, and only warned about when it does (the schema is
///   authoritative there).
///
/// `origin` is the source file label findings should carry.
pub fn scan_secrets(origin: &str, key: &ResourceKey, file_name: &str, node: &Node) -> SecretScan {
    if file_name != "config.json" {
        return scan_embedded(origin, node);
    }
    let mut walker = Walker {
        origin,
        rules: Some(Rules {
            schema: secret_paths(&key.module, &key.ty),
            key_names: true,
            described: openapi_describes(&key.module, &key.ty),
        }),
        scan: SecretScan::default(),
    };
    walker.walk(node, "", false, false);
    walker.scan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deploy::node;

    fn key(module: &str, ty: &str) -> ResourceKey {
        ResourceKey {
            module: module.to_string(),
            ty: ty.to_string(),
            name: Some("x".to_string()),
        }
    }

    fn scan(module: &str, ty: &str, json: &str) -> SecretScan {
        scan_secrets(
            "f/config.json",
            &key(module, ty),
            "config.json",
            &node::parse(json).unwrap(),
        )
    }

    #[test]
    fn embedded_nested_in_arrays_is_found_with_index_path() {
        let s = scan(
            "m",
            "t",
            r#"{"a":[{"b":{"type":"Embedded","data":{"ciphertext":"X"}}}]}"#,
        );
        assert_eq!(s.refused, ["f/config.json: a[0].b (embedded)"]);
    }

    #[test]
    fn embedded_without_object_data_is_not_embedded() {
        let s = scan("m", "t", r#"{"a":{"type":"Embedded","data":"x"}}"#);
        assert!(s.refused.is_empty());
    }

    #[test]
    fn embedded_at_a_root_is_reported_as_root() {
        let s = scan_embedded(
            "f.json",
            &node::parse(r#"{"type":"Embedded","data":{}}"#).unwrap(),
        );
        assert_eq!(s.refused, ["f.json: $ (embedded)"]);
    }

    #[test]
    fn schema_path_values() {
        let ok = scan("ignition", "database-connection", r#"{"password":null}"#);
        assert_eq!(ok, SecretScan::default());
        let ok = scan(
            "ignition",
            "database-connection",
            r#"{"password":{"type":"Referenced","data":{"providerName":"p","secretName":"s"}}}"#,
        );
        assert_eq!(ok, SecretScan::default());
        for bad in [r#""x""#, r#""""#, "1", "true", "[]", "{}"] {
            let s = scan(
                "ignition",
                "database-connection",
                &format!(r#"{{"password":{bad}}}"#),
            );
            assert_eq!(
                s.refused,
                ["f/config.json: password (plaintext)"],
                "value {bad}"
            );
        }
    }

    #[test]
    fn non_config_files_get_only_the_embedded_rule() {
        let node = node::parse(r#"{"password":"x"}"#).unwrap();
        let s = scan_secrets(
            "f/other.json",
            &key("com.cirruslink.mqtt.engine.gateway", "server"),
            "other.json",
            &node,
        );
        assert_eq!(s, SecretScan::default());
    }

    #[test]
    fn key_name_in_a_described_type_warns_by_path() {
        let s = scan(
            "ignition",
            "database-connection",
            r#"{"settings":{"mqttPassword":"CANARY"}}"#,
        );
        assert!(s.refused.is_empty());
        assert_eq!(s.warnings.len(), 1);
        assert!(s.warnings[0].contains("settings.mqttPassword"));
        assert!(!s.warnings[0].contains("CANARY"));
    }

    #[test]
    fn referenced_data_secret_name_is_a_name() {
        let s = scan(
            "com.cirruslink.mqtt.engine.gateway",
            "server",
            r#"{"password":{"type":"Referenced","data":{"providerName":"p","secretName":"s"}}}"#,
        );
        assert_eq!(s, SecretScan::default());
    }
}
