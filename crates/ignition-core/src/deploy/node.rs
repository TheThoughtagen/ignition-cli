//! Order-preserving JSON tree and the Gson-compatible writer (DRND-05).
//!
//! Gateway-written JSON is Gson pretty output. Render passes files through
//! as bytes; this module only decides whether a file *may be rewritten*
//! (uuid injection in 21-07) and gives the secret lint (21-08) a tree to
//! walk. A file is [`JsonClass::GatewayFormat`] when
//! `write(parse(bytes)) == bytes`, otherwise [`JsonClass::Opaque`] and render
//! never mutates it.
//!
//! ## The tree
//!
//! [`Node`] keeps object members in source order (never sorted: the spec's
//! "sorted keys" is wrong, ROADMAP constraint) and keeps every number,
//! `true`, `false` and `null` as its original lexeme ([`Node::Lit`]), so
//! `2.50`, `1.0E10`, `-0.0` and `18446744073709551616` survive unchanged.
//! `serde_json` runs with `raw_value` only; `preserve_order` and
//! `arbitrary_precision` stay off.
//!
//! ## The writer: Gson pretty, HTML-safe
//!
//! 2-space indent, `": "` between key and value, one member or element per
//! line, `{}` and `[]` for empty containers, no trailing newline. Strings
//! and keys are escaped as Gson's `JsonWriter` does with HTML-safe on
//! (`ESC(hhhh)` below is the six characters backslash, `u`, four lowercase
//! hex digits):
//!
//! | Character | Written as |
//! |---|---|
//! | `"` and `\` | backslash-escaped |
//! | TAB, BS, LF, CR, FF | backslash then `t`, `b`, `n`, `r`, `f` |
//! | other code points below U+0020 | `ESC(00hh)` |
//! | `<` `>` `&` `=` `'` | `ESC(003c)` `ESC(003e)` `ESC(0026)` `ESC(003d)` `ESC(0027)` |
//! | U+2028, U+2029 | `ESC(2028)`, `ESC(2029)` |
//! | everything else, DEL and non-ASCII included | raw UTF-8 |
//!
//! Raw UTF-8 for non-ASCII is decision CD4 (21-LIVE-CAPTURES.md): the 8.3.6
//! gateway wrote `Température ✓` unescaped.
//!
//! Errors are `String`s carrying serde_json's line/column text or a key
//! name, never a value (T-21-22).

use std::collections::BTreeSet;
use std::fmt;

use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use serde_json::value::RawValue;

/// Per-file size cap for a `.json` member (16 MiB). Larger files are refused
/// before any parse (T-21-19).
pub const JSON_MAX_BYTES: usize = 16_777_216;

/// Deepest container nesting [`parse`] accepts (serde_json's own default
/// recursion limit). Checked by a linear scan before the recursive build, so
/// hostile nesting can neither overflow the stack nor cost
/// O(depth x size) (T-21-19).
pub const MAX_DEPTH: usize = 128;

/// An order-preserving JSON document node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// Object members in source order; keys are unique within one object.
    Obj(Vec<(String, Node)>),
    /// Array elements.
    Arr(Vec<Node>),
    /// A decoded string value.
    Str(String),
    /// A number, `true`, `false` or `null`, as its source lexeme.
    Lit(String),
}

impl Node {
    /// The value of member `key` when this is an object that has it.
    pub fn get(&self, key: &str) -> Option<&Node> {
        match self {
            Node::Obj(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The decoded text when this is a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Node::Str(s) => Some(s),
            _ => None,
        }
    }
}

/// Whether render may rewrite a JSON file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonClass {
    /// `write(parse(bytes)) == bytes`: rewritable without changing a byte.
    GatewayFormat,
    /// Parses, but is not in the gateway's writer format: never mutated.
    Opaque,
}

/// Object members, refusing a repeated key (T-21-20).
struct Members(Vec<(String, Box<RawValue>)>);

impl<'de> Deserialize<'de> for Members {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Members;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a JSON object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Members, A::Error> {
                let mut seen: BTreeSet<String> = BTreeSet::new();
                let mut out = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, Box<RawValue>>()? {
                    if !seen.insert(key.clone()) {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate key `{key}` in one object"
                        )));
                    }
                    out.push((key, value));
                }
                Ok(Members(out))
            }
        }
        deserializer.deserialize_map(V)
    }
}

/// Parse one JSON document into a [`Node`].
///
/// The whole document is validated once (syntax and trailing content), then
/// its nesting depth is measured with a linear scan ([`MAX_DEPTH`]) before
/// the recursive build, so the build depth is bounded. Duplicate keys within
/// one object are an error.
pub fn parse(src: &str) -> Result<Node, String> {
    let raw: Box<RawValue> = serde_json::from_str(src).map_err(|e| e.to_string())?;
    if max_depth(raw.get().as_bytes()) > MAX_DEPTH {
        return Err(format!("nesting deeper than {MAX_DEPTH} levels"));
    }
    build(raw.get())
}

/// Deepest `{`/`[` nesting of already-validated JSON text. String contents
/// (including escaped quotes) are skipped.
fn max_depth(bytes: &[u8]) -> usize {
    let (mut depth, mut deepest, mut in_string, mut escaped) = (0usize, 0usize, false, false);
    for &b in bytes {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

fn build(text: &str) -> Result<Node, String> {
    let t = text.trim();
    match t.as_bytes().first() {
        Some(b'{') => {
            let Members(members) = serde_json::from_str(t).map_err(|e| e.to_string())?;
            let mut out = Vec::with_capacity(members.len());
            for (key, value) in members {
                out.push((key, build(value.get())?));
            }
            Ok(Node::Obj(out))
        }
        Some(b'[') => {
            let items: Vec<Box<RawValue>> = serde_json::from_str(t).map_err(|e| e.to_string())?;
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(build(item.get())?);
            }
            Ok(Node::Arr(out))
        }
        Some(b'"') => Ok(Node::Str(
            serde_json::from_str::<String>(t).map_err(|e| e.to_string())?,
        )),
        _ => Ok(Node::Lit(t.to_string())),
    }
}

/// Write `node` in the gateway's Gson pretty format (see the module doc).
pub fn write(node: &Node) -> String {
    let mut out = String::new();
    write_node(node, 0, &mut out);
    out
}

fn indent(depth: usize, out: &mut String) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn write_node(node: &Node, depth: usize, out: &mut String) {
    match node {
        Node::Lit(lexeme) => out.push_str(lexeme),
        Node::Str(s) => write_string(s, out),
        Node::Obj(members) if members.is_empty() => out.push_str("{}"),
        Node::Arr(items) if items.is_empty() => out.push_str("[]"),
        Node::Obj(members) => {
            out.push_str("{\n");
            for (i, (key, value)) in members.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                indent(depth + 1, out);
                write_string(key, out);
                out.push_str(": ");
                write_node(value, depth + 1, out);
            }
            out.push('\n');
            indent(depth, out);
            out.push('}');
        }
        Node::Arr(items) => {
            out.push_str("[\n");
            for (i, value) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                indent(depth + 1, out);
                write_node(value, depth + 1, out);
            }
            out.push('\n');
            indent(depth, out);
            out.push(']');
        }
    }
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{c}' => out.push_str("\\f"),
            '<' | '>' | '&' | '=' | '\'' | '\u{2028}' | '\u{2029}' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Classify a `.json` member's bytes and return its parsed tree.
///
/// Refuses (as `Err`): more than [`JSON_MAX_BYTES`] bytes, invalid UTF-8, a
/// leading byte-order mark, and anything [`parse`] refuses (syntax, trailing
/// content, nesting beyond [`MAX_DEPTH`], a repeated key).
pub fn classify(bytes: &[u8]) -> Result<(JsonClass, Node), String> {
    if bytes.len() > JSON_MAX_BYTES {
        return Err(format!(
            "JSON file is {} bytes, over the {JSON_MAX_BYTES}-byte cap",
            bytes.len()
        ));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|e| format!("not valid UTF-8 (valid up to byte {})", e.valid_up_to()))?;
    if text.starts_with('\u{feff}') {
        return Err("starts with a byte-order mark".to_string());
    }
    let node = parse(text)?;
    let class = if write(&node).as_bytes() == bytes {
        JsonClass::GatewayFormat
    } else {
        JsonClass::Opaque
    };
    Ok((class, node))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// ESC(hhhh): backslash, `u`, four lowercase hex digits.
    fn esc(hex: &str) -> String {
        format!("\\u{hex}")
    }

    #[test]
    fn gson_escapes() {
        let s = "<>&='\"\\\u{2028}\u{2029}\t\n\r\u{8}\u{c}\u{1}";
        let got = write(&Node::Str(s.to_string()));
        let want = format!(
            "\"{}{}{}{}{}\\\"\\\\{}{}\\t\\n\\r\\b\\f{}\"",
            esc("003c"),
            esc("003e"),
            esc("0026"),
            esc("003d"),
            esc("0027"),
            esc("2028"),
            esc("2029"),
            esc("0001"),
        );
        assert_eq!(got, want);
        // keys are escaped the same way
        let key = write(&Node::Obj(vec![("a<b".into(), Node::Lit("1".into()))]));
        assert_eq!(key, format!("{{\n  \"a{}b\": 1\n}}", esc("003c")));
    }

    #[test]
    fn non_ascii_follows_cd4() {
        // CD4: 8.3.6 wrote `Température ✓` as raw UTF-8
        assert_eq!(write(&Node::Str("é✓".into())), "\"é✓\"");
        assert_eq!(write(&Node::Str("\u{7f}".into())), "\"\u{7f}\"");
    }

    #[test]
    fn pretty_layout() {
        let n = parse(r#"{"a":{"b":[1,{"c":[]},{}],"d":{}},"e":[],"f":"x"}"#).unwrap();
        let want = "{\n  \"a\": {\n    \"b\": [\n      1,\n      {\n        \"c\": []\n      },\n      {}\n    ],\n    \"d\": {}\n  },\n  \"e\": [],\n  \"f\": \"x\"\n}";
        assert_eq!(write(&n), want);
        assert!(!write(&n).ends_with('\n'));
    }

    #[test]
    fn member_order_preserved() {
        let n = parse(r#"{"b":1,"a":2}"#).unwrap();
        assert_eq!(write(&n), "{\n  \"b\": 1,\n  \"a\": 2\n}");
        match n {
            Node::Obj(m) => assert_eq!(m[0].0, "b"),
            _ => panic!("object"),
        }
    }

    #[test]
    fn number_lexemes_preserved() {
        let src = "[\n  2.50,\n  1.0E10,\n  -0.0,\n  18446744073709551616\n]";
        let n = parse(src).unwrap();
        assert_eq!(write(&n), src);
        let (class, _) = classify(src.as_bytes()).unwrap();
        assert_eq!(class, JsonClass::GatewayFormat);
    }

    #[test]
    fn duplicate_key_refused() {
        let err = parse(r#"{"a":1,"a":2}"#).unwrap_err();
        assert!(err.contains("duplicate key `a`"), "{err}");
        // nested too
        let err = parse(r#"{"x":{"k":1,"k":null}}"#).unwrap_err();
        assert!(err.contains("`k`"), "{err}");
    }

    #[test]
    fn classify_boundaries() {
        let mut at_cap = b"{}".to_vec();
        at_cap.resize(JSON_MAX_BYTES, b' ');
        let (class, _) = classify(&at_cap).expect("exactly the cap classifies");
        assert_eq!(class, JsonClass::Opaque);

        at_cap.push(b' ');
        let err = classify(&at_cap).unwrap_err();
        assert!(
            err.contains("16777217") && err.contains("16777216"),
            "{err}"
        );

        assert!(classify(&[b'{', 0xff, b'}']).unwrap_err().contains("UTF-8"));
        assert!(
            classify("\u{feff}{}".as_bytes())
                .unwrap_err()
                .contains("byte-order mark")
        );
        assert!(classify(b"{} x").is_err());
        assert!(classify(b"{\"a\":").is_err());
        let deep = format!("{}{}", "[".repeat(200), "]".repeat(200));
        assert!(classify(deep.as_bytes()).unwrap_err().contains("nesting"));
        let at_limit = format!("{}{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
        assert!(classify(at_limit.as_bytes()).is_ok());
        // brackets inside strings do not count
        assert!(parse(&format!("[\"{}\\\"{}\"]", "[".repeat(300), "{".repeat(300))).is_ok());
    }

    #[test]
    fn classify_trailing_newline_is_opaque() {
        let doc = "{\n  \"a\": 1\n}";
        assert_eq!(
            classify(doc.as_bytes()).unwrap().0,
            JsonClass::GatewayFormat
        );
        let with_nl = format!("{doc}\n");
        assert_eq!(classify(with_nl.as_bytes()).unwrap().0, JsonClass::Opaque);
    }

    #[test]
    fn errors_never_echo_values() {
        let err = parse(r#"{"a":"SECRETVALUE" "b":1}"#).unwrap_err();
        assert!(!err.contains("SECRETVALUE"), "{err}");
    }

    fn arb_string() -> impl Strategy<Value = String> {
        prop::collection::vec(
            prop::sample::select(vec![
                'a', 'Z', '0', ' ', '<', '>', '&', '=', '\'', '"', '\\', '\t', '\n', 'é',
                '\u{2028}',
            ]),
            0..8,
        )
        .prop_map(|v| v.into_iter().collect())
    }

    fn arb_node() -> impl Strategy<Value = Node> {
        let leaf = prop_oneof![
            arb_string().prop_map(Node::Str),
            prop::sample::select(vec![
                "2.50",
                "1.0E10",
                "-0.0",
                "18446744073709551616",
                "0",
                "-1",
                "true",
                "false",
                "null"
            ])
            .prop_map(|s| Node::Lit(s.to_string())),
        ];
        leaf.prop_recursive(4, 48, 4, |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..4).prop_map(Node::Arr),
                prop::collection::vec((arb_string(), inner), 0..4).prop_map(|members| {
                    let mut seen = BTreeSet::new();
                    Node::Obj(
                        members
                            .into_iter()
                            .filter(|(k, _)| seen.insert(k.clone()))
                            .collect(),
                    )
                }),
            ]
        })
    }

    proptest! {
        #[test]
        fn writer_idempotence(n in arb_node()) {
            let text = write(&n);
            let back = parse(&text).unwrap();
            prop_assert_eq!(&back, &n);
            prop_assert_eq!(write(&back), text.clone());
            prop_assert_eq!(classify(text.as_bytes()).unwrap().0, JsonClass::GatewayFormat);
        }
    }
}
