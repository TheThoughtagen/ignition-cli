# Phase 22 — Live Captures (8.3.6 rig)

| Field | Value |
| --- | --- |
| Image | inductiveautomation/ignition:8.3.6 |
| Compose variant | bind (compose.yml + compose.bind.yml) |
| Bind path (outside repo) | $HOME/.cache/ign-deploy-rig/data/config/resources |
| Gateway name | ign-deploy-rig |
| deploymentMode | production |
| Started (UTC) | 2026-10-14T19:39:42Z |
| ign --version | ign 1.5.0 |

## CV1 — REST log entries after a scan loads a uuid-less named resource (research A6)

Resources are written into `production/` by a plain file write (no REST). Scans use the Phase 20 completion rule (scanActive false and lastScanTimestamp advanced). The log window start is computed by the script from the same scan answer.

### CV1a — a uuid-less database-translator (the tracer POSTGRES unit with attributes.uuid removed)

#### resource.json written (no uuid key)

`bash -c cat\ production/ignition/database-translator/CV1_NOUUID/resource.json ` @ 2026-10-14T20:00:09Z

~~~
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
      "actor": "8.3-migration",
      "timestamp": "2026-03-05T13:13:02Z"
    },
    "lastModificationSignature": "97226c4a1915d0fa6cade568e7138360fadad3476ea640598f597f9cefcb300c",
    "enabled": true
  }
}
[exit 0]
~~~

#### GET scan/config (before the scan)

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/scan/config'` @ 2026-10-14T20:00:09Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:09 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 91

{
  "scanActive": false,
  "lastScanTimestamp": 1792008001075,
  "lastScanDuration": 2046
}
[curl http=200 total=0.050991s]
~~~

#### POST scan/config

`curl -sS -g -i -X POST -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/scan/config'` @ 2026-10-14T20:00:10Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:10 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": true,
  "lastScanTimestamp": 1792008001075,
  "lastScanDuration": 2046
}
[curl http=200 total=0.058232s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/scan/config'` @ 2026-10-14T20:00:11Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:12 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 91

{
  "scanActive": false,
  "lastScanTimestamp": 1792008011345,
  "lastScanDuration": 1107
}
[curl http=200 total=0.066496s]
~~~

Scan of CV1a: lastScanTimestamp 1792008011345, lastScanDuration 1107, so the window start is 1792008011345 - 1107 - 1000 = 1792008009238 (all on the gateway clock). The script then waits 2 s before the first log read.

#### GET logs, all levels, window of the CV1a scan

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/logs?startTime=1792008009238&limit=200'` @ 2026-10-14T20:00:14Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:14 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 71

{"items":[],"metadata":{"total":0,"matching":0,"limit":200,"offset":0}}
[curl http=200 total=0.090863s]
~~~

#### GET resources/list/ignition/database-translator (is CV1_NOUUID loaded?)

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/ignition/database-translator?limit=-1'` @ 2026-10-14T20:00:14Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:14 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 9353

{"items":[{"type":"ignition/database-translator","name":"CV1_NOUUID","description":null,"enabled":true,"version":1,"collection":"production","collections":["production"],"signature":"5a167e2b8b902a1c7c119947aba0a16418785b62db0619d78e6ae39103931e3c","config":{"createTable":"CREATE TABLE {tablename} ({creationdef}{primarykeydef})","createIndex":"CREATE INDEX {indexname} ON {tablename}({columnname})","autoIncTypeDef":"SERIAL NOT NULL","alterTable":"ALTER TABLE {tablename} {alterdef}","alterTableColumnDef":"ADD COLUMN {columnname} {type}","primaryKeyDef":"PRIMARY KEY ({columnname})","limit":"LIMIT {limit}","limitClausePosition":"Back","currentTimeQuery":"SELECT CURRENT_TIMESTAMP","columnQuoteChar":"\"","supportsRGK":true,"fetchKeyQuery":"","tableListFilter":"","i1Type":"int","i2Type":"int","i4Type":"int","i8Type":"bigint","boolType":"int","datetimeType":"timestamp","r4Type":"float","r8Type":"double precision","stringType":"varchar(255)","blobType":"bytea","textType":"text"},"data":["config.json"],"attributes":{"lastModification":{"actor":"external","timestamp":"2026-10-14T20:00:11Z"},"lastModificationSignature":"0e4340ba0e10471be66f53bd014871f948f60d7ab5a82fee20de3bfd53edfd97","enabled":true},"metrics":{},"healthchecks":{}},{"type":"ignition/database-translator","name":"GENERIC","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"f266ce4f9efc59a3ce28cb3013b700c402ba89123ecad5b5c0bd9a7e9b5b68b6","config":{"createTable":"CREATE TABLE {tablename} ({creationdef}{primarykeydef})","createIndex":"CREATE INDEX {indexname} ON {tablename}({columnname})","autoIncTypeDef":"{type} NOT NULL AUTO_INCREMENT","alterTable":"ALTER TABLE {tablename} {alterdef}","alterTableColumnDef":"ADD COLUMN {columnname} {type}","primaryKeyDef":"PRIMARY KEY ({columnname})","limit":"LIMIT {limit}","limitClausePosition":"Back","currentTimeQuery":"SELECT CURRENT_TIMESTAMP","columnQuoteChar":"\"","supportsRGK":true,"fetchKeyQuery":"","tableListFilter":"","i1Type":"INT","i2Type":"INT","i4Type":"INT","i8Type":"BIGINT","boolType":"INT","datetimeType":"DATETIME","r4Type":"FLOAT","r8Type":"DOUBLE","stringType":"VARCHAR(255)","blobType":"VARBINARY","textType":""},"data":["config.json"],"attributes":{"lastModification":{"actor":"default","timestamp":"2026-10-14T19:42:45Z"},"uuid":"9ec01211-a1f2-4be6-9c40-675d0827a2ce","lastModificationSignature":"7b84b2b47e4dd395a8d5240475ab910d1fb29b1175100fe2c2b32dd17f34e747"},"metrics":{},"healthchecks":{}},{"type":"ignition/database-translator","name":"MSSQL","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"c450e559277a25737cad14d9f83b8cef538652da8227a7b38a2784eec555d499","config":{"createTable":"CREATE TABLE {tablename} ({creationdef}{primarykeydef})","createIndex":"CREATE INDEX {indexname} ON {tablename}({columnname})","autoIncTypeDef":"{type} IDENTITY(1, 1)","alterTable":"ALTER TABLE {tablename} ADD {alterdef}","alterTableColumnDef":"{columnname} {type}","primaryKeyDef":"PRIMARY KEY CLUSTERED ({columnname})","limit":"TOP {limit}","limitClausePosition":"Front","currentTimeQuery":"SELECT CURRENT_TIMESTAMP","columnQuoteChar":"\"","supportsRGK":true,"fetchKeyQuery":"","tableListFilter":"","i1Type":"INT","i2Type":"INT","i4Type":"INT","i8Type":"BIGINT","boolType":"INT","datetimeType":"DATETIME","r4Type":"FLOAT(10)","r8Type":"DOUBLE PRECISION","stringType":"NVARCHAR(255)","blobType":"VARBINARY","textType":"NVARCHAR(MAX)"},"data":["config.json"],"attributes":{"lastModification":{"actor":"default","timestamp":"2026-10-14T19:42:45Z"},"uuid":"d32915c9-b720-4378-9aeb-d57102c847a6","lastModificationSignature":"7dd0af31fc8fc080f17ac3aba0e00db249070c77d9cdc8ec3484ed8d151b6dc8"},"metrics":{},"healthchecks":{}},{"type":"ignition/database-translator","name":"MYSQL","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"48c9952e09aaec591f0ea0c1932f98133fdc39bd63d8175b4619e9bd457d1148","config":{"createTable":"CREATE TABLE {tablename} ({creationdef}{primarykeydef})","createIndex":"CREATE INDEX {indexname} ON {tablename}({columnname})","autoIncTypeDef":"{type} NOT NULL AUTO_INCREMENT","alterTable":"ALTER TABLE {tablename} {alterdef}","alterTableColumnDef":"ADD COLUMN {columnname} {type}","primaryKeyDef":"PRIMARY KEY ({columnname})","limit":"LIMIT {limit}","limitClausePosition":"Back","currentTimeQuery":"SELECT CURRENT_TIMESTAMP","columnQuoteChar":"`","supportsRGK":true,"fetchKeyQuery":"","tableListFilter":"","i1Type":"INT","i2Type":"INT","i4Type":"INT","i8Type":"BIGINT","boolType":"INT","datetimeType":"DATETIME","r4Type":"FLOAT","r8Type":"DOUBLE","stringType":"VARCHAR(255)","blobType":"VARBINARY","textType":"TEXT"},"data":["config.json"],"attributes":{"lastModification":{"actor":"default","timestamp":"2026-10-14T19:42:45Z"},"uuid":"614ac389-dc8f-4d8b-ab02-6f49f4a257f6","lastModificationSignature":"2a434e7d9fa1c17209c66fc46362e8f1739c1919cc46d35746b2132c17ec90ec"},"metrics":{},"healthchecks":{}},{"type":"ignition/database-translator","name":"ORACLE","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"dbeb07a0b9a18f270524b7b73ca12e6845e287be537af865bae3d2875a699ddc","config":{"createTable":"CREATE TABLE {tablename} ({creationdef}{primarykeydef})","createAutoIncSequence":"CREATE SEQUENCE {tablename}_seq START WITH 1 INCREMENT BY 1","createAutoIncTrigger":"CREATE TRIGGER {tablename}_trig \nBEFORE INSERT ON {tablename} \nREFERENCING NEW AS NEW\nFOR EACH ROW \nBEGIN \n    SELECT {tablename}_seq.NEXTVAL \n    INTO :NEW.{columnname} FROM DUAL;\nEND;","createIndex":"CREATE INDEX {indexname} ON {tablename}({columnname})","autoIncTypeDef":"{type} NOT NULL","alterTable":"ALTER TABLE {tablename} ADD ({alterdef})","alterTableColumnDef":"{columnname} {type}","primaryKeyDef":"PRIMARY KEY ({columnname})","limit":"ROWNUM <= {limit}","limitClausePosition":"Wrap","currentTimeQuery":"SELECT CURRENT_TIMESTAMP FROM DUAL","columnQuoteChar":"\"","supportsRGK":false,"fetchKeyQuery":"SELECT {tablename}_seq.CURRVAL FROM DUAL","tableListFilter":"SYS_INFO*;*SYSTEM*;WWV*;*$*;DBA*;LOGMNR*;ORDDCM*;APEX*;ALL_SA*;DATABASE*;DV_*;GSM*;HELP;MVIEW*;ORD_*;PRODUCT_PRIVS;REDO_*;SCHEDULER_*;SERVICE*;SI_*;SQLPLUS*;SYS_*;USER_SA*;VERIFY_HISTORY;VNCR;WLM_*;XML_*;CLOUD;REGION;CHANGE_LOG*","i1Type":"INT","i2Type":"INT","i4Type":"INT","i8Type":"INT","boolType":"INT","datetimeType":"TIMESTAMP","r4Type":"FLOAT","r8Type":"DOUBLE PRECISION","stringType":"VARCHAR2(255)","blobType":"VARBINARY","textType":"NCLOB"},"data":["config.json"],"attributes":{"lastModification":{"actor":"default","timestamp":"2026-10-14T19:42:45Z"},"uuid":"1f2db57a-bef9-49e6-aa71-ec1ae247edf8","lastModificationSignature":"2a0d4cf09c48c734448c2f067b0929cc8327b51f9217faa5627e96315a9e2afb"},"metrics":{},"healthchecks":{}},{"type":"ignition/database-translator","name":"POSTGRES","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"265f200b340ea386c6599659d07be21fd00dd6ec5115e1529cfb2ae697cdc090","config":{"createTable":"CREATE TABLE {tablename} ({creationdef}{primarykeydef})","createIndex":"CREATE INDEX {indexname} ON {tablename}({columnname})","autoIncTypeDef":"SERIAL NOT NULL","alterTable":"ALTER TABLE {tablename} {alterdef}","alterTableColumnDef":"ADD COLUMN {columnname} {type}","primaryKeyDef":"PRIMARY KEY ({columnname})","limit":"LIMIT {limit}","limitClausePosition":"Back","currentTimeQuery":"SELECT CURRENT_TIMESTAMP","columnQuoteChar":"\"","supportsRGK":true,"fetchKeyQuery":"","tableListFilter":"","i1Type":"INT","i2Type":"INT","i4Type":"INT","i8Type":"BIGINT","boolType":"INT","datetimeType":"TIMESTAMP","r4Type":"FLOAT","r8Type":"DOUBLE PRECISION","stringType":"VARCHAR(255)","blobType":"BYTEA","textType":"TEXT"},"data":["config.json"],"attributes":{"lastModification":{"actor":"default","timestamp":"2026-10-14T19:42:45Z"},"uuid":"29e085e2-1590-4baf-a6bc-d93a7fa4bb57","lastModificationSignature":"649022c7bb7f58887102155dbff707f6d40ea61083aad3a65e98cfd69622d789"},"metrics":{},"healthchecks":{}},{"type":"ignition/database-translator","name":"SQLITE","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"b0af97de81b8222763b6ce1b35e92d4444a0d7fb89e785e346d8b395b74783b5","config":{"createTable":"CREATE TABLE {tablename} ({creationdef}{primarykeydef})","createIndex":"CREATE INDEX {indexname} ON {tablename}({columnname})","autoIncTypeDef":"INTEGER PRIMARY KEY","alterTable":"ALTER TABLE {tablename} {alterdef}","alterTableColumnDef":"ADD COLUMN {columnname} {type}","primaryKeyDef":"","limit":"LIMIT {limit}","limitClausePosition":"Back","currentTimeQuery":"SELECT CURRENT_TIMESTAMP","columnQuoteChar":"\"","supportsRGK":true,"fetchKeyQuery":"","tableListFilter":"","i1Type":"INTEGER","i2Type":"INTEGER","i4Type":"INTEGER","i8Type":"INTEGER","boolType":"INTEGER","datetimeType":"TEXT","r4Type":"REAL","r8Type":"REAL","stringType":"TEXT","blobType":"BLOB","textType":"TEXT"},"data":["config.json"],"attributes":{"lastModification":{"actor":"default","timestamp":"2026-10-14T19:42:45Z"},"uuid":"9bdb8fc1-4ba8-49d7-89b6-20cef86487d9","lastModificationSignature":"a693ac5b875733bfa424ccaf69fa0f8fc552fe932028ba7817ac2df177beb9f2"},"metrics":{},"healthchecks":{}}],"metadata":{"total":7,"matching":7,"limit":-1,"offset":0}}
[curl http=200 total=0.088283s]
~~~

Script reading of the list above: CV1_NOUUID present in the REST view: yes.

### CV1b — a uuid-less database-connection (the type of the RIG.md line)

#### resource.json written (no uuid key)

`bash -c cat\ production/ignition/database-connection/CV1_NOUUID_DB/resource.json ` @ 2026-10-14T20:00:15Z

~~~
{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["config.json"],"attributes":{"enabled":true}}
[exit 0]
~~~

#### GET scan/config (before the scan)

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/scan/config'` @ 2026-10-14T20:00:15Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:15 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 91

{
  "scanActive": false,
  "lastScanTimestamp": 1792008011345,
  "lastScanDuration": 1107
}
[curl http=200 total=0.053461s]
~~~

#### POST scan/config

`curl -sS -g -i -X POST -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/scan/config'` @ 2026-10-14T20:00:15Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:15 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": true,
  "lastScanTimestamp": 1792008011345,
  "lastScanDuration": 1107
}
[curl http=200 total=0.084694s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/scan/config'` @ 2026-10-14T20:00:16Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:16 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1792008016235,
  "lastScanDuration": 509
}
[curl http=200 total=0.046328s]
~~~

Scan of CV1b: lastScanTimestamp 1792008016235, lastScanDuration 509, so the window start is 1792008016235 - 509 - 1000 = 1792008014726 (all on the gateway clock). The script then waits 2 s before the first log read.

#### GET logs minLevel=WARN (the query shape 22-10 sends)

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/logs?minLevel=WARN&startTime=1792008014726&limit=200'` @ 2026-10-14T20:00:18Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:18 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 598

{"items":[{"timestamp":1792008016254,"loggerName":"NamedResourceHandler-ignition/database-connection","level":"WARN","message":"Resource ignition/database-connection/CV1_NOUUID_DB does not have required UUID attribute, will not be loaded","mdc":{"collection":"production"}},{"timestamp":1792008016245,"loggerName":"NamedResourceHandler-ignition/database-connection","level":"WARN","message":"Resource ignition/database-connection/CV1_NOUUID_DB does not have required UUID attribute, will not be loaded","mdc":{"collection":"production"}}],"metadata":{"total":2,"matching":2,"limit":200,"offset":0}}
[curl http=200 total=0.033189s]
~~~

#### GET logs logger=NamedResourceHandler (the logger prefix filter), same window

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/logs?logger=NamedResourceHandler&startTime=1792008014726&limit=200'` @ 2026-10-14T20:00:18Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:18 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 71

{"items":[],"metadata":{"total":0,"matching":0,"limit":200,"offset":0}}
[curl http=200 total=0.015941s]
~~~

#### GET resources/list/ignition/database-connection (is CV1_NOUUID_DB loaded?)

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/ignition/database-connection?limit=-1'` @ 2026-10-14T20:00:19Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:19 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 964

{"items":[{"type":"ignition/database-connection","name":"CV1_NOUUID_DB","description":null,"enabled":true,"version":1,"collection":"production","collections":["production"],"signature":"c1ed32482911b0c63c571729a4bfa906959be5e11cf9809f5fdf4e32e227cd42","config":{"driver":"PostgreSQL","translator":"POSTGRES","includeSchemaInTableName":false,"connectURL":"jdbc:postgresql://mode-host:5432/spike","username":"spike","connectionProps":"","connectionResetParams":"","defaultTransactionLevel":"DEFAULT","poolInitSize":0,"poolMaxActive":8,"poolMaxIdle":8,"poolMinIdle":0,"poolMaxWait":5000,"validationQuery":"SELECT 1","testOnBorrow":true,"testOnReturn":false,"testWhileIdle":false,"evictionRate":-1,"evictionTests":3,"evictionTime":1800000,"failoverMode":"STANDARD","slowQueryLogThreshold":60000,"validationSleepTime":10000},"data":["config.json"],"attributes":{"enabled":true},"metrics":{},"healthchecks":{}}],"metadata":{"total":1,"matching":1,"limit":-1,"offset":0}}
[curl http=200 total=0.024289s]
~~~

Script reading of the list above: CV1_NOUUID_DB present in the REST view: yes.

### CV1c — a second scan with the resource unchanged and still uuid-less

#### GET scan/config (before the scan)

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/scan/config'` @ 2026-10-14T20:00:19Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:19 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1792008016235,
  "lastScanDuration": 509
}
[curl http=200 total=0.046628s]
~~~

#### POST scan/config

`curl -sS -g -i -X POST -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/scan/config'` @ 2026-10-14T20:00:19Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:19 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1792008016235,
  "lastScanDuration": 509
}
[curl http=200 total=0.026006s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/scan/config'` @ 2026-10-14T20:00:21Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:21 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1792008020569,
  "lastScanDuration": 938
}
[curl http=200 total=0.060203s]
~~~

Scan of CV1c: lastScanTimestamp 1792008020569, lastScanDuration 938, so the window start is 1792008020569 - 938 - 1000 = 1792008018631 (all on the gateway clock). The script then waits 2 s before the first log read.

#### GET logs minLevel=WARN, window of the CV1c scan

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/logs?minLevel=WARN&startTime=1792008018631&limit=200'` @ 2026-10-14T20:00:23Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:23 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 71

{"items":[],"metadata":{"total":0,"matching":0,"limit":200,"offset":0}}
[curl http=200 total=0.043706s]
~~~

The same query with the window opened at the start of the CV1b scan, so it covers both scans: it shows whether the second scan logged the resource again.

#### GET logs minLevel=WARN, window opened at the CV1b start (both scans)

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/logs?minLevel=WARN&startTime=1792008014726&limit=200'` @ 2026-10-14T20:00:23Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:23 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: no-cache, no-store
Pragma: no-cache
Content-Type: application/json
Content-Length: 598

{"items":[{"timestamp":1792008016254,"loggerName":"NamedResourceHandler-ignition/database-connection","level":"WARN","message":"Resource ignition/database-connection/CV1_NOUUID_DB does not have required UUID attribute, will not be loaded","mdc":{"collection":"production"}},{"timestamp":1792008016245,"loggerName":"NamedResourceHandler-ignition/database-connection","level":"WARN","message":"Resource ignition/database-connection/CV1_NOUUID_DB does not have required UUID attribute, will not be loaded","mdc":{"collection":"production"}}],"metadata":{"total":2,"matching":2,"limit":200,"offset":0}}
[curl http=200 total=0.023355s]
~~~

### CV1d — how long after "scan complete" the entry is readable, and where it is stamped

#### five uuid-less database-connections, one scan each, polling the logs every 50 ms

`bash -c bash\ \"\$CAPTURE22\"\ lagprobe\ 5 ` @ 2026-10-14T20:00:23Z

~~~
CV1_LAG_1: post_to_scan_complete_ms=1280 lastScanDuration=864 readable_after_complete_ms=516 entries=2 level=WARN entry_stamp_minus_lastScanTimestamp_ms=[126,124]
CV1_LAG_2: post_to_scan_complete_ms=763 lastScanDuration=453 readable_after_complete_ms=857 entries=2 level=WARN entry_stamp_minus_lastScanTimestamp_ms=[19,14]
CV1_LAG_3: post_to_scan_complete_ms=1276 lastScanDuration=1039 readable_after_complete_ms=644 entries=2 level=WARN entry_stamp_minus_lastScanTimestamp_ms=[55,51]
CV1_LAG_4: post_to_scan_complete_ms=1581 lastScanDuration=1212 readable_after_complete_ms=518 entries=2 level=WARN entry_stamp_minus_lastScanTimestamp_ms=[41,35]
CV1_LAG_5: post_to_scan_complete_ms=1287 lastScanDuration=902 readable_after_complete_ms=1450 entries=2 level=WARN entry_stamp_minus_lastScanTimestamp_ms=[17,13]
[exit 0]
~~~

### CV1e — cleanup: the CV1 resources removed again, then a scan

#### listing of production after the removal (empty directories pruned)

`bash -c find\ production\ -mindepth\ 1\ \|\ LC_ALL=C\ sort ` @ 2026-10-14T20:00:41Z

~~~
production/config-mode.json
production/ignition
[exit 0]
~~~

#### GET scan/config (before the scan)

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/scan/config'` @ 2026-10-14T20:00:43Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:43 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1792008039430,
  "lastScanDuration": 902
}
[curl http=200 total=0.041435s]
~~~

#### POST scan/config

`curl -sS -g -i -X POST -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/scan/config'` @ 2026-10-14T20:00:43Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:43 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1792008039430,
  "lastScanDuration": 902
}
[curl http=200 total=0.041791s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/scan/config'` @ 2026-10-14T20:00:46Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:46 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 91

{
  "scanActive": false,
  "lastScanTimestamp": 1792008045225,
  "lastScanDuration": 1518
}
[curl http=200 total=0.024335s]
~~~

## CV2 — REST effective-view config vs the file config.json, per non-tag type (research A7)

The tracer render is written into `production/` by a plain file write, one scan, then every non-tag `module/type` present in `core` or `production` on the bind directory is read back through the REST list (named types) or singleton route. Only the differing JSON paths are recorded, never values.

#### tracer render files placed in production/

`bash -c find\ production\ -type\ f\ \|\ LC_ALL=C\ sort ` @ 2026-10-14T20:00:48Z

~~~
production/config-mode.json
production/ignition/database-translator/POSTGRES/config.json
production/ignition/database-translator/POSTGRES/resource.json
production/ignition/general-alarm-settings/config.json
production/ignition/general-alarm-settings/resource.json
[exit 0]
~~~

#### GET scan/config (before the scan)

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/scan/config'` @ 2026-10-14T20:00:48Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:48 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 91

{
  "scanActive": false,
  "lastScanTimestamp": 1792008045225,
  "lastScanDuration": 1518
}
[curl http=200 total=0.020244s]
~~~

#### POST scan/config

`curl -sS -g -i -X POST -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/scan/config'` @ 2026-10-14T20:00:48Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:48 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": true,
  "lastScanTimestamp": 1792008045225,
  "lastScanDuration": 1518
}
[curl http=200 total=0.008201s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/scan/config'` @ 2026-10-14T20:00:49Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:49 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1792008049475,
  "lastScanDuration": 552
}
[curl http=200 total=0.009115s]
~~~

#### non-tag module/type directories present in core and production

`bash -c $'\n    for c in core production; do\n      [ -d "$c" ] || continue\n      find "$c" -mindepth 2 -maxdepth 2 -type d | while IFS= read -r d; do\n        r=${d#*/}\n        case "$r" in ignition/tag-*) ;; *) echo "$r" ;; esac\n      done\n    done | LC_ALL=C sort -u' ` @ 2026-10-14T20:00:49Z

~~~
com.inductiveautomation.eam/event-thresholds
com.inductiveautomation.eam/module-settings
com.inductiveautomation.opcua/access-control
com.inductiveautomation.opcua/one-time
com.inductiveautomation.opcua/server-config
com.inductiveautomation.perspective/themes
com.inductiveautomation.sfc/chart-settings
ignition/cobranding
ignition/database-driver
ignition/database-translator
ignition/edge-system-properties
ignition/gateway-network-proxy-rules
ignition/gateway-network-queue-settings
ignition/gateway-network-settings
ignition/general-alarm-settings
ignition/identity-provider
ignition/images
ignition/keyboard_layout
ignition/opc-connection
ignition/quickstart
ignition/schedule
ignition/security-levels
ignition/security-properties
ignition/system-properties
ignition/translations
ignition/user-source
[exit 0]
~~~

#### GET resources/list/com.inductiveautomation.eam/event-thresholds

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/com.inductiveautomation.eam/event-thresholds?limit=-1'` @ 2026-10-14T20:00:49Z

~~~
HTTP/1.1 404 Not Found
Date: Wed, 14 Oct 2026 20:00:50 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 539

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 No route match for path: /v1/resources/list/com.inductiveautomation.eam/event-thresholds</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/resources/list/com.inductiveautomation.eam/event-thresholds</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>No route match for path: /v1/resources/list/com.inductiveautomation.eam/event-thresholds</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.034044s]
~~~

#### GET resources/singleton/com.inductiveautomation.eam/event-thresholds

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/singleton/com.inductiveautomation.eam/event-thresholds'` @ 2026-10-14T20:00:50Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:50 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 1216

{
  "type": "com.inductiveautomation.eam/event-thresholds",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "a4151e98ea27795ae6b94f6ce2adf69aaefd3b30c819d7e7ea0fcc3e460efd16",
  "config": {
    "alarmOnActivity": true,
    "alarmOnMetrics": false,
    "alarmOnTasks": true,
    "warningPriority": "Low",
    "errorPriority": "High",
    "activePipeline": "",
    "ackPipeline": "",
    "connectionWarning": 5,
    "connectionError": 15,
    "cpuWarning": 70,
    "cpuError": 90,
    "memWarning": 70,
    "memError": 90,
    "errPerMinWarning": 2,
    "errPerMinError": 5,
    "errPerHourWarning": 20,
    "errPerHourError": 60,
    "connectedClientsWarning": 50,
    "connectedClientsError": 100,
    "perspectiveSessionsWarning": 50,
    "perspectiveSessionsError": 100,
    "dbConnUtilWarning": 80,
    "dbConnUtilError": 100
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "lastModification": {
      "actor": "default",
      "timestamp": "2026-10-14T19:42:17Z"
    },
    "lastModificationSignature": "89c4d661402e04d52bea062d195bb4b01eb4bf8067f936d9172958cee2ff6170"
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.023609s]
~~~

#### GET resources/list/com.inductiveautomation.eam/module-settings

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/com.inductiveautomation.eam/module-settings?limit=-1'` @ 2026-10-14T20:00:50Z

~~~
HTTP/1.1 404 Not Found
Date: Wed, 14 Oct 2026 20:00:50 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 536

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 No route match for path: /v1/resources/list/com.inductiveautomation.eam/module-settings</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/resources/list/com.inductiveautomation.eam/module-settings</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>No route match for path: /v1/resources/list/com.inductiveautomation.eam/module-settings</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.011342s]
~~~

#### GET resources/singleton/com.inductiveautomation.eam/module-settings

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/singleton/com.inductiveautomation.eam/module-settings'` @ 2026-10-14T20:00:50Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:50 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 1025

{
  "type": "com.inductiveautomation.eam/module-settings",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "7b00403976d60ed1a36f8345ad9c9b7f5b8727c3885ed7706bc55227b5e8f63e",
  "config": {
    "installMode": "NotInstalled",
    "controllerSettings": {
      "eventTableName": "agent_events",
      "archiveLocationMode": "Automatic",
      "lowDiskThresholdMB": 1024,
      "maxRetainedBackupCount": 5,
      "backupRetentionAge": 0,
      "backupRetentionTimeUnit": "Days"
    },
    "agentSettings": {
      "sendStatsInterval": 45,
      "httpConnectTimeout": 10,
      "httpReadTimeout": 60,
      "forwardLeasedLicense": false
    }
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "lastModification": {
      "actor": "default",
      "timestamp": "2026-10-14T19:42:17Z"
    },
    "lastModificationSignature": "1988cdaee78333d2f2f0eb673eb36eefa8e7accd672aaa48ddc419edad5c02f8"
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.022099s]
~~~

#### GET resources/list/com.inductiveautomation.opcua/access-control

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/com.inductiveautomation.opcua/access-control?limit=-1'` @ 2026-10-14T20:00:50Z

~~~
HTTP/1.1 404 Not Found
Date: Wed, 14 Oct 2026 20:00:50 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 539

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 No route match for path: /v1/resources/list/com.inductiveautomation.opcua/access-control</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/resources/list/com.inductiveautomation.opcua/access-control</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>No route match for path: /v1/resources/list/com.inductiveautomation.opcua/access-control</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.021795s]
~~~

#### GET resources/singleton/com.inductiveautomation.opcua/access-control

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/singleton/com.inductiveautomation.opcua/access-control'` @ 2026-10-14T20:00:50Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:50 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 1290

{
  "type": "com.inductiveautomation.opcua/access-control",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "b021d476554e6dbbfafcbdb700dc1176347a9e8566a3c163d0b83da980f06011",
  "config": {
    "defaultDeviceRolePermissionMappings": [
      {
        "role": "Anonymous",
        "permissions": [
          "Browse",
          "Read"
        ]
      },
      {
        "role": "AuthenticatedUser",
        "permissions": [
          "Browse",
          "Read",
          "Write",
          "Call"
        ]
      }
    ],
    "defaultTagProviderRolePermissionMappings": [
      {
        "role": "Anonymous",
        "permissions": [
          "Browse",
          "Read"
        ]
      },
      {
        "role": "AuthenticatedUser",
        "permissions": [
          "Browse",
          "Read",
          "Write",
          "Call"
        ]
      }
    ],
    "tagProviderRolePermissionMappings": {}
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "lastModification": {
      "actor": "default",
      "timestamp": "2026-10-14T19:42:10Z"
    },
    "lastModificationSignature": "01fa9ba90b2aabc84c737fc0d199985773d2097fcfbe3cd996d4856229352bec"
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.038116s]
~~~

#### GET resources/list/com.inductiveautomation.opcua/one-time

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/com.inductiveautomation.opcua/one-time?limit=-1'` @ 2026-10-14T20:00:50Z

~~~
HTTP/1.1 404 Not Found
Date: Wed, 14 Oct 2026 20:00:50 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 521

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 No route match for path: /v1/resources/list/com.inductiveautomation.opcua/one-time</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/resources/list/com.inductiveautomation.opcua/one-time</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>No route match for path: /v1/resources/list/com.inductiveautomation.opcua/one-time</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.023787s]
~~~

#### GET resources/singleton/com.inductiveautomation.opcua/one-time

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/singleton/com.inductiveautomation.opcua/one-time'` @ 2026-10-14T20:00:51Z

~~~
HTTP/1.1 404 Not Found
Date: Wed, 14 Oct 2026 20:00:51 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 536

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 No route match for path: /v1/resources/singleton/com.inductiveautomation.opcua/one-time</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/resources/singleton/com.inductiveautomation.opcua/one-time</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>No route match for path: /v1/resources/singleton/com.inductiveautomation.opcua/one-time</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.033557s]
~~~

#### GET resources/list/com.inductiveautomation.opcua/server-config

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/com.inductiveautomation.opcua/server-config?limit=-1'` @ 2026-10-14T20:00:51Z

~~~
HTTP/1.1 404 Not Found
Date: Wed, 14 Oct 2026 20:00:51 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 536

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 No route match for path: /v1/resources/list/com.inductiveautomation.opcua/server-config</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/resources/list/com.inductiveautomation.opcua/server-config</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>No route match for path: /v1/resources/list/com.inductiveautomation.opcua/server-config</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.125887s]
~~~

#### GET resources/singleton/com.inductiveautomation.opcua/server-config

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/singleton/com.inductiveautomation.opcua/server-config'` @ 2026-10-14T20:00:51Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:51 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 1207

{
  "type": "com.inductiveautomation.opcua/server-config",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "cc8f6efce34066ed244d0222a67497f8638240509373f69bd78a62c0acfead00",
  "config": {
    "endpoint": {
      "bindPort": 62541,
      "bindAddresses": [
        "localhost"
      ],
      "endpointAddresses": [
        "\u003chostname\u003e",
        "\u003clocalhost\u003e"
      ],
      "securityPolicies": [
        "Basic256Sha256"
      ],
      "messageSecurityModes": [
        "SignAndEncrypt"
      ]
    },
    "authentication": {
      "anonymousAccessAllowed": false,
      "authenticationProfile": "opcua-module"
    },
    "redundancy": {
      "readOnlyWhenInactive": false
    },
    "advanced": {
      "exposedTagsEnabled": false,
      "maxSessionCount": 100,
      "gdsPushEnabled": false
    }
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "lastModification": {
      "actor": "default",
      "timestamp": "2026-10-14T19:42:12Z"
    },
    "lastModificationSignature": "9059577141481fcff4d0caf3c6f57fc30a566eb5d13164d4f17cd63a23df7e77"
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.043601s]
~~~

#### GET resources/list/com.inductiveautomation.perspective/themes

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/com.inductiveautomation.perspective/themes?limit=-1'` @ 2026-10-14T20:00:51Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:51 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 5368

{"items":[{"type":"com.inductiveautomation.perspective/themes","name":"dark","description":"The dark theme for Perspective.","enabled":true,"version":1,"collection":"system","collections":["system"],"signature":"83219d9233513718b4861851c6fbcb32f6be10ede37c2298cf603f59953030c9","config":{"entrypoint":"index.css","isPrivate":false},"data":["config.json","globals.css","index.css","palette/alarm-journal-table.css","palette/alarm-status-table.css","palette/equipment-schedule.css","palette/horizontal-menu.css","palette/index.css","palette/led.css","palette/menu-tree.css","palette/power-chart.css","palette/table.css","variables.css"],"attributes":{},"metrics":{},"healthchecks":{}},{"type":"com.inductiveautomation.perspective/themes","name":"dark-cool","description":"The dark-cool theme for Perspective.","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"83f37f1e40163d18931173f1ee20de79b70ffd81e86aa9313b770354aa607457","config":{"entrypoint":"index.css","isPrivate":false},"data":["config.json","index.css","variables.css"],"attributes":{"lastModification":{"actor":"theme-manager","timestamp":"2026-10-14T19:42:08Z"},"lastModificationSignature":"3dc622898c0406711cda27546b476b1daa01933946489271169d180a5337cfa3"},"metrics":{},"healthchecks":{}},{"type":"com.inductiveautomation.perspective/themes","name":"dark-warm","description":"The dark-warm theme for Perspective.","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"b2a7b9020993d3e47aa49ae22017e52b81bb9a231967249a3ccc949d74ee9240","config":{"entrypoint":"index.css","isPrivate":false},"data":["config.json","index.css","variables.css"],"attributes":{"lastModification":{"actor":"theme-manager","timestamp":"2026-10-14T19:42:08Z"},"lastModificationSignature":"f72f730cec42de668d85f55a52c4e4f06c386cb3159e28772c215e4ac55474fc"},"metrics":{},"healthchecks":{}},{"type":"com.inductiveautomation.perspective/themes","name":"light","description":"The light theme for Perspective.","enabled":true,"version":1,"collection":"system","collections":["system"],"signature":"b80d3d94f79913c349972fc020ef52914d802f6e5bc65e40a2a61a93bbdbce06","config":{"entrypoint":"index.css","isPrivate":false},"data":["app/dock.css","app/index.css","app/popup.css","app/quality.css","common/alarm-table.css","common/button.css","common/checkbox.css","common/component-modal.css","common/container.css","common/context-menu.css","common/data-type-filter.css","common/date-range-picker.css","common/date-range-time-picker.css","common/dropdown.css","common/index.css","common/input-field.css","common/pager.css","common/progress-bar.css","common/radio.css","common/realtime-or-historical-range-selector.css","common/realtime-or-historical-range.css","common/select.css","common/slider.css","common/table.css","common/tag-browser.css","common/text-area.css","common/time-input.css","common/toggle-switch.css","common/view.css","config.json","designer/index.css","designer/selection.css","fonts.css","globals.css","index.css","palette/accordion.css","palette/alarm-journal-table.css","palette/alarm-status-table.css","palette/checkbox.css","palette/column-container.css","palette/context-menu.css","palette/cylindrical-tank.css","palette/dashboard.css","palette/date-time.css","palette/equipment-schedule.css","palette/file-upload.css","palette/form.css","palette/horizontal-menu.css","palette/index.css","palette/led.css","palette/linear-scale.css","palette/menu-tree.css","palette/moving-analog-indicator.css","palette/multi-state-button.css","palette/numeric-entry-field.css","palette/one-shot-button.css","palette/password-field.css","palette/piping.css","palette/power-chart.css","palette/radio-group.css","palette/report-viewer.css","palette/signature-pad.css","palette/sparkline.css","palette/split-container.css","palette/symbols.css","palette/tab-container.css","palette/table.css","palette/tag-browse-tree.css","palette/thermometer.css","palette/toggle-switch.css","palette/tooltip.css","palette/tree.css","palette/video-player.css","variables.css"],"attributes":{},"metrics":{},"healthchecks":{}},{"type":"com.inductiveautomation.perspective/themes","name":"light-cool","description":"The light-cool theme for Perspective.","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"4082d90afa188e758314320fd9b24fc31275b3213c83aeea43ae93ed4961972f","config":{"entrypoint":"index.css","isPrivate":false},"data":["config.json","index.css","variables.css"],"attributes":{"lastModification":{"actor":"theme-manager","timestamp":"2026-10-14T19:42:08Z"},"lastModificationSignature":"4c5c09f366498fe2086f6ae4281a97f9d23266c8382fbd1025bf660621d14fa5"},"metrics":{},"healthchecks":{}},{"type":"com.inductiveautomation.perspective/themes","name":"light-warm","description":"The light-warm theme for Perspective.","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"8d3b26e72c5060e729157ec95c19d4d9fff3c594cc876c4fcb01ff04e397c371","config":{"entrypoint":"index.css","isPrivate":false},"data":["config.json","index.css","variables.css"],"attributes":{"lastModification":{"actor":"theme-manager","timestamp":"2026-10-14T19:42:08Z"},"lastModificationSignature":"34e7b955383b5b4517e2100aaa305ad9fcc8f72dd4db0caefda7dc2a7c94b368"},"metrics":{},"healthchecks":{}}],"metadata":{"total":6,"matching":6,"limit":-1,"offset":0}}
[curl http=200 total=0.165992s]
~~~

#### GET resources/list/com.inductiveautomation.sfc/chart-settings

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/com.inductiveautomation.sfc/chart-settings?limit=-1'` @ 2026-10-14T20:00:52Z

~~~
HTTP/1.1 404 Not Found
Date: Wed, 14 Oct 2026 20:00:52 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 533

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 No route match for path: /v1/resources/list/com.inductiveautomation.sfc/chart-settings</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/resources/list/com.inductiveautomation.sfc/chart-settings</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>No route match for path: /v1/resources/list/com.inductiveautomation.sfc/chart-settings</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.073012s]
~~~

#### GET resources/singleton/com.inductiveautomation.sfc/chart-settings

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/singleton/com.inductiveautomation.sfc/chart-settings'` @ 2026-10-14T20:00:52Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:52 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 686

{
  "type": "com.inductiveautomation.sfc/chart-settings",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "5af16b46f4ba2f95675361ce5943ad14d2f87f310dd5ad35126ee94e14a45b93",
  "config": {
    "chartRecordingEnabled": false,
    "recordingsPerChart": 5,
    "pruneAge": 7,
    "pruneAgeUnits": "DAY"
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "lastModification": {
      "actor": "default",
      "timestamp": "2026-10-14T19:42:04Z"
    },
    "lastModificationSignature": "d7202acabbab2dcb8aee6c4720756cfde0679e369c3da652f440be042eb381e7"
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.037913s]
~~~

#### GET resources/list/ignition/cobranding

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/ignition/cobranding?limit=-1'` @ 2026-10-14T20:00:52Z

~~~
HTTP/1.1 404 Not Found
Date: Wed, 14 Oct 2026 20:00:52 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 464

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 No route match for path: /v1/resources/list/ignition/cobranding</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/resources/list/ignition/cobranding</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>No route match for path: /v1/resources/list/ignition/cobranding</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.020943s]
~~~

#### GET resources/singleton/ignition/cobranding

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/singleton/ignition/cobranding'` @ 2026-10-14T20:00:52Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:52 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 903

{
  "type": "ignition/cobranding",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "b5ed1ec2984777bfda248ed8668b3d3ab3fba3dcfe7d22b65b02ff1ac1f10c53",
  "config": {
    "enabled": false,
    "backgroundColor": "#697077",
    "textColor": "#FFFFFF",
    "buttonColor": "#0C7BB3",
    "buttonTextColor": "#FFFFFF",
    "logo": null,
    "favicon": null,
    "appIcon": null,
    "logoType": null,
    "logoName": null,
    "faviconName": null,
    "appIconName": null,
    "faviconSize": null,
    "appIconSize": null
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "lastModification": {
      "actor": "default",
      "timestamp": "2026-10-14T19:40:52Z"
    },
    "lastModificationSignature": "4760b0e9834dc6a3682b79156b4be658dfc3953a60db61b75ecc793993a6e3ad"
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.015036s]
~~~

#### GET resources/list/ignition/database-driver

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/ignition/database-driver?limit=-1'` @ 2026-10-14T20:00:53Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:54 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 9351

{"items":[{"type":"ignition/database-driver","name":"MariaDB","description":"The MariaDB (a community-owned fork of MySQL) JDBC Driver - compatible with all MariaDB servers, MySQL 5.x (>= 5.5.3) and MySQL 8.0.","enabled":true,"version":1,"collection":"system","collections":["system"],"signature":"235b314b634d1bffb39a0d6294e7b413dd82decc2e5d9a9ca462040aef1cd828","config":{"type":"MYSQL","defaultTranslator":"MYSQL","classname":"org.mariadb.jdbc.Driver","urlFormat":"jdbc:mariadb://localhost:3306/test","urlInstructions":"<br/>The format of the MariaDB connect URL is:<br><code>jdbc:mariadb://<b>host</b>:<b>port</b>/<b>database</b></code><br>With the three parameters (in bold) <ul style=\"list-style-type:none;margin-left:10px;\"><li><b>host</b>: The host name or IP address of the database server.</li><li><b>port</b>: The port that the database server is running on. MariaDB default port is <b>3306</b>.</li><li><b>database</b>: The name of the logical database that you are connecting to on the MariaDB server.</li></ul>","defaultProps":"connectTimeout=120000;socketTimeout=120000;rewriteBatchedStatements=true;","defaultPropInstructions":"There is an extensive list of extra connection properties available for MariaDB Connector/J. See <a href='https://mariadb.com/kb/en/library/about-mariadb-connector-j/#optional-url-parameters'>the documentation</a> for a table describing all connection properties.<br>A default <tt>serverTimezone</tt> value (taken from the gateway) will be appended to the connection string if one is not specified.","defaultValidationQuery":"SELECT 1"},"data":["config.json","mariadb-jdbc.jar"],"attributes":{},"metrics":{},"healthchecks":{"status":{"name":"database-driver.MariaDB.status","result":{"healthy":true,"time":1792008054630,"message":"Installed"}}}},{"type":"ignition/database-driver","name":"Microsoft SQLServer","description":"Microsoft SQLServer JDBC driver","enabled":true,"version":1,"collection":"system","collections":["system"],"signature":"04a64f3fa0d6d007095f989bd6f44abbe532fb74ba056edff5f3166a4a7b793a","config":{"type":"MSSQL","defaultTranslator":"MSSQL","classname":"com.microsoft.sqlserver.jdbc.SQLServerDriver","urlFormat":"jdbc:sqlserver://localhost\\MSSQLSERVER","urlInstructions":"<br/>The format of the SQL Server connect URL is:<br/><code>jdbc:sqlserver://<b>host</b>\\<b>instanceName</b>[:<b>port</b>]</code><br/>With the three parameters (in bold) <ul style=\"list-style-type:none;margin-left:10px;\"><li><b>host</b>: The host name or IP address of the database server.</li><li><b>instanceName</b>: (optional) the instance to connect to on the host. If not specified, a connection to the default instance is made.</li><li><b>port</b>: (optional) the port to connect to. The default is <b>1433</b>. If you are using the default, you can omit the port and the preceding ':'.</li></ul><br/>For SQL Server, you specify the <i>database name</i> to connect to using the <code>databaseName</code> property in the <i>Extra Connection Properties</i>.","defaultProps":"databaseName=test","defaultPropInstructions":"Use <i>databaseName=YOUR_DATABASE</i> to specify the database to connect to.","defaultValidationQuery":"SELECT 1"},"data":["config.json","mssql-jdbc.jar"],"attributes":{},"metrics":{},"healthchecks":{"status":{"name":"database-driver.Microsoft SQLServer.status","result":{"healthy":true,"time":1792008054645,"message":"Installed"}}}},{"type":"ignition/database-driver","name":"MySQL","description":"The official MySQL JDBC Driver, Connector/J.","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"df07d7e0605ffc08e5ff5a359b4af0cf8b77a84d39b7bd91e533b5124809854c","config":{"type":"MYSQL","defaultTranslator":"MYSQL","classname":"com.mysql.cj.jdbc.Driver","urlFormat":"jdbc:mysql://localhost:3306/test","urlInstructions":"<br/>The format of the MySQL connect URL is:<br><code>jdbc:mysql://<b>host</b>:<b>port</b>/<b>database</b></code><br>With the three parameters (in bold) <ul style=\"list-style-type:none;margin-left:10px;\"><li><b>host</b>: The host name or IP address of the database server.</li><li><b>port</b>: The port that the database server is running on. MySQL default port is <b>3306</b>.</li><li><b>database</b>: The name of the logical database that you are connecting to on the MySQL server.</li></ul>","defaultProps":"zeroDateTimeBehavior=CONVERT_TO_NULL;connectTimeout=120000;socketTimeout=120000;useSSL=false;allowPublicKeyRetrieval=true;rewriteBatchedStatements=true;disableMariaDbDriver","defaultPropInstructions":"There is an extensive list of extra connection properties available for MySQL Connector/J. See <a href='http://dev.mysql.com/doc/connectors/en/connector-j-reference-configuration-properties.html'>the documentation</a> for a table describing all connection properties.<br>A default <tt>serverTimezone</tt> value (taken from the gateway) will be appended to the connection string if one is not specified.","defaultValidationQuery":"SELECT 1"},"data":["config.json"],"attributes":{"uuid":"1f795aa1-dd34-4354-bab4-3812121e040a"},"metrics":{},"healthchecks":{"status":{"name":"database-driver.MySQL.status","result":{"healthy":false,"time":1792008054899,"message":"Error - either required files are missing or classname is incorrect"}}}},{"type":"ignition/database-driver","name":"Oracle Database","description":"The Oracle Database JDBC driver.","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"27f82310ce7cd983484f9b1f7d16586719228b3501a4d1911b4a71d409689e46","config":{"type":"ORACLE","defaultTranslator":"ORACLE","classname":"oracle.jdbc.driver.OracleDriver","urlFormat":"jdbc:oracle:thin:@localhost:1521:test","urlInstructions":"<br/>The format of the Oracle connect URL is:<br/><code>jdbc:oracle:thin:@<b>host</b>:<b>port</b>:<b>SID</b></code><br/>With the three parameters (in bold) <ul style=\"list-style-type:none;margin-left:10px;\"><li><b>host</b>: The host name or IP address of the database server.</li><li><b>port</b>: The port that the database server is running on. Oracle's default port is <b>1521</b>.</li><li><b>SID</b>: the system ID that identifies the database to connect to.</li></ul>","defaultProps":"","defaultPropInstructions":"","defaultValidationQuery":"SELECT 1 FROM DUAL"},"data":["config.json"],"attributes":{"uuid":"b8b8261d-ffb8-4d63-bbfa-4e95798e68a2"},"metrics":{},"healthchecks":{"status":{"name":"database-driver.Oracle Database.status","result":{"healthy":false,"time":1792008054918,"message":"Error - either required files are missing or classname is incorrect"}}}},{"type":"ignition/database-driver","name":"PostgreSQL","description":"The official PostgreSQL JDBC Driver","enabled":true,"version":1,"collection":"system","collections":["system"],"signature":"d02e13d27fbf86fdaca6d931cf493bde1bbd078dcb2be3280342d5c6357f5fe0","config":{"type":"POSTGRES","defaultTranslator":"POSTGRES","classname":"org.postgresql.Driver","urlFormat":"jdbc:postgresql://localhost:5432/postgres","urlInstructions":"<br/>The format of the PostgreSQL connect URL is:<br/><code>jdbc:postgresql://<b>host</b>:<b>port</b>/<b>database</b></code><br/>With the three parameters (in bold) <ul style=\"list-style-type:none;margin-left:10px;\"><li><b>host</b>: The host name or IP address of the database server.</li><li><b>port</b>: The port that the database server is running on. PostgreSQL default port is <b>5432</b>.</li><li><b>database</b>: The name of the logical database that you are connecting to on the PostgreSQL server.</li></ul>","defaultProps":"","defaultPropInstructions":"No extra connection parameters are recommended for PostgreSQL. For possible parameter values, see the documentation at <a href='http://jdbc.postgresql.org'>the PostgreSQL JDBC driver website</a>.","defaultValidationQuery":"SELECT 1"},"data":["config.json","postgresql-jdbc.jar"],"attributes":{},"metrics":{},"healthchecks":{"status":{"name":"database-driver.PostgreSQL.status","result":{"healthy":true,"time":1792008054918,"message":"Installed"}}}},{"type":"ignition/database-driver","name":"SQLite","description":"Driver for the popular embedded database system.","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"b7c99f52114d0c3916607826e3c8b4dace4b8a4183f4bcf41524367db9652013","config":{"type":"SQLITE","defaultTranslator":"SQLITE","classname":"org.sqlite.JDBC","urlFormat":"jdbc:sqlite:C:/Path/To/File.db","urlInstructions":"<br/>The format of the SQLite connect URL is:<br/><code>jdbc:sqlite:C:/Path/To/File.db</code><br/><code>jdbc:sqlite:/path/on/linux/File.db</code><br/><br/>Use <code>${data}</code> or <code>${local}</code> as a placeholder for the Ignition Gateway's data directory or local directory, respectively, as seen below:<br/><code>jdbc:sqlite:${data}/File.db</code><br/><code>jdbc:sqlite:${local}/Folder/File.db</code><br/><br/>Use <code>jdbc:sqlite::memory:</code> for a temporary database.<br/>","defaultProps":"","defaultPropInstructions":"No extra connection parameters are recommended for SQLite","defaultValidationQuery":"SELECT 1"},"data":["config.json"],"attributes":{"uuid":"24dc938b-b869-4683-b1e4-a25bb78253c0"},"metrics":{},"healthchecks":{"status":{"name":"database-driver.SQLite.status","result":{"healthy":true,"time":1792008054925,"message":"Installed"}}}}],"metadata":{"total":6,"matching":6,"limit":-1,"offset":0}}
[curl http=200 total=1.090200s]
~~~

#### GET resources/list/ignition/database-translator

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/ignition/database-translator?limit=-1'` @ 2026-10-14T20:00:55Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:55 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 8163

{"items":[{"type":"ignition/database-translator","name":"GENERIC","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"f266ce4f9efc59a3ce28cb3013b700c402ba89123ecad5b5c0bd9a7e9b5b68b6","config":{"createTable":"CREATE TABLE {tablename} ({creationdef}{primarykeydef})","createIndex":"CREATE INDEX {indexname} ON {tablename}({columnname})","autoIncTypeDef":"{type} NOT NULL AUTO_INCREMENT","alterTable":"ALTER TABLE {tablename} {alterdef}","alterTableColumnDef":"ADD COLUMN {columnname} {type}","primaryKeyDef":"PRIMARY KEY ({columnname})","limit":"LIMIT {limit}","limitClausePosition":"Back","currentTimeQuery":"SELECT CURRENT_TIMESTAMP","columnQuoteChar":"\"","supportsRGK":true,"fetchKeyQuery":"","tableListFilter":"","i1Type":"INT","i2Type":"INT","i4Type":"INT","i8Type":"BIGINT","boolType":"INT","datetimeType":"DATETIME","r4Type":"FLOAT","r8Type":"DOUBLE","stringType":"VARCHAR(255)","blobType":"VARBINARY","textType":""},"data":["config.json"],"attributes":{"lastModification":{"actor":"default","timestamp":"2026-10-14T19:42:45Z"},"uuid":"9ec01211-a1f2-4be6-9c40-675d0827a2ce","lastModificationSignature":"7b84b2b47e4dd395a8d5240475ab910d1fb29b1175100fe2c2b32dd17f34e747"},"metrics":{},"healthchecks":{}},{"type":"ignition/database-translator","name":"MSSQL","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"c450e559277a25737cad14d9f83b8cef538652da8227a7b38a2784eec555d499","config":{"createTable":"CREATE TABLE {tablename} ({creationdef}{primarykeydef})","createIndex":"CREATE INDEX {indexname} ON {tablename}({columnname})","autoIncTypeDef":"{type} IDENTITY(1, 1)","alterTable":"ALTER TABLE {tablename} ADD {alterdef}","alterTableColumnDef":"{columnname} {type}","primaryKeyDef":"PRIMARY KEY CLUSTERED ({columnname})","limit":"TOP {limit}","limitClausePosition":"Front","currentTimeQuery":"SELECT CURRENT_TIMESTAMP","columnQuoteChar":"\"","supportsRGK":true,"fetchKeyQuery":"","tableListFilter":"","i1Type":"INT","i2Type":"INT","i4Type":"INT","i8Type":"BIGINT","boolType":"INT","datetimeType":"DATETIME","r4Type":"FLOAT(10)","r8Type":"DOUBLE PRECISION","stringType":"NVARCHAR(255)","blobType":"VARBINARY","textType":"NVARCHAR(MAX)"},"data":["config.json"],"attributes":{"lastModification":{"actor":"default","timestamp":"2026-10-14T19:42:45Z"},"uuid":"d32915c9-b720-4378-9aeb-d57102c847a6","lastModificationSignature":"7dd0af31fc8fc080f17ac3aba0e00db249070c77d9cdc8ec3484ed8d151b6dc8"},"metrics":{},"healthchecks":{}},{"type":"ignition/database-translator","name":"MYSQL","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"48c9952e09aaec591f0ea0c1932f98133fdc39bd63d8175b4619e9bd457d1148","config":{"createTable":"CREATE TABLE {tablename} ({creationdef}{primarykeydef})","createIndex":"CREATE INDEX {indexname} ON {tablename}({columnname})","autoIncTypeDef":"{type} NOT NULL AUTO_INCREMENT","alterTable":"ALTER TABLE {tablename} {alterdef}","alterTableColumnDef":"ADD COLUMN {columnname} {type}","primaryKeyDef":"PRIMARY KEY ({columnname})","limit":"LIMIT {limit}","limitClausePosition":"Back","currentTimeQuery":"SELECT CURRENT_TIMESTAMP","columnQuoteChar":"`","supportsRGK":true,"fetchKeyQuery":"","tableListFilter":"","i1Type":"INT","i2Type":"INT","i4Type":"INT","i8Type":"BIGINT","boolType":"INT","datetimeType":"DATETIME","r4Type":"FLOAT","r8Type":"DOUBLE","stringType":"VARCHAR(255)","blobType":"VARBINARY","textType":"TEXT"},"data":["config.json"],"attributes":{"lastModification":{"actor":"default","timestamp":"2026-10-14T19:42:45Z"},"uuid":"614ac389-dc8f-4d8b-ab02-6f49f4a257f6","lastModificationSignature":"2a434e7d9fa1c17209c66fc46362e8f1739c1919cc46d35746b2132c17ec90ec"},"metrics":{},"healthchecks":{}},{"type":"ignition/database-translator","name":"ORACLE","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"dbeb07a0b9a18f270524b7b73ca12e6845e287be537af865bae3d2875a699ddc","config":{"createTable":"CREATE TABLE {tablename} ({creationdef}{primarykeydef})","createAutoIncSequence":"CREATE SEQUENCE {tablename}_seq START WITH 1 INCREMENT BY 1","createAutoIncTrigger":"CREATE TRIGGER {tablename}_trig \nBEFORE INSERT ON {tablename} \nREFERENCING NEW AS NEW\nFOR EACH ROW \nBEGIN \n    SELECT {tablename}_seq.NEXTVAL \n    INTO :NEW.{columnname} FROM DUAL;\nEND;","createIndex":"CREATE INDEX {indexname} ON {tablename}({columnname})","autoIncTypeDef":"{type} NOT NULL","alterTable":"ALTER TABLE {tablename} ADD ({alterdef})","alterTableColumnDef":"{columnname} {type}","primaryKeyDef":"PRIMARY KEY ({columnname})","limit":"ROWNUM <= {limit}","limitClausePosition":"Wrap","currentTimeQuery":"SELECT CURRENT_TIMESTAMP FROM DUAL","columnQuoteChar":"\"","supportsRGK":false,"fetchKeyQuery":"SELECT {tablename}_seq.CURRVAL FROM DUAL","tableListFilter":"SYS_INFO*;*SYSTEM*;WWV*;*$*;DBA*;LOGMNR*;ORDDCM*;APEX*;ALL_SA*;DATABASE*;DV_*;GSM*;HELP;MVIEW*;ORD_*;PRODUCT_PRIVS;REDO_*;SCHEDULER_*;SERVICE*;SI_*;SQLPLUS*;SYS_*;USER_SA*;VERIFY_HISTORY;VNCR;WLM_*;XML_*;CLOUD;REGION;CHANGE_LOG*","i1Type":"INT","i2Type":"INT","i4Type":"INT","i8Type":"INT","boolType":"INT","datetimeType":"TIMESTAMP","r4Type":"FLOAT","r8Type":"DOUBLE PRECISION","stringType":"VARCHAR2(255)","blobType":"VARBINARY","textType":"NCLOB"},"data":["config.json"],"attributes":{"lastModification":{"actor":"default","timestamp":"2026-10-14T19:42:45Z"},"uuid":"1f2db57a-bef9-49e6-aa71-ec1ae247edf8","lastModificationSignature":"2a0d4cf09c48c734448c2f067b0929cc8327b51f9217faa5627e96315a9e2afb"},"metrics":{},"healthchecks":{}},{"type":"ignition/database-translator","name":"POSTGRES","description":null,"enabled":true,"version":1,"collection":"production","collections":["core","production"],"signature":"e04e4e5b7dd47a9339271443bbb6879da23c0070aa955237543f1756eb85d6e1","config":{"createTable":"CREATE TABLE {tablename} ({creationdef}{primarykeydef})","createIndex":"CREATE INDEX {indexname} ON {tablename}({columnname})","autoIncTypeDef":"SERIAL NOT NULL","alterTable":"ALTER TABLE {tablename} {alterdef}","alterTableColumnDef":"ADD COLUMN {columnname} {type}","primaryKeyDef":"PRIMARY KEY ({columnname})","limit":"LIMIT {limit}","limitClausePosition":"Back","currentTimeQuery":"SELECT CURRENT_TIMESTAMP","columnQuoteChar":"\"","supportsRGK":true,"fetchKeyQuery":"","tableListFilter":"","i1Type":"int","i2Type":"int","i4Type":"int","i8Type":"bigint","boolType":"int","datetimeType":"timestamp","r4Type":"float","r8Type":"double precision","stringType":"varchar(255)","blobType":"bytea","textType":"text"},"data":["config.json"],"attributes":{"lastModification":{"actor":"8.3-migration","timestamp":"2026-03-05T13:13:02Z"},"uuid":"4a14d749-bb78-47c7-847a-cafa17da4da8","lastModificationSignature":"97226c4a1915d0fa6cade568e7138360fadad3476ea640598f597f9cefcb300c","enabled":true},"metrics":{},"healthchecks":{}},{"type":"ignition/database-translator","name":"SQLITE","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"b0af97de81b8222763b6ce1b35e92d4444a0d7fb89e785e346d8b395b74783b5","config":{"createTable":"CREATE TABLE {tablename} ({creationdef}{primarykeydef})","createIndex":"CREATE INDEX {indexname} ON {tablename}({columnname})","autoIncTypeDef":"INTEGER PRIMARY KEY","alterTable":"ALTER TABLE {tablename} {alterdef}","alterTableColumnDef":"ADD COLUMN {columnname} {type}","primaryKeyDef":"","limit":"LIMIT {limit}","limitClausePosition":"Back","currentTimeQuery":"SELECT CURRENT_TIMESTAMP","columnQuoteChar":"\"","supportsRGK":true,"fetchKeyQuery":"","tableListFilter":"","i1Type":"INTEGER","i2Type":"INTEGER","i4Type":"INTEGER","i8Type":"INTEGER","boolType":"INTEGER","datetimeType":"TEXT","r4Type":"REAL","r8Type":"REAL","stringType":"TEXT","blobType":"BLOB","textType":"TEXT"},"data":["config.json"],"attributes":{"lastModification":{"actor":"default","timestamp":"2026-10-14T19:42:45Z"},"uuid":"9bdb8fc1-4ba8-49d7-89b6-20cef86487d9","lastModificationSignature":"a693ac5b875733bfa424ccaf69fa0f8fc552fe932028ba7817ac2df177beb9f2"},"metrics":{},"healthchecks":{}}],"metadata":{"total":6,"matching":6,"limit":-1,"offset":0}}
[curl http=200 total=0.182334s]
~~~

#### GET resources/list/ignition/edge-system-properties

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/ignition/edge-system-properties?limit=-1'` @ 2026-10-14T20:00:55Z

~~~
HTTP/1.1 404 Not Found
Date: Wed, 14 Oct 2026 20:00:55 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 500

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 No route match for path: /v1/resources/list/ignition/edge-system-properties</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/resources/list/ignition/edge-system-properties</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>No route match for path: /v1/resources/list/ignition/edge-system-properties</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.028093s]
~~~

#### GET resources/singleton/ignition/edge-system-properties

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/singleton/ignition/edge-system-properties'` @ 2026-10-14T20:00:55Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:56 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 664

{
  "type": "ignition/edge-system-properties",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "dc813c214c2de3f619f0060d8ffac6fc6b941dc7068f115e981d24aeb3edd8dc",
  "config": {
    "projectName": "Edge",
    "historianName": "Edge Historian",
    "visualizationName": "VISION"
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "lastModification": {
      "actor": "default",
      "timestamp": "2026-10-14T19:40:52Z"
    },
    "lastModificationSignature": "be2999bd0dcbd249e8ff7c9ecdce98b53f8f1d23fc3f89e14f09b0e7dcb2def5"
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.048171s]
~~~

#### GET resources/list/ignition/gateway-network-proxy-rules

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/ignition/gateway-network-proxy-rules?limit=-1'` @ 2026-10-14T20:00:56Z

~~~
HTTP/1.1 404 Not Found
Date: Wed, 14 Oct 2026 20:00:56 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 515

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 No route match for path: /v1/resources/list/ignition/gateway-network-proxy-rules</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/resources/list/ignition/gateway-network-proxy-rules</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>No route match for path: /v1/resources/list/ignition/gateway-network-proxy-rules</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.336666s]
~~~

#### GET resources/singleton/ignition/gateway-network-proxy-rules

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/singleton/ignition/gateway-network-proxy-rules'` @ 2026-10-14T20:00:56Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:57 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 596

{
  "type": "ignition/gateway-network-proxy-rules",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "22c61f5a976357bda207808d1b5a5fbed89feca87e2e3e8e9332c560f08e413d",
  "config": {
    "proxyRules": []
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "lastModification": {
      "actor": "8.3-migration",
      "timestamp": "2026-10-14T19:40:49Z"
    },
    "lastModificationSignature": "af1b5dcff34ba9ba476cada33b4502eadb580be454167df5e05b8774138f8fb4"
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.233260s]
~~~

#### GET resources/list/ignition/gateway-network-queue-settings

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/ignition/gateway-network-queue-settings?limit=-1'` @ 2026-10-14T20:00:57Z

~~~
HTTP/1.1 404 Not Found
Date: Wed, 14 Oct 2026 20:00:57 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 524

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 No route match for path: /v1/resources/list/ignition/gateway-network-queue-settings</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/resources/list/ignition/gateway-network-queue-settings</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>No route match for path: /v1/resources/list/ignition/gateway-network-queue-settings</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.125806s]
~~~

#### GET resources/singleton/ignition/gateway-network-queue-settings

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/singleton/ignition/gateway-network-queue-settings'` @ 2026-10-14T20:00:57Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:57 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 1744

{
  "type": "ignition/gateway-network-queue-settings",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "c6e33909fadd2778207bef776975e363f15fcad2f36d2b85af5f3074ba5e4a45",
  "config": {
    "queueSettings": [
      {
        "queueId": "_default_",
        "maxActive": -1,
        "timeoutMillis": 60000,
        "friendlyName": "Default Queue",
        "description": "Generic Gateway Network queue"
      },
      {
        "queueId": "diagnosticInfoQueue",
        "maxActive": -1,
        "timeoutMillis": 300000,
        "friendlyName": "Diagnostic Info Queue",
        "description": "Handles gateway network diagnostic information messages"
      },
      {
        "queueId": "_rpcReturn_",
        "maxActive": -1,
        "timeoutMillis": 60000,
        "friendlyName": "Call Results Queue",
        "description": "Handles results for remote service calls over the Gateway Network"
      },
      {
        "queueId": "longWaitQueue",
        "maxActive": -1,
        "timeoutMillis": 3600000,
        "friendlyName": "Long Wait Queue",
        "description": "Handles messages that take up to an hour to deliver"
      },
      {
        "queueId": "_proxy_",
        "maxActive": -1,
        "timeoutMillis": 3600000,
        "friendlyName": "Proxy Queue",
        "description": "Forwards requests through a proxy Gateway"
      }
    ]
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "lastModification": {
      "actor": "8.3-migration",
      "timestamp": "2026-10-14T19:40:48Z"
    },
    "lastModificationSignature": "e43858fbc65309b95ef32918660811ddbe5384b749bb22623e305dad8c695657"
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.034351s]
~~~

#### GET resources/list/ignition/gateway-network-settings

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/ignition/gateway-network-settings?limit=-1'` @ 2026-10-14T20:00:57Z

~~~
HTTP/1.1 404 Not Found
Date: Wed, 14 Oct 2026 20:00:57 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 506

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 No route match for path: /v1/resources/list/ignition/gateway-network-settings</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/resources/list/ignition/gateway-network-settings</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>No route match for path: /v1/resources/list/ignition/gateway-network-settings</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.060183s]
~~~

#### GET resources/singleton/ignition/gateway-network-settings

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/singleton/ignition/gateway-network-settings'` @ 2026-10-14T20:00:58Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:58 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 1142

{
  "type": "ignition/gateway-network-settings",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "04b9f8f24145bad95804563767a9ba7181a41ed3473f9d83161572cb5ce5fc95",
  "config": {
    "requireSSL": true,
    "requireTwoWayAuth": true,
    "receiveQueueMax": 100,
    "overloadWaitSecs": 60,
    "allowIncoming": true,
    "securityPolicy": "ApprovedOnly",
    "whitelist": "",
    "allowedProxyHops": 0,
    "websocketSessionIdleTimeout": 30000,
    "tempFilesMaxAgeHours": 24,
    "incomingPingRateMillis": 5000,
    "incomingPingTimeoutMillis": 300,
    "incomingPingMaxMissed": 12,
    "proxyInterceptServiceCalls": false,
    "allowJavaSerialization": false,
    "dataChannelQueueSize": 50,
    "dataChannelThreadPoolMaxSize": 100
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "lastModification": {
      "actor": "default",
      "timestamp": "2026-10-14T19:42:50Z"
    },
    "lastModificationSignature": "d374c490301c948b3219c1f35226077603fc9599f8df71b56c0ea1fa4635b250",
    "enabled": true
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.021723s]
~~~

#### GET resources/list/ignition/general-alarm-settings

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/ignition/general-alarm-settings?limit=-1'` @ 2026-10-14T20:00:58Z

~~~
HTTP/1.1 404 Not Found
Date: Wed, 14 Oct 2026 20:00:58 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 500

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 No route match for path: /v1/resources/list/ignition/general-alarm-settings</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/resources/list/ignition/general-alarm-settings</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>No route match for path: /v1/resources/list/ignition/general-alarm-settings</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.033534s]
~~~

#### GET resources/singleton/ignition/general-alarm-settings

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/singleton/ignition/general-alarm-settings'` @ 2026-10-14T20:00:58Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:58 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 707

{
  "type": "ignition/general-alarm-settings",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "production",
  "collections": [
    "core",
    "production"
  ],
  "signature": "ab80a38182292b74a90d34461b4c10fc9b3cef32fcd22b54c6e1ceba06d2c280",
  "config": {
    "startupSuppressionTime": 10,
    "liveEventLimit": 5,
    "notifyInitialEvents": false
  },
  "data": [
    "config.json"
  ],
  "attributes": {
    "lastModification": {
      "actor": "8.3-migration",
      "timestamp": "2026-03-05T13:13:04Z"
    },
    "lastModificationSignature": "66611d3645e9d7661fb50c9ad82dee4d8c19d5c9c6c7fcb2a5a2fa1aa21da40c",
    "enabled": true
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.051086s]
~~~

#### GET resources/list/ignition/identity-provider

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/ignition/identity-provider?limit=-1'` @ 2026-10-14T20:00:58Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:58 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 1357

{"items":[{"type":"ignition/identity-provider","name":"default","description":"Automatically generated Ignition Identity Provider which uses the User Source Profile named \"default\".","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"07dba35f8ed35c23ab4221c779c97cc1f3c270be7e3d60f236f082b9c779dcd2","config":{"profile":{"type":"internal","userAttributeMapper":{"id":{"type":"direct","config":{"attributePath":"sub"}},"userName":{"type":"direct","config":{"attributePath":"preferred_username"}},"firstName":{"type":"direct","config":{"attributePath":"given_name"}},"lastName":{"type":"direct","config":{"attributePath":"family_name"}},"email":{"type":"direct","config":{"attributePath":"email"}},"roles":{"type":"direct","config":{"attributePath":"roles"}}},"userGrants":{"id":{},"username":{}},"securityLevelRules":{"nodes":[]}},"settings":{"userSource":"default","authMethods":[{"type":"basic","config":{}}],"sessionInactivityTimeout":30,"sessionExp":0,"rememberMeExp":0}},"data":["config.json"],"attributes":{"lastModification":{"actor":"system-init","timestamp":"2026-10-14T19:40:52Z"},"uuid":"d00cd2f0-dcc5-4d58-84eb-c61e516431fe","lastModificationSignature":"ff8bc7a2ad92f2bc9fac8f928c26f98f84d59aae2124d74ee4ce5e6f4bcaea5b"},"metrics":{},"healthchecks":{}}],"metadata":{"total":1,"matching":1,"limit":-1,"offset":0}}
[curl http=200 total=0.021200s]
~~~

#### GET resources/list/ignition/images

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/ignition/images?limit=-1'` @ 2026-10-14T20:00:58Z

~~~
HTTP/1.1 404 Not Found
Date: Wed, 14 Oct 2026 20:00:58 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 452

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 No route match for path: /v1/resources/list/ignition/images</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/resources/list/ignition/images</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>No route match for path: /v1/resources/list/ignition/images</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.016935s]
~~~

#### GET resources/singleton/ignition/images

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/singleton/ignition/images'` @ 2026-10-14T20:00:58Z

~~~
HTTP/1.1 404 Not Found
Date: Wed, 14 Oct 2026 20:00:58 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 467

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 No route match for path: /v1/resources/singleton/ignition/images</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/resources/singleton/ignition/images</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>No route match for path: /v1/resources/singleton/ignition/images</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.008471s]
~~~

#### GET resources/list/ignition/keyboard_layout

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/ignition/keyboard_layout?limit=-1'` @ 2026-10-14T20:00:58Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:58 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 9665

{"items":[{"type":"ignition/keyboard_layout","name":"de_qwertz","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"f5419fdf2a6daed9f36d33a6e327d920b3ce261de08b6a7e58b2e2fb1bde3832","config":{"alias":"DE","id":"73a221f3-e2de-4b5b-97a4-cecc32cc14ad","labels":{"BACKSPACE":"Backsp","CANCEL":"Cancel","SHIFT":"Shift","SPACE":"Space","SUBMIT":"Enter"},"name":"de_qwertz","rows":[[{"lowercase":"q","uppercase":"Q"},{"lowercase":"w","uppercase":"W"},{"lowercase":{"accents":["ê","é","è","ë","ė"],"character":"e"},"uppercase":{"accents":["Ê","É","È","Ë","Ė"],"character":"E"}},{"lowercase":"r","uppercase":"R"},{"lowercase":"t","uppercase":"T"},{"lowercase":"z","uppercase":"Z"},{"lowercase":{"accents":["ū","ù","ü","û","ú"],"character":"u"},"uppercase":{"accents":["Ū","Ù","Ü","Û","Ú"],"character":"U"}},{"lowercase":"i","uppercase":"I"},{"lowercase":{"accents":["ō","ø","œ","õ","ó","ò","ô","ö"],"character":"o"},"uppercase":{"accents":["Ō","Ø","Œ","Õ","Ó","Ò","Ô","Ö"],"character":"O"}},{"lowercase":"p","uppercase":"P"},"BACKSPACE"],[{"lowercase":{"accents":["æ","ã","å","ā","ä","â","à","á"],"character":"a"},"uppercase":{"accents":["Æ","Ã","Å","Ā","Ä","Â","À","Á"],"character":"A"}},{"lowercase":{"accents":["š","ß","ś"],"character":"s"},"uppercase":{"accents":["Š","SS","Ś"],"character":"S"}},{"lowercase":"d","uppercase":"D"},{"lowercase":"f","uppercase":"F"},{"lowercase":"g","uppercase":"G"},{"lowercase":"h","uppercase":"H"},{"lowercase":"j","uppercase":"J"},{"lowercase":"k","uppercase":"K"},{"lowercase":"l","uppercase":"L"},"SUBMIT"],["SHIFT",{"lowercase":"y","uppercase":"Y"},{"lowercase":"x","uppercase":"X"},{"lowercase":"c","uppercase":"C"},{"lowercase":"v","uppercase":"V"},{"lowercase":"b","uppercase":"B"},{"lowercase":{"accents":["ñ","ń"],"character":"n"},"uppercase":{"accents":["Ñ","Ń"],"character":"N"}},{"lowercase":"m","uppercase":"M"},"SHIFT"]],"supportedLanguages":["de"],"title":"Deutsch"},"data":["config.json"],"attributes":{"lastModification":{"actor":"system","timestamp":"2026-10-14T19:40:53Z"},"uuid":"73a221f3-e2de-4b5b-97a4-cecc32cc14ad","lastModificationSignature":"96ea4b0f0494513c4960e4febf8f06c3defcdcde34314dccc4a763398fc3636b"},"metrics":{},"healthchecks":{}},{"type":"ignition/keyboard_layout","name":"es_qwerty","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"b7e27627438f188145de79c746b14cdbabccba3b1f860646538cb09af88f3e37","config":{"alias":"ES","id":"d74a89d6-d4f0-4edd-acad-81f4effe0482","labels":{"BACKSPACE":"Retro","CANCEL":"Cancelar","SHIFT":"Shift","SPACE":"Espacio","SUBMIT":"Intro"},"name":"es_qwerty","rows":[[{"lowercase":"q","uppercase":"Q"},{"lowercase":"w","uppercase":"W"},{"lowercase":{"accents":["ē","ę","ė","ë","é","è","ê"],"character":"e"},"uppercase":{"accents":["Ē","Ę","Ė","Ë","É","È","Ê"],"character":"E"}},{"lowercase":"r","uppercase":"R"},{"lowercase":"t","uppercase":"T"},{"lowercase":"y","uppercase":"Y"},{"lowercase":{"accents":["ū","ù","ú","ü","û"],"character":"u"},"uppercase":{"accents":["Ū","Ù","Ú","Ü","Û"],"character":"U"}},{"lowercase":{"accents":["ī","î","į","ì","í","ï"],"character":"i"},"uppercase":{"accents":["Ī","Î","Į","Ì","Í","Ï"],"character":"I"}},{"lowercase":{"accents":["º","ō","œ","ø","õ","ô","ö","ò","ó"],"character":"o"},"uppercase":{"accents":["º","Ō","Œ","Ø","Õ","Ô","Ö","Ò","Ó"],"character":"O"}},{"lowercase":"p","uppercase":"P"},"BACKSPACE"],[{"lowercase":{"accents":["å","ą","æ","ā","ª","á","à","ä","â","ã"],"character":"a"},"uppercase":{"accents":["Å","Ą","Æ","Ā","ª","Á","À","Ä","Â","Ã"],"character":"A"}},{"lowercase":"s","uppercase":"S"},{"lowercase":"d","uppercase":"D"},{"lowercase":"f","uppercase":"F"},{"lowercase":"g","uppercase":"G"},{"lowercase":"h","uppercase":"H"},{"lowercase":"j","uppercase":"J"},{"lowercase":"k","uppercase":"K"},{"lowercase":"l","uppercase":"L"},{"lowercase":"ñ","uppercase":"Ñ"},"SUBMIT"],["SHIFT",{"lowercase":"z","uppercase":"Z"},{"lowercase":"x","uppercase":"X"},{"lowercase":{"accents":["č","ç","ć"],"character":"c"},"uppercase":{"accents":["Č","Ç","Ć"],"character":"C"}},{"lowercase":"v","uppercase":"V"},{"lowercase":"b","uppercase":"B"},{"lowercase":{"accents":["ñ","ń"],"character":"n"},"uppercase":{"accents":["Ñ","Ń"],"character":"N"}},{"lowercase":"m","uppercase":"M"},"SHIFT"]],"supportedLanguages":["es"],"title":"Español"},"data":["config.json"],"attributes":{"lastModification":{"actor":"system","timestamp":"2026-10-14T19:40:53Z"},"uuid":"d74a89d6-d4f0-4edd-acad-81f4effe0482","lastModificationSignature":"be70bff17b49c237d816e498af7c38423d6bd4e6659152906038e9fd41bdec13"},"metrics":{},"healthchecks":{}},{"type":"ignition/keyboard_layout","name":"fr_azerty","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"ba68f4ddadc37e2fc177d6234c5190afd6aad2d864afaa81fb7504e45104273f","config":{"alias":"FR","id":"41f00828-c338-4264-b262-b1a2c46b764f","labels":{"BACKSPACE":"Backsp","CANCEL":"Cancel","SHIFT":"Shift","SPACE":"Space","SUBMIT":"Enter"},"name":"fr_azerty","rows":[[{"lowercase":{"accents":["ã","å","ā","ª","à","â","æ","á","ä"],"character":"a"},"uppercase":{"accents":["Ã","Å","Ā","ª","À","Â","Æ","Á","Ä"],"character":"A"}},{"lowercase":"z","uppercase":"Z"},{"lowercase":{"accents":["ē","ę","ė","ê","é","è","ë"],"character":"e"},"uppercase":{"accents":["Ē","Ę","Ė","Ê","É","È","Ë"],"character":"E"}},{"lowercase":"r","uppercase":"R"},{"lowercase":"t","uppercase":"T"},{"lowercase":{"accents":["ÿ"],"character":"y"},"uppercase":{"accents":["Ÿ"],"character":"Y"}},{"lowercase":{"accents":["ū","ú","ũ","ü","ù","û"],"character":"u"},"uppercase":{"accents":["Ū","Ú","Ũ","Ü","Ù","Û"],"character":"U"}},{"lowercase":{"accents":["ī","ĩ","į","í","ì","î","ï"],"character":"i"},"uppercase":{"accents":["Ī","Ĩ","Į","Í","Ì","Î","Ï"],"character":"I"}},{"lowercase":{"accents":["º","ō","ø","õ","ó","ò","ö","œ","ô"],"character":"o"},"uppercase":{"accents":["º","Ō","Ø","Õ","Ó","Ò","Ö","Œ","Ô"],"character":"O"}},{"lowercase":"p","uppercase":"P"},"BACKSPACE"],[{"lowercase":"q","uppercase":"Q"},{"lowercase":"s","uppercase":"S"},{"lowercase":"d","uppercase":"D"},{"lowercase":"f","uppercase":"F"},{"lowercase":"g","uppercase":"G"},{"lowercase":"h","uppercase":"H"},{"lowercase":"j","uppercase":"J"},{"lowercase":"k","uppercase":"K"},{"lowercase":"l","uppercase":"L"},{"lowercase":"m","uppercase":"M"},"SUBMIT"],["SHIFT",{"lowercase":"w","uppercase":"W"},{"lowercase":"x","uppercase":"X"},{"lowercase":{"accents":["č","ç","ć"],"character":"c"},"uppercase":{"accents":["Č","Ç","Ć"],"character":"C"}},{"lowercase":"v","uppercase":"V"},{"lowercase":"b","uppercase":"B"},{"lowercase":"n","uppercase":"N"},"SHIFT"]],"supportedLanguages":["fr"],"title":"Français"},"data":["config.json"],"attributes":{"lastModification":{"actor":"system","timestamp":"2026-10-14T19:40:53Z"},"uuid":"41f00828-c338-4264-b262-b1a2c46b764f","lastModificationSignature":"96b1b5c959f0718f5bc2f3d89279519fa0b702d16052b569271e54e4c5e9d8ac"},"metrics":{},"healthchecks":{}},{"type":"ignition/keyboard_layout","name":"it_qwerty","description":null,"enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"331672264eeb2bd6bf2a854f863dd210103862e6a9155d3feaa6823cb46a50ce","config":{"alias":"IT","id":"37e58452-b92f-4dd0-ae5f-04b0848286dd","labels":{"BACKSPACE":"Backsp","CANCEL":"Cancel","SHIFT":"Shift","SPACE":"Space","SUBMIT":"Enter"},"name":"it_qwerty","rows":[[{"lowercase":"q","uppercase":"Q"},{"lowercase":"w","uppercase":"W"},{"lowercase":"e","uppercase":"E"},{"lowercase":{"accents":["ɜ","ę","ē","ê","è","é","ë","ə"],"character":"ė"},"uppercase":{"accents":["Ɜ","Ę","Ē","Ê","È","É","Ë","Ə"],"character":"Ė"}},{"lowercase":"r","uppercase":"R"},{"lowercase":"t","uppercase":"T"},{"lowercase":"y","uppercase":"Y"},{"lowercase":{"accents":["ū","û","ù","ú","ü"],"character":"u"},"uppercase":{"accents":["Ū","Û","Ù","Ú","Ü"],"character":"U"}},{"lowercase":{"accents":["ī","ï","į","î","ì","í"],"character":"i"},"uppercase":{"accents":["Ī","Ï","Į","Î","Ì","Í"],"character":"I"}},{"lowercase":{"accents":["º","ō","ø","œ","õ","ö","ô","ó","ò"],"character":"o"},"uppercase":{"accents":["º","Ō","Ø","Œ","Õ","Ö","Ô","Ó","Ò"],"character":"O"}},{"lowercase":"p","uppercase":"P"},"BACKSPACE"],[{"lowercase":{"accents":["ã","å","ā","ª","à","á","â","ä","æ"],"character":"a"},"uppercase":{"accents":["Ã","Å","Ā","ª","À","Á","Â","Ä","Æ"],"character":"A"}},{"lowercase":"s","uppercase":"S"},{"lowercase":"d","uppercase":"D"},{"lowercase":"f","uppercase":"F"},{"lowercase":"g","uppercase":"G"},{"lowercase":"h","uppercase":"H"},{"lowercase":"j","uppercase":"J"},{"lowercase":"k","uppercase":"K"},{"lowercase":"l","uppercase":"L"},"SUBMIT"],["SHIFT",{"lowercase":"z","uppercase":"Z"},{"lowercase":"x","uppercase":"X"},{"lowercase":"c","uppercase":"C"},{"lowercase":"v","uppercase":"V"},{"lowercase":"b","uppercase":"B"},{"lowercase":"n","uppercase":"N"},{"lowercase":"m","uppercase":"M"},"SHIFT"]],"supportedLanguages":["it"],"title":"Italiano"},"data":["config.json"],"attributes":{"lastModification":{"actor":"system","timestamp":"2026-10-14T19:40:53Z"},"uuid":"37e58452-b92f-4dd0-ae5f-04b0848286dd","lastModificationSignature":"1aa756fbb5ff5008af0af59c3c6249569e52cf5c76325325d31335eb0ae1051e"},"metrics":{},"healthchecks":{}}],"metadata":{"total":4,"matching":4,"limit":-1,"offset":0}}
[curl http=200 total=0.046191s]
~~~

#### GET resources/list/ignition/opc-connection

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/ignition/opc-connection?limit=-1'` @ 2026-10-14T20:00:58Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:58 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 2606

{"items":[{"type":"ignition/opc-connection","name":"Ignition OPC UA Server","description":"Default \"loopback\" connection to Ignition's OPC UA server","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"fa1ff72ef6c5beaa303e36db092c9c722bc57c5eb21e058ed78c8fa8c7aee1e1","config":{"profile":{"type":"com.inductiveautomation.OpcUaServerType","readOnly":false},"settings":{"endpoint":{"discoveryUrl":"opc.tcp://localhost:62541/discovery","endpointUrl":"opc.tcp://localhost:62541","securityPolicy":"Basic256Sha256","securityMode":"SignAndEncrypt","hostOverride":""},"authentication":{"authenticationType":"USERNAME","username":"opcuauser","password":{"type":"Embedded","data":{"ciphertext":"UwQS4h8Cw6NnqQ","encrypted_key":"R2orE2eNjU5kssKggZSC-Ts_fumRC4QSaOAD25TDoWR4GrFBieOljw","iv":"IY24_4uKCnp-bxJU","protected":"eyJhbGciOiJBMjU2S1ciLCJlbmMiOiJBMjU2R0NNIiwiaWF0IjoxNzkyMDA2OTcyLCJ6aXAiOiJERUYifQ","tag":"5Jjk1NyOBrKeKlY2LTwYmw"}}},"keepAlive":{"failuresAllowed":1,"interval":15000,"timeout":10000},"advanced":{"connectTimeout":5000,"acknowledgeTimeout":5000,"requestTimeout":60000,"sessionTimeout":120000,"maxPerOperation":8192,"maxReferencesPerNode":8192,"maxPendingPublishRequests":2,"maxNotificationsPerPublish":65535,"maxMessageSize":33554432,"maxArrayLength":2147483647,"maxStringLength":2147483647,"browseOrigin":"OBJECTS_FOLDER","deprecatedDataTypeDictionarySupport":false,"timestampSource":"OPC_PREFER_SOURCE"},"security":{"certificateValidationEnabled":true,"keyStoreAlias":"client","keyStoreAliasPassword":{"type":"Embedded","data":{"ciphertext":"8NrEHB5pqW7bNg","encrypted_key":"y8P-e47FM-UgPOoPMXR7VqjMNiyFfnhCu0ESa7rGWNdbi1DX0UBlkA","iv":"IsJjpj5Njj3wwd9m","protected":"eyJhbGciOiJBMjU2S1ciLCJlbmMiOiJBMjU2R0NNIiwiaWF0IjoxNzkyMDA2OTcyLCJ6aXAiOiJERUYifQ","tag":"QFa1ONOcQDwDLgsHAsVKjQ"}}},"failover":{"enabled":false,"threshold":3,"discoveryUrl":"","endpointUrl":"","hostOverride":""},"configVersion":2}},"data":["config.json"],"attributes":{"lastModification":{"actor":"unknown","timestamp":"2026-10-14T19:42:52Z"},"uuid":"fbce2517-6b4b-45e2-bf95-692fcd6c5dbd","lastModificationSignature":"b51c418bec2bbbf9fb23a786b27dd050fa1d388b2caece4c9987da9a6579c0f2"},"metrics":{},"healthchecks":{"status":{"name":"connections.connection-Ignition OPC UA Server.status","result":{"healthy":true,"time":1792008058973}},"uptime":{"name":"connections.connection-Ignition OPC UA Server.uptime","result":{"healthy":true,"time":1792008058973,"details":{"since":"2026-10-14T19:47:11.362331637Z","milliseconds":827611}}}}}],"metadata":{"total":1,"matching":1,"limit":-1,"offset":0}}
[curl http=200 total=0.017976s]
~~~

#### GET resources/list/ignition/quickstart

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/ignition/quickstart?limit=-1'` @ 2026-10-14T20:00:59Z

~~~
HTTP/1.1 404 Not Found
Date: Wed, 14 Oct 2026 20:00:59 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 464

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 No route match for path: /v1/resources/list/ignition/quickstart</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/resources/list/ignition/quickstart</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>No route match for path: /v1/resources/list/ignition/quickstart</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.016426s]
~~~

#### GET resources/singleton/ignition/quickstart

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/singleton/ignition/quickstart'` @ 2026-10-14T20:00:59Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:59 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 770

{
  "type": "ignition/quickstart",
  "description": "Quick Start Configuration",
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "60632702c1039dbf72041f580af620a6971c8f21617639a7c081fc103763d365",
  "config": {
    "finished": false,
    "steps": [
      {
        "completed": false,
        "name": "INSTALL"
      },
      {
        "completed": false,
        "name": "ENABLED"
      },
      {
        "completed": false,
        "name": "DESIGNER"
      },
      {
        "completed": false,
        "name": "PROJECT"
      },
      {
        "completed": false,
        "name": "SESSION"
      }
    ]
  },
  "data": [
    "config.json"
  ],
  "attributes": {},
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.040047s]
~~~

#### GET resources/list/ignition/schedule

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/ignition/schedule?limit=-1'` @ 2026-10-14T20:00:59Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:59 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 2072

{"items":[{"type":"ignition/schedule","name":"Always","description":"Built-in schedule that is always available: 24x7x365","enabled":true,"version":1,"collection":"system","collections":["system"],"signature":"67d1dbd7df1cab90563155f5ada0f8221553ac8a9c350f85e938ba9830926f08","config":{"profile":{"type":"basic schedule"},"settings":{"observeHolidays":false,"allDays":true,"allDayTime":"0:00-24:00","weekDays":false,"weekDayTime":"0:00-24:00","monday":false,"tuesday":false,"wednesday":false,"thursday":false,"friday":false,"saturday":false,"sunday":false,"mondayTime":"0:00-24:00","tuesdayTime":"0:00-24:00","wednesdayTime":"0:00-24:00","thursdayTime":"0:00-24:00","fridayTime":"0:00-24:00","saturdayTime":"0:00-24:00","sundayTime":"0:00-24:00","repeatMode":"Off","repeatOn":1,"repeatOff":1}},"data":["config.json"],"attributes":{},"metrics":{},"healthchecks":{"status":{"name":"schedules.Always.status","result":{"healthy":true,"time":1792008059339,"message":"Active"}}}},{"type":"ignition/schedule","name":"Example","description":"An example of a M-F 8am-5pm schedule with a lunch break","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"8f5ad1da2569d32e03191a8e683dbcafff530bdd7f1486c4ce59188394de3b51","config":{"profile":{"type":"basic schedule"},"settings":{"observeHolidays":true,"allDays":false,"allDayTime":"0:00-24:00","weekDays":true,"weekDayTime":"8:00-11:45,12:30-17:00","monday":false,"tuesday":false,"wednesday":false,"thursday":false,"friday":false,"saturday":false,"sunday":false,"mondayTime":"0:00-24:00","tuesdayTime":"0:00-24:00","wednesdayTime":"0:00-24:00","thursdayTime":"0:00-24:00","fridayTime":"0:00-24:00","saturdayTime":"0:00-24:00","sundayTime":"0:00-24:00","repeatMode":"Off","repeatOn":1,"repeatOff":1}},"data":["config.json"],"attributes":{"uuid":"1fbd8324-7f74-44d3-90f5-b7a64de8d468","enabled":true},"metrics":{},"healthchecks":{"status":{"name":"schedules.Example.status","result":{"healthy":false,"time":1792008059341,"message":"Inactive"}}}}],"metadata":{"total":2,"matching":2,"limit":-1,"offset":0}}
[curl http=200 total=0.016363s]
~~~

#### GET resources/list/ignition/security-levels

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/ignition/security-levels?limit=-1'` @ 2026-10-14T20:00:59Z

~~~
HTTP/1.1 404 Not Found
Date: Wed, 14 Oct 2026 20:00:59 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 479

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 No route match for path: /v1/resources/list/ignition/security-levels</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/resources/list/ignition/security-levels</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>No route match for path: /v1/resources/list/ignition/security-levels</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.011608s]
~~~

#### GET resources/singleton/ignition/security-levels

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/singleton/ignition/security-levels'` @ 2026-10-14T20:00:59Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:59 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 2073

{
  "type": "ignition/security-levels",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "33de3d84b9589debf1a5137af02da2da16af9445999c688e29f485d452b5e165",
  "config": {
    "securityLevels": [
      {
        "name": "Authenticated",
        "description": "Represents a user who has been authenticated by the system.",
        "children": [
          {
            "name": "Deploy",
            "description": "Custom level for pipeline API tokens (system Roles levels cannot be granted to config-defined tokens in 8.3.6)",
            "children": []
          },
          {
            "name": "Roles",
            "description": "Represents the roles that a user has.",
            "children": [
              {
                "name": "Administrator",
                "description": "System generated security level representing read and write privileges to Gateway configuration",
                "children": [
                  {
                    "name": "SecurityZones",
                    "description": "Represents the security zones that a user has.",
                    "children": [
                      {
                        "name": "Default",
                        "description": "This zone cannot be edited, and will be used for zone-based settings when no other zone is matched.",
                        "children": []
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
        "name": "SecurityZones",
        "description": "Represents the security zones that a user has.",
        "children": [
          {
            "name": "Default",
            "description": "This zone cannot be edited, and will be used for zone-based settings when no other zone is matched.",
            "children": []
          }
        ]
      }
    ]
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
[curl http=200 total=0.060686s]
~~~

#### GET resources/list/ignition/security-properties

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/ignition/security-properties?limit=-1'` @ 2026-10-14T20:00:59Z

~~~
HTTP/1.1 404 Not Found
Date: Wed, 14 Oct 2026 20:00:59 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 491

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 No route match for path: /v1/resources/list/ignition/security-properties</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/resources/list/ignition/security-properties</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>No route match for path: /v1/resources/list/ignition/security-properties</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.016379s]
~~~

#### GET resources/singleton/ignition/security-properties

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/singleton/ignition/security-properties'` @ 2026-10-14T20:00:59Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:00:59 GMT
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
[curl http=200 total=0.025656s]
~~~

#### GET resources/list/ignition/system-properties

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/ignition/system-properties?limit=-1'` @ 2026-10-14T20:00:59Z

~~~
HTTP/1.1 404 Not Found
Date: Wed, 14 Oct 2026 20:00:59 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 485

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 No route match for path: /v1/resources/list/ignition/system-properties</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/resources/list/ignition/system-properties</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>No route match for path: /v1/resources/list/ignition/system-properties</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.014325s]
~~~

#### GET resources/singleton/ignition/system-properties

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/singleton/ignition/system-properties'` @ 2026-10-14T20:00:59Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:01:00 GMT
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
  "signature": "c25e970c9a133652b167cc5c5034b1bcf836efdfe145e255f25e9262fda4c522",
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
      "timestamp": "2026-10-14T19:42:50Z"
    },
    "lastModificationSignature": "f346887378f904d82859a177bc7b12d4c5d8a95fb4c01d784da0a4e1f3a82744"
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.020294s]
~~~

#### GET resources/list/ignition/translations

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/ignition/translations?limit=-1'` @ 2026-10-14T20:01:00Z

~~~
HTTP/1.1 404 Not Found
Date: Wed, 14 Oct 2026 20:01:00 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Cache-Control: must-revalidate,no-cache,no-store
Content-Type: text/html;charset=iso-8859-1
Content-Length: 470

<html>
<head>
<meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/>
<title>Error 404</title>
</head>
<body><h2>HTTP ERROR 404 No route match for path: /v1/resources/list/ignition/translations</h2>
<table>
<tr><th>URI:</th><td>/data/api/v1/resources/list/ignition/translations</td></tr>
<tr><th>STATUS:</th><td>404</td></tr>
<tr><th>MESSAGE:</th><td>No route match for path: /v1/resources/list/ignition/translations</td></tr>
</table>

</body>
</html>

[curl http=404 total=0.017502s]
~~~

#### GET resources/singleton/ignition/translations

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/singleton/ignition/translations'` @ 2026-10-14T20:01:00Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:01:00 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 791

{
  "type": "ignition/translations",
  "description": null,
  "enabled": true,
  "version": 1,
  "collection": "core",
  "collections": [
    "core"
  ],
  "signature": "5af4cdf688184558acfc56e29c3243e4d98606b8d9731d182fb605d52c184090",
  "config": {
    "caseInsensitive": false,
    "ignorePunctuation": false,
    "ignoreTags": false,
    "ignoreWhitespace": false,
    "terms": {}
  },
  "data": [],
  "attributes": {
    "lastModification": {
      "actor": "8.3-migration",
      "timestamp": "2026-10-14T19:40:49Z"
    },
    "ignorePunctuation": false,
    "caseInsensitive": false,
    "lastModificationSignature": "10387ec27056b429f7ab5e74ee8314d2b93caaaeaa9c2a30071271487913cab4",
    "ignoreWhitespace": false,
    "ignoreTags": false
  },
  "metrics": {},
  "healthchecks": {}
}
[curl http=200 total=0.019506s]
~~~

#### GET resources/list/ignition/user-source

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/resources/list/ignition/user-source?limit=-1'` @ 2026-10-14T20:01:00Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:01:00 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 1972

{"items":[{"type":"ignition/user-source","name":"default","description":"This user source profile was automatically created during a startup","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"fda847c828b1a7c8d1f1aa491cdde85cd10512b8626dfdf11e6c0faea2253c04","config":{"profile":{"type":"INTERNAL","scheduleRestricted":false,"failoverMode":"HARD","cacheValidationTimeout":15,"lockoutEnabled":true,"lockoutAttempts":5,"lockoutWindow":15},"settings":{"passwordMaxAge":0,"passwordMinLength":8,"passwordComplexity":3,"passwordHistory":0,"passwordContainsPassword":true,"passwordContainsUserName":true,"passwordMaxRepeatedChars":3}},"data":["config.json","users.json"],"attributes":{"lastModification":{"actor":"system-init","timestamp":"2026-10-14T19:40:51Z"},"uuid":"ee1efd48-27b9-4d67-a608-09db3a60609c","lastModificationSignature":"3ee093ffd74bfa5a15072a958d8d6efc9231c38c20f1586b4499d8b9a0fc521c"},"metrics":{},"healthchecks":{}},{"type":"ignition/user-source","name":"opcua-module","description":"OPC UA clients will authenticate against this profile by default.","enabled":true,"version":1,"collection":"core","collections":["core"],"signature":"4b7bd8387851d5c69ea66395656a5804c6f9ca73471675d503e448a354a8512e","config":{"profile":{"type":"INTERNAL","scheduleRestricted":false,"failoverMode":"HARD","cacheValidationTimeout":15,"lockoutEnabled":true,"lockoutAttempts":5,"lockoutWindow":15},"settings":{"passwordMaxAge":0,"passwordMinLength":8,"passwordComplexity":3,"passwordHistory":0,"passwordContainsPassword":true,"passwordContainsUserName":true,"passwordMaxRepeatedChars":3}},"data":["config.json","users.json"],"attributes":{"lastModification":{"actor":"unknown","timestamp":"2026-10-14T19:42:12Z"},"uuid":"484bae27-ffcc-4e79-af61-1eeddf35add5","lastModificationSignature":"d1dbc518ba507a608800cdc88a12ac3d7883df8bdd2d1db7fdebf4ed0481cd01"},"metrics":{},"healthchecks":{}}],"metadata":{"total":2,"matching":2,"limit":-1,"offset":0}}
[curl http=200 total=0.023014s]
~~~

#### per-type equality of REST config and file config.json (differing JSON paths only)

`bash -c bash\ \"\$CAPTURE22\"\ cv2cmp ` @ 2026-10-14T20:01:00Z

~~~
com.inductiveautomation.eam/event-thresholds: equal (core)
com.inductiveautomation.eam/module-settings: equal (core)
com.inductiveautomation.opcua/access-control: equal (core)
com.inductiveautomation.opcua/one-time: no REST read route (HTTP 404 on the singleton route); not compared
com.inductiveautomation.opcua/server-config: equal (core)
com.inductiveautomation.perspective/themes/dark: collection 'system' (gateway-provided, not file-backed); not compared
com.inductiveautomation.perspective/themes/dark-cool: equal (core)
com.inductiveautomation.perspective/themes/dark-warm: equal (core)
com.inductiveautomation.perspective/themes/light: collection 'system' (gateway-provided, not file-backed); not compared
com.inductiveautomation.perspective/themes/light-cool: equal (core)
com.inductiveautomation.perspective/themes/light-warm: equal (core)
com.inductiveautomation.sfc/chart-settings: equal (core)
ignition/cobranding: equal (core)
ignition/database-driver/MariaDB: collection 'system' (gateway-provided, not file-backed); not compared
ignition/database-driver/Microsoft SQLServer: collection 'system' (gateway-provided, not file-backed); not compared
ignition/database-driver/MySQL: equal (core)
ignition/database-driver/Oracle Database: equal (core)
ignition/database-driver/PostgreSQL: collection 'system' (gateway-provided, not file-backed); not compared
ignition/database-driver/SQLite: equal (core)
ignition/database-translator/GENERIC: equal (core)
ignition/database-translator/MSSQL: equal (core)
ignition/database-translator/MYSQL: equal (core)
ignition/database-translator/ORACLE: equal (core)
ignition/database-translator/POSTGRES: equal after N1 (file members with JSON null, absent in REST: createAutoIncSequence, createAutoIncTrigger) (production)
ignition/database-translator/SQLITE: equal (core)
ignition/edge-system-properties: equal (core)
ignition/gateway-network-proxy-rules: equal (core)
ignition/gateway-network-queue-settings: equal (core)
ignition/gateway-network-settings: equal (core)
ignition/general-alarm-settings: equal (production)
ignition/identity-provider/default: equal (core)
ignition/images: no REST read route (HTTP 404 on the singleton route); not compared
ignition/keyboard_layout/de_qwertz: equal (core)
ignition/keyboard_layout/es_qwerty: equal (core)
ignition/keyboard_layout/fr_azerty: equal (core)
ignition/keyboard_layout/it_qwerty: equal (core)
ignition/opc-connection/Ignition OPC UA Server: equal (core)
ignition/quickstart: equal (core)
ignition/schedule/Always: collection 'system' (gateway-provided, not file-backed); not compared
ignition/schedule/Example: equal (core)
ignition/security-levels: DIFFERS {"file_is_name_keyed_subset_of_rest":true,"value_differs":["securityLevels.0.children.0.description","securityLevels.0.children.0.name","securityLevels.0.children.1.description","securityLevels.0.children.1.name"],"only_in_rest":["securityLevels.0.children.1.children.0.children.0.children.0.description","securityLevels.0.children.1.children.0.children.0.children.0.name","securityLevels.0.children.1.children.0.children.0.description","securityLevels.0.children.1.children.0.children.0.name","securityLevels.0.children.1.children.0.description","securityLevels.0.children.1.children.0.name","securityLevels.1.children.0.description","securityLevels.1.children.0.name","securityLevels.1.description","securityLevels.1.name"],"only_in_file":["securityLevels.0.children.0.children.0.description","securityLevels.0.children.0.children.0.name"]} (core)
ignition/security-properties: equal (core)
ignition/system-properties: equal (core)
ignition/translations: resource.json files [] (no config.json) (core): {"rest_config_keys":["caseInsensitive","ignorePunctuation","ignoreTags","ignoreWhitespace","terms"],"keys_also_in_resource_json_attributes_equal":true,"rest_only_keys":["terms"]}
ignition/user-source/default: equal (core)
ignition/user-source/opcua-module: equal (core)
[exit 0]
~~~

## CV3 — post-scan byte stability of resource files placed by a plain file write into production/

The files of the tracer render (placed by `cv2` with `cp -R`) are hashed from the render directory (the bytes written), then from `production/` after a fresh scan, then every 5 s for 30 s, and finally compared byte for byte.

#### sha256 of the rendered files (the bytes that were written)

`bash -c cd\ \"\$RENDER22\"\ \&\&\ find\ ignition\ -type\ f\ \|\ LC_ALL=C\ sort\ \|\ while\ IFS=\ read\ -r\ f\;\ do\ shasum\ -a\ 256\ \"\$f\"\;\ done ` @ 2026-10-14T20:01:06Z

~~~
3ce93dcca017856cc6a8a373c78dc398679404cffdbf0a0c50fa9d37d78b396b  ignition/database-translator/POSTGRES/config.json
81f9e8368e56cce393594d9574392e9c0c8b41b819d7ae0a402efe49f3f3b64b  ignition/database-translator/POSTGRES/resource.json
ce86cc6f24054029adf769412b908d93f5046c5026d8c64c17a953b22cddb4e6  ignition/general-alarm-settings/config.json
736c80f4f786953276f3213b7efae0461cb3f735f897b78baab363ac6de697ac  ignition/general-alarm-settings/resource.json
[exit 0]
~~~

#### GET scan/config (before the scan)

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/scan/config'` @ 2026-10-14T20:01:07Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:01:07 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 90

{
  "scanActive": false,
  "lastScanTimestamp": 1792008049475,
  "lastScanDuration": 552
}
[curl http=200 total=0.064575s]
~~~

#### POST scan/config

`curl -sS -g -i -X POST -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/scan/config'` @ 2026-10-14T20:01:07Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:01:07 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 89

{
  "scanActive": true,
  "lastScanTimestamp": 1792008049475,
  "lastScanDuration": 552
}
[curl http=200 total=0.072553s]
~~~

#### GET scan/config (after completion)

`curl -sS -g -i -X GET -H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A' '$BASE/data/api/v1/scan/config'` @ 2026-10-14T20:01:10Z

~~~
HTTP/1.1 200 OK
Date: Wed, 14 Oct 2026 20:01:10 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 91

{
  "scanActive": false,
  "lastScanTimestamp": 1792008069348,
  "lastScanDuration": 1356
}
[curl http=200 total=0.010392s]
~~~

#### sha256 of the same files in production/ right after the scan

`bash -c find\ production/ignition\ -type\ f\ \|\ LC_ALL=C\ sort\ \|\ while\ IFS=\ read\ -r\ f\;\ do\ shasum\ -a\ 256\ \"\$f\"\;\ done ` @ 2026-10-14T20:01:10Z

~~~
3ce93dcca017856cc6a8a373c78dc398679404cffdbf0a0c50fa9d37d78b396b  production/ignition/database-translator/POSTGRES/config.json
81f9e8368e56cce393594d9574392e9c0c8b41b819d7ae0a402efe49f3f3b64b  production/ignition/database-translator/POSTGRES/resource.json
ce86cc6f24054029adf769412b908d93f5046c5026d8c64c17a953b22cddb4e6  production/ignition/general-alarm-settings/config.json
736c80f4f786953276f3213b7efae0461cb3f735f897b78baab363ac6de697ac  production/ignition/general-alarm-settings/resource.json
[exit 0]
~~~

#### sha256 in production/ at +5 s after the scan

`bash -c find\ production/ignition\ -type\ f\ \|\ LC_ALL=C\ sort\ \|\ while\ IFS=\ read\ -r\ f\;\ do\ shasum\ -a\ 256\ \"\$f\"\;\ done ` @ 2026-10-14T20:01:15Z

~~~
3ce93dcca017856cc6a8a373c78dc398679404cffdbf0a0c50fa9d37d78b396b  production/ignition/database-translator/POSTGRES/config.json
81f9e8368e56cce393594d9574392e9c0c8b41b819d7ae0a402efe49f3f3b64b  production/ignition/database-translator/POSTGRES/resource.json
ce86cc6f24054029adf769412b908d93f5046c5026d8c64c17a953b22cddb4e6  production/ignition/general-alarm-settings/config.json
736c80f4f786953276f3213b7efae0461cb3f735f897b78baab363ac6de697ac  production/ignition/general-alarm-settings/resource.json
[exit 0]
~~~

#### sha256 in production/ at +10 s after the scan

`bash -c find\ production/ignition\ -type\ f\ \|\ LC_ALL=C\ sort\ \|\ while\ IFS=\ read\ -r\ f\;\ do\ shasum\ -a\ 256\ \"\$f\"\;\ done ` @ 2026-10-14T20:01:20Z

~~~
3ce93dcca017856cc6a8a373c78dc398679404cffdbf0a0c50fa9d37d78b396b  production/ignition/database-translator/POSTGRES/config.json
81f9e8368e56cce393594d9574392e9c0c8b41b819d7ae0a402efe49f3f3b64b  production/ignition/database-translator/POSTGRES/resource.json
ce86cc6f24054029adf769412b908d93f5046c5026d8c64c17a953b22cddb4e6  production/ignition/general-alarm-settings/config.json
736c80f4f786953276f3213b7efae0461cb3f735f897b78baab363ac6de697ac  production/ignition/general-alarm-settings/resource.json
[exit 0]
~~~

#### sha256 in production/ at +15 s after the scan

`bash -c find\ production/ignition\ -type\ f\ \|\ LC_ALL=C\ sort\ \|\ while\ IFS=\ read\ -r\ f\;\ do\ shasum\ -a\ 256\ \"\$f\"\;\ done ` @ 2026-10-14T20:01:26Z

~~~
3ce93dcca017856cc6a8a373c78dc398679404cffdbf0a0c50fa9d37d78b396b  production/ignition/database-translator/POSTGRES/config.json
81f9e8368e56cce393594d9574392e9c0c8b41b819d7ae0a402efe49f3f3b64b  production/ignition/database-translator/POSTGRES/resource.json
ce86cc6f24054029adf769412b908d93f5046c5026d8c64c17a953b22cddb4e6  production/ignition/general-alarm-settings/config.json
736c80f4f786953276f3213b7efae0461cb3f735f897b78baab363ac6de697ac  production/ignition/general-alarm-settings/resource.json
[exit 0]
~~~

#### sha256 in production/ at +20 s after the scan

`bash -c find\ production/ignition\ -type\ f\ \|\ LC_ALL=C\ sort\ \|\ while\ IFS=\ read\ -r\ f\;\ do\ shasum\ -a\ 256\ \"\$f\"\;\ done ` @ 2026-10-14T20:01:31Z

~~~
3ce93dcca017856cc6a8a373c78dc398679404cffdbf0a0c50fa9d37d78b396b  production/ignition/database-translator/POSTGRES/config.json
81f9e8368e56cce393594d9574392e9c0c8b41b819d7ae0a402efe49f3f3b64b  production/ignition/database-translator/POSTGRES/resource.json
ce86cc6f24054029adf769412b908d93f5046c5026d8c64c17a953b22cddb4e6  production/ignition/general-alarm-settings/config.json
736c80f4f786953276f3213b7efae0461cb3f735f897b78baab363ac6de697ac  production/ignition/general-alarm-settings/resource.json
[exit 0]
~~~

#### sha256 in production/ at +25 s after the scan

`bash -c find\ production/ignition\ -type\ f\ \|\ LC_ALL=C\ sort\ \|\ while\ IFS=\ read\ -r\ f\;\ do\ shasum\ -a\ 256\ \"\$f\"\;\ done ` @ 2026-10-14T20:01:37Z

~~~
3ce93dcca017856cc6a8a373c78dc398679404cffdbf0a0c50fa9d37d78b396b  production/ignition/database-translator/POSTGRES/config.json
81f9e8368e56cce393594d9574392e9c0c8b41b819d7ae0a402efe49f3f3b64b  production/ignition/database-translator/POSTGRES/resource.json
ce86cc6f24054029adf769412b908d93f5046c5026d8c64c17a953b22cddb4e6  production/ignition/general-alarm-settings/config.json
736c80f4f786953276f3213b7efae0461cb3f735f897b78baab363ac6de697ac  production/ignition/general-alarm-settings/resource.json
[exit 0]
~~~

#### sha256 in production/ at +30 s after the scan

`bash -c find\ production/ignition\ -type\ f\ \|\ LC_ALL=C\ sort\ \|\ while\ IFS=\ read\ -r\ f\;\ do\ shasum\ -a\ 256\ \"\$f\"\;\ done ` @ 2026-10-14T20:01:44Z

~~~
3ce93dcca017856cc6a8a373c78dc398679404cffdbf0a0c50fa9d37d78b396b  production/ignition/database-translator/POSTGRES/config.json
81f9e8368e56cce393594d9574392e9c0c8b41b819d7ae0a402efe49f3f3b64b  production/ignition/database-translator/POSTGRES/resource.json
ce86cc6f24054029adf769412b908d93f5046c5026d8c64c17a953b22cddb4e6  production/ignition/general-alarm-settings/config.json
736c80f4f786953276f3213b7efae0461cb3f735f897b78baab363ac6de697ac  production/ignition/general-alarm-settings/resource.json
[exit 0]
~~~

#### byte comparison: every rendered file against its copy in production/ (cmp)

`bash -c cd\ \"\$RENDER22\"\ \&\&\ find\ ignition\ -type\ f\ \|\ LC_ALL=C\ sort\ \|\ while\ IFS=\ read\ -r\ f\;\ do\ if\ cmp\ -s\ \"\$f\"\ \"\$BIND/production/\$f\"\;\ then\ echo\ \"identical\ \$f\"\;\ else\ echo\ \"DIFFERS\ \$f\"\;\ fi\;\ done ` @ 2026-10-14T20:01:45Z

~~~
identical ignition/database-translator/POSTGRES/config.json
identical ignition/database-translator/POSTGRES/resource.json
identical ignition/general-alarm-settings/config.json
identical ignition/general-alarm-settings/resource.json
[exit 0]
~~~


## C7a — Windows OpenSSH behaviours as svc-ignition-deploy (D-13, D-15)

Capture host: 40.114.71.134, an Azure VM (Windows Server 2022) standing in for WFLWVM01 by user decision of 2026-10-14. Account: `svc-ignition-deploy`. Every command runs from the macOS runner with the hardened option set of `hardened_options`; `<key>` is the temp key file. `ssh` is ssh -T with the remote command as one argument; `sftp` is `sftp -q -b -` with the batch on stdin. The Windows-client ssh.exe stdin item of the C7a procedure is NOT applicable to a macOS runner; it is exercised by the windows-latest CI leg (22-13).

`<H>` stands for exactly these ssh/sftp options, in this order (the `hardened_options` set with an identity): `-o BatchMode=yes -o StrictHostKeyChecking=yes -o UserKnownHostsFile=$HOME/.cache/ign-wflwvm01/known_hosts -o GlobalKnownHostsFile=/dev/null -o UpdateHostKeys=no -o VerifyHostKeyDNS=no -o PreferredAuthentications=publickey -o PasswordAuthentication=no -o KbdInteractiveAuthentication=no -o NumberOfPasswordPrompts=0 -o IdentitiesOnly=yes -o ForwardAgent=no -o ForwardX11=no -o ClearAllForwardings=yes -o PermitLocalCommand=no -o ControlMaster=no -o ControlPath=none -o ConnectTimeout=10 -o ServerAliveInterval=5 -o ServerAliveCountMax=3 -o LogLevel=ERROR `

### C7a-host — account, shell, groups, host key types

#### whoami

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 whoami ` @ 2026-10-15T02:34:16Z

~~~
[stdout]
ignwin836\svc-ignition-deploy
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### hostname

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 hostname ` @ 2026-10-15T02:34:17Z

~~~
[stdout]
ignwin836
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### ver

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ver ` @ 2026-10-15T02:34:18Z

~~~
[stdout]

Microsoft Windows [Version 10.0.20348.5761]
[2 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### default shell: COMSPEC and the registry value

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 echo\ COMSPEC=%COMSPEC%\ \&\ reg\ query\ HKLM\\SOFTWARE\\OpenSSH\ /v\ DefaultShell ` @ 2026-10-15T02:34:20Z

~~~
[stdout]
COMSPEC=C:\Windows\system32\cmd.exe 


[3 CR bytes removed from stdout]
[stderr]
ERROR: The system was unable to find the specified registry key or value.
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### USERPROFILE

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 echo\ %USERPROFILE% ` @ 2026-10-15T02:34:21Z

~~~
[stdout]
C:\Users\svc-ignition-deploy
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### the account groups (whoami /groups)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 whoami\ /groups ` @ 2026-10-15T02:34:22Z

~~~
[stdout]

GROUP INFORMATION
-----------------

Group Name                             Type             SID          Attributes                                        
====================================== ================ ============ ==================================================
Everyone                               Well-known group S-1-1-0      Mandatory group, Enabled by default, Enabled group
BUILTIN\Users                          Alias            S-1-5-32-545 Mandatory group, Enabled by default, Enabled group
NT AUTHORITY\NETWORK                   Well-known group S-1-5-2      Mandatory group, Enabled by default, Enabled group
NT AUTHORITY\Authenticated Users       Well-known group S-1-5-11     Mandatory group, Enabled by default, Enabled group
NT AUTHORITY\This Organization         Well-known group S-1-5-15     Mandatory group, Enabled by default, Enabled group
NT AUTHORITY\Local account             Well-known group S-1-5-113    Mandatory group, Enabled by default, Enabled group
NT AUTHORITY\NTLM Authentication       Well-known group S-1-5-64-10  Mandatory group, Enabled by default, Enabled group
Mandatory Label\Medium Mandatory Level Label            S-1-16-8192                                                    
[14 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### the account privileges (whoami /priv)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 whoami\ /priv ` @ 2026-10-15T02:34:24Z

~~~
[stdout]

PRIVILEGES INFORMATION
----------------------

Privilege Name                Description                    State  
============================= ============================== =======
SeChangeNotifyPrivilege       Bypass traverse checking       Enabled
SeIncreaseWorkingSetPrivilege Increase a process working set Enabled
[8 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### is the account an administrator? (net localgroup Administrators)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 net\ localgroup\ Administrators ` @ 2026-10-15T02:34:25Z

~~~
[stdout]
Alias name     Administrators
Comment        Administrators have complete and unrestricted access to the computer/domain

Members

-------------------------------------------------------------------------------
ignadmin
The command completed successfully.

[9 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### PowerShell version on the host

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIAAkAEUAcgByAG8AcgBBAGMAdABpAG8AbgBQAHIAZQBmAGUAcgBlAG4AYwBlAD0AJwBTAHQAbwBwACcAOwAgACQAUABTAFYAZQByAHMAaQBvAG4AVABhAGIAbABlAC4AUABTAFYAZQByAHMAaQBvAG4ALgBUAG8AUwB0AHIAaQBuAGcAKAApAA== ` @ 2026-10-15T02:34:26Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; $PSVersionTable.PSVersion.ToString()
[stdout]
5.1.20348.5756
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### host key types offered: ssh-keyscan -t ed25519,rsa,ecdsa (fingerprints via ssh-keygen -lf -)

`bash -c ssh-keyscan\ -p\ \"\$1\"\ -t\ ed25519\,rsa\,ecdsa\ \"\$2\"\ 2\>\&1\ \|\ tee\ /dev/stderr\ \|\ ssh-keygen\ -lf\ -\ 2\>\&1 _ 22 40.114.71.134 ` @ 2026-10-15T02:34:28Z

~~~
[stdout]
256 SHA256:CfSLIMXEfRsxKLydfjt7/I6skou0EBb0a4tBGBYs6D0 40.114.71.134 (ECDSA)
3072 SHA256:JFUQNah6KkrSZEA1I/BonrLJ6MVu4mTWENEgbEBgoYc 40.114.71.134 (RSA)
256 SHA256:wgjZzSMU/Ss+bnO8SgMfOY90cnutJvlAuxSGNPJNkZo 40.114.71.134 (ED25519)
[stderr]
# 40.114.71.134:22 SSH-2.0-OpenSSH_for_Windows_8.1
40.114.71.134 ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBFwfiMH9jljf3UmtSCQQ631fhn+vxyWq/Wa7YpixaJNsRp2xKLBeiiC3n2qQFm1qbLcrDlWWW+ln9xPG1Y+tsvU=
# 40.114.71.134:22 SSH-2.0-OpenSSH_for_Windows_8.1
40.114.71.134 ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQDIKmJnzho4cUNq0x3ab1cdSnPGTylGxHWy+wigT4E00PEExlyuCcvnYxvoIrv1hmuCAenpDPx4tkF4URB5mKqJoOdAcf4Xc7rJoomgFltfo808n6vGr7f6r1Lzm7aIa0CR5xACQ1xjyYl7/FqCZVhC8QY73ptJrWWURkR+eHEYzE00TOmPMdmx9LClJ+tuLRkOPDCwg2eczm1lbEFMG6ew4qmeTLajPJ+X4wvDsqJ+NJA82OHBeTzXBRIkXyPFqfnknzodkpb03ioA0N+htQvVODYnWapQhc1fRWSxcbcNG2WJxEpSaU/ToOUxhstjB/y22TdH3GVPAObvHGfIUzDk2hoyJbZlbKEVFAKE9w81U5uaYVA1eCDx5Qvs/uiw47s4lfbj9RBENru6/NfOGCXlQqc1Kpn+KL6A/PJajjRhpAJ3X48mRzFHhlRBROA3xcJY++6//H2f4YiCcI0zu7ZrGYamAegRuQMhgMdo051gZ0kiuHslXSk6Q3hvXmkV5G0=
# 40.114.71.134:22 SSH-2.0-OpenSSH_for_Windows_8.1
40.114.71.134 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIASouJX1oLlJYlXh11eqmpE0nfqK5SM6Dsurw64fxmqI
[exit 0]
~~~

#### host key types the server offers in KEX (ssh -vv, algorithm lines only; LogLevel option left out of this one block so -vv shows)

`bash -c $'\n    ssh -vv -T -p "$1" -i "$2" -o BatchMode=yes -o StrictHostKeyChecking=yes -o "UserKnownHostsFile=$3" -o GlobalKnownHostsFile=/dev/null -o IdentitiesOnly=yes -o ConnectTimeout=10 -- "$4" exit 0 2>&1 </dev/null | grep -E "host key algorithms|kex algorithms|Server host key|Remote protocol version" | cut -c1-300' _ 22 <key> $HOME/.cache/ign-wflwvm01/known_hosts svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:34:29Z

~~~
[stdout]
debug1: Remote protocol version 2.0, remote software version OpenSSH_for_Windows_8.1
debug2: host key algorithms: ssh-ed25519-cert-v01@openssh.com,ecdsa-sha2-nistp256-cert-v01@openssh.com,ecdsa-sha2-nistp384-cert-v01@openssh.com,ecdsa-sha2-nistp521-cert-v01@openssh.com,sk-ssh-ed25519-cert-v01@openssh.com,sk-ecdsa-sha2-nistp256-cert-v01@openssh.com,webauthn-sk-ecdsa-sha2-nistp256-cer
debug2: host key algorithms: rsa-sha2-512,rsa-sha2-256,ssh-rsa,ecdsa-sha2-nistp256,ssh-ed25519
debug1: Server host key: ssh-ed25519 SHA256:wgjZzSMU/Ss+bnO8SgMfOY90cnutJvlAuxSGNPJNkZo
debug2: host key algorithms: ssh-ed25519-cert-v01@openssh.com,ecdsa-sha2-nistp256-cert-v01@openssh.com,ecdsa-sha2-nistp384-cert-v01@openssh.com,ecdsa-sha2-nistp521-cert-v01@openssh.com,sk-ssh-ed25519-cert-v01@openssh.com,sk-ecdsa-sha2-nistp256-cert-v01@openssh.com,webauthn-sk-ecdsa-sha2-nistp256-cer
[3 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

Home as seen by sftp: `/C:/Users/svc-ignition-deploy`; as seen by cmd.exe: `C:\Users\svc-ignition-deploy`. All scratch work is under `ign-c7a/` in that home and is removed at the end.

### C7a-1 — sftp path forms

#### pwd and a listing of the login directory

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:34:33Z

~~~
[stdin]
pwd
@ls -1 "."
[stdout]
sftp> pwd
Remote working directory: /C:/Users/svc-ignition-deploy
./AppData
./Application Data
./Cookies
./Desktop
./Documents
./Downloads
./Favorites
./Links
./Local Settings
./Music
./My Documents
./NTUSER.DAT
./NTUSER.DAT{6b3cbf8b-c6cb-11f1-8338-00155d013703}.TM.blf
./NTUSER.DAT{6b3cbf8b-c6cb-11f1-8338-00155d013703}.TMContainer00000000000000000001.regtrans-ms
./NTUSER.DAT{6b3cbf8b-c6cb-11f1-8338-00155d013703}.TMContainer00000000000000000002.regtrans-ms
./NetHood
./Pictures
./PrintHood
./Recent
./Saved Games
./SendTo
./Start Menu
./Templates
./Videos
./ntuser.dat.LOG1
./ntuser.dat.LOG2
./ntuser.ini
[stderr]
[exit 0]
~~~

#### create the scratch directory (relative path)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:34:35Z

~~~
[stdin]
mkdir "ign-c7a"
mkdir "ign-c7a/d"
@ls -1 "ign-c7a"
[stdout]
sftp> mkdir "ign-c7a"
sftp> mkdir "ign-c7a/d"
ign-c7a/d
[stderr]
[exit 0]
~~~

#### ls -1 with the path form: /C:/Program Files/Inductive Automation/Ignition/data/config/resources

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:34:37Z

~~~
[stdin]
@ls -1 "/C:/Program Files/Inductive Automation/Ignition/data/config/resources"
[stdout]
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/external
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/local
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/migration-log-202610150204.md
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production
[stderr]
[exit 0]
~~~

#### ls -1 with the path form: C:/Program Files/Inductive Automation/Ignition/data/config/resources

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:34:39Z

~~~
[stdin]
@ls -1 "C:/Program Files/Inductive Automation/Ignition/data/config/resources"
[stdout]
[stderr]
Can't ls: "/C:/Users/svc-ignition-deploy/C:/Program Files/Inductive Automation/Ignition/data/config/resources" not found
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### ls -1 with the path form: /c:/program files/inductive automation/ignition/data/config/resources

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:34:41Z

~~~
[stdin]
@ls -1 "/c:/program files/inductive automation/ignition/data/config/resources"
[stdout]
/c:/program files/inductive automation/ignition/data/config/resources/core
/c:/program files/inductive automation/ignition/data/config/resources/external
/c:/program files/inductive automation/ignition/data/config/resources/local
/c:/program files/inductive automation/ignition/data/config/resources/migration-log-202610150204.md
/c:/program files/inductive automation/ignition/data/config/resources/production
[stderr]
[exit 0]
~~~

#### ls -1 with the path form: /C:\Program Files\Inductive Automation\Ignition\data\config\resources

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:34:43Z

~~~
[stdin]
@ls -1 "/C:\Program Files\Inductive Automation\Ignition\data\config\resources"
[stdout]
[stderr]
Can't ls: "/C:\\Program Files\\Inductive Automation\\Ignition\\data\\config\\resources" not found
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### ls -1 with the path form: C:\Program Files\Inductive Automation\Ignition\data\config\resources

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:34:44Z

~~~
[stdin]
@ls -1 "C:\Program Files\Inductive Automation\Ignition\data\config\resources"
[stdout]
[stderr]
Can't ls: "/C:/Users/svc-ignition-deploy/C:\\Program Files\\Inductive Automation\\Ignition\\data\\config\\resources" not found
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### ls -1 with the path form: /Program Files/Inductive Automation/Ignition/data/config/resources

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:34:46Z

~~~
[stdin]
@ls -1 "/Program Files/Inductive Automation/Ignition/data/config/resources"
[stdout]
/Program Files/Inductive Automation/Ignition/data/config/resources/core
/Program Files/Inductive Automation/Ignition/data/config/resources/external
/Program Files/Inductive Automation/Ignition/data/config/resources/local
/Program Files/Inductive Automation/Ignition/data/config/resources/migration-log-202610150204.md
/Program Files/Inductive Automation/Ignition/data/config/resources/production
[stderr]
[exit 0]
~~~

#### ls -1 with the path form: /C:/Program Files/Inductive Automation/Ignition/data/config/resources/

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:34:48Z

~~~
[stdin]
@ls -1 "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/"
[stdout]
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/external
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/local
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/migration-log-202610150204.md
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production
[stderr]
[exit 0]
~~~

#### ls -1 of the drive root forms

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:34:50Z

~~~
[stdin]
@ls -1 "/C:/"
[stdout]
/C:/$Recycle.Bin
/C:/Documents and Settings
/C:/Packages
/C:/PerfLogs
/C:/Program Files
/C:/Program Files (x86)
/C:/ProgramData
/C:/Recovery
/C:/System Volume Information
/C:/Temp
/C:/Users
/C:/Windows
/C:/WindowsAzure
/C:/ign-install
/C:/inetpub
[stderr]
[exit 0]
~~~

#### ls -1 of "/"

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:34:52Z

~~~
[stdin]
@ls -1 "/"
[stdout]
/C:
[stderr]
[exit 0]
~~~

#### ls -1 of "C:/" (no leading slash)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:34:54Z

~~~
[stdin]
@ls -1 "C:/"
[stdout]
[stderr]
Can't ls: "/C:/Users/svc-ignition-deploy/C:/" not found
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### what core/ignition holds (to pick a small directory for get -R)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:34:56Z

~~~
[stdin]
@ls -1 "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition"
[stdout]
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/cobranding
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/database-driver
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/database-translator
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/edge-system-properties
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/gateway-network-proxy-rules
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/gateway-network-queue-settings
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/gateway-network-settings
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/general-alarm-settings
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/identity-provider
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/images
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/keyboard_layout
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/opc-connection
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/quickstart
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/schedule
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/security-levels
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/security-properties
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/system-properties
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-group
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-provider
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-type-definition
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/translations
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/user-source
[stderr]
[exit 0]
~~~

#### ls -l of the small directory chosen for get -R

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:34:58Z

~~~
[stdin]
@ls -l "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/security-properties"
[stdout]
-rw-******    1 -        -            2839 Oct 15 02:07 config.json
-rw-******    1 -        -             117 Oct 15 02:07 resource.json
[stderr]
[exit 0]
~~~

#### get -R with the path form: /C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/security-properties

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:00Z

~~~
[stdin]
get -R "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/security-properties" "$TMP/local/get1"
[stdout]
sftp> get -R "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/security-properties" "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/get1"
Retrieving /C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/security-properties/
[stderr]
[exit 0]
~~~

#### what get -R left locally (find, sizes, sha256 of each file)

`bash -c cd\ \"\$1\"\ \&\&\ find\ .\ \|\ LC_ALL=C\ sort\ \&\&\ find\ .\ -type\ f\ -exec\ shasum\ -a\ 256\ \{\}\ \\\; _ $TMP/local/get1 ` @ 2026-10-15T02:35:02Z

~~~
[stdout]
.
./security-properties
./security-properties/config.json
./security-properties/resource.json
7321dabb9e1d3a8d52eb845232d4c666b2be15241fb237dba4feb3b6f51cc41d  ./security-properties/config.json
bc15af88b472315f0500943d714a752a33d15103e9e6ec28d0c63be5271edb7a  ./security-properties/resource.json
[stderr]
[exit 0]
~~~

#### get -R with the path form: C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/security-properties

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:02Z

~~~
[stdin]
get -R "C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/security-properties" "$TMP/local/get2"
[stdout]
sftp> get -R "C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/security-properties" "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/get2"
[stderr]
File "/C:/Users/svc-ignition-deploy/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/security-properties" not found.
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### what get -R left locally (find, sizes, sha256 of each file)

`bash -c cd\ \"\$1\"\ \&\&\ find\ .\ \|\ LC_ALL=C\ sort\ \&\&\ find\ .\ -type\ f\ -exec\ shasum\ -a\ 256\ \{\}\ \\\; _ $TMP/local/get2 ` @ 2026-10-15T02:35:04Z

~~~
[stdout]
.
[stderr]
[exit 0]
~~~

#### get -R with the path form: C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\security-properties

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:04Z

~~~
[stdin]
get -R "C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\security-properties" "$TMP/local/get3"
[stdout]
sftp> get -R "C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\security-properties" "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/get3"
[stderr]
File "/C:/Users/svc-ignition-deploy/C:\\Program Files\\Inductive Automation\\Ignition\\data\\config\\resources\\core\\ignition\\security-properties" not found.
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### what get -R left locally (find, sizes, sha256 of each file)

`bash -c cd\ \"\$1\"\ \&\&\ find\ .\ \|\ LC_ALL=C\ sort\ \&\&\ find\ .\ -type\ f\ -exec\ shasum\ -a\ 256\ \{\}\ \\\; _ $TMP/local/get3 ` @ 2026-10-15T02:35:06Z

~~~
[stdout]
.
[stderr]
[exit 0]
~~~

#### put with the relative path form

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:06Z

~~~
[stdin]
put "$TMP/local/a.txt" "ign-c7a/put-rel.txt"
[stdout]
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/a.txt" "ign-c7a/put-rel.txt"
[stderr]
[exit 0]
~~~

#### put with the /C:/ absolute path form

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:08Z

~~~
[stdin]
put "$TMP/local/a.txt" "/C:/Users/svc-ignition-deploy/ign-c7a/put-slashc.txt"
[stdout]
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/a.txt" "/C:/Users/svc-ignition-deploy/ign-c7a/put-slashc.txt"
[stderr]
[exit 0]
~~~

#### put with the C:/ drive path form (no leading slash)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:10Z

~~~
[stdin]
put "$TMP/local/a.txt" "C:/Users/svc-ignition-deploy/ign-c7a/put-drive.txt"
[stdout]
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/a.txt" "C:/Users/svc-ignition-deploy/ign-c7a/put-drive.txt"
[stderr]
dest open "/C:/Users/svc-ignition-deploy/C:/Users/svc-ignition-deploy/ign-c7a/put-drive.txt": No such file or directory
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### put with backslashes: ign-c7a\put-bs.txt

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:11Z

~~~
[stdin]
put "$TMP/local/a.txt" "ign-c7a\put-bs.txt"
[stdout]
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/a.txt" "ign-c7a\put-bs.txt"
[stderr]
[exit 0]
~~~

#### put with an absolute backslash drive path

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:13Z

~~~
[stdin]
put "$TMP/local/a.txt" "C:\Users\svc-ignition-deploy\ign-c7a\put-bs2.txt"
[stdout]
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/a.txt" "C:\Users\svc-ignition-deploy\ign-c7a\put-bs2.txt"
[stderr]
dest open "/C:/Users/svc-ignition-deploy/C:\\Users\\svc-ignition-deploy\\ign-c7a\\put-bs2.txt": No such file or directory
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### what exists after the puts (ls -1)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:15Z

~~~
[stdin]
@ls -1 "ign-c7a"
[stdout]
ign-c7a/d
ign-c7a/put-bs.txt
ign-c7a/put-rel.txt
ign-c7a/put-slashc.txt
[stderr]
[exit 0]
~~~

#### what exists after the puts (dir /b from the cmd.exe side)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 dir\ /b\ \"%USERPROFILE%\\ign-c7a\" ` @ 2026-10-15T02:35:17Z

~~~
[stdout]
d
put-bs.txt
put-rel.txt
put-slashc.txt
[4 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### rename, relative to relative

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:18Z

~~~
[stdin]
put "$TMP/local/b.txt" "ign-c7a/r1.txt"
rename "ign-c7a/r1.txt" "ign-c7a/r1-renamed.txt"
@ls -1 "ign-c7a"
[stdout]
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/b.txt" "ign-c7a/r1.txt"
sftp> rename "ign-c7a/r1.txt" "ign-c7a/r1-renamed.txt"
ign-c7a/d
ign-c7a/put-bs.txt
ign-c7a/put-rel.txt
ign-c7a/put-slashc.txt
ign-c7a/r1-renamed.txt
[stderr]
[exit 0]
~~~

#### rename, /C:/ absolute to /C:/ absolute

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:21Z

~~~
[stdin]
rename "/C:/Users/svc-ignition-deploy/ign-c7a/r1-renamed.txt" "/C:/Users/svc-ignition-deploy/ign-c7a/r1-abs.txt"
@ls -1 "ign-c7a"
[stdout]
sftp> rename "/C:/Users/svc-ignition-deploy/ign-c7a/r1-renamed.txt" "/C:/Users/svc-ignition-deploy/ign-c7a/r1-abs.txt"
ign-c7a/d
ign-c7a/put-bs.txt
ign-c7a/put-rel.txt
ign-c7a/put-slashc.txt
ign-c7a/r1-abs.txt
[stderr]
[exit 0]
~~~

#### rename, relative source to absolute /C:/ target

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:22Z

~~~
[stdin]
rename "ign-c7a/r1-abs.txt" "/C:/Users/svc-ignition-deploy/ign-c7a/r1-mixed.txt"
@ls -1 "ign-c7a"
[stdout]
sftp> rename "ign-c7a/r1-abs.txt" "/C:/Users/svc-ignition-deploy/ign-c7a/r1-mixed.txt"
ign-c7a/d
ign-c7a/put-bs.txt
ign-c7a/put-rel.txt
ign-c7a/put-slashc.txt
ign-c7a/r1-mixed.txt
[stderr]
[exit 0]
~~~

#### rename into a subdirectory (same volume)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:24Z

~~~
[stdin]
rename "ign-c7a/r1-mixed.txt" "ign-c7a/d/r1-moved.txt"
@ls -1 "ign-c7a/d"
[stdout]
sftp> rename "ign-c7a/r1-mixed.txt" "ign-c7a/d/r1-moved.txt"
ign-c7a/d/r1-moved.txt
[stderr]
[exit 0]
~~~

#### rename with backslash paths

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:27Z

~~~
[stdin]
rename "ign-c7a\d\r1-moved.txt" "ign-c7a\d\r1-bs.txt"
@ls -1 "ign-c7a/d"
[stdout]
sftp> rename "ign-c7a\d\r1-moved.txt" "ign-c7a\d\r1-bs.txt"
ign-c7a/d/r1-bs.txt
[stderr]
[exit 0]
~~~

### C7a-2 — listing format (ls -1, ls -l, ls -la), paths with spaces and commas

#### create a directory with spaces and a comma, files with spaces, commas and non-ASCII

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:29Z

~~~
[stdin]
mkdir "ign-c7a/dir with space, comma"
put "$TMP/local/a.txt" "ign-c7a/dir with space, comma/f one.txt"
put "$TMP/local/b.txt" "ign-c7a/dir with space, comma/f,two.txt"
put "$TMP/local/u.txt" "ign-c7a/dir with space, comma/café.txt"
mkdir "ign-c7a/dir with space, comma/sub dir"
[stdout]
sftp> mkdir "ign-c7a/dir with space, comma"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/a.txt" "ign-c7a/dir with space, comma/f one.txt"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/b.txt" "ign-c7a/dir with space, comma/f,two.txt"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/u.txt" "ign-c7a/dir with space, comma/caf\303\251.txt"
sftp> mkdir "ign-c7a/dir with space, comma/sub dir"
[stderr]
[exit 0]
~~~

#### ls -1 of the spaces-and-comma directory

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:31Z

~~~
[stdin]
@ls -1 "ign-c7a/dir with space, comma"
[stdout]
ign-c7a/dir with space, comma/caf\303\251.txt
ign-c7a/dir with space, comma/f one.txt
ign-c7a/dir with space, comma/f,two.txt
ign-c7a/dir with space, comma/sub dir
[stderr]
[exit 0]
~~~

#### ls -l of the spaces-and-comma directory

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:33Z

~~~
[stdin]
@ls -l "ign-c7a/dir with space, comma"
[stdout]
-rw-******    1 -        -               8 Oct 15 02:35 caf\303\251.txt
-rw-******    1 -        -               6 Oct 15 02:35 f one.txt
-rw-******    1 -        -               6 Oct 15 02:35 f,two.txt
drwx******    1 -        -               0 Oct 15 02:35 sub dir
[stderr]
[exit 0]
~~~

#### ls -la of the spaces-and-comma directory

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:35Z

~~~
[stdin]
@ls -la "ign-c7a/dir with space, comma"
[stdout]
-rw-******    1 -        -               8 Oct 15 02:35 caf\303\251.txt
-rw-******    1 -        -               6 Oct 15 02:35 f one.txt
-rw-******    1 -        -               6 Oct 15 02:35 f,two.txt
drwx******    1 -        -               0 Oct 15 02:35 sub dir
[stderr]
[exit 0]
~~~

#### the same ls -l with the client in a UTF-8 locale (LC_ALL=en_US.UTF-8): does the sftp client print the non-ASCII name raw?

`env LC_ALL=en_US.UTF-8 sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:36Z

~~~
[stdin]
@ls -l "ign-c7a/dir with space, comma"
[stdout]
-rw-******    1 -        -               8 Oct 15 02:35 café.txt
-rw-******    1 -        -               6 Oct 15 02:35 f one.txt
-rw-******    1 -        -               6 Oct 15 02:35 f,two.txt
drwx******    1 -        -               0 Oct 15 02:35 sub dir
[stderr]
[exit 0]
~~~

#### get -R of the spaces-and-comma directory with the client in a UTF-8 locale (local names)

`env LC_ALL=en_US.UTF-8 sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:38Z

~~~
[stdin]
get -R "ign-c7a/dir with space, comma" "$TMP/local/spaceget"
[stdout]
sftp> get -R "ign-c7a/dir with space, comma" "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/spaceget"
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/dir with space, comma/
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/dir with space, comma/sub dir
[stderr]
[exit 0]
~~~

#### what that get -R left locally (names as the local filesystem holds them, sha256)

`bash -c cd\ \"\$1\"\ \&\&\ find\ .\ \|\ LC_ALL=C\ sort\ \&\&\ find\ .\ -type\ f\ -exec\ shasum\ -a\ 256\ \{\}\ \\\; _ $TMP/local/spaceget ` @ 2026-10-15T02:35:41Z

~~~
[stdout]
.
./dir with space, comma
./dir with space, comma/café.txt
./dir with space, comma/f one.txt
./dir with space, comma/f,two.txt
./dir with space, comma/sub dir
5da8f23decf397b13f4f55b6fb8a61936238bfe08ed9d901132974f1beccc45c  ./dir with space, comma/f,two.txt
b6a98d9ce9a2d9149288fa3df42d377c3e42737afdcdaf714e33c0a100b51060  ./dir with space, comma/f one.txt
ebc45fabefbabdd06424b3c476b11e93fec784069ff10844e7383d59f491f8cb  ./dir with space, comma/café.txt
[stderr]
[exit 0]
~~~

#### ls -1 with the absolute /C:/ form of the same directory

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:41Z

~~~
[stdin]
@ls -1 "/C:/Users/svc-ignition-deploy/ign-c7a/dir with space, comma"
[stdout]
/C:/Users/svc-ignition-deploy/ign-c7a/dir with space, comma/caf\303\251.txt
/C:/Users/svc-ignition-deploy/ign-c7a/dir with space, comma/f one.txt
/C:/Users/svc-ignition-deploy/ign-c7a/dir with space, comma/f,two.txt
/C:/Users/svc-ignition-deploy/ign-c7a/dir with space, comma/sub dir
[stderr]
[exit 0]
~~~

#### ls -l of a single file

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:43Z

~~~
[stdin]
@ls -l "ign-c7a/dir with space, comma/f one.txt"
[stdout]
-rw-------    ? 0        0               6 Oct 14 20:35 ign-c7a/dir with space, comma/f one.txt
[stderr]
[exit 0]
~~~

#### ls -1 of a single file

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:44Z

~~~
[stdin]
@ls -1 "ign-c7a/dir with space, comma/f one.txt"
[stdout]
ign-c7a/dir with space, comma/f one.txt
[stderr]
[exit 0]
~~~

#### ls -1 of an empty directory

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:46Z

~~~
[stdin]
mkdir "ign-c7a/empty"
@ls -1 "ign-c7a/empty"
[stdout]
sftp> mkdir "ign-c7a/empty"
[stderr]
[exit 0]
~~~

#### ls -la of an empty directory (. and .. entries?)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:48Z

~~~
[stdin]
@ls -la "ign-c7a/empty"
[stdout]
[stderr]
[exit 0]
~~~

#### the same directory from the cmd.exe side (dir /b, dir /a)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 dir\ /a\ \"%USERPROFILE%\\ign-c7a\\dir\ with\ space\,\ comma\" ` @ 2026-10-15T02:35:50Z

~~~
[stdout]
 Volume in drive C is Windows
 Volume Serial Number is 6AEF-F68E

 Directory of C:\Users\svc-ignition-deploy\ign-c7a\dir with space, comma

10/15/2026  02:35 AM    <DIR>          .
10/15/2026  02:35 AM    <DIR>          ..
10/15/2026  02:35 AM                 8 caf\x82.txt
10/15/2026  02:35 AM                 6 f one.txt
10/15/2026  02:35 AM                 6 f,two.txt
10/15/2026  02:35 AM    <DIR>          sub dir
               3 File(s)             20 bytes
               3 Dir(s)  116,343,496,704 bytes free
[13 CR bytes removed from stdout]
[stdout is not valid UTF-8: every byte outside printable ASCII is shown as \xNN]
[stderr]
[exit 0]
~~~

#### ls -l of the real collection root (owners, permissions, size, date columns)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:51Z

~~~
[stdin]
@ls -l "/C:/Program Files/Inductive Automation/Ignition/data/config/resources"
[stdout]
drwx******    1 -        -            4096 Oct 15 02:05 core
drwx******    1 -        -               0 Oct 15 02:07 external
drwx******    1 -        -               0 Oct 15 02:05 local
-rw-******    1 -        -           26893 Oct 15 02:04 migration-log-202610150204.md
drwx******    1 -        -               0 Oct 15 02:08 production
[stderr]
[exit 0]
~~~

#### ls -la of the real collection root

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:53Z

~~~
[stdin]
@ls -la "/C:/Program Files/Inductive Automation/Ignition/data/config/resources"
[stdout]
drwx******    1 -        -          262144 Oct 15 02:10 .resources
drwx******    1 -        -            4096 Oct 15 02:05 core
drwx******    1 -        -               0 Oct 15 02:07 external
drwx******    1 -        -               0 Oct 15 02:05 local
-rw-******    1 -        -           26893 Oct 15 02:04 migration-log-202610150204.md
drwx******    1 -        -               0 Oct 15 02:08 production
[stderr]
[exit 0]
~~~

### C7a-3 — error signalling: sftp -b - exit codes and stderr

#### not found: ls -1

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:56Z

~~~
[stdin]
@ls -1 "ign-c7a/does-not-exist"
[stdout]
[stderr]
Can't ls: "/C:/Users/svc-ignition-deploy/ign-c7a/does-not-exist" not found
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### not found: get

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:58Z

~~~
[stdin]
get "ign-c7a/does-not-exist" "$TMP/local/nf.txt"
[stdout]
sftp> get "ign-c7a/does-not-exist" "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/nf.txt"
[stderr]
File "/C:/Users/svc-ignition-deploy/ign-c7a/does-not-exist" not found.
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### not found: get -R

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:35:59Z

~~~
[stdin]
get -R "ign-c7a/does-not-exist" "$TMP/local"
[stdout]
sftp> get -R "ign-c7a/does-not-exist" "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local"
[stderr]
File "/C:/Users/svc-ignition-deploy/ign-c7a/does-not-exist" not found.
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### not found: rm

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:01Z

~~~
[stdin]
rm "ign-c7a/does-not-exist"
[stdout]
sftp> rm "ign-c7a/does-not-exist"
[stderr]
remote delete /C:/Users/svc-ignition-deploy/ign-c7a/does-not-exist: No such file or directory
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### not found: rmdir

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:02Z

~~~
[stdin]
rmdir "ign-c7a/does-not-exist"
[stdout]
sftp> rmdir "ign-c7a/does-not-exist"
[stderr]
remote rmdir "/C:/Users/svc-ignition-deploy/ign-c7a/does-not-exist": No such file or directory
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### not found: rename source

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:04Z

~~~
[stdin]
rename "ign-c7a/does-not-exist" "ign-c7a/xx"
[stdout]
sftp> rename "ign-c7a/does-not-exist" "ign-c7a/xx"
[stderr]
remote rename "/C:/Users/svc-ignition-deploy/ign-c7a/does-not-exist" to "/C:/Users/svc-ignition-deploy/ign-c7a/xx": No such file or directory
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### rename target parent missing

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:06Z

~~~
[stdin]
put "$TMP/local/c.txt" "ign-c7a/np.txt"
rename "ign-c7a/np.txt" "ign-c7a/no-such-dir/np.txt"
[stdout]
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/c.txt" "ign-c7a/np.txt"
sftp> rename "ign-c7a/np.txt" "ign-c7a/no-such-dir/np.txt"
[stderr]
remote rename "/C:/Users/svc-ignition-deploy/ign-c7a/np.txt" to "/C:/Users/svc-ignition-deploy/ign-c7a/no-such-dir/np.txt": No such file or directory
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### mkdir of an existing directory

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:07Z

~~~
[stdin]
mkdir "ign-c7a/d"
[stdout]
sftp> mkdir "ign-c7a/d"
[stderr]
remote mkdir "/C:/Users/svc-ignition-deploy/ign-c7a/d": Failure
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### rmdir of a non-empty directory

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:09Z

~~~
[stdin]
rmdir "ign-c7a/dir with space, comma"
[stdout]
sftp> rmdir "ign-c7a/dir with space, comma"
[stderr]
remote rmdir "/C:/Users/svc-ignition-deploy/ign-c7a/dir with space, comma": Failure
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### permission denied: put into the resources root

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:10Z

~~~
[stdin]
put "$TMP/local/a.txt" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/zz-c7a-denied.txt"
[stdout]
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/a.txt" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/zz-c7a-denied.txt"
[stderr]
dest open "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/zz-c7a-denied.txt": Permission denied
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### permission denied: mkdir in the resources root

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:12Z

~~~
[stdin]
mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/zz-c7a-denied"
[stdout]
sftp> mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/zz-c7a-denied"
[stderr]
remote mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/zz-c7a-denied": Permission denied
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### what the data dir holds (ls -1)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:13Z

~~~
[stdin]
@ls -1 "/C:/Program Files/Inductive Automation/Ignition/data"
[stdout]
/C:/Program Files/Inductive Automation/Ignition/data/certificates
/C:/Program Files/Inductive Automation/Ignition/data/commissioning.json
/C:/Program Files/Inductive Automation/Ignition/data/config
/C:/Program Files/Inductive Automation/Ignition/data/db
/C:/Program Files/Inductive Automation/Ignition/data/email-profiles
/C:/Program Files/Inductive Automation/Ignition/data/gateway.xml
/C:/Program Files/Inductive Automation/Ignition/data/gateway.xml_clean
/C:/Program Files/Inductive Automation/Ignition/data/ignition.conf
/C:/Program Files/Inductive Automation/Ignition/data/init.properties.bak
/C:/Program Files/Inductive Automation/Ignition/data/jar-cache
/C:/Program Files/Inductive Automation/Ignition/data/log4j.properties
/C:/Program Files/Inductive Automation/Ignition/data/logback.xml
/C:/Program Files/Inductive Automation/Ignition/data/metricsdb
/C:/Program Files/Inductive Automation/Ignition/data/modules.json
/C:/Program Files/Inductive Automation/Ignition/data/projects
/C:/Program Files/Inductive Automation/Ignition/data/redundancy.xml
/C:/Program Files/Inductive Automation/Ignition/data/request
/C:/Program Files/Inductive Automation/Ignition/data/response
/C:/Program Files/Inductive Automation/Ignition/data/var
[stderr]
[exit 0]
~~~

#### user profile directories on the host (dir /b C:\Users)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 dir\ /b\ C:\\Users ` @ 2026-10-15T02:36:15Z

~~~
[stdout]
Public
svc-ignition-deploy
[2 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### permission denied: ls of another account's profile directory (none found)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:17Z

~~~
[stdin]
@ls -1 "/C:/Users/none"
[stdout]
[stderr]
Can't ls: "/C:/Users/none" not found
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### permission denied: ls of C:/Windows/System32/config

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:18Z

~~~
[stdin]
@ls -1 "/C:/Windows/System32/config"
[stdout]
[stderr]
remote readdir("/C:/Windows/System32/config/"): Bad message
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### permission denied: ls -l of the sshd host private key file (metadata only; the file is never fetched)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:21Z

~~~
[stdin]
@ls -l "/C:/ProgramData/ssh/ssh_host_ed25519_key"
[stdout]
-rw-------    ? 0        0             411 Oct 14 19:54 /C:/ProgramData/ssh/ssh_host_ed25519_key
[stderr]
[exit 0]
~~~

#### permission denied: get of the sshd log (a file that exists and is not readable by Users)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:23Z

~~~
[stdin]
get "/C:/ProgramData/ssh/logs/sshd.log" "$TMP/local/sshd.log"
[stdout]
sftp> get "/C:/ProgramData/ssh/logs/sshd.log" "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/sshd.log"
[stderr]
File "/C:/ProgramData/ssh/logs/sshd.log" not found.
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### permission denied: get of the SAM hive

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:25Z

~~~
[stdin]
get "/C:/Windows/System32/config/SAM" "$TMP/local/sam"
[stdout]
sftp> get "/C:/Windows/System32/config/SAM" "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/sam"
[stderr]
File "/C:/Windows/System32/config/SAM" not found.
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### after the denied writes: nothing was created in the resources root

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 dir\ /b\ \"C:\\Program\ Files\\Inductive\ Automation\\Ignition\\data\\config\\resources\"\ \|\ findstr\ /i\ zz-c7a ` @ 2026-10-15T02:36:27Z

~~~
[stdout]
[stderr]
[exit 1]
~~~

#### sftp exit code with a failing command in the MIDDLE of a batch (does the batch abort?)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:28Z

~~~
[stdin]
@ls -1 "ign-c7a/d"
@ls -1 "ign-c7a/does-not-exist"
@ls -1 "ign-c7a/empty"
[stdout]
ign-c7a/d/r1-bs.txt
[stderr]
Can't ls: "/C:/Users/svc-ignition-deploy/ign-c7a/does-not-exist" not found
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### sftp batch whose commands all succeed (exit code 0)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:31Z

~~~
[stdin]
@ls -1 "ign-c7a/d"
@ls -1 "ign-c7a/empty"
[stdout]
ign-c7a/d/r1-bs.txt
[stderr]
[exit 0]
~~~

### C7a-4 — rename over an existing target

#### file over an existing file: setup

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:34Z

~~~
[stdin]
put "$TMP/local/a.txt" "ign-c7a/ow-src.txt"
put "$TMP/local/b.txt" "ign-c7a/ow-dst.txt"
@ls -l "ign-c7a/ow-src.txt"
@ls -l "ign-c7a/ow-dst.txt"
[stdout]
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/a.txt" "ign-c7a/ow-src.txt"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/b.txt" "ign-c7a/ow-dst.txt"
-rw-------    ? 0        0               6 Oct 14 20:36 ign-c7a/ow-src.txt
-rw-------    ? 0        0               6 Oct 14 20:36 ign-c7a/ow-dst.txt
[stderr]
[exit 0]
~~~

#### file over an existing file: rename ow-src.txt onto ow-dst.txt

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:37Z

~~~
[stdin]
rename "ign-c7a/ow-src.txt" "ign-c7a/ow-dst.txt"
[stdout]
sftp> rename "ign-c7a/ow-src.txt" "ign-c7a/ow-dst.txt"
[stderr]
[exit 0]
~~~

#### file over an existing file: what is left, and the content of ow-dst.txt (alpha means replaced, bravo means untouched)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 dir\ /b\ \"%USERPROFILE%\\ign-c7a\\ow-\*\"\ \&\ type\ \"%USERPROFILE%\\ign-c7a\\ow-dst.txt\" ` @ 2026-10-15T02:36:39Z

~~~
[stdout]
ow-dst.txt
alpha
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### directory over an existing EMPTY directory

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:41Z

~~~
[stdin]
mkdir "ign-c7a/od-src"
put "$TMP/local/a.txt" "ign-c7a/od-src/in-src.txt"
mkdir "ign-c7a/od-dst-empty"
rename "ign-c7a/od-src" "ign-c7a/od-dst-empty"
[stdout]
sftp> mkdir "ign-c7a/od-src"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/a.txt" "ign-c7a/od-src/in-src.txt"
sftp> mkdir "ign-c7a/od-dst-empty"
sftp> rename "ign-c7a/od-src" "ign-c7a/od-dst-empty"
[stderr]
[exit 0]
~~~

#### directory over an empty directory: the whole tree (is in-src.txt inside od-dst-empty?)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 cmd\ /c\ \"dir\ /b\ /s\ \"%USERPROFILE%\\ign-c7a\"\ \|\ findstr\ /i\ /c:\"\\od-\"\" ` @ 2026-10-15T02:36:43Z

~~~
[stdout]
C:\Users\svc-ignition-deploy\ign-c7a\od-dst-empty
C:\Users\svc-ignition-deploy\ign-c7a\od-dst-empty\in-src.txt
[2 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### directory over an existing NON-EMPTY directory: setup

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:44Z

~~~
[stdin]
mkdir "ign-c7a/od2-src"
put "$TMP/local/a.txt" "ign-c7a/od2-src/in-src.txt"
mkdir "ign-c7a/od2-dst"
put "$TMP/local/b.txt" "ign-c7a/od2-dst/in-dst.txt"
[stdout]
sftp> mkdir "ign-c7a/od2-src"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/a.txt" "ign-c7a/od2-src/in-src.txt"
sftp> mkdir "ign-c7a/od2-dst"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/b.txt" "ign-c7a/od2-dst/in-dst.txt"
[stderr]
[exit 0]
~~~

#### directory over an existing NON-EMPTY directory: rename

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:46Z

~~~
[stdin]
rename "ign-c7a/od2-src" "ign-c7a/od2-dst"
[stdout]
sftp> rename "ign-c7a/od2-src" "ign-c7a/od2-dst"
[stderr]
remote rename "/C:/Users/svc-ignition-deploy/ign-c7a/od2-src" to "/C:/Users/svc-ignition-deploy/ign-c7a/od2-dst": Failure
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### directory over a non-empty directory: the whole tree (both directories intact?)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 cmd\ /c\ \"dir\ /b\ /s\ \"%USERPROFILE%\\ign-c7a\"\ \|\ findstr\ /i\ /c:\"\\od2-\"\" ` @ 2026-10-15T02:36:48Z

~~~
[stdout]
C:\Users\svc-ignition-deploy\ign-c7a\od2-dst
C:\Users\svc-ignition-deploy\ign-c7a\od2-src
C:\Users\svc-ignition-deploy\ign-c7a\od2-dst\in-dst.txt
C:\Users\svc-ignition-deploy\ign-c7a\od2-src\in-src.txt
[4 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### file over an existing DIRECTORY

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:50Z

~~~
[stdin]
put "$TMP/local/a.txt" "ign-c7a/fd-src.txt"
mkdir "ign-c7a/fd-dst"
rename "ign-c7a/fd-src.txt" "ign-c7a/fd-dst"
[stdout]
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/a.txt" "ign-c7a/fd-src.txt"
sftp> mkdir "ign-c7a/fd-dst"
sftp> rename "ign-c7a/fd-src.txt" "ign-c7a/fd-dst"
[stderr]
remote rename "/C:/Users/svc-ignition-deploy/ign-c7a/fd-src.txt" to "/C:/Users/svc-ignition-deploy/ign-c7a/fd-dst": Failure
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### rename onto itself, step 1: a fresh file and its listing

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:52Z

~~~
[stdin]
put "$TMP/local/a.txt" "ign-c7a/self.txt"
@ls -l "ign-c7a/self.txt"
[stdout]
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/a.txt" "ign-c7a/self.txt"
-rw-------    ? 0        0               6 Oct 14  2026 ign-c7a/self.txt
[stderr]
[exit 0]
~~~

#### rename onto itself, step 2: rename self.txt onto self.txt

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:54Z

~~~
[stdin]
rename "ign-c7a/self.txt" "ign-c7a/self.txt"
[stdout]
sftp> rename "ign-c7a/self.txt" "ign-c7a/self.txt"
[stderr]
remote rename "/C:/Users/svc-ignition-deploy/ign-c7a/self.txt" to "/C:/Users/svc-ignition-deploy/ign-c7a/self.txt": No such file or directory
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### rename onto itself, step 3: is the file still there?

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:55Z

~~~
[stdin]
@ls -l "ign-c7a/self.txt"
[stdout]
[stderr]
Can't ls: "/C:/Users/svc-ignition-deploy/ign-c7a/self.txt" not found
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### rename onto itself, step 4: the file from the cmd.exe side

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 dir\ /b\ \"%USERPROFILE%\\ign-c7a\\self.txt\"\ \&\ type\ \"%USERPROFILE%\\ign-c7a\\self.txt\" ` @ 2026-10-15T02:36:57Z

~~~
[stdout]
[stderr]
File Not Found
The system cannot find the file specified.
[2 CR bytes removed from stderr]
[exit 1]
~~~

#### rename over an existing file twice in a row (the commit step may repeat): first

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:36:58Z

~~~
[stdin]
put "$TMP/local/a.txt" "ign-c7a/tw-src1.txt"
put "$TMP/local/b.txt" "ign-c7a/tw-dst.txt"
rename "ign-c7a/tw-src1.txt" "ign-c7a/tw-dst.txt"
[stdout]
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/a.txt" "ign-c7a/tw-src1.txt"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/b.txt" "ign-c7a/tw-dst.txt"
sftp> rename "ign-c7a/tw-src1.txt" "ign-c7a/tw-dst.txt"
[stderr]
[exit 0]
~~~

#### rename over an existing file twice in a row: second

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:37:01Z

~~~
[stdin]
put "$TMP/local/c.txt" "ign-c7a/tw-src2.txt"
rename "ign-c7a/tw-src2.txt" "ign-c7a/tw-dst.txt"
@ls -1 "ign-c7a"
[stdout]
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/c.txt" "ign-c7a/tw-src2.txt"
sftp> rename "ign-c7a/tw-src2.txt" "ign-c7a/tw-dst.txt"
ign-c7a/d
ign-c7a/dir with space, comma
ign-c7a/empty
ign-c7a/fd-dst
ign-c7a/fd-src.txt
ign-c7a/np.txt
ign-c7a/od-dst-empty
ign-c7a/od2-dst
ign-c7a/od2-src
ign-c7a/ow-dst.txt
ign-c7a/put-bs.txt
ign-c7a/put-rel.txt
ign-c7a/put-slashc.txt
ign-c7a/tw-dst.txt
[stderr]
[exit 0]
~~~

#### rename over an existing file twice in a row: content of tw-dst.txt (charlie means the second replaced the first)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 type\ \"%USERPROFILE%\\ign-c7a\\tw-dst.txt\" ` @ 2026-10-15T02:37:03Z

~~~
[stdout]
charlie
[stderr]
[exit 0]
~~~

#### rename over a file that another session holds open: setup

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:37:04Z

~~~
[stdin]
put "$TMP/local/b.txt" "ign-c7a/lock-dst.txt"
[stdout]
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/b.txt" "ign-c7a/lock-dst.txt"
[stderr]
[exit 0]
~~~

#### rename over a file that a second ssh session holds open with FileShare.None (4 s hold; the sftp rename starts 2 s after it)

`bash -c $'\n    HOLD=$(printf "%s" "\\$f=[IO.File]::Open((Join-Path \\$env:USERPROFILE \\"ign-c7a\\\\lock-dst.txt\\"),\\"Open\\",\\"Read\\",\\"None\\"); \\"held\\"; Start-Sleep 4; \\$f.C...[890 chars]' _ 22 <key> $HOME/.cache/ign-wflwvm01/known_hosts svc-ignition-deploy@40.114.71.134 $TMP/local/c.txt ` @ 2026-10-15T02:37:06Z

~~~
[stdout]
held
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/c.txt" "ign-c7a/lock-src.txt"
sftp> rename "ign-c7a/lock-src.txt" "ign-c7a/lock-dst.txt"
sftp-exit=1
released
[2 CR bytes removed from stdout]
[stderr]
remote rename "/C:/Users/svc-ignition-deploy/ign-c7a/lock-src.txt" to "/C:/Users/svc-ignition-deploy/ign-c7a/lock-dst.txt": Failure
[1 CR bytes removed from stderr]
[exit 0]
~~~

#### after the held-handle rename attempt: which names exist, and the content of lock-dst.txt

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 dir\ /b\ \"%USERPROFILE%\\ign-c7a\\lock-\*\"\ \&\ type\ \"%USERPROFILE%\\ign-c7a\\lock-dst.txt\" ` @ 2026-10-15T02:37:12Z

~~~
[stdout]
lock-dst.txt
lock-src.txt
bravo
[2 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

### C7a-5 — junctions and links

#### create jx/real, a file in it, and a junction jx/junc to it (cmd /c mklink /J)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 cmd\ /c\ \"mkdir\ \"%USERPROFILE%\\ign-c7a\\jx\\real\"\ \&\&\ echo\ inreal\>\ \"%USERPROFILE%\\ign-c7a\\jx\\real\\r.txt\"\ \&\&\ mklink\ /J\ \"%USERPROFILE%\\ign-c7a\\jx\\junc\"\ \"%USERPROFILE%\\ign-c7a\\jx\\real\"\" ` @ 2026-10-15T02:37:13Z

~~~
[stdout]
Junction created for C:\Users\svc-ignition-deploy\ign-c7a\jx\junc <<===>> C:\Users\svc-ignition-deploy\ign-c7a\jx\real
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### dir of jx (junction marker)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 dir\ /a\ \"%USERPROFILE%\\ign-c7a\\jx\" ` @ 2026-10-15T02:37:15Z

~~~
[stdout]
 Volume in drive C is Windows
 Volume Serial Number is 6AEF-F68E

 Directory of C:\Users\svc-ignition-deploy\ign-c7a\jx

10/15/2026  02:37 AM    <DIR>          .
10/15/2026  02:37 AM    <DIR>          ..
10/15/2026  02:37 AM    <JUNCTION>     junc [C:\Users\svc-ignition-deploy\ign-c7a\jx\real]
10/15/2026  02:37 AM    <DIR>          real
               0 File(s)              0 bytes
               4 Dir(s)  116,335,869,952 bytes free
[11 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### ls -la of jx (how the junction appears to sftp)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:37:16Z

~~~
[stdin]
@ls -la "ign-c7a/jx"
[stdout]
drwx******    1 -        -               0 Oct 15 02:37 junc
drwx******    1 -        -               0 Oct 15 02:37 real
[stderr]
[exit 0]
~~~

#### ls -l of jx/junc contents

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:37:18Z

~~~
[stdin]
@ls -l "ign-c7a/jx/junc"
[stdout]
-rw-******    1 -        -               9 Oct 15 02:37 r.txt
[stderr]
[exit 0]
~~~

#### get of a file reached through the junction

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:37:20Z

~~~
[stdin]
get "ign-c7a/jx/junc/r.txt" "$TMP/local/viajunc.txt"
[stdout]
sftp> get "ign-c7a/jx/junc/r.txt" "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/viajunc.txt"
[stderr]
[exit 0]
~~~

#### get -R of jx (contains the junction)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:37:22Z

~~~
[stdin]
get -R "ign-c7a/jx" "$TMP/local/jget"
[stdout]
sftp> get -R "ign-c7a/jx" "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/jget"
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/real
[stderr]
[exit 0]
~~~

#### what get -R over a junction left locally

`bash -c cd\ \"\$1\"\ \&\&\ find\ .\ \|\ LC_ALL=C\ sort\ \&\&\ find\ .\ -type\ f\ -exec\ shasum\ -a\ 256\ \{\}\ \\\; _ $TMP/local/jget ` @ 2026-10-15T02:37:25Z

~~~
[stdout]
.
./jx
./jx/junc
./jx/junc/r.txt
./jx/real
./jx/real/r.txt
34f451e9277d4b96776445a4e3353233f37ae2f44af56297dfe26c0ea47eac63  ./jx/real/r.txt
34f451e9277d4b96776445a4e3353233f37ae2f44af56297dfe26c0ea47eac63  ./jx/junc/r.txt
[stderr]
[exit 0]
~~~

#### a junction that points at its own parent (jx/loop -> jx)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 cmd\ /c\ mklink\ /J\ \"%USERPROFILE%\\ign-c7a\\jx\\loop\"\ \"%USERPROFILE%\\ign-c7a\\jx\" ` @ 2026-10-15T02:37:25Z

~~~
[stdout]
Junction created for C:\Users\svc-ignition-deploy\ign-c7a\jx\loop <<===>> C:\Users\svc-ignition-deploy\ign-c7a\jx
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### get -R of jx WITH the self-referencing junction (150 s alarm bounds a runaway)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:37:27Z

~~~
[stdin]
get -R "ign-c7a/jx" "$TMP/local/jloop"
[stdout]
sftp> get -R "ign-c7a/jx" "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/jloop"
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/real
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/jx/real
[stderr]
remote readdir("/C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loo: No such file or directory
remote readdir "/C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/junc" failed
remote readdir("/C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loo: No such file or directory
remote readdir "/C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop" failed
remote readdir("/C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loo: No such file or directory
remote readdir "/C:/Users/svc-ignition-deploy/ign-c7a/jx/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/loop/real" failed
[6 CR bytes removed from stderr]
[exit 1]
~~~

#### what the looping get -R left locally: entry count and deepest path length (not the listing)

`bash -c cd\ \"\$1\"\ \&\&\ echo\ \"entries=\$\(find\ .\ \|\ wc\ -l\ \|\ tr\ -d\ \"\ \"\)\"\ \&\&\ echo\ \"deepest_path_chars=\$\(find\ .\ \|\ awk\ \"\{\ if\ \(length\(\\\$0\)\ \>\ m\)\ m\ =\ length\(\\\$0\)\ \}\ END\ \{\ print\ m\ \}\"\)\"\ \&\&\ find\ .\ -type\ f\ \|\ head\ -n\ 5 _ $TMP/local/jloop ` @ 2026-10-15T02:38:41Z

~~~
[stdout]
entries=218
deepest_path_chars=224
./jx/real/r.txt
./jx/junc/r.txt
./jx/loop/real/r.txt
./jx/loop/junc/r.txt
./jx/loop/loop/real/r.txt
[stderr]
[exit 0]
~~~

#### a directory symbolic link (mklink /D), which needs a privilege the account may not hold

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 cmd\ /c\ mklink\ /D\ \"%USERPROFILE%\\ign-c7a\\jx\\symd\"\ \"%USERPROFILE%\\ign-c7a\\jx\\real\" ` @ 2026-10-15T02:38:41Z

~~~
[stdout]
[stderr]
You do not have sufficient privilege to perform this operation.
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### ls -la of jx after the symlink attempt

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:38:43Z

~~~
[stdin]
@ls -la "ign-c7a/jx"
[stdout]
drwx******    1 -        -               0 Oct 15 02:37 junc
drwx******    1 -        -               0 Oct 15 02:37 loop
drwx******    1 -        -               0 Oct 15 02:37 real
[stderr]
[exit 0]
~~~

#### remove the junctions (rmdir removes the link only)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 cmd\ /c\ \"rmdir\ \"%USERPROFILE%\\ign-c7a\\jx\\loop\"\ \&\ rmdir\ \"%USERPROFILE%\\ign-c7a\\jx\\junc\"\ \&\ rmdir\ \"%USERPROFILE%\\ign-c7a\\jx\\symd\"\ \&\ dir\ /a\ \"%USERPROFILE%\\ign-c7a\\jx\"\ \&\ dir\ /b\ \"%USERPROFILE%\\ign-c7a\\jx\\real\"\" ` @ 2026-10-15T02:38:45Z

~~~
[stdout]
 Volume in drive C is Windows
 Volume Serial Number is 6AEF-F68E

 Directory of C:\Users\svc-ignition-deploy\ign-c7a\jx

10/15/2026  02:38 AM    <DIR>          .
10/15/2026  02:37 AM    <DIR>          ..
10/15/2026  02:37 AM    <DIR>          real
               0 File(s)              0 bytes
               3 Dir(s)  116,336,177,152 bytes free
r.txt
[11 CR bytes removed from stdout]
[stderr]
The system cannot find the file specified.
[1 CR bytes removed from stderr]
[exit 0]
~~~

### C7a-6 — default shell and PowerShell -EncodedCommand: exit codes and CLIXML

#### ok output, bare

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JwBvAGsAJwA= ` @ 2026-10-15T02:38:46Z

~~~
[script]
'ok'
[stdout]
ok
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### ok output, with the prelude

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIAAkAEUAcgByAG8AcgBBAGMAdABpAG8AbgBQAHIAZQBmAGUAcgBlAG4AYwBlAD0AJwBTAHQAbwBwACcAOwAgACcAbwBrACcA ` @ 2026-10-15T02:38:48Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; 'ok'
[stdout]
ok
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### exit 7, bare

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ ZQB4AGkAdAAgADcA ` @ 2026-10-15T02:38:49Z

~~~
[script]
exit 7
[stdout]
[stderr]
[exit 7]
~~~

#### exit 7, with the prelude

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIAAkAEUAcgByAG8AcgBBAGMAdABpAG8AbgBQAHIAZQBmAGUAcgBlAG4AYwBlAD0AJwBTAHQAbwBwACcAOwAgAGUAeABpAHQAIAA3AA== ` @ 2026-10-15T02:38:51Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; exit 7
[stdout]
[stderr]
[exit 7]
~~~

#### throw, bare

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ dABoAHIAbwB3ACAAJwBiAG8AbwBtACcA ` @ 2026-10-15T02:38:52Z

~~~
[script]
throw 'boom'
[stdout]
[stderr]
#< CLIXML
<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04"><S S="Error">boom_x000D__x000A_</S><S S="Error">At line:1 char:1_x000D__x000A_</S><S S="Error">+ throw 'boom'_x000D__x000A_</S><S S="Error">+ ~~~~~~~~~~~~_x000D__x000A_</S><S S="Error">    + CategoryInfo          : OperationStopped: (boom:String) [], RuntimeException_x000D__x000A_</S><S S="Error">    + FullyQualifiedErrorId : boom_x000D__x000A_</S><S S="Error"> _x000D__x000A_</S></Objs>
[stderr did not end with a newline]
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### throw, with the prelude

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIAAkAEUAcgByAG8AcgBBAGMAdABpAG8AbgBQAHIAZQBmAGUAcgBlAG4AYwBlAD0AJwBTAHQAbwBwACcAOwAgAHQAaAByAG8AdwAgACcAYgBvAG8AbQAnAA== ` @ 2026-10-15T02:38:54Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; throw 'boom'
[stdout]
[stderr]
#< CLIXML
<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04"><S S="Error">boom_x000D__x000A_</S><S S="Error">At line:1 char:72_x000D__x000A_</S><S S="Error">+ ... rence='SilentlyContinue'; $ErrorActionPreference='Stop'; throw 'boom'_x000D__x000A_</S><S S="Error">+                                                              ~~~~~~~~~~~~_x000D__x000A_</S><S S="Error">    + CategoryInfo          : OperationStopped: (boom:String) [], RuntimeException_x000D__x000A_</S><S S="Error">    + FullyQualifiedErrorId : boom_x000D__x000A_</S><S S="Error"> _x000D__x000A_</S></Objs>
[stderr did not end with a newline]
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### a cmdlet error (Get-Item of a missing path), bare

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ RwBlAHQALQBJAHQAZQBtACAAJwBDADoAXABkAGUAZgBpAG4AaQB0AGUAbAB5AC0AbQBpAHMAcwBpAG4AZwAnAA== ` @ 2026-10-15T02:38:56Z

~~~
[script]
Get-Item 'C:\definitely-missing'
[stdout]
[stderr]
#< CLIXML
<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04"><Obj S="progress" RefId="0"><TN RefId="0"><T>System.Management.Automation.PSCustomObject</T><T>System.Object</T></TN><MS><I64 N="SourceId">1</I64><PR N="Record"><AV>Preparing modules for first use.</AV><AI>0</AI><Nil /><PI>-1</PI><PC>-1</PC><T>Completed</T><SR>-1</SR><SD> </SD></PR></MS></Obj><S S="Error">Get-Item : Cannot find path 'C:\definitely-missing' because it does not exist._x000D__x000A_</S><S S="Error">At line:1 char:1_x000D__x000A_</S><S S="Error">+ Get-Item 'C:\definitely-missing'_x000D__x000A_</S><S S="Error">+ ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~_x000D__x000A_</S><S S="Error">    + CategoryInfo          : ObjectNotFound: (C:\definitely-missing:String) [Get-Item], ItemNotFoundException_x000D__x000A_</S><S S="Error">    + FullyQualifiedErrorId : PathNotFound,Microsoft.PowerShell.Commands.GetItemCommand_x000D__x000A_</S><S S="Error"> _x000D__x000A_</S></Objs>
[stderr did not end with a newline]
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### a cmdlet error (Get-Item of a missing path), with the prelude

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIAAkAEUAcgByAG8AcgBBAGMAdABpAG8AbgBQAHIAZQBmAGUAcgBlAG4AYwBlAD0AJwBTAHQAbwBwACcAOwAgAEcAZQB0AC0ASQB0AGUAbQAgACcAQwA6AFwAZABlAGYAaQBuAGkAdABlAGwAeQAtAG0AaQBzAHMAaQBuAGcAJwA= ` @ 2026-10-15T02:38:57Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; Get-Item 'C:\definitely-missing'
[stdout]
[stderr]
#< CLIXML
<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04"><S S="Error">Get-Item : Cannot find path 'C:\definitely-missing' because it does not exist._x000D__x000A_</S><S S="Error">At line:1 char:72_x000D__x000A_</S><S S="Error">+ ... nue'; $ErrorActionPreference='Stop'; Get-Item 'C:\definitely-missing'_x000D__x000A_</S><S S="Error">+                                          ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~_x000D__x000A_</S><S S="Error">    + CategoryInfo          : ObjectNotFound: (C:\definitely-missing:String) [Get-Item], ItemNotFoundException_x000D__x000A_</S><S S="Error">    + FullyQualifiedErrorId : PathNotFound,Microsoft.PowerShell.Commands.GetItemCommand_x000D__x000A_</S><S S="Error"> _x000D__x000A_</S></Objs>
[stderr did not end with a newline]
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### Write-Error then continue, bare

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ VwByAGkAdABlAC0ARQByAHIAbwByACAAJwBlADEAJwA7ACAAJwBhAGYAdABlAHIAJwA= ` @ 2026-10-15T02:38:59Z

~~~
[script]
Write-Error 'e1'; 'after'
[stdout]
after
[1 CR bytes removed from stdout]
[stderr]
#< CLIXML
<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04"><Obj S="progress" RefId="0"><TN RefId="0"><T>System.Management.Automation.PSCustomObject</T><T>System.Object</T></TN><MS><I64 N="SourceId">1</I64><PR N="Record"><AV>Preparing modules for first use.</AV><AI>0</AI><Nil /><PI>-1</PI><PC>-1</PC><T>Completed</T><SR>-1</SR><SD> </SD></PR></MS></Obj><S S="Error">Write-Error 'e1'; 'after' : e1_x000D__x000A_</S><S S="Error">    + CategoryInfo          : NotSpecified: (:) [Write-Error], WriteErrorException_x000D__x000A_</S><S S="Error">    + FullyQualifiedErrorId : Microsoft.PowerShell.Commands.WriteErrorException_x000D__x000A_</S><S S="Error"> _x000D__x000A_</S></Objs>
[stderr did not end with a newline]
[1 CR bytes removed from stderr]
[exit 0]
~~~

#### Write-Error then continue, with the prelude

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIAAkAEUAcgByAG8AcgBBAGMAdABpAG8AbgBQAHIAZQBmAGUAcgBlAG4AYwBlAD0AJwBTAHQAbwBwACcAOwAgAFcAcgBpAHQAZQAtAEUAcgByAG8AcgAgACcAZQAxACcAOwAgACcAYQBmAHQAZQByACcA ` @ 2026-10-15T02:39:01Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; Write-Error 'e1'; 'after'
[stdout]
[stderr]
#< CLIXML
<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04"><S S="Error">$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; Write-Error 'e1'; 'after' : e1_x000D__x000A_</S><S S="Error">At line:1 char:72_x000D__x000A_</S><S S="Error">+ ... tlyContinue'; $ErrorActionPreference='Stop'; Write-Error 'e1'; 'after ..._x000D__x000A_</S><S S="Error">+                                                  ~~~~~~~~~~~~~~~~_x000D__x000A_</S><S S="Error">    + CategoryInfo          : NotSpecified: (:) [Write-Error], WriteErrorException_x000D__x000A_</S><S S="Error">    + FullyQualifiedErrorId : Microsoft.PowerShell.Commands.WriteErrorException_x000D__x000A_</S><S S="Error"> _x000D__x000A_</S></Objs>
[stderr did not end with a newline]
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### Write-Progress, bare

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ VwByAGkAdABlAC0AUAByAG8AZwByAGUAcwBzACAALQBBAGMAdABpAHYAaQB0AHkAIABhACAALQBTAHQAYQB0AHUAcwAgAGIAOwAgACcAZABvAG4AZQAnAA== ` @ 2026-10-15T02:39:03Z

~~~
[script]
Write-Progress -Activity a -Status b; 'done'
[stdout]
done
[1 CR bytes removed from stdout]
[stderr]
#< CLIXML
<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04"><Obj S="progress" RefId="0"><TN RefId="0"><T>System.Management.Automation.PSCustomObject</T><T>System.Object</T></TN><MS><I64 N="SourceId">1</I64><PR N="Record"><AV>Preparing modules for first use.</AV><AI>0</AI><Nil /><PI>-1</PI><PC>-1</PC><T>Completed</T><SR>-1</SR><SD> </SD></PR></MS></Obj><Obj S="progress" RefId="1"><TNRef RefId="0" /><MS><I64 N="SourceId">0</I64><PR N="Record"><AV>a</AV><AI>0</AI><Nil /><PI>-1</PI><PC>-1</PC><T>Processing</T><SR>-1</SR><SD>b</SD></PR></MS></Obj></Objs>
[stderr did not end with a newline]
[1 CR bytes removed from stderr]
[exit 0]
~~~

#### Write-Progress, with the prelude

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIA...\[362\ chars\] ` @ 2026-10-15T02:39:05Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; Write-Progress -Activity a -Status b; 'done'
[stdout]
done
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### real progress: Compress-Archive of a small file then Expand-Archive, bare

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABkAD0ASgBvAGkAbgAtAFAAYQB0AGgAIAAkAGUAbgB2ADoAVQBTAEUAUgBQAFIATwBGAEkATABFACAAJwBpAGcAbgAtAGMANwBhACcAOw...\[958\ chars\] ` @ 2026-10-15T02:39:06Z

~~~
[script]
$d=Join-Path $env:USERPROFILE 'ign-c7a'; Compress-Archive -LiteralPath (Join-Path $d 'put-rel.txt') -DestinationPath (Join-Path $d 'z.zip') -Force; Expand-Archive -LiteralPath (Join-Path $d 'z.zip') -DestinationPath (Join-Path $d 'zx') -Force; Remove-Item -LiteralPath (Join-Path $d 'z.zip'),(Join-Path $d 'zx') -Recurse -Force; 'archived'
[stdout]
archived
[1 CR bytes removed from stdout]
[stderr]
#< CLIXML
<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04"><Obj S="progress" RefId="0"><TN RefId="0"><T>System.Management.Automation.PSCustomObject</T><T>System.Object</T></TN><MS><I64 N="SourceId">1</I64><PR N="Record"><AV>Preparing modules for first use.</AV><AI>0</AI><Nil /><PI>-1</PI><PC>-1</PC><T>Completed</T><SR>-1</SR><SD> </SD></PR></MS></Obj><Obj S="progress" RefId="1"><TNRef RefId="0" /><MS><I64 N="SourceId">2</I64><PR N="Record"><AV>Preparing modules for first use.</AV><AI>0</AI><Nil /><PI>-1</PI><PC>-1</PC><T>Completed</T><SR>-1</SR><SD> </SD></PR></MS></Obj><Obj S="progress" RefId="2"><TNRef RefId="0" /><MS><I64 N="SourceId">1</I64><PR N="Record"><AV>Preparing modules for first use.</AV><AI>0</AI><Nil /><PI>-1</PI><PC>-1</PC><T>Completed</T><SR>-1</SR><SD> </SD></PR></MS></Obj><Obj S="progress" RefId="3"><TNRef RefId="0" /><MS><I64 N="SourceId">0</I64><PR N="Record"><AV>Compress-Archive</AV><AI>0</AI><Nil /><PI>-1</PI><PC>1</PC><T>Processing</T><SR>-1</SR><SD>The archive file 'C:\Users\svc-ignition-deploy\ign-c7a\z.zip' creation is in progress...</SD></PR></MS></Obj><Obj S="progress" RefId="4"><TNRef RefId="0" /><MS><I64 N="SourceId">0</I64><PR N="Record"><AV>Compress-Archive</AV><AI>0</AI><Nil /><PI>-1</PI><PC>100</PC><T>Processing</T><SR>-1</SR><SD>The archive file 'C:\Users\svc-ignition-deploy\ign-c7a\z.zip' creation is in progress...</SD></PR></MS></Obj><Obj S="progress" RefId="5"><TNRef RefId="0" /><MS><I64 N="SourceId">0</I64><PR N="Record"><AV>Compress-Archive</AV><AI>0</AI><Nil /><PI>-1</PI><PC>-1</PC><T>Completed</T><SR>-1</SR><SD>Processing</SD></PR></MS></Obj><Obj S="progress" RefId="6"><TNRef RefId="0" /><MS><I64 N="SourceId">0</I64><PR N="Record"><AV>Expand-Archive</AV><AI>0</AI><Nil /><PI>-1</PI><PC>1</PC><T>Processing</T><SR>-1</SR><SD>The archive file 'C:\Users\svc-ignition-deploy\ign-c7a\zx' expansion is in progress...</SD></PR></MS></Obj><Obj S="progress" RefId="7"><TNRef RefId="0" /><MS><I64 N="SourceId">0</I64><PR N="Record"><AV>Expand-Archive</AV><AI>0</AI><Nil /><PI>-1</PI><PC>100</PC><T>Processing</T><SR>-1</SR><SD>The archive file 'C:\Users\svc-ignition-deploy\ign-c7a\z.zip' expansion is in progress...</SD></PR></MS></Obj><Obj S="progress" RefId="8"><TNRef RefId="0" /><MS><I64 N="SourceId">0</I64><PR N="Record"><AV>Expand-Archive</AV><AI>0</AI><Nil /><PI>-1</PI><PC>-1</PC><T>Completed</T><SR>-1</SR><SD>Processing</SD></PR></MS></Obj></Objs>
[stderr did not end with a newline]
[1 CR bytes removed from stderr]
[exit 0]
~~~

#### real progress: Compress-Archive of a small file then Expand-Archive, with the prelude

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIA...\[1150\ chars\] ` @ 2026-10-15T02:39:09Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; $d=Join-Path $env:USERPROFILE 'ign-c7a'; Compress-Archive -LiteralPath (Join-Path $d 'put-rel.txt') -DestinationPath (Join-Path $d 'z.zip') -Force; Expand-Archive -LiteralPath (Join-Path $d 'z.zip') -DestinationPath (Join-Path $d 'zx') -Force; Remove-Item -LiteralPath (Join-Path $d 'z.zip'),(Join-Path $d 'zx') -Recurse -Force; 'archived'
[stdout]
archived
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### a stderr line written by the host, with the prelude

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIA...\[382\ chars\] ` @ 2026-10-15T02:39:11Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; [Console]::Error.WriteLine('to-stderr'); 'to-stdout'
[stdout]
to-stdout
[1 CR bytes removed from stdout]
[stderr]
to-stderr
[1 CR bytes removed from stderr]
[exit 0]
~~~

#### non-ASCII output (UTF-8 or the console code page?), with the prelude

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIAAkAEUAcgByAG8AcgBBAGMAdABpAG8AbgBQAHIAZQBmAGUAcgBlAG4AYwBlAD0AJwBTAHQAbwBwACcAOwAgACcAYwBhAGYA6QAgAPwAIADxACcA ` @ 2026-10-15T02:39:12Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; 'café ü ñ'
[stdout]
caf\x82 \x81 \xA4
[1 CR bytes removed from stdout]
[stdout is not valid UTF-8: every byte outside printable ASCII is shown as \xNN]
[stderr]
[exit 0]
~~~

#### a script that prints a sentinel line and a JSON object, with the prelude

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIAAkAEUAcgByAG8AcgBBAGMAdABpAG8AbgBQAHIAZQBmAGUAcgBlAG4AYwBlAD0AJwBTAHQAbwBwACcAOwAgAFcAcgBpAHQAZQAtAE8AdQB0AHAAdQB0ACAAJwBJAEcATgAtAFIARQBTAFUATABUACAAewAiAG8AawAiADoAdAByAHUAZQB9ACcA ` @ 2026-10-15T02:39:14Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; Write-Output 'IGN-RESULT {"ok":true}'
[stdout]
IGN-RESULT {"ok":true}
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### PowerShell Rename-Item onto an existing name (src and dst both exist)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIA...\[1054\ chars\] ` @ 2026-10-15T02:39:16Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; $d=Join-Path $env:USERPROFILE 'ign-c7a'; Set-Content -LiteralPath (Join-Path $d 'pa-src.txt') -Value 'new'; Set-Content -LiteralPath (Join-Path $d 'pa-dst.txt') -Value 'old'; Rename-Item -LiteralPath (Join-Path $d 'pa-src.txt') -NewName 'pa-dst.txt'; Get-Content -LiteralPath (Join-Path $d 'pa-dst.txt')
[stdout]
[stderr]
#< CLIXML
<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04"><S S="Error">Rename-Item : Cannot create a file when that file already exists._x000D__x000A_</S><S S="Error">At line:1 char:247_x000D__x000A_</S><S S="Error">+ ... alue 'old'; Rename-Item -LiteralPath (Join-Path $d 'pa-src.txt') -New ..._x000D__x000A_</S><S S="Error">+                 ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~_x000D__x000A_</S><S S="Error">    + CategoryInfo          : WriteError: (C:\Users\svc-ig...-c7a\pa-src.txt:String) [Rename-Item], IOException_x000D__x000A_</S><S S="Error">    + FullyQualifiedErrorId : RenameItemIOError,Microsoft.PowerShell.Commands.RenameItemCommand_x000D__x000A_</S><S S="Error"> _x000D__x000A_</S></Objs>
[stderr did not end with a newline]
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### PowerShell Move-Item -Force onto an existing name

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIA...\[1118\ chars\] ` @ 2026-10-15T02:39:17Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; $d=Join-Path $env:USERPROFILE 'ign-c7a'; Set-Content -LiteralPath (Join-Path $d 'pb-src.txt') -Value 'new'; Set-Content -LiteralPath (Join-Path $d 'pb-dst.txt') -Value 'old'; Move-Item -LiteralPath (Join-Path $d 'pb-src.txt') -Destination (Join-Path $d 'pb-dst.txt') -Force; Get-Content -LiteralPath (Join-Path $d 'pb-dst.txt')
[stdout]
new
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### .NET File.Move onto an existing name (.NET Framework 4.x has no overwrite)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIA...\[1046\ chars\] ` @ 2026-10-15T02:39:19Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; $d=Join-Path $env:USERPROFILE 'ign-c7a'; Set-Content -LiteralPath (Join-Path $d 'pc-src.txt') -Value 'new'; Set-Content -LiteralPath (Join-Path $d 'pc-dst.txt') -Value 'old'; [IO.File]::Move((Join-Path $d 'pc-src.txt'),(Join-Path $d 'pc-dst.txt')); Get-Content -LiteralPath (Join-Path $d 'pc-dst.txt')
[stdout]
[stderr]
#< CLIXML
<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04"><S S="Error">Exception calling "Move" with "2" argument(s): "Cannot create a file when that file already exists._x000D__x000A_</S><S S="Error">"_x000D__x000A_</S><S S="Error">At line:1 char:247_x000D__x000A_</S><S S="Error">+ ... alue 'old'; [IO.File]::Move((Join-Path $d 'pc-src.txt'),(Join-Path $d ..._x000D__x000A_</S><S S="Error">+                 ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~_x000D__x000A_</S><S S="Error">    + CategoryInfo          : NotSpecified: (:) [], ParentContainsErrorRecordException_x000D__x000A_</S><S S="Error">    + FullyQualifiedErrorId : IOException_x000D__x000A_</S><S S="Error"> _x000D__x000A_</S></Objs>
[stderr did not end with a newline]
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### .NET File.Replace with a null backup name (atomic swap of two files)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIA...\[1110\ chars\] ` @ 2026-10-15T02:39:22Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; $d=Join-Path $env:USERPROFILE 'ign-c7a'; Set-Content -LiteralPath (Join-Path $d 'pd-src.txt') -Value 'new'; Set-Content -LiteralPath (Join-Path $d 'pd-dst.txt') -Value 'old'; [IO.File]::Replace((Join-Path $d 'pd-src.txt'),(Join-Path $d 'pd-dst.txt'),[NullString]::Value); Get-Content -LiteralPath (Join-Path $d 'pd-dst.txt')
[stdout]
new
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### Directory swap: Move-Item of a directory onto an existing directory name, and rename-aside-then-rename (the directory commit sequence)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIA...\[1786\ chars\] ` @ 2026-10-15T02:39:24Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; $d=Join-Path $env:USERPROFILE 'ign-c7a'; New-Item -ItemType Directory -Path (Join-Path $d 'pe-new') | Out-Null; Set-Content -LiteralPath (Join-Path $d 'pe-new\n.txt') -Value 'new'; New-Item -ItemType Directory -Path (Join-Path $d 'pe-live') | Out-Null; Set-Content -LiteralPath (Join-Path $d 'pe-live\o.txt') -Value 'old'; Rename-Item -LiteralPath (Join-Path $d 'pe-live') -NewName 'pe-old'; Rename-Item -LiteralPath (Join-Path $d 'pe-new') -NewName 'pe-live'; (Get-ChildItem -LiteralPath (Join-Path $d 'pe-live')).Name; (Get-ChildItem -LiteralPath (Join-Path $d 'pe-old')).Name
[stdout]
n.txt
o.txt
[2 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### ssh with a plain cmd.exe command: exit code propagation

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 cmd\ /c\ exit\ 5 ` @ 2026-10-15T02:39:26Z

~~~
[stdout]
[stderr]
[exit 5]
~~~

#### ssh with a plain command that does not exist: exit code and stderr

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 definitely-not-a-command-xyz ` @ 2026-10-15T02:39:27Z

~~~
[stdout]
[stderr]
'definitely-not-a-command-xyz' is not recognized as an internal or external command,
operable program or batch file.
[2 CR bytes removed from stderr]
[exit 1]
~~~

#### ssh with no output and exit 0

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 cmd\ /c\ exit\ 0 ` @ 2026-10-15T02:39:29Z

~~~
[stdout]
[stderr]
[exit 0]
~~~

### C7a-7 — maximum command length

#### command-length ladder: powershell -EncodedCommand with a padded script and the prelude; each row is one ssh run

`script = "<prelude>$x='<pad>'; Write-Output ('LEN=' + $x.Length)"`, padding chosen so the REMOTE COMMAND STRING (the argument after the destination) is about T characters; each row runs `ssh -T -p 22 -i <key> <H> -- user@host <that string>` @ 2026-10-15T02:39:30Z

~~~
target_chars  actual_chars  exit  seconds  stdout(first line)  stderr(first 120 chars)
        1000          1002     0        2  LEN=244  
        4000          4002     0        3  LEN=1369  
        8000          8002     0        2  LEN=2869  
        8100          8102     0        2  LEN=2907  
        8120          8122     0        2  LEN=2914  
        8140          8142     0        2  LEN=2922  
        8150          8154     0        2  LEN=2926  
        8160          8162     1        1    The command line is too long.
        8170          8174     1        1    The command line is too long.
        8180          8182     1        1    The command line is too long.
        8185          8190     1        1    The command line is too long.
        8190          8194     1        2    The command line is too long.
        8191          8194     1        1    The command line is too long.
        8192          8194     1        1    The command line is too long.
        8300          8302     1        2    The command line is too long.
       12000         12002     1        1    The command line is too long.
       24000         24002     1        1    The command line is too long.
       32000         32002     1        2    The command line is too long.
       32700         32702     1        1    The command line is too long.
       32760         32762   255        1    exec request failed on channel 0
       32766         32770   255        2    exec request failed on channel 0
       32767         32770   255        1    exec request failed on channel 0
       32768         32770   255        1    exec request failed on channel 0
       33000         33002   255        2    exec request failed on channel 0
       65000         65002   255        1    exec request failed on channel 0
      100000        100002   255        1    exec request failed on channel 0
~~~

### C7a-8 — sftp-server paths over 260 characters

#### build seven nested directories of 58 characters by mkdir + cd, then put a file (the batch aborts at the first failing command; the echoed lines show how far it got)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:40:09Z

~~~
[stdin]
mkdir "ign-c7a/lp"
cd "ign-c7a/lp"
mkdir "s1xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
cd "s1xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
mkdir "s2xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
cd "s2xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
mkdir "s3xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
cd "s3xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
mkdir "s4xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
cd "s4xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
mkdir "s5xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
cd "s5xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
mkdir "s6xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
cd "s6xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
mkdir "s7xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
cd "s7xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
put "$TMP/local/a.txt" "deep.txt"
pwd
@ls -1 "."
[stdout]
sftp> mkdir "ign-c7a/lp"
sftp> cd "ign-c7a/lp"
sftp> mkdir "s1xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
sftp> cd "s1xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
sftp> mkdir "s2xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
sftp> cd "s2xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
sftp> mkdir "s3xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
sftp> cd "s3xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
sftp> mkdir "s4xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
[stderr]
remote mkdir "/C:/Users/svc-ignition-deploy/ign-c7a/lp/s1xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx/s2xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx/s3xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx/s4xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx": No such file or directory
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### longest full path under ign-c7a/lp as PowerShell sees it (\\?\ prefix; length in characters)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIA...\[842\ chars\] ` @ 2026-10-15T02:40:12Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; $p='\\?\'+(Join-Path $env:USERPROFILE 'ign-c7a\lp'); $m=0; $last=''; Get-ChildItem -LiteralPath $p -Recurse -Force | ForEach-Object { $l=$_.FullName.Length; if($l -gt $m){ $m=$l; $last=$_.FullName } }; 'max_len=' + $m; $last
[stdout]
max_len=220
\\?\C:\Users\svc-ignition-deploy\ign-c7a\lp\s1xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\s2xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\s3xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
[2 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### build a tree with a 300+ character path with the \\?\ prefix (PowerShell), put a file at the bottom

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIA...\[998\ chars\] ` @ 2026-10-15T02:40:13Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; $base='\\?\'+(Join-Path $env:USERPROFILE 'ign-c7a\lq'); $seg='y'*56; $p=$base; 1..6 | ForEach-Object { $p=$p+'\'+('t'+$_+$seg) }; New-Item -ItemType Directory -Path $p -Force | Out-Null; Set-Content -LiteralPath ($p+'\deep.txt') -Value 'deep'; 'bottom_len=' + ($p+'\deep.txt').Length
[stdout]
bottom_len=406
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### ls -1 of level 1 of the deep tree, absolute path of 99 characters

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:40:15Z

~~~
[stdin]
@ls -1 "/C:/Users/svc-ignition-deploy/ign-c7a/lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy"
[stdout]
/C:/Users/svc-ignition-deploy/ign-c7a/lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t2yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy
[stderr]
[exit 0]
~~~

#### ls -1 of level 2 of the deep tree, absolute path of 158 characters

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:40:18Z

~~~
[stdin]
@ls -1 "/C:/Users/svc-ignition-deploy/ign-c7a/lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t2yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy"
[stdout]
/C:/Users/svc-ignition-deploy/ign-c7a/lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t2yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t3yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy
[stderr]
[exit 0]
~~~

#### ls -1 of level 3 of the deep tree, absolute path of 217 characters

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:40:20Z

~~~
[stdin]
@ls -1 "/C:/Users/svc-ignition-deploy/ign-c7a/lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t2yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t3yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy"
[stdout]
[stderr]
[exit 0]
~~~

#### ls -1 of level 4 of the deep tree, absolute path of 276 characters

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:40:21Z

~~~
[stdin]
@ls -1 "/C:/Users/svc-ignition-deploy/ign-c7a/lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t2yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t3yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t4yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy"
[stdout]
[stderr]
Can't ls: "/C:/Users/svc-ignition-deploy/ign-c7a/lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t2yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t3yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t4yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy" not found
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### ls -1 of level 5 of the deep tree, absolute path of 335 characters

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:40:23Z

~~~
[stdin]
@ls -1 "/C:/Users/svc-ignition-deploy/ign-c7a/lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t2yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t3yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t4yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t5yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy"
[stdout]
[stderr]
Can't ls: "/C:/Users/svc-ignition-deploy/ign-c7a/lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t2yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t3yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t4yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t5yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy" not found
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### ls -1 of level 6 of the deep tree, absolute path of 394 characters

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:40:25Z

~~~
[stdin]
@ls -1 "/C:/Users/svc-ignition-deploy/ign-c7a/lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t2yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t3yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t4yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t5yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t6yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy"
[stdout]
[stderr]
Can't ls: "/C:/Users/svc-ignition-deploy/ign-c7a/lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t2yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t3yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t4yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t5yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t6yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy" not found
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### ls -l of the deepest directory by its absolute path (394 chars)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:40:28Z

~~~
[stdin]
@ls -l "/C:/Users/svc-ignition-deploy/ign-c7a/lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t2yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t3yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t4yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t5yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t6yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy"
[stdout]
[stderr]
Can't ls: "/C:/Users/svc-ignition-deploy/ign-c7a/lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t2yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t3yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t4yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t5yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t6yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy" not found
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### get of the deep file by its absolute path

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:40:30Z

~~~
[stdin]
get "/C:/Users/svc-ignition-deploy/ign-c7a/lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t2yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t3yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t4yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t5yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t6yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/deep.txt" "$TMP/local/deepget/deep.txt"
[stdout]
sftp> get "/C:/Users/svc-ignition-deploy/ign-c7a/lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t2yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t3yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t4yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t5yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t6yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/deep.txt" "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/deepget/deep.txt"
[stderr]
File "/C:/Users/svc-ignition-deploy/ign-c7a/lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t2yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t3yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t4yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t5yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t6yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/deep.txt" not found.
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### get -R of ign-c7a/lq (the whole deep tree)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:40:32Z

~~~
[stdin]
get -R "ign-c7a/lq" "$TMP/local/deepget"
[stdout]
sftp> get -R "ign-c7a/lq" "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.2xgL3f0joV/local/deepget"
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/lq/
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t2yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy
Retrieving /C:/Users/svc-ignition-deploy/ign-c7a/lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t2yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t3yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy
[stderr]
[exit 0]
~~~

#### what the deep get -R left locally: every directory and file with its path length (a silent stop would show as missing levels)

`bash -c cd\ \"\$1\"\ \&\&\ find\ .\ \|\ awk\ \"\{\ print\ length\(\\\$0\)\ \\\"\ \\\"\ \\\$0\ \}\"\ \|\ cut\ -c1-140\ \&\&\ find\ .\ -type\ f\ -exec\ shasum\ -a\ 256\ \{\}\ \\\; _ $TMP/local/deepget ` @ 2026-10-15T02:40:35Z

~~~
[stdout]
1 .
4 ./lq
63 ./lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy
122 ./lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t2yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy
181 ./lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t2yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t3yyyyyyyyyyy
[stderr]
[exit 0]
~~~

#### what the deep tree really holds (levels and the file) as PowerShell sees it

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIA...\[870\ chars\] ` @ 2026-10-15T02:40:35Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; $p='\\?\'+(Join-Path $env:USERPROFILE 'ign-c7a\lq'); Get-ChildItem -LiteralPath $p -Recurse -Force | ForEach-Object { '{0} {1}' -f $_.FullName.Length, ($_.FullName.Substring($_.FullName.Length - [Math]::Min(30, $_.FullName.Length))) }
[stdout]
102 yyyyyyyyyyyyyyyyyyyyyyyyyyyyyy
161 yyyyyyyyyyyyyyyyyyyyyyyyyyyyyy
220 yyyyyyyyyyyyyyyyyyyyyyyyyyyyyy
279 yyyyyyyyyyyyyyyyyyyyyyyyyyyyyy
338 yyyyyyyyyyyyyyyyyyyyyyyyyyyyyy
397 yyyyyyyyyyyyyyyyyyyyyyyyyyyyyy
406 yyyyyyyyyyyyyyyyyyyyy\deep.txt
[7 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### ls -1 of the intermediate levels (is the path limit per call or per tree?)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:40:38Z

~~~
[stdin]
@ls -1 "/C:/Users/svc-ignition-deploy/ign-c7a/lq"
@ls -1 "/C:/Users/svc-ignition-deploy/ign-c7a/lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy"
[stdout]
/C:/Users/svc-ignition-deploy/ign-c7a/lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy
/C:/Users/svc-ignition-deploy/ign-c7a/lq/t1yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy/t2yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy
[stderr]
[exit 0]
~~~

### C7a-cleanup

#### cleanup: remove the account-home scratch directory ign-c7a

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIA...\[1282\ chars\] ` @ 2026-10-15T02:40:42Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Continue'; $p=Join-Path $env:USERPROFILE 'ign-c7a'; if(Test-Path -LiteralPath $p){ foreach($n in 'jx\junc','jx\loop','jx\symd'){ $q=Join-Path $p $n; if(Test-Path -LiteralPath $q){ cmd /c rmdir "$q" } }; Remove-Item -LiteralPath ('\\?\'+$p) -Recurse -Force; if(Test-Path -LiteralPath $p){ cmd /c rd /s /q ('\\?\'+$p) } }; if(Test-Path -LiteralPath $p){ 'STILL PRESENT' } else { 'ign-c7a removed' }
[stdout]
ign-c7a removed
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### the scratch directory is gone (dir of the home)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 dir\ /b\ \"%USERPROFILE%\" ` @ 2026-10-15T02:40:43Z

~~~
[stdout]
.ssh
Desktop
Documents
Downloads
Favorites
Links
Music
Pictures
Saved Games
Videos
[10 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~



## C6 — what svc-ignition-deploy can write and read (Decision B, research A16)

Account `svc-ignition-deploy` on 40.114.71.134. The gateway is the one behind the `azure-win` profile. A write probe inside the mode collection uses a new directory `zz-c6-probe` that is removed in the same session; `GET /data/api/v1/scan/config` is recorded before and after. Home and `ign-deploy` probes are removed at the end. ACL listings are taken as the account over ssh (`icacls`), the account's own view.

`<H>` stands for exactly these ssh/sftp options, in this order (the `hardened_options` set with an identity): `-o BatchMode=yes -o StrictHostKeyChecking=yes -o UserKnownHostsFile=$HOME/.cache/ign-wflwvm01/known_hosts -o GlobalKnownHostsFile=/dev/null -o UpdateHostKeys=no -o VerifyHostKeyDNS=no -o PreferredAuthentications=publickey -o PasswordAuthentication=no -o KbdInteractiveAuthentication=no -o NumberOfPasswordPrompts=0 -o IdentitiesOnly=yes -o ForwardAgent=no -o ForwardX11=no -o ClearAllForwardings=yes -o PermitLocalCommand=no -o ControlMaster=no -o ControlPath=none -o ConnectTimeout=10 -o ServerAliveInterval=5 -o ServerAliveCountMax=3 -o LogLevel=ERROR `

### C6-acl — the ACLs that explain each answer

#### whoami /groups (the account, again)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 whoami\ /groups ` @ 2026-10-15T02:43:33Z

~~~
[stdout]

GROUP INFORMATION
-----------------

Group Name                             Type             SID          Attributes                                        
====================================== ================ ============ ==================================================
Everyone                               Well-known group S-1-1-0      Mandatory group, Enabled by default, Enabled group
BUILTIN\Users                          Alias            S-1-5-32-545 Mandatory group, Enabled by default, Enabled group
NT AUTHORITY\NETWORK                   Well-known group S-1-5-2      Mandatory group, Enabled by default, Enabled group
NT AUTHORITY\Authenticated Users       Well-known group S-1-5-11     Mandatory group, Enabled by default, Enabled group
NT AUTHORITY\This Organization         Well-known group S-1-5-15     Mandatory group, Enabled by default, Enabled group
NT AUTHORITY\Local account             Well-known group S-1-5-113    Mandatory group, Enabled by default, Enabled group
NT AUTHORITY\NTLM Authentication       Well-known group S-1-5-64-10  Mandatory group, Enabled by default, Enabled group
Mandatory Label\Medium Mandatory Level Label            S-1-16-8192                                                    
[14 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### icacls data dir

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 icacls\ \"C:\\Program\ Files\\Inductive\ Automation\\Ignition\\data\" ` @ 2026-10-15T02:43:34Z

~~~
[stdout]
C:\Program Files\Inductive Automation\Ignition\data NT SERVICE\TrustedInstaller:(I)(F)
                                                    NT SERVICE\TrustedInstaller:(I)(CI)(IO)(F)
                                                    NT AUTHORITY\SYSTEM:(I)(F)
                                                    NT AUTHORITY\SYSTEM:(I)(OI)(CI)(IO)(F)
                                                    BUILTIN\Administrators:(I)(F)
                                                    BUILTIN\Administrators:(I)(OI)(CI)(IO)(F)
                                                    BUILTIN\Users:(I)(RX)
                                                    BUILTIN\Users:(I)(OI)(CI)(IO)(GR,GE)
                                                    CREATOR OWNER:(I)(OI)(CI)(IO)(F)
                                                    APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:(I)(RX)
                                                    APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:(I)(OI)(CI)(IO)(GR,GE)
                                                    APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:(I)(RX)
                                                    APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:(I)(OI)(CI)(IO)(GR,GE)

Successfully processed 1 files; Failed processing 0 files
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### icacls data\\config

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 icacls\ \"C:\\Program\ Files\\Inductive\ Automation\\Ignition\\data\\config\" ` @ 2026-10-15T02:43:35Z

~~~
[stdout]
C:\Program Files\Inductive Automation\Ignition\data\config NT SERVICE\TrustedInstaller:(I)(F)
                                                           NT SERVICE\TrustedInstaller:(I)(CI)(IO)(F)
                                                           NT AUTHORITY\SYSTEM:(I)(F)
                                                           NT AUTHORITY\SYSTEM:(I)(OI)(CI)(IO)(F)
                                                           BUILTIN\Administrators:(I)(F)
                                                           BUILTIN\Administrators:(I)(OI)(CI)(IO)(F)
                                                           BUILTIN\Users:(I)(RX)
                                                           BUILTIN\Users:(I)(OI)(CI)(IO)(GR,GE)
                                                           CREATOR OWNER:(I)(OI)(CI)(IO)(F)
                                                           APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:(I)(RX)
                                                           APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:(I)(OI)(CI)(IO)(GR,GE)
                                                           APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:(I)(RX)
                                                           APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:(I)(OI)(CI)(IO)(GR,GE)

Successfully processed 1 files; Failed processing 0 files
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### icacls data\\config\\resources

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 icacls\ \"C:\\Program\ Files\\Inductive\ Automation\\Ignition\\data\\config\\resources\" ` @ 2026-10-15T02:43:37Z

~~~
[stdout]
C:\Program Files\Inductive Automation\Ignition\data\config\resources ignwin836\svc-ignition-deploy:(OI)(CI)(RX)
                                                                     NT SERVICE\TrustedInstaller:(I)(F)
                                                                     NT SERVICE\TrustedInstaller:(I)(CI)(IO)(F)
                                                                     NT AUTHORITY\SYSTEM:(I)(F)
                                                                     NT AUTHORITY\SYSTEM:(I)(OI)(CI)(IO)(F)
                                                                     BUILTIN\Administrators:(I)(F)
                                                                     BUILTIN\Administrators:(I)(OI)(CI)(IO)(F)
                                                                     BUILTIN\Users:(I)(RX)
                                                                     BUILTIN\Users:(I)(OI)(CI)(IO)(GR,GE)
                                                                     CREATOR OWNER:(I)(OI)(CI)(IO)(F)
                                                                     APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:(I)(RX)
                                                                     APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:(I)(OI)(CI)(IO)(GR,GE)
                                                                     APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:(I)(RX)
                                                                     APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:(I)(OI)(CI)(IO)(GR,GE)

Successfully processed 1 files; Failed processing 0 files
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### icacls data\\config\\resources\\core

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 icacls\ \"C:\\Program\ Files\\Inductive\ Automation\\Ignition\\data\\config\\resources\\core\" ` @ 2026-10-15T02:43:38Z

~~~
[stdout]
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core ignwin836\svc-ignition-deploy:(I)(OI)(CI)(RX)
                                                                          NT SERVICE\TrustedInstaller:(I)(F)
                                                                          NT SERVICE\TrustedInstaller:(I)(CI)(IO)(F)
                                                                          NT AUTHORITY\SYSTEM:(I)(F)
                                                                          NT AUTHORITY\SYSTEM:(I)(OI)(CI)(IO)(F)
                                                                          BUILTIN\Administrators:(I)(F)
                                                                          BUILTIN\Administrators:(I)(OI)(CI)(IO)(F)
                                                                          BUILTIN\Users:(I)(RX)
                                                                          BUILTIN\Users:(I)(OI)(CI)(IO)(GR,GE)
                                                                          CREATOR OWNER:(I)(OI)(CI)(IO)(F)
                                                                          APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:(I)(RX)
                                                                          APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:(I)(OI)(CI)(IO)(GR,GE)
                                                                          APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:(I)(RX)
                                                                          APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:(I)(OI)(CI)(IO)(GR,GE)

Successfully processed 1 files; Failed processing 0 files
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### dir of data\\config\\resources (which collections exist)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 dir\ /a\ \"C:\\Program\ Files\\Inductive\ Automation\\Ignition\\data\\config\\resources\" ` @ 2026-10-15T02:43:40Z

~~~
[stdout]
 Volume in drive C is Windows
 Volume Serial Number is 6AEF-F68E

 Directory of C:\Program Files\Inductive Automation\Ignition\data\config\resources

10/15/2026  02:08 AM    <DIR>          .
10/15/2026  02:05 AM    <DIR>          ..
10/15/2026  02:10 AM    <DIR>          .resources
10/15/2026  02:05 AM    <DIR>          core
10/15/2026  02:07 AM    <DIR>          external
10/15/2026  02:05 AM    <DIR>          local
10/15/2026  02:04 AM            26,893 migration-log-202610150204.md
10/15/2026  02:08 AM    <DIR>          production
               1 File(s)         26,893 bytes
               7 Dir(s)  116,338,270,208 bytes free
[15 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### icacls data\\config\\resources\\production

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 icacls\ \"C:\\Program\ Files\\Inductive\ Automation\\Ignition\\data\\config\\resources\\production\" ` @ 2026-10-15T02:43:41Z

~~~
[stdout]
C:\Program Files\Inductive Automation\Ignition\data\config\resources\production ignwin836\svc-ignition-deploy:(I)(OI)(CI)(RX)
                                                                                NT SERVICE\TrustedInstaller:(I)(F)
                                                                                NT SERVICE\TrustedInstaller:(I)(CI)(IO)(F)
                                                                                NT AUTHORITY\SYSTEM:(I)(F)
                                                                                NT AUTHORITY\SYSTEM:(I)(OI)(CI)(IO)(F)
                                                                                BUILTIN\Administrators:(I)(F)
                                                                                BUILTIN\Administrators:(I)(OI)(CI)(IO)(F)
                                                                                BUILTIN\Users:(I)(RX)
                                                                                BUILTIN\Users:(I)(OI)(CI)(IO)(GR,GE)
                                                                                CREATOR OWNER:(I)(OI)(CI)(IO)(F)
                                                                                APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:(I)(RX)
                                                                                APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:(I)(OI)(CI)(IO)(GR,GE)
                                                                                APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:(I)(RX)
                                                                                APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:(I)(OI)(CI)(IO)(GR,GE)

Successfully processed 1 files; Failed processing 0 files
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### icacls data\\ign-deploy (does it exist?)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 icacls\ \"C:\\Program\ Files\\Inductive\ Automation\\Ignition\\data\\ign-deploy\" ` @ 2026-10-15T02:43:42Z

~~~
[stdout]
Successfully processed 0 files; Failed processing 1 files
[1 CR bytes removed from stdout]
[stderr]
C:\Program Files\Inductive Automation\Ignition\data\ign-deploy: The system cannot find the file specified.
[1 CR bytes removed from stderr]
[exit 2]
~~~

#### icacls the account home

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 icacls\ \"%USERPROFILE%\" ` @ 2026-10-15T02:43:44Z

~~~
[stdout]
C:\Users\svc-ignition-deploy NT AUTHORITY\SYSTEM:(OI)(CI)(F)
                             BUILTIN\Administrators:(OI)(CI)(F)
                             ignwin836\svc-ignition-deploy:(OI)(CI)(F)

Successfully processed 1 files; Failed processing 0 files
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### dir of the data dir

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 dir\ /a\ \"C:\\Program\ Files\\Inductive\ Automation\\Ignition\\data\" ` @ 2026-10-15T02:43:45Z

~~~
[stdout]
 Volume in drive C is Windows
 Volume Serial Number is 6AEF-F68E

 Directory of C:\Program Files\Inductive Automation\Ignition\data

10/15/2026  02:05 AM    <DIR>          .
10/15/2026  02:02 AM    <DIR>          ..
10/15/2026  02:09 AM                16 .context.tmp
10/15/2026  02:09 AM             1,313 .gateway.xml.bak
10/15/2026  02:09 AM             1,286 .redundancy.xml.bak
10/15/2026  02:03 AM    <DIR>          certificates
10/15/2026  02:04 AM               139 commissioning.json
10/15/2026  02:05 AM    <DIR>          config
10/15/2026  02:10 AM    <DIR>          db
10/15/2026  02:04 AM    <DIR>          email-profiles
10/15/2026  02:09 AM             1,313 gateway.xml
10/15/2026  02:02 AM               811 gateway.xml_clean
10/15/2026  02:09 AM             8,960 ignition.conf
10/15/2026  02:04 AM                90 init.properties.bak
10/15/2026  02:04 AM    <DIR>          jar-cache
10/15/2026  02:02 AM               271 log4j.properties
10/15/2026  02:02 AM             2,567 logback.xml
10/15/2026  02:42 AM    <DIR>          metricsdb
10/15/2026  02:09 AM             7,813 modules.json
10/15/2026  02:05 AM    <DIR>          projects
10/15/2026  02:09 AM             1,286 redundancy.xml
10/15/2026  02:03 AM                 0 request
10/15/2026  02:03 AM                 0 response
10/15/2026  02:05 AM    <DIR>          var
              14 File(s)         25,865 bytes
              10 Dir(s)  116,336,730,112 bytes free
[31 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### is the data dir and the home on the same volume? (fsutil is admin-only; vol and the drive letters instead)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 echo\ %USERPROFILE%\ \&\ echo\ %SystemDrive%\ \&\ vol\ C: ` @ 2026-10-15T02:43:46Z

~~~
[stdout]
C:\Users\svc-ignition-deploy 
C: 
 Volume in drive C is Windows
 Volume Serial Number is 6AEF-F68E
[4 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

### C6-scan-before — gateway scan state before any write probe

#### GET scan/config (before)

`ign --profile azure-win --json api call --method GET --path /data/api/v1/scan/config ` @ 2026-10-15T02:43:48Z

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
  "lastScanTimestamp": 1792030196522,
  "lastScanDuration": 639
}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET mode (deployment modes)

`ign --profile azure-win --json api call --method GET --path /data/api/v1/mode ` @ 2026-10-15T02:43:48Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "method": "GET",
    "path": "/data/api/v1/mode",
    "result": {
      "status": 200,
      "data": {"items":[{"name":"production","title":"Production","description":"ign-win-836 (azure-win)","resourceCount":0}],"metadata":{"total":1,"matching":1,"limit":-1,"offset":0}}
    }
  }
}
[stderr]
[exit 0]
~~~

#### GET gateway-info (the active deploymentMode)

`ign --profile azure-win --json api call --method GET --path /data/api/v1/gateway-info ` @ 2026-10-15T02:43:48Z

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
      "data": {"name":"ign-win-836","redundancyRole":"Independent","edition":"standard","hostname":"40.114.71.134","port":"8088","ignitionVersion":"8.3.6 (b2026042713)","deploymentMode":"production","timeZone":"Coordinated Universal Time","timeZoneId":"Etc/UTC","jvmVersion":"17.0.18","allowUnsignedModules":false,"license":{"mode":"Trial","validForVersion":8,"expirationDate":"Oct 15, 2026, 4:05:48 AM","licenseRestrictions":[]}}
    }
  }
}
[stderr]
[exit 0]
~~~

### C6-1 — home-relative staging directory ign-deploy/

#### mkdir ign-deploy in the home, put a file, rename it, list it

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:43:48Z

~~~
[stdin]
mkdir "ign-deploy"
mkdir "ign-deploy/gw"
put "$TMP/local/a.txt" "ign-deploy/gw/stage1.txt"
rename "ign-deploy/gw/stage1.txt" "ign-deploy/gw/stage2.txt"
@ls -l "ign-deploy/gw"
[stdout]
sftp> mkdir "ign-deploy"
sftp> mkdir "ign-deploy/gw"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.v5PIQfSVVF/local/a.txt" "ign-deploy/gw/stage1.txt"
sftp> rename "ign-deploy/gw/stage1.txt" "ign-deploy/gw/stage2.txt"
-rw-******    1 -        -               6 Oct 15 02:43 stage2.txt
[stderr]
[exit 0]
~~~

#### the same under the /C:/ absolute home path

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:43:51Z

~~~
[stdin]
put "$TMP/local/b.txt" "/C:/Users/svc-ignition-deploy/ign-deploy/gw/abs.txt"
@ls -1 "/C:/Users/svc-ignition-deploy/ign-deploy/gw"
[stdout]
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.v5PIQfSVVF/local/b.txt" "/C:/Users/svc-ignition-deploy/ign-deploy/gw/abs.txt"
/C:/Users/svc-ignition-deploy/ign-deploy/gw/abs.txt
/C:/Users/svc-ignition-deploy/ign-deploy/gw/stage2.txt
[stderr]
[exit 0]
~~~

### C6-2 — write directly into the mode collection (production)

#### ls -la of production (does the collection directory exist, who owns it)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:43:53Z

~~~
[stdin]
@ls -la "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production"
[stdout]
-rw-******    1 -        -             133 Oct 15 02:08 config-mode.json
[stderr]
[exit 0]
~~~

#### mkdir zz-c6-probe inside production

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:43:56Z

~~~
[stdin]
mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe"
[stdout]
sftp> mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe"
[stderr]
remote mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe": Permission denied
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### cross-directory rename of a staged FILE directly into production (the B(i) commit step; no probe directory needed)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:43:57Z

~~~
[stdin]
rename "ign-deploy/gw/stage2.txt" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-moved.txt"
[stdout]
sftp> rename "ign-deploy/gw/stage2.txt" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-moved.txt"
[stderr]
remote rename "/C:/Users/svc-ignition-deploy/ign-deploy/gw/stage2.txt" to "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-moved.txt": Permission denied
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### cross-directory rename of a staged DIRECTORY directly into production

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:43:59Z

~~~
[stdin]
mkdir "ign-deploy/gw/unit"
put "$TMP/local/a.txt" "ign-deploy/gw/unit/f.txt"
rename "ign-deploy/gw/unit" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-unit"
[stdout]
sftp> mkdir "ign-deploy/gw/unit"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.v5PIQfSVVF/local/a.txt" "ign-deploy/gw/unit/f.txt"
sftp> rename "ign-deploy/gw/unit" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-unit"
[stderr]
remote rename "/C:/Users/svc-ignition-deploy/ign-deploy/gw/unit" to "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-unit": Permission denied
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### put a file straight into production (no subdirectory)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:01Z

~~~
[stdin]
put "$TMP/local/a.txt" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-direct.txt"
[stdout]
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.v5PIQfSVVF/local/a.txt" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-direct.txt"
[stderr]
dest open "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-direct.txt": Permission denied
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### put a file into production/zz-c6-probe

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:03Z

~~~
[stdin]
put "$TMP/local/a.txt" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe/probe.txt"
[stdout]
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.v5PIQfSVVF/local/a.txt" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe/probe.txt"
[stderr]
dest open "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe/probe.txt": No such file or directory
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### rename inside production/zz-c6-probe

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:04Z

~~~
[stdin]
rename "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe/probe.txt" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe/probe2.txt"
[stdout]
sftp> rename "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe/probe.txt" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe/probe2.txt"
[stderr]
remote rename "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe/probe.txt" to "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe/probe2.txt": No such file or directory
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### cross-directory rename from the home staging dir INTO production/zz-c6-probe (the B(i) commit step)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:06Z

~~~
[stdin]
rename "ign-deploy/gw/stage2.txt" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe/moved.txt"
[stdout]
sftp> rename "ign-deploy/gw/stage2.txt" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe/moved.txt"
[stderr]
remote rename "/C:/Users/svc-ignition-deploy/ign-deploy/gw/stage2.txt" to "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe/moved.txt": No such file or directory
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### ls -l of production/zz-c6-probe after the attempts

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:08Z

~~~
[stdin]
@ls -l "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe"
[stdout]
[stderr]
Can't ls: "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe" not found
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### remove the probe: rm the files, rmdir the directory

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:09Z

~~~
[stdin]
rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe/probe.txt"
rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe/probe2.txt"
rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe/moved.txt"
rmdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe"
[stdout]
sftp> rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe/probe.txt"
[stderr]
remote delete /C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-probe/probe.txt: No such file or directory
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### production after the cleanup (the probe directory must be gone)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:11Z

~~~
[stdin]
@ls -1 "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production"
[stdout]
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/config-mode.json
[stderr]
[exit 0]
~~~

#### the same probes against the collection root itself: mkdir zz-c6-root-probe directly under resources

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:13Z

~~~
[stdin]
mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/zz-c6-root-probe"
[stdout]
sftp> mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/zz-c6-root-probe"
[stderr]
remote mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/zz-c6-root-probe": Permission denied
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### mkdir the collection directory production2 under resources (what creating a new collection would need)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:15Z

~~~
[stdin]
mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/zz-c6-collection"
[stdout]
sftp> mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/zz-c6-collection"
[stderr]
remote mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/zz-c6-collection": Permission denied
[1 CR bytes removed from stderr]
[exit 1]
~~~

### C6-3 — write <data_dir>\ign-deploy\

#### mkdir ign-deploy directly under the data dir

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:17Z

~~~
[stdin]
mkdir "/C:/Program Files/Inductive Automation/Ignition/data/ign-deploy"
[stdout]
sftp> mkdir "/C:/Program Files/Inductive Automation/Ignition/data/ign-deploy"
[stderr]
remote mkdir "/C:/Program Files/Inductive Automation/Ignition/data/ign-deploy": Permission denied
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### mkdir ign-deploy under data/config

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:19Z

~~~
[stdin]
mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/ign-deploy"
[stdout]
sftp> mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/ign-deploy"
[stderr]
remote mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/ign-deploy": Permission denied
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### mkdir under the data dir (any name): data/zz-c6-datadir

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:20Z

~~~
[stdin]
mkdir "/C:/Program Files/Inductive Automation/Ignition/data/zz-c6-datadir"
[stdout]
sftp> mkdir "/C:/Program Files/Inductive Automation/Ignition/data/zz-c6-datadir"
[stderr]
remote mkdir "/C:/Program Files/Inductive Automation/Ignition/data/zz-c6-datadir": Permission denied
[1 CR bytes removed from stderr]
[exit 1]
~~~

### C6-4 — read core and production

#### ls -1 of core

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:22Z

~~~
[stdin]
@ls -1 "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core"
[stdout]
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/com.inductiveautomation.eam
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/com.inductiveautomation.opcua
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/com.inductiveautomation.perspective
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/com.inductiveautomation.sfc
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/config-mode.json
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition
[stderr]
[exit 0]
~~~

#### ls -1 of core/ignition

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:24Z

~~~
[stdin]
@ls -1 "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition"
[stdout]
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/cobranding
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/database-driver
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/database-translator
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/edge-system-properties
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/gateway-network-proxy-rules
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/gateway-network-queue-settings
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/gateway-network-settings
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/general-alarm-settings
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/identity-provider
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/images
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/keyboard_layout
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/opc-connection
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/quickstart
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/schedule
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/security-levels
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/security-properties
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/system-properties
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-group
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-provider
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-type-definition
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/translations
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/user-source
[stderr]
[exit 0]
~~~

#### ls -1 of production

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:25Z

~~~
[stdin]
@ls -1 "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production"
[stdout]
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/config-mode.json
[stderr]
[exit 0]
~~~

#### get -R of one core directory (security-properties)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:27Z

~~~
[stdin]
get -R "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/security-properties" "$TMP/local/rd_core"
[stdout]
sftp> get -R "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/security-properties" "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.v5PIQfSVVF/local/rd_core"
Retrieving /C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/security-properties/
[stderr]
[exit 0]
~~~

#### get -R of production (whatever it holds)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:30Z

~~~
[stdin]
get -R "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production" "$TMP/local/rd_prod"
[stdout]
sftp> get -R "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production" "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.v5PIQfSVVF/local/rd_prod"
Retrieving /C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/
[stderr]
[exit 0]
~~~

#### what the reads left locally (relative names, sizes; sha256 of the files)

`bash -c cd\ \"\$1\"\ \&\&\ find\ .\ -type\ f\ -exec\ wc\ -c\ \{\}\ \\\;\ \|\ sort\ -k2\ \&\&\ find\ .\ -type\ f\ -exec\ shasum\ -a\ 256\ \{\}\ \\\; _ $TMP/local ` @ 2026-10-15T02:44:32Z

~~~
[stdout]
       6 ./a.txt
       6 ./b.txt
    2839 ./rd_core/security-properties/config.json
     117 ./rd_core/security-properties/resource.json
     133 ./rd_prod/production/config-mode.json
7321dabb9e1d3a8d52eb845232d4c666b2be15241fb237dba4feb3b6f51cc41d  ./rd_core/security-properties/config.json
bc15af88b472315f0500943d714a752a33d15103e9e6ec28d0c63be5271edb7a  ./rd_core/security-properties/resource.json
f7b5b709abfadf5ad7338bd40ca14ebe4b4d123f4b2689112fa4cca36161b2fa  ./rd_prod/production/config-mode.json
5da8f23decf397b13f4f55b6fb8a61936238bfe08ed9d901132974f1beccc45c  ./b.txt
b6a98d9ce9a2d9149288fa3df42d377c3e42737afdcdaf714e33c0a100b51060  ./a.txt
[stderr]
[exit 0]
~~~

#### ls -1 of the tag definitions in core (tag trees live under core/ignition/tag-definition)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:33Z

~~~
[stdin]
@ls -1 "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition"
[stdout]
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/System
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/default
[stderr]
[exit 0]
~~~

#### ls -1 of the other top-level data dir entries the plan does not need (config, data/projects), to bound the read scope

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:34Z

~~~
[stdin]
@ls -1 "/C:/Program Files/Inductive Automation/Ignition/data/config"
@ls -1 "/C:/Program Files/Inductive Automation/Ignition/data/projects"
[stdout]
/C:/Program Files/Inductive Automation/Ignition/data/config/com.inductiveautomation.opcua
/C:/Program Files/Inductive Automation/Ignition/data/config/ignition
/C:/Program Files/Inductive Automation/Ignition/data/config/local
/C:/Program Files/Inductive Automation/Ignition/data/config/resources
[stderr]
[exit 0]
~~~

### C6-scan-after — gateway scan state after the probes

#### GET scan/config (after)

`ign --profile azure-win --json api call --method GET --path /data/api/v1/scan/config ` @ 2026-10-15T02:44:37Z

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
  "lastScanTimestamp": 1792030196522,
  "lastScanDuration": 639
}
    }
  }
}
[stderr]
[exit 0]
~~~

### C6-cleanup

#### remove the home staging probe: files

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:37Z

~~~
[stdin]
rm "ign-deploy/gw/stage2.txt"
rm "ign-deploy/gw/abs.txt"
[stdout]
sftp> rm "ign-deploy/gw/stage2.txt"
sftp> rm "ign-deploy/gw/abs.txt"
[stderr]
[exit 0]
~~~

#### remove the home staging probe: unit directory

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:39Z

~~~
[stdin]
rm "ign-deploy/gw/unit/f.txt"
rmdir "ign-deploy/gw/unit"
[stdout]
sftp> rm "ign-deploy/gw/unit/f.txt"
sftp> rmdir "ign-deploy/gw/unit"
[stderr]
[exit 0]
~~~

#### remove the home staging probe: directories

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:40Z

~~~
[stdin]
rmdir "ign-deploy/gw"
rmdir "ign-deploy"
[stdout]
sftp> rmdir "ign-deploy/gw"
sftp> rmdir "ign-deploy"
[stderr]
[exit 0]
~~~

#### the home after the cleanup

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 dir\ /b\ \"%USERPROFILE%\" ` @ 2026-10-15T02:44:42Z

~~~
[stdout]
.ssh
Desktop
Documents
Downloads
Favorites
Links
Music
Pictures
Saved Games
Videos
[10 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### defensive removal: file in production (only present if a rename or put surprisingly succeeded)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:44Z

~~~
[stdin]
rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-moved.txt"
[stdout]
sftp> rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-moved.txt"
[stderr]
remote delete /C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-moved.txt: No such file or directory
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### defensive removal: file in production (direct put)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:45Z

~~~
[stdin]
rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-direct.txt"
[stdout]
sftp> rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-direct.txt"
[stderr]
remote delete /C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-direct.txt: No such file or directory
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### defensive removal: directory in production

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:47Z

~~~
[stdin]
rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-unit/f.txt"
rmdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-unit"
[stdout]
sftp> rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-unit/f.txt"
[stderr]
remote delete /C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6-unit/f.txt: No such file or directory
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### production after all defensive removals

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:49Z

~~~
[stdin]
@ls -1 "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production"
[stdout]
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/config-mode.json
[stderr]
[exit 0]
~~~

#### defensive removal of anything a surprising success left behind (each line may fail with no such file; the batch aborts at the first)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:51Z

~~~
[stdin]
rmdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/zz-c6-root-probe"
[stdout]
sftp> rmdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/zz-c6-root-probe"
[stderr]
remote rmdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/zz-c6-root-probe": No such file or directory
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### defensive removal: collection probe

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:53Z

~~~
[stdin]
rmdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/zz-c6-collection"
[stdout]
sftp> rmdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/zz-c6-collection"
[stderr]
remote rmdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/zz-c6-collection": No such file or directory
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### defensive removal: data dir probe

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:55Z

~~~
[stdin]
rmdir "/C:/Program Files/Inductive Automation/Ignition/data/zz-c6-datadir"
[stdout]
sftp> rmdir "/C:/Program Files/Inductive Automation/Ignition/data/zz-c6-datadir"
[stderr]
remote rmdir "/C:/Program Files/Inductive Automation/Ignition/data/zz-c6-datadir": No such file or directory
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### defensive removal: data/ign-deploy

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:56Z

~~~
[stdin]
rmdir "/C:/Program Files/Inductive Automation/Ignition/data/ign-deploy"
[stdout]
sftp> rmdir "/C:/Program Files/Inductive Automation/Ignition/data/ign-deploy"
[stderr]
remote rmdir "/C:/Program Files/Inductive Automation/Ignition/data/ign-deploy": No such file or directory
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### defensive removal: data/config/ign-deploy

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:44:58Z

~~~
[stdin]
rmdir "/C:/Program Files/Inductive Automation/Ignition/data/config/ign-deploy"
[stdout]
sftp> rmdir "/C:/Program Files/Inductive Automation/Ignition/data/config/ign-deploy"
[stderr]
remote rmdir "/C:/Program Files/Inductive Automation/Ignition/data/config/ign-deploy": No such file or directory
[1 CR bytes removed from stderr]
[exit 1]
~~~


## C5-NTFS attempt 1 — through `ign tags import` (WebDev routes): every import answered 404, nothing captured

Provider `CAP22H` is created on the capture gateway `azure-win`, nine folders are imported one by one through `ign tags import --provider CAP22H --file -`, the asynchronous flush is given 20 s, the provider's tag directories are listed over sftp as `svc-ignition-deploy` and from the cmd.exe side, the gateway log is read, and the provider is deleted. The WebDev bundle (`ign webdev deploy`, project ign-cli) is deployed first when absent: this is a capture host, not a customer gateway. Render stays fail-closed on Windows-illegal names whatever this shows.

Note on this attempt (written by the operator after the run): `ign webdev deploy` answered success (project `ign-cli` was created on the capture gateway; that project stays there), but `ign webdev status` and every `ign tags import` answered `404 not_found` at `/system/webdev/ign-cli/cli/tags`, because this host's module list has no WebDev module (`ign modules` lists Perspective, Vision, SFC, OPC UA drivers and others, and no `com.inductiveautomation.webdev`). So the nine imports below did not run on the gateway and the directory listings show only the provider's own `unary-resource.json`. These blocks are kept as the record of why the native route was used in the next section. The gateway-log block of this attempt held only unrelated startup errors (a missing audit profile `WIGNVM01_Audit`, stack traces) plus the capture token's client address in its mdc; it was removed from this file, which is the only edit made to this attempt.

`<H>` stands for exactly these ssh/sftp options, in this order (the `hardened_options` set with an identity): `-o BatchMode=yes -o StrictHostKeyChecking=yes -o UserKnownHostsFile=$HOME/.cache/ign-wflwvm01/known_hosts -o GlobalKnownHostsFile=/dev/null -o UpdateHostKeys=no -o VerifyHostKeyDNS=no -o PreferredAuthentications=publickey -o PasswordAuthentication=no -o KbdInteractiveAuthentication=no -o NumberOfPasswordPrompts=0 -o IdentitiesOnly=yes -o ForwardAgent=no -o ForwardX11=no -o ClearAllForwardings=yes -o PermitLocalCommand=no -o ControlMaster=no -o ControlPath=none -o ConnectTimeout=10 -o ServerAliveInterval=5 -o ServerAliveCountMax=3 -o LogLevel=ERROR `

#### ign webdev status (is the bundle there?)

`ign --profile azure-win --json webdev status ` @ 2026-10-15T02:45:10Z

~~~
[stdout]
[stderr]
{
  "ok": false,
  "profile": "azure-win",
  "error": {
    "code": "not_found",
    "message": "resource not found on the gateway",
    "endpoint": "http://40.114.71.134:8088/system/webdev/ign-cli/cli/tags",
    "hint": "check the id/path; a 404 JSON 'No route match' body can also mean a pre-8.3 gateway"
  }
}
[exit 6]
~~~

#### ign webdev deploy (the capture host has no bundle)

`ign --profile azure-win --json webdev deploy ` @ 2026-10-15T02:45:11Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "project": "ign-cli",
    "routes": [
      "tags",
      "tagConfig",
      "alarms",
      "tagHistory"
    ],
    "script_exec": false,
    "secret_rotated": false,
    "import": {
      "changes": [
        {
          "name": "ign-cli"
        }
      ],
      "success": true
    }
  }
}
[stderr]
[exit 0]
~~~

#### ign webdev status (after)

`ign --profile azure-win --json webdev status ` @ 2026-10-15T02:45:11Z

~~~
[stdout]
[stderr]
{
  "ok": false,
  "profile": "azure-win",
  "error": {
    "code": "not_found",
    "message": "resource not found on the gateway",
    "endpoint": "http://40.114.71.134:8088/system/webdev/ign-cli/cli/tags",
    "hint": "check the id/path; a 404 JSON 'No route match' body can also mean a pre-8.3 gateway"
  }
}
[exit 6]
~~~

#### ign tags provider create CAP22H

`ign --profile azure-win --json tags provider create CAP22H ` @ 2026-10-15T02:45:12Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "name": "CAP22H"
  }
}
[stderr]
[exit 0]
~~~

#### ign tags import: folder named 'plain' (5 chars)

`ign --profile azure-win --json tags import --provider CAP22H --file - ` @ 2026-10-15T02:45:12Z

~~~
[stdin]
[{"name":"plain","tagType":"Folder","tags":[{"name":"T1","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","value":1}]}]
[stdout]
[stderr]
{
  "ok": false,
  "profile": "azure-win",
  "error": {
    "code": "not_found",
    "message": "resource not found on the gateway",
    "endpoint": "http://40.114.71.134:8088/system/webdev/ign-cli/cli/tags",
    "hint": "check the id/path; a 404 JSON 'No route match' body can also mean a pre-8.3 gateway"
  }
}
[exit 6]
~~~

#### ign tags import: folder named 'CON' (3 chars)

`ign --profile azure-win --json tags import --provider CAP22H --file - ` @ 2026-10-15T02:45:12Z

~~~
[stdin]
[{"name":"CON","tagType":"Folder","tags":[{"name":"T1","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","value":1}]}]
[stdout]
[stderr]
{
  "ok": false,
  "profile": "azure-win",
  "error": {
    "code": "not_found",
    "message": "resource not found on the gateway",
    "endpoint": "http://40.114.71.134:8088/system/webdev/ign-cli/cli/tags",
    "hint": "check the id/path; a 404 JSON 'No route match' body can also mean a pre-8.3 gateway"
  }
}
[exit 6]
~~~

#### ign tags import: folder named 'aux' (3 chars)

`ign --profile azure-win --json tags import --provider CAP22H --file - ` @ 2026-10-15T02:45:13Z

~~~
[stdin]
[{"name":"aux","tagType":"Folder","tags":[{"name":"T1","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","value":1}]}]
[stdout]
[stderr]
{
  "ok": false,
  "profile": "azure-win",
  "error": {
    "code": "not_found",
    "message": "resource not found on the gateway",
    "endpoint": "http://40.114.71.134:8088/system/webdev/ign-cli/cli/tags",
    "hint": "check the id/path; a 404 JSON 'No route match' body can also mean a pre-8.3 gateway"
  }
}
[exit 6]
~~~

#### ign tags import: folder named 'NUL' (3 chars)

`ign --profile azure-win --json tags import --provider CAP22H --file - ` @ 2026-10-15T02:45:13Z

~~~
[stdin]
[{"name":"NUL","tagType":"Folder","tags":[{"name":"T1","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","value":1}]}]
[stdout]
[stderr]
{
  "ok": false,
  "profile": "azure-win",
  "error": {
    "code": "not_found",
    "message": "resource not found on the gateway",
    "endpoint": "http://40.114.71.134:8088/system/webdev/ign-cli/cli/tags",
    "hint": "check the id/path; a 404 JSON 'No route match' body can also mean a pre-8.3 gateway"
  }
}
[exit 6]
~~~

#### ign tags import: folder named 'trail ' (6 chars)

`ign --profile azure-win --json tags import --provider CAP22H --file - ` @ 2026-10-15T02:45:13Z

~~~
[stdin]
[{"name":"trail ","tagType":"Folder","tags":[{"name":"T1","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","value":1}]}]
[stdout]
[stderr]
{
  "ok": false,
  "profile": "azure-win",
  "error": {
    "code": "not_found",
    "message": "resource not found on the gateway",
    "endpoint": "http://40.114.71.134:8088/system/webdev/ign-cli/cli/tags",
    "hint": "check the id/path; a 404 JSON 'No route match' body can also mean a pre-8.3 gateway"
  }
}
[exit 6]
~~~

#### ign tags import: folder named 'trail.' (6 chars)

`ign --profile azure-win --json tags import --provider CAP22H --file - ` @ 2026-10-15T02:45:14Z

~~~
[stdin]
[{"name":"trail.","tagType":"Folder","tags":[{"name":"T1","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","value":1}]}]
[stdout]
[stderr]
{
  "ok": false,
  "profile": "azure-win",
  "error": {
    "code": "not_found",
    "message": "resource not found on the gateway",
    "endpoint": "http://40.114.71.134:8088/system/webdev/ign-cli/cli/tags",
    "hint": "check the id/path; a 404 JSON 'No route match' body can also mean a pre-8.3 gateway"
  }
}
[exit 6]
~~~

#### ign tags import: folder named 'a:b' (3 chars)

`ign --profile azure-win --json tags import --provider CAP22H --file - ` @ 2026-10-15T02:45:14Z

~~~
[stdin]
[{"name":"a:b","tagType":"Folder","tags":[{"name":"T1","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","value":1}]}]
[stdout]
[stderr]
{
  "ok": false,
  "profile": "azure-win",
  "error": {
    "code": "not_found",
    "message": "resource not found on the gateway",
    "endpoint": "http://40.114.71.134:8088/system/webdev/ign-cli/cli/tags",
    "hint": "check the id/path; a 404 JSON 'No route match' body can also mean a pre-8.3 gateway"
  }
}
[exit 6]
~~~

#### ign tags import: folder named 'LLLLLLLLLLLLLLLLLLLLLLLL' (200 chars)

`ign --profile azure-win --json tags import --provider CAP22H --file - ` @ 2026-10-15T02:45:14Z

~~~
[stdin]
[{"name":"LLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLL","tagType":"Folder","tags":[{"name":"T1","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","value":1}]}]
[stdout]
[stderr]
{
  "ok": false,
  "profile": "azure-win",
  "error": {
    "code": "not_found",
    "message": "resource not found on the gateway",
    "endpoint": "http://40.114.71.134:8088/system/webdev/ign-cli/cli/tags",
    "hint": "check the id/path; a 404 JSON 'No route match' body can also mean a pre-8.3 gateway"
  }
}
[exit 6]
~~~

#### ign tags import: folder named 'lpt1' (4 chars)

`ign --profile azure-win --json tags import --provider CAP22H --file - ` @ 2026-10-15T02:45:14Z

~~~
[stdin]
[{"name":"lpt1","tagType":"Folder","tags":[{"name":"T1","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","value":1}]}]
[stdout]
[stderr]
{
  "ok": false,
  "profile": "azure-win",
  "error": {
    "code": "not_found",
    "message": "resource not found on the gateway",
    "endpoint": "http://40.114.71.134:8088/system/webdev/ign-cli/cli/tags",
    "hint": "check the id/path; a 404 JSON 'No route match' body can also mean a pre-8.3 gateway"
  }
}
[exit 6]
~~~

Waiting 20 s for the gateway to flush the tag files (the flush is asynchronous, Phase 21 CD4).

#### sftp listing of the provider tag directory (ls -1)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:45:35Z

~~~
[stdin]
@ls -1 "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/CAP22H"
[stdout]
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/CAP22H/unary-resource.json
[stderr]
[exit 0]
~~~

#### sftp listing of the provider tag directory (ls -la)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:45:37Z

~~~
[stdin]
@ls -la "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/CAP22H"
[stdout]
-rw-******    1 -        -             135 Oct 15 02:45 unary-resource.json
[stderr]
[exit 0]
~~~

#### cmd.exe side: dir /b and dir /x (8.3 names) of the provider directory

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 dir\ /b\ \"C:\\Program\ Files\\Inductive\ Automation\\Ignition\\data\\config\\resources\\core\\ignition\\tag-definition\\CAP22H\"\ \&\ echo\ ----\ \&\ dir\ /x\ \"C:\\Program\ Files\\Inductive\ Automation\\Ignition\\data\\config\\resources\\core\\ignition\\tag-definition\\CAP22H\" ` @ 2026-10-15T02:45:39Z

~~~
[stdout]
unary-resource.json
---- 
 Volume in drive C is Windows
 Volume Serial Number is 6AEF-F68E

 Directory of C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H

10/15/2026  02:45 AM    <DIR>                       .
10/15/2026  02:45 AM    <DIR>                       ..
10/15/2026  02:45 AM               135 UNARY-~1.JSO unary-resource.json
               1 File(s)            135 bytes
               2 Dir(s)  116,338,507,776 bytes free
[12 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### PowerShell side: Get-ChildItem names with lengths and a hex dump of the first 24 characters of each name

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIA...\[1262\ chars\] ` @ 2026-10-15T02:45:40Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; Get-ChildItem -LiteralPath (Join-Path 'C:\Program Files\Inductive Automation\Ignition\data\config\resources' 'core\ignition\tag-definition\CAP22H') -Force | ForEach-Object { $n=$_.Name; $h=($n.Substring(0,[Math]::Min(24,$n.Length)).ToCharArray() | ForEach-Object { '{0:X4}' -f [int]$_ }) -join ' '; ('{0} chars [{1}] {2}' -f $n.Length,$h,$n.Substring(0,[Math]::Min(24,$n.Length))) }
[stdout]
19 chars [0075 006E 0061 0072 0079 002D 0072 0065 0073 006F 0075 0072 0063 0065 002E 006A 0073 006F 006E] unary-resource.json
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### sftp: the 200-character folder: contents

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:45:42Z

~~~
[stdin]
@ls -1 "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/CAP22H/LLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLL"
[stdout]
[stderr]
Can't ls: "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/CAP22H/LLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLL" not found
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### ign tags provider list (what the gateway thinks it holds)

`ign --profile azure-win --json tags provider list ` @ 2026-10-15T02:45:44Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "providers": [
      {
        "name": "CAP22H",
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
[stderr]
[exit 0]
~~~

#### ign tags provider delete CAP22H --yes

`ign --profile azure-win --json --yes tags provider delete CAP22H ` @ 2026-10-15T02:45:44Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "deleted": "CAP22H"
  }
}
[stderr]
[exit 0]
~~~

#### sftp listing after the provider delete (is the directory gone?)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:45:49Z

~~~
[stdin]
@ls -1 "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition"
[stdout]
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/System
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/default
[stderr]
[exit 0]
~~~

#### cmd.exe side after the delete: CAP22H still there?

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 dir\ /b\ \"C:\\Program\ Files\\Inductive\ Automation\\Ignition\\data\\config\\resources\\core\\ignition\\tag-definition\"\ \|\ findstr\ /i\ CAP22H ` @ 2026-10-15T02:45:51Z

~~~
[stdout]
[stderr]
[exit 1]
~~~



## C5-NTFS — on-disk names the 8.3.6 gateway writes on NTFS for hostile tag folder names (deferred here by 21-CONTEXT D-04)

Provider `CAP22H` is created on the capture gateway `azure-win`, nine folders are imported one by one through the gateway's NATIVE route `POST /data/api/v1/tags/import?provider=CAP22H&type=json&collisionPolicy=Abort` (via `ign api call`; this host has no WebDev module, so `ign tags import`, which rides the WebDev routes, cannot be used here, see attempt 1 above), the asynchronous flush is given 20 s, the provider's tag directories are listed over sftp as `svc-ignition-deploy` and from the cmd.exe side, the gateway log is read, and the provider is deleted. Render stays fail-closed on Windows-illegal names whatever this shows.

`<H>` stands for exactly these ssh/sftp options, in this order (the `hardened_options` set with an identity): `-o BatchMode=yes -o StrictHostKeyChecking=yes -o UserKnownHostsFile=$HOME/.cache/ign-wflwvm01/known_hosts -o GlobalKnownHostsFile=/dev/null -o UpdateHostKeys=no -o VerifyHostKeyDNS=no -o PreferredAuthentications=publickey -o PasswordAuthentication=no -o KbdInteractiveAuthentication=no -o NumberOfPasswordPrompts=0 -o IdentitiesOnly=yes -o ForwardAgent=no -o ForwardX11=no -o ClearAllForwardings=yes -o PermitLocalCommand=no -o ControlMaster=no -o ControlPath=none -o ConnectTimeout=10 -o ServerAliveInterval=5 -o ServerAliveCountMax=3 -o LogLevel=ERROR `

#### ign webdev status (is the WebDev module and the ign-cli bundle there?)

`ign --profile azure-win --json webdev status ` @ 2026-10-15T02:48:28Z

~~~
[stdout]
[stderr]
{
  "ok": false,
  "profile": "azure-win",
  "error": {
    "code": "not_found",
    "message": "resource not found on the gateway",
    "endpoint": "http://40.114.71.134:8088/system/webdev/ign-cli/cli/tags",
    "hint": "check the id/path; a 404 JSON 'No route match' body can also mean a pre-8.3 gateway"
  }
}
[exit 6]
~~~

#### ign tags provider create CAP22H

`ign --profile azure-win --json tags provider create CAP22H ` @ 2026-10-15T02:48:28Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "name": "CAP22H"
  }
}
[stderr]
[exit 0]
~~~

#### native tags/import: folder named 'plain' (5 chars)

`ign --profile azure-win --json api call --method POST --path /data/api/v1/tags/import --query provider=CAP22H --query type=json --query collisionPolicy=Abort --header Content-Type:\ application/octet-stream --data \{\"tags\":\[\{\"name\":\"plain\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\]\} ` @ 2026-10-15T02:48:28Z

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

#### native tags/import: folder named 'CON' (3 chars)

`ign --profile azure-win --json api call --method POST --path /data/api/v1/tags/import --query provider=CAP22H --query type=json --query collisionPolicy=Abort --header Content-Type:\ application/octet-stream --data \{\"tags\":\[\{\"name\":\"CON\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\]\} ` @ 2026-10-15T02:48:29Z

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

#### native tags/import: folder named 'aux' (3 chars)

`ign --profile azure-win --json api call --method POST --path /data/api/v1/tags/import --query provider=CAP22H --query type=json --query collisionPolicy=Abort --header Content-Type:\ application/octet-stream --data \{\"tags\":\[\{\"name\":\"aux\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\]\} ` @ 2026-10-15T02:48:29Z

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

#### native tags/import: folder named 'NUL' (3 chars)

`ign --profile azure-win --json api call --method POST --path /data/api/v1/tags/import --query provider=CAP22H --query type=json --query collisionPolicy=Abort --header Content-Type:\ application/octet-stream --data \{\"tags\":\[\{\"name\":\"NUL\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\]\} ` @ 2026-10-15T02:48:29Z

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

#### native tags/import: folder named 'trail ' (6 chars)

`ign --profile azure-win --json api call --method POST --path /data/api/v1/tags/import --query provider=CAP22H --query type=json --query collisionPolicy=Abort --header Content-Type:\ application/octet-stream --data \{\"tags\":\[\{\"name\":\"trail\ \"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\]\} ` @ 2026-10-15T02:48:29Z

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

#### native tags/import: folder named 'trail.' (6 chars)

`ign --profile azure-win --json api call --method POST --path /data/api/v1/tags/import --query provider=CAP22H --query type=json --query collisionPolicy=Abort --header Content-Type:\ application/octet-stream --data \{\"tags\":\[\{\"name\":\"trail.\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\]\} ` @ 2026-10-15T02:48:30Z

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

#### native tags/import: folder named 'a:b' (3 chars)

`ign --profile azure-win --json api call --method POST --path /data/api/v1/tags/import --query provider=CAP22H --query type=json --query collisionPolicy=Abort --header Content-Type:\ application/octet-stream --data \{\"tags\":\[\{\"name\":\"a:b\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\]\} ` @ 2026-10-15T02:48:30Z

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

#### native tags/import: folder named 'LLLLLLLLLLLLLLLLLLLLLLLL' (200 chars)

`ign --profile azure-win --json api call --method POST --path /data/api/v1/tags/import --query provider=CAP22H --query type=json --query collisionPolicy=Abort --header Content-Type:\ application/octet-stream --data \{\"tags\":\[\{\"name\":\"LLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLL\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\]\} ` @ 2026-10-15T02:48:30Z

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

#### native tags/import: folder named 'lpt1' (4 chars)

`ign --profile azure-win --json api call --method POST --path /data/api/v1/tags/import --query provider=CAP22H --query type=json --query collisionPolicy=Abort --header Content-Type:\ application/octet-stream --data \{\"tags\":\[\{\"name\":\"lpt1\"\,\"tagType\":\"Folder\"\,\"tags\":\[\{\"name\":\"T1\"\,\"tagType\":\"AtomicTag\"\,\"dataType\":\"Int4\"\,\"valueSource\":\"memory\"\,\"value\":1\}\]\}\]\} ` @ 2026-10-15T02:48:31Z

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

Waiting 20 s for the gateway to flush the tag files (the flush is asynchronous, Phase 21 CD4).

#### sftp listing of the provider tag directory (ls -1)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:48:51Z

~~~
[stdin]
@ls -1 "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/CAP22H"
[stdout]
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/CAP22H/%43%4f%4e
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/CAP22H/%4e%55%4c
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/CAP22H/%61%75%78
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/CAP22H/%6c%70%74%31
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/CAP22H/a%3ab
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/CAP22H/plain
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/CAP22H/trail%20
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/CAP22H/trail%2E
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/CAP22H/unary-resource.json
[stderr]
[exit 0]
~~~

#### sftp listing of the provider tag directory (ls -la)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:48:53Z

~~~
[stdin]
@ls -la "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/CAP22H"
[stdout]
drwx******    1 -        -               0 Oct 15 02:48 %43%4f%4e
drwx******    1 -        -               0 Oct 15 02:48 %4e%55%4c
drwx******    1 -        -               0 Oct 15 02:48 %61%75%78
drwx******    1 -        -               0 Oct 15 02:48 %6c%70%74%31
drwx******    1 -        -               0 Oct 15 02:48 a%3ab
drwx******    1 -        -               0 Oct 15 02:48 plain
drwx******    1 -        -               0 Oct 15 02:48 trail%20
drwx******    1 -        -               0 Oct 15 02:48 trail%2E
-rw-******    1 -        -             135 Oct 15 02:48 unary-resource.json
[stderr]
[exit 0]
~~~

#### cmd.exe side: dir /x (8.3 names) of the provider directory and the folders inside it

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 dir\ /x\ \"C:\\Program\ Files\\Inductive\ Automation\\Ignition\\data\\config\\resources\\core\\ignition\\tag-definition\\CAP22H\"\ \&\ dir\ /b\ /s\ \"C:\\Program\ Files\\Inductive\ Automation\\Ignition\\data\\config\\resources\\core\\ignition\\tag-definition\\CAP22H\" ` @ 2026-10-15T02:48:55Z

~~~
[stdout]
 Volume in drive C is Windows
 Volume Serial Number is 6AEF-F68E

 Directory of C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H

10/15/2026  02:48 AM    <DIR>                       .
10/15/2026  02:48 AM    <DIR>                       ..
10/15/2026  02:48 AM    <DIR>          %43%4F~1     %43%4f%4e
10/15/2026  02:48 AM    <DIR>          %4E%55~1     %4e%55%4c
10/15/2026  02:48 AM    <DIR>          %61%75~1     %61%75%78
10/15/2026  02:48 AM    <DIR>          %6C%70~1     %6c%70%74%31
10/15/2026  02:48 AM    <DIR>                       a%3ab
10/15/2026  02:48 AM    <DIR>          LLLLLL~1     LLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLL
10/15/2026  02:48 AM    <DIR>                       plain
10/15/2026  02:48 AM    <DIR>                       trail%20
10/15/2026  02:48 AM    <DIR>                       trail%2E
10/15/2026  02:48 AM               135 UNARY-~1.JSO unary-resource.json
               1 File(s)            135 bytes
              11 Dir(s)  116,335,632,384 bytes free
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\%43%4f%4e
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\%4e%55%4c
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\%61%75%78
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\%6c%70%74%31
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\a%3ab
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\LLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLL
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\plain
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\trail%20
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\trail%2E
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\unary-resource.json
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\%43%4f%4e\tags.json
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\%43%4f%4e\unary-resource.json
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\%4e%55%4c\tags.json
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\%4e%55%4c\unary-resource.json
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\%61%75%78\tags.json
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\%61%75%78\unary-resource.json
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\%6c%70%74%31\tags.json
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\%6c%70%74%31\unary-resource.json
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\a%3ab\tags.json
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\a%3ab\unary-resource.json
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\plain\tags.json
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\plain\unary-resource.json
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\trail%20\tags.json
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\trail%20\unary-resource.json
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\trail%2E\tags.json
C:\Program Files\Inductive Automation\Ignition\data\config\resources\core\ignition\tag-definition\CAP22H\trail%2E\unary-resource.json
[45 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### PowerShell side: every entry under the provider directory with its name length and a hex dump of the first 24 characters of the name

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 powershell\ -NoProfile\ -NonInteractive\ -EncodedCommand\ JABQAHIAbwBnAHIAZQBzAHMAUAByAGUAZgBlAHIAZQBuAGMAZQA9ACcAUwBpAGwAZQBuAHQAbAB5AEMAbwBuAHQAaQBuAHUAZQAnADsAIA...\[1286\ chars\] ` @ 2026-10-15T02:48:56Z

~~~
[script]
$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; Get-ChildItem -LiteralPath (Join-Path 'C:\Program Files\Inductive Automation\Ignition\data\config\resources' 'core\ignition\tag-definition\CAP22H') -Recurse -Force | ForEach-Object { $n=$_.Name; $h=($n.Substring(0,[Math]::Min(24,$n.Length)).ToCharArray() | ForEach-Object { '{0:X4}' -f [int]$_ }) -join ' '; ('{0} chars [{1}] {2}' -f $n.Length,$h,$n.Substring(0,[Math]::Min(24,$n.Length))) }
[stdout]
9 chars [0025 0034 0033 0025 0034 0066 0025 0034 0065] %43%4f%4e
9 chars [0025 0034 0065 0025 0035 0035 0025 0034 0063] %4e%55%4c
9 chars [0025 0036 0031 0025 0037 0035 0025 0037 0038] %61%75%78
12 chars [0025 0036 0063 0025 0037 0030 0025 0037 0034 0025 0033 0031] %6c%70%74%31
5 chars [0061 0025 0033 0061 0062] a%3ab
200 chars [004C 004C 004C 004C 004C 004C 004C 004C 004C 004C 004C 004C 004C 004C 004C 004C 004C 004C 004C 004C 004C 004C 004C 004C] LLLLLLLLLLLLLLLLLLLLLLLL
5 chars [0070 006C 0061 0069 006E] plain
8 chars [0074 0072 0061 0069 006C 0025 0032 0030] trail%20
8 chars [0074 0072 0061 0069 006C 0025 0032 0045] trail%2E
19 chars [0075 006E 0061 0072 0079 002D 0072 0065 0073 006F 0075 0072 0063 0065 002E 006A 0073 006F 006E] unary-resource.json
9 chars [0074 0061 0067 0073 002E 006A 0073 006F 006E] tags.json
19 chars [0075 006E 0061 0072 0079 002D 0072 0065 0073 006F 0075 0072 0063 0065 002E 006A 0073 006F 006E] unary-resource.json
9 chars [0074 0061 0067 0073 002E 006A 0073 006F 006E] tags.json
19 chars [0075 006E 0061 0072 0079 002D 0072 0065 0073 006F 0075 0072 0063 0065 002E 006A 0073 006F 006E] unary-resource.json
9 chars [0074 0061 0067 0073 002E 006A 0073 006F 006E] tags.json
19 chars [0075 006E 0061 0072 0079 002D 0072 0065 0073 006F 0075 0072 0063 0065 002E 006A 0073 006F 006E] unary-resource.json
9 chars [0074 0061 0067 0073 002E 006A 0073 006F 006E] tags.json
19 chars [0075 006E 0061 0072 0079 002D 0072 0065 0073 006F 0075 0072 0063 0065 002E 006A 0073 006F 006E] unary-resource.json
9 chars [0074 0061 0067 0073 002E 006A 0073 006F 006E] tags.json
19 chars [0075 006E 0061 0072 0079 002D 0072 0065 0073 006F 0075 0072 0063 0065 002E 006A 0073 006F 006E] unary-resource.json
[20 CR bytes removed from stdout]
[stderr]
#< CLIXML
<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04"><S S="Error">Get-ChildItem : Could not find a part of the path 'C:\Program Files\Inductive Automation\Ignition\data\config\resources_x000D__x000A_</S><S S="Error">\core\ignition\tag-definition\CAP22H\LLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLL_x000D__x000A_</S><S S="Error">LLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLL'_x000D__x000A_</S><S S="Error">._x000D__x000A_</S><S S="Error">At line:1 char:72_x000D__x000A_</S><S S="Error">+ ... nce='Stop'; Get-ChildItem -LiteralPath (Join-Path 'C:\Program Files\I ..._x000D__x000A_</S><S S="Error">+                 ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~_x000D__x000A_</S><S S="Error">    + CategoryInfo          : ReadError: (C:\Program File...LLLLLLLLLLLLLLL:String) [Get-ChildItem], DirectoryNotFound _x000D__x000A_</S><S S="Error">   Exception_x000D__x000A_</S><S S="Error">    + FullyQualifiedErrorId : DirIOError,Microsoft.PowerShell.Commands.GetChildItemCommand_x000D__x000A_</S><S S="Error"> _x000D__x000A_</S></Objs>
[stderr did not end with a newline]
[1 CR bytes removed from stderr]
[exit 1]
~~~

#### gateway log, WARN and above in the last 4 minutes (timestamp, logger, level, message only; stacks and mdc dropped by jq)

`bash -c \"\$1\"\ --profile\ \"\$2\"\ --json\ logs\ --min-level\ warn\ --since\ 4min\ --limit\ 200\ \|\ jq\ -c\ \".data.items\[\]\ \|\ \{timestamp\,\ loggerName\,\ level\,\ message\}\" _ ign azure-win ` @ 2026-10-15T02:48:58Z

~~~
[stdout]
{"timestamp":1792032512526,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032512526,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032512236,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032512235,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032511922,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032511922,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032511438,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032511438,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032511438,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032511438,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032511340,"loggerName":"tags.management.provider","level":"ERROR","message":"Error processing edit for tag path '[CAP22H]trail.': Error_Configuration(\"The name 'trail.' is not a valid tag name\")"}
{"timestamp":1792032510844,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032510844,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032510844,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032510844,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032510509,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032510508,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032510213,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032510213,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032509793,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032509755,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032509714,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032509711,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032509711,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032509688,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032509688,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032509663,"loggerName":"com.inductiveautomation.ignition.gateway.tags.ep.StandardTagProviderExtensionPoint","level":"ERROR","message":"valuePersistence not found in Tag Provider: 'CAP22H' profile, defaulting to Database"}
{"timestamp":1792032509650,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032489655,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032489654,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032489638,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032489637,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032489634,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032489634,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032489634,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032489616,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032460428,"loggerName":"tags.management.provider","level":"ERROR","message":"Error importing tags"}
{"timestamp":1792032460084,"loggerName":"tags.management.provider","level":"ERROR","message":"Error importing tags"}
{"timestamp":1792032459806,"loggerName":"tags.management.provider","level":"ERROR","message":"Error importing tags"}
{"timestamp":1792032459515,"loggerName":"tags.management.provider","level":"ERROR","message":"Error importing tags"}
{"timestamp":1792032459274,"loggerName":"tags.management.provider","level":"ERROR","message":"Error importing tags"}
{"timestamp":1792032458995,"loggerName":"tags.management.provider","level":"ERROR","message":"Error importing tags"}
{"timestamp":1792032458742,"loggerName":"tags.management.provider","level":"ERROR","message":"Error importing tags"}
{"timestamp":1792032458471,"loggerName":"tags.management.provider","level":"ERROR","message":"Error importing tags"}
{"timestamp":1792032458213,"loggerName":"tags.management.provider","level":"ERROR","message":"Error importing tags"}
{"timestamp":1792032458067,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032458027,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032457980,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032457973,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032457973,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032457944,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032457944,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032457917,"loggerName":"com.inductiveautomation.ignition.gateway.tags.ep.StandardTagProviderExtensionPoint","level":"ERROR","message":"valuePersistence not found in Tag Provider: 'CAP22H' profile, defaulting to Database"}
{"timestamp":1792032457908,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032345810,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032345810,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032345804,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032345804,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032345800,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032345800,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032345800,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032345766,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032313472,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032313430,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032313377,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032313374,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032313373,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032313350,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032313350,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032313323,"loggerName":"com.inductiveautomation.ignition.gateway.tags.ep.StandardTagProviderExtensionPoint","level":"ERROR","message":"valuePersistence not found in Tag Provider: 'CAP22H' profile, defaulting to Database"}
{"timestamp":1792032313287,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032312282,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
{"timestamp":1792032312282,"loggerName":"AuditManager","level":"ERROR","message":"Error getting gateway audit profile 'WIGNVM01_Audit'"}
[stderr]
[exit 0]
~~~

#### ign tags provider list (what the gateway thinks it holds)

`ign --profile azure-win --json tags provider list ` @ 2026-10-15T02:48:58Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "providers": [
      {
        "name": "CAP22H",
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
[stderr]
[exit 0]
~~~

#### ign tags provider delete CAP22H --yes

`ign --profile azure-win --json --yes tags provider delete CAP22H ` @ 2026-10-15T02:48:59Z

~~~
[stdout]
{
  "ok": true,
  "profile": "azure-win",
  "data": {
    "deleted": "CAP22H"
  }
}
[stderr]
[exit 0]
~~~

#### sftp listing after the provider delete (is the directory gone?)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T02:49:04Z

~~~
[stdin]
@ls -1 "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition"
[stdout]
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/System
/C:/Program Files/Inductive Automation/Ignition/data/config/resources/core/ignition/tag-definition/default
[stderr]
[exit 0]
~~~

#### cmd.exe side after the delete: CAP22H still there?

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 dir\ /b\ \"C:\\Program\ Files\\Inductive\ Automation\\Ignition\\data\\config\\resources\\core\\ignition\\tag-definition\"\ \|\ findstr\ /i\ CAP22H ` @ 2026-10-15T02:49:07Z

~~~
[stdout]
[stderr]
[exit 1]
~~~


## C6-post — after the post-C6 grant on production (2026-10-15)

POST-C6 PROVISIONING CHANGE. After the C6 section above was captured (and after Decision B was recorded), the USER ran, on the capture host, `icacls "C:\Program Files\Inductive Automation\Ignition\data\config\resources\production" /grant "ignwin836\svc-ignition-deploy:(OI)(CI)(M)"` on 2026-10-15 (output: "Successfully processed 1 files"). The C6 answers above keep describing the account as originally provisioned; this section describes it after the grant. This driver made no ACL change. Probes below run as `svc-ignition-deploy` over ssh and sftp: put a file straight into `production`, rename it there, rename a staged file from the home into it, remove it; the scan state is read before and after.

`<H>` stands for exactly these ssh/sftp options, in this order (the `hardened_options` set with an identity): `-o BatchMode=yes -o StrictHostKeyChecking=yes -o UserKnownHostsFile=$HOME/.cache/ign-wflwvm01/known_hosts -o GlobalKnownHostsFile=/dev/null -o UpdateHostKeys=no -o VerifyHostKeyDNS=no -o PreferredAuthentications=publickey -o PasswordAuthentication=no -o KbdInteractiveAuthentication=no -o NumberOfPasswordPrompts=0 -o IdentitiesOnly=yes -o ForwardAgent=no -o ForwardX11=no -o ClearAllForwardings=yes -o PermitLocalCommand=no -o ControlMaster=no -o ControlPath=none -o ConnectTimeout=10 -o ServerAliveInterval=5 -o ServerAliveCountMax=3 -o LogLevel=ERROR `

### C6-post-acl — the ACL after the grant

#### icacls data\\config\\resources\\production (after the grant)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 icacls\ \"C:\\Program\ Files\\Inductive\ Automation\\Ignition\\data\\config\\resources\\production\" ` @ 2026-10-15T11:16:23Z

~~~
[stdout]
C:\Program Files\Inductive Automation\Ignition\data\config\resources\production ignwin836\svc-ignition-deploy:(OI)(CI)(M)
                                                                                ignwin836\svc-ignition-deploy:(I)(OI)(CI)(RX)
                                                                                NT SERVICE\TrustedInstaller:(I)(F)
                                                                                NT SERVICE\TrustedInstaller:(I)(CI)(IO)(F)
                                                                                NT AUTHORITY\SYSTEM:(I)(F)
                                                                                NT AUTHORITY\SYSTEM:(I)(OI)(CI)(IO)(F)
                                                                                BUILTIN\Administrators:(I)(F)
                                                                                BUILTIN\Administrators:(I)(OI)(CI)(IO)(F)
                                                                                BUILTIN\Users:(I)(RX)
                                                                                BUILTIN\Users:(I)(OI)(CI)(IO)(GR,GE)
                                                                                CREATOR OWNER:(I)(OI)(CI)(IO)(F)
                                                                                APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:(I)(RX)
                                                                                APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:(I)(OI)(CI)(IO)(GR,GE)
                                                                                APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:(I)(RX)
                                                                                APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:(I)(OI)(CI)(IO)(GR,GE)

Successfully processed 1 files; Failed processing 0 files
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

#### icacls data\\config\\resources (the grant must be on production only)

`ssh -T -p 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 icacls\ \"C:\\Program\ Files\\Inductive\ Automation\\Ignition\\data\\config\\resources\" ` @ 2026-10-15T11:16:25Z

~~~
[stdout]
C:\Program Files\Inductive Automation\Ignition\data\config\resources ignwin836\svc-ignition-deploy:(OI)(CI)(RX)
                                                                     NT SERVICE\TrustedInstaller:(I)(F)
                                                                     NT SERVICE\TrustedInstaller:(I)(CI)(IO)(F)
                                                                     NT AUTHORITY\SYSTEM:(I)(F)
                                                                     NT AUTHORITY\SYSTEM:(I)(OI)(CI)(IO)(F)
                                                                     BUILTIN\Administrators:(I)(F)
                                                                     BUILTIN\Administrators:(I)(OI)(CI)(IO)(F)
                                                                     BUILTIN\Users:(I)(RX)
                                                                     BUILTIN\Users:(I)(OI)(CI)(IO)(GR,GE)
                                                                     CREATOR OWNER:(I)(OI)(CI)(IO)(F)
                                                                     APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:(I)(RX)
                                                                     APPLICATION PACKAGE AUTHORITY\ALL APPLICATION PACKAGES:(I)(OI)(CI)(IO)(GR,GE)
                                                                     APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:(I)(RX)
                                                                     APPLICATION PACKAGE AUTHORITY\ALL RESTRICTED APPLICATION PACKAGES:(I)(OI)(CI)(IO)(GR,GE)

Successfully processed 1 files; Failed processing 0 files
[1 CR bytes removed from stdout]
[stderr]
[exit 0]
~~~

### C6-post-scan-before

#### GET scan/config (before)

`ign --profile azure-win --json api call --method GET --path /data/api/v1/scan/config ` @ 2026-10-15T11:16:26Z

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
  "lastScanTimestamp": 1792062546739,
  "lastScanDuration": 1565
}
    }
  }
}
[stderr]
[exit 0]
~~~

### C6-post-1 — put, rename, rm directly in production

#### ls -la of production before

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T11:16:26Z

~~~
[stdin]
@ls -la "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production"
[stdout]
-rw-******    1 -        -             133 Oct 15 02:08 config-mode.json
[stderr]
[exit 0]
~~~

#### put a probe file straight into production

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T11:16:28Z

~~~
[stdin]
put "$TMP/local/a.txt" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6post-probe.txt"
[stdout]
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.S44hU6HxXk/local/a.txt" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6post-probe.txt"
[stderr]
[exit 0]
~~~

#### rename it inside production (replaces nothing)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T11:16:30Z

~~~
[stdin]
rename "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6post-probe.txt" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6post-probe2.txt"
[stdout]
sftp> rename "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6post-probe.txt" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6post-probe2.txt"
[stderr]
[exit 0]
~~~

#### stage a file in the home and rename it into production OVER the existing probe (the B(i) commit step, a replace)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T11:16:31Z

~~~
[stdin]
mkdir "ign-deploy-c6post"
put "$TMP/local/b.txt" "ign-deploy-c6post/staged.txt"
rename "ign-deploy-c6post/staged.txt" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6post-probe2.txt"
[stdout]
sftp> mkdir "ign-deploy-c6post"
sftp> put "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.S44hU6HxXk/local/b.txt" "ign-deploy-c6post/staged.txt"
sftp> rename "ign-deploy-c6post/staged.txt" "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6post-probe2.txt"
[stderr]
[exit 0]
~~~

#### get the replaced probe back (bytes must be bravo)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T11:16:33Z

~~~
[stdin]
get "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6post-probe2.txt" "$TMP/local/back.txt"
[stdout]
sftp> get "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6post-probe2.txt" "/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.S44hU6HxXk/local/back.txt"
[stderr]
[exit 0]
~~~

#### what the get-back holds locally

`bash -c cat\ \"\$1\"\;\ shasum\ -a\ 256\ \"\$1\" _ $TMP/local/back.txt ` @ 2026-10-15T11:16:35Z

~~~
[stdout]
bravo
5da8f23decf397b13f4f55b6fb8a61936238bfe08ed9d901132974f1beccc45c  /var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/tmp.S44hU6HxXk/local/back.txt
[stderr]
[exit 0]
~~~

#### mkdir and rmdir a probe directory inside production

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T11:16:35Z

~~~
[stdin]
mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6post-dir"
rmdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6post-dir"
[stdout]
sftp> mkdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6post-dir"
sftp> rmdir "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6post-dir"
[stderr]
[exit 0]
~~~

#### rm the probe file

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T11:16:37Z

~~~
[stdin]
rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6post-probe2.txt"
[stdout]
sftp> rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6post-probe2.txt"
[stderr]
[exit 0]
~~~

### C6-post-scan-after

#### GET scan/config (after)

`ign --profile azure-win --json api call --method GET --path /data/api/v1/scan/config ` @ 2026-10-15T11:16:38Z

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
  "lastScanTimestamp": 1792062546739,
  "lastScanDuration": 1565
}
    }
  }
}
[stderr]
[exit 0]
~~~

### C6-post-cleanup

#### remove the home staging directory

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T11:16:39Z

~~~
[stdin]
rmdir "ign-deploy-c6post"
[stdout]
sftp> rmdir "ign-deploy-c6post"
[stderr]
[exit 0]
~~~

#### production after the cleanup (config-mode.json only)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T11:16:40Z

~~~
[stdin]
@ls -la "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production"
[stdout]
-rw-******    1 -        -             133 Oct 15 02:08 config-mode.json
[stderr]
[exit 0]
~~~

#### defensive removal of the first probe name (fails with no such file when the rename moved it)

`sftp -q -b - -P 22 -i <key> <H> -- svc-ignition-deploy@40.114.71.134 ` @ 2026-10-15T11:16:42Z

~~~
[stdin]
rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6post-probe.txt"
[stdout]
sftp> rm "/C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6post-probe.txt"
[stderr]
remote delete /C:/Program Files/Inductive Automation/Ignition/data/config/resources/production/zz-c6post-probe.txt: No such file or directory
[1 CR bytes removed from stderr]
[exit 1]
~~~

**C6-post result (2026-10-15).** With the post-C6 grant, `svc-ignition-deploy` can: `put` a file straight into `production\`, `rename` inside it, `rename` a staged file from its home onto an existing file there (the target then held the staged bytes, sha256 `5da8f23d...`, "bravo"), `mkdir` and `rmdir` a directory in it, and `rm` its own file. The grant is on `production` only: `resources` still shows `svc-ignition-deploy:(OI)(CI)(RX)`. The scan state (`lastScanTimestamp 1792062546739`, `lastScanDuration 1565`, `scanActive false`) is identical before and after. Everything written was removed in the same session; `production` lists `config-mode.json` (133 bytes) only. The last block's exit 1 is the expected "No such file" for the first probe name, which the rename had already moved. This answers the "what C6 does NOT yet settle" caveat of the C6 table for `production` and for Decision B caveat 2 (`Modify` on the mode collection); the C6 table above is the original provisioning and is deliberately not rewritten.


## OQ5 run A (Defender exclusion off)

Capture-only harness `oq5_timings` in `crates/ignition-core/tests/live_transport_wflwvm01.rs` (`#[ignore]`, drives `mirror_for_node` stage and commit, `SftpTransport::read` and the gateway scan through the profile's `DeployApi`; no verb; the write half has no other caller). Host: the Azure VM `ign-win-836` (40.114.71.134), Windows Server 2022, Win32-OpenSSH 8.1, Ignition 8.3.6, as `svc-ignition-deploy` with the pinned host key, from a macOS runner over the public internet. Defender real-time protection ON, no exclusions (the state C7a, C6 and 22-13 were taken in).

Command (key and host from `~/.config/ign-azure-win/env`, mapped to `IGN_WFLWVM01_*`):

```
IGNITION_LIVE_WFLWVM01=1 IGNITION_LIVE_OQ5=1 IGN_OQ5_LABEL=A IGN_OQ5_SCALE=1 \
  cargo test -p ignition-core --test live_transport_wflwvm01 oq5_timings -- --ignored --nocapture
```

Started 2026-10-15T12:28:17Z, ended 2026-10-15T14:48:56Z (8434 s). Defaults of the harness: 10 iterations, 20 reads, 20 connect samples, per-call timeout of the harness target `op_timeout_s: 1800` (so a slow call is measured instead of killed; the product default is 120).

### Payload (D-16) and its limit

- Source: the OQ5 source is `none` (22-CONTEXT A-03), so the payload is the sanitized corpus in `tests/fixtures/deploy/corpus`.
- This run used **scale 1, not the plan's ten**: 70 named units, **152 files, 91,833 bytes** (the corpus has 195 files; the 42 singleton-unit files and `config-mode.json` are never written, because a copy of a singleton would override the live gateway's own settings in the `production` collection). Each unit goes in under `<name>-oq5-01`, with its `uuid` values re-derived. The unit names follow the corpus (27 module/type directories).
- Why not scale 10: the first calibration run (scale 1, one iteration) measured stage 77 s, commit 242 s and delete 118 s for these 152 files. Those costs are per file (about 0.5, 1.6 and 0.75 s per file), so ten copies (1,520 files) would take roughly 75 minutes per iteration and 12 hours for ten iterations, on one run, twice. The sample counts the plan requires (10 write iterations, 20 reads) were kept, the payload was not scaled; the per-file cost is stated so the 1,520-file figure can be extrapolated, and is NOT measured here.
- An earlier calibration run (scale 1, 1 iteration, 1 read, cut off by the shell's 580 s timeout during the read phase) left empty module/type directories in `production`; the restore-only mode of the same harness (`IGN_OQ5_RESTORE_ONLY`) removed them and asserted the listing equal to the recorded one. Its output is not part of the samples below.

### Gateway state during the run

- License: the trial had expired at the end of 22-13. Before the run `ign status` showed `trial_remaining_s: 0`; a manual `POST /data/api/v1/scan/config` on the expired trial completed normally (`lastScanDuration` 607 ms), so scans work the same on an expired trial.
- The coordinator reset the trial at 2026-10-15T12:47:31Z (`lastUpdated` 1792068451572; `trialSecondsLeft` 7161 at 12:48:07Z). Iterations 1 and 2 (and the start of 3) therefore ran on the expired trial, the rest on the live trial until it expired again at about 14:47Z (`trialSecondsLeft` 107 at 14:45:42Z, 0 and `expired: true` at 14:49Z). The restore scan ran after that expiry. No trial reset was made by the agent. The scan numbers show no step at the reset (gateway-side `lastScanDuration` 539 to 676 ms with the payload present in every iteration).
- No scan was ever refused or slow; the gateway-side `lastScanDuration` and the wall time from the trigger to the advanced timestamp (polled every 100 ms) are both recorded.

### Summary (nearest-rank percentiles, seconds; `_ms` rows are the gateway's `lastScanDuration` in milliseconds)

| Phase | n | p50 | p95 | min | max |
|---|---|---|---|---|---|
| connect (`sftp` session running `pwd`) | 20 | 1.575 | 1.688 | 1.450 | 1.749 |
| read of the scoped collection, payload present (46 core + 152 mode files, 117,914 bytes) | 20 | 164.120 | 181.155 | 154.791 | 186.683 |
| stage (152 files into the B(i) staging in the account home) | 10 | 78.012 | 83.265 | 72.676 | 83.265 |
| commit (rename into `production`, empty delete set) | 10 | 244.594 | 251.063 | 233.932 | 251.063 |
| scan, wall, payload present | 10 | 0.792 | 0.888 | 0.687 | 0.888 |
| scan, gateway `lastScanDuration`, payload present (ms) | 10 | 601 | 676 | 539 | 676 |
| commit of the delete set (152 files, 70 emptied unit directories removed) | 10 | 108.702 | 114.182 | 103.582 | 114.182 |
| scan, wall, after delete | 10 | 0.680 | 0.981 | 0.513 | 0.981 |
| scan, gateway `lastScanDuration`, after delete (ms) | 10 | 453 | 489 | 420 | 489 |

With n = 10 the nearest-rank p95 is the maximum; with n = 20 it is the 19th value.

### Restore (T-22-59)

After the loop the harness enumerated `production`, removed every file that was not in the pre-run listing through the delete set (152 files, 108 s), scanned, and removed the empty `<module>` directories that the delete-set commit leaves behind (it removes the emptied unit directory only; 8 module directories, each holding emptied type directories, remained; they were removed with the test-local `Remove-Item` script, not by the product). Then it downloaded and hashed everything in `production` again: `production_listing_equals_pre_run: true` (1 entry, `config-mode.json`, sha256 `f7b5b709abfadf5ad7338bd40ca14ebe4b4d123f4b2689112fa4cca36161b2fa`, 133 bytes). `ign-deploy` is absent from the account home. Checked again over ssh after the run: `dir /b production` lists `config-mode.json` only and `dir /b %USERPROFILE% | findstr /i ign-` prints nothing.

### Raw output

~~~
OQ5 {"connect_samples":20,"iters":10,"label":"A","op_timeout_s":1800,"payload":{"bytes":91833,"files":152,"source":"sanitized corpus (183 .json, 195 files) x 1, named units only","types":27,"units":70},"reads":20}
OQ5 {"gateway_scan_state_before":{"lastScanDuration":594,"lastScanTimestamp":1792066711645,"scanActive":false},"label":"A"}
OQ5 {"label":"A","pre_run_listing":[{"dir":false,"path":"config-mode.json","sha256":"f7b5b709abfadf5ad7338bd40ca14ebe4b4d123f4b2689112fa4cca36161b2fa","size":133}]}
OQ5 {"extra":{},"iter":1,"label":"A","phase":"connect_pwd","value":1.516}
OQ5 {"extra":{},"iter":2,"label":"A","phase":"connect_pwd","value":1.618}
test oq5_timings has been running for over 60 seconds
OQ5 {"extra":{},"iter":3,"label":"A","phase":"connect_pwd","value":1.65}
OQ5 {"extra":{},"iter":4,"label":"A","phase":"connect_pwd","value":1.568}
OQ5 {"extra":{},"iter":5,"label":"A","phase":"connect_pwd","value":1.502}
OQ5 {"extra":{},"iter":6,"label":"A","phase":"connect_pwd","value":1.651}
OQ5 {"extra":{},"iter":7,"label":"A","phase":"connect_pwd","value":1.568}
OQ5 {"extra":{},"iter":8,"label":"A","phase":"connect_pwd","value":1.655}
OQ5 {"extra":{},"iter":9,"label":"A","phase":"connect_pwd","value":1.637}
OQ5 {"extra":{},"iter":10,"label":"A","phase":"connect_pwd","value":1.583}
OQ5 {"extra":{},"iter":11,"label":"A","phase":"connect_pwd","value":1.574}
OQ5 {"extra":{},"iter":12,"label":"A","phase":"connect_pwd","value":1.749}
OQ5 {"extra":{},"iter":13,"label":"A","phase":"connect_pwd","value":1.688}
OQ5 {"extra":{},"iter":14,"label":"A","phase":"connect_pwd","value":1.643}
OQ5 {"extra":{},"iter":15,"label":"A","phase":"connect_pwd","value":1.503}
OQ5 {"extra":{},"iter":16,"label":"A","phase":"connect_pwd","value":1.645}
OQ5 {"extra":{},"iter":17,"label":"A","phase":"connect_pwd","value":1.5}
OQ5 {"extra":{},"iter":18,"label":"A","phase":"connect_pwd","value":1.45}
OQ5 {"extra":{},"iter":19,"label":"A","phase":"connect_pwd","value":1.502}
OQ5 {"extra":{},"iter":20,"label":"A","phase":"connect_pwd","value":1.575}
OQ5 {"extra":{"files":152},"iter":1,"label":"A","phase":"stage","value":72.676}
OQ5 {"extra":{"files":152},"iter":1,"label":"A","phase":"commit","value":244.594}
OQ5 {"extra":{},"iter":1,"label":"A","phase":"scan_with_payload","value":0.888}
OQ5 {"extra":{},"iter":1,"label":"A","phase":"scan_with_payload_gateway_ms","value":676.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":1,"label":"A","phase":"commit_delete","value":113.312}
OQ5 {"extra":{},"iter":1,"label":"A","phase":"scan_after_delete","value":0.678}
OQ5 {"extra":{},"iter":1,"label":"A","phase":"scan_after_delete_gateway_ms","value":478.0}
OQ5 {"extra":{"files":152},"iter":2,"label":"A","phase":"stage","value":81.58}
OQ5 {"extra":{"files":152},"iter":2,"label":"A","phase":"commit","value":233.932}
OQ5 {"extra":{},"iter":2,"label":"A","phase":"scan_with_payload","value":0.735}
OQ5 {"extra":{},"iter":2,"label":"A","phase":"scan_with_payload_gateway_ms","value":601.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":2,"label":"A","phase":"commit_delete","value":114.182}
OQ5 {"extra":{},"iter":2,"label":"A","phase":"scan_after_delete","value":0.713}
OQ5 {"extra":{},"iter":2,"label":"A","phase":"scan_after_delete_gateway_ms","value":489.0}
OQ5 {"extra":{"files":152},"iter":3,"label":"A","phase":"stage","value":79.186}
OQ5 {"extra":{"files":152},"iter":3,"label":"A","phase":"commit","value":251.063}
OQ5 {"extra":{},"iter":3,"label":"A","phase":"scan_with_payload","value":0.794}
OQ5 {"extra":{},"iter":3,"label":"A","phase":"scan_with_payload_gateway_ms","value":581.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":3,"label":"A","phase":"commit_delete","value":112.001}
OQ5 {"extra":{},"iter":3,"label":"A","phase":"scan_after_delete","value":0.691}
OQ5 {"extra":{},"iter":3,"label":"A","phase":"scan_after_delete_gateway_ms","value":480.0}
OQ5 {"extra":{"files":152},"iter":4,"label":"A","phase":"stage","value":83.265}
OQ5 {"extra":{"files":152},"iter":4,"label":"A","phase":"commit","value":248.693}
OQ5 {"extra":{},"iter":4,"label":"A","phase":"scan_with_payload","value":0.854}
OQ5 {"extra":{},"iter":4,"label":"A","phase":"scan_with_payload_gateway_ms","value":611.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":4,"label":"A","phase":"commit_delete","value":105.15}
OQ5 {"extra":{},"iter":4,"label":"A","phase":"scan_after_delete","value":0.688}
OQ5 {"extra":{},"iter":4,"label":"A","phase":"scan_after_delete_gateway_ms","value":481.0}
OQ5 {"extra":{"files":152},"iter":5,"label":"A","phase":"stage","value":74.704}
OQ5 {"extra":{"files":152},"iter":5,"label":"A","phase":"commit","value":241.349}
OQ5 {"extra":{},"iter":5,"label":"A","phase":"scan_with_payload","value":0.743}
OQ5 {"extra":{},"iter":5,"label":"A","phase":"scan_with_payload_gateway_ms","value":607.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":5,"label":"A","phase":"commit_delete","value":105.688}
OQ5 {"extra":{},"iter":5,"label":"A","phase":"scan_after_delete","value":0.68}
OQ5 {"extra":{},"iter":5,"label":"A","phase":"scan_after_delete_gateway_ms","value":453.0}
OQ5 {"extra":{"files":152},"iter":6,"label":"A","phase":"stage","value":79.558}
OQ5 {"extra":{"files":152},"iter":6,"label":"A","phase":"commit","value":239.751}
OQ5 {"extra":{},"iter":6,"label":"A","phase":"scan_with_payload","value":0.868}
OQ5 {"extra":{},"iter":6,"label":"A","phase":"scan_with_payload_gateway_ms","value":611.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":6,"label":"A","phase":"commit_delete","value":104.98}
OQ5 {"extra":{},"iter":6,"label":"A","phase":"scan_after_delete","value":0.555}
OQ5 {"extra":{},"iter":6,"label":"A","phase":"scan_after_delete_gateway_ms","value":429.0}
OQ5 {"extra":{"files":152},"iter":7,"label":"A","phase":"stage","value":76.103}
OQ5 {"extra":{"files":152},"iter":7,"label":"A","phase":"commit","value":248.052}
OQ5 {"extra":{},"iter":7,"label":"A","phase":"scan_with_payload","value":0.687}
OQ5 {"extra":{},"iter":7,"label":"A","phase":"scan_with_payload_gateway_ms","value":593.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":7,"label":"A","phase":"commit_delete","value":108.702}
OQ5 {"extra":{},"iter":7,"label":"A","phase":"scan_after_delete","value":0.513}
OQ5 {"extra":{},"iter":7,"label":"A","phase":"scan_after_delete_gateway_ms","value":420.0}
OQ5 {"extra":{"files":152},"iter":8,"label":"A","phase":"stage","value":78.012}
OQ5 {"extra":{"files":152},"iter":8,"label":"A","phase":"commit","value":247.752}
OQ5 {"extra":{},"iter":8,"label":"A","phase":"scan_with_payload","value":0.792}
OQ5 {"extra":{},"iter":8,"label":"A","phase":"scan_with_payload_gateway_ms","value":623.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":8,"label":"A","phase":"commit_delete","value":111.692}
OQ5 {"extra":{},"iter":8,"label":"A","phase":"scan_after_delete","value":0.699}
OQ5 {"extra":{},"iter":8,"label":"A","phase":"scan_after_delete_gateway_ms","value":429.0}
OQ5 {"extra":{"files":152},"iter":9,"label":"A","phase":"stage","value":73.92}
OQ5 {"extra":{"files":152},"iter":9,"label":"A","phase":"commit","value":246.081}
OQ5 {"extra":{},"iter":9,"label":"A","phase":"scan_with_payload","value":0.733}
OQ5 {"extra":{},"iter":9,"label":"A","phase":"scan_with_payload_gateway_ms","value":547.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":9,"label":"A","phase":"commit_delete","value":110.435}
OQ5 {"extra":{},"iter":9,"label":"A","phase":"scan_after_delete","value":0.981}
OQ5 {"extra":{},"iter":9,"label":"A","phase":"scan_after_delete_gateway_ms","value":460.0}
OQ5 {"extra":{"files":152},"iter":10,"label":"A","phase":"stage","value":78.633}
OQ5 {"extra":{"files":152},"iter":10,"label":"A","phase":"commit","value":242.86}
OQ5 {"extra":{},"iter":10,"label":"A","phase":"scan_with_payload","value":0.812}
OQ5 {"extra":{},"iter":10,"label":"A","phase":"scan_with_payload_gateway_ms","value":539.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":10,"label":"A","phase":"commit_delete","value":103.582}
OQ5 {"extra":{},"iter":10,"label":"A","phase":"scan_after_delete","value":0.518}
OQ5 {"extra":{},"iter":10,"label":"A","phase":"scan_after_delete_gateway_ms","value":438.0}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":1,"label":"A","phase":"read","value":166.158}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":2,"label":"A","phase":"read","value":161.497}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":3,"label":"A","phase":"read","value":165.869}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":4,"label":"A","phase":"read","value":177.936}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":5,"label":"A","phase":"read","value":164.867}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":6,"label":"A","phase":"read","value":154.791}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":7,"label":"A","phase":"read","value":156.445}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":8,"label":"A","phase":"read","value":158.33}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":9,"label":"A","phase":"read","value":171.4}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":10,"label":"A","phase":"read","value":165.62}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":11,"label":"A","phase":"read","value":162.006}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":12,"label":"A","phase":"read","value":163.312}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":13,"label":"A","phase":"read","value":164.12}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":14,"label":"A","phase":"read","value":186.683}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":15,"label":"A","phase":"read","value":164.61}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":16,"label":"A","phase":"read","value":181.155}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":17,"label":"A","phase":"read","value":160.507}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":18,"label":"A","phase":"read","value":164.146}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":19,"label":"A","phase":"read","value":163.367}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":20,"label":"A","phase":"read","value":159.947}
OQ5 {"label":"A","restore":{"files_to_remove":152}}
OQ5 {"label":"A","restore":{"commit_secs":108.073015459,"removed_dirs":70}}
OQ5 {"label":"A","restore":{"scan_secs":0.621665458}}
OQ5 {"label":"A","restore":{"removing_stray_dir":"com.cirruslink.mqtt.distributor.gateway"}}
OQ5 {"label":"A","restore":{"removing_stray_dir":"com.cirruslink.mqtt.engine.gateway"}}
OQ5 {"label":"A","restore":{"removing_stray_dir":"com.cirruslink.mqtt.transmission.gateway"}}
OQ5 {"label":"A","restore":{"removing_stray_dir":"com.inductiveautomation.eam"}}
OQ5 {"label":"A","restore":{"removing_stray_dir":"com.inductiveautomation.historian"}}
OQ5 {"label":"A","restore":{"removing_stray_dir":"com.inductiveautomation.opcua"}}
OQ5 {"label":"A","restore":{"removing_stray_dir":"com.inductiveautomation.perspective"}}
OQ5 {"label":"A","restore":{"removing_stray_dir":"ignition"}}
OQ5 {"label":"A","restore":{"staging_gone":true}}
OQ5 {"label":"A","summary":{"bytes":91833,"files":152,"label":"A","percentile":"nearest rank","phases":{"commit":{"max":251.063169042,"min":233.931712458,"n":10,"p50":244.593935,"p95":251.063169042},"commit_delete":{"max":114.18224675,"min":103.582237416,"n":10,"p50":108.701847333,"p95":114.18224675},"connect_pwd":{"max":1.7489185,"min":1.450035292,"n":20,"p50":1.574673041,"p95":1.6878268749999998},"read":{"max":186.683217167,"min":154.791211459,"n":20,"p50":164.119644125,"p95":181.154829291},"scan_after_delete":{"max":0.9807775,"min":0.51339875,"n":10,"p50":0.680079791,"p95":0.9807775},"scan_after_delete_gateway_ms":{"max":489.0,"min":420.0,"n":10,"p50":453.0,"p95":489.0},"scan_with_payload":{"max":0.887850459,"min":0.686637167,"n":10,"p50":0.792468834,"p95":0.887850459},"scan_with_payload_gateway_ms":{"max":676.0,"min":539.0,"n":10,"p50":601.0,"p95":676.0},"stage":{"max":83.265237791,"min":72.676209958,"n":10,"p50":78.012278208,"p95":83.265237791}},"source":"sanitized corpus (183 .json, 195 files) x 1, named units only","unit":"seconds unless the phase name ends in _ms","units":70}}
OQ5 {"label":"A","total_secs":8434.232989}
OQ5 {"label":"A","restore_assertion":{"entries_after":1,"entries_before":1,"production_listing_equals_pre_run":true}}
OQ5 {"ign_deploy_in_home_exists":false,"label":"A"}
PASS oq5_timings
test oq5_timings ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 8437.77s
~~~


## OQ5 run B (Defender exclusion on)

**Excluded paths (reported by the coordinator after the user added them as administrator with `az vm run-command` and `Add-MpPreference`; `Get-MpPreference` read them back verbatim as):**

```
C:\Program Files\Inductive Automation\Ignition\data
C:\Users\svc-ignition-deploy\ign-deploy
```

`RealTimeProtectionEnabled` is still `True`. The read-back went through `az vm run-command` (as SYSTEM), not as `svc-ignition-deploy`; the account cannot read Defender settings (T-22-61: the exclusion is on a capture host, the production recommendation is recorded in OQ5-3, not applied).

Same harness, host, account, payload (scale 1: 70 named units, 152 files, 91,833 bytes) and sample counts as run A, so the A/B delta is like for like. The scale-10 probe was ruled out by the coordinator; the 1,520-file figures in the decisions are extrapolations.

Command (environment as in run A):

```
IGNITION_LIVE_WFLWVM01=1 IGNITION_LIVE_OQ5=1 IGN_OQ5_LABEL=B IGN_OQ5_SCALE=1 \
  cargo test -p ignition-core --test live_transport_wflwvm01 oq5_timings -- --ignored --nocapture
```

### Attempts

1. **Attempt B1** (2026-10-15T14:59:06Z to 15:38:01Z, 2329 s) was cut off in iteration 5 by `exit code 255: Connection closed` on a `stage` call. It completed 4 iterations (stage, commit, scans, delete) and no reads, then restored the host (`production_listing_equals_pre_run: true`, `ign-deploy` absent). Its samples are listed under "Attempt B1" and are **not pooled** into the table below.
2. **Attempt B2** (2026-10-15T15:39:29Z to 18:14:42Z, 9303 s) is the run B of this section: 10 iterations, 20 reads, 20 connect samples. All its measurements completed. Its final restore step failed (`remove_tree: exit code 255` from ssh, a transient connection failure; the harness' own summary line was therefore not printed). The host was then restored with the harness' restore-only mode (`IGN_OQ5_RESTORE_ONLY`, first try: `ssh: connect to host ... port 22: Operation timed out`; second try, minutes later: restored, `production_listing_equals_pre_run: true`). The harness' restore now retries transient ssh failures (`remove_tree_retry`). The summary below is computed from the printed sample lines of B2 with the same nearest-rank rule as the harness (and the same script that reproduces run A's table exactly).

### Gateway and license state

- The trial was reset by the coordinator at about 2026-10-15T14:58Z (`POST /data/api/v1/trial`, `trialSecondsLeft` 7199) before attempt B1; it was at 4744 s left when B2 started at 15:39Z.
- It expired at about 16:58:09Z during B2 (`trialSecondsLeft` 0, `expired: true` at 16:58:09Z). **I reset it myself at 2026-10-15T16:58:14Z** with `ign api call --method POST --path /data/api/v1/trial --profile azure-win --json` (the token profile; `status 200`, `trialSecondsLeft` 7199, `expired: false`), as instructed. The run continued across the reset without a visible step. The trial had about 2257 s left at 18:20Z, after the run.
- Scans: no scan was refused. Gateway-side `lastScanDuration` with the payload present was 291 to 338 ms in B2 (run A: 539 to 676 ms).

### Summary (nearest-rank percentiles, seconds; `_ms` rows are the gateway's `lastScanDuration` in milliseconds), attempt B2

| Phase | n | p50 | p95 | min | max | run A p95 |
|---|---|---|---|---|---|---|
| connect (`sftp` session running `pwd`) | 20 | 2.010 | 2.423 | 1.507 | 3.026 | 1.688 |
| read of the scoped collection, payload present | 20 | 169.436 | 214.448 | 161.436 | 221.489 | 181.155 |
| stage | 10 | 84.490 | 100.471 | 74.056 | 100.471 | 83.265 |
| commit (rename into `production`, empty delete set) | 10 | 285.193 | 348.225 | 241.641 | 348.225 | 251.063 |
| scan, wall, payload present | 10 | 0.520 | 0.642 | 0.389 | 0.642 | 0.888 |
| scan, gateway `lastScanDuration`, payload present (ms) | 10 | 301 | 338 | 291 | 338 | 676 |
| commit of the delete set (152 files, 70 directories removed) | 10 | 111.838 | 128.288 | 108.247 | 128.288 | 114.182 |
| scan, wall, after delete | 10 | 0.378 | 0.562 | 0.299 | 0.562 | 0.981 |
| scan, gateway `lastScanDuration`, after delete (ms) | 10 | 197 | 266 | 194 | 266 | 489 |

Attempt B1 (n = 4, not pooled): stage p50 77.354 / max 99.467; commit p50 272.475 / max 324.699; delete commit p50 123.610 / max 136.220; scan with payload wall p50 0.508, gateway 315 to 489 ms; connect (20 samples) p50 1.446, p95 1.665.

### Restore and host-clean proof (T-22-59)

After the run: `production` lists `config-mode.json` only (sha256 `f7b5b709abfadf5ad7338bd40ca14ebe4b4d123f4b2689112fa4cca36161b2fa`, 133 bytes; the harness' hash-equality assertion `production_listing_equals_pre_run: true` passed in the restore-only run and again in a second restore-only check); `ign-deploy` is absent from the account home (`ign_deploy_in_home_exists: false`); and over ssh as the account, `dir /b production` printed `config-mode.json` only and `dir /b %USERPROFILE% | findstr /i ign-` printed nothing. The Defender exclusions were left in place by the harness; removing them is the user's call.

### Raw output, attempt B2

~~~
OQ5 {"connect_samples":20,"iters":10,"label":"B","op_timeout_s":1800,"payload":{"bytes":91833,"files":152,"source":"sanitized corpus (183 .json, 195 files) x 1, named units only","types":27,"units":70},"reads":20}
OQ5 {"gateway_scan_state_before":{"lastScanDuration":240,"lastScanTimestamp":1792078489720,"scanActive":false},"label":"B"}
OQ5 {"label":"B","pre_run_listing":[{"dir":false,"path":"config-mode.json","sha256":"f7b5b709abfadf5ad7338bd40ca14ebe4b4d123f4b2689112fa4cca36161b2fa","size":133}]}
OQ5 {"extra":{},"iter":1,"label":"B","phase":"connect_pwd","value":1.865}
OQ5 {"extra":{},"iter":2,"label":"B","phase":"connect_pwd","value":2.147}
OQ5 {"extra":{},"iter":3,"label":"B","phase":"connect_pwd","value":2.378}
OQ5 {"extra":{},"iter":4,"label":"B","phase":"connect_pwd","value":1.737}
OQ5 {"extra":{},"iter":5,"label":"B","phase":"connect_pwd","value":2.01}
OQ5 {"extra":{},"iter":6,"label":"B","phase":"connect_pwd","value":1.772}
OQ5 {"extra":{},"iter":7,"label":"B","phase":"connect_pwd","value":1.878}
OQ5 {"extra":{},"iter":8,"label":"B","phase":"connect_pwd","value":1.729}
OQ5 {"extra":{},"iter":9,"label":"B","phase":"connect_pwd","value":2.297}
OQ5 {"extra":{},"iter":10,"label":"B","phase":"connect_pwd","value":2.338}
OQ5 {"extra":{},"iter":11,"label":"B","phase":"connect_pwd","value":1.855}
OQ5 {"extra":{},"iter":12,"label":"B","phase":"connect_pwd","value":2.264}
OQ5 {"extra":{},"iter":13,"label":"B","phase":"connect_pwd","value":1.507}
test oq5_timings has been running for over 60 seconds
OQ5 {"extra":{},"iter":14,"label":"B","phase":"connect_pwd","value":2.103}
OQ5 {"extra":{},"iter":15,"label":"B","phase":"connect_pwd","value":1.748}
OQ5 {"extra":{},"iter":16,"label":"B","phase":"connect_pwd","value":2.355}
OQ5 {"extra":{},"iter":17,"label":"B","phase":"connect_pwd","value":3.026}
OQ5 {"extra":{},"iter":18,"label":"B","phase":"connect_pwd","value":2.241}
OQ5 {"extra":{},"iter":19,"label":"B","phase":"connect_pwd","value":1.874}
OQ5 {"extra":{},"iter":20,"label":"B","phase":"connect_pwd","value":2.423}
OQ5 {"extra":{"files":152},"iter":1,"label":"B","phase":"stage","value":100.471}
OQ5 {"extra":{"files":152},"iter":1,"label":"B","phase":"commit","value":289.487}
OQ5 {"extra":{},"iter":1,"label":"B","phase":"scan_with_payload","value":0.511}
OQ5 {"extra":{},"iter":1,"label":"B","phase":"scan_with_payload_gateway_ms","value":338.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":1,"label":"B","phase":"commit_delete","value":128.017}
OQ5 {"extra":{},"iter":1,"label":"B","phase":"scan_after_delete","value":0.423}
OQ5 {"extra":{},"iter":1,"label":"B","phase":"scan_after_delete_gateway_ms","value":197.0}
OQ5 {"extra":{"files":152},"iter":2,"label":"B","phase":"stage","value":96.904}
OQ5 {"extra":{"files":152},"iter":2,"label":"B","phase":"commit","value":289.002}
OQ5 {"extra":{},"iter":2,"label":"B","phase":"scan_with_payload","value":0.613}
OQ5 {"extra":{},"iter":2,"label":"B","phase":"scan_with_payload_gateway_ms","value":295.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":2,"label":"B","phase":"commit_delete","value":128.288}
OQ5 {"extra":{},"iter":2,"label":"B","phase":"scan_after_delete","value":0.482}
OQ5 {"extra":{},"iter":2,"label":"B","phase":"scan_after_delete_gateway_ms","value":195.0}
OQ5 {"extra":{"files":152},"iter":3,"label":"B","phase":"stage","value":92.319}
OQ5 {"extra":{"files":152},"iter":3,"label":"B","phase":"commit","value":306.513}
OQ5 {"extra":{},"iter":3,"label":"B","phase":"scan_with_payload","value":0.429}
OQ5 {"extra":{},"iter":3,"label":"B","phase":"scan_with_payload_gateway_ms","value":297.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":3,"label":"B","phase":"commit_delete","value":115.531}
OQ5 {"extra":{},"iter":3,"label":"B","phase":"scan_after_delete","value":0.378}
OQ5 {"extra":{},"iter":3,"label":"B","phase":"scan_after_delete_gateway_ms","value":198.0}
OQ5 {"extra":{"files":152},"iter":4,"label":"B","phase":"stage","value":92.924}
OQ5 {"extra":{"files":152},"iter":4,"label":"B","phase":"commit","value":348.225}
OQ5 {"extra":{},"iter":4,"label":"B","phase":"scan_with_payload","value":0.394}
OQ5 {"extra":{},"iter":4,"label":"B","phase":"scan_with_payload_gateway_ms","value":301.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":4,"label":"B","phase":"commit_delete","value":127.507}
OQ5 {"extra":{},"iter":4,"label":"B","phase":"scan_after_delete","value":0.299}
OQ5 {"extra":{},"iter":4,"label":"B","phase":"scan_after_delete_gateway_ms","value":206.0}
OQ5 {"extra":{"files":152},"iter":5,"label":"B","phase":"stage","value":82.387}
OQ5 {"extra":{"files":152},"iter":5,"label":"B","phase":"commit","value":292.818}
OQ5 {"extra":{},"iter":5,"label":"B","phase":"scan_with_payload","value":0.52}
OQ5 {"extra":{},"iter":5,"label":"B","phase":"scan_with_payload_gateway_ms","value":322.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":5,"label":"B","phase":"commit_delete","value":108.247}
OQ5 {"extra":{},"iter":5,"label":"B","phase":"scan_after_delete","value":0.371}
OQ5 {"extra":{},"iter":5,"label":"B","phase":"scan_after_delete_gateway_ms","value":194.0}
OQ5 {"extra":{"files":152},"iter":6,"label":"B","phase":"stage","value":84.49}
OQ5 {"extra":{"files":152},"iter":6,"label":"B","phase":"commit","value":241.641}
OQ5 {"extra":{},"iter":6,"label":"B","phase":"scan_with_payload","value":0.557}
OQ5 {"extra":{},"iter":6,"label":"B","phase":"scan_with_payload_gateway_ms","value":325.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":6,"label":"B","phase":"commit_delete","value":111.838}
OQ5 {"extra":{},"iter":6,"label":"B","phase":"scan_after_delete","value":0.562}
OQ5 {"extra":{},"iter":6,"label":"B","phase":"scan_after_delete_gateway_ms","value":228.0}
OQ5 {"extra":{"files":152},"iter":7,"label":"B","phase":"stage","value":75.207}
OQ5 {"extra":{"files":152},"iter":7,"label":"B","phase":"commit","value":255.937}
OQ5 {"extra":{},"iter":7,"label":"B","phase":"scan_with_payload","value":0.389}
OQ5 {"extra":{},"iter":7,"label":"B","phase":"scan_with_payload_gateway_ms","value":301.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":7,"label":"B","phase":"commit_delete","value":109.41}
OQ5 {"extra":{},"iter":7,"label":"B","phase":"scan_after_delete","value":0.392}
OQ5 {"extra":{},"iter":7,"label":"B","phase":"scan_after_delete_gateway_ms","value":197.0}
OQ5 {"extra":{"files":152},"iter":8,"label":"B","phase":"stage","value":84.61}
OQ5 {"extra":{"files":152},"iter":8,"label":"B","phase":"commit","value":285.193}
OQ5 {"extra":{},"iter":8,"label":"B","phase":"scan_with_payload","value":0.642}
OQ5 {"extra":{},"iter":8,"label":"B","phase":"scan_with_payload_gateway_ms","value":326.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":8,"label":"B","phase":"commit_delete","value":109.767}
OQ5 {"extra":{},"iter":8,"label":"B","phase":"scan_after_delete","value":0.345}
OQ5 {"extra":{},"iter":8,"label":"B","phase":"scan_after_delete_gateway_ms","value":203.0}
OQ5 {"extra":{"files":152},"iter":9,"label":"B","phase":"stage","value":74.056}
OQ5 {"extra":{"files":152},"iter":9,"label":"B","phase":"commit","value":265.5}
OQ5 {"extra":{},"iter":9,"label":"B","phase":"scan_with_payload","value":0.552}
OQ5 {"extra":{},"iter":9,"label":"B","phase":"scan_with_payload_gateway_ms","value":291.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":9,"label":"B","phase":"commit_delete","value":118.625}
OQ5 {"extra":{},"iter":9,"label":"B","phase":"scan_after_delete","value":0.386}
OQ5 {"extra":{},"iter":9,"label":"B","phase":"scan_after_delete_gateway_ms","value":266.0}
OQ5 {"extra":{"files":152},"iter":10,"label":"B","phase":"stage","value":80.364}
OQ5 {"extra":{"files":152},"iter":10,"label":"B","phase":"commit","value":264.526}
OQ5 {"extra":{},"iter":10,"label":"B","phase":"scan_with_payload","value":0.536}
OQ5 {"extra":{},"iter":10,"label":"B","phase":"scan_with_payload_gateway_ms","value":326.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":10,"label":"B","phase":"commit_delete","value":108.883}
OQ5 {"extra":{},"iter":10,"label":"B","phase":"scan_after_delete","value":0.36}
OQ5 {"extra":{},"iter":10,"label":"B","phase":"scan_after_delete_gateway_ms","value":197.0}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":1,"label":"B","phase":"read","value":168.23}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":2,"label":"B","phase":"read","value":170.942}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":3,"label":"B","phase":"read","value":205.709}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":4,"label":"B","phase":"read","value":175.804}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":5,"label":"B","phase":"read","value":165.635}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":6,"label":"B","phase":"read","value":212.931}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":7,"label":"B","phase":"read","value":169.436}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":8,"label":"B","phase":"read","value":162.286}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":9,"label":"B","phase":"read","value":167.901}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":10,"label":"B","phase":"read","value":168.011}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":11,"label":"B","phase":"read","value":170.99}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":12,"label":"B","phase":"read","value":161.436}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":13,"label":"B","phase":"read","value":185.873}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":14,"label":"B","phase":"read","value":214.448}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":15,"label":"B","phase":"read","value":221.489}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":16,"label":"B","phase":"read","value":164.397}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":17,"label":"B","phase":"read","value":176.672}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":18,"label":"B","phase":"read","value":172.419}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":19,"label":"B","phase":"read","value":166.887}
OQ5 {"extra":{"bytes":117914,"core_files":46,"mode_files":152},"iter":20,"label":"B","phase":"read","value":164.468}
OQ5 {"label":"B","restore":{"files_to_remove":152}}
OQ5 {"label":"B","restore":{"commit_secs":112.617156959,"removed_dirs":70}}
OQ5 {"label":"B","restore":{"scan_secs":0.326581666}}
OQ5 {"label":"B","restore":{"removing_stray_dir":"com.cirruslink.mqtt.distributor.gateway"}}
OQ5 {"label":"B","restore":{"removing_stray_dir":"com.cirruslink.mqtt.engine.gateway"}}
OQ5 {"label":"B","restore":{"removing_stray_dir":"com.cirruslink.mqtt.transmission.gateway"}}
OQ5 {"label":"B","restore":{"removing_stray_dir":"com.inductiveautomation.eam"}}
OQ5 {"label":"B","restore":{"removing_stray_dir":"com.inductiveautomation.historian"}}
OQ5 {"label":"B","restore":{"removing_stray_dir":"com.inductiveautomation.opcua"}}
OQ5 {"label":"B","restore":{"removing_stray_dir":"com.inductiveautomation.perspective"}}
OQ5 {"label":"B","restore":{"removing_stray_dir":"ignition"}}
thread 'oq5_timings' (35122017) panicked at crates/ignition-core/tests/live_transport_wflwvm01.rs:272:10:
remove_tree: TransportFailed { operation: "exec", node: "nodes.master", detail: "remove_tree: exit code 255" }
test oq5_timings ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 10 filtered out; finished in 9302.90s
~~~

### Raw output, restore-only runs after B2 (first try failed on a transient connection timeout, second restored)

~~~
OQ5 {"connect_samples":20,"iters":10,"label":"B-restore","op_timeout_s":1800,"payload":{"bytes":918330,"files":1520,"source":"sanitized corpus (183 .json, 195 files) x 10, named units only","types":27,"units":700},"reads":20}
OQ5 {"gateway_scan_state_before":{"lastScanDuration":202,"lastScanTimestamp":1792088041065,"scanActive":false},"label":"B-restore"}
OQ5 {"label":"B-restore","pre_run_listing":[{"dir":false,"path":"config-mode.json","sha256":"f7b5b709abfadf5ad7338bd40ca14ebe4b4d123f4b2689112fa4cca36161b2fa","size":133}]}
thread 'oq5_timings' (36824595) panicked at crates/ignition-core/tests/live_transport_wflwvm01.rs:282:10:
enumerate: TransportFailed { operation: "exec", node: "nodes.master", detail: "enumerate: exit code 255: ssh: connect to host 40.114.71.134 port 22: Operation timed out" }
test oq5_timings ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 10 filtered out; finished in 50.15s
OQ5 {"connect_samples":20,"iters":10,"label":"B-restore","op_timeout_s":1800,"payload":{"bytes":918330,"files":1520,"source":"sanitized corpus (183 .json, 195 files) x 10, named units only","types":27,"units":700},"reads":20}
OQ5 {"gateway_scan_state_before":{"lastScanDuration":202,"lastScanTimestamp":1792088041065,"scanActive":false},"label":"B-restore"}
OQ5 {"label":"B-restore","pre_run_listing":[{"dir":false,"path":"config-mode.json","sha256":"f7b5b709abfadf5ad7338bd40ca14ebe4b4d123f4b2689112fa4cca36161b2fa","size":133}]}
OQ5 {"label":"B-restore","restore":{"files_to_remove":0}}
OQ5 {"label":"B-restore","restore":{"staging_gone":true}}
OQ5 {"label":"B-restore","summary":{"bytes":918330,"files":1520,"label":"B-restore","percentile":"nearest rank","phases":{},"source":"sanitized corpus (183 .json, 195 files) x 10, named units only","unit":"seconds unless the phase name ends in _ms","units":700}}
OQ5 {"label":"B-restore","total_secs":41.507943292}
OQ5 {"label":"B-restore","restore_assertion":{"entries_after":1,"entries_before":1,"production_listing_equals_pre_run":true}}
OQ5 {"ign_deploy_in_home_exists":false,"label":"B-restore"}
PASS oq5_timings
test oq5_timings ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 43.59s
~~~

### Raw output, attempt B1

~~~
OQ5 {"connect_samples":20,"iters":10,"label":"B","op_timeout_s":1800,"payload":{"bytes":91833,"files":152,"source":"sanitized corpus (183 .json, 195 files) x 1, named units only","types":27,"units":70},"reads":20}
OQ5 {"gateway_scan_state_before":{"lastScanDuration":416,"lastScanTimestamp":1792075704531,"scanActive":false},"label":"B"}
OQ5 {"label":"B","pre_run_listing":[{"dir":false,"path":"config-mode.json","sha256":"f7b5b709abfadf5ad7338bd40ca14ebe4b4d123f4b2689112fa4cca36161b2fa","size":133}]}
OQ5 {"extra":{},"iter":1,"label":"B","phase":"connect_pwd","value":1.44}
OQ5 {"extra":{},"iter":2,"label":"B","phase":"connect_pwd","value":1.569}
OQ5 {"extra":{},"iter":3,"label":"B","phase":"connect_pwd","value":1.387}
OQ5 {"extra":{},"iter":4,"label":"B","phase":"connect_pwd","value":1.493}
OQ5 {"extra":{},"iter":5,"label":"B","phase":"connect_pwd","value":1.427}
OQ5 {"extra":{},"iter":6,"label":"B","phase":"connect_pwd","value":1.525}
OQ5 {"extra":{},"iter":7,"label":"B","phase":"connect_pwd","value":1.498}
OQ5 {"extra":{},"iter":8,"label":"B","phase":"connect_pwd","value":1.446}
OQ5 {"extra":{},"iter":9,"label":"B","phase":"connect_pwd","value":1.52}
OQ5 {"extra":{},"iter":10,"label":"B","phase":"connect_pwd","value":1.355}
OQ5 {"extra":{},"iter":11,"label":"B","phase":"connect_pwd","value":1.444}
OQ5 {"extra":{},"iter":12,"label":"B","phase":"connect_pwd","value":1.323}
OQ5 {"extra":{},"iter":13,"label":"B","phase":"connect_pwd","value":1.444}
OQ5 {"extra":{},"iter":14,"label":"B","phase":"connect_pwd","value":1.58}
OQ5 {"extra":{},"iter":15,"label":"B","phase":"connect_pwd","value":1.421}
OQ5 {"extra":{},"iter":16,"label":"B","phase":"connect_pwd","value":1.416}
test oq5_timings has been running for over 60 seconds
OQ5 {"extra":{},"iter":17,"label":"B","phase":"connect_pwd","value":1.635}
OQ5 {"extra":{},"iter":18,"label":"B","phase":"connect_pwd","value":1.665}
OQ5 {"extra":{},"iter":19,"label":"B","phase":"connect_pwd","value":1.573}
OQ5 {"extra":{},"iter":20,"label":"B","phase":"connect_pwd","value":1.705}
OQ5 {"extra":{"files":152},"iter":1,"label":"B","phase":"stage","value":76.698}
OQ5 {"extra":{"files":152},"iter":1,"label":"B","phase":"commit","value":248.735}
OQ5 {"extra":{},"iter":1,"label":"B","phase":"scan_with_payload","value":0.581}
OQ5 {"extra":{},"iter":1,"label":"B","phase":"scan_with_payload_gateway_ms","value":489.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":1,"label":"B","phase":"commit_delete","value":110.727}
OQ5 {"extra":{},"iter":1,"label":"B","phase":"scan_after_delete","value":0.398}
OQ5 {"extra":{},"iter":1,"label":"B","phase":"scan_after_delete_gateway_ms","value":212.0}
OQ5 {"extra":{"files":152},"iter":2,"label":"B","phase":"stage","value":82.979}
OQ5 {"extra":{"files":152},"iter":2,"label":"B","phase":"commit","value":272.475}
OQ5 {"extra":{},"iter":2,"label":"B","phase":"scan_with_payload","value":0.508}
OQ5 {"extra":{},"iter":2,"label":"B","phase":"scan_with_payload_gateway_ms","value":325.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":2,"label":"B","phase":"commit_delete","value":123.61}
OQ5 {"extra":{},"iter":2,"label":"B","phase":"scan_after_delete","value":0.343}
OQ5 {"extra":{},"iter":2,"label":"B","phase":"scan_after_delete_gateway_ms","value":201.0}
OQ5 {"extra":{"files":152},"iter":3,"label":"B","phase":"stage","value":77.354}
OQ5 {"extra":{"files":152},"iter":3,"label":"B","phase":"commit","value":279.026}
OQ5 {"extra":{},"iter":3,"label":"B","phase":"scan_with_payload","value":0.54}
OQ5 {"extra":{},"iter":3,"label":"B","phase":"scan_with_payload_gateway_ms","value":335.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":3,"label":"B","phase":"commit_delete","value":136.22}
OQ5 {"extra":{},"iter":3,"label":"B","phase":"scan_after_delete","value":0.351}
OQ5 {"extra":{},"iter":3,"label":"B","phase":"scan_after_delete_gateway_ms","value":203.0}
OQ5 {"extra":{"files":152},"iter":4,"label":"B","phase":"stage","value":99.467}
OQ5 {"extra":{"files":152},"iter":4,"label":"B","phase":"commit","value":324.699}
OQ5 {"extra":{},"iter":4,"label":"B","phase":"scan_with_payload","value":0.404}
OQ5 {"extra":{},"iter":4,"label":"B","phase":"scan_with_payload_gateway_ms","value":315.0}
OQ5 {"extra":{"files":152,"removed_dirs":70},"iter":4,"label":"B","phase":"commit_delete","value":135.361}
OQ5 {"extra":{},"iter":4,"label":"B","phase":"scan_after_delete","value":0.499}
OQ5 {"extra":{},"iter":4,"label":"B","phase":"scan_after_delete_gateway_ms","value":240.0}
OQ5 {"label":"B","restore":{"files_to_remove":0}}
OQ5 {"label":"B","restore":{"removing_stray_dir":"com.cirruslink.mqtt.distributor.gateway"}}
OQ5 {"label":"B","restore":{"removing_stray_dir":"com.cirruslink.mqtt.engine.gateway"}}
OQ5 {"label":"B","restore":{"removing_stray_dir":"com.cirruslink.mqtt.transmission.gateway"}}
OQ5 {"label":"B","restore":{"removing_stray_dir":"com.inductiveautomation.eam"}}
OQ5 {"label":"B","restore":{"removing_stray_dir":"com.inductiveautomation.historian"}}
OQ5 {"label":"B","restore":{"removing_stray_dir":"com.inductiveautomation.opcua"}}
OQ5 {"label":"B","restore":{"removing_stray_dir":"com.inductiveautomation.perspective"}}
OQ5 {"label":"B","restore":{"removing_stray_dir":"ignition"}}
OQ5 {"label":"B","restore":{"staging_gone":true}}
OQ5 {"label":"B","summary":{"bytes":91833,"files":152,"label":"B","percentile":"nearest rank","phases":{"commit":{"max":324.698530292,"min":248.734927792,"n":4,"p50":272.474689625,"p95":324.698530292},"commit_delete":{"max":136.22041625,"min":110.727371875,"n":4,"p50":123.610231,"p95":136.22041625},"connect_pwd":{"max":1.705235958,"min":1.3229339580000001,"n":20,"p50":1.446371083,"p95":1.6652476250000001},"scan_after_delete":{"max":0.499330042,"min":0.342669,"n":4,"p50":0.350623083,"p95":0.499330042},"scan_after_delete_gateway_ms":{"max":240.0,"min":201.0,"n":4,"p50":203.0,"p95":240.0},"scan_with_payload":{"max":0.581023417,"min":0.4041645,"n":4,"p50":0.507575625,"p95":0.581023417},"scan_with_payload_gateway_ms":{"max":489.0,"min":315.0,"n":4,"p50":325.0,"p95":489.0},"stage":{"max":99.467064416,"min":76.697937458,"n":4,"p50":77.354143333,"p95":99.467064416}},"source":"sanitized corpus (183 .json, 195 files) x 1, named units only","unit":"seconds unless the phase name ends in _ms","units":70}}
OQ5 {"label":"B","total_secs":2324.94185225}
OQ5 {"label":"B","restore_assertion":{"entries_after":1,"entries_before":1,"production_listing_equals_pre_run":true}}
OQ5 {"ign_deploy_in_home_exists":false,"label":"B"}
thread 'oq5_timings' (34397006) panicked at crates/ignition-core/tests/live_transport_wflwvm01.rs:1708:14:
the measurement loop failed; the host was restored: "TransportFailed { operation: \"stage\", node: \"nodes.master\", detail: \"exit code 255: Connection closed\" }"
test oq5_timings ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 10 filtered out; finished in 2328.51s
~~~


## Decisions

Written after `capture22.sh cv1`, `cv2` and `cv3` ran to completion against the rig above. Every decision cites the blocks it rests on. Two points where a capture contradicts 22-RESEARCH are marked CONTRADICTS. 22-10 follows these decisions; nothing below is taken from research or inference.

### CV1 — The "will not be loaded" line: match rule, window, settle time (research A6, Pattern 7)

Rests on `CV1a` (blocks "GET logs, all levels, window of the CV1a scan" and "is CV1_NOUUID loaded?"), `CV1b` (blocks "GET logs minLevel=WARN", "GET logs logger=NamedResourceHandler", "is CV1_NOUUID_DB loaded?", the two scan answers), `CV1c` (block "GET logs minLevel=WARN, window of the CV1c scan") and `CV1d` (the lag probe).

**Shape (CV1b, `GET logs minLevel=WARN`).** The REST `logs` route returns the RIG.md text verbatim. Per scan and per resource it returns two entries a few ms apart (9 ms here, 1 to 4 ms in CV1d), with identical content:

```json
{"timestamp":1792008016254,"loggerName":"NamedResourceHandler-ignition/database-connection","level":"WARN","message":"Resource ignition/database-connection/CV1_NOUUID_DB does not have required UUID attribute, will not be loaded","mdc":{"collection":"production"}}
```

Keys: `timestamp` (epoch ms, gateway clock), `loggerName`, `level` (`WARN`), `message`, `mdc.collection` (the collection the resource sits in). There is no `stack` key. The envelope is `{"items":[...],"metadata":{"total","matching","limit","offset"}}`. The `loggerName` is `NamedResourceHandler-<module>/<type>`; it carries the type, not the name, so the unit key comes from the `message`.

**Match rule for 22-10 check `loaded`.** A hit is an entry whose `message` contains `will not be loaded` AND `Resource <module>/<type>/<name> ` for a declared resource (match the `message` prefix `Resource ` plus the unit key plus a space). Do not filter on `loggerName` in the query: `logger=NamedResourceHandler` returned `items: []` for the same window in which `minLevel=WARN` returned both entries (CV1b), so the route's `logger` parameter is not a prefix filter. Use `minLevel=WARN`, `startTime`, `limit=200` and match client-side. Duplicate entries per resource count once. Report only `{timestamp, loggerName, resource_key}`, never the message.

**Window (CV1b, CV1d).**

- The entries are stamped AFTER `lastScanTimestamp` (CV1d `entry_stamp_minus_lastScanTimestamp_ms`: 13 to 126 ms; CV1b: 10 and 19 ms after `lastScanTimestamp`). They come from the resource load that follows the scan walk. A window that starts at `lastScanTimestamp` or later would therefore be safe only if the timestamp were the scan START; it is not claimed to be, so the window opens a full `lastScanDuration` before it, which is early under either reading of the field.
- Window start: `startTime = lastScanTimestamp - lastScanDuration - LOG_WINDOW_SLACK_MS` with **`LOG_WINDOW_SLACK_MS = 1000`**, all on the gateway clock (the scan answer and the log timestamps are both the gateway's, so the runner clock is never involved). No `endTime`. This narrows the 30 s slack 22-RESEARCH sketched: a wide slack re-reads lines of earlier scans, and with a still-declared resource that was fixed in between, a stale line would make `loaded` red. 1000 ms still keeps back-to-back scans from sharing a window in every captured run (the scans in CV1b and CV1c ended more than 3 s apart).
- **Settle before reading: `LOG_SETTLE_MS = 5000`.** An entry became readable 516 to 1450 ms after the scan reported complete (CV1d: 516, 857, 644, 518, 1450 ms; polled every 50 ms). Five seconds is more than three times the worst captured lag. An empty result before the settle time means nothing; the script that took CV1b/CV1c waited 2 s.
- **Logged once.** CV1c: a second scan with the resource unchanged and still uuid-less logs nothing (window of the CV1c scan: `items: []`), and the same query with the window opened at the CV1b scan start, which covers both scans, returns exactly the two CV1b entries and no new ones. A later verify run therefore cannot rediscover the line. `loaded` is evidence only for the scan verify itself waits on (or one it started less than the window ago); it must report `not_checked`/`unknown` rather than `ok` when the window predates the line's possible emission.
- **The REST view cannot detect an unloaded resource.** CV1b "is CV1_NOUUID_DB loaded?": the uuid-less database-connection is in the REST list with its full `config` and `attributes: {"enabled": true}`, even though the gateway logged `will not be loaded`. So `read_back` (CV2) does not subsume `loaded`; the log is the only gateway signal.
- **Not every uuid-less named resource logs.** CV1a: a uuid-less `database-translator` produced NO log entry (all levels, window of the scan: `items: []`) and IS in the REST list. The match rule must not infer "no line therefore every declared resource loaded" for types that do not require a uuid. Which types require one is not established here (database-connection does; database-translator does not). Render mints a uuid for every named resource whose type rule says so (Phase 21 CD3), so the line is a belt-and-braces check, not the main one.

### CV2 — REST effective view versus the file `config.json`: normalisation table (research A7, Pitfall 6)

Rests on `CV2`, block "per-type equality of REST config and file config.json (differing JSON paths only)". Comparison: each REST item's `config` against the `config.json` of the collection the item reports as `collection`, both through `jq -S` (key order ignored). 46 REST items over the non-tag types present on the rig after the tracer render.

| Type (module/type) | Result | Normalisation for 22-10 `read_back` |
| --- | --- | --- |
| `com.inductiveautomation.eam/event-thresholds`, `eam/module-settings`, `opcua/access-control`, `opcua/server-config`, `perspective/themes/{dark-cool,dark-warm,light-cool,light-warm}`, `sfc/chart-settings` | equal | none |
| `edge-system-properties`, `gateway-network-proxy-rules`, `gateway-network-queue-settings`, `gateway-network-settings`, `general-alarm-settings` (the tracer unit, `production`), `quickstart`, `security-properties`, `system-properties` | equal | none |
| `ignition/database-driver/{MySQL,Oracle Database,SQLite}`, `identity-provider/default`, `keyboard_layout/*`, `opc-connection/Ignition OPC UA Server`, `schedule/Example`, `user-source/{default,opcua-module}` | equal | none |
| `ignition/database-translator/{GENERIC,MSSQL,MYSQL,ORACLE,SQLITE}` | equal | none |
| `ignition/cobranding` | equal raw | **N1** (both sides). The REST `config` carries the nine unset image members (`appIcon`, `appIconName`, `appIconSize`, `favicon`, `faviconName`, `faviconSize`, `logo`, `logoName`, `logoType`) as explicit `null`, as the file does; they drop from both sides, so the type compares equal |
| `ignition/database-translator/POSTGRES` (the tracer render, `production`) | equal AFTER **N1** | **N1** (both sides): drop every object member whose value is JSON `null`, at every depth, from the file value AND from the REST value before comparing. The file holds `createAutoIncSequence` and `createAutoIncTrigger` as `null`; the REST view omits both. Applies to every type; after the drop a `null` on one side against a value on the other still differs |
| `ignition/security-levels` | DIFFERS | **N2**: the REST view is the file's tree MERGED with gateway-provided levels (`SecurityZones/Default` and the `Roles` branch appear only in REST; array order differs). Compare as a name-keyed subset: every object in the file's `children` arrays, matched by `name`, must appear in the REST tree with its members. The captured file is a name-keyed subset of the REST value (`file_is_name_keyed_subset_of_rest: true`). Do not require equality |
| `ignition/translations` | resource.json `files: []`, no `config.json` | **N3**: a unit whose `resource.json` has `files: []` has no `config.json`; the REST `config` is built from `resource.json` `attributes` (`caseInsensitive`, `ignorePunctuation`, `ignoreTags`, `ignoreWhitespace`, all equal to the attributes) plus an empty `terms`. Compare the attributes-backed keys, ignore `terms` |
| `perspective/themes/{dark,light}`, `ignition/database-driver/{MariaDB,Microsoft SQLServer,PostgreSQL}`, `ignition/schedule/Always` | REST `collection` is `system` | gateway-provided, not file-backed: no file to compare; `read_back` skips an item whose effective collection is `system` |
| `com.inductiveautomation.opcua/one-time`, `ignition/images` | `GET list` and `GET singleton` both answer 404 | no REST read route: `read_back` reports `not_checked` for these types |

- Tag types are out of scope here (compared through the WebDev `tags` rows, not the effective view).
- Amended 2026-10-15 (UAT gap G-22-2, plan 22-15): N1 was first applied to the file only. A default-branded gateway reports `ignition/cobranding`'s unset image members as explicit `null` in REST, so every tree from `deploy pull` came back red on those nine keys. N1 is now symmetric: null equals absent on both sides, and a one-sided null against a value still differs.
- **Not exercised on this rig** (so the table says nothing about them): `database-connection` (only as a uuid-less probe, config not compared), `oauth2-client`, `alarm-notification-profile`, `secret-provider`, `service-connector`, `roster-config`, `holiday`, and any type with a secret-typed field (the REST value of a `Referenced` secret was not compared). The N1 rule is stated for all types; a false red on an unexercised type is fixed by adding a row, not by loosening equality (Pitfall 6). 22-14 runs the finished verb on the prod-sized payload and should extend this table.
- `lastModification` and the other `attributes` keys are not part of `config` and are not compared.

### CV3 — A scan does not rewrite plain-file-write resource files (post-scan bytes equal rendered bytes)

Rests on `CV3`, blocks "sha256 of the rendered files", "sha256 of the same files in production/ right after the scan", the six "sha256 in production/ at +5 s ... +30 s" blocks and "byte comparison ... (cmp)".

- The four files of the tracer render (`database-translator/POSTGRES/{config,resource}.json`, `general-alarm-settings/{config,resource}.json`), written by `cp -R` into `production/`, have the same sha256 in the render directory, right after the scan, and at every 5 s step up to 30 s after it (`3ce93dcc...`, `81f9e836...`, `ce86cc6f...`, `736c80f4...`). `cmp` reports all four identical.
- Decision for 22-10 check `files` and for Phase 23: after a scan the effective files on disk equal the rendered bytes; `files` can compare sha256 of the post-scan bytes to the rendered sha256 and treat any difference as a finding. The Phase 21 CD2 result (omitted and stale `lastModificationSignature` not rewritten) points the same way and is consistent with this capture.
- Limits: four signature-bearing files from one render, one scan, 30 s. Tag files (the gateway writes those itself and flushes them asynchronously, Phase 21 CD4) and gateway-created resources are not covered; this rig had no hand-written tag files.

## Decisions: Windows OpenSSH host (22-12)

Written after `capture22w.sh c7a`, `c6` and `c5ntfs` ran to completion against the Azure VM `ign-win-836` (stand-in for WFLWVM01, 22-CONTEXT amendment of 2026-10-14): Windows Server 2022 build 10.0.20348.5761, `OpenSSH_for_Windows_8.1` (sshd banner), PowerShell 5.1.20348.5756, default shell `cmd.exe`, Ignition 8.3.6 (b2026042713), account `svc-ignition-deploy` (member of Users only). Every decision names the blocks it rests on by their titles in `## C7a`, `## C6` and `## C5-NTFS`. The host key was pinned from the `az vm run-command` fingerprint and re-checked by `ssh-keyscan` before any capture (SHA256:wgjZzSMU/Ss+bnO8SgMfOY90cnutJvlAuxSGNPJNkZo, ED25519). Verdicts on 22-RESEARCH assumptions: A4 CONFIRMED (with a correction), A5 CONFIRMED, A12 PARTLY CONTRADICTED, A16 CONFIRMED.

### C7a-1 — sftp path form (research A4)

Rests on C7a-1 blocks "ls -1 with the path form: ...", "get -R with the path form: ...", "put with the ... path form", "rename, ..." and "pwd and a listing of the login directory".

- **Use `/C:/Program Files/...`.** The server's home is `/C:/Users/svc-ignition-deploy` (`pwd`). `ls -1`, `get -R`, `put` and `rename` all work with the `/C:/...` form, with a trailing slash, with a lowercase path (`/c:/program files/...`, case-insensitive, echoed back as given) and with the drive-less form `/Program Files/...` (same files).
- **`C:/...` without the leading slash is a RELATIVE path**, resolved against the home: `Can't ls: "/C:/Users/svc-ignition-deploy/C:/Program Files/..." not found`. The same for `C:\...`. `/C:\Program Files\...` (backslashes after the drive) is not found either (stderr shows the backslashes doubled). So `windows_sftp_root` must produce `/` + drive letter + `:` + forward-slash path; a backslash path is never sent. (safe_remote_path already refuses backslashes; keep it.)
- A RELATIVE path with backslashes (`ign-c7a\put-bs.txt`, `ign-c7a\d\r1-moved.txt`) is accepted as separators by `put` and `rename`. This is a tolerance, not a form to rely on.
- Paths are resolved against the home, so relative names (`ign-deploy/...`) are home-relative. `ls -1 "/"` lists only `/C:`; `ls -1 "/C:/"` lists the drive root.
- `ls -1 <dir>` prefixes every entry with the directory exactly as the caller wrote it, joined by one `/` (a trailing slash on the input does not double it). `ls -1 <file>` prints the path as given. So A5 is CONFIRMED and `parse_ls_listing` strips the given prefix, never a resolved one.

### C7a-2 — listing format (research A5)

Rests on C7a-2 blocks "ls -1 of the spaces-and-comma directory", "ls -l ...", "ls -la ...", "ls -l of a single file", "ls -1 of an empty directory", "ls -la of an empty directory", the two UTF-8-locale blocks, and "ls -l of the real collection root" / "ls -la of the real collection root".

- `ls -1 dir`: one `<dir-as-given>/<name>` per line. No `.` or `..` entries in `-1`, `-l` or `-la`. An empty directory prints nothing (exit 0).
- `ls -l dir`: `<10-char mode> <nlink> <owner> <group> <size> <Mon DD HH:MM> <name>`, for example `-rw-******    1 -        -               6 Oct 15 02:27 f one.txt` and `drwx******    1 -        -               0 Oct 15 02:27 sub dir`. The mode shows the type character and `******` for the permission bits it cannot know; owner and group are `-`; the time is the server clock (UTC on this host), `HH:MM`, no year; the size of a directory is not its contents (`0`, `4096` and `262144` were observed).
- `ls -l <single file>` has a DIFFERENT shape: `-rw-------    ? 0        0               6 Oct 14 20:27 <full path as given>` (link count `?`, uid and gid `0`, the time shown 6 hours behind the server clock, which is this client's zone, sometimes with a year in place of `HH:MM`). The listing parser must only accept the directory form and must never `ls -l` a single file for metadata.
- `ls -la` is `ls -l` plus the entries that are hidden or dot-named: the collection root shows `.resources` (a 262144-byte gateway directory) only under `-la`. The read transport already lists with `LsLongAll`, which is therefore the form that sees everything; `-1` and `-l` hide it.
- Names with spaces (`f one.txt`, `sub dir`), commas (`f,two.txt`, `dir with space, comma`) list and transfer intact. A non-ASCII name (`café.txt`) lists raw when the sftp CLIENT runs in a UTF-8 locale and as `caf\303\251.txt` (the client's own escaping) when the client runs with `LC_ALL=C`. **Consequence for 22-06/22-13:** the process runner must set a UTF-8 locale for `sftp`, or the parser must treat a `\ooo` octal-escaped name as an error; it must not silently accept an escaped name as the real one. `get -R` in a UTF-8 locale writes `café.txt` locally with the right bytes (sha256 `ebc45fab...`).
- `cmd.exe`'s `dir` writes non-UTF-8 bytes (the OEM code page 437: `caf\x82.txt`), see C7a-6.

### C7a-3 — error signalling (stdout/stderr and exit codes of `sftp -q -b -`)

Rests on C7a-3 blocks "not found: ...", "permission denied: ...", "mkdir of an existing directory", "rmdir of a non-empty directory", "sftp exit code with a failing command in the MIDDLE of a batch" and "sftp batch whose commands all succeed".

- A batch whose commands all succeed exits 0. A batch with a failing command stops at it, exits **1**, and the later commands do not run (the middle-failure block printed only the first listing). Every failure kind exits 1; the class is only in stderr. stderr lines end in CRLF.
- Texts (path in the message is the RESOLVED absolute path, `/C:/...`):
  - `ls`: `Can't ls: "<abs>" not found`
  - `get` and `get -R`: `File "<abs>" not found.`
  - `rm`: `remote delete <abs>: No such file or directory`; `rmdir`: `remote rmdir "<abs>": No such file or directory`; `rename`: `remote rename "<from>" to "<to>": No such file or directory` (source missing, or the target's parent missing)
  - `mkdir` of an existing directory and `rmdir` of a non-empty directory: `remote mkdir|rmdir "<abs>": Failure` (no reason given)
  - write denied: `put`: `dest open "<abs>": Permission denied`; `mkdir`: `remote mkdir "<abs>": Permission denied`; `rename` into a directory without write: `remote rename "<from>" to "<to>": Permission denied`
  - reading `C:\Windows\System32\config` (no read ACE): `remote readdir("<abs>/"): Bad message`
  - **A read the ACL denies on a file that exists reports NOT FOUND**, not permission denied (`get` of `C:/ProgramData/ssh/logs/sshd.log` and of `C:/Windows/System32/config/SAM` both print `File "..." not found.`), while `ls -l` of `C:/ProgramData/ssh/ssh_host_ed25519_key` shows the file (metadata is readable, content is not). **Consequence:** `not found` on a path the listing just returned means unreadable, not absent. The transport classifies by stderr text into: not_found, permission_denied, failure; and a read of a name the listing returned that answers not-found is `transport_failed` (denied or removed since), never "absent".
- Nothing was created in the resources root by the denied writes (the follow-up `dir | findstr` found no `zz-c7a` entry, `findstr` exit 1 = no match).

### C7a-4 — rename over an existing target (decides the commit sequence)

Rests on C7a-4 blocks (all of them), plus C6-2 "cross-directory rename ...".

- **File over an existing file: replaces it, atomically from the caller's view, and repeats.** `rename src dst` with both present exits 0; `dst` then holds the source content (alpha); a second rename over the same target also works (charlie). The Windows server evidently uses a replace-existing move, so the write half does NOT need remove-then-rename for files. (`.NET` `File.Move` and `Rename-Item` refuse an existing target; `Move-Item -Force` and `File.Replace(src, dst, [NullString]::Value)` replace it; see C7a-6.)
- **Directory over an existing EMPTY directory: succeeds** and the source's contents end up inside the target (`od-dst-empty\in-src.txt`).
- **Directory over an existing NON-EMPTY directory: fails** with `remote rename ... : Failure`; both directories stay intact. **File over an existing directory: fails** the same way. Consequence: a commit that swaps a unit directory must rename the old one aside (or delete it) first, or work per file; PowerShell `Rename-Item` aside then rename in worked (C7a-6, block "Directory swap").
- **Rename onto itself DELETES the file.** `rename "ign-c7a/self.txt" "ign-c7a/self.txt"` answered `No such file or directory`, exit 1, and the file was gone afterwards (sftp `ls -l` not found; `dir` File Not Found). **The write half must never issue a rename whose source and target resolve to the same path** (compare after normalisation, case-insensitively). This is a data-loss hazard on this server.
- **A target held open for read with FileShare.None: the rename fails** (`remote rename ... : Failure`, sftp exit 1) and nothing is lost (source and target both remain, target content unchanged). A scanner (Defender, backup agent) holding a handle on a destination file can produce the same refusal; the commit needs a bounded retry on `Failure`, then a classified error.
- Cross-directory rename inside the home works (`rename ign-c7a/r1-mixed.txt ign-c7a/d/r1-moved.txt`); the home (`C:\Users\...`) and the data dir (`C:\Program Files\...`) are on the same volume `C:` (C6 block "is the data dir and the home on the same volume"), so a home-to-collection rename would be a same-volume move IF permitted (it is not, C6-2).

### C7a-5 — junctions and links

Rests on C7a-5 blocks.

- `mklink /J` works unprivileged; `mklink /D` (directory symlink) is refused for the account: `You do not have sufficient privilege to perform this operation.`
- sftp shows a junction as a plain directory (`drwx******`, no link marker). `ls -l junc/` lists the target's files and `get junc/r.txt` returns them.
- **`get -R` follows junctions.** A tree with `jx/real/r.txt` and a junction `jx/junc -> real` was copied as TWO copies of `r.txt` (same sha256). With a junction pointing at its own parent (`jx/loop -> jx`), `get -R` recursed until the server's path limit and ended with several `remote readdir(...)` errors and exit 1, leaving 218 local entries (deepest local path 224 characters). It terminates (the 260 limit stops it) but it is wasteful and the client cannot tell a junction from a directory.
- **Consequence:** the Windows read transport cannot rely on the client's symlink skip. Before trusting a `get -R` tree, the transport must refuse a tree that holds a reparse point. The reparse check belongs in the one PowerShell call that already lists the tree (C7a-8): `Attributes -band ReparsePoint` per entry; an entry that is one is `transport_failed`, not followed. Removing a junction with `rmdir` removes only the link (the target's file stayed).

### C7a-6 — default shell, PowerShell exit codes and CLIXML (research A12)

Rests on C7a-host blocks "default shell: COMSPEC ...", "PowerShell version on the host" and all C7a-6 blocks.

- Default shell is **cmd.exe** (`COMSPEC=C:\Windows\system32\cmd.exe`; the registry value `HKLM\SOFTWARE\OpenSSH\DefaultShell` does not exist). `ssh -T host <cmd>` runs through `cmd.exe /c`. Exit codes propagate: `cmd /c exit 5` gives 5; an unknown command gives 1 with cmd's `'x' is not recognized ...` on stderr.
- `powershell -NoProfile -NonInteractive -EncodedCommand <b64>`: the process exit code propagates (`exit 7` gives 7; `throw` gives 1; a terminating cmdlet error gives 1). The encoded form survives cmd.exe with no quoting problem.
- **The prelude changes what a non-terminating error does.** Bare, `Write-Error 'e1'; 'after'` exits **0** and prints `after`; with `$ErrorActionPreference='Stop'` it exits **1** and prints nothing. So the prelude is required for correctness, not only for output: without it a failed step looks like success.
- **CLIXML on stderr: A12 is PARTLY CONTRADICTED.** Whenever stderr is not a console, error records are serialized as `#< CLIXML` + `<Objs ...><S S="Error">...` XML on stderr, WITH the prelude too (`throw`, `Get-Item` of a missing path, `Write-Error` with the prelude all print CLIXML and exit 1). What the prelude removes is the PROGRESS CLIXML: `Write-Progress -Activity a ...` bare prints a progress `<Objs>` record on stderr (also the module-load record `Preparing modules for first use.` on first calls, seen even for `Get-Item` and `Write-Error` bare), with `$ProgressPreference='SilentlyContinue'` it prints nothing (exit 0). `[Console]::Error.WriteLine('to-stderr')` arrives as plain text. **Consequence:** the parser must treat stderr as opaque on a non-zero exit (scrub, then show a bounded tail), decode CLIXML `<S S="Error">` text only for display, and decide success from the exit code and the stdout sentinel (`IGN-RESULT {"ok":true}` arrived intact on stdout, CRLF-terminated), never from stderr being empty.
- **PowerShell stdout is the OEM code page (437), not UTF-8**: `'café ü ñ'` came back as bytes `caf\x82 \x81 \xA4`. 22-13's `WinScript` must keep script output ASCII (JSON with `\uXXXX` escapes) or set `[Console]::OutputEncoding` in the prelude; it must not assume UTF-8.
- Fallbacks for the file-replace and directory-swap steps, as PowerShell 5.1 runs them: `Rename-Item` and `[IO.File]::Move` onto an existing name fail (`Cannot create a file when that file already exists.`); `Move-Item -Force` replaces; `[IO.File]::Replace(src, dst, [NullString]::Value)` replaces (a plain `$null` backup path is `The path is not of a legal form`); renaming a directory aside and then renaming the new one into its place works.

### C7a-7 — command length budget

Rests on C7a-7 block "command-length ladder".

- The whole remote command string (`powershell -NoProfile -NonInteractive -EncodedCommand <b64>`) works at **8154** characters (script `LEN=2926` plus the prelude) and fails at **8162** with exit 1 and `The command line is too long.` (cmd.exe's 8191-character limit less its own wrapper). Every longer length up to 32702 fails the same way; from **32760** characters the ssh client itself fails: `exec request failed on channel 0`, exit **255**.
- UTF-16LE plus base64 expands a script 2.67 times, so the practical ceiling is a script of about **2900 characters** for a command of 8000. **Budget for 22-13: keep each `-EncodedCommand` command at or under 8000 characters (about 2900 script characters, prelude included), and treat exit 1 with `The command line is too long.` as `transport_failed` with that reason.** A larger script must be split into steps or fed some other way (the host has no `powershell -File` path that avoids an upload and execution-policy handling; decide in 22-13 whether a staged `.ps1` is acceptable).

### C7a-8 — sftp-server paths over 260 characters (silent truncation)

Rests on C7a-8 blocks, and on C5-NTFS blocks "sftp listing of the provider tag directory" and "PowerShell side".

- The server resolves no path of 260 characters or more. Absolute `ls` of a 217-character directory succeeds (and lists nothing, see the next bullet); `ls` of a 276-character directory answers `Can't ls: ... not found`; `mkdir` of a path that would exceed the limit fails (`remote mkdir "<abs>": No such file or directory`, the batch stops there); `get` of a file at 330+ characters answers `File ... not found.`
- **A directory that holds an entry whose full path would exceed the limit lists WITHOUT that entry, with exit 0 and no message.** Level 3 of the deep tree (217 chars) lists as empty although it holds a child; `get -R` of the whole tree retrieved levels 1 to 3, exited 0, and left a tree one level short (local find: levels at 63, 122 and 181 characters, nothing deeper) while PowerShell with the `\\?\` prefix showed six levels and the file (406 characters). The gateway itself wrote such a path (C5-NTFS: the 200-character tag folder, 335-character full path): `dir` and `dir /s` on cmd.exe show it, PowerShell without `\\?\` fails on it, and the sftp listing of the same directory simply does not contain it.
- **Consequence (must be addressed in 22-13, it is a correctness risk for plan, pull and verify):** an sftp listing on this host is NOT proof of completeness. The Windows read transport must cross-check the sftp tree against an independent complete enumeration: one PowerShell call that walks the resources subtree with the `\\?\` prefix and returns relative path, size and the reparse-point flag per entry (this also covers the junction check in C7a-5). Any entry in the enumeration that sftp did not return, or whose absolute path is 240 characters or more, is `transport_failed` naming the path; an sftp read must never be reported complete without that cross-check. The enumeration output must fit the command and output budget (C7a-7), so it is paged per top-level type directory.

### C7a-host — group membership, host keys and the account (D-15)

Rests on C7a-host blocks "the account groups (whoami /groups)", "the account privileges (whoami /priv)", "is the account an administrator? (net localgroup Administrators)", "host key types offered", "host key types the server offers in KEX".

- The account is in `BUILTIN\Users` (and the well-known groups `Everyone`, `NETWORK`, `Authenticated Users`, `This Organization`, `Local account`, `NTLM Authentication`) at Medium integrity; privileges: only `SeChangeNotifyPrivilege` and `SeIncreaseWorkingSetPrivilege`. `net localgroup Administrators` lists only `ignadmin`. The account is NOT an administrator, so the `administrators_authorized_keys` rule does not apply; its key authenticated on every run (the location of the key file was not read).
- Host keys offered: ED25519 (SHA256:wgjZzSMU/Ss+bnO8SgMfOY90cnutJvlAuxSGNPJNkZo), ECDSA nistp256 (SHA256:CfSLIMXEfRsxKLydfjt7/I6skou0EBb0a4tBGBYs6D0), RSA 3072 (SHA256:JFUQNah6KkrSZEA1I/BonrLJ6MVu4mTWENEgbEBgoYc); the KEX debug shows the server's host key algorithms as `rsa-sha2-512,rsa-sha2-256,ssh-rsa,ecdsa-sha2-nistp256,ssh-ed25519`. A `known_hosts` that pins only the ed25519 key is enough: with `StrictHostKeyChecking=yes` the client asked for and verified `ssh-ed25519` on every run. The committed `known_hosts` may hold one line per type; one ed25519 line is sufficient.
- sshd banner: `SSH-2.0-OpenSSH_for_Windows_8.1`.
- The Windows-client `ssh.exe` stdin item is NOT applicable to this macOS runner (recorded in the section prose); it stays with the windows-latest CI leg.

### C6 — what `svc-ignition-deploy` can write and read (Decision B inputs; research A16)

Rests on `## C6`: blocks under C6-acl, C6-1, C6-2, C6-3, C6-4, C6-scan-before/after and C6-cleanup. The account was provisioned (per the task) with read-and-execute on `data\config\resources` and no write access anywhere under the data dir; C6 measures what that is.

| Question | Answer | Evidence |
| --- | --- | --- |
| Create and write a home-relative staging directory (`ign-deploy/`) | **YES.** `mkdir`, `put`, `rename` inside it, `ls -l`, `rm`, `rmdir` all worked, by relative and by `/C:/Users/...` absolute path; the account has Full control on its home (`icacls`: `svc-ignition-deploy:(OI)(CI)(F)`) | C6-1 blocks, "icacls the account home" |
| Write directly into `<data_dir>\config\resources\production\` | **NO.** `mkdir zz-c6-probe`, `put` of a file straight into it, and a cross-directory `rename` of a staged FILE and of a staged DIRECTORY into it all answer `Permission denied`. `production` holds only `config-mode.json` (133 bytes) | C6-2 blocks; ACL: `svc-ignition-deploy:(I)(OI)(CI)(RX)` inherited from `resources`, `BUILTIN\Users:(I)(RX)`, no write ACE |
| Write `<data_dir>\ign-deploy\` | **NO.** `mkdir` is `Permission denied` for `data\ign-deploy`, `data\config\ign-deploy` and an arbitrary name under `data`; `data\ign-deploy` does not exist (`icacls` cannot find it). Nor can the resources root or a new collection directory be created (`Permission denied`) | C6-3 blocks, C6-2 "the same probes against the collection root" and "mkdir the collection directory" |
| Read `core\` and `production\` (research A16) | **YES.** `ls -1` of `core`, `core\ignition`, `production`, `tag-definition`; `get -R` of `core\ignition\security-properties` (2 files, sha256 `7321dabb...`, `bc15af88...`) and of `production`; the effective-view basis is readable, A16 is CONFIRMED | C6-4 blocks |
| Is a staging directory ever read as a collection | Home-relative: **no**. The gateway reads the collections under `data\config\resources\` (here `core`, `external`, `local`, `production`); the home directory is outside `data\`. The scan state was identical before and after the probes (`lastScanTimestamp 1792030196522`, `lastScanDuration 639`, `scanActive false`), and no probe wrote anything inside a collection | C6-scan-before, C6-scan-after |
| Same volume | Home `C:\Users\svc-ignition-deploy` and data dir `C:\Program Files\...` are both on `C:` | "is the data dir and the home on the same volume" |

Everything created was removed in the same session (C6-cleanup blocks; `production` listed `config-mode.json` only afterwards).

**What C6 does NOT yet settle, and the user must see:** as provisioned, the account cannot write the mode collection. Both options commit into `data\config\resources\<mode>\` (B(i) by renaming from the home, B(ii) by writing in place), and design-spec section 9 scopes the account to write exactly that directory. So an administrator must grant `Modify` on `data\config\resources\production\` (and on every mode collection a node uses) before ANY write flow can be exercised against this host; that grant is outside this plan and was not made. B(ii) additionally needs `data\ign-deploy\` (or another ledger directory) created and granted, which the ROADMAP rule's option text did not anticipate.

### C5-NTFS — on-disk names the 8.3.6 gateway writes on NTFS (deferred by 21-CONTEXT D-04)

Rests on `## C5-NTFS` (the native `tags/import` run) and attempt 1. The host has NO WebDev module, so `ign tags import` (WebDev routes) answered 404 (attempt 1, nothing captured); the capture used the gateway's native `POST /data/api/v1/tags/import?provider=CAP22H&type=json&collisionPolicy=Abort` with a `{"tags":[...]}` body (the first native try sent a bare array and the gateway answered `Not a JSON Object`; that run's blocks were replaced by the correct run, and its nine `Error importing tags` log entries, timestamps 1792032458213 to 1792032460428, still show in the log block). Imported one folder at a time into provider `CAP22H`, waited 20 s, listed over sftp, cmd.exe and PowerShell, then deleted the provider (the directory is gone afterwards).

| Folder name sent | On-disk directory name | Log verdict |
| --- | --- | --- |
| `plain` | `plain` | none |
| `CON` | `%43%4f%4e` (every byte percent-encoded, lowercase hex) | none |
| `aux` | `%61%75%78` | none |
| `NUL` | `%4e%55%4c` | none |
| `lpt1` | `%6c%70%74%31` | none |
| `trail ` (trailing space) | `trail%20` | none |
| `trail.` (trailing dot) | `trail%2E` (directory with `tags.json` and `unary-resource.json` exists) | `ERROR tags.management.provider: Error processing edit for tag path '[CAP22H]trail.': Error_Configuration("The name 'trail.' is not a valid tag name")` |
| `a:b` | `a%3ab` (same as the Linux rig, CD6) | none |
| 200 × `L` | `LLLL...` (200 characters, raw; full path 335 characters) | none; sftp listing OMITS it (C7a-8), PowerShell without `\\?\` cannot open it, cmd.exe `dir` lists it with the 8.3 name `LLLLLL~1` |

- The import answered `successCount 0, failureCount 0, failures []` for all nine, including `trail.`, whose refusal shows only in the gateway log. Directories for `CON`, `aux`, `NUL`, `lpt1` exist and were created by the gateway: it percent-encodes the Windows reserved device names (the whole name, not just one character), a trailing space and a trailing dot, and the colon. A gateway-written tag directory name on NTFS therefore never contains `< > : " | ? *`, never ends in a space or dot, and is never a reserved device name; it can be 200 characters.
- Difference from Phase 21 CD6 (Linux rig, WebDev import): there `trail.` produced no directory; here, via the native route, `trail%2E` exists even though the log calls the name invalid. Not resolved here; whichever route wrote it, `pull` must treat a directory name it finds on disk as data (percent-decoding is the layout inverse, 22-08) and `plan` already ignores nothing by name.
- **Render stays fail-closed on Windows-illegal names** (21-CONTEXT D-04): this capture does not relax it. A repo name that equals one of the gateway's encoded forms is the encoded form, not the original.
- Reserved-name handling matters for the TRANSPORT: `%43%4f%4e` is an ordinary name for sftp and cmd.exe, so encoded names need no special case in 22-13; a RAW `CON` can never be created by this account (not attempted).
- Entries whose total path exceeds 259 characters exist on disk and are invisible to the sftp read path (C7a-8). A tag tree with a long folder name therefore needs the C7a-8 cross-check before the Windows `plan` can call its read complete.

### Decision B: computed recommendation by the ROADMAP rule (the choice is the user's, at the Task 3 checkpoint)

The rule (ROADMAP v1.6, Decision B): choose (i) home-relative staging when C6 confirms home write access and no collection-read risk; otherwise (ii).

- **Computed result: (i).** C6 confirms home write access (C6-1: create, write, rename, remove, by relative and absolute path) and shows no collection-read risk (the home is outside `data\`; the scan state did not change). Under the rule's own wording the answer is B(i).
- **What the rule did not see:** (a) the rule counts the staging side only; the commit side needs the account to write the collection, which it cannot as provisioned (C6-2) in EITHER option; (b) B(ii) additionally needs `data\ign-deploy\` writable (C6-3: denied, and it does not exist); (c) the C7a-4 facts shape the commit sequence in both options (file rename replaces, directory rename over a non-empty directory fails, self-rename deletes, a held target fails with `Failure`).
- The ledger and journal location each option implies:
  - **B(i):** `ign-deploy/<gateway>/<mode>/` relative to the service account's home, which is `C:\Users\svc-ignition-deploy\ign-deploy\<gateway>\<mode>\` (sftp `/C:/Users/svc-ignition-deploy/ign-deploy/<gateway>/<mode>/`) for ssh and sftp nodes; `LocalFs` nodes keep `<data_dir>/ign-deploy/<gateway>/<mode>/` (the current `ledger_location` is relative to `data_dir`), so `ledger_location` returns a transport-specific root. Staging for the same run is `ign-deploy/<gateway>/<mode>/staging/<run_id>/` in the same home, outside every collection. A ledger is per service account, not per gateway: a second deploy account would not see the first one's ledger.
  - **B(ii):** `<data_dir>\ign-deploy\<gateway>\<mode>\` (sftp `/C:/Program Files/Inductive Automation/Ignition/data/ign-deploy/<gateway>/<mode>/`), one location for every transport (the current `ledger_location`), requiring an administrator to create `data\ign-deploy\` and grant the account `Modify` on it. No staging directory: the mirror writes in place under the scan lock.
- **Undo cost:** until Phase 23 ships, nothing writes a ledger or a journal on any host, so changing the choice now costs a plan edit (`ledger_location`, the default `StagingMode`, 22-13). After Phase 23 apply has written ledgers and journals on hosts, moving them is a one-way migration across runners and hosts (the ledger format v1 and its location are a contract read by every later plan, apply and rollback, D-05). B(i) also needs the design-spec section 9 amendment in whk-environment-orchestration (outside this repository) before it is implemented; B(ii) needs none but needs the `data\ign-deploy\` grant.
- **Hold budget effect (measured in 22-14):** B(i) keeps the upload outside the scan lock, so the lock covers only the renames; B(ii) runs the whole transfer under the lock, so its hold timeout scales with tree size and link speed.

### Decision B (recorded 2026-10-15 by the user at the Task 3 checkpoint): option B(i), home-relative staging

**Decision B: B(i).** Phase 23 stages in the service account's home, outside every collection, and commits by renaming into the mode collection. This matches the computed recommendation above. It rests on `## C6` blocks C6-1 (home create, write, rename and remove succeed), C6-scan-before and C6-scan-after (scan state unchanged, no collection-read risk) and "is the data dir and the home on the same volume" (both on `C:`), and on `## C7a` C7a-4 (a file renamed over an existing file replaces it; a directory renamed over a non-empty directory fails; a same-path rename deletes the file; a held target fails with `Failure`).

**Ledger and journal location (confirmed as proposed):**
- ssh and sftp nodes: `ign-deploy/<gateway>/<mode>/` relative to the service account's home. On the Windows capture host that is `C:\Users\svc-ignition-deploy\ign-deploy\<gateway>\<mode>\`, sftp form `/C:/Users/svc-ignition-deploy/ign-deploy/<gateway>/<mode>/`.
- Staging for a run: `ign-deploy/<gateway>/<mode>/staging/<run_id>/` in the same home (sftp form `/C:/Users/svc-ignition-deploy/ign-deploy/<gateway>/<mode>/staging/<run_id>/`).
- LocalFs nodes keep `<data_dir>/ign-deploy/<gateway>/<mode>/`. `ledger_location` therefore returns a transport-specific root; the current data_dir-relative function is the LocalFs case only. 22-13 makes that change.
- A ledger is per service account, not per gateway: a second deploy account on the same gateway does not see the first one's ledger.

**Caveats recorded with the decision:**
1. **The design-spec section 9 amendment in whk-environment-orchestration is still to do, and it is outside this repository.** Section 9 scopes `svc-ignition-deploy` to write `data/config/resources/<mode>` only; B(i) also needs the account to write `ign-deploy/` in its own home. That amendment must exist before Phase 23 implements the write flow.
2. **The account needs `Modify` on each mode collection directory.** As provisioned (C6-2) it cannot write `production\`, and B(i)'s commit step (rename into the collection) is denied without that grant: staged file and directory renames into `production` answered `Permission denied`. An administrator must grant `Modify` on every mode collection a node uses.
   - **Status of that grant on the Azure VM:** it had NOT been made when this entry was written; see the 22-12 SUMMARY for what happened. Until it is made and verified, the C6 answers above still describe the account as originally provisioned, which is the only state captured.

**Commit-sequence rules inherited from C7a-4** (for 22-13 and Phase 23): replace files by renaming over them; never rename a path onto itself; rename a directory aside (or delete it) before renaming a new one into its place; retry a `Failure` rename a bounded number of times, then classify it.

**Undo cost:** changing this now is a plan edit (`ledger_location`, the default `StagingMode`, 22-13). After Phase 23 writes ledgers and journals on hosts, moving them is a one-way migration across runners and hosts.

## Decisions: OQ5 timings, hold budget and timeout defaults (22-14)

Rests on `## OQ5 run A (Defender exclusion off)` and `## OQ5 run B (Defender exclusion on)` above. Host: the Azure VM `ign-win-836` standing in for WFLWVM01 (22-CONTEXT A-01), Windows Server 2022, Win32-OpenSSH 8.1, Ignition 8.3.6, driven from a macOS runner over the public internet as `svc-ignition-deploy`. Payload: the sanitized corpus at **scale 1** (70 named units, 152 files, 91,833 bytes), because the OQ5 source is `none` (A-03) and a ten-times payload costs about 75 minutes per iteration on this link (see run A). Every figure below is measured at 152 files; every 1,520-file figure is a labelled extrapolation. Sample counts: 10 stage/commit/scan iterations and 20 reads per run (n = 10 makes the nearest-rank p95 the maximum). Run B is attempt B2; attempt B1 (4 iterations) is in the run B section and is not pooled. One A/B pair was taken, not repeated.

### OQ5-1 — p50 and p95 for read, stage, commit and scan, both runs (seconds)

| Phase | n | A p50 | A p95 | B p50 | B p95 |
|---|---|---|---|---|---|
| connect (one sftp session, `pwd`) | 20 | 1.575 | 1.688 | 2.010 | 2.423 |
| read of the scoped collection (46 core + 152 mode files, 117,914 bytes) | 20 | 164.120 | 181.155 | 169.436 | 214.448 |
| stage (152 files into the account-home staging, B(i)) | 10 | 78.012 | 83.265 | 84.490 | 100.471 |
| commit (152 renames into `production`) | 10 | 244.594 | 251.063 | 285.193 | 348.225 |
| scan, wall, payload present | 10 | 0.792 | 0.888 | 0.520 | 0.642 |
| scan, gateway `lastScanDuration`, payload present | 10 | 0.601 | 0.676 | 0.301 | 0.338 |
| commit of the delete set (152 files, 70 directories) | 10 | 108.702 | 114.182 | 111.838 | 128.288 |
| scan, wall, after delete | 10 | 0.680 | 0.981 | 0.378 | 0.562 |

What the numbers say:
- Time is per file and per session, not per byte: 91 KB took 78 to 100 s to stage, 234 to 348 s to commit and 155 to 221 s to read. Per file that is about 0.5 s (stage), 1.6 to 2.3 s (commit: a rename costs three to four times a put), 1.0 to 1.4 s (read) and 0.7 to 0.8 s (delete). The cause is not measured here (per-op latency on this link, the Windows rename retry path of 22-13, or Defender are each possible); do not read one into these numbers.
- A config scan is not the bottleneck: it completes in under a second on this gateway, with and without 152 extra resources, on an expired and on a live trial.
- **Extrapolation, not measurement:** at the same per-file rates a 1,520-file tree would take roughly ten times as long (commit about 2,500 to 3,500 s, read about 1,600 to 2,200 s, stage about 780 to 1,000 s). Linear scaling is assumed and was not measured.

### OQ5-2 — the hold budget: `holdTimeout_s = ceil(2 x (p95 commit + p95 scan) + 1)`, floor 10 s, ceiling 300 s

| Run | Defender state | p95 commit | p95 scan (wall) | formula | result |
|---|---|---|---|---|---|
| A | exclusion off | 251.063 | 0.888 | ceil(2 x 251.951 + 1) | **505 s** |
| B | exclusion on | 348.225 | 0.642 | ceil(2 x 348.867 + 1) | **699 s** |

- **Both values are above the 300 s ceiling. This is recorded as an OPEN BLOCKER FOR PHASE 23, not clamped** and not replaced by 300: a hold of 300 s would lapse while a 152-file commit is still renaming (the commit alone takes 242 to 348 s here), which is the failure the lock exists to prevent. The spec's 5 s figure (a local `docker cp`) is off by two orders of magnitude on this transport, and the 1,520-file extrapolation (commit 2,500 to 3,500 s, hold roughly 5,000 to 7,000 s by the same formula) is further out.
- **Defender state to assume for Phase 23:** run B's 699 s (exclusion on) as the working figure. It is the larger measured value, and the exclusion did not make the commit faster (OQ5-3), so assuming the smaller figure of A would be the unsafe choice. Floor 10 s is not reached.
- Under Decision B(i) the lock window is the commit (renames), the upload is outside it (stage 83 to 100 s of the totals above is not under the lock). That is already the case in these numbers: the commit is the dominant part, and it is a sequence of per-file sftp renames. Phase 23 must therefore either (a) shorten the locked window (the measured commit of 152 files already costs minutes; whether fewer, directory-level renames or a pipelined batch would shorten it is a question this data raises and does not answer), (b) decide a larger ceiling than 300 s with the user, or (c) hold a lock that is released by the scan the deploy triggers (D12 in 20-LIVE-CAPTURES: the lock ends when the holder's scan runs) and budget the transfer separately. Which one is Phase 23's planning decision.

### OQ5-3 — Defender exclusion recommendation, with the measured p95 delta

Exclusions were `C:\Program Files\Inductive Automation\Ignition\data` and `C:\Users\svc-ignition-deploy\ign-deploy` (run B header), real-time protection still on.

| Phase | p95 A to B | p50 A to B |
|---|---|---|
| connect | 1.688 to 2.423 (+0.7 s) | 1.575 to 2.010 |
| read | 181.155 to 214.448 (+33.3 s, +18%) | 164.120 to 169.436 (+3%) |
| stage | 83.265 to 100.471 (+17.2 s, +21%) | 78.012 to 84.490 (+8%) |
| commit | 251.063 to 348.225 (+97.2 s, +39%) | 244.594 to 285.193 (+17%) |
| delete-set commit | 114.182 to 128.288 (+14.1 s, +12%) | 108.702 to 111.838 (+3%) |
| scan, gateway-measured | 676 to 338 ms (about halved) | 601 to 301 ms |

- **Recommendation: do not recommend a Defender exclusion for the transport on timing grounds.** The exclusion did not make any transport phase faster; every one was slower in run B (connect is a bare ssh session with no files, and is also slower, which points at network and host variation between the two runs rather than at Defender). The only improvement is the gateway-measured scan, about 300 ms either way, which is irrelevant against a commit of minutes.
- The honest limit: one A/B pair on a WAN link, taken in different hours, with the gateway trial reset between and during them. Differences of the size seen here are not distinguishable from run-to-run noise, so the data does not show that Defender costs time on this host, and it does not show that an exclusion is worth its attack-surface cost. The hypothesis that Defender holding files explains the slow commit is therefore not supported.
- Not applied anywhere: the exclusions stay as the user set them on the capture host; for a production host the recommendation is "none for this reason", and the administrator decides.

### OQ5-4 — the D-04 timeout defaults, revisited

Rule (22-14): `DEFAULT_OP_TIMEOUT_S` at least three times the larger of read p95 and commit p95, rounded up to 30 s; `DEFAULT_CONNECT_TIMEOUT_S` stays 10 unless the captured connect times exceed it.

- Larger p95 across both runs: B commit 348.225 s. 3 x 348.225 = 1044.7 s, rounded up to the next multiple of 30 = **1050 s**. (Run A alone would give 3 x 251.063 = 753.2, so 780 s; the larger p95 decides.) Within the accepted range 1..=3600.
- **`DEFAULT_OP_TIMEOUT_S` changes from 120 to 1050.** The old 120 s was below every measured commit (234 to 348 s) and read (155 to 221 s) of even this 152-file tree; those calls would have been killed as `transport_failed`. Landed in `crates/ignition-core/src/deploy/target.rs` (constant, doc comment, pinned test with a range check) and the README `nodes.*.ssh.op_timeout_s` row in the same commit; `cargo test -p ignition-core --lib deploy::target` is green.
- **`DEFAULT_CONNECT_TIMEOUT_S` stays 10.** Over 60 measured sessions (A 20, B2 20, B1 20) the slowest was 3.026 s.
- **Carry-forward (not a defect of the default):** the 1,520-file extrapolation (commit about 2,500 to 3,500 s) needs more than the 1050 s default and, at the top of the estimate, reaches the 3600 s upper bound of the accepted `op_timeout_s` range. A tree that size must set `op_timeout_s` explicitly, and the range cap itself would bind; whether to raise it, or to split a commit into calls that each fit, belongs with the Phase 23 hold-budget decision (OQ5-2).

### Exit gate (ROADMAP v1.6, before Phase 23 is planned)

Decision B is recorded (B(i), `## Decisions: Windows OpenSSH host (22-12)`, section "Decision B (recorded 2026-10-15 ...)"). The measured hold budget is recorded here: **505 s without a Defender exclusion, 699 s with one, both above the 300 s ceiling, so the budget replaces the spec's 5 s figure as an open blocker with these measurements attached, not as a number to clamp.** Phase 23 planning must open with that decision.
