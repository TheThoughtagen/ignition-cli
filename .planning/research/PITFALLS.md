# Pitfalls Research — ignition-cli v1.6 Declared Gateway Deploy

**Project:** ignition-cli — adding `ign deploy {render,plan,apply,rollback,verify,projects}`
**Milestone:** v1.6 (subsequent milestone; phases 20 client / 21 render / 22 plan / 23 apply-rollback-verify / 24 projects)
**Target:** production Ignition 8.3.6 redundant pair on Windows Server (master WIGNVM01 10.4.8.15, backup WIGNVM04 10.4.8.29), deployed from the Windows self-hosted runner WFLWVM01 over the VPN, into the `production` deployment-mode collection.
**Researched:** 2026-10-10
**Overall confidence:** MEDIUM-HIGH. Pitfalls built on the spike facts are HIGH (cited to the result file). Windows transport pitfalls are MEDIUM (well-known platform behaviour, NOT yet observed on WIGNVM01/04 — each carries a capture task). Items tagged **OPEN** are unverified and must be captured on a rig before code depends on them.

## How to read the evidence tags

| Tag | Meaning |
|-----|---------|
| **[SPIKE Sx]** | Proven on 8.3.6 in `whk-environment-orchestration/spikes/gateway-modes/results/Sx.md` (or `RIG.md` / `SUMMARY.md`). Do not re-guess. |
| **[SPEC §n]** | Stated in `docs/superpowers/specs/2026-10-09-prod-gateway-config-as-code-design.md`. |
| **[REPO]** | Observed in a real file in this repo or in `gw-config/` (the dev core snapshot), checked 2026-10-10. |
| **[MEM name]** | From the user's verified project memory (named note). |
| **[DOC url]** | Public IA documentation or IA-staff forum statement. |
| **[INFERRED]** | Reasoned from platform behaviour. Treat as a hypothesis; the pitfall's "capture" step proves or kills it. |

Path shorthand: `results/` = `/Users/pmannion/data/worktrees/whk-environment-orchestration/gateway-config-spec/spikes/gateway-modes/results/`.

Note on `.planning/WINDOWS.md`: despite the name it is the Broken Windows *defect ledger*, not platform notes. Two of its entries bind this milestone directly (Pitfalls 31 and 32).

---

## Critical Pitfalls

These cause an outage, a silent wrong-state production gateway, or a rewrite.

### Pitfall 1: Writing into a collection that is not the active one (mode not live on a node)

**What goes wrong:** `apply` writes `data/config/resources/production/` on the master, scans, and the read-back "passes" because it read `find ...?collection=production`, which returns exactly what was written. But the node never activated the mode, so effective config is still `core`. The backup is the likelier victim: it boots, sees no such mode locally, logs a warning, and never re-evaluates. At the next failover prod silently reverts to `core` config.

**Why it happens:** The JVM arg is ignored when the mode does not exist at boot, with only a log line ("Config mode set to "production", but no such mode exists. Ignoring.") [SPIKE S3, RIG.md]. A pair with differing modes only produces a backup-side warning ("Active deployment mode differs from peer node!") [RIG.md]. API reads that name a `collection` read the file, not the live view [SPIKE S1].

**How to avoid:**
- Pre-check, before any lock or write: `gateway-info.deploymentMode == target.mode` on BOTH nodes (read each node's API directly). Refuse with a dedicated slug (`deploy_mode_inactive`). An empty string `""` means no mode active.
- `verify` compares the EFFECTIVE view (`find` with no `collection`) against the render, never `?collection=production` alone. Reading `?collection=` is only for the shadowing diagnostic (Pitfall 2).
- Never auto-fix by creating/activating modes inside `apply` (cutover is a restart-class, ordered operation, SPEC §13 step 4).

**Warning signs:** Verify green but `deploymentMode` is `""` on either node; backup log contains "no such mode exists"; effective value equals the `core` value after apply.

**Phase to address:** 20 (client exposes per-node `deploymentMode`), 23 (pre-check + effective-view verify). Test: rig pair with the backup booted before the mode existed (the S3 wrong-order run); apply must refuse.

---

### Pitfall 2: Core-shadow blindness — declared deletes and core-only folders

**What goes wrong:** A resource or tag deleted from the declared tree is deleted from the mode collection, but a copy still in `core` shows through, so the "delete" is a no-op and verify (if it reads only the collection) passes. Worse for tags: a core-only tag inside a folder the mode also defines is hidden, but a core-only FOLDER shows through, so behaviour depends on tree shape.

**Why it happens:** Tags override at folder granularity [SPIKE S2: S2 core-only tag NO, S2b core-only folder YES]. Mode wins over core for config [SPIKE S1]. Commissioning creates `core` resources that shadow `external` [SPIKE RIG.md "core overrides external"].

**How to avoid:**
- `plan` computes, for every declared type, the set of names present in `core` (`find ?collection=core`) and flags (a) declared-and-still-in-core (shadow, benign but strip-pending), (b) removed-from-declared-but-still-in-core (the delete will not take effect: BLOCKING unless `--allow-shadowed`), (c) core-only folders under declared tag providers.
- Mode resources replace the core resource WHOLESALE (whole-resource override, [SPIKE S1]). A declared singleton that is a partial copy silently drops every core field it omits. Render must emit the complete effective resource.
- `ign deploy` never writes `core` in v1.6. The strip is a separate, snapshotted cutover step (SPEC §13 step 5); `plan` only reports it.

**Warning signs:** After "delete" the resource still appears in effective view; `tags/export` shows a folder that is absent from the declared tree.

**Phase to address:** 22 (shadow report is a plan feature), 21 (render emits complete resources). Test: rig with a folder present only in `core`.

---

### Pitfall 3: Lock expiry mid-write leaves a torn tree that the NEXT scan applies

**What goes wrong:** The deploy detects `GET /scan-lock/config` → 204 and correctly refuses to scan. But the half-written tree is still on disk. Any later scan (a second deploy, a Designer/UI config save, a project scan, gateway-internal triggers) applies the torn state. "Failed safely" was false.

**Why it happens:** Expiry applies nothing and gives no signal on a later `POST /scan` [SPIKE S4, SPEC §6.4.3]. The lock is the only thing standing between disk and the gateway, and it has a timer.

**How to avoid:**
- Abort path = restore, not just exit. On expiry/transport error, immediately re-acquire a lock and restore the pre-write on-box snapshot (Pitfall 21), then scan, then verify. Only then report `failed`.
- TOCTOU: checking "lock still held" and then calling scan is racy. Use the gateway-reported `remaining` from `GET /scan-lock/config` (it has a `remaining` field [SPIKE S4]) and require `remaining >= safety_margin` before issuing the scan; do not compute remaining from the runner's clock.
- Check the lock immediately before the scan AND treat "scan POST returned 200" as only "requested" (Pitfall 6).
- Keep the write window tiny (Pitfall 18: stage outside the lock, mirror inside it).

**Warning signs:** Verify disagrees with the render after a run that reported a lock problem; `plan` right after a failed apply shows changes already half-applied.

**Phase to address:** 23. Fault-injection test: acquire with `holdTimeout=2`, `sleep` mid-write, assert no scan was issued AND the collection equals the pre-write snapshot AND exit code is the distinct failure slug.

---

### Pitfall 4: `holdTimeout` sized from spike numbers

**What goes wrong:** `holdTimeout = 2*(write+scan)+1` evaluated with the spike values gives 5 s [SPIKE S4]. Real prod: file transfer over the VPN/SFTP plus Defender scanning plus a larger tree, and the scan itself is slower on the real gateway. The lock expires on the first real deploy (Pitfall 3).

**Why it happens:** The spike numbers are `docker cp` on localhost against ~1.3k files; SUMMARY.md and SPEC §6.4.3 both say "measured on the TARGET transport". Project scans measured 7.5 s on prod and 14.6 s on dev [MEM ignition-83-platform-scan-api], already above 5 s; config scan on a larger tree is unmeasured.

**How to avoid:**
- Never hard-code. `holdTimeout` comes from the target config with enforced floor/ceiling, and `plan` prints the computed value and its inputs. Measure p95 of (mirror + scan) on the `test`/rig target with a prod-sized tree and derive.
- Ceiling matters too: a crashed deploy holds the lock for the full hold and blocks gateway config application (409 for others). Keep it as tight as the measurement allows.
- Poll cadence for `GET /scan/config` ~500 ms; per-phase deadlines (acquire, write, scan-complete) separately configured.
- `acquireTimeout` handling: 409 = someone else holds it ("Lock already held by <actor>"), report the actor, never wait indefinitely [MEM ignition-83-platform-scan-api].

**Warning signs:** `lock_expired` on the first production run; scan duration near the hold.

**Phase to address:** 23 (value derivation), 20 (client surfaces `remaining`, `actor`, 409).

---

### Pitfall 5: Mirror-with-deletes erases gateway-owned files in the mode collection

**What goes wrong:** "Mirror the rendered tree into `data/config/resources/production` over SSH (deletes included)" literally wipes (a) `production/config-mode.json`, the file that DEFINES the mode, and (b) every resource of an undeclared type that the gateway or a UI/API edit put there. After `config-mode.json` disappears the mode is gone from the master and syncs to the backup; the next restart ignores the JVM arg (Pitfall 1).

**Why it happens:** `POST /mode` writes `resources/<mode>/config-mode.json` [SPIKE S1: shown in S1.md]. API/UI edits with no `collection` land in the active mode collection [SPIKE S1c], so the collection holds gateway-created content of undeclared types (e.g. a UI-made API token or user source). A naive `robocopy /MIR` or `rm -r` treats all of it as stale.

**How to avoid:**
- The delete set = (files under declared types/names in the previous applied manifest) minus (rendered set). Nothing else is ever deleted. Types outside the manifest allowlist are untouchable (SPEC §4: "Anything not listed stays owned by the gateway").
- `config-mode.json` is a protected path: either render emits it byte-identical to the gateway's, or the mirror excludes it. Add a hard guard that aborts if the post-mirror tree lacks it.
- `plan` lists every file it would delete, grouped as declared-removal vs unmanaged; unmanaged-in-collection is reported, not deleted.

**Warning signs:** `production/config-mode.json` missing; resources vanish that nobody declared; backup `GET /mode` lacks `production`.

**Phase to address:** 22 (plan names exact delete set), 23 (mirror implementation + guard). Test: seed an undeclared resource + `config-mode.json`, apply, assert both survive.

---

### Pitfall 6: Believing a 200 from `POST /scan/config`

**What goes wrong:** Scan returns 200 immediately with the OLD `lastScanTimestamp` and `scanActive:true`. A deploy that proceeds on 200, or that polls only `scanActive == false` (which may be true before the scan even starts), declares victory against unapplied files. Separately, scan "completing" does not mean resources loaded: an invalid resource (missing `attributes.uuid`, bad reference) is dropped with only a WARN in the gateway log.

**Why it happens:** Async scan [MEM ignition-83-platform-scan-api: green curl = "scan requested"]. Unloadable resources log "does not have required UUID attribute, will not be loaded" and carry on [SPIKE RIG.md].

**How to avoid:**
- Completion predicate: `scanActive == false && lastScanTimestamp > before`, where `before` is read from the same gateway just before the POST. Never compare gateway timestamps to the runner clock (Pitfall 24).
- Read-back every declared resource from the effective view and diff semantically; absence is a failure, not a skip.
- Scrape the gateway log for WARN/ERROR since the scan began (`will not be loaded`, `UUID`, `NamedResourceHandler`), using `ign logs`, and fail verify on any hit naming a declared resource.
- Use `allowInvalidReferences=false` on read-back (SPEC §6.1).

**Warning signs:** Scan duration ~0; `lastScanTimestamp` unchanged; read-back count lower than rendered count.

**Phase to address:** 20 (typed scan-status), 23 (predicate + log scrape + read-back).

---

### Pitfall 7: Writing a new resource without a uuid, or generating an unstable one

**What goes wrong:** (a) No `attributes.uuid` on a NAMED resource: the resource is silently ignored. (b) A random uuid on every render makes each render differ, so `plan` is never empty and rename tracking breaks. (c) Adding a uuid to a singleton that has none is a shape change.

**Why it happens:** Missing uuid → "will not be loaded" [SPIKE RIG.md, SPEC §3]. IA: a resource that tracks renames and lacks a UUID "is just ignored and treated as if it doesn't exist", and no attempt is made to add one [DOC forum.inductiveautomation.com/t/deployment-modes-and-resource-definition-uuids/114966]. Singletons (`system-properties`, `security-properties`, `security-levels`) have none by default (same thread). In this repo's `gw-config/` snapshot 30 of 91 `resource.json` have no `uuid`, so "always add one" is wrong [REPO gw-config resource.json census, 2026-10-10: 61 carry uuid, 30 have none (16 with only lastModification+signature+enabled, 12 with only lastModification+signature, 2 near-empty)]. Per IA, each resource in each collection should carry a unique uuid, so copying a uuid from core into the mode copy is discouraged (same thread).

**How to avoid:**
- Per-type rule table (named vs singleton), captured from a real bootstrap, drives render. Bootstrapped resources keep the gateway's own uuid (SPIKE S1/SPEC §3).
- New uuids are deterministic: UUIDv5 over (gateway, type, name) stored once in the declared tree, never `now()`/random at render time. A resource that exists in core with uuid U gets a distinct, stable uuid in the mode copy.
- Lint: named type without uuid fails render.

**Warning signs:** Read-back count short by exactly the new resources; `plan` non-empty immediately after apply with only uuid diffs.

**Phase to address:** 21 (rule table, deterministic uuid), 23 (log-scrape catch). Capture first: list which resource types in the 8.3.6 prod bootstrap carry uuid.

---

### Pitfall 8: Self-lockout by shipping the deploy token's authorization away

**What goes wrong:** The declared tree includes a resource that defines security levels / security-properties. Applied wholesale into the mode collection, it overrides core's version, which is where the `Authenticated/Deploy` level mapping for `ci-deploy` lives. The very next call (scan, verify, rollback) is 403 and the gateway is stuck half-deployed with no API path to scan or roll back.

**Why it happens:** Config-defined API tokens cannot hold system `Roles` levels ("Security level 'Authenticated/Roles' cannot be granted via config and will be ignored") so a custom level is required [SPIKE RIG.md]. In the spike, API calls stayed 403 until the level was written into `core/ignition/security-properties`, and core overrides external [SPIKE RIG.md]. Mode resources override core WHOLE-resource (Pitfall 2). IA notes 8.3.0-8.3.5 could overwrite a custom mode's `security-properties/config.json` at first boot with a forced password reset, fixed in 8.3.6 [DOC forum thread above].

**How to avoid:**
- Treat `security-properties`, `security-levels`, api-token and user-source types as DENY in the allowlist for v1.6 (SPEC §1 non-goal: API tokens and user stores); security zones are declarable but never the singletons above.
- Pre-write lint: if the render touches any security singleton, assert the deploy token's custom level survives, else refuse.
- Acquire the scan-lock FIRST (a write-permission probe): a 403 here fails before any file is touched. Existing `classify` maps 403 to an auth class; make sure the hint says "token lacks the custom Deploy level in gateway write permissions".
- Rollback must not depend on anything the apply could have broken: SSH restore + scan uses the same token, so keep the token path out of the render entirely.

**Warning signs:** 403 right after a scan on a token that worked before; `security-properties` in the render diff.

**Phase to address:** 21 (allowlist deny), 22 (plan flags), 23 (lock-first probe). Test: render containing a security singleton must be refused.

---

### Pitfall 9: Backup false-green: `syncPending=false` is not "backup has it"

**What goes wrong:** Verify reads master `GET /redundancy` (`peerConnected:true`, `syncPending:false`, `projectState:Good`), sees success, but the backup never converged. A backup that exhausts sync retries stops trying silently; a snapshot mismatch after diff aplication fails the sync; and a backup running the wrong mode shows only a warning.

**Why it happens:** RIG.md: `[config-resource] After applying diff, new local system snapshot ... did not match remote system snapshot`, then "Sync operation has failed 5 time(s) ... No further synchronization attempts will be made." [SPIKE RIG.md]. Mode divergence is a log warning only [SPIKE S3]. A clean, correctly ordered pair converges in ~4 s [SPIKE S3] but that says nothing about degraded pairs.

**How to avoid:**
- Verify the backup by CONTENT, not by status flags: read the effective resources directly from the backup's own API (10.4.8.29:8043) and compare to the render; read the file hash of the backup's `production` collection over SSH; compare `deploymentMode` on both nodes.
- `lastSyncTimestamp` is epoch-ms (S3.md values ~1.79e12) and must ADVANCE past a pre-apply sample from the same node; do not compare it to the runner clock.
- Timeout derived from S3's ~4 s with a large multiplier, then `POST /redundancy/gwaction/resync`, then re-check. `resync` returns 200 with an empty body, so a green resync proves nothing; re-verify content afterwards [SPIKE S3, MEM].
- Backup mismatch after a CLEAN master verify is a different failure class from master-verify failure: it must NOT trigger auto-rollback of the master (Pitfall 23). It reports `deploy_backup_lagging`.
- Do not interpret backup `uptime: -1` or `activityLevel: Cold` as unhealthy [SPIKE S3.md shows both on a healthy backup]. The client doc in `redundancy.rs` says role strings are `Primary`/`Backup`; real 8.3.6 pairs report `Master`/`Backup` [SPIKE S3]. Do not match on `Primary`, and use `activityLevel` (who is serving), not `role`, to decide which node is active; after a failover the `Master`-role node can be `Cold`.

**Warning signs:** Master API shows new value, backup API shows old; "No further synchronization attempts" in the backup log.

**Phase to address:** 23 (backup verification), 20 (client reads from either node; fix stale role vocabulary).

---

### Pitfall 10: Secret VALUES do not replicate; references do

**What goes wrong:** The declared config replicates and reads back identical on the backup (it only holds a secret NAME). But the secret value written to the internal provider on the master via the API never reached the backup. Failover then yields a DB connection / OAuth client that cannot decrypt. Verify was green.

**Why it happens:** Secrets are per-gateway state that redundancy does not replicate; `manage-secrets.sh` encrypts against the target gateway's own key; every gateway must be written independently [MEM gateway-secrets-dont-replicate-to-backup, ENT-3115]. SPEC §9 requires identical root/KEK keys on both nodes and a one-time check, but the VALUE write still has to hit both.

**How to avoid:**
- The secrets step writes to BOTH nodes' APIs; `verify` checks that every secret NAME referenced by the render exists on both nodes via `GET /secret-providers/{name}/secrets` (names only; never log values).
- Report per-node results in the structured output.
- Keep the key-identity check as a blocking pre-flight, not documentation.

**Warning signs:** "works on the master, not the backup" reports; bogus downstream errors (the ENT-3115 `AttributeError: 'unicode'` tell).

**Phase to address:** 23 (secret-name presence per node), 21 (render emits references only).

---

### Pitfall 11: Embedded secret ciphertext committed in the declared tree

**What goes wrong:** Bootstrapping from core copies `"type": "Embedded"` blobs into git. They look "encrypted, so safe" but: they are KEK-bound (undecryptable on test/ci gateways, so rehearsals break), nondeterministic (a re-encrypt changes the bytes, so perpetual diffs), and are credentials at rest in git history.

**Why it happens:** The dev snapshot already has this: 10 files in `gw-config/` contain `ciphertext`, e.g. the database-connection `password` is `{"data":{"ciphertext","encrypted_key","iv","protected","tag"},"type":"Embedded"}` with an `iat` in the protected header [REPO gw-config/ignition/database-connection/WHK01_PG_MES/config.json].

**How to avoid:**
- Render lint: ANY `"type": "Embedded"` fails, regardless of field name. Deny-by-default; additionally lint secret-typed plain-string fields per resource type (OIDC client secret, MQTT/OPC passwords, SMTP, JWTs), plus a high-entropy heuristic as backstop.
- Bootstrap converts to `Referenced` (provider `default`, name `<resource>.<field>`) (SPEC §9).
- Offer a `--bootstrap` helper in render/plan that rewrites Embedded to a reference and emits the "secret names to seed" list; never writes ciphertext.

**Warning signs:** `ciphertext` or `encrypted_key` anywhere under `gateways/`; git-secrets hits.

**Phase to address:** 21. Test: golden fixture with an Embedded password must fail with exit 2 slug and a message that names the path but NOT the value.

---

### Pitfall 12: Secrets leak through `plan`/`verify` output, envelopes, errors and artifacts

**What goes wrong:** `plan` prints a diff of `config.json` containing a secret field; the output goes to CI logs, the JSON envelope, MCP tool results, TUI scrollback, and run artifacts. GitHub log masking only knows the secrets Doppler registered.

**Why it happens:** Diff code is generic. Existing `classify` already rides a raw 422 body into the error `reason` when the gateway has no `messages` array (`gateway rejected the resource body (HTTP 422 from {url}): {body}`), and config-resource 422 bodies can echo the submitted config [REPO crates/ignition-core/src/client/classify.rs].

**How to avoid:**
- Redact by SCHEMA PATH (secret-typed fields per resource type), printing `<redacted sha256:abcd…>` so changes are still visible. Never by regex on values.
- Cap + redact any gateway body quoted in an error on deploy paths; no raw-body fallback.
- The `X-Ignition-API-Token` value must never appear in debug output; `actor` echoes (`api-token:<name>:<ip>`) are fine, the token secret is not.
- Snapshots (Pitfall 22) are treated as secrets.
- Property test: for every fixture containing a canary secret string, assert it appears in no stdout/stderr/envelope/error output across `plan`, `apply`, `verify`, `--json`, MCP.

**Warning signs:** a canary string in any captured output.

**Phase to address:** 21 (redactor), 22 (plan output), 23 (errors/verify), 20 (client error bodies).

---

### Pitfall 13: Line endings: `text=auto` on a Windows runner converts the declared tree to CRLF

**What goes wrong:** `actions/checkout` on WFLWVM01 with Git for Windows' default autocrlf, plus a repo `.gitattributes` of `* text=auto` (this repo's setting [REPO whk-environment-orchestration .gitattributes: `* text=auto`, `*.sh text eol=lf`]), checks out JSON/CSS as CRLF. Anything that byte-copies (non-JSON members) or hashes raw files now differs from what Ignition wrote (LF, no trailing newline), so file-hash verify and drift never converge.

**Why it happens:** Gateway-written files are LF with NO trailing newline [REPO: `tail -c` of gw-config `resource.json`/`config.json` ends `}` with no `\n`; zero `\r` bytes]. Platform eol translation is applied only to the checkout.

**How to avoid:**
- `gateways/** -text` (or `binary`) in `.gitattributes`, with a CI check that no CR exists in `gateways/`. The renderer must also tolerate CRLF input defensively.
- JSON is parsed and re-serialized (never copied); non-JSON members (CSS, JAR) are byte-copied.
- Hash over canonical bytes (what will land on disk), computed after render.
- Run the render golden test in the `windows-latest` CI leg (the matrix already includes it).

**Warning signs:** Verify hash mismatch only when run from the Windows runner; `git diff` clean but byte hashes differ.

**Phase to address:** 21 (+ a repo-level fix in whk-environment-orchestration).

---

### Pitfall 14: Gson-compat serialization: sorted keys is NOT a universal rule; perpetual diffs from formatting

**What goes wrong:** A renderer that blanket-sorts keys and uses a typical JSON writer produces bytes the gateway never writes, so every `plan` shows changes and every drift check fires.

**Why it happens (observed):**
- `config.json` keys are alphabetical (case-sensitive ASCII: `connectURL` before `connectionProps`), but `resource.json` is NOT sorted. Its order is POJO order: `scope, [description,] version, restricted, overridable, files, attributes`, and inside `attributes`: `lastModification, uuid, lastModificationSignature, enabled` (also `legacyId`) [REPO gw-config census, 2026-10-10: 63 resources with 6 top-level keys, 28 with `description` in 2nd position].
- Explicit `null` values are preserved (`"connectionProps": null`) [REPO]. Dropping nulls changes the file.
- 2-space indent, `": "` separator, no trailing newline [REPO].
- Gson HTML-safe escaping: `=`, `'`, `<`, `>`, `&` as `\u003d` etc [SPEC §4]. (Gson also emits U+2028/2029 escaped and other non-ASCII raw.) Number formatting (`1.0` vs `1`, exponent forms, big ints) differs between libraries.
- `serde_json` default `Map` is a sorted BTreeMap (the wrong order for resource.json) unless `preserve_order` is enabled, and `f64` round-tripping can rewrite numbers; arbitrary-precision or raw number tokens are needed.

**How to avoid:**
- Per-file-kind serializer: `config.json` = sorted/escaped; `resource.json` = fixed struct order with optional `description`.
- GOLDEN CORPUS GATE: take every JSON file from the real core snapshot (183 in `gw-config/`) plus a sample from prod `core` (tags included), parse → render → assert byte-identical. This is the proof that "files the gateway produced round-trip byte-identical" (SPEC §4). Run on all three CI OSes.
- Property: `render(parse(render(x))) == render(x)`.
- Cover hostile data: `=`/`'`/`<` in JDBC URLs and SQL, non-ASCII, emoji, U+2028, large ints, floats, empty `{}`/`[]`.
- No `HashMap`, no `read_dir` order, no `now()`, no locale in render; sort explicitly. Java string order is UTF-16 code units while Rust `str` is UTF-8 bytes (differ only for supplementary vs U+E000..U+FFFF); the corpus catches any real case.

**Warning signs:** Any non-empty `plan` immediately after `apply` (see the idempotence gate, Pitfall 26).

**Phase to address:** 21 (serializer + corpus gate), 22 (idempotence gate).

---

### Pitfall 15: Windows path semantics: drive letters, spaces, MAX_PATH, case, reserved names

**What goes wrong:** Transfers fail or land in the wrong place: the target is `C:\Program Files\Inductive Automation\Ignition\data\config\resources\production` (spaces, colon). `scp host:C:/Program Files/...` mis-parses the colon; a local `C:\...` source on the Windows runner reads as host `C`. Resource directory names contain spaces and commas (`Barry-Wehmiller Companies, Inc._7eeb…`, `Default Group`, `Microsoft SQLServer`, `WHK01 Distillery UNS`) [REPO gw-config names]. Deep tag trees can exceed MAX_PATH: the fixed prefix alone is 80 chars, the config snapshot's longest relative dir is 122 [REPO, measured], and tag folders nest deeper [INFERRED]. NTFS is case-insensitive and rejects trailing dots/spaces, `<>:"|?*`, and `CON/NUL/AUX`-style names that Ignition tag/resource names can legally contain [INFERRED].

**How to avoid:**
- Use the SFTP protocol (not scp-style `host:path` parsing and not shell-quoted commands) with `/C:/…` style absolute remote paths, or stage under a short path (e.g. `D:\deploy\…`) and mirror locally on the box. Never build remote commands by string concatenation (Pitfall 19).
- Take the gateway's OWN on-disk name encoding as truth: scan prod `core` for hostile names (WHK01 637 tag files) before assuming an encoding, and fail render on case-fold collisions or names NTFS cannot hold. (v1.1's injective path mapping in `ign workspace` solved a related problem but its mapping is not the gateway's; do not reuse it for the gateway's own layout.)
- Pre-flight: assert max rendered path length plus prefix < 248 (or confirm `LongPathsEnabled` and that every tool in the chain is long-path aware: sftp-server, robocopy, JVM).

**Warning signs:** "No such file" only for deep or space-named resources; copies that "succeed" into a wrong directory.

**Phase to address:** 21 (name/length lint), 23 (transport). Capture: longest tag path in prod; test with spaces and a >200-char path on the Windows box.

---

### Pitfall 16: Moved files keep the wrong ACL; the service cannot read what was "deployed"

**What goes wrong:** Files are uploaded to a staging dir and `move`d/`rename`d into the collection. On Windows a rename within a volume preserves the source file's ACL/owner and does NOT inherit the destination's, so the Ignition service account may be denied. Resources are then dropped or error at scan with no 4xx anywhere.

**Why it happens:** Windows ACL inheritance applies on creation/copy, not on same-volume move [INFERRED, standard NTFS behaviour; capture on WIGNVM01]. The deploy account (`svc-ignition-deploy`) is not the account the service runs as, and the two nodes already differ in account naming (`pmannion` on .15, `Administrator` on .29) [MEM prod-ign-gateways-ssh-key-auth].

**How to avoid:**
- Create/copy into the destination (robocopy default copies data/attributes/timestamps and lets new files inherit the parent ACL) instead of renaming from outside the tree.
- After a mirror, verify by the gateway's read-back (effective view) rather than `icacls`; additionally record an ACL sanity check (`icacls` on one new file) in `verify --deep`.
- Provision `svc-ignition-deploy` identically on both nodes via a documented/idempotent script; reuse the admin-key ACL knowledge (`administrators_authorized_keys` must be limited to Administrators and SYSTEM via `icacls /inheritance:r` or sshd ignores it) [MEM prod-ign-gateways-ssh-key-auth].

**Warning signs:** Resource present in files, absent in effective view, scan log "Access is denied".

**Phase to address:** 23. Capture on a Windows test box before the prod rehearsal.

---

### Pitfall 17: File locks and robocopy defaults inside the lock window

**What goes wrong:** (a) Overwriting/deleting a file the JVM or Defender holds open fails with a sharing violation: Java's classic file I/O on Windows opens without delete-share, so a loaded JDBC driver `.jar` under `ignition/database-driver/*/` (4 jars in the snapshot [REPO]) cannot be replaced while loaded [INFERRED; capture]. (b) `robocopy` retries a locked file 1,000,000 times with 30 s waits by default, so a single locked file consumes the lock window and then some. (c) `robocopy`'s exit code is a bitmask: 0-7 are success variants (1 = files copied), >=8 is failure; a wrapper that treats non-zero as failure fails every real deploy, and one that ignores it hides errors.

**How to avoid:**
- Delta-only writes: skip files whose bytes already match (idempotent apply means jars are touched only on a version change).
- Always pass explicit `/R:2 /W:1` (small, bounded by remaining lock) and `/NP /NFL` for parse-friendly logs; decode the exit bitmask in one function with a unit test of all values.
- Treat a driver-jar change as restart-class (SPEC §7) and flag it in `plan`; never attempt it as a hot replace.
- Defender: document an exclusion for the collection directory or measure its cost in the holdTimeout derivation (Pitfall 4).

**Warning signs:** Apply hangs for minutes inside the lock; sporadic "being used by another process".

**Phase to address:** 22 (restart-class flagging), 23.

---

### Pitfall 18: Doing the slow work inside the lock (per-file SFTP over a VPN)

**What goes wrong:** Hundreds to thousands of sequential SFTP round trips inside the lock blow the window; spike timing was `docker cp` [SUMMARY.md].

**How to avoid:**
- Two-phase transport: (1) BEFORE the lock, upload the rendered tree as a single archive or parallel pipelined upload to a staging dir on the same volume but OUTSIDE `data/config/resources/` (a dir beside the collections could itself be read as a collection [INFERRED]); verify staged hashes. (2) INSIDE the lock only fast local operations on the box (delta mirror from staging into the collection), then scan.
- Also take the on-box pre-write snapshot (Pitfall 21) in that locked window by a local copy, which is cheap, before the mirror.
- Prefer Windows `tar.exe`/`robocopy` over per-file protocol calls; keep binary-safe (no text-mode transfers for jar/css).

**Phase to address:** 23.

---

### Pitfall 19: Non-interactive SSH to Windows: shell, quoting, host keys, key files

**What goes wrong:**
- Windows OpenSSH default shell is `cmd.exe`; PowerShell needs `-EncodedCommand` (UTF-16LE base64) and quoting collides with names containing spaces/commas [MEM prod-ign-gateways-ssh-key-auth].
- First-connect host-key prompt hangs a CI job; "fixing" it with `StrictHostKeyChecking=no` removes the MITM protection on a path that writes production config.
- The Windows OpenSSH client refuses a private key whose ACL is not restricted to the current user.
- A key written from a GitHub secret onto a persistent runner stays on disk (WFLWVM01 is not ephemeral and is also used for local dev; its runner group is visible to every WhiskeyHouse repo [MEM onprem-windows-runner-for-vpn-deploys]).
- A half-dead VPN stalls an `ssh` for minutes, outliving `holdTimeout`.
- Today NO CD SSHes to the Windows gateways [MEM prod-ign-gateways-ssh-key-auth]; this is a brand-new trust path.

**How to avoid:**
- `BatchMode=yes`, `ConnectTimeout`, `ServerAliveInterval/CountMax`, pinned `UserKnownHostsFile` with the gateway host keys committed in `targets/`, `StrictHostKeyChecking=yes`. Key file mode set via `icacls`, deleted in an `always()` step (or use an in-process SSH implementation and keep the key in memory; this is a stack decision Phase 23 should make explicitly).
- No remote command built by string interpolation; use SFTP operations and a small fixed set of encoded, parameter-free remote scripts.
- Restrict the runner group for the deploy workflow to this repo (SPEC §5.5) and pin `runs-on: [self-hosted, Windows, onprem-deploy]`, never bare `self-hosted` (STGWECHVM01 has no VPN route and has already received a prod job) [MEM onprem-windows-runner-for-vpn-deploys].
- Workflow assumptions that have already bitten this runner: no default bash, Git `bin` not on PATH, `jq` not installed [MEM production-cd-pipeline-windows-shell-bug]. The deploy step should call only `ign`; install/pin `ign` and gate on `ign version`.

**Warning signs:** Job hangs at connect; "UNPROTECTED PRIVATE KEY FILE"; key file found in the workspace after a run.

**Phase to address:** 23 (+ runner/target setup checklist).

---

### Pitfall 20: Runner cancel or crash mid-apply (no journal, no recovery)

**What goes wrong:** GitHub cancels the job or the VM reboots after files are half-mirrored. Nothing records that a deploy was in flight; the lock times out silently (Pitfall 3); the next run starts from a torn tree that its plan misreads as drift.

**How to avoid:**
- Write a run journal OUTSIDE the collection on the box (`deploy.inprogress` with run id, phase, snapshot path, hashes) before the first write; remove only after verify. `apply`/`plan`/`rollback` refuse (or auto-offer `rollback --run <id>`) while a journal exists.
- Workflow: `concurrency:` group per environment with `cancel-in-progress: false`; no `timeout-minutes` shorter than the worst case; handle SIGINT/Ctrl-C by running the abort-restore path (Pitfall 3).
- `rollback` must be callable from a fresh process with only the run id (state read from the box + the local snapshot dir).

**Phase to address:** 23. Test: kill -9 the process after N files; next invocation detects, refuses, and `rollback` restores.

---

### Pitfall 21: Snapshot and rollback are themselves unverified transactions

**What goes wrong:** Rollback restores files, scans, and reports success without verifying; or the snapshot was taken outside the lock and a UI edit that landed in between is reverted/lost; or the snapshot lacks `config-mode.json` and the restore deletes the mode (Pitfall 5); or rollback is only runnable from the same runner that just died.

**How to avoid:**
- Snapshot = on-box local copy of the declared-type files in the collection taken INSIDE the lock, immediately before the mirror, plus a hash manifest; pulled to the runner afterwards. Rollback = same lock-mirror-scan-poll-verify procedure as apply with the snapshot as the "render", under the same expiry rules.
- Rollback verification uses the same strict `verify` plus the log scrape.
- Result vocabulary is explicit: `applied`, `failed_unchanged`, `rolled_back`, `rollback_failed` (needs human). Rolled back MUST NOT map to ok:true (Pitfall 28).
- Documented non-reversible items (SPEC §8): deleted tag runtime values (memory-tag values), project imports, secret writes, restart-class changes. `plan` marks each, `apply` requires the `destructive` acknowledgement for tag deletion.
- Auto-rollback only on MASTER effective-state verification failure. It must not fire for backup lag, transient `restart-tasks/pending` from something else, or a probe flake (Pitfall 9, 23).

**Phase to address:** 23.

---

### Pitfall 22: Snapshot artifacts are the most sensitive files you will produce

**What goes wrong:** SPEC §6.4.2 uploads `GET /backup` (.gwbk), the collection copy and project exports as run artifacts retained 30 days. A .gwbk contains the whole gateway configuration including secret material; GitHub artifacts are readable by anyone with repo read access and are logged/downloadable by tokens.

**How to avoid:**
- Decide explicitly in Phase 23: keep snapshots on the runner/on-prem share with restricted ACL, or encrypt (age/GPG to a recovery key) before upload; never upload raw .gwbk. Document retention and purge.
- The collection snapshot excludes nothing but contains Embedded ciphertext for any not-yet-converted resource; treat as secret.
- CI checks that artifact upload paths exclude `*.gwbk`.

**Phase to address:** 23 (decision) + pipeline repo.

---

### Pitfall 23: Auto-rollback misattribution and error masking

**What goes wrong:** Verification fails because of an environment condition (backup lag, a slow tag probe, unrelated pending restart) and the deploy rolls the master back, discarding a good deploy. Or the rollback's own result overwrites the original failure so the operator never sees WHY.

**How to avoid:**
- Classify verify failures: `master_effective_mismatch` (rollback), `backup_lagging` (resync/alert, keep), `probe_flake` (retry within budget), `unloaded_resource` (rollback).
- The structured output carries BOTH the original verify failure and the rollback outcome in `data`; exit code reflects the worst.
- The `restart-tasks/pending` pre-check must be empty AND the post-check compared to the pre-deploy set so only NEW tasks are attributed to this deploy.

**Phase to address:** 23.

---

### Pitfall 24: Clock and timestamp confusion

**What goes wrong:** Comparing gateway timestamps to the runner's clock (skew, DST, VM drift); treating the lock `timestamp` as epoch; interpreting `lastSyncTimestamp` in the wrong unit.

**Evidence:** The lock info `timestamp` is `149795344` [SPIKE S4], clearly not epoch-ms (`lastScanTimestamp` is `1791584841451`, `lastSyncTimestamp` is ~`1791586091827` [SPIKE S4, S3]). The client's existing `redundancy.rs` documents `lastSyncTimestamp` unit as "not capture-proven"; S3 now proves epoch-ms.

**How to avoid:** Only compare a gateway timestamp to an earlier sample from the SAME gateway; use the gateway-supplied `remaining` for lock budgeting; update the redundancy wire doc/tests to the S3 evidence; keep the runner's monotonic clock (`Instant`) for local deadlines, never wall-clock.

**Phase to address:** 20.

---

### Pitfall 25: Concurrent deploys, and the plan-to-apply gap

**What goes wrong:** Two actors (a PR deploy, a laptop `ign`, the drift job, a Designer user) overlap. The scan-lock 409 only protects the lock window, and it expires. Also: `plan` ran hours ago (before an approval gate), the gateway changed since, and `apply` blindly enforces a stale plan, clobbering an emergency edit (SPEC §14 risk).

**Why it happens:** UI/API edits land in the ACTIVE mode collection, which is the deploy target [SPIKE S1c]. Whether the scan-lock blocks UI/Designer writes is unknown (OPEN).

**How to avoid:**
- Bind apply to a plan: `apply --plan <hash>`; `apply` re-reads the remote state AFTER acquiring the lock and aborts (`deploy_plan_stale`, NOT `--yes`-able, matching v1.1's staleness-gate convention) if the remote collection hash differs from the plan's base hash.
- Local + remote mutual exclusion beyond the lock: the on-box journal (Pitfall 20) rejects a second `ign` regardless of lock expiry. GitHub `concurrency` group on the prod environment with `cancel-in-progress: false`.
- Pre-checks from SPEC §6.4.1 (master is the active node via `activityLevel`, no open Designers via `GET /designers`, no open drift) are re-run at lock time.
- Wrong-target guard: assert gateway-info identity (system name / expected hostname) equals the target's declared identity so a mis-pointed profile cannot deploy to dev or to the backup.

**Phase to address:** 22 (plan hash), 23 (re-check under lock, journal).

---

### Pitfall 26: Perpetual diffs: signature churn, lastModification, gateway-added defaults

**What goes wrong:** `plan` is never empty; drift fires forever; operators learn to ignore it.

**Why it happens:**
- `resource.json` carries `lastModification{actor,timestamp}` and `lastModificationSignature`. IA staff advise tracking `resource.json` "even though lastModificationSignature causes churn" [DOC forum 114966]. The signature algorithm is not published; the SDK `LastModification.verify` checks that the signature is valid for the resource contents [DOC sdk.inductiveautomation.com LastModification javadoc], so it cannot be computed client-side. A forum statement says 8.3 leaves a file alone when the signature is invalid rather than rewriting it [DOC forum "Git Version Control Resource.Json Files"; unverified for 8.3.6].
- A PUT-mutated resource gets a new signature [SPIKE S1: `newSignature`]; import/UI paths stamp `actor`/`timestamp`.
- The gateway may normalize/add default fields to `config.json` on load [INFERRED].
- A rendered `lastModification.timestamp` of `now` would churn every render.

**How to avoid:**
- OPEN capture on the rig before Phase 21 freezes the format: write a `config.json` change with (a) stale signature, (b) signature omitted, scan, then read back `find` and re-read the file. Does the gateway rewrite the file? Does the effective view show the change? Is the resource flagged?
- Semantic comparison ignores `lastModification` and `lastModificationSignature` (and `enabled` only where the type owns it) in `plan`/`verify`/drift; byte-hash comparison covers `config.json` plus stable attributes.
- Render is pure: a fixed `lastModification` actor/timestamp policy (e.g. carry the bootstrap value; never `now`).
- IDEMPOTENCE GATE (a test, run on every PR): `bootstrap → plan` empty (SPEC §13 step 2), and `apply → plan` empty, `apply → apply` writes zero files and issues no scan.

**Phase to address:** 20 (capture), 21 (render policy), 22 (compare rules + gate).

---

### Pitfall 27: Tolerant client that tolerates too much (2xx with empty/non-JSON bodies)

**What goes wrong:** To handle `204`/empty/HTML-ish 2xx bodies, the new client becomes "any 2xx is success". Then the failover that returns 200 while unable to reach the backup, `resync` 200-empty, and WebDev rejections riding HTTP 200 all read as success.

**Why it happens:** `PUT /redundancy/config` and `GET /scan-lock` return empty/non-JSON; failover returns 200 even when it fails ("Could not dispatch node responsibility request to backup") [SPIKE RIG.md, S3, MEM ignition-83-mode-and-config-api-facts]. `ign api call` ERRORS on those (so the new typed client cannot be built on the api-call passthrough). WebDev ignores the `status` key, returning 200 with the rejection in the body [MEM webdev-ignores-status-key]. The current client defaults to a 30 s total timeout and JSON bodies [REPO crates/ignition-core/src/client/mod.rs `build_client`].

**How to avoid:**
- Typed per-endpoint response shapes: `Json<T>`, `EmptyOk`, `NoContent`, `JsonOrEmpty<T>` with a documented success predicate per endpoint (lock-held vs lock-expired by 200 JSON vs 204 [SPIKE S4]).
- Success for state-changing calls is ALWAYS a follow-up read of state, never the status code: failover → `GET /redundancy` activity levels on BOTH nodes [SPIKE RIG.md]; resync → content re-verify; WebDev routes → parse body `status`/`result`.
- Per-request timeouts for backups/exports (client already has `get_bytes` overrides); never auto-retry non-idempotent POSTs (`scan`, `lock`, `failover`) [REPO: client rule "never silently retry"].
- Library-only in Phase 20 means every shape needs a recorded fixture (capture-first) including the empty/204 ones.

**Phase to address:** 20; 23 for follow-up-read rules.

---

### Pitfall 28: Frozen contract: new failure modes force envelope, exit-code, MCP and TUI decisions

**What goes wrong:** (a) A rolled-back deploy returns `ok:true` because the command ran. (b) New error variants with large payloads re-trigger the `clippy::result_large_err` problem (CoreError hit exactly 128 bytes in Phase 15 [REPO .planning/WINDOWS.md #3]). (c) The MCP catalog is derived from the clap tree, so `deploy apply/rollback/projects` become one-call agent tools unless marked guarded. (d) `tui_coverage` fails the build for a CLI verb without TUI parity or an exemption.

**How to avoid:**
- Additive slugs only, within exit classes 0-7, kept in the Three-Place rule (CoreError, README, enumerated test): e.g. `deploy_mode_inactive`, `deploy_lock_expired`, `deploy_plan_stale`, `deploy_verify_failed`, `deploy_rolled_back`, `deploy_rollback_failed`, `deploy_backup_lagging`, `deploy_journal_present`. Box large payloads.
- Partial/structured results ride in `data` of an `ok:false` envelope; stdout stays pure.
- Mark `// guarded:deploy apply` / `rollback` / `projects` so they route through `preview_then_confirm`; the refusal message IS the preview, `--yes` never overrides stale/journal/lock gates (v1.1 convention). Require the plan-hash handshake rather than a bare `--yes` for agent callers.
- Decide TUI parity up front (a read-only plan/verify view, or a documented exemption in the parity allowlist).
- Standing rule from the ledger: every plan's `<verification>` lists all FIVE CI gates verbatim (fmt, clippy `--all-targets -D warnings`, `build --workspace`, `build -p ignition-cli --no-default-features`, `test --workspace`) [REPO .planning/WINDOWS.md #4].

**Phase to address:** 20 (error variants/slugs), 23 (guard + envelope), all phases (CI gates).

---

### Pitfall 29: Project import: order, restarts, and what "hash verified" means

**What goes wrong:**
- Ordering by manifest order or alphabet instead of the inheritance graph.
- Importing a parent restarts bound children and drops their Perspective sessions; this was observed ("Starting project: SPIKE_CHILD… collection=SPIKE_PARENT…") but session impact was NOT observable on the rig [SPIKE S5].
- Whether a child can actually READ inherited resources after a parent overwrite is OPEN (no REST inherited view) [SPIKE S5, SUMMARY.md]. A parent hash match proves nothing about the child.
- Verify "by hash against the release zip" (SPEC §6.5) can never match raw: import stamps `lastModification` into each project resource's `resource.json` (project resources carry `attributes.lastModification {actor,timestamp}` [REPO resource-json example, MEM resource-json-needs-attributes]), and zip metadata (entry order, mtimes, compression) differs. Also, `overwrite=true` semantics (replace vs merge: are views removed from the project but absent from the zip?) are unproven.

**How to avoid:**
- Topological order from each project's `parent`/inheritance field; cycle and missing-parent detection at plan time; parents first, no child re-import [SPIKE S5].
- Canonical content hash: per-resource data files plus normalized `resource.json` (strip `lastModification*`, signatures), independent of zip bytes; compare release-zip hash to hash of post-import `GET /projects/export/{name}`. Reuse the existing `MemberSource` engine for tree/zip normalization [REPO .planning/PROJECT.md].
- Verify deletion semantics on a rig: import a zip lacking a resource that exists on the gateway; record whether it is removed.
- Pre-check no Perspective sessions on the target and its children (or require `--allow-session-disruption`); report the child restarts expected.
- Child functional probe on `test` (e.g. a view resolves via a Perspective session or a page-config read), since REST cannot show inheritance.

**Phase to address:** 24. Capture first: overwrite delete semantics; export-vs-zip normalization diff; session impact on a rig with real WHK-Global/QSI.

---

### Pitfall 30: The git module is still live on prod during the transition

**What goes wrong:** The git module on prod (Git-2.3.4) commits every Designer save locally, never pushes, and runs its own sync/commissioning cycles against project directories [MEM prod-gateway-repo-drift-reconcile]. An API project import into a git-managed project can be committed over, reverted by the next commissioning import from the `production` branch, or flip branches; restarts and sync cycles already flip repos to the enforced branch and auto-stash uncommitted work on the dev gateway [MEM gateway-restart-forces-branch-checkout]. The deploy "succeeds", and an unrelated restart silently reverts it. Separately, the module's commissioning flags could re-import gateway resources into `core` after the cutover strip (unverified; check the prod `git.yaml`).

**How to avoid:**
- `deploy projects` refuses a project that has a `.git` directory under `data/projects/<name>` (checked over SSH/SFTP) unless explicitly allowed, and if allowed re-verifies the hash AFTER the next module sync cycle. SPEC §13 step 8 (remove `.git`, uninstall the module) is the real fix and a hard prerequisite for unattended project deploys.
- Pre-check module state via `GET /modules/healthy`/module list and record the git module version in every deploy report.
- Never delete `.git` as part of deploy; that is a human cutover step.
- Verify the git module's commissioning configuration on prod does not recreate stripped core resources.

**Phase to address:** 24 (refusal + post-sync re-verify), cutover checklist.

---

### Pitfall 31: Backup projects do not sync today (TEC-5105): false failures and misdiagnosis

**What goes wrong:** Project sync to WIGNVM04 is currently broken in prod (TEC-5105) [SPEC §3, §14], possibly because of the git module. S3 showed project sync works (~4 s) on a clean pair [SPIKE S3], so a first prod project deploy may "fail backup verification" for a reason unrelated to the deploy.

**How to avoid:**
- Backup project mismatch has its own slug and message; offer `resync`, re-verify, and never roll back master projects for it (Pitfall 23).
- Record per-node project state before the deploy so only NEW divergence is attributed.
- Do not treat TEC-5105 as fixed until the git module is gone.

**Phase to address:** 24, 23.

---

### Pitfall 32: Pair rig and Windows transport are untestable with the current rigs

**What goes wrong:** Everything above that involves the backup (Pitfalls 1, 9, 10), failover, or Windows (13-19) is "verified" against wiremock or a single-node Linux rig. The result is green CI and a red prod, the exact green-wash class this team has been burned by (StatusPing-only smoke tests that pass against the old process [MEM verify-staging-gateway-synced-commit]; WebDev 200-with-error [MEM webdev-ignores-status-key]; wiremock-only live-gate erosion, v1.1 Pitfall 5).

**How to avoid:**
- Phase 20 builds on a TWO-NODE pair rig using the spike's compose pair; GAN/pair linking needs the local-PKI certificate move (`rejected → trusted/certs`, no API) plus `PUT /gateway-network-incoming/toggleApproval/{id}/true` [SPIKE RIG.md, MEM ignition-83-mode-and-config-api-facts]. Rigs must be 8.3.6 (prod runs 8.3.6, staging/CI have been 8.3.3 [MEM onprem-windows-runner-for-vpn-deploys]); gate `ign deploy` on gateway version >= the verified floor.
- Windows-specific behaviour (ACL, robocopy, sshd quoting, locks, MAX_PATH, CRLF) can only be proven on a Windows host: GitHub's `windows-latest` has no Linux containers, so use WFLWVM01 against a SCRATCH directory (never the prod data dir) for transport tests and a throwaway Windows Ignition for the end-to-end rehearsal. Mark these tests `#[ignore]` + a named workflow dispatched on the runner.
- Negative-assertion test matrix (each must assert the BAD state is reported):

| Fault | Expected |
|-------|----------|
| Backup booted before mode existed (S3 wrong order) | pre-check refuses |
| Lock expires mid-write | no scan; collection restored; failure slug |
| Failover returns 200, peer unreachable | reported as failed via activity levels |
| Stale signature | 422 mapped to a retry-after-reread conflict, not usage error |
| Resource without uuid | verify fails via read-back + log scrape |
| Core-only folder | plan flags shadow; delete not claimed done |
| Process killed mid-apply | journal blocks next run; rollback restores |
| Undeclared resource + config-mode.json present | both survive mirror |
| Canary secret in config | appears in no output |
| 204 on `GET /scan-lock` | classified as expired, not as error and not as held |

- Evidence-ledger discipline for human-verify items (VERIFIED / MACHINE-EVIDENCE / WAIVED) carries over from v1.1.

**Phase to address:** 20 (pair rig, capture), every phase (negative tests).

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| `robocopy /MIR` straight at the collection | One line | Deletes `config-mode.json` and unmanaged resources (Pitfall 5); default retries hang (17) | Never |
| Verify by `?collection=production` read | Simple | False green when mode inactive (1) | Only as a secondary shadow diagnostic |
| Blanket sorted-key JSON writer | Reuses serde_json default | Perpetual diffs, wrong resource.json bytes (14) | Never |
| Random uuid for new resources | Trivial | Unstable renders, rename tracking (7) | Never |
| `StrictHostKeyChecking=no` | Avoids first-connect failure | MITM on the prod write path (19) | Never on prod; throwaway rigs only |
| `ssl_verify=false` profile for prod | Works with IP URLs/self-signed | Token sent over unauthenticated TLS; the client then silently accepts invalid certs [REPO build_client] | Rig only. Prod: hostname that matches the cert, or pin the fingerprint |
| Raw 422 body in error text | Easy debugging | Secret echo (12) | Never on deploy paths |
| Skipping the on-box journal | Less code | Torn state undetectable after cancel (20) | Never |
| Treating backup lag as a rollback trigger | One failure class | Discards good deploys (23) | Never |
| Uploading .gwbk as an Actions artifact | Spec-literal | Secrets exposed for 30 days (22) | Only encrypted |
| Per-resource sequential `find` read-back | Simple loop | N VPN round trips; throttling | Small N only; batch via `list` |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| `POST /scan-lock/config` | Missing body (400); not handling 409 `Lock already held by <actor>` | Always send `{acquireTimeout, holdTimeout}`; surface actor; no infinite wait [MEM ignition-83-platform-scan-api] |
| `GET /scan-lock/config` | Treating 204 as error/empty-parse failure | 200 JSON = held (read `remaining`), 204 = expired/not held [SPIKE S4] |
| `POST /scan/config` | Trusting the 200 | Poll `scanActive==false && lastScanTimestamp>before` [MEM] |
| Resource PUT | Sending an object; omitting/old `signature`; blind retry with a fresh signature | Array of `{name, signature, config}`; 422 = re-read and re-plan (optimistic concurrency exists to detect concurrent edits); keep PUT off the apply path (SPEC D5) [SPIKE S1, MEM] |
| Resource `find` | Omitting `collection` when reading a collection; reading only collection for verify | `?collection=` for shadow analysis; effective (no param) for verify [SPIKE S1] |
| `POST /mode` | Calling it from `apply`; assuming JVM arg activates | Cutover-only; node must have the mode locally BEFORE boot with the arg [SPIKE S3] |
| `POST /redundancy/gwaction/failover` | Trusting 200 | Read `GET /redundancy` on both nodes; require `peerConnected`, `syncPending=false`, same `deploymentMode` before [SPIKE RIG.md, SPEC §7] |
| `POST /redundancy/gwaction/resync` | 200 means fixed | Re-verify content afterwards [SPIKE S3] |
| API token | `Authorization: Bearer`; whole header mishandled | `X-Ignition-API-Token: <name>:<key>` [MEM ignition-83-platform-scan-api] |
| `/projects/import/{name}?overwrite=true` | Child-first or re-import children; raw zip hash | Parents first; canonical content hash [SPIKE S5] |
| WebDev routes (ign bundle, project hash helpers) | HTTP status as truth | Parse body `status`/`result` [MEM webdev-ignores-status-key] |
| SSH to Windows | cmd.exe quoting, `Program Files`, `C:` colon, host-key prompts | SFTP + pinned host keys + fixed encoded scripts (19, 15) |
| Redundancy wire parsing | Matching `role == "Primary"`; `uptime -1` as dead | Real strings are `Master`/`Backup`; `uptime:-1` and `Cold` are normal on a healthy backup [SPIKE S3] |
| Tags read-back | `GET /tags/export` full provider; compares runtime `value` | Scope to declared providers; compare config subset only (export includes `value`/`defaultValue` [SPIKE S2]); exclude MQTT providers (SPEC §1) |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Per-file SFTP inside the lock | Lock expires mid-write | Stage outside lock; local mirror inside (18) | >~100 files over the VPN |
| Per-resource API read-back | Verify takes minutes; gateway CPU | Batch `list`/`find`; bounded concurrency; file hash tree over SSH for bulk, API for sample | Thousands of tag-definition files; prod core has ~15.8k files / 14,117 tag-definition (13,166 MQTT) [SPEC §3] |
| Full `tags/export` read-back | Large slow response, 30 s client timeout | Per-provider scoped, per-request timeout override | Big providers |
| Hash-comparing the whole collection each poll | CPU/IO on the gateway host | Hash once, compare manifests | Each 30-min drift cycle |
| Defender real-time scan on the collection | Writes slow, intermittent sharing violations | Measure in holdTimeout derivation; request exclusion | First prod-size deploy |
| Scan duration scale | S4 (1-3 s) is for 1.3k local files; project scan on prod already 7.5 s | Re-measure on prod-sized tree | Prod |
| Reading plan from API only | Cannot see files the gateway rejected/ignored | Hash tree over SSH + effective API; label API-only plans "partial" | Any invalid resource on disk |

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Embedded ciphertext/plaintext secrets in `gateways/` | Credential at rest in git (11) | Deny `Embedded`, field-typed lint, references only |
| Secrets in plan/verify/error output | Logs/artifacts/MCP expose them (12) | Schema-path redaction + canary property test |
| Raw .gwbk / collection snapshot artifacts | Whole-gateway secret exposure (22) | Encrypt or keep on-prem; never upload raw |
| `ssl_verify=false` against prod | Token and writes exposed to MITM | Verified TLS or pinned fingerprint on prod profiles |
| Unpinned SSH host keys | MITM on config write path (19) | Committed known_hosts, `StrictHostKeyChecking=yes` |
| Persistent runner holds keys/renders; runner group open to all repos | Other workflows read them [MEM onprem-windows-runner-for-vpn-deploys] | Restrict runner group; `always()` cleanup; key in memory if possible |
| `ci-deploy` token over-privileged | Token compromise = full gateway write | Custom `Authenticated/Deploy` level only [SPIKE RIG.md]; per-env tokens; rotation |
| Agent can invoke prod apply via MCP | Unreviewed prod change | Guarded marker + plan-hash handshake; deploy tools excluded from unattended catalog if the gate cannot be enforced |
| Deploy token via command-line args | Visible in process list | Env/keyring chain only; never argv |
| Writing the declared tree with secret-bearing `description`/comments | Leak via free text | Lint free-text fields too |

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| "Applied" printed before verify | Operator believes done | Distinct phases in output; final status only after verify and backup check |
| `plan` shows 400 lines of byte diffs | Reviewers skim past the destructive item | Group: adds/changes/deletes/shadowed/destructive/restart-class/unmanaged; counts first |
| Rolled back deploy exits 0 | CI green on a no-op | `ok:false`, dedicated slug, both failure and rollback outcome in `data` |
| Refusing with a generic 409 | User waits/retries | Name the actor and remaining time of the held lock; name the journal run id |
| `--yes` bypasses staleness | Clobbers an emergency edit | Staleness/journal/mode gates not `--yes`-able |
| Output differs between CLI, MCP, TUI | Agent and human disagree | One result struct, three renderers (existing contract pattern) |
| API-only `plan` claims "no changes" | False confidence | Label "partial: cannot see unloaded files" and exit non-zero under `--strict` |

## "Looks Done But Isn't" Checklist

- [ ] **Apply:** Often missing the pre-check that BOTH nodes run the mode; verify the refusal against a backup booted pre-mode.
- [ ] **Apply:** Often missing restore-on-abort; kill the process mid-write and assert the collection equals the snapshot.
- [ ] **Verify:** Often reads `?collection=` only; assert it reads the effective view and the backup's own API.
- [ ] **Verify:** Often misses unloaded resources; assert a uuid-less resource FAILS verify and the log-scrape names it.
- [ ] **Mirror:** Often deletes `config-mode.json`/unmanaged resources; assert both survive.
- [ ] **Render:** Often not byte-identical to gateway output; the 183-file corpus round-trip passes on Linux, macOS and Windows.
- [ ] **Render:** Often has CR bytes via checkout; CI asserts no `\r` in `gateways/`.
- [ ] **Plan:** Often non-empty immediately after apply; the apply-then-plan-empty gate passes against a real rig.
- [ ] **Plan:** Often doesn't flag core shadowing; a core-only folder fixture is flagged.
- [ ] **Secrets:** Often only the plaintext case is linted; an `Embedded` fixture fails and the canary property test passes across all outputs.
- [ ] **Backup:** Often trusts `syncPending=false`; assert a stale backup (sync gave up) fails verify, and secret NAMES exist on both nodes.
- [ ] **Failover/resync:** Often trusts HTTP 200; assert activity levels are read afterwards.
- [ ] **Rollback:** Often untested after a real failure; rollback runs the full lock-write-scan-verify and is itself fault-injected.
- [ ] **Projects:** Often hash-compares raw zips; a canonical hash ignoring `lastModification` matches post-import export; git-managed project refused.
- [ ] **Windows:** Often only tested from macOS; transport suite run on WFLWVM01 against a scratch dir with spaces and a >200-char path.
- [ ] **Contract:** Often adds slugs in one of three places; the Three-Place test and README table pass; TUI parity and guarded markers present; all five CI gates run locally.
- [ ] **Smoke tests:** Never StatusPing-only; every check asserts a body-level fact (version/hash/uuid/activity level), per [MEM verify-staging-gateway-synced-commit].

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Torn tree on disk, lock expired (3, 20) | MEDIUM | `ign deploy rollback --run <id>` (lock, restore on-box snapshot, scan, verify); if the process is gone, the journal names the snapshot; manual: acquire lock, robocopy snapshot back, scan |
| `config-mode.json` deleted (5) | HIGH | Re-create the mode via `POST /mode` on the master; confirm on backup BEFORE any restart; follow SPEC §13 step 4 ordering; backups: snapshot contains the file |
| Backup on wrong mode / stale (1, 9) | MEDIUM | Confirm mode+collection on backup; add the JVM arg (jvm-args.txt) and restart the backup; `POST /redundancy/gwaction/resync`; re-verify by content |
| Deploy token locked out (8) | HIGH | SSH restore of snapshot (SSH path does not need the token), then re-add the level in `core` security-properties via the UI with an admin; scan from the Designer/UI |
| Secret missing on backup (10) | LOW | Write the secrets to the backup's API; verify names |
| Perpetual diff after deploy (14, 26) | LOW-MEDIUM | Capture the differing bytes (`diff` of rendered vs on-disk), add to the golden corpus, fix serializer, no gateway change needed |
| Project reverted by git module (30) | MEDIUM | Re-import; check module; execute SPEC §13 step 8 |
| Child project broken after parent import (29) | MEDIUM | Re-import parent (no child re-import); check children restart in gateway log; session refresh |
| Snapshot leaked via artifact (22) | HIGH | Delete artifact, rotate everything in the gateway (DB passwords, OAuth secrets, API tokens), treat as incident |
| Robocopy hung in lock (17) | LOW | Abort-restore path; lower `/R /W`; locate the locking process |

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| 1 Inactive mode / writes to wrong collection | 20, 23 | Rig pair w/ wrong-order backup: apply refuses; verify reads effective view |
| 2 Core shadow / folder granularity | 21, 22 | Core-only folder fixture flagged; delete not claimed |
| 3 Lock expiry torn tree | 23 | Fault-injection: `holdTimeout=2` mid-write, no scan, restored |
| 4 holdTimeout from spike numbers | 20, 23 | Measured p95 on prod-size rig; floor/ceiling enforced; value printed |
| 5 Mirror deletes gateway-owned files | 22, 23 | config-mode.json + undeclared resource survive |
| 6 Scan 200 / unloaded resources | 20, 23 | uuid-less fixture fails verify; scan predicate unit tests |
| 7 uuid rules / stability | 21 | Per-type table from bootstrap; double-render byte-equal |
| 8 Self-lockout | 21, 22, 23 | Security singleton refused; lock-first 403 aborts before writes |
| 9 Backup false-green | 20, 23 | Stale-backup rig (sync gave up) fails verify |
| 10 Secret values per node | 21, 23 | Name presence checked on both nodes |
| 11 Embedded ciphertext | 21 | Embedded fixture fails render |
| 12 Secret leakage in output | 20, 21, 22, 23 | Canary property test across all surfaces |
| 13 CRLF | 21 | No `\r` in `gateways/`; Windows CI leg golden |
| 14 Gson-compat bytes | 21, 22 | 183-file corpus round-trip on 3 OSes |
| 15 Windows paths | 21, 23 | Space/colon/long-path tests on WFLWVM01 scratch |
| 16 ACL on moved files | 23 | Read-back after mirror on a Windows box |
| 17 Locks/robocopy defaults | 22, 23 | Exit-code bitmask unit test; `/R:2 /W:1` asserted |
| 18 Slow work in lock | 23 | Staging outside lock; lock-held time measured |
| 19 SSH non-interactive/Windows | 23 | BatchMode, pinned keys, key cleanup test |
| 20 Cancel/crash journal | 23 | kill -9 mid-apply; next run refuses; rollback restores |
| 21 Rollback unverified | 23 | Rollback fault-injected; verify after rollback |
| 22 Snapshot artifact secrecy | 23 | CI check excludes `*.gwbk`; encryption decision recorded |
| 23 Auto-rollback misattribution | 23 | Backup-lag scenario does NOT roll back master |
| 24 Clocks | 20 | No wall-clock comparisons; redundancy wire docs updated to S3 |
| 25 Concurrency / stale plan | 22, 23 | Plan-hash binding; re-hash under lock; two-process test |
| 26 Perpetual diffs / signatures | 20, 21, 22 | apply→plan empty; apply→apply writes zero; capture-first test recorded |
| 27 Tolerant-client over-tolerance | 20, 23 | Per-endpoint success predicates; failover 200-fail fixture |
| 28 Frozen contract/MCP/TUI | 20, 23 | Three-Place test; guarded marker; tui_coverage; five gates |
| 29 Project order/restart/hash | 24 | Topological-order test; canonical hash vs export |
| 30 Git module during transition | 24 | `.git` project refused; post-sync re-verify |
| 31 TEC-5105 backup project sync | 23, 24 | Distinct slug; no master rollback |
| 32 Rig and Windows test gap | 20 (+ all) | Pair rig exists; negative-assertion matrix green; Windows suite on WFLWVM01 |

## Capture-first backlog (record in `*-LIVE-CAPTURES.md` BEFORE writing dependent code)

| # | Question | Needed by |
|---|----------|-----------|
| C1 | Exact request/response of scan-lock acquire (409 body), `GET` held vs expired, any release/DELETE; scan/config status JSON | 20 |
| C2 | Does a scan-lock block UI/Designer/API writes to the active mode collection? | 23 |
| C3 | Stale/omitted `lastModificationSignature` on a file-written resource: loaded? rewritten? flagged? | 21 |
| C4 | Which prod resource types are named (uuid) vs singleton; bootstrap census from prod core | 21 |
| C5 | How Ignition encodes hostile names (`:` `?` trailing dot, case-collisions) in tag/resource directories | 21 |
| C6 | Does a mode-collection directory neighbour (staging dir) get read as a collection? Where may staging live? | 23 |
| C7 | Windows: ACL after robocopy vs rename; sharing violation on a loaded driver jar; Defender cost; long-path behaviour in sftp-server/robocopy | 23 |
| C8 | Does import `overwrite=true` delete resources absent from the zip? Does export-after-import equal the zip after normalization? | 24 |
| C9 | Child reads inherited resource after parent overwrite; Perspective session impact | 24 (S5 OPEN) |
| C10 | Prod git module `git.yaml`/commissioning flags: can it recreate stripped core resources or revert API imports? | 24 |
| C11 | Single-scan ordering when a tag-provider and its tag-definitions arrive together | 23 |
| C12 | Does a half-lagging backup recover through `resync` alone after "failed 5 times"? | 23 |

## Sources

- `results/S1.md` ... `S5.md`, `results/SUMMARY.md`, `results/RIG.md` (spike evidence, 8.3.6, 2026-10-09). HIGH.
- `/Users/pmannion/data/worktrees/whk-environment-orchestration/gateway-config-spec/docs/superpowers/specs/2026-10-09-prod-gateway-config-as-code-design.md` §1, §3, §4, §6, §7, §8, §9, §10, §12, §13, §14. HIGH (design intent); its own caveats (S5 OPEN, holdTimeout on target transport) are carried here.
- `/Users/pmannion/data/worktrees/whk-environment-orchestration/gateway-config-spec/gw-config/` file census and byte inspection (resource.json key order, absence of trailing newline, no CR, 10 Embedded ciphertext files, resource names with spaces). HIGH for what it shows; a dev snapshot, not prod.
- `/Users/pmannion/whiskeyhouse/ignition-cli/crates/ignition-core/src/client/{mod.rs,classify.rs,redundancy.rs}`, `.planning/PROJECT.md`, `.planning/WINDOWS.md`, `.github/workflows/ci.yml`. HIGH.
- User memory notes: `ignition-83-mode-and-config-api-facts`, `ignition-83-platform-scan-api`, `gateway-secrets-dont-replicate-to-backup`, `webdev-ignores-status-key`, `verify-staging-gateway-synced-commit`, `prod-ign-gateways-ssh-key-auth`, `onprem-windows-runner-for-vpn-deploys`, `production-cd-pipeline-windows-shell-bug`, `prod-gateway-repo-drift-reconcile`, `gateway-restart-forces-branch-checkout`, `resource-json-needs-attributes`. HIGH (first-hand verified), dated.
- IA docs and forum (MEDIUM; staff statements, not contractual): [resource.json file](https://docs.inductiveautomation.com/docs/8.3/appendix/reference-pages/resource-json-file), [gateway folder structure](https://docs.inductiveautomation.com/docs/8.3/appendix/reference-pages/gateway-folder-structure), [Deployment Modes and resource definition UUIDs](https://forum.inductiveautomation.com/t/deployment-modes-and-resource-definition-uuids/114966), [Git Version Control Resource.Json Files](https://forum.inductiveautomation.com/t/git-version-control-resource-json-files/111329/2), [LastModification javadoc 8.3.x](https://sdk.inductiveautomation.com/javadoc/ignition83/8.3.7/com/inductiveautomation/ignition/common/resourcecollection/LastModification.html). The signature algorithm is unpublished (searched 2026-10-10).
- Windows platform behaviour (NTFS ACL inheritance on move, robocopy retry defaults/exit bitmask, Java file sharing, MAX_PATH, OpenSSH client key ACL): general platform knowledge, MEDIUM, tagged [INFERRED] where used; every one has a capture task (C7) before dependent code.

---
*Pitfalls research for: declared-config deploy (`ign deploy`) against Ignition 8.3.6 redundant Windows pair*
*Researched: 2026-10-10*
