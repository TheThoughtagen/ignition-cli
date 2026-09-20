# Phase 09 — Live Wire Captures (09-02)

**Captured:** 2026-09-07, 02:37–02:41 UTC, from two disposable rigs spun+commissioned+token-provisioned per the Phase-4 recipe (ops log: [09-RIG-NOTES.md](./09-RIG-NOTES.md)). Auth via headless-provisioned API tokens (`X-Ignition-API-Token` header; token names `p9tok836b` / `p9tok833b` — keys recorded nowhere). Every observation below is **verbatim captured output**, labeled by rig. Zero shapes guessed; zero shapes sourced from web memory.

| Rig | Version | Host port | Container |
| --- | ------- | --------- | --------- |
| A | 8.3.6 | 18188 | `ign-p9-836` (ephemeral docker run) |
| B | 8.3.3 | 19188 | `ign-p9-833` (ephemeral compose project) |

The one mutation performed: the diagnostics-bundle generate→download loop (benign, touches nothing else). Everything else read-only or 4xx-refused.

---

## 1. `GET /data/api/v1/licenses`

### 8.3.6 (Rig A, 02:37Z) — HTTP 200

```json
{
  "cloud": [],
  "certificate": [],
  "hardware": [],
  "embedded": [],
  "leased": [],
  "effective": {
    "lastUpdated": 1788748551614
  },
  "leasedUnactivateTimeoutMS": 7500,
  "busy": false
}
```

### 8.3.3 (Rig B, 02:39Z) — HTTP 200

```json
{
  "cloud": [],
  "certificate": [],
  "hardware": [],
  "embedded": [],
  "leased": [],
  "effective": {
    "lastUpdated": 1788748731641
  },
  "leasedUnactivateTimeoutMS": 7500,
  "busy": false
}
```

**Shape notes:** top level = 5 arrays (`cloud`, `certificate`, `hardware`, `embedded`, `leased`) + 1 object (`effective`) + 2 scalars (`leasedUnactivateTimeoutMS`, `busy`). All arrays empty on a fresh trial rig — the non-empty element shapes inside `leased` etc. are NOT capturable on a fresh rig (no activations exist) — record as a known limitation: model those array elements leniently/passthrough. `effective.lastUpdated` = 1788748551614 → epoch **milliseconds** (×1000 of current epoch-seconds 1788748551 ✓). No point-release drift between rigs at this level (values only).

## 2. `GET /data/api/v1/trial`

### 8.3.6 (Rig A, 02:37Z) — HTTP 200

```json
{
  "licenseMode": "Trial",
  "trialState": "AllInDemo",
  "trialSecondsLeft": 6823,
  "expired": false,
  "emergency": false,
  "emergencySecondsLeft": 0,
  "development": false,
  "developmentSecondsLeft": 0
}
```

### 8.3.3 (Rig B, 02:39Z) — HTTP 200

```json
{
  "licenseMode": "Trial",
  "trialState": "AllInDemo",
  "trialSecondsLeft": 6547,
  "expired": false,
  "emergency": false,
  "emergencySecondsLeft": 0,
  "development": false,
  "developmentSecondsLeft": 0
}
```

**`TrialWire` cross-check (crates/ignition-core/src/client/trial.rs):** all 8 documented fields present on BOTH rigs, no extra keys, no drift — `TrialWire` parses these bodies as-is (active-state shape this time; the expired shape is already fixture-backed from Phase 4). Endpoint answers identically with or without the token header (both rigs).

## 3. `GET /data/api/v1/redundancy`

### 8.3.6 (Rig A, ~02:37:11Z) — HTTP 200

```json
{"role":"Independent","projectState":"Unknown","activityLevel":"Active","localId":"192.168.215.2","peerConnected":false,"hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":460959,"lastSyncTimestamp":-1}
```

### 8.3.3 (Rig B, ~02:39:47Z) — HTTP 200

```json
{"role":"Independent","projectState":"Unknown","activityLevel":"Active","localId":"192.168.171.2","peerConnected":false,"hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":617422,"lastSyncTimestamp":-1}
```

**Units determination (magnitude cross-check, both rigs agree):**
- `uptime` = **milliseconds since gateway (JVM) start**. Rig A: gateway RUNNING since 02:29:30Z (ops log); 460959 ms = 460.96 s → 02:29:30 + 461 s = **02:37:11Z**, exactly the capture moment. Rig B: 617422 ms = 617.4 s → 02:29:30 + 617 s = **02:39:47Z**, exactly the capture moment. A minutes-old-restarted rig rules out ms-since-epoch (would be ~1.789e12) AND seconds-since-epoch (~1.789e9); it also rules out seconds-since-start (would read ~461/617, not ~461k/617k).
- `lastSyncTimestamp` = **-1 sentinel** ("never synced") on a fresh Independent rig on BOTH versions. The unit is therefore **not determinable from fresh-rig captures alone** — no sync ever happened. Nearest live evidence: the sibling timestamp `licenses.effective.lastUpdated` is epoch-ms. Model implication for 09-04: treat `lastSyncTimestamp` as nullable — `-1` (or absent) → `None`; non-negative values interpreted as epoch-ms per the sibling pattern, flagged as inference (not capture-proven) in the model docs.
- `role` vocabulary captured: `Independent` (fresh rig). `projectState`: `Unknown`. `activityLevel`: `Active`. Peer-connected shapes (Backup/Primary roles, `peerConnected:true`) not capturable on a single fresh rig — passthrough for the unobserved fields.

## 4. `GET /data/api/v1/overview/gan`

### 8.3.6 (Rig A) — HTTP 200

```json
{
  "totalConnections": 0,
  "runningConnections": 0,
  "outgoingByteRate": 0.0,
  "incomingByteRate": 0.0,
  "remoteGateways": 0
}
```

### 8.3.3 (Rig B) — HTTP 200

```json
{
  "totalConnections": 0,
  "runningConnections": 0,
  "outgoingByteRate": 0.0,
  "incomingByteRate": 0.0,
  "remoteGateways": 0
}
```

**Zero-connection shape (fresh rig IS the capture):** 5 flat scalars, no arrays, no nesting. Number shapes: connection counts and `remoteGateways` serialize as JSON **ints** (`0`); the byte rates serialize as JSON **floats** (`0.0`) — model byte rates as f64; ints as integer types. No drift between versions.

## 5. Diagnostics bundle state machine (Pitfall-2 capture — observed, never guessed)

### 8.3.6 (Rig A)

`POST /data/api/v1/diagnostics/bundle/generate` (no body) — HTTP 200, immediate:

```json
{"state":"Generating"}
```

`GET /data/api/v1/diagnostics/bundle/status` — every distinct observation, 2 s poll (started 02:38:04Z):

```text
2026-09-07T02:38:04Z  {"state":"Generating"}            <- no fileSize while generating
2026-09-07T02:38:07Z  {"state":"Valid","fileSize":61053} <- terminal/ready, fileSize appears
```

`GET /data/api/v1/diagnostics/bundle/download` — response headers (verbatim):

```text
HTTP/1.1 200 OK
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename=Ignition-b63402cfb9d7_diagnostics_20260907-0238.zip
Content-Length: 61053
```

Downloaded body: **61053 bytes** (= `fileSize` exactly); first 4 bytes `50 4b 03 04` = `PK\x03\x04` (ZIP magic ✓). Status after download still `{"state":"Valid","fileSize":61053}` — the ready state persists; download is repeatable.

### 8.3.3 (Rig B)

`POST /data/api/v1/diagnostics/bundle/generate` (no body) — HTTP 200, immediate:

```json
{"state":"Generating"}
```

`GET /data/api/v1/diagnostics/bundle/status` — every distinct observation, 2 s poll (started 02:40:28Z):

```text
2026-09-07T02:40:28Z  {"state":"Generating"}            <- no fileSize while generating
2026-09-07T02:40:30Z  {"state":"Valid","fileSize":55281} <- terminal/ready, fileSize appears
```

`GET /data/api/v1/diagnostics/bundle/download` — response headers (verbatim):

```text
HTTP/1.1 200 OK
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename=Ignition-b099918f68ab_diagnostics_20260907-0240.zip
Content-Length: 55281
```

Downloaded body: **55281 bytes** (= `fileSize` exactly); first 4 bytes `50 4b 03 04` = `PK\x03\x04` ✓. Status after download still `{"state":"Valid","fileSize":55281}`.

**Observations:** state strings are **PascalCase** (`Generating`, `Valid`) — lowercase guesses would have been wrong (Pitfall 2 dodged). Generation is fast on a fresh rig (~2–6 s). While generating, `fileSize` is ABSENT from the body (not null, not 0 — key missing). On completion it is present and equals the eventual Content-Length. Filename pattern: `Ignition-<containerId>_diagnostics_<yyyyMMdd>-<HHmm>.zip`. Full state vocabulary beyond these two observed states (e.g. failure/cancel states) is **not observable on a healthy fresh rig** and remains unenumerated — anything 09-05 does not see here must degrade/passthrough, never assume.

## 6. Read-only 4xx probes

### 8.3.6 (Rig A)

`GET /data/api/v1/definitely-not-a-real-endpoint` — HTTP **404** (hypothesis 404 ✓), body verbatim:

```html
<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 No route match for path: /v1/definitely-not-a-real-endpoint</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/definitely-not-a-real-endpoint</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>No route match for path: /v1/definitely-not-a-real-endpoint</td></tr>
</table>

</body>
</html>
```

`DELETE /data/api/v1/gateway-info` — HTTP **404**, NOT the hypothesized 405. Headers verbatim:

```text
HTTP/1.1 404 Not Found
Content-Length: 0
```

### 8.3.3 (Rig B)

`GET /data/api/v1/definitely-not-a-real-endpoint` — HTTP **404**, body byte-identical to Rig A's above (same Jetty page, same `MESSAGE: No route match for path: /v1/definitely-not-a-real-endpoint`).

`DELETE /data/api/v1/gateway-info` — HTTP **404**, `Content-Length: 0` (identical to Rig A).

**The live 4xx partition (09-06 gate evidence):** the gateway does NOT answer 405 for a wrong-method request on a real path — method mismatch collapses into **404 with an empty body**, while path mismatch is **404 with a Jetty HTML body** (`text/html`, message line `No route match for path: <path-minus-/data/api>`). Classification rule for 09-06: 404+HTML-body ⇒ unknown path; 404+empty-body ⇒ known path, unsupported method; neither indicates the resource was touched (both are refusals — read-only posture holds). Auth failure on these routes is a third shape: 401 with a Jetty HTML page (`HTTP ERROR 401 Unauthorized` — observed during this run when the token header was accidentally absent, indistinguishable text pattern from the 404 page except status/title).

---

## Decisions locked by captures

These decisions are LOCKED — they come from the captures above only, and 09-04/09-05/09-06 must encode them as written.

1. **Bundle state vocabulary (Pitfall 2):** observed states are exactly `Generating` and `Valid` (PascalCase, both rigs). **`BUNDLE_GENERATING_STATES = ["Generating"]`** — the generating set has exactly one member on the captured evidence. Terminal/ready set = `["Valid"]`. No other state was observed; any other string must round-trip passthrough with the model's doc-comment marking it unobserved.
2. **Bundle status body:** `fileSize` key is ABSENT while generating and present (integer, **bytes**) when `Valid`; `fileSize` == download `Content-Length` exactly. Model as `Option<u64>`.
3. **Bundle download:** `Content-Type: application/zip;charset=utf-8`; `Content-Disposition` IS present on both versions (`attachment; filename=Ignition-<id>_diagnostics_<stamp>.zip`) — but parse it optionally anyway (Phase-4-style version tolerance); body begins `PK\x03\x04`; ready state persists after download (repeatable).
4. **Redundancy units:** `uptime` = **ms since gateway start** (wall-clock-proven twice); `lastSyncTimestamp` = `-1` sentinel when never synced — unit NOT capture-proven (model `Option`, ms per sibling `effective.lastUpdated` evidence, inference flagged); `fileSize` = bytes.
5. **Licenses top-level shape:** 5 arrays (`cloud`/`certificate`/`hardware`/`embedded`/`leased`) + `effective{lastUpdated: epoch-ms}` + `leasedUnactivateTimeoutMS` (int, ms, name-says-so) + `busy` (bool). Empty on fresh rigs → type the morning-check fields (the arrays' presence/count + effective freshness + busy), and ride **flatten/passthrough** for array ELEMENT shapes (uncapturable on a fresh rig — no leases exist). Partial-curated split confirmed: no deep nesting exists at top level to flatten past.
6. **Trial:** zero drift — `TrialWire` parses both live bodies unchanged (all 8 fields, no extras).
7. **GAN zero-connection shape:** 5 flat scalars; counts ints, byte rates JSON floats (`0.0`) → f64. A fresh rig's zero-connection body is the canonical shape.
8. **4xx partition (09-06 gates):** unknown path ⇒ **404 + HTML body** (`No route match for path: …`); unsupported method on real path ⇒ **404 + empty body** (NOT 405 — the plan's 405 hypothesis is falsified by capture); missing/bad auth ⇒ **401 + HTML body**. Read-only posture: all three are refusals; no partial mutation observed.
9. **Commissioning (bonus, recorded in 09-RIG-NOTES.md):** 8.3 image env vars `ACCEPT_EULA`/`GATEWAY_ADMIN_PASSWORD` are NOT consumed by the docker entrypoint; full headless commissioning rides the commissioner wire API (`/bootstrap` → `/get-step` → `/post-step` eula-accept → `/post-step` start-gateway). Relevant to any future rig-automation phase, not to 09-04/09-05 models.

## Teardown

Executed 02:43Z, recorded in [09-RIG-NOTES.md](./09-RIG-NOTES.md): `docker rm -f -v ign-p9-836` + `docker compose -p ign-p9-833 down -v --remove-orphans` (volume `ign-p9-833_gateway_data` deleted). Verified: zero `ign-p9*` containers/volumes, ports 18188/19188 freed, token scratch file shredded. Nothing left running.
