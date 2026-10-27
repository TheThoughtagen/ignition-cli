# Phase 25: Guarded Project Deploy - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-10-21
**Phase:** 25-guarded-project-deploy
**Areas discussed:** Fleet model, Artifact and pin, Parents across repos, Repo layout and ownership, Git module cutover

---

## Fleet model

| Option | Description | Selected |
|--------|-------------|----------|
| Spec §15: env repo deploys | whk-environment-orchestration holds gateway config and one manifest per gateway pinning every project | |
| Each repo deploys its own | WHK-Global deploys tags plus its project; each project repo deploys its own project | ✓ |
| Hybrid | Env repo owns gateway config; project repos deploy themselves | |

**User's choice:** "the env repo doesn't actually get used though for what prod is." Confirmed: each project repo deploys itself from CI, WHK-Global also deploys tags.
**Notes:** The user first described the model as "WHK-Global would deploy tag changes, the others wouldn't" (meaning the others would not deploy tags). Spec §15 F1/F2 is on an unmerged docs branch and not adopted.

---

## Artifact and pin

| Option | Description | Selected |
|--------|-------------|----------|
| Zip the checkout | Build the zip at run time; plan reports the hash, apply requires it | ✓ |
| Release artifact + pin | Release job builds the zip; manifest pins its sha256 | |
| Support both | Checkout mode plus pinned mode | |

| Option | Description | Selected |
|--------|-------------|----------|
| Merge to production branch | Same trigger as the git module today | |
| Release tag | Pushing a version tag deploys | ✓ |
| Manual dispatch | Run by hand | |

| Option | Description | Selected |
|--------|-------------|----------|
| Promote through envs | One tag runs dev then prod, each gated by its environment | ✓ |
| Tag pattern picks env | rc tags to non-prod, release tags to prod | |
| Prod only | Tags deploy to prod only | |

| Option | Description | Selected |
|--------|-------------|----------|
| Refuse unless flagged | Older release than deployed needs an explicit flag | ✓ |
| Allow, hash decides | Any tag deploys | |

---

## Parents across repos

| Option | Description | Selected |
|--------|-------------|----------|
| Require it exists | Refuse when the parent is absent on the gateway; never touch it | ✓ |
| Require a minimum release | Also check the parent's recorded release | |
| No check | Import anyway | |

| Option | Description | Selected |
|--------|-------------|----------|
| Nothing | Children pick up inheritance on their own | |
| Verify children | Hash-check children after a parent deploy | ✓ |
| Redeploy children | Trigger children's pipelines | |

| Option | Description | Selected |
|--------|-------------|----------|
| Warn, parent stays | List mismatches; parent deploy succeeds | ✓ |
| Fail the parent deploy | ok=false, exit 6, no rollback | |
| Roll back the parent | Restore the parent's export | |

---

## Repo layout and ownership

| Option | Description | Selected |
|--------|-------------|----------|
| deploy/ with today's format | targets and manifest copied into every repo | |
| Shared targets repo | Targets in one repo checked out by each pipeline | ✓ |
| One new root file | New format | |

**User's choice:** "Shared - iac repo?" Confirmed as `WhiskeyHouse/whk-onprem-infra-iac`.

| Option | Description | Selected |
|--------|-------------|----------|
| Repo root minus excludes | Zip the root without .git, .github, deploy/, excludes | ✓ |
| Path named in manifest | Manifest names the project dir | |

| Option | Description | Selected |
|--------|-------------|----------|
| One job, tags first | Same tag and approval; tags then project | |
| Separate jobs | Tags and project deploy independently | ✓ |

| Option | Description | Selected |
|--------|-------------|----------|
| Workflow input | Caller workflow passes the project name | |
| deploy/project.yaml | Small reviewed file with name and envs | ✓ |
| Add a field to project.json | Not recommended | |

**Notes:** The user first asked "Just use project.json?". project.json holds only the display title (WHK-Global's is "Global"), so it cannot supply the gateway name.

---

## Git module cutover

| Option | Description | Selected |
|--------|-------------|----------|
| Per project, gated on C10 | Remove one project's .git; fall back to gateway-wide uninstall if C10 shows interference | ✓ |
| Gateway-wide first | Uninstall the module, then cut all projects over | |

| Option | Description | Selected |
|--------|-------------|----------|
| Must be a no-op | First ign deploy must hash-match what the git module left | ✓ |
| Normal deploy | Treat like any deploy | |

---

## Claude's Discretion

- Location and format of the per-project deployed-release record
- Flag names, staleness slug, extra `deploy/project.yaml` fields
- Built-in exclude list
- Whether `deploy-projects.yml` is a plan in this phase or a follow-up
- How release order is compared for the downgrade guard

## Deferred Ideas

- Deleting projects absent from any repo
- Spec §15 central env-repo manifest model
