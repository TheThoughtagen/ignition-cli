# Phase 10 — Live Rig Ops Log (10-01)

**Purpose:** Ops record for the two disposable rigs spun to capture the EAM task-lifecycle + config-mutation wire truth (10-LIVE-CAPTURES.md). Follows the proven 09-RIG-NOTES recipe. No product code touched. **No secret material in this file** — token KEYS never entered the repo; they ride a chmod-600 scratch file (`/tmp/ign-p10-rigs/tokens.env`, outside the repo, to be shredded at teardown). Only token NAMES appear below.

## Rigs

| Rig | Image | Host port | Container | Created (UTC) | Commissioned RUNNING (UTC) | Token (NAME only) |
| --- | ----- | --------- | --------- | ------------- | -------------------------- | ----------------- |
| A | `inductiveautomation/ignition:8.3.6` | 18188 → 8088 | `ign-p10-836` (ephemeral docker run) | 2026-09-09T09:46:20Z | 2026-09-09T09:48:29Z | `p10tok836` |
| B | `inductiveautomation/ignition:8.3.3` | 19188 → 8088 | `ign-p10-833-ignition-1` (compose project `ign-p10-833`, dir `/tmp/ign-p10-rigs/ign-p10-833/`) | 2026-09-09T09:46:12Z | 2026-09-09T09:48:29Z | `p10tok833` |

Pre-flight: ports 18188/19188 confirmed free; both images already cached locally (Phases 4–9). **Prior-session contamination found and cleaned:** an `ign-p10-836` container + a commissioned `ign-p10-833_ign-p10-833_gateway_data` volume from an aborted 2026-09-08 attempt at this same plan were still on disk (OrbStack was down when this session started; `docker rm -f -v` + `docker compose down -v` wiped both before fresh boots — clean-rig provenance guaranteed).

## Launch commands

Rig A (ephemeral `docker run`, Phase-4 pattern verbatim):

```bash
docker run -d --name ign-p10-836 -p 18188:8088 \
  -e ACCEPT_EULA=Y -e GATEWAY_ADMIN_PASSWORD=password -e IGNITION_EDITION=standard \
  inductiveautomation/ignition:8.3.6
```

Rig B (own compose project, Phase-4 pattern — `docker-compose.yml` in `/tmp/ign-p10-rigs/ign-p10-833/`: image `inductiveautomation/ignition:8.3.3`, port `19188:8088`, env `ACCEPT_EULA=Y` / `GATEWAY_ADMIN_PASSWORD=password` / `IGNITION_EDITION=standard`, named volume `ign-p10-833_gateway_data:/var/lib/ignition/data`):

```bash
docker compose -p ign-p10-833 --project-directory /tmp/ign-p10-rigs/ign-p10-833 up -d
```

## Commissioning — the 09-02 headless wire recipe, re-proven

Both rigs booted to `{"state":"RUNNING","details":"COMMISSIONING"}` within ~30 s (warm image layers, 09-06 behavior). Headless commissioning verbatim: `GET /bootstrap` (both reported `{"steps":{"eula":"license"},…}`) → `POST /post-step` eula-accept (**201** both) → `POST /post-step` start-gateway (**200** both, `{"gatewayAddress":"http://localhost:8088"}`) → poll `/StatusPing` until plain `{"state":"RUNNING"}`. Timeline: EULA 09:47:44Z → both RUNNING **09:48:29Z** (~30 s this run).

## Headless API-token provisioning (Phase-4 recipe, third rig-generation proof)

`/tmp/ign-p9-rigs/provision_token.sh URL admin password NAME` (unchanged script). Names `p10tok836` / `p10tok833`; keys staged immediately after provisioning into `/tmp/ign-p10-rigs/tokens.env` (chmod 600, outside the repo) and sourced per command batch — the 09-02 lesson applied from the start (keys staged in the SAME shell batch that provisioned, no lost-key second pass needed).

Sanity per rig: `GET /data/api/v1/gateway-info` with `X-Ignition-API-Token: NAME:key` → **200 on both**. (One operator stumble recorded honestly: the first sanity pass sent the bare key without the `NAME:` prefix → 401 on both; the header takes the FULL `name:key` string per 04-VERIFICATION — fixed in the next batch, zero impact.)

## EAM controller-mode provisioning — NEW, rung (a) ACHIEVED on both rigs

The 10-01 plan's fallback ladder: this session landed **rung (a) — headless controller provisioning worked on both rigs**. Recipe (the 07-RESEARCH "controller flip", now scripted):

1. **Probe landscape:** `GET /data/api/v1/resources/com.inductiveautomation.eam/module-settings` has NO GET route (Jetty 404 HTML, `No route match for path: /v1/resources/com.inductiveautomation.eam/module-settings`, both rigs — the extract declares only PUT/POST/… for that path). The singleton read rides the security-properties pattern: **`GET /data/api/v1/resources/singleton/com.inductiveautomation.eam/module-settings?collection=core`** → 200 with the full singleton record (byte-identical on both fresh rigs): `config.installMode: "NotInstalled"`, `config.controllerSettings{eventTableName:"agent_events", archiveLocationMode:"Automatic", lowDiskThresholdMB:1024, maxRetainedBackupCount:5, backupRetentionAge:0, backupRetentionTimeUnit:"Days"}`, `config.agentSettings{sendStatsInterval:45, httpConnectTimeout:10, httpReadTimeout:60, forwardLeasedLicense:false}`.
2. **Flip:** clone the full record, mutate ONLY `config.installMode` → `"Controller"`, PUT array body `[{"name":"module-settings","type":<record.type>,"collection":"core","enabled":true,"description":"","signature":<record.signature>,"config":<record.config>}]` to `/data/api/v1/resources/com.inductiveautomation.eam/module-settings?collection=core` → **200 `{"success":true,"changes":[{"name":"module-settings","type":"com.inductiveautomation.eam/module-settings","collection":"core","newSignature":<…>}],"problem":null}`** on both.
3. **Verify:** singleton read-back `installMode = "Controller"` both rigs; `GET /data/eam/api/v1/eam-tasks/scheduled/false` now **200** `{"items":[],"metadata":{"total":0,"matching":0,"limit":-1,"offset":0}}` (was 403-HTML on stock gateways per 07-RESEARCH) — runtime surface activated instantly, no gateway restart.

**Notable capture-side observation:** the two rigs returned DIFFERENT `newSignature` values (8.3.6 `05a31768…`, 8.3.3 `214aac30…`) for the byte-identical flip body — signatures are not purely content-derived (timestamp/actor-involved); treat as opaque server-owned strings. Also: fresh-rig module-settings records carry `attributes.lastModification` + `attributes.lastModificationSignature` (gateway-written, same volatility the 07-01 diff research documented for resource.json).

## Scratch task creation (per rig)

Array POST to `/data/api/v1/resources/com.inductiveautomation.eam/eam-tasks` (create, config.settings PRESENT per the eam-create-422 trap):

```json
[{"name":"ign-p10-scratch","collection":"core","enabled":true,"description":"phase10 capture scratch task","config":{"profile":{"type":"eam_backup","scheduleMode":"OnDemand"},"settings":{"targetGateways":["_controller"],"targetGroups":[],"concurrentBackups":0,"forceBackups":false}}}]
```

Both rigs: **200** `{"success":true,"changes":[{…,"newSignature":<…>}],"problem":null}`; find read-back 200 with `config.profile.type=="eam_backup"`, `config.profile.isSuspended:false` (server-added on create — never sent), `version:1`, `attributes.uuid` (server-generated, distinct per rig), and `healthchecks.scheduledTaskState.result.details` = `{"repeats":false,"nextScheduled":"On Demand","currentState":"Stopped","erroredServers":"","owner":"eam"}` — note `currentState:"Stopped"` and `healthy:true` on a fresh OnDemand task (the v1.0 `cli-research-backup` witness had `"Errored"`/`healthy:false`; fresh-rig baseline now captured).

## Fallback ladder record

**Rung (a) achieved on both rigs** — headless controller provisioning worked; no stock-gateway 403 captures needed for the ladder (the 403 shape evidence remains available in 07-RESEARCH/09 captures and, if encountered in Task 2's probes, will be captured verbatim then). WHK controller-rig route not needed for provisioning.

## Capture run (Task 2, 2026-09-09 09:56–10:04 UTC)

All 12 probes executed per rig (probe list + verbatim evidence: 10-LIVE-CAPTURES.md). Wire-level notes that live in the ops domain rather than the captures doc:

- **Token staging held** for the whole run: keys sourced from the chmod-600 scratch env per batch, never printed, never written to the repo. One early 401 was the FULL `name:key` lesson (header takes the complete string), fixed within one batch.
- **Gateway non-strict JSON gotcha:** `find` responses whose healthcheck carries the stats NPE embed RAW TAB bytes inside `error.stacktrace` strings — python `json.load` and `jq` both reject them (`strict=False` works). Probe scripts switched to lenient parsing mid-run; the CLI's own parser must tolerate this (locked in captures Decision 10).
- **Probe-spec correction recorded:** the plan's delete probe suggested `?confirm=true&collection=eam-tasks`; live wire proves `collection=` takes the COLLECTION name (`core`) — the `eam-tasks` value 404s. Captured both ways (captures §3c).
- Mutations were confined to the two `ign-p10-*` scratch tasks + the module-settings flip; `names` lists ended empty on both rigs before teardown.

## Teardown (2026-09-09 10:05Z)

```bash
docker rm -f -v ign-p10-836
docker compose -p ign-p10-833 --project-directory /tmp/ign-p10-rigs/ign-p10-833 down -v --remove-orphans
```

Verified: zero `ign-p10*` containers/volumes (`docker ps -a` / `docker volume ls` greps empty); ports 18188/19188 freed (`lsof` empty); `/tmp/ign-p10-rigs/tokens.env` — the only file that ever held this run's key material — destroyed with `shred -u`; the stale Phase-9 `/tmp/ign-p9-rigs/tokens.env` (dead key material from 09 teardown leftovers) also shredded as hygiene. Nothing left running.

---

*Executed: 2026-09-09, autonomous capture-first run for phase 10 plan 01 (GSD executor). Rigs/recipe sections above are the Task-1 provisioning log (committed before captures began).*
