//! Fixture guard for `tests/fixtures/deploy/` (Phase 21, plan 21-01).
//!
//! The deploy corpus is a snapshot of real gateway config, and
//! `ignition-cli` is a PUBLIC repository. Every file under
//! `fixtures/deploy/` (the sanitized corpus today, captured and repo
//! fixtures later) must pass these scanners before it can be committed:
//!
//! - S1 every string member of an Embedded secret `data` object carries
//!   the `SANITIZED` marker,
//! - S2 every Entra tenant or client GUID is a fake (`00000000-0000-4000-8000-...`),
//! - S3 every IPv4 literal is loopback, multicast, unspecified or RFC 5737,
//! - S4 no tenant mail host appears in a JSON string,
//! - S5 no text fixture holds a carriage return (git for Windows would
//!   rewrite it at checkout and break the byte-identity gates).
//!
//! Scanners return findings that name the file and the rule, never the
//! offending value, so a failing run cannot itself leak a secret.

use std::path::{Path, PathBuf};

use serde_json::Value;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/deploy");
const CORPUS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/deploy/corpus");
const GITATTRIBUTES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../.gitattributes");

const TEXT_EXTENSIONS: &[&str] = &[
    "json",
    "css",
    "yaml",
    "yml",
    "txt",
    "md",
    "xml",
    "py",
    "csv",
    "properties",
    "html",
    "js",
    "svg",
];

const MARKER: &str = "SANITIZED";
const FAKE_GUID_PREFIX: &str = "00000000-0000-4000-8000-";

// ---- Walking ----

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read fixture dir {}: {e}", dir.display()));
    for entry in entries {
        let entry = entry.expect("fixture dir entry");
        let path = entry.path();
        let kind = entry.file_type().expect("fixture file type");
        if kind.is_dir() {
            walk(&path, out);
        } else if kind.is_file() {
            out.push(path);
        }
    }
}

/// Every regular file under `root`, sorted (directory order is not
/// portable, so findings stay in a stable order on every OS).
fn files_under(root: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(Path::new(root), &mut out);
    out.sort();
    out
}

fn rel(path: &Path) -> String {
    path.strip_prefix(FIXTURES)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn ext(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn is_text(path: &Path) -> bool {
    TEXT_EXTENSIONS.contains(&ext(path).as_str())
}

fn json_files() -> Vec<(String, Value)> {
    files_under(FIXTURES)
        .into_iter()
        .filter(|p| ext(p) == "json")
        .map(|p| {
            let bytes = std::fs::read(&p).expect("read fixture json");
            let value = serde_json::from_slice::<Value>(&bytes)
                .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", rel(&p)));
            (rel(&p), value)
        })
        .collect()
}

// ---- Scanners ----

fn scan_embedded(value: &Value, path: &str) -> Vec<String> {
    let mut findings = Vec::new();
    embedded_walk(value, path, &mut findings);
    findings
}

fn embedded_walk(value: &Value, path: &str, findings: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            let is_embedded = map.get("type").and_then(Value::as_str) == Some("Embedded");
            if is_embedded {
                if let Some(Value::Object(data)) = map.get("data") {
                    for (key, member) in data {
                        if let Value::String(s) = member {
                            if !s.starts_with(MARKER) {
                                findings.push(format!(
                                    "{path}: Embedded secret member `{key}` lacks the {MARKER} marker"
                                ));
                            }
                        }
                    }
                }
            }
            for member in map.values() {
                embedded_walk(member, path, findings);
            }
        }
        Value::Array(items) => {
            for item in items {
                embedded_walk(item, path, findings);
            }
        }
        _ => {}
    }
}

fn ipv4_allowed(o: [u16; 4]) -> bool {
    o[0] == 127
        || o == [0, 0, 0, 0]
        || (224..=239).contains(&o[0])
        || (o[0] == 192 && o[1] == 0 && o[2] == 2)
        || (o[0] == 198 && o[1] == 51 && o[2] == 100)
        || (o[0] == 203 && o[1] == 0 && o[2] == 113)
}

/// Dotted-quad scanner over runs of digits and dots; no regex crate.
/// A run counts as an address only when it splits into exactly four
/// 1-3 digit decimal parts each at most 255.
fn scan_ipv4(text: &str, path: &str) -> Vec<String> {
    let mut findings = Vec::new();
    let mut run = String::new();
    let flush = |run: &mut String, findings: &mut Vec<String>| {
        let trimmed = run.trim_matches('.');
        let parts: Vec<&str> = trimmed.split('.').collect();
        if parts.len() == 4
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.len() <= 3 && p.bytes().all(|b| b.is_ascii_digit()))
        {
            let nums: Vec<u16> = parts.iter().filter_map(|p| p.parse().ok()).collect();
            if nums.len() == 4 && nums.iter().all(|n| *n <= 255) {
                let octets = [nums[0], nums[1], nums[2], nums[3]];
                if !ipv4_allowed(octets) {
                    findings.push(format!(
                        "{path}: IPv4 literal outside the allowed reserved ranges"
                    ));
                }
            }
        }
        run.clear();
    };
    for c in text.chars() {
        if c.is_ascii_digit() || c == '.' {
            run.push(c);
        } else if !run.is_empty() {
            flush(&mut run, &mut findings);
        }
    }
    if !run.is_empty() {
        flush(&mut run, &mut findings);
    }
    findings
}

/// Every 8-4-4-4-12 hex GUID inside `s`.
fn find_guids(s: &str) -> Vec<&str> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    if b.len() < 36 {
        return out;
    }
    let dash_at = [8usize, 13, 18, 23];
    for start in 0..=(b.len() - 36) {
        let window = &b[start..start + 36];
        let ok = window.iter().enumerate().all(|(i, c)| {
            if dash_at.contains(&i) {
                *c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        });
        let bounded = (start == 0 || !b[start - 1].is_ascii_hexdigit())
            && (start + 36 == b.len() || !b[start + 36].is_ascii_hexdigit());
        if ok && bounded {
            out.push(&s[start..start + 36]);
        }
    }
    out
}

fn scan_entra_guids(value: &Value, path: &str) -> Vec<String> {
    let mut findings = Vec::new();
    entra_walk(value, path, &mut findings);
    findings
}

fn entra_walk(value: &Value, path: &str, findings: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, member) in map {
                if key == "clientId" || key == "tenantId" {
                    if let Value::String(s) = member {
                        for g in find_guids(s) {
                            if !g.starts_with(FAKE_GUID_PREFIX) {
                                findings.push(format!("{path}: non-fake GUID in `{key}`"));
                            }
                        }
                    }
                }
                entra_walk(member, path, findings);
            }
        }
        Value::Array(items) => {
            for item in items {
                entra_walk(item, path, findings);
            }
        }
        Value::String(s) => {
            if s.contains("login.microsoftonline.com/") {
                for g in find_guids(s) {
                    if !g.starts_with(FAKE_GUID_PREFIX) {
                        findings.push(format!("{path}: non-fake GUID in an Entra login URL"));
                    }
                }
            }
        }
        _ => {}
    }
}

fn scan_mail_hosts(text: &str, path: &str) -> Vec<String> {
    let lower = text.to_ascii_lowercase();
    let mut findings = Vec::new();
    for needle in ["mail.protection.outlook.com", "onmicrosoft.com"] {
        if lower.contains(needle) {
            findings.push(format!("{path}: tenant mail host `*.{needle}`"));
        }
    }
    findings
}

fn scan_carriage_return(bytes: &[u8], path: &str) -> Vec<String> {
    if bytes.contains(&0x0D) {
        vec![format!("{path}: text fixture contains a carriage return")]
    } else {
        Vec::new()
    }
}

fn strings_of(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) => out.push(s.clone()),
        Value::Array(items) => items.iter().for_each(|i| strings_of(i, out)),
        Value::Object(map) => map.values().for_each(|m| strings_of(m, out)),
        _ => {}
    }
}

fn assert_clean(findings: Vec<String>) {
    assert!(findings.is_empty(), "fixture findings:\n{}", findings.join("\n"));
}

// ---- Tests ----

#[test]
fn scanners_flag_synthetic_leaks() {
    // Embedded value without the marker is a finding; the marker form is not.
    let leak: Value = serde_json::from_str(
        r#"{"password":{"type":"Embedded","data":{"ciphertext":"abc123","iv":"x"}}}"#,
    )
    .unwrap();
    let clean: Value = serde_json::from_str(
        r#"{"password":{"type":"Embedded","data":{"ciphertext":"SANITIZEDAAA","iv":"SANITIZED"}}}"#,
    )
    .unwrap();
    assert!(!scan_embedded(&leak, "synthetic").is_empty());
    assert!(scan_embedded(&clean, "synthetic").is_empty());

    // Private IPv4 is a finding; loopback, unspecified, multicast and RFC 5737 are not.
    assert!(!scan_ipv4("host 10.1.2.3 here", "synthetic").is_empty());
    assert!(!scan_ipv4("172.16.0.9", "synthetic").is_empty());
    for ok in [
        "192.0.2.3",
        "198.51.100.7",
        "203.0.113.9",
        "127.0.0.1",
        "0.0.0.0",
        "239.255.255.250",
    ] {
        assert!(scan_ipv4(ok, "synthetic").is_empty(), "{ok} must pass");
    }
    // Version strings are not addresses.
    assert!(scan_ipv4("driver 9.4.0.jre11 and 42.7.2", "synthetic").is_empty());

    // Entra GUIDs: a non-fake GUID in a login URL or a clientId/tenantId member is a finding.
    let real_url: Value = serde_json::from_str(
        r#"{"u":"https://login.microsoftonline.com/1f456c2a-3b4d-4e5f-8a9b-0c1d2e3f4a5b/v2.0"}"#,
    )
    .unwrap();
    let fake_url: Value = serde_json::from_str(
        r#"{"u":"https://login.microsoftonline.com/00000000-0000-4000-8000-000000000001/v2.0"}"#,
    )
    .unwrap();
    assert!(!scan_entra_guids(&real_url, "synthetic").is_empty());
    assert!(scan_entra_guids(&fake_url, "synthetic").is_empty());
    let real_client: Value =
        serde_json::from_str(r#"{"settings":{"clientId":"1f456c2a-3b4d-4e5f-8a9b-0c1d2e3f4a5b"}}"#)
            .unwrap();
    let fake_client: Value = serde_json::from_str(
        r#"{"settings":{"clientId":"00000000-0000-4000-8000-000000000002"}}"#,
    )
    .unwrap();
    assert!(!scan_entra_guids(&real_client, "synthetic").is_empty());
    assert!(scan_entra_guids(&fake_client, "synthetic").is_empty());

    // Tenant mail hosts.
    assert!(!scan_mail_hosts("\"x-example.mail.protection.outlook.com\"", "synthetic").is_empty());
    assert!(!scan_mail_hosts("\"someone@tenant.onmicrosoft.com\"", "synthetic").is_empty());
    assert!(scan_mail_hosts("\"smtp.example.com\"", "synthetic").is_empty());

    // CR in a text fixture.
    assert!(!scan_carriage_return(b"{\r\n}", "synthetic.json").is_empty());
    assert!(scan_carriage_return(b"{\n}", "synthetic.json").is_empty());
}

#[test]
fn embedded_secret_values_are_sanitized() {
    let findings: Vec<String> = json_files()
        .iter()
        .flat_map(|(path, value)| scan_embedded(value, path))
        .collect();
    assert_clean(findings);
}

#[test]
fn ipv4_literals_are_reserved_ranges_only() {
    let findings: Vec<String> = files_under(FIXTURES)
        .iter()
        .filter(|p| is_text(p))
        .flat_map(|p| {
            let text = String::from_utf8_lossy(&std::fs::read(p).expect("read fixture"))
                .into_owned();
            scan_ipv4(&text, &rel(p))
        })
        .collect();
    assert_clean(findings);
}

#[test]
fn entra_identifiers_are_fake() {
    let findings: Vec<String> = json_files()
        .iter()
        .flat_map(|(path, value)| scan_entra_guids(value, path))
        .collect();
    assert_clean(findings);
}

#[test]
fn no_tenant_mail_hosts() {
    let findings: Vec<String> = json_files()
        .iter()
        .flat_map(|(path, value)| {
            let mut strings = Vec::new();
            strings_of(value, &mut strings);
            strings
                .iter()
                .flat_map(|s| scan_mail_hosts(s, path))
                .collect::<Vec<_>>()
        })
        .collect();
    assert_clean(findings);
}

#[test]
fn text_fixtures_have_no_carriage_return() {
    let findings: Vec<String> = files_under(FIXTURES)
        .iter()
        .filter(|p| is_text(p))
        .flat_map(|p| scan_carriage_return(&std::fs::read(p).expect("read fixture"), &rel(p)))
        .collect();
    assert_clean(findings);
}

#[test]
fn corpus_inventory_matches_source_census() {
    let files = files_under(CORPUS);
    let count = |e: &str| files.iter().filter(|p| ext(p) == e).count();
    assert_eq!(count("json"), 183, "corpus .json count");
    assert_eq!(count("css"), 8, "corpus .css count");
    assert_eq!(count("jar"), 4, "corpus .jar count");
    assert_eq!(files.len(), 195, "corpus holds no other file kind");

    for jar in files.iter().filter(|p| ext(p) == "jar") {
        let bytes = std::fs::read(jar).expect("read stand-in jar");
        let name = rel(jar);
        assert!(bytes.len() <= 64, "{name}: stand-in jar over 64 bytes");
        assert!(bytes.contains(&0x00), "{name}: stand-in jar lacks a 0x00 byte");
        assert!(bytes.contains(&0x0D), "{name}: stand-in jar lacks a 0x0D byte");
    }
}

#[test]
fn gitattributes_marks_deploy_fixtures_non_text() {
    let attrs = std::fs::read_to_string(GITATTRIBUTES).expect("read repo .gitattributes");
    let has = |wanted: &str| attrs.lines().any(|l| l.trim() == wanted);
    assert!(
        has("crates/ignition-core/tests/fixtures/deploy/** -text"),
        ".gitattributes must mark fixtures/deploy/** as -text"
    );
    assert!(has("*.jar binary"), ".gitattributes must mark *.jar as binary");
}
