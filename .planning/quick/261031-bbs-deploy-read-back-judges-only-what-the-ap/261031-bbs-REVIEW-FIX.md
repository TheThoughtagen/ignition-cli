---
phase: 261031-bbs-deploy-read-back-judges-only-what-the-ap
fixed_at: 2026-10-31T16:55:00Z
review_path: .planning/quick/261031-bbs-deploy-read-back-judges-only-what-the-ap/261031-bbs-REVIEW.md
iteration: 2
findings_in_scope: 7
fixed: 7
skipped: 0
not_in_scope: 5
status: all_fixed
round_2_fixed_at: 2026-10-31T18:30:00Z
round_2_items: 5
round_2_fixed: 5
round_2_skipped: 0
---

# Quick 261031-bbs: Code Review Fix Report

**Fixed at:** 2026-10-31T16:55:00Z
**Source review:** .planning/quick/261031-bbs-deploy-read-back-judges-only-what-the-ap/261031-bbs-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope (orchestrator scope): 7. These are CR-01, WR-01 and IN-01 (code) and WR-02, WR-03, WR-04 and IN-07 (README only).
- Fixed: 7
- Skipped: 0
- Not fixed by instruction: 5 (IN-02 to IN-06)

The work ran on branch `fix/deploy-read-back-scope` in the main checkout, with no worktree (`workflow.use_worktrees=false`, CLAUDE.md cargo rule). Verification also ran in the main checkout. Every code fix had its failing test first.

## Fixed Issues

### CR-01: A partial write to a covered path passes post-apply verify

**Files modified:** `crates/ignition-core/src/deploy/verify.rs`, `crates/ignition-core/tests/deploy_verify.rs`, `README.md`
**Commit:** 6005f00
**Status:** fixed: requires human verification (it is a logic change to the safety judge)

**Applied fix:**
- `BaselineEntry` now stores `writes_at`, the uncapped set of paths the write changes. It is `diff_exact` of the null-stripped desired comparison against the pre-apply live one, and comparisons of different kinds give `(root)`. It replaces `write_differs`, and a public `writes_at()` accessor exposes it.
- In the `LiveFile` branch, after the new-path check, the whole-view A1 runs first, unchanged: red when `writes_at` is non-empty and REST did not move at all.
- Then the per-path rule runs. A written path is "stuck" when the post-apply difference overlaps it (the path itself, an ancestor or a descendant, with `(root)` overlapping everything) and no REST movement r0→r1 overlaps it. Any stuck path is red, with the detail `differs at: <stuck> (its REST view did not change there during this apply, so that part of the write did not take effect)`.
- `path_covered` was refactored onto a shared `path_under` helper, with the same behaviour (`rb_judge_prefix_rule` is unchanged).

**Tests:**
- `rb_judge_partial_write_covered_path_fails`: the Chiller1 shape. The baseline differs at `settings.deviceSettings`, the write changes `settings.deviceSettings.hostname` and `settings.enabled`, and only `enabled` is applied. Result: red on the hostname only.
- `rb_judge_full_write_covered_path_passes`: the same write with both members applied. Result: warn, green.
- `rb_judge_partial_write_root_covered_fails`: the REST array lists an extra, gateway-provided entry, so the baseline is `(root)`. The write changes `0.host` and `1.port`, and only `0.host` is applied. Result: red on `1.port`. With both applied it is a warn.
- Adjusted `rb_judge_written_within_live_baseline_warns`. Its old scenario was itself the masked partial write: desired `settings.a=5`, with REST at 1 before and after. CR-01 now makes that red, as intended. The scenario now has the write take effect and keeps a pre-existing member (`settings.extra`) as the warning.

**README:** the `changed` rule now lists all four red cases, including the per-path one with the `settings.deviceSettings` / `(root)` example. The residual-risk paragraph replaces the false claim that "A1 still catches" the masking with the actual remaining risks:
- a written path that moved but still differs in the pre-existing way is a warning;
- a written path the gateway normalises back to its old value is a false red;
- the window risk.

**Known limit, not new:** for an `ignition/security-levels` (`NameKeyedSubset`) unit, the post-apply paths use `[name]` segments while `writes_at` uses indices, so the per-path rule cannot match them and only the whole-view A1 applies. That type is denied by render, so it is not deployable today.

### WR-01: A written unit whose post-apply effective copy is in `system` is never judged

**Files modified:** `crates/ignition-core/src/deploy/verify.rs`, `crates/ignition-core/tests/deploy_verify.rs`, `README.md`
**Commit:** 887191e
**Status:** fixed: requires human verification (it is a logic change to the safety judge)

**Applied fix:** `read_back_check_with` now asks a new `system_red(key, judge)` before it emits the "not compared" warning.
- Under `ReadBackJudge::Baseline`, it returns a red `resource_missing` (counted in `resources_checked`) in two cases:
  - for a written unit (`LiveFile` or `Strict` basis): `... (this apply wrote it, so the write did not take effect)`;
  - for an unwritten unit that was `Config` before: `... (it was file-backed before this apply and dropped out of the effective view during this apply)`.
- Everything else keeps the old warning. That covers the `Strict` and `Standalone` judges, no baseline entry, and an unwritten unit that was `missing` or `system` before.
- No new finding code was added.

**Test:** `rb_judge_written_now_system_fails` covers these cases:
- `(LiveFile, Config) -> System`, `(Changed(None), Config) -> System`, `(Added, Missing) -> System` and `(Added, System) -> System`, all red;
- unwritten `Config -> System`, red;
- unwritten `System -> System`, still the warning with `resources_checked` 0.

The README baseline rules gained a bullet for this.

### IN-01: `rest_sha256` is a guessing oracle

**Files modified:** `crates/ignition-core/src/deploy/verify.rs`, `crates/ignition-core/tests/deploy_verify.rs`, `crates/ignition-core/tests/deploy_apply.rs`, `README.md`
**Commit:** 76752d7
**Status:** fixed

**Applied fix:** removed `BaselineRecordEntry::rest_sha256` and `canonical_sha256`. The record now holds `state`, `basis` and `differs_at` only. The in-memory r0 comparison is unchanged.

**Tests:**
- `rb_judge_record_value_free` asserts the entry's key set is exactly `{basis, differs_at, state}`, and that `sha256` does not appear in the record.
- `rb_apply_baseline_file_value_free` (the canary test) asserts there is no `rest_sha256` key, and that no `sha256` bytes appear in `read_back_baseline.json`. It still runs `run_dir_canary_leaks`.

**README:** the run-dir schema and the "Where secrets can be" text no longer mention a hash.

### WR-02: Deleted units are not verified after the apply

**Files modified:** `README.md`
**Commit:** e2c459c
**Status:** fixed (README only, as scoped)

**Applied fix:** the baseline rules now say plainly that `read_back` does not verify deleted (`removed`) units. On the master, `files` covers tag units only, so a deleted non-tag unit the gateway still serves over REST is not reported. In a pair, only the backup's `files` check compares the deleted paths. This replaces the line around 3610 that said "`files`, the plan and the ledger judge them". No code check was added (the reviewer's "better" option was not in scope).

### WR-03: Standalone verify is green when the gateway is not running the repo's `same` config

**Files modified:** `README.md`
**Commit:** bdc5b71
**Status:** fixed (README only, as scoped)

**Applied fix:** the standalone-warn paragraph no longer claims the difference is gateway defaults. It says the warning marks a pre-existing REST/file difference on a plan-`same` resource whose cause verify cannot tell. It names the possibilities: module defaults or normalisation, files copied without a scan, a scan that ignored a file silently, or a backup that never reloaded. It also says a green verify no longer proves the REST view matches the repo for `same` resources. The check table now says a resource with no plan status also stays red. The opt-in `--strict-read-back` idea was not implemented (out of scope).

### WR-04: A `changed` unit missing from REST before the apply can never be applied

**Files modified:** `README.md`
**Commit:** 648a614
**Status:** fixed (README only, as scoped)

**Applied fix:** a new baseline-rules bullet says such a unit is judged strictly, and that it rolls back on every apply unless its REST view appears and matches. It names this as a known blocker, and notes that `ignition/edge-sync-settings` on production is in this state, with plan `same` today and so no impact yet. The code behaviour is unchanged.

### IN-07: Smaller README gaps

**Files modified:** `README.md`
**Commit:** df01c31
**Status:** fixed

**Applied fix:**
- The `--node backup` text now names both strict fallbacks: the backup has no file transport, or the backup's or the master's files read fails. In the second case `files` reports a red `backup_unreachable` finding naming the node and the error code. This was confirmed against `verify_backup_files` in `actions/deploy.rs`.
- The baseline section now says a failed baseline read ends the run with the underlying error's own envelope, slug and exit code (for example `network_error` or `transport_failed`, both exit 4), not `deploy_preflight_failed`, and that `result.json` records the same `error.code`. This was confirmed by `rb_apply_baseline_read_failure_writes_nothing`.

## Not Fixed (by instruction)

### IN-02: The canonical hash is not number-canonical

**File:** `crates/ignition-core/src/deploy/verify.rs:740`
**Reason:** not in the orchestrator's scope (it said to skip IN-02 to IN-06). It is moot anyway: IN-01 removed `canonical_sha256` and `rest_sha256`.
**Original issue:** `scalar.to_string()` writes `1` and `1.0` differently, so two views `read_back` treats as equal could hash differently.

### IN-03: Paths with `.` or `[` in keys, or truncated keys, can collide in `path_covered`

**File:** `crates/ignition-core/src/deploy/verify.rs` (`path_covered`, `printable`, `child_path`)
**Reason:** not in the orchestrator's scope. The new `paths_overlap` used by CR-01 has the same joined-string limitation. Fixing it needs segment-vector paths, which is a wider refactor.
**Original issue:** a literal key `a.b` is "covered" by `a`, and two keys over 96 characters with a shared prefix both become `<prefix>...`.

### IN-04: `converge::Baseline::read_back` is an `Option` whose `None` branch cannot be reached

**File:** `crates/ignition-core/src/deploy/apply/converge.rs:58`, `crates/ignition-core/src/deploy/apply/mod.rs:1541`
**Reason:** not in the orchestrator's scope. It is a type-shape refactor. The fallback is strict, so it can only false-red.
**Original issue:** the `Option` hides the invariant that a backup baseline exists whenever the backup does.

### IN-05: `pair::read_back_backup_check` is used only by tests

**File:** `crates/ignition-core/src/deploy/pair.rs:469-476`
**Reason:** not in the orchestrator's scope. It is dead-code hygiene only.
**Original issue:** the strict wrapper could be removed or made `#[cfg(test)]`.

### IN-06: The live test's cleanup docs over-promise; the leftover check ignores the seeded alarm unit

**File:** `crates/ignition-core/tests/live_deploy_apply.rs:2142-2143`, `:291-295`, `:1349-1363`
**Reason:** not in the orchestrator's scope. A live negative test (a partial write that must roll back) remains a useful follow-up now that CR-01 has landed.
**Original issue:** `Clean::drop` only sweeps files; `forget_units` does not check that `ignition/general-alarm-settings` left the mode collection.

## Verification

All verification ran in the **main checkout** (no worktree), so the numbers can be reproduced from this tree.

- Per fix: the targeted `cargo test -p ignition-core --test deploy_verify` and `--test deploy_apply rb_` runs, plus `cargo test -p ignition-cli --test deploy_family_coverage` for the README edits.
- Full gate after df01c31:
  - `cargo fmt --all --check`: clean.
  - `cargo clippy --workspace --all-targets -- -D warnings`: exit 0.
  - `cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings` (with `~/.cargo/bin` first on PATH): exit 0.
  - `cargo test --workspace`: 96 result lines, 3065 passed, 0 failed, 92 ignored.
- No change to Cargo.toml or Cargo.lock. `deploy_plan.rs` and `contract_deploy.rs` are untouched, so the goldens are byte-identical.
- Docker rig: `live_deploy_apply` 7/7 passed and `live_deploy_plan` 5/5 passed, then the rig was torn down. There were no production calls. SUMMARY.md has the details.


## Round 2 (second review of CR-01 and WR-01)

A second review of 6005f00 (CR-01) and 887191e (WR-01) returned REQUEST CHANGES. WR-01 was fine. The CR-01 rule false-redded every legitimate edit to a member REST does not expose. The value-free production shapes that showed it:
- OPC device `Chiller1`: the file's `settings.deviceSettings` is a non-JSON string, and REST `config.settings` has no `deviceSettings` key.
- Historian `SparkDB`: the file's `settings.profile` is a string, and REST has no `settings.profile`.
- Cirrus `namespace-server-set/1`: the file is `{namespace, serverSet}`, and REST `config` is null.

Same branch, main checkout, no worktree (`workflow.use_worktrees=false`, CLAUDE.md cargo rule; no other worktree exists, so the shared `CARGO_TARGET_DIR` is safe). Each code fix had its failing test first. All verification ran in the **main checkout**.

| Item | Commit | Message |
|------|--------|---------|
| 1 UNOBSERVABLE | 58fe621 | fix(261031-bbs): unobservable written paths warn instead of failing read_back |
| 2 ALREADY-SHOWN | 451c163 | fix(261031-bbs): already-shown written paths are not judged as untaken writes |
| 3 Review tests | eeeb610 | fix(261031-bbs): pin the review's read_back cases and the collapse residuals |
| 4 `writes_at` record | 7c04158 | fix(261031-bbs): record writes_at in read_back_baseline.json |
| 5 README | 6bc4ae3 | fix(261031-bbs): README limits read_back's per-path claim and documents what REST cannot show |
| 5 (doc comment) | 37280f0 | fix(261031-bbs): read_back doc comment lists the collapse and new-path residuals |

### 1. UNOBSERVABLE written paths

**Files:** `crates/ignition-core/src/deploy/verify.rs`, `crates/ignition-core/tests/deploy_verify.rs`
**Commit:** 58fe621
**Status:** fixed: requires human verification (a logic change to the safety judge)

- New `shows(rest, at, target)` decides whether a REST value shows anything at a path. It shows the path when the path resolves (so its descendants are covered), or when an ancestor is one opaque value (a string standing for a structured member). A `null`, an absent member, an index past the end, or a member of an object that lacks the key shows nothing, and a `null` root shows nothing at all. It walks paths with `child_path`, as the diffs build them.
- `BaselineEntry::unobservable(r0, r1)` is the set of `writes_at` paths that neither `r0` nor `r1` shows.
- In the live-file branch, those paths are left out of the whole-view A1 rule and the CR-01 per-path rule. They are reported as one `warn` finding (code `resource_differs`, no new code) with the detail `<paths> is not visible in the gateway's REST view, so the write could not be verified` ("are" for several), and the `data.warnings` line `read_back: <key>: <that detail>`. That warning goes out even when nothing else differs, and beside a red verdict on other paths.
- Post-apply difference paths that overlap an unobservable written path are not repeated in the "already different" warning, so each path is explained once.
- `judge_resource` now returns `Vec<Verdict>`. The live-file arm moved into `judge_written`.

**RED → GREEN:** `deploy_verify rb_judge`: `22 passed; 3 failed`, then 25 passed.
- `rb_judge_unobservable_blob_other_member_moving_warns`: the Chiller1 shape, with the blob written, REST lacking the key and `settings.enabled` moving. It was red under CR-01 and is now **warn** (one finding, the not-visible detail).
- `rb_judge_unobservable_blob_alone_warns`: the blob alone, with nothing moving. It was red under A1 and is now **warn**.
- `rb_judge_unobservable_null_config_warns`: the Cirrus shape, a null REST config before and after. It was red under A1 and is now **warn** naming `namespace`.
- Unit test `shows_resolves_paths_ancestors_and_opaque_values`.
- The doc comment on the existing `chiller`/`chiller_rest` helpers is corrected. They are the opaque-string shape, and production's Chiller1 lacks the key.

### 2. ALREADY-SHOWN written paths

**Files:** `crates/ignition-core/src/deploy/verify.rs`, `crates/ignition-core/tests/deploy_verify.rs`
**Commit:** 451c163
**Status:** fixed: requires human verification

At build time, `BaselineEntry.shown_at` holds the `writes_at` paths where `r0` resolves (through the new `resolve`, nulls stripped) to a value equal to the desired comparison's value there. These paths are left out of A1 and the per-path rule. `compare_doc` was factored out of `write_paths` and is shared.

**RED → GREEN:** `rb_judge_write_already_shown_root_covered_warns`.
- Under a `(root)`-covered `p`, `0.host` was written equal to `r0`. With no move it was red under A1 (`differs at: (root) (its REST view did not change…)`) and is now **warn**.
- Beside a landing `1.port` it is **warn**.
- Unit test `resolve_finds_present_paths_only`.

### 3. Tests from the review

**Files:** `crates/ignition-core/tests/deploy_verify.rs`, `crates/ignition-core/src/deploy/verify.rs` (unit test)
**Commit:** eeeb610

- `rb_judge_masked_partial_write_fails`: the old masked partial write (desired `settings.a=5`, REST at 1 before and after, `v` landing). **fail**, `differs at: settings (…did not take effect)`.
- `rb_judge_two_members_under_structured_ancestor_one_lands_fails`: `s` is covered by a gateway list entry, `s.0.a` and `s.1.b` are written, and only `s.0.a` lands. **fail** on `s.1.b`. Both landing gives **warn**.
- `rb_judge_attributes_partial_write_fails`: CR-01 on `Compare::Attributes`, with `b` normalised by REST, `a` and `b` written, and only `a` landing. **fail** on `b`. Both landing gives **pass**.
- `paths_overlap_is_symmetric_on_segment_boundaries`: unit tests for equal, ancestor, descendant, `(root)`, a shared non-boundary prefix, and siblings.
- Accepted residual risk, pinned at today's verdict (a false green) so a collapse-aware fix flips them deliberately:
  - `rb_judge_residual_collapse_key_absent_in_live_warns`: `writes_at = {s}`; `s.b` lands and `s.a` does not. **warn**.
  - `rb_judge_residual_collapse_array_length_warns`: `l` grows from 1 to 3 items and the third does not land. **warn**.
  - `rb_judge_residual_opaque_rest_value_warns`: REST `deviceSettings` is a string, and only the port under it lands. **warn**.
- Mutation check: disabling the per-path red turned 5 tests red: the 3 new pins plus `rb_judge_partial_write_covered_path_fails` and `rb_judge_partial_write_root_covered_fails`. The file was restored afterwards.

### 4. `writes_at` in `read_back_baseline.json`

**Files:** `verify.rs`, `tests/deploy_verify.rs`, `tests/deploy_apply.rs`, `tests/live_deploy_apply.rs`, `README.md` (schema line)
**Commit:** 7c04158

- `BaselineRecordEntry.writes_at: Vec<String>` holds the uncapped, byte-sorted paths. It is filled for `live_file` units only and is `[]` otherwise. `schema_version` stays 1, because the change is additive.
- RED → GREEN:
  - `rb_judge_record_value_free`: the exact key set is now `basis, differs_at, state, writes_at`. A written-unit case puts a canary in both the old and the new value and asserts `writes_at = ["nested.b", "secretish"]` with neither canary in the record.
  - `rb_apply_baseline_file_value_free`: both entries carry exactly those four keys, with `writes_at` `[]` (this rig's change is bytes only). The canary and no-hash assertions are unchanged.
  - The live test `live_preexisting_rest_difference_warns` asserts the alarm unit's `writes_at` is `[]`.

### 5. README

**Files:** `README.md` (plus the pins in `tests/deploy_verify.rs`); the doc comment in `verify.rs`
**Commits:** 6bc4ae3, 37280f0

- The CR-01 bullet is limited to written leaves that REST exposes as structured members. Its example is now a REST list with an extra gateway entry.
- New bullets:
  - The unobservable warning, with the production examples (OPC UA `settings.deviceSettings`, historian `settings.profile`, Cirrus Link namespace sets with no REST config) and a note that a secret field REST leaves out of its view is unobservable too, so it warns.
  - The already-shown exclusion.
- The residual list is now a list. It adds:
  - the collapse cases (a key absent in live, an array length change, an opaque REST value);
  - the explicit-default and reorder false reds of the new-path rule, with the advice to author `changed` units with `deploy pull`.
- The two false reds are pinned at today's verdict:
  - `rb_judge_false_red_explicit_default_fails`: red `differs at: k (not different before this apply)` plus the not-visible warning for `k`.
  - `rb_judge_false_red_reorder_fails`: red `differs at: l.0, l.1`.

### Declined or left as is

- **The unobservable exclusion is not applied to the new-path rule.** The instruction scoped it to A1 and CR-01, and asked for the explicit-default false red to be documented instead. Extending it would turn that false red into a warning. It is a candidate follow-up.
- **Secret redaction.** The README claim holds when REST *leaves out* a secret field. If a gateway instead returns a fixed mask, the field is visible, and a password change would read as not taken (a false red). I did not verify this against production; no production calls were made.
- **`shown_at` is not recorded** in `read_back_baseline.json`. Only `writes_at` was asked for.
- **IN-02 to IN-06** stay as in round 1. The collapse-aware path sets are tracked as a follow-up (below).

### Follow-up (tracked)

- **Collapse-aware path sets (segment-vector paths, IN-03).** Judge written leaves rather than the shallowest differing path, and stop joining path strings, so the three residual pins flip to red and keys containing `.` or `[` stop colliding.

### Round 2 verification

All in the **main checkout**.

| Gate | Result |
|------|--------|
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `PATH="$HOME/.cargo/bin:$PATH" cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace` | 96 result lines: 3080 passed, 0 failed, 92 ignored (15 new tests on top of round 1's 3065) |
| `git diff --quiet df01c31..HEAD -- Cargo.toml Cargo.lock crates/*/Cargo.toml crates/ignition-core/tests/deploy_plan.rs crates/ignition-cli/tests/contract_deploy.rs` | no change, so the goldens are byte-identical |

Live Docker rig (no production call): `live_deploy_apply` 7/7 and `live_deploy_plan` 5/5. SUMMARY.md has the steps.

---

_Fixed: 2026-10-31T16:55:00Z (round 1), 2026-10-31T18:30:00Z (round 2)_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 2_
