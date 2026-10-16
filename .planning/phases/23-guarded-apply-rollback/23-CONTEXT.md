# Phase 23: Guarded Apply & Rollback - Context

**Gathered:** 2026-10-15
**Status:** Ready for planning

<domain>
## Phase Boundary

`ign deploy apply` and `ign deploy rollback --run <id>` against a single gateway (DAPL-01..10, DSURF-02, DSURF-03). An apply ends either verified or with its snapshot restored and scanned. It never leaves a partially loaded tree. Rollback works from a fresh process. Captures C2, C7b and C11 run first. Built on one ephemeral 8.3.6 with `LocalFsTransport`; the both-nodes mode preflight runs against the stateful in-memory fake. Backup checks and backup lag are Phase 24.

Already locked by ROADMAP.md, REQUIREMENTS.md and Phase 22, and not re-decided here:
- Decision A: `deploy_rolled_back` lives in `data`, exit 6, and is never an exit-table row.
- Decision B = B(i): home-relative staging. The ledger and journal sit at `ign-deploy/<gateway>/<mode>/` in the service account home on ssh nodes and at `<data_dir>/ign-deploy/...` on LocalFs nodes. Staging is at `.../staging/<run_id>/`.
- Decision C: `--plan-sha256`. Decision D: `apply` and `rollback` are not MCP tools.
- SF-3: report `restart_pending` and never restart. SF-6: `.gwbk` is on by default and `--skip-gwbk` is recorded.
- There is no `--force`, `--skip-verify` or `--no-rollback`. One `restore_snapshot(run)` serves abort, auto-rollback and manual rollback.
- Lock decisions use the gateway's `remaining` and a monotonic local clock.
- Phase 22 D-05..D-08 still hold: the ledger contract, `unmanaged`, the ledger refusal and `plan_sha256`.
- Phase 20 captured lock facts: a holder that re-acquires gets 409, so a lock cannot be extended (C1d). The holder's own scan releases the lock (C1j / D12).

</domain>

<decisions>
## Implementation Decisions

### Lock window and hold budget (resolves the open STATE.md blocker, OQ5-2 / OQ5-4)
- **D-01:** The commit runs on the box as one batched script, not as per-file sftp renames. Before the lock, apply stages the files over sftp as today and checks the staged hashes. Under the lock, one remote call (PowerShell `EncodedCommand` through `exec_powershell` on Windows, a POSIX `sh` script on Linux ssh nodes, native filesystem calls on LocalFs) re-hashes the staged copies, takes the snapshot (D-04), moves the files into the mode collection, and removes the delete set. The scan follows. The C7a-4 rules still apply inside the script: replace a file by moving it over the old one, never move a path onto itself, move a directory aside before replacing it, and retry a held target a bounded number of times. — **Reversibility:** costly — this replaces the 22-13 commit path inside `TreeMirror::commit` for ssh nodes, and the on-box script becomes part of the write contract.
- **D-02:** The hold timeout is one tested constant. Phase 23 re-runs OQ5 with the batched commit on the Azure VM `ign-win-836` and keeps `ceil(2 x (p95 commit + p95 scan) + 1)`, with a 10 s floor and a 300 s ceiling. There is no per-run scaling and no per-target override.
- **D-03:** Apply refuses before the lock and before any write when the change is too large for the budget. The estimated commit time, based on the run's write and delete counts and the measured per-file cost, must fit the hold. The refusal names the estimate and the budget.
- **D-03a:** If the re-measured hold is still above 300 s, execution stops at a `checkpoint:decision` for the user before the write flow ships. Never clamp silently. The `op_timeout_s` cap of 3600 s (OQ5-4) is revisited at the same checkpoint only if it binds.

### Snapshot
- **D-04:** The on-box snapshot is taken inside the lock, by the same batched script, before the first move. It goes to `ign-deploy/<gateway>/<mode>/snapshots/<run_id>/`, outside every collection, with a hash for every file. The run directory downloads the snapshot after the scan, outside the lock. — **Reversibility:** one-way — the on-box snapshot layout is read by `rollback --run` from any later runner.

### Run directory and run identity
- **D-05:** The run directory is repo-local: `<deploy-repo>/.ign/deploy/<gateway>/<run-id>/`, with the directory at 0700 and its files at 0600 (owner-only ACL on Windows). It holds `run.json`, `result.json`, `plan.json`, `events.ndjson`, the downloaded snapshot and the `.gwbk`. If git does not ignore that path, apply refuses before any write, because the directory holds secrets. — **Reversibility:** costly — the path is documented and users' `.gitignore` files depend on it.
- **D-06:** A run ID is a UTC timestamp plus a random suffix, for example `20261015T142233Z-a1b2c3`. It sorts by time and fits the existing staging rule `[A-Za-z0-9._-]{1,128}`.
- **D-07:** Apply also writes a small on-box run record next to the on-box snapshot. It holds the run id, the snapshot hashes, the pre-run fingerprint and the post-apply fingerprint. `rollback --run <id>` works with only the target file and the run id, so an ephemeral CI runner can roll back a run it did not perform. When the local run directory is present it is used and cross-checked against the on-box record. — **Reversibility:** one-way — the on-box run record format is read across runners and hosts, like the ledger (22 D-05).
- **D-08:** v1.6 keeps every run: nothing prunes run directories or on-box snapshots. The README documents manual cleanup.

### Journal recovery
- **D-09:** `rollback --run <id>` is the recovery command. No new verb is added. The journal names its run id, and the `deploy_journal_present` refusal from `plan`, `apply` and `rollback` prints the exact `ign deploy rollback --run <id>` command. Rollback restores that run's snapshot through `restore_snapshot`, scans, and then removes the journal.
- **D-10:** If the journal shows the run died before any move began, and the live collection hash equals the journal's pre-run fingerprint, recovery removes the journal and the partial snapshot without restoring. Otherwise it restores.
- **D-11:** Recovering an interrupted run skips the post-apply staleness check, because no post-apply fingerprint exists and the tree may be partial. It restores without needing `--discard-live-changes` and records the mid-run state hashes in the new run's events. A rollback of a run that completed keeps the DAPL-08 check.
- **D-12:** No flag discards a journal without restoring it. When a snapshot is lost or corrupt, `rollback_failed` guidance tells the administrator to restore from the `.gwbk` or the run-dir snapshot and then delete the journal at the printed path by hand.

### .gwbk and Designer preflight
- **D-13:** The `.gwbk` is downloaded through the existing `backup_download` into the run directory, after the preflights and before the lock, so it never spends hold budget. A failed or truncated download is a pre-write refusal. Only `--skip-gwbk`, which is recorded, bypasses it.
- **D-14:** `--designers wait` polls the open Designer sessions every 10 s, for up to 600 s by default. `--designers-timeout <secs>` changes the limit. On timeout it refuses with the session list, the same refusal as `fail`.
- **D-15:** `--designers proceed` takes no reason string. `run.json` and the events record that `proceed` was used and the sessions open at preflight time (user, project, host).

### Claude's Discretion
- The internal journal phase markers and their names, as long as D-10 can tell a run that died before any write from one that died during writes.
- How the per-file commit cost for D-03 is derived from the re-measured OQ5 data.
- The exact on-box script structure and how it reports per-file results, as long as it fits the existing `encoded_command` budget or chunks within one lock window.
- The `apply` and `rollback` human output and progress events on stderr, following the existing sink pattern.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone scope and decisions
- `.planning/ROADMAP.md` — v1.6 header (Decisions A–D, capture discipline) and "Phase 23: Guarded Apply & Rollback" (SC-1..5, Capture first C2/C7b/C11, Constraints)
- `.planning/REQUIREMENTS.md` — DAPL-01..10, DSURF-02, DSURF-03 (and the coverage notes on DSURF-02/03)
- `.planning/STATE.md` — Blockers: the OPEN hold-budget entry that D-01..D-03a resolve
- `.planning/research/SUMMARY.md` — slug list (`deploy_journal_present`, verdict slugs), the apply flow, the on-box journal, the run-dir layout, SF-3/SF-6, captures C2/C7b/C11, Phase 23 deliverables (lines ~160-170)
- `.planning/research/ARCHITECTURE.md` — §4.4 transport and staging, §7 slugs and verdicts
- `.planning/research/PITFALLS.md` — P3, P4, P5, P8, P16, P17, P18, P20, P21, P22, P23, P25; C2, C7, C11
- Design spec: whk-environment-orchestration PR #60, `docs/superpowers/specs/2026-10-09-prod-gateway-config-as-code-design.md` (§6.4 apply, §9 service-account scope) — external repo. The §9 amendment for B(i) home-write access is still to do there (22-LIVE-CAPTURES Decision B caveat 1).

### Prior phase artifacts
- `.planning/phases/22-transport-plan-pull-verify/22-CONTEXT.md` — D-05..D-08 (ledger, unmanaged, ledger refusal, plan_sha256), D-17 (`live_in`)
- `.planning/phases/22-transport-plan-pull-verify/22-LIVE-CAPTURES.md` — `## Decisions: Windows OpenSSH host (22-12)` (C7a-1..C7a-6, Decision B), `## Decisions: OQ5 timings, hold budget and timeout defaults (22-14)` (the numbers D-02 re-measures)
- `.planning/phases/20-config-api-client/20-LIVE-CAPTURES.md` — C1b..C1j scan-lock facts (no re-acquire extension, holder scan releases, `remaining`)

### Contract docs
- `README.md` exit-code table and `crates/ignition-core/src/error.rs` — pinned by `readme_exit_table_agreement` and `exit_code_mapping_enumerated`. New slugs follow the four-place rule.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/ignition-core/src/deploy/transport/mod.rs` — `TreeMirror` (`stage`/`commit`), `StagingMode::Separate`, `Staged`, `MirrorReport`, `mirror_for_node`, `default_staging_mode` (already B(i))
- `crates/ignition-core/src/deploy/transport/remote.rs` — `exec_powershell`, `RemoteCommand::powershell_encoded` / `posix`, `classify_sftp_stderr`, hardened ssh argv
- `crates/ignition-core/src/deploy/transport/winps.rs` — `WinScript::move_replace`, `enumerate`, `encoded_command` (with a budget), `parse_ign_result`, `clixml_errors`: the base for the D-01 batched commit script
- `crates/ignition-core/src/deploy/transport/sftp.rs` — the current per-file `commit_inner` (read-back hash then rename batch, rename `Failure` retry); D-01 replaces its under-lock part
- `crates/ignition-core/src/deploy/transport/local.rs` — `LocalFsTransport::with_mirror`, the fault-injection surface
- `crates/ignition-core/src/deploy/ledger.rs` — `LedgerRoot::for_node`, `ledger_rel`, `staging_rel`, `ledger_location`: extend them for the `snapshots/<run_id>/`, journal and on-box run-record paths
- `crates/ignition-core/src/deploy/{plan,verify}.rs` — re-plan plus the `plan_sha256` gate, and master verify for auto-rollback
- `crates/ignition-core/src/client/scan.rs` — `scan_lock_path`, the scan-complete predicate
- `crates/ignition-core/src/actions/backup.rs:72` — `backup_download` for D-13
- `crates/ignition-core/src/actions/sessions.rs:71` — `sessions()` for the Designer preflight (D-14/D-15)

### Established Patterns
- Actions never print. Progress goes to stderr through a sink, and the JSON envelope is the only stdout. The verdict post-write shape is set in `main.rs` (the `ign testing run` precedent).
- `GUARDED_OPS` in `crates/ignition-cli/src/main.rs:3933`, plus `// guarded:` markers, with the drift test `guarded_ops_registry_tracks_every_dispatch_site`.
- The MCP catalog exclusion for guarded deploy verbs is built here (DSURF-02). `render`, `plan`, `pull` and `verify` stay in the catalog.
- New verbs land with TUI rows (`crates/ignition-tui/src/routes.rs`, `deploy_rows_cover_the_family`), README entries and exit-table rows in the same commit series.
- The secret canary started in Phase 22 over `plan` and `verify` and extends to `apply`, `rollback`, run-dir files and events (DSURF-03).

### Integration Points
- `crates/ignition-cli/src/cli.rs` `DeployCommand` (line 622) gains `Apply` and `Rollback`.
- `crates/ignition-core/src/actions/deploy.rs` gains the thin verb functions. New core modules cover the run store, journal, held lock and `restore_snapshot`.

</code_context>

<specifics>
## Specific Ideas

- The refusal for a journal that is present prints a command the user can copy: `ign deploy rollback --run <id> --env <env>`.
- The on-box layout under `ign-deploy/<gateway>/<mode>/` has `ledger`, the journal, `staging/<run_id>/`, `snapshots/<run_id>/` and the run record. Nothing is dot-named and nothing sits inside `data/config/resources/`.

</specifics>

<deferred>
## Deferred Ideas

- Automatic pruning of run directories and on-box snapshots (keep the last N). Not in v1.6 (D-08).
- A per-target `hold_timeout_s` override and per-run budget scaling were not selected (D-02).
- A dedicated `ign deploy recover` verb and an `--abandon-journal` escape were not selected (D-09, D-12).

</deferred>

---

*Phase: 23-guarded-apply-rollback*
*Context gathered: 2026-10-15*
