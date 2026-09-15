---
phase: 13-composite-engine-workspace-historian-edit
plan: 03
subsystem: core-library
tags: [workspace, checkout, manifest, scripts-codec, byte-exact, round-trip, rfc3339, clobber-guard]

# Dependency graph
requires:
  - phase: 13-02
    provides: build_mapping / local_path_for (injective hostile-name-safe mapping, proptest-pinned) and the resources.rs member engine (resource_members/read_member/member_hashes, descriptor-normalized)
  - phase: 07-04 (scripts_codec)
    provides: decode_member / Manifest / encode_export_tree — the codec leg checkout rides for --decode-scripts
provides:
  - actions/workspace.rs WorkspaceManifest + ManifestMember (`.ign-workspace.json`) with strict read ownership — stable refusal message prefixes (13-07 golden anchors)
  - read_manifest/write_manifest (pretty, trailing newline, 0640, deterministic serialization)
  - workspace_checkout — export → build_mapping ONCE → mapped tree write → manifest records gateway↔local pairs + zip-side descriptor-normalized hashes → idempotent .gitignore (the manifest is RECORDED; 13-06 status/push never re-derive)
  - --decode-scripts checkout: sidecars at mapped siblings + codec manifest keyed by mapped tree-relative paths — encode_export_tree consumes the tree, UNEDITED re-encode is BYTE-EXACT per member (codec invariant at tree scale)
  - clobber-safe refusals (unrelated non-empty dir / corrupt / foreign manifest naming both projects) + same-project re-checkout refresh semantics
  - tests/workspace_checkout.rs — 9-test contract suite (byte-exact round-trip, exact-set tag exclusion, descriptor-volatility hash pin, edit-splice isolation, refusal shapes)
affects: [13-06 status/push (consume the recorded manifest), 13-07 workspace CLI (envelope + golden anchors), edit slice (codec leg proven at tree scale)]

# Tech tracking
tech-stack:
  added: [] # zero new dependencies — RFC3339 timestamp hand-rolled (civil-from-days)
  patterns: [recorded-not-recomputed checkout manifest, mapped-path checkout (mapping IS the fs truth), codec-composition over tree-glue, clobber-gate before any write, stable refusal prefixes as golden anchors]

key-files:
  created:
    - crates/ignition-core/tests/workspace_checkout.rs
  modified:
    - crates/ignition-core/src/actions/workspace.rs
    - crates/ignition-core/src/actions/mod.rs

key-decisions:
  - "checkout composes the codec's OWN pub primitives (decode_member/Manifest/MANIFEST_NAME) instead of calling decode_export_tree — the codec's tree wrapper writes RAW zip member names, which contradicts the locked 'never path.join on the raw member name — the mapping IS the fs truth' invariant; no new script extraction exists (the codec's scanner/decode engine stays single-source)"
  - "codec manifest (scripts-manifest.json) keys members by their MAPPED tree-relative paths — the exact key encode_export_tree resolves when walking the tree, which is what makes the unedited re-encode byte-exact"
  - "profile rides workspace_checkout as an explicit parameter (plan signature omitted it; the planner-locked manifest requires the field and the action layer cannot read config — 13-07 passes the resolved profile down)"
  - "checkout's .gitignore is append-idempotent: existing content preserved, missing entries appended, second call a no-op"
  - "reserved root names (.ign-workspace.json, scripts-manifest.json, .gitignore, project.json) refuse member shadowing — the checkout owns the files it writes"
  - "no project.json in the tree: it is not a resource member (resource_members skips it), no consumer needs it (push splices into a fresh export), and the exclusion-pin inventory does not include it"

patterns-established:
  - "Recorded manifest as the workspace identity: build_mapping runs ONCE at checkout; status/push read the recorded pairs+hashes, never re-derive (Pitfall W1 enforced structurally)"
  - "Clobber gate precedes any write: existing target must be empty or a valid same-project workspace — corrupt/foreign manifests refuse with folded reasons naming what was found"
  - "Stable refusal message prefixes are contract (13-07 golden anchors): 'not an ign workspace — run `ign workspace checkout` first', 'is not valid JSON', 'not empty and is not an ign workspace', 'schema_version N (expected 1)'"

# Metrics
duration: 31min
completed: 2026-09-15
---

# Phase 13 Plan 03: Workspace Manifest + Checkout Summary

**`.ign-workspace.json` manifest + checkout action: mapped tree with recorded gateway↔local pairs and descriptor-normalized hashes, --decode-scripts proven BYTE-EXACT at tree scale through the codec's own encode, tag-value exclusion structurally pinned, clobber-safe refusals — SC-1's checkout half and SC-2's decode-scripts + exclusion clauses code-complete pending CLI.**

## Performance

- **Duration:** 31 min
- **Started:** 2026-09-15T02:25:19Z
- **Completed:** 2026-09-15T02:56:48Z
- **Tasks:** 2
- **Files modified:** 3 (1 created, 2 modified)

## Accomplishments
- The workspace identity exists in strict form: `WorkspaceManifest` (schema_version/project/profile/checked_out_at/members) with deterministic BTreeMap serialization, 0640 perms, and read ownership that refuses missing/foreign-schema/corrupt manifests with STABLE message prefixes — never a panic, never a silent default
- `workspace_checkout` rides the one export seam (`export_zip_bytes` → exactly ONE GET, ZERO imports), maps members through 13-02's `build_mapping` exactly once, writes every member at its MAPPED local path, records pairs + zip-side descriptor-normalized hashes, and writes an append-idempotent `.gitignore` (manifest committed; codec artifacts ignored)
- `--decode-scripts` produces nvim-editable sidecars + codec manifest at mapped paths, and the crown test proves the UNEDITED tree re-encodes through `encode_export_tree` with EVERY member byte-identical (`assert_eq!` on raw `Vec<u8>` — not hashes, not structure) — the codec's sacred invariant at tree scale, the roadmap's precondition for `ign edit`
- SC-2's exclusion clause is structural: the exact-set tree pin (members + workspace files + codec artifacts, NOTHING else) holds because checkout ingests only the project export zip; descriptor-volatility is pinned live through the manifest (two exports differing only in lastModification record IDENTICAL hashes; a real content change moves the hash)
- Clobber safety: unrelated non-empty target refuses before ANY write (pre-existing file untouched, verified), foreign manifest refuses naming both projects, corrupt manifest refuses, 13-02's case-collision refusal propagates verbatim naming BOTH members with the target never created

## Task Commits

Each task was committed atomically:

1. **Task 1: WorkspaceManifest model + read/write with strict ownership rules** - `d2f7052` (feat)
2. **Task 2: checkout action — export → map → write tree → record manifest (+ decode-scripts, .gitignore, refusals)** - `2c93d9b` (feat)

## Files Created/Modified
- `crates/ignition-core/src/actions/workspace.rs` (modified from Task 1's creation) — manifest model + read/write + checkout action + RFC3339 formatter + 9 unit tests (manifest refusals, round-trip/determinism, gitignore idempotence, clobber gate, timestamp pins)
- `crates/ignition-core/src/actions/mod.rs` — `pub mod workspace;` (alphabetical)
- `crates/ignition-core/tests/workspace_checkout.rs` (created) — 9-test contract suite over wiremock-served real exports: byte-exact round-trip (the crown), decode vs plain tree shapes, exact-set exclusion pin, descriptor-volatility hash pin, edit-splice isolation, all refusal shapes, single-export/zero-import wire pin

## Decisions Made
- checkout composes the codec's OWN pub primitives (`decode_member`, `Manifest`, `MANIFEST_NAME`) rather than calling `decode_export_tree`: that function writes members at RAW zip member names (`X/resources/Y`), which contradicts the plan's own locked invariant ("never path.join on the raw member name — the mapping IS the fs truth") and would break `MemberSource::Tree` reads in 13-06. No new script extraction exists — the codec's scanner/decode/dedent engine is used verbatim; only the tree-glue (which files land where) is mapping-shaped
- The codec manifest keys members by their MAPPED tree-relative paths — the exact key `encode_export_tree` resolves from its walk, making the unedited re-encode byte-exact and the sidecar strip-set exact
- `profile` is an explicit `workspace_checkout` parameter (plan's signature omitted it; the planner-locked manifest field requires it and actions cannot read config/secret state)
- `project.json` is deliberately NOT written into the tree: not a resource member, no downstream consumer (push splices into a fresh export), and it is absent from the plan's exclusion-pin inventory
- Reserved-root-name refusal added (`.gitignore`/`scripts-manifest.json`/`.ign-workspace.json`/`project.json`) — a root-level member mapping onto a workspace-owned name would shadow machinery; refuses naming the member

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] decode_export_tree cannot express the mapped checkout layout**
- **Found during:** Task 2 (checkout implementation)
- **Issue:** The plan's key_link pins `decode_export_tree` as the decode leg, but that function writes the tree at RAW zip member names — checkout's locked invariant requires members at MAPPED local paths ("the mapping IS the fs truth"); a verbatim call would desync the recorded manifest from disk and break 13-06's `MemberSource::Tree` reads
- **Fix:** checkout calls the codec's own `decode_member` per member (basename-only dependence — identical for user and raw paths), assembles the codec's `Manifest` keyed by mapped tree-relative paths, and writes sidecars at mapped siblings; mapped-namespace shadow refusals pin the guard `decode_export_tree` would have provided
- **Files modified:** crates/ignition-core/src/actions/workspace.rs
- **Verification:** unedited round-trip test is byte-exact through `encode_export_tree`; shadow-collision class covered by the exact-set pin + reserved-name refusals
- **Committed in:** 2c93d9b (Task 2 commit)

**2. [Rule 1 - Bug] Plan signature lacked the profile the locked manifest requires**
- **Found during:** Task 2 (checkout signature)
- **Issue:** `WorkspaceManifest.profile` is planner-locked, but the plan's `workspace_checkout(api, project, target_dir, decode_scripts)` signature has no profile source, and the action layer cannot read config (ARCHITECTURE.md layering)
- **Fix:** added `profile: &str` parameter; 13-07 passes the resolved profile name down
- **Files modified:** crates/ignition-core/src/actions/workspace.rs
- **Verification:** manifest round-trip pins the field; integration tests record "test-profile"
- **Committed in:** 2c93d9b (Task 2 commit)

**3. [Rule 2 - Missing Critical] Reserved-root-name shadow guard**
- **Found during:** Task 2 (checkout write loop)
- **Issue:** a member whose mapped local path is a root-level name the checkout itself writes (`.gitignore`, `scripts-manifest.json`, `.ign-workspace.json`, `project.json`) would shadow — or be shadowed by — workspace machinery (the same silent-loss class decode_export_tree guards at raw-name level)
- **Fix:** refuse at mapping time, naming the member and the reserved file
- **Files modified:** crates/ignition-core/src/actions/workspace.rs
- **Verification:** unit-level gate tests; refusal rides invalid_input exit 2
- **Committed in:** 2c93d9b (Task 2 commit)

---

**Total deviations:** 3 auto-fixed (1 blocking, 1 bug, 1 missing-critical)
**Impact on plan:** All three preserve the plan's invariants exactly; the decode-leg composition is the only architectural nuance and it is grep-verifiable (no new codec logic in actions/ — `decode_member`/`Manifest` are the codec's own pub surface). No scope creep.

## Issues Encountered
- serde_json object keys serialize sorted (no `preserve_order`) — the outcome-envelope smoke test compares sorted key sets, matching the codebase's no-`preserve_order` discipline
- Integration-test round-trip assertions initially used raw zip member names where the re-zip carries MAPPED (user) names — test-side fix, the implementation was correct

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness
- 13-06 status/push consume the RECORDED manifest (pairs + hashes): `read_manifest` at any workspace root, three-way compare = manifest vs `MemberSource::Tree` (note: Tree's strict walk refuses sidecars/manifests/.gitignore — status filters at the action layer, as 13-02 planned) vs fresh `MemberSource::Zip`
- 13-07 CLI wires `workspace checkout` with `--decode-scripts`; the stable refusal prefixes in `read_manifest`/`ensure_recheckout_safe` are its golden anchors; `CheckoutOutcome` is the additive envelope shape
- The byte-exact round-trip at tree scale is the green light for `ign edit` riding the same codec leg (roadmap ordering satisfied)
- No blockers

---
*Phase: 13-composite-engine-workspace-historian-edit*
*Completed: 2026-09-15*

## Self-Check: PASSED

- Files verified on disk: actions/workspace.rs, tests/workspace_checkout.rs, 13-03-SUMMARY.md
- Commits verified in history: d2f7052 (Task 1), 2c93d9b (Task 2)
- Byte-exact assert cited: `assert_eq!(actual, expected, "BYTE-EXACT member {user}")` in `unedited_decode_checkout_round_trips_byte_exact`
- Final verification battery re-run green: cargo test -p ignition-core (654 passed, 0 failed — 13-02 proptest suite + 13-03's 18 new tests + all pre-existing pins), workspace clippy -D warnings clean, fmt --check clean, no route/bundle/slug changes, no format-specific readers in action code
