//! Strict YAML parsing for the declared tree (`manifest.yaml`,
//! `targets/<env>.yaml`).
//!
//! [`strict_parse`] is a PURE function of `(file label, bytes)`: it reads no
//! file, holds no global state, and parsing the same bytes twice yields
//! equal results or equal errors. Every defect becomes a
//! [`CoreError::DeployTreeInvalid`] whose message never echoes a value read
//! from the input.
//!
//! Position convention: `yaml_serde::Error::location()` is 1-BASED for both
//! line and column (pinned by `unknown_top_level_field_has_position`), and
//! the column counts the key start for unknown-field and wrong-type
//! errors, so [`DeployFinding`] positions are passed through unchanged. `yaml_serde` reports 1:1 for missing and
//! duplicate fields, which is not a real position; those findings carry
//! none and name only the file.
//!
//! [`DeployFinding`]: crate::error::DeployFinding

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::error::CoreError;

/// The only `schema_version` this `ign` reads.
pub const SCHEMA_VERSION: u32 = 1;

/// Largest YAML input accepted, in bytes (1 MiB). Checked before any parse.
pub const YAML_MAX_BYTES: usize = 1_048_576;

/// A `schema_version` that is exactly [`SCHEMA_VERSION`]; any other value
/// fails deserialization with an `unsupported schema_version` message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchemaVersion;

impl<'de> Deserialize<'de> for SchemaVersion {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match u32::deserialize(d)? {
            SCHEMA_VERSION => Ok(Self),
            v => Err(serde::de::Error::custom(format!(
                "unsupported schema_version {v}; this ign reads {SCHEMA_VERSION}"
            ))),
        }
    }
}

/// First-pass probe: reads only `schema_version`, WITHOUT
/// `deny_unknown_fields`, so an unknown key earlier in the file cannot
/// mask a version error.
#[derive(Deserialize)]
struct SchemaProbe {
    #[allow(dead_code)]
    schema_version: SchemaVersion,
}

/// Parse `bytes` (labelled `file` in errors) into `T`.
///
/// Guards, in order: size cap, carriage return, UTF-8, schema version
/// (first pass), then the strict typed parse (second pass; `T` is expected
/// to carry `deny_unknown_fields`).
pub fn strict_parse<T: DeserializeOwned>(file: &str, bytes: &[u8]) -> Result<T, CoreError> {
    if bytes.len() > YAML_MAX_BYTES {
        return Err(CoreError::deploy_tree_invalid(
            file,
            None,
            None,
            format!(
                "file is {} bytes; the YAML size cap is {YAML_MAX_BYTES} bytes",
                bytes.len()
            ),
        ));
    }
    if let Some(at) = bytes.iter().position(|b| *b == b'\r') {
        let (line, column) = position_of(bytes, at);
        return Err(CoreError::deploy_tree_invalid(
            file,
            Some(line),
            Some(column),
            "carriage return (0x0D); save the file with LF line endings",
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|e| {
        let (line, column) = position_of(bytes, e.valid_up_to());
        CoreError::deploy_tree_invalid(file, Some(line), Some(column), "file is not valid UTF-8")
    })?;
    yaml_serde::from_str::<SchemaProbe>(text).map_err(|e| {
        let mapped = map_error(file, &e);
        relocate_schema_error(file, text, mapped)
    })?;
    yaml_serde::from_str::<T>(text).map_err(|e| map_error(file, &e))
}

/// yaml_serde positions a `custom` error raised inside a value at the START
/// OF THE ENCLOSING MAPPING, not at the key. For the version error the key
/// is known, so point at it: the `schema_version:` line at column 1.
fn relocate_schema_error(file: &str, text: &str, err: CoreError) -> CoreError {
    let CoreError::DeployTreeInvalid(details) = &err else {
        return err;
    };
    let Some(finding) = details.findings.first() else {
        return err;
    };
    if !finding.message.starts_with("unsupported schema_version") {
        return err;
    }
    match text.lines().position(|l| l.starts_with("schema_version:")) {
        Some(i) => {
            CoreError::deploy_tree_invalid(file, Some(i + 1), Some(1), finding.message.clone())
        }
        None => err,
    }
}

/// 1-based `(line, column)` of byte offset `at` (column counted in bytes).
fn position_of(bytes: &[u8], at: usize) -> (usize, usize) {
    let before = &bytes[..at.min(bytes.len())];
    let line = before.iter().filter(|b| **b == b'\n').count() + 1;
    let line_start = before
        .iter()
        .rposition(|b| *b == b'\n')
        .map_or(0, |i| i + 1);
    (line, before.len() - line_start + 1)
}

fn map_error(file: &str, e: &yaml_serde::Error) -> CoreError {
    let text = e.to_string();
    let message = scrub_values(strip_position(&text));
    let positionless = ["missing field", "duplicate field"]
        .iter()
        .any(|p| message.starts_with(p) || message.contains(&format!(": {p}")));
    let (line, column) = if positionless {
        (None, None)
    } else {
        match e.location() {
            Some(loc) => (Some(loc.line()), Some(loc.column())),
            None => (None, None),
        }
    };
    CoreError::deploy_tree_invalid(file, line, column, message)
}

/// Drop yaml_serde's trailing ` at line N column M`, so the position is
/// reported once, by us.
fn strip_position(msg: &str) -> &str {
    match msg.rfind(" at line ") {
        Some(i) if msg[i..].contains(" column ") => &msg[..i],
        _ => msg,
    }
}

/// Remove every user-supplied scalar value serde or yaml_serde would echo
/// (T-21-10): a wrong-typed credential must never reach an error, an
/// envelope, or a log. Audited sources of an echoed value:
/// - `invalid type: <kind> <value>` and `invalid value: <kind> <value>`
///   (string, integer, floating point, boolean, character);
/// - `unknown variant `<value>`, expected ...` (any enum field);
/// - `deserializing nested enum in ...::<tag>`, which echoes a YAML tag.
///
/// Not echoed, so left alone: `unknown field `<key>`` (a key, kept so the
/// message is actionable), `missing field`, `duplicate field`,
/// `invalid length N`, and every libyaml scanner/parser message (static
/// text plus a position).
fn scrub_values(msg: &str) -> String {
    scrub_invalid(&scrub_variant(msg))
}

/// `unknown variant `<value>`, expected ...` becomes
/// `unknown variant, expected ...`. The value is raw (not escaped), so it
/// may itself contain a backtick or `, expected `; the real list is the
/// LAST `, expected ` because variant names are static.
fn scrub_variant(msg: &str) -> String {
    const LEAD: &str = "unknown variant ";
    if msg.contains("deserializing nested enum in ") {
        return "deserializing nested enum from YAML is not supported".to_string();
    }
    let Some(at) = msg.find(LEAD) else {
        return msg.to_string();
    };
    let head = &msg[..at + LEAD.len() - 1]; // keep "unknown variant", drop the space
    match msg.rfind(", expected ") {
        Some(end) if end >= at + LEAD.len() => format!("{head}{}", &msg[end..]),
        _ => head.to_string(),
    }
}

/// Remove the value from `invalid type: <kind> <value>` and
/// `invalid value: <kind> <value>` messages.
fn scrub_invalid(msg: &str) -> String {
    const LEADS: [&str; 2] = ["invalid type: ", "invalid value: "];
    const KINDS: [&str; 5] = [
        "string ",
        "integer ",
        "floating point ",
        "boolean ",
        "character ",
    ];
    for lead in LEADS {
        let Some(at) = msg.find(lead) else { continue };
        let after_lead = at + lead.len();
        let rest = &msg[after_lead..];
        for kind in KINDS {
            let Some(value_part) = rest.strip_prefix(kind) else {
                continue;
            };
            let value_len = quoted_len(value_part);
            if value_len == 0 {
                continue;
            }
            let head = &msg[..after_lead + kind.len() - 1]; // keep "string", drop the space
            let tail = &value_part[value_len..];
            return format!("{head}{tail}");
        }
    }
    msg.to_string()
}

/// Byte length of the leading quoted token (`"…"` with backslash escapes,
/// `` `…` `` or `'…'`), or 0 when `s` does not start with one.
fn quoted_len(s: &str) -> usize {
    let mut chars = s.char_indices();
    let Some((_, open)) = chars.next() else {
        return 0;
    };
    if !matches!(open, '"' | '`' | '\'') {
        return 0;
    }
    let mut escaped = false;
    for (i, c) in chars {
        if escaped {
            escaped = false;
        } else if c == '\\' && open == '"' {
            escaped = true;
        } else if c == open {
            return i + c.len_utf8();
        }
    }
    // Unterminated: scrub everything rather than leak a fragment.
    s.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CoreError;
    use std::time::{Duration, Instant};

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct Probe {
        schema_version: SchemaVersion,
        #[serde(default)]
        port: Option<u16>,
        #[serde(default)]
        nested: Option<Nested>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct Nested {
        #[serde(default)]
        a: Option<String>,
    }

    fn err(file: &str, yaml: &str) -> String {
        strict_parse::<Probe>(file, yaml.as_bytes())
            .expect_err("must fail")
            .to_string()
    }

    fn finding(e: CoreError) -> crate::error::DeployFinding {
        match e {
            CoreError::DeployTreeInvalid(d) => d.findings.into_iter().next().expect("one finding"),
            other => panic!("expected deploy_tree_invalid, got {other}"),
        }
    }

    fn fail(yaml: &str) -> crate::error::DeployFinding {
        finding(strict_parse::<Probe>("m.yaml", yaml.as_bytes()).expect_err("must fail"))
    }

    #[test]
    fn unknown_top_level_field_has_position() {
        let f = fail("schema_version: 1\nbogus: 2\n");
        assert_eq!((f.line, f.column), (Some(2), Some(1)));
        assert!(f.message.contains("bogus"), "{}", f.message);
        assert!(!f.message.contains(" at line "), "{}", f.message);
        assert_eq!(f.file, "m.yaml");
    }

    #[test]
    fn unknown_nested_field_has_position() {
        let f = fail("schema_version: 1\nnested:\n  a: x\n  extra: 1\n");
        assert_eq!((f.line, f.column), (Some(4), Some(3)));
        assert!(f.message.contains("extra"), "{}", f.message);
    }

    #[test]
    fn wrong_type_is_positioned_and_scrubbed() {
        let f = fail("schema_version: 1\nport: s3cr3t-canary\n");
        assert_eq!(f.line, Some(2));
        assert!(!f.message.contains("s3cr3t-canary"), "{}", f.message);
        let shown = err("m.yaml", "schema_version: 1\nport: s3cr3t-canary\n");
        assert!(!shown.contains("s3cr3t-canary"), "{shown}");

        // Escaped quotes inside the value cannot leak a fragment.
        let shown = err("m.yaml", "schema_version: 1\nport: 'a\"b\\\"c-canary2'\n");
        assert!(!shown.contains("canary2"), "{shown}");

        // Numeric and out-of-range values are scrubbed as well.
        let shown = err("m.yaml", "schema_version: 1\nport: 99999\n");
        assert!(!shown.contains("99999"), "{shown}");
    }

    #[test]
    fn enum_canary_is_scrubbed_but_variants_are_kept() {
        #[derive(Debug, Deserialize, PartialEq)]
        #[serde(rename_all = "lowercase")]
        enum Kind {
            Windows,
            Linux,
        }
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WithEnum {
            #[allow(dead_code)]
            schema_version: SchemaVersion,
            #[allow(dead_code)]
            kind: Kind,
        }
        let run = |yaml: &str| {
            let e = strict_parse::<WithEnum>("t.yaml", yaml.as_bytes()).expect_err("bad variant");
            (e.to_string(), finding(e))
        };

        let (shown, f) = run("schema_version: 1\nkind: s3cr3t-canary\n");
        assert!(!shown.contains("s3cr3t-canary"), "{shown}");
        assert_eq!(f.line, Some(2));
        assert!(shown.contains("unknown variant"), "{shown}");
        assert!(shown.contains("expected `windows` or `linux`"), "{shown}");

        // A value that imitates the message structure cannot leak a tail.
        let (shown, _) = run("schema_version: 1\nkind: 'evil`, expected `windows`-canary3 `x'\n");
        assert!(!shown.contains("canary3"), "{shown}");
        assert!(shown.contains("expected `windows` or `linux`"), "{shown}");

        // A tagged value's tag name is not echoed either.
        let (shown, _) = run("schema_version: 1\nkind: !tagcanary4 {a: 1}\n");
        assert!(!shown.contains("tagcanary4"), "{shown}");

        // Booleans and numbers in an enum slot are scrubbed too.
        let (shown, _) = run("schema_version: 1\nkind: 8675309\n");
        assert!(!shown.contains("8675309"), "{shown}");
    }

    #[test]
    fn wrong_schema_version_is_positioned() {
        let f = fail("schema_version: 2\n");
        assert_eq!(f.message, "unsupported schema_version 2; this ign reads 1");
        assert_eq!(f.line, Some(1));
        // The probe runs first, so an earlier unknown key cannot mask it.
        let f = fail("bogus: 1\nschema_version: 2\n");
        assert!(f.message.starts_with("unsupported schema_version 2"));
        assert_eq!(f.line, Some(2));
    }

    #[test]
    fn syntax_error_is_positioned() {
        let f = fail("schema_version: 1\nnested:\n\ta: x\n");
        assert_eq!(f.line, Some(3));
        assert!(f.column.is_some());
    }

    #[test]
    fn missing_and_duplicate_fields_name_file_only() {
        let missing = fail("port: 1\n");
        assert_eq!((missing.line, missing.column), (None, None));
        assert!(
            missing.message.contains("missing field"),
            "{}",
            missing.message
        );
        let shown = err("targets/prod.yaml", "port: 1\n");
        assert!(shown.contains("targets/prod.yaml: "), "{shown}");

        let dup = fail("schema_version: 1\nport: 1\nport: 2\n");
        assert_eq!((dup.line, dup.column), (None, None));
        assert!(dup.message.contains("duplicate field"), "{}", dup.message);
    }

    #[test]
    fn anchors_and_aliases_are_accepted() {
        let ok: Probe = strict_parse(
            "m.yaml",
            b"schema_version: 1\nnested: &n\n  a: x\nport: 80\n",
        )
        .expect("anchors parse");
        assert_eq!(ok.port, Some(80));
        // An alias to a scalar.
        #[derive(Debug, Deserialize)]
        struct Loose {
            #[allow(dead_code)]
            schema_version: SchemaVersion,
            a: String,
            b: String,
        }
        let ok: Loose =
            strict_parse("m.yaml", b"schema_version: 1\na: &x hello\nb: *x\n").expect("alias");
        assert_eq!((ok.a.as_str(), ok.b.as_str()), ("hello", "hello"));
    }

    #[test]
    fn alias_bomb_is_refused() {
        // Nine levels of nine aliases each: 9^9 leaves if expanded. yaml_serde
        // carries its own repetition limit, so no pre-parse alias guard is
        // needed; this pins that behaviour for the pinned 0.10 line.
        let mut yaml = String::from(
            "schema_version: 1\nl0: &l0 [\"x\",\"x\",\"x\",\"x\",\"x\",\"x\",\"x\",\"x\",\"x\"]\n",
        );
        for level in 1..=9 {
            let refs = vec![format!("*l{}", level - 1); 9].join(",");
            yaml.push_str(&format!("l{level}: &l{level} [{refs}]\n"));
        }
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Consumes {
            #[allow(dead_code)]
            schema_version: SchemaVersion,
            #[allow(dead_code)]
            l0: serde_json::Value,
            #[allow(dead_code)]
            l1: serde_json::Value,
            #[allow(dead_code)]
            l2: serde_json::Value,
            #[allow(dead_code)]
            l3: serde_json::Value,
            #[allow(dead_code)]
            l4: serde_json::Value,
            #[allow(dead_code)]
            l5: serde_json::Value,
            #[allow(dead_code)]
            l6: serde_json::Value,
            #[allow(dead_code)]
            l7: serde_json::Value,
            #[allow(dead_code)]
            l8: serde_json::Value,
            #[allow(dead_code)]
            l9: serde_json::Value,
        }
        let started = Instant::now();
        let r = strict_parse::<Consumes>("bomb.yaml", yaml.as_bytes());
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "bomb took {:?}",
            started.elapsed()
        );
        let shown = r.expect_err("the bomb must be refused").to_string();
        assert!(shown.contains("bomb.yaml"), "{shown}");
        assert!(shown.contains("repetition limit exceeded"), "{shown}");
        assert_eq!(
            CoreError::DEPLOY_SLUGS[0],
            strict_parse::<Consumes>("bomb.yaml", yaml.as_bytes())
                .expect_err("again")
                .code()
        );
    }

    #[test]
    fn size_cap_boundary() {
        // Exactly the cap parses: pad with a trailing comment.
        let head = b"schema_version: 1\n#";
        let mut at_cap = head.to_vec();
        at_cap.resize(YAML_MAX_BYTES, b'x');
        assert_eq!(at_cap.len(), YAML_MAX_BYTES);
        strict_parse::<Probe>("m.yaml", &at_cap).expect("exactly the cap parses");

        at_cap.push(b'x');
        let shown = strict_parse::<Probe>("m.yaml", &at_cap)
            .expect_err("over the cap")
            .to_string();
        assert!(shown.contains("m.yaml"), "{shown}");
        assert!(shown.contains("1048576"), "{shown}");
        assert!(shown.contains("1048577"), "{shown}");
    }

    #[test]
    fn carriage_return_is_refused_with_line() {
        let f = fail("schema_version: 1\r\n");
        assert_eq!((f.line, f.column), (Some(1), Some(18)));
        assert!(f.message.contains("0x0D"), "{}", f.message);
        let f = fail("schema_version: 1\nport: 1\r\n");
        assert_eq!(f.line, Some(2));
    }

    #[test]
    fn non_utf8_is_refused() {
        let e = strict_parse::<Probe>("m.yaml", b"schema_version: 1\n# \xff\xfe\n")
            .expect_err("invalid utf-8");
        let f = finding(e);
        assert!(f.message.contains("UTF-8"), "{}", f.message);
        assert_eq!(f.line, Some(2));
    }

    #[test]
    fn strict_parse_is_pure() {
        for yaml in [
            "schema_version: 1\nport: 80\n",
            "schema_version: 1\nbogus: 1\n",
            "schema_version: 9\n",
            "port: 1\n",
        ] {
            let a = strict_parse::<Probe>("m.yaml", yaml.as_bytes());
            let b = strict_parse::<Probe>("m.yaml", yaml.as_bytes());
            assert_eq!(format!("{a:?}"), format!("{b:?}"), "{yaml:?}");
        }
    }
}
