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

- [ ] **Phase 15: 15-module-artifact-fetch-verify** — Pinned signed-release resolution, sha256 verification, version+digest cache, offline-from-cache *(RMOD-02, RMOD-03)*
- [ ] **Phase 16: 16-compose-override-module-injection** — Generated `compose.ign-modules.yml`, acceptance variables, recreate-durable mount, `ign`-owned regeneration, module registry with a second module proving the seam *(RMOD-01, RMOD-04, RMOD-05, RMOD-06, RMOD-07)*
- [ ] **Phase 17: 17-git-module-commissioning** — `git.yaml` generation, credential kept out via `GATEWAY_GIT_USER_SECRET`/`_FILE`, exactly-one `gateway_exportResources` validation, human-next-steps reporting *(GITM-01, GITM-02, GITM-03, GITM-04)*
- [ ] **Phase 18: 18-declared-rig-discovery** — Convention roots move from binary consts to `[rig]` config; clear error when nothing is declared; v1.2 ledger backfill *(RDISC-01, RDISC-02, LEDG-01)*

**Dependencies:** 16 depends on 15 (nothing to inject without a verified artifact). 17 depends on 16 (the override carries the generated `git.yaml` mount). 18 is independent of 15-17 and may run in parallel or slip.

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
| 15. Module Artifact — Fetch & Verify | v1.3 | 0/? | Not started | — |
| 16. Compose Override — Module Injection | v1.3 | 0/? | Not started | — |
| 17. Git Module Commissioning | v1.3 | 0/? | Not started | — |
| 18. Declared Rig Discovery | v1.3 | 0/? | Not started | — |

---
*Roadmap created: 2026-09-04 — milestone v1.1; v1.1 completed 2026-09-16 (25/25 requirements shipped)*
*v1.2 shipped 2026-09-19 outside the milestone workflow (tag `v1.2.0`, PR #8) — no phase structure; ledger backfill tracked as LEDG-01 in Phase 18*
*v1.3 started 2026-09-20 — Phases 15-18, 14 requirements*
