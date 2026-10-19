# API Coverage — Ignition 8.3.6 redundancy and secret-provider listing routes (Phase 24)

> Full coverage by default. Opt-outs are explicit, reasoned decisions.
> Surface source: the local 8.3.6 OpenAPI (`~/.cache/ign-deploy-rig/openapi-8.3.6.json`), every path matching `redundancy`, `secret-providers` or `gateway-network-incoming/toggleApproval`.
> Phase 24 reads the backup and sends exactly one write, the resync, to the master. Every INTEGRATE row is a read on `DeployApi` / `GatewayApi` (or the one master-only POST), proven by a 24-01/24-02 capture and pinned by a wiremock contract test.
> Rows marked "re-decided at 24-02" are settled by capture C-P8 and the 24-02 contract checkpoint (item K5); 24-02 Task 2 edits them in place.

| capability | decision | reason |
|---|---|---|
| redundancy.status | INTEGRATE | |
| redundancy.resync | INTEGRATE | |
| redundancy.providers | INTEGRATE | |
| redundancy.events | OPT-OUT | not needed while redundancy.providers carries the sync-exhausted signal (D-10); re-decided at 24-02 from capture C-P8 |
| redundancy.config.read | OPT-OUT | not needed: the backup check reads status, mode, effective view and files; the rig setup script reads it with curl (test fixture only, D-15) |
| redundancy.config.write | OPT-OUT | explicitly out of scope: a write to a gateway's redundancy configuration; only the rig setup script sets it, with curl, on a disposable rig |
| redundancy.failover | OPT-OUT | explicitly out of scope: no verb fails over a gateway and nothing restarts inside apply (SF-3) |
| secret-providers.secrets.list | INTEGRATE | |
| gateway-network-incoming.toggleApproval | OPT-OUT | explicitly out of scope for `ign`: approving or cutting a gateway-network link is rig setup and the D-17 live gate's link cut, done with curl in the fixture scripts |
| gateway-network-incoming.reset | OPT-OUT | explicitly out of scope: a write that resets a gateway-network connection; no requirement asks for it |

## Route key

| capability | route |
|---|---|
| redundancy.status | `GET /data/api/v1/redundancy` (existing `GatewayApi::redundancy_status`, read on both nodes) |
| redundancy.resync | `POST /data/api/v1/redundancy/gwaction/resync` (new `DeployApi::redundancy_resync`, master session only, never retried) |
| redundancy.providers | `GET /data/api/v1/redundancy/providers` (new `DeployApi::redundancy_providers` when K5 = a) |
| redundancy.events | `GET /data/api/v1/redundancy/events` |
| redundancy.config.* | `GET` / `PUT /data/api/v1/redundancy/config` |
| redundancy.failover | `POST /data/api/v1/redundancy/gwaction/failover` |
| secret-providers.secrets.list | `GET /data/api/v1/secret-providers/{provider-name}/secrets` (new `DeployApi::secret_names`, names only) |
| gateway-network-incoming.toggleApproval | `PUT /data/api/v1/gateway-network-incoming/toggleApproval/{remoteId}/{approve}` |
| gateway-network-incoming.reset | `PUT /data/api/v1/gateway-network-incoming/reset/{resourceName}` |
