# Phase 20 — Live Captures (8.3.6)

**Captured:** from one disposable gateway, compose project `ign-deploy-rig`, bound to 127.0.0.1:18290 only. Auth via two config-defined API tokens (`cap-a`, `cap-b`, custom `Authenticated > Deploy` level) sent in `X-Ignition-API-Token` as the full `name:key`; keys are recorded nowhere in this repository. Every block below is verbatim captured output (`curl -i`, so status line, response headers and body are all shown), stamped with UTC time. Driver: `crates/ignition-core/tests/fixtures/live-rig-deploy/capture.sh`.

| Item | Value |
| --- | --- |
| Image | `inductiveautomation/ignition:8.3.6` (linux/amd64, run under emulation on arm64) |
| `ignitionVersion` (gateway-info) | `8.3.6 (b2026042713)` |
| Compose variant | `compose.yml` + `compose.bind.yml` (bind-mounted `resources/`), see D11 |
| Host arch | arm64 (Darwin 25.6.0) |
| Rig start | 2026-10-12T14:29:20Z |
| Gateway name | `ign-deploy-rig` |

## C-S3 — carried S3 redundancy pair (8.3.6; pre- and post-failover)

Source: `/Users/pmannion/data/worktrees/whk-environment-orchestration/gateway-config-spec/spikes/gateway-modes/results/S3.md` (spike date 2026-10-09). Carried from the gateway-modes spike S3 (clean 8.3.6 pair), not captured on this phase's single-gateway rig.

# S3.md:6-7 state: paired, no mode (pre-failover)

~~~
a={"role":"Master","projectState":"Good","activityLevel":"Active","localId":"192.168.97.3","peerId":"spike-a-Backup","peerConnected":true,"hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":29279,"lastSyncTimestamp":1791586091827}  mode_a=''
b={"role":"Backup","projectState":"Good","activityLevel":"Cold","localId":"192.168.97.2","peerId":"spike-a-Master","peerConnected":true,"hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":-1,"lastSyncTimestamp":1791586091875}  mode_b=''
~~~

# S3.md:40-41 state: after failover (post-failover)

~~~
a={"role":"Master","projectState":"Good","activityLevel":"Cold","localId":"192.168.97.3","peerId":"spike-a-Backup","peerConnected":true,"hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":-1,"lastSyncTimestamp":1791586358959}  mode_a='production'
b={"role":"Backup","projectState":"Good","activityLevel":"Active","localId":"192.168.97.2","peerId":"spike-a-Master","peerConnected":true,"hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":30539,"lastSyncTimestamp":1791586359045}  mode_b='production'
~~~

Role stays fixed across the failover (a is always Master, b is always Backup) while `activityLevel` moves (a Active to Cold, b Cold to Active). `lastSyncTimestamp` (1791586091827 and similar) is epoch-ms.

## C1a — idle GET scan-lock/config (no lock held)

### GET idle

~~~
GET /data/api/v1/scan-lock/config @ 2026-10-12T14:32:44Z
HTTP/1.1 204 No Content
Date: Mon, 12 Oct 2026 14:32:44 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block


[curl http=204 total=0.062841s]
~~~

## C1b — POST scan-lock/config with no body, then with {"acquireTimeout":10,"holdTimeout":60}

### POST no body

~~~
POST /data/api/v1/scan-lock/config @ 2026-10-12T14:32:44Z
HTTP/1.1 500 Server Error
Date: Mon, 12 Oct 2026 14:32:44 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 552

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 500</title>
</head>
<body><h2>HTTP ERROR 500 java.lang.NullPointerException: Cannot read field &quot;acquireTimeout&quot; because &quot;lockOptions&quot; is null</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/scan-lock/config</td></tr>
<tr><th>STATUS:</th><td>500</td></tr>
<tr><th>MESSAGE:</th><td>java.lang.NullPointerException: Cannot read field &quot;acquireTimeout&quot; because &quot;lockOptions&quot; is null</td></tr>
</table>

</body>
</html>

[curl http=500 total=0.120414s]
~~~

### POST acquire 10/60

~~~
POST /data/api/v1/scan-lock/config @ 2026-10-12T14:32:44Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:32:44 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "timestamp": 380308558,
  "remaining": 59,
  "actor": "api-token:cap-a:192.168.97.1"
}
[curl http=200 total=0.035169s]
~~~

## C1c — held GET twice, 2 s apart (cap-a holds)

### GET held #1

~~~
GET /data/api/v1/scan-lock/config @ 2026-10-12T14:32:44Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:32:44 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "timestamp": 380308558,
  "remaining": 59,
  "actor": "api-token:cap-a:192.168.97.1"
}
[curl http=200 total=0.009516s]
~~~

### GET held #2 (2 s later)

~~~
GET /data/api/v1/scan-lock/config @ 2026-10-12T14:32:46Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:32:46 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "timestamp": 380308558,
  "remaining": 57,
  "actor": "api-token:cap-a:192.168.97.1"
}
[curl http=200 total=0.013429s]
~~~

## C1d — cap-a acquires twice (same-token re-acquire while held)

### POST re-acquire by the holder 3/20

~~~
POST /data/api/v1/scan-lock/config @ 2026-10-12T14:32:46Z
HTTP/1.1 409 Conflict
Date: Mon, 12 Oct 2026 14:32:46 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 418

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 409</title>
</head>
<body><h2>HTTP ERROR 409 Lock already held by api-token:cap-a:192.168.97.1</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/scan-lock/config</td></tr>
<tr><th>STATUS:</th><td>409</td></tr>
<tr><th>MESSAGE:</th><td>Lock already held by api-token:cap-a:192.168.97.1</td></tr>
</table>

</body>
</html>

[curl http=409 total=0.007785s]
~~~

### GET after re-acquire

~~~
GET /data/api/v1/scan-lock/config @ 2026-10-12T14:32:47Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:32:47 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "timestamp": 380308558,
  "remaining": 57,
  "actor": "api-token:cap-a:192.168.97.1"
}
[curl http=200 total=0.005840s]
~~~

## C1e — cap-a holds (holdTimeout 20) while cap-b POSTs with acquireTimeout 2

### cap-a acquires 10/20

~~~
POST /data/api/v1/scan-lock/config @ 2026-10-12T14:33:45Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:33:45 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "timestamp": 380369593,
  "remaining": 19,
  "actor": "api-token:cap-a:192.168.97.1"
}
[curl http=200 total=0.016170s]
~~~

### cap-b competing acquire 2/5

~~~
POST /data/api/v1/scan-lock/config @ 2026-10-12T14:33:45Z
HTTP/1.1 409 Conflict
Date: Mon, 12 Oct 2026 14:33:45 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 418

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 409</title>
</head>
<body><h2>HTTP ERROR 409 Lock already held by api-token:cap-a:192.168.97.1</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/scan-lock/config</td></tr>
<tr><th>STATUS:</th><td>409</td></tr>
<tr><th>MESSAGE:</th><td>Lock already held by api-token:cap-a:192.168.97.1</td></tr>
</table>

</body>
</html>

[curl http=409 total=0.015868s]
~~~

### GET as cap-b (holder visible?)

~~~
GET /data/api/v1/scan-lock/config @ 2026-10-12T14:33:45Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:33:45 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "timestamp": 380369593,
  "remaining": 19,
  "actor": "api-token:cap-a:192.168.97.1"
}
[curl http=200 total=0.006804s]
~~~

## C1f — holdTimeout 3, sleep 5, GET (expect 204), then scan POST and GET

### acquire 10/3

~~~
POST /data/api/v1/scan-lock/config @ 2026-10-12T14:34:06Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:34:06 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "timestamp": 380390544,
  "remaining": 2,
  "actor": "api-token:cap-a:192.168.97.1"
}
[curl http=200 total=0.011124s]
~~~

### GET after expiry

~~~
GET /data/api/v1/scan-lock/config @ 2026-10-12T14:34:11Z
HTTP/1.1 204 No Content
Date: Mon, 12 Oct 2026 14:34:11 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block


[curl http=204 total=0.044194s]
~~~

### POST scan/config

~~~
POST /data/api/v1/scan/config @ 2026-10-12T14:34:12Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:34:11 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": true,
  "lastScanTimestamp": 1791815507409,
  "lastScanDuration": 1009
}
[curl http=200 total=0.072251s]
~~~

### GET scan/config

~~~
GET /data/api/v1/scan/config @ 2026-10-12T14:34:12Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:34:12 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": true,
  "lastScanTimestamp": 1791815507409,
  "lastScanDuration": 1009
}
[curl http=200 total=0.059439s]
~~~

## C1g — release probes: DELETE scan-lock/config and POST scan-lock/config/release (lock held by cap-a)

### acquire 10/20

~~~
POST /data/api/v1/scan-lock/config @ 2026-10-12T14:34:14Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:34:12 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "timestamp": 380395931,
  "remaining": 18,
  "actor": "api-token:cap-a:192.168.97.1"
}
[curl http=200 total=1.918185s]
~~~

### DELETE scan-lock/config

~~~
DELETE /data/api/v1/scan-lock/config @ 2026-10-12T14:34:14Z
HTTP/1.1 404 Not Found
Date: Mon, 12 Oct 2026 14:34:14 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Length: 0


[curl http=404 total=0.040361s]
~~~

### POST scan-lock/config/release

~~~
POST /data/api/v1/scan-lock/config/release @ 2026-10-12T14:34:14Z
HTTP/1.1 404 Not Found
Date: Mon, 12 Oct 2026 14:34:14 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 434

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 No route match for path: /v1/scan-lock/config/release</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/scan-lock/config/release</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>No route match for path: /v1/scan-lock/config/release</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.019273s]
~~~

### GET after the probes (still held?)

~~~
GET /data/api/v1/scan-lock/config @ 2026-10-12T14:34:14Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:34:14 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "timestamp": 380395931,
  "remaining": 17,
  "actor": "api-token:cap-a:192.168.97.1"
}
[curl http=200 total=0.016988s]
~~~

## C1h — GET /openapi.json (live 8.3.6): path count and scan/lock routes

~~~
GET /openapi.json @ 2026-10-12T14:34:35Z
HTTP/1.1 200 OK
saved to $IGN_DEPLOY_RIG_STATE/openapi-8.3.6.json (12637301 bytes; not in the repo)
path count: 584
openapi: 3.1.0   info: {"title":"Ignition HTTP API","description":"### About this page\nThis specification covers all public HTTP API endpoints for the Ignition platform and currently\ninstalled modules. It is intended to be used by developers to understand the capabilities and\ndata formats used by the API. This documentation is generated from the\n[OpenAPI](https://www.openapis.org/) specification document which is available in JSON format\n[here](/openapi.json).\n","contact":{"url":"https://support.inductiveautomation.com/","email":"support@inductiveautomation.com"},"license":{"name":"Inductive Automation EULA","url":"https://inductiveautomation.com/ignition/license"},"version":"1.0.0","x-logo":{"url":"/res/sys/img/homepage/gw_hp_ignition_logo.png","altText":"Ignition by Inductive Automation"}}
paths matching scan or lock, with methods:
[{"p":"/data/api/v1/diagnostics/threads/deadlocks","m":["get"]},{"p":"/data/api/v1/scan-lock/config","m":["get","post"]},{"p":"/data/api/v1/scan-lock/projects","m":["get","post"]},{"p":"/data/api/v1/scan/config","m":["get","post"]},{"p":"/data/api/v1/scan/projects","m":["get","post"]}]
paths matching release|unlock:
[]
~~~

## C1i — scan/config: idle GET, POST, poll until scanActive is false and lastScanTimestamp advanced; second POST; cap-b scan while cap-a holds

### GET idle

~~~
GET /data/api/v1/scan/config @ 2026-10-12T14:34:36Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:34:36 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 91

{
  "scanActive": false,
  "lastScanTimestamp": 1791815654089,
  "lastScanDuration": 2081
}
[curl http=200 total=0.017761s]
~~~

### POST scan #1

~~~
POST /data/api/v1/scan/config @ 2026-10-12T14:34:36Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:34:36 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": true,
  "lastScanTimestamp": 1791815654089,
  "lastScanDuration": 2081
}
[curl http=200 total=0.018954s]
~~~

### POST scan #2 immediately after #1

~~~
POST /data/api/v1/scan/config @ 2026-10-12T14:34:36Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:34:36 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": true,
  "lastScanTimestamp": 1791815654089,
  "lastScanDuration": 2081
}
[curl http=200 total=0.006795s]
~~~

### poll GET scan/config every 500 ms (before lastScanTimestamp=1791815654089) @ 2026-10-12T14:34:36Z

~~~
t+0s {
  "scanActive": true,
  "lastScanTimestamp": 1791815654089,
  "lastScanDuration": 2081
}
t+1s {
  "scanActive": false,
  "lastScanTimestamp": 1791815676996,
  "lastScanDuration": 582
}
[poll done: 2 polls, completion predicate met]
~~~

### acquire 10/20 (cap-a)

~~~
POST /data/api/v1/scan-lock/config @ 2026-10-12T14:34:37Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:34:37 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "timestamp": 380420858,
  "remaining": 19,
  "actor": "api-token:cap-a:192.168.97.1"
}
[curl http=200 total=0.019091s]
~~~

### cap-b POST scan while cap-a holds the lock

~~~
POST /data/api/v1/scan/config @ 2026-10-12T14:34:37Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:34:37 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1791815676996,
  "lastScanDuration": 582
}
[curl http=200 total=0.020236s]
~~~

### GET scan after the cap-b scan

~~~
GET /data/api/v1/scan/config @ 2026-10-12T14:34:37Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:34:37 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1791815676996,
  "lastScanDuration": 582
}
[curl http=200 total=0.010051s]
~~~

### GET lock after the cap-b scan

~~~
GET /data/api/v1/scan-lock/config @ 2026-10-12T14:34:37Z
HTTP/1.1 204 No Content
Date: Mon, 12 Oct 2026 14:34:37 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block


[curl http=204 total=0.007983s]
~~~

## C1j — does a holder's scan release the lock? (cap-a acquires 20, cap-a POSTs scan/config, GET lock)

### acquire 10/20

~~~
POST /data/api/v1/scan-lock/config @ 2026-10-12T14:34:37Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:34:37 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "timestamp": 380421039,
  "remaining": 19,
  "actor": "api-token:cap-a:192.168.97.1"
}
[curl http=200 total=0.147316s]
~~~

### holder POST scan/config

~~~
POST /data/api/v1/scan/config @ 2026-10-12T14:34:37Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:34:37 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1791815677461,
  "lastScanDuration": 260
}
[curl http=200 total=0.007956s]
~~~

### GET lock after the holder scan

~~~
GET /data/api/v1/scan-lock/config @ 2026-10-12T14:34:37Z
HTTP/1.1 204 No Content
Date: Mon, 12 Oct 2026 14:34:37 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block


[curl http=204 total=0.010513s]
~~~

### poll GET scan/config every 500 ms (before lastScanTimestamp=0) @ 2026-10-12T14:34:37Z

~~~
t+0s {
  "scanActive": true,
  "lastScanTimestamp": 1791815677461,
  "lastScanDuration": 260
}
t+1s {
  "scanActive": false,
  "lastScanTimestamp": 1791815677748,
  "lastScanDuration": 246
}
[poll done: 2 polls, completion predicate met]
~~~

## C-null (no mode)

### GET gateway-info (raw)

~~~
GET /data/api/v1/gateway-info @ 2026-10-12T14:35:00Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:35:00 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 406

{"name":"ign-deploy-rig","redundancyRole":"Independent","edition":"standard","hostname":"localhost","port":"18290","ignitionVersion":"8.3.6 (b2026042713)","deploymentMode":"","timeZone":"Coordinated Universal Time","timeZoneId":"Etc/UTC","jvmVersion":"17.0.18","allowUnsignedModules":false,"license":{"mode":"Trial","validForVersion":8,"expirationDate":"Oct 12, 2026, 4:31:03 PM","licenseRestrictions":[]}}
[curl http=200 total=0.035954s]
~~~

~~~
{"has":true,"v":"","t":"string"}
~~~

## C-mode

### GET /mode (before)

~~~
GET /data/api/v1/mode @ 2026-10-12T14:35:00Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:35:00 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 70

{"items":[],"metadata":{"total":0,"matching":0,"limit":-1,"offset":0}}
[curl http=200 total=0.157460s]
~~~

### POST /mode production

~~~
POST /data/api/v1/mode @ 2026-10-12T14:35:01Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:35:01 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 108

{
  "name": "production",
  "title": "Production",
  "description": "ign-deploy-rig",
  "resourceCount": 0
}
[curl http=200 total=0.163653s]
~~~

### POST /mode production again (duplicate)

~~~
POST /data/api/v1/mode @ 2026-10-12T14:35:01Z
HTTP/1.1 409 Conflict
Date: Mon, 12 Oct 2026 14:35:01 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 346

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 409</title>
</head>
<body><h2>HTTP ERROR 409 Mode already exists</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/mode</td></tr>
<tr><th>STATUS:</th><td>409</td></tr>
<tr><th>MESSAGE:</th><td>Mode already exists</td></tr>
</table>

</body>
</html>

[curl http=409 total=0.017139s]
~~~

### POST /mode with the illegal name "a b/"

~~~
POST /data/api/v1/mode @ 2026-10-12T14:35:01Z
HTTP/1.1 400 Bad Request
Date: Mon, 12 Oct 2026 14:35:01 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 342

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 400</title>
</head>
<body><h2>HTTP ERROR 400 Invalid mode name</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/mode</td></tr>
<tr><th>STATUS:</th><td>400</td></tr>
<tr><th>MESSAGE:</th><td>Invalid mode name</td></tr>
</table>

</body>
</html>

[curl http=400 total=0.022097s]
~~~

### GET /mode (after)

~~~
GET /data/api/v1/mode @ 2026-10-12T14:35:01Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:35:01 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 161

{"items":[{"name":"production","title":"Production","description":"ign-deploy-rig","resourceCount":0}],"metadata":{"total":1,"matching":1,"limit":-1,"offset":0}}
[curl http=200 total=0.040926s]
~~~

## C3a

### POST create (one-element array, name + config, no collection, no attributes)

~~~
POST /data/api/v1/resources/ignition/database-connection @ 2026-10-12T14:35:01Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:35:01 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 281

{
  "success": true,
  "changes": [
    {
      "name": "IGN_DEPLOY_RIG_DB_1791815701",
      "type": "ignition/database-connection",
      "collection": "core",
      "newSignature": "5de3e2f09a55346efa902ca0369b55829507e18e8d64d10c82d8a5e1eb9ea57c"
    }
  ],
  "problem": null
}
[curl http=200 total=0.980144s]
~~~

### POST the same name again (duplicate)

~~~
POST /data/api/v1/resources/ignition/database-connection @ 2026-10-12T14:35:02Z
HTTP/1.1 409 Conflict
Date: Mon, 12 Oct 2026 14:35:02 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 6161

{
  "success": false,
  "changes": [],
  "problem": {
    "message": "CREATE conflict: \u0027ResourceId{resourcePath\u003dignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701, collectionName\u003dcore}\u0027 already exists",
    "stacktrace": [
      "com.inductiveautomation.ignition.common.resourcecollection.PushConflictException: CREATE conflict: \u0027ResourceId{resourcePath\u003dignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701, collectionName\u003dcore}\u0027 already exists",
      "\tat com.inductiveautomation.ignition.gateway.resourcecollection.ChangeOperationValidationHandler$AtomicPushValidationHandler.throwIfInvalid(ChangeOperationValidationHandler.java:58)",
      "\tat com.inductiveautomation.ignition.gateway.resourcecollection.ResourceCollectionManagerImpl.push(ResourceCollectionManagerImpl.java:1129)",
      "\tat com.inductiveautomation.ignition.gateway.config.ConfigurationManagerImpl.push(ConfigurationManagerImpl.java:409)",
      "\tat com.inductiveautomation.ignition.gateway.config.actions.DefaultResourceActions$CreateImpl.create(DefaultResourceActions.java:100)",
      "\tat com.inductiveautomation.ignition.gateway.config.actions.DefaultResourceActions$Create.create(DefaultResourceActions.java:59)",
      "\tat com.inductiveautomation.ignition.gateway.config.routes.NamedTypeRoutes.handleCreateNamed(NamedTypeRoutes.java:270)",
      "\tat com.inductiveautomation.ignition.gateway.config.routes.NamedTypeRoutes.lambda$mountCreateNamedRoute$8(NamedTypeRoutes.java:167)",
      "\tat com.inductiveautomation.ignition.gateway.dataroutes.Route.service(Route.java:355)",
      "\tat com.inductiveautomation.ignition.gateway.dataroutes.RouteGroupImpl.service(RouteGroupImpl.java:117)",
      "\tat com.inductiveautomation.ignition.gateway.dataroutes.RouteGroupCollectionServlet.serviceInternal(RouteGroupCollectionServlet.java:151)",
      "\tat com.inductiveautomation.ignition.gateway.dataroutes.AbstractRouteGroupServlet.service(AbstractRouteGroupServlet.java:40)",
      "\tat jakarta.servlet.http.HttpServlet.service(HttpServlet.java:614)",
      "\tat org.eclipse.jetty.ee10.servlet.ServletHolder$NotAsync.service(ServletHolder.java:1385)",
      "\tat org.eclipse.jetty.ee10.servlet.ServletHolder.handle(ServletHolder.java:751)",
      "\tat org.eclipse.jetty.ee10.servlet.ServletHandler$ChainEnd.doFilter(ServletHandler.java:1622)",
      "\tat com.inductiveautomation.catapult.filters.GatewayFilter.doFilter(GatewayFilter.java:119)",
      "\tat jakarta.servlet.http.HttpFilter.doFilter(HttpFilter.java:97)",
      "\tat org.eclipse.jetty.ee10.servlet.FilterHolder.doFilter(FilterHolder.java:205)",
      "\tat org.eclipse.jetty.ee10.servlet.ServletHandler$Chain.doFilter(ServletHandler.java:1594)",
      "\tat org.eclipse.jetty.ee10.servlet.ServletHandler$MappedServlet.handle(ServletHandler.java:1555)",
      "\tat org.eclipse.jetty.ee10.servlet.ServletChannel.dispatch(ServletChannel.java:823)",
      "\tat org.eclipse.jetty.ee10.servlet.ServletChannel.handle(ServletChannel.java:440)",
      "\tat org.eclipse.jetty.ee10.servlet.ServletHandler.handle(ServletHandler.java:470)",
      "\tat org.eclipse.jetty.security.SecurityHandler.handle(SecurityHandler.java:575)",
      "\tat org.eclipse.jetty.ee10.servlet.SessionHandler.handle(SessionHandler.java:717)",
      "\tat org.eclipse.jetty.server.handler.ContextHandler.handle(ContextHandler.java:1071)",
      "\tat org.eclipse.jetty.rewrite.handler.RewriteHandler$LastRuleHandler.handle(RewriteHandler.java:159)",
      "\tat org.eclipse.jetty.rewrite.handler.Rule$Handler.handle(Rule.java:108)",
      "\tat org.eclipse.jetty.rewrite.handler.HeaderPatternRule$1.handle(HeaderPatternRule.java:89)",
      "\tat org.eclipse.jetty.rewrite.handler.Rule$Handler.handle(Rule.java:108)",
      "\tat org.eclipse.jetty.rewrite.handler.HeaderPatternRule$1.handle(HeaderPatternRule.java:89)",
      "\tat org.eclipse.jetty.rewrite.handler.Rule$Handler.handle(Rule.java:108)",
      "\tat org.eclipse.jetty.rewrite.handler.HeaderPatternRule$1.handle(HeaderPatternRule.java:89)",
      "\tat org.eclipse.jetty.rewrite.handler.Rule$Handler.handle(Rule.java:108)",
      "\tat org.eclipse.jetty.rewrite.handler.HeaderPatternRule$1.handle(HeaderPatternRule.java:89)",
      "\tat org.eclipse.jetty.rewrite.handler.Rule$Handler.handle(Rule.java:108)",
      "\tat org.eclipse.jetty.rewrite.handler.RewriteHandler.handle(RewriteHandler.java:143)",
      "\tat org.eclipse.jetty.server.Handler$Sequence.handle(Handler.java:805)",
      "\tat org.eclipse.jetty.server.Handler$Sequence.handle(Handler.java:805)",
      "\tat org.eclipse.jetty.server.Server.handle(Server.java:182)",
      "\tat org.eclipse.jetty.server.internal.HttpChannelState$HandlerInvoker.run(HttpChannelState.java:677)",
      "\tat org.eclipse.jetty.server.internal.HttpConnection.onFillable(HttpConnection.java:416)",
      "\tat org.eclipse.jetty.io.AbstractConnection$ReadCallback.succeeded(AbstractConnection.java:322)",
      "\tat org.eclipse.jetty.io.FillInterest.fillable(FillInterest.java:105)",
      "\tat org.eclipse.jetty.io.SelectableChannelEndPoint$1.run(SelectableChannelEndPoint.java:53)",
      "\tat org.eclipse.jetty.util.thread.strategy.AdaptiveExecutionStrategy.runTask(AdaptiveExecutionStrategy.java:480)",
      "\tat org.eclipse.jetty.util.thread.strategy.AdaptiveExecutionStrategy.consumeTask(AdaptiveExecutionStrategy.java:443)",
      "\tat org.eclipse.jetty.util.thread.strategy.AdaptiveExecutionStrategy.tryProduce(AdaptiveExecutionStrategy.java:293)",
      "\tat org.eclipse.jetty.util.thread.strategy.AdaptiveExecutionStrategy.run(AdaptiveExecutionStrategy.java:201)",
      "\tat org.eclipse.jetty.util.thread.ReservedThreadExecutor$ReservedThread.run(ReservedThreadExecutor.java:311)",
      "\tat org.eclipse.jetty.util.thread.QueuedThreadPool.runJob(QueuedThreadPool.java:981)",
      "\tat org.eclipse.jetty.util.thread.QueuedThreadPool$Runner.doRunJob(QueuedThreadPool.java:1211)",
      "\tat org.eclipse.jetty.util.thread.QueuedThreadPool$Runner.run(QueuedThreadPool.java:1166)",
      "\tat java.base/java.lang.Thread.run(Unknown Source)"
    ]
  }
}
[curl http=409 total=0.313926s]
~~~

### GET find (read the signature)

~~~
GET /data/api/v1/resources/find/ignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701 @ 2026-10-12T14:35:02Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:35:02 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 3657

{
  "type": "ignition/database-connection",
  "name": "IGN_DEPLOY_RIG_DB_1791815701",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "5de3e2f09a55346efa902ca0369b55829507e18e8d64d10c82d8a5e1eb9ea57c",
  "config": {
    "driver": "PostgreSQL",
    "translator": "POSTGRES",
    "includeSchemaInTableName": false,
    "connectURL": "jdbc:postgresql://mode-host:5432/spike",
    "username": "spike",
    "connectionProps": "",
    "connectionResetParams": "",
    "defaultTransactionLevel": "DEFAULT",
    "poolInitSize": 0,
    "poolMaxActive": 8,
    "poolMaxIdle": 8,
    "poolMinIdle": 0,
    "poolMaxWait": 5000,
    "validationQuery": "SELECT 1",
    "testOnBorrow": true,
    "testOnReturn": false,
    "testWhileIdle": false,
    "evictionRate": -1,
    "evictionTests": 3,
    "evictionTime": 1800000,
    "failoverMode": "STANDARD",
    "slowQueryLogThreshold": 60000,
    "validationSleepTime": 10000
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "uuid": "0939907f-c2a5-44d8-ac13-2a8d252f312f",
    "enabled": true
  },
  "metrics": {
    "active-connections": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.active-connections",
      "metric": {
        "value": 0.0,
        "numerator": 0.0,
        "denominator": 8.0,
        "type": "ratio-gauge",
        "description": "Active Connections",
        "units": ""
      }
    },
    "active-queries": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.active-queries",
      "metric": {
        "value": [],
        "type": "gauge",
        "description": "Active Queries",
        "units": "queries"
      }
    },
    "expensive-queries": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.expensive-queries",
      "metric": {
        "value": [],
        "type": "gauge",
        "description": "Expensive Queries",
        "units": "queries"
      }
    },
    "queries": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.queries",
      "metric": {
        "oneMinuteRate": 0.0,
        "fiveMinuteRate": 0.0,
        "fifteenMinuteRate": 0.0,
        "count": 0,
        "meanRate": 0.0,
        "histogram": {
          "max": 0.0,
          "min": 0.0,
          "p50": 0.0,
          "p75": 0.0,
          "p95": 0.0,
          "p98": 0.0,
          "p99": 0.0,
          "mean": 0.0,
          "bucketSize": 0.0,
          "buckets": [
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0
          ],
          "type": "histogram",
          "stdev": 0.0
        },
        "type": "timer"
      }
    },
    "rows": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.rows",
      "metric": {
        "oneMinuteRate": 0.0,
        "fiveMinuteRate": 0.0,
        "fifteenMinuteRate": 0.0,
        "count": 0,
        "meanRate": 0.0,
        "type": "meter"
      }
    },
    "throughput": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.throughput",
      "metric": {
        "value": -1.0,
        "type": "gauge",
        "description": "",
        "units": "queries/sec"
      }
    }
  },
  "healthchecks": {
    "status": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.status",
      "result": {
        "healthy": true,
        "time": 1791815702942,
        "message": "Reconnecting"
      }
    }
  }
}
[curl http=200 total=0.081191s]
~~~

### PUT with the right signature

~~~
PUT /data/api/v1/resources/ignition/database-connection @ 2026-10-12T14:35:03Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:35:03 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 281

{
  "success": true,
  "changes": [
    {
      "name": "IGN_DEPLOY_RIG_DB_1791815701",
      "type": "ignition/database-connection",
      "collection": "core",
      "newSignature": "8e01629a0ccc4115d874626df494edf5cdb1cd7510789a4ce9cc39cc85063f14"
    }
  ],
  "problem": null
}
[curl http=200 total=0.151400s]
~~~

### GET find after the good PUT

~~~
GET /data/api/v1/resources/find/ignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701 @ 2026-10-12T14:35:03Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:35:03 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 10817

{
  "type": "ignition/database-connection",
  "name": "IGN_DEPLOY_RIG_DB_1791815701",
  "description": "c3a update",
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "8e01629a0ccc4115d874626df494edf5cdb1cd7510789a4ce9cc39cc85063f14",
  "config": {
    "driver": "PostgreSQL",
    "translator": "POSTGRES",
    "includeSchemaInTableName": false,
    "connectURL": "jdbc:postgresql://mode-host:5432/spike",
    "username": "spike",
    "connectionProps": "",
    "connectionResetParams": "",
    "defaultTransactionLevel": "DEFAULT",
    "poolInitSize": 0,
    "poolMaxActive": 9,
    "poolMaxIdle": 8,
    "poolMinIdle": 0,
    "poolMaxWait": 5000,
    "validationQuery": "SELECT 1",
    "testOnBorrow": true,
    "testOnReturn": false,
    "testWhileIdle": false,
    "evictionRate": -1,
    "evictionTests": 3,
    "evictionTime": 1800000,
    "failoverMode": "STANDARD",
    "slowQueryLogThreshold": 60000,
    "validationSleepTime": 10000
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "lastModification": {
      "actor": "api-token:cap-a:192.168.97.1",
      "timestamp": "2026-10-12T14:35:03Z"
    },
    "uuid": "0939907f-c2a5-44d8-ac13-2a8d252f312f",
    "lastModificationSignature": "ffc636164e849e022d4eee0e76ab409a129a36e526d354c2609eab2615128172",
    "enabled": true
  },
  "metrics": {
    "active-connections": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.active-connections",
      "metric": {
        "value": 0.0,
        "numerator": 0.0,
        "denominator": 9.0,
        "type": "ratio-gauge",
        "description": "Active Connections",
        "units": ""
      }
    },
    "active-queries": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.active-queries",
      "metric": {
        "value": [],
        "type": "gauge",
        "description": "Active Queries",
        "units": "queries"
      }
    },
    "expensive-queries": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.expensive-queries",
      "metric": {
        "value": [],
        "type": "gauge",
        "description": "Expensive Queries",
        "units": "queries"
      }
    },
    "queries": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.queries",
      "metric": {
        "oneMinuteRate": 0.0,
        "fiveMinuteRate": 0.0,
        "fifteenMinuteRate": 0.0,
        "count": 0,
        "meanRate": 0.0,
        "histogram": {
          "max": 0.0,
          "min": 0.0,
          "p50": 0.0,
          "p75": 0.0,
          "p95": 0.0,
          "p98": 0.0,
          "p99": 0.0,
          "mean": 0.0,
          "bucketSize": 0.0,
          "buckets": [
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0
          ],
          "type": "histogram",
          "stdev": 0.0
        },
        "type": "timer"
      }
    },
    "rows": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.rows",
      "metric": {
        "oneMinuteRate": 0.0,
        "fiveMinuteRate": 0.0,
        "fifteenMinuteRate": 0.0,
        "count": 0,
        "meanRate": 0.0,
        "type": "meter"
      }
    },
    "throughput": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.throughput",
      "metric": {
        "value": -1.0,
        "type": "gauge",
        "description": "",
        "units": "queries/sec"
      }
    }
  },
  "healthchecks": {
    "status": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.status",
      "result": {
        "healthy": false,
        "time": 1791815703249,
        "message": "Faulted",
        "error": {
          "message": "Cannot create PoolableConnectionFactory (The connection attempt failed.)",
          "stacktrace": [
            "java.sql.SQLException: Cannot create PoolableConnectionFactory (The connection attempt failed.)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:645)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)",
            "\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)",
            "\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)",
            "\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)",
            "\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)",
            "\tat java.base/java.lang.Thread.run(Unknown Source)",
            "Caused by: org.postgresql.util.PSQLException: The connection attempt failed.",
            "\tat org.postgresql.core.v3.ConnectionFactoryImpl.openConnectionImpl(ConnectionFactoryImpl.java:358)",
            "\tat org.postgresql.core.ConnectionFactory.openConnection(ConnectionFactory.java:54)",
            "\tat org.postgresql.jdbc.PgConnection.\u003cinit\u003e(PgConnection.java:273)",
            "\tat org.postgresql.Driver.makeConnection(Driver.java:446)",
            "\tat org.postgresql.Driver.connect(Driver.java:298)",
            "\tat org.apache.commons.dbcp2.DriverConnectionFactory.createConnection(DriverConnectionFactory.java:52)",
            "\tat org.apache.commons.dbcp2.PoolableConnectionFactory.makeObject(PoolableConnectionFactory.java:481)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.validateConnectionFactory(BasicDataSource.java:114)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:641)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)",
            "\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)",
            "\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)",
            "\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)",
            "\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)",
            "\tat java.base/java.lang.Thread.run(Unknown Source)",
            "Caused by: java.net.UnknownHostException: mode-host",
            "\tat java.base/sun.nio.ch.NioSocketImpl.connect(Unknown Source)",
            "\tat java.base/java.net.SocksSocketImpl.connect(Unknown Source)",
            "\tat java.base/java.net.Socket.connect(Unknown Source)",
            "\tat org.postgresql.core.PGStream.createSocket(PGStream.java:243)",
            "\tat org.postgresql.core.PGStream.\u003cinit\u003e(PGStream.java:98)",
            "\tat org.postgresql.core.v3.ConnectionFactoryImpl.tryConnect(ConnectionFactoryImpl.java:136)",
            "\tat org.postgresql.core.v3.ConnectionFactoryImpl.openConnectionImpl(ConnectionFactoryImpl.java:262)",
            "\tat org.postgresql.core.ConnectionFactory.openConnection(ConnectionFactory.java:54)",
            "\tat org.postgresql.jdbc.PgConnection.\u003cinit\u003e(PgConnection.java:273)",
            "\tat org.postgresql.Driver.makeConnection(Driver.java:446)",
            "\tat org.postgresql.Driver.connect(Driver.java:298)",
            "\tat org.apache.commons.dbcp2.DriverConnectionFactory.createConnection(DriverConnectionFactory.java:52)",
            "\tat org.apache.commons.dbcp2.PoolableConnectionFactory.makeObject(PoolableConnectionFactory.java:481)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.validateConnectionFactory(BasicDataSource.java:114)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:641)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)",
            "\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)",
            "\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)",
            "\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)",
            "\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)",
            "\tat java.base/java.lang.Thread.run(Unknown Source)"
          ]
        }
      }
    }
  }
}
[curl http=200 total=0.040112s]
~~~

### PUT with a stale (64-hex wrong) signature

~~~
PUT /data/api/v1/resources/ignition/database-connection @ 2026-10-12T14:35:03Z
HTTP/1.1 500 Server Error
Date: Mon, 12 Oct 2026 14:35:03 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 6173

{
  "success": false,
  "changes": [],
  "problem": {
    "message": "MODIFY illegal: signature mismatch for \u0027ResourceId{resourcePath\u003dignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701, collectionName\u003dcore}\u0027",
    "stacktrace": [
      "com.inductiveautomation.ignition.common.resourcecollection.PushException: MODIFY illegal: signature mismatch for \u0027ResourceId{resourcePath\u003dignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701, collectionName\u003dcore}\u0027",
      "\tat com.inductiveautomation.ignition.gateway.resourcecollection.ChangeOperationValidationHandler$AtomicPushValidationHandler.throwIfInvalid(ChangeOperationValidationHandler.java:61)",
      "\tat com.inductiveautomation.ignition.gateway.resourcecollection.ResourceCollectionManagerImpl.push(ResourceCollectionManagerImpl.java:1129)",
      "\tat com.inductiveautomation.ignition.gateway.config.ConfigurationManagerImpl.push(ConfigurationManagerImpl.java:409)",
      "\tat com.inductiveautomation.ignition.gateway.config.actions.DefaultResourceActions$ModifyImpl.modify(DefaultResourceActions.java:185)",
      "\tat com.inductiveautomation.ignition.gateway.config.actions.DefaultResourceActions$Modify.modify(DefaultResourceActions.java:140)",
      "\tat com.inductiveautomation.ignition.gateway.config.routes.ResourceTypeRoutes.modifyResources(ResourceTypeRoutes.java:350)",
      "\tat com.inductiveautomation.ignition.gateway.config.routes.ResourceTypeRoutes.lambda$mountModifyRoute$6(ResourceTypeRoutes.java:211)",
      "\tat com.inductiveautomation.ignition.gateway.dataroutes.Route.service(Route.java:355)",
      "\tat com.inductiveautomation.ignition.gateway.dataroutes.RouteGroupImpl.service(RouteGroupImpl.java:117)",
      "\tat com.inductiveautomation.ignition.gateway.dataroutes.RouteGroupCollectionServlet.serviceInternal(RouteGroupCollectionServlet.java:151)",
      "\tat com.inductiveautomation.ignition.gateway.dataroutes.AbstractRouteGroupServlet.service(AbstractRouteGroupServlet.java:40)",
      "\tat jakarta.servlet.http.HttpServlet.service(HttpServlet.java:614)",
      "\tat org.eclipse.jetty.ee10.servlet.ServletHolder$NotAsync.service(ServletHolder.java:1385)",
      "\tat org.eclipse.jetty.ee10.servlet.ServletHolder.handle(ServletHolder.java:751)",
      "\tat org.eclipse.jetty.ee10.servlet.ServletHandler$ChainEnd.doFilter(ServletHandler.java:1622)",
      "\tat com.inductiveautomation.catapult.filters.GatewayFilter.doFilter(GatewayFilter.java:119)",
      "\tat jakarta.servlet.http.HttpFilter.doFilter(HttpFilter.java:97)",
      "\tat org.eclipse.jetty.ee10.servlet.FilterHolder.doFilter(FilterHolder.java:205)",
      "\tat org.eclipse.jetty.ee10.servlet.ServletHandler$Chain.doFilter(ServletHandler.java:1594)",
      "\tat org.eclipse.jetty.ee10.servlet.ServletHandler$MappedServlet.handle(ServletHandler.java:1555)",
      "\tat org.eclipse.jetty.ee10.servlet.ServletChannel.dispatch(ServletChannel.java:823)",
      "\tat org.eclipse.jetty.ee10.servlet.ServletChannel.handle(ServletChannel.java:440)",
      "\tat org.eclipse.jetty.ee10.servlet.ServletHandler.handle(ServletHandler.java:470)",
      "\tat org.eclipse.jetty.security.SecurityHandler.handle(SecurityHandler.java:575)",
      "\tat org.eclipse.jetty.ee10.servlet.SessionHandler.handle(SessionHandler.java:717)",
      "\tat org.eclipse.jetty.server.handler.ContextHandler.handle(ContextHandler.java:1071)",
      "\tat org.eclipse.jetty.rewrite.handler.RewriteHandler$LastRuleHandler.handle(RewriteHandler.java:159)",
      "\tat org.eclipse.jetty.rewrite.handler.Rule$Handler.handle(Rule.java:108)",
      "\tat org.eclipse.jetty.rewrite.handler.HeaderPatternRule$1.handle(HeaderPatternRule.java:89)",
      "\tat org.eclipse.jetty.rewrite.handler.Rule$Handler.handle(Rule.java:108)",
      "\tat org.eclipse.jetty.rewrite.handler.HeaderPatternRule$1.handle(HeaderPatternRule.java:89)",
      "\tat org.eclipse.jetty.rewrite.handler.Rule$Handler.handle(Rule.java:108)",
      "\tat org.eclipse.jetty.rewrite.handler.HeaderPatternRule$1.handle(HeaderPatternRule.java:89)",
      "\tat org.eclipse.jetty.rewrite.handler.Rule$Handler.handle(Rule.java:108)",
      "\tat org.eclipse.jetty.rewrite.handler.HeaderPatternRule$1.handle(HeaderPatternRule.java:89)",
      "\tat org.eclipse.jetty.rewrite.handler.Rule$Handler.handle(Rule.java:108)",
      "\tat org.eclipse.jetty.rewrite.handler.RewriteHandler.handle(RewriteHandler.java:143)",
      "\tat org.eclipse.jetty.server.Handler$Sequence.handle(Handler.java:805)",
      "\tat org.eclipse.jetty.server.Handler$Sequence.handle(Handler.java:805)",
      "\tat org.eclipse.jetty.server.Server.handle(Server.java:182)",
      "\tat org.eclipse.jetty.server.internal.HttpChannelState$HandlerInvoker.run(HttpChannelState.java:677)",
      "\tat org.eclipse.jetty.server.internal.HttpConnection.onFillable(HttpConnection.java:416)",
      "\tat org.eclipse.jetty.io.AbstractConnection$ReadCallback.succeeded(AbstractConnection.java:322)",
      "\tat org.eclipse.jetty.io.FillInterest.fillable(FillInterest.java:105)",
      "\tat org.eclipse.jetty.io.SelectableChannelEndPoint$1.run(SelectableChannelEndPoint.java:53)",
      "\tat org.eclipse.jetty.util.thread.strategy.AdaptiveExecutionStrategy.runTask(AdaptiveExecutionStrategy.java:480)",
      "\tat org.eclipse.jetty.util.thread.strategy.AdaptiveExecutionStrategy.consumeTask(AdaptiveExecutionStrategy.java:443)",
      "\tat org.eclipse.jetty.util.thread.strategy.AdaptiveExecutionStrategy.tryProduce(AdaptiveExecutionStrategy.java:293)",
      "\tat org.eclipse.jetty.util.thread.strategy.AdaptiveExecutionStrategy.run(AdaptiveExecutionStrategy.java:201)",
      "\tat org.eclipse.jetty.util.thread.ReservedThreadExecutor$ReservedThread.run(ReservedThreadExecutor.java:311)",
      "\tat org.eclipse.jetty.util.thread.QueuedThreadPool.runJob(QueuedThreadPool.java:981)",
      "\tat org.eclipse.jetty.util.thread.QueuedThreadPool$Runner.doRunJob(QueuedThreadPool.java:1211)",
      "\tat org.eclipse.jetty.util.thread.QueuedThreadPool$Runner.run(QueuedThreadPool.java:1166)",
      "\tat java.base/java.lang.Thread.run(Unknown Source)"
    ]
  }
}
[curl http=500 total=0.018328s]
~~~

### GET find read-back after the refusal

~~~
GET /data/api/v1/resources/find/ignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701 @ 2026-10-12T14:35:03Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:35:03 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 10817

{
  "type": "ignition/database-connection",
  "name": "IGN_DEPLOY_RIG_DB_1791815701",
  "description": "c3a update",
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "8e01629a0ccc4115d874626df494edf5cdb1cd7510789a4ce9cc39cc85063f14",
  "config": {
    "driver": "PostgreSQL",
    "translator": "POSTGRES",
    "includeSchemaInTableName": false,
    "connectURL": "jdbc:postgresql://mode-host:5432/spike",
    "username": "spike",
    "connectionProps": "",
    "connectionResetParams": "",
    "defaultTransactionLevel": "DEFAULT",
    "poolInitSize": 0,
    "poolMaxActive": 9,
    "poolMaxIdle": 8,
    "poolMinIdle": 0,
    "poolMaxWait": 5000,
    "validationQuery": "SELECT 1",
    "testOnBorrow": true,
    "testOnReturn": false,
    "testWhileIdle": false,
    "evictionRate": -1,
    "evictionTests": 3,
    "evictionTime": 1800000,
    "failoverMode": "STANDARD",
    "slowQueryLogThreshold": 60000,
    "validationSleepTime": 10000
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "lastModification": {
      "actor": "api-token:cap-a:192.168.97.1",
      "timestamp": "2026-10-12T14:35:03Z"
    },
    "uuid": "0939907f-c2a5-44d8-ac13-2a8d252f312f",
    "lastModificationSignature": "ffc636164e849e022d4eee0e76ab409a129a36e526d354c2609eab2615128172",
    "enabled": true
  },
  "metrics": {
    "active-connections": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.active-connections",
      "metric": {
        "value": 0.0,
        "numerator": 0.0,
        "denominator": 9.0,
        "type": "ratio-gauge",
        "description": "Active Connections",
        "units": ""
      }
    },
    "active-queries": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.active-queries",
      "metric": {
        "value": [],
        "type": "gauge",
        "description": "Active Queries",
        "units": "queries"
      }
    },
    "expensive-queries": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.expensive-queries",
      "metric": {
        "value": [],
        "type": "gauge",
        "description": "Expensive Queries",
        "units": "queries"
      }
    },
    "queries": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.queries",
      "metric": {
        "oneMinuteRate": 0.0,
        "fiveMinuteRate": 0.0,
        "fifteenMinuteRate": 0.0,
        "count": 0,
        "meanRate": 0.0,
        "histogram": {
          "max": 0.0,
          "min": 0.0,
          "p50": 0.0,
          "p75": 0.0,
          "p95": 0.0,
          "p98": 0.0,
          "p99": 0.0,
          "mean": 0.0,
          "bucketSize": 0.0,
          "buckets": [
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0
          ],
          "type": "histogram",
          "stdev": 0.0
        },
        "type": "timer"
      }
    },
    "rows": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.rows",
      "metric": {
        "oneMinuteRate": 0.0,
        "fiveMinuteRate": 0.0,
        "fifteenMinuteRate": 0.0,
        "count": 0,
        "meanRate": 0.0,
        "type": "meter"
      }
    },
    "throughput": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.throughput",
      "metric": {
        "value": -1.0,
        "type": "gauge",
        "description": "",
        "units": "queries/sec"
      }
    }
  },
  "healthchecks": {
    "status": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.status",
      "result": {
        "healthy": false,
        "time": 1791815703392,
        "message": "Faulted",
        "error": {
          "message": "Cannot create PoolableConnectionFactory (The connection attempt failed.)",
          "stacktrace": [
            "java.sql.SQLException: Cannot create PoolableConnectionFactory (The connection attempt failed.)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:645)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)",
            "\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)",
            "\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)",
            "\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)",
            "\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)",
            "\tat java.base/java.lang.Thread.run(Unknown Source)",
            "Caused by: org.postgresql.util.PSQLException: The connection attempt failed.",
            "\tat org.postgresql.core.v3.ConnectionFactoryImpl.openConnectionImpl(ConnectionFactoryImpl.java:358)",
            "\tat org.postgresql.core.ConnectionFactory.openConnection(ConnectionFactory.java:54)",
            "\tat org.postgresql.jdbc.PgConnection.\u003cinit\u003e(PgConnection.java:273)",
            "\tat org.postgresql.Driver.makeConnection(Driver.java:446)",
            "\tat org.postgresql.Driver.connect(Driver.java:298)",
            "\tat org.apache.commons.dbcp2.DriverConnectionFactory.createConnection(DriverConnectionFactory.java:52)",
            "\tat org.apache.commons.dbcp2.PoolableConnectionFactory.makeObject(PoolableConnectionFactory.java:481)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.validateConnectionFactory(BasicDataSource.java:114)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:641)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)",
            "\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)",
            "\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)",
            "\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)",
            "\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)",
            "\tat java.base/java.lang.Thread.run(Unknown Source)",
            "Caused by: java.net.UnknownHostException: mode-host",
            "\tat java.base/sun.nio.ch.NioSocketImpl.connect(Unknown Source)",
            "\tat java.base/java.net.SocksSocketImpl.connect(Unknown Source)",
            "\tat java.base/java.net.Socket.connect(Unknown Source)",
            "\tat org.postgresql.core.PGStream.createSocket(PGStream.java:243)",
            "\tat org.postgresql.core.PGStream.\u003cinit\u003e(PGStream.java:98)",
            "\tat org.postgresql.core.v3.ConnectionFactoryImpl.tryConnect(ConnectionFactoryImpl.java:136)",
            "\tat org.postgresql.core.v3.ConnectionFactoryImpl.openConnectionImpl(ConnectionFactoryImpl.java:262)",
            "\tat org.postgresql.core.ConnectionFactory.openConnection(ConnectionFactory.java:54)",
            "\tat org.postgresql.jdbc.PgConnection.\u003cinit\u003e(PgConnection.java:273)",
            "\tat org.postgresql.Driver.makeConnection(Driver.java:446)",
            "\tat org.postgresql.Driver.connect(Driver.java:298)",
            "\tat org.apache.commons.dbcp2.DriverConnectionFactory.createConnection(DriverConnectionFactory.java:52)",
            "\tat org.apache.commons.dbcp2.PoolableConnectionFactory.makeObject(PoolableConnectionFactory.java:481)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.validateConnectionFactory(BasicDataSource.java:114)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:641)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)",
            "\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)",
            "\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)",
            "\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)",
            "\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)",
            "\tat java.base/java.lang.Thread.run(Unknown Source)"
          ]
        }
      }
    }
  }
}
[curl http=200 total=0.022804s]
~~~

~~~
unchanged check: signature=8e01629a0ccc4115d874626df494edf5cdb1cd7510789a4ce9cc39cc85063f14 expected=8e01629a0ccc4115d874626df494edf5cdb1cd7510789a4ce9cc39cc85063f14 poolMaxActive=9 description=c3a update
~~~

### PUT with the signature key omitted

~~~
PUT /data/api/v1/resources/ignition/database-connection @ 2026-10-12T14:35:03Z
HTTP/1.1 422 Unprocessable Entity
Date: Mon, 12 Oct 2026 14:35:03 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 168

{
  "messages": [],
  "fieldMessages": [
    {
      "fieldName": "signature",
      "messages": [
        "Missing \u0027signature\u0027 parameter"
      ]
    }
  ]
}
[curl http=422 total=0.010854s]
~~~

### GET find read-back after the refusal

~~~
GET /data/api/v1/resources/find/ignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701 @ 2026-10-12T14:35:03Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:35:03 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 10817

{
  "type": "ignition/database-connection",
  "name": "IGN_DEPLOY_RIG_DB_1791815701",
  "description": "c3a update",
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "8e01629a0ccc4115d874626df494edf5cdb1cd7510789a4ce9cc39cc85063f14",
  "config": {
    "driver": "PostgreSQL",
    "translator": "POSTGRES",
    "includeSchemaInTableName": false,
    "connectURL": "jdbc:postgresql://mode-host:5432/spike",
    "username": "spike",
    "connectionProps": "",
    "connectionResetParams": "",
    "defaultTransactionLevel": "DEFAULT",
    "poolInitSize": 0,
    "poolMaxActive": 9,
    "poolMaxIdle": 8,
    "poolMinIdle": 0,
    "poolMaxWait": 5000,
    "validationQuery": "SELECT 1",
    "testOnBorrow": true,
    "testOnReturn": false,
    "testWhileIdle": false,
    "evictionRate": -1,
    "evictionTests": 3,
    "evictionTime": 1800000,
    "failoverMode": "STANDARD",
    "slowQueryLogThreshold": 60000,
    "validationSleepTime": 10000
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "lastModification": {
      "actor": "api-token:cap-a:192.168.97.1",
      "timestamp": "2026-10-12T14:35:03Z"
    },
    "uuid": "0939907f-c2a5-44d8-ac13-2a8d252f312f",
    "lastModificationSignature": "ffc636164e849e022d4eee0e76ab409a129a36e526d354c2609eab2615128172",
    "enabled": true
  },
  "metrics": {
    "active-connections": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.active-connections",
      "metric": {
        "value": 0.0,
        "numerator": 0.0,
        "denominator": 9.0,
        "type": "ratio-gauge",
        "description": "Active Connections",
        "units": ""
      }
    },
    "active-queries": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.active-queries",
      "metric": {
        "value": [],
        "type": "gauge",
        "description": "Active Queries",
        "units": "queries"
      }
    },
    "expensive-queries": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.expensive-queries",
      "metric": {
        "value": [],
        "type": "gauge",
        "description": "Expensive Queries",
        "units": "queries"
      }
    },
    "queries": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.queries",
      "metric": {
        "oneMinuteRate": 0.0,
        "fiveMinuteRate": 0.0,
        "fifteenMinuteRate": 0.0,
        "count": 0,
        "meanRate": 0.0,
        "histogram": {
          "max": 0.0,
          "min": 0.0,
          "p50": 0.0,
          "p75": 0.0,
          "p95": 0.0,
          "p98": 0.0,
          "p99": 0.0,
          "mean": 0.0,
          "bucketSize": 0.0,
          "buckets": [
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0
          ],
          "type": "histogram",
          "stdev": 0.0
        },
        "type": "timer"
      }
    },
    "rows": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.rows",
      "metric": {
        "oneMinuteRate": 0.0,
        "fiveMinuteRate": 0.0,
        "fifteenMinuteRate": 0.0,
        "count": 0,
        "meanRate": 0.0,
        "type": "meter"
      }
    },
    "throughput": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.throughput",
      "metric": {
        "value": -1.0,
        "type": "gauge",
        "description": "",
        "units": "queries/sec"
      }
    }
  },
  "healthchecks": {
    "status": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.status",
      "result": {
        "healthy": false,
        "time": 1791815703522,
        "message": "Faulted",
        "error": {
          "message": "Cannot create PoolableConnectionFactory (The connection attempt failed.)",
          "stacktrace": [
            "java.sql.SQLException: Cannot create PoolableConnectionFactory (The connection attempt failed.)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:645)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)",
            "\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)",
            "\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)",
            "\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)",
            "\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)",
            "\tat java.base/java.lang.Thread.run(Unknown Source)",
            "Caused by: org.postgresql.util.PSQLException: The connection attempt failed.",
            "\tat org.postgresql.core.v3.ConnectionFactoryImpl.openConnectionImpl(ConnectionFactoryImpl.java:358)",
            "\tat org.postgresql.core.ConnectionFactory.openConnection(ConnectionFactory.java:54)",
            "\tat org.postgresql.jdbc.PgConnection.\u003cinit\u003e(PgConnection.java:273)",
            "\tat org.postgresql.Driver.makeConnection(Driver.java:446)",
            "\tat org.postgresql.Driver.connect(Driver.java:298)",
            "\tat org.apache.commons.dbcp2.DriverConnectionFactory.createConnection(DriverConnectionFactory.java:52)",
            "\tat org.apache.commons.dbcp2.PoolableConnectionFactory.makeObject(PoolableConnectionFactory.java:481)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.validateConnectionFactory(BasicDataSource.java:114)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:641)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)",
            "\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)",
            "\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)",
            "\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)",
            "\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)",
            "\tat java.base/java.lang.Thread.run(Unknown Source)",
            "Caused by: java.net.UnknownHostException: mode-host",
            "\tat java.base/sun.nio.ch.NioSocketImpl.connect(Unknown Source)",
            "\tat java.base/java.net.SocksSocketImpl.connect(Unknown Source)",
            "\tat java.base/java.net.Socket.connect(Unknown Source)",
            "\tat org.postgresql.core.PGStream.createSocket(PGStream.java:243)",
            "\tat org.postgresql.core.PGStream.\u003cinit\u003e(PGStream.java:98)",
            "\tat org.postgresql.core.v3.ConnectionFactoryImpl.tryConnect(ConnectionFactoryImpl.java:136)",
            "\tat org.postgresql.core.v3.ConnectionFactoryImpl.openConnectionImpl(ConnectionFactoryImpl.java:262)",
            "\tat org.postgresql.core.ConnectionFactory.openConnection(ConnectionFactory.java:54)",
            "\tat org.postgresql.jdbc.PgConnection.\u003cinit\u003e(PgConnection.java:273)",
            "\tat org.postgresql.Driver.makeConnection(Driver.java:446)",
            "\tat org.postgresql.Driver.connect(Driver.java:298)",
            "\tat org.apache.commons.dbcp2.DriverConnectionFactory.createConnection(DriverConnectionFactory.java:52)",
            "\tat org.apache.commons.dbcp2.PoolableConnectionFactory.makeObject(PoolableConnectionFactory.java:481)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.validateConnectionFactory(BasicDataSource.java:114)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:641)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)",
            "\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)",
            "\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)",
            "\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)",
            "\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)",
            "\tat java.base/java.lang.Thread.run(Unknown Source)"
          ]
        }
      }
    }
  }
}
[curl http=200 total=0.016112s]
~~~

~~~
unchanged check: signature=8e01629a0ccc4115d874626df494edf5cdb1cd7510789a4ce9cc39cc85063f14 expected=8e01629a0ccc4115d874626df494edf5cdb1cd7510789a4ce9cc39cc85063f14 poolMaxActive=9 description=c3a update
~~~

### PUT with an empty-string signature

~~~
PUT /data/api/v1/resources/ignition/database-connection @ 2026-10-12T14:35:03Z
HTTP/1.1 422 Unprocessable Entity
Date: Mon, 12 Oct 2026 14:35:03 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 168

{
  "messages": [],
  "fieldMessages": [
    {
      "fieldName": "signature",
      "messages": [
        "Missing \u0027signature\u0027 parameter"
      ]
    }
  ]
}
[curl http=422 total=0.009440s]
~~~

### GET find read-back after the refusal

~~~
GET /data/api/v1/resources/find/ignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701 @ 2026-10-12T14:35:03Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:35:03 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 10817

{
  "type": "ignition/database-connection",
  "name": "IGN_DEPLOY_RIG_DB_1791815701",
  "description": "c3a update",
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "8e01629a0ccc4115d874626df494edf5cdb1cd7510789a4ce9cc39cc85063f14",
  "config": {
    "driver": "PostgreSQL",
    "translator": "POSTGRES",
    "includeSchemaInTableName": false,
    "connectURL": "jdbc:postgresql://mode-host:5432/spike",
    "username": "spike",
    "connectionProps": "",
    "connectionResetParams": "",
    "defaultTransactionLevel": "DEFAULT",
    "poolInitSize": 0,
    "poolMaxActive": 9,
    "poolMaxIdle": 8,
    "poolMinIdle": 0,
    "poolMaxWait": 5000,
    "validationQuery": "SELECT 1",
    "testOnBorrow": true,
    "testOnReturn": false,
    "testWhileIdle": false,
    "evictionRate": -1,
    "evictionTests": 3,
    "evictionTime": 1800000,
    "failoverMode": "STANDARD",
    "slowQueryLogThreshold": 60000,
    "validationSleepTime": 10000
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "lastModification": {
      "actor": "api-token:cap-a:192.168.97.1",
      "timestamp": "2026-10-12T14:35:03Z"
    },
    "uuid": "0939907f-c2a5-44d8-ac13-2a8d252f312f",
    "lastModificationSignature": "ffc636164e849e022d4eee0e76ab409a129a36e526d354c2609eab2615128172",
    "enabled": true
  },
  "metrics": {
    "active-connections": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.active-connections",
      "metric": {
        "value": 0.0,
        "numerator": 0.0,
        "denominator": 9.0,
        "type": "ratio-gauge",
        "description": "Active Connections",
        "units": ""
      }
    },
    "active-queries": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.active-queries",
      "metric": {
        "value": [],
        "type": "gauge",
        "description": "Active Queries",
        "units": "queries"
      }
    },
    "expensive-queries": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.expensive-queries",
      "metric": {
        "value": [],
        "type": "gauge",
        "description": "Expensive Queries",
        "units": "queries"
      }
    },
    "queries": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.queries",
      "metric": {
        "oneMinuteRate": 0.0,
        "fiveMinuteRate": 0.0,
        "fifteenMinuteRate": 0.0,
        "count": 0,
        "meanRate": 0.0,
        "histogram": {
          "max": 0.0,
          "min": 0.0,
          "p50": 0.0,
          "p75": 0.0,
          "p95": 0.0,
          "p98": 0.0,
          "p99": 0.0,
          "mean": 0.0,
          "bucketSize": 0.0,
          "buckets": [
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0
          ],
          "type": "histogram",
          "stdev": 0.0
        },
        "type": "timer"
      }
    },
    "rows": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.rows",
      "metric": {
        "oneMinuteRate": 0.0,
        "fiveMinuteRate": 0.0,
        "fifteenMinuteRate": 0.0,
        "count": 0,
        "meanRate": 0.0,
        "type": "meter"
      }
    },
    "throughput": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.throughput",
      "metric": {
        "value": -1.0,
        "type": "gauge",
        "description": "",
        "units": "queries/sec"
      }
    }
  },
  "healthchecks": {
    "status": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.status",
      "result": {
        "healthy": false,
        "time": 1791815703655,
        "message": "Faulted",
        "error": {
          "message": "Cannot create PoolableConnectionFactory (The connection attempt failed.)",
          "stacktrace": [
            "java.sql.SQLException: Cannot create PoolableConnectionFactory (The connection attempt failed.)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:645)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)",
            "\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)",
            "\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)",
            "\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)",
            "\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)",
            "\tat java.base/java.lang.Thread.run(Unknown Source)",
            "Caused by: org.postgresql.util.PSQLException: The connection attempt failed.",
            "\tat org.postgresql.core.v3.ConnectionFactoryImpl.openConnectionImpl(ConnectionFactoryImpl.java:358)",
            "\tat org.postgresql.core.ConnectionFactory.openConnection(ConnectionFactory.java:54)",
            "\tat org.postgresql.jdbc.PgConnection.\u003cinit\u003e(PgConnection.java:273)",
            "\tat org.postgresql.Driver.makeConnection(Driver.java:446)",
            "\tat org.postgresql.Driver.connect(Driver.java:298)",
            "\tat org.apache.commons.dbcp2.DriverConnectionFactory.createConnection(DriverConnectionFactory.java:52)",
            "\tat org.apache.commons.dbcp2.PoolableConnectionFactory.makeObject(PoolableConnectionFactory.java:481)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.validateConnectionFactory(BasicDataSource.java:114)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:641)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)",
            "\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)",
            "\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)",
            "\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)",
            "\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)",
            "\tat java.base/java.lang.Thread.run(Unknown Source)",
            "Caused by: java.net.UnknownHostException: mode-host",
            "\tat java.base/sun.nio.ch.NioSocketImpl.connect(Unknown Source)",
            "\tat java.base/java.net.SocksSocketImpl.connect(Unknown Source)",
            "\tat java.base/java.net.Socket.connect(Unknown Source)",
            "\tat org.postgresql.core.PGStream.createSocket(PGStream.java:243)",
            "\tat org.postgresql.core.PGStream.\u003cinit\u003e(PGStream.java:98)",
            "\tat org.postgresql.core.v3.ConnectionFactoryImpl.tryConnect(ConnectionFactoryImpl.java:136)",
            "\tat org.postgresql.core.v3.ConnectionFactoryImpl.openConnectionImpl(ConnectionFactoryImpl.java:262)",
            "\tat org.postgresql.core.ConnectionFactory.openConnection(ConnectionFactory.java:54)",
            "\tat org.postgresql.jdbc.PgConnection.\u003cinit\u003e(PgConnection.java:273)",
            "\tat org.postgresql.Driver.makeConnection(Driver.java:446)",
            "\tat org.postgresql.Driver.connect(Driver.java:298)",
            "\tat org.apache.commons.dbcp2.DriverConnectionFactory.createConnection(DriverConnectionFactory.java:52)",
            "\tat org.apache.commons.dbcp2.PoolableConnectionFactory.makeObject(PoolableConnectionFactory.java:481)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.validateConnectionFactory(BasicDataSource.java:114)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:641)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)",
            "\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)",
            "\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)",
            "\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)",
            "\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)",
            "\tat java.base/java.lang.Thread.run(Unknown Source)"
          ]
        }
      }
    }
  }
}
[curl http=200 total=0.028273s]
~~~

~~~
unchanged check: signature=8e01629a0ccc4115d874626df494edf5cdb1cd7510789a4ce9cc39cc85063f14 expected=8e01629a0ccc4115d874626df494edf5cdb1cd7510789a4ce9cc39cc85063f14 poolMaxActive=9 description=c3a update
~~~

### PUT of an unknown name (with the current signature)

~~~
PUT /data/api/v1/resources/ignition/database-connection @ 2026-10-12T14:35:03Z
HTTP/1.1 404 Not Found
Date: Mon, 12 Oct 2026 14:35:03 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Length: 0


[curl http=404 total=0.015865s]
~~~

### GET find read-back after the refusal

~~~
GET /data/api/v1/resources/find/ignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701 @ 2026-10-12T14:35:03Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:35:03 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 10817

{
  "type": "ignition/database-connection",
  "name": "IGN_DEPLOY_RIG_DB_1791815701",
  "description": "c3a update",
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "8e01629a0ccc4115d874626df494edf5cdb1cd7510789a4ce9cc39cc85063f14",
  "config": {
    "driver": "PostgreSQL",
    "translator": "POSTGRES",
    "includeSchemaInTableName": false,
    "connectURL": "jdbc:postgresql://mode-host:5432/spike",
    "username": "spike",
    "connectionProps": "",
    "connectionResetParams": "",
    "defaultTransactionLevel": "DEFAULT",
    "poolInitSize": 0,
    "poolMaxActive": 9,
    "poolMaxIdle": 8,
    "poolMinIdle": 0,
    "poolMaxWait": 5000,
    "validationQuery": "SELECT 1",
    "testOnBorrow": true,
    "testOnReturn": false,
    "testWhileIdle": false,
    "evictionRate": -1,
    "evictionTests": 3,
    "evictionTime": 1800000,
    "failoverMode": "STANDARD",
    "slowQueryLogThreshold": 60000,
    "validationSleepTime": 10000
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "lastModification": {
      "actor": "api-token:cap-a:192.168.97.1",
      "timestamp": "2026-10-12T14:35:03Z"
    },
    "uuid": "0939907f-c2a5-44d8-ac13-2a8d252f312f",
    "lastModificationSignature": "ffc636164e849e022d4eee0e76ab409a129a36e526d354c2609eab2615128172",
    "enabled": true
  },
  "metrics": {
    "active-connections": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.active-connections",
      "metric": {
        "value": 0.0,
        "numerator": 0.0,
        "denominator": 9.0,
        "type": "ratio-gauge",
        "description": "Active Connections",
        "units": ""
      }
    },
    "active-queries": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.active-queries",
      "metric": {
        "value": [],
        "type": "gauge",
        "description": "Active Queries",
        "units": "queries"
      }
    },
    "expensive-queries": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.expensive-queries",
      "metric": {
        "value": [],
        "type": "gauge",
        "description": "Expensive Queries",
        "units": "queries"
      }
    },
    "queries": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.queries",
      "metric": {
        "oneMinuteRate": 0.0,
        "fiveMinuteRate": 0.0,
        "fifteenMinuteRate": 0.0,
        "count": 0,
        "meanRate": 0.0,
        "histogram": {
          "max": 0.0,
          "min": 0.0,
          "p50": 0.0,
          "p75": 0.0,
          "p95": 0.0,
          "p98": 0.0,
          "p99": 0.0,
          "mean": 0.0,
          "bucketSize": 0.0,
          "buckets": [
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0
          ],
          "type": "histogram",
          "stdev": 0.0
        },
        "type": "timer"
      }
    },
    "rows": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.rows",
      "metric": {
        "oneMinuteRate": 0.0,
        "fiveMinuteRate": 0.0,
        "fifteenMinuteRate": 0.0,
        "count": 0,
        "meanRate": 0.0,
        "type": "meter"
      }
    },
    "throughput": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.throughput",
      "metric": {
        "value": -1.0,
        "type": "gauge",
        "description": "",
        "units": "queries/sec"
      }
    }
  },
  "healthchecks": {
    "status": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.status",
      "result": {
        "healthy": false,
        "time": 1791815703787,
        "message": "Faulted",
        "error": {
          "message": "Cannot create PoolableConnectionFactory (The connection attempt failed.)",
          "stacktrace": [
            "java.sql.SQLException: Cannot create PoolableConnectionFactory (The connection attempt failed.)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:645)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)",
            "\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)",
            "\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)",
            "\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)",
            "\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)",
            "\tat java.base/java.lang.Thread.run(Unknown Source)",
            "Caused by: org.postgresql.util.PSQLException: The connection attempt failed.",
            "\tat org.postgresql.core.v3.ConnectionFactoryImpl.openConnectionImpl(ConnectionFactoryImpl.java:358)",
            "\tat org.postgresql.core.ConnectionFactory.openConnection(ConnectionFactory.java:54)",
            "\tat org.postgresql.jdbc.PgConnection.\u003cinit\u003e(PgConnection.java:273)",
            "\tat org.postgresql.Driver.makeConnection(Driver.java:446)",
            "\tat org.postgresql.Driver.connect(Driver.java:298)",
            "\tat org.apache.commons.dbcp2.DriverConnectionFactory.createConnection(DriverConnectionFactory.java:52)",
            "\tat org.apache.commons.dbcp2.PoolableConnectionFactory.makeObject(PoolableConnectionFactory.java:481)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.validateConnectionFactory(BasicDataSource.java:114)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:641)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)",
            "\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)",
            "\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)",
            "\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)",
            "\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)",
            "\tat java.base/java.lang.Thread.run(Unknown Source)",
            "Caused by: java.net.UnknownHostException: mode-host",
            "\tat java.base/sun.nio.ch.NioSocketImpl.connect(Unknown Source)",
            "\tat java.base/java.net.SocksSocketImpl.connect(Unknown Source)",
            "\tat java.base/java.net.Socket.connect(Unknown Source)",
            "\tat org.postgresql.core.PGStream.createSocket(PGStream.java:243)",
            "\tat org.postgresql.core.PGStream.\u003cinit\u003e(PGStream.java:98)",
            "\tat org.postgresql.core.v3.ConnectionFactoryImpl.tryConnect(ConnectionFactoryImpl.java:136)",
            "\tat org.postgresql.core.v3.ConnectionFactoryImpl.openConnectionImpl(ConnectionFactoryImpl.java:262)",
            "\tat org.postgresql.core.ConnectionFactory.openConnection(ConnectionFactory.java:54)",
            "\tat org.postgresql.jdbc.PgConnection.\u003cinit\u003e(PgConnection.java:273)",
            "\tat org.postgresql.Driver.makeConnection(Driver.java:446)",
            "\tat org.postgresql.Driver.connect(Driver.java:298)",
            "\tat org.apache.commons.dbcp2.DriverConnectionFactory.createConnection(DriverConnectionFactory.java:52)",
            "\tat org.apache.commons.dbcp2.PoolableConnectionFactory.makeObject(PoolableConnectionFactory.java:481)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.validateConnectionFactory(BasicDataSource.java:114)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:641)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)",
            "\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)",
            "\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)",
            "\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)",
            "\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)",
            "\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)",
            "\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)",
            "\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)",
            "\tat java.base/java.lang.Thread.run(Unknown Source)"
          ]
        }
      }
    }
  }
}
[curl http=200 total=0.012216s]
~~~

~~~
unchanged check: signature=8e01629a0ccc4115d874626df494edf5cdb1cd7510789a4ce9cc39cc85063f14 expected=8e01629a0ccc4115d874626df494edf5cdb1cd7510789a4ce9cc39cc85063f14 poolMaxActive=9 description=c3a update
~~~

### GET find of the unknown name (never created?)

~~~
GET /data/api/v1/resources/find/ignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701_NOPE @ 2026-10-12T14:35:03Z
HTTP/1.1 404 Not Found
Date: Mon, 12 Oct 2026 14:35:03 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 545

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 Resource not found: ignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701_NOPE</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/resources/find/ignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701_NOPE</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>Resource not found: ignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701_NOPE</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.012662s]
~~~
