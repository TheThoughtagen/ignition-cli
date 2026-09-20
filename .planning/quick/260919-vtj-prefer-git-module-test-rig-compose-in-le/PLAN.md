---
phase: quick-260919-vtj
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/ignition-core/src/rig/mod.rs
  - README.md
autonomous: true
requirements: [RIG-01]

estimate:
  tokens: 30000
  raw_tokens: 30000
  tasks: 3
  confidence: low

must_haves:
  truths:
    - "With a git-module checkout that has BOTH `docker/test-rig/docker-compose.yml` and `docker/docker-compose.yml`, `ign rig status` (no flag, no config, no cwd compose) resolves the TEST-RIG file (D-01)."
    - "A checkout that has ONLY `docker/docker-compose.yml` still resolves — the repo-root rig remains a working fallback, never dropped (D-01)."
    - "WHK-Global still resolves only after BOTH git-module relpaths miss under BOTH roots (D-01)."
    - "Both home roots are still probed for every relpath, `~/Documents/whiskeyhouse/` first (D-01)."
    - "`IGNITION_RIG_ROOTS` still overrides the convention roots verbatim; `WHK_HOME_ROOTS` is byte-identical to before (D-01)."
    - "The nothing-found error trail names every probed path, including the new test-rig path (D-03)."
    - "The module doc comment and README discovery table both state the new 3-entry level-5 order and WHY test-rig leads (D-02, D-04)."
  artifacts:
    - "crates/ignition-core/src/rig/mod.rs — `GIT_MODULE_RELPATHS` const array replacing `GIT_MODULE_RELPATH`"
    - "crates/ignition-core/src/rig/mod.rs — new test `git_module_test_rig_beats_the_repo_root_rig`"
    - "README.md — discovery table row 5 + the `[rigs.git-module]` example compose_file"
  key_links:
    - "`GIT_MODULE_RELPATHS` → the level-5 loop at rig/mod.rs:226 (the ONE place probe order is expressed)"
    - "level-5 loop → the `trail` vector → the `CoreError::Rig` nothing-found message (tests assert on the trail)"
    - "rig/mod.rs module doc `## Discovery order (LOCKED)` → README.md discovery table (two-place doc contract; they must agree)"
---

<objective>
Level-5 ("WHK conventions") rig discovery currently resolves the git-module
repo's ROOT compose file, which is the rig that CANNOT load the module: it
mounts the unsigned `Git-unsigned.modl` into a stock `inductiveautomation/
ignition` image with neither `ACCEPT_MODULE_CERTS` nor
`ACCEPT_MODULE_LICENSES` set, so Ignition 8.3 quarantines the module on
boot (it also wants a live `GATEWAY_GIT_USER_SECRET` and commissions
against real WhiskeyHouse GitHub repos). The repo's `docker/test-rig/`
compose is the working one — a BUILT gateway image with developer mode
baked in before the first module scan, both accept-vars set, and a local
`git-server` service.

Make level-5 probe the test rig FIRST, keep the repo-root rig as a
fallback, and keep WHK-Global last. Same two home roots, same
`IGNITION_RIG_ROOTS` override, same search-trail contract.

Purpose: auto-discovery should land on a rig that actually works.
Output: one behavior change in `rig/mod.rs`, its tests, and the two
documentation surfaces (module doc + README) that state the LOCKED order.

Task 1 is the vertical slice — const + loop + doc + the proving test, all
of which together change observable `ign rig` behavior. Tasks 2 and 3
expand out from it (existing-test hygiene, README).
</objective>

<decisions>
Directives from the requester, treated as locked:

- **D-01** — Replace the single `GIT_MODULE_RELPATH` const with an ordered
  const array named `GIT_MODULE_RELPATHS`, test-rig path FIRST, repo-root
  path SECOND; the level-5 loop then iterates test-rig → repo-root →
  WHK-Global, each still probed under BOTH roots, first hit wins. Outer
  loop stays relpath, inner stays root.
- **D-02** — Update the module-level `## Discovery order (LOCKED)` doc
  comment (rig/mod.rs ~lines 14-31) with the new level-5 ordering and a
  SHORT why (stock-image rig cannot load the unsigned module).
- **D-03** — Update the three existing level-5 unit tests so they still
  hold and state the new reality, and ADD a test proving test-rig wins
  when both git-module compose files exist under the same root.
- **D-04** — Update README.md discovery table row 5; judge and state
  whether the `[rigs.git-module]` example at README.md:444 should change
  too. **Judgment: YES, change it** — see Task 3.
</decisions>

<execution_context>
@$HOME/.claude/gsd-core/workflows/execute-plan.md
</execution_context>

<context>
@.planning/PROJECT.md
@.planning/STATE.md
@crates/ignition-core/src/rig/mod.rs
@README.md
</context>

<verified_evidence>
All evidence below was re-verified against the worktree on 2026-09-19 —
line numbers are current, do not re-derive:

- `crates/ignition-core/src/rig/mod.rs:91` — `const GIT_MODULE_RELPATH:
  &str = "ignition-git-module/docker/docker-compose.yml";`
  Line 92 — `WHK_GLOBAL_RELPATH`. Line 88 — `WHK_HOME_ROOTS` (untouched).
- `crates/ignition-core/src/rig/mod.rs:226` — `for relpath in
  [GIT_MODULE_RELPATH, WHK_GLOBAL_RELPATH] {` with the root loop nested
  inside and `trail.push(path.display().to_string())` on each miss.
- `crates/ignition-core/src/rig/mod.rs:237-240` — the nothing-found
  `CoreError::Rig` message that renders `{trail:?}`.
- Module doc `## Discovery order (LOCKED)` spans lines 14-31; the level-5
  bullet is lines 24-27.
- Level-5 tests live at lines 715-821: `git_module_convention_probes_both_
  roots_first_hit_wins` (716), `whk_global_convention_tried_after_git_
  module` (~745), `git_module_beats_whk_global_when_both_exist` (~764),
  `first_root_wins_when_both_roots_have_the_repo` (~792). The trail test
  `no_rig_anywhere_errors_with_search_trail` is at line 690.
- `README.md:435` — discovery table row 5. `README.md:444` — the
  `[rigs.git-module]` example `compose_file`. These are the ONLY two
  README lines naming `ignition-git-module/docker`.
- `crates/ignition-core/src/error.rs:1343` `parse_readme_exit_table` is
  scoped via `readme.split("## Exit codes").nth(1)` — CONFIRMED still
  scoped. Editing lines 435/444 cannot reach it. Its own doc comment even
  warns that unrelated tables have rows starting `| 5 |`.
- `crates/ignition-tui/src/ui/rig.rs:443` — a hardcoded 75+ char path
  literal used only to exercise tail-ellipsis fitting. It is NOT produced
  by discovery, so this change cannot break it. OUT OF SCOPE; leave it.
- `docs/` (installation/quickstart/troubleshooting) contains no discovery
  table and no git-module compose path — no third doc surface to sync.
- `crates/ignition-cli/tests/contract_rig.rs` drives discovery with
  `IGNITION_RIG_ROOTS` pointed at an EMPTY temp dir, so one extra probed
  relpath changes nothing it asserts.
- Both real checkouts (`~/Documents/whiskeyhouse/` and `~/whiskeyhouse/`)
  have `ignition-git-module/docker/test-rig/docker-compose.yml`; the
  ACCEPT_MODULE_CERTS / ACCEPT_MODULE_LICENSES pair is at its lines 50-51,
  and the repo-root file's stock `image:` + unsigned `.modl` mount are at
  its lines 4 and 19.
- Baseline on this worktree: `cargo test -p ignition-core --lib` →
  428 passed / 0 failed (77 `rig::` tests).
</verified_evidence>

<tasks>

<task type="tracer" tdd="true">
  <name>Task 1: Prefer the git-module test rig in level-5 discovery (D-01, D-02)</name>
  <files>crates/ignition-core/src/rig/mod.rs</files>
  <read_first>
    Read `crates/ignition-core/src/rig/mod.rs` lines 1-35 (module doc),
    85-95 (the consts), 215-245 (the level-5 loop + nothing-found error),
    and 710-800 (the level-5 tests, for fixture style: `MINIMAL_COMPOSE`,
    `FakeRunner::with(vec![resolve_output()])`, `discovery_env(...)`).
  </read_first>
  <behavior>
    RED first — add the failing test, run it, confirm it fails for the
    right reason (it resolves the repo-root path today), then fix.

    - Test `git_module_test_rig_beats_the_repo_root_rig`: one temp root
      carrying BOTH `ignition-git-module/docker/test-rig/docker-compose.yml`
      and `ignition-git-module/docker/docker-compose.yml` (both
      `MINIMAL_COMPOSE`), cwd `/empty-cwd`, `RigSelection::Auto`,
      `Config::default()` → `plan.compose_file` equals the TEST-RIG path.
      Assert message should name why (the repo-root rig cannot load the
      unsigned module).
    - Unchanged behaviors the existing suite already pins and that must
      keep passing: both roots probed per relpath, first root wins,
      WHK-Global after git-module, `IGNITION_RIG_ROOTS` override.
  </behavior>
  <action>
    Write the RED test first at the end of the `// ----- level 5: WHK
    conventions` block in `mod tests`, matching the surrounding fixture
    style. Run it and confirm it fails by resolving the repo-root path.

    Then replace the single `GIT_MODULE_RELPATH` const at line 91 with
    `const GIT_MODULE_RELPATHS: &[&str]` holding, in order:
    `"ignition-git-module/docker/test-rig/docker-compose.yml"` then
    `"ignition-git-module/docker/docker-compose.yml"`. Leave
    `WHK_GLOBAL_RELPATH` (line 92) and `WHK_HOME_ROOTS` (line 88) exactly
    as they are — the roots array stays the single source of truth for
    roots, and this array becomes the single source of truth for the
    git-module probe order (per D-01).

    Give the new const a doc comment in the surrounding density: state
    that the git-module repo ships two rigs and that the test rig leads
    because it is the only one whose gateway can LOAD the module (built
    image with developer mode on before the first module scan, plus both
    accept-env vars), while the repo-root entry mounts the unsigned modl
    into a stock image with neither accept var — 8.3 quarantines it — and
    additionally expects a live `GATEWAY_GIT_USER_SECRET` and real remote
    repos. Say WHY the repo-root entry is KEPT: checkouts that predate the
    test rig must still discover something.

    Rewrite the loop header at line 226 to
    `for relpath in GIT_MODULE_RELPATHS.iter().copied().chain(std::iter::once(WHK_GLOBAL_RELPATH))`
    — `.copied()` keeps the iterator item `&str` so the body (including
    `root.join(relpath)` and the `trail.push`) is untouched. Preserve the
    outer=relpath / inner=root nesting exactly: each relpath is probed
    under BOTH roots before the next relpath. Update the two-line comment
    above the loop to name the three-entry order.

    Update the module doc comment level-5 bullet (lines 24-27) per D-02:
    the git-module TEST rig first, then the git-module repo-root rig, then
    WHK-Global, each under both home roots, `~/Documents/whiskeyhouse/`
    first — plus one short clause on why the test rig leads (the stock-image
    rig quarantines the unsigned module). Keep the existing plan-checker
    parenthetical about machine layouts and the `IGNITION_RIG_ROOTS`
    sentence verbatim; they are still true.

    Do NOT add any crate. Do NOT touch `whk_roots()`, `expand_path`,
    `CWD_CANDIDATES`, or the nothing-found error string.
  </action>
  <verify>
    <automated>cd /Users/pmannion/Documents/whiskeyhouse/ignition-cli/.claude/worktrees/git-module-rig-discovery && cargo test -p ignition-core --lib rig::tests::git_module_test_rig_beats_the_repo_root_rig 2>&1 | tail -5</automated>
    <automated>cd /Users/pmannion/Documents/whiskeyhouse/ignition-cli/.claude/worktrees/git-module-rig-discovery && grep -c 'GIT_MODULE_RELPATHS' crates/ignition-core/src/rig/mod.rs</automated>
    <automated>cd /Users/pmannion/Documents/whiskeyhouse/ignition-cli/.claude/worktrees/git-module-rig-discovery && grep -n 'ignition-git-module/docker' crates/ignition-core/src/rig/mod.rs | head -2</automated>
  </verify>
  <done>
    The new test passes (it failed before the const/loop change, for the
    right reason). `GIT_MODULE_RELPATHS` appears at least twice in the file
    (definition + loop). The first `ignition-git-module/docker` occurrence
    in the file is the `test-rig/` path — ordering is visible at a glance.
    `cargo test -p ignition-core --lib rig::` is green. Committed as a
    single commit, e.g. `fix(rig): prefer the git-module test rig in
    level-5 discovery`.
  </done>
</task>

<task type="auto">
  <name>Task 2: Re-point the existing level-5 tests at the new order (D-03)</name>
  <files>crates/ignition-core/src/rig/mod.rs</files>
  <read_first>
    `crates/ignition-core/src/rig/mod.rs` lines 688-825 — the trail test
    and the four level-5 tests.
  </read_first>
  <action>
    These three tests still PASS unchanged after Task 1 (each writes only
    `docker/docker-compose.yml`, which is now simply the second git-module
    probe). The work here is to stop them silently under-specifying the
    order, not to repair breakage:

    - `git_module_convention_probes_both_roots_first_hit_wins` — its
      inline comment currently says "Only root2 has the git-module repo."
      Say which relpath it exercises and note that after Task 1 it proves
      something stronger: the full relpath-by-root grid, since the test-rig
      relpath misses under both roots before the repo-root relpath hits
      under root2. Keep the fixture and the assert as-is.
    - `whk_global_convention_tried_after_git_module` — extend its comment
      to say BOTH git-module relpaths miss before WHK-Global is reached.
      Keep the fixture and the assert as-is.
    - `git_module_beats_whk_global_when_both_exist` — its assert message
      says "git-module outranks WHK-Global (discovery order)". Make it
      explicit that this is the git-module repo-root rig, so the reader
      does not confuse it with the new test-rig precedence test.
    - `no_rig_anywhere_errors_with_search_trail` (line 690) — add ONE
      assertion that the trail names the test-rig probe path, so the
      "every probed path appears in the trail" contract stays pinned. The
      convention branch pushes the full display path, so a substring check
      on the test-rig relpath tail is the right shape; put it next to the
      existing convention-roots assertion with a matching message.

    Comments only plus one new assertion — do not restructure fixtures,
    rename tests, or change any existing assert.
  </action>
  <verify>
    <automated>cd /Users/pmannion/Documents/whiskeyhouse/ignition-cli/.claude/worktrees/git-module-rig-discovery && cargo test -p ignition-core --lib rig::tests:: 2>&1 | tail -5</automated>
  </verify>
  <done>
    All `rig::tests::` tests pass, count is at least 78 (the 77 baseline
    plus Task 1's new test). The three named tests and the trail test read
    correctly against the new three-entry level-5 order. Committed, e.g.
    `test(rig): pin the three-entry level-5 probe order`.
  </done>
</task>

<task type="auto">
  <name>Task 3: Sync the README discovery table and rig example (D-04)</name>
  <files>README.md</files>
  <read_first>
    `README.md` lines 426-450 — the discovery table and the `Config
    surface` TOML block directly beneath it.
  </read_first>
  <action>
    Rewrite row 5 of the discovery table (line 435) to list all three
    convention relpaths in the new order: the git-module
    `docker/test-rig/docker-compose.yml`, then the git-module
    `docker/docker-compose.yml`, then
    `whk-environment-orchestration/docker-compose.yml` — each probed under
    BOTH `~/Documents/whiskeyhouse/` and `~/whiskeyhouse/`, first hit wins.
    Append one short clause saying the test rig leads because it is the
    only git-module rig whose gateway loads the module. Keep the row on a
    single line and keep every existing pipe cell boundary intact.

    Also update the `[rigs.git-module]` example `compose_file` at line 444
    to the test-rig path. **Judgment (D-04): yes, change it.** The example
    exists to show the canonical git-module rig; leaving it aimed at the
    repo-root file would hand a copy-pasting reader exactly the rig this
    change exists to stop auto-selecting, and would contradict the table
    two lines above it. Keep the `~/Documents/whiskeyhouse/` prefix and the
    `# project_name optional` comment unchanged.

    Touch nothing else in README.md. In particular do not edit the
    `## Exit codes` section — `error.rs::readme_exit_table_agreement`
    parses that section verbatim from `include_str!`.
  </action>
  <verify>
    <automated>cd /Users/pmannion/Documents/whiskeyhouse/ignition-cli/.claude/worktrees/git-module-rig-discovery && grep -n 'test-rig/docker-compose.yml' README.md</automated>
    <automated>cd /Users/pmannion/Documents/whiskeyhouse/ignition-cli/.claude/worktrees/git-module-rig-discovery && cargo test -p ignition-core --lib error::tests::readme_exit_table_agreement 2>&1 | tail -3</automated>
    <automated>cd /Users/pmannion/Documents/whiskeyhouse/ignition-cli/.claude/worktrees/git-module-rig-discovery && git diff --stat README.md</automated>
  </verify>
  <done>
    `test-rig/docker-compose.yml` appears on both the table row and the
    example line. `readme_exit_table_agreement` still passes.
    `git diff --stat README.md` shows 2 changed lines and no more.
    Committed, e.g. `docs(readme): level-5 discovery prefers the git-module
    test rig`.
  </done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| home filesystem → `docker compose` execution | Level-5 discovery picks a compose file off disk by convention and `resolve_file` immediately runs `docker compose config` against it; later verbs `up` it. Whoever can write the probed path controls what runs. |
| operator env → discovery | `IGNITION_RIG_ROOTS` redirects the convention scan to arbitrary roots. Unchanged by this task. |

## STRIDE Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation Plan |
|-----------|----------|-----------|----------|-------------|-----------------|
| T-QUICK-01 | Tampering | `GIT_MODULE_RELPATHS` → level-5 loop → `resolve_file` | low | accept | The new relpath sits under the SAME operator-owned home roots and the SAME repo (`ignition-git-module/docker/`) as the relpath it outranks. Anyone able to plant `docker/test-rig/docker-compose.yml` could already have planted `docker/docker-compose.yml`. No boundary widened; no new root, no glob, no recursive walk. |
| T-QUICK-02 | Information disclosure | nothing-found `CoreError::Rig` search trail | low | accept | The trail already prints absolute probed paths under the operator's own home — deliberate, so agents self-diagnose (RIG-01). One more path of the same shape reveals nothing new, and Task 2 keeps it asserted rather than drifting. |
| T-QUICK-03 | Elevation of privilege | the now-preferred test rig disables module signature enforcement (`ACCEPT_MODULE_CERTS` / `ACCEPT_MODULE_LICENSES`) | low | accept | That posture lives in the ignition-git-module repo, not here; `ign` ships no compose file and creates none. The rig is a disposable, network-isolated dev rig with a local `git-server` — never a production path. Preferring it is what makes the module loadable at all. Out of scope to change. |
| T-QUICK-SC | Tampering | npm/pip/cargo installs | n/a | n/a | No package installs in this change — zero new crates (the workspace lean-tree constraint). The Package Legitimacy Gate does not apply. |
</threat_model>

<source_audit>
| Source | Item | Plan coverage |
|--------|------|---------------|
| CONTEXT | D-01 — ordered `GIT_MODULE_RELPATHS`, test-rig first, loop iterates test-rig → repo-root → WHK-Global under both roots | COVERED — Task 1 |
| CONTEXT | D-02 — module doc `## Discovery order (LOCKED)` updated with new order + short why | COVERED — Task 1 |
| CONTEXT | D-03 — three existing level-5 tests still hold and state the new order; new precedence test added | COVERED — Task 1 (new test), Task 2 (existing three + trail) |
| CONTEXT | D-04 — README row 5 updated; judgment given on the `[rigs.*]` example | COVERED — Task 3 (judgment: change it, rationale stated in the action) |
| REQ | RIG-01 — discovery resolves a usable rig | COVERED — Tasks 1-3 |
| CONTEXT | No new crates; edition 2024 / MSRV 1.85 | COVERED — Task 1 action forbids new deps; the change is std-only iterator chaining |
| CONTEXT | `IGNITION_RIG_ROOTS` and `WHK_HOME_ROOTS` unchanged | COVERED — Task 1 action forbids touching `whk_roots()` / `WHK_HOME_ROOTS`; existing tests `whk_roots_const_pins_both_home_roots_in_order` and the contract-test suite pin it |
| CONTEXT | Search trail still names every probed path | COVERED — Task 2 adds the assertion |
| CONTEXT | `ignition-tui/src/ui/rig.rs:443` out of scope | COVERED — verified as a literal test fixture unrelated to discovery; see `<verified_evidence>`; `cargo test --workspace` in `<verification>` proves it |
| CONTEXT | `.planning/` history not rewritten | COVERED — `files_modified` is two source files; nothing under `.planning/` is edited |

No unplanned items.
</source_audit>

<verification>
Required gates, run from the worktree root
(`/Users/pmannion/Documents/whiskeyhouse/ignition-cli/.claude/worktrees/git-module-rig-discovery`):

1. `cargo build --workspace --all-targets`
2. `cargo test -p ignition-core --lib` — must be **>= 429 passing, 0
   failed**. 429 = the 428-passing pre-change baseline on this worktree
   plus Task 1's one new test; Task 2 adds an assertion, not a test.
3. `cargo clippy --workspace --all-targets` — no NEW warnings vs. the
   pre-change baseline. Watch specifically for a `dead_code` warning,
   which would mean the old `GIT_MODULE_RELPATH` const was left behind.

Confirmation beyond the gates (cheap, proves the two out-of-scope claims):

4. `cargo test --workspace` — proves `ignition-tui`'s compose-path fixture
   and `ignition-cli`'s `contract_rig.rs` discovery tests are unaffected.
   If `crates/ignition-tui/src/ui/rig.rs:443` DOES break, stop and report
   — that would contradict the scope call, do not silently edit it.
5. `git diff --stat` — expect exactly two files changed:
   `crates/ignition-core/src/rig/mod.rs` and `README.md`.
</verification>

<success_criteria>
- A git-module checkout with both compose files auto-discovers the
  `docker/test-rig/` one; a checkout with only the repo-root file still
  discovers that one.
- WHK-Global is still reached only after both git-module relpaths miss
  under both roots; `~/Documents/whiskeyhouse/` still wins over
  `~/whiskeyhouse/`.
- `IGNITION_RIG_ROOTS` behavior and the `WHK_HOME_ROOTS` const are
  byte-identical to before.
- The probe order is expressed in exactly one place in code
  (`GIT_MODULE_RELPATHS` + the chained `WHK_GLOBAL_RELPATH`) and is stated
  identically in the module doc and README row 5.
- All three verification gates pass; `cargo test --workspace` is green.
- Three commits, one per task, each independently buildable.
</success_criteria>

<output>
Create `.planning/quick/260919-vtj-prefer-git-module-test-rig-compose-in-le/SUMMARY.md` when done.
</output>
