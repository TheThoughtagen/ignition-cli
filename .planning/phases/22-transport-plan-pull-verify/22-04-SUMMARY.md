---
phase: 22-transport-plan-pull-verify
plan: 04
subsystem: deploy
tags: [rust, plan, drift, diff, similar, secret-redaction, openat]

requires:
  - phase: 22-transport-plan-pull-verify
    provides: plan engine, flags and delete set (22-03), TreeReader and open_regular_file (22-01), deploy plan verb (22-02)
provides:
  - "ign deploy plan --base DIR: three-way drift against a re-hashed previous render (BaseTree::read, apply_base, PlanEntry.drift, PlanSummary.drifted, DeployPlanResult.base)"
  - "ign deploy plan --diff: redacted unified text diff (render_diffs, DIFF_MAX_BYTES, DIFF_WITHHELD, PlanEntry.diff)"
  - "PlanOptions { base, diff } and BaseSummary { dir, render_sha256, files }; deploy_plan takes &PlanOptions"
  - "Plan output under the secret canary (core and CLI, human and --json)"
affects: [22-06, 22-08, 22-10, 22-13, 23]

actuals:
  tokens: 15800
  tasks: 2
  commits: 4

tech-stack:
  added: ["similar 3 (already in Cargo.lock at 3.2.0 via snapbox; only the ignition-core dependency list changed)"]
  patterns:
    - "A previous render is re-hashed from its real bytes; render.json is identity and a consistency record only"
    - "Bytes from --base are read through tree::open_regular_file after a capped walk_capped, hashed while streaming and never kept"
    - "File text reaches the plan only after both sides pass scan_secrets/scan_embedded; the scan result is reduced to a count"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/deploy/plan.rs
    - crates/ignition-core/src/deploy/tree.rs
    - crates/ignition-core/src/actions/deploy.rs
    - crates/ignition-core/Cargo.toml
    - Cargo.lock
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - README.md
    - crates/ignition-core/tests/deploy_plan.rs
    - crates/ignition-cli/tests/contract_deploy.rs

key-decisions:
  - "STATE blocker WR-03 / R-03 is resolved by re-hashing: Phase 22 never trusts render.json hashes. A base whose walked files, per-file sha256 or size, or listing digest disagree with its marker (including a leftover .ign-render-tmp) is refused as invalid_input naming the first difference"
  - "drift rule: a file drifts when live differs from both base and desired, an absent side counting as its own value; applied to every entry including unmanaged"
  - "Withheld and hash-only diff lines are prefixed with the member path so an entry with several differing files stays readable"

patterns-established:
  - "render_base(fx) test helper builds a base with deploy_render(.., Some(out)) into a scratch dir, then tests tamper with it"
  - "PlanOptions::default() is the no-flags call used by every pre-22-04 test"

requirements-completed: [DPLN-05]
requirements-advanced: [DPLN-02]

duration: 70min
completed: 2026-10-14
status: complete
---

# Phase 22 Plan 04: `--base` drift and redacted `--diff` Summary

**`ign deploy plan --base DIR` marks out-of-pipeline changes as drift against a previous render that is re-hashed from its real bytes (never trusted from `render.json`), and `--diff` adds a unified text diff that withholds anything secret-bearing or unscanned, with plan output under the secret canary.**

## Accomplishments

- **`BaseTree::read`** (`deploy/plan.rs`): the base must be a real directory (not a symlink); it is walked with `walk_capped` (entry cap `READ_MAX_ENTRIES`), every file is opened through `open_regular_file` (rustix `openat`, `O_NOFOLLOW`) and streamed through SHA-256 under the per-file and total byte caps. `render.json` is parsed as a schema 1 `RenderMarker` with a value-free error, its `gateway`/`env`/`mode` must equal the plan's, the walked file list must equal the marker's, and each recomputed sha256 and size and the recomputed listing digest must equal the marker's. Every refusal is `invalid_input` shaped `--base <dir>: <problem> (first difference: <path>)`. The base is read before the live tree (`bad_base_costs_no_transport_work`).
- **Drift** (`apply_base`): `PlanEntry.drift` (after `lockout_risk`) and `PlanSummary.drifted`, null without `--base`; `DeployPlanResult.base` is `{dir, render_sha256, files}` or null. Excluded from `plan_sha256`.
- **`render_diffs`**: `PlanEntry.diff` (last field). Per differing file: a `similar` unified diff (3 lines of context, `live/<path>` and `desired/<path>` headers) only for `.json` members that parse and report zero refused or warned locations on both sides (`scan_secrets` for `config.json` keyed by `resource_key`, `scan_embedded` for tag files and other `.json`); otherwise a `withheld: contains secret-bearing locations (<n>); live <sha|->, desired <sha|->` line or a `differs (live .., desired ..)` line. Capped at `DIFF_MAX_BYTES` (65536) with the marker `... [diff truncated at 65536 bytes]`. Excluded from `plan_sha256`.
- **CLI**: `--base DIR` and `--diff` on `deploy plan`, `PlanOptions` through `main.rs`, human output prints the diff indented two spaces under its entry and a `drift` token on the entry line. README gains `#### Drift (--base DIR)` and `#### The diff (--diff)`, the new data keys, and the Commands row `ign deploy plan --env <ENV> [--repo DIR] [--base DIR] [--diff]`.

## Task Commits

1. **Task 1 RED: drift tests and skeleton** - `4ae8e2c` (test; 7 of the new tests failed against the skeleton)
2. **Task 1 GREEN: `--base` drift** - `3c1942e` (feat)
3. **Task 2 RED: diff and canary tests** - `5025892` (test; core diff tests and `plan_diff_canary_cli` failed; `similar` added to Cargo.toml here)
4. **Task 2 GREEN: `--diff` with withholding** - `d4d253e` (feat)

## Verification

- `cargo test --workspace`: no failures (`deploy_plan` 52, `contract_deploy` 22, core lib 671, `deploy_family_coverage`, `tui_coverage`, `contract_stdout_purity` green).
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` clean.
- `PLAN_GOLDEN` and `PLAN_GOLDEN_LEDGER` unchanged (`plan_sha256_golden`, `plan_sha256_golden_with_ledger` still pass).
- Acceptance greps: `pub fn read` in `plan.rs`; `base: Option<PathBuf>` in `cli.rs` (1 line); `^similar` in `ignition-core/Cargo.toml` (1 line); `git diff Cargo.lock` shows a single added line, `"similar"` in the `ignition-core` dependency list, no new `[[package]]`; the README Commands row matches.
- Test sensitivity: replacing the scan count with 0 in `secret_locations` made `diff_withholds_secret_bearing_member`, `diff_withholds_when_only_the_desired_side_is_secret_bearing` and `canary_never_leaves_plan` fail; reverted.

## Edge coverage

| Edge | Evidence |
|---|---|
| DPLN-05 three-way | `drift_three_way` (live edited, pipeline change not applied, change applied since base) |
| DPLN-05 absent-side cases (flagged assumption) | `drift_for_unit_added_outside_pipeline`, `drift_for_unit_deleted_outside_pipeline` |
| DPLN-05 null / hash | `drift_null_without_base`, `hash_independent_of_base` |
| R-03 / WR-03 | `base_modified_refused` (extra file; edited file with stale marker; edited file with rewritten marker hash and size so only the listing digest disagrees; leftover `.ign-render-tmp`; missing file; missing and invalid marker), `base_identity_mismatch_refused` |
| Hostile base | `base_hostile_refused` (absent, regular file, symlinked base, symlink inside), `bad_base_costs_no_transport_work` |
| Diff | `diff_changed_json_shows_unified_text`, `diff_added_and_removed_entries`, `diff_withholds_secret_bearing_member`, `diff_withholds_when_only_the_desired_side_is_secret_bearing`, `diff_non_json_member_is_hash_only`, `diff_is_capped`, `diff_absent_without_flag` |
| Canary (SC-5 plan half) | `canary_never_leaves_plan` (JSON, pretty JSON and `Debug`, with `--base` and `--diff`), `plan_diff_canary_cli` (human and `--json`, stdout and stderr) |

## Deviations from Plan

**1. [Rule 3 - Blocking] `TMP_SUFFIX` in `tree.rs` made `pub(crate)`.** `BaseTree::read` names a leftover temporary file by the same suffix the writer uses. `tree.rs` is not in the plan's `files_modified`; the change is one visibility keyword. Commit `3c1942e`.

**2. [Rule 3 - Blocking] Existing callers updated for the new signature.** `deploy_plan(declared, reader, &PlanOptions)` required `&PlanOptions::default()` at every call in `tests/deploy_plan.rs`, and `contract_deploy.rs::plan_cli_live_equal_to_render_exits_0` now expects the additive `base` key. Commits `4ae8e2c`, `3c1942e`.

**3. [Clarification] Withheld and hash-only lines carry the member path.** The plan's text is `<DIFF_WITHHELD> (<n>); live <sha|->, desired <sha|->`; for an entry with several differing files the path is needed, so the line is `<path>: withheld: contains secret-bearing locations (<n>); live .., desired ..`, matching the `<path>: differs (...)` form the plan gives for non-JSON members. A check for `DIFF_WITHHELD` as a substring still holds.

**4. [Clarification] Key order.** The plan places `drift` after `lockout_risk` and `diff` last; the 22-03 summary said both would follow `files`. The plan text was followed (`..., lockout_risk, drift, files, diff`). Only matters with `preserve_order`; README does not promise object order.

**5. [Extra enforcement] Base walk is entry-capped and directory-checked.** The plan names `walk_out`; `walk_capped(.., READ_MAX_ENTRIES)` is used (as the prior wave required), a directory holding no listed file is refused, and a marker `bytes` mismatch is refused alongside the sha256 check.

**6. [Addition] Human `drift` token and a `plan_base_cli_marks_drift_and_refuses_a_tampered_base` contract test.** Not in the plan; small, and the verb needed an end-to-end `--base` test.

**Process note:** DPLN-05's absent-side drift cases remain the plan's flagged assumption (no source states them); the tests pin the implemented behaviour only.

## Known Stubs

None. The 22-01 `for_node` ssh-arm stub remains until 22-06.

## Threat Flags

None beyond the register. T-22-13 to T-22-16 and T-22-SC are mitigated and tested as listed above. Residual: `--diff` relies on the render lints' secret rules; a secret placed under a key name those lints do not cover in a `.json` file the lints treat generically (for example a plaintext `password` value in a tag file) would be shown as text. This is the same rule the render applies, not a new gap, and the withholding can only get stricter.

## Not covered

- Windows: `BaseTree::read` uses `open_regular_file`, whose Windows path has the 22-01 residual race on intermediate directories; not compiled for Windows here.
- No live-gateway capture of `--base` or `--diff`; both are exercised over the local transport only. `cargo install` and a manual smoke of the installed `ign` were not run; the behaviour is covered by the binary-level contract tests (`assert_cmd`).

## Requirement bookkeeping

DPLN-05 is delivered in full by this plan and is marked complete. DPLN-02 was already complete. DPLN-01 stays pending until 22-06 (ssh).

## Self-Check: PASSED

- Commits `4ae8e2c`, `3c1942e`, `5025892`, `d4d253e` present in `git log`.
- Modified files present; README, Cargo.toml, cli.rs greps return the expected counts.

## Post-review security fixes

### F4: `--diff` printed plaintext secrets from LIVE files (commit `a5656ef`)

**Finding.** The render lints were designed for the desired (repo) side. The live side is written by the gateway and never linted. For any member other than `config.json`, and for tag files, `file_diff` ran only the Embedded-object scan (`scan_embedded`) with no key-name check, so `{"password": "x"}` in a live non-config `.json` or tag file was printed verbatim in the text diff. This corrects the "Residual" paragraph in the threat-flags section above: the "same rule the render applies" argument did not hold, because render never sees the live side. The Residual note is superseded.

**Fix** (`crates/ignition-core/src/deploy/plan.rs`). A diff-only check, `sensitive_key_hits`, runs over every parsed `.json` member on BOTH sides, in `file_diff`, in addition to `secret_locations`:

- It counts one hit for each non-empty string and each non-null scalar (`true`, `false`, numbers) at or beneath an object key whose ASCII-lowercased name contains `password`, `passwd`, `secret`, `token`, `apikey`, `api_key`, `credential`, `privatekey` or `private_key`. Empty strings and `null` do not count. A secret-named key holding an object or array counts every scalar leaf under it.
- It applies regardless of file name, OpenAPI description or tag-file status.
- Hits are added to the location count `<n>`, so the member is withheld with `DIFF_WITHHELD`. Counts only: neither the value nor the key path is kept.
- `lint.rs`, the render path and `plan_sha256` are untouched. `PLAN_GOLDEN` and `PLAN_GOLDEN_LEDGER` did not change (the diff is not hashed). `--diff` is now deliberately stricter than render.
- README `#### The diff (--diff)` and the `render_diffs` doc comment updated.

**Behaviour note.** The check is a key-name substring match, so a benign value under a key like `tokenType` or `passwordPolicy` is withheld as well. That fails closed to the hash-only line, which still carries both sha256 values.

**Tests** (`crates/ignition-core/tests/deploy_plan.rs`, 5 added; red first, the canary printed verbatim before the fix):

- `diff_withholds_live_non_config_json_with_a_secret_key`: live `resource.json` (`password`), a live-only `extra.json` (nested `apiKey`) and a live-only `token.json` (`Token` inside an array), all withheld; no canary in JSON, pretty JSON, `Debug` or the diff text (the human form prints only `entry.diff`).
- `diff_withholds_live_tag_file_with_a_secret_key`: a live tag file with `password`, `token` and `authToken` keys, withheld.
- `diff_secret_key_check_covers_the_desired_side_and_the_key_list`: nine key spellings and cases, on the desired side.
- `diff_secret_key_check_ignores_empty_and_null_values_and_benign_keys`: a benign member still diffs; an empty `password` and a `null` `token` are not hits.
- `diff_secret_key_check_leaves_the_plan_hash_alone`: `plan_sha256` is equal with and without `--diff`.

**Not covered.** The check does not look at string values for secret-shaped content (a secret stored under an innocuous key such as `value` or `data` still diffs as text), and it does not cover non-`.json` members, which were already hash-only. No live-gateway capture.
