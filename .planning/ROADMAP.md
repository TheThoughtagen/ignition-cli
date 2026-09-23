# Roadmap: ignition-cli

## Overview

Build `ign` — a single Rust binary + ratatui cockpit that replaces both the Ignition gateway webpage and the author's ignition-mcp server as the canonical human/agent interface to Ignition 8.3+ gateways.

**Mode:** mvp — every phase delivers an end-to-end user capability (vertical slice), not a horizontal layer.

## Milestones

- ✅ **v1.0 MVP** — Phases 1-7 (shipped 2026-08-30) — [archive](milestones/v1.0-ROADMAP.md)
- ✅ **v1.1 Agent Surface & IDE Integration** — Phases 8-14 (shipped 2026-09-16) — [archive](milestones/v1.1-ROADMAP.md)
- ✅ **v1.2** — shipped 2026-09-19 outside the milestone workflow (tag `v1.2.0`, PR #8: `ign testing run`, `ign session login`, e2e Playwright scaffold); no phase structure, ledger backfill tracked as LEDG-01
- 🚧 **v1.3 Rig Modules & Declared Discovery** — Phases 15-18 (started 2026-09-20)

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

### 🚧 v1.3 Rig Modules & Declared Discovery (Phases 15-18, In Progress)

**Milestone Goal:** A developer can opt into running an Ignition module on a rig — signed, verified, and commissioned by `ign` — and rig selection becomes something declared rather than guessed.

**Design spec:** [research/2026-09-20-rig-module-provisioning-DESIGN.md](research/2026-09-20-rig-module-provisioning-DESIGN.md) (approved, commit `2fd00c7`)

**Structure rationale:** The artifact path is the risky, external-facing half and everything else depends on it, so fetch-and-verify lands first and alone — it is the only phase touching a third-party feed, and it is provable without Docker. Injection (the override) comes next because it is what makes a fetched artifact actually load, and it closes RMOD-01 end-to-end for a single module. Commissioning config is separable from injection: `git.yaml` generation and the credential/validation rules are pure generation logic over a schema, testable by golden file with no gateway. Discovery is independent of all three — it is the one phase that can slip without blocking module work — so it goes last and carries the small ledger backfill with it.

- [x] **Phase 15: 15-module-artifact-fetch-verify** — Pinned signed-release resolution, sha256 verification, version+digest cache, offline-from-cache *(RMOD-02, RMOD-03)* *(complete 2026-09-20; 2/2 plans, all 4 SCs tested; live fetch confirmed the real Git-2.3.4-signed.modl byte-for-byte)*
- [ ] **Phase 16: 16-compose-override-module-injection** — Generated `compose.ign-modules.yml`, acceptance variables, recreate-durable mount, `ign`-owned regeneration, module registry with a second module proving the seam *(RMOD-01, RMOD-04, RMOD-05, RMOD-06, RMOD-07)*
- [ ] **Phase 17: 17-git-module-commissioning** — `git.yaml` generation, credential kept out via `GATEWAY_GIT_USER_SECRET`/`_FILE`, exactly-one `gateway_exportResources` validation, human-next-steps reporting *(GITM-01, GITM-02, GITM-03, GITM-04)*
- [ ] **Phase 18: 18-declared-rig-discovery** — Convention roots move from binary consts to `[rig]` config; clear error when nothing is declared; v1.2 ledger backfill *(RDISC-01, RDISC-02, LEDG-01)*

**Dependencies:** 16 depends on 15 (nothing to inject without a verified artifact). 17 depends on 16 (the override carries the generated `git.yaml` mount). 18 is independent of 15-17 and may run in parallel or slip.

## Phase Details

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
  4. `ign` never writes to the user's compose file; the override is a separate `ign`-owned file carrying a generated-by header, and deleting it fully reverts provisioning
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
| 16. Compose Override — Module Injection | v1.3 | 0/? | Not started | — |
| 17. Git Module Commissioning | v1.3 | 0/? | Not started | — |
| 18. Declared Rig Discovery | v1.3 | 0/? | Not started | — |

---
*Roadmap created: 2026-09-04 — milestone v1.1; v1.1 completed 2026-09-16 (25/25 requirements shipped)*
*v1.2 shipped 2026-09-19 outside the milestone workflow (tag `v1.2.0`, PR #8) — no phase structure; ledger backfill tracked as LEDG-01 in Phase 18*
*v1.3 started 2026-09-20 — Phases 15-18, 14 requirements*
