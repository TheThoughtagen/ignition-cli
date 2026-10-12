# API Coverage — Ignition 8.3.6 config-resource / scan-lock / scan / mode REST family (Phase 20)

> Full coverage by default. Opt-outs are explicit, reasoned decisions.
> Surface source: the gateway OpenAPI (737 paths, `whk-environment-orchestration/gateway-config-spec/docs/openapi.json`), re-checked against the live 8.3.6 document in capture C1h (`20-LIVE-CAPTURES.md`).
> Phase 20 is library-only: every INTEGRATE row is a `DeployApi` method (or a pure decision over an existing read), proven by a live capture and pinned by a wiremock contract test.

| capability | decision | reason |
|---|---|---|
| resources.list | INTEGRATE | |
| resources.names | INTEGRATE | |
| resources.find (effective view and ?collection=) | INTEGRATE | |
| resources.singleton (?collection=) | INTEGRATE | |
| resources.create (POST, array body, name only) | INTEGRATE | |
| resources.update (PUT, array body, name + signature) | INTEGRATE | |
| resources.delete | OPT-OUT | not needed in Phase 20 — no DAPI requirement deletes via the API; deploy deletes use the file transport's bounded delete set (Phases 22/23) |
| resources.update allowInvalidReferences param | OPT-OUT | not needed yet — Phase 22 verify decides whether reference validation rides the read-back |
| scan-lock.info (config, projects) | INTEGRATE | |
| scan-lock.acquire (config, projects) | INTEGRATE | |
| scan-lock.release | OPT-OUT | the route does not exist in the 8.3.6 OpenAPI; capture C1g/C1h records the live probe (D8) |
| scan.status (config, projects) | INTEGRATE | |
| scan.trigger (config, projects) | INTEGRATE | |
| mode.list | INTEGRATE | |
| mode.create | INTEGRATE | |
| mode.modify | OPT-OUT | not needed yet — no v1.6 requirement renames or retitles a mode |
| mode.delete | OPT-OUT | explicitly out of scope — deleting a mode is destructive and no v1.6 requirement asks for it |
| gateway-info.deploymentMode (private probe struct) | INTEGRATE | |
| redundancy.status active-node decision | INTEGRATE | |
| redundancy.resync | OPT-OUT | not needed yet — Phase 24 (DPAIR-01) owns the bounded resync |
| redundancy.failover | OPT-OUT | explicitly out of scope — v1.6 never restarts or fails over a gateway (DAPL-10) |
| restart-tasks.pending | OPT-OUT | not needed yet — Phase 23 reports restart_pending |

## Route key

| capability | route |
|---|---|
| resources.list | `GET /data/api/v1/resources/list/{module}/{type}` |
| resources.names | `GET /data/api/v1/resources/names/{module}/{type}` |
| resources.find | `GET /data/api/v1/resources/find/{module}/{type}/{name}` |
| resources.singleton | `GET /data/api/v1/resources/singleton/{module}/{type}` |
| resources.create / update | `POST` / `PUT /data/api/v1/resources/{module}/{type}` |
| resources.delete | `DELETE /data/api/v1/resources/{module}/{type}/{name}/{signature}` |
| scan-lock.* | `GET` / `POST /data/api/v1/scan-lock/{config,projects}` |
| scan.* | `GET` / `POST /data/api/v1/scan/{config,projects}` |
| mode.list / create | `GET` / `POST /data/api/v1/mode` |
| mode.modify / delete | `PUT` / `DELETE /data/api/v1/mode/{name}` |
| gateway-info.deploymentMode | `GET /data/api/v1/gateway-info` |
| redundancy.status | `GET /data/api/v1/redundancy` (existing `GatewayApi::redundancy_status` + pure `active_node`) |
