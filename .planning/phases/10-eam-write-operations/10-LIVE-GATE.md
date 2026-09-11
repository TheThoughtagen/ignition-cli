# Phase 10 — SC-5 Live Gate Record (10-05)

**Status: `failed-with-findings`** (gap-closure 10-06 re-run: env blocker resolved via the
UAT-recorded disposable-controller-rig substitution; both runs aborted at the §2 vanish poll)

**Recorded:** 2026-09-10T07:05Z (plan 10-05 execution session); re-run 2026-09-11 (plan 10-06)
**Gate:** `live_eam_write_lifecycle` in `crates/ignition-core/tests/live_gateway.rs` (commit `1293113`; 10-06 fix commits `cf33221` + `9f99ed5`)
**Target rig:** 10-05 originally named the REAL WHK controller gateway. **Superseded by the UAT-recorded decision (10-UAT.md test 12): the user stated WHK has no EAM, and approved the disposable-controller-rig substitution** — the re-run rode the disposable UAT rig `ign-uat-836` (8.3.6, port 18188, controller mode via the 10-RIG-NOTES flip recipe), per 10-06's plan contract. This substitution supersedes 10-05's WHK-only constraint.

---

## 1. Env confirmation — ABSENT (names only, never values)

The suite's env-gate convention (unchanged since Phase 4) was checked verbatim in the
execution shell:

| var | required by | state at record time |
|---|---|---|
| `IGNITION_LIVE_URL` | the whole gate — the WHK controller gateway's base URL | **UNSET** (shell, `.zshrc`/`.zshenv`/`.zprofile`, repo `.env` — all absent) |
| `IGNITION_LIVE_TOKEN` | the whole gate — an API token on the WHK controller with EAM rights, FULL `name:key` string | **UNSET** (same sweep) |

Both are the **user-provisioned** access contract (10-05 `user_setup`): Claude cannot
provision access to the WHK controller rig. Per the plan's Task-2 contract, the gate
was NOT pointed at a disposable rig and presented as WHK, and no run was fabricated.

## 2. Verbatim outcome of the attempted run

`cargo test -p ignition-core --test live_gateway live_eam_write_lifecycle -- --ignored --nocapture`

```text
running 1 test
skipping: IGNITION_LIVE_URL / IGNITION_LIVE_TOKEN not both set
test live_eam_write_lifecycle ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 12 filtered out; finished in 0.00s
```

EXIT=0. The gate is present, compiled, and its guard skipped honestly — a missing env
is a green no-op, never a false failure. (The default `cargo test` run likewise stayed
green: 13 ignored in the suite.)

## 3. What the gate RUNS once the env is present (per-step protocol)

Every step below is already implemented in the committed test; the run record (§5) will
carry each step's verbatim outcome.

1. **Controller-gate diagnostic first:** one cheap `scheduled/false` read; a 403
   "configured as a controller" fails immediately with the operator-facing message
   (rig-config error, not a code bug — headless flip recipe: 10-RIG-NOTES.md).
2. **CREATE** through the action layer (`actions::eam::eam_task_create`):
   `eam_backup`/OnDemand scratch task `ign-live-scratch-{epoch}`, `config.settings`
   present (the 422 trap), `targetGateways=["_controller"]` default.
3. **NAME ASSERTION + create read-back:** found record's name equals the scratch name;
   `profile.type == "eam_backup"`; `config.settings` present.
4. **FLIP to Scheduled + cron** — capture §6a verbatim full-record clone →
   `profile.scheduleMode="Scheduled"` + `profile.scheduleDetails="0/30 * * * * ?"` →
   array PUT with the original signature. (Suspend of an OnDemand task is the captured
   500, §1a — the trigger must exist; this is the capture-proven path to a suspendable
   task.)
5. **SUSPEND** through the action layer, retried up to 7×/30 s while the gateway
   registers the trigger (captured late-500 "Task could not be suspended", §1c);
   read-back asserts `config.profile.isSuspended == true` (Decision 1 persistence).
6. **SCHEDULED VOCABULARY:** the suspended scratch task must VANISH from
   `scheduled/false` — deadline-bounded poll, 10 s interval, ~90 s deadline
   (§2: the vanish takes ~48 s and a transient grace row
   `taskState="Suspended"` may still be listed immediately post-suspend;
   the poll tolerates it and only panics at the deadline — a single-shot
   absence check would false-fail on the grace row).
7. **RESUME** through the action layer: read-back `isSuspended == false`; the task
   reappears in `scheduled/false` (§2, retry tolerance for load).
8. **DELETE** through the action layer (find-derived signature, `?collection=core`,
   no `?confirm=` — lone-resource shape, §3b): outcome `success: true`.
9. **POST-DELETE PROOF:** find answers the config-resource 404 (`not_found`).
10. **Cleanup on every path:** `ScratchTaskGuard::Drop` best-effort deletes the
    scratch task (fresh runtime + fresh client); disarmed on the happy path; Drop-time
    cleanup notes print to the run log.

**Safety invariants enforced in test code (not convention):** the scratch name is
derived from the fixed `ign-live-scratch` prefix + an epoch suffix and is CREATED by
the test; the find + name assertion runs IMMEDIATELY BEFORE every write (flip, each
suspend attempt, resume, delete) — a misconfigured `IGNITION_LIVE_URL` pointing at a
production gateway can never touch a real task because the pre-write assertion fails
first. No environment-supplied task name is ever used for a write. NO force call
exists in the test — lifecycle writes only, so an expired trial cannot false-fail the
gate (phase pitfall 8).

## 4. Drift vs the wiremock expectations

**10-06 re-run drift (RECORDED — the gate never completed §2; both runs):**

**D1 — the suspended row never left `scheduled/false` within the 90 s poll deadline, twice.**
The §2 vanish step (10-06's poll-until-vanish fix) observed the scratch row present at
EVERY poll — always `taskState="Suspended"` (the grace-row shape) — through the full
~90 s deadline in run 1 (91.31 s, 10 polls) and the honest retry (94.19 s, 10 polls).
This CONTRADICTS the latency evidence the deadline was sized from:

- Original captures session (2026-09-09, FRESH 8.3.6 + 8.3.3 rigs): the suspended row
  was gone from `scheduled/false` within the <48 s window between suspend (09:58:51Z)
  and the next reads (09:59:39Z) — a single upper-bound observation, never a measured
  latency (10-LIVE-CAPTURES §2).
- UAT gate run (2026-09-10, this rig): the single-shot check ran ~20 s post-suspend —
  row still present (that abort is the gap 10-06 closes).
- 10-06 runs (2026-09-11, this rig, ~22 h uptime, accumulated history): row present at
  every poll through 90 s, TWICE — the ~48 s bound did not hold.

**Unresolved wire question:** the reconcile mechanics of `scheduled/false` post-suspend
(when/how the gateway drops the row — scheduler-trigger refresh? execution-history
processing? uptime/history-volume dependent?) are NOT known. Per this section's contract,
that is FOLLOW-UP GAP WORK: a dedicated capture session must measure the vanish
distribution (or its absence) across fresh + long-lived rigs before the gate's deadline
is re-sized — NOT a blind deadline bump (guesswork is forbidden here by the 10-06 plan).

**What did NOT drift (wire shapes all held, both runs):** create through the action layer,
the §6a full-record flip, suspend 204 + `isSuspended=true` persistence (Decision 1), the
grace-row shape itself (`taskState="Suspended"` listed in `scheduled/false` — now
firmly captured, captures §2), delete cleanup via the Drop guard, and post-run rig state
(zero scratch tasks before teardown). The 10-05 wiremock-pinned REQUEST shapes were never
contradicted.

---

**10-05 original section (no live run occurred then):**

**None observable — no live run occurred.** The wire-shape expectations embedded in the
gate (sync `isSuspended`, §2 scheduled vocabulary, §3b no-confirm delete, §6a echo
modify, §1c trigger-registration latency) all cite 10-LIVE-CAPTURES.md verbatim. When
the gate runs, any drift (the controller answering a shape wiremock mocked differently)
must be recorded here as evidence; wire-shape FIXES belong in a follow-up gap plan, not
in this plan (the REQUEST pins' provenance chain stays intact).

## 5. Run record

**Two runs recorded (2026-09-11, disposable controller rig `ign-uat-836`, 8.3.6, port 18188,
token staged outside the repo per RIG-NOTES hygiene — shredded at teardown).** The second
run is the ONE honest retry the 10-06 plan allows (run 1's failure was transient-shaped:
no HTTP/wire error, timing-variance signature only). Both failed at the same §2 poll —
the retry budget is spent; the drift is recorded in §4 (D1) and the follow-up named in §6.

### Run 1 — 2026-09-11T11:39:37Z (scratch `ign-live-scratch-1789126777`), 91.31 s, FAILED at §2

```text
running 1 test
gate step create: scratch task "ign-live-scratch-1789126777" created (eam_backup/OnDemand, action layer)
gate step flip: "ign-live-scratch-1789126777" is Scheduled with cron "0/30 * * * * ?" — waiting for the gateway to register the trigger (captured ~80 s, §1c)
gate step suspend: 204 + read-back isSuspended=true (previous state None)
gate step verify: grace-period row: taskState=Suspended still listed in scheduled/false — transient, UAT rig 2026-09-10
gate step verify: grace-period row: taskState=Suspended still listed in scheduled/false — transient, UAT rig 2026-09-10
gate step verify: grace-period row: taskState=Suspended still listed in scheduled/false — transient, UAT rig 2026-09-10
gate step verify: grace-period row: taskState=Suspended still listed in scheduled/false — transient, UAT rig 2026-09-10
gate step verify: grace-period row: taskState=Suspended still listed in scheduled/false — transient, UAT rig 2026-09-10
gate step verify: grace-period row: taskState=Suspended still listed in scheduled/false — transient, UAT rig 2026-09-10
test live_eam_write_lifecycle has been running for over 60 seconds
gate step verify: grace-period row: taskState=Suspended still listed in scheduled/false — transient, UAT rig 2026-09-10
gate step verify: grace-period row: taskState=Suspended still listed in scheduled/false — transient, UAT rig 2026-09-10
gate step verify: grace-period row: taskState=Suspended still listed in scheduled/false — transient, UAT rig 2026-09-10
gate step verify: grace-period row: taskState=Suspended still listed in scheduled/false — transient, UAT rig 2026-09-10

thread 'live_eam_write_lifecycle' (41578965) panicked at crates/ignition-core/tests/live_gateway.rs:896:9:
suspended scratch task must vanish from scheduled/false within ~90s (capture §2) — grace row seen: true; last observed taskState: "Suspended"
cleanup: scratch task "ign-live-scratch-1789126777" best-effort deleted (Drop path)
test live_eam_write_lifecycle ... FAILED

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 13 filtered out; finished in 91.31s
```

Per-step: create ✅ → flip ✅ → suspend ✅ (first attempt — no trigger-pending retries)
→ **§2 poll ❌ (grace row present at all 10 polls through the 90 s deadline)** — resume/
delete steps NOT reached by the gate. Cleanup ✅: the 10-06 Drop guard deleted the
scratch task on the FAILURE path ("best-effort deleted (Drop path)") — the runtime-safe
Drop fix live-proven; post-run `list` read confirmed zero scratch tasks.

### Run 2 (honest retry) — 2026-09-11T11:42:25Z (scratch `ign-live-scratch-1789126945`), 94.19 s, FAILED at §2

Identical signature, verbatim:

```text
running 1 test
gate step create: scratch task "ign-live-scratch-1789126945" created (eam_backup/OnDemand, action layer)
gate step flip: "ign-live-scratch-1789126945" is Scheduled with cron "0/30 * * * * ?" — waiting for the gateway to register the trigger (captured ~80 s, §1c)
gate step suspend: 204 + read-back isSuspended=true (previous state None)
gate step verify: grace-period row: taskState=Suspended still listed in scheduled/false — transient, UAT rig 2026-09-10
gate step verify: grace-period row: taskState=Suspended still listed in scheduled/false — transient, UAT rig 2026-09-10
gate step verify: grace-period row: taskState=Suspended still listed in scheduled/false — transient, UAT rig 2026-09-10
gate step verify: grace-period row: taskState=Suspended still listed in scheduled/false — transient, UAT rig 2026-09-10
gate step verify: grace-period row: taskState=Suspended still listed in scheduled/false — transient, UAT rig 2026-09-10
gate step verify: grace-period row: taskState=Suspended still listed in scheduled/false — transient, UAT rig 2026-09-10
test live_eam_write_lifecycle has been running for over 60 seconds
gate step verify: grace-period row: taskState=Suspended still listed in scheduled/false — transient, UAT rig 2026-09-10
gate step verify: grace-period row: taskState=Suspended still listed in scheduled/false — transient, UAT rig 2026-09-10
gate step verify: grace-period row: taskState=Suspended still listed in scheduled/false — transient, UAT rig 2026-09-10
gate step verify: grace-period row: taskState=Suspended still listed in scheduled/false — transient, UAT rig 2026-09-10

thread 'live_eam_write_lifecycle' (41615651) panicked at crates/ignition-core/tests/live_gateway.rs:896:9:
suspended scratch task must vanish from scheduled/false within ~90s (capture §2) — grace row seen: true; last observed taskState: "Suspended"
cleanup: scratch task "ign-live-scratch-1789126945" best-effort deleted (Drop path)
test live_eam_write_lifecycle ... FAILED

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 13 filtered out; finished in 94.19s
```

Per-step: identical to run 1 — create ✅ → flip ✅ → suspend ✅ (first attempt) →
**§2 poll ❌ (identical: 10 grace-row polls, deadline panic)**. Cleanup ✅ (Drop guard);
post-run list read: zero scratch tasks.

**Pre-flight both runs:** `/StatusPing` → `{"state":"RUNNING"}`; token sanity
(gateway-info 200); controller mode active (`scheduled/false` → 200 `{"items":[],…}`);
zero pre-existing EAM tasks. **Teardown after both runs:** `docker rm -f -v ign-uat-836`,
token env shredded (`shred -u`), zero `ign-uat*` containers/volumes, port freed, zero
scratch tasks at delete time (verified by the pre-teardown list read above).

## 6. SC-5 verdict

**STILL NOT CLOSED — the gate has now RUN on a real controller (twice, 2026-09-11) but
aborted at its own §2 vanish poll both times.** Phase 10's fifth success criterion
("at least one guarded write live-verified end-to-end") requires the FULL lifecycle
through resume → delete → not_found; the gate reached neither on either run. This is
precisely the honest-stop branch the 10-06 plan defined for a failing live step: drift
recorded (§4 D1), one honest retry logged (§5), no guesswork fix applied.

**What the 10-06 runs DID live-prove on the disposable controller rig** (both runs, no
wire-shape drift):

- The two 10-06 code fixes work as designed: the poll replaced the false-failing
  single-shot check (10 grace-row observations instead of one misleading assert), and
  the runtime-safe Drop guard deleted BOTH runs' scratch tasks on the FAILURE path —
  the UAT run's stranded-task failure mode ("Drop cleanup itself panicked") is gone.
- Through-suspend is live-proven END-TO-END through the action layer: create →
  Scheduled+cron flip → suspend 204 + `isSuspended=true` persistence (Decision 1),
  twice, first-attempt suspends both times.
- The grace-row shape (`taskState="Suspended"` in `scheduled/false`) is firmly captured.
- resume/delete themselves remain live-proven via the UAT CLI tests on this same rig
  (10-UAT.md test 12 note) — but NOT by this gate.

**Follow-up gap work (the §4 D1 contract):** a dedicated capture pass measuring the
`scheduled/false` post-suspend vanish behavior (distribution, or whether the row EVER
leaves on long-lived rigs) across fresh + long-lived controllers, then re-size the §2
deadline (or re-shape the check if the vanish is not a stable invariant). Only then does
the gate re-run for the SC-5 close. The env blocker is gone — rig access is now a
documented recipe (10-RIG-NOTES + the UAT-recorded substitution), not a user dependency.
