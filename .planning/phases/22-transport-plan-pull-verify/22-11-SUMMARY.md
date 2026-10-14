---
phase: 22-transport-plan-pull-verify
plan: 11
subsystem: deploy
tags: [rust, clap, mcp, readme, verify-verb, verdict-envelope, exit-6, contract-exception]

requires:
  - phase: 22-transport-plan-pull-verify
    provides: deploy_verify, DeployVerifyResult.failure_exit_code, VerifyOptions, ForbidWrites (22-10); load_declared, transport::for_node (22-01)
provides:
  - "ign deploy verify --env <ENV> [--repo DIR] [--project NAME] (DeployCommand::Verify, ActionOutput::DeployVerify)"
  - "the main.rs exit-6 success-path exception (third sanctioned, after lint --strict and testing run)"
  - "render_deploy_verify_human, Screen::Projects route row, unguarded MCP tool deploy_verify"
  - "README Commands row, ### deploy verify section, dated verdict exception (2026-10-14) pinned by deploy_family_coverage"
  - "CheckId::word and CheckStatus::word (core) for the human renderer"
affects: [22-12, 22-13, 22-14, 23, 24]

actuals:
  tokens: 11600
  tasks: 2
  commits: 5

tech-stack:
  added: []
  patterns:
    - "Verdict verbs: success envelope first, process exit decided after render_ok from a failure_exit_code() on the result"
    - "CLI contract tests that need a gateway: local: tracer copy for the host plus a wiremock gateway whose REST bodies are derived from the live tree's rendered config.json (nulls stripped, CV2 N1)"

key-files:
  created: []
  modified:
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/src/mcp.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-core/src/deploy/verify.rs
    - README.md
    - crates/ignition-cli/tests/contract_deploy.rs
    - crates/ignition-cli/tests/contract_stdout_purity.rs
    - crates/ignition-cli/tests/deploy_family_coverage.rs

key-decisions:
  - "Human mode keeps the CORE-01 [profile: NAME] header for deploy verify (a session IS resolved here, unlike plan and pull); the verdict line is the first line after it"
  - "The README must not contain the literal text '## Exit codes' before the real section: readme_exit_table_agreement splits on it. The dated exception says 'the exit-code table below' instead"
  - "The exception section needle for the DSURF-04 test is the prose words data.ok, deploy_verify_failed, exit 6 and ok: true, so the README states them as words and not only inside the JSON example"

patterns-established:
  - "Same-day dated exception heading format: ### The `<verb>` verdict exception (YYYY-MM-DD), first dated by the commit date of the README change"

requirements-completed: [DPLN-07, DPLN-08, DPLN-09, DSURF-04]

duration: about 60min
completed: 2026-10-14
status: complete
---

# Phase 22 Plan 11: `ign deploy verify` verb, exit-6 verdict and README exception Summary

**`ign deploy verify` resolves the master's profile through the normal secret chain, runs the 22-10 read-only engine and prints the frozen success envelope; a red verdict carries `data.ok: false` / `data.slug: deploy_verify_failed` and exits 6 after the envelope, a green one exits 0, and everything that fails before a verdict is an ordinary error envelope.**

## Accomplishments

- **Verb** (`cli.rs`, `main.rs`). `DeployCommand::Verify { env, repo, project }` (`--project` default `ign-cli`). The dispatch arm runs `load_declared`, builds the reader with `transport::for_node(.., "nodes.master", ..)`, resolves the master's profile with `named_profile_client(&mut config, &master.profile)` and awaits `deploy_verify(&declared, &project, session.api(), session.api(), reader.as_ref(), &VerifyOptions::default())`. It calls no scan-lock, scan trigger or write method, and names none of `TreeMirror`, `mirror_for_node`, `stage` or `commit` (`write_half_unreachable` stays green). The envelope `profile` is the master's profile name.
- **Exit 6** (`main.rs`). After `render_ok`, `if let ActionOutput::DeployVerify(verify) = &out && let Some(code) = verify.failure_exit_code() { return ExitCode::from(code) }`, with a comment naming Decision A and DSURF-04. This is the third success-path exit exception.
- **Human output** (`render.rs`). `verify <gateway> (<env>, mode <mode>): PASS|FAIL`, one `<id>: <status> <detail>` line per check in the fixed order, one indented `<code> <subject> <detail>` line per finding; warnings on stderr. Core gained `CheckId::word` and `CheckStatus::word` (pinned equal to the serialized forms by `words_equal_the_serialized_forms`).
- **Surface.** `Screen::Projects` route row `deploy verify`; MCP tool `deploy_verify` is derived from the clap tree, absent from `GUARDED_OPS`, advertises no `confirm`; README Commands row (after `deploy pull`), `### \`deploy verify\`` section (six checks table, CV1 window, CV2 normalisation, probe semantics, warning rule, what is never printed, output, exit codes, MCP), and the dated section `### The \`deploy verify\` verdict exception (2026-10-14)` under `## Output contract (for agents)`.
- **No exit-table change.** `deploy_verify_failed` is absent from `## Exit codes`, from `CoreError` and from `DEPLOY_SLUGS`; `verify_verdict_exception_is_dated_and_documented` asserts all three.

## Task Commits

1. **Task 1 RED:** `9182dcf` (test: five CLI contract tests, all failing with "unrecognized subcommand 'verify'")
2. **Task 1 GREEN:** `e8bed3c` (feat: verb, dispatch, exit-6 hook, human render, route row, README row and section, `word()` helpers)
3. **Task 2 RED/pins:** `4baeca4` (test: `verify_verdict_exception_is_dated_and_documented` failed RED; `deploy_verify_is_an_unguarded_catalog_tool` and `deploy_verify_json_stdout_is_the_envelope_only` pass at once because the verb already existed)
4. **Task 2 GREEN:** `9d2bfc8` (docs: the dated exception)
5. **Fix:** `fe8d727` (README exit-table parser anchor, clippy needless lifetime in a test helper)

## Tests

| Test | Where |
|---|---|
| `verify_cli_red_exits_6`, `verify_cli_green_exits_0`, `verify_cli_unreachable_gateway_is_an_error_envelope` (exit 4, empty stdout), `verify_cli_unknown_target_profile_is_profile_not_found` (exit 3, added), `verify_cli_human_lines` (log-message canary absent from stdout and stderr) | `tests/contract_deploy.rs` (30 tests, was 25) |
| `verify_verdict_exception_is_dated_and_documented` | `tests/deploy_family_coverage.rs` (5, was 4) |
| `deploy_verify_is_an_unguarded_catalog_tool` | `src/mcp.rs` (49 bin tests) |
| `deploy_verify_json_stdout_is_the_envelope_only` (red verify, `IGNITION_LOG=trace`, exit 6, one value and one newline) | `tests/contract_stdout_purity.rs` (11, was 10) |
| `words_equal_the_serialized_forms` | `ignition-core` lib, `deploy::verify` |

## Gate results (main checkout, private `CARGO_TARGET_DIR`, no worktree)

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo build --workspace` | exit 0 |
| `cargo build -p ignition-cli --no-default-features` | exit 0 |
| `cargo test --workspace` | 2085 passed, 0 failed, 44 ignored (2076 before this plan; +9 = 5 + 1 + 1 + 1 + 1) |

`PLAN_GOLDEN` and `PLAN_GOLDEN_LEDGER` untouched; `deploy_plan` and `deploy_pull` suites unchanged; `write_half_unreachable`, `deploy_family_coverage`, `tui_coverage`, `guarded_ops_registry_tracks_every_dispatch_site`, `catalog_names_match_the_clap_tree_bidirectionally`, `readme_exit_table_agreement` and `exit_code_mapping_enumerated` green.

## Installed-binary smoke

After `cargo install --path crates/ignition-cli --force` (replaced `~/.cargo/bin/ign`), with a scratch repo (tracer copy, `nodes.master.local`) and a profile pointing at a closed port:

```text
ign deploy verify --help             exit 0 (usage, --env, --repo, --project default ign-cli)
ign deploy plan --env dev --repo R --json
                                     ok true, data.plan_sha256 bdea032da5ff0b5ee20a0451b1f86f2e9c55591688911c56148f09891f563e1d
ign deploy verify --env dev --repo R --json     (closed port)
                                     exit 4, error.code network_error, profile "dev", stdout empty
ign deploy verify --env nope --repo R --json
                                     exit 2, error.code deploy_tree_invalid, profile null
```

Scratch files were removed. No live rig was used; the verdict path is proven by the wiremock-backed binary tests and the 22-10 live run.

## README exception date

`2026-10-14`, the commit date of the README change (the plan's flagged assumption for DSURF-04: the requirement fixes neither the placement nor the date source). Placement: under `## Output contract (for agents)`, after the MCP refusal-prose exception, before `## Exit codes`.

## Deviations from Plan

**1. [Rule 1 - Bug in own draft] README exit-table parser anchor.** The first draft of the dated exception said "never a row of the `## Exit codes` table". `readme_exit_table_agreement` takes `README.split("## Exit codes").nth(1)`, so an earlier occurrence made it see no rows (found 0). Reworded to "the exit-code table below" (`fe8d727`). Anyone editing the README must keep the string `## Exit codes` unique before the real heading.

**2. [Judgement] Human header.** The plan's human format reads `verify <gateway> ...`. The `[profile: dev]` CORE-01 header is kept as the first line because a session is resolved (the 22-02 suppression for plan and pull exists only because they resolve none). The tests skip the header line.

**3. [Judgement] Extra test.** `verify_cli_unknown_target_profile_is_profile_not_found` (T-22-47: the target's profile goes through the normal chain; an unknown name is exit 3). Not in the plan's list.

**4. [Judgement] `word()` helpers in core.** `CheckId` and `CheckStatus` serialize snake_case/lowercase; the renderer needs the same words, so two small `pub fn word` methods and one equality test were added in `deploy/verify.rs` (a file outside the plan's `files_modified`), the same move 22-02 made for `PlanStatus::word`.

**5. [Judgement] RED shape of Task 2.** Only the DSURF-04 test could fail RED; the MCP and purity tests pass the moment the verb exists, which Task 1 already delivered. They are still committed as separate pins.

**6. [Environment] Disk full.** The build failed with "No space left on device" (the data volume had 681 MiB free). I removed `~/Library/Caches/cargo-target-ign-cli/debug/incremental` (6.1 GB of rebuildable cache in the private target dir) and ran the rest with `CARGO_INCREMENTAL=0`. Nothing outside the private target dir was touched. The volume is still at about 99 percent; the next executor may hit the same limit.

**Total deviations:** 1 own-draft bug, 5 judgement/environment notes, 0 architectural changes.

## Requirement bookkeeping

- **DPLN-07 complete.** `ign deploy verify` exists and checks scan, mode, REST read-back and the `will not be loaded` log lines against the master (text fully delivered; engine live-proven in 22-10, verb proven at the binary level here).
- **DPLN-08 complete.** The manifest's `verify.tag_probes[]` are read by the verb, a probe not Good fails, an absent list is a warning (`tag_probes_absent`, never a pass). Delivered in 22-10 at the action level and now reachable by users; the plan's own frontmatter lists only DPLN-07/09 and DSURF-04, and no later Phase 22 plan claims it, so it is closed here. Limitation: probes were only exercised through wiremock (the rig has no `ign-cli` bundle, research A8); a caller who wants live probe evidence can reopen it.
- **DPLN-09 complete.** Red verify returns the frozen envelope with findings in `data` and exit 6 (`verify_cli_red_exits_6`, `deploy_verify_json_stdout_is_the_envelope_only`).
- **DSURF-04 complete.** The verdict envelope is documented as a dated README exception and pinned by `verify_verdict_exception_is_dated_and_documented`.

## Known Stubs

None.

## Threat Flags

None beyond the register. T-22-47 (profile from the target via `named_profile_client`, envelope names it), T-22-64 (canary test, trace-level stdout purity), T-22-65 (exit 6 test and README exception) are mitigated; T-22-49 accepted (unguarded read-only MCP tool).

## Self-Check: PASSED

- Files present: `crates/ignition-cli/src/{cli,main,render,mcp}.rs` edits, `crates/ignition-tui/src/routes.rs`, `README.md` (`### \`deploy verify\``: 1, dated exception heading: 1, Commands row: 1), and the three test files.
- Commits `9182dcf`, `e8bed3c`, `4baeca4`, `9d2bfc8`, `fe8d727` are in `git log`.
- `.planning/config.json` and `.planning/research/.cache/` untouched by this plan and not committed.
