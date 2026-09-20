---
phase: 03-project-operations
verified: 2026-08-22T09:05:00Z
status: passed
score: 18/18 must-haves verified
human_verification:
  - test: "Run the family against a live Ignition 8.3+ rig (IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN, plus IGNITION_LIVE_MUTATIONS=1 for the e2e loop)"
    expected: "project CRUD/export/import and resource list/get/put/delete behave per the wiremock-pinned contracts; `cargo test -- --ignored` in ignition-cli runs the full create→edit→export→import(abort/overwrite)→rename→copy→delete loop green; the openapi-capture test writes the trimmed phase-3 extract"
    why_human: "Wire contracts are pinned via wiremock recorded-request proofs (the phase's stated development discipline); live-gateway behavior against real Ignition cannot be verified programmatically. The resource family is MEDIUM-confidence (single source — ignition-mcp) and is doc-flagged as such in client/resources.rs with the openapi capture gate ready. Explicitly deferred opt-in per plans and orchestrator notes — NOT a gap."
  - test: "Export a large project (100+ MB ZIP) and observe memory usage"
    expected: "RSS stays flat — the ZIP streams chunk-by-chunk through download_to_file with no Vec<u8> accumulation"
    why_human: "Streaming code shape is verified (bytes_stream → write_all chunk loop), but actual memory profile under load requires a live gateway and a real large export"
---

# Phase 3: Project Operations Verification Report

**Phase Goal:** A user can create, move, export, import, and surgically edit Ignition projects entirely from the CLI — the gateway webpage's project management replaced, and the first mutating commands prove the `--yes`/collision-policy conventions.
**Verified:** 2026-08-22T09:05:00Z
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

**Plan 03-01 — Project CRUD (PROJ-01, PROJ-02)**

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `ign project list` shows every runnable project with name/title/description/enabled/parent/inheritable | ✓ VERIFIED | `ProjectCommand::List` (cli.rs:203); ProjectRecord models; `project_list_render_modes_golden` (3 render modes); README row documents six-key agent shape |
| 2 | `ign project new NAME [--title --description --parent --inheritable --disabled]` — only provided fields serialize | ✓ VERIFIED | cli.rs:205 `New` with all flags; `project_new_success_golden`; wiremock body-proof per 03-01-SUMMARY |
| 3 | copy/rename/set without --yes; `set --parent` IS the inheritance move (PUT modify) | ✓ VERIFIED | `project_copy_and_rename_human_lines`, `project_set_title_success_golden`; clap ArgGroup requires ≥1 field on `set` |
| 4 | delete without --yes refuses exit 2 pre-resolution; with --yes the DELETE carries `confirm=true` QUERY param | ✓ VERIFIED | main.rs:703 guard BEFORE `resolve_gateway_api` (verbatim sessions-terminate shape); client `delete_with_query(..., [("confirm", "true")])` (mod.rs:866-869); goldens `project_delete_without_yes_exits_2_golden`, `project_delete_with_yes_proves_confirm_true_on_wire`, `project_delete_nonexistent_exits_6` |
| 5 | Names with spaces/mixed case ride the wire percent-encoded per segment | ✓ VERIFIED | `encode_segment` (NON_ALPHANUMERIC) used in all 7 path builders (projects.rs:45-74); unit test `encode_segment_handles_spaces_and_symbols` pins `My%20Project` |

**Plan 03-02 — Export/Import (PROJ-03, PROJ-04)**

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 6 | Export streams ZIP to disk chunk-by-chunk (no Vec<u8>) under 120 s per-request timeout; stdout data-only | ✓ VERIFIED | `download_to_file`: classify → `bytes_stream()` → `AsyncWriteExt::write_all` chunk loop (mod.rs:412-462); `PROJECT_EXPORT_TIMEOUT` 120 s; export goldens incl. explicit-output |
| 7 | Import from file/stdin: raw bytes, Content-Type application/zip + known Content-Length + `overwrite=<bool>` query, 300 s timeout | ✓ VERIFIED | mod.rs:893-906 (query param + CONTENT_TYPE header); `PROJECT_IMPORT_TIMEOUT = Duration::from_secs(300)` (projects.rs:85); `project_import_stdin_golden` |
| 8 | Default collision abort: exit 6 `project_exists` BEFORE any upload (find pre-check) | ✓ VERIFIED | Action-layer find pre-check → `CoreError::ProjectExists` (actions/projects.rs:516) before upload; `project_import_abort_collision_exits_6_golden`; exit-code table test pins 6/`project_exists` |
| 9 | `--collision-policy overwrite` refused without --yes (exit 2, pre-resolution); hint warns replace-not-merge | ✓ VERIFIED | main.rs:749-756 conditional guard (Overwrite-only) BEFORE `resolve_gateway_api`; `project_import_overwrite_without_yes_exits_2_golden` + `project_import_overwrite_with_yes_uploads` |
| 10 | Both export and import JSON carry `scope: {includes, excludes}` naming tag-providers/tags/UDTs excluded | ✓ VERIFIED | `EXPORT_INCLUDES`/`EXPORT_EXCLUDES` consts (actions/projects.rs:40-58) with "tag-providers"/"tags"/"udts"; unit test pins scope arrays (roadmap criterion 4) |
| 11 | Non-ZIP (missing PK\x03\x04) or >512 MB input refuses exit 2 `invalid_import_file` before network I/O | ✓ VERIFIED | ZIP magic const + `IMPORT_MAX_BYTES = 512*1024*1024` (actions/projects.rs:95-100); `project_import_non_zip_exits_2_golden` + `import_size_guard_refuses_over_512mb` |

**Plan 03-03 — Surgical Resource Loop (PROJ-05)**

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 12 | `ign resource list PROJECT [--prefix P]` — path query filter rides wire when given; family doc-flagged MEDIUM | ✓ VERIFIED | ResourceCommand::List (cli.rs:311); `path=<prefix>` query only when given (resources.rs:40); MEDIUM-confidence flag in module docs (resources.rs:5) |
| 13 | `ign resource get` returns JSON/text; binary resource refuses exit 6 `resource_binary` with export/import hint | ✓ VERIFIED | `resource_get_json_pretty_golden`, `resource_get_text_raw_golden`, `resource_get_binary_refuses_exit_6_golden`; ResourceBinary exit 6 pinned in error.rs table test |
| 14 | `ign resource put --file F\|-` upserts; JSON if parseable, else UTF-8 text; binary-looking input refuses | ✓ VERIFIED | content sniffer (actions/resources.rs:142-183); `resource_put_from_file_json_golden`, `resource_put_stdin_text_golden`, `resource_put_binary_input_refuses_before_network_golden`, `resource_put_missing_file_exits_2_golden` (invalid_input) |
| 15 | `ign resource delete` refuses without --yes (exit 2, pre-resolution) | ✓ VERIFIED | main.rs:878 guard BEFORE `resolve_gateway_api` (LOCKED shape); `resource_delete_without_yes_exits_2_golden` |
| 16 | Resource paths per-segment encoded, slashes preserved | ✓ VERIFIED | `split('/').map(encode_segment)` (resources.rs:61-66) reusing 03-01's encoder |
| 17 | e2e skeleton green by default; `-- --ignored` + env contract runs full loop with two-sided replace-not-merge pin | ✓ VERIFIED | 490-line e2e_projects.rs; `assert_cmd::Command::cargo_bin("ign")` (true binary e2e); skip-by-default green (0 failures in default run, 12+2+1 ignored); assertions pin pre-export resource SURVIVED + post-export resource `not_found` (lines 358-370) |
| 18 | `#[ignore]` openapi-extract test fetches /openapi.json and writes trimmed phase extract | ✓ VERIFIED | `openapi_capture_writes_phase3_extract` (e2e_projects.rs:416), ignore-gated on env contract |

**Score:** 18/18 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/ignition-core/src/client/projects.rs` | models + path consts + encode_segment + 7 capabilities (≥80 lines) | ✓ VERIFIED | 307 lines; all 7 path builders + encoder + unit tests |
| `crates/ignition-core/src/actions/projects.rs` | CRUD + export/import actions with scope (≥60/≥100 lines) | ✓ VERIFIED | 1168 lines; EXPORT_INCLUDES/EXCLUDES + collision pre-check |
| `crates/ignition-core/src/client/mod.rs` | post_json/put_json + download_to_file (`bytes_stream`) + `async fn project_delete` | ✓ VERIFIED | 1017 lines; all patterns present (mod.rs:439, :861) |
| `crates/ignition-cli/tests/contract_projects.rs` | goldens (≥60 lines) | ✓ VERIFIED | 888 lines, 14 tokio tests |
| `crates/ignition-core/src/error.rs` | ProjectExists/InvalidImportFile/ResourceBinary additive variants | ✓ VERIFIED | 600 lines; slugs + exit codes pinned by table test |
| `crates/ignition-core/src/client/resources.rs` | ResourceEntry/ResourceContent + 4 capabilities, MEDIUM-flagged (≥60 lines) | ✓ VERIFIED | 156 lines; doc-flagged |
| `crates/ignition-core/src/actions/resources.rs` | list/get/put/delete + sniffer (≥60 lines) | ✓ VERIFIED | 278 lines; NUL-in-first-8KiB heuristic |
| `crates/ignition-cli/tests/e2e_projects.rs` | ignore-gated assert_cmd e2e skeleton (≥80 lines) | ✓ VERIFIED | 490 lines; env contract documented in-module |
| `crates/ignition-core/tests/projects_contract.rs` | core-level wiremock contract | ✓ VERIFIED | 586 lines, 15 tests |
| `crates/ignition-core/tests/resources_contract.rs` | resource wiremock contract | ✓ VERIFIED | 311 lines, 8 tests |
| `crates/ignition-cli/tests/contract_resources.rs` | CLI resource goldens | ✓ VERIFIED | 686 lines, 11 tests |
| `crates/ignition-core/tests/live_gateway.rs` | `live_projects_list` #[ignore] hook | ✓ VERIFIED | present (line 384), ignore-gated |

### Key Link Verification

| From | To | Via | Status |
|------|----|----|--------|
| project delete / import-overwrite / resource delete dispatch arms | `require_confirmation` | guard BEFORE `resolve_gateway_api` (exit 2, profile null) | ✓ WIRED (main.rs:703, :753, :878) |
| client `project_delete` | DELETE + confirm=true query | `delete_with_query` | ✓ WIRED (mod.rs:866-869) + wiremock recorded-request golden |
| all {name} path builders | `encode_segment` | percent-encoding per segment | ✓ WIRED (projects.rs ×7; resources.rs per-segment split) |
| new trait methods | test doubles | `impl GatewayApi for` ×10 rigs | ✓ WIRED (10 impls incl. ProjectsRig; unreachable! stubs sanctioned) |
| `download_to_file` | reqwest `bytes_stream` + tokio::fs | classify-first chunk loop | ✓ WIRED (mod.rs:412-462) |
| `project_import` | POST import/{name}?overwrite= | application/zip raw body + 300 s timeout | ✓ WIRED (mod.rs:893-906) |
| collision pre-check | `project_find` → `ProjectExists` | action-layer find before upload | ✓ WIRED (actions/projects.rs:516) |
| resource get/put | content sniffer → `ResourceBinary` | UTF-8+JSON / UTF-8 / NUL heuristic | ✓ WIRED (actions/resources.rs:142-183) |
| e2e loop | built `ign` binary | `assert_cmd::Command::cargo_bin` | ✓ WIRED (e2e_projects.rs:43, :115) |

### Requirements Coverage

| Requirement | Status | Blocking Issue |
|-------------|--------|----------------|
| PROJ-01 — list projects with inheritance/parent info | ✓ SATISFIED | — |
| PROJ-02 — create, delete, copy, rename projects | ✓ SATISFIED | — |
| PROJ-03 — export to file with collision policy on import | ✓ SATISFIED | — |
| PROJ-04 — import from file or stdin | ✓ SATISFIED | — |
| PROJ-05 — list/get/put/delete individual resources | ✓ SATISFIED | — |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| actions/* (test doubles) | various | 286 × `unreachable!` stubs | ℹ️ Info | Sanctioned by plan key_links ("stub unreachable!(\"not part of this action\") so nothing compiles half-way") — not a defect |

No TODO/FIXME/PLACEHOLDER/unimplemented markers in any phase source file. No empty-handler or console-log-only implementations.

### Build & Test Evidence

- `cargo test --workspace`: **ALL GREEN** — 0 failures across all suites (~250 tests: 98+15+14+13+12+12+12+11+9+8+8+7+7+6+6+6+6+3…); 15 ignored are the designed opt-in live/e2e gates
- `cargo clippy --workspace --all-targets -- -D warnings`: **CLEAN**
- Git: 11 phase commits (971b88b…a26811e) covering all three plans; working tree clean (one unrelated untracked `.playwright-mcp/`)

### Human Verification Required

Non-blocking, deferred by design (wiremock-first discipline per plans; live hooks #[ignore]-gated and ready):

1. **Live-rig family validation** — set IGNITION_LIVE_URL/ TOKEN/ MUTATIONS against a commissioned 8.3+ gateway; run `cargo test -p ignition-cli -- --ignored`. Especially valuable for the MEDIUM-confidence resource family; the openapi-capture test settles wire truth the moment a token exists.
2. **Large-export memory profile** — verify flat RSS on a 100+ MB export (code shape verified; runtime profile needs a real gateway).

### Gaps Summary

None. All 18 truths verified with code + golden + wiremock-recorded-request evidence; all 12 artifacts substantive and wired; all 9 key links connected; PROJ-01 through PROJ-05 satisfied; tests and clippy green. The `--yes`/collision-policy conventions are proven by three independently-guarded destructive verbs (project delete, import-overwrite, resource delete) and a two-sided replace-not-merge e2e pin. Scope metadata names tag-provider exclusion in both export and import JSON (roadmap criterion 4). Live-gateway capture remains the documented opt-in path, with the harness gates already in place.

---

_Verified: 2026-08-22T09:05:00Z_
_Verifier: Claude (gsd-verifier)_
