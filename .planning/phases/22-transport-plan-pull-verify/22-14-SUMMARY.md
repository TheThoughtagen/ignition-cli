---
phase: 22-transport-plan-pull-verify
plan: 14
subsystem: deploy
tags: [oq5, hold-budget, timeouts, defender, decision-b, exit-gate, live-proof]

requires:
  - phase: 22-transport-plan-pull-verify
    provides: Windows SftpTransport and mirror_for_node (22-13), Decision B(i) staging, the capture host and its Modify grant on production
provides:
  - "oq5_timings: a capture-only #[ignore] harness driving the transport library and the gateway scan, with a pre-run hash listing and a post-run equality assertion"
  - "OQ5 runs A (Defender exclusion off) and B (on) and decisions OQ5-1..4 in 22-LIVE-CAPTURES.md"
  - "DEFAULT_OP_TIMEOUT_S 120 -> 1050 (constant, pinned tests, README together); DEFAULT_CONNECT_TIMEOUT_S stays 10"
  - "Phase 22 exit gate recorded in STATE.md and ROADMAP.md: Decision B(i) and the measured hold budget, which is above the ceiling"
affects: [23]

actuals:
  tokens: 40000
  tasks: 3
  commits: 7

key-files:
  created: []
  modified:
    - crates/ignition-core/tests/live_transport_wflwvm01.rs
    - crates/ignition-core/src/deploy/target.rs
    - crates/ignition-core/tests/deploy_transport.rs
    - README.md
    - .planning/phases/22-transport-plan-pull-verify/22-LIVE-CAPTURES.md
    - .planning/STATE.md
    - .planning/ROADMAP.md

key-decisions:
  - "Hold budget measured at 152 files: 505 s without a Defender exclusion, 699 s with one; both above the 300 s ceiling, recorded as an open item for Phase 23, not clamped"
  - "No Defender exclusion is recommended on timing grounds: no transport phase was faster with it; only the gateway-measured scan halved (about 300 ms)"
  - "op_timeout_s default is 1050 s (3 x the larger p95, 348.2 s, rounded up to 30 s); connect stays 10 s"
  - "Payload scale 1 (152 files), not the plan's ten: per-file costs make scale 10 about 75 min per iteration; 1,520-file figures are labelled extrapolations"

patterns-established:
  - "Capture harness restores the host from a recorded pre-run listing, and has a restore-only mode (IGN_OQ5_RESTORE_ONLY=<config-mode.json sha256>) for a run that was cut off"

requirements-completed: [DPLN-01]

duration: 15h
completed: 2026-10-15
status: complete
---

# Phase 22 Plan 14: OQ5 timings, hold budget, timeout defaults and the exit gate Summary

**The transport and scan were timed on the Windows capture host with and without a Defender exclusion; the measured hold budget (505 s and 699 s) is above the 300 s ceiling and goes to Phase 23 as an open item, the op timeout default moves from 120 s to 1050 s, and Decision B(i) plus the hold budget are recorded as the exit gate.**

## Commits

| Commit | What |
|---|---|
| 04a1c76 | test: `oq5_timings` harness (payload from the sanitized corpus or a D-16 pull; stage, commit, scan, delete, read; restore with hash equality) |
| ef922c9 | docs: `## OQ5 run A (Defender exclusion off)` |
| b02f74c | test: the restore retries transient ssh failures |
| 0299775 | feat: `DEFAULT_OP_TIMEOUT_S` 120 to 1050, pinned tests, README row |
| 7417052 | docs: `## OQ5 run B`, decisions OQ5-1..4, STATE and ROADMAP exit gate |

## Results (152-file payload, nearest-rank p95, seconds)

| Phase | A (no exclusion) p50 / p95 | B (exclusion) p50 / p95 |
|---|---|---|
| connect (n=20) | 1.575 / 1.688 | 2.010 / 2.423 |
| read (n=20) | 164.1 / 181.2 | 169.4 / 214.4 |
| stage (n=10) | 78.0 / 83.3 | 84.5 / 100.5 |
| commit (n=10) | 244.6 / 251.1 | 285.2 / 348.2 |
| scan wall (n=10) | 0.792 / 0.888 | 0.520 / 0.642 |
| delete commit (n=10) | 108.7 / 114.2 | 111.8 / 128.3 |

- **Hold budget** `ceil(2 x (p95 commit + p95 scan) + 1)`: A 505 s, B 699 s. Floor 10 s, ceiling 300 s. Both exceed the ceiling: an open item for Phase 23 planning (a 300 s hold would lapse mid-commit). Phase 23 should work from 699 s, the larger measured figure.
- **Defender:** the exclusion made nothing faster (commit p95 +97 s, read +33 s, stage +17 s). One A/B pair over a WAN, in different hours, with trial resets between: differences of this size are not distinguishable from noise, and the "Defender holds files and slows the commit" hypothesis is not supported. No exclusion is recommended on timing grounds.
- **Timeouts:** `DEFAULT_OP_TIMEOUT_S` = 1050 (3 x 348.225 = 1044.7, rounded up to 30 s). `DEFAULT_CONNECT_TIMEOUT_S` stays 10 (slowest of 60 sessions: 3.0 s).

## Deviations from Plan

**1. [Judgement, needs the user's eye] Payload is scale 1 (152 files, 91,833 bytes), not the plan's ten.** A calibration iteration showed per-file costs (stage 0.5 s, commit 1.6 to 2.3 s, delete 0.75 s, read 1.0 to 1.4 s), so 1,520 files would take about 75 minutes per iteration and about 12 hours per run for 10 iterations. The coordinator confirmed same-scale runs for a like-for-like A/B and ruled out a scale-10 probe. The required sample counts (10 iterations, 20 reads) were kept. The 1,520-file figures in the decisions are extrapolations from the per-file rates and are labelled as such. The payload contains named units only (singletons would override the live gateway's own settings), with uuids re-derived per copy.

**2. [Rule 3] Run B needed two attempts.** Attempt B1 died in iteration 5 on `exit code 255: Connection closed` (4 iterations recorded, host restored, not pooled). Attempt B2 completed all measurements, then its final restore step hit a transient ssh failure; the host was restored with the harness' restore-only mode (one more timeout, then success), and `remove_tree_retry` now retries that step. B2's summary is computed from its printed sample lines with the harness' nearest-rank rule (same script reproduces run A's table).

**3. [Rule 1] Pinned test updated.** `deploy_transport.rs::require_success_maps_every_failure_shape` asserted "timed out after 120s"; changed with the default.

**4. [Process] Trial resets.** The gateway trial was expired at the start. A manual scan on the expired trial completed normally, so scans behave the same expired or not. The coordinator reset it twice (2026-10-15T12:47:31Z during run A, after iteration 2 had run on the expired trial, and about 14:58Z before run B); I reset it once, at 2026-10-15T16:58:14Z, right after it expired during run B's B2, via `ign api call --method POST --path /data/api/v1/trial --profile azure-win`. All are recorded in the captures. The trial had about 2257 s left at 18:20Z, after the last run, and will expire again without further action.

**5. [Plan text vs code]** Used the typed `SftpOp`/`mirror_for_node` only; the write half has no CLI, action or MCP caller (`write_half_unreachable` stays green). `PLAN_GOLDEN` and `PLAN_GOLDEN_LEDGER` are unchanged. The harness stages with `default_staging_mode` (B(i)); there is no `InPlace` timing, as in 22-13 the in-place mode is not Decision B's choice.

## DPLN-06 not marked complete

DPLN-06 stays unchecked. This plan's `requirements` field is DPLN-01 only, and the live evidence for DPLN-06 so far is 22-13's pull of 91 files (7 skipped) followed by an empty render and plan on a rig-sized gateway; this plan produced no `deploy pull` evidence from a prod-sized source (the OQ5 source is `none`, 22-CONTEXT A-03). The scaled corpus payload is not a pull.

## Exit gate

- STATE.md: the Decision B blocker is replaced by a resolved line naming B(i) and the hold budget (505 s / 699 s, both above the ceiling) with the pointer to `22-LIVE-CAPTURES.md`, and a new open blocker for Phase 23 on the above-ceiling hold budget; WR-03 / R-03 stays marked resolved by 22-04; the stale "no write on the mode collection" blocker is marked resolved by the post-C6 grant (22-13).
- ROADMAP.md: the Decision B bullet now reads "B(i), recorded 2026-10-15" with the pointer (scoped single-line edit).

## What stays open or unproven

- Hold budget above the ceiling (Phase 23 planning must open with it). Whether directory-level renames or a pipelined batch would shorten the locked window is not answered by this data.
- The per-file cost of the Windows rename is not attributed (link latency, the 22-13 rename retry, or Defender are each possible).
- 1,520-file timings are not measured; a tree that size needs more than the 1050 s default and, at the top of the extrapolation, more than the 3600 s range cap.
- One A/B pair; no repeat.
- The Defender exclusions stay on the capture host as the user set them.

## Host state

Proven clean after each run and again at the end: `production` lists `config-mode.json` only (sha256 `f7b5b709abfadf5ad7338bd40ca14ebe4b4d123f4b2689112fa4cca36161b2fa`, 133 bytes; hash equality asserted by the harness), and `dir /b %USERPROFILE% | findstr /i ign-` prints nothing.

## Known Stubs

None.

## Threat Flags

None. T-22-59 (host left changed): mitigated, hash equality asserted. T-22-60: no pull source was used; the corpus is sanitized and only counts and timings were committed. T-22-61/62 accepted as planned.

## Test counts

`cargo test --workspace`: 2130 passed, 0 failed, 50 ignored (live). `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --all -- --check` clean.

## Self-Check: PASSED

- Commits 04a1c76, ef922c9, b02f74c, 0299775, 7417052 exist on main.
- `22-LIVE-CAPTURES.md` has `## OQ5 run A`, `## OQ5 run B` and decisions OQ5-1..4; no key text in it (`grep -c 'BEGIN OPENSSH'` prints 0).
- `.planning/config.json` and `.planning/research/.cache/` were never staged.
