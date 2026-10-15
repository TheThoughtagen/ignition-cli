---
phase: 22-transport-plan-pull-verify
verified: 2026-10-15T19:00:00Z
status: human_needed
score: 5/5 roadmap success criteria verified (10/10 requirement IDs satisfied)
behavior_unverified: 0
overrides_applied: 0
re_verification: false
gaps: []
deferred:
  - truth: "Hold budget (505 s without a Defender exclusion, 699 s with one) exceeds the 300 s ceiling"
    addressed_in: "Phase 23"
    evidence: "Phase 23 SC-3 'Apply writes inside the scan-lock within the hold budget measured in Phase 22'; STATE.md carries it as an OPEN blocker 'Phase 23 planning must open with it'. Phase 22's gate asks for the budget to be measured and recorded, which it is."
human_verification:
  - test: "Push main and confirm the windows-latest leg of ci.yml is green"
    expected: "ignition-core and ignition-cli compile and `cargo test --workspace` passes on Windows, including the cfg(windows) branches (remote.rs restrict_to_owner via icacls, NULL_DEVICE = NUL, process.rs, tree.rs, deploy_transport.rs:657)"
    why_human: "The branches cannot be built on this Mac (x86_64-pc-windows-msvc is listed by rustup but its std is missing for the active toolchain; `cargo check --target` fails with E0463). origin/main is 87 commits behind HEAD, so CI has not yet run on any Phase 22 code. This is the Windows RUNNER as ssh client. SC-1's Windows HOST proof was driven from macOS and does not cover it."
  - test: "Run `ign deploy verify` against a gateway whose manifest has `verify.tag_probes[]` and whose `ign-cli` WebDev bundle is deployed (one probe Good, one Bad)"
    expected: "probes check passes for the Good tag, fails with `tag_probe_not_good` for the Bad one, exit 6 on red"
    why_human: "DPLN-08 has wiremock-only evidence. The rig used in 22-10 had no WebDev bundle, so the live runs show `tag_probes_absent`. The code reuses `tags_read`, which has live evidence from earlier phases, so this is a recommendation and not a suspected defect."
---

# Phase 22: Transport, Plan, Pull & Verify - Verification Report

**Phase Goal:** A user can see exactly what a deploy would change on the live master, bootstrap a declared tree from a running gateway, and verify a gateway against its tree, all read-only, over a transport whose write half is built and tested here but not yet called by any verb.
**Verified:** 2026-10-15 at HEAD 62cb9c8
**Status:** human_needed. There are no gaps. Two unrun checks remain (above), plus one deferred item for Phase 23.
**Re-verification:** No, initial verification.

## Goal Achievement

### Observable Truths (ROADMAP success criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | System-OpenSSH read of the config tree, proven on a real Windows OpenSSH host, plus local-FS; batch mode, pinned host key, per-call timeouts, argv-only, client-side hashes; write half passes tests on a scratch dir and no verb calls it | VERIFIED | `remote.rs::hardened_options` emits `BatchMode=yes`, `StrictHostKeyChecking=yes`, `UserKnownHostsFile=<repo known_hosts copy>`, `IdentitiesOnly`, `ForwardAgent=no`, `ClearAllForwardings=yes`, `ControlMaster=no`, `ConnectTimeout`, ServerAlive, in a pinned order. `process.rs:170` sets `kill_on_drop(true)` and applies a per-call timeout. `RemoteCommand` is the only constructor of remote command text. `SftpTransport`'s TreeReader impl (sftp.rs:1412-1729) issues no Put/Rename/Rm/Mkdir; every write op sits in the later `TreeMirror` impl. `grep mirror_for_node/TreeMirror` outside `deploy/transport/` and `tests/` returns nothing. `write_half_unreachable` (deploy_transport.rs:2702) scans actions/deploy.rs, main.rs and mcp.rs for `TreeMirror`, `mirror_for_node`, `StagingMode`, `.stage(`, `.commit(` and is green. Live: 22-13 ran 5 `#[ignore]` tests green (494.9 s) against the Azure Windows Server 2022 / Win32-OpenSSH 8.1 host, covering read, scratch write half, PowerShell exec, forced timeout ("timed out after 1s and was killed") and 313-char-path/junction fail-closed. 22-LIVE-CAPTURES.md holds C7a, C6, C6-post, C5-NTFS and OQ5. The host is an Azure VM instead of WFLWVM01 under the user-approved CONTEXT amendment A-01/A-02. |
| 2 | `plan` makes zero writes and takes no lock, exits 0, lists added/changed/removed/same with `has_changes` and `plan_sha256`; `core` relation, destructive and lockout flags; exact delete set that never contains `config-mode.json` or an undeclared resource | VERIFIED | `plan.rs::plan()` is pure. `deploy_plan` takes `&dyn TreeReader` and no gateway session. The CLI help says "resolves no gateway session". `has_changes = added+changed+removed > 0`. `CoreRelation` is none/identical/differs/hidden/shadows_removal. `destructive`/`destructive_reason` and `lockout_risk` are derived from status, delete-set membership and type. `delete_set()` rejects any ledger path with a dot-named component or a `GATEWAY_OWNED_FILES` member (`config-mode.json`), anything outside the manifest scope, anything rendered, anything not in the mode collection, and case-fold collisions. No ledger gives an empty set, and a corrupt ledger refuses (`deploy_ledger_invalid`). `unmanaged` (D-06) is never deleted and never counts in `has_changes`. `canonical_text` is golden-pinned (`PLAN_GOLDEN`, `PLAN_GOLDEN_LEDGER`). `plan_takes_no_lock_live` is green on the rig. |
| 3 | `--base` marks drift; `pull` bootstraps `gateways/<name>/` without writing to the gateway; `render` then `plan` reports no changes | VERIFIED | `BaseTree::read` re-hashes the base and refuses a modified, incomplete or foreign one (R-03 closed). `apply_base` sets `drift`, which is excluded from the hash. `deploy_pull` uses `TreeReader` only, and the `--yes` guard is in the action (and in `GUARDED_OPS`, "deploy pull", main.rs:4047). **Live, twice:** (a) 22-09 on the 8.3.6 Docker rig: pull wrote 95 files (7 skipped with reasons), then plan gave `added 0 changed 0 removed 0 same 48 unmanaged 1, has_changes false`, and every unmanaged entry is asserted to be a skipped unit. (b) 22-13 on the Windows host over ssh: pull wrote 91 files (7 skipped), then plan gave `same 48 unmanaged 1`, `has_changes false`. |
| 4 | `verify` on the master checks scan complete, `deploymentMode` = target, every declared resource reads back equal, no "will not be loaded" for a declared resource, every `tag_probes[]` Good (absent list = warning); red returns the frozen envelope with findings in `data`, exit 6, README dated exception | VERIFIED (probes: hermetic evidence only, see human item 2) | `verify.rs::evaluate` always returns the six checks (scan, mode, read_back, files, loaded, probes). `ok` is false exactly when a check is `fail`. `check_probes`: empty list is `Warn` with `tag_probes_absent`, a non-Good or missing reading fails, an unreadable route fails (`tag_probes_unavailable`). `deploy_verify` calls only `scan_status`, `deployment_mode`, list/singleton reads, `reader.read`, logs and `tags_read` (no lock, no scan trigger, no write). The slug `deploy_verify_failed` is not a `CoreError`. main.rs:572 maps `DeployVerify.failure_exit_code()` to exit 6 after the envelope. README has the dated section "The `deploy verify` verdict exception (2026-10-14)" (line 102). Live on the rig: `verify_green_then_red_live` (22-10): green on a scanned tree, red with `unloaded_resource` and `resource_differs`. 45 `deploy_verify` tests cover probes, canaries and `ForbidWrites`. |
| 5 | `plan`, `pull`, `verify` land with TUI rows, README entries, exit-table rows (DSURF-01 coverage green), MCP catalog; output including `plan --diff` is under the secret canary | VERIFIED | TUI rows `deploy plan`, `deploy pull`, `deploy verify` (routes.rs:487-495). README Commands rows (lines 373-375) and sections. `transport_failed` (exit 4) and `deploy_ledger_invalid` (exit 6) are in the README exit table. MCP: `deploy_plan` and `deploy_verify` are unguarded catalog tools with tests asserting no `confirm`; `deploy pull` is in `GUARDED_OPS` and advertises `confirm`. `deploy_family_coverage.rs` exists. `canary_never_leaves_plan` (deploy_plan.rs:1739) and `assert_no_canary` cover JSON, pretty and diff text. Verify has `no_message_or_value_echo` and the log-hit and 403 tests. Full workspace suite green (below). |

**Score:** 5/5 truths verified. `behavior_unverified` is 0: the cancellation, lock and rollback invariants belong to Phase 23, not here.

### Independent test run

`CARGO_TARGET_DIR=~/Library/Caches/cargo-target-ign-cli cargo test --workspace` at HEAD 62cb9c8: **2130 passed, 0 failed, 50 ignored**, no "FAILED" or "panicked" lines. This matches the orchestrator's figure. The 50 ignored are live tests, which were not re-run here. Their recorded outputs are in the 22-09, 22-10, 22-13 and 22-14 SUMMARYs and in 22-LIVE-CAPTURES.md. The built `ign` binary shows `deploy plan|pull|verify` in `--help`, and `deploy plan --env nosuch --repo /nonexistent --json` returns a `deploy_tree_invalid` error envelope.

### Required Artifacts

| Artifact | Status | Details |
|----------|--------|---------|
| `crates/ignition-core/src/deploy/transport/{mod,local,process,remote,sftp,winps}.rs` | VERIFIED | 10.8k lines. Substantive, wired through `for_node` and the actions. |
| `deploy/plan.rs` (1195), `ledger.rs` (629), `pull.rs` (425), `verify.rs` (1052) | VERIFIED | Real engines, wired from `actions/deploy.rs` (`deploy_plan`, `deploy_pull`, `deploy_verify`), CLI, TUI and MCP. |
| `tests/{deploy_plan,deploy_pull,deploy_transport,deploy_verify,live_deploy_plan,live_transport_wflwvm01}.rs` | VERIFIED | Present and run. The live files are `#[ignore]` with a quiet skip guard. |
| `22-LIVE-CAPTURES.md` | VERIFIED | Has CV1-CV3, C7a, C6, C6-post, C5-NTFS, OQ5 run A and run B, and Decisions. |

### Key Link Verification

| From | To | Status |
|------|----|--------|
| CLI `DeployCommand::{Plan,Pull,Verify}` -> main.rs dispatch -> `actions::deploy::*` | WIRED |
| `for_node` -> `LocalFsTransport` / `SftpTransport` as `Box<dyn TreeReader>` | WIRED; write half unreachable by test |
| `deploy_verify` -> `ActionOutput::DeployVerify` -> main.rs exit-6 hook | WIRED |
| `deploy pull` -> `GUARDED_OPS` -> MCP `confirm` | WIRED |

### Data-Flow Trace (Level 4)

Plan and pull data come from `TreeReader::read`, which is real file or sftp bytes, hashed client-side. Verify read-back comes from `DeployApi` REST list/singleton reads. Live runs show non-empty flows (48 units same). No static or hollow sources found.

### Requirements Coverage

Every ID is claimed by a PLAN frontmatter `requirements` field, and REQUIREMENTS.md maps exactly these 10 IDs to Phase 22. There are no orphans.

| Requirement | Source Plan(s) | Status | Evidence |
|-------------|----------------|--------|----------|
| DPLN-01 | 22-01, 05, 06, 07, 12, 13, 14 | SATISFIED | SC-1 evidence. Checked in REQUIREMENTS.md. |
| DPLN-02 | 22-01, 02, 03, 04, 06, 09 | SATISFIED | SC-2. Checked. |
| DPLN-03 | 22-01, 02, 03 | SATISFIED | SC-2. Checked. |
| DPLN-04 | 22-03 | SATISFIED | SC-2 delete set. Checked. |
| DPLN-05 | 22-04 | SATISFIED | SC-3 `--base`. Checked. |
| DPLN-06 | 22-08, 22-09 | **SATISFIED, but box left unchecked** | See judgement below. |
| DPLN-07 | 22-09, 10, 11 | SATISFIED | SC-4. Checked. |
| DPLN-08 | 22-10 (22-11 closes it) | SATISFIED (hermetic evidence) | SC-4 probes; human item 2. Checked. |
| DPLN-09 | 22-10, 11 | SATISFIED | SC-4 exit 6. Checked. |
| DSURF-04 | 22-11 | SATISFIED | README dated exception, line 102. Checked. |

**DPLN-06 judgement (the item the orchestrator asked about).** The requirement reads: "run `ign deploy pull` to bootstrap a `gateways/<name>/` tree from a live gateway read-only, such that `render` then `plan` against that gateway is empty." The only closure condition recorded in 22-08 and STATE.md was "from a live gateway", meaning not merely offline. That condition is met twice with `has_changes: false` and `same 48 unmanaged 1`: on the real 8.3.6 Docker rig (22-09) and on the real Windows OpenSSH host over ssh (22-13). 22-14 left it unchecked because that plan declares only DPLN-01 and produced no pull from a *prod-sized* source. Neither the requirement text nor the SC mentions prod size (D-16 and A-03 concern the OQ5 payload, and no prod-like source was ever named). This is bookkeeping lag, not a gap. **Recommendation: tick DPLN-06 and change its traceability row from Pending to Complete.** I did not edit REQUIREMENTS.md.

One caveat that does not change the verdict: pull skips units that carry an embedded secret, are of a denied type, or have no render rule (7 skipped on each live gateway). They surface as `unmanaged`, so "plan empty" holds by D-06 and D-09. That is the designed contract.

### Anti-Patterns Found

Scanned the 38 `crates/` and README files changed since the first 22-01 commit. There are no `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`, no `todo!()`/`unimplemented!()` in product code, and no stub returns on the verb paths. The README "skipped" hits are the skip vocabulary, not debt markers. The only `#[ignore]` tests are the live gates, with documented environment guards.

### Probe Execution

No `scripts/*/tests/probe-*.sh` exist and the phase declares none. The live drivers are `capture22.sh` and `capture22w.sh` under `tests/fixtures/`, which are capture tools. SKIPPED.

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full suite | `cargo test --workspace` | 2130 passed / 0 failed / 50 ignored | PASS |
| Verbs exposed | `ign deploy --help` | plan, pull, verify listed | PASS |
| Pre-verdict failure is an error envelope | `ign deploy plan --env nosuch --repo /nonexistent --json` | `ok:false`, `deploy_tree_invalid` | PASS |
| Windows cfg branches compile | `cargo check --target x86_64-pc-windows-msvc` | E0463, target std missing | SKIPPED (human item 1) |

## Warnings (non-blocking)

1. **Hold budget above the ceiling (deferred to Phase 23).** The measured budget is 505 s (no Defender exclusion) and 699 s (exclusion), against the 300 s ceiling. 22-14 recorded it as an open Phase 23 blocker and did not clamp it. The Phase 22 exit gate asks for the budget to be measured and recorded, and it is. This is a Phase 23 planning input and not a Phase 22 failure. The measurement is one A/B pair at 152 files, and the 1,520-file figures are extrapolations. The reads are also slow: `pull` took 125 s and `plan` 103 s on the Windows host, because each file costs an sftp round trip.
2. **`verify` read-back false-reds a hand-written, minimal `database-connection` `config.json`** (17 members differ because the gateway fills defaults; 22-10 records it). A pulled tree compares equal. The CV2 table has no defaults row for it, and 22-14 did not add one, though 22-10 had pointed it there. The README documents the limitation. SC-4 as worded ("reads back equal") is met. It matters for Phase 23, whose auto-rollback fires on a verify failure.
3. **DPLN-08 has no live run.** See human item 2.
4. **`cfg(windows)` client-side code is uncompiled locally and not yet run on CI.** See human item 1.
5. The Windows host is an Azure VM running Win32-OpenSSH 8.1, standing in for WFLWVM01 under user-approved amendment A-01/A-02. Timings reflect that VM and an internet link.
6. The `plan --diff` secret canary and the verify no-echo tests are hermetic. The canary is "completed" by Phase 23 per SC-5, which is consistent.

## Gaps Summary

None. All five roadmap success criteria hold in the code and in recorded live evidence, all ten requirement IDs are claimed and satisfied, and the independent test run matches the reported numbers. The `human_needed` status reflects two checks that nothing has run (the Windows CI leg and a live tag-probe verify), not a suspected defect. If the orchestrator accepts those as follow-ups, the phase can be treated as passed.

---

_Verified: 2026-10-15T19:00:00Z_
_Verifier: Claude (gsd-verifier)_
