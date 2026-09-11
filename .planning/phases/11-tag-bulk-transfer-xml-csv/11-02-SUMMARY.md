---
phase: 11-tag-bulk-transfer-xml-csv
plan: 02
subsystem: webdev-routes
tags: [jython, webdev, base64, wiremock, route-versioning, xml, bulk-transfer]

# Dependency graph
requires:
  - phase: 11-tag-bulk-transfer-xml-csv/11-01
    provides: live-captured wire truth (Probe 1 kwargs+xml return form; Probe 2 importTags file-path signature, basePath forms, QualityCode shape; CRLF/no-declaration byte constants)
  - phase: 05-webdev-routes
    provides: the five-route deploy bundle, three-way version pin test, envelope contract
provides:
  - tagConfig exportTags `format` body param (`json` byte-identical default; `xml` → base64 `payload_b64` + `format` through the JSON envelope)
  - new `importTagsFile` route action (base64 in → gateway temp file → system.tag.importTags → finally-deleted temp → QualityCode strings; collisionPolicy LOCKED to a/o)
  - ROUTE_BUNDLE_VERSION 1.2.0 landed atomically across all five routes + mod.rs + routes/VERSION (MIN_CLI stays 1.0)
  - Wiremock REQUEST pins for the new action bodies + error-envelope mappings at the raw webdev_route_call layer
affects: [11-03, 11-04, 11-05, 11-06]

# Tech tracking
tech-stack:
  added: [] # planner lock honored — Jython-side `import base64` (stdlib); no Rust-side base64 dep
  patterns:
    - "base64 byte-fidelity carrier through the JSON envelope (immune to string-escaping; base64 equality ⇔ byte equality for pins)"
    - "kwargs-primary + documented-positional-fallback (probe-proven primary, live-captured fallback semantics)"
    - "atomic three-way version bump enforced by the existing pin test (one commit = the contract)"

key-files:
  created: []
  modified:
    - crates/ignition-core/webdev/routes/com.inductiveautomation.webdev/resources/cli/tagConfig/doPost.py
    - crates/ignition-core/webdev/routes/com.inductiveautomation.webdev/resources/cli/tags/doPost.py
    - crates/ignition-core/webdev/routes/com.inductiveautomation.webdev/resources/cli/alarms/doPost.py
    - crates/ignition-core/webdev/routes/com.inductiveautomation.webdev/resources/cli/tagHistory/doPost.py
    - crates/ignition-core/webdev/routes/com.inductiveautomation.webdev/resources/cli/scriptExec/doPost.py
    - crates/ignition-core/src/webdev/mod.rs
    - crates/ignition-core/webdev/routes/VERSION
    - crates/ignition-core/tests/tags_contract.rs
    - crates/ignition-cli/tests/contract_webdev.rs # Rule-1 golden fix (Task 2 item 4 mandate)

key-decisions:
  - "exportTags format=xml primary path is the kwargs+xml return form live-proven by 11-01 Probe 1 on both rigs; the documented positional temp-file fallback is retained as defense-in-depth for builds where the kwargs signature differs (capture note: NOT needed on the captured rigs — plan-mandated retention)"
  - "importTagsFile has NO provider-root pre-flight refusal — 11-01 Probe 2 proved importTags FREE of the RpcContext constraint (script-thread truth); the No-RpcContext catch is kept defensively and honestly translated, with WebDev-thread truth deferred to the 11-06 live gate"
  - "XML rides base64 (payload_b64) so the CRLF/no-declaration/trailing-CRLF byte document survives the JSON envelope byte-exactly — the fidelity oracle's transport tier (11-01) depends on it"
  - "Route error codes stay route-level contract strings (unsupported_format, invalid_collision_policy) — no new CoreError slugs; provider_root_unsupported reuses the existing mapped slug"

patterns-established:
  - "Raw-layer wire pins: new route actions are pinned at the webdev_route_call layer BEFORE their action-layer consumers exist (11-04 rides these bodies)"
  - "Base64 fixture pinning without a base64 crate: precomputed constant + injectivity comment"

# Metrics
duration: 25min
completed: 2026-09-11
---

# Phase 11 Plan 02: Route Actions + Atomic Bundle Bump Summary

**tagConfig route gains exportTags `format=xml` (base64 byte-exact passthrough) + `importTagsFile` (temp-file system.tag.importTags, finally-deleted) at an atomically-landed ROUTE_BUNDLE_VERSION 1.2.0, REQUEST-pinned in wiremock**

## Performance

- **Duration:** 25 min
- **Started:** 2026-09-11T16:25:24Z
- **Completed:** 2026-09-11T16:51:21Z
- **Tasks:** 2
- **Files modified:** 9 (7 in the declared list + the Task-2-mandated golden fix)

## Accomplishments

- `exportTags` accepts `format` (`json` default is byte-identical to 1.1.0 — contract-frozen; `xml` returns the gateway's full CRLF document as `payload_b64`; anything else refuses `unsupported_format`)
- New `importTagsFile` action: base64 file bytes → gateway-side temp file → `system.tag.importTags` (the FILE-PATH-only signature, Probe 2) → temp deleted in a `finally` on EVERY path → QualityCode strings; collisionPolicy LOCKED to `'a'`/`'o'` with `invalid_collision_policy` refusal (gateway's `'i'` never surfaced)
- ROUTE_VERSION 1.1.0 → 1.2.0 in all FIVE routes + `ROUTE_BUNDLE_VERSION` + `routes/VERSION` in ONE atomic commit (`27942e2`) — the three-way pin test is the enforcement mechanism; MIN_CLI stays `'1.0'`
- Four new wiremock REQUEST pins at the raw `webdev_route_call` layer: format=xml request body (format+paths), importTagsFile body verbatim, provider-root denial → named slug exit 6, invalid-policy → `webdev_route_error` verbatim contract
- Dynamic-version proof: all 23 pre-existing tags_contract pins pass with ZERO fixture edits post-bump (precondition mocks serve `ROUTE_BUNDLE_VERSION` dynamically)

## Task Commits

Each task was committed atomically:

1. **Task 1: Route actions + atomic 1.2.0 bundle bump** - `27942e2` (feat) — all seven files in ONE commit (the bump's atomicity contract)
2. **Task 2: Wiremock contract pins for the new action bodies** - `34d6594` (test) — includes the Rule-1 golden fix

**Plan metadata:** see final docs commit (SUMMARY + STATE + ROADMAP)

## Files Created/Modified

- `.../cli/tagConfig/doPost.py` - exportTags format param (json/xml + temp-file fallback) + importTagsFile action; header contract comment updated; ROUTE_VERSION 1.2.0
- `.../cli/{tags,alarms,tagHistory,scriptExec}/doPost.py` - constant-only bump to 1.2.0 (route-folder independence honored — no functional changes, no de-duplication)
- `crates/ignition-core/src/webdev/mod.rs` - ROUTE_BUNDLE_VERSION = "1.2.0" (three-way pin leg)
- `crates/ignition-core/webdev/routes/VERSION` - 1.2.0 (three-way pin leg)
- `crates/ignition-core/tests/tags_contract.rs` - 4 new raw-layer pins + CANNED_XML_B64 fixture (Probe-1b byte-shape constants)
- `crates/ignition-cli/tests/contract_webdev.rs` - `webdev_status_all_present_golden` goldens 1.1.0 → 1.2.0 (Rule-1 fix)

## Decisions Made

- **Probe-vs-plan deltas (verification item 5), all resolved toward the captures:**
  - Probe 1 proved the kwargs+xml form works on both rigs (no fallback needed on captured rigs); the plan's temp-file fallback was retained verbatim as a defensive path — noted here per the plan's instruction to record the delta.
  - Probe 2 proved provider-root basePath WORKS for importTags (07-06 RpcContext constraint is getConfiguration/exportTags-specific) — so importTagsFile deliberately has no `is_provider_root` pre-flight; the No-RpcContext translation is kept as an honest defensive catch (script-thread truth only; the 11-06 live gate exercises the true WebDev thread).
  - Collision failures ride `Bad_Failure(...)` QualityCode elements, never exceptions (Probe 2d) — the route returns the list verbatim (`[str(x) for x in results]`, the configure action's existing pattern); inspecting that list for failures is the action layer's job (11-04/11-05).
- **Route-folder independence honored:** only tagConfig got functional changes; the other four routes got the constant + version-comment line only — the shared-core duplication was NOT "fixed."
- **No Rust-side base64 dependency:** fixture pins ride a precomputed base64 constant (injective — string equality ⇔ byte equality), honoring the planner lock.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `webdev_status_all_present_golden` goldens hardcoded 1.1.0**
- **Found during:** Task 2 (no-fixture-blast-radius proof, item 4)
- **Issue:** the binary-level golden test's snapshots pinned `1.1.0` while its mock serves `ROUTE_BUNDLE_VERSION` dynamically — the 1.2.0 bump made the goldens lie (test FAILED)
- **Fix:** bumped both snapshots (human table 4 lines + compact JSON `deployed_version`/`expected_version` ×4 routes) to 1.2.0
- **Files modified:** crates/ignition-cli/tests/contract_webdev.rs (outside the declared file list, but explicitly mandated by Task 2 item 4: "that's a bug to fix in this task")
- **Verification:** `cargo test -p ignition-cli --test contract_webdev` 5/5 green
- **Committed in:** 34d6594 (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug, plan-anticipated and plan-mandated)
**Impact on plan:** The golden drift was the exact scenario Task 2 item 4 predicted. No scope creep.

## Issues Encountered

- **Parallel-wave churn in the shared working tree (11-03 running concurrently in ignition-core):** mid-execution, 11-03's in-flight edits landed `quick_xml` usage in `tag_loss.rs` that temporarily broke lib compilation and made workspace clippy/tests/fmt red in the SHARED tree — exclusively in 11-03's file. Resolution: verification was re-grounded in an isolated `git worktree` at this plan's HEAD (`34d6594`): clippy `-D warnings` **clean workspace-wide**; tests **381 passed, 0 failures outside `actions::tag_loss::tests`** (the 7 tag_loss failures are 11-03's committed TDD RED-phase tests, failing by design per their commit `9049ca9`); fmt diffs confined to `tag_loss.rs` (11-03), `live_gateway.rs` + `ignition-tui/src/ui/mod.rs` (pre-existing Phase-10 drift predating this plan). NONE of this plan's declared files appear in any red state. 11-03's `Cargo.lock` churn (csv deps) was never staged.
- **cargo fmt style-resolution flap:** two consecutive `cargo fmt --check` runs asked for opposite import orders in tags_contract.rs (style-edition resolution changed between runs — concurrent toolchain churn). Resolution: reverted to the original order, which the settled toolchain accepts; net import change = zero. Scoped proof: `cargo fmt --check` flags zero files from this plan's declared list.

## User Setup Required

None - no external service configuration required. (Both live rigs are kept alive per 11-RIG-NOTES.md but this plan was wiremock-driven; rigs untouched.)

## Next Phase Readiness

- The route surface 11-04/11-05 need is live in the bundle: XML out via `payload_b64`, file bytes in via `importTagsFile` — the Rust-side actions ride these exact bodies (pins already enforce the request shapes)
- 11-03's concurrent work (loss-scan) owns `tag_loss.rs`; its RED tests are its own GREEN-phase work — no coordination needed from this plan
- 11-06's live gate inherits: redeploy-at-1.2.0 routes, the No-RpcContext-vs-provider-root open question on importTagsFile (WebDev-thread truth), and the base64 transport tier of the fidelity oracle
- MIN_CLI unchanged means no forced CLI-version floor movement; version-drift refusals remain live-proven machinery (research Pattern 4, zero new checks)

---
*Phase: 11-tag-bulk-transfer-xml-csv*
*Completed: 2026-09-11*

## Self-Check: PASSED

- All declared artifacts exist on disk (7 route/core files + both test files + this summary)
- Both task commits verified in git log: `27942e2` (feat, 7 files, atomic bump), `34d6594` (test, 2 files)
- `importTagsFile` present in the route source (3 hits) and the contract pins (8 hits)
- Three-way pin live-verified: 5× ROUTE_VERSION '1.2.0' + mod.rs "1.2.0" + VERSION 1.2.0; MIN_CLI '1.0' ×5 untouched
