# Phase 1: Foundation & Agentic Contracts - Research

**Researched:** 2026-08-21
**Domain:** Rust Cargo workspace + CLI contracts (clap 4, config/profiles, keyring, error envelope, exit codes, golden-file testing)
**Confidence:** HIGH (stack versions + MSRV verified live against crates.io API 2026-08-21; keyring headless behavior verified against keyring-rs official docs and its own CI workflow; version endpoint verified in local 83-api collection)

> **No CONTEXT.md exists for this phase.** No user decisions are locked. Recommendations below are research-derived; the planner should treat them as defaults, not constraints. Project-level stack research in `.planning/research/STACK.md` remains authoritative for crate choices — this file goes deeper on Phase-1 mechanics and **overrides STACK.md on two points** (see Corrections).

## Summary

Phase 1 is exhaustively-documented Rust-CLI territory, but three findings materially change the plan. **First, the MSRV is wrong in STACK.md: `keyring` 4.1.x declares `rust_version = 1.88.0` (verified via crates.io), so the workspace MSRV must be 1.88, not 1.85** — every other core crate (clap 4.6, toml 1.1, reqwest 0.13, snapbox 1.2, assert_cmd 2.2) sits at 1.85, so keyring alone sets the floor. **Second, the STATE.md keyring blocker has a verified, two-part resolution:** keyring 4.1's default Linux store (zbus Secret Service) fails fast with no D-Bus daemon — and keyring-rs's own CI proves the headless-Linux smoke-test recipe (`apt install gnome-keyring` + `gnome-keyring-daemon --components=secrets --daemonize --unlock`); combined with an env-first resolution order and a `SecretStore` trait seam in our code, default CI never needs a secret service at all. **Third, the exit-code taxonomy needs one reconciliation** before golden tests are written: CORE-04 demands a distinct *config* code that STACK.md's table lacks — this research proposes the final 0–7 table (config=3, network=4, auth=5, target-state=6) so the contract is locked before the first golden file exists.

The recommended shape: a three-crate workspace (`ignition-cli` bin / `ignition-core` lib / `ignition-tui` stub lib) with `workspace.dependencies` single-sourcing; global clap args (`--profile/--json/--compact/--yes/--verbose`) defined once on the root with `global = true`; hand-rolled TOML config via `directories` 6.0 + a mandatory `IGNITION_CLI_CONFIG` env override (because `directories` ignores XDG on macOS — a real test-isolation gotcha); a `SecretStore` trait with env > keyring resolution and a redaction-verified `Secret<T>` newtype; a thiserror `CoreError` enum → single exit-mapping point in `main() -> ExitCode`; and golden-file contract tests with **snapbox 1.2** (the rust-cli-org snapshot toolbox clap itself uses, with `SNAPSHOTS=overwrite` review workflow and built-in Redactions) alongside assert_cmd exit-code assertions.

**Primary recommendation:** Build the output contract (envelope + exit codes + golden files) in the FIRST task after the workspace skeleton, not last — every subsequent subcommand test then enforces the contract for free.

## Corrections to Prior Research (STACK.md)

| # | STACK.md says | Correction | Evidence |
|---|---------------|------------|----------|
| 1 | MSRV 1.85 (driven by toml 1.1) | **MSRV must be 1.88** — `keyring` 4.1.x declares `rust_version = 1.88.0` | crates.io `/crates/keyring/versions` (4.1.1–4.1.6 all 1.88.0), fetched 2026-08-21 |
| 2 | Keyring MEDIUM confidence, "smoke-test in Phase 1" | Blocker **resolved**: fail-fast headless behavior + CI recipe + seam pattern documented below. Confidence now HIGH | keyring-rs official docs (Context7) + its own `.github/workflows/ci.yaml` (fetched) |

Also resolved from ARCHITECTURE.md open question 6: **defer `comfy-table` decision to the first human-rendering task in Phase 2** — Phase 1 only needs JSON + trivial line rendering (`profile list` can render plain lines), so do not add a table crate in Phase 1.

## Standard Stack

### Core (Phase 1 — additions to STACK.md's verified set)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `clap` (derive) | 4.6 | CLI parsing, global args, usage-error exit 2 | Rust CLI default; exit-code-2-for-usage verified via Context7 /websites/rs_clap |
| `clap_complete` | 4.6.9 | `completions` subcommand (bash/zsh/fish) | Same repo/org as clap; `aot::{generate, Shell}` API verified on docs.rs |
| `toml` + `serde` + `directories` | 1.1.4 / 1.0 / 6.0.0 | Config + profile files, path discovery | Hand-rolled per STACK.md (config-rs/figment already rejected) |
| `keyring` | 4.1.6 | OS secret storage (macOS Keychain / Linux Secret Service) | default feature `v1` = classic `Entry` API (verified: `default = ["v1"]`, `Entry::new/set_password/get_password/delete_credential`) |
| `thiserror` | 2.0.20 | Typed error enum → envelope + exit codes | STACK.md decision; stable slugs need typed variants |
| `tracing` + `tracing-subscriber` (env-filter) | 0.1 / 0.3 | stderr diagnostics, never stdout | STACK.md; logs must not pollute `--json` stdout |
| `semver` | 1.0 | Gateway ≥8.3.1 comparison | Only for `Version::parse` of dotted triples; tolerate pre-release suffixes (see Version Check) |

### Testing (Phase 1 additions)

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `snapbox` | 1.2.2 | Golden-file JSON contract tests, `SNAPSHOTS=overwrite` review flow, dynamic-value Redactions | **For every `--json` output shape**; same org as assert_cmd (used by clap's own repo) |
| `assert_cmd` + `predicates` + `tempfile` | 2.2.2 / 3.1.4 / 3.27 | Binary-level exit-code/stderr assertions, isolated HOME/config dirs | Exit-code taxonomy tests + redaction tests (secret substring never in stdout/stderr) |
| `wiremock` | 0.6.5 | `GatewayApi` HTTP contract tests | Mock `/data/api/v1/gateway-info` (200/HTML-error/401/timeout shapes) |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| snapbox | insta | insta is the generic snapshot king, but snapbox is purpose-built for CLI stdout/stderr+exit codes, made by the rust-cli org (assert_cmd/clap maintainers), and has `RedactedValue` for dynamic fields — better fit, no cross-ecosystem jump |
| snapbox | trycmd | trycmd (same org) runs *many* blunt file-driven cases and is doc-friendly; heavier harness than Phase 1 needs — consider for Phase 2+ docs tests |
| wiremock | httpmock | wiremock already chosen in STACK.md; verified matchers (`method/path/header/body_json`), `expect(n)` verification, `mount_as_scoped().received_requests()` for asserting auth headers were sent — everything Phase 1–2 needs |
| keyring `cli` feature (`use_sample_store`) | own `SecretStore` trait | keyring's `cli` feature pulls dbus-secret-service + linux-keyutils + db-keystore stores on Linux (verified from its Cargo.toml) — heavy tree cost just for a test mock. Our own trait is ~20 lines and keeps `keyring = { version = "4.1" }` default-lean |
| `secrecy` crate | hand-rolled `Secret<T>` | STACK.md called secrecy "optional polish"; a ~15-line newtype with `Debug`/`Display` → `***` and no `Serialize` impl gives the same guarantee with zero deps. Use the hand-roll in Phase 1 |

**Installation (workspace `Cargo.toml`):**
```toml
[workspace]
resolver = "3"
members = ["crates/ignition-cli", "crates/ignition-core", "crates/ignition-tui"]

[workspace.package]
edition = "2024"
rust-version = "1.88"        # CORRECTED: keyring 4.1.x floor (was 1.85 in STACK.md)
version = "0.1.0"

[workspace.dependencies]
ignition-core = { path = "crates/ignition-core" }
ignition-tui = { path = "crates/ignition-tui" }
clap = { version = "4.6", features = ["derive"] }
clap_complete = "4.6"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.53", features = ["rt-multi-thread", "macros", "time"] }
reqwest = { version = "0.13", default-features = true, features = ["json"] }
toml = "1.1"
directories = "6.0"
keyring = "4.1"
thiserror = "2.0"
semver = "1.0"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
url = "2.5"
# dev
wiremock = "0.6"
snapbox = { version = "1.2", features = ["cmd", "json", "regex"] }  # dev-dep only
assert_cmd = "2.2"
predicates = "3.1"
tempfile = "3.27"
```

## Architecture Patterns

### Recommended Project Structure

```
ignition-cli/
├── Cargo.toml                      # virtual manifest (above)
├── crates/
│   ├── ignition-core/
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config/
│   │       │   ├── mod.rs          # load/save/discovery + env overlay
│   │       │   ├── profile.rs      # GatewayProfile struct (serde)
│   │       │   └── secret.rs       # Secret<T>, SecretStore trait, EnvStore, KeyringStore
│   │       ├── error.rs            # CoreError (thiserror) + ErrorEnvelope + exit mapping
│   │       ├── output.rs           # JsonEnvelope render (pretty/compact), profile echo
│   │       ├── client/
│   │       │   ├── mod.rs          # trait GatewayApi + ReqwestGatewayApi
│   │       │   └── version.rs      # GatewayInfo model + min-version check
│   │       └── actions/
│   │           ├── mod.rs          # Action trait/registry seam (TUI rides this in Phase 6)
│   │           ├── profile_add.rs / profile_list.rs / profile_use.rs
│   │           └── version.rs      # version action (CLI + gateway check)
│   ├── ignition-cli/
│   │   └── src/
│   │       ├── main.rs             # try_parse → dispatch → single exit point
│   │       ├── cli.rs              # Cli derive: global args + subcommands
│   │       ├── completions.rs      # clap_complete::aot::generate
│   │       └── render.rs           # human (non-JSON) output — never in core
│   └── ignition-tui/
│       └── src/lib.rs              # STUB: `pub fn run() -> anyhow-free placeholder`
│                                   # no ratatui dep until Phase 6
└── webdev/                         # empty scaffold dir + README pointing at Phase 5
```

**Key placement rules (the Phase-6-enabling invariants):**
- Everything in `ignition-core` compiles without clap and without ratatui. Core never prints to stdout — it returns models; the bin renders.
- Actions take `&dyn GatewayApi` (or generic `A: GatewayApi`) + typed params → return serde models. CLI handlers and (later) the TUI call the *same* action fns. This is the ARCHITECTURE.md layering invariant, made structural in Phase 1.
- `ignition-tui` is created as a **stub lib in Phase 1** (so the workspace shape and feature plumbing are final) with zero TUI deps; `ignition-cli` gets `[features] default = ["tui"]`, `tui = ["dep:ignition-tui"]` and a `tui` subcommand stub that errors "TUI arrives in a later phase" — proving the feature gate compiles lean both ways from day one.

### Pattern 1: Global clap args (one definition, propagated)

**What:** `--profile`, `--json`, `--compact`, `--yes`, `--verbose` defined once on the root `Cli` struct with `global = true`; subcommand structs never redeclare them.
**When to use:** every subcommand, by construction.
**Gotchas (verified / flagged):**
- Global args propagate down and their values propagate back up once used (docs.rs/clap `Arg::global`). Define them ONLY at top level.
- MEDIUM-confidence gotcha (training-data flag, cheap to avoid): clap historically rejects `required = true` combined with `global = true`. All five of our globals are optional-with-default anyway (`bool` flags via `ArgAction::SetTrue`, `--profile` as `Option<String>`), so the constraint never bites. Do not mark any global arg required.
- Usage errors (unknown flag, missing positional) exit **2** from clap's own `Error::exit()` before our code runs — this is *by design* in the contract. Under `--json`, usage errors are NOT JSON-enveloped (clap can't know about `--json` if parsing itself failed). Document this in the README taxonomy ("exit 2 = clap usage error, rendered by clap"); do not build a clap error hook for it in Phase 1.

```rust
// crates/ignition-cli/src/cli.rs
use clap::{ArgAction, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "ign", version, propagate_version = true,
          about = "Operate Ignition 8.3+ gateways from the terminal")]
pub struct Cli {
    /// Gateway profile to use (default: active profile in config)
    #[arg(long, global = true, value_name = "NAME")]
    pub profile: Option<String>,

    /// Machine-readable JSON output (stable field names)
    #[arg(long, global = true, action = ArgAction::SetTrue)]
    pub json: bool,

    /// One-line compact JSON (implies --json)
    #[arg(long, global = true, action = ArgAction::SetTrue)]
    pub compact: bool,

    /// Non-interactive confirmation for destructive operations
    #[arg(long, short = 'y', global = true, action = ArgAction::SetTrue)]
    pub yes: bool,

    /// Increase diagnostics (-vv for HTTP trace) to stderr
    #[arg(long, short = 'v', global = true, action = ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Commands,
}
```

**Env equivalents (test + agent path):** `--profile` ← `IGNITION_PROFILE`, `--yes` ← `IGNITION_YES=1`, `--json` ← `IGNITION_JSON=1`. Resolve env→flag in `main()` before dispatch (one `fn apply_env_defaults(cli: &mut Cli)`), so subcommand code only ever reads the struct — single precedence point, trivially testable.

### Pattern 2: Config discovery + env overlay

**What:** TOML config at the platform-correct path, with an explicit override, and env layered on top.
**Precedence (final):** CLI flag > `IGNITION_*` env > profile value > built-in default.

```toml
# Config file (discovery order):
# 1. $IGNITION_CLI_CONFIG          (explicit path — scripts and tests)
# 2. <ProjectDirs::config_dir>/ignition-cli/config.toml
#    macOS:   ~/Library/Application Support/ignition-cli/config.toml
#    Linux:   ~/.config/ignition-cli/config.toml
#    Windows: %APPDATA%\ignition-cli\config.toml
active = "dev"

[profiles.dev]
url = "http://localhost:9088"
ssl_verify = false
auth = { token_env = "IGNITION_TOKEN" }   # secret REFERENCE, never a secret value

[profiles.prod]
url = "https://gw.example.com:8443"
auth = { keyring = "profile:prod" }       # keyring user under service "ignition-cli"
                                          # basic fallback: auth = { user_env = "IGNITION_USER", password_env = "IGNITION_PASSWORD" }
```

```rust
// crates/ignition-core/src/config/mod.rs (shape, not full impl)
#[derive(serde::Deserialize, serde::Serialize, Default)]
pub struct Config {
    pub active: Option<String>,
    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,  // BTreeMap => deterministic list output
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct Profile {
    pub url: url::Url,
    #[serde(default = "default_true")]
    pub ssl_verify: bool,
    #[serde(default)]
    pub auth: AuthRef,
}

pub fn config_path() -> PathBuf {
    std::env::var_os("IGNITION_CLI_CONFIG").map(PathBuf::from)
        .unwrap_or_else(|| {
            let dirs = directories::ProjectDirs::from("", "", "ignition-cli")
                .expect("no home directory");
            dirs.config_dir().join("config.toml")
        })
}
```

**Rules:**
- Create the file with `0600` perms on first write (`std::os::unix::fs::OpenOptionsExt::mode(0o600)`; on Windows, normal ACLs) — PITFALLS 3.6 prevention, verified-by-test in 01-02.
- Env overlay applies per-key: `IGNITION_URL` overrides the *active profile's* URL, `IGNITION_PROFILE` selects the profile, `IGNITION_TOKEN` satisfies auth regardless of `auth` ref. Keep the overlay to those three in Phase 1 (URL/profile/token) + `IGNITION_JSON`/`IGNITION_YES`/`IGNITION_LOG` as behavior flags. Legacy interop names (`GATEWAY_ADMIN_USERNAME/PASSWORD`) are a Phase-4 rig concern — do NOT wire them in Phase 1.
- Unknown keys in config.toml: warn (tracing), don't fail. Missing `active` + no `--profile` = config error with hint "run `ign profile add`".

**⚠ macOS test gotcha (verified from directories 6.0 docs):** `ProjectDirs` uses Apple Standard Directories on macOS — `XDG_CONFIG_HOME` is **ignored**, config lands in `~/Library/Application Support/`. Every config-related test must either set `IGNITION_CLI_CONFIG` to a `tempfile` path (preferred — deterministic on all OSes) or sandbox `HOME`. Prescribe: all tests use `IGNITION_CLI_CONFIG`; the `directories` path is only exercised by one smoke test.

### Pattern 3: SecretStore trait + resolution order + redaction (the blocker resolution)

**What:** secrets resolved through one trait with a strict order; keyring is one impl, not an assumption.

```rust
// crates/ignition-core/src/config/secret.rs

/// A value that must never render. No Serialize impl exists on purpose.
#[derive(Clone)]
pub struct Secret(String);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("Secret(***)") }
}
impl std::fmt::Display for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("***") }
}
impl Secret {
    pub fn expose(&self) -> &str { &self.0 }   // the ONLY way to read it — auditable by grep
}

pub trait SecretStore: Send + Sync {
    /// Ok(None) = not found here; Err = found-but-unreadable (surface with hint)
    fn resolve(&self, profile: &str, auth: &AuthRef) -> Result<Option<Secret>, CoreError>;
}

pub struct EnvStore;        // IGNITION_TOKEN_<PROFILE_UP>, IGNITION_TOKEN, IGNITION_USER/PASSWORD
pub struct KeyringStore;    // keyring::Entry::new("ignition-cli", &format!("profile:{profile}"))

pub fn resolve_secret(profile: &str, auth: &AuthRef, stores: &[Box<dyn SecretStore>])
    -> Result<Secret, CoreError>
{
    for s in stores {
        match s.resolve(profile, auth)? {
            Some(sec) => return Ok(sec),
            None => continue,
        }
    }
    Err(CoreError::SecretUnavailable { profile, /* hint: */ })
    // hint: "set IGNITION_TOKEN (or token_env in the profile), or store a keyring \
    //        entry: service 'ignition-cli', user 'profile:<name>'. On headless Linux \
    //        without a secret service, env vars are the supported path."
}
```

**Resolution order (lock this):** `IGNITION_TOKEN_<PROFILE>` (profile-specific env) → per-profile `token_env` name → generic `IGNITION_TOKEN` → keyring entry → `IGNITION_USER`+`IGNITION_PASSWORD` basic pair → error with the hint above.

**Why keyring fails fast on headless Linux (verified):** keyring 4.1's default feature set is `v1` → on Linux that wires `zbus-secret-service-keyring-store` (verified from keyring-rs Cargo.toml). `Entry::new` checks the store-initialization result and returns `Error::NoDefaultStore` / platform errors **at creation time** when no D-Bus Secret Service exists — no hang, no silent failure. Map that error in `KeyringStore` to `Ok(None)`+warning (try next store) *only* for the no-store condition; a real store error (entry locked) should surface. Practical simplification: treat any `Entry::new` failure as "keyring unavailable here" + tracing warn — keyring is a fallback anyway.

**Redaction guarantee (CORE-02, verified by test):**
1. `Secret` has no `Serialize` — it cannot appear in JSON output at the type level.
2. `Debug`/`Display` render `***` — tracing logs are safe by construction. tracing-subscriber formats `{:?}` — the redacted impls cover it.
3. HTTP logging (when `-vv`): log method/URL/status, never headers. If header logging is ever added, it must be behind `IGNITION_CLI_DEBUG_SECRETS=1` (PITFALLS 4.4).
4. **The test:** write a config with a token env var set to a canary string (`CANARY-t0k3n`), run `ign profile list --json` (and `--verbose`), assert canary absent from stdout AND stderr (`Command::output` + `assert!(!stdout.contains(CANARY))`). This is the acceptance test for CORE-02.

### Pattern 4: Error taxonomy → envelope → exit codes (the contract)

**What:** one thiserror enum in core; one envelope shape; one exit-mapping in `main()`. Golden-tested.

**Final exit-code table (reconciles ROADMAP wording with STACK.md numbering — LOCK THIS before writing golden files):**

| Code | Slug class | Meaning | Examples |
|------|-----------|---------|----------|
| 0 | — | success | — |
| 1 | `internal` | unexpected runtime failure (catch-all; report as bug) | panic-caught, serde surprise |
| 2 | `usage` | clap usage error (clap renders it, not us) | unknown flag, missing arg; also `--yes` required for destructive op |
| 3 | `config` | local configuration problem | no profiles, unknown profile name, unreadable TOML, secret unavailable |
| 4 | `network` | gateway unreachable / timeout / TLS | connection refused, timeout, DNS |
| 5 | `auth` | gateway rejected credentials | 401, 403 (hint points at the three-part token setup per PITFALLS 1.1) |
| 6 | `target_state` | gateway reachable but command invalid in current state | gateway version < 8.3.1, project not found, rig not running |
| 7 | `rig` | docker/compose failure | reserved; first used Phase 4 |

(Deltas vs STACK.md: config split out of 1/3 and renumbered per CORE-04's "distinct config, auth, network, target-state"; nothing is built yet so renumbering is free. `--yes`-required maps to 2 because it names a flag the user must add — same class as usage.)

```rust
// crates/ignition-core/src/error.rs
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("profile {name:?} not found; known: {known}")]
    ProfileNotFound { name: String, known: Vec<String> },        // exit 3
    #[error("secret unavailable for profile {profile:?}")]
    SecretUnavailable { profile: String },                        // exit 3
    #[error("gateway unreachable at {url}: {source}")]
    Network { url: String, #[source] source: reqwest::Error },    // exit 4
    #[error("gateway rejected credentials ({status})")]
    Auth { status: u16 },                                         // exit 5
    #[error("gateway version {found} is below minimum {minimum}")]
    GatewayTooOld { found: String, minimum: String },             // exit 6
    // ...
}

impl CoreError {
    pub fn code(&self) -> &'static str {   // stable slug — part of the public contract
        match self { Self::ProfileNotFound { .. } => "profile_not_found", /* ... */ }
    }
    pub fn exit_code(&self) -> u8 { /* table above */ }
    pub fn hint(&self) -> Option<String> { /* e.g. GatewayTooOld → "upgrade the gateway or pin a lower CLI expectation" */
                                             /* Auth → "check token; three-part setup: security level + write permissions + token assignment" */ }
}

#[derive(serde::Serialize)]
pub struct ErrorEnvelope<'a> {
    pub ok: bool,                       // always false here
    pub profile: Option<&'a str>,       // active profile, echoed in EVERY output (CORE-01)
    pub error: ErrorBody<'a>,
}
#[derive(serde::Serialize)]
pub struct ErrorBody<'a> {
    pub code: &'a str,                  // stable slug, e.g. "gateway_too_old"
    pub message: String,
    pub endpoint: Option<String>,       // URL/path when a request was involved (CORE-05)
    pub hint: Option<String>,           // actionable next step (CORE-05)
}
```

**Envelope decisions (lock these):**
- Success stdout (`--json`): `{"ok":true,"profile":"dev","data":{...}}` — `profile` at top level satisfies CORE-01 "active profile visible in every output" for machines; human mode shows it as a prefix/header.
- Errors go to **stderr** in both modes (human-readable by default, the JSON envelope under `--json`); stdout stays data-only-on-success. Exit code carries the same signal. This resolves the STACK-vs-PITFALLS 4.1 stdout/stderr ambiguity in favor of clig.dev discipline.
- `--compact` implies `--json`, renders `serde_json::to_string` (one line) — for both data and error envelope.

```rust
// crates/ignition-cli/src/main.rs — the single exit point
fn main() -> ExitCode {
    let mut cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => e.exit(),                       // clap: 2 for usage, 0 for help/version
    };
    apply_env_defaults(&mut cli);                  // IGNITION_PROFILE / IGNITION_JSON / IGNITION_YES
    init_tracing(cli.verbose);                     // stderr only
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    let result = runtime.block_on(dispatch(cli));  // Result<ActionOutput, CoreError>
    match result {
        Ok(out) => { render(out); ExitCode::SUCCESS }
        Err(err) => {
            render_error(&err);                    // envelope to stderr (JSON under --json)
            ExitCode::from(err.exit_code())
        }
    }
}
```

### Pattern 5: `GatewayApi` trait seam + wiremock harness

**What:** a coarse trait so actions never touch reqwest types, and tests inject either a mock impl (unit) or a wiremock server (HTTP contract).

```rust
// crates/ignition-core/src/client/mod.rs
#[async_trait::async_trait]   // or use Rust 1.75+ RPITIT; async-trait is the boring choice — pick one, be consistent
pub trait GatewayApi: Send + Sync {
    async fn gateway_info(&self) -> Result<GatewayInfo, CoreError>;
    // Phase 2 grows this: status, modules, logs... keep it COARSE (per capability, not per endpoint)
}

pub struct ReqwestGatewayApi { base: url::Url, auth: AuthHeaders, client: reqwest::Client }

impl ReqwestGatewayApi {
    pub fn new(profile: &Profile, secret: Secret) -> Result<Self, CoreError> {
        let mut builder = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30));               // per-class timeouts land Phase 2 (PITFALLS 1.10)
        if !profile.ssl_verify {
            builder = builder.danger_accept_invalid_certs(true);  // dev-rig only, per-profile, never global
        }
        // ...
    }
}
```

Phase 1 implements exactly ONE method (`gateway_info`) — enough to drive `ign version` and prove auth-header construction + error mapping end-to-end against wiremock:

```rust
// crates/ignition-core/tests/gateway_info_contract.rs
#[tokio::test]
async fn sends_token_header_and_parses_version() {
    let server = wiremock::MockServer::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/gateway-info"))
        .and(wiremock::matchers::header("X-Ignition-API-Token", "test-token"))
        .respond_with(wiremock::ResponseTemplate::new(200)
            .set_body_json(serde_json::json!({"version":"8.3.2","edition":"Standard","state":"RUNNING"})))
        .expect(1)
        .mount_as_scoped(&server)
        .await;
    let api = ReqwestGatewayApi::for_tests(&server.uri(), /* token */);
    let info = api.gateway_info().await.unwrap();
    assert_eq!(info.version, "8.3.2");
    guard.received_requests().await; // expectation verified on drop
}
```

Auth-header rule (verified from ignition-mcp `ignition_client.py:47-52`): `X-Ignition-API-Token: <token>` when a token resolved, else `Authorization: Basic <b64(user:pass)>`. Never both.

### Pattern 6: `ign version` + gateway check

**What:** CLI version always; gateway check when a profile resolves; hard refusal on <8.3.1.
**Verified endpoint:** `GET /data/api/v1/gateway-info` returns `{version, edition, state, uptime}` (shape confirmed in ignition-mcp `docs/api-reference.md:57`) and is marked `auth: none` in the local 83-api Bruno collection — the version check works **without credentials** (send the token anyway when present; harmless). MEDIUM-HIGH confidence unauthenticated on a real gateway; confirm in Phase 2 live check, design so auth is optional on this call.

```rust
pub const MIN_GATEWAY: &str = "8.3.1";

fn below_minimum(raw: &str) -> bool {
    // gateway versions are plain dotted triples ("8.3.1"), sometimes with suffixes
    let clean = raw.split(['-', ' ']).next().unwrap_or(raw);
    match semver::Version::parse(&format!("{clean}.0")) {  // tolerate "8.3"
        Ok(v) => v < semver::Version::parse("8.3.1.0").unwrap(),
        Err(_) => true,  // unparseable => refuse safely (exit 6) rather than guess
    }
}
```

Behavior matrix (lock):
- No config/profiles → print CLI version, exit 0 (version must work on a fresh install).
- Profile resolves, gateway reachable, version ≥ 8.3.1 → both versions, exit 0.
- Profile resolves, version < 8.3.1 or unparseable → `gateway_too_old` envelope (exit 6) with hint "upgrade gateway to ≥8.3.1; see `ign doctor` (Phase 2)".
- Profile resolves, gateway **unreachable** → print CLI version + warning envelope field, **exit 0** (a version command that hard-fails on a down gateway is hostile in scripts); the refusal contract applies only when the gateway answered.

### Pattern 7: Shell completions

Verified on docs.rs/clap_complete 4.6.9 (fetched 2026-08-21): runtime generation via `clap_complete::aot::{generate, Shell}`; `Shell` implements the value-parser trait so it drops straight into a clap arg (`value_parser!(Shell)` accepts bash/zsh/fish/…). The `unstable-dynamic` engine (`COMPLETE=$SHELL`) exists but is flag-gated — **don't use it in Phase 1**.

```rust
// crates/ignition-cli/src/completions.rs
use clap::CommandFactory;
use clap_complete::aot::{generate, Shell};

pub fn completions(shell: Shell) {
    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "ign", &mut std::io::stdout());
}
// Subcommand: `ign completions <SHELL>`; add `#[command(arg_required_else_help = true)]`.
```

Note: completions print to **stdout regardless of `--json`** (scripts pipe them) — document as the one sanctioned exception.

### Anti-Patterns to Avoid

- **Error envelope on stdout** — stdout is data-only on success; envelopes go to stderr. Mixed streams are the #1 agent-parsing failure.
- **Per-subcommand exit-code ad-hocery** — every `std::process::exit` outside `main()` is a contract bug. One mapping point.
- **Secrets as `String` in scope** — a bare `String` token will eventually be logged. `Secret` newtype + `expose()` only at the reqwest header-construction site.
- **Prompting ever** — Phase 1 has zero interactive prompts; `profile add` takes flags. The confirm-guard helper exists only so later phases inherit the pattern.
- **Testing with the real config dir** — always `IGNITION_CLI_CONFIG` → tempfile; never `~`.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Config-dir resolution | `~/.config` string building | `directories` 6.0 `ProjectDirs` | Cross-platform correctness (Apple Standard Dirs, Known Folders); we only hand-roll the TOML+env layer, which is genuinely ~100 lines |
| Shell completion scripts | Emit bash/zsh/fish by hand | `clap_complete::aot` | Generated from the live clap definition — never drifts from actual flags |
| Snapshot review workflow | Manual fixture editing + string compare | `snapbox` (`SNAPSHOTS=overwrite`) | Structured diffs, dynamic-value redactions, used by clap itself |
| HTTP contract tests | `TcpListener` + hand-rolled HTTP | `wiremock` | Matchers/expectations/received-requests; verified API covers auth-header assertions |
| Usage-error rendering | Custom flag validation code | clap defaults (`Error::exit`) | Exit 2 for free; rewriting it will diverge from clap's help output |
| Version comparison | String compare `>= "8.3.1"` | `semver::Version` (with suffix tolerance) | `"8.3.10"` vs `"8.3.9"` string-compares wrong |

## Common Pitfalls

### Pitfall 1: MSRV drift breaks CI on day one
**What goes wrong:** workspace declares `rust-version = "1.85"` (STACK.md) but keyring 4.1.x requires 1.88 → build fails for users/CI pinned at 1.85.
**Why:** STACK.md derived MSRV from toml only; keyring's floor wasn't checked.
**How to avoid:** set `rust-version = "1.88"` in `[workspace.package]` day one (verified floor: keyring 1.88 > clap/toml/reqwest/snapbox 1.85 > thiserror 1.71).
**Warning signs:** `cargo build` error mentioning `package `keyring v4.1.x` cannot be built because it requires rustc 1.88`.

### Pitfall 2: Keyring on headless Linux (the STATE.md blocker)
**What goes wrong:** `keyring::Entry::new` returns `NoDefaultStore`/platform error on Linux without a D-Bus Secret Service (default `v1` feature wires zbus secret-service store). Naive code treats this as a fatal config error or, worse, the CLI hangs waiting on dbus.
**Why:** CI runners and SSH rigs have no gnome-keyring.
**How to avoid (three layers):**
1. **Env-first resolution** — the default CI/test path never constructs a keyring `Entry` (Pattern 3 order). Zero secret service needed.
2. **`SecretStore` seam** — `KeyringStore` maps `Entry::new` failure → "store unavailable" (warn + skip), not fatal.
3. **Dedicated smoke job** (the actual smoke-test from STATE.md) using keyring-rs's own verified CI recipe:
```yaml
# .github/workflows/ci.yml — keyring smoke job
keyring-smoke:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v7
    - uses: actions-rust-lang/setup-rust-toolchain@v1
      with: { toolchain: stable }
    - name: gnome-keyring        # recipe verbatim from keyring-rs CI (fetched 2026-08-21)
      run: |
        sudo apt-get update
        sudo apt-get install -y gnome-keyring
        gnome-keyring-daemon --components=secrets --daemonize --unlock <<< 'foobar'
    - run: cargo test -p ignition-core --test keyring_smoke -- --ignored
```
macOS runner gets keyring tests for free (Keychain present); keep them `#[cfg(not(target_os = "linux"))]`-gated or `#[ignore]`+explicitly-invoked so default `cargo test` never depends on a daemon.
**Warning signs:** CI log lines about `zbus`/`dbus` session bus; `NoDefaultStore` in test output.
**Confidence:** HIGH (docs + keyring-rs's own workflow file fetched).

### Pitfall 3: `directories` ignores XDG on macOS → tests hit the real `~/Library`
**What goes wrong:** config tests set `XDG_CONFIG_HOME` on a macOS dev machine; `directories` doesn't read it; tests read/write the developer's real config or fail mysteriously.
**How to avoid:** every test sets `IGNITION_CLI_CONFIG` to a tempfile path (deterministic on every OS); one dedicated test asserts the platform path derivation separately.
**Warning signs:** tests pass on Linux CI, fail/flake locally on macOS; `~` config mutated by test runs.

### Pitfall 4: Golden files with dynamic values (paths, durations, versions)
**What goes wrong:** `profile list --json` includes absolute temp paths or timestamps → golden test fails on every machine/CI run → team deletes the tests.
**How to avoid:** (a) design output models to exclude ambient noise (don't serialize the config path in output); (b) where unavoidable, snapbox `Redactions`/regex substitution (`[..]` elision in `file![]` fixtures); (c) assert `serde_json::Value` structure (parse actual + fixture, compare `Value`s) for shape-stability tests where exact string goldens are overkill.
**Warning signs:** golden files containing `/tmp/`, ISO timestamps, or durations.

### Pitfall 5: Exit-code taxonomy drift between docs and binary
**What goes wrong:** README documents 3=network but code returns 3=config; agents branch wrong. (This almost already happened: STACK.md's table lacks the config code CORE-04 requires.)
**How to avoid:** the table exists in exactly TWO places — `CoreError::exit_code()` and the README/help — and a golden test enumerates the mapping (`assert_eq!(CoreError::ProfileNotFound{..}.exit_code(), 3)` plus a binary-level `--json` failure per class asserting code + envelope slug).
**Warning signs:** any `exit(` call outside `main()`; envelope slugs changing spelling between releases.

### Pitfall 6: Global args + `--compact` before `--json`
**What goes wrong:** `--compact` tested only with `--json` present (or only absent); three render modes (human/pretty/compact) drift.
**How to avoid:** define the precedence once — `--compact` **implies** `--json` — and golden-test all three modes for at least `profile list` and one error case.

### Pitfall 7: `deny_unknown_fields` on config structs
**What goes wrong:** adding a config key in a later phase hard-crashes older-config users (or vice versa); also blocks forward-compat comments-in-config patterns.
**How to avoid:** no `deny_unknown_fields` on config or output models; unknown keys ignored (+ warn in config, silent in gateway payloads). Stability comes from golden tests, not serde strictness.

### Pitfall 8: Testing keyring code paths on machines with a working keychain silently mutates user state
**What goes wrong:** keyring unit tests run against the *developer's real* macOS Keychain (prompts appear during `cargo test`; CI macOS runners work but leave state).
**How to avoid:** the `KeyringStore` impl is covered by the dedicated smoke tests only; all other tests use `EnvStore`/in-memory stores via the trait.

## Code Examples

### Golden-file contract test (snapbox + assert_cmd hybrid)

```rust
// crates/ignition-cli/tests/contract_profile_list.rs
use assert_cmd::prelude::*;
use std::process::Command;

#[test]
fn profile_list_json_envelope_shape() {
    let cfg = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(&cfg, r#"
active = "dev"
[profiles.dev]
url = "http://localhost:9088"
"#).unwrap();

    let out = Command::cargo_bin("ign").unwrap()
        .env("IGNITION_CLI_CONFIG", cfg.path())
        .args(["--json", "profile", "list"])
        .output().unwrap();
    assert!(out.status.success());

    snapbox::Assert::new()
        .action_env("SNAPSHOTS")
        .eq(
            String::from_utf8_lossy(&out.stdout),
            snapbox::str![[r#"
{
  "ok": true,
  "profile": "dev",
  "data": [..]
}
"#]],
        );
    // exit-code taxonomy: unknown profile
    let out = Command::cargo_bin("ign").unwrap()
        .env("IGNITION_CLI_CONFIG", cfg.path())
        .args(["--profile", "nope", "--json", "version"])
        .output().unwrap();
    assert_eq!(out.status.code(), Some(3));          // config
    let body: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(body["error"]["code"], "profile_not_found");
    assert!(body["error"]["hint"].is_string());       // CORE-05
}
```
Update goldens with `SNAPSHOTS=overwrite cargo test` then review the diff (snapbox's documented workflow, verified docs.rs 1.2.2).

### CI workflow (workspace + matrix + smoke)

```yaml
name: CI
on: { push: { branches: [main] }, pull_request: {} }
jobs:
  check:
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v7
      - uses: actions-rust-lang/setup-rust-toolchain@v1
        with: { toolchain: stable, components: rustfmt, clippy }
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo build --workspace
      - run: cargo test --workspace            # env-store paths; no keyring daemon needed
      - run: cargo build -p ignition-cli --no-default-features   # prove lean/agent build
  keyring-smoke:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: actions-rust-lang/setup-rust-toolchain@v1
        with: { toolchain: stable }
      - run: |
          sudo apt-get update
          sudo apt-get install -y gnome-keyring
          gnome-keyring-daemon --components=secrets --daemonize --unlock <<< 'foobar'
      - run: cargo test -p ignition-core --test keyring_smoke -- --ignored
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| keyring 3.x monolithic crate | keyring 4.x: keyring-core + pluggable stores; `v1` feature restores classic `Entry` API as default | v4.0 Apr 2026 / v4.1 Jun 2026 | Default Linux store = zbus Secret Service; headless needs the recipe above |
| `clap_complete::shells` / `generator` module | `clap_complete::aot` (old paths deprecated); dynamic engine is `unstable-dynamic` | 4.6 line | Import from `aot` |
| MSRV 1.85 assumption (STACK.md) | 1.88 floor (keyring) | keyring 4.1.1+ (Jun 2026) | Set workspace rust-version now |
| String/fixture-only golden tests | snapbox 1.x with `SNAPSHOTS` action-env + Redactions | 1.2.x current (Jul 2026) | Reviewable golden updates, clap's own stack |
| `anyhow`-style main | `main() -> ExitCode` + single error mapping | stable since Rust 1.61 | Required for the exit-code contract |

**Deprecated/outdated to avoid:** `exitcode` crate (dead since 2017); `figment` (stale, per STACK.md); keyring's `cli` feature just for `use_sample_store` (heavy Linux deps — verified from its Cargo.toml feature graph); `directories`-competing `dirs` crate flip-flopping (keep `directories`).

## Open Questions

1. **Is `GET /data/api/v1/gateway-info` truly unauthenticated on a live 8.3.1 gateway?**
   - What we know: 83-api Bruno collection marks it `auth: none`; ignition-mcp calls it with auth attached.
   - What's unclear: whether a hardened gateway (strict security) still serves it anonymously.
   - Recommendation: design `version` to attach credentials when available and not require them on a 200; verify empirically in Phase 2's live-gateway check (already a flagged gap).
2. **`async_trait` vs RPITIT for `GatewayApi`** — both fine on Rust 1.88. Recommendation: `async_trait` (dyn-compatible today, ubiquitous); revisit only if the TUI needs `dyn` avoidance. Planner picks once.
3. **Envelope top-level shape** (`{"ok","profile","data"}`) — research-recommended, not user-locked. The planner should treat the exact field set as the Phase-1 "API freeze" moment; changing it later is a breaking change for agents.
4. **`ign version` on unreachable gateway: exit 0 + warning vs exit 4.** Recommendation: exit 0 + warning (version is a local-info command; hard-failing scripts on a sleeping rig is hostile). Flag for one-line user confirmation at review time.
5. **Windows in the CI matrix** — target list says macOS/Linux primarily. Recommendation: skip Windows CI in Phase 1 (code paths stay `cfg(unix)`-clean where perms matter); revisit if distribution demands it.

## Sources

### Primary (HIGH confidence)
- **Context7 `/open-source-cooperative/keyring-rs`** — v4 architecture (`default = ["v1"]`, zbus secret-service default on Linux), `Entry` API source (`src/v1.rs`), headless/SSH guidance ("use the sample store… Secret Service requires D-Bus, generally unsuitable for headless"), `cli` feature's store-selection fns, Linux dependency/feature graph from its Cargo.toml.
- **keyring-rs `.github/workflows/ci.yaml`** (raw fetch 2026-08-21) — the gnome-keyring smoke recipe (verbatim above), runner matrix, MSRV job at 1.88.
- **crates.io `/crates/{keyring,clap,clap_complete,toml,wiremock,snapbox,thiserror,reqwest,directories,assert_cmd,predicates}/versions`** (fetched 2026-08-21) — versions + `rust_version` fields; keyring 4.1.1–4.1.6 = 1.88.0.
- **docs.rs/clap_complete 4.6.9** (fetched) — `aot::{generate, generate_to, Shell}`, Shell-as-value-parser example, `unstable-dynamic` gating.
- **docs.rs/snapbox 1.2.2** (fetched) — cmd/json/regex features, `action_env("SNAPSHOTS")`, `file![]`/`str![]` macros, `Redactions`, "which tool is right" guidance (assert_cmd vs snapbox vs trycmd).
- **docs.rs/directories 6.0.0** (fetched) — ProjectDirs per-platform semantics (XDG on Linux, Standard Directories on macOS, Known Folders on Windows).
- **Context7 `/websites/rs_clap`** — `Arg::global` propagation semantics, `from_global` derive, usage-error exit-2 behavior.
- **Local 83-api Bruno collection** — `gateway-info/Gateway Info.bru` (`auth: none`) + ignition-mcp `ignition_client.py:47-52` (auth header construction), `docs/api-reference.md:57` (response shape).
- **Local `.planning/research/{STACK,ARCHITECTURE,FEATURES,PITFALLS,SUMMARY}.md`** — all stack decisions, envelope/exit-code discipline, rig conventions, redaction requirements.
- **Context7 `/lukemathwalker/wiremock-rs`** — MockServer/matchers/ResponseTemplate/expect/mount_as_scoped/received_requests API.

### Secondary (MEDIUM confidence)
- "clap global args cannot be required" — training-data recall, not re-verified against current docs this session; mitigation (optional-with-default globals) is the pattern regardless.
- `use_sample_store` being exclusive to the `cli` feature — from keyring-rs `_autodocs` module structure; we avoid needing it anyway.

### Tertiary (LOW confidence)
- Whether zbus connection attempts without a session bus are always *fast* failures (could be slow-timeout on exotic setups) — our resolution order makes this a warning path, not a hang risk, but observe during the smoke test.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — every version/MSRV verified live; keyring blocker resolved with the upstream project's own CI recipe.
- Architecture: HIGH — clap-global-args, config discovery, SecretStore seam, envelope/exit mapping all verified against official docs; envelope field set is a recommendation pending planner lock.
- Pitfalls: HIGH — keyring headless (docs + CI), directories-macOS (docs), MSRV (crates.io); clap-global-required flagged MEDIUM with avoidance built in.

**Research date:** 2026-08-21
**Valid until:** 2026-09-21 (crate versions stable; keyring 4.x actively evolving — recheck its release notes if Phase 1 starts later)
