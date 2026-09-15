//! CORE-11 stdout-purity harness: byte-exact stdout over the REAL spawned
//! binary under MAXIMUM diagnostics. The invariant being pinned: tracing is
//! stderr-only by construction (`init_tracing` ends
//! `.with_writer(std::io::stderr)`, main.rs) — so unknown-key config
//! warnings plus `IGNITION_LOG=trace` noise can never leak one stray byte
//! into stdout. If a regression ever prints a tracing line, panic message,
//! or partial write to stdout under noise, the byte compare fails on that
//! single stray byte.
//!
//! Noise recipe (both tests): a tempdir config file carrying an unknown
//! top-level key (`bogus_key = 1` — fires `warn_unknown_keys` tracing on
//! EVERY config load) and `IGNITION_LOG=trace` (max diagnostics).
//!
//! Harness design: ASSERT-based, deliberately NOT snapbox goldens, so
//! `SNAPSHOTS=overwrite` cannot silently sanitize a failure — a leaked byte
//! here must FAIL, never be rewritten. The expected bytes are hard-coded
//! (derived once from the pinned envelope shape in contract_profile.rs /
//! the real binary): pretty-JSON empty-list envelope + its single trailing
//! `println!` newline; human mode with no profiles and no active profile
//! prints NOTHING (the `[profile: …]` header is suppressed when no profile
//! resolved and there are no rows to render).
//!
//! Every spawn also strips the ambient `IGNITION_PROFILE` / `IGNITION_JSON`
//! / `IGNITION_YES` knobs — a developer shell with one of those exported
//! would otherwise change the envelope bytes (profile echo / render mode)
//! and turn a byte-exact assertion into a shell-dependent flake.

use assert_cmd::Command;

/// Expected `ign profile list --json` stdout for the empty config, byte for
/// byte (pretty envelope, single trailing newline from `println!`).
const EXPECTED_JSON_STDOUT: &[u8] = b"{\n  \"ok\": true,\n  \"profile\": null,\n  \"data\": {\n    \"active\": null,\n    \"profiles\": []\n  }\n}\n";

/// Expected `ign profile list` (human mode) stdout for the empty config:
/// ZERO bytes — no header (no active profile), no rows.
const EXPECTED_HUMAN_STDOUT: &[u8] = b"";

/// Build `ign` under maximum diagnostics: isolated tempdir config whose
/// file carries an unknown top-level key, trace logging on, and the
/// ambient IGNITION_* knobs stripped for determinism.
fn noisy_ign() -> (tempfile::TempDir, Command) {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "bogus_key = 1\n").expect("write noisy config");
    let mut cmd = Command::cargo_bin("ign").expect("binary 'ign' not found");
    cmd.env("IGNITION_CLI_CONFIG", &config);
    cmd.env("IGNITION_LOG", "trace");
    cmd.env_remove("IGNITION_PROFILE");
    cmd.env_remove("IGNITION_JSON");
    cmd.env_remove("IGNITION_YES");
    (dir, cmd)
}

/// Byte-equality with a diff-friendly panic message (first differing offset
/// plus both sides' lengths) — a bare `assert_eq!` on byte slices dumps
/// hundreds of escaped bytes for one stray character.
fn assert_stdout_bytes(actual: &[u8], expected: &[u8], label: &str) {
    if actual == expected {
        return;
    }
    let first_diff = actual
        .iter()
        .zip(expected.iter())
        .position(|(a, e)| a != e)
        .unwrap_or(actual.len().min(expected.len()));
    panic!(
        "{label}: stdout is not byte-pure under max diagnostics.\n  first \
         differing byte at offset {first_diff}\n  actual len: {}, expected \
         len: {}\n  actual:   {:?}\n  expected: {:?}",
        actual.len(),
        expected.len(),
        String::from_utf8_lossy(actual),
        String::from_utf8_lossy(expected),
    );
}

/// JSON mode: stdout must be EXACTLY the empty-list envelope — one leaked
/// tracing byte under unknown-key warnings + `IGNITION_LOG=trace` fails.
#[test]
fn stdout_is_byte_pure_under_max_diagnostics() {
    let (_dir, mut cmd) = noisy_ign();
    let output = cmd
        .args(["profile", "list", "--json"])
        .output()
        .expect("spawn ign");
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Byte-exact: raw stdout equals the expected envelope bytes (single
    // trailing newline included).
    assert_stdout_bytes(&output.stdout, EXPECTED_JSON_STDOUT, "json mode");

    // Belt and braces (documents intent; byte-equality already implies it):
    // stdout is valid UTF-8 and parses as exactly ONE serde_json value with
    // nothing after it — `from_slice` rejects trailing non-whitespace bytes,
    // so this assertion localizes a future failure to "trailing junk"
    // versus "wrong bytes".
    let text = std::str::from_utf8(&output.stdout).expect("stdout is valid UTF-8");
    serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .expect("stdout parses as exactly one JSON value with nothing after it");
    assert!(
        text.trim_end().ends_with('}'),
        "stdout ends inside a JSON value, not after it: {text:?}"
    );

    // The noise really fired: stderr carried the unknown-key warning.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("bogus_key"),
        "expected the unknown-key warning on stderr (noise precondition): {stderr}"
    );
}

/// Human mode: stdout must be EXACTLY zero bytes — the `[profile: …]`
/// header is suppressed when no profile resolved, there are no rows, and
/// the diagnostics noise stays on stderr.
#[test]
fn human_stdout_is_line_pure_under_max_diagnostics() {
    let (_dir, mut cmd) = noisy_ign();
    let output = cmd.args(["profile", "list"]).output().expect("spawn ign");
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert_stdout_bytes(&output.stdout, EXPECTED_HUMAN_STDOUT, "human mode");

    // The noise really fired here too.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("bogus_key"),
        "expected the unknown-key warning on stderr (noise precondition): {stderr}"
    );
}

/// 11-05 TAGS-12 scenario: the loss-gate REFUSAL prose (multi-line,
/// loss-report) must appear ONLY on stderr — byte-exact zero stdout
/// under max diagnostics. The gate runs PRE-resolution, so no
/// profile or gateway is needed: the refusal happens before any
/// resolution work and stdout must carry nothing at all.
#[test]
fn tags_import_loss_prose_never_touches_stdout() {
    let (_dir, mut cmd) = noisy_ign();
    let dir = tempfile::tempdir().expect("tempdir");
    // A gateway-export-shaped file (MinVersion root attr) — the
    // xml_export_edited_only fact fires the gate.
    let xml = dir.path().join("p5.xml");
    std::fs::write(
        &xml,
        "<Tags MinVersion=\"8.0.0\" locale=\"en_US\">\r\n   <Tag name=\"T1\" type=\"AtomicTag\">\r\n      <Property name=\"valueSource\">memory</Property>\r\n   </Tag>\r\n</Tags>\r\n",
    )
    .expect("write xml fixture");
    let output = cmd
        .args([
            "tags",
            "import",
            "--format",
            "xml",
            "--file",
            xml.to_str().expect("utf-8 path"),
            "--provider",
            "p5import",
        ])
        .output()
        .expect("spawn ign");

    assert_eq!(
        output.status.code(),
        Some(2),
        "the gate refuses exit 2 pre-resolution"
    );
    assert_stdout_bytes(&output.stdout, b"", "loss-gate refusal");

    // The prose really fired — on stderr.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("loss report (xml)") && stderr.contains("re-run with --yes"),
        "expected the loss report prose on stderr: {stderr}"
    );
}

/// 11-05 sibling: the stdout-mode raw export (`tags export --format
/// xml -o -`) writes the payload bytes and NOTHING else to stdout —
/// but the raw path needs a gateway, so this pins the failure-side
/// purity instead: any xml/csv export ERROR (precondition refusal)
/// leaves stdout byte-empty under noise, same invariant, zero wire.
#[test]
fn tags_export_error_keeps_stdout_byte_empty() {
    let (_dir, mut cmd) = noisy_ign();
    let output = cmd
        .args([
            "tags",
            "export",
            "[default]P5",
            "--format",
            "xml",
            "-o",
            "-",
        ])
        .output()
        .expect("spawn ign");
    assert_ne!(
        output.status.code(),
        Some(0),
        "no gateway configured — must refuse"
    );
    assert_stdout_bytes(&output.stdout, b"", "xml export refusal");
}

/// 13-07: the workspace family joins the purity harness — the three
/// new verbs are normal envelope verbs, so every refusal renders as
/// the contract error envelope on STDERR and stdout stays byte-empty
/// under max diagnostics. `workspace status` refuses PRE-resolution
/// (the missing-manifest read needs no gateway): profile null, exit
/// 2, zero requests, zero stdout bytes.
#[test]
fn workspace_status_missing_manifest_refusal_never_touches_stdout() {
    let (_dir, mut cmd) = noisy_ign();
    let missing = tempfile::tempdir().expect("tempdir").path().join("nope");
    let output = cmd
        .args(["workspace", "status", missing.to_str().expect("utf-8 path")])
        .output()
        .expect("spawn ign");
    assert_eq!(
        output.status.code(),
        Some(2),
        "the pre-resolve refusal exits 2"
    );
    assert_stdout_bytes(&output.stdout, b"", "missing-manifest refusal");

    // 13-03's stable refusal prefix really fired — on stderr.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not an ign workspace"),
        "expected the stable manifest refusal on stderr: {stderr}"
    );
}

/// 13-07 sibling: the same purity invariant for `workspace push`
/// (the PRE-resolve missing-manifest refusal — the reachable
/// zero-gateway case) and `workspace checkout` against a DEAD PORT —
/// the action-layer network refusal must also leave stdout untouched
/// under noise.
#[test]
fn workspace_push_and_checkout_refusals_keep_stdout_byte_empty() {
    // push: pre-resolve manifest refusal (no gateway involved).
    let (_dir, mut cmd) = noisy_ign();
    let absent = tempfile::tempdir().expect("tempdir");
    let missing = absent.path().join("nope");
    let output = cmd
        .args(["workspace", "push", missing.to_str().expect("utf-8 path")])
        .output()
        .expect("spawn ign");
    assert_eq!(
        output.status.code(),
        Some(2),
        "the pre-resolve refusal exits 2"
    );
    assert_stdout_bytes(&output.stdout, b"", "push missing-manifest refusal");

    // checkout: resolution succeeds against a profile whose URL is a
    // dead port — the action-layer network refusal (exit 6) rides the
    // envelope on stderr too.
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("config.toml");
    std::fs::write(
        &config,
        "active = \"dev\"\n\n[profiles.dev]\nurl = \"http://127.0.0.1:9\"\nauth = { token_env = \"IGNITION_TOKEN\" }\n",
    )
    .expect("write dead-port config");
    let output = Command::cargo_bin("ign")
        .expect("binary 'ign' not found")
        .env("IGNITION_CLI_CONFIG", &config)
        .env("IGNITION_TOKEN", "mock:name-key")
        .env("IGNITION_LOG", "trace")
        .env_remove("IGNITION_PROFILE")
        .env_remove("IGNITION_JSON")
        .env_remove("IGNITION_YES")
        .args(["workspace", "checkout", "proj", "/tmp/ign-purity-checkout"])
        .output()
        .expect("spawn ign");
    assert_ne!(output.status.code(), Some(0), "a dead port must refuse");
    assert_stdout_bytes(&output.stdout, b"", "checkout network refusal");
}
