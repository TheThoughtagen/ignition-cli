---
phase: 22-transport-plan-pull-verify
plan: 03
subsystem: deploy
tags: [rust, ledger, delete-set, plan-engine, thiserror, openat]

requires:
  - phase: 22-transport-plan-pull-verify
    provides: plan engine, TreeReader, LocalFsTransport, open_regular_file, deploy plan verb (22-01, 22-02)
provides:
  - "Declared-names ledger format v1: Ledger, Ledger::parse (strict), Ledger::to_bytes, ledger_location, LEDGER_MAX_BYTES"
  - "TreeReader::read_ledger (local transport, descriptor-relative) and DeleteSet"
  - "deploy_ledger_invalid (exit 6) in all four places plus README"
  - "Plan delete set, removed status, shadows_removal, destructive / destructive_reason / lockout_risk flags, D lines in plan_sha256"
affects: [22-04, 22-06, 22-08, 22-10, 22-12, 22-13, 23]

actuals:
  tokens: 25700
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "An on-box contract file is parsed through node::classify (size, UTF-8, duplicate keys, depth) before serde, then validated by explicit checks whose messages name fields and listed paths only"
    - "Every read of bytes from the data dir goes through open_regular_file (rustix openat, O_NOFOLLOW); path-based stat is used only to classify absence"
    - "plan() stays pure: the caller parses the ledger, plan receives Option<&Ledger> and the manifest Scope"

key-files:
  created:
    - crates/ignition-core/src/deploy/ledger.rs
  modified:
    - crates/ignition-core/src/deploy/mod.rs
    - crates/ignition-core/src/deploy/layout.rs
    - crates/ignition-core/src/deploy/plan.rs
    - crates/ignition-core/src/deploy/transport/mod.rs
    - crates/ignition-core/src/deploy/transport/local.rs
    - crates/ignition-core/src/actions/deploy.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-cli/src/render.rs
    - README.md
    - crates/ignition-core/tests/deploy_plan.rs
    - crates/ignition-core/tests/common/plan_fixture.rs
    - crates/ignition-cli/tests/contract_deploy.rs

key-decisions:
  - "Ledger v1, slug deploy_ledger_invalid (exit 6), flag vocabulary, verify ids and pull skip reasons: adopted as proposed (option-a), see Checkpoint outcome"
  - "removed means: not rendered AND at least one delete-set file (a ledger file that still exists in the mode collection). A ledger file that only survives in core makes the unit unmanaged, because there is nothing to delete"
  - "lockout_risk excludes unmanaged entries (see Deviations)"
  - "Ledger refusal text never echoes the foreign gateway or mode a copied ledger names; it states the target's own value instead"
  - "RelPath gained a Serialize derive (newtype, serializes as its string) so DeleteSet serializes as {files: [...]}"

patterns-established:
  - "PlanFixture::ledger(gateway, mode, files) writes a valid sorted ledger; PlanFixture::ledger_bytes writes raw bytes for corrupt-ledger tests; GATEWAY and MODE constants exported"
  - "deploy_plan.rs keeps a plan_units(tree, live) wrapper (no ledger, fixed scope) so the 22-01 engine tests are untouched"

requirements-completed: [DPLN-02, DPLN-03, DPLN-04]
requirements-advanced: [DPLN-01]

duration: 75min
completed: 2026-10-14
status: complete
---

# Phase 22 Plan 03: Declared-names ledger, delete set and plan flags Summary

**`ign deploy plan` now reads a strict on-box ledger (format v1) over the transport and states the exact delete set (ledger minus rendered, bounded to the mode collection and the current allowlist), reports `removed` and `shadows_removal`, flags destructive and lockout-risk entries, binds the set into `plan_sha256`, and refuses an unusable ledger with `deploy_ledger_invalid` (exit 6) instead of assuming it empty.**

## Checkpoint outcome

Task 1 was a `checkpoint:decision`. The user's response, recorded verbatim: **option-a**. Adopt all five as proposed:

1. Ledger v1:
   - Shape: JSON `{"schema_version":1,"gateway","mode","render_sha256":"<64 hex>","run_id":"<[A-Za-z0-9._-]{1,128}>","files":[...]}`.
   - Unknown fields are refused. The file is at most 8 MiB. `files` is byte-sorted and unique. No per-file hashes.
   - Location: `<data_dir>/ign-deploy/<gateway>/<mode>/ledger.json`, pending Decision B in 22-12.
2. Ledger refusal slug: `deploy_ledger_invalid`, exit 6 (target_state).
3. Plan entry flags:
   - `destructive` is true for `removed` entries and for any entry with delete-set files.
   - `destructive_reason` values:
     - `tag_data_loss`: ignition/tag-provider, tag-group, tag-type-definition, tag-definition.
     - `connection_removed`: ignition/database-connection, opc-connection, identity-provider.
     - `storage_removed`: ignition/store-and-forward-engine, alarm-journal.
     - Otherwise `resource_removed` for `removed` entries and `files_removed` for `changed` entries.
   - `lockout_risk` is true for every non-`same` entry of ignition/identity-provider or ignition/security-zone.
4. Verify:
   - Check ids: `scan`, `mode`, `read_back`, `files`, `loaded`, `probes`.
   - Statuses: `pass`, `fail`, `warn`.
   - Finding codes: `scan_active`, `mode_mismatch`, `resource_missing`, `resource_differs`, `files_differ`, `logs_unavailable`, `unloaded_resource`, `tag_probes_absent`, `tag_probes_unavailable`, `tag_probe_not_good`.
5. Pull `skipped[].reason`: `embedded_secret`, `not_round_trippable`, `render_refused: <value-free finding>`, `type_without_rule`, `denied_type`.

Plans 22-08 (item 5) and 22-10 (item 4) read their vocabularies from this section. Items 1 to 3 are implemented here exactly as listed, with the one `lockout_risk` clarification under Deviations.

## Reference values

- **`PLAN_GOLDEN`** unchanged: `a89de5043275c1b057289e89cdd78e05bc3d32680ce169e1d885e1a22708c473` (no ledger, so no `D` lines).
- **`PLAN_GOLDEN_LEDGER`** = `5084afccdf21436e192a5e71b8da3b4ba5cafa49fa822552ab393ffdc963f87a` (`tests/deploy_plan.rs`, `plan_sha256_golden_with_ledger`): the `PLAN_GOLDEN` scenario (one `changed`, one `added`) plus a removed unit `ignition/database-translator/OLD` whose two files are in the ledger. It equals an in-test recomputation from the documented text including the two `D` lines.
- **Entry key order (struct order):** `key, module, type, name, status, live_in, core, destructive, destructive_reason, lockout_risk, files[{path, live_sha256, desired_sha256}]`. 22-04 adds `drift` and `diff` after `files`.
- **`DeployPlanResult` key order:** `env, gateway, mode, node, transport, render_sha256, plan_sha256, summary, entries, delete_set, warnings` (22-04 adds `base`). `delete_set` is `{"files": [...]}`.
- **Ledger location:** `ign-deploy/<gateway>/<mode>/ledger.json` below the node's data dir (`ledger_location`).

## Accomplishments

- `deploy/ledger.rs`: `Ledger`, strict `Ledger::parse` (cap and empty check first, `node::classify` for UTF-8 / BOM / duplicate keys / depth, then explicit unknown-field and missing-field checks, then serde for types with value-free errors, then schema, gateway, mode, hash, run id and per-path rules), `Ledger::to_bytes` (pretty JSON plus LF, byte-identical round trip).
- `LocalFsTransport::read_ledger`: each level inspected with `symlink_metadata` (absent at any level is `None`; symlink, directory, or non-regular file is a refusal), bytes read only through `open_regular_file(data_dir, rel, before)` (rustix `openat`, `O_NOFOLLOW`), bounded by 8 MiB. An unreadable `data_dir` is `transport_failed`, never "no ledger".
- Plan engine: delete set, `removed`, partial deletes stay `changed`, `shadows_removal`, flags, `D` lines. `plan()` takes `Option<&Ledger>` and the manifest `Scope`; `deploy_plan` reads and parses the ledger after the tree read.
- CLI: human lines carry `destructive=<reason>` and `lockout_risk`, followed by `delete <path>` lines, with `plan_sha256` still last.
- README: `#### The declared-names ledger`, delete-set rules, flag table, `D` lines and `plan_stale` note; Commands row data keys; exit row 6 slug.

## Task Commits

1. **Task 1: contract vocabulary** - decision checkpoint, no code (see Checkpoint outcome).
2. **Task 2 RED: ledger tests and slug** - `cc2ce8a` (test; 21 of 24 ledger tests failed against the skeleton; `deploy_ledger_invalid` landed in all four places plus README here).
3. **Task 2 GREEN: strict reader and `read_ledger`** - `f91b995` (feat).
4. **Task 3: delete set, removed, shadows_removal, flags** - `5b12e6f` (feat).

## Verification

- `cargo test --workspace`: all green (core lib 671, `deploy_plan` 35, `contract_deploy` 20, `deploy_family_coverage` 4, `error::` 12).
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` clean.
- Plan acceptance greps: `pub fn ledger_location` (1), `const PLAN_GOLDEN_LEDGER` (1), `shadows_removal` in README (>= 1).

## Edge coverage

| Edge | Evidence |
|---|---|
| DPLN-03 ordering | `destructive_reasons`, `lockout_risk_follows_status_and_type`, determinism check in `ledger_change_changes_hash` |
| DPLN-04 adjacency | `delete_set_is_ledger_minus_rendered`, `partial_unit_delete_is_changed` |
| DPLN-04 empty | `no_ledger_no_removals`, `empty_ledger_files_valid`, `refuses_empty_file` |
| DPLN-04 encoding | `case_variant_is_excluded_with_warning` (ASCII fold excluded, `U+00C9` vs `U+00E9` not folded), `refuses_case_variant_duplicates` |
| DPLN-04 ordering / idempotency | `refuses_unsorted_files`, `refuses_duplicate_files`, `ledger_change_changes_hash`, `plan_never_writes_the_ledger` |
| DPLN-04 concurrency (backstop) | hash sensitivity proved in `ledger_change_changes_hash`; the `plan_stale` refusal is Phase 23 |

## Deviations from Plan

**1. [Rule 1 - Bug/judgement] `lockout_risk` excludes `unmanaged` entries**
- **Found during:** Task 3, writing `lockout_risk_follows_status_and_type`.
- **Issue:** the confirmed text says "every non-`same` entry" of `ignition/identity-provider` or `ignition/security-zone`. Read literally that includes `unmanaged` entries, which are live units the plan never touches. Every gateway has a live identity provider, so every plan would carry a lockout flag and Phase 23's flag-based gate would refuse every apply.
- **Fix:** `lockout_risk` is true for `added`, `changed` and `removed` entries of those types; `same` and `unmanaged` are false. One line in `plan.rs` (`lockout_risk`) and one README table row; reverting is a one-line change.
- **Needs confirmation:** this is an interpretation of an approved item. If the literal reading was intended, drop the `unmanaged` exclusion and the matching assertion in `lockout_risk_follows_status_and_type`.
- **Commit:** `5b12e6f`

**2. [Rule 3 - Blocking] `RelPath` derives `Serialize`; `DeleteSet` derives `Eq`.** `DeleteSet.files` is `Vec<RelPath>` and `Plan` derives `Eq`. Commits `f91b995`, `5b12e6f`.

**3. [Clarification] Meaning of `removed`.** The truth text says "not rendered, has a ledger file and has an effective live copy". Implemented as "has at least one delete-set file" (a ledger file that exists in the mode collection). A ledger file that survives only in `core` has nothing to delete, so that unit stays `unmanaged` instead of being reported `removed` with an empty delete set.

**4. [Extra enforcement] `Ledger::parse` also checks `render_sha256` (64 lowercase hex) and `run_id` (`[A-Za-z0-9._-]{1,128}`)**, which the confirmed format states but the plan's refusal list did not enumerate. Both have tests.

**Process note:** Task 3's engine was written before its tests (the tests were then added and run against it, and the golden value was pinned only after the independent recomputation agreed). Task 2 followed RED then GREEN. Test sensitivity for Task 3 was not mutation-checked.

**Requirement bookkeeping:** DPLN-02, DPLN-03 and DPLN-04 are marked complete (the verb, statuses, relations, flags and delete set are all delivered). DPLN-01 stays pending: its ssh half arrives with 22-06.

## Known Stubs

None added. The 22-01 stub in `transport::for_node` (the `ssh` arm) remains until 22-06; the `PlanStatus::Removed` and `CoreRelation::ShadowsRemoval` producers named in the 22-01 summary are now implemented.

## Threat Flags

None beyond the plan's register. T-22-07 to T-22-12 are mitigated and tested: strict reader and scope / mode-collection filters, gateway and mode match, ASCII case-fold exclusion, size cap before parse plus depth cap, `D` lines in `plan_sha256`, value-free refusal text (`refuses_another_gateway_without_echoing_it`, `refuses_a_wrong_field_type_without_echoing_it`).

## Not covered

- The ssh transport has no `read_ledger` yet (22-06 adds `SftpTransport`'s).
- Windows: `read_ledger` inherits the 22-01 residual (intermediate-directory swap race, std has no descriptor-relative open there); the path was not compiled for windows.
- The ledger location is provisional until Decision B (22-12 / 22-13).
- Nothing in Phase 22 writes a ledger, so only synthetic ledgers (tests, the CLI test seeding one) exercise the reader; a real apply-written ledger arrives with Phase 23.

## Self-Check: PASSED

- `ledger.rs`, `plan.rs`, `local.rs` present; commits `cc2ce8a`, `f91b995`, `5b12e6f` present in `git log`.

## Deviation confirmation

- 2026-10-14: User confirmed `lockout_risk` excludes `unmanaged` entries (Deviation 1 kept as implemented).
