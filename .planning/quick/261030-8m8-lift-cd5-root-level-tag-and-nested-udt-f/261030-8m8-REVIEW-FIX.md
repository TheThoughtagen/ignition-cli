---
phase: 261030-8m8-lift-cd5-root-level-tag-and-nested-udt-f
fixed_at: 2026-10-30T16:56:53Z
review_path: .planning/quick/261030-8m8-lift-cd5-root-level-tag-and-nested-udt-f/261030-8m8-REVIEW.md
iteration: 1
findings_in_scope: 8
fixed: 8
skipped: 0
status: all_fixed
---

# Quick 261030-8m8: Code Review Fix Report

**Fixed at:** 2026-10-30T16:56:53Z
**Source review:** .planning/quick/261030-8m8-lift-cd5-root-level-tag-and-nested-udt-f/261030-8m8-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 8 (CR-01, CR-02, WR-01 to WR-06). IN-01 to IN-06 were out of scope and left alone. The untracked `svc-ignition-deploy@52.154.75ace-uat4.ps1` (IN-06) was not touched or staged.
- Fixed: 8. WR-01 and WR-02 share one commit. CR-01 has a second commit that narrows the rule, on the coordinator's decision.
- Skipped: 0

Work ran on the main checkout, branch `fix/deploy-cd5-tag-layouts`, with no worktree (`workflow.use_worktrees=false`). Every gate below ran in the main checkout, so the numbers can be reproduced from this tree. Each code finding was test-first: the new test failed before the fix and passes after it.

## Fixed Issues

### CR-01: A tag unit that drops a live member file is not destructive

**Files modified:** `crates/ignition-core/src/deploy/plan.rs`, `crates/ignition-core/tests/deploy_plan.rs`, `README.md`
**Commits:** 86495d9 (first fix), 5883f1e (narrowed to the effective copy, coordinator decision)
**Status:** fixed: requires human verification (gate logic)
**Applied fix:** An `added` or `changed` tag-definition or tag-type-definition unit (`is_tag_folder`) is `destructive` with reason `tag_data_loss` only when its **effective** live copy (the mode copy when one exists, otherwise the core copy) holds a `tags.json` or `udts.json` (any letter case) that the rendered unit lacks. That is what the gateway runs now and would stop loading.

- On a first deploy the effective copy is the core copy, so core members about to be hidden are caught.
- Once a mode copy already omits those core members, nothing more is lost: the unit is `hidden` (informational, CR-02) but not destructive.
- A `same` unit is never flagged, because the apply does not touch it.

The first commit also counted every `hidden` relation as destructive. The coordinator chose the narrower rule, and 5883f1e removes that clause. The existing `--allow-destructive` gate applies; no new gate or slug was added. Each destructive tag entry adds one plan warning naming the unit and the dropped paths (never content). Warnings sit in `data.warnings`, so the envelope shape is unchanged and `plan_sha256` is unaffected.

Tests:

- `cr01_root_unit_dropping_a_live_member_is_destructive`: the provider-root mode orphan on a first deploy, with no ledger and with a ledger that omits the orphan; plus a keeps-every-member control.
- `cr01_judges_the_effective_copy_not_hidden_core`, written to fail before 5883f1e:
  - a nested `A/B` first deploy over core is destructive;
  - a later deploy whose mode copy already omits the core `udts.json` is `hidden` but not destructive, with no warning;
  - a `same` unit is not destructive;
  - the provider-root unit: a mode copy that already omits the core `udts.json` is not destructive, and one that still holds it is;
  - a non-tag control.
- `cr01_dropped_member_entry_needs_allow_destructive`: a real `evaluate_gates` refusal (`destructive_change_refused`, exit 2), and `Ready` with the flag.
- `cr02_first_deploy_over_core_is_hidden` also checks the destructive first deploy and its warning.

### CR-02: A first deploy of a tag unit over `core` reports `differs`, not `hidden`

**Files modified:** `crates/ignition-core/src/deploy/plan.rs`, `crates/ignition-core/tests/deploy_plan.rs`, `README.md`
**Commit:** d54d61b
**Status:** fixed: requires human verification (relation logic)
**Applied fix:** The `Hidden` arm now holds for a tag unit that has a mode copy **or is rendered** (`mode_copy.is_some() || wanted.is_some()`), when `hides(core, rendered)`. A first deploy over `core` with no mode copy is therefore `hidden`, as a later deploy is. Through CR-01 it is gated in the same way.

Note that such a unit's status is `changed`, not `added` as the review said, because the core copy is its effective live copy. `Hidden` feeds the confirmation prose (`[hidden]`) and the plan text (`core=hidden`). It is informational: a first deploy that hides core members is destructive through CR-01 because the effective copy is the core copy, not because of the `hidden` word. The `core` word is hashed, so `plan_sha256` changes for exactly those first-deploy tag units. None of `PLAN_GOLDEN`, `PLAN_GOLDEN_LEDGER` or `PLAN_GOLDEN_UNREADABLE` contains a tag unit, and all three are byte-identical and still pass. Controls cover a byte-only first deploy, which stays `differs` and is not destructive, and an identical render, which stays `same`/`identical`.

### WR-01: A member file in a folder with no marker is accepted

**Files modified:** `crates/ignition-core/src/deploy/render.rs`, `crates/ignition-core/tests/deploy_render.rs`, `crates/ignition-core/tests/deploy_pull.rs`, `crates/ignition-core/tests/deploy_verify.rs`, `README.md`
**Commit:** b2f78f6 (shared with WR-02)
**Applied fix:** `check_tag_markers` now walks every rendered file under the two marked tag types. It refuses a `tags.json` or `udts.json` whose own folder renders no `unary-resource.json`, as a `deploy_tree_invalid` finding naming the member's repo path (CD5c). Tests cover:

- `A/B/udts.json` under an `A/` marker only;
- a root `tags.json` with no root marker (the old README example);
- a folder `F/tags.json` with no marker.

Pull skips such a live unit as `render_refused`, and a new pull test covers this. The synthetic fixtures that relied on the leniency now carry gateway-shaped markers with `files` as CD5 captured it:

- `tags_map_to_collection_paths`, its overlay half, `overlay_repo`, and the `tags_json` control in `cd5_member_named_folder_refused` (`deploy_render.rs`);
- `tags_are_placed_under_tags` and `populate_live` (`deploy_pull.rs`);
- `with_tag_unit` and `with_cd5_units` (`deploy_verify.rs`).

The README now shows the provider-root marker in the `tags/default/tag-definition/tags.json` example and the folder marker in the `Plant` example. The "two deliberate gaps" text is replaced.

### WR-02: A marker with no `files` member, or one that is not a JSON object, skips the CD5c check

**Files modified:** as WR-01
**Commit:** b2f78f6
**Applied fix:** A marker that is not a JSON object (an array, a string or a number) is refused as "not a JSON object". An object with no `files` is refused as "`files` is missing", with or without member files beside it. Every gateway-written marker carries `files`, `[]` included, per the CD5 captures. Both refusals are fail-closed `deploy_tree_invalid` findings on the marker's repo path. A marker that does not parse is still reported only once, by the stage-5 parser. The new pull test checks that live units with a `files`-less marker and with an array marker are skipped as `render_refused`.

### WR-03: The member check is exact-case, and other files beside a marker are not checked

**Files modified:** `crates/ignition-core/src/deploy/render.rs`, `crates/ignition-core/tests/deploy_render.rs`, `crates/ignition-core/tests/deploy_pull.rs`, `README.md`
**Commit:** f8ed211
**Applied fix:** Every CD5 capture, on both hosts, lists only `tags.json`, `udts.json` and `unary-resource.json` in tag folders. Every file name under `tag-definition`/`tag-type-definition` in the fixtures and planning captures is one of the three; the only other name, `tags.json.tmp`, is the gateway's transient write. So the conditional refusal applies: render refuses any file below `tag-definition/<P>/` or `tag-type-definition/<P>/` whose name is not exactly one of the three.

A name that matches one of them regardless of case (`Tags.json`, `UDTS.JSON`, `Unary-Resource.json`) gets its own letter-case message, consistent with CD6 and the case-insensitive CD5d check. Other names (`notes.json`, `tags.json.bak`, `README.md`) get the "only tags.json, udts.json and its marker" message. A `files` entry in another letter case was already counted as an "other entry" and refused. `tag-provider` and `tag-group` files are not tag folders and are unaffected; a control test pins this. Pull skips such a live unit as `render_refused`.

### WR-04: CD5d only checks the rendered side

**Files modified:** `crates/ignition-core/src/deploy/render.rs`, `crates/ignition-core/src/deploy/plan.rs`, `crates/ignition-core/src/actions/deploy.rs`, `crates/ignition-core/tests/deploy_plan.rs`, `crates/ignition-core/tests/deploy_pull.rs`, `README.md`
**Commit:** 6aa92b3
**Applied fix:** The CD5d predicate is now the shared `render::member_named_tag_folder`, and render's own check uses it with unchanged behaviour. The new `plan::live_tag_folder_findings` scans the live `core` and mode paths, unreadable paths included. `deploy_plan` refuses with `deploy_tree_invalid` (exit 2, no new slug) while any tag folder named like a member file is live. There is one finding per folder, naming it under the data directory (`config/resources/<collection>/ignition/tag-definition/P/tags.json/`), with no content.

`deploy apply` plans through `deploy_plan` first, so it refuses before any write instead of failing on the commit rename. `deploy verify` calls the pure `plan()` and is not blocked, because it writes nothing. Pull already skipped such a live unit as `render_refused`, through render's CD5d check; a new test pins this for both collections. The plan test failed before the fix (the plan returned `unmanaged` for the clash) and passes after it. A `tags_json/` control still plans.

### WR-05: `capture-cd5.sh`'s LOCALHOST-ONLY guard does not cover the `ign` calls

**Files modified:** `crates/ignition-core/tests/fixtures/live-rig-deploy/capture-cd5.sh`
**Commit:** f85c59c
**Applied fix:** Right after `assert_local`, the driver unsets `IGNITION_URL`, `IGNITION_TOKEN`, `IGNITION_USER`, `IGNITION_PASSWORD`, `IGNITION_PROFILE` and `IGNITION_RIG`. The new `assert_rig` runs at the start of `run`, `seed` and `clean`, before any write or delete. It rewrites the throwaway profile from the checked `BASE` (`ign_cfg`). It then refuses (exit 2) unless `gateway-info` names the rig `ign-deploy-rig` (override: `IGN_DEPLOY_RIG_GATEWAY_NAME`), both through curl at `BASE` and through `ign --profile cd5-rig`. The header comment documents this. The driver was not run live.

### WR-06: `capture22w.sh` CD5-W has no gateway-identity guard, and its prose claims a setting the script never makes

**Files modified:** `crates/ignition-core/tests/fixtures/wflwvm01/capture22w.sh`
**Commit:** 5ee64ad
**Applied fix:** The new `cd5_guard` runs first in `cd5w` (before any section text or network call), `cd5wseed` and `cd5wclean`. It does three things in order:

1. It runs `ssh whoami` with the pinned host key, so a dynamic VM address that now belongs to another host stops the run before any API token is sent.
2. It exports `IGNITION_URL=http://HOST:8088`. The port is `IGN_WFLWVM01_GATEWAY_PORT` and is checked to be numeric, and an IPv6 address is bracketed. With this, the prose's P8 claim is true.
3. It refuses unless `gateway-info` names `Ignition-ignwin836` (override: `IGN_WFLWVM01_GATEWAY_NAME` or `IGN_WINVM_GATEWAY_NAME`).

The section prose now says that the driver sets the URL and checks identity, and it documents the cleartext HTTP exposure, as does the header comment. HTTPS is not available on the VM gateway (port 8088 is HTTP), so the exposure is documented rather than removed. The driver was not run live.

## Verification

All gates ran in the main checkout (`/Users/pmannion/whiskeyhouse/ignition-cli`), not a worktree, with the global `CARGO_TARGET_DIR` and a single checkout.

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings` | pass |
| `cargo test --workspace` | pass: 3030 passed, 0 failed, 91 ignored across 96 test binaries (baseline before the fixes: 3019 passed, 0 failed, 91 ignored; the 11 new tests account for the difference) |

The final gate ran after 5883f1e with `~/.cargo/bin` first on PATH (rustup stable rustc 1.99.1) for every step. Homebrew's rustc 1.94.1 sits ahead of `~/.cargo/bin` on this machine's default PATH and has no Windows target, so xwin clippy fails there with E0463 `can't find crate for core`. That is an environment issue, not a code one.
| `bash -n` on both drivers | pass |
| `shellcheck` on both drivers | no new findings. The pre-existing 29 (capture-cd5.sh) and 31 (capture22w.sh) notes and warnings are unchanged, compared message by message before and after. |

`PLAN_GOLDEN`, `PLAN_GOLDEN_LEDGER`, `PLAN_GOLDEN_UNREADABLE` and `RENDER_GOLDEN` are byte-identical, and their tests pass. `Cargo.toml` and `Cargo.lock` are unchanged. No live, network or production call was made.

## Notes for review

- The pure plan-engine unit tests in `deploy_plan.rs` (`tag_folders_are_units`, `hidden_tag_folder`, `destructive_reasons`) still feed `{}` markers straight to `plan()`. They never go through render and test grouping, not marker content, so they did not rely on the render leniency and were left unchanged.
- capture22w.sh's older subcommands (`preflight`, `c6`, `c6post`, `c5ntfs`) were not changed. They are outside the reviewed diff, and forcing `IGNITION_URL` there could break the Phase 22 procedure. The coordinator accepted this, and accepted leaving the pure plan tests' `{}` markers.

---

_Fixed: 2026-10-30T16:56:53Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
