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
