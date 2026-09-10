# Phase 10 — SC-5 Live Gate Record (10-05)

**Status: `blocked-on-env`**

**Recorded:** 2026-09-10T07:05Z (plan 10-05 execution session)
**Gate:** `live_eam_write_lifecycle` in `crates/ignition-core/tests/live_gateway.rs` (commit `1293113`)
**Target rig:** the REAL WHK controller gateway (the roadmap names it specifically — a disposable rig is NOT an acceptable substitute for SC-5)

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
6. **SCHEDULED VOCABULARY:** the suspended scratch task must be ABSENT from
   `scheduled/false` (§2).
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

**None observable — no live run occurred.** The wire-shape expectations embedded in the
gate (sync `isSuspended`, §2 scheduled vocabulary, §3b no-confirm delete, §6a echo
modify, §1c trigger-registration latency) all cite 10-LIVE-CAPTURES.md verbatim. When
the gate runs, any drift (the controller answering a shape wiremock mocked differently)
must be recorded here as evidence; wire-shape FIXES belong in a follow-up gap plan, not
in this plan (the REQUEST pins' provenance chain stays intact).

## 5. Run record

_(empty — blocked-on-env; populated verbatim on the first provisioned run)_

## 6. SC-5 verdict

**NOT YET CLOSED — precisely blocked on user-provisioned env access.** Phase 10's fifth
success criterion ("at least one guarded write live-verified end-to-end against the
real WHK controller rig, recorded during the phase") is neither proven nor silently
skipped: the gate exists (compiles, clippy-clean, skips green without env), and the
single remaining unblock is:

1. Provision the two env vars (user_setup contract): `IGNITION_LIVE_URL` = the WHK
   controller gateway's base URL; `IGNITION_LIVE_TOKEN` = a WHK-controller API token
   with EAM rights (FULL `name:key` string).
2. Re-run: `cargo test -p ignition-core --test live_gateway live_eam_write_lifecycle -- --ignored --nocapture`
3. Append the per-step verbatim outcomes to §5 and flip the status to `passed` (or
   `failed-with-findings` — the orchestrator decides gap-closure).
