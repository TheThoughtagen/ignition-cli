---
phase: 04-rig-lifecycle-trial-state
verified: 2026-08-24T03:54:18Z
status: passed
score: 18/18 must-haves verified
re_verification:
  previous_status: human_needed
  previous_score: 17/18
  trigger: "Delta re-verification after autonomous live-gate execution (addendum below, commit bf51760)"
  gaps_closed:
    - "Truth 13 — 8.3.6 trial-reset e2e: tier-1 gate run live on a fresh 8.3.6 rig (expired:true→false with the gate's required read-back; corroborated 0s/expired → 7187s/active); with 04-03's 8.3.3 flip, criterion 3's ≥2-minor-version requirement is MET via the spike-chosen tier-1 mechanism"
    - "Truth 17 live leg — snapshot→mutate→restore round-trip executed two-sided via real CLI verbs on an 8.3.3 rig (pre-witness project survived restore, post-snapshot marker absent, 20.4s restore witnessed RUNNING, token-reset warning emitted)"
    - "Optional lifecycle smoke — status/up/logs/reset/down from a bare cwd via IGNITION_RIG_ROOTS (reset removed ign-gate2-833_gateway_data; fresh trial 7172s proves clean slate)"
  gaps_remaining: []
  regressions: []
cross_phase_findings:
  - id: PROJ-05-resource-routes
    owner_phase: 3
    finding: "The phase-03 resource client family (crates/ignition-core/src/client/resources.rs, the ign resource CLI arm, and the e2e_rig/e2e_projects witness approach) targets /data/api/v1/projects/{name}/resources/** routes that DO NOT EXIST on real 8.3 gateways. Openapi-evidenced: the committed 10.7 MB extract contains ZERO such paths; the projects family exposes only /projects, copy, export/{name}, find/{name}, import/{name}, list, names, parents*, rename/{name}. Per-project resource access exists only via projects/export|import."
    phase4_impact: "NONE counted against Phase 4 — Phase 4's truths never depended on resource routes (snapshot manifest = gwbk + project exports); the shipped e2e_rig gate cannot pass until this is resolved, but the two-sided outcome it was designed to prove has been demonstrated by equivalent real-CLI-verb evidence (see addendum Gate 2)."
    routing: "Phase 5 planning must decide: re-point the resource family to export/import or drop it; then re-point the e2e witness approach."
  - id: gate-hygiene-nits
    finding: "openapi-capture gate's path.contains('/resources') matcher false-positives on the config-resources family (/data/api/v1/resources/{module}/{type} — real routes); extract filename says 8.3.6 but captured_from records the 8.3.3 rig (port 19188). Both disclosed by the executor; both verified here against the artifact."
  - id: tier0-on-836-curiosity
    finding: "The tier-0 token-probe on 8.3.6 remains formally unsettled (unrunnable after the tier-1 reset due to the live-verified trial-not-expired state gate; would need another ~2h expiry cycle). NOT a phase criterion — tier-1 is the spike-chosen mechanism per 04-RESEARCH.md and is now live-proven on both versions. The headless token recipe is solved, so a future run is one expiry-wait away."
---

# Phase 4: Rig Lifecycle & Trial State Verification Report

**Phase Goal:** A user can run a complete Docker test rig from the CLI — up/down/status/reset with compose discovery, logs, trial state management, and snapshot/restore — giving the project (and CI) a self-managed gateway fixture.
**Verified:** 2026-08-24T03:54:18Z (delta re-verification after automated live-gate execution)
**Status:** passed — 18/18 truths verified; all three previously-deferred live items executed with evidence (see "Delta Re-Verification" section and the addendum below)
**Re-verification:** Yes — prior status `human_needed` (17/18); all deferred items executed autonomously via docker compose (commit bf51760) and spot-checked here

## Verification Method

Goal-backward verification against the phase's 4 success criteria and the 18 must-have truths across 04-01..04-04 PLAN frontmatter. gsd-tools could not parse these plans' frontmatter (tool limitation), so artifact/key-link verification was performed manually: line counts, content reads of every action, grep-based link checks, binary-test inspection, and full `cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings` runs. SUMMARY claims were treated as leads, not evidence; every code-level claim below was re-checked in the source.

## Workspace Health (executed as part of this verification)

| Check | Result |
| ----- | ------ |
| `cargo test --workspace` | ✅ 30 suites, **371 passed, 0 failed** |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ clean |
| `poll.rs` touched in phase range (661dce5^..ab76020) | ✅ untouched (diff empty) — LOCKED retry set preserved |
| `Cargo.toml` in phase range | ✅ +1/−1 line only (tokio `process` feature) — no new crates |
| All 14 summary commit hashes | ✅ present in git log (661dce5…ab76020) |

## Goal Achievement

### Observable Truths

**Criterion 1 — rig up/down/status/reset from compose discovery, port pre-flight, wait-for-commissioned**

| # | Truth (04-01) | Status | Evidence |
| - | ----- | ------ | -------- |
| 1 | `rig up` brings rig up (`-d --wait`), reports RUNNING or uncommissioned-as-data exit 0 | ✓ VERIFIED | `rig_up` (actions/rig.rs:181): version gate → preflight → `up_args` → `commissioned_wait` (poll.rs verbatim, `GatewayNotCommissioned`→Pending translation); degradation fires ONLY on `Network{source:None}` deadline + terminal-uncommissioned flag; unit tests pin both paths with fake runner/gateway. Live-verified per 04-01 summary. |
| 2 | `rig down` stops the rig's compose project | ✓ VERIFIED | `rig_down` (actions/rig.rs:289) with `down_args(volumes=false)`; explicit `-p`. |
| 3 | `rig status` allowlist — services/state/health/ports/volumes, no compose-config secrets | ✓ VERIFIED | `rig_status` (actions/rig.rs:921): ps LDJSON → explicit `StatusService`/`StatusPublisher` structs; no config passthrough anywhere; exit 0 when down. |
| 4 | 5-level discovery in LOCKED order | ✓ VERIFIED | rig/mod.rs `resolve_plan`: Named→`[rigs.*]`, `[rig].default` (loud stale error), cwd candidates, git-module both roots, WHK-Global both roots; `WHK_HOME_ROOTS` probed in order; search-trail errors. Binary tests: `missing_rig_exits_7_with_search_trail`, `discovery_precedence_config_default_beats_cwd`, `ignition_rig_env_folds_into_selection`. |
| 5 | `.env COMPOSE_PROJECT_NAME` honored; explicit `-p` on every op | ✓ VERIFIED | All builders (`up_args`/`down_args`/`ps_args`/`logs_args`) lead with `-p <plan.name>`; resolve-then-act ends in one `config --format json` run whose `.name` is authoritative; `--project-directory` always passed. 04-01 recorded live resolution (`ignition-devops` from the rig's own .env). |
| 6 | Port collision pre-up reported with attribution, exit 7 | ✓ VERIFIED | `port_preflight` (rig/mod.rs): `docker ps --filter publish=` (container + compose-project attribution) then advisory `lsof`; cross-project conflict → `CoreError::Rig` → exit 7. Unit-tested with own-occupant fixtures (deterministic). |

**Criterion 2 — clean reset (orphans + volumes, explicit project names, no stale state)**

| # | Truth (04-02) | Status | Evidence |
| --- | ----- | ------ | -------- |
| 7 | `rig reset` (guarded) tears down volumes+orphans and brings rig back up — no stale state | ✓ VERIFIED | `rig_reset` (actions/rig.rs:319): preview → version gate → `down_args(plan, true)` (`-v --remove-orphans`, request shape asserted on the fake-runner call log) → preflight → `up_args` → shared `commissioned_wait`. Binary pin: `rig_reset_refuses_without_yes_before_any_discovery` (exit 2, zero discovery) + `--yes` fallthrough to exit-7 discovery. Live-verified per 04-02 summary (`removed_volumes:["ignition-devops_gateway_data"]`). |
| 8 | reset output previews removed volume names | ✓ VERIFIED | `reset_preview` (compose.rs:510) — project-label-filtered volume names, carried in `removed_volumes` result data. |
| 9 | `rig logs` passthrough with `--tail`/`-f`, no envelope on follow | ✓ VERIFIED | `rig_logs` (actions/rig.rs:620): one-shot via `run` (line-split to sink), follow via `run_streaming` (piped stdout, concurrent stderr drain); compose stderr → tracing, never the sink; `render_ok` intercepts in every mode. Binary help-surface test present. Live-smoked per 04-02 summary (human + `--json` modes). |
| 10 | reset re-runs port pre-flight before the up half | ✓ VERIFIED | Explicit ordering in `rig_reset` step 4 with the torn-down-state hint message; unit-tested (mid-cycle re-grab → Rig error with attribution). |

**Criterion 3 — logs, trial status, trial reset verified against ≥2 gateway minor versions**

| # | Truth (04-03) | Status | Evidence |
| --- | ----- | ------ | -------- |
| 11 | `rig trial status` shows licenseMode/trialState/seconds/expired — no credential required | ✓ VERIFIED | `trial_status` (actions/rig.rs:446): trial endpoint primary, conditional-auth client (headers ride only when present); wiremock header-absence proof in trial_contract.rs; `TrialStatusResult` carries all unit-explicit keys. Live on BOTH rigs per summary. |
| 12 | `rig trial reset` (guarded) flips expired:true→false via spike-chosen mechanism | ✓ VERIFIED | `trial_reset` (actions/rig.rs:537): expiry pre-check (`TrialNotExpired` exit 6 for the live-discovered 403 state gate) → tier-0 token POST → tier-1 native OIDC (idp.rs) → REQUIRED read-back flip (`finish` refuses a non-flipped read-back). Guard binary-pinned (`rig_trial_reset_refuses_without_yes_before_any_discovery`). **Live flip on 8.3.3 recorded in summary (expired:true→false, 7199s)**; wiremock pins assert the full request chain (token threading, cookie replay, CSRF header). |
| 13 | Reset mechanism verified e2e on 8.3.3 AND 8.3.6 | ✓ VERIFIED | **8.3.3: live flip** (04-03 summary: full tier-1 ladder, expired:true→false, 7199s). **8.3.6: live flip** (addendum Gate 1: fresh rig, trial expired naturally to 0s/expired:true → `trial_reset_tier1_live ... ok` with the gate's REQUIRED read-back flip → corroborated by post-test curl `{"trialSecondsLeft":7187,"expired":false}`). Spike-chosen tier-1 mechanism proven on both minor versions — criterion 3's ≥2-version requirement MET. (Tier-0-on-8.3.6 probe remains a non-criterion curiosity — see cross_phase_findings.) |
| 14 | Banners cross-check rides the same command output | ✓ VERIFIED | `TrialBanners` block in `TrialStatusResult`; Pitfall-7 derivation (`severity=="info" && expireTime>now_ms`); failed fetch degrades to nulls + data warning. Fixtures from BOTH versions' live captures. |

**Criterion 4 — snapshot/restore to repeatable state**

| # | Truth (04-04) | Status | Evidence |
| --- | ----- | ------ | -------- |
| 15 | `rig snapshot` writes streamed .gwbk + project exports + manifest.json into timestamped dir | ✓ VERIFIED | `rig_snapshot` (actions/rig.rs:767): std-only `yyyyMMdd-HHmmss` stamp (civil_from_days, no chrono), gwbk FIRST via `backup_download`, per-project exports with injective `encode_segment` names, manifest asserted exactly in tests with BOTH exclusion notes verbatim. |
| 16 | `rig restore --file` (guarded) POSTs, waits RUNNING, warns tokens may be reset | ✓ VERIFIED | `rig_restore` (actions/rig.rs:873): `is_file()`+non-empty pre-checks (exit 2) → octet-stream POST → witnessed RUNNING via shared `commissioned_wait` with `restore_deadline` max-clamped at 300s → `RESTORE_TOKEN_WARNING` inserted FIRST in data. Guard binary-pinned (`rig_restore_refuses_without_yes_before_any_discovery` + `--yes` fallthrough). |
| 17 | Round-trip e2e gate two-sided (snapshot→mutate→restore→prior state back) | ✓ VERIFIED (live round-trip executed) | Gate `e2e_rig.rs::snapshot_mutate_restore_round_trip` asserts BOTH sides (lines 339/343: post-marker absent, pre-witness present). **Live round-trip EXECUTED two-sided via the real CLI verbs** (addendum Gate 2, 8.3.3 rig): `project new` pre-witness → `rig snapshot` (gwbk + 4-project manifest) → `project new` marker → `rig restore --yes` (20.4s, witnessed RUNNING, token-reset warning) → post-restore list: pre-witness SURVIVED, marker GONE. The literal shipped gate binary is blocked at its `resource get` step (line 314) by the **Phase 3** resource-family defect (nonexistent routes — see cross_phase_findings PROJ-05-resource-routes); Phase 4's own paths never touch resource routes, so this is NOT counted against this phase. |
| 18 | gwbk download streams to disk (never Vec\<u8\>) | ✓ VERIFIED | `download_to_file` (client/mod.rs:458): classify-first, then `bytes_stream()` chunk → `write_all` with `u64` counter — no buffering. `backup_download` rides it (mod.rs:1063) with `Accept: application/octet-stream`; read-back byte-identity pinned. (Upload-direction buffering is documented-by-design for Content-Length; the truth concerns the download.) |

**Score:** 18/18 truths verified. No partials, no failures.

### Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | --------- | ------ | ------- |
| `crates/ignition-core/src/rig/mod.rs` | RigPlan, 5-level discovery, port pre-flight (≥120 lines) | ✓ VERIFIED | 935 lines; all levels + `WHK_HOME_ROOTS`/`IGNITION_RIG_ROOTS` + preflight present; wired from actions + main dispatch |
| `crates/ignition-core/src/rig/compose.rs` | Runner seam, version check, arg builders, LDJSON/array parsers, exit mapping (≥150) | ✓ VERIFIED | 1066 lines; `run`/`run_docker`/`run_streaming`, `compose_version` (≥v2), all builders exact-pinned, both parser conventions, stderr-tail exit mapping |
| `crates/ignition-core/src/actions/rig.rs` | up/down/status (+reset/logs/trial/snapshot/restore) serde-out actions (≥150/≥100) | ✓ VERIFIED | 2762 lines; all 9 actions substantive (read in full), serde models with all-keys-always |
| `crates/ignition-cli/tests/contract_rig.rs` | guard/error-path binary tests (≥60) | ✓ VERIFIED | 534 lines; 16 tests incl. all 3 destructive-guard zero-work pins + precedence + env fold + help surfaces |
| `crates/ignition-core/src/client/trial.rs` | TrialWire/BannerSet wire-faithful models (exports `TrialState, BannerSet`) | ✓ VERIFIED | 204 lines; `TrialWire` implements the plan's TrialState role (two-layer naming documented — action layer re-exposes unit-explicit keys); `BannerSet`/`Banner`/`BannerData` per plan |
| `crates/ignition-core/src/client/idp.rs` | Native OIDC login+CSRF flow (tier 1) | ✓ VERIFIED | 541 lines; 10-step flow, token rotation, manual cookie replay, `GatewaySession{csrf_token}`, `trial_reset_via_session` with `X-CSRF-Token` header (idp.rs:505) |
| `crates/ignition-core/tests/trial_contract.rs` | Wiremock pins + login request-sequence proof | ✓ VERIFIED | 691 lines; header-less proof, tier-0 shape, full ladder chain pins (token threading/cookies/CSRF on REQUESTS), bad-creds + HTML-400 paths, live `#[ignore]` gates |
| `crates/ignition-core/src/client/backup.rs` | backup_download (streaming, 300s) + backup_restore (octet POST, 300s) | ✓ VERIFIED | 106 lines; path consts + 300s class + 4-explicit-falses query builder; impl bodies in the single mod.rs impl block (E0119 rule) — the plan's own file-ownership convention |
| `crates/ignition-core/src/actions/rig.rs` (snapshot/restore) | rig_snapshot + rig_restore | ✓ VERIFIED | See truths 15–16 |
| `crates/ignition-core/tests/backup_contract.rs` | Wiremock pins: octet download/restore, params, content types | ✓ VERIFIED | 149 lines; byte-identical read-back, request-pinned octet POST, 401-HTML classify |
| `crates/ignition-cli/tests/e2e_rig.rs` | Two-sided round-trip gate | ✓ VERIFIED | 396 lines; see truth 17 |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | -- | --- | ------ | ------- |
| main.rs | actions::rig::{up,down,status} | `Commands::Rig(RigArgs` dispatch | ✓ WIRED | main.rs:930; docker-only — no profile/secret/client resolution; `profile: null` on success AND error |
| actions/rig.rs | rig/compose.rs | `ComposeRunner::run` | ✓ WIRED | 18 matches; actions never spawn processes directly |
| actions/rig.rs | poll.rs | commissioned-wait via poll engine | ✓ WIRED | 5 matches; `commissioned_wait` shared by up/reset/restore; poll.rs diff-empty |
| main.rs | require_confirmation | reset guard BEFORE resolution | ✓ WIRED | `guarded_operation` match (main.rs:936-948) covers rig reset/restore/trial-reset and fires BEFORE `resolve_plan` (line 964) — semantic satisfaction of the per-verb pattern with a STRONGER binary pin (exit-2-not-exit-7 tests for all three) |
| actions/rig.rs | rig/compose.rs | `down_args(volumes=true)` + up_args through seam | ✓ WIRED | 10 matches; teardown request shape asserted on the call log |
| actions/rig.rs | client/trial.rs | trial status merges trial + banners | ✓ WIRED | 44 matches; trial primary + banners cross-check |
| client/idp.rs | POST /data/api/v1/trial | session cookie + X-CSRF-Token | ✓ WIRED | idp.rs:495-505 — header + cookie on the reset POST; ladder chain request-pinned in trial_contract.rs |
| client/backup.rs (impl in mod.rs) | download_to_file | gwbk streaming | ✓ WIRED | mod.rs:1063 `self.download_to_file(...)`; single body-consumption site preserved (optional `Accept` param) |
| actions/rig.rs | poll.rs | post-restore RUNNING wait ≥300s | ✓ WIRED | `rig_restore` step 3 via shared `commissioned_wait`; `restore_deadline` max-clamp at 300s (unit-pinned) |
| main.rs | require_confirmation | restore guard before resolution | ✓ WIRED | Same `guarded_operation` mechanism; binary-pinned |

### Requirements Coverage

| Requirement | Status | Blocking Issue |
| ----------- | ------ | -------------- |
| RIG-01 — up/down/status/reset lifecycle, clean reset | ✓ SATISFIED | None (live-verified per 04-01/04-02 summaries; code+tests re-verified here) |
| RIG-02 — logs passthrough + trial status | ✓ SATISFIED | None |
| RIG-03 — trial reset via spike-chosen mechanism, ≥2 minor versions | ✓ SATISFIED | None — tier-1 live-proven on 8.3.3 (04-03) AND 8.3.6 (addendum Gate 1) |
| RIG-04 — snapshot/restore repeatable state | ✓ SATISFIED | None — two-sided round-trip executed via real CLI verbs (addendum Gate 2); bonus live observations: trial clock rides the restore; token inside snapshot survives restore (Pitfall 5 not observed) |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| — | — | None: zero TODO/FIXME/PLACEHOLDER/unimplemented!/todo! across all 10 phase artifacts; no empty-handler or console-only implementations | — | — |

Notable positive discipline observed: status allowlist (no compose-config passthrough — secrets like `GATEWAY_ADMIN_PASSWORD` cannot leak), password never a CLI flag (grep-verified in help-surface test), password confined to one JSON body construction site (wiremock-pinned), uncommissioned/state-gate degradations carry agent-visible data rather than masking errors.

### Human Verification Required

None remaining. All three items from the initial verification were executed autonomously via docker compose (per the user's confirmation) and evidenced in the addendum below: (1) 8.3.6 trial-reset tier-1 gate — PASS, criterion 3 closed; (2) snapshot→mutate→restore round-trip — two-sided PASS via real CLI verbs (the shipped e2e gate binary itself is blocked by the Phase 3 resource-route defect — cross-phase finding, not a Phase 4 criterion); (3) fresh-shell lifecycle smoke — PASS. 04-USER-SETUP.md is marked superseded; the headless token-provisioning recipe (collection:"core" + security-properties permissions patch) makes future live gates fully automatable.

### Gaps Summary

No code gaps. Every artifact exists, is substantive (935–2762 lines for the core modules), and is wired end-to-end; all 10 key links verified; all 14 phase commits present plus the gates commit (bf51760); workspace 371/371 green with clippy `-D warnings` clean; LOCKED invariants held (poll.rs untouched, envelope/taxonomy additive-only, single streaming site, no new dependencies; `crates/` diff across bf51760 verified empty here).

The two previously-deferred live legs and the optional smoke have all been executed with verbatim evidence (see addendum) and spot-checked against real artifacts (see Delta Re-Verification section). Criterion 3's ≥2-minor-version requirement is met by the spike-chosen tier-1 mechanism on 8.3.3 + 8.3.6. Criterion 4's round-trip is two-sided-proven at project granularity — the granularity Phase 4's snapshot manifest actually uses.

**Cross-phase item (explicit, NOT counted against Phase 4):** the Phase 3 resource client family targets `/data/api/v1/projects/{name}/resources/**` routes absent from real 8.3 gateways — openapi-evidenced by the committed extract (0 matching paths; only `projects/export|import` exist). This blocks the shipped `e2e_rig` gate's resource step and the `ign resource` arm generally. Routed to Phase 5 planning (re-point to export/import, or drop the family, then re-point the e2e witness approach). Details in frontmatter `cross_phase_findings`.

---

_Verified: 2026-08-23T03:40:48Z_
_Verifier: Claude (gsd-verifier)_

---

## Live Gates — Automated Execution (addendum)

**Executed:** 2026-08-24 (01:47–03:55 UTC), autonomously via docker compose, per the user's confirmation that these items are automatable. No product code was modified (git diff on `crates/` is empty). All work ran against disposable rigs created for this purpose.

### Rigs used (all disposable, all removed afterwards)

| Rig | Image | Port | Project/Name | Fate |
| --- | ----- | ---- | ------------ | ---- |
| Rig A | `inductiveautomation/ignition:8.3.6` | 18188 | `ign-gate1-836` (docker run) | trial left to expire naturally, gates run, container removed |
| Rig B | `inductiveautomation/ignition:8.3.3` | 19188 (+19143 probe) | `ign-gate2-833` (own compose, `IGNITION_RIG_ROOTS=/tmp/ign-gate-rigs` redirect) | gates + lifecycle smoke run, `down -v --remove-orphans` |

Untouched throughout: cask-agents, t3code, whk-mes, whk-services stacks; ign-research (its admin password is genuinely unrecoverable — no env, no history — Path B fresh-rig was used instead); the 9088 ssh tunnel.

### Gate 1 — 8.3.6 trial reset (criterion 3, truth 13): **CLOSED**

Fresh 8.3.6 rig commissioned at 01:47:01Z (`GATEWAY_ADMIN_PASSWORD=password`), trial expired naturally on schedule:

```
$ curl http://localhost:18188/data/api/v1/trial   (03:47:48Z)
{"trialSecondsLeft":0,"expired":true}

$ IGNITION_LIVE_URL=http://localhost:18188 IGNITION_LIVE_USER=admin IGNITION_LIVE_PASSWORD=password \
  IGNITION_LIVE_MUTATIONS=1 cargo test -p ignition-core --test trial_contract trial_reset_tier1_live -- --ignored --exact --nocapture
running 1 test
test trial_reset_tier1_live ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 8 filtered out; finished in 1.17s

$ curl http://localhost:18188/data/api/v1/trial   (post-test corroboration)
{"trialSecondsLeft":7187,"expired":false}
```

The tier-1 live gate asserts the full OIDC login dance + reset POST + **required read-back flip** (`expired:true → false`). With the 04-03 live flip on 8.3.3, the spike-chosen mechanism is now verified e2e on **both** minor versions — criterion 3's ≥2-version requirement is met.

**Tier-0 probe (bonus, formally still open):** skipped by design after the tier-1 reset ("trial not expired (7186s left) — the state gate makes the probe meaningless"). Answering it on 8.3.6 would need another 2 h expiry cycle; out of the timebox. The token it needs was provisioned and verified on the rig (below), so a future run is one expiry-wait away.

### Gate 2 — snapshot→mutate→restore round-trip (criterion 4, truth 17's live leg): **two-sided PASS via the real CLI verbs; the shipped e2e gate itself is BLOCKED by a real product finding**

The shipped gate (`cargo test -p ignition-cli --test e2e_rig -- --ignored`) fails at its first `resource put` — and the failure is a genuine wire-truth discovery, not a harness defect:

```
resource put (pre-witness) failed (exit Some(6)):
{"code":"not_found","message":"resource not found on the gateway",
 "endpoint":"http://localhost:19188/data/api/v1/projects/ign%2De2e%2Drig%2D1787538817%2Dpre/resources/ignition/script%2Dpython/e2e/snapshot"}
```

The gateway's own `/openapi.json` (11.6 MB, 575 paths — fetched authed) contains **zero** `/data/api/v1/projects/{name}/resources/*` routes: the phase-03 `resource` client family (03-03, flagged MEDIUM-CONFIDENCE from ignition-mcp) targets invented endpoints. Phase 03's own openapi-capture gate was run and settled its Open Question 1 negatively:

```
$ IGNITION_LIVE_URL=... IGNITION_LIVE_TOKEN=... cargo test -p ignition-cli --test e2e_projects openapi_capture_writes_phase3_extract -- --ignored
project-resources family present in openapi.json: true (FALSE settles 03-RESEARCH Open Question 1 ...)
wrote .../.planning/phases/03-project-operations/openapi-8.3.6-phase3-extract.json (401 phase-3 paths)
test openapi_capture_writes_phase3_extract ... ok
```

(Two honesty notes on that run: the `true` verdict is a false positive — the gate's matcher is `path.contains("/resources")`, which matches the config-resources family; the extract itself contains no project-resource paths, which is the real, negative answer. And the extract's filename says "8.3.6" but it was captured from the 8.3.3 rig — the file's `captured_from` field records the truth. Both are gate-hygiene nits, not product code.)

Since projects CRUD is real, the round-trip leg was executed at project granularity using the **real CLI verbs** (built `ign` binary, discovery via `IGNITION_RIG_ROOTS` resolving `ign-gate2-833` — verified: `rig status` reported the rig + 19188):

```
1. ign project new gate2-pre-1787539012            -> {"ok":true,"name":"gate2-pre-1787539012"}
2. ign rig snapshot -o /tmp/gate2-run/snap         -> {"ok":true}; manifest {gwbk, 4 projects, 2 notes}
3. ign project new gate2-post-1787539040           -> {"ok":true} (marker confirmed present pre-restore)
4. ign rig restore --file .../ign-gate2-833.gwbk --yes --compact
   -> {"ok":true,"state":"running","warnings":["API tokens may have been reset by restore — ..."]} (20.4s, witnessed RUNNING)
5. post-restore projects/list (raw API):
   ["gate2-pre-1787538935","gate2-pre-1787539012","ign-e2e-rig-1787538817-pre","write-probe"]
   SIDE A PASS: pre-witness gate2-pre-1787539012 SURVIVED the restore
   SIDE B PASS: post-snapshot marker gate2-post-1787539040 is GONE after the restore
6. ign doctor --compact -> ok:true (url/liveness/commissioned/auth/rig ok)
```

Two bonus live observations for the 04-03/04-04 open points: **the trial clock rides the restore** (post-restore trial showed the snapshot-time remaining, 4557 s — restore resets the clock to the gwbk's captured value), and **Pitfall 5 was NOT observed** when the token definition + patched permissions are inside the snapshot (the token kept working across the restore).

### Optional item 3 — fresh-shell rig lifecycle smoke: **PASS**

From `/tmp` with `IGNITION_RIG_ROOTS` pointing at the rig dir (discovery level 5 redirect — the documented agent affordance):

```
ign rig status --compact  -> {"ok":true,"rig":"ign-gate2-833","state":"running"}
ign rig up --compact      -> {"ok":true,"state":"running","degraded":null}   (idempotent)
ign rig logs --tail 3     -> raw jvm gateway lines streamed
ign rig reset --yes ...   -> {"ok":true,"state":"running","removed":["ign-gate2-833_gateway_data"]} (43.4s;
                             fresh trial 7172s proves the clean slate)
ign rig down --compact    -> {"ok":true} (stack stopped)
```

### Headless API-token provisioning (the headline ask) — **solved, version-agnostic (proven on 8.3.3 and 8.3.6)**

The 04-03 blocker (the `resources/ignition/api-token` create's `collection` value, UA-gated config UI) is fully solved. The wire recipe, captured from the gateway UI's own wizard traffic and then replayed headlessly:

1. **Login** — the shipped tier-1 OIDC 10-step ladder (curl; `idp.rs`'s exact endpoint sequence) → `webui-sid` cookie + `csrfToken`.
2. **Generate** — `POST /data/api/v1/api-token/generate` `{}` (session + `X-CSRF-Token`) → `{"key","hash"}`.
3. **Register** — `POST /data/api/v1/resources/ignition/api-token` (session + CSRF) with an ARRAY body: `[{"name":N,"collection":"core","enabled":true,"description":"","config":{"profile":{"type":"basic-token","secureChannelRequired":false,"securityLevels":[{"name":"Authenticated","children":[]}],"timestamp":<epoch_ms>},"settings":{"tokenHash":<hash>}}}]`. **`collection` is `"core"`** — the 04-03 mystery value.
4. **The 403 fix (required, and phase 02's research predicted it)** — the gateway's `security-properties` singleton defaults `readPermissions`/`writePermissions` (AnyOf) to `Authenticated/Roles/Administrator`; a plain-`Authenticated` token gets 403 on every `/data/api/v1/*` route. `GET /data/api/v1/resources/singleton/ignition/security-properties?collection=core`, then `PUT /data/api/v1/resources/ignition/security-properties?collection=core` (array body carrying the current `signature`) with both permission sets set to `AnyOf [{"name":"Authenticated","children":[]}]`.
5. The token is `NAME:key`, sent as `X-Ignition-API-Token` (per-token `secureChannelRequired:false` keeps plain http working).

Full copy-paste script (this exact script produced working tokens on both rigs):

```bash
#!/bin/bash
# provision_token.sh URL USER PASS TOKENNAME — prints NAME:KEY
set -e
URL=$1; USER=$2; PASS=$3; NAME=$4
JAR=$(mktemp)
LOC1=$(curl -s -c "$JAR" -o /dev/null -w '%{redirect_url}' "$URL/data/app/login")
LOC2=$(curl -s -b "$JAR" -c "$JAR" -o /dev/null -w '%{redirect_url}' "$LOC1")
T0=$(echo "$LOC2" | sed -n 's/.*token=\([^&]*\).*/\1/p')
T1=$(curl -s -b "$JAR" -c "$JAR" -H 'Content-Type: application/json' -d "{\"token\":\"$T0\"}" "$URL/idp/default/authn/next-challenge" | jq -r .token)
R4=$(curl -s -b "$JAR" -c "$JAR" -H 'Content-Type: application/json' -d "{\"token\":\"$T1\",\"rememberMe\":false,\"challenge\":{\"username\":\"$USER\",\"password\":\"$PASS\"}}" "$URL/idp/default/authn/submit-challenge/basic")
[ "$(echo "$R4" | jq -r .success)" = "true" ] || { echo "LOGIN-FAILED: $R4" >&2; exit 5; }
T2=$(echo "$R4" | jq -r .token)
T3=$(curl -s -b "$JAR" -c "$JAR" -H 'Content-Type: application/json' -d "{\"token\":\"$T2\"}" "$URL/idp/default/authn/next-challenge" | jq -r .token)
LOC6=$(curl -s -b "$JAR" -c "$JAR" -o /dev/null -w '%{redirect_url}' "${LOC1}&token=${T3}")
curl -s -b "$JAR" -c "$JAR" -o /dev/null "$(curl -s -b "$JAR" -c "$JAR" -o /dev/null -w '%{redirect_url}' "$LOC6")"
CSRF=$(curl -s -b "$JAR" -c "$JAR" "$URL/data/app/session" | jq -r '.csrfToken // empty')
GEN=$(curl -s -b "$JAR" -H "X-CSRF-Token: $CSRF" -H 'Content-Type: application/json' -X POST -d '{}' "$URL/data/api/v1/api-token/generate")
KEY=$(echo "$GEN" | jq -r .key); HASH=$(echo "$GEN" | jq -r .hash)
TS=$(($(date +%s)*1000))
BODY=$(printf '[{"name":"%s","collection":"core","enabled":true,"description":"","config":{"profile":{"type":"basic-token","secureChannelRequired":false,"securityLevels":[{"name":"Authenticated","children":[]}],"timestamp":%s},"settings":{"tokenHash":"%s"}}}]' "$NAME" "$TS" "$HASH")
curl -s -b "$JAR" -H "X-CSRF-Token: $CSRF" -H 'Content-Type: application/json' -X POST -d "$BODY" "$URL/data/api/v1/resources/ignition/api-token" | jq -e '.success==true' >/dev/null
curl -s -b "$JAR" "$URL/data/api/v1/resources/singleton/ignition/security-properties?collection=core" > /tmp/sp.json
python3 -c "
import json; d=json.load(open('/tmp/sp.json')); p={'name':'Authenticated','children':[]}
d['config']['readPermissions']={'type':'AnyOf','securityLevels':[p]}
d['config']['writePermissions']={'type':'AnyOf','securityLevels':[p]}
json.dump([{'name':'security-properties','type':d['type'],'collection':'core','enabled':True,'description':'','signature':d['signature'],'config':d['config']}], open('/tmp/sp-arr.json','w'))"
curl -s -b "$JAR" -H "X-CSRF-Token: $CSRF" -H 'Content-Type: application/json' -X PUT -d @/tmp/sp-arr.json "$URL/data/api/v1/resources/ignition/security-properties?collection=core" | jq -e '.success==true' >/dev/null
echo "$NAME:$KEY"
```

Verified working: `g836tok:...` on the 8.3.6 rig (gateway-info 200, projects/list 200) and `gatetest1:...` on the 8.3.3 rig (reads AND writes — project create/delete both 200).

### Newly-open product finding (for phase 5+ planning, NOT fixed here)

The `resource` client family (`crates/ignition-core/src/client/resources.rs`, the `ign resource` CLI arm, and the e2e_rig/e2e_projects witness approach) targets `/data/api/v1/projects/{name}/resources/**` routes that **do not exist** in real 8.3 gateways. Per-project resource access exists only via `projects/export/{name}` / `projects/import/{name}` (both in the openapi extract). This needs a phase-5 re-plan decision (re-point the family to export/import, or drop it) — deliberately not touched in this run per the execution contract.

### Environmental note (not caused by this run)

At ~02:45 UTC — during an idle window with no docker commands from this session — an OrbStack VM restart cycled containers: `whk-services-ignition-1` re-exited (137, as before), and `ignition-devops-gateway-1` (restart-policy `no`, down since 2026-08-23) came back up and is currently running/healthy with OrbStack proxying 9088/9043, alongside the still-running 9088 ssh tunnel. The same OrbStack instability was recorded in 04-03's session. All guarded stacks (cask-agents, t3code, whk-mes, whk-services) are accounted for and were never touched by any command in this run.

### Cleanup state

Both disposable rigs fully removed (container `ign-gate1-836` removed; compose project `ign-gate2-833` `down -v --remove-orphans` — volume `ign-gate2-833_gateway_data` deleted); temp rig dirs and scratch files removed; no disposable stacks left running. Repo working tree: only `.planning/` doc changes + the openapi extract artifact; `crates/` diff empty.

_Addendum executed: 2026-08-24, autonomous docker-compose run (executor: Claude, GSD live-gate closure)._

---

## Delta Re-Verification (2026-08-24T03:54:18Z)

Independent spot-check of the addendum's claims by the verifier (gates were NOT re-run — rigs are disposed and a re-run needs another ~2h trial expiry; claims were checked against committed artifacts, git, docker state, and source code).

| Claim (addendum) | Check performed | Result |
| ---------------- | --------------- | ------ |
| Gates commit bf51760 (docs only, no product code) | `git show bf51760 --stat`; `git diff ab76020..bf51760 --stat -- crates/` | ✅ exactly 3 files: openapi extract (+203,046 lines), 04-USER-SETUP.md (+30/−5), 04-VERIFICATION.md (+314); crates/ diff EMPTY |
| Openapi extract committed with the resource-route evidence | File present (10.7 MB) in `.planning/phases/03-project-operations/`; enumerated all `/data/api/v1/projects/*` and `*resources*` paths | ✅ `captured_from: http://localhost:19188` (the 8.3.3 rig — matches the executor's filename-misnomer disclosure); **ZERO** `/projects/{name}/resources` paths; projects family = copy/export/find/import/list/names/parents/rename only; the `resources` family that exists is config-resources (`/data/api/v1/resources/{module}/{type}`) — which also explains the gate-matcher false-positive exactly as disclosed |
| 04-USER-SETUP.md updated to superseded | Read in full | ✅ "Status: superseded (no human action required)", both tasks struck obsolete, points at the addendum recipe |
| Disposable rigs cleaned up (`ign-gate*` gone) | `docker ps -a` + `docker compose ls -a` | ✅ zero `ign-gate` containers/projects anywhere (not even exited) |
| Guarded stacks untouched | `docker ps -a`, `docker compose ls` | ✅ cask-agents `running(1)` (cask-postgres Up 14h — predates the run); t3code `running(2)`; whk-mes partially up with rabbitmq exit predating the run (14h) and db restart consistent with the disclosed OrbStack VM restart (~02:45 UTC); whk-services `exited(6)` as before (its ignition-1 re-exit 137 during the OrbStack restart, exactly as disclosed); ign-research untouched (Up, healthy, 18088). The also-disclosed surprise — `ignition-devops-gateway-1` back Up/healthy on 9088/9043 after the OrbStack restart — verified present. No evidence of any session command touching these stacks |
| Trial flip physics (8.3.6: 0s/expired → 7187s/active) | Arithmetic + harness source | ✅ 7200−7187=13s elapsed-to-curl, consistent with a flip moments before; `trial_reset_tier1_live` exists (trial_contract.rs:640) and my initial verification confirmed the gate refuses to pass without the read-back flip |
| e2e gate blocked at resource step, not snapshot paths | e2e_rig.rs source | ✅ blocked step is `resource get` (line 314) = Phase 3 client family; the round-trip asserts are project-granular (lines 339/343) and were satisfied by the CLI-verb run |
| Token recipe (collection:"core" + security-properties patch) | Recipe reviewed against phase-02 research prediction + 04-USER-SETUP.md corroboration | ✅ internally consistent; verified working on both rigs per addendum output; version-agnostic claim supported by two independent rigs |

**Credence note:** the executor demonstrably reports adverse findings faithfully (gate-matcher false-positive, filename misnomer, OrbStack restart, tier-0 skip) — every disclosed negative spot-checked here is accurate, which raises confidence in the reported positives.

### Judgment

- **Criterion 3 → CLOSED.** The ≥2-minor-version requirement asked for the reset mechanism verified e2e on two minor versions. The spike-chosen mechanism (tier-1 native OIDC + required read-back flip, per 04-RESEARCH.md) is now live-proven on 8.3.3 and 8.3.6. Tier-0 on 8.3.6 was never a criterion (it is a fallback probe, and the live-verified state gate makes it unrunnable post-reset).
- **Criterion 4 → CLOSED.** The round-trip is two-sided-proven via real CLI verbs (snapshot → mutate → restore → witness survives + marker gone) at project granularity — the granularity the snapshot manifest itself uses (gwbk + project exports). The shipped `e2e_rig` binary's failure is attributable line-by-line to the **Phase 3** resource-family defect (nonexistent routes, openapi-evidenced), not to any Phase 4 code path; recorded as cross-phase finding PROJ-05-resource-routes and routed to Phase 5 planning.
- **Goal → achieved.** A user (or agent) can run the complete rig lifecycle from the CLI on real gateways of both supported minor versions — including trial management and repeatable-state snapshot/restore — as demonstrated end-to-end on live rigs, plus the bonus discovery that the headless token recipe makes the whole fixture self-managed with zero human-held secrets beyond admin creds.

**Final: status `passed`, 18/18 truths.** Open cross-phase item for Phase 5 planning: the Phase 3 resource-route defect (and the two disclosed gate-hygiene nits).

---

_Verified: 2026-08-24T03:54:18Z (delta re-verification; initial: 2026-08-23T03:40:48Z)_
_Verifier: Claude (gsd-verifier)_
