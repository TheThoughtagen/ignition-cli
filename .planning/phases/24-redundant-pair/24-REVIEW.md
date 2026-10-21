---
phase: 24-redundant-pair
reviewed: 2026-10-21T00:00:00Z
depth: standard
files_reviewed: 35
files_reviewed_list:
  - README.md
  - crates/ignition-cli/src/cli.rs
  - crates/ignition-cli/src/main.rs
  - crates/ignition-cli/src/render.rs
  - crates/ignition-cli/tests/common/mod.rs
  - crates/ignition-cli/tests/contract_deploy.rs
  - crates/ignition-cli/tests/contract_diagnostics.rs
  - crates/ignition-cli/tests/deploy_family_coverage.rs
  - crates/ignition-core/src/actions/deploy.rs
  - crates/ignition-core/src/actions/deploy_apply.rs
  - crates/ignition-core/src/actions/redundancy.rs
  - crates/ignition-core/src/client/deploy_api.rs
  - crates/ignition-core/src/deploy/apply/converge.rs
  - crates/ignition-core/src/deploy/apply/hold.rs
  - crates/ignition-core/src/deploy/apply/mod.rs
  - crates/ignition-core/src/deploy/apply/preflight.rs
  - crates/ignition-core/src/deploy/apply/rollback.rs
  - crates/ignition-core/src/deploy/lint.rs
  - crates/ignition-core/src/deploy/mod.rs
  - crates/ignition-core/src/deploy/pair.rs
  - crates/ignition-core/src/deploy/pair/tests.rs
  - crates/ignition-core/src/deploy/verify.rs
  - crates/ignition-core/tests/common/fake_deploy.rs
  - crates/ignition-core/tests/common/faulty_deploy.rs
  - crates/ignition-core/tests/common/forbid_writes.rs
  - crates/ignition-core/tests/deploy_apply.rs
  - crates/ignition-core/tests/deploy_client_contract.rs
  - crates/ignition-core/tests/deploy_pair.rs
  - crates/ignition-core/tests/deploy_transport.rs
  - crates/ignition-core/tests/deploy_verify.rs
  - crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh
  - crates/ignition-core/tests/fixtures/live-rig-pair/compose.yml
  - crates/ignition-core/tests/fixtures/live-rig-pair/setup-pair.sh
  - crates/ignition-core/tests/live_deploy_apply.rs
  - crates/ignition-core/tests/live_deploy_pair.rs
findings:
  critical: 0
  warning: 7
  info: 6
  total: 13
status: issues_found
---

# Phase 24: Code Review Report

**Reviewed:** 2026-10-21
**Depth:** standard
**Files Reviewed:** 35
**Status:** issues_found

## Summary

Reviewed the phase-24 hunks (diff base `8b04401^..HEAD`) with emphasis on the priorities in the
review brief. Production sources (`converge.rs`, `pair.rs`, `apply/mod.rs`, `preflight.rs`,
`deploy_api.rs`, `actions/*`, `main.rs`, `render.rs`) were read in full or by hunk. Test files
and the rig scripts were read for safety and for places where they could hide a defect. The
large test files (`deploy_apply.rs`, `contract_deploy.rs`, `live_deploy_*.rs`) were skimmed for
assertion quality and unsafe patterns, not line by line.

What holds up under adversarial reading:

- **Backup read-only.** `BackupNode` has private fields and only read methods. The only
  `redundancy_resync` call in the phase is `converge.rs:151`, which goes through `ctx.deploy`
  (the master session). Nothing in `apply/` or `actions/` can reach a write through a
  `BackupNode`.
- **Convergence state machine.** `converge_backup` (`converge.rs:102-117`) and `wait_step`
  (`pair.rs:561`) implement K6/D-07 as stated. `Exhausted` sends the one resync and goes
  straight to the final `tick(true)`. `TimedOut` sends the one resync and runs a second budget
  first. `resync.is_some()` guards a second send. `elapsed >= budget` and
  `budget - elapsed` cannot underflow.
- **No master rollback on an unsynced backup.** The backup phase runs after `finalize`, the
  restart report and the snapshot download. The `AppliedBackupUnsynced` outcome never enters
  `roll_back`.
- **Ordering.** Preflights run, then the backup sample, then gwbk, staging and
  `HeldLock::acquire`. `LockAcquired` is emitted at `mod.rs:1231`, before `host.commit` at
  `mod.rs:1234`.
- **Exit/slug contract.** `failure_exit_code`, `base_result`, `run_apply`'s result doc, the
  `main.rs` hook comment, the `cli.rs` help and the README text agree: exit 6, slug only in
  `data.slug`, not a `CoreError`.
- **Secret names.** Only names are collected and listed. No value is read, and error details
  carry `provider/name` pairs. 404, 500 and 501 are mapped before `classify`, as K7/K8 require.
- **Rig scripts.** Both scripts refuse non-loopback URLs and a state directory inside the repo.
  `capture24.sh` refuses to append any block that contains a token key, redacts `tokenHash`,
  and never enables `set -x`.

No BLOCKER-class defect was found. The warnings below are real behavioral gaps; the most
consequential are WR-01, WR-02 and WR-04.

## Warnings

### WR-01: Sync-exhausted detection reads a 10-minute log window that predates the apply, so a stale line can end the wait immediately

**Fix status: fixed in `bdc0665`.**

**File:** `crates/ignition-core/src/deploy/pair.rs:153-181` (used from `converge.rs:205-208`)
**Issue:** `BackupNode::sync_exhausted` queries the backup's ERROR logs from `now - 600_000 ms`,
where `now` is the CLI host's `SystemTime`. It is not anchored to the start of this apply. The
window is also compared against gateway timestamps, so host/gateway clock skew shifts it.

If the backup logged the "exceeds the max attempt count" line during the previous 10 minutes
(a failed earlier deploy, or a recovery by restart or manual resync that left the line in the
log), then `tick()` sets `exhausted = true` on the very first look. `wait_step` returns
`Exhausted` as soon as the backup has not yet converged, which is the normal state right after
the commit. The engine then:

1. sends the one resync,
2. skips the second budget (K6 a),
3. runs one immediate final evaluation, and
4. reports `applied_backup_unsynced` / `sync_exhausted`, exit 6.

That is a false "backup will not catch up" verdict, with a resync sent for nothing, on a pair
that would have synced within seconds. K6 a is meant to apply only when the gateway gave up
during this run.
**Fix:** Anchor the window to the run. Take `started_ms` once (at the backup sample, in
`drive`) and pass it into the poll as the log `start_time`. Better, ignore lines whose
timestamp predates the sample. This also removes the reliance on host/gateway clock agreement,
because the timestamp comes from the gateway's own status read.
```rust
// BackupNode::sync_exhausted(&self, since_ms: i64)
let query = LogQuery { start_time: Some(since_ms), min_level: Some("ERROR".into()), .. };
```
Pass the gateway-side epoch-ms of the sample (or the host `now` at sample time minus a small
skew margin) from `Phase`.

### WR-02: The "before" timestamp is sampled before `.gwbk`, staging and lock acquisition, so the advance signal can be satisfied by an unrelated sync

**Fix status: fixed in `a9a562f`.**

**File:** `crates/ignition-core/src/deploy/apply/mod.rs:1139-1150` (sample) versus `:1154`, `:1212-1233` (gwbk, `prepare`, lock)
**Issue:** `backup_before` is read right after the preflights. The first write (the commit) is
only reached after `take_gwbk` (a full `type=all` download), `host.prepare` (staging, which the
README says can take about 9 minutes at 1,520 files) and the lock acquire. Any sync that
advances the backup's `lastSyncTimestamp` in that window (a Designer save on the master, a
tag-provider change, any other config sync) satisfies `BackupSignals::advanced()` before this
apply's commit exists.

On a REST-only backup, `content()` can only check mode and the REST read-back. File-level
differences are a `warn`, which does not block `converged`. That combination can report
`converged: true` for a backup that has not received this deploy's file changes. With a
transport the file-hash check still catches it, so the weakness is mainly the REST-only case.
The contract text says "before the first write", so the engine is within the letter of K1/V3,
but the baseline is far earlier than it needs to be.
**Fix:** Re-sample the backup immediately before the commit, after `LockAcquired` is emitted
and before `host.commit` (still before the first write, and still delivering `lock_acquired`
to the observer after the preflights, which K3 requires). Keep the early sample only as the
fail-fast read. Document in the README that a REST-only backup's convergence rests on the
timestamp plus mode and read-back only.

### WR-03: `deploy verify --node both` silently drops the backup read-back warnings

**Fix status: fixed in `3fa5715`.**

**File:** `crates/ignition-core/src/actions/deploy.rs:1068-1094`
**Issue:** `backup_warnings` is filled by `verify_backup_tick` and only used in the
`None => DeployVerifyResult { warnings: backup_warnings, .. }` arm (`--node backup`). In the
`Some(result)` arm (`--node both`) the vector is discarded. The two modes therefore report
different warnings for the same backup. The apply path drops them too, but there the code
comment says the master verify owns them; in `both` mode the master verify has not read the
backup.
**Fix:**
```rust
let mut result = match master_result {
    Some(mut r) => { r.warnings.extend(backup_warnings); r }
    None => DeployVerifyResult { /* ... */ warnings: backup_warnings, /* ... */ },
};
```

### WR-04: The `secrets` preflight refuses a name the same render creates when its provider already exists

**Fix status: fixed in `2c72a14`.**

**File:** `crates/ignition-core/src/deploy/pair.rs:963-983`, called from `preflight.rs` `secrets_finding` (Preflight mode)
**Issue:** K8 skips a render-declared provider only when the node answers 404 (`NoProvider` or
`None`). If the provider already exists on the node (200, `Listed`) and this render adds a new
secret entry to the provider's own `config.json` (a file or internal provider) and also
references that name elsewhere, `Listed(names)` does not contain the new name. It lands in
`missing`, the check fails, and the apply is refused with `deploy_preflight_failed`
(`secrets`). The README states there is no flag that skips a preflight, so the only workaround
is a two-step deploy.

The check treats "provider declared by this render" as an excuse only when the provider is
absent. The provider's own config is part of the write set and can introduce the name, so
`Listed` on a declared provider is not proof that the name is missing after the apply.
**Fix:** In `Preflight` mode, when the provider is in `declared` and the name is not listed,
report it as a note ("will be created or changed by this render") rather than `missing`, or
refine it to check the rendered provider config for the name. Keep `Verify` strict, since
after the apply the name must exist.

### WR-05: `nodes.backup.profile` equal to `nodes.master.profile` is not rejected, so `verify --node backup` can report a green backup read from the master

**Fix status: fixed in `964183b`.**

**File:** `crates/ignition-core/src/deploy/target.rs:169-215` (validation), consumed at `crates/ignition-cli/src/main.rs` verify branch
**Issue:** Target validation checks each node in isolation. Nothing rejects two nodes naming
the same profile (or the same `local.data_dir`). `deploy apply` is protected by
`master_active`: two Active nodes read as split-brain and the preflight fails. `deploy verify
--node backup|both` has no such guard. With a duplicated profile it checks the master as if it
were the backup and can report `converged: true`, so a copy-paste error in the target file
yields a false assurance.
**Fix:** In `Target::validate`, push a problem when `backup.profile == master.profile`, and
when both declare `local`/`ssh` with the same data dir or host.

### WR-06: `cleanup24` runs `rm -rf` on a path built from an unchecked variable

**Fix status: fixed in `0fd67e1`.**

**File:** `crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh:732, 739`
**Issue:** `sname` comes from `awk '$1=="sync"{print $2}' "$NAMES"`. If `$NAMES` exists but has
no `sync` line (truncated, hand-edited, or from a different run), `sname` is empty and line
739 becomes `rm -rf "$BIND_A"/production/ignition/database-connection/`. That deletes every
database-connection unit in gw-a's bind-mounted production collection, and the sync then
removes them on gw-b too. The script is test-only and the rig is disposable, but the guard
costs one line.
**Fix:**
```bash
[ -n "$sname" ] && [ -n "$rname" ] || { echo "cp1-names is missing sync or rest" >&2; exit 1; }
case "$sname" in CAP24_SYNC_*) ;; *) echo "unexpected unit name" >&2; exit 1 ;; esac
```

### WR-07: `setup-pair.sh secrets` interpolates the secret name into an `sh -c` string inside the container

**Fix status: fixed in `2953a93`.**

**File:** `crates/ignition-core/tests/fixtures/live-rig-pair/setup-pair.sh:473, 477`
**Issue:** The name is only checked against `*/*`, `*..*` and the empty string. Line 477 then
runs `sh -c "cat > '/tmp/$name' && chown ... '/tmp/$name' && chmod ..."` as root. A name
containing a single quote, `;`, `$(...)` or a backtick breaks out of the quoting and executes
in the container (a name like `x'; id; echo '` works). The input is operator-supplied on a
disposable loopback rig, so exposure is low, but the guard is clearly intended as a
"plain file name" check and does not enforce it.
**Fix:** Allow-list the name and keep the interpolation out of the shell string:
```bash
case "$name" in ''|*[!A-Za-z0-9._-]*) echo "REFUSING: secret name must match [A-Za-z0-9._-]+" >&2; exit 2 ;; esac
# or pass it as a positional parameter:
docker exec -i -u root "$c" sh -c 'f=/tmp/$1; cat > "$f" && chown 2003:2003 "$f" && chmod 600 "$f"' _ "$name"
```

## Fix status

Warnings WR-01 to WR-07 fixed on 2026-10-21 (Info items out of scope). One commit per finding, `fix(24-review): WR-0N ...`; test first where the code is testable.

| Finding | Status | Commit | What changed |
|---|---|---|---|
| WR-01 | fixed | `bdc0665` | `BackupNode::sync_exhausted(since_ms)` counts only log entries stamped at or after the anchor the engine took (query `startTime` and each entry's own gateway timestamp); the fixed 10-minute window is gone. Clock assumption: the anchor is the CLI host's clock and the entries the gateway's, so they must agree to within the age of a stale line; a gateway clock behind the host can hide a fresh line, which only costs the early exit. |
| WR-02 | fixed | `a9a562f` (follow-ups `55f4b7c`, `51f3c95`) | The backup is sampled again after `LockAcquired` is delivered and before `host.commit`; that second sample is the baseline and the anchor for WR-01. The early sample stays as the fail-fast read. A re-sample that cannot be read keeps the early sample. A cut made at `lock_acquired` (K3 b) is seen by the re-sample and still ends `not_syncing`. README notes the REST-only convergence basis. `55f4b7c` updates the CLI pair mock's read count, `51f3c95` bundles the baseline into a struct for clippy. |
| WR-03 | fixed | `3fa5715` | `deploy verify --node both` extends the master result's warnings with the backup read-back warnings. |
| WR-04 | fixed (K8 extension, user-approved) | `2c72a14` | The `secrets` preflight no longer requires a name the tree itself defines in a declared provider's `config.json` (`settings.files` or `settings.secrets` keys). Verify stays strict. Recorded in `24-02-SUMMARY.md` (K8) and `24-10-SUMMARY.md` (post-plan review fixes); README preflight row updated. |
| WR-05 | fixed (profile and location; not URL) | `964183b` | `Target::parse` refuses `nodes.backup.profile` equal to `nodes.master.profile`, the same `local.data_dir`, and the same `ssh` host, port and `data_dir`, before any contact (`deploy_tree_invalid`, exit 2). Two different profiles that resolve to one URL are not detected: the URL is known only after profile resolution at three call sites, so it was not cheap. |
| WR-06 | fixed | `0fd67e1` | `cleanup24` refuses an empty name or one containing `/` or `..`, and `rm -rf` uses `${BIND_A:?}` and `${sname:?}`. |
| WR-07 | fixed | `2953a93` | `setup-pair.sh secrets` and `secrets-rm` allow-list PROVIDER and NAME (`[A-Za-z0-9._-]+`, no `..`) before any request or docker call; `secrets` passes the name to the container shell as a positional parameter instead of interpolating it. |

Verification: `cargo clippy --workspace --all-targets -- -D warnings` clean; targeted tests (`deploy::` lib, `deploy_apply`, `deploy_verify`, `deploy_pair`, `deploy_transport`, `deploy_plan`, `deploy_client_contract`, `contract_deploy`, `deploy_family_coverage`) green; `bash -n` on both rig scripts; live gates on the pair rig all PASS (6 passed, 206 s).

## Info

### IN-01: API tokens and the admin password travel as curl argv in the rig scripts

**File:** `setup-pair.sh:145, 147`; `capture24.sh:164, 188, 239, 246, 384, 394, 422, 846, 859, 906, 990, 1012, 1014`
**Issue:** `-H "X-Ignition-API-Token: $tok"` and `-u admin:ign-pair-rig` put the credential in
the process argument list, visible to other local users via `ps` for the duration of each
call. The rig is loopback-only and disposable, and no token is echoed or written to a
committed file (verified: `refuse_if_token`, `tokenHash` redaction, `shcap` argument checks).
So this is hygiene only.
**Fix:** Use `curl -H @file` or `--config -` fed from stdin (`printf 'header = "X-Ignition-API-Token: %s"\n' "$tok" | curl -K - ...`).

### IN-02: `pair_module_never_writes` is a deny-list text scan

**File:** `crates/ignition-core/tests/deploy_transport.rs:2901-2928`
**Issue:** The test fails only if `pair.rs` contains one of 12 named identifiers. `BackupNode`
holds `&dyn GatewayApi` and `&dyn DeployApi` with their full write surfaces. A future read
method that calls a write not in the list (any `GatewayApi` mutation, or a new `DeployApi`
write) passes the scan. The runtime half (`ForbidWrites` in `deploy_pair.rs`) covers the
`DeployApi` writes it knows about.
**Fix:** Invert to an allow-list. Assert that every `self.gateway.` and `self.deploy.` call in
`pair.rs` is one of `redundancy_status`, `logs`, `deployment_mode`, `secret_names` and the
`gather_read_back` reads. A narrow read-only trait object passed to `BackupNode::new` would be
stronger still.

### IN-03: The sync-exhausted matcher assumes ERROR level, which was never observed

**File:** `crates/ignition-core/src/deploy/pair.rs:159-164, 176-181`
**Issue:** C-P8 never reproduced the line (24-02 summary, K5 d). `min_level: "ERROR"` is an
assumption. If the gateway logs "exceeds the max attempt count" at WARN, `sync_exhausted`
never fires. The text is also matched on a free-form message. The code and README present the
feature as working, while it is only unit-tested against the spike's recorded line.
**Fix:** Drop `min_level` (match on text only, bounded by the window from WR-01), or state in
the README that `sync_exhausted` is best-effort and unverified live.

### IN-04: Secret listing maps every 404 to "no provider" and every 500 to "cannot list"

**File:** `crates/ignition-core/src/client/deploy_api.rs:943-966`; `validate_segment` use at `:944`
**Issue:** K7/K8 are approved, so the mapping itself is not disputed. Two follow-ons:
- A gateway where the route itself does not exist (the contract rests on the 8.3.6 OpenAPI;
  the project supports 8.3.1+) answers 404 for every provider. The preflight then reports
  every non-render-declared referenced secret as `missing`, which is a false refusal rather
  than "unsupported".
- `validate_segment` rejects provider names outside `[A-Za-z0-9._-]` with `invalid_input`,
  aborting the whole preflight (and `verify --node`) for a provider named with a space.
**Fix:** Optional. Probe a known-good listing once and treat a route-level 404 as
`CannotList`, and percent-encode the provider segment instead of refusing it.

### IN-05: Human `deploy apply` output uses "backup" for two different things

**File:** `crates/ignition-cli/src/render.rs:1590-1603, 1619-1623`
**Issue:** `backup <file>.gwbk` (the downloaded gateway backup) and `backup: converged` /
`backup: NOT converged (...)` (the redundant node) appear in the same output block, and
`backup waited ...` follows. An operator skimming a failed apply can misread which backup is
meant. The JSON is unambiguous (`gwbk` versus `backup`).
**Fix:** Label the node block `backup node:` and the file `gwbk`.

### IN-06: Small safety gaps in the live helpers

**File:** `crates/ignition-core/tests/live_deploy_pair.rs:476-499` and `:447-454`; `setup-pair.sh:284, 300-302`
**Issue:**
- `IGN_PAIR_MODE` is unvalidated and joined into `data_a/config/resources/<mode>/ignition/<unit>`
  for `remove_dir_all`. A value containing `..` walks out of the collection. Only fixed unit
  names are removed and the gateway-name guard runs first, so the damage is bounded.
- `setup-pair.sh cmd_up` guards `rm -rf` with `"$STATE"/data-?`, which passes when
  `IGN_PAIR_RIG_STATE=/` (it then removes `/data-a`, `/data-b`).
- `cmd_mint` truncates `tokens.env` before minting, so a failure after node a leaves a partial
  file that later steps source.
- `tmp` in `cmd_mint` is not declared `local`.

**Fix:** Reject `mode` values containing `/` or `..`. Refuse `STATE` equal to `/` or `$HOME`.
Write `tokens.env` to a temp file and `mv` it on success.

---

_Reviewed: 2026-10-21_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
