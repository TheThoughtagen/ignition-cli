---
phase: quick-261018-nmf
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/ignition-cli/tests/deploy_family_coverage.rs
autonomous: true
requirements: [QUICK-nmf, DAPL-07]

estimate:
  tokens: 12000
  raw_tokens: 12000
  tasks: 1
  confidence: low

must_haves:
  truths:
    - "`rolled_back_verdict_exception_is_dated_and_documented` passes when README.md has CRLF line endings (the windows-latest checkout), not only LF"
    - "The test still fails if the README loses its `### `deploy apply`` heading line: the check matches a whole heading line, not a substring"
    - "README.md, .gitattributes and every other assertion in deploy_family_coverage.rs are unchanged; the leading-newline matches at lines 58, 64, 243, 280, 333 are left alone"
    - "`cargo test -p ignition-cli --test deploy_family_coverage`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check` and `cargo build -p ignition-cli --no-default-features` all pass locally"
  artifacts:
    - path: "crates/ignition-cli/tests/deploy_family_coverage.rs"
      provides: "Line-ending-agnostic `### `deploy apply`` heading check in rolled_back_verdict_exception_is_dated_and_documented"
      contains: "README.lines().any(|line| line == \"### `deploy apply`\")"
  key_links:
    - from: "crates/ignition-cli/tests/deploy_family_coverage.rs:17 (`README` = include_str! of README.md)"
      to: "crates/ignition-cli/tests/deploy_family_coverage.rs:208-211 (the heading assertion)"
      via: "`str::lines()` splits on LF and strips a trailing CR, so the heading line compares equal under LF and CRLF checkouts (same approach as the `subsection` helper at line 301 and `dated_exception_body` at line 224)"
      pattern: "README\\.lines\\(\\)\\.any"
---

<objective>
Fix the one windows-latest failure on draft PR #21 (CI run 39174143079): `rolled_back_verdict_exception_is_dated_and_documented` panics at crates/ignition-cli/tests/deploy_family_coverage.rs:208 with "the README has a `### `deploy apply`` section". Line 209 does a substring search for the heading wrapped in an LF on both sides. Git for Windows checks README.md out with CRLF, so a CR sits between the heading text and the trailing LF and the search misses. ubuntu and macos pass. The assertion came in with 897a8b4 (23-21, the DAPL-07 rolled-back verdict exception docs).

Purpose: the Windows CI leg runs `cargo test --workspace`, which stops at the first failing test binary. Other Windows-only failures may be hiding behind this one, so this one has to go green first.
Output: a test-only edit to crates/ignition-cli/tests/deploy_family_coverage.rs and one commit, `fix(23): match the deploy apply README heading under CRLF`.
</objective>

<execution_context>
@$HOME/.claude/gsd-core/workflows/execute-plan.md
@$HOME/.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.claude/CLAUDE.md
@crates/ignition-cli/tests/deploy_family_coverage.rs

Verified facts (planner read these directly):
- deploy_family_coverage.rs:17 is `const README: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../README.md"));`.
- Lines 206-211 (inside `rolled_back_verdict_exception_is_dated_and_documented`, which starts at 182) are a two-line comment followed by an `assert!` whose condition is `README.contains(...)` on the heading with an LF before and after it, and whose message is "the README has a `### `deploy apply`` section".
- README.md:3027 is exactly the heading line "### `deploy apply`" with nothing after it.
- The file already handles CRLF elsewhere through `lines()`. The `subsection` helper (line 301, doc comment "CRLF-safe") compares `line == heading_line`. `dated_exception_body` (lines 224-225) has the comment "`lines()` drops a CRLF too: git for Windows checks the README out with CRLF line endings."
- The file's other newline matches (lines 58, 64, 243, 280, 333) put an LF only BEFORE the text, so they still match under CRLF. Do not touch them.
- README.md has no uncommitted changes. The working tree has unrelated changes: `.planning/config.json` is modified and `.planning/research/.cache/` is untracked. Neither goes in this commit.
- CLAUDE.md says: no worktree, because this machine sets a global CARGO_TARGET_DIR. Run on the main checkout.
</context>

<tasks>

<task type="auto">
  <name>Task 1: Make the deploy apply heading check match a whole line under LF and CRLF, and prove it on a CRLF copy of the README</name>
  <files>crates/ignition-cli/tests/deploy_family_coverage.rs</files>
  <read_first>
    - crates/ignition-cli/tests/deploy_family_coverage.rs lines 180-212 (the failing test and its heading assertion)
    - crates/ignition-cli/tests/deploy_family_coverage.rs lines 298-321 (the `subsection` helper, the CRLF-safe precedent to match)
  </read_first>
  <precondition>`git diff --quiet -- README.md` exits 0 (README.md is clean, so `git checkout -- README.md` can restore it safely after the CRLF simulation).</precondition>
  <action>
    1. Reproduce the CI failure locally. Check the precondition. Convert README.md to CRLF in place with `perl -pi -e 's/\r?\n/\r\n/' README.md`. Run `cargo test -p ignition-cli --test deploy_family_coverage`. Record which tests fail. The expected result is that only `rolled_back_verdict_exception_is_dated_and_documented` fails, with the "the README has a `### `deploy apply`` section" message. Then run `git checkout -- README.md` to restore it, whatever the test result was. If some other test in this file also fails under CRLF, fix it the same way (compare whole lines from `README.lines()`) and list it in the SUMMARY. Do not look outside this file.
    2. In `rolled_back_verdict_exception_is_dated_and_documented`, change the `assert!` condition at line 209 from the substring search to a whole-line comparison: `README.lines().any(|line| line == "### `deploy apply`")`. Keep the assertion message exactly as it is. `str::lines()` strips the trailing CR, so plain `==` works on both checkouts and matches the `subsection` helper. Do not use `trim_end()`, which would also accept trailing spaces. Add one short clause to the existing comment above the assertion, noting that `lines()` drops a CRLF (Windows checkout), in the same wording style as the comment in `dated_exception_body`.
    3. Leave README.md, .gitattributes, and the leading-newline matches at lines 58, 64, 243, 280 and 333 alone. Do not add a CRLF unit test or a fixture. The step-4 simulation is the proof.
    4. Prove it under both line endings. Run `cargo test -p ignition-cli --test deploy_family_coverage` on the normal LF README and check that it passes. Convert README.md to CRLF again with the same perl command and confirm it now contains CRLF. Run the same test command and check that every test passes. Then run `git checkout -- README.md` unconditionally and confirm `git diff --quiet -- README.md`.
    5. Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo build -p ignition-cli --no-default-features`. If rustfmt wants a different layout for the edited assert, run `cargo fmt --all` and include that in the same file.
    6. Commit only this file: `git add crates/ignition-cli/tests/deploy_family_coverage.rs`, then `git commit -m "fix(23): match the deploy apply README heading under CRLF"`. No Co-Authored-By trailer, no push, no worktree, no branch. Do not stage `.planning/config.json` or `.planning/research/.cache/`.
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && grep -cF 'README.lines().any(|line| line == "### `deploy apply`")' crates/ignition-cli/tests/deploy_family_coverage.rs | grep -qx 1 && cargo test -p ignition-cli --test deploy_family_coverage && git diff --quiet -- README.md && perl -pi -e 's/\r?\n/\r\n/' README.md && perl -ne '$c++ if /\r\n/; END { exit($c ? 0 : 1) }' README.md && { cargo test -p ignition-cli --test deploy_family_coverage; rc=$?; git checkout -- README.md; test $rc -eq 0; } && git diff --quiet -- README.md && cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo build -p ignition-cli --no-default-features</automated>
  </verify>
  <done>
    - Before the edit, the deploy_family_coverage tests fail on a CRLF copy of README.md, and the SUMMARY records which tests failed.
    - The heading assertion in `rolled_back_verdict_exception_is_dated_and_documented` uses `README.lines().any(|line| line == "### `deploy apply`")`, and its message is unchanged.
    - `cargo test -p ignition-cli --test deploy_family_coverage` passes with README.md in LF and again in CRLF.
    - README.md is back to its committed state (`git diff --quiet -- README.md` exits 0).
    - `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo build -p ignition-cli --no-default-features` exit 0.
    - `git show --stat HEAD` shows one commit, `fix(23): match the deploy apply README heading under CRLF`, touching only crates/ignition-cli/tests/deploy_family_coverage.rs, with no Co-Authored-By trailer. Nothing is pushed.
  </done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| README contract to agents | The README's `deploy apply` section and the dated rolled-back verdict exception (DAPL-07) are part of the agent contract. This test pins that they exist. |

## STRIDE Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation Plan |
|-----------|----------|-----------|----------|-------------|-----------------|
| T-nmf-01 | Tampering | deploy_family_coverage.rs heading assertion | low | mitigate | Whole-line `==` comparison, not `trim_end()` or a looser `contains`, so the test still fails if the heading is removed or renamed. The LF and CRLF runs both prove it passes on the real heading. |
| T-nmf-02 | Tampering | README.md during the CRLF simulation | low | mitigate | The precondition requires a clean README.md. Every simulation ends with an unconditional `git checkout -- README.md`, and `git diff --quiet -- README.md` is checked before the commit, which stages only the test file. |
</threat_model>

<verification>
- The deploy_family_coverage test binary passes with README.md in LF and in CRLF.
- The CI gates pass locally: fmt check, workspace clippy with -D warnings, and the headless build.
- The diff touches only crates/ignition-cli/tests/deploy_family_coverage.rs: the heading assertion and its comment.
</verification>

<success_criteria>
- The next PR #21 CI run on windows-latest no longer fails `rolled_back_verdict_exception_is_dated_and_documented` (the CRLF simulation reproduces the failure and the fix locally).
- No README, .gitattributes or other test edits.
- One commit, `fix(23): match the deploy apply README heading under CRLF`, with no co-author, not pushed.
</success_criteria>

<output>
Create `.planning/quick/261018-nmf-make-the-deploy-apply-readme-heading-che/261018-nmf-SUMMARY.md` when done
</output>
