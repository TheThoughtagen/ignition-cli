# Phase 25: Guarded Project Deploy - Context

**Gathered:** 2026-10-21
**Status:** Ready for planning

<domain>
## Phase Boundary

`ign deploy projects` deploys one Ignition project from its own repo to a gateway, replacing the git module's pull of a production branch. The artifact is checked by hash before upload, the project is exported before it is overwritten and hash-verified after import, and no project other than the one being deployed is ever imported or deleted. `--check` reports what would change with zero writes. Requirements DPRJ-01..05 still apply; the decisions below change how the project set and its parents are sourced (one repo per project instead of one manifest per gateway).

**Fleet model (decided in this discussion, overrides spec §15 F1/F2):** every project repo deploys itself from its own CI on a release tag. WHK-Global additionally deploys gateway config (its tag providers) through the existing `deploy apply`. `whk-environment-orchestration` is not part of the prod path and holds no deploy state. The spec's central-manifest model (F1: gateway config in the env repo; F2: one manifest pins every project on a gateway) is not adopted.

</domain>

<decisions>
## Implementation Decisions

### Artifact, hash and trigger
- **D-01:** The project artifact is built from the repo checkout at run time: `deploy projects` zips the project directory. Plan (and `--check`) reports the zip's canonical hash; apply requires that hash, the same contract as `--plan-sha256` under Decision C (missing hash with `--json` or a non-terminal is `invalid_input`; a different hash is a staleness refusal). DPRJ-01's "verified against its pin before any upload" is met by this hash, not by a committed `ProjectPin.sha256`. — **Reversibility:** costly — the flag name and the staleness slug become part of the scripting contract and the CI workflow.
- **D-02:** A release tag starts a project deploy. One tag promotes through the environments in order (for example dev, then prod), each as its own job gated by that environment's GitHub protection rules, each with its own plan against its own gateway, all from the same tagged commit.
- **D-03:** Deploying a release older than the one the gateway last received from `ign` refuses unless an explicit flag is given (for example `--allow-downgrade`). This needs a durable record of the deployed release per project on the gateway side (see Claude's Discretion). — **Reversibility:** costly — the record format is read by later runs from other runners, like the on-box run record (23 D-07).
- **D-04:** `ProjectPin`-based pinned artifacts (`manifest.projects`) are not the input for this phase's self-deploy flow. Whether the existing `projects` manifest field is kept, reused or left unused is the planner's call, as long as no committed pin is required to deploy.

### Parents across repos
- **D-05:** A child deploy reads `parent` from the repo's `project.json`. Plan, `--check` and apply refuse when that parent is not present on the gateway. A child deploy never imports, exports or modifies its parent. DPRJ-02's parent-first ordering applies within a single deploy only, which here is one project; cycles and unknown parents are handled by this refusal.
- **D-06:** After a parent project (one other projects inherit from, e.g. WHK-Global) deploys, `ign` finds its children on the gateway by their `parent` field and hash-checks each against the hash recorded by that child's last `ign` deploy. A mismatch is a warning in the result and the CI job summary. The parent deploy still succeeds and nothing rolls back. Children without an `ign` deploy record are listed as unverified, not as failures. This is how capture C9's open risk (does overwriting a parent affect a child) is surfaced in production.

### Layout and ownership
- **D-07:** Gateway targets (`targets/<env>.yaml`) and their reviewed `known_hosts` files live in `WhiskeyHouse/whk-onprem-infra-iac` (folder chosen by the planner, for example `ignition/targets/`). Project repos do not hold target files. Project CI checks that repo out read-only with a same-org token. `ign` gains a flag that reads targets from a directory other than `--repo` (for example `--targets DIR`); `known_hosts` paths in a target resolve against that targets checkout. — **Reversibility:** costly — the flag is a CLI contract and the IaC repo layout becomes a dependency of every project pipeline.
- **D-08:** Each project repo holds `deploy/project.yaml`: the gateway project name and the environments it deploys to. `project.json` cannot supply the name (it holds only the display `title`; WHK-Global's title is `"Global"`). The file is strict (`schema_version: 1`, unknown fields refused), like the other deploy files. — **Reversibility:** costly — the file format is read by every project repo's pipeline.
- **D-09:** The zip is the repo root minus `.git`, `.github`, `deploy/`, a fixed built-in exclude list, and the patterns in an optional `.ignexclude`. A repo without `project.json` at the root is refused.
- **D-10:** WHK-Global runs its tag deploy (`deploy apply`, gateway config) and its project deploy (`deploy projects`) as separate jobs with separate approvals. Neither waits on the other. Only WHK-Global's gateway tree lists `tag_providers`; no other repo carries gateway config.

### Git module cutover
- **D-11:** Cutover is per project: an administrator removes one project's `.git` folder on the gateway while the git module keeps serving the others. `ign` never deletes `.git` and refuses a project that still has one (DPRJ-04). This is gated on capture C10: if C10 shows the module recreates `.git` or reverts an API import while it stays installed, the documented procedure falls back to uninstalling the module gateway-wide before any project cuts over.
- **D-12:** The first `ign` deploy of a project (no prior `ign` deploy record for it on that gateway) must change nothing: the release must hash-match what the git module left. A first deploy that would change the project refuses unless explicitly flagged. This proves the release tag equals what the gateway runs before `ign` takes over (spec §13 step 6).

### Carried forward (not re-decided)
- Decision C (hash required in non-interactive runs), Decision D (`projects` is not in the MCP catalog), the run store and guarded gate from Phase 23 (`GUARDED_OPS`, `--yes`, run directories at 0700/0600, git-ignored run dir refusal), Designer/session pre-check before import (23 D-14/D-15 vocabulary), the secret canary on output and run files, and Phase 24's rule that nothing writes to the backup node.

### Claude's Discretion
- Where the per-project deployed-release record lives (on-box next to the Phase 23 run record, or another gateway-side location) and its format, as long as any later runner can read it for D-03, D-06 and D-12.
- Flag names (`--allow-downgrade`, the first-deploy override, `--targets`), the staleness slug for D-01, and the exact `deploy/project.yaml` fields beyond name and environments.
- The fixed built-in exclude list for D-09.
- Whether a reusable `deploy-projects.yml` workflow (alongside `deploy-plan.yml` and `deploy-apply.yml`) is a plan in this phase or a follow-up quick task.
- How release order is compared for D-03 (semver on the tag, or tag creation order).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase scope and requirements
- `.planning/ROADMAP.md` §Phase 25 — goal, success criteria, captures C8/C9/C10, constraints (reuse list, Decision D, cutover prerequisite)
- `.planning/REQUIREMENTS.md` — DPRJ-01..05

### Prior phase decisions
- `.planning/phases/23-guarded-apply-rollback/23-CONTEXT.md` — run store, on-box run record (D-07), Designer preflight (D-13..D-15), guarded gate
- `.planning/phases/24-redundant-pair/24-CONTEXT.md` — no backup writes, payload-slug verdict pattern (D-08)
- `.planning/phases/21-*/` — manifest parsing (`crates/ignition-core/src/deploy/manifest.rs`, `ProjectPin`)

### Design spec
- `whk-environment-orchestration` branch `docs/prod-gateway-config-as-code`, `docs/superpowers/specs/2026-10-09-prod-gateway-config-as-code-design.md` — §13 cutover steps 6 and 8 apply; §15 F1/F2 are explicitly NOT adopted (see Phase Boundary)

### CI
- `.github/workflows/deploy-plan.yml`, `.github/workflows/deploy-apply.yml` and README "Running deploy in CI" — the pattern a project workflow follows (pinned ign install with checksum, config from secrets, sticky plan comment, explicit secret mapping, environment approval)

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/ignition-core/src/actions/projects.rs:144` `validate_import` — zip validation, to be promoted for reuse (roadmap constraint)
- `crates/ignition-core/src/client/mod.rs` `project_export_to_file` / `project_import` — export-before-overwrite and import (with its 200-denial check)
- `crates/ignition-core/src/client/workspace.rs:284` `MemberSource` (use `MemberSource::Zip`, projects only)
- `crates/ignition-core/src/deploy/manifest.rs` — strict YAML parsing pattern (`deny_unknown_fields`, `SchemaVersion`) to follow for `deploy/project.yaml`; an empty `types: []` already passes validation
- `crates/ignition-cli/src/main.rs:4354` `GUARDED_OPS` — `projects` joins it

### Established Patterns
- Plan hash carried from plan to apply (`plan_sha256`, Decision C)
- Payload-slug verdicts with exit 6 after the envelope (Decision A, `deploy_rolled_back`, `deploy_backup_unsynced`)
- Capture-first plans (phases 20-24): the first plan records live captures (here C8, C9, C10) before dependent code

### Integration Points
- Target resolution (`targets/<env>.yaml`) must accept a targets directory separate from `--repo` (D-07)
- README exit table and dated exceptions, TUI row, MCP catalog exclusion test

</code_context>

<specifics>
## Specific Ideas

- Fleet facts from spec §15.1: WHK-Global is parent to `QSI_WhiskeyHouseKentucky01` and `WHK-Controls`; `WHK-Controls` is parent to `WHK-Distillery01-HMI`. Reporting, Scheduling and Diagnostics are standalone. Controls and HMI are CI-only today.
- Example `project.json` (`whk-ignition-scada`): `{"title": "Whiskey House Kentucky HMI", "parent": "WHK-Global", "enabled": true, "inheritable": false}` — the gateway name is not in it.

</specifics>

<deferred>
## Deferred Ideas

- Deleting projects absent from any repo (roadmap: out of scope).
- Spec §15 central env-repo manifest (F1/F2), if the env repo is ever adopted for prod.

</deferred>

---

*Phase: 25-guarded-project-deploy*
*Context gathered: 2026-10-21*
