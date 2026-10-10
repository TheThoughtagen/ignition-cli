# Project Research Summary

**Project:** ignition-cli — milestone v1.6 "Declared Gateway Deploy" (`ign deploy {render,plan,apply,rollback,verify,projects}`)
**Domain:** Declared-config deploy tooling for a redundant Ignition 8.3.6 pair (Windows Server; master WIGNVM01 10.4.8.15, backup WIGNVM04 10.4.8.29; deployed from runner WFLWVM01 over VPN into the `production` deployment-mode collection)
**Researched:** 2026-10-10
**Confidence:** MEDIUM overall — HIGH on crate choices and the spec contract; MEDIUM-LOW on Windows transport and gateway-written tag bytes, both capture-gated

## Executive Summary

`ign deploy` turns the `production` deployment-mode collection of a redundant Ignition pair into a declared, reviewable artifact. A repo tree (`gateways/<name>/`) is rendered offline, diffed against live state, applied under the scan-lock with a snapshot and automatic rollback on master-verify failure, then proven by read-back plus backup-node checks. Terraform, kubectl, Helm and Stoker converge on plan-before-apply, one shared diff engine, and rollback recorded as a run. None models the Ignition-specific hazards: scan-lock expiry that silently applies a partial tree, core-layer shadowing, a mode a node silently ignores when absent at boot, and resources that fail to load with only a log line. Correctness rests on a few verified gateway facts and on byte-level fidelity for files the gateway writes, so build capture-first: record wire truth before the code that depends on it.

Recommended shape: one apply engine, reused by rollback and auto-rollback, in six phases — library-only config-API client (20); offline render with a Gson-compatible writer and secret lint (21); shared transport plus read-only `plan` and `verify` (22); single-gateway guarded `apply` and `rollback` (23a); pair semantics (23b); guarded `projects` (24). One new crate (`yaml_serde`) plus promoting `similar`. SSH shells out to system OpenSSH behind a trait, not an SSH library. Every mutating verb is gated once, in core, after a read-only plan; the refusal message is the plan.

Main risks are in the write path: a lock that expires mid-write leaves a torn tree the next scan applies; a backup that never converged reads green on status flags; a mirror with deletes wipes `config-mode.json` and undeclared resources; Windows transport quirks (quoting, ACLs, robocopy exit codes, CRLF) are unproven on real hosts. Each has a named prevention and a fault-injection test in its owning phase. "Conflict Resolutions" settles cross-file disagreements. "Decisions Required" lists four items to decide before Phases 22 and 23a are planned.

## Conflict Resolutions

1. **YAML crate: `yaml_serde` 0.10 (STACK wins over ARCHITECTURE).** ARCHITECTURE (§3.2, §14 Q6) suggested `serde-saphyr`. STACK verified serde-saphyr 1.2.0+ declares MSRV 1.89; workspace floor is 1.88. `yaml_serde` 0.10.7 (MSRV 1.82) was compiled and run on rustc 1.88 and enforces `deny_unknown_fields` with line/column errors. Closes ARCHITECTURE OQ6.

2. **Transport seam: one layered seam (reconciles ARCHITECTURE §4.4, STACK §1, PITFALLS 18/19/25).**
   - **Bottom, `RemoteShell`** (STACK §1; mirrors `rig/compose.rs`). `run_ps(script)` = `ssh ... powershell -NoProfile -NonInteractive -EncodedCommand <b64>`. `put(local, remote)` = `sftp -b <batch>`. Argv only. `BatchMode=yes`, `StrictHostKeyChecking=yes` with pinned known_hosts, `ConnectTimeout`, `ServerAlive*`. Stdin `Stdio::piped()` then closed immediately, not NUL (Win32-OpenSSH hang). `kill_on_drop(true)` plus `tokio::time::timeout` on every call.
   - **Domain, `FileTransport`** (ARCHITECTURE §4.4). Coarse verbs `hash_tree`, `read_tree`, `mirror_tree`. Impls: `SftpTransport` (over `RemoteShell`) and `LocalFsTransport` (tests, ephemeral rig bind mount).
   - **Write flow.** (a) Before the lock: one archive uploaded via sftp to a home-relative staging path; remote hash verified. (b) Inside the lock: ONE encoded PowerShell call takes an on-box snapshot copy, applies a bounded delta from staging, returns per-file hashes as one `IGN-RESULT:{...}` line; parse only that sentinel. (c) Scan.
   - **`mirror_tree` takes an explicit delete set, not "delete extras".** Delete set = (names declared in the previously applied tree/snapshot) minus (rendered set). Rollback's delete set = (current declared) minus (snapshot). Nothing outside the manifest allowlist is ever deleted (PITFALLS 5).
   - **STACK's `robocopy /MIR` is rejected.** A full mirror deletes `config-mode.json` and undeclared resources. Any robocopy use is without `/MIR`, with `/R:2 /W:1 /NP /NFL`, exit code decoded as a bitmask (0–7 success, 8+ failure). Deletes use the bounded list.
   - **Unresolved: staging location.** ARCHITECTURE §4.4: no staging directory beside the collection, because spec §9 scopes `svc-ignition-deploy` to write `data/config/resources/<mode>` only. STACK and PITFALLS 18 want staging outside the lock. Blocks 23a; see Decision B.

3. **Phase order: ARCHITECTURE §9, with two clarifications.**
   - ARCHITECTURE §9 moves BOTH transport halves into 22, so the write path is written and tested once, as a pair. No mutating verb calls the write half until 23a.
   - PITFALLS 32 tags a two-node pair rig to Phase 20; ARCHITECTURE puts the pair fixture in 23b. Adopt ARCHITECTURE: Phases 20 and 21 need one 8.3.6 ephemeral gateway. Backup-dependent captures (C12, pair convergence) go to 23b.

4. **"Sorted keys" in spec §4 is wrong — spec correction.** Of 91 gateway `resource.json` files, 0 are key-sorted. Order is POJO order (`scope, [description,] version, restricted, overridable, files, attributes`); `attributes` is `lastModification, uuid, lastModificationSignature, enabled`. `config.json` is mostly alphabetical but `security-properties/config.json` is not. No single sort rule matches; sorting would rewrite every `resource.json`. Consequences:
   - Spec §4 "sorted keys" becomes "preserve source order".
   - ARCHITECTURE §3.1 (`deploy/gson.rs` described as sorted) and PITFALLS 14 (sorted `config.json`) are superseded.
   - Writer uses an order-preserving `Node` tree (`Vec<(String, Node)>`), not serde_json's map. Gson HTML-safe escaping: `<` `>` `&` `=` `'` → `\u003c \u003e \u0026 \u003d \u0027`. 2-space indent, no trailing newline, non-ASCII raw. Byte-identical on 178/183 files; the other 5 come from different writers.
   - `serde_json/preserve_order` and `arbitrary_precision` are NOT enabled workspace-wide (feature unification would change key order for every `json!` payload in the frozen contract).
   - Render is passthrough-by-default: gateway bytes copied; only ign-authored files synthesized; rewrite only if `write(parse(bytes)) == bytes`, else opaque and copied verbatim.

5. **Resource uuid scheme (FEATURES/PITFALLS 7 say UUIDv5; STACK §5 says persisted v4).** Use a deterministic name-based id so render stays pure and nothing is persisted. Bootstrapped resources keep the gateway's uuid. Phase 21 checks `cargo tree` for `sha1`; if absent, either add it or derive a 128-bit id from the existing `sha2` with version-5-style bits, labelled name-based (not strict RFC 4122 v5). Do not add the `uuid` crate (MSRV 1.89). 30 of 91 gateway resources carry no uuid, so "always add one" is wrong; a per-type rule table (named vs singleton) drives it.

6. **Error envelope vs verdict (FEATURES §3.3–3.5 vs ARCHITECTURE §7.2).** The error envelope has no `data` (SF-1). Resolution:
   - **Pre-write refusals** are `CoreError` envelopes (exit 2, 4 or 6): preflight, lock 409, stale plan, destructive refusal, confirmation required, invalid tree, embedded secret, journal present.
   - **Post-write outcomes** (`rolled_back`, backup unsynced, verify mismatch) are success-shaped envelopes with full `data` and exit 6, set in `main.rs` after the envelope (`ign testing run` precedent).
   - Every run writes `result.json`, including on failure.
   - Top-level `ok` for verdicts is Decision A.

7. **Slug names (three lists disagree).** Canonical = ARCHITECTURE §7.1, 11 `CoreError` rows:

   | Slug | Exit | Note |
   |---|---|---|
   | `deploy_tree_invalid` | 2 | manifest/path/allowlist error, cycle |
   | `embedded_secret` | 2 | names path, never value |
   | `destructive_change_refused` | 2 | `--allow-destructive` absent |
   | `transport_failed` | 4 | ssh/sftp failure |
   | `scan_lock_held` | 6 | 409, actor named |
   | `scan_lock_expired` | 6 | FEATURES: `deploy_lock_expired` |
   | `deployment_mode_mismatch` | 6 | PITFALLS: `deploy_mode_inactive` |
   | `deploy_preflight_failed` | 6 | all failing checks at once |
   | `project_hash_mismatch` | 6 | pin mismatch, pre-upload |
   | `rollback_failed` | 6 | FEATURES: `deploy_rollback_failed`; state unknown |
   | `plan_stale` | 6 | FEATURES: `deploy_plan_stale` |

   Add `deploy_journal_present` (exit 6, pre-write refusal, PITFALLS 20) as a twelfth variant. Verdict slugs in payload only, never in exit tables: `deploy_verify_failed`, `deploy_rolled_back`, `deploy_backup_unsynced` (PITFALLS' `deploy_backup_lagging` folded into it). Three-Place rule applies to all.

8. **Plan-binding naming.** Use `plan_sha256` and `--plan-sha256` everywhere (FEATURES), with `plan_stale` as the error. ARCHITECTURE's `plan_id` / `--expect-plan` are retired.

9. **Redundancy vocabulary.** Live 8.3.6 pairs report `Master`/`Backup`; `redundancy.rs` doc says `Primary`/`Backup`. Decide the active node by `activityLevel`, never `role`. Fix the doc in Phase 20. `uptime: -1` or `Cold` on a healthy backup is normal.

10. **`deploymentMode` on a no-mode gateway.** Spec says `""`; other notes say `null`. Normalize both to `""` (ARCHITECTURE OQ7).

## Key Findings

### Recommended Stack

One new crate, one promotion, no SSH library.

| Technology | Version | Purpose | Why |
|---|---|---|---|
| `yaml_serde` | 0.10 (0.10.7) | Parse `manifest.yaml`, `targets/<env>.yaml`, `gateway-deps.yaml` (read-only; render emits JSON) | YAML-org continuation of serde_yaml. MSRV 1.82, verified on 1.88. `deny_unknown_fields`, line/column errors, no YAML 1.1 "Norway problem". |
| `similar` | 3 (locked 3.2.0), `default-features=false`, `text` | Unified text diff for `plan` preview | Already compiled (snapbox). Promotion adds no crate. |
| system `ssh`/`sftp` via `tokio::process` | OpenSSH 10.3 local; Win32-OpenSSH on hosts | Transport | Honors `~/.ssh/config`, `IdentityAgent` (1Password/Bitwarden), `known_hosts`. russh needs MSRV 1.89 and ~124 more crates; `openssh` is Unix-only. |
| local `ordered_json` on `serde_json` `PrettyFormatter` | existing 1.0.151 | Gson-compatible, order-preserving writer | ~100 lines, zero new deps. |
| hand-rolled UUID over `getrandom` 0.2 or `sha2` | existing | `attributes.uuid` | `uuid` crate MSRV 1.89. See Conflict 5. |

Reused with new consumers: `tokio` (process, time), `base64` (UTF-16LE for `-EncodedCommand`), `sha2`, `zip` (deterministic archive), `tempfile` (batch files; keys at 0600), `proptest` (writer idempotence). Do NOT add: `russh`, `russh-sftp`, `ssh2`, `openssh`, `uuid`, `json-patch`, `walkdir`, `serde_yml` (unsound, RUSTSEC-2025-0068), `serde_yaml` (archived), `serde_yaml_ng` (stale), a second TLS provider.

Windows constraints:
- PowerShell 5.1 only. Prelude every script with `$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'`, UTF-8 output, explicit `exit`.
- Base64(UTF-16LE) inflates 2.67×. Keep inline scripts under ~3,000 chars or ship a `.ps1` in the archive and call it from a one-line `-EncodedCommand`. Assert at build time.
- Zip via `System.IO.Compression`, not `Expand-Archive`.
- No ControlMaster on Win32-OpenSSH: one archive up, one remote script inside the lock.
- Keys: user-only ACL; 0600 temp file; delete in an `always()` step.
- WFLWVM01 needs the OpenSSH Client capability; `onprem-linux` runner needs `openssh-client`. Preflight fails fast if missing.
- Never place snapshots or staging inside `data/config/resources/`.

### Expected Features

**Table stakes:**
- `render`: deterministic (identical `render_sha256`); overlay by path (`modes/<env>/` wins, whole-file); allowlist (unlisted types stay gateway-owned); embedded-secret lint (references only); uuid on every NEW named resource; JSON-validity lint; case-fold collision refusal.
- `plan`: four-status diff (`added|changed|removed|same`), path-sorted entries, summary counts; zero writes, zero locks; `plan_sha256`; destructive flag (removal of tag definitions, DB/OPC connections, identity providers, engine/journal); lockout flag (`security-properties`); core-shadow classification; preflight rows.
- `apply`: re-plan and gate on fresh plan; empty plan = exit 0 `noop`, no lock/snapshot/scan; snapshot before write (collection tree with hashes, project exports, `.gwbk` default with recorded `--skip-gwbk`); scan-lock with 409 refusal and expiry detection; bounded mirror; scan and poll; master verify; auto-rollback on master verify failure only; backup verification last; `restart_pending` reported, not performed; redacted output.
- `rollback --run <id>`: apply engine with snapshot source; refuse if live state differs from the run's post-apply fingerprint unless `--discard-live-changes`; honest scope (tag values, secrets, modules, restart-class changes not restored).
- `verify`: scan complete; `deploymentMode` equals target on BOTH nodes; every declared resource reads back equal (effective view, `allowInvalidReferences=false`); log scan for `will not be loaded`; sampled OPC/MQTT tags Good; `restart-tasks/pending` reported.
- `projects`: release zip sha256 pin verified before any write; hash no-op skip; parents first with cycle/missing-parent detection; pre-import export; `import?overwrite=true`; verify by normalized hash; unmanaged projects reported, never deleted.

**Differentiators:** `plan_sha256` binding; stage-then-mirror; re-hash under lock; `shadows_removal` core state that BLOCKS apply; security-singleton lockout guard; three-way drift via `--base` (SF-4); phase timings in `run.json` to derive `holdTimeout`.

**Defer:** `ign drift`; `redundancy restart --rolling`; `--detailed-exitcode` (new exit code = taxonomy change); bootstrap/`deploy pull`; tombstone overlays (SF-7); restart-class hints; `rollback --list`; TUI panels beyond rows.

**Anti-features:** `--force`, `--skip-verify`, `--no-rollback`; auto-revert of drift; restart or failover inside apply; any write to backup; templating/JSON-patch overlays; secret values in render; deleting projects absent from manifest; gateway-side revision storage; "rollback to git ref" (= render+plan+apply of old commit).

### Architecture Approach

New code in `ignition-core/src/deploy/` (manifest, target, layout, render, gson, secret_lint, tree, plan, lock, run_store, events, transport/, projects); thin verbs in `actions/deploy.rs`; wire in `client/{config_resources,scan,mode,deploy_api}.rs`. CLI: six `Commands::Deploy` leaves, dispatch in `main.rs`. TUI: six rows to `Screen::Projects` (workspace precedent). MCP: no catalog change; `apply`, `rollback`, `projects` added to `GUARDED_OPS` with `// guarded:` markers kept in `main.rs`.

- **New trait `DeployApi`**, not new `GatewayApi` methods: `GatewayApi` has 15 hand-written `unreachable!()` doubles. A separate trait needs zero edits and allows one stateful in-memory fake gateway.
- **`MemberSource::Zip` only, only for `projects`.** `Tree` is manifest-scoped, FNV-1a, project-export layout. The deploy tree uses a sha256 `RenderedTree`; reuse only `normalize_descriptor`.
- **Reuse:** `Session::resolve_side`, `poll::poll`, `project_import` (200-denial check), `project_export_to_file`, `diff_members`, `redundancy_status`, `designers`, `backup_download`, `status_ping`, `validate_import` (promoted to `pub(crate)`).
- **Gate in core, after read-only plan, before any write** (`workspace_push` precedent): load; render+lint; resolve sessions; preflight; observe+plan; zero-write noop; destructive refusal; plan-stale refusal; `require_confirmation(yes, preview)`; then snapshot, lock, mirror, witness, scan, poll, verify.
- **Types for safety:** `HeldLock::witness` returns `ScanReady`; scan requires it. Pure `lock_verdict(elapsed, hold, margin, info)`; gateway `remaining` is primary, monotonic `Instant` for local deadlines. Never compare gateway timestamps to the runner clock.
- **Abort path:** no documented scan-lock release route (confirm in Phase 20). Expiry applies nothing, but a half-written tree would be applied by the next scan from any actor. So on failure, while still held, restore snapshot then scan; if lost, fresh lock then the same. Auto-rollback, manual rollback and abort share one `restore_snapshot(run)`.
- **On-box journal:** `deploy.inprogress` outside `resources/`, written before first write; plan/apply/rollback refuse while present (`deploy_journal_present`); rollback runnable from a fresh process with only the run id.
- **Run dir** (`./.ign/deploy/<env>/<run-id>/`): 0700 dir, 0600 files; `run.json`, `result.json`, `plan.json`, `snapshot/`, `events.ndjson`, `verify.json`. Secret-bearing (`.gwbk`, exports). Never in the collection.
- **Stdout purity:** actions never print; progress via `DeploySink` to stderr and `events.ndjson`; the JSON envelope is the only stdout.

### Critical Pitfalls

1. **Mode not active on a node.** Pre-check `deploymentMode` on BOTH nodes before lock; verify the effective view (no `collection` param); never auto-activate a mode in apply. Test: backup booted before mode existed; apply refuses.
2. **Lock expiry mid-write leaves a torn tree the next scan applies.** Restore while held, then scan, verify, report. Use gateway `remaining`. Fault injection: `holdTimeout=2`, sleep mid-write; assert no scan, collection equals snapshot, correct slug.
3. **200 from scan or green flags is not "loaded".** Predicate: `scanActive==false && lastScanTimestamp > before` (same gateway). Read back every declared resource. Scrape the log for `does not have required UUID attribute, will not be loaded`; such a resource fails verify.
4. **Mirror deletes erase `config-mode.json` and gateway-owned resources.** Bounded delete set; guard aborts if `config-mode.json` is missing after mirror. Test: an undeclared resource and `config-mode.json` both survive.
5. **Backup false green.** Verify backup content via the backup's own API and SSH file hash, plus `deploymentMode` on both nodes. A backup that gave up after "failed 5 times" fails verify. Secret NAMES must exist on both nodes (values do not replicate; ENT-3115). Backup lag never auto-rolls-back master.
6. **Secrets.** `"type": "Embedded"` fails render regardless of field name (10 files in the dev snapshot contain ciphertext). Redact by schema path, never regex on values. Cap gateway bodies in errors (`classify.rs` echoes raw 422 bodies today). Canary property test across plan/apply/verify/`--json`/MCP. Never upload a raw `.gwbk`.
7. **Perpetual diffs.** `lastModification*` churn: compare semantically with those ignored; render never uses `now()`. Gate: `bootstrap → plan` empty; `apply → plan` empty; `apply → apply` writes zero files, no scan.
8. **Self-lockout via security singletons.** Deny `security-properties`, `security-levels`, API tokens and user sources in the v1.6 allowlist. Lock-first 403 probe before any file is touched.

Also: CRLF from `text=auto` checkout on WFLWVM01 (`gateways/** -text`; CI check for `\r`); robocopy retry defaults; git-managed projects (refuse `.git` under `data/projects/`); TEC-5105 backup project sync (own slug, never master rollback).

## Implications for Roadmap

Phases 20 and 21 can run in parallel.

### Phase 20: Config API client, scan-lock, mode (library-only)
**Rationale:** Every later phase depends on the wire; captures decide success predicates. Capture doc before code.
**Delivers:** `client/{config_resources,scan,mode,deploy_api}.rs`; `DeployApi` trait and `ReqwestGatewayApi` impl; `classify.rs` 409 arm on `/scan-lock/` → `ScanLockHeld`; typed response shapes (`Json`, `EmptyOk`, `NoContent`, `JsonOrEmpty`) with per-endpoint success predicates; `deployment_mode()` normalization; `redundancy.rs` vocabulary fix; wiremock contract tests; `20-LIVE-CAPTURES.md`.
**Avoids:** P3 (no 200 trust), P24 (no wall clock), P27 (tolerant client), P28 (three-place slugs).
**Captures (blocking):** C1, C3a, release-route grep, `null` vs `""`, scan completion rule.
**Gate:** single ephemeral 8.3.6, bind-mounted `resources/`.

### Phase 21: Render engine and `deploy render` (local, offline)
**Rationale:** No gateway needed; must be byte-correct before anything writes to a gateway.
**Delivers:** manifest/target parsing (`yaml_serde`, `deny_unknown_fields`, `schema_version` checked first, no anchors); `layout.rs` (single injective repo-path → collection-path → `ResourceKey` map; fail-closed on case-fold collisions); order-preserving writer and passthrough; overlay; allowlist with security singletons denied; uuid rule table and deterministic id; secret lint (Embedded denied by type; field-typed plaintext); name and path-length lint; `render.json` with `render_sha256`; `deploy render` verb with TUI and README rows; `similar` promotion.
**Avoids:** P7 (uuid), P8 (singletons), P11 (Embedded), P12 (redaction), P13 (CRLF), P14 (bytes), P15 (Windows names).
**Gate:** 183-file corpus round-trips byte-identical on Linux, macOS, Windows; render golden on `windows-latest`; no `\r` in `gateways/`.
**Captures (blocking):** C4, C5, C3b, OQ1 (tag-file bytes), OQ4 (tag path mapping), OQ8 (Embedded shape).

### Phase 22: Transport (both halves), `plan`, `verify` (master, read-only)
**Rationale:** `plan` must read the live collection; tags have no per-resource REST route. Writing the transport as a pair avoids retrofitting. Proves spec §13 step 2 ("render equals prod, empty plan") before any write code exists.
**Delivers:** `RemoteShell`, `SftpTransport`, `LocalFsTransport`, `ProcessRunner`, `FileTransport`; `ObservedState`; planner (four-status diff, core classification, destructive/lockout flags, preflight rows); `plan_sha256`; `deploy plan` (`--diff` redacted); `deploy verify` master (scan complete, mode, read-back, hash compare, log scrape, tag probes); hold-budget measurement. Write half tested against a scratch dir, not called by any mutating verb.
**Avoids:** P1, P2 (shadow flags), P5 (delete set in plan), P12 (plan redaction), P13/P15 (paths), P19 (transport hardening), P25 (plan hash).
**Captures (blocking for 23a):** C6 (staging), C7a (Win32-OpenSSH sftp path/quoting/key ACL/stdin hang/CLIXML), OQ5 (real-transport timings on WFLWVM01 with a prod-sized tree; sets `holdTimeout`).

### Phase 23a: Guarded `apply` and `rollback`, single gateway
**Rationale:** Riskiest phase; build on a single ephemeral 8.3.6 with `LocalFsTransport` where fault injection is cheap.
**Delivers:** run store and run dir; on-box snapshot inside lock, pulled to run dir; `HeldLock`, `ScanReady`, `lock_verdict`; abort path; bounded-delete mirror (staging per Decision B); scan and poll; master verify; auto-rollback on master verify failure only, with classified causes (`master_effective_mismatch`, `unloaded_resource` roll back; `probe_flake` retries; backup lag never rolls back); on-box journal; preflight with wrong-target guard (system name); `--designers fail|wait|proceed` (default fail); lock-first 403 probe; gate and refusal-as-preview; verdict exit hook; `--plan-sha256` handshake (Decision C); `rollback --run` with staleness refusal; `restart_pending` reported; `.gwbk` default on via `backup_download`.
**Avoids:** P3, P4 (measured `holdTimeout`, floor/ceiling), P5, P8, P16 (ACL), P17 (locks, robocopy), P18 (stage outside lock), P20 (journal, kill -9 test), P21 (rollback fault-injected), P22 (snapshot secrecy), P23, P25.
**Captures (blocking):** C2 (scan-lock vs UI writes), C7b (ACL after mirror, loaded driver jar sharing violation, Defender, long paths), C11 (single-scan ordering).

### Phase 23b: Pair semantics
**Rationale:** Needs a two-node rig with GAN trust (PKI move plus `toggleApproval`, from `s3_redundancy.sh`). The backup is checked, never written.
**Delivers:** pair rig fixture (with sshd); backup checks (mode and effective view via backup's own API; SSH hash equal to master and render; redundancy flags; project hashes); bounded convergence loop; exactly one `resync` then content re-verify; sync-exhausted detection from backup logs; `deploy_backup_unsynced` verdict with no master rollback; backup designer/peer preflight; secret-name presence on both nodes; `verify --node both`; `failover` library method never called by a verb.
**Avoids:** P1 (wrong-order refusal), P9, P10, P23, P27 (failover 200 lies), P31 (TEC-5105 own slug).
**Captures:** C12; GAN trust recipe on 8.3.6. May overlap with 24 once the 23a run store exists.

### Phase 24: Guarded `deploy projects` (with `--check`)
**Rationale:** Needs manifest parsing and the 23a run store; nothing from pair work; can run parallel to 23b.
**Delivers:** release-zip pin verification before upload; `validate_import` promoted; topological order from `project.json` parent with cycle/unknown-parent refusal; hash no-op skip; pre-import export; `import?overwrite=true` in order; verification by canonical hash ignoring `lastModification*` and signatures; `removed`-member listing as destructive signal; unmanaged reported, never deleted; refuse `.git`-managed projects under `data/projects/`; Designer/session pre-check; `--check` read-only form.
**Avoids:** P29, P30, P31.
**Captures (blocking):** C8, C9 (S5 residual; do not claim in success criteria), C10.
**Cutover prerequisite (human, not code):** spec §13 step 8 (remove `.git`, uninstall git module) before unattended project deploys. Deploy never deletes `.git`.

### Phase Ordering Rationale
- 20 and 21 first, independent; every later phase needs one or both.
- Transport and plan before any write: render proven equal to prod with an empty plan first. `verify` ships with `plan` as a read-only cutover safety net.
- All write logic in 23a, built once; rollback reuses the apply engine.
- Pair split to 23b: needs its own fixture; backup failure modes must never trigger master rollback, so they come after the master path is trusted.
- Projects last: reuses existing machinery; needs only the run store.
- Surface parity (TUI rows, MCP markers, README and exit-table rows) lands with each verb, never as a final sweep (tui_coverage and guarded-ops drift tests fail otherwise).

### Research Flags
Needs research/capture: 20 (scan-lock wire, stale-signature status, `null` vs `""`); 21 (tag bytes, Embedded shape, name census, tag paths, uuid `sha1` check); 22 (Windows sftp, staging C6, timings); 23a (scan-lock vs UI writes, Windows mirror, single-scan ordering, fault-injection design); 23b (pair rig, GAN trust, C12); 24 (overwrite delete semantics, inheritance, prod git module).

Standard patterns (skip research-phase): CLI/TUI/MCP wiring (`workspace` and `testing run` precedents); Three-Place slug rule; `poll.rs` reuse; `classify.rs` arms; guarded gate shape (`workspace_push`); Gson writer once the corpus is committed; proptest writer idempotence.

## Spec Flags (SF-1 to SF-10)

| # | Flag | Recommendation | Phase |
|---|---|---|---|
| SF-1 | Frozen failure envelope has no `data` | Keep frozen. Pre-write refusals = CoreError. Post-write verdicts carry `data`, exit 6. Always write `result.json`. Top-level `ok` is Decision A. | 20, 23a |
| SF-2 | "Changes present" exit code (TF 2, kubectl 1; ign 1 internal, 2 usage) | `plan` exits 0; `data.summary.has_changes`. `--detailed-exitcode` deferred. | 22 |
| SF-3 | Spec §6.4.6 has apply run the rolling restart; milestone excludes it | Apply reports `restart_pending[]`, exit 0, prominent; documented as no-restart. | 23a, 23b |
| SF-4 | Spec §6.4.1/§14 "no open drift" vs `ign drift` deferred | `plan` shows divergence; optional `--base DIR` three-way; apply refuses on drift unless `--allow-drift "<reason>"` recorded in `run.json`. | 22 (base), 23a (gate) |
| SF-5 | §13.5 strip `core` and §13.2 bootstrap have no command | Plan flags shadowing. Later: guarded `apply --strip-core` with snapshot, or a scripted one-off. Bootstrap separate. | Later (v1.6.x) |
| SF-6 | §6.4.2 `.gwbk` default on, heavy on prod | Keep default-on; recorded `--skip-gwbk`; never upload `.gwbk` as an artifact. | 23a |
| SF-7 | Overlay is add/override only; no "absent in test" | Accept; document. | 21 |
| SF-8 | Probe list for "OPC/MQTT tags Good" missing from schema | Optional `verify.tag_probes[]` and `verify.timeouts`; absent = warning, not pass. | 21 (schema), 22 |
| SF-9 | TUI route per clap leaf; OutOfBand closed; guard markers | Six rows to `Screen::Projects`; `deploy_rows_cover_the_family`; `// guarded:` markers in `main.rs`; carry forward "cockpit deploy panel". | Each verb's phase |
| SF-10 | `targets/<env>.yaml` vs ign profiles | Target names master/backup profiles plus `ssh {host, port, user, key_file|key_env, os, data_dir}` and restart/failover method. Secrets stay in the profile chain. | 21 (schema), 22 |

Additions beyond FEATURES: spec §4 sorted-keys correction (Conflict 4); spec §9 write scope vs staging (Conflict 2, Decision B); spec §13 step 8 cutover prerequisite; spec §6 `holdTimeout` formula correct but the 5 s value is from local `docker cp` and must be measured, floored, ceilinged and printed.

## Capture-first backlog (deduplicated, one phase each)

| ID | Question | Source | Phase |
|---|---|---|---|
| C1 | Scan-lock 409 body; GET held (`remaining`, `actor`) vs expired (204); any release/DELETE route (grep 617-route OpenAPI); scan/config status JSON | PITFALLS C1; ARCH OQ2, §5.2 | 20 |
| C3a | Stale/omitted signature PUT: 422 or 500 "signature mismatch"; message arm only if distinguishable | ARCH OQ3; PITFALLS C3 | 20 |
| C3b | Stale/omitted `lastModificationSignature`: loads, rewritten, or flagged? Does the gateway rewrite a hand-written `resource.json` after load? | PITFALLS C3, P26; STACK flag 2 | 21 |
| C4 | Named (uuid) vs singleton types; census from prod core bootstrap | PITFALLS C4; FEATURES 3.1 | 21 |
| C5 | Hostile name encoding in tag/resource dirs (`:`, `?`, trailing dot, case collision, spaces, commas); WHK01 637 tag files | PITFALLS C5, P15 | 21 |
| OQ1 | Gateway-written bytes for `tags.json`, `udts.json`, `unary-resource.json` (key order, indent, newline, escapes, numbers); spike fixtures are synthetic | ARCH OQ1; STACK flag 1 | 21 |
| OQ4 | Repo-path → collection-path mapping for tag-definition nesting and name depth | ARCH OQ4 | 21 |
| OQ8 | Embedded vs Referenced JSON shape for secret fields, exact 8.3.6 form | FEATURES 3.1; ARCH OQ8; PITFALLS 11 | 21 |
| C6 | Can `svc-ignition-deploy` write a home-relative staging dir outside `data/config/`? Is it read as a collection? Blocks 23a | PITFALLS C6; ARCH §4.4 | 22 |
| C7a | Win32-OpenSSH sftp path form (`/C:/` vs `C:/`); quoting; key ACL; stdin-NUL hang; CLIXML on stderr; sftp-server long paths | STACK §1; ARCH OQ5; PITFALLS 15, 19 | 22 |
| OQ5 | Real-transport write/scan timings on WFLWVM01 with a prod-sized tree; derive `holdTimeout` (p95) and `margin`; Defender exclusion decision | ARCH OQ5, OQ10; PITFALLS 4, 17 | 22 |
| C2 | Does the scan-lock block UI, Designer and API writes to the active mode collection? | PITFALLS C2 | 23a |
| C7b | ACL after copy vs rename; sharing violation on loaded JDBC jar in `ignition/database-driver/`; Defender cost; long paths | PITFALLS C7, P16, P17 | 23a |
| C11 | Single-scan ordering when a provider and its definitions arrive together | PITFALLS C11 | 23a |
| C12 | Does a lagging backup recover via `resync` alone after "failed 5 times"? | PITFALLS C12 | 23b |
| GAN | GAN trust recipe on 8.3.6 (certificate move, `toggleApproval`) for the pair fixture | ARCH §9; PITFALLS 32 | 23b |
| C8 | Does `overwrite=true` delete resources absent from the zip? Does export-after-import equal the zip after normalizing `lastModification*`? | PITFALLS C8 | 24 |
| C9 | Child reads inherited resource after parent overwrite; session impact | PITFALLS C9; ARCH OQ9 (S5) | 24 |
| C10 | Prod git module `git.yaml`/commissioning: can it recreate stripped `core` or revert API imports? | PITFALLS C10, P30 | 24 |

Already resolved by existing evidence (no capture): scan/lock shapes from S1–S4 (completion rule confirmed under C1); mode-not-active behavior (S3, RIG.md); core shadowing granularity (S1, S2, S2b); 183-file corpus for passthrough and key order.

## Decisions Required (before planning Phases 22 and 23a)

- **Decision A — verdict envelope shape.** Post-write red outcomes: success-shaped with exit 6 (ARCH §7.2, testing-run precedent) vs `ok:false` (PITFALLS 23, 28). Recommendation: keep the frozen shape (`ok:true` plus `data.ok=false`, slug in `data`, exit 6), documented as a dated README exception.
- **Decision B — staging location and `svc-ignition-deploy` write scope.** (i) home-relative staging outside `data/config/` (requires a spec §9 amendment), or (ii) in-place mirror with the lock covering the network transfer (longer `holdTimeout`). Recommendation: (i) if C6 confirms write access and no collection-read risk; otherwise (ii).
- **Decision C — plan-hash handshake.** Require `--plan-sha256` for CI and agent callers (recommended); optional interactively.
- **Decision D — MCP exposure.** Keep marker-based confirm, or exclude `apply`/`rollback`/`projects` from the unattended catalog until Decision C is implemented. Recommendation: exclude until C is implemented.

## Confidence Assessment

| Area | Confidence | Notes |
|---|---|---|
| Stack | HIGH on versions, MSRVs, rejections (crates.io 2026-10-10; compiled on 1.88). MEDIUM-LOW on Windows OpenSSH behaviors (single-source). | Each is a Phase 22 capture. |
| Features | HIGH on spec contract and reuse (source read). HIGH on Terraform/kubectl (docs). MEDIUM on Helm/Ansible/Stoker. LOW on community CI/CD. | Spec and spikes are binding. |
| Architecture | HIGH on integration points (every seam read). MEDIUM on gateway wire (spec/spikes). LOW-MEDIUM on Windows SSH transport. | Least-proven area is transport. |
| Pitfalls | MEDIUM-HIGH. HIGH for spike-proven items. MEDIUM for Windows platform items, tagged [INFERRED] with captures. | OPEN items are not claimable in success criteria until captured. |

**Overall: MEDIUM.** Crate and contract decisions are solid. The riskiest unknowns (Windows transport timings, tag-file bytes, scan-lock vs UI writes, signature behavior, overwrite-import deletion) gate Phases 21, 22, 23a and 24.

### Gaps to Address
- **Staging location (Decision B, C6):** resolve in Phase 22 before 23a design.
- **`holdTimeout`/`margin`:** unmeasured on the target transport. Measured p95, floored, ceilinged, printed by `plan`. Do not ship 5 s.
- **Tag-file bytes and Embedded shape (OQ1, OQ8):** capture a real tag directory before committing tag policy.
- **Stale-signature behavior (C3a, C3b):** signatures cannot be regenerated (algorithm unpublished); capture before render's `lastModification` policy freezes.
- **uuid derivation (Conflict 5):** check `cargo tree` for `sha1` in Phase 21.
- **Scan-lock vs UI writes (C2):** if not blocked, the designers preflight becomes hard and the Pitfall 25 race is live.
- **S5 inheritance (C9):** not claimable until captured.
- **Prod git module (C10):** Phase 24 refuses `.git`-managed projects; the cutover itself is human work.
- **TEC-5105:** the first prod project deploy may fail backup verification for reasons unrelated to the deploy; separate slug; record per-node state.
- **Repo-side fixes outside this repo:** CRLF (`gateways/** -text` in `whk-environment-orchestration/.gitattributes`); runner group restriction and `runs-on: [self-hosted, Windows, onprem-deploy]`; `jq`/PATH on WFLWVM01. Cutover checklist items, not ign code.
- **Dot-directory hazard:** Ignition 8.3.3 Designer throws `NameInvalidException` on dot-named entries; no `.ign-deploy`, `.git` or state files under `resources/`.

## Sources

**Primary (HIGH):** design spec `2026-10-09-prod-gateway-config-as-code-design.md` (D1–D13, §3–§15); spike results `spikes/gateway-modes/results/{S1,S3,S4,S5,SUMMARY,RIG}.md` and scripts; ignition-cli source at cited symbols (`error.rs`, `session.rs`, `poll.rs`, `client/*`, `actions/*`, `rig/compose.rs`, `cli.rs`, `main.rs`, `mcp.rs`, `tests/tui_coverage.rs`, `ignition-tui/src/routes.rs`); gateway corpus `gw-config/` (183 JSON files); `.planning/PROJECT.md`, `WINDOWS.md`; crates.io API read 2026-10-10; Terraform plan docs.

**Secondary (MEDIUM):** kubectl ApplySet blog; Helm `--atomic` and Helm 4 `--rollback-on-failure`; Stoker operator repo; IA 8.3 version-control guide and deployment-modes docs; IA forum threads on deployment-mode UUIDs (114966) and resource.json churn (111329); IA SDK `LastModification` javadoc; Context7 serde-saphyr and russh notes; `openssh` crate docs; Ansible `--check --diff`.

**Tertiary (LOW, validate):** Win32-OpenSSH stdin-NUL hang (openssh-bugs 2019-02); libssh2 WinCNG ed25519 gap; Windows platform behaviors (NTFS ACL on move, robocopy defaults and bitmask, Java file sharing, MAX_PATH, OpenSSH key ACL), all [INFERRED] with captures; community Ignition CI/CD threads; RUSTSEC-2025-0068.

---
*Research completed: 2026-10-10. Ready for roadmap once Decisions A–D are recorded. Phases 20 and 21 can be planned now; 22 and 23a need Decisions B and C.*
