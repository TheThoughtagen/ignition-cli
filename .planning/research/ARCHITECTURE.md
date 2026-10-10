# Architecture Research: ignition-cli v1.6 "Declared Gateway Deploy"

**Domain:** `ign deploy {render,plan,apply,rollback,verify,projects}` added to the existing 3-crate Rust CLI/TUI workspace (Ignition 8.3+ gateway cockpit)
**Researched:** 2026-10-10
**Mode:** Subsequent milestone, architecture integration only
**Confidence:** HIGH on integration points (every seam below was read in source at the cited path/symbol). MEDIUM on gateway wire behaviour (spec + spikes only; this repo's capture-first rule applies). LOW-MEDIUM on the SSH transport against Windows OpenSSH (nothing in either repo has exercised it).

**Inputs read:** `.planning/PROJECT.md`; the design spec `whk-environment-orchestration/gateway-config-spec/docs/superpowers/specs/2026-10-09-prod-gateway-config-as-code-design.md` (§3, §5, §6, §8, §11, §12, §13, §15); spike scripts `spikes/gateway-modes/{lib.sh,s1_config_precedence.sh,s3_redundancy.sh}` plus `results/{S4,S5,SUMMARY}.md`; ignition-cli source (`error.rs`, `session.rs`, `poll.rs`, `client/{mod,classify,resources,redundancy,restart,projects,workspace}.rs`, `actions/{mod,projects,workspace,redundancy}.rs`, `cli.rs`, `main.rs`, `mcp.rs`, `tests/tui_coverage.rs`, `ignition-tui/src/routes.rs`, `rig/compose.rs`).

---

## 1. Verdict in one screen

| Question | Answer |
|---|---|
| New code home | `ignition-core/src/deploy/` (pure domain: manifest, target, layout, render, Gson writer, tree, plan, lock budget, run store, transport) + `ignition-core/src/actions/deploy.rs` (thin verbs) + `ignition-core/src/client/{config_resources,scan,mode,deploy_api}.rs` (wire). Same split as `rig/` + `actions/rig.rs`. |
| GatewayApi vs new trait | **New trait `DeployApi`** (`client/deploy_api.rs`), implemented by `ReqwestGatewayApi`. Do NOT grow `GatewayApi`: it has 15 hand-written `unreachable!()` test doubles (verified: `actions/{tags,rig,connections,diagnostics,version,doctor,logs x2,projects,webdev,sessions,script,apicall,inspect x2}.rs`), so every new method there is 15 edits. A separate trait needs zero. It also lets us write ONE stateful in-memory fake gateway, which is what the lock-expiry and auto-rollback tests actually need. |
| `MemberSource` reuse | **Zip arm only, and only for `deploy projects` hash verification.** The Tree arm is NOT reusable for the config tree (§4.2). |
| `deploy projects` | Composition over existing seams: `project_import(overwrite=true)`, `export_zip_bytes`, `diff_members`, `projects()` for inheritance. Almost no new client code. |
| Redundancy / backup verification | Reuse `GatewayApi::redundancy_status` as-is on the backup `Session`. New REST is only `resync`, `failover` (library), `restart-tasks/pending`, and `deploymentMode`. |
| Guarded gate | One gate inside the core action (the `workspace_push` precedent), after the read-only plan, before any write. Refusal message IS the rendered plan. |
| Red verdicts | `deploy verify` mismatch, and `deploy apply` that auto-rolled back or left the backup stale, use the **`ign testing run` precedent**: success-shaped envelope carrying full data, exit 6 decided in `main` afterwards. NOT new CoreError variants (the error envelope cannot carry data). |
| TUI | Six rows in `routes()` mapped to `Screen::Projects`, exactly like the `workspace` family (13-07). No new Screen, no OutOfBand change (that set is pinned and closed). |
| MCP | Automatic: catalog derives from the clap tree. Add 3 `GUARDED_OPS` entries (`deploy apply`, `deploy rollback`, `deploy projects`) so each gets the synthetic `confirm` property. |
| New CoreError variants | 10, all at EXISTING exit codes (2, 4, 6). Box any variant with 3+ Strings (clippy `result_large_err`, WINDOWS.md entry 3). |
| New dependency | One: a maintained serde YAML **deserializer** for `manifest.yaml` / `targets/*.yaml` (no YAML writing needed; render emits JSON). Candidate `serde-saphyr` (typed-deserialize only, no `unsafe`); confirm at Phase 21 planning. No SSH crate (shell out, §4.4). |
| Phase order | Keep 20-24 numbering, but **move `SshTransport` and `deploy verify` into Phase 22 with `plan`**, and **split 23** (§9). |

---

## 2. System overview (what exists, where deploy plugs in)

```
┌────────────────────────────── ign (single binary) ──────────────────────────────┐
│ cli.rs  Commands::Deploy(DeployArgs) ── NEW (6 leaves)                          │
│ main.rs dispatch → dispatch_deploy ──► actions::deploy::*    ActionOutput::Deploy* NEW (6)
│         GUARDED_OPS += 3, `// guarded:` markers, failure_exit_code() verdicts   │
│ render.rs render_deploy_*_human  NEW                                            │
│ mcp.rs  (no code change: catalog derives from clap tree, confirm from GUARDED_OPS)
│ ignition-tui routes.rs +6 rows (Screen::Projects)                               │
├──────────────────────────────────────────────────────────────────────────────────┤
│ ignition-core                                                                    │
│                                                                                  │
│  actions/deploy.rs  (render, plan, apply, rollback, verify, projects)            │
│        │  orchestration only; NEVER prints; progress via DeploySink callback     │
│        ▼                                                                         │
│  deploy/  (NEW pure domain, no network)         client/ (NEW wire files)         │
│   manifest.rs  target.rs  layout.rs              config_resources.rs             │
│   render.rs  gson.rs  secret_lint.rs             scan.rs   mode.rs               │
│   tree.rs (RenderedTree, sha256)                 deploy_api.rs  trait DeployApi  │
│   plan.rs (diff, flags, plan_id)                   └─ impl for ReqwestGatewayApi │
│   lock.rs (budget + ScanReady witness)                (reuses its PRIVATE helpers│
│   run_store.rs (snapshot, run dir)                     send_and_classify, get_json│
│   events.rs (DeployEvent, DeploySink)                  post_json, put_json,       │
│   transport/ { mod.rs trait FileTransport,             download_to_file)          │
│                local.rs, sftp.rs, runner.rs }                                    │
│        │                                              │                          │
│        │ REUSED, unchanged:                           ▼                          │
│        │  session.rs  Session::resolve_side          classify.rs (+1 arm)        │
│        │  poll.rs     poll(PollConfig, probe)                                    │
│        │  client::GatewayApi: project_import, project_export_to_file, projects,  │
│        │      designers, redundancy_status, status_ping, backup_download         │
│        │  client::resources: diff_members, normalize_descriptor, project_meta_delta
│        │  actions::resources::export_zip_bytes                                   │
│        ▼                                                                         │
│  error.rs  +10 variants   output.rs envelope (unchanged)                         │
└──────────────────────────────────────────────────────────────────────────────────┘
        │ HTTPS /data/api/v1/{scan-lock,scan,mode,resources,...}     │ ssh/sftp
        ▼                                                            ▼
   master gateway (WIGNVM01)  ◄── redundancy sync ──►  backup gateway (WIGNVM04)
```

Two handles flow through every deploy action, not one: `api: &dyn GatewayApi` (existing capability set) and `deploy: &dyn DeployApi` (new). `ReqwestGatewayApi` implements both, so the CLI passes `&*session` twice. Tests pass different doubles for each.

---

## 3. New vs modified (explicit)

### 3.1 NEW files

| Path | Contents |
|---|---|
| `crates/ignition-core/src/client/config_resources.rs` | Wire models + path builders for `/data/api/v1/resources/{find,list,names}/<module>/<type>[/<name>]` with `collection` param, and `PUT /resources/<module>/<type>` (array body `{name, signature, config}`). `encode_segment` reused from `client/projects.rs` (it is `pub(crate)`). **Not** `client/resources.rs`: that file is the project-export zip surgery and the name collision would be a trap. |
| `crates/ignition-core/src/client/scan.rs` | `ScanLockKind {Config, Projects}`, `ScanLockInfo {timestamp, remaining, actor}`, `ScanStatus {scan_active, last_scan_timestamp, last_scan_duration}`, path consts, body type `{acquireTimeout, holdTimeout}` (seconds). |
| `crates/ignition-core/src/client/mode.rs` | `ModeRecord`, create body `{name,title,description}`, `deployment_mode` extraction from gateway-info (own tiny wire struct; see §5.3), path consts. |
| `crates/ignition-core/src/client/deploy_api.rs` | `#[async_trait] pub trait DeployApi: Send + Sync` + the one `impl DeployApi for ReqwestGatewayApi` block (child module of `client`, so it may call that type's private helpers). |
| `crates/ignition-core/src/deploy/mod.rs` | Module map + doc. Registered in `lib.rs` next to `rig`, `module`, `webdev`. |
| `deploy/manifest.rs` | `Manifest {standards_version, types: allowlist, tag_providers, projects: [ProjectPin{name, version, sha256, artifact}]}`. YAML in, typed struct. |
| `deploy/target.rs` | `Target {gateway, mode, nodes: {master, backup?}, os, data_dir, transport, restart}`. Each node carries an **ign profile name** (credentials stay in the one secret chain) plus `ssh {host, port, user, key_file\|key_env}`. |
| `deploy/layout.rs` | THE path mapping: repo path → collection-relative path, and path → `ResourceKey{module,type,name}`. Single source (the `member_path` precedent in `client/resources.rs`). Always `/`-separated; OS separators exist only inside transports. |
| `deploy/gson.rs` | Gson-compatible JSON writer = a custom `serde_json::ser::Formatter` (sorted keys, Ignition's `\u00xx` escaping of `= ' < > &`, Gson's pretty layout). Round-trip contract: gateway-written bytes → parse → write → identical bytes. |
| `deploy/render.rs` | `render(repo, manifest, env) -> RenderedTree`: overlay `modes/<env>/` onto `resources/` + `tags/` by path, allowlist enforcement, uuid minting for NEW resources (`attributes.uuid`), fail-closed case-fold collision check. |
| `deploy/secret_lint.rs` | Embedded-secret detection over the rendered tree. |
| `deploy/tree.rs` | `RenderedTree {files: BTreeMap<RelPath, Vec<u8>>}`, `TreeDigest` (sha256 per file + rollup). SHA-256 (already a core dep via `sha2`). |
| `deploy/plan.rs` | `DeployPlan`, `PlannedChange`, `PlanFlag`, the diff function, `plan_id`. Pure: takes `(RenderedTree, ObservedState)`. |
| `deploy/lock.rs` | `HeldLock`, `ScanReady`, the pure expiry decision function (§6.3). |
| `deploy/run_store.rs` | Run directory: `plan.json`, `snapshot/collection/**`, `snapshot/projects/*.zip`, optional `snapshot/backup.gwbk`, `events.ndjson`, `result.json`. Rollback reads from here. |
| `deploy/events.rs` | `DeployEvent`, `trait DeploySink` (the `run_streaming` line-sink precedent). |
| `deploy/transport/{mod,local,sftp,runner}.rs` | `FileTransport` trait, `LocalFsTransport`, `SftpTransport`, `ProcessRunner` seam (§4.4). |
| `deploy/projects.rs` | Pin verification (sha256 of release zip), parents-first ordering from each zip's `project.json`. |
| `crates/ignition-core/src/actions/deploy.rs` | `deploy_render`, `deploy_plan`, `deploy_apply`, `deploy_rollback`, `deploy_verify`, `deploy_projects` + their result models. |
| `crates/ignition-core/tests/deploy_client_contract.rs` | wiremock pins of the spike wire (§10 L2). |
| `crates/ignition-core/tests/deploy_{render,plan,apply}.rs` | tempdir + fake-gateway suites (§10 L1/L3). |
| `crates/ignition-core/tests/fixtures/deploy/` | Gateway-WRITTEN `resource.json`/`config.json`/`tags.json` captures (Phase 21 capture-first). |
| `crates/ignition-cli/tests/contract_deploy.rs` | assert_cmd + snapbox goldens + wiremock (§10 L4). |
| `crates/ignition-cli/tests/e2e_deploy.rs` | `#[ignore]` live gates (§10 L6). |
| `crates/ignition-core/tests/fixtures/live-rig-pair/compose.yml` | Two-gateway pair + sshd, ported from `spikes/gateway-modes/docker-compose.yml`. Needed by Phase 23b. |
| `.planning/phases/2x-*/2x-LIVE-CAPTURES.md` | Wire truth per phase, recorded before model code (v1.1 lesson). |

### 3.2 MODIFIED files

| Path / symbol | Change |
|---|---|
| `ignition-core/src/lib.rs` | `pub mod deploy;` + module-map doc line. |
| `ignition-core/src/actions/mod.rs` | `pub mod deploy;`. |
| `ignition-core/src/client/mod.rs` | Four `pub mod` lines (`config_resources`, `scan`, `mode`, `deploy_api`). **No `GatewayApi` change, so none of the 15 doubles are touched.** |
| `ignition-core/src/client/classify.rs` | One path-scoped arm: `409` on `/data/api/v1/scan-lock/` → `ScanLockHeld` (same shape as the existing `is_eam_force_url` 409 arm). The existing `422`-on-`/data/api/v1/resources/` arm (`is_config_resource_url`) already maps to `InvalidInput`; Phase 20 captures whether a stale-signature PUT is 422 or 500 (the v1.1 EAM capture saw `500` + "signature mismatch" on DELETE) and adds a message-classified arm only if it is distinguishable. |
| `ignition-core/src/error.rs` | +10 variants (§7): `code()`, `exit_code()`, `hint()`, `endpoint()`, the module-header exit table, the `exit_code_mapping_enumerated` case list. |
| `ignition-core/src/actions/projects.rs` | Promote private `fn validate_import` (line 144) to `pub(crate)` so `deploy projects` runs the identical pre-upload structural check (a truncated zip wipes the target; live-witnessed in v1.0). |
| `ignition-core/Cargo.toml` + workspace `Cargo.toml` | YAML deserializer dep. Nothing else. |
| `ignition-cli/src/cli.rs` | `Commands::Deploy(DeployArgs)`, `DeployCommand {Render, Plan, Apply, Rollback, Verify, Projects}`. |
| `ignition-cli/src/main.rs` | 6 `ActionOutput::Deploy*` variants (2 exhaustive matches: JSON arm ~line 428 and `render.rs` ~353), `dispatch_deploy`, `GUARDED_OPS` +3 entries and 3 `// guarded:` markers **in main.rs** (the drift test does `include_str!("main.rs")` only; keep markers there), verdict exit hook next to `TestingRun`'s `failure_exit_code()` (~line 523). |
| `ignition-cli/src/render.rs` | Six human renderers. |
| `ignition-tui/src/routes.rs` | 6 rows → `Mapping::Screen(Screen::Projects)`. |
| `ignition-cli/tests/tui_coverage.rs` | `deploy_rows_cover_the_family` (clone of `workspace_rows_cover_the_family`). The Dashboard count pin (41) is untouched because no row maps to Dashboard. |
| `README.md` | Commands table rows, exit-code table rows (the `readme_exit_table_agreement` test parses it), a deploy section with the verdict-exit exceptions. |

### 3.3 Explicitly NOT modified

`GatewayApi` and its 15 doubles; `GatewayInfo` (it serialises into `ign version/status --json`; adding a field changes a frozen envelope); `MemberSource`; the WebDev route bundle (deploy uses no WebDev route, so `ROUTE_BUNDLE_VERSION` stays); the output envelope; the exit-code numbering; `EXCLUDED_LEAVES` in `mcp.rs`; `Session` (no widening needed).

---

## 4. Seam-by-seam decisions

### 4.1 Reuse map

| Existing seam | Verdict | How deploy uses it |
|---|---|---|
| `Session::resolve_side(&mut Config, name)` + `main.rs::resolve_two_clients` | **Reuse** | Target names two ign profiles (master, backup). Credentials stay in `config::resolve_secret` (env token_env → keyring), preserving the grep-auditable "zero second-source client construction" invariant. CI sets `token_env` per profile. |
| `ReqwestGatewayApi` private helpers (`send_and_classify`, `get_json`, `post_json`, `put_json`, `post_empty`, `download_to_file`, `apply_auth`) | **Reuse** | New `client/*.rs` are child modules of `client`, so private items are in scope. Zero second HTTP pipeline. |
| `classify.rs` | **Extend by one arm** | See §3.2. |
| `poll::poll(PollConfig, probe)` | **Reuse** | Scan completion, backup convergence, post-restart readiness. It already retries `Network`/`GatewayRestarting`, fails fast on `Auth`, and clamps backoff. |
| `GatewayApi::project_import(name, zip, overwrite)` | **Reuse** | Includes the denial-rides-200 check (`ImportDenied`). Do not re-check per caller. |
| `GatewayApi::project_export_to_file` + `actions::resources::export_zip_bytes` | **Reuse** | Snapshot and post-import verify. |
| `client::resources::{diff_members, project_meta_delta, normalize_descriptor}` | **Reuse** | Project verification (post-import export vs release zip) and descriptor normalisation for config `resource.json` (strips `attributes.lastModification` / `lastModificationSignature`; sorted keys). |
| `GatewayApi::redundancy_status()` → `RedundancyStatusWire {role, project_state, activity_level, peer_connected, sync_pending, failover_pending, last_sync_timestamp}` | **Reuse unchanged** | Preflight ("master Active, peer connected, `sync_pending=false`, `project_state=Good`") and backup convergence. Called on the backup's `Session`. |
| `GatewayApi::designers()` | **Reuse** | Preflight "no open Designer sessions". |
| `GatewayApi::backup_download` | **Reuse** | Optional `.gwbk` snapshot (spec §6 step 2). 300 s class already wired. |
| `GatewayApi::status_ping` | **Reuse** | "node RUNNING" probes. |
| `rig/compose.rs` `ComposeRunner` | **Pattern only** | Its `run()` hard-prefixes `docker compose`. Write a sibling `ProcessRunner` (program + args + optional stdin → code/stdout/stderr, spawn failure = 127) rather than bending it. |
| `rig` live harness + `tests/fixtures/live-rig/compose.yml` | **Reuse pattern** | Phase 20-22 live gates use a single ephemeral 8.3.6; 23b needs the pair fixture. |
| `module/fetch.rs` sha256 streaming | **Pattern only** | `sha2` is already a core dep. Pinned-digest refusal copies `ModuleDigestMismatch`'s shape. |
| `actions::workspace::workspace_push` gate | **Pattern** | The gate-in-core precedent (§6.1). |
| `ign testing run` verdict exit | **Pattern** | Red verdict as success envelope (§7.2). |
| `MemberSource::Zip` | **Reuse (projects only)** | `MemberSource::Zip(release).member_hashes()` vs `Zip(exported).member_hashes()`. |
| `MemberSource::Tree` | **Do NOT reuse** | §4.2. |

### 4.2 Why `MemberSource::Tree` does not fit the config tree

Verified in `client/workspace.rs`: Tree is **manifest-scoped** (mapping from the recorded `.ign-workspace.json`; unmapped local files are an error), addresses **project-export members** (`<collection>/resources/<rest>`), hashes with **FNV-1a** (non-cryptographic, built for change detection), and normalises only descriptors named `resource.json`. The deploy tree is a different shape (`<module>/<type>/<name>/{config.json,resource.json}` plus tag trees), needs **cryptographic** hashes (the spec's "file-hash comparison is empty" is a security-relevant equality across a transport), and has no checkout manifest. Forcing it through Tree would mean faking a manifest and downgrading the hash. Build `RenderedTree` + `TreeDigest` (sha256) and reuse only `normalize_descriptor` for volatile-attribute stripping.

### 4.3 `deploy projects` reuse (Phase 24)

```
pins (manifest) + artifacts dir
 1 sha256(release zip) == pin           → else ProjectHashMismatch (pre-upload, exit 6)
 2 validate_import(zip)                 (promoted pub(crate))
 3 order: parse project.json parent of each zip; parents first; cycle/unknown parent → DeployTreeInvalid
 4 snapshot: export_zip_bytes(master, p) → run_dir/snapshot/projects/p.zip  (shared run_store with apply)
 5 gate (preview = per-project: pinned version, live-vs-release diff_members summary)
 6 project_import(master, p, zip, overwrite=true)   in order; S5: never re-import a child after its parent
 7 verify: export again; diff_members(release, exported) empty and project_meta_delta empty
 8 backup (if node present): export on backup, diff_members vs master export (23b logic reused)
 rollback: re-import snapshot zips (spec §8: "Project: re-import the exported pre-deploy zip")
```
`deploy projects --check` is the read-only form (steps 1, 3, 7 against the live gateway; exit 0 with data). The pins are consumed from the manifest parsed in Phase 21; the zips come from a local `--artifacts DIR` (CI does the release download). **No network release feed in v1.6.**

### 4.4 File transport seam

```rust
#[async_trait]
pub trait FileTransport: Send + Sync {
    /// sha256 of every file under `root` (collection-relative, '/'-separated).
    async fn hash_tree(&self, root: &RemoteRoot) -> Result<TreeDigest, CoreError>;
    /// Pull `root` into a local directory (snapshot). Permissions/ownership not preserved.
    async fn read_tree(&self, root: &RemoteRoot, into: &Path) -> Result<TreeDigest, CoreError>;
    /// Make `root` equal `tree`: write adds/changes, delete extras. Returns what it did.
    async fn mirror_tree(&self, root: &RemoteRoot, tree: &RenderedTree) -> Result<MirrorReport, CoreError>;
}
```
Three coarse verbs (the `GatewayApi` "coarse on purpose" rule). Rollback is `mirror_tree(root, snapshot_as_tree)`, so no fourth verb.

| Impl | Purpose | Used by |
|---|---|---|
| `LocalFsTransport { root }` | Plain directory ops. Also fronts an ephemeral gateway whose `data/config/resources` is a bind mount. | Unit/fake tests, CI-ephemeral gateway, the rig live gates |
| `SftpTransport { ssh: SshTarget, os }` | Production. Shells out to the system `sftp -b -` (batch on stdin) via `ProcessRunner`. | Prod/test pair |
| (optional later) `DockerCpTransport` | The spike-proven `docker cp` + `chown 2003:2003` path | Only if a Rust live gate cannot use a bind mount |

**Why `sftp` batch, not tar-over-ssh and not an SSH crate.**
1. No crate: `russh` + `russh-sftp` is a large async/crypto tree; `ring` is already here but the rest is not, and the project's stated posture is a lean tree.
2. Uses the operator's ssh-agent, `known_hosts`, key file: exactly the "key-only, scoped `svc-ignition-deploy`" model in spec §9.
3. SFTP is OS-neutral. Windows OpenSSH ships `sftp-server`; tar-over-ssh would need per-OS remote command dialects (PowerShell vs sh) for deletes and hashing, and spec §5.4 wants "everything OS-specific" confined to the target, not scattered through code.
4. Weakness to own: per-file latency for large trees. Mitigation is the spec's own: S4 numbers were local `docker cp`; `holdTimeout = 2×(write+scan)+1` **must be measured on the real transport** (Phase 22 capture). If thousands of tag files blow the budget, add a tar-stream implementation behind the same trait; nothing above the trait changes.

`hash_tree` over SFTP means pulling bytes and hashing locally (no remote shell). That is slower than a remote `sha256sum` but dialect-free and trustworthy (we never trust a remote tool's output). Plan and verify both need it, so Phase 22 owns it.

**Staging constraint:** the spec scopes `svc-ignition-deploy` write access to `data/config/resources/<mode>` only, and the gateway treats every directory under `resources/` as a collection. So there is NO staging directory next to the collection; `mirror_tree` applies in place and the lock + snapshot are the safety net. (An atomic swap is not available under that account.)

---

## 5. Gateway client (Phase 20)

### 5.1 `DeployApi`

```rust
#[async_trait]
pub trait DeployApi: Send + Sync {
    // config resources
    async fn resource_find(&self, module: &str, ty: &str, name: &str, collection: Option<&str>) -> Result<ResourceRecord, CoreError>;
    async fn resource_list(&self, module: &str, ty: &str, collection: Option<&str>) -> Result<Vec<ResourceRecord>, CoreError>;
    async fn resource_put(&self, module: &str, ty: &str, items: &[ResourcePut]) -> Result<(), CoreError>; // array body, signature required
    // scan-lock + scan
    async fn scan_lock_acquire(&self, kind: ScanLockKind, acquire_s: u64, hold_s: u64) -> Result<(), CoreError>;   // 200; 409 -> ScanLockHeld
    async fn scan_lock_info(&self, kind: ScanLockKind) -> Result<Option<ScanLockInfo>, CoreError>;                 // 204 -> None
    async fn scan_start(&self, kind: ScanLockKind) -> Result<(), CoreError>;                                       // POST; applies AND releases the lock
    async fn scan_status(&self, kind: ScanLockKind) -> Result<ScanStatus, CoreError>;
    // modes
    async fn modes(&self) -> Result<Vec<ModeRecord>, CoreError>;
    async fn mode_create(&self, body: &ModeCreate) -> Result<(), CoreError>;
    async fn deployment_mode(&self) -> Result<String, CoreError>;      // gateway-info.deploymentMode; "" = none active
    // pair operations
    async fn restart_tasks_pending(&self) -> Result<Vec<serde_json::Value>, CoreError>;
    async fn redundancy_resync(&self) -> Result<(), CoreError>;
    async fn redundancy_failover(&self) -> Result<(), CoreError>;       // library only in v1.6; rolling restart is out of scope
}
```

### 5.2 Wire facts from the spikes (each becomes a wiremock pin and a `20-LIVE-CAPTURES.md` row)

| Call | Verified shape (8.3.6, `lib.sh`, `s1`, `s3`, `S4.md`) |
|---|---|
| lock | `POST /data/api/v1/scan-lock/config` body `{"acquireTimeout":10,"holdTimeout":N}` (seconds), `Content-Type: application/json`, 200; 409 if held |
| lock info | `GET /data/api/v1/scan-lock/config` → `{timestamp, remaining, actor}` while held; **HTTP 204, empty body** after expiry |
| scan | `POST /data/api/v1/scan/config` → 200 (applies and releases). `GET` → `{scanActive, lastScanTimestamp, lastScanDuration}`. Completion = `scanActive==false && lastScanTimestamp > before` (capture `before` first) |
| find | `GET /resources/find/<module>/<type>/<name>[?collection=c]` → `{signature, config, …}` |
| PUT | `PUT /resources/<module>/<type>` body `[{name, signature, config}]` (array even for singletons; 422 if signature missing) |
| mode | `GET /mode` → `{items:[{name,…}]}`; `POST /mode {name,title,description}`; active mode only visible as `gateway-info.deploymentMode` |
| resync / failover | `POST /redundancy/gwaction/{resync,failover}`; failover answers **200 even when the backup is unreachable** ("Could not dispatch node responsibility request to backup", RIG.md): never treat the status as success; read `activityLevel` back |

### 5.3 Empty-body tolerance (a stated requirement)

The spike had to bypass `ign api call` with raw `curl` for writes because `ign api call` refuses 2xx with a non-JSON body (`lib.sh api_write`, `raw_get`). The new client must not repeat that: write calls use `send_and_classify(...).await.map(|_| ())` (classify-first, body ignored), and `scan_lock_info` reads `status()` before touching the body (204 → `Ok(None)`). `deployment_mode()` deserialises its own `struct { #[serde(default, rename="deploymentMode")] deployment_mode: String }` from the gateway-info path, so `GatewayInfo` (frozen, user-visible) stays untouched.

### 5.4 PUT is not the apply path

Decision D5 writes the whole collection as files in one scan-locked transaction. `resource_put` exists because the milestone requires it (and the CI-ephemeral bootstrap and the live captures use it), but `deploy apply` never calls it. Mixing REST writes with file writes would split one logical transaction across two mechanisms with different failure semantics. Keep it out of `actions/deploy.rs`.

---

## 6. Core data flow

### 6.1 Guarded gate: how apply previews

Established precedents: `preview_then_confirm` in `main.rs` is EAM-specific (takes a task name, fetches a blast radius); `workspace_push` is the generic pattern (status → conflicts refuse → selection → zero-write short-circuit → ONE `require_confirmation(yes, &preview)` → execute). Deploy follows the **workspace_push** shape, with the gate inside core so CLI, MCP bridge and any future TUI panel get identical behaviour:

```
deploy_apply(repo, env, opts{yes, allow_destructive, expect_plan, run_dir, ..}):
  1 load Target + Manifest                         (pure; InvalidInput/DeployTreeInvalid before any I/O)
  2 render + lint                                  (pure; EmbeddedSecret / DeployTreeInvalid)
  3 resolve Sessions (master, backup?)             (SecretUnavailable etc. as today)
  4 preflight  (read-only REST)                    → DeploymentModeMismatch | DeployPreflightFailed(all failing checks at once)
  5 observe + plan (read-only: transport.hash_tree + REST core-shadow reads)
  6 plan empty?  → return Applied{outcome:"noop"}  ZERO writes, no gate, no snapshot, no lock   (workspace_push step 4)
  7 plan.destructive && !allow_destructive → DestructiveChangeRefused   (NOT confirmable by --yes)
  8 expect_plan present && != plan_id → PlanStale
  9 !yes → ConfirmationRequired{ operation: render_plan_preview(&plan) }   exit 2; the message IS the plan
 ---- everything above is read-only; everything below writes ----
 10 snapshot → run_dir
 11 lock → write → witness → scan → poll → verify   (§6.2)
```
`--yes` is a GLOBAL flag and `apply_env_defaults` merges `IGNITION_YES`; over MCP the bridge sets `--yes` ONLY from `confirm: true`. Both behaviours are inherited free. Destructive deletes need `--allow-destructive` **in addition** to `--yes`, mirroring "conflicts refuse even with `--yes`" in workspace push, and spec §8's "`destructive` label + explicit approval".

`deploy plan` and `deploy verify` are read-only and take no gate. `deploy render` is local-only (no profile, envelope `profile: null`).

### 6.2 Apply state machine

```
snapshot ──► lock(holdTimeout) ──► mirror_tree ──► witness ──► scan ──► poll ──► verify ──► [backup verify] ──► done
   │             │ 409                │ err         │ expired     │err      │timeout     │ fail
   │             ▼                    ▼             ▼             ▼         ▼            ▼
   │        ScanLockHeld        ──────── abort path (7r) ────────────────────────────  auto-rollback
   ▼
 run_dir (plan.json, snapshot/**, events.ndjson, result.json) written BEFORE the first remote write
```

**Abort path (7r).** There is no documented release for a held scan lock; it ends by `scan` (applies and releases) or by expiry (applies **nothing**, S4). So a failed or partial write must never be left to expire: partial files sitting in the collection would be applied by the next scan from anyone. The abort path is: while the lock is still witnessed held, `mirror_tree(snapshot)` then scan (the diff against the gateway's loaded state is empty, so it is a no-op apply). If the lock is already lost, take a fresh lock and do the same. Auto-rollback after a failed verify is the same function (`restore_snapshot(run)`), and explicit `deploy rollback --run <id>` calls it too. One restore implementation, three entry points.

### 6.3 Lock-expiry detection (spec §6 step 4.3, S4)

Failure to guard: a scan after the lock expired applies *whatever was written so far*, with no signal. Encode the rule in types so it cannot be skipped:

```rust
pub struct HeldLock { kind: ScanLockKind, acquired_at: Instant, hold_timeout: Duration }
pub struct ScanReady(()); // only constructible by HeldLock::witness
impl HeldLock {
    pub async fn witness(self, api: &dyn DeployApi, clock: &dyn DeployClock, margin: Duration)
        -> Result<ScanReady, CoreError>   // ScanLockExpired if elapsed >= hold_timeout - margin
                                          // OR scan_lock_info() == None (204) OR remaining < margin
}
pub(crate) async fn scan_applying(api: &dyn DeployApi, ready: ScanReady) -> Result<(), CoreError>;
```
The pure decision `fn lock_verdict(elapsed, hold, margin, info: Option<&ScanLockInfo>) -> LockVerdict` carries all the logic and is table-tested + proptested; the async wrapper only fetches. `DeployClock` is a tiny seam (`now()`, `sleep()`) because the workspace tokio features do not include `test-util` (so no `start_paused`); `ManualClock` drives the tests. `holdTimeout` comes from the target (`2×(write+scan)+1`, measured), never a literal.

### 6.4 Data flow summary

```
repo (gateways/<g>/…, targets/<env>.yaml) ──parse──► Manifest + Target
Manifest + resources/ + tags/ + modes/<env>/ ──render──► RenderedTree (in memory, sha256 per file)
                                      │
                      ┌───────────────┴────────────────┐
   deploy render ◄────┘ (write to --out)               │
                                                       ▼
 live state ──► ObservedState{ transport.hash_tree(master root),
                                core-collection reads, deploymentMode, designers, redundancy, restart-tasks }
                                                       │
                                 plan(RenderedTree, ObservedState) ──► DeployPlan{plan_id, changes, flags}
                                                       │
   deploy plan ◄───────────────────────────────────────┤
   deploy verify = observe again after apply and compare to RenderedTree (no new logic)
   deploy apply  = plan → gate → snapshot → lock/mirror/witness/scan → verify → backup verify
```

---

## 7. Errors, exit codes, envelopes

### 7.1 New `CoreError` variants (all at existing exit codes)

Each needs the **five-place update**: variant + `code()`, `exit_code()`, `hint()`, `endpoint()` (when it has one), the `error.rs` module-header table, the README table, and a case in `exit_code_mapping_enumerated`.

| Slug | Exit | Raised when | Payload (box if 3+ Strings) |
|---|---|---|---|
| `deploy_tree_invalid` | 2 | manifest/target malformed, path outside the allowlist, invalid JSON, case-fold collision, project pin cycle/unknown parent | `reason` |
| `embedded_secret` | 2 | render lint found a secret value instead of a reference | `Vec<path>` (never the value) |
| `destructive_change_refused` | 2 | plan deletes tags/resources and `--allow-destructive` absent | counts + first N paths |
| `transport_failed` | 4 | ssh/sftp connect, auth, or remote command failure | host, stage, detail (stderr tail, capped) |
| `scan_lock_held` | 6 | 409 on scan-lock acquire (classify arm) | kind, endpoint |
| `scan_lock_expired` | 6 | `witness` failed: expiry or `204` before scan | kind, elapsed, hold |
| `deployment_mode_mismatch` | 6 | node's `deploymentMode` ≠ target mode (the silent failure: a node booted before the mode existed ignores the JVM arg, writes to the collection are NOT effective) | expected, actual per node |
| `deploy_preflight_failed` | 6 | one or more preflight checks failed (all reported together) | `Box<Vec<(check, detail)>>` |
| `project_hash_mismatch` | 6 | release zip sha256 ≠ manifest pin (pre-upload) | project, expected, actual |
| `rollback_failed` | 6 | the restore itself failed; gateway state unknown | run_id, run_dir, cause |
| `plan_stale` | 6 | `--expect-plan` ≠ recomputed plan (optional feature, §9 Phase 23) | expected, actual |

Total 11 if `plan_stale` ships; 10 without. Hints must name a command that actually fixes the case (the `RoutesNotDeployed` rule in `error.rs`); e.g. `deployment_mode_mismatch` hint = "the node ignores -Dignition.config.mode if the mode did not exist at boot: create the mode, confirm it reached the backup, then restart that node (spec §13 step 4)".

### 7.2 Verdicts, not errors (the `testing run` precedent)

`ErrorEnvelope` has no `data` field, so an error cannot carry the plan, steps, run id and mismatch list an agent needs. `main.rs` already has two "success envelope, exit decided afterwards" exceptions (`lint --strict`, `testing run`, ~lines 510-535). Deploy adds a third using the same hook:

| Result | Envelope | Exit |
|---|---|---|
| `apply` outcome `applied` / `noop` | ok, full data | 0 |
| `apply` outcome `rolled_back` (verify failed, auto-rollback succeeded) | ok, full data, `data.slug="deploy_verify_failed"` | **6** |
| `apply` outcome `applied_backup_stale` (master good, backup never converged after resync) | ok, full data, `data.slug="backup_out_of_sync"` | **6** |
| `verify` with mismatches | ok, `data.ok=false` + `mismatches[]` | **6** |
| `plan` | ok, data (`summary.empty` is the machine signal; no new exit semantic, taxonomy is frozen) | 0 |

The slugs ride the payload and are NOT CoreError variants, never in the exit tables (exactly the `tests_failed` rule). `rollback_failed` IS a CoreError because it means "stop, the gateway state is unknown" and there is nothing coherent to report as data except the run dir named in the message.

### 7.3 JSON envelope `data` shapes (additive, all keys always present)

```jsonc
// deploy render  (profile: null)
{"env":"production","gateway":"distillery01","mode":"production","out":"/tmp/r","files":1322,"bytes":2874112,
 "digest":"sha256:…","resources":[{"module":"ignition","type":"database-connection","name":"MES","files":2}],
 "tag_providers":["WHK01","default"],"warnings":[]}

// deploy plan
{"env":"production","mode":"production","nodes":{"master":{"profile":"wignvm01","deployment_mode":"production"},
 "backup":{"profile":"wignvm04","deployment_mode":"production"}},
 "render_digest":"sha256:…","base_digest":"sha256:…","plan_id":"sha256:…",
 "summary":{"add":3,"change":12,"delete":0,"same":1307,"destructive":0,"empty":false},
 "changes":[{"path":"ignition/database-connection/MES/config.json","resource":{"module":"ignition","type":"database-connection","name":"MES"},
             "kind":"change","class":"config","destructive":false,"flags":[]}],
 "flags":[{"code":"core_shadow","severity":"warn","path":"…","detail":"declared resource also exists in core with different content; mode wins"},
          {"code":"core_residue","severity":"block","path":"…","detail":"core-only folder shows through the mode collection (S2b); strip core at cutover"}]}

// deploy apply
{"env":"production","run_id":"20261010T184441Z-3fa9","run_dir":"…","outcome":"applied",
 "plan_id":"sha256:…","applied":{"add":3,"change":12,"delete":0},
 "lock":{"hold_timeout_s":11,"elapsed_ms":2310,"remaining_ms_at_scan":8120},
 "scan":{"duration_ms":925,"last_scan_timestamp":1791584841451},
 "snapshot":{"collection_files":1322,"projects":["WHK-Global"],"gwbk":"…/backup.gwbk"},
 "verify":{"resources_checked":150,"file_hash_mismatches":0,"tag_probe":{"sampled":20,"good":20}},
 "backup":{"converged":true,"waited_s":4,"resynced":false},
 "restart_pending":[],"steps":[{"name":"snapshot","status":"ok","ms":1840}]}

// deploy rollback --run ID
{"run_id":"…","restored":{"written":15,"deleted":3},"scan":{…},"verify":{"file_hash_mismatches":0},"projects_restored":[]}

// deploy verify
{"env":"production","ok":true,"resources_checked":150,"mismatches":[],
 "deployment_mode":{"master":"production","backup":"production"},"backup":{"converged":true},"tag_probe":{…}}

// deploy projects
{"env":"production","outcome":"applied","projects":[{"name":"WHK-Global","version":"2.4.0","sha256":"…","action":"import","verified":true},
 {"name":"QSI_WhiskeyHouseKentucky01","version":"1.9.0","action":"import","verified":true}],"order":["WHK-Global","QSI_WhiskeyHouseKentucky01"],"snapshot_dir":"…"}
```
Profile echo: envelope `profile` = the master's profile name (render: null). The global `--profile` flag does not select a deploy target (the target names its profiles); document that it is ignored for `deploy`.

---

## 8. TUI and MCP parity rules for deploy

### TUI (`ignition-cli/tests/tui_coverage.rs`, `ignition-tui/src/routes.rs`)

The walk requires a registry row for every leaf. Each of `deploy render|plan|apply|rollback|verify|projects` is a leaf of a required-subcommand group (`deploy` itself gets no row, like `workspace`). Options and why:

| Option | Verdict |
|---|---|
| `Mapping::OutOfBand` | **No.** The set is pinned to exactly `[adopt, api call, completions, edit, lsp, mcp]` and described as closed; deploy is an ordinary envelope verb with no secret-printing or protocol-stream justification. |
| `Mapping::Screen(Screen::Dashboard)` | **No.** The Dashboard count pin (41) plus the MENU_HOSTED/ACTIONS parity test force a menu verb + executor arms or a written exclusion for each. Deploy is long-running and document-shaped (the `testing run` exclusion), so that is pure ceremony. |
| `Mapping::Screen(Screen::Projects)` | **Yes**, the 13-07 `workspace` precedent: "normal envelope verbs, NOT OutOfBand … hosted on the Projects screen". No count pin applies to Projects. |

Honest caveat to record in the routes.rs comment: the `workspace` rows are registry-only (grep shows `workspace` appears only in `routes.rs` within `ignition-tui`), so "Screen(Projects)" is a coverage claim, not an implemented panel. Do the same and log a carry-forward ("cockpit deploy-plan panel"). The rows must land **in the same change as the clap commands** (Pitfall 5: an orphan row fails the walk; a missing row fails it too). Add `deploy_rows_cover_the_family` asserting exactly six rows on Projects.

### MCP (`mcp.rs`)

Catalog derives from `Cli::command()`; all six leaves become tools with no code change, and the bidirectional parity test stays green by construction. Required: add `("deploy apply", …)`, `("deploy rollback", …)`, `("deploy projects", …)` to `GUARDED_OPS` (+ markers in `main.rs`) so each advertises the optional `confirm` boolean. Prose column: representative dynamic-prose strings (the refusal message is the rendered plan, like `workspace push` / `e2e init`; pinned by markers, not by a byte-matched literal). `deploy plan/verify/render` are unguarded reads. Check before Phase 23 ships that a multi-minute `tools/call` has no bridge timeout (grep of `mcp.rs` found none) and that progress goes to stderr only.

### Stdout purity / progress

Actions never print. `DeploySink` receives `DeployEvent`s; the CLI forwards them to **stderr** (human mode) and `run_dir/events.ndjson`; the JSON envelope is the only stdout. Same rule as `logs -f` diagnostics.

---

## 9. Build order and where I would change yours

Dependencies: `20` (client) and `21` (render) are independent of each other (render needs no gateway) and can run in parallel; `22` needs both for `plan`; `23` needs `22`'s transport and plan; `24` needs `21`'s manifest and `23`'s run store.

| Phase | Scope | Library vs verb | Capture-first doc |
|---|---|---|---|
| **20** config-resource client + scan-lock/scan + mode + `deployment_mode`, resync/failover/restart-tasks (library-only) | `client/{config_resources,scan,mode,deploy_api}.rs`, classify 409 arm, `ScanLockHeld` variant, wiremock contract tests, single-ephemeral-gateway live gate | library | `20-LIVE-CAPTURES.md`: signature-mismatch status (422 vs 500), 204 on lock-info, scan completion rule, whether any scan-lock release route exists in the gateway OpenAPI, `deploymentMode` null vs `""` |
| **21** render engine + `deploy render` | manifest/target parse (YAML dep decision), layout, Gson writer, overlay, uuid mint, allowlist, secret lint, case-fold check; `RenderedTree`; CLI `deploy render` (+ TUI row + README) | verb (local only) | `21-LIVE-CAPTURES.md`: **gateway-written** `resource.json`/`config.json`/`tags.json` bytes for every declared type (key order, indent, trailing newline, escape set, number formatting). The spike fixtures are hand-written compact single-line JSON with unsorted keys, so they do NOT prove the writer contract; spec §4's "sorted keys" must be confirmed against real output. Also the secret-reference shape (`Referenced` vs `Embedded`, to be verified) and the repo-path → collection-path mapping for tags. |
| **22** **file transport + `plan` + `verify`** | `FileTransport` + `LocalFs` + `SftpTransport` (read AND write halves) + `ProcessRunner`; `ObservedState`; planner/diff; `plan_id`; `deploy plan`; `deploy verify` | verbs (read-only) | `22-LIVE-CAPTURES.md`: real-transport write/scan timings that fix `holdTimeout`; sftp behaviour on Windows OpenSSH (path form `/C:/…`, quoting, permissions) |
| **23a** guarded `apply` + `rollback` on a single gateway | run store, snapshot, lock/witness/scan/poll, verify, abort path, auto-rollback, `deployment_mode_mismatch`/preflight, gate, verdict exits | verbs (guarded) | `23-LIVE-CAPTURES.md` |
| **23b** pair semantics | backup convergence, `resync`, backup deploymentMode check, designers/peer preflight, restart-pending report, pair rig live gate | extends apply | pair-rig capture (GAN trust + approval recipe from `s3_redundancy.sh`) |
| **24** guarded `deploy projects` (+ `--check`) | §4.3 | verb (guarded) | `24-LIVE-CAPTURES.md`: S5 residual (can a child read inherited resources after a parent import) is OPEN in the spec and only verifiable on a test gateway |

### Changes from your proposed order, with reasons

1. **Move `SshTransport` (both halves) and the `FileTransport` seam from 23 into 22.** `plan` must read the live collection to diff, and tags have no per-resource REST route (spec §3), so the read half of the real transport is a hard dependency of `plan`. It also makes spec §13 step 2's acceptance ("the render must equal prod: an empty `ign deploy plan`") provable against real hosts before any write code exists. Writing it once, as a pair, in a phase that is already transport-focused avoids an SSH retrofit mid-apply.
2. **Move `deploy verify` from 23 into 22.** It is `plan`'s read-only twin (observe, compare, report) and needs no lock, snapshot, or rollback. Shipping it with `plan` gives the cutover rehearsal (§13 step 3) a read-only safety net and keeps 23 purely about writes.
3. **Split 23 into 23a (single gateway) and 23b (pair).** Apply+snapshot+lock-expiry+auto-rollback is already the riskiest phase. Backup verification needs a two-gateway rig with GAN trust (per-node PKI files + `toggleApproval`, from `s3_redundancy.sh`), which is a large fixture of its own. 23a is provable on one ephemeral gateway with a `LocalFsTransport` bind mount; 23b adds the pair and the real sshd.
4. **Land TUI row, MCP `GUARDED_OPS`, README rows, and exit-table updates in the phase that adds each verb**, never as a final sweep (the tui coverage and guarded-ops drift tests fail the build otherwise, and v1.1's "family atomicity" lesson).
5. **Optional, cheap, recommended: `plan_id` in 22 and `--expect-plan` in 23a.** The prod approval gate is a GitHub environment reviewer approving a *plan*; binding apply to the approved plan digest closes the window where state changes between review and apply. It is a few dozen lines on top of data the planner already computes. Defer if scope pressure appears; the error variant (`plan_stale`) is the only coupling.
6. **Keep `deploy projects` last but note it can run parallel to 23b** once 23a's run store exists; it needs nothing from the pair work.

---

## 10. Test strategy per layer

| Layer | Where | Tool | What it proves |
|---|---|---|---|
| L1 pure unit | `ignition-core/src/deploy/*` | `cargo test`, `proptest` (already a dev-dep) | Gson writer round-trip on captured gateway bytes; overlay precedence; allowlist; case-fold collision; layout path↔`ResourceKey` injectivity; secret lint (never echoes a value); `lock_verdict` table + proptest; plan diff invariants (`plan(R,R)` empty; every file in exactly one bucket) |
| L2 wire contract | `ignition-core/tests/deploy_client_contract.rs` | wiremock recorded-request pins | Exact paths, bodies, `Content-Type`, `collection` query; 204 on lock-info; empty-body 2xx tolerated; 409 → `scan_lock_held`; 422 body → `invalid_input`; classify-first on every call |
| L3 orchestration | `ignition-core/tests/deploy_apply.rs` | **stateful in-memory `FakeDeploy` + `LocalFsTransport` in a tempdir + `ManualClock`** | happy path; noop writes nothing; lock expiry → no scan; mid-write failure → abort path restores snapshot; verify failure → auto-rollback; preflight refusal; idempotent re-apply (property: `apply` then `plan` is empty); destructive refusal; projects parents-first order |
| L4 CLI contract | `ignition-cli/tests/contract_deploy.rs` | assert_cmd + snapbox + wiremock over the real binary | Envelope shapes (§7.3); exit 2 without `--yes` with the plan as message and ZERO write requests; `verify` red → exit 6 with envelope; stdout purity (progress on stderr); `--json`/`--compact` |
| L5 parity | existing suites | `tui_coverage.rs`, `mcp.rs` parity, `guarded_ops_registry_tracks_every_dispatch_site`, `readme_exit_table_agreement`, `exit_code_mapping_enumerated` | Rows present, confirm gate advertised, drift pins, exit table sync |
| L6 live gates | `ignition-cli/tests/e2e_deploy.rs`, `#[ignore]` + env opt-in (the `e2e_projects.rs` convention: green no-op when unset) | single ephemeral 8.3.6 (bind-mounted `data/config/resources`) for 20-23a; pair + sshd container for 23b; Windows OpenSSH only via a human check recorded in the evidence ledger (VERIFIED / MACHINE-EVIDENCE / WAIVED) | Real scan-lock/scan/mode wire; render→write→scan→read-back equality; real lock expiry; pair convergence and `resync`; Linux sftp; Windows sftp is **waived or manually verified**, not CI-proven |

Rules inherited: every plan's `<verification>` enumerates CI's FIVE gates verbatim (`cargo fmt --check`, `clippy --workspace --all-targets -D warnings`, `build --workspace`, `build -p ignition-cli --no-default-features`, `test --workspace`; WINDOWS.md entry 4). Worktrees need their own `CARGO_TARGET_DIR` (STATE.md carry-forward). `cargo install` refresh is required before any live run (PATH resolves `~/.cargo/bin/ign`).

---

## 11. Patterns to follow

1. **Capture-first.** Record wire truth in `NN-LIVE-CAPTURES.md` before model code. v1.1's captures falsified 405, PUT-rename and signature-mismatch hypotheses; this milestone has the same exposure (writer bytes, signature failure status, Windows sftp).
2. **Per-capability files own models + path consts; one block owns delegation.** `client/deploy_api.rs` mirrors this, in its own trait.
3. **Gate once, in core, after the read-only plan, before the first write; the refusal message is the preview** (`workspace_push`).
4. **Zero-write honesty.** Empty plan means no snapshot, no lock, no gate.
5. **Fail-closed injective path handling** (the workspace path-mapping lesson). Case-folding remote filesystems (Windows) mean two resource names differing only by case must refuse at render time, not clobber on write.
6. **Types for safety properties** (`ScanReady`): the compiler enforces "no scan without a lock witness".
7. **Boxed payloads for wide error variants** (WINDOWS.md entry 3).
8. **Run directory permissions.** `run_dir` may hold a `.gwbk` and project exports (secrets-bearing). Create 0700/0600; never echo contents into any envelope.

## 12. Anti-patterns to avoid

| Anti-pattern | Why bad | Instead |
|---|---|---|
| Add deploy REST methods to `GatewayApi` | 15 hand-written `unreachable!()` doubles to edit; widens MCP/TUI-visible surface; no stateful fake possible | `DeployApi` trait |
| Force the deploy tree through `MemberSource::Tree` | manifest-scoped, project-export member layout, FNV-1a | `RenderedTree` + sha256; reuse only `normalize_descriptor` |
| Use `resource_put` as the apply path | splits one transaction across REST and files; per-resource partial states | file mirror under one lock (D5) |
| Scan without a lock witness | an expired lock + scan applies a partial tree silently | `ScanReady` type + `lock_verdict` |
| Treat 2xx on `redundancy/failover` as success | endpoint answers 200 even when the backup is unreachable | read `activityLevel` back |
| Trust a node's mode without reading it | a node booted before the mode existed ignores the JVM arg silently; writes to the collection do nothing | `deployment_mode_mismatch` preflight on BOTH nodes, again in verify |
| Stage files in a sibling directory of the collection | gateway treats every `resources/<dir>` as a collection; the SSH account is scoped to `resources/<mode>` | in-place mirror + snapshot + lock |
| Rely on gateway lock release for abort | no documented release route | restore snapshot while locked, then scan |
| New error variant for every red verdict | error envelope cannot carry data | success-shaped envelope + verdict exit hook |
| Hash with a remote `sha256sum`/`Get-FileHash` | per-OS dialects; trusting remote tool output | pull bytes through the transport and hash locally |
| Put deploy rows in OutOfBand | pinned and closed set; no justification applies | `Screen::Projects` rows |
| Put the guard markers in a new source file | the drift test only `include_str!("main.rs")` | keep dispatch arms and markers in `main.rs` (or extend the test in the same change) |
| Write the YAML parser by hand | secondary dialect risk (the module override writer is hand-rolled for a *fixed* format; this is user-authored) | one maintained deserialize-only crate |
| Create worktrees under `~/data/projects` or run `ign` against WHK prod from tests | project rule; prod gateways unreachable from CI | `LocalFs`/rig fixtures, `assert_local`-style refusal in live gates |

## 13. Scalability considerations

| Concern | ~1.3k files (S4 spike) | ~14k tag files (prod `core` today) | Fleet (several gateways, §15) |
|---|---|---|---|
| Write + scan under lock | 1-2 s write, 1-3 s scan (local `docker cp`) | unmeasured on real transport; sftp per-file latency dominates | per-gateway `Target`; sequential v1.6 |
| Hold budget | 5 s from the rule | must be measured in Phase 22 | per target `hold_timeout` override |
| Hashing for plan/verify | negligible | pulls bytes over sftp; minutes possible | cache `ObservedState` per run |
| Mitigation | none needed | tar-stream impl behind `FileTransport`; only declared types/providers are in the mode collection (MQTT 13k excluded by the allowlist), so the real mode collection is far smaller than `core` | one `manifest.yaml` per gateway already isolates trees |

## 14. Open questions (need a capture or a decision, ranked)

1. **Gateway-written file bytes** (Phase 21). The writer's exact contract is unproven; spike fixtures are hand-written.
2. **Is there any scan-lock release route?** (Phase 20, OpenAPI grep of the 617-route spec.) The abort path avoids needing one, but a release route would simplify it.
3. **Stale-signature PUT status** (422 per spec §3, 500 + "signature mismatch" per v1.1 EAM captures): capture decides whether a message-classified arm is needed.
4. **Repo-path → collection-path mapping for `tag-definition` nesting and resource-name depth** (Phase 21 capture).
5. **Real-transport timings and Windows OpenSSH sftp behaviour** (Phase 22). Highest technical risk in the milestone.
6. **YAML crate choice** (Phase 21): `serde-saphyr` (typed deserialize only) vs a maintained serde_yaml fork; confirm maintenance status at planning time; note YAML 1.2 strictness (`yes`/`no`).
7. **`deploymentMode` representation on a no-mode gateway** (`""` per spec §3; local dev gateways report `null` per other repo notes): normalise to `""` in `deployment_mode()`.
8. **Embedded-secret detection rule** (which fields, which JSON shape marks a secret as embedded vs referenced): Phase 21 capture; MEDIUM confidence until then.
9. **S5 residual** (child reading inherited resources after a parent overwrite-import): verifiable only on the Plan-4 test gateway; do not claim it in Phase 24's success criteria.
10. **`holdTimeout` and `margin` defaults** until measured.

## Sources

- Design spec: `/Users/pmannion/data/worktrees/whk-environment-orchestration/gateway-config-spec/docs/superpowers/specs/2026-10-09-prod-gateway-config-as-code-design.md` (HIGH for intent; wire claims MEDIUM until captured)
- Spikes (exact API sequences on 8.3.6): `…/spikes/gateway-modes/{lib.sh,s1_config_precedence.sh,s3_redundancy.sh}`, `results/{S4,S5,SUMMARY}.md` (HIGH for the recorded sequences)
- ignition-cli source read in full at the cited symbols: `crates/ignition-core/src/{error.rs,session.rs,poll.rs,lib.rs}`, `client/{mod,classify,resources,redundancy,restart,projects,workspace}.rs`, `actions/{mod,projects,workspace,redundancy}.rs`, `rig/compose.rs`; `crates/ignition-cli/src/{cli,main,mcp}.rs`, `tests/tui_coverage.rs`; `crates/ignition-tui/src/routes.rs` (HIGH)
- `.planning/{PROJECT,ROADMAP,STATE,WINDOWS}.md`, `README.md` exit-code and commands tables (HIGH)
- Rust YAML crate landscape: https://users.rust-lang.org/t/serde-yaml-deprecation-alternatives/108868 and https://rustsec.org/advisories/RUSTSEC-2025-0068 (LOW-MEDIUM; blog/forum level, verify on crates.io at Phase 21)
