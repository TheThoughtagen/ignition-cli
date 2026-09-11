---
status: complete
phase: 10-eam-write-operations
source: 10-01-SUMMARY.md, 10-02-SUMMARY.md, 10-03-SUMMARY.md, 10-04-SUMMARY.md, 10-05-SUMMARY.md
started: 2026-09-10T00:00:00Z
updated: 2026-09-10T18:20:00Z
---

## Current Test

[testing complete]

setup: |
  UAT rig provisioned (Claude, following the 10-RIG-NOTES recipe):
  - Docker rig `ign-uat-836` — Ignition 8.3.6 on http://localhost:18188, commissioned headlessly,
    API token `uattok` provisioned, EAM controller mode flipped, scratch task `ign-uat-scratch` created (eam_backup/OnDemand)
  - CLI profile `uat` (ACTIVE) — token via env var IGN_UAT_TOKEN; `ign` on PATH updated to the Phase 10 build
  - Token staging: `set -a; source /tmp/ign-uat/tokens.env; set +a` (chmod 600, outside repo)
  - Teardown when UAT finishes: docker rm -f -v ign-uat-836

## Tests

### 1. Guarded suspend refused without --yes (blast radius in refusal)
expected: Run `ign eam task suspend <existing-task>` without --yes: exits 2 pre-write, refusal message names the task + agent targets + controller impact, task state unchanged afterward
result: pass
evidence: "error: suspend ign-uat-scratch: suspends task ign-uat-scratch (eam_backup) — future scheduled dispatches to 1 agent stop until resumed targets: [_controller] pending: 0 is destructive; rerun with --yes to confirm" — task record verified enabled/unchanged after the refusal (request-counting golden also proves refusal traffic is reads-only)

### 2. Suspend executes with --yes (read-back proof)
expected: Run `ign eam task suspend <task> --yes`: command succeeds and output proves the suspension with read-back data (previous_state, isSuspended/config_suspended=true, pending, fired). `ign eam tasks` afterward shows the suspended state.
result: pass
note: scratch task flipped to Scheduled+cron (0/30) pre-test per capture §1a/§6a; wire verified after: config.profile.isSuspended=true synced into definition, taskState=Suspended

### 3. Resume executes with --yes
expected: Run `ign eam task resume <task> --yes` on the suspended task: succeeds with read-back proof (isSuspended=false); task reappears as scheduled/dispatchable in `ign eam tasks`.
result: pass
note: wire verified after: isSuspended=False written back, taskState back to Scheduled — bidirectional isSuspended sync proven (capture Decision 1)

### 4. Cancel reports honest no-op when nothing pending
expected: Run `ign eam task cancel <task> --yes` when no execution is pending: the command reports `fired: false` with a human-readable reason (e.g. "no pending execution") — a write we declined is reported honestly, never disguised as a dispatch.
result: pass
evidence: human mode printed "cancelled ign-uat-scratch  previous-state: -" + "not fired: no pending execution", EXIT=0; JSON mode data carries fired:false + reason verbatim; both binaries md5-identical (user paste had truncated the second line)

### 5. Modify changes targeted keys (full-record RMW)
expected: Run `ign eam task modify <task> --setting <K>=<V> --yes`: succeeds and output shows exactly the changed keys plus the NEW signature. Untouched keys survive (verify with `ign eam tasks <task>` detail view).
result: pass
note: user ran with --description; output "modified ign-uat-scratch  changed: description" + full new signature; wire verified: description changed, type/scheduleMode/cron/targetGateways untouched, CLI signature matches record byte-for-byte

### 6. Modify targeting nothing refused pre-network
expected: Run `ign eam task modify <task> --yes` with NO change flags (--enable/--disable/--setting/--schedule-mode/--description all omitted): exits 2 BEFORE any network request/prompt with a clear refusal (a no-op PUT would still rotate the server-side signature).
result: pass
note: refusal fired "invalid input: ... no targeted keys — a modify must change something (enabled / description / schedule-mode / settings overlay)"; NO profile line in output = refused pre-resolution, zero network. COSMETIC (observation): hint text mismatched — "fix the input source via --file / stdin" belongs to the --file error class, actual fix is adding a change flag

### 7. Unknown task name refused BEFORE the confirm prompt
expected: Run `ign eam task suspend nonexistent-task-name` (no --yes needed to see this): exits 6 not_found immediately — no "are you sure" confirmation prompt ever appears for a task that does not exist.
result: pass

### 8. Delete executes behind the guard, reports affected resources
expected: Run `ign eam task delete <scratch-task> --yes` on a disposable task: succeeds, output shows the deleted task name and `affected` resources the gateway reported; the task is gone from `ign eam tasks`.
result: pass
note: wire verified after — `ign eam tasks` shows "no EAM task definitions", find answers 404

### 9. Force without --yes: refusal names the blast radius
expected: Run `ign eam task force <task>` without --yes: exits 2, refusal message embeds the blast-radius preview (task + agent targets + impact) — force rides the SAME gate as the other five verbs.
result: pass
evidence: "error: force ign-uat-scratch: dispatches task ign-uat-scratch (eam_backup) now to 1 agent targets: [_controller] pending: 1 is destructive; rerun with --yes to confirm" — live pending count included, zero dispatches

### 10. TUI Confirm modal body IS the blast-radius preview
expected: In the TUI (Dashboard), invoke a guarded EAM verb (e.g. suspend) from the Actions menu: a Confirm modal opens whose body IS the blast-radius preview (task + agents + pending). The gate only arms AFTER the read-only preview fetch lands — a bad task name opens the error modal and arms NO confirmation gate.
result: issue
reported: "cuts off" (screenshot: modal opens, body line clipped at right edge mid-word — "eam_bac" — targets/pending tail unreadable)
severity: minor
note: gate behavior itself confirmed working — modal opened with the preview after the fetch (arming works); only the body text clips

### 11. README documents the EAM write contract
expected: README contains command-table rows for all five `ign eam task` verbs, a two-tier guard walkthrough, and the reconciliation note: no agent-level suspend/resume exists on the wire (tasks are the unit; affected agents ride targetGateways in the preview) and rename is NOT offered (create-new + delete-old composite documented).
result: pass

### 12. SC-5 live gate on WHK controller (env-gated)
expected: With IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN (WHK controller, full name:key) provisioned per 10-USER-SETUP.md, run `cargo test -p ignition-core --test live_gateway live_eam_write_lifecycle -- --ignored --nocapture`: the scratch-task lifecycle passes (create → scheduled flip → suspend with retry → isSuspended=true → resume → delete → not_found post-proof) and the run is appended to 10-LIVE-GATE.md §5. Alternatively: explicitly accept the blocked-on-env status.
result: issue
reported: "we don't have EAM on our setup - we would need to do this via docker compose automation" (WHK controller has NO EAM — original SC-5 premise unsatisfiable; user directs the docker-compose route)
severity: minor
note: gate run executed on the UAT disposable controller rig (8.3.6, user-approved substitution). WRITE LIFECYCLE LIVE-VERIFIED through suspend: create (action layer) → Scheduled+cron flip → suspend 204 + isSuspended=true — then the gate ABORTED at its own §2 assertion (single-shot absence check vs capture's ~48s vanish latency; this build also shows a never-captured grace-period row taskState=Suspended in scheduled/false). Resume/delete steps not reached by the gate (but live-proven via the UAT CLI tests on the same rig). Leftover scratch task cleaned manually (delete 200, find 404, segment empty). DECISION RECORDED: disposable-controller-rig substitution supersedes 10-05's WHK-only constraint — user-stated WHK has no EAM.

## Summary

total: 12
passed: 10
issues: 2
pending: 0
skipped: 0

## Gaps

- truth: "TUI Confirm modal body IS the blast-radius preview (task + agents + pending), fully readable"
  status: failed
  reason: "User reported: cuts off (screenshot shows the preview line clipped mid-word at the modal's right edge; targets/pending tail unreadable)"
  severity: minor
  test: 10
  root_cause: "render_modal Confirm arm (crates/ignition-tui/src/ui/mod.rs:137-147) renders the preview body as a single unwrapped Line in a Ratio(1,2)-width box; Paragraph has no .wrap(Wrap { trim: false }), and the height calc (line 108: body.lines().count()) counts raw lines, not display-wrapped rows — a ~140-char preview line clips at the box edge and any folded rows would overflow the computed height"
  artifacts:
    - path: "crates/ignition-tui/src/ui/mod.rs"
      issue: "Confirm Paragraph lacks .wrap(); modal width Ratio(1,2) too narrow for blast-radius lines; height calc ignores wrapping"
  missing:
    - "Add .wrap(Wrap { trim: false }) to the Confirm modal Paragraph"
    - "Account for display-wrapped row count in the Confirm modal height (or widen the Confirm box / give the preview its own wrapped multi-line body composed by update.rs)"
  debug_session: "diagnosed inline during UAT (render site read; no debug agent needed)"

- truth: "Live gate completes the full scratch lifecycle: create → flip → suspend → §2 vocabulary check → resume → delete → not_found post-proof"
  status: failed
  reason: "Gate aborted at its own §2 assertion — 'suspended scratch task must vanish from scheduled/false' — checked single-shot immediately after suspend; capture evidence records the vanish at ~48s latency, and this build shows a never-captured grace-period row (taskState=Suspended still in scheduled/false). The guarded writes themselves succeeded live."
  severity: minor
  test: 12
  root_cause: "live_gateway.rs:828-835 asserts absence from scheduled/false in ONE read immediately post-suspend — stricter than the capture's own evidence (10-LIVE-CAPTURES §2: suspend 09:58:51Z → vanish observed 09:59:39Z ≈ 48s). Secondary: ScratchTaskGuard::drop (line 646) panics 'Cannot start a runtime from within a runtime' when unwinding drops it inside the test runtime — it catches itself but cannot clean up remotely."
  artifacts:
    - path: "crates/ignition-core/tests/live_gateway.rs"
      issue: "§2 assertion is single-shot with no polling window; Drop guard cannot clean up when dropped during unwind inside the runtime"
  missing:
    - "Replace the single-shot §2 check with a poll-until-vanish window (~90s deadline, matching captured latency), tolerating the transient taskState=Suspended grace row"
    - "Extend the §2 vocabulary record with the newly observed grace-period row shape (provenance: UAT rig 2026-09-10)"
    - "Make ScratchTaskGuard::drop cleanup work when dropped inside the test runtime (spawn detached task on the current runtime as fallback)"
  debug_session: "diagnosed inline during UAT (assertion site + capture provenance read; no debug agent needed)"

## Observations

- (test 3 follow-up, user re-ran resume on the already-resumed task): gateway answered `500: Task could not be resumed`; CLI surfaced the gateway error page verbatim as `internal error` with -vv hint. Uncaptured edge (captures only prove resume-on-suspended §1b); behavior is loud-but-honest, no state corruption (isSuspended stayed False). Candidate minor polish: a symmetric already-resumed precheck like suspend's, if ever capture-proven. NOT a gap unless user flags it.
- (test 6, cosmetic): `eam task modify` no-targeted-keys refusal carries a mismatched hint ("fix the input source — a readable file path via --file, or `-` to pipe the content on stdin") — that hint belongs to the --file/stdin input class; here the fix is "add a change flag". Error line itself is clear and pre-network. One-line hint-text polish candidate.
