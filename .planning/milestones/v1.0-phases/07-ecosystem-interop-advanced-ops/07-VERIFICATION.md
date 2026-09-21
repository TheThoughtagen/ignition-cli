---
phase: 07-ecosystem-interop-advanced-ops
verified: 2026-08-28T23:55:00Z
status: passed
score: 8/8 requirements verified (19/19 plan truths verified)
re_verification: false
human_verification:
  - test: "Live cross-gateway promotion against two real gateways"
    expected: "ign project diff dev prod --project X shows real deltas; sync --all-changed --yes lands A's resources in B (e2e witness: cargo test -p ignition-cli --test e2e_projects -- --ignored with IGNITION_LIVE_URL + IGNITION_LIVE_URL_B + IGNITION_LIVE_MUTATIONS=1)"
    why_human: "Requires two live Ignition gateways; e2e witness exists and is env-gated by design"
  - test: "Real gwbk download + restore on a live gateway"
    expected: "ign backup download produces a valid .gwbk; restore --yes replaces gateway state (gateway restarts and blocks ~minutes)"
    why_human: "Destructive against a live gateway; wire shapes are wiremock-pinned but the restart-block window is a live behavior"
  - test: "scriptExec round trip on a deployed route"
    expected: "ign webdev deploy --with-script-exec then ign script run --code '2+2' returns result 4 with stdout/elapsedMs"
    why_human: "Requires live gateway with deployed routes; probe+exec sequence is wiremock-pinned"
  - test: "nvim/VS Code round-trip editing workflow end-to-end"
    expected: "export --decode-scripts, edit a .py sidecar in an editor, import --encode-scripts, see the edit on the gateway"
    why_human: "Human editor workflow; byte-identical unedited round-trip is contract-pinned"
---

# Phase 7: Ecosystem Interop & Advanced Ops Verification Report

**Phase Goal:** The CLI plugs into the WhiskeyHouse ecosystem and handles the advanced workflows — cross-gateway promotion, backups/EAM, opt-in script execution, and round-trip editing with nvim/ignition-lint/git-module — completing the toolset.
**Verified:** 2026-08-28T23:55:00Z
**Status:** passed
**Re-verification:** No — initial verification

## Verification Method

Goal-backward, evidence over claims: (1) all four plans' artifacts checked at three levels (exists / substantive / wired) via grep + read; (2) full workspace test suite executed; (3) clippy `-D warnings` + fmt check run; (4) **live binary smoke tests** of the actual `ign` executable for every behavior verifiable without a gateway (refusals, exit codes, envelopes, offline paths, guard ladder, help semantics); (5) all 12 task commits from the four summaries verified to exist in git.

## Goal Achievement

### Observable Truths (by Success Criterion)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | User can diff two gateways' projects at resource level and selectively sync (dev→test→prod), with scope explicit | ✓ VERIFIED | `project diff`/`project sync` verbs live (help shows B-relative-to-A semantics verbatim); golden pins `"scope":"project","profile_a":"dev","profile_b":"other"` with envelope keeping active profile; sync contract proves member-level import-body honesty; e2e two-gateway witness exists (env-gated) |
| 2 | User can download/restore gwbk via native API, list EAM history, create guarded EAM tasks | ✓ VERIFIED | `ign backup download/restore` + `ign eam history/tasks/task new/task force` all present; live: restore refusal exit 2 profile null; live: guard ladder all three classes (refused type exit 6 `eam_task_type_refused` + EXT-03 pointer; mutating exit 2; `eam_backup` OnDemand unguarded); contract_backup 7 green, contract_eam 8 green |
| 3 | `ign script run` executes via scriptExec route — opt-in only (disabled by default) | ✓ VERIFIED | Live: with token but no webdev secret → exit 6 `script_exec_not_configured`, hint names `ign webdev deploy --with-script-exec`; CLI enum has NO `--yes` (structural opt-in); `{stdout, result, elapsedMs}` contract-pinned; secret redaction canary pinned |
| 4 | `--decode-scripts`/`--encode-scripts` round-trip, `ign lint` delegation, `--from-export` offline browsing | ✓ VERIFIED | Flags live on export/import help; contract `unedited_round_trip_is_byte_identical_per_member` + splice-scope + missing-sidecar pins green; live: lint absent-tool exit 6 with `uv tool install ignition-lint-toolkit` hint; doctor posture + `--strict` in help and 5 contract_lint tests; live: all three from-export layouts browse offline (CLI-export JSON, legacy single-file, git-module dir with `Pump%2FMain` decoding, `_types_`, dot-entry skip), profile null, `source: "export"` |

**Score:** 4/4 success criteria — all verified.

### Plan-Level Truths (must_haves consolidated)

| Plan | Truths | Status | Key evidence |
|------|--------|--------|--------------|
| 07-01 | 6 | ✓ all VERIFIED | normalize_descriptor strips lastModification+Signature (key-order-independence unit pin); `diff_same_content_differing_modification_attributes_is_same`; sync guard live exit 2/profile-null/zero-requests binary-pinned; `--delete` opt-in contract; label reconciliation documented at mapping site |
| 07-02 | 6 | ✓ all VERIFIED | classify.rs path+content-scoped 403 arm → `EamNotController`; guard ladder live-proven all 3 classes; force = find→204→history sequence contract-pinned; both slugs in the two-place exit table + enumerated test |
| 07-03 | 5 | ✓ all VERIFIED | script_run action + `read_script_input` pure reader (both-inputs refusal); probe+exec wiremock pin; slug hint names the deploy flag verbatim; no `--yes` anywhere on the verb |
| 07-04 | 5+ | ✓ all VERIFIED | `flint_encode(flint_decode(x)) == x` sacred-invariant test + corpus; expression/script-python pass-through pinned; lint arg-vector spawn proof; from-export short-circuits before resolution (structural offline proof) |

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/ignition-core/src/client/resources.rs` | pure diff engine | ✓ VERIFIED | `diff_members` + `normalize_descriptor` + 20 unit tests incl. key-order pin |
| `crates/ignition-core/src/actions/projects.rs` | project_diff + project_sync | ✓ VERIFIED | both `pub async fn`s; scope literal "project" at both result sites; 1711 lines |
| `crates/ignition-cli/src/cli.rs` | Diff/Sync/Backup/Eam/Script/Lint surfaces + flags | ✓ VERIFIED | all enums + `decode_scripts`/`encode_scripts`/`from_export` flags present |
| `crates/ignition-tui/src/routes.rs` | rows for all new verbs | ✓ VERIFIED | "project diff"/"project sync"/"script run"/"lint" + 6 backup/eam rows |
| `crates/ignition-core/src/client/eam.rs` | EAM wire constants + models | ✓ VERIFIED | `EAM_HISTORY_PATH`, `EAM_TASKS_RESOURCE = "com.inductiveautomation.eam/eam-tasks"` |
| `crates/ignition-core/src/actions/eam.rs` | eam action family | ✓ VERIFIED | `eam_history`/`eam_tasks`/create/force + pure guard ladder; 575 lines |
| `crates/ignition-core/src/actions/backup.rs` | backup actions | ✓ VERIFIED | `backup_download`/`backup_restore` with pre-checks |
| `crates/ignition-core/src/error.rs` | 4 additive exit-6 slugs | ✓ VERIFIED | eam_not_controller, eam_task_type_refused, script_exec_not_configured, lint_tool_absent — all in doc table + slug map + enumerated test |
| `crates/ignition-core/src/actions/script.rs` | script_run action | ✓ VERIFIED | secret gate + probe/exec; 656 lines |
| `crates/ignition-core/src/client/scripts_codec.rs` | PURE Flint codec | ✓ VERIFIED | 1248 lines; span-level splice; sacred invariant tests |
| `crates/ignition-core/src/actions/lint.rs` | PATH discovery + spawn | ✓ VERIFIED | `lint_run` + `find_lint_tool` + `strict_exit_code` |
| `crates/ignition-core/src/actions/tags.rs` | from-export parsing | ✓ VERIFIED | `browse_rows_from_export` — 3 layouts, pure |

No MISSING, STUB, or ORPHANED artifacts. All wired (dispatch arms verified in main.rs for every verb).

### Key Link Verification

| From → To | Via | Status |
|-----------|-----|--------|
| main.rs → project_diff/project_sync | two-client resolution then action | ✓ WIRED (guard order verified: selection validation → --yes → resolve_two_clients) |
| actions/projects.rs → diff_members / replace_member / scripts_codec:: | export-compare-splice chain | ✓ WIRED |
| classify.rs → EamNotController | /data/eam/ path-scoped 403 | ✓ WIRED (is_eam_url + content check, negative pins) |
| actions/eam.rs → config-resource seam | EAM_TASKS_RESOURCE array-body POST | ✓ WIRED (wiremock body pins) |
| actions/script.rs → webdev_route_call + webdev_secret | probe+exec with secret header | ✓ WIRED |
| actions/lint.rs → tokio Command::new | arg-vector spawn | ✓ WIRED (argv recorded + asserted in contract) |
| main.rs → browse_rows_from_export | offline short-circuit before resolution | ✓ WIRED (live-proven: no credential needed) |

### Requirements Coverage

| Requirement | Status | Evidence |
|-------------|--------|----------|
| SYNC-01 | ✓ SATISFIED | `ign project diff` verb + normalization + goldens (live help + tests) |
| SYNC-02 | ✓ SATISFIED | `ign project sync` guarded A→B promotion, member-level proven, e2e witness |
| SCRPT-01 | ✓ SATISFIED | `ign script run` — structural opt-in live-proven (exit 6 without deploy) |
| BKUP-01 | ✓ SATISFIED | backup download (--type roaming/all) + 8th guarded restore |
| BKUP-02 | ✓ SATISFIED | eam history + tasks reads; task new guard ladder; task force |
| INTR-01 | ✓ SATISFIED | decode/encode flags + byte-identical unedited round-trip contract |
| INTR-02 | ✓ SATISFIED | lint delegation, doctor posture, --strict, install hint (all live/tool-pinned) |
| INTR-03 | ✓ SATISFIED | from-export 3 layouts offline, profile null (live-verified on real fixtures) |

Note: REQUIREMENTS.md checkboxes still read "Pending" — tracker state only; code satisfies all eight.

### Anti-Patterns Found

| File | Pattern | Severity | Impact |
|------|---------|----------|--------|
| (none) | Zero TODO/FIXME/HACK/placeholder/unimplemented! across all 16 phase source files | — | — |

ℹ️ Info: one stray untracked file `pterm_20260828031855.zip` at repo root (not a phase artifact). ℹ️ Info: `eam task new` uses positional `<NAME> <TYPE>` rather than the plan's `--type` flag sketch — CLI-shape deviation, guard semantics exactly as locked. ℹ️ Info: STATE.md says "841 tests"; current count 852 (07-04 landed after the note).

### Workspace Health (executed during verification)

- `cargo test --workspace`: **852 passed, 0 failed** (incl. 26 contract_projects, 8 contract_eam, 7 contract_backup, 5 contract_script, 5 contract_lint, 28 contract_tags, scripts_codec_contract, tui coverage)
- `cargo clippy --workspace -- -D warnings`: **clean**
- `cargo fmt --all --check`: **clean**
- All 12 task commits across the four summaries exist in git history

### Live Binary Smoke Tests (no gateway required)

| Test | Expected | Actual |
|------|----------|--------|
| `project sync a b --project X --all-changed` (no --yes) | exit 2, profile null, consequence text | ✓ exit 2, `confirmation_required`, "overwrite-import the whole project on b — replaces concurrent Designer edits", profile null |
| `backup restore` (no --yes) | exit 2 | ✓ exit 2 |
| `eam task new X eam_restoreBackup` | exit 6 refused + EXT-03 pointer | ✓ `eam_task_type_refused` naming fleet consequence |
| `eam task new X eam_restart` (no --yes) | exit 2 | ✓ `confirmation_required`, "mutates the agent targets" |
| `eam task new X eam_backup` | passes guard → proceeds to resolution | ✓ (reached secret resolution — guard correctly absent) |
| `script run --code '2+2'` (no webdev secret) | exit 6 slug + deploy-flag hint | ✓ exact hint: "run `ign webdev deploy --with-script-exec`…" |
| `lint` with empty PATH | exit 6 + install hint | ✓ `uv tool install ignition-lint-toolkit` + repo URL |
| `tags browse --from-export` (single-file, git-module dir) | offline tree, profile null | ✓ both layouts + `Pump%2FMain` decoded, dot-entry skipped, `source: "export"` |

### Human Verification Required (optional live confirmations)

### 1. Live cross-gateway promotion
**Test:** Run the env-gated e2e witness against two real gateways (`IGNITION_LIVE_URL` + `IGNITION_LIVE_URL_B` + `IGNITION_LIVE_MUTATIONS=1`)
**Expected:** put→diff→sync→re-export adoption oracle passes
**Why human:** needs two live Ignition gateways

### 2. Real gwbk restore
**Test:** `ign backup download` then `backup restore --yes` on a live gateway
**Expected:** valid gwbk; gateway state replaced, restart-block window observed
**Why human:** destructive on live infrastructure

### 3. Live scriptExec round trip
**Test:** `ign webdev deploy --with-script-exec` then `ign script run --code '2+2'`
**Expected:** `{stdout, result: 4, elapsedMs}`
**Why human:** needs live gateway with deployed routes

### 4. Editor round-trip workflow
**Test:** export `--decode-scripts`, edit a sidecar in nvim/VS Code, import `--encode-scripts`
**Expected:** only the edited value changed on the gateway
**Why human:** human editor workflow (byte-level round-trip already contract-pinned)

### Gaps Summary

None. All 8 requirements satisfied; all plan truths verified at artifact, wiring, and behavior levels; live binary smoke tests corroborate the wiremock contract suite; workspace fully green (852 tests, clippy/fmt clean). Live-gateway items are env-gated e2e by the project's own design and are listed as optional human confirmations, not gaps.

---

_Verified: 2026-08-28T23:55:00Z_
_Verifier: Claude (gsd-verifier)_
