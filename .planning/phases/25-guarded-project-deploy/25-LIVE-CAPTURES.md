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
