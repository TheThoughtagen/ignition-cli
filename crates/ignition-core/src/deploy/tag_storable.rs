//! Owned tag properties the gateway would not store as written (8.3.6).
//!
//! A `tags/import` stores some values in another form than the body holds:
//! a UDT parameter keeps its `dataType` only for `Integer`, `Float`,
//! `String` and `DateTime` (anything else becomes `Integer`, and a value
//! that is not a number is dropped), and an AtomicTag `value` is converted
//! to the tag's `dataType` (a string on an `Int4` is parsed or dropped, a
//! number on a `Boolean` is read the wrong way round, a fraction on an
//! integer type is truncated). The run's verify then reads back what the
//! repo did not write and goes red after the writes. [`check_storable`]
//! finds such properties in the repo before any contact, so `--check` and
//! the apply refuse `tags_not_storable` instead.
//!
//! Not findings: an AtomicTag `value: null` and an integer-valued number on
//! a `Float4`/`Float8` (the comparison treats both as equal to what the
//! gateway stores, [`super::tag_canon`]), a binding object
//! (`{"bindType": ...}`), and any object or array value. An instance member
//! override that names no `dataType` is not judged: its type is the UDT's.
//!
//! Names, property paths and fixed hints only, never a value. Pure.

use serde_json::Value;

use super::native_tags::NativeProvider;
use crate::error::NotStorableTag;

/// Every property of `provider`'s nodes the gateway would not store as
/// written, in node path order, then property order within a node.
pub fn check_storable(provider: &NativeProvider) -> Vec<NotStorableTag> {
    let mut out = Vec::new();
    for (path, node) in &provider.nodes {
        let mut found = Vec::new();
        walk(&node.json, "", &mut found);
        found.sort_by(|a: &(String, String), b| a.0.as_bytes().cmp(b.0.as_bytes()));
        out.extend(found.into_iter().map(|(property, hint)| NotStorableTag {
            path: format!("[{}]{path}", provider.name),
            property,
            hint,
        }));
    }
    out
}

/// The UDT parameter data types the gateway keeps as written.
const PARAMETER_TYPES: [&str; 4] = ["Integer", "Float", "String", "DateTime"];

/// Check the object `v` (a node, or a member at property prefix `at`) and
/// every member below it in `tags`, pushing `(property, hint)`.
fn walk(v: &Value, at: &str, out: &mut Vec<(String, String)>) {
    let tag_type = v.get("tagType").and_then(Value::as_str);
    if matches!(tag_type, Some("UdtType" | "UdtInstance"))
        && let Some(params) = v.get("parameters").and_then(Value::as_object)
    {
        for (name, entry) in params {
            let prop = format!("{at}parameters.{name}");
            if let Some((key, hint)) = parameter_problem(entry) {
                out.push((format!("{prop}.{key}"), hint));
            }
        }
    }
    if tag_type == Some("AtomicTag")
        && let Some(data_type) = v.get("dataType").and_then(Value::as_str)
        && let Some(value) = v.get("value")
        && let Some(hint) = atomic_problem(data_type, value)
    {
        out.push((format!("{at}value"), hint));
    }
    for member in v
        .get("tags")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(name) = member.get("name").and_then(Value::as_str) {
            walk(member, &format!("{at}tags.{name}."), out);
        }
    }
}

/// A value nothing here judges: absent from the store's point of view
/// (`null`), a binding, or any object or array.
fn not_judged(value: &Value) -> bool {
    matches!(value, Value::Null | Value::Object(_) | Value::Array(_))
}

/// A number with a fraction.
fn fractional(value: &Value) -> bool {
    value.as_f64().is_some_and(|f| f.fract() != 0.0)
}

/// What is wrong with one UDT parameter entry: the key (`dataType` or
/// `value`) and the hint.
fn parameter_problem(entry: &Value) -> Option<(&'static str, String)> {
    let data_type = entry.get("dataType").and_then(Value::as_str);
    let Some(data_type) = data_type.filter(|dt| PARAMETER_TYPES.contains(dt)) else {
        let hint = match data_type {
            None => "no dataType: the gateway stores Integer and drops a value that is not a \
                     number; set dataType to Integer, Float, String or DateTime"
                .to_string(),
            Some("Boolean") => "dataType Boolean: the gateway stores Integer 0/1; write \
                                dataType Integer with value 0 or 1"
                .to_string(),
            Some(other) => format!(
                "dataType {other}: the gateway stores Integer (a fraction is truncated, a \
                 value that is not a number dropped); use Integer, Float, String or DateTime"
            ),
        };
        return Some(("dataType", hint));
    };
    let value = entry.get("value")?;
    if not_judged(value) {
        return None;
    }
    let hint = match (data_type, value) {
        ("Integer", Value::Number(_)) if fractional(value) => {
            "Integer with a fraction: the gateway truncates it; write an integer or use \
             dataType Float"
        }
        ("Integer" | "Float", Value::String(_)) => {
            "a string on a numeric parameter: the gateway converts or drops it; write a number"
        }
        ("Integer" | "Float", Value::Bool(_)) => {
            "a boolean on a numeric parameter: the gateway stores 0/1; write the number"
        }
        ("DateTime", Value::String(_)) => {
            "a string on DateTime: the gateway stores epoch milliseconds; write the number"
        }
        ("String", Value::Number(_) | Value::Bool(_)) => {
            "a non-string on String: the gateway stores it as text; quote it"
        }
        _ => return None,
    };
    Some(("value", hint.to_string()))
}

/// What is wrong with an AtomicTag `value` of `data_type`, as a hint.
fn atomic_problem(data_type: &str, value: &Value) -> Option<String> {
    if not_judged(value) {
        return None;
    }
    let integer = matches!(data_type, "Int1" | "Int2" | "Int4" | "Int8");
    let float = matches!(data_type, "Float4" | "Float8");
    let hint = match value {
        Value::String(_) if integer || float => format!(
            "a string value on {data_type}: the gateway parses it or drops it (an \
             expression in `value` is never evaluated); write a number or drop `value`"
        ),
        Value::Bool(_) if integer || float => {
            format!("a boolean value on {data_type}: the gateway stores 0/1; write the number")
        }
        Value::Number(_) if integer && fractional(value) => {
            format!("a fraction on {data_type}: the gateway truncates it; write an integer")
        }
        Value::Number(_) | Value::String(_) if data_type == "Boolean" => {
            "a non-boolean value on Boolean: the gateway converts it (1 is stored as false); \
             write true or false"
                .to_string()
        }
        Value::Number(_) | Value::Bool(_) if data_type == "String" => {
            "a non-string value on String: the gateway stores it as text; quote it".to_string()
        }
        Value::String(_) if data_type == "DateTime" => {
            "a string value on DateTime: the gateway stores epoch milliseconds; write the \
             number"
                .to_string()
        }
        _ => return None,
    };
    Some(hint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deploy::native_tags::{NativeNode, NodeKind};
    use serde_json::json;
    use std::collections::BTreeMap;

    fn provider(nodes: &[(&str, Value)]) -> NativeProvider {
        NativeProvider {
            name: "P".to_string(),
            nodes: nodes
                .iter()
                .map(|(path, json)| {
                    (
                        (*path).to_string(),
                        NativeNode {
                            path: (*path).to_string(),
                            kind: NodeKind::Tag,
                            json: json.clone(),
                            file: format!("tags/P/{path}.json"),
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>(),
            groups: None,
        }
    }

    /// The `(path, property)` pairs [`check_storable`] reports for one node.
    fn found(path: &str, node: Value) -> Vec<(String, String)> {
        check_storable(&provider(&[(path, node)]))
            .into_iter()
            .map(|f| (f.path, f.property))
            .collect()
    }

    fn atomic(data_type: &str, value: Value) -> Value {
        json!({"tagType":"AtomicTag","valueSource":"memory","dataType":data_type,"value":value})
    }

    fn param(data_type: Option<&str>, value: Option<Value>) -> Value {
        let mut entry = serde_json::Map::new();
        if let Some(dt) = data_type {
            entry.insert("dataType".into(), json!(dt));
        }
        if let Some(v) = value {
            entry.insert("value".into(), v);
        }
        json!({"tagType":"UdtType","parameters":{"Enabled": Value::Object(entry)}})
    }

    #[test]
    fn udt_parameter_data_types_outside_the_four_are_refused() {
        for dt in ["Integer", "Float", "String", "DateTime"] {
            assert!(
                found("_types_/Scanner", param(Some(dt), None)).is_empty(),
                "{dt}"
            );
        }
        for dt in ["Boolean", "Int4", "Float8", "Double", "Long", "Text"] {
            assert_eq!(
                found("_types_/Scanner", param(Some(dt), Some(json!(1)))),
                [(
                    "[P]_types_/Scanner".to_string(),
                    "parameters.Enabled.dataType".to_string()
                )],
                "{dt}"
            );
        }
        assert_eq!(
            found("_types_/Scanner", param(None, Some(json!(1)))),
            [(
                "[P]_types_/Scanner".to_string(),
                "parameters.Enabled.dataType".to_string()
            )]
        );
        // An instance's parameters, and a nested instance member's.
        let instance = json!({"tagType":"UdtInstance","typeId":"Scanner",
            "parameters":{"Enabled":{"dataType":"Boolean","value":false}},
            "tags":[{"name":"Sub","tagType":"UdtInstance","typeId":"Other",
                "parameters":{"Mode":{"value":3}}}]});
        assert_eq!(
            found("F/I", instance),
            [
                (
                    "[P]F/I".to_string(),
                    "parameters.Enabled.dataType".to_string()
                ),
                (
                    "[P]F/I".to_string(),
                    "tags.Sub.parameters.Mode.dataType".to_string()
                ),
            ]
        );
    }

    #[test]
    fn the_boolean_hint_names_what_the_gateway_stores() {
        let findings = check_storable(&provider(&[(
            "_types_/Scanner",
            param(Some("Boolean"), Some(json!(true))),
        )]));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].hint.contains("Integer"), "{}", findings[0].hint);
        assert!(findings[0].hint.contains("0/1"), "{}", findings[0].hint);
    }

    #[test]
    fn udt_parameter_values_the_gateway_rewrites_are_refused() {
        let bad = [
            ("Integer", json!(1.5)),
            ("Integer", json!("5")),
            ("Integer", json!(true)),
            ("Float", json!("1.5")),
            ("Float", json!(false)),
            ("DateTime", json!("2026-01-01")),
            ("String", json!(5)),
            ("String", json!(true)),
        ];
        for (dt, value) in bad {
            assert_eq!(
                found("_types_/Scanner", param(Some(dt), Some(value.clone()))),
                [(
                    "[P]_types_/Scanner".to_string(),
                    "parameters.Enabled.value".to_string()
                )],
                "{dt} {value}"
            );
        }
        let good = [
            ("Integer", json!(5)),
            ("Integer", json!(5.0)),
            ("Integer", json!(-2)),
            ("Float", json!(1.5)),
            ("Float", json!(2)),
            ("DateTime", json!(1_767_225_600_000_i64)),
            ("String", json!("x")),
            ("String", json!("")),
            ("Integer", Value::Null),
            (
                "String",
                json!({"bindType":"parameter","binding":"{Other}"}),
            ),
            (
                "Integer",
                json!({"bindType":"parameter","binding":"{Other}"}),
            ),
        ];
        for (dt, value) in good {
            assert!(
                found("_types_/Scanner", param(Some(dt), Some(value.clone()))).is_empty(),
                "{dt} {value}"
            );
        }
        // A bad dataType is one finding; its value is not judged again.
        assert_eq!(
            found("_types_/Scanner", param(Some("Boolean"), Some(json!("x")))).len(),
            1
        );
    }

    #[test]
    fn atomic_values_the_gateway_rewrites_are_refused() {
        let bad = [
            ("Int4", json!("5")),
            ("Int4", json!("{PollSec}")),
            ("Int1", json!(true)),
            ("Int8", json!(5.7)),
            ("Int2", json!(0.5)),
            ("Float4", json!("1.5")),
            ("Float8", json!(true)),
            ("Boolean", json!(1)),
            ("Boolean", json!("true")),
            ("String", json!(5)),
            ("String", json!(false)),
            ("DateTime", json!("2026-01-01")),
        ];
        for (dt, value) in bad {
            assert_eq!(
                found("F/T", atomic(dt, value.clone())),
                [("[P]F/T".to_string(), "value".to_string())],
                "{dt} {value}"
            );
        }
        let good = [
            ("Int4", json!(5)),
            ("Int4", json!(5.0)),
            ("Int8", json!(-3)),
            ("Float4", json!(0)),
            ("Float8", json!(2.5)),
            ("Boolean", json!(false)),
            ("String", json!("5")),
            ("DateTime", json!(1_767_225_600_000_i64)),
            ("Int4", Value::Null),
            ("Boolean", Value::Null),
            ("Int4", json!({"bindType":"parameter","binding":"{Rate}"})),
            ("DataSet", json!("anything")),
            ("Document", json!({"a": 1})),
            ("Int4Array", json!([1, 2])),
        ];
        for (dt, value) in good {
            assert!(
                found("F/T", atomic(dt, value.clone())).is_empty(),
                "{dt} {value}"
            );
        }
        // Any valueSource: an expression tag's `value` is stored the same way.
        let expr = json!({"tagType":"AtomicTag","valueSource":"expr","dataType":"Int4",
            "expression":"{PollSec}","value":"{PollSec}"});
        assert_eq!(
            found("F/T", expr),
            [("[P]F/T".to_string(), "value".to_string())]
        );
        // No `value` at all is nothing to store.
        assert!(found("F/T", json!({"tagType":"AtomicTag","dataType":"Int4"})).is_empty());
    }

    #[test]
    fn members_are_walked_and_named() {
        let ty = json!({"tagType":"UdtType","tags":[
            {"name":"Speed","tagType":"AtomicTag","dataType":"Float4","value":"0"},
            {"name":"Grp","tagType":"Folder","tags":[
                {"name":"On","tagType":"AtomicTag","dataType":"Boolean","value":1}]},
            {"name":"Ok","tagType":"AtomicTag","dataType":"Int4","value":3}]});
        assert_eq!(
            found("_types_/Scanner", ty),
            [
                (
                    "[P]_types_/Scanner".to_string(),
                    "tags.Grp.tags.On.value".to_string()
                ),
                (
                    "[P]_types_/Scanner".to_string(),
                    "tags.Speed.value".to_string()
                ),
            ]
        );
        // An instance member override without a dataType is not judged.
        let inst = json!({"tagType":"UdtInstance","typeId":"Scanner","tags":[
            {"name":"Speed","tagType":"AtomicTag","value":"0"}]});
        assert!(found("F/I", inst).is_empty());
    }

    #[test]
    fn findings_carry_no_value() {
        let findings = check_storable(&provider(&[(
            "F/T",
            atomic("Int4", json!("secret-sentinel")),
        )]));
        assert_eq!(findings.len(), 1);
        let text = format!(
            "{} {} {}",
            findings[0].path, findings[0].property, findings[0].hint
        );
        assert!(!text.contains("sentinel"), "{text}");
    }
}
