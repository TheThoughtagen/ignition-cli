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
| `ignition/cobranding`, `edge-system-properties`, `gateway-network-proxy-rules`, `gateway-network-queue-settings`, `gateway-network-settings`, `general-alarm-settings` (the tracer unit, `production`), `quickstart`, `security-properties`, `system-properties` | equal | none |
| `ignition/database-driver/{MySQL,Oracle Database,SQLite}`, `identity-provider/default`, `keyboard_layout/*`, `opc-connection/Ignition OPC UA Server`, `schedule/Example`, `user-source/{default,opcua-module}` | equal | none |
| `ignition/database-translator/{GENERIC,MSSQL,MYSQL,ORACLE,SQLITE}` | equal | none |
| `ignition/database-translator/POSTGRES` (the tracer render, `production`) | equal AFTER **N1** | **N1**: drop every object member whose value is JSON `null` from the file value before comparing. The file holds `createAutoIncSequence` and `createAutoIncTrigger` as `null`; the REST view omits both. Applies to every type; compare after the drop, so a REST `null` member would still differ |
| `ignition/security-levels` | DIFFERS | **N2**: the REST view is the file's tree MERGED with gateway-provided levels (`SecurityZones/Default` and the `Roles` branch appear only in REST; array order differs). Compare as a name-keyed subset: every object in the file's `children` arrays, matched by `name`, must appear in the REST tree with its members. The captured file is a name-keyed subset of the REST value (`file_is_name_keyed_subset_of_rest: true`). Do not require equality |
| `ignition/translations` | resource.json `files: []`, no `config.json` | **N3**: a unit whose `resource.json` has `files: []` has no `config.json`; the REST `config` is built from `resource.json` `attributes` (`caseInsensitive`, `ignorePunctuation`, `ignoreTags`, `ignoreWhitespace`, all equal to the attributes) plus an empty `terms`. Compare the attributes-backed keys, ignore `terms` |
| `perspective/themes/{dark,light}`, `ignition/database-driver/{MariaDB,Microsoft SQLServer,PostgreSQL}`, `ignition/schedule/Always` | REST `collection` is `system` | gateway-provided, not file-backed: no file to compare; `read_back` skips an item whose effective collection is `system` |
| `com.inductiveautomation.opcua/one-time`, `ignition/images` | `GET list` and `GET singleton` both answer 404 | no REST read route: `read_back` reports `not_checked` for these types |

- Tag types are out of scope here (compared through the WebDev `tags` rows, not the effective view).
- **Not exercised on this rig** (so the table says nothing about them): `database-connection` (only as a uuid-less probe, config not compared), `oauth2-client`, `alarm-notification-profile`, `secret-provider`, `service-connector`, `roster-config`, `holiday`, and any type with a secret-typed field (the REST value of a `Referenced` secret was not compared). The N1 rule is stated for all types; a false red on an unexercised type is fixed by adding a row, not by loosening equality (Pitfall 6). 22-14 runs the finished verb on the prod-sized payload and should extend this table.
- `lastModification` and the other `attributes` keys are not part of `config` and are not compared.

### CV3 — A scan does not rewrite plain-file-write resource files (post-scan bytes equal rendered bytes)

Rests on `CV3`, blocks "sha256 of the rendered files", "sha256 of the same files in production/ right after the scan", the six "sha256 in production/ at +5 s ... +30 s" blocks and "byte comparison ... (cmp)".

- The four files of the tracer render (`database-translator/POSTGRES/{config,resource}.json`, `general-alarm-settings/{config,resource}.json`), written by `cp -R` into `production/`, have the same sha256 in the render directory, right after the scan, and at every 5 s step up to 30 s after it (`3ce93dcc...`, `81f9e836...`, `ce86cc6f...`, `736c80f4...`). `cmp` reports all four identical.
- Decision for 22-10 check `files` and for Phase 23: after a scan the effective files on disk equal the rendered bytes; `files` can compare sha256 of the post-scan bytes to the rendered sha256 and treat any difference as a finding. The Phase 21 CD2 result (omitted and stale `lastModificationSignature` not rewritten) points the same way and is consistent with this capture.
- Limits: four signature-bearing files from one render, one scan, 30 s. Tag files (the gateway writes those itself and flushes them asynchronously, Phase 21 CD4) and gateway-created resources are not covered; this rig had no hand-written tag files.
