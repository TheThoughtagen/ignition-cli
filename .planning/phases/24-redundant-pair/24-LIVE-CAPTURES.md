# Phase 24 — Live Captures (8.3.6 redundant pair)

| Field | Value |
| --- | --- |
| Date (UTC) | 2026-10-19 |
| Image | inductiveautomation/ignition:8.3.6 (platform linux/amd64, emulated on an arm64 host) |
| Compose project | ign-pair-rig |
| Rig started (UTC) | 2026-10-19T20:13:59Z |
| Docker server | 29.8.2 |
| Docker Compose | 5.5.0 |
| Bind paths (outside repo) | $HOME/.cache/ign-pair-rig/data-a/config/resources and $HOME/.cache/ign-pair-rig/data-b/config/resources |
| Runner | setup-pair.sh and capture24.sh (crates/ignition-core/tests/fixtures/live-rig-pair) |

Rule: every captured block is preceded by the exact command and a UTC time taken by the driver just before it. A request line names the token variable it used and the base URL variable (`$BASE_A` is gw-a, the master; `$BASE_B` is gw-b, the backup), never a token value. Tokens live in a 0600 file outside the repo.

## Captures Owed

Written before any capture block (decision D-18: capture first). Each row names the question, the CONTEXT decision it settles (and the research assumption or pitfall) and the plan that reads the record.

| ID | Question | Settles | Read by |
| --- | --- | --- | --- |
| C-P1 | Does the backup's `lastSyncTimestamp` advance after a master config change made the way apply makes it (files into the bind-mounted `production` collection, scan-lock, config scan), and after a REST-made change? How soon does the changed resource read back on gw-b? | D-06 timestamp-advance clause (Assumption A1, Pitfall 1) | 24-02, 24-05 |
| C-P2 | After a sync, are the backup's on-disk files for a changed unit and an untouched unit byte-equal to the master's? | D-03 byte equality (Assumption A2, Pitfall 4) | 24-05 |
| C-P3 | What does `POST /data/api/v1/redundancy/gwaction/resync` sent to the master answer (status, headers, body), and what does the backup do next? | D-07 resync shape (Pitfall 9) | 24-03, 24-06 |
| C-P4 | What do `GET /redundancy` and `GET /redundancy/providers` answer on each node when settled, during a sync, after a link cut and after the reconnect? Which token answers on which node? | status field values (Pitfall 5, Assumptions A8, A10) | 24-05 |
| C-P5 | Does `toggleApproval/<id>/false` on the master cut an established link while both REST APIs stay up? How fast does `peerConnected` flip and recover? Does the two-network fallback differ? | D-17 link cut (Pitfall 2, Assumption A5) | 24-10 |
| C-P6 | With the ping settings applied, does a cut backup stay `Cold` or promote itself (`active_node` reads `Both`)? | D-11 versus D-17 (Pitfall 3, Assumption A3) | 24-07, 24-10 |
| C-P7 | Secret listings on both nodes: the internal provider, a file provider, an unknown provider (404), a 501 candidate, a token without permission. | D-13/D-14 listings (Pitfall 6, Assumption A4) | 24-03, 24-07, 24-09 |
| C-P8 | Is the sync-exhausted state (TEC-5105) reachable on the rig, how is it detected (providers, events, log lines), and does a resync alone clear it? | D-10 detection (C12, Assumptions A6, A7) | 24-03, 24-06 |

C-P1 to C-P6 are recorded by 24-01 (this plan). C-P7 and C-P8 are owed to 24-02.

## Pair rig (24-01)

| Field | Value |
| --- | --- |
| Compose project | `ign-pair-rig` (compose.yml in this fixture directory) |
| gw-a | gateway name `ign-pair-a`, hostname `gw-a`, redundancy role Master, REST `127.0.0.1:18300` |
| gw-b | gateway name `ign-pair-b`, hostname `gw-b`, redundancy role Backup, REST `127.0.0.1:18301` |
| Networks | `rig` (published REST ports) and `gan` (`internal: true`); gw-a alias `gw-a-gan`, GAN port 8060 with TLS, not published on the host |
| GAN trust | rejected certificates moved to `trusted/certs` on both nodes (spike recipe), incoming connection approved on gw-a with `PUT /data/api/v1/gateway-network-incoming/toggleApproval/<id>/true` |
| Deployment mode | production on both nodes (mode created on gw-a, seen on gw-b, then gw-b and gw-a recreated with `-Dignition.config.mode=production`, in that order) |
| Bind mounts | each node's `config/resources` from the host (`data-a`, `data-b`) so master and backup read with a `local:` transport (D-16) |
| Ping settings applied (`gatewayNetworkSetup`) | pingRate 1000 ms, pingTimeout 1000 ms, pingMaxMissed 1200 (at least 1200 s of missed pings; the backup should not treat the master as faulted inside the capture windows) |

### Redundancy config as applied (read back from each node)

Defaults found before the settings were applied (script output, `pair-before.log`):

~~~
gw-a redundancy/config before (defaults): {"role":"Independent","joinWaitTime":30000,"masterRecoveryMode":"Automatic","gatewayNetworkSetup":{"host":"","port":8060,"enableSsl":true,"pingRate":1000,"pingTimeout":300,"pingMaxMissed":10}}
gw-b redundancy/config before (defaults): {"role":"Independent","joinWaitTime":30000,"masterRecoveryMode":"Automatic","gatewayNetworkSetup":{"host":"","port":8060,"enableSsl":true,"pingRate":1000,"pingTimeout":300,"pingMaxMissed":10}}
~~~

#### GET redundancy/config on gw-a (after pairing)

`curl -sS -g [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy/config' | jq -c '{role, joinWaitTime, masterRecoveryMode, standbyActivityLevel, syncTimeoutSecs, gatewayNetworkSetup: (.gatewayNetworkSetup | {host, port, enableSsl, pingRate, pingTimeout, pingMaxMissed})}'` @ 2026-10-19T20:37:40Z

~~~
{"role":"Master","joinWaitTime":30000,"masterRecoveryMode":"Automatic","standbyActivityLevel":"Cold","syncTimeoutSecs":60,"gatewayNetworkSetup":{"host":"gw-a-gan","port":8060,"enableSsl":true,"pingRate":1000,"pingTimeout":1000,"pingMaxMissed":1200}}
~~~

#### GET redundancy/config on gw-b (after pairing)

`curl -sS -g [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy/config' | jq -c '{role, joinWaitTime, masterRecoveryMode, standbyActivityLevel, syncTimeoutSecs, gatewayNetworkSetup: (.gatewayNetworkSetup | {host, port, enableSsl, pingRate, pingTimeout, pingMaxMissed})}'` @ 2026-10-19T20:37:40Z

~~~
{"role":"Backup","joinWaitTime":30000,"masterRecoveryMode":"Automatic","standbyActivityLevel":"Cold","syncTimeoutSecs":60,"gatewayNetworkSetup":{"host":"gw-a-gan","port":8060,"enableSsl":true,"pingRate":1000,"pingTimeout":1000,"pingMaxMissed":1200}}
~~~

### Credential each setup write needed

Every redundancy-config write with the credential that answered, and every call that needed more than the node's own token (`credentials.log`, no secret in it):

~~~
2026-10-19T20:19:13Z PUT redundancy/config node=a credential=token status=200
2026-10-19T20:19:14Z PUT redundancy/config node=b credential=token status=200
~~~

### Seconds each setup step took

~~~
2026-10-19T20:16:13Z up 134 s
2026-10-19T20:18:18Z mint 120 s
2026-10-19T20:21:43Z pair 150 s
2026-10-19T20:36:57Z mode 886 s (script bug: its final settled wait false-timed-out at 600 s because the jq read of syncPending=false came back empty; the pair was settled by 20:32:10; mode_definition_sync_seconds=23, relink after gw-b recreate 33 s, after gw-a recreate 8 s)
~~~

### Observations while setting up (author's notes, not gateway bytes)

- Tokens: `PUT /redundancy/config` and every other setup call succeeded with the node's own token; no call needed the basic-auth fallback (the Pair rig credential log above shows only token writes).
- Defaults before the settings were applied: pingRate 1000, pingTimeout 300, pingMaxMissed 10, joinWaitTime 30000, masterRecoveryMode Automatic. Applied: pingTimeout 1000, pingMaxMissed 1200.
- `setup-pair.sh pair` printed `linked after 50 s`, but a `status` taken immediately afterwards (about 20:22Z) read `peerConnected` false and `projectState` Unknown on both nodes (`lastSyncTimestamp` 1792441308127 on gw-a, 1792441309086 on gw-b); the pair read `peerConnected` true, `syncPending` false, `projectState` Good on both nodes by 20:32Z (`lastSyncTimestamp` 1792441595934 and 1792441595954). The first link is not yet a settled pair.
- On the settled pair gw-b's `gateway-info` `name` reads `ign-pair-a` (the master's name), not `ign-pair-b` which it was started with; `peerId` reads `ign-pair-a-Backup` on the master and `ign-pair-a-Master` on the backup.
- Token A answers 401 on gw-b and token B answers 401 on gw-a after the pair settled: the `external` collection (where the API tokens live) is not shared between the nodes, so each node needs its own token (A10).

## C-P4

Redundancy status of both nodes at labelled moments: GET `/data/api/v1/redundancy` and GET `/data/api/v1/redundancy/providers` verbatim, then each node's gateway-info `name` and `deploymentMode`. gw-a is the master (`$BASE_A`), gw-b the backup (`$BASE_B`).

### C-P4 settled

#### GET redundancy on gw-a (settled)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy'` @ 2026-10-19T20:37:40Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:37:40 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node01qszvpteb64nwnf1n5ec9redb6.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 257

{"role":"Master","projectState":"Good","activityLevel":"Active","localId":"192.168.156.2","peerConnected":true,"peerId":"ign-pair-a-Backup","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":665697,"lastSyncTimestamp":1792441595934}
[curl http=200 total=0.015420s]
~~~

#### GET redundancy/providers on gw-a (settled)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy/providers'` @ 2026-10-19T20:37:40Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:37:40 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 2968

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":2109238983},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1984441276},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1774155707},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1511368313},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1692738986},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1405609793},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1096241197},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":906366992},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":2063837967},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":387098305},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1816087094}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.031373s]
~~~

#### gateway-info name and deploymentMode on gw-a (settled)

`curl -sS -g [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/gateway-info' | jq -c '{name, deploymentMode}'` @ 2026-10-19T20:37:40Z

~~~
{"name":"ign-pair-a","deploymentMode":"production"}
~~~

#### GET redundancy on gw-b (settled)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy'` @ 2026-10-19T20:37:40Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:37:40 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node0430nmvr1cu15hxjzizpc8gvk392.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 251

{"role":"Backup","projectState":"Good","activityLevel":"Cold","localId":"192.168.156.3","peerConnected":true,"peerId":"ign-pair-a-Master","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":-1,"lastSyncTimestamp":1792441595954}
[curl http=200 total=0.007990s]
~~~

#### GET redundancy/providers on gw-b (settled)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy/providers'` @ 2026-10-19T20:37:40Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:37:40 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 3039

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":664758,"lastPullDuration":36,"lastApplyDuration":22,"lastSyncError":"","status":"GOOD","hashcode":434897317},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":664688,"lastPullDuration":18,"lastApplyDuration":16,"lastSyncError":"","status":"GOOD","hashcode":1304977627},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":665055,"lastPullDuration":29,"lastApplyDuration":23,"lastSyncError":"","status":"GOOD","hashcode":1522136736},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":664628,"lastPullDuration":14,"lastApplyDuration":9,"lastSyncError":"","status":"GOOD","hashcode":1942970930},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":664718,"lastPullDuration":18,"lastApplyDuration":12,"lastSyncError":"","status":"GOOD","hashcode":181904583},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":664657,"lastPullDuration":16,"lastApplyDuration":14,"lastSyncError":"","status":"GOOD","hashcode":68666366},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":664820,"lastPullDuration":20,"lastApplyDuration":25,"lastSyncError":"","status":"GOOD","hashcode":294529322},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":664608,"lastPullDuration":12,"lastApplyDuration":11,"lastSyncError":"","status":"GOOD","hashcode":425199579},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":664953,"lastPullDuration":70,"lastApplyDuration":30,"lastSyncError":"","status":"GOOD","hashcode":782133675},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":664867,"lastPullDuration":58,"lastApplyDuration":32,"lastSyncError":"","status":"GOOD","hashcode":346838965},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":665118,"lastPullDuration":255,"lastApplyDuration":73,"lastSyncError":"","status":"GOOD","hashcode":1946938227}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.039317s]
~~~

#### gateway-info name and deploymentMode on gw-b (settled)

`curl -sS -g [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/gateway-info' | jq -c '{name, deploymentMode}'` @ 2026-10-19T20:37:40Z

~~~
{"name":"ign-pair-a","deploymentMode":"production"}
~~~

Which token answers which node: the HTTP status of `GET /data/api/v1/redundancy` per token and node (status codes only; A10: the spike reused one key on both nodes, the setup mints one per node).

| token | gw-a (`$BASE_A`) | gw-b (`$BASE_B`) |
| --- | --- | --- |
| `$IGN_PAIR_TOKEN_A` | 200 | 401 |
| `$IGN_PAIR_TOKEN_B` | 401 | 200 |

## C-P1

Does gw-b `lastSyncTimestamp` advance after a change on the master, and how soon does the change read back on gw-b? Two write paths: (1) the way apply writes: a scan-lock on gw-a, files into gw-a's bind-mounted `production` collection, a config scan; (2) the REST API: `POST` then `PUT` with its signature on `/data/api/v1/resources/ignition/database-connection`. Poll lines: UTC, seconds since the poll started, gw-b `lastSyncTimestamp`, `syncPending`, `peerConnected`, and the `find` result of the changed resource on both nodes (HTTP status and the first 12 characters of the signature).

### C-P1 file path: CAP24_SYNC_1792442633 written into gw-a production/, scan-lock, config scan

#### GET redundancy on gw-b, before the change

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy'` @ 2026-10-19T20:43:53Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:43:53 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node0yddf8vp4cg5y136kxjgnltf9c396.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 251

{"role":"Backup","projectState":"Good","activityLevel":"Cold","localId":"192.168.156.3","peerConnected":true,"peerId":"ign-pair-a-Master","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":-1,"lastSyncTimestamp":1792441595954}
[curl http=200 total=0.120990s]
~~~

#### GET redundancy on gw-a, before the change

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy'` @ 2026-10-19T20:43:54Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:43:54 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node01ukfcmkk690q26it9p0uk1td910.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 258

{"role":"Master","projectState":"Good","activityLevel":"Active","localId":"192.168.156.2","peerConnected":true,"peerId":"ign-pair-a-Backup","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":1039663,"lastSyncTimestamp":1792441595934}
[curl http=200 total=0.083425s]
~~~

#### POST scan-lock/config (acquire 5, hold 60)

`curl -sS -g -i -X POST [token $IGN_PAIR_TOKEN_A] -H Content-Type: application/json --data {"acquireTimeout":5,"holdTimeout":60} '$BASE_A/data/api/v1/scan-lock/config'` @ 2026-10-19T20:43:55Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:43:55 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 91

{
  "timestamp": 27897860,
  "remaining": 59,
  "actor": "api-token:pair-a:192.168.147.1"
}
[curl http=200 total=0.476876s]
~~~

#### CAP24_SYNC_1792442633 on gw-a's bind mount (relative paths)

`bash -c cd\ \'/Users/pmannion/.cache/ign-pair-rig/data-a/config/resources\'\ \&\&\ find\ production\ -path\ \'\*CAP24_SYNC_1792442633\*\'\ -type\ f\ \|\ LC_ALL=C\ sort ` @ 2026-10-19T20:43:57Z

~~~
production/ignition/database-connection/CAP24_SYNC_1792442633/config.json
production/ignition/database-connection/CAP24_SYNC_1792442633/resource.json
[exit 0]
~~~

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/scan/config'` @ 2026-10-19T20:43:57Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:43:57 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": true,
  "lastScanTimestamp": 1792441545720,
  "lastScanDuration": 1199
}
[curl http=200 total=0.076406s]
~~~

#### C-P1 file path poll: 120 s, every 2 s (CAP24_SYNC_1792442633)

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh c1poll 120 CAP24_SYNC_1792442633 exists 1792441595954 yes ` @ 2026-10-19T20:43:57Z

~~~
# columns: UTC, elapsed since the poll started, gw-b lastSyncTimestamp, gw-b syncPending, gw-b peerConnected, gw-a find (HTTP:signature12), gw-b find (HTTP:signature12)
# pre-change gw-b lastSyncTimestamp=1792441595954; read-back rule: exists
2026-10-19T20:43:59Z +  0s lastSyncTimestamp=1792441595954 syncPending=false peerConnected=true gwA_find=404:- gwB_find=404:-
2026-10-19T20:44:02Z +  3s lastSyncTimestamp=1792441595954 syncPending=false peerConnected=true gwA_find=404:- gwB_find=404:-
2026-10-19T20:44:03Z +  4s lastSyncTimestamp=1792441595954 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=404:-
2026-10-19T20:44:05Z +  6s lastSyncTimestamp=1792441595954 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=404:-
2026-10-19T20:44:06Z +  8s lastSyncTimestamp=1792441595954 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=404:-
2026-10-19T20:44:09Z + 10s lastSyncTimestamp=1792442647207 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:10Z + 12s lastSyncTimestamp=1792442647207 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:12Z + 14s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:15Z + 16s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:17Z + 18s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:19Z + 20s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:20Z + 22s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:22Z + 24s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:25Z + 26s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:26Z + 28s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:28Z + 30s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:30Z + 32s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:32Z + 34s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:35Z + 36s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:36Z + 38s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:38Z + 40s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:40Z + 42s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:42Z + 44s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:44Z + 46s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:47Z + 48s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:48Z + 50s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:50Z + 52s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:52Z + 54s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:54Z + 56s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:56Z + 58s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:44:58Z + 60s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:01Z + 62s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:02Z + 64s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:04Z + 66s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:06Z + 68s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:08Z + 70s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:11Z + 72s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:12Z + 74s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:14Z + 76s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:16Z + 78s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:18Z + 80s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:20Z + 82s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:23Z + 84s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:24Z + 86s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:26Z + 88s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:28Z + 90s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:30Z + 92s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:32Z + 94s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:35Z + 96s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:36Z + 98s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:38Z +100s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:40Z +102s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:42Z +104s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:44Z +106s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:47Z +108s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:48Z +110s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:50Z +112s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:52Z +114s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:54Z +116s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:56Z +118s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
2026-10-19T20:45:58Z +120s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:59ab3cd6b220 gwB_find=200:59ab3cd6b220
SUMMARY first_readback_on_gw-b=10 first_syncPending_true=never first_lastSyncTimestamp_change=10 (elapsed seconds; empty = never within 120 s)
[exit 0]
~~~

#### GET scan-lock/config after the scan

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/scan-lock/config'` @ 2026-10-19T20:45:59Z

~~~
HTTP/1.1 204 No Content
Date: Mon, 19 Oct 2026 20:45:59 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block


[curl http=204 total=0.028345s]
~~~

### C-P1 REST path: CAP24_REST_1792442633 created by POST, changed by PUT with its signature

#### POST resources/ignition/database-connection (create CAP24_REST_1792442633)

`curl -sS -g -i -X POST [token $IGN_PAIR_TOKEN_A] -H Content-Type: application/json --data @/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.qz9vYL9SyG/c1-post.json '$BASE_A/data/api/v1/resources/ignition/database-connection'` @ 2026-10-19T20:45:59Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:45:59 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 274

{
  "success": true,
  "changes": [
    {
      "name": "CAP24_REST_1792442633",
      "type": "ignition/database-connection",
      "collection": "core",
      "newSignature": "4348e8c465568ec2f6fa4b60bd79c8ab0bbd735f14f23891c6895b4cd0d07382"
    }
  ],
  "problem": null
}
[curl http=200 total=0.232381s]
~~~

#### C-P1 REST create poll: 30 s, every 2 s (CAP24_REST_1792442633)

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh c1poll 30 CAP24_REST_1792442633 exists 1792442650993 yes ` @ 2026-10-19T20:45:59Z

~~~
# columns: UTC, elapsed since the poll started, gw-b lastSyncTimestamp, gw-b syncPending, gw-b peerConnected, gw-a find (HTTP:signature12), gw-b find (HTTP:signature12)
# pre-change gw-b lastSyncTimestamp=1792442650993; read-back rule: exists
2026-10-19T20:45:59Z +  0s lastSyncTimestamp=1792442650993 syncPending=false peerConnected=true gwA_find=200:4348e8c46556 gwB_find=404:-
2026-10-19T20:46:01Z +  2s lastSyncTimestamp=1792442760884 syncPending=false peerConnected=true gwA_find=200:4348e8c46556 gwB_find=200:4348e8c46556
2026-10-19T20:46:03Z +  4s lastSyncTimestamp=1792442760884 syncPending=false peerConnected=true gwA_find=200:4348e8c46556 gwB_find=200:4348e8c46556
2026-10-19T20:46:06Z +  6s lastSyncTimestamp=1792442765845 syncPending=false peerConnected=true gwA_find=200:4348e8c46556 gwB_find=200:4348e8c46556
2026-10-19T20:46:07Z +  8s lastSyncTimestamp=1792442765845 syncPending=false peerConnected=true gwA_find=200:4348e8c46556 gwB_find=200:4348e8c46556
2026-10-19T20:46:09Z + 10s lastSyncTimestamp=1792442765845 syncPending=false peerConnected=true gwA_find=200:4348e8c46556 gwB_find=200:4348e8c46556
2026-10-19T20:46:11Z + 12s lastSyncTimestamp=1792442765845 syncPending=false peerConnected=true gwA_find=200:4348e8c46556 gwB_find=200:4348e8c46556
2026-10-19T20:46:13Z + 14s lastSyncTimestamp=1792442765845 syncPending=false peerConnected=true gwA_find=200:4348e8c46556 gwB_find=200:4348e8c46556
2026-10-19T20:46:15Z + 16s lastSyncTimestamp=1792442765845 syncPending=false peerConnected=true gwA_find=200:4348e8c46556 gwB_find=200:4348e8c46556
2026-10-19T20:46:18Z + 18s lastSyncTimestamp=1792442765845 syncPending=false peerConnected=true gwA_find=200:4348e8c46556 gwB_find=200:4348e8c46556
2026-10-19T20:46:19Z + 20s lastSyncTimestamp=1792442765845 syncPending=false peerConnected=true gwA_find=200:4348e8c46556 gwB_find=200:4348e8c46556
2026-10-19T20:46:21Z + 22s lastSyncTimestamp=1792442765845 syncPending=false peerConnected=true gwA_find=200:4348e8c46556 gwB_find=200:4348e8c46556
2026-10-19T20:46:24Z + 24s lastSyncTimestamp=1792442765845 syncPending=false peerConnected=true gwA_find=200:4348e8c46556 gwB_find=200:4348e8c46556
2026-10-19T20:46:25Z + 26s lastSyncTimestamp=1792442765845 syncPending=false peerConnected=true gwA_find=200:4348e8c46556 gwB_find=200:4348e8c46556
2026-10-19T20:46:27Z + 28s lastSyncTimestamp=1792442765845 syncPending=false peerConnected=true gwA_find=200:4348e8c46556 gwB_find=200:4348e8c46556
2026-10-19T20:46:30Z + 30s lastSyncTimestamp=1792442765845 syncPending=false peerConnected=true gwA_find=200:4348e8c46556 gwB_find=200:4348e8c46556
SUMMARY first_readback_on_gw-b=2 first_syncPending_true=never first_lastSyncTimestamp_change=2 (elapsed seconds; empty = never within 30 s)
[exit 0]
~~~

#### GET find CAP24_REST_1792442633 (read the signature)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/resources/find/ignition/database-connection/CAP24_REST_1792442633'` @ 2026-10-19T20:46:30Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:46:30 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 10530

{
  "type": "ignition/database-connection",
  "name": "CAP24_REST_1792442633",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "4348e8c465568ec2f6fa4b60bd79c8ab0bbd735f14f23891c6895b4cd0d07382",
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
    "uuid": "d6f24362-3707-457a-9127-c48260afa54d",
    "enabled": true
  },
  "metrics": {
    "active-connections": {
      "name": "databases.connection-CAP24_REST_1792442633.active-connections",
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
      "name": "databases.connection-CAP24_REST_1792442633.active-queries",
      "metric": {
        "value": [],
        "type": "gauge",
        "description": "Active Queries",
        "units": "queries"
      }
    },
    "expensive-queries": {
      "name": "databases.connection-CAP24_REST_1792442633.expensive-queries",
      "metric": {
        "value": [],
        "type": "gauge",
        "description": "Expensive Queries",
        "units": "queries"
      }
    },
    "queries": {
      "name": "databases.connection-CAP24_REST_1792442633.queries",
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
      "name": "databases.connection-CAP24_REST_1792442633.rows",
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
      "name": "databases.connection-CAP24_REST_1792442633.throughput",
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
      "name": "databases.connection-CAP24_REST_1792442633.status",
      "result": {
        "healthy": false,
        "time": 1792442790247,
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
[curl http=200 total=0.019227s]
~~~

#### GET redundancy on gw-b, before the PUT

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy'` @ 2026-10-19T20:46:30Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:46:30 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node01bbnd3uu5h3k3o2m3cm1n3rb0478.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 251

{"role":"Backup","projectState":"Good","activityLevel":"Cold","localId":"192.168.156.3","peerConnected":true,"peerId":"ign-pair-a-Master","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":-1,"lastSyncTimestamp":1792442765845}
[curl http=200 total=0.007282s]
~~~

#### PUT resources/ignition/database-connection (change CAP24_REST_1792442633)

`curl -sS -g -i -X PUT [token $IGN_PAIR_TOKEN_A] -H Content-Type: application/json --data @/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.qz9vYL9SyG/c1-put.json '$BASE_A/data/api/v1/resources/ignition/database-connection'` @ 2026-10-19T20:46:30Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:46:30 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 274

{
  "success": true,
  "changes": [
    {
      "name": "CAP24_REST_1792442633",
      "type": "ignition/database-connection",
      "collection": "core",
      "newSignature": "b5ed41e3e5aa5ef80b0233fcacf6ab1f337113cf93d2e5b7daedaf7381e0758e"
    }
  ],
  "problem": null
}
[curl http=200 total=0.242602s]
~~~

#### C-P1 REST change poll: 120 s, every 2 s (CAP24_REST_1792442633, read-back = gw-b answers the new signature)

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh c1poll 120 CAP24_REST_1792442633 sig:b5ed41e3e5aa 1792442765845 yes ` @ 2026-10-19T20:46:30Z

~~~
# columns: UTC, elapsed since the poll started, gw-b lastSyncTimestamp, gw-b syncPending, gw-b peerConnected, gw-a find (HTTP:signature12), gw-b find (HTTP:signature12)
# pre-change gw-b lastSyncTimestamp=1792442765845; read-back rule: sig:b5ed41e3e5aa
2026-10-19T20:46:31Z +  0s lastSyncTimestamp=1792442765845 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:4348e8c46556
2026-10-19T20:46:34Z +  2s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:46:35Z +  4s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:46:37Z +  6s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:46:40Z +  8s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:46:41Z + 10s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:46:43Z + 12s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:46:46Z + 14s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:46:47Z + 16s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:46:49Z + 18s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:46:51Z + 20s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:46:53Z + 22s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:46:55Z + 24s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:46:58Z + 26s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:46:59Z + 28s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:02Z + 30s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:03Z + 32s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:05Z + 34s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:07Z + 36s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:09Z + 38s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:11Z + 40s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:14Z + 42s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:15Z + 44s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:17Z + 46s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:19Z + 48s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:21Z + 50s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:23Z + 52s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:26Z + 55s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:27Z + 56s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:29Z + 58s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:31Z + 60s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:34Z + 62s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:35Z + 64s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:37Z + 66s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:39Z + 68s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:41Z + 70s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:43Z + 72s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:46Z + 74s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:47Z + 76s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:49Z + 78s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:51Z + 80s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:53Z + 82s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:55Z + 84s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:58Z + 86s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:47:59Z + 88s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:48:01Z + 90s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:48:03Z + 92s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:48:05Z + 94s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:48:08Z + 96s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:48:09Z + 98s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:48:11Z +100s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:48:13Z +102s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:48:15Z +104s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:48:18Z +106s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:48:19Z +108s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:48:21Z +110s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:48:23Z +112s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:48:25Z +114s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:48:28Z +116s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:48:29Z +118s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
2026-10-19T20:48:31Z +120s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true gwA_find=200:b5ed41e3e5aa gwB_find=200:b5ed41e3e5aa
SUMMARY first_readback_on_gw-b=2 first_syncPending_true=never first_lastSyncTimestamp_change=2 (elapsed seconds; empty = never within 120 s)
[exit 0]
~~~

No `### C-P4 during-sync` block: no 2-second poll of the C-P1 runs read gw-b `syncPending` as true, so the state was never caught mid-sync at this poll rate.

## C-P2

After the C-P1 syncs settled: the sha256 of every file below each changed unit and below one untouched unit, on gw-a's and gw-b's bind directories (`$HOME/.cache/ign-pair-rig/data-a/config/resources` and `data-b`), and whether the pair is byte-equal. The units are found by name in either directory, so a REST-made resource shows which collection it landed in.

`for each unit: (cd $BIND_A/config/resources; find <unit> -type f) + (cd $BIND_B/config/resources; ...) | shasum -a 256` @ 2026-10-19T20:48:54Z

Units compared (relative to `config/resources`): core/ignition/store-and-forward-engine/CAP24_SYNC_1792442633 core/ignition/tag-definition/System/Gateway/Database/CAP24_SYNC_1792442633 production/ignition/database-connection/CAP24_SYNC_1792442633 core/ignition/database-connection/CAP24_REST_1792442633 core/ignition/store-and-forward-engine/CAP24_REST_1792442633 core/ignition/tag-definition/System/Gateway/Database/CAP24_REST_1792442633 core/ignition/general-alarm-settings

| path (relative to config/resources) | sha256 gw-a | sha256 gw-b | equal |
| --- | --- | --- | --- |
| `core/ignition/store-and-forward-engine/CAP24_SYNC_1792442633/config.json` | `55fe28071bfc1dbdbdb9f30c04d699e034aeec89c7cc06e10eacd47b6fba524f` | `55fe28071bfc1dbdbdb9f30c04d699e034aeec89c7cc06e10eacd47b6fba524f` | yes |
| `core/ignition/store-and-forward-engine/CAP24_SYNC_1792442633/resource.json` | `e821b2db711f8f2821287ba51558e3d3ed902a2a590eaeb33a200ce536c95cf5` | `b0b7fc1c2d1857d5a7c379aebd36b7d4b5b7d30738e3e731c538a19fd28c0a0b` | no |
| `core/ignition/tag-definition/System/Gateway/Database/CAP24_SYNC_1792442633/unary-resource.json` | `4e76fe7dd0d0ee1da9fe7be0fb8a8dca8027f4430bbfec579b8f478556a0cf20` | `4e76fe7dd0d0ee1da9fe7be0fb8a8dca8027f4430bbfec579b8f478556a0cf20` | yes |
| `production/ignition/database-connection/CAP24_SYNC_1792442633/config.json` | `6fd83e6185cc852432fe139b3fef86cf1a6b29cd545d11cf948d8257d0b9ac82` | `6fd83e6185cc852432fe139b3fef86cf1a6b29cd545d11cf948d8257d0b9ac82` | yes |
| `production/ignition/database-connection/CAP24_SYNC_1792442633/resource.json` | `e73be7224892bfa0244e30349ff9f679fa1deeae72f57f2bdef94748e06052ec` | `4ea0545411dd1a7db4dfffebe14c83fa9f8f4109012f65cef3c8c32bcb12d709` | no |
| `core/ignition/database-connection/CAP24_REST_1792442633/config.json` | `620a0e53061c406d42186ec82c7981e155d6e2e969f8c8d8a7ff199fdee3660d` | `620a0e53061c406d42186ec82c7981e155d6e2e969f8c8d8a7ff199fdee3660d` | yes |
| `core/ignition/database-connection/CAP24_REST_1792442633/resource.json` | `ebcf5d1eafee599aca413741c946ddbd638d4ffd2914cc14236373900e0c8329` | `11bc990ce4d919430d6b709f114ba15dccb2013a3f7f18aff50053309a9dbfa5` | no |
| `core/ignition/store-and-forward-engine/CAP24_REST_1792442633/config.json` | `55fe28071bfc1dbdbdb9f30c04d699e034aeec89c7cc06e10eacd47b6fba524f` | `55fe28071bfc1dbdbdb9f30c04d699e034aeec89c7cc06e10eacd47b6fba524f` | yes |
| `core/ignition/store-and-forward-engine/CAP24_REST_1792442633/resource.json` | `0793e2d0e533d61eec11396499175707bb2dc3bf34e4ca6c889d2e801c1a2b77` | `f14d8a4f7f335c66b64570b5235c8d719616fcb7540cbf4cb6b48215b2c80e4d` | no |
| `core/ignition/tag-definition/System/Gateway/Database/CAP24_REST_1792442633/unary-resource.json` | `4e76fe7dd0d0ee1da9fe7be0fb8a8dca8027f4430bbfec579b8f478556a0cf20` | `4e76fe7dd0d0ee1da9fe7be0fb8a8dca8027f4430bbfec579b8f478556a0cf20` | yes |
| `core/ignition/general-alarm-settings/config.json` | `ce86cc6f24054029adf769412b908d93f5046c5026d8c64c17a953b22cddb4e6` | `ce86cc6f24054029adf769412b908d93f5046c5026d8c64c17a953b22cddb4e6` | yes |
| `core/ignition/general-alarm-settings/resource.json` | `d5dabe5666a136176d2b65cf94c7c96a35bb69d2f51e6a77b23e4be06a531a72` | `d5dabe5666a136176d2b65cf94c7c96a35bb69d2f51e6a77b23e4be06a531a72` | yes |

Files that differ: core/ignition/store-and-forward-engine/CAP24_SYNC_1792442633/resource.json production/ignition/database-connection/CAP24_SYNC_1792442633/resource.json core/ignition/database-connection/CAP24_REST_1792442633/resource.json core/ignition/store-and-forward-engine/CAP24_REST_1792442633/resource.json

### C-P2 diff of the differing files (values of secret-bearing members replaced by <redacted>)

`diff -u $BIND_A/config/resources/core/ignition/store-and-forward-engine/CAP24_SYNC_1792442633/resource.json $BIND_B/config/resources/core/ignition/store-and-forward-engine/CAP24_SYNC_1792442633/resource.json | perl (redact password/secret/token/apikey values)` @ 2026-10-19T20:48:55Z

~~~
--- /Users/pmannion/.cache/ign-pair-rig/data-a/config/resources/core/ignition/store-and-forward-engine/CAP24_SYNC_1792442633/resource.json	2026-10-19 14:44:04
+++ /Users/pmannion/.cache/ign-pair-rig/data-b/config/resources/core/ignition/store-and-forward-engine/CAP24_SYNC_1792442633/resource.json	2026-10-19 14:44:06
@@ -11,7 +11,7 @@
       "actor": "unknown",
       "timestamp": "2026-10-19T20:44:04Z"
     },
-    "uuid": "442ee5dc-ee11-4380-ba0d-c73e96336159",
-    "lastModificationSignature": "1cd9cd68739d650465c0144bbf243b1a4781de162f79a1bb7ce8b250c508527e"
+    "lastModificationSignature": "1cd9cd68739d650465c0144bbf243b1a4781de162f79a1bb7ce8b250c508527e",
+    "uuid": "442ee5dc-ee11-4380-ba0d-c73e96336159"
   }
 }
\ No newline at end of file
~~~

`diff -u $BIND_A/config/resources/production/ignition/database-connection/CAP24_SYNC_1792442633/resource.json $BIND_B/config/resources/production/ignition/database-connection/CAP24_SYNC_1792442633/resource.json | perl (redact password/secret/token/apikey values)` @ 2026-10-19T20:48:55Z

~~~
--- /Users/pmannion/.cache/ign-pair-rig/data-a/config/resources/production/ignition/database-connection/CAP24_SYNC_1792442633/resource.json	2026-10-19 14:43:55
+++ /Users/pmannion/.cache/ign-pair-rig/data-b/config/resources/production/ignition/database-connection/CAP24_SYNC_1792442633/resource.json	2026-10-19 14:44:06
@@ -1 +1,13 @@
-{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["config.json"],"attributes":{"uuid":"5aac1cea-c95c-4b8b-a9b3-df383c703601","enabled":true}}
+{
+  "scope": "A",
+  "version": 1,
+  "restricted": false,
+  "overridable": true,
+  "files": [
+    "config.json"
+  ],
+  "attributes": {
+    "uuid": "5aac1cea-c95c-4b8b-a9b3-df383c703601",
+    "enabled": true
+  }
+}
\ No newline at end of file
~~~

`diff -u $BIND_A/config/resources/core/ignition/database-connection/CAP24_REST_1792442633/resource.json $BIND_B/config/resources/core/ignition/database-connection/CAP24_REST_1792442633/resource.json | perl (redact password/secret/token/apikey values)` @ 2026-10-19T20:48:55Z

~~~
--- /Users/pmannion/.cache/ign-pair-rig/data-a/config/resources/core/ignition/database-connection/CAP24_REST_1792442633/resource.json	2026-10-19 14:46:30
+++ /Users/pmannion/.cache/ign-pair-rig/data-b/config/resources/core/ignition/database-connection/CAP24_REST_1792442633/resource.json	2026-10-19 14:46:32
@@ -12,8 +12,8 @@
       "actor": "api-token:pair-a:192.168.147.1",
       "timestamp": "2026-10-19T20:46:30Z"
     },
-    "uuid": "d6f24362-3707-457a-9127-c48260afa54d",
     "lastModificationSignature": "c79fd5cd76c4fc4beaf6ad97fd9a1b85574f605a640e3c1720bca2db977d900b",
+    "uuid": "d6f24362-3707-457a-9127-c48260afa54d",
     "enabled": true
   }
 }
\ No newline at end of file
~~~

`diff -u $BIND_A/config/resources/core/ignition/store-and-forward-engine/CAP24_REST_1792442633/resource.json $BIND_B/config/resources/core/ignition/store-and-forward-engine/CAP24_REST_1792442633/resource.json | perl (redact password/secret/token/apikey values)` @ 2026-10-19T20:48:55Z

~~~
--- /Users/pmannion/.cache/ign-pair-rig/data-a/config/resources/core/ignition/store-and-forward-engine/CAP24_REST_1792442633/resource.json	2026-10-19 14:45:59
+++ /Users/pmannion/.cache/ign-pair-rig/data-b/config/resources/core/ignition/store-and-forward-engine/CAP24_REST_1792442633/resource.json	2026-10-19 14:46:00
@@ -11,7 +11,7 @@
       "actor": "unknown",
       "timestamp": "2026-10-19T20:45:59Z"
     },
-    "uuid": "0158c3c5-7efc-4688-b626-f831f6cbdfba",
-    "lastModificationSignature": "29177fcf0da2d0300743d5a1bcd3f18e88c98e7461a253b9f99844a50a340d5f"
+    "lastModificationSignature": "29177fcf0da2d0300743d5a1bcd3f18e88c98e7461a253b9f99844a50a340d5f",
+    "uuid": "0158c3c5-7efc-4688-b626-f831f6cbdfba"
   }
 }
\ No newline at end of file
~~~

Any value of a member whose name holds password, secret, token or apikey would be shown as `<redacted>` above; none of these units is expected to carry one.

## C-P3

POST `/data/api/v1/redundancy/gwaction/resync` sent to the master (gw-a, `$BASE_A`); it is never sent to gw-b. Then gw-b status every 2 s until the pair is settled with an advanced `lastSyncTimestamp`, or 120 s.

#### GET redundancy on gw-b, before the resync

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy'` @ 2026-10-19T20:49:00Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:49:00 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node0n1ekxelj1uomav5fc2ontsqc543.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 251

{"role":"Backup","projectState":"Good","activityLevel":"Cold","localId":"192.168.156.3","peerConnected":true,"peerId":"ign-pair-a-Master","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":-1,"lastSyncTimestamp":1792442792772}
[curl http=200 total=0.008394s]
~~~

#### POST redundancy/gwaction/resync (sent to gw-a)

`curl -sS -g -i -X POST [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy/gwaction/resync'` @ 2026-10-19T20:49:00Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:49:00 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node01xe121v1dm5zp4mbjq0nt9jy15.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 26

{"resync_state":"started"}
[curl http=200 total=0.016476s]
~~~

#### C-P3 poll: gw-b status every 2 s until settled with an advanced lastSyncTimestamp or 120 s

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh c3poll 120 1792442792772 ` @ 2026-10-19T20:49:00Z

~~~
# columns: UTC, elapsed since the poll started, gw-b lastSyncTimestamp, syncPending, peerConnected, activityLevel, gw-b projectState, gw-a projectState, settled
# pre-resync gw-b lastSyncTimestamp=1792442792772
2026-10-19T20:49:00Z +  0s lastSyncTimestamp=1792442792772 syncPending=false peerConnected=true activityLevel=Cold gwB_projectState=Good gwA_projectState=Good settled=yes
2026-10-19T20:49:02Z +  2s lastSyncTimestamp=1792442942220 syncPending=false peerConnected=true activityLevel=Cold gwB_projectState=Good gwA_projectState=Good settled=yes
SUMMARY settled with an advanced lastSyncTimestamp at +2s (first_syncPending_true=never first_lastSyncTimestamp_change=2 first_settled=0)
[exit 0]
~~~

#### GET redundancy/providers on gw-b, after the resync

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy/providers'` @ 2026-10-19T20:49:02Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:49:02 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 3005

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":748,"lastPullDuration":18,"lastApplyDuration":11,"lastSyncError":"","status":"GOOD","hashcode":566020179},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":705,"lastPullDuration":11,"lastApplyDuration":12,"lastSyncError":"","status":"GOOD","hashcode":9128490},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":963,"lastPullDuration":12,"lastApplyDuration":13,"lastSyncError":"","status":"GOOD","hashcode":1157797129},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":657,"lastPullDuration":15,"lastApplyDuration":8,"lastSyncError":"","status":"GOOD","hashcode":821629443},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":727,"lastPullDuration":10,"lastApplyDuration":10,"lastSyncError":"","status":"GOOD","hashcode":2000730655},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":682,"lastPullDuration":11,"lastApplyDuration":9,"lastSyncError":"","status":"GOOD","hashcode":767587527},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":784,"lastPullDuration":10,"lastApplyDuration":18,"lastSyncError":"","status":"GOOD","hashcode":2092631723},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":630,"lastPullDuration":18,"lastApplyDuration":17,"lastSyncError":"","status":"GOOD","hashcode":69892661},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":920,"lastPullDuration":26,"lastApplyDuration":44,"lastSyncError":"","status":"GOOD","hashcode":1992861933},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":809,"lastPullDuration":56,"lastApplyDuration":29,"lastSyncError":"","status":"GOOD","hashcode":1484819647},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":992,"lastPullDuration":284,"lastApplyDuration":37,"lastSyncError":"","status":"GOOD","hashcode":2083149658}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.010393s]
~~~

## Cleanup after C-P1 to C-P3

Remove CAP24_SYNC_1792442633 (files, under a scan-lock, then a config scan) and CAP24_REST_1792442633 (REST DELETE with its signature) from gw-a, and wait for gw-b to drop both.

#### POST scan-lock/config (acquire 5, hold 60)

`curl -sS -g -i -X POST [token $IGN_PAIR_TOKEN_A] -H Content-Type: application/json --data {"acquireTimeout":5,"holdTimeout":60} '$BASE_A/data/api/v1/scan-lock/config'` @ 2026-10-19T20:49:06Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:49:06 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 91

{
  "timestamp": 28209073,
  "remaining": 59,
  "actor": "api-token:pair-a:192.168.147.1"
}
[curl http=200 total=0.013171s]
~~~

#### every path of CAP24_SYNC_1792442633 on gw-a's bind mount before removal

`bash -c cd\ \'/Users/pmannion/.cache/ign-pair-rig/data-a/config/resources\'\ \&\&\ find\ .\ -path\ \'\*CAP24_SYNC_1792442633\*\'\ \|\ LC_ALL=C\ sort ` @ 2026-10-19T20:49:06Z

~~~
./core/ignition/store-and-forward-engine/CAP24_SYNC_1792442633
./core/ignition/store-and-forward-engine/CAP24_SYNC_1792442633/config.json
./core/ignition/store-and-forward-engine/CAP24_SYNC_1792442633/resource.json
./core/ignition/tag-definition/System/Gateway/Database/CAP24_SYNC_1792442633
./core/ignition/tag-definition/System/Gateway/Database/CAP24_SYNC_1792442633/unary-resource.json
./production/ignition/database-connection/CAP24_SYNC_1792442633
./production/ignition/database-connection/CAP24_SYNC_1792442633/config.json
./production/ignition/database-connection/CAP24_SYNC_1792442633/resource.json
[exit 0]
~~~

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/scan/config'` @ 2026-10-19T20:49:06Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:49:06 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": true,
  "lastScanTimestamp": 1792442643382,
  "lastScanDuration": 5458
}
[curl http=200 total=0.012222s]
~~~

#### GET find CAP24_REST_1792442633 (read the signature)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/resources/find/ignition/database-connection/CAP24_REST_1792442633'` @ 2026-10-19T20:49:08Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:49:08 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 10770

{
  "type": "ignition/database-connection",
  "name": "CAP24_REST_1792442633",
  "description": "cap24 rest update",
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "b5ed41e3e5aa5ef80b0233fcacf6ab1f337113cf93d2e5b7daedaf7381e0758e",
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
      "actor": "api-token:pair-a:192.168.147.1",
      "timestamp": "2026-10-19T20:46:30Z"
    },
    "uuid": "d6f24362-3707-457a-9127-c48260afa54d",
    "lastModificationSignature": "c79fd5cd76c4fc4beaf6ad97fd9a1b85574f605a640e3c1720bca2db977d900b",
    "enabled": true
  },
  "metrics": {
    "active-connections": {
      "name": "databases.connection-CAP24_REST_1792442633.active-connections",
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
      "name": "databases.connection-CAP24_REST_1792442633.active-queries",
      "metric": {
        "value": [],
        "type": "gauge",
        "description": "Active Queries",
        "units": "queries"
      }
    },
    "expensive-queries": {
      "name": "databases.connection-CAP24_REST_1792442633.expensive-queries",
      "metric": {
        "value": [],
        "type": "gauge",
        "description": "Expensive Queries",
        "units": "queries"
      }
    },
    "queries": {
      "name": "databases.connection-CAP24_REST_1792442633.queries",
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
      "name": "databases.connection-CAP24_REST_1792442633.rows",
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
      "name": "databases.connection-CAP24_REST_1792442633.throughput",
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
      "name": "databases.connection-CAP24_REST_1792442633.status",
      "result": {
        "healthy": false,
        "time": 1792442948139,
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
[curl http=200 total=0.061611s]
~~~

#### DELETE resources/ignition/database-connection/CAP24_REST_1792442633/<signature>

`curl -sS -g -i -X DELETE [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/resources/ignition/database-connection/CAP24_REST_1792442633/b5ed41e3e5aa5ef80b0233fcacf6ab1f337113cf93d2e5b7daedaf7381e0758e'` @ 2026-10-19T20:49:08Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:49:08 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 294

{
  "success": true,
  "changes": [
    {
      "name": "CAP24_REST_1792442633",
      "type": "ignition/database-connection",
      "collection": "core",
      "newSignature": "b5ed41e3e5aa5ef80b0233fcacf6ab1f337113cf93d2e5b7daedaf7381e0758e"
    }
  ],
  "problem": null,
  "references": []
}
[curl http=200 total=0.199884s]
~~~

#### wait for gw-b to drop CAP24_SYNC_1792442633 and CAP24_REST_1792442633 (every 2 s, bound 120 s)

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh dropwait CAP24_SYNC_1792442633 CAP24_REST_1792442633 ` @ 2026-10-19T20:49:08Z

~~~
2026-10-19T20:49:08Z +  0s CAP24_SYNC_1792442633=gwB:200:59ab3cd6b220 CAP24_REST_1792442633=gwB:200:b5ed41e3e5aa
2026-10-19T20:49:10Z +  2s CAP24_SYNC_1792442633=gwB:404:- CAP24_REST_1792442633=gwB:404:-
SUMMARY gw-b dropped every named resource at +2s
[exit 0]
~~~

## C-P4 (continued: after-cleanup)

### C-P4 after-cleanup

#### GET redundancy on gw-a (after-cleanup)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy'` @ 2026-10-19T20:49:10Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:49:10 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node0s5c3rs3jyl3xg8mjl2pt8lx19.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 258

{"role":"Master","projectState":"Good","activityLevel":"Active","localId":"192.168.156.2","peerConnected":true,"peerId":"ign-pair-a-Backup","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":1356181,"lastSyncTimestamp":1792442949547}
[curl http=200 total=0.004917s]
~~~

#### GET redundancy/providers on gw-a (after-cleanup)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy/providers'` @ 2026-10-19T20:49:10Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:49:10 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 3033

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1255635867},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1509758935},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1003355160},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1269873268},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":977931066},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":188276747},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1606545480},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":300716766},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":0,"queuedUpdatesPerSec":0.004641664768091066,"dispatchedUpdatesPerSec":0.0047747079638245,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":2073380866},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":929159107},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":0,"queuedUpdatesPerSec":0.002888161521135872,"dispatchedUpdatesPerSec":0.0030118410402350144,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1743921879}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.006736s]
~~~

#### gateway-info name and deploymentMode on gw-a (after-cleanup)

`curl -sS -g [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/gateway-info' | jq -c '{name, deploymentMode}'` @ 2026-10-19T20:49:10Z

~~~
{"name":"ign-pair-a","deploymentMode":"production"}
~~~

#### GET redundancy on gw-b (after-cleanup)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy'` @ 2026-10-19T20:49:10Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:49:10 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node0w8uq4lsfw0mm19w2l2mdnrh7y548.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 251

{"role":"Backup","projectState":"Good","activityLevel":"Cold","localId":"192.168.156.3","peerConnected":true,"peerId":"ign-pair-a-Master","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":-1,"lastSyncTimestamp":1792442949809}
[curl http=200 total=0.004576s]
~~~

#### GET redundancy/providers on gw-b (after-cleanup)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy/providers'` @ 2026-10-19T20:49:10Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:49:10 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 3018

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":8911,"lastPullDuration":18,"lastApplyDuration":11,"lastSyncError":"","status":"GOOD","hashcode":1518590349},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":8868,"lastPullDuration":11,"lastApplyDuration":12,"lastSyncError":"","status":"GOOD","hashcode":287056042},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":9126,"lastPullDuration":12,"lastApplyDuration":13,"lastSyncError":"","status":"GOOD","hashcode":1359953086},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":8820,"lastPullDuration":15,"lastApplyDuration":8,"lastSyncError":"","status":"GOOD","hashcode":838742188},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":8890,"lastPullDuration":10,"lastApplyDuration":10,"lastSyncError":"","status":"GOOD","hashcode":1970653359},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":8845,"lastPullDuration":11,"lastApplyDuration":9,"lastSyncError":"","status":"GOOD","hashcode":94158811},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":8947,"lastPullDuration":10,"lastApplyDuration":18,"lastSyncError":"","status":"GOOD","hashcode":761174298},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":8793,"lastPullDuration":18,"lastApplyDuration":17,"lastSyncError":"","status":"GOOD","hashcode":1843771213},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":1188,"lastPullDuration":85,"lastApplyDuration":194,"lastSyncError":"","status":"GOOD","hashcode":915286766},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":8972,"lastPullDuration":56,"lastApplyDuration":29,"lastSyncError":"","status":"GOOD","hashcode":1443641731},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":1472,"lastPullDuration":157,"lastApplyDuration":58,"lastSyncError":"","status":"GOOD","hashcode":256109585}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.004970s]
~~~

#### gateway-info name and deploymentMode on gw-b (after-cleanup)

`curl -sS -g [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/gateway-info' | jq -c '{name, deploymentMode}'` @ 2026-10-19T20:49:11Z

~~~
{"name":"ign-pair-a","deploymentMode":"production"}
~~~

## C-P5

Does `PUT /data/api/v1/gateway-network-incoming/toggleApproval/<id>/false` on gw-a cut an established link while both REST APIs stay up? Poll lines every 2 s for 60 s: gw-b and gw-a `peerConnected`, the HTTP status of `GET /StatusPing` on both nodes and of `GET /data/api/v1/redundancy` on gw-b. The fallback (`docker network disconnect` of gw-b from `gan` only) is captured only when the API cut does not drop the link.

### C-P5 cut by toggleApproval false on gw-a

#### GET resources/list/ignition/gateway-network-incoming (before the cut)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/resources/list/ignition/gateway-network-incoming'` @ 2026-10-19T20:49:41Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:49:41 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 1110

{"items":[{"type":"ignition/gateway-network-incoming","name":"ign-pair-a-backup_335fe286-20e6-411b-a5e2-e96c8ba2cfdc","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"7c7d17d8be52f4c077655770b1695386de3c6d14812f2b1632b0ca49394ab169","config":{"connectionId":"ign-pair-a-backup|335fe286-20e6-411b-a5e2-e96c8ba2cfdc","securityStatus":"Approved"},"data":["config.json"],"attributes":{"lastModification":{"actor":"unknown","timestamp":"2026-10-19T20:24:53Z"},"uuid":"932a31e3-aa40-415d-9dd0-b8532e7e139d","lastModificationSignature":"31a243e6dc211847634890b25d58a2289a68abd77b26417ec33e0b0a5f84ee81"},"metrics":{},"healthchecks":{"status":{"name":"gateway-network-incoming.ign-pair-a-backup_335fe286-20e6-411b-a5e2-e96c8ba2cfdc.status","result":{"healthy":true,"time":1792442981381,"message":"Running","details":{"redundancyRole":"Backup","gatewayName":"ign-pair-a-backup","certificate":"gw-b:8060 [4680003373881824017]","securityStatus":"Approved","remoteVersion":"8.3.6","serializer":"protobuf"}}}}}],"metadata":{"total":1,"matching":1,"limit":-1,"offset":0}}
[curl http=200 total=0.027377s]
~~~

#### setup-pair.sh status (before the cut)

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/setup-pair.sh status ` @ 2026-10-19T20:49:41Z

~~~
gw-a Master Active true false 1792442949547 Good production
gw-b Backup Cold true false 1792442949809 Good production
[exit 0]
~~~

#### cut the link: setup-pair.sh cut (PUT toggleApproval false on gw-a)

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/setup-pair.sh cut ` @ 2026-10-19T20:49:41Z

~~~
toggleApproval false 'ign-pair-a-backup|335fe286-20e6-411b-a5e2-e96c8ba2cfdc' on gw-a -> HTTP 200
[exit 0]
~~~

#### C-P5 poll: 60 s, every 2 s, after the toggleApproval cut

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh c5poll 60 ` @ 2026-10-19T20:49:41Z

~~~
# columns: UTC, elapsed, gw-b peerConnected, gw-a peerConnected, gw-a StatusPing HTTP, gw-b StatusPing HTTP, gw-b GET redundancy HTTP, gw-b syncPending
2026-10-19T20:49:42Z +  0s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:49:44Z +  2s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:49:46Z +  4s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:49:48Z +  6s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:49:51Z +  8s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:49:52Z + 10s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:49:55Z + 12s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:49:56Z + 14s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:49:58Z + 16s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:50:00Z + 18s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:50:03Z + 20s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:50:04Z + 22s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:50:06Z + 24s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:50:09Z + 26s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:50:10Z + 28s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:50:12Z + 30s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:50:14Z + 32s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:50:16Z + 34s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:50:19Z + 36s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:50:20Z + 38s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:50:22Z + 40s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:50:25Z + 42s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:50:26Z + 44s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:50:28Z + 46s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:50:30Z + 48s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:50:32Z + 50s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:50:34Z + 52s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:50:36Z + 54s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:50:38Z + 56s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:50:41Z + 58s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:50:42Z + 60s gwB_peerConnected=false gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
SUMMARY first_gw-b_peerConnected_false=0 (elapsed seconds; never = it stayed true for 60 s)
[exit 0]
~~~

#### GET resources/list/ignition/gateway-network-incoming (during the cut)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/resources/list/ignition/gateway-network-incoming'` @ 2026-10-19T20:50:42Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:50:42 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 1118

{"items":[{"type":"ignition/gateway-network-incoming","name":"ign-pair-a-backup_335fe286-20e6-411b-a5e2-e96c8ba2cfdc","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"1e0d8eee7243d2bdbb283e5f462fa6ab105746d88b1d5bd2a46acda3fa7265b2","config":{"connectionId":"ign-pair-a-backup|335fe286-20e6-411b-a5e2-e96c8ba2cfdc","securityStatus":"PendingApproval"},"data":["config.json"],"attributes":{"lastModification":{"actor":"unknown","timestamp":"2026-10-19T20:49:41Z"},"uuid":"932a31e3-aa40-415d-9dd0-b8532e7e139d","lastModificationSignature":"445f735494f553015a4d2984ca23cac3549115e59cb3e1ab2da2a129709a519e"},"metrics":{},"healthchecks":{"status":{"name":"gateway-network-incoming.ign-pair-a-backup_335fe286-20e6-411b-a5e2-e96c8ba2cfdc.status","result":{"healthy":false,"time":1792443042973,"message":"Faulted","details":{"redundancyRole":"Unknown","gatewayName":"Unknown","certificate":"gw-b:8060 [4680003373881824017]","securityStatus":"PendingApproval","remoteVersion":"Unknown","serializer":"protobuf"}}}}}],"metadata":{"total":1,"matching":1,"limit":-1,"offset":0}}
[curl http=200 total=0.120029s]
~~~

## C-P4 (continued: after-cut)

### C-P4 after-cut

#### GET redundancy on gw-a (after-cut)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy'` @ 2026-10-19T20:50:43Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:50:43 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node0t7ps61gowymr14u0k7updglwo54.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 262

{"role":"Master","projectState":"Unknown","activityLevel":"Active","localId":"192.168.156.2","peerConnected":false,"peerId":"ign-pair-a-Backup","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":1448505,"lastSyncTimestamp":1792442949547}
[curl http=200 total=0.018023s]
~~~

#### GET redundancy/providers on gw-a (after-cut)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy/providers'` @ 2026-10-19T20:50:43Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:50:43 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 3065

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1878041700},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":749586816},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1513250505},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":572549455},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1217997547},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":171580337},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1110890537},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1677568817},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":1,"queuedUpdatesPerSec":0.02875553148774783,"dispatchedUpdatesPerSec":0.02238880124516227,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1647446999},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":948921267},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":1,"queuedUpdatesPerSec":0.021259354379206903,"dispatchedUpdatesPerSec":0.014890701870651413,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1284248347}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.045123s]
~~~

#### gateway-info name and deploymentMode on gw-a (after-cut)

`curl -sS -g [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/gateway-info' | jq -c '{name, deploymentMode}'` @ 2026-10-19T20:50:43Z

~~~
{"name":"ign-pair-a","deploymentMode":"production"}
~~~

#### GET redundancy on gw-b (after-cut)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy'` @ 2026-10-19T20:50:43Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:50:43 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node0t2klbsduna0w1uzznpwdvdv5m614.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 264

{"role":"Backup","projectState":"Out of Date","activityLevel":"Active","localId":"192.168.156.3","peerConnected":false,"peerId":"ign-pair-a-Master","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":61621,"lastSyncTimestamp":1792442949809}
[curl http=200 total=0.007259s]
~~~

#### GET redundancy/providers on gw-b (after-cut)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy/providers'` @ 2026-10-19T20:50:43Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:50:43 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 2998

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":108723352},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":454008359},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1242323730},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":2064303643},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":709196766},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1306273303},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1704996981},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1211566276},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1236078999},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1710058027},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":19582331}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.060802s]
~~~

#### gateway-info name and deploymentMode on gw-b (after-cut)

`curl -sS -g [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/gateway-info' | jq -c '{name, deploymentMode}'` @ 2026-10-19T20:50:44Z

~~~
{"name":"ign-pair-a","deploymentMode":"production"}
~~~

#### restore the link: toggleApproval true on gw-a and relink, then wait for settled

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh c5link api ` @ 2026-10-19T20:50:44Z

~~~
toggleApproval true 'ign-pair-a-backup|335fe286-20e6-411b-a5e2-e96c8ba2cfdc' on gw-a -> HTTP 200
peerConnected true after 7 s
SUMMARY reconnect: peerConnected true 7 s and settled 7 s after the restore command started
[exit 0]
~~~

### C-P4 after-reconnect

#### GET redundancy on gw-a (after-reconnect)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy'` @ 2026-10-19T20:50:51Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:50:51 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node01ib3mltog3ptqmyvow5z4kcf156.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 258

{"role":"Master","projectState":"Good","activityLevel":"Active","localId":"192.168.156.2","peerConnected":true,"peerId":"ign-pair-a-Backup","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":1456594,"lastSyncTimestamp":1792443050579}
[curl http=200 total=0.009319s]
~~~

#### GET redundancy/providers on gw-a (after-reconnect)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy/providers'` @ 2026-10-19T20:50:51Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:50:51 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 3031

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":379901714},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":424514346},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1112382410},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1140735873},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1934781611},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":116614440},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":684600823},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":444472664},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":0,"queuedUpdatesPerSec":0.03905356984163698,"dispatchedUpdatesPerSec":0.05093394524450877,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":2049510792},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1743711680},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":0,"queuedUpdatesPerSec":0.032708192912713714,"dispatchedUpdatesPerSec":0.044586941152571585,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1041679711}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.007579s]
~~~

#### gateway-info name and deploymentMode on gw-a (after-reconnect)

`curl -sS -g [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/gateway-info' | jq -c '{name, deploymentMode}'` @ 2026-10-19T20:50:51Z

~~~
{"name":"ign-pair-a","deploymentMode":"production"}
~~~

#### GET redundancy on gw-b (after-reconnect)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy'` @ 2026-10-19T20:50:51Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:50:51 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node0228jdj8yt7uc1j4cnfqlrdgyc618.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 251

{"role":"Backup","projectState":"Good","activityLevel":"Cold","localId":"192.168.156.3","peerConnected":true,"peerId":"ign-pair-a-Master","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":-1,"lastSyncTimestamp":1792443050592}
[curl http=200 total=0.013741s]
~~~

#### GET redundancy/providers on gw-b (after-reconnect)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy/providers'` @ 2026-10-19T20:50:51Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:50:51 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 2971

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":965085793},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":301645665},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":211867833},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1875985857},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1720553304},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":575157599},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":673666239},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1304294199},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":862,"lastPullDuration":40,"lastApplyDuration":2,"lastSyncError":"","status":"GOOD","hashcode":1195397783},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":608265709},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":904,"lastPullDuration":166,"lastApplyDuration":38,"lastSyncError":"","status":"GOOD","hashcode":911298422}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.009434s]
~~~

#### gateway-info name and deploymentMode on gw-b (after-reconnect)

`curl -sS -g [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/gateway-info' | jq -c '{name, deploymentMode}'` @ 2026-10-19T20:50:51Z

~~~
{"name":"ign-pair-a","deploymentMode":"production"}
~~~

Result: the toggleApproval cut dropped the link (gw-b `peerConnected` read false in the poll above) while the REST APIs stayed up; the fallback was not needed.

## C-P6

With the ping settings setup-pair.sh applied (pingRate 1000 ms, pingTimeout 1000 ms, pingMaxMissed 1200), does a cut backup stay Cold or promote itself? Cut method: api. Poll lines every 5 s for 120 s: both nodes' `activityLevel` and `peerConnected`, gw-b `projectState` and both `/StatusPing` statuses.

#### cut the link: setup-pair.sh cut (PUT toggleApproval false on gw-a)

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/setup-pair.sh cut ` @ 2026-10-19T20:51:11Z

~~~
toggleApproval false 'ign-pair-a-backup|335fe286-20e6-411b-a5e2-e96c8ba2cfdc' on gw-a -> HTTP 200
[exit 0]
~~~

#### C-P6 poll: 120 s, every 5 s, after the cut (api)

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh c6poll 120 5 ` @ 2026-10-19T20:51:11Z

~~~
# columns: UTC, elapsed, gw-a role-activityLevel, gw-a peerConnected, gw-b activityLevel, gw-b peerConnected, gw-b projectState, gw-a StatusPing, gw-b StatusPing
2026-10-19T20:51:11Z +  0s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:51:17Z +  5s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:51:21Z + 10s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:51:26Z + 15s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:51:32Z + 20s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:51:36Z + 25s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:51:43Z + 30s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:51:46Z + 35s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:51:51Z + 40s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:51:56Z + 45s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:52:02Z + 50s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:52:06Z + 55s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:52:11Z + 60s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:52:17Z + 65s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:52:21Z + 70s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:52:26Z + 75s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:52:32Z + 80s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:52:36Z + 85s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:52:41Z + 90s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:52:46Z + 95s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:52:51Z +100s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:52:56Z +105s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:53:01Z +110s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:53:06Z +115s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:53:11Z +120s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Active gwB_peerConnected=false gwB_projectState=Out gwA_StatusPing=200 gwB_StatusPing=200
SUMMARY gw-b reported Active after 0 s of the cut window: the cut backup promoted itself
SUMMARY gw-a first non-Active reading: never
[exit 0]
~~~

Result: gw-b reported `Active` during the cut window; the cut backup promoted itself (see the SUMMARY line above for the second).

Note on the poll above: the column `gwB_projectState=Out` is the first word only. The poll splits the status on spaces and the real value is `Out of Date` (shown in full in `### C-P4 during-cut-c-p6` below). The driver was changed afterwards to join the words, so later polls read `Out_of_Date`. The poll started right after the `cut` command returned, so gw-b was already `Active` at its first line: the promotion followed the revoked approval within the 2 to 5 seconds between the cut and the first reading, not after the 1200 missed pings the settings allow. The same promotion shows in C-P5: `gwB_peerConnected=false` at its first line and gw-b `activityLevel` `Active` in `### C-P4 after-cut`.

## C-P4 (continued: during-cut-c-p6)

### C-P4 during-cut-c-p6

#### GET redundancy on gw-a (during-cut-c-p6)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy'` @ 2026-10-19T20:53:11Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:53:11 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node0dzg0zr0gsp3031bbpe8kbx84.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 262

{"role":"Master","projectState":"Unknown","activityLevel":"Active","localId":"192.168.156.2","peerConnected":false,"peerId":"ign-pair-a-Backup","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":1596624,"lastSyncTimestamp":1792443050579}
[curl http=200 total=0.008838s]
~~~

#### GET redundancy/providers on gw-a (during-cut-c-p6)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy/providers'` @ 2026-10-19T20:53:11Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:53:11 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 3064

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1724726010},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":869840507},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":506277031},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1739771075},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1755787301},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":507131940},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1761965904},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":404777575},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":1,"queuedUpdatesPerSec":0.00613933835094635,"dispatchedUpdatesPerSec":0.00493916490145788,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1230460766},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":115588424},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":1,"queuedUpdatesPerSec":0.005524014663307267,"dispatchedUpdatesPerSec":0.004323683424619297,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1151268147}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.006581s]
~~~

#### gateway-info name and deploymentMode on gw-a (during-cut-c-p6)

`curl -sS -g [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/gateway-info' | jq -c '{name, deploymentMode}'` @ 2026-10-19T20:53:11Z

~~~
{"name":"ign-pair-a","deploymentMode":"production"}
~~~

#### GET redundancy on gw-b (during-cut-c-p6)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy'` @ 2026-10-19T20:53:11Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:53:11 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node08fsm6378zt5x11c6dmhkoixub646.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 265

{"role":"Backup","projectState":"Out of Date","activityLevel":"Active","localId":"192.168.156.3","peerConnected":false,"peerId":"ign-pair-a-Master","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":119702,"lastSyncTimestamp":1792443050592}
[curl http=200 total=0.005892s]
~~~

#### GET redundancy/providers on gw-b (during-cut-c-p6)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy/providers'` @ 2026-10-19T20:53:11Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:53:11 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 2999

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1829018932},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1506336707},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1897731064},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1479834663},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":294692648},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":238638110},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":388109554},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1812760437},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":2016531515},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1022158471},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":997122156}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.022302s]
~~~

#### gateway-info name and deploymentMode on gw-b (during-cut-c-p6)

`curl -sS -g [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/gateway-info' | jq -c '{name, deploymentMode}'` @ 2026-10-19T20:53:11Z

~~~
{"name":"ign-pair-a","deploymentMode":"production"}
~~~

#### restore the link, then wait for settled

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh c5link api ` @ 2026-10-19T20:53:11Z

~~~
toggleApproval true 'ign-pair-a-backup|335fe286-20e6-411b-a5e2-e96c8ba2cfdc' on gw-a -> HTTP 200
peerConnected true after 8 s
SUMMARY reconnect: peerConnected true 9 s and settled 9 s after the restore command started
[exit 0]
~~~

### C-P4 end-of-24-01

#### GET redundancy on gw-a (end-of-24-01)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy'` @ 2026-10-19T20:53:20Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:53:20 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node01sttuzwselb02jfrbu8cdhbsu86.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 258

{"role":"Master","projectState":"Good","activityLevel":"Active","localId":"192.168.156.2","peerConnected":true,"peerId":"ign-pair-a-Backup","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":1605788,"lastSyncTimestamp":1792443196968}
[curl http=200 total=0.008302s]
~~~

#### GET redundancy/providers on gw-a (end-of-24-01)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy/providers'` @ 2026-10-19T20:53:20Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:53:20 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 3031

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":177008455},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1675286422},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1008705751},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":2055386516},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":387550036},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":575313990},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1662160320},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":215198243},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":0,"queuedUpdatesPerSec":0.02163958103344313,"dispatchedUpdatesPerSec":0.004544251080519514,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1472971718},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":565570001},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":0,"queuedUpdatesPerSec":0.02107345591144167,"dispatchedUpdatesPerSec":0.003977980785446369,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1455295658}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.006375s]
~~~

#### gateway-info name and deploymentMode on gw-a (end-of-24-01)

`curl -sS -g [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/gateway-info' | jq -c '{name, deploymentMode}'` @ 2026-10-19T20:53:20Z

~~~
{"name":"ign-pair-a","deploymentMode":"production"}
~~~

#### GET redundancy on gw-b (end-of-24-01)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy'` @ 2026-10-19T20:53:20Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:53:20 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node04fx1ygazqix1ifbde94cudox650.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 251

{"role":"Backup","projectState":"Good","activityLevel":"Cold","localId":"192.168.156.3","peerConnected":true,"peerId":"ign-pair-a-Master","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":-1,"lastSyncTimestamp":1792443196981}
[curl http=200 total=0.008061s]
~~~

#### GET redundancy/providers on gw-b (end-of-24-01)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy/providers'` @ 2026-10-19T20:53:20Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:53:20 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 2974

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":788166832},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":770843786},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":846614912},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":661094991},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1512395243},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1724901457},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1723162457},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":2103172154},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":3739,"lastPullDuration":38,"lastApplyDuration":3,"lastSyncError":"","status":"GOOD","hashcode":218194035},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":336956642},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":3780,"lastPullDuration":477,"lastApplyDuration":94,"lastSyncError":"","status":"GOOD","hashcode":1486243502}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.012558s]
~~~

#### gateway-info name and deploymentMode on gw-b (end-of-24-01)

`curl -sS -g [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/gateway-info' | jq -c '{name, deploymentMode}'` @ 2026-10-19T20:53:20Z

~~~
{"name":"ign-pair-a","deploymentMode":"production"}
~~~

Which token answers which node: the HTTP status of `GET /data/api/v1/redundancy` per token and node (status codes only; A10: the spike reused one key on both nodes, the setup mints one per node).

| token | gw-a (`$BASE_A`) | gw-b (`$BASE_B`) |
| --- | --- | --- |
| `$IGN_PAIR_TOKEN_A` | 200 | 401 |
| `$IGN_PAIR_TOKEN_B` | 401 | 200 |

### C-P5 fallback (extra): gw-b disconnected from the gan network, same series as the API cut

Run in addition to the plan's conditional fallback because the toggleApproval cut dropped the link and promoted gw-b immediately: this shows whether a network-level cut, which only starves the pings, behaves differently under pingMaxMissed 1200.

#### cut the link, fallback: setup-pair.sh cut-net (docker network disconnect of gw-b from gan)

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/setup-pair.sh cut-net ` @ 2026-10-19T20:54:01Z

~~~
gw-b disconnected from ign-pair-rig_gan
[exit 0]
~~~

#### C-P5 fallback poll: 60 s, every 2 s, after docker network disconnect

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh c5poll 60 ` @ 2026-10-19T20:54:05Z

~~~
# columns: UTC, elapsed, gw-b peerConnected, gw-a peerConnected, gw-a StatusPing HTTP, gw-b StatusPing HTTP, gw-b GET redundancy HTTP, gw-b syncPending
2026-10-19T20:54:06Z +  0s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:07Z +  2s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:10Z +  4s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:11Z +  6s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:13Z +  8s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:15Z + 10s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:18Z + 12s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:19Z + 14s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:21Z + 16s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:23Z + 18s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:25Z + 20s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:28Z + 22s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:29Z + 24s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:32Z + 26s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:33Z + 28s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:35Z + 30s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:37Z + 32s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:40Z + 34s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:41Z + 36s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:43Z + 38s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:46Z + 40s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:47Z + 42s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:49Z + 44s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:51Z + 46s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:54Z + 49s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:55Z + 50s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:58Z + 52s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:54:59Z + 54s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:55:02Z + 56s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:55:03Z + 58s gwB_peerConnected=true gwA_peerConnected=true gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
2026-10-19T20:55:05Z + 60s gwB_peerConnected=true gwA_peerConnected=false gwA_StatusPing=200 gwB_StatusPing=200 gwB_redundancy_http=200 gwB_syncPending=false
SUMMARY first_gw-b_peerConnected_false=never (elapsed seconds; never = it stayed true for 60 s)
[exit 0]
~~~

#### C-P6 fallback poll: 120 s, every 5 s, activityLevel after docker network disconnect (continues the 60 s above)

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh c6poll 120 5 ` @ 2026-10-19T20:55:05Z

~~~
# columns: UTC, elapsed, gw-a role-activityLevel, gw-a peerConnected, gw-b activityLevel, gw-b peerConnected, gw-b projectState, gw-a StatusPing, gw-b StatusPing
2026-10-19T20:55:05Z +  0s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:55:11Z +  5s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:55:15Z + 10s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:55:20Z + 15s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:55:25Z + 20s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:55:30Z + 25s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:55:35Z + 30s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:55:41Z + 35s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:55:45Z + 40s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:55:50Z + 45s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:55:55Z + 50s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:56:01Z + 55s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:56:05Z + 60s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:56:10Z + 65s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:56:15Z + 70s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:56:21Z + 75s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:56:25Z + 80s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:56:30Z + 85s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:56:35Z + 90s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:56:41Z + 95s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:56:45Z +100s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:56:50Z +105s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:56:55Z +110s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:57:00Z +115s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
2026-10-19T20:57:05Z +120s gwA_activityLevel=Active gwA_peerConnected=false gwB_activityLevel=Cold gwB_peerConnected=true gwB_projectState=Good gwA_StatusPing=200 gwB_StatusPing=200
SUMMARY gw-b never reported Active in 120 s: the cut backup stayed Cold
SUMMARY gw-a first non-Active reading: never
[exit 0]
~~~

### C-P4 after-cut-net

#### GET redundancy on gw-a (after-cut-net)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy'` @ 2026-10-19T20:57:05Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:57:05 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node01vgs43k02x5ni17fy3r5sev0d3146.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 262

{"role":"Master","projectState":"Unknown","activityLevel":"Active","localId":"192.168.156.2","peerConnected":false,"peerId":"ign-pair-a-Backup","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":1831298,"lastSyncTimestamp":1792443196968}
[curl http=200 total=0.008088s]
~~~

#### GET redundancy/providers on gw-a (after-cut-net)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy/providers'` @ 2026-10-19T20:57:05Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:57:06 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 3066

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1839729784},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1535688822},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":137738510},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1241588402},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1743207494},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":2062670780},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1604968485},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":478043600},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":0,"queuedUpdatesPerSec":5.089141671750316E-4,"dispatchedUpdatesPerSec":9.243854822168706E-4,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":1118131989},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":914184390},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":0,"queuedUpdatesPerSec":4.95600180433099E-4,"dispatchedUpdatesPerSec":9.110680813315334E-4,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"UNKNOWN","hashcode":642072292}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.010673s]
~~~

#### gateway-info name and deploymentMode on gw-a (after-cut-net)

`curl -sS -g [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/gateway-info' | jq -c '{name, deploymentMode}'` @ 2026-10-19T20:57:06Z

~~~
{"name":"ign-pair-a","deploymentMode":"production"}
~~~

#### GET redundancy on gw-b (after-cut-net)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy'` @ 2026-10-19T20:57:06Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:57:06 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node0vvj550gvov041chwl2shefjf9741.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 251

{"role":"Backup","projectState":"Good","activityLevel":"Cold","localId":"192.168.147.3","peerConnected":true,"peerId":"ign-pair-a-Master","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":-1,"lastSyncTimestamp":1792443196981}
[curl http=200 total=0.007330s]
~~~

#### GET redundancy/providers on gw-b (after-cut-net)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy/providers'` @ 2026-10-19T20:57:06Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:57:06 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 2979

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":201889835},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1170550111},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1003750223},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":272026081},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1488441868},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":880063376},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":799084586},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":167689726},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":229179,"lastPullDuration":38,"lastApplyDuration":3,"lastSyncError":"","status":"GOOD","hashcode":1874803071},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":2118582059},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":229220,"lastPullDuration":477,"lastApplyDuration":94,"lastSyncError":"","status":"GOOD","hashcode":1505853577}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.007753s]
~~~

#### gateway-info name and deploymentMode on gw-b (after-cut-net)

`curl -sS -g [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/gateway-info' | jq -c '{name, deploymentMode}'` @ 2026-10-19T20:57:06Z

~~~
{"name":"ign-pair-a","deploymentMode":"production"}
~~~

#### restore the link: docker network connect and relink, then wait for settled

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh c5link net ` @ 2026-10-19T20:57:06Z

~~~
gw-b reconnected to ign-pair-rig_gan; peerConnected true after 0 s
SUMMARY reconnect: peerConnected true 1 s and settled 16 s after the restore command started
[exit 0]
~~~

### C-P4 after-reconnect-net

#### GET redundancy on gw-a (after-reconnect-net)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy'` @ 2026-10-19T20:57:22Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:57:22 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node015ykb8nc9io4t18edw6rv8tih7155.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 258

{"role":"Master","projectState":"Good","activityLevel":"Active","localId":"192.168.156.2","peerConnected":true,"peerId":"ign-pair-a-Backup","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":1847515,"lastSyncTimestamp":1792443196968}
[curl http=200 total=0.011854s]
~~~

#### GET redundancy/providers on gw-a (after-reconnect-net)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy/providers'` @ 2026-10-19T20:57:22Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:57:22 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 3033

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1443049938},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":2031431876},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1227548066},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1302386481},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":48423670},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1377919358},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1287166317},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":124438164},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":0,"queuedUpdatesPerSec":3.64652935175494E-4,"dispatchedUpdatesPerSec":6.623511410482386E-4,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":311058909},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1274665059},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":0,"queuedUpdatesPerSec":3.551130468063349E-4,"dispatchedUpdatesPerSec":6.528088063384314E-4,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1132614528}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.011052s]
~~~

#### gateway-info name and deploymentMode on gw-a (after-reconnect-net)

`curl -sS -g [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/gateway-info' | jq -c '{name, deploymentMode}'` @ 2026-10-19T20:57:22Z

~~~
{"name":"ign-pair-a","deploymentMode":"production"}
~~~

#### GET redundancy on gw-b (after-reconnect-net)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy'` @ 2026-10-19T20:57:22Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:57:22 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node0gpsu5ayw0anknq9k0ie737ix751.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 251

{"role":"Backup","projectState":"Good","activityLevel":"Cold","localId":"192.168.147.3","peerConnected":true,"peerId":"ign-pair-a-Master","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":-1,"lastSyncTimestamp":1792443441211}
[curl http=200 total=0.004014s]
~~~

#### GET redundancy/providers on gw-b (after-reconnect-net)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy/providers'` @ 2026-10-19T20:57:22Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:57:22 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 2977

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1107741072},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":822287628},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":190508472},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1841771291},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":435764602},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":809216052},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1825617062},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":953884108},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":245335,"lastPullDuration":38,"lastApplyDuration":3,"lastSyncError":"","status":"GOOD","hashcode":1746531648},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":93227647},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":245376,"lastPullDuration":477,"lastApplyDuration":94,"lastSyncError":"","status":"GOOD","hashcode":1821535212}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.003668s]
~~~

#### gateway-info name and deploymentMode on gw-b (after-reconnect-net)

`curl -sS -g [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/gateway-info' | jq -c '{name, deploymentMode}'` @ 2026-10-19T20:57:22Z

~~~
{"name":"ign-pair-a","deploymentMode":"production"}
~~~

### C-P4 end-of-24-01-final

#### GET redundancy on gw-a (end-of-24-01-final)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy'` @ 2026-10-19T20:57:31Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:57:31 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node06p6p2s70qolvz9b3t6mj0fda157.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 258

{"role":"Master","projectState":"Good","activityLevel":"Active","localId":"192.168.156.2","peerConnected":true,"peerId":"ign-pair-a-Backup","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":1856557,"lastSyncTimestamp":1792443196968}
[curl http=200 total=0.025602s]
~~~

#### GET redundancy/providers on gw-a (end-of-24-01-final)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy/providers'` @ 2026-10-19T20:57:31Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:57:31 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 3033

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1801207856},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":141274780},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":550069560},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":891384135},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1022753299},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1765780575},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":37024997},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1008801453},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":0,"queuedUpdatesPerSec":3.0867204555377745E-4,"dispatchedUpdatesPerSec":5.606681363577794E-4,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":798443096},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1004423841},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":0,"queuedUpdatesPerSec":3.0059670439178777E-4,"dispatchedUpdatesPerSec":5.525907244131384E-4,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":518521571}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.009197s]
~~~

#### gateway-info name and deploymentMode on gw-a (end-of-24-01-final)

`curl -sS -g [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/gateway-info' | jq -c '{name, deploymentMode}'` @ 2026-10-19T20:57:31Z

~~~
{"name":"ign-pair-a","deploymentMode":"production"}
~~~

#### GET redundancy on gw-b (end-of-24-01-final)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy'` @ 2026-10-19T20:57:31Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:57:31 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node0yjaxj2a5o3ddlta4wmanm3zz753.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 251

{"role":"Backup","projectState":"Good","activityLevel":"Cold","localId":"192.168.147.3","peerConnected":true,"peerId":"ign-pair-a-Master","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":-1,"lastSyncTimestamp":1792443441211}
[curl http=200 total=0.016873s]
~~~

#### GET redundancy/providers on gw-b (end-of-24-01-final)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy/providers'` @ 2026-10-19T20:57:31Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 20:57:31 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 2978

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":937987985},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1585175814},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":415530095},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1312798831},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1029186110},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1028344702},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":114782140},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":899171273},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":254413,"lastPullDuration":38,"lastApplyDuration":3,"lastSyncError":"","status":"GOOD","hashcode":1236131478},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":786070630},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":254454,"lastPullDuration":477,"lastApplyDuration":94,"lastSyncError":"","status":"GOOD","hashcode":323837070}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.006767s]
~~~

#### gateway-info name and deploymentMode on gw-b (end-of-24-01-final)

`curl -sS -g [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/gateway-info' | jq -c '{name, deploymentMode}'` @ 2026-10-19T20:57:31Z

~~~
{"name":"ign-pair-a","deploymentMode":"production"}
~~~

## C-P7

Secret listings, names only: `GET /data/api/v1/secret-providers/<provider>/secrets` on both nodes (gw-a `$BASE_A`, gw-b `$BASE_B`). The throwaway secret value lives only inside `setup-pair.sh` and is written into a file inside the container through stdin; it is never printed, so no capture below can hold it. Provider config is recorded through a jq projection of name and type only.

### C-P7 provider inventory and the candidate built-in names (before any capture provider)

#### GET resources/list/ignition/secret-provider on gw-a (name and type only)

`curl -sS -g [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/resources/list/ignition/secret-provider' | jq -c '[.items[] | {name, type: (.config.profile.type // .config.type)}]'` @ 2026-10-19T21:03:57Z

~~~
[]
~~~

#### GET resources/list/ignition/secret-provider on gw-b (name and type only)

`curl -sS -g [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/resources/list/ignition/secret-provider' | jq -c '[.items[] | {name, type: (.config.profile.type // .config.type)}]'` @ 2026-10-19T21:03:57Z

~~~
[]
~~~

#### GET secret-providers/default/secrets on gw-a (candidate built-in name)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/secret-providers/default/secrets'` @ 2026-10-19T21:03:57Z

~~~
HTTP/1.1 404 Not Found
Date: Mon, 19 Oct 2026 21:03:57 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Pragma: no-cache
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 354

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 Not Found</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/secret-providers/default/secrets</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>Not Found</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.009210s]
~~~

#### GET secret-providers/default/secrets on gw-b (candidate built-in name)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/secret-providers/default/secrets'` @ 2026-10-19T21:03:57Z

~~~
HTTP/1.1 404 Not Found
Date: Mon, 19 Oct 2026 21:03:57 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Pragma: no-cache
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 354

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 Not Found</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/secret-providers/default/secrets</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>Not Found</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.010750s]
~~~

#### GET secret-providers/internal/secrets on gw-a (candidate built-in name)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/secret-providers/internal/secrets'` @ 2026-10-19T21:03:58Z

~~~
HTTP/1.1 404 Not Found
Date: Mon, 19 Oct 2026 21:03:58 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Pragma: no-cache
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 355

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 Not Found</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/secret-providers/internal/secrets</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>Not Found</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.011069s]
~~~

#### GET secret-providers/internal/secrets on gw-b (candidate built-in name)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/secret-providers/internal/secrets'` @ 2026-10-19T21:03:58Z

~~~
HTTP/1.1 404 Not Found
Date: Mon, 19 Oct 2026 21:03:58 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Pragma: no-cache
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 355

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 Not Found</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/secret-providers/internal/secrets</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>Not Found</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.004344s]
~~~

### C-P7 unknown provider nope-24

#### GET secret-providers/nope-24/secrets on gw-a

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/secret-providers/nope-24/secrets'` @ 2026-10-19T21:03:58Z

~~~
HTTP/1.1 404 Not Found
Date: Mon, 19 Oct 2026 21:03:58 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Pragma: no-cache
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 354

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 Not Found</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/secret-providers/nope-24/secrets</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>Not Found</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.009605s]
~~~

#### GET secret-providers/nope-24/secrets on gw-b

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/secret-providers/nope-24/secrets'` @ 2026-10-19T21:03:58Z

~~~
HTTP/1.1 404 Not Found
Date: Mon, 19 Oct 2026 21:03:58 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Pragma: no-cache
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 354

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 Not Found</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/secret-providers/nope-24/secrets</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>Not Found</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.014317s]
~~~

### C-P7 providers created on gw-a for the capture

#### setup-pair.sh provider a cap24-file file (empty file provider)

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/setup-pair.sh provider a cap24-file file ` @ 2026-10-19T21:03:58Z

~~~
created provider cap24-file (file) on gw-a
[exit 0]
~~~

#### setup-pair.sh provider a cap24-int internal (internal provider, no secrets)

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/setup-pair.sh provider a cap24-int internal ` @ 2026-10-19T21:03:58Z

~~~
created provider cap24-int (internal) on gw-a
[exit 0]
~~~

#### wait for gw-b to list both providers

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh spwait cap24-file cap24-int ` @ 2026-10-19T21:03:58Z

~~~
# columns: UTC, elapsed, find status on gw-b per provider name
2026-10-19T21:03:58Z +  0s cap24-file=404 cap24-int=404
2026-10-19T21:04:01Z +  3s cap24-file=200 cap24-int=200
SUMMARY reached gw-b after 3 s
[exit 0]
~~~

#### GET resources/list/ignition/secret-provider on gw-a (name and type only, after creating the capture providers)

`curl -sS -g [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/resources/list/ignition/secret-provider' | jq -c '[.items[] | {name, type: (.config.profile.type // .config.type)}]'` @ 2026-10-19T21:04:01Z

~~~
[{"name":"cap24-file","type":"file"},{"name":"cap24-int","type":"internal"}]
~~~

#### GET resources/list/ignition/secret-provider on gw-b (name and type only, after creating the capture providers)

`curl -sS -g [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/resources/list/ignition/secret-provider' | jq -c '[.items[] | {name, type: (.config.profile.type // .config.type)}]'` @ 2026-10-19T21:04:01Z

~~~
[{"name":"cap24-file","type":"file"},{"name":"cap24-int","type":"internal"}]
~~~

#### GET secret-providers/cap24-file/secrets on gw-a (no secret created yet)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/secret-providers/cap24-file/secrets'` @ 2026-10-19T21:04:01Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:04:01 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Transfer-Encoding: chunked

{"secrets":[]}
[curl http=200 total=0.025455s]
~~~

#### GET secret-providers/cap24-file/secrets on gw-b (no secret created yet)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/secret-providers/cap24-file/secrets'` @ 2026-10-19T21:04:01Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:04:01 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Transfer-Encoding: chunked

{"secrets":[]}
[curl http=200 total=0.014791s]
~~~

#### GET secret-providers/cap24-int/secrets on gw-a (no secret created yet)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/secret-providers/cap24-int/secrets'` @ 2026-10-19T21:04:01Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:04:01 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Transfer-Encoding: chunked

{"secrets":[]}
[curl http=200 total=0.018359s]
~~~

#### GET secret-providers/cap24-int/secrets on gw-b (no secret created yet)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/secret-providers/cap24-int/secrets'` @ 2026-10-19T21:04:01Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:04:01 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Transfer-Encoding: chunked

{"secrets":[]}
[curl http=200 total=0.016660s]
~~~

### C-P7 calls that cannot read the listing: no token, a wrong key, a token without the permission

#### GET secret-providers/cap24-file/secrets on gw-a (no token header)

`curl -sS -g -i -X GET [no token] '$BASE_A/data/api/v1/secret-providers/cap24-file/secrets'` @ 2026-10-19T21:04:01Z

~~~
HTTP/1.1 401 Unauthorized
Date: Mon, 19 Oct 2026 21:04:01 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Pragma: no-cache
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 363

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 401</title>
</head>
<body><h2>HTTP ERROR 401 Unauthorized</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/secret-providers/cap24-file/secrets</td></tr>
<tr><th>STATUS:</th><td>401</td></tr>
<tr><th>MESSAGE:</th><td>Unauthorized</td></tr>
</table>

</body>
</html>

[curl http=401 total=0.011840s]
~~~

#### GET secret-providers/cap24-file/secrets on gw-a (right token name, wrong key)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_WRONG] '$BASE_A/data/api/v1/secret-providers/cap24-file/secrets'` @ 2026-10-19T21:04:01Z

~~~
HTTP/1.1 401 Unauthorized
Date: Mon, 19 Oct 2026 21:04:01 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Pragma: no-cache
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 363

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 401</title>
</head>
<body><h2>HTTP ERROR 401 Unauthorized</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/secret-providers/cap24-file/secrets</td></tr>
<tr><th>STATUS:</th><td>401</td></tr>
<tr><th>MESSAGE:</th><td>Unauthorized</td></tr>
</table>

</body>
</html>

[curl http=401 total=0.005352s]
~~~

#### POST resources/ignition/api-token (a token holding only Authenticated, no Deploy level; the key is generated here and never printed)

`curl -sS -g -i -X POST [token $IGN_PAIR_TOKEN_A] -H Content-Type: application/json --data [{"name":"cap24-noperm","config":{"profile":{"type":"basic-token","secureChannelRequired":false,"securityLevels":[{"name":"Authenticated","children":[]}],"timestamp":1792443841000},"settings":{"tokenHash":"<redacted>"}}}] '$BASE_A/data/api/v1/resources/ignition/api-token'` @ 2026-10-19T21:04:01Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:04:01 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 255

{
  "success": true,
  "changes": [
    {
      "name": "cap24-noperm",
      "type": "ignition/api-token",
      "collection": "core",
      "newSignature": "93ac43f74be1ae71d547d87ef2691a8759bfa7da8a735c80f172e9ab1bf6aeec"
    }
  ],
  "problem": null
}
[curl http=200 total=0.053646s]
~~~

#### GET secret-providers/cap24-file/secrets on gw-a (token without the Deploy level)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_NOPERM] '$BASE_A/data/api/v1/secret-providers/cap24-file/secrets'` @ 2026-10-19T21:04:06Z

~~~
HTTP/1.1 403 Forbidden
Date: Mon, 19 Oct 2026 21:04:06 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Pragma: no-cache
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 357

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 403</title>
</head>
<body><h2>HTTP ERROR 403 Forbidden</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/secret-providers/cap24-file/secrets</td></tr>
<tr><th>STATUS:</th><td>403</td></tr>
<tr><th>MESSAGE:</th><td>Forbidden</td></tr>
</table>

</body>
</html>

[curl http=403 total=0.020554s]
~~~

#### GET resources/list/ignition/secret-provider on gw-a (token without the Deploy level)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_NOPERM] '$BASE_A/data/api/v1/resources/list/ignition/secret-provider'` @ 2026-10-19T21:04:06Z

~~~
HTTP/1.1 403 Forbidden
Date: Mon, 19 Oct 2026 21:04:06 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 361

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 403</title>
</head>
<body><h2>HTTP ERROR 403 Forbidden</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/resources/list/ignition/secret-provider</td></tr>
<tr><th>STATUS:</th><td>403</td></tr>
<tr><th>MESSAGE:</th><td>Forbidden</td></tr>
</table>

</body>
</html>

[curl http=403 total=0.018268s]
~~~

### C-P7 a secret created on gw-a: does the name reach gw-b, and is the case kept

#### setup-pair.sh secrets a cap24-file CAP24-Name

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/setup-pair.sh secrets a cap24-file CAP24-Name ` @ 2026-10-19T21:04:06Z

~~~
created CAP24-Name in cap24-file on gw-a
[exit 0]
~~~

#### C-P7 poll: 60 s, every 5 s, cap24-file listing on both nodes

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh c7poll 60 5 cap24-file ` @ 2026-10-19T21:04:07Z

~~~
# columns: UTC, elapsed, then per provider the HTTP status and the secrets array on gw-a and gw-b (names only)
2026-10-19T21:04:07Z +  0s cap24-file: gwA=200:["CAP24-Name"] gwB=200:[]
2026-10-19T21:04:12Z +  5s cap24-file: gwA=200:["CAP24-Name"] gwB=200:["CAP24-Name"]
2026-10-19T21:04:17Z + 10s cap24-file: gwA=200:["CAP24-Name"] gwB=200:["CAP24-Name"]
2026-10-19T21:04:22Z + 15s cap24-file: gwA=200:["CAP24-Name"] gwB=200:["CAP24-Name"]
2026-10-19T21:04:27Z + 20s cap24-file: gwA=200:["CAP24-Name"] gwB=200:["CAP24-Name"]
2026-10-19T21:04:32Z + 25s cap24-file: gwA=200:["CAP24-Name"] gwB=200:["CAP24-Name"]
2026-10-19T21:04:37Z + 30s cap24-file: gwA=200:["CAP24-Name"] gwB=200:["CAP24-Name"]
2026-10-19T21:04:42Z + 35s cap24-file: gwA=200:["CAP24-Name"] gwB=200:["CAP24-Name"]
2026-10-19T21:04:47Z + 40s cap24-file: gwA=200:["CAP24-Name"] gwB=200:["CAP24-Name"]
2026-10-19T21:04:52Z + 45s cap24-file: gwA=200:["CAP24-Name"] gwB=200:["CAP24-Name"]
2026-10-19T21:04:58Z + 51s cap24-file: gwA=200:["CAP24-Name"] gwB=200:["CAP24-Name"]
2026-10-19T21:05:02Z + 55s cap24-file: gwA=200:["CAP24-Name"] gwB=200:["CAP24-Name"]
2026-10-19T21:05:07Z + 60s cap24-file: gwA=200:["CAP24-Name"] gwB=200:["CAP24-Name"]
[exit 0]
~~~

#### setup-pair.sh secrets a cap24-file cap24-name (same name in lower case)

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/setup-pair.sh secrets a cap24-file cap24-name ` @ 2026-10-19T21:05:07Z

~~~
created cap24-name in cap24-file on gw-a
[exit 0]
~~~

#### C-P7 poll: 30 s, every 5 s, cap24-file listing on both nodes after the second name

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh c7poll 30 5 cap24-file ` @ 2026-10-19T21:05:08Z

~~~
# columns: UTC, elapsed, then per provider the HTTP status and the secrets array on gw-a and gw-b (names only)
2026-10-19T21:05:08Z +  0s cap24-file: gwA=200:["CAP24-Name","cap24-name"] gwB=200:["CAP24-Name"]
2026-10-19T21:05:13Z +  5s cap24-file: gwA=200:["CAP24-Name","cap24-name"] gwB=200:["CAP24-Name","cap24-name"]
2026-10-19T21:05:18Z + 10s cap24-file: gwA=200:["CAP24-Name","cap24-name"] gwB=200:["CAP24-Name","cap24-name"]
2026-10-19T21:05:23Z + 15s cap24-file: gwA=200:["CAP24-Name","cap24-name"] gwB=200:["CAP24-Name","cap24-name"]
2026-10-19T21:05:28Z + 20s cap24-file: gwA=200:["CAP24-Name","cap24-name"] gwB=200:["CAP24-Name","cap24-name"]
2026-10-19T21:05:33Z + 25s cap24-file: gwA=200:["CAP24-Name","cap24-name"] gwB=200:["CAP24-Name","cap24-name"]
2026-10-19T21:05:38Z + 30s cap24-file: gwA=200:["CAP24-Name","cap24-name"] gwB=200:["CAP24-Name","cap24-name"]
[exit 0]
~~~

#### GET resources/list/ignition/secret-provider on gw-a (provider name, type and secret NAMES only)

`curl -sS -g [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/resources/list/ignition/secret-provider' | jq -c '[.items[] | {name, type: (.config.profile.type // .config.type), secret_names: ((.config.settings.files // {}) | keys)}]'` @ 2026-10-19T21:05:38Z

~~~
[{"name":"cap24-file","type":"file","secret_names":["CAP24-Name","cap24-name"]},{"name":"cap24-int","type":"internal","secret_names":[]}]
~~~

#### GET resources/list/ignition/secret-provider on gw-b (provider name, type and secret NAMES only)

`curl -sS -g [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/resources/list/ignition/secret-provider' | jq -c '[.items[] | {name, type: (.config.profile.type // .config.type), secret_names: ((.config.settings.files // {}) | keys)}]'` @ 2026-10-19T21:05:38Z

~~~
[{"name":"cap24-file","type":"file","secret_names":["CAP24-Name","cap24-name"]},{"name":"cap24-int","type":"internal","secret_names":[]}]
~~~

#### GET secret-providers/cap24-file/secrets on gw-a (final)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/secret-providers/cap24-file/secrets'` @ 2026-10-19T21:05:38Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:05:38 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Transfer-Encoding: chunked

{"secrets":["CAP24-Name","cap24-name"]}
[curl http=200 total=0.005031s]
~~~

#### GET secret-providers/cap24-file/secrets on gw-b (final)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/secret-providers/cap24-file/secrets'` @ 2026-10-19T21:05:39Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:05:39 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Transfer-Encoding: chunked

{"secrets":["CAP24-Name","cap24-name"]}
[curl http=200 total=0.021592s]
~~~

### C-P7 cleanup

Cleanup after C-P7: the two capture providers and the extra token are deleted from gw-a (with the signature find answers), the throwaway files are removed from gw-a's container, and gw-b is waited on.

#### GET find ignition/secret-provider/cap24-file (read the signature)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/resources/find/ignition/secret-provider/cap24-file'` @ 2026-10-19T21:05:39Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:05:39 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 990

{
  "type": "ignition/secret-provider",
  "name": "cap24-file",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "ab9219451ad85f48817fac526fe46ab079cd263f34579db2420195f86870818e",
  "config": {
    "profile": {
      "type": "file"
    },
    "settings": {
      "files": {
        "CAP24-Name": {
          "filePath": "/tmp/CAP24-Name",
          "fileType": "CLEARTEXT"
        },
        "cap24-name": {
          "filePath": "/tmp/cap24-name",
          "fileType": "CLEARTEXT"
        }
      }
    }
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "lastModification": {
      "actor": "api-token:pair-a:192.168.147.1",
      "timestamp": "2026-10-19T21:05:07Z"
    },
    "uuid": "d89304a4-5943-47b9-a7ab-7f12ec2d1162",
    "lastModificationSignature": "25f2da468b99c91bc0c3d90c2e1bc3f12adaf787f0166189a63c514886158402",
    "enabled": true
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.013265s]
~~~

#### DELETE resources/ignition/secret-provider/cap24-file/<signature>

`curl -sS -g -i -X DELETE [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/resources/ignition/secret-provider/cap24-file/ab9219451ad85f48817fac526fe46ab079cd263f34579db2420195f86870818e'` @ 2026-10-19T21:05:39Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:05:39 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 279

{
  "success": true,
  "changes": [
    {
      "name": "cap24-file",
      "type": "ignition/secret-provider",
      "collection": "core",
      "newSignature": "ab9219451ad85f48817fac526fe46ab079cd263f34579db2420195f86870818e"
    }
  ],
  "problem": null,
  "references": []
}
[curl http=200 total=0.062537s]
~~~

#### GET find ignition/secret-provider/cap24-int (read the signature)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/resources/find/ignition/secret-provider/cap24-int'` @ 2026-10-19T21:05:39Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:05:39 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 544

{
  "type": "ignition/secret-provider",
  "name": "cap24-int",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "95eb2260d930092c833468b875bbbb2a8fb4b2e252131d98e84302f1f922535a",
  "config": {
    "profile": {
      "type": "internal"
    },
    "settings": {
      "secrets": {}
    }
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "uuid": "7ed3c0cc-4575-4238-8d95-7943d1b4a456",
    "enabled": true
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.005649s]
~~~

#### DELETE resources/ignition/secret-provider/cap24-int/<signature>

`curl -sS -g -i -X DELETE [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/resources/ignition/secret-provider/cap24-int/95eb2260d930092c833468b875bbbb2a8fb4b2e252131d98e84302f1f922535a'` @ 2026-10-19T21:05:39Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:05:39 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 278

{
  "success": true,
  "changes": [
    {
      "name": "cap24-int",
      "type": "ignition/secret-provider",
      "collection": "core",
      "newSignature": "95eb2260d930092c833468b875bbbb2a8fb4b2e252131d98e84302f1f922535a"
    }
  ],
  "problem": null,
  "references": []
}
[curl http=200 total=0.019374s]
~~~

#### GET find ignition/api-token/cap24-noperm (read the signature)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/resources/find/ignition/api-token/cap24-noperm'` @ 2026-10-19T21:05:39Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:05:39 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 776

{
  "type": "ignition/api-token",
  "name": "cap24-noperm",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "93ac43f74be1ae71d547d87ef2691a8759bfa7da8a735c80f172e9ab1bf6aeec",
  "config": {
    "profile": {
      "type": "basic-token",
      "secureChannelRequired": false,
      "securityLevels": [
        {
          "name": "Authenticated",
          "children": []
        }
      ],
      "timestamp": 1792443841000
    },
    "settings": {
      "tokenHash": "<redacted>"
    }
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "uuid": "e9388527-1a2f-493f-84e0-1995eb43db48",
    "enabled": true
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.007890s]
~~~

#### DELETE resources/ignition/api-token/cap24-noperm/<signature>

`curl -sS -g -i -X DELETE [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/resources/ignition/api-token/cap24-noperm/93ac43f74be1ae71d547d87ef2691a8759bfa7da8a735c80f172e9ab1bf6aeec'` @ 2026-10-19T21:05:39Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:05:39 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 275

{
  "success": true,
  "changes": [
    {
      "name": "cap24-noperm",
      "type": "ignition/api-token",
      "collection": "core",
      "newSignature": "93ac43f74be1ae71d547d87ef2691a8759bfa7da8a735c80f172e9ab1bf6aeec"
    }
  ],
  "problem": null,
  "references": []
}
[curl http=200 total=0.024939s]
~~~

#### remove the throwaway secret files from gw-a's container

`docker exec -u root 8d60abffd2c8 sh -c rm\ -f\ /tmp/CAP24-Name\ /tmp/cap24-name ` @ 2026-10-19T21:05:39Z

~~~
[exit 0]
~~~

#### wait for gw-b to drop both providers (every 2 s, up to 90 s)

`bash -c $'\n    t0=$(date +%s)\n    while :; do\n      n=0\n      for p in cap24-file cap24-int; do\n        c=$(curl -sS -g -m 8 -o /dev/null -w "%{http_code}" -H "X-Ignition-API-Token: ${IGN_PAIR_TOKEN_B}" "$BASE_B/data/api/v1/resources/find/ignition/secret-provider/$p" 2>/dev/null) || c=000\n        [ "$c" = 404 ] || n=$((n+1))\n      done\n      echo "$(date -u +%FT%TZ) +$(( $(date +%s) - t0 ))s still_present=$n"\n      [ "$n" = 0 ] && break\n      [ $(( $(date +%s) - t0 )) -lt 90 ] || break\n      sleep 2\n    done' ` @ 2026-10-19T21:05:39Z

~~~
2026-10-19T21:05:40Z +1s still_present=2
2026-10-19T21:05:42Z +3s still_present=0
[exit 0]
~~~

#### GET resources/list/ignition/secret-provider on gw-b (after cleanup)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/resources/list/ignition/secret-provider'` @ 2026-10-19T21:05:42Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:05:42 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 70

{"items":[],"metadata":{"total":0,"matching":0,"limit":-1,"offset":0}}
[curl http=200 total=0.015201s]
~~~

### C-P7 remote provider (candidate for a 501 listing)

The provider types the gateway offers are `file`, `internal` and `remote` (`GET /data/api/v1/resources/type/ignition/secret-provider`). A `remote` provider links to a secret provider on another gateway through the gateway network, so it is the one candidate for a provider that cannot list its secrets. Created on gw-a pointing at the peer `ign-pair-a-Backup`, provider `cap24-file`.

#### setup-pair.sh provider a cap24-remote remote

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/setup-pair.sh provider a cap24-remote remote ` @ 2026-10-19T21:06:42Z

~~~
created provider cap24-remote (remote) on gw-a
[exit 0]
~~~

#### wait for gw-b to list the provider

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh spwait cap24-remote ` @ 2026-10-19T21:06:43Z

~~~
# columns: UTC, elapsed, find status on gw-b per provider name
2026-10-19T21:06:43Z +  0s cap24-remote=404
2026-10-19T21:06:45Z +  2s cap24-remote=200
SUMMARY reached gw-b after 2 s
[exit 0]
~~~

#### GET secret-providers/cap24-remote/secrets on gw-a

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/secret-providers/cap24-remote/secrets'` @ 2026-10-19T21:06:45Z

~~~
HTTP/1.1 500 Server Error
Date: Mon, 19 Oct 2026 21:06:45 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Pragma: no-cache
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 365

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 500</title>
</head>
<body><h2>HTTP ERROR 500 Server Error</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/secret-providers/cap24-remote/secrets</td></tr>
<tr><th>STATUS:</th><td>500</td></tr>
<tr><th>MESSAGE:</th><td>Server Error</td></tr>
</table>

</body>
</html>

[curl http=500 total=0.017410s]
~~~

#### GET secret-providers/cap24-remote/secrets on gw-b

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/secret-providers/cap24-remote/secrets'` @ 2026-10-19T21:06:45Z

~~~
HTTP/1.1 500 Server Error
Date: Mon, 19 Oct 2026 21:06:45 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Pragma: no-cache
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 365

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 500</title>
</head>
<body><h2>HTTP ERROR 500 Server Error</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/secret-providers/cap24-remote/secrets</td></tr>
<tr><th>STATUS:</th><td>500</td></tr>
<tr><th>MESSAGE:</th><td>Server Error</td></tr>
</table>

</body>
</html>

[curl http=500 total=0.008478s]
~~~

#### GET find ignition/secret-provider/cap24-remote (read the signature)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/resources/find/ignition/secret-provider/cap24-remote'` @ 2026-10-19T21:06:45Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:06:45 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 605

{
  "type": "ignition/secret-provider",
  "name": "cap24-remote",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "22de708610d5d9eb77243d5e5efd0abf241e41be81a4721d9906e56fa2e9535c",
  "config": {
    "profile": {
      "type": "remote"
    },
    "settings": {
      "targetServer": "ign-pair-a-Backup",
      "targetProvider": "cap24-file"
    }
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "uuid": "5bd33b20-d20d-4d09-985c-7ff9d34fe449",
    "enabled": true
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.012254s]
~~~

#### DELETE resources/ignition/secret-provider/cap24-remote/<signature>

`curl -sS -g -i -X DELETE [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/resources/ignition/secret-provider/cap24-remote/22de708610d5d9eb77243d5e5efd0abf241e41be81a4721d9906e56fa2e9535c'` @ 2026-10-19T21:06:45Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:06:45 GMT
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
      "name": "cap24-remote",
      "type": "ignition/secret-provider",
      "collection": "core",
      "newSignature": "22de708610d5d9eb77243d5e5efd0abf241e41be81a4721d9906e56fa2e9535c"
    }
  ],
  "problem": null,
  "references": []
}
[curl http=200 total=0.021021s]
~~~

#### wait for gw-b to drop the provider

`bash -c $'\n    t0=$(date +%s)\n    while :; do\n      c=$(curl -sS -g -m 8 -o /dev/null -w "%{http_code}" -H "X-Ignition-API-Token: ${IGN_PAIR_TOKEN_B}" "$BASE_B/data/api/v1/resources/find/ignition/secret-provider/cap24-remote" 2>/dev/null) || c=000\n      echo "$(date -u +%FT%TZ) +$(( $(date +%s) - t0 ))s find=$c"\n      [ "$c" = 404 ] && break\n      [ $(( $(date +%s) - t0 )) -lt 90 ] || break\n      sleep 2\n    done' ` @ 2026-10-19T21:06:45Z

~~~
2026-10-19T21:06:45Z +0s find=200
2026-10-19T21:06:47Z +2s find=404
[exit 0]
~~~

## C-P8

TEC-5105 case (backup project-sync failure): this state feeds its own finding, reason `sync_exhausted` (D-09, D-10), and never triggers a master rollback or restore (ROADMAP Phase 24 constraint). Every K6 option keeps that rule; the open question is only whether resync clears the state.

Time box 20 minutes, started 2026-10-19T21:07:14Z (UTC). Path (the spike RIG.md path): gw-b runs without a deployment mode while gw-a runs `production`; expected log chain on gw-b: `Active deployment mode differs from peer node!`, then `[config-resource] After applying diff, new local system snapshot ... did not match remote system snapshot`, then `Sync operation has failed 5 time(s), which exceeds the max attempt count`. Only GETs go to gw-b; gw-b is changed only by recreating its container.

## C-P4 (continued: before-exhaust)

### C-P4 before-exhaust

#### GET redundancy on gw-a (before-exhaust)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy'` @ 2026-10-19T21:07:14Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:07:14 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node010ul3z9qz1c7j1pkfjtjjrgw8b162.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 258

{"role":"Master","projectState":"Good","activityLevel":"Active","localId":"192.168.156.2","peerConnected":true,"peerId":"ign-pair-a-Backup","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":2440249,"lastSyncTimestamp":1792444006715}
[curl http=200 total=0.016681s]
~~~

#### GET redundancy/providers on gw-a (before-exhaust)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy/providers'` @ 2026-10-19T21:07:14Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:07:14 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 3028

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1948533259},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1880868951},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":890849328},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1376005935},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":73800574},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":714154709},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1429649279},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":649627553},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":0,"queuedUpdatesPerSec":0.036828970895152434,"dispatchedUpdatesPerSec":0.03774512883890971,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":313644515},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":831900394},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":0,"queuedUpdatesPerSec":0.0368289703834931,"dispatchedUpdatesPerSec":0.037745128327119175,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1366822632}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.011192s]
~~~

#### gateway-info name and deploymentMode on gw-a (before-exhaust)

`curl -sS -g [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/gateway-info' | jq -c '{name, deploymentMode}'` @ 2026-10-19T21:07:14Z

~~~
{"name":"ign-pair-a","deploymentMode":"production"}
~~~

#### GET redundancy on gw-b (before-exhaust)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy'` @ 2026-10-19T21:07:15Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:07:15 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node0cs8564xcmyesqu11ndsrr8x3758.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 251

{"role":"Backup","projectState":"Good","activityLevel":"Cold","localId":"192.168.147.3","peerConnected":true,"peerId":"ign-pair-a-Master","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":-1,"lastSyncTimestamp":1792444006723}
[curl http=200 total=0.015371s]
~~~

#### GET redundancy/providers on gw-b (before-exhaust)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy/providers'` @ 2026-10-19T21:07:15Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:07:15 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 2974

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":695084089},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":340511639},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1150566721},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1928890896},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":720316314},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1613907491},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":105549101},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":810823678},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":28442,"lastPullDuration":20,"lastApplyDuration":2,"lastSyncError":"","status":"GOOD","hashcode":1674845593},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":296049822},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":28464,"lastPullDuration":61,"lastApplyDuration":9,"lastSyncError":"","status":"GOOD","hashcode":1138002018}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.067008s]
~~~

#### gateway-info name and deploymentMode on gw-b (before-exhaust)

`curl -sS -g [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/gateway-info' | jq -c '{name, deploymentMode}'` @ 2026-10-19T21:07:15Z

~~~
{"name":"ign-pair-a","deploymentMode":"production"}
~~~

#### recreate gw-b with no deployment mode (IGN_PAIR_MODE_B=-Dign.pair.rig.noop=true)

`env IGN_PAIR_MODE_B=-Dign.pair.rig.noop=true docker compose --project-directory /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair -p ign-pair-rig -f /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/compose.yml up -d gw-b ` @ 2026-10-19T21:07:15Z

~~~
 Container ign-pair-rig-gw-b-1 Recreate 
 Container ign-pair-rig-gw-b-1 Recreated 
 Container ign-pair-rig-gw-b-1 Starting 
 Container ign-pair-rig-gw-b-1 Started 
[exit 0]
~~~

#### relink: setup-pair.sh link (toggleApproval true on gw-a, trust loop until gw-b peerConnected)

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/setup-pair.sh link ` @ 2026-10-19T21:07:29Z

~~~
toggleApproval true 'ign-pair-a-backup|335fe286-20e6-411b-a5e2-e96c8ba2cfdc' on gw-a -> HTTP 200
peerConnected true after 69 s
[exit 0]
~~~

#### C-P8 poll: gw-b state every 15 s until the max-attempt line or 20 minutes since 2026-10-19T21:07:14Z

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh c8poll 1200 15 2026-10-19T21:07:15Z stop ` @ 2026-10-19T21:08:39Z

~~~
# columns: UTC, elapsed, gw-b lastSyncTimestamp syncPending peerConnected activityLevel projectState, gw-b config-resource provider (status, pendingUpdates, lastSyncError cut to 120, hashcode), count of providers not GOOD, count of redundancy events at WARN or ERROR; then new filtered log lines (cut to 300)
2026-10-19T21:08:39Z +   0s gwB: -1 false true Cold Out_of_Date | provider/notGOOD=[{"status":"UNKNOWN","pendingUpdates":0,"lastSyncError":"","hashcode":98487576},11] | warn_error_events=0
    LOG gw-b-1  | jvm 1    | 2026/10/19 21:08:38 | W [R.S.BackupStateManager        ] [21:08:38.277]: Active deployment mode differs from peer node! Local active mode: '', Peer active mode: 'production' redundant-provider=system
2026-10-19T21:08:54Z +  15s gwB: 1792444120141 false false Cold Out_of_Date | provider/notGOOD=[{"status":"UNKNOWN","pendingUpdates":0,"lastSyncError":"","hashcode":983472216},11] | warn_error_events=0
2026-10-19T21:09:09Z +  30s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":415232773},0] | warn_error_events=0
2026-10-19T21:09:24Z +  45s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1904255363},0] | warn_error_events=0
2026-10-19T21:09:39Z +  60s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":744759126},0] | warn_error_events=0
2026-10-19T21:09:54Z +  75s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":628665782},0] | warn_error_events=0
2026-10-19T21:10:10Z +  91s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":664850363},0] | warn_error_events=0
2026-10-19T21:10:24Z + 105s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1112697489},0] | warn_error_events=0
2026-10-19T21:10:39Z + 120s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":2072929777},0] | warn_error_events=0
2026-10-19T21:10:55Z + 136s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1108873229},0] | warn_error_events=0
2026-10-19T21:11:09Z + 150s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1064049585},0] | warn_error_events=0
2026-10-19T21:11:24Z + 165s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1251447878},0] | warn_error_events=0
2026-10-19T21:11:39Z + 180s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":758377295},0] | warn_error_events=0
2026-10-19T21:11:54Z + 195s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":583710914},0] | warn_error_events=0
2026-10-19T21:12:09Z + 210s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1472260579},0] | warn_error_events=0
2026-10-19T21:12:24Z + 225s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":130806955},0] | warn_error_events=0
2026-10-19T21:12:39Z + 240s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1229989798},0] | warn_error_events=0
2026-10-19T21:12:55Z + 256s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":2053030560},0] | warn_error_events=0
2026-10-19T21:13:09Z + 270s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1132275890},0] | warn_error_events=0
2026-10-19T21:13:25Z + 286s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1853692776},0] | warn_error_events=0
2026-10-19T21:13:40Z + 301s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1231315066},0] | warn_error_events=0
2026-10-19T21:13:54Z + 315s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":588196070},0] | warn_error_events=0
2026-10-19T21:14:09Z + 330s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1138353441},0] | warn_error_events=0
2026-10-19T21:14:24Z + 345s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":93627431},0] | warn_error_events=0
2026-10-19T21:14:39Z + 360s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1251195673},0] | warn_error_events=0
2026-10-19T21:14:54Z + 375s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":852827117},0] | warn_error_events=0
2026-10-19T21:15:09Z + 390s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1408044804},0] | warn_error_events=0
2026-10-19T21:15:25Z + 406s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":231367559},0] | warn_error_events=0
2026-10-19T21:15:39Z + 420s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":292481734},0] | warn_error_events=0
2026-10-19T21:15:55Z + 436s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1702454594},0] | warn_error_events=0
2026-10-19T21:16:09Z + 450s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":847831317},0] | warn_error_events=0
2026-10-19T21:16:24Z + 465s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":576742496},0] | warn_error_events=0
2026-10-19T21:16:39Z + 480s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":523440457},0] | warn_error_events=0
2026-10-19T21:16:54Z + 495s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1634134277},0] | warn_error_events=0
2026-10-19T21:17:09Z + 510s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":2064490815},0] | warn_error_events=0
2026-10-19T21:17:24Z + 525s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1617726151},0] | warn_error_events=0
2026-10-19T21:17:39Z + 540s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":702516485},0] | warn_error_events=0
2026-10-19T21:17:54Z + 555s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1555469121},0] | warn_error_events=0
2026-10-19T21:18:09Z + 570s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1532648841},0] | warn_error_events=0
2026-10-19T21:18:24Z + 585s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1694761511},0] | warn_error_events=0
2026-10-19T21:18:39Z + 600s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":2070955785},0] | warn_error_events=0
2026-10-19T21:18:54Z + 615s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1136083048},0] | warn_error_events=0
2026-10-19T21:19:09Z + 630s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1529004808},0] | warn_error_events=0
2026-10-19T21:19:24Z + 645s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1774027760},0] | warn_error_events=0
2026-10-19T21:19:39Z + 660s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":799442101},0] | warn_error_events=0
2026-10-19T21:19:54Z + 675s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1223569412},0] | warn_error_events=0
2026-10-19T21:20:09Z + 690s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":461398657},0] | warn_error_events=0
2026-10-19T21:20:24Z + 705s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1554261027},0] | warn_error_events=0
2026-10-19T21:20:40Z + 721s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1861620395},0] | warn_error_events=0
2026-10-19T21:20:54Z + 735s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":377245708},0] | warn_error_events=0
2026-10-19T21:21:09Z + 750s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":987941738},0] | warn_error_events=0
2026-10-19T21:21:24Z + 765s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1779760568},0] | warn_error_events=0
2026-10-19T21:21:39Z + 780s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1024154608},0] | warn_error_events=0
2026-10-19T21:21:54Z + 795s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1313937178},0] | warn_error_events=0
2026-10-19T21:22:10Z + 811s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":761665439},0] | warn_error_events=0
2026-10-19T21:22:24Z + 825s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1462057361},0] | warn_error_events=0
2026-10-19T21:22:39Z + 840s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":864590131},0] | warn_error_events=0
2026-10-19T21:22:54Z + 855s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1211285190},0] | warn_error_events=0
2026-10-19T21:23:09Z + 870s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":931290750},0] | warn_error_events=0
2026-10-19T21:23:24Z + 885s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":664332897},0] | warn_error_events=0
2026-10-19T21:23:39Z + 900s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1281817743},0] | warn_error_events=0
2026-10-19T21:23:54Z + 915s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1466991149},0] | warn_error_events=0
2026-10-19T21:24:09Z + 930s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":637047046},0] | warn_error_events=0
2026-10-19T21:24:24Z + 945s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":815132668},0] | warn_error_events=0
2026-10-19T21:24:39Z + 960s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1877615215},0] | warn_error_events=0
2026-10-19T21:24:54Z + 975s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1826632188},0] | warn_error_events=0
2026-10-19T21:25:09Z + 990s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":2083461270},0] | warn_error_events=0
2026-10-19T21:25:24Z +1005s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":96722898},0] | warn_error_events=0
2026-10-19T21:25:39Z +1020s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1592374666},0] | warn_error_events=0
2026-10-19T21:25:54Z +1035s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1155196301},0] | warn_error_events=0
2026-10-19T21:26:09Z +1050s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":361429496},0] | warn_error_events=0
2026-10-19T21:26:24Z +1065s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1585230148},0] | warn_error_events=0
2026-10-19T21:26:39Z +1080s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":2112900759},0] | warn_error_events=0
2026-10-19T21:26:54Z +1095s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":2083993022},0] | warn_error_events=0
2026-10-19T21:27:09Z +1110s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1495253652},0] | warn_error_events=0
2026-10-19T21:27:24Z +1125s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":862946816},0] | warn_error_events=0
2026-10-19T21:27:39Z +1140s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":2032134313},0] | warn_error_events=0
2026-10-19T21:27:54Z +1155s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":992403326},0] | warn_error_events=0
2026-10-19T21:28:10Z +1171s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1467801058},0] | warn_error_events=0
2026-10-19T21:28:24Z +1185s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1662523972},0] | warn_error_events=0
2026-10-19T21:28:39Z +1200s gwB: 1792444140651 false true Cold Good | provider/notGOOD=[{"status":"GOOD","pendingUpdates":0,"lastSyncError":"","hashcode":1376965414},0] | warn_error_events=0
SUMMARY max_attempt_count_line_seen_at=never
[exit 0]
~~~

#### GET redundancy/providers on gw-b (box ended, line never seen)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy/providers'` @ 2026-10-19T21:28:40Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:28:40 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 3059

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":1200945,"lastPullDuration":23,"lastApplyDuration":53,"lastSyncError":"","status":"GOOD","hashcode":457292093},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":1200826,"lastPullDuration":46,"lastApplyDuration":37,"lastSyncError":"","status":"GOOD","hashcode":1249216469},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":1201424,"lastPullDuration":18,"lastApplyDuration":33,"lastSyncError":"","status":"GOOD","hashcode":2083598713},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":1200455,"lastPullDuration":16,"lastApplyDuration":11,"lastSyncError":"","status":"GOOD","hashcode":1521279882},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":1200898,"lastPullDuration":12,"lastApplyDuration":26,"lastSyncError":"","status":"GOOD","hashcode":1278353843},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":1200634,"lastPullDuration":17,"lastApplyDuration":302,"lastSyncError":"","status":"GOOD","hashcode":706698447},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":1201006,"lastPullDuration":21,"lastApplyDuration":14,"lastSyncError":"","status":"GOOD","hashcode":1607469580},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":1200427,"lastPullDuration":16,"lastApplyDuration":19,"lastSyncError":"","status":"GOOD","hashcode":1658655439},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":1179666,"lastPullDuration":68,"lastApplyDuration":10,"lastSyncError":"","status":"GOOD","hashcode":1069521054},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":1201049,"lastPullDuration":152,"lastApplyDuration":109,"lastSyncError":"","status":"GOOD","hashcode":1863729794},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":1179744,"lastPullDuration":312,"lastApplyDuration":63,"lastSyncError":"","status":"GOOD","hashcode":818521350}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.004406s]
~~~

#### GET redundancy/events?limit=25 on gw-b (box ended)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy/events?limit=25'` @ 2026-10-19T21:28:40Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:28:40 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 7296

{"items":[{"timestamp":1792444140639,"loggerName":"Redundancy.StateMonitoring.BackupStateManager","level":"INFO","message":"Redundancy state changed: Role\u003dBackup, Activity level\u003dCold, Project state\u003dGood, History level\u003dFull","marker":"evt:redundancy","sourceLocation":"?#?() (?:-1)","properties":{"redundant-provider":"system"}},{"timestamp":1792444119991,"loggerName":"Redundancy.StateMonitoring.BackupStateManager","level":"INFO","message":"Redundancy state changed: Role\u003dBackup, Activity level\u003dCold, Project state\u003dOutOfDate, History level\u003dFull","marker":"evt:redundancy","sourceLocation":"?#?() (?:-1)","properties":{"redundant-provider":"system"}},{"timestamp":1792444119898,"loggerName":"Redundancy.StateMonitoring.BackupStateManager","level":"INFO","message":"Redundancy state changed: Role\u003dBackup, Activity level\u003dCold, Project state\u003dGood, History level\u003dFull","marker":"evt:redundancy","sourceLocation":"?#?() (?:-1)","properties":{"redundant-provider":"system"}},{"timestamp":1792444118409,"loggerName":"Redundancy.StateMonitoring.SyncManager","level":"INFO","message":"[alarm-notification] Will perform full pull. Reason: major change detected between nodes (sync provider UUIDs did not match).","marker":"evt:redundancy","sourceLocation":"?#?() (?:-1)","properties":{"gan-remote-gateway-name":"ign-pair-a-Master","redundant-provider":"alarm-notification"}},{"timestamp":1792444118409,"loggerName":"Redundancy.StateMonitoring.SyncManager","level":"INFO","message":"[vision_auth_token_runtime_provider] Will perform full pull. Reason: major change detected between nodes (sync provider UUIDs did not match).","marker":"evt:redundancy","sourceLocation":"?#?() (?:-1)","properties":{"gan-remote-gateway-name":"ign-pair-a-Master","redundant-provider":"vision_auth_token_runtime_provider"}},{"timestamp":1792444118408,"loggerName":"Redundancy.StateMonitoring.SyncManager","level":"INFO","message":"[perspective_auth_token_runtime_provider] Will perform full pull. Reason: major change detected between nodes (sync provider UUIDs did not match).","marker":"evt:redundancy","sourceLocation":"?#?() (?:-1)","properties":{"gan-remote-gateway-name":"ign-pair-a-Master","redundant-provider":"perspective_auth_token_runtime_provider"}},{"timestamp":1792444118408,"loggerName":"Redundancy.StateMonitoring.SyncManager","level":"INFO","message":"[sfc] Will perform full pull. Reason: major change detected between nodes (sync provider UUIDs did not match).","marker":"evt:redundancy","sourceLocation":"?#?() (?:-1)","properties":{"gan-remote-gateway-name":"ign-pair-a-Master","redundant-provider":"sfc"}},{"timestamp":1792444118407,"loggerName":"Redundancy.StateMonitoring.SyncManager","level":"INFO","message":"[alarm-system] Will perform full pull. Reason: major change detected between nodes (sync provider UUIDs did not match).","marker":"evt:redundancy","sourceLocation":"?#?() (?:-1)","properties":{"gan-remote-gateway-name":"ign-pair-a-Master","redundant-provider":"alarm-system"}},{"timestamp":1792444118406,"loggerName":"Redundancy.StateMonitoring.SyncManager","level":"INFO","message":"[alarm-shelf] Will perform full pull. Reason: major change detected between nodes (sync provider UUIDs did not match).","marker":"evt:redundancy","sourceLocation":"?#?() (?:-1)","properties":{"gan-remote-gateway-name":"ign-pair-a-Master","redundant-provider":"alarm-shelf"}},{"timestamp":1792444118406,"loggerName":"Redundancy.StateMonitoring.SyncManager","level":"INFO","message":"[Tag - default - Runtime Values] Will perform full pull. Reason: major change detected between nodes (sync provider UUIDs did not match).","marker":"evt:redundancy","sourceLocation":"?#?() (?:-1)","properties":{"gan-remote-gateway-name":"ign-pair-a-Master","redundant-provider":"Tag - default - Runtime Values"}},{"timestamp":1792444118406,"loggerName":"Redundancy.StateMonitoring.SyncManager","level":"INFO","message":"[Tag - default - DB Values] Will perform full pull. Reason: major change detected between nodes (sync provider UUIDs did not match).","marker":"evt:redundancy","sourceLocation":"?#?() (?:-1)","properties":{"gan-remote-gateway-name":"ign-pair-a-Master","redundant-provider":"Tag - default - DB Values"}},{"timestamp":1792444118395,"loggerName":"Redundancy.StateMonitoring.SyncManager","level":"INFO","message":"[tag-config-resource] Will perform full pull. Reason: major change detected between nodes (sync provider UUIDs did not match).","marker":"evt:redundancy","sourceLocation":"?#?() (?:-1)","properties":{"gan-remote-gateway-name":"ign-pair-a-Master","redundant-provider":"tag-config-resource"}},{"timestamp":1792444118394,"loggerName":"Redundancy.StateMonitoring.SyncManager","level":"INFO","message":"[projects] Will perform full pull. Reason: major change detected between nodes (sync provider UUIDs did not match).","marker":"evt:redundancy","sourceLocation":"?#?() (?:-1)","properties":{"gan-remote-gateway-name":"ign-pair-a-Master","redundant-provider":"projects"}},{"timestamp":1792444118325,"loggerName":"Redundancy.StateMonitoring.SyncManager","level":"INFO","message":"[config-resource] Will perform full pull. Reason: major change detected between nodes (sync provider UUIDs did not match).","marker":"evt:redundancy","sourceLocation":"?#?() (?:-1)","properties":{"gan-remote-gateway-name":"ign-pair-a-Master","redundant-provider":"config-resource"}},{"timestamp":1792444118305,"loggerName":"Redundancy.StateMonitoring.BackupStateManager","level":"INFO","message":"Redundancy state changed: Role\u003dBackup, Activity level\u003dCold, Project state\u003dOutOfDate, History level\u003dFull","marker":"evt:redundancy","sourceLocation":"?#?() (?:-1)","properties":{"redundant-provider":"system"}},{"timestamp":1792444118277,"loggerName":"Redundancy.StateMonitoring.BackupStateManager","level":"INFO","message":"Redundancy state changed: Role\u003dBackup, Activity level\u003dCold, Project state\u003dUnknown, History level\u003dFull","marker":"evt:redundancy","sourceLocation":"?#?() (?:-1)","properties":{"redundant-provider":"system"}},{"timestamp":1792444108126,"loggerName":"Redundancy.StateMonitoring.BackupStateManager","level":"INFO","message":"Redundancy state changed: Role\u003dBackup, Activity level\u003dUndecided, Project state\u003dUnknown, History level\u003dFull","marker":"evt:redundancy","sourceLocation":"?#?() (?:-1)","properties":{"redundant-provider":"system"}},{"timestamp":1792444108083,"loggerName":"IgnitionGateway","level":"INFO","message":"Ignition[state\u003dSTARTING] ContextState \u003d RUNNING","marker":"evt:GatewayContextImpl.systemEvent","sourceLocation":"?#?() (?:-1)"},{"timestamp":1792444058143,"loggerName":"Redundancy.StateMonitoring.BackupStateManager","level":"INFO","message":"Redundancy state changed: Role\u003dBackup, Activity level\u003dCold, Project state\u003dUnknown, History level\u003dPartial","marker":"evt:redundancy","sourceLocation":"?#?() (?:-1)","properties":{"redundant-provider":"system"}},{"timestamp":1792444057876,"loggerName":"IgnitionGateway","level":"INFO","message":"Ignition[state\u003dSTOPPED] ContextState \u003d STARTING","marker":"evt:GatewayContextImpl.systemEvent","sourceLocation":"?#?() (?:-1)"}],"metadata":{"total":20,"matching":20,"limit":25,"offset":0}}
[curl http=200 total=0.023812s]
~~~

not reachable on the rig. Tried: gw-b recreated without a deployment mode against a `production` master, relinked, then polled every 15 s from 2026-10-19T21:07:14Z to 2026-10-19T21:28:40Z (UTC); the `max attempt count` line did not appear. The log lines that did appear are in the poll above.

What the poll did show (author's reading of the lines above, not gateway bytes): the mode mismatch alone produced one `Active deployment mode differs from peer node!` warning at 21:08:38Z and nothing else. gw-b went `Out_of_Date` with `lastSyncTimestamp` -1, took a first sync (providers all `UNKNOWN`, then all `GOOD` by 21:09:09Z), and then sat `Cold`, `Good`, `peerConnected` true, `syncPending` false for the remaining 19 minutes. No `did not match remote system snapshot` line, no WARN or ERROR redundancy event, no provider with a `lastSyncError`. The spike's exhausted state needed more than the mode mismatch; a config change made on the master while gw-b ran without the mode was NOT tried within the time box. No resync was sent in this capture because the exhausted state never appeared.

Restore: gw-b is recreated with the production mode argument, relinked, and waited on until settled; the end of the time box was 2026-10-19T21:28:40Z (UTC).

#### recreate gw-b with the production mode argument

`env IGN_PAIR_MODE_B=-Dignition.config.mode=production docker compose --project-directory /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair -p ign-pair-rig -f /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/compose.yml up -d gw-b ` @ 2026-10-19T21:28:40Z

~~~
 Container ign-pair-rig-gw-b-1 Recreate 
 Container ign-pair-rig-gw-b-1 Recreated 
 Container ign-pair-rig-gw-b-1 Starting 
 Container ign-pair-rig-gw-b-1 Started 
[exit 0]
~~~

#### relink: setup-pair.sh link

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-pair/setup-pair.sh link ` @ 2026-10-19T21:28:46Z

~~~
toggleApproval true 'ign-pair-a-backup|335fe286-20e6-411b-a5e2-e96c8ba2cfdc' on gw-a -> HTTP 200
peerConnected true after 63 s
[exit 0]
~~~

### C-P4 after-exhaust

#### GET redundancy on gw-a (after-exhaust)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy'` @ 2026-10-19T21:30:10Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:30:10 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node0bg1ivx32zzxv18zxa3klf8ahd174.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 258

{"role":"Master","projectState":"Good","activityLevel":"Active","localId":"192.168.156.2","peerConnected":true,"peerId":"ign-pair-a-Backup","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":3815671,"lastSyncTimestamp":1792445409740}
[curl http=200 total=0.007835s]
~~~

#### GET redundancy/providers on gw-a (after-exhaust)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/redundancy/providers'` @ 2026-10-19T21:30:10Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:30:10 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 3033

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":2075988783},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":568903071},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1886856369},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1780402598},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":2032765857},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":512392486},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1460764906},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1272852567},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":0,"queuedUpdatesPerSec":0.029836063824357575,"dispatchedUpdatesPerSec":0.02490778903839267,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":2124402190},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":867299254},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":0,"queuedUpdatesPerSec":0.02983606381700317,"dispatchedUpdatesPerSec":0.024907789030399138,"lastPullTimestamp":0,"lastPullDuration":0,"lastApplyDuration":0,"lastSyncError":"","status":"GOOD","hashcode":1761357818}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.021187s]
~~~

#### gateway-info name and deploymentMode on gw-a (after-exhaust)

`curl -sS -g [token $IGN_PAIR_TOKEN_A] '$BASE_A/data/api/v1/gateway-info' | jq -c '{name, deploymentMode}'` @ 2026-10-19T21:30:10Z

~~~
{"name":"ign-pair-a","deploymentMode":"production"}
~~~

#### GET redundancy on gw-b (after-exhaust)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy'` @ 2026-10-19T21:30:10Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:30:10 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Set-Cookie: JSESSIONID=node013xl2l2n9mdbrptquh5ijsoa14.node0; Path=/; HttpOnly
Expires: Thu, 01 Jan 1970 00:00:00 GMT
Content-Type: application/json
Content-Length: 251

{"role":"Backup","projectState":"Good","activityLevel":"Cold","localId":"192.168.147.3","peerConnected":true,"peerId":"ign-pair-a-Master","hasConfigAccess":true,"syncPending":false,"failoverPending":false,"uptime":-1,"lastSyncTimestamp":1792445409763}
[curl http=200 total=0.005968s]
~~~

#### GET redundancy/providers on gw-b (after-exhaust)

`curl -sS -g -i -X GET [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/redundancy/providers'` @ 2026-10-19T21:30:10Z

~~~
HTTP/1.1 200 OK
Date: Mon, 19 Oct 2026 21:30:10 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 3027

{"items":[{"name":"alarm-shelf","friendlyName":"Alarm Shelf","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":21748,"lastPullDuration":30,"lastApplyDuration":32,"lastSyncError":"","status":"GOOD","hashcode":1416665205},{"name":"sfc","friendlyName":"SFC","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":21497,"lastPullDuration":174,"lastApplyDuration":81,"lastSyncError":"","status":"GOOD","hashcode":554985317},{"name":"projects","friendlyName":"Projects","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":21996,"lastPullDuration":21,"lastApplyDuration":14,"lastSyncError":"","status":"GOOD","hashcode":1852035981},{"name":"vision_auth_token_runtime_provider","friendlyName":"Vision Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":21318,"lastPullDuration":19,"lastApplyDuration":20,"lastSyncError":"","status":"GOOD","hashcode":7086928},{"name":"alarm-system","friendlyName":"Alarm Events","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":21705,"lastPullDuration":24,"lastApplyDuration":17,"lastSyncError":"","status":"GOOD","hashcode":1343205400},{"name":"perspective_auth_token_runtime_provider","friendlyName":"Perspective Auth Tokens","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":21384,"lastPullDuration":25,"lastApplyDuration":64,"lastSyncError":"","status":"GOOD","hashcode":1386404403},{"name":"Tag - default - Runtime Values","friendlyName":"Tag - default - Runtime Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":21826,"lastPullDuration":24,"lastApplyDuration":32,"lastSyncError":"","status":"GOOD","hashcode":581796945},{"name":"alarm-notification","friendlyName":"Alarm Notification","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":21291,"lastPullDuration":14,"lastApplyDuration":17,"lastSyncError":"","status":"GOOD","hashcode":1127896567},{"name":"tag-config-resource","friendlyName":"Tag Resource Updates","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":734,"lastPullDuration":29,"lastApplyDuration":15,"lastSyncError":"","status":"GOOD","hashcode":1855501048},{"name":"Tag - default - DB Values","friendlyName":"Tag - default - DB Values","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":21862,"lastPullDuration":25,"lastApplyDuration":32,"lastSyncError":"","status":"GOOD","hashcode":707763802},{"name":"config-resource","friendlyName":"System Configuration","pendingUpdates":0,"queuedUpdatesPerSec":0.0,"dispatchedUpdatesPerSec":0.0,"lastPullTimestamp":779,"lastPullDuration":95,"lastApplyDuration":41,"lastSyncError":"","status":"GOOD","hashcode":1693325378}],"metadata":{"total":11,"matching":11,"limit":-1,"offset":0}}
[curl http=200 total=0.028474s]
~~~

#### gateway-info name and deploymentMode on gw-b (after-exhaust)

`curl -sS -g [token $IGN_PAIR_TOKEN_B] '$BASE_B/data/api/v1/gateway-info' | jq -c '{name, deploymentMode}'` @ 2026-10-19T21:30:10Z

~~~
{"name":"ign-pair-a","deploymentMode":"production"}
~~~

## Decisions

Written by 24-02 Task 2 from the blocks above. Each subsection states what the capture showed (quoting the block it comes from) and which option of the contract items below it supports. A recommendation other than `a` on K1, K2, K3, K6 or K8 relaxes a locked CONTEXT decision and is settled only by the user's answer under `## Contract decisions (24-02)`.

### C-P1

In `### C-P1 file path` gw-b `lastSyncTimestamp` advanced (1792441595954 to 1792442647207) and the new resource read back on gw-b 10 s into the poll, with a second advance 4 s later; in `### C-P1 REST path` the POST and the PUT each read back on gw-b within 2 s with an advanced timestamp. `syncPending` was never true at 2 s resolution, so it cannot discriminate a synced backup from a syncing one. Supports K1 a: the timestamp-advance clause of D-06 holds for an incremental file-path change and for REST writes, and no healthy apply ends unsynced because of it.

### C-P2

In `## C-P2` `config.json` is byte-equal on both nodes for every changed unit and the untouched `core/ignition/general-alarm-settings` is byte-equal in both files, but `resource.json` differs for all four changed units. The diff shows no changed value: the backup rewrites the file with the members in another order (`uuid` and `lastModificationSignature` swap) or pretty-printed (the one-line file the driver wrote becomes 13 lines), and `uuid`, `lastModificationSignature` and every other member carry the same value. Supports K2 b: byte equality of `resource.json` would fail on every healthy apply (K2 a), so `resource.json` is compared as a parsed JSON document with member order and whitespace ignored and every value equal, as one named exception; every other file stays byte-equal.

### C-P3

In `## C-P3` `POST /data/api/v1/redundancy/gwaction/resync` sent to gw-a answered 200 `{"resync_state":"started"}`; gw-b `lastSyncTimestamp` advanced within 2 s and the pair read settled throughout. The answer carries no sign of completion, so a 200 is only a request accepted. Supports the V2 `resync` field as `{ok, code}` (the status of the one POST) and the rule that the resync alone is never proof; says nothing yet about the exhausted state (see C-P8).

### C-P4

In the `### C-P4` snapshots a settled pair reads gw-a `Master Active true false`, gw-b `Backup Cold true false` (`peerConnected` true, `syncPending` false), both `projectState` `Good`, gw-b `uptime` -1, both `deploymentMode` `production`; token A answers 200 on gw-a and 401 on gw-b, token B the reverse (A10 does not hold: tokens are node-local, one per node). `projectState` reads `Out of Date` on gw-b while the link is cut (`### C-P4 during-cut-c-p6`) and `Good` again once the pair settles, and gw-b `gateway-info.name` reads the master's name. Supports V9 (`projectState` is recorded, not a convergence condition), V6 (the verify `redundancy` check is `peerConnected` and not `syncPending`), and the 24-05/24-10 rule that the backup session uses its own token and its profile name comes from the target, not from `gateway-info`.

### C-P5

In `### C-P5 cut by toggleApproval false on gw-a` the call answered 200 and dropped the link on both nodes within 2 s while `/StatusPing` and gw-b `GET redundancy` still answered 200 for all 60 s; `toggleApproval true` plus the trust loop settled in 7 s. In `### C-P5 fallback (extra)` `docker network disconnect` of gw-b from `gan` left `peerConnected` true on both nodes for 180 s. Supports K4 a: only the approval cut looks like a cut to the status fields; the network cut does not within a gate-sized window.

### C-P6

In `## C-P6` gw-b reported `Active` from its first reading after the toggleApproval cut and stayed `Active` for the 120 s window while gw-a stayed `Active` (`SUMMARY gw-b reported Active after 0 s of the cut window`), so `active_node` would read `Both`; with the network cut (`### C-P5 fallback (extra)`) gw-b stayed `Cold`. K3 a (cut before the apply, `master_active` refuses `Both`) cannot work with the only cut that registers: the gate would be refused by its own preflight. Supports K3 b: cut the link when the engine reports `lock_acquired`, after the preflights have read both nodes, with D-11 unchanged.

### C-P7

Across `## C-P7`: a fresh rig has no secret-provider resources and the names `default` and `internal` answer 404 (HTML body) on both nodes, as does the unknown `nope-24`; a `file` and an `internal` provider list `200 {"secrets":[]}` on both nodes and the file provider lists the names created on gw-a; a `remote` provider answers 500 (HTML body) on both nodes and no provider answered 501. No token answers 401, a wrong key 401, a token without the Deploy permission 403 (listing and resource list). A name created on gw-a appears on gw-b in 0 to 5 s (+0 s `gwB=[]`, +5 s `gwB=["CAP24-Name"]`), the listing is built from provider config (ENT-3115: names only, no value), and `CAP24-Name` and `cap24-name` list as two distinct names. Supports K7 a with a sub-clause (the 500 is the only non-listing answer seen; see the K7 row), K8 a (a provider the render declares answers 404 until it replicates), K9 a (exact, case-sensitive), and the V7 names-only shape.

### C-P8

`## C-P8` is the TEC-5105 case (own finding `sync_exhausted`, never a master rollback). The sync-exhausted state was not reached in the 21-minute box: gw-b without a deployment mode against a `production` master produced one `Active deployment mode differs from peer node!` warning at 21:08:38Z, then a first sync and 19 minutes of `Cold`, `Good`, `peerConnected` true, `syncPending` false, with no `did not match remote system snapshot` line, no warning or error redundancy event and no provider `lastSyncError`. No resync was sent. Supports K5 d (the signal is not observable on the rig, so the log matcher ships unit-tested only, D-18) and K6 a (the plan's rule for an unreached state); whether a resync clears the state stays unknown, and K6 a keeps the one resync with no second budget.

## Contract proposal (24-02)

One table; the Recommended column is the option letter (or `as written` for the published vocabulary V1 to V10). Items K1 to K9 come from the captures, V1 to V10 are the new published vocabulary. Rows other than `a` on K1, K2, K3, K6 and K8 are named in the checkpoint because they relax a locked decision.

| Item | Question | Options | Recommended | Evidence |
| --- | --- | --- | --- | --- |
| K1 | D-06: is the convergence signal the backup `lastSyncTimestamp` advancing past its own pre-apply sample? | a = D-06 as written, `SyncPredicate::TimestampAdvanced` (`-1` = never, plus `peerConnected`, not `syncPending`, mode, read-back and files equal); b = `SyncPredicate::StableTicks(2)` (a relaxation of D-06) | a | `### C-P1 file path` and `### C-P1 REST path`: the timestamp advanced on every incremental change; D-06 |
| K2 | D-03: how are backup files compared to the master's live copy? | a = byte equality, `FILE_NORMALIZATION` empty; b = named exceptions, each an explicit row, never a looser comparison | b, one row: `resource.json` compared as a parsed JSON document (member order and whitespace ignored, every value equal, no member excluded); all other files byte-equal | `## C-P2`: `config.json` and untouched units byte-equal, all four changed `resource.json` differ in key order or layout only; D-03 |
| K3 | D-11 against D-17: how does the unsynced gate cut the link given gw-b promotes itself? | a = cut before the apply, `master_active` refuses `OnlyB`, `Both` and `Neither`; b = cut from an `ApplyObserver` at `lock_acquired`, D-11 unchanged; c = accept `Both` in D-11 | b | `## C-P6`: gw-b `Active` from the first reading after the approval cut (`Both`); `### C-P5 fallback (extra)`: a network cut stays `Cold` but `peerConnected` stays true for 180 s; D-11, D-17 |
| K4 | D-17: which cut method does the gate use? | a = `PUT toggleApproval/<id>/false` on gw-a; b = `cut-net` (gw-b off the `gan` network only) | a | `### C-P5 cut by toggleApproval false on gw-a`: link down within 2 s, both REST APIs up, reconnect in 7 s; `### C-P5 fallback (extra)` |
| K5 | D-10: where is the sync-exhausted state read? | a = `GET /redundancy/providers` on the backup, field values from C-P8; b = `GET /redundancy/events` on the backup, matched client-side; c = the backup log through `GatewayApi::logs`, matched client-side on `exceeds the max attempt count`; d = not reachable on the rig, ship c, unit-tested only | d | `## C-P8`: not reachable in 21 minutes, no field, event or log line of the state recorded; spike `results/RIG.md` supplies the log line text; D-18 |
| K6 | D-10, A7, C12, TEC-5105: after the signal, is there a second budget? | a = end the wait, send the one resync to the master, report `sync_exhausted` (no second budget); b = wait a second budget first (only if resync alone clears the state) | a | `## C-P8`: state not reached, so C12 is unanswered and the plan's rule gives a; D-10. The TEC-5105 rule (own finding, never a master rollback) holds in both options |
| K7 | A4: how does a listing that cannot answer count? | a = a provider that cannot list is `unverifiable` (check `warn`, provider named), not a failure; b = it fails the check | a, where "cannot list" is 501 and also the 500 seen on a `remote` provider (the 24-03 `SecretNameListing::CannotList` case); any other status is an error | `### C-P7 remote provider (candidate for a 501 listing)`: 500 on both nodes, no 501 anywhere; Pitfall 6 |
| K8 | Pitfall 6, D-14: a render declares a provider that answers 404 on a node (first deploy) | a = the preflight skips it with a detail naming the provider, verify still requires it; b = refuse (D-14 literal) | a | `### C-P7 unknown provider nope-24` and `### C-P7 providers created on gw-a for the capture`: 404 until the provider exists, then it reaches gw-b in 0 to 5 s; D-14 |
| K9 | C-P7: how are secret names compared? | a = exact, case-sensitive strings as listed; b = case-insensitive | a | `### C-P7 a secret created on gw-a`: `CAP24-Name` and `cap24-name` are two entries on both nodes, the case is kept |
| V1 | D-08 (one-way): the outcome word, slug and exit code for a master that applied and verified with an unproven backup | outcome `applied_backup_unsynced` (`OutcomeKind::AppliedBackupUnsynced`), `data.ok` false, `data.slug` `deploy_backup_unsynced`, `data.cause` null, exit 6 after the envelope, the same in `result.json`; not a `CoreError`, not in `DEPLOY_SLUGS` (stays 12), not an exit-table row; README `### The deploy apply backup-unsynced verdict exception (<date>)` | as written | D-08, D-09; the `deploy_rolled_back` pattern (Decision A) |
| V2 | D-09 (costly): the shape of `data.backup` | keys `converged`, `reason` (`unreachable`, `not_syncing`, `content_differs`, `sync_exhausted` or null), `budget_s`, `waited_s`, `resync` (null or `{ok, code}`), `last_sync_before`, `last_sync_after`, `project_state`, `files_checked`, `checks` (`[{id, status, detail, findings}]`, ids `redundancy`, `mode`, `read_back`, `files`; statuses `pass`, `fail`, `warn`; findings value-free); `backup` null without a backup node, `noop` and `rolled_back`; reason precedence `unreachable` then `sync_exhausted` then `not_syncing` then `content_differs` on the final observation; `unreachable` covers a backup REST or file read that could not be answered (error code in the check detail) | as written | D-09; `## C-P3` (resync answer), `### C-P4` (status fields); the user confirms the precedence rule here |
| V3 | Events an apply emits for the backup phase | `backup_sampled {last_sync}`, `backup_wait_started {budget_s}`, `backup_poll {peer_connected, sync_pending, advanced}`, `backup_resync {ok, code}`, `backup_verdict {converged, reason}` | as written | D-07, D-09; `### C-P1` (the signals polled) |
| V4 | Preflight ids added by Phase 24 | appended after `hold_budget`: `master_active`, `backup_read`, `secrets` (10 ids); `master_active` `skipped` without a backup node and run by rollback too; `backup_read` `skipped` for a REST-only backup and in rollback; `secrets` `skipped` in rollback | as written | D-02, D-11, D-14; `## C-P6` (`master_active` reads `Both`) |
| V5 | D-05: the wait flag | `--backup-timeout <SECS>` default 300, range 1..=86400; poll interval 5 s; a failed pre-apply sample read aborts before any write with its own error | as written | D-05; `### C-P1` (sync seen within 2 to 10 s, so 5 s polling is fine) |
| V6 | D-12 (costly): `deploy verify --node` | `--node master, backup or both`, default `master` with byte-identical output; `--node backup` runs only the backup checks (`data.checks` empty, `data.node` `backup`, envelope profile the backup's); `--node both` runs the six master checks then the backup checks; `data.backup` has the V2 shape with `budget_s` 0, `waited_s` 0, `resync` null; the verify `redundancy` check is `peerConnected` and not `syncPending`; a red verdict is `data.ok` false, slug `deploy_verify_failed`, exit 6; verify never waits or resyncs; `--node backup` or `both` without `nodes.backup` is `invalid_input` (exit 2) | as written | D-12; `### C-P4` (`syncPending` never true) |
| V7 | D-13 with D-12: where the secret-name check runs and its shape | runs for `--node backup` (backup node) and `--node both` (both nodes), never for `--node master`; `data.secrets` = `{nodes: [{node, status, providers_checked, missing: [{provider, name}], unverifiable: [provider]}]}`, byte-sorted, names only; a tree with no reference passes with `providers_checked` 0 | as written | D-13; `### C-P7 a secret created on gw-a` (names only, no value) |
| V8 | D-11: the active-node report | `ign redundancy status --peer <PROFILE>` adds `data.active_node` = `{active: this, peer, both or neither, this_role, peer_role, peer}`; without `--peer` the output is byte-identical (key omitted); human line `active node: this (Master)`; alternative b = `--env <ENV>` reading both profiles from the target | as written (a) | D-11; `## C-P6` (the `Both` reading) |
| V9 | A8: where `projectState` goes | recorded as `data.backup.project_state`, not a convergence condition | as written | `### C-P4`: `Out of Date` is transient on a node that is otherwise settled; A8 |
| V10 | D-08: when no backup phase runs | a `noop` or `rolled_back` apply runs no backup phase (`backup` null); `ign deploy verify --node backup` is the on-demand check | as written | D-08, D-12 |

## Contract decisions (24-02)

Recorded by 24-02 Task 3 on 2026-10-19 (UTC). The user's answer, verbatim: "Approve all (Recommended)". It selects the Recommended option of every row in `## Contract proposal (24-02)`, with the sub-clause on K7 as written in its row. No item differs from its Recommended value. The user chose to pass plan-text overrides to later executors in their prompts and not to revise the 24-03..24-10 PLAN.md files, so the plans still carry their conditional wording; the executor of each later plan applies the choices below.

| Item | Chosen | Date (UTC) | Decision |
| --- | --- | --- | --- |
| K1 | a | 2026-10-19 | D-06 as written: `SyncPredicate::TimestampAdvanced` |
| K2 | b | 2026-10-19 | One `FILE_NORMALIZATION` row: `resource.json` compared as a parsed JSON document, member order and whitespace ignored, every value equal, no member excluded; every other file byte-equal |
| K3 | b | 2026-10-19 | The D-17 gate cuts the link from an `ApplyObserver` when the engine reports `lock_acquired`; D-11 unchanged (`master_active` refuses `OnlyB`, `Both`, `Neither`) |
| K4 | a | 2026-10-19 | The cut is `PUT toggleApproval/<id>/false` on gw-a |
| K5 | d | 2026-10-19 | Not reachable on the rig; the log matcher ships, unit-tested only; `COVERAGE.md` rows unchanged |
| K6 | a | 2026-10-19 | On the signal: end the wait, one resync to the master, report `sync_exhausted`, no second budget; never a master rollback |
| K7 | a | 2026-10-19 | A provider that cannot list is `unverifiable` (check `warn`, provider named). "Cannot list" is HTTP 501 and also HTTP 500 (the `remote` provider answer); any other non-2xx is an error |
| K8 | a | 2026-10-19 | The preflight skips a render-declared provider that answers 404, with a detail naming it; verify still requires it |
| K9 | a | 2026-10-19 | Secret names compare as exact, case-sensitive strings |
| V1 | as written | 2026-10-19 | `applied_backup_unsynced` / `deploy_backup_unsynced` / exit 6 |
| V2 | as written | 2026-10-19 | `data.backup` shape and reason precedence |
| V3 | as written | 2026-10-19 | Five backup events |
| V4 | as written | 2026-10-19 | Preflight ids `master_active`, `backup_read`, `secrets` (10 ids) |
| V5 | as written | 2026-10-19 | `--backup-timeout` default 300, range 1..=86400, poll 5 s |
| V6 | as written | 2026-10-19 | `deploy verify --node` master, backup or both |
| V7 | as written | 2026-10-19 | Secret-name check for `--node backup` and `--node both`; `data.secrets` shape |
| V8 | as written | 2026-10-19 | `ign redundancy status --peer <PROFILE>` adds `data.active_node` |
| V9 | as written | 2026-10-19 | `projectState` recorded, not a convergence condition |
| V10 | as written | 2026-10-19 | No backup phase for `noop` or `rolled_back` |

## Live gates (24-10)

Rig brought up again for these gates (the 24-02 rig was torn down): `setup-pair.sh up && mint && pair && mode`, run 2026-10-21T00:21:11Z to 00:30:13Z (up 173 s, mint 55 s, pair 123 s, mode 191 s; the first `up` also pulled the 8.3.6 image). Settled at 00:30:13Z: `gw-a Master Active true false ... Good production`, `gw-b Backup Cold true false ... Good production`. Trial licenses on the new rig last about 2 hours from the first start (about 02:22Z). Tokens are node-local: `IGN_PAIR_TOKEN_A` answers on gw-a, `IGN_PAIR_TOKEN_B` on gw-b (values live only in `~/.cache/ign-pair-rig/tokens.env`). Every gate reaches gw-b only through `MethodTap` (a loopback tap that records method and path and refuses to forward anything but GET and HEAD); the counts below are the tap's.

Common command (values of the variables not shown):

~~~
. ~/.cache/ign-pair-rig/tokens.env
IGNITION_LIVE_PAIR=1 IGN_PAIR_URL_A=http://localhost:18300 IGN_PAIR_URL_B=http://localhost:18301 \
  IGN_PAIR_DATA_A="$HOME/.cache/ign-pair-rig/data-a" IGN_PAIR_DATA_B="$HOME/.cache/ign-pair-rig/data-b" \
  cargo test -p ignition-core --test live_deploy_pair <gate> -- --ignored --nocapture --test-threads=1
~~~

### live_pair_converges

Command: the common command with `<gate>` = `live_pair_converges`. UTC start 2026-10-21T01:12:13Z, end 01:12:31Z (test `finished in 40.54s` including the process start). Result: PASS.

First attempt (01:10:36Z) failed with `IncompleteMessage` on the first request through the tap: the tap resolved `localhost` to `::1` first and the rig publishes `127.0.0.1` only. The tap now takes the first resolved address that accepts; nothing else changed.

Verdict fields:

~~~
plan: added 0 changed 2 removed 0
apply: outcome Applied ok true run 20261021T011213Z-506174 in 8 s; backup converged true reason None waited_s 0 budget_s 120 files_checked 4 resync None
apply events: run_started backup_sampled gwbk_downloaded staged lock_acquired committed lock_verdict scan_done verify_done finalized backup_wait_started backup_poll backup_verdict
backup check Redundancy: Pass
backup check Mode: Pass
backup check ReadBack: Pass
backup check Files: Pass
tap after apply: 8 requests (GET 8)
verify --node both: ok true slug None backup converged true files_checked 4 secrets nodes 2
tap total: 12 requests (GET 12)
cleanup: pair settled after 0 s
~~~

`converged: true`, `files_checked` 4 (the tracer's four files compared with gw-a's), the `files` check `pass`, the backup's `lastSyncTimestamp` had already advanced by the time the first poll ran (`waited_s` 0, no resync), tap counts GET 12, POST 0, PUT 0, DELETE 0.
