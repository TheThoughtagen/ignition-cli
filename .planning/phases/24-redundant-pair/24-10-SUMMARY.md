---
phase: 24-redundant-pair
plan: 10
subsystem: testing
tags: [ignition, redundancy, live-gates, deploy-apply, deploy-verify, secret-names, tap]
status: complete

requires:
  - phase: 24-redundant-pair
    provides: "24-01 pair rig; 24-02 contract decisions K3..K9; 24-05..24-09 BackupNode, converge_backup, preflights, deploy verify --node"
provides:
  - "live_deploy_pair.rs: live_pair_converges, live_pair_unsynced_then_reconverge, live_pair_secret_names, MethodTap, Pair, PAIR_LOCK, guarded cleanup"
  - "setup-pair.sh secrets-rm NODE PROVIDER NAME"
  - "24-LIVE-CAPTURES.md ## Live gates (24-10): three gates, installed ign, full suite, one finding"
affects: [phase-24-verification]

actuals:
  tokens: 98000
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "a loopback HTTP tap in front of the backup records method and path only and forwards GET and HEAD, so a live gate proves the backup write-free"
    - "gate body runs in its own task; cleanup (sweep gw-a, scan, restore the link, wait settled, wait for gw-b to drop the units) runs even when an assertion fails"
    - "the link is cut at the engine's lock_acquired event by an ApplyObserver that shells out to the rig script"

key-files:
  created:
    - crates/ignition-core/tests/live_deploy_pair.rs
  modified:
    - crates/ignition-core/tests/fixtures/live-rig-pair/setup-pair.sh
    - .planning/phases/24-redundant-pair/24-LIVE-CAPTURES.md

key-decisions:
  - "K3 b with K4 a: the unsynced gate cuts the link (toggleApproval false on gw-a) at lock_acquired, after every preflight, because a cut makes both nodes Active and master_active refuses that"
  - "K6 a only on sync-exhausted and K5 d: sync-exhausted is unit-tested only, not live"
  - "Secret-name red state is made by removing a replicated name (secrets-rm), not by creating it on one node, because a name made on gw-a reaches gw-b in 0-5 s (C-P7)"

patterns-established:
  - "A live fixture resource.json carries only uuid and enabled in attributes: a lastModification block with no valid signature is re-saved by the backup alone (see Findings)"

requirements-completed: []

duration: not timed (live gates ran 2026-10-21T02:28Z to 03:02Z; full suite waited on cargo lock contention)
completed: 2026-10-21
---

# Phase 24 Plan 10: Live gates for the redundant pair Summary

**On a real 8.3.6 two-node pair, an apply converges the backup with the backup proven write-free by a method tap, an apply with the link cut at lock_acquired ends `deploy_backup_unsynced` without touching the master and the pair re-converges, `verify --node both` turns red on a missing secret name, and the installed `ign` applies and verifies the pair.**

## What was built

- **Task 1 (tracer, 87bdc3b, approved by the user):** `live_deploy_pair.rs` with the guards (`IGNITION_LIVE_PAIR=1`, loopback URL guard with its two unit tests, name guards for `ign-pair-a` and gw-b's recorded name, `PAIR_LOCK`), `Pair`, `MethodTap`, `repo_for_pair`, the cleanup guard and `live_pair_converges`.
- **Task 2 (b402aac):** `live_pair_unsynced_then_reconverge` and `live_pair_secret_names`, `setup-pair.sh secrets-rm`, the captures for both, and the fixture fix described under Deviations 8.
- **Task 3 (the closing docs commit and the captures it carries):** the installed binary run (`### installed ign`) and the full-suite record (`### full suite`).

## Live gate results (rig `ign-pair-rig`, gw-a master 18300 `ign-pair-a`, gw-b backup 18301)

| Gate | Result | Time | Verdict fields |
|---|---|---|---|
| `live_pair_converges` | PASS | 15.4 s in the combined run (18 s first run) | outcome applied, ok, backup converged, `waited_s` 0, `files_checked` 4, files check pass, tap GET 12 and nothing else, `verify --node both` green |
| `live_pair_unsynced_then_reconverge` | PASS | 127.3 s combined run (125.3 s alone) | cut at `lock_acquired` took 60.2 s; outcome `applied_backup_unsynced`, ok false, slug `deploy_backup_unsynced`, exit 6, reason `not_syncing`, `waited_s` 40 of a 20 s budget, one resync, no restore event, master record `applied`, gw-a files equal the render; link restored in 12 s, settled 3 s later, backup converged at the first poll; tap GET 32 |
| `live_pair_secret_names` | PASS | 24.9 s combined run (25.3 s alone) | green with the name on both nodes; red after removal naming `CAP24_GATE_SP/<name>` on master and backup (`deploy_verify_failed`); green again; canary value absent; tap GET 28 |
| installed `ign` (`cargo install --path crates/ignition-cli --force`) | PASS | 7 s apply | plan exit 0; apply `--backup-timeout 60` exit 0, `data.outcome` applied, `data.ok` true, `backup.converged` true, `files_checked` 4; `verify --node both` JSON and human both exit 0, backup converged, secrets pass on both nodes |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS | 10 min 1 s (cargo lock contention) | clean |
| `cargo test --workspace --no-fail-fast` | 2642 passed, 1 failed (known, unrelated), 72 ignored | about 1 h 50 min (lock contention) | the failure is `live_gateway::guard_drop_during_unwind_inside_runtime_still_cleans_up`, the same one 24-09 saw |

The three gates also passed together in one `--test-threads=1` run (192.9 s, 02:56:23Z to 02:59:11Z). After every gate `setup-pair.sh status` showed `gw-a Master Active`, `gw-b Backup Cold`, both `Good production`.

K3/K4 path used: K3 b (cut at `lock_acquired`), K4 a (toggleApproval false on gw-a). The `link-net` / `cut-net` fallback was not needed.

## Verification

- `cargo test -p ignition-core --test live_deploy_pair` offline: the guard unit tests pass, the live tests are ignored.
- Live gates as in the table.
- `grep -cE 'IGN_PAIR_TOKEN_[AB]=[^$]|X-Ignition-API-Token: [A-Za-z0-9]'` on `24-LIVE-CAPTURES.md` prints 0; on this file it prints 0.
- Workspace suite: `cargo test --workspace --no-fail-fast` (run once, after the live gates): 2642 passed, 1 failed, 72 ignored; the one failure is `live_gateway::guard_drop_during_unwind_inside_runtime_still_cleans_up`, known and unrelated (note only). `cargo clippy --workspace --all-targets -- -D warnings`: clean.

## Deviations from Plan

1. **[Override, user] K3 = b, K4 = a.** The unsynced gate cuts at `ApplyEvent::LockAcquired` through a `CutAtLock` observer, not before the apply. The link is restored by the cleanup guard even on failure and the pair is waited to settled before the next gate.
2. **[Override, user] K6 = a only on sync-exhausted, K5 = d.** The sync-exhausted path is unit-tested only, not live.
3. **[Rule 1 - Bug] The tap resolved `localhost` to `::1` first;** the rig publishes `127.0.0.1` only, so the first request failed with `IncompleteMessage`. The tap now takes the first resolved address that accepts. Commit 87bdc3b.
4. **[Rule 3 - Blocking] `setup-pair.sh secrets-rm`.** A name made on gw-a replicates to gw-b in 0-5 s (C-P7), and the secrets preflight refuses an apply that references a name a node lacks, so the red state is made by removing a replicated name. Commit b402aac.
5. **[Detail] gw-b's `gateway-info.name` reads the master's name (C-P4),** so the name guard accepts `ign-pair-a` or `ign-pair-b`.
6. **[Process] Rig bring-up took 9 min** (00:21:11Z to 00:30:13Z), not 20+, and the first test compile about 22 min under cargo lock contention.
7. **[Format] rustfmt expanded the test file to 1346 lines.**
8. **[Rule 1 - Bug, test fixture] The secret gate's `oauth2-client` unit first failed to render** (`resource.json is not in gateway format`: no `attributes.uuid`), and after a uuid was added with a `lastModification` block the apply ended `applied_backup_unsynced` with `content_differs` (see Findings). The fixture now carries only `uuid` and `enabled` in `attributes`, which converges. Three failed gate runs (02:35Z, 02:36Z to 02:40Z, 02:47Z to 02:52Z) preceded the green run; each cleaned up and left the pair settled.
9. **[Addition] The leftover rig state of the secret gate:** provider `CAP24_GATE_SP` and the file entry `CAP24-Gate-<epoch>` stay on the rig (the gate leaves the name present at the end). The rig is disposable.
10. **[Process] Tracer gate:** stopped after Task 1 for the interactive checkpoint; the user approved ("tracer verified").
11. **[Process] The plan's trial-licence time (about 02:22Z) had passed when this run started (02:28Z).** The gateways still served REST and replication, so no trial reset was needed.

## Findings (open, for the verifier and the user)

**(Resolved after the plan: see "Post-plan gap fix (user-approved K2 revision)" below.)** **A `resource.json` whose `attributes` hold a `lastModification` block without a valid `lastModificationSignature` never converges on the backup.** gw-b re-saves such a unit alone (`lastModification.actor` becomes `external`, a new signature is written) while gw-a keeps the text as written, so the `files` check fails on that one `resource.json` and an apply ends `applied_backup_unsynced` with reason `content_differs` although both nodes hold the same configuration. Reproduced by hand in 8 s. A resource with only `uuid` and `enabled`, and the tracer's properly signed units, converge. A repo whose `resource.json` was edited by hand and kept a stale signature would hit this. `FILE_NORMALIZATION` (one row, `resource.json`, whole-document JSON equality) does not cover it; a fix would be a normalization that ignores `attributes.lastModification` and `attributes.lastModificationSignature` for the comparison. This is a product decision and was not made here; the gate fixture avoids the trigger and the evidence is in `24-LIVE-CAPTURES.md` under `### Finding:`.

## Auth gates

None.

## Known Stubs

None.

## Threat Flags

None. The tap and the CLI run add no surface beyond the plan's threat model (T-24-35 to T-24-38 held: guards before the first request, every tap record is GET, output carries counts and ids only, the temporary ign config names `token_env` variables and holds no value, the captures grep is 0).

## Requirements

DPAIR-01..04 are not marked complete here (hard rule of this execution): phase completion does that.

## Rig state and teardown

The rig is left UP and settled (`gw-a Master Active true false ... Good production`, `gw-b Backup Cold true false ... Good production`). Teardown command (not run): `docker compose -p ign-pair-rig down -v`.

## Self-Check: PASSED

- FOUND: crates/ignition-core/tests/live_deploy_pair.rs, crates/ignition-core/tests/fixtures/live-rig-pair/setup-pair.sh, .planning/phases/24-redundant-pair/24-LIVE-CAPTURES.md (`## Live gates (24-10)`, `### live_pair_converges`, `### live_pair_unsynced_then_reconverge`, `### live_pair_secret_names`, `### installed ign`, `### full suite`).
- FOUND commits: 87bdc3b, b402aac, and the Task 3 captures commit (see git log).
- No `todo`, `fixme`, `.only` or skipped test in the new test file; the three live tests are `#[ignore]` by design and skip quietly without `IGNITION_LIVE_PAIR=1`.
- Token grep on the captures and this file prints 0.

## Post-plan gap fix (user-approved K2 revision)

No PLAN.md; done before phase verification at the user's request, on the main checkout, no worktree.

**Decision.** K2 was "`resource.json` compared as parsed JSON with no member excluded". It is now: compare `resource.json` as parsed JSON while ignoring ONLY `attributes.lastModification` and `attributes.lastModificationSignature`. Every other member of `resource.json` and every other file (sha256 byte-equal) is unchanged; the exclusion was not widened.

**Why.** The 24-10 finding above: a `resource.json` with a `lastModification` block and no valid signature is re-saved by gw-b alone (actor `external`, new signature), so the `files` check failed and an apply ended `applied_backup_unsynced` / `content_differs` while both nodes held the same configuration.

**What changed.**
- `crates/ignition-core/src/deploy/pair.rs`: `FILE_NORMALIZATION` is now `&[(&str, &[&str])]`, one row `("resource.json", ["attributes.lastModification", "attributes.lastModificationSignature"])`; the members are dotted paths removed by a small `remove_member` helper before the parsed-JSON compare. A missing `attributes`, a non-object `attributes` or a non-object document removes nothing and compares as before (no panic). Doc comments on `FILE_NORMALIZATION` and `compare_files` state the two ignored members.
- `README.md` (the `deploy verify` backup `files` check text) states the two ignored members. No README text said "no member excluded".
- `crates/ignition-core/src/deploy/pair/tests.rs`: `only_resource_json_is_compared_as_a_document` pins the new row; new tests `resource_json_ignores_only_the_two_modification_stamp_members`, `resource_json_still_differs_on_every_other_member` (enabled, uuid, top-level member, extra member, a nested `lastModification`, a top-level `lastModification`), `resource_json_without_an_object_attributes_is_compared_as_before` (absent and non-object attributes, non-object document) and `other_files_do_not_ignore_the_modification_stamp`.
- `crates/ignition-core/tests/live_deploy_pair.rs`: new gate `live_pair_converges_with_a_stale_signature`; the comment in `repo_for_pair` now points at it. The secret-gate fixture was left as it is (its `resource.json` has only `uuid` and `enabled`); making it realistic would add a live run for no new coverage.

**Commits.** 9106cc9 `test(24-gap)` (RED: `resource_json_ignores_only_the_two_modification_stamp_members` failed, the others held), 41ab6e0 `fix(24-gap)`, 3acf4b0 `test(24-gap)` live gate, plus the docs commit with this section.

**Verification.**
- `cargo test -p ignition-core --lib deploy::pair`: 24 passed.
- `deploy_apply` 130 passed, `deploy_verify` 60 passed, `contract_deploy` 72 passed, `deploy_family_coverage` 8 passed (ignition-cli), `live_deploy_pair` non-ignored 2 passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean. The full workspace suite was not run (about 2 h under load).
- Live: the new gate PASS in 16.0 s alone and 15.4 s in the 4-gate run (all four gates PASS, 258.90 s); negative control with the fix reverted: FAIL `applied_backup_unsynced` / `ContentDiffers` after 247 s. Evidence in `24-LIVE-CAPTURES.md`, `### Post-plan gap fix: live proof of the K2 revision (stale signature)`.
- Rig left UP and settled (`gw-a Master Active`, `gw-b Backup Cold`, both `Good production`); the trial licence had not lapsed.

DPAIR-01..04 are not marked complete here; phase completion does that. Token grep (`IGN_PAIR_TOKEN_[AB]=[^$]`) on the captures and this file prints 0.
