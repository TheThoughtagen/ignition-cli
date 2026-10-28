//! G-25-11's live gate (plan 25-24): which `writePermissions` entry admits
//! which API key for writes, measured on fresh gateways.
//!
//! `ign adopt` used to add the bare root of the granted level to
//! `writePermissions`. For the default `Authenticated/Roles/Administrator` that
//! root is bare `Authenticated`, which every API key holds, so a check token
//! minted for a read-only level could write. ADOPT-RESEARCH pitfall 3 says a
//! nested write entry did not admit an exact-path key on 8.3.6, which is why the
//! bare root was used. This test re-measures that, and measures whether a
//! dedicated write level works, before any adopt code changes.
//!
//! ```text
//! IGNITION_LIVE_ADOPT=1 cargo test -p ignition-core --test live_adopt_write_admission -- --ignored --exact --nocapture live_adopt_write_admission
//! IGNITION_LIVE_ADOPT=1 IGNITION_LIVE_ADOPT_IMAGE_TAG=8.3.3 cargo test -p ignition-core --test live_adopt_write_admission -- --ignored --exact --nocapture live_adopt_write_admission
//! ```
//!
//! | Variable | Meaning |
//! |---|---|
//! | `IGNITION_LIVE_ADOPT` | Must equal `1`. Unset = green no-op skip. |
//! | `IGNITION_LIVE_ADOPT_USER` | Gateway admin user. Defaults to `admin`, matching the fixture. |
//! | `IGNITION_LIVE_ADOPT_PASSWORD` | Gateway admin password. Defaults to `password`, matching the fixture. |
//! | `IGNITION_LIVE_ADOPT_IMAGE_TAG` | Ignition image tag for the fixture. Defaults to `8.3.6`. |
//!
//! The decision rule (pure [`decide`], tested offline), applied in this order:
//!
//! - `nested-exact` when, in state S1 (writePermissions = the commissioned list
//!   only), the Administrator key writes and the IgnCapRead key is refused;
//! - else `dedicated-nested` when, in S3 (commissioned + nested
//!   `Authenticated>IgnCapWrite`), both keys holding `Authenticated/IgnCapWrite`
//!   write, and the Administrator-only and IgnCapRead keys are refused;
//! - else `dedicated-root` when, in S2 (commissioned + bare `IgnCapWrite`), both
//!   keys holding `IgnCapWrite` write, the Administrator-only and IgnCapRead
//!   keys are refused, and the Administrator+IgnCapWrite key still reads;
//! - else `none`.
//!
//! Keys are held in memory only. `CAPTURE` lines carry level names, HTTP status
//! codes and booleans, never a key, a hash or a signature.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use ignition_core::client::GatewayApi;
use ignition_core::client::adopt::{
    build_security_put_body, create_api_token_via_session, generate_api_key_via_session,
    level_tree, put_security_properties_via_session, security_properties_via_session,
};
use ignition_core::client::idp::{GatewaySession, IdpLoginFlow, login};
use ignition_core::client::query::ListQuery;
use ignition_core::config::{Config, Credential, RigEntry, Secret};
use ignition_core::error::CoreError;
use ignition_core::rig::compose::{ComposeRunner, DockerCompose};
use ignition_core::rig::{ModuleProvisioning, RigSelection, resolve_plan};
use serde_json::{Value, json};

/// Wait budget handed to `rig up`.
const UP_TIMEOUT_S: u64 = 300;

/// The compose project name, so a leftover container is attributable and the
/// plan's `docker ps` check has something to grep for.
const PROJECT_NAME: &str = "ign-adopt-gate";

/// The rig name inside the seeded config.
const RIG_NAME: &str = "adopt-gate";

fn live_enabled() -> bool {
    std::env::var("IGNITION_LIVE_ADOPT").ok().as_deref() == Some("1")
}

fn skip(message: &str) {
    eprintln!("skipping: {message}");
}

fn image_tag() -> String {
    std::env::var("IGNITION_LIVE_ADOPT_IMAGE_TAG")
        .ok()
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "8.3.6".to_string())
}

fn admin_user() -> String {
    std::env::var("IGNITION_LIVE_ADOPT_USER").unwrap_or_else(|_| "admin".to_string())
}

fn admin_password() -> String {
    std::env::var("IGNITION_LIVE_ADOPT_PASSWORD").unwrap_or_else(|_| "password".to_string())
}

fn utc_now() -> String {
    let out = std::process::Command::new("date")
        .args(["-u", "+%FT%TZ"])
        .output()
        .expect("date");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// ---------------------------------------------------------------------------
// The decision rule (pure, offline-tested)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Decision {
    NestedExact,
    DedicatedNested,
    DedicatedRoot,
    None,
}

impl Decision {
    fn slug(self) -> &'static str {
        match self {
            Self::NestedExact => "nested-exact",
            Self::DedicatedNested => "dedicated-nested",
            Self::DedicatedRoot => "dedicated-root",
            Self::None => "none",
        }
    }
}

/// `(state, key)` to `(read status, write status)`. A key whose create was
/// refused has no row, and a missing row is never counted as "refused".
type Matrix = BTreeMap<(&'static str, &'static str), (u16, u16)>;

fn writes(m: &Matrix, state: &'static str, key: &'static str) -> bool {
    m.get(&(state, key)).is_some_and(|(_, write)| *write == 200)
}

fn refused(m: &Matrix, state: &'static str, key: &'static str) -> bool {
    m.get(&(state, key)).is_some_and(|(_, write)| *write == 403)
}

fn reads(m: &Matrix, state: &'static str, key: &'static str) -> bool {
    m.get(&(state, key)).is_some_and(|(read, _)| *read == 200)
}

fn decide(m: &Matrix) -> Decision {
    if writes(m, "S1", "cap25-adm") && refused(m, "S1", "cap25-ro") {
        return Decision::NestedExact;
    }
    if writes(m, "S3", "cap25-nestw")
        && writes(m, "S3", "cap25-adm-nestw")
        && refused(m, "S3", "cap25-adm")
        && refused(m, "S3", "cap25-ro")
    {
        return Decision::DedicatedNested;
    }
    if writes(m, "S2", "cap25-rootw")
        && writes(m, "S2", "cap25-adm-rootw")
        && refused(m, "S2", "cap25-adm")
        && refused(m, "S2", "cap25-ro")
        && reads(m, "S2", "cap25-adm-rootw")
    {
        return Decision::DedicatedRoot;
    }
    Decision::None
}

#[cfg(test)]
mod decide_tests {
    use super::*;

    fn matrix(rows: &[(&'static str, &'static str, u16, u16)]) -> Matrix {
        rows.iter()
            .map(|(state, key, read, write)| ((*state, *key), (*read, *write)))
            .collect()
    }

    fn nested_exact_rows() -> Vec<(&'static str, &'static str, u16, u16)> {
        vec![("S1", "cap25-adm", 200, 200), ("S1", "cap25-ro", 200, 403)]
    }

    fn dedicated_nested_rows() -> Vec<(&'static str, &'static str, u16, u16)> {
        vec![
            ("S3", "cap25-nestw", 200, 200),
            ("S3", "cap25-adm-nestw", 200, 200),
            ("S3", "cap25-adm", 200, 403),
            ("S3", "cap25-ro", 200, 403),
        ]
    }

    fn dedicated_root_rows() -> Vec<(&'static str, &'static str, u16, u16)> {
        vec![
            ("S2", "cap25-rootw", 200, 200),
            ("S2", "cap25-adm-rootw", 200, 200),
            ("S2", "cap25-adm", 200, 403),
            ("S2", "cap25-ro", 200, 403),
        ]
    }

    #[test]
    fn decide_follows_the_rule_order() {
        // One matrix per outcome.
        assert_eq!(decide(&matrix(&nested_exact_rows())), Decision::NestedExact);
        assert_eq!(
            decide(&matrix(&dedicated_nested_rows())),
            Decision::DedicatedNested
        );
        assert_eq!(
            decide(&matrix(&dedicated_root_rows())),
            Decision::DedicatedRoot
        );
        assert_eq!(decide(&matrix(&[])), Decision::None);

        // Every rule holds: the first (nested-exact) wins.
        let mut all = nested_exact_rows();
        all.extend(dedicated_nested_rows());
        all.extend(dedicated_root_rows());
        assert_eq!(decide(&matrix(&all)), Decision::NestedExact);

        // Nested-exact fails, the other two hold: dedicated-nested wins.
        let mut rest = vec![("S1", "cap25-adm", 200, 200), ("S1", "cap25-ro", 200, 200)];
        rest.extend(dedicated_nested_rows());
        rest.extend(dedicated_root_rows());
        assert_eq!(decide(&matrix(&rest)), Decision::DedicatedNested);

        // dedicated-root also needs the Administrator+IgnCapWrite key to read.
        let mut no_read = dedicated_root_rows();
        no_read[1] = ("S2", "cap25-adm-rootw", 403, 200);
        assert_eq!(decide(&matrix(&no_read)), Decision::None);

        // A key that returns 0 (any other error) is never counted as refused.
        let zero = vec![("S1", "cap25-adm", 200, 200), ("S1", "cap25-ro", 200, 0)];
        assert_eq!(decide(&matrix(&zero)), Decision::None);
        let mut zero_nested = dedicated_nested_rows();
        zero_nested[3] = ("S3", "cap25-ro", 200, 0);
        assert_eq!(decide(&matrix(&zero_nested)), Decision::None);
    }
}

// ---------------------------------------------------------------------------
// Rig plumbing (copied in shape from live_rig_module_injection.rs)
// ---------------------------------------------------------------------------

/// Copy the fixture compose into `dir`, returning the copied path.
fn seed_fixture(dir: &Path) -> std::path::PathBuf {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/live-adopt-rig/compose.yml");
    let dest = dir.join("compose.yml");
    std::fs::copy(&fixture, &dest).expect("copy fixture compose");
    dest
}

fn config_for(compose_file: &Path) -> Config {
    let mut config = Config::default();
    config.rigs.insert(
        RIG_NAME.to_string(),
        RigEntry {
            compose_file: compose_file.display().to_string(),
            project_name: Some(PROJECT_NAME.to_string()),
            modules: BTreeMap::new(),
            module_service: None,
        },
    );
    config
}

/// Bring the fixture gateway up and return its URL.
async fn bring_up(runner: &DockerCompose, compose_file: &Path) -> Result<String, String> {
    let config = config_for(compose_file);
    let plan = resolve_plan(runner, RigSelection::Named(RIG_NAME.into()), &config)
        .await
        .map_err(|e| format!("resolve_plan: {e}"))?;
    ignition_core::actions::rig::rig_up(
        runner,
        &plan,
        UP_TIMEOUT_S,
        None,
        &ModuleProvisioning::default(),
    )
    .await
    .map_err(|e| format!("rig_up: {e}"))?;
    ignition_core::actions::rig::gateway_url_from(&plan)
        .ok_or_else(|| "no gateway URL derivable from the plan".to_string())
}

/// Unconditional teardown: `down -v`, so the volume goes too.
async fn tear_down(runner: &DockerCompose, compose_file: &Path) {
    let config = config_for(compose_file);
    if let Ok(plan) = resolve_plan(runner, RigSelection::Named(RIG_NAME.into()), &config).await {
        let existing: Vec<std::path::PathBuf> = ignition_core::rig::existing_override(&plan)
            .into_iter()
            .collect();
        let _ = ComposeRunner::run(
            runner,
            &ignition_core::rig::compose::down_args(&plan, true, &existing),
        )
        .await;
    }
}

/// Log in as the admin, retrying while the freshly started gateway finishes
/// commissioning. Bounded, never an open loop.
async fn admin_login(url: &str) -> Result<(IdpLoginFlow, GatewaySession), String> {
    let password = Secret::new(admin_password());
    let mut last = String::from("never attempted");
    for _ in 0..30 {
        let flow = IdpLoginFlow::new(url).map_err(|e| format!("login flow: {e}"))?;
        match login(flow, &admin_user(), &password).await {
            Ok(pair) => return Ok(pair),
            Err(err) => last = format!("{err}"),
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    Err(format!("admin login never succeeded: {last}"))
}

// ---------------------------------------------------------------------------
// Wire helpers
// ---------------------------------------------------------------------------

/// A test-local create body: `build_token_create_body`'s shape with the given
/// `securityLevels` list. Src is not changed by the capture.
fn token_body(name: &str, levels: &[Value], hash: &str, timestamp_ms: i64) -> Value {
    json!([{
        "name": name,
        "collection": "core",
        "enabled": true,
        "description": "",
        "config": {
            "profile": {
                "securityLevels": levels,
                "secureChannelRequired": false,
                "type": "basic-token",
                "timestamp": timestamp_ms
            },
            "settings": { "tokenHash": hash }
        }
    }])
}

/// Mint a key with the given levels. `Ok(None)` means the gateway refused the
/// create (data, not a failure). The key is held in memory only.
async fn mint_key(
    flow: &IdpLoginFlow,
    session: &GatewaySession,
    name: &str,
    levels: &[Value],
) -> Result<Option<String>, String> {
    let generated = generate_api_key_via_session(flow, session)
        .await
        .map_err(|e| format!("generate for {name}: {e}"))?;
    if generated.key.is_empty() || generated.hash.is_empty() {
        return Err(format!("generate for {name} answered an empty key or hash"));
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_millis() as i64;
    let body = token_body(name, levels, &generated.hash, ts);
    match create_api_token_via_session(flow, session, &body).await {
        Ok(answer) if answer.success => Ok(Some(format!("{name}:{}", generated.key))),
        Ok(answer) => {
            eprintln!(
                "CAPTURE create {name}: refused (success false, problem {:?})",
                answer.problem
            );
            Ok(None)
        }
        Err(err) => {
            eprintln!("CAPTURE create {name}: refused ({err})");
            Ok(None)
        }
    }
}

fn status_of<T>(result: Result<T, CoreError>) -> u16 {
    match result {
        Ok(_) => 200,
        Err(CoreError::Auth { status, .. }) => status,
        Err(err) => {
            eprintln!("CAPTURE probe: unexpected error kind {}", err.code());
            0
        }
    }
}

/// `(read, write)` statuses for one key: read is a projects list, write is the
/// harmless scan/projects rescan (the same probe doctor's `--check-write` uses).
async fn probe(url: &str, token: &str) -> (u16, u16) {
    let parsed: url::Url = url.parse().expect("rig url parses");
    let session = ignition_core::session::Session::for_url(
        parsed,
        Some(Credential::Token(Secret::new(token.to_string()))),
        true,
    )
    .expect("probe session");
    let api = session.api();
    let read = status_of(api.projects(&ListQuery::default()).await);
    let write = status_of(api.scan_projects().await);
    (read, write)
}

fn levels_of(config: &Value, field: &str) -> Vec<Value> {
    config
        .get(field)
        .and_then(|p| p.get("securityLevels"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// Strip every `description` so a gateway that echoes descriptions does not
/// read as a rewritten list.
fn strip_descriptions(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| k.as_str() != "description")
                .map(|(k, v)| (k.clone(), strip_descriptions(v)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(strip_descriptions).collect()),
        other => other.clone(),
    }
}

fn replace_levels(config: &mut Value, field: &str, levels: &[Value]) -> Result<(), String> {
    let object = config
        .as_object_mut()
        .ok_or("security config is not an object")?;
    let permissions = object
        .entry(field)
        .or_insert_with(|| json!({ "type": "AnyOf" }));
    permissions
        .as_object_mut()
        .ok_or_else(|| format!("{field} is not an object"))?
        .insert("securityLevels".into(), Value::Array(levels.to_vec()));
    Ok(())
}

/// Apply one permission state through the admin session. Returns the admin
/// session's write verdict (the PUT's `success`). Errors with a `gate premise`
/// message when the gateway rewrote the lists it was given.
async fn set_state(
    flow: &IdpLoginFlow,
    session: &GatewaySession,
    read_levels: &[Value],
    write_levels: &[Value],
) -> Result<bool, String> {
    let singleton = security_properties_via_session(flow, session)
        .await
        .map_err(|e| format!("read security singleton: {e}"))?;
    let mut config = singleton.config.clone();
    replace_levels(&mut config, "readPermissions", read_levels)?;
    replace_levels(&mut config, "writePermissions", write_levels)?;
    let body = build_security_put_body(&singleton, &config);
    let answer = match put_security_properties_via_session(flow, session, &body).await {
        Ok(answer) => answer,
        Err(err) => {
            eprintln!("CAPTURE admin PUT error: {err}");
            return Ok(false);
        }
    };
    if !answer.success {
        eprintln!("CAPTURE admin PUT refused: problem {:?}", answer.problem);
        return Ok(false);
    }
    let after = security_properties_via_session(flow, session)
        .await
        .map_err(|e| format!("re-read security singleton: {e}"))?;
    for (field, wanted) in [
        ("readPermissions", read_levels),
        ("writePermissions", write_levels),
    ] {
        let got = strip_descriptions(&Value::Array(levels_of(&after.config, field)));
        let want = strip_descriptions(&Value::Array(wanted.to_vec()));
        if got != want {
            return Err(format!(
                "gate premise: the gateway rewrote the permission lists: {field} wanted {want} got {got}"
            ));
        }
    }
    Ok(true)
}

fn bare(name: &str) -> Value {
    level_tree(&[name])
}

fn is_open_entry(entry: &Value) -> bool {
    let name = entry.get("name").and_then(Value::as_str).unwrap_or("");
    let bare_form = entry
        .get("children")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty);
    bare_form && (name.eq_ignore_ascii_case("authenticated") || name.eq_ignore_ascii_case("public"))
}

// ---------------------------------------------------------------------------
// The capture
// ---------------------------------------------------------------------------

struct KeySpec {
    name: &'static str,
    levels: Vec<Value>,
}

fn key_specs() -> Vec<KeySpec> {
    let admin = level_tree(&["Authenticated", "Roles", "Administrator"]);
    let read = level_tree(&["Authenticated", "Roles", "IgnCapRead"]);
    let root_w = level_tree(&["IgnCapWrite"]);
    let nest_w = level_tree(&["Authenticated", "IgnCapWrite"]);
    vec![
        KeySpec {
            name: "cap25-adm",
            levels: vec![admin.clone()],
        },
        KeySpec {
            name: "cap25-ro",
            levels: vec![read],
        },
        KeySpec {
            name: "cap25-rootw",
            levels: vec![root_w.clone()],
        },
        KeySpec {
            name: "cap25-nestw",
            levels: vec![nest_w.clone()],
        },
        KeySpec {
            name: "cap25-adm-rootw",
            levels: vec![admin.clone(), root_w],
        },
        KeySpec {
            name: "cap25-adm-nestw",
            levels: vec![admin, nest_w],
        },
    ]
}

async fn run_capture(url: &str) -> Result<Decision, String> {
    let (flow, session) = admin_login(url).await?;
    println!("CAPTURE start {} image {}", utc_now(), image_tag());

    let singleton = security_properties_via_session(&flow, &session)
        .await
        .map_err(|e| format!("read security singleton: {e}"))?;
    let r0 = levels_of(&singleton.config, "readPermissions");
    let w0 = levels_of(&singleton.config, "writePermissions");
    println!(
        "CAPTURE commissioned readPermissions: {}",
        Value::Array(r0.clone())
    );
    println!(
        "CAPTURE commissioned writePermissions: {}",
        Value::Array(w0.clone())
    );

    let mut failures: Vec<String> = Vec::new();
    if w0.is_empty() || w0.iter().any(is_open_entry) {
        failures.push(
            "gate premise: the commissioned writePermissions already admits every key".into(),
        );
    }

    // Mint the six keys.
    let mut tokens: Vec<(&'static str, String)> = Vec::new();
    for spec in key_specs() {
        let names: Vec<String> = spec
            .levels
            .iter()
            .map(ignition_core::client::adopt::level_path_string)
            .collect();
        match mint_key(&flow, &session, spec.name, &spec.levels).await? {
            Some(token) => {
                println!("CAPTURE create {} levels {:?}: created", spec.name, names);
                tokens.push((spec.name, token));
            }
            None => println!(
                "CAPTURE create {} levels {:?}: create refused",
                spec.name, names
            ),
        }
    }
    for needed in ["cap25-adm", "cap25-ro"] {
        if !tokens.iter().any(|(n, _)| *n == needed) {
            failures.push(format!("gate premise: {needed} was not created"));
        }
    }

    let mut rx = r0.clone();
    if !rx.iter().any(is_open_entry) {
        rx.push(bare("Authenticated"));
    }
    let with = |extra: Value| {
        let mut list = w0.clone();
        list.push(extra);
        list
    };
    let states: Vec<(&'static str, Vec<Value>, Vec<Value>)> = vec![
        ("S0", r0.clone(), w0.clone()),
        ("S1", rx.clone(), w0.clone()),
        ("S2", rx.clone(), with(bare("IgnCapWrite"))),
        (
            "S3",
            rx.clone(),
            with(level_tree(&["Authenticated", "IgnCapWrite"])),
        ),
        ("S4", rx.clone(), with(bare("Authenticated"))),
    ];

    let mut matrix: Matrix = BTreeMap::new();
    let mut admin_puts: Vec<(&'static str, bool)> = Vec::new();
    for (state, read_levels, write_levels) in &states {
        let verdict = set_state(&flow, &session, read_levels, write_levels).await?;
        admin_puts.push((*state, verdict));
        println!("CAPTURE state {state} admin PUT success {verdict}");
        // Let the gateway apply the new permission lists before probing.
        tokio::time::sleep(Duration::from_secs(3)).await;
        for (name, token) in &tokens {
            let (read, write) = probe(url, token).await;
            println!("CAPTURE state {state} key {name} read {read} write {write}");
            matrix.insert((*state, *name), (read, write));
        }
    }
    // One more no-op PUT under S4, recording the admin write there.
    let (_, s4_read, s4_write) = &states[4];
    let after_s4 = set_state(&flow, &session, s4_read, s4_write).await?;
    admin_puts.push(("S4-after", after_s4));
    println!("CAPTURE state S4-after admin PUT success {after_s4}");

    if !admin_puts.iter().all(|(_, ok)| *ok) {
        failures.push(format!(
            "gate premise: an admin PUT was refused: {admin_puts:?}"
        ));
    }
    if !writes(&matrix, "S4", "cap25-ro") {
        failures.push(
            "gate premise: with bare Authenticated in writePermissions the IgnCapRead key did \
             not write, which does not reproduce G-25-11"
                .into(),
        );
    }
    if !reads(&matrix, "S1", "cap25-ro") {
        failures.push("gate premise: in S1 the IgnCapRead key does not read".into());
    }

    let decision = decide(&matrix);
    println!("CAPTURE decision: {}", decision.slug());
    println!("CAPTURE end {}", utc_now());

    if let Some(first) = failures.first() {
        return Err(failures
            .iter()
            .skip(1)
            .fold(first.clone(), |acc, f| format!("{acc}; {f}")));
    }
    if decision == Decision::None {
        return Err(
            "no exclusive write admission: none of the three write models admits the \
                    intended keys and refuses the others"
                .to_string(),
        );
    }
    Ok(decision)
}

#[ignore = "live: needs Docker and IGNITION_LIVE_ADOPT=1"]
#[tokio::test]
async fn live_adopt_write_admission() {
    if !live_enabled() {
        skip("IGNITION_LIVE_ADOPT is not 1");
        return;
    }
    println!(
        "live gate: inductiveautomation/ignition:{} (set IGNITION_LIVE_ADOPT_IMAGE_TAG to select another)",
        image_tag()
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let compose_file = seed_fixture(dir.path());
    let runner = DockerCompose;

    let result = match bring_up(&runner, &compose_file).await {
        Ok(url) => run_capture(&url).await,
        Err(message) => Err(format!("gate premise: image or bring-up: {message}")),
    };

    // Teardown is UNCONDITIONAL, then the verdict panics.
    tear_down(&runner, &compose_file).await;

    if let Err(message) = result {
        panic!("{message}");
    }
}
