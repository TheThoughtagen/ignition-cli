# API Coverage — Ignition 8.3 gateway project routes (Phase 25)

> Full coverage by default. Opt-outs are explicit, reasoned decisions.
> Surface source: the local 8.3.6 OpenAPI (`~/.cache/ign-deploy-rig/openapi-8.3.6.json`), every path matching `/projects`, `/scan`, `/scan-lock`, `/designers` or `gateway-info`.
> Phase 25 reads the project list, one project, the designer sessions and the gateway name, exports the live project before an overwrite and sends exactly one write, the project import, per deploy. Every INTEGRATE row is a method on `GatewayApi` that already exists, proven by the 25-01 and 25-02 captures and pinned by a wiremock contract test in a later plan.

| capability | decision | reason |
|---|---|---|
| projects.list | INTEGRATE | |
| projects.find | INTEGRATE | |
| projects.export | INTEGRATE | |
| projects.import | INTEGRATE | |
| designers.list | INTEGRATE | |
| gateway-info | INTEGRATE | |
| projects.create | OPT-OUT | import with `overwrite=false` creates the project (K10, D-13), so the separate create route (`POST /projects`) adds nothing a deploy needs |
| projects.copy | OPT-OUT | explicitly out of scope: a deploy never copies a project |
| projects.rename | OPT-OUT | explicitly out of scope: a deploy never renames a project |
| projects.modify | OPT-OUT | explicitly out of scope: D-05, a deploy never modifies or reparents any project; the `parent` comes from the artifact's `project.json` through the import |
| projects.delete | OPT-OUT | explicitly out of scope: DPRJ-04 and the roadmap, a deploy never deletes a project |
| projects.names | OPT-OUT | not needed: `projects/list` carries the same names with the fields the checks read (K6) |
| projects.parents | OPT-OUT | not captured in Phase 25: the parent, cycle and inheritable checks read `parent` and `inheritable` from `projects/list` (K6) and the gateway's own `invalidParent` flag |
| scan.projects | OPT-OUT | explicitly out of scope: no capture needed a project scan after an import (C8, C9 and C10 all took effect at once), and a scan of the gateway is not a deploy step |
| scan-lock.projects | OPT-OUT | explicitly out of scope: locking the project scan is gateway administration, not part of a project deploy |

## Route key

| capability | route |
|---|---|
| projects.list | `GET /data/api/v1/projects/list` (existing `GatewayApi::projects`; also the source for the parent checks and the children report) |
| projects.find | `GET /data/api/v1/projects/find/{name}` (existing `GatewayApi::project_find`) |
| projects.export | `GET /data/api/v1/projects/export/{name}` (existing `GatewayApi::project_export_to_file`; the live and pre-image digests) |
| projects.import | `POST /data/api/v1/projects/import/{name}` (existing `GatewayApi::project_import`, with its 200-denial check; `overwrite=false` creates) |
| designers.list | `GET /data/api/v1/designers` (existing `GatewayApi::designers`) |
| gateway-info | `GET /data/api/v1/gateway-info` (existing `GatewayApi::gateway_info`) |
| projects.create | `POST /data/api/v1/projects` |
| projects.copy | `POST /data/api/v1/projects/copy` |
| projects.rename | `POST /data/api/v1/projects/rename/{name}` |
| projects.modify | `PUT /data/api/v1/projects/{name}` |
| projects.delete | `DELETE /data/api/v1/projects/{name}` |
| projects.names | `GET /data/api/v1/projects/names` |
| projects.parents | `GET /data/api/v1/projects/parents` and `GET /data/api/v1/projects/parents/{name}` |
| scan.projects | `GET` / `POST /data/api/v1/scan/projects` |
| scan-lock.projects | `GET` / `POST /data/api/v1/scan-lock/projects` |
