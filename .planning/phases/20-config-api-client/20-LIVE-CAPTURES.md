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

## C-null (mode production)

### GET gateway-info (raw)

~~~
GET /data/api/v1/gateway-info @ 2026-10-12T14:37:44Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:37:44 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 416

{"name":"ign-deploy-rig","redundancyRole":"Independent","edition":"standard","hostname":"localhost","port":"18290","ignitionVersion":"8.3.6 (b2026042713)","deploymentMode":"production","timeZone":"Coordinated Universal Time","timeZoneId":"Etc/UTC","jvmVersion":"17.0.18","allowUnsignedModules":false,"license":{"mode":"Trial","validForVersion":8,"expirationDate":"Oct 12, 2026, 4:32:16 PM","licenseRestrictions":[]}}
[curl http=200 total=0.102770s]
~~~

~~~
{"has":true,"v":"production","t":"string"}
~~~

## C-list

### POST the C3a name into collection production

~~~
POST /data/api/v1/resources/ignition/database-connection @ 2026-10-12T14:37:44Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:37:44 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 287

{
  "success": true,
  "changes": [
    {
      "name": "IGN_DEPLOY_RIG_DB_1791815701",
      "type": "ignition/database-connection",
      "collection": "production",
      "newSignature": "c3ee2b70350a263381b5c1a4f92c3cdacb26ea949f255a48ead9a468aaeafcb9"
    }
  ],
  "problem": null
}
[curl http=200 total=0.327267s]
~~~

### GET find, no collection (effective view)

~~~
GET /data/api/v1/resources/find/ignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701 @ 2026-10-12T14:37:44Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:37:44 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 10636

{
  "type": "ignition/database-connection",
  "name": "IGN_DEPLOY_RIG_DB_1791815701",
  "description": "c-list production override",
  "enabled": true,
  "version": 1,
  "collection": "production",
  "collections": [
    "core",
    "production"
  ],
  "signature": "c3ee2b70350a263381b5c1a4f92c3cdacb26ea949f255a48ead9a468aaeafcb9",
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
    "poolMaxActive": 20,
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
    "uuid": "ef39fc66-c7b4-441a-a907-b02df07b17aa",
    "enabled": true
  },
  "metrics": {
    "active-connections": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.active-connections",
      "metric": {
        "value": 0.0,
        "numerator": 0.0,
        "denominator": 20.0,
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
        "time": 1791815864963,
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
[curl http=200 total=0.052170s]
~~~

### GET find?collection=core

~~~
GET /data/api/v1/resources/find/ignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701?collection=core @ 2026-10-12T14:37:45Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:37:45 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 10836

{
  "type": "ignition/database-connection",
  "name": "IGN_DEPLOY_RIG_DB_1791815701",
  "description": "c3a update",
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core",
    "production"
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
        "denominator": 20.0,
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
        "time": 1791815865048,
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
[curl http=200 total=0.032479s]
~~~

### GET find?collection=production

~~~
GET /data/api/v1/resources/find/ignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701?collection=production @ 2026-10-12T14:37:45Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:37:45 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 10636

{
  "type": "ignition/database-connection",
  "name": "IGN_DEPLOY_RIG_DB_1791815701",
  "description": "c-list production override",
  "enabled": true,
  "version": 1,
  "collection": "production",
  "collections": [
    "core",
    "production"
  ],
  "signature": "c3ee2b70350a263381b5c1a4f92c3cdacb26ea949f255a48ead9a468aaeafcb9",
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
    "poolMaxActive": 20,
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
    "uuid": "ef39fc66-c7b4-441a-a907-b02df07b17aa",
    "enabled": true
  },
  "metrics": {
    "active-connections": {
      "name": "databases.connection-IGN_DEPLOY_RIG_DB_1791815701.active-connections",
      "metric": {
        "value": 0.0,
        "numerator": 0.0,
        "denominator": 20.0,
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
        "time": 1791815865094,
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
[curl http=200 total=0.024016s]
~~~

### GET list?limit=-1

~~~
GET /data/api/v1/resources/list/ignition/database-connection?limit=-1 @ 2026-10-12T14:37:45Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:37:45 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 8552

{"items":[{"type":"ignition/database-connection","name":"IGN_DEPLOY_RIG_DB_1791815701","description":"c-list production override","enabled":true,"version":1,"collection":"production","collections":["core","production"],"signature":"c3ee2b70350a263381b5c1a4f92c3cdacb26ea949f255a48ead9a468aaeafcb9","config":{"driver":"PostgreSQL","translator":"POSTGRES","includeSchemaInTableName":false,"connectURL":"jdbc:postgresql://mode-host:5432/spike","username":"spike","connectionProps":"","connectionResetParams":"","defaultTransactionLevel":"DEFAULT","poolInitSize":0,"poolMaxActive":20,"poolMaxIdle":8,"poolMinIdle":0,"poolMaxWait":5000,"validationQuery":"SELECT 1","testOnBorrow":true,"testOnReturn":false,"testWhileIdle":false,"evictionRate":-1,"evictionTests":3,"evictionTime":1800000,"failoverMode":"STANDARD","slowQueryLogThreshold":60000,"validationSleepTime":10000},"data":["config.json"],"attributes":{"uuid":"ef39fc66-c7b4-441a-a907-b02df07b17aa","enabled":true},"metrics":{"active-connections":{"name":"databases.connection-IGN_DEPLOY_RIG_DB_1791815701.active-connections","metric":{"value":0.0,"numerator":0.0,"denominator":20.0,"type":"ratio-gauge","description":"Active Connections","units":""}},"active-queries":{"name":"databases.connection-IGN_DEPLOY_RIG_DB_1791815701.active-queries","metric":{"value":[],"type":"gauge","description":"Active Queries","units":"queries"}},"expensive-queries":{"name":"databases.connection-IGN_DEPLOY_RIG_DB_1791815701.expensive-queries","metric":{"value":[],"type":"gauge","description":"Expensive Queries","units":"queries"}},"queries":{"name":"databases.connection-IGN_DEPLOY_RIG_DB_1791815701.queries","metric":{"oneMinuteRate":0.0,"fiveMinuteRate":0.0,"fifteenMinuteRate":0.0,"count":0,"meanRate":0.0,"histogram":{"max":0.0,"min":0.0,"p50":0.0,"p75":0.0,"p95":0.0,"p98":0.0,"p99":0.0,"mean":0.0,"bucketSize":0.0,"buckets":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"type":"histogram","stdev":0.0},"type":"timer"}},"rows":{"name":"databases.connection-IGN_DEPLOY_RIG_DB_1791815701.rows","metric":{"oneMinuteRate":0.0,"fiveMinuteRate":0.0,"fifteenMinuteRate":0.0,"count":0,"meanRate":0.0,"type":"meter"}},"throughput":{"name":"databases.connection-IGN_DEPLOY_RIG_DB_1791815701.throughput","metric":{"value":-1.0,"type":"gauge","description":"","units":"queries/sec"}}},"healthchecks":{"status":{"name":"databases.connection-IGN_DEPLOY_RIG_DB_1791815701.status","result":{"healthy":false,"time":1791815865160,"message":"Faulted","error":{"message":"Cannot create PoolableConnectionFactory (The connection attempt failed.)","stacktrace":["java.sql.SQLException: Cannot create PoolableConnectionFactory (The connection attempt failed.)","\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:645)","\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)","\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)","\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)","\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)","\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)","\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)","\tat java.base/java.lang.Thread.run(Unknown Source)","Caused by: org.postgresql.util.PSQLException: The connection attempt failed.","\tat org.postgresql.core.v3.ConnectionFactoryImpl.openConnectionImpl(ConnectionFactoryImpl.java:358)","\tat org.postgresql.core.ConnectionFactory.openConnection(ConnectionFactory.java:54)","\tat org.postgresql.jdbc.PgConnection.<init>(PgConnection.java:273)","\tat org.postgresql.Driver.makeConnection(Driver.java:446)","\tat org.postgresql.Driver.connect(Driver.java:298)","\tat org.apache.commons.dbcp2.DriverConnectionFactory.createConnection(DriverConnectionFactory.java:52)","\tat org.apache.commons.dbcp2.PoolableConnectionFactory.makeObject(PoolableConnectionFactory.java:481)","\tat org.apache.commons.dbcp2.BasicDataSource.validateConnectionFactory(BasicDataSource.java:114)","\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:641)","\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)","\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)","\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)","\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)","\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)","\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)","\tat java.base/java.lang.Thread.run(Unknown Source)","Caused by: java.net.UnknownHostException: mode-host","\tat java.base/sun.nio.ch.NioSocketImpl.connect(Unknown Source)","\tat java.base/java.net.SocksSocketImpl.connect(Unknown Source)","\tat java.base/java.net.Socket.connect(Unknown Source)","\tat org.postgresql.core.PGStream.createSocket(PGStream.java:243)","\tat org.postgresql.core.PGStream.<init>(PGStream.java:98)","\tat org.postgresql.core.v3.ConnectionFactoryImpl.tryConnect(ConnectionFactoryImpl.java:136)","\tat org.postgresql.core.v3.ConnectionFactoryImpl.openConnectionImpl(ConnectionFactoryImpl.java:262)","\tat org.postgresql.core.ConnectionFactory.openConnection(ConnectionFactory.java:54)","\tat org.postgresql.jdbc.PgConnection.<init>(PgConnection.java:273)","\tat org.postgresql.Driver.makeConnection(Driver.java:446)","\tat org.postgresql.Driver.connect(Driver.java:298)","\tat org.apache.commons.dbcp2.DriverConnectionFactory.createConnection(DriverConnectionFactory.java:52)","\tat org.apache.commons.dbcp2.PoolableConnectionFactory.makeObject(PoolableConnectionFactory.java:481)","\tat org.apache.commons.dbcp2.BasicDataSource.validateConnectionFactory(BasicDataSource.java:114)","\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:641)","\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)","\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)","\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)","\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)","\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)","\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)","\tat java.base/java.lang.Thread.run(Unknown Source)"]}}}}}],"metadata":{"total":1,"matching":1,"limit":-1,"offset":0}}
[curl http=200 total=0.039669s]
~~~

### GET list?limit=-1&filter=collection[eq]=production

~~~
GET /data/api/v1/resources/list/ignition/database-connection?limit=-1&filter=collection[eq]=production @ 2026-10-12T14:37:45Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:37:45 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 70

{"items":[],"metadata":{"total":1,"matching":0,"limit":-1,"offset":0}}
[curl http=200 total=0.037969s]
~~~

### GET names

~~~
GET /data/api/v1/resources/names/ignition/database-connection @ 2026-10-12T14:37:45Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:37:45 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 154

{"items":[{"name":"IGN_DEPLOY_RIG_DB_1791815701","enabled":true,"modes":["core","production"]}],"metadata":{"total":1,"matching":1,"limit":-1,"offset":0}}
[curl http=200 total=0.012992s]
~~~

### GET singleton security-properties (effective)

~~~
GET /data/api/v1/resources/singleton/ignition/security-properties @ 2026-10-12T14:37:45Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:37:45 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 3489

{
  "type": "ignition/security-properties",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "cac5b33f86d5ffca2b1f8ad1486e8da8ccde9bcd14d244bc2d4b970f61cb5de8",
  "config": {
    "systemAuthProfile": "default",
    "systemIdentityProvider": "default",
    "forceIdpAuth": false,
    "designerAuthStrategy": "CLASSIC",
    "designerAuthTokenInactivityTimeout": 10,
    "designerAuthTokenTimeToLive": 0,
    "designerRoleName": "Administrator",
    "createProjectRoleName": "",
    "accessPermissions": {
      "type": "AnyOf",
      "securityLevels": [
        {
          "name": "Authenticated",
          "children": [
            {
              "name": "Roles",
              "children": [
                {
                  "name": "Administrator",
                  "children": []
                }
              ]
            },
            {
              "name": "Deploy",
              "children": []
            }
          ]
        }
      ]
    },
    "readPermissions": {
      "type": "AnyOf",
      "securityLevels": [
        {
          "name": "Authenticated",
          "children": [
            {
              "name": "Roles",
              "children": [
                {
                  "name": "Administrator",
                  "children": []
                }
              ]
            },
            {
              "name": "Deploy",
              "children": []
            }
          ]
        }
      ]
    },
    "writePermissions": {
      "type": "AnyOf",
      "securityLevels": [
        {
          "name": "Authenticated",
          "children": [
            {
              "name": "Roles",
              "children": [
                {
                  "name": "Administrator",
                  "children": []
                }
              ]
            },
            {
              "name": "Deploy",
              "children": []
            }
          ]
        }
      ]
    },
    "designerPermissions": {
      "type": "AnyOf",
      "securityLevels": [
        {
          "name": "Authenticated",
          "children": [
            {
              "name": "Roles",
              "children": [
                {
                  "name": "Administrator",
                  "children": []
                }
              ]
            },
            {
              "name": "it",
              "children": [
                {
                  "name": "admin",
                  "children": []
                }
              ]
            }
          ]
        }
      ]
    },
    "createProjectPermissions": {
      "type": "AnyOf",
      "securityLevels": [
        {
          "name": "Authenticated",
          "children": [
            {
              "name": "Roles",
              "children": [
                {
                  "name": "Administrator",
                  "children": []
                }
              ]
            },
            {
              "name": "it",
              "children": [
                {
                  "name": "admin",
                  "children": []
                }
              ]
            }
          ]
        }
      ]
    },
    "userInactivityTimeout": 10,
    "allowUserAdmin": false,
    "allowDesignerSSO": false,
    "gatewayAuditProfile": "WIGNVM01_Audit"
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "enabled": true
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.024538s]
~~~

### GET singleton security-properties?collection=core

~~~
GET /data/api/v1/resources/singleton/ignition/security-properties?collection=core @ 2026-10-12T14:37:45Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:37:45 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 3489

{
  "type": "ignition/security-properties",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "cac5b33f86d5ffca2b1f8ad1486e8da8ccde9bcd14d244bc2d4b970f61cb5de8",
  "config": {
    "systemAuthProfile": "default",
    "systemIdentityProvider": "default",
    "forceIdpAuth": false,
    "designerAuthStrategy": "CLASSIC",
    "designerAuthTokenInactivityTimeout": 10,
    "designerAuthTokenTimeToLive": 0,
    "designerRoleName": "Administrator",
    "createProjectRoleName": "",
    "accessPermissions": {
      "type": "AnyOf",
      "securityLevels": [
        {
          "name": "Authenticated",
          "children": [
            {
              "name": "Roles",
              "children": [
                {
                  "name": "Administrator",
                  "children": []
                }
              ]
            },
            {
              "name": "Deploy",
              "children": []
            }
          ]
        }
      ]
    },
    "readPermissions": {
      "type": "AnyOf",
      "securityLevels": [
        {
          "name": "Authenticated",
          "children": [
            {
              "name": "Roles",
              "children": [
                {
                  "name": "Administrator",
                  "children": []
                }
              ]
            },
            {
              "name": "Deploy",
              "children": []
            }
          ]
        }
      ]
    },
    "writePermissions": {
      "type": "AnyOf",
      "securityLevels": [
        {
          "name": "Authenticated",
          "children": [
            {
              "name": "Roles",
              "children": [
                {
                  "name": "Administrator",
                  "children": []
                }
              ]
            },
            {
              "name": "Deploy",
              "children": []
            }
          ]
        }
      ]
    },
    "designerPermissions": {
      "type": "AnyOf",
      "securityLevels": [
        {
          "name": "Authenticated",
          "children": [
            {
              "name": "Roles",
              "children": [
                {
                  "name": "Administrator",
                  "children": []
                }
              ]
            },
            {
              "name": "it",
              "children": [
                {
                  "name": "admin",
                  "children": []
                }
              ]
            }
          ]
        }
      ]
    },
    "createProjectPermissions": {
      "type": "AnyOf",
      "securityLevels": [
        {
          "name": "Authenticated",
          "children": [
            {
              "name": "Roles",
              "children": [
                {
                  "name": "Administrator",
                  "children": []
                }
              ]
            },
            {
              "name": "it",
              "children": [
                {
                  "name": "admin",
                  "children": []
                }
              ]
            }
          ]
        }
      ]
    },
    "userInactivityTimeout": 10,
    "allowUserAdmin": false,
    "allowDesignerSSO": false,
    "gatewayAuditProfile": "WIGNVM01_Audit"
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "enabled": true
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.020058s]
~~~

### GET list?limit=-1&collection=core (undeclared param)

~~~
GET /data/api/v1/resources/list/ignition/database-connection?limit=-1&collection=core @ 2026-10-12T14:37:56Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:37:56 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 70

{"items":[],"metadata":{"total":1,"matching":0,"limit":-1,"offset":0}}
[curl http=200 total=0.036363s]
~~~

### GET list?limit=-1&filter=collection[eq]=core

~~~
GET /data/api/v1/resources/list/ignition/database-connection?limit=-1&filter=collection[eq]=core @ 2026-10-12T14:37:56Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:37:57 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 70

{"items":[],"metadata":{"total":1,"matching":0,"limit":-1,"offset":0}}
[curl http=200 total=0.020909s]
~~~

### GET list?limit=-1&filter=name[eq]=<c3a name> (control: a filter on another field)

~~~
GET /data/api/v1/resources/list/ignition/database-connection?limit=-1&filter=name[eq]=IGN_DEPLOY_RIG_DB_1791815701 @ 2026-10-12T14:37:57Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:37:57 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 70

{"items":[],"metadata":{"total":1,"matching":0,"limit":-1,"offset":0}}
[curl http=200 total=0.019058s]
~~~

### GET list?limit=-1&collection[eq]=production (object-style filter)

~~~
GET /data/api/v1/resources/list/ignition/database-connection?limit=-1&collection[eq]=production @ 2026-10-12T14:38:30Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:38:31 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 8552

{"items":[{"type":"ignition/database-connection","name":"IGN_DEPLOY_RIG_DB_1791815701","description":"c-list production override","enabled":true,"version":1,"collection":"production","collections":["core","production"],"signature":"c3ee2b70350a263381b5c1a4f92c3cdacb26ea949f255a48ead9a468aaeafcb9","config":{"driver":"PostgreSQL","translator":"POSTGRES","includeSchemaInTableName":false,"connectURL":"jdbc:postgresql://mode-host:5432/spike","username":"spike","connectionProps":"","connectionResetParams":"","defaultTransactionLevel":"DEFAULT","poolInitSize":0,"poolMaxActive":20,"poolMaxIdle":8,"poolMinIdle":0,"poolMaxWait":5000,"validationQuery":"SELECT 1","testOnBorrow":true,"testOnReturn":false,"testWhileIdle":false,"evictionRate":-1,"evictionTests":3,"evictionTime":1800000,"failoverMode":"STANDARD","slowQueryLogThreshold":60000,"validationSleepTime":10000},"data":["config.json"],"attributes":{"uuid":"ef39fc66-c7b4-441a-a907-b02df07b17aa","enabled":true},"metrics":{"active-connections":{"name":"databases.connection-IGN_DEPLOY_RIG_DB_1791815701.active-connections","metric":{"value":0.0,"numerator":0.0,"denominator":20.0,"type":"ratio-gauge","description":"Active Connections","units":""}},"active-queries":{"name":"databases.connection-IGN_DEPLOY_RIG_DB_1791815701.active-queries","metric":{"value":[],"type":"gauge","description":"Active Queries","units":"queries"}},"expensive-queries":{"name":"databases.connection-IGN_DEPLOY_RIG_DB_1791815701.expensive-queries","metric":{"value":[],"type":"gauge","description":"Expensive Queries","units":"queries"}},"queries":{"name":"databases.connection-IGN_DEPLOY_RIG_DB_1791815701.queries","metric":{"oneMinuteRate":0.0,"fiveMinuteRate":0.0,"fifteenMinuteRate":0.0,"count":0,"meanRate":0.0,"histogram":{"max":0.0,"min":0.0,"p50":0.0,"p75":0.0,"p95":0.0,"p98":0.0,"p99":0.0,"mean":0.0,"bucketSize":0.0,"buckets":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"type":"histogram","stdev":0.0},"type":"timer"}},"rows":{"name":"databases.connection-IGN_DEPLOY_RIG_DB_1791815701.rows","metric":{"oneMinuteRate":0.0,"fiveMinuteRate":0.0,"fifteenMinuteRate":0.0,"count":0,"meanRate":0.0,"type":"meter"}},"throughput":{"name":"databases.connection-IGN_DEPLOY_RIG_DB_1791815701.throughput","metric":{"value":-1.0,"type":"gauge","description":"","units":"queries/sec"}}},"healthchecks":{"status":{"name":"databases.connection-IGN_DEPLOY_RIG_DB_1791815701.status","result":{"healthy":false,"time":1791815911071,"message":"Faulted","error":{"message":"Cannot create PoolableConnectionFactory (The connection attempt failed.)","stacktrace":["java.sql.SQLException: Cannot create PoolableConnectionFactory (The connection attempt failed.)","\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:645)","\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)","\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)","\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)","\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)","\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)","\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)","\tat java.base/java.lang.Thread.run(Unknown Source)","Caused by: org.postgresql.util.PSQLException: The connection attempt failed.","\tat org.postgresql.core.v3.ConnectionFactoryImpl.openConnectionImpl(ConnectionFactoryImpl.java:358)","\tat org.postgresql.core.ConnectionFactory.openConnection(ConnectionFactory.java:54)","\tat org.postgresql.jdbc.PgConnection.<init>(PgConnection.java:273)","\tat org.postgresql.Driver.makeConnection(Driver.java:446)","\tat org.postgresql.Driver.connect(Driver.java:298)","\tat org.apache.commons.dbcp2.DriverConnectionFactory.createConnection(DriverConnectionFactory.java:52)","\tat org.apache.commons.dbcp2.PoolableConnectionFactory.makeObject(PoolableConnectionFactory.java:481)","\tat org.apache.commons.dbcp2.BasicDataSource.validateConnectionFactory(BasicDataSource.java:114)","\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:641)","\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)","\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)","\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)","\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)","\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)","\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)","\tat java.base/java.lang.Thread.run(Unknown Source)","Caused by: java.net.UnknownHostException: mode-host","\tat java.base/sun.nio.ch.NioSocketImpl.connect(Unknown Source)","\tat java.base/java.net.SocksSocketImpl.connect(Unknown Source)","\tat java.base/java.net.Socket.connect(Unknown Source)","\tat org.postgresql.core.PGStream.createSocket(PGStream.java:243)","\tat org.postgresql.core.PGStream.<init>(PGStream.java:98)","\tat org.postgresql.core.v3.ConnectionFactoryImpl.tryConnect(ConnectionFactoryImpl.java:136)","\tat org.postgresql.core.v3.ConnectionFactoryImpl.openConnectionImpl(ConnectionFactoryImpl.java:262)","\tat org.postgresql.core.ConnectionFactory.openConnection(ConnectionFactory.java:54)","\tat org.postgresql.jdbc.PgConnection.<init>(PgConnection.java:273)","\tat org.postgresql.Driver.makeConnection(Driver.java:446)","\tat org.postgresql.Driver.connect(Driver.java:298)","\tat org.apache.commons.dbcp2.DriverConnectionFactory.createConnection(DriverConnectionFactory.java:52)","\tat org.apache.commons.dbcp2.PoolableConnectionFactory.makeObject(PoolableConnectionFactory.java:481)","\tat org.apache.commons.dbcp2.BasicDataSource.validateConnectionFactory(BasicDataSource.java:114)","\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:641)","\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)","\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)","\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)","\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)","\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)","\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)","\tat java.base/java.lang.Thread.run(Unknown Source)"]}}}}}],"metadata":{"total":1,"matching":1,"limit":-1,"offset":0}}
[curl http=200 total=0.114694s]
~~~

### GET list?limit=-1&collection[eq]=core (the core copy exists but is shadowed)

~~~
GET /data/api/v1/resources/list/ignition/database-connection?limit=-1&collection[eq]=core @ 2026-10-12T14:38:31Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:38:31 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 70

{"items":[],"metadata":{"total":1,"matching":0,"limit":-1,"offset":0}}
[curl http=200 total=0.021452s]
~~~

### GET list?limit=-1&name[eq]=<c3a name> (control)

~~~
GET /data/api/v1/resources/list/ignition/database-connection?limit=-1&name[eq]=IGN_DEPLOY_RIG_DB_1791815701 @ 2026-10-12T14:38:31Z
HTTP/1.1 200 OK
Date: Mon, 12 Oct 2026 14:38:31 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 8552

{"items":[{"type":"ignition/database-connection","name":"IGN_DEPLOY_RIG_DB_1791815701","description":"c-list production override","enabled":true,"version":1,"collection":"production","collections":["core","production"],"signature":"c3ee2b70350a263381b5c1a4f92c3cdacb26ea949f255a48ead9a468aaeafcb9","config":{"driver":"PostgreSQL","translator":"POSTGRES","includeSchemaInTableName":false,"connectURL":"jdbc:postgresql://mode-host:5432/spike","username":"spike","connectionProps":"","connectionResetParams":"","defaultTransactionLevel":"DEFAULT","poolInitSize":0,"poolMaxActive":20,"poolMaxIdle":8,"poolMinIdle":0,"poolMaxWait":5000,"validationQuery":"SELECT 1","testOnBorrow":true,"testOnReturn":false,"testWhileIdle":false,"evictionRate":-1,"evictionTests":3,"evictionTime":1800000,"failoverMode":"STANDARD","slowQueryLogThreshold":60000,"validationSleepTime":10000},"data":["config.json"],"attributes":{"uuid":"ef39fc66-c7b4-441a-a907-b02df07b17aa","enabled":true},"metrics":{"active-connections":{"name":"databases.connection-IGN_DEPLOY_RIG_DB_1791815701.active-connections","metric":{"value":0.0,"numerator":0.0,"denominator":20.0,"type":"ratio-gauge","description":"Active Connections","units":""}},"active-queries":{"name":"databases.connection-IGN_DEPLOY_RIG_DB_1791815701.active-queries","metric":{"value":[],"type":"gauge","description":"Active Queries","units":"queries"}},"expensive-queries":{"name":"databases.connection-IGN_DEPLOY_RIG_DB_1791815701.expensive-queries","metric":{"value":[],"type":"gauge","description":"Expensive Queries","units":"queries"}},"queries":{"name":"databases.connection-IGN_DEPLOY_RIG_DB_1791815701.queries","metric":{"oneMinuteRate":0.0,"fiveMinuteRate":0.0,"fifteenMinuteRate":0.0,"count":0,"meanRate":0.0,"histogram":{"max":0.0,"min":0.0,"p50":0.0,"p75":0.0,"p95":0.0,"p98":0.0,"p99":0.0,"mean":0.0,"bucketSize":0.0,"buckets":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"type":"histogram","stdev":0.0},"type":"timer"}},"rows":{"name":"databases.connection-IGN_DEPLOY_RIG_DB_1791815701.rows","metric":{"oneMinuteRate":0.0,"fiveMinuteRate":0.0,"fifteenMinuteRate":0.0,"count":0,"meanRate":0.0,"type":"meter"}},"throughput":{"name":"databases.connection-IGN_DEPLOY_RIG_DB_1791815701.throughput","metric":{"value":-1.0,"type":"gauge","description":"","units":"queries/sec"}}},"healthchecks":{"status":{"name":"databases.connection-IGN_DEPLOY_RIG_DB_1791815701.status","result":{"healthy":false,"time":1791815911230,"message":"Faulted","error":{"message":"Cannot create PoolableConnectionFactory (The connection attempt failed.)","stacktrace":["java.sql.SQLException: Cannot create PoolableConnectionFactory (The connection attempt failed.)","\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:645)","\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)","\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)","\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)","\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)","\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)","\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)","\tat java.base/java.lang.Thread.run(Unknown Source)","Caused by: org.postgresql.util.PSQLException: The connection attempt failed.","\tat org.postgresql.core.v3.ConnectionFactoryImpl.openConnectionImpl(ConnectionFactoryImpl.java:358)","\tat org.postgresql.core.ConnectionFactory.openConnection(ConnectionFactory.java:54)","\tat org.postgresql.jdbc.PgConnection.<init>(PgConnection.java:273)","\tat org.postgresql.Driver.makeConnection(Driver.java:446)","\tat org.postgresql.Driver.connect(Driver.java:298)","\tat org.apache.commons.dbcp2.DriverConnectionFactory.createConnection(DriverConnectionFactory.java:52)","\tat org.apache.commons.dbcp2.PoolableConnectionFactory.makeObject(PoolableConnectionFactory.java:481)","\tat org.apache.commons.dbcp2.BasicDataSource.validateConnectionFactory(BasicDataSource.java:114)","\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:641)","\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)","\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)","\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)","\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)","\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)","\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)","\tat java.base/java.lang.Thread.run(Unknown Source)","Caused by: java.net.UnknownHostException: mode-host","\tat java.base/sun.nio.ch.NioSocketImpl.connect(Unknown Source)","\tat java.base/java.net.SocksSocketImpl.connect(Unknown Source)","\tat java.base/java.net.Socket.connect(Unknown Source)","\tat org.postgresql.core.PGStream.createSocket(PGStream.java:243)","\tat org.postgresql.core.PGStream.<init>(PGStream.java:98)","\tat org.postgresql.core.v3.ConnectionFactoryImpl.tryConnect(ConnectionFactoryImpl.java:136)","\tat org.postgresql.core.v3.ConnectionFactoryImpl.openConnectionImpl(ConnectionFactoryImpl.java:262)","\tat org.postgresql.core.ConnectionFactory.openConnection(ConnectionFactory.java:54)","\tat org.postgresql.jdbc.PgConnection.<init>(PgConnection.java:273)","\tat org.postgresql.Driver.makeConnection(Driver.java:446)","\tat org.postgresql.Driver.connect(Driver.java:298)","\tat org.apache.commons.dbcp2.DriverConnectionFactory.createConnection(DriverConnectionFactory.java:52)","\tat org.apache.commons.dbcp2.PoolableConnectionFactory.makeObject(PoolableConnectionFactory.java:481)","\tat org.apache.commons.dbcp2.BasicDataSource.validateConnectionFactory(BasicDataSource.java:114)","\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:641)","\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)","\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)","\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)","\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)","\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)","\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)","\tat java.base/java.lang.Thread.run(Unknown Source)"]}}}}}],"metadata":{"total":1,"matching":1,"limit":-1,"offset":0}}
[curl http=200 total=0.016502s]
~~~

## Carried evidence

S3 redundancy pair (Master/Backup, Active/Cold, pre- and post-failover): see block C-S3 above (not re-quoted here).

S4 lock held, then 204. Source: `/Users/pmannion/data/worktrees/whk-environment-orchestration/gateway-config-spec/spikes/gateway-modes/results/S4.md` lines 33-47 (gateway-modes spike, 8.3.6, 2026-10-09).

~~~
## scan-lock info during hold

~~~
{
  "timestamp": 149795344,
  "remaining": 4,
  "actor": "api-token:spike:192.168.97.1"
} [http 200]
~~~

## scan-lock info after expiry

~~~
 [http 204]
~~~
~~~

v1.1 8.3.6 signature-mismatch PUT: HTTP 500 plus JSON problem body. Source: `/Users/pmannion/whiskeyhouse/ignition-cli/.planning/milestones/v1.1-phases/10-eam-write-operations/10-LIVE-CAPTURES.md` lines 222-233.

~~~
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
~~~

v1.1 PUT of an unknown name: HTTP 404, empty body. Source: `/Users/pmannion/whiskeyhouse/ignition-cli/.planning/milestones/v1.1-phases/10-eam-write-operations/10-LIVE-CAPTURES.md` lines 246-248 and 254.

~~~
## 5. Rename via PUT (changed `name`, ORIGINAL signature) — HTTP **404, empty body** — both rigs (10:01:30/10:01:31Z)

`PUT /data/api/v1/resources/com.inductiveautomation.eam/eam-tasks` with single-element array `{"name":"ign-p10-scratch-renamed", …,"signature":<original>}` → 404 empty. Post-check `names` list (both rigs):
**No rename, NO new resource, no error body** — the modify route resolves the resource BY the body's name and finds nothing. Rename via PUT is NOT supported; a CLI rename must be create-new + delete-old (composition decision for 10-03/10-04). Bonus shape: the `names` endpoint items carry `modes:["core"]` (key is `modes`, not `collections`).
~~~

## Decisions

Every Decision cites the block headings it rests on. Where a capture contradicts 20-RESEARCH, the text says so and the dependent plan (20-03, 20-04, 20-05) follows the capture. "Overturns" below means overturns the research or the OpenAPI document, not a prior Decision.

### D1 — A competing acquire is an immediate HTTP 409 with an HTML body; 408 never appeared

Rests on `## C1e` (sub-block "cap-b competing acquire 2/5") and `## C1d`.

- Status 409 Conflict, `Content-Type: text/html;charset=iso-8859-1`, Jetty's stock error page, NOT JSON.
- Elapsed: `total=0.015868s` with `acquireTimeout` 2. The gateway did not wait; 409 is immediate. `acquireTimeout` was not honoured as a wait in any capture. The 408 route that the OpenAPI declares ("Unable to acquire lock within timeout") was never observed. 20-03 still maps both 409 and 408 on `/data/api/v1/scan-lock/` to `scan_lock_held`, but only 409 is capture-proven.
- The holder appears in the body twice, in the `<h2>` and in the `MESSAGE:` table cell: `Lock already held by api-token:cap-a:192.168.97.1`. Format: `api-token:<token-name>:<client-ip>` (the IP is the Docker bridge gateway, not a gateway-side value to rely on). Parsing rule for 20-03: take the text after `Lock already held by ` up to the next `<`. Cap it (T-20-13). If it is absent, enrich from `scan_lock_info` (the GET in the same block also names the actor).
- Overturns the OpenAPI/research expectation that the body is structured; the actor must be scraped from HTML. Confirms memory `ignition-83-platform-scan-api` ("Lock already held by <actor>").

### D2 — Re-acquiring with the same token is also 409; it does not extend the hold

Rests on `## C1d` (sub-blocks "POST re-acquire by the holder 3/20" and "GET after re-acquire").

- cap-a held the lock (`remaining` 57), then POSTed again with `{"acquireTimeout":3,"holdTimeout":20}`. Answer: 409, `Lock already held by api-token:cap-a:192.168.97.1`, `total=0.007785s`.
- The lock is not re-entrant and not renewable: the follow-up GET still shows `timestamp` 380308558 and `remaining` 57 (the original hold, still counting down).
- Consequence for 20-03/Phase 23: a holder that wants a longer hold cannot extend it; it must let it lapse or finish with a scan. The holder's own 409 and a foreign 409 are the same wire outcome; only the actor string tells them apart.

### D3 — Stale, omitted and empty signatures: stale is 500, omitted and empty are an identical 422

Rests on `## C3a` (sub-blocks "PUT with a stale (64-hex wrong) signature", "PUT with the signature key omitted", "PUT with an empty-string signature", "PUT of an unknown name", and the four "read-back after the refusal" blocks).

| Case | Status | Content-Type | Distinguishing marker |
| --- | --- | --- | --- |
| stale (64 hex, wrong) | 500 | `application/json` | body `problem.message` contains `signature mismatch` (full text `MODIFY illegal: signature mismatch for 'ResourceId{resourcePath=ignition/database-connection/<name>, collectionName=core}'`); `success:false`, `changes:[]` |
| `signature` key omitted | 422 | `application/json` | `fieldMessages[0].fieldName == "signature"` and message `Missing 'signature' parameter`; `Content-Length: 168` |
| `signature: ""` | 422 | `application/json` | byte-identical to the omitted case (same 168 bytes, same message) |
| unknown name, current signature | 404 | none | empty body, `Content-Length: 0` |

- The omitted and empty cases are NOT wire-distinguishable from each other. Both are distinguishable from stale (status 500 vs 422) and from every other refusal (the `fieldName` of `signature`). `SignatureReason::Missing` therefore covers both; the client's pre-wire refusal of an empty signature is still the first line of defence.
- Resource unchanged after each refusal: every read-back block reports the same signature (`8e01629a...3f14`), `poolMaxActive=9` and `description=c3a update`. The refusals are clean, with zero partial effect.
- The 8.3.6 message does NOT leak the live signature (it names the resource id only), unlike 8.3.3 in v1.1. 20-04 must still not echo `problem.message` or `stacktrace` (the stacktrace alone is about 6 KB).
- Overturns the research assumption that "missing" might be a 400 or 422 with a body we cannot tell apart: it is a 422 with a precise `fieldName`. Confirms the v1.1 finding that stale is a 500 (so it cannot live in `classify.rs`).

### D4 — Completion rule: a no-op scan advances `lastScanTimestamp`; a scan POST is a snapshot, and ANY actor's scan also releases the lock

Rests on `## C1i`, `## C1f` and `## C1j`.

- A scan on an idle gateway with no changes advances the timestamp: before 1791815654089, after 1791815676996, `scanActive` false after about 1 s (`poll GET scan/config`: `t+0s` active, `t+1s` complete, 2 polls). A8 holds.
- The POST's own 200 body is a snapshot taken at trigger time: `scanActive:true` with the OLD `lastScanTimestamp` (1791815654089). Never read it as completion. Same in `## C1f` ("POST scan/config", then "GET scan/config" still `scanActive:true`).
- A second POST immediately after the first (while the first is active): 200 with the same snapshot (`scanActive:true`, `lastScanTimestamp` 1791815654089). No error, no queue visible. The single poll that followed saw exactly one completion. Pitfall 3 stands: a foreign scan's completion satisfies `scanActive == false && ts > before` if `before` was read during it, so `scan_and_wait` must wait out an active scan before reading `before`.
- cap-b POSTed a scan while cap-a held the lock: 200, and the scan ran (`scanActive:true`). The lock did not block cap-b, and the GET that followed answered 204: the scan released cap-a's lock. A scan by a NON-holder releases the lock.
- Phase 23 consequence: the lock does not stop another actor's scan, so a held lock is a courtesy signal, not mutual exclusion for scans. Only the holder's own acquire/scan sequence is guarded against other lock acquisitions.

### D5 — `deploymentMode` is `""` with no mode and `"production"` with one; never null or absent on 8.3.6

Rests on `## C-null (no mode)` and `## C-null (mode production)`.

- No mode: raw `"deploymentMode":""`; the jq line reports `{"has":true,"v":"","t":"string"}`.
- Mode `production` (gateway recreated with `-Dignition.config.mode=production`): raw `"deploymentMode":"production"`, `{"has":true,"v":"production","t":"string"}`. The recreate took effect; there is no drift to report.
- A3 resolved as `""` (spec wording). `null` and absent were not observed on 8.3.6. `Option<String>` + `#[serde(default)]` + `filter(|s| !s.is_empty())` stays correct and covers other versions. `GatewayInfo` stays frozen.

### D6 — Mode create answers 200 with a JSON `ModeRecord`; a duplicate is 409 and an illegal name is 400, both HTML

Rests on `## C-mode`.

- `POST /data/api/v1/mode` `{"name":"production","title":"Production","description":"ign-deploy-rig"}`: 200, `application/json`, pretty-printed body `{"name":"production","title":"Production","description":"ign-deploy-rig","resourceCount":0}`.
- Duplicate: 409, `text/html`, `HTTP ERROR 409 Mode already exists`.
- Illegal name `a b/`: 400, `text/html`, `HTTP ERROR 400 Invalid mode name`.
- `GET /mode` envelope: `{"items":[{"name","title","description","resourceCount"}],"metadata":{"total","matching","limit","offset"}}`, empty before (`"items":[]`) and one item after.
- 20-05: `mode_create` is declared EmptyOk and confirmed by a follow-up `modes` read; the 200 body is a `ModeRecord` and may be ignored. 409 and 400 need a typed outcome in 20-05, not a body echo.

### D7 — `find` honours `?collection=`; `list` is the effective view only and filters by `field[op]=value` as its own query parameter

Rests on `## C-list` (all sub-blocks, including the two extra probe groups).

- Setup: the C3a resource exists in `core`; C-list added a `production` copy (`POST` answered 200, `collection:"production"`). Mode `production` is the active mode.
- `find` with no collection: the effective view, `collection:"production"`, signature `c3ee2b70...`. `find?collection=core`: `collection:"core"`, signature `8e01629a...`. `find?collection=production`: the production copy. So `find` honours the collection and the two copies carry different signatures; an update must use the signature of the copy it targets.
- `list?limit=-1` returns ONE item (the effective one): `collection:"production"`, `collections:["core","production"]`, and it carries `signature` (so list items do carry signatures).
- `list` declares no `collection` parameter. `list?limit=-1&collection=core` (undeclared) returned `matching:0`.
- A4 FALSIFIED as written: `filter=collection[eq]=production` returned `matching:0` even though the effective item is in production, and so did the control `filter=name[eq]=<exact name>`. The OpenAPI declares `filter` as a form-style exploded object whose keys are `field[op]` (`"propertyNames":{"pattern":"^[a-zA-Z0-9_]+\\[(eq|ne|cn|sw|ew|gt|gte|lt|lte|rgx)\\]$"}`), so the filter is NOT sent as `filter=...`; each `field[op]=value` is its own query parameter. The object-style probes confirm it: `collection[eq]=production` and the `name[eq]` control matched the item; `collection[eq]=core` matched nothing although a core copy exists, because the filter sees only the effective item.
- `names` returns `{"name","enabled","modes":["core","production"]}`: the way to enumerate which collections hold a resource.
- 20-04: list-by-collection is NOT available from `list`. `resource_list` takes no collection argument (effective view; free-form filters go out as `field[op]=value` parameters, not `filter=`). To read a named collection, call `names` (its `modes`) and then `find?collection=` per name. Overturns the research's `filter=collection[eq]=` recommendation.

### D8 — There is no scan-lock release route; the live 8.3.6 OpenAPI has 584 paths

Rests on `## C1g` and `## C1h`.

- `DELETE /data/api/v1/scan-lock/config`: 404, empty body (`Content-Length: 0`). `POST /data/api/v1/scan-lock/config/release`: 404, `No route match for path: /v1/scan-lock/config/release`. After both probes the lock was still held (`remaining` 17).
- `GET /openapi.json` (token required, bare path): 200, OpenAPI 3.1.0, 584 paths, 12,637,301 bytes. Paths matching `scan|lock`: `/data/api/v1/scan-lock/{config,projects}` and `/data/api/v1/scan/{config,projects}`, GET and POST only, plus the unrelated `/data/api/v1/diagnostics/threads/deadlocks`. Nothing matches `release|unlock`.
- The 584 count differs from the 737 (spec worktree document) and 617 (memory note for the prod gateway) in 20-RESEARCH; this is the live 8.3.6 standard-edition rig, and the count varies with installed modules.
- The `scan_lock_held` hint must keep saying "no release route: wait for the holder (holdTimeout) or let a scan release it" (D4, D12).

### D9 — Lock info bytes: 200 pretty-printed JSON while held, 204 with no body and no Content-Type when free; acquire 200 returns the same JSON

Rests on `## C1a`, `## C1b`, `## C1c`, `## C1f`.

- Idle `GET /data/api/v1/scan-lock/config`: `HTTP/1.1 204 No Content`, only a `Date` (plus the usual security headers), no `Content-Type`, no `Content-Length`, empty body.
- Held GET: 200, `Content-Type: application/json`, `Content-Length: 90`, body `{ "timestamp": 380308558, "remaining": 59, "actor": "api-token:cap-a:192.168.97.1" }` (pretty-printed over four lines). Two reads 2 s apart: `remaining` 59 then 57, `timestamp` unchanged.
- `timestamp` is an opaque value (380308558, not epoch-ms), unchanged across reads of one hold and different for each new hold (380308558, 380369593, 380390544); never compare it to a clock. `remaining` is integer seconds; right after an acquire it reads `holdTimeout - 1` (59 for 60, 19 for 20, 2 for 3).
- Acquire 200 body: the same JSON as the info read (`timestamp`, `remaining`, `actor`), `Content-Type: application/json`. Declared shape stays EmptyOk (see the Endpoint shapes table).
- POST with NO body (and no `Content-Type`): 500, `text/html`, `HTTP ERROR 500 java.lang.NullPointerException: Cannot read field "acquireTimeout" because "lockOptions" is null`. The OpenAPI declares 400 and the memory note says 400; the live gateway answers 500. The client always sends the body, so this is a limit, not a path.
- After the hold expired (`holdTimeout` 3, sleep 5): `204 No Content`.

### D10 — Create needs only `name` (+ `config`); the duplicate POST is a 409 JSON body; PUT of an unknown name is a bare 404

Rests on `## C3a` (sub-blocks "POST create", "POST the same name again (duplicate)", "PUT of an unknown name").

- `POST /data/api/v1/resources/ignition/database-connection` with `[{"name":"IGN_DEPLOY_RIG_DB_<epoch>","config":{...}}]`, no `collection`, no `attributes`: 200, `{"success":true,"changes":[{"name","type":"ignition/database-connection","collection":"core","newSignature":"..."}],"problem":null}`. A9 holds: no `attributes.uuid` is needed.
- The same POST again: 409 Conflict, `application/json`, `{"success":false,"changes":[],"problem":{"message":"CREATE conflict: 'ResourceId{resourcePath=ignition/database-connection/<name>, collectionName=core}' already exists","stacktrace":[...]}}`. A JSON `problem` body, not HTML, and a 6 KB stacktrace: cap or drop it.
- PUT of an unknown name with a valid current signature: 404, empty body. Create cannot be done by PUT.
- A good PUT answers 200 `{"success":true,"changes":[{...,"newSignature":"..."}],"problem":null}` and the read-back shows the change.
- Side effect to know about: creating a database connection also made the gateway create `core/ignition/store-and-forward-engine/<same name>/` (see D11 listing). Resources of that type are not isolated to one directory.

### D11 — The bind-mounted `resources/` directory is viable (A7 holds)

Rests on the rig boot record (no curl block applies; the evidence is the host directory and gateway state), with the listing taken 2026-10-12T14:39:51Z after all captures:

~~~
$ ls -1 $IGN_DEPLOY_RIG_RESOURCES
core
external
local
migration-log-202610121429.md
production
$ ls -1 $IGN_DEPLOY_RIG_RESOURCES/core | wc -l
6
$ find $IGN_DEPLOY_RIG_RESOURCES -path "*IGN_DEPLOY_RIG_DB_*" -name config.json
./production/ignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701/config.json
./core/ignition/database-connection/IGN_DEPLOY_RIG_DB_1791815701/config.json
./core/ignition/store-and-forward-engine/IGN_DEPLOY_RIG_DB_1791815701/config.json
$ ls -1 $IGN_DEPLOY_RIG_RESOURCES/external/ignition
api-token
$ docker inspect (Mounts, Source path shortened)
volume /var/lib/docker/volumes/ign-deploy-rig_gw-data/_data -> /usr/local/bin/ignition/data
bind $HOME/.cache/ign-deploy-rig/resources -> /usr/local/bin/ignition/data/config/resources
~~~

- First boot with the bind mount over an empty host directory: StatusPing RUNNING 92 s after the first poll (the poll began right after `up -d`), and `core/` populated by the gateway (the A7 falsifier: non-empty `core/` plus RUNNING).
- Token mint (docker cp + chown into the bind, then restart): RUNNING again 53 s after a 20 s settle wait; `GET /data/api/v1/gateway-info` answered 200 with cap-a.
- Recreate with `-Dignition.config.mode=production` on the same bind: RUNNING 98 s after a 10 s settle wait. The gateway wrote the `production/` collection directory and the resource writes from the API (`core/` and `production/` copies) appear on the host, so the host sees what the gateway writes.
- `compose.bind.yml` is the shipped variant; plain `compose.yml` also works for 20-06, with `docker cp` for file writes. The host directory is outside the repository (`$IGN_DEPLOY_RIG_STATE/resources`).

### D12 — A holder's own scan releases the lock

Rests on `## C1j` (sub-blocks "acquire 10/20", "holder POST scan/config", "GET lock after the holder scan"), reinforced by `## C1i`.

- cap-a acquired (`remaining` 19), POSTed `/scan/config` (200, `scanActive:true`), and the next GET of the lock answered 204, while the scan was still active. The lock is released at scan START, not at completion.
- Same result for a non-holder's scan (D4, `## C1i`). Consequences for Phase 23: after the scan POST the lock is gone, so `scan_lock_info` answering `NotHeld` is the NORMAL state after a successful scan trigger (the verdict is deliberately named `NotHeld`, not `Expired`), and the holder cannot rely on the lock to guard writes made after its own scan trigger.

## Endpoint shapes

One declared shape per `DeployApi` endpoint id. Shapes come from `Json`, `EmptyOk`, `JsonOrNoContent` (`NoContent` is unused: no endpoint answers exactly 204, the lock info answers 200 or 204). No default was changed: no block contradicts a default, but three rows record that the live 2xx body is JSON while the declared shape ignores it, and why.

| id | method | path | declared shape | capture block |
| --- | --- | --- | --- | --- |
| resource_names | GET | `/data/api/v1/resources/names/{module}/{type}` | Json | C-list (GET names), C-list extra probes |
| resource_list | GET | `/data/api/v1/resources/list/{module}/{type}` | Json | C-list (GET list?limit=-1, object-style filter probes), D7 |
| resource_find | GET | `/data/api/v1/resources/find/{module}/{type}/{name}` | Json | C3a (GET find), C-list (find with and without collection) |
| resource_singleton | GET | `/data/api/v1/resources/singleton/{module}/{type}` | Json | C-list (singleton security-properties, effective and collection=core) |
| resource_create | POST | `/data/api/v1/resources/{module}/{type}` | Json | C3a (POST create, duplicate), D10 |
| resource_update | PUT | `/data/api/v1/resources/{module}/{type}` | Json | C3a (PUT right signature; stale, omitted, empty, unknown refusals), D3 |
| scan_lock_acquire | POST | `/data/api/v1/scan-lock/{kind}` | EmptyOk | C1b, C1d, C1e, D1, D2, D9 |
| scan_lock_info | GET | `/data/api/v1/scan-lock/{kind}` | JsonOrNoContent | C1a, C1c, C1f, D9 |
| scan_status | GET | `/data/api/v1/scan/{kind}` | Json | C1i, C1f, D4 |
| scan_request | POST | `/data/api/v1/scan/{kind}` | EmptyOk | C1i, C1j, D4, D12 |
| modes | GET | `/data/api/v1/mode` | Json | C-mode (GET /mode before and after) |
| mode_create | POST | `/data/api/v1/mode` | EmptyOk | C-mode (POST /mode, duplicate, illegal name), D6 |
| deployment_mode | GET | `/data/api/v1/gateway-info` | Json | C-null (no mode), C-null (mode production), D5 |

Notes on the three EmptyOk rows whose live body is JSON:

- `scan_lock_acquire`: the 200 body is `{timestamp, remaining, actor}` (C1b, C1e). EmptyOk stays because success is decided by the follow-up `scan_lock_info` read and the acquire body adds nothing the info read does not give; ignoring it means a body drift cannot fail an acquire that succeeded. A 409 (immediate, HTML) is raised by the route-scoped classify arm before the shape check.
- `scan_request`: the 200 body is a trigger-time `ScanStatus` snapshot (`scanActive:true`, old timestamp). It is not completion (D4), so it is ignored; completion comes only from `scan_status` and `scan_complete`.
- `mode_create`: the 200 body is a `ModeRecord` (D6). Confirmed by a `modes` read; the body is ignored.

Rows `resource_create` and `resource_update` are `Json` only for their 2xx. Their refusals are not a shape: stale signature is a 500 JSON `problem`, omitted or empty signature is a 422 `fieldMessages`, a duplicate create is a 409 JSON `problem`, an unknown name on update is an empty 404 (D3, D10). A 2xx with `success:false` is still an error (research table).

## Assumptions

| # | Claim | Verdict | Deciding block |
| --- | --- | --- | --- |
| A1 | A second token's POST to a held lock answers 409 (or 408) with a body naming the actor | HOLDS, refined: 409 (never 408), immediate, HTML body naming the actor | C1e (D1) |
| A2 | A missing-signature PUT answers 422 or 400 with a distinguishable body | HOLDS: 422 with `fieldName: "signature"`; the empty-string case is byte-identical to the omitted case | C3a (D3) |
| A3 | `deploymentMode` is `""`, `null` or absent on a no-mode gateway | HOLDS for `""`: the key is present, a string, empty; `null` and absent were not observed on 8.3.6 | C-null (no mode) (D5) |
| A4 | `list` accepts `filter=collection[eq]=<c>` | FALSIFIED: that syntax matches nothing; the object-style `collection[eq]=<c>` works but sees the effective view only, so a shadowed collection cannot be listed | C-list and its extra probe groups (D7) |
| A5 | New slug names `scan_lock_held` and `resource_signature_rejected` | HOLDS (naming only, no capture contradicts it; the 409 of C1e and the 500 and 422 of C3a map onto them) | C1e, C3a |
| A6 | `deployment_mode()` returns `Option<String>` rather than a `""` sentinel | HOLDS: the wire value is `""`, normalised to `None` | C-null (no mode) |
| A7 | A bind mount of `./resources` over the data volume boots a commissioned gateway and the gateway populates `core/` into it | HOLDS | D11 listing (no curl block applies) |
| A8 | A no-op config scan still advances `lastScanTimestamp` | HOLDS: 1791815654089 to 1791815676996 on an idle gateway | C1i (D4) |
| A9 | `POST /resources` create works without `attributes.uuid` | HOLDS: `[{name, config}]` answered 200 | C3a (POST create) (D10) |

## Drift and limits

- Covers 8.3.6 only (`8.3.6 (b2026042713)`, standard edition, trial license), a single gateway, run under amd64 emulation on an arm64 host. Timings (boot 53 to 98 s, scan about 1 s) are not portable.
- A single gateway, so redundancy comes from the carried block C-S3, not from this rig. `redundancyRole` on this rig reads `Independent`.
- The `projects` scan-lock and scan kinds (`/scan-lock/projects`, `/scan/projects`) are wiremock-pinned in 20-03, not live-captured: C1 exercised `config` only. The live OpenAPI declares both with GET and POST (C1h).
- Overturned expectations, in one place: the competing acquire is an immediate 409 with an HTML body and never a 408 (D1); same-token re-acquire is 409 and does not extend the hold (D2); omitted and empty signatures are the same 422 (D3); a scan by any actor releases the lock at trigger time (D4, D12); `filter=` is object-style and `list` sees only the effective view (D7); a body-less lock POST is a 500 NPE, not the declared 400 (D9); the live OpenAPI has 584 paths (D8).
- Timestamps in the sub-block headers (`@ <UTC>`) of the C1 blocks were taken after the response arrived (the script was fixed to stamp before the request for the later groups, C-null onward). The response `Date` header is authoritative for server time.
- The C-list group was run in three steps: `clist` (the plan's matrix), `clist_extra` (probes with the research's `filter=` syntax, which matched nothing), and `clist_extra2` (object-style filters after reading the OpenAPI `filter` parameter schema). All three appear under the single `## C-list` heading.
- The C3a resource also produced a `store-and-forward-engine` directory with the same name (D11 listing): creating a database connection has a gateway-side side effect on a second resource type. Not exercised further.
- The same `IGN_DEPLOY_RIG_DB_<epoch>` name was used in `core` and then `production` (C-list), so the `production` copy carries the same name as the `core` copy; both exist on the rig until teardown.
- `curl -i` bodies were kept verbatim, including the roughly 6 KB `stacktrace` arrays of the 409 and 500 `problem` bodies; they are in the blocks above and add nothing beyond `message`.
- Signatures in these blocks are opaque per-write hashes of a disposable gateway that is destroyed after the captures (T-20-04). No API-token key appears anywhere in this file.

### Live gate 20-06 (2026-10-12)

- Image `inductiveautomation/ignition:8.3.6` (linux/amd64 under emulation), compose variant `compose.yml` alone (no bind mount; D11 allows it), project `ign-deploy-rig`, 127.0.0.1:18290. Boot RUNNING on the 13th 10 s poll; after the token mint and restart RUNNING again on the 5th poll. Run: `cargo test -p ignition-core --test live_deploy_api -- --ignored --nocapture`, 13.24 s, committed as `20-06-live-gate.log`.
- Per-section results: PASS guard (loopback URL, gateway name `ign-deploy-rig`); PASS A (lock held by cap-a, cap-b refused naming cap-a, Free after expiry); PASS B (`scan_and_wait` Complete, `lastScanTimestamp` advanced); PASS D (no active mode, `production` mode created and listed, still no active mode); PASS C (create, effective/core/production reads, update, stale refusal leaves the signature unchanged, list, names, duplicate refused); PASS E (live trace equals fake trace, 13 steps); PASS Z (cleanup scan).
- Live trace: `lock:acquired, lock:held-by-a, lock:refused-held-by-a, lock:free, scan:complete, create:ok, find:ok, update:ok, update:stale, create-duplicate:internal, mode:created, modes:listed, deployment-mode:none`. Fake trace: identical.
- Drift found: none: live trace equals fake trace. A collection-less PUT (the open question from 20-05) was accepted on the live gateway as `update:ok` when only the core copy existed in the scenario run's names; which copy it updates is still not shown by a capture.
- The rig was run with no active mode throughout; the rig and its volume were destroyed after the run.
