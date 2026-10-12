# Phase 20: Config API Client - Pattern Map

**Mapped:** 2026-10-11
**Files analyzed:** 17 (new/modified)
**Analogs found:** 15 / 17 (two partial: localhost-refusal guard, include_str! pin on a client file)

All paths are relative to `/Users/pmannion/whiskeyhouse/ignition-cli/`. Phase 20 must NOT create `crates/ignition-core/src/deploy/` (Phase 21 owns it).

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match |
|---|---|---|---|---|
| `crates/ignition-core/src/client/deploy_api.rs` (trait `DeployApi`, impl, `send_shaped`, `DEPLOY_ENDPOINTS`) | service (client trait) | request-response | `client/mod.rs` `post_json`/`put_json`/`send_and_classify` + `eam_task_modify` (mod.rs ~1713-1734) | role-match |
| `client/config_resources.rs` | model + path builders | request-response | `client/eam.rs:324-379` (`ModifyOutcome`/`ResourceChange`), `client/query.rs` | role-match |
| `client/scan.rs` (+ pure `lock_verdict`, `scan_complete`, `scan_and_wait`) | model + utility | polling | `client/restart.rs` + `src/poll.rs` | role-match |
| `client/mode.rs` (+ `normalize_deployment_mode`) | model + utility | request-response | `client/redundancy.rs` (lenient wire struct) + `client/version.rs` (`GatewayInfo`) | role-match |
| `client/classify.rs` (edit: scan-lock 409/408 arm, cap 422 echo) | middleware | request-response | itself, lines 80-228 | exact |
| `client/redundancy.rs` (edit: Master/Backup docs, `active_node`) | model + utility | transform | itself | exact |
| `client/mod.rs` (edit: 4 `pub mod` lines) | config | n/a | lines 35-60 | exact |
| `client/version.rs:24`, `client/status.rs:123`, `README.md:409` (edit: Primary -> Master docs) | doc | n/a | self | exact |
| `crates/ignition-cli/tests/e2e_api_diagnostics.rs:241-250` (edit: allowlist) | test | live | self | exact |
| `src/error.rs` (edit: +2 variants, four places) | model | n/a | `SessionNotPrunable` (route-scoped) and `EamTaskInFlight` | exact |
| `README.md:112` (exit-6 row) | config/doc | n/a | existing row | exact |
| `tests/deploy_client_contract.rs` | test | wiremock | `tests/backup_contract.rs`, `tests/eam_contract.rs:1130-1170` | exact |
| `tests/common/fake_deploy.rs` (+ `pub mod` in `common/mod.rs`) | test double | stateful | `tests/common/mod.rs` (`IgnitionMock`) | partial |
| `tests/live_deploy_api.rs` | test (`#[ignore]`) | live | `tests/live_rig_module_injection.rs`, `tests/live_gateway.rs` | role-match |
| `tests/fixtures/live-rig-deploy/compose.yml` | config | n/a | `tests/fixtures/live-rig/compose.yml` | exact |
| proptest invariants (in `client/scan.rs` tests or contract file) | test | transform | `tests/workspace_path_mapping.rs` | exact |
| structural no-clock pin on `client/scan.rs` | test | source scan | `tests/workspace_status_push.rs:235-262` | exact |
| `20-LIVE-CAPTURES.md` | doc | n/a | `.planning/milestones/v1.1-phases/10-eam-write-operations/10-LIVE-CAPTURES.md` | exact |

## Pattern Assignments

### `client/deploy_api.rs` (service, request-response)

**Auth site (the ONLY `Secret::expose`)** `client/mod.rs:600-616`. Every new request goes through `self.apply_auth(...)`; never add a second header site.
```rust
fn apply_auth(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    ...
        Some(Credential::Token(token)) => {
            request = request.header("X-Ignition-API-Token", token.expose());
```

**URL + test ctor** (mod.rs:578, 588-590): `ReqwestGatewayApi::for_tests(base, Some(cred))`; `fn url_for(&self, path:&str) -> url::Url` is private but reachable from a child module (`deploy_api` must be declared `mod`/`pub mod` inside `client/`).

**Send + classify tail** (mod.rs:904-915) is the base for `send_shaped`:
```rust
async fn send_and_classify(&self, request: reqwest::RequestBuilder, url: &url::Url)
    -> Result<reqwest::Response, CoreError> {
    let response = request.send().await.map_err(|err| CoreError::Network {
        url: url.to_string(), source: Some(err), observation: None,
    })?;
    classify::classify(response, url.as_ref(), false).await
}
```
Copy the transport-error mapping verbatim in the deploy helper (needed because the 500 branch must see the response BEFORE classify). `classify::classify` is `pub(crate)` and `mod classify;` is private (mod.rs:38) but children of `client` can call it.

**Which helper for which method** (the planner's typed-shape question):
- `post_json` (mod.rs:810-818): `async fn post_json<T: Serialize + ?Sized>(&self, path, body:&T) -> Result<reqwest::Response, CoreError>` — returns the Response, usable for `resource_create`, `scan_lock_acquire` (body `{acquireTimeout, holdTimeout}`), `scan_request`, `mode_create`. It uses the default 30 s client timeout; lock acquire needs a per-request `.timeout(acquire + slack)` so build the request directly: `self.apply_auth(self.client.post(url.clone()).json(&body).timeout(..))`.
- `put_json` (mod.rs:889-897) returns `Result<(), _>` and DROPS the body: do NOT use for `resource_update` (needs `{success, changes, problem}`).
- `get_json` (mod.rs:623-643) calls `.json()`: do NOT use for `scan_lock_info` (204).
- Write precedent to copy for `resource_update` (mod.rs ~1722-1733, `eam_task_modify`):
```rust
let url = self.url_for(&path);
let request = self.apply_auth(self.client.put(url.clone()).json(&items));
let response = self.send_and_classify(request, &url).await?;   // REPLACE with deploy helper for the 500 branch
let text = response.text().await.unwrap_or_default();
```

**500 branch (deploy-scoped, NOT in classify.rs)**: gate on `status == 500 && url.contains("/data/api/v1/resources/")` (same predicate as private `is_config_resource_url`, classify.rs:243-245; it is private to classify.rs, so either make it `pub(super)` or duplicate the one-line `contains`). On body containing `"signature mismatch"` return `CoreError::ResourceSignatureRejected { .. }` with body NOT echoed; else `Internal(format!(".. {}", truncate_api_body(&body)))`. Falling through goes to `classify::classify`. Rationale: `tests/eam_contract.rs:1135-1170` pins `exit_code()==1` and `code()=="internal"` for 500 signature mismatch; a classify arm would break it. Keep `cargo test -p ignition-core --test eam_contract` green.

**Body cap** `src/error.rs:69-91`:
```rust
pub const GATEWAY_CLIENT_BODY_CAP_BYTES: usize = 4096;
pub const GATEWAY_CLIENT_BODY_TRUNCATION_MARKER: &str = "... [truncated]";
pub fn truncate_api_body(body: &str) -> String { ... }
```
Every new `format!` containing a gateway body/string passes through this.

**Outcome type to reuse** `client/eam.rs:324-379`: `ModifyOutcome {success, changes: Vec<ResourceChange>, problem: Option<MutationProblem>}`, `ResourceChange {name, resource_type (rename "type"), collection, new_signature: Option<String> (rename "newSignature")}`. Lenient (`#[serde(default)]` everywhere). Re-export under a generic alias (`MutationOutcome`) rather than redefining. Treat 2xx with `success == false` as an error.

**Path segment helpers**: `client/projects.rs:92` `pub(crate) fn encode_segment` (NON_ALPHANUMERIC) for `{name}` only; module/type validated against `[A-Za-z0-9._-]` (dots in `com.inductiveautomation.eam` must not be encoded). `ListQuery`/`ListEnvelope<T>` in `client/query.rs:19-68` for list/names/modes.

**Wait engine for `scan_and_wait`** `src/poll.rs`:
```rust
pub async fn poll<T, S, F>(cfg: PollConfig, state: S, mut probe: F) -> Result<T, CoreError>
where F: for<'a> FnMut(&'a mut S) -> Probe<'a, T>
pub enum PollState<T> { Done(T), Pending(Option<String>) }
pub struct PollConfig { subject: String, interval: Duration, max: Duration, deadline: Duration }
```
Retries `Network`/`GatewayRestarting`, fails fast on `Auth`, deadline error is `CoreError::Network` with `source: None`. Probe is a `Pin<Box<dyn Future + Send>>`. Per research: probe tracks own `Instant`, returns `Done(NotComplete)` at its budget; poll deadline = budget + 2 intervals backstop.

**Trait style**: `#[async_trait::async_trait] pub trait DeployApi: Send + Sync` like `GatewayApi`; `impl DeployApi for ReqwestGatewayApi`. `Session` derefs to `Arc<ReqwestGatewayApi>` (session.rs:76-81) so `&*session` serves as `&dyn DeployApi`.

---

### `client/classify.rs` (edit; middleware, request-response)

**Arm order & route-scoped precedents** (classify.rs:80-228). Add the scan-lock arm AFTER the 404 arm, next to the other route-scoped CONFLICT arms. Copy this shape (classify.rs ~121-129):
```rust
S::CONFLICT if is_designer_prune_url(url) => Err(CoreError::SessionNotPrunable {
    id: designer_prune_id(url),
    endpoint: Some(url.to_string()),
}),
```
and the body-reading variant (EAM force arm, ~145-160: `let body = resp.text().await.unwrap_or_default(); ... html_error_parts(&body) ...`). New arm: `S::CONFLICT | S::REQUEST_TIMEOUT if is_scan_lock_url(url) => Err(CoreError::ScanLockHeld { actor: <parse from body "Lock already held by <actor>" else None>, kind, endpoint })`. Add `fn is_scan_lock_url(url:&str)->bool { url.contains("/data/api/v1/scan-lock/") }` beside `is_eam_url` / `is_config_resource_url` (classify.rs:~230-250). Never echo the body (cap if any).

**Cap the 422 echo (DAPI-06 gap)** classify.rs:181-185, change both `{joined}` and `{body}` interpolations:
```rust
Some(joined) => format!("gateway rejected the resource body (HTTP 422 from {url}): {joined}"),
None if !body.trim().is_empty() => format!("gateway rejected the resource body (HTTP 422 from {url}): {body}"),
```
to `crate::error::truncate_api_body(&joined)` / `truncate_api_body(&body)` (already used at classify.rs ~203 as `crate::error::truncate_api_body(&body)`).

Update the module-doc dispatch order list (classify.rs:1-30) for the new arm.

---

### `src/error.rs` (edit; four-place rule for `scan_lock_held` and `resource_signature_rejected`)

**Analog:** `SessionNotPrunable` (classify-constructed, route-scoped, exit 6) and `EamTaskInFlight`. Touch ALL of these locations (line numbers as of mapping; they shift as you edit, so grep the analog slug):

1. **Module-doc exit-6 row** (error.rs:15): append `` `scan_lock_held` (20-xx), `resource_signature_rejected` (20-xx) `` after `` `module_uninstall_denied` (19-01) ``.
2. **Variant** (analog error.rs:420-431 and 495-512):
```rust
/// ... Exit 6 — target state ...
#[error("designer session {id} is live — the gateway refused the prune")]
SessionNotPrunable {
    /// The Designer session id that was refused.
    id: String,
    /// URL of the refused DELETE, when known.
    endpoint: Option<String>,
},
```
Payloads: `ScanLockHeld { actor: Option<String>, kind: String, endpoint: Option<String> }`, `ResourceSignatureRejected { name: String, reason: SignatureReason, endpoint: Option<String> }`. Box any variant with 3+ Strings (`.planning/WINDOWS.md` entry 3; CoreError was exactly 128 bytes). Both payloads are small, but verify size test still passes. No dead variants: each must have a constructor in this phase.
3. **`code()`** (error.rs:~808-813): `Self::SessionNotPrunable { .. } => "session_not_prunable",` add `Self::ScanLockHeld { .. } => "scan_lock_held", Self::ResourceSignatureRejected { .. } => "resource_signature_rejected",`.
4. **`exit_code()`** (error.rs:~855-870): add both to the `| Self::SessionNotPrunable { .. } | ...` arm that returns 6.
5. **`hint()`** (error.rs:~1082):
```rust
Self::SessionNotPrunable { .. } => Some(
    "close the Designer first — prune removes stale entries only".to_string(),
),
```
Hint text must not name a nonexistent command (research gives proposed text).
6. **`endpoint()`** (error.rs:~1228-1242): add `| Self::ScanLockHeld { endpoint, .. } | Self::ResourceSignatureRejected { endpoint, .. }` to the `=> endpoint.clone()` or-pattern.
7. **`exit_code_mapping_enumerated`** case (error.rs:~1560-1572):
```rust
(
    CoreError::SessionNotPrunable {
        id: "d-live-1".into(),
        endpoint: Some("http://gw:8088/data/api/v1/designer/d-live-1".into()),
    },
    6,
    "session_not_prunable",
),
```
8. **`EXIT_SLUG_LITERALS`** (error.rs:~1735-1752): add `(6, "scan_lock_held"), (6, "resource_signature_rejected"),` in the exit-6 run.
9. **README exit table** `README.md:112`: append both slugs to the exit-6 cell (after `module_uninstall_denied`). `readme_exit_table_agreement` (error.rs:~1820, parses README via `include_str!("../../../README.md")`, checks both directions) fails otherwise. Optional: a hint-coverage test exists near error.rs:1984/2081 (per-variant Display/hint assertions); mirror it for the two new variants.

**Collision note (Pitfall 7):** Phase 21 adds `deploy_tree_invalid` and `embedded_secret` to the same lines. Land Phase 20's error edits as ONE early commit.

---

### `client/redundancy.rs` (edit; model + pure fn)

Existing wire struct (redundancy.rs:44-120): `RedundancyStatusWire { role: String, activity_level: Option<String> (rename "activityLevel"), peer_connected: bool, uptime: Option<i64>, last_sync_timestamp: Option<i64>, ... extra }`. `role` is a `String` on purpose. Edits:
- Doc lines 14-19 ("`lastSyncTimestamp` unit NOT capture-proven") and 22, 39, 182, 191 (`Primary`): replace with `Master`/`Backup`/`Independent`; activity levels `Active`/`Cold`; `lastSyncTimestamp` is epoch-ms (S3 spike); healthy backup `uptime: -1`.
- Test `redundancy_normalizes_and_passes_unknown_keys` (redundancy.rs:~182-191) uses `"role": "Primary"`; swap for the S3 verbatim bodies (`"role":"Master"` + `"activityLevel":"Active"`, and `"role":"Backup"` + `"Cold"`).
- Add `active_node(a: &RedundancyStatusWire, b: &RedundancyStatusWire) -> ActiveNode { OnlyA, OnlyB, Both, Neither }` comparing `activity_level.as_deref() == Some("Active")`; unit tests in the existing `#[cfg(test)] mod tests` (pattern at redundancy.rs:~111-204: `serde_json::from_value(serde_json::json!({...})).expect(...)`).
- Check grep gate: `! grep -n Primary crates/ignition-core/src/client/redundancy.rs`. Other `Primary` mentions: `client/version.rs:24`, `client/status.rs:123`, `README.md:409`, `crates/ignition-cli/tests/e2e_api_diagnostics.rs:241-250`.

---

### `client/mode.rs` (model + pure fn)

**Analog (lenient wire struct):** redundancy.rs `RedundancyStatusWire` recipe: `#[serde(rename = "...", alias = "snake", default)]` per field plus `#[serde(flatten)] extra: BTreeMap<String, serde_json::Value>`. `GatewayInfo` (`client/version.rs:20-47`) stays untouched (frozen `--json` output): add a private `GatewayModeProbe { #[serde(rename="deploymentMode", default)] deployment_mode: Option<String> }` and
```rust
pub fn normalize_deployment_mode(raw: Option<String>) -> Option<String> { raw.filter(|s| !s.is_empty()) }
```
Fetch via `get_json::<GatewayModeProbe>("/data/api/v1/gateway-info")`. Test with `null`, `""`, absent, `"production"`; `tests/gateway_info_contract.rs` must stay green.

---

### `client/scan.rs` (model + pure fns)

No exact analog for the lock/scan models; use `client/restart.rs` (POST then poll) and `redundancy.rs` lenient serde recipe. Pure fns per research Pattern 2 (`lock_verdict(obs, margin_s)`, `scan_complete(before, now)`); NO `SystemTime`/`Utc::now`/`Instant` in verdict fns. `ScanLockInfo.timestamp` is opaque, never read.

**Proptest analog (exact):** `tests/workspace_path_mapping.rs:27-28, 95-120`:
```rust
use proptest::prelude::*;
use proptest::string::string_regex;
proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]
    #[test]
    fn p1_round_trip_is_exact(member in valid_member_path()) {
        ... prop_assert_eq!(unescape_local(&local), member);
    }
    #[test]
    fn p2_injective_byte_wise(a in valid_member_path(), b in valid_member_path()) {
        prop_assume!(a != b, "distinct paths only — equal paths trivially map equal");
```
proptest is only used in an integration test today (no `src/` proptest), so put the `lock_verdict` invariant ("verdict independent of `timestamp` for any i64") either in an integration test or an in-`src` `#[cfg(test)]` module (proptest is a core dev-dep, so both compile).

**`include_str!` structural pin (exact analog):** `tests/workspace_status_push.rs:235-262`:
```rust
let source = include_str!("../src/actions/workspace.rs");
let prose = source.lines().map(|line| { let trimmed = line.trim_start();
    trimmed.strip_prefix("/// ").or_else(|| trimmed.strip_prefix("///")).unwrap_or(trimmed) })
    .collect::<Vec<_>>().join(" ");
let normalized = prose.split_whitespace().collect::<Vec<_>>().join(" ");
for phrase in [...] { assert!(normalized.contains(phrase), "...{phrase:?}"); }
```
Adapt for `client/scan.rs`: `include_str!("../src/client/scan.rs")`, strip the test module, assert the source contains none of `SystemTime`, `Utc::now`, `Instant::now` outside `scan_and_wait`'s documented budget probe (or keep the budget in a separate file so the pin can be absolute). Other `include_str!` precedents: `error.rs:1821` (README), `crates/ignition-cli/src/main.rs:4182`.

---

### `tests/deploy_client_contract.rs` (test, wiremock)

**Analog:** `tests/backup_contract.rs:1-60` (+ `eam_contract.rs:1130-1170`).
```rust
mod common;
use common::IgnitionMock;
use ignition_core::client::{GatewayApi, ReqwestGatewayApi};
use ignition_core::config::{Credential, Secret};

fn token_credential() -> Credential { Credential::Token(Secret::new("backup:tokengeneratedlive")) }

#[tokio::test]
async fn backup_download_streams_bytes_identical() {
    let mock = IgnitionMock::start().await;
    let guard = wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(BACKUP_PATH))
        .and(wiremock::matchers::query_param("type", "roaming"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_raw(fixture.clone(), "application/octet-stream"))
        .expect(1)
        .mount_as_scoped(&mock.server)
        .await;
    let api = ReqwestGatewayApi::for_tests(&mock.uri(), Some(token_credential()));
```
Add `use ignition_core::client::DeployApi;` (trait must be in scope for method calls).

**500 problem-body pin to copy for the new signature test** (`eam_contract.rs:1135-1160`): `ResponseTemplate::new(500).set_body_json(json!({"success": false, "changes": [], "problem": {"message": "DELETE illegal: signature mismatch for '...'", "stacktrace": [...]}, "references": null}))`. Use the 8.3.6 verbatim message (`MODIFY illegal: signature mismatch for 'ResourceId{resourcePath=..., collectionName=core}'`) and assert `code()=="resource_signature_rejected"`, `exit_code()==6`, and that the error Display/JSON contains no `problem.message` text.

**Harness helpers** (`tests/common/mod.rs`): `IgnitionMock::start()`, `.uri()`, `.server` (bespoke mocks), `list_json(method,path,body)`, `status_json(method,path,status,body)`, `html_error(method,path,status)`; each builder registers `.expect(1)` (checked on server drop). For the 200-then-204 lock-info pin use two `Mock`s with `.up_to_n_times(1)` (see research code example); wiremock matches newest-mounted first, so verify order. Header assertions via `mock.server.received_requests()`; confirm `X-Ignition-API-Token` equals the full `name:key`.

**Timing-sensitive tests** (`scan_and_wait` "200 but never advances"): real short deadlines (`PollConfig{ interval: 20ms, max: .., deadline: 300ms, subject }`), no tokio `test-util`.

**Cap tests:** 100 KB body through the 422 arm and the deploy 500 branch, assert `len <= 4096 + GATEWAY_CLIENT_BODY_TRUNCATION_MARKER.len()`.

---

### `tests/common/fake_deploy.rs` (test double, stateful)

Partial analog: `tests/common/mod.rs` (top-of-file `#![allow(dead_code)]` because it is a harness library; add `pub mod fake_deploy;` to `common/mod.rs`). No existing stateful fake gateway; implement `#[async_trait::async_trait] impl DeployApi for FakeDeploy` with interior `std::sync::Mutex` state (lock holder + remaining in injected ticks, scan counter, resource map with signatures). Reuse the SAME pure fns (`lock_verdict`, `scan_complete`) and the same typed errors as the real client. Differential scenario: `async fn scenario(api: &dyn DeployApi) -> Trace` run on fake always and on live under `#[ignore]`.

---

### `tests/live_deploy_api.rs` (test, `#[ignore]` live gate)

**Analog:** `tests/live_rig_module_injection.rs` (env gate at lines 15, 92-94, 295-299; ONE ordered test) and `tests/live_gateway.rs:36-60`.
```rust
#[ignore = "live: needs Docker and IGNITION_LIVE_RIG_MODULES=1"]
#[tokio::test]
async fn ... {
    if !<enabled>() { skip("... is not 1"); return; }
```
```rust
fn live_url() -> Option<String> {
    std::env::var("IGNITION_LIVE_URL").ok().filter(|value| !value.trim().is_empty())
}
fn skip(message: &str) { eprintln!("skipping: {message}"); }
```
Quiet skip when env absent (green no-op). Env for Phase 20: `IGNITION_LIVE_DEPLOY=1`, `IGNITION_LIVE_URL`, `IGNITION_LIVE_TOKEN` (full `name:key`). Credential: `Credential::Token(Secret::new(token))` via `ReqwestGatewayApi::for_tests` (live_gateway.rs:60).

**NO ANALOG for the localhost refusal.** No existing live test rejects non-localhost URLs (grep for `localhost|127.0.0.1|assert_local` finds only doc strings). New code: parse `IGNITION_LIVE_URL` with `url::Url`, `assert!(matches!(host_str(), Some("localhost"|"127.0.0.1"|"[::1]")))` at the top, before any request. The model is the spike's `assert_local` (`spikes/gateway-modes/lib.sh:9-12`). Rig-bringup via `DockerCompose` as in live_rig_module_injection.rs is optional: Phase 20 can instead document `docker compose up` against the fixture and let the test only consume the env URL.

Use one sequential test (§A.. sections; module-injection header explains why: Rust does not order tests, state carries). Use `holdTimeout` 3-5 s and finish with a cleanup scan (Pitfall 9).

---

### `tests/fixtures/live-rig-deploy/compose.yml` (config)

**Analog:** `tests/fixtures/live-rig/compose.yml:1-30` (single `gateway` service, pinned image tag, named volume, `ACCEPT_IGNITION_EULA=Y`, `GATEWAY_ADMIN_*`, `IGNITION_EDITION=standard`, `command: >\n -n <name>`). Use the research's reduced one-service spike variant (image `inductiveautomation/ignition:8.3.6`, `platform: linux/amd64`, port `18290:8088`, named volume `gw-data`). Copy `mint-token.sh` and `fixtures/security-*` from `spikes/gateway-modes/` in the whk-environment-orchestration worktree.

---

### `.planning/phases/20-config-api-client/20-LIVE-CAPTURES.md`

**Analog:** `.planning/milestones/v1.1-phases/10-eam-write-operations/10-LIVE-CAPTURES.md` (header table, verbatim block per observation, Decisions section, drift/limits). Capture FIRST (C1a-C1i, C3a, C-null, C-mode, C-list per RESEARCH "Capture matrix"); dependent code follows.

## Shared Patterns

### Auth header
**Source:** `client/mod.rs:600-616` (`apply_auth`). **Apply to:** every method in `deploy_api.rs`. Never call `Secret::expose` elsewhere.

### Classify-first, body cap
**Source:** `client/classify.rs` + `error.rs:82` `truncate_api_body`. **Apply to:** `deploy_api.rs`, `classify.rs` (422 echo), any `format!` that includes gateway text. Gate: `grep -n "text().await" crates/ignition-core/src/client/{deploy_api,config_resources,scan,mode}.rs` must each feed a capped path or a non-echoed decision.

### Never believe a status code
**Apply to:** all state-changing methods: success is a follow-up read. `scan_lock_info` decides 204 from `response.status()` BEFORE any body read; a 200 with an empty body fails loudly with `CoreError::Internal` (no body echoed).

### Four-place slug rule
**Source:** `error.rs` `SessionNotPrunable` footprint (see above). **Apply to:** both new variants.

### Leniency of wire models
**Source:** `redundancy.rs:44-120` (`rename` + `alias` + `default` + flatten extra). **Apply to:** `ScanLockInfo`, `ScanStatus`, `ModeRecord`, `ResourceRecord`. Do NOT enable `serde_json/preserve_order` or `arbitrary_precision`.

### CI gates (for every plan)
`cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo build --workspace`; `cargo test --workspace`; `cargo build -p ignition-cli --no-default-features`. MSRV 1.88, edition 2024; all doc items need `///` (core crate is documented to the standard of existing files).

## No Analog Found

| File / Concern | Role | Data Flow | Reason |
|---|---|---|---|
| Localhost-only refusal in `live_deploy_api.rs` | test guard | n/a | No existing live gate refuses non-local URLs; write fresh (spike `lib.sh:9-12` is the model). |
| `FakeDeploy` stateful gateway | test double | stateful | Only stateless wiremock builders exist (`IgnitionMock`); write fresh. |
| `proptest` inside `src/` | test | transform | Only `tests/workspace_path_mapping.rs` uses proptest today; use that style. |

## Metadata

**Analog search scope:** `crates/ignition-core/{src,src/client,tests,tests/common,tests/fixtures}`, `crates/ignition-cli/tests`, `README.md`
**Files scanned/read:** about 14 (mod.rs, classify.rs, error.rs, redundancy.rs, eam.rs, poll.rs, tests/common/mod.rs, backup_contract.rs, eam_contract.rs, live_gateway.rs, live_rig_module_injection.rs, workspace_path_mapping.rs, workspace_status_push.rs, live-rig/compose.yml)
**Pattern extraction date:** 2026-10-11
**Caveat:** line numbers in error.rs, mod.rs and classify.rs are as of mapping; grep the analog slug (`SessionNotPrunable`, `session_not_prunable`) to relocate after edits.
