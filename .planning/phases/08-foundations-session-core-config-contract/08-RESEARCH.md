# Phase 8: 08-foundations-session-core-config-contract - Research

**Researched:** 2026-09-04
**Domain:** ignition-cli codebase state verification (Session seam, config schema, contract tests, TUI worker cadence)
**Confidence:** HIGH (every claim below was read out of the actual source files on this date; no external/library research was needed — the stack is settled)

No CONTEXT.md exists for this phase — the roadmap flags say "pure refactor of source-verified code … skip research-phase." This document therefore contains **no user constraints section** and **no library selection**: it is a verified map of the exact code Phase 8 touches, the exhaustive construction-site inventory, and the gotchas the planner must design around.

## Summary

Phase 8 has four workstreams, and all four land on seams that already exist in near-final shape. The **Session seam (CORE-09)** is a promotion, not an invention: every auth/gateway client in the tree is built through exactly one core constructor (`ReqwestGatewayApi::new(profile, credential)`, `client/mod.rs:398`), and the resolution choreography around it is duplicated in two private-fn clusters — 8 call sites in `crates/ignition-cli/src/main.rs` (lines 431, 760, 1248, 2000, 2167 via `resolve_gateway_api`/`named_profile_client`/`resolve_headerless_api`, 2185, 2232, 2266) and 2 in `crates/ignition-tui/src/context.rs` (lines 81, 127). The v1.1 research already prescribed the shape (`.planning/research/ARCHITECTURE.md` Pattern 2: `Session::resolve(profile_flag) -> Result<Session, CoreError>` holding profile name + `Arc<dyn GatewayApi>`); Phase 8 executes it.

The **config schema (CORE-10/TUIX-05)** attaches to two structs that today have neither key: `Config` (no `ui` field) and `Profile` (no `poll_interval_secs`), in `crates/ignition-core/src/config/profile.rs:16,65`. Two things the planner must know cold: (1) invalid TOML is currently a **hard error** (`ConfigInvalid`, exit 3) at `config::load`, and the TUI inherits it — `context::resolve` propagates before `ratatui::init`, so today a config typo **does** kill TUI startup; success criterion 1 changes that for the TUI path specifically. (2) The `KNOWN_TOP_LEVEL`/`KNOWN_PROFILE_KEYS` warn-lists in `config/mod.rs:71-73` must gain `"ui"` / `"poll_interval_secs"` or the new keys load-but-warn as unknown.

The **contract rituals (CORE-11)** all have verified anchors: the exit-code enum test `exit_code_mapping_enumerated` is a unit test at `error.rs:846`; the README table is `README.md:37-52` (with the two-places prose at lines 50-52); the OutOfBand registry is the `Mapping` enum + `routes()` table in `crates/ignition-tui/src/routes.rs` with the pinned test `out_of_band_rows_are_exactly_the_completions_leaf` at `crates/ignition-cli/tests/tui_coverage.rs:119-131`; the stdout-purity harness has a working precedent to copy (`cli_chassis.rs` spawns the real binary via `Command::cargo_bin("ign")` and already byte-pins exact stdout). One structural fact constrains the OutOfBand extension: the bidirectional clap-walk test makes registry rows for `mcp`/`lsp`/`edit` **impossible until those clap commands exist** (they would be orphan rows) — so the Phase 8 extension is taxonomy + pinned-test + justification text, not rows.

**Primary recommendation:** Plan this as four independently verifiable slices in dependency order — (1) config schema + validation + TUI degradation, (2) `Session` in core with both front-ends migrated onto it, (3) `spawn_refresh`/`rebuild` parameterization reading the configured interval, (4) contract test hardening — because (3) reads what (1) wrote and (2) is the criterion-4 "no second client construction" sweep that must grep clean at the end.

---

## 1. Session extraction (CORE-09) — client construction sites

### Verified current state

**The ONE constructor in core:** `ReqwestGatewayApi::new(profile: &Profile, credential: Option<Credential>) -> Result<Self, CoreError>` — `crates/ignition-core/src/client/mod.rs:398`. Plus `for_tests(base_url, credential)` at line 408 (test-only). The type holds `base: url::Url`, `credential: Option<Credential>`, `client: reqwest::Client`. The trait is `GatewayApi` (`async_trait`, dyn-compatible, coarse one-method-per-capability) at `client/mod.rs:84`; `Secret::expose` is confined to `apply_auth` (`client/mod.rs:430`).

**Production construction sites — the complete inventory:**

| # | File:line | Path | Shape |
|---|-----------|------|-------|
| 1 | `crates/ignition-cli/src/main.rs:431` | `Commands::Version` arm | `ReqwestGatewayApi::new(&profile, credential)` — credential **degrades to None** (`resolve_secret_opt`) |
| 2 | `main.rs:760` | (wait-family / headerless site) | same degraded shape via `resolve_headerless_api` |
| 3 | `main.rs:1248` | dispatch arm (mid-file) | `.and_then(\|credential\| ReqwestGatewayApi::new(&profile, Some(credential)))` |
| 4 | `main.rs:2000` | `Commands::Script` → `ScriptCommand::Run` | required-credential chain inline |
| 5 | `main.rs:2167` | **`resolve_gateway_api`** (`main.rs:2158-2172`) — THE shared helper, called by `run_inspection` and ~10 command arms | required credential |
| 6 | `main.rs:2185` | **`named_profile_client`** (`main.rs:2178-2186`) — per-side client for `resolve_two_clients` (project diff/sync) | required credential |
| 7 | `main.rs:2232` | **`resolve_headerless_api`** (`main.rs:2223-2237`) — `wait gateway`/`wait restart`, secret degradation to `None` | optional credential |
| 8 | `main.rs:2266` | **`rig_gateway_client`** (`main.rs:2254-2268`) — builds an **ad-hoc Profile** from the rig's derived URL (`ssl_verify=false`, default auth) | rig-family, NOT profile-config |

**TUI construction sites (`crates/ignition-tui/src/context.rs`):**
- `build_client` at line 75-83: `config::resolve_secret(...)` → `ReqwestGatewayApi::new(&profile, Some(credential))` (line 81) → returns `(String, String, Arc<ReqwestGatewayApi>)`. Called by `resolve(profile_flag)` (line 42) and `rebuild(profile_name)` (line 52).
- `rig_client_with` at line 119-127: ad-hoc rig Profile → `ReqwestGatewayApi::new(&profile, credential).ok()` (line 127) — the TUI twin of `main.rs:2254`. Plus `rig_trial_ladder`/`rig_token_only` (env-only credential sourcing).

**The duplicated choreography** (what Session absorbs): `secret_chain()` is defined **twice, byte-identical** — `main.rs:2287-2293` and `context.rs:28-34` (env tokens → keyring → basic pair, the LOCKED order). The overlay→selection sequence is also duplicated (`main.rs:2111-2118 resolve_profile_context` ≡ `context.rs:62-69 resolve_from`). This duplication was a **deliberate v1.0 decision** (06-RESEARCH: "the choke files stay untouched; compose the same public building blocks") that Phase 8 now dissolves by promoting it into core.

**Test-only construction (NOT in scope for the sweep):** ~50 `ReqwestGatewayApi::for_tests(...)` sites in `crates/ignition-core/tests/*_contract.rs`, `crates/ignition-core/src/client/mod.rs:1334` (cfg(test)), and ignition-tui `#[cfg(test)]` modules (`update.rs` ×10, `workers/{refresh,watch,tail,ops}.rs`, `ui/dashboard.rs`). Tests build mock clients directly — they must keep doing so; `Session` is the resolve-from-config path, not a test fixture.

**Something session-like already?** No. There is **no `Session` type today**. Naming-collision inventory for the new type: `ignition_core::client::sessions` (lowercase module — Designer/Perspective/Vision session families), `actions/sessions.rs` + `SessionsResult`, `CoreError::SessionNotPrunable`, and TUI `SessionType`/`SessionRow`. A new top-level `ignition_core::session` module + `Session` struct compiles clean against all of these, but plan docs should disambiguate in prose ("execution session" vs "gateway session").

**What `Session` should hold** (per `.planning/research/ARCHITECTURE.md` Pattern 2, lines 271-291 — the locked design intent): `Session::resolve(profile_flag: Option<&str>) -> Result<Session, CoreError>` holding the profile **name** and `Arc<dyn GatewayApi>`. Design notes verified against the code:
- The three resolution modes are real and distinct: **required** credential (`resolve_gateway_api`), **degraded-to-None** credential (`resolve_secret_opt` — version, waits), and **headerless-by-construction** (rig clients). A `Session::resolve` for the default path plus explicit constructors (e.g. `Session::headerless`, or `resolve` with a policy arg) must preserve all three behaviors — criterion 4 forbids changing behavior, only the construction path.
- `version()` already takes `Option<&dyn GatewayApi>` (`actions/version.rs:46-47`) — the existing precedent for dyn-injected actions. All action fns are free functions over `&dyn GatewayApi` (module map invariant, `core/lib.rs:10-11`), so a `Session` exposing `api()` / `Deref` to `&dyn GatewayApi` feeds every existing action unchanged.
- `main.rs:431`'s comment ("The client is built from the POST-OVERLAY profile — the research-locked precedence must hold at the construction site") is the behavioral contract `Session::resolve` must reproduce: env overlay **scoped to the would-be selection** FIRST, then selection, then secret.
- Construction pattern: `resolve` associated fn (mirrors `context::resolve`) fits existing conventions; no builder exists anywhere in the tree to imitate.

**What must change (criterion 4's sweep target):** all 8 CLI sites + 2 TUI clusters route through `ignition-core::Session`. The rig-family clients (`main.rs:2254`, `context.rs:119`) are a **planner decision**: they don't come from profile config, so either they get a `Session::for_url(url, credential)`-style constructor or remain the documented exception — but criterion 4 says "no second client construction anywhere in the tree," which reads as: they too must route through a Session-owned constructor (the `ReqwestGatewayApi::new` call itself moves behind Session; the ad-hoc Profile assembly can stay at the call site). A completion check: `rg 'ReqwestGatewayApi::new' crates/` must return only the Session module (plus `for_tests` in tests).

---

## 2. Config schema (CORE-10 + TUIX-05)

### Verified current state

**Structs** — `crates/ignition-core/src/config/profile.rs`:
- `Config` (line 16-32): `active: Option<String>`, `profiles: BTreeMap<String, Profile>`, `rig: RigConfig`, `rigs: BTreeMap<String, RigEntry>`. Derives `Debug, Clone, Default, PartialEq, Serialize, Deserialize`. All fields `#[serde(default)]`, empties `skip_serializing_if`.
- `Profile` (line 64-88): `url: url::Url` (required), `label: Option<String>`, `ssl_verify: bool` (`#[serde(default = "default_ssl_verify")]`), `auth: AuthRef` (`#[serde(default)]`, untagged enum TokenEnv/Keyring/Basic with `Default` = generic `IGNITION_TOKEN`), `webdev_secret: Option<String>`. Derives `Debug, Clone, PartialEq, Serialize, Deserialize` — **NOT Default**.
- No `ui` field, no `poll_interval_secs` anywhere. The `Profile` doc-comment (line 1-7) states the no-`deny_unknown_fields` rule explicitly.

**Load semantics** — `config/mod.rs:50-69`: missing file → `Config::default()` (fresh install); invalid TOML → **`CoreError::ConfigInvalid` (exit 3)** — a hard error today. Unknown keys → `tracing::warn!` only (`warn_unknown_keys`, lines 77-111).

**The warn-lists that must grow** — `config/mod.rs:71-73`:
```rust
const KNOWN_TOP_LEVEL: &[&str] = &["active", "profiles", "rig", "rigs"];
const KNOWN_PROFILE_KEYS: &[&str] = &["url", "label", "ssl_verify", "auth", "webdev_secret"];
```
Adding `[ui].theme` / `poll_interval_secs` without updating these makes the new keys load fine (serde default) but **warn as unknown on every load** — a contract test should pin the new keys as warn-silent.

**TUI config flow and the degradation gap** — `ignition_tui/src/lib.rs:53-63`: `run()` calls `context::resolve(profile_flag)?` **before** `ratatui::init()`. `context::resolve` → `config::load(...)?` — so **today an invalid config exits 3 before the terminal is ever touched** (documented as a feature at `lib.rs:48-52`: "Resolution failures return BEFORE the terminal is touched"). Success criterion 1 ("an invalid config degrades to defaults and the TUI still starts") **reverses this for the invalid-config case specifically**. Design tension the planner must resolve (see Open Questions): after degrading to `Config::default()`, there is no active profile, and `context::resolve` currently refuses with `NoActiveProfile` ("the cockpit is a gateway surface and cannot open without a target"). Either the TUI gains a degraded/no-client start state, or degradation keeps a resolvable profile while degrading the *schema* parts. The CLI path must keep hard-erroring (`config_invalid` exit 3 is frozen taxonomy and pinned by goldens — `contract_profile.rs` and `error.rs` tests).

**Where the keys attach:**
- Top-level `[ui].theme`: new `ui: UiConfig` field on `Config` with `#[serde(default)]` (+ `skip_serializing_if` emptiness so existing configs keep their exact on-disk shape — the `rig_config_is_empty` precedent at `profile.rs:36-38`). Phase 8 lands **plumbing only** — theme *rendering* is Phase 12 (TUIX-03/04); do not scope-creep.
- Per-profile `poll_interval_secs`: new field on `Profile`. Recommended shape: `Option<u64>` (or `u64` with a `#[serde(default = ...)]` default of the 5s constant) + `#[serde(default, skip_serializing_if = "Option::is_none")]` so existing profiles round-trip byte-identically. The **clamp is validation, not serde**: criterion 2 says sub-second values are "refused by config validation with a clear error." With an integer field, the refusal case is `0` (and fractional values are already TOML type errors → `ConfigInvalid`). There is **no existing numeric-validation precedent** in the config module — the closest patterns are `default_ssl_verify()` (serde default fn) and `load()`'s post-parse checks; add a `validate()` step in `load()` (or `Profile::validated()`) mapping the failure to a **new clear error**. Exit-class note: refusal should map to the existing `ConfigInvalid` class (exit 3, `config_invalid` slug) or `InvalidInput` (exit 2) — **never a new exit code** (frozen taxonomy; additive slugs only per REQUIREMENTS.md line 75).

**The TUI cadence map (what `poll_interval_secs` parameterizes)** — all verified:

| Constant | Value | File:line | What it polls |
|----------|-------|-----------|---------------|
| `REFRESH_PERIOD` | 5 s | `crates/ignition-tui/src/workers/refresh.rs:21` | Dashboard refresh worker — "panels update every 5 s with zero keystrokes (must-have truth #1)" — **THE natural target for TUIX-05** |
| `ALARMS_PERIOD` | 5 s | `crates/ignition-tui/src/workers/watch.rs:20` | Alarms screen poll |
| `WATCH_PERIOD` | 2 s | `crates/ignition-tui/src/workers/watch.rs:33` | Tags live-watch |
| `TAIL_INTERVAL` | 2 s | `crates/ignition-tui/src/workers/tail.rs:26` | Log tail pages (already "the CLI's `--interval` default, inherited verbatim") |
| `TICK` | 250 ms | `crates/ignition-tui/src/lib.rs:44` | Redraw staleness floor — **LOCKED, explicitly "not tick-only"; do not parameterize** |

**The spawn seam** — `spawn_refresh` (`refresh.rs:120-133`) hardcodes the constant at line 131:
```rust
handle.spawn(refresh_worker(client, tx, shutdown_rx, era, REFRESH_PERIOD));
```
Crucially, `refresh_worker(api, tx, shutdown, era, **period: Duration**)` **already accepts the period as a parameter** (line 86-92, with `MissedTickBehavior::Skip` already set). So the parameterization is: (a) `AppState` (defined `state.rs:1231-1273`) carries the configured interval — a new field, or riding `ClientHandle` (`state.rs:350`, the `Arc<ReqwestGatewayApi>` newtype that exists only because the API isn't `Debug`); (b) `context::resolve`/`rebuild` return it — note both return the `(String, String, Arc<ReqwestGatewayApi>)` triple (`context.rs:44,52`), which would need to grow or become a small struct; (c) `spawn_refresh` reads state instead of the const. The **profile-switch path is the trap**: `switch_profile` (`update.rs:545-587`) calls `context::rebuild(name)` then `spawn_refresh(state)` (line 584) — the *new* profile's interval must ride `rebuild`'s return or the dashboard silently keeps the old cadence after a switch. Outside a tokio runtime, `spawn_refresh` already no-ops safely (`Handle::try_current` guard) — state-machine unit tests are unaffected.

**What must change (the "one goldens migration"):** "Goldens" here are snapbox **inline** golden strings (`snapbox::str![[r#"..."#]]` + `SNAPSHOTS=overwrite` workflow — no separate golden *files* exist), plus the serde-shape pins. Compile-break surface if fields are added (Rust struct literals are exhaustive; neither struct's literals use `..Default` except where noted):

`Profile {` literals — **7 sites**: `main.rs:2259`, `context.rs:120`, `context.rs:196` (test), `update.rs:4182` (test), `config/mod.rs:216` + `config/mod.rs:228` (tests), `actions/profile.rs:79`.
`Config {` literals — **4 sites**: `context.rs:189` (uses `..Config::default()` — survives), `update.rs:4175`, `rig/mod.rs:621` + `rig/mod.rs:645` (rig-config tests).

Config-output goldens: `contract_profile.rs` pins `profile list` JSON in 3 render modes — **these stay byte-identical if the new Profile field is not added to the `ProfileListResult` model** (the render model is separate from the serde struct — `actions/profile.rs`). Keeping the action model untouched is the cheap way to honor "one goldens migration, never two." Round-trip unit tests (`config/mod.rs:243-262`, `profile.rs:148-210`) assert `PartialEq` losslessness — `Option` + `skip_serializing_if` keeps them green without edits.

---

## 3. Contract discipline (CORE-11)

### Verified current state

**Three-Place slug rule — all three places located:**
1. **The enum test:** `exit_code_mapping_enumerated` — unit test at `crates/ignition-core/src/error.rs:846` (doc-comment: "the FULL 1–7 taxonomy enumerated on day one so no later phase can silently renumber it or respell a slug"). It asserts `(CoreError, u8, &'static str)` triples against literals for every variant.
2. **The README table:** `README.md:39-48` — full 0–7 table with per-bucket slug lists (exit 6 currently carries 22 slugs).
3. **The prose:** `README.md:20-35` (Output contract section) + `README.md:50-52` — "The exit-code table lives in exactly two places — this README and `CoreError::exit_code()` … kept in sync by the enumerated mapping unit test."

**Test-enforcement gap:** the enumerated test (place 1) pins enum ↔ literals; the goldens pin specific slugs' exit codes end-to-end. But **no test parses the README table** — "enum test + README table + prose agree" is today enforced by discipline, not machinery. Phase 8's executable ritual means adding a test that reads `README.md` (e.g. `include_str!("../../../../README.md")` from a core or cli test) and asserts every slug in `CoreError::code()`'s match appears in the table row matching its `exit_code()` (and vice versa). A slug-regex over the table is std-only; the repo-relative path from `crates/ignition-cli/tests/` is `../../README.md`, from a core unit test `../../../../README.md`. The error.rs doc-comment table (lines 8-16) is a **fourth** copy the planner may want to include in the agreement test or deliberately exempt (it drifts already — check: it lists 07-06's slugs, appears current).

**OutOfBand registry — exact anatomy:**
- The taxonomy: `Mapping` enum — `crates/ignition-tui/src/routes.rs:14-24` (`Screen(Screen)` / `Streamed` / `OutOfBand`). The registry: `routes() -> &'static [CliRoute]` (`routes.rs:37-424`), one row per clap leaf.
- The pinned test: `out_of_band_rows_are_exactly_the_completions_leaf` — `crates/ignition-cli/tests/tui_coverage.rs:119-131`, asserting the OutOfBand set is exactly `vec!["completions"]`.
- The **structural constraint the planner must design around**: the sibling test `every_row_requiring_cli_node_is_mapped_and_no_orphans` (`tui_coverage.rs:79-113`) walks the live clap tree and asserts **bidirectional set equality** with the registry. `mcp`, `lsp`, and `edit` do not exist in the clap tree today — adding registry rows for them in Phase 8 would fail as orphan rows. Therefore the Phase 8 "extension" is: update the pinned test's expected set/justification text and the taxonomy documentation (routes.rs module docs at lines 20-23 and 322-329, which already carry the "sanctioned stdout exceptions" prose naming `completions` as "the leaf-representable exception ONLY") to **pre-declare `mcp`/`lsp`/`edit` as reserved OutOfBand slugs with justification** (they are protocol/lens surfaces, not TUI screens — MCP stdio and LSP speak their own protocols; `edit` is an editor round-trip, not a cockpit verb). When phases 13/14 land those commands, their rows + the pinned-test entries arrive together. The "justification text" the roadmap asks for lives in the pinned test's doc-comment and the routes.rs taxonomy block.
- The four sanctioned stdout exceptions (traceable through routes.rs comments + main.rs): `completions` (leaf, OutOfBand), `logs -f` NDJSON (flag on a Screen leaf), `rig logs` passthrough (the `Streamed` kind), `tags export -o -` (flag value, comment-only at `routes.rs:164-173`).

**stdout-purity — what exists today:**
- Tracing is **stderr-only by construction**: `init_tracing` (`main.rs:2433-2450`) ends with `.with_writer(std::io::stderr)`. This is the invariant the harness pins.
- Existing precedents to copy: `cli_chassis.rs:78-90` (`verbose_keeps_stdout_version_line_only` — asserts stdout is *exactly* the version line while `-v` fires tracing) and `cli_chassis.rs:159-180` (TUI pipe refusal — asserts "zero alt-screen escapes on stdout").
- The harness (`command dispatch` over the **real spawned binary**): `assert_cmd::Command::cargo_bin("ign")` is the established spawn pattern (dev-deps already include `assert_cmd`, `predicates`, `tempfile`; `snapbox` has a `cmd` feature too). Design that "fails on a single stray byte": spawn `ign version --json` (no network) with (a) a config carrying unknown keys (fires `warn_unknown_keys` tracing), (b) `IGNITION_LOG=trace` (max-diagnostics environment), and assert `out.stdout` byte-equals the exact expected envelope (or: parses as exactly one JSON value with zero trailing bytes + zero non-UTF8). The known stray-byte risks to prove against: tracing writes, panics, partial writes. **No new dependency needed** — std + existing dev-deps suffice; say so in the plan.
- Where it lives: `crates/ignition-cli/tests/` (it needs the binary — this is a binary-level test, the `cli_chassis.rs`/`contract_*.rs` home). Consider a dedicated `contract_stdout_purity.rs` so the ritual is name-discoverable.

**Gotcha:** `SNAPSHOTS=overwrite` is the sanctioned golden-regen escape hatch (`contract_profile.rs:8`); the harness should be *assert*-based (predicates/byte-compare), not snapbox-golden-based, so it can't be silently overwritten.

---

## 4. TUI worker parameterization (TUIX-05) — consolidated

Covered in §2 (cadence table + spawn seam). The additional verified facts:

- **Config flow into the TUI entrypoint today:** `ign tui` dispatch arm (`main.rs:2096-2104`) → TTY guard → `ignition_tui::run(cli.profile.clone())` → `context::resolve(flag)` → triple `(profile_name, profile_url, Arc<ReqwestGatewayApi>)` → `run_loop` sets `state.client/profile/profile_url/events_tx` then `workers::refresh::spawn_refresh(&mut state)` (`lib.rs:66-84`). The configured interval must enter through this same chain (resolve/rebuild return values → AppState → spawn).
- **Mechanism:** `tokio::time::interval(period)` with `MissedTickBehavior::Skip` (refresh.rs:93-96) — the pattern to keep; no crossbeam anywhere in the TUI.
- **Other spawn sites that read constants** (planner decides scope; criterion 2 only demands the dashboard cadence visibly change): `spawn_alarms` (`watch.rs:88`) → `ALARMS_PERIOD`; tags watch spawn (`watch.rs:~276`) → `WATCH_PERIOD`; `spawn_tail` (`tail.rs:86`) → `TAIL_INTERVAL` (passed into `actions::logs::tail`, which routes through core's `PollConfig` — `poll.rs:42-61`, the shared wait/retry engine). Minimal-compliant scope = refresh worker only; a cleaner contract = one `PollCadence` struct on AppState consulted by all four. Recommend minimal + naming the others as follow-ups, to keep the goldens blast radius at one.

---

## 5. Testing conventions (verified)

| Layer | Location | Mechanism |
|-------|----------|-----------|
| Binary-level contract tests | `crates/ignition-cli/tests/contract_*.rs`, `cli_chassis.rs` | `assert_cmd::Command::cargo_bin("ign")` + snapbox **inline** goldens (`Assert::new().action_env("SNAPSHOTS").eq(actual, snapbox::str![[…]])`); every spawn sets `IGNITION_CLI_CONFIG` to a tempdir path (macOS/XDG gotcha documented at `config/mod.rs:5-7`) |
| Gateway-mock contract tests | `crates/ignition-core/tests/*_contract.rs` (+ `tests/common/mod.rs`) | `wiremock` servers + `ReqwestGatewayApi::for_tests(server.uri(), credential)` |
| Unit tests | inline `#[cfg(test)] mod tests` throughout | env mutation serialized by crate-wide `ENV_LOCK` mutexes (`config/mod.rs:31`, `ignition-tui/src/lib.rs:39` — the comment at the TUI one documents that per-module locks do NOT serialize) |
| TUI coverage walk | `crates/ignition-cli/tests/tui_coverage.rs` | clap `CommandFactory` tree-walk, bidirectional registry equality (gated `#![cfg(feature = "tui")]`) |
| Live gates | `crates/ignition-core/tests/live_gateway.rs`, `crates/ignition-cli/tests/e2e_*.rs`, `version_gateway_contract.rs` | env-gated; run against real rigs (not CI-default) |
| Keyring smoke | `keyring_smoke.rs` (`--ignored`), CI job with gnome-keyring | separate CI job |
| Golden update ritual | — | `SNAPSHOTS=overwrite cargo test` then review the diff (documented in every contract file header) |

`Config`-dependent test isolation is uniformly `IGNITION_CLI_CONFIG` → `tempfile::TempDir` — the Phase 8 config tests (new keys, invalid-config degradation, sub-second refusal) follow `config/mod.rs`'s existing inline tests + a `contract_profile.rs`-style binary test.

## 6. Gotchas (planner-critical)

1. **CI is `clippy -D warnings --all-targets` + `fmt --check`** (`.github/workflows/ci.yml:22-23`) — plus `cargo build -p ignition-cli --no-default-features` (line 27): the lean/agent build must compile. `Session` in core is feature-free (fine); anything touching `ignition-tui` is behind the `tui` feature (`ignition-cli/Cargo.toml:17-19`, `default = ["tui"]`).
2. **Frozen contract invariants** (REQUIREMENTS.md:75): envelope `{ok,profile,data|error}` locked; **exit codes never renumbered; new error surfaces map to existing exit classes additively**. The `config_invalid`/`profile_not_found` slugs and exit 3 survive Phase 8 untouched. The README table, error.rs doc-comment table (error.rs:8-16), and `exit_code_mapping_enumerated` must move together in one commit per slug change (the established two-places rule, README.md:50-52).
3. **The TUI currently treats config errors as a pre-init hard exit** (by design, `lib.rs:48-52`). Criterion 1 partially reverses this — but `NoActiveProfile` refusal for the authed cockpit is ALSO locked design (`context.rs:38-41`). The invalid-config degradation must be scoped precisely (see Open Questions).
4. **`warn_unknown_keys` lists are hand-maintained** (config/mod.rs:71-73) — new keys warn as "unknown" until both lists grow. Add a pin.
5. **Struct-literal exhaustiveness:** 7 `Profile {` + 4 `Config {` literal sites break on field addition (§2 inventory). `Profile` has no `Default` derive; adding one is an option but changes the type's contract — explicit fields or `..Default::default()` at test sites is the smaller move.
6. **Profile switcher must adopt the new interval** — `switch_profile` → `context::rebuild` → `spawn_refresh` (`update.rs:545-587`); `rebuild` returns the `(name, url, Arc<api>)` triple that would need to grow. Miss this and the dashboard cadence silently ignores the newly-selected profile's setting.
7. **`Session` naming collisions exist but are benign:** `client::sessions` module, `SessionsResult`, `SessionNotPrunable`, TUI `SessionType`. Prose-level confusion only; no compile conflict with a top-level `session` module.
8. **`rig_gateway_client` / `context::rig_client_with` build ad-hoc Profiles** (derived rig URLs, `ssl_verify=false`, default auth) — these are *not* profile-config clients; criterion 4's sweep must give them an explicit home (Session constructor for URL+credential) rather than silently leaving a "second client construction" behind for the completion grep to find.
9. **Two byte-identical `secret_chain()` fns exist** (`main.rs:2287`, `context.rs:28`) — Session absorbs one; delete both (the chain becomes core-owned). Same for the duplicated overlay→selection choreography. The 06-RESEARCH note ("choke files stay untouched") is v1.0 history that Phase 8 deliberately supersedes.
10. **The `tui_coverage` walk means any future `mcp`/`lsp`/`edit` clap command auto-demands a registry row** — the OutOfBand pre-declaration in Phase 8 is what makes phases 13/14 mechanical instead of contract-negotiating.
11. **`TICK` (250 ms) is LOCKED** (`lib.rs:41-44`: "the tick guarantees a redraw even when the world is quiet (LOCKED cadence — not tick-only)") — `poll_interval_secs` parameterizes *data polling*, never the redraw tick.
12. **snapbox backslash normalization** (recorded in 07-03/03-02 summaries): `\n` in golden strings must be written `/n` — relevant if new goldens carry escaped content.

---

## Standard Stack (confirmed, in use — nothing new)

| Library | Version | Role in this phase | Verified at |
|---------|---------|--------------------|-------------|
| clap (derive) | 4.6 | command tree (read-only this phase — the tui_coverage walk reads it) | root `Cargo.toml:16` |
| serde / toml | 1.0 / 1.1 | config schema fields, `#[serde(default)]` + `skip_serializing_if` conventions | `profile.rs` throughout |
| reqwest + async_trait | 0.13 / 0.1 | the client Session wraps (`GatewayApi` is `async_trait`, dyn-compatible) | `client/mod.rs:83-84` |
| tokio | 1.53 | `time::interval` + `MissedTickBehavior::Skip` (the cadence mechanism), `sync::watch` shutdown | `refresh.rs:93-96` |
| ratatui / crossterm | 0.30.2 / 0.29 | TUI loop (event-loop untouched; only worker spawn sites change) | root `Cargo.toml:39-40` |
| assert_cmd + predicates + tempfile | 2.2 / 3.1 / 3.27 | stdout-purity harness + config tests (binary spawn, isolated config) | `cli_chassis.rs:9-10`, dev-deps |
| snapbox | 1.2 | inline goldens (`SNAPSHOTS=overwrite` ritual) — for any config-shape golden re-pins | `contract_profile.rs:103` |
| wiremock | 0.6 | gateway mocks — untouched by this phase but the pattern for Session-adjacent tests | core dev-deps |
| thiserror | 2.0 | any new validation error variant rides the existing enum | `error.rs:34` |

**No new dependencies.** The stdout-purity harness and README-table agreement test are std-only (`include_str!`, byte comparison) over existing dev-deps.

## Don't Hand Roll

| Problem | Don't build | Reuse instead | Why |
|---------|-------------|---------------|-----|
| Config load/discovery/env-overlay | new loader or override plumbing | `ignition_core::config::{load, save, config_path, apply_env_overlay, resolve_selection}` — public, tested | Already handles fresh-install, 0600, unknown-key warnings, LOCKED precedence |
| Profile→client resolution choreography | a second copy in Session | Promote the existing `resolve_profile_context` + `secret_chain` sequence verbatim into `Session::resolve` | The precedence (flag > env > profile > default, overlay scoped to selection) is research-locked; copy, don't redesign |
| Interval worker loop | custom polling loop | `refresh_worker` — it **already takes `period: Duration`** | Only `spawn_refresh`'s constant read changes |
| Binary spawn in tests | hand-rolled `std::process` spawn | `assert_cmd::Command::cargo_bin("ign")` | Handles the workspace bin path, env scoping, output capture |
| Secret redaction | any new exposure path | `Secret::expose` stays confined to `apply_auth` (`client/mod.rs:430`) — Session must not widen it | The grep-auditable redaction boundary (CORE-02) |
| Exit-code/slug stability | ad-hoc exit numbers at call sites | `CoreError::exit_code()` — "the only place exit codes are decided" (`main.rs:1-6`) | Frozen taxonomy; the enumerated test guards it |
| Wait/retry for anything new | bespoke retry loops | `ignition_core::poll::PollConfig` | The ONE wait/retry engine (`poll.rs:1-4`) |

## Open Questions (for the planner — not research blockers)

1. **Invalid-config TUI degradation semantics.** After degrading an invalid config to defaults, there is no active profile — but the cockpit currently *cannot open* without one (`NoActiveProfile` by design). Options: (a) TUI gains a profile-less "degraded" start state (larger change, touches `run_loop`/render), or (b) degradation applies to the schema *after* profile resolution — i.e. profile/auth resolution errors still hard-exit, but a broken `[ui]`/`poll_interval_secs` value alone falls back to defaults with a stderr warning and the TUI starts. Option (b) matches "a config typo never kills TUI startup" narrowly and keeps the authed-surface lock. **Recommendation: (b)** — scope degradation to schema-validation failures, not resolution failures; pin both behaviors in tests.
2. **Sub-second refusal exit class.** `0` (and fractional TOML) must refuse "with a clear error" — new slug `poll_interval_too_small` (or similar) on the existing `ConfigInvalid` exit-3 class is the additive-compliant shape. Confirm slug name and whether the CLI `version`/`profile list` paths (which tolerate config quirks) should also surface it or degrade silently.
3. **Rig-family clients in the criterion-4 sweep.** Give `Session` a `for_url(url, credential)`-style constructor (full sweep, "no second construction" grep-clean) or document the rig clients as the sanctioned exception. **Recommendation: full sweep** — criterion 4's wording is absolute.
4. **`Session` handle type: `Arc<dyn GatewayApi>` vs concrete `Arc<ReqwestGatewayApi>`.** ARCHITECTURE Pattern 2 prescribes dyn; the TUI's `ClientHandle` and worker signatures are concrete today (`workers/*` take `Arc<ReqwestGatewayApi>`). Dyn-first means touching those signatures; concrete-first means Session wraps the concrete type and the `as &dyn` casts at use sites (the `rig_stream.rs:188` precedent) continue. **Recommendation: follow ARCHITECTURE (dyn) only if the signature churn stays bounded** — otherwise concrete-with-deref is a compliant Phase 8 and dyn-widening can ride Phase 14 where MCP actually needs dyn.
5. **OutOfBand extension mechanics.** Rows for `mcp`/`lsp`/`edit` cannot exist until the clap commands do (orphan rows fail the walk). The pinned-test update is therefore documentation-of-intent + justification text now, rows later. Confirm the planner accepts this reading of "registry is extended."

## Sources

All findings verified by direct file reads on 2026-09-04:

- `crates/ignition-core/src/client/mod.rs` (trait, `new`/`for_tests`, `apply_auth`, construction conventions)
- `crates/ignition-cli/src/main.rs` (dispatch chassis, all 8 construction sites, resolve helpers, `init_tracing`, `secret_chain`)
- `crates/ignition-tui/src/context.rs` (resolve/rebuild/rig clients, duplicated chain)
- `crates/ignition-core/src/config/mod.rs` + `profile.rs` (load semantics, warn-lists, struct shapes, round-trip tests)
- `crates/ignition-tui/src/lib.rs`, `workers/{refresh,watch,tail,ops,mod}.rs`, `state.rs`, `update.rs`, `routes.rs` (cadence constants, spawn seams, AppState, Mapping taxonomy)
- `crates/ignition-cli/tests/tui_coverage.rs`, `cli_chassis.rs`, `contract_profile.rs` (pinned tests, spawn harness, golden ritual)
- `crates/ignition-core/src/error.rs` (taxonomy table doc, `code()`/`exit_code()`/`hint()`, `exit_code_mapping_enumerated`)
- `README.md:20-52` (output contract prose + exit-code table + two-places statement)
- `.github/workflows/ci.yml`, root/`ignition-cli` `Cargo.toml` (lint gates, features, dep versions)
- `.planning/research/ARCHITECTURE.md` (Session Pattern 2 design intent), `.planning/ROADMAP.md:47-57` (phase definition), `.planning/REQUIREMENTS.md:75` (frozen-contract constraint)

## Metadata

**Confidence breakdown:**
- Construction-site inventory: HIGH — grep-enumerated, every site read in context
- Config schema + cadence map: HIGH — struct definitions and constants read directly; line numbers verified
- Contract-test anchors: HIGH — test bodies read; the README-agreement gap confirmed absent by search
- Design recommendations (§Open Questions): MEDIUM — verified constraints, planner judgment required

**Research date:** 2026-09-04
**Valid until:** stable repo — re-verify only if a phase lands before Phase 8 planning completes
