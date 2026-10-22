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
