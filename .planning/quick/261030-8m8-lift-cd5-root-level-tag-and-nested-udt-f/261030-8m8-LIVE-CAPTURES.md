# Quick 261030-8m8 — Live Captures: root-level tags and nested UDT type folders (CD5a..CD5d)

| Field | Value |
| --- | --- |
| Docker image | inductiveautomation/ignition:8.3.6 |
| Compose variant | bind (compose.yml + compose.bind.yml), project ign-deploy-rig, 127.0.0.1:18290 |
| Bind path (outside repo) | $HOME/.cache/ign-deploy-rig/data/config/resources |
| deploymentMode | production |
| Rig started (UTC) | 2026-10-30T12:59:09Z |
| Docker runner | capture-cd5.sh (crates/ignition-core/tests/fixtures/live-rig-deploy) |
| Windows runner | capture22w.sh cd5w (crates/ignition-core/tests/fixtures/wflwvm01) |
| ign --version | ign 1.6.1 |
| ign config | throwaway `$HOME/.cache/ign-deploy-rig/cd5-ign-config.toml` selected with IGNITION_CLI_CONFIG (never the user config) |

Rule: every captured block is preceded by the exact command and a UTC time taken by the driver just before it. A request line names the token variable it used, never the token. Tokens live in a 0600 file outside the repo; the VM key is a temp file shown as `<key>`.

Accepted deviation (L1): the Docker rig is brought up with `capture22.sh up22` then `setup22` (the bind-mounted, production-mode rig), not with `ign rig up`. `ign rig` takes one compose file per rig and cannot express the `compose.bind.yml` overlay or the mode recreate; it is used here only for `status` and `logs`, through the throwaway config above.

Providers and names are synthetic (CAP5, CAP5B, CAP5X; R1, RInst, F, F1, G, G1, T0, A, B, T, M0, M1, R9, X1). No production gateway is contacted and no production data is used.

Rig history (written by hand): this record comes from a rig created fresh for it (`docker compose -p ign-deploy-rig down -v`, then `up22` and `setup22`; start time in the table above). An earlier run on the previous rig volume was discarded. A dry run of this driver had run on that volume, and its clean step left `core/ignition/tag-definition/CAP5/` (the root and F units) on disk after the provider delete. The same effect is recorded below in `CD5-D-clean-1`. When CAP5 was created again, the gateway loaded those files, and the S1 import answered `Tag '[CAP5]R1' already exists`. For this reason `clean` now removes the production/ files and scans before the API delete, then sweeps any core file left for these providers (`CD5-D-clean`).

Windows VM history (written by hand, from the orchestrator, 2026-10-30): the VM gateway was re-commissioned before the CD5-W pass, with the user's approval. The cached admin password was out of date, so it was reset with `gwcmd -p` through `az vm run-command` and the service was restarted. The user re-commissioned the gateway as `admin`, which changed the system auth profile to `temp_0` and reset `security-properties`. `ign adopt --key-name ign-cli-1030` then minted a new API key (`Authenticated/Roles/Administrator` plus `Authenticated/IgnCliWrite`), added bare `Authenticated` to read and `IgnCliWrite` to write (rewriting `security-properties`), and stored the key in the keyring for profile `azure-win`. The trial was reset with an authed `POST /data/api/v1/trial` (active, about 7,199 s left at 14:50 UTC by the orchestrator). Anyone comparing CD5-W with the 22/23 VM captures should know about this re-commissioning. The ssh key used for CD5-W is the 0600 key file under `~/.cache/ign-wflwvm01/` (`svc-ignition-deploy@ignwin836`); the VM refused the key in `~/.config/ign-azure-win/env`.

## CD5-D

Docker 8.3.6 rig, production deployment mode, bind-mounted resources. Expectations are the deploy model's: a mode-collection unit hides the `core` files it does not hold (`plan.rs` `is_tag_folder` / `CoreRelation::Hidden`). Each scan is ONE `POST /data/api/v1/scan/config` waited to completion. A row that does not match after the one scan gets the C11 ladder (poll every 5 s for 30 s, then a second scan).

#### ign --json rig --rig ign-deploy-rig status (through the throwaway config)

`ign --json rig --rig ign-deploy-rig status ` @ 2026-10-30T13:01:52Z

~~~
{
  "ok": true,
  "profile": null,
  "data": {
    "rig": "ign-deploy-rig",
    "project": "ign-deploy-rig",
    "compose_file": "/Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-deploy/compose.yml",
    "services": [
      {
        "name": "gw",
        "state": "running",
        "health": "healthy",
        "exit_code": 0,
        "publishers": [
          {
            "published_port": 18290,
            "target_port": 8088,
            "protocol": "tcp"
          }
        ]
      }
    ],
    "volumes": [
      "ign-deploy-rig_gw-data"
    ],
    "ports_free": false
  }
}
[exit 0]
~~~

#### GET gateway-info

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/gateway-info'` @ 2026-10-30T13:01:52Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:01:52 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 416

{"name":"ign-deploy-rig","redundancyRole":"Independent","edition":"standard","hostname":"localhost","port":"18290","ignitionVersion":"8.3.6 (b2026042713)","deploymentMode":"production","timeZone":"Coordinated Universal Time","timeZoneId":"Etc/UTC","jvmVersion":"17.0.18","allowUnsignedModules":false,"license":{"mode":"Trial","validForVersion":8,"expirationDate":"Oct 30, 2026, 3:00:28 PM","licenseRestrictions":[]}}
[curl http=200 total=0.016510s]
~~~

### S1 — layout: what the gateway writes in core for root-level tags and nested UDT type folders

Provider CAP5 is created through the REST resources route (`ign tags provider create`), then two imports through the native route `POST /data/api/v1/tags/import?provider=CAP5&type=json&collisionPolicy=Abort` (the c5ntfs call form): the `_types_` folder first, then the root tags and folders, so the instance RInst is imported after its type exists. The asynchronous flush (CD4) is given 20 s.

#### ign tags provider create CAP5

`ign --profile cd5-rig --json tags provider create CAP5 ` @ 2026-10-30T13:01:52Z

~~~
{
  "ok": true,
  "profile": "cd5-rig",
  "data": {
    "name": "CAP5"
  }
}
[exit 0]
~~~

#### native tags/import into CAP5: the _types_ folder (T0 flat; A/B/T nested)

`ign --profile cd5-rig --json api call --method POST --path /data/api/v1/tags/import --query provider=CAP5 --query type=json --query collisionPolicy=Abort --header Content-Type:\ application/octet-stream --data \{\"tags\":\[\{\"name\":\"_types_\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T0\"\,\"tagType\":\"UdtType\"\,\"tags\":\[\{\"name\":\"M0\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":0\}\]\}\,\{\"name\":\"A\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"B\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T\"\,\"tagType\":\"UdtType\"\,\"tags\":\[\{\"name\":\"M1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\]\}\]\}\]\}\]\} ` @ 2026-10-30T13:01:53Z

~~~
{
  "ok": true,
  "profile": "cd5-rig",
  "data": {
    "method": "POST",
    "path": "/data/api/v1/tags/import",
    "result": {
      "status": 200,
      "data": {"successCount":5,"failureCount":0,"failures":[]}
    }
  }
}
[exit 0]
~~~

#### native tags/import into CAP5: root R1 and RInst (of A/B/T), folders F (F1) and G (G1)

`ign --profile cd5-rig --json api call --method POST --path /data/api/v1/tags/import --query provider=CAP5 --query type=json --query collisionPolicy=Abort --header Content-Type:\ application/octet-stream --data \{\"tags\":\[\{\"name\":\"R1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\,\{\"name\":\"RInst\"\,\"tagType\":\"UdtInstance\"\,\"typeId\":\"A/B/T\"\}\,\{\"name\":\"F\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"F1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\,\{\"name\":\"G\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"G1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\]\} ` @ 2026-10-30T13:01:53Z

~~~
{
  "ok": true,
  "profile": "cd5-rig",
  "data": {
    "method": "POST",
    "path": "/data/api/v1/tags/import",
    "result": {
      "status": 200,
      "data": {"successCount":6,"failureCount":0,"failures":[]}
    }
  }
}
[exit 0]
~~~

Waiting 20 s for the gateway to flush the tag files (CD4).

#### sorted listing of every CAP5 file under the four tag types (relative to the bind dir)

`bash -c find\ core\ production\ -path\ \"\*/CAP5/\*\"\ -type\ f\ 2\>/dev/null\ \|\ LC_ALL=C\ sort ` @ 2026-10-30T13:02:13Z

~~~
core/ignition/tag-definition/CAP5/F/tags.json
core/ignition/tag-definition/CAP5/F/unary-resource.json
core/ignition/tag-definition/CAP5/G/tags.json
core/ignition/tag-definition/CAP5/G/unary-resource.json
core/ignition/tag-definition/CAP5/tags.json
core/ignition/tag-definition/CAP5/udts.json
core/ignition/tag-definition/CAP5/unary-resource.json
core/ignition/tag-group/CAP5/Default Historical/config.json
core/ignition/tag-group/CAP5/Default Historical/resource.json
core/ignition/tag-group/CAP5/Default/config.json
core/ignition/tag-group/CAP5/Default/resource.json
core/ignition/tag-provider/CAP5/config.json
core/ignition/tag-provider/CAP5/resource.json
core/ignition/tag-type-definition/CAP5/A/B/udts.json
core/ignition/tag-type-definition/CAP5/A/B/unary-resource.json
core/ignition/tag-type-definition/CAP5/A/unary-resource.json
core/ignition/tag-type-definition/CAP5/udts.json
core/ignition/tag-type-definition/CAP5/unary-resource.json
[exit 0]
~~~

#### every directory of CAP5 under the four tag types

`bash -c find\ core\ production\ -path\ \"\*/CAP5\*\"\ -type\ d\ 2\>/dev/null\ \|\ LC_ALL=C\ sort ` @ 2026-10-30T13:02:13Z

~~~
core/ignition/tag-definition/CAP5
core/ignition/tag-definition/CAP5/F
core/ignition/tag-definition/CAP5/G
core/ignition/tag-group/CAP5
core/ignition/tag-group/CAP5/Default
core/ignition/tag-group/CAP5/Default Historical
core/ignition/tag-provider/CAP5
core/ignition/tag-type-definition/CAP5
core/ignition/tag-type-definition/CAP5/A
core/ignition/tag-type-definition/CAP5/A/B
[exit 0]
~~~

#### is there a _types_ directory anywhere on the bind mount?

`bash -c r=\$\(find\ .\ -name\ _types_\ 2\>/dev/null\)\;\ echo\ \"\$\{r:-\<none\>\}\" ` @ 2026-10-30T13:02:13Z

~~~
<none>
[exit 0]
~~~

#### every CAP5 file: size, sha256, then its bytes verbatim (a [eof] line follows each file; no newline is added inside)

`bash -c find\ core\ -path\ \"\*/CAP5/\*\"\ -type\ f\ \|\ LC_ALL=C\ sort\ \|\ while\ IFS=\ read\ -r\ f\;\ do\ printf\ \"==\ %s\ size=%s\ sha256=%s\\n\"\ \"\$f\"\ \"\$\(wc\ -c\ \<\ \"\$f\"\ \|\ tr\ -d\ \"\ \"\)\"\ \"\$\(shasum\ -a\ 256\ \"\$f\"\ \|\ cut\ -d\"\ \"\ -f1\)\"\;\ cat\ \"\$f\"\;\ printf\ \"\\n\[eof\]\\n\"\;\ done ` @ 2026-10-30T13:02:13Z

~~~
== core/ignition/tag-definition/CAP5/F/tags.json size=132 sha256=9e452e81f12d0d900dfced037dba1993acfcf78af38ba96a2a8110390de45f95
[
  {
    "valueSource": "memory",
    "dataType": "Int4",
    "name": "F1",
    "defaultValue": 1,
    "tagType": "AtomicTag"
  }
]
[eof]
== core/ignition/tag-definition/CAP5/F/unary-resource.json size=154 sha256=8bf62a66d6a0155515359d27a0a5f40363dd0623d12c0b45b21d287320ba30e8
{
  "scope": "G",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "tags.json"
  ],
  "attributes": {
    "config": {}
  }
}
[eof]
== core/ignition/tag-definition/CAP5/G/tags.json size=132 sha256=eeb0bade74e1a8cf985b2096818e5e79ae602b942b627b852d1e5364fdba8e59
[
  {
    "valueSource": "memory",
    "dataType": "Int4",
    "name": "G1",
    "defaultValue": 1,
    "tagType": "AtomicTag"
  }
]
[eof]
== core/ignition/tag-definition/CAP5/G/unary-resource.json size=154 sha256=8bf62a66d6a0155515359d27a0a5f40363dd0623d12c0b45b21d287320ba30e8
{
  "scope": "G",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "tags.json"
  ],
  "attributes": {
    "config": {}
  }
}
[eof]
== core/ignition/tag-definition/CAP5/tags.json size=132 sha256=ea4059c9add6e7de7f38d8645f21d1d4932b46a89a9d93f02e14070f2de74438
[
  {
    "valueSource": "memory",
    "dataType": "Int4",
    "name": "R1",
    "defaultValue": 1,
    "tagType": "AtomicTag"
  }
]
[eof]
== core/ignition/tag-definition/CAP5/udts.json size=84 sha256=32db14cb77d9df5e5405d4a942c91c5bbdbc22c4bcc1fe16aba41acf4057849a
[
  {
    "name": "RInst",
    "typeId": "A/B/T",
    "tagType": "UdtInstance"
  }
]
[eof]
== core/ignition/tag-definition/CAP5/unary-resource.json size=171 sha256=e6ca38f4553c5c6f6a292cf6df3541ddce4036550056ae611738cc12d56c4e97
{
  "scope": "G",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "tags.json",
    "udts.json"
  ],
  "attributes": {
    "config": {}
  }
}
[eof]
== core/ignition/tag-group/CAP5/Default Historical/config.json size=199 sha256=3681179aa816d7860708c77fb0c8364e648fc7c203ed637117b71384ff677c41
{
  "config": {
    "mode": "Direct",
    "oneShot": false,
    "opcDataMode": "Subscribed",
    "optWriteTimeout": 60000,
    "optWrites": false,
    "rate": 10000,
    "readAfterWrite": false
  }
}
[eof]
== core/ignition/tag-group/CAP5/Default Historical/resource.json size=136 sha256=6171900ae01a3a5d8fa7dda5fdc07283a260008caaec82f475d12147e593ba64
{
  "scope": "A",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "config.json"
  ],
  "attributes": {}
}
[eof]
== core/ignition/tag-group/CAP5/Default/config.json size=198 sha256=2cbb0ed18730c5572182dd61952a0dbe271908f9aaef4779d54f6ab2b3409b4b
{
  "config": {
    "mode": "Direct",
    "oneShot": false,
    "opcDataMode": "Subscribed",
    "optWriteTimeout": 60000,
    "optWrites": false,
    "rate": 1000,
    "readAfterWrite": false
  }
}
[eof]
== core/ignition/tag-group/CAP5/Default/resource.json size=136 sha256=6171900ae01a3a5d8fa7dda5fdc07283a260008caaec82f475d12147e593ba64
{
  "scope": "A",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "config.json"
  ],
  "attributes": {}
}
[eof]
== core/ignition/tag-provider/CAP5/config.json size=307 sha256=302f76fcbc0cf1f1f3f00cf5f1eed4a8acfd188f84286f0b754340be17243e55
{
  "profile": {
    "allowBackfill": false,
    "enableTagReferenceStore": false,
    "type": "STANDARD"
  },
  "settings": {
    "defaultDatasourceName": null,
    "editPermissions": null,
    "readOnly": false,
    "readPermissions": null,
    "valuePersistence": null,
    "writePermissions": null
  }
}
[eof]
== core/ignition/tag-provider/CAP5/resource.json size=211 sha256=14f988b7fecf016f43f6e987bef851fdfcdf6bfb2c3d38ba041f67f97376668d
{
  "scope": "A",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "config.json"
  ],
  "attributes": {
    "uuid": "26640ae5-bade-41c5-8d62-c2ce5917ce57",
    "enabled": true
  }
}
[eof]
== core/ignition/tag-type-definition/CAP5/A/B/udts.json size=224 sha256=76181d0f09b8f132c9acbec4901be8295f39d013521bb04aab84aaf7bd0b1fcc
[
  {
    "name": "T",
    "tagType": "UdtType",
    "tags": [
      {
        "valueSource": "memory",
        "dataType": "Int4",
        "name": "M1",
        "value": 1,
        "tagType": "AtomicTag"
      }
    ]
  }
]
[eof]
== core/ignition/tag-type-definition/CAP5/A/B/unary-resource.json size=154 sha256=16d0f629e68eb94a5114668d97d16337320175ac606f9ed01260808ef90d483e
{
  "scope": "G",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "udts.json"
  ],
  "attributes": {
    "config": {}
  }
}
[eof]
== core/ignition/tag-type-definition/CAP5/A/unary-resource.json size=135 sha256=4e76fe7dd0d0ee1da9fe7be0fb8a8dca8027f4430bbfec579b8f478556a0cf20
{
  "scope": "G",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [],
  "attributes": {
    "config": {}
  }
}
[eof]
== core/ignition/tag-type-definition/CAP5/udts.json size=225 sha256=8cdbf390f7c0abc130ef2da139ae7f232e9036df2b3f11a6c4fa48b771e834a5
[
  {
    "name": "T0",
    "tagType": "UdtType",
    "tags": [
      {
        "valueSource": "memory",
        "dataType": "Int4",
        "name": "M0",
        "value": 0,
        "tagType": "AtomicTag"
      }
    ]
  }
]
[eof]
== core/ignition/tag-type-definition/CAP5/unary-resource.json size=154 sha256=16d0f629e68eb94a5114668d97d16337320175ac606f9ed01260808ef90d483e
{
  "scope": "G",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "udts.json"
  ],
  "attributes": {
    "config": {}
  }
}
[eof]
[exit 0]
~~~

#### tag-definition/CAP5/unary-resource.json (the provider-root marker)

`cat core/ignition/tag-definition/CAP5/unary-resource.json ` @ 2026-10-30T13:02:14Z

~~~
{
  "scope": "G",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "tags.json",
    "udts.json"
  ],
  "attributes": {
    "config": {}
  }
}
[output did not end with a newline]
[exit 0]
~~~

#### tag-type-definition/CAP5/unary-resource.json (the type-root marker)

`cat core/ignition/tag-type-definition/CAP5/unary-resource.json ` @ 2026-10-30T13:02:14Z

~~~
{
  "scope": "G",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "udts.json"
  ],
  "attributes": {
    "config": {}
  }
}
[output did not end with a newline]
[exit 0]
~~~

#### ls -la of tag-type-definition/CAP5/A (does the folder holding only a sub-folder carry its own marker?)

`ls -la core/ignition/tag-type-definition/CAP5/A ` @ 2026-10-30T13:02:14Z

~~~
total 8
drwxr-xr-x@ 4 pmannion  staff  128 Oct 30 07:01 .
drwxr-xr-x@ 5 pmannion  staff  160 Oct 30 07:01 ..
drwxr-xr-x@ 4 pmannion  staff  128 Oct 30 07:01 B
-rw-r--r--@ 1 pmannion  staff  135 Oct 30 07:01 unary-resource.json
[exit 0]
~~~

Observation CD5b-A docker: marker

#### GET resources/list/ignition/tag-provider

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/resources/list/ignition/tag-provider?limit=-1'` @ 2026-10-30T13:02:14Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:14 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 2419

{"items":[{"type":"ignition/tag-provider","name":"CAP5","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"140b3d84a8c304748bfb7b9cffb3df77d46fba4cd2f23e9efb4f45707f9bdf2c","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"26640ae5-bade-41c5-8d62-c2ce5917ce57","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP5.tagCount","metric":{"value":14,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP5.status","result":{"healthy":true,"time":1793365334551,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"System","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"39ed39673e87457ded38060c151d393f56ccba2f126107442a7e32a70b4b757d","config":{"profile":{"type":"MANAGED","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"nonUseCount":0}},"data":["config.json"],"attributes":{"uuid":"e28fb5d6-3b24-4222-a6fe-c16fccbda3e7"},"metrics":{"tagCount":{"name":"tags.provider.System.tagCount","metric":{"value":36,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.System.status","result":{"healthy":true,"time":1793365334568,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"default","description":"Default tag provider","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"dcfb8bd66642fa3d763795dc25d9abfcb9676d1e9c3f59249c31ee78e026d813","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"defaultDatasourceName":null,"readPermissions":{"type":"AllOf","securityLevels":[]},"readOnly":false,"writePermissions":{"type":"AllOf","securityLevels":[]},"editPermissions":{"type":"AllOf","securityLevels":[]},"valuePersistence":"Database"}},"data":["config.json"],"attributes":{"uuid":"4c8d1102-b9bf-4b6e-999f-6e19f6fbf761"},"metrics":{"tagCount":{"name":"tags.provider.default.tagCount","metric":{"value":1,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.default.status","result":{"healthy":true,"time":1793365334571,"message":"Running"}}}}],"metadata":{"total":3,"matching":3,"limit":-1,"offset":0}}
[curl http=200 total=0.124974s]
~~~

#### GET tags/export?provider=CAP5&type=json

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/tags/export?provider=CAP5&type=json'` @ 2026-10-30T13:02:14Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:14 GMT
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
      "dataType": "Int4",
      "defaultValue": 1,
      "name": "R1",
      "tagType": "AtomicTag",
      "value": 1,
      "valueSource": "memory"
    },
    {
      "name": "F",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "F1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "RInst",
      "tagType": "UdtInstance",
      "tags": [
        {
          "name": "M1",
          "tagType": "AtomicTag"
        }
      ],
      "typeId": "A/B/T"
    },
    {
      "name": "G",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "G1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "_types_",
      "tagType": "Folder",
      "tags": [
        {
          "name": "T0",
          "tagType": "UdtType",
          "tags": [
            {
              "dataType": "Int4",
              "name": "M0",
              "tagType": "AtomicTag",
              "value": 0,
              "valueSource": "memory"
            }
          ]
        },
        {
          "name": "A",
          "tagType": "Folder",
          "tags": [
            {
              "name": "B",
              "tagType": "Folder",
              "tags": [
                {
                  "name": "T",
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
      ]
    }
  ]
}
[curl http=200 total=0.150506s]
~~~

#### probe: provider listing and every exported tag of CAP5

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-deploy/capture-cd5.sh probe CAP5 ` @ 2026-10-30T13:02:14Z

~~~
provider=CAP5 listed=yes(core) export: F<Folder> F/F1=1 G<Folder> G/G1=1 R1=1 RInst<UdtInstance> RInst/M1<AtomicTag> _types_<Folder> _types_/A<Folder> _types_/A/B<Folder> _types_/A/B/T<UdtType> _types_/A/B/T/M1=1 _types_/T0<UdtType> _types_/T0/M0=0
[exit 0]
~~~

#### gateway log, WARN and above since the previous scan (ign logs --since 1793365312986; timestamp, logger, level, message)

`bash -c \"\$0\"\ --profile\ \"\$1\"\ --json\ logs\ --min-level\ warn\ --since\ \"\$2\"\ --limit\ 200\ \|\ jq\ -c\ \".data.items\[\]\ \|\ \{timestamp\,\ loggerName\,\ level\,\ message\}\" ign cd5-rig 1793365312986 ` @ 2026-10-30T13:02:14Z

~~~
{"timestamp":1793365313804,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365313804,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365313804,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365313804,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365313804,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365313804,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365313804,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365313804,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365313804,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365313804,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365313337,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365313310,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365313275,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365313237,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365313237,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365313199,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365313199,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365313168,"loggerName":"com.inductiveautomation.ignition.gateway.tags.ep.StandardTagProviderExtensionPoint","level":"ERROR","message":"valuePersistence not found in Tag Provider: 'CAP5' profile, defaulting to Database"}
{"timestamp":1793365313114,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
[exit 0]
~~~

#### container log through ign rig logs --tail 400 gw, WARN/ERROR lines only (last 40)

`bash -c \"\$0\"\ rig\ --rig\ ign-deploy-rig\ logs\ --tail\ 400\ gw\ 2\>\&1\ \|\ grep\ -wE\ \"WARN\|ERROR\"\ \|\ tail\ -40 ign ` @ 2026-10-30T13:02:15Z

~~~
[exit 0]
~~~

### S2 — write: plain file-level writes into production/, then ONE scan for all rows

(a) Production-only provider CAP5B, the C11 shape: the gateway-written core files of CAP5 for all four tag types copied to `production/ignition/<type>/CAP5B/`, with a fresh uuid in the tag-provider `resource.json` (exact-lexeme edit). (b) Overrides of CAP5 in `production/`: the root unit of `tag-definition/CAP5` (marker, `tags.json`, `udts.json`) with R1 1 -> 2; the unit `tag-type-definition/CAP5/A/B/` with M1 1 -> 2; the control unit `tag-definition/CAP5/F/` with F1 1 -> 2. G is never written to `production/`. Every value edit is an exact-lexeme perl substitution on one file (CD4).

#### the files written into production/ (relative paths, sha256)

`bash -c find\ production\ -type\ f\ \|\ LC_ALL=C\ sort\ \|\ while\ IFS=\ read\ -r\ f\;\ do\ printf\ \"%s\ %s\\n\"\ \"\$\(shasum\ -a\ 256\ \"\$f\"\ \|\ cut\ -c1-16\)\"\ \"\$f\"\;\ done ` @ 2026-10-30T13:02:15Z

~~~
ef9bd2d6b678b350 production/config-mode.json
48018c641e5532ef production/ignition/tag-definition/CAP5/F/tags.json
8bf62a66d6a01555 production/ignition/tag-definition/CAP5/F/unary-resource.json
88d6d14a53ccb164 production/ignition/tag-definition/CAP5/tags.json
32db14cb77d9df5e production/ignition/tag-definition/CAP5/udts.json
e6ca38f4553c5c6f production/ignition/tag-definition/CAP5/unary-resource.json
9e452e81f12d0d90 production/ignition/tag-definition/CAP5B/F/tags.json
8bf62a66d6a01555 production/ignition/tag-definition/CAP5B/F/unary-resource.json
eeb0bade74e1a8cf production/ignition/tag-definition/CAP5B/G/tags.json
8bf62a66d6a01555 production/ignition/tag-definition/CAP5B/G/unary-resource.json
ea4059c9add6e7de production/ignition/tag-definition/CAP5B/tags.json
32db14cb77d9df5e production/ignition/tag-definition/CAP5B/udts.json
e6ca38f4553c5c6f production/ignition/tag-definition/CAP5B/unary-resource.json
3681179aa816d786 production/ignition/tag-group/CAP5B/Default Historical/config.json
6171900ae01a3a5d production/ignition/tag-group/CAP5B/Default Historical/resource.json
2cbb0ed18730c557 production/ignition/tag-group/CAP5B/Default/config.json
6171900ae01a3a5d production/ignition/tag-group/CAP5B/Default/resource.json
302f76fcbc0cf1f1 production/ignition/tag-provider/CAP5B/config.json
86edc47f53fde53e production/ignition/tag-provider/CAP5B/resource.json
fb8317ed67bb4c36 production/ignition/tag-type-definition/CAP5/A/B/udts.json
16d0f629e68eb94a production/ignition/tag-type-definition/CAP5/A/B/unary-resource.json
76181d0f09b8f132 production/ignition/tag-type-definition/CAP5B/A/B/udts.json
16d0f629e68eb94a production/ignition/tag-type-definition/CAP5B/A/B/unary-resource.json
4e76fe7dd0d0ee1d production/ignition/tag-type-definition/CAP5B/A/unary-resource.json
8cdbf390f7c0abc1 production/ignition/tag-type-definition/CAP5B/udts.json
16d0f629e68eb94a production/ignition/tag-type-definition/CAP5B/unary-resource.json
[exit 0]
~~~

#### diff core vs production for the three CAP5 overrides and the CAP5B tag-provider resource.json (the edits only)

`bash -c for\ f\ in\ tag-definition/CAP5/tags.json\ tag-type-definition/CAP5/A/B/udts.json\ tag-definition/CAP5/F/tags.json\;\ do\ diff\ core/ignition/\$f\ production/ignition/\$f\;\ done\;\ diff\ core/ignition/tag-provider/CAP5/resource.json\ production/ignition/tag-provider/CAP5B/resource.json\ \|\ sed\ -E\ \"s/\[0-9a-f\]\{8\}-\[0-9a-f-\]\{27\}/\<uuid\>/\"\;\ true ` @ 2026-10-30T13:02:16Z

~~~
6c6
<     "defaultValue": 1,
---
>     "defaultValue": 2,
10c10
<         "value": 1,
---
>         "value": 2,
6c6
<     "defaultValue": 1,
---
>     "defaultValue": 2,
10c10
<     "uuid": "<uuid>",
---
>     "uuid": "<uuid>",
[exit 0]
~~~

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-30T13:02:16Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:16 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1793365262564,
  "lastScanDuration": 410
}
[curl http=200 total=0.025752s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-30T13:02:17Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:17 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1793365336933,
  "lastScanDuration": 539
}
[curl http=200 total=0.009405s]
~~~

#### GET resources/list/ignition/tag-provider

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/resources/list/ignition/tag-provider?limit=-1'` @ 2026-10-30T13:02:17Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:17 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 3226

{"items":[{"type":"ignition/tag-provider","name":"CAP5","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"140b3d84a8c304748bfb7b9cffb3df77d46fba4cd2f23e9efb4f45707f9bdf2c","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"26640ae5-bade-41c5-8d62-c2ce5917ce57","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP5.tagCount","metric":{"value":14,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP5.status","result":{"healthy":true,"time":1793365337078,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"CAP5B","description":null,"enabled":true,"version":1,"collection":"production","collections":["production"],"signature":"bf4a85dc584df9432ee40b49389c828727999b5cdf04c41d190faea3714a7de9","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"56e84c85-555c-450c-9ec3-447225762b36","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP5B.tagCount","metric":{"value":14,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP5B.status","result":{"healthy":true,"time":1793365337079,"message":"Starting"}}}},{"type":"ignition/tag-provider","name":"System","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"39ed39673e87457ded38060c151d393f56ccba2f126107442a7e32a70b4b757d","config":{"profile":{"type":"MANAGED","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"nonUseCount":0}},"data":["config.json"],"attributes":{"uuid":"e28fb5d6-3b24-4222-a6fe-c16fccbda3e7"},"metrics":{"tagCount":{"name":"tags.provider.System.tagCount","metric":{"value":36,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.System.status","result":{"healthy":true,"time":1793365337083,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"default","description":"Default tag provider","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"dcfb8bd66642fa3d763795dc25d9abfcb9676d1e9c3f59249c31ee78e026d813","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"defaultDatasourceName":null,"readPermissions":{"type":"AllOf","securityLevels":[]},"readOnly":false,"writePermissions":{"type":"AllOf","securityLevels":[]},"editPermissions":{"type":"AllOf","securityLevels":[]},"valuePersistence":"Database"}},"data":["config.json"],"attributes":{"uuid":"4c8d1102-b9bf-4b6e-999f-6e19f6fbf761"},"metrics":{"tagCount":{"name":"tags.provider.default.tagCount","metric":{"value":1,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.default.status","result":{"healthy":true,"time":1793365337083,"message":"Running"}}}}],"metadata":{"total":4,"matching":4,"limit":-1,"offset":0}}
[curl http=200 total=0.044273s]
~~~

#### GET tags/export?provider=CAP5&type=json

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/tags/export?provider=CAP5&type=json'` @ 2026-10-30T13:02:17Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:17 GMT
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
      "dataType": "Int4",
      "defaultValue": 2,
      "name": "R1",
      "tagType": "AtomicTag",
      "valueSource": "memory"
    },
    {
      "name": "F",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 2,
          "name": "F1",
          "tagType": "AtomicTag",
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "RInst",
      "tagType": "UdtInstance",
      "tags": [
        {
          "name": "M1",
          "tagType": "AtomicTag"
        }
      ],
      "typeId": "A/B/T"
    },
    {
      "name": "G",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "G1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "_types_",
      "tagType": "Folder",
      "tags": [
        {
          "name": "T0",
          "tagType": "UdtType",
          "tags": [
            {
              "dataType": "Int4",
              "name": "M0",
              "tagType": "AtomicTag",
              "value": 0,
              "valueSource": "memory"
            }
          ]
        },
        {
          "name": "A",
          "tagType": "Folder",
          "tags": [
            {
              "name": "B",
              "tagType": "Folder",
              "tags": [
                {
                  "name": "T",
                  "tagType": "UdtType",
                  "tags": [
                    {
                      "dataType": "Int4",
                      "name": "M1",
                      "tagType": "AtomicTag",
                      "value": 2,
                      "valueSource": "memory"
                    }
                  ]
                }
              ]
            }
          ]
        }
      ]
    }
  ]
}
[curl http=200 total=0.058222s]
~~~

#### GET tags/export?provider=CAP5B&type=json

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/tags/export?provider=CAP5B&type=json'` @ 2026-10-30T13:02:17Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:17 GMT
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
      "name": "F",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "F1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "dataType": "Int4",
      "defaultValue": 1,
      "name": "R1",
      "tagType": "AtomicTag",
      "value": 1,
      "valueSource": "memory"
    },
    {
      "name": "G",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "G1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "RInst",
      "tagType": "UdtInstance",
      "tags": [
        {
          "name": "M1",
          "tagType": "AtomicTag"
        }
      ],
      "typeId": "A/B/T"
    },
    {
      "name": "_types_",
      "tagType": "Folder",
      "tags": [
        {
          "name": "T0",
          "tagType": "UdtType",
          "tags": [
            {
              "dataType": "Int4",
              "name": "M0",
              "tagType": "AtomicTag",
              "value": 0,
              "valueSource": "memory"
            }
          ]
        },
        {
          "name": "A",
          "tagType": "Folder",
          "tags": [
            {
              "name": "B",
              "tagType": "Folder",
              "tags": [
                {
                  "name": "T",
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
      ]
    }
  ]
}
[curl http=200 total=0.036310s]
~~~

#### probe: provider listing and every exported tag of CAP5 CAP5B

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-deploy/capture-cd5.sh probe CAP5 CAP5B ` @ 2026-10-30T13:02:17Z

~~~
provider=CAP5 listed=yes(core) export: F<Folder> F/F1=2 G<Folder> G/G1=1 R1=2 RInst<UdtInstance> RInst/M1<AtomicTag> _types_<Folder> _types_/A<Folder> _types_/A/B<Folder> _types_/A/B/T<UdtType> _types_/A/B/T/M1=2 _types_/T0<UdtType> _types_/T0/M0=0
provider=CAP5B listed=yes(production) export: F<Folder> F/F1=1 G<Folder> G/G1=1 R1=1 RInst<UdtInstance> RInst/M1<AtomicTag> _types_<Folder> _types_/A<Folder> _types_/A/B<Folder> _types_/A/B/T<UdtType> _types_/A/B/T/M1=1 _types_/T0<UdtType> _types_/T0/M0=0
[exit 0]
~~~

#### S2 rows CAP5B (production-only provider, C11 shape): CD5a R1 RInst; CD5b T (M1); control F1; standard G1 T0

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-deploy/capture-cd5.sh check CAP5B R1=1 RInst F/F1=1 G/G1=1 _types_/T0 _types_/A/B/T/M1=1 ` @ 2026-10-30T13:02:17Z

~~~
ok   R1=1 (observed =1)
ok   RInst (observed <UdtInstance>)
ok   F/F1=1 (observed =1)
ok   G/G1=1 (observed =1)
ok   _types_/T0 (observed <UdtType>)
ok   _types_/A/B/T/M1=1 (observed =1)
[exit 0]
~~~

#### S2 rows CAP5 (production overrides): CD5a R1=2 and G1 not hidden; CD5b M1=2 and T0 not hidden; control F1=2

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-deploy/capture-cd5.sh check CAP5 R1=2 G/G1=1 _types_/A/B/T/M1=2 _types_/T0 F/F1=2 RInst ` @ 2026-10-30T13:02:17Z

~~~
ok   R1=2 (observed =2)
ok   G/G1=1 (observed =1)
ok   _types_/A/B/T/M1=2 (observed =2)
ok   _types_/T0 (observed <UdtType>)
ok   F/F1=2 (observed =2)
ok   RInst (observed <UdtInstance>)
[exit 0]
~~~

#### S2 row: CAP5B is listed as a tag provider

`bash -c echo\ \"CAP5B\ listed:\ \$1\" _ yes\(production\) ` @ 2026-10-30T13:02:17Z

~~~
CAP5B listed: yes(production)
[exit 0]
~~~

#### gateway log, WARN and above since the previous scan (ign logs --since 1793365336359; timestamp, logger, level, message)

`bash -c \"\$0\"\ --profile\ \"\$1\"\ --json\ logs\ --min-level\ warn\ --since\ \"\$2\"\ --limit\ 200\ \|\ jq\ -c\ \".data.items\[\]\ \|\ \{timestamp\,\ loggerName\,\ level\,\ message\}\" ign cd5-rig 1793365336359 ` @ 2026-10-30T13:02:17Z

~~~
{"timestamp":1793365337026,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365337022,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365337022,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365337011,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365337010,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365336971,"loggerName":"com.inductiveautomation.ignition.gateway.tags.ep.StandardTagProviderExtensionPoint","level":"ERROR","message":"valuePersistence not found in Tag Provider: 'CAP5B' profile, defaulting to Database"}
[exit 0]
~~~

#### container log through ign rig logs --tail 400 gw, WARN/ERROR lines only (last 40)

`bash -c \"\$0\"\ rig\ --rig\ ign-deploy-rig\ logs\ --tail\ 400\ gw\ 2\>\&1\ \|\ grep\ -wE\ \"WARN\|ERROR\"\ \|\ tail\ -40 ign ` @ 2026-10-30T13:02:17Z

~~~
[exit 0]
~~~

Script reading (S2): every row matched its expectation after ONE scan; no ladder needed.

### S3 — change: one member value edited in each production/ copy, then ONE scan

CAP5B R1 1 -> 3, CAP5B M1 1 -> 3, CAP5B F1 1 -> 3 (control); CAP5 overrides R1 2 -> 3, M1 2 -> 3, F1 2 -> 3 (control). Exact-lexeme edits.

#### the six edited files (value lines only)

`bash -c for\ f\ in\ tag-definition/CAP5B/tags.json\ tag-type-definition/CAP5B/A/B/udts.json\ tag-definition/CAP5B/F/tags.json\ tag-definition/CAP5/tags.json\ tag-type-definition/CAP5/A/B/udts.json\ tag-definition/CAP5/F/tags.json\;\ do\ printf\ \"%s:\ %s\\n\"\ \"\$f\"\ \"\$\(grep\ -o\ \"\\\"value\\\":\ \[0-9\]\*\"\ production/ignition/\$f\ \|\ tr\ \"\\n\"\ \"\ \"\)\"\;\ done ` @ 2026-10-30T13:02:18Z

~~~
tag-definition/CAP5B/tags.json: 
tag-type-definition/CAP5B/A/B/udts.json: "value": 3 
tag-definition/CAP5B/F/tags.json: 
tag-definition/CAP5/tags.json: 
tag-type-definition/CAP5/A/B/udts.json: "value": 3 
tag-definition/CAP5/F/tags.json: 
[exit 0]
~~~

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-30T13:02:18Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:18 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1793365336933,
  "lastScanDuration": 539
}
[curl http=200 total=0.004777s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-30T13:02:18Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:18 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1793365338440,
  "lastScanDuration": 220
}
[curl http=200 total=0.008851s]
~~~

#### GET resources/list/ignition/tag-provider

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/resources/list/ignition/tag-provider?limit=-1'` @ 2026-10-30T13:02:18Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:18 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 3225

{"items":[{"type":"ignition/tag-provider","name":"CAP5","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"140b3d84a8c304748bfb7b9cffb3df77d46fba4cd2f23e9efb4f45707f9bdf2c","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"26640ae5-bade-41c5-8d62-c2ce5917ce57","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP5.tagCount","metric":{"value":14,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP5.status","result":{"healthy":true,"time":1793365338863,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"CAP5B","description":null,"enabled":true,"version":1,"collection":"production","collections":["production"],"signature":"bf4a85dc584df9432ee40b49389c828727999b5cdf04c41d190faea3714a7de9","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"56e84c85-555c-450c-9ec3-447225762b36","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP5B.tagCount","metric":{"value":14,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP5B.status","result":{"healthy":true,"time":1793365338864,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"System","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"39ed39673e87457ded38060c151d393f56ccba2f126107442a7e32a70b4b757d","config":{"profile":{"type":"MANAGED","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"nonUseCount":0}},"data":["config.json"],"attributes":{"uuid":"e28fb5d6-3b24-4222-a6fe-c16fccbda3e7"},"metrics":{"tagCount":{"name":"tags.provider.System.tagCount","metric":{"value":36,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.System.status","result":{"healthy":true,"time":1793365338865,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"default","description":"Default tag provider","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"dcfb8bd66642fa3d763795dc25d9abfcb9676d1e9c3f59249c31ee78e026d813","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"defaultDatasourceName":null,"readPermissions":{"type":"AllOf","securityLevels":[]},"readOnly":false,"writePermissions":{"type":"AllOf","securityLevels":[]},"editPermissions":{"type":"AllOf","securityLevels":[]},"valuePersistence":"Database"}},"data":["config.json"],"attributes":{"uuid":"4c8d1102-b9bf-4b6e-999f-6e19f6fbf761"},"metrics":{"tagCount":{"name":"tags.provider.default.tagCount","metric":{"value":1,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.default.status","result":{"healthy":true,"time":1793365338866,"message":"Running"}}}}],"metadata":{"total":4,"matching":4,"limit":-1,"offset":0}}
[curl http=200 total=0.023441s]
~~~

#### GET tags/export?provider=CAP5&type=json

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/tags/export?provider=CAP5&type=json'` @ 2026-10-30T13:02:18Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:18 GMT
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
      "dataType": "Int4",
      "defaultValue": 3,
      "name": "R1",
      "tagType": "AtomicTag",
      "valueSource": "memory"
    },
    {
      "name": "F",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 3,
          "name": "F1",
          "tagType": "AtomicTag",
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "RInst",
      "tagType": "UdtInstance",
      "tags": [
        {
          "name": "M1",
          "tagType": "AtomicTag"
        }
      ],
      "typeId": "A/B/T"
    },
    {
      "name": "G",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "G1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "_types_",
      "tagType": "Folder",
      "tags": [
        {
          "name": "T0",
          "tagType": "UdtType",
          "tags": [
            {
              "dataType": "Int4",
              "name": "M0",
              "tagType": "AtomicTag",
              "value": 0,
              "valueSource": "memory"
            }
          ]
        },
        {
          "name": "A",
          "tagType": "Folder",
          "tags": [
            {
              "name": "B",
              "tagType": "Folder",
              "tags": [
                {
                  "name": "T",
                  "tagType": "UdtType",
                  "tags": [
                    {
                      "dataType": "Int4",
                      "name": "M1",
                      "tagType": "AtomicTag",
                      "value": 3,
                      "valueSource": "memory"
                    }
                  ]
                }
              ]
            }
          ]
        }
      ]
    }
  ]
}
[curl http=200 total=0.022544s]
~~~

#### GET tags/export?provider=CAP5B&type=json

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/tags/export?provider=CAP5B&type=json'` @ 2026-10-30T13:02:18Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:18 GMT
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
      "name": "F",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 3,
          "name": "F1",
          "tagType": "AtomicTag",
          "valueSource": "memory"
        }
      ]
    },
    {
      "dataType": "Int4",
      "defaultValue": 3,
      "name": "R1",
      "tagType": "AtomicTag",
      "valueSource": "memory"
    },
    {
      "name": "G",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "G1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "RInst",
      "tagType": "UdtInstance",
      "tags": [
        {
          "name": "M1",
          "tagType": "AtomicTag"
        }
      ],
      "typeId": "A/B/T"
    },
    {
      "name": "_types_",
      "tagType": "Folder",
      "tags": [
        {
          "name": "T0",
          "tagType": "UdtType",
          "tags": [
            {
              "dataType": "Int4",
              "name": "M0",
              "tagType": "AtomicTag",
              "value": 0,
              "valueSource": "memory"
            }
          ]
        },
        {
          "name": "A",
          "tagType": "Folder",
          "tags": [
            {
              "name": "B",
              "tagType": "Folder",
              "tags": [
                {
                  "name": "T",
                  "tagType": "UdtType",
                  "tags": [
                    {
                      "dataType": "Int4",
                      "name": "M1",
                      "tagType": "AtomicTag",
                      "value": 3,
                      "valueSource": "memory"
                    }
                  ]
                }
              ]
            }
          ]
        }
      ]
    }
  ]
}
[curl http=200 total=0.028504s]
~~~

#### probe: provider listing and every exported tag of CAP5 CAP5B

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-deploy/capture-cd5.sh probe CAP5 CAP5B ` @ 2026-10-30T13:02:18Z

~~~
provider=CAP5 listed=yes(core) export: F<Folder> F/F1=3 G<Folder> G/G1=1 R1=3 RInst<UdtInstance> RInst/M1<AtomicTag> _types_<Folder> _types_/A<Folder> _types_/A/B<Folder> _types_/A/B/T<UdtType> _types_/A/B/T/M1=3 _types_/T0<UdtType> _types_/T0/M0=0
provider=CAP5B listed=yes(production) export: F<Folder> F/F1=3 G<Folder> G/G1=1 R1=3 RInst<UdtInstance> RInst/M1<AtomicTag> _types_<Folder> _types_/A<Folder> _types_/A/B<Folder> _types_/A/B/T<UdtType> _types_/A/B/T/M1=3 _types_/T0<UdtType> _types_/T0/M0=0
[exit 0]
~~~

#### S3 rows CAP5B: CD5a R1=3; CD5b M1=3; control F1=3

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-deploy/capture-cd5.sh check CAP5B R1=3 _types_/A/B/T/M1=3 F/F1=3 ` @ 2026-10-30T13:02:19Z

~~~
ok   R1=3 (observed =3)
ok   _types_/A/B/T/M1=3 (observed =3)
ok   F/F1=3 (observed =3)
[exit 0]
~~~

#### S3 rows CAP5: CD5a R1=3; CD5b M1=3; control F1=3

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-deploy/capture-cd5.sh check CAP5 R1=3 _types_/A/B/T/M1=3 F/F1=3 ` @ 2026-10-30T13:02:19Z

~~~
ok   R1=3 (observed =3)
ok   _types_/A/B/T/M1=3 (observed =3)
ok   F/F1=3 (observed =3)
[exit 0]
~~~

#### gateway log, WARN and above since the previous scan (ign logs --since 1793365338202; timestamp, logger, level, message)

`bash -c \"\$0\"\ --profile\ \"\$1\"\ --json\ logs\ --min-level\ warn\ --since\ \"\$2\"\ --limit\ 200\ \|\ jq\ -c\ \".data.items\[\]\ \|\ \{timestamp\,\ loggerName\,\ level\,\ message\}\" ign cd5-rig 1793365338202 ` @ 2026-10-30T13:02:19Z

~~~
[exit 0]
~~~

#### container log through ign rig logs --tail 400 gw, WARN/ERROR lines only (last 40)

`bash -c \"\$0\"\ rig\ --rig\ ign-deploy-rig\ logs\ --tail\ 400\ gw\ 2\>\&1\ \|\ grep\ -wE\ \"WARN\|ERROR\"\ \|\ tail\ -40 ign ` @ 2026-10-30T13:02:19Z

~~~
[exit 0]
~~~

Script reading (S3): every row matched its expectation after ONE scan; no ladder needed.

### S4 — delete: files removed from production/ (markers set to the gateway files: [] bytes), then ONE scan

CAP5B: root `tags.json` and `udts.json` removed, root marker replaced with the gateway `files: []` bytes (the CAP21 fixture shape, the consistent deploy case); `tag-type-definition/CAP5B/A/B/` removed whole; `tag-definition/CAP5B/F/` removed whole (control). CAP5: production root `tags.json` and `udts.json` removed and the production root marker set to `files: []`; the production `A/B/` override removed whole; production `F/tags.json` removed and the production F marker set to `files: []` (control).

#### production/ after the removals (relative paths)

`bash -c find\ production\ -type\ f\ \|\ LC_ALL=C\ sort ` @ 2026-10-30T13:02:19Z

~~~
production/config-mode.json
production/ignition/tag-definition/CAP5/F/unary-resource.json
production/ignition/tag-definition/CAP5/unary-resource.json
production/ignition/tag-definition/CAP5B/G/tags.json
production/ignition/tag-definition/CAP5B/G/unary-resource.json
production/ignition/tag-definition/CAP5B/unary-resource.json
production/ignition/tag-group/CAP5B/Default Historical/config.json
production/ignition/tag-group/CAP5B/Default Historical/resource.json
production/ignition/tag-group/CAP5B/Default/config.json
production/ignition/tag-group/CAP5B/Default/resource.json
production/ignition/tag-provider/CAP5B/config.json
production/ignition/tag-provider/CAP5B/resource.json
production/ignition/tag-type-definition/CAP5B/A/unary-resource.json
production/ignition/tag-type-definition/CAP5B/udts.json
production/ignition/tag-type-definition/CAP5B/unary-resource.json
[exit 0]
~~~

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-30T13:02:19Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:19 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1793365338440,
  "lastScanDuration": 220
}
[curl http=200 total=0.006995s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-30T13:02:20Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:20 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1793365340087,
  "lastScanDuration": 213
}
[curl http=200 total=0.009369s]
~~~

#### GET resources/list/ignition/tag-provider

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/resources/list/ignition/tag-provider?limit=-1'` @ 2026-10-30T13:02:20Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:20 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 3224

{"items":[{"type":"ignition/tag-provider","name":"CAP5","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"140b3d84a8c304748bfb7b9cffb3df77d46fba4cd2f23e9efb4f45707f9bdf2c","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"26640ae5-bade-41c5-8d62-c2ce5917ce57","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP5.tagCount","metric":{"value":10,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP5.status","result":{"healthy":true,"time":1793365340562,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"CAP5B","description":null,"enabled":true,"version":1,"collection":"production","collections":["production"],"signature":"bf4a85dc584df9432ee40b49389c828727999b5cdf04c41d190faea3714a7de9","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"56e84c85-555c-450c-9ec3-447225762b36","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP5B.tagCount","metric":{"value":6,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP5B.status","result":{"healthy":true,"time":1793365340568,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"System","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"39ed39673e87457ded38060c151d393f56ccba2f126107442a7e32a70b4b757d","config":{"profile":{"type":"MANAGED","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"nonUseCount":0}},"data":["config.json"],"attributes":{"uuid":"e28fb5d6-3b24-4222-a6fe-c16fccbda3e7"},"metrics":{"tagCount":{"name":"tags.provider.System.tagCount","metric":{"value":36,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.System.status","result":{"healthy":true,"time":1793365340570,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"default","description":"Default tag provider","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"dcfb8bd66642fa3d763795dc25d9abfcb9676d1e9c3f59249c31ee78e026d813","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"defaultDatasourceName":null,"readPermissions":{"type":"AllOf","securityLevels":[]},"readOnly":false,"writePermissions":{"type":"AllOf","securityLevels":[]},"editPermissions":{"type":"AllOf","securityLevels":[]},"valuePersistence":"Database"}},"data":["config.json"],"attributes":{"uuid":"4c8d1102-b9bf-4b6e-999f-6e19f6fbf761"},"metrics":{"tagCount":{"name":"tags.provider.default.tagCount","metric":{"value":1,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.default.status","result":{"healthy":true,"time":1793365340571,"message":"Running"}}}}],"metadata":{"total":4,"matching":4,"limit":-1,"offset":0}}
[curl http=200 total=0.080998s]
~~~

#### GET tags/export?provider=CAP5&type=json

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/tags/export?provider=CAP5&type=json'` @ 2026-10-30T13:02:20Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:20 GMT
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
      "name": "F",
      "tagType": "Folder"
    },
    {
      "name": "G",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "G1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "_types_",
      "tagType": "Folder",
      "tags": [
        {
          "name": "T0",
          "tagType": "UdtType",
          "tags": [
            {
              "dataType": "Int4",
              "name": "M0",
              "tagType": "AtomicTag",
              "value": 0,
              "valueSource": "memory"
            }
          ]
        },
        {
          "name": "A",
          "tagType": "Folder",
          "tags": [
            {
              "name": "B",
              "tagType": "Folder",
              "tags": [
                {
                  "name": "T",
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
      ]
    }
  ]
}
[curl http=200 total=0.048089s]
~~~

#### GET tags/export?provider=CAP5B&type=json

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/tags/export?provider=CAP5B&type=json'` @ 2026-10-30T13:02:20Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:20 GMT
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
      "name": "G",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "G1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "_types_",
      "tagType": "Folder",
      "tags": [
        {
          "name": "T0",
          "tagType": "UdtType",
          "tags": [
            {
              "dataType": "Int4",
              "name": "M0",
              "tagType": "AtomicTag",
              "value": 0,
              "valueSource": "memory"
            }
          ]
        },
        {
          "name": "A",
          "tagType": "Folder"
        }
      ]
    }
  ]
}
[curl http=200 total=0.011549s]
~~~

#### probe: provider listing and every exported tag of CAP5 CAP5B

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-deploy/capture-cd5.sh probe CAP5 CAP5B ` @ 2026-10-30T13:02:20Z

~~~
provider=CAP5 listed=yes(core) export: F<Folder> G<Folder> G/G1=1 _types_<Folder> _types_/A<Folder> _types_/A/B<Folder> _types_/A/B/T<UdtType> _types_/A/B/T/M1=1 _types_/T0<UdtType> _types_/T0/M0=0
provider=CAP5B listed=yes(production) export: G<Folder> G/G1=1 _types_<Folder> _types_/A<Folder> _types_/T0<UdtType> _types_/T0/M0=0
[exit 0]
~~~

#### S4 rows CAP5B: CD5a R1 RInst gone; CD5b T gone; control F1 gone

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-deploy/capture-cd5.sh check CAP5B \!R1 \!RInst \!_types_/A/B/T \!F/F1 ` @ 2026-10-30T13:02:20Z

~~~
ok   !R1 (observed absent)
ok   !RInst (observed absent)
ok   !_types_/A/B/T (observed absent)
ok   !F/F1 (observed absent)
[exit 0]
~~~

#### S4 row: CAP5B is still listed as a tag provider

`bash -c echo\ \"CAP5B\ listed:\ \$1\" _ yes\(production\) ` @ 2026-10-30T13:02:20Z

~~~
CAP5B listed: yes(production)
[exit 0]
~~~

#### S4 rows CAP5: CD5a R1 RInst hidden by the production root unit, G1 still present; control F1 hidden

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-deploy/capture-cd5.sh check CAP5 \!R1 \!RInst G/G1=1 \!F/F1 ` @ 2026-10-30T13:02:20Z

~~~
ok   !R1 (observed absent)
ok   !RInst (observed absent)
ok   G/G1=1 (observed =1)
ok   !F/F1 (observed absent)
[exit 0]
~~~

#### gateway log, WARN and above since the previous scan (ign logs --since 1793365339853; timestamp, logger, level, message)

`bash -c \"\$0\"\ --profile\ \"\$1\"\ --json\ logs\ --min-level\ warn\ --since\ \"\$2\"\ --limit\ 200\ \|\ jq\ -c\ \".data.items\[\]\ \|\ \{timestamp\,\ loggerName\,\ level\,\ message\}\" ign cd5-rig 1793365339853 ` @ 2026-10-30T13:02:21Z

~~~
[exit 0]
~~~

#### container log through ign rig logs --tail 400 gw, WARN/ERROR lines only (last 40)

`bash -c \"\$0\"\ rig\ --rig\ ign-deploy-rig\ logs\ --tail\ 400\ gw\ 2\>\&1\ \|\ grep\ -wE\ \"WARN\|ERROR\"\ \|\ tail\ -40 ign ` @ 2026-10-30T13:02:21Z

~~~
[exit 0]
~~~

Script reading (S4): every row matched its expectation after ONE scan; no ladder needed.

#### S4 informational (not part of any verdict): with the production A/B unit removed, is T back to the core value M1=1?

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-deploy/capture-cd5.sh check CAP5 _types_/A/B/T/M1=1 ` @ 2026-10-30T13:02:21Z

~~~
ok   _types_/A/B/T/M1=1 (observed =1)
[exit 0]
~~~

### S5 — does the gateway honour unary-resource.json `files`? (OQ3, CD5c; on CAP5B)

(i) A root `tags.json` holding AtomicTag R9 (the gateway-written CAP5 root `tags.json` with the name lexeme R1 -> R9) written while the root marker still says `files: []`; ONE scan. Is R9 loaded?

#### S5(i) production CAP5B root: files and the marker

`bash -c ls\ -la\ production/ignition/tag-definition/CAP5B\;\ cat\ production/ignition/tag-definition/CAP5B/unary-resource.json\;\ echo\;\ cat\ production/ignition/tag-definition/CAP5B/tags.json\;\ echo ` @ 2026-10-30T13:02:21Z

~~~
total 16
drwxr-xr-x@ 5 pmannion  staff  160 Oct 30 07:02 .
drwxr-xr-x@ 4 pmannion  staff  128 Oct 30 07:02 ..
drwxr-xr-x@ 4 pmannion  staff  128 Oct 30 07:02 G
-rw-r--r--@ 1 pmannion  staff  132 Oct 30 07:02 tags.json
-rw-r--r--@ 1 pmannion  staff  135 Oct 30 07:02 unary-resource.json
{
  "scope": "G",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [],
  "attributes": {
    "config": {}
  }
}
[
  {
    "valueSource": "memory",
    "dataType": "Int4",
    "name": "R9",
    "defaultValue": 1,
    "tagType": "AtomicTag"
  }
]
[exit 0]
~~~

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-30T13:02:21Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:21 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1793365340087,
  "lastScanDuration": 213
}
[curl http=200 total=0.011260s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-30T13:02:22Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:22 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1793365342024,
  "lastScanDuration": 296
}
[curl http=200 total=0.006208s]
~~~

#### GET resources/list/ignition/tag-provider

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/resources/list/ignition/tag-provider?limit=-1'` @ 2026-10-30T13:02:22Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:22 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 3224

{"items":[{"type":"ignition/tag-provider","name":"CAP5","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"140b3d84a8c304748bfb7b9cffb3df77d46fba4cd2f23e9efb4f45707f9bdf2c","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"26640ae5-bade-41c5-8d62-c2ce5917ce57","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP5.tagCount","metric":{"value":10,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP5.status","result":{"healthy":true,"time":1793365342439,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"CAP5B","description":null,"enabled":true,"version":1,"collection":"production","collections":["production"],"signature":"bf4a85dc584df9432ee40b49389c828727999b5cdf04c41d190faea3714a7de9","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"56e84c85-555c-450c-9ec3-447225762b36","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP5B.tagCount","metric":{"value":6,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP5B.status","result":{"healthy":true,"time":1793365342440,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"System","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"39ed39673e87457ded38060c151d393f56ccba2f126107442a7e32a70b4b757d","config":{"profile":{"type":"MANAGED","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"nonUseCount":0}},"data":["config.json"],"attributes":{"uuid":"e28fb5d6-3b24-4222-a6fe-c16fccbda3e7"},"metrics":{"tagCount":{"name":"tags.provider.System.tagCount","metric":{"value":36,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.System.status","result":{"healthy":true,"time":1793365342447,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"default","description":"Default tag provider","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"dcfb8bd66642fa3d763795dc25d9abfcb9676d1e9c3f59249c31ee78e026d813","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"defaultDatasourceName":null,"readPermissions":{"type":"AllOf","securityLevels":[]},"readOnly":false,"writePermissions":{"type":"AllOf","securityLevels":[]},"editPermissions":{"type":"AllOf","securityLevels":[]},"valuePersistence":"Database"}},"data":["config.json"],"attributes":{"uuid":"4c8d1102-b9bf-4b6e-999f-6e19f6fbf761"},"metrics":{"tagCount":{"name":"tags.provider.default.tagCount","metric":{"value":1,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.default.status","result":{"healthy":true,"time":1793365342448,"message":"Running"}}}}],"metadata":{"total":4,"matching":4,"limit":-1,"offset":0}}
[curl http=200 total=0.045178s]
~~~

#### GET tags/export?provider=CAP5B&type=json

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/tags/export?provider=CAP5B&type=json'` @ 2026-10-30T13:02:22Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:22 GMT
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
      "name": "G",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "G1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "_types_",
      "tagType": "Folder",
      "tags": [
        {
          "name": "T0",
          "tagType": "UdtType",
          "tags": [
            {
              "dataType": "Int4",
              "name": "M0",
              "tagType": "AtomicTag",
              "value": 0,
              "valueSource": "memory"
            }
          ]
        },
        {
          "name": "A",
          "tagType": "Folder"
        }
      ]
    }
  ]
}
[curl http=200 total=0.031032s]
~~~

#### probe: provider listing and every exported tag of CAP5B

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-deploy/capture-cd5.sh probe CAP5B ` @ 2026-10-30T13:02:22Z

~~~
provider=CAP5B listed=yes(production) export: G<Folder> G/G1=1 _types_<Folder> _types_/A<Folder> _types_/T0<UdtType> _types_/T0/M0=0
[exit 0]
~~~

#### S5(i) row: is R9 loaded while the marker lists no files?

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-deploy/capture-cd5.sh check CAP5B R9=1 ` @ 2026-10-30T13:02:22Z

~~~
FAIL R9=1 (observed absent)
[exit 0]
~~~

#### gateway log, WARN and above since the previous scan (ign logs --since 1793365341703; timestamp, logger, level, message)

`bash -c \"\$0\"\ --profile\ \"\$1\"\ --json\ logs\ --min-level\ warn\ --since\ \"\$2\"\ --limit\ 200\ \|\ jq\ -c\ \".data.items\[\]\ \|\ \{timestamp\,\ loggerName\,\ level\,\ message\}\" ign cd5-rig 1793365341703 ` @ 2026-10-30T13:02:22Z

~~~
[exit 0]
~~~

#### container log through ign rig logs --tail 400 gw, WARN/ERROR lines only (last 40)

`bash -c \"\$0\"\ rig\ --rig\ ign-deploy-rig\ logs\ --tail\ 400\ gw\ 2\>\&1\ \|\ grep\ -wE\ \"WARN\|ERROR\"\ \|\ tail\ -40 ign ` @ 2026-10-30T13:02:22Z

~~~
[exit 0]
~~~

(ii) The root marker set to list `tags.json` (exact-lexeme edit of `"files": []`) and `tags.json` removed; ONE scan. Is R9 gone or still loaded, and is a WARN logged naming the resource?

#### S5(ii) production CAP5B root: files and the marker

`bash -c ls\ -la\ production/ignition/tag-definition/CAP5B\;\ cat\ production/ignition/tag-definition/CAP5B/unary-resource.json\;\ echo ` @ 2026-10-30T13:02:23Z

~~~
total 8
drwxr-xr-x@ 4 pmannion  staff  128 Oct 30 07:02 .
drwxr-xr-x@ 4 pmannion  staff  128 Oct 30 07:02 ..
drwxr-xr-x@ 4 pmannion  staff  128 Oct 30 07:02 G
-rw-r--r--@ 1 pmannion  staff  154 Oct 30 07:02 unary-resource.json
{
  "scope": "G",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "tags.json"
  ],
  "attributes": {
    "config": {}
  }
}
[exit 0]
~~~

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-30T13:02:23Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:23 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1793365342024,
  "lastScanDuration": 296
}
[curl http=200 total=0.007217s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-30T13:02:23Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:23 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1793365343444,
  "lastScanDuration": 211
}
[curl http=200 total=0.010283s]
~~~

#### GET resources/list/ignition/tag-provider

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/resources/list/ignition/tag-provider?limit=-1'` @ 2026-10-30T13:02:23Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:23 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 3224

{"items":[{"type":"ignition/tag-provider","name":"CAP5","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"140b3d84a8c304748bfb7b9cffb3df77d46fba4cd2f23e9efb4f45707f9bdf2c","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"26640ae5-bade-41c5-8d62-c2ce5917ce57","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP5.tagCount","metric":{"value":10,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP5.status","result":{"healthy":true,"time":1793365343918,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"CAP5B","description":null,"enabled":true,"version":1,"collection":"production","collections":["production"],"signature":"bf4a85dc584df9432ee40b49389c828727999b5cdf04c41d190faea3714a7de9","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"56e84c85-555c-450c-9ec3-447225762b36","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP5B.tagCount","metric":{"value":6,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP5B.status","result":{"healthy":true,"time":1793365343919,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"System","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"39ed39673e87457ded38060c151d393f56ccba2f126107442a7e32a70b4b757d","config":{"profile":{"type":"MANAGED","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"nonUseCount":0}},"data":["config.json"],"attributes":{"uuid":"e28fb5d6-3b24-4222-a6fe-c16fccbda3e7"},"metrics":{"tagCount":{"name":"tags.provider.System.tagCount","metric":{"value":36,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.System.status","result":{"healthy":true,"time":1793365343920,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"default","description":"Default tag provider","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"dcfb8bd66642fa3d763795dc25d9abfcb9676d1e9c3f59249c31ee78e026d813","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"defaultDatasourceName":null,"readPermissions":{"type":"AllOf","securityLevels":[]},"readOnly":false,"writePermissions":{"type":"AllOf","securityLevels":[]},"editPermissions":{"type":"AllOf","securityLevels":[]},"valuePersistence":"Database"}},"data":["config.json"],"attributes":{"uuid":"4c8d1102-b9bf-4b6e-999f-6e19f6fbf761"},"metrics":{"tagCount":{"name":"tags.provider.default.tagCount","metric":{"value":1,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.default.status","result":{"healthy":true,"time":1793365343921,"message":"Running"}}}}],"metadata":{"total":4,"matching":4,"limit":-1,"offset":0}}
[curl http=200 total=0.028274s]
~~~

#### GET tags/export?provider=CAP5B&type=json

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/tags/export?provider=CAP5B&type=json'` @ 2026-10-30T13:02:23Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:23 GMT
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
      "name": "G",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "G1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "_types_",
      "tagType": "Folder",
      "tags": [
        {
          "name": "T0",
          "tagType": "UdtType",
          "tags": [
            {
              "dataType": "Int4",
              "name": "M0",
              "tagType": "AtomicTag",
              "value": 0,
              "valueSource": "memory"
            }
          ]
        },
        {
          "name": "A",
          "tagType": "Folder"
        }
      ]
    }
  ]
}
[curl http=200 total=0.012883s]
~~~

#### probe: provider listing and every exported tag of CAP5B

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-deploy/capture-cd5.sh probe CAP5B ` @ 2026-10-30T13:02:23Z

~~~
provider=CAP5B listed=yes(production) export: G<Folder> G/G1=1 _types_<Folder> _types_/A<Folder> _types_/T0<UdtType> _types_/T0/M0=0
[exit 0]
~~~

#### S5(ii) row: is R9 gone after tags.json is removed while the marker lists it?

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-deploy/capture-cd5.sh check CAP5B \!R9 ` @ 2026-10-30T13:02:24Z

~~~
ok   !R9 (observed absent)
[exit 0]
~~~

#### gateway log, WARN and above since the previous scan (ign logs --since 1793365343212; timestamp, logger, level, message)

`bash -c \"\$0\"\ --profile\ \"\$1\"\ --json\ logs\ --min-level\ warn\ --since\ \"\$2\"\ --limit\ 200\ \|\ jq\ -c\ \".data.items\[\]\ \|\ \{timestamp\,\ loggerName\,\ level\,\ message\}\" ign cd5-rig 1793365343212 ` @ 2026-10-30T13:02:24Z

~~~
[exit 0]
~~~

#### container log through ign rig logs --tail 400 gw, WARN/ERROR lines only (last 40)

`bash -c \"\$0\"\ rig\ --rig\ ign-deploy-rig\ logs\ --tail\ 400\ gw\ 2\>\&1\ \|\ grep\ -wE\ \"WARN\|ERROR\"\ \|\ tail\ -40 ign ` @ 2026-10-30T13:02:24Z

~~~
[exit 0]
~~~

### S6 — a tag folder named tags.json (A5, CD5d; Docker only)

Provider CAP5X is created, root AtomicTag R1 is imported, then a Folder named `tags.json` holding X1. 20 s flush, then the listing and the log.

#### ign tags provider create CAP5X

`ign --profile cd5-rig --json tags provider create CAP5X ` @ 2026-10-30T13:02:24Z

~~~
{
  "ok": true,
  "profile": "cd5-rig",
  "data": {
    "name": "CAP5X"
  }
}
[exit 0]
~~~

#### native tags/import into CAP5X: root R1

`ign --profile cd5-rig --json api call --method POST --path /data/api/v1/tags/import --query provider=CAP5X --query type=json --query collisionPolicy=Abort --header Content-Type:\ application/octet-stream --data \{\"tags\":\[\{\"name\":\"R1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\} ` @ 2026-10-30T13:02:24Z

~~~
{
  "ok": true,
  "profile": "cd5-rig",
  "data": {
    "method": "POST",
    "path": "/data/api/v1/tags/import",
    "result": {
      "status": 200,
      "data": {"successCount":1,"failureCount":0,"failures":[]}
    }
  }
}
[exit 0]
~~~

#### native tags/import into CAP5X: a Folder named tags.json holding X1

`ign --profile cd5-rig --json api call --method POST --path /data/api/v1/tags/import --query provider=CAP5X --query type=json --query collisionPolicy=Abort --header Content-Type:\ application/octet-stream --data \{\"tags\":\[\{\"name\":\"tags.json\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"X1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\]\} ` @ 2026-10-30T13:02:24Z

~~~
{
  "ok": true,
  "profile": "cd5-rig",
  "data": {
    "method": "POST",
    "path": "/data/api/v1/tags/import",
    "result": {
      "status": 200,
      "data": {"successCount":1,"failureCount":1,"failures":[{"quality":"Error","qualitySubCode":769,"diagnosticMessage":"The name 'tags.json' is not a valid tag name"}]}
    }
  }
}
[exit 0]
~~~

Waiting 20 s for the gateway to flush the tag files (CD4).

#### every entry under core/ignition/tag-definition/CAP5X (type d/f and path)

`bash -c find\ core/ignition/tag-definition/CAP5X\ \|\ LC_ALL=C\ sort\ \|\ while\ IFS=\ read\ -r\ p\;\ do\ if\ \[\ -d\ \"\$p\"\ \]\;\ then\ echo\ \"d\ \$p\"\;\ else\ echo\ \"f\ \$p\"\;\ fi\;\ done ` @ 2026-10-30T13:02:44Z

~~~
d core/ignition/tag-definition/CAP5X
d core/ignition/tag-definition/CAP5X/tags.json
f core/ignition/tag-definition/CAP5X/tags.json/tags.json
f core/ignition/tag-definition/CAP5X/tags.json/unary-resource.json
f core/ignition/tag-definition/CAP5X/unary-resource.json
[exit 0]
~~~

#### the CAP5X files, verbatim

`bash -c find\ core/ignition/tag-definition/CAP5X\ -type\ f\ \|\ LC_ALL=C\ sort\ \|\ while\ IFS=\ read\ -r\ f\;\ do\ printf\ \"==\ %s\\n\"\ \"\$f\"\;\ cat\ \"\$f\"\;\ printf\ \"\\n\[eof\]\\n\"\;\ done ` @ 2026-10-30T13:02:44Z

~~~
== core/ignition/tag-definition/CAP5X/tags.json/tags.json
[
  {
    "valueSource": "memory",
    "dataType": "Int4",
    "name": "X1",
    "defaultValue": 1,
    "tagType": "AtomicTag"
  }
]
[eof]
== core/ignition/tag-definition/CAP5X/tags.json/unary-resource.json
{
  "scope": "G",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "tags.json"
  ],
  "attributes": {
    "config": {}
  }
}
[eof]
== core/ignition/tag-definition/CAP5X/unary-resource.json
{
  "scope": "G",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [],
  "attributes": {
    "config": {}
  }
}
[eof]
[exit 0]
~~~

#### GET tags/export?provider=CAP5X&type=json

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/tags/export?provider=CAP5X&type=json'` @ 2026-10-30T13:02:44Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:02:44 GMT
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
      "dataType": "Int4",
      "defaultValue": 1,
      "name": "R1",
      "tagType": "AtomicTag",
      "value": 1,
      "valueSource": "memory"
    },
    {
      "name": "tags.json",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "X1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "_types_",
      "tagType": "Folder"
    }
  ]
}
[curl http=200 total=0.040980s]
~~~

#### probe: CAP5X

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-deploy/capture-cd5.sh probe CAP5X ` @ 2026-10-30T13:02:44Z

~~~
provider=CAP5X listed=yes(core) export: R1=1 _types_<Folder> tags.json<Folder> tags.json/X1=1
[exit 0]
~~~

#### gateway log, WARN and above since the previous scan (ign logs --since 1793365344428; timestamp, logger, level, message)

`bash -c \"\$0\"\ --profile\ \"\$1\"\ --json\ logs\ --min-level\ warn\ --since\ \"\$2\"\ --limit\ 200\ \|\ jq\ -c\ \".data.items\[\]\ \|\ \{timestamp\,\ loggerName\,\ level\,\ message\}\" ign cd5-rig 1793365344428 ` @ 2026-10-30T13:02:44Z

~~~
{"timestamp":1793365344899,"loggerName":"tags.storage.resource","level":"WARN","message":"Failed to push ModifyResourceOperation{resourceId=ResourceId{resourcePath=ignition/tag-definition/CAP5X/_unary, collectionName=core}}: I/O error: /usr/local/bin/ignition/data/config/resources/core/ignition/tag-definition/CAP5X/tags.json.tmp -> /usr/local/bin/ignition/data/config/resources/core/ignition/tag-definition/CAP5X/tags.json: Is a directory"}
{"timestamp":1793365344898,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365344897,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365344886,"loggerName":"com.inductiveautomation.ignition.gateway.resourcecollection.ResourceTypePartition","level":"ERROR","message":"I/O error committing operation for ModifyResourceOperation{resourceId=ResourceId{resourcePath=ignition/tag-definition/CAP5X/_unary, collectionName=core}}: /usr/local/bin/ignition/data/config/resources/core/ignition/tag-definition/CAP5X/tags.json.tmp -> /usr/local/bin/ignition/data/config/resources/core/ignition/tag-definition/CAP5X/tags.json: Is a directory"}
{"timestamp":1793365344749,"loggerName":"tags.execution.batchoperations","level":"ERROR","message":"Uncaught exception executing batch context."}
{"timestamp":1793365344669,"loggerName":"tags.management.provider","level":"ERROR","message":"Error processing edit for tag path '[CAP5X]tags.json': Error_Configuration(\"The name 'tags.json' is not a valid tag name\")"}
{"timestamp":1793365344632,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365344626,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365344606,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365344579,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365344579,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365344541,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365344540,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1793365344529,"loggerName":"com.inductiveautomation.ignition.gateway.tags.ep.StandardTagProviderExtensionPoint","level":"ERROR","message":"valuePersistence not found in Tag Provider: 'CAP5X' profile, defaulting to Database"}
{"timestamp":1793365344493,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
[exit 0]
~~~

#### container log through ign rig logs --tail 400 gw, WARN/ERROR lines only (last 40)

`bash -c \"\$0\"\ rig\ --rig\ ign-deploy-rig\ logs\ --tail\ 400\ gw\ 2\>\&1\ \|\ grep\ -wE\ \"WARN\|ERROR\"\ \|\ tail\ -40 ign ` @ 2026-10-30T13:02:45Z

~~~
[exit 0]
~~~

Script reading (S6): a directory `ignition/tag-definition/CAP5X/tags.json/` exists.

### CD5-D-clean-1 — first procedure: providers CAP5, CAP5B and CAP5X deleted through the API, then production/ files removed, one scan (it left core files; see CD5-D-clean)

#### ign tags provider delete CAP5X --yes

`ign --profile cd5-rig --json --yes tags provider delete CAP5X ` @ 2026-10-30T13:03:11Z

~~~
{
  "ok": true,
  "profile": "cd5-rig",
  "data": {
    "deleted": "CAP5X"
  }
}
[exit 0]
~~~

#### ign tags provider delete CAP5B --yes

`ign --profile cd5-rig --json --yes tags provider delete CAP5B ` @ 2026-10-30T13:03:11Z

~~~
{
  "ok": true,
  "profile": "cd5-rig",
  "data": {
    "deleted": "CAP5B"
  }
}
[exit 0]
~~~

#### ign tags provider delete CAP5 --yes

`ign --profile cd5-rig --json --yes tags provider delete CAP5 ` @ 2026-10-30T13:03:11Z

~~~
{
  "ok": true,
  "profile": "cd5-rig",
  "data": {
    "deleted": "CAP5"
  }
}
[exit 0]
~~~

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-30T13:03:11Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:03:11 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1793365343444,
  "lastScanDuration": 211
}
[curl http=200 total=0.006317s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-30T13:03:12Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:03:12 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1793365391823,
  "lastScanDuration": 236
}
[curl http=200 total=0.006599s]
~~~

#### GET resources/list/ignition/tag-provider (CAP5, CAP5B, CAP5X gone?)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/resources/list/ignition/tag-provider?limit=-1'` @ 2026-10-30T13:03:12Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:03:12 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 1628

{"items":[{"type":"ignition/tag-provider","name":"System","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"39ed39673e87457ded38060c151d393f56ccba2f126107442a7e32a70b4b757d","config":{"profile":{"type":"MANAGED","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"nonUseCount":0}},"data":["config.json"],"attributes":{"uuid":"e28fb5d6-3b24-4222-a6fe-c16fccbda3e7"},"metrics":{"tagCount":{"name":"tags.provider.System.tagCount","metric":{"value":36,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.System.status","result":{"healthy":true,"time":1793365392268,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"default","description":"Default tag provider","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"dcfb8bd66642fa3d763795dc25d9abfcb9676d1e9c3f59249c31ee78e026d813","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"defaultDatasourceName":null,"readPermissions":{"type":"AllOf","securityLevels":[]},"readOnly":false,"writePermissions":{"type":"AllOf","securityLevels":[]},"editPermissions":{"type":"AllOf","securityLevels":[]},"valuePersistence":"Database"}},"data":["config.json"],"attributes":{"uuid":"4c8d1102-b9bf-4b6e-999f-6e19f6fbf761"},"metrics":{"tagCount":{"name":"tags.provider.default.tagCount","metric":{"value":1,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.default.status","result":{"healthy":true,"time":1793365392269,"message":"Running"}}}}],"metadata":{"total":2,"matching":2,"limit":-1,"offset":0}}
[curl http=200 total=0.011837s]
~~~

#### anything named CAP5* left on the bind mount

`bash -c r=\$\(find\ .\ -name\ \"CAP5\*\"\ 2\>/dev/null\ \|\ LC_ALL=C\ sort\)\;\ echo\ \"\$\{r:-\<none\>\}\" ` @ 2026-10-30T13:03:12Z

~~~
./core/ignition/tag-definition/CAP5
[exit 0]
~~~

#### production/ after the cleanup

`bash -c find\ production\ -type\ f\ \|\ LC_ALL=C\ sort ` @ 2026-10-30T13:03:12Z

~~~
production/config-mode.json
[exit 0]
~~~

### CD5-D-clean — production/ files removed and scanned, providers CAP5, CAP5B and CAP5X deleted, leftovers swept, one scan

Order matters: a provider deleted through the API while a production/ override of one of its tag units exists loses the override but keeps the core copy of that unit on disk (seen in CD5-D-clean-1), and a later create of the same name loads it again. So the production/ files go first, with a scan, then the API delete, then any core file left for these providers is removed.

#### production/ after removing every CAP5* file this driver wrote

`bash -c find\ production\ -type\ f\ \|\ LC_ALL=C\ sort ` @ 2026-10-30T13:04:12Z

~~~
production/config-mode.json
[exit 0]
~~~

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-30T13:04:12Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:04:12 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1793365391823,
  "lastScanDuration": 236
}
[curl http=200 total=0.013386s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-30T13:04:13Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:04:13 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1793365452671,
  "lastScanDuration": 247
}
[curl http=200 total=0.011940s]
~~~

#### is CAP5X still a tag provider? (no API delete needed when it is not)

`bash -c echo\ \"\$1\ listed:\ \$2\" _ CAP5X no ` @ 2026-10-30T13:04:13Z

~~~
CAP5X listed: no
[exit 0]
~~~

#### is CAP5B still a tag provider? (no API delete needed when it is not)

`bash -c echo\ \"\$1\ listed:\ \$2\" _ CAP5B no ` @ 2026-10-30T13:04:13Z

~~~
CAP5B listed: no
[exit 0]
~~~

#### is CAP5 still a tag provider? (no API delete needed when it is not)

`bash -c echo\ \"\$1\ listed:\ \$2\" _ CAP5 no ` @ 2026-10-30T13:04:13Z

~~~
CAP5 listed: no
[exit 0]
~~~

#### anything named CAP5* left on the bind mount after the API deletes

`bash -c r=\$\(find\ .\ -name\ \"CAP5\*\"\ 2\>/dev/null\ \|\ LC_ALL=C\ sort\)\;\ echo\ \"\$\{r:-\<none\>\}\" ` @ 2026-10-30T13:04:18Z

~~~
./core/ignition/tag-definition/CAP5
[exit 0]
~~~

#### POST scan/config

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-30T13:04:18Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:04:18 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1793365452671,
  "lastScanDuration": 247
}
[curl http=200 total=0.009011s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/scan/config'` @ 2026-10-30T13:04:19Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:04:19 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1793365458695,
  "lastScanDuration": 248
}
[curl http=200 total=0.004610s]
~~~

#### GET resources/list/ignition/tag-provider (CAP5, CAP5B, CAP5X gone?)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/resources/list/ignition/tag-provider?limit=-1'` @ 2026-10-30T13:04:19Z

~~~
HTTP/1.1 200 OK
Date: Fri, 30 Oct 2026 13:04:19 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 1628

{"items":[{"type":"ignition/tag-provider","name":"System","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"39ed39673e87457ded38060c151d393f56ccba2f126107442a7e32a70b4b757d","config":{"profile":{"type":"MANAGED","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"nonUseCount":0}},"data":["config.json"],"attributes":{"uuid":"e28fb5d6-3b24-4222-a6fe-c16fccbda3e7"},"metrics":{"tagCount":{"name":"tags.provider.System.tagCount","metric":{"value":36,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.System.status","result":{"healthy":true,"time":1793365459166,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"default","description":"Default tag provider","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"dcfb8bd66642fa3d763795dc25d9abfcb9676d1e9c3f59249c31ee78e026d813","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"defaultDatasourceName":null,"readPermissions":{"type":"AllOf","securityLevels":[]},"readOnly":false,"writePermissions":{"type":"AllOf","securityLevels":[]},"editPermissions":{"type":"AllOf","securityLevels":[]},"valuePersistence":"Database"}},"data":["config.json"],"attributes":{"uuid":"4c8d1102-b9bf-4b6e-999f-6e19f6fbf761"},"metrics":{"tagCount":{"name":"tags.provider.default.tagCount","metric":{"value":1,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.default.status","result":{"healthy":true,"time":1793365459167,"message":"Running"}}}}],"metadata":{"total":2,"matching":2,"limit":-1,"offset":0}}
[curl http=200 total=0.014463s]
~~~

#### anything named CAP5* left on the bind mount (after the sweep)

`bash -c r=\$\(find\ .\ -name\ \"CAP5\*\"\ 2\>/dev/null\ \|\ LC_ALL=C\ sort\)\;\ echo\ \"\$\{r:-\<none\>\}\" ` @ 2026-10-30T13:04:19Z

~~~
<none>
[exit 0]
~~~

#### production/ after the cleanup

`bash -c find\ production\ -type\ f\ \|\ LC_ALL=C\ sort ` @ 2026-10-30T13:04:19Z

~~~
production/config-mode.json
[exit 0]
~~~

#### probe: are CAP5, CAP5B and CAP5X gone?

`bash /Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/live-rig-deploy/capture-cd5.sh probe CAP5 CAP5B CAP5X ` @ 2026-10-30T13:04:19Z

~~~
provider=CAP5 listed=no export: export_unreadable
provider=CAP5B listed=no export: export_unreadable
provider=CAP5X listed=no export: export_unreadable
[exit 0]
~~~

### CD5-D-teardown

`IGN_DEPLOY_RIG_RESOURCES=$HOME/.cache/ign-deploy-rig/data/config/resources docker compose -p ign-deploy-rig -f compose.yml -f compose.bind.yml down -v` @ 2026-10-30T13:07:16Z

~~~
 Container ign-deploy-rig-gw-1 Stopping 
 Container ign-deploy-rig-gw-1 Stopped 
 Container ign-deploy-rig-gw-1 Removing 
 Container ign-deploy-rig-gw-1 Removed 
 Volume ign-deploy-rig_gw-data Removing 
 Network ign-deploy-rig_default Removing 
 Volume ign-deploy-rig_gw-data Removed 
 Network ign-deploy-rig_default Removed 
[exit 0]
~~~


## CD5-W

Windows test VM `ign-win-836` (Windows Server 2022, Ignition 8.3.6 service), reached at 52.154.75.231 as `svc-ignition-deploy` (Modify on `production` only). Gateway calls go through the `azure-win` profile with `IGNITION_URL` set for the run to this address (P8). S1 to S5 of CD5-D, with every production/ write and delete done over sftp. Each scan is ONE `POST /data/api/v1/scan/config` waited to completion; a row that does not match after it gets the C11 ladder.

`<H>` stands for exactly these ssh/sftp options, in this order (the `hardened_options` set with an identity): `-o BatchMode=yes -o StrictHostKeyChecking=yes -o UserKnownHostsFile=$HOME/.cache/ign-wflwvm01/known_hosts -o GlobalKnownHostsFile=/dev/null -o UpdateHostKeys=no -o VerifyHostKeyDNS=no -o PreferredAuthentications=publickey -o PasswordAuthentication=no -o KbdInteractiveAuthentication=no -o NumberOfPasswordPrompts=0 -o IdentitiesOnly=yes -o ForwardAgent=no -o ForwardX11=no -o ClearAllForwardings=yes -o PermitLocalCommand=no -o ControlMaster=no -o ControlPath=none -o ConnectTimeout=10 -o ServerAliveInterval=5 -o ServerAliveCountMax=3 -o LogLevel=ERROR `

#### ign status (the gateway this run talks to)

`ign --profile azure-win --json status ` @ 2026-10-30T14:31:04Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "gateway": {
      "name": "Ignition-ignwin836",
      "ignition_version": "8.3.6 (b2026042713)",
      "edition": "standard",
      "license": {
        "mode": "Trial",
        "expirationDate": "Oct 30, 2026, 4:30:06 PM"
      }
    },
    "state": "RUNNING",
    "overview": {
      "java": {
        "version": "17.0.18+8-LTS",
        "vendor": "Azul Systems, Inc.",
        "name": "OpenJDK 64-Bit Server VM"
      },
      "os": {
        "name": "Windows Server 2022",
        "arch": "amd64",
        "version": "10.0"
      },
      "uptime_ms": 1209918,
      "memory": [
        484521200,
        1073741824
      ],
      "cpu_fraction": 0.005989859292806352,
      "disk": {
        "total": 135771664384,
        "used": 24336588800
      },
      "license": {
        "state": "trial",
        "trial_remaining_s": 7141
      }
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET gateway-info (name, deploymentMode, license)

`ign --profile azure-win --json api call --method GET --path /data/api/v1/gateway-info ` @ 2026-10-30T14:31:05Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/gateway-info",
    "result": {
      "status": 200,
      "data": {"name":"Ignition-ignwin836","redundancyRole":"Independent","edition":"standard","hostname":"52.154.75.231","port":"8088","ignitionVersion":"8.3.6 (b2026042713)","deploymentMode":"production","timeZone":"Coordinated Universal Time","timeZoneId":"Etc/UTC","jvmVersion":"17.0.18","allowUnsignedModules":false,"license":{"mode":"Trial","validForVersion":8,"expirationDate":"Oct 30, 2026, 4:30:06 PM","licenseRestrictions":[]}}
    }
  }
}
[stderr]
[exit 0]
~~~

### S1 — layout: what the gateway writes in core for root-level tags and nested UDT type folders (NTFS)

Provider CAP5 is created (`ign tags provider create`), then two native imports (`_types_` first, then the root tags and folders). The flush is given 20 s. The CAP5 files of all four tag types are then read back over sftp as the service account (`get -R`, the deploy read path), listed with their bytes and sha256, and their absolute NTFS path lengths are taken from PowerShell against `LONG_PATH_LIMIT` 240.

#### ign tags provider create CAP5

`ign --profile azure-win --json tags provider create CAP5 ` @ 2026-10-30T14:31:05Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "name": "CAP5"
  }
}
[stderr]
[exit 0]
~~~

#### native tags/import into CAP5: the _types_ folder (T0 flat; A/B/T nested)

`ign --profile azure-win --json api call --method POST --path /data/api/v1/tags/import --query provider=CAP5 --query type=json --query collisionPolicy=Abort --header Content-Type:\ application/octet-stream --data \{\"tags\":\[\{\"name\":\"_types_\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T0\"\,\"tagType\":\"UdtType\"\,\"tags\":\[\{\"name\":\"M0\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource...\[398\ chars\] ` @ 2026-10-30T14:31:06Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "POST",
    "path": "/data/api/v1/tags/import",
    "result": {
      "status": 200,
      "data": {"successCount":0,"failureCount":0,"failures":[]}
    }
  }
}
[stderr]
[exit 0]
~~~

#### native tags/import into CAP5: root R1 and RInst (of A/B/T), folders F (F1) and G (G1)

`ign --profile azure-win --json api call --method POST --path /data/api/v1/tags/import --query provider=CAP5 --query type=json --query collisionPolicy=Abort --header Content-Type:\ application/octet-stream --data \{\"tags\":\[\{\"name\":\"R1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\,\{\"name\":\"RInst\"\,\"tagType\":\"UdtInstance\"\,\"typeId\":\"A/B/T\"\}\,\{\"name...\[411\ chars\] ` @ 2026-10-30T14:31:06Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "POST",
    "path": "/data/api/v1/tags/import",
    "result": {
      "status": 200,
      "data": {"successCount":0,"failureCount":0,"failures":[]}
    }
  }
}
[stderr]
[exit 0]
~~~

Waiting 20 s for the gateway to flush the tag files (CD4).

#### get -R core/ignition/tag-provider/CAP5 (sftp, as svc-ignition-deploy)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@52.154.75.231 ` @ 2026-10-30T14:31:26Z

~~~
[stdin]
get -R "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-provider/CAP5" "$TMP/local/s1/tag-provider/CAP5"
[stdout]
sftp> get -R "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-provider/CAP5" "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s1/tag-provider/CAP5"
Retrieving /C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-provider/CAP5/
[stderr]
[exit 0]
~~~

#### get -R core/ignition/tag-group/CAP5 (sftp, as svc-ignition-deploy)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@52.154.75.231 ` @ 2026-10-30T14:31:29Z

~~~
[stdin]
get -R "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-group/CAP5" "$TMP/local/s1/tag-group/CAP5"
[stdout]
sftp> get -R "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-group/CAP5" "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s1/tag-group/CAP5"
Retrieving /C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-group/CAP5/
Retrieving /C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-group/CAP5/Default
Retrieving /C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-group/CAP5/Default Historical
[stderr]
[exit 0]
~~~

#### get -R core/ignition/tag-type-definition/CAP5 (sftp, as svc-ignition-deploy)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@52.154.75.231 ` @ 2026-10-30T14:31:33Z

~~~
[stdin]
get -R "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-type-definition/CAP5" "$TMP/local/s1/tag-type-definition/CAP5"
[stdout]
sftp> get -R "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-type-definition/CAP5" "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s1/tag-type-definition/CAP5"
Retrieving /C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-type-definition/CAP5/
Retrieving /C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-type-definition/CAP5/A
Retrieving /C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-type-definition/CAP5/A/B
[stderr]
[exit 0]
~~~

#### get -R core/ignition/tag-definition/CAP5 (sftp, as svc-ignition-deploy)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@52.154.75.231 ` @ 2026-10-30T14:31:38Z

~~~
[stdin]
get -R "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/CAP5" "$TMP/local/s1/tag-definition/CAP5"
[stdout]
sftp> get -R "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/CAP5" "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s1/tag-definition/CAP5"
Retrieving /C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/CAP5/
Retrieving /C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/CAP5/F
Retrieving /C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/CAP5/G
[stderr]
[exit 0]
~~~

#### sorted listing of every CAP5 file read back (relative to core/ignition)

`bash -c cd\ \"\$1\"\ \&\&\ find\ .\ -type\ f\ \|\ sed\ \"s\|\^\\./\|\|\"\ \|\ LC_ALL=C\ sort _ $TMP/local/s1 ` @ 2026-10-30T14:31:43Z

~~~
[stdout]
tag-definition/CAP5/F/tags.json
tag-definition/CAP5/F/unary-resource.json
tag-definition/CAP5/G/tags.json
tag-definition/CAP5/G/unary-resource.json
tag-definition/CAP5/tags.json
tag-definition/CAP5/udts.json
tag-definition/CAP5/unary-resource.json
tag-group/CAP5/Default Historical/config.json
tag-group/CAP5/Default Historical/resource.json
tag-group/CAP5/Default/config.json
tag-group/CAP5/Default/resource.json
tag-provider/CAP5/config.json
tag-provider/CAP5/resource.json
tag-type-definition/CAP5/A/B/udts.json
tag-type-definition/CAP5/A/B/unary-resource.json
tag-type-definition/CAP5/A/unary-resource.json
tag-type-definition/CAP5/udts.json
tag-type-definition/CAP5/unary-resource.json
[stderr]
[exit 0]
~~~

#### every directory read back

`bash -c cd\ \"\$1\"\ \&\&\ find\ .\ -mindepth\ 1\ -type\ d\ \|\ sed\ \"s\|\^\\./\|\|\"\ \|\ LC_ALL=C\ sort _ $TMP/local/s1 ` @ 2026-10-30T14:31:43Z

~~~
[stdout]
tag-definition
tag-definition/CAP5
tag-definition/CAP5/F
tag-definition/CAP5/G
tag-group
tag-group/CAP5
tag-group/CAP5/Default
tag-group/CAP5/Default Historical
tag-provider
tag-provider/CAP5
tag-type-definition
tag-type-definition/CAP5
tag-type-definition/CAP5/A
tag-type-definition/CAP5/A/B
[stderr]
[exit 0]
~~~

#### every CAP5 tag file: size, sha256, then its bytes verbatim (a [eof] line follows each file)

`bash -c cd\ \"\$1\"\ \&\&\ find\ tag-definition\ tag-type-definition\ -type\ f\ \|\ LC_ALL=C\ sort\ \|\ while\ IFS=\ read\ -r\ f\;\ do\ printf\ \"==\ core/ignition/%s\ size=%s\ sha256=%s\\n\"\ \"\$f\"\ \"\$\(wc\ -c\ \<\ \"\$f\"\ \|\ tr\ -d\ \"\ \"\)\"\ \"\$\(shasum\ -a\ 256\ \"\$f\"\ \|\ cut\ -d\"\ \"\ -f1\)\"\;\ cat\ \"\$f\"\;\ printf\ \"\\n\[eof\]\\n\"\;\ done _ $TMP/local/s1 ` @ 2026-10-30T14:31:43Z

~~~
[stdout]
== core/ignition/tag-definition/CAP5/F/tags.json size=132 sha256=9e452e81f12d0d900dfced037dba1993acfcf78af38ba96a2a8110390de45f95
[
  {
    "valueSource": "memory",
    "dataType": "Int4",
    "name": "F1",
    "defaultValue": 1,
    "tagType": "AtomicTag"
  }
]
[eof]
== core/ignition/tag-definition/CAP5/F/unary-resource.json size=154 sha256=8bf62a66d6a0155515359d27a0a5f40363dd0623d12c0b45b21d287320ba30e8
{
  "scope": "G",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "tags.json"
  ],
  "attributes": {
    "config": {}
  }
}
[eof]
== core/ignition/tag-definition/CAP5/G/tags.json size=132 sha256=eeb0bade74e1a8cf985b2096818e5e79ae602b942b627b852d1e5364fdba8e59
[
  {
    "valueSource": "memory",
    "dataType": "Int4",
    "name": "G1",
    "defaultValue": 1,
    "tagType": "AtomicTag"
  }
]
[eof]
== core/ignition/tag-definition/CAP5/G/unary-resource.json size=154 sha256=8bf62a66d6a0155515359d27a0a5f40363dd0623d12c0b45b21d287320ba30e8
{
  "scope": "G",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "tags.json"
  ],
  "attributes": {
    "config": {}
  }
}
[eof]
== core/ignition/tag-definition/CAP5/tags.json size=132 sha256=ea4059c9add6e7de7f38d8645f21d1d4932b46a89a9d93f02e14070f2de74438
[
  {
    "valueSource": "memory",
    "dataType": "Int4",
    "name": "R1",
    "defaultValue": 1,
    "tagType": "AtomicTag"
  }
]
[eof]
== core/ignition/tag-definition/CAP5/udts.json size=84 sha256=32db14cb77d9df5e5405d4a942c91c5bbdbc22c4bcc1fe16aba41acf4057849a
[
  {
    "name": "RInst",
    "typeId": "A/B/T",
    "tagType": "UdtInstance"
  }
]
[eof]
== core/ignition/tag-definition/CAP5/unary-resource.json size=171 sha256=e6ca38f4553c5c6f6a292cf6df3541ddce4036550056ae611738cc12d56c4e97
{
  "scope": "G",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "tags.json",
    "udts.json"
  ],
  "attributes": {
    "config": {}
  }
}
[eof]
== core/ignition/tag-type-definition/CAP5/A/B/udts.json size=224 sha256=76181d0f09b8f132c9acbec4901be8295f39d013521bb04aab84aaf7bd0b1fcc
[
  {
    "name": "T",
    "tagType": "UdtType",
    "tags": [
      {
        "valueSource": "memory",
        "dataType": "Int4",
        "name": "M1",
        "value": 1,
        "tagType": "AtomicTag"
      }
    ]
  }
]
[eof]
== core/ignition/tag-type-definition/CAP5/A/B/unary-resource.json size=154 sha256=16d0f629e68eb94a5114668d97d16337320175ac606f9ed01260808ef90d483e
{
  "scope": "G",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "udts.json"
  ],
  "attributes": {
    "config": {}
  }
}
[eof]
== core/ignition/tag-type-definition/CAP5/A/unary-resource.json size=135 sha256=4e76fe7dd0d0ee1da9fe7be0fb8a8dca8027f4430bbfec579b8f478556a0cf20
{
  "scope": "G",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [],
  "attributes": {
    "config": {}
  }
}
[eof]
== core/ignition/tag-type-definition/CAP5/udts.json size=225 sha256=8cdbf390f7c0abc130ef2da139ae7f232e9036df2b3f11a6c4fa48b771e834a5
[
  {
    "name": "T0",
    "tagType": "UdtType",
    "tags": [
      {
        "valueSource": "memory",
        "dataType": "Int4",
        "name": "M0",
        "value": 0,
        "tagType": "AtomicTag"
      }
    ]
  }
]
[eof]
== core/ignition/tag-type-definition/CAP5/unary-resource.json size=154 sha256=16d0f629e68eb94a5114668d97d16337320175ac606f9ed01260808ef90d483e
{
  "scope": "G",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "udts.json"
  ],
  "attributes": {
    "config": {}
  }
}
[eof]
[stderr]
[exit 0]
~~~

#### sha256 of the Windows bytes against the Docker S1 fixtures (captured-cd5/)

`bash -c cd\ \"\$1\"\ \&\&\ for\ f\ in\ \$\(cd\ \"\$2\"\ \&\&\ find\ .\ -type\ f\ \|\ sed\ \"s\|\^\\./\|\|\"\ \|\ LC_ALL=C\ sort\)\;\ do\ a=\$\(shasum\ -a\ 256\ \"\$f\"\ 2\>/dev/null\ \|\ cut\ -c1-64\)\;\ b=\$\(shasum\ -a\ 256\ \"\$2/\$f\"\ \|\ cut\ -c1-64\)\;\ if\ \[\ \"\$a\"\ =\ \"\$b\"\ \]\;\ then\ echo\ \"same\ \ \ \ \ \ \$f\"\;\ else\ echo\ \"DIFFERENT\ \$f\"\;\ fi\;\ done _ $TMP/local/s1 $HOME/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/deploy/captured-cd5/ignition ` @ 2026-10-30T14:31:43Z

~~~
[stdout]
same      tag-definition/CAP5/F/tags.json
same      tag-definition/CAP5/F/unary-resource.json
same      tag-definition/CAP5/G/tags.json
same      tag-definition/CAP5/G/unary-resource.json
same      tag-definition/CAP5/tags.json
same      tag-definition/CAP5/udts.json
same      tag-definition/CAP5/unary-resource.json
same      tag-type-definition/CAP5/A/B/udts.json
same      tag-type-definition/CAP5/A/B/unary-resource.json
same      tag-type-definition/CAP5/A/unary-resource.json
same      tag-type-definition/CAP5/udts.json
same      tag-type-definition/CAP5/unary-resource.json
[stderr]
[exit 0]
~~~

#### ls -la of core/ignition/tag-type-definition/CAP5/A (does the folder holding only a sub-folder carry its own marker?)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@52.154.75.231 ` @ 2026-10-30T14:31:44Z

~~~
[stdin]
@ls -la "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-type-definition/CAP5/A"
[stdout]
drwx******    1 -        -               0 Oct 30 14:31 B
-rw-******    1 -        -             135 Oct 30 14:31 unary-resource.json
[stderr]
[exit 0]
~~~

Observation CD5b-A windows: marker

#### absolute NTFS path length of every file below core\ignition\tag-type-definition\CAP5 (FullName.Length; LONG_PATH_LIMIT 240)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@52.154.75.231 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIA...\[1106\ chars\] ` @ 2026-10-30T14:31:47Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; $r=Join-Path 'C:\Program Files\Inductive Automation\Ignition\data\config\resources' 'core\ignition\tag-type-definition\CAP5'; $m=0; Get-ChildItem -LiteralPath $r -Recurse -File | ForEach-Object { if($_.FullName.Length -gt $m){$m=$_.FullName.Length}; '{0} {1}' -f $_.FullName.Length, $_.FullName }; 'max={0} limit=240' -f $m
[stdout]
117 C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-type-definition\CAP5\udts.json
127 C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-type-definition\CAP5\unary-resource.json
129 C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-type-definition\CAP5\A\unary-resource.json
121 C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-type-definition\CAP5\A\B\udts.json
131 C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-type-definition\CAP5\A\B\unary-resource.json
max=131 limit=240
[6 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### GET resources/list/ignition/tag-provider

`ign --profile azure-win --json api call --method GET --path /data/api/v1/resources/list/ignition/tag-provider --query limit=-1 ` @ 2026-10-30T14:31:49Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/resources/list/ignition/tag-provider",
    "result": {
      "status": 200,
      "data": {"items":[{"type":"ignition/tag-provider","name":"CAP5","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"9126111f852618f0a357bae275c18bdedf0f4c8b9e45e3d1b314463543f4afd2","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"c93fe62e-fd8a-487c-9525-e2b8ff3f4898","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP5.tagCount","metric":{"value":14,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP5.status","result":{"healthy":true,"time":1793370709566,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"System","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"7d25b29271a47ff52b6d6712ccecbcd5964ea2baefe25c6be850d07efe39d1d5","config":{"profile":{"type":"MANAGED","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"nonUseCount":0}},"data":["config.json"],"attributes":{"uuid":"6219db0d-9b8e-4f3a-b352-c7d8cc1e44ff"},"metrics":{"tagCount":{"name":"tags.provider.System.tagCount","metric":{"value":39,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.System.status","result":{"healthy":true,"time":1793370709567,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"default","description":"Default tag provider","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"c75b0f1fed92b9087d20984eddc79595326f7c4289a84535ac633b63b576e16f","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"defaultDatasourceName":null,"readPermissions":{"type":"AllOf","securityLevels":[]},"readOnly":false,"writePermissions":{"type":"AllOf","securityLevels":[]},"editPermissions":{"type":"AllOf","securityLevels":[]},"valuePersistence":"Database"}},"data":["config.json"],"attributes":{"uuid":"b8994ab8-8e74-4852-86f8-7dfbdcd1e8e0"},"metrics":{"tagCount":{"name":"tags.provider.default.tagCount","metric":{"value":1,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.default.status","result":{"healthy":true,"time":1793370709569,"message":"Running"}}}}],"metadata":{"total":3,"matching":3,"limit":-1,"offset":0}}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET tags/export?provider=CAP5&type=json

`ign --profile azure-win --json api call --method GET --path /data/api/v1/tags/export --query provider=CAP5 --query type=json ` @ 2026-10-30T14:31:49Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/tags/export",
    "result": {
      "status": 200,
      "data": {
  "name": "",
  "tagType": "Provider",
  "tags": [
    {
      "name": "G",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "G1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "RInst",
      "tagType": "UdtInstance",
      "tags": [
        {
          "name": "M1",
          "tagType": "AtomicTag"
        }
      ],
      "typeId": "A/B/T"
    },
    {
      "name": "F",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "F1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "_types_",
      "tagType": "Folder",
      "tags": [
        {
          "name": "T0",
          "tagType": "UdtType",
          "tags": [
            {
              "dataType": "Int4",
              "name": "M0",
              "tagType": "AtomicTag",
              "value": 0,
              "valueSource": "memory"
            }
          ]
        },
        {
          "name": "A",
          "tagType": "Folder",
          "tags": [
            {
              "name": "B",
              "tagType": "Folder",
              "tags": [
                {
                  "name": "T",
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
      ]
    },
    {
      "dataType": "Int4",
      "defaultValue": 1,
      "name": "R1",
      "tagType": "AtomicTag",
      "value": 1,
      "valueSource": "memory"
    }
  ]
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### probe: provider listing and every exported tag of CAP5

`printf %s $'provider=CAP5 listed=yes(core) export: F<Folder> F/F1=1 G<Folder> G/G1=1 R1=1 RInst<UdtInstance> RInst/M1<AtomicTag> _types_<Folder> _types_/A<Folder> _types_/A/B<Folder> _types_/A/B/T<UdtType> _types_/A/B/T/M1=1 _types_/T0<UdtType> _types_/T0/M0=0\n' ` @ 2026-10-30T14:31:51Z

~~~
[stdout]
provider=CAP5 listed=yes(core) export: F<Folder> F/F1=1 G<Folder> G/G1=1 R1=1 RInst<UdtInstance> RInst/M1<AtomicTag> _types_<Folder> _types_/A<Folder> _types_/A/B<Folder> _types_/A/B/T<UdtType> _types_/A/B/T/M1=1 _types_/T0<UdtType> _types_/T0/M0=0
[stderr]
[exit 0]
~~~

#### gateway log, WARN and above since the previous scan (ign logs --since 1793370663871; timestamp, logger, level, message)

`bash -c \"\$0\"\ --profile\ \"\$1\"\ --json\ logs\ --min-level\ warn\ --since\ \"\$2\"\ --limit\ 200\ \|\ jq\ -c\ \".data.items\[\]\ \|\ \{timestamp\,\ loggerName\,\ level\,\ message\}\" ign azure-win 1793370663871 ` @ 2026-10-30T14:31:51Z

~~~
[stdout]
{"timestamp":1793370666199,"loggerName":"com.inductiveautomation.ignition.gateway.tags.ep.StandardTagProviderExtensionPoint","level":"ERROR","message":"valuePersistence not found in Tag Provider: 'CAP5' profile, defaulting to Database"}
[stderr]
[exit 0]
~~~

### S2 — write: plain sftp puts into production/, then ONE scan for all rows

(a) Production-only provider CAP5B, the C11 shape: the CAP5 core files of all four tag types (as read back in S1) put to `production/ignition/<type>/CAP5B/`, with a fresh uuid in the tag-provider `resource.json`. (b) Overrides of CAP5 in `production/`: the root unit of `tag-definition/CAP5` with R1 1 -> 2; `tag-type-definition/CAP5/A/B/` with M1 1 -> 2; the control `tag-definition/CAP5/F/` with F1 1 -> 2. G is never written to `production/`. Edits are exact-lexeme perl substitutions on local copies.

#### the local files to put (relative paths, sha256 prefix): b/ is CAP5B, o/ the CAP5 overrides

`bash -c cd\ \"\$1\"\ \&\&\ find\ .\ -type\ f\ \|\ sed\ \"s\|\^\\./\|\|\"\ \|\ LC_ALL=C\ sort\ \|\ while\ IFS=\ read\ -r\ f\;\ do\ printf\ \"%s\ %s\\n\"\ \"\$\(shasum\ -a\ 256\ \"\$f\"\ \|\ cut\ -c1-16\)\"\ \"\$f\"\;\ done _ $TMP/local/s2 ` @ 2026-10-30T14:31:51Z

~~~
[stdout]
9e452e81f12d0d90 b/tag-definition/CAP5B/F/tags.json
8bf62a66d6a01555 b/tag-definition/CAP5B/F/unary-resource.json
eeb0bade74e1a8cf b/tag-definition/CAP5B/G/tags.json
8bf62a66d6a01555 b/tag-definition/CAP5B/G/unary-resource.json
ea4059c9add6e7de b/tag-definition/CAP5B/tags.json
32db14cb77d9df5e b/tag-definition/CAP5B/udts.json
e6ca38f4553c5c6f b/tag-definition/CAP5B/unary-resource.json
3681179aa816d786 b/tag-group/CAP5B/Default Historical/config.json
6171900ae01a3a5d b/tag-group/CAP5B/Default Historical/resource.json
2cbb0ed18730c557 b/tag-group/CAP5B/Default/config.json
6171900ae01a3a5d b/tag-group/CAP5B/Default/resource.json
302f76fcbc0cf1f1 b/tag-provider/CAP5B/config.json
96bb30c1720d43d5 b/tag-provider/CAP5B/resource.json
76181d0f09b8f132 b/tag-type-definition/CAP5B/A/B/udts.json
16d0f629e68eb94a b/tag-type-definition/CAP5B/A/B/unary-resource.json
4e76fe7dd0d0ee1d b/tag-type-definition/CAP5B/A/unary-resource.json
8cdbf390f7c0abc1 b/tag-type-definition/CAP5B/udts.json
16d0f629e68eb94a b/tag-type-definition/CAP5B/unary-resource.json
48018c641e5532ef o/tag-definition/CAP5/F/tags.json
8bf62a66d6a01555 o/tag-definition/CAP5/F/unary-resource.json
88d6d14a53ccb164 o/tag-definition/CAP5/tags.json
32db14cb77d9df5e o/tag-definition/CAP5/udts.json
e6ca38f4553c5c6f o/tag-definition/CAP5/unary-resource.json
fb8317ed67bb4c36 o/tag-type-definition/CAP5/A/B/udts.json
16d0f629e68eb94a o/tag-type-definition/CAP5/A/B/unary-resource.json
[stderr]
[exit 0]
~~~

#### production/ignition and its four tag-type directories (created when missing)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@52.154.75.231 ` @ 2026-10-30T14:31:52Z

~~~
[stdin]
-mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition"
-mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-provider"
-mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-group"
-mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition"
-mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition"
[stdout]
sftp> -mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition"
sftp> -mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-provider"
sftp> -mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-group"
sftp> -mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition"
sftp> -mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition"
[stderr]
[exit 0]
~~~

#### put CAP5B tag-provider into production/ (sftp)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@52.154.75.231 ` @ 2026-10-30T14:31:54Z

~~~
[stdin]
-mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-provider/CAP5B"
put "$TMP/local/s2/b/tag-provider/CAP5B/config.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-provider/CAP5B/config.json"
put "$TMP/local/s2/b/tag-provider/CAP5B/resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-provider/CAP5B/resource.json"
[stdout]
sftp> -mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-provider/CAP5B"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/b/tag-provider/CAP5B/config.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-provider/CAP5B/config.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/b/tag-provider/CAP5B/resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-provider/CAP5B/resource.json"
[stderr]
[exit 0]
~~~

#### put CAP5B tag-group into production/ (sftp)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@52.154.75.231 ` @ 2026-10-30T14:31:58Z

~~~
[stdin]
-mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-group/CAP5B"
-mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-group/CAP5B/Default"
-mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-group/CAP5B/Default Historical"
put "$TMP/local/s2/b/tag-group/CAP5B/Default Historical/config.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-group/CAP5B/Default Historical/config.json"
put "$TMP/local/s2/b/tag-group/CAP5B/Default Historical/resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-group/CAP5B/Default Historical/resource.json"
put "$TMP/local/s2/b/tag-group/CAP5B/Default/config.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-group/CAP5B/Default/config.json"
put "$TMP/local/s2/b/tag-group/CAP5B/Default/resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-group/CAP5B/Default/resource.json"
[stdout]
sftp> -mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-group/CAP5B"
sftp> -mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-group/CAP5B/Default"
sftp> -mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-group/CAP5B/Default Historical"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/b/tag-group/CAP5B/Default Historical/config.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-group/CAP5B/Default Historical/config.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/b/tag-group/CAP5B/Default Historical/resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-group/CAP5B/Default Historical/resource.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/b/tag-group/CAP5B/Default/config.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-group/CAP5B/Default/config.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/b/tag-group/CAP5B/Default/resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-group/CAP5B/Default/resource.json"
[stderr]
[exit 0]
~~~

#### put CAP5B tag-type-definition into production/ (sftp)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@52.154.75.231 ` @ 2026-10-30T14:32:01Z

~~~
[stdin]
-mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B"
-mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B/A"
-mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B/A/B"
put "$TMP/local/s2/b/tag-type-definition/CAP5B/A/B/udts.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B/A/B/udts.json"
put "$TMP/local/s2/b/tag-type-definition/CAP5B/A/B/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B/A/B/unary-resource.json"
put "$TMP/local/s2/b/tag-type-definition/CAP5B/A/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B/A/unary-resource.json"
put "$TMP/local/s2/b/tag-type-definition/CAP5B/udts.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B/udts.json"
put "$TMP/local/s2/b/tag-type-definition/CAP5B/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B/unary-resource.json"
[stdout]
sftp> -mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B"
sftp> -mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B/A"
sftp> -mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B/A/B"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/b/tag-type-definition/CAP5B/A/B/udts.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B/A/B/udts.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/b/tag-type-definition/CAP5B/A/B/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B/A/B/unary-resource.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/b/tag-type-definition/CAP5B/A/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B/A/unary-resource.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/b/tag-type-definition/CAP5B/udts.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B/udts.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/b/tag-type-definition/CAP5B/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B/unary-resource.json"
[stderr]
[exit 0]
~~~

#### put CAP5B tag-definition into production/ (sftp)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@52.154.75.231 ` @ 2026-10-30T14:32:04Z

~~~
[stdin]
-mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B"
-mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/F"
-mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/G"
put "$TMP/local/s2/b/tag-definition/CAP5B/F/tags.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/F/tags.json"
put "$TMP/local/s2/b/tag-definition/CAP5B/F/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/F/unary-resource.json"
put "$TMP/local/s2/b/tag-definition/CAP5B/G/tags.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/G/tags.json"
put "$TMP/local/s2/b/tag-definition/CAP5B/G/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/G/unary-resource.json"
put "$TMP/local/s2/b/tag-definition/CAP5B/tags.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/tags.json"
put "$TMP/local/s2/b/tag-definition/CAP5B/udts.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/udts.json"
put "$TMP/local/s2/b/tag-definition/CAP5B/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/unary-resource.json"
[stdout]
sftp> -mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B"
sftp> -mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/F"
sftp> -mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/G"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/b/tag-definition/CAP5B/F/tags.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/F/tags.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/b/tag-definition/CAP5B/F/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/F/unary-resource.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/b/tag-definition/CAP5B/G/tags.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/G/tags.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/b/tag-definition/CAP5B/G/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/G/unary-resource.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/b/tag-definition/CAP5B/tags.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/tags.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/b/tag-definition/CAP5B/udts.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/udts.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/b/tag-definition/CAP5B/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/unary-resource.json"
[stderr]
[exit 0]
~~~

#### put the CAP5 tag-definition overrides (root unit and F) into production/ (sftp)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@52.154.75.231 ` @ 2026-10-30T14:32:08Z

~~~
[stdin]
-mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5"
-mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/F"
put "$TMP/local/s2/o/tag-definition/CAP5/F/tags.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/F/tags.json"
put "$TMP/local/s2/o/tag-definition/CAP5/F/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/F/unary-resource.json"
put "$TMP/local/s2/o/tag-definition/CAP5/tags.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/tags.json"
put "$TMP/local/s2/o/tag-definition/CAP5/udts.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/udts.json"
put "$TMP/local/s2/o/tag-definition/CAP5/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/unary-resource.json"
[stdout]
sftp> -mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5"
sftp> -mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/F"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/o/tag-definition/CAP5/F/tags.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/F/tags.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/o/tag-definition/CAP5/F/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/F/unary-resource.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/o/tag-definition/CAP5/tags.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/tags.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/o/tag-definition/CAP5/udts.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/udts.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/o/tag-definition/CAP5/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/unary-resource.json"
[stderr]
[exit 0]
~~~

#### put the CAP5 tag-type-definition override (A/B) into production/ (sftp)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@52.154.75.231 ` @ 2026-10-30T14:32:11Z

~~~
[stdin]
-mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5"
-mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5/A"
-mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5/A/B"
put "$TMP/local/s2/o/tag-type-definition/CAP5/A/B/udts.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5/A/B/udts.json"
put "$TMP/local/s2/o/tag-type-definition/CAP5/A/B/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5/A/B/unary-resource.json"
[stdout]
sftp> -mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5"
sftp> -mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5/A"
sftp> -mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5/A/B"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/o/tag-type-definition/CAP5/A/B/udts.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5/A/B/udts.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/o/tag-type-definition/CAP5/A/B/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5/A/B/unary-resource.json"
[stderr]
[exit 0]
~~~

#### absolute NTFS path length of every file below production\ignition\tag-type-definition (CAP5, CAP5B)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@52.154.75.231 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIA...\[1110\ chars\] ` @ 2026-10-30T14:32:14Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; $r=Join-Path 'C:\Program Files\Inductive Automation\Ignition\data\config\resources' 'production\ignition\tag-type-definition'; $m=0; Get-ChildItem -LiteralPath $r -Recurse -File | ForEach-Object { if($_.FullName.Length -gt $m){$m=$_.FullName.Length}; '{0} {1}' -f $_.FullName.Length, $_.FullName }; 'max={0} limit=240' -f $m
[stdout]
127 C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\ignition\tag-type-definition\CAP5\A\B\udts.json
137 C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\ignition\tag-type-definition\CAP5\A\B\unary-resource.json
124 C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\ignition\tag-type-definition\CAP5B\udts.json
134 C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\ignition\tag-type-definition\CAP5B\unary-resource.json
136 C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\ignition\tag-type-definition\CAP5B\A\unary-resource.json
128 C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\ignition\tag-type-definition\CAP5B\A\B\udts.json
138 C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\ignition\tag-type-definition\CAP5B\A\B\unary-resource.json
max=138 limit=240
[8 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### POST scan/config

`ign --profile azure-win --json api call --method POST --path /data/api/v1/scan/config ` @ 2026-10-30T14:32:16Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "POST",
    "path": "/data/api/v1/scan/config",
    "result": {
      "status": 200,
      "data": {
  "scanActive": true,
  "lastScanTimestamp": 1793370478874,
  "lastScanDuration": 634
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET scan/config (after completion)

`ign --profile azure-win --json api call --method GET --path /data/api/v1/scan/config ` @ 2026-10-30T14:32:18Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/scan/config",
    "result": {
      "status": 200,
      "data": {
  "scanActive": false,
  "lastScanTimestamp": 1793370737131,
  "lastScanDuration": 592
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET resources/list/ignition/tag-provider

`ign --profile azure-win --json api call --method GET --path /data/api/v1/resources/list/ignition/tag-provider --query limit=-1 ` @ 2026-10-30T14:32:18Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/resources/list/ignition/tag-provider",
    "result": {
      "status": 200,
      "data": {"items":[{"type":"ignition/tag-provider","name":"CAP5","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"9126111f852618f0a357bae275c18bdedf0f4c8b9e45e3d1b314463543f4afd2","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"c93fe62e-fd8a-487c-9525-e2b8ff3f4898","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP5.tagCount","metric":{"value":14,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP5.status","result":{"healthy":true,"time":1793370738583,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"CAP5B","description":null,"enabled":true,"version":1,"collection":"production","collections":["production"],"signature":"2b621a66b394529d8c2340b5edc2ff7b3a58699c378974d68a9c2224ac264363","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"5265e98d-1f3e-4f7c-a375-cd7e12eee0cb","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP5B.tagCount","metric":{"value":14,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP5B.status","result":{"healthy":true,"time":1793370738584,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"System","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"7d25b29271a47ff52b6d6712ccecbcd5964ea2baefe25c6be850d07efe39d1d5","config":{"profile":{"type":"MANAGED","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"nonUseCount":0}},"data":["config.json"],"attributes":{"uuid":"6219db0d-9b8e-4f3a-b352-c7d8cc1e44ff"},"metrics":{"tagCount":{"name":"tags.provider.System.tagCount","metric":{"value":39,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.System.status","result":{"healthy":true,"time":1793370738585,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"default","description":"Default tag provider","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"c75b0f1fed92b9087d20984eddc79595326f7c4289a84535ac633b63b576e16f","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"defaultDatasourceName":null,"readPermissions":{"type":"AllOf","securityLevels":[]},"readOnly":false,"writePermissions":{"type":"AllOf","securityLevels":[]},"editPermissions":{"type":"AllOf","securityLevels":[]},"valuePersistence":"Database"}},"data":["config.json"],"attributes":{"uuid":"b8994ab8-8e74-4852-86f8-7dfbdcd1e8e0"},"metrics":{"tagCount":{"name":"tags.provider.default.tagCount","metric":{"value":1,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.default.status","result":{"healthy":true,"time":1793370738586,"message":"Running"}}}}],"metadata":{"total":4,"matching":4,"limit":-1,"offset":0}}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET tags/export?provider=CAP5&type=json

`ign --profile azure-win --json api call --method GET --path /data/api/v1/tags/export --query provider=CAP5 --query type=json ` @ 2026-10-30T14:32:18Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/tags/export",
    "result": {
      "status": 200,
      "data": {
  "name": "",
  "tagType": "Provider",
  "tags": [
    {
      "name": "G",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "G1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "RInst",
      "tagType": "UdtInstance",
      "tags": [
        {
          "name": "M1",
          "tagType": "AtomicTag"
        }
      ],
      "typeId": "A/B/T"
    },
    {
      "name": "F",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 2,
          "name": "F1",
          "tagType": "AtomicTag",
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "_types_",
      "tagType": "Folder",
      "tags": [
        {
          "name": "T0",
          "tagType": "UdtType",
          "tags": [
            {
              "dataType": "Int4",
              "name": "M0",
              "tagType": "AtomicTag",
              "value": 0,
              "valueSource": "memory"
            }
          ]
        },
        {
          "name": "A",
          "tagType": "Folder",
          "tags": [
            {
              "name": "B",
              "tagType": "Folder",
              "tags": [
                {
                  "name": "T",
                  "tagType": "UdtType",
                  "tags": [
                    {
                      "dataType": "Int4",
                      "name": "M1",
                      "tagType": "AtomicTag",
                      "value": 2,
                      "valueSource": "memory"
                    }
                  ]
                }
              ]
            }
          ]
        }
      ]
    },
    {
      "dataType": "Int4",
      "defaultValue": 2,
      "name": "R1",
      "tagType": "AtomicTag",
      "valueSource": "memory"
    }
  ]
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET tags/export?provider=CAP5B&type=json

`ign --profile azure-win --json api call --method GET --path /data/api/v1/tags/export --query provider=CAP5B --query type=json ` @ 2026-10-30T14:32:18Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/tags/export",
    "result": {
      "status": 200,
      "data": {
  "name": "",
  "tagType": "Provider",
  "tags": [
    {
      "name": "F",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "F1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "dataType": "Int4",
      "defaultValue": 1,
      "name": "R1",
      "tagType": "AtomicTag",
      "value": 1,
      "valueSource": "memory"
    },
    {
      "name": "G",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "G1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "RInst",
      "tagType": "UdtInstance",
      "tags": [
        {
          "name": "M1",
          "tagType": "AtomicTag"
        }
      ],
      "typeId": "A/B/T"
    },
    {
      "name": "_types_",
      "tagType": "Folder",
      "tags": [
        {
          "name": "T0",
          "tagType": "UdtType",
          "tags": [
            {
              "dataType": "Int4",
              "name": "M0",
              "tagType": "AtomicTag",
              "value": 0,
              "valueSource": "memory"
            }
          ]
        },
        {
          "name": "A",
          "tagType": "Folder",
          "tags": [
            {
              "name": "B",
              "tagType": "Folder",
              "tags": [
                {
                  "name": "T",
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
      ]
    }
  ]
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### probe: provider listing and every exported tag of CAP5 CAP5B

`printf %s provider=CAP5\ listed=yes\(core\)\ export:\ F\<Folder\>\ F/F1=2\ G\<Folder\>\ G/G1=1\ R1=2\ RInst\<UdtInstance\>\ RInst/M1\<AtomicTag\>\ _types_\<Folder\>\ _types_/A\<Folder\>\ _types_/A...\[505\ chars\] ` @ 2026-10-30T14:32:20Z

~~~
[stdout]
provider=CAP5 listed=yes(core) export: F<Folder> F/F1=2 G<Folder> G/G1=1 R1=2 RInst<UdtInstance> RInst/M1<AtomicTag> _types_<Folder> _types_/A<Folder> _types_/A/B<Folder> _types_/A/B/T<UdtType> _types_/A/B/T/M1=2 _types_/T0<UdtType> _types_/T0/M0=0
provider=CAP5B listed=yes(production) export: F<Folder> F/F1=1 G<Folder> G/G1=1 R1=1 RInst<UdtInstance> RInst/M1<AtomicTag> _types_<Folder> _types_/A<Folder> _types_/A/B<Folder> _types_/A/B/T<UdtType> _types_/A/B/T/M1=1 _types_/T0<UdtType> _types_/T0/M0=0
[stderr]
[exit 0]
~~~

#### S2 rows CAP5B (production-only provider, C11 shape): CD5a R1 RInst; CD5b T (M1); control F1; standard G1 T0

`bash -c \"\$0\"\ --profile\ \"\$1\"\ --json\ api\ call\ --method\ GET\ --path\ /data/api/v1/tags/export\ --query\ \"provider=\$2\"\ --query\ type=json\ \|\ python3\ -I\ \"\$3\"\ check\ \"\$\{@:4\}\" ign azure-win CAP5B $TMP/cd5flat.py R1=1 RInst F/F1=1 G/G1=1 _types_/T0 _types_/A/B/T/M1=1 ` @ 2026-10-30T14:32:20Z

~~~
[stdout]
ok   R1=1 (observed =1)
ok   RInst (observed <UdtInstance>)
ok   F/F1=1 (observed =1)
ok   G/G1=1 (observed =1)
ok   _types_/T0 (observed <UdtType>)
ok   _types_/A/B/T/M1=1 (observed =1)
[stderr]
[exit 0]
~~~

#### S2 rows CAP5 (production overrides): CD5a R1=2 and G1 not hidden; CD5b M1=2 and T0 not hidden; control F1=2

`bash -c \"\$0\"\ --profile\ \"\$1\"\ --json\ api\ call\ --method\ GET\ --path\ /data/api/v1/tags/export\ --query\ \"provider=\$2\"\ --query\ type=json\ \|\ python3\ -I\ \"\$3\"\ check\ \"\$\{@:4\}\" ign azure-win CAP5 $TMP/cd5flat.py R1=2 G/G1=1 _types_/A/B/T/M1=2 _types_/T0 F/F1=2 RInst ` @ 2026-10-30T14:32:20Z

~~~
[stdout]
ok   R1=2 (observed =2)
ok   G/G1=1 (observed =1)
ok   _types_/A/B/T/M1=2 (observed =2)
ok   _types_/T0 (observed <UdtType>)
ok   F/F1=2 (observed =2)
ok   RInst (observed <UdtInstance>)
[stderr]
[exit 0]
~~~

#### S2 row: CAP5B is listed as a tag provider

`printf %s\ listed:\ %s\\n CAP5B yes\(production\) ` @ 2026-10-30T14:32:21Z

~~~
[stdout]
CAP5B listed: yes(production)
[stderr]
[exit 0]
~~~

#### gateway log, WARN and above since the previous scan (ign logs --since 1793370734335; timestamp, logger, level, message)

`bash -c \"\$0\"\ --profile\ \"\$1\"\ --json\ logs\ --min-level\ warn\ --since\ \"\$2\"\ --limit\ 200\ \|\ jq\ -c\ \".data.items\[\]\ \|\ \{timestamp\,\ loggerName\,\ level\,\ message\}\" ign azure-win 1793370734335 ` @ 2026-10-30T14:32:21Z

~~~
[stdout]
{"timestamp":1793370737142,"loggerName":"com.inductiveautomation.ignition.gateway.tags.ep.StandardTagProviderExtensionPoint","level":"ERROR","message":"valuePersistence not found in Tag Provider: 'CAP5B' profile, defaulting to Database"}
[stderr]
[exit 0]
~~~

Script reading (S2): every row matched its expectation after ONE scan; no ladder needed.

### S3 — change: one member value edited in each production/ copy (sftp put over the file), then ONE scan

CAP5B R1 1 -> 3, M1 1 -> 3, F1 1 -> 3 (control); CAP5 overrides R1 2 -> 3, M1 2 -> 3, F1 2 -> 3 (control).

#### put the six edited files over their production/ copies

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@52.154.75.231 ` @ 2026-10-30T14:32:21Z

~~~
[stdin]
put "$TMP/local/s2/b/tag-definition/CAP5B/tags.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/tags.json"
put "$TMP/local/s2/b/tag-type-definition/CAP5B/A/B/udts.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B/A/B/udts.json"
put "$TMP/local/s2/b/tag-definition/CAP5B/F/tags.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/F/tags.json"
put "$TMP/local/s2/o/tag-definition/CAP5/tags.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/tags.json"
put "$TMP/local/s2/o/tag-type-definition/CAP5/A/B/udts.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5/A/B/udts.json"
put "$TMP/local/s2/o/tag-definition/CAP5/F/tags.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/F/tags.json"
[stdout]
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/b/tag-definition/CAP5B/tags.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/tags.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/b/tag-type-definition/CAP5B/A/B/udts.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B/A/B/udts.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/b/tag-definition/CAP5B/F/tags.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/F/tags.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/o/tag-definition/CAP5/tags.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/tags.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/o/tag-type-definition/CAP5/A/B/udts.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5/A/B/udts.json"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s2/o/tag-definition/CAP5/F/tags.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/F/tags.json"
[stderr]
[exit 0]
~~~

#### POST scan/config

`ign --profile azure-win --json api call --method POST --path /data/api/v1/scan/config ` @ 2026-10-30T14:32:25Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "POST",
    "path": "/data/api/v1/scan/config",
    "result": {
      "status": 200,
      "data": {
  "scanActive": true,
  "lastScanTimestamp": 1793370737131,
  "lastScanDuration": 592
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET scan/config (after completion)

`ign --profile azure-win --json api call --method GET --path /data/api/v1/scan/config ` @ 2026-10-30T14:32:27Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/scan/config",
    "result": {
      "status": 200,
      "data": {
  "scanActive": false,
  "lastScanTimestamp": 1793370746389,
  "lastScanDuration": 557
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET resources/list/ignition/tag-provider

`ign --profile azure-win --json api call --method GET --path /data/api/v1/resources/list/ignition/tag-provider --query limit=-1 ` @ 2026-10-30T14:32:27Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/resources/list/ignition/tag-provider",
    "result": {
      "status": 200,
      "data": {"items":[{"type":"ignition/tag-provider","name":"CAP5","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"9126111f852618f0a357bae275c18bdedf0f4c8b9e45e3d1b314463543f4afd2","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"c93fe62e-fd8a-487c-9525-e2b8ff3f4898","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP5.tagCount","metric":{"value":14,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP5.status","result":{"healthy":true,"time":1793370747815,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"CAP5B","description":null,"enabled":true,"version":1,"collection":"production","collections":["production"],"signature":"2b621a66b394529d8c2340b5edc2ff7b3a58699c378974d68a9c2224ac264363","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"5265e98d-1f3e-4f7c-a375-cd7e12eee0cb","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP5B.tagCount","metric":{"value":14,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP5B.status","result":{"healthy":true,"time":1793370747816,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"System","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"7d25b29271a47ff52b6d6712ccecbcd5964ea2baefe25c6be850d07efe39d1d5","config":{"profile":{"type":"MANAGED","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"nonUseCount":0}},"data":["config.json"],"attributes":{"uuid":"6219db0d-9b8e-4f3a-b352-c7d8cc1e44ff"},"metrics":{"tagCount":{"name":"tags.provider.System.tagCount","metric":{"value":39,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.System.status","result":{"healthy":true,"time":1793370747817,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"default","description":"Default tag provider","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"c75b0f1fed92b9087d20984eddc79595326f7c4289a84535ac633b63b576e16f","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"defaultDatasourceName":null,"readPermissions":{"type":"AllOf","securityLevels":[]},"readOnly":false,"writePermissions":{"type":"AllOf","securityLevels":[]},"editPermissions":{"type":"AllOf","securityLevels":[]},"valuePersistence":"Database"}},"data":["config.json"],"attributes":{"uuid":"b8994ab8-8e74-4852-86f8-7dfbdcd1e8e0"},"metrics":{"tagCount":{"name":"tags.provider.default.tagCount","metric":{"value":1,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.default.status","result":{"healthy":true,"time":1793370747820,"message":"Running"}}}}],"metadata":{"total":4,"matching":4,"limit":-1,"offset":0}}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET tags/export?provider=CAP5&type=json

`ign --profile azure-win --json api call --method GET --path /data/api/v1/tags/export --query provider=CAP5 --query type=json ` @ 2026-10-30T14:32:27Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/tags/export",
    "result": {
      "status": 200,
      "data": {
  "name": "",
  "tagType": "Provider",
  "tags": [
    {
      "name": "G",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "G1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "RInst",
      "tagType": "UdtInstance",
      "tags": [
        {
          "name": "M1",
          "tagType": "AtomicTag"
        }
      ],
      "typeId": "A/B/T"
    },
    {
      "name": "F",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 3,
          "name": "F1",
          "tagType": "AtomicTag",
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "_types_",
      "tagType": "Folder",
      "tags": [
        {
          "name": "T0",
          "tagType": "UdtType",
          "tags": [
            {
              "dataType": "Int4",
              "name": "M0",
              "tagType": "AtomicTag",
              "value": 0,
              "valueSource": "memory"
            }
          ]
        },
        {
          "name": "A",
          "tagType": "Folder",
          "tags": [
            {
              "name": "B",
              "tagType": "Folder",
              "tags": [
                {
                  "name": "T",
                  "tagType": "UdtType",
                  "tags": [
                    {
                      "dataType": "Int4",
                      "name": "M1",
                      "tagType": "AtomicTag",
                      "value": 3,
                      "valueSource": "memory"
                    }
                  ]
                }
              ]
            }
          ]
        }
      ]
    },
    {
      "dataType": "Int4",
      "defaultValue": 3,
      "name": "R1",
      "tagType": "AtomicTag",
      "valueSource": "memory"
    }
  ]
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET tags/export?provider=CAP5B&type=json

`ign --profile azure-win --json api call --method GET --path /data/api/v1/tags/export --query provider=CAP5B --query type=json ` @ 2026-10-30T14:32:28Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/tags/export",
    "result": {
      "status": 200,
      "data": {
  "name": "",
  "tagType": "Provider",
  "tags": [
    {
      "name": "F",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 3,
          "name": "F1",
          "tagType": "AtomicTag",
          "valueSource": "memory"
        }
      ]
    },
    {
      "dataType": "Int4",
      "defaultValue": 3,
      "name": "R1",
      "tagType": "AtomicTag",
      "valueSource": "memory"
    },
    {
      "name": "G",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "G1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "RInst",
      "tagType": "UdtInstance",
      "tags": [
        {
          "name": "M1",
          "tagType": "AtomicTag"
        }
      ],
      "typeId": "A/B/T"
    },
    {
      "name": "_types_",
      "tagType": "Folder",
      "tags": [
        {
          "name": "T0",
          "tagType": "UdtType",
          "tags": [
            {
              "dataType": "Int4",
              "name": "M0",
              "tagType": "AtomicTag",
              "value": 0,
              "valueSource": "memory"
            }
          ]
        },
        {
          "name": "A",
          "tagType": "Folder",
          "tags": [
            {
              "name": "B",
              "tagType": "Folder",
              "tags": [
                {
                  "name": "T",
                  "tagType": "UdtType",
                  "tags": [
                    {
                      "dataType": "Int4",
                      "name": "M1",
                      "tagType": "AtomicTag",
                      "value": 3,
                      "valueSource": "memory"
                    }
                  ]
                }
              ]
            }
          ]
        }
      ]
    }
  ]
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### probe: provider listing and every exported tag of CAP5 CAP5B

`printf %s provider=CAP5\ listed=yes\(core\)\ export:\ F\<Folder\>\ F/F1=3\ G\<Folder\>\ G/G1=1\ R1=3\ RInst\<UdtInstance\>\ RInst/M1\<AtomicTag\>\ _types_\<Folder\>\ _types_/A\<Folder\>\ _types_/A...\[505\ chars\] ` @ 2026-10-30T14:32:29Z

~~~
[stdout]
provider=CAP5 listed=yes(core) export: F<Folder> F/F1=3 G<Folder> G/G1=1 R1=3 RInst<UdtInstance> RInst/M1<AtomicTag> _types_<Folder> _types_/A<Folder> _types_/A/B<Folder> _types_/A/B/T<UdtType> _types_/A/B/T/M1=3 _types_/T0<UdtType> _types_/T0/M0=0
provider=CAP5B listed=yes(production) export: F<Folder> F/F1=3 G<Folder> G/G1=1 R1=3 RInst<UdtInstance> RInst/M1<AtomicTag> _types_<Folder> _types_/A<Folder> _types_/A/B<Folder> _types_/A/B/T<UdtType> _types_/A/B/T/M1=3 _types_/T0<UdtType> _types_/T0/M0=0
[stderr]
[exit 0]
~~~

#### S3 rows CAP5B: CD5a R1=3; CD5b M1=3; control F1=3

`bash -c \"\$0\"\ --profile\ \"\$1\"\ --json\ api\ call\ --method\ GET\ --path\ /data/api/v1/tags/export\ --query\ \"provider=\$2\"\ --query\ type=json\ \|\ python3\ -I\ \"\$3\"\ check\ \"\$\{@:4\}\" ign azure-win CAP5B $TMP/cd5flat.py R1=3 _types_/A/B/T/M1=3 F/F1=3 ` @ 2026-10-30T14:32:29Z

~~~
[stdout]
ok   R1=3 (observed =3)
ok   _types_/A/B/T/M1=3 (observed =3)
ok   F/F1=3 (observed =3)
[stderr]
[exit 0]
~~~

#### S3 rows CAP5: CD5a R1=3; CD5b M1=3; control F1=3

`bash -c \"\$0\"\ --profile\ \"\$1\"\ --json\ api\ call\ --method\ GET\ --path\ /data/api/v1/tags/export\ --query\ \"provider=\$2\"\ --query\ type=json\ \|\ python3\ -I\ \"\$3\"\ check\ \"\$\{@:4\}\" ign azure-win CAP5 $TMP/cd5flat.py R1=3 _types_/A/B/T/M1=3 F/F1=3 ` @ 2026-10-30T14:32:29Z

~~~
[stdout]
ok   R1=3 (observed =3)
ok   _types_/A/B/T/M1=3 (observed =3)
ok   F/F1=3 (observed =3)
[stderr]
[exit 0]
~~~

#### gateway log, WARN and above since the previous scan (ign logs --since 1793370743617; timestamp, logger, level, message)

`bash -c \"\$0\"\ --profile\ \"\$1\"\ --json\ logs\ --min-level\ warn\ --since\ \"\$2\"\ --limit\ 200\ \|\ jq\ -c\ \".data.items\[\]\ \|\ \{timestamp\,\ loggerName\,\ level\,\ message\}\" ign azure-win 1793370743617 ` @ 2026-10-30T14:32:29Z

~~~
[stdout]
[stderr]
[exit 0]
~~~

Script reading (S3): every row matched its expectation after ONE scan; no ladder needed.

### S4 — delete: files removed from production/ over sftp (markers set to the gateway files: [] bytes), then ONE scan

CAP5B: root `tags.json` and `udts.json` removed, root marker replaced with the gateway `files: []` bytes (CAP21 fixture); `tag-type-definition/CAP5B/A/B/` removed whole; `tag-definition/CAP5B/F/` removed whole (control). CAP5: production root `tags.json` and `udts.json` removed and the root marker set to `files: []`; the production `A/B/` override removed whole; production `F/tags.json` removed and the F marker set to `files: []` (control).

#### S4 sftp batch: removals and marker replacements

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@52.154.75.231 ` @ 2026-10-30T14:32:30Z

~~~
[stdin]
rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/tags.json"
rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/udts.json"
put "$HOME/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/deploy/captured/ignition/tag-definition/CAP21/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/unary-resource.json"
rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B/A/B/udts.json"
rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B/A/B/unary-resource.json"
rmdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B/A/B"
rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/F/tags.json"
rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/F/unary-resource.json"
rmdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/F"
rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/tags.json"
rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/udts.json"
put "$HOME/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/deploy/captured/ignition/tag-definition/CAP21/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/unary-resource.json"
rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5/A/B/udts.json"
rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5/A/B/unary-resource.json"
rmdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5/A/B"
rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/F/tags.json"
put "$HOME/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/deploy/captured/ignition/tag-definition/CAP21/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/F/unary-resource.json"
[stdout]
sftp> rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/tags.json"
sftp> rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/udts.json"
sftp> put "/Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/deploy/captured/ignition/tag-definition/CAP21/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/unary-resource.json"
sftp> rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B/A/B/udts.json"
sftp> rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B/A/B/unary-resource.json"
sftp> rmdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5B/A/B"
sftp> rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/F/tags.json"
sftp> rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/F/unary-resource.json"
sftp> rmdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/F"
sftp> rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/tags.json"
sftp> rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/udts.json"
sftp> put "/Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/deploy/captured/ignition/tag-definition/CAP21/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/unary-resource.json"
sftp> rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5/A/B/udts.json"
sftp> rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5/A/B/unary-resource.json"
sftp> rmdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-type-definition/CAP5/A/B"
sftp> rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/F/tags.json"
sftp> put "/Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/tests/fixtures/deploy/captured/ignition/tag-definition/CAP21/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5/F/unary-resource.json"
[stderr]
[exit 0]
~~~

#### production\ignition after the removals (every file, relative)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@52.154.75.231 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIA...\[830\ chars\] ` @ 2026-10-30T14:32:34Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; $r=Join-Path 'C:\Program Files\Inductive Automation\Ignition\data\config\resources' 'production\ignition'; Get-ChildItem -LiteralPath $r -Recurse -File | ForEach-Object { $_.FullName.Substring($r.Length) } | Sort-Object
[stdout]
\tag-definition\CAP5\F\unary-resource.json
\tag-definition\CAP5\unary-resource.json
\tag-definition\CAP5B\G\tags.json
\tag-definition\CAP5B\G\unary-resource.json
\tag-definition\CAP5B\unary-resource.json
\tag-group\CAP5B\Default Historical\config.json
\tag-group\CAP5B\Default Historical\resource.json
\tag-group\CAP5B\Default\config.json
\tag-group\CAP5B\Default\resource.json
\tag-provider\CAP5B\config.json
\tag-provider\CAP5B\resource.json
\tag-type-definition\CAP5B\A\unary-resource.json
\tag-type-definition\CAP5B\udts.json
\tag-type-definition\CAP5B\unary-resource.json
[14 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### POST scan/config

`ign --profile azure-win --json api call --method POST --path /data/api/v1/scan/config ` @ 2026-10-30T14:32:36Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "POST",
    "path": "/data/api/v1/scan/config",
    "result": {
      "status": 200,
      "data": {
  "scanActive": true,
  "lastScanTimestamp": 1793370746389,
  "lastScanDuration": 557
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET scan/config (after completion)

`ign --profile azure-win --json api call --method GET --path /data/api/v1/scan/config ` @ 2026-10-30T14:32:38Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/scan/config",
    "result": {
      "status": 200,
      "data": {
  "scanActive": false,
  "lastScanTimestamp": 1793370757256,
  "lastScanDuration": 498
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET resources/list/ignition/tag-provider

`ign --profile azure-win --json api call --method GET --path /data/api/v1/resources/list/ignition/tag-provider --query limit=-1 ` @ 2026-10-30T14:32:38Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/resources/list/ignition/tag-provider",
    "result": {
      "status": 200,
      "data": {"items":[{"type":"ignition/tag-provider","name":"CAP5","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"9126111f852618f0a357bae275c18bdedf0f4c8b9e45e3d1b314463543f4afd2","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"c93fe62e-fd8a-487c-9525-e2b8ff3f4898","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP5.tagCount","metric":{"value":10,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP5.status","result":{"healthy":true,"time":1793370758663,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"CAP5B","description":null,"enabled":true,"version":1,"collection":"production","collections":["production"],"signature":"2b621a66b394529d8c2340b5edc2ff7b3a58699c378974d68a9c2224ac264363","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"5265e98d-1f3e-4f7c-a375-cd7e12eee0cb","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP5B.tagCount","metric":{"value":6,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP5B.status","result":{"healthy":true,"time":1793370758665,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"System","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"7d25b29271a47ff52b6d6712ccecbcd5964ea2baefe25c6be850d07efe39d1d5","config":{"profile":{"type":"MANAGED","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"nonUseCount":0}},"data":["config.json"],"attributes":{"uuid":"6219db0d-9b8e-4f3a-b352-c7d8cc1e44ff"},"metrics":{"tagCount":{"name":"tags.provider.System.tagCount","metric":{"value":39,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.System.status","result":{"healthy":true,"time":1793370758666,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"default","description":"Default tag provider","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"c75b0f1fed92b9087d20984eddc79595326f7c4289a84535ac633b63b576e16f","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"defaultDatasourceName":null,"readPermissions":{"type":"AllOf","securityLevels":[]},"readOnly":false,"writePermissions":{"type":"AllOf","securityLevels":[]},"editPermissions":{"type":"AllOf","securityLevels":[]},"valuePersistence":"Database"}},"data":["config.json"],"attributes":{"uuid":"b8994ab8-8e74-4852-86f8-7dfbdcd1e8e0"},"metrics":{"tagCount":{"name":"tags.provider.default.tagCount","metric":{"value":1,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.default.status","result":{"healthy":true,"time":1793370758666,"message":"Running"}}}}],"metadata":{"total":4,"matching":4,"limit":-1,"offset":0}}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET tags/export?provider=CAP5&type=json

`ign --profile azure-win --json api call --method GET --path /data/api/v1/tags/export --query provider=CAP5 --query type=json ` @ 2026-10-30T14:32:38Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/tags/export",
    "result": {
      "status": 200,
      "data": {
  "name": "",
  "tagType": "Provider",
  "tags": [
    {
      "name": "G",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "G1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "F",
      "tagType": "Folder"
    },
    {
      "name": "_types_",
      "tagType": "Folder",
      "tags": [
        {
          "name": "T0",
          "tagType": "UdtType",
          "tags": [
            {
              "dataType": "Int4",
              "name": "M0",
              "tagType": "AtomicTag",
              "value": 0,
              "valueSource": "memory"
            }
          ]
        },
        {
          "name": "A",
          "tagType": "Folder",
          "tags": [
            {
              "name": "B",
              "tagType": "Folder",
              "tags": [
                {
                  "name": "T",
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
      ]
    }
  ]
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET tags/export?provider=CAP5B&type=json

`ign --profile azure-win --json api call --method GET --path /data/api/v1/tags/export --query provider=CAP5B --query type=json ` @ 2026-10-30T14:32:38Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/tags/export",
    "result": {
      "status": 200,
      "data": {
  "name": "",
  "tagType": "Provider",
  "tags": [
    {
      "name": "G",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "G1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "_types_",
      "tagType": "Folder",
      "tags": [
        {
          "name": "T0",
          "tagType": "UdtType",
          "tags": [
            {
              "dataType": "Int4",
              "name": "M0",
              "tagType": "AtomicTag",
              "value": 0,
              "valueSource": "memory"
            }
          ]
        },
        {
          "name": "A",
          "tagType": "Folder"
        }
      ]
    }
  ]
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### probe: provider listing and every exported tag of CAP5 CAP5B

`printf %s $'provider=CAP5 listed=yes(core) export: F<Folder> G<Folder> G/G1=1 _types_<Folder> _types_/A<Folder> _types_/A/B<Folder> _types_/A/B/T<UdtType> _types_/A/B/T/M1=1 _types_/T0<UdtType> _types_/T0/M0=0\nprovider=CAP5B listed=yes(production) export: G<Folder> G/G1=1 _types_<Folder> _types_/A<Folder> _types_/T0<UdtType> _types_/T0/M0=0\n' ` @ 2026-10-30T14:32:40Z

~~~
[stdout]
provider=CAP5 listed=yes(core) export: F<Folder> G<Folder> G/G1=1 _types_<Folder> _types_/A<Folder> _types_/A/B<Folder> _types_/A/B/T<UdtType> _types_/A/B/T/M1=1 _types_/T0<UdtType> _types_/T0/M0=0
provider=CAP5B listed=yes(production) export: G<Folder> G/G1=1 _types_<Folder> _types_/A<Folder> _types_/T0<UdtType> _types_/T0/M0=0
[stderr]
[exit 0]
~~~

#### S4 rows CAP5B: CD5a R1 RInst gone; CD5b T gone; control F1 gone

`bash -c \"\$0\"\ --profile\ \"\$1\"\ --json\ api\ call\ --method\ GET\ --path\ /data/api/v1/tags/export\ --query\ \"provider=\$2\"\ --query\ type=json\ \|\ python3\ -I\ \"\$3\"\ check\ \"\$\{@:4\}\" ign azure-win CAP5B $TMP/cd5flat.py \!R1 \!RInst \!_types_/A/B/T \!F/F1 ` @ 2026-10-30T14:32:40Z

~~~
[stdout]
ok   !R1 (observed absent)
ok   !RInst (observed absent)
ok   !_types_/A/B/T (observed absent)
ok   !F/F1 (observed absent)
[stderr]
[exit 0]
~~~

#### S4 row: CAP5B is still listed as a tag provider

`printf %s\ listed:\ %s\\n CAP5B yes\(production\) ` @ 2026-10-30T14:32:40Z

~~~
[stdout]
CAP5B listed: yes(production)
[stderr]
[exit 0]
~~~

#### S4 rows CAP5: CD5a R1 RInst hidden by the production root unit, G1 still present; control F1 hidden

`bash -c \"\$0\"\ --profile\ \"\$1\"\ --json\ api\ call\ --method\ GET\ --path\ /data/api/v1/tags/export\ --query\ \"provider=\$2\"\ --query\ type=json\ \|\ python3\ -I\ \"\$3\"\ check\ \"\$\{@:4\}\" ign azure-win CAP5 $TMP/cd5flat.py \!R1 \!RInst G/G1=1 \!F/F1 ` @ 2026-10-30T14:32:40Z

~~~
[stdout]
ok   !R1 (observed absent)
ok   !RInst (observed absent)
ok   G/G1=1 (observed =1)
ok   !F/F1 (observed absent)
[stderr]
[exit 0]
~~~

#### gateway log, WARN and above since the previous scan (ign logs --since 1793370754555; timestamp, logger, level, message)

`bash -c \"\$0\"\ --profile\ \"\$1\"\ --json\ logs\ --min-level\ warn\ --since\ \"\$2\"\ --limit\ 200\ \|\ jq\ -c\ \".data.items\[\]\ \|\ \{timestamp\,\ loggerName\,\ level\,\ message\}\" ign azure-win 1793370754555 ` @ 2026-10-30T14:32:41Z

~~~
[stdout]
[stderr]
[exit 0]
~~~

Script reading (S4): every row matched its expectation after ONE scan; no ladder needed.

#### S4 informational (not part of any verdict): with the production A/B unit removed, is T back to the core value M1=1?

`bash -c \"\$0\"\ --profile\ \"\$1\"\ --json\ api\ call\ --method\ GET\ --path\ /data/api/v1/tags/export\ --query\ \"provider=\$2\"\ --query\ type=json\ \|\ python3\ -I\ \"\$3\"\ check\ \"\$\{@:4\}\" ign azure-win CAP5 $TMP/cd5flat.py _types_/A/B/T/M1=1 ` @ 2026-10-30T14:32:41Z

~~~
[stdout]
ok   _types_/A/B/T/M1=1 (observed =1)
[stderr]
[exit 0]
~~~

### S5 — does the gateway honour unary-resource.json `files`? (OQ3, CD5c; on CAP5B)

(i) A root `tags.json` holding AtomicTag R9 (the S1 CAP5 root `tags.json` with the name lexeme R1 -> R9) put while the root marker still says `files: []`; ONE scan. Is R9 loaded?

#### S5(i) put tags.json (R9) beside the files: [] marker, then list the root

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@52.154.75.231 ` @ 2026-10-30T14:32:41Z

~~~
[stdin]
put "$TMP/local/s5/tags.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/tags.json"
@ls -la "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B"
[stdout]
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s5/tags.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/tags.json"
drwx******    1 -        -               0 Oct 30 14:32 G
-rw-******    1 -        -             132 Oct 30 14:32 tags.json
-rw-******    1 -        -             135 Oct 30 14:32 unary-resource.json
[stderr]
[exit 0]
~~~

#### POST scan/config

`ign --profile azure-win --json api call --method POST --path /data/api/v1/scan/config ` @ 2026-10-30T14:32:44Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "POST",
    "path": "/data/api/v1/scan/config",
    "result": {
      "status": 200,
      "data": {
  "scanActive": true,
  "lastScanTimestamp": 1793370757256,
  "lastScanDuration": 498
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET scan/config (after completion)

`ign --profile azure-win --json api call --method GET --path /data/api/v1/scan/config ` @ 2026-10-30T14:32:46Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/scan/config",
    "result": {
      "status": 200,
      "data": {
  "scanActive": false,
  "lastScanTimestamp": 1793370765095,
  "lastScanDuration": 488
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET resources/list/ignition/tag-provider

`ign --profile azure-win --json api call --method GET --path /data/api/v1/resources/list/ignition/tag-provider --query limit=-1 ` @ 2026-10-30T14:32:46Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/resources/list/ignition/tag-provider",
    "result": {
      "status": 200,
      "data": {"items":[{"type":"ignition/tag-provider","name":"CAP5","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"9126111f852618f0a357bae275c18bdedf0f4c8b9e45e3d1b314463543f4afd2","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"c93fe62e-fd8a-487c-9525-e2b8ff3f4898","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP5.tagCount","metric":{"value":10,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP5.status","result":{"healthy":true,"time":1793370766537,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"CAP5B","description":null,"enabled":true,"version":1,"collection":"production","collections":["production"],"signature":"2b621a66b394529d8c2340b5edc2ff7b3a58699c378974d68a9c2224ac264363","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"5265e98d-1f3e-4f7c-a375-cd7e12eee0cb","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP5B.tagCount","metric":{"value":6,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP5B.status","result":{"healthy":true,"time":1793370766538,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"System","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"7d25b29271a47ff52b6d6712ccecbcd5964ea2baefe25c6be850d07efe39d1d5","config":{"profile":{"type":"MANAGED","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"nonUseCount":0}},"data":["config.json"],"attributes":{"uuid":"6219db0d-9b8e-4f3a-b352-c7d8cc1e44ff"},"metrics":{"tagCount":{"name":"tags.provider.System.tagCount","metric":{"value":39,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.System.status","result":{"healthy":true,"time":1793370766539,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"default","description":"Default tag provider","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"c75b0f1fed92b9087d20984eddc79595326f7c4289a84535ac633b63b576e16f","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"defaultDatasourceName":null,"readPermissions":{"type":"AllOf","securityLevels":[]},"readOnly":false,"writePermissions":{"type":"AllOf","securityLevels":[]},"editPermissions":{"type":"AllOf","securityLevels":[]},"valuePersistence":"Database"}},"data":["config.json"],"attributes":{"uuid":"b8994ab8-8e74-4852-86f8-7dfbdcd1e8e0"},"metrics":{"tagCount":{"name":"tags.provider.default.tagCount","metric":{"value":1,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.default.status","result":{"healthy":true,"time":1793370766540,"message":"Running"}}}}],"metadata":{"total":4,"matching":4,"limit":-1,"offset":0}}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET tags/export?provider=CAP5B&type=json

`ign --profile azure-win --json api call --method GET --path /data/api/v1/tags/export --query provider=CAP5B --query type=json ` @ 2026-10-30T14:32:46Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/tags/export",
    "result": {
      "status": 200,
      "data": {
  "name": "",
  "tagType": "Provider",
  "tags": [
    {
      "name": "G",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "G1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "_types_",
      "tagType": "Folder",
      "tags": [
        {
          "name": "T0",
          "tagType": "UdtType",
          "tags": [
            {
              "dataType": "Int4",
              "name": "M0",
              "tagType": "AtomicTag",
              "value": 0,
              "valueSource": "memory"
            }
          ]
        },
        {
          "name": "A",
          "tagType": "Folder"
        }
      ]
    }
  ]
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### probe: provider listing and every exported tag of CAP5B

`printf %s $'provider=CAP5B listed=yes(production) export: G<Folder> G/G1=1 _types_<Folder> _types_/A<Folder> _types_/T0<UdtType> _types_/T0/M0=0\n' ` @ 2026-10-30T14:32:47Z

~~~
[stdout]
provider=CAP5B listed=yes(production) export: G<Folder> G/G1=1 _types_<Folder> _types_/A<Folder> _types_/T0<UdtType> _types_/T0/M0=0
[stderr]
[exit 0]
~~~

#### S5(i) row: is R9 loaded while the marker lists no files?

`bash -c \"\$0\"\ --profile\ \"\$1\"\ --json\ api\ call\ --method\ GET\ --path\ /data/api/v1/tags/export\ --query\ \"provider=\$2\"\ --query\ type=json\ \|\ python3\ -I\ \"\$3\"\ check\ \"\$\{@:4\}\" ign azure-win CAP5B $TMP/cd5flat.py R9=1 ` @ 2026-10-30T14:32:47Z

~~~
[stdout]
FAIL R9=1 (observed absent)
[stderr]
[exit 0]
~~~

#### gateway log, WARN and above since the previous scan (ign logs --since 1793370762256; timestamp, logger, level, message)

`bash -c \"\$0\"\ --profile\ \"\$1\"\ --json\ logs\ --min-level\ warn\ --since\ \"\$2\"\ --limit\ 200\ \|\ jq\ -c\ \".data.items\[\]\ \|\ \{timestamp\,\ loggerName\,\ level\,\ message\}\" ign azure-win 1793370762256 ` @ 2026-10-30T14:32:47Z

~~~
[stdout]
[stderr]
[exit 0]
~~~

(ii) The root marker set to list `tags.json` (exact-lexeme edit of `"files": []`) and `tags.json` removed; ONE scan. Is R9 gone or still loaded, and is a WARN logged naming the resource?

#### S5(ii) the marker to put

`cat $TMP/local/s5/unary-resource.json ` @ 2026-10-30T14:32:47Z

~~~
[stdout]
{
  "scope": "G",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "tags.json"
  ],
  "attributes": {
    "config": {}
  }
}
[stdout did not end with a newline]
[stderr]
[exit 0]
~~~

#### S5(ii) put the marker listing tags.json, remove tags.json, list the root

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@52.154.75.231 ` @ 2026-10-30T14:32:47Z

~~~
[stdin]
put "$TMP/local/s5/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/unary-resource.json"
rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/tags.json"
@ls -la "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B"
[stdout]
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.ZN0juwLHkG/local/s5/unary-resource.json" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/unary-resource.json"
sftp> rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/ignition/tag-definition/CAP5B/tags.json"
drwx******    1 -        -               0 Oct 30 14:32 G
-rw-******    1 -        -             154 Oct 30 14:32 unary-resource.json
[stderr]
[exit 0]
~~~

#### POST scan/config

`ign --profile azure-win --json api call --method POST --path /data/api/v1/scan/config ` @ 2026-10-30T14:32:50Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "POST",
    "path": "/data/api/v1/scan/config",
    "result": {
      "status": 200,
      "data": {
  "scanActive": true,
  "lastScanTimestamp": 1793370765095,
  "lastScanDuration": 488
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET scan/config (after completion)

`ign --profile azure-win --json api call --method GET --path /data/api/v1/scan/config ` @ 2026-10-30T14:32:51Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/scan/config",
    "result": {
      "status": 200,
      "data": {
  "scanActive": false,
  "lastScanTimestamp": 1793370770966,
  "lastScanDuration": 526
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET resources/list/ignition/tag-provider

`ign --profile azure-win --json api call --method GET --path /data/api/v1/resources/list/ignition/tag-provider --query limit=-1 ` @ 2026-10-30T14:32:52Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/resources/list/ignition/tag-provider",
    "result": {
      "status": 200,
      "data": {"items":[{"type":"ignition/tag-provider","name":"CAP5","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"9126111f852618f0a357bae275c18bdedf0f4c8b9e45e3d1b314463543f4afd2","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"c93fe62e-fd8a-487c-9525-e2b8ff3f4898","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP5.tagCount","metric":{"value":10,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP5.status","result":{"healthy":true,"time":1793370772355,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"CAP5B","description":null,"enabled":true,"version":1,"collection":"production","collections":["production"],"signature":"2b621a66b394529d8c2340b5edc2ff7b3a58699c378974d68a9c2224ac264363","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}},"data":["config.json"],"attributes":{"uuid":"5265e98d-1f3e-4f7c-a375-cd7e12eee0cb","enabled":true},"metrics":{"tagCount":{"name":"tags.provider.CAP5B.tagCount","metric":{"value":6,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.CAP5B.status","result":{"healthy":true,"time":1793370772356,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"System","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"7d25b29271a47ff52b6d6712ccecbcd5964ea2baefe25c6be850d07efe39d1d5","config":{"profile":{"type":"MANAGED","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"nonUseCount":0}},"data":["config.json"],"attributes":{"uuid":"6219db0d-9b8e-4f3a-b352-c7d8cc1e44ff"},"metrics":{"tagCount":{"name":"tags.provider.System.tagCount","metric":{"value":39,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.System.status","result":{"healthy":true,"time":1793370772356,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"default","description":"Default tag provider","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"c75b0f1fed92b9087d20984eddc79595326f7c4289a84535ac633b63b576e16f","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"defaultDatasourceName":null,"readPermissions":{"type":"AllOf","securityLevels":[]},"readOnly":false,"writePermissions":{"type":"AllOf","securityLevels":[]},"editPermissions":{"type":"AllOf","securityLevels":[]},"valuePersistence":"Database"}},"data":["config.json"],"attributes":{"uuid":"b8994ab8-8e74-4852-86f8-7dfbdcd1e8e0"},"metrics":{"tagCount":{"name":"tags.provider.default.tagCount","metric":{"value":1,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.default.status","result":{"healthy":true,"time":1793370772357,"message":"Running"}}}}],"metadata":{"total":4,"matching":4,"limit":-1,"offset":0}}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET tags/export?provider=CAP5B&type=json

`ign --profile azure-win --json api call --method GET --path /data/api/v1/tags/export --query provider=CAP5B --query type=json ` @ 2026-10-30T14:32:52Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/tags/export",
    "result": {
      "status": 200,
      "data": {
  "name": "",
  "tagType": "Provider",
  "tags": [
    {
      "name": "G",
      "tagType": "Folder",
      "tags": [
        {
          "dataType": "Int4",
          "defaultValue": 1,
          "name": "G1",
          "tagType": "AtomicTag",
          "value": 1,
          "valueSource": "memory"
        }
      ]
    },
    {
      "name": "_types_",
      "tagType": "Folder",
      "tags": [
        {
          "name": "T0",
          "tagType": "UdtType",
          "tags": [
            {
              "dataType": "Int4",
              "name": "M0",
              "tagType": "AtomicTag",
              "value": 0,
              "valueSource": "memory"
            }
          ]
        },
        {
          "name": "A",
          "tagType": "Folder"
        }
      ]
    }
  ]
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### probe: provider listing and every exported tag of CAP5B

`printf %s $'provider=CAP5B listed=yes(production) export: G<Folder> G/G1=1 _types_<Folder> _types_/A<Folder> _types_/T0<UdtType> _types_/T0/M0=0\n' ` @ 2026-10-30T14:32:52Z

~~~
[stdout]
provider=CAP5B listed=yes(production) export: G<Folder> G/G1=1 _types_<Folder> _types_/A<Folder> _types_/T0<UdtType> _types_/T0/M0=0
[stderr]
[exit 0]
~~~

#### S5(ii) row: is R9 gone after tags.json is removed while the marker lists it?

`bash -c \"\$0\"\ --profile\ \"\$1\"\ --json\ api\ call\ --method\ GET\ --path\ /data/api/v1/tags/export\ --query\ \"provider=\$2\"\ --query\ type=json\ \|\ python3\ -I\ \"\$3\"\ check\ \"\$\{@:4\}\" ign azure-win CAP5B $TMP/cd5flat.py \!R9 ` @ 2026-10-30T14:32:53Z

~~~
[stdout]
ok   !R9 (observed absent)
[stderr]
[exit 0]
~~~

#### gateway log, WARN and above since the previous scan (ign logs --since 1793370768247; timestamp, logger, level, message)

`bash -c \"\$0\"\ --profile\ \"\$1\"\ --json\ logs\ --min-level\ warn\ --since\ \"\$2\"\ --limit\ 200\ \|\ jq\ -c\ \".data.items\[\]\ \|\ \{timestamp\,\ loggerName\,\ level\,\ message\}\" ign azure-win 1793370768247 ` @ 2026-10-30T14:32:53Z

~~~
[stdout]
{"timestamp":1793370770856,"loggerName":"com.inductiveautomation.ignition.gateway.resourcecollection.ResourceCollectionFileTree","level":"ERROR","message":"Error creating dataFile for resourceManifest=C:\\Program Files\\Inductive Automation\\Ignition\\data\\config\\resources\\production\\ignition\\tag-definition\\CAP5B\\unary-resource.json, file=tags.json"}
[stderr]
[exit 0]
~~~


### CD5-W-clean — production/ files removed and scanned, providers CAP5 and CAP5B deleted, one scan

The CD5-D order: the production/ files go first (PowerShell Remove-Item as the account, which holds Modify on production), with a scan, then the API delete, then a listing of anything CAP5* left in core (the account cannot remove core files; any leftover is reported, not swept).

#### remove every CAP5 and CAP5B directory this driver wrote under production\ignition

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@52.154.75.231 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIA...\[3898\ chars\] ` @ 2026-10-30T14:33:30Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; foreach($n in 'CAP5','CAP5B'){ $p=Join-Path 'C:\Program Files\Inductive Automation\Ignition\data\config\resources' ('production\ignition\tag-provider\'+$n); if(Test-Path -LiteralPath $p){ Remove-Item -LiteralPath $p -Recurse -Force; 'removed '+$p.Substring('C:\Program Files\Inductive Automation\Ignition\data\config\resources'.Length) } }; foreach($n in 'CAP5','CAP5B'){ $p=Join-Path 'C:\Program Files\Inductive Automation\Ignition\data\config\resources' ('production\ignition\tag-group\'+$n); if(Test-Path -LiteralPath $p){ Remove-Item -LiteralPath $p -Recurse -Force; 'removed '+$p.Substring('C:\Program Files\Inductive Automation\Ignition\data\config\resources'.Length) } }; foreach($n in 'CAP5','CAP5B'){ $p=Join-Path 'C:\Program Files\Inductive Automation\Ignition\data\config\resources' ('production\ignition\tag-type-definition\'+$n); if(Test-Path -LiteralPath $p){ Remove-Item -LiteralPath $p -Recurse -Force; 'removed '+$p.Substring('C:\Program Files\Inductive Automation\Ignition\data\config\resources'.Length) } }; foreach($n in 'CAP5','CAP5B'){ $p=Join-Path 'C:\Program Files\Inductive Automation\Ignition\data\config\resources' ('production\ignition\tag-definition\'+$n); if(Test-Path -LiteralPath $p){ Remove-Item -LiteralPath $p -Recurse -Force; 'removed '+$p.Substring('C:\Program Files\Inductive Automation\Ignition\data\config\resources'.Length) } }; 
[stdout]
removed \production\ignition\tag-provider\CAP5B
removed \production\ignition\tag-group\CAP5B
removed \production\ignition\tag-type-definition\CAP5
removed \production\ignition\tag-type-definition\CAP5B
removed \production\ignition\tag-definition\CAP5
removed \production\ignition\tag-definition\CAP5B
[6 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### production\ignition after the removal (every file, relative)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@52.154.75.231 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIA...\[1014\ chars\] ` @ 2026-10-30T14:33:31Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; $r=Join-Path 'C:\Program Files\Inductive Automation\Ignition\data\config\resources' 'production\ignition'; if(Test-Path -LiteralPath $r){ Get-ChildItem -LiteralPath $r -Recurse -File | ForEach-Object { $_.FullName.Substring($r.Length) } | Sort-Object } else { '<no production\ignition>' }
[stdout]
[stderr]
[exit 0]
~~~

#### POST scan/config

`ign --profile azure-win --json api call --method POST --path /data/api/v1/scan/config ` @ 2026-10-30T14:33:33Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "POST",
    "path": "/data/api/v1/scan/config",
    "result": {
      "status": 200,
      "data": {
  "scanActive": true,
  "lastScanTimestamp": 1793370770966,
  "lastScanDuration": 526
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET scan/config (after completion)

`ign --profile azure-win --json api call --method GET --path /data/api/v1/scan/config ` @ 2026-10-30T14:33:35Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/scan/config",
    "result": {
      "status": 200,
      "data": {
  "scanActive": false,
  "lastScanTimestamp": 1793370814473,
  "lastScanDuration": 455
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### is CAP5B still a tag provider? (no API delete needed when it is not)

`printf %s\ listed:\ no\\n CAP5B ` @ 2026-10-30T14:33:35Z

~~~
[stdout]
CAP5B listed: no
[stderr]
[exit 0]
~~~

#### ign tags provider delete CAP5 --yes

`ign --profile azure-win --json --yes tags provider delete CAP5 ` @ 2026-10-30T14:33:36Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "deleted": "CAP5"
  }
}
[stderr]
[exit 0]
~~~

#### POST scan/config

`ign --profile azure-win --json api call --method POST --path /data/api/v1/scan/config ` @ 2026-10-30T14:33:41Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "POST",
    "path": "/data/api/v1/scan/config",
    "result": {
      "status": 200,
      "data": {
  "scanActive": true,
  "lastScanTimestamp": 1793370814473,
  "lastScanDuration": 455
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET scan/config (after completion)

`ign --profile azure-win --json api call --method GET --path /data/api/v1/scan/config ` @ 2026-10-30T14:33:43Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/scan/config",
    "result": {
      "status": 200,
      "data": {
  "scanActive": false,
  "lastScanTimestamp": 1793370822463,
  "lastScanDuration": 494
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET resources/list/ignition/tag-provider (CAP5 and CAP5B gone?)

`ign --profile azure-win --json api call --method GET --path /data/api/v1/resources/list/ignition/tag-provider --query limit=-1 ` @ 2026-10-30T14:33:43Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/resources/list/ignition/tag-provider",
    "result": {
      "status": 200,
      "data": {"items":[{"type":"ignition/tag-provider","name":"System","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"7d25b29271a47ff52b6d6712ccecbcd5964ea2baefe25c6be850d07efe39d1d5","config":{"profile":{"type":"MANAGED","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"nonUseCount":0}},"data":["config.json"],"attributes":{"uuid":"6219db0d-9b8e-4f3a-b352-c7d8cc1e44ff"},"metrics":{"tagCount":{"name":"tags.provider.System.tagCount","metric":{"value":39,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.System.status","result":{"healthy":true,"time":1793370823895,"message":"Running"}}}},{"type":"ignition/tag-provider","name":"default","description":"Default tag provider","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"c75b0f1fed92b9087d20984eddc79595326f7c4289a84535ac633b63b576e16f","config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":true},"settings":{"defaultDatasourceName":null,"readPermissions":{"type":"AllOf","securityLevels":[]},"readOnly":false,"writePermissions":{"type":"AllOf","securityLevels":[]},"editPermissions":{"type":"AllOf","securityLevels":[]},"valuePersistence":"Database"}},"data":["config.json"],"attributes":{"uuid":"b8994ab8-8e74-4852-86f8-7dfbdcd1e8e0"},"metrics":{"tagCount":{"name":"tags.provider.default.tagCount","metric":{"value":1,"type":"gauge"}}},"healthchecks":{"status":{"name":"tags.provider.default.status","result":{"healthy":true,"time":1793370823896,"message":"Running"}}}}],"metadata":{"total":2,"matching":2,"limit":-1,"offset":0}}
    }
  }
}
[stderr]
[exit 0]
~~~

#### anything named CAP5* left under data\config\resources

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@52.154.75.231 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIA...\[926\ chars\] ` @ 2026-10-30T14:33:43Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; $r='C:\Program Files\Inductive Automation\Ignition\data\config\resources'; $h=Get-ChildItem -LiteralPath $r -Recurse -Force -Filter 'CAP5*' -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName.Substring($r.Length) }; if($h){ $h } else { '<none>' }
[stdout]
<none>
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### probe: provider listing and every exported tag of CAP5 CAP5B

`printf %s $'provider=CAP5 listed=no export: export_unreadable\nprovider=CAP5B listed=no export: export_unreadable\n' ` @ 2026-10-30T14:33:46Z

~~~
[stdout]
provider=CAP5 listed=no export: export_unreadable
provider=CAP5B listed=no export: export_unreadable
[stderr]
[exit 0]
~~~

## CD5-RT

The live round trip after the code change (commit 1c21626), on the same two hosts. The binary under test was installed from this branch first (`cargo install --path crates/ignition-cli --force`; it reports `ign 1.6.0` because release.yml stamps the version from the tag). Env values are named, never printed: `$IGN_TOKEN_CAP_A` comes from the 0600 `~/.cache/ign-deploy-rig/tokens.env`. The provider is CAP5, seeded through the gateway's own native import (`capture-cd5.sh seed` / `capture22w.sh cd5wseed`, S1's two imports), so every tag byte the tests read is gateway-written.

### CD5-RT-D — Docker 8.3.6 rig

A fresh rig: no `ign-deploy-rig` container or volume existed before (`docker volume ls` showed none), then `capture22.sh up22` (2026-10-30T15:12:15Z) and `capture22.sh setup22` (RUNNING, deploymentMode production at 2026-10-30T15:15:06Z). `capture-cd5.sh cfg` rewrote the throwaway ign config for the new token.

#### seed (excerpt of the output)

`bash capture-cd5.sh seed` @ 2026-10-30T15:15:16Z

~~~
(ign tags provider create CAP5, then the two native imports; the second answered)
"data": {"successCount":6,"failureCount":0,"failures":[]}
(after the 20 s flush, the CAP5 files in core)
core/ignition/tag-definition/CAP5/F/tags.json
core/ignition/tag-definition/CAP5/F/unary-resource.json
core/ignition/tag-definition/CAP5/G/tags.json
core/ignition/tag-definition/CAP5/G/unary-resource.json
core/ignition/tag-definition/CAP5/tags.json
core/ignition/tag-definition/CAP5/udts.json
core/ignition/tag-definition/CAP5/unary-resource.json
core/ignition/tag-group/CAP5/Default Historical/config.json
core/ignition/tag-group/CAP5/Default Historical/resource.json
core/ignition/tag-group/CAP5/Default/config.json
core/ignition/tag-group/CAP5/Default/resource.json
core/ignition/tag-provider/CAP5/config.json
core/ignition/tag-provider/CAP5/resource.json
core/ignition/tag-type-definition/CAP5/A/B/udts.json
core/ignition/tag-type-definition/CAP5/A/B/unary-resource.json
core/ignition/tag-type-definition/CAP5/A/unary-resource.json
core/ignition/tag-type-definition/CAP5/udts.json
core/ignition/tag-type-definition/CAP5/unary-resource.json
~~~

The layout is the S1 layout of CD5-D, file for file.

#### live_deploy_plan (pull, render, plan; the whole suite; excerpt: every test line, the pull, skip, placed and plan lines, and the result line)

`IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 IGNITION_LIVE_TOKEN=$IGN_TOKEN_CAP_A IGN_DEPLOY_RIG_DATA=$HOME/.cache/ign-deploy-rig/data IGN_DEPLOY_RIG_MODE=production cargo test -p ignition-core --test live_deploy_plan -- --ignored --nocapture --test-threads=1` @ 2026-10-30T15:15:46Z

~~~
test cd5_layouts_round_trip_live ... pull: 92 files written, manifest generated, 7 skipped
  skipped ignition/images (type_without_rule)
  skipped ignition/keyboard_layout (type_without_rule)
  skipped ignition/opc-connection/Ignition OPC UA Server (embedded_secret)
  skipped ignition/security-levels (denied_type)
  skipped ignition/security-properties (denied_type)
  skipped ignition/translations (type_without_rule)
  skipped ignition/user-source (denied_type)
  placed tags/CAP5/tag-definition/tags.json (132 bytes, equal to live)
  placed tags/CAP5/tag-definition/udts.json (84 bytes, equal to live)
  placed tags/CAP5/tag-definition/unary-resource.json (171 bytes, equal to live)
  placed tags/CAP5/tag-type-definition/A/unary-resource.json (135 bytes, equal to live)
  placed tags/CAP5/tag-type-definition/A/B/udts.json (224 bytes, equal to live)
  placed tags/CAP5/tag-type-definition/A/B/unary-resource.json (154 bytes, equal to live)
plan: added 0 changed 0 removed 0 same 43 unmanaged 1 has_changes false
PASS cd5_layouts_round_trip_live
test plan_takes_no_lock_live ... PASS plan_takes_no_lock_live: lock free before and after plan
test pull_render_plan_is_empty_live ... pull: 92 files written, manifest generated, 7 skipped
plan: added 0 changed 0 removed 0 same 43 unmanaged 1 has_changes false
PASS pull_render_plan_is_empty_live
test pull_then_verify_is_green_live ... PASS pull_then_verify_is_green_live
test verify_green_then_red_live ... PASS verify_green_then_red_live
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 13.28s
~~~

None of the 7 skips is a CAP5 unit, and none is `render_refused`. On the 2026-10-29 production pull the same two layouts were skipped `render_refused`. The provider-root unit `ignition/tag-definition/CAP5` (root marker, root `tags.json` and `udts.json`), `ignition/tag-type-definition/CAP5/A` and `.../A/B` all plan `same`.

#### live_deploy_apply cd5 (apply an edit to both layouts, then roll it back)

`IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 IGNITION_LIVE_TOKEN=$IGN_TOKEN_CAP_A IGN_DEPLOY_RIG_DATA=$HOME/.cache/ign-deploy-rig/data IGN_DEPLOY_RIG_MODE=production cargo test -p ignition-core --test live_deploy_apply cd5 -- --ignored --nocapture --test-threads=1` @ 2026-10-30T15:16:15Z

~~~
test cd5_apply_root_and_nested_live ... cd5_apply_root_and_nested_live @ 2026-10-30T15:16:21Z
pull: 92 files written, 7 skipped
plan: added 0 changed 2 removed 0 same 41 plan_sha256 47daaa8647987c637f728fb33aaf93d6be3e86abfe95eb5366a3aa7640f923c1
apply: outcome Applied run Some("20261030T151622Z-119b23") written 5 deleted 0
apply warning: read_back: com.inductiveautomation.opcua/one-time not compared: no REST read route
apply warning: probes: the manifest declares no `verify.tag_probes`; nothing was probed
export after apply: R1 Some(7) M1 Some(7)
rollback: outcome Restored restored 0 deleted 5
export after rollback: R1 Some(1) M1 Some(1)
PASS cd5_apply_root_and_nested_live in 8.7s (plan 47daaa8647987c637f728fb33aaf93d6be3e86abfe95eb5366a3aa7640f923c1)
ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 8.89s
~~~

The test edited R1's `"defaultValue": 1,` to 7 in the pulled root `tags.json` and M1's `"value": 1,` to 7 in the pulled `A/B/udts.json` (exact-lexeme edits, CD4). The plan changed exactly the units `ignition/tag-definition/CAP5` and `ignition/tag-type-definition/CAP5/A/B`, with an empty delete set. Apply wrote the 5 files of those two units into `production/`, and its own post-apply verify (`verify_done` `ok: true`) was green. After the one apply scan, the REST `tags/export` showed R1 = 7 and `_types_/A/B/T/M1` = 7. The applied tree then planned empty. A rollback by run id removed the 5 files, the `production/` tree was back byte for byte, and the export showed both values back at 1. The two apply warnings are the rig's usual ones (an OPC UA singleton with no REST read route; no tag probes declared) and do not concern CAP5.

#### cleanup and teardown

`CAPTURE_OUT=<scratch file> bash capture-cd5.sh clean` @ 2026-10-30T15:16:38Z (its blocks went to a scratch file so that this doc keeps one `CD5-D-clean` section; the readings that matter are quoted here)

~~~
(anything named CAP5* left on the bind mount, after the sweep)
<none>
(probe: are CAP5, CAP5B and CAP5X gone?)
provider=CAP5 listed=no export: export_unreadable
provider=CAP5B listed=no export: export_unreadable
provider=CAP5X listed=no export: export_unreadable
~~~

`IGN_DEPLOY_RIG_RESOURCES=$HOME/.cache/ign-deploy-rig/data/config/resources docker compose -p ign-deploy-rig -f compose.yml -f compose.bind.yml down -v` @ 2026-10-30T15:16:53Z: volume `ign-deploy-rig_gw-data` and network removed; afterwards no container or volume of the project is left. (A first `down -v` without `IGN_DEPLOY_RIG_RESOURCES` set refused on compose interpolation and changed nothing.)

## Decisions

Each entry is an answer taken from the blocks above. Code comments and the README cite these entries by ID. Per-host verdicts follow the plan's rules (P2). A final verdict is the more conservative of the two hosts' verdicts (refused over read-only over accept; honoured over ignored).

**Log lines not counted (orchestrator decision D7).** The only gateway WARN/ERROR lines that name a CAP5* provider and are not about tag files are these two kinds:

- `com.inductiveautomation.ignition.gateway.tags.ep.StandardTagProviderExtensionPoint` ERROR `valuePersistence not found in Tag Provider: '<P>' profile, defaulting to Database`. It is logged once on every provider load, whatever the layout and the control included: for CAP5 in CD5-D S1 and CD5-W S1 (created through the API), for CAP5B in CD5-D S2 and CD5-W S2 (the first load of the production-only provider), and for CAP5X in CD5-D S6. It concerns the provider profile, not a tag file, so it is not counted.
- `AuditManager` ERROR `Error getting gateway audit profile 'WIGNVM01_Audit'` (Docker only: 18 lines in CD5-D S1, 5 in S2, 10 in S6). It is fixture noise. The rig's `security-properties` comes from the repo's sanitized `live-rig-deploy/security-properties/config.json` (`gatewayAuditProfile`), installed by `mint-token.sh`. No production gateway is involved. Not counted.

Apart from these, the S2, S3 and S4 windows logged no WARN or ERROR line on either host. The `ign rig logs` blocks of CD5-D held no line naming CAP5, CAP5B or a tag path.

### CD5a — root-level tags

Rests on `## CD5-D` S1 to S4 and `## CD5-W` S1 to S4 (the listings, the per-file bytes, the `tags/export` and probe blocks, the row checks and the log blocks).

- Layout (S1, both hosts, identical bytes: CD5-W S1 compares all 12 tag files to the Docker fixtures and every one is `same`). Root AtomicTag R1 is written to `ignition/tag-definition/CAP5/tags.json` and root UdtInstance RInst to `ignition/tag-definition/CAP5/udts.json`. Both sit beside the provider-root `unary-resource.json`, whose `files` is `["tags.json", "udts.json"]` (CD5 saw `files: []` when there were no root tags). Folders F and G are `F/` and `G/`, each with `tags.json` and a marker listing it. This is the layout the mapping assumes: a root `tags.json` or `udts.json` joins the existing provider-root unit `ignition/tag-definition/<P>`.
- S2 write (one scan): on both hosts, the production-only CAP5B showed R1=1 and RInst, and the CAP5 production root unit gave R1=2 while G1 (core folder G, never written to production) stayed present. A production root unit does not hide core sibling folders.
- S3 change (one scan): R1 1 -> 3 (CAP5B) and 2 -> 3 (CAP5) on both hosts.
- S4 delete (one scan): CAP5B lost R1 and RInst and stayed listed. CAP5's production root unit, reduced to a `files: []` marker, hid the core root members (R1 and RInst absent) while G1 stayed. This is the deploy model's `CoreRelation::Hidden` for the provider-root unit (research A4 confirmed).
- No ladder was needed on either host, and no counted WARN was logged.

Host verdict CD5a docker: accept
Host verdict CD5a windows: accept

Verdict CD5a: accept

### CD5b — nested UDT type folders

Rests on `## CD5-D` S1 to S4 and `## CD5-W` S1 to S4.

- Layout (S1, both hosts, identical bytes). The UdtType at `_types_/A/B/T` is written to `ignition/tag-type-definition/CAP5/A/B/udts.json` with its own `A/B/unary-resource.json` (`files: ["udts.json"]`). T0 at the top of `_types_` stays in the type root `udts.json` beside its marker (`files: ["udts.json"]`). No `_types_` directory exists anywhere. The folder `A/`, which holds only the sub-folder, carries its own marker with `files: []` on both hosts (`Observation CD5b-A docker: marker`, `Observation CD5b-A windows: marker`). So every nested folder is its own unit, `ignition/tag-type-definition/<P>/A` and `.../A/B`, which matches the 76 distinct skipped units seen on WIGNVM01 (research A2, A3 confirmed).
- S2 write (one scan): on both hosts, CAP5B showed T with M1=1, and the CAP5 production `A/B/` unit gave M1=2 while T0 stayed present. A nested override does not hide the core type root.
- S3 change (one scan): M1 1 -> 3 (CAP5B) and 2 -> 3 (CAP5) on both hosts.
- S4 delete (one scan): removing `tag-type-definition/CAP5B/A/B/` whole removed T. Informational, outside any verdict: with the CAP5 production `A/B/` override removed, T went back to the core value M1=1 on both hosts.
- NTFS budget (CD5-W): for provider `CAP5` the longest absolute path below `core\ignition\tag-type-definition\CAP5` is 131 characters, and below `production\ignition\tag-type-definition` it is 138 (CAP5B), against `LONG_PATH_LIMIT` 240. Each further nesting level adds its folder name plus one separator. The v1.6.1 `path_too_long` and `unreadable` handling bounds deep chains.
- No ladder was needed on either host, and no counted WARN was logged.

Host verdict CD5b docker: accept
Host verdict CD5b windows: accept

Verdict CD5b: accept

### CD5c — unary-resource.json files

Rests on the S5 sub-sections of `## CD5-D` and `## CD5-W` (on CAP5B).

- (i) A root `tags.json` holding R9 was written beside a root marker that still said `files: []`. After one scan R9 was not loaded, on either host (`FAIL R9=1 (observed absent)` is the expected reading of an honoured `files`). Contrast S3, where a changed `tags.json` that the marker lists was picked up in one scan: the gateway loads only the member files its marker lists.
- (ii) The marker was then set to list `tags.json` and `tags.json` was removed. R9 stayed absent. On Windows the gateway logged, in that window, `com.inductiveautomation.ignition.gateway.resourcecollection.ResourceCollectionFileTree` ERROR `Error creating dataFile for resourceManifest=C:\Program Files\Inductive Automation\Ignition\data\config\resources\production\ignition\tag-definition\CAP5B\unary-resource.json, file=tags.json`. On Docker the recorded run logged no such line within the window. The dry run that preceded the discarded first run, recorded in scratch and not in this doc, logged the same ERROR for the Linux path.
- So the gateway honours `files`. A tag unit whose marker disagrees with its member files is silently partial (an unlisted member is ignored) or logs an error (a listed member is missing).

Host verdict CD5c docker: honoured
Host verdict CD5c windows: honoured

Verdict CD5c: honoured

### CD5d — a tag folder named tags.json

Rests on `## CD5-D` S6 (Docker only).

- Provider CAP5X got a root AtomicTag R1, then a Folder named `tags.json` holding X1. The import answered `{"successCount":1,"failureCount":1,"failures":[{"quality":"Error","qualitySubCode":769,"diagnosticMessage":"The name 'tags.json' is not a valid tag name"}]}`, and the log holds `tags.management.provider` ERROR `Error processing edit for tag path '[CAP5X]tags.json': Error_Configuration("The name 'tags.json' is not a valid tag name")`.
- The folder was created anyway. The export shows `tags.json<Folder> tags.json/X1=1`, and the disk holds a directory `ignition/tag-definition/CAP5X/tags.json/` with `tags.json` (X1) and a marker. The root R1 file was then lost: no `CAP5X/tags.json` file exists, and the log holds `tags.storage.resource` WARN `Failed to push ModifyResourceOperation{resourceId=ResourceId{resourcePath=ignition/tag-definition/CAP5X/_unary, collectionName=core}}: I/O error: .../CAP5X/tags.json.tmp -> .../CAP5X/tags.json...`, plus the matching `ResourceTypePartition` ERROR.
- The gateway therefore accepts a folder named like a member file into its tree and the file system, despite the refusal text, and the result corrupts the provider-root unit on disk. Render must refuse such a path itself.

Verdict CD5d: accepted

### CD5-CTL — folder control

Rests on the F rows of S2 to S4 in `## CD5-D` and `## CD5-W`.

- Folder F's unit `tag-definition/<P>/F/`: production-only in CAP5B, and a production override in CAP5. On both hosts, F1 read 1 (CAP5B) and 2 (CAP5) after the S2 scan and 3 after the S3 scan, and was absent after the S4 scan in both shapes: the CAP5B folder removed whole, and the CAP5 override reduced to a `files: []` marker, which hides the core `F/tags.json`. Every row matched after one scan.
- No WARN or ERROR line names the control's resources. The only lines naming its providers are the D7 not-counted kinds above.
- The shipped v1.6 folder behaviour is confirmed for file-level write, change and delete inside an already loaded provider. C11 had only shown a whole provider loading at once.

Host verdict CD5-CTL docker: pass
Host verdict CD5-CTL windows: pass

Verdict CD5-CTL: pass
