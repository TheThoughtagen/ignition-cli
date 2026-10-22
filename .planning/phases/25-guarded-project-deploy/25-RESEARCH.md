# Phase 25: Guarded Project Deploy - Research

**Researched:** 2026-10-22
**Domain:** Rust CLI (`ign deploy projects`): deterministic project-zip build from a repo checkout, canonical content hashing, gateway project import/export over REST, host-side `.git` probe and release record over ssh/local, guarded gate and run store reuse
**Confidence:** MEDIUM overall. HIGH on the in-repo seams and surface-parity mechanics (all read this session). MEDIUM-LOW on the gateway's behaviour for a repo-layout zip (the single biggest risk, see Critical Finding F1): it can only be settled by the capture-first plan.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Fleet model (decided in the discussion, overrides spec §15 F1/F2):** every project repo deploys itself from its own CI on a release tag. WHK-Global additionally deploys gateway config (its tag providers) through the existing `deploy apply`. `whk-environment-orchestration` is not part of the prod path and holds no deploy state. The spec's central-manifest model (F1: gateway config in the env repo; F2: one manifest pins every project on a gateway) is not adopted.

**Artifact, hash and trigger**
- **D-01:** The project artifact is built from the repo checkout at run time: `deploy projects` zips the project directory. Plan (and `--check`) reports the zip's canonical hash; apply requires that hash, the same contract as `--plan-sha256` under Decision C (missing hash with `--json` or a non-terminal is `invalid_input`; a different hash is a staleness refusal). DPRJ-01's "verified against its pin before any upload" is met by this hash, not by a committed `ProjectPin.sha256`. Reversibility: costly (the flag name and the staleness slug become part of the scripting contract and the CI workflow).
- **D-02:** A release tag starts a project deploy. One tag promotes through the environments in order (for example dev, then prod), each as its own job gated by that environment's GitHub protection rules, each with its own plan against its own gateway, all from the same tagged commit.
- **D-03:** Deploying a release older than the one the gateway last received from `ign` refuses unless an explicit flag is given (for example `--allow-downgrade`). This needs a durable record of the deployed release per project on the gateway side (see Claude's Discretion). Reversibility: costly (the record format is read by later runs from other runners, like the on-box run record, 23 D-07).
- **D-04:** `ProjectPin`-based pinned artifacts (`manifest.projects`) are not the input for this phase's self-deploy flow. Whether the existing `projects` manifest field is kept, reused or left unused is the planner's call, as long as no committed pin is required to deploy.

**Parents across repos**
- **D-05:** A child deploy reads `parent` from the repo's `project.json`. Plan, `--check` and apply refuse when that parent is not present on the gateway. A child deploy never imports, exports or modifies its parent. DPRJ-02's parent-first ordering applies within a single deploy only, which here is one project; cycles and unknown parents are handled by this refusal.
- **D-06:** After a parent project (one other projects inherit from, e.g. WHK-Global) deploys, `ign` finds its children on the gateway by their `parent` field and hash-checks each against the hash recorded by that child's last `ign` deploy. A mismatch is a warning in the result and the CI job summary. The parent deploy still succeeds and nothing rolls back. Children without an `ign` deploy record are listed as unverified, not as failures. This is how capture C9's open risk (does overwriting a parent affect a child) is surfaced in production.

**Layout and ownership**
- **D-07:** Gateway targets (`targets/<env>.yaml`) and their reviewed `known_hosts` files live in `WhiskeyHouse/whk-onprem-infra-iac` (folder chosen by the planner, for example `ignition/targets/`). Project repos do not hold target files. Project CI checks that repo out read-only with a same-org token. `ign` gains a flag that reads targets from a directory other than `--repo` (for example `--targets DIR`); `known_hosts` paths in a target resolve against that targets checkout. Reversibility: costly (the flag is a CLI contract and the IaC repo layout becomes a dependency of every project pipeline).
- **D-08:** Each project repo holds `deploy/project.yaml`: the gateway project name and the environments it deploys to. `project.json` cannot supply the name (it holds only the display `title`; WHK-Global's title is `"Global"`). The file is strict (`schema_version: 1`, unknown fields refused), like the other deploy files. Reversibility: costly (the file format is read by every project repo's pipeline).
- **D-09:** The zip is the repo root minus `.git`, `.github`, `deploy/`, a fixed built-in exclude list, and the patterns in an optional `.ignexclude`. A repo without `project.json` at the root is refused.
- **D-10:** WHK-Global runs its tag deploy (`deploy apply`, gateway config) and its project deploy (`deploy projects`) as separate jobs with separate approvals. Neither waits on the other. Only WHK-Global's gateway tree lists `tag_providers`; no other repo carries gateway config.

**Git module cutover**
- **D-11:** Cutover is per project: an administrator removes one project's `.git` folder on the gateway while the git module keeps serving the others. `ign` never deletes `.git` and refuses a project that still has one (DPRJ-04). This is gated on capture C10: if C10 shows the module recreates `.git` or reverts an API import while it stays installed, the documented procedure falls back to uninstalling the module gateway-wide before any project cuts over.
- **D-12:** The first `ign` deploy of a project (no prior `ign` deploy record for it on that gateway) must change nothing: the release must hash-match what the git module left. A first deploy that would change the project refuses unless explicitly flagged. This proves the release tag equals what the gateway runs before `ign` takes over (spec §13 step 6).

**Carried forward (not re-decided):** Decision C (hash required in non-interactive runs), Decision D (`projects` is not in the MCP catalog), the run store and guarded gate from Phase 23 (`GUARDED_OPS`, `--yes`, run directories at 0700/0600, git-ignored run dir refusal), Designer/session pre-check before import (23 D-14/D-15 vocabulary), the secret canary on output and run files, and Phase 24's rule that nothing writes to the backup node.

### Claude's Discretion
- Where the per-project deployed-release record lives (on-box next to the Phase 23 run record, or another gateway-side location) and its format, as long as any later runner can read it for D-03, D-06 and D-12.
- Flag names (`--allow-downgrade`, the first-deploy override, `--targets`), the staleness slug for D-01, and the exact `deploy/project.yaml` fields beyond name and environments.
- The fixed built-in exclude list for D-09.
- Whether a reusable `deploy-projects.yml` workflow (alongside `deploy-plan.yml` and `deploy-apply.yml`) is a plan in this phase or a follow-up quick task.
- How release order is compared for D-03 (semver on the tag, or tag creation order).

### Deferred Ideas (OUT OF SCOPE)
- Deleting projects absent from any repo (roadmap: out of scope).
- Spec §15 central env-repo manifest (F1/F2), if the env repo is ever adopted for prod.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| DPRJ-01 | `ign deploy projects` verifies every project artifact against its manifest sha256 pin before any upload | Re-scoped by D-01 to "the artifact's canonical hash, reported by plan/`--check` and required at apply, is verified before any upload". Canonical digest design (F2, F5), `project_hash_mismatch` (F4), Decision C gate order |
| DPRJ-02 | Projects import parent-first from `project.json` inheritance; cycles and unknown parents refuse; unchanged projects (canonical hash) are skipped | Re-scoped by D-05 to one project per deploy: parent read from repo `project.json`, checked against the gateway's `projects()` list; cycle detection over the gateway's parent chain plus the new edge; skip when `live == artifact` canonical digests |
| DPRJ-03 | Each project exported before overwritten, verified after import by canonical hash, ignoring `lastModification*` and signatures | `project_export_to_file` into the run dir before import; second export after; both reduced with `canonical_digest` (reuses `normalize_descriptor`); capture C8 decides whether equality holds |
| DPRJ-04 | Projects not in the manifest reported, never deleted; git-managed project (`.git` present) refused | `unmanaged` = other gateway projects, names only, no delete path exists in the verb; `.git` probe through a new read-only host capability (F3) |
| DPRJ-05 | `ign deploy projects --check` reports what would change with no writes | `--check` returns before the guard and before any run dir, export file, release record or import; proven by a write-forbidding fake (`ForbidWrites` pattern) |
</phase_requirements>

## Summary

Phase 25 is mostly composition over seams that already exist: `project_import` (with the 200-denial check), `project_export_to_file`, a promoted `validate_import`, `projects()`/`project_find`, the Phase 23 run store (`RunDir`, `EventLog`, git-ignore guard), the Designer preflight vocabulary, `strict_parse` for a new `deploy/project.yaml`, the Decision C gate shape, `GUARDED_OPS` plus the MCP full-path exclusion, the TUI route row and the README/exit-table machinery. There are no new crates to add: `zip 8.6`, `sha2 0.10`, `semver 1.0` and `yaml_serde 0.10` are already direct workspace dependencies [VERIFIED: Cargo.toml lines 35, 40, 62 and the yaml_serde entry].

The context decisions (D-01, D-09) change the shape of the problem relative to the roadmap text and the v1.6 research: the artifact is no longer a gateway-produced release zip with a committed pin, it is a zip that `ign` assembles from a git checkout. That surfaces four risks the earlier research never saw, and the plan must treat them as first-class (all detailed under Critical Findings):
(F1) the git-module/checkout directory layout is not the same as the layout of a gateway-produced project export, and nobody has captured whether `projects/import` accepts the checkout layout and what a re-export looks like; (F2) a checkout contains files the gateway will silently not adopt, which would make "export after import equals the zip" fail forever unless the canonical hash and the zip builder agree on the adoptable member set; (F3) the `.git` probe and the per-project release record both need host access that does not exist today (the transport only reads `config/resources/`, and the deploy account has no list right under `data/projects`); (F4) roadmap SC-1 names `project_hash_mismatch` against a "manifest sha256 pin", D-01 replaces the pin with the artifact hash, so the slug and flag must be reconciled explicitly.

**Primary recommendation:** Make Plan 01 a capture-first plan that answers C8 (including the new layout and stray-member questions C8c/C8d), C9 and C10 on the 8.3.6 rig before any dependent code, with a blocking contract checkpoint (the 24-02 pattern) that fixes: the zip layout the importer accepts, the canonical member key space, the built-in include/exclude rule, and the post-import verify verdict. Then build in this order: pure core (`deploy/projects.rs`: project.yaml, artifact build, canonical digest, parent graph, release compare), host capability (`.git` probe + release record, local and ssh), error slugs (four-place rule), engine/action with `--check`, CLI/TUI/MCP/README surface, canary and live gates.

## Critical Findings (read before planning)

### F1. The repo layout is not the gateway export layout (HIGH risk, unresolved)
- A gateway-produced export in this codebase carries a `resources/` segment after the module directory: `<collection>/resources/<rest>`. [VERIFIED: crates/ignition-core/src/client/resources.rs:14-19] ("Zip layout of an 8.3 project export (05-RESEARCH, live-extracted): `project.json` at the root plus `<collection>/resources/<rest>` file members"), and `member_path` documents: "`<collection>/<rest>` -> `<collection>/resources/<rest>`". The README decode example shows `com.inductiveautomation.perspective/resources/views/Dashboard/` [VERIFIED: README.md:1320]. The 05-07 live notes (8.3.3) use `ignition/resources/script-python/uat/hello.py` [CITED: .planning/milestones/v1.0-phases/05-webdev-backend-tag-operations/05-07-PLAN.md:122].
- A git-module-managed project checkout (the repos `ign` will deploy) has no such segment. WHK-Global at `~/data/projects/WHK-Global` has `ignition/script-python`-style directories and `com.inductiveautomation.perspective/{views,style-classes,...}`; the only `resources` directory in the tree is `com.inductiveautomation.webdev/resources` [VERIFIED: `ls` and `find -name resources` on that checkout, this session]. The same checkout also has `project.json` containing exactly `{"title": "Global", "description": ..., "enabled": true, "inheritable": true}` [VERIFIED: file read].
- Counter-evidence: the WebDev deploy zips the CLI builds from `webdev/routes/com.inductiveautomation.webdev/resources/cli/...` (checkout layout) were "import-verified live" [CITED: .planning/milestones/v1.0-phases/05-webdev-backend-tag-operations/05-RESEARCH.md:125-145], so the importer may accept more than one layout, or WebDev is special.
- Consequence: D-09's "zip the repo root" may (a) import fine, (b) import with `{"success":true,"changes":[]}` and silently adopt nothing (README/05-07 document exactly this failure class: "a TRUNCATED zip ... imports with `success:true,changes:[]` and, on overwrite, REPLACES the project with the partial contents" [VERIFIED: actions/projects.rs:151-156]), or (c) be refused. Case (b) with `overwrite=true` is a data-loss path.
- **Required capture (C8c):** import a zip of a real checkout (WHK-Global is ideal, it has Perspective, scripts, WebDev, Vision, reporting) into a fresh project on the 8.3.6 rig, with `overwrite=false` and then `overwrite=true`, and export it back. Record: accepted/denied, `changes` list, member list of the re-export, and which layout (checkout vs `member_path`-adapted export layout) round-trips. If the checkout layout is not adopted, the artifact builder must adapt with the existing `member_path` mapping (the `workspace push` precedent) and the canonical key space must be the user-path form so both sides agree.
- **Defence in depth that is independent of the capture result:** never import a zip whose member set is empty after filtering, and refuse an import whose `outcome.changes` is empty when the plan said `import` (a no-op answer to a non-no-op plan is a failure, not a success). The post-import export check (DPRJ-03) is the final backstop but runs after the overwrite, so the pre-import export in the run dir is what makes it recoverable.

### F2. Unadoptable members make the post-import hash fail forever (HIGH risk)
- The gateway only lands a file member when its parent folder's `resource.json` lists the basename; an unlisted file is "silently ignored (the import still answers `{"success":true}` while nothing lands)" [VERIFIED: resources.rs doc on `FOLDER_DESCRIPTOR`, lines 100-112]. A checkout routinely holds files the gateway never re-exports (READMEs, `__pycache__`, editor files, `.gitkeep`, `*.py` sidecars, dot-directories).
- If the artifact digest includes such a file but the post-import export lacks it, DPRJ-03 verification can never pass and D-12's "first deploy changes nothing" can never hold.
- **Rule for the plan:** the canonical digest and the zip builder must operate over the same *adoptable member set*. Recommended definition (to be confirmed by C8d): `project.json`, plus for every directory that contains a `resource.json`, that `resource.json` and exactly the files its `files` array lists; any listed-but-missing file refuses (`deploy_tree_invalid`); any present-but-unlisted file is excluded and named in the plan's `excluded` list so a reviewer sees it. This is a stricter refinement of D-09's "fixed built-in exclude list", not a contradiction of it.
- `MemberSource::Zip(..).members()/member_hashes()` must NOT be used as the digest walk: they go through `user_path()`, which returns `None` for every member that is not `<collection>/resources/<rest>`, and they skip `project.json` [VERIFIED: resources.rs `user_path` and `member_hashes` doc, lines 156-181 and 466-490]. On a checkout-layout zip they return an EMPTY map, which would read as "no differences" and wrongly skip the import. They also use 64-bit FNV-1a, documented as "change detection, not security" [VERIFIED: resources.rs:411-413].

### F3. `.git` probe and the release record need host capabilities that do not exist (HIGH effort, plan it explicitly)
- Existing transports read only `data/config/resources/<collection>` and the account-home `ign-deploy/...` files [VERIFIED: transport/mod.rs module doc, "reaches the gateway HOST's `data/config/resources/<collection>/` directories"; `TreeReader` has `list_types`, `read`, `read_ledger`, `read_journal` only]. Nothing can look at `data/projects/<name>/.git`, and nothing can write a small per-project record.
- The deploy account's rights are narrow: 23-22 granted `r-x` on `data` and `data/config` only, non-recursive [CITED: STATE.md blockers, "RESOLVED 23-22"]. The Decision B(i) result is that the account "can write its own home but nothing under the data directory" [VERIFIED: ledger.rs module doc]. A `.git` probe under `data/projects/<name>` therefore needs an additional, documented grant (list/traverse on `data/projects` and the project directory). This is human work and must be a checkpoint plus a README account-scope row.
- **Design:** one narrow new read-only trait next to `TreeReader` (for example `ProjectHost`) obtained from the same `node_access` machinery: `git_entry(project) -> GitEntry {Absent|Directory|File|Other}` and `read_release_record(gateway, project)`, plus one tightly-scoped write `write_release_record(gateway, project, bytes)` into the account home (ssh) or `<data_dir>/ign-deploy` (local). Fail closed exactly like the ledger reader: an unreadable path is `transport_failed`, never "absent". A `.git` that is a file (worktree/submodule pointer) or a symlink counts as present. On Windows a `.git` directory is normally hidden, so a PowerShell probe must use `Test-Path -LiteralPath ... -Force` (without `-Force` a hidden `.git` reads as absent) [ASSUMED: PowerShell hidden-attribute behaviour; the capture on the Windows host must confirm].
- A target with neither `ssh` nor `local` cannot be probed: refuse, do not skip (the existing message is "set ssh or local; this verb needs host access" [VERIFIED: transport/mod.rs `node_access`]).
- The write half must stay unreachable from read-only verbs: `write_half_unreachable` source-scan pins that no verb names `TreeMirror`, `mirror_for_node`, `StagingMode`, `.stage(`, `.commit(` [VERIFIED: transport/mod.rs `TreeMirror` doc]. Put the new write behind a separately named trait and extend that scan test, do not widen `TreeReader`.

### F4. SC-1's `project_hash_mismatch` versus D-01's "different hash is a staleness refusal"
- Roadmap SC-1: "a mismatch refuses as `project_hash_mismatch` with nothing uploaded" against a manifest pin. D-01: no committed pin; apply requires the artifact hash; "a different hash is a staleness refusal", slug at the planner's discretion. `project_hash_mismatch` does not exist in `error.rs` yet [VERIFIED: grep of error.rs found `plan_stale`, `deploy_blocked`, no `project_hash_mismatch`].
- **Recommendation:** introduce `project_hash_mismatch` (exit 6, `CoreError::ProjectHashMismatch { project, expected, actual }`) and use it for the D-01 refusal. It honours the roadmap wording, is distinct from `plan_stale` (which is about a reviewed *plan*), and carries hashes only. Name the flag `--artifact-sha256` (it is the artifact's canonical hash, not a plan hash) and keep Decision C semantics: required with `--json`, `--compact` or a non-terminal, otherwise `invalid_input`. Flag this as an assumption for user confirmation (A1) because the flag name and slug are "costly" to change (D-01).

### F5. The repo root holds a lot that is not project content
- WHK-Global's root contains: `depot.json`, `pyrightconfig.json`, `archive`, `images`, `.jython_cache`, `.ignition-stubs`, `db_backup_sqlite.idb`, `docs`, `tags`, `.ignition-preview`, `styling`, `.planning`, `.depot`, `.ai`, `tags.json`, `udts.json`, `.coderabbit.yaml`, `test-results`, `e2e`, `themes`, `.ci-runtime`, `TEC-2293`, `.claude`, `.github`, `.superpowers` next to the real module directories `ignition`, `com.inductiveautomation.{perspective,vision,reporting,alarm-notification,webdev}` [VERIFIED: `find . -maxdepth 3` on that checkout]. A denylist ("fixed built-in exclude list") cannot keep up with that.
- **Recommendation (compatible with D-09):** make the built-in rule an allowlist by shape plus a small fixed denylist for safety: include the root `project.json` and only top-level directories named `ignition` or matching an Ignition module id (`^[a-z][a-z0-9_-]*(\.[a-z0-9_-]+)+$`); everything else at the root is excluded and listed in the plan. The fixed denylist still names `.git`, `.github`, `deploy`, `.ign`, `node_modules`, `__pycache__`, `*.pyc` for any depth. `.ignexclude` then removes further paths inside module directories. A root `tags/` or `themes/` or `images/` directory is gateway configuration, not project content, and must never reach the project import (the project export scope excludes tag providers, tags, UDTs, gateway config [VERIFIED: actions/projects.rs `EXPORT_EXCLUDES`]).
- **`.ign/` must be excluded at any depth.** The run directory is `<repo>/.ign/deploy/<gateway>/<run-id>/` inside the project repo and holds the pre-import export of the live project (secrets) [VERIFIED: run_store.rs `RUN_DIR_ROOT = ".ign/deploy"`]. If the zip walk included it, a second deploy would upload the first deploy's export. It is covered by the allowlist (no dot-directories) but also pin it in a test.

### F6. Things the git module still does that `ign` will not (cutover gaps to document, not to code)
- The prod pull sets `importTags:true, importThemes:true, importImages:true` [VERIFIED: ~/data/projects/WHK-Global/.github/ci/trigger-prod-pull.sh:124]. Tags are covered by WHK-Global's `deploy apply` (D-10). Themes and images are gateway-level resources in 8.3 (images live under `data/config/resources/core/ignition/images` [CITED: Inductive Automation forum, "Ignition 8.3 GIT Deployment Best Practices"]) and are NOT in a project export. After cutover nothing delivers WHK-Global's `images/` and `themes/`. Record this as an Open Question for the user; it is outside DPRJ-01..05 but it will bite the first cutover.
- Prod runs the module with `production_mode: true`, `production_branch: production`, `production_tagPattern: "v*"` [CITED: ~/data/projects/WHK-Global/.planning/codebase/STACK.md:45,98]. The module is therefore already driven by `v*` tags; the same tag will start the new `ign` job, so the cutover must disable the old pull path per project (spec §13 step 8, human work).

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Build artifact zip, canonical digest, project.yaml parse, parent graph, release compare | `ignition-core` pure module (`deploy/projects.rs`) | - | Pure functions of (bytes, paths); testable with no gateway; both front-ends call the same code |
| Gateway project list/find/export/import, Designer list | `GatewayApi` (REST, `client/`) | - | Existing seam; import 200-denial check lives there once for every caller |
| `.git` probe and release record read/write | Host transport (`deploy/transport`, new narrow read trait + separate write trait) | - | These are host-filesystem facts the REST API does not expose; fail-closed rules already live in the transport layer |
| Plan/`--check` decisions, gate order, run orchestration, children sweep | `ignition-core` action (`actions/deploy_projects.rs`) | Run store (`deploy/apply/run_store.rs`) | Gate in core after a read-only plan, before any write (`workspace_push` precedent); actions never print |
| CLI flags, `--yes` guard site, exit-6 verdict hook, human rendering | `ignition-cli` (`cli.rs`, `main.rs`, `render.rs`) | - | Dispatch layer owns `require_confirmation`, `// guarded:` marker, `GUARDED_OPS` |
| MCP exclusion | `ignition-cli/src/mcp.rs` `EXCLUDED_PATHS` | - | Full-path list so the word `projects` is not dropped elsewhere |
| TUI parity row | `ignition-tui/src/routes.rs` | - | Registry-only row on `Screen::Projects` like the rest of the deploy family |
| CI promotion (tag -> dev -> prod, approvals, targets checkout) | GitHub Actions in the project repos / this repo's reusable workflow | IaC repo `WhiskeyHouse/whk-onprem-infra-iac` | D-02, D-07; orchestration only, all decisions stay in `ign` |

## Standard Stack

### Core (all already in the workspace; no new dependency)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `zip` | 8.6 (deflate only) | Build the artifact zip, read exports for hashing, `validate_import` walk | Already the project's only archive crate [VERIFIED: Cargo.toml:40] |
| `sha2` | 0.10 | Canonical digest (sha256) | Promoted into `ignition-core` in Phase 15; used by `deploy::tree::sha256_hex` [VERIFIED: Cargo.toml:62] |
| `semver` | 1.0 | D-03 release ordering on the tag (`v1.4.0` -> `1.4.0`) | Direct dependency of `ignition-core` already [VERIFIED: crates/ignition-core/Cargo.toml:18] |
| `yaml_serde` | 0.10 (lock 0.10.7) | `deploy/project.yaml` via `strict_parse` | The roadmap-locked strict YAML crate [VERIFIED: Cargo.toml comment block, Cargo.lock] |
| `serde`/`serde_json` | workspace | `release.json` record, result/plan/run JSON | Existing |
| `tempfile`, `getrandom` | workspace | Staging of the export, run ids | Existing (`RunId::generate`) |

### Supporting (existing in-repo seams to reuse)
| Seam | Where | Use |
|------|-------|-----|
| `GatewayApi::project_import`, `project_export_to_file`, `project_find`, `projects`, `designers` | `client/mod.rs` | All gateway I/O |
| `actions::resources::export_zip_bytes` | `actions/resources.rs:150` | Export to bytes (temp file + read) for hashing |
| `resources::normalize_descriptor`, `FOLDER_DESCRIPTOR` | `client/resources.rs:455`, `:100-ish` | Strip `attributes.lastModification` and `lastModificationSignature`, canonicalise key order |
| `actions::projects::validate_import` | `actions/projects.rs:144` (private `fn`) | Promote to `pub(crate)`; truncation/corruption guard before any upload |
| `RunDir::create_fresh`, `write_json`, `create_private_file`, `EventLog` | `deploy/apply/run_store.rs` | Run directory, 0700/0600, git-ignore guard, events |
| `strict_parse`, `SchemaVersion` | `deploy/yamlio.rs` | `deploy/project.yaml` |
| `component_problem` | `deploy/mod.rs:36` | Validate the project name and env as single path components |
| `Target::parse`, `load_target`, `node_access` | `deploy/target.rs`, `actions/deploy.rs:144`, `transport/mod.rs` | Targets from `--targets DIR` |
| Designer preflight logic | `deploy/apply/preflight.rs` (`designers_finding`, `DesignersPolicy`, `wait_step`) | Reuse the vocabulary; the finding fn is coupled to `PreflightInput`, so extract the loop into a free function both callers use (see Pitfall 9) |
| `DestructiveChangeRefused { gate, keys, count }` | `error.rs` | Exit-2 "needs a flag" refusals for downgrade and first-deploy change (`gate` is a free string) |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Hand-rolled `.ignexclude` matcher | `globset`/`ignore` crates | Not in the tree (`cargo` lock has no `globset`/`ignore`); project rule "keep the dependency tree lean". Keep the grammar deliberately tiny: one repo-relative path per line, trailing `/` = directory subtree, `*` within one segment, `#` comments, no negation. Justified hand-roll |
| FNV `member_hashes` for the digest | sha256 `canonical_digest` | FNV is for diff UX only, and returns empty on checkout-layout zips (F2). Use sha256 |
| Store the release record in the project (e.g. project description) | On-box file | A project-side marker would be in `project.json` and therefore in the artifact hash, and an overwrite would erase it. On-box file survives import |

**Installation:** none. `cargo build` only.
**Version verification:** `gsd-tools query package-legitimacy check --ecosystem crates semver sha2 zip` returned OK for all three [VERIFIED: seam run this session: semver 19.0M/wk, sha2 24.8M/wk, zip 6.9M/wk, all with source repos, no postinstall].

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| semver | crates.io | 11+ yrs | 19.0M/wk | github.com/dtolnay/semver | OK | Approved (existing direct dep) |
| sha2 | crates.io | 10+ yrs | 24.8M/wk | github.com/RustCrypto/hashes | OK | Approved (existing direct dep) |
| zip | crates.io | 11+ yrs | 6.9M/wk | github.com/zip-rs/zip2 | OK | Approved (existing direct dep) |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none
**New external packages this phase:** none. Every recommendation uses crates already in `Cargo.toml`.

## Architecture Patterns

### System Architecture Diagram

```
 project repo CI (release tag vX.Y.Z)         IaC repo checkout (read-only)
   job per env (dev -> prod), env approval      ignition/targets/<env>.yaml + known_hosts
            |                                              |
            v                                              v
 ign deploy projects --env E --repo . --targets DIR --release vX.Y.Z
            [--check | --artifact-sha256 H --yes] [--designers ..] [--allow-downgrade] [--allow-first-change ..]
            |
 1 load deploy/project.yaml (strict; env must be listed)  --> deploy_tree_invalid
 2 load targets/E.yaml from DIR (known_hosts resolved against DIR)
 3 build artifact: allowlist+excludes+.ignexclude -> adoptable member set -> deterministic zip
 4 canonical_digest(zip) -> artifact_sha256 ; validate_import(zip) ; project.json parent
            |
 5 READ gateway: projects() (parent chain, inheritable, unmanaged names), project_find(name),
                 export live project -> live_digest     [GET only]
 6 READ host:   .git probe (data/projects/<name>/.git) ; release record ign-deploy/<gw>/projects/<name>/release.json
            |
 7 decide:  create | skip(live==artifact) | import
            refuse: .git present | parent absent/cycle | older release (no flag) | first deploy would change (no flag)
            |
        --check ------------------------------------------> render plan, exit 0, ZERO writes
            |
 8 Decision C: no hash + (--json|--compact|non-tty) -> invalid_input ; hash != artifact -> project_hash_mismatch (nothing uploaded)
 9 require_confirmation(--yes)  [// guarded:deploy projects]   -> exit 2 confirmation_required (message = plan)
10 preflights: designers policy, host writable (record), gateway write probe
11 RunDir::create_fresh (git-ignore guard) ; run.json ; events.ndjson ; plan.json
12 export live project -> run_dir/export/<name>.zip  (pre-image; secret-bearing, canary-exempt)
13 project_import(name, zip, overwrite = project exists)   [200-denial -> import_denied]
14 export again -> canonical_digest == artifact_sha256 ?  else verdict deploy_project_unverified (exit 6 after envelope)
15 write release record (on-box)                           [failure = warning, gateway state is already correct]
16 if project is a parent: for each child (projects() parent==name): export, digest, compare to child's record
        -> warnings[], unverified[]    (never fails the deploy, never rolls back)
17 result.json ; envelope ; exit 0 (or 6 for the verdict)
```

### Recommended Project Structure
```
crates/ignition-core/src/deploy/
├── projects.rs            # NEW pure: ProjectFile (deploy/project.yaml), build_artifact, canonical_digest,
│                          #   Exclude rules, parent graph (cycle/unknown), release compare, ReleaseRecord
├── transport/             # NEW narrow ProjectHost (read: git_entry, read_release_record; write: write_release_record)
│   └── projects.rs        #   local + sftp(posix) + sftp/winps(windows) implementations
crates/ignition-core/src/actions/
├── deploy_projects.rs     # NEW: deploy_projects_prepare / _run, result models, children sweep
└── projects.rs            # validate_import -> pub(crate)
crates/ignition-core/src/error.rs      # 3 new slugs (four-place rule)
crates/ignition-core/tests/
├── deploy_projects.rs     # NEW engine tests over FakeGateway + LocalFs tempdir
├── common/fake_deploy.rs  # EXTEND FakeGateway: project store (list/find/export/import, stamp lastModification, configurable overwrite semantics)
└── live_deploy_projects.rs# NEW ignored live gate (rig)
crates/ignition-cli/src/{cli.rs,main.rs,render.rs,mcp.rs}   # leaf, dispatch, guard site, renderer, EXCLUDED_PATHS
crates/ignition-tui/src/routes.rs                             # one CliRoute row
README.md                                                     # Commands row, ### deploy projects, exit-table rows, dated verdict exception, account scope
```

### Pattern 1: Gate order (copy the `deploy apply` order, drop what does not apply)
**What:** read-only plan first, Decision C before any host or gateway contact, `require_confirmation` with the plan as the message, then preflights, run dir, pre-image, write, verify.
**Source:** `DeployCommand::Apply` dispatch in `crates/ignition-cli/src/main.rs:3331-3492` (Decision C block at 3350-3363, guard site at 3395-3398) [VERIFIED: read this session].
**Example (shape only):**
```rust
// Source: crates/ignition-cli/src/main.rs (deploy apply dispatch), adapted
DeployCommand::Projects { env, repo, targets, release, artifact_sha256, check, .. } => {
    // --check: no hash needed, no guard, zero writes.
    if !check && artifact_sha256.is_none()
        && (cli.json || cli.compact || !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal())
    {
        return (None, Err(CoreError::InvalidInput { reason: "deploy projects needs --artifact-sha256 when --json ..." .into() }));
    }
    let prepared = actions::deploy_projects::deploy_projects_prepare(/* project.yaml, targets, artifact, gateway reads, host reads */).await?;
    if check { return (profile, Ok(ActionOutput::DeployProjects(prepared.into_check_result()))); }
    if let Ready { prose, .. } = &prepared {
        // guarded:deploy projects
        if let Err(err) = require_confirmation(cli.yes, prose) { return (profile, Err(err)); }
    }
    // preflights, run dir, export, import, verify ...
}
```

### Pattern 2: Canonical digest (one function, both sides, user-path key space)
**What:** sha256 over a sorted `(key, sha256(normalised bytes))` list. Key = the user-path form (`<collection>/<rest>`, or `project.json`), so a checkout and a gateway export agree whichever layout C8c proves. Normalise (a) any basename `resource.json` through `normalize_descriptor` (falls back to raw bytes if it is not a JSON object, the existing rule), (b) `project.json` the same way (drop only `lastModification*` if present, keep the rest). Directory entries never count. An empty member set is an error, not a digest.
**Example:**
```rust
// Source: pattern from client/resources.rs member_hashes + normalize_descriptor; new code
pub struct CanonicalDigest { pub sha256: String, pub members: BTreeMap<String, String> }
pub fn canonical_digest(zip: &[u8]) -> Result<CanonicalDigest, CoreError> {
    // walk EVERY non-dir member (not via MemberSource::members(): it drops project.json and non-`resources/` shapes)
    // key = to_user_path(name) | "project.json"; bytes = if basename == "resource.json" { normalize_descriptor(&b).unwrap_or(b) } else { b }
    // members.insert(key, sha256_hex(&bytes)); then sha256 over "key\0hash\n" lines in BTreeMap order.
}
```

### Pattern 3: Deterministic artifact zip
Entries sorted by key, fixed timestamp, `Deflated` (the `rewrite_options()` precedent in `resources.rs`), a directory entry for every ancestor folder (05-07: "real gateway-produced zips carry folder entries; ad-hoc appenders skip them"), file permission bits fixed, symlinks refused (never followed, same stance as the transport readers), size caps taken from the existing import limit (`IMPORT_MAX_BYTES` 512 MB) and `READ_MAX_*`-style entry caps. Two builds of the same checkout produce identical bytes, so the reported hash is stable across runners (D-01 requires plan-on-one-runner/apply-on-another stability only through the canonical digest, but byte determinism makes debugging sane).

### Pattern 4: Release record (D-03, D-06, D-12)
On-box file `ign-deploy/<gateway>/projects/<project>/release.json`, same root rule as the ledger (`LedgerRoot::for_node`: account home for ssh, data dir for local) [VERIFIED: ledger.rs `ledger_rel` = `format!("ign-deploy/{gateway}/{mode}/{LEDGER_FILE}")`, `LedgerRoot`]. Strict JSON, `schema_version: 1`, `deny_unknown_fields`, size-capped, never trust unparsed bytes (the `Ledger::parse` pattern). Fields: `schema_version`, `gateway`, `project`, `release` (the tag), `artifact_sha256`, `run_id`, `deployed_at` (RFC 3339, informational only), `parent` (for D-06 child lookup the gateway `parent` field is authoritative, do not rely on it). Reserve the segment: a target `mode` named `projects` would collide with `ign-deploy/<gateway>/projects/`, so add `projects` to the `Target::validate` reserved-mode refusal next to `core`/`local` (the existing check refuses `core` and `local` [VERIFIED: target.rs `validate`]). Write the record only after the verify passes, and also on a first-deploy no-op (so the baseline exists), never in `--check`.
- Release order (D-03): `semver::Version::parse(tag.trim_start_matches('v'))`, standard semver precedence including pre-releases. A tag that does not parse is `invalid_input` unless the record is absent. Equal release re-deploy is allowed (idempotent, covers re-runs and promotion retries). `--allow-downgrade` is recorded in `run.json` and in the result.

### Anti-Patterns to Avoid
- **Using `member_hashes`/`MemberSource::members` as the digest** (F2). Empty result reads as "same".
- **Importing before the hash gate, or computing the gate from the live gateway only.** The gate compares the supplied hash with the locally built artifact before any gateway write; the plan separately reports live-vs-artifact.
- **Letting `--check` create `.ign/`** or write a record. The Phase 23 no-op precedent creates no `.ign/` directory [VERIFIED: README deploy apply row: "an empty plan ... creates no `.ign/` directory"].
- **Widening `TreeReader` or `GatewayApi`.** `GatewayApi` has hand-written `unreachable!()` doubles in many modules [CITED: .planning/research/ARCHITECTURE.md:17], so add host methods on a new trait and keep gateway calls on the existing methods.
- **Touching the parent or children from a project deploy** beyond read-only exports for D-06. No import, no export-for-restore, no modify of any project but the one named in `deploy/project.yaml`.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Import with denial detection | A second HTTP import path | `GatewayApi::project_import` | The 200-with-`success:false` check lives at that one seam for every caller [VERIFIED: client/mod.rs, "ONE seam here fixes every import caller at once"] |
| Zip structure validation | Another magic/size/truncation check | `validate_import` (promote to `pub(crate)`) | 05-07 found truncated zips import as `success:true` and replace the project |
| Volatile-field normalisation | A new JSON scrub | `resources::normalize_descriptor` | Strips exactly `attributes.lastModification` and `lastModificationSignature`, sorts keys |
| Strict config file | Custom YAML parsing | `strict_parse` + `deny_unknown_fields` + `SchemaVersion` | Gives positioned findings, rejects CR, caps size |
| Single path component safety | Ad hoc `contains("..")` | `component_problem` (+ the windows variant on ssh Windows nodes) | Project names and env names become path segments and CLI arguments |
| Run directory | Custom temp dir | `RunDir::create_fresh` | 0700/0600, git-ignore refusal, `deploy_run_invalid` on reuse |
| Guarded gate | New confirmation prompt | `require_confirmation(cli.yes, prose)` with `// guarded:deploy projects` marker | The drift test walks markers both ways |
| Semver ordering | String compare of tags | `semver::Version` | Pre-release precedence is non-trivial |
| Exit-6 verdict after envelope | Printing then `exit(6)` ad hoc | The `failure_exit_code()` hook at `main.rs:601-602` | Same mechanism as apply |

**Key insight:** every dangerous behaviour in this phase (silent partial import, perpetual diff, secret echo, path escape) already has a named guard in the codebase. The risk is not missing machinery, it is wiring the new artifact (a checkout instead of a gateway export) into guards that assumed export-shaped input.

## Runtime State Inventory

Not a rename/refactor/migration phase. Skipped, with one cutover caveat recorded for the planner: the git module's per-project `.git` folder and its `git.yaml` entries are runtime state on the production gateway that this phase reads (refuses on) but never changes. That is human work (D-11).

## Common Pitfalls

### Pitfall 1: A "successful" import that adopted nothing
**What goes wrong:** The gateway answers 200 `{"success":true,"changes":[]}` for a zip it could not parse into resources; with `overwrite=true` the project is emptied. **Why:** F1/F2 and the 05-07 truncation finding. **Avoid:** refuse an empty adoptable member set locally; treat an `import` plan whose response `changes` is empty as a failure; always export before; verify after. **Warning signs:** `changes: []` on an import of a non-empty zip; post-import member count lower than the artifact's.

### Pitfall 2: Perpetual drift from volatile fields
**What goes wrong:** the re-export has `lastModification` stamps and different member order, so a byte hash never matches. **Avoid:** the canonical digest only; never compare raw zip bytes (PITFALLS #29 [CITED: .planning/research/PITFALLS.md:506]). C8b records which other fields churn (for example `project.json` extras the gateway adds).

### Pitfall 3: Downgrade and first-deploy guards that depend on a missing record
**What goes wrong:** an unreadable or corrupt `release.json` is treated as "absent", so the first-deploy rule or the downgrade guard silently switches off. **Avoid:** absent = `None` only when no level of the path exists; any anomaly (symlink, non-regular, over cap, parse error, schema mismatch, foreign gateway/project) is a typed refusal, exactly like `read_ledger` ("A failed read is never an empty read").

### Pitfall 4: Windows `.git` is hidden
**What goes wrong:** the PowerShell probe reports no `.git` because the directory carries the Hidden attribute. **Avoid:** `-Force`; treat a `.git` file or reparse point as present; capture on the Windows host (`ign-win-836`) before relying on it. [ASSUMED]

### Pitfall 5: CRLF in `deploy/project.yaml` and `.ignexclude`
**What goes wrong:** a Windows checkout with `text=auto` converts line endings; `strict_parse` refuses any CR ("save the file with LF line endings" [VERIFIED: yamlio.rs]). **Avoid:** document `deploy/** -text` (or `eol=lf`) in the README and have `.ignexclude` parsing tolerate CRLF (it is a path list, not a contract file) while `project.yaml` stays strict. The v1.6 research already flagged the same issue for `gateways/**` [CITED: SUMMARY.md:137].

### Pitfall 6: Run directory inside the repo being zipped (F5)
Covered above: exclude `.ign` everywhere; also the run-dir git-ignore guard refuses when the project repo's `.gitignore` lacks `.ign/`, which every project repo must add once (document in README and the workflow).

### Pitfall 7: Children restart when the parent is overwritten
S5 observed "Starting project: SPIKE_CHILD ... collection=SPIKE_PARENT" after a parent overwrite; session impact was not observable on the rig [VERIFIED: spike S5.md in whk-environment-orchestration branch `docs/prod-gateway-config-as-code`, verdicts `S5a-child-restarts-bound-to-parent-after-overwrite: OBSERVED`, `S5a-child-reads-inherited-resource: OPEN`]. **Avoid:** do not claim it in any success criterion (roadmap: "no criterion here claims it"); D-06 only surfaces child hash drift as warnings. Run the child sweep after a settle wait (poll the gateway's project list until the children are `enabled` and the parent export is stable) rather than immediately; the wait bound is a planner constant.

### Pitfall 8: Release record races between runners
Two runners (dev and prod jobs use different gateways, so different records) are safe; two jobs for the SAME gateway and project are not. Use a GitHub `concurrency` group per gateway/project in the workflow (the existing apply workflow does this per env) and make the record write a replace-by-move (temp then rename), so a reader never sees a torn file.

### Pitfall 9: Designer check is coupled to `PreflightInput`
`designers_finding` takes `&PreflightInput` and writes `ApplyEvent`s through `Steps`. Reusing it means either constructing a `PreflightInput` with dummy fields or extracting the poll loop into a function over `(&dyn GatewayApi, &DesignersPolicy, &dyn Clock, emit)`. Extract; keep `--designers fail|wait|proceed` and `--designers-timeout` identical to apply so the vocabulary is shared (23 D-14/D-15). Consider scoping the refusal to sessions on the target project or its descendants (a Designer open on an unrelated project does not conflict with this import); the 23 behaviour is "any session", so default to "any" for consistency and record scoping as an Open Question.

### Pitfall 10: Project name as an on-box path segment
Gateway project names can contain spaces and mixed case (the REST path is percent-encoded per segment [VERIFIED: client/projects.rs `encode_segment` tests]). The host probe and the record path put the name in a path: validate with `component_problem` and, for Windows ssh nodes, the Windows component rule used by `read_home_file`; refuse otherwise (never escape).

## Code Examples

### `deploy/project.yaml` (new, strict)
```rust
// Source: pattern from deploy/manifest.rs and deploy/target.rs (deny_unknown_fields + SchemaVersion + validate())
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectFile {
    pub schema_version: SchemaVersion,
    /// The project's name on the gateway (a single path component).
    pub project: String,
    /// Environments (target file names without `.yaml`) this project deploys to.
    pub environments: Vec<String>,
}
```
The file is read from `<repo>/deploy/project.yaml`; `--env` must be listed or the verb refuses with a `deploy_tree_invalid` finding.

### Existing structure the manifest field leaves untouched (D-04)
```rust
// Source: crates/ignition-core/src/deploy/manifest.rs:40-52 [VERIFIED: Read this session]
pub struct ProjectPin {
    /// Project name on the gateway.
    pub name: String,
    /// Version label of the artifact.
    pub version: String,
    /// Lowercase hex sha256 (64 chars) of the artifact bytes.
    pub sha256: String,
    /// Where the artifact is fetched from; opaque to Phase 21.
    pub artifact: String,
}
```
Recommendation for D-04: leave `Manifest.projects` and `ProjectPin` unchanged and unused by this verb (no code, no deprecation churn); note in the README that gateway-config repos may carry it but `deploy projects` ignores it. Removing it would break `strict_parse` of any manifest that already declares `projects`.

### Guard registration
```rust
// Source: crates/ignition-cli/src/main.rs GUARDED_OPS (line 4354) [VERIFIED]
// add, with the dynamic-prose comment style of "deploy apply":
(
    "deploy projects",
    "deploy projects overwrite-imports the project into the gateway from the repo checkout",
),
```
```rust
// Source: crates/ignition-cli/src/mcp.rs:72 and :880 [VERIFIED]
pub(crate) const EXCLUDED_PATHS: &[&str] = &["deploy apply", "deploy rollback", "deploy projects"];
const GUARDED_AND_EXCLUDED: &[&str] = &["edit", "deploy apply", "deploy rollback", "deploy projects"];
```
`deploy_rollback_is_excluded_by_full_path` currently pins the sorted set equal to `["deploy apply", "deploy rollback", "edit"]` [VERIFIED: mcp.rs:960-961 region]; update it to include `deploy projects` and add a `deploy_projects_is_excluded_by_full_path` test alongside it. Because the exclusion is by leaf path, `--check` is excluded from MCP too (accepted: Decision D says `projects` is not an MCP tool in v1.6).

### TUI row
```rust
// Source: crates/ignition-tui/src/routes.rs:478-505 pattern [VERIFIED]
CliRoute { path: "deploy projects", mapping: Mapping::Screen(Screen::Projects) },
```

## Captures (Plan 01, capture-first; Phases 20-24 convention)

Rig: `crates/ignition-core/tests/fixtures/live-rig-deploy/` (Compose project `ign-deploy-rig`, 8.3.6, port 18290, `capture22.sh`/`capture23.sh` drivers) [VERIFIED: compose.yml header and `live_deploy_apply.rs` header]. Record in `25-LIVE-CAPTURES.md` using the 24-LIVE-CAPTURES convention: a "Captures Owed" table written BEFORE any capture block, every block preceded by the exact command and a UTC time, tokens never printed [VERIFIED: 24-LIVE-CAPTURES.md head]. End with a blocking contract checkpoint (24-02 pattern) whose decisions every later plan asserts as a precondition.

| ID | Question | Method | Decides |
|----|----------|--------|---------|
| C8a | Does `import?overwrite=true` delete resources absent from the zip? | Import a zip with members A,B; re-import with only A, `overwrite=true`; export. Also drop a whole folder and drop a `resource.json`-listed file | Already supported by: README ("resources absent from the ZIP are deleted" [VERIFIED: README.md:1285-1310 region]), `ign resource delete` relying on member removal, 05-UAT "REPLACE of an existing member and member REMOVAL import cleanly" [CITED]. Re-confirm on 8.3.6; decides whether the plan must list `removed` members as the destructive signal |
| C8b | After import, does the re-export equal the zip once `lastModification*` is normalised? Which other fields churn (`project.json`)? | Round trip a gateway-produced export; diff `canonical_digest` | Whether `canonical_digest` as designed can ever be equal; extra normalisation rules |
| **C8c** | **Does the importer accept the git-module checkout layout (no `resources/` segment), and what does the re-export look like?** | Zip WHK-Global (and a small synthetic) with the proposed allowlist; import to a fresh project name with `overwrite=false` then `true`; compare member lists with the export layout | **F1.** Artifact layout (verbatim vs `member_path` adapter), canonical key space, whether `changes:[]` appears |
| **C8d** | What happens to unlisted files, dotfiles, empty dirs, a folder with no `resource.json`, `*.py` next to a `code.py`? | Add each to the zip; import; re-export | **F2.** The adoptable-member rule and the `excluded` reporting |
| C8e | What does the gateway do with a `project.json` whose `parent` does not exist, is itself, or points at a non-inheritable project? | Import three variants with `overwrite=false` | Whether the local parent checks are advisory or the only line of defence; the `not_inheritable` refusal |
| C9 | After overwriting a parent, do children keep reading inherited resources; what restarts; is anything observable via REST or a script call? | Parent P (script-python module + a view), child C inheriting; overwrite P; call P's function through C via `ign script exec`/`ign testing run` (project scope C); read the gateway log; optionally a Perspective session via `ign e2e` | Only the D-06 sweep wording and the settle-wait constant. **S5 residual stays OPEN; no criterion claims it** |
| C10 | With the git module installed and a `git.yaml` carrying `commissioning_importThemes/Tags/Images` and `enforceBranch`, does it recreate a stripped `core` resource, or revert an API import, on restart or commissioning cycle? | `ign rig module` can provision the git module (RMOD, Phase 15-19); set up a local repo; API-import a change; restart; observe | **D-11.** Per-project `.git` removal with the module installed, or the documented fallback of uninstalling the module gateway-wide first. Prod `git.yaml` is not readable from this machine, so a human checkpoint must read it (known template flags: `.github/ci/git.yaml.tmpl` has `commissioning_*: true`, `commissioning_enforceBranch: false`, `production_mode: false` [VERIFIED: file read]; prod runs `production_mode: true`, `production_tagPattern: "v*"` [CITED: STACK.md]) |
| C-H | Host probe rights: can the deploy account list `data/projects/<name>` on Linux (`ign-lnx-836`) and Windows (`ign-win-836`) hosts; does a hidden `.git` show; how long does the probe take? | Run the proposed probes with the real account | **F3.** Required grant text for README, per-OS probe commands |

## Suggested Plan Decomposition (planner owns the final split)

| Wave | Plan | Content | Reqs |
|------|------|---------|------|
| 1 | 25-01 | Rig fixture reuse + captures C8a-e, C9, C10, C-H; "Captures Owed" first; COVERAGE.md (all `/projects/*` routes already integrated: expected all INTEGRATE/existing, no new routes); contract checkpoint (layout, key space, include rule, verify verdict, slug names, flag names) | all |
| 2 | 25-02 | `deploy/projects.rs` pure core: `ProjectFile`, exclude/allowlist, deterministic zip, `canonical_digest`, parent graph (self/cycle/unknown), `ReleaseRecord` (+ strict parse), semver compare; `validate_import` promoted; `Target` reserves `projects`; unit tests with fixture checkouts | DPRJ-01, 02, 03 |
| 3 | 25-03 | Four-place slugs: `project_hash_mismatch`, `project_git_managed`, `project_parent_invalid` (+ README rows, `DEPLOY_SLUGS`, `exit_code_mapping_enumerated`); `DestructiveChangeRefused` gate values `downgrade` and `first_deploy` | DPRJ-01, 02, 04 |
| 3 | 25-04 | Host capability `ProjectHost` (git probe, record read/write) for LocalFs and sftp posix/windows, fakes, `write_half_unreachable` scan extended, per-OS probe tests (parallel with 25-03) | DPRJ-04 |
| 4 | 25-05 | `FakeGateway` project store (stamps `lastModification`, configurable overwrite/adoption semantics from the captures) + `ForbidWrites` for `--check` | DPRJ-03, 05 |
| 5 | 25-06 | Action: `deploy_projects_prepare` (plan, `--check`), `deploy_projects_run` (preflights, run dir, export, import, verify, record, children sweep), result models, events, designers extraction | DPRJ-01..05 |
| 6 | 25-07 | CLI surface: `DeployCommand::Projects`, `--targets`, dispatch with Decision C and guard marker, `GUARDED_OPS`, `EXCLUDED_PATHS`, verdict exit hook, human renderer, TUI row | DPRJ-05 |
| 7 | 25-08 | README (Commands row, `### deploy projects`, exit rows, dated sixth verdict exception, account-scope grant, `.gitignore`/`-text` requirements, cutover runbook per D-11/D-12), coverage tests, canary tests (DSURF-03 extension) | DPRJ-05 |
| 8 | 25-09 | Live gate on the rig (create, skip, import, parent missing, `.git`, downgrade, first-deploy, verify mismatch, children sweep), installed-binary smoke (`cargo install --path crates/ignition-cli --force`), reusable `deploy-projects.yml` OR a recorded follow-up quick task | all |

Phase conventions to keep in every plan: no worktrees (shared `CARGO_TARGET_DIR` hazard) [VERIFIED: ./.claude/CLAUDE.md "Worktrees and cargo builds"]; exit-code source of truth is README plus `crates/ignition-core/src/error.rs`, pinned by `readme_exit_table_agreement` and `exit_code_mapping_enumerated`; surface parity (TUI row, README row, guarded registry, MCP exclusion) lands in the same plans as the verb, never as a final sweep.

## Slug and Exit-Code Plan (four-place rule: variant, `code()`, `exit_code()`, `hint()`, plus README row and `exit_code_mapping_enumerated`)

Existing table facts: exit 2 usage includes `invalid_input`, `deploy_tree_invalid`, `destructive_change_refused`; exit 6 target_state includes `plan_stale`, `deploy_preflight_failed`, `deploy_run_invalid`, `import_denied`, `transport_failed` is exit 4 [VERIFIED: error.rs:1-20 table and README.md:219-232]. `CoreError::DEPLOY_SLUGS` lists the deploy slugs in README row order and a test pins hints do not contain `ign ` for them [CITED: 23-PATTERNS.md:162].

| Situation | Slug | Exit | New? |
|-----------|------|------|------|
| `deploy/project.yaml`/`project.json` invalid, env not listed, zip entry/size cap, unlisted-but-declared file missing | `deploy_tree_invalid` | 2 | reuse |
| Missing hash with `--json`/`--compact`/non-terminal; unparseable release tag | `invalid_input` | 2 | reuse |
| Supplied hash differs from the built artifact (SC-1, F4) | `project_hash_mismatch` | 6 | NEW |
| `.git` present on the gateway project (DPRJ-04) | `project_git_managed` | 6 | NEW |
| Parent absent on gateway, cycle, parent not inheritable (DPRJ-02) | `project_parent_invalid` (`reason`: `missing`, `cycle`, `not_inheritable`) | 6 | NEW |
| Downgrade without flag (D-03); first deploy would change the project without flag (D-12) | `destructive_change_refused` with `gate` `downgrade` / `first_deploy` | 2 | reuse (free-form `gate`) |
| Designers open under `fail`/timeout | `deploy_preflight_failed` check `designers` | 6 | reuse |
| Run dir not git-ignored / exists | `deploy_preflight_failed` check `run_dir_not_ignored` / `deploy_run_invalid` | 6 | reuse |
| Gateway denies the import (200 `success:false`) | `import_denied` | 6 | reuse |
| Host unreadable (`.git` probe, record read) | `transport_failed` | 4 | reuse |
| Release record corrupt or foreign | `deploy_run_invalid` (named location) | 6 | reuse (mirrors `read_journal`) |
| Post-import canonical hash differs (DPRJ-03) | payload verdict `deploy_project_unverified`, `data.outcome` `imported_unverified`, `data.ok:false`, success envelope then exit 6 | 6 | verdict, NOT a `CoreError` slug and NOT an exit-table row (Decision A pattern: tests `rolled_back_verdict_exception_is_dated_and_documented`, `backup_unsynced_verdict_exception_is_dated_and_documented`) |

The README needs a dated SIXTH success-path exit exception under `## Output contract (for agents)` (the existing ones are numbered up to the FIFTH, `deploy apply` backup-unsynced, 2026-10-20 [VERIFIED: README.md:174-176]); copy the structure and add a `deploy_projects_verdict_exception_is_dated_and_documented` test in `deploy_family_coverage.rs`.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Spec §6.5 / v1.6 research: release zip from the env repo, manifest `ProjectPin` sha256, `--artifacts DIR` | Zip the project's own checkout at run time; the artifact hash is the pin (D-01); targets from the IaC repo (D-07) | Phase 25 discussion, 2026-10-21 | The artifact is no longer gateway-produced, so layout and adoptable-member risks (F1, F2) are new |
| Git module pulls `v*` tags on the gateway (`production_mode`) | `ign` pushes via REST on the same tag in CI | cutover | Old path must be disabled per project (D-11) |
| `member_hashes` FNV diff for workspace/sync | sha256 canonical digest for deploy | Phase 22 precedent for the config tree | Do not reuse FNV for a gate |

**Deprecated/outdated inside this repo's own planning text:** ROADMAP/REQUIREMENTS wording "manifest sha256 pin", "manifest-pinned project set", and "A project absent from the manifest" describe the superseded F1/F2 model. D-01/D-04/D-05 govern. The plan's must-haves should quote the CONTEXT wording and map DPRJ-01..05 as in the `phase_requirements` table above, or the verifier will test the wrong thing.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Flag `--artifact-sha256` and slug `project_hash_mismatch` for the D-01 contract (CONTEXT leaves the names to the planner but marks the choice "costly") | F4, Slug plan | CI workflow and scripts must be edited; user should confirm before 25-03 |
| A2 | The adoptable member set = `project.json` + `resource.json` descriptors + files they list (stricter than D-09's literal "root minus excludes") | F2, F5 | If the user wants a literal zip-everything, perpetual hash mismatch and silent non-adoption return; C8d decides |
| A3 | Module-shaped top-level directory allowlist regex `^(ignition|[a-z][a-z0-9_-]*(\.[a-z0-9_-]+)+)$` covers every real project module dir (third-party modules with unusual ids would be excluded) | F5 | A real module's resources would be omitted and, with overwrite, deleted from the gateway; list the omitted top-level dirs in every plan output and let `.ignexclude`'s inverse be an `.ignaccept`-style escape only if the user asks |
| A4 | Windows `.git` is hidden and needs `Test-Path -Force` | Pitfall 4, F3 | `.git` refusal silently off on Windows prod (WIGNVM01/04); C-H must capture on the Windows host |
| A5 | A parent with `inheritable == Some(false)` should refuse (`project_parent_invalid`, `not_inheritable`) | Slug plan | A false refusal blocks a legitimate deploy; C8e decides whether to keep it advisory |
| A6 | A project absent on the gateway is created (`overwrite=false`, no pre-export) rather than refused | Open Question 4 | Surprise project creation on prod; user should confirm |
| A7 | Post-import verify failure leaves the gateway as imported and does NOT auto-restore from the pre-import export | Slug plan, Open Question 2 | A bad deploy stays live until a human re-imports the export; the roadmap wording ("exported before overwritten") does not require auto-restore but operators may expect it |
| A8 | Designer refusal scope stays "any open session" for vocabulary parity | Pitfall 9 | Needless refusals on unrelated Designer sessions |
| A9 | `deploy-projects.yml` reusable workflow is a follow-up (the apply/plan workflows share byte-identical `shared:` regions checked by a harness [VERIFIED: deploy-apply.yml header comment]) | Plan 25-09 | If the user wants the workflow in-phase, add a plan and the harness diff |

## Open Questions

1. **Flag and slug names for D-01 (A1).** What we know: roadmap says `project_hash_mismatch`; D-01 says "staleness refusal". Unclear: whether the user wants one flag (`--artifact-sha256`) or the apply-style `--plan-sha256`. Recommendation: `--artifact-sha256` and `project_hash_mismatch`, confirmed at the 25-01 contract checkpoint.
2. **Post-import mismatch handling (A7).** Recommendation: verdict `deploy_project_unverified`, exit 6, no auto-restore, print the exact manual restore command (`ign project import <name> --file <run_dir>/export/<name>.zip --collision-policy overwrite --yes`). Confirm with the user; an auto-restore would be a second overwrite import and needs its own safeguards.
3. **Removed-member gate.** C8a will confirm deletion of absent resources. Recommendation: report `added/changed/removed/same` counts and the removed paths in the plan and the confirmation prose; do not add a removal flag in v1.6 (the repo is the source of truth and the pre-import export is the safety net). If the user wants a flag (`--allow-removals`), it reuses `DestructiveChangeRefused` with `gate: "removals"`.
4. **Create when absent (A6).** Recommendation: allowed, action `create`, no first-deploy flag needed because nothing exists to change.
5. **Themes and images after cutover (F6).** Nothing delivers WHK-Global's `images/` and `themes/` once the git module is retired. Out of DPRJ scope; needs an owner before the WHK-Global cutover.
6. **Where do the grant and runbook live?** The `data/projects` list right for the deploy account (F3) and the per-project cutover procedure (D-11, D-12) are human steps; the README account-scope section plus a checkpoint in 25-09 is the recommendation.
7. **Drift between deploys.** A Designer save on the gateway after the last `ign` deploy is overwritten silently (live digest differs from the recorded `artifact_sha256`). The plan will report `live_matches_last_deploy: false` as a warning. Whether to gate it (like `--allow-drift`) is not decided in CONTEXT; recommendation: warning only in v1.6, because the pre-import export preserves the live state.

8. **Meaning of `--targets DIR` (D-07).** What we know: `load_target(repo, env)` reads `<repo>/targets/<env>.yaml` and `node_access(repo, ...)` resolves `known_hosts` and relative `local.data_dir` against the same `repo` argument [VERIFIED: actions/deploy.rs:144-158, transport/mod.rs `node_access`]. Unclear: whether the user wants `DIR` to be the directory holding `<env>.yaml` or the root containing `targets/`. Recommendation: `--targets DIR` is a ROOT with the existing layout beneath it (`DIR/targets/<env>.yaml`, `DIR/<known_hosts>`), so the IaC folder is `ignition/` containing `targets/` and the `known_hosts` files, and the implementation passes `DIR` where `--repo` was passed today. This changes no parser, and "known_hosts resolve against that targets checkout" holds literally.
9. **Gateway name for the run directory.** The run directory is `<repo>/.ign/deploy/<gateway>/<run-id>` and `<gateway>` comes from the target's `gateway` field, which for a project deploy is the IaC target (for example the WHK-Global gateway), not the project. Recommendation: keep it (the run is a run against that gateway) and note in the README that `.ign/` must be git-ignored in each project repo.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain (cargo) | build/test | yes | cargo 1.94.1 (MSRV 1.88 in Cargo.toml) [VERIFIED: `cargo --version`, Cargo.toml] | - |
| Docker | rig captures C8/C9/C10, live gate | yes (daemon responsive but slow: `docker ps` took ~2 min) [VERIFIED: command timing this session] | not queried | the Linux VM `ign-lnx-836` or the Azure VM `ign-win-836` for host-probe captures |
| 8.3.6 rig fixture | captures | yes, files present: `crates/ignition-core/tests/fixtures/live-rig-deploy/{compose.yml,compose.bind.yml,capture22.sh,capture23.sh,mint-token.sh}` | image `inductiveautomation/ignition:8.3.6` (amd64, emulated on arm64) | - |
| Git module artifact | C10 | fetchable via `ign rig module` (RMOD feed `WhiskeyHouse/ignition-git-module`) [VERIFIED: module/mod.rs registry entry `id: "git"`] | 2.3.4 signed per Phase 15 | - |
| Real project checkout for C8c | C8c | yes, `~/data/projects/WHK-Global` (+ 6 other project repos) | - | synthetic fixture |
| Windows host for C-H | `.git` probe | `ign-win-836` is a recorded Azure capture host; availability must be checked at execution [CITED: STATE.md blockers] | - | Linux-only probe with Windows marked waived/manual (the Phase 22/23 evidence-ledger convention) |
| Prod `git.yaml` | C10 human step | no (not readable from this machine) | - | user reads and pastes the flags |

**Missing with no fallback:** prod `git.yaml` content (human checkpoint).

## Validation Architecture

> `workflow.nyquist_validation` is `true` in `.planning/config.json` [VERIFIED: file read this session].

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust `cargo test` (libtest + `tokio::test`), `wiremock`, `assert_cmd`-style CLI tests, `proptest` present [CITED: 24-RESEARCH.md:474] |
| Config file | workspace `Cargo.toml`; no separate runner config |
| Quick run command | `cargo test -p ignition-core --lib deploy::projects` |
| Full suite command | `cargo test --workspace` (live gates are `#[ignore]` and never in CI) |
| Live gate | `IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 IGNITION_LIVE_TOKEN=... cargo test -p ignition-core --test live_deploy_projects -- --ignored --nocapture --test-threads=1` (mirrors the `live_deploy_apply.rs` header; loopback and gateway-name guards) |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| DPRJ-01 | Supplied hash equal to the built artifact passes; unequal refuses `project_hash_mismatch` with the fake gateway recording ZERO write requests | engine | `cargo test -p ignition-core --test deploy_projects hash_mismatch_uploads_nothing` | Wave 0 |
| DPRJ-01 | Decision C: no hash + `--json`/`--compact`/non-terminal is `invalid_input` before any host or gateway contact; `--check` needs no hash | CLI contract | `cargo test -p ignition-cli --test contract_deploy projects_requires_hash_when_noninteractive` | extend |
| DPRJ-01 | Artifact build is deterministic (same checkout -> same bytes and digest); empty adoptable set refuses; `.ign`, `.git`, `deploy/` never enter; symlink refused; cap enforced | unit | `cargo test -p ignition-core --lib deploy::projects::artifact` | Wave 0 |
| DPRJ-01 | `canonical_digest` ignores `lastModification` and signature, key order, zip entry order, compression; is sensitive to a one-byte content change and to a member rename | unit/proptest | `cargo test -p ignition-core --lib deploy::projects::digest` | Wave 0 |
| DPRJ-02 | Parent absent on gateway refuses `project_parent_invalid{missing}`; self-parent and repo-parent cycle refuse `{cycle}`; non-inheritable parent per C8e; zero writes in each | engine | `cargo test -p ignition-core --test deploy_projects parent_` | Wave 0 |
| DPRJ-02 | `live == artifact` canonical digest => action `skip`, no export file, no import request | engine | `cargo test -p ignition-core --test deploy_projects unchanged_project_is_skipped` | Wave 0 |
| DPRJ-02 | Release ordering via semver; older refuses `destructive_change_refused{gate:downgrade}` unless `--allow-downgrade` (recorded); equal allowed | unit + engine | `cargo test -p ignition-core --lib deploy::projects::release` and `... --test deploy_projects downgrade_` | Wave 0 |
| DPRJ-02 / D-12 | No record and plan would change => `destructive_change_refused{gate:first_deploy}`; no record and digests equal => proceeds, writes the baseline record | engine | `cargo test -p ignition-core --test deploy_projects first_deploy_` | Wave 0 |
| DPRJ-03 | Pre-image export lands in `run_dir/export/<name>.zip` (0600) BEFORE the import request (request-order assertion); post-import export digest equals artifact digest => `applied` | engine | `cargo test -p ignition-core --test deploy_projects export_before_import_and_verify` | Wave 0 |
| DPRJ-03 | Post-import digest differs => success envelope, `data.ok:false`, `data.slug:deploy_project_unverified`, exit 6, record NOT written, no restore request | engine + CLI | `cargo test -p ignition-core --test deploy_projects verify_mismatch_` and `cargo test -p ignition-cli --test contract_deploy projects_unverified_exit_6` | Wave 0 / extend |
| DPRJ-03 | Import answering 200 `success:false` => `import_denied`; import answering `changes:[]` for a non-empty plan => failure (F1 backstop) | engine | `cargo test -p ignition-core --test deploy_projects import_denial_` | Wave 0 |
| DPRJ-04 | Gateway projects other than the target appear in `unmanaged` (names only); the fake records NO delete request across every scenario (`project_delete` is `unreachable` in the `ForbidWrites` double) | engine | `cargo test -p ignition-core --test deploy_projects unmanaged_never_deleted` | Wave 0 |
| DPRJ-04 | `.git` as directory, as file, as symlink refuses `project_git_managed`; absent passes; unreadable parent => `transport_failed` (never "absent"); node with no ssh/local refuses | transport/unit | `cargo test -p ignition-core --lib deploy::transport::projects` and `cargo test -p ignition-core --test deploy_transport project_host_` | Wave 0 |
| DPRJ-05 | `--check` makes zero gateway writes (ForbidWrites), zero host writes, creates no `.ign/`, exits 0 with `would_change` and the plan; works without `--yes` and without a hash | engine + CLI | `cargo test -p ignition-core --test deploy_projects check_is_read_only` and `cargo test -p ignition-cli --test contract_deploy projects_check_` | Wave 0 / extend |
| SC-5 | `deploy projects` is in `GUARDED_OPS` with a `// guarded:deploy projects` marker (both drift directions) | unit | `cargo test -p ignition-cli guarded_ops_registry_tracks_every_dispatch_site` | exists, goes red until added |
| SC-5 | Absent from the MCP catalog; guarded-and-excluded set equals `{edit, deploy apply, deploy rollback, deploy projects}`; read-only deploy leaves still present | unit | `cargo test -p ignition-cli deploy_projects_is_excluded_by_full_path` and `deploy_rollback_is_excluded_by_full_path` | extend |
| SC-5 | README Commands row, `### deploy projects`, TUI route row, exit-table rows for the three new slugs, dated verdict exception, verdict slug not in table | doc/coverage | `cargo test -p ignition-cli --test deploy_family_coverage` | exists, extend |
| SC-5 | Exit table agreement for new slugs; hint rule | unit | `cargo test -p ignition-core --lib error::tests::exit_code_mapping_enumerated error::tests::readme_exit_table_agreement` | exists, extend |
| SC-5 / DSURF-03 | Secret canary: a canary string in a project resource of the artifact AND in the live export never appears in stdout JSON, human output, `run.json`, `plan.json`, `events.ndjson`, `result.json`, errors; the only file allowed to hold it is `export/<name>.zip` (assert it does, like `assert_exempt_holds_canary`) | engine + CLI | `cargo test -p ignition-core --test deploy_projects canary_` and `cargo test -p ignition-cli --test contract_deploy projects_canary` | Wave 0 / extend |
| D-06 | Parent deploy sweeps children by `parent` field; child with matching record => ok, mismatch => warning only (deploy still `applied`, exit 0), no record => `unverified`; zero writes to children | engine | `cargo test -p ignition-core --test deploy_projects children_sweep_` | Wave 0 |
| D-07 | `--targets DIR` makes DIR the root for BOTH `targets/<env>.yaml` and the target's `known_hosts` path (see Open Question 8); with no flag both still resolve against `--repo`; a `known_hosts` that escapes DIR is refused by the existing `known_hosts_problem` | unit/CLI | `cargo test -p ignition-core --lib deploy::target` and `cargo test -p ignition-cli --test contract_deploy projects_targets_dir` | Wave 0 (new tests; `load_target` and `node_access` already take the base dir as a parameter) |
| D-08/D-09 | `deploy/project.yaml` strict (unknown field, wrong schema_version, CR, env not listed); `.ignexclude` grammar incl. CRLF tolerance | unit | `cargo test -p ignition-core --lib deploy::projects::project_file deploy::projects::exclude` | Wave 0 |
| Live SC-1..4 | create, skip, import with verify, parent missing, `.git` (dir created on the rig host), downgrade, first-deploy, verify of the real re-export equality | live | `IGNITION_LIVE_DEPLOY=1 ... cargo test -p ignition-core --test live_deploy_projects -- --ignored` | Wave 0 (needs rig) |
| Installed binary | after `cargo install --path crates/ignition-cli --force`, `ign deploy projects --check` against the rig | manual-scripted | `bash` step in the live-gate plan (installed `ign` on PATH is not the repo build) | Wave 0 |

### Sampling Rate
- **Per task commit:** the quick command for the module touched (`cargo test -p ignition-core --lib deploy::projects`, `... --test deploy_projects`, or `cargo test -p ignition-cli --test deploy_family_coverage`), expected under 30 s once built.
- **Per wave merge:** `cargo test -p ignition-core` and `cargo test -p ignition-cli --test contract_deploy --test deploy_family_coverage --test tui_coverage`; plus `cargo test -p ignition-cli --lib` for the guarded-ops/MCP drift tests.
- **Phase gate:** `cargo test --workspace` green, `cargo clippy --workspace -- -D warnings` clean (the workspace enforces it, and `CoreError` size matters: boxing large payloads is the 15-01 lesson [CITED: PITFALLS.md:487]), `cargo test --workspace --no-default-features` for the headless build, then the live gate (manual, never CI), before `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] `crates/ignition-core/src/deploy/projects.rs` with its unit-test modules (artifact, digest, project_file, exclude, release, graph)
- [ ] `crates/ignition-core/tests/deploy_projects.rs` engine suite
- [ ] `FakeGateway` project store in `tests/common/fake_deploy.rs` (list/find/export/import, `lastModification` stamping, switches for "import adopts nothing", "overwrite deletes absent", "import denies"), `ForbidWrites` extended for project writes (the engine tests need to assert no `project_import`/`project_delete`/modify request in `--check` and in refusals)
- [ ] Host fakes for `ProjectHost` (local tempdir with `.git` as dir/file/symlink; sftp `FakeSftp` cases; Windows listing case)
- [ ] `25-LIVE-CAPTURES.md`, `COVERAGE.md`, fixture scripts (`capture25.sh` in `live-rig-deploy/`)
- [ ] `crates/ignition-core/tests/live_deploy_projects.rs`

## Security Domain

> `security_enforcement` is enabled (not set to false) and `security_asvs_level` is 1 [VERIFIED: .planning/config.json].

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no (reuses the profile secret chain; no new credential path) | existing `named_profile_client` |
| V3 Session Management | no | - |
| V4 Access Control | yes | `ign` never deletes projects or `.git`; the deploy account gets list-only (`r-x`) on `data/projects`, documented least-privilege grant; guarded `--yes`; not an MCP tool |
| V5 Input Validation | yes | `strict_parse` for `deploy/project.yaml`; `component_problem` on project/env names; semver parse for the tag; size and entry caps on the zip; zip member names treated as untrusted (hashed, never extracted) |
| V6 Cryptography | yes | sha256 via `sha2` only; no custom hashing; FNV never used as a gate |
| V7 Logging/Error | yes | canary tests: output, errors, events and run files carry hashes, counts and names, never file content; the pre-import export (secret-bearing) is the only exempt file and is 0600 in a 0700 directory |
| V12 Files | yes | artifact walk refuses symlinks and special files, does not follow links outside the checkout, bounded size; run dir git-ignore guard; no `.ign/` in the zip |

### Known Threat Patterns for this stack
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Symlink in the checkout points outside the repo (zip exfiltrates a host file into the gateway) | Information disclosure | Refuse symlinks and special files in the walk; canonicalise and assert each file is under the repo root |
| Hostile `project.json` `parent` or project name (path/argument injection) | Tampering | `component_problem`, percent-encoding via the existing `encode_segment`, no shell interpolation (ssh argv is `RemoteCommand::posix`/`powershell_encoded` with validated components) |
| Secret echoed in an error/result (project scripts can embed credentials) | Information disclosure | Results carry names, counts and hashes; `import_denied` problem text is bounded as today; canary suite incl. a canary inside a script resource |
| Silent partial import / data loss (F1, truncated zip) | Tampering / DoS | `validate_import`, empty-set refusal, `changes:[]` refusal, pre-import export, post-import digest |
| Replay of an old hash / wrong-gateway deploy | Spoofing | hash binds artifact bytes only, so also keep `wrong_target`-style gateway-name binding from the target (`gateway_name`) in the preflights; the run dir and record name the gateway |
| CI token with write to prod run by an untrusted branch | Elevation of privilege | Out of `ign`'s hands; README states the same-org read-only token for the IaC checkout and environment approval for prod (D-02, D-07) |
| Concurrent deploys of the same project | Tampering | workflow `concurrency` group; record replace-by-rename |

## Project Constraints (from ./.claude/CLAUDE.md)
- Tech stack: Rust, clap, ratatui; keep the dependency tree lean (no new crates here).
- Every subcommand non-interactive by default with `--json`; TUI opt-in. `deploy projects` needs a TUI route row (`Screen::Projects`).
- Compatibility: Ignition 8.3.1+ only.
- Do NOT create git worktrees that share the cargo target dir; agents and GSD tasks run sequentially on the main checkout (`workflow.use_worktrees=false`). If a worktree is unavoidable, set a unique `CARGO_TARGET_DIR` first. `rm` is wrapped and rejects `-rf` (use `/bin/rm`).
- `ign` on PATH is the installed binary: run `cargo install --path crates/ignition-cli --force` before any manual smoke check of a changed verb.
- The exit-code table source of truth is the README plus `crates/ignition-core/src/error.rs`, pinned by `readme_exit_table_agreement` and `exit_code_mapping_enumerated`; read both before specifying any exit code.
- GSD workflow enforcement: file changes go through a GSD command; no direct repo edits outside it.
- User global rules: use `uv` for any Python (none needed here); no emojis in output; foundry boundary applies only if work is handed to non-Claude/Codex models (none planned).

## Sources

### Primary (HIGH confidence)
- In-repo files read this session: `.planning/phases/25-guarded-project-deploy/25-CONTEXT.md` and `25-DISCUSSION-LOG.md`; `.planning/ROADMAP.md` Phase 25 section; `REQUIREMENTS.md` DPRJ rows; `STATE.md`; `.planning/config.json`; `.planning/research/{SUMMARY,ARCHITECTURE,PITFALLS}.md` (project-deploy sections); `23-CONTEXT.md`; `24-LIVE-CAPTURES.md`, `24-RESEARCH.md` (validation section), `COVERAGE.md` (phase 24); `crates/ignition-core/src/deploy/{manifest,target,ledger,yamlio,mod}.rs`; `deploy/apply/{mod,run_store,preflight,host}.rs`; `deploy/transport/mod.rs` and the `sftp.rs` ledger/journal readers; `actions/{projects,deploy_apply,resources}.rs`; `client/{mod,projects,resources,workspace}.rs`; `crates/ignition-cli/src/{cli,main,mcp}.rs` (deploy dispatch, `GUARDED_OPS`, exclusion tests); `crates/ignition-tui/src/routes.rs`; `crates/ignition-cli/tests/deploy_family_coverage.rs`; `README.md` exit-code table and deploy apply sections; `.github/workflows/deploy-apply.yml`; `Cargo.toml`.
- `gsd-tools query package-legitimacy check --ecosystem crates semver sha2 zip` -> OK x3.
- Spike results: `whk-environment-orchestration` branch `docs/prod-gateway-config-as-code`, `spikes/gateway-modes/results/S5.md` (read via `git show`), spec §5.2, §6, §13, §15.

### Secondary (MEDIUM confidence)
- `~/data/projects/WHK-Global` checkout (layout, `project.json`, `.github/ci/git.yaml.tmpl`, `trigger-prod-pull.sh`, `.planning/codebase/STACK.md`) read this session; it is a working copy, not necessarily identical to prod.
- `.planning/milestones/v1.0-phases/05-webdev-backend-tag-operations/{05-RESEARCH,05-UAT,05-07-PLAN}.md` for live 8.3.3 import/export behaviour (older gateway version than 8.3.6).
- Inductive Automation docs search (project export/import, gateway folder structure): did not state the export zip's internal layout; forum thread on 8.3 Git deployment mentions `data/config/resources/core/ignition/images`.

### Tertiary (LOW confidence, validate via captures)
- Gateway behaviour for a checkout-layout zip (C8c), unlisted members (C8d), parent edge cases (C8e), git module interference (C10), Windows hidden `.git` (C-H).

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH, no new dependencies, versions read from `Cargo.toml`/`Cargo.lock`.
- Architecture and surface parity (guard, MCP, TUI, README, exit table, run store, Decision C): HIGH, every seam read and cited.
- Gateway import/export behaviour for a checkout-built zip: LOW until C8c/C8d are captured (flagged F1, F2).
- Host capabilities (`.git` probe, release record, account rights): MEDIUM, design follows the ledger/journal reader pattern but the Windows and rights behaviour is unverified (F3, C-H).
- Pitfalls: MEDIUM-HIGH, drawn from in-repo live findings plus the v1.6 pitfall catalogue.

**Research date:** 2026-10-22
**Valid until:** 2026-11-21 for in-repo facts (the code moves per phase; re-check line numbers after Phase 24 follow-ups land); the gateway-behaviour items are superseded as soon as 25-01 records captures.
