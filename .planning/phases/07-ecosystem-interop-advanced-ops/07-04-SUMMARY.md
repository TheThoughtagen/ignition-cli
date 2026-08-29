---
phase: 07-ecosystem-interop-advanced-ops
plan: "04"
subsystem: interop
tags: [flint-codec, ignition-lint, delegation, tag-exports, offline-browsing, zip-surgery, span-splicing]

# Dependency graph
requires:
  - phase: 05-ecosystem-interop
    provides: "export-zip surgery helpers (client/resources.rs), the WebDev route seam, tags export interchange normalization"
  - phase: 07-ecosystem-interop-advanced-ops (07-01/07-02/07-03)
    provides: "export_zip_bytes shared seam, the rig-verb profile-null precedent, TUI row conventions"
provides:
  - "PURE Flint codec (client/scripts_codec): flint_encode/decode, dedent/reindent, SCRIPT_KEYS, span-level member decode/encode, tree wrappers with scripts-manifest.json"
  - "project export --decode-scripts / import --encode-scripts (the nvim/ignition-lint editing loop, byte-exact unedited round-trip)"
  - "ign lint: ignition-lint PATH delegation with the doctor posture + --strict passthrough + lint_tool_absent exit 6"
  - "tags browse --from-export: offline parsing of git-module directories, legacy single files, and the CLI interchange"
affects: [phase-7-verification, future-flint-tooling, tag-promotion-pipelines]

# Tech tracking
tech-stack:
  added: []  # zero new dependencies (plan mandate)
  patterns:
    - "Position-tracking raw-byte JSON scanner + span-level splicing (NO serde_json preserve_order, NO Value re-serialization — key-order stability of existing goldens preserved by construction)"
    - "Counter-named sidecars (<member>.<n>.py) + JSON-pointer manifest at the tree root; marker-free exported JSON"
    - "Doctor posture for external tool delegation (child ran = exit 0; findings/child_exit_code/report as data) with a sanctioned success-path strict exit exception decided AFTER the envelope renders"
    - "Serialized PATH-mutation test guard (edition-2024 unsafe set_var under a lifetime-held mutex) + builtin-only fake tools on an isolated PATH"

key-files:
  created:
    - crates/ignition-core/src/client/scripts_codec.rs
    - crates/ignition-core/src/actions/lint.rs
    - crates/ignition-core/tests/scripts_codec_contract.rs
    - crates/ignition-core/tests/lint_contract.rs
    - crates/ignition-cli/tests/contract_lint.rs
  modified:
    - crates/ignition-core/src/actions/projects.rs
    - crates/ignition-core/src/actions/tags.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-tui/src/state.rs
    - crates/ignition-tui/src/update.rs
    - crates/ignition-tui/src/workers/ops.rs
    - crates/ignition-cli/tests/contract_projects.rs
    - crates/ignition-cli/tests/contract_tags.rs
    - README.md

key-decisions:
  - "Span-level splice over raw member bytes, NOT serde_json preserve_order — the planner lock: feature unification is workspace-wide and would churn every Value-re-serializing golden; serde_json stays read-only in the codec"
  - "SCRIPT_KEYS is NINE keys (the dual-ported ignition-nvim source list) — the plan sketch's [&str; 10] was an off-by-one; source is the authority"
  - "Lint strict exit decided in main() AFTER render_ok via a serde(skip) strict_exit_code() — envelope prints first, then the child's masked (0..127) code; the one sanctioned success-path exit exception"
  - "decode heuristic = escape markers AND decodes multi-line (expressions pass through); whitespace-only script lines normalize to empty on reindent (the ignition-nvim semantics, accepted)"
  - "from-export provider = file stem for FILE inputs (the only derivable provenance); System provider excluded, .tag-config.json skipped, _types_ children default UdtType"

patterns-established:
  - "External-tool delegation contract: PATH discovery (first executable), arg-vector spawn, doctor-posture data shape {ran, tool, child_exit_code, issues_found, report, stdout, stderr_preview}"
  - "Offline command contract: short-circuit before profile/secret/client resolution, envelope profile null, offline errors are invalid_input exit 2"
  - "Decoded-export tree format: members + <member>.<n>.py siblings + scripts-manifest.json {version, members: {path: [{pointer, sidecar, indent_prefix}]}}"

# Metrics
duration: 122min
completed: 2026-08-29
---

# Phase 7 Plan 4: Interop Trio (scripts codec, lint delegation, offline tag browsing) Summary

**Flint script codec with byte-exact unedited decode→encode round-trip (span-spliced, no preserve_order), `ign lint` doctor-posture delegation to PATH-discovered ignition-lint with `--strict` CI passthrough, and offline `tags browse --from-export` parsing git-module/legacy/interchange layouts — zero new dependencies**

## Performance

- **Duration:** 122 min
- **Started:** 2026-08-29T03:12:00Z
- **Completed:** 2026-08-29T05:14:09Z
- **Tasks:** 3
- **Files modified:** 18

## Accomplishments
- INTR-01: `client/scripts_codec` (PURE) ports the ignition-nvim Flint codec exactly (backslash-first encode, single-pass decode, tab dedent/reindent) and adds a shared position-tracking JSON scanner so decode resolves and encode re-resolves script strings to raw byte spans — `project export --decode-scripts` writes the member tree + counter-named `.py` sidecars + JSON-pointer manifest; `import --encode-scripts` splices edits back and strips the manifest before the standard import path (validate_import rides free)
- INTR-02: `ign lint` delegates to ignition-lint via PATH discovery + tokio arg-vector spawn; findings are DATA (exit 0 whenever the child ran, full report in the envelope, `--strict` passes the child's code through after the envelope renders); absent tool = additive `lint_tool_absent` exit 6 with the uv/pip install hint; TUI dashboard row (clientless worker — the script-run twin)
- INTR-03: `tags browse --from-export` browses three export layouts fully offline (profile null, dead-URL-proven) into the EXISTING BrowseRow/tree/flat-JSON surfaces — git-module directories (individual-file format with `_types_`, `%XX` name decoding, System/dot-entry skips), legacy `<provider>.json` whole trees, and the CLI's own interchange files
- Phase 7 complete: all 8 requirements landed across 07-01..07-04

## Task Commits

Each task was committed atomically:

1. **Task 1: scripts_codec + decode/encode flags** - `075a518` (feat)
2. **Task 2: ign lint delegation + doctor posture + --strict** - `b2c88ef` (feat)
3. **Task 3: tags browse --from-export (three layouts)** - `55c2bfd` (feat)

**Plan metadata:** (this commit) — docs: complete plan

## Files Created/Modified
- `crates/ignition-core/src/client/scripts_codec.rs` - the PURE Flint codec + scanner + splice + tree wrappers (NEW)
- `crates/ignition-core/src/actions/lint.rs` - PATH discovery + arg-vector spawn + doctor posture (NEW)
- `crates/ignition-core/src/actions/projects.rs` - project_export_decoded action + rig export-body override
- `crates/ignition-core/src/actions/tags.rs` - browse_rows_from_export + the three-layout walkers + fs-name decoding
- `crates/ignition-core/src/error.rs` - LintToolAbsent variant (additive slug, exit 6) + enumerated test
- `crates/ignition-cli/src/cli.rs` - decode_scripts/encode_scripts flags, Lint leaf, from_export flag
- `crates/ignition-cli/src/main.rs` - export/import arms, lint dispatch + strict exit, from-export short-circuit
- `crates/ignition-cli/src/render.rs` - decoded-export/lint/from-export human renders + shared browse tree helper
- `crates/ignition-tui/src/{routes,state,update}.rs`, `workers/ops.rs` - lint row + input modal + clientless worker
- `crates/ignition-core/tests/scripts_codec_contract.rs` - the file-level sacred invariant (NEW)
- `crates/ignition-core/tests/lint_contract.rs` - fake-tool PATH-controlled contracts (NEW)
- `crates/ignition-cli/tests/contract_lint.rs` - binary goldens (NEW)
- `crates/ignition-cli/tests/contract_projects.rs` - decode/encode goldens + member-level upload honesty
- `crates/ignition-cli/tests/contract_tags.rs` - three-layout + profile-null + offline goldens
- `README.md` - "Script decode/encode", "Linting", "Browsing tag exports offline" sections + exit table + command rows

## Decisions Made
- Span-level byte splicing over enabling serde_json `preserve_order` (the planner lock — workspace feature unification would churn every Value-re-serializing golden; serde_json stays read-only in the codec, pinned by design comments)
- SCRIPT_KEYS is nine keys per the dual-ported ignition-nvim sources, not the plan sketch's ten (source-verified; documented at the const)
- Strict-mode lint exit decided in `main()` after `render_ok` via a `serde(skip)` field — the envelope always renders first; the code masks to the 0..127 shell range (plan's `code & 0x7f`)
- The lint worker is the first CLIENTLESS dashboard TUI worker (local delegation needs no gateway handle)
- File-input from-export provider = file stem (the only honest derivable provenance); the Provider-typed/empty-named wrapper lands children at the stem root (the 05-06 effective-top-level rule reused)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Test fixtures were not in the Flint table's image**
- **Found during:** Task 1 (codec unit tests)
- **Issue:** Initial fixtures used literal `'`/`<>&=` inside script strings; the Flint table escapes them (`\u0027`, `\u003c`…), so unedited round-trips were not byte-identical — a real gateway export escapes them and only table-image inputs can round-trip exactly
- **Fix:** Rewrote all fixtures in gateway-image form (documented at each fixture); behavior unchanged, tests now prove the real contract
- **Files modified:** scripts_codec.rs / projects.rs / scripts_codec_contract.rs / contract_projects.rs fixtures
- **Verification:** unedited round-trip byte-identity tests green
- **Committed in:** 075a518 (part of the task commit)

**2. [Rule 3 - Blocking] Edition-2024 env mutation in lint tests**
- **Found during:** Task 2 (lint contract)
- **Issue:** `std::env::set_var` is unsafe in edition 2024 and PATH-mutating tests raced in parallel; the first fake-tool harness also relied on external `cat` while the child inherits the ISOLATED tempdir PATH (tool produced no output)
- **Fix:** Lifetime-held mutex PATH guard (the codebase ENV_LOCK pattern, test-file edition) + sh-builtin-only fake tools carrying payloads via env vars
- **Files modified:** crates/ignition-core/tests/lint_contract.rs, crates/ignition-cli/tests/contract_lint.rs
- **Verification:** all 7 core + 5 binary lint tests green
- **Committed in:** b2c88ef (part of the task commit)

---

**Total deviations:** 2 auto-fixed (1 bug, 1 blocking)
**Impact on plan:** Both were test-harness correctness fixes; no scope creep. The plan's architecture (span splicing, doctor posture, offline contract) executed exactly as written.

## Issues Encountered
None beyond the auto-fixes above.

## User Setup Required

None - no external service configuration required. (ignition-lint is OPTIONAL: `ign lint` refuses with an actionable install hint when the tool is absent, by design.)

## Next Phase Readiness
- Phase 7 complete — all four plans shipped (diff/sync, backup+EAM, script run, interop trio); 8/8 requirements landed
- Next: `/gsd-verify-work 7` (the UAT pass), then phase transition
- Workspace state: 800+ tests green, fmt + clippy -D warnings clean, zero new dependencies, additive-only error taxonomy

---
*Phase: 07-ecosystem-interop-advanced-ops*
*Completed: 2026-08-29*

## Self-Check: PASSED

All created files exist on disk; all three task commits (075a518, b2c88ef, 55c2bfd) present in git log; full workspace suite 47/47 test results ok, zero failures.
