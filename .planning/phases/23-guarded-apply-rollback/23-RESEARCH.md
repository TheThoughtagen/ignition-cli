# Phase 23: Guarded Apply & Rollback - Research

**Researched:** 2026-10-16
**Domain:** Rust, transactional file deploy to an Ignition 8.3.6 gateway host (system OpenSSH or local FS) under the config scan-lock, with snapshot, journal, auto-rollback and a private run store
**Confidence:** MEDIUM-HIGH. The in-repo seams, wire facts and decisions are HIGH (read this session). Three things are not provable from the repo and are tagged `[ASSUMED]` with a capture owed: the on-box script mechanics on Windows (script size, ACL after a same-volume move), what the scan-lock actually blocks (C2), and the batched commit timing (D-02). The Windows capture host may no longer exist (see Environment Availability).

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

Already locked by ROADMAP.md, REQUIREMENTS.md and Phase 22, and not re-decided here:
- Decision A: `deploy_rolled_back` lives in `data`, exit 6, and is never an exit-table row.
- Decision B = B(i): home-relative staging. The ledger and journal sit at `ign-deploy/<gateway>/<mode>/` in the service account home on ssh nodes and at `<data_dir>/ign-deploy/...` on LocalFs nodes. Staging is at `.../staging/<run_id>/`.
- Decision C: `--plan-sha256`. Decision D: `apply` and `rollback` are not MCP tools.
- SF-3: report `restart_pending` and never restart. SF-6: `.gwbk` is on by default and `--skip-gwbk` is recorded.
- There is no `--force`, `--skip-verify` or `--no-rollback`. One `restore_snapshot(run)` serves abort, auto-rollback and manual rollback.
- Lock decisions use the gateway's `remaining` and a monotonic local clock.
- Phase 22 D-05..D-08 still hold: the ledger contract, `unmanaged`, the ledger refusal and `plan_sha256`.
- Phase 20 captured lock facts: a holder that re-acquires gets 409, so a lock cannot be extended (C1d). The holder's own scan releases the lock (C1j / D12).

**Lock window and hold budget (resolves the open STATE.md blocker, OQ5-2 / OQ5-4)**
- **D-01:** The commit runs on the box as one batched script, not as per-file sftp renames. Before the lock, apply stages the files over sftp as today and checks the staged hashes. Under the lock, one remote call (PowerShell `EncodedCommand` through `exec_powershell` on Windows, a POSIX `sh` script on Linux ssh nodes, native filesystem calls on LocalFs) re-hashes the staged copies, takes the snapshot (D-04), moves the files into the mode collection, and removes the delete set. The scan follows. The C7a-4 rules still apply inside the script: replace a file by moving it over the old one, never move a path onto itself, move a directory aside before replacing it, and retry a held target a bounded number of times. — **Reversibility:** costly — this replaces the 22-13 commit path inside `TreeMirror::commit` for ssh nodes, and the on-box script becomes part of the write contract.
- **D-02:** The hold timeout is one tested constant. Phase 23 re-runs OQ5 with the batched commit on the Azure VM `ign-win-836` and keeps `ceil(2 x (p95 commit + p95 scan) + 1)`, with a 10 s floor and a 300 s ceiling. There is no per-run scaling and no per-target override.
- **D-03:** Apply refuses before the lock and before any write when the change is too large for the budget. The estimated commit time, based on the run's write and delete counts and the measured per-file cost, must fit the hold. The refusal names the estimate and the budget.
- **D-03a:** If the re-measured hold is still above 300 s, execution stops at a `checkpoint:decision` for the user before the write flow ships. Never clamp silently. The `op_timeout_s` cap of 3600 s (OQ5-4) is revisited at the same checkpoint only if it binds.

**Snapshot**
- **D-04:** The on-box snapshot is taken inside the lock, by the same batched script, before the first move. It goes to `ign-deploy/<gateway>/<mode>/snapshots/<run_id>/`, outside every collection, with a hash for every file. The run directory downloads the snapshot after the scan, outside the lock. — **Reversibility:** one-way — the on-box snapshot layout is read by `rollback --run` from any later runner.

**Run directory and run identity**
- **D-05:** The run directory is repo-local: `<deploy-repo>/.ign/deploy/<gateway>/<run-id>/`, with the directory at 0700 and its files at 0600 (owner-only ACL on Windows). It holds `run.json`, `result.json`, `plan.json`, `events.ndjson`, the downloaded snapshot and the `.gwbk`. If git does not ignore that path, apply refuses before any write, because the directory holds secrets. — **Reversibility:** costly — the path is documented and users' `.gitignore` files depend on it.
- **D-06:** A run ID is a UTC timestamp plus a random suffix, for example `20261015T142233Z-a1b2c3`. It sorts by time and fits the existing staging rule `[A-Za-z0-9._-]{1,128}`.
- **D-07:** Apply also writes a small on-box run record next to the on-box snapshot. It holds the run id, the snapshot hashes, the pre-run fingerprint and the post-apply fingerprint. `rollback --run <id>` works with only the target file and the run id, so an ephemeral CI runner can roll back a run it did not perform. When the local run directory is present it is used and cross-checked against the on-box record. — **Reversibility:** one-way — the on-box run record format is read across runners and hosts, like the ledger (22 D-05).
- **D-08:** v1.6 keeps every run: nothing prunes run directories or on-box snapshots. The README documents manual cleanup.

**Journal recovery**
- **D-09:** `rollback --run <id>` is the recovery command. No new verb is added. The journal names its run id, and the `deploy_journal_present` refusal from `plan`, `apply` and `rollback` prints the exact `ign deploy rollback --run <id>` command. Rollback restores that run's snapshot through `restore_snapshot`, scans, and then removes the journal.
- **D-10:** If the journal shows the run died before any move began, and the live collection hash equals the journal's pre-run fingerprint, recovery removes the journal and the partial snapshot without restoring. Otherwise it restores.
- **D-11:** Recovering an interrupted run skips the post-apply staleness check, because no post-apply fingerprint exists and the tree may be partial. It restores without needing `--discard-live-changes` and records the mid-run state hashes in the new run's events. A rollback of a run that completed keeps the DAPL-08 check.
- **D-12:** No flag discards a journal without restoring it. When a snapshot is lost or corrupt, `rollback_failed` guidance tells the administrator to restore from the `.gwbk` or the run-dir snapshot and then delete the journal at the printed path by hand.

**.gwbk and Designer preflight**
- **D-13:** The `.gwbk` is downloaded through the existing `backup_download` into the run directory, after the preflights and before the lock, so it never spends hold budget. A failed or truncated download is a pre-write refusal. Only `--skip-gwbk`, which is recorded, bypasses it.
- **D-14:** `--designers wait` polls the open Designer sessions every 10 s, for up to 600 s by default. `--designers-timeout <secs>` changes the limit. On timeout it refuses with the session list, the same refusal as `fail`.
- **D-15:** `--designers proceed` takes no reason string. `run.json` and the events record that `proceed` was used and the sessions open at preflight time (user, project, host).

### Claude's Discretion
- The internal journal phase markers and their names, as long as D-10 can tell a run that died before any write from one that died during writes.
- How the per-file commit cost for D-03 is derived from the re-measured OQ5 data.
- The exact on-box script structure and how it reports per-file results, as long as it fits the existing `encoded_command` budget or chunks within one lock window.
- The `apply` and `rollback` human output and progress events on stderr, following the existing sink pattern.

### Deferred Ideas (OUT OF SCOPE)
- Automatic pruning of run directories and on-box snapshots (keep the last N). Not in v1.6 (D-08).
- A per-target `hold_timeout_s` override and per-run budget scaling were not selected (D-02).
- A dedicated `ign deploy recover` verb and an `--abandon-journal` escape were not selected (D-09, D-12).
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| DAPL-01 | `apply` without `--yes` refuses and prints the plan; with `--plan-sha256` a changed plan refuses as `plan_stale` | Gate ordering (Pattern 2), `plan_sha256` recomputed from `deploy_plan` (plan.rs module doc), `ConfirmationRequired` carries the plan prose, Decision C enforcement is Open Question 3 |
| DAPL-02 | Destructive needs `--allow-destructive`; drift needs `--allow-drift "<reason>"` (recorded); `shadows_removal` blocks regardless | `PlanEntry.destructive`, `.drift`, `.core == shadows_removal` already computed (plan.rs:203-236); pure `gates.rs`; drift exists only with `--base` (plan.rs:229) |
| DAPL-03 | Empty plan exits 0, no lock, snapshot or scan | `summary.has_changes` (plan.rs:253); noop precedes preflight and run dir |
| DAPL-04 | All preflights before any write, reported together: mode on both nodes, wrong-target, write permission, no journal, Designers | `GatewayApi::designers`, `scan_projects` write probe, `DeployApi::deployment_mode`, host `read` of journal; wrong-target definition is Open Question 1 |
| DAPL-05 | Snapshot on host and in run dir with hashes; `.gwbk` default; neither uploaded off-prem | On-box script snapshot (D-04), `backup_download` (backup.rs:72) into a 0700 run dir, `zip` crate validation of the download |
| DAPL-06 | Writes inside the scan-lock within a measured hold; expiry or failed write mid-way restores and scans | `HeldLock` + `lock_verdict` (scan.rs:107-143), one `restore_snapshot`, fault-injection wrapper over the host seam |
| DAPL-07 | Verify failure after apply auto-rolls back through the same engine: `deploy_rolled_back` exit 6; failed rollback is `rollback_failed` | `deploy_verify` reused as-is (actions/deploy.rs:893); restore verification is a different check (Pitfall 9) |
| DAPL-08 | `rollback --run <id>` from a fresh process; refuses when live differs from post-apply fingerprint unless `--discard-live-changes` | On-box run record (D-07); fingerprint defined over the touched set (Pattern 5) so no manifest is needed |
| DAPL-09 | Interrupted apply leaves an on-box journal; plan, apply, rollback refuse until recovery | New `TreeReader::read_journal`; journal written first by the script under the lock; task-abort crash simulation |
| DAPL-10 | Every run, failures included, writes a private run dir; `restart_pending` reported, never restarts | `run_store.rs`; `restart-tasks/pending` needs a new `DeployApi` method and a live shape capture (C-restart) |
| DSURF-02 | `apply`, `rollback` in `GUARDED_OPS`; absent from MCP; `render`, `plan`, `pull`, `verify` remain | `GUARDED_OPS` main.rs:3933, `EXCLUDED_LEAVES` mcp.rs:62 (segment match, change to path match), mcp.rs:833-845 test must be amended |
| DSURF-03 | Secret canary over plan, apply, rollback, verify output, `--json`, run-dir files, events | Existing canary pattern (contract_deploy.rs:968-1042); run-dir scope must exclude `snapshot/**` and `*.gwbk` explicitly (Pitfall 12) |
</phase_requirements>

## Summary

Phase 23 is the first phase that writes to a gateway host. Everything it needs below the engine already exists and was read this session: a two-trait transport (`TreeReader`, `TreeMirror`, transport/mod.rs), a hardened ssh/sftp shell with a PowerShell `EncodedCommand` executor (remote.rs:994, winps.rs), a pure plan with `plan_sha256`, destructive/lockout/shadow flags and drift (plan.rs), the ledger format and its location rules (ledger.rs), the scan-lock/scan wire with capture-proven predicates (deploy_api.rs, scan.rs), a stateful in-memory `DeployApi` fake with a lock clock (tests/common/fake_deploy.rs), and `deploy_verify` (actions/deploy.rs:893). What does not exist is the engine, the on-box batch script, the run store, the journal and run-record formats, a held-lock type, the CLI verbs, and six or more new error slugs.

The central design fact is time. The measured sftp commit of 152 files took 234 to 348 s on the Windows host (22-LIVE-CAPTURES.md, OQ5-1), and reading the current `commit_inner` (sftp.rs:2268-2527) shows why it is slow: before the first rename it fetches every staged copy back (152 `get`s), then renames each file over sftp, then fetches every final path back again (another 152 `get`s). The batched on-box script replaces all three with local operations, so the lock window should shrink from minutes to seconds, but that is an inference and is exactly what D-02's re-measure must prove. The re-measure needs the script to exist, so "capture first" cannot be fully ahead of the code: captures that do not need the script (C2, C11, restart-pending shape, gwbk, script-run mechanics) go first; C7b and the OQ5 re-run go the moment the host layer lands and before the engine's hold constant is committed.

Four non-obvious decisions are made in this research, each flagged for confirmation in the Assumptions Log. (1) The snapshot, journal and both fingerprints are defined over the **touched set** (every path apply writes or deletes, plus the ledger), not the whole collection. That makes `rollback --run` need only the target file and run id (no manifest), makes the staleness refusal exactly "we would overwrite a live change", and keeps secrets in the run dir to what apply actually touched. (2) The on-box script is **uploaded as a file with a work-list manifest** and run with a short command, because parameters travel inside the 8000-character command budget (winps.rs:55) and a 152-file path list alone is about 12 KB. (3) The secret canary (DSURF-03) must exempt `snapshot/**` and `*.gwbk` in the run directory, because D-05 itself says they hold secrets; the exemption is asserted, not silent. (4) The gate order puts `--yes` before the preflights so a refusal never spends a 600 s Designer wait or a projects-scan probe.

**Primary recommendation:** Build the engine as a new `deploy/apply/` module behind a narrow `ApplyHost` seam (LocalFs in Rust, POSIX `sh` and PowerShell as uploaded scripts with one shared conformance table), keep every decision pure and unit-testable (`gates`, `hold`, `fingerprint`, `journal`, `record`), run the engine under a fault-injection wrapper that can fail the nth operation, expire the lock via `FakeGateway::tick`, or abort the task to simulate `kill -9`, and treat the hold constant as the output of a re-measure that gates the write flow.

## Project Constraints (from CLAUDE.md)

Source: `/Users/pmannion/whiskeyhouse/ignition-cli/.claude/CLAUDE.md` and the user global CLAUDE.md, read this session.

- Rust, clap derive, tokio, reqwest; **keep the dependency tree lean**; no framework creep; single binary, no daemon. This phase needs no new crate (see Standard Stack).
- Ignition 8.3.1+ only.
- **Every subcommand is non-interactive by default with `--json` output**; the JSON envelope is the only stdout; actions never print.
- The exit-code table source of truth is the README table and `crates/ignition-core/src/error.rs`, pinned by `readme_exit_table_agreement` and `exit_code_mapping_enumerated`. New slugs follow the four-place rule.
- **Do NOT create git worktrees that share the cargo target dir.** Run plans sequentially on the main checkout (`workflow.use_worktrees=false`). If a worktree is unavoidable give it its own `CARGO_TARGET_DIR`. Phantom "module not found" or "test vanished" errors mean a shared target dir, not a code defect. The shell `rm` is wrapped and rejects `-rf`; use `/bin/rm`.
- `ign` on PATH is the installed binary, not the repo build: run `cargo install --path crates/ignition-cli --force` before any manual smoke of a changed verb.
- GSD workflow enforcement: file edits happen through a GSD command. Superpowers TDD, verification-before-completion and systematic-debugging apply per task (user global).
- WHK data boundary (user global): this phase is deterministic Rust; no private content goes to a non-Claude backend.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Re-plan, gate evaluation, `plan_sha256` compare, hold estimate | `ignition-core` pure functions | CLI flag parsing | Pure and unit-testable; the CLI only parses flags and renders |
| Preflights (mode, wrong-target, write probe, Designers, journal) | Gateway REST (`DeployApi`/`GatewayApi`) + host read | CLI (prints the aggregated refusal) | Mode and Designers are gateway state; journal is host state; results aggregate in core |
| Stage files, work-list, scripts | Transport (sftp put / local copy) | — | Outside the lock; reuses `TreeMirror::stage` |
| Snapshot, journal, moves, deletes, ledger swap, fingerprints | **On the gateway host** (script / native FS) | Transport only uploads and invokes | The whole point of D-01: local operations inside the lock |
| Scan-lock, scan, completion | Gateway REST (`DeployApi`) | Core `HeldLock` timer | Lock state is the gateway's `remaining`; local monotonic clock is the second opinion |
| Verify | Core `deploy_verify` (REST + host read + logs) | — | Reused unchanged |
| Run store (run.json, result.json, plan.json, events.ndjson, snapshot copy, .gwbk) | Runner filesystem, `<repo>/.ign/deploy/` | git (ignore check) | Private, never uploaded; refusal when not git-ignored |
| Verdict exit (`deploy_rolled_back` exit 6) | `ignition-cli` `main.rs` after envelope render | — | Third-to-fourth sanctioned success-path exit exception (main.rs:572-576 pattern) |
| MCP exclusion, `GUARDED_OPS`, TUI rows, README rows | CLI surface | — | DSURF-01/02 coverage tests enforce it |

## Standard Stack

### Core

No new crates. Everything the phase needs is already in `[workspace.dependencies]` (Cargo.toml read this session).

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `sha2` | 0.10 (workspace) | File and listing hashes, plan hash recompute | Already the project hash; `tree::sha256_hex` exists |
| `getrandom` | 0.2 (workspace) | Random suffix of the run id | Already pinned for the webdev secret; zero new edge |
| `tempfile` | workspace | Staging temp dirs (existing `copy_to_temp`) | Already used by the mirror |
| `base64` | 0.22 (workspace) | `EncodedCommand`, work-list blob if inlined | Already the single encoder (`RemoteCommand::powershell_encoded`) |
| `serde` / `serde_json` | workspace | Journal, run record, run.json, events | Ledger precedent (strict, `deny_unknown_fields`) |
| `zip` | 8.6 (workspace) | Validate a downloaded `.gwbk` is a complete archive (central directory readable) `[ASSUMED]` that `.gwbk` is a zip | Already a dependency; opening the central directory fails on a truncated file |
| `tokio` | 1.53 (workspace) | Async engine, `spawn` + `abort` for crash simulation | Existing |
| `async-trait` | workspace | `ApplyHost` seam | Existing pattern for `TreeReader` |

### Supporting

| Tool | Version | Purpose | When to Use |
|------|---------|---------|-------------|
| system `git` | 2.46.2 here | `git check-ignore -q -- <path>` for D-05 | Run-dir ignore guard. Verified this session: exit 0 ignored, exit 1 not ignored, exit 128 not a repo; works for paths that do not exist yet when `.ign/` is ignored |
| system `ssh`/`sftp`, `sh` | OpenSSH | Transport, POSIX script | Existing hardened shell |
| `icacls` | Windows | Owner-only ACL of the run dir | Precedent for a file at remote.rs:383-401 (`restrict_to_owner`); needs a directory variant |
| `pwsh` 7.5.4 | local macOS | Optional local smoke of the PowerShell script | Not Windows PowerShell 5.1 semantics; the authoritative run is windows-latest CI or the capture host |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Hand-rolled UTC timestamp | `jiff`/`time` | `workspace.rs:976-1000` already hand-rolls `unix_ms_to_rfc3339_utc` (Hinnant civil-from-days); reuse it to format `YYYYMMDDTHHMMSSZ`. A date crate is a new dependency for one format string |
| Whole-collection snapshot | Touched-set snapshot (recommended) | Whole-collection copy is simpler to explain but downloads every file over ~1 s/file sftp and puts every secret in the run dir. See Assumptions A1 |
| Tokio signal handling for Ctrl-C | None | `tokio` has no `signal` feature in the workspace. The journal already covers a kill; do not add a handler in v1.6 |

**Installation:** none. **Version verification:** not applicable (no new package). The existing pins were read from `/Users/pmannion/whiskeyhouse/ignition-cli/Cargo.toml` this session.

## Package Legitimacy Audit

No external package is added by this phase. `sha2`, `getrandom`, `tempfile`, `base64`, `zip`, `serde`, `serde_json`, `tokio`, `async-trait` are existing workspace dependencies.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| (none new) | — | — | — | — | — | Nothing to install; the legitimacy gate does not apply |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

If planning decides it needs a crate after all (a time crate, a signal feature), that is a user decision against the "keep the dependency tree lean" constraint, and the planner must add a `checkpoint:human-verify` and run `package-legitimacy check` first.

## Architecture Patterns

### System Architecture Diagram

```
 ign deploy apply --env E [--plan-sha256 H] [--yes] [--allow-*] [--designers m] [--skip-gwbk] [--base D]
        |
        v
 [1 flags] validate (run-id-safe, hash shape, --plan-sha256 policy) ---------- bad -> invalid_input (2)
        |
        v
 [2 load] load_declared(repo, env) -> Declared (render)   [offline, no gateway]
        |
        v
 [3 plan] reader.read_journal --present--> deploy_journal_present (6)  (names run id + rollback cmd)
          deploy_plan(declared, reader, {base}) -> plan, plan_sha256, delete_set   (host READ only)
        |
        v
 [4 gates, pure]  --plan-sha256 != plan_sha256 --------------------------------> plan_stale (6)
                  no changes (has_changes=false) ----------------------------> exit 0 "noop"  (no lock/snapshot/scan/run dir)
                  any shadows_removal ---------------------------------------> deploy_blocked (6)
                  destructive w/o --allow-destructive, drift w/o --allow-drift -> destructive_change_refused (2)
                  no --yes --------------------------------------------------> confirmation_required (2), prose = the plan
        |
        v
 [5a run store] run-dir path git-ignored? (D-05) --no--> refuse (nothing written)
                 create .ign/deploy/<gw>/<run-id>/ (0700) ; write run.json, plan.json ; open events.ndjson
                 (from here on every outcome, failures included, has a run dir: DAPL-10)
        |
        v
 [5b preflight, ALL reported together]
    mode(master)  mode(backup if any)  wrong-target  write-permission(REST probe + host probe)
    designers(fail|wait|proceed)  hold-budget estimate (D-03)
        |  any failure ---------------------------------------------------> deploy_preflight_failed (6) (result.json written)
        |
        v
 [7 .gwbk] backup_download -> run dir (0600), validate non-empty + zip central directory  --fail--> deploy_preflight_failed
        |   (skipped & recorded with --skip-gwbk)
        v
 [8 stage, OUTSIDE the lock]  TreeMirror::stage(subset of added/changed units) + upload work-list + script
        |                      -> ign-deploy/<gw>/<mode>/staging/<run-id>/
        v
 [9 LOCK] scan_lock_acquire(Config, 0?, HOLD_S)  --409--> scan_lock_held (6) ; 403 -> auth error (6/5) before any file
        |   HeldLock { Instant, hold_s }
        v
 [10 ONE host call: commit script]  (journal create_new -> pre-image check vs plan -> snapshot -> moves -> deletes
        |                              -> ledger swap -> result JSON)         journal phase markers advance in place
        |   pre-image mismatch -> abort BEFORE any move (journal+snapshot removed) -> plan_stale (6)
        |   any later failure / lock check Expiring|NotHeld -> [R]
        v
 [11] lock_verdict(scan_lock_info) must be Held{remaining >= margin} AND local clock inside hold
        |  else -> [R]  (no scan issued by us)
        v
 [12 SCAN] scan_and_wait(Config)  (releases the lock at trigger, D12)  --NotComplete--> [R]
        v
 [13 VERIFY] deploy_verify(declared ...)  green -> [14]    red -> [R]
        v
 [14 finalize] fingerprint(post) ; write on-box run record ; download snapshot to run dir ;
               remove journal + staging ; restart_pending delta ; write result.json -> exit 0 (outcome "applied")

 [R] restore_snapshot(run): re-acquire lock -> restore script (put back pre-images, delete created files,
     restore ledger, hashes must equal record) -> scan_and_wait -> check restored hashes + mode
        ok  -> result.json outcome "rolled_back", data.ok=false, data.slug=deploy_rolled_back, cause=<..>, exit 6 (after envelope)
        bad -> rollback_failed (6, CoreError, run id + run dir + on-box path + recovery guidance); journal stays

 ign deploy rollback --run ID --env E [--discard-live-changes] [--yes]   (fresh process)
     load_target only -> read on-box run record (+ cross-check run dir) -> journal present for ANOTHER run -> refuse
     -> staleness: live fingerprint(touched set) vs record.post_fingerprint  (skipped for an interrupted run, D-11)
     -> require --yes -> preflights (mode, write probe, designers) -> [R]
```

### Recommended Project Structure

```
crates/ignition-core/src/deploy/apply/        # NEW. The only place (with transport/) that names the write half
├── mod.rs          # run_apply(), run_rollback(), restore_snapshot(), outcome/result types
├── gates.rs        # PURE: flags x plan -> Gate result (shadows, destructive, drift, stale, confirm)
├── preflight.rs    # checks, aggregation into one deploy_preflight_failed
├── hold.rs         # HOLD_FLOOR_S/HOLD_CEILING_S/HOLD_S, hold_for(), estimate(), HeldLock, Clock trait
├── fingerprint.rs  # PURE: touched-set fingerprint (path, sha|absent) -> hex
├── journal.rs      # Journal v1 (strict JSON), Phase markers, recovery decision (D-10)
├── record.rs       # RunRecord v1 (on-box, D-07)
├── run_store.rs    # RunDir: create 0700/0600, git guard, events.ndjson writer, result/plan/run json
└── host.rs         # trait ApplyHost; BatchSpec/BatchReport; LocalHost impl (Rust); script hosts
crates/ignition-core/src/deploy/transport/scripts/   # NEW
├── commit.sh       # POSIX, include_str!; ops: probe, commit, restore, fingerprint, finalize
└── commit.ps1      # Windows PowerShell 5.1, same ops, same result JSON
crates/ignition-core/src/actions/deploy_apply.rs     # NEW thin verbs: deploy_apply(), deploy_rollback(), results, failure_exit_code()
crates/ignition-core/tests/deploy_apply.rs           # NEW engine tests (FakeGateway + LocalHost + FaultyHost)
crates/ignition-core/tests/deploy_host_conformance.rs# NEW one scenario table run against Local, sh (unix), PowerShell (windows CI)
```

`actions/deploy.rs` stays free of the write half: `write_half_unreachable` (tests/deploy_transport.rs:2811-2842) scans `actions/deploy.rs`, `main.rs` and `mcp.rs` for `TreeMirror`, `mirror_for_node`, `StagingMode`, `.stage(`, `.commit(`. Put apply in the new files, and have `main.rs` call only `actions::deploy_apply::*` (which builds the host internally). Update that test deliberately: keep the three files forbidden and add a positive assertion that only `deploy/apply/` and `deploy/transport/` name the write half.

### Pattern 1: The on-box batch is a file-driven script, not a parameterized command

**What:** Stage uploads, next to the staged files, (a) `work.json`: strict JSON with `run_id`, `gateway`, `mode`, `writes[]{path,sha256,pre}`, `deletes[]{path,pre}`, `ledger_new` (staged file name or absent), margin and retry bounds; and (b) the script file. The remote command is short and fixed: it names the script path and the work-list's sha256, e.g. `powershell -NoProfile -NonInteractive -EncodedCommand <b64 of a 3-line stub>` where the stub verifies the script file hash and runs it as a script block. `RemoteCommand::posix(&["sh", script, run_id, work_sha256])` on Linux.
**Why:** `COMMAND_BUDGET` is 8000 characters for the whole command; base64 of UTF-16LE expands a script 2.67 times so about 2900 characters of script fit (22-LIVE-CAPTURES.md C7a-7, quoted in winps.rs:55 `pub const COMMAND_BUDGET: usize = 8000;`). Parameters travel base64-JSON inside that budget (winps.rs `encoded_command`), and 152 paths of about 80 characters is about 12 KB. Commit, restore, snapshot and fingerprint logic will not fit 2900 characters on PowerShell.
**Windows execution mechanics `[ASSUMED]`, capture owed (C-script-run):** run the uploaded file as `& ([ScriptBlock]::Create([IO.File]::ReadAllText('<path>')))` inside the encoded stub. That does not execute a `.ps1` file, so the execution policy does not apply to it; `powershell -File` would. Not captured on the Windows host: C7a-6 only says the host "has no `powershell -File` path that avoids an upload and execution-policy handling". The stub must compare the file's SHA-256 with the value in the command and exit non-zero on mismatch.
**LocalFs:** the same operations in Rust, using the existing descriptor-relative helpers (`dirfd::rename_at`, `unlink_at`, `ensure_walk`, `read_file`, local.rs:1073-1239) on unix and the path-based fallback on Windows (local.rs:1245).
**Result contract (all three hosts):** exactly one `IGN-RESULT:` line of ASCII-escaped JSON (winps.rs `parse_ign_result`), carrying per-phase status and per-file `{path, status, sha256}`; PowerShell stdout is the OEM code page so no non-ASCII (C7a-6).

### Pattern 2: Gate order (pin it in a test matrix)

1. Flag validation (exit 2): `--plan-sha256` shape (64 lowercase hex), `--designers` value, `--designers-timeout` range, `--allow-drift` reason (non-empty, capped, no control characters).
2. `load_declared`, then `reader.read_journal`: journal present -> `deploy_journal_present`.
3. `deploy_plan` (host read only; on Windows about 3 minutes, OQ5-1 read p95 181 to 214 s).
4. `plan_stale` if `--plan-sha256` is given and differs.
5. No changes -> exit 0 `noop` (no lock, snapshot, scan, run directory).
6. `shadows_removal` -> `deploy_blocked`. Then destructive and drift gates -> `destructive_change_refused`.
7. No `--yes` -> `confirmation_required` whose prose is the plan summary, entry keys and statuses, and `plan_sha256` (so the reviewer can pass it).
8. Only then the preflights (they have side effects: the projects-scan probe, a Designer wait of up to 600 s).

The reasons: `plan_stale` before `noop` keeps Decision C strict (a reviewed non-empty plan that became empty is stale, which is accurate); `--yes` before the preflights keeps a refusal cheap. Both are Open Question 3 and 4 items for confirmation. Drift only exists when `--base` is passed (`drift: Option<bool>`, null without `--base`, plan.rs:229); `apply` therefore needs `--base DIR` and passes it to `deploy_plan` (`PlanOptions.base`, actions/deploy.rs:254-260). `plan_sha256` excludes drift (plan.rs module doc), so a hash reviewed with `--base` equals one without.

### Pattern 3: Touched set, pre-image check and write set

The write set is every rendered file of every unit whose status is `added` or `changed` (whole unit: a unit present in the mode collection overrides `core` as a whole, plan.rs module doc "the gateway overrides whole resources, and tag folders, by name"). Writing only the differing file of a unit that lives in `core` would create a partial unit in the mode collection. `same` units are not written (they are not changes, and DAPL-03 requires an empty plan to be a no-op). The delete set comes from `DeployPlanResult.delete_set`.

Each touched path carries its **expected pre-image in the mode collection**, derived from the plan entry: if the entry's `live_in` contains `"mode"`, the file's `live_sha256` (or absent when null); if `live_in` is `["core"]`, absent. The script verifies these under the lock, before the first move. A mismatch means the collection changed after the plan (a UI or Designer edit, C2): abort with nothing moved and report `plan_stale`. This is Pitfall 25's "re-read after acquiring the lock" implemented as data the script already hashes anyway.

### Pattern 4: Journal, record, ledger (formats are one-way contracts)

All three are strict JSON modelled on `Ledger` (ledger.rs: `schema_version`, `deny_unknown_fields`, size cap, sorted unique paths, `to_bytes`/`parse` round trip). Locations, all under the node's ledger root (account home on ssh, `<data_dir>` on local, ledger.rs:35-54), none inside `config/resources/`, none dot-named:

| File | Path under `ign-deploy/<gateway>/<mode>/` | Written by |
|------|-------------------------------------------|------------|
| ledger | `ledger.json` (`ledger_rel`) | script, last file move of the commit |
| journal | `journal.json` | script, `create_new` as the first action under the lock |
| snapshot | `snapshots/<run_id>/` (CONTEXT D-04) with pre-image copies | script |
| run record | `snapshots/<run_id>/record.json` | script (pre part), finalize call (post part) |
| staging | `staging/<run_id>/` (`staging_rel`) | `TreeMirror::stage` |

`journal.json` fields: `schema_version`, `run_id`, `gateway`, `mode`, `phase`, `pre_fingerprint`, `touched` (paths). Phase markers (discretion, names are mine): `snapshotting`, `ready`, `writing`, `written`, `scanned`, `verified`, `restoring`. D-10: a journal at `snapshotting` or `ready` whose live touched-set fingerprint equals `pre_fingerprint` is cleared without restoring; any other phase restores.
**Ledger after apply:** new files = (old ledger files minus the delete set) union the written files, byte-sorted, `render_sha256` and `run_id` of this run. Build it in Rust, validate with `Ledger::parse(to_bytes())` before upload, stage it, and swap it in as the last move. The snapshot holds the prior ledger (or an "absent" marker) so a restore reverts it too.
**Journal create is exclusive** (`create_new` / `New-Item` fails if present / `set -C`): together with the immediate 409 on a second lock acquire (D1) it makes two concurrent `ign` runs mutually exclusive.

### Pattern 5: Fingerprints over the touched set

`fingerprint(entries)` = SHA-256 over `path NUL (sha256 | "-") LF` lines in byte order of path, the same shape as `render_sha256` (tree.rs:62-71) and `plan_sha256` (plan.rs module doc). Compute in Rust from `path -> sha` maps the script returns as JSON, never in the script (PowerShell ordinal sort is UTF-16, not UTF-8 byte order). `pre_fingerprint` is over each touched path's pre-image or absent; `post_fingerprint` over each touched path's final hash or absent. Take `post` **after** the scan and verify, with a read-only `fingerprint` script call, because the gateway could rewrite a file on load (CV3 says it did not for plain-file-write resources, 22-LIVE-CAPTURES.md CV3, but this is the safe point). Rollback staleness = live fingerprint of the recorded touched set differs from `post_fingerprint`. Unrelated edits elsewhere in the collection never block a rollback, and no manifest is needed (satisfies D-07).

### Pattern 6: `HeldLock` with an injectable clock

```rust
// Source: shapes read this session: client/scan.rs:107-143, client/deploy_api.rs:293-313
pub trait Clock: Send + Sync { fn now(&self) -> std::time::Instant; }   // SystemClock + ManualClock in tests

pub struct HeldLock { kind: ScanKind, hold_s: u64, acquired: std::time::Instant }

impl HeldLock {
    /// Gateway `remaining` is primary; the local monotonic clock is the second opinion.
    /// Either one saying "expiring" counts as expired. Never reads `timestamp`.
    pub async fn check(&self, api: &dyn DeployApi, margin_s: i64, clock: &dyn Clock)
        -> Result<LockVerdict, CoreError>;
}
```
The workspace has no `tokio` `test-util` feature and `poll.rs` uses `std::time::Instant`, so an injected `Clock` is simpler than paused tokio time. Gateway time is controlled in tests by `FakeGateway::tick(seconds)` (fake_deploy.rs:207-219). `ScanKind` has `Config` and `Projects` (scan.rs:36-41); use `Config`.

### Pattern 7: Crash simulation without a test backdoor in the binary

A production-binary env var that crashes at step N is an attack surface. Instead wrap the host: `FaultyHost<H: ApplyHost>` delegates until the nth operation, then returns `Err`, or sleeps forever so the test can `JoinHandle::abort()` the task. Aborting the task drops the future at an await point, which is the same observable state as `kill -9` (no further code, journal left behind). Keep the engine free of `Drop`-based cleanup so the simulation is faithful.

### Anti-Patterns to Avoid

- **Per-file sftp renames, read-backs, or hash fetches inside the lock.** That is the 234 to 348 s path D-01 replaces. Nothing in the lock window may cost per-file network round trips.
- **Writing only the differing files of a changed unit.** Creates partial overriding units in the mode collection (Pattern 3).
- **A whole-collection snapshot "to be safe".** Slow to download, widens the secret exposure, and breaks the manifest-free rollback (A1).
- **Computing the post-apply fingerprint before the scan.** Take it after verify.
- **Deleting `config-mode.json` or anything not in the delete set.** `GATEWAY_OWNED_FILES` and `mirror_path_problem` (transport/mod.rs:489-508) already refuse; the restore script must obey the same rule (Pitfall 5 / PITFALLS "Pitfall 21").
- **A `--force`, `--skip-verify` or `--no-rollback` flag, or a journal-discard flag.** Out of scope by decision.
- **Printing the rollback command in `hint()`.** See Pitfall 4.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Scan-complete predicate | Your own timestamp compare | `scan_and_wait` (deploy_api.rs:930), `scan_complete` (scan.rs:171) | Waits out an active scan before the trigger and judges by the same gateway's `lastScanTimestamp`; a trigger's 200 is never completion (D4) |
| Lock verdicts | `remaining > 0` checks inline | `lock_verdict` + `LockVerdict::{Held,Expiring,NotHeld}` (scan.rs:107-143) | `NotHeld` is also the normal state after the holder's own scan (D12); the type already encodes that |
| Post-apply verification | A second comparator | `deploy_verify` (actions/deploy.rs:893) | Six checks, value-free, red returns `ok:false` with slug `deploy_verify_failed` |
| Restore verification | Re-running `deploy_verify` against the declared tree | Hash equality against the record + scan complete + mode check (Pitfall 9) | After a restore the declared tree is no longer the expected state |
| Plan hash and gates inputs | New diffing | `deploy_plan`, `PlanEntry.destructive/drift/core/live_in` | Already computed and canary-tested in Phase 22 |
| Ledger format | A second on-box JSON dialect | `Ledger` (ledger.rs) as the template and, for the ledger itself, `Ledger::to_bytes/parse` | Strict parse, size cap, case-fold and gateway-owned-file rules exist |
| Path safety | New validators | `validate_mirror_paths`, `mirror_path_problem`, `safe_remote_path`, `windows_component_problem`, `component_problem` | Windows device names, trailing dot, 240-char limit already handled (transport/mod.rs, winps.rs, remote.rs) |
| PowerShell invocation | Another encoder | `RemoteCommand::powershell_encoded`, `exec_powershell`, `parse_ign_result`, `interpret_output` | One UTF-16LE encoder; CLIXML handled; exit-code plus sentinel decide success |
| Designers list | A new endpoint | `GatewayApi::designers` / `sessions()` (actions/sessions.rs:71); `DesignerInfo{user, project, address}` | D-15 needs exactly user, project, host |
| REST write-permission probe | A lock-and-let-expire probe | `GatewayApi::scan_projects()` (client/mod.rs:1226), as `ign doctor --check-write` uses it (actions/doctor.rs:393-420) | A lock acquire would start the hold timer; the probe is harmless and maps 403 to `Auth{status:403}` |
| `.gwbk` download | A new stream | `actions::backup::backup_download(api, Some(out), stem, BackupType)` | Passing `out` skips the `.part` rename dance (backup.rs:72-85). Pre-create the file 0600 first (`download_to_file` uses `File::create`, which keeps an existing file's mode) |
| UTC timestamp | A date crate | `unix_ms_to_rfc3339_utc` pattern in actions/workspace.rs:976-1000 | Zero dependencies |
| Atomic JSON file write | Plain `write` | temp + rename in the same dir (precedent: pull's `create_new`, ledger) | Journal and record are read by other runners |

**Key insight:** every hard part of this phase is already a tested pure function or a typed seam. The new code is the orchestration, three small file formats, one script contract implemented three times, and the discipline of proving the three implementations agree.

## Runtime State Inventory

Not a rename or migration phase. Omitted. (New on-box state this phase introduces is listed in Pattern 4 and in the README doc obligations; nothing existing is renamed.)

## Common Pitfalls

### Pitfall 1: The lock may not stop anyone, and nothing proves what it does stop
**What goes wrong:** The design treats the lock as the guard between disk and gateway. Phase 20 proved it is "a courtesy signal, not mutual exclusion for scans" (20-LIVE-CAPTURES.md D4: a non-holder's scan POST answered 200 and ran), cannot be extended (D2), and has no release route (D8). The official 8.3 docs say it prevents "updates to Gateway resources until either a timeout occurs, or a Gateway configuration scan is performed" `[CITED: docs.inductiveautomation.com/docs/8.3/appendix/scripting-functions/system-config]`, which is the C2 question stated as a claim, not a capture.
**Why it matters:** If it does not block UI, Designer or API writes, a write racing the apply is only caught by the under-lock pre-image check and the Designer preflight. If a file write with no lock triggers an automatic scan, a torn tree can load without anyone calling scan.
**How to avoid:** Capture C2 with five sub-probes (below) before any fault-injection assertion is written. Build the pre-image check regardless; it is cheap and independent of C2.
**Warning signs:** `plan_stale` raised from the under-lock check in a live run with no foreign actor.

### Pitfall 2: Hold budget is a function of the script, and the script does not exist yet
**What goes wrong:** D-02 says re-run OQ5 with the batched commit. The commit constant cannot be measured before the script is written, but the engine's `HOLD_S` is needed to write the engine.
**How to avoid:** Order the plans: captures that need no script, then the host layer with an explicit `HOLD_S` placeholder behind a test that fails until the measured value is committed, then the re-measure, then the engine. D-03a: if the re-measure is above 300 s, stop at the `checkpoint:decision`.
**Numbers to carry (22-LIVE-CAPTURES.md OQ5-1/OQ5-2):** old p95 commit 251.063 s (run A) and 348.225 s (run B), p95 scan wall 0.888 s and 0.642 s, formula results 505 s and 699 s, ssh connect p95 1.688 s and 2.423 s. The new window is connect + script + scan, so the floor of 10 s is likely to bind. `[ASSUMED]` until measured.

### Pitfall 3: The estimate (D-03) and the constant (D-02) are two different numbers
**What goes wrong:** A fixed `HOLD_S` measured at 152 files cannot also accept a 1,300-file tree.
**How to avoid:** `HOLD_S` is one constant. The refusal test is `ceil(2 x (est_commit(n) + p95_scan) + 1) <= HOLD_S`, so the supported tree size is derived from the per-file slope. Measure at scale 1 (152 files) and scale 10 (1,520) to fit intercept and slope (OQ5-1 note: 91 KB took 78 to 100 s to stage, so a scale-10 stage is about 13 to 17 minutes per iteration; stage once and iterate commit then restore).

### Pitfall 4: Existing error tests forbid the hint that D-09 requires
**What goes wrong:** `deploy_slugs_have_their_exit_codes` (error.rs:2918-2955) asserts `!hint.contains("`ign ") && !hint.contains("ign ")` for every `DEPLOY_SLUGS` entry and asserts `codes == CoreError::DEPLOY_SLUGS` in order. D-09 requires the `deploy_journal_present` refusal to print `ign deploy rollback --run <id>`.
**How to avoid:** Put the command in the variant's `Display` (payload `run_id`, `env`), not only the hint, and carve the new variants out of the hint rule explicitly (the rule's purpose is "a hint must never name a command that does not fix it", and this one does). Append new slugs to `DEPLOY_SLUGS` in a fixed order and update the pinned test.

### Pitfall 5: `EXCLUDED_LEAVES` matches ANY path segment
**What goes wrong:** `path_is_excluded` splits the path on spaces and tests each segment against `["completions","mcp","lsp","tui","edit"]` (mcp.rs:62, 105-108). Adding `"apply"` and `"rollback"` there would exclude any future leaf containing those words (for example under `eam`), silently.
**How to avoid:** Add a separate `EXCLUDED_PATHS: &[&str]` matched on the full path (`"deploy apply"`, `"deploy rollback"`, later `"deploy projects"`). Amend the registry-to-catalog test at mcp.rs:833-845, which currently asserts `*path == "edit"` for every guarded-and-excluded path. The independent catalog parity walk in the same file uses `path_is_excluded`, so both sides move together.

### Pitfall 6: `GUARDED_OPS` marker discipline
`guarded_ops_registry_tracks_every_dispatch_site` (main.rs near 4600) requires an adjacent `// guarded:deploy apply` and `// guarded:deploy rollback` marker at each `require_confirmation` site and an entry in `GUARDED_OPS` (main.rs:3933, entry shape `("deploy pull", "deploy pull would replace ...")` at main.rs:4046-4049). Dynamic-prose sites are pinned by the marker alone. The marker is only valid lowercase letters, digits and spaces.

### Pitfall 7: ACL and mode after a same-volume move (C7b)
**Windows:** a same-volume move keeps the file's original permissions; a copy takes the destination's `[CITED: support.microsoft.com/kb/310316]`. A file moved from the account's home staging directory into `production\` may therefore keep the staging ACL, not the collection's, and the gateway service account might not be able to read it. Capture C7b must `icacls` a moved file next to an untouched one. If they differ, reset with `icacls <file> /reset` in the script (the account owns the moved file) or copy then delete.
**Linux:** the sftp-created staged files carry the deploy user's umask. A 0600 file owned by the deploy user is unreadable to the `ignition` user. The script must set file mode 0644 and directory mode 0755 (or the group mode the host uses); capture it. `[ASSUMED]`, no Linux host was captured in Phase 22.
**Sharing violation:** a loaded JDBC jar under `ignition/database-driver/` held open makes the move fail after the bounded retries (`RENAME_RETRIES: u32 = 3`, sftp.rs:1666). That is a `write_failed` abort and restore, not a hang. PITFALLS "Pitfall 17/restart-class" says plan should flag driver-jar changes; plan does not yet (restart-class hints are a Future Requirement).

### Pitfall 8: sftp and `\\?\` blind spots reach the snapshot too
The sftp server resolves no path of 260 characters or more and lists without such entries silently (C7a-8). The snapshot and staging roots add about 100 characters of prefix under the home, so a 140-character collection-relative path may already be unreachable by `get` even though PowerShell with `\\?\` can write it. The Windows path gate (`windows_paths` in sftp.rs, `LONG_PATH_LIMIT = 240` in winps.rs) governs the collection-relative path only; add the snapshot/staging prefix to the gate for any file the run will download, or archive on the box (one file) and download that. Archiving (`tar` on Server 2022, or .NET `ZipFile`) is a discretionary optimization to capture, not a requirement.

### Pitfall 9: Verifying a rollback with the verb that verifies a deploy
After a restore the declared tree no longer matches the gateway, so `deploy_verify(declared)` would be red for a correct rollback. Restore verification is: on-box hashes of the touched set equal the record's pre-images, scan complete and advanced, `deployment_mode` still the target's. Only the post-apply path calls `deploy_verify`.

### Pitfall 10: The journal outlives a verified deploy by one call
Between "verify green" and "journal removed" there is one script call. A kill there leaves a journal and the recovery command would roll back a deploy that verified. D-12 forbids a journal-discard flag. Recommended: accept, document it in the README as the safe direction, and keep the window to one call (Open Question 6).

### Pitfall 11: Two concurrent runs and the staging dir
`staging/<run_id>/` is per run, and leftovers from a killed run are not pruned (D-08). Recovery (`rollback --run`) must also remove that run's staging dir; a plain refusal must not. Document manual cleanup.

### Pitfall 12: The canary scope contradicts the run directory
DSURF-03 says no secret appears in "run-directory files". D-05 says the run dir holds the downloaded snapshot and the `.gwbk` and "holds secrets". Both are true only if the canary scans `run.json`, `result.json`, `plan.json`, `events.ndjson` (and stdout, stderr, `--json`, the on-box journal and record) and **explicitly exempts** `snapshot/**` and `*.gwbk`, while a separate assertion pins those at 0700/0600 and git-ignored. State the exemption in the test and the README so it is a decision, not a gap.

### Pitfall 13: Windows-only code is only checked by CI
22-16 needed six CI rounds because the `cfg(windows)` code never compiled locally (STATE.md, 22-16). The run-dir ACL (`icacls` directory variant), the non-unix LocalFs apply, and the PowerShell script conformance run only on windows-latest. Land them early and in small commits; `cargo clippy --workspace --all-targets -- -D warnings` is the CI gate (ci.yml:40).

### Pitfall 14: Ledger and run record are written under different failure modes
If the ledger swap succeeds but the process dies before the record's post part, the journal exists and `rollback --run` restores both (the ledger is in the snapshot). If the run record is lost but the journal and snapshot survive, recovery still works from the journal's `touched` list. Do not make recovery depend on the post part of the record.

### Pitfall 15: Local clock and gateway clock
Never compare `ScanLockInfo.timestamp` (opaque, 380308558 in C1c) or `lastScanTimestamp` (gateway epoch ms) with the runner clock (scan.rs module doc). `HeldLock` reads only `remaining` and its own monotonic `Instant`.

## Code Examples

Only shapes and values verified verbatim this session. Everything else in a skeleton is labelled.

### In-repo values the skeletons rely on (verbatim)

```rust
// crates/ignition-core/src/deploy/ledger.rs:136-144
pub fn ledger_rel(gateway: &str, mode: &str) -> String {
    format!("ign-deploy/{gateway}/{mode}/{LEDGER_FILE}")
}
pub fn staging_rel(gateway: &str, mode: &str, run_id: &str) -> String {
    format!("ign-deploy/{gateway}/{mode}/staging/{run_id}")
}
// ledger.rs:72   pub const LEDGER_FILE: &str = "ledger.json";
```
```rust
// crates/ignition-core/src/deploy/plan.rs:134-145  (serde rename_all = "lowercase")
pub enum PlanStatus { Added, Changed, Removed, Same, Unmanaged }
// plan.rs:163-175  (serde rename_all = "snake_case")
pub enum CoreRelation { None, Identical, Differs, Hidden, ShadowsRemoval }
```
```rust
// crates/ignition-core/src/client/scan.rs:107-124
pub enum LockVerdict { Held { remaining_s: i64 }, Expiring { remaining_s: i64 }, NotHeld }
// crates/ignition-core/src/client/deploy_api.rs (trait DeployApi)
async fn scan_lock_acquire(&self, kind: ScanKind, acquire_s: u64, hold_s: u64) -> Result<(), CoreError>;
async fn scan_lock_info(&self, kind: ScanKind) -> Result<LockObservation, CoreError>;
// deploy_api.rs:930
pub async fn scan_and_wait(api: &dyn DeployApi, kind: ScanKind, cfg: PollConfig) -> Result<ScanOutcome, CoreError>;
```
```rust
// crates/ignition-core/src/error.rs:1564-1569
pub const DEPLOY_SLUGS: &'static [&'static str] = &[
    "deploy_tree_invalid",
    "embedded_secret",
    "transport_failed",
    "deploy_ledger_invalid",
];
// crates/ignition-core/src/deploy/verify.rs:82
pub const VERIFY_FAILED_SLUG: &str = "deploy_verify_failed";
// crates/ignition-cli/src/mcp.rs:62
pub(crate) const EXCLUDED_LEAVES: &[&str] = &["completions", "mcp", "lsp", "tui", "edit"];
// crates/ignition-core/src/deploy/transport/winps.rs:55    pub const COMMAND_BUDGET: usize = 8000;
// crates/ignition-core/src/deploy/transport/sftp.rs:1666   const RENAME_RETRIES: u32 = 3;
// crates/ignition-core/src/deploy/target.rs:114            pub const DEFAULT_OP_TIMEOUT_S: u32 = 1050;
```

### Hold budget (pure; the numbers are placeholders until the re-measure)

```rust
// Formula quoted from 22-LIVE-CAPTURES.md OQ5-2: ceil(2 x (p95 commit + p95 scan) + 1), floor 10 s, ceiling 300 s
pub const HOLD_FLOOR_S: u64 = 10;
pub const HOLD_CEILING_S: u64 = 300;
pub fn hold_for(p95_commit_s: f64, p95_scan_s: f64) -> u64 {
    let raw = (2.0 * (p95_commit_s + p95_scan_s) + 1.0).ceil() as u64;
    raw.clamp(HOLD_FLOOR_S, HOLD_CEILING_S)           // clamp is for the constant's derivation only (D-03a: never clamp silently at runtime)
}
// D-03: refuse when the estimate does not fit the one constant.
pub fn fits(hold_s: u64, est_commit_s: f64, p95_scan_s: f64) -> bool {
    (2.0 * (est_commit_s + p95_scan_s) + 1.0).ceil() as u64 <= hold_s
}
```
The unclamped value of `hold_for` must be what the capture records, and a test must fail if the committed `HOLD_S` is not `hold_for(measured...)` of the recorded numbers.

### Gate evaluation (pure)

```rust
// Source: PlanEntry fields read at plan.rs:203-236; CoreRelation::ShadowsRemoval at plan.rs:174
pub enum Gate { Stale, Blocked(Vec<String>), NeedsDestructive(Vec<String>), NeedsDrift(Vec<String>), NeedsYes, Go }
pub fn evaluate_gates(plan: &DeployPlanResult, flags: &GateFlags) -> Gate { /* order per Pattern 2 */ }
```
Entries considered: those with `status != Unmanaged` (unmanaged never counts, plan.rs `has_changes` doc). `destructive` entries need the flag; `drift == Some(true)` entries need the reason; `core == ShadowsRemoval` blocks.

### Fault-injecting host

```rust
// Pattern 7. `ApplyHost` is NEW (proposed); the wrapper shape is the contract the tests need.
pub struct FaultyHost<H> { inner: H, fail_at: Option<usize>, hang_at: Option<usize>, ops: AtomicUsize }
// test: spawn run_apply(...) on a task, wait for the hang signal, task.abort(), then assert:
//   journal.json present, snapshot complete, collection partially written, plan/apply/rollback refuse,
//   `rollback --run <id>` restores the collection to the pre-apply listing.
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Per-file sftp rename + fetch-back verification inside the lock (22-13 `commit_inner`) | One on-box batched script; hashes computed on the box | This phase (D-01) | Lock window from minutes to (hypothesis) seconds |
| Spec §6.4.3 "5 s hold" from a local `docker cp` | Measured, floored, ceilinged constant | 22-14 / D-02 | The spec figure was off by two orders of magnitude on the real transport |
| Spec §6.4.6 apply runs the rolling restart | Apply reports `restart_pending`, never restarts (SF-3) | Scoping | Needs the `restart-tasks/pending` read, never captured |
| `plan_id` / `--expect-plan` (ARCHITECTURE) | `plan_sha256` / `--plan-sha256`, error `plan_stale` | Research conflict resolution 8 | Names settled |

**Deprecated/outdated in the research docs:** ARCHITECTURE §7.1 slug names `scan_lock_expired` and `deployment_mode_mismatch` as separate `CoreError`s. Under Decision A and the abort-restore design, lock expiry is a `deploy_rolled_back` cause and the mode mismatch is a preflight check.

## New Error Slugs (four-place rule: variant + `code()`, `exit_code()`, `hint()`, header table + README exit table + `exit_code_mapping_enumerated` + `DEPLOY_SLUGS`)

Names below are **my recommendation and are public contract once shipped**; confirm before the first commit (Assumption A3).

| Slug | Exit | Class | Raised when | Payload |
|------|------|-------|-------------|---------|
| `plan_stale` | 6 | target_state | `--plan-sha256` differs from the recomputed plan, or the under-lock pre-image check fails | expected, actual (hashes) |
| `destructive_change_refused` | 2 | usage | destructive entries without `--allow-destructive`, or drift without `--allow-drift` | `gate`, first N entry keys, count |
| `deploy_blocked` | 6 | target_state | `shadows_removal` entries (not satisfiable by any flag) | entry keys |
| `deploy_preflight_failed` | 6 | target_state | one or more preflight checks failed, all listed (includes `hold_budget`, `run_dir_not_ignored`, `designers`, a failed `.gwbk` download) | `Box<Vec<(check, detail)>>` |
| `deploy_journal_present` | 6 | target_state | journal exists (plan, apply, rollback of another run) | `run_id`, `env`; Display carries `ign deploy rollback --run <id> --env <env>` |
| `deploy_live_changed` | 6 | target_state | rollback: live touched set differs from `post_fingerprint` and no `--discard-live-changes` | run_id, paths (capped) |
| `deploy_run_invalid` | 6 | target_state | rollback: no on-box record for the run id, a corrupt or mismatching record | run_id, problem |
| `rollback_failed` | 6 | target_state | the restore itself failed; gateway state unknown; journal kept | run_id, run dir, on-box path, cause, guidance |

Verdict (payload only, never an exit-table row, never in `DEPLOY_SLUGS`): `deploy_rolled_back`, with `data.ok=false`, `data.cause` in {`lock_expired`, `write_failed`, `scan_incomplete`, `verify_failed`}, exit 6 after the envelope, the same shape as the verify exception (main.rs:564-577). A test mirroring `verify_verdict_exception_is_dated_and_documented` (deploy_family_coverage.rs:177-226) must pin the README text for it. `apply` outcomes in `data.outcome`: `applied`, `noop`, `rolled_back`. Existing reused slugs: `confirmation_required` (2), `invalid_input` (2), `transport_failed` (4), `scan_lock_held` (6), `network_error` (4), `auth_rejected` (5).

## Captures Owed (record in `23-LIVE-CAPTURES.md` before the code that depends on them)

| ID | Question | Needs | Blocks |
|----|----------|-------|--------|
| C2 | (a) with the lock held by token A, does an API resource write by token B succeed? (b) does a Designer or UI save to the active mode collection succeed (manual)? (c) does a file written into the mode collection with NO lock cause an automatic scan, and how soon? (d) does a file written under a held lock cause one? (e) when a lock lapses with unscanned files, does the gateway scan on its own? | Local rig (`capture22.sh up22`, bind-mounted resources) | Meaning of every fault-injection assertion; whether the Designer preflight is the only guard |
| C11 | Provider and its definitions in one scan: loaded in one pass or two? | Local rig | Whether the engine needs a second scan |
| C-restart | Live shape of `GET /data/api/v1/restart-tasks/pending`. The OpenAPI in `~/.cache/ign-deploy-rig/openapi-8.3.6.json` declares `{"pending": [string]}` with examples `["Module Uninstalled: com.test.test", "Module Enabled: com.test.test2"]` `[VERIFIED: openapi-8.3.6.json, read this session]`; DAPI-06 requires a live capture before a `DeployApi` method (and `DEPLOY_ENDPOINTS` row) exists. client/restart.rs:16 documents the endpoint as deliberately absent today | Local rig | DAPL-10 `restart_pending` |
| C-gwbk | Is a `.gwbk` a zip; truncation behaviour of `download_to_file`; size and time on the rig; Content-Length present? | Local rig | D-13 "truncated download" detection |
| C-sysname | `gateway-info.name` against `ignition/system-properties` `systemName` on the rig (corpus fixture shows `"systemName": "whk-development-gw"`, tests/fixtures/deploy/corpus/ignition/system-properties/config.json:26) | Local rig | Wrong-target guard (Open Question 1) |
| C-script-run | Windows: does `& ([ScriptBlock]::Create(ReadAllText(file)))` run under the host's execution policy; `-File` alternative; PowerShell 5.1 `Move-Item -Force`/`File.Replace` behaviours inside one script | Windows host | The PowerShell script design |
| C7b | ACL after a same-volume move vs the collection's ACL; sharing violation on a loaded JDBC jar; Defender cost; long paths; Linux file modes | Windows host (+ a Linux host if Linux ssh apply is in scope) | Script ACL/mode steps |
| OQ5-rerun | p95 of connect + script + scan with the batched commit at 152 and 1,520 files; derives `HOLD_S` and the per-file slope | Windows host, after the script exists | D-02, D-03, D-03a |

Existing facts to build on (22-LIVE-CAPTURES.md, quoted in 22-12): a file renamed over an existing file replaces it; a directory renamed over a non-empty directory fails; a rename onto itself deletes the file; a held target fails with `Failure`; `mklink /D` refused, junctions work; PowerShell stdout is OEM 437 so output stays ASCII; a command of 8154 characters works and 8162 fails.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Snapshot, journal and both fingerprints are defined over the **touched set** (writes, deletes, ledger), not the whole collection. CONTEXT/ROADMAP say "snapshots the target collection" and "collection equal to the snapshot" | Pattern 3, 5 | If the user meant whole-collection, the snapshot size, download time and secret exposure grow and the manifest-free rollback needs a path-set anyway. Tests can still assert whole-collection equality before/after |
| A2 | `.gwbk` is a ZIP archive whose central directory proves completeness | Standard Stack, C-gwbk | Truncation detection falls back to size and Content-Length only |
| A3 | The eight slug names and exits in the New Error Slugs table | New Error Slugs | Public contract once shipped; rename costs a breaking change |
| A4 | Windows can run the uploaded script via `[ScriptBlock]::Create(ReadAllText(file))` regardless of execution policy | Pattern 1 | Fall back to `-ExecutionPolicy Bypass -File`, or chunk the script across several `EncodedCommand` calls inside one lock window (each costs a connect, p95 up to 2.4 s) |
| A5 | A same-volume move keeps the staging ACL on Windows and the script must `icacls /reset` | Pitfall 7 (the rule itself is `[CITED: support.microsoft.com/kb/310316]`; the effect on this host is unproven) | Gateway cannot read moved files, or a needless ACL step |
| A6 | Linux staged files need explicit modes (0644/0755) for the `ignition` user | Pitfall 7 | A deploy that "succeeds" and a gateway that cannot read the files |
| A7 | The batched commit window will be seconds, so the 10 s floor binds | Pitfall 2 | If above 300 s, D-03a checkpoint |
| A8 | Windows write-permission probe = open `config-mode.json` for write without writing; POSIX = `test -w` on the collection dir | Pattern 1 / preflight | A false positive or a probe that touches a gateway-owned file; fallback: rely on the first move failing safely |
| A9 | A journal kill window after "verified" is acceptable and documented | Pitfall 10 | Operator rolls back a good deploy after a rare kill |
| A10 | A Ctrl-C handler is not needed in v1.6 | Standard Stack | A Ctrl-C leaves a held lock to expire and, after journal creation, a journal to recover |

## Open Questions

1. **Wrong-target guard (DAPL-04).** The target schema (`target.rs`, `deny_unknown_fields`) has no identity field, and `Manifest` has none either. REST `gateway-info.name` exists (client/version.rs:20-23); the host exposes `systemName` in `ignition/system-properties`. Recommendation: bind REST to host (REST `name` must equal the host `systemName`, read with one extra scoped `TreeReader::read` of that singleton) so a profile pointing at dev while ssh points at prod is caught, and add an **optional** `nodes.*.gateway_name` to the target schema for an explicit declared identity. The schema field is "costly" reversibility (22 D-01 precedent); needs a user call. Capture C-sysname first.
2. **Journal check placement.** DAPL-04 lists "no on-box journal" among the preflights reported together; D-09 says apply refuses with `deploy_journal_present`. Recommendation: check it before the plan (it is host-only and plan refuses anyway) and return `deploy_journal_present` immediately, so one stable slug serves plan, apply and rollback. This narrows the literal DAPL-04 wording.
3. **Decision C enforcement ("CI and agents must pass `--plan-sha256`, optional interactively").** No code defines "interactive". Recommendation: required (`invalid_input`, exit 2, before any gateway contact) unless both stdin and stdout are terminals; `std::io::IsTerminal` is already used in main.rs for `tui`. All assert_cmd tests then pass the flag, which is desirable.
4. **`plan_stale` vs `noop` order** when the reviewed hash is non-empty and the live plan is now empty. Recommendation: `plan_stale` (strict binding), exit 6.
5. **Whole-unit write rule and `same`-in-`core` units** (Pattern 3). Recommendation as written; confirm that declared units that already equal `core` stay in `core` (DAPL-03 forces this).
6. **Journal at phase `verified`.** Recovery could finalize instead of restoring. Recommendation: do not add the behavior; document the safe direction (A9).
7. **Windows host for the OQ5 re-run and C7b.** See Environment Availability. A human checkpoint opens planning.
8. **Linux ssh apply.** Phase 22 captured only Windows. Recommendation: ship the `sh` script and test it locally against a temp dir, mark Linux ssh apply "unproven against a live host" in the README until a host is available.
9. **`restart_pending` when the token lacks permission for the endpoint.** Recommendation: a warning `restart_pending_unavailable`, never a failure.
11. **"Every run writes a run directory" (DAPL-10) versus refusals.** A refusal before the plan gates pass cannot write one (the ignore guard in D-05 must run first, and a noop must create nothing). Recommendation: create the run directory right after the ignore guard passes and before the other preflights, so a preflight failure and everything later is recorded; flag errors, journal, `plan_stale`, noop, gate and confirmation refusals write nothing. This is my reading of "every run"; it needs a user nod.
10. **Spec §9 amendment in whk-environment-orchestration** (the service account may write `ign-deploy/` in its home) is still to do and is outside this repo (22-LIVE-CAPTURES.md Decision B caveat 1). It must exist before the write flow ships.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain | build, tests | yes | rustc 1.94.1, cargo 1.94.1 (MSRV 1.88) | — |
| Docker (OrbStack) | local rig for C2, C11, C-restart, C-gwbk, live gate | yes | 29.8.2; image `inductiveautomation/ignition:8.3.6` present | — |
| Rig state and recipe | live gate | yes | `~/.cache/ign-deploy-rig/` (openapi, tokens.env, logs), `crates/ignition-core/tests/fixtures/live-rig-deploy/capture22.sh`, `compose.bind.yml` | `capture22.sh up22` / `setup22` (a trial reset may be needed) |
| `ssh`, `sftp` | transport | yes | `/usr/bin/ssh`, `/usr/bin/sftp` | — |
| `git` | D-05 ignore guard | yes | 2.46.2 | — |
| `sh`, `sha256sum`/`shasum` | POSIX script conformance | yes | `/sbin/sha256sum`, `/usr/bin/shasum` | — |
| `pwsh` | optional local PowerShell smoke | yes | 7.5.4 (not Windows PowerShell 5.1) | CI windows-latest |
| `az` CLI | start/inspect the Windows capture VM | yes | 2.84.0, logged in as pmannion@whiskeyhouse.com | — |
| **Windows capture host `ign-win-836`** | D-02 OQ5 re-run, C7b, C-script-run, live Windows proof | **UNVERIFIED, probably gone** | — | Human re-provision (see below) |
| Windows PowerShell 5.1 locally | script conformance | no | — | windows-latest CI leg; the capture host |
| Linux ssh host | Linux apply proof | no | — | Local `sh` conformance only (Open Question 8) |

**The Windows host:** `az vm show -d -g rg-ign-cli-win-test -n ign-win-836` returns `ResourceGroupNotFound` in the default subscription "Microsoft Azure Sponsorship"; the other two subscriptions are inaccessible from this session (one needs re-login, one is `AuthorizationFailed`). The pinned `known_hosts` is still at `~/.cache/ign-wflwvm01/known_hosts`, but `IGN_WINVM_*` is not set in this shell. D-02, C7b and C-script-run all need a Windows OpenSSH host with `svc-ignition-deploy` holding `Modify` on the mode collection (the post-C6 grant of 2026-10-15 is recorded in STATE.md for the Azure VM; WFLWVM01 would need its own administrator grant).

**Missing with no fallback:** a Windows OpenSSH + Ignition host for the hold re-measure. Planning must open with a human checkpoint to confirm or re-provision it (the same pattern as 22-12).
**Missing with fallback:** Windows PowerShell 5.1 locally (use CI), a Linux ssh host (use local `sh`).

## Validation Architecture

Nyquist validation is enabled (`workflow.nyquist_validation: true`, .planning/config.json).

### Test Framework

| Property | Value |
|----------|-------|
| Framework | cargo test (built-in harness) with `wiremock`, `assert_cmd`, `predicates`, `proptest`, `tempfile` (ignition-core Cargo.toml read) |
| Config file | none beyond `Cargo.toml`; CI at `.github/workflows/ci.yml` runs fmt, clippy `-D warnings`, `cargo test --workspace`, `cargo build -p ignition-cli --no-default-features` on ubuntu, macos, windows |
| Quick run command | `cargo test -p ignition-core --test deploy_apply` |
| Full suite command | `cargo test --workspace` (2151 passed, 0 failed, 51 ignored at Phase 22 HEAD, 22-VERIFICATION.md) |
| Live gate | `IGNITION_LIVE_DEPLOY=1 ... cargo test -p ignition-core --test live_deploy_apply -- --ignored --nocapture` (NEW, mirrors live_deploy_plan.rs) |

### Phase Requirements to Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| DAPL-01 | no `--yes` refuses with the plan prose and `plan_sha256`; stale hash -> `plan_stale`; correct hash proceeds; precedence matrix over the gate order | unit (pure gates) + CLI | `cargo test -p ignition-core --lib deploy::apply::gates` ; `cargo test -p ignition-cli --test contract_deploy apply_gates` | Wave 0 |
| DAPL-01 | `--plan-sha256` required when stdin/stdout are not terminals | CLI | `cargo test -p ignition-cli --test contract_deploy apply_requires_plan_hash_when_noninteractive` | Wave 0 |
| DAPL-02 | destructive, drift (with `--base`), `shadows_removal` each refuse with the right slug and exit; `--allow-drift` reason is recorded in `run.json` and capped | unit + engine | `cargo test -p ignition-core --test deploy_apply gates_` | Wave 0 |
| DAPL-03 | empty plan: exit 0, zero lock acquires, no run dir, no host write (counting wrapper) | engine | `cargo test -p ignition-core --test deploy_apply empty_plan_is_a_noop` | Wave 0 |
| DAPL-04 | each failing check alone and all together appear in ONE `deploy_preflight_failed`; modes on two `FakeGateway`s; Designers `fail`/`wait`(timeout shortened)/`proceed`; journal check; hold-budget refusal names estimate and budget; ignore guard | engine | `cargo test -p ignition-core --test deploy_apply preflight_` | Wave 0 |
| DAPL-04 | `git check-ignore` guard: ignored, not ignored, not a repo | unit | `cargo test -p ignition-core --lib deploy::apply::run_store` | Wave 0 |
| DAPL-05 | on-box snapshot has a hash per touched file; run-dir copy equals it; `.gwbk` written 0600, truncated/empty download refuses, `--skip-gwbk` recorded; nothing else leaves the box | engine (wiremock backup route) | `cargo test -p ignition-core --test deploy_apply snapshot_` | Wave 0 |
| DAPL-06 | lock expiry (FakeGateway::tick / ManualClock) and a failed Nth write both end with collection listing == pre-apply listing, a scan issued, `deploy_rolled_back`; no scan is issued BEFORE the restore on expiry | engine fault injection | `cargo test -p ignition-core --test deploy_apply fault_` | Wave 0 |
| DAPL-06 | hold constant equals `hold_for(recorded p95s)`; estimate refusal boundary | unit | `cargo test -p ignition-core --lib deploy::apply::hold` | Wave 0 |
| DAPL-07 | red `deploy_verify` triggers the same `restore_snapshot`; exit 6, `data.slug == "deploy_rolled_back"`, cause `verify_failed`; failed restore -> `rollback_failed` with journal kept | engine + CLI | `cargo test -p ignition-core --test deploy_apply auto_rollback_` ; `cargo test -p ignition-cli --test contract_deploy apply_rolled_back_exit_6` | Wave 0 |
| DAPL-08 | fresh-process rollback by run id from the on-box record alone (no run dir, no manifest); stale touched set refuses `deploy_live_changed`; `--discard-live-changes` proceeds; rollback of an older run behind a newer one refuses | engine + CLI | `cargo test -p ignition-core --test deploy_apply rollback_` | Wave 0 |
| DAPL-09 | task aborted after N ops leaves `journal.json`; `plan`, `apply`, `rollback` of another run refuse with the exact command in the message; `rollback --run` restores; pre-write death with matching fingerprint clears without restoring (D-10); interrupted run skips staleness (D-11) | engine crash simulation | `cargo test -p ignition-core --test deploy_apply journal_` | Wave 0 |
| DAPL-10 | run dir exists after success, a preflight failure, a post-write failure and a rollback; NO run dir for flag errors, journal/stale/noop/gate/confirmation refusals (Open Question 11); modes 0700/0600; `result.json` always written once the dir exists; `restart_pending` is the post-minus-pre delta and the engine calls no restart route | engine | `cargo test -p ignition-core --test deploy_apply run_dir_` | Wave 0 |
| DSURF-02 | `deploy apply`/`deploy rollback` in `GUARDED_OPS`, absent from catalog; `deploy render/plan/pull/verify` present; guarded-and-excluded test amended | CLI unit | `cargo test -p ignition-cli mcp::tests` ; `cargo test -p ignition-cli guarded_ops_registry_tracks_every_dispatch_site` | exists, must be amended |
| DSURF-01 (keep green) | README Commands rows, TUI route rows (`deploy apply`, `deploy rollback` -> `Screen::Projects`), exit-table rows for every new slug | CLI | `cargo test -p ignition-cli --test deploy_family_coverage` | exists |
| DSURF-03 | canary seeded in live config, gwbk bytes, gateway log/error bodies and ssh stderr; absent from stdout, stderr, `--json`, `run.json`, `result.json`, `plan.json`, `events.ndjson`, journal, record, for success, noop, rolled_back, refusals and `rollback`; `snapshot/**` and `*.gwbk` exempt AND asserted present at 0600 | CLI + core | `cargo test -p ignition-cli --test contract_deploy apply_canary` | Wave 0 |
| write-half guard | only `deploy/apply/` and `deploy/transport/` name the write half | source scan | `cargo test -p ignition-core --test deploy_transport write_half_unreachable` | exists, must be amended |
| stdout purity | `apply --json` and `rollback --json` stdout is exactly the envelope | CLI | `cargo test -p ignition-cli --test contract_stdout_purity` | exists, add cases |
| Host parity | the same scenario table (commit, restore, pre-image mismatch, held target retry, self-move guard, delete + empty-dir removal, ledger swap, fingerprint) passes on LocalHost, the POSIX script run locally via `sh`, and the PowerShell script on windows-latest | conformance | `cargo test -p ignition-core --test deploy_host_conformance` | Wave 0 |
| Live | rig apply, kill, rollback; C2/C11 captures; Windows apply and timing | live, `#[ignore]` | see Live gate | Wave 0 / human |

### Sampling Rate
- **Per task commit:** `cargo test -p ignition-core --test deploy_apply` plus the touched module's `--lib` filter.
- **Per wave merge:** `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`.
- **Phase gate:** full suite green on all three CI legs, the live rig gate run once, the Windows live proof recorded, then `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] `crates/ignition-core/tests/deploy_apply.rs`: engine suite.
- [ ] `crates/ignition-core/tests/deploy_host_conformance.rs`: shared scenario table.
- [ ] `crates/ignition-core/tests/common/` additions: a `FakeGateway` coupling that **loads the mode-collection directory on scan** (today it holds REST resources only, fake_deploy.rs), `restart_pending` and `gateway_info` name support, two-gateway setup for the both-nodes preflight, a `FaultyHost`, a git-repo tempdir fixture for the ignore guard.
- [ ] A `GatewayApi` double for the apply-only calls (`designers`, `backup_download`, `scan_projects`, `logs`, tags): use `ReqwestGatewayApi::for_tests(&server.uri(), None)` + wiremock as `deploy_verify.rs` does (GatewayApi has many doubles that `unreachable!()`; do not add another full impl).
- [ ] `crates/ignition-core/tests/live_deploy_apply.rs` (`#[ignore]`, `IGNITION_LIVE_DEPLOY=1`, loopback URL guard and gateway-name guard copied from live_deploy_plan.rs).
- [ ] `crates/ignition-core/src/deploy/transport/scripts/commit.sh` and `commit.ps1`.
- [ ] `23-LIVE-CAPTURES.md` with the Captures Owed table.
- [ ] Migrate, do not silently delete, the sftp commit tests that pin the per-file path (`windows_commit_*`, sftp.rs:4655, 4714 and neighbours) when the ssh commit is replaced.

## Security Domain

`security_enforcement` is enabled (ASVS level 1, block on high; .planning/config.json).

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes | Existing profile secret chain (`Session::resolve_side`); credentials never in argv or events. Host key pinned (`StrictHostKeyChecking=yes`, 22 D-01) |
| V3 Session Management | no | Stateless CLI; the scan-lock is not a session |
| V4 Access Control | yes | Guarded ops (`--yes`), plan-hash binding (`--plan-sha256`), `shadows_removal` unoverridable, no `--force`, MCP exclusion, write-permission probe |
| V5 Input Validation | yes | `--run` id `[A-Za-z0-9._-]{1,128}` no leading dot validated before it forms any on-box path (path traversal via `rollback --run ../x`); `--plan-sha256` shape; `--allow-drift` reason capped and control-char free; work-list is data, never interpolated into a command |
| V6 Cryptography | yes | `sha2` only; `getrandom` for the run suffix; never hand-roll; no secrets derived from hashes |
| V7 Error handling and logging | yes | Events and errors are value-free (paths, hashes, ids); `scrub_stderr` for ssh output; canary test |
| V8 Data protection | yes | Run dir 0700/0600 (owner-only ACL via `icacls` on Windows), git-ignore guard, `.gwbk` and snapshot never uploaded, snapshot limited to the touched set |
| V12 Files and resources | yes | Descriptor-relative ops (unix), symlink refusal, no dot-named or foreign entries in the collection, bounded retries |
| V9 Communications | yes | Existing hardened ssh/sftp argv (BatchMode, pinned host key, timeouts) |

### Known Threat Patterns

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Command injection through paths or reasons into the on-box script | Tampering / Elevation | Work-list uploaded as a file and hash-checked; the command carries only run id and hashes; `RemoteCommand::posix` quoting; paths pass `safe_remote_path` and the Windows gate |
| Path traversal via `rollback --run` | Tampering | Run-id validator reused from `default_staging_mode` (transport/mod.rs:437-446); no path built from unvalidated text |
| Secret leakage via events, stdout, errors, ssh stderr | Information disclosure | Value-free structures only; canary over every output channel; run-dir exemption for snapshot and `.gwbk` is explicit and permission-pinned |
| Run dir committed to git | Information disclosure | `git check-ignore` guard before any write (D-05); fail closed if git is absent while inside a work tree (decide in planning) |
| Partial tree loaded after lock expiry | Tampering | Abort-restore, scan only after a `Held` verdict, pre-image check, journal |
| Concurrent apply by two actors | Tampering | Exclusive journal create + lock 409; a second `ign` refuses |
| Torn deploy after a runner kill | Denial of service | Journal + `rollback --run` from any runner; the on-box record is the source of truth |
| Self-lockout via security singletons | Elevation / DoS | Already refused at render (DRND-04); `lockout_risk` flag; lock-first/probe 403 aborts before any file |
| Staging/snapshot tamper by another account on the host | Tampering | Home-directory ACLs (Full control for the account, C6-acl); stub verifies script and work-list SHA-256; staged copies re-hashed on the box before the first move |
| Symlink swap in staging or collection | Tampering | `open_regular_file`, descriptor-relative ops, junction refusal in the Windows enumeration (C7a-5) |

## Sources

### Primary (HIGH confidence)
- Repository, read this session with file and line evidence: `.planning/phases/23-guarded-apply-rollback/23-CONTEXT.md`, `23-DISCUSSION-LOG.md`; `.planning/REQUIREMENTS.md`; `.planning/STATE.md`; `.planning/ROADMAP.md` (v1.6 header, Phase 23); `.planning/research/{SUMMARY,ARCHITECTURE,PITFALLS}.md`; `.planning/phases/22-transport-plan-pull-verify/{22-CONTEXT,22-LIVE-CAPTURES,22-VERIFICATION}.md`; `.planning/phases/20-config-api-client/20-LIVE-CAPTURES.md` (Decisions D1-D12).
- Code: `crates/ignition-core/src/deploy/{ledger,plan,verify,target,manifest,tree}.rs`, `deploy/transport/{mod,sftp,local,remote,winps}.rs`, `client/{scan,deploy_api,backup,mod,redundancy,version,restart}.rs`, `actions/{deploy,backup,sessions,doctor,workspace}.rs`, `error.rs`; `crates/ignition-cli/src/{main,mcp,cli}.rs`; `crates/ignition-cli/tests/{deploy_family_coverage,contract_deploy}.rs`; `crates/ignition-core/tests/{common/fake_deploy,deploy_transport,deploy_verify,live_deploy_plan}.rs`; `crates/ignition-tui/src/routes.rs`; `Cargo.toml`, `.github/workflows/ci.yml`.
- `~/.cache/ign-deploy-rig/openapi-8.3.6.json` (live 8.3.6 OpenAPI): `GET /data/api/v1/restart-tasks/pending` response schema.
- Local probes this session: `git check-ignore` exit codes (0/1/128) with `.ign/` and `.ign` patterns on a non-existent path; `az` resource-group lookup; tool versions.

### Secondary (MEDIUM confidence)
- [How permissions are handled when you copy and move files and folders (Microsoft KB 310316)](https://support.microsoft.com/kb/310316): same-volume move keeps original permissions, copy takes the destination's.
- [Examine permissions when copying and moving files (Microsoft Learn)](https://learn.microsoft.com/en-us/training/modules/configure-manage-file-access/8-examine-permissions-when-copying-moving-files)
- [system.config scripting functions, Ignition 8.3 docs](https://docs.inductiveautomation.com/docs/8.3/appendix/scripting-functions/system-config): the scan-lock endpoints "prevent updates ... until either a timeout occurs, or a Gateway configuration scan is performed" (search result summary; page not opened in full).

### Tertiary (LOW confidence)
- [Forum: 403 on /data/api/v1/scan/projects with an 8.3 API key](https://forum.inductiveautomation.com/t/403-forbidden-on-data-api-v1-scan-projects-using-ignition-8-3-api-key/116321): a custom security level can 403 the projects-scan write probe. Search snippet only.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH. No new dependency; every crate and pin read from Cargo.toml.
- Architecture: MEDIUM-HIGH. Seams and ordering are grounded in read code and captures; the on-box script mechanics on Windows and the touched-set definition are recommendations that need capture or user confirmation (A1, A4, A5).
- Pitfalls: HIGH for the in-repo test/registry traps (Pitfalls 4, 5, 6, 12, 13, `write_half_unreachable`), MEDIUM for platform behavior (ACL, modes, sharing) until C7b.
- Timing: LOW until D-02's re-measure; the seconds-scale window is a hypothesis from the commit_inner read.

**Research date:** 2026-10-16
**Valid until:** 2026-11-15 (stable repo; re-check if Phase 22 code is touched or the gateway version changes)
