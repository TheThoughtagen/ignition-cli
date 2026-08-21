---
phase: 01-foundation-agentic-contracts
verified: 2026-08-21T12:05:00Z
status: passed
score: 25/25 must-haves verified
human_verification:
  resolved: "2026-08-21 — repo pushed to github.com/TheThoughtagen/ignition-cli; CI run 32517734178: check (ubuntu-latest) ✓, check (macos-latest) ✓, keyring-smoke ✓ (after two fixes below). Original 'human_needed' items CONFIRMED by green CI runs."
ci_followup_fixes:
  - "0801b3e — clippy: #[expect(dead_code)] on require_confirmation unfulfilled in test-target compiles (unit tests call it) → gated to #[cfg_attr(not(test), expect(...))]"
  - "76d2546 — ubuntu test failures: keyring-unavailable fail-soft warn preceded the JSON error envelope on headless Linux → demoted to debug (expected environmental condition) AND stderr envelope tests now parse from first '{' (log-tolerant)"
---

# Phase 01: Foundation & Agentic Contracts Verification Report

**Phase Goal:** The `ign` binary is installable and configurable — a user can set up multiple gateway profiles with secure auth, and every command honors the machine-readable output contract (JSON, errors, exit codes) that agents and all later phases depend on.
**Verified:** 2026-08-21T12:05:00Z (CI confirmation 2026-08-21, run 32517734178)
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

**Verdict: goal achieved.** The binary builds (full + lean), all 61 tests pass, and live
spot-checks of the actual `ign` binary confirm the contract end-to-end. CI confirmation
completed 2026-08-21: after push, run 32517734178 went green on all three jobs
(check ubuntu ✓, check macos ✓, keyring-smoke ✓). Two latent platform issues were found
and fixed during CI bring-up (see frontmatter ci_followup_fixes) — both are CI-hardening,
not contract changes; the envelope taxonomy, exit codes, and goldens are unchanged.

### Method

gsd-tools could not parse these PLAN frontmatters, so verification was performed
directly: 22/22 artifacts checked (exists + pattern + substantive), all key links
grepped, `cargo test --workspace` run (61 passed / 0 failed / 1 ignored-by-design),
`cargo build -p ignition-cli --no-default-features` run, plus live binary spot-checks
of help/version/envelope/profile round-trip/error paths/completions/guard.

### Observable Truths

| # | Plan | Truth | Status | Evidence |
|---|------|-------|--------|----------|
| 1 | 01-01 | Workspace builds; lean `--no-default-features` build also succeeds | ✓ VERIFIED | Both builds exit 0 locally; CI runs both |
| 2 | 01-01 | `ign --help` shows all five global flags | ✓ VERIFIED | Live: `--profile --json --compact --yes --verbose` all present; test `help_lists_all_five_global_flags` |
| 3 | 01-01 | `ign version` exits 0 on fresh install, no config | ✓ VERIFIED | Live: exit 0; tests `version_subcommand_works_without_config`, `fresh_install_version_exit_0_profile_null` |
| 4 | 01-01 | IGNITION_JSON/YES/PROFILE env apply in one place | ✓ VERIFIED | `apply_env_defaults` single site (main.rs:83); tests `env_json_flag_is_accepted`, `env_yes_flag_is_accepted`, `env_profile_selection` |
| 5 | 01-01 | Diagnostics stderr-only, stdout never polluted | ✓ VERIFIED | `with_writer(std::io::stderr)` (main.rs:335); test `verbose_keeps_stdout_version_line_only` |
| 6 | 01-01 | CI green on ubuntu+macos, no Windows | ? HUMAN | Workflow verified correct (matrix `[ubuntu-latest, macos-latest]`, no Windows comment); no git remote → runs unobservable |
| 7 | 01-02 | `version --json` emits LOCKED `{"ok","profile","data"}` envelope | ✓ VERIFIED | Live output matches exactly; test `version_json_envelope_shape`; struct-level field set in output.rs |
| 8 | 01-02 | `--compact` implies `--json`, one-line output | ✓ VERIFIED | Live: 1 line; tests `version_compact_implies_json`, `version_compact_one_line` |
| 9 | 01-02 | Errors → stderr always; human default, JSON under `--json`; stdout data-only-on-success | ✓ VERIFIED | Live `--profile ghost` error on stderr with `2>&1 >/dev/null`; tests `human_mode_is_not_json`, goldens |
| 10 | 01-02 | Error envelope carries code/message/endpoint/hint | ✓ VERIFIED | Live: `{"code":"profile_not_found","message":...,"endpoint":null,"hint":...}` all four fields |
| 11 | 01-02 | Exit codes from one mapping, enumerated unit test | ✓ VERIFIED | `exit_code_mapping_enumerated` (error.rs:259) enumerates full 1–7 taxonomy |
| 12 | 01-02 | Exit-code table in exactly 2 places (README + exit_code), sync-tested | ✓ VERIFIED | README table rows 0–6 incl. `target_state`; enumerated test guards sync |
| 13 | 01-03 | profile add/list/use; active profile in every envelope + human header | ✓ VERIFIED | Live round-trip: `"profile":"rig1"` in add/list/use/version JSON; `[profile: NAME]` header (render.rs:84,97) |
| 14 | 01-03 | `--profile` and IGNITION_PROFILE both work; flag wins | ✓ VERIFIED | Tests `selection_flag_beats_active`, `env_profile_selection` |
| 15 | 01-03 | Env-first secret chain; canary never in stdout/stderr even --verbose | ✓ VERIFIED | `secret_redaction_canary` (CANARY-t0k3n-zzz) passes; chain-order unit tests; `Secret` has NO Serialize impl (type-level redaction, grep-confirmed) |
| 16 | 01-03 | Config 0600 perms; unknown TOML keys warn not fail | ✓ VERIFIED | Live `ls -l`: `-rw-------`; tests `save_enforces_0600_and_creates_parents`, `unknown_keys_warn_but_do_not_fail` |
| 17 | 01-03 | Unknown profile exit 3 + profile_not_found + hint; fresh-install graceful | ✓ VERIFIED | Live: exit 3 with full envelope; tests `unknown_profile_exit_3_golden`, `no_config_version_exit_0` |
| 18 | 01-03 | Keyring smoke passes on headless Linux CI | ? HUMAN | CI job `keyring-smoke` with gnome-keyring recipe verified verbatim; test is `#[ignore]` locally (1 ignored), `--ignored` in CI — run needs push |
| 19 | 01-04 | Gateway check: <8.3.1/unparseable → exit 6 gateway_too_old + upgrade hint | ✓ VERIFIED | Tests `answered_below_minimum_refuses_exit_6`, `too_old_gateway_refuses_exit_6`, `below_minimum_matrix`; MIN_GATEWAY=8.3.1 |
| 20 | 01-04 | Unreachable gateway → exit 0 + warning in data | ✓ VERIFIED | Tests `unreachable_gateway_degrades_to_warning_exit_0` (core + binary level) |
| 21 | 01-04 | Token XOR Basic headers, never both; secret.expose() only at client site | ✓ VERIFIED | Wiremock tests `token_credential_sends_token_header_only`, `basic_credential_sends_authorization_only`, `no_credential_is_header_less`; XOR `match` in client/mod.rs:99 |
| 22 | 01-04 | completions bash/zsh/fish on stdout regardless of --json | ✓ VERIFIED | Live: bash script on stdout; tests `completions_*_generate`, `completions_ignore_json_flag` |
| 23 | 01-04 | Destructive guard: no --yes → exit 2 + confirmation_required + dual hint | ✓ VERIFIED | Unit test `confirmation_guard_refuses_without_yes` (exit 2, slug, --yes+IGNITION_YES in hint) |
| 24 | 01-04 | Network(4)/auth(5)/target-state(6) binary-tested vs wiremock | ✓ VERIFIED | `connection_refused_maps_to_network_exit_4`, `auth_rejected_exit_5`, exit-6 tests |
| 25 | 01-04 | IGNITION_URL overlays profile URL before client construction | ✓ VERIFIED | Test `ignition_url_overlay_beats_profile_url_before_client_construction`; `apply_env_overlay` (config/mod.rs:153) called in main.rs:233 |

**Score:** 23/25 truths verified (2 require CI observation after push)

### Required Artifacts

All 22 artifacts across 4 plans: ✓ VERIFIED (exists, substantive, pattern present, wired).

| Artifact | Lines | Status | Notes |
|----------|-------|--------|-------|
| `Cargo.toml` | 33 | ✓ VERIFIED | rust-version 1.88, workspace.dependencies single-sourced |
| `crates/ignition-cli/src/cli.rs` | 110 | ✓ VERIFIED | 5 `global = true` args |
| `crates/ignition-cli/src/main.rs` | 366 | ✓ VERIFIED | `fn main() -> ExitCode`, try_parse → env → tracing → dispatch |
| `crates/ignition-tui/src/lib.rs` | 14 | ✓ VERIFIED | Planned stub (Phase 6), no ratatui dep |
| `.github/workflows/ci.yml` | 44 | ✓ VERIFIED | check + keyring-smoke jobs, ubuntu/macos only |
| `crates/ignition-core/src/error.rs` | 390 | ✓ VERIFIED | Full taxonomy, envelope, enumerated test |
| `crates/ignition-core/src/output.rs` | 65 | ✓ VERIFIED | LOCKED `{ok,profile,data}` envelope |
| `crates/ignition-cli/tests/contract_version.rs` | 184 | ✓ VERIFIED | snapbox goldens, 3 modes |
| `README.md` | 40 | ✓ VERIFIED | Exit-code table incl. target_state |
| `crates/ignition-core/src/config/mod.rs` | 425 | ✓ VERIFIED | IGNITION_CLI_CONFIG, 0600, env overlay |
| `crates/ignition-core/src/config/profile.rs` | 176 | ✓ VERIFIED | BTreeMap deterministic list |
| `crates/ignition-core/src/config/secret.rs` | 431 | ✓ VERIFIED | Secret newtype, no Serialize |
| `crates/ignition-core/src/actions/profile.rs` | 229 | ✓ VERIFIED | No printing (layering invariant) |
| `crates/ignition-core/tests/keyring_smoke.rs` | 42 | ✓ VERIFIED | `#[ignore]`-gated round-trip |
| `crates/ignition-cli/tests/contract_profile.rs` | 326 | ✓ VERIFIED | Goldens + redaction canary |
| `crates/ignition-core/src/client/mod.rs` | 133 | ✓ VERIFIED | async_trait, XOR auth, for_tests |
| `crates/ignition-core/src/client/version.rs` | 83 | ✓ VERIFIED | MIN_GATEWAY 8.3.1, below_minimum |
| `crates/ignition-core/src/actions/version.rs` | 187 | ✓ VERIFIED | Locked behavior matrix |
| `crates/ignition-cli/src/completions.rs` | 27 | ✓ VERIFIED | clap_complete bash/zsh/fish |
| `crates/ignition-core/tests/gateway_info_contract.rs` | 175 | ✓ VERIFIED | Wiremock header/status tests |
| `crates/ignition-cli/tests/version_gateway_contract.rs` | 235 | ✓ VERIFIED | Binary exit-code classes 4/5/6 |

Wiring: core crate has zero `println!/eprintln!` (grep-verified) — binary owns all
output. `actions::profile`/`version` dispatched from main.rs. All links wired.

### Key Link Verification

| From | To | Via | Status |
|------|----|----|--------|
| main.rs | clap errors | `try_parse()` → `Err(e) => e.exit()` | ✓ WIRED (main.rs:77–81) |
| main.rs | env defaults | `apply_env_defaults(&mut cli)` | ✓ WIRED (main.rs:83) |
| main.rs | stderr tracing | `with_writer(std::io::stderr)` | ✓ WIRED (main.rs:335) |
| main.rs | error.rs | `ExitCode::from(err.exit_code())` | ✓ WIRED (main.rs:102) |
| output.rs | serde_json | `to_string` / `to_string_pretty` | ✓ WIRED (output.rs:60–62) |
| contract tests | ign binary | `snapbox::str!` goldens | ✓ WIRED (3 goldens) |
| config/mod.rs | ProjectDirs | `IGNITION_CLI_CONFIG` override first | ✓ WIRED |
| secret.rs | keyring | `Entry::new` failure → warn + skip | ✓ WIRED (secret.rs:136) |
| main.rs | actions::profile | dispatch + envelope threads profile | ✓ WIRED (main.rs:49–53) |
| Secret | serde | NO Serialize impl exists | ✓ VERIFIED (negative check) |
| render.rs | profile header | `[profile: NAME]` | ✓ WIRED (render.rs:84,97) |
| client/mod.rs | gateway REST | `GET /data/api/v1/gateway-info` | ✓ WIRED |
| client/mod.rs | Secret | `X-Ignition-API-Token`, expose() only site | ✓ WIRED (client/mod.rs:99) |
| actions/version.rs | GatewayApi | `below_minimum` → `GatewayTooOld` (exit 6) | ✓ WIRED (version.rs:60–63) |
| main.rs | apply_env_overlay | IGNITION_URL before client construction | ✓ WIRED (main.rs:233) |

### Requirements Coverage

CORE-01 (profiles visible in every output) ✓ · CORE-02 (env/config/keyring auth,
secrets never echoed — canary-proven) ✓ · CORE-04/05 (exit-code taxonomy 0–7,
stderr discipline) ✓ · CORE-06 (--yes guard pattern) ✓ · CORE-07 (completions) ✓ ·
CORE-08 (gateway <8.3.1 refusal) ✓ — all Phase-1 requirements satisfied per plan
objectives and verified truths.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| crates/ignition-tui/src/lib.rs | 13 | `unimplemented!("TUI cockpit arrives in Phase 6")` | ℹ️ Info | Intentional, planned stub per Plan 01-01 ("no ratatui dep until Phase 6"); behind default-off `tui` feature; not on any Phase-1 path |

No TODO/FIXME/placeholder in any phase-modified source. Zero `println!` in core.
No stub handlers, no unwired artifacts.

### Human Verification Required

### 1. CI green on ubuntu-latest + macos-latest

**Test:** Push the repo to GitHub (no remote currently configured), open/watch the Actions run.
**Expected:** `check` job passes fmt/clippy/build/test/no-default-features on both OSes.
**Why human:** GitHub Actions runs cannot be observed without a remote; the workflow file itself is verified correct.

### 2. Keyring smoke job green on headless Linux

**Test:** Same Actions run — check the `keyring-smoke` job.
**Expected:** `cargo test --test keyring_smoke -- --ignored` passes under the gnome-keyring recipe (STATE.md blocker stays CLOSED).
**Why human:** Requires a real gnome-keyring daemon; local environment can't exercise it.

### Gaps Summary

No code gaps. All 22 artifacts exist, are substantive, and are wired. All 61 tests
pass; lean build passes; live binary spot-checks confirm the LOCKED envelope shape,
exit codes (0/2/3 verified live; 4/5/6 via wiremock binary tests), profile
round-trip, 0600 config perms, profile echo in every envelope, stderr discipline,
and completions-on-stdout. The only outstanding items are observing the two CI jobs
run green after the repo is pushed — configuration for both is verified verbatim
against the plans.

---

_Verified: 2026-08-21T12:05:00Z_
_Verifier: Claude (gsd-verifier)_
