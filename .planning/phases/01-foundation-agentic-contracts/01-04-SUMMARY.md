---
phase: 01-foundation-agentic-contracts
plan: 04
subsystem: client
tags: [rust, async-trait, reqwest, wiremock, semver, clap-complete, exit-codes, contract-testing, core-08]

# Dependency graph
requires:
  - phase: 01-foundation-agentic-contracts (plan 02)
    provides: LOCKED envelope + CoreError taxonomy with Network(4)/Auth(5)/GatewayTooOld(6) classes, render modes, snapbox harness
  - phase: 01-foundation-agentic-contracts (plan 03)
    provides: Profile/AuthRef config structs, apply_env_overlay, Secret/Credential, resolve_secret chain [EnvStore, KeyringStore, BasicEnvStore]
provides:
  - GatewayApi trait (async_trait — LOCKED), ONE coarse method (gateway_info) — Phase 2 grows it by capability, zero restructuring
  - ReqwestGatewayApi — auth headers (token XOR basic XOR neither, match-enforced), 10s/30s timeouts, per-profile danger_accept_invalid_certs, for_tests constructor
  - Error mapping proven on the wire: transport→Network(4), 401/403→Auth(5), both populate endpoint
  - GatewayInfo model + MIN_GATEWAY 8.3.1 + below_minimum (suffix/short-form tolerant; unparseable → refuse)
  - version action implementing the LOCKED behavior matrix — unreachable→exit-0 warning inside data, answered-too-old→exit 6
  - resolve_secret_opt dispatch adapter — only SecretUnavailable degrades to header-less (version never demands a secret)
  - IGNITION_URL overlay proven applied BEFORE client construction (binary test)
  - `ign completions bash|zsh|fish` via clap_complete::aot — RAW stdout regardless of --json (the one sanctioned success-path exception)
  - require_confirmation guard (exit 2 + confirmation_required + --yes/IGNITION_YES hint) — Phase 3+/4+ inherit verbatim
affects: [phase-2-inspection, phase-3-projects, phase-4-rig, phase-6-tui]

# Tech tracking
tech-stack:
  added: [async-trait 0.1 (workspace), clap_complete in ignition-cli, wiremock dev-dep in ignition-cli]
  patterns: [auth-header rule as a match (token XOR basic XOR neither), Secret::expose() confined to the single header-construction site (grep-audited), GatewayInfo carries a serde(skip) endpoint field so action-built errors populate CORE-05 endpoint, dead-loopback-port (127.0.0.1:1) for deterministic unreachable tests, #[expect(dead_code)] for guard helpers whose first caller arrives in a later phase, completions bypass config load entirely]

key-files:
  created:
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/src/client/version.rs
    - crates/ignition-core/src/actions/version.rs
    - crates/ignition-core/tests/gateway_info_contract.rs
    - crates/ignition-cli/src/completions.rs
    - crates/ignition-cli/tests/version_gateway_contract.rs
  modified:
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/cli_chassis.rs
    - crates/ignition-cli/tests/contract_version.rs
    - crates/ignition-core/src/actions/mod.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-core/src/lib.rs
    - crates/ignition-cli/Cargo.toml
    - Cargo.toml
    - Cargo.lock
    - README.md

key-decisions:
  - "ReqwestGatewayApi credential field is Option<Credential>, not Credential — the LOCKED unreachable/no-secret matrix row requires a header-less construction path; the Task 1 sketch and Task 2 matrix reconciled in Task 2's favor"
  - "below_minimum compares against plain three-component 8.3.1 — the research sketch's 8.3.1.0 constant would never parse (semver is strict), making every comparison fail; fixed at implementation"
  - "GatewayInfo.endpoint is a serde(skip) field stamped by the client so the ACTION-built GatewayTooOld can populate the CORE-05 endpoint without changing the locked action signature"
  - "VersionResult omits gateway/warnings when absent/empty (skip_serializing_if) — fresh-install goldens keep the bare {cli_version} shape; absent field and null are equivalent under Value indexing"
  - "Completions dispatch before config load — a broken config.toml must not break shell installation; render_ok prints the raw script regardless of mode (documented in README as the one success-path exception)"
  - "with-config version golden repinned on the dead port 127.0.0.1:1 — localhost:9088 would now trigger a live gateway check and flake on machines actually running Ignition"
  - "ConfirmationRequired hint updated to name both --yes and IGNITION_YES=1 (no golden referenced the old wording yet — safe)"
  - "#[expect(dead_code)] on require_confirmation — unfulfilled-expectation warns when Phase 3 gives it a caller, forcing attribute removal"

patterns-established:
  - "Wiremock header proofs: exact-value matchers + expect(1) verify sent values; a lowercased Debug dump of the recorded request's headers asserts presence/absence without depending on wiremock's header-map API"
  - "Binary gateway tests: config profile URL → mock server URI (or dead port); assertions on exit code + stderr envelope slugs/hints/endpoints via serde_json::Value"
  - "FakeApi test double constructs CoreErrors lazily (Network via a real dead-port reqwest) because CoreError is not Clone"
  - "Precomputed base64 literal (YWRtaW46c2VrcmV0 = admin:sekret) proves reqwest basic_auth's encoding without a base64 dev-dep"

# Metrics
duration: 37min
completed: 2026-08-21
---

# Phase 1 Plan 04: Gateway Seam & Phase-1 Finish Line Summary

**GatewayApi seam (async_trait, wiremock-proven auth headers: token XOR basic, never both) + `ign version` with the LOCKED minimum-version matrix (≥8.3.1 or exit-6 refusal; unreachable exits 0 with a warning inside data) + shell completions + the --yes guard — every Phase-1 requirement (CORE-01…08) now test-enforced**

## Performance

- **Duration:** 37 min
- **Started:** 2026-08-21T17:02:53Z
- **Completed:** 2026-08-21T17:40:15Z
- **Tasks:** 3
- **Files modified:** 18 (6 created, 12 modified)

## Accomplishments
- CORE-08 complete: `ign version` with a resolvable profile checks `GET /data/api/v1/gateway-info`; a gateway that ANSWERED below 8.3.1 (or unparseably) refuses with the exit-6 `gateway_too_old` envelope + upgrade hint; unreachable and fresh-install both exit 0 (regression-guarded)
- Auth-header construction proven on the wire for all three credential shapes: token → `X-Ignition-API-Token`, basic → `Authorization: Basic <b64>`, none → header-less — NEVER both (header-absence asserted on recorded requests); `Secret::expose()` lives at exactly the one header-construction site
- CORE-04/05 closed out: exit classes 4 (network), 5 (auth), 6 (target-state) now binary-tested end-to-end through the whole stack with endpoint+hint asserted in every error envelope
- CORE-07: `ign completions bash|zsh|fish` generates from the live clap definition; prints raw to stdout regardless of `--json` (the documented one success-path exception) and works with a broken config
- CORE-06: `require_confirmation` guard unit-proven (exit 2, `confirmation_required`, hint naming `--yes` AND `IGNITION_YES=1`) — Phase 3's `project delete` and Phase 4's `rig reset` inherit it verbatim; zero interactive prompts anywhere (grep-verified)
- IGNITION_URL env overlay proven applied BEFORE client construction at the dispatch site (binary test: dead profile URL + live overlay URL → the overlay wins)

## Task Commits

Each task was committed atomically:

1. **Task 1: GatewayApi trait + ReqwestGatewayApi + wiremock contract tests** - `c9ec700` (feat)
2. **Task 2: version action — LOCKED behavior matrix + binary-level exit tests** - `3fbb94d` (feat)
3. **Task 3: completions subcommand + --yes confirmation guard** - `7f8f6cb` (feat)

## Files Created/Modified
- `crates/ignition-core/src/client/mod.rs` - GatewayApi trait + ReqwestGatewayApi (auth-header match, timeouts, ssl_verify, error mapping, for_tests)
- `crates/ignition-core/src/client/version.rs` - GatewayInfo model + MIN_GATEWAY + below_minimum + 10-row boundary unit test
- `crates/ignition-core/src/actions/version.rs` - version action (LOCKED matrix) + 4 FakeApi unit tests
- `crates/ignition-core/tests/gateway_info_contract.rs` - 4 wiremock tests: token-only, basic-only, header-less 401→Auth(5), refused→Network(4)
- `crates/ignition-cli/tests/version_gateway_contract.rs` - 6 binary tests: exit 0/6/5, unreachable→exit-0 warning, fresh install, IGNITION_URL overlay
- `crates/ignition-cli/src/completions.rs` - clap_complete::aot generation into a String
- `crates/ignition-cli/src/main.rs` - version dispatch (post-overlay client construction), resolve_secret_opt adapter, Completions arm pre-config, require_confirmation + unit test
- `crates/ignition-cli/src/cli.rs` - Completions { shell: Shell } variant (value_enum), version doc refresh
- `crates/ignition-cli/src/render.rs` - Completions stdout exception, human gateway/warning lines
- `crates/ignition-cli/tests/cli_chassis.rs` - 5 completions tests (3 shells, bare exit 2, --json bypass)
- `crates/ignition-cli/tests/contract_version.rs` - with-config golden repinned on dead port + unreachable warning
- `crates/ignition-core/src/error.rs` - ConfirmationRequired hint names --yes + IGNITION_YES=1
- `README.md` - completions stdout exception in the contract section
- `Cargo.toml` / `crates/*/Cargo.toml` / `Cargo.lock` - async-trait, clap_complete, wiremock (cli dev-dep)

## Decisions Made
See key-decisions frontmatter. Highlights: Option<Credential> reconciliation, the semver-strictness fix in below_minimum, serde(skip) endpoint on GatewayInfo, completions-before-config-load, dead-port goldens.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Research's below_minimum comparison constant would never parse**
- **Found during:** Task 1 (client/version.rs)
- **Issue:** The research sketch compares against `semver::Version::parse("8.3.1.0")` after appending `.0` to every input — semver is strictly three-component, so the constant panics on unwrap and even the fixed form would mark "8.3.2" too old
- **Fix:** Compare against plain `8.3.1`; append `.0` only when the cleaned version has exactly one dot (the short "8.3" form); also trim surrounding whitespace
- **Files modified:** crates/ignition-core/src/client/version.rs
- **Verification:** 10-row below_minimum boundary unit test including the plan's full case list
- **Committed in:** c9ec700 (Task 1 commit)

**2. [Plan reconciliation] ReqwestGatewayApi credential is Option<Credential>**
- **Found during:** Task 1/2 boundary (header-less construction)
- **Issue:** Task 1 sketches `credential: Credential`, but Task 2's LOCKED matrix row 4 ("no secret for a *check* → proceed header-less") requires constructing the client without a credential
- **Fix:** `Option<Credential>` with a `None => {}` match arm (header-less); the auth rule stays token XOR basic XOR neither
- **Files modified:** crates/ignition-core/src/client/mod.rs
- **Verification:** no_credential_is_header_less wiremock test (both auth headers absent)
- **Committed in:** c9ec700 (Task 1 commit)

**3. [Rule 3 - Blocking] Existing with-config version golden would flake on developer machines**
- **Found during:** Task 2 (running the full suite)
- **Issue:** `version_json_envelope_with_config` pinned a profile at localhost:9088 — harmless before, but Task 2 makes version CONNECT to it: a machine actually running Ignition on 9088 would receive a real gateway-info response and break the golden
- **Fix:** Repinned the fixture on the dead port `http://127.0.0.1:1/` and the golden now locks the unreachable-warning output deterministically
- **Files modified:** crates/ignition-cli/tests/contract_version.rs
- **Verification:** golden passes; SNAPSHOTS=overwrite no-op
- **Committed in:** 3fbb94d (Task 2 commit)

**4. [Rule 2 - Missing Critical] ConfirmationRequired hint did not name the env escape hatch**
- **Found during:** Task 3 (guard implementation)
- **Issue:** The plan requires the hint "re-run with --yes or set IGNITION_YES=1"; the 01-02 hint only named the flag (agents and scripts need the non-interactive path too)
- **Fix:** Updated `CoreError::hint()` for ConfirmationRequired to name both; no golden referenced the old wording yet
- **Files modified:** crates/ignition-core/src/error.rs
- **Verification:** confirmation_guard_refuses_without_yes asserts both substrings
- **Committed in:** 7f8f6cb (Task 3 commit)

**5. [Rule 1 - Bug] Shell completions would break on an invalid config**
- **Found during:** Task 3 (dispatch design)
- **Issue:** dispatch loads config before matching — an unreadable config.toml would exit 3 on `completions`, hostile to shell install-time sourcing
- **Fix:** Completions returns from dispatch BEFORE config load (documented runtime-unreachable match arm for exhaustiveness)
- **Files modified:** crates/ignition-cli/src/main.rs
- **Verification:** completions tests run with nonexistent config paths (chassis harness) — success
- **Committed in:** 7f8f6cb (Task 3 commit)

---

**Total deviations:** 5 auto-fixed (2 bug, 1 blocking, 1 missing critical, 1 plan reconciliation)
**Impact on plan:** All fixes required for the plan's own must_haves and for deterministic tests; no scope creep. render.rs/error.rs touched beyond the per-task file lists as forced consequences of the ActionOutput refactor and guard hint.

## Issues Encountered
- clap_complete generates into an `io::Write` — `String` only implements `fmt::Write`, so completions buffer as `Vec<u8>` and convert (noted inline)
- Bash completion marker is `complete -F _ign -o bashdefault -o default ign` (flag order differs from the research example's `complete -o default -F`) — test marker adjusted
- `expose()` count in client/mod.rs is 3 call sites (token, basic user, basic password) rather than the plan's expected 1-2 — all three sit inside the single header-construction match; the one-location audit boundary holds

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Phase 1 COMPLETE: every CORE requirement implemented and test-enforced; `ign` is installable/configurable with multiple profiles, secure auth, completions, and the frozen machine-readable contract
- Phase 2 builds the full client by ADDING trait methods to GatewayApi (status, modules, logs…) — zero restructuring; the wiremock harness, error mapping, and for_tests constructor are waiting
- The research Open Question 1 (gateway-info truly unauthenticated on a hardened live gateway) remains flagged for Phase 2's live-gateway check, as planned

## Self-Check: PASSED

Verified: all 6 created files exist on disk; commits c9ec700 / 3fbb94d / 7f8f6cb present on main; `cargo test --workspace` green (61 passed across 11 suites + 1 ignored-by-design keyring smoke); `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --check` clean; `cargo build -p ignition-cli --no-default-features` green; SNAPSHOTS=overwrite no-op (zero golden drift); `grep process::exit` = 0; `grep read_line/confirm` interactive prompts = 0; `impl Serialize for Secret` = 0; expose() src call sites outside secret.rs confined to client/mod.rs header construction; `ign completions bash|zsh|fish` emit with shell markers; bare `completions` exits 2; `ign version` no-config exits 0 human + JSON.

---
*Phase: 01-foundation-agentic-contracts*
*Completed: 2026-08-21*
