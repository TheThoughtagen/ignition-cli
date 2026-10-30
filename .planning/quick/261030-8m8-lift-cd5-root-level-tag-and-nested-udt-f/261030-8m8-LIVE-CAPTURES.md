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

## Decisions
