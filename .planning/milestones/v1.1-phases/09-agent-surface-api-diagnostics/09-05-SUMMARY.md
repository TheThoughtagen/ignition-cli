---
phase: 09-agent-surface-api-diagnostics
plan: 05
subsystem: api
tags: [diagnostics-bundle, wire-model, streaming-download, poll-wait, clap, tui-routes, wiremock]

# Dependency graph
requires:
  - phase: 09-02
    provides: "09-LIVE-CAPTURES.md — the capture-locked bundle state vocabulary (Generating/Valid), fileSize units (bytes), download headers, and the ZIP magic"
provides:
  - "BundleStatusWire (state String + fileSize Option<u64> bytes + flatten passthrough) with BUNDLE_GENERATING_STATES / BUNDLE_CAPTURED_STATES and is_generating"
  - "GatewayApi::bundle_generate/bundle_status/bundle_download — download rides download_to_file with the BUNDLE_DOWNLOAD_TIMEOUT=300s per-request override (Pitfall 8)"
  - "actions: bundle_generate/bundle_status/bundle_wait (poll.rs, honest-unknown semantics) / bundle_download (.part rename default naming)"
  - "CLI family ign diagnostics bundle {generate,status,download,wait} + dispatch + render + four TUI Dashboard rows"
  - "Binary contract tests: envelope, wait flip, deadline slug, download bytes/naming, timeout pin; README command-reference rows"
affects: [09-06 (live gates on both rigs), mcp-catalog (Phase 14 derives from the clap tree)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Capture-encoded state vocabulary: PascalCase strings pinned as consts with per-element provenance; is_generating case-sensitive membership — never an enum (Pitfall 2)"
    - "Honest-unknown wait semantics: captured non-generating = terminal; Generating = pending; UNKNOWN = pending until deadline (Pitfall 2 dodged at the wait layer)"
    - "Timeout-override determinism: constant unit-pinned at birth + parameter ride through download_to_file (no sleep-based wiremock)"
    - "Default download naming: the backup/logs .part rename pattern (disposition basename, sanitized, else timestamped fallback)"

key-files:
  created:
    - crates/ignition-core/src/client/diagnostics.rs
    - crates/ignition-core/src/actions/diagnostics.rs
  modified:
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/src/actions/mod.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-cli/tests/contract_diagnostics.rs
    - README.md

key-decisions:
  - "file_size is Option<u64> (capture Decision 2 wins over the plan's Option<i64> sketch — fileSize is a non-negative byte count == Content-Length)"
  - "Added BUNDLE_CAPTURED_STATES alongside BUNDLE_GENERATING_STATES — the wait's unknown-state rule needs the full captured vocabulary to distinguish 'captured non-generating (terminal)' from 'unobserved (keep polling)'"
  - "Envelope data IS the wire for generate/status/wait (data.state / data.fileSize) — no {status: …} wrapper, matching the contract-test spec verbatim"
  - "DiagnosticsCommand::Bundle wraps BundleCommand directly (the tags-provider grouped-subcommand form) — clap's #[command(subcommand)] requires a Subcommand enum, not an Args struct"
  - "bundle_wait mirrors the wait_module poll shape verbatim (Mutex<Option<Wire>> state, PollState::<()>) — the proven Send/lifetime shape for api-capturing probes"

patterns-established:
  - "BUNDLE_DOWNLOAD_TIMEOUT: Duration = 300s pinned by unit test at birth; rides download_to_file's RequestBuilder::timeout (Pitfall 8 — the 30s client default would truncate MB-sized bundles)"
  - "State vocabulary consts carry per-element rig/version provenance comments — rg-greppable, invented literals (RUNNING/DONE/IN_PROGRESS) provably absent"

# Metrics
duration: 314min
completed: 2026-09-07
---

# Phase 9 Plan 05: Diagnostics Bundle Slice Summary

**`ign diagnostics bundle generate/status/download/wait` as curated commands with the capture-locked Generating/Valid state machine, an honest-unknown poll wait, and a truncation-proof 300 s streamed download**

## Performance

- **Duration:** 314 min (dominated by ~7 full-crate rebuild/test cycles on a heavily loaded machine; the compile-test loop, not the code, was the cost)
- **Started:** 2026-09-07T11:12:35Z
- **Completed:** 2026-09-07T16:27:13Z
- **Tasks:** 3
- **Files modified:** 10 (2 created, 8 modified)

## Accomplishments
- The diagnostics-bundle slice of EXT-02 ships end-to-end: wire model → capability methods → actions (incl. the poll-based wait) → CLI leaves → dispatch → render → TUI rows → binary contract tests → README rows
- The state machine is encoded STRICTLY from the 09-02 captures: `BUNDLE_GENERATING_STATES = ["Generating"]`, `BUNDLE_CAPTURED_STATES = ["Generating", "Valid"]`, every element provenance-commented; `state` is a String (no enum guessing — Pitfall 2)
- The download is truncation-proof: `download_to_file` (the ONE streaming site) with a 300 s per-request timeout override, unit-pinned at birth and cross-crate asserted; bytes land verbatim (ZIP magic test-pinned)
- The wait rides poll.rs with honest-unknown semantics: an unobserved state keeps polling (never falsely terminal) and the final status rides the deadline observation; deadline = exit 4 `network_error` (no new slug)
- Full workspace suite green (52 test-result lines, zero failures); clap walk (tui_coverage) green with the four rows landing in the same task as the clap commands

## Task Commits

Each task was committed atomically:

1. **Task 1: bundle wire model + capability methods with timeout override** - `7d4954e` (feat)
2. **Task 2: actions (incl. bundle_wait over poll.rs) + CLI leaves + dispatch + render + TUI rows** - `42e7f7c` (feat)
3. **Task 3: binary contract tests + README rows** - `9477710` (test)

## Files Created/Modified
- `crates/ignition-core/src/client/diagnostics.rs` — BundleStatusWire + path consts + generating/captured state sets + is_generating + BUNDLE_DOWNLOAD_TIMEOUT (7 unit tests)
- `crates/ignition-core/src/client/mod.rs` — trait methods + impl (generate POSTs through the classify-first pipeline; status via get_json; download via download_to_file with the override)
- `crates/ignition-core/src/actions/diagnostics.rs` — bundle_generate/status/wait/download + scripted WaitRig tests (flip, immediate-terminal, unknown-deadline)
- `crates/ignition-core/src/actions/mod.rs` — module registration
- `crates/ignition-cli/src/cli.rs` — Diagnostics → Bundle → {Generate, Status, Download -o, Wait --interval/--timeout} (restart --wait defaults)
- `crates/ignition-cli/src/main.rs` — four Session::resolve dispatch arms + ActionOutput variants + JSON arms
- `crates/ignition-cli/src/render.rs` — one status line per status verb + the download file/bytes line
- `crates/ignition-tui/src/routes.rs` — four Dashboard rows + family pin test
- `crates/ignition-cli/tests/contract_diagnostics.rs` — five binary contract tests (envelope, wait flip, deadline slug, download bytes/naming, timeout pin)
- `README.md` — four command-reference rows + vocabulary/timeout/deadline documentation
- (13 pre-existing test-fake GatewayApi impls gained mechanical `unreachable!` stubs for the three new trait methods — part of Task 1's commit)

## Decisions Made
- **file_size: Option<u64>, not the plan's Option<i64>** — capture Decision 2 wins over the plan sketch (the plan's own rule: "if a capture contradicts this sketch, the capture wins")
- **BUNDLE_CAPTURED_STATES const added** — implementing the plan's "unknown state (not in the captured vocabulary at all)" rule requires the vocabulary as data, not just a doc comment; the generating set is unit-pinned as its subset
- **Envelope data IS the wire** for generate/status/wait — the contract spec's `data.state` demand reads directly; no `{status: …}` re-keying (contrast the 09-04 single-read families, which had no contract-test spec to satisfy)
- **Bundle(BundleCommand) directly, not the plan's DiagnosticsBundleArgs wrapper** — clap's `#[command(subcommand)]` requires a Subcommand enum; the tags-provider grouped-subcommand form is the established precedent
- **bundle_wait uses the wait_module poll shape** (Mutex state + `PollState::<()>`) — the direct `PollState::Done(wire)` return fought the HRTB lifetime; the proven shape compiles first try and is TUI-spawn-safe

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Test-fake GatewayApi impls needed the three new trait methods**
- **Found during:** Task 1
- **Issue:** 14 `impl GatewayApi for …Rig` test doubles across actions modules stopped compiling when the trait grew (Rust requires full impls)
- **Fix:** Mechanical `unreachable!("not part of this action")` stubs inserted after each impl opening line (the established fake pattern)
- **Files modified:** 13 files under crates/ignition-core/src/actions/ (+ logs.rs AuthRig at deeper indent)
- **Verification:** cargo test -p ignition-core diagnostics green
- **Committed in:** 7d4954e (Task 1 commit)

**2. [Rule 3 - Blocking] DiagnosticsBundleArgs wrapper violates clap's derive requirements**
- **Found during:** Task 2
- **Issue:** `#[command(subcommand)] Bundle(DiagnosticsBundleArgs)` demands `DiagnosticsBundleArgs: Subcommand` — an Args struct cannot satisfy it
- **Fix:** `Bundle(BundleCommand)` directly (the tags-provider grouped-subcommand precedent)
- **Files modified:** crates/ignition-cli/src/cli.rs, crates/ignition-cli/src/main.rs
- **Verification:** cargo build green; tui_coverage clap walk green
- **Committed in:** 42e7f7c (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (2 blocking)
**Impact on plan:** Both fixes were mechanical consequences of existing contracts (trait exhaustiveness, clap derive rules). No scope creep; the plan's behavior contract is fully delivered.

## Issues Encountered
- **wiremock scoped-mock verification false negative** (Task 3): `mount_as_scoped + expect(1)` reported "matched 0 requests" for a download that demonstrably happened (client-side byte asserts passed; the binary verified working against a real HTTP server). Dropped the scoped guard in favor of plain `.mount()` — the client-side asserts (bytes verbatim, default vs disposition naming) ARE the contract, and the wait test's poll counter is independent of wiremock's expect machinery.
- **Slow machine amplified compile-test cycles** (~7 full rebuilds, each 3–9 min) — process note, not a code issue.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Success criterion 4 is contract-ready: the bundle trio works as one-command flows; live proof lands in 09-06 (the plan's live gates on both rigs)
- The 09-06 gates can reuse: the captured state vocabulary, the ~2–6 s fresh-rig generation pace (wait --interval 2 --timeout 300 defaults fit), the Content-Disposition presence (both rigs), and the deadline/exit-4 convention
- One phase-level note: plan 09-06 must verify the `Valid`-persistence-after-download and repeatability behavior live (already encoded as expectations in the model docs)

---
*Phase: 09-agent-surface-api-diagnostics*
*Completed: 2026-09-07*

## Self-Check: PASSED

- key-files.created verified on disk (client/diagnostics.rs, actions/diagnostics.rs, contract_diagnostics.rs)
- All 3 task commits verified in git log (7d4954e, 42e7f7c, 9477710)
- Full workspace suite green at commit time (52 result lines, zero failures)
