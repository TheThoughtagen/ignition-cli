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
| State | `ready` (2026-10-17, see the dated line below; was `pending` on 2026-10-16) |
| Host label | none (`ign-win-836` in resource group `rg-ign-cli-win-test` is gone; no replacement named) |
| Account | `svc-ignition-deploy` (Modify on `data\config\resources\production`, granted 2026-10-15, now unreachable) |
| Subscription or site | "Microsoft Azure Sponsorship" (default subscription); the two other subscriptions are inaccessible from the agent session (one needs `az login`, one is `AuthorizationFailed`) |
| Recorded (UTC) | 2026-10-16 |
| Answer | `pending` (user, relayed by the orchestrator at the Task 1 checkpoint): the host will come later |

Facts established for this answer, all read-only: `az vm show -d -g rg-ign-cli-win-test -n ign-win-836` still returns `ResourceGroupNotFound` in subscription "Microsoft Azure Sponsorship" (confirmed by the orchestrator on 2026-10-16); no `IGN_WINVM_*` or `IGN_WFLWVM01_*` variable is set in the executor's environment; the pinned host-key file `~/.cache/ign-wflwvm01/known_hosts` still exists. No `ssh` was attempted: there is no host to reach. No key text is recorded here.

Matching row of `## Windows host outcome routing` in 23-01-PLAN.md, quoted:

> | `pending` | 23-01 Tasks 2-3, 23-02..23-07, 23-18, 23-19, 23-17, 23-09, 23-11, 23-12 | the 23-08 Task 1 precondition (an unmet precondition halts with "Blocked by") | paused, not complete. When the host exists, the user exports the env and the executor re-runs the Task 1 verification (`az vm show`, `ssh ... pwd`), appends a dated `ready` line to `## Windows capture host (23-01)`, and execution resumes at 23-08. A host recorded `ready` that stops answering by 23-08 is treated as `pending` | one OPEN line: host pending, stop point 23-08 |

Stop point: the 23-08 Task 1 precondition. Plans that will not run until a host is recorded `ready`: 23-08, 23-20, 23-10, 23-13, 23-21, 23-14, 23-15, 23-16. D-02's re-measure (23-10), C-script-run and C7b (23-08) stay owed in the table below.

**2026-10-17 `ready`** (user, relayed by the orchestrator: "We will need to set one up", then "I provision via az"). The host was rebuilt with the az CLI under the same names. Task 1 verification, run 2026-10-17 by the orchestrator:

| Field | Value |
| --- | --- |
| VM | `ign-win-836` (computer `ignwin836`), resource group `rg-ign-cli-win-test`, eastus, Standard_D2s_v4, Windows Server 2022 Datacenter g2. `az vm show -d` reports `VM running`, public IP `52.154.75.231` |
| Inbound | NSG allows TCP 22 and 8088 from the operator's /32 only |
| Gateway | Ignition 8.3.6 (b2026042713), standard edition, installed from the Windows zip as the `Ignition` service. `deploymentMode` = `production`: the mode was created with `POST /data/api/v1/mode`, then the gateway was restarted. Trial reset 2026-10-17 (it lasts 2 h; reset again with an authed `POST /data/api/v1/trial`) |
| Account | `ignwin836\svc-ignition-deploy`, member of Users only, key login only. Explicit `(OI)(CI)(RX)` on `data\config\resources`, plus `(OI)(CI)(M)` on `data\config\resources\production` (granted 2026-10-17 by the exact icacls command in 22-12 deviation 3). No write ACE anywhere else under `data\` |
| sftp `pwd` | `Remote working directory: /C:/Users/svc-ignition-deploy` |
| ssh `whoami & cd` | `ignwin836\svc-ignition-deploy` / `C:\Users\svc-ignition-deploy` (Win32-OpenSSH 8.1p1, default shell cmd.exe) |
| Host key | ED25519 `SHA256:Izw8z+3YPObb46VCy/CmWITrfunmruC2TuytpQhfOPA`. The trusted-channel (run-command) read matched ssh-keyscan, and the key is pinned in `~/.cache/ign-wflwvm01/known_hosts` (mode 0600). It replaces the 22-CONTEXT A-01 fingerprint, which belonged to the deleted host |
| Profile | `azure-win` -> `http://52.154.75.231:8088`, adopted (API key `ign-cli`, OS keyring). `ign --profile azure-win status` reports RUNNING |
| Defender | real-time protection on, no exclusions |

The `IGN_WINVM_*` and `IGN_WFLWVM01_*` values are exported from the 0600 files under `~/.cache/ign-wflwvm01/`. No key text or password is recorded here. Execution resumes at 23-08.

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

Status 2026-10-17 (23-08): C-script-run and C7b are captured on the rebuilt Windows host; see `## C-script-run`, `## C7b` and their entries under `## Decisions`. The C7b rows for Linux file modes and Defender with an exclusion are not taken (no Linux ssh host in scope; the Defender configuration is not readable by the deploy account).

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

## C2

What the config scan-lock blocks (research Pitfall 1, Phase 20 D1/D2/D4/D8/D12). Token A holds the lock; token B and the host filesystem are the other actors. Sub-probes (a), (c), (d), (e); (b), a Designer or gateway-UI save under a held lock, is the manual section C2b. Each sub-probe restores the rig afterwards.

### C2a — token A holds the lock (hold 60 s); token B creates a resource through the API

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:22:25Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:22:25 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1792160508037,
  "lastScanDuration": 276
}
[curl http=200 total=0.005229s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:22:26Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:22:26 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1792160545746,
  "lastScanDuration": 358
}
[curl http=200 total=0.004634s]
~~~

#### the request body of the write by token B

`cat /var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.NeNVhHDHE6/c2a-body.json ` @ 2026-10-16T14:22:26Z

~~~
[{"name":"C2_B_WRITE","config":{"connectURL":"jdbc:postgresql://mode-host:5432/spike","driver":"PostgreSQL","translator":"POSTGRES","username":"spike","validationQuery":"SELECT 1","poolMaxActive":8}}]
[exit 0]
~~~

#### POST scan-lock/config by token A (acquire 5, hold 60)

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/json --data {"acquireTimeout":5,"holdTimeout":60} '$BASE/data/api/v1/scan-lock/config'` @ 2026-10-16T14:22:26Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:22:26 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "timestamp": 11148005,
  "remaining": 59,
  "actor": "api-token:cap-a:192.168.147.1"
}
[curl http=200 total=0.014621s]
~~~

#### GET scan-lock/config by token B, before its write

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_B] '$BASE/data/api/v1/scan-lock/config'` @ 2026-10-16T14:22:26Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:22:26 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "timestamp": 11148005,
  "remaining": 59,
  "actor": "api-token:cap-a:192.168.147.1"
}
[curl http=200 total=0.009977s]
~~~

#### POST resources/ignition/database-connection by token B while A holds the lock

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_B] -H Content-Type: application/json --data @/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.NeNVhHDHE6/c2a-body.json '$BASE/data/api/v1/resources/ignition/database-connection'` @ 2026-10-16T14:22:26Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:22:26 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 263

{
  "success": true,
  "changes": [
    {
      "name": "C2_B_WRITE",
      "type": "ignition/database-connection",
      "collection": "core",
      "newSignature": "f857151f14ce1f1924531491e1ba15da7a294bd92094ba1463d52371665a17d1"
    }
  ],
  "problem": null
}
[curl http=200 total=59.943446s]
~~~

#### GET scan-lock/config by token A, after the write

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan-lock/config'` @ 2026-10-16T14:23:26Z

~~~
HTTP/1.1 204 No Content
Date: Fri, 16 Oct 2026 14:23:26 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block


[curl http=204 total=0.012304s]
~~~

#### GET resources/list/ignition/database-connection (is C2_B_WRITE in the view?)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/resources/list/ignition/database-connection?limit=-1'` @ 2026-10-16T14:23:26Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:23:26 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 8363

{"items":[{"type":"ignition/database-connection","name":"C2_B_WRITE","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"f857151f14ce1f1924531491e1ba15da7a294bd92094ba1463d52371665a17d1","config":{"driver":"PostgreSQL","translator":"POSTGRES","includeSchemaInTableName":false,"connectURL":"jdbc:postgresql://mode-host:5432/spike","username":"spike","connectionProps":"","connectionResetParams":"","defaultTransactionLevel":"DEFAULT","poolInitSize":0,"poolMaxActive":8,"poolMaxIdle":8,"poolMinIdle":0,"poolMaxWait":5000,"validationQuery":"SELECT 1","testOnBorrow":true,"testOnReturn":false,"testWhileIdle":false,"evictionRate":-1,"evictionTests":3,"evictionTime":1800000,"failoverMode":"STANDARD","slowQueryLogThreshold":60000,"validationSleepTime":10000},"data":["config.json"],"attributes":{"uuid":"ef0dc841-f5a9-4c8c-9cbb-a87a31e3d324","enabled":true},"metrics":{"active-connections":{"name":"databases.connection-C2_B_WRITE.active-connections","metric":{"value":0.0,"numerator":0.0,"denominator":8.0,"type":"ratio-gauge","description":"Active Connections","units":""}},"active-queries":{"name":"databases.connection-C2_B_WRITE.active-queries","metric":{"value":[],"type":"gauge","description":"Active Queries","units":"queries"}},"expensive-queries":{"name":"databases.connection-C2_B_WRITE.expensive-queries","metric":{"value":[],"type":"gauge","description":"Expensive Queries","units":"queries"}},"queries":{"name":"databases.connection-C2_B_WRITE.queries","metric":{"oneMinuteRate":0.0,"fiveMinuteRate":0.0,"fifteenMinuteRate":0.0,"count":0,"meanRate":0.0,"histogram":{"max":0.0,"min":0.0,"p50":0.0,"p75":0.0,"p95":0.0,"p98":0.0,"p99":0.0,"mean":0.0,"bucketSize":0.0,"buckets":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"type":"histogram","stdev":0.0},"type":"timer"}},"rows":{"name":"databases.connection-C2_B_WRITE.rows","metric":{"oneMinuteRate":0.0,"fiveMinuteRate":0.0,"fifteenMinuteRate":0.0,"count":0,"meanRate":0.0,"type":"meter"}},"throughput":{"name":"databases.connection-C2_B_WRITE.throughput","metric":{"value":-1.0,"type":"gauge","description":"","units":"queries/sec"}}},"healthchecks":{"status":{"name":"databases.connection-C2_B_WRITE.status","result":{"healthy":false,"time":1792160606587,"message":"Faulted","error":{"message":"Cannot create PoolableConnectionFactory (The connection attempt failed.)","stacktrace":["java.sql.SQLException: Cannot create PoolableConnectionFactory (The connection attempt failed.)","\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:645)","\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)","\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)","\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)","\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)","\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)","\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)","\tat java.base/java.lang.Thread.run(Unknown Source)","Caused by: org.postgresql.util.PSQLException: The connection attempt failed.","\tat org.postgresql.core.v3.ConnectionFactoryImpl.openConnectionImpl(ConnectionFactoryImpl.java:358)","\tat org.postgresql.core.ConnectionFactory.openConnection(ConnectionFactory.java:54)","\tat org.postgresql.jdbc.PgConnection.<init>(PgConnection.java:273)","\tat org.postgresql.Driver.makeConnection(Driver.java:446)","\tat org.postgresql.Driver.connect(Driver.java:298)","\tat org.apache.commons.dbcp2.DriverConnectionFactory.createConnection(DriverConnectionFactory.java:52)","\tat org.apache.commons.dbcp2.PoolableConnectionFactory.makeObject(PoolableConnectionFactory.java:481)","\tat org.apache.commons.dbcp2.BasicDataSource.validateConnectionFactory(BasicDataSource.java:114)","\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:641)","\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)","\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)","\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)","\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)","\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)","\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)","\tat java.base/java.lang.Thread.run(Unknown Source)","Caused by: java.net.UnknownHostException: mode-host","\tat java.base/sun.nio.ch.NioSocketImpl.connect(Unknown Source)","\tat java.base/java.net.SocksSocketImpl.connect(Unknown Source)","\tat java.base/java.net.Socket.connect(Unknown Source)","\tat org.postgresql.core.PGStream.createSocket(PGStream.java:243)","\tat org.postgresql.core.PGStream.<init>(PGStream.java:98)","\tat org.postgresql.core.v3.ConnectionFactoryImpl.tryConnect(ConnectionFactoryImpl.java:136)","\tat org.postgresql.core.v3.ConnectionFactoryImpl.openConnectionImpl(ConnectionFactoryImpl.java:262)","\tat org.postgresql.core.ConnectionFactory.openConnection(ConnectionFactory.java:54)","\tat org.postgresql.jdbc.PgConnection.<init>(PgConnection.java:273)","\tat org.postgresql.Driver.makeConnection(Driver.java:446)","\tat org.postgresql.Driver.connect(Driver.java:298)","\tat org.apache.commons.dbcp2.DriverConnectionFactory.createConnection(DriverConnectionFactory.java:52)","\tat org.apache.commons.dbcp2.PoolableConnectionFactory.makeObject(PoolableConnectionFactory.java:481)","\tat org.apache.commons.dbcp2.BasicDataSource.validateConnectionFactory(BasicDataSource.java:114)","\tat org.apache.commons.dbcp2.BasicDataSource.createPoolableConnectionFactory(BasicDataSource.java:641)","\tat org.apache.commons.dbcp2.BasicDataSource.createDataSource(BasicDataSource.java:546)","\tat org.apache.commons.dbcp2.BasicDataSource.getConnection(BasicDataSource.java:723)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.getConnectionInternal(DatasourceImpl.java:373)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:326)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)","\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)","\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)","\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)","\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)","\tat java.base/java.lang.Thread.run(Unknown Source)"]}}}}}],"metadata":{"total":1,"matching":1,"limit":-1,"offset":0}}
[curl http=200 total=0.038227s]
~~~

### C2a — restore: scan, remove C2_B_WRITE from the bind mount, scan, post-state

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:23:26Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:23:26 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1792160545746,
  "lastScanDuration": 358
}
[curl http=200 total=0.005602s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:23:27Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:23:27 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1792160607061,
  "lastScanDuration": 368
}
[curl http=200 total=0.005037s]
~~~

#### every path of C2_B_WRITE on the bind mount before removal

`bash -c find\ .\ -path\ \"\*/database-connection/C2_B_WRITE\*\"\ \|\ LC_ALL=C\ sort ` @ 2026-10-16T14:23:27Z

~~~
./core/ignition/database-connection/C2_B_WRITE
./core/ignition/database-connection/C2_B_WRITE/config.json
./core/ignition/database-connection/C2_B_WRITE/resource.json
[exit 0]
~~~

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:23:27Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:23:27 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1792160607061,
  "lastScanDuration": 368
}
[curl http=200 total=0.005257s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:23:28Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:23:28 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1792160607640,
  "lastScanDuration": 164
}
[curl http=200 total=0.006196s]
~~~

#### GET scan-lock/config (post-state)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan-lock/config'` @ 2026-10-16T14:23:28Z

~~~
HTTP/1.1 204 No Content
Date: Fri, 16 Oct 2026 14:23:28 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block


[curl http=204 total=0.003743s]
~~~

#### GET resources/list/ignition/database-connection (post-state)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/resources/list/ignition/database-connection?limit=-1'` @ 2026-10-16T14:23:28Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:23:28 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 70

{"items":[],"metadata":{"total":0,"matching":0,"limit":-1,"offset":0}}
[curl http=200 total=0.017069s]
~~~

### C2c — no lock: a resource written into production/ with no lock and no scan request (poll every 5 s for 120 s)

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:23:28Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:23:28 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1792160607640,
  "lastScanDuration": 164
}
[curl http=200 total=0.010018s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:23:28Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:23:28 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1792160608501,
  "lastScanDuration": 168
}
[curl http=200 total=0.004215s]
~~~

#### C2_NOLOCK written into production/ (relative paths)

`bash -c find\ production\ -path\ \"\*C2_NOLOCK\*\"\ -type\ f\ \|\ LC_ALL=C\ sort ` @ 2026-10-16T14:23:29Z

~~~
production/ignition/database-connection/C2_NOLOCK/config.json
production/ignition/database-connection/C2_NOLOCK/resource.json
[exit 0]
~~~

#### poll 120 s: lastScanTimestamp, scanActive, visibility, lock, no scan request sent

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-deploy/capture23.sh c2poll 120 C2_NOLOCK ` @ 2026-10-16T14:23:29Z

~~~
t+0s 2026-10-16T14:23:29Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+5s 2026-10-16T14:23:34Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+10s 2026-10-16T14:23:39Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+16s 2026-10-16T14:23:45Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+21s 2026-10-16T14:23:50Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+27s 2026-10-16T14:23:57Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+33s 2026-10-16T14:24:02Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+38s 2026-10-16T14:24:07Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+43s 2026-10-16T14:24:12Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+48s 2026-10-16T14:24:17Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+53s 2026-10-16T14:24:23Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+59s 2026-10-16T14:24:28Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+64s 2026-10-16T14:24:33Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+69s 2026-10-16T14:24:38Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+74s 2026-10-16T14:24:43Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+79s 2026-10-16T14:24:48Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+84s 2026-10-16T14:24:53Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+89s 2026-10-16T14:24:59Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+95s 2026-10-16T14:25:04Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+100s 2026-10-16T14:25:09Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+105s 2026-10-16T14:25:14Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+110s 2026-10-16T14:25:19Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+115s 2026-10-16T14:25:25Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
t+121s 2026-10-16T14:25:30Z lastScanTimestamp=1792160608501 scanActive=false in_rest_view=no scan_lock=free
[exit 0]
~~~

### C2c — restore: scan, remove C2_NOLOCK from the bind mount, scan, post-state

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:25:30Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:25:30 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1792160608501,
  "lastScanDuration": 168
}
[curl http=200 total=0.004437s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:25:31Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:25:31 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1792160730801,
  "lastScanDuration": 413
}
[curl http=200 total=0.005087s]
~~~

#### every path of C2_NOLOCK on the bind mount before removal

`bash -c find\ .\ -path\ \"\*/database-connection/C2_NOLOCK\*\"\ \|\ LC_ALL=C\ sort ` @ 2026-10-16T14:25:31Z

~~~
./production/ignition/database-connection/C2_NOLOCK
./production/ignition/database-connection/C2_NOLOCK/config.json
./production/ignition/database-connection/C2_NOLOCK/resource.json
[exit 0]
~~~

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:25:31Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:25:31 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1792160730801,
  "lastScanDuration": 413
}
[curl http=200 total=0.006998s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:25:32Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:25:32 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1792160731888,
  "lastScanDuration": 157
}
[curl http=200 total=0.004362s]
~~~

#### GET scan-lock/config (post-state)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan-lock/config'` @ 2026-10-16T14:25:32Z

~~~
HTTP/1.1 204 No Content
Date: Fri, 16 Oct 2026 14:25:32 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block


[curl http=204 total=0.012024s]
~~~

#### GET resources/list/ignition/database-connection (post-state)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/resources/list/ignition/database-connection?limit=-1'` @ 2026-10-16T14:25:32Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:25:32 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 70

{"items":[],"metadata":{"total":0,"matching":0,"limit":-1,"offset":0}}
[curl http=200 total=0.008453s]
~~~

### C2d — the same while token A holds the lock (hold 150 s, covers the whole window)

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:25:32Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:25:32 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1792160731888,
  "lastScanDuration": 157
}
[curl http=200 total=0.009894s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:25:33Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:25:33 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1792160732840,
  "lastScanDuration": 175
}
[curl http=200 total=0.004898s]
~~~

#### POST scan-lock/config by token A (acquire 5, hold 150)

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/json --data {"acquireTimeout":5,"holdTimeout":150} '$BASE/data/api/v1/scan-lock/config'` @ 2026-10-16T14:25:33Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:25:33 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 91

{
  "timestamp": 11334938,
  "remaining": 149,
  "actor": "api-token:cap-a:192.168.147.1"
}
[curl http=200 total=0.009084s]
~~~

#### C2_HELD written into production/ while the lock is held (relative paths)

`bash -c find\ production\ -path\ \"\*C2_HELD\*\"\ -type\ f\ \|\ LC_ALL=C\ sort ` @ 2026-10-16T14:25:33Z

~~~
production/ignition/database-connection/C2_HELD/config.json
production/ignition/database-connection/C2_HELD/resource.json
[exit 0]
~~~

#### poll 120 s: lastScanTimestamp, scanActive, visibility, lock, no scan request sent

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-deploy/capture23.sh c2poll 120 C2_HELD ` @ 2026-10-16T14:25:33Z

~~~
t+0s 2026-10-16T14:25:33Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=149
t+5s 2026-10-16T14:25:39Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=144
t+11s 2026-10-16T14:25:44Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=139
t+16s 2026-10-16T14:25:49Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=134
t+21s 2026-10-16T14:25:54Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=128
t+26s 2026-10-16T14:25:59Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=123
t+31s 2026-10-16T14:26:05Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=117
t+37s 2026-10-16T14:26:10Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=112
t+42s 2026-10-16T14:26:15Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=107
t+47s 2026-10-16T14:26:20Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=102
t+52s 2026-10-16T14:26:26Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=97
t+58s 2026-10-16T14:26:31Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=92
t+63s 2026-10-16T14:26:36Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=87
t+68s 2026-10-16T14:26:41Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=81
t+73s 2026-10-16T14:26:47Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=76
t+79s 2026-10-16T14:26:52Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=71
t+84s 2026-10-16T14:26:57Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=66
t+89s 2026-10-16T14:27:02Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=60
t+94s 2026-10-16T14:27:08Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=55
t+100s 2026-10-16T14:27:13Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=49
t+105s 2026-10-16T14:27:18Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=44
t+110s 2026-10-16T14:27:23Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=39
t+115s 2026-10-16T14:27:28Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=34
t+121s 2026-10-16T14:27:34Z lastScanTimestamp=1792160732840 scanActive=false in_rest_view=no scan_lock=held remaining=29
[exit 0]
~~~

### C2d — restore: scan, remove C2_HELD from the bind mount, scan, post-state

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:27:34Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:27:34 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1792160732840,
  "lastScanDuration": 175
}
[curl http=200 total=0.005540s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:27:34Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:27:34 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1792160854698,
  "lastScanDuration": 464
}
[curl http=200 total=0.004454s]
~~~

#### every path of C2_HELD on the bind mount before removal

`bash -c find\ .\ -path\ \"\*/database-connection/C2_HELD\*\"\ \|\ LC_ALL=C\ sort ` @ 2026-10-16T14:27:34Z

~~~
./production/ignition/database-connection/C2_HELD
./production/ignition/database-connection/C2_HELD/config.json
./production/ignition/database-connection/C2_HELD/resource.json
[exit 0]
~~~

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:27:35Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:27:35 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1792160854698,
  "lastScanDuration": 464
}
[curl http=200 total=0.018154s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:27:35Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:27:35 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1792160855323,
  "lastScanDuration": 272
}
[curl http=200 total=0.004463s]
~~~

#### GET scan-lock/config (post-state)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan-lock/config'` @ 2026-10-16T14:27:35Z

~~~
HTTP/1.1 204 No Content
Date: Fri, 16 Oct 2026 14:27:35 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block


[curl http=204 total=0.005879s]
~~~

#### GET resources/list/ignition/database-connection (post-state)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/resources/list/ignition/database-connection?limit=-1'` @ 2026-10-16T14:27:35Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:27:35 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 70

{"items":[],"metadata":{"total":0,"matching":0,"limit":-1,"offset":0}}
[curl http=200 total=0.004792s]
~~~

### C2e — a lock lapses with an unscanned file (hold 15 s, no scan, poll 120 s)

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:27:36Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:27:36 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1792160855323,
  "lastScanDuration": 272
}
[curl http=200 total=0.003853s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:27:36Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:27:36 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1792160856764,
  "lastScanDuration": 743
}
[curl http=200 total=0.003668s]
~~~

#### POST scan-lock/config by token A (acquire 5, hold 15)

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/json --data {"acquireTimeout":5,"holdTimeout":15} '$BASE/data/api/v1/scan-lock/config'` @ 2026-10-16T14:27:36Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:27:36 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "timestamp": 11458528,
  "remaining": 14,
  "actor": "api-token:cap-a:192.168.147.1"
}
[curl http=200 total=0.020236s]
~~~

#### C2_LAPSE written into production/ while the lock is held (relative paths)

`bash -c find\ production\ -path\ \"\*C2_LAPSE\*\"\ -type\ f\ \|\ LC_ALL=C\ sort ` @ 2026-10-16T14:27:37Z

~~~
production/ignition/database-connection/C2_LAPSE/config.json
production/ignition/database-connection/C2_LAPSE/resource.json
[exit 0]
~~~

#### poll 120 s across the lapse: lastScanTimestamp, scanActive, visibility, lock, no scan request sent

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-deploy/capture23.sh c2poll 120 C2_LAPSE ` @ 2026-10-16T14:27:37Z

~~~
t+0s 2026-10-16T14:27:37Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=held remaining=14
t+5s 2026-10-16T14:27:42Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=held remaining=9
t+10s 2026-10-16T14:27:47Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=held remaining=4
t+15s 2026-10-16T14:27:52Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=free
t+20s 2026-10-16T14:27:58Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=free
t+26s 2026-10-16T14:28:03Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=free
t+31s 2026-10-16T14:28:08Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=free
t+36s 2026-10-16T14:28:13Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=free
t+41s 2026-10-16T14:28:18Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=free
t+46s 2026-10-16T14:28:23Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=free
t+51s 2026-10-16T14:28:28Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=free
t+56s 2026-10-16T14:28:34Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=free
t+62s 2026-10-16T14:28:39Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=free
t+67s 2026-10-16T14:28:44Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=free
t+72s 2026-10-16T14:28:49Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=free
t+77s 2026-10-16T14:28:54Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=free
t+82s 2026-10-16T14:29:00Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=free
t+88s 2026-10-16T14:29:05Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=free
t+93s 2026-10-16T14:29:10Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=free
t+99s 2026-10-16T14:29:16Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=free
t+104s 2026-10-16T14:29:21Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=free
t+109s 2026-10-16T14:29:26Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=free
t+114s 2026-10-16T14:29:31Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=free
t+119s 2026-10-16T14:29:37Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=free
t+125s 2026-10-16T14:29:42Z lastScanTimestamp=1792160856764 scanActive=false in_rest_view=no scan_lock=free
[exit 0]
~~~

### C2e — restore: scan, remove C2_LAPSE from the bind mount, scan, post-state

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:29:42Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:29:42 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1792160856764,
  "lastScanDuration": 743
}
[curl http=200 total=0.014371s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:29:43Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:29:43 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1792160983144,
  "lastScanDuration": 814
}
[curl http=200 total=0.013296s]
~~~

#### every path of C2_LAPSE on the bind mount before removal

`bash -c find\ .\ -path\ \"\*/database-connection/C2_LAPSE\*\"\ \|\ LC_ALL=C\ sort ` @ 2026-10-16T14:29:43Z

~~~
./production/ignition/database-connection/C2_LAPSE
./production/ignition/database-connection/C2_LAPSE/config.json
./production/ignition/database-connection/C2_LAPSE/resource.json
[exit 0]
~~~

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:29:43Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:29:44 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1792160983144,
  "lastScanDuration": 814
}
[curl http=200 total=0.011101s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:29:44Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:29:45 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1792160984524,
  "lastScanDuration": 501
}
[curl http=200 total=0.014377s]
~~~

#### GET scan-lock/config (post-state)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan-lock/config'` @ 2026-10-16T14:29:45Z

~~~
HTTP/1.1 204 No Content
Date: Fri, 16 Oct 2026 14:29:45 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block


[curl http=204 total=0.023025s]
~~~

#### GET resources/list/ignition/database-connection (post-state)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/resources/list/ignition/database-connection?limit=-1'` @ 2026-10-16T14:29:45Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:29:45 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 70

{"items":[],"metadata":{"total":0,"matching":0,"limit":-1,"offset":0}}
[curl http=200 total=0.010798s]
~~~

## C2b (manual)

NOT RUN BY THE DRIVER. A Designer or gateway-UI save under a held lock is an interactive session. Manual steps:

1. With the rig up, hold the lock: `POST /data/api/v1/scan-lock/config` with `{"acquireTimeout":5,"holdTimeout":300}` as token A (source `~/.cache/ign-deploy-rig/tokens.env`; the header carries the token, never print it).

2. Open the gateway web UI (http://localhost:18290, admin / the rig password in compose.yml) and save a change to a config resource in the production mode (for example edit a database connection description). Record whether the save succeeds, any refusal text, and the UTC time.

3. If a Designer is available, repeat the save from the Designer and record the same.

4. `GET /data/api/v1/scan-lock/config` after each save, then end the hold with one `POST /data/api/v1/scan/config` and remove what was saved.

Result: RUN 2026-10-18 on the local 8.3.6 rig (`ign-deploy-rig-gw-1`, localhost:18290), gateway web UI as admin. No Designer was used.

- 21:17:04Z: lock acquired by `api-token:cap-a` (`holdTimeout` 300, gateway log `Scan lock 'config-scan-lock' acquired ... Holding for 300 sec`).
- During the hold, the user edited the `Ignition OPC UA Server` opc-connection description (resource stamp `lastModification` 21:20:26Z) and added a SQLite database connection `Test` in production. The UI showed HTTP 503 and nothing appeared to happen.
- 21:21:13.344Z: lock released by one `POST /data/api/v1/scan/config` (200, then `GET scan-lock/config` 204).
- 21:21:13.6Z: both queued UI writes were applied (gateway log routes `/v1/resources/ignition/opc-connection` and `/v1/resources/ignition/database-connection`, actor admin). Files on disk were written at 21:21:13Z, and a store-and-forward engine `Test` came up at 21:21:14Z.
- Cleanup: the user deleted the `Test` connection and the store-and-forward engine and reverted the description. The rig was rechecked clean (no production database-connection, original description, no lock held).

Conclusion: the config scan-lock does not reject a gateway-UI save. It queues the save and applies it right after release, and the UI reports 503. For an apply, a UI save made during the hold lands after the engine's release, so it overwrites the deployed file rather than being blocked. The post-release fingerprint and verify read the tree after that write. This matches the C2 API-write queuing, and it confirms the Designer and editor preflight as the guard against a concurrent editor. The Designer save path is covered in the Designer run below.

Designer run, 2026-10-18 on the same rig. The Designer 8.3.6 session connected at 21:34:53Z as admin.

- 21:34:30Z: lock acquired by `api-token:cap-a` (`holdTimeout` 300).
- 21:35:20Z: Designer created project `Test` (RPC `projects.create`). It was written to `data/projects/Test/` and started right away, during the hold.
- 21:37:34Z: Designer saved the project-library script `Test`. `script-python/Test/code.py` and `resource.json` were written right away, and the gateway log shows `Restarting gateway scripts... project=Test`. The Designer reported it saved.
- 21:37:45Z: lock still held (`remaining` 102). Released at 21:38Z by one `POST scan/config` (200, then 204).

Conclusion: Designer project saves are not under the config scan-lock and land immediately. This is not a conflict for `ign deploy apply`, which writes only the mode collection's config resources (`config/resources/<mode>/`) and never touches `data/projects/`. Designer-side config edits that go through the gateway resource API would follow the queued UI path above. The remaining open question, whether an apply detects a config save queued during its hold, is answered by the rig test record below.

Rig test live_queued_config_save_during_apply, 2026-10-18 22:05Z to 22:09Z on the same rig (8.3.6). Setup: a production `POSTGRES` database-translator was created over the resource API (`POST /data/api/v1/resources/ignition/database-translator`, collection `production`, a marked `currentTimeQuery`) so the gateway wrote the file and issued a signature. The tracer plan over that seed was POSTGRES changed plus the alarm settings added (added 0, changed 2, removed 0).

- Writer: the engine's own token (cap-a). No second token was minted. The foreign write was `PUT /data/api/v1/resources/ignition/database-translator`, collection `production`, carrying the seed signature and a config with another marker.
- Hold window: a 4-file local commit holds the lock for well under a second, too short to land a write in from outside by polling. The test therefore ran a hook after the engine's commit and before its lock check and release scan. The hook read the lock, sent the write, watched it for 3 s and then returned control to the engine (hold 30 s). This widened window is a deviation from pure polling.
- Queue proof (22:09Z run): the lock was held by `api-token:cap-a` when the write was sent and still after the 3 s probe. The write was still unanswered at 3 s. It was answered 0.6 s after the hook returned, 3.6 s after it was sent. On the two runs before the final one the answer came 0.25 s and 0.8 s after the hook returned. The write was queued, as in C2 and C2b, even though the writer was the lock holder's own token.
- Verdict of the queued write: `resource_signature_rejected`. The engine's release scan reloaded the deployed copy first, so the seed signature was stale and the gateway refused the write.
- Run: outcome `applied`, no cause, `result.json` outcome `applied` and `ok` true; events: run_started gwbk_downloaded staged lock_acquired committed lock_verdict scan_done verify_done finalized. The final `config.json` on disk equals the declared config, the REST view equals the declared config (the REST view omits null members), and a fresh `deploy verify` was green.
- Cleanup: the test swept its units and ran one config scan at start and end; no production translator is left in the REST view.
- Five-gate run (22:07Z to 22:09Z, `--test-threads=1`): live_apply_then_rollback (15.1 s), live_kill_after_green_verify_then_recover (9.0 s), live_kill_mid_write_then_recover (32.6 s), live_lock_lapse_restores (20.9 s) and live_queued_config_save_during_apply (18.4 s) all passed (5 passed, 107 s).

Conclusion: the queued save is refused as stale because the engine's release scan reloaded the deployed copy first, and the deployed file stays. The apply does not report `applied` over a foreign save in this case. Limit: only the case of a write quoting a signature that the scan has made stale was exercised. A write whose signature is still current after the scan (for example one read after the release) is not a queued save and would not be caught by this gate.

## C11

A tag provider and its tag definitions written together into `production/` by a plain file write (the Phase 21 captured CAP21 unit: tag-provider, tag-group, tag-type-definition, tag-definition with FolderA), then ONE config scan. Recorded: whether the provider and its tags are present after that scan, and, if not, after a wait with no scan and after a second scan.

#### the CAP21 files written into production/ (relative paths)

`bash -c mkdir\ -p\ production/ignition\;\ for\ t\ in\ tag-provider\ tag-group\ tag-type-definition\ tag-definition\;\ do\ cp\ -R\ \"\$1/ignition/\$t\"\ production/ignition/\;\ done\;\ find\ production\ -type\ f\ \|\ LC_ALL=C\ sort _ /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/deploy/captured ` @ 2026-10-16T14:29:45Z

~~~
production/config-mode.json
production/ignition/tag-definition/CAP21/FolderA/tags.json
production/ignition/tag-definition/CAP21/FolderA/udts.json
production/ignition/tag-definition/CAP21/FolderA/unary-resource.json
production/ignition/tag-definition/CAP21/unary-resource.json
production/ignition/tag-group/CAP21/Default Historical/config.json
production/ignition/tag-group/CAP21/Default Historical/resource.json
production/ignition/tag-group/CAP21/Default/config.json
production/ignition/tag-group/CAP21/Default/resource.json
production/ignition/tag-provider/CAP21/config.json
production/ignition/tag-provider/CAP21/resource.json
production/ignition/tag-type-definition/CAP21/udts.json
production/ignition/tag-type-definition/CAP21/unary-resource.json
[exit 0]
~~~

### C11a — one scan

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:29:46Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:29:46 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1792160984524,
  "lastScanDuration": 501
}
[curl http=200 total=0.004683s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:29:46Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:29:46 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1792160986273,
  "lastScanDuration": 174
}
[curl http=200 total=0.006740s]
~~~

#### GET resources/list/ignition/tag-provider

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/resources/list/ignition/tag-provider?limit=-1'` @ 2026-10-16T14:29:46Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:29:46 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 2434

{"items":[{"type":"ignition/tag-provider","name":"CAP21","description":null,"enabled":true,"version":1,"collection":"production","collections":["production"],"signature":"ca24b458efbd8656c1241b51a6ca5d16bc0d6bd47936409161d7a983fd1fe0e4","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"c6e25805-b786-4d44-ae77-6fe6685410fb","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP21.tagCount","metric":{"value":11,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP21.status","result":{"healthy":true,"time":1792160986892,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"System","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"685d4b9111569d8f9288be581ecf1ddd8bf24146702a7c9c3802c9614e366ae7","config":{"profile":{"type":"MANAGED","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"nonUseCount":0}},"data":["config.json"],"attributes":{"uuid":"510e694d-c48f-4b0b-be72-99104b2ba9c1"},"metrics":{"tagCount":{"name":"tags.provider.System.tagCount","metric":{"value":38,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.System.status","result":{"healthy":true,"time":1792160986912,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"default","description":"Default tag provider","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"3c2bec2d649f5e1485b98304a99597e8c83291f383e4208cfb80bd3adff22724","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"defaultDatasourceName":null,"readPermissions":{"type":"AllOf","securityLevels":[]},"readOnly":false,"writePermissions":{"type":"AllOf","securityLevels":[]},"editPermissions":{"type":"AllOf","securityLevels":[]},"valuePersistence":"Database"}},"data":["config.json"],"attributes":{"uuid":"a16db799-ca7b-4d84-81c4-226d01b317b8"},"metrics":{"tagCount":{"name":"tags.provider.default.tagCount","metric":{"value":1,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.default.status","result":{"healthy":true,"time":1792160986920,"message":"Running"}}}}],"metadata":{"total":3,"matching":3,"limit":-1,"offset":0}}
[curl http=200 total=0.054167s]
~~~

#### GET tags/export?provider=CAP21&type=json (the tags the gateway holds)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/tags/export?provider=CAP21&type=json'` @ 2026-10-16T14:29:46Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:29:47 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Disposition: attachment; filename=export.json
Transfer-Encoding: chunked

{
  "name": "",
  "tagType": "Provider",
  "tags": [
    {
      "name": "FolderA",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Float8",
          "defaultValue": 1.0E10,
          "name": "F2",
          "tagType": "AtomicTag",
          "value": 1.0E10,
          "valueSource": "memory"
        },
        {
          "dataType": "Boolean",
          "expression": "1 \u003c 2 \u0026\u0026 3 \u003e 2 \u0026\u0026 \u0027a\u0027 \u003d \u0027a\u0027",
          "name": "Expr",
          "tagType": "AtomicTag",
          "valueSource": "expr"
        },
        {
          "name": "Inst1",
          "tagType": "UdtInstance",
          "tags": [
            {
              "name": "M1",
              "tagType": "AtomicTag"
            }
          ],
          "typeId": "CAP21_Type"
        },
        {
          "dataType": "String",
          "defaultValue": "Température ✓",
          "name": "Unicode",
          "tagType": "AtomicTag",
          "value": "Température ✓",
          "valueSource": "memory"
        },
        {
          "dataType": "Int8",
          "defaultValue": 9007199254740993,
          "name": "BigInt",
          "tagType": "AtomicTag",
          "value": 9007199254740993,
          "valueSource": "memory"
        },
        {
          "dataType": "Float8",
          "defaultValue": 1.5,
          "name": "F1",
          "tagType": "AtomicTag",
          "value": 1.5,
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "_types_",
      "tagType": "Folder",
      "tags": [
        {
          "name": "CAP21_Type",
          "tagType": "UdtType",
          "tags": [
            {
              "dataType": "Int4",
              "name": "M1",
              "tagType": "AtomicTag",
              "value": 1,
              "valueSource": "memory"
            }
          ]
        }
      ]
    }
  ]
}
[curl http=200 total=0.123116s]
~~~

Script reading after scan 1: provider CAP21 listed and tag BigInt exported: yes.

Script verdict: provider and tags present = yes after 1 scan(s).

### C11d — cleanup: the CAP21 files removed again, then a scan

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:29:47Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:29:47 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1792160986273,
  "lastScanDuration": 174
}
[curl http=200 total=0.006226s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-16T14:29:48Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:29:48 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1792160987548,
  "lastScanDuration": 168
}
[curl http=200 total=0.005216s]
~~~

#### GET resources/list/ignition/tag-provider (CAP21 gone?)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/resources/list/ignition/tag-provider?limit=-1'` @ 2026-10-16T14:29:48Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:29:48 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 1628

{"items":[{"type":"ignition/tag-provider","name":"System","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"685d4b9111569d8f9288be581ecf1ddd8bf24146702a7c9c3802c9614e366ae7","config":{"profile":{"type":"MANAGED","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"nonUseCount":0}},"data":["config.json"],"attributes":{"uuid":"510e694d-c48f-4b0b-be72-99104b2ba9c1"},"metrics":{"tagCount":{"name":"tags.provider.System.tagCount","metric":{"value":38,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.System.status","result":{"healthy":true,"time":1792160988075,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"default","description":"Default tag provider","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"3c2bec2d649f5e1485b98304a99597e8c83291f383e4208cfb80bd3adff22724","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"defaultDatasourceName":null,"readPermissions":{"type":"AllOf","securityLevels":[]},"readOnly":false,"writePermissions":{"type":"AllOf","securityLevels":[]},"editPermissions":{"type":"AllOf","securityLevels":[]},"valuePersistence":"Database"}},"data":["config.json"],"attributes":{"uuid":"a16db799-ca7b-4d84-81c4-226d01b317b8"},"metrics":{"tagCount":{"name":"tags.provider.default.tagCount","metric":{"value":1,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.default.status","result":{"healthy":true,"time":1792160988076,"message":"Running"}}}}],"metadata":{"total":2,"matching":2,"limit":-1,"offset":0}}
[curl http=200 total=0.009806s]
~~~

## C-gwbk

A gateway backup downloaded from the rig into a temp directory outside the repo. Only size, time, hash, headers and the entry listing are recorded; the file is deleted afterwards and never enters the repo. `type=all` is the type D-13 uses.

### C-gwbk-1 — the download route, response headers only (the body is discarded)

#### curl of GET backup?type=all: response headers, status, bytes counted, time

`bash -c curl\ -sS\ -m\ 300\ -D\ -\ -o\ /dev/null\ -w\ \"\[curl\ http=%\{http_code\}\ size_download=%\{size_download\}\ total=%\{time_total\}s\]\\n\"\ -H\ @\"\$0\"\ \"\$1/data/api/v1/backup\?type=all\" /var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.fHNOYXImcO/tokhdr http://localhost:18290 ` @ 2026-10-16T14:29:48Z

~~~
HTTP/1.1 200 OK
Date: Fri, 16 Oct 2026 14:29:48 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/x-ignition-gateway-backup;charset=utf-8
Content-Disposition: attachment; filename="ign-deploy-rig_Ignition-backup-20261016-1429.gwbk"
Transfer-Encoding: chunked

[curl http=200 size_download=3674513 total=1.127318s]
[exit 0]
~~~

### C-gwbk-2 — ign backup download through a profile for the rig

#### a throwaway ign config with a profile for the rig (token read from an env var)

`ign profile add ign-deploy-rig http://localhost:18290 --token-env IGN_C23_TOKEN --active --json ` @ 2026-10-16T14:29:49Z

~~~
{
  "ok": true,
  "profile": "ign-deploy-rig",
  "data": {
    "name": "ign-deploy-rig",
    "url": "http://localhost:18290/",
    "auth_kind": "token_env",
    "active": true
  }
}
[exit 0]
~~~

#### ign backup download --type all, wall-clock time

`bash -c s=\$\(perl\ -MTime::HiRes=time\ -e\ \"printf\ q\(%d\)\,\ time\*1000\"\)\;\ \"\$0\"\ backup\ download\ --profile\ ign-deploy-rig\ --type\ all\ -o\ \"\$1\"\ --json\;\ rc=\$\?\;\ e=\$\(perl\ -MTime::HiRes=time\ -e\ \"printf\ q\(%d\)\,\ time\*1000\"\)\;\ echo\ \"elapsed_ms=\$\(\(e\ -\ s\)\)\ ign_exit=\$rc\" ign /var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.fHNOYXImcO/gwbk/rig.gwbk ` @ 2026-10-16T14:29:49Z

~~~
{
  "ok": true,
  "profile": "ign-deploy-rig",
  "data": {
    "file": "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.fHNOYXImcO/gwbk/rig.gwbk",
    "type": "all"
  }
}
elapsed_ms=744 ign_exit=0
[exit 0]
~~~

### C-gwbk-3 — what the file is

#### size, first four bytes, type, sha256 of the original

`bash -c f=\$1\;\ echo\ \"size_bytes=\$\(wc\ -c\ \<\ \"\$f\"\ \|\ tr\ -d\ \"\ \"\)\"\;\ echo\ \"first4=\$\(head\ -c\ 4\ \"\$f\"\ \|\ xxd\ -p\)\"\;\ file\ -b\ \"\$f\"\;\ shasum\ -a\ 256\ \"\$f\"\ \|\ cut\ -d\"\ \"\ -f1 _ /var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.fHNOYXImcO/gwbk/rig.gwbk ` @ 2026-10-16T14:29:50Z

~~~
size_bytes=3674513
first4=504b0304
Zip archive data, at least v2.0 to extract, compression method=deflate
14a1b1c0687c944319f287395b2c5b88b546db79297eac2f9984dfbed6a6f2d2
[exit 0]
~~~

#### unzip -l: entry names and sizes (first 60 lines and the totals line)

`bash -c unzip\ -l\ \"\$1\"\ \|\ \{\ head\ -60\;\ \}\ \;\ echo\ \"...\"\;\ unzip\ -l\ \"\$1\"\ \|\ tail\ -2 _ /var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.fHNOYXImcO/gwbk/rig.gwbk ` @ 2026-10-16T14:29:50Z

~~~
Archive:  /var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.fHNOYXImcO/gwbk/rig.gwbk
  Length      Date    Time    Name
---------  ---------- -----   ----
  1355776  10-16-2026 14:29   db_backup_sqlite.idb
      106  10-16-2026 14:29   config/resources/core/com.inductiveautomation.sfc/chart-settings/config.json
      340  10-16-2026 14:29   config/resources/core/com.inductiveautomation.sfc/chart-settings/resource.json
      174  10-16-2026 14:29   config/resources/core/config-mode.json
       80  10-16-2026 14:29   config/resources/core/com.inductiveautomation.opcua/one-time/config.json
      340  10-16-2026 14:29   config/resources/core/com.inductiveautomation.opcua/one-time/resource.json
      578  10-16-2026 14:29   config/resources/core/com.inductiveautomation.opcua/server-config/config.json
      340  10-16-2026 14:29   config/resources/core/com.inductiveautomation.opcua/server-config/resource.json
      642  10-16-2026 14:29   config/resources/core/com.inductiveautomation.opcua/access-control/config.json
      340  10-16-2026 14:29   config/resources/core/com.inductiveautomation.opcua/access-control/resource.json
      596  10-16-2026 14:29   config/resources/core/com.inductiveautomation.eam/event-thresholds/config.json
      340  10-16-2026 14:29   config/resources/core/com.inductiveautomation.eam/event-thresholds/resource.json
      422  10-16-2026 14:29   config/resources/core/com.inductiveautomation.eam/module-settings/config.json
      340  10-16-2026 14:29   config/resources/core/com.inductiveautomation.eam/module-settings/resource.json
       97  10-16-2026 14:29   config/resources/core/ignition/edge-system-properties/config.json
      340  10-16-2026 14:29   config/resources/core/ignition/edge-system-properties/resource.json
     2839  10-16-2026 14:29   config/resources/core/ignition/security-properties/config.json
      117  10-16-2026 14:29   config/resources/core/ignition/security-properties/resource.json
      519  10-16-2026 14:29   config/resources/core/ignition/schedule/Example/config.json
      287  10-16-2026 14:29   config/resources/core/ignition/schedule/Example/resource.json
      471  10-16-2026 14:29   config/resources/core/ignition/tag-provider/default/config.json
      232  10-16-2026 14:29   config/resources/core/ignition/tag-provider/default/resource.json
      151  10-16-2026 14:29   config/resources/core/ignition/tag-provider/System/config.json
      191  10-16-2026 14:29   config/resources/core/ignition/tag-provider/System/resource.json
       89  10-16-2026 14:29   config/resources/core/ignition/general-alarm-settings/config.json
      340  10-16-2026 14:29   config/resources/core/ignition/general-alarm-settings/resource.json
      508  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/delete2.png/delete2.png
      412  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/delete2.png/resource.json
      744  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/lock_open.png/lock_open.png
      414  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/lock_open.png/resource.json
      990  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/warning.png/warning.png
      412  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/warning.png/resource.json
      590  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/document.png/document.png
      413  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/document.png/resource.json
      976  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/pie-chart.png/pie-chart.png
      414  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/pie-chart.png/resource.json
      809  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/stop.png/stop.png
      409  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/stop.png/resource.json
     1291  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/businessman.png/businessman.png
      417  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/businessman.png/resource.json
      578  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/export1.png/export1.png
      412  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/export1.png/resource.json
     1338  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/businessman_add.png/businessman_add.png
      421  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/businessman_add.png/resource.json
     1351  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/stopwatch.png/stopwatch.png
      415  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/stopwatch.png/resource.json
      634  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/clipboard.png/clipboard.png
      414  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/clipboard.png/resource.json
     1224  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/camera2.png/camera2.png
      413  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/camera2.png/resource.json
     1197  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/logic_not.png/logic_not.png
      415  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/logic_not.png/resource.json
     1304  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/trafficlight_red.png/trafficlight_red.png
      422  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/trafficlight_red.png/resource.json
     1309  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/trafficlight_green.png/trafficlight_green.png
      424  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/trafficlight_green.png/resource.json
      559  10-16-2026 14:29   config/resources/core/ignition/images/Builtin/icons/32/key1.png/key1.png
...
---------                     -------
 12085800                     1597 files
[exit 0]
~~~

#### unzip -tq of the whole file

`unzip -tq /var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.fHNOYXImcO/gwbk/rig.gwbk ` @ 2026-10-16T14:29:50Z

~~~
No errors detected in compressed data of /var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.fHNOYXImcO/gwbk/rig.gwbk.
[exit 0]
~~~

#### python zipfile: is_zipfile, entry count, testzip (None means every CRC reads)

`python3 -I -c import\ sys\,\ zipfile\;\ p\ =\ sys.argv\[1\]\;\ print\(\"is_zipfile\"\,\ zipfile.is_zipfile\(p\)\)\;\ z\ =\ zipfile.ZipFile\(p\)\;\ print\(\"entries\"\,\ len\(z.namelist\(\)\)\)\;\ print\(\"testzip\"\,\ z.testzip\(\)\) /var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.fHNOYXImcO/gwbk/rig.gwbk ` @ 2026-10-16T14:29:50Z

~~~
is_zipfile True
entries 1597
testzip None
[exit 0]
~~~

### C-gwbk-4 — a copy truncated to half its size

#### head -c half of the original into trunc.gwbk, then its size

`bash -c f=\$1\;\ n=\$\(\(\ \$\(wc\ -c\ \<\ \"\$f\"\)\ /\ 2\ \)\)\;\ head\ -c\ \"\$n\"\ \"\$f\"\ \>\ \"\$2\"\;\ echo\ \"truncated_to_bytes=\$\(wc\ -c\ \<\ \"\$2\"\ \|\ tr\ -d\ \"\ \"\)\" _ /var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.fHNOYXImcO/gwbk/rig.gwbk /var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.fHNOYXImcO/gwbk/trunc.gwbk ` @ 2026-10-16T14:29:50Z

~~~
truncated_to_bytes=1837256
[exit 0]
~~~

#### unzip -tq of the truncated copy

`unzip -tq /var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.fHNOYXImcO/gwbk/trunc.gwbk ` @ 2026-10-16T14:29:50Z

~~~
[/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.fHNOYXImcO/gwbk/trunc.gwbk]
  End-of-central-directory signature not found.  Either this file is not
  a zipfile, or it constitutes one disk of a multi-part archive.  In the
  latter case the central directory and zipfile comment will be found on
  the last disk(s) of this archive.
unzip:  cannot find zipfile directory in one of /var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.fHNOYXImcO/gwbk/trunc.gwbk or
        /var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.fHNOYXImcO/gwbk/trunc.gwbk.zip, and cannot find /var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.fHNOYXImcO/gwbk/trunc.gwbk.ZIP, period.
[exit 9]
~~~

#### unzip -l of the truncated copy (first 5 lines)

`bash -c unzip\ -l\ \"\$1\"\ 2\>\&1\ \|\ head\ -5 _ /var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.fHNOYXImcO/gwbk/trunc.gwbk ` @ 2026-10-16T14:29:50Z

~~~
Archive:  /var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.fHNOYXImcO/gwbk/trunc.gwbk
  End-of-central-directory signature not found.  Either this file is not
  a zipfile, or it constitutes one disk of a multi-part archive.  In the
  latter case the central directory and zipfile comment will be found on
  the last disk(s) of this archive.
[exit 0]
~~~

#### python zipfile on the truncated copy: is_zipfile and the error ZipFile raises

`python3 -I -c $'import sys, zipfile; p = sys.argv[1]; print("is_zipfile", zipfile.is_zipfile(p))\ntry:\n    zipfile.ZipFile(p)\n    print("opened")\nexcept Exception as e:\n    print(type(e).__name__, str(e))' /var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.fHNOYXImcO/gwbk/trunc.gwbk ` @ 2026-10-16T14:29:50Z

~~~
is_zipfile False
BadZipFile File is not a zip file
[exit 0]
~~~

### C-gwbk-5 — cleanup

#### the temp directory after deletion

`bash -c ls\ \"\$1\"\ 2\>\&1\ \|\|\ true _ /var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.fHNOYXImcO/gwbk ` @ 2026-10-16T14:29:50Z

~~~
ls: /var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.fHNOYXImcO/gwbk: No such file or directory
[exit 0]
~~~

## C-script-run

Windows capture host `ign-win-836` (Windows Server 2022, Windows PowerShell 5.1.20348.5756), account `svc-ignition-deploy`, the live test `wflwvm01_apply_script_run` of `crates/ignition-core/tests/live_transport_wflwvm01.rs`. The test uploads a two-line script to the account home and asks, in this order: which execution policy the host has; whether `-File` and `[ScriptBlock]::Create` run it under each policy (each case is a child `powershell` of the account, started with `-ExecutionPolicy`); what `Move-Item -Force`, `[IO.File]::Replace` and `[IO.File]::Move` do with a free target and with a target another handle holds open with each sharing mode, all on `\\?\` paths inside one script; how long the encoded stub is; and then it runs the real thing: a hash-checked stub that commits one batch (a new file with a non-ASCII name, a replaced file, a delete that empties a directory, a ledger) through `commit.ps1`, refuses a tampered work list and a tampered script, refuses a second commit over a leftover journal, restores the batch and restores it a second time. It works in `ign-live-scratch` below the account home, never in the gateway's data directory.

#### live run

`IGNITION_LIVE_WFLWVM01=1 cargo test -p ignition-core --test live_transport_wflwvm01 wflwvm01_apply_script_run -- --ignored --nocapture` @ 2026-10-17T20:22:30Z

This is the second full run of the test; it ran with the final `commit.ps1` (the ACL reset and the `held` refusal of 2026-10-17 are in it; the first run, 2026-10-17T18:58:30Z, passed with the same probe results and 2.69 s, 15.18 s and 28.87 s for commit, restore and prepare).

~~~
running 1 test
CAPTURE c-script-run @ 2026-10-17T20:22:44Z
CAPTURE policy: powershell "5.1.20348.5756" language mode "FullLanguage" execution policy ["MachinePolicy=Undefined,UserPolicy=Undefined,Process=Undefined,CurrentUser=Undefined,LocalMachine=Unrestricted"]
CAPTURE run: "-File policy=default" -> exit 0 : "IGN-RESULT:{\"ok\":1,\"a\":\"x\",\"b\":\"y\"}"
CAPTURE run: "-File policy=AllSigned" -> exit 1 : "File C:\\Users\\svc-ignition-deploy\\ign-live-scratch\\probe\\probe.ps1 cannot be loaded. The file  C:\\Users\\svc-ignition-deploy\\ign-live-scratch\\probe\\pro"
CAPTURE run: "-File policy=Restricted" -> exit 1 : "File C:\\Users\\svc-ignition-deploy\\ign-live-scratch\\probe\\probe.ps1 cannot be loaded because running scripts is  disabled on this system. For more info"
CAPTURE run: "-File policy=Bypass" -> exit 0 : "IGN-RESULT:{\"ok\":1,\"a\":\"x\",\"b\":\"y\"}"
CAPTURE run: "ScriptBlock policy=default" -> exit 0 : "IGN-RESULT:{\"ok\":1,\"a\":\"x\",\"b\":\"y\"}"
CAPTURE run: "ScriptBlock policy=AllSigned" -> exit 0 : "IGN-RESULT:{\"ok\":1,\"a\":\"x\",\"b\":\"y\"}"
CAPTURE run: "ScriptBlock policy=Restricted" -> exit 0 : "IGN-RESULT:{\"ok\":1,\"a\":\"x\",\"b\":\"y\"}"
CAPTURE move: "Move-Item -Force" target "free" -> "ok" ; target now "NEW-C" ; staged copy still there 0
CAPTURE move: "Move-Item -Force" target "None" -> "ERR Cannot create a file when that file already exists." ; target now "OLD-C" ; staged copy still there 1
CAPTURE move: "Move-Item -Force" target "Read" -> "ERR Cannot create a file when that file already exists." ; target now "OLD-C" ; staged copy still there 1
CAPTURE move: "Move-Item -Force" target "ReadWrite" -> "ERR Cannot create a file when that file already exists." ; target now "OLD-C" ; staged copy still there 1
CAPTURE move: "Move-Item -Force" target "Delete" -> "ok" ; target now "NEW-C" ; staged copy still there 0
CAPTURE move: "Move-Item -Force" target "ReadWrite,Delete" -> "ok" ; target now "NEW-C" ; staged copy still there 0
CAPTURE move: "File.Replace" target "free" -> "ok" ; target now "NEW-C" ; staged copy still there 0
CAPTURE move: "File.Replace" target "None" -> "ERR Exception calling \"Replace\" with \"3\" argument(s): \"The process cannot access the file because it is being used by another process.\"" ; target now "OLD-C" ; staged copy still there 1
CAPTURE move: "File.Replace" target "Read" -> "ERR Exception calling \"Replace\" with \"3\" argument(s): \"The process cannot access the file because it is being used by another process.\"" ; target now "OLD-C" ; staged copy still there 1
CAPTURE move: "File.Replace" target "ReadWrite" -> "ERR Exception calling \"Replace\" with \"3\" argument(s): \"The process cannot access the file because it is being used by another process.\"" ; target now "OLD-C" ; staged copy still there 1
CAPTURE move: "File.Replace" target "Delete" -> "ok" ; target now "NEW-C" ; staged copy still there 0
CAPTURE move: "File.Replace" target "ReadWrite,Delete" -> "ok" ; target now "NEW-C" ; staged copy still there 0
CAPTURE move: "File.Move" target "free" -> "ERR Exception calling \"Move\" with \"2\" argument(s): \"Cannot create a file when that file already exists.\r\n\"" ; target now "OLD-C" ; staged copy still there 1
CAPTURE move: "File.Move" target "None" -> "ERR Exception calling \"Move\" with \"2\" argument(s): \"Cannot create a file when that file already exists.\r\n\"" ; target now "OLD-C" ; staged copy still there 1
CAPTURE move: "File.Move" target "Read" -> "ERR Exception calling \"Move\" with \"2\" argument(s): \"Cannot create a file when that file already exists.\r\n\"" ; target now "OLD-C" ; staged copy still there 1
CAPTURE move: "File.Move" target "ReadWrite" -> "ERR Exception calling \"Move\" with \"2\" argument(s): \"Cannot create a file when that file already exists.\r\n\"" ; target now "OLD-C" ; staged copy still there 1
CAPTURE move: "File.Move" target "Delete" -> "ERR Exception calling \"Move\" with \"2\" argument(s): \"Cannot create a file when that file already exists.\r\n\"" ; target now "OLD-C" ; staged copy still there 1
CAPTURE move: "File.Move" target "ReadWrite,Delete" -> "ERR Exception calling \"Move\" with \"2\" argument(s): \"Cannot create a file when that file already exists.\r\n\"" ; target now "OLD-C" ; staged copy still there 1
CAPTURE move: "Move-Item -Force onto itself" target "-" -> "ok" ; target now "OLD-C" ; staged copy still there 0
CAPTURE encoded stub: 4014 characters of the 8000 budget (real roots; staging 95 chars)
TIMING prepare (upload + verify): 29.04s
CAPTURE tampered work list: transport failed (exec) on nodes.master: commit: the staged script or work list does not match its hash; nothing ran
test wflwvm01_apply_script_run has been running for over 60 seconds
CAPTURE tampered script: transport failed (exec) on nodes.master: commit: the staged script or work list does not match its hash; nothing ran
TIMING commit (one script call): 3.06s
CAPTURE commit report: BatchReport { phase: Written, stale: false, moved: [0, 1], deleted: [0], removed_dirs: ["ignition/database-connection/Doomed"], failure: None }
CAPTURE second commit over a journal: an interrupted deploy left the journal ~/ign-deploy/live-scratch/production/journal.json for run 20261017T120000Z-aaaaaa; recover it with: ign deploy rollback --run 20261017T120000Z-aaaaaa --env <env>
TIMING restore (one script call): 14.59s
CAPTURE restore report: BatchReport { phase: Restoring, stale: false, moved: [1, 2], deleted: [0, 3], removed_dirs: ["ignition/database-connection/Café DB,1"], failure: None }
cleanup: scratch gone true, staging gone true
TIMING wflwvm01_apply_script_run total: 367.30s
PASS wflwvm01_apply_script_run
test wflwvm01_apply_script_run ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 12 filtered out; finished in 367.73s
~~~

## C7b

What a move does on the real Windows host, taken on the REAL `production` collection of the gateway on `ign-win-836` (`C:\Program Files\Inductive Automation\Ignition\data\config\resources\production`, which held only the gateway-owned `config-mode.json` before every run) with the gateway running, through the host `ApplyHost` of this plan: the ACL of a file moved in from the account home against an untouched file and against a file the same account makes in place; whether the gateway loads the moved unit after a scan (REST `resources/list` and the gateway log, read through the installed `ign --profile azure-win api call`, because the credential of that profile is in the OS keyring and only the installed binary may read it without a prompt); what a target another process holds open does to the commit; the script commit of the 152-file corpus; and the longest path below the account home that the sftp server still lists and serves. Production was put back to its pre-run listing and scanned at the end of each run (asserted by the test).

#### run 1: before the ACL step existed (the move left the access list as staged)

`IGNITION_LIVE_WFLWVM01=1 cargo test -p ignition-core --test live_transport_wflwvm01 wflwvm01_c7b -- --ignored --nocapture` @ 2026-10-17T19:22:04Z

This run stopped at the held-target step (the test's own listing script could not read a file held without sharing, a test defect fixed afterwards) and is kept for its ACL blocks. The test panicked before its restore; production held the probe unit until it was removed by hand with PowerShell (the stray `ignition` directory this run created, nothing else; `config-mode.json` untouched), the account home's `ign-deploy` directory was removed and a scan was requested (HTTP 200).

~~~
running 1 test
CAPTURE c7b @ 2026-10-17T19:22:25Z
CAPTURE gateway service (sc qc Ignition): runs as "" ; "[SC] OpenService FAILED 5:  Access is denied. "
CAPTURE defender as found: {"err":"Cannot connect to CIM server. Access denied ","ok":1}
CAPTURE production before: 1 files, 0 directories: ["config-mode.json"]
CAPTURE scan before anything is written: (0.779764375, 557)
CAPTURE the probe unit: ignition/database-connection/SparkDB-oq5-01 (2 files)
TIMING c7b: script commit of the probe unit: 2.86s
CAPTURE commit report: BatchReport { phase: Written, stale: false, moved: [0, 1], deleted: [], removed_dirs: [], failure: None }
CAPTURE acl after the script moved the unit in @ 2026-10-17T19:23:15Z
CAPTURE acl   C:\Program Files\Inductive Automation\Ignition\data\config\resources\production
                 owner=BUILTIN\Administrators protected=False
                 ignwin836\svc-ignition-deploy:Modify, Synchronize:Allow:E:ContainerInherit, ObjectInherit | ignwin836\svc-ignition-deploy:ReadAndExecute, Synchronize:Allow:I:ContainerInherit, ObjectInherit | NT SERVICE\TrustedInstaller:FullControl:Allow:I:None | NT SERVICE\TrustedInstaller:268435456:Allow:I:ContainerInherit | NT AUTHORITY\SYSTEM:FullControl:Allow:I:None | NT AUTHORITY\SYSTEM:268435456:Allow:I:ContainerInherit, ObjectInherit | BUILTIN\Administrators:FullControl:Allow:I:None | BUILTIN\Administrators:268435456:Allow:I:ContainerInherit, ObjectInherit | BUILTIN\Users:ReadAndExecute, Synchronize:Allow:I:None | BUILTIN\Users:-1610612736:Allow:I:ContainerInherit, ObjectInherit | CREATOR OWNER:268435456:Allow:I:ContainerInherit, ObjectInherit | APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:-1610612736:Allow:I:ContainerInherit, ObjectInherit | APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:-1610612736:Allow:I:ContainerInherit, ObjectInherit
CAPTURE acl   C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\config-mode.json
                 owner=BUILTIN\Administrators protected=False
                 ignwin836\svc-ignition-deploy:Modify, Synchronize:Allow:I:None | NT AUTHORITY\SYSTEM:FullControl:Allow:I:None | BUILTIN\Administrators:FullControl:Allow:I:None | BUILTIN\Users:ReadAndExecute, Synchronize:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None
CAPTURE acl   C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\ignition\database-connection
                 owner=ignwin836\svc-ignition-deploy protected=False
                 ignwin836\svc-ignition-deploy:Modify, Synchronize:Allow:I:ContainerInherit, ObjectInherit | NT SERVICE\TrustedInstaller:FullControl:Allow:I:None | NT SERVICE\TrustedInstaller:268435456:Allow:I:ContainerInherit | NT AUTHORITY\SYSTEM:FullControl:Allow:I:None | NT AUTHORITY\SYSTEM:268435456:Allow:I:ContainerInherit, ObjectInherit | BUILTIN\Administrators:FullControl:Allow:I:None | BUILTIN\Administrators:268435456:Allow:I:ContainerInherit, ObjectInherit | BUILTIN\Users:ReadAndExecute, Synchronize:Allow:I:None | BUILTIN\Users:-1610612736:Allow:I:ContainerInherit, ObjectInherit | ignwin836\svc-ignition-deploy:FullControl:Allow:I:None | CREATOR OWNER:268435456:Allow:I:ContainerInherit, ObjectInherit | APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:-1610612736:Allow:I:ContainerInherit, ObjectInherit | APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:-1610612736:Allow:I:ContainerInherit, ObjectInherit
CAPTURE acl   C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\ignition\database-connection\SparkDB-oq5-01
                 owner=ignwin836\svc-ignition-deploy protected=False
                 ignwin836\svc-ignition-deploy:Modify, Synchronize:Allow:I:ContainerInherit, ObjectInherit | NT SERVICE\TrustedInstaller:FullControl:Allow:I:None | NT SERVICE\TrustedInstaller:268435456:Allow:I:ContainerInherit | NT AUTHORITY\SYSTEM:FullControl:Allow:I:None | NT AUTHORITY\SYSTEM:268435456:Allow:I:ContainerInherit, ObjectInherit | BUILTIN\Administrators:FullControl:Allow:I:None | BUILTIN\Administrators:268435456:Allow:I:ContainerInherit, ObjectInherit | BUILTIN\Users:ReadAndExecute, Synchronize:Allow:I:None | BUILTIN\Users:-1610612736:Allow:I:ContainerInherit, ObjectInherit | ignwin836\svc-ignition-deploy:FullControl:Allow:I:None | CREATOR OWNER:268435456:Allow:I:ContainerInherit, ObjectInherit | APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:-1610612736:Allow:I:ContainerInherit, ObjectInherit | APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:-1610612736:Allow:I:ContainerInherit, ObjectInherit
CAPTURE acl   C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\ignition\database-connection\SparkDB-oq5-01\config.json
                 owner=ignwin836\svc-ignition-deploy protected=True
                 NT AUTHORITY\SYSTEM:FullControl:Allow:E:None | BUILTIN\Administrators:FullControl:Allow:E:None | ignwin836\svc-ignition-deploy:FullControl:Allow:E:None
CAPTURE acl   C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\ignition\database-connection\SparkDB-oq5-01\resource.json
                 owner=ignwin836\svc-ignition-deploy protected=True
                 NT AUTHORITY\SYSTEM:FullControl:Allow:E:None | BUILTIN\Administrators:FullControl:Allow:E:None | ignwin836\svc-ignition-deploy:FullControl:Allow:E:None
CAPTURE acl a control file put by sftp into the same directory @ 2026-10-17T19:23:19Z
test wflwvm01_c7b has been running for over 60 seconds
CAPTURE acl   C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\ignition\database-connection\SparkDB-oq5-01\control.txt
                 owner=ignwin836\svc-ignition-deploy protected=False
                 ignwin836\svc-ignition-deploy:Modify, Synchronize:Allow:I:None | NT AUTHORITY\SYSTEM:FullControl:Allow:I:None | BUILTIN\Administrators:FullControl:Allow:I:None | BUILTIN\Users:ReadAndExecute, Synchronize:Allow:I:None | ignwin836\svc-ignition-deploy:FullControl:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None
CAPTURE scan after the move: 0.79s (gateway 531 ms); the REST view (resources/list) lists SparkDB-oq5-01: true
CAPTURE   SparkDB-oq5-01: collection "production" enabled true version 1 health {"status":{"name":"databases.connection-SparkDB-oq5-01.status","result":{"error":{"message":"Unable to decrypt ciphertext","stacktrace":["com.inductiveautomation.ignition.gateway.secrets.SecretException: Unable to decrypt ciphertext","\tat com.inductiveautomation.ignition.gateway.secrets.Secret$Embedded.getPlaintext(Secret.java:92)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.initializeDatasource(DatasourceImpl.java:263)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:312)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)","\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)","\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)","\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)","\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)","\tat java.base/java.lang.Thread.run(Unknown Source)","Caused by: com.inductiveautomation.ignition.gateway.secrets.SystemEncryptionServiceException: Unable to decrypt ciphertext","\tat com.inductiveautomation.ignition.gateway.secrets.AbstractSystemEncryptionService.decryptFromJson(AbstractSystemEncryptionService.java:84)","\tat com.inductiveautomation.ignition.gateway.secrets.Secret$Embedded.getPlaintext(Secret.java:90)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.initializeDatasource(DatasourceImpl.java:263)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:312)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)","\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)","\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)","\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)","\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)","\tat java.base/java.lang.Thread.run(Unknown Source)","Caused by: org.jose4j.lang.JoseException: Parsing error: org.jose4j.json.internal.json_simple.parser.ParseException: Unexpected character (H) at position 0.","\tat org.jose4j.json.JsonUtil.parseJson(JsonUtil.java:71)","\tat org.jose4j.jwx.Headers.setEncodedHeader(Headers.java:128)","\tat org.jose4j.jwx.JsonWebStructure.setEncodedHeader(JsonWebStructure.java:135)","\tat org.jose4j.jwe.JsonWebEncryption.setCompactSerializationParts(JsonWebEncryption.java:198)","\tat org.jose4j.jwx.JsonWebStructure.setCompactSerialization(JsonWebStructure.java:109)","\tat com.inductiveautomation.ignition.gateway.secrets.AbstractSystemEncryptionService.decryptFromJson(AbstractSystemEncryptionService.java:79)","\tat com.inductiveautomation.ignition.gateway.secrets.Secret$Embedded.getPlaintext(Secret.java:90)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.initializeDatasource(DatasourceImpl.java:263)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:312)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)","\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)","\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)","\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)","\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)","\tat java.base/java.lang.Thread.run(Unknown Source)","Caused by: org.jose4j.json.internal.json_simple.parser.ParseException: Unexpected character (H) at position 0.","\tat org.jose4j.json.internal.json_simple.parser.Yylex.yylex(Yylex.java:612)","\tat org.jose4j.json.internal.json_simple.parser.JSONParser.nextToken(JSONParser.java:267)","\tat org.jose4j.json.internal.json_simple.parser.JSONParser.parse(JSONParser.java:116)","\tat org.jose4j.json.internal.json_simple.parser.JSONParser.parse(JSONParser.java:79)","\tat org.jose4j.json.JsonUtil.parseJson(JsonUtil.java:62)","\tat org.jose4j.jwx.Headers.setEncodedHeader(Headers.java:128)","\tat org.jose4j.jwx.JsonWebStructure.setEncodedHeader(JsonWebStructure.java:135)","\tat org.jose4j.jwe.JsonWebEncryption.setCompactSerializationParts(JsonWebEncryption.java:198)","\tat org.jose4j.jwx.JsonWebStructure.setCompactSerialization(JsonWebStructure.java:109)","\tat com.inductiveautomation.ignition.gateway.secrets.AbstractSystemEncryptionService.decryptFromJson(AbstractSystemEncryptionService.java:79)","\tat com.inductiveautomation.ignition.gateway.secrets.Secret$Embedded.getPlaintext(Secret.java:90)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.initializeDatasource(DatasourceImpl.java:263)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:312)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)","\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)","\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)","\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)","\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)","\tat java.base/java.lang.Thread.run(Unknown Source)"]},"healthy":false,"message":"Faulted","time":1792265004244}}}
CAPTURE gateway log since the scan: 2 WARN+ lines, 2 mention SparkDB-oq5-01 or `will not be loaded`
CAPTURE   log "ERROR" "gateway.Database" Error creating javax.sql.DataSource for datasource 'SparkDB-oq5-01'.
CAPTURE   log "ERROR" "gateway.Database" Error creating javax.sql.DataSource for datasource 'SparkDB-oq5-01'.
CAPTURE acl after the gateway scanned the unit @ 2026-10-17T19:23:24Z
CAPTURE acl   C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\ignition\database-connection\SparkDB-oq5-01\config.json
                 owner=ignwin836\svc-ignition-deploy protected=True
                 NT AUTHORITY\SYSTEM:FullControl:Allow:E:None | BUILTIN\Administrators:FullControl:Allow:E:None | ignwin836\svc-ignition-deploy:FullControl:Allow:E:None
CAPTURE acl   C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\ignition\database-connection\SparkDB-oq5-01\resource.json
                 owner=ignwin836\svc-ignition-deploy protected=True
                 NT AUTHORITY\SYSTEM:FullControl:Allow:E:None | BUILTIN\Administrators:FullControl:Allow:E:None | ignwin836\svc-ignition-deploy:FullControl:Allow:E:None
CAPTURE commit with the target held open: 2.9s -> Ok(BatchReport { phase: Snapshotting, stale: true, moved: [], deleted: [], removed_dirs: [], failure: Some("1 path(s) are not what the plan was made against: ignition/database-connection/SparkDB-oq5-01/config.json") })
...
thread 'wflwvm01_c7b' (161097389) panicked at crates/ignition-core/tests/live_transport_wflwvm01.rs:1824:10:
list_hashes: TransportFailed { operation: "exec", node: "nodes.master", detail: "list_hashes: exit code 1: At line:1 char:692\n+ ... IO.SearchOption]::AllDirectories)){$s=[IO.File]::OpenRead($f);$o[$f.S ...\n+                                        ~~~~~~~~~~~~~~~~~~~~~~~~~~\n+ CategoryInfo          : NotSpecified: (:) [], ParentContainsErrorRecordException\n+ FullyQualifiedErrorId : IOException" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test wflwvm01_c7b ... FAILED

failures:

failures:
    wflwvm01_c7b

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 12 filtered out; finished in 115.39s

error: test failed, to rerun pass `-p ignition-core --test live_transport_wflwvm01`
~~~

#### run 2: the final run, with the ACL reset in `commit.ps1`

`IGNITION_LIVE_WFLWVM01=1 cargo test -p ignition-core --test live_transport_wflwvm01 wflwvm01_c7b -- --ignored --nocapture` @ 2026-10-17T19:28:51Z

~~~
running 1 test
CAPTURE c7b @ 2026-10-17T19:29:06Z
CAPTURE gateway service (sc qc Ignition): runs as "" ; "[SC] OpenService FAILED 5:  Access is denied. "
CAPTURE defender as found: {"err":"Cannot connect to CIM server. Access denied ","ok":1}
CAPTURE production before: 1 files, 0 directories: ["config-mode.json"]
CAPTURE scan before anything is written: (1.098097917, 500)
CAPTURE the probe unit: ignition/database-connection/SparkDB-oq5-01 (2 files)
CAPTURE acl the staged copies in the account home, before the move @ 2026-10-17T19:29:56Z
CAPTURE acl   C:\Users\svc-ignition-deploy\ign-deploy\live-scratch\production\staging\20261017T130000Z-c7b001\files\ignition\database-connection\SparkDB-oq5-01\config.json
                 owner=ignwin836\svc-ignition-deploy protected=False
                 NT AUTHORITY\SYSTEM:FullControl:Allow:I:None | BUILTIN\Administrators:FullControl:Allow:I:None | ignwin836\svc-ignition-deploy:FullControl:Allow:I:None
CAPTURE acl   C:\Users\svc-ignition-deploy\ign-deploy\live-scratch\production\staging\20261017T130000Z-c7b001\files\ignition\database-connection\SparkDB-oq5-01\resource.json
                 owner=ignwin836\svc-ignition-deploy protected=False
                 NT AUTHORITY\SYSTEM:FullControl:Allow:I:None | BUILTIN\Administrators:FullControl:Allow:I:None | ignwin836\svc-ignition-deploy:FullControl:Allow:I:None
TIMING c7b: script commit of the probe unit: 2.91s
CAPTURE commit report: BatchReport { phase: Written, stale: false, moved: [0, 1], deleted: [], removed_dirs: [], failure: None }
CAPTURE acl after the script moved the unit in @ 2026-10-17T19:30:00Z
test wflwvm01_c7b has been running for over 60 seconds
CAPTURE acl   C:\Program Files\Inductive Automation\Ignition\data\config\resources\production
                 owner=BUILTIN\Administrators protected=False
                 ignwin836\svc-ignition-deploy:Modify, Synchronize:Allow:E:ContainerInherit, ObjectInherit | ignwin836\svc-ignition-deploy:ReadAndExecute, Synchronize:Allow:I:ContainerInherit, ObjectInherit | NT SERVICE\TrustedInstaller:FullControl:Allow:I:None | NT SERVICE\TrustedInstaller:268435456:Allow:I:ContainerInherit | NT AUTHORITY\SYSTEM:FullControl:Allow:I:None | NT AUTHORITY\SYSTEM:268435456:Allow:I:ContainerInherit, ObjectInherit | BUILTIN\Administrators:FullControl:Allow:I:None | BUILTIN\Administrators:268435456:Allow:I:ContainerInherit, ObjectInherit | BUILTIN\Users:ReadAndExecute, Synchronize:Allow:I:None | BUILTIN\Users:-1610612736:Allow:I:ContainerInherit, ObjectInherit | CREATOR OWNER:268435456:Allow:I:ContainerInherit, ObjectInherit | APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:-1610612736:Allow:I:ContainerInherit, ObjectInherit | APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:-1610612736:Allow:I:ContainerInherit, ObjectInherit
CAPTURE acl   C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\config-mode.json
                 owner=BUILTIN\Administrators protected=False
                 ignwin836\svc-ignition-deploy:Modify, Synchronize:Allow:I:None | NT AUTHORITY\SYSTEM:FullControl:Allow:I:None | BUILTIN\Administrators:FullControl:Allow:I:None | BUILTIN\Users:ReadAndExecute, Synchronize:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None
CAPTURE acl   C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\ignition\database-connection
                 owner=ignwin836\svc-ignition-deploy protected=False
                 ignwin836\svc-ignition-deploy:Modify, Synchronize:Allow:I:ContainerInherit, ObjectInherit | NT SERVICE\TrustedInstaller:FullControl:Allow:I:None | NT SERVICE\TrustedInstaller:268435456:Allow:I:ContainerInherit | NT AUTHORITY\SYSTEM:FullControl:Allow:I:None | NT AUTHORITY\SYSTEM:268435456:Allow:I:ContainerInherit, ObjectInherit | BUILTIN\Administrators:FullControl:Allow:I:None | BUILTIN\Administrators:268435456:Allow:I:ContainerInherit, ObjectInherit | BUILTIN\Users:ReadAndExecute, Synchronize:Allow:I:None | BUILTIN\Users:-1610612736:Allow:I:ContainerInherit, ObjectInherit | ignwin836\svc-ignition-deploy:FullControl:Allow:I:None | CREATOR OWNER:268435456:Allow:I:ContainerInherit, ObjectInherit | APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:-1610612736:Allow:I:ContainerInherit, ObjectInherit | APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:-1610612736:Allow:I:ContainerInherit, ObjectInherit
CAPTURE acl   C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\ignition\database-connection\SparkDB-oq5-01
                 owner=ignwin836\svc-ignition-deploy protected=False
                 ignwin836\svc-ignition-deploy:Modify, Synchronize:Allow:I:ContainerInherit, ObjectInherit | NT SERVICE\TrustedInstaller:FullControl:Allow:I:None | NT SERVICE\TrustedInstaller:268435456:Allow:I:ContainerInherit | NT AUTHORITY\SYSTEM:FullControl:Allow:I:None | NT AUTHORITY\SYSTEM:268435456:Allow:I:ContainerInherit, ObjectInherit | BUILTIN\Administrators:FullControl:Allow:I:None | BUILTIN\Administrators:268435456:Allow:I:ContainerInherit, ObjectInherit | BUILTIN\Users:ReadAndExecute, Synchronize:Allow:I:None | BUILTIN\Users:-1610612736:Allow:I:ContainerInherit, ObjectInherit | ignwin836\svc-ignition-deploy:FullControl:Allow:I:None | CREATOR OWNER:268435456:Allow:I:ContainerInherit, ObjectInherit | APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:-1610612736:Allow:I:ContainerInherit, ObjectInherit | APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:-1610612736:Allow:I:ContainerInherit, ObjectInherit
CAPTURE acl   C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\ignition\database-connection\SparkDB-oq5-01\config.json
                 owner=ignwin836\svc-ignition-deploy protected=False
                 ignwin836\svc-ignition-deploy:Modify, Synchronize:Allow:I:None | NT AUTHORITY\SYSTEM:FullControl:Allow:I:None | BUILTIN\Administrators:FullControl:Allow:I:None | BUILTIN\Users:ReadAndExecute, Synchronize:Allow:I:None | ignwin836\svc-ignition-deploy:FullControl:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None
CAPTURE acl   C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\ignition\database-connection\SparkDB-oq5-01\resource.json
                 owner=ignwin836\svc-ignition-deploy protected=False
                 ignwin836\svc-ignition-deploy:Modify, Synchronize:Allow:I:None | NT AUTHORITY\SYSTEM:FullControl:Allow:I:None | BUILTIN\Administrators:FullControl:Allow:I:None | BUILTIN\Users:ReadAndExecute, Synchronize:Allow:I:None | ignwin836\svc-ignition-deploy:FullControl:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None
CAPTURE acl a control file put by sftp into the same directory @ 2026-10-17T19:30:05Z
CAPTURE acl   C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\ignition\database-connection\SparkDB-oq5-01\control.txt
                 owner=ignwin836\svc-ignition-deploy protected=False
                 ignwin836\svc-ignition-deploy:Modify, Synchronize:Allow:I:None | NT AUTHORITY\SYSTEM:FullControl:Allow:I:None | BUILTIN\Administrators:FullControl:Allow:I:None | BUILTIN\Users:ReadAndExecute, Synchronize:Allow:I:None | ignwin836\svc-ignition-deploy:FullControl:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None
CAPTURE scan after the move: 0.95s (gateway 423 ms); the REST view (resources/list) lists SparkDB-oq5-01: true
CAPTURE   SparkDB-oq5-01: collection "production" enabled true version 1 health {"status":{"name":"databases.connection-SparkDB-oq5-01.status","result":{"error":{"message":"Unable to decrypt ciphertext","stacktrace":["com.inductiveautomation.ignition.gateway.secrets.SecretException: Unable to decrypt ciphertext","\tat com.inductiveautomation.ignition.gateway.secrets.Secret$Embedded.getPlaintext(Secret.java:92)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.initializeDatasource(DatasourceImpl.java:263)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:312)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)","\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)","\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)","\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)","\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)","\tat java.base/java.lang.Thread.run(Unknown Source)","Caused by: com.inductiveautomation.ignition.gateway.secrets.SystemEncryptionServiceException: Unable to decrypt ciphertext","\tat com.inductiveautomation.ignition.gateway.secrets.AbstractSystemEncryptionService.decryptFromJson(AbstractSystemEncryptionService.java:84)","\tat com.inductiveautomation.ignition.gateway.secrets.Secret$Embedded.getPlaintext(Secret.java:90)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.initializeDatasource(DatasourceImpl.java:263)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:312)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)","\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)","\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)","\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)","\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)","\tat java.base/java.lang.Thread.run(Unknown Source)","Caused by: org.jose4j.lang.JoseException: Parsing error: org.jose4j.json.internal.json_simple.parser.ParseException: Unexpected character (H) at position 0.","\tat org.jose4j.json.JsonUtil.parseJson(JsonUtil.java:71)","\tat org.jose4j.jwx.Headers.setEncodedHeader(Headers.java:128)","\tat org.jose4j.jwx.JsonWebStructure.setEncodedHeader(JsonWebStructure.java:135)","\tat org.jose4j.jwe.JsonWebEncryption.setCompactSerializationParts(JsonWebEncryption.java:198)","\tat org.jose4j.jwx.JsonWebStructure.setCompactSerialization(JsonWebStructure.java:109)","\tat com.inductiveautomation.ignition.gateway.secrets.AbstractSystemEncryptionService.decryptFromJson(AbstractSystemEncryptionService.java:79)","\tat com.inductiveautomation.ignition.gateway.secrets.Secret$Embedded.getPlaintext(Secret.java:90)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.initializeDatasource(DatasourceImpl.java:263)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:312)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)","\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)","\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)","\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)","\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)","\tat java.base/java.lang.Thread.run(Unknown Source)","Caused by: org.jose4j.json.internal.json_simple.parser.ParseException: Unexpected character (H) at position 0.","\tat org.jose4j.json.internal.json_simple.parser.Yylex.yylex(Yylex.java:612)","\tat org.jose4j.json.internal.json_simple.parser.JSONParser.nextToken(JSONParser.java:267)","\tat org.jose4j.json.internal.json_simple.parser.JSONParser.parse(JSONParser.java:116)","\tat org.jose4j.json.internal.json_simple.parser.JSONParser.parse(JSONParser.java:79)","\tat org.jose4j.json.JsonUtil.parseJson(JsonUtil.java:62)","\tat org.jose4j.jwx.Headers.setEncodedHeader(Headers.java:128)","\tat org.jose4j.jwx.JsonWebStructure.setEncodedHeader(JsonWebStructure.java:135)","\tat org.jose4j.jwe.JsonWebEncryption.setCompactSerializationParts(JsonWebEncryption.java:198)","\tat org.jose4j.jwx.JsonWebStructure.setCompactSerialization(JsonWebStructure.java:109)","\tat com.inductiveautomation.ignition.gateway.secrets.AbstractSystemEncryptionService.decryptFromJson(AbstractSystemEncryptionService.java:79)","\tat com.inductiveautomation.ignition.gateway.secrets.Secret$Embedded.getPlaintext(Secret.java:90)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.initializeDatasource(DatasourceImpl.java:263)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceImpl.runTest(DatasourceImpl.java:312)","\tat com.inductiveautomation.ignition.gateway.datasource.DatasourceManagerImpl$FaultedDatasourceRetryer.lambda$newRetryRunnable$0(DatasourceManagerImpl.java:1233)","\tat com.inductiveautomation.ignition.common.execution.impl.BasicExecutionEngine$TrackedTask.run(BasicExecutionEngine.java:599)","\tat java.base/java.util.concurrent.Executors$RunnableAdapter.call(Unknown Source)","\tat java.base/java.util.concurrent.FutureTask.runAndReset(Unknown Source)","\tat java.base/java.util.concurrent.ScheduledThreadPoolExecutor$ScheduledFutureTask.run(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor.runWorker(Unknown Source)","\tat java.base/java.util.concurrent.ThreadPoolExecutor$Worker.run(Unknown Source)","\tat java.base/java.lang.Thread.run(Unknown Source)"]},"healthy":false,"message":"Faulted","time":1792265412026}}}
CAPTURE gateway log since the scan: 2 WARN+ lines, 2 mention SparkDB-oq5-01 or `will not be loaded`
CAPTURE   log "ERROR" "gateway.Database" Error creating javax.sql.DataSource for datasource 'SparkDB-oq5-01'.
CAPTURE   log "ERROR" "gateway.Database" Error creating javax.sql.DataSource for datasource 'SparkDB-oq5-01'.
CAPTURE acl after the gateway scanned the unit @ 2026-10-17T19:30:12Z
CAPTURE acl   C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\ignition\database-connection\SparkDB-oq5-01\config.json
                 owner=ignwin836\svc-ignition-deploy protected=False
                 ignwin836\svc-ignition-deploy:Modify, Synchronize:Allow:I:None | NT AUTHORITY\SYSTEM:FullControl:Allow:I:None | BUILTIN\Administrators:FullControl:Allow:I:None | BUILTIN\Users:ReadAndExecute, Synchronize:Allow:I:None | ignwin836\svc-ignition-deploy:FullControl:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None
CAPTURE acl   C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\ignition\database-connection\SparkDB-oq5-01\resource.json
                 owner=ignwin836\svc-ignition-deploy protected=False
                 ignwin836\svc-ignition-deploy:Modify, Synchronize:Allow:I:None | NT AUTHORITY\SYSTEM:FullControl:Allow:I:None | BUILTIN\Administrators:FullControl:Allow:I:None | BUILTIN\Users:ReadAndExecute, Synchronize:Allow:I:None | ignwin836\svc-ignition-deploy:FullControl:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None | APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:ReadAndExecute, Synchronize:Allow:I:None
CAPTURE commit with the target held open (share Read) @ 2026-10-17T19:31:09Z: 5.7s -> Ok(BatchReport { phase: Writing, stale: false, moved: [], deleted: [], removed_dirs: [], failure: Some("write 0: cannot move the file into place") })
CAPTURE held target (share Read) after the commit: production listing unchanged true
CAPTURE restore of the held run (share Read): BatchReport { phase: Restoring, stale: false, moved: [0], deleted: [], removed_dirs: [], failure: None }
CAPTURE commit with the target held open (share None) @ 2026-10-17T19:32:40Z: 5.6s -> Err(TransportFailed { operation: "commit", node: "nodes.master", detail: "ignition/database-connection/SparkDB-oq5-01/config.json could not be read because another process holds it open; nothing was changed" })
CAPTURE held target (share None) after the commit: production listing unchanged true
CAPTURE restore of the probe unit: BatchReport { phase: Restoring, stale: false, moved: [], deleted: [0, 1], removed_dirs: ["ignition/database-connection/SparkDB-oq5-01"], failure: None }
CAPTURE corpus: 152 files (91833 bytes); prepare 86.0s; script commit 6.92s; moved 152 failure None
CAPTURE corpus restore: 18.03s (including its uploads and journal read); failure None
CAPTURE path length 200: ls lists f.json true (exit 0); get ok true (exit 0)
CAPTURE path length 230: ls lists f.json true (exit 0); get ok true (exit 0)
CAPTURE path length 240: ls lists f.json true (exit 0); get ok true (exit 0)
CAPTURE path length 250: ls lists f.json true (exit 0); get ok true (exit 0)
CAPTURE path length 255: ls lists f.json true (exit 0); get ok true (exit 0)
CAPTURE path length 258: ls lists f.json true (exit 0); get ok true (exit 0)
CAPTURE path length 259: ls lists f.json true (exit 0); get ok true (exit 0)
CAPTURE path length 260: ls lists f.json false (exit 0); get ok false (exit 1)
CAPTURE path length 261: ls lists f.json false (exit 0); get ok false (exit 1)
CAPTURE path length 270: ls lists f.json false (exit 1); get ok false (exit 1)
CAPTURE longest reachable file path (ls and get both work): Some(259)
CAPTURE production listing equals the pre-run listing: true
CAPTURE closing scan: (1.571151833, 437)
CAPTURE cleanup: deploy dir gone true, length probe gone true
TIMING wflwvm01_c7b total: 529.78s
PASS wflwvm01_c7b
test wflwvm01_c7b ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 12 filtered out; finished in 529.82s
~~~

#### run 3: review follow-up (hardened `commit.ps1`)

An automated review of the script flagged insecure permissions and check-then-use gaps on reparse points. After the script was hardened (a re-check of the path chain before each move attempt and around the descriptor change, `.NET` tree removal that does not follow a link) `wflwvm01_apply_script_run` (2026-10-17T20:36Z onward) and `wflwvm01_c7b` (with a new assertion that no moved production file lets `Users`, `Everyone` or `Authenticated Users` write) passed again, and this test checked what the script creates and what it does with links.

`IGNITION_LIVE_WFLWVM01=1 cargo test -p ignition-core --test live_transport_wflwvm01 wflwvm01_script_acl_and_reparse -- --ignored --nocapture` @ 2026-10-17T20:53:46Z

~~~
running 1 test
test wflwvm01_script_acl_and_reparse has been running for over 60 seconds
CAPTURE acl: 28 files and directories the script created or moved carry no entry for Users, Everyone or Authenticated Users
CAPTURE restore through a junction: BatchReport { phase: Restoring, stale: false, moved: [], deleted: [], removed_dirs: [], failure: Some("restore 0: the live path is not a regular file") }
CAPTURE commit through a junction: BatchReport { phase: Snapshotting, stale: true, moved: [], deleted: [], removed_dirs: [], failure: Some("1 path(s) are not what the plan was made against: ignition/database-connection/Linked/config.json") }
cleanup: scratch gone true, staging gone true
PASS wflwvm01_script_acl_and_reparse
test wflwvm01_script_acl_and_reparse ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 14 filtered out; finished in 141.83s
~~~

## 23-20 PowerShell host ops

Plan 23-20 needs no new gateway capture; it completes `commit.ps1` on the host 23-08 captured. These blocks are the live evidence for the ops 23-08 did not build, taken on `ign-win-836` (Windows PowerShell 5.1.20348, Win32-OpenSSH, `svc-ignition-deploy`) on 2026-10-17. Nothing here touched the gateway REST, the real `production` collection or the trial: every run worked in the scratch gateway `ign-live-scratch\data` below the account home.

Command: `IGNITION_LIVE_WFLWVM01=1 cargo test -p ignition-core --test live_transport_wflwvm01 wflwvm01_ps_ops -- --ignored --nocapture`, run 2026-10-17T21:47Z.

~~~
CAPTURE ps ops @ 2026-10-17T21:47:39Z
TIMING probe (one script call): 5.67s
CAPTURE probe: ProbeReport { writable: true, problems: [] }
CAPTURE probe with CreateFiles denied: ProbeReport { writable: false, problems: ["the mode collection is not writable"] }
TIMING fingerprint of a held file (bounded retries): 7.55s
CAPTURE fingerprint of a held file: transport failed (fingerprint) on nodes.master: ignition/database-connection/Existing/config.json could not be read because another process holds it open; nothing was changed
TIMING read_record (sftp get): 15.32s
TIMING fetch_snapshot (sftp): 24.65s
CAPTURE fetch_snapshot: {"files/ignition/database-connection/Doomed/config.json": "ea9eea61...", "files/ignition/database-connection/ReplaceMe/config.json": "79384ec1...", "record.json": "d0f93919..."}
TIMING finalize (one script call): 13.79s
TIMING mirror commit (one script call): 5.64s
CAPTURE acl of a file the mirror moved in: protected False, "NT AUTHORITY\\SYSTEM:FullControl:Allow:I:None | BUILTIN\\Administrators:FullControl:Allow:I:None | ignwin836\\svc-ignition-deploy:FullControl:Allow:I:None"
TIMING mirror commit over a held target: 9.26s
CAPTURE mirror over a held target: transport failed (commit) on nodes.master: the commit stopped at ignition/database-connection/MirrorNew/config.json: write 0: cannot move the file into place; 0 of 1 files were written and 0 deleted, and nothing was rolled back
CAPTURE mirror with the staged copy a hard link of the live file: Ok(MirrorReport { written: ["ignition/database-connection/MirrorNew/config.json"], deleted: [], removed_dirs: [] })
cleanup: scratch gone true, staging gone true
TIMING wflwvm01_ps_ops total: 581.79s
PASS wflwvm01_ps_ops
~~~

The same session ran the whole test binary of `deploy_host_conformance` (47 tests: the shared table against the local host and against the PowerShell host, the PowerShell-only rows, the fault rows) and the `ignition-core` unit tests, both built for `x86_64-pc-windows-msvc` with `cargo xwin` on the Mac, copied to the account home and run there by ssh: the conformance binary 47 passed, 0 failed; the library tests 906 passed, 13 failed, every one of the 13 a `run_store` test that starts `git` (the VM has no git: `program not found`); `winps::tests::stub_refuses_hash_mismatch` passed. An earlier run of the same binary had three failures, all in the test code (a `\` in `tree_names`, a `/` in the path given to `mklink`, a control file made by the test process); they are fixed in `deploy_host_conformance.rs`.

Two facts found while writing the tests:

~~~
$ icacls on a directory made by an ssh-session process, and a file made inside it by .NET and by cmd
...\acl-probe\a\b                SYSTEM:(OI)(CI)(F)   Administrators:(OI)(CI)(F)   ignwin836\svc-ignition-deploy:(OI)(CI)(F)
...\acl-probe\a\b\net.txt       SYSTEM:(F)           Administrators:(F)           ignwin836\svc-ignition-deploy:(F)
...\acl-probe\a\b\cmd.txt       SYSTEM:(F)           Administrators:(F)           ignwin836\svc-ignition-deploy:(F)

$ echo pwd | sftp -q -b - -D C:\Windows\System32\OpenSSH\sftp-server.exe
sftp> pwd
Remote working directory: /C:/Users/svc-ignition-deploy
~~~

## OQ5b (batched commit, 23-10)

The lock window of the batched commit (D-01), measured through the Windows `ApplyHost` the engine uses (`SftpTransport::new_windows(..).with_apply(..)`: `prepare`, `commit`, `restore`, all through `commit.ps1`), on the Windows capture host `ign-win-836` (computer `ignwin836`, 52.154.75.231, Windows Server 2022, Ignition 8.3.6, eastus, Standard_D2s_v4), as `svc-ignition-deploy` with the pinned host key, from the macOS runner over the public internet. Test: `oq5b_timings` of `crates/ignition-core/tests/live_transport_wflwvm01.rs` (`IGNITION_LIVE_OQ5B=1`).

| Field | Value |
| --- | --- |
| Command | `IGNITION_LIVE_WFLWVM01=1 IGNITION_LIVE_OQ5B=1 IGN_OQ5B_SCALES=<1 or 10> cargo test -p ignition-core --test live_transport_wflwvm01 oq5b_timings -- --ignored --nocapture` (`IGN_OQ5B_ITERS` 10 and `IGN_OQ5B_REPLACE_ITERS` 5, the defaults); one invocation per scale |
| Scale 1 run | 2026-10-17T23:07:28Z, 767.6 s: the 152-file corpus (70 named units, 91,833 bytes) |
| Scale 10 run | 2026-10-17T23:20:54Z, 3147.8 s: the 1,520-file corpus (700 named units, 918,330 bytes) |
| Smoke run (not pooled) | 2026-10-17T23:00:40Z, scale 1, 2 write iterations and 1 replace iteration, 391 s; it found nothing to change |
| Gateway | `ign --profile azure-win`, Ignition 8.3.6, trial `AllInDemo` live throughout (7,023 s left at 22:56Z; 2,412 s left after the scale 10 run). The reset `ign --profile azure-win --json api call --method POST --path /data/api/v1/trial` was tried once at 2026-10-17T23:20:36Z and answered `auth_rejected` (HTTP 403); the trial was live (5,596 s left) so nothing depended on it. No reset took effect during this plan |
| Defender | not readable by the account (`Cannot connect to CIM server. Access denied`, both runs); the host record in `## Windows capture host (23-01)` says real-time protection on, no exclusions, and that is the state of every number below |
| Production before and after | `config-mode.json` only, no directories; both runs end with the listing equal to the pre-run listing (`production_listing_equals_pre_run: true`), the deploy directory in the account home gone, and a closing scan of 0.79 s and 0.64 s |
| Failures | none; every commit and restore reported `failure: None`, `stale: false` with the expected moved and deleted counts; the `ign-deploy` staging and snapshots of the measurement runs are removed (measurement artefacts, not deploys) |

What each series is (all seconds, wall clock at the runner):

- `commit`: the one `ApplyHost::commit` call for a batch of NEW files plus the ledger (152 or 1,520 writes), ssh connect included. The staged copies are put back from a host-side `robocopy` copy before every iteration (`reset`, outside the timing), because a commit moves them out of staging; `prepare_*` is the one sftp upload of the batch (outside the lock).
- `scan`, `restore_scan`: a config scan requested and waited on to completion through `ign api call` (process start, keyring read and a 200 ms poll are included, so the wall time is longer than the engine's own REST client would see); `*_gateway_ms` is the gateway's `lastScanDuration`.
- `restore`: the one `ApplyHost::restore` call that undoes the commit (journal read, work list upload, script).
- `replace_*`: the same with every file REPLACED (a second version of the payload over the installed one), the costlier path because every pre-image is copied into the snapshot first. Not asked for by the plan; added because an apply of a changed tree replaces files, and the hold must cover that path.
- `commit_delete`: one commit that deletes the whole installed payload (and the directories it empties); one sample per scale as the plan says.
- Not under the lock in the engine (checked in `deploy/apply/mod.rs`): `prepare`, the post-write `fingerprint`, `finalize` and `fetch_snapshot`. The lock is taken after `prepare`, held across `commit`, the `lock.check` and the scan (the scan releases it, D12), and the fingerprint and finalize come after the scan and after `deploy_verify`.

### scale 1 (152 files) raw output

~~~
OQ5 {"host":"52.154.75.231","iters":10,"oq5b":"header","replace_iters":5,"scales":[1],"utc":"2026-10-17T23:07:32Z"}
OQ5 {"defender_as_found":{"err":"Cannot connect to CIM server. Access denied ","ok":1}}
OQ5 {"production_before":{"dirs":[],"files":["config-mode.json"]}}
OQ5 {"scan_before_anything":{"gateway_ms":390,"wall_s":0.790893542}}
OQ5 {"iters":10,"label":"b-scale1","payload":{"bytes":91833,"files":152,"source":"sanitized corpus (183 .json, 195 files) x 1, named units only","types":27,"units":70},"replace_iters":5,"utc":"2026-10-17T23:07:47Z"}
OQ5 {"extra":{"files":152},"iter":1,"label":"b-scale1","phase":"prepare_write","value":81.537}
OQ5 {"extra":{},"iter":1,"label":"b-scale1","phase":"master_copy","value":1.702}
OQ5 {"extra":{},"iter":1,"label":"b-scale1","phase":"reset","value":1.833}
OQ5 {"extra":{"deleted":0,"moved":152},"iter":1,"label":"b-scale1","phase":"commit","value":8.158}
OQ5 {"extra":{},"iter":1,"label":"b-scale1","phase":"scan","value":0.745}
OQ5 {"extra":{},"iter":1,"label":"b-scale1","phase":"scan_gateway_ms","value":554.0}
OQ5 {"extra":{"deleted":153,"moved":0},"iter":1,"label":"b-scale1","phase":"restore","value":15.364}
OQ5 {"extra":{},"iter":1,"label":"b-scale1","phase":"restore_scan","value":0.845}
OQ5 {"extra":{},"iter":1,"label":"b-scale1","phase":"restore_scan_gateway_ms","value":429.0}
OQ5 {"extra":{},"iter":2,"label":"b-scale1","phase":"reset","value":1.755}
OQ5 {"extra":{"deleted":0,"moved":152},"iter":2,"label":"b-scale1","phase":"commit","value":8.336}
OQ5 {"extra":{},"iter":2,"label":"b-scale1","phase":"scan","value":0.812}
OQ5 {"extra":{},"iter":2,"label":"b-scale1","phase":"scan_gateway_ms","value":573.0}
OQ5 {"extra":{"deleted":153,"moved":0},"iter":2,"label":"b-scale1","phase":"restore","value":15.759}
OQ5 {"extra":{},"iter":2,"label":"b-scale1","phase":"restore_scan","value":0.832}
OQ5 {"extra":{},"iter":2,"label":"b-scale1","phase":"restore_scan_gateway_ms","value":453.0}
OQ5 {"extra":{},"iter":3,"label":"b-scale1","phase":"reset","value":1.713}
OQ5 {"extra":{"deleted":0,"moved":152},"iter":3,"label":"b-scale1","phase":"commit","value":8.359}
OQ5 {"extra":{},"iter":3,"label":"b-scale1","phase":"scan","value":1.104}
OQ5 {"extra":{},"iter":3,"label":"b-scale1","phase":"scan_gateway_ms","value":592.0}
OQ5 {"extra":{"deleted":153,"moved":0},"iter":3,"label":"b-scale1","phase":"restore","value":15.283}
OQ5 {"extra":{},"iter":3,"label":"b-scale1","phase":"restore_scan","value":0.794}
OQ5 {"extra":{},"iter":3,"label":"b-scale1","phase":"restore_scan_gateway_ms","value":439.0}
OQ5 {"extra":{},"iter":4,"label":"b-scale1","phase":"reset","value":1.856}
OQ5 {"extra":{"deleted":0,"moved":152},"iter":4,"label":"b-scale1","phase":"commit","value":8.219}
OQ5 {"extra":{},"iter":4,"label":"b-scale1","phase":"scan","value":1.221}
OQ5 {"extra":{},"iter":4,"label":"b-scale1","phase":"scan_gateway_ms","value":664.0}
OQ5 {"extra":{"deleted":153,"moved":0},"iter":4,"label":"b-scale1","phase":"restore","value":15.463}
OQ5 {"extra":{},"iter":4,"label":"b-scale1","phase":"restore_scan","value":0.685}
OQ5 {"extra":{},"iter":4,"label":"b-scale1","phase":"restore_scan_gateway_ms","value":499.0}
OQ5 {"extra":{},"iter":5,"label":"b-scale1","phase":"reset","value":1.735}
OQ5 {"extra":{"deleted":0,"moved":152},"iter":5,"label":"b-scale1","phase":"commit","value":8.311}
OQ5 {"extra":{},"iter":5,"label":"b-scale1","phase":"scan","value":0.79}
OQ5 {"extra":{},"iter":5,"label":"b-scale1","phase":"scan_gateway_ms","value":553.0}
OQ5 {"extra":{"deleted":153,"moved":0},"iter":5,"label":"b-scale1","phase":"restore","value":15.103}
OQ5 {"extra":{},"iter":5,"label":"b-scale1","phase":"restore_scan","value":0.704}
OQ5 {"extra":{},"iter":5,"label":"b-scale1","phase":"restore_scan_gateway_ms","value":443.0}
OQ5 {"extra":{},"iter":6,"label":"b-scale1","phase":"reset","value":1.864}
OQ5 {"extra":{"deleted":0,"moved":152},"iter":6,"label":"b-scale1","phase":"commit","value":8.263}
OQ5 {"extra":{},"iter":6,"label":"b-scale1","phase":"scan","value":0.758}
OQ5 {"extra":{},"iter":6,"label":"b-scale1","phase":"scan_gateway_ms","value":541.0}
OQ5 {"extra":{"deleted":153,"moved":0},"iter":6,"label":"b-scale1","phase":"restore","value":15.555}
OQ5 {"extra":{},"iter":6,"label":"b-scale1","phase":"restore_scan","value":0.822}
OQ5 {"extra":{},"iter":6,"label":"b-scale1","phase":"restore_scan_gateway_ms","value":490.0}
OQ5 {"extra":{},"iter":7,"label":"b-scale1","phase":"reset","value":1.674}
OQ5 {"extra":{"deleted":0,"moved":152},"iter":7,"label":"b-scale1","phase":"commit","value":8.258}
OQ5 {"extra":{},"iter":7,"label":"b-scale1","phase":"scan","value":0.837}
OQ5 {"extra":{},"iter":7,"label":"b-scale1","phase":"scan_gateway_ms","value":556.0}
OQ5 {"extra":{"deleted":153,"moved":0},"iter":7,"label":"b-scale1","phase":"restore","value":15.267}
OQ5 {"extra":{},"iter":7,"label":"b-scale1","phase":"restore_scan","value":0.807}
OQ5 {"extra":{},"iter":7,"label":"b-scale1","phase":"restore_scan_gateway_ms","value":416.0}
OQ5 {"extra":{},"iter":8,"label":"b-scale1","phase":"reset","value":1.921}
OQ5 {"extra":{"deleted":0,"moved":152},"iter":8,"label":"b-scale1","phase":"commit","value":8.374}
OQ5 {"extra":{},"iter":8,"label":"b-scale1","phase":"scan","value":1.064}
OQ5 {"extra":{},"iter":8,"label":"b-scale1","phase":"scan_gateway_ms","value":573.0}
OQ5 {"extra":{"deleted":153,"moved":0},"iter":8,"label":"b-scale1","phase":"restore","value":15.723}
OQ5 {"extra":{},"iter":8,"label":"b-scale1","phase":"restore_scan","value":0.812}
OQ5 {"extra":{},"iter":8,"label":"b-scale1","phase":"restore_scan_gateway_ms","value":424.0}
OQ5 {"extra":{},"iter":9,"label":"b-scale1","phase":"reset","value":1.888}
OQ5 {"extra":{"deleted":0,"moved":152},"iter":9,"label":"b-scale1","phase":"commit","value":8.266}
OQ5 {"extra":{},"iter":9,"label":"b-scale1","phase":"scan","value":1.047}
OQ5 {"extra":{},"iter":9,"label":"b-scale1","phase":"scan_gateway_ms","value":572.0}
OQ5 {"extra":{"deleted":153,"moved":0},"iter":9,"label":"b-scale1","phase":"restore","value":16.006}
OQ5 {"extra":{},"iter":9,"label":"b-scale1","phase":"restore_scan","value":0.704}
OQ5 {"extra":{},"iter":9,"label":"b-scale1","phase":"restore_scan_gateway_ms","value":439.0}
OQ5 {"extra":{},"iter":10,"label":"b-scale1","phase":"reset","value":1.923}
OQ5 {"extra":{"deleted":0,"moved":152},"iter":10,"label":"b-scale1","phase":"commit","value":8.731}
OQ5 {"extra":{},"iter":10,"label":"b-scale1","phase":"scan","value":1.097}
OQ5 {"extra":{},"iter":10,"label":"b-scale1","phase":"scan_gateway_ms","value":705.0}
OQ5 {"extra":{"deleted":153,"moved":0},"iter":10,"label":"b-scale1","phase":"restore","value":14.849}
OQ5 {"extra":{},"iter":10,"label":"b-scale1","phase":"restore_scan","value":0.671}
OQ5 {"extra":{},"iter":10,"label":"b-scale1","phase":"restore_scan_gateway_ms","value":438.0}
OQ5 {"extra":{},"iter":1,"label":"b-scale1","phase":"install_scan","value":0.976}
OQ5 {"extra":{"files":152},"iter":1,"label":"b-scale1","phase":"prepare_replace","value":79.436}
OQ5 {"extra":{},"iter":1,"label":"b-scale1","phase":"replace_reset","value":1.753}
OQ5 {"extra":{"deleted":0,"moved":152},"iter":1,"label":"b-scale1","phase":"replace_commit","value":10.065}
OQ5 {"extra":{},"iter":1,"label":"b-scale1","phase":"replace_scan","value":0.832}
OQ5 {"extra":{},"iter":1,"label":"b-scale1","phase":"replace_scan_gateway_ms","value":651.0}
OQ5 {"extra":{"deleted":0,"moved":152},"iter":1,"label":"b-scale1","phase":"replace_restore","value":20.064}
OQ5 {"extra":{},"iter":1,"label":"b-scale1","phase":"replace_restore_scan","value":1.185}
OQ5 {"extra":{},"iter":1,"label":"b-scale1","phase":"replace_restore_scan_gateway_ms","value":590.0}
OQ5 {"extra":{},"iter":2,"label":"b-scale1","phase":"replace_reset","value":1.837}
OQ5 {"extra":{"deleted":0,"moved":152},"iter":2,"label":"b-scale1","phase":"replace_commit","value":10.123}
OQ5 {"extra":{},"iter":2,"label":"b-scale1","phase":"replace_scan","value":0.789}
OQ5 {"extra":{},"iter":2,"label":"b-scale1","phase":"replace_scan_gateway_ms","value":544.0}
OQ5 {"extra":{"deleted":0,"moved":152},"iter":2,"label":"b-scale1","phase":"replace_restore","value":18.906}
OQ5 {"extra":{},"iter":2,"label":"b-scale1","phase":"replace_restore_scan","value":0.755}
OQ5 {"extra":{},"iter":2,"label":"b-scale1","phase":"replace_restore_scan_gateway_ms","value":546.0}
OQ5 {"extra":{},"iter":3,"label":"b-scale1","phase":"replace_reset","value":1.875}
OQ5 {"extra":{"deleted":0,"moved":152},"iter":3,"label":"b-scale1","phase":"replace_commit","value":10.045}
OQ5 {"extra":{},"iter":3,"label":"b-scale1","phase":"replace_scan","value":0.929}
OQ5 {"extra":{},"iter":3,"label":"b-scale1","phase":"replace_scan_gateway_ms","value":544.0}
OQ5 {"extra":{"deleted":0,"moved":152},"iter":3,"label":"b-scale1","phase":"replace_restore","value":19.216}
OQ5 {"extra":{},"iter":3,"label":"b-scale1","phase":"replace_restore_scan","value":1.073}
OQ5 {"extra":{},"iter":3,"label":"b-scale1","phase":"replace_restore_scan_gateway_ms","value":551.0}
OQ5 {"extra":{},"iter":4,"label":"b-scale1","phase":"replace_reset","value":1.788}
OQ5 {"extra":{"deleted":0,"moved":152},"iter":4,"label":"b-scale1","phase":"replace_commit","value":9.879}
OQ5 {"extra":{},"iter":4,"label":"b-scale1","phase":"replace_scan","value":1.158}
OQ5 {"extra":{},"iter":4,"label":"b-scale1","phase":"replace_scan_gateway_ms","value":569.0}
OQ5 {"extra":{"deleted":0,"moved":152},"iter":4,"label":"b-scale1","phase":"replace_restore","value":18.591}
OQ5 {"extra":{},"iter":4,"label":"b-scale1","phase":"replace_restore_scan","value":0.727}
OQ5 {"extra":{},"iter":4,"label":"b-scale1","phase":"replace_restore_scan_gateway_ms","value":562.0}
OQ5 {"extra":{},"iter":5,"label":"b-scale1","phase":"replace_reset","value":1.691}
OQ5 {"extra":{"deleted":0,"moved":152},"iter":5,"label":"b-scale1","phase":"replace_commit","value":9.802}
OQ5 {"extra":{},"iter":5,"label":"b-scale1","phase":"replace_scan","value":0.778}
OQ5 {"extra":{},"iter":5,"label":"b-scale1","phase":"replace_scan_gateway_ms","value":559.0}
OQ5 {"extra":{"deleted":0,"moved":152},"iter":5,"label":"b-scale1","phase":"replace_restore","value":21.026}
OQ5 {"extra":{},"iter":5,"label":"b-scale1","phase":"replace_restore_scan","value":0.914}
OQ5 {"extra":{},"iter":5,"label":"b-scale1","phase":"replace_restore_scan_gateway_ms","value":572.0}
OQ5 {"extra":{"files":152},"iter":1,"label":"b-scale1","phase":"prepare_delete","value":21.218}
OQ5 {"extra":{"deleted":152,"removed_dirs":70},"iter":1,"label":"b-scale1","phase":"commit_delete","value":6.13}
OQ5 {"extra":{},"iter":1,"label":"b-scale1","phase":"scan_after_delete","value":0.679}
OQ5 {"extra":{},"iter":1,"label":"b-scale1","phase":"scan_after_delete_gateway_ms","value":447.0}
OQ5 {"summary":{"bytes":91833,"files":152,"label":"b-scale1","percentile":"nearest rank","phases":{"commit":{"max":8.730658125,"min":8.157653875,"n":10,"p50":8.266489791,"p95":8.730658125},"commit_delete":{"max":6.1295775,"min":6.1295775,"n":1,"p50":6.1295775,"p95":6.1295775},"install_scan":{"max":0.976113208,"min":0.976113208,"n":1,"p50":0.976113208,"p95":0.976113208},"master_copy":{"max":1.702039875,"min":1.702039875,"n":1,"p50":1.702039875,"p95":1.702039875},"prepare_delete":{"max":21.217672834,"min":21.217672834,"n":1,"p50":21.217672834,"p95":21.217672834},"prepare_replace":{"max":79.435769916,"min":79.435769916,"n":1,"p50":79.435769916,"p95":79.435769916},"prepare_write":{"max":81.536957791,"min":81.536957791,"n":1,"p50":81.536957791,"p95":81.536957791},"replace_commit":{"max":10.123231709,"min":9.801627667,"n":5,"p50":10.044742083,"p95":10.123231709},"replace_reset":{"max":1.874855083,"min":1.690589917,"n":5,"p50":1.788394,"p95":1.874855083},"replace_restore":{"max":21.026326958,"min":18.591281666,"n":5,"p50":19.215784,"p95":21.026326958},"replace_restore_scan":{"max":1.184980083,"min":0.726859334,"n":5,"p50":0.914096125,"p95":1.184980083},"replace_restore_scan_gateway_ms":{"max":590.0,"min":546.0,"n":5,"p50":562.0,"p95":590.0},"replace_scan":{"max":1.157903959,"min":0.778244125,"n":5,"p50":0.831806375,"p95":1.157903959},"replace_scan_gateway_ms":{"max":651.0,"min":544.0,"n":5,"p50":559.0,"p95":651.0},"reset":{"max":1.922878709,"min":1.6744531249999999,"n":10,"p50":1.8326245829999999,"p95":1.922878709},"restore":{"max":16.00593125,"min":14.848553208,"n":10,"p50":15.363554333,"p95":16.00593125},"restore_scan":{"max":0.844810417,"min":0.671075917,"n":10,"p50":0.794310959,"p95":0.844810417},"restore_scan_gateway_ms":{"max":499.0,"min":416.0,"n":10,"p50":439.0,"p95":499.0},"scan":{"max":1.220650209,"min":0.745441541,"n":10,"p50":0.837305209,"p95":1.220650209},"scan_after_delete":{"max":0.678961792,"min":0.678961792,"n":1,"p50":0.678961792,"p95":0.678961792},"scan_after_delete_gateway_ms":{"max":447.0,"min":447.0,"n":1,"p50":447.0,"p95":447.0},"scan_gateway_ms":{"max":705.0,"min":541.0,"n":10,"p50":572.0,"p95":705.0}},"source":"sanitized corpus (183 .json, 195 files) x 1, named units only","unit":"seconds unless the phase name ends in _ms","units":70}}
OQ5 {"restore_assertion":{"dirs_after":0,"dirs_before":0,"files_after":1,"files_before":1,"production_listing_equals_pre_run":true}}
OQ5 {"closing_scan":{"gateway_ms":433,"wall_s":0.788610083}}
OQ5 {"cleanup":{"deploy_dir_gone":true},"total_secs":767.614461375}
PASS oq5b_timings
~~~

### scale 10 (1,520 files) raw output

~~~
OQ5 {"host":"52.154.75.231","iters":10,"oq5b":"header","replace_iters":5,"scales":[10],"utc":"2026-10-17T23:20:58Z"}
OQ5 {"defender_as_found":{"err":"Cannot connect to CIM server. Access denied ","ok":1}}
OQ5 {"production_before":{"dirs":[],"files":["config-mode.json"]}}
OQ5 {"scan_before_anything":{"gateway_ms":438,"wall_s":0.77563975}}
OQ5 {"iters":10,"label":"b-scale10","payload":{"bytes":918330,"files":1520,"source":"sanitized corpus (183 .json, 195 files) x 10, named units only","types":27,"units":700},"replace_iters":5,"utc":"2026-10-17T23:21:13Z"}
OQ5 {"extra":{"files":1520},"iter":1,"label":"b-scale10","phase":"prepare_write","value":553.482}
OQ5 {"extra":{},"iter":1,"label":"b-scale10","phase":"master_copy","value":3.343}
OQ5 {"extra":{},"iter":1,"label":"b-scale10","phase":"reset","value":3.654}
OQ5 {"extra":{"deleted":0,"moved":1520},"iter":1,"label":"b-scale10","phase":"commit","value":56.367}
OQ5 {"extra":{},"iter":1,"label":"b-scale10","phase":"scan","value":1.106}
OQ5 {"extra":{},"iter":1,"label":"b-scale10","phase":"scan_gateway_ms","value":789.0}
OQ5 {"extra":{"deleted":1521,"moved":0},"iter":1,"label":"b-scale10","phase":"restore","value":27.054}
OQ5 {"extra":{},"iter":1,"label":"b-scale10","phase":"restore_scan","value":1.096}
OQ5 {"extra":{},"iter":1,"label":"b-scale10","phase":"restore_scan_gateway_ms","value":570.0}
OQ5 {"extra":{},"iter":2,"label":"b-scale10","phase":"reset","value":5.57}
OQ5 {"extra":{"deleted":0,"moved":1520},"iter":2,"label":"b-scale10","phase":"commit","value":59.6}
OQ5 {"extra":{},"iter":2,"label":"b-scale10","phase":"scan","value":1.567}
OQ5 {"extra":{},"iter":2,"label":"b-scale10","phase":"scan_gateway_ms","value":817.0}
OQ5 {"extra":{"deleted":1521,"moved":0},"iter":2,"label":"b-scale10","phase":"restore","value":28.937}
OQ5 {"extra":{},"iter":2,"label":"b-scale10","phase":"restore_scan","value":0.86}
OQ5 {"extra":{},"iter":2,"label":"b-scale10","phase":"restore_scan_gateway_ms","value":583.0}
OQ5 {"extra":{},"iter":3,"label":"b-scale10","phase":"reset","value":5.597}
OQ5 {"extra":{"deleted":0,"moved":1520},"iter":3,"label":"b-scale10","phase":"commit","value":60.185}
OQ5 {"extra":{},"iter":3,"label":"b-scale10","phase":"scan","value":1.055}
OQ5 {"extra":{},"iter":3,"label":"b-scale10","phase":"scan_gateway_ms","value":818.0}
OQ5 {"extra":{"deleted":1521,"moved":0},"iter":3,"label":"b-scale10","phase":"restore","value":27.711}
OQ5 {"extra":{},"iter":3,"label":"b-scale10","phase":"restore_scan","value":0.854}
OQ5 {"extra":{},"iter":3,"label":"b-scale10","phase":"restore_scan_gateway_ms","value":567.0}
OQ5 {"extra":{},"iter":4,"label":"b-scale10","phase":"reset","value":5.817}
OQ5 {"extra":{"deleted":0,"moved":1520},"iter":4,"label":"b-scale10","phase":"commit","value":61.772}
OQ5 {"extra":{},"iter":4,"label":"b-scale10","phase":"scan","value":1.205}
OQ5 {"extra":{},"iter":4,"label":"b-scale10","phase":"scan_gateway_ms","value":846.0}
OQ5 {"extra":{"deleted":1521,"moved":0},"iter":4,"label":"b-scale10","phase":"restore","value":26.487}
OQ5 {"extra":{},"iter":4,"label":"b-scale10","phase":"restore_scan","value":0.803}
OQ5 {"extra":{},"iter":4,"label":"b-scale10","phase":"restore_scan_gateway_ms","value":567.0}
OQ5 {"extra":{},"iter":5,"label":"b-scale10","phase":"reset","value":6.157}
OQ5 {"extra":{"deleted":0,"moved":1520},"iter":5,"label":"b-scale10","phase":"commit","value":61.487}
OQ5 {"extra":{},"iter":5,"label":"b-scale10","phase":"scan","value":1.106}
OQ5 {"extra":{},"iter":5,"label":"b-scale10","phase":"scan_gateway_ms","value":832.0}
OQ5 {"extra":{"deleted":1521,"moved":0},"iter":5,"label":"b-scale10","phase":"restore","value":27.134}
OQ5 {"extra":{},"iter":5,"label":"b-scale10","phase":"restore_scan","value":0.964}
OQ5 {"extra":{},"iter":5,"label":"b-scale10","phase":"restore_scan_gateway_ms","value":587.0}
OQ5 {"extra":{},"iter":6,"label":"b-scale10","phase":"reset","value":6.838}
OQ5 {"extra":{"deleted":0,"moved":1520},"iter":6,"label":"b-scale10","phase":"commit","value":59.349}
OQ5 {"extra":{},"iter":6,"label":"b-scale10","phase":"scan","value":1.079}
OQ5 {"extra":{},"iter":6,"label":"b-scale10","phase":"scan_gateway_ms","value":814.0}
OQ5 {"extra":{"deleted":1521,"moved":0},"iter":6,"label":"b-scale10","phase":"restore","value":25.905}
OQ5 {"extra":{},"iter":6,"label":"b-scale10","phase":"restore_scan","value":0.749}
OQ5 {"extra":{},"iter":6,"label":"b-scale10","phase":"restore_scan_gateway_ms","value":561.0}
OQ5 {"extra":{},"iter":7,"label":"b-scale10","phase":"reset","value":5.246}
OQ5 {"extra":{"deleted":0,"moved":1520},"iter":7,"label":"b-scale10","phase":"commit","value":61.157}
OQ5 {"extra":{},"iter":7,"label":"b-scale10","phase":"scan","value":1.171}
OQ5 {"extra":{},"iter":7,"label":"b-scale10","phase":"scan_gateway_ms","value":844.0}
OQ5 {"extra":{"deleted":1521,"moved":0},"iter":7,"label":"b-scale10","phase":"restore","value":28.67}
OQ5 {"extra":{},"iter":7,"label":"b-scale10","phase":"restore_scan","value":1.213}
OQ5 {"extra":{},"iter":7,"label":"b-scale10","phase":"restore_scan_gateway_ms","value":575.0}
OQ5 {"extra":{},"iter":8,"label":"b-scale10","phase":"reset","value":5.44}
OQ5 {"extra":{"deleted":0,"moved":1520},"iter":8,"label":"b-scale10","phase":"commit","value":60.565}
OQ5 {"extra":{},"iter":8,"label":"b-scale10","phase":"scan","value":1.113}
OQ5 {"extra":{},"iter":8,"label":"b-scale10","phase":"scan_gateway_ms","value":792.0}
OQ5 {"extra":{"deleted":1521,"moved":0},"iter":8,"label":"b-scale10","phase":"restore","value":28.591}
OQ5 {"extra":{},"iter":8,"label":"b-scale10","phase":"restore_scan","value":1.033}
OQ5 {"extra":{},"iter":8,"label":"b-scale10","phase":"restore_scan_gateway_ms","value":628.0}
OQ5 {"extra":{},"iter":9,"label":"b-scale10","phase":"reset","value":5.403}
OQ5 {"extra":{"deleted":0,"moved":1520},"iter":9,"label":"b-scale10","phase":"commit","value":60.291}
OQ5 {"extra":{},"iter":9,"label":"b-scale10","phase":"scan","value":1.146}
OQ5 {"extra":{},"iter":9,"label":"b-scale10","phase":"scan_gateway_ms","value":783.0}
OQ5 {"extra":{"deleted":1521,"moved":0},"iter":9,"label":"b-scale10","phase":"restore","value":27.134}
OQ5 {"extra":{},"iter":9,"label":"b-scale10","phase":"restore_scan","value":0.826}
OQ5 {"extra":{},"iter":9,"label":"b-scale10","phase":"restore_scan_gateway_ms","value":572.0}
OQ5 {"extra":{},"iter":10,"label":"b-scale10","phase":"reset","value":6.267}
OQ5 {"extra":{"deleted":0,"moved":1520},"iter":10,"label":"b-scale10","phase":"commit","value":62.04}
OQ5 {"extra":{},"iter":10,"label":"b-scale10","phase":"scan","value":1.18}
OQ5 {"extra":{},"iter":10,"label":"b-scale10","phase":"scan_gateway_ms","value":806.0}
OQ5 {"extra":{"deleted":1521,"moved":0},"iter":10,"label":"b-scale10","phase":"restore","value":26.578}
OQ5 {"extra":{},"iter":10,"label":"b-scale10","phase":"restore_scan","value":0.734}
OQ5 {"extra":{},"iter":10,"label":"b-scale10","phase":"restore_scan_gateway_ms","value":561.0}
OQ5 {"extra":{},"iter":1,"label":"b-scale10","phase":"install_scan","value":1.212}
OQ5 {"extra":{"files":1520},"iter":1,"label":"b-scale10","phase":"prepare_replace","value":533.198}
OQ5 {"extra":{},"iter":1,"label":"b-scale10","phase":"replace_reset","value":3.602}
OQ5 {"extra":{"deleted":0,"moved":1520},"iter":1,"label":"b-scale10","phase":"replace_commit","value":77.425}
OQ5 {"extra":{},"iter":1,"label":"b-scale10","phase":"replace_scan","value":1.549}
OQ5 {"extra":{},"iter":1,"label":"b-scale10","phase":"replace_scan_gateway_ms","value":952.0}
OQ5 {"extra":{"deleted":0,"moved":1520},"iter":1,"label":"b-scale10","phase":"replace_restore","value":71.839}
OQ5 {"extra":{},"iter":1,"label":"b-scale10","phase":"replace_restore_scan","value":1.542}
OQ5 {"extra":{},"iter":1,"label":"b-scale10","phase":"replace_restore_scan_gateway_ms","value":1020.0}
OQ5 {"extra":{},"iter":2,"label":"b-scale10","phase":"replace_reset","value":5.689}
OQ5 {"extra":{"deleted":0,"moved":1520},"iter":2,"label":"b-scale10","phase":"replace_commit","value":83.409}
OQ5 {"extra":{},"iter":2,"label":"b-scale10","phase":"replace_scan","value":1.57}
OQ5 {"extra":{},"iter":2,"label":"b-scale10","phase":"replace_scan_gateway_ms","value":1004.0}
OQ5 {"extra":{"deleted":0,"moved":1520},"iter":2,"label":"b-scale10","phase":"replace_restore","value":69.267}
OQ5 {"extra":{},"iter":2,"label":"b-scale10","phase":"replace_restore_scan","value":1.521}
OQ5 {"extra":{},"iter":2,"label":"b-scale10","phase":"replace_restore_scan_gateway_ms","value":995.0}
OQ5 {"extra":{},"iter":3,"label":"b-scale10","phase":"replace_reset","value":5.28}
OQ5 {"extra":{"deleted":0,"moved":1520},"iter":3,"label":"b-scale10","phase":"replace_commit","value":85.496}
OQ5 {"extra":{},"iter":3,"label":"b-scale10","phase":"replace_scan","value":1.497}
OQ5 {"extra":{},"iter":3,"label":"b-scale10","phase":"replace_scan_gateway_ms","value":1133.0}
OQ5 {"extra":{"deleted":0,"moved":1520},"iter":3,"label":"b-scale10","phase":"replace_restore","value":70.86}
OQ5 {"extra":{},"iter":3,"label":"b-scale10","phase":"replace_restore_scan","value":1.48}
OQ5 {"extra":{},"iter":3,"label":"b-scale10","phase":"replace_restore_scan_gateway_ms","value":995.0}
OQ5 {"extra":{},"iter":4,"label":"b-scale10","phase":"replace_reset","value":4.993}
OQ5 {"extra":{"deleted":0,"moved":1520},"iter":4,"label":"b-scale10","phase":"replace_commit","value":84.559}
OQ5 {"extra":{},"iter":4,"label":"b-scale10","phase":"replace_scan","value":1.461}
OQ5 {"extra":{},"iter":4,"label":"b-scale10","phase":"replace_scan_gateway_ms","value":952.0}
OQ5 {"extra":{"deleted":0,"moved":1520},"iter":4,"label":"b-scale10","phase":"replace_restore","value":70.136}
OQ5 {"extra":{},"iter":4,"label":"b-scale10","phase":"replace_restore_scan","value":1.475}
OQ5 {"extra":{},"iter":4,"label":"b-scale10","phase":"replace_restore_scan_gateway_ms","value":988.0}
OQ5 {"extra":{},"iter":5,"label":"b-scale10","phase":"replace_reset","value":5.108}
OQ5 {"extra":{"deleted":0,"moved":1520},"iter":5,"label":"b-scale10","phase":"replace_commit","value":83.092}
OQ5 {"extra":{},"iter":5,"label":"b-scale10","phase":"replace_scan","value":1.579}
OQ5 {"extra":{},"iter":5,"label":"b-scale10","phase":"replace_scan_gateway_ms","value":987.0}
OQ5 {"extra":{"deleted":0,"moved":1520},"iter":5,"label":"b-scale10","phase":"replace_restore","value":71.027}
OQ5 {"extra":{},"iter":5,"label":"b-scale10","phase":"replace_restore_scan","value":1.235}
OQ5 {"extra":{},"iter":5,"label":"b-scale10","phase":"replace_restore_scan_gateway_ms","value":948.0}
OQ5 {"extra":{"files":1520},"iter":1,"label":"b-scale10","phase":"prepare_delete","value":26.171}
OQ5 {"extra":{"deleted":1520,"removed_dirs":700},"iter":1,"label":"b-scale10","phase":"commit_delete","value":34.459}
OQ5 {"extra":{},"iter":1,"label":"b-scale10","phase":"scan_after_delete","value":1.098}
OQ5 {"extra":{},"iter":1,"label":"b-scale10","phase":"scan_after_delete_gateway_ms","value":569.0}
OQ5 {"summary":{"bytes":918330,"files":1520,"label":"b-scale10","percentile":"nearest rank","phases":{"commit":{"max":62.0403075,"min":56.367028917,"n":10,"p50":60.2908875,"p95":62.0403075},"commit_delete":{"max":34.459301125,"min":34.459301125,"n":1,"p50":34.459301125,"p95":34.459301125},"install_scan":{"max":1.211642834,"min":1.211642834,"n":1,"p50":1.211642834,"p95":1.211642834},"master_copy":{"max":3.342881,"min":3.342881,"n":1,"p50":3.342881,"p95":3.342881},"prepare_delete":{"max":26.171453667,"min":26.171453667,"n":1,"p50":26.171453667,"p95":26.171453667},"prepare_replace":{"max":533.197846208,"min":533.197846208,"n":1,"p50":533.197846208,"p95":533.197846208},"prepare_write":{"max":553.482100458,"min":553.482100458,"n":1,"p50":553.482100458,"p95":553.482100458},"replace_commit":{"max":85.495882458,"min":77.424884333,"n":5,"p50":83.409136542,"p95":85.495882458},"replace_reset":{"max":5.688521791,"min":3.602199125,"n":5,"p50":5.108476625,"p95":5.688521791},"replace_restore":{"max":71.83902125,"min":69.266901292,"n":5,"p50":70.859791042,"p95":71.83902125},"replace_restore_scan":{"max":1.5422485,"min":1.234754125,"n":5,"p50":1.480215791,"p95":1.5422485},"replace_restore_scan_gateway_ms":{"max":1020.0,"min":948.0,"n":5,"p50":995.0,"p95":1020.0},"replace_scan":{"max":1.578508792,"min":1.461005125,"n":5,"p50":1.549346084,"p95":1.578508792},"replace_scan_gateway_ms":{"max":1133.0,"min":952.0,"n":5,"p50":987.0,"p95":1133.0},"reset":{"max":6.8375907080000005,"min":3.654172833,"n":10,"p50":5.569739709,"p95":6.8375907080000005},"restore":{"max":28.937364209,"min":25.905104,"n":10,"p50":27.133897417,"p95":28.937364209},"restore_scan":{"max":1.213304291,"min":0.73444175,"n":10,"p50":0.854057375,"p95":1.213304291},"restore_scan_gateway_ms":{"max":628.0,"min":561.0,"n":10,"p50":570.0,"p95":628.0},"scan":{"max":1.566750417,"min":1.055023292,"n":10,"p50":1.112894833,"p95":1.566750417},"scan_after_delete":{"max":1.097678209,"min":1.097678209,"n":1,"p50":1.097678209,"p95":1.097678209},"scan_after_delete_gateway_ms":{"max":569.0,"min":569.0,"n":1,"p50":569.0,"p95":569.0},"scan_gateway_ms":{"max":846.0,"min":783.0,"n":10,"p50":814.0,"p95":846.0}},"source":"sanitized corpus (183 .json, 195 files) x 10, named units only","unit":"seconds unless the phase name ends in _ms","units":700}}
OQ5 {"restore_assertion":{"dirs_after":0,"dirs_before":0,"files_after":1,"files_before":1,"production_listing_equals_pre_run":true}}
OQ5 {"closing_scan":{"gateway_ms":425,"wall_s":0.63946875}}
OQ5 {"cleanup":{"deploy_dir_gone":true},"total_secs":3147.775127458}
PASS oq5b_timings
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

### C2

Rests on `## C2` (sub-sections C2a, C2c, C2d, C2e) and `## C2b (manual)`. One answer per sub-probe:

- (a) Token A held the lock (hold 60 s). Token B's `POST resources/ignition/database-connection` was not refused and did not fail fast: it blocked for 59.94 s (`total=59.943446s`) and answered `200` with `success: true` at about the moment the hold lapsed; the lock read free (`204`) right after. So the scan-lock does hold back an API resource write by another actor until the hold ends (timeout) or a scan runs; it queues the write, it does not return a refusal status. Token B could read the lock meanwhile (`200`, holder `api-token:cap-a:<ip>`). The write landed in collection `core`, not `production`.
- (b) Not run. A Designer or gateway-UI save under a held lock is the interactive check in `## C2b (manual)`; its result is PENDING (human check, owed to the user). Until it is recorded, the Designer preflight stays the only assumed guard against a UI save during an apply.
- (c) No lock, a valid resource directory written into `production/` by a plain file write, no scan request: not loaded. 24 polls over 120 s, `lastScanTimestamp` unchanged (1792160608501) at every poll, the resource never appeared in the REST view. The gateway does not scan on its own for a file dropped into a mode collection within 120 s.
- (d) The same while token A held the lock (hold 150 s, remaining 149 at t+0 and 29 at t+121): not loaded, `lastScanTimestamp` unchanged (1792160732840) at all 24 polls, never in the REST view.
- (e) A 15 s hold, a file written under it, no scan: the lock read free from t+15 s (remaining 14, 9, 4, then free); `lastScanTimestamp` stayed 1792160856764 at all 25 polls to t+120 s and the resource never appeared. A lapsing lock does not trigger a scan.

Consequences for the engine (23-05, 23-17): the lock does not stop a filesystem write, and a file written without the engine's own scan stays unloaded (within 120 s) whether the lock is held, lapsed or never taken. So a fault that kills the engine, or a lock that lapses, after files moved leaves them on disk unloaded rather than applied; the next scan by any actor would load them (any actor's scan runs, Phase 20 D4), which is why rollback restores the disk and does not rely on the absence of a scan. The lock does queue other actors' API writes for up to the hold, so a hold sized for an apply delays a concurrent API writer for the whole window.

### C11

Rests on `## C11`. A tag provider and its tag definitions written together (the CAP21 unit: tag-provider, two tag-groups, tag-type-definition, tag-definition with FolderA) load in ONE config scan. Immediately after the first scan completed (Phase 20 rule), `resources/list/ignition/tag-provider` listed `CAP21` (collection `production`) and `tags/export?provider=CAP21&type=json` returned FolderA with its tags (`BigInt`, `Unicode`, `Expr`, `F1`, `F2`, `Inst1`) and the UDT type `CAP21_Type` under `_types_`. No wait and no second scan were needed, so the engine needs one scan for a provider and its definitions. Limit: six tags and one UDT; a large tree was not tried.

### C-gwbk

Rests on `## C-gwbk` (`type=all`, the type D-13 uses).

- A `.gwbk` is a ZIP: first bytes `504b0304`, `file` reports "Zip archive data, deflate", `unzip -tq` reports no errors, Python `zipfile.is_zipfile` is True with 1597 entries and `testzip()` None (every CRC reads).
- Size and time on the rig: 3,674,513 bytes (12,085,800 bytes uncompressed, 1597 files); `ign backup download` took 744 ms wall-clock, the raw `curl` of the route 1.13 s.
- Content-Length is NOT sent. The download answers `200`, `Transfer-Encoding: chunked`, `Content-Type: application/x-ignition-gateway-backup;charset=utf-8` and `Content-Disposition: attachment; filename="<gateway>_Ignition-backup-<yyyymmdd>-<hhmm>.gwbk"`. A byte count cannot be compared with a header, so size alone is no completeness check.
- A copy truncated to half (1,837,256 bytes) fails at once: `unzip -tq` exit 9 "End-of-central-directory signature not found", Python `is_zipfile` False and `ZipFile()` raises `BadZipFile: File is not a zip file`. The ZIP central directory sits at the end of the file, so a truncated download cannot read as a ZIP.
- The completeness check for D-13: the downloaded file must open as a ZIP (end-of-central-directory record found, central directory readable) with at least one entry; a CRC walk (`testzip`) is cheap at this size and may be added. What `download_to_file` itself does when the connection dies mid-stream (a chunked body cut short) was not exercised here; the ZIP open is the check that does not depend on it.
- Handling: the file was downloaded into a temp directory outside the repo and deleted afterwards (`ls` after deletion: no such directory); only size, time, headers, the entry listing (first 60 lines, totals) and the sha-256 of the original were recorded.

### C-script-run

Rests on `## C-script-run` (the live run of 2026-10-17T20:22:30Z on `ign-win-836`; an earlier run at 18:58:30Z gave the same probe answers).

- **Mechanism: `[ScriptBlock]::Create` over the bytes the stub hashed.** The host's execution policy is `LocalMachine=Unrestricted` (every other scope Undefined), language mode `FullLanguage`. `& ([ScriptBlock]::Create([IO.File]::ReadAllText(path))) x y` printed the script's `IGN-RESULT` line with exit 0 under the policy as found, under `-ExecutionPolicy AllSigned` and under `-ExecutionPolicy Restricted`. The stub of `winps::script_stub` therefore reads `commit.ps1` and `work.tsv` once each as bytes, compares the SHA-256 of those bytes with the runner's values, and runs the very text it hashed through `[ScriptBlock]::Create` (it passes the operation, the three roots and the text of the work list as arguments, so the list the script acts on is the list that was checked).
- **What `-File` does instead:** exit 0 under the policy as found (Unrestricted) and under `Bypass`; exit 1 under `AllSigned` ("is not digitally signed") and under `Restricted` ("running scripts is disabled on this system"). A `-File` stub would work on this host and stop working on any host whose policy is stricter, so it is not used. A staged `.ps1` is acceptable only because the stub never hands it to the policy.
- **Encoded length:** the stub for the real roots (staging 95 characters, the real collection and deploy directories) is 4014 characters against `COMMAND_BUDGET` 8000. With every root at the longest a legal node can make (150-character data directory, staging under 240) the unit test `stub_fits_the_budget` measures 5678 characters at 240-character roots and 5882 at 259, so the budget holds for every legal node. Paths travel only in the base64 JSON parameters; the command text carries no path.
- **Tamper stops at the stub:** a `work.tsv` replaced after `prepare` and a `commit.ps1` replaced after `prepare` each end in `transport_failed` ("the staged script or work list does not match its hash; nothing ran"), exit 97 from the stub, no journal on the host and the collection untouched.
- **Replacing a file inside one script (PowerShell 5.1, `\\?\` paths):** `Move-Item -LiteralPath .. -Destination .. -Force` and `[IO.File]::Replace(src, dst, [NullString]::Value)` both replace an existing target when it is free or held open with a share mode that allows delete (`Delete`, `ReadWrite,Delete`), and both fail, leaving target and staged copy as they were, when it is held open with `None`, `Read` or `ReadWrite`. `[IO.File]::Move` refuses any existing target. `Move-Item` names a held target with the same words as an existing one ("Cannot create a file when that file already exists."), so the script reports a failed move by its index and a fixed sentence, never by that text. `Move-Item -Force` onto its own path reports no error and leaves the file, unlike an sftp rename onto itself (C7a-4); `commit.ps1` still refuses a self-move before it tries. `Move-Item -LiteralPath/-Destination` handled a name with a non-ASCII character, a space and a comma (`Café DB,1`) in the batch of the same run. `commit.ps1` uses `Move-Item -Force`, retried `RENAME_RETRIES` times with a one-second pause, then reports the entry as failed.
- **A trap found on the way:** Windows PowerShell resolves an alias before a function. A helper named `LP` (the alias of `Out-Printer`) was never called and every hash came back empty on the first run; the helper is `Xp` now and the unit test `the_shipped_script_names_no_function_like_an_alias_or_a_cmdlet` holds the script to the alias list of this host.
- **Timing of one call on the host:** the commit of that batch (journal, snapshot, two moves, a delete, the emptied directory, ledger) was one `exec_powershell` call of 3.06 s (2.69 s in the first run); its restore (an upload of the work list and journal into a throw-away directory, the journal read, the script) took 14.59 s wall-clock (15.18 s) of which the script is a small part; the 29.04 s (28.87 s) of `prepare` is the sftp upload and is outside the lock. Every sftp batch on this link costs a fresh ssh handshake.

### C7b

Rests on `## C7b` (run 1 for the access list before the ACL step, run 2 for everything after it).

- **ACL of a moved file (A5, Pitfall 7).** The staged copies in the account home hold the three entries of the home, inherited (`SYSTEM`, `Administrators`, `svc-ignition-deploy`, all FullControl; run 2, "staged copies"). After `Move-Item -Force` into `production` without any further step the file held the same three entries as EXPLICIT entries with inheritance protected (run 1, `protected=True`, `E`). An untouched file of the collection (`config-mode.json`) and a file the same account makes in place by sftp carry the inherited list of the directory, which adds `BUILTIN\Users` ReadAndExecute and the two `APPLICATION PACKAGE` groups. So a moved file does not keep the collection's access list. **The ACL step is kept:** after each move into the collection (`commit` and `restore`) `commit.ps1` replaces the file's security descriptor with an empty, unprotected one (`[IO.File]::SetAccessControl` of a `FileSecurity` after `SetAccessRuleProtection($false, $false)`; the host's `icacls /reset` equivalent, which works on `\\?\` paths). Run 2 shows both moved files with exactly the control file's list afterwards, `protected=False`, all `I`. A failure of the step fails that entry (`write N: cannot reset the permissions of the file`), never a silent success. Not needed for the gateway to load the file on this host (next item); needed for any gateway whose service account is not `SYSTEM` or an administrator.
- **Does the gateway load a moved file after a scan.** Yes, in both runs: a scan right after the move took 0.95 s (gateway 423 ms), `resources/list/ignition/database-connection` lists the probe unit `SparkDB-oq5-01` with `collection: production` and `enabled: true`, and the gateway log since the scan holds two WARN+ lines, both `gateway.Database ... Error creating javax.sql.DataSource` (the unit is a sanitized fixture whose embedded password cannot be decrypted on this gateway: `Unable to decrypt ciphertext`), none says `will not be loaded`, none is a permission error. The load worked with the protected list of run 1 as well, so the gateway reads as `SYSTEM` or an administrator; `sc qc Ignition` is denied to the account ("OpenService FAILED 5") so the service identity itself was not read.
- **A target another process holds open.** Replacing a target held with `FileShare.Read` (what a JVM or a scanner usually uses: the file stays readable, nobody may write or delete it): the stale check reads it, the snapshot copies it, and the move fails after the bounded retries (3 retries, 1 s apart): the report is `phase: Writing`, `moved: []`, `failure: "write 0: cannot move the file into place"` after 5.7 s in all, the file is unchanged, and `restore` of that run succeeded while the file was still held (nothing to put back, the live file already equals the snapshot). A target held with `FileShare.None` cannot even be read to compare it with its pre-image: the commit stops BEFORE the journal's first move with the refusal `held` after the same retries (5.6 s), `transport_failed` naming the path and saying nothing was changed, no journal left. Neither case hangs. Before this was handled the unreadable target was reported as a stale plan (run 1 of the held step), which would have sent the user to re-plan; `Live` now tells an unreadable file (`?`) from a changed one.
- **Defender.** The account cannot read the Defender configuration (`Get-MpComputerStatus` and `Get-MpPreference` answer "Cannot connect to CIM server. Access denied"). The host record in `## Windows capture host (23-01)` has real-time protection on and no exclusions, and that is the state the timings below were taken in. The script commit of the 152-file corpus (`corpus_payload(1)`: 70 named units, 91,833 bytes, moved by one `exec_powershell`) took **6.92 s**; the prepare outside the lock took 86.0 s and the restore 18.03 s wall-clock including its uploads and journal read. The per-file commit this replaces took 234 to 348 s for the same 152-file tree on the earlier incarnation of this VM (22-LIVE-CAPTURES.md, OQ5-1; the VM was rebuilt on 2026-10-17, so the two figures are one tree on two builds of the same VM, not one host). A second run with an exclusion was not taken: adding one needs an administrator and this plan does not use the administrator credential, so "with and without the exclusion" is "as found" only.
- **The longest path sftp can reach below the account home (Pitfalls 7 and 8, C7a-8).** A file at a native path of 259 characters is listed by `ls` of its directory and served by `get` (exit 0); at 260, 261 and 270 the listing omits it and `get` fails (exit 1; the listing of the 270-character directory itself fails). The checked lengths were 200, 230, 240, 250, 255, 258, 259, 260, 261 and 270, with 80-character directory components. **The limit is 259 characters, the C7a-8 limit, measured here for files below the home.** `WINDOWS_SNAPSHOT_PATH_MAX = 259` in `sftp/apply_ops.rs`. The Windows `probe_paths` refuses, in Rust and before any write, a touched path whose snapshot (`snapshots\<run>\files\<path>`) or staging (`staging\<run>\files\<path>`) copy below the account home would be longer than that, naming the path, as a `write_host` preflight failure; the longer prefix, `snapshots`, decides.
- **Hold budget (for 23-10).** One script call moved 152 files in 6.92 s here; the call count is one per apply, the per-file cost is about 45 ms, of which the ACL reset is a part. 23-10 re-measures the p95 over 152 and 1,520 files for D-02.
- **Access lists of what the script creates, and links (review follow-up, run 3).** The journal, ledger, snapshots, record, staging and moved files below the account home carry only the home's entries (28 files and directories checked: no entry at all for `Users`, `Everyone` or `Authenticated Users`). Files moved into the real `production` collection end with the collection's own inherited list, whose only broad entry is `BUILTIN\Users` ReadAndExecute (asserted: no write-capable entry for a broad principal). A restore whose directory had become a junction failed with `restore 0: the live path is not a regular file` and changed nothing at the link's target; a commit whose new file would go through a junction stopped at the pre-image check and wrote nothing through it.

### 23-20

Rests on `## 23-20 PowerShell host ops`.

- **`probe` reads the access list, it does not write into the collection.** The gateway watches the collection, so a probe file there could be picked up. `CanAddFile` evaluates the directory's access rules against the account's own token (user and groups, an explicit deny wins, a rule that only inherits to children is skipped) for the create-file right. An explicit `CreateFiles` deny on `production` made the probe answer "the mode collection is not writable" and nothing changed. The `ign-deploy` side is written (a probe file created and removed): the deploy directories are ours. Limit: the right checked is create-file only; a collection where the account may create but not delete would pass the probe and fail the first replacing move (reported as `write N: cannot move the file into place` at phase `writing`, restorable).
- **`fingerprint` of a file another process holds without sharing is a refusal** (`held`, after the bounded retries, naming the path), not `None` and not a hash of something else. A directory where a file should be is `not_regular`. Read-only: the collection and the deploy directory are unchanged after both.
- **`mirror` moves through the commit path's `MoveOne`:** a file moved into the collection ends with inherited entries only (`protected False`, every entry `I`); a target held with `FileShare.Read` stops the move after the bounded retries with `write 0: cannot move the file into place` and nothing rolled back; the same commit works once the file is free; a staged copy that is a second name (hard link) for the live file does not lose the file (the move reports success and the bytes are unchanged), which differs from `sh`, where the inode comparison refuses it. Two staging modes (`Separate` at the Decision B(i) root, `InPlace` through a control directory) both ran.
- **A file made by a process of an ssh session is not a model of a gateway-made file:** it gets explicit entries (`(F)`, no `(I)`) from the process's default access list although the directory has inheritable entries. The conformance test therefore asserts that a moved file has only inherited entries, and does not compare it with a file the test made.
- **`fetch_snapshot` at the 259-character limit works** (a snapshot path of exactly `WINDOWS_SNAPSHOT_PATH_MAX`; hash equal to the pre-image) and `probe_paths` refuses the 260th; `read_record` of another run is `None`; `finalize` installs the final record, removes the journal and the run's staging and keeps the snapshot, and a restore of the finalized record still returns the pre-run listing; `discard_interrupted` drops a run whose journal says it never moved a file and refuses (`deploy_run_invalid`) a run that did.
- **`sftp -D <sftp-server.exe>` works on a Win32-OpenSSH host** (the first sftp command answered `Remote working directory: /C:/Users/...`), so the windows-latest conformance rows can use the runner's own `C:\Windows\System32\OpenSSH\sftp-server.exe` if the runner has it. Not verified for windows-latest itself.

## Decisions: OQ5b

Rests on `## OQ5b (batched commit, 23-10)` (two runs, 2026-10-17, host `ign-win-836`, Defender as found: real-time protection on, no exclusions). D-02 and D-03 read this section; `crates/ignition-core/src/deploy/apply/hold.rs` carries the literals and `hold_s_is_derived_from_recorded_measurements` recomputes the hold from them. Percentiles are nearest rank: with n = 10 the p95 is the maximum, with n = 5 it is the maximum, with n = 1 it is the one sample.

### OQ5b-1: p50 and p95 per scale (seconds, wall clock at the runner)

| Series | n | 152 files p50 | 152 files p95 | 1,520 files p50 | 1,520 files p95 |
|---|---|---|---|---|---|
| `commit`, new files plus the ledger | 10 | 8.266 | 8.731 | 60.291 | 62.040 |
| `replace_commit`, every file replaced (pre-images copied) | 5 | 10.045 | 10.123 | 83.409 | **85.496** |
| `commit_delete`, the whole payload deleted | 1 | 6.130 | 6.130 | 34.459 | 34.459 |
| `restore` after the new-file commit | 10 | 15.364 | 16.006 | 27.134 | 28.937 |
| `replace_restore` | 5 | 19.216 | 21.026 | 70.860 | 71.839 |
| `scan` after the new-file commit (wall) | 10 | 0.837 | 1.221 | 1.113 | 1.567 |
| `replace_scan` (wall) | 5 | 0.832 | 1.158 | 1.549 | **1.579** |
| `restore_scan` | 10 | 0.794 | 0.845 | 0.854 | 1.213 |
| `replace_restore_scan` | 5 | 0.914 | 1.185 | 1.480 | 1.542 |
| `scan_after_delete` | 1 | 0.679 | 0.679 | 1.098 | 1.098 |
| gateway `lastScanDuration` after the new-file commit (ms) | 10 | 572 | 705 | 814 | 846 |
| `prepare` of the new-file batch (outside the lock) | 1 | 81.537 | | 553.482 | |
| `prepare` of the replace batch (outside the lock) | 1 | 79.436 | | 533.198 | |

- Per file: the new-file commit costs 57 ms (152 files) and 41 ms (1,520); the replace commit 67 ms and 56 ms; the 2-file commit of 23-08 cost 2.7 to 3.1 s. The 22-14 per-file commit cost 1.6 to 2.3 s a file. At 152 files the p95 commit fell from 251.063 and 348.225 s to 8.731 s (replace: 10.123 s), 29 to 40 times; at 1,520 files the 22-14 extrapolation was 2,500 to 3,500 s and the measurement is 62.040 s (replace: 85.496 s).
- A scan is not the bottleneck: 1.6 s at most with 1,520 extra resources loaded (700 named units), 0.85 s at the gateway. The wall figures include the `ign api call` process start and the 200 ms poll.
- The restore is not proportional to the tree at small sizes: 15 to 21 s for 152 files and 27 to 72 s for 1,520. About 14 s of it is the fixed cost of the restore (journal read, upload of the work list and journal: several fresh ssh sessions), which the 2-file restore of 23-08 showed (14.6 s). The commit has no such floor (2.7 s for two files).
- Where the lock is held (read from `deploy/apply/mod.rs`): `prepare` first and outside it; then `HeldLock::acquire`, `commit`, one `lock.check` (a REST read), and the config scan, which releases the lock (D12). The post-write `fingerprint`, `deploy_verify`, `finalize` and `fetch_snapshot` come after the scan, so they are outside the hold (23-20: fingerprint, finalize 13.8 s, `fetch_snapshot` 24.7 s are single runs outside the window). The rollback path takes a lock for `restore` and its scan the same way.

### OQ5b-2: the hold

D-02: `hold = ceil(2 x (p95 commit + p95 scan) + 1)`, floor 10 s, ceiling 300 s, one constant, no per-run scaling, no per-target override.

- **Scale (FA-23-07):** the largest measured scale, 1,520 files, so a change up to that size fits.
- **Which commit (FA-23-08):** the plan names the new-file commit. The replace commit, which copies every pre-image into the snapshot first, is slower (85.496 s against 62.040 s at 1,520 files), and an apply of a changed tree is mostly replacements. The hold must cover the slowest call the lock is held across, so the constant uses the slowest commit variant measured, the replace commit: `P95_COMMIT_S_SCALE10 = 85.496`, `P95_COMMIT_S_SCALE1 = 10.123`. The scan figure is the largest p95 scan wall measured at that scale, the scan after a replace commit: `P95_SCAN_S = 1.579`.
- **Unclamped:** `ceil(2 x (85.496 + 1.579) + 1) = ceil(175.15) = 176` s. **Clamped** to [10, 300]: **176 s**. `HOLD_S = 176`.
- With the plan's literal choice (the new-file commit, 62.040 s, and its scan, 1.567 s) the hold would be `ceil(2 x (62.040 + 1.567) + 1) = 129` s. At 129 s a 1,520-file replace commit and its scan (85.496 + 1.579 = 87.1 s) would use 68% of the hold instead of 49%; the 2x margin D-02 asks for would be 1.5x. The difference is 47 s of lock window. To use the plan's literal, change the two `P95_COMMIT_S_*` literals to 8.731 and 62.040 and `P95_SCAN_S` to 1.567 in `hold.rs` and the same three numbers in the test; nothing else depends on the choice.
- **D-03a not triggered: unclamped hold 176 s, ceiling 300 s.** The blocker of Phase 22 (505 s and 699 s, per-file renames) does not apply to the batched commit: the previous VM build's 234 to 348 s for 152 files is now 8.7 to 10.1 s on the rebuilt host with the one-call script, and the old extrapolation (hold of roughly 5,000 to 7,000 s at 1,520 files) is now 176 s.
- A hold of 176 s is also long enough for the restore of the worst measured case (replace restore p95 71.839 s plus its scan 1.542 s, doubled plus one is 148 s) and for the restore at 152 files (21.026 s, which is above the 152-file replace commit of 10.123 s and below its hold).

### OQ5b-3: the per-file model behind the D-03 refusal

`HoldModel::MEASURED` fits the p95 commit through the two scales: `commit_per_file_s = (85.496 - 10.123) / (1520 - 152) = 0.055097...` and `commit_fixed_s = 10.123 - 152 x 0.055097... = 1.748...` (positive, so the floor at 0 does not apply). The model estimate at 1,520 files is 85.496 s, so `fits(1520)` holds (176 s needed, 176 s of hold). The count `touched_files` is `writes + deletes` (the ledger is not counted, as in the measurement).

- **Largest change that fits the hold: 1,527 files (writes plus deletes); 1,528 is the smallest that does not fit** (estimate 85.94 s, hold needed 177 s). The D-03 refusal names the estimate and the 176 s budget for such a change.
- The fit is exact at the two measured sizes and linear between and beyond them, which was not measured. A delete-only change costs less per file (34.459 s for 1,520 deletes), so the refusal is conservative for deletes. A change of 2 files is estimated at 1.9 s; it measured 2.7 to 3.1 s (23-08), still far inside the hold.

### OQ5b-4: timeouts (OQ5-4 revisited)

The slowest call made through the transport in these runs is `prepare` of the 1,520-file batch (553.482 s: one sftp upload, 0.364 s a file), which is under `DEFAULT_OP_TIMEOUT_S` (1050 s) and far under the 3600 s cap; the commit (85.496 s), the restore (71.839 s) and the delete commit (34.459 s) are well inside it. Nothing measured approaches either value, so **`DEFAULT_OP_TIMEOUT_S` and the 3600 s cap are unchanged.** A batch of more than about 2,900 files would need a `prepare` longer than 1050 s at this rate and sets `op_timeout_s` explicitly (the upload is outside the lock, so it does not affect the hold). The 22-14 note that the cap would bind at 1,520 files came from the per-file commit and no longer applies.

### OQ5b-5: what stays open

- The percentiles come from one run per scale on one WAN link and one VM build; the replace series has 5 samples, not 10 (FA-23-08). The 2x factor and the extra second of D-02 are the margin for that. A different link or Defender configuration moves the commit (22-14 saw +39% for an exclusion on the old path).
- Defender could not be read or changed by the account; the numbers are for the state the host record names.
- No Linux timing was taken; the sh host (23-06) has no measured hold. `HOLD_S` is one constant for every target by D-02.

## Live gate: rig apply and rollback (23-16)

Rig: `inductiveautomation/ignition:8.3.6` (b2026042713), project `ign-deploy-rig`, bind-mounted `config/resources`, `deploymentMode` production, gateway name `ign-deploy-rig`; started 2026-10-18T2026-10-18T03:52:03Z by `capture22.sh up22` then `setup22` after a `down -v` (the first start reused a stale `gw-data` volume and faulted at commissioning). A local node on the bind mount, the tracer's two units (`ignition/database-translator/POSTGRES` and `ignition/general-alarm-settings`, four files), a real `ReqwestGatewayApi` with token A for both `DeployApi` and `GatewayApi`, `.gwbk` on, `--designers fail`.

Steps the test takes: plan (record `plan_sha256`); `deploy_apply_run` (the production entry: factory host, `HoldModel::MEASURED`, the production scan wait); check outcome `applied`, the run directory, the `.gwbk` (a ZIP), the event sequence (`verify_done` with `ok: true`, `finalized`, no `restore_started`), the REST view of the mode collection; drop every handle and the repo's `.ign/`; rebuild reader, REST handle and host from the target file; `deploy_rollback_prepare_for_env` then `deploy_rollback_run` by run id; check outcome `restored`, a completed scan after the rollback (`lastScanTimestamp` advanced, `scanActive` false), the collection listing equal to the pre-apply listing, the REST view equal to its pre-apply view, every file and every directory below `<module>/<type>` as before (a `<module>/<type>` directory that an apply created is never removed by the host ops, by design, so it is not compared), no `will not be loaded` WARN line since the restore scan began (CV1 rule), and `deploy plan` equal to the first plan.

**First run (04:07:56Z) found a bug, fixed in `84dca10`:** the apply stopped at the scan-lock acquire with `unexpected HTTP 400 Bad Request ... Error 400: Timeouts must be positive.` The engine sent `acquireTimeout: 0` ("no wait"); the real gateway refuses a non-positive timeout, and never waits out a positive one (D1: a held lock is an immediate 409), so the engine now sends 1 (`ACQUIRE_WAIT_S`). Every offline gateway had accepted 0; the fake gateway now refuses it as the real one does (mutation-checked: with the constant at 0 `apply_happy_path_local` fails with that message). Every `ign deploy apply` and `ign deploy rollback` before this commit would have failed at the lock on a real gateway.

The plan says `changed 2` and not `added`: the effective view already holds both units from the `core` collection, so the apply writes the `production` copies (four files) over them and the rollback deletes those four.

`IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 IGNITION_LIVE_TOKEN=$IGN_TOKEN_CAP_A IGN_DEPLOY_RIG_DATA=$HOME/.cache/ign-deploy-rig/data IGN_DEPLOY_RIG_MODE=production cargo test -p ignition-core --test live_deploy_apply -- --ignored --nocapture --test-threads=1` @ 2026-10-18T04:17:53Z

~~~
running 1 test
test live_apply_then_rollback ... live_apply_then_rollback @ 2026-10-18T04:18:07Z
plan: added 0 changed 2 removed 0 plan_sha256 bc0b21e0b7281f4239af3aed513031c2c6eb8777144a2bb8e698c51de57ddef4
apply: outcome Applied run Some("20261018T041807Z-e4168e") written 4 deleted 0 gwbk Some("20261018T041807Z-e4168e.gwbk")
apply warning: probes: the manifest declares no `verify.tag_probes`; nothing was probed
gwbk: 3673753 bytes
apply events: run_started gwbk_downloaded staged lock_acquired committed lock_verdict scan_done verify_done finalized
rollback: outcome Restored run 20261018T041813Z-02b834 of 20261018T041807Z-e4168e restored 0 deleted 4
PASS live_apply_then_rollback in 13.3s (plan bc0b21e0b7281f4239af3aed513031c2c6eb8777144a2bb8e698c51de57ddef4)
ok

test result: ok. 1 passed (selected from the four-test run below)
~~~

## Live gate: rig faults (23-16)

Same rig, same repo, same guards as the section above. These tests drive the library entries with an explicit `HoldModel` (the verbs have one hold, 176 s, and no override), a `FaultyHost` around the `local:` apply host and the real `ReqwestGatewayApi`.

- **`live_kill_mid_write_then_recover`:** `ApplyFault::HangAfterMove(1, ..)` on a spawned task (hold 30 s); the task is aborted at the hang. The journal is on the host at phase `writing`; `deploy plan`, a new apply and a rollback of another run each refuse with `deploy_journal_present` naming the recovery command, and write nothing; the rollback of the journal's own run, from a fresh context, restores the pre-apply tree, removes the journal and ends with the plan equal to the start. Observation: **the recovery tried 0.1 s after the kill failed with `scan_lock_held`** (the dead apply still holds its scan-lock, `api-token:cap-a:...`); the lock read free 30.6 s after the kill, which is the dead apply's hold (30 s here, 176 s for the shipped hold), and the recovery then succeeded. This is the "wait for the dead apply's hold" residual the README states for an ssh drop, seen on a real gateway.
- **`live_kill_after_green_verify_then_recover` (the A9 window):** the run is aborted at `finalize`, after the scan and a green verify. The journal is at phase `written`, the REST view lists `POSTGRES` (the gateway loaded a good deploy), a new apply refuses, and the rollback of the journal's own run restores the pre-apply tree and the gateway drops the unit after the restore scan. The recovery at once (0.6 s) succeeded: the scan released the lock (D12), so there is no wait in this window.
- **`live_lock_lapse_restores`:** hold 10 s (`HOLD_FLOOR_S`); a hook after `commit` sleeps 13 s, so the lock lapses before the engine's check. What `### C2` predicted for (e): a lapsing lock does not trigger a scan, and a file written into the collection is not loaded. What happened inside the engine: `lastScanTimestamp` the same before and after the lapse (`1792297141956` both), the lock read free, the REST view of the mode held no translator while the four files sat on disk. Then the engine found the lock gone, restored the host and scanned once: outcome `rolled_back`, cause `lock_expired`, exit 6 verdict slug `deploy_rolled_back`; the restore scan's timestamp is later than the lapse's, the REST view holds none of the units, no `will not be loaded` WARN line since the restore scan (CV1 rule), and the plan is equal to the start. Prediction and result agree.

`IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 IGNITION_LIVE_TOKEN=$IGN_TOKEN_CAP_A IGN_DEPLOY_RIG_DATA=$HOME/.cache/ign-deploy-rig/data IGN_DEPLOY_RIG_MODE=production cargo test -p ignition-core --test live_deploy_apply -- --ignored --nocapture --test-threads=1` @ 2026-10-18T04:17:53Z

~~~
running 3 tests
test live_kill_after_green_verify_then_recover ... live_kill_after_green_verify_then_recover @ 2026-10-18T04:18:20Z
killed past the green verify, before finalize @ 2026-10-18T04:18:26Z
journal: run 20261018T041820Z-6a3346 phase written
REST translators in the mode while the journal stands: ["POSTGRES"]
recovery at once (0.6s after the kill): ok
PASS live_kill_after_green_verify_then_recover in 7.1s
ok
test live_kill_mid_write_then_recover ... live_kill_mid_write_then_recover @ 2026-10-18T04:18:27Z
killed after the first move @ 2026-10-18T04:18:28Z
journal: run 20261018T041827Z-4ad582 phase writing
recovery at once (0.1s after the kill) failed: scan_lock_held / the config scan-lock is held by api-token:cap-a:192.168.117.1
lock free 30.6s after the kill
PASS live_kill_mid_write_then_recover in 35.6s
ok
test live_lock_lapse_restores ... live_lock_lapse_restores @ 2026-10-18T04:19:03Z
C2e in the engine: scan timestamp across the lapse: 1792297141956 -> 1792297141956 (active false); lock free; REST translators in the mode: []
C2e in the engine: scan unchanged across the lapse: true
C2e in the engine: translators_visible: 0
apply: outcome RolledBack cause Some(LockExpired) ok false slug Some("deploy_rolled_back")
PASS live_lock_lapse_restores in 24.5s
ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 83.05s (the same run also ran live_apply_then_rollback)
~~~

## Spec §9 amendment draft (23-16)

Target: `docs/superpowers/specs/2026-10-09-prod-gateway-config-as-code-design.md` in `whk-environment-orchestration` (PR #60, branch `docs/prod-gateway-config-as-code`). The text below was drafted against the spec as it stands on that local branch (read with `git show`, nothing checked out or changed there). Owed by 22-LIVE-CAPTURES `### Decision B (recorded 2026-10-15 ...)`, caveat 1. Nothing here has been applied to the spec or pushed.

**Decision (Task 3, 2026-10-18): user chose: a PR for that repo (stacked on #60).** Landed as <https://github.com/WhiskeyHouse/whk-environment-orchestration/pull/63> (base `docs/prod-gateway-config-as-code`, head `docs/spec9-deploy-account-scope`, one commit `675e583` from a fresh clone; the user's local clone and PR #60's own branch were not touched). The PR text lists the three open points below. The line "Nothing here has been applied to the spec or pushed" above described the draft at the time it was written.

### What the spec says today (§9, "Pipeline identities")

> - **SSH account `svc-ignition-deploy` on each node.** Key-only auth, with write access limited to `data/config/resources/<mode>`. Its key is a GitHub environment secret.

### Replacement text for that bullet

> - **SSH account `svc-ignition-deploy` on each node.** Key-only auth. It is not an administrator and holds these write rights and no others:
>   1. **Modify on each mode collection** a deploy target names, `data/config/resources/<mode>` (granted on the directory, so the files it moves in are inherited into the collection's access list). Read and execute on `data/config/resources` and on `core` stay as provisioned. It holds no write right anywhere else under `data/`.
>   2. **Create, write and delete under `ign-deploy/<gateway>/<mode>/` in its own home directory** (Windows `C:\Users\svc-ignition-deploy\ign-deploy\<gateway>\<mode>\`, Linux `~/ign-deploy/<gateway>/<mode>/`). That tree is outside `data/` and outside every collection, so the gateway never reads it. It holds the deploy ledger (`ledger.json`), the in-flight journal (`journal.json`), the staged files of a run (`staging/<run_id>/`) and the snapshot and run record of each run (`snapshots/<run_id>/`, with `record.json`).
>
>   The home directory is readable by the account, `SYSTEM` and the administrators only (the Windows home grants nothing to `Users`, `Everyone` or `Authenticated Users`); nothing `ign` creates there grants anything either. The snapshots hold the pre-images of the files a run replaced or deleted, which can contain secret values, so that privacy is part of the control: the declared-resource lint (no embedded secrets) does not cover them. `ign` never prunes `ign-deploy/`; retention of runs and snapshots on the node is the operator's, and removing a run's snapshot makes `ign deploy rollback --run <id>` for that run impossible. (The 30 days of §8 are the CI run artifacts, not this tree.) Its key is a GitHub environment secret.

### Evidence pointers

| Claim in the replacement | Evidence |
|---|---|
| The account can create, write, rename and delete in its home, which has Full control for it; the home is outside `data/` and a collection | 22-LIVE-CAPTURES `### C6` row 1, blocks `C6-1`; scan state identical before and after (`C6-scan-before`, `C6-scan-after`) |
| The account cannot write the collection as provisioned and cannot create `data/ign-deploy/`; so Modify on the collection is a separate grant, and the home tree is the only other write place | `### C6` rows 2 and 3 (`C6-2`, `C6-3`); `C6-post-1` (after the grant of 2026-10-15, put, rename onto an existing file, mkdir, rmdir and rm work in `production`) |
| Why the home and not the data directory: Decision B(i) and the on-box layout | 22-LIVE-CAPTURES `### Decision B (recorded 2026-10-15 ...)`; `ign-deploy/<gateway>/<mode>/` for ledger, journal, `staging/<run_id>/`; `snapshots/<run_id>/` and `record.json` are 23-02's layout |
| Files moved into the collection need the account to own them (the access-list reset needs WRITE_DAC), and end with the collection's inherited list | 23-LIVE-CAPTURES `### C7b` (ACL of a moved file; the reset after each move; `BUILTIN\Users` ReadAndExecute is the only broad entry) |
| Nothing the account creates in its home grants `Users`, `Everyone` or `Authenticated Users` | 23-LIVE-CAPTURES `### C7b` (28 created or moved paths checked), `wflwvm01_script_acl_and_reparse` |
| The probe needs create-file on the collection and reads the access list instead of writing a probe file in it | 23-LIVE-CAPTURES `### 23-20` |
| Snapshots and the `.gwbk` hold secrets; everything else a run writes or prints does not | README `deploy apply`, "Where secrets can be, and where they cannot" (23-15, D-05); the canary tests `apply_canary`, `rollback_canary`, `canary_on_box_files_clean`, `canary_aborted_run_clean` |
| Nothing is pruned | 23-CONTEXT D-08; README "Runs are never pruned" |
| The whole flow runs under this account with these rights | `## Live gate: Windows binary apply and rollback (23-16)` below (if it has run) and `wflwvm01_apply_script_run`, `wflwvm01_ps_ops` |

### Open points for the spec author

- **Linux nodes are unproven.** The text says `~/ign-deploy/...` for Linux, but `commit.sh` has been run only under a local `dash`; the file modes it sets (0644 files, 0755 directories) and a staged-file move across file systems (a copy and delete, not an atomic rename) are uncaptured (FA-23-05). If the spec prefers to say nothing about Linux until it is captured, drop the Linux path in item 2.
- **Rollback is part of §8, not only §6.** §8's "Config or tags" row says "Restore the pre-deploy collection snapshot under scan-lock, then scan". `ign deploy rollback --run <id>` is that restore, from the node's own snapshot, and §8's "Automatic rollback fires only when post-apply verification fails" is what `ign deploy apply` does (exit 6, `deploy_rolled_back`). A follow-up edit to §8 may want to name the command; it needs no new account right.
- **The lock hold.** §6 step 4.3 describes `holdTimeout = 2x(write+scan)+1`, measured on the real transport. 23-10 measured it: one fixed hold of 176 s for every target, and a change of 1,528 or more files is refused. This is a §6 fact, not a §9 one; it is mentioned here only so the author can update both in one pass.

### Follow-up from 23-22 for PR #63 (not applied; the other repo was not touched)

The account-scope text of the spec §9 amendment (whk-environment-orchestration PR #63, stacked on #60) should add, for Linux ssh nodes: the deploy account needs **read and execute (list) on `data` and `data/config`** (not recursive, no write) in addition to read on `config/resources` and write on each mode collection and its own home. The read half lists every ancestor from the data directory down to `resources/<mode>` to refuse a symlink on the path, so a traverse-only account fails at `ign deploy plan` with `remote readdir(".../data/"): Permission denied` (seen on `ign-lnx-836`, 2026-10-19; see `### Grant of list rights on data and data/config`). This matches the Windows host, where Users has inherited read and execute on `data` and `data\config`.

## Live gate: Windows binary apply and rollback (23-16)

Host: `ign-win-836` (Windows Server 2022, Win32-OpenSSH, Ignition 8.3.6 `Ignition-ignwin836`, the real `production` collection, account `svc-ignition-deploy`), profile `azure-win`. The installed `ign` (`cargo install --path crates/ignition-cli --force` at the `84dca10` state, 2026-10-18T04:25Z) ran `deploy plan`, `deploy apply --yes --plan-sha256 <h>` and `deploy rollback --yes --run <id>` with `--json` against a temp repo holding the tracer's two units (four files), through `apply_host_for_node` (the Windows ssh arm), with the `.gwbk` on and `--designers fail`.

Trial: the gateway showed `expired: true`, `trialSecondsLeft` 0 at 2026-10-18T13:54:55Z; **reset at 2026-10-18T13:55:14Z** with `ign --profile azure-win --json api call --method POST --path /data/api/v1/trial` (HTTP 200, 7199 s left). The first Keychain read after the reinstall had blocked from 04:26Z until the user allowed it.

Results: apply `applied` in 166.0 s wall (4 files written, `.gwbk` 3,710,688 bytes, `snapshot/record.json` present, events run_started gwbk_downloaded staged lock_acquired committed lock_verdict scan_done verify_done finalized); **apply lock window (lock_acquired to scan_done) 3.808 s against HOLD_S 176 s**. Rollback by run id `restored` in 125.7 s wall (4 deleted), its lock window 13.232 s against 176 s. `deploy plan` after the rollback equals the first plan (`f7403f25...`), and, inside the test body, the production files equal the pre-run files exactly (one file, `config-mode.json`) with no directory left below `<module>/<type>`. Empty `<module>/<type>` directories may remain after a rollback by design; the test then removes any stray directory itself (`put_back`) before the final check, and the final listing of files and directories equals the pre-run listing (`true`). The test removed its `ign-deploy` tree below the account home. The closing scan completed (0.97 s).

Whole-apply wall time (166 s) is dominated by ssh handshakes (prepare, fingerprint, finalize, `fetch_snapshot` at about 14 to 25 s each on this link, 23-20) and a plan of 32.5 s (read of the scoped collection); the lock is held for 3.8 s of it.

`IGNITION_LIVE_WFLWVM01=1 IGN_WFLWVM01_HOST=... IGN_WFLWVM01_KEY=... IGN_WFLWVM01_PROFILE=azure-win cargo test -p ignition-core --test live_transport_wflwvm01 wflwvm01_cli_apply_rollback -- --ignored --nocapture --test-threads=1` @ 2026-10-18T13:55:52Z

~~~
running 1 test
test wflwvm01_cli_apply_rollback ... CAPTURE cli apply and rollback @ 2026-10-18T13:55:57Z
CAPTURE production before: 1 files, 0 directories: ["config-mode.json"]
CAPTURE ign deploy plan: exit 0 in 32.5s, summary {"added":0,"changed":2,"drifted":null,"has_changes":true,"removed":0,"same":0,"unmanaged":5}, plan_sha256 f7403f2574495451f1315c5f26ffc4688e21444b956e4322c18bc107ece36b14
CAPTURE ign deploy apply: exit 0 in 166.0s, outcome "applied", run "20261018T135706Z-bcfdef", written 4, deleted 0, gwbk "20261018T135706Z-bcfdef.gwbk", warnings ["probes: the manifest declares no `verify.tag_probes`; nothing was probed"]
CAPTURE apply run directory: .gwbk 3710688 bytes, snapshot/record.json present, events ["run_started", "gwbk_downloaded", "staged", "lock_acquired", "committed", "lock_verdict", "scan_done", "verify_done", "finalized"]
CAPTURE apply lock window (lock_acquired to scan_done): 3.808s against HOLD_S 176s
CAPTURE production after the apply: 5 files (was 1)
CAPTURE ign deploy rollback: exit 0 in 125.7s, outcome "restored", run "20261018T140017Z-b08526", restored 0, deleted 4, discarded []
CAPTURE rollback lock window (lock_acquired to scan_done): 13.232s against HOLD_S 176s; events ["run_started", "lock_acquired", "restore_started", "scan_done", "restored"]
CAPTURE plan after the rollback: exit 0, plan_sha256 f7403f2574495451f1315c5f26ffc4688e21444b956e4322c18bc107ece36b14
CAPTURE production listing equals the pre-run listing: true
CAPTURE closing scan: (0.972923291, 430)
CAPTURE cleanup: deploy dir gone true
TIMING wflwvm01_cli_apply_rollback total: 395.18s
PASS wflwvm01_cli_apply_rollback
ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 19 filtered out; finished in 395.22s
~~~

## Live gate: Linux ssh host (FA-23-05)

**Outcome: GAP.** The first gate on a real Linux host stopped on the first sftp listing. The ssh path itself (key file, pinned host key, hash-checking stub, account home by `sftp pwd`) worked, but every operation that lists a directory through sftp (`prepare`, `read`, so `ign deploy plan` and `ign deploy apply`) fails with `transport_failed` on this host, because the POSIX listing parser assumes the sftp server prefixes every long-listing name with the listed directory and this host's server does not. No commit reached the scratch collection (it was only seeded) and nothing was written to production. No engine or script source was changed (`git diff 9062706 -- crates/ignition-core/src crates/ignition-cli/src` is empty).

Host: `ign-lnx-836`, Azure, Ubuntu 22.04 (x86_64, glibc 2.35), Ignition 8.3.6 at `/usr/local/bin/ignition` (systemd `Ignition-Gateway.service`, java running as root, deployment mode `production`). `/bin/sh` is dash; `sha256sum` and `shasum` are present. Server side: OpenSSH `8.9p1 Ubuntu-3ubuntu0.18` with package `openssh-sftp-server 1:8.9p1-3ubuntu0.18` (`/usr/lib/openssh/sftp-server`). Account `svc-ignition-deploy` (uid 1001): key-only, no sudo, login shell bash, ssh session umask 0002, home `/home/svc-ignition-deploy`. POSIX ACLs: traverse on `data` and `data/config`, read (default-inherited) on `resources`, rwx (default-inherited) on `production`, which holds only `config-mode.json` (`-rw-rw-r--+ root root`). Home and production are on one filesystem, so a cross-filesystem `mv` is not exercised here and stays open. Runner: macOS, OpenSSH_10.3p1 client, profile `azure-lnx`, `IGNITION_CLI_CONFIG` pointing at an isolated config with the token in the environment (no Keychain access).

Trial: `GET /data/api/v1/trial` at 2026-10-18T23:44:18Z showed `licenseMode` Trial, `trialSecondsLeft` 4026, `expired` false; no reset was needed.

Install: `cargo install --path crates/ignition-cli --force` at the `c763478` source state (no `src` change since `9062706`), 2026-10-18T23:43Z. `ign --version` still reads 1.5.0.

Conformance build: `ubuntu:22.04` with `--platform linux/amd64` in a throw-away container (OrbStack on aarch64, Rosetta), its own `CARGO_TARGET_DIR=/out/target`, `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `git archive HEAD` of `9062706`, `--locked`; the binary is an ELF 64-bit x86-64 PIE, 7.1 MB stripped. Docker had enough disk this time, so the host-side build fallback was not used.

### Results per gate

| Gate | Result |
|---|---|
| `lnxvm_apply_script_run` | FAIL at the first `prepare` (sftp listing), 30.6 s. Session facts captured before it: umask `0002`, uid 1001. |
| `lnxvm_sh_commit_time_budget` | FAIL at the first `prepare`, same error, 19.2 s. |
| `lnxvm_conformance_table_on_host` | FAIL: 48 listed, 28 passed, 20 failed (the binary exited 101) as uid 1001, 4.5 s on the host. All 20 failures are the same listing error. |
| `lnxvm_cli_apply_rollback` (opt-in) | FAIL at `ign deploy plan` (exit 4, `transport_failed`), before any write. Production was put back (listing equal), the closing scan completed (0.56 s) and the deploy dir was gone. |

Scratch and conformance directories were removed on the host after every run (`ls -A` of the home shows only `.bash_logout .bashrc .cache .profile .ssh`; production holds only `config-mode.json`, dated 22:59 before any run).

### Cause

`SftpTransport::new` lists with `ListFormat::Prefixed` (`crates/ignition-core/src/deploy/transport/sftp.rs`, chosen at the `windows` switch near line 688; the parser is `parse_batch_listing_as`). It finds the listed directory in each `ls -la` line (`<dir>/<name>`). The OpenSSH 8.9p1 `sftp-server` prints bare names, as Win32-OpenSSH does, so the very first line (`.`) has no prefix. The server in use is the difference: macOS's `/usr/libexec/sftp-server` (used by every local test) prefixes names; this host's does not. Reproduced by hand with the pinned key (no secrets in the output):

~~~
sftp> pwd
Remote working directory: /home/svc-ignition-deploy
sftp> ls -la /home
drwxr-xr-x    4 root     root         4096 Oct 18 22:52 .
drwxr-xr-x   19 root     root         4096 Oct 18 22:33 ..
drwxr-x---    8 ignadmin ignadmin     4096 Oct 18 22:59 ignadmin
drwxr-x---    5 svc-ignition-deploy svc-ignition-deploy     4096 Oct 18 23:36 svc-ignition-deploy
~~~

The Windows `ListFormat::Marked` path (an `@pwd` line before each `@ls -la`) handles exactly this shape. Not fixed here (out of scope: no engine change). The conformance rows that passed on the host are the ones that never list through sftp: the 17 `conformance_local_*` rows, `conformance_sh_fingerprint_is_read_only`, `conformance_sh_probe_reports_writable_without_touching_the_collection`, and the non-row tests.

### Commands

`IGNITION_LIVE_LNXVM=1 IGN_LNXVM_HOST=... IGN_LNXVM_USER=... IGN_LNXVM_KEY_FILE=... IGN_LNXVM_KNOWN_HOSTS=... IGN_LNXVM_PROFILE=azure-lnx IGN_LNXVM_DATA_DIR=... cargo test -p ignition-core --test live_transport_lnxvm lnxvm_apply_script_run -- --ignored --nocapture --test-threads=1` @ 2026-10-18T23:36:51Z

~~~
running 1 test
test lnxvm_apply_script_run ... CAPTURE lnxvm_apply_script_run @ 2026-10-18T23:36:51Z
CAPTURE ssh session: umask "0002" uid 1001
thread 'lnxvm_apply_script_run' (284459449) panicked at crates/ignition-core/tests/live_transport_lnxvm.rs:789:53:
prepare: TransportFailed { operation: "prepare", node: "nodes.master", detail: "cannot use the sftp listing: a listing line does not carry a listed directory prefix: \"drwxr-xr-x    4 root     root         4096 Oct 18 22:52 .\"" }
FAILED
    lnxvm_apply_script_run
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 4 filtered out; finished in 30.59s
~~~

`... lnxvm_sh_commit_time_budget -- --ignored --nocapture --test-threads=1`

~~~
running 1 test
test lnxvm_sh_commit_time_budget ... 
thread 'lnxvm_sh_commit_time_budget' (284936837) panicked at crates/ignition-core/tests/live_transport_lnxvm.rs:966:53:
prepare: TransportFailed { operation: "prepare", node: "nodes.master", detail: "cannot use the sftp listing: a listing line does not carry a listed directory prefix: \"drwxr-xr-x    4 root     root         4096 Oct 18 22:52 .\"" }
FAILED
    lnxvm_sh_commit_time_budget
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 9 filtered out; finished in 19.17s
~~~

`... IGN_LNXVM_CONFORMANCE_BIN=... lnxvm_conformance_table_on_host -- --ignored --nocapture --test-threads=1` @ 2026-10-18T23:42:11Z

~~~
running 1 test
test lnxvm_conformance_table_on_host ... CAPTURE conformance table on the host @ 2026-10-18T23:42:11Z
CAPTURE cleanup: conformance dir gone true
CAPTURE conformance output (names and results only):
running 48 tests
test conformance_local_add_one_file ... ok
test conformance_local_delete_file_and_emptied_dirs ... ok
test conformance_local_directory_where_file_goes_is_stale ... ok
test conformance_local_discard_interrupted_touches_no_collection_file ... ok
test conformance_local_fetch_snapshot_copies_with_hashes ... ok
test conformance_local_finalize_removes_journal_and_staging_keeps_snapshot ... ok
test conformance_local_fingerprint_is_read_only ... ok
test conformance_local_gateway_owned_file_refused ... ok
test conformance_local_journal_present_refuses ... ok
test conformance_local_ledger_swap_and_restore ... ok
test conformance_local_pre_image_mismatch_aborts_before_any_move ... ok
test conformance_local_probe_reports_writable_without_touching_the_collection ... ok
test conformance_local_read_record_absent_is_none ... ok
test conformance_local_replace_file ... ok
test conformance_local_restore_twice_is_idempotent ... ok
test conformance_local_self_move_refused ... ok
test conformance_local_symlinked_target_dir_is_refused ... ok
test conformance_sh_add_one_file ... FAILED
test conformance_sh_delete_file_and_emptied_dirs ... FAILED
test conformance_sh_directory_where_file_goes_is_stale ... FAILED
test conformance_sh_discard_interrupted_touches_no_collection_file ... FAILED
test conformance_sh_fetch_snapshot_copies_with_hashes ... FAILED
test conformance_sh_finalize_removes_journal_and_staging_keeps_snapshot ... FAILED
test conformance_sh_fingerprint_is_read_only ... ok
test conformance_sh_gateway_owned_file_refused ... FAILED
test conformance_sh_journal_present_refuses ... FAILED
test conformance_sh_ledger_swap_and_restore ... FAILED
test conformance_sh_pre_image_mismatch_aborts_before_any_move ... FAILED
test conformance_sh_probe_reports_writable_without_touching_the_collection ... ok
test conformance_sh_read_record_absent_is_none ... FAILED
test conformance_sh_replace_file ... FAILED
test conformance_sh_restore_twice_is_idempotent ... FAILED
test conformance_sh_self_move_refused ... FAILED
test conformance_sh_symlinked_target_dir_is_refused ... FAILED
test every_row_runs_against_the_local_host ... ok
test every_row_runs_against_the_powershell_host ... ok
test every_row_runs_against_the_sh_host ... ok
test fail_move_at_leaves_a_recoverable_partial_tree ... ok
test fail_restore_at_reports_the_index ... ok
test faulty_host_fails_the_nth_call ... ok
test faulty_host_hangs_the_nth_call ... ok
test faulty_host_runs_a_hook_after_the_nth_call ... ok
test hang_after_move_can_be_aborted ... ok
test sh_commit_stops_when_its_time_budget_runs_out ... FAILED
test sh_failed_move_reports_index ... FAILED
test sh_moved_files_are_0644 ... FAILED
test stub_refuses_a_tampered_script ... FAILED
test stub_refuses_a_tampered_work_list ... FAILED
test result: FAILED. 28 passed; 20 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.43s
CAPTURE conformance: rc 101 uid 1001 listed 48 summary Some((false, 28, 20))
thread 'lnxvm_conformance_table_on_host' (284867221) panicked at crates/ignition-core/tests/live_transport_lnxvm.rs:1730:5:
assertion `left == right` failed: the test binary's exit code
  left: Some(101)
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 9 filtered out; finished in 35.62s
~~~

`... IGNITION_LIVE_LNXVM_PROD=1 lnxvm_cli_apply_rollback -- --ignored --nocapture --test-threads=1` @ 2026-10-18T23:44:27Z

~~~
running 1 test
test lnxvm_cli_apply_rollback ... CAPTURE cli apply and rollback @ 2026-10-18T23:44:27Z
CAPTURE production before: 1 files, 0 directories: ["config-mode.json"]
CAPTURE production listing equals the pre-run listing: true
CAPTURE closing scan: Ok((0.563081125, 127))
CAPTURE cleanup: deploy dir gone true
thread 'lnxvm_cli_apply_rollback' (285050627) panicked at crates/ignition-core/tests/live_transport_lnxvm.rs:1372:13:
the CLI body failed; production was put back: "plan: no plan_sha256 (exit 4): {\"error\":{\"code\":\"transport_failed\",\"endpoint\":null,\"hint\":\"check the node's host access in targets/<env>.yaml: for `local`, that data_dir exists and is readable; for `ssh`, the pinned host key, the key and reachability. Nothing was read as absent, so no plan was made\",\"message\":\"transport failed (read) on nodes.master: cannot use the sftp listing: a listing line does not carry a listed directory prefix: \\\"drwxr-x---    4 svc-ignition-deploy svc-ignition-deploy     4096 Oct 18 23:43 .\\\"\"},\"ok\":false,\"profile\":\"azure-lnx\"}"
FAILED
    lnxvm_cli_apply_rollback
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 9 filtered out; finished in 28.37s
~~~

### Rerun after the listing fix (2026-10-19)

Plan 23-22 (gap G1) fixed the POSIX listing parser (commit `c67e8bc`: every listing leads with `@pwd`, a name is read bare or after the listed directory's prefix). Three of the four gates now pass on the live host; the fourth, `lnxvm_cli_apply_rollback`, stops at `ign deploy plan` on a host fact (below) and is a **blocker, not a pass**. The failing record of 2026-10-18 above is left in place.

Install: `cargo install --path crates/ignition-cli --force` on 2026-10-19 between 00:41Z and 00:44Z at source state `9516ba7` (no `crates/*/src` change after `c67e8bc`); `ign --version` still reads 1.5.0.

Conformance build: `ubuntu:22.04` with `--platform linux/amd64` in a throw-away container (OrbStack on aarch64), its own `CARGO_TARGET_DIR=/out/target`, `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `git archive HEAD` of `9516ba7` (started 2026-10-19T00:39:16Z, finished before 00:45:39Z), `--locked`; the binary is an ELF 64-bit x86-64 PIE, 7.1 MB stripped. Docker had the disk; the host-side build fallback was not used.

Trial: `GET /data/api/v1/trial` at 00:44:18Z showed `trialSecondsLeft` 426, so a reset was due. `POST` at 00:44:18Z was refused with `auth_rejected` (HTTP 403) while the trial was still active; it expired at about 00:51:20Z (`expired` true, 0 s left at 00:51:26Z) and `POST` at 2026-10-19T00:51:30Z returned HTTP 200, `trialSecondsLeft` 7199. (Reset works only on an expired trial on this host.)

Power state: the VM was `VM running` when 23-22 started and was left running.

| Gate | Result |
|---|---|
| `lnxvm_listing_capture` | PASS, 4.1 s (read only; output under `## G1: POSIX sftp listing shapes`) |
| `lnxvm_apply_script_run` | PASS, 268.7 s: umask `0002`, uid 1001, prepare, commit through `commit.sh`, both tamper refusals, the journal refusal, restore, cleanup (output under `## G1: POSIX sftp listing shapes`) |
| `lnxvm_sh_commit_time_budget` | PASS, 118.5 s: a commit inside a 174 s budget completes; over a read-only directory with a 1 s budget it stops at the deadline (`deadline: true`) |
| `lnxvm_conformance_table_on_host` | PASS: `48 passed; 0 failed` as uid 1001 under the host's dash and `/usr/lib/openssh/sftp-server`, 26.1 s |
| `lnxvm_cli_apply_rollback` (opt-in) | **FAIL at `ign deploy plan` (exit 4, `transport_failed`)**, before any write: `remote readdir("/usr/local/bin/ignition/data/"): Permission denied`. Production was put back (listing equal), the closing scan ran (0.36 s) and the deploy dir was gone. Blocker, see below. |

After every gate the host was as found: `ls -A` of the home shows only `.bash_logout .bashrc .cache .profile .ssh`, and production holds only `config-mode.json` (140 bytes, 2026-10-18 22:59).

#### Blocker: the deploy account cannot list the gateway data directory

*Resolved the same day: see `### Grant of list rights on data and data/config, and the rerun of all five gates (2026-10-19)` below.*

`ign deploy plan` reads through `SftpTransport::read`, which walks from the data directory and lists each parent to find the next component (`config`, `resources`, the collection) so that a symlink anywhere on the path is refused. The host was provisioned least-privilege: `getfacl` as the account shows `user:svc-ignition-deploy:--x` (traverse only) on `data` (`drwxr-x---+ root root`, `other::---`) and on `data/config`, and `r-x` from `data/config/resources` down. A traverse-only account cannot `readdir` the data directory, so the walk's first listing is denied. This is not a listing-format defect and not a test-code defect: the listing fix worked (the failure moved from "does not carry a listed directory prefix" to a server permission error). It contradicts the read half's design (it needs read on every ancestor of `config/resources`), so it needs a decision: grant the account read on `data` and `data/config` (a host change that widens the account to the gateway data directory's listing, which holds more than the collections), or let the read half start at a configured `config/resources` and trust the ancestors (a design change that gives up the ancestor symlink check). Neither was done in 23-22.

Commands of the three gates not recorded under `## G1` (env sourced from the 0600 file, nothing echoed):

`IGNITION_LIVE_LNXVM=1 cargo test -p ignition-core --test live_transport_lnxvm lnxvm_sh_commit_time_budget -- --ignored --nocapture --test-threads=1` @ 2026-10-19T00:39:29Z

~~~
running 1 test
test lnxvm_sh_commit_time_budget ... CAPTURE commit inside a 174 s budget: BatchReport { phase: Written, stale: false, moved: [0], deleted: [], removed_dirs: [], failure: None, deadline: false }
TIMING commit over a read-only directory with a 1 s budget: 3.31s
CAPTURE commit over a read-only directory with a 1 s budget (2.11s wall): BatchReport { phase: Writing, stale: false, moved: [0], deleted: [], removed_dirs: [], failure: Some("write 1: the commit's time budget ran out"), deadline: true }
cleanup: scratch gone true, staging gone true
TIMING lnxvm_sh_commit_time_budget total: 118.48s
PASS lnxvm_sh_commit_time_budget
ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 118.49s
~~~

`IGNITION_LIVE_LNXVM=1 IGN_LNXVM_CONFORMANCE_BIN=<the built binary> cargo test -p ignition-core --test live_transport_lnxvm lnxvm_conformance_table_on_host -- --ignored --nocapture --test-threads=1` @ 2026-10-19T00:45:48Z

~~~
running 1 test
test lnxvm_conformance_table_on_host ... CAPTURE conformance table on the host @ 2026-10-19T00:45:56Z
TIMING conformance run on the host: 15.05s
CAPTURE cleanup: conformance dir gone true
CAPTURE conformance output (names and results only):

running 48 tests
test conformance_local_add_one_file ... ok
test conformance_local_delete_file_and_emptied_dirs ... ok
test conformance_local_directory_where_file_goes_is_stale ... ok
test conformance_local_discard_interrupted_touches_no_collection_file ... ok
test conformance_local_fetch_snapshot_copies_with_hashes ... ok
test conformance_local_finalize_removes_journal_and_staging_keeps_snapshot ... ok
test conformance_local_fingerprint_is_read_only ... ok
test conformance_local_gateway_owned_file_refused ... ok
test conformance_local_journal_present_refuses ... ok
test conformance_local_ledger_swap_and_restore ... ok
test conformance_local_pre_image_mismatch_aborts_before_any_move ... ok
test conformance_local_probe_reports_writable_without_touching_the_collection ... ok
test conformance_local_read_record_absent_is_none ... ok
test conformance_local_replace_file ... ok
test conformance_local_restore_twice_is_idempotent ... ok
test conformance_local_self_move_refused ... ok
test conformance_local_symlinked_target_dir_is_refused ... ok
test conformance_sh_add_one_file ... ok
test conformance_sh_delete_file_and_emptied_dirs ... ok
test conformance_sh_directory_where_file_goes_is_stale ... ok
test conformance_sh_discard_interrupted_touches_no_collection_file ... ok
test conformance_sh_fetch_snapshot_copies_with_hashes ... ok
test conformance_sh_finalize_removes_journal_and_staging_keeps_snapshot ... ok
test conformance_sh_fingerprint_is_read_only ... ok
test conformance_sh_gateway_owned_file_refused ... ok
test conformance_sh_journal_present_refuses ... ok
test conformance_sh_ledger_swap_and_restore ... ok
test conformance_sh_pre_image_mismatch_aborts_before_any_move ... ok
test conformance_sh_probe_reports_writable_without_touching_the_collection ... ok
test conformance_sh_read_record_absent_is_none ... ok
test conformance_sh_replace_file ... ok
test conformance_sh_restore_twice_is_idempotent ... ok
test conformance_sh_self_move_refused ... ok
test conformance_sh_symlinked_target_dir_is_refused ... ok
test every_row_runs_against_the_local_host ... ok
test every_row_runs_against_the_powershell_host ... ok
test every_row_runs_against_the_sh_host ... ok
test fail_move_at_leaves_a_recoverable_partial_tree ... ok
test fail_restore_at_reports_the_index ... ok
test faulty_host_fails_the_nth_call ... ok
test faulty_host_hangs_the_nth_call ... ok
test faulty_host_runs_a_hook_after_the_nth_call ... ok
test hang_after_move_can_be_aborted ... ok
test sh_commit_stops_when_its_time_budget_runs_out ... ok
test sh_failed_move_reports_index ... ok
test sh_moved_files_are_0644 ... ok
test stub_refuses_a_tampered_script ... ok
test stub_refuses_a_tampered_work_list ... ok

test result: ok. 48 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 13.93s


CAPTURE conformance: rc 0 uid 1001 listed 48 summary Some((true, 48, 0))
TIMING lnxvm_conformance_table_on_host total: 26.09s
PASS lnxvm_conformance_table_on_host
ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 26.09s
~~~

`IGNITION_LIVE_LNXVM=1 IGNITION_LIVE_LNXVM_PROD=1 cargo test -p ignition-core --test live_transport_lnxvm lnxvm_cli_apply_rollback -- --ignored --nocapture --test-threads=1` @ 2026-10-19T00:51:38Z

~~~
running 1 test
test lnxvm_cli_apply_rollback ... CAPTURE cli apply and rollback @ 2026-10-19T00:51:42Z
CAPTURE production before: 1 files, 0 directories: ["config-mode.json"]
CAPTURE production listing equals the pre-run listing: true
CAPTURE closing scan: Ok((0.355005333, 148))
CAPTURE cleanup: deploy dir gone true

thread 'lnxvm_cli_apply_rollback' (290192793) panicked at crates/ignition-core/tests/live_transport_lnxvm.rs:1409:13:
the CLI body failed; production was put back: "plan: no plan_sha256 (exit 4): {\"error\":{\"code\":\"transport_failed\",\"endpoint\":null,\"hint\":\"check the node's host access in targets/<env>.yaml: for `local`, that data_dir exists and is readable; for `ssh`, the pinned host key, the key and reachability. Nothing was read as absent, so no plan was made\",\"message\":\"transport failed (read) on nodes.master: exit code 1 (permission denied): remote readdir(\\\"/usr/local/bin/ignition/data/\\\"): Permission denied\"},\"ok\":false,\"profile\":\"azure-lnx\"}"
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
FAILED

failures:

failures:
    lnxvm_cli_apply_rollback

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 10 filtered out; finished in 15.68s

error: test failed, to rerun pass `-p ignition-core --test live_transport_lnxvm`
~~~

### Grant of list rights on data and data/config, and the rerun of all five gates (2026-10-19)

**The grant (applied by the user at their own prompt, not by the executor).** The deploy account was traverse-only on the data directory and `data/config`, so the read half's first listing was denied (the blocker above). The user ran, as `ignadmin`, before 01:30Z:

`sudo -n setfacl -m u:svc-ignition-deploy:r-x /usr/local/bin/ignition/data /usr/local/bin/ignition/data/config` (exit 0; non-recursive, no default ACL, nothing else changed).

Checked read-only as `svc-ignition-deploy` over ssh at 2026-10-19T01:30:08Z (`getfacl -p`; no secrets):

~~~
# file: /usr/local/bin/ignition/data
# owner: root
# group: root
user::rwx
user:svc-ignition-deploy:r-x
group::r-x
mask::r-x
other::---
# file: /usr/local/bin/ignition/data/config
# owner: root
# group: root
user::rwx
user:svc-ignition-deploy:r-x
group::r-x
mask::r-x
other::r-x
~~~

As the account: listing `data` shows 20 entries and `data/config` 4; `[ -w ]` is false on both (write still denied); `config/resources` lists (`core external local migration-log-202610182250.md production`) and `production/config-mode.json` reads. No `default:` entries on either directory. The provisioning script (`sh/30-svc-account.sh`, outside the repo) now sets `r-x` on the two directories with a comment on why: the read half lists every ancestor from `data` down to `resources/<mode>` for the symlink check; this matches the Windows host, where Users has inherited RX on `data` and `data\config`. The rest of the script is unchanged.

**Results.**

| Gate | Result |
|---|---|
| `lnxvm_cli_apply_rollback`, run alone @ 2026-10-19T01:30:42Z | PASS, 571.6 s (output below) |
| five-gate run #1 @ 2026-10-19T01:51:58Z | 4 of 5 PASS; `lnxvm_cli_apply_rollback` FAIL (transient, below) |
| five-gate run #2 @ 2026-10-19T02:22:49Z (one sequential invocation of all five) | **5 of 5 PASS**: `lnxvm_apply_script_run` 459.6 s, `lnxvm_cli_apply_rollback` 608.5 s, `lnxvm_conformance_table_on_host` 48 passed; 0 failed (29.3 s), `lnxvm_listing_capture`, `lnxvm_sh_commit_time_budget` 124.3 s; `test result: ok. 5 passed` |

Run #2 outputs of the CLI gate: `ign deploy plan` exit 0, `apply` outcome `applied` with the `.gwbk` present, `verify` ok, `rollback --run` outcome `restored`, the plan after the rollback equal to the first plan, the production listing equal to the pre-run listing, the closing scan ran and the deploy dir was gone.

**Honest record of the other runs.**

- Run #1 failed only `lnxvm_cli_apply_rollback`, at `ign deploy rollback` (exit 4) with `network_error` on `POST .../data/api/v1/scan/projects`: `gateway unreachable ... error sending request`, after 161.7 s. `plan`, `apply` (outcome `applied`) and `verify` had passed in the same test. The test put production back (listing equal, closing scan ran, deploy dir gone); the gateway answered a trial read minutes later and the host was clean. Not reproduced in the next two runs of the CLI gate; treated as a transient client/network error on the public http endpoint, not a transport defect.
- A third CLI-gate run was cut off by the executor's own 595 s tool timeout after `rollback` had returned `restored` (199.2 s) and before the test's own put-back. It left two empty directories under `production/ignition` and the run directory under the home's `ign-deploy`. Those were removed by hand as the account (`rmdir`, `rm` of exactly those paths) and the host checked clean before run #2. This run is not counted as a result.

Run #1, lines of the failed gate (the gateway address is replaced by `<gateway-ip>`):

~~~
PASS lnxvm_apply_script_run
CAPTURE ign deploy rollback: exit 4 in 161.7s, outcome null, run null, restored 0, deleted 0, discarded null
CAPTURE production listing equals the pre-run listing: true
CAPTURE closing scan: Ok((1.314015542, 89))
CAPTURE cleanup: deploy dir gone true
the CLI body failed; production was put back: "rollback: exit 4: {\"error\":{\"code\":\"network_error\",\"endpoint\":\"http://<gateway-ip>:8088/data/api/v1/scan/projects\",\"hint\":\"check the gateway is reachable at http://<gateway-ip>:8088/data/api/v1/scan/projects (host, port, VPN, TLS)\",\"message\":\"gateway unreachable at http://<gateway-ip>:8088/data/api/v1/scan/projects: error sending request for url (http://<gateway-ip>:8088/data/api/v1/scan/projects)\"},\"ok\":false,\"profile\":\"azure-lnx\"}"
test result: ok. 48 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 14.86s
PASS lnxvm_conformance_table_on_host
PASS lnxvm_sh_commit_time_budget
test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 6 filtered out; finished in 1080.52s
~~~

Gate `lnxvm_cli_apply_rollback` alone @ 2026-10-19T01:30:42Z:

~~~
running 1 test
test lnxvm_cli_apply_rollback ... CAPTURE cli apply and rollback @ 2026-10-19T01:30:42Z
CAPTURE production before: 1 files, 0 directories: ["config-mode.json"]
CAPTURE ign deploy plan: exit 0 in 39.1s, summary {"added":0,"changed":2,"drifted":null,"has_changes":true,"removed":0,"same":0,"unmanaged":5}, plan_sha256 7d4202e41ae166fb56a7e6d1a725b9d0c2dc3d46881fc5c20aec3153ccbf3d8c
CAPTURE ign deploy apply: exit 0 in 214.3s, outcome "applied", run "20261019T013158Z-c17b81", written 4, deleted 0, gwbk "20261019T013158Z-c17b81.gwbk", warnings ["probes: the manifest declares no `verify.tag_probes`; nothing was probed"]
CAPTURE apply run directory: .gwbk 3678873 bytes, snapshot/record.json present, events ["run_started", "gwbk_downloaded", "staged", "lock_acquired", "committed", "lock_verdict", "scan_done", "verify_done", "finalized"]
CAPTURE apply lock window (lock_acquired to scan_done): 2.007s against HOLD_S 176s
CAPTURE production after the apply: 5 files (was 1)
CAPTURE modes of the 4 files written: {"ignition/database-translator/POSTGRES/config.json": "644", "ignition/database-translator/POSTGRES/resource.json": "644", "ignition/general-alarm-settings/config.json": "644", "ignition/general-alarm-settings/resource.json": "644"}
CAPTURE modes of the 4 directories created: {"ignition": "775", "ignition/database-translator": "775", "ignition/database-translator/POSTGRES": "775", "ignition/general-alarm-settings": "775"}
CAPTURE getfacl of ignition: # file: /usr/local/bin/ignition/data/config/resources/production/ignition | # owner: svc-ignition-deploy | # group: svc-ignition-deploy | user::rwx | user:svc-ignition-deploy:rwx | group::r-x | mask::rwx | other::r-x | default:user::rwx | default:user:svc-ignition-deploy:rwx | default:group::r-x | default:mask::rwx | default:other::r-x |  | 
CAPTURE ign deploy verify: exit 0 in 47.1s, ok true, resources_checked 2, checks ["\"scan\"=\"pass\"", "\"mode\"=\"pass\"", "\"read_back\"=\"pass\"", "\"files\"=\"pass\"", "\"loaded\"=\"pass\"", "\"probes\"=\"warn\""]
CAPTURE ign deploy rollback: exit 0 in 185.8s, outcome "restored", run "20261019T013715Z-fbfbbd", restored 0, deleted 4, discarded []
CAPTURE rollback lock window (lock_acquired to scan_done): 24.123s against HOLD_S 176s; events ["run_started", "lock_acquired", "restore_started", "scan_done", "restored"]
CAPTURE plan after the rollback: exit 0, plan_sha256 7d4202e41ae166fb56a7e6d1a725b9d0c2dc3d46881fc5c20aec3153ccbf3d8c
CAPTURE production listing equals the pre-run listing: true
CAPTURE closing scan: Ok((0.486236291, 99))
CAPTURE cleanup: deploy dir gone true
TIMING lnxvm_cli_apply_rollback total: 571.63s
PASS lnxvm_cli_apply_rollback
ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 571.67s
~~~

Five-gate run #2 (`IGNITION_LIVE_LNXVM=1 IGNITION_LIVE_LNXVM_PROD=1 IGN_LNXVM_CONFORMANCE_BIN=<the binary built from 9516ba7> cargo test -p ignition-core --test live_transport_lnxvm lnxvm_ -- --ignored --nocapture --test-threads=1`, sources unchanged since `c67e8bc`):

~~~
running 5 tests
test lnxvm_apply_script_run ... CAPTURE lnxvm_apply_script_run @ 2026-10-19T02:22:49Z
CAPTURE ssh session: umask "0002" uid 1001
TIMING prepare (upload + verify): 41.89s
CAPTURE tampered work list: transport failed (exec) on nodes.master: commit: the staged script or work list does not match its hash; nothing ran
CAPTURE tampered script: transport failed (exec) on nodes.master: commit: the staged script or work list does not match its hash; nothing ran
TIMING commit (one ssh call): 1.51s
CAPTURE commit report: BatchReport { phase: Written, stale: false, moved: [0, 1], deleted: [0], removed_dirs: ["ignition/database-connection/Doomed"], failure: None, deadline: false }
CAPTURE modes after the commit: {"ignition": "775", "ignition/database-connection": "775", "ignition/database-connection/Café DB,1": "755", "ignition/database-connection/Café DB,1/config.json": "644", "ignition/database-connection/Existing": "775", "ignition/database-connection/ReplaceMe/config.json": "644"}
CAPTURE second commit over a journal: an interrupted deploy left the journal ~/ign-deploy/live-scratch/production/journal.json for run 20261018T120000Z-aaaaaa; recover it with: ign deploy rollback --run 20261018T120000Z-aaaaaa --env <env>
TIMING restore (one ssh call): 20.91s
CAPTURE restore report: BatchReport { phase: Restoring, stale: false, moved: [1, 2], deleted: [0, 3], removed_dirs: ["ignition/database-connection/Café DB,1"], failure: None, deadline: false }
cleanup: scratch gone true, staging gone true
TIMING lnxvm_apply_script_run total: 459.58s
PASS lnxvm_apply_script_run
ok
test lnxvm_cli_apply_rollback ... CAPTURE cli apply and rollback @ 2026-10-19T02:30:21Z
CAPTURE production before: 1 files, 0 directories: ["config-mode.json"]
CAPTURE ign deploy plan: exit 0 in 52.4s, summary {"added":0,"changed":2,"drifted":null,"has_changes":true,"removed":0,"same":0,"unmanaged":5}, plan_sha256 7d4202e41ae166fb56a7e6d1a725b9d0c2dc3d46881fc5c20aec3153ccbf3d8c
CAPTURE ign deploy apply: exit 0 in 295.2s, outcome "applied", run "20261019T023207Z-a7b083", written 4, deleted 0, gwbk "20261019T023207Z-a7b083.gwbk", warnings ["probes: the manifest declares no `verify.tag_probes`; nothing was probed"]
CAPTURE apply run directory: .gwbk 3678876 bytes, snapshot/record.json present, events ["run_started", "gwbk_downloaded", "staged", "lock_acquired", "committed", "lock_verdict", "scan_done", "verify_done", "finalized"]
CAPTURE apply lock window (lock_acquired to scan_done): 2.580s against HOLD_S 176s
CAPTURE production after the apply: 5 files (was 1)
CAPTURE modes of the 4 files written: {"ignition/database-translator/POSTGRES/config.json": "644", "ignition/database-translator/POSTGRES/resource.json": "644", "ignition/general-alarm-settings/config.json": "644", "ignition/general-alarm-settings/resource.json": "644"}
CAPTURE modes of the 4 directories created: {"ignition": "775", "ignition/database-translator": "775", "ignition/database-translator/POSTGRES": "775", "ignition/general-alarm-settings": "775"}
CAPTURE getfacl of ignition: # file: /usr/local/bin/ignition/data/config/resources/production/ignition | # owner: svc-ignition-deploy | # group: svc-ignition-deploy | user::rwx | user:svc-ignition-deploy:rwx | group::r-x | mask::rwx | other::r-x | default:user::rwx | default:user:svc-ignition-deploy:rwx | default:group::r-x | default:mask::rwx | default:other::r-x |  | 
CAPTURE ign deploy verify: exit 0 in 42.0s, ok true, resources_checked 2, checks ["\"scan\"=\"pass\"", "\"mode\"=\"pass\"", "\"read_back\"=\"pass\"", "\"files\"=\"pass\"", "\"loaded\"=\"pass\"", "\"probes\"=\"warn\""]
CAPTURE ign deploy rollback: exit 0 in 134.1s, outcome "restored", run "20261019T023808Z-990fba", restored 0, deleted 4, discarded []
CAPTURE rollback lock window (lock_acquired to scan_done): 15.392s against HOLD_S 176s; events ["run_started", "lock_acquired", "restore_started", "scan_done", "restored"]
CAPTURE plan after the rollback: exit 0, plan_sha256 7d4202e41ae166fb56a7e6d1a725b9d0c2dc3d46881fc5c20aec3153ccbf3d8c
CAPTURE production listing equals the pre-run listing: true
CAPTURE closing scan: Ok((0.420304166, 85))
CAPTURE cleanup: deploy dir gone true
TIMING lnxvm_cli_apply_rollback total: 608.53s
PASS lnxvm_cli_apply_rollback
ok
test lnxvm_conformance_table_on_host ... CAPTURE conformance table on the host @ 2026-10-19T02:40:34Z
TIMING conformance run on the host: 15.29s
CAPTURE cleanup: conformance dir gone true
CAPTURE conformance output (names and results only):

running 48 tests
test conformance_local_add_one_file ... ok
test conformance_local_delete_file_and_emptied_dirs ... ok
test conformance_local_directory_where_file_goes_is_stale ... ok
test conformance_local_discard_interrupted_touches_no_collection_file ... ok
test conformance_local_fetch_snapshot_copies_with_hashes ... ok
test conformance_local_finalize_removes_journal_and_staging_keeps_snapshot ... ok
test conformance_local_fingerprint_is_read_only ... ok
test conformance_local_gateway_owned_file_refused ... ok
test conformance_local_journal_present_refuses ... ok
test conformance_local_ledger_swap_and_restore ... ok
test conformance_local_pre_image_mismatch_aborts_before_any_move ... ok
test conformance_local_probe_reports_writable_without_touching_the_collection ... ok
test conformance_local_read_record_absent_is_none ... ok
test conformance_local_replace_file ... ok
test conformance_local_restore_twice_is_idempotent ... ok
test conformance_local_self_move_refused ... ok
test conformance_local_symlinked_target_dir_is_refused ... ok
test conformance_sh_add_one_file ... ok
test conformance_sh_delete_file_and_emptied_dirs ... ok
test conformance_sh_directory_where_file_goes_is_stale ... ok
test conformance_sh_discard_interrupted_touches_no_collection_file ... ok
test conformance_sh_fetch_snapshot_copies_with_hashes ... ok
test conformance_sh_finalize_removes_journal_and_staging_keeps_snapshot ... ok
test conformance_sh_fingerprint_is_read_only ... ok
test conformance_sh_gateway_owned_file_refused ... ok
test conformance_sh_journal_present_refuses ... ok
test conformance_sh_ledger_swap_and_restore ... ok
test conformance_sh_pre_image_mismatch_aborts_before_any_move ... ok
test conformance_sh_probe_reports_writable_without_touching_the_collection ... ok
test conformance_sh_read_record_absent_is_none ... ok
test conformance_sh_replace_file ... ok
test conformance_sh_restore_twice_is_idempotent ... ok
test conformance_sh_self_move_refused ... ok
test conformance_sh_symlinked_target_dir_is_refused ... ok
test every_row_runs_against_the_local_host ... ok
test every_row_runs_against_the_powershell_host ... ok
test every_row_runs_against_the_sh_host ... ok
test fail_move_at_leaves_a_recoverable_partial_tree ... ok
test fail_restore_at_reports_the_index ... ok
test faulty_host_fails_the_nth_call ... ok
test faulty_host_hangs_the_nth_call ... ok
test faulty_host_runs_a_hook_after_the_nth_call ... ok
test hang_after_move_can_be_aborted ... ok
test sh_commit_stops_when_its_time_budget_runs_out ... ok
test sh_failed_move_reports_index ... ok
test sh_moved_files_are_0644 ... ok
test stub_refuses_a_tampered_script ... ok
test stub_refuses_a_tampered_work_list ... ok

test result: ok. 48 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 14.00s


CAPTURE conformance: rc 0 uid 1001 listed 48 summary Some((true, 48, 0))
TIMING lnxvm_conformance_table_on_host total: 29.26s
PASS lnxvm_conformance_table_on_host
ok
test lnxvm_listing_capture ... CAPTURE lnxvm_listing_capture @ 2026-10-19T02:40:58Z
CAPTURE batch: pwd, ls -la <home>, pwd, ls -la <production>
CAPTURE stdout:
Remote working directory: /home/svc-ignition-deploy
drwxr-x---    4 svc-ignition-deploy svc-ignition-deploy     4096 Oct 19 02:40 .
drwxr-xr-x    4 root     root         4096 Oct 18 22:52 ..
-rw-r--r--    1 svc-ignition-deploy svc-ignition-deploy      220 Jan  6  2022 .bash_logout
-rw-r--r--    1 svc-ignition-deploy svc-ignition-deploy     3771 Jan  6  2022 .bashrc
drwx------    2 svc-ignition-deploy svc-ignition-deploy     4096 Oct 18 22:52 .cache
-rw-r--r--    1 svc-ignition-deploy svc-ignition-deploy      807 Jan  6  2022 .profile
drwx------    2 svc-ignition-deploy svc-ignition-deploy     4096 Oct 18 22:52 .ssh
Remote working directory: /home/svc-ignition-deploy
drwxrwxr-x    2 root     root         4096 Oct 19 02:40 .
drwxr-xr-x    7 root     root         4096 Oct 18 22:50 ..
-rw-rw-r--    1 root     root          140 Oct 18 22:59 config-mode.json

ok
test lnxvm_sh_commit_time_budget ... CAPTURE commit inside a 174 s budget: BatchReport { phase: Written, stale: false, moved: [0], deleted: [], removed_dirs: [], failure: None, deadline: false }
TIMING commit over a read-only directory with a 1 s budget: 3.65s
CAPTURE commit over a read-only directory with a 1 s budget (2.40s wall): BatchReport { phase: Writing, stale: false, moved: [0], deleted: [], removed_dirs: [], failure: Some("write 1: the commit's time budget ran out"), deadline: true }
cleanup: scratch gone true, staging gone true
TIMING lnxvm_sh_commit_time_budget total: 124.25s
PASS lnxvm_sh_commit_time_budget
ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 1226.09s

ALL5-EXIT 0
~~~

Host after run #2: `ls -A` of the home shows only `.bash_logout .bashrc .cache .profile .ssh`; production holds only `config-mode.json` (22:59). Trial: 4865 s left at 01:30:24Z, 1736 s at 02:22:34Z; no reset needed. VM power state found `VM running`; left running.

### FA-23-05 status

NOT closed. Proven live on this host: the ssh transport itself (key file with `IdentitiesOnly`, the pinned `known_hosts` with `StrictHostKeyChecking=yes`, the home by `sftp pwd`, `exec_posix` returning the `IGN-RESULT` line, the account's session umask 0002 and uid 1001), and the parts of the conformance table that do not list through sftp (28 of 48 under the host's dash, GNU coreutils and `/usr/lib/openssh/sftp-server`). Blocked, so unproven: `commit.sh` over ssh against a real collection, the 0644/0755 modes under umask 0002, the WR-04 deadline over ssh, the hash-stub tamper refusal over ssh, and `ign deploy plan`, `apply`, `verify` and `rollback --run` against the real `production` collection. Still open regardless: a cross-filesystem `mv`. Rerun all four gates after the listing fix with the same commands (the conformance binary must be rebuilt from the fixed tree). The README's FA-23-05 sentence is stale in either case (it says the Linux host is unproven) and was not edited here.

**Update 2026-10-19 (23-22).** After the listing fix, three of the four gates pass on `ign-lnx-836`, plus the new read-only capture gate. Proven live: `commit.sh` over ssh against a real (scratch) collection on the host's own file system (`lnxvm_apply_script_run`: prepare, commit, both tamper refusals, the journal refusal, restore, cleanup), the 0644 file and 0755 directory modes the commit leaves under the session umask of 0002, the WR-04 time budget stopping a commit over ssh (`lnxvm_sh_commit_time_budget`), the hash stub's tamper refusal over ssh, and the conformance table 48 of 48 on the host (dash, GNU coreutils, `/usr/lib/openssh/sftp-server`). **Still unproven:** `ign deploy plan`, `apply`, `verify` and `rollback --run` against the real `production` collection (`lnxvm_cli_apply_rollback` is blocked: the account has traverse-only access to the gateway data directory and cannot list it; see the blocker above). Also still open: a move across file systems, because the home and production share one file system on this host.

**Update 2026-10-19 (23-22, after the data and data/config read grant).** All five lnxvm gates pass in one sequential run on `ign-lnx-836`. Proven live, in addition to the paragraph above: `ign deploy plan`, `apply` (outcome `applied`, `.gwbk` present), `verify` and `rollback --run` (outcome `restored`) against the real `production` collection over Linux ssh, with the plan after the rollback equal to the first plan and the collection restored. This needs the deploy account to have read and execute (list) on `data` and `data/config` (granted by the user, non-recursive, write still denied). **Still open:** a move of staged files across file systems, because the home and production share one file system on this host.

## G1: POSIX sftp listing shapes

Added by 23-22 (gap G1, UAT test 2). The quick task 261018-nr2 run of FA-23-05 failed on `ign-lnx-836` because the POSIX listing parser found a line's directory only by a `<dir>/<name>` prefix. The sftp CLIENT prints that prefix only when it formats the line itself (openssh-portable `sftp.c` `do_ls_dir`: with `-n`/`-h`, or when the server offers `users-groups-by-id@openssh.com`); otherwise it prints the server's `longname`, a bare name. The macOS `sftp-server` offers the extension, Ubuntu 22.04's OpenSSH 8.9p1 `sftp-server` does not. Both shapes below are what the parser's regression tests hold verbatim (`posix_listing_parses_captured_openssh_8_9_output`, `posix_listing_parses_captured_client_formatted_output`). Every batch leads each `@ls -la` with an `@pwd` marker, so a line is attributed by marker and the name is read either after the listed directory's prefix or bare.

### Shape 1: OpenSSH 8.9p1 `sftp-server` on ign-lnx-836 (bare names)

`IGNITION_LIVE_LNXVM=1 cargo test -p ignition-core --test live_transport_lnxvm lnxvm_listing_capture -- --ignored --nocapture --test-threads=1` @ 2026-10-19T00:27:25Z. The gate is read only: one batch of `@pwd`, `@ls -la` of the account home, `@pwd`, `@ls -la` of `<data dir>/config/resources/production`.

~~~
running 1 test
test lnxvm_listing_capture ... CAPTURE lnxvm_listing_capture @ 2026-10-19T00:27:25Z
CAPTURE batch: pwd, ls -la <home>, pwd, ls -la <production>
CAPTURE stdout:
Remote working directory: /home/svc-ignition-deploy
drwxr-x---    4 svc-ignition-deploy svc-ignition-deploy     4096 Oct 18 23:43 .
drwxr-xr-x    4 root     root         4096 Oct 18 22:52 ..
-rw-r--r--    1 svc-ignition-deploy svc-ignition-deploy      220 Jan  6  2022 .bash_logout
-rw-r--r--    1 svc-ignition-deploy svc-ignition-deploy     3771 Jan  6  2022 .bashrc
drwx------    2 svc-ignition-deploy svc-ignition-deploy     4096 Oct 18 22:52 .cache
-rw-r--r--    1 svc-ignition-deploy svc-ignition-deploy      807 Jan  6  2022 .profile
drwx------    2 svc-ignition-deploy svc-ignition-deploy     4096 Oct 18 22:52 .ssh
Remote working directory: /home/svc-ignition-deploy
drwxrwxr-x    2 root     root         4096 Oct 18 22:59 .
drwxr-xr-x    7 root     root         4096 Oct 18 22:50 ..
-rw-rw-r--    1 root     root          140 Oct 18 22:59 config-mode.json

ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 4.11s
~~~

### Shape 2: macOS `/usr/libexec/sftp-server` through `sftp -D` (client-formatted, prefixed)

`printf '@pwd\n@ls -la "<scratch>"\n@pwd\n@ls -la "<scratch>/sub"\n' | sftp -q -b - -D /usr/libexec/sftp-server` run from a scratch directory (one file `file.txt`, one sub directory `sub` holding `inner.json`), macOS 26.6.1, `ssh -V`: OpenSSH_10.3p1, LibreSSL 3.3.6, @ 2026-10-19T00:27:41Z.

~~~
Remote working directory: /private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/7dbe57f3-a4c4-4f47-88c4-a2249a4d2b80/scratchpad/g1mac
drwxr-xr-x    ? pmannion wheel         128 Oct 18 18:27 /private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/7dbe57f3-a4c4-4f47-88c4-a2249a4d2b80/scratchpad/g1mac/scratch/.
drwxr-xr-x    ? pmannion wheel          96 Oct 18 18:27 /private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/7dbe57f3-a4c4-4f47-88c4-a2249a4d2b80/scratchpad/g1mac/scratch/..
-rw-r--r--    ? pmannion wheel           6 Oct 18 18:27 /private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/7dbe57f3-a4c4-4f47-88c4-a2249a4d2b80/scratchpad/g1mac/scratch/file.txt
drwxr-xr-x    ? pmannion wheel          96 Oct 18 18:27 /private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/7dbe57f3-a4c4-4f47-88c4-a2249a4d2b80/scratchpad/g1mac/scratch/sub
Remote working directory: /private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/7dbe57f3-a4c4-4f47-88c4-a2249a4d2b80/scratchpad/g1mac
drwxr-xr-x    ? pmannion wheel          96 Oct 18 18:27 /private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/7dbe57f3-a4c4-4f47-88c4-a2249a4d2b80/scratchpad/g1mac/scratch/sub/.
drwxr-xr-x    ? pmannion wheel         128 Oct 18 18:27 /private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/7dbe57f3-a4c4-4f47-88c4-a2249a4d2b80/scratchpad/g1mac/scratch/sub/..
-rw-r--r--    ? pmannion wheel           1 Oct 18 18:27 /private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/7dbe57f3-a4c4-4f47-88c4-a2249a4d2b80/scratchpad/g1mac/scratch/sub/inner.json
~~~

### The first Linux gate after the fix

`IGNITION_LIVE_LNXVM=1 cargo test -p ignition-core --test live_transport_lnxvm lnxvm_apply_script_run -- --ignored --nocapture --test-threads=1` @ 2026-10-19T00:33:12Z (the tracer of 23-22: `prepare`, a commit through `commit.sh`, both tamper refusals, the journal refusal, restore, cleanup; none of its steps after `prepare` had run before).

~~~
running 1 test
test lnxvm_apply_script_run ... CAPTURE lnxvm_apply_script_run @ 2026-10-19T00:33:12Z
CAPTURE ssh session: umask "0002" uid 1001
TIMING prepare (upload + verify): 24.65s
CAPTURE tampered work list: transport failed (exec) on nodes.master: commit: the staged script or work list does not match its hash; nothing ran
CAPTURE tampered script: transport failed (exec) on nodes.master: commit: the staged script or work list does not match its hash; nothing ran
TIMING commit (one ssh call): 1.20s
CAPTURE commit report: BatchReport { phase: Written, stale: false, moved: [0, 1], deleted: [0], removed_dirs: ["ignition/database-connection/Doomed"], failure: None, deadline: false }
CAPTURE modes after the commit: {"ignition": "775", "ignition/database-connection": "775", "ignition/database-connection/Café DB,1": "755", "ignition/database-connection/Café DB,1/config.json": "644", "ignition/database-connection/Existing": "775", "ignition/database-connection/ReplaceMe/config.json": "644"}
CAPTURE second commit over a journal: an interrupted deploy left the journal ~/ign-deploy/live-scratch/production/journal.json for run 20261018T120000Z-aaaaaa; recover it with: ign deploy rollback --run 20261018T120000Z-aaaaaa --env <env>
TIMING restore (one ssh call): 14.85s
CAPTURE restore report: BatchReport { phase: Restoring, stale: false, moved: [1, 2], deleted: [0, 3], removed_dirs: ["ignition/database-connection/Café DB,1"], failure: None, deadline: false }
cleanup: scratch gone true, staging gone true
TIMING lnxvm_apply_script_run total: 268.70s
PASS lnxvm_apply_script_run
ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 268.70s
~~~

## G2: 8.3 short names (windows-latest)

CI run 39175065312, job `check` (windows-latest): 18 of 47 `deploy_host_conformance` tests fail (every `conformance_ps_*` row and the `ps_*` tests), each with the same `prepare` error:

~~~
TransportFailed { operation: "prepare", node: "nodes.test", detail: "exit code 1 (refused with no reason: a held or non-empty target): remote mkdir \"/C:/Users/RUNNER~1\": Failure" }
~~~

Hypothesis (23-23 Task 1): `put_tree` surveys the staging root from the drive root (`/C:`) down. The runner's temp dir is spelled with the 8.3 short name `RUNNER~1`; the listing of `/C:/Users` shows only the long name, so the survey records the short-named component as missing and `make_dirs` asks the server to create a directory that exists.

### Host state

ign-win-836 was found `VM deallocating` at 2026-10-19T02:59Z (the 0300 UTC auto-shutdown; the public IP had changed from the stale `~/.config/ign-azure-win/env` value to the one in `~/.cache/ign-wflwvm01/host`, which is the pinned address in `known_hosts`). Started with `az vm start` at 2026-10-19T03:07:12Z, running at 03:09:40Z. ssh as `svc-ignition-deploy` with the cached key and pinned host key answers. The NSG source prefix for port 22 already matched the operator's /32.

### Root cause, read only plus one refused `mkdir`

`ssh svc-ignition-deploy@<host> 'for %I in ("%USERPROFILE%") do @echo %~sI'` @ 2026-10-19T03:12:52Z

~~~
C:\Users\SVC-IG~1
~~~

`ssh ... 'dir /x C:\Users'` @ 2026-10-19T03:14:33Z (the account has an 8.3 short name; no SYSTEM fallback needed)

~~~
10/18/2026  06:52 PM    <DIR>          SVC-IG~1     svc-ignition-deploy
~~~

One sftp batch (`@pwd`, `@ls -la "/C:/Users"`) @ 2026-10-19T03:14:35Z: the listing names `svc-ignition-deploy` and has no `SVC-IG~1` entry.

~~~
Remote working directory: /C:/Users/svc-ignition-deploy
lrwx******    1 -        -               0 May  8  2021 All Users
dr-x******    1 -        -            8192 Oct 13 06:00 Default
drwx******    1 -        -               0 May  8  2021 Default User
dr-x******    1 -        -            4096 Oct 13 06:01 Public
-rw-******    1 -        -             174 May  8  2021 desktop.ini
drwx******    1 -        -            8192 Oct 18 18:52 svc-ignition-deploy
~~~

sftp `mkdir "/C:/Users/SVC-IG~1"` @ 2026-10-19T03:14:39Z (the directory exists under that spelling; the server refuses):

~~~
sftp> mkdir "/C:/Users/SVC-IG~1"
remote mkdir "/C:/Users/SVC-IG~1": Failure
~~~

This is the shape CI printed for `RUNNER~1`: a short-spelled component that the listing does not show, and a `mkdir` of it answering `Failure`.

### Engine-level confirmation, before the fix

`source` the env (host from `~/.cache/ign-wflwvm01/host`, key from the cached key file; nothing printed), then `IGNITION_LIVE_WFLWVM01=1 cargo test -p ignition-core --test live_transport_wflwvm01 wflwvm01_short_name_home_prepare -- --ignored --nocapture --test-threads=1` on HEAD 14d0b01 plus the new gate @ 2026-10-19T03:17:28Z. The gate builds the scratch gateway below the long home, then runs `prepare` with the account home and data dir spelled `C:\Users\SVC-IG~1`.

~~~
CAPTURE short_name_home_prepare @ 2026-10-19T03:17:28Z
CAPTURE home: long C:\Users\svc-ignition-deploy short C:\Users\SVC-IG~1
CAPTURE prepare under the short home FAILED: transport failed (prepare) on nodes.master: exit code 1 (refused with no reason: a held or non-empty target): remote mkdir "/C:/Users/SVC-IG~1": Failure
cleanup: scratch gone true, staging gone true
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 21 filtered out; finished in 41.03s
~~~

The error text is the CI text with `SVC-IG~1` for `RUNNER~1`. Root cause confirmed: `put_tree` surveys from `/C:`, the listing of `/C:/Users` has only the long name, `make_dirs` issues `mkdir /C:/Users/SVC-IG~1`, and the server refuses it. The "held or non-empty target" label comes from `require_success` naming every sftp `Failure` that way.

### Engine-level confirmation, after the fix

Same command @ 2026-10-19T03:46:02Z, with `SftpTransport::survey_root` in `put_tree` and `stage_inner`.

~~~
CAPTURE short_name_home_prepare @ 2026-10-19T03:46:02Z
CAPTURE home: long C:\Users\svc-ignition-deploy short C:\Users\SVC-IG~1
TIMING prepare (short home): 46.41s
CAPTURE commit report: BatchReport { phase: Written, stale: false, moved: [0], deleted: [], removed_dirs: [], failure: None, deadline: false }
cleanup: scratch gone true, staging gone true
TIMING wflwvm01_short_name_home_prepare total: 122.32s
PASS wflwvm01_short_name_home_prepare
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 21 filtered out; finished in 127.59s
~~~

The gate also checks that the new file arrived and that `restore` put the collection back to the pre-run listing.

### Decision

Engine fix, not a test-only change. An account home spelled in 8.3 form fails every Windows `prepare` the same way; the drive-root survey also lists every ancestor of the home, which a hardened host may deny; and canonicalising the conformance tests' temp dir would hide the case on the one runner (windows-latest) that exercises it. The conformance tests keep the runner's own temp path.

### The windows-latest leg rehearsed on ign-win-836

The conformance binary was built on the Mac for `x86_64-pc-windows-msvc` with `cargo xwin test -p ignition-core --test deploy_host_conformance --no-run --target x86_64-pc-windows-msvc` (rustup's toolchain first on PATH, `RUSTFLAGS="-C target-feature=+crt-static"`, `CARGO_TARGET_DIR=$HOME/.cache/ign-xwin-target-static`, outside the repo) at HEAD 3a47279 (both G2 fixes). The first build, without `+crt-static`, did not start on the VM: it needs `vcruntime140.dll`, which the VM does not have (`dir C:\Windows\System32\vcruntime140.dll` answers File Not Found), and `--list` exited silently. The static build runs (`--list` prints its 47 tests). Copied by sftp to `C:\Users\svc-ignition-deploy\ign-live-conf-win\conf2.exe`, then run over ssh (cmd.exe) with `--test-threads=1`.

TEMP and TMP below the 8.3 form of the home, which stands in for the runner's `C:\Users\RUNNER~1\AppData\Local\Temp` (`set "TEMP=C:\Users\SVC-IG~1\ign-live-conf-win\tmp83"`, same for TMP) @ 2026-10-19T04:24:36Z:

~~~
test result: ok. 47 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 101.75s
~~~

Default TEMP @ 2026-10-19T04:26:36Z:

~~~
test result: ok. 47 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 101.87s
~~~

The scratch directory `ign-live-conf-win` was removed afterwards (`rmdir /s /q`, then `dir /b` of the home shows no `ign-live-conf-win`, no `ign-live-scratch`, no `ign-deploy`). No conformance test needed a change.

### Regression gates on the changed tree (HEAD 3a47279)

`IGNITION_LIVE_WFLWVM01=1 cargo test -p ignition-core --test live_transport_wflwvm01 -- wflwvm01_write_half_scratch wflwvm01_apply_script_run --ignored --nocapture --test-threads=1` @ 2026-10-19T04:14Z (host and key from the cached files; nothing printed). The real long home, so `survey_root` takes the home as base:

~~~
TIMING prepare (upload + verify): 41.64s
TIMING commit (one script call): 2.89s
CAPTURE commit report: BatchReport { phase: Written, stale: false, moved: [0, 1], deleted: [0], removed_dirs: ["ignition/database-connection/Doomed"], failure: None, deadline: false }
TIMING restore (one script call): 21.53s
TIMING wflwvm01_apply_script_run total: 425.83s
PASS wflwvm01_apply_script_run
TIMING wflwvm01_write_half_scratch total: 124.77s
PASS wflwvm01_write_half_scratch
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 20 filtered out; finished in 550.75s
~~~

`wflwvm01_short_name_home_prepare` PASS is above (03:46Z). `lnxvm_apply_script_run` was not run: the instruction for this execution was to leave ign-lnx-836 alone. The Linux path is covered by the offline POSIX tests (`prepare_lists_nothing_above_the_home_posix`, `mirror_stage_below_the_home_surveys_from_the_home`) and by the 23-22 gates, which ran before this change. `wflwvm01_cli_apply_rollback` (optional) was not run: the `azure-win` profile reads the macOS Keychain, which this execution must not touch.

VM power state: found deallocated (auto-shutdown at 0300 UTC) with a new public IP; started with `az vm start` at 03:07Z; left running.
