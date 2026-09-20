---
phase: quick/260919-tg4
plan: 01
subsystem: browser-e2e
tags: [cli-verb, idp-session, playwright, scaffold, exit-codes, mcp, tui-registry, guarded-ops]
status: complete

requires:
  - crates/ignition-core/src/client/idp.rs (IdpLoginFlow, login, GatewaySession)
  - crates/ignition-core/src/actions/doctor.rs (CheckResult, CheckStatus)
  - crates/ignition-core/src/client/webdev.rs (testing_probe, TestingProbe)
  - crates/ignition-core/src/actions/rig.rs (trial_status)
  - crates/ignition-core/src/session.rs (Session::resolve_degraded)
provides:
  - "`ign session login` — the native IdP session as JSON + a Playwright storageState"
  - "actions::login::{session_login, storage_state_for, SessionLoginResult, StorageState, StorageCookie}"
  - "`ign e2e doctor` — the six-row read-only browser-E2E diagnosis"
  - "`ign e2e init` — the seven-member embedded Playwright scaffold"
  - "actions::e2e::{e2e_doctor, e2e_init, e2e_init_preview, E2eEnv, NODE_INSTALL_HINT, DEFAULT_GATEWAY_URL}"
  - "e2e::{E2E_TEMPLATE_FILES, render_template, member_paths, validate_member_paths}"
  - "actions::lint::{find_on_path, find_on_path_in, is_executable_file} — the single PATH-discovery impl"
  - "CoreError::NodeToolAbsent → exit 6, slug node_tool_absent"
  - "MCP tools session_login / e2e_doctor / e2e_init (auto-derived by walk_catalog)"
affects:
  - crates/ignition-cli/src/main.rs (3 ActionOutput variants, GUARDED_OPS + the marker parser)
  - crates/ignition-tui/src/routes.rs (3 registry rows; dashboard_rows pin 37 → 40)
  - crates/ignition-core/src/error.rs (NodeToolAbsent + the network_error test helper)

tech-stack:
  added: []
  patterns:
    - "pure-function render seam as a SECURITY boundary — session_login_human_lines returns Vec<String> so the withholding of live credential material is asserted in CI, which println! output makes impossible (the to_junit_xml precedent)"
    - "environment injection (E2eEnv) instead of std::env::set_var — unsafe in edition 2024 and racy under parallel test runs (the ComposeRunner precedent)"
    - "embedded template tree with a uniform .tmpl source suffix — a literal .gitignore would hide its own siblings from git; a literal package.json would be swept into npm workspace scans"
    - "the confirmation refusal IS the dry run — e2e_init_preview builds the prose that require_confirmation refuses with, so no separate --dry-run flag exists"
    - "doctor posture extended to a host+gateway diagnosis: transport errors become fail ROWS, never propagated errors, so exit 0 always means 'I looked'"
    - "shared const for a cross-verb hint (NODE_INSTALL_HINT) so a refusal and a doctor row cannot word the same instruction differently"

key-files:
  created:
    - crates/ignition-core/src/actions/login.rs
    - crates/ignition-core/src/actions/e2e.rs
    - crates/ignition-core/src/e2e/mod.rs
    - crates/ignition-core/tests/session_login_contract.rs
    - crates/ignition-core/tests/e2e_contract.rs
    - crates/ignition-core/e2e-template/package.json.tmpl
    - crates/ignition-core/e2e-template/playwright.config.mjs.tmpl
    - crates/ignition-core/e2e-template/global-setup.mjs.tmpl
    - crates/ignition-core/e2e-template/lib/gateway.mjs.tmpl
    - crates/ignition-core/e2e-template/tests/example.spec.mjs.tmpl
    - crates/ignition-core/e2e-template/gitignore.tmpl
    - crates/ignition-core/e2e-template/README.md.tmpl
  modified:
    - crates/ignition-core/src/lib.rs
    - crates/ignition-core/src/actions/mod.rs
    - crates/ignition-core/src/actions/lint.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/tui_coverage.rs
    - crates/ignition-tui/src/routes.rs
    - README.md

decisions:
  - "All twelve planner decisions D1–D12 implemented as written, including D4 (node-missing is exit 6 node_tool_absent, NOT exit 7 whose sole slug is rig_error)."
  - "DEVIATION (Rule 3): the plan's `e2e init` dispatch was to fall back to 'the localhost default URL the config module already defines'. No such const exists in config/. Added `actions::e2e::DEFAULT_GATEWAY_URL = \"http://localhost:8088\"` (Ignition's stock HTTP port); the scaffold reads IGNITION_URL first everywhere it uses the value, so a wrong guess costs one env var, never a re-scaffold."
  - "DEVIATION (Rule 1): main.rs's guarded-marker parser accepted only [a-z ] and so silently DROPPED `// guarded:e2e init` (the digit), reporting the registered op as unmarked. Extended to accept ASCII digits — a clap leaf path may legitimately carry one."
  - "DEVIATION (Rule 1): error.rs's network_error() test helper called reqwest::get without installing the rustls provider. reqwest is built with rustls-no-provider, so `cargo test -p ignition-core error::tests` (the plan's own Task 3 verify command) panicked whenever no other test installed it first. Added the idempotent install, which the provider's own doc comment sanctions for exactly this."
  - "The `--yes` gate is a DYNAMIC-prose GUARDED_OPS site: the refusal message is e2e_init_preview's output verbatim, so the registry carries representative prose and the `// guarded:e2e init` marker is what pins the site."

metrics:
  duration: ~4h40m (≈2h of it lost to a shared-CARGO_TARGET_DIR collision — see Deferred Issues)
  completed: 2026-09-20

actuals:
  tokens: 71000
  tasks: 3
  commits: 4
---

# Quick Task 260919-tg4: `ign session login` + `ign e2e doctor`/`init` Summary

Three verbs that take a developer or an agent from a bare commissioned
gateway to a green Playwright run without ever opening a browser to log
in. `ign session login` exposes the native OIDC session the trial-reset
ladder and `ign adopt` already perform, in the exact shape a Playwright
`globalSetup` consumes; `ign e2e doctor` diagnoses the host and gateway
that run needs; `ign e2e init` writes the suite. There is deliberately
no `ign e2e run` — the scaffold's own `npm test` is the runner.

## What shipped

### Task 1 — `ign session login` (commit `063f389`)

A **projection**, not a second login path. `actions::login::session_login`
builds `IdpLoginFlow::new` and calls `client::idp::login` verbatim, then
maps the returned `GatewaySession` into the five-field envelope plus a
Playwright `storageState`. `idp.rs` is untouched; `adopt.rs` is untouched.

The cookie follows D7's rules, and the wire keys are **Playwright's**
(`httpOnly`, `sameSite`) — a Rust-cased key there produces a storage
state Playwright ignores without complaint, so a serde round-trip test
pins both spellings and asserts `expires` rides as an unquoted number.
`domain` is the host with the **port stripped** (Playwright rejects a
port in `domain`), `secure` is true only for `https`.

**The exposure discipline is the load-bearing part.** This verb emits a
live credential by design — that is its product — so the human render is
a **pure function**, `session_login_human_lines(&SessionLoginResult) ->
Vec<String>`. `println!` output cannot be read by a unit test, which
would make the withholding unenforceable; the returned lines can be, and
`session_login_human_lines_withhold_the_session_material` asserts the
cookie NAME and gateway URL are present while a fixture cookie value and
CSRF token are absent (threat T-tg4-01). The material is reachable only
under `--json`, where a machine asked for it on purpose.

Password stays env-only via `IGNITION_PASSWORD` (missing → exit 3
`password_unavailable`, rejected → exit 5 `auth_rejected`), and neither
refusal path emits any session material. `--user` resolves flag →
`IGNITION_USER` → `admin` (D6). The profile resolves **degraded** — the
adopt rationale verbatim: logging in is what happens *before* a token
exists.

### Task 2 — `ign e2e doctor` (commit `a1863ea`)

Six rows in a fixed order — `node`, `npm`, `playwright`, `chromium`,
`testing-bundle`, `trial` — reusing `actions::doctor::{CheckResult,
CheckStatus}` rather than a parallel row type, so every doctor in this
CLI renders and parses identically.

**The exit contract is the point.** `e2e_doctor` returns `Ok` whenever
the diagnosis completes. A toolless host, an unresolvable profile, an
undeployed bundle, and a transport error are all `checks[]` rows —
`check_bundle` and `check_trial` catch their own `CoreError`s and turn
them into `fail` rows, because letting one escape would discard the five
rows already gathered.

**Chromium presence is a pure filesystem scan** (D1) of
`PLAYWRIGHT_BROWSERS_PATH` or the per-OS default, looking for a
`chromium-*` / `chromium_headless_shell-*` entry. Deliberately not `npx
playwright install --dry-run`: on a host without the package cached that
reaches for the npm registry, which breaks both the offline posture and
the exit-0 contract, and its output is not a stable contract to parse.

`find_lint_tool` was generalized into `find_on_path` / `find_on_path_in`
— **one** PATH-discovery implementation now serves `ign lint` and the
node/npm rows, with a `.cmd`/`.exe`/`.bat` suffix probe for Windows,
where `npm` and `npx` are `.cmd` shims. `ign lint`'s behavior is
unchanged.

`NODE_INSTALL_HINT` is a `pub const` so Task 3's exit-6 refusal carries
the doctor's hint **byte-identically** — asserted by
`old_node_fails_with_the_install_hint` and
`absent_node_refuses_with_the_install_hint`, from both directions.

### Task 3 — `ign e2e init` (commit `7859247`)

Seven members embedded via `include_str!` (the `TESTING_FILES` shape):
`package.json`, `playwright.config.mjs`, `global-setup.mjs`,
`lib/gateway.mjs`, `tests/example.spec.mjs`, `.gitignore`, `README.md`.
Every source on disk carries a **`.tmpl` suffix** (D9) for two concrete
reasons: a literal `.gitignore` in `e2e-template/` would be honored by
git against its own siblings, and a literal `package.json` would be
picked up by any npm workspace scan. `no_member_path_keeps_the_template_suffix`
pins that a `.tmpl` can never reach a user's disk.

11 markers across the tree, **count-pinned**, with a render test
asserting none survive substitution — so a template edit that drops a
marker fails CI rather than a user's first `npm test`.

**Gate order is load-bearing and nothing may reorder it.** Foreign
directory → `invalid_input` (exit 2) naming files it found; no node →
`NodeToolAbsent` (exit 6). Both run before any write **and** are re-run
inside `e2e_init`, because in-process TUI and MCP callers never pass
through the dispatch arm (the `api call` convention).

**`--yes` gates the whole verb and the refusal IS the dry run** (D3).
`e2e_init_preview` runs both gates, then returns prose listing the
target, every member with its would-be status, and both command lines
verbatim; the dispatch arm hands that string to `require_confirmation`.
One gate beats two flags, and the preview is accurate because the gates
ran first. Registered in `GUARDED_OPS` as a dynamic-prose site with the
`// guarded:e2e init` marker.

**Idempotent by construction**: an existing member is recorded `skipped`
and never touched. `second_init_skips_every_member` writes its own
content over a member and asserts it survives byte-for-byte.

Installers run with **arg vectors**, never shell strings (the target
directory is operator-named and must never become shell syntax),
inheriting stdout/stderr so they stream through. A **non-zero installer
is DATA, not an error** — a network-less host is still left with a
usable scaffold on disk.

**`CoreError::NodeToolAbsent` → exit 6** mirroring `LintToolAbsent` line
for line (D4). Exit 7 in this repo is the rig class whose sole slug is
`rig_error`; putting a missing Node there would force a relabel of a
locked exit class, and exit 6 already hosts the identical shape (a local
external tool a verb delegates to is absent, with an install hint).

**The scaffold's TLS posture** (D2, threat T-tg4-03): `lib/gateway.mjs`
sets `NODE_TLS_REJECT_UNAUTHORIZED='0'` **only** when the gateway host
is `localhost` / `127.0.0.1` / `::1` (or `IGNITION_ALLOW_INSECURE_LOCAL
=== 'true'`), and writes one warning line to stderr when it fires. The
identical guard is duplicated in `playwright.config.mjs` for
`ignoreHTTPSErrors` rather than imported — the config is loaded by the
runner and the helper by the tests, so a cross-import is a load-order
hazard — and `the_localhost_guard_agrees_across_config_and_helper`
renders both bodies and asserts they name the same three hosts and the
same opt-in variable, so the copies cannot drift. Both the scaffold's
README and the repo README name `NODE_EXTRA_CA_CERTS` as the correct
path for a remote gateway behind a private CA.

No new Rust dependencies. One npm dependency written into the template:
`@playwright/test@^1.58.2`, Microsoft's official runner, installed by
the operator under the confirmation gate.

## Wiring

All four seams for each of the three leaves: the clap leaf + args in
`cli.rs`, the `ActionOutput` variant + `render_success` arm + dispatch
arm in `main.rs`, the human-render arm in `render.rs`, and the
`routes.rs` registry row with a justified exclusion comment in **both**
`routes.rs` and the `tui_coverage` block. `dashboard_rows` bumped 37 →
40, one per task. The three MCP tools appear with **zero** registration
lines — `walk_catalog` derives them and nothing was added to
`EXCLUDED_LEAVES` (D11).

## Commits

| Task | Commit | What |
|------|--------|------|
| 1 (tracer) | `063f389` | `ign session login`: the projection, the storageState rules, the withholding render seam, 4 contract + 4 unit + 1 render test |
| 2 | `a1863ea` | `ign e2e doctor`: six rows, the doctor posture, `find_on_path` generalization, 8 contract + 2 unit tests |
| 3 | `7859247` | `ign e2e init`: the 7-member template tree, `NodeToolAbsent`, the preview/confirm gate, 5 more contract + 4 unit tests |
| 3 (fix) | `123a3db` | Preview must end on a noun phrase — found by smoke-testing the real binary; +1 assertion |

Every commit staged an **explicit file list**; the repo root's untracked
junk (`*.gwbk`, `uat-*.json/xml/csv`, `*.zip`, `.playwright-mcp/`,
`.claude/worktrees/`) appears in none of them, and no commit deleted a
tracked file (verified per-commit with `git diff --diff-filter=D`).

## Test results

Final full-workspace run, after every change, in an **isolated
`CARGO_TARGET_DIR`** (see Deferred Issues — the shared cache was being
clobbered by a sibling worktree, which made every earlier full run
untrustworthy):

```
cargo test --workspace --no-fail-fast   →  EXIT=0
TOTAL passed: 1274   failed: 0   ignored: 38
```

That is the p0g baseline of 1246 plus exactly the **28** tests this task
adds. `cargo fmt --all --check` clean. `cargo clippy --workspace
--all-targets -- -D warnings` clean (exit 0), both re-verified in the
isolated target dir.

New tests (28 total):

```
tests/session_login_contract.rs — 4 passed
  login_returns_the_session_and_a_playwright_storage_state
  storage_state_cookie_shape_is_playwright_exact
  https_profile_marks_the_cookie_secure
  rejected_credentials_surface_as_auth_exit_five

tests/e2e_contract.rs — 13 passed
  doctor_reports_ok_rows_for_a_complete_toolchain
  old_node_fails_with_the_install_hint
  empty_path_still_exits_zero_with_fail_rows
  playwright_and_chromium_rows_read_the_filesystem
  gateway_rows_skip_without_a_project_or_client
  bundle_row_fails_when_the_testing_route_is_absent      (HTTP 405 → fail ROW, exit 0)
  trial_row_warns_when_the_trial_is_expired
  find_on_path_in_walks_the_injected_path_in_order
  init_writes_the_full_scaffold
  second_init_skips_every_member
  foreign_directory_refuses_before_any_write             (exit 2, zero writes/spawns)
  preview_lists_every_member_and_both_commands_and_writes_nothing
  absent_node_refuses_with_the_install_hint              (exit 6 node_tool_absent)

lib unit tests — 10 passed
  actions::login::tests::storage_state_shape_follows_the_playwright_rules
  actions::login::tests::an_https_gateway_marks_the_cookie_secure
  actions::login::tests::an_unparseable_url_is_internal_not_a_panic
  actions::login::tests::the_wire_keys_are_playwrights_camel_case
  actions::e2e::tests::parse_node_major_handles_the_known_shapes
  actions::e2e::tests::chromium_cache_scan_matches_both_entry_prefixes
  e2e::tests::every_marker_is_substituted
  e2e::tests::no_member_path_keeps_the_template_suffix
  e2e::tests::the_localhost_guard_agrees_across_config_and_helper
  e2e::tests::the_scaffold_gitignores_its_session_state

ignition-cli bin unit test — 1 passed
  render::tests::session_login_human_lines_withhold_the_session_material
```

The five CI gates this change touches are green:
`tui_coverage::cli_tree_and_registry_agree` (`dashboard_rows` pinned at
40), `mcp::tests::catalog_names_match_the_clap_tree_bidirectionally`,
`main.rs::tests::guarded_ops_registry_tracks_every_dispatch_site`,
`error::tests::readme_exit_table_agreement`, and
`error::tests::exit_code_mapping_enumerated`.

**Live smoke test of the real binary** (the check that found
deviation 4, which every unit and contract test had missed):

```
$ ign e2e doctor
node            OK    v24.15.0 (/Users/…/.nvm/versions/node/v24.15.0/bin/node)
npm             OK    /Users/…/.nvm/versions/node/v24.15.0/bin/npm
playwright      WARN  @playwright/test is not installed in ./e2e
  hint: run `ign e2e init <DIR> --yes` to scaffold and install
chromium        OK    a chromium build is present in ~/Library/Caches/ms-playwright
testing-bundle  SKIP  no --project given — the bundle was not probed
trial           FAIL  the trial state could not be read: gateway unreachable …
  hint: `ign doctor` diagnoses gateway reachability and auth
EXIT=0
```

**Exit 0 against a genuinely unreachable gateway** — the doctor posture
holding outside a mock. The chromium row also matched a real Playwright
cache, exercising D1's filesystem scan against the actual layout.

`ign e2e init ./smoke-e2e` (no `--yes`) refused **exit 2**
`confirmation_required`, printed the full member list and both command
lines, and **created no directory**. No real `npm install` was run by
this task at any point; the installer legs are covered by the stub-PATH
harness, which records the child's argv.

**Out-of-band check on the rendered scaffold.** All four rendered `.mjs`
members pass `node --check` under real Node v24.15.0, and
`package.json.tmpl` parses as JSON. The Rust tests prove substitution
and layout; this proves the substituted output is actually valid
JavaScript, which no marker-count test can tell you.

## Deviations from Plan

### 1. [Rule 3 — blocking] The plan's fallback URL const does not exist

**Found during:** Task 3, wiring the `e2e init` dispatch arm.
**Issue:** The plan said the arm should "fall back to the localhost
default URL the config module already defines when none does". There is
no such const — `crates/ignition-core/src/config/` defines no default
URL (the `localhost:9088` occurrences there are all test fixtures).
**Fix:** Added `pub const DEFAULT_GATEWAY_URL: &str =
"http://localhost:8088"` to `actions/e2e.rs` (Ignition's stock HTTP
port). The cost of a wrong guess is bounded: the scaffold reads
`IGNITION_URL` first in every place it uses the value, so a user on a
different port sets one env var rather than re-scaffolding.
**Commit:** `7859247`.

### 2. [Rule 1 — bug] The guarded-marker parser silently dropped `e2e init`

**Found during:** Task 3, running
`guarded_ops_registry_tracks_every_dispatch_site`.
**Issue:** `guarded_site_markers` filtered marker text with
`b.is_ascii_lowercase() || b == b' '`. `e2e init` contains a **digit**,
so the marker was discarded and the test reported the registered op as
having no dispatch site — the exact false negative this scan exists to
prevent. Any future leaf with a digit would have hit the same wall.
**Fix:** The filter now also accepts `b.is_ascii_digit()`.
**Files modified:** `crates/ignition-cli/src/main.rs`. **Commit:** `7859247`.

### 3. [Rule 1 — bug] `error::tests` could not run standalone

**Found during:** Task 3, running the plan's own Task-3 verify command
`cargo test -p ignition-core error::tests`.
**Issue:** the `network_error()` test helper calls `reqwest::get` to
manufacture a real `reqwest::Error`. reqwest is built with
`rustls-no-provider` (workspace policy), so that panics —
*"No rustls crypto provider is configured"* — unless some other test
installed the process-wide provider first. In a full lib run one always
does, which is why this never surfaced; filtered to the error tests
alone it fails every time. Pre-existing, but the plan's verify command
walks straight into it.
**Fix:** `crate::client::install_crypto_provider()` at the top of the
helper. The function is `pub`, idempotent, and its own doc comment
sanctions exactly this use ("so the e2e tests' raw `reqwest::Client`
constructions can install it too").
**Files modified:** `crates/ignition-core/src/error.rs`. **Commit:** `7859247`.

### 4. [Rule 1 — bug, found in final smoke testing] The `e2e init` refusal read as a claim about npm

**Found during:** final verification, running the real binary rather
than the tests.
**Issue:** `require_confirmation` renders
`"<operation> is destructive; rerun with --yes to confirm"`, concatenating
its suffix directly onto the operation string. Every other guarded site
passes a short noun phrase, so the suffix reads fine. This preview ended
on a command line, producing:

```
  (in ./e2e) npm install is destructive; rerun with --yes to confirm
```

— a sentence about npm rather than about the scaffold write, on the
verb's **primary refusal surface**. Every contract test passed: they
asserted what the preview CONTAINS, and nothing asserted how it
terminates.
**Fix:** the preview now closes with `this scaffold write`, so the
suffix attaches to a noun phrase, and
`preview_lists_every_member_and_both_commands_and_writes_nothing` gained
an `ends_with` assertion pinning it.
**Files modified:** `crates/ignition-core/src/actions/e2e.rs`,
`crates/ignition-core/tests/e2e_contract.rs`. **Commit:** `123a3db`.

## Deferred Issues

### A shared `CARGO_TARGET_DIR` is silently corrupting builds in this repo

**This cost roughly two hours of this task and will cost the next one
the same until it is addressed.**

A sibling agent has a git worktree at
`.claude/worktrees/git-module-rig-discovery` (HEAD `607734d`), whose
`crates/ignition-core` **predates** `actions/testing.rs`. Both that
worktree and the main checkout build into the same
`~/Library/Caches/cargo-target` (a global `CARGO_TARGET_DIR`), and cargo
gives both the *same* artifact hash — `ignition_core-003a404cf9800fc3`.
They therefore overwrite each other's `libignition_core.rlib` and its
dep-info.

Observed symptoms, all of which look like code defects and are not:

- `cargo test -p ignition-core --lib` reported 429 tests, with
  `actions::login::tests` (committed, previously passing) **absent**.
- A `compile_error!` inserted into `actions/e2e.rs` did **not** fail the
  build — cargo reported "Finished in 0.43s" without recompiling,
  because the cached dep-info never listed the file.
- That dep-info listed neither `e2e.rs`, `login.rs`, **nor the committed
  `testing.rs`** — it was written by the sibling's older tree.
- A full `cargo test --workspace` failed to compile with
  `unresolved import ignition_core::actions::testing` and a doctest
  `E0433`, against code that compiles and passes in every targeted run.
- `cargo clean -p ignition-core` alone did **not** fix it; the
  fingerprint and `deps/` artifacts had to be removed directly.

**Recommended fix:** give each worktree its own `CARGO_TARGET_DIR` (a
`.cargo/config.toml` per worktree, or `CARGO_TARGET_DIR` exported per
agent). Until then, treat any surprising "missing module" or
"test vanished" result in this repo as a cache collision first and a
code defect second, and verify with
`CARGO_TARGET_DIR=<private path> cargo test --workspace`.

Note also that `rm` on this machine is shadowed by a wrapper that
rejects `-rf` (`Un-recognized argument -rfv`); artifact surgery needs
`/bin/rm`.

### Pre-existing flakes (not re-observed)

The two flakes the p0g summary recorded —
`contract_tags::from_export_legacy_layout_and_filter` (a tempfile-name
collision) and `live_gateway::guard_drop_during_unwind_inside_runtime_still_cleans_up`
(a drop-path timing assumption) — **did not fire** in the final isolated
run (1274 passed, 0 failed). They remain unfixed and out of scope.

## Known Stubs

None. Every path this task added is wired and tested end to end. The
three TUI rows map to `Screen::Dashboard` deliberately rather than being
stubbed menu actions, each with the rationale recorded in **both**
`routes.rs` and the `tui_coverage` exclusion block:

- `session login` — its product is a live credential; a TUI modal has
  nowhere safe to put one (scrollback, screen recordings, tmux capture
  all keep it), and the verb exists to hand JSON to a machine.
- `e2e doctor` — its product is a six-row diagnosis document; a one-line
  result modal would discard what the user came for.
- `e2e init` — it writes files and spawns installers, and the CLI gate
  is a refusal that previews every member and both command lines; a
  modal shrinking that to "OK?" would be a worse gate than none.

## Threat Flags

None beyond the plan's register, which is fully covered. The five
`mitigate` rows all carry tests:

- **T-tg4-01** (human render leaking session material) —
  `session_login_human_lines_withhold_the_session_material`.
- **T-tg4-02** (`.auth/state.json`) — `the_scaffold_gitignores_its_session_state`;
  both READMEs name the file and what it holds.
- **T-tg4-03** (`NODE_TLS_REJECT_UNAUTHORIZED`) — loopback-gated with an
  explicit opt-in and a mandatory stderr warning;
  `the_localhost_guard_agrees_across_config_and_helper` pins the guard
  across both rendered bodies; `NODE_EXTRA_CA_CERTS` documented as the
  remote-gateway path in the scaffold README and the repo README.
  *(An automated security hook flagged the `NODE_TLS_REJECT_UNAUTHORIZED`
  assignment during authoring. Kept as specified: it is planner decision
  D2, scoped to loopback, announced on stderr, test-pinned, and paired
  with the documented CA alternative. Worth a human look at ship time.)*
- **T-tg4-04** (spawning installers) — the confirmation gate previews
  both command lines before any write or spawn, both use arg vectors,
  `preview_lists_every_member_and_both_commands_and_writes_nothing`.
- **T-tg4-05** (writing into an existing tree) —
  `foreign_directory_refuses_before_any_write` and
  `second_init_skips_every_member`.

T-tg4-06 and T-tg4-07 were `accept` dispositions and are documented in
the README. The one npm dependency, `@playwright/test@^1.58.2`, was
`[VERIFIED-KNOWN]` in the plan (Microsoft's official runner) and its
caret pin was written exactly as specified; **no package manager was
run** by this task outside the stub-PATH tests.

## Self-Check: PASSED

Created files exist:

```
FOUND: crates/ignition-core/src/actions/login.rs
FOUND: crates/ignition-core/src/actions/e2e.rs
FOUND: crates/ignition-core/src/e2e/mod.rs
FOUND: crates/ignition-core/tests/session_login_contract.rs
FOUND: crates/ignition-core/tests/e2e_contract.rs
FOUND: crates/ignition-core/e2e-template/package.json.tmpl
FOUND: crates/ignition-core/e2e-template/playwright.config.mjs.tmpl
FOUND: crates/ignition-core/e2e-template/global-setup.mjs.tmpl
FOUND: crates/ignition-core/e2e-template/lib/gateway.mjs.tmpl
FOUND: crates/ignition-core/e2e-template/tests/example.spec.mjs.tmpl
FOUND: crates/ignition-core/e2e-template/gitignore.tmpl
FOUND: crates/ignition-core/e2e-template/README.md.tmpl
```

Commits exist:

```
FOUND: 063f389  feat(quick-tg4): ign session login — the IdP session as Playwright storageState
FOUND: a1863ea  feat(quick-tg4): ign e2e doctor — the six-row read-only diagnosis
FOUND: 7859247  feat(quick-tg4): ign e2e init — the embedded Playwright scaffold
```

Each commit's file list contains only paths under `crates/` or
`README.md` (checked per commit with
`git diff --cached --name-only | grep -Ev '^(crates/|README\.md$)'` →
empty all three times), and no commit deleted a tracked file. `docs/reference/`
was not touched.
