# Phase 10 — Live Wire Captures (10-01)

**Captured:** 2026-09-09, 09:56–10:04 UTC, from two disposable rigs spun+commissioned+token-provisioned per the Phase-9 recipe, then made EAM **controllers** via the headless module-settings flip (ops log: [10-RIG-NOTES.md](./10-RIG-NOTES.md)). Auth via headless-provisioned API tokens (`X-Ignition-API-Token` header carrying the FULL `name:key`; token names `p10tok836` / `p10tok833` — keys recorded nowhere, scratch env shredded at teardown). Every observation below is **verbatim captured output**, labeled by rig and UTC timestamp. Zero shapes guessed; zero shapes sourced from web memory.

| Rig | Version | Host port | Container | EAM mode |
| --- | ------- | --------- | --------- | -------- |
| A | 8.3.6 | 18188 | `ign-p10-836` (ephemeral docker run) | Controller (headless flip) |
| B | 8.3.3 | 19188 | `ign-p10-833-ignition-1` (ephemeral compose project) | Controller (headless flip) |

Mutations performed: the controller flip on module-settings, scratch-task create/modify/delete (`ign-p10-scratch`, `ign-p10-scratch-sched`), suspend/resume/cancel/force lifecycle POSTs on the scratch tasks only. Everything else read-only or 4xx/5xx-refused. Both rigs torn down with zero residue after captures (§Teardown).

---

## 0. Controller flip (precondition captures) — `module-settings`

Stock fresh gateway singleton read: `GET /data/api/v1/resources/singleton/com.inductiveautomation.eam/module-settings?collection=core` — HTTP 200, byte-identical on **8.3.6 (09:48Z)** and **8.3.3 (09:48Z)**:

```json
{
  "type": "com.inductiveautomation.eam/module-settings",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": ["core"],
  "signature": "0d61231df5e82d98f8c29a5530855d136533543d2e9df5617063b7a43601fb22",
  "config": {
    "installMode": "NotInstalled",
    "controllerSettings": {
      "eventTableName": "agent_events",
      "archiveLocationMode": "Automatic",
      "lowDiskThresholdMB": 1024,
      "maxRetainedBackupCount": 5,
      "backupRetentionAge": 0,
      "backupRetentionTimeUnit": "Days"
    },
    "agentSettings": {
      "sendStatsInterval": 45,
      "httpConnectTimeout": 10,
      "httpReadTimeout": 60,
      "forwardLeasedLicense": false
    }
  },
  "data": ["config.json"],
  "attributes": {
    "lastModification": { "actor": "default", "timestamp": "2026-09-09T09:48:04Z" },
    "lastModificationSignature": "61edcb06b46193de08f54b7f6e30a2c34b33cd8600cbf0015c81cb877ceba5f0"
  },
  "metrics": {},
  "healthchecks": {}
}
```

Note: the plain path `GET /data/api/v1/resources/com.inductiveautomation.eam/module-settings` has **no route** — Jetty 404 HTML `No route match for path: /v1/resources/com.inductiveautomation.eam/module-settings` on both rigs (the extract declares only PUT/POST there; reads ride the `singleton/` prefix).

Flip = full-record clone, ONLY `config.installMode` → `"Controller"`, array PUT with the original signature → **200** on both rigs:

```json
{
  "success": true,
  "changes": [
    { "name": "module-settings", "type": "com.inductiveautomation.eam/module-settings", "collection": "core", "newSignature": "05a31768c1526b1e6a5fd3ebb03cb464400f251a01ad379bb48b800942d0c5ae" }
  ],
  "problem": null
}
```

**Rig-provenance quirk:** the two rigs returned DIFFERENT `newSignature` values (8.3.6 `05a31768…` shown above; 8.3.3 `214aac302561ff520aadccf6b6a9d98b8890d1688c7bbf42255877ff75ec3204`) for the byte-identical flip body — signatures are not purely content-derived. Treat as opaque server-owned strings, never cache across mutations.

Read-back after flip: `installMode = "Controller"` on both; `GET /data/eam/api/v1/eam-tasks/scheduled/false` went 403-HTML (07-RESEARCH stock shape) → **200** immediately, no gateway restart. Scratch tasks then created identically on both rigs (200 `{success, changes[{…, newSignature}], problem:null}`), find read-back 200 with `config.profile.type=="eam_backup"` (full baseline record in 10-RIG-NOTES.md; keys: `type/name/description/enabled/version/collection/collections/signature/config{profile{type,isSuspended,scheduleMode},settings{targetGateways,targetGroups,concurrentBackups,forceBackups}}/data/attributes{uuid,enabled}/metrics/healthchecks{scheduledTaskState}`).

---

## 1. suspend→isSuspended sync (POST `/data/eam/api/v1/eam-tasks/suspend/{name}` / `resume/{name}`)

### 1a. Suspend of the OnDemand task — HTTP **500**, Jetty HTML — 8.3.6 (09:56:55Z) AND 8.3.3 (09:56:55Z), byte-identical except URI echo

```html
<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 500</title>
</head>
<body><h2>HTTP ERROR 500 Task could not be suspended</h2>
<table>
<tr><th>URI:</th><td>/data/eam/api/v1/eam-tasks/suspend/ign-p10-scratch</td></tr>
<tr><th>STATUS:</th><td>500</td></tr>
<tr><th>MESSAGE:</th><td>Task could not be suspended</td></tr>
</table>

</body>
</html>
```

Find read-back after the failed suspend: `isSuspended` still `false`, `scheduledTaskState.currentState` still `"Stopped"`, signature UNCHANGED — **no partial effect**. Identical on both rigs.

### 1b. Resume of the never-suspended OnDemand task — HTTP **204**, empty body — both rigs (09:56:55Z)

### 1c. Suspend of the Scheduled task (valid Quartz cron registered) — HTTP **204** — both rigs (09:58:51Z / 09:58:56Z)

**Timing caveat captured honestly:** the FIRST suspend attempt on this same task (09:57:32Z, seconds after its cron PUT) answered the SAME 500 "Task could not be suspended" HTML — the gateway had not yet registered the trigger. Retried ~80 s later → 204. Suspend success requires an actual scheduler trigger to exist; client models must expect late-500s right after task creation.

Read-back after the 204 suspend — **`config.profile.isSuspended` SYNCS to `true`** — 8.3.6 (09:58:51Z):

```json
{
  "profile": { "type": "eam_backup", "isSuspended": true, "scheduleMode": "Scheduled", "scheduleDetails": "0/30 * * * * ?" },
  "healthy": true,
  "details": { "repeats": true, "nextScheduled": "N/A", "currentState": "Suspended", "erroredServers": "", "owner": "eam" }
}
```

8.3.3 (09:58:56Z): identical key set and values modulo signature (`isSuspended:true`, `currentState:"Suspended"`, `nextScheduled:"N/A"`, `healthy:true`). **No drift.**

### 1d. Resume of the suspended Scheduled task — HTTP **204** — both rigs (09:59:39Z)

Read-back: `isSuspended` back to `false` on both rigs; healthcheck returned to the task's errored-stats shape (below). The runtime verbs PERSIST the flag into the definition — sync is bidirectional.

**Interim healthcheck observation (both rigs):** the Scheduled+cron task's `find` healthcheck intermittently carries a gateway-side NPE (broken STATS path, not task state):

```json
"result": {
  "healthy": false,
  "time": 1788947896010,
  "message": "Cannot invoke \"java.util.Date.getTime()\" because \"te.startDate\" is null",
  "error": { "message": "Cannot invoke \"java.util.Date.getTime()\" because \"te.startDate\" is null", "stacktrace": ["java.lang.NullPointerException: …", "\tat com.inductiveautomation.eam.gateway.tasks.TaskManagerImpl.getTaskStats(TaskManagerImpl.java:1404)", "…"] }
}
```

**Non-strict JSON warning:** these `find` bodies contain RAW TAB bytes (U+0009) inside `error.stacktrace` strings — the gateway emits JSON that strict parsers (python `json.load`, `jq`) REJECT. Any client parsing `find` must use a lenient parser or tolerate control chars in string values. 8.3.3 stack frames differ from 8.3.6's (different Jetty internals) but the shape is identical.

## 2. taskState vocabulary + can* truth table (`GET /data/eam/api/v1/eam-tasks/scheduled/{running}`)

Literal path segments `true`/`false` both answered 200 on both rigs (segment takes the literal word, no encoding surprises).

### 8.3.6 (09:59:39Z) — `scheduled/false` with one Scheduled task

```json
{"items":[{"name":"ign-p10-scratch-sched","owner":"eam","type":"Collect Backup","execStart":null,"message":"","repeats":true,"canPause":true,"canResume":false,"canCancel":true,"taskState":"Scheduled","isForced":false,"isRunning":false,"progress":0.0}],"metadata":{"total":1,"matching":1,"limit":-1,"offset":0}}
```

### 8.3.3 (09:59:40Z) — `scheduled/false` — byte-identical shape, same values

```json
{"items":[{"name":"ign-p10-scratch-sched","owner":"eam","type":"Collect Backup","execStart":null,"message":"","repeats":true,"canPause":true,"canResume":false,"canCancel":true,"taskState":"Scheduled","isForced":false,"isRunning":false,"progress":0.0}],"metadata":{"total":1,"matching":1,"limit":-1,"offset":0}}
```

`scheduled/true` (running) at the same moment — both rigs:

```json
{"items":[],"metadata":{"total":0,"matching":0,"limit":-1,"offset":0}}
```

**Live keys BEYOND the extract:** `isForced` (bool), `isRunning` (bool), `progress` (JSON float `0.0`) — the extract documents only 10 of these 13 keys. Model must include all 13, leniently.

**can\* truth (captured cell):** `taskState:"Scheduled"` → `canPause:true, canResume:false, canCancel:true`. **`type:"Collect Backup"`** is a HUMAN LABEL, not the profile.type token (`eam_backup`) — the runtime seam and config seam use DIFFERENT vocabularies for "type"; passthrough both, never normalize (the history-`taskType` precedent, now confirmed on the scheduled seam too).

**Observed `taskState` set: `"Scheduled"`, `"Suspended"`** (suspended task vanished from scheduled/false after suspend — a suspended task is NOT listed as scheduled; the "Suspended" value itself is captured from `find` healthcheck details). **`currentState` set (find healthcheck): `"Stopped"` (fresh OnDemand), `"Errored"` (broken schedule), `"Suspended"` (post-suspend).** Running/Pending rows: NOT capturable — force dispatch (below) fails fast against the unconnected `"_controller"` agent, so no running row ever materialized.

**Grace-period row (10-06 vocabulary extension — provenance: UAT rig 8.3.6, 2026-09-10, 10-UAT.md test 12):** immediately after `suspend` answered 204, the scratch row STILL appeared in `scheduled/false` with `taskState:"Suspended"` — a row shape never seen in the original 12-probe captures above (those observed "Scheduled" rows and fully-vanished suspended rows, sampled minutes apart, never inside the grace window). **Consumer implication:** suspended tasks leave `scheduled/false` EVENTUALLY, not immediately — absence checks must POLL, never single-shot assert.

**Vanish-latency UPDATE (2026-09-11, 10-06 live re-runs — contradicts the sizing evidence):** the row is NOT reliably transient within the once-assumed window. On the FRESH capture rigs (2026-09-09) the suspended row vanished within the <48 s suspend→next-read window (a single UPPER-BOUND observation, never a measured latency). On the long-lived UAT rig (2026-09-11, ~22 h uptime), the grace row (`taskState="Suspended"` at every read) persisted through the gate's full ~90 s poll deadline in BOTH 10-06 runs (10-RIG-NOTES-class rigs: fresh vs long-lived may reconcile differently; mechanics unresolved). Deadlines must be sized from MEASURED distributions across rig ages — follow-up capture work named in 10-LIVE-GATE.md §4 D1 / §6.

### Force attempt for a running row — both rigs 10:00:13/10:00:19Z

`POST /data/eam/api/v1/eam-tasks/force/eam/ign-p10-scratch-sched` → **204** (both). Polls of scheduled/true + scheduled/false at 2 s ×3 (both rigs): row stayed `taskState:"Scheduled", isForced:false, isRunning:false` — the execution failed against the GNET-unconnected agent before any row appeared. (History shows the Failed outcome per 07-RESEARCH; that failure is DATA, not an error.) Running-row shapes remain **uncaptured — honest limitation**: model `Running`/`Pending` taskState values passthrough.

## 3. DELETE confirm semantics (`DELETE /data/api/v1/resources/com.inductiveautomation.eam/eam-tasks/{name}/{signature}`)

### 3a. Wrong signature (no confirm) — HTTP **500** + JSON problem body — 8.3.6 (10:03:04Z)

```json
{
  "success": false,
  "changes": [],
  "problem": {
    "message": "DELETE illegal: signature mismatch for 'ResourceId{resourcePath=com.inductiveautomation.eam/eam-tasks/ign-p10-scratch-sched, collectionName=core}'",
    "stacktrace": ["com.inductiveautomation.ignition.common.resourcecollection.PushException: DELETE illegal: signature mismatch for …", "\tat com.inductiveautomation.ignition.gateway.resourcecollection.ChangeOperationValidationHandler$AtomicPushValidationHandler.throwIfInvalid(ChangeOperationValidationHandler.java:61)", "…"]
  },
  "references": null
}
```

8.3.3 (10:03:05Z) — same 500 + `success:false`, DRIFTED message (and it LEAKS the current live signature):

```json
"problem": {
  "message": "DELETE illegal, signature mismatch ResourceSignature{resourceId=ResourceId{resourcePath=com.inductiveautomation.eam/eam-tasks/ign-p10-scratch-sched, collectionName=core}, signature=1215b6e08a3d9959d8a6f91c71bafa92aac072b167760909d9cbecaeaab5a47c} != ResourceSignature{resourceId=ResourceId{resourcePath=com.inductiveautomation.eam/eam-tasks/ign-p10-scratch-sched, collectionName=core}, signature=deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef}'",
  "stacktrace": ["com.inductiveautomation.ignition.common.resourcecollection.PushException: DELETE illegal, signature mismatch ResourceSignature{…} != ResourceSignature{…}'", "\tat com.inductiveautomation.ignition.gateway.resourcecollection.ResourceCollectionManagerImpl$AtomicPushValidationHandler.throwIfInvalid(ResourceCollectionManagerImpl.java:760)", "…"]
}
```

### 3b. Correct signature, NO `?confirm=` — HTTP **200**, deletion SUCCEEDS — both rigs (10:03:04/10:03:05Z)

```json
{
  "success": true,
  "changes": [
    { "name": "ign-p10-scratch-sched", "type": "com.inductiveautomation.eam/eam-tasks", "collection": "core", "newSignature": "ec961ee921c63b18013094870ed2664331e965c4770fdf84bfe136e0b4164244" }
  ],
  "problem": null,
  "references": []
}
```

**A single-resource delete does NOT require the gateway-side confirm** — `changes[].newSignature` echoes the DELETED resource's last signature; NEW key `references` present (`[]` on success, `null` on the failure 500).

### 3c. `?collection=` value is the COLLECTION (`core`), not the type — the `eam-tasks` guess 404s

`DELETE …/ign-p10-scratch/{sig}?confirm=true&collection=eam-tasks` → **404 empty body**, task NOT deleted (names list unchanged) — both rigs (10:03:47Z). Retry with `?confirm=true&collection=core` → **200** success body identical in shape to 3b (8.3.6: `newSignature: "e3610cfe01c7df086da6596ccfbb7735abd5b8f2ceafd5916944919975902acf"` = the resource's creation-time signature; 8.3.3: its own). Names lists end `{"items":[],"metadata":{"total":0,"matching":0,"limit":-1,"offset":0}}` on both.

### 3d. The confirm-REQUIRED branch (dependent resources affected) — NOT capturable on a quiet rig

A lone scratch task has no dependents; every valid delete succeeded without confirm. The `{success:false, changes:[affected…]}` confirm-demand shape remains **unobserved — honest limitation**; client policy must model it leniently (see Decisions §D3).

### 3e. Unknown name + wrong signature → **404, empty body** — both rigs (10:03:47Z)

`DELETE …/definitely-not-a-task/deadbeef…` → 404 empty. Also captured: `DELETE …/{name}/` (empty signature path segment) → 404 empty (both rigs, 10:02:25Z).

## 4. Signature mismatch on PUT (array body, wrong `signature` field)

### 8.3.6 (10:00:55Z) — HTTP **500** (NOT 400/409) + JSON problem body

```json
{
  "success": false,
  "changes": [],
  "problem": {
    "message": "MODIFY illegal: signature mismatch for 'ResourceId{resourcePath=com.inductiveautomation.eam/eam-tasks/ign-p10-scratch, collectionName=core}'",
    "stacktrace": ["com.inductiveautomation.ignition.common.resourcecollection.PushException: MODIFY illegal: signature mismatch for …", "\tat com.inductiveautomation.ignition.gateway.resourcecollection.ChangeOperationValidationHandler$AtomicPushValidationHandler.throwIfInvalid(ChangeOperationValidationHandler.java:61)", "…"]
  }
}
```

### 8.3.3 (10:00:55Z) — HTTP **500**, drifted message text (leaks the live signature), same shape

```json
"problem": {
  "message": "MODIFY illegal, signature mismatch ResourceSignature{resourceId=ResourceId{resourcePath=com.inductiveautomation.eam/eam-tasks/ign-p10-scratch, collectionName=core}, signature=31922241820e8a9929756c948244d0262dc964896e71a4de826f10487e89148e} != ResourceSignature{resourceId=ResourceId{resourcePath=com.inductiveautomation.eam/eam-tasks/ign-p10-scratch, collectionName=core}, signature=deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef}'",
  "stacktrace": ["…PushException: MODIFY illegal, signature mismatch …", "\tat …ResourceCollectionManagerImpl$AtomicPushValidationHandler.throwIfInvalid(ResourceCollectionManagerImpl.java:760)", "…"]
}
```

**No drift in STATUS or SHAPE; drift only in message prose + stack frames.** Read-back after both mismatch PUTs: resource UNCHANGED (signature/enabled untouched) — mismatch is a clean refusal, zero partial effect. Classification-relevant fact: this is a 4xx-SEMANTIC failure delivered as HTTP 500 with a JSON `problem` body whose message contains `signature mismatch` on both point releases.

## 5. Rename via PUT (changed `name`, ORIGINAL signature) — HTTP **404, empty body** — both rigs (10:01:30/10:01:31Z)

`PUT /data/api/v1/resources/com.inductiveautomation.eam/eam-tasks` with single-element array `{"name":"ign-p10-scratch-renamed", …,"signature":<original>}` → 404 empty. Post-check `names` list (both rigs):

```json
{"items":[{"name":"ign-p10-scratch","enabled":true,"modes":["core"]},{"name":"ign-p10-scratch-sched","enabled":true,"modes":["core"]}],"metadata":{"total":2,"matching":2,"limit":-1,"offset":0}}
```

**No rename, NO new resource, no error body** — the modify route resolves the resource BY the body's name and finds nothing. Rename via PUT is NOT supported; a CLI rename must be create-new + delete-old (composition decision for 10-03/10-04). Bonus shape: the `names` endpoint items carry `modes:["core"]` (key is `modes`, not `collections`).

## 6. PUT echo semantics (full-record modify) + partial-body refusal

### 6a. Full-record modify, ONE key mutated — HTTP 200 + read-back proves the landing — both rigs (09:58:12/09:58:16Z)

Request: find → clone FULL record → `config.profile.scheduleDetails = "0/30 * * * * ?"` → single-element array PUT carrying the ORIGINAL signature. Response (8.3.6; 8.3.3 identical shape with own sig):

```json
{
  "success": true,
  "changes": [
    { "name": "ign-p10-scratch-sched", "type": "com.inductiveautomation.eam/eam-tasks", "collection": "core", "newSignature": "0d0dfea2919abb1f02fc86baea73d99696626524169a9ac36526044f89ac16e0" }
  ],
  "problem": null
}
```

Read-back: `scheduleDetails:"0/30 * * * * ?"` present, NEW signature equals `changes[0].newSignature` exactly, `version` stayed `1`, unknown round-trip keys (`attributes.uuid`, `data`) preserved. **Echo semantics proven: full find-record + signature round-trips cleanly; `newSignature` in the response is authoritative for the NEXT mutation.**

### 6b. Partial body: `config.settings` REMOVED — HTTP **422** — both rigs (10:01:30/10:01:31Z)

```json
{
  "messages": ["Settings cannot be null"],
  "fieldMessages": []
}
```

Read-back: settings still present, enabled unchanged — the create-422 trap (eam-create-422.md) repeats VERBATIM on modify. The existing 422→`InvalidInput` classify arm covers the modify path as-is. Read-modify-write of the FULL record is mandatory, confirmed on live wire.

### 6c. Malformed JSON body (raw control chars from a lenient clone) — HTTP **400, empty body** — both rigs (09:58:51/09:58:56Z, 10:02:05Z)

An array-PUT whose body contained unescaped TAB bytes → 400 empty (gateway never parsed it). Shape note for clients: **400 = request never parsed (empty body); 422 = parsed but failed validation (JSON error body); 500+problem = semantic refusal.**

## 7. 404 shapes on runtime verbs — THERE ARE NO 404s; unknown names are 500s/204s

Both rigs (10:00:55Z), byte-identical HTML except URI:

`POST suspend/definitely-not-a-task` → **500** Jetty HTML, `MESSAGE: Task could not be suspended` — INDISTINGUISHABLE from suspend-of-OnDemand (§1a). Suspend cannot distinguish unknown-task from untriggered-task by status code.

`POST resume/definitely-not-a-task` → **500** Jetty HTML with the distinguishing message:

```html
<title>Error 500</title>
…
<tr><th>MESSAGE:</th><td>java.io.IOException: No NamedResourceHandler found for task &apos;definitely-not-a-task&apos;</td></tr>
```

`POST cancel/definitely-not-a-task` → **204 empty** — cancel of a nonexistent task is a SILENT SUCCESS. Also `POST cancel/ign-p10-scratch` (real OnDemand task, nothing pending) → **204 empty** (10:03:47Z, both rigs). Cancel is always-204 unless a pending execution exists (running-row shapes uncapturable on these rigs — limitation).

## 8. `scheduled/{running}` empty-list shape + pagination metadata

Captured repeatedly (09:48Z, 09:59Z, 10:00Z, 10:03Z) on both rigs — the canonical quiet-controller body:

```json
{"items":[],"metadata":{"total":0,"matching":0,"limit":-1,"offset":0}}
```

`limit:-1` = unbounded default; `offset:0`; `total`/`matching` int pair. Same envelope on `names` (`items:[{name,enabled,modes}]`) and `list` families. No drift between 8.3.3 and 8.3.6.

## 9. PUT success response shape

Captured on module-settings flip, task create (POST), task modify (PUT), and both deletes — invariant across all five:

```json
{ "success": true, "changes": [ { "name": "…", "type": "…", "collection": "core", "newSignature": "…" } ], "problem": null }
```

`changes[]` is per-array-item; `newSignature` is the post-write signature (== find read-back signature; for DELETE it equals the deleted resource's final signature). Delete adds the 4th key `references` (`[]` on success). `problem` is `null` on every success observed.

## 10. `problem{message, stacktrace}` appearance rules

- **Present + non-null ONLY on the semantic-refusal 500s** (signature-mismatch MODIFY/DELETE): `{"message": String, "stacktrace": [String, …]}` — stacktrace frame text DRIFTS between 8.3.3/8.3.6 (different internals), message prose drifts, the substring `signature mismatch` is stable on both.
- **`null` on every 200 success** (5/5 mutations).
- **ABSENT (key not present at all)** on 422 validation bodies (`{messages, fieldMessages}` shape) and on 404s (empty body) and on Jetty HTML 500s (lifecycle verbs).
- A `problem`-shaped body also appears in HEALTHCHECKS (`find` → `healthchecks.scheduledTaskState.result.{message,error{message,stacktrace}}`) with raw TAB bytes in strings (§1 caveat).

## 11. Rig drift summary (8.3.3 vs 8.3.6)

| Surface | Drift? | Detail |
| --- | --- | --- |
| Envelope shapes (`success/changes/problem/references`) | NONE | byte-shape identical; values (signatures) differ per rig |
| Signature values for identical input | YES | different `newSignature` for the same flip body — server-derived, opaque |
| Signature-mismatch 500 message prose | YES | 8.3.6: `MODIFY illegal: signature mismatch for 'ResourceId{…}'`; 8.3.3: `MODIFY illegal, signature mismatch ResourceSignature{…} != ResourceSignature{…}'` (leaks live signature); both contain `signature mismatch` |
| Signature-mismatch stacktrace frames | YES | `ChangeOperationValidationHandler:61` (8.3.6) vs `ResourceCollectionManagerImpl:760` (8.3.3) |
| suspend/resume/cancel 4xx-5xx shapes | NONE | same statuses, same HTML messages (`&apos;` escaping identical) |
| scheduled/{running} record keys | NONE | all 13 keys both rigs; `progress` JSON float both |
| find healthcheck NPE (Scheduled-no-startDate stats path) | SHAPE same, frames differ | raw TAB bytes in both |
| create 422 ("Settings cannot be null") | NONE | identical body on modify AND create paths |
| module-settings singleton default record | NONE | byte-identical on both fresh rigs (incl. signature) |

## 12. `scheduleDetails` wire form

`scheduleDetails` is a **plain JSON string** on the profile, whose meaning is scheduleMode-dependent:

| scheduleMode | scheduleDetails captured | Evidence |
| --- | --- | --- |
| `OnDemand` | key ABSENT | both rigs' scratch find (09:56Z) |
| `Scheduled` | Quartz cron string, e.g. `"0/30 * * * * ?"` | PUT (09:58Z) accepted + echoed on both rigs; gateway registered a repeating trigger (`repeats:true`, `nextScheduled` epoch-ms STRING e.g. `"1788947835694"` while errored) |
| `AtDelay` | seconds string, `"60"` | PUT (10:02:25Z) accepted + echoed on both rigs; trigger fired one-shot, then task left the scheduled set (scheduled/false empty at 10:03:04Z with task still existing) |
| `AtTime` / `Immediate` / `SuspendedByFailover` | NOT captured | not needed for Phase 10 scope; unobserved — passthrough |

Pre-validation shapes: `Scheduled` WITHOUT scheduleDetails is accepted on create (200) but yields a broken task (`currentState:"Errored"`, `healthy:false`, `nextScheduled` epoch-ms string, and eventually the stats NPE). `nextScheduled` in healthcheck details is a **string-wrapped epoch-ms** value when numeric, `"N/A"` when suspended, `"On Demand"` for OnDemand — polymorphic STRING, never a JSON number. `find` healthcheck `time` fields are plain epoch-ms JSON integers.

---

## Decisions locked by captures

These decisions are LOCKED — they come from the captures above only, and 10-02/10-03/10-04/10-05 must encode them as written.

1. **suspend/resume DO sync `config.profile.isSuspended`** — the runtime verbs persist the flag into the task definition (suspend → `isSuspended:true` + healthcheck `currentState:"Suspended"` + `nextScheduled:"N/A"`; resume → `false`). Lifecycle results can honestly report the definition flag from a find read-back. Suspend 204 success additionally REQUIRES a live scheduler trigger — a just-created task (and any OnDemand task) answers **500 HTML "Task could not be suspended"**; `String` consts: observed `currentState` set = `{"Stopped","Errored","Suspended"}` (rig-proven both versions); anything else passthrough.
2. **taskState/can\* vocabulary** — observed `taskState`: `"Scheduled"`, `"Suspended"` (8.3.3 + 8.3.6). `Scheduled` cell of the truth table: `canPause:true, canResume:false, canCancel:true`. ScheduledTask live record has **13 keys** — the extract's 10 PLUS `isForced:bool, isRunning:bool, progress:f64`. `type` on this seam is a human label (`"Collect Backup"`) — never enum, never conflate with `profile.type`. `metadata` envelope: `{total, matching, limit:-1, offset:0}`. Running/Pending rows + their can\* cells: UNCAPTURED (no connected-agent rig) — passthrough with explicit unobserved marking.
3. **DELETE confirm policy (RECOMMENDATION for 10-03 — 10-03 decides):** a lone-resource delete SUCCEEDS without `?confirm=` (200 `{success:true, changes:[…], problem:null, references:[]}`), and `?collection=` must carry the COLLECTION name (`core`), not the type — wrong collection value ⇒ 404 empty. Recommendation: client sends NO confirm param by default; on `success:false`+`references`/`changes` evidence (the confirm-demand shape, UNOBSERVED here) re-run once with `confirm=true` under the user's `--yes`. Never hard-code `confirm=true` (it would bypass a genuine multi-resource warning we cannot yet see).
4. **Signature mismatch = HTTP 500 + JSON `{success:false, changes:[], problem{message, stacktrace}}`** (PUT and DELETE both) — NOT 4xx. Classify by status-500 + JSON body + message substring `signature mismatch` (stable across 8.3.3/8.3.6; the surrounding prose and stack frames drift). 8.3.3's message LEAKS the current live signature — do not surface raw gateway messages where a sanitized hint will do, but the mismatch cause is client-detectable.
5. **Rename via PUT is NOT supported** — changed `name` + original signature ⇒ 404 empty (no rename, no create). A rename verb must compose create-new + delete-old. CLI must not offer a rename that silently maps to this PUT.
6. **PUT is full-record echo-modify** — find → clone → mutate keys → array PUT with the ORIGINAL signature lands exactly the sent keys and returns `{success, changes[{name,type,collection,newSignature}], problem:null}`; `newSignature` == read-back signature and is REQUIRED for the next mutation (signatures are per-write, never cacheable across writes). Omitting `config.settings` ⇒ **422 `{"messages":["Settings cannot be null"],"fieldMessages":[]}`** (create trap repeats on modify; existing InvalidInput arm correct). Malformed JSON ⇒ **400 empty body**.
7. **Runtime-verb "not found" is NOT 404** — unknown-name suspend/resume ⇒ 500 Jetty HTML (suspend's message is indistinguishable from untriggered-task; resume's says `No NamedResourceHandler found for task '<name>'` with `&apos;` escapes); unknown-name cancel ⇒ **204**. Curated-path 404-empty remains reserved for the config-resource routes (wrong name/collection on PUT/DELETE). Classification note for 10-03: lifecycle-verb failures are 500s with HTML bodies — do NOT build not_found handling for lifecycle verbs on status alone; use find-before-write for name validation (the blast-radius preview read gives this for free).
8. **PUT/POST/DELETE success shape (probe 9):** invariant `{success:true, changes:[{name,type,collection,newSignature}], problem:null}` across create/modify/delete/module-settings; delete additionally carries `references` (`[]` success, `null` on the 500 problem shape). Parse all four keys leniently.
9. **`problem{message,stacktrace}` (probe 10) appears ONLY on the semantic 500s** (signature mismatch family); `null` on successes; absent on 422/404/HTML-500 shapes. Report `problem.message` honestly in envelopes; classify ONLY on the stable substring, never on stack frames.
10. **scheduleDetails (probe 12):** plain string; `OnDemand` ⇒ absent; `Scheduled` ⇒ Quartz cron; `AtDelay` ⇒ seconds. Model as `Option<String>` passthrough; never parse cron client-side. Healthcheck `nextScheduled` is a polymorphic string (`"N/A"` / `"On Demand"` / epoch-ms-as-string); `find` bodies may contain RAW TAB bytes inside healthcheck stacktrace strings — parser must be lenient.
11. **Controller-mode provisioning is headless-capable (bonus, also in RIG-NOTES):** singleton GET `resources/singleton/com.inductiveautomation.eam/module-settings?collection=core` → full-record clone → flip `config.installMode` → `"Controller"` → array PUT with original signature → 200; runtime `/data/eam/api/v1/*` activates instantly. The CLI must NOT expose this as a verb (07-RESEARCH ladder stands: README-documented manual flip only).
12. **Per-rig drift (probe 11):** zero drift in any STATUS/envelope/shape; drift confined to (a) signature VALUES (server-derived), (b) 500-problem message prose + stack frames (`signature mismatch` substring stable), (c) healthcheck NPE stack frames. All String-vocab decisions survive both point releases as written.

## Teardown

Executed 10:05Z, recorded in [10-RIG-NOTES.md](./10-RIG-NOTES.md): `docker rm -f -v ign-p10-836` + `docker compose -p ign-p10-833 --project-directory /tmp/ign-p10-rigs/ign-p10-833 down -v --remove-orphans` (volume `ign-p10-833_ign-p10-833_gateway_data` deleted). Verified: zero `ign-p10*` containers/volumes, ports 18188/19188 freed, `tokens.env` (sole key-material holder) shredded (`shred -u`), plus the stale Phase-9 `tokens.env` shredded as hygiene. Nothing left running.
