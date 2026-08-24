# Phase 5: WebDev Backend & Tag Operations - Research

**Researched:** 2026-08-24
**Domain:** Ignition 8.3 WebDev module (route authoring, deployment, wire protocol) + tag/alarm/history scripting APIs via WebDev + native 8.3 config-resource REST
**Confidence:** HIGH (live-probed end-to-end on a fresh disposable 8.3.3 gateway with a deployed probe route; every claim below marked [LIVE] was executed against a real gateway, recorded with exact paths/payloads)

**Live-probe environment (reproducible):** fresh `inductiveautomation/ignition:8.3.3` container (`ign-p5-research`, port 28088), commissioned by restoring the git-module `base.gwbk` via `docker-entrypoint.sh -r /base.gwbk` (admin/password), API token provisioned with the Phase 4 headless recipe (`p5research:*`, X-Ignition-API-Token header). The WebDev module ships in the official image (`Web Developer Module.modl` present in both 8.3.3 and 8.3.6 images). ign-research (8.3.6, :18088) was up the whole time but its admin password is unknown and its trial is expired — its WebDev servlet answered **402** (module installed, unlicensed), which independently confirms the servlet path and module behavior on 8.3.6.

---

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions

**scriptExec security posture (user decision — LOCKED):**
- The scriptExec route MUST carry its own auth mechanism — it is never deployed wide-open relying solely on gateway session auth. Anyone with gateway access must not be able to invoke arbitrary script execution by mere route presence
- Exact mechanism (shared secret/token, config gate, permission probe) = the flagged script-exec security spike → research proposes, planner locks
- Deploy-default posture (skip-unless-requested vs always-deploy) = planner/research recommendation; the auth requirement holds in every posture
- `ign script run` command surfaces in Phase 7 (roadmap-sequenced, not deferred)

**Phase 3 `resource` family fate (user decision):**
- Research decides, steered HARD to the native Ignition 8.3 API first: the 575-path openapi extract (`.planning/phases/03-project-operations/openapi-8.3.6-phase3-extract.json`) is the evidence base — find real endpoints supporting per-resource operations and plan on them
- Only if the native API cannot support it does research choose among fallbacks: re-point onto Phase 5's own WebDev routes, export/import-machinery round-trip, or drop the family until Phase 7's decode/encode arrives
- Whichever path wins, the e2e witness approach re-points with it

### Derived Decisions (conventions, planner refines)

- Grouped subfamily pattern per established CLI conventions (`sessions`/`connections`/`logs` family precedent); exact verb naming at planner discretion
- `ignition_tools_summary.json` is the authoritative parity checklist; research resolves the 37-vs-42 divergence against live route behavior
- Agent-stable JSON shapes per the frozen envelope; all-family-keys-always convention inherited (filtered = `[]`, never key-hunt)
- Routes deploy into a dedicated project (exact name research/planner picks); never pollute user projects; `--project` override at planner discretion
- Version mismatch → refuse with actionable error per roadmap success criterion — no auto-upgrade magic
- Route versioning + handshake contract internals = research/planner territory

### Claude's Discretion

- Tree/table rendering for tag browse; quality/timestamp display conventions
- Route-internal versioning scheme and handshake shape
- Bulk transfer format details (json/xml/csv beyond roadmap minimum), collision-policy plumbing reusing Phase 3 conventions
- Alarm panel/table output shapes

### Deferred Ideas (OUT OF SCOPE)

- `webdev undeploy`/teardown command — not in roadmap success criteria; candidate for a later phase or backlog
- Live tag `watch` streaming command — Phase 6 TUI owns live-watch UX; CLI-side watch not roadmap-scoped here

</user_constraints>

## Summary

Phase 5 splits cleanly along a native/WebDev seam that live probing made crisp. **Native REST covers:** tag provider CRUD (TAGS-01: list/find/names/create/delete/rename via the `ignition/tag-provider` config-resource family — proven live including create and delete-by-signature), plus provisioning of historians (an `InternalHistorian` needs no database — created and healthy via REST). **Everything else — browse, read/write values, config CRUD, UDTs, alarms, history, bulk transfer (TAGS-02..09) — has NO native endpoint** (confirmed against the live 575-path openapi: zero tag-value/alarm-query/history-query routes) and must ride the CLI's own WebDev routes at `/system/webdev/{project}/...`, deployed as a project-zip import through the existing Phase 3 machinery.

The deployed-routes design is now evidence-backed rather than guesswork: a fresh disposable 8.3.3 rig ran a real probe route (imported via project zip, overwrite-replace verified) exercising the complete lifecycle — browse (dict-shaped results, `tagType` discriminator), configure (basePath + nested children + alarms-as-LIST), read/write (readBlocking/writeBlocking), getConfiguration (STRING arg, not list — the list form poisons the path parser), UDT types/instances, active alarms (`queryStatus` → eventId/source/state/priority), acknowledge (gateway scope requires the 3-arg `String[] eventIds, note, username` form), tag history (structurally works on default rigs; null data without a historian), and bulk transfer via `exportTags(tagPaths=[...])` payload → `configure(tags=[payload])` round-trip with values intact. Nine distinct ignition-mcp prior-art defects were found and corrected against live behavior — the replacement bar is lower than its tool count suggests; several of its tools never worked against a real 8.3 gateway.

The scriptExec security spike resolves to a concrete, prior-art-validated proposal: a deploy-time generated shared secret baked into the route folder, sent as a header, fail-closed compared in-route (WHK-Global's production `webdev_auth` pattern — constant-time compare, dual header shapes, "unconfigured = reject"), with `config.json require-auth`/`required-roles` as defense-in-depth (IMPORTANT: **API tokens do not authenticate WebDev require-auth routes** — live-proven 401 — so require-auth alone would lock the CLI's own token-authed calls out; the shared secret is the primary gate, not a nicety). The Phase 3 resource-family defect resolves to **export/import zip surgery** — the native steer's honest endpoint: no generic per-resource routes exist (re-verified against the live 575-path openapi plus the EAM endpoint which 403s without a controller), gateway scripts have no API for reading arbitrary project resources (killing the WebDev-hosting fallback), so `ign resource` keeps its UX but rides project-export zip member get/put/list, with e2e witnesses re-pointed to export-and-inspect.

**Primary recommendation:** one action-dispatch route per concern family (`tags`, `tagConfig`, `alarms`, `tagHistory`, `scriptExec` — five route folders mirroring the ignition-mcp endpoint split but with corrected semantics), deployed into a dedicated `ign-cli` project via the Phase 3 import path (fresh → clean create; existing → `overwrite=true` clean replace), versioned via a `version` handshake action on each route, with the CLI refusing WebDev-dependent commands on 405-missing or version-mismatch; scriptExec additionally gated by the baked shared secret and deployed only on explicit request.

---

## Parity Checklist Resolution (37 vs 42) — RESOLVED

[LIVE + source-audited] The two numbers count different artifacts:

| Count | What it is | Source |
|---|---|---|
| **37** | Registered MCP tools in the ignition-mcp server: gateway 6, projects 8, resources 4, designers 1, tag_providers 4, tags 9, alarms 3, historian 1, execution 1 | `mcp.tool()` registrations in `~/whiskeyhouse/ignition-mcp/src/ignition_mcp/tools/*.py` (counted per module) |
| **42** | Native `/data/api/v1/*` REST endpoints in `ignition_tools_summary.json` — a discovery artifact of the native surface; contains **zero** WebDev tools | the JSON itself (`total_tools: 42`, all paths native) |

**The parity bar for Phase 5 is the 37-tool MCP surface, of which 21 tools are tag/WebDev-domain** (browse_tags, read_tags, write_tag, get/create/edit/delete_tag_config, list_udt_types, get_udt_definition, list/get/create/delete_tag_provider, get_active_alarms, get_alarm_history, acknowledge_alarms, get_tag_history, run_gateway_script) **plus the 4 resource tools whose fate this phase decides.** PROJECT.md's "37 tools over native REST + WebDev" was accurate.

**Prior-art defects found (live-verified against 8.3.3) — parity means matching intent, not replicating bugs:**

| # | ignition-mcp behavior | Live reality on 8.3.3 |
|---|---|---|
| 1 | resource tools → `/data/api/v1/projects/{p}/resources/**` | routes do not exist (Phase 3 defect originates here) |
| 2 | `browse_tags` → `/data/api/v1/entity/browse` | entity/browse browses **config entities only** — `[default]` returns `[]`; never actually browsed tags |
| 3 | tagConfig route `system.tag.configure(provider, ...)` | first arg is a **basePath** (`[default]`), not a provider name — `Tag provider '' could not be found` |
| 4 | `system.tag.getConfiguration([path], False)` (list) | takes a **STRING**; the list's `[` poisons TagPathParser → `TagPathFormatException: Invalid source or array specification` |
| 5 | alarms as name-keyed dict in configure | silently ignored (returns Good, no alarm attached); must be a **LIST** of dicts |
| 6 | `system.alarm.acknowledge(uuid_list, note)` | gateway scope requires 3 args `(String[] eventIds, note, username)`; java.util.UUID list not coercible to String[] |
| 7 | `system.tag.importTags(payload, ...)` for import | expects a file path ("Import file not found"); payload import rides `configure` |
| 8 | history column named `Timestamp` | column is **`t_stamp`**; tag columns are provider-relative paths (`P5/T1`) |
| 9 | `request['data']` assumed string | arrives as a **parsed dict** for application/json bodies; raw string only for malformed bodies (and Jython `unicode` ≠ `str` — check `isinstance(data, (str, unicode))`) |

---

## WebDev Wire Protocol (all [LIVE] on 8.3.3, servlet cross-checked on 8.3.6)

### Servlet & URL shape
- Base path: **`/system/webdev/{project}/{folderPath}/{routeName}`** (e.g. `/system/webdev/cli-probe/cli/probe`). NOT `/data/webdev/*` — that prefix does not exist.
- WebDev module ships **in the official Docker image** (`Web Developer Module.modl`); on an expired-trial gateway the servlet answers **402** with an HTML page (module installed, unlicensed) — cross-verified on ign-research 8.3.6.

### Status-code semantics (critical for `webdev status` and doctor)
| Situation | HTTP answer |
|---|---|
| Nonexistent project OR nonexistent route (POST) | **405** (not 404!) |
| GET against a doPost-only route | **501** |
| Route exception (uncaught Java error) | **500 with Jetty HTML body** (not JSON) |
| require-auth=true, no/failed credentials | **401 Jetty HTML** |
| require-auth=true, `user-source: ""` | **500 "No user source for project"** |
| require-auth=true + `user-source: "default"` + Basic | **200**, session carries `user` |
| **X-Ignition-API-Token on require-auth route** | **401 — API tokens do NOT authenticate WebDev require-auth** |

⚠ **Doctor impact:** Phase 2's `webdev_route_status` precedent documents "404 = absent". On 8.3 the absent-marker is **405**. The presence probe must be re-pinned (suggest: POST the route's `version` action; 405 → absent, 200 → present + version, 402 → module unlicensed, 401 → present-but-auth-gated). This is an in-scope fix (WEB-01/02 depend on it).

### Request dict (from an echo route dumping `request`)
Keys: `context, data, headers, params, postData, remainingPath, remoteAddr, remoteHost, scheme, servletRequest, servletResponse`.
- `request['data']`: **parsed dict** when Content-Type is application/json and body is valid; **raw string** when malformed; Jython-2.7 `unicode` vs `str` distinction applies — routes must `isinstance(data, (str, unicode))`.
- `request['headers']`: dict, **case preserved as sent** — lower-case before lookup (WHK-Global's `extract_token` precedent).
- `request['params']`: query params.

### Response & error conventions
- Return `{'json': <payload>}` — payload may be dict or list.
- **`'status'` in the return dict is IGNORED by WebDev** — a denial returns HTTP 200 with the error in the body. Clients must parse the body, never branch on status (WHK-Global documented this hard-won lesson). Design consequence: our route contract should carry an explicit `ok`/`error` field in every response body; the CLI's classify() treats 200 as success and the action layer inspects the body envelope.
- Route-side Java exceptions must be caught (`except:` bare in Jython — `except Exception` misses some Throwables) and converted to `{'json': {'error': ...}}`; otherwise the caller gets an HTML 500.

### Session (auth spike evidence)
- Unauthenticated session: a dict without `user`.
- Basic-authenticated session (require-auth + user-source): carries `user`; `getattr(user, 'roles')` → `['Administrator']`. **In-route role probing is available** when require-auth is on.

---

## WebDev Deploy Mechanism — SPIKE (a) RESOLVED: project-zip import

[LIVE] Routes are project resources: `com.inductiveautomation.webdev/resources/{folders}/{route}/` inside a project zip. Deploying = importing that zip via the **existing Phase 3 import machinery** (`POST /data/api/v1/projects/import/{name}`, application/zip body, `overwrite` QUERY param). Per-resource import does not exist for WebDev routes (no native per-resource endpoints at all).

### Route folder format (extracted from WHK-Global source, import-verified live)
```
project.json                          # {title, description, enabled, inheritable, parent}
com.inductiveautomation.webdev/
  resources/
    cli/                              # folder path (arbitrary depth)
      probe/                          # route name → URL segment
        resource.json                 # {scope:"G", version:1, restricted, overridable, files:[...], attributes}
        config.json                   # per-method gates (below)
        doPost.py                     # handler def doPost(request, session):
```
`config.json` per-method gates (doGet/doPost/doPut/doDelete/doHead/doOptions/doTrace/doPatch — each `{enabled, max-retry-attempts, require-auth, require-https, required-roles, user-source}`):
```json
{
  "resource-type": "python-resource",
  "doPost": {"enabled": true, "max-retry-attempts": 3, "require-auth": false, "require-https": false, "required-roles": "", "user-source": ""}
}
```

### Import semantics [LIVE]
| Scenario | Result |
|---|---|
| Fresh project name | `{"success":true,"changes":[{"name":...}]}` — clean |
| Existing project, no `overwrite` | refusal: `NameInUseException "Name 'X' already in use"` (`success:false`) — maps to the locked abort collision convention |
| Existing project, `overwrite=true` | clean success; route code **replaced and live immediately** (verified: version string changed after reimport) |
| First import into a project **created via REST POST** | one-shot quirk: reports `"resource already exists: ResourceId{resourcePath=com.inductiveautomation.webdev, collectionName=X}"` **but resources still applied and route worked**; subsequent imports clean. Guidance: `ign webdev deploy` should treat its dedicated project as import-created (deploy the project.json in the same zip — no pre-create via REST) so the quirk never triggers |

Deploy guidance: the dedicated project (recommend **`ign-cli`**) is born from the first deploy zip; every later deploy is `overwrite=true` (the CLI owns the project wholesale — replace-not-merge is correct here). No pre-flight project create.

---

## Tag Operations Semantics (all [LIVE] via the deployed probe route)

### Browse — `system.tag.browse(path, {})`
- Root `''` → provider entries: `{'fullPath': '[default]', 'name': 'default', 'tagType': 'Provider', 'hasChildren': true}` (results are **dicts**, not attribute objects; discriminator key is **`tagType`**: Provider/Folder/AtomicTag/UdtType/UdtInstance/**Property**).
- Atomic tags expose their **properties as children** (`[default]P5.valueSource`, `.engHigh`, `.documentation`...) with `tagType: 'Property'` — the browse route should filter Property children by default (display decision at planner discretion; they're present in the payload).
- `provider + '_types_'` browses UDT definitions.

### Configure — `system.tag.configure(basePath, tags, collisionPolicy)`
- **basePath** form: `'[default]'`, `'[default]Folder'`, `'[provider]_types_'` — NOT a provider name.
- **`tagType`** discriminator: `Folder` / `AtomicTag` / `UdtType` / `UdtInstance` (a `type` key is silently ignored — an entire folder of "folders" became atomic tags before this was pinned).
- Children **nest** under Folder/UdtType entries (`tags: [...]`); slash-names in `name` are rejected (`Error_Configuration: name is not a valid tag name`).
- AtomicTag `value`/`defaultValue` sub-dict: `{sourceType: 'memory', dataType: 'Int4'|'Float8'|'String'|'Boolean'}`.
- **Alarms are a LIST**: `"alarms": [{"name":"HighLimit","enabled":true,"mode":"AboveValue","setpointA":100,"priority":"High"}]` (name-keyed dict is silently ignored). Source: IA 8.1 official docs (Context7) + live confirm (alarm activated after list-form configure).
- History binding: top-level `historyEnabled: true` + `historicalProvider: "p5hist"` (both store; see History section).
- UdtType: `parameters` dict + nested `tags`; UdtInstance: `{"name":"Motor1","tagType":"UdtInstance","typeId":"Motor"}`.
- Returns per-entry quality strings: `'Good'` / `'Bad_NotFound(...)'` / `'Error_Configuration(...)'` / `'Bad_Unsupported(...)'` — surface verbatim.
- collisionPolicy: `'a'` abort / `'m'` merge / `'o'` overwrite — **this is TAGS-09's collision plumbing** (maps directly onto the locked abort/overwrite CLI conventions).

### Read / Write — `system.tag.readBlocking(paths)` / `writeBlocking([path],[value])`
- Paths bracket-qualified: `'[default]P5/T1'`. Read → QualifiedValue with `.value` (native JSON types), `.quality` (`'Good'`, `'Bad_NotFound("Path ... not found.")'` — stringified with embedded detail), `.timestamp` (Java Date → str).
- Write returns a list of Quality objects — `str(qv[0])` (Quality has no `.quality` attr — prior-art bug).

### Config CRUD — `system.tag.getConfiguration(path, recursive)`
- **STRING first arg** (list form → TagPathFormatException). Bracket paths required (unbracketed parses first segment as provider → `Provider not found`).
- Non-recursive on a tag → flat dict `{path, name, tagType, value, defaultValue, ...}`; recursive on UDT type → nested definition.
- **Serialization hazard**: config dicts contain Java objects (TagPath, enums) and `system.util.jsonEncode` **stack-overflows** on them (live: `maximum recursion depth exceeded (Java StackOverflowError)`); raw `{'json': cfg}` returns an empty body. **Routes need a manual recursive walker** (str() unknown types, depth-capped — the probe's `jv()` worked; ~15 lines of Jython). Also: `value`/`defaultValue` for AtomicTags arrive as **stringified JSON** (`"{\n \"dataType\": ...}"`) — client-side re-parse needed.

### Delete — `system.tag.deleteTags(['[default]P5'])` — works, returns nothing; count echoes the request length.

### UDTs
- List: browse `[provider]_types_` → UdtType entries.
- Definition: `getConfiguration('[default]_types_/Motor', True)` → nested dict via the walker (parameters + child tags).

### Alarms — `system.alarm.*`
- `queryStatus()` → iterable; fields: `eventId` (UUID), `source` (`prov:default:/tag:AlarmTag:/alm:HighLimit`), `state` (`'Active, Unacknowledged'` / `'Active, Acknowledged'`), `priority`, `name`. Filters: source/priority/state.
- `queryJournal(...)` → **fails on default rigs**: `No alarm journal profile specified`. The journal is a config-resource chain: database-connection + `ignition/alarm-journal` profile + `general-alarm-settings` singleton. The `/data/api/v1/resources/type/ignition/alarm-journal` endpoint exposes machine-readable `defaultSettings`/`profileSchema` (settings reference a datasource of type `ignition/database-connection` — **no database connection exists on a default rig**, and the internal DB is not exposed as one). Planner decision: alarm-history on unprovisioned rigs = actionable error naming the missing journal (+ optionally a later setup verb); e2e fixture provisions a real DB connection + journal (needs a datasource — the t3code/postgres containers or a MariaDB container are candidates) OR the requirement is satisfied against a rig so provisioned.
- `acknowledge(String[] eventIds, notes, username)` — gateway-scope 3-arg form; eventIds are **strings** (UUID objects don't coerce); returns the list of UNacknowledged ids (8.3 behavior). Live: ack flipped state Active,Unack → Active,Ack.

### Tag History — SPIKE (c) ANSWER
- `system.tag.queryTagHistory(paths=..., aggregationMode='LastValue', returnSize=N, startDate/endDate=Date(long_ms))` **does not error on a default rig with zero historians** — returns a structural dataset (`t_stamp` column + one column per tag, values null). So WEB-01/TAGS-08's *query path* works everywhere; *data* needs a historian.
- **InternalHistorian is creatable via native REST with no database** [LIVE]: `POST /data/api/v1/resources/com.inductiveautomation.historian/historian-provider` with profile type `InternalHistorian` (schema from `/resources/type/...` `defaultSettings`) → provider healthy (`historian.p5hist.status: Running`). This is the rig fixture for history e2e.
- **Open execution spike (LOW confidence): tag↔historian data flow.** Tag config stores `historyEnabled: true` + `historicalProvider: 'p5hist'` (getConfiguration confirms), no container errors in logs, historian registered (`browseHistoricalTags` → `histprov:p5hist`), writes happen — but `queryTagHistory` still returned null values in the session's window. Something in the binding incantation (execution-level scan class / provider default) is still missing. **Resolution path for execution:** in the Designer on a rig, create one history-enabled tag by hand, `getConfig` it via the deployed route, diff against the script-created shape; apply the missing key in the route. Budget: one execution-time spike, ≤30min. The route and CLI structure do not change regardless.

### Bulk Export/Import — TAGS-09 SOLVED [LIVE]
- **Export**: `system.tag.exportTags(tagPaths=['[default]P5'])` (kwargs — positional form fails) → **JSON string payload** of the full subtree (names, tagTypes, values, alarms, history bindings).
- **Import**: `system.tag.configure('[target-provider]', [payloadDict], collisionPolicy)` — payload shape ≈ configure shape; round-trip proven: `[p5import]P5/T1` read back value 123/Good.
- `system.tag.importTags` is a file-path API ("Import file not found") — not the vehicle.
- json/xml/csv framing: the route exchanges the JSON payload (native interchange); csv could be a CLI-side transform of the same payload (discretion); xml is Ignition's legacy .csv-ish export — recommend JSON-primary, defer others unless trivial.
- Collision policy = configure's `a`/`m`/`o` mapping to the locked abort(6)/overwrite(--yes) CLI conventions.

---

## Tag Provider CRUD (TAGS-01) — native REST, fully [LIVE]

| Op | Endpoint | Notes |
|---|---|---|
| list | `GET /data/api/v1/resources/list/ignition/tag-provider` | full items incl. `config`, `metrics.tagCount`, `healthchecks.status` |
| names | `GET .../resources/names/ignition/tag-provider` | light `{name, enabled, modes}` |
| get | `GET .../resources/find/ignition/tag-provider/{name}` | incl. signature (needed for delete), tagCount |
| create | `POST .../resources/ignition/tag-provider` | **array body** `[{name, type:"ignition/tag-provider", collection:"core", enabled, config:{profile:{type:"STANDARD",...},settings:{}}}]` — proven |
| delete | `DELETE .../resources/ignition/tag-provider/{name}/{signature}` | signature from find — proven (also `POST .../resources/delete/ignition/tag-provider` exists; its body must be a JSON **array**) |
| rename | `POST .../resources/rename/ignition/tag-provider/{name}` | in openapi, untested live |
| type info | `GET .../resources/type/ignition/tag-provider` | schemas |

`System` provider is MANAGED-type and special; `default` is STANDARD. The two-layer naming convention applies: wire-faithful client models, unit-explicit action keys.

---

## scriptExec Security — SPIKE (b) PROPOSAL (planner locks)

**Context facts [LIVE]:** API tokens do not authenticate WebDev require-auth routes (401); require-auth requires `user-source` set (else 500) and then accepts Basic; `required-roles` is enforced natively; the WebDev session exposes `user.roles` to route code; WebDev ignores `'status'` in returns (denials ride HTTP 200).

**Prior art validated:** WHK-Global's production `core.util.webdev_auth` (`~/data/projects/WHK-Global/ignition/script-python/core/util/webdev_auth/code.py`): fail-closed shared token; accepts `X-Ignition-API-Token` or `Authorization: Bearer` (case-insensitive headers); constant-time compare via SHA-256-both-sides (Jython 2.7 lacks `hmac.compare_digest`); "no token configured" = reject. Its testing/tags route additionally layers a dev-gate (DeploymentMode/SystemName) — the "which gateway" vs "which caller" split.

**Recommended mechanism (defense in depth, all layers CLI-controllable):**
1. **Primary — deploy-time shared secret**: `ign webdev deploy` generates a high-entropy secret, bakes it into the scriptExec route folder (a `secret.json` data file in the zip — not code), stores it in the CLI profile config (0600, same store as gateway secrets), and every scriptExec call sends it as `X-Ignition-CLI-Secret`. The route fail-closed compares (sha256-both-sides constant-time; missing/unconfigured → explicit denial in the 200 body). Rotation = redeploy (`--rotate-secret`). Rationale: works with ANY caller auth (including the CLI's API token which WebDev ignores); survives `require-auth=false` misconfig; no gateway-side seeding dependency (system.secrets needs scripting to seed — chicken-and-egg; credentials.json needs filesystem access).
2. **Secondary — config.json gates**: `require-auth: true, required-roles: "Administrator", user-source: "default"` deployed as the defaults for the scriptExec route only. Works where Basic is available; harmless where not (the secret gate still holds). NOTE: setting user-source hardcodes an assumption about the gateway's IdP name — consider `user-source: ""` + require-auth:false in the deployed config if 500-on-missing-source is worse than the Basic layer is worth (planner call).
3. **Audit**: scriptExec logs hash+elapsed per invocation (prior-art pattern).

**Deploy-default posture recommendation:** scriptExec deploys **only when explicitly requested** (`ign webdev deploy --with-script-exec` or a config gate) — matches ignition-mcp's default-off posture and the "not by mere route presence" decision; the other four routes are safe-by-comparison (tag/alarm ops carry no arbitrary-exec) and deploy always. The LOCKED auth requirement holds in every posture.

**Threat model honesty:** the baked secret is readable by gateway admins via Designer/zip export — acceptable: gateway admins already have full control. The gate protects against "anyone with HTTP reach to the gateway" invoking exec, which is exactly the locked decision's bar.

---

## Route/Version Handshake Design (WEB-02) — recommended

- Every route implements `{"action": "version"}` → `{"routeVersion": "<semver-ish>", "minCli": "<x.y>"}` (route-side constant; one shared version per deployed zip).
- CLI-side expectation table keyed by route family; `ign webdev status` probes each route's version action.
- Refusal matrix (all exit 6 with actionable errors naming `ign webdev deploy`):
  - 405 → routes absent → "not deployed"
  - 402 → module unlicensed (trial expired) → distinct error
  - version < expected → "route version mismatch: deployed X, CLI expects Y — run ign webdev deploy"
  - version > expected (future route) → "CLI older than deployed routes — update ign"
- No auto-upgrade magic (roadmap-locked).

---

## Phase 3 `resource` Family Decision — RESOLVED: export/import zip surgery

Evidence (native-first, per the steer):
1. **No generic per-resource endpoints exist**: live 575-path openapi (8.3.3, `/tmp` capture during research; consistent with the committed 8.3.6 extract) has ZERO `/projects/{name}/resources` paths; the `resources/**` family is config-resource-type-scoped (tag-provider, database-connection, alarm-journal, ...). The only project-granular endpoints: export/find/import/list/names/parents/rename/copy + PUT/DELETE project.
2. **`/data/api/v1/resources/datafile/{typeId}/{name}/{filename}`** exists but is scoped to config-module datafiles (opcua device, perspective fonts/icons/themes, database-driver, service-connector, translations) — NOT project resources.
3. **EAM `project-resources`** → 403 "only when EAM is configured as a controller" — not general.
4. **WebDev-hosting fallback is not viable**: gateway scripting has no API for reading arbitrary project resource files (nothing in `dir(system.tag)`/`system.util`/`system.project` touches project resource payloads).
5. **Export/import round-trip IS native and machinery exists** (Phase 3: streaming export, buffered import, collision conventions locked).

**Decision: re-point `ign resource` onto project-export zip surgery, keeping the CLI UX:**
- `resource list` → export project zip (streaming, existing download_to_file) → enumerate members under the resource root.
- `resource get` → export → extract member (text passthrough; binary fence stays).
- `resource put` → export → replace/inject member → import `overwrite=true` (the CLI owns the semantics; concurrent-edit race documented — acceptable for the CLI's dev-tool scope).
- e2e witnesses re-point to export-and-inspect (no live route needed — witnesses ride the same verbs the CLI ships).
- Perf honesty: whole-project zip per resource op is heavier than a hypothetical per-resource route; rigs and dev projects are small; the alternative was dropping the family.

---

## Standard Stack

No new runtime dependencies except one:

| Crate | Version | Purpose | Why |
|---|---|---|---|
| `zip` | 6.x (verify exact at planning) | **read** export zips (resource family) + **write** deploy zips in-process | Phase 3 streamed zips without parsing; Phase 5 must both build deploy zips and perform zip-member surgery. Shell-out to zip/unzip is a platform dependency — rejected. Only planned new dep; `deflate` feature |

Everything else is already locked: reqwest (json/query/stream), tokio, serde/serde_json, snapbox/wiremock for tests. Route sources are **Jython 2.7 `.py` files** in `webdev/` (repo scaffold exists) — no Rust-side changes for route logic itself; keep them versionable and tested via the e2e rig.

**Route source layout (recommended, mirrors WHK-Global/Designer format so a Designer-opened project looks native):**
```
webdev/
  README.md
  routes/                                # source of truth, zipped at deploy (build step) or shipped pre-zipped
    project.json
    com.inductiveautomation.webdev/
      resources/
        cli/
          tags/          (resource.json, config.json, doPost.py)
          tagConfig/
          alarms/
          tagHistory/
          scriptExec/    (secret.json EXCLUDED from repo; generated at deploy)
  VERSION                                 # route bundle version (handshake)
```

## Architecture Patterns

### Route design (recommended): five action-dispatch routes
One folder per family (`tags`, `tagConfig`, `alarms`, `tagHistory`, `scriptExec`), each a doPost action-dispatcher (`{"action": ...}`) mirroring the corrected probe route. Rationale: matches the ignition-mcp endpoint split (parity), keeps config.json gates per-family (scriptExec locked, others open), avoids a single mega-route blast radius. The probe route source from this research is a working skeleton for `tags`+`tagConfig`.

**Action inventory per route (parity-mapped):**

| Route | Actions | Backing calls |
|---|---|---|
| tags | version, browse, read, write | system.tag.browse / readBlocking / writeBlocking |
| tagConfig | version, getConfig, configure, deleteTags, listUDTTypes, getUDTDefinition, exportTags | getConfiguration(STRING) / configure(basePath,...) / exportTags(kwargs) |
| alarms | version, active, history, acknowledge | queryStatus / queryJournal / acknowledge(String[],note,user) |
| tagHistory | version, query | queryTagHistory(Date(long)) |
| scriptExec | version, exec (secret-gated) | exec with OutputCapture + timeout (prior-art pattern) |

### CLI family shape (inherited conventions)
- `ign webdev deploy|status` (+ `--project` override, default `ign-cli`); deploy arms the version table.
- `ign tags` subfamily: `providers list/create/delete`, `browse`, `read`, `write`, `config get/create/edit/delete`, `udt types/def`, `alarms active/history/ack`, `history query`, `export/import` (exact verb naming planner's discretion; grouped-subfamily precedent).
- Every WebDev-dependent command runs the cheap precondition: route probe (405→) or cached handshake → refusal error (exit 6) naming the fix. Precedents: doctor's webdev_route_status (bypasses classify), destructive-delete double-guard.
- JSON shapes: frozen envelope, all-family-keys-always, two-layer naming. WebDev route bodies add a body-level envelope `{ok, data|error}` since HTTP-200-with-error is the route norm.

### Anti-patterns to avoid
- **Never branch on HTTP status for WebDev route bodies** (denials ride 200) — parse the body envelope.
- **Never pass lists to getConfiguration**; never pass provider names to configure; never dict-form alarms; never 2-arg acknowledge; never UUID objects into acknowledge.
- **Never trust `system.util.jsonEncode` on config payloads** (StackOverflow) — manual walker in every config-returning route.
- **Never pre-create the deploy project via REST** before first import (the one-shot "resource already exists" quirk).
- **Never rely on require-auth alone** for scriptExec (API tokens 401 there — the CLI would lock itself out).

## Don't Hand-Roll

| Problem | Don't build | Use instead | Why |
|---|---|---|---|
| Deploy packaging | custom archive writer | `zip` crate | zip64/deflate edge cases |
| Route auth | ad-hoc token compares | WHK-Global webdev_auth pattern (port the ~40 lines: dual-header extraction, sha256 constant-time, fail-closed) | production-proven on a public-internet gateway |
| Config serialization | jsonEncode / raw passthrough | manual jv() walker (probe-verified) | Java objects stack-overflow jsonEncode; raw returns empty bodies |
| Provider CRUD | WebDev routes | native config-resource REST | exists, proven, healthier data (tagCount metrics) |
| Historian provisioning | DB-backed SQL historian on rigs | InternalHistorian via native REST | no datasource needed, creatable headlessly |

## Common Pitfalls

1. **405-not-404**: missing WebDev routes answer 405; doctor's documented assumption (404) is wrong on 8.3 — re-pin the presence probe and its tests.
2. **HTTP-200 denials**: WebDev ignores `status` — every refusal must be detectable from the body; classify() alone misleads.
3. **Jython 2.7 unicode/str**: `isinstance(data, (str, unicode))` or malformed-JSON bodies crash routes.
4. **Stringified JSON inside configs** (`value`/`defaultValue`) — client must re-parse nested JSON strings.
5. **tagType vs type**; **nested children vs slash names**; **alarms-as-list**; **basePath-not-provider** — the four configure-shape traps, each live-reproduced.
6. **getConfiguration wants a STRING** — the list form's error (`Invalid source or array specification`) looks like a path problem and misleads.
7. **API token ≠ WebDev auth** — 401 on require-auth routes; user-source must be set for Basic (else 500).
8. **Empty-body returns** when a route returns raw Java-object graphs — always walk/str() before `{'json': ...}`.
9. **Trial-expired rigs answer 402** on /system/webdev — distinct from absent; status must not conflate.
10. **First-import-into-REST-created-project quirk** — deploy must not pre-create the project.
11. **Alarm history needs journal provisioning** (DB connection chain) — default rigs 500/`No alarm journal profile specified`; plan the error message and the e2e fixture.
12. **Date(long) in Jython** — `Date(float)` fails coercion; wrap `long()`.
13. **Export payload round-trip**: exportTags kwargs-only; import via configure, not importTags.

## Code Examples

### Deploy zip member (route folder) — exact verified format
```json
// com.inductiveautomation.webdev/resources/cli/tags/resource.json
{"scope": "G", "version": 1, "restricted": false, "overridable": true,
 "files": ["config.json", "doPost.py"],
 "attributes": {"lastModification": {"actor": "external", "timestamp": "2026-08-24T00:00:00Z"}}}

// config.json (per-method gates; scriptExec variant: require-auth true + required-roles)
{"resource-type": "python-resource",
 "doPost": {"enabled": true, "max-retry-attempts": 3, "require-auth": false,
            "require-https": false, "required-roles": "", "user-source": ""}}
```

### Corrected route handler core (probe-verified fragments)
```python
def doPost(request, session):
    import json, traceback, java.lang.Throwable
    data = request['data']
    if isinstance(data, (str, unicode)):       # Pitfall 3
        data = json.loads(data)
    try:
        if data.get('action') == 'configure':
            # basePath NOT provider; nested children; alarms a LIST
            result = system.tag.configure('[default]', data['tags'], data.get('collisionPolicy', 'm'))
            return {'json': {'results': [str(x) for x in result]}}
        if data.get('action') == 'getConfig':
            cfg = system.tag.getConfiguration(data['tagPath'], False)   # STRING arg
            return {'json': {'config': jv(cfg[0]) if cfg and cfg[0] else None}}
        if data.get('action') == 'exportTags':
            payload = system.tag.exportTags(tagPaths=data['paths'])     # kwargs
            return {'json': {'payload': payload}}
        ...
    except:                                     # bare — catches Java Throwables
        return {'json': {'error': traceback.format_exc()}}
```
(The full working probe — including the `jv()` walker, alarm ack 3-arg form, Date(long) history windowing — is preserved in this research session; resurrect from this doc's findings when writing `webdev/routes/`.)

### Deploy + collision (Phase 3 machinery, unchanged)
```text
POST /data/api/v1/projects/import/ign-cli?overwrite=true   Content-Type: application/zip
→ fresh: {"success":true,...} | existing no-overwrite: NameInUseException | overwrite: clean replace
```

## State of the Art

| Old assumption | Current (8.3.x, live-verified) | Impact |
|---|---|---|
| WebDev missing route = 404 | **405** | doctor + status probes |
| API token authorizes WebDev | **401 — no** | scriptExec auth design |
| getConfiguration(paths:list) | **string** | route + client |
| configure(provider,...) | **configure(basePath,...)** | route |
| alarms as dict | **list** | route |
| acknowledge(ids, note) | **(String[], note, username)** | route |
| history col 'Timestamp' | **t_stamp** | client parsing |
| importTags(payload) | **file-path API; use configure** | TAGS-09 |
| request['data'] is string | **parsed dict for JSON** | routes |

## Open Questions

1. **Tag-history data flow binding** (the one unresolved execution detail)
   - What we know: query path safe on default rigs; InternalHistorian provisionable + healthy; tag config stores historyEnabled/historicalProvider; no container errors; historian browsable — yet no data returned.
   - What's unclear: the exact execution-level binding (scan class / provider default / one Designer-only key).
   - Recommendation: ≤30min execution spike — Designer-create one history tag on the rig, `getConfig` it via the deployed route, diff, port the missing key into the route. Structure-independent.

2. **Alarm-journal e2e fixture datasource** — journal needs a real database-connection resource. Candidate: a MariaDB/Postgres sidecar container on the rig compose (the t3code/postgres pattern) + `ignition/database-connection` create via REST. Planner decides fixture vs error-message-only for unprovisioned rigs.

3. **scriptExec secondary-gate posture** — deploy `require-auth:true + user-source:"default"` (breaks on renamed IdPs) vs `require-auth:false` + secret-only (simpler, relies on primary gate). Planner call; the LOCKED requirement is satisfied either way.

4. **`webdev status` against Basic-only gateways** — if scriptExec ships require-auth+user-source, status probing it with the API token 401s; status should treat 401 on scriptExec as "present (auth-gated)" — a happy coincidence that doubles as posture verification.

## Sources

### Primary (HIGH confidence — live execution)
- Fresh disposable 8.3.3 gateway (`inductiveautomation/ignition:8.3.3`, base.gwbk restore commissioning, Phase 4 token recipe) — every [LIVE] claim; probe routes deployed at `/system/webdev/cli-probe/cli/probe`, `cli-fresh/cli/echo`, `cli-secure/cli/secure`
- Live full openapi (575 paths) captured from the rig (`/openapi.json`) — resource-family + native-surface evidence
- ign-research (8.3.6, :18088) — WebDev servlet 402 behavior cross-check

### Secondary (HIGH-MEDIUM — authoritative sources)
- Ignition 8.1 official scripting docs via Context7 (`/websites/inductiveautomation`) — configure alarm-list shape, acknowledge gateway-scope 3-arg form, JSON export format
- WHK-Global source: `webdev_auth` module, `testing/tags` route, WebDev resource.json/config.json format (the deployed-project ground truth)
- ignition-mcp source (`~/whiskeyhouse/ignition-mcp`) — tool inventory (37), client endpoint map, webdev-setup.md route prior art; `ignition_tools_summary.json` (42 native endpoints)
- git-module docker machinery (base.gwbk, entrypoint restore) — rig commissioning method

### Tertiary (LOW confidence — flagged)
- Tag-history data-flow binding (Open Question 1) — all else about history is HIGH

## Metadata

**Confidence breakdown:**
- WebDev wire protocol/deploy/auth: HIGH — live-verified end-to-end, incl. 8.3.6 cross-check
- Tag ops semantics (browse/configure/read/write/config/UDT/bulk): HIGH — live-verified with exact payloads
- Alarms (status/ack): HIGH; alarm history mechanics: HIGH (journal chain identified) — fixture approach MEDIUM
- Tag history: query-path HIGH; data-flow MEDIUM-LOW (open spike with resolution path)
- Provider CRUD: HIGH (create/delete live-proven; rename openapi-only)
- Resource-family decision: HIGH (negative claims triple-verified: live openapi + committed extract + EAM probe)
- scriptExec security proposal: HIGH on constraints (live), MEDIUM on exact secondary-gate posture (planner)

**Research date:** 2026-08-24
**Valid until:** 2026-09-24 (Ignition 8.3.x is slow-moving; re-verify if the rig image bumps)
