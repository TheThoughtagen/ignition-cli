---
phase: 08-foundations-session-core-config-contract
verified: 2026-09-06T23:20:00Z
status: passed
score: 8/8 must-haves verified
human_verification:
  - test: "Optional re-run of the TUIX-05 visible-cadence check on a real terminal (per 08-05 how-to-verify steps 1-3)"
    expected: "Dashboard refreshes at the configured poll_interval_secs; live profile switch adopts the new interval; typo'd poll_interval_secs warns on stderr and the TUI still starts"
    why_human: "Real-time cadence on a live terminal is not fully programmatically verifiable; the plan's blocking checkpoint was closed via delegated PTY verification against the live gateway (evidence recorded in 08-05-SUMMARY: 1s burst gaps 0.993-1.001s, 8s adoption after switch, degraded start at default 5s), so this is an optional user confirmation, not an open gap"
---

# Phase 8: Foundations — Session, Core Config Contract Verification Report

**Phase Goal:** Every downstream feature builds on one shared execution seam (ignition-core::Session), one config migration ([ui].theme + per-profile poll_interval_secs land together — one goldens migration, never two), and executable contract rituals (Three-Place slug rule, OutOfBand taxonomy, stdout-purity harness) — so nothing later mutates the frozen contract by accident.

**Verified:** 2026-09-06T23:20:00Z
**Status:** PASSED
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | `[ui].theme` + per-profile `poll_interval_secs` load cleanly, warn-silent (one schema migration) | ✓ VERIFIED | `profile.rs`: `UiConfig` (L47) + `poll_interval_secs: Option<u64>` (L162) with `lenient_u64`/`lenient_ui`; warn-lists grown (`config/mod.rs:133` `"ui"`, `:140` `"poll_interval_secs"`) + membership tests (mod.rs:398-402); round-trip + legacy-shape tests in profile.rs |
| 2 | Invalid NEW-surface values degrade to defaults; `load_for_tui` exists with the strict/degrading split | ✓ VERIFIED | `load_inner(path, strict_clamp)` shared body (mod.rs:73), `load` strict (L52) / `load_for_tui` degrading (L68); degrade tests at mod.rs:469-508 (clamp→Ok, broken url→Err, garbage TOML→Err); `lenient_u64("banana")→None` test (profile.rs:346-351); TUI boundary tests in context.rs |
| 3 | `poll_interval_secs = 0` refused: `poll_interval_too_small`, exit 3, no new exit code | ✓ VERIFIED | `config/mod.rs:125` slug in clamp validation; `error.rs:459` code mapping + `:1123` enumerated triple; README exit-3 row (L44) carries the slug; binary contract `contract_config_clamp.rs` (exit-3 refusal + floor=1 control) green |
| 4 | ONE production client construction: all CLI + TUI dispatch resolves through `ignition-core::Session` | ✓ VERIFIED | `rg 'ReqwestGatewayApi::new' crates/` → only session.rs (4 constructors) + test doc-comment + for_tests; main.rs: 13 `Session::` call sites, zero construction/`secret_chain`; context.rs: `Session::resolve_loaded` (L91) + `Session::for_url` (L148), zero construction; `fn secret_chain` → zero hits tree-wide (core chain renamed `locked_secret_chain`, session.rs:268) |
| 5 | Session reproduces the LOCKED choreography (overlay→selection→secret chain) with behavior parity | ✓ VERIFIED | session.rs: `resolve`/`resolve_loaded`/`resolve_side`/`resolve_degraded`/`for_url` + `api()/api_handle()/profile_name()/profile_url()/credential_present()/Deref` (L98-257); 7 parity test fns (overlay-targets-selected-profile, selection precedence, env-token-beats-basic-pair, headerless degraded) all green |
| 6 | `poll_interval_secs` visibly drives cadence; profile switch adopts immediately; TICK untouched | ✓ VERIFIED | Chain: context.rs `ResolvedContext.poll_interval` (L41-50, L92-100) → state.rs `AppState.poll_interval` (L1252, defaults REFRESH_PERIOD L1295) → refresh.rs `spawn_refresh` reads `state.poll_interval` (L145) → update.rs switch trap closed: `state.poll_interval = ctx.poll_interval` (L576) BEFORE `spawn_refresh` (L590); `TICK = 250ms` untouched (lib.rs:44); plumbing tests (context.rs:286, update.rs:4335, refresh.rs:395) green; live PTY checkpoint evidence recorded (08-05: 1s bursts, 8s switch adoption) |
| 7 | Three-Place slug rule machine-enforced (README table ↔ code, both directions) | ✓ VERIFIED | `readme_exit_table_agreement` (error.rs:1207) parses README via `include_str!` (L1208), cross-checks both directions against literal triples; test green in the verified run; negative proofs recorded in 08-06 SUMMARY (missing slug + stale row both failed during development) |
| 8 | OutOfBand mcp/lsp/edit reserved with justification (no rows); stdout-purity byte-scan harness over the real binary | ✓ VERIFIED | tui_coverage.rs:44 + 125-135 reserved justification, pinned set exactly `["completions"]` (L146); routes.rs:338 pre-declaration; clap-walk tests green; `contract_stdout_purity.rs`: raw `output.stdout` byte-equality under unknown-key warning + `IGNITION_LOG=trace` (L80, L121), assert-based (overwrite-proof), negative proof recorded; both tests green in the verified run |

**Score:** 8/8 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
| --- | --- | --- | --- |
| `crates/ignition-core/src/config/profile.rs` | UiConfig + poll_interval_secs + lenient deserializers | ✓ VERIFIED | Substantive (tests included), wired into Config/Profile serde |
| `crates/ignition-core/src/config/mod.rs` | warn-lists, clamp validate, load_for_tui/load_inner | ✓ VERIFIED | All present; consumed by TUI |
| `crates/ignition-core/src/error.rs` | PollIntervalTooSmall slug + readme agreement test | ✓ VERIFIED | code/exit-3/hint + enumerated triple + agreement test; doc-table row (L12) |
| `README.md` | exit-3 row carries poll_interval_too_small; prose names both sync tests | ✓ VERIFIED | L44 + agreement test green (prose enforced by test data flow) |
| `crates/ignition-core/src/session.rs` | Session seam, 5 constructors, accessors, Deref | ✓ VERIFIED | 540+ lines, only production construction site in tree |
| `crates/ignition-cli/tests/contract_config_clamp.rs` | exit-3 clamp refusal binary contract | ✓ VERIFIED | Refusal + floor=1 control, green |
| `crates/ignition-cli/tests/contract_stdout_purity.rs` | byte-exact stdout harness, assert-based | ✓ VERIFIED | 6KB, 2 tests, byte-compare helper, green |
| `crates/ignition-cli/tests/tui_coverage.rs` | reserved mcp/lsp/edit justification; pinned set intact | ✓ VERIFIED | Both walk tests green |
| `crates/ignition-tui/src/routes.rs` | reserved-slug pre-declaration | ✓ VERIFIED | L338 + module docs |
| `crates/ignition-tui/src/context.rs` | Session routing, load_for_tui, ResolvedContext | ✓ VERIFIED | All key links present (L90-91, L148) |
| `crates/ignition-tui/src/state.rs` | AppState.poll_interval | ✓ VERIFIED | L1252, manual Default → REFRESH_PERIOD |
| `crates/ignition-tui/src/workers/refresh.rs` | spawn_refresh reads state.poll_interval | ✓ VERIFIED | L145; REFRESH_PERIOD stays the single default (L26) |

### Key Link Verification

| From | To | Via | Status | Details |
| --- | --- | --- | --- | --- |
| config/mod.rs | error.rs | clamp returns poll_interval_too_small | ✓ WIRED | mod.rs:125 slug → error.rs:459 code() → exit 3 |
| config/mod.rs | ignition-tui/context.rs | load_for_tui is the TUI load entry | ✓ WIRED | context.rs:90 `config::load_for_tui(&config::config_path())?` |
| main.rs | core/session.rs | all dispatch via Session | ✓ WIRED | 13 `Session::` sites; zero construction remains |
| context.rs | core/session.rs | resolve via Session::resolve_loaded; rig via Session::for_url | ✓ WIRED | L91, L148 |
| config/profile.rs | context.rs | Profile.poll_interval_secs → ResolvedContext.poll_interval | ✓ WIRED | context.rs:92-100 unwrap_or(REFRESH_PERIOD) |
| context/state → refresh.rs | spawn_refresh | state.poll_interval consumed at spawn | ✓ WIRED | refresh.rs:145; switch adoption at update.rs:576 |
| error.rs | README.md | agreement test via include_str! | ✓ WIRED | error.rs:1208; test green |
| contract_stdout_purity.rs | ign binary | assert_cmd cargo_bin + trace noise | ✓ WIRED | cargo_bin("ign") L45, IGNITION_LOG=trace L47 |

### Requirements Coverage

| Requirement | Status | Blocking Issue |
| --- | --- | --- |
| CORE-09 (shared execution core, no second client) | ✓ SATISFIED | Truth 4/5 |
| CORE-10 (one config migration, degradation) | ✓ SATISFIED | Truths 1/2/3 |
| CORE-11 (executable contract rituals) | ✓ SATISFIED | Truth 7/8 |
| TUIX-05 (per-profile cadence + hard clamp) | ✓ SATISFIED | Truths 3/6 |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| --- | --- | --- | --- | --- |
| (none) | — | TODO/FIXME/placeholder/empty-impl scan across all 9 phase key files | — | Clean |

Goldens: zero snapshot-file commits in phase-8 git history; `contract_profile`/`contract_version`/diff/sync goldens byte-identical (all suites green with zero golden edits — confirmed by contract test runs).

### Human Verification Required

1. **Optional: visible-cadence confirmation (TUIX-05 checkpoint)** — already closed inside plan 08-05 via delegated PTY verification against the live gateway (all four steps PASS with recorded evidence: 1s cadence bursts, 8s live-switch adoption, degraded start with stderr WARN + default cadence, clamp exit 3). A user may re-run the steps in 08-05 `how-to-verify` for personal confirmation; no open gap.

### Gaps Summary

None. All four success criteria are verified against the actual codebase:

1. **Config migration** — `[ui].theme` + `poll_interval_secs` landed together in one additive schema change with lenient degradation, the exit-3 `poll_interval_too_small` clamp, and the `load_for_tui` strict/degrading entry-point pair. Goldens untouched.
2. **Cadence** — `poll_interval_secs` flows profile → ResolvedContext → AppState → spawn_refresh; the documented switch trap is closed (assignment precedes respawn); TICK stays 250ms; sub-second refusal enforced by the binary contract test.
3. **Contract rituals** — Three-Place rule enforced in both directions by the README-parsing agreement test (negative-proofed), mcp/lsp/edit pre-declared as reserved OutOfBand slugs with justification in the pinned test + routes docs (set still exactly `[completions]`), and the byte-exact stdout purity harness runs over the real spawned binary under maximum diagnostics (assert-based, overwrite-proof, negative-proofed).
4. **One seam** — `rg 'ReqwestGatewayApi::new' crates/` shows production construction only in `session.rs`'s four constructors; `secret_chain` deleted from CLI and TUI; all CLI arms and in-process TUI callers route through `ignition-core::Session` (including the plan-03 additions `resolve_side` and `resolve_loaded`, which keep diff/sync and degradation behavior byte-identical).

**Full verification gates re-run at verification time (2026-09-06):**
- `cargo test --workspace --lib --tests`: **892 passed / 0 failed** across 47 binaries (doc-tests content-empty per 08-05 note)
- `cargo clippy --all-targets -- -D warnings`: clean
- `cargo fmt --check`: clean
- `cargo build -p ignition-cli --no-default-features`: clean

---

_Verified: 2026-09-06T23:20:00Z_
_Verifier: Claude (gsd-verifier)_
