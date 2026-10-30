---
phase: quick-261030-8m8
verified: 2026-10-30T16:30:00Z
status: passed
score: 8/8 must-haves verified
behavior_unverified: 0
overrides_applied: 0
---

# Quick 261030-8m8: Lift the CD5 refusals of root-level tags and nested UDT folders. Verification Report

**Task goal:** lift the two CD5 fail-closed tag layouts in `ign deploy` (root-level `tags.json`/`udts.json` at the `tag-definition` provider root, and nested UDT type folders), live capture first on Docker 8.3.6 and the Windows VM. Each layout is accepted, made read-only or kept refused per the capture. `collection_path`/`repo_path` stay inverses. A non-default collection stays refused. Goldens are byte-identical for trees without the layouts. README parity holds and the secret canary is unaffected. No production calls and no Cargo.toml bump.
**Verified:** 2026-10-30, on branch `fix/deploy-cd5-tag-layouts`, commits 8946e06..cdd7355
**Status:** passed
**Re-verification:** No. This is the first verification.

## Method

- I read the plan, context, summary and capture doc, the full `src/` diff, and every new test.
- I re-ran the gate myself on the main checkout. There is no worktree, and builds use the global cargo `target-dir` (`~/Library/Caches/cargo-target`, from `~/.cargo/config.toml`):
  - `cargo test -p ignition-core cd5`
  - `cargo test --workspace`, run once
  - `cargo fmt --all --check`
  - workspace clippy
  - `cargo xwin clippy` for windows-msvc
- I ran no live or ignored test and made no gateway or network call.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | The capture doc records both hosts, with a command and UTC time on each block, no secrets, and one final verdict each for CD5a, CD5b, CD5c, CD5d and CD5-CTL (`pass`), each the more conservative host verdict | VERIFIED | `## CD5-D` (line 25) and `## CD5-W` (line 2703) are present. Grep counts: each `^Verdict CD5x:` line appears exactly once, there are 8 `Host verdict` lines for CD5a, CD5b, CD5c and CD5-CTL, plus 2 for CD5-RT. There are 0 hits for `BEGIN OPENSSH`, a token header with a value, or `PRIVATE KEY`. Both drivers have 0 `StrictHostKeyChecking=no` and pass `bash -n`. The verdicts are accept/accept/honoured/accepted/pass, and both hosts agree. The 12 `captured-cd5/` fixture sha256 values all appear in the Docker S1 section. All S2 to S4 rows read `ok` after one scan, with no ladder. The only `FAIL` rows are the S5 R9 probes, which are the expected reading for `honoured`. |
| 2 | For CD5a and CD5b the code does exactly what the verdict says | VERIFIED | Both verdicts are `accept`. Both `if` refusals in `layout.rs::tag_collection_path` are deleted, and the comment cites CD5a/CD5b. Per P4, no read-only machinery was built: no `TagLayout`, `read_only_refusal` or `deploy_layout_read_only` anywhere in the tree, `apply/` and `error.rs` are untouched, and `DEPLOY_SLUGS` stays at 16 (`deploy_family_coverage` passes). |
| 3 | `collection_path` and `repo_path` are exact inverses, and the remaining refusals still refuse | VERIFIED | `tags_map_per_cd5` and `repo_path_inverts_collection_path` gain the root and nested rows. The proptest `cd5_tag_paths_round_trip` covers the 4 tag types and tails of 1 to 6 segments, both directions. `unmapped_tag_layouts_stay_fail_closed` keeps `tags/P/other/x.json`, `tags/P/x.json`, `tags/x.json` and `tags/P/tag-definition`. The non-default collection is still structurally unexpressible, since `collection_path` has no collection concept and that is unchanged. README line 2608 and the `layout.rs` doc both state this. All of these pass. |
| 4 | Pull places root and nested units (no `render_refused`), and render then plan is empty, offline and live on both hosts (CD5-RT) | VERIFIED | Offline (I re-ran it): `cd5_pull_places_root_level_and_nested_type_units` asserts `skipped` is empty, the 6 repo files are written, every placed file is byte-equal to the captured live bytes, `render_sha256` matches, and all 7 unit keys are `Same` with `has_changes: false`. Live: the `## CD5-RT` record has the commands, UTC times and `test result: ok` lines (5 passed for `live_deploy_plan`, 1 for `live_deploy_apply cd5`, 2 for `live_transport_wflwvm01`). The filtered-out counts match the current suites (7/5, 8/6 and 34/19 total/ignored, which I enumerated with `--list`). The CD5-RT runs started after the code commit 1c21626 (15:00Z). I did not re-run them, by instruction. |
| 5 | Trees with neither layout are byte-identical in plan and render (`PLAN_GOLDEN`, `PLAN_GOLDEN_LEDGER`, `RENDER_GOLDEN` unchanged and passing) | VERIFIED | The literals at `deploy_plan.rs:577` (`a89de504…`), `:1084` (`5084afcc…`) and `contract_deploy.rs:25` (`7c8b41b1…`) are unchanged. The diff to `deploy_plan.rs` is append-only and `contract_deploy.rs` is untouched. `plan_sha256_golden`, `plan_sha256_golden_with_ledger`, `plan_sha256_golden_with_unreadable` and `render_digest_golden` all pass. No plan field or line type was added (P7). |
| 6 | The v1.6.1 guards cover both layouts: excluded provider, the `Embedded` canary, and long nested paths | VERIFIED | `cd5_excluded_provider_root_file_refused` covers System and an excluded MQTT Engine, for root and nested files. `cd5_excluded_root_ledger_file_never_deleted` shows an empty delete set, `has_changes` false and `plan_sha256` unchanged, with a control proving a kept provider does delete. `cd5_embedded_secret_in_root_and_nested_files` gives `embedded_secret` (exit 2), and `assert_no_canary` checks both Display and Debug. `cd5_pull_skips_embedded_root_unit` skips the whole unit and the canary is absent from Debug. For long paths, `unreadable_unit_keys`/`unit_key_layer` are path-generic and unchanged, and the live Windows test asserts that every placed nested host path is under `LONG_PATH_LIMIT` (137/240). See the note below. |
| 7 | The CD5c branch is implemented as recorded (`honoured`, so render checks `files`) | VERIFIED | `render.rs::check_tag_markers` checks every marker under `tag-definition`/`tag-type-definition`, at every level. The finding names the marker's source path and the missing or absent member names. Unexpected entries are only counted. `cd5_marker_files_must_match_members` asserts the canary entry is never echoed, the finding is `deploy_tree_invalid` (exit 2), and consistent markers render. `cd5_captured_trees_render_clean` shows CAP21 and CAP5 render byte-unchanged. `cd5_pull_skips_a_unit_whose_marker_disagrees` covers the pull side. CD5d is `accepted`, and `cd5_member_named_folder_refused` covers it, case-folded, with a near-miss control. |
| 8 | README cites CD5a/CD5b and states CD5c and the NTFS budget; no slug was added; the full gate is green; there is no Cargo change and no production call | VERIFIED | README `#### Tags layout` cites CD5a, CD5b and CD5c with examples both ways, and states the 122-character budget (checked: the prefix is 117 characters, and 239 − 117 = 122). The `render_refused` skip row cites CD5a, CD5b and CD5c, and two new rows in "What render refuses" cover CD5c and CD5d. Since no slug was added, the exit table is unchanged, and `readme_exit_table_agreement`, `exit_code_mapping_enumerated` and `every_deploy_slug_is_in_the_readme_exit_table` pass. The gate results are in the spot-check table below. `git diff 8946e06..HEAD` on all Cargo.toml and Cargo.lock files is 0 lines (8946e06 = `merge-base HEAD origin/main`). The only production-looking strings are the pre-existing `local_url_guard` refusal-test inputs (blamed to 2026-10-14 and 2026-10-17) and the fixture audit-profile name `WIGNVM01_Audit` in rig logs. No production host appears in the drivers. |

**Score:** 8/8 truths verified (0 present but behavior-unverified)

### Required Artifacts

| Artifact | Status | Details |
|---|---|---|
| `261030-8m8-LIVE-CAPTURES.md` | VERIFIED | 5601 lines. Holds `Verdict CD5a:` and the full Decisions section. |
| `tests/fixtures/live-rig-deploy/capture-cd5.sh` | VERIFIED | 659 lines. Has `CAPTURE_OUT` and the WR-08 `assert_local` authority cut. `bash -n` passes. |
| `tests/fixtures/wflwvm01/capture22w.sh` | VERIFIED | Adds `cd5w`, `cd5wseed` and `cd5wclean`. `bash -n` passes. |
| `tests/fixtures/deploy/captured-cd5/` | VERIFIED | 12 files whose hashes match capture S1, pinned by `captured_cd5_tag_files_follow_cd4`. |
| `src/deploy/layout.rs` | VERIFIED | Refusals removed and docs cite CD5a/CD5b. The P3 types are correctly absent (P4). |
| `tests/deploy_pull.rs` | VERIFIED | Tracer, embedded-skip and marker-disagree tests. |
| `tests/live_deploy_plan.rs` | VERIFIED | `cd5_layouts_round_trip_live`, `#[ignore]`, behind the loopback guard via `rig()`. |
| `README.md` | VERIFIED | Contains `CD5a`. |

### Key Link Verification

| From | To | Via | Status |
|---|---|---|---|
| render stage 3 | `layout::collection_path` → `tag_collection_path` | `fn tag_collection_path`, refusals removed | WIRED |
| pull render check | `render_refused` skip | Root and nested units no longer skipped (tracer `skipped` is empty), and a CD5c mismatch still skips (`cd5_pull_skips_a_unit_whose_marker_disagrees`) | WIRED |
| render | `check_tag_markers` and the CD5d check in `enforce_policy` | Called at `render.rs:114` after `classify_json`; CD5d is in `enforce_policy` | WIRED |
| `read_only_refusal` / `deploy_layout_read_only` | — | Not applicable: both verdicts are `accept` (P4) | N/A (correctly absent) |
| Capture Decisions | `layout.rs` docs, `render.rs` module doc, README | All cite CD5a..CD5d by ID | WIRED |

### Data-Flow Trace (Level 4)

| Artifact | Data | Source | Real data | Status |
|---|---|---|---|---|
| Tracer pull, then render and plan | CAP5 tag files | `captured-cd5/` gateway-written bytes in a local-transport `core` | yes (bytes captured from the gateway, hash-matched to the capture doc) | FLOWING |
| Plan unit grouping | unit keys | `plan::unit_key` over the observed layer (nearest marker) | yes (`cd5_unit_keys`; root files join the provider-root unit, `A/` and `A/B/` are separate units) | FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|---|---|---|---|
| cd5 offline tests | `cargo test -p ignition-core cd5` | 14 offline passed, 0 failed; 3 live tests ignored | PASS |
| Full suite (run once) | `cargo test --workspace` | 96 suites, 3019 passed, 0 failed, 91 ignored | PASS |
| Formatting | `cargo fmt --all --check` | clean | PASS |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 | PASS |
| Windows cross-check | `cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings` | exit 0 | PASS |
| Capture verdict grammar | grep counts per P2 | all exactly as specified | PASS |
| Commit order (capture before code) | `git diff 8946e06..ffa8a99 -- crates/ignition-core/src` | 0 lines; code commit 1c21626 comes after the capture commits | PASS |

### Probe Execution

Not applicable. No `scripts/*/tests/probe-*.sh` is declared, and the capture drivers are live-only, so I did not run them by instruction.

### Requirements Coverage

| Requirement | Source Plan | Status | Evidence |
|---|---|---|---|
| QUICK-8m8 | 261030-8m8-PLAN.md | SATISFIED | Truths 1 to 8 |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| — | — | No `TBD`, `FIXME` or `XXX` in changed code, tests or README; no added `TODO`, `HACK`, `todo!` or `unimplemented!` | — | none |
| `src/deploy/render.rs` | `check_tag_markers` | Deliberate gaps: a marker with no `files` member, and a member file in a folder with no marker, are not checked | Info | Documented in README and in SUMMARY follow-up 6. Every gateway-written marker carries `files`, and capture shows every folder has a marker. |

### Notes (non-blocking)

1. **Long nested paths have no dedicated offline test.** No offline test uses a nested `tag-type-definition` path for `path_too_long`/`unreadable`. The existing test `pull_path_too_long_unit_is_skipped_and_plans_unreadable` uses a `database-translator` path. The mechanism (`unreadable_unit_keys` over `unit_key_layer`) does not depend on the path and this diff does not change it, and the live Windows test asserts the host-path bound. Optional hardening: add a nested-tag case to that test.
2. **Production pull exposure (residual risk, out of scope).** The CD5c check now refuses any live marker whose `files` list holds an entry other than `tags.json`/`udts.json`, or that disagrees with its members. A WIGNVM01 marker of an unexpected shape would show up as `render_refused` on the planned post-release re-pull, not as data loss. This is covered by the hand-off follow-up (a read-only re-pull with the released binary), which is not part of this task.
3. **Stale local `main`.** The plan's Task 3 manifest check diffs against `merge-base HEAD main`, and the local `main` is stale. Measured against `merge-base HEAD origin/main` (8946e06), I independently found no Cargo.toml or Cargo.lock change.
4. **Untracked task files.** CONTEXT, PLAN, RESEARCH and SUMMARY are untracked, for the orchestrator to bundle. The L6 files (`.planning/config.json`, `.planning/research/.cache/`, `svc-ignition-deploy@52.154.75ace-uat4.ps1`) are not in any commit, and no commit has a co-author line.

### Human Verification Required

None. The live CD5-RT runs are recorded in the capture doc with commands, UTC times and result lines, and their test counts match the current suites.

### Gaps Summary

No gaps. Both CD5 layouts were captured on Docker 8.3.6 and on the Windows VM before any `src/` change. Both came back `accept`, and the code maps them read-write with no read-only machinery, as P4 requires. The marker-consistency (CD5c) and member-named-folder (CD5d) checks follow from the capture, are tested, and leave every captured fixture rendering clean. Inverse mapping, goldens, exclusions, the canary, README parity and the full gate all hold, with no Cargo change and no production call.

---

_Verified: 2026-10-30_
_Verifier: Claude (gsd-verifier)_
