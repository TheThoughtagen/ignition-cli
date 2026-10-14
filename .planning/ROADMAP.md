# Roadmap: ignition-cli

## Overview

Build `ign` — a single Rust binary + ratatui cockpit that replaces both the Ignition gateway webpage and the author's ignition-mcp server as the canonical human/agent interface to Ignition 8.3+ gateways.

**Mode:** mvp — every phase delivers an end-to-end user capability (vertical slice), not a horizontal layer.

## Milestones

- ✅ **v1.0 MVP** — Phases 1-7 (shipped 2026-08-30) — [archive](milestones/v1.0-ROADMAP.md)
- ✅ **v1.1 Agent Surface & IDE Integration** — Phases 8-14 (shipped 2026-09-16) — [archive](milestones/v1.1-ROADMAP.md)
- ✅ **v1.2** — shipped 2026-09-19 outside the milestone workflow (tag `v1.2.0`, PR #8: `ign testing run`, `ign session login`, e2e Playwright scaffold); no phase structure, ledger backfill tracked as LEDG-01
- ⏸ **v1.3 Rig Modules & Declared Discovery** — Phases 15,16,19 shipped; 17,18 parked behind v1.6
- 🚧 **v1.6 Declared Gateway Deploy** — Phases 20-25 (started 2026-10-10)

## Phases

<details>
<summary>✅ v1.0 MVP (Phases 1-7) — SHIPPED 2026-08-30</summary>

- [x] Phase 1: Foundation & Agentic Contracts (4/4 plans) — completed 2026-08-21
- [x] Phase 2: Gateway Health & Inspection (5/5 plans) — completed 2026-08-22
- [x] Phase 3: Project Operations (3/3 plans) — completed 2026-08-22
- [x] Phase 4: Rig Lifecycle & Trial State (4/4 plans) — completed 2026-08-23
- [x] Phase 5: WebDev Backend & Tag Operations (8/8 plans) — completed 2026-08-26
- [x] Phase 6: TUI Cockpit (11/11 plans) — completed 2026-08-28
- [x] Phase 7: Ecosystem Interop & Advanced Ops (6/6 plans) — completed 2026-08-29

Full phase details, goals, requirements mapping, and planner decisions: [milestones/v1.0-ROADMAP.md](milestones/v1.0-ROADMAP.md)

</details>

<details>
<summary>✅ v1.1 Agent Surface & IDE Integration (Phases 8-14) — SHIPPED 2026-09-16</summary>

- [x] Phase 8: Foundations — Session core, config migration, contract discipline (6/6 plans) — completed 2026-09-06
- [x] Phase 9: Agent Surface — api passthrough + curated diagnostics (8/8 plans) — completed 2026-09-07
- [x] Phase 10: EAM Write Operations — guarded lifecycle + blast-radius preview (7/7 plans) — completed 2026-09-11
- [x] Phase 11: Tag Bulk Transfer XML/CSV — byte-faithful + loss gates (8/8 plans) — completed 2026-09-14
- [x] Phase 12: TUI Theming & Degradation (4/4 plans) — completed 2026-09-14
- [x] Phase 13: Composite Engine — workspace, historian, edit (8/8 plans) — completed 2026-09-15
- [x] Phase 14: Transports — MCP + LSP (6/6 plans) — completed 2026-09-16

Full phase details, goals, requirements mapping, and planner decisions: [milestones/v1.1-ROADMAP.md](milestones/v1.1-ROADMAP.md)

</details>

### ⏸ v1.3 Rig Modules & Declared Discovery (Phases 15-19, Parked)

**Parked 2026-10-10:** Phases 15, 16 and 19 shipped (RMOD-01..07). Phases 17 and 18 stay open and are parked behind v1.6 Declared Gateway Deploy, per the PROJECT.md sequencing.

**Milestone Goal:** A developer can opt into running an Ignition module on a rig — signed, verified, and commissioned by `ign` — and rig selection becomes something declared rather than guessed.

**Design spec:** [research/2026-09-20-rig-module-provisioning-DESIGN.md](research/2026-09-20-rig-module-provisioning-DESIGN.md) (approved, commit `2fd00c7`)

**Structure rationale:** The artifact path is the risky, external-facing half and everything else depends on it, so fetch-and-verify lands first and alone — it is the only phase touching a third-party feed, and it is provable without Docker. Injection (the override) comes next because it is what makes a fetched artifact actually load, and it closes RMOD-01 end-to-end for a single module. Commissioning config is separable from injection: `git.yaml` generation and the credential/validation rules are pure generation logic over a schema, testable by golden file with no gateway. Discovery is independent of all three — it is the one phase that can slip without blocking module work — so it goes last and carries the small ledger backfill with it.

- [x] **Phase 15: 15-module-artifact-fetch-verify** — Pinned signed-release resolution, sha256 verification, version+digest cache, offline-from-cache *(RMOD-02, RMOD-03)* *(complete 2026-09-20; 2/2 plans, all 4 SCs tested; live fetch confirmed the real Git-2.3.4-signed.modl byte-for-byte)*
- [x] **Phase 16: 16-compose-override-module-injection** — Generated `compose.ign-modules.yml`, acceptance variables, recreate-durable mount, `ign`-owned regeneration, module registry with a second module proving the seam *(RMOD-01, RMOD-04, RMOD-05, RMOD-06, RMOD-07)*
- [ ] **Phase 17: 17-git-module-commissioning** — `git.yaml` generation, credential kept out via `GATEWAY_GIT_USER_SECRET`/`_FILE`, exactly-one `gateway_exportResources` validation, human-next-steps reporting *(GITM-01, GITM-02, GITM-03, GITM-04)*
- [ ] **Phase 18: 18-declared-rig-discovery** — Convention roots move from binary consts to `[rig]` config; clear error when nothing is declared; v1.2 ledger backfill *(RDISC-01, RDISC-02, LEDG-01)*
- [x] **Phase 19: 19-module-uninstall-on-undeclare** — `DELETE /data/api/v1/modules/uninstall` so a module the user stops declaring is actually removed from the gateway, not just unmounted. Opened by Phase 16's live falsification of SC-4 *(closes RMOD-06's amended half)* *(complete 2026-09-28; 3/3 plans; all four SCs proven, SC-1 and SC-4 LIVE on 8.3.3 and 8.3.9)*

**Dependencies:** 16 depends on 15 (nothing to inject without a verified artifact). 17 depends on 16 (the override carries the generated `git.yaml` mount). 18 is independent of 15-17 and may run in parallel or slip.

### 🚧 v1.6 Declared Gateway Deploy (Phases 20-25, In Progress)

**Milestone Goal:** `ign` deploys a declared gateway config tree (`gateways/<name>/`) to a redundant Ignition 8.3 pair through the `production` deployment-mode collection — plan, apply, verify, roll back — with no dependency on the git module.

**Design spec:** whk-environment-orchestration PR #60 — `docs/superpowers/specs/2026-10-09-prod-gateway-config-as-code-design.md` (§3 verified 8.3.6 facts, §4 layout, §5–§6 pipeline, §8 rollback, §11, §15 fleet model; evidence in `spikes/gateway-modes/results/`, incl. `RIG.md`). This milestone is that roadmap's Plan 2. Research: [research/SUMMARY.md](research/SUMMARY.md) (conflict resolutions, capture backlog C1–C12, spec flags SF-1..SF-10).

**Structure rationale:** Correctness rests on a few verified gateway facts and on byte-level fidelity for files the gateway writes, so the order is capture-first and read-before-write. The wire client (20) and the offline renderer (21) share nothing and can run in parallel; every later phase needs one or both. Transport, `plan`, `pull` and master `verify` (22) come before any write, so the pipeline proves "pull, render, plan is empty" while the only thing at risk is a read. Both transport halves are written in 22 as a pair, so the write path is built and tested once even though no verb calls it until 23. All write logic lands in 23 on a single ephemeral gateway, where fault injection is cheap; rollback and auto-rollback reuse the apply engine instead of a second code path. Pair semantics (24) need their own two-node fixture, and a backup failure must never roll back a good master, so they come after the master path is trusted. Project deploy (25) reuses the existing import/export machinery and needs only 23's run store, so it may overlap 24. Surface parity (TUI rows, README and exit-table rows, guarded-ops markers, the secret canary) lands with each verb, never as a final sweep: the coverage and guarded-ops drift tests fail otherwise.

**Decisions in force** (recorded in REQUIREMENTS.md at scoping, 2026-10-10):

- **A — verdict envelope:** post-write red outcomes keep the frozen envelope: `ok:true`, `data.ok=false`, verdict slug in `data`, exit 6. Pre-write refusals stay `CoreError` envelopes.
- **B — staging location: OPEN.** Resolved in Phase 22 from capture C6 and recorded before Phase 23 is planned: (i) home-relative staging outside `data/config/` (needs a spec §9 amendment) if C6 confirms write access and no collection-read risk, otherwise (ii) in-place mirror with the scan-lock covering the transfer.
- **C — plan-hash handshake:** CI and agent callers must pass `--plan-sha256`; it is optional interactively.
- **D — MCP exposure:** `apply`, `rollback` and `projects` are not MCP tools in v1.6; `render`, `plan`, `pull` and `verify` are.
- **Scope:** bootstrap (`ign deploy pull`) is in; the spec §13.5 core strip is not (SF-5). Also in: the `--base` drift gate, verify tag probes, the `.gwbk` snapshot default-on, the Designer preflight.
- **Spec correction:** rendered JSON preserves source key order. Spec §4 "sorted keys" is falsified by the 183-file corpus.

**Capture discipline:** each phase records its "Capture first" items in a `*-LIVE-CAPTURES.md` doc before the code that depends on them. Nothing still open is stated as fact in a success criterion. S5/C9 (a child project reading an inherited resource after its parent is overwritten) is OPEN. The Windows transport behaviors (sftp path form, quoting, key ACLs, the stdin hang, robocopy, sharing violations) are INFERRED until C7a and C7b are captured.

- [x] **Phase 20: Config API Client** — Library-only wire client for config resources, scan-lock, scan and deployment modes, with capture-proven success predicates *(DAPI-01..07)* (completed 2026-10-12)
- [x] **Phase 21: Deploy Render** — `ign deploy render`: manifest plus mode overlay rendered to the gateway's on-disk layout; byte-faithful, deterministic, secret-safe and Windows-safe *(DRND-01..08, DSURF-01)* (completed 2026-10-13)
- [ ] **Phase 22: Transport, Plan, Pull & Verify** — Both transport halves (system OpenSSH and local filesystem); read-only `ign deploy plan`, `pull` and master `verify`; the verdict envelope *(DPLN-01..09, DSURF-04)*
- [ ] **Phase 23: Guarded Apply & Rollback** — Single-gateway `ign deploy apply` and `rollback` under the scan-lock, with snapshot, auto-rollback, on-box journal and run store *(DAPL-01..10, DSURF-02, DSURF-03)*
- [ ] **Phase 24: Redundant Pair** — Backup verified through its own API and file hashes, never written; `deploy_backup_unsynced`; `verify --node both`; two-node live gate *(DPAIR-01..04)*
- [ ] **Phase 25: Guarded Project Deploy** — `ign deploy projects`: pinned, parent-first, exported before overwrite, hash-verified, never deleting an unmanaged project *(DPRJ-01..05)*

**Dependencies:** 20 and 21 are independent and may run in parallel. 22 depends on 20 and 21. 23 depends on 22 and on Decision B being recorded from capture C6. 24 depends on 23. 25 depends on 23's run store and guarded gate (and on 21's manifest parsing); it may overlap 24.

## v1.3 Rig Modules & Declared Discovery (Phase Details)

### Phase 15: 15-module-artifact-fetch-verify

**Goal**: `ign` can obtain a specific, signed Ignition module artifact and prove it is the right bytes before anything else in the milestone is allowed to depend on it — the only phase that touches a third-party feed, isolated so its failure modes are visible rather than buried inside a Docker flow.
**Depends on**: Nothing (first phase of v1.3)
**Requirements**: RMOD-02, RMOD-03
**Success Criteria** (what must be TRUE):

  1. Given a pinned module version, `ign` resolves that release's signed `.modl` asset and downloads it; an unknown version fails loudly naming the version and the resolved URL, never falling back to "latest"
  2. A downloaded artifact whose sha256 does not match the release's published digest is REFUSED — the bytes are discarded, nothing is cached, and the error names both expected and actual digests. This is a hard failure, never a warning
  3. A verified artifact is cached keyed by version+digest; a second fetch of the same version reuses the cache and makes no network request (provable by a test that fails if a request is issued)
  4. With a populated cache and no network, provisioning proceeds to completion — offline is a supported state, not a degraded one

**Research/Planning flags**: GitHub release asset resolution is new ground for this codebase (first third-party artifact feed). Worth a short research pass on: unauthenticated vs. token-authenticated release-asset fetch against a private/public repo boundary, and whether `reqwest`'s redirect handling needs configuration for GitHub's asset CDN redirect. Streaming-to-disk with incremental hashing (avoid buffering 7.7 MB in memory) is the expected shape. *(Research done 2026-09-20 → `15-RESEARCH.md`, HIGH confidence, repo/asset/redirect/digest facts verified live.)*
**Planner locks** (user decisions + planner calls, binding on plans): LIBRARY-ONLY — no clap surface, no CLI verb (that is Phase 16/RMOD-01); deliverable is an `ignition-core` API proven by `cargo test`. A pinned version re-released upstream with different bytes is REFUSED by default naming both cached and upstream digests with the cached artifact left intact, overridable only by an explicit documented policy value. A SEPARATE `reqwest::Client` from `ReqwestGatewayApi` (whose redirect suppression would swallow the mandatory release-CDN 302). NO new crates — `sha2` is promoted from `ignition-cli` dev-deps into `ignition-core` `[dependencies]`, the `tempfile` 05-02 precedent. Five new `CoreError` slugs, every one at an EXISTING exit code: `module_feed_unreachable` (4), `module_feed_unusable` (6), `module_release_not_found` (6), `module_digest_mismatch` (6), `module_digest_changed` (6). Cache = `<cache_dir>/modules/<id>/<version>-<sha256hex>.modl`, `IGNITION_CLI_CACHE` override, lookup keyed by version alone so a hit never needs the network.
**Plans:** 2 plans
Plans:

- [ ] 15-01-PLAN.md — Tracer slice (resolve → 302 → stream+hash → verify → atomic persist) + module registry seam + cache authority (SC-3/SC-4) + loud input refusals (SC-1)
- [ ] 15-02-PLAN.md — Digest-mismatch hard refusal that caches nothing + published-size cap (SC-2), upstream digest-drift refusal with its explicit override, opt-in live fetch of the real `Git-2.3.4-signed.modl`

### Phase 16: 16-compose-override-module-injection

**Goal**: A module the user asked for is actually loaded by the rig's gateway, durably across recreates, without `ign` ever editing a file the user owns — and the mechanism is a module registry rather than a Git-module special case.
**Depends on**: Phase 15 (nothing to inject without a verified artifact)
**Requirements**: RMOD-01, RMOD-04, RMOD-05, RMOD-06, RMOD-07
**Success Criteria** (what must be TRUE):

  1. A rig with no module declaration behaves byte-identically to today — no override generated, no env vars added, no behavior change (regression-proven against existing rig tests)
  2. A rig with a declared module comes up with that module LOADED and running on a stock `inductiveautomation/ignition` image — no custom image, no developer mode, verified on a live rig
  3. The module survives `ign rig down` followed by `ign rig up` — the mount is declared in the override, not copied into a container (the recreate is the test)
  4. `ign` never writes to the user's compose file; the override is a separate `ign`-owned file carrying a generated-by header, and deleting it reverts the MOUNT *(amended 2026-09-25 after the live gate falsified the original wording: Ignition installs an accepted module into its data directory, which outlives the mount, so a gateway that already loaded a module keeps it. Uninstalling from the gateway is phase 16-04.)*
  5. A second, different module registers and provisions through the same seam with no Git-module-specific code path — the registry abstraction is proven, not asserted

**Research/Planning flags**: RESOLVED during planning. Compose merge semantics verified live (`docker compose config`, Compose v5.1.2): `volumes:` lists APPEND across `-f` files and `environment:` maps merge per key with the later file winning — the override is safe as a purely additive second `-f`. Second module chosen by the user: `bw-design-group/ignition-project-scan-endpoint` v1.0.0.
**Planner locks** (user decisions + planner calls, binding on plans): Config is a MAP of modules per rig (`[rigs.NAME.modules.<id>] version = "..."`), not one-module-per-rig. `ModuleSpec` gains `gateway_module_id` as its own field — verified from each artifact's `module.xml`: `com.axone_io.ignition.git` (reverse-DNS, requires 8.3.1) vs `project-scan-endpoint` (bare slug, requires 8.3.0) — because nothing derivable from the registry slug produces both. The `0600` cache-permission defect is already FIXED and merged (PR #13, a2e5299); do not re-plan it. "Module loaded" is proven by the EXISTING `GatewayApi::modules` surface (`/data/api/v1/modules/healthy` + `/quarantined`) — no new client code. Override writer is hand-rolled (no maintained serde-derive YAML crate exists); Compose LONG mapping volume syntax with SINGLE-quoted paths, never the short colon form (Windows drive letters) and never double quotes (backslash escapes). Tri-state override lifecycle: declared → write whole, none declared → delete. `config_args` (the resolve step) NEVER receives the override. One shared `-f` helper for all four builders, override always SECOND. ONE new slug: `module_not_registered` (exit 3); every pre-flight refusal reuses `rig_error` (exit 7). Every plan enumerates CI's FIVE gates verbatim (`.planning/WINDOWS.md` entry 4).
**Plans:** 3 plans
Plans:

- [ ] 16-01-PLAN.md — Tracer slice (config declaration → fetch → pre-flight → generated override → second `-f` on `up`) + bind-mount refusals and the delete-on-undeclare half + one `-f` helper across all four builders with the down→up durability proof (SC-1, SC-3 mechanism, SC-4)
- [ ] 16-02-PLAN.md — Register `project-scan-endpoint` and prove the seam has no Git-module branch (SC-5 registration) + `--with-module ID@VERSION` on `rig up`/`reset`, `provisioned_modules` in the `--json` envelope, Phase 15's `FetchPolicy` flags, README docs
- [ ] 16-03-PLAN.md — Live-rig gate on a stock image with a fixture compose that accepts NOTHING: both modules in the gateway's healthy list and neither quarantined, survival across `down`/`up`, and revert-on-delete observed (SC-2, SC-3, SC-5 live)

### Phase 17: 17-git-module-commissioning

**Goal**: The Git module arrives configured, not just installed — `ign` generates its commissioning file from declared config, keeps every credential out of that file, and refuses configurations that are known to corrupt a gateway.
**Depends on**: Phase 16 (the override carries the generated `git.yaml` mount)
**Requirements**: GITM-01, GITM-02, GITM-03, GITM-04
**Success Criteria** (what must be TRUE):

  1. From declared config, `ign` emits a valid `git.yaml` the module accepts — proven by a rig that commissions its declared projects end-to-end, not merely by schema match
  2. No generated file on disk ever contains a credential; the git secret reaches the gateway only via `GATEWAY_GIT_USER_SECRET` or `GATEWAY_GIT_USER_SECRET_FILE`, resolved from keyring with env fallback. A test asserts `user_password` is never emitted under any input
  3. A configuration where zero or two-plus projects claim `gateway_exportResources` is REFUSED before anything is written, naming the offending projects — the failure mode this prevents (competing projects each exporting their own copy of the same gateway-scoped resources) is the upstream bug the test rig was built to reproduce
  4. After provisioning, `ign` reports the human-owned remaining steps (repo access, credential provisioning, first Designer connection) explicitly rather than implying completion

**Research/Planning flags**: The `git.yaml` schema is source-verified against `ProjectConfig.java` and both existing rig configs; low unknowns. The secret-precedence behavior (`GATEWAY_GIT_USER_SECRET` before `_FILE`) is read from `GitCommissioningUtils.java:437` and should be confirmed live once. Skip a full research pass.

### Phase 18: 18-declared-rig-discovery

**Goal**: Rig selection becomes something the user declared rather than something the binary guesses — the hardcoded WhiskeyHouse directory layout leaves the shipped artifact without stranding anyone who relied on it.
**Depends on**: Nothing (independent of 15-17; may run in parallel or slip without blocking module work)
**Requirements**: RDISC-01, RDISC-02, LEDG-01
**Success Criteria** (what must be TRUE):

  1. No hardcoded home paths or convention repo names remain in the shipped binary — `WHK_HOME_ROOTS` and the repo relpath consts are gone, grep-proven, and the WHK reference count in `crates/` drops accordingly
  2. Roots declared in `[rig]` config produce the same discovery outcome the hardcoded consts did — a user who declares their two roots once loses no convenience
  3. A user with no declared roots and no other rig configuration gets a clear exit-7 error naming every way to configure a rig — never a silent scan and never a scan of a directory they did not name
  4. `MILESTONES.md` records v1.2's shipped work with its requirements archived, so the ledger is continuous from v1.0 through v1.3

**Research/Planning flags**: None — the discovery code is fully source-verified (`crates/ignition-core/src/rig/mod.rs`, five-level chain, `IGNITION_RIG_ROOTS` override already exists as the seam). Pure refactor plus config schema addition. Skip research. Note the existing `IGNITION_RIG_ROOTS` env var already does most of SC-2's job and may simply be promoted to config rather than replaced.

### Phase 19: 19-module-uninstall-on-undeclare

**Goal**: A module the user stops declaring is actually gone from the gateway — not merely unmounted — so the override's removal means what Phase 16's documentation originally promised and now cannot deliver.
**Depends on**: Phase 16 (the provisioning seam, the registry, and `provisioned_modules`)
**Requirements**: RMOD-06 (amended half)
**Success Criteria** (what must be TRUE):

  1. A gateway that has a module installed, whose rig no longer declares it, ends with that module ABSENT from the gateway's healthy module list — proven live, since no unit test can observe a gateway's installed-module state (this is exactly how the original claim survived to ship)
  2. Removal NEVER happens without explicit confirmation: the uninstall path is in `GUARDED_OPS` and refuses without `--yes` (exit 2, `confirmation_required`), following the `project delete` precedent
  3. `rig up` on a rig with no module changes performs no uninstall and issues no uninstall request — provable by a test that fails if a request is made
  4. A module the user never declared through `ign` is never uninstalled — `ign` removes only what it provisioned, and the test seeds a gateway-installed module `ign` did not place to prove it

**Outcome (2026-09-28):** all four proven. SC-1 and SC-4 LIVE on `inductiveautomation/ignition` 8.3.3 (112s) and 8.3.9 (98s): 31 modules healthy before the uninstall, both registry modules ABSENT after, and all 29 stock modules `ign` did not place still healthy. The seed's non-emptiness is asserted, so SC-4 cannot pass vacuously. Transcripts in `19-03-SUMMARY.md`.
**Research/Planning flags**: The endpoint is confirmed present (`DELETE /data/api/v1/modules/uninstall`, body `{"uninstall": [...]}`, 83-api collection) but its response shape, whether it requires a gateway restart to take effect, and whether it accepts gateway module ids or some other handle are ALL unverified. A short live probe against a disposable rig should settle those before the plan locks a client signature.
**Planner locks** (user decision, 2026-09-25, binding): **An uninstall REQUIRES confirmation.** It is the feature's first destructive gateway write, and undeclaring a module in config is a quiet way to trigger one — someone editing a TOML table should not lose a gateway module as a side effect. The uninstall joins `GUARDED_OPS` (`crates/ignition-cli/src/main.rs:3425`) and refuses without `--yes`. `rig up` itself stays UNGUARDED: the gate belongs on the destructive act, not on bringing a rig up.
**Open for the planner** (design, not policy): whether an undeclared module is uninstalled at all as part of `rig up`, or whether `rig up` merely REPORTS the orphaned module and removal is a deliberate verb (`ign rig module uninstall <id> --yes`). The orchestrator's view is that the second is safer — a report makes removal something the user asks for, rather than something they are asked to approve mid-`up` — but the planner should weigh both and record why.
**SETTLED — planner decision D-19-01 (2026-09-26, binding on plans):** `rig up` **REPORTS** the orphan (a new additive `orphaned_modules` key) and makes zero gateway writes; removal is the deliberate guarded verb `ign rig module uninstall <REGISTRY-ID> --yes`. Four reasons, in order of force: (1) `--yes` is a GLOBAL arg and `apply_env_defaults` merges `IGNITION_YES`, so the documented agent/CI pattern `IGNITION_YES=1` would turn a mid-`up` confirmation into a no-op — a gate the project's own recommended usage disables is not a gate; (2) `GUARDED_OPS` is a static registry keyed by clap leaf path and `mcp.rs` derives each tool's `confirm` property from it, so a conditionally-destructive `rig up` is structurally inexpressible; (3) `rig up` needs NO gateway credential today (the commissioned wait is header-less against `/StatusPing` on purpose) and a gateway write would change that contract for every user; (4) live fact 3 — an uninstall is not undone by re-declaring, so recovery means `ign rig reset` and a destroyed data volume, and every destructive verb in this codebase is already a deliberate separate command. **Rejected:** inline uninstall behind the confirmation, on (1) and (2) as structural grounds with (3) and (4) independent. Full record in `19-01-PLAN.md`'s `<design_decision>`.
**Plans:** 3 plans
Plans:

- [ ] 19-01-PLAN.md — The guarded gateway write end to end: `CoreError::ModuleUninstallDenied` (denial-rides-200, exit 6), `GatewayApi::uninstall_module` (one id per call), the `rig module uninstall` leaf in `GUARDED_OPS`, and the three refusals — no `--yes` (exit 2 before discovery), unregistered id (exit 3, zero requests), still-declared id (exit 7, zero requests) *(SC-2, SC-4's CLI-surface half)*
- [ ] 19-02-PLAN.md — Orphan detection and reporting: `previously_provisioned` parses the PRE-overwrite override's mount targets and intersects them with the registry, `orphaned_modules` is a pure diff, `undeclared_provisioning` reads before it deletes, and `rig up`/`rig reset` report orphans as data while a fake gateway that panics on `uninstall_module` proves no write happens *(SC-3, SC-4's on-disk half)*
- [ ] 19-03-PLAN.md — The live gate: §C stops hand-deleting the override and drives the real undeclare path, §D uninstalls both ghost entries through `ign`'s own action and proves them ABSENT from the gateway's healthy list, §E proves a non-empty set of gateway-installed modules `ign` did not place is untouched, on two pinned 8.3.x images *(SC-1 live, SC-4 live)*

## v1.6 Declared Gateway Deploy (Phase Details)

### Phase 20: Config API Client

**Goal**: The core library speaks the 8.3.6 config-resource, scan-lock, scan and deployment-mode APIs with success predicates taken from live captures, so every later deploy verb builds on wire truth rather than on HTTP status codes.
**Depends on**: Nothing (first v1.6 phase; runs in parallel with Phase 21)
**Requirements**: DAPI-01, DAPI-02, DAPI-03, DAPI-04, DAPI-05, DAPI-06, DAPI-07
**Success Criteria** (what must be TRUE):

  1. Against an ephemeral 8.3.6 gateway, the library lists and reads config resources by type and name from the effective view and from a named collection, creates them by POST and updates them with the signature-checked array PUT; a stale or missing signature comes back as its own typed error, never a generic failure
  2. The library acquires the scan-lock, reads its remaining time while held, and detects expiry from the response change alone (200 JSON while held, 204 after); a test with a deliberately skewed local clock still decides correctly. A lock held by another actor surfaces as `scan_lock_held` naming the holder
  3. A config scan counts as complete only when `scanActive` is false and `lastScanTimestamp` has advanced on the same gateway; a contract test in which the trigger returns 200 but the status never advances reports the scan as not complete
  4. The library lists and creates deployment modes and reports the active `deploymentMode`, reading `null` and `""` both as "no mode"; redundancy status names the active node by activity level using the live `Master`/`Backup` vocabulary, and the `redundancy.rs` doc no longer says `Primary`
  5. Every new endpoint has a declared response shape (JSON, empty 2xx or 204) recorded in `20-LIVE-CAPTURES.md` before the code that depends on it and pinned by a wiremock contract test; any gateway body echoed into an error is size-capped

**Capture first**: C1 (scan-lock 409 body; held GET with `remaining` and actor vs expired 204; whether any release/DELETE route exists, grepped from the 617-route OpenAPI; scan/config status JSON and the completion rule); C3a (stale or omitted signature PUT: 422 vs 500 "signature mismatch"; add a message-based error arm only if the two are distinguishable); the `null` vs `""` `deploymentMode` reading on a no-mode gateway
**Constraints**: Library-only: no clap surface, no verb. A new `DeployApi` trait rather than new `GatewayApi` methods (avoids editing 15 hand-written `unreachable!()` doubles and allows one stateful in-memory fake gateway). Do not enable `serde_json/preserve_order` or `arbitrary_precision` workspace-wide; feature unification would reorder frozen-contract payloads. Every new slug follows the four-place rule. Live gate: one ephemeral 8.3.6 with a bind-mounted `resources/`; no pair rig.
**Plans**: 6/6 plans executed (waves: 1 = 20-01 ∥ 20-02, 2 = 20-03, 3 = 20-04, 4 = 20-05, 5 = 20-06)

Plans:
**Wave 1**

- [x] 20-01-PLAN.md — Live 8.3.6 captures (C1a–C1j, C3a, C-null, C-mode, C-list), Decisions D1–D12, Endpoint shapes table, reusable loopback rig fixture
- [x] 20-02-PLAN.md — `scan_lock_held` + `resource_signature_rejected` in ONE early commit (Phase 21 rebases onto it); `active_node` by activityLevel; Master/Backup vocabulary sweep

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 20-03-PLAN.md — Tracer: held scan-lock → `scan_lock_held` through `DeployApi` end to end; scan completion (`scanActive` false + timestamp advanced)

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 20-04-PLAN.md — Config resources: list/names/find/singleton (effective view + named collection); POST create, signature-checked array PUT update, typed stale/missing refusal; capped 422 echo

**Wave 4** *(blocked on Wave 3 completion)*

- [x] 20-05-PLAN.md — Deployment modes + normalized `deploymentMode`; 13-row endpoint closure; FakeDeploy + shared differential scenario

**Wave 5** *(blocked on Wave 4 completion)*

- [x] 20-06-PLAN.md — Live gate against the ephemeral 8.3.6 rig; live trace == fake trace

### Phase 21: Deploy Render

**Goal**: A user can turn a `gateways/<name>/` tree into the exact bytes the gateway would hold on disk for one environment, offline and deterministically, and render refuses anything that would leak a secret, lock out an administrator, or break on a Windows host.
**Depends on**: Nothing (offline; runs in parallel with Phase 20)
**Requirements**: DRND-01, DRND-02, DRND-03, DRND-04, DRND-05, DRND-06, DRND-07, DRND-08, DSURF-01
**Success Criteria** (what must be TRUE):

  1. `ign deploy render` on the same tree and environment yields the same `render_sha256` on every run; a `manifest.yaml` or `targets/<env>.yaml` with an unknown field or a wrong `schema_version` fails as `deploy_tree_invalid` naming file, line and column
  2. A file under `modes/<env>/` replaces its base-tree counterpart whole-file by path; only manifest-allowlisted resource types are rendered, and a tree that declares security-properties, security-levels, an API token or a user source is refused
  3. Gateway-written files pass through byte-for-byte: the 183-file gateway corpus round-trips byte-identical on Linux, macOS and Windows. Files `ign` synthesizes come from a Gson-compatible writer that keeps source key order. A new named resource gets a deterministic name-based `attributes.uuid`, a singleton type gets none, and an existing uuid is kept
  4. Render refuses an `Embedded` secret or a plaintext secret field as `embedded_secret`, naming the path and never the value, and refuses case-fold collisions, Windows-illegal names, paths over the Windows length budget and CR bytes anywhere in the tree
  5. `deploy render` lands with its TUI route row, README entry and exit-table rows, its new slugs pass the four-place rule, and a family coverage test fails if any `deploy` leaf ships without them; every later verb phase keeps this test green

**Capture first**: C4 (named, uuid-bearing vs singleton resource types: census from the prod `core` bootstrap, feeding the uuid rule table); C5 (hostile-name encoding in tag and resource directories, covering `:`, `?`, trailing dot, case collision, spaces and commas, against WHK01's 637 tag files); C3b (whether a stale or omitted `lastModificationSignature` loads, is rewritten or is flagged, and whether the gateway rewrites a hand-written `resource.json` after load); OQ1 (gateway-written bytes of `tags.json`, `udts.json` and `unary-resource.json`; the spike fixtures are synthetic); OQ4 (repo-path to collection-path mapping for tag-definition nesting and name depth); OQ8 (exact 8.3.6 JSON shape of Embedded vs Referenced secret fields). Also check `cargo tree` for `sha1` before choosing the uuid derivation.
**Constraints**: Spec correction: preserve source key order. Spec §4 "sorted keys" is wrong; use an order-preserving node tree, Gson HTML-safe escaping, 2-space indent and no trailing newline, and rewrite a gateway file only when `write(parse(bytes)) == bytes`, otherwise copy it opaque. YAML via `yaml_serde` 0.10 (compiles on the 1.88 floor; not serde-saphyr); no `uuid` crate. The manifest and target schemas include `verify.tag_probes[]` (SF-8) and the target `ssh` block (SF-10) now, so strict parsing does not reject them in Phase 22. Overlays are add/override only; "absent in env" is documented, not built (SF-7). The render golden runs on `windows-latest`, and fixture `gateways/` trees carry no `\r`. `render` reaches the MCP catalog by clap derivation (Decision D).
**Plans**: 9/9 plans executed (waves: 1 = 21-01 ∥ 21-02 ∥ 21-03, 2 = 21-04, 3 = 21-05, 4 = 21-06, 5 = 21-07, 6 = 21-08, 7 = 21-09)

Plans:
**Wave 1**

- [x] 21-01-PLAN.md — Sanitized 183-file corpus (public-repo safe, stand-in jars with CR for D-01), OpenAPI resource-type/secret-field extract, fixture guard test, `-text` gitattributes
- [x] 21-02-PLAN.md — Live captures on the 8.3.6 rig: D-03 v8-uuid load check, C3b, C4, OQ8, OQ1 tag bytes, OQ4 tag mapping, C5 hostile names (Linux only, D-04); Decisions CD1–CD7
- [x] 21-03-PLAN.md — `deploy_tree_invalid` + `embedded_secret` in ONE four-place commit; `yaml_serde` strict parse with file:line:column; manifest/target schemas (SF-8, SF-10)

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 21-04-PLAN.md — Tracer: `ign deploy render --env` end to end (allowlist + D-02 refusal, `render_sha256` golden), TUI row, README, deploy family coverage test, MCP exposure

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 21-05-PLAN.md — Order-preserving Node + Gson writer, identity gate (gateway-format vs opaque), corpus byte-fidelity on all OSes

**Wave 4** *(blocked on Wave 3 completion)*

- [x] 21-06-PLAN.md — `modes/<env>/` overlay, `tags/` mapping (CD5), deny security singletons, refuse `config-mode.json` and dot-names (`.gitkeep` skipped)

**Wave 5** *(blocked on Wave 4 completion)*

- [x] 21-07-PLAN.md — Evidence-backed type rule table and sha2 version-8 name-based `attributes.uuid` (D-03, live vector CD1)

**Wave 6** *(blocked on Wave 5 completion)*

- [x] 21-08-PLAN.md — Lints: `embedded_secret` (paths only, canaries), case-fold, Windows names, path budget, CR in text members only (D-01)

**Wave 7** *(blocked on Wave 6 completion)*

- [x] 21-09-PLAN.md — `--out` clobber ladder + `render.json` marker (never deletes); phase closure: five CI gates, installed-binary smoke

### Phase 22: Transport, Plan, Pull & Verify

**Goal**: A user can see exactly what a deploy would change on the live master, bootstrap a declared tree from a running gateway, and verify a gateway against its tree, all read-only, over a transport whose write half is built and tested here but not yet called by any verb.
**Depends on**: Phase 20 (wire client), Phase 21 (rendered tree)
**Requirements**: DPLN-01, DPLN-02, DPLN-03, DPLN-04, DPLN-05, DPLN-06, DPLN-07, DPLN-08, DPLN-09, DSURF-04
**Success Criteria** (what must be TRUE):

  1. `ign` reads a gateway's config tree over system OpenSSH, proven against a real Windows OpenSSH host, and over the local filesystem, with batch mode, a pinned host key, per-call timeouts, argv-only invocation and client-side file hashes; the write half (a mirror driven by an explicit delete set) passes its tests against a scratch directory, and no verb calls it
  2. `ign deploy plan` against the master makes zero writes and takes no lock, exits 0, and lists every resource as added, changed, removed or same with `data.summary.has_changes` and a `plan_sha256`; each entry carries its `core` relation (none, identical, differs, hidden, shadows_removal) and its destructive and lockout-risk flags, and the plan states the exact delete set (previously declared names minus rendered names), which never contains `config-mode.json` or an undeclared resource
  3. With `--base <previous render>`, plan entries the live gateway changed outside the pipeline are marked as drift; `ign deploy pull` bootstraps a `gateways/<name>/` tree from a live gateway without writing to it, and `render` then `plan` against that same gateway reports no changes
  4. `ign deploy verify` against the master checks that the scan is complete, `deploymentMode` equals the target, every declared resource reads back equal in the effective view, the gateway log has no "will not be loaded" entry for a declared resource, and every `verify.tag_probes[]` tag is Good (an absent probe list is a warning, never a pass); a red verify returns the frozen envelope with findings in `data` and exit 6, documented as a dated README contract exception
  5. `plan`, `pull` and `verify` land with their TUI rows, README entries and exit-table rows (the DSURF-01 coverage test stays green), appear in the MCP catalog, and their output, `plan --diff` included, is already under the secret canary that Phase 23 completes

**Capture first**: C6 (can `svc-ignition-deploy` write a home-relative staging directory outside `data/config/`, and is that directory ever read as a collection; this decides Decision B); C7a (Win32-OpenSSH sftp path form `/C:/` vs `C:/`, quoting, key ACL, the stdin hang, CLIXML on stderr, sftp-server long paths; all INFERRED until captured); OQ5 (real-transport write and scan timings from WFLWVM01 with a prod-sized tree, setting the `holdTimeout` p95 and margin and the Defender-exclusion decision). All three block Phase 23 planning.
**Exit gate before Phase 23 is planned**: Decision B is recorded from C6, and the hold budget measured under OQ5 (p95, floored, ceilinged) replaces the spec's 5 s figure, which came from a local `docker cp`.
**Constraints**: Decision A governs the verify verdict (`ok:true`, `data.ok=false`, slug in `data`, exit 6; verdict slugs never appear in exit tables). `plan` always exits 0; no `--detailed-exitcode` (SF-2). `--base` is the three-way drift input (SF-4); the apply-side drift gate is Phase 23. `pull` is in scope; a core-strip command is not (SF-5). System `ssh`/`sftp` only, with no SSH library crate. One layered seam: `RemoteShell` under `FileTransport` (`SftpTransport`, `LocalFsTransport`). `robocopy /MIR` is rejected; deletes come only from the bounded list. Snapshots and staging never go inside `data/config/resources/`, and nothing dot-named is created there (Designer `NameInvalidException`).
**Plans**: 11/14 plans executed (waves: 1 = 22-01, 2 = 22-02, 3 = 22-03, 4 = 22-04, 5 = 22-05, 6 = 22-06, 7 = 22-07 ∥ 22-08, 8 = 22-09, 9 = 22-10, 10 = 22-11, 11 = 22-12, 12 = 22-13, 13 = 22-14)

Plans:
**Wave 1**

- [x] 22-01-PLAN.md — Tracer: `deploy_plan` end to end at library level over a `local:` node (transport seam, `LocalFsTransport`, effective view + `live_in`, five statuses incl. `unmanaged`, `core` relations, golden `plan_sha256`), `transport_failed`

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 22-02-PLAN.md — `ign deploy plan` verb surface: clap + `main.rs` dispatch (no session), human output, TUI route, MCP tool, stdout purity, README section + `nodes.*.local` row, REQUIREMENTS `unmanaged` note

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 22-03-PLAN.md — Contract-vocabulary checkpoint; ledger v1 + strict reader + `deploy_ledger_invalid`; exact delete set, `removed`, `shadows_removal`, destructive/lockout flags

**Wave 4** *(blocked on Wave 3 completion)*

- [x] 22-04-PLAN.md — `--base` three-way drift on a re-hashed render (closes R-03), `--diff` with secret withholding, plan secret canary

**Wave 5** *(blocked on Wave 4 completion)*

- [x] 22-05-PLAN.md — `ProcessRunner` (timeouts, kill-on-drop) and `RemoteShell` (hardened argv via `ssh -G`, repo `known_hosts` pin, `key_env` temp key, timeouts); target ssh fields

**Wave 6** *(blocked on Wave 5 completion)*

- [x] 22-06-PLAN.md — `SftpTransport` read half (existence-first, client-side hashes) proven with `sftp -D`; plan parity with the local transport

**Wave 7** *(blocked on Wave 6 completion)*

- [x] 22-07-PLAN.md — `TreeMirror` write half (stage/commit, explicit delete set, both staging modes) on scratch dirs; `write_half_unreachable`
- [x] 22-08-PLAN.md — `ign deploy pull` (core→base, mode→overlay, render fixpoint, Embedded skip, `--yes` guard, GUARDED_OPS + MCP confirm); pull→render→plan empty

**Wave 8** *(blocked on Wave 7 completion)*

- [x] 22-09-PLAN.md — Rig captures CV1–CV3 (log shape/window, read-back equality, post-scan bytes); live pull→render→plan empty on 8.3.6

**Wave 9** *(blocked on Wave 8 completion)*

- [x] 22-10-PLAN.md — Verify core: six-check engine, read-only `deploy_verify` gathering, `ForbidWrites`, verdict slug + `failure_exit_code` (exit 6), live rig verify green then red

**Wave 10** *(blocked on Wave 9 completion)*

- [x] 22-11-PLAN.md — `ign deploy verify` verb surface: profile resolution, `main.rs` exit-6 hook, TUI route, MCP tool, dated README verdict exception (DSURF-04), five CI gates + installed-binary smoke

**Wave 11** *(blocked on Wave 10 completion)*

- [ ] 22-12-PLAN.md — WFLWVM01 provisioning (human), captures C7a, C6, C5-NTFS; Decision B checkpoint with the ledger location

**Wave 12** *(blocked on Wave 11 completion)*

- [ ] 22-13-PLAN.md — Windows transport code from C7a (path form, rename, PowerShell `-EncodedCommand`), Decision B ledger location; live proof on WFLWVM01 (SC-1)

**Wave 13** *(blocked on Wave 12 completion)*

- [ ] 22-14-PLAN.md — OQ5 timings with and without a Defender exclusion, hold budget, D-04 defaults revisited, exit gate recorded in STATE/ROADMAP

### Phase 23: Guarded Apply & Rollback

**Goal**: A user can apply a reviewed plan to a single gateway and end with either a verified deploy or the snapshot restored, never a partially loaded tree, and can roll a past run back from a fresh process.
**Depends on**: Phase 22 (transport, plan, verify), with Decision B recorded
**Requirements**: DAPL-01, DAPL-02, DAPL-03, DAPL-04, DAPL-05, DAPL-06, DAPL-07, DAPL-08, DAPL-09, DAPL-10, DSURF-02, DSURF-03
**Success Criteria** (what must be TRUE):

  1. `ign deploy apply` without `--yes` refuses and prints the plan as the refusal; with `--plan-sha256`, a plan that changed since review refuses as `plan_stale`; destructive entries need `--allow-destructive`, drift entries need `--allow-drift "<reason>"` (recorded in the run), and `shadows_removal` entries block regardless of flags; an empty plan exits 0 with no lock, snapshot or scan
  2. Before any write, apply runs every preflight (`deploymentMode` on both nodes, wrong-target guard, write-permission probe, no on-box journal, open Designer sessions under `--designers fail|wait|proceed`, default fail) and reports all failures in one refusal; it then snapshots the target collection on the gateway host and in the run directory with hashes, and downloads a `.gwbk` unless `--skip-gwbk` (recorded); neither is ever uploaded off-prem
  3. Apply writes inside the scan-lock within the hold budget measured in Phase 22; fault-injection tests that expire the lock or fail a write mid-way end with the collection equal to the snapshot and scanned, never a partial tree loaded. A master verify failure after apply rolls back automatically through the same engine and reports `deploy_rolled_back` with exit 6; a failed rollback reports `rollback_failed` with recovery guidance
  4. `ign deploy rollback --run <id>` from a fresh process restores that run's snapshot, and refuses when live state differs from the run's post-apply fingerprint unless `--discard-live-changes`. An apply killed mid-write leaves an on-box journal, and `plan`, `apply` and `rollback` refuse until recovery runs. Every run, failed ones included, writes a private run directory (`run.json`, `result.json`, `plan.json`, `events.ndjson`), and apply reports `restart_pending` without restarting or failing over a gateway
  5. `apply` and `rollback` are in `GUARDED_OPS` and absent from the MCP catalog while `render`, `plan`, `pull` and `verify` remain in it; a secret canary proves no secret value appears in plan, apply, rollback or verify output, `--json`, run-directory files or events. Phases 24 and 25 keep this canary and the DSURF-01 coverage test green

**Capture first**: C2 (does the scan-lock block UI, Designer and API writes to the active mode collection; if not, the Designer preflight is the only guard against a write racing the apply); C7b (ACL after copy vs rename, sharing violation on a loaded JDBC jar under `ignition/database-driver/`, Defender cost, long paths; INFERRED until captured); C11 (single-scan ordering when a provider and its definitions arrive together)
**Constraints**: Decision B must be recorded in Phase 22 before this phase is planned; the write flow and hold budget depend on it. Decision A for verdicts (`deploy_rolled_back` in `data`, exit 6, never an exit-table row). Decision C: CI and agent callers must pass `--plan-sha256`. Decision D: `apply` and `rollback` are not MCP tools in v1.6. The `.gwbk` default-on (SF-6) and the Designer preflight are in scope. Apply reports restarts and never performs them (SF-3). No `--force`, `--skip-verify` or `--no-rollback`. One `restore_snapshot(run)` serves abort, auto-rollback and manual rollback. Lock decisions use the gateway's `remaining` and a monotonic local clock, never gateway timestamps against the runner clock. Built on a single ephemeral 8.3.6 with `LocalFsTransport`, where fault injection is cheap; the both-nodes mode preflight is exercised against the stateful in-memory fake, and backup checks and backup-lag handling belong to Phase 24.
**Plans**: TBD

### Phase 24: Redundant Pair

**Goal**: A deploy to a redundant pair ends with proof that the backup holds the same config as the master, checked through the backup's own API and files and never by writing to it, and a backup that did not converge is reported without undoing a good master deploy.
**Depends on**: Phase 23 (apply engine, run store, master verify)
**Requirements**: DPAIR-01, DPAIR-02, DPAIR-03, DPAIR-04
**Success Criteria** (what must be TRUE):

  1. After a master-verified apply, `ign` checks the backup through its own API and file hashes (same `deploymentMode`, effective view equal to the master's, redundancy healthy) after a bounded convergence wait and at most one resync, and sends no write to the backup
  2. A backup that stopped syncing or still differs after that wait reports `deploy_backup_unsynced` with exit 6, and the master is not rolled back
  3. `ign deploy verify --node both` runs the backup checks on demand and fails when a secret name the tree references is missing on either node
  4. A two-node 8.3.6 rig with a trusted gateway-network link live-gates the three behaviors above, including at least one run that ends `deploy_backup_unsynced`; the DSURF-01 coverage test and the DSURF-03 canary stay green with `--node both` and the backup verdicts added

**Capture first**: C12 (does a lagging backup recover through `resync` alone after "failed 5 times"; decides whether one resync is enough before reporting unsynced); GAN trust recipe on 8.3.6 (certificate move plus `toggleApproval`, from `s3_redundancy.sh`) for the pair fixture
**Constraints**: Any write to the backup is out of scope; its state comes from redundancy sync. Decision A for `deploy_backup_unsynced` (payload slug, exit 6). The active node is decided by activity level, never by role; `uptime: -1` or `Cold` on a healthy backup is normal. Secret values do not replicate between nodes (ENT-3115), so only names are checked. A `failover` library method may exist, but no verb calls it, and nothing restarts inside apply (SF-3). A backup project-sync failure (TEC-5105) gets its own finding and never triggers a master rollback. May run in parallel with Phase 25. Wire `ign redundancy status` to the Phase 20 library `active_node` (activity level, Master/Backup vocabulary); carried from DAPI-07, which closed at library level in Phase 20.
**Plans**: TBD

### Phase 25: Guarded Project Deploy

**Goal**: A user can deploy the manifest-pinned project set to a gateway (verified before upload, imported parents first, exported before overwrite and hash-verified after) without ever deleting a project the manifest does not name.
**Depends on**: Phase 23 (run store and guarded gate) and Phase 21 (manifest parsing); may overlap Phase 24
**Requirements**: DPRJ-01, DPRJ-02, DPRJ-03, DPRJ-04, DPRJ-05
**Success Criteria** (what must be TRUE):

  1. `ign deploy projects` checks every project artifact against its manifest sha256 pin before any upload; a mismatch refuses as `project_hash_mismatch` with nothing uploaded
  2. Projects import parent-first from `project.json` inheritance; a cycle or an unknown parent refuses before any import, and a project whose canonical hash already matches is skipped
  3. Each project is exported before it is overwritten and, after import, verified by canonical hash with `lastModification*` and signatures ignored
  4. A project absent from the manifest is reported and never deleted, a project with `.git` present is refused, and `ign deploy projects --check` reports what would change with zero writes
  5. `projects` joins `GUARDED_OPS` and stays out of the MCP catalog under the Phase 23 rule, lands with its TUI row, README entry and exit-table rows, and its output and run files pass the secret canary

**Capture first**: C8 (does `import?overwrite=true` delete resources absent from the zip; does export-after-import equal the zip once `lastModification*` is normalized); C9 (does a child read an inherited resource after its parent is overwritten, and what is the session impact; S5 residual, OPEN, and no criterion here claims it); C10 (can the prod git module's `git.yaml`/commissioning recreate a stripped `core` or revert API imports)
**Constraints**: Decision D: `projects` is not an MCP tool in v1.6. Deleting projects absent from the manifest is out of scope. Reuse `MemberSource::Zip` (projects only), `project_import`'s 200-denial check, `project_export_to_file` and a promoted `validate_import`. Designer/session pre-check before import. Cutover prerequisite (human work, not code): spec §13 step 8, removing `.git` and uninstalling the git module on the target, comes before unattended project deploys; `ign` never deletes `.git`.
**Plans**: TBD

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Foundation & Agentic Contracts | v1.0 | 4/4 | Complete | 2026-08-21 |
| 2. Gateway Health & Inspection | v1.0 | 5/5 | Complete | 2026-08-22 |
| 3. Project Operations | v1.0 | 3/3 | Complete | 2026-08-22 |
| 4. Rig Lifecycle & Trial State | v1.0 | 4/4 | Complete | 2026-08-23 |
| 5. WebDev Backend & Tag Operations | v1.0 | 8/8 | Complete | 2026-08-26 |
| 6. TUI Cockpit | v1.0 | 11/11 | Complete | 2026-08-28 |
| 7. Ecosystem Interop & Advanced Ops | v1.0 | 6/6 | Complete | 2026-08-29 |
| 8. Foundations — Session/Config/Contract | v1.1 | 6/6 | Complete | 2026-09-06 |
| 9. Agent Surface — API/Diagnostics | v1.1 | 8/8 | Complete | 2026-09-07 |
| 10. EAM Write Operations | v1.1 | 7/7 | Complete | 2026-09-11 |
| 11. Tag Bulk Transfer XML/CSV | v1.1 | 8/8 | Complete | 2026-09-14 |
| 12. TUI Theming & Degradation | v1.1 | 4/4 | Complete | 2026-09-14 |
| 13. Composite Engine — Workspace/Historian/Edit | v1.1 | 8/8 | Complete | 2026-09-15 |
| 14. Transports — MCP/LSP | v1.1 | 6/6 | Complete | 2026-09-16 |
| 15. Module Artifact — Fetch & Verify | v1.3 | 2/2 | Complete | 2026-09-20 |
| 16. Compose Override — Module Injection | v1.3 | 3/3 | Complete (SC-4 amended) | 2026-09-25 |
| 17. Git Module Commissioning | v1.3 | 0/? | Parked behind v1.6 | — |
| 18. Declared Rig Discovery | v1.3 | 0/? | Parked behind v1.6 | — |
| 19. Module Uninstall on Undeclare | v1.3 | 3/3 | Complete (SC-1, SC-4 proven live) | 2026-09-28 |
| 20. Config API Client | v1.6 | 6/6 | Complete    | 2026-10-12 |
| 21. Deploy Render | v1.6 | 9/9 | Complete    | 2026-10-13 |
| 22. Transport, Plan, Pull & Verify | v1.6 | 11/14 | In Progress|  |
| 23. Guarded Apply & Rollback | v1.6 | 0/? | Not started | — |
| 24. Redundant Pair | v1.6 | 0/? | Not started | — |
| 25. Guarded Project Deploy | v1.6 | 0/? | Not started | — |

---
*Roadmap created: 2026-09-04 — milestone v1.1; v1.1 completed 2026-09-16 (25/25 requirements shipped)*
*v1.2 shipped 2026-09-19 outside the milestone workflow (tag `v1.2.0`, PR #8) — no phase structure; ledger backfill tracked as LEDG-01 in Phase 18*
*v1.3 started 2026-09-20 — Phases 15-18, 14 requirements*
*v1.6 started 2026-10-10 — Phases 20-25, 47 requirements; v1.3 Phases 17-18 parked behind it*
