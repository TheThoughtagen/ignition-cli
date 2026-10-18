---
phase: quick/261018-lqg
plan: 01
status: complete
completed: 2026-10-18
actuals:
  tokens: 40000
  tasks: 3
  commits: 2
---

# Quick 261018-lqg: queued config save during apply (live rig gate)

One-liner: new rig gate `live_queued_config_save_during_apply` shows a config save queued during the apply's scan-lock hold is refused as stale (`resource_signature_rejected`) after the release scan, so the apply reports `applied` with the declared POSTGRES config on disk and in REST. No phase 23 gap.

## Outcome

- Gate verdict: PASS, branch `applied_write_refused` (22:09Z run; three runs after the Designer closed behaved the same).
- Queue proof (22:09Z): lock actor at send `api-token:cap-a` (engine token, no second token minted), same actor after the 3 s probe, the write unanswered at 3 s, answered 0.6 s after the hook returned (3.6 s after send), verdict `resource_signature_rejected`.
- Run: outcome applied, run 20261018T220910Z-585edd, result.json outcome applied ok true, on disk Declared, REST Declared, fresh `deploy verify` green.
- Four existing gates plus the new one, one sequential run 22:07Z to 22:09Z: live_apply_then_rollback 15.1 s, live_kill_after_green_verify_then_recover 9.0 s, live_kill_mid_write_then_recover 32.6 s, live_lock_lapse_restores 20.9 s, new gate 18.4 s. 5 passed, 107 s.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check` clean. No file under crates/ignition-core/src changed. No token in any log or doc (0 hits).

## Commits

- 12754c9 test(23): live gate for a config save queued during the apply hold
- c6f921b docs(23): record the queued config save rig test (C2b, U1)

## Deviations from Plan

1. Hold window widened with a hook after the engine's commit (3 s probe, hold 30 s), as the plan prescribes; recorded in 23-LIVE-CAPTURES.md.
2. [Rule 1 - harness bug] The first live runs reported `PHASE 23 GAP:` falsely: the gateway's REST view omits null config members (`createAutoIncSequence`, `createAutoIncTrigger`), so the strict equality made the REST classification `Other` although the fresh verify was green and the config matched. `classify` now compares with null members dropped on both sides. Test harness only, in 12754c9.
3. The findings block also prints the engine error text (the preflight refusal), a small addition.
4. The Designer preflight refused the first 12 attempts (a Designer session was open); waited, then the user closed it. `--designers proceed` was not used.
5. No second token was minted: the engine-token write was queued ("C2 not contradicted").

## Known limit

Only a queued write whose signature the release scan made stale was exercised. The 23-LIVE-CAPTURES.md conclusion states this.

## Self-Check: PASSED
