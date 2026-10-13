---
phase: 21-deploy-render
plan: 06
subsystem: deploy
tags: [render, overlay, tags, policy, deny-list, cd5]

requires:
  - phase: 21-02
    provides: decision CD5 (tags/<P>/<type>/<rest> to ignition/<type>/<P>/<rest>) and its not-exercised list
  - phase: 21-05
    provides: render stage 5 and the origin map that findings use
provides:
  - "modes/<env>/ overlay (render stage 2): whole-file replace or add, selected environment only"
  - "tags/ layout mapping per CD5 with TAG_TYPES, injective collection mapping"
  - "deploy::rules: DENIED_TYPES, GATEWAY_OWNED_FILES, is_denied"
  - "render policy refusals: denied types (manifest and tree), config-mode.json, dot-names, undeclared tag providers, tag types under resources/"
  - "README subsections Environments and overlays, Tags layout, What render refuses"
affects: [21-07, 21-08, 21-09]

tech-stack:
  added: []
  patterns:
    - "Overlay keys travel in SourceTree under their full gateway-relative path (modes/<env>/...); stage 2 resolves them, so the reader stays free of render logic"
    - "Each stage drops an entry after reporting it, so one defect yields one finding"

key-files:
  created:
    - crates/ignition-core/src/deploy/rules.rs
  modified:
    - crates/ignition-core/src/deploy/mod.rs
    - crates/ignition-core/src/deploy/layout.rs
    - crates/ignition-core/src/deploy/tree.rs
    - crates/ignition-core/src/deploy/render.rs
    - crates/ignition-core/tests/deploy_render.rs
    - README.md

key-decisions:
  - "CD5 equals the RESEARCH guess, so the mapping is ignition/<type>/<P>/<rest>; the doc comment cites CD5"
  - "The three layouts CD5 did not exercise stay fail-closed: root-level tags.json/udts.json of tag-definition, folders nested under tag-type-definition, non-core collection"
  - "Tag types are not allowlisted through manifest types; they come from tags/ only, and resources/ignition/<tag type> is refused"
  - "A modes/<other> entry is skipped unopened; only a non-directory entry directly under modes/ is a finding"
  - "denied_types_exist_in_openapi_extract confirmed all four spellings (security-properties, security-levels, api-token, user-source); no change from the plan"

patterns-established:
  - "Dot-name screening happens at read time (tree.rs) and again on the in-memory tree (render.rs), so hand-built SourceTrees get the same policy as disk reads"

requirements-completed: [DRND-03, DRND-04]

actuals:
  tokens: 15200
  tasks: 2
  commits: 2

duration: 45min
completed: 2026-10-13
status: complete
---

# Phase 21 Plan 06: Overlays, tags layout and policy refusals Summary

**`modes/<env>/` overlays replace or add whole files for the selected environment only, `tags/<P>/<type>/<rest>` maps to `ignition/<type>/<P>/<rest>` per CD5, and the security singletons, `config-mode.json`, dot-names, undeclared providers and colliding sources are refused fail-closed with every finding reported together.**

## Accomplishments

- **Reader (`tree.rs`)**: recognises `tags/` and `modes/`; opens only `modes/<env>/` (other environments are skipped without being opened, so their invalid content cannot affect a render); accepts `resources/` and `tags/` under the overlay; refuses dot-named entries at read time and does not walk a dot-named directory; skips a regular `.gitkeep` at any depth. `component_problem` and module names with dots (`com.inductiveautomation.perspective`) are unaffected because only the first character decides.
- **Layout (`layout.rs`)**: `TAG_TYPES`, `tag_provider`, and `collection_path` for `tags/<P>/<type>/<rest>` with the CD5 doc comment and its fail-closed list.
- **Pipeline (`render.rs`)**: stage 2 overlay (origin prefix kept so findings name `gateways/<g>/modes/<env>/...`), name screening (dot-names, gateway-owned files), stage 3 injective mapping naming both sources on a collision, stage 4 policy (allowlist D-02, tag types under `resources/`, deny list on the tree and on manifest `types`, undeclared providers, one finding per type directory or provider). `tag_providers` in the outcome lists providers that rendered at least one file, byte-sorted.
- **Rules (`rules.rs`)**: `DENIED_TYPES`, `GATEWAY_OWNED_FILES`, `is_denied`.
- **README**: `#### Environments and overlays` (including the SF-7 "cannot remove a base resource" limitation), `#### Tags layout`, `#### What render refuses`.

## Task Commits

1. Task 1, overlay, tags layout, injectivity: `e5a2fc2`
2. Task 2, policy refusals, dot-names, `.gitkeep`: `c28c296`

## Requirement tracking

- **DRND-03** (files under `modes/<env>/` override the base by path, whole-file): fully delivered and tested (`overlay_replaces_whole_file`, `overlay_adds_new_file`, `other_env_overlays_are_never_read`, `overlay_render_is_read_only_and_repeatable`). Marked complete.
- **DRND-04** (only allowlisted types rendered; security singletons refused): fully delivered (D-02 from 21-04, deny list here, tested with `each_denied_type_is_refused_even_when_allowlisted` and the extract spelling check). Marked complete.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] Dot-name and gateway-owned screening also runs on the in-memory tree**
- **Issue:** the plan places dot-name refusal at read time only, but `render` is also called on hand-built `SourceTree`s (and by the action for any future reader), which would skip the policy entirely.
- **Fix:** `screen_names` in render stage 4 applies the same rule (dot-name except a `.gitkeep` file, plus `GATEWAY_OWNED_FILES`) so the policy cannot be bypassed by the input source. A hostile directory is reported once.
- **Files:** `render.rs`. **Commit:** `e5a2fc2`, tests in `c28c296`.

**2. TDD note.** Tests and implementation for each task were written in the same working session and committed together as `feat` (the tests need the new API to compile); each test file section was executed against the implementation before the commit. No separate failing-test commits.

**3. Scope.** The plan's Task 1 file list did not include `rules.rs` or `mod.rs`, but the tag-type and denial constants were used by the stage 4 code written in Task 1, so `rules.rs` landed in the first commit. Task 2 then added the dot-name reader changes and the tests.

## Issues Encountered

- First `cargo build` after editing ran into the shared target directory lock for several minutes (other build in flight); no code issue.
- A first attempt to drop `.gitkeep` from a heredoc using `sed -i` failed on BSD sed; redone with a Python edit.

## Known Stubs

None.

## Threat Flags

None. T-21-23 (deny list on manifest and tree, spelling pinned to the OpenAPI extract), T-21-24 (`config-mode.json` at any depth, base, overlay and tags), T-21-25 (`other_env_overlays_are_never_read` with broken JSON, a dot-name and a symlink in another environment), T-21-26 (injective mapping, `colliding_sources_are_refused`) and T-21-27 (dot-name refusal, `.gitkeep` skipped) are covered. Findings carry paths and key names only, never values.

## Verification

- `cargo test -p ignition-core --test deploy_render`: 23 passed (8 pre-existing, 9 from Task 1, 6 from Task 2)
- `cargo test -p ignition-core --lib deploy::`: 51 passed
- `cargo test -p ignition-core --test deploy_corpus`: 4 passed unchanged
- `cargo test -p ignition-cli --test contract_deploy`: 5 passed, `render_digest_golden` unchanged
- `cargo test --workspace`: no failures
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`, `cargo build -p ignition-cli --no-default-features`: clean
- Determinism grep over `deploy/`: only `yamlio.rs:448` (21-03 test-only guard)
- `grep -oE '"(security-properties|security-levels|api-token|user-source)"' rules.rs | sort -u | wc -l`: 4
- `grep -c 'Environments and overlays' README.md`: 1; `grep -n CD5 layout.rs`: present

## Self-Check: PASSED

- FOUND: crates/ignition-core/src/deploy/rules.rs
- FOUND commits: e5a2fc2, c28c296
