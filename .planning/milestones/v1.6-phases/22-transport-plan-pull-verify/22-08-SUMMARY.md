---
phase: 22-transport-plan-pull-verify
plan: 08
subsystem: deploy
tags: [rust, pull, render-fixpoint, descriptor-relative-writes, guarded-verb, mcp]

requires:
  - phase: 22-transport-plan-pull-verify
    provides: TreeReader read path and for_node (22-01, 22-06), plan engine and unit_key (22-01, 22-03), deploy plan verb helpers (22-02), skip-reason vocabulary (22-03), tree::dirfd helpers (22-07)
provides:
  - "ign deploy pull --env <ENV> [--repo DIR]: bootstrap gateways/<name>/ from the live master, read-only against the gateway (DeployCommand::Pull, ActionOutput::DeployPull)"
  - "deploy/pull.rs: pull, PullInput, PullOutcome, Skipped, generate_manifest, PULL_MAX_ROUNDS and the REASON_* constants"
  - "layout::repo_path, the exact inverse of collection_path"
  - "actions::deploy: deploy_pull_preview, deploy_pull, DeployPullResult"
  - "deploy pull in GUARDED_OPS (// guarded:deploy pull) and the MCP tool deploy_pull with the optional confirm"
affects: [22-09, 22-10, 22-14, 23]

actuals:
  tokens: 24300
  tasks: 2
  commits: 3

tech-stack:
  added: []
  patterns:
    - "Render as the oracle: the pulled tree is rendered to a fixpoint, units the pipeline refuses or changes are dropped and named, so what is written is exactly what render reproduces"
    - "A finding is attributed to a unit by an index of full repo paths (exact or directory-prefix lookup via BTreeMap range); a secret location by trying every ': ' split point"
    - "Replace-then-write goes through tree::dirfd rooted at the canonicalized repo: removal by unlink_at/rmdir_at per recorded file, writes by ensure_walk + create_new"

key-files:
  created:
    - crates/ignition-core/src/deploy/pull.rs
    - crates/ignition-core/tests/deploy_pull.rs
  modified:
    - crates/ignition-core/src/deploy/layout.rs
    - crates/ignition-core/src/deploy/mod.rs
    - crates/ignition-core/src/actions/deploy.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/src/mcp.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-cli/tests/contract_deploy.rs
    - README.md

key-decisions:
  - "The overwrite guard counts only files in the three subtrees a pull replaces (resources/, tags/, modes/<env>/). A gateway directory holding only manifest.yaml and other environments' overlays replaces nothing and needs no --yes (see Deviations)"
  - "deploy_pull re-checks the guard itself (defence in depth for the main.rs gate), before the gateway is read and before any write"
  - "A tag provider or type name that is not a safe path component is left out with render_refused rather than breaking the generated manifest"
  - "Pull does not mark DPLN-06 complete: the round trip is proven offline over the local transport; the requirement says 'from a live gateway', which 22-09 and 22-14 capture"

patterns-established:
  - "Pull tests build ObservedFiles in memory for the engine and drive deploy_pull/load_declared/deploy_plan over a PlanFixture data dir for the round trip; a Counting TreeReader wrapper proves a refusal costs zero reads"

requirements-completed: []
requirements-advanced: [DPLN-06]

duration: 190min
completed: 2026-10-14
status: complete
---

# Phase 22 Plan 08: `ign deploy pull` Summary

**`ign deploy pull` bootstraps `gateways/<name>/` from the live master through the read-only `TreeReader` and keeps only units that `render` reproduces byte for byte, so pull then render then plan reports `has_changes: false`; every unit left out is named with a locked reason, and replacing a non-empty tree is `--yes`-guarded on the CLI and over MCP.**

## Accomplishments

- **Pull engine** (`deploy/pull.rs`, pure): groups `core` and mode files into units (`plan::unit_key`), places `core` under `resources/` and `tags/` and mode files under `modes/<env>/` (via `layout::repo_path`), then loops `render` up to `PULL_MAX_ROUNDS` (8) times. A secret location drops its unit as `embedded_secret`; any other finding drops the units it names as `render_refused: <finding message>`; once render succeeds each kept unit must render to exactly its effective live files (mode copy else core: same file set, same bytes) or it is dropped as `not_round_trippable`. A finding that names no unit (the manifest) is returned unchanged; a round that cannot make progress returns the error instead of looping.
- **`generate_manifest`**: byte-sorted rule-bearing live types, minus denied and tag types, plus the live provider names; it quotes a scalar only when a plain one would be misread (`true`, `123`). Parses with `Manifest::parse`.
- **`layout::repo_path`**: the inverse of `collection_path`; refuses short paths, a tag type with no provider/file, and an `ignition/tag-*` type that is not one of the four.
- **Action** (`actions/deploy.rs`): `deploy_pull_preview` (the `--yes` prose), `deploy_pull` (guard, manifest kept or generated, provider discovery read, main read, `pull`, then write). Never touches `TreeMirror`, `mirror_for_node`, `StagingMode`, `.stage(` or `.commit(`; `write_half_unreachable` stays green. The write is descriptor-relative on unix (`tree::dirfd`), path-based behind `#[cfg(not(unix))]`.
- **Surface**: `DeployCommand::Pull`, the dispatch arm with `// guarded:deploy pull` directly above its `require_confirmation`, the `GUARDED_OPS` row, `render_deploy_pull_human`, the `deploy pull` route row on `Screen::Projects`, README Commands row and `### \`deploy pull\``, MCP test `deploy_pull_is_a_guarded_catalog_tool`.

## `DeployPullResult` key order

`env, gateway, mode, node, transport, out, files_written, manifest, render_sha256, skipped, warnings`

`out` is `gateways/<name>`; `files_written` counts source files only (the manifest is not counted); `manifest` is `kept` or `generated`; `skipped` is `[{path, reason}]` byte-sorted by `path`.

## Skip reasons seen in the round-trip fixture

`pull_render_plan_round_trip` (a live gateway with a core-only resource, a singleton with a mode override, a mode-only resource, a tag provider with a folder, an Embedded-secret unit, a uuid-less named resource, a denied type and a rule-less type) produced exactly:

| path | reason |
|---|---|
| `com.example.unknown/thing` | `type_without_rule` |
| `ignition/api-token` | `denied_type` |
| `ignition/database-translator/NOUUID` | `not_round_trippable` |
| `ignition/database-translator/SECRET` | `embedded_secret` |

`render_refused: <finding>` is exercised by `fixpoint_converges` (a `bad:name` unit: the Windows-illegal character finding). The following `plan` reports the five written units `same` and `NOUUID` and `SECRET` `unmanaged`, `has_changes: false`.

## Task Commits

1. **Task 1: pull engine, `repo_path`, `generate_manifest`, fixpoint:** `8cd8a40` (feat)
2. **Task 2: `deploy pull` action, guard, surface, README, round-trip and canary tests:** `427c513` (feat)
3. **Post-task fix: resolve the repo root once so a symlinked `--repo` works:** `7d48d63` (fix)

(Plus the final docs commit with this summary.)

## Verification

- `cargo test --workspace`: 2025 passed, 0 failed (was 2001 before this plan). `deploy_pull` 18 (engine 10, action 8), `contract_deploy` 25 (3 new), `ignition-cli --bin ign` 48 (`deploy_pull_is_a_guarded_catalog_tool`, `guarded_ops_registry_tracks_every_dispatch_site`), `deploy_family_coverage` 4, `tui_coverage` 5, `deploy_transport` 59 (`write_half_unreachable`), `deploy_plan` unchanged with `PLAN_GOLDEN` and `PLAN_GOLDEN_LEDGER` untouched, `ignition-core --lib` 734 (`repo_path_inverts_collection_path` among them).
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` clean.
- Manual smoke with the freshly built debug binary (not the PATH `ign`, and not a `cargo install`): a non-empty tree without `--yes` exits 2 with the prose naming 4 files; with `--yes` it pulls; `plan` then reports `0 added, 0 changed, 0 removed, 1 same`.
- **Build environment note:** this machine's shared global `CARGO_TARGET_DIR` was held by another project's cargo runs (one build waited 18 minutes on the artifact lock). The later runs used a private `CARGO_TARGET_DIR=~/Library/Caches/cargo-target-ign-cli`. That directory is only for this checkout, so the worktree/target-dir hazard in the project CLAUDE.md does not apply.

## Edge coverage

| Edge | Evidence |
|---|---|
| DPLN-06 empty | `empty_live_tree` (engine), `pull_empty_live_tree` (action): a generated `types: []` manifest, no resource files, no `modes/<env>/`, `EMPTY_RENDER_SHA256`, plan with zero entries |
| DPLN-06 encoding | `layering` and `pull_render_plan_round_trip` (bytes and uuid kept exactly), `fixpoint_converges` (hostile live name skipped with a reason, not renamed); a non-UTF-8 live name fails the read as `transport_failed` in the transport (22-01/22-06) |
| Read-only on the host | `pull_render_plan_round_trip` and `pull_never_writes_secret` snapshot the data dir before and after; `write_half_unreachable` |
| `--yes` guard | `pull_refuses_non_empty_without_yes` (zero reader calls, repo byte-identical), `pull_yes_replaces_only_its_subtrees`, `pull_cli_overwrite_needs_yes` |
| Symlinks | `pull_refuses_symlink_in_replaced_subtree` (below `resources/`, as `resources/` itself, as the gateway dir; nothing removed, outside tree untouched), `pull_works_through_a_symlinked_repo_root` |
| Secrets | `embedded_unit_skipped`, `pull_never_writes_secret` (canary absent from every written file, stdout and stderr, human and `--json`) |

## Deviations from Plan

**1. [Judgement] The guard counts only what a pull replaces.** The plan says an existing non-empty `gateways/<name>/` needs `--yes`. The preview lists files in `resources/`, `tags/` and `modes/<env>/` only; a directory holding just `manifest.yaml` and other environments' overlays replaces nothing and needs no `--yes` (`pull_into_absent_and_empty_dirs_needs_no_yes` covers it). The truth's own prose ("the first five paths that would be replaced") and the "only three subtrees replaced" rule are the basis; an empty confirmation for zero files would be noise. Say so in review if the literal reading is wanted.

**2. [Rule 2 - Missing critical functionality] Unsafe type and provider names.** A live type or provider name that fails `component_problem` would have made the generated manifest invalid. It is left out with `render_refused: the type name ...` / `the tag provider name ...` and listed in `skipped`.

**3. [Rule 1 - Bug, found in my own test] Symlinked `--repo`.** The first version opened the repo root with `O_NOFOLLOW`, refusing a legitimately symlinked `--repo`. The root is now canonicalized once; components below it are still never followed (`7d48d63`).

**4. [Judgement] `repo_path` also refuses `ignition/tag-*` types that are not one of the four tag types**, so a future tag-like type fails closed (the unit is skipped as `render_refused`) instead of landing under `resources/` where render rejects it as a tag type.

**5. [Judgement] Writer lives in `actions/deploy.rs`.** The plan's file list has no `tree.rs`, so the replace/write helpers sit next to `deploy_pull` and use the existing `tree::dirfd` and `walk_capped` helpers rather than adding new ones.

**6. [Addition] Local-collection warning.** Units that also exist in the gateway's `local` collection (which pull does not read) are reported in `warnings`, as `plan` does.

**7. [Process] TDD.** As in 22-05 to 22-07, tests and implementation landed in the same task commit; there is no separate RED commit. All engine tests were written before `pull.rs` was run against them.

**Total deviations:** 1 bug (self-found), 1 Rule 2 addition, 4 judgement calls. No architectural change, no new packages.

## Known limitations and residuals

- **Plaintext warned-only content:** render only *warns* about a non-empty string under a `password`/`secret` key when the 8.3.6 OpenAPI describes the type (the schema is authoritative there). Pull writes such a unit and passes the value-free warning through in `warnings`; it is the render lint's rule, not a new gap.
- **UDT and root-level tags are skipped.** CD5 leaves nested `tag-type-definition` folders and root-level `tags.json` unexercised and fail-closed in `collection_path`; pull reports those units as `render_refused: ...`. A gateway whose tags live there pulls fewer tags than it holds until those layouts are captured.
- **Cost:** each round rebuilds the source tree and re-renders (at most 8 rounds), and a generated manifest needs one extra discovery read of `ignition/tag-provider`. Not measured at prod size; 22-14 runs the finished verb against the D-16 payload.
- **Windows:** the replace/write path is path-based behind `#[cfg(not(unix))]` (std has no descriptor-relative primitives), compiled but not run here, same status as the other 22-01/22-07 Windows branches. The repo root and its ancestors are the operator's own and are followed.
- **Not atomic:** the three subtrees are removed and then written; a failure in between leaves the repo partially written (git is the recovery). The engine work, the render and the symlink scan all happen before the first removal.
- **Live gateways:** no capture against a rig or WFLWVM01 (22-09, 22-12, 22-14). The ssh read path is the same `TreeReader` plan already uses; pull over ssh was not run here.
- **`cargo install` of the verb was not run;** the binary-level `assert_cmd` tests and a debug-binary smoke cover it.

## Requirement bookkeeping

DPLN-06 is **advanced, not marked complete**: the offline round trip is proven, but the requirement says "from a live gateway", and 22-09 and 22-14 (which also list DPLN-06) capture that. `REQUIREMENTS.md` is untouched.

## New packages

None. `Cargo.lock` unchanged.

## Known Stubs

None.

## Threat Flags

None beyond the plan's register. T-22-35 (secrets): render fixpoint plus canary tests over files, stdout and stderr. T-22-36 (overwrite): `confirmation_required` before any read or write, three subtrees only, `GUARDED_OPS` and MCP `confirm`. T-22-37 (symlinks): symlink scan before any removal, descriptor-relative `create_new` writes. T-22-38 (hostile names): render's lints and the fixpoint drop such units, `safe_component` re-checks at write. T-22-39 (MCP pull into an absent or empty target without `confirm`): accepted as planned and documented in the README.

## Self-Check: PASSED

- Files present: `crates/ignition-core/src/deploy/pull.rs`, `crates/ignition-core/tests/deploy_pull.rs`; `grep -n 'pub fn repo_path' crates/ignition-core/src/deploy/layout.rs` prints one line; `grep -n 'guarded:deploy pull' crates/ignition-cli/src/main.rs` prints one line; `grep -c '^### `deploy pull`' README.md` prints 1.
- Commits `8cd8a40`, `427c513`, `7d48d63` present in `git log`.
- No unwrap, TODO or placeholder in the new non-test code; `.planning/config.json` and `.planning/research/.cache/` untouched.
