# Phase 23 — Live Captures (8.3.6 rig)

| Field | Value |
| --- | --- |
| Image | inductiveautomation/ignition:8.3.6 |
| Compose variant | bind (compose.yml + compose.bind.yml) |
| Bind path (outside repo) | $HOME/.cache/ign-deploy-rig/data/config/resources |
| Gateway name (compose -n) | ign-deploy-rig |
| deploymentMode | production |
| Rig started (UTC) | 2026-10-16T14:05:35Z |
| Runner | capture23.sh (this directory: crates/ignition-core/tests/fixtures/live-rig-deploy) |
| ign --version | ign 1.5.0 |

Rule: every captured block is preceded by the exact command and a UTC time taken by the driver just before it. A request line names the token variable it used, never the token. Tokens live in a 0600 file outside the repo.

## Windows capture host (23-01)

| Field | Value |
| --- | --- |
| State | `pending` |
| Host label | none (`ign-win-836` in resource group `rg-ign-cli-win-test` is gone; no replacement named) |
| Account | `svc-ignition-deploy` (Modify on `data\config\resources\production`, granted 2026-10-15, now unreachable) |
| Subscription or site | "Microsoft Azure Sponsorship" (default subscription); the two other subscriptions are inaccessible from the agent session (one needs `az login`, one is `AuthorizationFailed`) |
| Recorded (UTC) | 2026-10-16 |
| Answer | `pending` (user, relayed by the orchestrator at the Task 1 checkpoint): the host will come later |

Facts established for this answer, all read-only: `az vm show -d -g rg-ign-cli-win-test -n ign-win-836` still returns `ResourceGroupNotFound` in subscription "Microsoft Azure Sponsorship" (confirmed by the orchestrator on 2026-10-16); no `IGN_WINVM_*` or `IGN_WFLWVM01_*` variable is set in the executor's environment; the pinned host-key file `~/.cache/ign-wflwvm01/known_hosts` still exists. No `ssh` was attempted: there is no host to reach. No key text is recorded here.

Matching row of `## Windows host outcome routing` in 23-01-PLAN.md, quoted:

> | `pending` | 23-01 Tasks 2-3, 23-02..23-07, 23-18, 23-19, 23-17, 23-09, 23-11, 23-12 | the 23-08 Task 1 precondition (an unmet precondition halts with "Blocked by") | paused, not complete. When the host exists, the user exports the env and the executor re-runs the Task 1 verification (`az vm show`, `ssh ... pwd`), appends a dated `ready` line to `## Windows capture host (23-01)`, and execution resumes at 23-08. A host recorded `ready` that stops answering by 23-08 is treated as `pending` | one OPEN line: host pending, stop point 23-08 |

Stop point: the 23-08 Task 1 precondition. Plans that will not run until a host is recorded `ready`: 23-08, 23-20, 23-10, 23-13, 23-21, 23-14, 23-15, 23-16. D-02's re-measure (23-10), C-script-run and C7b (23-08) stay owed in the table below.

## Captures Owed

Capture discipline: each row is recorded here, from a live 8.3.6 rig or the Windows host, before the code that depends on it is written.

| ID | Question | Needs | Blocks | Plan |
| --- | --- | --- | --- | --- |
| C2 | (a) with the lock held by token A, does an API resource write by token B succeed? (b) does a Designer or gateway-UI save to the active mode collection succeed (manual)? (c) does a file written into the mode collection with no lock load without a scan request, and how soon? (d) the same under a held lock? (e) when a lock lapses with unscanned files, does the gateway scan on its own? | Local rig; (b) a human | The meaning of every fault-injection assertion; whether the Designer preflight is the only guard (23-05, 23-17) | 23-01 |
| C11 | A tag provider and its definitions written together: loaded in one scan or two? | Local rig | Whether the engine needs a second scan (23-05, 23-17) | 23-01 |
| C-restart | Live status and body of `GET /data/api/v1/restart-tasks/pending` (nothing pending, a pending entry, a token without permission) | Local rig | DAPL-10 `restart_pending`; the `DeployApi` method and `DEPLOY_ENDPOINTS` row (23-09) | 23-01 |
| C-gwbk | Is a `.gwbk` a ZIP; size and time on the rig; Content-Length; how a truncated copy fails | Local rig | D-13 truncated-download check (23-02 checkpoint, 23-09) | 23-01 |
| C-sysname | REST `gateway-info.name` against the on-disk `systemName` of `ignition/system-properties` | Local rig | The wrong-target rule chosen at the 23-02 checkpoint; the `wrong_target` preflight (23-07) | 23-01 |
| C-script-run | Does `& ([ScriptBlock]::Create(ReadAllText(file)))` run under the host's execution policy; the `-File` alternative; PowerShell 5.1 `Move-Item -Force` and `File.Replace` behaviour inside one script | Windows host | The PowerShell script design (23-08, 23-20) | 23-08 |
| C7b | ACL after a same-volume move against the collection's ACL; sharing violation on a loaded JDBC jar; Defender cost; long paths; Linux file modes | Windows host (and a Linux host if Linux ssh apply is in scope) | The ACL and mode steps of `commit.ps1` (23-08) | 23-08 |
| OQ5b | p95 of connect, script and scan with the batched commit at 152 and 1,520 files; derives `HOLD_S` and the per-file slope (research name OQ5-rerun) | Windows host, after the script exists | D-02, D-03, D-03a; `HOLD_S`, `HoldModel::MEASURED` (23-10, 23-13) | 23-10 |

## C-sysname

The wrong-target rule can compare the REST gateway-info `name` with the host `systemName` (research Open Question 1). Both are read from one fresh rig: the REST answer, the value of `systemName` in the bind-mounted file of the core collection, and the same value through the REST singleton read.

#### GET gateway-info (token A)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/gateway-info'` @ 2026-10-16T14:12:45Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:12:45 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 416

{"name":"ign-deploy-rig","redundancyRole":"Independent","edition":"standard","hostname":"localhost","port":"18290","ignitionVersion":"8.3.6 (b2026042713)","deploymentMode":"production","timeZone":"Coordinated Universal Time","timeZoneId":"Etc/UTC","jvmVersion":"17.0.18","allowUnsignedModules":false,"license":{"mode":"Trial","validForVersion":8,"expirationDate":"Oct 16, 2026, 4:09:04 PM","licenseRestrictions":[]}}
[curl http=200 total=0.035008s]
~~~

#### systemName in core/ignition/system-properties/config.json (the one value, not the file)

`python3 -I -c import\ json\;\ d\ =\ json.load\(open\(\"core/ignition/system-properties/config.json\"\)\)\;\ print\(json.dumps\(\{\"systemName\":\ d.get\(\"systemName\"\,\ \"\<absent\>\"\)\}\)\) ` @ 2026-10-16T14:12:45Z

~~~
{"systemName": "ign-deploy-rig"}
[exit 0]
~~~

#### which collections carry an ignition/system-properties directory on the bind mount

`bash -c ls\ -d\ \*/ignition/system-properties\ 2\>/dev/null\ \|\|\ echo\ \"\<none\>\" ` @ 2026-10-16T14:12:45Z

~~~
core/ignition/system-properties
[exit 0]
~~~

#### GET resources/singleton/ignition/system-properties (the REST view, defaultIfUndefined)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/resources/singleton/ignition/system-properties?defaultIfUndefined=true'` @ 2026-10-16T14:12:45Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:12:46 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 2078

{
  "type": "ignition/system-properties",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "203974186b707390051f4b68286894cc78f85b5427dfb7a75edd5214f45891fc",
  "config": {
    "systemName": "ign-deploy-rig",
    "designerRAM": "MB1024",
    "disable3DPipeline": true,
    "disableDirectDraw": false,
    "useCondensedDialogFont": false,
    "homepageUrl": "/app",
    "homepageNotes": "Welcome to your Ignition Gateway. This customizable area is intended to capture the critical details you want to document for this Gateway. You might include information about the environment (production, testing, or development), key workflows, or maintenance schedules. It\u0027s also a great place to list instructions for your team, important reminders about system health, or details about your configuration that require regular review. Use this space to keep everyone informed and ensure smooth operations across the board.",
    "clientFallbackEnabled": false,
    "secondsBeforeFailover": 60,
    "scheduledBackupsEnabled": false,
    "scheduledBackupTimes": "15 1 * * *",
    "scheduledBackupRetentionCount": 5,
    "scheduledBackupFilename": "${gatewayName}_Ignition-backup-${edition}${timestamp}.gwbk",
    "automaticThreadDumpsEnabled": false,
    "automaticThreadDumpsIntervalSeconds": 60,
    "automaticThreadDumpsCpuThreshold": 70,
    "automaticThreadDumpsCpuThresholdExceededDuration": 10,
    "automaticThreadDumpsRetentionCount": 50,
    "multicastEnabled": true,
    "multicastIPAddr": "231.1.1.1",
    "multicastSendPort": "4445",
    "multicastReceivePort": "4446",
    "multicastIntervalMillis": "5000",
    "enableTagReferenceStore": true,
    "scriptMessageEncoding": "Protobuf"
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "lastModification": {
      "actor": "unknown",
      "timestamp": "2026-10-16T14:07:15Z"
    },
    "lastModificationSignature": "51201c11a3fa0bc2ec273b93d7830ca24560aaa374e195b8b242bc7fbc8e65ec"
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.182373s]
~~~

## C-restart

GET restart-tasks/pending (DAPL-10, DAPI-06). The OpenAPI declares `{"pending": [string]}`. Recorded: nothing pending; with no token and with a token that holds no security level; then after a restart-requiring change the rig can make (a module disabled through the REST toggle-state route); then after the change is reversed.

### C-restart-1 — nothing pending

#### GET restart-tasks/pending, token A (Authenticated > Deploy)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/restart-tasks/pending'` @ 2026-10-16T14:12:46Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:12:46 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 14

{"pending":[]}
[curl http=200 total=0.008819s]
~~~

#### GET restart-tasks/pending, token B

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_B] '$BASE/data/api/v1/restart-tasks/pending'` @ 2026-10-16T14:12:46Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:12:46 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 14

{"pending":[]}
[curl http=200 total=0.005807s]
~~~

### C-restart-2 — a token without the permission, and no token

#### GET restart-tasks/pending, token cap-ro (an API token with no security levels)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_RO] '$BASE/data/api/v1/restart-tasks/pending'` @ 2026-10-16T14:12:46Z

~~~
HTTP/1.1 401 Unauthorized
Date: Fri, 16 Oct 2026 14:12:46 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 349

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 401</title>
</head>
<body><h2>HTTP ERROR 401 Unauthorized</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/restart-tasks/pending</td></tr>
<tr><th>STATUS:</th><td>401</td></tr>
<tr><th>MESSAGE:</th><td>Unauthorized</td></tr>
</table>

</body>
</html>

[curl http=401 total=0.007618s]
~~~

#### GET restart-tasks/pending, no token

`curl -sS -g -i -X GET [no token] '$BASE/data/api/v1/restart-tasks/pending'` @ 2026-10-16T14:12:46Z

~~~
HTTP/1.1 401 Unauthorized
Date: Fri, 16 Oct 2026 14:12:46 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 349

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 401</title>
</head>
<body><h2>HTTP ERROR 401 Unauthorized</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/restart-tasks/pending</td></tr>
<tr><th>STATUS:</th><td>401</td></tr>
<tr><th>MESSAGE:</th><td>Unauthorized</td></tr>
</table>

</body>
</html>

[curl http=401 total=0.005220s]
~~~

### C-restart-3 — a restart-requiring change

#### GET modules/healthy (is the candidate module present?)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/modules/healthy'` @ 2026-10-16T14:12:46Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:12:46 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 12815

{"items":[{"id":"com.inductiveautomation.opcua.drivers.tcpudp","onStartup":"enabled","shouldUpgrade":false,"name":"UDP and TCP Drivers","version":"7.3.6 (b2026042713)","description":"Drivers supporting communication with UDP or TCP packets.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.555963202"},{"id":"com.inductiveautomation.opcua.drivers.omron","onStartup":"enabled","shouldUpgrade":false,"name":"Omron Driver","version":"5.3.6 (b2026042713)","description":"A driver supporting communication with Omron devices.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.556019785"},{"id":"com.inductiveautomation.sfc","onStartup":"enabled","shouldUpgrade":false,"name":"SFC","version":"6.3.6 (b2026042713)","description":"A module enabling the use of Sequential Function Charts to execute and visualize customized logic.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:51.396361950"},{"id":"com.inductiveautomation.sqlbridge","onStartup":"enabled","shouldUpgrade":false,"name":"SQL Bridge","version":"11.3.6 (b2026042713)","description":"A module allowing OPC-to-SQL data logging and transaction management.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.406553856"},{"id":"com.inductiveautomation.jdbc.mariadb","onStartup":"enabled","shouldUpgrade":false,"name":"MariaDB JDBC Driver","version":"1.3.6 (b2026042713)","description":"A JDBC driver providing MariaDB database connectivity.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Free","startupTime":"2026-10-16T14:11:51.423919007"},{"id":"com.inductiveautomation.opcua.drivers.bacnet","onStartup":"enabled","shouldUpgrade":false,"name":"BACnet Driver","version":"3.3.6 (b2026042713)","description":"A driver supporting communication with BACnet devices.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.647508252"},{"id":"com.inductiveautomation.eventstream","onStartup":"enabled","shouldUpgrade":false,"name":"Event Streams","version":"1.3.6 (b2026042713)","description":"A module providing source and handler combinations to receive, filter, and transform event-driven data.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:51.429564203"},{"id":"com.inductiveautomation.jdbc.postgresql","onStartup":"enabled","shouldUpgrade":false,"name":"PostgreSQL JDBC Driver","version":"1.3.6 (b2026042713)","description":"A JDBC driver providing PostgreSQL database connectivity.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Free","startupTime":"2026-10-16T14:11:51.440642634"},{"id":"com.inductiveautomation.opcua.drivers.siemens","onStartup":"enabled","shouldUpgrade":false,"name":"Siemens Drivers","version":"7.3.6 (b2026042713)","description":"Drivers supporting communication with Siemens S7-300, S7-400, S7-1200, and S7-1500 devices.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.648625091"},{"id":"com.inductiveautomation.sms-notification","onStartup":"enabled","shouldUpgrade":false,"name":"SMS Notification","version":"7.3.6 (b2026042713)","description":"A module providing SMS notification functionality to Alarming.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.667130561"},{"id":"com.inductiveautomation.opcua.drivers.mitsubishi","onStartup":"enabled","shouldUpgrade":false,"name":"Mitsubishi Driver","version":"2.3.6 (b2026042713)","description":"A driver supporting communication with Mitsubishi devices.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.648780634"},{"id":"com.inductiveautomation.opcua.drivers.micro800","onStartup":"enabled","shouldUpgrade":false,"name":"Micro800 Driver","version":"2.3.6 (b2026042713)","description":"A driver supporting communication with Allen-Bradley Micro800 series devices.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.651185729"},{"id":"com.inductiveautomation.opcua.drivers.modbus","onStartup":"enabled","shouldUpgrade":false,"name":"Modbus Driver","version":"8.3.6 (b2026042713)","description":"A driver supporting communication with Modbus-TCP devices.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.654688622"},{"id":"com.inductiveautomation.opcua.drivers.siemens-symbolic","onStartup":"enabled","shouldUpgrade":false,"name":"Siemens Enhanced Driver","version":"1.3.6 (b2026042713)","description":"A driver supporting symbolic tag access for S7-1200 and S7-1500 devices and absolute addressing for S7-300, S7-400, S7-1200, and S7-1500 devices.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.656242505"},{"id":"com.inductiveautomation.opcua.drivers.ablegacy","onStartup":"enabled","shouldUpgrade":false,"name":"Allen-Bradley Driver","version":"7.3.6 (b2026042713)","description":"An Allen-Bradley driver suite for the OPC UA module.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.656764050"},{"id":"com.inductiveautomation.perspective","onStartup":"enabled","shouldUpgrade":false,"name":"Perspective","version":"3.3.6 (b2026042713)","description":"A module providing modern, responsive HTML-based graphical interfaces for Ignition projects.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.083643041"},{"id":"com.inductiveautomation.reporting","onStartup":"enabled","shouldUpgrade":false,"name":"Reporting","version":"7.3.6 (b2026042713)","description":"A module providing reporting capabilities.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.524263873"},{"id":"com.inductiveautomation.phone-notification","onStartup":"enabled","shouldUpgrade":false,"name":"Voice Notification","version":"7.3.6 (b2026042713)","description":"A module providing voice notification functionality to Alarming.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.773789105"},{"id":"com.inductiveautomation.connectors.kafka","onStartup":"enabled","shouldUpgrade":false,"name":"Kafka Connector","version":"1.3.6 (b2026042713)","description":"A module enabling communication with Kafka.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.412170551"},{"id":"com.inductiveautomation.symbol-factory","onStartup":"enabled","shouldUpgrade":false,"name":"Symbol Factory","version":"8.3.6 (b2026042713)","description":"A library containing vector graphics for use in Ignition projects.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.152657976"},{"id":"com.inductiveautomation.alarm-notification","onStartup":"enabled","shouldUpgrade":false,"name":"Alarm Notification","version":"7.3.6 (b2026042713)","description":"A module providing email notification functionality to Alarming.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.465621991"},{"id":"com.inductiveautomation.webdev","onStartup":"enabled","shouldUpgrade":false,"name":"WebDev","version":"6.3.6 (b2026042713)","description":"A module enabling users to create and serve basic web resources and respond to web requests through scripting.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.506648367"},{"id":"com.inductiveautomation.jdbc.mssql","onStartup":"enabled","shouldUpgrade":false,"name":"MSSQL JDBC Driver","version":"1.3.6 (b2026042713)","description":"A JDBC driver providing MSSQL database connectivity.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Free","startupTime":"2026-10-16T14:11:52.152797060"},{"id":"com.inductiveautomation.opcua.drivers.iec61850","onStartup":"enabled","shouldUpgrade":false,"name":"IEC 61850 Driver","version":"2.3.6 (b2026042713)","description":"A driver supporting communication with IEC 61850 devices.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.657927431"},{"id":"com.inductiveautomation.opcua","onStartup":"enabled","shouldUpgrade":false,"name":"OPC-UA","version":"10.3.6 (b2026042713)","description":"A module providing OPC UA client and server functionality.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.212316073"},{"id":"com.inductiveautomation.eam","onStartup":"enabled","shouldUpgrade":false,"name":"Enterprise Administration","version":"5.3.6 (b2026042713)","description":"A module providing a remote Gateway administration system to manage Gateways and automate tasks from a single controller.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.535458722"},{"id":"com.inductiveautomation.opcua.drivers.dnp3","onStartup":"enabled","shouldUpgrade":false,"name":"Legacy DNP3 Driver","version":"5.3.6 (b2026042713)","description":"A legacy driver supporting communication with DNP3 (Distributed Network Protocol) devices.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.661898284"},{"id":"com.inductiveautomation.opcua.drivers.logix","onStartup":"enabled","shouldUpgrade":false,"name":"Logix Driver","version":"6.3.6 (b2026042713)","description":"A driver supporting communication with Allen-Bradley Logix5000 series devices.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.666757101"},{"id":"com.inductiveautomation.historian","onStartup":"enabled","shouldUpgrade":false,"name":"Historian Core","version":"1.3.6 (b2026042713)","description":"Adds powerful time-series historian functionality to Ignition.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.363373802"},{"id":"com.inductiveautomation.historian.sql","onStartup":"enabled","shouldUpgrade":false,"name":"SQL Historian","version":"1.3.6 (b2026042713)","description":"Enables time-series historian functionality to any SQL database within Ignition.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.667090561"},{"id":"com.inductiveautomation.vision","onStartup":"enabled","shouldUpgrade":false,"name":"Vision","version":"12.3.6 (b2026042713)","description":"A module providing web-launched HMI/SCADA Clients.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.376033867"},{"id":"com.inductiveautomation.opcua.drivers.dnp3v2","onStartup":"enabled","shouldUpgrade":false,"name":"DNP3 Driver","version":"2.3.6 (b2026042713)","description":"A driver supporting communication with DNP3 (Distributed Network Protocol) devices.","vendorId":-1,"vendorName":"Inductive Automation, LLC","selfSigned":false,"state":"ACTIVE","licenseState":"Trial","startupTime":"2026-10-16T14:11:52.666888935"}],"metadata":{"total":32,"matching":32,"limit":-1,"offset":0}}
[curl http=200 total=0.039620s]
~~~

#### PUT modules/toggle-state: disable com.inductiveautomation.sfc

`curl -sS -g -i -X PUT [token $IGN_TOKEN_CAP_A] -H Content-Type: application/json --data {"disable":["com.inductiveautomation.sfc"]} '$BASE/data/api/v1/modules/toggle-state'` @ 2026-10-16T14:12:46Z

~~~
HTTP/1.1 500 Server Error
Date: Fri, 16 Oct 2026 14:12:46 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Length: 0


[curl http=500 total=0.033870s]
~~~

#### GET restart-tasks/pending after the disable

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/restart-tasks/pending'` @ 2026-10-16T14:12:49Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:12:49 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 60

{"pending":["Module Disabled: com.inductiveautomation.sfc"]}
[curl http=200 total=0.019959s]
~~~

### C-restart-4 — the change reversed

#### PUT modules/toggle-state: enable com.inductiveautomation.sfc

`curl -sS -g -i -X PUT [token $IGN_TOKEN_CAP_A] -H Content-Type: application/json --data {"enable":["com.inductiveautomation.sfc"]} '$BASE/data/api/v1/modules/toggle-state'` @ 2026-10-16T14:12:49Z

~~~
HTTP/1.1 500 Server Error
Date: Fri, 16 Oct 2026 14:12:49 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Length: 0


[curl http=500 total=0.047369s]
~~~

#### GET restart-tasks/pending after the enable

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/restart-tasks/pending'` @ 2026-10-16T14:12:52Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:12:52 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 60

{"pending":["Module Disabled: com.inductiveautomation.sfc"]}
[curl http=200 total=0.024002s]
~~~

## Decisions

Each entry is an answer taken from the blocks above, not a hypothesis. Later plans read these entries by id.

### C-sysname

Rests on `## C-sysname`.

- On a fresh 8.3.6 rig the REST `GET /data/api/v1/gateway-info` `name` is `ign-deploy-rig` and the on-disk `systemName` in `core/ignition/system-properties/config.json` is `ign-deploy-rig`. They agree, verbatim. The REST singleton read of `ignition/system-properties` shows the same `config.systemName` (`ign-deploy-rig`).
- The file lives in the `core` collection only (no other collection on the bind mount carries `ignition/system-properties`). The rig was started with `-n ign-deploy-rig`, so commissioning wrote that name into both places; the value on a hand-built gateway is whatever its installer chose (the corpus fixture shows `whk-development-gw`).
- So a wrong-target binding can compare REST `gateway-info.name` with the host `systemName`, read from `core/ignition/system-properties/config.json` (or the effective view of that singleton) with one scoped `TreeReader::read`. Both are plain strings.
- Limit: this capture does not show the two diverging. Whether `gateway-info.name` follows a later edit of `systemName` without a restart was not probed, and a stale on-disk file would not be seen by the gateway. The 23-02 checkpoint chooses the rule knowing the two agree at rest.

### C-restart

Rests on `## C-restart`.

- Nothing pending: `GET /data/api/v1/restart-tasks/pending` answers `200`, `Content-Type: application/json`, `Content-Length: 14`, body `{"pending":[]}`. Token A and token B (both `Authenticated > Deploy`) get the same answer.
- With a pending entry: the shape is the OpenAPI shape. After `PUT /data/api/v1/modules/toggle-state` with `{"disable":["com.inductiveautomation.sfc"]}` the endpoint answered `200`, `Content-Length: 60`, `{"pending":["Module Disabled: com.inductiveautomation.sfc"]}`. An object with one key `pending`, an array of free-text strings of the form `Module <State>: <module id>`; nothing else in the body. The entry was produced on the rig, so FA-23-01 is closed: the non-empty case is captured, not inferred from the OpenAPI.
- The toggle route itself answered `500` with an empty body (`Content-Length: 0`) for the disable, yet the change took effect (the pending entry appeared 3 s later). The reverse `enable` also answered `500` empty, and the pending entry stayed: a reversal does not clear it. A pending entry persists until the gateway restarts; "new since the run began" must therefore be computed by comparing the list before and after, not by reading it once.
- No permission: a token that holds no security level (`cap-ro`, loaded by the gateway and listed by `resources/names/ignition/api-token`) and a request with no token both get `401 Unauthorized`, a Jetty HTML page (`Content-Type: text/html;charset=iso-8859-1`, `Content-Length: 349`, `HTTP ERROR 401`), not JSON. The route is therefore not reached; `cap-ro` also gets 401 on `gateway-info`. A `403` from this route was not produced on this rig. The `restart_pending_unavailable` warning (23-02 checkpoint item 9) should treat any non-200 answer of the route (401, 403, other) as "unavailable", never as a failure.
- Consequence for 23-09: a `DeployApi::restart_pending` method and a `DEPLOY_ENDPOINTS` row `restart_pending` may now be written; the declared shape is `{"pending": [string]}`, an HTML body on a refusal.
- Rig state after this capture: SFC shows as disabled pending a restart. The rig is disposable.
