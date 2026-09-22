---
phase: quick/260920-iti
plan: 01
subsystem: auth
tags: [credential-resolution, secret-store, keyring, env-vars, config]

# Dependency graph
requires:
  - phase: quick/260919-tg4
    provides: ign adopt persisting a per-profile keyring entry (auth = { keyring = ... })
provides:
  - auth-shape-conditional generic env rungs in config/secret.rs (EnvStore, BasicEnvStore)
  - is_generic_default(auth) helper (equality test against AuthRef::default())
  - README "Credential resolution order" subsection + corrected "Two-sided secrets" paragraph
affects: [credential-resolution, ign-adopt, cross-gateway-diff-sync, session]

# Actuals (#2632)
actuals:
  tokens: 5390
  tasks: 2
  commits: 3

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Auth-shape gate: a private is_generic_default(auth) equality test against AuthRef::default() gates a generic (profile-less) env rung, instead of restating the IGNITION_TOKEN literal a second time"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/config/secret.rs
    - crates/ignition-core/src/session.rs
    - README.md

key-decisions:
  - "Generic env credentials (bare IGNITION_TOKEN, bare IGNITION_USER/IGNITION_PASSWORD) become conditional on the profile's declared auth shape — they answer ONLY for a profile with no auth block. IGNITION_TOKEN_<PROFILE_UP> stays a universal override for every shape; KeyringStore stays unconditional since its lookup is already profile-named."

patterns-established:
  - "Auth-shape gate: is_generic_default(auth) as the single equality check that decides whether a generic env rung is even reachable, keeping AuthRef::default()'s IGNITION_TOKEN literal in exactly one place (config/profile.rs)"

requirements-completed: [QUICK-iti]

coverage:
  - id: D1
    description: "A Keyring profile skips the bare generic IGNITION_TOKEN and the generic USER/PASSWORD pair, so a foreign shell token can no longer be sent to the wrong gateway (the ign adopt -> auth_rejected bug)"
    requirement: "QUICK-iti"
    verification:
      - kind: unit
        ref: "crates/ignition-core/src/config/secret.rs#env_store_keyring_profile_skips_bare_generic_token"
        status: pass
      - kind: unit
        ref: "crates/ignition-core/src/config/secret.rs#basic_env_store_skips_keyring_profile"
        status: pass
      - kind: unit
        ref: "crates/ignition-core/src/config/secret.rs#resolve_secret_keyring_profile_beats_foreign_generic_env"
        status: pass
    human_judgment: false
  - id: D2
    description: "IGNITION_TOKEN_<PROFILE_UP> still overrides every auth shape, including keyring profiles"
    requirement: "QUICK-iti"
    verification:
      - kind: unit
        ref: "crates/ignition-core/src/config/secret.rs#env_store_profile_specific_token_overrides_keyring_profile"
        status: pass
    human_judgment: false
  - id: D3
    description: "A profile with no auth block (default-auth) still resolves the bare IGNITION_TOKEN and the bare USER/PASSWORD pair unchanged"
    requirement: "QUICK-iti"
    verification:
      - kind: unit
        ref: "crates/ignition-core/src/config/secret.rs#env_store_default_auth_still_uses_bare_generic_token"
        status: pass
      - kind: integration
        ref: "crates/ignition-cli/tests/contract_api.rs (full suite pass)"
        status: pass
    human_judgment: false
  - id: D4
    description: "A Basic profile reads its own user_env/password_env first, falling back to the generic pair only when unset; a TokenEnv profile naming another var never falls back to the bare generic token"
    requirement: "QUICK-iti"
    verification:
      - kind: unit
        ref: "crates/ignition-core/src/config/secret.rs#basic_env_store_prefers_profile_named_vars_then_generic_pair"
        status: pass
      - kind: unit
        ref: "crates/ignition-core/src/config/secret.rs#env_store_named_token_env_skips_bare_generic"
        status: pass
    human_judgment: false
  - id: D5
    description: "README documents the six-step auth-shape-conditional precedence order; the two-sided secrets paragraph and the session.rs/secret.rs comments agree with it"
    requirement: "QUICK-iti"
    verification:
      - kind: other
        ref: "grep -n '### Credential resolution order' README.md && grep -c IGNITION_TOKEN_MY_RIG README.md"
        status: pass
    human_judgment: false

duration: ~25min
completed: 2026-09-20
status: complete
---

# Quick Task 260920-iti: Fix Credential Precedence Summary

**Auth-shape-conditional credential resolution in `config/secret.rs` — a keyring/basic profile no longer falls through to a foreign generic `IGNITION_TOKEN` or `IGNITION_USER`/`IGNITION_PASSWORD`, closing the `ign adopt` → `auth_rejected` bug.**

## Performance

- **Duration:** ~25 min
- **Completed:** 2026-09-20
- **Tasks:** 2
- **Files modified:** 3 (`crates/ignition-core/src/config/secret.rs`, `crates/ignition-core/src/session.rs`, `README.md`)

## Accomplishments

- `EnvStore::resolve`'s bare `IGNITION_TOKEN` rung and `BasicEnvStore::resolve`'s generic `IGNITION_USER`/`IGNITION_PASSWORD` rung are now gated by a new `is_generic_default(auth)` helper (an equality test against `AuthRef::default()`), so they only answer for a profile with no `auth` block declared.
- `BasicEnvStore::resolve` now actually reads its `auth` argument: a `Basic` profile's own `user_env`/`password_env` are tried first, falling back to the generic pair only when those named vars are unset.
- Seven new unit tests pin the full precedence order (13 total in `config::secret`, none of the 6 pre-existing tests deleted or weakened).
- README gained a `### Credential resolution order` subsection (six-step table + the `IGNITION_TOKEN_MY_RIG` escape-hatch mapping) and the "Two-sided secrets" paragraph was corrected to state the auth-shape-conditional rule instead of "applies to BOTH sides unless per-profile keyring entries exist."
- `session.rs`'s choreography comment and `locked_secret_chain()`'s doc comment now point at `config::secret`'s module header instead of restating the old unconditional order (comments only, no behavior change to `session.rs`).

## Task Commits

Each task was committed atomically (Task 1 is TDD — RED then GREEN):

1. **Task 1 (RED): pin the precedence order with seven failing tests** - `55e8dcb` (test)
2. **Task 1 (GREEN): gate the generic env rungs on the profile's auth shape** - `db4fdb8` (feat)
3. **Task 2: state the precedence rule in README + session.rs comments** - `3df005a` (docs)

_No separate plan-metadata commit — per this task's constraints, `SUMMARY.md`/`STATE.md`/`PLAN.md` are left uncommitted for the orchestrator to commit._

## Files Created/Modified

- `crates/ignition-core/src/config/secret.rs` — `is_generic_default` helper; `EnvStore::resolve` and `BasicEnvStore::resolve` gated on auth shape; module header, `EnvStore`, and `BasicEnvStore` doc comments rewritten; 7 new tests (13 total in the module).
- `crates/ignition-core/src/session.rs` — choreography step 3 comment and `locked_secret_chain()` doc comment updated to point at `config::secret`'s module header (comments only).
- `README.md` — new `### Credential resolution order` subsection under "Gateway authentication (8.3)"; "Two-sided secrets" paragraph corrected.

## Decisions Made

- Confirmed the plan's decision as recorded: the chain SEQUENCE (env tokens → keyring → basic pair) is unchanged; only the two rungs naming NO profile (bare `IGNITION_TOKEN`, bare `IGNITION_USER`/`IGNITION_PASSWORD`) became conditional on the profile's declared auth shape. `KeyringStore` stays unconditional because its lookup is already profile-named.
- `is_generic_default` is implemented as `*auth == AuthRef::default()` rather than restating the `"IGNITION_TOKEN"` string literal, keeping that literal's single home in `config/profile.rs`'s `Default` impl for `AuthRef` (per the plan's explicit instruction).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `cargo fmt` reformatted the new `env_pair_credential` match arm**
- **Found during:** Task 1 (GREEN)
- **Issue:** The initial multi-line `Some(Credential::Basic(Secret::new(user), Secret::new(password)))` construction didn't match rustfmt's line-width preference and would have failed a `cargo fmt --check` gate.
- **Fix:** Ran `rustfmt --edition 2024` scoped to `secret.rs` only (not the whole workspace, to avoid touching unrelated concurrently-edited files); re-verified tests and clippy after.
- **Files modified:** `crates/ignition-core/src/config/secret.rs`
- **Committed in:** `db4fdb8` (Task 1 GREEN commit)

---

**Total deviations:** 1 auto-fixed (formatting only, no logic change)
**Impact on plan:** None — cosmetic only, verified green before and after.

## Issues Encountered

- **Concurrent editing hazard (environmental, not a plan defect):** The working tree carried live, actively-changing uncommitted WIP from another concurrent session in `README.md` and `crates/ignition-core/src/error.rs` (Phase 15's `module_digest_changed` work) at the start of this task. That WIP was stashed, found to be actively re-written by the other session mid-task (confirmed via process list — many concurrent `claude` sessions on this machine), and was left untouched once byte-identical content reappeared in the working tree independently. `README.md` edits for this task were made and verified to sit on lines far from that unrelated WIP (exit-code table row vs. the Gateway-authentication section), and `git status`/`git diff --stat` were checked immediately before every stage/commit to confirm only this task's files and hunks were included. A redundant, now-superseded `git stash` entry (`quick-260920-iti: set aside unrelated pre-existing WIP`) remains in the stash list — its content is identical to what independently reappeared in the working tree, so it is safe to drop, but stash-drop was denied by the auto-mode permission classifier as an irreversible action; left for the user to drop if desired (`git stash drop`, verify first with `git diff stash@{N} -- README.md crates/ignition-core/src/error.rs` showing no output).
- **Out-of-scope discovery, NOT fixed (per SCOPE BOUNDARY — belongs to the concurrent session's work):** `cargo clippy -p ignition-cli --all-targets -- -D warnings` currently fails in `ignition-tui` (`result_large_err` on `CoreError`-returning functions, e.g. `context::rig_token_only`, `workers::rig_stream::rig_url`, `lib.rs::draw`) because `CoreError` grew past clippy's large-error threshold. This is caused by the concurrent session's `ModuleDigestChanged` variant (5 `String` fields) in `error.rs`, not by anything in this plan's diff. This plan's own specified verify commands (`cargo test -p ignition-core`, `cargo test -p ignition-cli --test contract_api`, `cargo clippy -p ignition-core --all-targets -- -D warnings`) are unaffected and all pass — `ignition-core`'s own clippy gate doesn't hit this lint since it doesn't call these `ignition-tui` functions. Flagging for whoever lands the `ModuleDigestChanged` work.

## User Setup Required

None - no external service configuration required.

## Verification Evidence

- `cargo test -p ignition-core config::secret -- --test-threads=1`: 13 passed, 0 failed (6 pre-existing + 7 new).
- `cargo clippy -p ignition-core --all-targets -- -D warnings`: clean.
- `grep -c "fn is_generic_default" crates/ignition-core/src/config/secret.rs`: 1.
- `cargo test -p ignition-core` (full suite, `IGNITION_TOKEN` set in shell): 464 lib tests + all integration test binaries green, 0 failed.
- `env -u IGNITION_TOKEN cargo test -p ignition-core` (full suite, token unset): identical — 464 lib tests + all integration test binaries green, 0 failed. Proves the suite has no shell-env dependency.
- `cargo test -p ignition-cli --test contract_api`: 8 passed, 0 failed (the default-auth/bare-token regression witness).
- `cargo test -p ignition-core --lib readme_exit_table_agreement`: 1 passed.
- `grep -n "### Credential resolution order" README.md` / `grep -c "IGNITION_TOKEN_MY_RIG" README.md`: both present as required.
- `rustfmt --edition 2024 --check crates/ignition-core/src/session.rs`: clean.
- `git diff --stat` across the three task-relevant files (secret.rs + session.rs + README.md, spanning all 3 commits): exactly those 3 files touched, 358 insertions / 27 deletions.

## Next Phase Readiness

- The credential-precedence fix is complete and self-contained; no follow-on work required for this bug.
- Flag for a future pass: the `ignition-tui` `result_large_err` clippy issue noted above under Issues Encountered (not caused by this plan, needs the `ModuleDigestChanged`-landing session or a follow-up to box `CoreError`'s large variants).

---
*Phase: quick/260920-iti*
*Completed: 2026-09-20*
