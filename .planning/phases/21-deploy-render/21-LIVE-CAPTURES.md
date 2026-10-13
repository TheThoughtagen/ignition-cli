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
