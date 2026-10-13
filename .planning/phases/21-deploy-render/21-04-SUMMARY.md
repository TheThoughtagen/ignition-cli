---
phase: 21-deploy-render
plan: 04
subsystem: deploy
tags: [render, sha256, tracer, clap, tui-route, mcp, readme, family-coverage]

requires:
  - phase: 21-03
    provides: strict YAML layer, Manifest/Target schemas, deploy_tree_invalid and embedded_secret slugs
provides:
  - "ign deploy render --env <ENV> [--repo DIR]: offline verb, profile null, no Session, no network"
  - "deploy::{layout,tree,render} pipeline (collect, map, allowlist, finish) with named stages later plans insert into"
  - "render_sha256 listing format (path NUL hex-sha256 LF, byte-sorted) and the empty-render digest, documented in README"
  - "actions::deploy::deploy_render and DeployRenderResult (12 data keys)"
  - "deploy_family_coverage.rs: data-driven README row, exit-table slug and TUI route coverage for the deploy family"
  - "tracer fixture repo and pinned golden digest"
affects: [21-05, 21-06, 21-07, 21-08, 21-09, 22]

tech-stack:
  added: []
  patterns:
    - "BTreeMap/BTreeSet only under deploy/; directory entries sorted by name bytes before recursing"
    - "Offline envelope verb dispatches (None, result) like Commands::Lint"
    - "Family coverage test walks Cli::command() and collects every miss"

key-files:
  created:
    - crates/ignition-core/src/deploy/layout.rs
    - crates/ignition-core/src/deploy/tree.rs
    - crates/ignition-core/src/deploy/render.rs
    - crates/ignition-core/src/actions/deploy.rs
    - crates/ignition-core/tests/deploy_render.rs
    - crates/ignition-core/tests/fixtures/deploy/repos/tracer/
    - crates/ignition-cli/tests/contract_deploy.rs
    - crates/ignition-cli/tests/deploy_family_coverage.rs
  modified:
    - crates/ignition-core/src/deploy/mod.rs
    - crates/ignition-core/src/actions/mod.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/src/mcp.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-cli/tests/contract_stdout_purity.rs
    - README.md

key-decisions:
  - "render_sha256 listing is `<collection path> NUL <lowercase hex sha256 of file bytes> LF`, concatenated in UTF-8 byte order of the collection path. Consumed by Phase 22 (plan, --base); changing it means re-goldening every consumer"
  - "deploy_render is exposed over MCP by clap derivation, unguarded, with no confirm property (Decision D); pinned by deploy_render_is_an_unguarded_catalog_tool"
  - "The deploy family coverage test matches a README row only when `ign deploy L` is followed by a space or a backtick, and reports misses as `deploy <leaf>`"

patterns-established:
  - "Render stages are private functions listed in the render.rs module doc, so 21-05..21-09 insert at a named stage"
  - "SourceTree::read_gateway findings are collected and raised once as one deploy_tree_invalid"

requirements-completed: [DSURF-01]
requirements-partial: "DRND-01 and DRND-04 stay Pending: see Requirement tracking"

actuals:
  tokens: 18000
  tasks: 2
  commits: 2

duration: 60min
completed: 2026-10-13
status: complete
---

# Phase 21 Plan 04: `ign deploy render` tracer and deploy family surface Summary

**`ign deploy render --env dev` renders a real gateway tree offline through every layer (strict YAML, tree read, D-02 allowlist, digest, action, clap, envelope, TUI row, README, MCP) with a `render_sha256` golden cross-checked by an independent shell oracle.**

## Accomplishments

- Tracer (Task 1, `e731b4a`): the leading end-to-end slice. `render_sha256` for the tracer repo is `7c8b41b16aa7680fac4e3cc1b4ac0ba3031973a720f33a625079e408f5b376a1` (4 files, 1763 bytes, resources `ignition/database-translator/POSTGRES` then the `general-alarm-settings` singleton). The tracer gate was approved by the user, and the orchestrator re-ran the verify: `--env nope` exits 2 `deploy_tree_invalid` on stderr with empty stdout.
- README (Task 2): new `## Declared gateway deploy (\`ign deploy\`)` section before `## Linting`, with the repo layout, `--env` resolution, the manifest and target field reference, the D-02 refusal, the exact listing format, the data keys, the empty-render digest and the MCP note. The Commands row landed in Task 1 because the must_haves require it with the clap leaf.
- `deploy_family_coverage.rs` (4 tests, 3 under `--no-default-features`) walks the live clap tree and fails when `deploy` is missing or has no leaves.
- Two `contract_stdout_purity.rs` tests, the MCP catalog test and seven core determinism tests (`crates/ignition-core/tests/deploy_render.rs`).

## Shell oracle for the golden

Independent of the Rust digest code, run in `crates/ignition-core/tests/fixtures/deploy/repos/tracer/gateways/tracer-gw/resources`:

```sh
for p in $(find ignition -type f | LC_ALL=C sort); do
  printf '%s\0%s\n' "$p" "$(shasum -a 256 "$p" | cut -d' ' -f1)"
done | shasum -a 256
```

Output: `7c8b41b16aa7680fac4e3cc1b4ac0ba3031973a720f33a625079e408f5b376a1`, equal to `RENDER_GOLDEN` in `contract_deploy.rs`.

## Task Commits

1. Task 1 tracer, `ign deploy render` end to end: `e731b4a`
2. Task 2 README section, family coverage, purity, MCP, determinism tests: `b1ec4e4`

## Requirement tracking

- **DSURF-01: complete.** The first deploy verb (`render`) has its clap leaf, `routes()` row (Screen::Projects), README Commands row and the exit-table slugs, and the family coverage test exists and is green. REQUIREMENTS.md assigns DSURF-01 to close in Phase 21 with the first verb and that test. The four-place error rule was met in 21-03. 21-09 lists it again for the `--out` flag, and Phases 22-25 keep the coverage test green.
- **DRND-01: stays Pending.** The digest is identical for identical inputs, but the requirement says "get a rendered tree". Writing the tree to disk (`--out`) lands in 21-09, and overlay and tags in 21-06 change what a rendered tree holds.
- **DRND-04: stays Pending.** Only the manifest allowlist half is delivered. The refusal of security singletons (deny list, gateway-owned resources) lands in 21-06.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Stdout-purity test consumed the trailing newline before measuring it**
- **Found during:** Task 2 (`cargo test --workspace`)
- **Issue:** `deploy_render_json_stdout_is_the_envelope_only` called `stream.next()` a second time before `byte_offset()`; the second poll skips whitespace, so the offset passed the newline and the "only a newline follows" assertion failed. A test defect, not a product one.
- **Fix:** capture `byte_offset()` before the second poll.
- **Files modified:** crates/ignition-cli/tests/contract_stdout_purity.rs
- **Commit:** b1ec4e4

**2. [Rule 1 - Bug] Coverage-test failure message named `render`, not `deploy render`**
- **Found during:** Task 2 (the "remove the README row" acceptance check)
- **Issue:** the failure listed the bare leaf path, but the plan says it must name `deploy render`.
- **Fix:** misses are reported as `deploy <leaf>`. Re-ran the check: removing the README row now fails naming `"deploy render"`; the row was restored and is not in the commit.
- **Files modified:** crates/ignition-cli/tests/deploy_family_coverage.rs
- **Commit:** b1ec4e4

**3. Plan counting note.** The plan's test table puts `leaf_matching_requires_a_word_boundary` nowhere, and the acceptance wants exactly 4 tests (3 under `--no-default-features`). The adjacency edge is therefore asserted inside `every_deploy_leaf_has_a_readme_commands_row` instead of as a fifth test.

## Determinism grep

The plan's acceptance grep (`HashMap|HashSet|SystemTime|Instant::now|thread_rng|getrandom` over `crates/ignition-core/src/deploy/` and `actions/deploy.rs`) matches exactly one line: `crates/ignition-core/src/deploy/yamlio.rs:448` `Instant::now()`, inside the 21-03 test `alias_bomb_is_refused` (a test-only timeout guard). Production code is clean.

## Known Stubs

None. `out` serializes as null, and `tag_providers`, `uuids_minted` and `opaque_files` are empty arrays until 21-06, 21-07 and 21-05; these are documented additive-key placeholders in the contract, not UI stubs.

## Threat Flags

None. The plan's threat model (T-21-14..T-21-18) is covered: symlinks refused via `symlink_metadata`, `--env` checked with `component_problem` before any path join, no Session or credential read, ordered collections only, read-only render (`render_is_read_only`).

## Verification

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`, `cargo build -p ignition-cli --no-default-features`: clean
- `cargo test --workspace`: all green, 0 failures
- `deploy_family_coverage` 4 pass (3 with `--no-default-features`); `deploy_render` 7 pass; `contract_stdout_purity` 8 pass; MCP test passes

## Self-Check: PASSED

Files verified present: deploy_family_coverage.rs, deploy_render.rs, the README section, contract_deploy.rs. Commits `e731b4a` and `b1ec4e4` exist in `git log`.
