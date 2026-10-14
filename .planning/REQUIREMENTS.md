# Requirements: ignition-cli — v1.6 Declared Gateway Deploy

**Defined:** 2026-10-10
**Core Value:** One binary that lets a developer (or an AI agent) fully operate and inspect an Ignition 8.3+ gateway — health, projects, tags, rigs — without opening the gateway webpage or Designer.

**Design spec:** whk-environment-orchestration PR #60 — `docs/superpowers/specs/2026-10-09-prod-gateway-config-as-code-design.md` (Plan 2 of that roadmap). **Research:** [`research/SUMMARY.md`](research/SUMMARY.md) — conflict resolutions, capture backlog C1–C12, spec flags SF-1..SF-10.

**Decisions recorded at scoping (2026-10-10):**

- Bootstrap (`ign deploy pull`) is in v1.6; the spec §13.5 core strip is not (SF-5).
- Post-write red outcomes keep the frozen envelope: `ok:true`, `data.ok=false`, verdict slug in `data`, exit 6 (Decision A).
- CI and agent callers must pass `--plan-sha256`; `apply`/`rollback`/`projects` are not exposed as MCP tools in v1.6 (Decisions C, D).
- In: `--base` drift gate, verify tag probes, `.gwbk` snapshot default-on, Designer preflight.
- Spec correction: rendered JSON preserves source key order (spec §4 "sorted keys" is falsified by the 183-file corpus).

## v1.6 Requirements

Numbering: new categories. **47 requirements.**

### Config API Client (library-only)

- [x] **DAPI-01**: The core library can list and read config resources by type and name, from the effective view or a named collection
- [x] **DAPI-02**: The core library can create config resources by POST and update them with the signature-checked array PUT; a stale or missing signature surfaces as its own error, never a generic failure *(amended 2026-10-12: create is POST per OpenAPI and the v1.1 capture of PUT-unknown-name → 404; confirmed on 8.3.6 by capture C3a)*
- [x] **DAPI-03**: The core library can acquire the config scan-lock, read its remaining time, and detect expiry (200 JSON while held → 204 after) without comparing gateway timestamps to the local clock
- [x] **DAPI-04**: The core library can trigger a config scan and decide completion by `scanActive == false` and `lastScanTimestamp` advanced on the same gateway — HTTP 200 alone is never success
- [x] **DAPI-05**: The core library can list and create deployment modes and reads the active `deploymentMode`, normalizing `null` and `""` to "no mode"
- [x] **DAPI-06**: Every new endpoint has a declared response shape (JSON, empty 2xx, or 204) proven by a live capture; gateway bodies echoed into errors are size-capped
- [x] **DAPI-07**: A held scan-lock (409) surfaces as `scan_lock_held` naming the holder, and redundancy status decides the active node by activity level with the live `Master`/`Backup` vocabulary

### Render

- [x] **DRND-01**: User can run `ign deploy render` on a `gateways/<name>/` tree for an environment and get a rendered tree whose `render_sha256` is identical for identical inputs
- [x] **DRND-02**: `manifest.yaml` and `targets/<env>.yaml` parse strictly — unknown fields and a wrong `schema_version` fail with file, line and column
- [x] **DRND-03**: Files under `modes/<env>/` override the base tree by path, whole-file
- [x] **DRND-04**: Only manifest-allowlisted resource types are rendered; security singletons (security-properties, security-levels, API tokens, user sources) are refused
- [x] **DRND-05**: Gateway-written files pass through byte-for-byte; files `ign` synthesizes use a Gson-compatible, order-preserving writer, proven by the gateway corpus round-tripping byte-identical on Linux, macOS and Windows
- [x] **DRND-06**: New named resources get a deterministic name-based `attributes.uuid` from a per-type rule table; singleton types get none; existing uuids are kept
- [x] **DRND-07**: Render refuses an embedded secret (an `Embedded` secret value or a plaintext secret field), naming the path and never the value
- [x] **DRND-08**: Render refuses case-fold collisions, Windows-illegal names, paths over the Windows length budget, and CR bytes in the tree

### Transport, Plan, Pull, Verify (read-only)

- [ ] **DPLN-01**: `ign` reads and writes a gateway's config tree over system OpenSSH to a Windows OpenSSH host (and over the local filesystem for rigs and tests) with batch mode, a pinned host key, per-call timeouts and argv-only invocation; file hashes are computed client-side
- [x] **DPLN-02**: User can run `ign deploy plan` to see each resource as added, changed, removed or same against the live master, with zero writes and zero locks, exit 0, `data.summary.has_changes`, and a `plan_sha256` (Phase 22 D-06: a fifth status, `unmanaged`, lists live allowlisted resources that are neither rendered nor in the delete set; never deleted, never counted in has_changes)
- [x] **DPLN-03**: The plan classifies every entry's relation to `core` (none, identical, differs, hidden, shadows_removal) and flags destructive and lockout-risk entries
- [x] **DPLN-04**: The plan states the exact delete set — previously declared names minus rendered names — which never includes `config-mode.json` or an undeclared resource
- [x] **DPLN-05**: User can pass `--base <previous render>` to mark entries the live gateway changed outside the pipeline as drift
- [ ] **DPLN-06**: User can run `ign deploy pull` to bootstrap a `gateways/<name>/` tree from a live gateway read-only, such that `render` then `plan` against that gateway is empty
- [ ] **DPLN-07**: User can run `ign deploy verify` against the master: scan complete, `deploymentMode` equals the target, every declared resource reads back equal in the effective view, and the gateway log has no "will not be loaded" entry for a declared resource
- [ ] **DPLN-08**: `verify` reads the manifest's `verify.tag_probes[]` and fails when a probed tag is not Good; an absent probe list is a warning, never a pass
- [ ] **DPLN-09**: A red `verify` returns the frozen envelope with findings in `data` and exit 6

### Apply and Rollback (single gateway)

- [ ] **DAPL-01**: `ign deploy apply` without `--yes` refuses and prints the plan as the refusal; with `--plan-sha256`, a plan that changed since review refuses as `plan_stale`
- [ ] **DAPL-02**: Destructive entries require `--allow-destructive`; drift entries require `--allow-drift "<reason>"`, recorded in the run; `shadows_removal` entries block apply regardless of flags
- [ ] **DAPL-03**: An empty plan exits 0 as a no-op with no lock, snapshot or scan
- [ ] **DAPL-04**: Before any write, apply runs every preflight and reports all failures at once: `deploymentMode` on both nodes, wrong-target guard, write-permission probe, no on-box journal, and open Designer sessions (`--designers fail|wait|proceed`, default fail)
- [ ] **DAPL-05**: Before writing, apply snapshots the target collection (on the gateway host and in the run directory, with hashes) and downloads a `.gwbk` by default; `--skip-gwbk` is recorded; neither is ever uploaded off-prem
- [ ] **DAPL-06**: Apply writes inside the scan-lock with a measured hold budget; if the lock expires or a write fails mid-way, apply restores the snapshot and scans, so no partial tree is ever loaded
- [ ] **DAPL-07**: A master verify failure after apply triggers automatic rollback through the same engine and reports `deploy_rolled_back` with exit 6; a failed rollback reports `rollback_failed` with recovery guidance
- [ ] **DAPL-08**: User can run `ign deploy rollback --run <id>` from a fresh process to restore that run's snapshot; it refuses when live state differs from the run's post-apply fingerprint unless `--discard-live-changes`
- [ ] **DAPL-09**: An interrupted apply leaves an on-box journal, and plan, apply and rollback refuse while it is present until recovery runs
- [ ] **DAPL-10**: Every run, including failed ones, writes a private run directory (`run.json`, `result.json`, `plan.json`, `events.ndjson`); apply reports `restart_pending` and never restarts or fails over a gateway

### Redundant Pair

- [ ] **DPAIR-01**: Apply ends by verifying the backup through its own API and file hashes: same `deploymentMode`, effective view equal to master, redundancy healthy — after a bounded convergence wait and at most one resync
- [ ] **DPAIR-02**: A backup that stopped syncing or still differs reports `deploy_backup_unsynced` with exit 6 and never rolls back the master
- [ ] **DPAIR-03**: User can run `ign deploy verify --node both`, which also checks that every referenced secret name exists on both nodes
- [ ] **DPAIR-04**: A two-node 8.3.6 rig with a trusted gateway-network link live-gates DPAIR-01..03

### Projects

- [ ] **DPRJ-01**: `ign deploy projects` verifies every project artifact against its manifest sha256 pin before any upload
- [ ] **DPRJ-02**: Projects import parent-first from `project.json` inheritance; cycles and unknown parents refuse; unchanged projects (canonical hash) are skipped
- [ ] **DPRJ-03**: Each project is exported before it is overwritten and verified after import by canonical hash, ignoring `lastModification*` and signatures
- [ ] **DPRJ-04**: Projects not in the manifest are reported and never deleted; a git-managed project (`.git` present) is refused
- [ ] **DPRJ-05**: `ign deploy projects --check` reports what would change with no writes

### Surface and Contract

- [x] **DSURF-01**: Each deploy verb lands with its TUI route row, README entry and exit-table rows in the same phase; every new error follows the four-place rule
- [ ] **DSURF-02**: `apply`, `rollback` and `projects` are registered in `GUARDED_OPS` and are not exposed as MCP tools in v1.6; `render`, `plan`, `pull` and `verify` are
- [ ] **DSURF-03**: A secret canary test proves no secret value appears in plan, apply or verify output, `--json`, run-directory files or events
- [ ] **DSURF-04**: The verdict envelope (`ok:true`, `data.ok=false`, exit 6) is documented as a dated README contract exception

## Future Requirements (deferred)

- `ign drift` — scheduled drift detection and alerting (roadmap Plan 6)
- `ign redundancy restart --rolling` — restart-class changes and failover sequencing (roadmap Plan 5)
- Guarded `apply --strip-core` with core snapshot (spec §13.5, SF-5)
- `--detailed-exitcode` for "changes present" (needs an exit-taxonomy change)
- `rollback --list`, tombstone overlays ("absent in env"), restart-class hints
- A real TUI deploy panel beyond registry rows
- MCP exposure of `apply`/`rollback`/`projects` once the plan-hash handshake has shipped and been used

## Out of Scope (v1.6)

| Item | Reason |
|------|--------|
| `--force`, `--skip-verify`, `--no-rollback` | Each removes a safety the design depends on |
| Auto-reverting drift | Drift is reported and gated, never silently overwritten |
| Any write to the backup node | Backup state comes from redundancy sync; `ign` only checks it |
| Templated or JSON-patch overlays | Whole-file overlays keep render reviewable |
| Secret values in the rendered tree | References only; values stay in the gateway secret store |
| Deleting projects absent from the manifest | Unmanaged projects are reported, not removed |
| "Rollback to a git ref" | That is render + plan + apply of the old commit |
| An SSH library dependency (russh, ssh2, openssh) | MSRV 1.89, dependency weight, Windows client gaps; system OpenSSH is used instead |
| Workspace MSRV bump | Not coupled to this milestone |
| Edge gateway (8.1) and controls/HMI stack | Spec 3 and later fleet plans |

## Traceability

v1.6 only. The parked v1.3 requirements below keep their own section.

| Requirement | Phase | Status |
|-------------|-------|--------|
| DAPI-01 | Phase 20 | Complete |
| DAPI-02 | Phase 20 | Complete |
| DAPI-03 | Phase 20 | Complete |
| DAPI-04 | Phase 20 | Complete |
| DAPI-05 | Phase 20 | Complete |
| DAPI-06 | Phase 20 | Complete |
| DAPI-07 | Phase 20 | Complete — library level (`scan_lock_held`, `active_node`); `ign redundancy status` CLI wiring tracked in Phase 24 |
| DRND-01 | Phase 21 | Complete |
| DRND-02 | Phase 21 | Complete |
| DRND-03 | Phase 21 | Complete |
| DRND-04 | Phase 21 | Complete |
| DRND-05 | Phase 21 | Complete (CI run 38483193605) |
| DRND-06 | Phase 21 | Complete |
| DRND-07 | Phase 21 | Complete |
| DRND-08 | Phase 21 | Complete |
| DPLN-01 | Phase 22 | Pending |
| DPLN-02 | Phase 22 | Complete |
| DPLN-03 | Phase 22 | Complete |
| DPLN-04 | Phase 22 | Complete |
| DPLN-05 | Phase 22 | Complete |
| DPLN-06 | Phase 22 | Pending |
| DPLN-07 | Phase 22 | Pending |
| DPLN-08 | Phase 22 | Pending |
| DPLN-09 | Phase 22 | Pending |
| DAPL-01 | Phase 23 | Pending |
| DAPL-02 | Phase 23 | Pending |
| DAPL-03 | Phase 23 | Pending |
| DAPL-04 | Phase 23 | Pending |
| DAPL-05 | Phase 23 | Pending |
| DAPL-06 | Phase 23 | Pending |
| DAPL-07 | Phase 23 | Pending |
| DAPL-08 | Phase 23 | Pending |
| DAPL-09 | Phase 23 | Pending |
| DAPL-10 | Phase 23 | Pending |
| DPAIR-01 | Phase 24 | Pending |
| DPAIR-02 | Phase 24 | Pending |
| DPAIR-03 | Phase 24 | Pending |
| DPAIR-04 | Phase 24 | Pending |
| DPRJ-01 | Phase 25 | Pending |
| DPRJ-02 | Phase 25 | Pending |
| DPRJ-03 | Phase 25 | Pending |
| DPRJ-04 | Phase 25 | Pending |
| DPRJ-05 | Phase 25 | Pending |
| DSURF-01 | Phase 21 | Complete |
| DSURF-02 | Phase 23 | Pending |
| DSURF-03 | Phase 23 | Pending |
| DSURF-04 | Phase 22 | Pending |

**Coverage:** 47 v1.6 requirements, 47 mapped, 0 unmapped, 0 mapped twice.

| Phase | Requirements | Count |
|-------|--------------|-------|
| 20. Config API Client | DAPI-01..07 | 7 |
| 21. Deploy Render | DRND-01..08, DSURF-01 | 9 |
| 22. Transport, Plan, Pull & Verify | DPLN-01..09, DSURF-04 | 10 |
| 23. Guarded Apply & Rollback | DAPL-01..10, DSURF-02, DSURF-03 | 12 |
| 24. Redundant Pair | DPAIR-01..04 | 4 |
| 25. Guarded Project Deploy | DPRJ-01..05 | 5 |

**Cross-cutting assignments** (each is mapped to the phase where it is first fully provable; later phases must keep it green):

- **DSURF-01** closes in Phase 21 with the first verb (`render`) and the `deploy` family coverage test. Phases 22–25 keep it green as `plan`, `pull`, `verify`, `apply`, `rollback`, `--node both` and `projects` land.
- **DSURF-03** closes in Phase 23, the first phase with apply output, a run directory and events. Phase 22 starts the canary over `plan` and `verify` output; Phases 24 and 25 keep it green.
- **DSURF-02** closes in Phase 23, where the MCP exclusion for guarded deploy verbs is built and `apply`/`rollback` are registered. Phase 25 registers `projects` under the same rule.
- **DSURF-04** closes in Phase 22 with the first verdict-producing verb (`verify`); Phases 23 and 24 reuse the same verdict envelope.

---

# Parked: v1.3 Rig Modules & Declared Discovery (open)

v1.3 Phases 15, 16 and 19 shipped (RMOD-01..07). Phases 17 and 18 are parked behind v1.6; their requirements stay open below.

### v1.3 Requirements

Requirements for the v1.3 release. Numbering continues v1.1 (v1.0 shipped 44,
v1.1 shipped 25 — see `milestones/`). **14 requirements.**

Note: v1.2 shipped outside the milestone workflow (tag `v1.2.0`, PR #8 —
`ign testing run`, `ign session login`, the e2e Playwright scaffold) and has no
requirements archive. LEDG-01 reconciles that.

### Rig Modules

- [ ] **RMOD-01**: User can declare an optional module for a rig and have `ign` provision it on `rig up` — nothing is installed unless asked for; a rig with no module declaration behaves exactly as it does today
- [ ] **RMOD-02**: `ign` fetches the module as a **signed release artifact** pinned by version, verifies it against the release's published sha256 before use, and caches it keyed by version+digest; a digest mismatch is a hard failure, never a warning
- [ ] **RMOD-03**: Provisioning works offline when the pinned artifact is already cached and verified — no network access required on subsequent `rig up`
- [ ] **RMOD-04**: `ign` injects the module through a **generated compose override** (`compose.ign-modules.yml`), never by mutating the user's compose file and never by copying into a running container; the mount survives `rig up` recreates
- [ ] **RMOD-05**: The override sets the gateway's acceptance variables (`ACCEPT_IGNITION_EULA`, `ACCEPT_MODULE_CERTS`, `ACCEPT_MODULE_LICENSES`) so EULA, certificate, and licence gates clear without a human in the commissioning UI
- [ ] **RMOD-06**: The override is `ign`-owned output — regenerated rather than merged, carrying a header saying so, and removable without touching anything the user wrote. *Amended 2026-09-25:* removing it reverts the mount, NOT the gateway's install — Ignition installs an accepted module into its data directory, which survives. Proven live in 16-03; uninstall is 16-04.
- [ ] **RMOD-07**: The mechanism is a **module registry**, not Git-module-specific — a second module is registered and provisioned through the same seam to prove the abstraction holds

### Git Module Commissioning

- [ ] **GITM-01**: `ign` generates the module's `git.yaml` commissioning file from declared config — repo URI, branch, Ignition project name, inheritance, commissioning import flags, default branch
- [ ] **GITM-02**: **No credential is ever written to generated config.** `ign` resolves the git credential from keyring with an env fallback and wires it via `GATEWAY_GIT_USER_SECRET` / `GATEWAY_GIT_USER_SECRET_FILE`; a `user_password` key is never emitted
- [ ] **GITM-03**: `ign` refuses to emit a config unless **exactly one** project claims `gateway_exportResources`, naming the offending projects — zero means gateway-scoped resources are exported by nobody, two or more means competing projects overwrite each other
- [ ] **GITM-04**: `ign` reports the remaining human steps after provisioning (repo access, credential provisioning, first Designer connection) rather than failing silently or pretending they are done

### Declared Rig Discovery

- [ ] **RDISC-01**: Convention roots move out of the binary and into `[rig]` config — the hardcoded `WHK_HOME_ROOTS` and repo relpaths are gone, and rig discovery consults only what the user declared
- [ ] **RDISC-02**: Zero-config convenience survives for anyone who sets roots up once; a user with no declared roots gets a clear error naming every way to configure a rig, not a silent scan of someone else's directory layout

### Planning Ledger

- [ ] **LEDG-01**: v1.2's shipped work is recorded in `MILESTONES.md` with its requirements archived, so the ledger is continuous before v1.3 phases are numbered

## Out of Scope (v1.3)

| Item | Reason |
|------|--------|
| Repairing `docker/docker-compose.yml` upstream | Different repo (`WhiskeyHouse/ignition-git-module`). That rig is broken three ways — the module path is a *directory*, the image is stock with no developer mode, neither accept var is set. Since v1.3's supported path is signed-release-on-stock-image, deletion is likelier correct than repair. |
| The credential committed upstream | `docker/gw-init/git.yaml` is tracked in the git-module repo containing `user_password: abc123` plus a real name and email against live `WHK01` repos. Needs handling in that repo's history. |
| Unsigned or locally built modules | `ign` will never build, sign, or install an unsigned module. Unsigned requires developer mode set before the first module scan, which forces a custom image — out of scope permanently, not just for v1.3. |
| Module-backed CLI verbs (commit/pull/push through the gateway module) | Would create a second, competing sync story against the existing `ign workspace checkout/status/push`. |
| Auto-detecting that a user "probably wants" the Git module | Contradicts RMOD-01. No heuristic ever adds a module. |

## Key Constraints Carried In

- **Signed artifact source:** `WhiskeyHouse/ignition-git-module` GitHub releases. Current: `Git-2.3.4-signed.modl`, 7,704,578 bytes, `sha256:b74070346e587b1e1c14ff5093cc70625f7bfa14c2d24b24e7a0888a619b33eb`. This is `ign`'s first dependency on a third-party artifact feed — hence RMOD-03.
- **Why the override, not a copy:** `user-lib/modules` lives in the *image*, not the `gateway_data` volume. Anything copied into a running container is destroyed on the next recreate.
- **Why signed changes everything:** a signed module loads on the stock `inductiveautomation/ignition` image. No Maven, no custom build, no developer mode.
