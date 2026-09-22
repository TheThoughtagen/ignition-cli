---
phase: quick/260920-iti
plan: 01
type: execute
wave: 1
depends_on: []
autonomous: true
requirements: [QUICK-iti]
files_modified:
  - crates/ignition-core/src/config/secret.rs
  - crates/ignition-core/src/session.rs
  - README.md

estimate:
  tokens: 45000
  raw_tokens: 45000
  tasks: 2
  confidence: low

must_haves:
  truths:
    - "A profile whose auth is `{ keyring = \"profile:e2e\" }` authenticates with its keyring token even while a foreign bare `IGNITION_TOKEN` is exported in the shell (the `ign adopt` → `ign --profile e2e status` → `auth_rejected` failure closes)"
    - "`IGNITION_TOKEN_<PROFILE_UP>` still overrides every auth shape, keyring profiles included — the universal explicit escape hatch is unchanged"
    - "A profile with no `auth` block (including one whose URL comes from the `IGNITION_URL` overlay) still resolves the bare `IGNITION_TOKEN` exactly as before"
    - "`IGNITION_USER`/`IGNITION_PASSWORD` never reach a keyring profile"
    - "A `Basic` profile resolves its OWN named user/password vars, falling back to the generic pair only when those named vars are unset"
    - "The README's authentication section states the auth-shape-conditional precedence order, and the `secret.rs` module header states the same rule in the same words"
  artifacts:
    - crates/ignition-core/src/config/secret.rs
    - README.md
  key_links:
    - "`EnvStore::resolve` generic rung → an `AuthRef::default()` equality test (the `\"IGNITION_TOKEN\"` literal keeps its single home in `config/profile.rs`)"
    - "`BasicEnvStore::resolve` → the `AuthRef` it is finally allowed to read (it takes the arg today and ignores it)"
    - "`config::resolve_secret` → `session::locked_secret_chain()` — the ONE chain construction site every command, the TUI, and both diff/sync sides flow through"
    - "`ign adopt` persist step (rewrites the profile to `auth = { keyring = … }`) → EnvStore's new skip — that rewrite is what turns a stale shell token into a 401 today"
---

<objective>
Make a profile's explicitly declared credential win over the generic, profile-less
environment credentials. Today `EnvStore` falls through to the bare `IGNITION_TOKEN`
for EVERY profile, so `ign --profile e2e status` sends an unrelated shell token to a
gateway whose profile says `auth = { keyring = "profile:e2e" }` — and answers
`auth_rejected` immediately after a successful `ign adopt` stored the correct key.

Purpose: an adopted profile must authenticate as itself. A developer's shell token for
gateway A must not silently become gateway B's credential.
Output: auth-shape-conditional generic rungs in `config/secret.rs`, seven unit tests
pinning the precedence order under `ENV_LOCK`, and the rule stated in the README plus
the two source comments that currently describe the old unconditional fallback.

Task 1 is the whole behavior slice (resolver + chain-level reproduction pin); task 2
carries the rule into the docs and the chain-construction comment.
</objective>

## Decision: generic env credentials become auth-shape-conditional (supersedes the CORE-02 comment)

**Status:** decided by the requester, not open. Recorded here because the `secret.rs`
module header currently calls the old behavior "LOCKED (CORE-02 must-have)" and this
plan edits that claim.

**What changes:** the chain SEQUENCE is untouched (env tokens → keyring → basic pair).
What changes is that the two rungs naming NO profile — bare `IGNITION_TOKEN` and the
bare `IGNITION_USER`/`IGNITION_PASSWORD` pair — become conditional on the profile's
declared auth shape.

**Why:** CORE-02's env-first order was written for a one-gateway developer where a bare
`IGNITION_TOKEN` was the only credential in play. Since ADOPT-02, `ign adopt` persists a
per-profile keyring entry and rewrites the profile to `auth = { keyring = … }` — so the
profile now declares exactly where its credential lives, and the generic rung's job is
over. Env-first still holds for env vars that NAME the profile (`IGNITION_TOKEN_<PROFILE>`)
or that the profile NAMES itself (`token_env`, `user_env`/`password_env`); it stops holding
for credentials that name nothing.

**The new order (the rule the tests and README must both state):**

| Step | Source | Applies to |
|------|--------|-----------|
| 1 | `IGNITION_TOKEN_<PROFILE_UP>` | EVERY auth shape — the universal explicit override (unchanged) |
| 2 | the profile's own `token_env` var | `TokenEnv` profiles (unchanged) |
| 3 | bare `IGNITION_TOKEN` | ONLY a profile whose auth equals `AuthRef::default()` — i.e. no `auth` key in the TOML (NEW gate) |
| 4 | keyring entry `ignition-cli` / `profile:<name>` | every profile (unchanged, and unconditional — see below) |
| 5 | the profile's own `user_env`/`password_env` | `Basic` profiles (NEW read — those names are declared today and never consulted) |
| 6 | bare `IGNITION_USER` + `IGNITION_PASSWORD` | a default-auth profile, or a `Basic` profile whose own named vars are unset (NEW gate) |

**`KeyringStore` is deliberately NOT gated.** Its lookup is already profile-named
(`profile:<name>`), so it cannot serve one profile's credential to another; a
token-env profile simply has no entry and the store returns `Ok(None)` as it does today.
Gating it would break the headless/adopt interplay for no safety gain.

**Exhaustion is the correct loud failure.** A keyring profile with an empty keyring and a
foreign `IGNITION_TOKEN` in the shell now exhausts to `SecretUnavailable` (exit 3, hint
names the env path) instead of sending the wrong token and getting exit 5 `auth_rejected`.
That is the intended trade: a diagnosable refusal beats a misleading rejection.

## Regression surface (read before editing — these must NOT change)

- `crates/ignition-cli/src/main.rs:2121`, `:2167`, `:2190` — the rig family
  (`rig trial reset`, `rig snapshot`, `rig restore`) reads `IGNITION_TOKEN` DIRECTLY,
  never through `EnvStore`, because a rig has no profile at all. Out of scope, untouched.
- `Session::for_url` (session.rs ~208) synthesizes a profile with `AuthRef::default()`
  but passes the credential EXPLICITLY — no resolution happens there. Untouched.
- `crates/ignition-cli/tests/contract_api.rs` writes a config with no `auth` key and sets
  bare `IGNITION_TOKEN`; that fixture rides step 3 and must stay green — it is the
  regression witness for the default-profile path.
- `session.rs`'s `two_profile_toml` fixture likewise declares no `auth`, so
  `locked_chain_env_first_required_errors_degraded_headerless` and the selection tests
  stay green unchanged.
- `KeyringStore`, `KeyringStore::set`/`delete`, and `actions/adopt.rs` — untouched.
- No GUARDED_OPS surface, no new dependency, no clap/CLI surface change.

<execution_context>
@$HOME/.claude/gsd-core/workflows/execute-plan.md
@$HOME/.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.claude/CLAUDE.md

@crates/ignition-core/src/config/secret.rs
@crates/ignition-core/src/config/profile.rs
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: Gate the generic env rungs on the profile's declared auth shape</name>
  <files>crates/ignition-core/src/config/secret.rs</files>
  <read_first>
    `crates/ignition-core/src/config/profile.rs` — `AuthRef`'s three variants and its
    `Default` impl (`TokenEnv { token_env: "IGNITION_TOKEN" }`). `AuthRef` derives
    `PartialEq`, which is what makes the "is this the generic default" test a one-liner
    without restating the var-name literal.
  </read_first>
  <behavior>
    New tests in `secret.rs`'s existing `mod tests`, all holding `ENV_LOCK`:
    - Test 1 `env_store_keyring_profile_skips_bare_generic_token`: auth `Keyring { keyring: "profile:e2e" }`,
      `IGNITION_TOKEN` = "foreign", `IGNITION_TOKEN_E2E` removed → `EnvStore::resolve("e2e", …)` is `Ok(None)`.
      This is the live failure, reproduced at the store level.
    - Test 2 `env_store_profile_specific_token_overrides_keyring_profile`: same auth, plus
      `IGNITION_TOKEN_E2E` = "specific" → `Some(Token("specific"))` (step 1 stays universal).
    - Test 3 `env_store_default_auth_still_uses_bare_generic_token`: `AuthRef::default()`,
      `IGNITION_TOKEN` = "generic", profile-specific var removed → `Some(Token("generic"))`.
      The no-auth-block / `IGNITION_URL`-overlay path is pinned unchanged.
    - Test 4 `env_store_named_token_env_skips_bare_generic`: auth `TokenEnv { token_env: "MY_TOKEN" }`
      with `MY_TOKEN` removed and `IGNITION_TOKEN` set → `Ok(None)`.
    - Test 5 `basic_env_store_skips_keyring_profile`: auth `Keyring`, `IGNITION_USER` +
      `IGNITION_PASSWORD` both set → `BasicEnvStore::resolve` is `Ok(None)`.
    - Test 6 `basic_env_store_prefers_profile_named_vars_then_generic_pair`: auth
      `Basic { user_env: "GW_USER", password_env: "GW_PASS" }` — with `GW_USER`/`GW_PASS` set,
      those values come back; with them removed and `IGNITION_USER`/`IGNITION_PASSWORD` set,
      the generic pair comes back.
    - Test 7 `resolve_secret_keyring_profile_beats_foreign_generic_env`: the chain-level pin —
      `EnvStore` + a `FixedStore` standing in for a populated keyring + `BasicEnvStore`, with
      `IGNITION_TOKEN`, `IGNITION_USER`, `IGNITION_PASSWORD` all set and auth `Keyring` →
      the FixedStore's token wins. This is the end-to-end shape of the live bug.
  </behavior>
  <action>
    Add a private helper in `secret.rs` — name it `is_generic_default(auth: &AuthRef) -> bool` —
    implemented as an equality test against `AuthRef::default()`. Do NOT restate the
    `IGNITION_TOKEN` string literal: `profile.rs`'s `Default` impl is its one home.

    In `EnvStore::resolve`, leave the `IGNITION_TOKEN_<PROFILE_UP>` rung and the
    `AuthRef::TokenEnv { token_env }` rung exactly as they are, then guard the final bare
    `IGNITION_TOKEN` rung with `is_generic_default(auth)` so it is reached only by a profile
    that declared no auth. A `Keyring` profile, a `Basic` profile, and a `TokenEnv` profile
    naming any other variable all fall through to `Ok(None)` and let the next store answer.

    In `BasicEnvStore::resolve`, start using the `auth` argument it currently ignores
    (drop the leading underscore on the parameter). Factor the two-var read into one small
    private helper taking two var names so the named-pair and generic-pair reads share it.
    For `AuthRef::Basic { user_env, password_env }`, read those named vars first and fall
    back to `IGNITION_USER`/`IGNITION_PASSWORD` when they are not both set. For a
    default-auth profile, read the generic pair as today. For every other shape — a keyring
    profile, or a token-env profile naming its own var — return `Ok(None)` without touching
    the environment. Keep the existing "both vars or nothing" rule on every read path:
    a lone user is still not a credential.

    Rewrite the module header comment (lines 1-16) so it states the order from the plan's
    decision table: the profile-specific var applies to every auth shape, the profile's own
    named vars apply to the shape that declares them, and the two bare/generic rungs apply
    only to a profile with no `auth` block (plus the `Basic` named-vars-unset fallback). Say
    that the sequence is unchanged and only the generic rungs became conditional, and note
    that `KeyringStore` stays unconditional because its lookup is already profile-named.
    Update `EnvStore`'s and `BasicEnvStore`'s own doc comments to match — no comment in this
    file may still promise an unconditional generic fallback. Keep the existing note that the
    chain, not the structs, encodes the sequence.

    ENV HYGIENE — this is the trap that will make a green test lie: `cargo test` inherits the
    developer's shell, and that shell HAS `IGNITION_TOKEN` exported (it is what produced the
    bug report). Every new test must explicitly `remove_var` each variable it needs ABSENT —
    the profile-specific var in tests 1/3/4, `MY_TOKEN` in test 4, `GW_USER`/`GW_PASS` in the
    second half of test 6 — not merely decline to set it. Follow the file's existing
    discipline exactly: hold `ENV_LOCK` for the whole body, carry the `// SAFETY:
    single-threaded under ENV_LOCK` comment on each unsafe block, and remove every variable
    the test set before returning.

    Reuse the existing `FixedStore` double for test 7; no test in this file may touch a real
    OS keychain (that stays the `#[ignore]`-gated `tests/keyring_smoke.rs` job).
  </action>
  <verify>
    <automated>cargo test -p ignition-core config::secret 2>&1 | tail -20</automated>
    <automated>cargo clippy -p ignition-core --all-targets -- -D warnings</automated>
    <automated>grep -c "fn is_generic_default" crates/ignition-core/src/config/secret.rs</automated>
  </verify>
  <done>
    All seven new tests pass alongside the four pre-existing `config::secret` tests (11 total
    in the module, none deleted). Clippy is clean with `-D warnings`. A keyring-auth profile
    resolves `Ok(None)` from both generic stores while a default-auth profile still resolves
    the bare token.
  </done>
</task>

<task type="auto">
  <name>Task 2: State the precedence rule in the README and the chain-construction comment</name>
  <files>README.md, crates/ignition-core/src/session.rs</files>
  <read_first>
    README.md lines 118-144 (the `## Gateway authentication (8.3)` section, ending at the
    `### Token-setup troubleshooting` heading) and lines 948-958 (the "Two-sided secrets"
    paragraph under `### Cross-gateway diff & sync`). `session.rs` lines 16-30 (the
    choreography list) and `locked_secret_chain()` at ~line 281.
  </read_first>
  <action>
    Insert a new `### Credential resolution order` subsection into README.md immediately
    BEFORE the `### Token-setup troubleshooting` heading, so it sits inside the existing
    `## Gateway authentication (8.3)` section. Carry the six-step table from this plan's
    decision section verbatim in shape (Step / Source / Applies to), then three short
    paragraphs: (1) the generic `IGNITION_TOKEN` and `IGNITION_USER`/`IGNITION_PASSWORD`
    apply ONLY to a profile with no `auth` block — a profile that declares keyring or named
    env auth authenticates as itself; (2) `IGNITION_TOKEN_<PROFILE_UP>` is the universal
    escape hatch that still overrides any shape, with the `my-rig` → `IGNITION_TOKEN_MY_RIG`
    name mapping shown; (3) why this matters after `ign adopt` — adopt rewrites the profile
    to `auth = { keyring = … }`, and a leftover shell token for another gateway no longer
    shadows it. Close with the exhaustion note: a keyring profile with no entry now refuses
    exit 3 `secret_unavailable` rather than sending a foreign token and drawing exit 5
    `auth_rejected`.

    Correct the "Two-sided secrets" paragraph (~line 952): the sentence claiming
    `IGNITION_TOKEN` and the basic pair apply to BOTH sides "unless per-profile keyring
    entries exist" is now wrong twice over. State instead that generic env credentials reach
    a side only when that side's profile declares no `auth` block, that a side declaring
    keyring or named-env auth resolves its own credential, and keep the existing
    recommendation to store per-profile tokens via `ign profile add --keyring`. Link the
    reader to the new `### Credential resolution order` subsection.

    In `session.rs`, extend choreography step 3 (line ~22) and the `locked_secret_chain()`
    doc comment so both say the sequence is unchanged but the generic rungs are conditional
    on the profile's auth shape, pointing at `config::secret`'s module header as the full
    statement. Comments only — do not change `locked_secret_chain()`'s contents or any
    behavior in this file.

    Do not touch the exit-code table (`error::tests::readme_exit_table_agreement` parses it
    via `include_str!`) or any other README section.
  </action>
  <verify>
    <automated>grep -n "### Credential resolution order" README.md</automated>
    <automated>grep -c "IGNITION_TOKEN_MY_RIG" README.md</automated>
    <automated>cargo test -p ignition-core 2>&1 | tail -15</automated>
    <automated>cargo test -p ignition-cli --test contract_api 2>&1 | tail -10</automated>
  </verify>
  <done>
    The README carries a `### Credential resolution order` subsection under Gateway
    authentication with the six-step table and the escape-hatch name mapping; the two-sided
    secrets paragraph no longer describes the generic token as applying to every profile.
    The full `ignition-core` suite is green (including `readme_exit_table_agreement` and the
    `session` chain tests) and `contract_api` — the default-auth/bare-token witness — passes
    unchanged.
  </done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| process environment → gateway HTTP request | an ambient, profile-less env var becomes an `X-Ignition-API-Token` / `Authorization` header sent to whichever gateway the selected profile names |
| OS keyring → process memory | the per-profile entry `ignition-cli` / `profile:<name>` is read into a `Secret` |
| profile config (`config.toml`) → credential selection | the `auth` reference decides WHICH source is legitimate for this gateway |

## STRIDE Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation Plan |
|-----------|----------|-----------|----------|-------------|-----------------|
| T-iti-01 | Information Disclosure | `EnvStore::resolve` bare `IGNITION_TOKEN` rung | high | mitigate | THE bug: a token minted for gateway A is transmitted to gateway B whenever B's profile is selected, exposing A's credential to a host that must never see it (and to B's request logs). Gate the rung on `is_generic_default(auth)` so a profile declaring keyring or named-env auth never receives it. Pinned by tests 1 and 7. |
| T-iti-02 | Information Disclosure | `BasicEnvStore::resolve` generic pair | high | mitigate | Same leak in the basic-auth shape: `IGNITION_USER`/`IGNITION_PASSWORD` were sent for EVERY profile, including keyring profiles, and 8.3 `/data` rejects basic auth anyway — so the credential is spent for nothing. Skip non-default, non-`Basic` shapes entirely. Pinned by test 5. |
| T-iti-03 | Spoofing | credential substitution across gateways | high | mitigate | A wrong-credential 401 is indistinguishable from a revoked-token 401, so the operator debugs the wrong gateway. After the gate, a missing credential exhausts to `SecretUnavailable` (exit 3) whose hint names the env path — a diagnosable refusal instead of a misleading rejection. |
| T-iti-04 | Denial of Service | tightened gate locks out a working setup | medium | mitigate | Deliberate escape hatch retained: `IGNITION_TOKEN_<PROFILE_UP>` still overrides every shape (test 2), and the no-`auth`-block path is unchanged (test 3, plus the `contract_api` witness). Headless CI keeps a supported route that needs no keyring. |
| T-iti-05 | Information Disclosure | new unit tests handling credential strings | low | accept | Tests use synthetic literals only, run under `ENV_LOCK`, and touch no real keychain (`FixedStore` double); `Secret`'s `Debug`/`Display` redaction is untouched, so an assertion failure cannot print a value. |
| T-iti-06 | Tampering | README/comment drift re-legitimizing the old fallback | medium | mitigate | The rule is written in three places that are edited in the same change set (module header, `locked_secret_chain` doc, README subsection), and the behavior itself is pinned by seven tests — prose drift cannot change what resolves. |

No package-manager installs are introduced by this plan (no new crate, no `cargo add`),
so no supply-chain (`T-…-SC`) row applies and no Package Legitimacy Gate is required.
</threat_model>

<verification>
- `cargo test -p ignition-core` green — the 11 `config::secret` tests, the `session` chain
  and selection tests, and `error::tests::readme_exit_table_agreement`.
- `cargo test -p ignition-cli --test contract_api` green (the bare-token/default-auth rung).
- `cargo clippy -p ignition-core --all-targets -- -D warnings` and `cargo fmt --check` clean.
- No pre-existing test deleted or weakened: `env_store_profile_specific_token_wins`,
  `env_store_token_env_ref_and_suffix_mapping`, `basic_env_store_requires_both_vars`, and
  `resolve_secret_chain_order_first_some_wins_and_exhaustion` all still pass as written.
- `git diff --stat` touches exactly three files: `config/secret.rs`, `session.rs`, `README.md`.
- NO git worktree is created at any point (project rule); work happens in the main checkout.
- Each commit stages an explicit file list — no repo-root artifacts (the working tree already
  carries untracked `.gwbk`/`uat-*` scratch files that must stay untracked).
</verification>

<success_criteria>
- A `Keyring` profile resolves `Ok(None)` from both generic env stores while a foreign
  `IGNITION_TOKEN` is set, so the keyring rung answers — the `ign adopt` → `ign --profile e2e
  status` → `auth_rejected` sequence cannot recur.
- `IGNITION_TOKEN_<PROFILE_UP>` still overrides every auth shape.
- A profile with no `auth` block still resolves the bare `IGNITION_TOKEN`, and the basic pair
  still works for it.
- A `Basic` profile reads its own `user_env`/`password_env` first, generic pair second.
- README `### Credential resolution order` documents the six-step order; the two-sided
  secrets paragraph and both source comments agree with it.
</success_criteria>

<output>
Create `.planning/quick/260920-iti-fix-credential-precedence-a-profile-with/260920-iti-SUMMARY.md` when done.
</output>
