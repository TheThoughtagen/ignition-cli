---
phase: quick-261021-tir
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - Cargo.toml
  - Cargo.lock
autonomous: true
requirements: [QUICK-tir]

estimate:
  tokens: 18000
  raw_tokens: 18000
  tasks: 2
  confidence: low

must_haves:
  truths:
    - "The repo build reports the release-candidate version: `cargo run -q -p ignition-cli -- --version` prints a line containing `1.6.0-rc.1`"
    - "The workspace resolves: `cargo metadata --format-version 1 >/dev/null` exits 0 (a `^1.2.0` path-dependency requirement never matches a pre-release, so the intra-workspace requirements name `1.6.0-rc.1`)"
    - "Cargo.lock changes in exactly three lines: the `version` of ignition-cli, ignition-core and ignition-tui; third-party crates that happen to sit at 1.5.0 (byteorder, http, lazy_static) are untouched"
    - "The version contract tests pass unchanged: `cargo test -p ignition-cli --test cli_chassis` and `cargo test -p ignition-cli --test contract_version` (both assert against `env!(\"CARGO_PKG_VERSION\")`, so no test edits are needed)"
    - "Nothing is published, pushed or tagged; the uncommitted `.planning/config.json` and untracked `.planning/research/.cache/` are left exactly as found"
  artifacts:
    - path: "Cargo.toml"
      provides: "[workspace.package] version and the two intra-workspace dependency requirements, all at 1.6.0-rc.1"
      contains: "version = \"1.6.0-rc.1\""
    - path: "Cargo.lock"
      provides: "ignition-cli / ignition-core / ignition-tui package entries at 1.6.0-rc.1"
      contains: "version = \"1.6.0-rc.1\""
  key_links:
    - from: "Cargo.toml [workspace.package] version (line 8)"
      to: ".github/workflows/release.yml publish-crates job (`VERSION=$(grep -m1 '^version' Cargo.toml | sed ...)`)"
      via: "the release pipeline reads the FIRST `^version` line of the root manifest; it must stay the [workspace.package] line and read 1.6.0-rc.1"
      pattern: "^version = \"1\\.6\\.0-rc\\.1\""
    - from: "Cargo.toml [workspace.dependencies] ignition-core / ignition-tui `version` requirements (lines 14-15)"
      to: "crates/ignition-tui and crates/ignition-cli manifests (`{ workspace = true }`)"
      via: "cargo checks each path dependency's version against the requirement; a pre-release only matches a requirement that names a pre-release"
      pattern: "path = \"crates/ignition-(core|tui)\", version = \"1\\.6\\.0-rc\\.1\""
---

<objective>
Bump the workspace version for the v1.6.0-rc.1 release candidate. The version is single-sourced in the root Cargo.toml `[workspace.package]` (all three crates use `version.workspace = true`), so the change is: that line, the two intra-workspace dependency requirements in `[workspace.dependencies]`, and the three workspace-member entries in Cargo.lock.

Purpose: cut the release-candidate commit on the already-checked-out branch `release-1.6.0-rc.1`. Pushing, tagging and publishing are the user's job and are out of scope.
Output: one commit `chore: release v1.6.0-rc.1` touching only Cargo.toml and Cargo.lock (same shape as the prior release commit 0bebfa9 `chore: release v1.5.0`, which changed Cargo.toml by 1 line and Cargo.lock by 3).
</objective>

<execution_context>
@$HOME/.claude/gsd-core/workflows/execute-plan.md
@$HOME/.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.claude/CLAUDE.md
@Cargo.toml

Verified facts (planner checked these directly, 2026-10-21):
- Cargo.toml line 8: `[workspace.package]` version, currently 1.5.0. Lines 14-15: `ignition-core = { path = "crates/ignition-core", version = "1.2.0" }` and `ignition-tui = { path = "crates/ignition-tui", version = "1.2.0" }`. Line 13 is the comment `# version = required alongside path for crates.io publish (path wins locally)`.
- crates/ignition-cli, ignition-core and ignition-tui Cargo.toml all use `version.workspace = true`; none carries its own version literal.
- Pre-release semver was reproduced in a scratch workspace: with the workspace at 1.6.0-rc.1 and a path dependency requiring "1.2.0", `cargo metadata` exits 101 with "failed to select a version for the requirement `a = \"^1.2.0\"` ... candidate versions found which didn't match: 1.6.0-rc.1 ... if you are looking for the prerelease package it needs to be specified explicitly". Changing the requirement to "1.6.0-rc.1" makes it exit 0. So the requirement change is REQUIRED, not optional. A "1.6.0-rc.1" requirement (caret) also matches the final 1.6.0, so it need not be reverted for the stable release.
- Cargo.lock has six `version = "1.5.0"` entries: ignition-cli (line ~1457), ignition-core (~1485), ignition-tui (~1518), plus third-party byteorder (~400), http (~1240) and lazy_static (~1702). Only the three ignition-* entries may change.
- `git grep '1\.5\.0'` outside `.planning/`, `Cargo.lock` and `website/package-lock.json` (npm deps, unrelated) finds ONLY Cargo.toml line 8. No README install snippet, docs page, RELEASING.md line or test pins 1.5.0. RELEASING.md uses v1.1.0 as a generic example; leave it alone.
- Every runtime/test version consumer uses `env!("CARGO_PKG_VERSION")`: crates/ignition-cli/src/main.rs:643,648; src/mcp.rs:346,1139; crates/ignition-tui/src/update.rs:2586; tests/cli_chassis.rs:38,49,88; tests/contract_version.rs:86. No code parses the CLI's own version with `semver` (the semver parses in error.rs:1543 and client/version.rs:86 are for the WebDev route version and the gateway version), so a pre-release string breaks nothing.
- .github/workflows/release.yml extracts `VERSION=$(grep -m1 '^version' Cargo.toml | sed 's/version = "//;s/"//')` — the [workspace.package] line must remain the first line in Cargo.toml starting with `version`.
- Working tree at plan time: `.planning/config.json` modified (uncommitted) and `.planning/research/.cache/` untracked. Both belong to the user; do not stage, revert, or clean them.
- CLAUDE.md: global CARGO_TARGET_DIR on this machine; build on the main checkout, no worktree. `ign` on PATH is the INSTALLED binary, so check the version through `cargo run`, never through `ign --version`.
</context>

<tasks>

<task type="tracer">
  <name>Task 1: Bump the workspace to 1.6.0-rc.1 end to end (manifest, requirements, lockfile, built binary) and commit</name>
  <files>Cargo.toml, Cargo.lock</files>
  <action>
In the root Cargo.toml make exactly three value edits with the Edit tool:
(a) line 8 under `[workspace.package]`: set `version` to "1.6.0-rc.1";
(b) line 14: set the `version` requirement of `ignition-core` (keep `path = "crates/ignition-core"`) to "1.6.0-rc.1";
(c) line 15: set the `version` requirement of `ignition-tui` (keep `path = "crates/ignition-tui"`) to "1.6.0-rc.1".
Edits (b) and (c) are required because a caret requirement on a stable version never matches a pre-release (reproduced in a scratch workspace: cargo exits 101 without them), and the exact pre-release requirement is what lets the crates.io copies of ignition-tui and ignition-cli resolve the rc ignition-core. Extend the existing line-13 comment by one clause so the next maintainer knows why: a pre-release workspace version must be named exactly in these requirements because a caret requirement on a stable version never matches a pre-release. Do not quote the outgoing version number anywhere in the comment. Change nothing else in Cargo.toml (no dependency bumps, no reordering; the [workspace.package] `version` line must stay the first line of the file that starts with `version`, because release.yml reads it with `grep -m1 '^version'`).
<!-- planner-discipline-allow: 1.5.0 -->

Run `cargo metadata --format-version 1 >/dev/null` from the repo root and confirm exit 0. Then update the lockfile with `cargo update --workspace` (alias `-w`), which re-resolves only the workspace members. Check `git diff --numstat Cargo.lock`: it must read `3	3	Cargo.lock`. If it shows any other change (a third-party bump), restore it with `git checkout -- Cargo.lock` and instead let `cargo build -p ignition-cli` rewrite the lockfile, then re-check the numstat. The third-party byteorder, http and lazy_static entries must stay at their current versions.

Build and check the binary: `cargo build -p ignition-cli`, then `cargo run -q -p ignition-cli -- --version` must print a line containing 1.6.0-rc.1. Use `cargo run`, never the `ign` on PATH (that is the installed binary, per CLAUDE.md). Do not run `cargo publish` in any form (not even `--dry-run`) and do not run `cargo package`.

Commit only the two files: `git add Cargo.toml Cargo.lock`, confirm `git diff --cached --name-only` lists exactly Cargo.toml and Cargo.lock, then `git commit -m "chore: release v1.6.0-rc.1"`. No co-author or other trailer lines. Do not use `git commit -a` / `-am` (that would sweep in the user's modified `.planning/config.json`). Do not tag and do not push.
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && cargo metadata --format-version 1 >/dev/null && test "$(grep -m1 '^version' Cargo.toml)" = 'version = "1.6.0-rc.1"' && test "$(grep -cE 'path = "crates/ignition-(core|tui)", version = "1\.6\.0-rc\.1"' Cargo.toml)" = 2 && test "$(grep -A1 -E '^name = "ignition-(cli|core|tui)"$' Cargo.lock | grep -c '^version = "1.6.0-rc.1"$')" = 3 && test "$(git show --numstat --format= HEAD -- Cargo.lock | cut -f1,2)" = "$(printf '3\t3')" && test "$(git show --name-only --format= HEAD | sort | tr '\n' ' ')" = "Cargo.lock Cargo.toml " && cargo build -p ignition-cli && cargo run -q -p ignition-cli -- --version | grep -F '1.6.0-rc.1'</automated>
  </verify>
  <done>Cargo.toml carries 1.6.0-rc.1 in [workspace.package] and in both intra-workspace requirements; `cargo metadata` exits 0; Cargo.lock differs from the parent commit only in the three ignition-* version lines; `cargo run -p ignition-cli -- --version` prints 1.6.0-rc.1; HEAD is `chore: release v1.6.0-rc.1` containing exactly Cargo.toml and Cargo.lock, no trailers, not tagged, not pushed.</done>
</task>

<task type="auto">
  <name>Task 2: Run the version contract tests and confirm no other pins or stray changes</name>
  <files>crates/ignition-cli/tests/cli_chassis.rs, crates/ignition-cli/tests/contract_version.rs (run only; edited only if a test genuinely breaks on the pre-release string)</files>
  <action>
Verification only; this task is expected to change no files. Run `cargo test -p ignition-cli --test cli_chassis` and `cargo test -p ignition-cli --test contract_version`. Both assert the printed and reported version against `env!("CARGO_PKG_VERSION")`, so they must pass with no test edits. If either fails, debug the cause with superpowers:systematic-debugging before changing anything; a failure here would mean something parses or compares the CLI's own version string, which the planner's read found nothing doing. Fix only code or tests that genuinely break on the pre-release string, and commit any such fix separately as `fix: accept the 1.6.0-rc.1 pre-release version` (explicit `git add` of the touched paths, no trailers).

Then confirm the repo holds no other stale pin: `git grep -n '1\.5\.0' -- ':!.planning' ':!Cargo.lock' ':!website/package-lock.json'` must print nothing (website/package-lock.json entries are npm dependencies such as @colors/colors and statuses, not this project's version). Do not edit historical changelog entries, RELEASING.md's generic v1.1.0 example, or anything under `.planning/`.

Finally confirm the user's working-tree state is intact: outside this quick task's own directory (whose PLAN/SUMMARY files the GSD workflow manages), `git status --short` must still show ` M .planning/config.json` and `?? .planning/research/.cache/` and nothing else, and `git tag --points-at HEAD` must print nothing. Do not push.
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && cargo test -p ignition-cli --test cli_chassis && cargo test -p ignition-cli --test contract_version && test -z "$(git grep -n '1\.5\.0' -- ':!.planning' ':!Cargo.lock' ':!website/package-lock.json')" && test "$(git status --short -- . ':!.planning/quick/261021-tir-bump-workspace-version-to-1-6-0-rc-1' | sort | tr '\n' '|')" = " M .planning/config.json|?? .planning/research/.cache/|" && test -z "$(git tag --points-at HEAD)"</automated>
  </verify>
  <done>cli_chassis and contract_version pass against 1.6.0-rc.1 with no test edits; no stale version pin remains outside .planning/, Cargo.lock and the npm lockfile; the working tree shows only the user's pre-existing `.planning/config.json` modification and untracked `.planning/research/.cache/`; HEAD carries no tag; nothing pushed or published.</done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| local repo → crates.io / GitHub | publishing, tagging or pushing would release the rc; out of scope for this task |
| cargo resolver → Cargo.lock | a lockfile refresh can silently move third-party dependency versions |

## STRIDE Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation Plan |
|-----------|----------|-----------|----------|-------------|-----------------|
| T-tir-01 | Tampering | Cargo.lock refresh | medium | mitigate | `cargo update --workspace` only (never a bare `cargo update`); Task 1 verify requires the committed Cargo.lock diff to be exactly 3 added / 3 removed lines and the three changed entries to be the ignition-* packages |
| T-tir-02 | Elevation of Privilege | accidental release (cargo publish, tag push) | high | mitigate | plan forbids `cargo publish` (including `--dry-run`), `cargo package`, `git tag` and `git push`; Task 2 verify asserts no tag points at HEAD |
| T-tir-03 | Information Disclosure | user's uncommitted `.planning/config.json` swept into the commit | low | mitigate | explicit `git add Cargo.toml Cargo.lock`, cached-name check before commit, `-a`/`-am` forbidden; Task 1 verify asserts HEAD touches exactly those two files and Task 2 verify asserts the working tree still shows the user's two entries |
| T-tir-SC | Tampering | package installs | low | accept | no new dependency and no package-manager install in this plan; the package-legitimacy gate does not apply |
</threat_model>

<verification>
- `cargo metadata --format-version 1 >/dev/null` exits 0.
- `grep -m1 '^version' Cargo.toml` prints `version = "1.6.0-rc.1"` (what release.yml reads).
- `cargo run -q -p ignition-cli -- --version` prints 1.6.0-rc.1.
- `cargo test -p ignition-cli --test cli_chassis` and `--test contract_version` pass.
- `git show --stat HEAD` shows only Cargo.toml and Cargo.lock; Cargo.lock 3 insertions / 3 deletions.
- No tag at HEAD, nothing pushed, nothing published; `.planning/config.json` and `.planning/research/.cache/` untouched.
</verification>

<success_criteria>
The branch `release-1.6.0-rc.1` has one new commit `chore: release v1.6.0-rc.1` that sets the workspace version and both intra-workspace dependency requirements to 1.6.0-rc.1 and refreshes the three ignition-* lockfile entries; the repo build reports 1.6.0-rc.1; the version contract tests pass; nothing else in the repo or the user's working tree changed.
</success_criteria>

<output>
Create `.planning/quick/261021-tir-bump-workspace-version-to-1-6-0-rc-1/261021-tir-SUMMARY.md` when done
</output>
