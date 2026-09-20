---
phase: 01-foundation-agentic-contracts
plan: 03
subsystem: config
tags: [rust, toml, directories, keyring, secrets, redaction, profiles, clap-subcommands, golden-tests, snapbox, ci]

# Dependency graph
requires:
  - phase: 01-foundation-agentic-contracts (plan 02)
    provides: LOCKED envelope {ok,profile,data}/{ok,profile,error}, CoreError taxonomy with config class (exit 3), render modes, snapbox golden harness
provides:
  - Config/Profile/AuthRef serde structs — TOML config with BTreeMap profiles (deterministic list), untagged AuthRef covering token_env/keyring/basic REFERENCE forms (never values), optional label skip-when-none
  - config discovery (IGNITION_CLI_CONFIG env FIRST, ProjectDirs second), load (missing=fresh-install Ok, invalid=ConfigInvalid exit 3, unknown keys warn-not-fail), save (parents + 0600 created AND re-asserted), resolve_selection (flag > IGNITION_PROFILE > active; unknown = ProfileNotFound with knowns; none = Ok(None))
  - apply_env_overlay — IGNITION_URL overrides the selected profile only
  - Secret newtype (no Serialize — type-level redaction; Debug/Display = ***; expose() the only read), Credential enum (Token | Basic), SecretStore trait seam (Ok(None)=not here / Err=found-but-unreadable)
  - LOCKED resolution chain: IGNITION_TOKEN_<PROFILE_UP> → token_env ref → IGNITION_TOKEN → keyring (Entry::new failure = warn+skip, never fatal) → IGNITION_USER/PASSWORD
  - actions::profile add/list/use — serde models only, no printing (the TUI rides this layer in Phase 6)
  - profile add/list/use subcommands born contract-complete (all three render modes golden-tested), [profile: NAME] human header on every success AND error render
  - profile name threaded into every envelope (success + error) via dispatch-resolved context
  - keyring-smoke CI job (gnome-keyring recipe verbatim from keyring-rs CI) — STATE.md blocker CLOSED
affects: [01-04, phase-2-inspection, phase-4-rig, phase-6-tui]

# Tech tracking
tech-stack:
  added: [url serde feature (workspace), gnome-keyring in CI smoke job]
  patterns: [SecretStore chain encodes the LOCKED order (structs don't), env-mutating unit tests serialize on a shared ENV_LOCK mutex (edition 2024 unsafe set_var), nested clap subcommands via Args-struct wrapper, ActionOutput::render_json monomorphic per-variant (Serialize is not dyn-compatible), URL trailing-slash normalization pinned in goldens]

key-files:
  created:
    - crates/ignition-core/src/config/mod.rs
    - crates/ignition-core/src/config/profile.rs
    - crates/ignition-core/src/config/secret.rs
    - crates/ignition-core/src/actions/mod.rs
    - crates/ignition-core/src/actions/profile.rs
    - crates/ignition-core/tests/keyring_smoke.rs
    - crates/ignition-cli/tests/contract_profile.rs
  modified:
    - crates/ignition-core/src/lib.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/contract_version.rs
    - .github/workflows/ci.yml
    - Cargo.toml
    - Cargo.lock

key-decisions:
  - "Basic env pair (USER/PASSWORD) is a separate BasicEnvStore placed AFTER KeyringStore so the LOCKED order (keyring before basic) holds through the store chain — the chain, not the struct, encodes order"
  - "profile add does NOT pre-resolve selection (a --profile flag naming the NEW profile must not fail); its envelope echoes post-add active state"
  - "Config save re-asserts 0600 on overwrite, not just on create (OpenOptions::mode only applies at creation)"
  - "AuthRef::default() = generic IGNITION_TOKEN token_env so a profile without an auth key resolves through the last env-token step"
  - "URL normalization (trailing slash on bare host) is pinned in goldens rather than fought"
  - "KeyringStore::set/delete are inherent methods (not on the trait) — writing requires a working store and is an error; resolve stays fail-soft"

patterns-established:
  - "Secret exposure audit: expose() is the only read; `impl Serialize for Secret` grep must stay 0 (verified in plan verification)"
  - "In-process env tests share config::ENV_LOCK; integration tests pass env per-spawn via .env (subprocess — no race)"
  - "Human-mode profile header [profile: NAME] is the CORE-01 mechanism for humans; the envelope's top-level profile field is the machine mechanism"

# Metrics
duration: 15min
completed: 2026-08-21
---

# Phase 1 Plan 03: Profiles, Config & Secrets Summary

**TOML config with discovery/0600/env overlay, the SecretStore seam (env-first → keyring → basic) with canary-proven redaction, and `profile add/list/use` born contract-complete through the 01-02 envelope in all three render modes**

## Performance

- **Duration:** 15 min
- **Started:** 2026-08-21T16:44:46Z
- **Completed:** 2026-08-21T17:00:44Z
- **Tasks:** 3
- **Files modified:** 15 (7 created, 8 modified)

## Accomplishments
- CORE-01 complete: add/list/switch profiles; the active profile name is echoed in EVERY envelope (success and error) and leads every human-mode render as `[profile: NAME]`; `--profile` beats `IGNITION_PROFILE` (proven by test)
- CORE-02 complete: secrets resolve env-first through the SecretStore seam (profile-specific env → token_env ref → generic env → keyring → basic pair), KeyringStore fails soft wherever no OS keyring exists, and the canary test proves a live token never appears in stdout or stderr even at `-vv`
- Config-error class golden-enforced: unknown profile exits 3 with `profile_not_found` slug + known-profiles hint on stderr; invalid TOML is `config_invalid`; fresh install (no file) is NOT an error — `version` exits 0 with `"profile": null`
- STATE.md keyring blocker CLOSED: `#[ignore]`-gated real-keychain round-trip + dedicated `keyring-smoke` CI job using the gnome-keyring recipe verbatim from keyring-rs's own CI; smoke passes locally on macOS

## Task Commits

Each task was committed atomically:

1. **Task 1: Config module — discovery, TOML load/save (0600), structs, env overlay** - `d1c4e8d` (feat)
2. **Task 2: Secret newtype, SecretStore seam, env-first resolution, keyring smoke + CI job** - `1211fe8` (feat)
3. **Task 3: profile actions + subcommands + envelope threading + goldens + canary** - `276446d` (feat)

## Files Created/Modified
- `crates/ignition-core/src/config/mod.rs` - config_path/load/save/resolve_selection/apply_env_overlay + 8 unit tests (0600, unknown keys, overlay scoping)
- `crates/ignition-core/src/config/profile.rs` - Config/Profile/AuthRef serde structs (untagged AuthRef, label skip-when-none, no deny_unknown_fields)
- `crates/ignition-core/src/config/secret.rs` - Secret/Credential/SecretStore/EnvStore/BasicEnvStore/KeyringStore/resolve_secret + 6 unit tests
- `crates/ignition-core/src/actions/profile.rs` - add/list/use actions (serde models out, no printing) + 2 unit tests
- `crates/ignition-core/tests/keyring_smoke.rs` - #[ignore]-gated real-keychain round-trip
- `crates/ignition-cli/src/cli.rs` - ProfileCmd (flags-only add), nested via ProfileArgs wrapper
- `crates/ignition-cli/src/main.rs` - dispatch with once-resolved profile context threaded into every envelope; IGNITION_URL overlay; auth_ref_from_flags precedence
- `crates/ignition-cli/src/render.rs` - [profile: NAME] header on every human render (success AND error); list rows
- `crates/ignition-cli/tests/contract_profile.rs` - 6 contract tests: 3-mode lifecycle golden, use-switch, env+flag selection, exit-3 golden, canary, no-config version
- `crates/ignition-cli/tests/contract_version.rs` - +version-with-config golden (profile: dev)
- `.github/workflows/ci.yml` - keyring-smoke job
- `Cargo.toml`/`Cargo.lock` - url gains serde feature

## Decisions Made
- BasicEnvStore split (see key-decisions): the plan's EnvStore sketch listed the basic pair as its 4th internal step, but the LOCKED must_have order puts keyring before USER/PASSWORD — a separate store after KeyringStore in the chain honors the lock exactly
- `profile add` skips pre-resolution so `--profile <new-name>` cannot fail on a profile that doesn't exist yet; the add envelope echoes the post-add active state
- 0600 is re-asserted on every save (not just create) so a loosened file self-heals
- KeyringStore::set/delete errors are surfaced (writing requires a store) while resolve stays fail-soft — asymmetric on purpose

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] url workspace dependency lacked the serde feature**
- **Found during:** Task 1 (Profile.url serde derives)
- **Issue:** `url = "2.5"` without `features = ["serde"]` — url::Url implements Serialize/Deserialize only with that feature; the whole config module would not compile
- **Fix:** `url = { version = "2.5", features = ["serde"] }` in workspace.dependencies
- **Files modified:** Cargo.toml, Cargo.lock
- **Verification:** cargo test -p ignition-core green
- **Committed in:** d1c4e8d (Task 1 commit)

**2. [Rule 3 - Blocking] Nested clap subcommand enum needed an Args-struct wrapper**
- **Found during:** Task 3 (first compile of ProfileCmd)
- **Issue:** `Commands::Profile(ProfileCmd)` requires the payload to implement `clap::Args`; a bare `Subcommand` enum does not, and `#[derive(Args)]` directly on the enum is rejected ("only supports non-tuple structs")
- **Fix:** `ProfileArgs { #[command(subcommand)] command: ProfileCmd }` wrapper deriving Args
- **Files modified:** crates/ignition-cli/src/cli.rs, crates/ignition-cli/src/main.rs
- **Verification:** cargo build --workspace + all contract tests green
- **Committed in:** 276446d (Task 3 commit)

**3. [Rule 1 - Bug] ActionOutput::data() Box<dyn Serialize> — Serialize is not dyn-compatible**
- **Found during:** Task 3 (multi-variant data payloads)
- **Issue:** `serde::Serialize` has a generic method (`serialize<S>`), so `Box<dyn Serialize>` cannot compile; the 01-02 single-arm `impl Serialize` return stopped generalizing across 4 variants
- **Fix:** `ActionOutput::render_json(profile, compact)` — monomorphic per-variant match calling `render_success` directly (also preserves each payload's declaration/golden order)
- **Files modified:** crates/ignition-cli/src/main.rs, crates/ignition-cli/src/render.rs
- **Verification:** envelope goldens pass unchanged in field order
- **Committed in:** 276446d (Task 3 commit)

**4. [Plan reconciliation] EnvStore/BasicEnvStore split for the LOCKED order**
- **Found during:** Task 2 (design)
- **Issue:** Plan's EnvStore sketch (basic pair as internal step 4) + chain [EnvStore, KeyringStore] yields basic-env BEFORE keyring, contradicting the must_have lock `...IGNITION_TOKEN → keyring → IGNITION_USER/PASSWORD`
- **Fix:** token envs stay in EnvStore; the USER/PASSWORD pair is a separate BasicEnvStore the chain places after KeyringStore — order preserved exactly, unit-tested with FixedStore doubles
- **Files modified:** crates/ignition-core/src/config/secret.rs
- **Verification:** `resolve_secret_chain_order_first_some_wins_and_exhaustion` pins env→keyring→basic explicitly
- **Committed in:** 1211fe8 (Task 2 commit)

---

**Total deviations:** 4 auto-fixed (2 blocking, 1 bug, 1 plan reconciliation)
**Impact on plan:** All fixes required for the plan's own must_haves to hold; no scope creep. The BasicEnvStore split and render_json pattern are recorded so 01-04 inherits them.

## Issues Encountered
- URL normalization (bare host gains trailing slash) shows up in list output and goldens — accepted and pinned rather than worked around (standard Url behavior; fighting it would store strings instead of typed URLs)

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Profiles/config/secrets complete: 01-04 (gateway version check) constructs `ReqwestGatewayApi` from a resolved Profile + `resolve_secret` credential (chain: [EnvStore, KeyringStore, BasicEnvStore]), uses `Secret::expose()` at exactly one header-construction site, and exercises Network/Auth/GatewayTooOld end-to-end against wiremock
- Blockers: none — the keyring smoke job closes the STATE.md headless-Linux concern

## Self-Check: PASSED

Verified: all 7 created files exist on disk; commits d1c4e8d / 1211fe8 / 276446d present on main; `cargo test --workspace` green (40 passed, 1 ignored-by-design keyring smoke — separately run and PASSED locally); `SNAPSHOTS=overwrite` no-op (goldens inline, zero drift); `impl Serialize for Secret` grep = 0; `IGNITION_PROFILE=nope ign version --json` exits 3 with profile_not_found slug; 0600 config file perms observed live (`-rw-------`); clippy -D warnings + fmt clean; no-default-features build clean.

---
*Phase: 01-foundation-agentic-contracts*
*Completed: 2026-08-21*
