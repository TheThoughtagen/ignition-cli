# Phase 20: Config API Client - Research

**Researched:** 2026-10-10
**Domain:** Rust library seam over the Ignition 8.3.6 config-resource, scan-lock, scan and deployment-mode REST APIs (reqwest + wiremock + one stateful fake)
**Confidence:** MEDIUM-HIGH. HIGH on in-repo seams (every file:line below was read this session) and on the wire facts already captured by the spikes and by v1.1. MEDIUM on five wire behaviours that this phase's capture task must settle (lock-held status and body, stale vs missing signature status, `null` vs `""`, list-by-collection, create-mode response).

<user_constraints>
## User Constraints (from CONTEXT.md)

No `20-CONTEXT.md` exists for this phase: the user chose to plan from requirements plus milestone research. The constraints below are locked by `REQUIREMENTS.md` (header decisions) and the `ROADMAP.md` Phase 20 block, and are copied from them. Treat them with CONTEXT.md authority.

### Locked Decisions
- Library-only: no clap surface, no verb, no TUI row, no MCP change. (ROADMAP Phase 20 Constraints)
- A new `DeployApi` trait, NOT new `GatewayApi` methods (avoids 15 hand-written `unreachable!()` doubles; allows one stateful in-memory fake gateway). (ROADMAP)
- Do not enable `serde_json/preserve_order` or `arbitrary_precision` workspace-wide. (ROADMAP)
- Every new slug follows the four-place rule: `CoreError` code, `exit_code`, hint, README exit table. (ROADMAP)
- Live gate: one ephemeral 8.3.6 with bind-mounted `resources/`; no pair rig. Localhost only. (ROADMAP, task brief)
- MSRV 1.88, edition 2024. CI gates on Linux, macOS and Windows: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`, `cargo test --workspace`, `cargo build -p ignition-cli --no-default-features`. (ROADMAP, `.github/workflows/ci.yml`)
- Capture first: C1, C3a, and `null` vs `""` `deploymentMode` are recorded verbatim in `20-LIVE-CAPTURES.md` BEFORE dependent code. (ROADMAP)
- Gateway bodies echoed into errors are size-capped. (DAPI-06)
- Post-write red outcomes keep the frozen envelope (Decision A), CI/agent callers pass `--plan-sha256` (Decision C), `apply`/`rollback`/`projects` are not MCP tools (Decision D). None of these touch Phase 20 code; recorded so the planner does not re-open them.

### Claude's Discretion
- Slug names for the stale/missing-signature error (the milestone slug table in `research/SUMMARY.md` has no row for it).
- Module and file layout inside `client/`.
- Whether `deployment_mode()` returns `Option<String>` or `String`.
- The shape of the typed scan outcome.
- Which endpoints beyond DAPI-01..07 get methods (recommendation below: none).

### Deferred Ideas (OUT OF SCOPE)
- `HeldLock`/`ScanReady` witness types, the abort path, auto-rollback (Phase 23).
- `redundancy_resync`, `redundancy_failover`, `restart_tasks_pending` (Phase 23/24; the ARCHITECTURE sketch lists them, the Phase 20 requirements do not).
- Any `ign deploy` verb, plan/verify logic, SSH transport (Phases 21-25).
- `scan_lock_expired` and `deployment_mode_mismatch` CoreError variants (their constructors live in Phase 23).
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| DAPI-01 | List and read config resources by type and name, effective view or named collection | `resource_list`, `resource_names`, `resource_find(.., collection)`, `resource_singleton`; OpenAPI paths in "Wire facts"; list-by-collection is capture-gated (D7) |
| DAPI-02 | Create and update with the signature-checked array PUT; stale or missing signature is its own error | `resource_create` is POST (name only), `resource_update` is PUT (name + signature); stale = HTTP 500 `signature mismatch` per v1.1 8.3.6 capture; new variant + deploy-scoped helper (NOT classify) |
| DAPI-03 | Acquire scan-lock, read remaining, detect expiry (200 JSON then 204) without comparing gateway timestamps to the local clock | `scan_lock_acquire`, `scan_lock_info -> LockObservation`, pure `lock_verdict`, structural no-clock guard |
| DAPI-04 | Trigger scan; complete only when `scanActive == false` and `lastScanTimestamp` advanced on the same gateway | `scan_request`, `scan_status`, pure `scan_complete`, `scan_and_wait -> ScanOutcome` over `poll::poll` |
| DAPI-05 | List/create deployment modes; read active `deploymentMode`, `null` and `""` both mean none | `modes`, `mode_create`, `deployment_mode -> Option<String>` via a private wire struct (GatewayInfo stays frozen) |
| DAPI-06 | Every endpoint has a declared response shape proven by a live capture; echoed bodies are size-capped | `ResponseShape` enum + `DEPLOY_ENDPOINTS` table; `send_shaped`; cap the classify 422 echo and every new echo with `truncate_api_body` |
| DAPI-07 | Held lock (409) surfaces as `scan_lock_held` naming the holder; redundancy decides the active node by activity level with `Master`/`Backup` | classify arm on `/data/api/v1/scan-lock/` (409 and 408) + holder enrichment; pure `active_node`; `redundancy.rs` doc fix |
</phase_requirements>

## Summary

Phase 20 is a small, well-bounded library phase because the repo already has every plumbing seam it needs: one auth site (`apply_auth`), one classify-first pipeline (`send_and_classify`), one wait engine (`poll::poll`), one body cap (`truncate_api_body`), one wiremock harness (`tests/common/mod.rs`), and a proven live-gate convention. The work is (1) capture wire truth, (2) a new `DeployApi` trait implemented by `ReqwestGatewayApi` in new `client/*.rs` files, (3) four pure decision functions (`lock_verdict`, `scan_complete`, `active_node`, `normalize_deployment_mode`), (4) two new `CoreError` slugs, (5) one stateful fake, and (6) a vocabulary fix for `redundancy.rs`.

Six findings change the plan relative to the milestone research. (a) **Stale signature on PUT is already evidenced as HTTP 500 with JSON `problem.message` containing `signature mismatch`** on 8.3.6 (v1.1, EAM tasks) and v1.1 pinned that as exit 1 in two shipped tests, so the new typed error must live in a deploy-scoped helper, not in `classify.rs`, or those tests break. (b) **Create is `POST`, not `PUT`**: the OpenAPI declares PUT = modify (`name` + `signature` required) and POST = create (`name` only); v1.1 captured PUT-of-unknown-name as 404. DAPI-02's wording ("create and update with the signature-checked array PUT") is imprecise. (c) **The gateway OpenAPI declares a 408 on lock acquire** ("Unable to acquire lock within timeout") distinct from 409 ("Lock already held."); which one a competing actor sees is unknown until C1 runs. (d) **There is no vendored 617-route OpenAPI**; the closest copies have 737 paths (spec worktree and `whk-environment-orchestration/docs`) and 401 paths (this repo's v1.0 extract); none contains a scan-lock release route, but the grep step must still run against the live ephemeral gateway. (e) **`classify.rs` echoes 422 bodies unbounded** (lines 181-185) and is the one place DAPI-06's cap is violated today. (f) **`Primary` appears in six places**, not only `redundancy.rs`, and one live gate (`e2e_api_diagnostics.rs:247`) asserts the wrong vocabulary.

**Primary recommendation:** Land `20-LIVE-CAPTURES.md` first (rig recipe below), then build `client/{config_resources,scan,mode,deploy_api}.rs` around a declared `ResponseShape` table where success is always a follow-up read, add exactly two CoreError slugs (`scan_lock_held`, `resource_signature_rejected`), and keep every verdict a pure function so the skewed-clock and "200 but never advances" tests are deterministic with no `tokio` `test-util`.

## Architectural Responsibility Map

This phase has no UI/browser tier. The tiers are library layers.

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Wire calls, auth header, classify | `client/deploy_api.rs` (child of `client`) | `client/classify.rs` | Reuses private `ReqwestGatewayApi` helpers; one HTTP pipeline |
| Endpoint models and path builders | `client/{config_resources,scan,mode}.rs` | - | Capability files own models + path consts (existing convention) |
| Success predicates (lock verdict, scan complete, active node, mode normalize) | Pure fns in the same capability files | - | Table-testable and proptest-able with no I/O or clock |
| Typed failure taxonomy | `error.rs` (+2 variants) | `README.md` exit table | Four-place rule |
| Wire truth | `.planning/phases/20-.../20-LIVE-CAPTURES.md` | contract tests | Capture-first; tests pin recorded bodies |
| Test double | `tests/common/fake_deploy.rs` | - | One stateful fake implementing `DeployApi`; reused by Phase 23 |
| Orchestration (lock + write + scan + verify) | NOT this phase (Phase 23 `actions/deploy.rs`) | - | Library-only |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| reqwest | 0.13 (workspace) | HTTP; per-request `.timeout()` | Existing pipeline; `redirect(Policy::none())`, 30 s total default (`client/mod.rs:954-969`) |
| serde / serde_json | 1.0 / 1.0 + `raw_value` (workspace) | Models; `Value` for passthrough `config` | `raw_value` is already enabled; `preserve_order` and `arbitrary_precision` stay OFF |
| async-trait | 0.1 | `#[async_trait] DeployApi` (dyn-compatible) | Same as `GatewayApi` |
| tokio | 1.53 (`rt-multi-thread, macros, time, fs, io-std, io-util, process, sync`) | `poll` sleeps | NO `test-util` feature, so no `start_paused`; tests use real short deadlines |
| percent-encoding | 2 | `encode_segment` for `{name}` only | Existing `pub(crate)` helper |
| wiremock | 0.6 (dev) | Contract pins | Existing `IgnitionMock` |
| proptest | 1.11.0 (dev, core only) | `lock_verdict` and `scan_complete` invariants | Already a core dev-dep |

[VERIFIED: /Users/pmannion/whiskeyhouse/ignition-cli/Cargo.toml and crates/ignition-core/Cargo.toml read this session]

### Supporting
None. No new crate is needed. `sha2`, `base64`, `zip`, `tempfile` are irrelevant here.

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| New `DeployApi` trait | Methods on `GatewayApi` | Locked out by the roadmap (15 doubles) |
| Deploy-scoped 500 handling | New classify arm for 500 `signature mismatch` | A classify arm flips `task_delete_signature_mismatch_500_is_the_recorded_finding` (pins exit 1 / `internal`) and the EAM modify path; scoped helper leaves v1.1 untouched |
| `poll::poll` for scan wait | Hand-rolled loop | poll is "the ONE wait/retry engine" (poll.rs header); reuse |
| `tokio` `test-util` clock | Pure verdict fns + real short deadlines | Adding a tokio feature to the workspace is unnecessary churn |

**Installation:** none.

## Package Legitimacy Audit

No external packages are installed by this phase. Every crate named above is already a workspace dependency read from `Cargo.toml` this session. The `gsd-tools query package-legitimacy check` gate is therefore not applicable.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| (none) | - | - | - | - | - | n/a |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```
 caller (Phase 23 actions, or a test)
        |
        |  &dyn DeployApi            (ReqwestGatewayApi implements both GatewayApi and DeployApi;
        v                             Session derefs to it, so &*session serves both)
 +---------------------------+
 | client/deploy_api.rs      |   one method per capability, each tagged with an EndpointDecl
 |  send_shaped(decl, req)   |---------------------------------------------+
 +-------------+-------------+                                             |
               | build URL (url_for) + apply_auth (the ONLY Secret::expose) |
               v                                                           v
   request.send()  --transport error--> CoreError::Network (exit 4)     pure verdict fns
               |                                                       (no I/O, no clock)
               v                                                        lock_verdict(obs, margin)
   status 500 on /resources/ write?  --yes--> read body, substring       scan_complete(before, now)
               |                              "signature mismatch"       active_node(a, b)
               no                             -> ResourceSignatureRejected normalize_deployment_mode
               v
   classify::classify(resp, url, false)
     2xx pass | 401/403 Auth | 404 NotFound | 503 Restarting
     409/408 on /scan-lock/  -> ScanLockHeld{actor?}   (NEW arm)
     422 on /resources/      -> InvalidInput (body now CAPPED)
               v
   shape check by ResponseShape: Json | EmptyOk | NoContent | JsonOrNoContent
               v
   typed result  --> caller decides success by a FOLLOW-UP READ, never by the status code
```

### Recommended Project Structure
```
crates/ignition-core/src/client/
  config_resources.rs  # ResourceRecord, ResourceCreate, ResourceUpdate, path builders, validate_segment
  scan.rs              # ScanKind, ScanLockInfo, LockObservation, LockVerdict, lock_verdict,
                       # ScanStatus, scan_complete, ScanOutcome, scan_and_wait
  mode.rs              # ModeRecord, ModeCreate, private GatewayModeProbe, normalize_deployment_mode
  deploy_api.rs        # trait DeployApi + impl for ReqwestGatewayApi + send_shaped + DEPLOY_ENDPOINTS
  redundancy.rs        # (edit) Master/Backup docs + ACTIVITY consts + active_node
  classify.rs          # (edit) scan-lock 409/408 arm; cap the 422 echo
  mod.rs               # (edit) four `pub mod` lines
crates/ignition-core/src/error.rs      # (edit) +2 variants
README.md                               # (edit) exit-6 row + redundancy row wording
crates/ignition-core/tests/
  deploy_client_contract.rs             # wiremock pins
  common/fake_deploy.rs                 # stateful fake (+ `pub mod fake_deploy;` in common/mod.rs)
  live_deploy_api.rs                    # #[ignore] live gate, ONE sequential test
  fixtures/live-rig-deploy/             # compose.yml, mint-token.sh, fixtures (copied from the spike)
.planning/phases/20-config-api-client/20-LIVE-CAPTURES.md
```
Do NOT create `crates/ignition-core/src/deploy/` in Phase 20: Phase 21 creates `deploy/mod.rs` and `lib.rs` module lines in parallel, which would conflict. Keep the pure functions in the capability files. `DeployApi` itself stays in `client/`.

### Existing seams the plan must use (file:line, read this session)

| Seam | Location | Fact the plan relies on |
|------|----------|-------------------------|
| Auth header, the only `Secret::expose` outside `secret.rs` | `crates/ignition-core/src/client/mod.rs:600-616`; line 604: `request = request.header("X-Ignition-API-Token", token.expose());` | Token credential holds the full `name:key`; every new request MUST route through `self.apply_auth(...)` |
| URL join | `client/mod.rs:588-590` `fn url_for(&self, path: &str) -> url::Url` | Absolute path replaces from root; private, reachable from child modules |
| Test constructor | `client/mod.rs:578` `pub fn for_tests(base_url: &str, credential: Option<Credential>)` | Used by every contract test |
| Send + classify tail | `client/mod.rs:904-915` `async fn send_and_classify` | Transport error becomes `CoreError::Network`; then `classify::classify(response, url.as_ref(), false)` |
| `get_json` | `client/mod.rs:623-643` | Calls `response.json::<T>()`; MUST NOT be used for a 204 (lock info) |
| `put_json` | `client/mod.rs:889-897` | Returns `Result<(), _>` and DROPS the body: unusable for the PUT because `{success, changes, problem}` must be read |
| `post_json` | `client/mod.rs:810-818` | Returns the `Response`; usable for create and lock acquire |
| Existing resource-write precedent (body read manually) | `client/mod.rs:1713-1734` `eam_task_modify`: `self.apply_auth(self.client.put(url.clone()).json(&[definition]))`, then `send_and_classify`, then `response.text()` | Copy this shape for `resource_update` |
| Default timeouts | `client/mod.rs:960-962` `.connect_timeout(Duration::from_secs(10)).timeout(Duration::from_secs(30))` | A lock POST with `acquireTimeout` near or above 30 s would trip the client timeout: set `.timeout(acquire + slack)` per request |
| `mod classify;` is private | `client/mod.rs:38` | `deploy_api.rs` is a child module, so it may call `classify::classify` (`pub(crate)`, `classify.rs:48`) |
| classify arm order | `client/classify.rs:80-228` | 401/403, 503, 404, designer-prune 409, EAM-force 409, resource 422, fallback. No 408 or scan-lock 409 arm yet |
| UNBOUNDED echo (DAPI-06 gap) | `client/classify.rs:181` and `:183-185`: `format!("gateway rejected the resource body (HTTP 422 from {url}): {body}")` | Cap both `joined` and `body` with `truncate_api_body` |
| Path-scoped 422 | `client/classify.rs:243-245` `is_config_resource_url`: `url.contains("/data/api/v1/resources/")` | Reuse for scoping the 500 check in the deploy helper |
| Body cap | `crates/ignition-core/src/error.rs:69` `pub const GATEWAY_CLIENT_BODY_CAP_BYTES: usize = 4096;`, `:75` marker `"... [truncated]"`, `:82` `pub fn truncate_api_body(body: &str) -> String` | The single cap function; reuse, do not write a second one |
| Wait engine | `crates/ignition-core/src/poll.rs` (`PollConfig {subject, interval, max, deadline}`, `poll(cfg, state, probe)`) | Retries `Network` and `GatewayRestarting`, fails fast on `Auth`; deadline error is `CoreError::Network` with `source: None` |
| `encode_segment` | `client/projects.rs:92` `pub(crate) fn encode_segment` (NON_ALPHANUMERIC) | Use for `{name}` only. Module ids contain dots (`com.inductiveautomation.eam`); encoding a dot would produce `%2E`, so module and type are VALIDATED (charset `[A-Za-z0-9._-]`), not encoded |
| `ListQuery` / `ListEnvelope` | `client/query.rs:19-65`, `:68` | `to_query_pairs()` always emits `limit` and `offset`; `filter` is a free string |
| `CoreError` code/exit/hint/endpoint | `error.rs:779` `code()`, `:830` `exit_code()`, `:880` `hint()`, `:1217` `endpoint()` | Each is an exhaustive `match`; a new variant fails compilation until all four are updated |
| Slug sync tests | `error.rs:1371` `exit_code_mapping_enumerated`, `:1716` `EXIT_SLUG_LITERALS`, `:1820` `readme_exit_table_agreement` | Add the case, the literal, and the README token; the README parser reads the `## Exit codes` table |
| Header exit-table doc | `error.rs:15` (exit-6 row of the module doc) | Add the two slugs with `(20-xx)` markers |
| README exit-6 row | `README.md:112` | Add the two slugs to the exit-6 cell |
| Boxing rule | `.planning/WINDOWS.md` entry 3 | `CoreError` was exactly 128 bytes; box any variant with 3+ Strings |
| wiremock harness | `crates/ignition-core/tests/common/mod.rs` (`IgnitionMock::start/uri/status_json/html_error`) | `.expect(1)` mocks; bespoke mocks via `mock.server` |
| Contract test to copy | `crates/ignition-core/tests/backup_contract.rs` (token credential helper, `Mock::given(method).and(path).and(query_param)`, `received_requests()` header and body assertions) and `tests/eam_contract.rs:1135-1170` (500 JSON `problem` body) | Template for recorded-request pins |
| Live-gate pattern | `crates/ignition-core/tests/live_gateway.rs:1-90` and `crates/ignition-cli/tests/e2e_eam.rs:1-100`: `#[ignore = "..."]`, env `IGNITION_LIVE_URL` and `IGNITION_LIVE_TOKEN`, quiet skip when absent; `live_rig_module_injection.rs` runs sections as ONE ordered test | Copy for `live_deploy_api.rs` |
| Rig fixture to copy | `crates/ignition-core/tests/fixtures/live-rig/compose.yml` (named volume `gateway_data`, pinned tag, `${IGNITION_LIVE_RIG_IMAGE_TAG:-8.3.3}`) | Basis for `live-rig-deploy/compose.yml` |
| `Session` | `session.rs:76-81`: holds `Arc<ReqwestGatewayApi>`, `Deref` to it | `&*session` is a `&dyn DeployApi` once the trait is implemented |

### Wire facts (inputs to the capture task; each becomes a `20-LIVE-CAPTURES.md` row)

[VERIFIED: spike files opened this session] Lock acquire body, quoted from `spikes/gateway-modes/lib.sh:40`:
```
lock() { api_status "$1" POST "/data/api/v1/scan-lock/$2" --data "{\"acquireTimeout\":10,\"holdTimeout\":$3}" --header 'Content-Type: application/json'; }
```
Lock info while held, quoted from `results/S4.md:35-40`, and after expiry from `S4.md:46`:
```
{
  "timestamp": 149795344,
  "remaining": 4,
  "actor": "api-token:spike:192.168.97.1"
} [http 200]
...
 [http 204]
```
`timestamp` is not epoch-ms (`lastScanTimestamp` is `1791584841451`, `S4.md:18`: `{"scanActive":false,"lastScanTimestamp":1791584841451,"lastScanDuration":925}`), so it is opaque. `remaining` was `4` for `holdTimeout=5` read after about 1 s, so it is integer seconds, floored.

[VERIFIED: gateway OpenAPI in `/Users/pmannion/data/worktrees/whk-environment-orchestration/gateway-config-spec/docs/openapi.json`, queried with jq this session; 737 paths, 858 operations]

| Route | Method | Declared responses |
|-------|--------|--------------------|
| `/data/api/v1/scan-lock/config` (and `/projects`) | GET | 200 JSON `{timestamp:int, remaining:int (seconds), actor:string}`; 204 "If no lock is currently held" |
| same | POST | body `{acquireTimeout, holdTimeout}` (numbers, seconds; defaults 10 and 60); 200 JSON "Success - lock acquired"; 400 "Bad request body - unable to parse timeout option"; **408 "Unable to acquire lock within timeout"**; **409 "Lock already held."** |
| `/data/api/v1/scan/config` (and `/projects`) | GET and POST | 200 JSON `{scanActive:boolean, lastScanTimestamp:number, lastScanDuration:number}` for both |
| `/data/api/v1/mode` | GET | 200 `{items:[{name,title,description,resourceCount}], metadata}`; params limit, offset, sortBy, search, filter |
| `/data/api/v1/mode` | POST | body `{name,title,description}`; 200 JSON `{name,title,description,resourceCount}`; 400 invalid body or illegal name; 500 "Failed to create mode" |
| `/data/api/v1/mode/{name}` | PUT, DELETE | PUT adds 404 and 409 "Mode name already in use." (out of scope for Phase 20) |
| `/data/api/v1/resources/find/{module}/{type}/{name}` | GET | `?collection=`; 200, 404 "Resource definition not found"; record keys: attributes, backupConfig, collection, collections, config, data, description, enabled, healthchecks, metrics, name, signature, type, version |
| `/data/api/v1/resources/list/{module}/{type}` and `/names/...` | GET | params limit, offset, sortBy, search, filter only. **No `collection` param.** `filter` is `fieldName[op]=value` with ops eq, ne, cn, sw, ew, gt, gte, lt, lte, rgx |
| `/data/api/v1/resources/singleton/{module}/{type}` | GET | `?defaultIfUndefined=&collection=` |
| `/data/api/v1/resources/{module}/{type}` | PUT | `?allowInvalidReferences=`; array body, item required `name` and `signature`, optional `collection, enabled, description, config, backupConfig`; 200, 400, 422 "Validation failed", 500 (body `{success, changes[{name,type,collection,newSignature}], problem{message, stacktrace[]}}`) |
| same | POST | array body, item required `name` only; 200, 400, 422, 500, **409 "Conflict"** |
| `/data/api/v1/resources/{module}/{type}/{name}/{signature}` | DELETE | out of scope (v1.1 owns it) |
| `/data/api/v1/gateway-info` | GET | property `deploymentMode` declared `type: string`; also `name`, `redundancyRole`, `ignitionVersion` |
| any scan-lock release/unlock/DELETE route | - | **None** in this document (paths matching lock: only `/scan-lock/config` and `/scan-lock/projects`, GET and POST) |

[VERIFIED: v1.1 live capture, `.planning/milestones/v1.1-phases/10-eam-write-operations/10-LIVE-CAPTURES.md:224-233`, 8.3.6 verbatim]
```
{
  "success": false,
  "changes": [],
  "problem": {
    "message": "MODIFY illegal: signature mismatch for 'ResourceId{resourcePath=com.inductiveautomation.eam/eam-tasks/ign-p10-scratch, collectionName=core}'",
```
with HTTP **500**, resource unchanged afterwards (same file, line 244). The 8.3.3 message differs and LEAKS the live signature (line 188 and 239 of the same file), so never echo `problem.message` verbatim. PUT of an unknown name with the original signature answered **404 empty body** (same file, lines 246-254), so create cannot be done by PUT.

[VERIFIED: `results/S1.md:67`] PUT success body, 8.3.6: `{"success":true,"changes":[{"name":"SPIKE_DB_1791584721","type":"ignition/database-connection","collection":"production","newSignature":"d80c81d167362252bf389893ea0f3a81479521ba0bb68ee3bfc46e94bc2540b7"}],"problem":null}`

[VERIFIED: `results/S3.md:6-7` and `:40-41`] Redundancy vocabulary on a clean live pair (quoted verbatim):
```
a={"role":"Master","projectState":"Good","activityLevel":"Active",...,"peerConnected":true,...,"uptime":29279,"lastSyncTimestamp":1791586091827}
b={"role":"Backup","projectState":"Good","activityLevel":"Cold",...,"uptime":-1,"lastSyncTimestamp":1791586091875}
```
and after failover `a` is `"role":"Master" ... "activityLevel":"Cold"` and `b` is `"role":"Backup" ... "activityLevel":"Active"`. So role is configuration, `activityLevel` decides who is active, and a healthy backup reports `uptime: -1`. `lastSyncTimestamp` is epoch-ms (it sits next to `lastScanTimestamp` in the same epoch-ms range), which retires the "unit NOT capture-proven" doc in `redundancy.rs:14-19`.

[CITED: memory `ignition-83-platform-scan-api`] 409 text is "Lock already held by <actor>"; the actor is `api-token:<name>:<ip>`; the body `{"acquireTimeout":10,"holdTimeout":60}` is required (omitting it is 400); the gateway OpenAPI is served at bare `/openapi.json` only, token required. These are the planner's starting hypotheses for C1 and are NOT verified in this repo.

### Pattern 1: Declared response shape per endpoint (DAPI-06 and Pitfall 27)

**What:** every endpoint carries an `EndpointDecl { id, method, path_template, shape, capture_ref }`, and one helper `send_shaped` enforces the shape. Success of a state-changing call is NEVER the status code; the caller reads state back.

```rust
// Pattern sketch. Status and header literals below are quoted in the wire-facts section above.
pub enum ResponseShape {
    Json,            // exactly 2xx with a JSON body that parses into T
    EmptyOk,         // any 2xx, body ignored; caller MUST confirm by a follow-up read
    NoContent,       // exactly 204
    JsonOrNoContent, // 200 + JSON => Some(T), 204 => None; 200 with an empty body is an error
}
```
`scan_lock_info` is `JsonOrNoContent`; the 204 branch must be decided from `response.status()` BEFORE any body read. A 200 with an empty body is ambiguous and must fail loudly (`CoreError::Internal` naming the endpoint and the declared shape, no body echoed).

### Pattern 2: Pure verdicts, no clock (DAPI-03, DAPI-04, DAPI-07, DAPI-05)

```rust
pub enum LockObservation { Held(ScanLockInfo), Free }       // 200 JSON | 204
pub enum LockVerdict { Held { remaining_s: i64 }, Expiring { remaining_s: i64 }, NotHeld }

/// No clock parameter, and `ScanLockInfo.timestamp` is never read.
pub fn lock_verdict(obs: &LockObservation, margin_s: i64) -> LockVerdict { /* Free => NotHeld;
    Held(i) if i.remaining >= margin_s => Held; Held(i) => Expiring */ }

pub fn scan_complete(before: &ScanStatus, now: &ScanStatus) -> bool {
    !now.scan_active && now.last_scan_timestamp > before.last_scan_timestamp
}
```
Name the 204 verdict `NotHeld`, not `Expired`: `POST /scan` applies AND releases the lock (spec section 3, `docs/.../2026-10-09-prod-gateway-config-as-code-design.md:52`), so a 204 after a successful scan is the normal state and only the caller's context says whether it means "lost the lock". `remaining` is integer seconds floored, so a margin below 2 s is not meaningful.

### Pattern 3: Active node by `activityLevel`, never `role` (DAPI-07)
`active_node(a, b) -> ActiveNode { OnlyA, OnlyB, Both, Neither }` compares `activity_level.as_deref() == Some("Active")` on each status. `Both` is a split-brain signal the caller must refuse on. Unknown or absent level is "not active". The `role` string is only echoed (`Master`, `Backup`, and `Independent` on a single rig, captured in `redundancy.rs:135`).

### Pattern 4: One stateful fake plus a differential scenario
Implement `FakeDeploy` in `tests/common/fake_deploy.rs` with real captured semantics (lock countdown measured in injected ticks, 204 after expiry, scan advances `lastScanTimestamp`, PUT with a wrong signature returns the same typed error as the real client). Write each behavioural scenario as `async fn scenario(api: &dyn DeployApi) -> Trace`, run it against the fake always and against the live gateway under `#[ignore]`, and compare traces by shape. This stops the fake from drifting from the gateway.

### Anti-Patterns to Avoid
- **Adding any of this to `GatewayApi`.** Locked out.
- **Putting the 500 `signature mismatch` arm in `classify.rs`.** Breaks `tests/eam_contract.rs:1135` (pins `exit_code() == 1` and `code() == "internal"`) and changes `actions/eam.rs:765-785` behaviour. Handle it in the deploy helper only.
- **Using `put_json`** for the PUT (drops the body) **or `get_json`** for lock info (calls `.json()` on a 204).
- **Using `ign api call` or the `api call` pipeline for any of this.** README:79 says a 2xx non-JSON body is refused; the spike had to fall back to curl for the same reason.
- **Comparing gateway timestamps to `SystemTime`/`Utc::now`.** `ScanLockInfo.timestamp` is not epoch.
- **Treating `POST /scan` 200 as done**, or reading `before` while another actor's scan is active (see Pitfall 3).
- **Echoing `problem.message` or `stacktrace`** (8.3.3 leaks the live signature; frames drift).
- **A dead `CoreError` variant.** Add only variants that have a constructor in this phase.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Auth header | A second header site | `apply_auth` (`client/mod.rs:600`) | The only audited `Secret::expose` call site |
| Body cap | A new truncation fn | `truncate_api_body` (`error.rs:82`, 4096 bytes) | Char-boundary safe, one marker |
| Waiting for scan | A sleep loop | `poll::poll` | Backoff, Auth fail-fast, Network retry already tested |
| Path segment encoding | `format!` with raw names | `encode_segment` for `{name}`; charset validation for module/type | Resource names contain spaces and symbols; module ids contain dots |
| Pagination envelope | A new list type | `ListEnvelope<T>` / `ListQuery` | Same `{items, metadata}` everywhere |
| Mutation outcome | New `{success, changes, problem}` structs | `eam::ModifyOutcome`, `ResourceChange`, `MutationProblem` (`client/eam.rs:324-379`) | Already captured and lenient; re-export under a generic name |
| JSON in tests | Hand-built strings | `serde_json::json!` with the verbatim captured bodies | Matches existing tests |

**Key insight:** the wire is simple and mostly captured; the risk is not building plumbing, it is *believing* a status code. Every design choice above exists to make a 200 or a 204 impossible to mistake for "done".

## Runtime State Inventory

Not a rename/migration phase, with one exception: the `Primary` to `Master` vocabulary correction, which is a docs/test-fixture edit with no stored state.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | None - verified: the CLI persists no redundancy role anywhere | none |
| Live service config | None - read-only vocabulary | none |
| OS-registered state | None | none |
| Secrets/env vars | None | none |
| Build artifacts | None | none |
| In-repo `Primary` mentions | `crates/ignition-core/src/client/redundancy.rs:22,39,182,191`; `client/version.rs:24`; `client/status.rs:123`; `README.md:409`; `crates/ignition-cli/tests/e2e_api_diagnostics.rs:241-250` (asserts `["Independent", "Primary", "Backup"].contains(&role)`, which fails on a live Master) | code edit: doc comments, one test fixture (use the S3 body with `Master`), the live-gate allowlist, README wording |

## Common Pitfalls

### Pitfall 1: "Stale or missing signature" is two wire outcomes, not one
**What goes wrong:** the spec says 422; v1.1's 8.3.6 capture says a *stale* signature is a 500 with a JSON `problem`. A *missing* one is unobserved on a database connection.
**Why:** the 422 path is body validation (`{messages, fieldMessages}`, v1.1 section 10); the mismatch is raised later by the push validator.
**How to avoid:** C3a captures stale, omitted, and empty-string signatures. Implementation: stale (500 + substring) and any gateway-distinguishable missing case map to `resource_signature_rejected`; the client also refuses an empty `signature` before the wire (type: `signature: String` non-empty, checked in the constructor). Everything else on 500 stays a capped `Internal`.
**Warning signs:** a contract test passing against a hand-written 422 while the live gate shows 500.

### Pitfall 2: Changing classify breaks frozen v1.1 pins
**What goes wrong:** a path-scoped 500 arm in `classify.rs` changes `eam_task_modify`/`eam_task_delete` results and flips `tests/eam_contract.rs:1135` and `:1744`.
**How to avoid:** deploy-scoped helper (read body only for 500 on `/data/api/v1/resources/`, otherwise `classify::classify`). Verification step: `cargo test -p ignition-core --test eam_contract` unchanged and green.

### Pitfall 3: Scan completion false positive from someone else's scan
**What goes wrong:** the predicate is `scanActive == false && lastScanTimestamp > before`. If another actor's scan is running when `before` is read, its completion satisfies the predicate for OUR scan.
**How to avoid:** in `scan_and_wait`, if the first `before` read has `scanActive == true`, poll until false and re-read `before`, then POST. Capture what a second POST does while a scan is active (D4).
**Also:** the memory note "a completed scan does NOT guarantee fresh code" is project-scan only; config completion plus read-back is Phase 22/23's job.

### Pitfall 4: Request timeout shorter than `acquireTimeout`
The default client timeout is 30 s (`client/mod.rs:962`) and the gateway may block up to `acquireTimeout` before answering 408/409. Set `.timeout(Duration::from_secs(acquire_s) + slack)` per request, and never auto-retry the non-idempotent POSTs (lock, scan).

### Pitfall 5: 204 and `.json()`
`get_json` on a 204 yields a deserialization error that reads as "gateway drift" instead of "lock free". Decide on `response.status()` first.

### Pitfall 6: `deploymentMode` null, absent, or `""`
The spike helper `active_mode` uses jq `// ""` (`lib.sh:26`), which collapses `null` and absent into `""`, so the spike never distinguished them. The OpenAPI says `type: string`. Capture the raw bytes (D5). Implementation: `Option<String>` field with `#[serde(default)]`, then `filter(|s| !s.is_empty())`. `GatewayInfo` (`client/version.rs:20-47`) stays untouched because it serialises into frozen `--json` output.

### Pitfall 7: Parallel Phase 21 edit collisions
Phase 21 adds `deploy_tree_invalid` and `embedded_secret` to the same lines in `error.rs` (header table, `code()`, `exit_code()`, `hint()`, `exit_code_mapping_enumerated`, `EXIT_SLUG_LITERALS`) and the README exit table. If both phases run in parallel, land Phase 20's error edits as ONE early commit and rebase Phase 21 onto it. Both also add `pub mod` lines in different files (`client/mod.rs` vs `lib.rs`), which is why Phase 20 must not create `src/deploy/`.

### Pitfall 8: Fake gateway lies
A hand-written stateful fake that is never compared to the gateway encodes assumptions as facts. Mitigation: Pattern 4 (differential scenario), and the fake shares the same pure verdict functions as production.

### Pitfall 9: Live-gate state leaks
A held lock from a failed live test blocks the next run for up to `holdTimeout`. Use `holdTimeout` of 3 to 5 s in live tests, and end the live test with a cleanup scan.

### Pitfall 10: Unbounded echo is not only classify
Any new `format!` that includes a response body or a gateway string must pass `truncate_api_body`. Grep gate: `grep -n "text().await" crates/ignition-core/src/client/{deploy_api,config_resources,scan,mode}.rs` must show each use feeding a capped path.

## Code Examples

### Existing pattern to copy for the PUT (read the body, classify-first)
```rust
// Source: crates/ignition-core/src/client/mod.rs:1722-1733 (eam_task_modify), adapted
let url = self.url_for(&path);
let request = self.apply_auth(self.client.put(url.clone()).json(&items));
let response = self.send_and_classify(request, &url).await?;   // deploy helper replaces this for 500
let text = response.text().await.unwrap_or_default();
```

### Shape of the deploy helper's 500 branch
```rust
// Sketch (not verified code). "signature mismatch" is quoted in the capture above.
let response = request.send().await.map_err(|err| CoreError::Network { url: url.to_string(), source: Some(err), observation: None })?;
if response.status().as_u16() == 500 && is_config_resource_url(url.as_str()) {
    let body = response.text().await.unwrap_or_default();
    if body.contains("signature mismatch") {
        return Err(CoreError::ResourceSignatureRejected { name, endpoint: Some(url.to_string()) }); // body NOT echoed
    }
    return Err(CoreError::Internal(format!("unexpected HTTP 500 from {url}: {}", truncate_api_body(&body))));
}
classify::classify(response, url.as_ref(), false).await?;
```

### Wiremock pin for lock info (shape to copy from `backup_contract.rs`)
```rust
// 200 JSON while held, then 204 with no body (S4.md:35-46). Two Mocks with `up_to_n_times(1)`.
wiremock::Mock::given(wiremock::matchers::method("GET"))
    .and(wiremock::matchers::path("/data/api/v1/scan-lock/config"))
    .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
        serde_json::json!({"timestamp": 149795344, "remaining": 4, "actor": "api-token:spike:192.168.97.1"})))
    .up_to_n_times(1).mount(&mock.server).await;
wiremock::Mock::given(wiremock::matchers::method("GET"))
    .and(wiremock::matchers::path("/data/api/v1/scan-lock/config"))
    .respond_with(wiremock::ResponseTemplate::new(204)).mount(&mock.server).await;
```

## Proposed `DeployApi` (final recommendation)

```rust
#[async_trait::async_trait]
pub trait DeployApi: Send + Sync {
    // DAPI-01
    async fn resource_names(&self, module: &str, ty: &str) -> Result<ListEnvelope<ResourceName>, CoreError>;   // {name, enabled, modes}
    async fn resource_list(&self, module: &str, ty: &str, q: &ListQuery) -> Result<ListEnvelope<ResourceRecord>, CoreError>;
    async fn resource_find(&self, module: &str, ty: &str, name: &str, collection: Option<&str>) -> Result<ResourceRecord, CoreError>;
    async fn resource_singleton(&self, module: &str, ty: &str, collection: Option<&str>) -> Result<ResourceRecord, CoreError>;
    // DAPI-02
    async fn resource_create(&self, module: &str, ty: &str, items: &[ResourceCreate]) -> Result<MutationOutcome, CoreError>;  // POST array, name only
    async fn resource_update(&self, module: &str, ty: &str, items: &[ResourceUpdate]) -> Result<MutationOutcome, CoreError>;  // PUT array, name + signature
    // DAPI-03 / DAPI-07
    async fn scan_lock_acquire(&self, kind: ScanKind, acquire_s: u64, hold_s: u64) -> Result<(), CoreError>;   // 409/408 -> ScanLockHeld
    async fn scan_lock_info(&self, kind: ScanKind) -> Result<LockObservation, CoreError>;                    // JsonOrNoContent
    // DAPI-04
    async fn scan_status(&self, kind: ScanKind) -> Result<ScanStatus, CoreError>;
    async fn scan_request(&self, kind: ScanKind) -> Result<ScanStatus, CoreError>;                           // POST; body is NOT completion
    // DAPI-05
    async fn modes(&self) -> Result<ListEnvelope<ModeRecord>, CoreError>;
    async fn mode_create(&self, body: &ModeCreate) -> Result<ModeRecord, CoreError>;
    async fn deployment_mode(&self) -> Result<Option<String>, CoreError>;                                   // None = no mode (null, absent, "")
}
// free fns: lock_verdict, scan_complete, scan_and_wait(&dyn DeployApi, ScanKind, PollConfig) -> ScanOutcome,
//           active_node, normalize_deployment_mode
```
Notes: `resource_singleton` and the singleton array PUT are included because DAPI-02 says "array even for singletons" and v1.1 already captured the singleton read and write shape on 8.3.6 (`10-LIVE-CAPTURES.md` section 0), so no new capture is required beyond a collection-param probe. `ScanKind` has `Config` and `Projects` (same code path; wiremock-pin both, live-capture `Config` only). `scan_and_wait` returns `Ok(ScanOutcome::Complete(status))` or `Ok(ScanOutcome::NotComplete { last, waited })` and errors only for transport/auth, so "200 but never advances" is an Ok(NotComplete), never a false success. Implementation hint: the probe tracks its own `Instant` and returns `Done(NotComplete)` at its budget; set the `poll` deadline to budget plus two intervals as a backstop.

**Per-endpoint shape table (provisional; C1..C9 confirm each row):**

| Method | Endpoint | Shape | Success predicate |
|--------|----------|-------|-------------------|
| `resource_list/names/find/singleton` | GET `/data/api/v1/resources/{list,names,find,singleton}/...` | Json | 200 + parse; 404 -> `not_found` |
| `resource_create` | POST `/data/api/v1/resources/{m}/{t}` | Json (`{success,changes,problem}`) | 200 AND `success == true`; `success:false` on a 2xx is an error (denial rides 200) |
| `resource_update` | PUT same | Json | same, plus 500 `signature mismatch` -> typed error |
| `scan_lock_acquire` | POST `/data/api/v1/scan-lock/{k}` | EmptyOk (200 JSON ignored) | 2xx; confirm with `scan_lock_info`; 409/408 -> `scan_lock_held` |
| `scan_lock_info` | GET same | JsonOrNoContent | 200 JSON -> Held; 204 -> Free |
| `scan_status` | GET `/data/api/v1/scan/{k}` | Json | 200 + parse |
| `scan_request` | POST same | Json (snapshot, not completion) | 200; completion only via `scan_complete` |
| `modes` / `mode_create` | GET/POST `/data/api/v1/mode` | Json | 200 + parse; confirm create by `modes` |
| `deployment_mode` | GET `/data/api/v1/gateway-info` | Json (private struct) | 200; normalize |

## New `CoreError` variants (four-place rule)

| Slug | Exit | Payload | Constructed by | Hint (must not name a nonexistent command) |
|------|------|---------|----------------|--------------------------------------------|
| `scan_lock_held` | 6 | `{ actor: Option<String>, kind: String, endpoint: Option<String> }` (small; no box needed) | classify arm on `/data/api/v1/scan-lock/` for 409 and 408; `scan_lock_acquire` fills `actor` from `scan_lock_info` when the body does not name it | "another actor holds the config scan-lock; it expires on its own (holdTimeout) and has no release route - wait for the holder, then retry" |
| `resource_signature_rejected` | 6 | `{ name: String, reason: SignatureReason (Stale or Missing), endpoint: Option<String> }` | the deploy helper | "the resource changed since it was read, or no signature was sent: re-read it (find) and retry with the fresh signature; signatures are per-write and opaque" |

Slug names are the planner's discretion [ASSUMED: names chosen here, not decided in any prior document]; exit class 6 is target-state, consistent with `session_not_prunable` and `import_denied`. `scan_lock_expired` and `deployment_mode_mismatch` from the milestone table are NOT added now (no constructor until Phase 23). `endpoint()` must list both variants (they carry one), and the README row plus `EXIT_SLUG_LITERALS` plus `exit_code_mapping_enumerated` get both slugs.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Spike used `ign api call` + curl and jq `// ""` | Typed client with declared shapes | this phase | 204 and empty bodies are first-class |
| Redundancy doc: `Independent`/`Primary`/`Backup` | `Independent`/`Master`/`Backup`; activity `Active`/`Cold` | S3 spike 2026-10-09 | Decide active by `activityLevel` |
| `lastSyncTimestamp` unit "not capture-proven" | epoch-ms (S3) | S3 spike | Update `redundancy.rs` doc |
| v1.1: stale signature handled by post-failure find, no slug | Typed slug for the deploy client | this phase | EAM keeps its evidence-based path |

**Deprecated/outdated:** `GatewayInfo.redundancy_role` doc strings (`version.rs:24`, `status.rs:123`) naming `Primary`.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | A second token's POST to a held lock answers 409 (or 408) with a body naming the actor | Wire facts, Pattern list | Holder enrichment via `scan_lock_info` is the fallback; capture D1 decides |
| A2 | Missing-signature PUT answers 422 or 400 with a distinguishable body | Pitfall 1 | If indistinguishable, the client's pre-wire empty-signature refusal carries the "missing" case |
| A3 | `deploymentMode` is `""`, `null`, or absent on a no-mode gateway (spec says `""`, other notes say `null`) | Pitfall 6 | `Option<String>` + filter handles all three; only the capture doc changes |
| A4 | `list` accepts `filter=collection[eq]=<c>` | Proposed API | If not, list-by-collection = `names` (`modes` array) then per-name `find?collection=` |
| A5 | New slug names `scan_lock_held` and `resource_signature_rejected` | New variants | Naming only |
| A6 | `deployment_mode()` returns `Option<String>` rather than the `""` sentinel in SUMMARY Conflict 10 | Proposed API | Intent (both mean "no mode") preserved; Phase 22 compares `Option` to the target |
| A7 | A bind mount of `./resources` over the named data volume boots a commissioned gateway and the gateway populates `core/` into it | Capture procedure | If not, use the spike's named volume plus `docker compose cp` (proven); Phase 20 needs no filesystem writes |
| A8 | A no-op config scan still advances `lastScanTimestamp` | Pitfall 3 | The spike `smoke.sh` calls `scan_wait` on an idle rig and passes (indirect evidence); capture D4 confirms |
| A9 | Mixed-type `POST /resources` create works without `attributes.uuid` in the body (gateway assigns it) | Capture | If create requires a uuid, `ResourceCreate` gains an optional `attributes` passthrough |

## Open Questions

1. **Which status does a competing actor see, and when?**
   - Known: OpenAPI declares both 408 (timeout waiting) and 409 (already held); memory says 409 with text "Lock already held by <actor>".
   - Unclear: whether 409 is immediate and 408 only after `acquireTimeout`, and whether the same token re-acquires.
   - Recommendation: capture D1/D2 with two tokens; map BOTH to `scan_lock_held`.
2. **Does `list` honour a collection filter?** (A4) Recommendation: capture D7; design degrades gracefully.
3. **Create-mode response and duplicate behaviour.** The spikes never recorded the body (`S1.md:11-13` says "already exists"). Capture D6.
4. **Does a scan POST while another scan is active queue or no-op?** Capture D4; informs Pitfall 3.
5. **Does the lock block API writes by another actor?** That is C2 (Phase 23); do not claim it in Phase 20.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Docker + compose | live rig | yes | Compose v5.1.2; daemon answered `docker ps` | none (live gate would be waived, captures blocked) |
| `inductiveautomation/ignition:8.3.6` image | live rig | yes, already pulled | 2.31 GB | pull |
| cargo / rustc | build and tests | yes | 1.94.1 (CI uses stable; `rust-version = "1.88"`) | - |
| jq, curl, openssl | capture scripts | yes (`/opt/homebrew/bin/jq`; curl and openssl standard) | - | - |
| Free host port 18290 | rig | yes (lsof empty) | - | any free port; avoid 18088/18089/18188/19188/9088/9198 which existing profiles use |
| `ign` | optional second-token mint | yes | 1.5.0 | spike-style config token |

Host is arm64; the spike pins `platform: linux/amd64` (Rosetta). A running container `ign-jvm-live-test` (8.3.6 on 19088) belongs to someone else and has no profile: **do not use it**. No disposable rig is running; the capture task starts one.

**Missing dependencies with no fallback:** none.

## Capture Procedure (the plan's FIRST task; localhost only)

Output: `.planning/phases/20-config-api-client/20-LIVE-CAPTURES.md`, structured like `10-LIVE-CAPTURES.md` (header table with rig version and UTC times, then one verbatim block per observation, then a Decisions section, then a drift/limits section). Record verbatim; nothing from memory.

**1. Rig fixture** (copy from the spike, which is proven on 8.3.6). Create `crates/ignition-core/tests/fixtures/live-rig-deploy/compose.yml` from `spikes/gateway-modes/docker-compose.yml:3-28`, reduced to ONE service:
```yaml
services:
  gw:
    image: inductiveautomation/ignition:8.3.6
    platform: linux/amd64
    hostname: gw
    ports: ["18290:8088"]
    environment: { ACCEPT_IGNITION_EULA: "Y", GATEWAY_ADMIN_USERNAME: admin, GATEWAY_ADMIN_PASSWORD: cap20-password, IGNITION_EDITION: standard, IGNITION_UID: "2003", IGNITION_GID: "2003" }
    command: ["-n", "cap20", "--", "${CAP20_MODE_ARG:--Dcap20.noop=true}"]
    volumes: ["gw-data:/usr/local/bin/ignition/data"]   # PLUS the bind mount below if A7 holds
volumes: { gw-data: {} }
```
Bind-mount variant to try first (roadmap constraint): add `- ./resources:/usr/local/bin/ignition/data/config/resources`. Falsifier (30 s after boot): `ls resources/core` on the host shows gateway-created content and `curl -s localhost:18290/StatusPing` returns RUNNING. If not, drop the bind and use `docker compose cp` for any file writes (Phase 20 captures do not need any).

**2. Boot and tokens.** `docker compose up -d`; wait `until curl -s http://localhost:18290/StatusPing | grep -q RUNNING; do sleep 3; done` (spike `lib.sh:49-52`). Mint TWO config-defined tokens `cap-a` and `cap-b` with a custom level by adapting `spikes/gateway-modes/mint-token.sh` (it injects `external/ignition/api-token/<name>/{config.json,resource.json}` with `securityLevels:[{"name":"Authenticated","children":[{"name":"Deploy"}]}]` and copies `fixtures/security-levels` and `fixtures/security-properties` into `core/ignition/...`, then restarts; copy those fixtures verbatim, including the `Deploy` level in `readPermissions`/`writePermissions`). The header value is `<name>:<key>` (`mint-token.sh:28`). Keep the keys in env vars; never write them into the capture doc. Fallback: `ign profile add cap20 http://localhost:18290` then `IGNITION_PASSWORD=cap20-password ign adopt --profile cap20 --key-name cap20-a` (Administrator-level; run again with `--key-name cap20-b`); the token is only printed when no keyring is available, so curl captures prefer the config-token route.

**3. Capture helper** (verbatim request/response; never logs the token):
```bash
BASE=http://localhost:18290; OUT=.planning/phases/20-config-api-client/20-LIVE-CAPTURES.md
cap() { local label=$1 tok=$2 m=$3 p=$4; shift 4
  { printf '\n## %s\n\n~~~\n%s %s  @ %s\n' "$label" "$m" "$p" "$(date -u +%FT%TZ)"
    curl -sS -i -m 60 -w '\n[curl http=%{http_code} total=%{time_total}s]\n' -X "$m" -H "X-Ignition-API-Token: $tok" "$@" "$BASE$p"
    printf '~~~\n'; } >> "$OUT"; }
```
Use `-i` so 204 and empty bodies and headers (Content-Type, Content-Length) are recorded.

**4. Capture matrix** (D-numbers are the decisions the doc's Decisions section must state):

| ID | Request sequence | Record | Feeds |
|----|------------------|--------|-------|
| C1a | idle `GET /data/api/v1/scan-lock/config` | status, headers, empty body | JsonOrNoContent (D9) |
| C1b | `POST /scan-lock/config` with NO body; then with `{"acquireTimeout":10,"holdTimeout":60}` | 400 body; 200 body bytes | EmptyOk vs Json (D9) |
| C1c | held `GET` twice, 2 s apart | `remaining` countdown, `actor` format, `timestamp` | unit, opaque timestamp |
| C1d | token A acquires, token A acquires again | status/body/timing | reentrancy (D2) |
| C1e | token A holds (`holdTimeout=20`); token B `POST` with `acquireTimeout=2` | **status (409 vs 408), body verbatim, elapsed seconds** | `scan_lock_held` arm and actor parsing (D1) |
| C1f | `holdTimeout=3`, sleep 5, `GET` | 204; then `POST /scan/config` and `GET /scan/config` | expiry detection |
| C1g | probe `DELETE /scan-lock/config`, `POST /scan-lock/config/release` | 404/405 | no release route (D8) |
| C1h | `GET /openapi.json` (token) saved to scratch; `jq '[.paths\|to_entries[]\|select(.key\|test("scan\|lock"))\|{p:.key,m:(.value\|keys)}]'` | live 8.3.6 path count and the scan-lock methods | confirms the grep on the REAL doc (the 617/737 gap) |
| C1i | `GET /scan/config` idle; `POST /scan/config`; poll GET at 500 ms until `scanActive==false && lastScanTimestamp>before`; second POST while active; scan while A holds the lock (as B) | bodies, `lastScanDuration`, does a no-op scan advance the timestamp | completion rule (D4) |
| C3a | `POST` create `CAP20_DB_<ts>` (DB connection, body `[{"name":..,"config":{...}}]` with the config from `fixtures/db/mode/config.json`); `find` for the signature; `PUT` with the right signature; `PUT` with `"signature":"deadbeef..."` (64 hex); `PUT` with the key omitted; `PUT` with `""`; `PUT` unknown name; duplicate `POST` | status + full body for each; read-back after each refusal | stale vs missing (D3), create body and 409 shape (D10), POST-vs-PUT |
| C-null | raw `GET /data/api/v1/gateway-info` on the no-mode gateway; `jq -c '{has:has("deploymentMode"),v:.deploymentMode,t:(.deploymentMode\|type)}'`; then `POST /mode` `{"name":"production","title":"Production","description":"cap20"}`, recreate with `CAP20_MODE_ARG=-Dignition.config.mode=production docker compose up -d`, read gateway-info again | raw bytes both times | `null` vs `""` (D5) |
| C-mode | `GET /mode`; `POST /mode` response body; duplicate create; illegal name (e.g. `a b/`) | status + bodies | `ModeRecord`, create response (D6) |
| C-list | `GET /resources/list/ignition/database-connection` with `limit=-1`, with `filter=collection[eq]=production`; `names`; `find?collection=core` vs `?collection=production` on a resource present in both | which honour the collection; `signature` present on list items? | D7, A4 |

**5. Exit criteria for task 1:** every row above has a verbatim block; the Decisions section states D1..D10; the doc names the final `ResponseShape` for each endpoint in the table above; a trimmed OpenAPI extract (scan, scan-lock, mode, gateway-info, one named resource type) is committed next to it (full file stays in the scratch dir, about 12 MB). Tear down with `docker compose down -v`.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | `cargo test` (built-in) + wiremock 0.6 + proptest 1.11.0 |
| Config file | none (workspace `Cargo.toml`); live gate is env-gated |
| Quick run command | `cargo test -p ignition-core --lib client:: && cargo test -p ignition-core --test deploy_client_contract` |
| Full suite command | the five CI gates: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo build --workspace`; `cargo test --workspace`; `cargo build -p ignition-cli --no-default-features` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|--------------|
| DAPI-01 | list/names/find/singleton hit exact paths and query (`collection`, `limit=-1`); module/type charset refused; `{name}` percent-encoded | wiremock contract + unit | `cargo test -p ignition-core --test deploy_client_contract resource_` | Wave 0 |
| DAPI-01 | effective view vs named collection against a real gateway | live gate | `IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 IGNITION_LIVE_TOKEN=cap-a:... cargo test -p ignition-core --test live_deploy_api -- --ignored --nocapture` | Wave 0 |
| DAPI-02 | POST create body is `[{name, ...}]`; PUT body is an array with `name` + `signature`; 500 `signature mismatch` -> `resource_signature_rejected`; 2xx `success:false` -> error; empty signature refused pre-wire; v1.1 EAM pins still green | wiremock contract | `cargo test -p ignition-core --test deploy_client_contract signature_` and `cargo test -p ignition-core --test eam_contract` | Wave 0 / existing |
| DAPI-02 | `error.rs` slug sync (variant, code, exit, hint, endpoint, README row) | unit | `cargo test -p ignition-core --lib error::tests` | existing (edit) |
| DAPI-03 | `lock_verdict` table; proptest: verdict independent of `timestamp` for any i64; 200 JSON then 204 sequence; no `SystemTime`/`Utc`/`now()` in `client/scan.rs` (structural pin via `include_str!`) | unit + proptest + contract | `cargo test -p ignition-core --lib client::scan` | Wave 0 |
| DAPI-03 | real hold then 204 | live gate | live command above | Wave 0 |
| DAPI-04 | `scan_complete` truth table (4 cases: active, unchanged ts, advanced+inactive, advanced+active); trigger 200 but status never advances -> `Ok(NotComplete)`; foreign scan active at `before` is not counted | unit + wiremock contract with `PollConfig{interval:20ms, deadline:300ms}` | `cargo test -p ignition-core --test deploy_client_contract scan_` | Wave 0 |
| DAPI-04 | real scan advances the timestamp | live gate | live command | Wave 0 |
| DAPI-05 | `normalize_deployment_mode` over `null`, `""`, absent, `"production"`; `modes` and `mode_create` request pins; `GatewayInfo` JSON unchanged (existing `gateway_info_contract` still green) | unit + contract | `cargo test -p ignition-core --lib client::mode` and `cargo test -p ignition-core --test gateway_info_contract` | Wave 0 / existing |
| DAPI-06 | `DEPLOY_ENDPOINTS` has one entry per trait method (compile-time array length pinned by a unit test); each shape enforced (200-empty on JsonOrNoContent fails loudly); 100 KB body through the 422 arm and the 500 branch is capped (<= 4096 + marker); `20-LIVE-CAPTURES.md` has a block per endpoint id | unit + contract + doc grep | `cargo test -p ignition-core --test deploy_client_contract shape_ cap_` and `grep -c '^## ' .planning/phases/20-config-api-client/20-LIVE-CAPTURES.md` | Wave 0 |
| DAPI-07 | 409 (and 408) on `/scan-lock/` -> `scan_lock_held` naming the actor (from body, else from the follow-up `GET`); `active_node` over the S3 verbatim pre-failover and post-failover bodies, both-Active, neither, `Independent`; `redundancy.rs` has no `Primary` | unit + contract + grep | `cargo test -p ignition-core --test deploy_client_contract lock_held` and `cargo test -p ignition-core --lib client::redundancy` and `! grep -n Primary crates/ignition-core/src/client/redundancy.rs` | Wave 0 / existing (edit) |
| (parity) | fake gateway honours the same scenario as the live gateway | differential | `cargo test -p ignition-core --test deploy_client_contract fake_` plus the live command | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p ignition-core --lib client::` plus the contract file.
- **Per wave merge:** the five CI gates, in full, locally (WINDOWS.md entry 4).
- **Phase gate:** five gates green on all three OSes plus one recorded live-gate run against the ephemeral 8.3.6 (`cargo install --path crates/ignition-cli` is NOT needed: the live gate calls the library, not the `ign` binary).

### Wave 0 Gaps
- [ ] `crates/ignition-core/tests/deploy_client_contract.rs` (covers DAPI-01..07)
- [ ] `crates/ignition-core/tests/common/fake_deploy.rs` plus `pub mod fake_deploy;` in `common/mod.rs`
- [ ] `crates/ignition-core/tests/live_deploy_api.rs` (`#[ignore]`, ONE sequential test, refuses any non-localhost `IGNITION_LIVE_URL`)
- [ ] `crates/ignition-core/tests/fixtures/live-rig-deploy/` (compose, mint script, security fixtures)
- [ ] `.planning/phases/20-config-api-client/20-LIVE-CAPTURES.md`
- Framework install: none.

## Security Domain

`security_enforcement` is enabled (absent = enabled; config shows `true`, ASVS level 1).

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes | Token only via `apply_auth`; `Secret::expose` stays confined there; no new header site |
| V3 Session Management | no | stateless token client |
| V4 Access Control | yes (client side) | 401/403 -> `Auth`; the write-permission probe is Phase 23 |
| V5 Input Validation | yes | validate `module`/`type` against `[A-Za-z0-9._-]` and reject `..`, `/`, empty; `encode_segment` for `{name}`; reject empty `signature` |
| V7 Error handling and logging | yes | cap echoed bodies with `truncate_api_body`; never echo `problem.message`/`stacktrace` (8.3.3 leaks the live signature); never log the token |
| V9 Communications | yes | `ssl_verify` per profile as today; live gate refuses non-localhost |
| V6 Cryptography | no | none hand-rolled |

### Known Threat Patterns
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Path-segment injection via resource names (`../`, `%2F`) | Tampering | charset validation for module/type; `encode_segment` (`/` -> `%2F`, `projects.rs:279-287`) for names |
| Secret or signature leak through error text | Information disclosure | no verbatim `problem`; cap; typed variant carries only the resource name |
| Unbounded gateway body (memory or log flood) | Denial of service | cap on echo; shapes ignore bodies they do not need (note `.text()` still buffers; acceptable, error bodies are small) |
| Live test pointed at production | Tampering | host allow-list check (`localhost`, `127.0.0.1`) at the top of `live_deploy_api.rs`, same as `assert_local` in the spike `lib.sh:9-12`; never use profile `prod` |
| Lock left held after a crash | Denial of service | short `holdTimeout` in tests; gateway self-heals on expiry |

## Sources

### Primary (HIGH confidence, opened this session)
- `/Users/pmannion/whiskeyhouse/ignition-cli/.planning/{REQUIREMENTS,STATE,ROADMAP,WINDOWS}.md`, `.planning/research/{SUMMARY,ARCHITECTURE,PITFALLS}.md`, `.planning/config.json`, `.claude/CLAUDE.md`
- `crates/ignition-core/src/{error.rs,poll.rs,session.rs,lib.rs}`, `client/{mod,classify,redundancy,restart,eam,projects,query,version}.rs`, `tests/{common/mod.rs,backup_contract.rs,eam_contract.rs,live_gateway.rs,live_rig_module_injection.rs}`, `tests/fixtures/live-rig/compose.yml`, `crates/ignition-cli/tests/{e2e_eam.rs,e2e_api_diagnostics.rs}`, `README.md:100-125,409`, `.github/workflows/ci.yml`
- `.planning/milestones/v1.1-phases/10-eam-write-operations/10-LIVE-CAPTURES.md`
- `/Users/pmannion/data/worktrees/whk-environment-orchestration/gateway-config-spec/docs/superpowers/specs/2026-10-09-prod-gateway-config-as-code-design.md` (section 3, 6)
- `.../spikes/gateway-modes/{lib.sh,mint-token.sh,docker-compose.yml,s1_config_precedence.sh,s3_redundancy.sh,s4_scale.sh,smoke.sh,fixtures/**}`, `results/{RIG,S1,S3,S4,SUMMARY}.md`
- `.../gateway-config-spec/docs/openapi.json` (jq queries; 737 paths) and `.planning/milestones/v1.0-phases/03-project-operations/openapi-8.3.6-phase3-extract.json` (401 paths)

### Secondary (MEDIUM confidence)
- User memory notes `ignition-83-platform-scan-api`, `ignition-83-mode-and-config-api-facts` (409 text, token header, prod 617 vs dev 745 paths, `/openapi.json` location): spike and prior-session facts, not re-verified here.

### Tertiary (LOW confidence)
- None used. No web search or Context7 query was needed: the domain is a proprietary gateway whose authoritative sources are the OpenAPI document and live captures. The `gsd-tools` research-plan, research-store and package-legitimacy seams were not run (no external providers are enabled in `config.json`; no packages are installed).

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH, no new dependency; all versions read from `Cargo.toml`.
- Architecture: HIGH on seams (file:line verified); MEDIUM on shapes pending D1..D10.
- Pitfalls: HIGH for the six items backed by repo evidence (500 vs 422, classify pins, unbounded echo, `Primary` places, 204 vs `.json()`, timeout vs acquire); MEDIUM for the foreign-scan race and bind-mount behaviour (cheap falsifiers given).

**Research date:** 2026-10-10
**Valid until:** 2026-11-10, or until the ephemeral image tag moves off 8.3.6.
