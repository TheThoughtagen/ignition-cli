# Feature Research — v1.6 Declared Gateway Deploy (`ign deploy`)

**Domain:** declared-config deploy tooling for a redundant Ignition 8.3.6 pair (CLI + agent/CI consumer)
**Researched:** 2026-10-10
**Scope:** only the NEW surface: `ign deploy {render,plan,apply,rollback,verify,projects}` + the config-API client and SSH transport they need. v1.0/v1.1 capabilities are dependencies, not re-researched.
**Confidence:** HIGH on spec-derived contract and on reuse points (read from source); HIGH on Terraform (official docs fetched); MEDIUM on kubectl/Helm/Ansible (long-stable behavior, partly re-verified); MEDIUM on Stoker (docs + source read; several of its "safety" features are roadmap, not shipped); LOW on community Ignition CI/CD (almost nothing exists beyond "copy files + POST scan").

**Treatment of the spec:** `2026-10-09-prod-gateway-config-as-code-design.md` (D1-D13, §3 facts, §6-§8, §15) is binding. Where research finds a gap, collision or contradiction it is listed in **Spec Flags (SF-1..SF-10)** below with a recommendation. Nothing here silently diverges.

---

## 1. What the reference tools teach us

| Tool | Behavior worth copying | Behavior to reject for `ign deploy` | Confidence |
|---|---|---|---|
| **Terraform / OpenTofu** | `plan` is read-only and side-effect free; `-detailed-exitcode` (0 = no changes, 1 = error, 2 = changes); `plan -out` + `apply planfile` (apply the exact reviewed plan; stale plan refused); `-json` machine output; destroy/replace shown as distinct actions; plan files hold sensitive values in cleartext. | A persistent state file / remote state (the gateway IS the state; D1 says enforce, don't reconcile). Opaque plan-file format (secrets-in-cleartext risk, format versioning). `-detailed-exitcode` value 2 collides with ign's exit 2 (see SF-2). Terraform has no rollback (revert commit + re-apply) — we need one (D7). | HIGH (docs fetched) |
| **kubectl** | `diff` against the LIVE object (server-side dry-run), no state file; `diff` exit 0 = none, 1 = differences, >1 = error; `apply` computes + applies; `--prune` shows deletion is the dangerous half of apply. | Legacy `--prune` (label/selector over-selection, object leakage; ApplySet replacement still alpha). Delete by selector. exit 1 collides with ign's exit 1 = internal. | MEDIUM-HIGH |
| **Ansible** `--check --diff` | `--diff` shows per-file content delta; per-task `changed` vs `ok` idempotence reporting. | Check mode is only as truthful as each module's check support — a plan that is a SEPARATE re-implementation of apply lies. => ONE diff engine shared by plan, apply gate, verify. | MEDIUM |
| **Helm** | `history` + `rollback REVISION` (rollback is itself a new revision, with its own record); `--atomic` (Helm 3) / `--rollback-on-failure` (Helm 4, MEDIUM) = auto-rollback when post-install checks fail; rollback does not undo data/PVC/hook side effects (documented limitation — we must document "tag values / secrets are not rolled back" the same way). | Cluster-side revision storage (we keep run artifacts off-gateway; nothing is written into the collection). | MEDIUM |
| **Stoker operator** (knorrlabs, Go, k8s-only) | Closest domain match. Flow: read -> clone -> **build plan & stage** -> merge -> **clean orphans within managed paths only** -> scan (`/scan/projects` then `/scan/config`) -> report. `dryRun` (diff only, no write). `designerSessionPolicy: proceed|wait|fail`. **Post-commission re-sync** because commissioning overwrites `security-properties`. Per-gateway status Pending/Synced/Error. | Content templating + JSON-patch engine (non-deterministic render, secret-leak surface; our overlay is whole-file by path). Continuous reconcile loop (no daemon). **It has NO scan-lock** (scan is fire-and-forget), **no snapshot/rollback, no post-sync verification, no drift detection** — all are on its roadmap ("pre-sync backup with automatic rollback on scan failure", "post-sync health verification", "drift detection", "approval annotation gate" = future). So `ign deploy` is ahead of Stoker on exactly the safety features. | MEDIUM (docs + roadmap read; shipped-vs-planned distinguished) |
| **IA guidance (8.3 version-control guide, deployment-modes docs)** | Config is files in `data/config`; gateway does NOT watch disk, so call `/data/api/v1/scan/config` and `/scan/projects`; deployment modes override whole resources; on scan the gateway reconciles in-memory state to disk (removes what is gone) — which is why file deletion + scan is a real delete. | IA prescribes `git pull` into the data dir; guide documents no rollback and no secrets story. | HIGH (docs) |
| **Community Ignition CI/CD** (forum threads, DMC git-hook post) | "Copy files + POST scan" with an API token. That is the whole state of the art. | No lock, no plan, no snapshot, no verify, no backup-node check. Nothing to copy; this is the differentiator space. | LOW (sparse) |

**Net:** table stakes come from Terraform/kubectl/Helm UX (plan-before-apply, structured diff, idempotent re-run, atomic-ish failure handling, rollback record). Ignition-specific differentiators come from the spec's verified hazards (scan-lock expiry, core shadowing, mode-not-on-backup, silent resource-not-loaded, redundancy sync exhaustion) that none of the tools above model.

---

## 2. Cross-cutting conventions (apply to every verb)

These are decided by existing `ign` contracts; they are constraints, not options.

1. **Frozen envelope.** Success = `{ok:true, profile, data}`; failure = `{ok:false, profile, error:{code,message,endpoint,hint}}` on stderr. The failure envelope has **no `data` field** (read from `error.rs`). See SF-1.
2. **Exit taxonomy 0-7 is frozen.** 1 internal, 2 usage/`confirmation_required`, 3 config, 4 network, 5 auth, 6 target-state, 7 rig. New slugs obey the **Three-Place rule** (enum in `error.rs`, README table, enumerated unit test). All deploy failures that are "the gateway/world is not in the state required" are **exit 6**; SSH/transport unreachable is **exit 4**.
3. **Reporting verbs exit 0; gating verbs exit non-zero.** Precedent: `project diff` and `doctor` exit 0 with findings in `data` ("failing checks are the product"). `render` and `plan` follow that. `apply`, `rollback`, `verify`, `projects` are CI gates, so a failed outcome MUST be non-zero (this shop has been burned by green-washing: WebDev-200-on-reject, StatusPing-only smoke tests). A deploy tool that reports a failed verify with exit 0 is a defect.
4. **One guarded-write gate** (`require_confirmation(yes, &preview)`; refusal message IS the deterministic preview; zero mutations without `--yes`; no separate `--dry-run` flag — `plan` is the dry run, and the refusal text of any mutating verb is its own dry run). Staleness/conflict refusals are NOT `--yes`-able (v1.1 precedent: `edit`, `workspace push`).
5. **Run directory is the structured channel.** Every mutating verb writes `<run-dir>/` (default `./.ign/deploy/<env>/<run-id>/`, configurable; CI uploads it as the 30-day artifact, spec §8): `run.json` (ids, env, hosts, render sha, plan sha, phase timings, outcome), `result.json` (the envelope that was/would have been printed — including for failures), `snapshot/`, `plan.json`, `verify.json`. This is how structured findings reach agents when the failure envelope cannot carry `data` (SF-1).
6. **Nothing is written into the gateway collection except resources.** No state file, marker or dotfile inside `data/config/resources/<mode>/` — Ignition 8.3.3's resource tree throws `NameInvalidException` on dot-named entries (known WHK failure, same class as the `.omc` collision). Run state lives off-gateway.
7. **New leaves need TUI + MCP bookkeeping.** `tests/tui_coverage.rs` fails CI when a clap leaf has no TUI route row, and the OutOfBand set is documented as closed. Six new leaves = six route rows or a deliberate, documented exception (SF-9). MCP catalog derives from clap automatically; guarded verbs need `// guarded:deploy apply`-style markers + confirm-gate parity.
8. **Additive JSON, flat, all keys always** (precedent `ProjectDiffResult`): `scope`, explicit counts in `summary`, path-sorted `entries[]`, empty vecs not omitted. Reuse the diff vocabulary already in the agent contract: `added | changed | removed | same`.

---

## 3. Per-subcommand feature landscape

Complexity: LOW (<= 1 plan), MEDIUM (2-3 plans), HIGH (4+ plans or needs a capture-first spike).

### 3.1 `ign deploy render`

**Expected behavior:** pure, offline, deterministic. Effective tree for env E = `resources/` + `tags/` with `modes/E/` overlaid by path (whole-file replacement), restricted to the manifest allowlist; emit a file manifest with sha256 so plan/apply/verify share one render identity.

| Aspect | Contract |
|---|---|
| Inputs | `--gateway NAME` (or path to `gateways/<name>/`), `--env E`, `--out DIR` (default run dir), `--check` (render to memory, no files) |
| Outputs | rendered tree on disk (exact `data/config/resources/<mode>/` layout) + `render.json` |
| JSON `data` | `{scope:"gateway-config", gateway, env, mode, render_sha256, file_count, by_type:[{module,type,count}], files:[{path,sha256}] (path-sorted; `--summary-only` drops it), lint:{errors:[],warnings:[]}}` |
| Network | none. No profile required (so config errors are exit 3, never 4/5). |
| Gate | none (writes only to `--out`; refuses to write into a non-empty dir that is not a prior render unless `--force` — clobber class) |
| Exit | 0 with lint warnings; lint ERRORS (embedded secret, type outside allowlist, invalid JSON, duplicate path after overlay) -> exit 2 `invalid_input` pre-network, like other pre-resolution refusals |

**Table stakes**
| Feature | Why expected | Complexity | Notes |
|---|---|---|---|
| Deterministic, idempotent output (render twice -> identical `render_sha256`) | Everything downstream compares hashes | MEDIUM | Sorted traversal; fixed newline policy; no timestamps |
| Overlay by path (`modes/E/` wins) | Spec §4 | LOW | Whole-file replace; no field-level merge |
| Manifest allowlist enforcement ("anything not listed stays owned by the gateway") | Spec §4; prevents deploying/deleting unmanaged types | LOW | `manifest.yaml` is a NEW parse (no YAML dep in core today — STACK concern) |
| Embedded-secret lint (fail on any embedded value; references only) | Spec §9, success criterion 5 | MEDIUM | Must detect Ignition's embedded-secret object shape + plaintext-looking keys; confirm the exact 8.3.6 JSON shape in a capture (MEDIUM confidence on `type: Embedded` vs `Referenced`) |
| `attributes.uuid` on every NEW resource's `resource.json` | Spec §3: resources without it "will not be loaded" (silently — RIG.md) | MEDIUM | See differentiator below: uuid must be STABLE across renders |
| JSON-validity lint of every rendered file | Spec §6.1 | LOW | |
| Render-time failure on duplicate/ambiguous overlay | Prevents silent clobber | LOW | |

**Differentiators**
| Feature | Value | Complexity | Notes |
|---|---|---|---|
| **Deterministic UUID derivation** (UUIDv5 over `<gateway>/<module>/<type>/<name>`) for resources with no uuid; bootstrapped resources keep the gateway's own uuid | A random uuid per render makes every plan dirty and churns identity; v5 needs no state file | LOW | Decide in phase planning; the alternative (persist uuids into the repo) is worse for review noise |
| **Byte-copy by default; Gson-compatible writer only for files `ign` synthesizes** | v1.1's hard lesson: CLI re-serialization of gateway formats is a fidelity rabbit hole. Files bootstrapped from the gateway are already canonical; copying them preserves byte identity (spec: "gateway-produced files round-trip byte-identical") | MEDIUM | REFINEMENT of spec §4, not a disagreement: output is still deterministic. Add `render --check-canonical` to flag committed files that are not in canonical form (a "fmt" gate) rather than silently rewriting them |
| `render.json` = the shared identity (render sha) consumed by plan/apply/verify and recorded in `run.json` | One hash ties PR -> plan -> apply -> verify together | LOW | |
| Golden-fixture oracle from bootstrapped prod files (render == prod bytes) | The spec's §13.2 gate ("render must equal prod") is only provable if the writer is tested against real gateway output | MEDIUM | Test-suite feature, not a flag |

**Anti-features**
| Anti-feature | Why requested | Why bad | Instead |
|---|---|---|---|
| Templating / JSON-patch / variable substitution (Stoker-style) | Per-env differences without duplicate files | Non-deterministic render, secret-injection path, un-reviewable output | Whole-file `modes/E/` overlay (spec decision) |
| Pulling secret VALUES into the render | "Make the deploy self-contained" | Violates D8; success criterion 5 | References by name only; Doppler -> `manage-secrets.sh` upstream |
| Tombstones/"absent in this env" in overlays | Needed eventually (resource exists in base, not in test) | Not in spec; adds a second delete channel | Defer; note limitation (SF-7) |

### 3.2 `ign deploy plan`

**Expected behavior:** read-only diff of render vs LIVE (kubectl-diff style: against the gateway, no state file). Same diff engine that `apply` uses for its gate and `verify` uses for its read-back.

| Aspect | Contract |
|---|---|
| Inputs | `--gateway`, `--env E` (resolves target: master URL + SSH), `--render DIR` (reuse a prior render) , `--diff` (include per-file unified content delta; off by default — large + may contain sensitive config), `--only TYPE[,TYPE]`, `--out FILE` (write plan JSON) |
| Live-state sources | (a) byte/normalized hash of the target mode collection over SSH (the only source that covers `tag-definition`, which has no per-resource REST route and cannot be listed by collection via `tags/export`); (b) REST `find`/`list` with `collection=core` for shadow detection; (c) `GET /mode` + `gateway-info.deploymentMode` for "is the mode actually active" |
| JSON `data` | `{scope:"gateway-config", env, mode, render_sha256, plan_sha256, summary:{added,changed,removed,same, has_changes, destructive, lockout_risk, core_shadow_blocking, unmanaged}, entries:[{path, module, type, name, status, destructive, destructive_reason, core:"none\|identical\|differs\|hidden\|shadows_removal", live_sha256, desired_sha256}], preflight:[{name,status,detail,hint}], warnings:[]}` |
| Gate | none — read-only. Exit 0 always on completion; "changes present" is `data.summary.has_changes` (SF-2) |
| Complexity | HIGH — SSH tree hashing at scale + normalization + core layer + classification |

**Table stakes**
| Feature | Why expected | Complexity | Notes |
|---|---|---|---|
| Four-status per-resource diff `added/changed/removed/same` with path-sorted entries and summary counts | Terraform/kubectl baseline; matches `project diff` agent shape | MEDIUM | Reuse `diff_members` + `normalize_descriptor` semantics for `resource.json` (key-order-independent, volatile attributes ignored) |
| Zero writes, zero locks (does not take scan-lock, does not touch the collection) | Plan must be safe to run by any agent any time | LOW | |
| Deletion scoped to the manifest allowlist only (never "everything not in render") | kubectl `--prune` / Stoker lesson: over-selection is the incident | LOW | Stoker's "orphans within managed paths only" is the validated precedent |
| Destructive flagging (see §4.1) | Spec §8/§14 `destructive` label | MEDIUM | |
| Core-shadow flagging (see §4.1) | Spec §3 S1/S2/S2b, §13.5 | MEDIUM | |
| Structured preflight rows (active node, pair healthy, backup in sync, designers open, restart-tasks pending, deployment mode active) on the doctor `{name,status,detail,hint}` shape | Spec §6.4.1 pre-checks must be visible BEFORE apply | MEDIUM | Reuse `CheckResult` shape from doctor |
| Stable `plan_sha256` (hash of the canonical, sorted entries) | Lets CI bind approval to the exact plan (see apply) | LOW | Hash of entries, not of the file |

**Differentiators**
| Feature | Value | Complexity | Notes |
|---|---|---|---|
| **Three-way plan against a base render** (`--base DIR`: the previous release's render): `live != base` -> `drift:true`; `desired != base` -> intended change | Without a state file, a plan cannot tell "your PR changes this" from "someone Designer-edited prod". This is how v1.6 gets §6.4.1 "no open drift" / §14 "plan refuses while drift is open" WITHOUT `ign drift` (deferred). Precedent: `ign workspace status` manifest three-way compare | MEDIUM | Recommended P2 (SF-4) |
| `--detailed-exitcode`-style opt-in | CI ergonomics | LOW | Defer; needs a contract decision (SF-2) |
| Restart-class hints (`may_require_restart` per type table) | Spec §7 restarts are surprising | HIGH/LOW-confidence | Gateway only reveals `restart-tasks/pending` AFTER a scan. Defer; apply reports actual pending tasks |
| Plan scoped by type/provider (`--only`) | Faster iteration on 1.3k tag files | LOW | |

**Anti-features:** opaque binary plan files (use `plan_sha256` + a JSON `--out`); refusing to plan when drift exists (plan must always be able to SHOW drift; the refusal belongs to `apply`); network calls to the backup in `plan` (verify's job).

### 3.3 `ign deploy apply`

**Expected behavior (spec §6.4):** preflight -> snapshot -> gate -> scan-lock -> mirror over SSH -> scan -> poll -> verify (master) -> auto-rollback on verify failure -> backup verification. One state machine, every phase timed and recorded.

| Aspect | Contract |
|---|---|
| Inputs | `--env`, `--gateway`, `--yes`, `--allow-destructive`, `--plan-sha256 H` (refuse if fresh plan differs), `--hold-timeout S` (default derived: 2x(measured write+scan)+1), `--designers fail\|wait\|proceed` (default fail; Stoker's `designerSessionPolicy` vocabulary), `--run ID` (join an existing run dir), `--skip-gwbk` (recorded in run.json) |
| Outputs | run dir (§2.5); data: `{run_id, run_dir, phases:[{name,status,elapsed_ms}], snapshot:{collection_sha256, files, gwbk:{path,bytes}\|null, projects:[]}, lock:{hold_timeout_s, elapsed_s, expired:false}, scan:{status}, verify:{...}, restart_pending:[...], backup:{in_sync, checks:[]}}` |
| Gate preview (refusal message IS this) | target env + both nodes; render sha + plan sha; counts by status and by module/type; **every** destructive entry listed by path (never truncated); every blocking core-shadow entry; lockout-risk entries; snapshot location + size estimate; hold timeout; "restart tasks pending: N" if known; "N files unchanged". Large plans: bounded body (top N per type + `--plan-out` path) but destructive/blocking lists are always complete |
| Exit | 0 success; 2 `confirmation_required` (no `--yes`); 6 for every post-gate failure with distinct slugs (`deploy_preflight_failed`, `scan_lock_held`, `deploy_lock_expired`, `deploy_plan_stale`, `deploy_verify_failed_rolled_back`, `deploy_rollback_failed`, `deploy_backup_unsynced`); 4 SSH/network; 5 auth |
| Complexity | HIGH — the state machine, Windows SSH transport, and failure-path discipline |

**Table stakes**
| Feature | Why expected | Complexity | Notes |
|---|---|---|---|
| Re-plan inside apply + gate on the fresh plan (never apply an unseen diff) | kubectl apply / terraform apply w/o file | MEDIUM | Same engine as `plan` |
| Idempotent: empty plan -> no lock, no scan, exit 0 `no changes` | Terraform/Ansible baseline; avoids needless lock + scan on prod | LOW | |
| **Snapshot before write** (§4.2): mode-collection tree + exports of projects to be imported + (default) `.gwbk` | Spec §6.4.2, D7 | MEDIUM-HIGH | `.gwbk` is the heavy piece; existing `backup download` reusable |
| **Scan-lock discipline**: acquire (`409` = another holder -> refuse, never steal); poll `GET /scan-lock/config` during write; **JSON while held / 204 after expiry (S4)** -> abort before scanning if expired or elapsed >= holdTimeout | Spec §6.4.3; an expired lock + scan applies a PARTIAL tree | MEDIUM | `holdTimeout` must be measured on the real SSH transport (spike numbers are local `docker cp`) |
| Mirror with deletes (declared deletions actually delete) restricted to the mode collection root | Spec §5.1, D5 | HIGH | See §4.3 Windows mirror note |
| Scan + poll to completion; failure/timeout = non-zero | Spec §6.4.3 | LOW | Reuse `poll.rs` (adaptive backoff, retry Network/GatewayRestarting, never retry Auth) |
| Post-apply verify of the MASTER (read-back + hash + loaded-ness + tag-quality probe) | Spec §6.4.4 | see verify | |
| **Auto-rollback on master verify failure** (apply(source=snapshot) under the same lock/scan/verify path); distinct slugs for "rolled back clean" vs "rollback itself failed" | D7, spec §8 | MEDIUM | Alerting must discriminate; both exit 6 |
| **Failure BEFORE scan = write the snapshot back, THEN scan** | Lock expiry without scan applies nothing, but a half-written disk would be applied by the next scan from ANY actor; restoring first leaves disk == last good | MEDIUM | Needs a capture-first check of whether the lock can be released without a scan (DELETE route?) — not in spec §3 |
| Backup-node verification as the final phase (§3.5); backup failure -> exit 6 `deploy_backup_unsynced`, NO auto-rollback (spec: auto-rollback only on master verify) | Spec §6.5 | MEDIUM | |
| Pre-checks: master is ACTIVE; both healthy; backup in sync; no Designer sessions (`GET /designers`); `restart-tasks/pending` empty | Spec §6.4.1 | MEDIUM | Designer policy flag borrowed from Stoker |
| Report (do NOT perform) pending restart tasks after scan | Milestone excludes `redundancy restart --rolling`; spec §6.4.6 puts it in apply (SF-3) | LOW | `data.restart_pending[]`; success exit 0 but loud |
| Redaction: no secret values in any output/run artifact | Contract (scriptExec redaction precedent) | LOW | `--diff` output goes through the same redactor |

**Differentiators**
| Feature | Value | Complexity | Notes |
|---|---|---|---|
| **`--plan-sha256` approval binding** (apply refuses, not `--yes`-able, if the fresh plan's hash != the one the reviewer approved) | Fixes "approved X, applied Y" across the test -> approval -> prod pipeline without Terraform's opaque plan file | LOW | Cheap given `plan_sha256`; strongly recommended |
| **Stage-then-mirror**: upload rendered tree to a staging dir OUTSIDE the lock; inside the lock run a LOCAL mirror on the node | Shrinks the lock window from "network transfer time" to "local copy time" (spike: 1-2 s); makes holdTimeout predictable | MEDIUM | Within spec (§6.4.3 says mirror over SSH). On Windows the native mirror is `robocopy /MIR` — exit codes are a BITMASK (0-7 success, >= 8 failure), a classic pitfall |
| **Re-hash live under the lock and compare to the snapshot** (abort if it moved between snapshot and lock) | Closes the snapshot-to-lock race (Designer saves, another operator) | LOW | |
| Deploy token self-lockout guard (`lockout_risk` when the plan changes `security-properties`/identity providers) | RIG.md: the token's level lives in `security-properties`; commissioning/ core overrides already broke API access once; Stoker's post-commission re-sync exists for the same reason | MEDIUM | Flag + require `--allow-destructive`-class ack |
| Snapshot integrity (sha256 manifest of snapshot; rollback refuses a tampered/incomplete snapshot) | Rollback must be trustworthy at 3 a.m. | LOW | |
| Phase timing telemetry in `run.json` | Lets S4's "measure on the real transport" become data | LOW | |

**Anti-features**
| Anti-feature | Why requested | Why bad | Instead |
|---|---|---|---|
| `--force` / `--skip-verify` / `--no-rollback` | Debugging, impatience | Bypasses the only safety net on a plant gateway; green-washing | Re-run `verify`; fix forward; the only recorded opt-out is `--skip-gwbk` |
| Auto-revert of drift (D1) | "Self-healing" | Overwrites emergency Designer edits silently; D1 rejected it | Plan shows drift; next apply enforces |
| Failover/restart inside apply | One-button deploy | `failover` returns 200 even when it cannot reach the backup (RIG.md); restarts are a separate, time-boxed procedure (§7) | Report `restart_pending`; `ign redundancy restart --rolling` is a later milestone |
| Writing to the backup node | Faster convergence | Spec §5.1: master only; redundancy carries it; writing both creates the sync-snapshot-mismatch failure (RIG.md) | Verify the backup, `resync` once |
| Interactive prompts | Human comfort | Violates non-interactive-by-default | `--yes` + refusal-as-preview |
| Cloud-runner deploys to prod | Convenience | Spec F7 retired this (no route to plant) | `onprem-linux` runner (spec §5.5) |

### 3.4 `ign deploy rollback --run <id>`

**Expected behavior:** restore the snapshot recorded in run `<id>` through the SAME engine as apply (mirror + lock + scan + verify), with `source = snapshot`. Helm precedent: rollback is itself a recorded run (new run dir, `rollback_of: <id>`).

| Aspect | Contract |
|---|---|
| Inputs | `--run ID` (or path), `--yes`, `--projects` / `--config` (default both), `--discard-live-changes` |
| Gate preview | which run is being reverted; files restored / files removed (the deploy's ADDED resources) / projects re-imported; **live-vs-post-apply fingerprint comparison** (has anyone changed prod since that run?); what is NOT restored (see below) |
| Refusals (not `--yes`-able) | live state != the run's recorded post-apply fingerprint -> `deploy_plan_stale`-class refusal naming both hashes, unless `--discard-live-changes`; snapshot missing/tampered |
| Complexity | MEDIUM given the shared apply engine |

**Table stakes**
| Feature | Why expected | Complexity | Notes |
|---|---|---|---|
| Restore collection snapshot under scan-lock, then scan, then verify against the SNAPSHOT | Spec §8 row 1 | MEDIUM | Includes removing resources the deploy added (mirror semantics) |
| Project rollback = re-import pre-deploy export zips, parents first | Spec §8 row 2; S5 (parent overwrite restarts children, no child re-import) | MEDIUM | |
| Rollback record (own run dir, links to origin) | Helm history semantics; audit | LOW | |
| Honest scope statement in output: what rollback does NOT restore | Helm's documented limitation; spec §8 (memory-tag values and runtime state lost on tag deletion) | LOW | Not restored: tag runtime values (`valueStore.idb`), secret-provider contents, installed modules, restart-class changes (those need rolling restart + previous artifact), `core` unless the run stripped it (SF-5) |
| Works from run artifacts alone (no dependence on git/CI state) | CI artifact is retained 30 days (§8) | LOW | |

**Differentiators:** rollback of a rollback is just another apply of a snapshot (free); `--list` of runs for an env (LOW, P3); disaster tier is `ign backup restore` of the `.gwbk` (already exists, node-by-node, restart) — surfaced in the hint, not reimplemented.

**Anti-features:** gateway-side revision storage; "rollback to an arbitrary git ref" (that is `render` + `plan` + `apply` of the old commit = the spec's "revert the PR" path — one mechanism, not two); partial per-resource rollback.

#### Minimum viable rollback — what is snapshotted

| Component | Needed for auto-rollback? | Source | Notes |
|---|---|---|---|
| **The target mode collection tree**, byte-exact with per-file sha256 | **YES — the core of rollback** | SSH read of `data/config/resources/<mode>/` on the master BEFORE the lock | The effective-config rollback unit. Restore = mirror. |
| **Exports of every project the run will import** (`GET /projects/export/{name}`) | YES for the `projects` verb | REST | Spec §6.4.2 / §8 row 2; existing `project_export` |
| **`run.json` with pre-state fingerprint + post-apply fingerprint** | YES | local | Enables the staleness refusal |
| `.gwbk` (`GET /backup`) | NO for auto-rollback; YES for the disaster tier | REST (existing `backup_download`) | Spec default-on; potentially large/slow on prod; the only sanctioned opt-out `--skip-gwbk`, recorded |
| `core` collection | Only when the run includes the §13.5 strip | SSH | SF-5 |
| Tag values, secrets, modules | NO — cannot be restored by file snapshot | — | Documented, never promised |

### 3.5 `ign deploy verify`

**Expected behavior:** read-only proof that a node serves what was declared. Standalone (`--env`) or as apply's internal phase. Exit non-zero on any mismatch (§2.3). Must NOT be satisfiable by file hashes alone — see the "will not be loaded" trap.

| Aspect | Contract |
|---|---|
| Inputs | `--env`, `--render DIR`, `--node master\|backup\|both` (default both), `--timeout S` (backup convergence wait), `--run ID`, `--tag-probe PATH` (repeatable; default from manifest `verify.tag_probes`), `--resync` (the ONE mutating option; gated, default off in standalone verify) |
| JSON `data` | `{scope, env, nodes:[{role, url, ok, checks:[{name,status,detail,hint}]}], mismatches:[{path,kind,expected_sha256,actual_sha256}], findings_count}` — doctor-shaped rows |
| Gate | none unless `--resync` |
| Exit | 0 all checks pass; 6 `deploy_verify_failed` (message = counts + first N mismatches + run-dir path); findings in `result.json` (SF-1) |
| Complexity | MEDIUM-HIGH |

**Table stakes (master)**
| Check | Why | Complexity | Notes |
|---|---|---|---|
| Scan complete (`GET /scan/config` status) | Spec §6.4.3 | LOW | |
| `gateway-info.deploymentMode` == manifest mode | Mode silently ignored if absent at boot (§3) | LOW | `""` = none active |
| Every declared resource reads back equal to render via REST (`allowInvalidReferences=false`) | Spec §6.4.4; proves it is LOADED and valid, not just present on disk | MEDIUM | |
| **Gateway-log scan since apply start for load failures** (e.g. `does not have required UUID attribute, will not be loaded`, invalid-reference warnings) | RIG.md: a file can match the render hash and the resource still be unloaded. Hash-only verification green-washes. | MEDIUM | Reuse `logs`; match the verbatim message set from RIG.md |
| File-hash comparison render vs collection is empty (normalized) | Spec §6.4.4 | MEDIUM | Same diff engine as plan; a verify == "a plan that must be empty" |
| Sample OPC + MQTT tags read Good quality | Spec §6.4.4 | LOW | Existing `tags read`; probe list must be declared in the manifest (SF-8) |
| `restart-tasks/pending` reported | Restart-class changes | LOW | |

**Table stakes (backup node — spec §6.5; see §4.4 for the full rationale)**
| Check | Why | Complexity |
|---|---|---|
| Backup REST reachable; same `deploymentMode` as master AND as expected | Gateway only LOGS "Active deployment mode differs from peer node!" (S3) | LOW |
| `GET /mode` on the backup lists the mode; `find ...?collection=production` on the backup equals the render | S3: mode definition + collection reach the backup in ~3-4 s on a clean pair; a backup booted before the mode exists ignores it permanently | MEDIUM |
| Redundancy: master `peerConnected`, `syncPending=false`; backup `projectState=Good` | Spec §7 failover preconditions | LOW |
| File-hash of the backup's collection (SSH) == master's == render | Independent of the REST path | MEDIUM |
| Projects on the backup match the release zips (export hash) | **TEC-5105: project sync to .29 is the known-broken path** — likeliest failure | MEDIUM |
| Convergence loop with timeout; on mismatch one `POST /redundancy/gwaction/resync`, then re-check once; still failing -> `deploy_backup_unsynced` (critical) | Spec §6.5.2-3 | MEDIUM |
| Detect "sync exhausted" ("Sync operation has failed 5 time(s)... No further synchronization attempts") from backup logs | RIG.md: a backup that exhausted attempts stays stale SILENTLY; content mismatch alone eventually shows it, the log line names the cause | LOW |

**Differentiators:** `lastSyncTimestamp` freshness as a SECONDARY signal only (existing code flags its unit as INFERENCE; `-1` = never synced) — content equality is primary; `verify --node both` runs master and backup checks concurrently; output `nodes[]` per node so alerting can route "backup out of sync" separately.

**Anti-features:** triggering failover to "prove" the backup (200 lies; belongs to rolling restart §7); marking verify green on `StatusPing` alone; comparing ONLY the hash of files (see log-scan check).

### 3.6 `ign deploy projects`

**Expected behavior (spec §5.2, §6.4.5, F2):** manifest pins the project set (name, version, parent, release-zip path/URL, sha256). Import parents-first, skip hash no-ops, verify by hash after.

| Aspect | Contract |
|---|---|
| Inputs | `--env`, `--gateway`, `--yes`, `--allow-destructive`, `--run ID`, `--only NAME`, `--artifacts DIR` |
| Order | topological by `parent` (cycle = exit 2 pre-network); parents first; **never re-import a child merely because its parent changed** (S5: gateway restarts bound children itself) |
| JSON `data` | `{scope:"projects", order:[...], projects:[{name, pinned_version, parent, action:"import\|skip_noop\|blocked", member_diff:{added,changed,removed,same}, destructive, verify:{ok, expected_sha256, actual_sha256}}], unmanaged:[names on gateway not in manifest]}` |
| Gate preview | per project: action, order position, member-diff summary, `removed` members listed (potential stranded Designer saves — TEC-4624 class), open-Designer check, "children of X will restart (S5)", snapshot location |
| Complexity | MEDIUM — heavy reuse of v1.0/v1.1 project machinery |

**Table stakes**
| Feature | Why | Complexity | Notes |
|---|---|---|---|
| Verify the release zip against its pinned sha256 BEFORE any gateway write | Supply-chain/mix-up guard; module-artifact precedent (RMOD-02) | LOW | |
| Hash no-op skip (the §13.6 cutover expectation: "the import must be a hash no-op") | Avoids needless child restarts + Perspective session refresh | MEDIUM | Compare via `diff_members` normalized member hashes of live export vs zip |
| Parents-first ordering with cycle/missing-parent detection | Spec §5.2 | LOW | |
| Pre-import export of each project to the run dir (rollback input) | Spec §6.4.2 | LOW | |
| `import?overwrite=true` then re-export and compare hash to the release zip; mismatch -> exit 6 | Spec §6.4.5 "verify by hash" | MEDIUM | Zip is gateway-produced (CI export), so formats are comparable |
| Designer-session pre-check (same policy flag as apply) | Overwrite import discards unsaved Designer edits | LOW | |
| Report unpinned projects on the gateway (`unmanaged`), never delete | F2 pins "every project on that gateway"; absent from manifest = finding, not a delete | LOW | |

**Differentiators:** per-project `removed`-member list as the destructive signal (resources live on prod but absent from the release = Designer saves that would be lost) — no base render needed; `--only` for partial promotion; verify the backup holds the same project hashes (feeds §3.5).

**Anti-features:** deleting projects absent from the manifest (`DELETE /projects/{name}` exists — do not wire it in v1.6); importing children after a parent "just in case" (S5 says no); building the release zip in `ign` (CI exports it from an ephemeral gateway so the bytes are gateway-produced — spec D10).

---

## 4. Cross-cutting design answers

### 4.1 How `plan` expresses core-shadowed resources and destructive changes

**Core shadowing** is orthogonal to the add/change/remove status, so it is a separate field `core` on every entry plus a summary counter, evaluated at the **granularity the spikes verified** (S1/S2/S2b):

| `core` value | Meaning | Effect |
|---|---|---|
| `none` | no copy of this resource in `core` | normal |
| `identical` | `core` holds the same normalized content (the state at/before cutover: declared resources still live in `core`) | informational; strip pending |
| `differs` | `core` holds different content (a UI edit that the mode silently overrides) | WARNING; mode wins on the gateway, but the UI shows the `core` copy |
| `hidden` | removal of a tag inside a folder the mode still defines (S2: core-only tag is hidden) | informational; deletion works |
| `shadows_removal` | status=`removed` but `core` still has the resource/folder (S2b: core-only **folder** shows through) | **BLOCKING**: the declared deletion will NOT take effect. `apply` refuses (not `--yes`-able) until the core strip (§13.5) has been done. Summary counter `core_shadow_blocking`. |

Plan prints a top-level warning when `core` still contains ANY declared-type resource ("cutover core strip not done"). Tag shadowing is judged at tag-folder-resource granularity, not per tag.

**Destructive changes** are a boolean per entry with a machine reason, a summary count, and a gate escalation:

- `destructive:true` for: every `removed` entry in the allowlist (resource deletions); `removed` or type-changed `tag-definition`/`tag-type-definition`/`tag-provider` (memory-tag values and UDT instances are lost — spec §8, §14); `removed` store-and-forward engine or alarm journal (buffered/historical data); `removed` database connection/OPC connection/identity provider (breaks bindings; locks users out).
- `lockout_risk:true` (separate flag) for `changed` `security-properties`, identity providers, security zones, anything touching the deploy token's level (RIG.md: API stayed 403 until the level was in the `core` `security-properties`).
- Gate escalation: `--yes` confirms the plan; any `destructive` or `lockout_risk` entry additionally requires `--allow-destructive` (CI binds it to the spec's `destructive` PR label + explicit approval). Destructive/blocking lists are ALWAYS printed in full in the refusal message.

### 4.2 Minimum viable rollback

See the snapshot table in §3.4. In one line: **snapshot = the target mode-collection tree (byte-exact + hashes) + pre-import project export zips + a run record with pre/post fingerprints; rollback = apply(source=snapshot) with the same lock/scan/verify engine; the `.gwbk` is the disaster tier, not the rollback tier.** Auto-rollback fires only on master post-apply verification failure (D7/§8); every other rollback is operator-invoked or "revert the PR".

### 4.3 Mirror/transport notes that shape features

- Windows OpenSSH both ends; default shell may be `cmd` or PowerShell (quoting hazard); use `scp`/`sftp` for transfer and a single remote command for hashing/mirroring.
- Gateway collection root is `data/config/resources/<mode>/`; SSH account is scoped to it (spec §9) — the stage dir must be INSIDE writable scope or use a sibling staging path the account may write; confirm in the SSH-transport phase.
- Lock window should contain only: local mirror + (optional) re-hash. All transfer happens outside the lock.

### 4.4 What `verify` checks on the backup node, and why

The backup is never written by `ign`; it is only checked, because three independently-verified failure modes live there: (1) it ignores the mode argument if the mode was not present at its boot and only logs a warning on mismatch (§3, S3); (2) a backup whose config sync failed 5 times stops trying and stays stale silently (RIG.md); (3) project sync to .29 is already known broken (TEC-5105). Therefore the backup checks are: same active mode; mode defined; collection content equal (REST + SSH hash); redundancy flags healthy; projects hash-equal; bounded convergence wait; exactly one `resync` retry; explicit sync-exhausted detection. All of it is read-only except the single sanctioned `resync`.

---

## 5. Spec Flags (decisions or gaps requiring acknowledgement)

| # | Flag | Type | Recommendation |
|---|---|---|---|
| **SF-1** | The frozen failure envelope has no `data`, so apply/verify/auto-rollback cannot return structured findings or a run id on failure. | Contract collision | Keep the envelope frozen. Always write `result.json` into the run dir; failure `message` carries run id + counts; `hint` carries the path. Offer (do not assume) an ADDITIVE optional `error.details` later. Needs user ack because agents/CI will read the file, not stderr JSON. |
| **SF-2** | "Changes present" exit code: Terraform uses 2, kubectl uses 1; ign's 1 = internal, 2 = usage/`confirmation_required`. Reusing either would make CI mistake a normal plan for an error (or a usage error for a plan). | Contract collision | `plan` exits 0; `data.summary.has_changes` (+ counts) is the signal (matches `project diff`/`doctor`). Defer `--detailed-exitcode`; if ever wanted it needs a NEW exit code (taxonomy change = deliberate decision). |
| **SF-3** | Spec §6.4.6 has apply run the rolling restart (§7); milestone excludes `ign redundancy restart --rolling`. | Scope | v1.6 apply reports `restart_pending[]` and stops. Exit 0 with a prominent field; the pipeline (or a later milestone) runs the restart. Document so nobody assumes apply restarts. |
| **SF-4** | Spec §6.4.1 / §14 require "no open drift findings" and "plan refuses while drift is open", but `ign drift` is deferred. | Dependency gap | v1.6: `plan` shows live-vs-render divergence inherently; optional `--base` three-way marks `drift:true`; `apply` refuses when drift entries exist unless `--allow-drift "<reason>"` (reason recorded in `run.json`) — the spec's "recorded reason". Without `--base`, ign cannot distinguish drift from intended change; the pipeline must supply the previous render. |
| **SF-5** | §13.5 (strip declared resources from `core`, snapshot `core` first) and §13.2 (bootstrap: read prod into `gateways/`) have NO command in §11. | Spec gap | Plan must still FLAG shadowing (it does). Recommend `apply --strip-core` (guarded, snapshots `core` into the run, rollback restores it) as a cutover-only differentiator, or explicitly script it via `ign api call`/SSH for the one-time cutover and say so. Bootstrap (`ign deploy pull`/adopt-style reverse of render) is a separate decision; the §13.2 "empty plan" gate needs a way to create the tree. |
| **SF-6** | §6.4.2 snapshot includes `GET /backup` (.gwbk) by default — heavy on prod; also snapshot covers only the mode collection + projects. | Cost note | Keep default-on (binding). Allow recorded `--skip-gwbk`. Snapshot `core` only when the run strips it (SF-5). |
| **SF-7** | §4 overlay is path-based add/override only; no way to say "this resource is absent in `test`". | Limitation | Accept for v1.6; document; revisit if a real case appears. |
| **SF-8** | §6.4.4 "sample of OPC and MQTT tags reads Good" needs a declared probe list; `manifest.yaml` schema in §4 lists types/providers/projects/standards_version only. | Schema gap | Add optional `verify.tag_probes[]` (paths + expected quality) and `verify.timeouts`; absent list = warning, not pass. |
| **SF-9** | `tests/tui_coverage.rs` requires a TUI route per clap leaf; the OutOfBand set is documented as closed; MCP catalog auto-derives; guarded verbs need markers/confirm parity. | Scope cost | Budget 6 TUI route rows (minimal screens or a read-only runs view) or an explicit, documented exception for CI-first verbs; add `// guarded:` markers for apply/rollback/projects. |
| **SF-10** | `targets/<env>.yaml` (§5.4) vs `ign` profiles (URL + token via env/keyring chain). Two config systems for one gateway. | Design question | Targets file names profile(s) for master/backup and adds `ssh:{host,user,key_ref,os,data_dir}` + restart/failover method; secrets stay in the existing chain. CI sets profile env vars as today. |

Refinements (not disagreements): render byte-copies canonical gateway files and only runs the Gson-compatible writer for files `ign` synthesizes (v1.1 fidelity lesson; output remains deterministic); apply ends with backup verification by default (spec §6.5 is part of the deploy, and a deploy that cannot say the backup converged must not exit 0).

---

## 6. Feature dependencies

```
Config API client (scan-lock, scan/config, mode, find/list, resync, restart-tasks, designers; 204-tolerant)
    |--> render --> (render_sha256)
    |                    |--> plan --(plan_sha256)--> apply(--plan-sha256)
    |                    |        \--(same diff engine)--> verify (empty-plan semantics)
    |                    \--> verify
SSH transport (read tree/hash, upload stage, local mirror, snapshot pull, Windows)
    |--> plan (live collection hash) --> apply (snapshot, mirror) --> rollback (apply(source=snapshot))
manifest.yaml schema (allowlist, projects, verify probes) --> render, plan, projects, verify
apply --requires--> snapshot --requires--> SSH read + existing backup_download + project_export
auto-rollback --requires--> rollback engine == apply engine (build once)
projects --requires--> existing project_export/import + diff_members + sha256 + parent topo-sort
verify(backup) --requires--> backup profile/target + redundancy status (existing) + logs (existing) + resync
three-way plan (--base) --enhances--> plan, apply drift gate (SF-4)
core strip (SF-5) --conflicts-with--> running before plan shows core_shadow clean (it is the cutover step)
rolling restart --NOT in v1.6--> apply only reports restart_pending
```

**Dependencies on existing `ign` capabilities (reuse, don't rebuild):** `require_confirmation`/guarded markers/MCP confirm gate; `CoreError` + Three-Place rule; frozen envelope + `data` struct conventions; `poll.rs`; `Session` seam; `diff_members` + `normalize_descriptor` (resource.json normalization); sha256 (`sha2` already in core); `project_export`/`project_import`/`project_sync` MemberSource engine; `backup_download`/`backup_restore`; `redundancy_status` (`peerConnected`, `syncPending`, `projectState`, `lastSyncTimestamp` with `-1` sentinel and INFERENCE caveat); `logs`; `tags read`; doctor `CheckResult` rows; `ign api call` for any route without a typed client yet.
**NEW dependencies:** YAML parsing (manifest/targets); SSH/SCP transport (nothing in the dependency tree today — shell out to the system OpenSSH client vs a Rust crate is a STACK decision; Windows both ends); typed config-API client for scan-lock/scan/mode/resources/`resync`/`designers`.

---

## 7. MVP definition

### Launch With (v1.6)
- [ ] Config API client + SSH transport (library-only foundation) — everything else blocks on it
- [ ] `render` — deterministic overlay, allowlist, embedded-secret lint, stable uuids, `render.json`
- [ ] `plan` — four-status diff over SSH hashes + REST core layer, destructive/lockout/core-shadow classification, preflight rows, `plan_sha256`
- [ ] `apply` — preflight, snapshot (collection + projects, `.gwbk` default), gate, scan-lock with expiry detection, mirror, scan+poll, master verify, auto-rollback, backup verification, run dir, `restart_pending` reporting
- [ ] `rollback` — snapshot restore through the apply engine, staleness refusal
- [ ] `verify` — master + backup checks incl. the loaded-ness/log-scan check and backup convergence/resync
- [ ] `projects` — pinned set, hash no-op skip, parents-first, hash verify, destructive signal

### Add After Validation (v1.6.x)
- [ ] `--plan-sha256` approval binding — as soon as the test -> approval -> prod pipeline exists (LOW; arguably belongs in launch)
- [ ] Three-way `--base` plan / `--allow-drift` (SF-4) — when `ign drift` is still deferred and the pipeline needs the gate
- [ ] Stage-then-mirror + measured holdTimeout defaults — after S4 numbers exist on the real transport
- [ ] `apply --strip-core` (SF-5) — needed once, at cutover; could also be a scripted one-off

### Future Consideration (v2+)
- [ ] `ign drift`, `ign redundancy restart --rolling` — separate milestone per roadmap
- [ ] Tombstone overlays (SF-7); restart-class hints; `rollback --list`; `deploy pull`/bootstrap if not scripted

## 8. Feature prioritization matrix

| Feature | User value | Cost | Priority |
|---|---|---|---|
| Config API client + SSH transport | HIGH | HIGH | P1 |
| apply state machine (lock/expiry/mirror/scan/verify) | HIGH | HIGH | P1 |
| Snapshot + auto-rollback + `rollback` | HIGH | MEDIUM | P1 |
| plan (diff + classification) | HIGH | HIGH | P1 |
| verify incl. backup + log-scan loaded-ness | HIGH | MEDIUM-HIGH | P1 |
| render + lint + stable uuid | HIGH | MEDIUM | P1 |
| projects (no-op skip, parents-first, hash verify) | HIGH | MEDIUM | P1 |
| `--plan-sha256` binding | HIGH | LOW | P1/P2 |
| Lockout-risk guard | MEDIUM-HIGH | MEDIUM | P2 |
| Three-way drift via `--base` | MEDIUM-HIGH | MEDIUM | P2 |
| Stage-then-mirror | MEDIUM | MEDIUM | P2 |
| `--strip-core` | MEDIUM (one-time) | MEDIUM | P2 |
| TUI screens beyond route rows | LOW | MEDIUM | P3 |
| `--detailed-exitcode`, restart hints, `runs` list | LOW | LOW-HIGH | P3 |

## 9. Competitor feature analysis

| Feature | Terraform | kubectl | Helm | Stoker | Community Ignition | `ign deploy` |
|---|---|---|---|---|---|---|
| Plan/diff before write | yes | `diff` | `diff` plugin | `dryRun` | no | `plan` (+ refusal-as-preview) |
| Change-present exit code | 2 (`-detailed-exitcode`) | 1 | n/a | n/a | n/a | `data.summary.has_changes`, exit 0 (SF-2) |
| Apply exactly the reviewed plan | `-out` file | no | no | no | no | `--plan-sha256` |
| Prune/delete scoping | explicit destroy | selector/ApplySet | release-owned | managed paths only | no | manifest allowlist only |
| Concurrency lock | state lock | none | none | none | none | Ignition scan-lock + expiry detection |
| Snapshot + rollback | no | no | `rollback` (revisions) | roadmap only | no | collection + projects (+ .gwbk), auto + manual |
| Post-apply verification | no | rollout status | `--wait`/`--atomic` | roadmap only | no | read-back + loaded-ness + hashes + tag probe |
| Redundant/peer node check | n/a | n/a | n/a | n/a | no | backup verification + resync (spec §6.5) |
| Drift handling | refresh/`-refresh-only` | `diff` | none | roadmap only | no | alert-only (D1); `--base` three-way |

## Sources

- Spec: `whk-environment-orchestration/.../2026-10-09-prod-gateway-config-as-code-design.md` (D1-D13, §3-§8, §10-§15) — HIGH
- Spikes: `spikes/gateway-modes/results/SUMMARY.md`, `RIG.md` (verbatim 8.3.6 gateway messages) — HIGH
- `ignition-cli/.planning/PROJECT.md`; source: `error.rs` (exit taxonomy, failure envelope), `output.rs` (success envelope), `main.rs` (`preview_then_confirm`, guarded markers), `actions/projects.rs` + `client/resources.rs` (`project_diff`, `normalize_descriptor`), `client/redundancy.rs`, `actions/doctor.rs` (exit-0-on-findings contract), `poll.rs`, `tests/tui_coverage.rs` — HIGH
- Terraform `plan` docs (`-detailed-exitcode`, `-out`, `-refresh-only`, stale plans, `-json`): https://developer.hashicorp.com/terraform/cli/commands/plan — HIGH
- kubectl ApplySet / legacy `--prune` problems: https://kubernetes.io/blog/2023/05/09/introducing-kubectl-applyset-pruning/ — MEDIUM-HIGH
- Helm `--atomic` / Helm 4 `--rollback-on-failure`: https://helm.sh/docs/helm/helm_upgrade and Kestra Helm 4 plugin notes — MEDIUM (Helm 4 rename not confirmed from release notes)
- Stoker operator: https://github.com/knorrlabs/stoker-operator — README, `docs/docs/overview/architecture.md`, `concepts.md`, `reference/gatewaysync-cr.md`, `roadmap.md`, `internal/ignition/client.go` (scan order projects then config; no scan-lock) — MEDIUM (shipped vs roadmap separated)
- IA 8.3 version control guide: https://www.docs.inductiveautomation.com/docs/8.3/tutorials/version-control-guide ; deployment modes: https://docs.inductiveautomation.com/docs/8.3/platform/gateway/web-interface/platform/gateway-deployment-modes — HIGH (collection/mode specifics beyond these come from the spec's verified spikes)
- Community: IA forum "Ignition 8.3 GIT Deployment Best Practices" (https://forum.inductiveautomation.com/t/ignition-8-3-git-deployment-best-practices/109355), DMC git-hook post (https://www.dmcinfo.com/blog/44721/ignition-8-3-gateway-file-sync-git/) — LOW (copy files + scan only)
- Ansible `--check --diff` semantics: general product knowledge, not re-fetched — MEDIUM

---
*Feature research for: declared-config deploy of a redundant Ignition 8.3 pair*
*Researched: 2026-10-10*
