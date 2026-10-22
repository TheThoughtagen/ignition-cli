# Phase 25 — Live Captures (8.3.6 rig)

| Field | Value |
| --- | --- |
| Date (UTC) | 2026-10-22 |
| Image | inductiveautomation/ignition:8.3.6 (platform linux/amd64, emulated on an arm64 host) |
| Compose project | ign-deploy-rig (compose.yml, this fixture directory) |
| deploymentMode | production |
| Rig started (UTC) | 2026-10-22T13:49:24Z |
| Docker server | 29.8.2 |
| Docker Compose | 5.5.0 |
| Runner | capture25.sh (crates/ignition-core/tests/fixtures/live-rig-deploy) |

Rule: every captured block is preceded by the exact command and a UTC time taken by the driver just before it. A request line names the token variable it used, never the token. Tokens live in a 0600 file outside the repo. This file holds member names, sizes, sha256 values, statuses and counts only: no token value and no project file content. Every project the captures create is named `CAP25-*` and is deleted at the end of the block that made it.

## Captures Owed

Written before any capture block (capture first, as in Phases 20 to 24). Each row names the question, the CONTEXT decision or research finding it settles, and the plan that reads the record.

| ID | Question | Settles | Read by |
| --- | --- | --- | --- |
| C8a | Does `import?overwrite=true` delete resources the zip omits (a dropped member, a dropped folder, a dropped `resource.json`-listed file)? What does each answer list under `changes`? | Research C8a (silent emptying on overwrite); contract item K5 | 25-03, 25-11 |
| C8b | Does a gateway-produced export survive export, overwrite import, export unchanged? Which members and fields still differ before and after `resource.json` normalisation, `project.json` included? | D-09 canonical hash; contract item K4 | 25-06, 25-11 |
| C8c | Which layout does the importer adopt: the checkout layout (no `resources/` segment) or the `member_path`-adapted export layout? Which layout does the export come back in? Both for a synthetic checkout and for the WHK-Global allowlist zip. | Research F1, D-09 (zip the repo root); contract item K1 | 25-04, 25-06 |
| C8d | What does the gateway do with a file present but not listed in its folder's `resource.json`, a dotfile, an empty directory, a folder with no `resource.json`, a `*.py` sidecar, a `__pycache__` directory, and a listed file the zip lacks? | Research F2 (Assumptions A2, A3); contract items K2, K3 | 25-06 |
| C8e | What does import answer for a `parent` naming a missing project, the project itself, and a non-inheritable project? Does `projects/list` show a disabled project and an inheritable parent? | D-05 (parent presence from the list or from `find`); Assumption A5; contract item K6 | 25-10 |
| C9 | What does a child project do after its parent is overwritten? | D-06; contract item K7 | 25-15 |
| C10 | Does the git module interfere with a project import? | D-11; contract item K8 | 25-17 |
| C-H | What does a host `.git` probe and the deploy account's rights on `data/projects` answer? | DPRJ-04, research F3; contract item K9 | 25-07, 25-08 |

C8a to C8e are recorded by 25-01 (this plan). C9, C10 and C-H are owed to 25-02.

## C8c

C8c: which layout the 8.3.6 importer adopts. A synthetic checkout (project.json, one script folder, one Perspective view) is zipped in the checkout layout (no `resources/` segment) and in the `member_path` export layout (`<collection>/resources/<rest>`), each imported with overwrite=false into a fresh CAP25-SYNTH-* project and again with overwrite=true, and re-exported. Member keys in the tables are user-path keys (the `resources/` segment dropped), so a checkout-layout zip and its export compare member for member; the raw member names are in the member lists above each table.

#### GET projects/list (before)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/list?limit=-1'` @ 2026-10-22T13:57:17Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 13:57:17 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 70

{"items":[],"metadata":{"total":0,"matching":0,"limit":-1,"offset":0}}
[curl http=200 total=0.009477s]
~~~

#### the synthetic checkout (relative paths)

`bash -c cd\ \"\$1\"\ \&\&\ find\ .\ -type\ f\ \|\ LC_ALL=C\ sort _ $STATE/c8/synth` @ 2026-10-22T13:57:17Z

~~~
./com.inductiveautomation.perspective/views/Cap25/resource.json
./com.inductiveautomation.perspective/views/Cap25/view.json
./ignition/script-python/cap25/code.py
./ignition/script-python/cap25/resource.json
./project.json
[exit 0]
~~~

### layout checkout

### CAP25-SYNTH-checkout — overwrite=false

#### members of the uploaded zip (synth-checkout.zip)

`bash capture25.sh members $STATE/c8/synth-checkout.zip` @ 2026-10-22T13:57:17Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/	0	-
com.inductiveautomation.perspective/views/	0	-
com.inductiveautomation.perspective/views/Cap25/	0	-
com.inductiveautomation.perspective/views/Cap25/resource.json	102	87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/	0	-
ignition/script-python/	0	-
ignition/script-python/cap25/	0	-
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/resource.json	100	cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938
project.json	59	5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354
[exit 0]
~~~

#### POST import CAP25-SYNTH-checkout overwrite=false

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/synth-checkout.zip '$BASE/data/api/v1/projects/import/CAP25-SYNTH-checkout?overwrite=false'` @ 2026-10-22T13:57:17Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 13:57:17 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 60

{"success":true,"changes":[{"name":"CAP25-SYNTH-checkout"}]}
[curl http=200 total=0.069069s]
~~~

#### GET projects/find/CAP25-SYNTH-checkout (status, parent, enabled, inheritable)

`bash capture25.sh findsel CAP25-SYNTH-checkout` @ 2026-10-22T13:57:17Z

~~~
status=200 {"parent":"","enabled":true,"inheritable":false}
[exit 0]
~~~

#### GET projects/export/CAP25-SYNTH-checkout

`curl -sS -g -D - -o '$STATE/c8/CAP25-SYNTH-checkout.1.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-SYNTH-checkout'` @ 2026-10-22T13:57:17Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 13:57:17 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-SYNTH-checkout_20261022135717.zip"
Content-Length: 1283

size_bytes=1283
sha256=a4783a8f02482a659d65a5fc4f34a0b306a6a29454dc0bd207a3fc16c8a5e2b8
[curl http=200 total=0.012727s]
~~~

#### members of the export after overwrite=false

`bash capture25.sh members $STATE/c8/CAP25-SYNTH-checkout.1.zip` @ 2026-10-22T13:57:17Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/views/Cap25/resource.json	134	34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/resource.json	132	fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05
project.json	108	9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14
[exit 0]
~~~

#### table: uploaded zip against export after overwrite=false

`bash capture25.sh cmp $STATE/c8/synth-checkout.zip $STATE/c8/CAP25-SYNTH-checkout.1.zip` @ 2026-10-22T13:57:18Z

~~~
member (user-path key) | in zip | in export | sha256 zip | sha256 export | equal | equal after normalisation
com.inductiveautomation.perspective/views/Cap25/resource.json | yes | yes | 87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1 | 34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e | no | yes
com.inductiveautomation.perspective/views/Cap25/view.json | yes | yes | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | yes | yes
ignition/script-python/cap25/code.py | yes | yes | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | yes | yes
ignition/script-python/cap25/resource.json | yes | yes | cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938 | fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05 | no | yes
project.json | yes | yes | 5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354 | 9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14 | no | no
directory entries: zip 6, export 0
project.json differing key names: description, parent
[exit 0]
~~~

### CAP25-SYNTH-checkout — overwrite=true

#### POST import CAP25-SYNTH-checkout overwrite=true

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/synth-checkout.zip '$BASE/data/api/v1/projects/import/CAP25-SYNTH-checkout?overwrite=true'` @ 2026-10-22T13:57:18Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 13:57:18 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 60

{"success":true,"changes":[{"name":"CAP25-SYNTH-checkout"}]}
[curl http=200 total=0.039851s]
~~~

#### GET projects/export/CAP25-SYNTH-checkout (after overwrite=true)

`curl -sS -g -D - -o '$STATE/c8/CAP25-SYNTH-checkout.2.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-SYNTH-checkout'` @ 2026-10-22T13:57:18Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 13:57:18 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-SYNTH-checkout_20261022135718.zip"
Content-Length: 1283

size_bytes=1283
sha256=faaeeac774f98362d27798d6705272b1a9e12b0377d9ab73b5396861c2124c45
[curl http=200 total=0.011051s]
~~~

#### table: uploaded zip against export after overwrite=true

`bash capture25.sh cmp $STATE/c8/synth-checkout.zip $STATE/c8/CAP25-SYNTH-checkout.2.zip` @ 2026-10-22T13:57:18Z

~~~
member (user-path key) | in zip | in export | sha256 zip | sha256 export | equal | equal after normalisation
com.inductiveautomation.perspective/views/Cap25/resource.json | yes | yes | 87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1 | 34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e | no | yes
com.inductiveautomation.perspective/views/Cap25/view.json | yes | yes | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | yes | yes
ignition/script-python/cap25/code.py | yes | yes | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | yes | yes
ignition/script-python/cap25/resource.json | yes | yes | cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938 | fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05 | no | yes
project.json | yes | yes | 5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354 | 9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14 | no | no
directory entries: zip 6, export 0
project.json differing key names: description, parent
[exit 0]
~~~

#### DELETE project CAP25-SYNTH-checkout

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-SYNTH-checkout?confirm=true'` @ 2026-10-22T13:57:18Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 13:57:18 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.012979s]
~~~

### layout export

### CAP25-SYNTH-export — overwrite=false

#### members of the uploaded zip (synth-export.zip)

`bash capture25.sh members $STATE/c8/synth-export.zip` @ 2026-10-22T13:57:18Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/	0	-
com.inductiveautomation.perspective/resources/	0	-
com.inductiveautomation.perspective/resources/views/	0	-
com.inductiveautomation.perspective/resources/views/Cap25/	0	-
com.inductiveautomation.perspective/resources/views/Cap25/resource.json	102	87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1
com.inductiveautomation.perspective/resources/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/	0	-
ignition/resources/	0	-
ignition/resources/script-python/	0	-
ignition/resources/script-python/cap25/	0	-
ignition/resources/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/resources/script-python/cap25/resource.json	100	cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938
project.json	59	5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354
[exit 0]
~~~

#### POST import CAP25-SYNTH-export overwrite=false

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/synth-export.zip '$BASE/data/api/v1/projects/import/CAP25-SYNTH-export?overwrite=false'` @ 2026-10-22T13:57:18Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 13:57:18 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 58

{"success":true,"changes":[{"name":"CAP25-SYNTH-export"}]}
[curl http=200 total=0.033725s]
~~~

#### GET projects/find/CAP25-SYNTH-export (status, parent, enabled, inheritable)

`bash capture25.sh findsel CAP25-SYNTH-export` @ 2026-10-22T13:57:18Z

~~~
status=200 {"parent":"","enabled":true,"inheritable":false}
[exit 0]
~~~

#### GET projects/export/CAP25-SYNTH-export

`curl -sS -g -D - -o '$STATE/c8/CAP25-SYNTH-export.1.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-SYNTH-export'` @ 2026-10-22T13:57:18Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 13:57:18 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-SYNTH-export_20261022135718.zip"
Content-Length: 1363

size_bytes=1363
sha256=89a34e37655c565c768bc97d4aa20c736d56bdb402a5ee73cdac7445ca3af0b1
[curl http=200 total=0.011940s]
~~~

#### members of the export after overwrite=false

`bash capture25.sh members $STATE/c8/CAP25-SYNTH-export.1.zip` @ 2026-10-22T13:57:18Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/resources/views/Cap25/resource.json	134	34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e
com.inductiveautomation.perspective/resources/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/resources/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/resources/script-python/cap25/resource.json	132	fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05
project.json	108	9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14
[exit 0]
~~~

#### table: uploaded zip against export after overwrite=false

`bash capture25.sh cmp $STATE/c8/synth-export.zip $STATE/c8/CAP25-SYNTH-export.1.zip` @ 2026-10-22T13:57:18Z

~~~
member (user-path key) | in zip | in export | sha256 zip | sha256 export | equal | equal after normalisation
com.inductiveautomation.perspective/views/Cap25/resource.json | yes | yes | 87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1 | 34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e | no | yes
com.inductiveautomation.perspective/views/Cap25/view.json | yes | yes | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | yes | yes
ignition/script-python/cap25/code.py | yes | yes | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | yes | yes
ignition/script-python/cap25/resource.json | yes | yes | cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938 | fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05 | no | yes
project.json | yes | yes | 5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354 | 9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14 | no | no
directory entries: zip 8, export 0
project.json differing key names: description, parent
[exit 0]
~~~

### CAP25-SYNTH-export — overwrite=true

#### POST import CAP25-SYNTH-export overwrite=true

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/synth-export.zip '$BASE/data/api/v1/projects/import/CAP25-SYNTH-export?overwrite=true'` @ 2026-10-22T13:57:19Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 13:57:19 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 58

{"success":true,"changes":[{"name":"CAP25-SYNTH-export"}]}
[curl http=200 total=0.032341s]
~~~

#### GET projects/export/CAP25-SYNTH-export (after overwrite=true)

`curl -sS -g -D - -o '$STATE/c8/CAP25-SYNTH-export.2.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-SYNTH-export'` @ 2026-10-22T13:57:19Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 13:57:19 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-SYNTH-export_20261022135719.zip"
Content-Length: 1363

size_bytes=1363
sha256=89a34e37655c565c768bc97d4aa20c736d56bdb402a5ee73cdac7445ca3af0b1
[curl http=200 total=0.013857s]
~~~

#### table: uploaded zip against export after overwrite=true

`bash capture25.sh cmp $STATE/c8/synth-export.zip $STATE/c8/CAP25-SYNTH-export.2.zip` @ 2026-10-22T13:57:19Z

~~~
member (user-path key) | in zip | in export | sha256 zip | sha256 export | equal | equal after normalisation
com.inductiveautomation.perspective/views/Cap25/resource.json | yes | yes | 87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1 | 34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e | no | yes
com.inductiveautomation.perspective/views/Cap25/view.json | yes | yes | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | yes | yes
ignition/script-python/cap25/code.py | yes | yes | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | yes | yes
ignition/script-python/cap25/resource.json | yes | yes | cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938 | fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05 | no | yes
project.json | yes | yes | 5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354 | 9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14 | no | no
directory entries: zip 8, export 0
project.json differing key names: description, parent
[exit 0]
~~~

#### DELETE project CAP25-SYNTH-export

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-SYNTH-export?confirm=true'` @ 2026-10-22T13:57:19Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 13:57:19 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.015298s]
~~~

### end of block

#### GET projects/list (limit=-1), end of block

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/list?limit=-1'` @ 2026-10-22T13:57:19Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 13:57:19 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 70

{"items":[],"metadata":{"total":0,"matching":0,"limit":-1,"offset":0}}
[curl http=200 total=0.005577s]
~~~

Script reading: 0 CAP25-* project(s) listed after this block; the rig list holds 0 project(s) in all.

### C8c layout-recognition probe

Question: does the gateway recognise an export-layout zip (`<category>/resources/<rest>`) as resources, or only store it as opaque paths? The synthetic round trip above shows the gateway keeps zip paths as given, which cannot tell the two apart. code.py v1 returns 1 and v2 returns 2; the sha256 of code.py tells which version a path holds. Gateway disk file names are read with `docker exec` on the rig container (names only).

#### sha256 of code.py v1 and v2 (the two versions told apart below)

`bash -c printf\ \"def\ f\(\):\\n\ \ \ \ return\ 1\\n\"\ \|\ shasum\ -a\ 256\ \|\ cut\ -d\"\ \"\ -f1\ \|\ sed\ \"s/\^/v1\ /\"\;\ printf\ \"def\ f\(\):\\n\ \ \ \ return\ 2\\n\"\ \|\ shasum\ -a\ 256\ \|\ cut\ -d\"\ \"\ -f1\ \|\ sed\ \"s/\^/v2\ /\"` @ 2026-10-22T14:01:04Z

~~~
v1 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
v2 b7fdeefd2ff2fd36afb5919c77890537a8d74c15b1fc316059fc69dfb527a93f
[exit 0]
~~~

### Probe A: import checkout layout v1, overwrite with export layout v2

#### POST import CAP25-PROBE-A overwrite=false (checkout layout, code.py v1)

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/p1-checkout.zip '$BASE/data/api/v1/projects/import/CAP25-PROBE-A?overwrite=false'` @ 2026-10-22T14:01:04Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:01:04 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 53

{"success":true,"changes":[{"name":"CAP25-PROBE-A"}]}
[curl http=200 total=0.071613s]
~~~

#### gateway disk file names of CAP25-PROBE-A (after-checkout-v1)

`bash capture25.sh gwdisk CAP25-PROBE-A` @ 2026-10-22T14:01:04Z

~~~
CAP25-PROBE-A/com.inductiveautomation.perspective/views/Cap25/resource.json
CAP25-PROBE-A/com.inductiveautomation.perspective/views/Cap25/view.json
CAP25-PROBE-A/ignition/script-python/cap25/code.py
CAP25-PROBE-A/ignition/script-python/cap25/resource.json
CAP25-PROBE-A/project.json
[exit 0]
~~~

#### GET projects/export/CAP25-PROBE-A (after-checkout-v1)

`curl -sS -g -D - -o '$STATE/c8/CAP25-PROBE-A.after-checkout-v1.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-PROBE-A'` @ 2026-10-22T14:01:05Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:01:05 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-PROBE-A_20261022140105.zip"
Content-Length: 1283

size_bytes=1283
sha256=9b511e128626205d676c64545e8c9310241c91566e8d20976ec317f1c98f86cf
[curl http=200 total=0.028625s]
~~~

#### members of the export of CAP25-PROBE-A (after-checkout-v1)

`bash capture25.sh members $STATE/c8/CAP25-PROBE-A.after-checkout-v1.zip` @ 2026-10-22T14:01:05Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/views/Cap25/resource.json	134	34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/resource.json	132	fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05
project.json	108	9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14
[exit 0]
~~~

#### POST import CAP25-PROBE-A overwrite=true (export layout, code.py v2)

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/p2-export.zip '$BASE/data/api/v1/projects/import/CAP25-PROBE-A?overwrite=true'` @ 2026-10-22T14:01:05Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:01:06 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 53

{"success":true,"changes":[{"name":"CAP25-PROBE-A"}]}
[curl http=200 total=0.077989s]
~~~

#### gateway disk file names of CAP25-PROBE-A (after-export-v2-overwrite)

`bash capture25.sh gwdisk CAP25-PROBE-A` @ 2026-10-22T14:01:06Z

~~~
CAP25-PROBE-A/com.inductiveautomation.perspective/resources/views/Cap25/resource.json
CAP25-PROBE-A/com.inductiveautomation.perspective/resources/views/Cap25/view.json
CAP25-PROBE-A/ignition/resources/script-python/cap25/code.py
CAP25-PROBE-A/ignition/resources/script-python/cap25/resource.json
CAP25-PROBE-A/project.json
[exit 0]
~~~

#### GET projects/export/CAP25-PROBE-A (after-export-v2-overwrite)

`curl -sS -g -D - -o '$STATE/c8/CAP25-PROBE-A.after-export-v2-overwrite.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-PROBE-A'` @ 2026-10-22T14:01:06Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:01:06 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-PROBE-A_20261022140106.zip"
Content-Length: 1363

size_bytes=1363
sha256=e53deb0a2479d33e3325ca9cae39a2447f5fe8e03cb89d2c1442b227b009c763
[curl http=200 total=0.008423s]
~~~

#### members of the export of CAP25-PROBE-A (after-export-v2-overwrite)

`bash capture25.sh members $STATE/c8/CAP25-PROBE-A.after-export-v2-overwrite.zip` @ 2026-10-22T14:01:06Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/resources/views/Cap25/resource.json	134	34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e
com.inductiveautomation.perspective/resources/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/resources/script-python/cap25/code.py	22	b7fdeefd2ff2fd36afb5919c77890537a8d74c15b1fc316059fc69dfb527a93f
ignition/resources/script-python/cap25/resource.json	132	fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05
project.json	108	9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14
[exit 0]
~~~

### Probe B: import export layout v1, overwrite with checkout layout v2

#### POST import CAP25-PROBE-B overwrite=false (export layout, code.py v1)

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/p1-export.zip '$BASE/data/api/v1/projects/import/CAP25-PROBE-B?overwrite=false'` @ 2026-10-22T14:01:06Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:01:06 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 53

{"success":true,"changes":[{"name":"CAP25-PROBE-B"}]}
[curl http=200 total=0.048917s]
~~~

#### gateway disk file names of CAP25-PROBE-B (after-export-v1)

`bash capture25.sh gwdisk CAP25-PROBE-B` @ 2026-10-22T14:01:06Z

~~~
CAP25-PROBE-B/com.inductiveautomation.perspective/resources/views/Cap25/resource.json
CAP25-PROBE-B/com.inductiveautomation.perspective/resources/views/Cap25/view.json
CAP25-PROBE-B/ignition/resources/script-python/cap25/code.py
CAP25-PROBE-B/ignition/resources/script-python/cap25/resource.json
CAP25-PROBE-B/project.json
[exit 0]
~~~

#### GET projects/export/CAP25-PROBE-B (after-export-v1)

`curl -sS -g -D - -o '$STATE/c8/CAP25-PROBE-B.after-export-v1.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-PROBE-B'` @ 2026-10-22T14:01:06Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:01:06 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-PROBE-B_20261022140106.zip"
Content-Length: 1363

size_bytes=1363
sha256=f3353a714a3f3164aef6e74b4a528264d36a8cd438ebb32ee9ed02eb3bcc89bb
[curl http=200 total=0.008435s]
~~~

#### members of the export of CAP25-PROBE-B (after-export-v1)

`bash capture25.sh members $STATE/c8/CAP25-PROBE-B.after-export-v1.zip` @ 2026-10-22T14:01:06Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/resources/views/Cap25/resource.json	134	34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e
com.inductiveautomation.perspective/resources/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/resources/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/resources/script-python/cap25/resource.json	132	fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05
project.json	108	9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14
[exit 0]
~~~

#### POST import CAP25-PROBE-B overwrite=true (checkout layout, code.py v2)

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/p2-checkout.zip '$BASE/data/api/v1/projects/import/CAP25-PROBE-B?overwrite=true'` @ 2026-10-22T14:01:06Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:01:06 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 53

{"success":true,"changes":[{"name":"CAP25-PROBE-B"}]}
[curl http=200 total=0.034477s]
~~~

#### gateway disk file names of CAP25-PROBE-B (after-checkout-v2-overwrite)

`bash capture25.sh gwdisk CAP25-PROBE-B` @ 2026-10-22T14:01:06Z

~~~
CAP25-PROBE-B/com.inductiveautomation.perspective/views/Cap25/resource.json
CAP25-PROBE-B/com.inductiveautomation.perspective/views/Cap25/view.json
CAP25-PROBE-B/ignition/script-python/cap25/code.py
CAP25-PROBE-B/ignition/script-python/cap25/resource.json
CAP25-PROBE-B/project.json
[exit 0]
~~~

#### GET projects/export/CAP25-PROBE-B (after-checkout-v2-overwrite)

`curl -sS -g -D - -o '$STATE/c8/CAP25-PROBE-B.after-checkout-v2-overwrite.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-PROBE-B'` @ 2026-10-22T14:01:07Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:01:07 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-PROBE-B_20261022140107.zip"
Content-Length: 1283

size_bytes=1283
sha256=cf46cef80ad3c6638b9e7530f023b677108871ef22be7a9bd270b8897b7e36be
[curl http=200 total=0.018436s]
~~~

#### members of the export of CAP25-PROBE-B (after-checkout-v2-overwrite)

`bash capture25.sh members $STATE/c8/CAP25-PROBE-B.after-checkout-v2-overwrite.zip` @ 2026-10-22T14:01:07Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/views/Cap25/resource.json	134	34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/script-python/cap25/code.py	22	b7fdeefd2ff2fd36afb5919c77890537a8d74c15b1fc316059fc69dfb527a93f
ignition/script-python/cap25/resource.json	132	fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05
project.json	108	9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14
[exit 0]
~~~

### Probe C: projects/copy of an export-layout project and of a checkout-layout project

#### POST import CAP25-PROBE-C overwrite=false (export layout, code.py v1)

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/p1-export.zip '$BASE/data/api/v1/projects/import/CAP25-PROBE-C?overwrite=false'` @ 2026-10-22T14:01:07Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:01:07 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 53

{"success":true,"changes":[{"name":"CAP25-PROBE-C"}]}
[curl http=200 total=0.043315s]
~~~

#### POST projects/copy CAP25-PROBE-C -> CAP25-PROBE-CC

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/json --data {"fromName":"CAP25-PROBE-C","toName":"CAP25-PROBE-CC"} '$BASE/data/api/v1/projects/copy'` @ 2026-10-22T14:01:07Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:01:07 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.041411s]
~~~

#### gateway disk file names of CAP25-PROBE-CC (copy-of-export-layout)

`bash capture25.sh gwdisk CAP25-PROBE-CC` @ 2026-10-22T14:01:07Z

~~~
CAP25-PROBE-CC/com.inductiveautomation.perspective/resources/views/Cap25/resource.json
CAP25-PROBE-CC/com.inductiveautomation.perspective/resources/views/Cap25/view.json
CAP25-PROBE-CC/ignition/resources/script-python/cap25/code.py
CAP25-PROBE-CC/ignition/resources/script-python/cap25/resource.json
CAP25-PROBE-CC/project.json
[exit 0]
~~~

#### GET projects/export/CAP25-PROBE-CC (copy-of-export-layout)

`curl -sS -g -D - -o '$STATE/c8/CAP25-PROBE-CC.copy-of-export-layout.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-PROBE-CC'` @ 2026-10-22T14:01:07Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:01:07 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-PROBE-CC_20261022140107.zip"
Content-Length: 1363

size_bytes=1363
sha256=f3353a714a3f3164aef6e74b4a528264d36a8cd438ebb32ee9ed02eb3bcc89bb
[curl http=200 total=0.009742s]
~~~

#### members of the export of CAP25-PROBE-CC (copy-of-export-layout)

`bash capture25.sh members $STATE/c8/CAP25-PROBE-CC.copy-of-export-layout.zip` @ 2026-10-22T14:01:07Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/resources/views/Cap25/resource.json	134	34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e
com.inductiveautomation.perspective/resources/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/resources/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/resources/script-python/cap25/resource.json	132	fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05
project.json	108	9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14
[exit 0]
~~~

#### POST import CAP25-PROBE-D overwrite=false (checkout layout, code.py v1)

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/p1-checkout.zip '$BASE/data/api/v1/projects/import/CAP25-PROBE-D?overwrite=false'` @ 2026-10-22T14:01:07Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:01:07 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 53

{"success":true,"changes":[{"name":"CAP25-PROBE-D"}]}
[curl http=200 total=0.048563s]
~~~

#### POST projects/copy CAP25-PROBE-D -> CAP25-PROBE-DD

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/json --data {"fromName":"CAP25-PROBE-D","toName":"CAP25-PROBE-DD"} '$BASE/data/api/v1/projects/copy'` @ 2026-10-22T14:01:07Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:01:07 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.037891s]
~~~

#### gateway disk file names of CAP25-PROBE-DD (copy-of-checkout-layout)

`bash capture25.sh gwdisk CAP25-PROBE-DD` @ 2026-10-22T14:01:08Z

~~~
CAP25-PROBE-DD/com.inductiveautomation.perspective/views/Cap25/resource.json
CAP25-PROBE-DD/com.inductiveautomation.perspective/views/Cap25/view.json
CAP25-PROBE-DD/ignition/script-python/cap25/code.py
CAP25-PROBE-DD/ignition/script-python/cap25/resource.json
CAP25-PROBE-DD/project.json
[exit 0]
~~~

#### GET projects/export/CAP25-PROBE-DD (copy-of-checkout-layout)

`curl -sS -g -D - -o '$STATE/c8/CAP25-PROBE-DD.copy-of-checkout-layout.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-PROBE-DD'` @ 2026-10-22T14:01:08Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:01:08 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-PROBE-DD_20261022140108.zip"
Content-Length: 1283

size_bytes=1283
sha256=af9c17cc383d1c8d562e2feaa3c2d38a845008cc984ac070ff89e051508bea5d
[curl http=200 total=0.008857s]
~~~

#### members of the export of CAP25-PROBE-DD (copy-of-checkout-layout)

`bash capture25.sh members $STATE/c8/CAP25-PROBE-DD.copy-of-checkout-layout.zip` @ 2026-10-22T14:01:08Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/views/Cap25/resource.json	134	34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/resource.json	132	fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05
project.json	108	9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14
[exit 0]
~~~

### Probe cleanup

#### DELETE project CAP25-PROBE-A

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-PROBE-A?confirm=true'` @ 2026-10-22T14:01:08Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:01:08 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.009026s]
~~~

#### DELETE project CAP25-PROBE-B

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-PROBE-B?confirm=true'` @ 2026-10-22T14:01:08Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:01:08 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.014834s]
~~~

#### DELETE project CAP25-PROBE-C

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-PROBE-C?confirm=true'` @ 2026-10-22T14:01:08Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:01:08 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.016155s]
~~~

#### DELETE project CAP25-PROBE-CC

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-PROBE-CC?confirm=true'` @ 2026-10-22T14:01:08Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:01:08 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.012773s]
~~~

#### DELETE project CAP25-PROBE-D

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-PROBE-D?confirm=true'` @ 2026-10-22T14:01:08Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:01:08 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.010689s]
~~~

#### DELETE project CAP25-PROBE-DD

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-PROBE-DD?confirm=true'` @ 2026-10-22T14:01:08Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:01:08 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.007620s]
~~~

#### GET projects/list (limit=-1), end of block

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/list?limit=-1'` @ 2026-10-22T14:01:08Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:01:08 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 70

{"items":[],"metadata":{"total":0,"matching":0,"limit":-1,"offset":0}}
[curl http=200 total=0.009474s]
~~~

Script reading: 0 CAP25-* project(s) listed after this block; the rig list holds 0 project(s) in all.

### C8c layout-recognition probe: verdict

Script reading of the records above (hand-written conclusions; every fact cited is in a block above).

1. **Opaque path storage, no translation in either direction.** An export-layout zip lands on the gateway disk under the exact zip paths (`ignition/resources/script-python/cap25/code.py`, `com.inductiveautomation.perspective/resources/views/Cap25/view.json`) and exports under them; a checkout-layout zip lands under `ignition/script-python/...` and exports under that. Import, overwrite and `projects/copy` never rewrite a path (copy of PROBE-C exports in the export layout, copy of PROBE-D in the checkout layout; each copy exports the same member names the import wrote on disk).
2. **`overwrite=true` replaces the whole tree by path; layouts do not merge.** In probe A the overwrite with the export layout left none of the five checkout-layout paths on disk, and in probe B the reverse. code.py is v2 (sha256 b7fdeefd...) under the new layout only. So every member the new zip lacks is deleted (C8a, whole-project replace), and switching layout between two deploys of the same project swaps every path.
3. **Not observable from REST: whether the runtime loads resources stored under `<category>/resources/<rest>`.** The REST surface has no per-project resource read (openapi-8.3.6.json lists the core project routes `list`, `find`, `names`, `parents`, `copy`, `rename`, `export`, `import`, create, modify, delete, `scan` and `scan-lock`, plus module routes for EAM, event streams, FSQL and SFC; none reads a script or Perspective view resource), so this probe shows the gateway does not treat the two layouts as one resource set on import, copy or overwrite, not that the export layout is inert at runtime.
4. **Evidence for the native layout from outside the rig.** The WHK-Global checkout used with the git module has no `resources/` segment under `ignition/` or any module directory except `com.inductiveautomation.webdev/resources`, which is WebDev's own resource-type folder, not a layout segment.
5. **Input to 25-03 K1.** The checkout layout, zipped verbatim, is the only option consistent with every observation: the gateway stores it unchanged, a re-export reproduces the same paths, and a copy or overwrite in either layout never converts between them. The old `member_path` adaptation (`<collection>/resources/<rest>`) would store paths that differ from the repo checkout and from the paths the gateway writes for a project it creates. Runtime adoption of the export layout stays unobserved; if K1 ever depends on it, 25-19 needs a runtime-visible check.
6. **Side finding for 25-04 and 25-06.** `com.inductiveautomation.webdev/resources` is a real checkout directory, so any rule that drops a `resources` second path segment to get a user-path key (the `cmp` key function in this script, the inverse of `member_path`) mis-keys WebDev members. It is harmless for the comparisons recorded here (the same rule runs on both sides of each comparison), but the artifact builder must not strip by segment name.

### C8c WHK-Global allowlist zip

Source: `~/data/projects/WHK-Global`, copied into `$STATE/c8/whk` keeping the root `project.json` and the top-level directories named `ignition` or matching `^[a-z][a-z0-9_-]*(\.[a-z0-9_-]+)+$` (research A3). Layout: checkout (verbatim), as settled by the synthetic round trip and the layout probe. `resource.json` normalisation is the one in the script (drop `attributes.lastModification` and `lastModificationSignature`, sort keys), the same transform as `jq -S 'del(.attributes.lastModification, .attributes.lastModificationSignature)'`.

#### git status --porcelain of the WHK-Global checkout (before)

`git -C ~/data/projects/WHK-Global status --porcelain` @ 2026-10-22T14:03:28Z

~~~
?? .ignition-preview/
?? db_backup_sqlite.idb
?? e2e/lib/preflight-mes-target.mjs
?? pyrightconfig.json
?? styling/
?? test-results/
[exit 0]
~~~

### C8c WHK-Global excluded roots

#### top-level names kept (root project.json plus these directories)

`printf %s\\n com.inductiveautomation.alarm-notification com.inductiveautomation.perspective com.inductiveautomation.reporting com.inductiveautomation.vision com.inductiveautomation.webdev ignition` @ 2026-10-22T14:03:30Z

~~~
com.inductiveautomation.alarm-notification
com.inductiveautomation.perspective
com.inductiveautomation.reporting
com.inductiveautomation.vision
com.inductiveautomation.webdev
ignition
[exit 0]
~~~

#### top-level names excluded

`printf %s\\n .ai .ci-runtime .claude .coderabbit.yaml .depot .git .github .gitignore .ignition-preview .ignition-stubs .jython_cache .planning .superpowers TEC-2293 archive db_backup_sqlite.idb depot.json docs e2e images pyrightconfig.json styling tags tags.json test-results themes udts.json` @ 2026-10-22T14:03:30Z

~~~
.ai
.ci-runtime
.claude
.coderabbit.yaml
.depot
.git
.github
.gitignore
.ignition-preview
.ignition-stubs
.jython_cache
.planning
.superpowers
TEC-2293
archive
db_backup_sqlite.idb
depot.json
docs
e2e
images
pyrightconfig.json
styling
tags
tags.json
test-results
themes
udts.json
[exit 0]
~~~

#### allowlist copy: file count, symlink count, size in KiB

`bash -c cd\ \"\$1\"\ \&\&\ printf\ \"files\ %s\\nsymlinks\ %s\\n\"\ \"\$\(find\ .\ -type\ f\ \|\ wc\ -l\ \|\ tr\ -d\ \"\ \"\)\"\ \"\$\(find\ .\ -type\ l\ \|\ wc\ -l\ \|\ tr\ -d\ \"\ \"\)\"\ \&\&\ du\ -sk\ .\ \|\ cut\ -f1\ \|\ sed\ \"s/\^/KiB\ /\" _ $STATE/c8/whk` @ 2026-10-22T14:03:30Z

~~~
files 2214
symlinks 0
KiB 20156
[exit 0]
~~~

#### the zip: entries (files and directories), size in bytes, sha256

`bash -c printf\ \"entries\ %s\\nbytes\ %s\\nsha256\ %s\\n\"\ \"\$\(unzip\ -Z1\ \"\$1\"\ \|\ wc\ -l\ \|\ tr\ -d\ \"\ \"\)\"\ \"\$\(wc\ -c\ \<\ \"\$1\"\ \|\ tr\ -d\ \"\ \"\)\"\ \"\$\(shasum\ -a\ 256\ \"\$1\"\ \|\ cut\ -d\"\ \"\ -f1\)\" _ $STATE/c8/whk.zip` @ 2026-10-22T14:03:30Z

~~~
entries 3388
bytes 4978717
sha256 68e2f3185883f29fafa61f64a7bba30d601972a49a95ee57202a6280085a3c35
[exit 0]
~~~

### C8c WHK-Global — overwrite=false

#### POST import CAP25-WHK overwrite=false

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/whk.zip '$BASE/data/api/v1/projects/import/CAP25-WHK?overwrite=false'` @ 2026-10-22T14:03:31Z

~~~
HTTP/1.1 100 Continue

HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:03:31 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 49

{"success":true,"changes":[{"name":"CAP25-WHK"}]}
[curl http=200 total=20.558934s]
~~~

#### GET projects/find/CAP25-WHK (status, parent, enabled, inheritable)

`bash capture25.sh findsel CAP25-WHK` @ 2026-10-22T14:03:51Z

~~~
status=200 {"parent":"","enabled":true,"inheritable":true}
[exit 0]
~~~

#### GET projects/export/CAP25-WHK

`curl -sS -g -D - -o '$STATE/c8/CAP25-WHK.1.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-WHK'` @ 2026-10-22T14:03:51Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:03:51 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-WHK_20261022140351.zip"
Content-Length: 4773798

size_bytes=4773798
sha256=793cba57ff5f7a3868a63b5962319302b751b464c78aa653f72f600c895f9b79
[curl http=200 total=0.816756s]
~~~

#### zip against export after overwrite=false (member counts, hashes differing raw and after normalisation, first 40 zip-only and export-only keys)

`bash capture25.sh cmpsum $STATE/c8/whk.zip $STATE/c8/CAP25-WHK.1.zip` @ 2026-10-22T14:03:52Z

~~~
members (files): zip 2214, export 2213, in both 2213, zip only 1, export only 0
directory entries: zip 1174, export 0
in both, sha256 differs raw: 245
in both, sha256 differs after resource.json normalisation: 49
first 40 zip-only keys:
  com.inductiveautomation.perspective/views/Changeover/ChangeoverPanel/README.md
first 40 export-only keys:
first 20 raw-differing keys:
  com.inductiveautomation.alarm-notification/alarm-pipelines/Active Emergency Pipeline/resource.json
  com.inductiveautomation.alarm-notification/alarm-pipelines/Active Escalation Pipeline/resource.json
  com.inductiveautomation.alarm-notification/alarm-pipelines/Active Priority Pipeline/resource.json
  com.inductiveautomation.alarm-notification/alarm-pipelines/Test Pipeline/resource.json
  com.inductiveautomation.alarm-notification/alarm-pipelines/WH Standard Active Pipeline/resource.json
  com.inductiveautomation.alarm-notification/alarm-pipelines/WH Standard Clear Pipeline/resource.json
  com.inductiveautomation.perspective/session-props/resource.json
  com.inductiveautomation.perspective/views/Changeover/ChangeoverPanel/resource.json
  com.inductiveautomation.perspective/views/Changeover/Dashboard/resource.json
  com.inductiveautomation.perspective/views/Changeover/Embedded/ActionButtons/resource.json
  com.inductiveautomation.perspective/views/Changeover/Embedded/AlertPanel/resource.json
  com.inductiveautomation.perspective/views/Changeover/Embedded/AreaCard/resource.json
  com.inductiveautomation.perspective/views/Changeover/Embedded/AuditLogPanel/resource.json
  com.inductiveautomation.perspective/views/Changeover/Embedded/CipProgressBar/resource.json
  com.inductiveautomation.perspective/views/Changeover/Embedded/CookerChangeoverCard/resource.json
  com.inductiveautomation.perspective/views/Changeover/Embedded/EquipmentGrid/resource.json
  com.inductiveautomation.perspective/views/Changeover/Embedded/EquipmentTile/resource.json
  com.inductiveautomation.perspective/views/Changeover/Embedded/OrderCard/resource.json
  com.inductiveautomation.perspective/views/Changeover/Embedded/QueueRow/resource.json
  com.inductiveautomation.perspective/views/Changeover/Embedded/StateBadge/resource.json
first 20 differing keys after normalisation:
  com.inductiveautomation.perspective/views/Exchange/AlarmMessenger/Framework/Embedded/AlarmConfiguration/resource.json
  com.inductiveautomation.perspective/views/Exchange/AlarmMessenger/Framework/Page/Configure/resource.json
  com.inductiveautomation.perspective/views/Exchange/AlarmMessenger/Framework/Page/TestAlarm/resource.json
  com.inductiveautomation.perspective/views/Exchange/AlarmMessenger/Framework/Popup/DeleteConfirmation/resource.json
  com.inductiveautomation.perspective/views/Exchange/AlarmMessenger/Main/resource.json
  com.inductiveautomation.perspective/views/RosterManagement/RosterManagement/resource.json
  com.inductiveautomation.perspective/views/RosterManagement/Tools/Icon/resource.json
  com.inductiveautomation.perspective/views/RosterManagement/Tools/Users/resource.json
  com.inductiveautomation.perspective/views/RosterManagement/Views/AddEditRoster/resource.json
  com.inductiveautomation.perspective/views/RosterManagement/Views/Home/resource.json
  com.inductiveautomation.perspective/views/RosterManagement/Views/Popups/Popup_RemoveRoster/resource.json
  com.inductiveautomation.perspective/views/RosterManagement/Views/Popups/Popup_SaveError/resource.json
  com.inductiveautomation.perspective/views/ScheduleManagement/ScheduleManagement/resource.json
  com.inductiveautomation.perspective/views/ScheduleManagement/Views/AddEditHoliday/resource.json
  com.inductiveautomation.perspective/views/ScheduleManagement/Views/AddEditSchedule/resource.json
  com.inductiveautomation.perspective/views/ScheduleManagement/Views/Home/resource.json
  com.inductiveautomation.perspective/views/ScheduleManagement/Views/Popups/Popup_CheckRemoval/resource.json
  com.inductiveautomation.perspective/views/ScheduleManagement/Views/Popups/Popup_SaveError/resource.json
  com.inductiveautomation.webdev/Agent/admin/resource.json
  com.inductiveautomation.webdev/Agent/chat/resource.json
project.json differing key names: parent
[exit 0]
~~~

### C8c WHK-Global — overwrite=true

#### POST import CAP25-WHK overwrite=true

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/whk.zip '$BASE/data/api/v1/projects/import/CAP25-WHK?overwrite=true'` @ 2026-10-22T14:03:53Z

~~~
HTTP/1.1 100 Continue

HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:03:53 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 49

{"success":true,"changes":[{"name":"CAP25-WHK"}]}
[curl http=200 total=1.055995s]
~~~

#### GET projects/export/CAP25-WHK (after overwrite=true)

`curl -sS -g -D - -o '$STATE/c8/CAP25-WHK.2.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-WHK'` @ 2026-10-22T14:03:54Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:03:54 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-WHK_20261022140354.zip"
Content-Length: 4773798

size_bytes=4773798
sha256=0aaccbe2f1d28366f6fb5ad867cffaaa58449de03497d70798901b7e6eaf9674
[curl http=200 total=0.650318s]
~~~

#### zip against export after overwrite=true

`bash capture25.sh cmpsum $STATE/c8/whk.zip $STATE/c8/CAP25-WHK.2.zip` @ 2026-10-22T14:03:55Z

~~~
members (files): zip 2214, export 2213, in both 2213, zip only 1, export only 0
directory entries: zip 1174, export 0
in both, sha256 differs raw: 245
in both, sha256 differs after resource.json normalisation: 49
first 40 zip-only keys:
  com.inductiveautomation.perspective/views/Changeover/ChangeoverPanel/README.md
first 40 export-only keys:
first 20 raw-differing keys:
  com.inductiveautomation.alarm-notification/alarm-pipelines/Active Emergency Pipeline/resource.json
  com.inductiveautomation.alarm-notification/alarm-pipelines/Active Escalation Pipeline/resource.json
  com.inductiveautomation.alarm-notification/alarm-pipelines/Active Priority Pipeline/resource.json
  com.inductiveautomation.alarm-notification/alarm-pipelines/Test Pipeline/resource.json
  com.inductiveautomation.alarm-notification/alarm-pipelines/WH Standard Active Pipeline/resource.json
  com.inductiveautomation.alarm-notification/alarm-pipelines/WH Standard Clear Pipeline/resource.json
  com.inductiveautomation.perspective/session-props/resource.json
  com.inductiveautomation.perspective/views/Changeover/ChangeoverPanel/resource.json
  com.inductiveautomation.perspective/views/Changeover/Dashboard/resource.json
  com.inductiveautomation.perspective/views/Changeover/Embedded/ActionButtons/resource.json
  com.inductiveautomation.perspective/views/Changeover/Embedded/AlertPanel/resource.json
  com.inductiveautomation.perspective/views/Changeover/Embedded/AreaCard/resource.json
  com.inductiveautomation.perspective/views/Changeover/Embedded/AuditLogPanel/resource.json
  com.inductiveautomation.perspective/views/Changeover/Embedded/CipProgressBar/resource.json
  com.inductiveautomation.perspective/views/Changeover/Embedded/CookerChangeoverCard/resource.json
  com.inductiveautomation.perspective/views/Changeover/Embedded/EquipmentGrid/resource.json
  com.inductiveautomation.perspective/views/Changeover/Embedded/EquipmentTile/resource.json
  com.inductiveautomation.perspective/views/Changeover/Embedded/OrderCard/resource.json
  com.inductiveautomation.perspective/views/Changeover/Embedded/QueueRow/resource.json
  com.inductiveautomation.perspective/views/Changeover/Embedded/StateBadge/resource.json
first 20 differing keys after normalisation:
  com.inductiveautomation.perspective/views/Exchange/AlarmMessenger/Framework/Embedded/AlarmConfiguration/resource.json
  com.inductiveautomation.perspective/views/Exchange/AlarmMessenger/Framework/Page/Configure/resource.json
  com.inductiveautomation.perspective/views/Exchange/AlarmMessenger/Framework/Page/TestAlarm/resource.json
  com.inductiveautomation.perspective/views/Exchange/AlarmMessenger/Framework/Popup/DeleteConfirmation/resource.json
  com.inductiveautomation.perspective/views/Exchange/AlarmMessenger/Main/resource.json
  com.inductiveautomation.perspective/views/RosterManagement/RosterManagement/resource.json
  com.inductiveautomation.perspective/views/RosterManagement/Tools/Icon/resource.json
  com.inductiveautomation.perspective/views/RosterManagement/Tools/Users/resource.json
  com.inductiveautomation.perspective/views/RosterManagement/Views/AddEditRoster/resource.json
  com.inductiveautomation.perspective/views/RosterManagement/Views/Home/resource.json
  com.inductiveautomation.perspective/views/RosterManagement/Views/Popups/Popup_RemoveRoster/resource.json
  com.inductiveautomation.perspective/views/RosterManagement/Views/Popups/Popup_SaveError/resource.json
  com.inductiveautomation.perspective/views/ScheduleManagement/ScheduleManagement/resource.json
  com.inductiveautomation.perspective/views/ScheduleManagement/Views/AddEditHoliday/resource.json
  com.inductiveautomation.perspective/views/ScheduleManagement/Views/AddEditSchedule/resource.json
  com.inductiveautomation.perspective/views/ScheduleManagement/Views/Home/resource.json
  com.inductiveautomation.perspective/views/ScheduleManagement/Views/Popups/Popup_CheckRemoval/resource.json
  com.inductiveautomation.perspective/views/ScheduleManagement/Views/Popups/Popup_SaveError/resource.json
  com.inductiveautomation.webdev/Agent/admin/resource.json
  com.inductiveautomation.webdev/Agent/chat/resource.json
project.json differing key names: parent
[exit 0]
~~~

#### DELETE project CAP25-WHK

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-WHK?confirm=true'` @ 2026-10-22T14:03:55Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:03:55 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 49

{"success":true,"changes":[{"name":"CAP25-WHK"}]}
[curl http=200 total=0.137554s]
~~~

#### git status --porcelain of the WHK-Global checkout (after)

`git -C ~/data/projects/WHK-Global status --porcelain` @ 2026-10-22T14:03:55Z

~~~
?? .ignition-preview/
?? db_backup_sqlite.idb
?? e2e/lib/preflight-mes-target.mjs
?? pyrightconfig.json
?? styling/
?? test-results/
[exit 0]
~~~

Script reading: the WHK-Global `git status --porcelain` output is identical before and after, so the source checkout is untouched.

#### GET projects/list (limit=-1), end of block

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/list?limit=-1'` @ 2026-10-22T14:03:56Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:03:56 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 70

{"items":[],"metadata":{"total":0,"matching":0,"limit":-1,"offset":0}}
[curl http=200 total=0.010616s]
~~~

Script reading: 0 CAP25-* project(s) listed after this block; the rig list holds 0 project(s) in all.

### C8c WHK-Global — what still differs in resource.json after normalisation

#### field names that differ, export after overwrite=false

`bash capture25.sh rjdiff $STATE/c8/whk.zip $STATE/c8/CAP25-WHK.1.zip` @ 2026-10-22T14:05:01Z

~~~
resource.json members in both: 900, equal after normalisation: 852, differing: 48
  differing field files (same set, order differs): 48 member(s)
[exit 0]
~~~

#### field names that differ, export after overwrite=true

`bash capture25.sh rjdiff $STATE/c8/whk.zip $STATE/c8/CAP25-WHK.2.zip` @ 2026-10-22T14:05:01Z

~~~
resource.json members in both: 900, equal after normalisation: 852, differing: 48
  differing field files (same set, order differs): 48 member(s)
[exit 0]
~~~

Script reading (WHK-Global): the gateway accepted the 2,214-file allowlist zip (3,388 entries) with `success: true` and a single `changes` entry for both `overwrite` values, and the export holds 2,213 files. The one zip-only member is `com.inductiveautomation.perspective/views/Changeover/ChangeoverPanel/README.md`, a file in a folder whose `resource.json` does not list it: it was dropped silently (C8d below tests this on the synthetic project). Of 245 members whose bytes differ, 196 differ only in `resource.json` metadata (`attributes.lastModification` and `lastModificationSignature`); after normalisation 49 differ: `project.json` (key `parent` added) and 48 `resource.json` whose `files` array holds the same names in a different order (the gateway re-sorts it). Both exports are identical in these counts, and every directory entry (1,174 in the zip) is absent from the export. So the canonical hash for 25-03 K4 must normalise `files` order (sort it) as well as the two attributes, and must either drop unlisted files from the digest or exclude them from the artifact.

## C8d

C8d: what the gateway does with a file present but not listed in its folder's `resource.json`, a dotfile, an empty directory, a folder with no `resource.json`, a `*.py` sidecar, a `__pycache__` directory, and a listed file the zip lacks. Each variant starts from the synthetic checkout (checkout layout), is imported with overwrite=false as CAP25-C8D-<variant>, and is re-exported. The summary table at the end has one row per variant.

### variant unlisted

#### variant unlisted: members of the uploaded zip

`bash capture25.sh members $STATE/c8/c8d-unlisted.zip` @ 2026-10-22T14:05:19Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/	0	-
com.inductiveautomation.perspective/views/	0	-
com.inductiveautomation.perspective/views/Cap25/	0	-
com.inductiveautomation.perspective/views/Cap25/resource.json	102	87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/	0	-
ignition/script-python/	0	-
ignition/script-python/cap25/	0	-
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/notes.txt	6	444e0fffbd825e9610ff5b199485707a0c895339ae80c15cc8a8aee41b106fda
ignition/script-python/cap25/resource.json	100	cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938
project.json	59	5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354
[exit 0]
~~~

#### POST import CAP25-C8D-unlisted overwrite=false

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/c8d-unlisted.zip '$BASE/data/api/v1/projects/import/CAP25-C8D-unlisted?overwrite=false'` @ 2026-10-22T14:05:19Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:19 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 58

{"success":true,"changes":[{"name":"CAP25-C8D-unlisted"}]}
[curl http=200 total=0.127486s]
~~~

#### GET projects/export/CAP25-C8D-unlisted

`curl -sS -g -D - -o '$STATE/c8/CAP25-C8D-unlisted.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C8D-unlisted'` @ 2026-10-22T14:05:19Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:19 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C8D-unlisted_20261022140519.zip"
Content-Length: 1283

size_bytes=1283
sha256=cc6958e9e72d0edf9401b4d6199149ae0cd29c91bc26c6a1f9cbe4fdd333468b
[curl http=200 total=0.022761s]
~~~

#### variant unlisted: table, uploaded zip against export

`bash capture25.sh cmp $STATE/c8/c8d-unlisted.zip $STATE/c8/CAP25-C8D-unlisted.zip` @ 2026-10-22T14:05:19Z

~~~
member (user-path key) | in zip | in export | sha256 zip | sha256 export | equal | equal after normalisation
com.inductiveautomation.perspective/views/Cap25/resource.json | yes | yes | 87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1 | 34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e | no | yes
com.inductiveautomation.perspective/views/Cap25/view.json | yes | yes | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | yes | yes
ignition/script-python/cap25/code.py | yes | yes | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | yes | yes
ignition/script-python/cap25/notes.txt | yes | no | 444e0fffbd825e9610ff5b199485707a0c895339ae80c15cc8a8aee41b106fda | - | no | no
ignition/script-python/cap25/resource.json | yes | yes | cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938 | fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05 | no | yes
project.json | yes | yes | 5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354 | 9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14 | no | no
directory entries: zip 6, export 0
project.json differing key names: description, parent
[exit 0]
~~~

#### variant unlisted: resource.json files array of cap25 in the export

`bash capture25.sh files $STATE/c8/CAP25-C8D-unlisted.zip ignition/script-python/cap25/resource.json` @ 2026-10-22T14:05:19Z

~~~
["code.py"]
[exit 0]
~~~

#### DELETE project CAP25-C8D-unlisted

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-C8D-unlisted?confirm=true'` @ 2026-10-22T14:05:19Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:19 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.019740s]
~~~

### variant dotfile

#### variant dotfile: members of the uploaded zip

`bash capture25.sh members $STATE/c8/c8d-dotfile.zip` @ 2026-10-22T14:05:20Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/	0	-
com.inductiveautomation.perspective/views/	0	-
com.inductiveautomation.perspective/views/Cap25/	0	-
com.inductiveautomation.perspective/views/Cap25/resource.json	102	87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/	0	-
ignition/script-python/	0	-
ignition/script-python/cap25/	0	-
ignition/script-python/cap25/.gitkeep	0	e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/resource.json	100	cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938
project.json	59	5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354
[exit 0]
~~~

#### POST import CAP25-C8D-dotfile overwrite=false

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/c8d-dotfile.zip '$BASE/data/api/v1/projects/import/CAP25-C8D-dotfile?overwrite=false'` @ 2026-10-22T14:05:20Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:20 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 57

{"success":true,"changes":[{"name":"CAP25-C8D-dotfile"}]}
[curl http=200 total=0.028614s]
~~~

#### GET projects/export/CAP25-C8D-dotfile

`curl -sS -g -D - -o '$STATE/c8/CAP25-C8D-dotfile.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C8D-dotfile'` @ 2026-10-22T14:05:20Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:20 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C8D-dotfile_20261022140520.zip"
Content-Length: 1283

size_bytes=1283
sha256=394f00b1883cde9d8ba9789885b9ed0650d6ee18a18db47f5c122139ad0c1ef0
[curl http=200 total=0.009869s]
~~~

#### variant dotfile: table, uploaded zip against export

`bash capture25.sh cmp $STATE/c8/c8d-dotfile.zip $STATE/c8/CAP25-C8D-dotfile.zip` @ 2026-10-22T14:05:20Z

~~~
member (user-path key) | in zip | in export | sha256 zip | sha256 export | equal | equal after normalisation
com.inductiveautomation.perspective/views/Cap25/resource.json | yes | yes | 87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1 | 34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e | no | yes
com.inductiveautomation.perspective/views/Cap25/view.json | yes | yes | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | yes | yes
ignition/script-python/cap25/.gitkeep | yes | no | e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 | - | no | no
ignition/script-python/cap25/code.py | yes | yes | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | yes | yes
ignition/script-python/cap25/resource.json | yes | yes | cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938 | fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05 | no | yes
project.json | yes | yes | 5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354 | 9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14 | no | no
directory entries: zip 6, export 0
project.json differing key names: description, parent
[exit 0]
~~~

#### variant dotfile: resource.json files array of cap25 in the export

`bash capture25.sh files $STATE/c8/CAP25-C8D-dotfile.zip ignition/script-python/cap25/resource.json` @ 2026-10-22T14:05:20Z

~~~
["code.py"]
[exit 0]
~~~

#### DELETE project CAP25-C8D-dotfile

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-C8D-dotfile?confirm=true'` @ 2026-10-22T14:05:20Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:20 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.016080s]
~~~

### variant dotlisted

#### variant dotlisted: members of the uploaded zip

`bash capture25.sh members $STATE/c8/c8d-dotlisted.zip` @ 2026-10-22T14:05:20Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/	0	-
com.inductiveautomation.perspective/views/	0	-
com.inductiveautomation.perspective/views/Cap25/	0	-
com.inductiveautomation.perspective/views/Cap25/resource.json	102	87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/	0	-
ignition/script-python/	0	-
ignition/script-python/cap25/	0	-
ignition/script-python/cap25/.gitkeep	0	e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/resource.json	111	5a2092fcc35d5362d0dbd3048936d0dd13de9b3ff731880e9620ee1c14eb6886
project.json	59	5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354
[exit 0]
~~~

#### POST import CAP25-C8D-dotlisted overwrite=false

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/c8d-dotlisted.zip '$BASE/data/api/v1/projects/import/CAP25-C8D-dotlisted?overwrite=false'` @ 2026-10-22T14:05:20Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:20 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 59

{"success":true,"changes":[{"name":"CAP25-C8D-dotlisted"}]}
[curl http=200 total=0.056097s]
~~~

#### GET projects/export/CAP25-C8D-dotlisted

`curl -sS -g -D - -o '$STATE/c8/CAP25-C8D-dotlisted.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C8D-dotlisted'` @ 2026-10-22T14:05:20Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:20 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C8D-dotlisted_20261022140520.zip"
Content-Length: 1461

size_bytes=1461
sha256=acc2bf657e5549a8a384ae5ffc154dfcaef4c7169f8c8e7ee7985da2475ac510
[curl http=200 total=0.106323s]
~~~

#### variant dotlisted: table, uploaded zip against export

`bash capture25.sh cmp $STATE/c8/c8d-dotlisted.zip $STATE/c8/CAP25-C8D-dotlisted.zip` @ 2026-10-22T14:05:21Z

~~~
member (user-path key) | in zip | in export | sha256 zip | sha256 export | equal | equal after normalisation
com.inductiveautomation.perspective/views/Cap25/resource.json | yes | yes | 87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1 | 34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e | no | yes
com.inductiveautomation.perspective/views/Cap25/view.json | yes | yes | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | yes | yes
ignition/script-python/cap25/.gitkeep | yes | yes | e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 | e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 | yes | yes
ignition/script-python/cap25/code.py | yes | yes | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | yes | yes
ignition/script-python/cap25/resource.json | yes | yes | 5a2092fcc35d5362d0dbd3048936d0dd13de9b3ff731880e9620ee1c14eb6886 | 9eaf0985c7c261de1cbdba3a45dc0af0d7897c28c7b8f4d4156d71adbe8a1c2d | no | no
project.json | yes | yes | 5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354 | 9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14 | no | no
directory entries: zip 6, export 0
project.json differing key names: description, parent
[exit 0]
~~~

#### variant dotlisted: resource.json files array of cap25 in the export

`bash capture25.sh files $STATE/c8/CAP25-C8D-dotlisted.zip ignition/script-python/cap25/resource.json` @ 2026-10-22T14:05:21Z

~~~
[".gitkeep", "code.py"]
[exit 0]
~~~

#### DELETE project CAP25-C8D-dotlisted

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-C8D-dotlisted?confirm=true'` @ 2026-10-22T14:05:21Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:21 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.057270s]
~~~

### variant emptydir

#### variant emptydir: members of the uploaded zip

`bash capture25.sh members $STATE/c8/c8d-emptydir.zip` @ 2026-10-22T14:05:21Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/	0	-
com.inductiveautomation.perspective/views/	0	-
com.inductiveautomation.perspective/views/Cap25/	0	-
com.inductiveautomation.perspective/views/Cap25/resource.json	102	87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/	0	-
ignition/script-python/	0	-
ignition/script-python/cap25/	0	-
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/resource.json	100	cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938
ignition/script-python/empty/	0	-
project.json	59	5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354
[exit 0]
~~~

#### POST import CAP25-C8D-emptydir overwrite=false

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/c8d-emptydir.zip '$BASE/data/api/v1/projects/import/CAP25-C8D-emptydir?overwrite=false'` @ 2026-10-22T14:05:21Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:21 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 58

{"success":true,"changes":[{"name":"CAP25-C8D-emptydir"}]}
[curl http=200 total=0.055853s]
~~~

#### GET projects/export/CAP25-C8D-emptydir

`curl -sS -g -D - -o '$STATE/c8/CAP25-C8D-emptydir.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C8D-emptydir'` @ 2026-10-22T14:05:21Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:21 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C8D-emptydir_20261022140521.zip"
Content-Length: 1283

size_bytes=1283
sha256=394f00b1883cde9d8ba9789885b9ed0650d6ee18a18db47f5c122139ad0c1ef0
[curl http=200 total=0.014885s]
~~~

#### variant emptydir: table, uploaded zip against export

`bash capture25.sh cmp $STATE/c8/c8d-emptydir.zip $STATE/c8/CAP25-C8D-emptydir.zip` @ 2026-10-22T14:05:21Z

~~~
member (user-path key) | in zip | in export | sha256 zip | sha256 export | equal | equal after normalisation
com.inductiveautomation.perspective/views/Cap25/resource.json | yes | yes | 87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1 | 34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e | no | yes
com.inductiveautomation.perspective/views/Cap25/view.json | yes | yes | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | yes | yes
ignition/script-python/cap25/code.py | yes | yes | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | yes | yes
ignition/script-python/cap25/resource.json | yes | yes | cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938 | fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05 | no | yes
project.json | yes | yes | 5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354 | 9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14 | no | no
directory entries: zip 7, export 0
project.json differing key names: description, parent
[exit 0]
~~~

#### variant emptydir: resource.json files array of cap25 in the export

`bash capture25.sh files $STATE/c8/CAP25-C8D-emptydir.zip ignition/script-python/cap25/resource.json` @ 2026-10-22T14:05:21Z

~~~
["code.py"]
[exit 0]
~~~

#### DELETE project CAP25-C8D-emptydir

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-C8D-emptydir?confirm=true'` @ 2026-10-22T14:05:22Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:22 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.023282s]
~~~

### variant nodescriptor

#### variant nodescriptor: members of the uploaded zip

`bash capture25.sh members $STATE/c8/c8d-nodescriptor.zip` @ 2026-10-22T14:05:22Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/	0	-
com.inductiveautomation.perspective/views/	0	-
com.inductiveautomation.perspective/views/Cap25/	0	-
com.inductiveautomation.perspective/views/Cap25/resource.json	102	87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/	0	-
ignition/script-python/	0	-
ignition/script-python/cap25/	0	-
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/resource.json	100	cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938
ignition/script-python/loose/	0	-
ignition/script-python/loose/code.py	22	86367fd8af0c0fa376b40b37869d7415d8dd52dbd9c8d7d70caf777fb8cbaf51
project.json	59	5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354
[exit 0]
~~~

#### POST import CAP25-C8D-nodescriptor overwrite=false

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/c8d-nodescriptor.zip '$BASE/data/api/v1/projects/import/CAP25-C8D-nodescriptor?overwrite=false'` @ 2026-10-22T14:05:22Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:22 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 62

{"success":true,"changes":[{"name":"CAP25-C8D-nodescriptor"}]}
[curl http=200 total=0.038609s]
~~~

#### GET projects/export/CAP25-C8D-nodescriptor

`curl -sS -g -D - -o '$STATE/c8/CAP25-C8D-nodescriptor.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C8D-nodescriptor'` @ 2026-10-22T14:05:22Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:22 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C8D-nodescriptor_20261022140522.zip"
Content-Length: 1283

size_bytes=1283
sha256=04a7dbad996edd2dae5e6521136389be4ece40038b6ac53e08b1cb4bbf8633cd
[curl http=200 total=0.012713s]
~~~

#### variant nodescriptor: table, uploaded zip against export

`bash capture25.sh cmp $STATE/c8/c8d-nodescriptor.zip $STATE/c8/CAP25-C8D-nodescriptor.zip` @ 2026-10-22T14:05:22Z

~~~
member (user-path key) | in zip | in export | sha256 zip | sha256 export | equal | equal after normalisation
com.inductiveautomation.perspective/views/Cap25/resource.json | yes | yes | 87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1 | 34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e | no | yes
com.inductiveautomation.perspective/views/Cap25/view.json | yes | yes | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | yes | yes
ignition/script-python/cap25/code.py | yes | yes | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | yes | yes
ignition/script-python/cap25/resource.json | yes | yes | cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938 | fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05 | no | yes
ignition/script-python/loose/code.py | yes | no | 86367fd8af0c0fa376b40b37869d7415d8dd52dbd9c8d7d70caf777fb8cbaf51 | - | no | no
project.json | yes | yes | 5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354 | 9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14 | no | no
directory entries: zip 7, export 0
project.json differing key names: description, parent
[exit 0]
~~~

#### variant nodescriptor: resource.json files array of cap25 in the export

`bash capture25.sh files $STATE/c8/CAP25-C8D-nodescriptor.zip ignition/script-python/cap25/resource.json` @ 2026-10-22T14:05:22Z

~~~
["code.py"]
[exit 0]
~~~

#### DELETE project CAP25-C8D-nodescriptor

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-C8D-nodescriptor?confirm=true'` @ 2026-10-22T14:05:22Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:22 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.011256s]
~~~

### variant pysidecar

#### variant pysidecar: members of the uploaded zip

`bash capture25.sh members $STATE/c8/c8d-pysidecar.zip` @ 2026-10-22T14:05:22Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/	0	-
com.inductiveautomation.perspective/views/	0	-
com.inductiveautomation.perspective/views/Cap25/	0	-
com.inductiveautomation.perspective/views/Cap25/resource.json	102	87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/	0	-
ignition/script-python/	0	-
ignition/script-python/cap25/	0	-
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/helper.py	22	2f5c70e4813e8472a90aec96c2db932472d2da10c1c5efd2207012092d262355
ignition/script-python/cap25/resource.json	100	cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938
project.json	59	5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354
[exit 0]
~~~

#### POST import CAP25-C8D-pysidecar overwrite=false

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/c8d-pysidecar.zip '$BASE/data/api/v1/projects/import/CAP25-C8D-pysidecar?overwrite=false'` @ 2026-10-22T14:05:22Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:22 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 59

{"success":true,"changes":[{"name":"CAP25-C8D-pysidecar"}]}
[curl http=200 total=0.050156s]
~~~

#### GET projects/export/CAP25-C8D-pysidecar

`curl -sS -g -D - -o '$STATE/c8/CAP25-C8D-pysidecar.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C8D-pysidecar'` @ 2026-10-22T14:05:22Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:22 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C8D-pysidecar_20261022140522.zip"
Content-Length: 1283

size_bytes=1283
sha256=04a7dbad996edd2dae5e6521136389be4ece40038b6ac53e08b1cb4bbf8633cd
[curl http=200 total=0.009686s]
~~~

#### variant pysidecar: table, uploaded zip against export

`bash capture25.sh cmp $STATE/c8/c8d-pysidecar.zip $STATE/c8/CAP25-C8D-pysidecar.zip` @ 2026-10-22T14:05:23Z

~~~
member (user-path key) | in zip | in export | sha256 zip | sha256 export | equal | equal after normalisation
com.inductiveautomation.perspective/views/Cap25/resource.json | yes | yes | 87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1 | 34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e | no | yes
com.inductiveautomation.perspective/views/Cap25/view.json | yes | yes | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | yes | yes
ignition/script-python/cap25/code.py | yes | yes | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | yes | yes
ignition/script-python/cap25/helper.py | yes | no | 2f5c70e4813e8472a90aec96c2db932472d2da10c1c5efd2207012092d262355 | - | no | no
ignition/script-python/cap25/resource.json | yes | yes | cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938 | fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05 | no | yes
project.json | yes | yes | 5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354 | 9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14 | no | no
directory entries: zip 6, export 0
project.json differing key names: description, parent
[exit 0]
~~~

#### variant pysidecar: resource.json files array of cap25 in the export

`bash capture25.sh files $STATE/c8/CAP25-C8D-pysidecar.zip ignition/script-python/cap25/resource.json` @ 2026-10-22T14:05:23Z

~~~
["code.py"]
[exit 0]
~~~

#### DELETE project CAP25-C8D-pysidecar

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-C8D-pysidecar?confirm=true'` @ 2026-10-22T14:05:23Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:23 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.011200s]
~~~

### variant pycache

#### variant pycache: members of the uploaded zip

`bash capture25.sh members $STATE/c8/c8d-pycache.zip` @ 2026-10-22T14:05:23Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/	0	-
com.inductiveautomation.perspective/views/	0	-
com.inductiveautomation.perspective/views/Cap25/	0	-
com.inductiveautomation.perspective/views/Cap25/resource.json	102	87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/	0	-
ignition/script-python/	0	-
ignition/script-python/cap25/	0	-
ignition/script-python/cap25/__pycache__/	0	-
ignition/script-python/cap25/__pycache__/code.cpython-311.pyc	8	4d88a88efe0309051d5600523a1d29e1d0888c51fb5bdf98c842162eadf7a332
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/resource.json	100	cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938
project.json	59	5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354
[exit 0]
~~~

#### POST import CAP25-C8D-pycache overwrite=false

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/c8d-pycache.zip '$BASE/data/api/v1/projects/import/CAP25-C8D-pycache?overwrite=false'` @ 2026-10-22T14:05:23Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:23 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 57

{"success":true,"changes":[{"name":"CAP25-C8D-pycache"}]}
[curl http=200 total=0.040823s]
~~~

#### GET projects/export/CAP25-C8D-pycache

`curl -sS -g -D - -o '$STATE/c8/CAP25-C8D-pycache.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C8D-pycache'` @ 2026-10-22T14:05:23Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:23 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C8D-pycache_20261022140523.zip"
Content-Length: 822

size_bytes=822
sha256=da4fd5bba3f586eac4ce1511535dd23dbb730f0d5e428a04ceabff33ce521a33
[curl http=200 total=0.006671s]
~~~

#### variant pycache: table, uploaded zip against export

`bash capture25.sh cmp $STATE/c8/c8d-pycache.zip $STATE/c8/CAP25-C8D-pycache.zip` @ 2026-10-22T14:05:23Z

~~~
member (user-path key) | in zip | in export | sha256 zip | sha256 export | equal | equal after normalisation
com.inductiveautomation.perspective/views/Cap25/resource.json | yes | yes | 87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1 | 34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e | no | yes
com.inductiveautomation.perspective/views/Cap25/view.json | yes | yes | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | yes | yes
ignition/script-python/cap25/__pycache__/code.cpython-311.pyc | yes | no | 4d88a88efe0309051d5600523a1d29e1d0888c51fb5bdf98c842162eadf7a332 | - | no | no
ignition/script-python/cap25/code.py | yes | no | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | - | no | no
ignition/script-python/cap25/resource.json | yes | no | cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938 | - | no | no
project.json | yes | yes | 5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354 | 9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14 | no | no
directory entries: zip 7, export 0
project.json differing key names: description, parent
[exit 0]
~~~

#### variant pycache: resource.json files array of cap25 in the export

`bash capture25.sh files $STATE/c8/CAP25-C8D-pycache.zip ignition/script-python/cap25/resource.json` @ 2026-10-22T14:05:23Z

~~~
<absent>
[exit 0]
~~~

#### DELETE project CAP25-C8D-pycache

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-C8D-pycache?confirm=true'` @ 2026-10-22T14:05:23Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:23 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.010726s]
~~~

### variant listedmissing

#### variant listedmissing: members of the uploaded zip

`bash capture25.sh members $STATE/c8/c8d-listedmissing.zip` @ 2026-10-22T14:05:23Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/	0	-
com.inductiveautomation.perspective/views/	0	-
com.inductiveautomation.perspective/views/Cap25/	0	-
com.inductiveautomation.perspective/views/Cap25/resource.json	102	87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/	0	-
ignition/script-python/	0	-
ignition/script-python/cap25/	0	-
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/resource.json	111	f2ead837b9d98dcb95e1be225cc21e5167957d7962ee80dda54fbaea93121faf
project.json	59	5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354
[exit 0]
~~~

#### POST import CAP25-C8D-listedmissing overwrite=false

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/c8d-listedmissing.zip '$BASE/data/api/v1/projects/import/CAP25-C8D-listedmissing?overwrite=false'` @ 2026-10-22T14:05:24Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:24 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 63

{"success":true,"changes":[{"name":"CAP25-C8D-listedmissing"}]}
[curl http=200 total=0.038939s]
~~~

#### GET projects/export/CAP25-C8D-listedmissing

`curl -sS -g -D - -o '$STATE/c8/CAP25-C8D-listedmissing.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C8D-listedmissing'` @ 2026-10-22T14:05:24Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:24 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C8D-listedmissing_20261022140524.zip"
Content-Length: 1283

size_bytes=1283
sha256=fa904cb3e52ddb0cd1064ca272cedb06d9ab80cd3586fea3913f3a69101de61e
[curl http=200 total=0.008166s]
~~~

#### variant listedmissing: table, uploaded zip against export

`bash capture25.sh cmp $STATE/c8/c8d-listedmissing.zip $STATE/c8/CAP25-C8D-listedmissing.zip` @ 2026-10-22T14:05:24Z

~~~
member (user-path key) | in zip | in export | sha256 zip | sha256 export | equal | equal after normalisation
com.inductiveautomation.perspective/views/Cap25/resource.json | yes | yes | 87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1 | 34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e | no | yes
com.inductiveautomation.perspective/views/Cap25/view.json | yes | yes | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | yes | yes
ignition/script-python/cap25/code.py | yes | yes | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | yes | yes
ignition/script-python/cap25/resource.json | yes | yes | f2ead837b9d98dcb95e1be225cc21e5167957d7962ee80dda54fbaea93121faf | fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05 | no | no
project.json | yes | yes | 5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354 | 9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14 | no | no
directory entries: zip 6, export 0
project.json differing key names: description, parent
[exit 0]
~~~

#### variant listedmissing: resource.json files array of cap25 in the export

`bash capture25.sh files $STATE/c8/CAP25-C8D-listedmissing.zip ignition/script-python/cap25/resource.json` @ 2026-10-22T14:05:24Z

~~~
["code.py"]
[exit 0]
~~~

#### DELETE project CAP25-C8D-listedmissing

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-C8D-listedmissing?confirm=true'` @ 2026-10-22T14:05:24Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:24 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.018148s]
~~~

### C8d summary table

| variant | import status | success | changes | body keys | entries named like the probed member in zip | entries in export | cap25/resource.json files after export |
| --- | --- | --- | --- | --- | --- | --- | --- |
| unlisted | 200 | success=true | changes=1 | body keys: changes,success | in zip: 1 | in export: 0 | cap25/resource.json files after export: ["code.py"] |
| dotfile | 200 | success=true | changes=1 | body keys: changes,success | in zip: 1 | in export: 0 | cap25/resource.json files after export: ["code.py"] |
| dotlisted | 200 | success=true | changes=1 | body keys: changes,success | in zip: 1 | in export: 1 | cap25/resource.json files after export: [".gitkeep", "code.py"] |
| emptydir | 200 | success=true | changes=1 | body keys: changes,success | in zip: 1 | in export: 0 | cap25/resource.json files after export: ["code.py"] |
| nodescriptor | 200 | success=true | changes=1 | body keys: changes,success | in zip: 1 | in export: 0 | cap25/resource.json files after export: ["code.py"] |
| pysidecar | 200 | success=true | changes=1 | body keys: changes,success | in zip: 1 | in export: 0 | cap25/resource.json files after export: ["code.py"] |
| pycache | 200 | success=true | changes=1 | body keys: changes,success | in zip: 2 | in export: 0 | cap25/resource.json files after export: <absent> |
| listedmissing | 200 | success=true | changes=1 | body keys: changes,success | in zip: 0 | in export: 0 | cap25/resource.json files after export: ["code.py"] |

#### GET projects/list (limit=-1), end of block

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/list?limit=-1'` @ 2026-10-22T14:05:24Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:05:24 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 70

{"items":[],"metadata":{"total":0,"matching":0,"limit":-1,"offset":0}}
[curl http=200 total=0.012533s]
~~~

Script reading: 0 CAP25-* project(s) listed after this block; the rig list holds 0 project(s) in all.

### C8d extra variants

### variant nesteddir

#### variant nesteddir: members of the uploaded zip

`bash capture25.sh members $STATE/c8/c8d-nesteddir.zip` @ 2026-10-22T14:06:11Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/	0	-
com.inductiveautomation.perspective/views/	0	-
com.inductiveautomation.perspective/views/Cap25/	0	-
com.inductiveautomation.perspective/views/Cap25/resource.json	102	87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/	0	-
ignition/script-python/	0	-
ignition/script-python/cap25/	0	-
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/resource.json	100	cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938
ignition/script-python/cap25/sub/	0	-
ignition/script-python/cap25/sub/x.txt	2	73cb3858a687a8494ca3323053016282f3dad39d42cf62ca4e79dda2aac7d9ac
project.json	59	5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354
[exit 0]
~~~

#### POST import CAP25-C8D-nesteddir overwrite=false

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/c8d-nesteddir.zip '$BASE/data/api/v1/projects/import/CAP25-C8D-nesteddir?overwrite=false'` @ 2026-10-22T14:06:11Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:06:11 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 59

{"success":true,"changes":[{"name":"CAP25-C8D-nesteddir"}]}
[curl http=200 total=0.096158s]
~~~

#### GET projects/export/CAP25-C8D-nesteddir

`curl -sS -g -D - -o '$STATE/c8/CAP25-C8D-nesteddir.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C8D-nesteddir'` @ 2026-10-22T14:06:11Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:06:11 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C8D-nesteddir_20261022140611.zip"
Content-Length: 822

size_bytes=822
sha256=767773cfea87668dc825ca5bf1cb8478a910cb5c1a09766734f7a10f0599ff1c
[curl http=200 total=0.016695s]
~~~

#### variant nesteddir: table, uploaded zip against export

`bash capture25.sh cmp $STATE/c8/c8d-nesteddir.zip $STATE/c8/CAP25-C8D-nesteddir.zip` @ 2026-10-22T14:06:11Z

~~~
member (user-path key) | in zip | in export | sha256 zip | sha256 export | equal | equal after normalisation
com.inductiveautomation.perspective/views/Cap25/resource.json | yes | yes | 87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1 | 34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e | no | yes
com.inductiveautomation.perspective/views/Cap25/view.json | yes | yes | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | yes | yes
ignition/script-python/cap25/code.py | yes | no | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | - | no | no
ignition/script-python/cap25/resource.json | yes | no | cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938 | - | no | no
ignition/script-python/cap25/sub/x.txt | yes | no | 73cb3858a687a8494ca3323053016282f3dad39d42cf62ca4e79dda2aac7d9ac | - | no | no
project.json | yes | yes | 5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354 | 9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14 | no | no
directory entries: zip 7, export 0
project.json differing key names: description, parent
[exit 0]
~~~

#### variant nesteddir: resource.json files array of cap25 in the export

`bash capture25.sh files $STATE/c8/CAP25-C8D-nesteddir.zip ignition/script-python/cap25/resource.json` @ 2026-10-22T14:06:11Z

~~~
<absent>
[exit 0]
~~~

#### DELETE project CAP25-C8D-nesteddir

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-C8D-nesteddir?confirm=true'` @ 2026-10-22T14:06:11Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:06:12 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.029114s]
~~~

### C8d summary table (extra variants)

| variant | import status | success | changes | body keys | entries named like the probed member in zip | entries in export | cap25/resource.json files after export |
| --- | --- | --- | --- | --- | --- | --- | --- |
| nesteddir | 200 | success=true | changes=1 | body keys: changes,success | in zip: 1 | in export: 0 | cap25/resource.json files after export: <absent> |

#### GET projects/list (limit=-1), end of block

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/list?limit=-1'` @ 2026-10-22T14:06:12Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:06:12 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 70

{"items":[],"metadata":{"total":0,"matching":0,"limit":-1,"offset":0}}
[curl http=200 total=0.008172s]
~~~

Script reading: 0 CAP25-* project(s) listed after this block; the rig list holds 0 project(s) in all.

Script reading (C8d), from the two summary tables above:

1. **The importer never reports what it drops.** All nine variants answered HTTP 200, `success: true`, one `changes` entry (the project name), and no other body key.
2. **A file not listed in its folder's `resource.json` is dropped silently, and the resource survives.** `unlisted` (`notes.txt`), `dotfile` (`.gitkeep`) and `pysidecar` (`helper.py`) are absent from the export, and the exported `resource.json` still lists only `code.py`. The same file is kept when it is listed: `dotlisted` exports `.gitkeep` and `code.py` (the `files` array comes back sorted). The WHK-Global zip agrees: its one unlisted README was dropped (A2 holds for dotfiles and `*.py` sidecars too).
3. **A listed file the zip lacks is not an error.** `listedmissing` imports with `success: true`; the export's `resource.json` lists only `code.py`, so the gateway rewrites `files` to what exists (the dangling name `extra.py` is gone).
4. **A folder with no `resource.json` is ignored.** `nodescriptor` (`loose/code.py`) is absent from the export. An empty directory leaves no trace (`emptydir`), which is consistent with an export that carries no directory entries at all.
5. **A subdirectory inside a resource folder drops the whole resource folder.** `pycache` (`cap25/__pycache__/code.cpython-311.pyc`) and `nesteddir` (`cap25/sub/x.txt`) both import with `success: true`, and then `code.py`, `resource.json` and the nested file are all absent from the export while the rest of the project survives (the Perspective view and `project.json` are still there). One stray directory therefore deletes a script resource with no signal from the importer. Only these two directory shapes were tested; whether other resource types behave the same is not measured here.
6. **Input to 25-03 K2 and K3.** The artifact builder has to keep the dropped shapes out of the zip (or refuse them) because the gateway will not say it dropped them, and the canonical digest has to be computed over what the gateway keeps: the `resource.json` plus the files it lists that exist, with `files` sorted. A `__pycache__` or any other directory under a resource folder must be a build refusal, not just an exclusion, because it silently removes a resource.

## C8a

C8a: does `import?overwrite=true` delete resources the zip omits? One project (`CAP25-C8A`) holds script folders A (`cap25`) and B (`cap25b`) and one Perspective view. Before each case the project is restored from the full zip with overwrite=true and exported; the case zip is then imported with overwrite=true and exported again. The reading line under each case counts the export entries for the omitted members before and after. Cases: (i) without folder B, (ii) without the view folder, (iii) folder A `resource.json` still lists `code.py` but the `code.py` member is absent, (iv) a zip holding only `project.json`.

### baseline

#### POST import CAP25-C8A overwrite=false (full zip)

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/a-full.zip '$BASE/data/api/v1/projects/import/CAP25-C8A?overwrite=false'` @ 2026-10-22T14:07:55Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:07:55 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 49

{"success":true,"changes":[{"name":"CAP25-C8A"}]}
[curl http=200 total=0.077196s]
~~~

### case noB

#### POST import CAP25-C8A overwrite=true (restore the full zip)

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/a-full.zip '$BASE/data/api/v1/projects/import/CAP25-C8A?overwrite=true'` @ 2026-10-22T14:07:55Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:07:55 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 49

{"success":true,"changes":[{"name":"CAP25-C8A"}]}
[curl http=200 total=0.015861s]
~~~

#### GET projects/export/CAP25-C8A (before noB)

`curl -sS -g -D - -o '$STATE/c8/c8a-noB.pre.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C8A'` @ 2026-10-22T14:07:56Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:07:56 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C8A_20261022140756.zip"
Content-Length: 1748

size_bytes=1748
sha256=0a3e2fd2ba76322fc417f13249c3ae20a8c4601ca8f4992bc54f78c5626b09f1
[curl http=200 total=0.013253s]
~~~

#### members of the export before case noB

`bash capture25.sh members $STATE/c8/c8a-noB.pre.zip` @ 2026-10-22T14:07:56Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/views/Cap25/resource.json	134	34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/resource.json	132	fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05
ignition/script-python/cap25b/code.py	22	b10c690bf702ea0a33f59131ff7c2ee2f3061aaf37ae6bbc5f727d4877647214
ignition/script-python/cap25b/resource.json	132	fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05
project.json	108	9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14
[exit 0]
~~~

#### members of the case zip noB

`bash capture25.sh members $STATE/c8/a-noB.zip` @ 2026-10-22T14:07:56Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/	0	-
com.inductiveautomation.perspective/views/	0	-
com.inductiveautomation.perspective/views/Cap25/	0	-
com.inductiveautomation.perspective/views/Cap25/resource.json	102	87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/	0	-
ignition/script-python/	0	-
ignition/script-python/cap25/	0	-
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/resource.json	100	cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938
project.json	59	5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354
[exit 0]
~~~

#### POST import CAP25-C8A overwrite=true (case noB)

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/a-noB.zip '$BASE/data/api/v1/projects/import/CAP25-C8A?overwrite=true'` @ 2026-10-22T14:07:56Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:07:56 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 49

{"success":true,"changes":[{"name":"CAP25-C8A"}]}
[curl http=200 total=0.015523s]
~~~

#### GET projects/export/CAP25-C8A (after noB)

`curl -sS -g -D - -o '$STATE/c8/c8a-noB.post.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C8A'` @ 2026-10-22T14:07:56Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:07:56 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C8A_20261022140756.zip"
Content-Length: 1283

size_bytes=1283
sha256=b3d7b471ca89ee49c29ea205c4423cdef822306aa7a342527571cb9353ccc836
[curl http=200 total=0.009386s]
~~~

#### members of the export after case noB

`bash capture25.sh members $STATE/c8/c8a-noB.post.zip` @ 2026-10-22T14:07:56Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/views/Cap25/resource.json	134	34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/resource.json	132	fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05
project.json	108	9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14
[exit 0]
~~~

Script reading (noB): before: folder B (script-python/cap25b/) entries 2, view folder entries 2, folder A code.py entries 1, folder A resource.json entries 1, project.json entries 1, entries in all 7. After: folder B (script-python/cap25b/) entries 0, view folder entries 2, folder A code.py entries 1, folder A resource.json entries 1, project.json entries 1, entries in all 5.

### case noview

#### POST import CAP25-C8A overwrite=true (restore the full zip)

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/a-full.zip '$BASE/data/api/v1/projects/import/CAP25-C8A?overwrite=true'` @ 2026-10-22T14:07:56Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:07:56 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 49

{"success":true,"changes":[{"name":"CAP25-C8A"}]}
[curl http=200 total=0.034465s]
~~~

#### GET projects/export/CAP25-C8A (before noview)

`curl -sS -g -D - -o '$STATE/c8/c8a-noview.pre.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C8A'` @ 2026-10-22T14:07:56Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:07:56 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C8A_20261022140756.zip"
Content-Length: 1748

size_bytes=1748
sha256=0a3e2fd2ba76322fc417f13249c3ae20a8c4601ca8f4992bc54f78c5626b09f1
[curl http=200 total=0.009596s]
~~~

#### members of the export before case noview

`bash capture25.sh members $STATE/c8/c8a-noview.pre.zip` @ 2026-10-22T14:07:56Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/views/Cap25/resource.json	134	34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/resource.json	132	fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05
ignition/script-python/cap25b/code.py	22	b10c690bf702ea0a33f59131ff7c2ee2f3061aaf37ae6bbc5f727d4877647214
ignition/script-python/cap25b/resource.json	132	fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05
project.json	108	9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14
[exit 0]
~~~

#### members of the case zip noview

`bash capture25.sh members $STATE/c8/a-noview.zip` @ 2026-10-22T14:07:56Z

~~~
member	bytes	sha256
ignition/	0	-
ignition/script-python/	0	-
ignition/script-python/cap25/	0	-
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/resource.json	100	cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938
ignition/script-python/cap25b/	0	-
ignition/script-python/cap25b/code.py	22	b10c690bf702ea0a33f59131ff7c2ee2f3061aaf37ae6bbc5f727d4877647214
ignition/script-python/cap25b/resource.json	100	cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938
project.json	59	5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354
[exit 0]
~~~

#### POST import CAP25-C8A overwrite=true (case noview)

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/a-noview.zip '$BASE/data/api/v1/projects/import/CAP25-C8A?overwrite=true'` @ 2026-10-22T14:07:56Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:07:56 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 49

{"success":true,"changes":[{"name":"CAP25-C8A"}]}
[curl http=200 total=0.021969s]
~~~

#### GET projects/export/CAP25-C8A (after noview)

`curl -sS -g -D - -o '$STATE/c8/c8a-noview.post.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C8A'` @ 2026-10-22T14:07:56Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:07:57 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C8A_20261022140757.zip"
Content-Length: 1151

size_bytes=1151
sha256=c7833c46b1f8c97dd301677ce609f494f5c84db17ab96b525773edff524348ea
[curl http=200 total=0.012801s]
~~~

#### members of the export after case noview

`bash capture25.sh members $STATE/c8/c8a-noview.post.zip` @ 2026-10-22T14:07:57Z

~~~
member	bytes	sha256
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/resource.json	132	fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05
ignition/script-python/cap25b/code.py	22	b10c690bf702ea0a33f59131ff7c2ee2f3061aaf37ae6bbc5f727d4877647214
ignition/script-python/cap25b/resource.json	132	fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05
project.json	108	9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14
[exit 0]
~~~

Script reading (noview): before: folder B (script-python/cap25b/) entries 2, view folder entries 2, folder A code.py entries 1, folder A resource.json entries 1, project.json entries 1, entries in all 7. After: folder B (script-python/cap25b/) entries 2, view folder entries 0, folder A code.py entries 1, folder A resource.json entries 1, project.json entries 1, entries in all 5.

### case nocode

#### POST import CAP25-C8A overwrite=true (restore the full zip)

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/a-full.zip '$BASE/data/api/v1/projects/import/CAP25-C8A?overwrite=true'` @ 2026-10-22T14:07:57Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:07:57 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 49

{"success":true,"changes":[{"name":"CAP25-C8A"}]}
[curl http=200 total=0.019745s]
~~~

#### GET projects/export/CAP25-C8A (before nocode)

`curl -sS -g -D - -o '$STATE/c8/c8a-nocode.pre.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C8A'` @ 2026-10-22T14:07:57Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:07:57 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C8A_20261022140757.zip"
Content-Length: 1748

size_bytes=1748
sha256=0a3e2fd2ba76322fc417f13249c3ae20a8c4601ca8f4992bc54f78c5626b09f1
[curl http=200 total=0.010467s]
~~~

#### members of the export before case nocode

`bash capture25.sh members $STATE/c8/c8a-nocode.pre.zip` @ 2026-10-22T14:07:57Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/views/Cap25/resource.json	134	34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/resource.json	132	fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05
ignition/script-python/cap25b/code.py	22	b10c690bf702ea0a33f59131ff7c2ee2f3061aaf37ae6bbc5f727d4877647214
ignition/script-python/cap25b/resource.json	132	fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05
project.json	108	9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14
[exit 0]
~~~

#### members of the case zip nocode

`bash capture25.sh members $STATE/c8/a-nocode.zip` @ 2026-10-22T14:07:57Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/	0	-
com.inductiveautomation.perspective/views/	0	-
com.inductiveautomation.perspective/views/Cap25/	0	-
com.inductiveautomation.perspective/views/Cap25/resource.json	102	87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/	0	-
ignition/script-python/	0	-
ignition/script-python/cap25/	0	-
ignition/script-python/cap25/resource.json	100	cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938
ignition/script-python/cap25b/	0	-
ignition/script-python/cap25b/code.py	22	b10c690bf702ea0a33f59131ff7c2ee2f3061aaf37ae6bbc5f727d4877647214
ignition/script-python/cap25b/resource.json	100	cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938
project.json	59	5c297c267180f28d6e9c446800e0686e15213c05f550dabb133b2dbda9219354
[exit 0]
~~~

#### POST import CAP25-C8A overwrite=true (case nocode)

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/a-nocode.zip '$BASE/data/api/v1/projects/import/CAP25-C8A?overwrite=true'` @ 2026-10-22T14:07:57Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:07:57 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 49

{"success":true,"changes":[{"name":"CAP25-C8A"}]}
[curl http=200 total=0.042141s]
~~~

#### GET projects/export/CAP25-C8A (after nocode)

`curl -sS -g -D - -o '$STATE/c8/c8a-nocode.post.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C8A'` @ 2026-10-22T14:07:57Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:07:57 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C8A_20261022140757.zip"
Content-Length: 1553

size_bytes=1553
sha256=0d736e07f5af040a864346fd9cb0f8ae9632daa858fc39df5a0f48b518198a8e
[curl http=200 total=0.006636s]
~~~

#### members of the export after case nocode

`bash capture25.sh members $STATE/c8/c8a-nocode.post.zip` @ 2026-10-22T14:07:57Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/views/Cap25/resource.json	134	34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/script-python/cap25/resource.json	115	5aee0d6c5055403f299d4df6bc36f73e74847a26f38c0d32c3391fca824d07b8
ignition/script-python/cap25b/code.py	22	b10c690bf702ea0a33f59131ff7c2ee2f3061aaf37ae6bbc5f727d4877647214
ignition/script-python/cap25b/resource.json	132	fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05
project.json	108	9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14
[exit 0]
~~~

Script reading (nocode): before: folder B (script-python/cap25b/) entries 2, view folder entries 2, folder A code.py entries 1, folder A resource.json entries 1, project.json entries 1, entries in all 7. After: folder B (script-python/cap25b/) entries 2, view folder entries 2, folder A code.py entries 0, folder A resource.json entries 1, project.json entries 1, entries in all 6.

### case projonly

#### POST import CAP25-C8A overwrite=true (restore the full zip)

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/a-full.zip '$BASE/data/api/v1/projects/import/CAP25-C8A?overwrite=true'` @ 2026-10-22T14:07:57Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:07:57 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 49

{"success":true,"changes":[{"name":"CAP25-C8A"}]}
[curl http=200 total=0.023137s]
~~~

#### GET projects/export/CAP25-C8A (before projonly)

`curl -sS -g -D - -o '$STATE/c8/c8a-projonly.pre.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C8A'` @ 2026-10-22T14:07:57Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:07:57 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C8A_20261022140757.zip"
Content-Length: 1748

size_bytes=1748
sha256=0a3e2fd2ba76322fc417f13249c3ae20a8c4601ca8f4992bc54f78c5626b09f1
[curl http=200 total=0.007730s]
~~~

#### members of the export before case projonly

`bash capture25.sh members $STATE/c8/c8a-projonly.pre.zip` @ 2026-10-22T14:07:58Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/views/Cap25/resource.json	134	34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/resource.json	132	fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05
ignition/script-python/cap25b/code.py	22	b10c690bf702ea0a33f59131ff7c2ee2f3061aaf37ae6bbc5f727d4877647214
ignition/script-python/cap25b/resource.json	132	fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05
project.json	108	9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14
[exit 0]
~~~

#### members of the case zip projonly

`bash capture25.sh members $STATE/c8/a-projonly.zip` @ 2026-10-22T14:07:58Z

~~~
member	bytes	sha256
project.json	57	367ead9a74c4c9ccce9ca35d0be1e5a473479e91c8d7dc6dfdb77ecf681f7338
[exit 0]
~~~

#### POST import CAP25-C8A overwrite=true (case projonly)

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/a-projonly.zip '$BASE/data/api/v1/projects/import/CAP25-C8A?overwrite=true'` @ 2026-10-22T14:07:58Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:07:58 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.012873s]
~~~

#### GET projects/export/CAP25-C8A (after projonly)

`curl -sS -g -D - -o '$STATE/c8/c8a-projonly.post.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C8A'` @ 2026-10-22T14:07:58Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:07:58 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C8A_20261022140758.zip"
Content-Length: 223

size_bytes=223
sha256=0f29c2a500ea1f25e99a768785f4e519b78648b5815c94f58e5f6c29c95c23ba
[curl http=200 total=0.005311s]
~~~

#### members of the export after case projonly

`bash capture25.sh members $STATE/c8/c8a-projonly.post.zip` @ 2026-10-22T14:07:58Z

~~~
member	bytes	sha256
project.json	106	d54a12f93977b24c776dbe07e62809461d03a072446e833e43c705971c0309ff
[exit 0]
~~~

Script reading (projonly): before: folder B (script-python/cap25b/) entries 2, view folder entries 2, folder A code.py entries 1, folder A resource.json entries 1, project.json entries 1, entries in all 7. After: folder B (script-python/cap25b/) entries 0, view folder entries 0, folder A code.py entries 0, folder A resource.json entries 0, project.json entries 1, entries in all 1.

#### DELETE project CAP25-C8A

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-C8A?confirm=true'` @ 2026-10-22T14:07:58Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:07:58 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.005790s]
~~~

#### GET projects/list (limit=-1), end of block

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/list?limit=-1'` @ 2026-10-22T14:07:58Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:07:58 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 70

{"items":[],"metadata":{"total":0,"matching":0,"limit":-1,"offset":0}}
[curl http=200 total=0.006229s]
~~~

Script reading: 0 CAP25-* project(s) listed after this block; the rig list holds 0 project(s) in all.

#### folder A resource.json files array in the export after case nocode (names only)

`bash capture25.sh files $STATE/c8/c8a-nocode.post.zip ignition/script-python/cap25/resource.json` @ 2026-10-22T14:08:23Z

~~~
[]
[exit 0]
~~~

### C8a: reading

Script reading of the four cases above (counts are from the per-case reading lines):

| Case | Omitted from the zip | Entries before, after | Deleted | Import answer |
| --- | --- | --- | --- | --- |
| (i) noB | script folder B (`cap25b`: `code.py`, `resource.json`) | 2 to 0 | yes | 200, `success: true`, `changes: [{"name": "CAP25-C8A"}]` |
| (ii) noview | the Perspective view folder (`view.json`, `resource.json`) | 2 to 0 | yes | 200, `success: true`, `changes: [{"name": "CAP25-C8A"}]` |
| (iii) nocode | `code.py` of folder A, whose `resource.json` still lists it | 1 to 0 | yes | 200, `success: true`, `changes: [{"name": "CAP25-C8A"}]` |
| (iv) projonly | everything except `project.json` | 6 resource entries to 0 | yes, all | 200, `success: true`, `changes: []` |

1. **`import?overwrite=true` replaces the whole project tree: every resource the zip omits is deleted.** It matches the cross-layout probe in C8c, where the old paths vanished too. A zip built from a partial or empty checkout silently empties the project, and the answer is a success.
2. **The `changes` list never names what was deleted.** It carries the project name when the project holds resources after the import and is `[]` when it holds none (as for the `projonly` case and for every resource-less import in C8e).
3. **Case (iii) leaves the resource and loses its file.** The exported folder A `resource.json` lists `files: []` (the dangling `code.py` entry is rewritten, as in C8d `listedmissing`), so the resource still exists as an empty script.
4. **Input to 25-03 K5 and 25-11.** The deploy must compare the artifact's member set with the live project's export before any overwrite and treat a member that would disappear as a gated change, because the gateway reports none of it.

## C8b

C8b: round trip of a gateway-produced export. Part 1, the synthetic project: import the synthetic checkout as `CAP25-C8B`, export it (E1), import E1 back with overwrite=true, export again (E2). Part 2, the WHK-Global allowlist export from C8c (the export after overwrite=false, `CAP25-WHK.1.zip`) is imported into `CAP25-C8B-WHK`, exported (E1b), imported again with overwrite=true and exported (E2b). Equality is by sha256 per member, raw and after `resource.json` normalisation (drop `attributes.lastModification` and `lastModificationSignature`, sort keys). Only member names, field names and counts are recorded.

### Part 1: synthetic project

#### POST import CAP25-C8B overwrite=false (synthetic checkout)

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/b-synth.zip '$BASE/data/api/v1/projects/import/CAP25-C8B?overwrite=false'` @ 2026-10-22T14:08:23Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:08:23 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 49

{"success":true,"changes":[{"name":"CAP25-C8B"}]}
[curl http=200 total=0.066900s]
~~~

#### GET projects/export/CAP25-C8B (E1)

`curl -sS -g -D - -o '$STATE/c8/c8b-E1.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C8B'` @ 2026-10-22T14:08:23Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:08:23 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C8B_20261022140823.zip"
Content-Length: 1283

size_bytes=1283
sha256=bcf98f3ec8784cd7442e5fbf5bfb7797d104a6582ff746896614d8e402e36780
[curl http=200 total=0.008032s]
~~~

#### POST import CAP25-C8B overwrite=true (E1 back, gateway-produced zip)

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/c8b-E1.zip '$BASE/data/api/v1/projects/import/CAP25-C8B?overwrite=true'` @ 2026-10-22T14:08:23Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:08:23 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 49

{"success":true,"changes":[{"name":"CAP25-C8B"}]}
[curl http=200 total=0.014477s]
~~~

#### GET projects/export/CAP25-C8B (E2)

`curl -sS -g -D - -o '$STATE/c8/c8b-E2.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C8B'` @ 2026-10-22T14:08:23Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:08:23 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C8B_20261022140823.zip"
Content-Length: 1283

size_bytes=1283
sha256=bcf98f3ec8784cd7442e5fbf5bfb7797d104a6582ff746896614d8e402e36780
[curl http=200 total=0.006367s]
~~~

#### members of E1

`bash capture25.sh members $STATE/c8/c8b-E1.zip` @ 2026-10-22T14:08:23Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/views/Cap25/resource.json	134	34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/resource.json	132	fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05
project.json	108	9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14
[exit 0]
~~~

#### members of E2

`bash capture25.sh members $STATE/c8/c8b-E2.zip` @ 2026-10-22T14:08:23Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/views/Cap25/resource.json	134	34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e
com.inductiveautomation.perspective/views/Cap25/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/script-python/cap25/code.py	22	5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352
ignition/script-python/cap25/resource.json	132	fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05
project.json	108	9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14
[exit 0]
~~~

#### table: E1 against E2 (per member sha256, raw and after normalisation)

`bash capture25.sh cmp $STATE/c8/c8b-E1.zip $STATE/c8/c8b-E2.zip` @ 2026-10-22T14:08:23Z

~~~
member (user-path key) | in zip | in export | sha256 zip | sha256 export | equal | equal after normalisation
com.inductiveautomation.perspective/views/Cap25/resource.json | yes | yes | 34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e | 34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e | yes | yes
com.inductiveautomation.perspective/views/Cap25/view.json | yes | yes | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385 | yes | yes
ignition/script-python/cap25/code.py | yes | yes | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | 5b76d0962c09ab4ee309fac65fad3568c97abdec983b405146ae3e86a235e352 | yes | yes
ignition/script-python/cap25/resource.json | yes | yes | fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05 | fe837844d74674601b73355a4311016fbedc1e4d4bf5d6dbab9fbf2d23cf3a05 | yes | yes
project.json | yes | yes | 9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14 | 9e2e019458b0e16f248f39d1299c9073d6bcf37affbe97d43f4234313a96ec14 | yes | yes
directory entries: zip 0, export 0
project.json: byte-identical
[exit 0]
~~~

#### entry order and compression method of E1

`bash capture25.sh zinfo $STATE/c8/c8b-E1.zip` @ 2026-10-22T14:08:24Z

~~~
index	method	name
0	deflated	project.json
1	deflated	ignition/script-python/cap25/resource.json
2	deflated	ignition/script-python/cap25/code.py
3	deflated	com.inductiveautomation.perspective/views/Cap25/resource.json
4	deflated	com.inductiveautomation.perspective/views/Cap25/view.json
[exit 0]
~~~

#### entry order and compression method of E2

`bash capture25.sh zinfo $STATE/c8/c8b-E2.zip` @ 2026-10-22T14:08:24Z

~~~
index	method	name
0	deflated	project.json
1	deflated	ignition/script-python/cap25/resource.json
2	deflated	ignition/script-python/cap25/code.py
3	deflated	com.inductiveautomation.perspective/views/Cap25/resource.json
4	deflated	com.inductiveautomation.perspective/views/Cap25/view.json
[exit 0]
~~~

#### DELETE project CAP25-C8B

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-C8B?confirm=true'` @ 2026-10-22T14:08:24Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:08:24 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.008324s]
~~~

### Part 2: WHK-Global allowlist export

#### POST import CAP25-C8B-WHK overwrite=false (the C8c export, E1)

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/CAP25-WHK.1.zip '$BASE/data/api/v1/projects/import/CAP25-C8B-WHK?overwrite=false'` @ 2026-10-22T14:08:24Z

~~~
HTTP/1.1 100 Continue

HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:08:24 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 53

{"success":true,"changes":[{"name":"CAP25-C8B-WHK"}]}
[curl http=200 total=18.256869s]
~~~

#### GET projects/export/CAP25-C8B-WHK (E1b)

`curl -sS -g -D - -o '$STATE/c8/c8b-E1b.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C8B-WHK'` @ 2026-10-22T14:08:42Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:08:42 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C8B-WHK_20261022140842.zip"
Content-Length: 4773794

size_bytes=4773794
sha256=82c6e6eed77e0eca3049e4fb265fe7f197bec76eccd437c43eee8c35f88afada
[curl http=200 total=1.337183s]
~~~

#### POST import CAP25-C8B-WHK overwrite=true (E1 again)

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/CAP25-WHK.1.zip '$BASE/data/api/v1/projects/import/CAP25-C8B-WHK?overwrite=true'` @ 2026-10-22T14:08:44Z

~~~
HTTP/1.1 100 Continue

HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:08:44 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 53

{"success":true,"changes":[{"name":"CAP25-C8B-WHK"}]}
[curl http=200 total=1.433557s]
~~~

#### GET projects/export/CAP25-C8B-WHK (E2b)

`curl -sS -g -D - -o '$STATE/c8/c8b-E2b.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C8B-WHK'` @ 2026-10-22T14:08:45Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:08:45 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C8B-WHK_20261022140845.zip"
Content-Length: 4773794

size_bytes=4773794
sha256=57aa6c4a30859f93dbb57537f567b5b62e321860068453b0894da34e4d2f7500
[curl http=200 total=1.134573s]
~~~

#### E1 against E1b (import overwrite=false, export)

`bash capture25.sh cmpsum $STATE/c8/CAP25-WHK.1.zip $STATE/c8/c8b-E1b.zip` @ 2026-10-22T14:08:46Z

~~~
members (files): zip 2213, export 2213, in both 2213, zip only 0, export only 0
directory entries: zip 0, export 0
in both, sha256 differs raw: 18
in both, sha256 differs after resource.json normalisation: 18
first 40 zip-only keys:
first 40 export-only keys:
first 20 raw-differing keys:
  com.inductiveautomation.webdev/BarrelPrinting/POST_PrintSummary/resource.json
  com.inductiveautomation.webdev/BarrelPrinting/POST_Status/resource.json
  com.inductiveautomation.webdev/DocumentManagement/get/resource.json
  com.inductiveautomation.webdev/GatewayAPI/ListNamedQueries/resource.json
  com.inductiveautomation.webdev/GatewayAPI/RunNamedQueries/resource.json
  com.inductiveautomation.webdev/GatewayAPI/getAllTagPaths/resource.json
  com.inductiveautomation.webdev/GatewayAPI/getGroupData/resource.json
  com.inductiveautomation.webdev/GatewayAPI/getSystemHealth/resource.json
  com.inductiveautomation.webdev/GatewayAPI/getTagValueUnits/resource.json
  com.inductiveautomation.webdev/GatewayAPI/pull/resource.json
  com.inductiveautomation.webdev/GatewayAPI/qrcode/resource.json
  com.inductiveautomation.webdev/GatewayAPI/runTests/resource.json
  com.inductiveautomation.webdev/Verkada/Stream/getStream/resource.json
  com.inductiveautomation.webdev/exchange/perspective-screenshot/screenshot/resource.json
  com.inductiveautomation.webdev/formbricks/formField/resource.json
  com.inductiveautomation.webdev/integration/atlassian/StatusPage/ServeStatusPageToast/resource.json
  com.inductiveautomation.webdev/integration/atlassian/confluence/categoryList/resource.json
  com.inductiveautomation.webdev/integration/atlassian/confluence/searchPages/resource.json
first 20 differing keys after normalisation:
  com.inductiveautomation.webdev/BarrelPrinting/POST_PrintSummary/resource.json
  com.inductiveautomation.webdev/BarrelPrinting/POST_Status/resource.json
  com.inductiveautomation.webdev/DocumentManagement/get/resource.json
  com.inductiveautomation.webdev/GatewayAPI/ListNamedQueries/resource.json
  com.inductiveautomation.webdev/GatewayAPI/RunNamedQueries/resource.json
  com.inductiveautomation.webdev/GatewayAPI/getAllTagPaths/resource.json
  com.inductiveautomation.webdev/GatewayAPI/getGroupData/resource.json
  com.inductiveautomation.webdev/GatewayAPI/getSystemHealth/resource.json
  com.inductiveautomation.webdev/GatewayAPI/getTagValueUnits/resource.json
  com.inductiveautomation.webdev/GatewayAPI/pull/resource.json
  com.inductiveautomation.webdev/GatewayAPI/qrcode/resource.json
  com.inductiveautomation.webdev/GatewayAPI/runTests/resource.json
  com.inductiveautomation.webdev/Verkada/Stream/getStream/resource.json
  com.inductiveautomation.webdev/exchange/perspective-screenshot/screenshot/resource.json
  com.inductiveautomation.webdev/formbricks/formField/resource.json
  com.inductiveautomation.webdev/integration/atlassian/StatusPage/ServeStatusPageToast/resource.json
  com.inductiveautomation.webdev/integration/atlassian/confluence/categoryList/resource.json
  com.inductiveautomation.webdev/integration/atlassian/confluence/searchPages/resource.json
project.json: byte-identical
[exit 0]
~~~

#### E1 against E2b (import overwrite=true, export)

`bash capture25.sh cmpsum $STATE/c8/CAP25-WHK.1.zip $STATE/c8/c8b-E2b.zip` @ 2026-10-22T14:08:47Z

~~~
members (files): zip 2213, export 2213, in both 2213, zip only 0, export only 0
directory entries: zip 0, export 0
in both, sha256 differs raw: 18
in both, sha256 differs after resource.json normalisation: 18
first 40 zip-only keys:
first 40 export-only keys:
first 20 raw-differing keys:
  com.inductiveautomation.webdev/BarrelPrinting/POST_PrintSummary/resource.json
  com.inductiveautomation.webdev/BarrelPrinting/POST_Status/resource.json
  com.inductiveautomation.webdev/DocumentManagement/get/resource.json
  com.inductiveautomation.webdev/GatewayAPI/ListNamedQueries/resource.json
  com.inductiveautomation.webdev/GatewayAPI/RunNamedQueries/resource.json
  com.inductiveautomation.webdev/GatewayAPI/getAllTagPaths/resource.json
  com.inductiveautomation.webdev/GatewayAPI/getGroupData/resource.json
  com.inductiveautomation.webdev/GatewayAPI/getSystemHealth/resource.json
  com.inductiveautomation.webdev/GatewayAPI/getTagValueUnits/resource.json
  com.inductiveautomation.webdev/GatewayAPI/pull/resource.json
  com.inductiveautomation.webdev/GatewayAPI/qrcode/resource.json
  com.inductiveautomation.webdev/GatewayAPI/runTests/resource.json
  com.inductiveautomation.webdev/Verkada/Stream/getStream/resource.json
  com.inductiveautomation.webdev/exchange/perspective-screenshot/screenshot/resource.json
  com.inductiveautomation.webdev/formbricks/formField/resource.json
  com.inductiveautomation.webdev/integration/atlassian/StatusPage/ServeStatusPageToast/resource.json
  com.inductiveautomation.webdev/integration/atlassian/confluence/categoryList/resource.json
  com.inductiveautomation.webdev/integration/atlassian/confluence/searchPages/resource.json
first 20 differing keys after normalisation:
  com.inductiveautomation.webdev/BarrelPrinting/POST_PrintSummary/resource.json
  com.inductiveautomation.webdev/BarrelPrinting/POST_Status/resource.json
  com.inductiveautomation.webdev/DocumentManagement/get/resource.json
  com.inductiveautomation.webdev/GatewayAPI/ListNamedQueries/resource.json
  com.inductiveautomation.webdev/GatewayAPI/RunNamedQueries/resource.json
  com.inductiveautomation.webdev/GatewayAPI/getAllTagPaths/resource.json
  com.inductiveautomation.webdev/GatewayAPI/getGroupData/resource.json
  com.inductiveautomation.webdev/GatewayAPI/getSystemHealth/resource.json
  com.inductiveautomation.webdev/GatewayAPI/getTagValueUnits/resource.json
  com.inductiveautomation.webdev/GatewayAPI/pull/resource.json
  com.inductiveautomation.webdev/GatewayAPI/qrcode/resource.json
  com.inductiveautomation.webdev/GatewayAPI/runTests/resource.json
  com.inductiveautomation.webdev/Verkada/Stream/getStream/resource.json
  com.inductiveautomation.webdev/exchange/perspective-screenshot/screenshot/resource.json
  com.inductiveautomation.webdev/formbricks/formField/resource.json
  com.inductiveautomation.webdev/integration/atlassian/StatusPage/ServeStatusPageToast/resource.json
  com.inductiveautomation.webdev/integration/atlassian/confluence/categoryList/resource.json
  com.inductiveautomation.webdev/integration/atlassian/confluence/searchPages/resource.json
project.json: byte-identical
[exit 0]
~~~

#### E1b against E2b

`bash capture25.sh cmpsum $STATE/c8/c8b-E1b.zip $STATE/c8/c8b-E2b.zip` @ 2026-10-22T14:08:47Z

~~~
members (files): zip 2213, export 2213, in both 2213, zip only 0, export only 0
directory entries: zip 0, export 0
in both, sha256 differs raw: 0
in both, sha256 differs after resource.json normalisation: 0
first 40 zip-only keys:
first 40 export-only keys:
first 20 raw-differing keys:
first 20 differing keys after normalisation:
project.json: byte-identical
[exit 0]
~~~

#### resource.json fields still differing, E1 against E2b

`bash capture25.sh rjdiff $STATE/c8/CAP25-WHK.1.zip $STATE/c8/c8b-E2b.zip` @ 2026-10-22T14:08:48Z

~~~
resource.json members in both: 900, equal after normalisation: 882, differing: 18
  differing field files (same set, order differs): 18 member(s)
[exit 0]
~~~

#### entry order and compression method: first 8 entries of E1

`bash -c \"\$0\"\ zinfo\ \"\$1\"\ \|\ head\ -9 capture25.sh $STATE/c8/CAP25-WHK.1.zip` @ 2026-10-22T14:08:48Z

~~~
index	method	name
0	deflated	project.json
1	deflated	ignition/script-python/core/mes/changeover/client/resource.json
2	deflated	ignition/script-python/core/mes/changeover/client/code.py
3	deflated	com.inductiveautomation.perspective/views/Framework/Fields/Text/resource.json
4	deflated	com.inductiveautomation.perspective/views/Framework/Fields/Text/view.json
5	deflated	com.inductiveautomation.perspective/views/Framework/Fields/Text/thumbnail.png
6	deflated	ignition/named-query/Exchange/Comments/Get/Search Comments Null Instance/resource.json
7	deflated	ignition/named-query/Exchange/Comments/Get/Search Comments Null Instance/query.sql
Traceback (most recent call last):
  File "<stdin>", line 8, in <module>
BrokenPipeError: [Errno 32] Broken pipe
Exception ignored while flushing sys.stdout:
BrokenPipeError: [Errno 32] Broken pipe
[exit 0]
~~~

#### entry order and compression method: first 8 entries of E2b

`bash -c \"\$0\"\ zinfo\ \"\$1\"\ \|\ head\ -9 capture25.sh $STATE/c8/c8b-E2b.zip` @ 2026-10-22T14:08:49Z

~~~
index	method	name
0	deflated	project.json
1	deflated	com.inductiveautomation.webdev/resources/GatewayAPI/getGroupData/resource.json
2	deflated	com.inductiveautomation.webdev/resources/GatewayAPI/getGroupData/config.json
3	deflated	com.inductiveautomation.webdev/resources/GatewayAPI/getGroupData/doTrace.py
4	deflated	com.inductiveautomation.webdev/resources/GatewayAPI/getGroupData/doPut.py
5	deflated	com.inductiveautomation.webdev/resources/GatewayAPI/getGroupData/doHead.py
6	deflated	com.inductiveautomation.webdev/resources/GatewayAPI/getGroupData/doPatch.py
7	deflated	com.inductiveautomation.webdev/resources/GatewayAPI/getGroupData/doOptions.py
Traceback (most recent call last):
  File "<stdin>", line 8, in <module>
BrokenPipeError: [Errno 32] Broken pipe
Exception ignored while flushing sys.stdout:
BrokenPipeError: [Errno 32] Broken pipe
[exit 0]
~~~

#### DELETE project CAP25-C8B-WHK

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-C8B-WHK?confirm=true'` @ 2026-10-22T14:08:49Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:08:49 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 53

{"success":true,"changes":[{"name":"CAP25-C8B-WHK"}]}
[curl http=200 total=0.421761s]
~~~

#### GET projects/list (limit=-1), end of block

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/list?limit=-1'` @ 2026-10-22T14:08:50Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:08:50 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 70

{"items":[],"metadata":{"total":0,"matching":0,"limit":-1,"offset":0}}
[curl http=200 total=0.027860s]
~~~

Script reading: 0 CAP25-* project(s) listed after this block; the rig list holds 0 project(s) in all.

### C8b: reading

Script reading of the two parts above:

1. **The synthetic export is a fixed point.** E1 and E2 are byte-identical member for member (`project.json` included), in the same entry order, every entry deflated, no directory entries. E1 already carries the two `project.json` keys the gateway adds on first import (`description`, `parent`; see C8c), so a repo `project.json` that lacks them differs from its own export by those two keys.
2. **WHK-Global: nothing is added or lost.** E1 (the C8c export), E1b (export after importing E1) and E2b (after importing E1 again with overwrite=true) hold the same 2,213 file members and no directory entry; `project.json` is byte-identical in all three; E1b and E2b are identical member for member (0 differing).
3. **What still differs between E1 and E1b/E2b: 18 members, all WebDev `resource.json`, one field.** The differing field is `files`, which holds the same names in another order (the `rjdiff` block). Raw and normalised counts are both 18, so `lastModification` and `lastModificationSignature` agree between the exports; after sorting `files` nothing differs. Entry order also differs between E1 and E2b (their first entries differ), so a digest must not depend on zip entry order. The two `BrokenPipeError` traces in the entry-order blocks are an artefact of `head -9` closing the pipe (the script now uses `sed -n 1,9p`); the nine rows above each trace are complete.
4. **Input to 25-03 K4 and 25-06.** The canonical digest has to: hash members by name, not by position; ignore directory entries; drop `attributes.lastModification` and `lastModificationSignature` from every `resource.json`; sort each `resource.json` `files` array; read an absent `description` or `parent` in `project.json` as `""`; and hash only the files a `resource.json` lists (C8d). With those rules both WHK-Global round trips are stable and the synthetic one is byte-exact.

## C8e

C8e: parent edge cases and project-list membership. `CAP25-PARENT-NI` (`inheritable: false`) and `CAP25-PARENT-I` (`inheritable: true`) hold no resources. Child zips are imported with overwrite=false: `CAP25-CHILD-MISSING` (parent `CAP25-NOPE`, absent), `CAP25-CHILD-SELF` (parent is itself), `CAP25-CHILD-NI` (parent `CAP25-PARENT-NI`) and, as a control, `CAP25-CHILD-OK` (parent `CAP25-PARENT-I`). `CAP25-DISABLED` is `enabled: false`.

### parents

#### POST import CAP25-PARENT-NI overwrite=false

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/e-ni.zip '$BASE/data/api/v1/projects/import/CAP25-PARENT-NI?overwrite=false'` @ 2026-10-22T14:08:50Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:08:50 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=28.640004s]
~~~

#### POST import CAP25-PARENT-I overwrite=false

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/e-i.zip '$BASE/data/api/v1/projects/import/CAP25-PARENT-I?overwrite=false'` @ 2026-10-22T14:09:19Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:09:19 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.539978s]
~~~

#### POST import CAP25-DISABLED overwrite=false

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/e-dis.zip '$BASE/data/api/v1/projects/import/CAP25-DISABLED?overwrite=false'` @ 2026-10-22T14:09:19Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:09:19 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.066833s]
~~~

### children

#### POST import CAP25-CHILD-MISSING overwrite=false

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/e-c-missing.zip '$BASE/data/api/v1/projects/import/CAP25-CHILD-MISSING?overwrite=false'` @ 2026-10-22T14:09:20Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:09:20 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.386235s]
~~~

#### POST import CAP25-CHILD-SELF overwrite=false

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/e-c-self.zip '$BASE/data/api/v1/projects/import/CAP25-CHILD-SELF?overwrite=false'` @ 2026-10-22T14:09:20Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:09:20 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.321655s]
~~~

#### POST import CAP25-CHILD-NI overwrite=false

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/e-c-ni.zip '$BASE/data/api/v1/projects/import/CAP25-CHILD-NI?overwrite=false'` @ 2026-10-22T14:09:20Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:09:20 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.102095s]
~~~

#### POST import CAP25-CHILD-OK overwrite=false

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/e-c-ok.zip '$BASE/data/api/v1/projects/import/CAP25-CHILD-OK?overwrite=false'` @ 2026-10-22T14:09:21Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:09:21 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.082018s]
~~~

### find and list membership

#### GET projects/find/CAP25-PARENT-NI (status, parent, enabled, inheritable)

`bash capture25.sh findsel CAP25-PARENT-NI` @ 2026-10-22T14:09:21Z

~~~
status=200 {"parent":"","enabled":true,"inheritable":false}
[exit 0]
~~~

#### GET projects/find/CAP25-PARENT-I (status, parent, enabled, inheritable)

`bash capture25.sh findsel CAP25-PARENT-I` @ 2026-10-22T14:09:21Z

~~~
status=200 {"parent":"","enabled":true,"inheritable":true}
[exit 0]
~~~

#### GET projects/find/CAP25-DISABLED (status, parent, enabled, inheritable)

`bash capture25.sh findsel CAP25-DISABLED` @ 2026-10-22T14:09:21Z

~~~
status=200 {"parent":"","enabled":false,"inheritable":false}
[exit 0]
~~~

#### GET projects/find/CAP25-CHILD-MISSING (status, parent, enabled, inheritable)

`bash capture25.sh findsel CAP25-CHILD-MISSING` @ 2026-10-22T14:09:21Z

~~~
status=200 {"parent":"CAP25-NOPE","enabled":true,"inheritable":false}
[exit 0]
~~~

#### GET projects/find/CAP25-CHILD-SELF (status, parent, enabled, inheritable)

`bash capture25.sh findsel CAP25-CHILD-SELF` @ 2026-10-22T14:09:21Z

~~~
status=200 {"parent":"CAP25-CHILD-SELF","enabled":true,"inheritable":false}
[exit 0]
~~~

#### GET projects/find/CAP25-CHILD-NI (status, parent, enabled, inheritable)

`bash capture25.sh findsel CAP25-CHILD-NI` @ 2026-10-22T14:09:21Z

~~~
status=200 {"parent":"CAP25-PARENT-NI","enabled":true,"inheritable":false}
[exit 0]
~~~

#### GET projects/find/CAP25-CHILD-OK (status, parent, enabled, inheritable)

`bash capture25.sh findsel CAP25-CHILD-OK` @ 2026-10-22T14:09:22Z

~~~
status=200 {"parent":"CAP25-PARENT-I","enabled":true,"inheritable":false}
[exit 0]
~~~

#### GET projects/list (limit=-1)

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/list?limit=-1'` @ 2026-10-22T14:09:22Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:09:22 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 1781

{"items":[{"name":"CAP25-CHILD-MISSING","description":"","title":"CAP25 child of a missing project","enabled":true,"parent":"CAP25-NOPE","inheritable":false,"invalidParent":true,"mutable":true,"userSource":"","identityProvider":"","tagProvider":"","defaultDb":""},{"name":"CAP25-CHILD-NI","description":"","title":"CAP25 child of a non-inheritable project","enabled":true,"parent":"CAP25-PARENT-NI","inheritable":false,"invalidParent":true,"mutable":true,"userSource":"","identityProvider":"","tagProvider":"","defaultDb":""},{"name":"CAP25-CHILD-OK","description":"","title":"CAP25 child of an inheritable project","enabled":true,"parent":"CAP25-PARENT-I","inheritable":false,"invalidParent":false,"mutable":true,"userSource":"","identityProvider":"","tagProvider":"","defaultDb":""},{"name":"CAP25-CHILD-SELF","description":"","title":"CAP25 child of itself","enabled":true,"parent":"CAP25-CHILD-SELF","inheritable":false,"invalidParent":true,"mutable":true,"userSource":"","identityProvider":"","tagProvider":"","defaultDb":""},{"name":"CAP25-DISABLED","description":"","title":"CAP25 disabled","enabled":false,"parent":"","inheritable":false,"invalidParent":false,"mutable":true,"userSource":"","identityProvider":"","tagProvider":"","defaultDb":""},{"name":"CAP25-PARENT-I","description":"","title":"CAP25 parent inheritable","enabled":true,"parent":"","inheritable":true,"invalidParent":false,"mutable":true,"userSource":"","identityProvider":"","tagProvider":"","defaultDb":""},{"name":"CAP25-PARENT-NI","description":"","title":"CAP25 parent not inheritable","enabled":true,"parent":"","inheritable":false,"invalidParent":false,"mutable":true,"userSource":"","identityProvider":"","tagProvider":"","defaultDb":""}],"metadata":{"total":7,"matching":7,"limit":-1,"offset":0}}
[curl http=200 total=0.012309s]
~~~

#### GET projects/names

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/names'` @ 2026-10-22T14:09:22Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:09:22 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 259

{"items":[{"name":"CAP25-CHILD-MISSING"},{"name":"CAP25-CHILD-NI"},{"name":"CAP25-CHILD-OK"},{"name":"CAP25-CHILD-SELF"},{"name":"CAP25-DISABLED"},{"name":"CAP25-PARENT-I"},{"name":"CAP25-PARENT-NI"}],"metadata":{"total":7,"matching":7,"limit":-1,"offset":0}}
[curl http=200 total=0.052624s]
~~~

#### GET projects/parents

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/parents'` @ 2026-10-22T14:09:22Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:09:22 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 95

{"items":[{"name":"CAP25-PARENT-I"}],"metadata":{"total":1,"matching":1,"limit":-1,"offset":0}}
[curl http=200 total=0.137325s]
~~~

### cleanup

#### DELETE project CAP25-CHILD-OK

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-CHILD-OK?confirm=true'` @ 2026-10-22T14:09:22Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:09:22 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.107968s]
~~~

#### DELETE project CAP25-CHILD-NI

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-CHILD-NI?confirm=true'` @ 2026-10-22T14:09:22Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:09:22 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.043037s]
~~~

#### DELETE project CAP25-CHILD-SELF

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-CHILD-SELF?confirm=true'` @ 2026-10-22T14:09:22Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:09:22 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.012567s]
~~~

#### DELETE project CAP25-CHILD-MISSING

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-CHILD-MISSING?confirm=true'` @ 2026-10-22T14:09:22Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:09:22 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.014140s]
~~~

#### DELETE project CAP25-DISABLED

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-DISABLED?confirm=true'` @ 2026-10-22T14:09:22Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:09:22 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.039318s]
~~~

#### DELETE project CAP25-PARENT-I

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-PARENT-I?confirm=true'` @ 2026-10-22T14:09:23Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:09:23 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.008798s]
~~~

#### DELETE project CAP25-PARENT-NI

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-PARENT-NI?confirm=true'` @ 2026-10-22T14:09:23Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:09:23 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.024877s]
~~~

#### GET projects/list (limit=-1), end of block

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/list?limit=-1'` @ 2026-10-22T14:09:23Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:09:23 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 70

{"items":[],"metadata":{"total":0,"matching":0,"limit":-1,"offset":0}}
[curl http=200 total=0.008725s]
~~~

Script reading: 0 CAP25-* project(s) listed after this block; the rig list holds 0 project(s) in all.

### C8e: reading

Script reading of the C8e block:

1. **The importer refuses no parent.** `CAP25-CHILD-MISSING` (parent absent), `CAP25-CHILD-SELF` (parent is itself) and `CAP25-CHILD-NI` (parent not inheritable) all import with 200, `success: true`, `changes: []`, and `find` returns the parent string as written. Assumption A5 (a non-inheritable parent is refused) does not hold on 8.3.6.
2. **The gateway flags the bad ones itself.** `projects/list` items carry `invalidParent`: `true` for `CAP25-CHILD-MISSING`, `CAP25-CHILD-SELF` and `CAP25-CHILD-NI`; `false` for `CAP25-CHILD-OK` (parent inheritable) and for every project without a parent.
3. **The list shows disabled and inheritable projects.** `projects/list?limit=-1` holds all seven projects, including `CAP25-DISABLED` (`enabled: false`) and `CAP25-PARENT-I` (`inheritable: true`, no resources); `projects/names` holds the same seven; `projects/parents` holds only `CAP25-PARENT-I`. `find` answers 200 for every one, with the same `parent`, `enabled` and `inheritable` the list shows.
4. **Input to 25-03 K6 and 25-10 (D-05).** Parent presence can be read from `projects/list` without a per-name `find`, and `invalidParent` is the gateway's own verdict to verify against after the import. Because the gateway does not refuse, the parent check before the import is the CLI's job (missing, self, not inheritable).

## C9

C9: what a parent overwrite does to a child. The S5 residual (session impact on a child after a parent overwrite) stays OPEN and no Phase 25 criterion claims it; this capture only sizes the D-06 settle wait and states whether a child's export changes when its parent is overwritten. `CAP25-P` is `inheritable: true` and holds one `script-python` library `cap25lib` (`answer()` returns 1, then 2) and one view. `CAP25-C` (`parent: CAP25-P`) holds one view of its own and the CLI scriptExec route bundle, so a call can run in its project scope (`ign script run --project CAP25-C`). Each poll line is: seconds since the overwrite call, the child's `find` (status, parent, enabled), its export member count and digest (sha256 of the sorted `name sha256` lines, first 16 hex), and the result of `import cap25lib; print cap25lib.answer()` in the child's scope (the scriptExec namespace holds no project libraries until they are imported, so a bare `cap25lib.answer()` answers NameError). `took` is the wall time of the poll itself; a poll that runs long makes the next ones fire back to back.

### setup

#### a throwaway ign config with a profile for the rig (token read from an env var)

`ign profile add ign-deploy-rig http://localhost:18290 --token-env IGN_C9_TOKEN --active --json` @ 2026-10-22T14:20:53Z

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

#### members of the parent zip, answer() returns 1

`bash capture25.sh members $STATE/c8/c9-p1.zip` @ 2026-10-22T14:20:53Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/	0	-
com.inductiveautomation.perspective/views/	0	-
com.inductiveautomation.perspective/views/CapParent/	0	-
com.inductiveautomation.perspective/views/CapParent/resource.json	102	87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1
com.inductiveautomation.perspective/views/CapParent/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
ignition/	0	-
ignition/script-python/	0	-
ignition/script-python/cap25lib/	0	-
ignition/script-python/cap25lib/code.py	27	dd46cb2f80fe2d22e284d503f04d3f7c6a66956a171eaa0e8d128e7e2dee5a0e
ignition/script-python/cap25lib/resource.json	100	cee375e7519cdf79b1cb4d9cf0ff6791712ff664542eac80d6f332e2c68fe938
project.json	62	e1ec22f590c0f975991fddef61523ce02b947020273d3deba1952ebb997dee13
[exit 0]
~~~

#### POST import CAP25-P overwrite=false

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/c9-p1.zip '$BASE/data/api/v1/projects/import/CAP25-P?overwrite=false'` @ 2026-10-22T14:20:53Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:20:53 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 47

{"success":true,"changes":[{"name":"CAP25-P"}]}
[curl http=200 total=1.822155s]
~~~

Script note: the child is first born from `ign webdev deploy --project CAP25-C --with-script-exec` (the only way the CLI installs scriptExec), exported, given `parent: CAP25-P` and a view of its own by `c9child`, deleted, and imported again with overwrite=false, so the imported child is the one under test.

#### ign webdev deploy --project CAP25-C --with-script-exec (route bundle; child born without a parent)

`ign webdev deploy --project CAP25-C --with-script-exec --yes --json` @ 2026-10-22T14:20:55Z

~~~
{
  "ok": true,
  "profile": "ign-deploy-rig",
  "data": {
    "project": "CAP25-C",
    "routes": [
      "tags",
      "tagConfig",
      "alarms",
      "tagHistory",
      "scriptExec"
    ],
    "script_exec": true,
    "secret_rotated": true,
    "import": {
      "changes": [
        {
          "name": "CAP25-C"
        }
      ],
      "success": true
    }
  }
}
[exit 0]
~~~

#### GET projects/export/CAP25-C (route bundle only)

`curl -sS -g -D - -o '$STATE/c8/c9-born.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C'` @ 2026-10-22T14:20:56Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:20:56 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C_20261022142056.zip"
Content-Length: 19300

size_bytes=19300
sha256=039d4b102f4728ed1cef1b3b1255a27026be3363d2c473d776a4d7c341d611d3
[curl http=200 total=0.032942s]
~~~

#### DELETE project CAP25-C

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-C?confirm=true'` @ 2026-10-22T14:20:56Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:20:56 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.009697s]
~~~

#### members of the child zip (route bundle, project.json with parent, one view)

`bash capture25.sh members $STATE/c8/c9-c.zip` @ 2026-10-22T14:20:56Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/views/CapChild/resource.json	102	87c6382945b440ec4783a5e67bbc1515f67eb9dd65a81440e949384e50b3baa1
com.inductiveautomation.perspective/views/CapChild/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
com.inductiveautomation.webdev/resources/cli/alarms/config.json	179	8ba70412f7c537635d0ee2e59eefd5e7a79dbfc5793dd1ad12d7e04a8b6584f0
com.inductiveautomation.webdev/resources/cli/alarms/doPost.py	5685	785268174533176bd27897eb766bc6a4cb7895922a6b4c5ff1456bbe744b2ecd
com.inductiveautomation.webdev/resources/cli/alarms/resource.json	257	7b34b4d48c3cd1e3c341225354e1a7a185ed9302f6bfb2b15aea3e0f5374f9fb
com.inductiveautomation.webdev/resources/cli/scriptExec/config.json	179	8ba70412f7c537635d0ee2e59eefd5e7a79dbfc5793dd1ad12d7e04a8b6584f0
com.inductiveautomation.webdev/resources/cli/scriptExec/doPost.py	7517	83d0f99f2e10185c82d9644f37c8ff32aa90907dfc8f4625c3e4493bd9e2ac96
com.inductiveautomation.webdev/resources/cli/scriptExec/resource.json	257	7b34b4d48c3cd1e3c341225354e1a7a185ed9302f6bfb2b15aea3e0f5374f9fb
com.inductiveautomation.webdev/resources/cli/tagConfig/config.json	179	8ba70412f7c537635d0ee2e59eefd5e7a79dbfc5793dd1ad12d7e04a8b6584f0
com.inductiveautomation.webdev/resources/cli/tagConfig/doPost.py	13189	a08cf41500b47406096b3e8a3809b9db8b57c940e11dbc50f68887beaa5db722
com.inductiveautomation.webdev/resources/cli/tagConfig/resource.json	257	7b34b4d48c3cd1e3c341225354e1a7a185ed9302f6bfb2b15aea3e0f5374f9fb
com.inductiveautomation.webdev/resources/cli/tagHistory/config.json	179	8ba70412f7c537635d0ee2e59eefd5e7a79dbfc5793dd1ad12d7e04a8b6584f0
com.inductiveautomation.webdev/resources/cli/tagHistory/doPost.py	3919	52623d1fa8143ef2a0353141641d881fdf60f3c4860037aa5aa5b4ece76fa964
com.inductiveautomation.webdev/resources/cli/tagHistory/resource.json	257	7b34b4d48c3cd1e3c341225354e1a7a185ed9302f6bfb2b15aea3e0f5374f9fb
com.inductiveautomation.webdev/resources/cli/tags/config.json	179	8ba70412f7c537635d0ee2e59eefd5e7a79dbfc5793dd1ad12d7e04a8b6584f0
com.inductiveautomation.webdev/resources/cli/tags/doPost.py	4131	af9b7ab2a92d7ff01f166fd22b137f2c6b5361af1865d410fb7ea89cb5662840
com.inductiveautomation.webdev/resources/cli/tags/resource.json	257	7b34b4d48c3cd1e3c341225354e1a7a185ed9302f6bfb2b15aea3e0f5374f9fb
project.json	186	4347cd485077a2b0353383033b9f2fedbbf895e76bc2ccbe449a32eae00aa6be
[exit 0]
~~~

#### POST import CAP25-C overwrite=false

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/c9-c.zip '$BASE/data/api/v1/projects/import/CAP25-C?overwrite=false'` @ 2026-10-22T14:20:56Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:20:56 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 47

{"success":true,"changes":[{"name":"CAP25-C"}]}
[curl http=200 total=0.645446s]
~~~

### E0: the child before the overwrite

#### GET projects/find/CAP25-C (status, parent, enabled, inheritable)

`bash capture25.sh findsel CAP25-C` @ 2026-10-22T14:20:57Z

~~~
status=200 {"parent":"CAP25-P","enabled":true,"inheritable":false}
[exit 0]
~~~

#### GET projects/export/CAP25-C (E0)

`curl -sS -g -D - -o '$STATE/c8/c9-e0.zip' -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/export/CAP25-C'` @ 2026-10-22T14:20:57Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:20:57 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/zip;charset=utf-8
Content-Disposition: attachment; filename="CAP25-C_20261022142057.zip"
Content-Length: 19911

size_bytes=19911
sha256=830d39f37fe4fa9f155e56c1fae6acf45829c29300c9c5b06b02c33423c1523b
[curl http=200 total=0.026392s]
~~~

#### members of the child export E0 (names, sizes, sha256)

`bash capture25.sh members $STATE/c8/c9-e0.zip` @ 2026-10-22T14:20:57Z

~~~
member	bytes	sha256
com.inductiveautomation.perspective/views/CapChild/resource.json	134	34d26a561d939ba8d6703c799fbf41090644927b49986b8358ee66011b2d829e
com.inductiveautomation.perspective/views/CapChild/view.json	87	c8714afb7eac457a10e2a22c4d0f3d76fd635e6457eb7744d8eff21e8dd88385
com.inductiveautomation.webdev/resources/cli/alarms/config.json	179	8ba70412f7c537635d0ee2e59eefd5e7a79dbfc5793dd1ad12d7e04a8b6584f0
com.inductiveautomation.webdev/resources/cli/alarms/doPost.py	5685	785268174533176bd27897eb766bc6a4cb7895922a6b4c5ff1456bbe744b2ecd
com.inductiveautomation.webdev/resources/cli/alarms/resource.json	257	7b34b4d48c3cd1e3c341225354e1a7a185ed9302f6bfb2b15aea3e0f5374f9fb
com.inductiveautomation.webdev/resources/cli/scriptExec/config.json	179	8ba70412f7c537635d0ee2e59eefd5e7a79dbfc5793dd1ad12d7e04a8b6584f0
com.inductiveautomation.webdev/resources/cli/scriptExec/doPost.py	7517	83d0f99f2e10185c82d9644f37c8ff32aa90907dfc8f4625c3e4493bd9e2ac96
com.inductiveautomation.webdev/resources/cli/scriptExec/resource.json	257	7b34b4d48c3cd1e3c341225354e1a7a185ed9302f6bfb2b15aea3e0f5374f9fb
com.inductiveautomation.webdev/resources/cli/tagConfig/config.json	179	8ba70412f7c537635d0ee2e59eefd5e7a79dbfc5793dd1ad12d7e04a8b6584f0
com.inductiveautomation.webdev/resources/cli/tagConfig/doPost.py	13189	a08cf41500b47406096b3e8a3809b9db8b57c940e11dbc50f68887beaa5db722
com.inductiveautomation.webdev/resources/cli/tagConfig/resource.json	257	7b34b4d48c3cd1e3c341225354e1a7a185ed9302f6bfb2b15aea3e0f5374f9fb
com.inductiveautomation.webdev/resources/cli/tagHistory/config.json	179	8ba70412f7c537635d0ee2e59eefd5e7a79dbfc5793dd1ad12d7e04a8b6584f0
com.inductiveautomation.webdev/resources/cli/tagHistory/doPost.py	3919	52623d1fa8143ef2a0353141641d881fdf60f3c4860037aa5aa5b4ece76fa964
com.inductiveautomation.webdev/resources/cli/tagHistory/resource.json	257	7b34b4d48c3cd1e3c341225354e1a7a185ed9302f6bfb2b15aea3e0f5374f9fb
com.inductiveautomation.webdev/resources/cli/tags/config.json	179	8ba70412f7c537635d0ee2e59eefd5e7a79dbfc5793dd1ad12d7e04a8b6584f0
com.inductiveautomation.webdev/resources/cli/tags/doPost.py	4131	af9b7ab2a92d7ff01f166fd22b137f2c6b5361af1865d410fb7ea89cb5662840
com.inductiveautomation.webdev/resources/cli/tags/resource.json	257	7b34b4d48c3cd1e3c341225354e1a7a185ed9302f6bfb2b15aea3e0f5374f9fb
project.json	182	dfec7f7f05cfaf0d71257a2d3f963e4c2584d1f073d04230eadeee13ecc7aaee
[exit 0]
~~~

#### ign webdev status --project CAP25-C (is scriptExec answering after the import?)

`bash -c for\ i\ in\ 1\ 2\ 3\ 4\ 5\ 6\;\ do\ out=\$\(\"\$0\"\ webdev\ status\ --project\ CAP25-C\ --json\ 2\>\&1\)\;\ if\ echo\ \"\$out\"\ \|\ jq\ -e\ \".data.ok\ //\ false\"\ \>/dev/null\ 2\>\&1\;\ then\ echo\ \"\$out\"\ \|\ jq\ -c\ \"\[.data.routes\[\]\ \|\ \{route\,\ status\}\]\"\;\ exit\ 0\;\ fi\;\ sleep\ 5\;\ done\;\ echo\ \"\$out\"\ \|\ jq\ -c\ \"\(.data.routes\ //\ .error\)\ \|\ tostring\ \|\ .\[0:300\]\" ign` @ 2026-10-22T14:20:57Z

~~~
[{"route":"tags","status":"present"},{"route":"tagConfig","status":"present"},{"route":"alarms","status":"present"},{"route":"tagHistory","status":"present"},{"route":"scriptExec","status":"present"}]
[exit 0]
~~~

#### E0: import cap25lib; print cap25lib.answer() in the child scope, before the overwrite

`bash -c c=\$\(\"\$0\"\ script\ run\ --project\ CAP25-C\ --code\ \"import\ cap25lib\;\ print\ cap25lib.answer\(\)\"\ --json\ 2\>\&1\)\;\ echo\ \"\$c\"\ \|\ jq\ -r\ \".data.stdout\ //\ .error.code\"\ \|\ head\ -3 ign` @ 2026-10-22T14:21:03Z

~~~
1

[exit 0]
~~~

#### E0 digest of the child export

`bash capture25.sh c9digest $STATE/c8/c9-e0.zip` @ 2026-10-22T14:21:03Z

~~~
members=18 digest=939fa6ad14fd0167
[exit 0]
~~~

### overwrite of the parent and 120 s of polling

#### POST import CAP25-P overwrite=true (answer() returns 2) — T0

`curl -sS -g -i -X POST [token $IGN_TOKEN_CAP_A] -H Content-Type: application/zip --data-binary @$STATE/c8/c9-p2.zip '$BASE/data/api/v1/projects/import/CAP25-P?overwrite=true'` @ 2026-10-22T14:21:04Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:21:04 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 47

{"success":true,"changes":[{"name":"CAP25-P"}]}
[curl http=200 total=0.292741s]
~~~

#### poll CAP25-C every 5 s for 120 s from T0 (25 lines)

`bash capture25.sh c9poll CAP25-C 1792678864` @ 2026-10-22T14:21:04Z

~~~
t=0s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=1  
t=5s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=10s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=15s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=20s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=25s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=30s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=35s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=40s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=45s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=50s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=55s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=60s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=65s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=70s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=75s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=80s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=85s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=90s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=95s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=100s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=105s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=110s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=115s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
t=120s took=0s find=status=200,{"parent":"CAP25-P","enabled":true,"inheritable":false} export=members=18 digest=939fa6ad14fd0167 call=2  
[exit 0]
~~~

#### gateway log lines naming CAP25- since 2026-10-22T14:21:04Z, cut to 300 characters

`bash -c docker\ compose\ -p\ ign-deploy-rig\ logs\ --no-log-prefix\ --since\ \"\$0\"\ 2\>\&1\ \|\ grep\ \"CAP25-\"\ \|\ cut\ -c1-300 2026-10-22T14:21:04Z` @ 2026-10-22T14:23:04Z

~~~
jvm 1    | 2026/10/22 14:21:03 | I [ign-cli-scriptexec            ] [14:21:03.803]: exec sha256=3d6266c7d4cf elapsedMs=19 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:21:04 | I [Project                       ] [14:21:04.328]: Restarting gateway scripts... project=CAP25-C, collection=CAP25-P
jvm 1    | 2026/10/22 14:21:04 | I [ign-cli-scriptexec            ] [14:21:04.522]: exec sha256=3d6266c7d4cf elapsedMs=3 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:21:05 | I [Project                       ] [14:21:05.023]: Restarting gateway scripts... project=CAP25-P, collection=CAP25-P
jvm 1    | 2026/10/22 14:21:09 | I [ign-cli-scriptexec            ] [14:21:09.770]: exec sha256=3d6266c7d4cf elapsedMs=13 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:21:14 | I [ign-cli-scriptexec            ] [14:21:14.307]: exec sha256=3d6266c7d4cf elapsedMs=10 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:21:19 | I [ign-cli-scriptexec            ] [14:21:19.249]: exec sha256=3d6266c7d4cf elapsedMs=4 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:21:24 | I [ign-cli-scriptexec            ] [14:21:24.614]: exec sha256=3d6266c7d4cf elapsedMs=6 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:21:29 | I [ign-cli-scriptexec            ] [14:21:29.834]: exec sha256=3d6266c7d4cf elapsedMs=9 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:21:34 | I [ign-cli-scriptexec            ] [14:21:34.294]: exec sha256=3d6266c7d4cf elapsedMs=19 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:21:39 | I [ign-cli-scriptexec            ] [14:21:39.713]: exec sha256=3d6266c7d4cf elapsedMs=19 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:21:44 | I [ign-cli-scriptexec            ] [14:21:44.600]: exec sha256=3d6266c7d4cf elapsedMs=16 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:21:49 | I [ign-cli-scriptexec            ] [14:21:49.538]: exec sha256=3d6266c7d4cf elapsedMs=8 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:21:54 | I [ign-cli-scriptexec            ] [14:21:54.414]: exec sha256=3d6266c7d4cf elapsedMs=6 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:21:59 | I [ign-cli-scriptexec            ] [14:21:59.324]: exec sha256=3d6266c7d4cf elapsedMs=18 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:22:04 | I [ign-cli-scriptexec            ] [14:22:04.201]: exec sha256=3d6266c7d4cf elapsedMs=3 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:22:09 | I [ign-cli-scriptexec            ] [14:22:09.485]: exec sha256=3d6266c7d4cf elapsedMs=2 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:22:14 | I [ign-cli-scriptexec            ] [14:22:14.315]: exec sha256=3d6266c7d4cf elapsedMs=8 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:22:19 | I [ign-cli-scriptexec            ] [14:22:19.157]: exec sha256=3d6266c7d4cf elapsedMs=3 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:22:24 | I [ign-cli-scriptexec            ] [14:22:24.540]: exec sha256=3d6266c7d4cf elapsedMs=9 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:22:29 | I [ign-cli-scriptexec            ] [14:22:29.388]: exec sha256=3d6266c7d4cf elapsedMs=18 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:22:34 | I [ign-cli-scriptexec            ] [14:22:34.211]: exec sha256=3d6266c7d4cf elapsedMs=3 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:22:39 | I [ign-cli-scriptexec            ] [14:22:39.563]: exec sha256=3d6266c7d4cf elapsedMs=13 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:22:44 | I [ign-cli-scriptexec            ] [14:22:44.485]: exec sha256=3d6266c7d4cf elapsedMs=11 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:22:49 | I [ign-cli-scriptexec            ] [14:22:49.347]: exec sha256=3d6266c7d4cf elapsedMs=5 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:22:54 | I [ign-cli-scriptexec            ] [14:22:54.241]: exec sha256=3d6266c7d4cf elapsedMs=19 project=CAP25-C, method=POST, route-path=cli/scriptExec
jvm 1    | 2026/10/22 14:22:59 | I [ign-cli-scriptexec            ] [14:22:59.292]: exec sha256=3d6266c7d4cf elapsedMs=3 project=CAP25-C, method=POST, route-path=cli/scriptExec
[exit 0]
~~~

### readings

#### derived readings (awk over the poll lines above)

`printf %s\\n $'polls=25\nchild_export_digest_changed=no\nchild_reads_parent_new_value=yes (first at t=5s)\nsettle_seconds=5 s'` @ 2026-10-22T14:23:04Z

~~~
polls=25
child_export_digest_changed=no
child_reads_parent_new_value=yes (first at t=5s)
settle_seconds=5 s
[exit 0]
~~~

Script reading: `child_export_digest_changed` compares each poll digest with the E0 digest; `child_reads_parent_new_value` is the first poll whose call result is `2`; `settle_seconds` is the elapsed time of the last poll whose (find, export, call) fields differ from the poll before it.

### cleanup

#### DELETE project CAP25-C

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-C?confirm=true'` @ 2026-10-22T14:23:05Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:23:05 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.020389s]
~~~

#### DELETE project CAP25-P

`curl -sS -g -i -X DELETE [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/CAP25-P?confirm=true'` @ 2026-10-22T14:23:05Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:23:05 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 29

{"success":true,"changes":[]}
[curl http=200 total=0.009277s]
~~~

#### GET projects/list (limit=-1), end of block

`curl -sS -g -i -X GET [token $IGN_TOKEN_CAP_A] '$BASE/data/api/v1/projects/list?limit=-1'` @ 2026-10-22T14:23:05Z

~~~
HTTP/1.1 200 OK
Date: Thu, 22 Oct 2026 14:23:05 GMT
Referrer-Policy: strict-origin-when-cross-origin
X-Content-Type-Options: nosniff
X-Frame-Options: SAMEORIGIN
X-XSS-Protection: 1; mode=block
Content-Type: application/json
Content-Length: 70

{"items":[],"metadata":{"total":0,"matching":0,"limit":-1,"offset":0}}
[curl http=200 total=0.009760s]
~~~

Script reading: 0 CAP25-* project(s) listed after this block; the rig list holds 0 project(s) in all.

### C9: reading

Script reading of the C9 block (the 8.3.6 rig, production mode, one parent and one child, 25 polls at 5 s):

1. **The child project is untouched.** The child export has the same 18 members and the same digest at E0 and at every one of the 25 polls (`child_export_digest_changed=no`); `find` answers `parent: CAP25-P, enabled: true` throughout. A parent overwrite does not change a child export, so D-06 can only report child drift against the digest taken before the import, and on this evidence it reports none.

2. **The child reads the parent's new value within the first 5 s.** `import cap25lib; print cap25lib.answer()` in the child scope returned `1` at E0 and at t=0 s and `2` at t=5 s and every later poll (`child_reads_parent_new_value=yes, first at t=5s`; `settle_seconds=5`, the resolution of the poll). The inheritance is real on this rig: the library is stored only in the parent.

3. **The log names the child twice and no restart line.** The gateway log for the 120 s holds, besides the scriptExec audit lines of the polls, exactly two project lines: `Restarting gateway scripts... project=CAP25-C, collection=CAP25-P` about 0.2 s after the import returned, then `Restarting gateway scripts... project=CAP25-P, collection=CAP25-P` about 0.7 s later. There is no `Starting project:` line (the S5 spike saw `Starting project: SPIKE_CHILD ... collection=SPIKE_PARENT`, a different line shape) and no `Project started` line. The poll at t=0 s fell between the two lines and still returned the old value `1`: the new value is served from about 1 s after the overwrite call.

4. **Input to 25-03 K7 (D-06).** The settle wait can be small: the observed settle is at most 5 s with a 5 s poll, and the gateway lines that mark it carry the child's name as `project=` and the parent's as `collection=`. A 30 s ceiling (six times the observation) leaves ample margin and a longer one buys nothing on this rig. What this capture does NOT show, and what stays OPEN (S5 residual): whether a session or a Perspective page open on the child is disturbed. `ign script run` also needs `import` for a project library: the scriptExec namespace holds no project libraries until they are imported.

Capture note: an earlier run of this block called `cap25lib.answer()` without the `import` and every call answered `NameError`; that run was discarded (the file was restored) and this block is the rerun. The first-touch lazy compile of a freshly imported scriptExec route answered one 500 on `ign webdev status` in a manual trial before the block; the block itself needed no retry.
