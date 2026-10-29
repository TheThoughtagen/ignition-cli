---
phase: 25-guarded-project-deploy
plan: 24
subsystem: auth
tags: [adopt, api-keys, write-permissions, security-levels, read-only-token, live-capture]
status: complete
gap_closure: true
gap_ids: [G-25-11]

requires:
  - phase: 25-guarded-project-deploy
    provides: ign adopt (session-tier mint, security-properties singleton merge), doctor --check-write probe
provides:
  - live capture of which writePermissions entry admits which API key (8.3.6 and 8.3.3), Decision dedicated-nested
  - adopt grants write only to named levels (default Authenticated/IgnCliWrite), never a bare Authenticated
  - ign adopt --read-only (key proven refused write), --write-level (repeatable), --restrict-write (opt-in removal)
  - adopt probe also runs scan/projects, so a key that cannot write fails at adopt time
  - README adopt section: read-only check token minting and verification, gateways adopted by earlier releases
affects: [25-25, 25-26 CI secrets table link, 25-27 live UAT check token]

actuals:
  tokens: 52000
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "Capture first: a pure decide rule, tested offline, turns a live matrix into a recorded Decision line that a const (WRITE_ADMISSION) mirrors"
    - "Write-side permission merge adds the exact level tree and never derives a root; read side keeps the bare-root merge"

key-files:
  created:
    - crates/ignition-core/tests/live_adopt_write_admission.rs
    - crates/ignition-core/tests/fixtures/live-adopt-rig/compose.yml
    - crates/ignition-cli/tests/contract_adopt.rs
  modified:
    - crates/ignition-core/src/client/adopt.rs
    - crates/ignition-core/src/actions/adopt.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-core/tests/live_rig_module_injection.rs
    - README.md
    - .planning/phases/25-guarded-project-deploy/25-LIVE-CAPTURES.md

key-decisions:
  - "Decision: dedicated-nested. On both 8.3.6 and 8.3.3 a nested write entry for the key's exact three-level path admits no key (S1), while the nested two-level entry Authenticated>IgnCapWrite admits exactly the keys granted it (S3). nested-exact therefore fails and the default write level is Authenticated/IgnCliWrite, granted to the minted key as well"
  - "A key minted by an earlier release lacks IgnCliWrite, so the existing-key check refuses it and the operator deletes and re-mints it (recorded as a costly behaviour change)"
  - "--restrict-write is opt-in and only removes bare Authenticated from writePermissions, refusing before any PUT when the list would become empty"

requirements-completed: [DPRJ-05]
---

# Phase 25 Plan 24: Adopt write scope and read-only check token Summary

`ign adopt` now writes only exact write levels into `writePermissions` (default `Authenticated/IgnCliWrite`, chosen by a live capture on 8.3.6 and 8.3.3), mints proven read-only keys with `--read-only`, and removes the old bare `Authenticated` write entry on request with `--restrict-write`, which closes G-25-11.

## Tasks

| Task | Name | Commit |
|------|------|--------|
| 1 (tracer) | Capture which writePermissions entries admit which API keys; Decision recorded | 648d1e8 |
| 2 | adopt grants write only to named levels, mints read-only keys, removes bare Authenticated on request | 83c21c0 |
| 3 | New adopt proven live end to end, CLI contract pinned offline, README adopt section | 7e44d59 |

## The capture (Task 1)

Six keys (Administrator, IgnCapRead, bare IgnCapWrite, nested Authenticated/IgnCapWrite and two combinations) were probed for read (projects list) and write (scan/projects rescan) in five permission states, on fresh gateways. Both images agree.

- Premises held: the commissioned write list is not open; with bare `Authenticated` added (today's adopt) the IgnCapRead key writes 200, which reproduces G-25-11; every admin session PUT succeeded in all six states.
- S1 (commissioned nested Administrator write entry only): even the Administrator key is refused (403). Pitfall 3 holds for a three-level nested entry, so `nested-exact` fails.
- S3 (nested `Authenticated>IgnCapWrite`): exactly the two keys holding `Authenticated/IgnCapWrite` write; the Administrator-only, IgnCapRead and bare-IgnCapWrite keys get 403.
- S2 (bare `IgnCapWrite`) also isolates correctly, but the nested form is preferred (adds no root-level name).
- S4 (bare `Authenticated`): every key writes.

`Decision: dedicated-nested` (single line in `25-LIVE-CAPTURES.md`, `### API key write admission (G-25-11)`).

## Verification

Rustup cargo first on PATH (`~/.cargo/bin`).

- `cargo test -p ignition-core --test live_adopt_write_admission -- decide_follows_the_rule_order`: pass
- Live capture 8.3.6 and 8.3.3 (`IGNITION_LIVE_ADOPT=1 ... live_adopt_write_admission`): both pass, both `CAPTURE decision: dedicated-nested`
- Ungated `--ignored` run: quiet skip, pass
- Live end to end 8.3.6 (`live_adopt_write_scope_end_to_end`): pass; (a) default key reads 200 and writes 200 with no bare Authenticated; (b) read-only key reads 200, writes 403, doctor write row Warn; (c) planted hole makes the read-only key write 200, the second read-only adopt is refused before minting naming `--restrict-write`, no orphan key; (d) `--restrict-write` removes it, read-only key 403, default key 200, admin PUT succeeds
- `cargo test -p ignition-core --lib adopt`: 16 passed (the seven planned tests plus the existing ones)
- `cargo test -p ignition-core --lib -- readme_exit_table_agreement exit_code_mapping_enumerated`: 2 passed
- `cargo test -p ignition-cli --test contract_adopt`: 4 passed
- `bash .planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/readme-test.sh`: all five checks PASS
- `cargo test --workspace --no-fail-fast`: 96 test binaries, 2927 passed, 0 failed, 88 ignored
- `cargo clippy --workspace --all-targets -- -D warnings`: pass; `cargo fmt --all -- --check`: pass
- `git diff --quiet HEAD~3 -- crates/ignition-core/src/error.rs crates/ignition-core/src/actions/doctor.rs`: unchanged; `docker ps -a` holds no `adopt-gate` container

## Deviations from Plan

None. The plan executed as written. Notes that are not deviations:

- The offline `decide_follows_the_rule_order` test sits in a `decide_tests` module inside the integration test binary, so its full name is `decide_tests::decide_follows_the_rule_order`; the plan's substring filter still selects it.
- The capture's `mint_key` also records an HTTP error on the create (not only `success: false`) as `create refused` data. No create was refused in either run.
- No post-PUT propagation delay showed up in the end-to-end test: the probe right after the permissions PUT answered correctly, so no probe retry was added.

## Known Stubs

None.

## Threat Flags

None. The new surface is the operator-requested edit of the gateway's security-properties singleton, already covered by T-25-78 to T-25-83. No key, hash or signature is printed by any test or capture line.

## Open items for later plans

- 25-26 should link the README `#### A read-only check token` section from the CI secrets table.
- 25-27 mints the UAT read-only check token with the new adopt and runs `ign doctor --check-write` against the UAT gateway.

## Self-Check: PASSED

Created files exist (`live_adopt_write_admission.rs`, `live-adopt-rig/compose.yml`, `contract_adopt.rs`), commits 648d1e8, 83c21c0 and 7e44d59 exist, and error.rs and doctor.rs are unchanged since 5074b84.
