---
phase: quick/260919-p0g
plan: 01
subsystem: gateway-testing
tags: [cli-verb, webdev, testing-bundle, exit-codes, mcp, tui-registry]
status: complete

requires:
  - the deployed gateway testing bundle (`ign webdev deploy --with-testing` / `ign adopt --project NAME --testing`)
  - crates/ignition-core/src/client/webdev.rs (testing_route_url)
  - crates/ignition-core/src/error.rs (RoutesNotDeployed, Auth, WebdevUnlicensed, InvalidInput)
provides:
  - "`ign testing run` — the standalone CLI/agent/CI verb over `POST /system/webdev/{project}/testing/run`"
  - "actions::testing::{testing_run, TestingRunOptions, TestingRunResult, TestingFormat, to_junit_xml, to_console}"
  - "client::webdev::{TestingProbe, testing_probe} + ReqwestGatewayApi::testing_post_raw (raw-status transport)"
  - "the testing-scoped `routes_not_deployed` hint (content-addressed off TESTING_ROUTE_PREFIX)"
  - "MCP tool `testing_run` (auto-derived by walk_catalog — zero registration lines)"
affects:
  - crates/ignition-cli/src/main.rs (ActionOutput + the second success-path EXIT exception)
  - crates/ignition-tui/src/routes.rs (registry row; dashboard_rows pin 36 → 37)

tech-stack:
  added: []
  patterns:
    - "raw-status probe before a classified call (the webdev_route_probe precedent) — classify collapses 405/500/501 into Internal (exit 1) and would make an undeployed bundle read as a CLI bug"
    - "content-addressed hint off a stable prefix const (third instance: TUI_TTY_REFUSAL_REASON, LOSS_GATE_REFUSAL_REASON_PREFIX, now TESTING_ROUTE_PREFIX)"
    - "success-path EXIT exception (the `ign lint --strict` seam): render the success envelope, then return ExitCode::from(6)"
    - "clap ValueEnum in cli.rs + From impl to a framework-free core enum (the TransferFormat precedent)"

key-files:
  created:
    - crates/ignition-core/src/actions/testing.rs
    - crates/ignition-core/tests/testing_contract.rs
  modified:
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/src/client/webdev.rs
    - crates/ignition-core/src/actions/mod.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/tui_coverage.rs
    - crates/ignition-tui/src/routes.rs
    - README.md

decisions:
  - "Task 2 resolved option-a by the orchestrator: always POST `format: \"json\"`, render junit/text client-side as pure Rust functions. The route's own junit/text answers stay HTTP 200 and carry no counts, so a passthrough would exit 0 on a red suite."
  - "Red-run exit code is 6 (target_state) through the SUCCESS envelope, not the error envelope: ErrorEnvelope is LOCKED without a `data` field and cannot carry results. `tests_failed` rides as `data.slug` and is deliberately ABSENT from every exit-code table."
  - "404, 405 AND 501 are all absent markers for the testing route (405 is the live-proven 8.3 WebDev marker; 501 is what a route whose methods failed to register answers)."
  - "`--project` is required with no `ign-cli` default — running a suite executes that project's gateway-side code (threat T-p0g-01)."
  - "DEVIATION: docs/reference/ is generated, gitignored output (website/sync-reference.mjs reads README.md). The plan's two doc targets there cannot be committed; README.md is the tracked source and received both edits."

metrics:
  duration: ~95 min
  completed: 2026-09-19

actuals:
  tokens: 46000
  tasks: 2
  commits: 2
---

# Quick Task 260919-p0g: `ign testing run` Summary

`ign testing run --project NAME` now wraps the gateway's Jython testing bundle
as a first-class verb — discover, module/package selection, junit/text reports
rendered client-side, and the pass/fail verdict carried in the exit code (0
green, 6 red with every result field still in the success envelope).

## What shipped

**The transport seam.** `ReqwestGatewayApi::testing_post_raw` sits beside
`webdev_post_raw` and POSTs to the `testing/` URL space **without** calling
`classify`. This is the load-bearing decision of the whole task: `classify`
maps 404 → `NotFound` and collapses 405/500/501 into `Internal` (exit 1, status
discarded), so presence discrimination *cannot* live downstream of it without
making "the testing bundle was never deployed" — a target state the user fixes
with one command — indistinguishable from a CLI defect.

`client::webdev::TestingProbe` (`Present` / `Absent` / `Unlicensed` /
`AuthGated` / `LazyCompile`) is a separate enum from `RouteProbe`: the latter's
variant set is pinned by the webdev tests, and the two route families answer
structurally different shapes (the testing routes have no `{ok, data|error}`
envelope and no `routeVersion` handshake).

**The action.** `actions::testing::testing_run` runs the usage gate first (zero
HTTP), then the presence probe — whose 200 body *is* the discover answer, so
`--discover` costs exactly one round trip — then the run POST. Both legs carry
the single bounded 3 s lazy-compile retry mirrored from `actions::adopt`.

**The verdict rule.** Read from the BODY (`failed + errors > 0`), never the
status line: the route answers 200 on green and **207** on failures, and both
are success transport shapes. Contract-tested at 207 (threat T-p0g-05).

**`--format` (Task 2, option-a).** The CLI always POSTs `format: "json"` and
renders `to_junit_xml` / `to_console` locally as pure functions mirroring the
bundle's `testing/reporter/code.py`. One suite execution, one round trip, and
the verdict + exit code are identical in every format — a passthrough would
have exited 0 on a red suite in junit and text mode, since the route's own
junit/text answers stay HTTP 200 and carry no counts. The rendered document
lands in `data.report`, the structured truth in `data.results`.

**The exit shape.** A red run renders the **success** envelope and then returns
`ExitCode::from(6)` — the second sanctioned success-path EXIT exception, placed
directly beside the `ign lint --strict` one in `main`. `ErrorEnvelope` is LOCKED
to `{ok, profile, error{…}}` with no `data` field, so the error path physically
cannot carry results; and this is how every test runner behaves (full report on
stdout, verdict in the exit code).

**The hint.** `RoutesNotDeployed`'s hint is now content-addressed off
`TESTING_ROUTE_PREFIX`: a `testing/` route names `ign webdev deploy
--with-testing` and `ign adopt --project NAME --testing`, every other route
keeps its previous text byte-identically. No new variant, no new slug, no
exit-code table edits — both branches are pinned by a unit test so a future
edit cannot silently merge them.

**Wiring.** The `testing run` clap leaf, the dispatch arm, the human render arm,
and the TUI registry row (`dashboard_rows` pin bumped 36 → 37 with the exclusion
justified in the comment block, as that assertion's own message sanctions). The
MCP tool `testing_run` appears with **zero** registration lines — `walk_catalog`
derives it from the clap tree and `testing` is not in `EXCLUDED_LEAVES`.

`ign adopt --testing` is untouched: no edits landed in `testing_discover`,
`testing_run` (the client fn), `post_json`, or `adopt.rs`.

## Commits

| Task | Commit | What |
|------|--------|------|
| 1 (tracer) | `5006f88` | End-to-end `--discover`: raw transport, `TestingProbe`, the testing-scoped hint, the action, the clap leaf, dispatch, render, TUI row, 3 contract tests |
| 3 | `d5938dd` | Run leg, `--module`/`--package`/`--format`, the red verdict, 6 more contract tests + 6 unit tests, README docs |

Task 2 was a `checkpoint:decision` resolved by the orchestrator before execution
(option-a) — no execution stop.

## Test results

Final full-workspace run, after every change:

```
cargo test --workspace --no-fail-fast   →  EXIT=0
TOTAL passed: 1246   failed: 0   ignored: 38
```

`cargo fmt --check` clean. `cargo clippy --workspace --all-targets -- -D warnings`
clean.

New tests (15 total):

```
tests/testing_contract.rs — 9 passed
  discover_lists_modules
  absent_route_refuses_routes_not_deployed          (404, 405, 501)
  first_touch_500_is_retried_once
  green_run_exits_zero_with_full_results
  red_run_reports_tests_failed_with_data_intact     (HTTP 207 → Ok, exit 6)
  module_selection_rides_the_body                   (body_json-pinned)
  exclusive_flags_refuse_before_any_request         (exit 2, zero HTTP)
  run_leg_500_is_retried_once
  format_renders_locally_and_keeps_the_red_verdict

lib unit tests — 6 passed
  actions::testing::tests::selectors_are_mutually_exclusive
  actions::testing::tests::run_body_carries_only_what_was_set
  actions::testing::tests::verdict_reads_the_body_not_the_status
  actions::testing::tests::the_verdict_is_identical_in_every_format
  actions::testing::tests::junit_xml_mirrors_the_gateway_reporter
  actions::testing::tests::console_text_mirrors_the_gateway_reporter
  actions::testing::tests::escape_xml_strips_illegal_control_characters
  error::tests::routes_not_deployed_hint_is_testing_scoped
```

The three CI gates a new clap leaf touches are green:
`tui_coverage::cli_tree_and_registry_agree`,
`mcp::tests::catalog_names_match_the_clap_tree_bidirectionally`,
`error::tests::readme_exit_table_agreement`.

## Deviations from Plan

### 1. [Rule 3 — blocking] `docs/reference/` is generated, gitignored output

**Found during:** Task 3, at the staging step.
**Issue:** The plan named `docs/reference/commands.md` and
`docs/reference/gateway-adoption-ign-adopt.md` as doc targets. `git add` refused
both: `.gitignore:12` ignores `/docs/reference/`, and `git ls-files docs/reference/`
returns zero tracked files. The directory is **built from README.md** by
`website/sync-reference.mjs` — README is the single source of truth. Editing the
generated files would have produced an uncommittable change that the next site
build silently overwrote.
**Fix:** Both edits landed in `README.md` instead — the command-table row beside
the `ign script run` row, and an adopt-section pointer saying `--testing` only
smoke-checks the bundle while `ign testing run` is the everyday surface. Ran the
generator to confirm the pages regenerate correctly.
**Side finding:** `docs/reference/gateway-adoption-ign-adopt.md` was **stale
output** from an older README revision — its "A standalone `ign testing run`
verb (the route exists; the CLI wrapper does not)" sentence, which the plan asked
to replace, no longer had any source in README.md. Regenerating retired that page
entirely (the adopt section now lives in `gateway-authentication-83.md`, carrying
the new pointer).
**Files modified:** `README.md`. **Commit:** `d5938dd`.

### 2. [Rule 3 — blocking] `TESTING_RUN_ROUTE` const → `testing_run_route()` fn

**Found during:** Task 1, first clean build.
**Issue:** `use crate::error::TESTING_ROUTE_PREFIX` warned unused — the const was
referenced only from a rustdoc link, which does not count as a use, and
`TESTING_RUN_ROUTE` was an independent `"testing/run"` literal. Two literals for
one concept is exactly the drift the content-addressed hint depends on not
happening.
**Fix:** Replaced the const with `fn testing_run_route() -> String` building the
route from `TESTING_ROUTE_PREFIX`. The raise site and the hint's content address
are now the same source — a `testing/`-less route here is impossible.
**Commit:** `5006f88`.

## Deferred Issues

Two **pre-existing** test flakes surfaced under parallel load during the full
runs. Both are unrelated to this task (they touch no file it changed), and both
pass in isolation and in their own target's full run — but both are real test
bugs worth a follow-up:

1. **`contract_tags::from_export_legacy_layout_and_filter`**
   (`crates/ignition-cli/tests/contract_tags.rs:2120`). The assertion
   `assert!(!stdout.contains("T1"), "non-matching rows drop")` also scans the
   **random tempfile path** printed in the `browsing export <path>` origin
   header. When `tempfile` happens to generate a name containing `T1`, the test
   fails. Fix: assert against the row lines only, not the header.

2. **`live_gateway::guard_drop_during_unwind_inside_runtime_still_cleans_up`**
   (`crates/ignition-core/tests/live_gateway.rs:1069`). The Drop-path cleanup
   find+delete did not reach the mock within its window under contention (19.45 s
   vs 3.01 s in isolation). Likely a timing assumption in the unwind cleanup path.

Neither was fixed — both are outside this task's scope (the executor's
scope-boundary rule) and neither blocks the verb.

## Known Stubs

None. Every code path this task added is wired and contract-tested; the TUI row
maps to `Screen::Dashboard` deliberately (documented in both `routes.rs` and the
`tui_coverage` exclusion block) rather than being a stubbed menu action — the
verb's product is a results document, not a modal round trip, and wiring it into
`ACTIONS` behind a dedicated results pane is a noted clean follow-up.

## Threat Flags

None. The plan's five-row STRIDE register is fully covered: T-p0g-01
(`--project` required, no default), T-p0g-04 (raw-status probe before any run
POST; 404/405/501 all contract-tested) and T-p0g-05 (verdict from the body, 207
contract-tested) are the three `mitigate` rows and all three carry tests.
T-p0g-02 and T-p0g-03 were `accept` dispositions and are documented in the
README Notes cell. No package-manager installs were introduced.

## Self-Check: PASSED

Created files exist:

```
FOUND: crates/ignition-core/src/actions/testing.rs
FOUND: crates/ignition-core/tests/testing_contract.rs
```

Commits exist:

```
FOUND: 5006f88  feat(quick-p0g): ign testing run --discover over the gateway testing bundle
FOUND: d5938dd  feat(quick-p0g): ign testing run full suite — selection, the red verdict, docs
```

Working tree clean of task artifacts — no repo-root junk staged in either
commit (`git diff --cached --name-only | grep -Ev '^(crates/|docs/|README\.md)'`
returned 0 both times), and neither commit deleted a tracked file.
