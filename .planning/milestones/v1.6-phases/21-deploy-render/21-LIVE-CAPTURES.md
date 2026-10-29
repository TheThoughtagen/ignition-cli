# Phase 21 — Live Captures (8.3.6)

| Field | Value |
| --- | --- |
| Image | inductiveautomation/ignition:8.3.6 |
| Compose variant | bind (compose.yml + compose.bind.yml) |
| Bind path (outside repo) | $HOME/.cache/ign-deploy-rig/resources-21 |
| Gateway name | ign-deploy-rig |
| deploymentMode | production |
| Started (UTC) | 2026-10-13T11:50:17Z |
| ign --version | ign 1.5.0 |
| WebDev routes (ign webdev deploy) | deployed to rig21 with `ign webdev deploy --yes`; first import left the `tags` route unregistered (`ign webdev status`: absent), a second deploy registered all four routes |

## C21-UUID8 — D-03 live check: a hand-written resource carrying a sha2-derived version-8 uuid

### POST create database-translator CAP21_API through the config API (no collection)

~~~
POST /data/api/v1/resources/ignition/database-translator @ 2026-10-13T11:57:46Z
HTTP/1.1 200 OK
Date: Tue, 13 Oct 2026 11:57:46 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 262

{
  "success": true,
  "changes": [
    {
      "name": "CAP21_API",
      "type": "ignition/database-translator",
      "collection": "core",
      "newSignature": "e6f7681a742f71acaf4c117cc8a18fa53acb1110c120fbec5ca5b2002a6856be"
    }
  ],
  "problem": null
}
[curl http=200 total=0.231315s]
~~~

### CAP21_API config.json as the gateway wrote it

~~~
$ cat core/ignition/database-translator/CAP21_API/config.json @ 2026-10-13T11:57:46Z
{
  "alterTable": "ALTER TABLE {tablename} {alterdef}",
  "alterTableColumnDef": "ADD COLUMN {columnname} {type}",
  "autoIncTypeDef": "SERIAL NOT NULL",
  "blobType": "bytea",
  "boolType": "int",
  "columnQuoteChar": "\"",
  "createIndex": "CREATE INDEX {indexname} ON {tablename}({columnname})",
  "createTable": "CREATE TABLE {tablename} ({creationdef}{primarykeydef})",
  "currentTimeQuery": "SELECT CURRENT_TIMESTAMP",
  "datetimeType": "timestamp",
  "fetchKeyQuery": "",
  "i1Type": "int",
  "i2Type": "int",
  "i4Type": "int",
  "i8Type": "bigint",
  "limit": "LIMIT {limit}",
  "limitClausePosition": "Back",
  "primaryKeyDef": "PRIMARY KEY ({columnname})",
  "r4Type": "float",
  "r8Type": "double precision",
  "stringType": "varchar(255)",
  "supportsRGK": true,
  "tableListFilter": "",
  "textType": "text"
}
[output did not end with a newline]
[exit 0]
~~~

### CAP21_API resource.json as the gateway wrote it

~~~
$ cat core/ignition/database-translator/CAP21_API/resource.json @ 2026-10-13T11:57:46Z
{
  "scope": "A",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "config.json"
  ],
  "attributes": {
    "uuid": "be605da6-baa2-44f3-b292-4739baf58421",
    "enabled": true
  }
}
[output did not end with a newline]
[exit 0]
~~~

### CAP21_API files with sha256

~~~
$ bash -c find\ .\ -path\ \"\*/database-translator/CAP21_API/\*\"\ -type\ f\ -exec\ shasum\ -a\ 256\ \{\}\ +\ \|\ sort\ -k2 @ 2026-10-13T11:57:46Z
b542c38cae700d8244957e6f35db71bdea4046856d1a140348c920e8816748ca  ./core/ignition/database-translator/CAP21_API/config.json
926adb8fac7b367b8c17afd59d34d60c9f107e76c578c39f8b4f42aecf8ce12a  ./core/ignition/database-translator/CAP21_API/resource.json
[exit 0]
~~~

### v8 uuid for CAP21_V8 (D-03 derivation)

~~~
$ v8uuid ign-deploy-rig ignition database-translator CAP21_V8 @ 2026-10-13T11:57:47Z
4516ad9c-d6b0-8d86-89b9-802c96e33a9e
[exit 0]
~~~

### CAP21_V8 resource.json (hand-written, before scan)

~~~
$ cat production/ignition/database-translator/CAP21_V8/resource.json @ 2026-10-13T11:57:47Z
{
  "scope": "A",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "config.json"
  ],
  "attributes": {
    "uuid": "4516ad9c-d6b0-8d86-89b9-802c96e33a9e",
    "enabled": true
  }
}
[output did not end with a newline]
[exit 0]
~~~

### CAP21_V8 sha256 (before scan)

~~~
$ shasum -a 256 production/ignition/database-translator/CAP21_V8/config.json production/ignition/database-translator/CAP21_V8/resource.json @ 2026-10-13T11:57:47Z
b542c38cae700d8244957e6f35db71bdea4046856d1a140348c920e8816748ca  production/ignition/database-translator/CAP21_V8/config.json
912be19e176ff6a34838796882c578872b4df1712873f35423145d99930c13ee  production/ignition/database-translator/CAP21_V8/resource.json
[exit 0]
~~~

### POST scan/config

~~~
POST /data/api/v1/scan/config @ 2026-10-13T11:57:47Z
HTTP/1.1 200 OK
Date: Tue, 13 Oct 2026 11:57:47 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1791892381352,
  "lastScanDuration": 379
}
[curl http=200 total=0.007871s]
~~~

### GET scan/config (after completion)

~~~
GET /data/api/v1/scan/config @ 2026-10-13T11:57:48Z
HTTP/1.1 200 OK
Date: Tue, 13 Oct 2026 11:57:48 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1791892668144,
  "lastScanDuration": 528
}
[curl http=200 total=0.006817s]
~~~

### GET find CAP21_V8 (collection=production)

~~~
GET /data/api/v1/resources/find/ignition/database-translator/CAP21_V8?collection=production @ 2026-10-13T11:57:48Z
HTTP/1.1 200 OK
Date: Tue, 13 Oct 2026 11:57:48 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 1334

{
  "type": "ignition/database-translator",
  "name": "CAP21_V8",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "production",
  "collections": [
    "production"
  ],
  "signature": "bd307482963a7e5190cb0bbf65552d3dbc1435148d698cb856ee77d034746f28",
  "config": {
    "createTable": "CREATE TABLE {tablename} ({creationdef}{primarykeydef})",
    "createIndex": "CREATE INDEX {indexname} ON {tablename}({columnname})",
    "autoIncTypeDef": "SERIAL NOT NULL",
    "alterTable": "ALTER TABLE {tablename} {alterdef}",
    "alterTableColumnDef": "ADD COLUMN {columnname} {type}",
    "primaryKeyDef": "PRIMARY KEY ({columnname})",
    "limit": "LIMIT {limit}",
    "limitClausePosition": "Back",
    "currentTimeQuery": "SELECT CURRENT_TIMESTAMP",
    "columnQuoteChar": "\"",
    "supportsRGK": true,
    "fetchKeyQuery": "",
    "tableListFilter": "",
    "i1Type": "int",
    "i2Type": "int",
    "i4Type": "int",
    "i8Type": "bigint",
    "boolType": "int",
    "datetimeType": "timestamp",
    "r4Type": "float",
    "r8Type": "double precision",
    "stringType": "varchar(255)",
    "blobType": "bytea",
    "textType": "text"
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "uuid": "4516ad9c-d6b0-8d86-89b9-802c96e33a9e",
    "enabled": true
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.033691s]
~~~

### GET names (database-translator)

~~~
GET /data/api/v1/resources/names/ignition/database-translator @ 2026-10-13T11:57:48Z
HTTP/1.1 200 OK
Date: Tue, 13 Oct 2026 11:57:48 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 481

{"items":[{"name":"CAP21_API","enabled":true,"modes":["core"]},{"name":"CAP21_V8","enabled":true,"modes":["production"]},{"name":"GENERIC","enabled":true,"modes":["core"]},{"name":"MSSQL","enabled":true,"modes":["core"]},{"name":"MYSQL","enabled":true,"modes":["core"]},{"name":"ORACLE","enabled":true,"modes":["core"]},{"name":"POSTGRES","enabled":true,"modes":["core"]},{"name":"SQLITE","enabled":true,"modes":["core"]}],"metadata":{"total":8,"matching":8,"limit":-1,"offset":0}}
[curl http=200 total=0.017796s]
~~~

### CAP21_V8 sha256 (after scan)

~~~
$ shasum -a 256 production/ignition/database-translator/CAP21_V8/config.json production/ignition/database-translator/CAP21_V8/resource.json @ 2026-10-13T11:57:48Z
b542c38cae700d8244957e6f35db71bdea4046856d1a140348c920e8816748ca  production/ignition/database-translator/CAP21_V8/config.json
912be19e176ff6a34838796882c578872b4df1712873f35423145d99930c13ee  production/ignition/database-translator/CAP21_V8/resource.json
[exit 0]
~~~

### gateway log lines matching: CAP21_V8|will not be loaded|CAP21_API

~~~
$ bash -c docker\ logs\ \"\$1\"\ 2\>\&1\ \|\ grep\ -E\ --\ \"\$2\"\ \|\|\ true _ ign-deploy-rig-gw-1 CAP21_V8\|will\ not\ be\ loaded\|CAP21_API @ 2026-10-13T11:57:48Z
[exit 0]
~~~

## C21-C3b — omitted vs stale lastModificationSignature on hand-written resource.json, and whether the gateway rewrites it

### v8 uuid for CAP21_STALE

~~~
$ v8uuid ign-deploy-rig ignition database-translator CAP21_STALE @ 2026-10-13T11:57:59Z
1caa175b-6703-89d8-8ead-c9b7c6bdfb96
[exit 0]
~~~

### CAP21_STALE resource.json (hand-written: stale lastModificationSignature)

~~~
$ cat production/ignition/database-translator/CAP21_STALE/resource.json @ 2026-10-13T11:57:59Z
{
  "scope": "A",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "config.json"
  ],
  "attributes": {
    "lastModification": {
      "actor": "cap21",
      "timestamp": "2026-01-01T00:00:00Z"
    },
    "uuid": "1caa175b-6703-89d8-8ead-c9b7c6bdfb96",
    "lastModificationSignature": "0000000000000000000000000000000000000000000000000000000000000000",
    "enabled": true
  }
}
[output did not end with a newline]
[exit 0]
~~~

### sha256 of both hand-written resource.json (before scan)

~~~
$ shasum -a 256 production/ignition/database-translator/CAP21_V8/resource.json production/ignition/database-translator/CAP21_STALE/resource.json @ 2026-10-13T11:57:59Z
912be19e176ff6a34838796882c578872b4df1712873f35423145d99930c13ee  production/ignition/database-translator/CAP21_V8/resource.json
bb5d201ecabd705c90573e3b49bd65ee3571fccb6cbb50b42ec4f2c1e9c2de31  production/ignition/database-translator/CAP21_STALE/resource.json
[exit 0]
~~~

### POST scan/config

~~~
POST /data/api/v1/scan/config @ 2026-10-13T11:58:00Z
HTTP/1.1 200 OK
Date: Tue, 13 Oct 2026 11:58:00 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1791892668144,
  "lastScanDuration": 528
}
[curl http=200 total=0.025158s]
~~~

### GET scan/config (after completion)

~~~
GET /data/api/v1/scan/config @ 2026-10-13T11:58:01Z
HTTP/1.1 200 OK
Date: Tue, 13 Oct 2026 11:58:01 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1791892680906,
  "lastScanDuration": 768
}
[curl http=200 total=0.023993s]
~~~

### GET find CAP21_V8 (after scan 1)

~~~
GET /data/api/v1/resources/find/ignition/database-translator/CAP21_V8?collection=production @ 2026-10-13T11:58:01Z
HTTP/1.1 200 OK
Date: Tue, 13 Oct 2026 11:58:01 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 1334

{
  "type": "ignition/database-translator",
  "name": "CAP21_V8",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "production",
  "collections": [
    "production"
  ],
  "signature": "bd307482963a7e5190cb0bbf65552d3dbc1435148d698cb856ee77d034746f28",
  "config": {
    "createTable": "CREATE TABLE {tablename} ({creationdef}{primarykeydef})",
    "createIndex": "CREATE INDEX {indexname} ON {tablename}({columnname})",
    "autoIncTypeDef": "SERIAL NOT NULL",
    "alterTable": "ALTER TABLE {tablename} {alterdef}",
    "alterTableColumnDef": "ADD COLUMN {columnname} {type}",
    "primaryKeyDef": "PRIMARY KEY ({columnname})",
    "limit": "LIMIT {limit}",
    "limitClausePosition": "Back",
    "currentTimeQuery": "SELECT CURRENT_TIMESTAMP",
    "columnQuoteChar": "\"",
    "supportsRGK": true,
    "fetchKeyQuery": "",
    "tableListFilter": "",
    "i1Type": "int",
    "i2Type": "int",
    "i4Type": "int",
    "i8Type": "bigint",
    "boolType": "int",
    "datetimeType": "timestamp",
    "r4Type": "float",
    "r8Type": "double precision",
    "stringType": "varchar(255)",
    "blobType": "bytea",
    "textType": "text"
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "uuid": "4516ad9c-d6b0-8d86-89b9-802c96e33a9e",
    "enabled": true
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.025872s]
~~~

### GET find CAP21_STALE (after scan 1)

~~~
GET /data/api/v1/resources/find/ignition/database-translator/CAP21_STALE?collection=production @ 2026-10-13T11:58:01Z
HTTP/1.1 200 OK
Date: Tue, 13 Oct 2026 11:58:01 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 1540

{
  "type": "ignition/database-translator",
  "name": "CAP21_STALE",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "production",
  "collections": [
    "production"
  ],
  "signature": "89ecc80361b754ce1c4ba0be207842a2dc88fbccab2d2be7bccf8670bca8bd58",
  "config": {
    "createTable": "CREATE TABLE {tablename} ({creationdef}{primarykeydef})",
    "createIndex": "CREATE INDEX {indexname} ON {tablename}({columnname})",
    "autoIncTypeDef": "SERIAL NOT NULL",
    "alterTable": "ALTER TABLE {tablename} {alterdef}",
    "alterTableColumnDef": "ADD COLUMN {columnname} {type}",
    "primaryKeyDef": "PRIMARY KEY ({columnname})",
    "limit": "LIMIT {limit}",
    "limitClausePosition": "Back",
    "currentTimeQuery": "SELECT CURRENT_TIMESTAMP",
    "columnQuoteChar": "\"",
    "supportsRGK": true,
    "fetchKeyQuery": "",
    "tableListFilter": "",
    "i1Type": "int",
    "i2Type": "int",
    "i4Type": "int",
    "i8Type": "bigint",
    "boolType": "int",
    "datetimeType": "timestamp",
    "r4Type": "float",
    "r8Type": "double precision",
    "stringType": "varchar(255)",
    "blobType": "bytea",
    "textType": "text"
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "lastModification": {
      "actor": "external",
      "timestamp": "2026-10-13T11:58:00Z"
    },
    "uuid": "1caa175b-6703-89d8-8ead-c9b7c6bdfb96",
    "lastModificationSignature": "07012e7b0680489721a7bc757617f1fb2a27506dbed2a874826ed33d5158a9e8",
    "enabled": true
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.014977s]
~~~

### sha256 of both hand-written resource.json (after scan 1)

~~~
$ shasum -a 256 production/ignition/database-translator/CAP21_V8/resource.json production/ignition/database-translator/CAP21_STALE/resource.json @ 2026-10-13T11:58:01Z
912be19e176ff6a34838796882c578872b4df1712873f35423145d99930c13ee  production/ignition/database-translator/CAP21_V8/resource.json
bb5d201ecabd705c90573e3b49bd65ee3571fccb6cbb50b42ec4f2c1e9c2de31  production/ignition/database-translator/CAP21_STALE/resource.json
[exit 0]
~~~

### POST scan/config

~~~
POST /data/api/v1/scan/config @ 2026-10-13T11:59:01Z
HTTP/1.1 200 OK
Date: Tue, 13 Oct 2026 11:59:01 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1791892680906,
  "lastScanDuration": 768
}
[curl http=200 total=0.032915s]
~~~

### GET scan/config (after completion)

~~~
GET /data/api/v1/scan/config @ 2026-10-13T11:59:03Z
HTTP/1.1 200 OK
Date: Tue, 13 Oct 2026 11:59:03 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1791892742794,
  "lastScanDuration": 820
}
[curl http=200 total=0.014626s]
~~~

### sha256 of both hand-written resource.json (after scan 2, 60 s later)

~~~
$ shasum -a 256 production/ignition/database-translator/CAP21_V8/resource.json production/ignition/database-translator/CAP21_STALE/resource.json @ 2026-10-13T11:59:03Z
912be19e176ff6a34838796882c578872b4df1712873f35423145d99930c13ee  production/ignition/database-translator/CAP21_V8/resource.json
bb5d201ecabd705c90573e3b49bd65ee3571fccb6cbb50b42ec4f2c1e9c2de31  production/ignition/database-translator/CAP21_STALE/resource.json
[exit 0]
~~~

### resource.json bytes after scan 2: CAP21_V8

~~~
$ cat production/ignition/database-translator/CAP21_V8/resource.json @ 2026-10-13T11:59:03Z
{
  "scope": "A",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "config.json"
  ],
  "attributes": {
    "uuid": "4516ad9c-d6b0-8d86-89b9-802c96e33a9e",
    "enabled": true
  }
}
[output did not end with a newline]
[exit 0]
~~~

### resource.json bytes after scan 2: CAP21_STALE

~~~
$ cat production/ignition/database-translator/CAP21_STALE/resource.json @ 2026-10-13T11:59:03Z
{
  "scope": "A",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "config.json"
  ],
  "attributes": {
    "lastModification": {
      "actor": "cap21",
      "timestamp": "2026-01-01T00:00:00Z"
    },
    "uuid": "1caa175b-6703-89d8-8ead-c9b7c6bdfb96",
    "lastModificationSignature": "0000000000000000000000000000000000000000000000000000000000000000",
    "enabled": true
  }
}
[output did not end with a newline]
[exit 0]
~~~

### gateway log lines matching: CAP21_V8|CAP21_STALE|will not be loaded|signature

~~~
$ bash -c docker\ logs\ \"\$1\"\ 2\>\&1\ \|\ grep\ -E\ --\ \"\$2\"\ \|\|\ true _ ign-deploy-rig-gw-1 CAP21_V8\|CAP21_STALE\|will\ not\ be\ loaded\|signature @ 2026-10-13T11:59:03Z
[exit 0]
~~~

## C21-C4 — uuid/scope census for types absent from the corpus: oauth2-client, alarm-notification-profile, roster-config, holiday, service-connector

### POST create ignition/oauth2-client CAP21_OAUTH (minimal config, no secret values)

~~~
POST /data/api/v1/resources/ignition/oauth2-client @ 2026-10-13T11:59:09Z
HTTP/1.1 422 Unprocessable Entity
Date: Tue, 13 Oct 2026 11:59:09 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 103

{
  "messages": [
    "Missing required property \u0027clientSecret\u0027"
  ],
  "fieldMessages": []
}
[curl http=422 total=0.087440s]
~~~

### POST create com.inductiveautomation.alarm-notification/alarm-notification-profile CAP21_ANP (minimal config, no secret values)

~~~
POST /data/api/v1/resources/com.inductiveautomation.alarm-notification/alarm-notification-profile @ 2026-10-13T11:59:09Z
HTTP/1.1 200 OK
Date: Tue, 13 Oct 2026 11:59:09 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 303

{
  "success": true,
  "changes": [
    {
      "name": "CAP21_ANP",
      "type": "com.inductiveautomation.alarm-notification/alarm-notification-profile",
      "collection": "core",
      "newSignature": "9accf5245a7152f817f5ce6c5c60bbed7c6318484c9fa2d046d5a23ad9aa9925"
    }
  ],
  "problem": null
}
[curl http=200 total=0.138547s]
~~~

### CAP21_ANP files as the gateway wrote them

~~~
$ bash -c find\ .\ -path\ \"\*/\$1/\$2/\*\"\ -type\ f\ -exec\ shasum\ -a\ 256\ \{\}\ +\ \|\ sort\ -k2 _ alarm-notification-profile CAP21_ANP @ 2026-10-13T11:59:09Z
b20241273ba4f11f4f5b75d55c2802b7e6bcda9613c59418c889b9fcfe7595af  ./core/com.inductiveautomation.alarm-notification/alarm-notification-profile/CAP21_ANP/config.json
77fa405ba87fa1ba417c03bbcef231055d4c9d300c8617e61c456bc9b7825da4  ./core/com.inductiveautomation.alarm-notification/alarm-notification-profile/CAP21_ANP/resource.json
[exit 0]
~~~

### CAP21_ANP resource.json as the gateway wrote it

~~~
$ cat core/com.inductiveautomation.alarm-notification/alarm-notification-profile/CAP21_ANP/resource.json @ 2026-10-13T11:59:09Z
{
  "scope": "A",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "config.json"
  ],
  "attributes": {
    "uuid": "987d6ce2-7372-4274-a897-90cd648e6bf1",
    "enabled": true
  }
}
[output did not end with a newline]
[exit 0]
~~~

### POST create ignition/roster-config CAP21_ROSTER (minimal config, no secret values)

~~~
POST /data/api/v1/resources/ignition/roster-config @ 2026-10-13T11:59:09Z
HTTP/1.1 200 OK
Date: Tue, 13 Oct 2026 11:59:09 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 259

{
  "success": true,
  "changes": [
    {
      "name": "CAP21_ROSTER",
      "type": "ignition/roster-config",
      "collection": "core",
      "newSignature": "fcf89411070d0c0c56bebdda108d81ec91b2142b8f042ad2df488264590dfdcb"
    }
  ],
  "problem": null
}
[curl http=200 total=0.047599s]
~~~

### CAP21_ROSTER files as the gateway wrote them

~~~
$ bash -c find\ .\ -path\ \"\*/\$1/\$2/\*\"\ -type\ f\ -exec\ shasum\ -a\ 256\ \{\}\ +\ \|\ sort\ -k2 _ roster-config CAP21_ROSTER @ 2026-10-13T11:59:09Z
ea31094351e10f4b5e82509e36145b6efe551d814bace6e6b1516de10bfd8c4e  ./core/ignition/roster-config/CAP21_ROSTER/config.json
cbd351d2dda7169ad3d4c295f6926fdf35803de75c38f7634ec3584232d4b450  ./core/ignition/roster-config/CAP21_ROSTER/resource.json
[exit 0]
~~~

### CAP21_ROSTER resource.json as the gateway wrote it

~~~
$ cat core/ignition/roster-config/CAP21_ROSTER/resource.json @ 2026-10-13T11:59:10Z
{
  "scope": "A",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "config.json"
  ],
  "attributes": {
    "uuid": "642ad71b-72eb-4da6-92b0-014d01529510",
    "enabled": true
  }
}
[output did not end with a newline]
[exit 0]
~~~

### POST create ignition/holiday CAP21_HOLIDAY (minimal config, no secret values)

~~~
POST /data/api/v1/resources/ignition/holiday @ 2026-10-13T11:59:10Z
HTTP/1.1 200 OK
Date: Tue, 13 Oct 2026 11:59:10 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 254

{
  "success": true,
  "changes": [
    {
      "name": "CAP21_HOLIDAY",
      "type": "ignition/holiday",
      "collection": "core",
      "newSignature": "4824426775f7b72fc566a9c5792c36f5e81d6b13d080d2a202b953f7fb551c91"
    }
  ],
  "problem": null
}
[curl http=200 total=0.042806s]
~~~

### CAP21_HOLIDAY files as the gateway wrote them

~~~
$ bash -c find\ .\ -path\ \"\*/\$1/\$2/\*\"\ -type\ f\ -exec\ shasum\ -a\ 256\ \{\}\ +\ \|\ sort\ -k2 _ holiday CAP21_HOLIDAY @ 2026-10-13T11:59:10Z
7792a1383963ad973cd03220831ef26ba066a741e2b222da5fe66bdc196eaa79  ./core/ignition/holiday/CAP21_HOLIDAY/config.json
5f9ea186fd990eee79f7910e3953c9c4fdd8d85d4ae0b0b27340a7bc11daa76a  ./core/ignition/holiday/CAP21_HOLIDAY/resource.json
[exit 0]
~~~

### CAP21_HOLIDAY resource.json as the gateway wrote it

~~~
$ cat core/ignition/holiday/CAP21_HOLIDAY/resource.json @ 2026-10-13T11:59:10Z
{
  "scope": "A",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "config.json"
  ],
  "attributes": {
    "uuid": "fd65f158-2ad4-4fc1-b781-9a35cd25198e",
    "enabled": true
  }
}
[output did not end with a newline]
[exit 0]
~~~

### POST create ignition/service-connector CAP21_SVC (minimal config, no secret values)

~~~
POST /data/api/v1/resources/ignition/service-connector @ 2026-10-13T11:59:10Z
HTTP/1.1 200 OK
Date: Tue, 13 Oct 2026 11:59:10 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 260

{
  "success": true,
  "changes": [
    {
      "name": "CAP21_SVC",
      "type": "ignition/service-connector",
      "collection": "core",
      "newSignature": "88fae75f0296cfa48a76a5aad5b6c786a8e41eaa8114d11715753b0907904ff4"
    }
  ],
  "problem": null
}
[curl http=200 total=0.085574s]
~~~

### CAP21_SVC files as the gateway wrote them

~~~
$ bash -c find\ .\ -path\ \"\*/\$1/\$2/\*\"\ -type\ f\ -exec\ shasum\ -a\ 256\ \{\}\ +\ \|\ sort\ -k2 _ service-connector CAP21_SVC @ 2026-10-13T11:59:10Z
2c86b275e4ffb5e74943308b89c5418175b79ef540b2a962f30bcb242d85f3f1  ./core/ignition/service-connector/CAP21_SVC/config.json
594ac12a43f3688ae675fab0b940bb3cafdd4bc07c5c4c1cf20c7616fff896b2  ./core/ignition/service-connector/CAP21_SVC/resource.json
[exit 0]
~~~

### CAP21_SVC resource.json as the gateway wrote it

~~~
$ cat core/ignition/service-connector/CAP21_SVC/resource.json @ 2026-10-13T11:59:10Z
{
  "scope": "A",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "config.json"
  ],
  "attributes": {
    "uuid": "dd9fbce4-cd53-49bc-95be-4447dc0e85b2",
    "enabled": true
  }
}
[output did not end with a newline]
[exit 0]
~~~

## C21-OQ8 — Referenced secret: the gateway-written key order of {type, data{providerName, secretName}}

### throwaway secret value generated now, piped into a file inside the disposable rig container (value never printed or stored on the host)

~~~
$ bash -c openssl\ rand\ -hex\ 16\ \|\ docker\ exec\ -i\ -u\ root\ \"\$1\"\ sh\ -c\ \"cat\ \>\ /tmp/cap21-secret\ \&\&\ chown\ 2003:2003\ /tmp/cap21-secret\ \&\&\ chmod\ 600\ /tmp/cap21-secret\" _ ign-deploy-rig-gw-1 @ 2026-10-13T11:59:18Z
[exit 0]
~~~

### POST create secret-provider CAP21_SP_INT (internal, no secrets)

~~~
POST /data/api/v1/resources/ignition/secret-provider @ 2026-10-13T11:59:18Z
HTTP/1.1 200 OK
Date: Tue, 13 Oct 2026 11:59:18 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 261

{
  "success": true,
  "changes": [
    {
      "name": "CAP21_SP_INT",
      "type": "ignition/secret-provider",
      "collection": "core",
      "newSignature": "654f2bef94a0c93d022d7c9982ab46eec775b7fe4d39aed0795b2353e18a770b"
    }
  ],
  "problem": null
}
[curl http=200 total=0.238231s]
~~~

### POST create secret-provider CAP21_SP_FILE (file provider holding cap21-secret)

~~~
POST /data/api/v1/resources/ignition/secret-provider @ 2026-10-13T11:59:18Z
HTTP/1.1 200 OK
Date: Tue, 13 Oct 2026 11:59:18 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 262

{
  "success": true,
  "changes": [
    {
      "name": "CAP21_SP_FILE",
      "type": "ignition/secret-provider",
      "collection": "core",
      "newSignature": "c8aba6562a2be9925f08943d541e92b355faee031aa2501ea81ab3bc7e7cf490"
    }
  ],
  "problem": null
}
[curl http=200 total=0.051438s]
~~~

### GET secret names of CAP21_SP_FILE

~~~
GET /data/api/v1/secret-providers/CAP21_SP_FILE/secrets @ 2026-10-13T11:59:18Z
HTTP/1.1 200 OK
Date: Tue, 13 Oct 2026 11:59:18 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Transfer-Encoding: chunked

{"secrets":["cap21-secret"]}
[curl http=200 total=0.008317s]
~~~

### POST create database-connection CAP21_REF (Referenced password; type first, secretName before providerName)

~~~
POST /data/api/v1/resources/ignition/database-connection @ 2026-10-13T11:59:18Z
HTTP/1.1 200 OK
Date: Tue, 13 Oct 2026 11:59:18 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 262

{
  "success": true,
  "changes": [
    {
      "name": "CAP21_REF",
      "type": "ignition/database-connection",
      "collection": "core",
      "newSignature": "acf6fd1131da0822ed32aaa08a34a93353d9e7059d5b5a2672ea5323f87c297d"
    }
  ],
  "problem": null
}
[curl http=200 total=0.243388s]
~~~

### CAP21_REF config.json as the gateway wrote it

~~~
$ cat core/ignition/database-connection/CAP21_REF/config.json @ 2026-10-13T11:59:19Z
{
  "connectURL": "jdbc:postgresql://mode-host:5432/spike",
  "connectionProps": "",
  "connectionResetParams": "",
  "defaultTransactionLevel": "DEFAULT",
  "driver": "PostgreSQL",
  "evictionRate": -1,
  "evictionTests": 3,
  "evictionTime": 1800000,
  "failoverMode": "STANDARD",
  "includeSchemaInTableName": false,
  "password": {
    "data": {
      "providerName": "CAP21_SP_FILE",
      "secretName": "cap21-secret"
    },
    "type": "Referenced"
  },
  "poolInitSize": 0,
  "poolMaxActive": 8,
  "poolMaxIdle": 8,
  "poolMaxWait": 5000,
  "poolMinIdle": 0,
  "slowQueryLogThreshold": 60000,
  "testOnBorrow": true,
  "testOnReturn": false,
  "testWhileIdle": false,
  "translator": "POSTGRES",
  "username": "spike",
  "validationQuery": "SELECT 1",
  "validationSleepTime": 10000
}
[output did not end with a newline]
[exit 0]
~~~

### POST create database-connection CAP21_REF2 (Referenced password; data first, providerName before secretName)

~~~
POST /data/api/v1/resources/ignition/database-connection @ 2026-10-13T11:59:19Z
HTTP/1.1 200 OK
Date: Tue, 13 Oct 2026 11:59:19 GMT
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
      "name": "CAP21_REF2",
      "type": "ignition/database-connection",
      "collection": "core",
      "newSignature": "ceb34f704656388a5d4f0f9cb5e93c785f6858741ba6a7e980b1bc86c197a25c"
    }
  ],
  "problem": null
}
[curl http=200 total=0.135596s]
~~~

### CAP21_REF2 config.json as the gateway wrote it

~~~
$ cat core/ignition/database-connection/CAP21_REF2/config.json @ 2026-10-13T11:59:19Z
{
  "connectURL": "jdbc:postgresql://mode-host:5432/spike",
  "connectionProps": "",
  "connectionResetParams": "",
  "defaultTransactionLevel": "DEFAULT",
  "driver": "PostgreSQL",
  "evictionRate": -1,
  "evictionTests": 3,
  "evictionTime": 1800000,
  "failoverMode": "STANDARD",
  "includeSchemaInTableName": false,
  "password": {
    "data": {
      "providerName": "CAP21_SP_FILE",
      "secretName": "cap21-secret"
    },
    "type": "Referenced"
  },
  "poolInitSize": 0,
  "poolMaxActive": 8,
  "poolMaxIdle": 8,
  "poolMaxWait": 5000,
  "poolMinIdle": 0,
  "slowQueryLogThreshold": 60000,
  "testOnBorrow": true,
  "testOnReturn": false,
  "testWhileIdle": false,
  "translator": "POSTGRES",
  "username": "spike",
  "validationQuery": "SELECT 1",
  "validationSleepTime": 10000
}
[output did not end with a newline]
[exit 0]
~~~

## C21-C4 — oauth2-client retried with a Referenced clientSecret (no secret value)

### POST create ignition/oauth2-client CAP21_OAUTH (minimal config, no secret values)

~~~
POST /data/api/v1/resources/ignition/oauth2-client @ 2026-10-13T11:59:29Z
HTTP/1.1 200 OK
Date: Tue, 13 Oct 2026 11:59:29 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 258

{
  "success": true,
  "changes": [
    {
      "name": "CAP21_OAUTH",
      "type": "ignition/oauth2-client",
      "collection": "core",
      "newSignature": "26fd775be66d54ec439e2a6332e23a268d0f44467c82e6c1b86f6b100705fcc7"
    }
  ],
  "problem": null
}
[curl http=200 total=0.140236s]
~~~

### CAP21_OAUTH files as the gateway wrote them

~~~
$ bash -c find\ .\ -path\ \"\*/\$1/\$2/\*\"\ -type\ f\ -exec\ shasum\ -a\ 256\ \{\}\ +\ \|\ sort\ -k2 _ oauth2-client CAP21_OAUTH @ 2026-10-13T11:59:29Z
7f63a804fe6f885a6fe4709419cf3688dca4b0341604bc395568405ecc6df46a  ./core/ignition/oauth2-client/CAP21_OAUTH/config.json
19f9f95550a0c889702c135c1ecb60da8205c75b6213dd0f59c5c73742fc4cb1  ./core/ignition/oauth2-client/CAP21_OAUTH/resource.json
[exit 0]
~~~

### CAP21_OAUTH resource.json as the gateway wrote it

~~~
$ cat core/ignition/oauth2-client/CAP21_OAUTH/resource.json @ 2026-10-13T11:59:29Z
{
  "scope": "A",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "config.json"
  ],
  "attributes": {
    "uuid": "4173d915-6eb4-4caa-b4b7-c6c5cec111e4",
    "enabled": true
  }
}
[output did not end with a newline]
[exit 0]
~~~

## C21-OQ1 — gateway-written bytes of tag-provider, tag-group, tag-type-definition and tag-definition files for provider CAP21

### import file for provider CAP21 (script-written input)

~~~
$ bash -c cat\ \"\$IGN21_DIR/oq1-import.json\" @ 2026-10-13T12:04:15Z
[
  {"name": "_types_", "tagType": "Folder", "tags": [
    {"name": "CAP21_Type", "tagType": "UdtType", "tags": [
      {"name": "M1", "tagType": "AtomicTag", "dataType": "Int4", "valueSource": "memory", "value": 1}
    ]}
  ]},
  {"name": "FolderA", "tagType": "Folder", "tags": [
    {"name": "F1", "tagType": "AtomicTag", "dataType": "Float8", "valueSource": "memory", "value": 1.5},
    {"name": "F2", "tagType": "AtomicTag", "dataType": "Float8", "valueSource": "memory", "value": 10000000000.0},
    {"name": "BigInt", "tagType": "AtomicTag", "dataType": "Int8", "valueSource": "memory", "value": 9007199254740993},
    {"name": "Unicode", "tagType": "AtomicTag", "dataType": "String", "valueSource": "memory", "value": "Température ✓"},
    {"name": "Expr", "tagType": "AtomicTag", "dataType": "Boolean", "valueSource": "expr", "expression": "1 < 2 && 3 > 2 && 'a' = 'a'"},
    {"name": "Inst1", "tagType": "UdtInstance", "typeId": "CAP21_Type"}
  ]}
]
[exit 0]
~~~

### ign tags provider create CAP21

~~~
$ ign tags provider create CAP21 --profile rig21 --json @ 2026-10-13T12:04:15Z
{
  "ok": true,
  "profile": "rig21",
  "data": {
    "name": "CAP21"
  }
}
[exit 0]
~~~

### ign tags import into CAP21

~~~
$ bash -c ign\ tags\ import\ --profile\ rig21\ --provider\ CAP21\ --file\ \"\$IGN21_DIR/oq1-import.json\"\ --json @ 2026-10-13T12:04:15Z
{
  "ok": true,
  "profile": "rig21",
  "data": {
    "project": "ign-cli",
    "provider": "CAP21",
    "collision_policy": "abort",
    "imported": 2,
    "format": "json",
    "top_level_names": [
      "_types_",
      "FolderA"
    ],
    "loss_facts": [],
    "failed": []
  }
}
[exit 0]
~~~

### file list for CAP21 with sizes (bytes)

~~~
$ bash -c find\ core/ignition/tag-provider/CAP21\ core/ignition/tag-group/CAP21\ core/ignition/tag-type-definition/CAP21\ core/ignition/tag-definition/CAP21\ -type\ f\ 2\>/dev/null\ \|\ LC_ALL=C\ sort\ \|\ while\ IFS=\ read\ -r\ f\;\ do\ printf\ \"%s\ %s\\n\"\ \"\$\(wc\ -c\ \<\ \"\$f\"\ \|\ tr\ -d\ \"\ \"\)\"\ \"\$f\"\;\ done @ 2026-10-13T12:04:30Z
790 core/ignition/tag-definition/CAP21/FolderA/tags.json
89 core/ignition/tag-definition/CAP21/FolderA/udts.json
171 core/ignition/tag-definition/CAP21/FolderA/unary-resource.json
135 core/ignition/tag-definition/CAP21/unary-resource.json
199 core/ignition/tag-group/CAP21/Default Historical/config.json
136 core/ignition/tag-group/CAP21/Default Historical/resource.json
198 core/ignition/tag-group/CAP21/Default/config.json
136 core/ignition/tag-group/CAP21/Default/resource.json
307 core/ignition/tag-provider/CAP21/config.json
211 core/ignition/tag-provider/CAP21/resource.json
233 core/ignition/tag-type-definition/CAP21/udts.json
154 core/ignition/tag-type-definition/CAP21/unary-resource.json
[exit 0]
~~~

### file list for CAP21 with sha256

~~~
$ bash -c find\ core/ignition/tag-provider/CAP21\ core/ignition/tag-group/CAP21\ core/ignition/tag-type-definition/CAP21\ core/ignition/tag-definition/CAP21\ -type\ f\ 2\>/dev/null\ \|\ LC_ALL=C\ sort\ \|\ while\ IFS=\ read\ -r\ f\;\ do\ shasum\ -a\ 256\ \"\$f\"\;\ done @ 2026-10-13T12:04:30Z
69b77cfab736e4d83c0471f295ec014b7fa6df60b47949ca29776b527cf4a8d4  core/ignition/tag-definition/CAP21/FolderA/tags.json
9c4aea4975b04d2147e902a34b96bb4bdca3c5fc825673f3dd5d6c359f5e1cfe  core/ignition/tag-definition/CAP21/FolderA/udts.json
e6ca38f4553c5c6f6a292cf6df3541ddce4036550056ae611738cc12d56c4e97  core/ignition/tag-definition/CAP21/FolderA/unary-resource.json
4e76fe7dd0d0ee1da9fe7be0fb8a8dca8027f4430bbfec579b8f478556a0cf20  core/ignition/tag-definition/CAP21/unary-resource.json
3681179aa816d7860708c77fb0c8364e648fc7c203ed637117b71384ff677c41  core/ignition/tag-group/CAP21/Default Historical/config.json
6171900ae01a3a5d8fa7dda5fdc07283a260008caaec82f475d12147e593ba64  core/ignition/tag-group/CAP21/Default Historical/resource.json
2cbb0ed18730c5572182dd61952a0dbe271908f9aaef4779d54f6ab2b3409b4b  core/ignition/tag-group/CAP21/Default/config.json
6171900ae01a3a5d8fa7dda5fdc07283a260008caaec82f475d12147e593ba64  core/ignition/tag-group/CAP21/Default/resource.json
302f76fcbc0cf1f1f3f00cf5f1eed4a8acfd188f84286f0b754340be17243e55  core/ignition/tag-provider/CAP21/config.json
79650d22b461bb8f654f7ea9b9e882e548367d8b627ba774735e3f095bb38b31  core/ignition/tag-provider/CAP21/resource.json
70ad0242e32f2dbea3a9c9311943a9af1fb217934eb98a24208b48b6ba0b5657  core/ignition/tag-type-definition/CAP21/udts.json
16d0f629e68eb94a5114668d97d16337320175ac606f9ed01260808ef90d483e  core/ignition/tag-type-definition/CAP21/unary-resource.json
[exit 0]
~~~

### bytes of core/ignition/tag-definition/CAP21/FolderA/tags.json

~~~
$ cat core/ignition/tag-definition/CAP21/FolderA/tags.json @ 2026-10-13T12:04:31Z
[
  {
    "valueSource": "memory",
    "dataType": "Int8",
    "name": "BigInt",
    "defaultValue": 9007199254740993,
    "tagType": "AtomicTag"
  },
  {
    "valueSource": "memory",
    "dataType": "String",
    "name": "Unicode",
    "defaultValue": "Température ✓",
    "tagType": "AtomicTag"
  },
  {
    "valueSource": "expr",
    "expression": "1 \u003c 2 \u0026\u0026 3 \u003e 2 \u0026\u0026 \u0027a\u0027 \u003d \u0027a\u0027",
    "dataType": "Boolean",
    "name": "Expr",
    "tagType": "AtomicTag"
  },
  {
    "valueSource": "memory",
    "dataType": "Float8",
    "name": "F1",
    "defaultValue": 1.5,
    "tagType": "AtomicTag"
  },
  {
    "valueSource": "memory",
    "dataType": "Float8",
    "name": "F2",
    "defaultValue": 1.0E10,
    "tagType": "AtomicTag"
  }
]
[output did not end with a newline]
[exit 0]
~~~

### bytes of core/ignition/tag-definition/CAP21/FolderA/udts.json

~~~
$ cat core/ignition/tag-definition/CAP21/FolderA/udts.json @ 2026-10-13T12:04:31Z
[
  {
    "name": "Inst1",
    "typeId": "CAP21_Type",
    "tagType": "UdtInstance"
  }
]
[output did not end with a newline]
[exit 0]
~~~

### bytes of core/ignition/tag-definition/CAP21/FolderA/unary-resource.json

~~~
$ cat core/ignition/tag-definition/CAP21/FolderA/unary-resource.json @ 2026-10-13T12:04:31Z
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

### bytes of core/ignition/tag-definition/CAP21/unary-resource.json

~~~
$ cat core/ignition/tag-definition/CAP21/unary-resource.json @ 2026-10-13T12:04:31Z
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
[output did not end with a newline]
[exit 0]
~~~

### bytes of core/ignition/tag-group/CAP21/Default Historical/config.json

~~~
$ cat core/ignition/tag-group/CAP21/Default\ Historical/config.json @ 2026-10-13T12:04:31Z
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
[output did not end with a newline]
[exit 0]
~~~

### bytes of core/ignition/tag-group/CAP21/Default Historical/resource.json

~~~
$ cat core/ignition/tag-group/CAP21/Default\ Historical/resource.json @ 2026-10-13T12:04:31Z
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
[output did not end with a newline]
[exit 0]
~~~

### bytes of core/ignition/tag-group/CAP21/Default/config.json

~~~
$ cat core/ignition/tag-group/CAP21/Default/config.json @ 2026-10-13T12:04:31Z
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
[output did not end with a newline]
[exit 0]
~~~

### bytes of core/ignition/tag-group/CAP21/Default/resource.json

~~~
$ cat core/ignition/tag-group/CAP21/Default/resource.json @ 2026-10-13T12:04:31Z
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
[output did not end with a newline]
[exit 0]
~~~

### bytes of core/ignition/tag-provider/CAP21/config.json

~~~
$ cat core/ignition/tag-provider/CAP21/config.json @ 2026-10-13T12:04:31Z
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
[output did not end with a newline]
[exit 0]
~~~

### bytes of core/ignition/tag-provider/CAP21/resource.json

~~~
$ cat core/ignition/tag-provider/CAP21/resource.json @ 2026-10-13T12:04:31Z
{
  "scope": "A",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "config.json"
  ],
  "attributes": {
    "uuid": "c6e25805-b786-4d44-ae77-6fe6685410fb",
    "enabled": true
  }
}
[output did not end with a newline]
[exit 0]
~~~

### bytes of core/ignition/tag-type-definition/CAP21/udts.json

~~~
$ cat core/ignition/tag-type-definition/CAP21/udts.json @ 2026-10-13T12:04:31Z
[
  {
    "name": "CAP21_Type",
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
[output did not end with a newline]
[exit 0]
~~~

### bytes of core/ignition/tag-type-definition/CAP21/unary-resource.json

~~~
$ cat core/ignition/tag-type-definition/CAP21/unary-resource.json @ 2026-10-13T12:04:31Z
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

### copy gateway-written tag-provider/CAP21 into fixtures/deploy/captured (cp -R, bytes unchanged)

~~~
$ bash -c mkdir\ -p\ \"\$IGN21_CAPTURED/ignition/\$1\"\ \&\&\ cp\ -R\ \"core/ignition/\$1/CAP21\"\ \"\$IGN21_CAPTURED/ignition/\$1/\" _ tag-provider @ 2026-10-13T12:04:31Z
[exit 0]
~~~

### copy gateway-written tag-group/CAP21 into fixtures/deploy/captured (cp -R, bytes unchanged)

~~~
$ bash -c mkdir\ -p\ \"\$IGN21_CAPTURED/ignition/\$1\"\ \&\&\ cp\ -R\ \"core/ignition/\$1/CAP21\"\ \"\$IGN21_CAPTURED/ignition/\$1/\" _ tag-group @ 2026-10-13T12:04:31Z
[exit 0]
~~~

### copy gateway-written tag-type-definition/CAP21 into fixtures/deploy/captured (cp -R, bytes unchanged)

~~~
$ bash -c mkdir\ -p\ \"\$IGN21_CAPTURED/ignition/\$1\"\ \&\&\ cp\ -R\ \"core/ignition/\$1/CAP21\"\ \"\$IGN21_CAPTURED/ignition/\$1/\" _ tag-type-definition @ 2026-10-13T12:04:31Z
[exit 0]
~~~

### copy gateway-written tag-definition/CAP21 into fixtures/deploy/captured (cp -R, bytes unchanged)

~~~
$ bash -c mkdir\ -p\ \"\$IGN21_CAPTURED/ignition/\$1\"\ \&\&\ cp\ -R\ \"core/ignition/\$1/CAP21\"\ \"\$IGN21_CAPTURED/ignition/\$1/\" _ tag-definition @ 2026-10-13T12:04:31Z
[exit 0]
~~~

## C21-OQ4 — sorted find listing of every path the gateway wrote for provider CAP21 (repo-path to collection-path mapping)

### find listing, whole bind directory, paths naming CAP21 as a provider directory

~~~
$ bash -c find\ .\ \\\(\ -path\ \"\*/tag-\*/CAP21\"\ -o\ -path\ \"\*/tag-\*/CAP21/\*\"\ \\\)\ \|\ LC_ALL=C\ sort @ 2026-10-13T12:04:36Z
./core/ignition/tag-definition/CAP21
./core/ignition/tag-definition/CAP21/FolderA
./core/ignition/tag-definition/CAP21/FolderA/tags.json
./core/ignition/tag-definition/CAP21/FolderA/udts.json
./core/ignition/tag-definition/CAP21/FolderA/unary-resource.json
./core/ignition/tag-definition/CAP21/unary-resource.json
./core/ignition/tag-group/CAP21
./core/ignition/tag-group/CAP21/Default
./core/ignition/tag-group/CAP21/Default Historical
./core/ignition/tag-group/CAP21/Default Historical/config.json
./core/ignition/tag-group/CAP21/Default Historical/resource.json
./core/ignition/tag-group/CAP21/Default/config.json
./core/ignition/tag-group/CAP21/Default/resource.json
./core/ignition/tag-provider/CAP21
./core/ignition/tag-provider/CAP21/config.json
./core/ignition/tag-provider/CAP21/resource.json
./core/ignition/tag-type-definition/CAP21
./core/ignition/tag-type-definition/CAP21/udts.json
./core/ignition/tag-type-definition/CAP21/unary-resource.json
[exit 0]
~~~

### tag-related directories of the core collection (one level)

~~~
$ bash -c ls\ -1\ core/ignition\ \|\ grep\ -E\ \"\^tag-\"\ \|\ LC_ALL=C\ sort @ 2026-10-13T12:04:36Z
tag-definition
tag-group
tag-provider
tag-type-definition
[exit 0]
~~~

### ign tags provider list (provider names and tag counts)

~~~
$ ign tags provider list --profile rig21 --json @ 2026-10-13T12:04:36Z
{
  "ok": true,
  "profile": "rig21",
  "data": {
    "providers": [
      {
        "name": "CAP21",
        "enabled": true,
        "tag_count": null,
        "health": null,
        "managed": false
      },
      {
        "name": "System",
        "enabled": true,
        "tag_count": null,
        "health": null,
        "managed": true
      },
      {
        "name": "default",
        "enabled": true,
        "tag_count": null,
        "health": null,
        "managed": false
      }
    ]
  }
}
[exit 0]
~~~

## C21-C5 — hostile names (colon, question mark, trailing dot, space, comma, Case/case) imported into provider CAP21H; on-disk names on Linux

### ign tags provider create CAP21H

~~~
$ ign tags provider create CAP21H --profile rig21 --json @ 2026-10-13T12:04:36Z
{
  "ok": true,
  "profile": "rig21",
  "data": {
    "name": "CAP21H"
  }
}
[exit 0]
~~~

### ign tags import folder named 'a:b' into CAP21H

~~~
$ bash -c printf\ \"%s\"\ \"\$1\"\ \|\ ign\ tags\ import\ --profile\ rig21\ --provider\ CAP21H\ --file\ -\ --json _ \[\{\"name\":\"a:b\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\] @ 2026-10-13T12:04:36Z
{
  "ok": true,
  "profile": "rig21",
  "data": {
    "project": "ign-cli",
    "provider": "CAP21H",
    "collision_policy": "abort",
    "imported": 1,
    "format": "json",
    "top_level_names": [
      "a:b"
    ],
    "loss_facts": [],
    "failed": []
  }
}
[exit 0]
~~~

### ign tags import folder named 'q?' into CAP21H

~~~
$ bash -c printf\ \"%s\"\ \"\$1\"\ \|\ ign\ tags\ import\ --profile\ rig21\ --provider\ CAP21H\ --file\ -\ --json _ \[\{\"name\":\"q\?\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\] @ 2026-10-13T12:04:36Z
{
  "ok": true,
  "profile": "rig21",
  "data": {
    "project": "ign-cli",
    "provider": "CAP21H",
    "collision_policy": "abort",
    "imported": 1,
    "format": "json",
    "top_level_names": [
      "q?"
    ],
    "loss_facts": [],
    "failed": []
  }
}
[exit 0]
~~~

### ign tags import folder named 'trail.' into CAP21H

~~~
$ bash -c printf\ \"%s\"\ \"\$1\"\ \|\ ign\ tags\ import\ --profile\ rig21\ --provider\ CAP21H\ --file\ -\ --json _ \[\{\"name\":\"trail.\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\] @ 2026-10-13T12:04:36Z
{
  "ok": true,
  "profile": "rig21",
  "data": {
    "project": "ign-cli",
    "provider": "CAP21H",
    "collision_policy": "abort",
    "imported": 1,
    "format": "json",
    "top_level_names": [
      "trail."
    ],
    "loss_facts": [],
    "failed": []
  }
}
[exit 0]
~~~

### ign tags import folder named 'has space' into CAP21H

~~~
$ bash -c printf\ \"%s\"\ \"\$1\"\ \|\ ign\ tags\ import\ --profile\ rig21\ --provider\ CAP21H\ --file\ -\ --json _ \[\{\"name\":\"has\ space\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\] @ 2026-10-13T12:04:37Z
{
  "ok": true,
  "profile": "rig21",
  "data": {
    "project": "ign-cli",
    "provider": "CAP21H",
    "collision_policy": "abort",
    "imported": 1,
    "format": "json",
    "top_level_names": [
      "has space"
    ],
    "loss_facts": [],
    "failed": []
  }
}
[exit 0]
~~~

### ign tags import folder named 'com,ma' into CAP21H

~~~
$ bash -c printf\ \"%s\"\ \"\$1\"\ \|\ ign\ tags\ import\ --profile\ rig21\ --provider\ CAP21H\ --file\ -\ --json _ \[\{\"name\":\"com\,ma\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\] @ 2026-10-13T12:04:37Z
{
  "ok": true,
  "profile": "rig21",
  "data": {
    "project": "ign-cli",
    "provider": "CAP21H",
    "collision_policy": "abort",
    "imported": 1,
    "format": "json",
    "top_level_names": [
      "com,ma"
    ],
    "loss_facts": [],
    "failed": []
  }
}
[exit 0]
~~~

### ign tags import sibling folders 'Case' and 'case' into CAP21H (one import)

~~~
$ bash -c printf\ \"%s\"\ \"\$1\"\ \|\ ign\ tags\ import\ --profile\ rig21\ --provider\ CAP21H\ --file\ -\ --json _ \[\{\"name\":\"Case\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\,\{\"name\":\"case\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T2\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":2\}\]\}\] @ 2026-10-13T12:04:37Z
{
  "ok": true,
  "profile": "rig21",
  "data": {
    "project": "ign-cli",
    "provider": "CAP21H",
    "collision_policy": "abort",
    "imported": 2,
    "format": "json",
    "top_level_names": [
      "Case",
      "case"
    ],
    "loss_facts": [],
    "failed": []
  }
}
[exit 0]
~~~

### on-disk names under every CAP21H tag directory, repr() of each entry

~~~
$ python3 -I -c $'\nimport os\nout = []\nfor kind in ("tag-provider", "tag-group", "tag-type-definition", "tag-definition"):\n    base = os.path.join("core/ignition", kind, "CAP21H")\n    for root, dirs, files in os.walk(base):\n        for n in dirs + files:\n            out.append(os.path.relpath(os.path.join(root, n), "core/ignition"))\nfor p in sorted(out):\n    print(repr(p))\n' @ 2026-10-13T12:04:52Z
'tag-definition/CAP21H/Case'
'tag-definition/CAP21H/Case/tags.json'
'tag-definition/CAP21H/Case/unary-resource.json'
'tag-definition/CAP21H/a%3ab'
'tag-definition/CAP21H/a%3ab/tags.json'
'tag-definition/CAP21H/a%3ab/unary-resource.json'
'tag-definition/CAP21H/has space'
'tag-definition/CAP21H/has space/tags.json'
'tag-definition/CAP21H/has space/unary-resource.json'
'tag-definition/CAP21H/unary-resource.json'
'tag-group/CAP21H/Default'
'tag-group/CAP21H/Default Historical'
'tag-group/CAP21H/Default Historical/config.json'
'tag-group/CAP21H/Default Historical/resource.json'
'tag-group/CAP21H/Default/config.json'
'tag-group/CAP21H/Default/resource.json'
'tag-provider/CAP21H/config.json'
'tag-provider/CAP21H/resource.json'
'tag-type-definition/CAP21H/unary-resource.json'
[exit 0]
~~~

## C21-C5 — gateway verdicts for the CAP21H imports (log) and the on-disk names again after a longer wait

### on-disk names under every CAP21H tag directory, repr() of each entry (second look)

~~~
$ python3 -I -c $'\nimport os\nout = []\nfor kind in ("tag-provider", "tag-group", "tag-type-definition", "tag-definition"):\n    base = os.path.join("core/ignition", kind, "CAP21H")\n    for root, dirs, files in os.walk(base):\n        for n in dirs + files:\n            out.append(os.path.relpath(os.path.join(root, n), "core/ignition"))\nfor p in sorted(out):\n    print(repr(p))\n' @ 2026-10-13T12:06:05Z
'tag-definition/CAP21H/Case'
'tag-definition/CAP21H/Case/tags.json'
'tag-definition/CAP21H/Case/unary-resource.json'
'tag-definition/CAP21H/a%3ab'
'tag-definition/CAP21H/a%3ab/tags.json'
'tag-definition/CAP21H/a%3ab/unary-resource.json'
'tag-definition/CAP21H/has space'
'tag-definition/CAP21H/has space/tags.json'
'tag-definition/CAP21H/has space/unary-resource.json'
'tag-definition/CAP21H/unary-resource.json'
'tag-group/CAP21H/Default'
'tag-group/CAP21H/Default Historical'
'tag-group/CAP21H/Default Historical/config.json'
'tag-group/CAP21H/Default Historical/resource.json'
'tag-group/CAP21H/Default/config.json'
'tag-group/CAP21H/Default/resource.json'
'tag-provider/CAP21H/config.json'
'tag-provider/CAP21H/resource.json'
'tag-type-definition/CAP21H/unary-resource.json'
[exit 0]
~~~

### gateway log lines matching: CAP21H.*(Error processing edit|not a valid tag name|already exists)|Error processing edit for tag path '\[CAP21H\]

~~~
$ bash -c docker\ logs\ \"\$1\"\ 2\>\&1\ \|\ grep\ -E\ --\ \"\$2\"\ \|\|\ true _ ign-deploy-rig-gw-1 CAP21H.\*\(Error\ processing\ edit\|not\ a\ valid\ tag\ name\|already\ exists\)\|Error\ processing\ edit\ for\ tag\ path\ \'\\\[CAP21H\\\] @ 2026-10-13T12:06:05Z
jvm 1    | 2026/10/13 12:04:36 | E [t.m.provider                  ] [12:04:36.968]: Error processing edit for tag path '[CAP21H]q?': Error_Configuration("The name 'q?' is not a valid tag name") provider=CAP21H
jvm 1    | 2026/10/13 12:04:37 | E [t.m.provider                  ] [12:04:37.028]: Error processing edit for tag path '[CAP21H]trail.': Error_Configuration("The name 'trail.' is not a valid tag name") provider=CAP21H
jvm 1    | 2026/10/13 12:04:37 | E [t.m.provider                  ] [12:04:37.147]: Error processing edit for tag path '[CAP21H]com,ma': Error_Configuration("The name 'com,ma' is not a valid tag name") provider=CAP21H
jvm 1    | 2026/10/13 12:04:37 | E [t.m.provider                  ] [12:04:37.217]: Error processing edit for tag path '[CAP21H]case': Bad_Failure("Tag '[CAP21H]case' already exists, and 'abort' collision policy has been specified") 
[exit 0]
~~~

### bytes of core/ignition/tag-definition/CAP21H/Case/tags.json

~~~
$ cat core/ignition/tag-definition/CAP21H/Case/tags.json @ 2026-10-13T12:06:05Z
[
  {
    "valueSource": "memory",
    "dataType": "Int4",
    "name": "T1",
    "defaultValue": 1,
    "tagType": "AtomicTag"
  }
]
[output did not end with a newline]
[exit 0]
~~~

### bytes of core/ignition/tag-definition/CAP21H/a%3ab/tags.json

~~~
$ cat core/ignition/tag-definition/CAP21H/a%3ab/tags.json @ 2026-10-13T12:06:05Z
[
  {
    "valueSource": "memory",
    "dataType": "Int4",
    "name": "T1",
    "defaultValue": 1,
    "tagType": "AtomicTag"
  }
]
[output did not end with a newline]
[exit 0]
~~~

## C21-C5 — extra probes in CAP21H: Windows-illegal characters (* | " < >) and a literal a%3ab beside the encoded a:b

### ign tags import folder named 'a*b' into CAP21H

~~~
$ bash -c printf\ \"%s\"\ \"\$1\"\ \|\ ign\ tags\ import\ --profile\ rig21\ --provider\ CAP21H\ --file\ -\ --json _ \[\{\"name\":\"a\*b\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\] @ 2026-10-13T12:06:05Z
{
  "ok": true,
  "profile": "rig21",
  "data": {
    "project": "ign-cli",
    "provider": "CAP21H",
    "collision_policy": "abort",
    "imported": 1,
    "format": "json",
    "top_level_names": [
      "a*b"
    ],
    "loss_facts": [],
    "failed": []
  }
}
[exit 0]
~~~

### ign tags import folder named 'a|b' into CAP21H

~~~
$ bash -c printf\ \"%s\"\ \"\$1\"\ \|\ ign\ tags\ import\ --profile\ rig21\ --provider\ CAP21H\ --file\ -\ --json _ \[\{\"name\":\"a\|b\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\] @ 2026-10-13T12:06:05Z
{
  "ok": true,
  "profile": "rig21",
  "data": {
    "project": "ign-cli",
    "provider": "CAP21H",
    "collision_policy": "abort",
    "imported": 1,
    "format": "json",
    "top_level_names": [
      "a|b"
    ],
    "loss_facts": [],
    "failed": []
  }
}
[exit 0]
~~~

### ign tags import folder named 'a"b' into CAP21H

~~~
$ bash -c printf\ \"%s\"\ \"\$1\"\ \|\ ign\ tags\ import\ --profile\ rig21\ --provider\ CAP21H\ --file\ -\ --json _ \[\{\"name\":\"a\\\"b\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\] @ 2026-10-13T12:06:05Z
{
  "ok": true,
  "profile": "rig21",
  "data": {
    "project": "ign-cli",
    "provider": "CAP21H",
    "collision_policy": "abort",
    "imported": 1,
    "format": "json",
    "top_level_names": [
      "a\"b"
    ],
    "loss_facts": [],
    "failed": []
  }
}
[exit 0]
~~~

### ign tags import folder named 'a<b' into CAP21H

~~~
$ bash -c printf\ \"%s\"\ \"\$1\"\ \|\ ign\ tags\ import\ --profile\ rig21\ --provider\ CAP21H\ --file\ -\ --json _ \[\{\"name\":\"a\<b\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\] @ 2026-10-13T12:06:05Z
{
  "ok": true,
  "profile": "rig21",
  "data": {
    "project": "ign-cli",
    "provider": "CAP21H",
    "collision_policy": "abort",
    "imported": 1,
    "format": "json",
    "top_level_names": [
      "a<b"
    ],
    "loss_facts": [],
    "failed": []
  }
}
[exit 0]
~~~

### ign tags import folder named 'a>b' into CAP21H

~~~
$ bash -c printf\ \"%s\"\ \"\$1\"\ \|\ ign\ tags\ import\ --profile\ rig21\ --provider\ CAP21H\ --file\ -\ --json _ \[\{\"name\":\"a\>b\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\] @ 2026-10-13T12:06:05Z
{
  "ok": true,
  "profile": "rig21",
  "data": {
    "project": "ign-cli",
    "provider": "CAP21H",
    "collision_policy": "abort",
    "imported": 1,
    "format": "json",
    "top_level_names": [
      "a>b"
    ],
    "loss_facts": [],
    "failed": []
  }
}
[exit 0]
~~~

### ign tags import folder named 'a%3ab' into CAP21H

~~~
$ bash -c printf\ \"%s\"\ \"\$1\"\ \|\ ign\ tags\ import\ --profile\ rig21\ --provider\ CAP21H\ --file\ -\ --json _ \[\{\"name\":\"a%3ab\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\] @ 2026-10-13T12:06:05Z
{
  "ok": true,
  "profile": "rig21",
  "data": {
    "project": "ign-cli",
    "provider": "CAP21H",
    "collision_policy": "abort",
    "imported": 1,
    "format": "json",
    "top_level_names": [
      "a%3ab"
    ],
    "loss_facts": [],
    "failed": []
  }
}
[exit 0]
~~~

### on-disk names under every CAP21H tag directory, repr() of each entry (after the extra probes)

~~~
$ python3 -I -c $'\nimport os\nout = []\nfor kind in ("tag-provider", "tag-group", "tag-type-definition", "tag-definition"):\n    base = os.path.join("core/ignition", kind, "CAP21H")\n    for root, dirs, files in os.walk(base):\n        for n in dirs + files:\n            out.append(os.path.relpath(os.path.join(root, n), "core/ignition"))\nfor p in sorted(out):\n    print(repr(p))\n' @ 2026-10-13T12:06:30Z
'tag-definition/CAP21H/Case'
'tag-definition/CAP21H/Case/tags.json'
'tag-definition/CAP21H/Case/unary-resource.json'
'tag-definition/CAP21H/a%3ab'
'tag-definition/CAP21H/a%3ab/tags.json'
'tag-definition/CAP21H/a%3ab/unary-resource.json'
'tag-definition/CAP21H/has space'
'tag-definition/CAP21H/has space/tags.json'
'tag-definition/CAP21H/has space/unary-resource.json'
'tag-definition/CAP21H/unary-resource.json'
'tag-group/CAP21H/Default'
'tag-group/CAP21H/Default Historical'
'tag-group/CAP21H/Default Historical/config.json'
'tag-group/CAP21H/Default Historical/resource.json'
'tag-group/CAP21H/Default/config.json'
'tag-group/CAP21H/Default/resource.json'
'tag-provider/CAP21H/config.json'
'tag-provider/CAP21H/resource.json'
'tag-type-definition/CAP21H/unary-resource.json'
[exit 0]
~~~

### gateway log lines matching: Error processing edit for tag path '\[CAP21H\]

~~~
$ bash -c docker\ logs\ \"\$1\"\ 2\>\&1\ \|\ grep\ -E\ --\ \"\$2\"\ \|\|\ true _ ign-deploy-rig-gw-1 Error\ processing\ edit\ for\ tag\ path\ \'\\\[CAP21H\\\] @ 2026-10-13T12:06:31Z
jvm 1    | 2026/10/13 12:04:36 | E [t.m.provider                  ] [12:04:36.968]: Error processing edit for tag path '[CAP21H]q?': Error_Configuration("The name 'q?' is not a valid tag name") provider=CAP21H
jvm 1    | 2026/10/13 12:04:37 | E [t.m.provider                  ] [12:04:37.028]: Error processing edit for tag path '[CAP21H]trail.': Error_Configuration("The name 'trail.' is not a valid tag name") provider=CAP21H
jvm 1    | 2026/10/13 12:04:37 | E [t.m.provider                  ] [12:04:37.147]: Error processing edit for tag path '[CAP21H]com,ma': Error_Configuration("The name 'com,ma' is not a valid tag name") provider=CAP21H
jvm 1    | 2026/10/13 12:04:37 | E [t.m.provider                  ] [12:04:37.217]: Error processing edit for tag path '[CAP21H]case': Bad_Failure("Tag '[CAP21H]case' already exists, and 'abort' collision policy has been specified") 
jvm 1    | 2026/10/13 12:06:05 | E [t.m.provider                  ] [12:06:05.428]: Error processing edit for tag path '[CAP21H]a*b': Error_Configuration("The name 'a*b' is not a valid tag name") provider=CAP21H
jvm 1    | 2026/10/13 12:06:05 | E [t.m.provider                  ] [12:06:05.527]: Error processing edit for tag path '[CAP21H]a|b': Error_Configuration("The name 'a|b' is not a valid tag name") provider=CAP21H
jvm 1    | 2026/10/13 12:06:05 | E [t.m.provider                  ] [12:06:05.589]: Error processing edit for tag path '[CAP21H]a"b': Error_Configuration("The name 'a"b' is not a valid tag name") provider=CAP21H
jvm 1    | 2026/10/13 12:06:05 | E [t.m.provider                  ] [12:06:05.656]: Error processing edit for tag path '[CAP21H]a<b': Error_Configuration("The name 'a<b' is not a valid tag name") provider=CAP21H
jvm 1    | 2026/10/13 12:06:05 | E [t.m.provider                  ] [12:06:05.792]: Error processing edit for tag path '[CAP21H]a>b': Error_Configuration("The name 'a>b' is not a valid tag name") provider=CAP21H
jvm 1    | 2026/10/13 12:06:05 | E [t.m.provider                  ] [12:06:05.952]: Error processing edit for tag path '[CAP21H]a%3ab': Error_Configuration("The name 'a%3ab' is not a valid tag name") provider=CAP21H
[exit 0]
~~~

## C21-C4 — side effect of creating a database-connection: the gateway-written store-and-forward-engine resource (scope G, no uuid in the corpus)

### store-and-forward-engine files written for CAP21_REF

~~~
$ bash -c find\ .\ -path\ \"\*/store-and-forward-engine/CAP21_REF/\*\"\ -type\ f\ \|\ LC_ALL=C\ sort @ 2026-10-13T12:07:21Z
./core/ignition/store-and-forward-engine/CAP21_REF/config.json
./core/ignition/store-and-forward-engine/CAP21_REF/resource.json
[exit 0]
~~~

### bytes of core/ignition/store-and-forward-engine/CAP21_REF/resource.json

~~~
$ cat core/ignition/store-and-forward-engine/CAP21_REF/resource.json @ 2026-10-13T12:07:21Z
{
  "scope": "A",
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": [
    "config.json"
  ],
  "attributes": {
    "lastModification": {
      "actor": "unknown",
      "timestamp": "2026-10-13T11:59:19Z"
    },
    "uuid": "5f2894fc-2d20-48c0-bab1-388e81f9595e",
    "lastModificationSignature": "9b67360fee0f7f617db70fdfa3bcd1a6f129d7be4d8d9e99c51724bedb34d68b"
  }
}
[output did not end with a newline]
[exit 0]
~~~

### bytes of core/ignition/store-and-forward-engine/CAP21_REF/config.json

~~~
$ cat core/ignition/store-and-forward-engine/CAP21_REF/config.json @ 2026-10-13T12:07:21Z
{
  "batchSize": 10000,
  "dataThreshold": 10000,
  "forwardRateMs": 1000,
  "forwardingPolicy": "ALL",
  "isThirdParty": false,
  "primaryMaintenancePolicy": {
    "action": "PREVENT_NEW_DATA",
    "limit": {
      "limitType": "COUNT",
      "value": 0
    }
  },
  "scanRateMs": 100,
  "secondaryMaintenancePolicy": {
    "action": "PREVENT_NEW_DATA",
    "limit": {
      "limitType": "COUNT",
      "value": 0
    }
  },
  "timeThresholdMs": 30000
}
[output did not end with a newline]
[exit 0]
~~~

## Decisions

Written only after `capture.sh check21` exited 0 for all seven ids (UUID8, C3b, C4, OQ8, OQ1, OQ4, C5). Every Decision cites the capture sections it rests on and states where a capture contradicts 21-RESEARCH; 21-05, 21-06, 21-07 and 21-08 follow the capture. Nothing below was taken from research, inference or a previous phase.

### CD1 — A sha2-derived version-8 uuid loads on 8.3.6 (D-03 frozen as researched)

Rests on `C21-UUID8` (blocks "v8 uuid for CAP21_V8 (D-03 derivation)", "CAP21_V8 resource.json (hand-written, before scan)", "GET find CAP21_V8 (collection=production)", "GET names (database-translator)", "CAP21_V8 sha256 (after scan)", and the gateway log block).

- The D-03 derivation for gateway `ign-deploy-rig`, module `ignition`, type `database-translator`, name `CAP21_V8` is **`4516ad9c-d6b0-8d86-89b9-802c96e33a9e`** (the version nibble is `8`, the variant nibble is `8`). This is the literal 21-07 pins in `name_based_uuid_matches_live_vector`.
- Loaded: yes. After one scan the `find?collection=production` read-back answered 200 with `"collection":"production"`, `"enabled":true` and `"attributes":{"uuid":"4516ad9c-d6b0-8d86-89b9-802c96e33a9e","enabled":true}`, and `names` listed `CAP21_V8` with `"modes":["production"]`.
- The gateway log held no line naming `CAP21_V8` and none saying "will not be loaded" (the grep block ends with only `[exit 0]`).
- The hand-written `resource.json` was not rewritten: sha256 `912be19e176ff6a34838796882c578872b4df1712873f35423145d99930c13ee` before and after the scan.
- Confirms RESEARCH assumption A1 (v8 uuid loads); no fallback to `sha1`/UUIDv5 is needed. `21-07` may freeze the derivation. For comparison, the gateway's own API-created resources carry random version-4 uuids (`CAP21_API`: `be605da6-baa2-44f3-b292-4739baf58421`); only the one v8 id was tried, and it loaded.

### CD2 — Omitted `lastModification`/`lastModificationSignature` loads; a stale signature loads too and is not flagged or rewritten

Rests on `C21-C3b` (all blocks) and `C21-UUID8` (for the omitted case).

- Omitted (`CAP21_V8`, `attributes` = `uuid`, `enabled` only): loads (find 200), the API view carries no `lastModification` either, and the file sha256 is `912be19e...13ee` before the scan, after scan 1 and after scan 2 (60 s later). The gateway does not rewrite it.
- Stale (`CAP21_STALE`: `lastModification` {actor `cap21`, timestamp `2026-01-01T00:00:00Z`} and a `lastModificationSignature` of 64 zeros): loads (find 200, `"enabled":true`). The find read-back shows a gateway-computed view, not the file's values: `lastModification` {actor `external`, timestamp `2026-10-13T11:58:00Z`} and `lastModificationSignature` `07012e7b0680489721a7bc757617f1fb2a27506dbed2a874826ed33d5158a9e8`. The signature in the file is neither validated nor flagged.
- The stale file is not rewritten either: sha256 `bb5d201ecabd705c90573e3b49bd65ee3571fccb6cbb50b42ec4f2c1e9c2de31` at all three points, and the file bytes after scan 2 still show the zeros and the `cap21` actor.
- No gateway log line named either resource (the final log block holds only `[exit 0]`).
- Consequence for 21-07: a synthesized `resource.json` needs no `lastModification` or `lastModificationSignature`, and render may keep an existing signature verbatim without recomputing it. Confirms RESEARCH A1 (absent `lastModification` accepted).

### CD3 — Rule-table facts per type attempted in C4 (every new named type is scope `A` with a uuid; the store-and-forward side effect contradicts the corpus)

Rests on `C21-C4` (three sections: the five creates, the oauth2-client retry, and the store-and-forward side effect) and `C21-UUID8` (database-translator).

All gateway-written `resource.json` files below have the same shape and key order: `scope`, `version` (1), `restricted` (false), `overridable` (true), `files` (`["config.json"]`), `attributes`; no `description`; created through the config API with no `collection`, so they landed in `core`.

| Type (module/type) | Outcome | scope | uuid | `attributes` key order |
| --- | --- | --- | --- | --- |
| `ignition/database-translator` (`CAP21_API`) | created | `A` | present, version 4 | `uuid`, `enabled` |
| `ignition/oauth2-client` (`CAP21_OAUTH`) | `clientSecret: null` refused 422 `Missing required property 'clientSecret'`; created with a Referenced `clientSecret` | `A` | present, version 4 | `uuid`, `enabled` |
| `com.inductiveautomation.alarm-notification/alarm-notification-profile` (`CAP21_ANP`) | created (`OneWayNotificationProfileType`, empty `settings`) | `A` | present, version 4 | `uuid`, `enabled` |
| `ignition/roster-config` (`CAP21_ROSTER`) | created (`users: []`) | `A` | present, version 4 | `uuid`, `enabled` |
| `ignition/holiday` (`CAP21_HOLIDAY`) | created | `A` | present, version 4 | `uuid`, `enabled` |
| `ignition/service-connector` (`CAP21_SVC`) | created (kafka profile, PLAINTEXT) | `A` | present, version 4 | `uuid`, `enabled` |
| `ignition/store-and-forward-engine` (`CAP21_REF`, created by the gateway as a side effect of a `database-connection` create) | written by the gateway | `A` | present, version 4 | `lastModification` {actor `unknown`}, `uuid`, `lastModificationSignature` (no `enabled`) |

- None of the five types the research left open is a singleton: each took a name and got a uuid, so each gets a "named, with uuid" row.
- For `oauth2-client` a `null` secret-typed field is refused at create time (422) while a Referenced value is accepted; a render that blanked a required secret to `null` would be refused by the gateway for that type.
- The tag types are not named-resource rows in the same sense; their `scope`/`attributes` are in CD5.
- Contradicts RESEARCH Pattern 4 on one point: the corpus census says `store-and-forward-engine` carries scope `G` and no uuid (0 of 5), but the gateway-written resource on 8.3.6 carries scope `A`, a uuid and a `lastModification`. The corpus files come from a gateway that predates this layout, so `21-07` must not infer "named store-and-forward never has a uuid" from the gateway; the corpus-derived `NamedNoUuid` row stays valid only as "do not synthesize one". Whether a gateway loads a store-and-forward or themes resource with no uuid at all was not tested (CD2 omitted only `lastModification` and the signature, on a `database-translator` that kept its uuid).
- Confirms RESEARCH A2 only partly: `A` goes with a uuid on every newly created named type here, while `G` appears on the unary tag resources (CD5).

### CD4 — Gateway-written tag file format (Gson pretty print, no trailing newline, HTML-safe escapes) and the identity-gate expectation for 21-05

Rests on `C21-OQ1` (the block "bytes of ..." for each of the twelve files, the sizes and sha256 lists, and the import file) and `C21-C5` (the second-section `Case`/`a%3ab` bytes).

- Pretty-printed with a 2-space indent, `": "` between key and value, one array element per line, `[]` and `{}` for empty array and object, JSON `null` kept (`tag-provider/CAP21/config.json` writes `"defaultDatasourceName": null`).
- No trailing newline in any captured file: every `bytes of ...` block carries the `[output did not end with a newline]` marker.
- Non-ASCII is written raw UTF-8: `"defaultValue": "Température ✓"` in `FolderA/tags.json`.
- HTML-safe escapes are applied to string values: the expression `1 < 2 && 3 > 2 && 'a' = 'a'` is written `1 < 2 && 3 > 2 && 'a' = 'a'` (`<`, `&`, `>`, `'`, `=` all escaped as lowercase `\u00xx`). A writer that does not mirror Gson's HTML-safe mode will not be byte-identical.
- Numbers: `1.5` stays `1.5`; the input lexeme `10000000000.0` is written `1.0E10`; the integer `9007199254740993` (above 2^53) is written exactly as `9007199254740993`. So the float lexeme is not preserved from input, it is Gson's `Double.toString` form, and big integers are not rounded.
- Key order inside a tag is not alphabetical and not the input order: `valueSource`, `dataType`, `name`, then `defaultValue` (or `expression`), then `tagType`; an atomic tag's input `value` was written as `defaultValue` in `tags.json`, while a UDT-type member kept `value` (`tag-type-definition/CAP21/udts.json`). The order of tags inside one `tags.json` also differs from the input order (`BigInt`, `Unicode`, `Expr`, `F1`, `F2` for an input of `F1`, `F2`, `BigInt`, `Unicode`, `Expr`), so render must treat tag-file member order as source data, not sort it.
- UDT instances are not in `tags.json`: `Inst1` is written to `FolderA/udts.json` beside it.
- Expectation for 21-05: all twelve captured files are Gson-identity candidates and none needs the opaque path. The discriminating file is `tag-definition/CAP21/FolderA/tags.json` (`1.0E10`, the `\u00xx` escapes, raw non-ASCII, an integer above 2^53). The `resource.json` and `unary-resource.json` files and the tag-group configs use the same writer and are plain identity cases.
- The gateway flushes tag files asynchronously: they appeared between 5 and 15 s after the import answered success, so any capture or test that reads them must wait or poll.

### CD5 — Repo-path to collection-path mapping for the four tag types (confirms RESEARCH A5)

Rests on `C21-OQ4` (the sorted `find` listing) and `C21-OQ1` (the per-file bytes).

For provider `CAP21` the gateway wrote, all under `core/` (no collection was passed), and mapped to the collection-relative path by dropping `core/`:

| Type | Collection path | Files |
| --- | --- | --- |
| `tag-provider` | `ignition/tag-provider/<P>/` | `config.json`, `resource.json` (scope `A`, `uuid` + `enabled`) |
| `tag-group` | `ignition/tag-group/<P>/<GroupName>/` | `config.json`, `resource.json` per group (scope `A`, `attributes` is `{}`); `Default` and `Default Historical` exist for every new provider |
| `tag-type-definition` | `ignition/tag-type-definition/<P>/` | `udts.json`, `unary-resource.json` (scope `G`, `files: ["udts.json"]`, `attributes: {"config": {}}`); the `_types_` folder of the import does not appear as a directory |
| `tag-definition` | `ignition/tag-definition/<P>/` and `<P>/<Folder>/…` | provider root: `unary-resource.json` only (`files: []`) when there are no root-level tags; each folder is a directory holding `tags.json` (its atomic tags), `udts.json` (its UDT instances, only when present) and `unary-resource.json` (scope `G`, `files` listing those) |

- The repo layout `tags/<P>/<type>/<rest>` therefore maps to `ignition/<type>/<P>/<rest>`: the type and provider segments swap, exactly as RESEARCH Pattern 3 and assumption A5 guessed, now confirmed for `tag-group` and `tag-type-definition` too.
- Folder nesting is directory nesting (`FolderA/` under `tag-definition/CAP21/`); atomic tags do not get their own directory, they are members of the folder's `tags.json`.
- Contradicts the research's implicit uniformity on one point: `tag-group` entries are named resources with their own `resource.json` (scope `A`), while `tag-type-definition` and `tag-definition` use `unary-resource.json` (scope `G`) with the data in `udts.json`/`tags.json`; 21-06 must map both file-name families.
- Not exercised, so 21-06 stays fail-closed on them: root-level tags (a `tags.json` at the provider root), UDT types nested in sub-folders of `_types_`, and a non-`core` collection.

### CD6 — Linux on-disk names for hostile tag names; NTFS deferred to Phase 22 (D-04)

Rests on `C21-C5` (all three sections: the plan's names, the gateway log and second listing, and the extra probes).

- `a:b` is accepted and its directory is **`a%3ab`** (the colon percent-encoded, lowercase hex).
- `has space` is accepted and written raw (`has space/`).
- `Case` is accepted and written raw; `case` is refused by the gateway as a collision (`Tag '[CAP21H]case' already exists`), so tag names are case-insensitive and only one of the pair exists on disk.
- `q?`, `trail.` and `com,ma` are refused by the gateway (`The name '…' is not a valid tag name`) after the import call itself answered success with `failed: []`, so no directory exists for them. The extra probes show the same refusal for `a*b`, `a|b`, `a"b`, `a<b`, `a>b` and the literal `a%3ab`. The gateway's own log is the only place the refusal shows; the CLI import envelope does not report it.
- Net effect: a gateway-written tag directory never contains `< > : " | ? *`; the colon is encoded and every other Windows-illegal character is refused as a tag name. Render stays fail-closed on Windows-illegal names (hand-authored trees can still contain them), and NTFS behaviour (reserved names, trailing space, path length) is **deferred to Phase 22 (D-04)**. No hostile-named file was copied into the repository.
- Resolves RESEARCH A7 (encoding unknown): the gateway encodes the colon and rejects the other hostile characters, rather than writing them raw.

### CD7 — `Referenced` secret key order: `data` before `type`, `providerName` before `secretName`, gateway-determined

Rests on `C21-OQ8` (blocks "CAP21_REF config.json as the gateway wrote it" and "CAP21_REF2 config.json as the gateway wrote it") and `C21-C4` (the oauth2-client retry).

- Two creates sent the same reference in opposite key orders (`CAP21_REF`: `type` first, `secretName` before `providerName`; `CAP21_REF2`: `data` first, `providerName` before `secretName`). Both were written identically: `"password": {"data": {"providerName": …, "secretName": …}, "type": "Referenced"}`. The order is therefore the gateway's own (alphabetical), not preserved from the request.
- This matches the Embedded form in the corpus (`data` first, `type` after) and RESEARCH Pattern 7. The render lint stays order-insensitive, as planned, but anything that rewrites a `Referenced` object must emit `data` then `type`.
- An internal secret provider with `secrets: {}` was created (200), but an internal secret cannot be created through the REST API without JWE ciphertext. The Referenced name was resolved through a file secret provider (`CAP21_SP_FILE`, secret `cap21-secret`); `GET secret-providers/CAP21_SP_FILE/secrets` listed the name. The throwaway value was written only inside the disposable rig container and is gone with the volume; it never reached the host or the repository. This differs from the plan's wording (an internal provider holding the secret) for that reason.
- The gateway accepted a Referenced `clientSecret` on `oauth2-client` where `null` was refused (CD3).
