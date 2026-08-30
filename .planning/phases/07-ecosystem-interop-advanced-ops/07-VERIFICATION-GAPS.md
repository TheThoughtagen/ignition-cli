---
phase: 07-ecosystem-interop-advanced-ops
verified: 2026-08-29T18:20:00Z
status: passed
scope: gap-closure addendum (plans 07-05 + 07-06 against 07-UAT.md gaps 1-5)
base_verification: 07-VERIFICATION.md (passed 8/8 — unchanged, not modified)
score: 8/8 gap-closure must-have truths verified
human_verification:
  - test: "Optional live re-probe of the force-409 refusal (gap 4)"
    expected: "ign --profile uat eam task force cli-research-backup --yes -> exit 6 eam_task_in_flight carrying the gateway's 'already exists' page text"
    why_human: "Deliberately NOT re-run by the verifier: if gateway A's '(forced)' slot has since freed, a force probe would DISPATCH a real task (mutation). Evidence stands on the wiremock contract + the executor's captured live envelope (07-06-SUMMARY). A human may re-probe knowing the mutation risk."
---

# Phase 7 Gap-Closure Verification Addendum (07-05 + 07-06)

**Scope:** Verification that the two gap-closure plans closed the five
07-UAT.md gaps, verified goal-backward against the ACTUAL codebase and live
gateway (read-only probes only). The base 07-VERIFICATION.md (passed 8/8) is
untouched; this addendum covers the gap-closure work only.

**Verified:** 2026-08-29T18:20:00Z
**Status:** passed

## Test / Audit Output Summary

| Check | Result | Detail |
| --- | --- | --- |
| `cargo test --workspace` | ✓ GREEN | **863 passed, 0 failed, 26 ignored** (opt-in live gates) across 44 binaries incl. doc-tests — exactly matches the 07-06 SUMMARY claim. (First two verifier runs hit 10/15-min tool timeouts — subprocess-heavy suite + machine load; a full background run completed clean.) |
| `cargo fmt --all --check` | ✓ CLEAN | exit 0 |
| `cargo clippy --all-targets -- -D warnings` | ✓ CLEAN | exit 0 |
| Commits exist | ✓ | 6e8236f, 4bdbb81, a2c6a8b (07-05); 055def2, ab9e909 (07-06) all in git log, diffstats match the plans' files_modified |
| Gateway A state | ✓ PRESERVED | Read-only probes only; the `cli-research-backup (forced)` run still present in live history; no reset/deploy/mutation by the verifier |

## Plan 07-05 Must-Haves (UAT gaps 1, 2, 3)

### Observable Truths

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | `eam history` on live 8.3.3 exits 0, lists `cli-research-backup (forced)` with Failed + GNET detail as DATA | ✓ VERIFIED | **Live read-only probe** (fresh `target/debug/ign`, profile `uat`): exit 0, `2026-08-28T13:48:16.509Z  cli-research-backup (forced)  [Failed]  target=_controller  Attempt 1: Gateway network for agent '_controller' is currently not connected…`. Plus contract `eam_history_decodes_the_raw_capture` (raw-capture-shaped wiremock body, UUID taskId) green. |
| 2 | `eam task new uat-backup-demo eam_backup` creates (exit 0) and `eam tasks` lists it | ✓ VERIFIED | **Live read-only probe**: `ign eam tasks` lists `uat-backup-demo  type=eam_backup  schedule=OnDemand` — the definition exists on gateway A (create live-proven by the executor; not re-created by the verifier to avoid mutation). Composition wiremock-pinned: `task_new_backup_ondemand_fires_unguarded` re-pinned to the `config.profile`/`config.settings` split (body assertions under `config.settings`, profile carries no settings keys) + `task_new_backup_no_target_defaults_to_controller` pins `targetGateways == ["_controller"]`. |
| 3 | Gateway 422 on `/data/api/v1/resources/` surfaces as `invalid_input` (exit 2) carrying the gateway's message, never `internal_error` | ✓ VERIFIED | classify.rs: `S::UNPROCESSABLE_ENTITY if is_config_resource_url(url)` arm (path-scoped to `/data/api/v1/resources/`) reads the body, joins the `messages` array into the reason → `CoreError::InvalidInput`. Contract `task_new_422_classifies_invalid_input` (set_body_raw JSON `{"messages":["Settings cannot be null"],…}`) asserts exit 2, slug `invalid_input`, message carries the gateway text — green in the 863. |
| 4 | `eam task new --help` enumerates the three type classes with full token lists + worked example | ✓ VERIFIED | **Live offline probe**: help shows benign (`eam_backup`), mutating (7 tokens), refused (3 tokens) + fail-safe note + `Example: ign eam task new nightly-backup eam_backup --target gw-a`. Token lists match `REFUSED_TYPES`/`MUTATING_TYPES` in actions/eam.rs EXACTLY (checked line-by-line). |

### Artifacts (all three levels: exists / substantive / wired)

| Artifact | Status | Detail |
| --- | --- | --- |
| `client/eam.rs` — `task_id: String` | ✓ VERIFIED | `pub task_id: String` (L88) with wire-faithful doc; UUID-string fixtures (`a2f4dab1-…`, `b3c5ebc2-…`) in unit tests; wired via the history decode path (live probe exit 0 proves it) |
| `actions/eam.rs` — `config.settings` composition | ✓ VERIFIED | `composed_settings` with `targetGateways`/`targetGroups` + K=V + `--definition` overlay onto settings; unit pins for bare (`["_controller"]`), targeted, K=V, and overlay cases |
| `client/classify.rs` — 422 arm | ✓ VERIFIED | Path-scoped arm + `is_config_resource_url` helper with positive/negative unit tests (EAM create path matches; runtime `/data/eam/` + gateway-info do not) |
| `cli.rs` — TYPE doc | ✓ VERIFIED | Full taxonomy enumeration + example; live help output confirmed |
| `tests/contract_eam.rs` | ✓ VERIFIED | UUID-string fixtures in `mount_history`/force golden; re-pinned JSON goldens (taskId as JSON string); raw-capture test; no-target default test; 422 contract |

### Key Links

| From | To | Via | Status |
| --- | --- | --- | --- |
| `eam_task_create` | `config.settings.targetGateways` | composed create body | ✓ WIRED — `{"targetGateways": ["_controller"], "targetGroups": []}` composition + request-body assertions in contracts |
| classify 422 arm | `CoreError::InvalidInput` | path scope `/data/api/v1/resources/` | ✓ WIRED — arm constructs InvalidInput with joined gateway messages |
| `EamTaskCommand::New` TYPE doc | guard taxonomy | token-list match | ✓ WIRED — doc lists match REFUSED_TYPES/MUTATING_TYPES exactly |

## Plan 07-06 Must-Haves (UAT gaps 4, 5)

### Observable Truths

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | `eam task force` on a task whose '(forced)' run occupies the slot exits 6 `eam_task_in_flight` carrying the gateway's page text, never `internal_error` | ✓ VERIFIED | error.rs `EamTaskInFlight` (exit 6, code `eam_task_in_flight`, Display carries detail verbatim, hint names the EAM console). classify.rs `S::CONFLICT if is_eam_force_url(url)` arm reads the body, sniffs the Jetty page via `html_error_parts`, `'(forced)'` fallback when absent; `eam_force_task_name` extracts the last force-URL segment (unit-tested incl. query-safety). Contract `task_force_conflict_refusal_golden` mounts the captured HTML page (set_body_raw, text/html) and asserts exit 6 + slug + "already exists" + task name — green in the 863. Executor's live capture (07-06-SUMMARY) shows the real envelope. Not re-probed live by the verifier (mutation risk — see Human Verification). |
| 2 | `tags export default` / `tags config get [default]` exit 6 `provider_root_unsupported` naming the subtree workaround | ✓ VERIFIED | **Live read-only probes, both forms**: bare `default` and bracket `[default]` → exit 6, slug `provider_root_unsupported`, message + hint name `[provider]folder` subtree form. Route-side: tagConfig doPost.py pre-call bracket detection + `No RpcContext` traceback translation (both denial sites present); Rust-side: `denial_to_error` arm (webdev.rs L207) → `CoreError::ProviderRootUnsupported`. |
| 3 | Subtree tag paths still export/configure fine after the redeploy — no regression | ✓ VERIFIED | **Live read-only probes**: `tags export [default]uattest` → exit 0, 1 real entry (`uattest AtomicTag`); `tags config get [default]uattest` → exit 0 with real config (dataType Int4, defaultValue 42); `tags export [default]Area01` → exit 0. |
| 4 | Route bundle version-bumped so stale 1.0.0 refuses `route_version_mismatch` until deploy | ✓ VERIFIED | `webdev/routes/VERSION` = 1.1.0; ALL FIVE route doPost.py files carry `ROUTE_VERSION = '1.1.0'`; `ROUTE_BUNDLE_VERSION = "1.1.0"` in webdev/mod.rs with equality-enforcing drift tests (green in the 863). The live provider-root refusals above are served by the 1.1.0 route logic (the denial code exists only in 1.1.0) — the redeploy is live-proven by consequence; the pre-redeploy staleness refusal was captured live by the executor (07-06-SUMMARY). |

### Artifacts

| Artifact | Status | Detail |
| --- | --- | --- |
| `error.rs` — both additive variants | ✓ VERIFIED | `EamTaskInFlight` (beside EamTaskTypeRefused) + `ProviderRootUnsupported` (beside EamNotController); code()/exit_code()/endpoint()/hint() all plumb through; both in `exit_code_mapping_enumerated` |
| `classify.rs` — 409 force arm | ✓ VERIFIED | Path-scoped arm + `is_eam_force_url`/`eam_force_task_name` helpers with unit tests (force path matches; history + config-resource list paths do NOT; name extraction incl. query strings) |
| `tagConfig/doPost.py` | ✓ VERIFIED | `provider_root_unsupported` denials at all four sites (getConfig pre-call, exportTags pre-flight scan naming the offending path, both RpcContext translations); `ROUTE_VERSION = '1.1.0'`; **byte-0 rule intact** (`def doPost` at byte 0, verified via od); outer bare-except unchanged |
| `client/webdev.rs` — denial arm | ✓ VERIFIED | `"provider_root_unsupported" => CoreError::ProviderRootUnsupported` + denial-mapping test extension |

### Key Links

| From | To | Via | Status |
| --- | --- | --- | --- |
| classify 409 arm | `CoreError::EamTaskInFlight` | force-URL match + Jetty page sniff | ✓ WIRED |
| tagConfig route | `CoreError::ProviderRootUnsupported` | 200-body denial → `denial_to_error` | ✓ WIRED (webdev.rs L207; live probes prove end-to-end) |
| `routes/VERSION` ↔ `ROUTE_BUNDLE_VERSION` | equality-enforced 1.1.0 | drift tests | ✓ WIRED (all 7 version copies in lockstep; suite green) |

Route-source pin (wiremock cannot run Python): webdev/mod.rs test asserts the tagConfig doPost source contains `provider_root_unsupported` — present (L248) and green.

## Two-Place Exit-Table Rule

| Slug | error.rs enumerated test | README exit table | Prose |
| --- | --- | --- | --- |
| `eam_task_in_flight` | ✓ (L1054–1063, exit 6) | ✓ (table row L36) | ✓ (L621 force semantics) |
| `provider_root_unsupported` | ✓ (L1040–1044, exit 6) | ✓ (table row L36) | ✓ (L155/L161 tags rows, L993) |

## 07-UAT Gap Cross-Check

| UAT Gap | Status | Closed By |
| --- | --- | --- |
| 1 — eam history decode (UUID taskId) | ✓ CLOSED | Wire-faithful model + raw-capture contract + live probe exit 0 |
| 2 — TYPE help UX | ✓ CLOSED | Taxonomy-enumerated doc + example; live help verified |
| 3 — eam task new 422 (config.settings + classification) | ✓ CLOSED | Composition re-pinned + 422→invalid_input + live definition exists on A |
| 4 — force 409 misclassification | ✓ CLOSED | Additive exit-6 slug + path-scoped arm + contract + executor's live capture |
| 5 — provider-root route_error | ✓ CLOSED | Route 1.1.0 fix + dedicated slug + live refusals both forms + subtree regression clean |

## Anti-Patterns

None found in the gap-closure changes — no TODO/FIXME/placeholder markers, no
stub arms (both classify arms read and surface real body content), no
console-only implementations. Both new slugs are additive; no existing
slug/golden moved except the intended re-pins (guard-ladder contract
`task_new_guard_ladder_refusals_do_zero_work` still green; tui_coverage green
— no CLI command/flag shape changed).

## Informational Notes (not gaps)

1. **Stale installed binary:** `~/.cargo/bin/ign` (built Aug 29 07:22, BEFORE
   the gap-closure commits) still exhibits the ORIGINAL gap-1 symptom on live
   `eam history`. The workspace `target/debug/ign` (Aug 29 17:24) is correct.
   Anyone dogfooding via PATH `ign` should `cargo install --path
   crates/ignition-cli` (or equivalent) to pick up the fixes. Environment
   issue, not a code gap.
2. **Workspace suite duration:** the full suite takes >15 min under load
   (subprocess-heavy contract tests); the verifier's completed background run
   confirms 863/0 green with zero flakiness.

## Gaps Summary

None. All eight gap-closure truths verified against the actual codebase and
(where safely possible) the live gateway; the one deliberately deferred live
re-probe (force 409, mutation-risk) is covered by the wiremock contract, the
code, and the executor's captured live envelope.

---

_Verified: 2026-08-29T18:20:00Z_
_Verifier: Claude (gsd-verifier) — gap-closure addendum_
