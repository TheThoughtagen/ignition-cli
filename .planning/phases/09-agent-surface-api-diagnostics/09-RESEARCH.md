# Phase 9: 09-agent-surface-api-diagnostics - Research

**Researched:** 2026-09-06
**Domain:** Raw REST passthrough command + curated diagnostics reads (Ignition 8.3 gateway API) over the existing Rust client seam
**Confidence:** HIGH (all wire shapes from the repo's designated ground truth: the 83-api Bruno/Postman collection + live-captured 8.3.6 OpenAPI extracts; all code patterns verified in-tree)

> **No CONTEXT.md exists** (no `/gsd-discuss-phase` was run). There are no locked user decisions beyond the roadmap's stated flags, which are treated as **directives** (quoted verbatim where they constrain design).

---

## Summary

Phase 9 lands two command families. **EXT-01** is `ign api call` — a generic passthrough for any gateway REST endpoint with method/path/body/header control, envelope-wrapped output whose `data` is the gateway's verbatim JSON (the one new documented contract exception), and a **catch-all 4xx→exit-2 classification carrying the verbatim body** so an unclassified gateway rejection never renders as the internal-error exit-1 storm. **EXT-02** is four curated read families for the morning gateway check: `license status`, `redundancy status`, `gan status`, and the diagnostics-bundle generate/download/wait loop — each live-verified against both rigs (8.3.3 + 8.3.6).

Every ingredient already exists in the repo as an established pattern: the `classify()` pipeline (client/classify.rs) is where the api-call catch-all arm goes; the additive-slug mechanism (Phase 7/8 precedent) is how the new exit-2 slug enters the frozen 1–7 taxonomy; `apply_auth` (client/mod.rs:430) is the ONE auth-header site the passthrough must route through (user-supplied auth-pattern headers are refused at the CLI layer — `webdev_route_call`'s `extra_headers` plumbing is the precedent); `get_bytes`/`download_to_file` + `PollState/PollConfig` are the bundle download and wait patterns; Session::resolve (Phase 8) is the construction seam. The 83-api collection supplies the exact endpoints: `GET /data/api/v1/licenses`, `GET /data/api/v1/redundancy`, `GET /data/api/v1/overview/gan`, and the `/data/api/v1/diagnostics/bundle/{generate,status,download}` trio (schemas already captured in the Phase-2 OpenAPI extract).

Two structural constraints shape planning beyond the happy path: (1) **the TUI coverage clap-tree walk is CI-enforced** — every new clap leaf needs a `routes.rs` row the same phase, and the pinned OutOfBand set (`["completions"]` + reserved `mcp`/`lsp`/`edit`) must be deliberately extended if `api` maps out-of-band; (2) **the Three-Place slug rule is executable** — the new exit-2 slug must land in enum + literal table + README exit table together or `readme_exit_table_agreement` fails, and the contract-exception write-up belongs in README §"Output contract (for agents)" in the same task.

**Primary recommendation:** Build the catch-all classifier arm and its exit-2 slug FIRST (roadmap directive: "catch-all classifier is the FIRST task before the happy path"), wire `api call` through `apply_auth` + `url_for` with header-refusal at parse time, then land the four curated reads as thin GatewayApi capability methods over version-tolerant wire models, and close with env-gated read-only live gates run against both docker rigs using the Phase-4 headless provisioning recipe.

---

## Wire Shapes (from the 83-api collection + live OpenAPI — the ground truth)

Mined 2026-09-06 from `~/whiskeyhouse/83-api/bruno/Ignition HTTP API/` and `.planning/phases/02-gateway-health-inspection/openapi-8.3.6-phase2-extract.json` (captured live from an 8.3.3 rig, `/openapi.json`). Base path for all: `{{baseUrl}}` = profile URL, e.g. `http://host:8088`. Auth: `X-Ignition-API-Token: name:key` header (already handled by `apply_auth` — see Pattern 2).

### EXT-02 curated reads

| Command (proposed) | Endpoint | Method | Response shape (documented) | Notes |
|---|---|---|---|---|
| `license status` | `/data/api/v1/licenses` | GET | `{hardware: [{key, items: [{name, title, version, details: {…deep…}}]}], …}` (Postman sample shows deep nesting — likely a `leased` sibling array too) | **Deeply nested, point-release-variable.** Strong candidate for version-tolerant parse (deny_unknown_fields OFF) or passthrough-shaped data. Live capture REQUIRED on both rigs. |
| `license status` (companion) | `/data/api/v1/trial` | GET | `{trialSecondsLeft, expired, …}` | **Already implemented** — `client/trial.rs::TrialWire`, live-proven in Phase 4. Reuse. |
| `redundancy status` | `/data/api/v1/redundancy` | GET | `{role, projectState, activityLevel, localId, peerConnected: bool, peerId, hasConfigAccess: bool, syncPending: bool, failoverPending: bool, uptime: number, lastSyncTimestamp: number}` | Flat, clean. `role` values `"Independent"/"Primary"/"Backup"` (Independent already seen in `GatewayInfo.redundancy_role`). `uptime`/`lastSyncTimestamp` units unverified — live capture. Family also has `/redundancy/config` (GET/PUT), `/redundancy/events`, `/redundancy/providers` (both `{items, metadata}` list envelopes), and WRITE routes `/gwaction/failover`, `/gwaction/resync` — NOT in scope (no curated write this phase; `api call` covers them). |
| `gan status` | `/data/api/v1/overview/gan` | GET | `{totalConnections, runningConnections, outgoingByteRate, incomingByteRate, remoteGateways}` (all numbers) | Verified against live OpenAPI extract (all `x-ignition-non-secret: false`). Zero-connection shape on a non-GAN gateway needs live confirmation. Companion candidates (planner discretion): `GET /data/api/v1/gateway-network/gateways` (`{items, metadata}` paginated list of remote gateways). |
| diagnostics generate | `/data/api/v1/diagnostics/bundle/generate` | POST (no body) | 200 `{state: string}` | Kicks off server-side generation. |
| diagnostics status | `/data/api/v1/diagnostics/bundle/status` | GET | 200 `{state: string, fileSize: integer}` | **The `state` enum values are NOT in the OpenAPI spec or the collection** — must be captured live (expected something like NOT_STARTED/GENERATING/READY; do not guess). |
| diagnostics download | `/data/api/v1/diagnostics/bundle/download` | GET | raw bytes (no schema in spec) | ZIP. Rides the `get_bytes`/`download_to_file` pipeline. `Content-Disposition` filename may be absent (LogDownload fields are already Option). |

**8.3.x variance warning (roadmap directive: "8.3.x point-release variance is the documented failure mode"):** the OpenAPI extracts were captured on 8.3.3 and the Postman samples are doc-generated placeholders. Every curated model must use `#[serde(default)]` on every optional field, never `deny_unknown_fields`, and — where a field's type could plausibly vary (number-as-string) — parse tolerantly. The repo precedent is `GatewayInfo`'s `#[serde(rename = "ignitionVersion", alias = "version")]` and `Overview`'s `#[serde(flatten)] extra: serde_json::Value`-style passthrough (client/status.rs:68).

### EXT-01 `api call` surface

Any `/data/...` path is fair game (the collection's 92 endpoint families are the coverage reference, not a checklist). The passthrough contract pins from the success criteria:

1. `ign api call --method GET --path /data/api/v1/gateway-info [--header k:v]... [--data ...]` → success envelope `{ok: true, profile, data: <gateway JSON verbatim>}`.
2. **Documented contract exception:** `data` is the gateway's own JSON, NOT a curated model — write this into README §"Output contract (for agents)" during this phase (that section already documents the completions stdout exception; this is the same genre).
3. Unclassified 4xx → exit-2 class with verbatim body. Never `CoreError::Internal` (exit 1).
4. User-supplied auth-pattern headers (`Authorization`, `X-Ignition-API-Token`, `Cookie`) are REFUSED — auth comes from the profile, full stop.

---

## Standard Stack

No new dependencies. The phase is expressible entirely with what's in the tree (constraint: "keep the dependency tree lean — held: zero heavy deps added beyond plan").

### Core (in-tree, verified)

| Library/module | Version | Purpose | Why |
|---|---|---|---|
| `reqwest` | workspace | HTTP + `Method` enum + per-request timeout | Already the client; `RequestBuilder::method()` supports arbitrary verbs (GET/POST/PUT/DELETE/PATCH/HEAD) |
| `serde_json::Value` | workspace | gateway-verbatim `data` for api call | Passthrough precedent: `Overview`'s flatten; `webdev_route_call` returns `serde_json::Value` today |
| `clap` (derive) | workspace | `Api(ApiArgs)` + `License`/`Redundancy`/`Gan`/`Diagnostics` families | Existing tree in cli.rs |
| `wiremock` | workspace (dev) | contract fixtures for every classification arm | `IgnitionMock` harness in ignition-core/tests/common/mod.rs |
| `assert_cmd` | workspace (dev) | binary-level contract tests via `IGNITION_CLI_CONFIG` tempfile isolation | `version_gateway_contract.rs` pattern |
| `Session` (core/src/session.rs) | Phase 8 | construction seam — `Session::resolve()` → `Arc<ReqwestGatewayApi>` | The ONLY sanctioned construction path (Phase 8 decision) |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|---|---|---|
| A dedicated api-call classifier arm inside `classify()` | A fully separate api-call request pipeline that bypasses `classify()` | Bypassing forks the pipeline rule ("Nothing ever calls `.json()` on a response that skipped `classify()`" — client/mod.rs:30). Keep ONE pipeline; add a path-scoped/method-agnostic catch-all arm or a classifier parameter. |
| Curated nested model for `/licenses` | Version-tolerant partial model + passthrough remainder | Full curated model of the deep license tree is high-variance/high-maintenance; the roadmap's version-tolerance directive favors partial-curated (the fields the morning check needs: mode/edition/count) + flatten passthrough. |
| New exit code for gateway 4xx | Additive slug riding exit 2 | **The 1–7 taxonomy is frozen** (error.rs:14 "never a new exit code"). Additive slug is the Phase-7/8 mechanism. |

---

## Architecture Patterns

### Where things land (existing tree, no new crates)

```
crates/ignition-core/src/
├── client/
│   ├── mod.rs              # GatewayApi trait: ADD raw_call + 4 capability methods; ReqwestGatewayApi impl
│   ├── classify.rs         # ADD the api-call catch-all 4xx arm (or a classifier param — see Pitfall 1)
│   ├── apicall.rs (new)    # raw-call request model + header-refusal helper (or in mod.rs)
│   ├── license.rs (new)    # /licenses + /trial reuse models
│   ├── redundancy.rs (new) # /redundancy status model
│   ├── gan.rs (new)        # /overview/gan (+ optional /gateway-network/gateways)
│   └── diagnostics.rs (new)# bundle {state, fileSize} model + paths
├── actions/
│   ├── apicall.rs (new)    # api call action (parse-guarded, envelope-wraps verbatim data)
│   ├── license.rs (new)    # curated license status output model
│   ├── redundancy.rs (new)
│   ├── gan.rs (new)
│   └── diagnostics.rs (new)# generate/status/download/wait orchestration over PollConfig
crates/ignition-cli/src/
├── cli.rs                  # new Commands variants + Args structs
├── main.rs                 # dispatch arms (guard BEFORE client construction where applicable)
└── render.rs               # ActionOutput variants + table rendering
crates/ignition-cli/tests/
├── contract_api.rs (new)   # binary-level: envelope, verbatim data, 4xx→exit-2, header refusal
├── contract_diagnostics.rs (new) etc.
└── e2e_api_diagnostics.rs (new)  # env-gated live gate (read-only)
crates/ignition-tui/src/routes.rs  # MANDATORY: rows for every new leaf (tui_coverage walk)
```

### Pattern 1: The catch-all 4xx arm — additive slug on the usage class (exit 2)

**What:** A new `CoreError` variant (roadmap: "catch-all classifier is the FIRST task before the happy path") that classify's final fallback maps to **when the request came from the api-call path** and the status is 4xx — carrying the verbatim body.

**The taxonomy math (verified):** exit 2 is the *usage* class (`confirmation_required`, `invalid_import_file`, `invalid_input`). A 4xx is the *caller's* problem — wrong path, bad body, insufficient target — so exit 2 is semantically right and matches the success criterion's "exit-2 class". Existing arms keep their meanings even on the api-call path: 401/403 → Auth (exit 5), 404 → NotFound (exit 6), 503 → GatewayRestarting (exit 6). Only statuses that today fall through to `Internal` (e.g. 400, 405, 409, 415, 422-on-non-resource-paths) get the catch-all. "Unclassified" in the criterion means exactly this fall-through set.

**Slug naming:** propose `gateway_client_error` (or `api_gateway_rejected`) — planner's call, but the Three-Place rule makes it a one-shot decision: the new variant + `code()` arm + `exit_code()` arm (error.rs:448/488), the literal `(exit, slug)` test table (the `exit_code_mapping_enumerated` test), AND the README exit table row — all in the same task, or `readme_exit_table_agreement` (include_str! README parse, both-directions cross-check) fails CI. Add the README exit-table edit to the SAME task as the enum change.

**Scoping decision the planner must make:** how the classifier knows a response is api-call traffic. Two verified options:
- (a) A `raw: bool` (or enum) parameter threaded through a new `send_and_classify_for_api(...)` — the classifier gains one arm keyed on the flag. Keeps `classify()` single-pipeline.
- (b) A dedicated passthrough method that calls `classify()` first, then re-maps only the `CoreError::Internal` variant it receives back when the original status was 4xx (requires classify to retain the status in the Internal message — it already embeds `unexpected HTTP {status} from {url}`, but that's stringly; option (a) is cleaner).

Recommendation: (a) — an explicit parameter beats parsing error strings.

```rust
// Source: client/classify.rs dispatch order + error.rs additive-slug precedent (poll_interval_too_small)
// New variant — rides exit 2, own slug, carries the verbatim gateway body:
/// The gateway answered the api call with a 4xx this CLI does not
/// curate — the caller's request is the problem, and the body is
/// theirs to read. Exit 2 (usage class; additive slug).
#[error("gateway rejected the api call (HTTP {status} from {endpoint}): {body}")]
GatewayClientError {
    status: u16,
    endpoint: String,
    /// The response body VERBATIM (truncated at a documented cap if
    /// huge — decide the cap; e.g. 4 KiB — and pin it in the contract test).
    body: String,
},
// code() => "gateway_client_error"; exit_code() => 2
```

### Pattern 2: Routing the passthrough through apply_auth + refusing auth-pattern headers

**What:** `api call` builds its request exactly like every capability method — `url_for(path)` → user headers (minus refused ones) → `apply_auth` → `send_and_classify`. The refusal is a CLI/action-level parse check, NOT a header-strip: refuse loudly (`CoreError::InvalidInput`, exit 2, hint naming the profile/auth rule) before any network I/O.

**Why at the action layer:** `apply_auth` is the ONE place `Secret::expose` is called (redaction boundary, CORE-02 — client/mod.rs:12). A user-supplied `X-Ignition-API-Token` must never be silently overwritten (silent override leaks the wrong credential's behavior) nor double-sent (two auth headers = undefined gateway behavior). Refusing makes the contract honest.

**Header plumbing precedent (verified):** `webdev_route_call(_project, _route, _body, extra_headers: &[(&str, &str)])` already accepts user headers and loops `request = request.header(name, value)` (client/mod.rs:643-648). Reuse the shape.

```rust
// Source: main.rs guard precedent (require_confirmation before ANY API construction, main.rs:617-627)
// + client/mod.rs apply_auth doc (the ONE Secret::expose site)
const REFUSED_AUTH_HEADERS: [&str; 3] = ["authorization", "x-ignition-api-token", "cookie"];

fn refuse_auth_headers(headers: &[(String, String)]) -> Result<(), CoreError> {
    for (name, _) in headers {
        if REFUSED_AUTH_HEADERS.contains(&name.to_ascii_lowercase().as_str()) {
            return Err(CoreError::InvalidInput {
                reason: format!(
                    "header {name:?} is auth-pattern and refused — credentials come \
                     from the profile (X-Ignition-API-Token is applied by ign itself)"
                ),
            });
        }
    }
    Ok(())
}
```

### Pattern 3: Version-tolerant curated models (the diagnostics-slice directive)

**What:** Every new wire model: all optional fields `#[serde(default)]` + `skip_serializing_if`, no `deny_unknown_fields`, flatten-passthrough remainder where the shape is deep/variable (licenses), explicit `Option` for anything not live-captured on BOTH rigs.

```rust
// Source: client/version.rs GatewayInfo (rename+alias, default, skip) and client/status.rs Overview
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedundancyStatus {
    /// "Independent" / "Primary" / "Backup"
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_state: Option<String>,     // wire key: projectState — rename camelCase
    #[serde(rename = "peerConnected", alias = "peer_connected", default)]
    pub peer_connected: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime: Option<i64>,               // unit UNVERIFIED — capture live before pinning a comment
    #[serde(rename = "lastSyncTimestamp", alias = "last_sync_timestamp", default, skip_serializing_if = "Option::is_none")]
    pub last_sync_timestamp: Option<i64>,
    // … remaining documented fields all default/Option …
}
```

The repo's own history is the cautionary tale: the missing `expirationDate` rename silently DROPPED a live field on parse (client/version.rs comment on `LicenseInfo`), and `ignitionVersion` needed an alias for older 8.3.x. Every camelCase wire key gets `rename` + snake_case `alias` + `default`.

### Pattern 4: The bundle wait loop over PollConfig

**What:** `diagnostics bundle wait` polls `/diagnostics/bundle/status` until the state leaves the generating set (states captured live first — Pitfall 2), riding the existing poll machinery.

```rust
// Source: poll.rs PollState/PollConfig + actions/restart.rs restart_and_wait signature
pub async fn bundle_wait(
    api: &dyn GatewayApi,
    interval: Duration,   // cli.rs precedent: --interval default 2
    timeout: Duration,    // default e.g. 300 like restart
) -> Result<BundleStatus, CoreError>
```

Deadline expiry → `CoreError::Network { url, source: None }` (the documented poll-deadlock convention — error.rs:107-113: `source: None` MARKS a poll deadline, same slug `network_error`).

### Anti-Patterns to Avoid

- **Do not hook clap** for the api-call argument validation (auth-header refusal etc.) — render it as a contract error envelope. The frozen rule: "clap renders its own usage errors — never hook clap" (error.rs:11).
- **Do not `.json()` before `classify()`** — the pipeline rule is absolute; a 4xx HTML Jetty page through `.json()` is the original Phase-2 bug.
- **Do not add envelope fields** for download progress or bundle state — everything rides `data`; the LOCKED envelope never grows top-level fields (actions/version.rs:11-13).
- **Do not pre-declare OutOfBand rows** for commands that don't exist yet — orphan rows fail the clap walk BY DESIGN (routes.rs:26-29). Rows and clap commands land together, in the same task.
- **Do not guess bundle `state` values** from web memory — the OpenAPI says only `string`. Capture live, then encode.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---|---|---|---|
| HTTP verb dispatch/arbitrary methods | A method-string router | `reqwest::Client::request(Method, url)` (`RequestBuilder::method`) | reqwest owns verb semantics; HEAD/DELETE parity free |
| Auth header application | A second header-apply site in the api-call path | `apply_auth` via the existing pipeline | Redaction boundary (CORE-02) is grep-audited to ONE site |
| Error classification | A parallel classifier for api calls | The `classify()` pipeline + one new arm | "Nothing ever calls `.json()` on a response that skipped classify()" — forking it reintroduces the Jetty-HTML-through-json bug |
| File download (bundle) | Chunked writer from scratch | `download_to_file` pipeline (client/mod.rs:506+, classify-first) or `get_bytes` (LogDownload) | Timeout-per-request + Content-Disposition handling already solved |
| Poll/wait loop | Custom sleep-poll | `PollState`/`PollConfig` (poll.rs) + `restart_and_wait` shape | Deadline → `Network{source:None}` convention is contract-pinned |
| List endpoints (gateways/events/providers) | Ad-hoc pagination parsing | `ListQuery` + `ListEnvelope<T>` (client/query.rs) | The `{items, metadata}` shape + `-1`=all convention is standard across the client |
| Mock gateway | Hand-rolled test server | `IgnitionMock` (ignition-core/tests/common/mod.rs) | Builders for list_json, html_error, redirect, status_json, literal_true already speak gateway shapes |
| Exit-table sync | Manual README/enum review | The existing two tests (`exit_code_mapping_enumerated`, `readme_exit_table_agreement`) | They're the enforcement — just add the row everywhere in one task |

**Key insight:** Phase 9 adds almost zero new machinery — it is composition of the Phase-1/2/4/8 skeleton plus one classifier arm. The risk is concentrated in (a) the wire shapes' point-release variance and (b) the contract surfaces (slug, README, TUI rows), not in HTTP plumbing.

---

## Common Pitfalls

### Pitfall 1: The api-call arm leak into curated commands
**What goes wrong:** Making the catch-all 4xx→exit-2 mapping unconditional in `classify()` would reclassify e.g. a 400/409/422 from CURATED commands (today honestly `Internal` exit-1 "report as a bug" for shapes we don't understand) into usage errors, muddying the contract for v1.0-era commands.
**Why it happens:** classify is a shared chokepoint.
**How to avoid:** Scope the arm to api-call traffic (explicit parameter or dedicated send path, Pattern 1). Existing route-scoped arms (designer-prune 409, EAM-force 409, config-resource 422) keep their exact behavior — the phase must not move them.
**Warning signs:** any existing contract test that asserted `internal`/exit-1 changing slug.

### Pitfall 2: Guessing the bundle state machine
**What goes wrong:** Coding `wait` against imagined state values ("RUNNING"/"DONE") — real gateway states are uncaptured; 8.3.3 vs 8.3.6 may differ (the documented failure mode).
**How to avoid:** First live task = curl `generate` then poll `status` on BOTH rigs, capture state strings + timing verbatim into the plan/research addendum, THEN encode. Keep the waiter tolerant: treat any captured non-generating state as terminal, treat unknown states honestly (keep polling until timeout; the final status body rides `data`).
**Warning signs:** a state literal in code that isn't backed by a captured response in the repo.

### Pitfall 3: Header-name normalization
**What goes wrong:** Comparing user header names case-sensitively (`authorization` vs `Authorization`) lets the refused header through; HTTP headers are case-insensitive.
**How to avoid:** `to_ascii_lowercase()` before the refusal check (Pattern 2). Also refuse whitespace-prefixed variants by trimming.
**Warning signs:** a contract test with only the canonical capitalization.

### Pitfall 4: Path handling surprises
**What goes wrong:** `--path` starting without `/` (relative join lands somewhere unexpected), or containing a full URL (`http://other-host/...`) turning the CLI into a proxy; query strings embedded in `--path` vs separate `--query` flags double-encoded.
**How to avoid:** Decide and pin in the contract test: require a leading `/` (InvalidInput otherwise), reject absolute URLs to other hosts (`url_for`'s `join` semantics would silently rebase — see client/mod.rs:416-420), and decide ONE query mechanism (recommend: `--query k=v` repeatable flags via reqwest `.query(&pairs)` like `get_json`, not embedding `?` in path — or document verbatim path passthrough; either is fine, just pinned and tested).
**Warning signs:** a test asserting behavior for a path missing its leading slash.

### Pitfall 5: TUI coverage walk failure at the end of the phase
**What goes wrong:** All CLI+core tasks green; `tui_coverage` fails CI because `api`, `license`, `redundancy`, `gan`, `diagnostics` leaves have no routes.rs rows; worse, the pinned `out_of_band_rows_are_exactly_the_completions_leaf` test needs a deliberate edit if `api` maps OutOfBand.
**How to avoid:** Treat routes.rs as a deliverable of the SAME task that adds the clap commands. Planner decides the mapping up front: recommendation — curated reads → `Mapping::Screen(Screen::Dashboard)` (they are morning-check verbs, dashboard-shaped) or a new `Screen::Diagnostics` (bigger TUI task); `api call` → `Mapping::OutOfBand` with an extended pinned test + justification comment (raw passthrough is not a cockpit verb — same genre as completions). Screen-count changes ripple into ui/ — a new screen is a meaningfully larger task.
**Warning signs:** a plan task adding clap commands without a routes.rs edit in the same file-touch list.

### Pitfall 6: Verbatim body size in the exit-2 envelope
**What goes wrong:** A gateway error page can be kilobytes; stuffing it verbatim into the failure envelope message makes agent output unreadable and goldens unstable.
**How to avoid:** Pin a documented truncation cap (e.g. 4 KiB, with an explicit `… [truncated]` marker) in the variant + contract test, and document it beside the contract exception in README.
**Warning signs:** golden-file churn on any long error body.

### Pitfall 7: The live gate mutating the rig
**What goes wrong:** `api call`'s live e2e defaults into exercising a destructive route (or someone points it at a production WHK gateway); "passthrough can't nuke the rig" is an explicit roadmap directive.
**How to avoid:** The api-call live gate exercises ONLY read-only paths (e.g. GET gateway-info, GET overview/gan) and requires NO mutations env; bundle `generate` is a server-side resource hog — gate its live run behind the existing `IGNITION_LIVE_MUTATIONS=1` convention (webdev precedent) since it is a real (if benign) mutation. Never point live gates at shared/production gateways; both rigs are docker rigs you spin (Pattern 5).
**Warning signs:** an e2e gate that POSTs anything without a mutations env check.

### Pitfall 8: Diagnostics download size vs the 30s client default
**What goes wrong:** Bundles are MBs; the default 10s/30s client timeouts (client/mod.rs:395) can truncate mid-download.
**How to avoid:** Per-request `RequestBuilder::timeout` override like `get_bytes(timeout)` / the backup pipeline (`BACKUP_TIMEOUT` in client/backup.rs) — a large archive must not be truncated (the 02-04 lesson, in-code).
**Warning signs:** intermittent `Network` errors on download in live gates only.

---

## Code Examples

Verified patterns to reference in task actions:

### Existing raw-Value passthrough (webdev_route_call — the closest ancestor of `api call`)
```rust
// Source: crates/ignition-core/src/client/mod.rs:628-650
async fn webdev_route_call(
    &self, project: &str, route: &str,
    body: &serde_json::Value, extra_headers: &[(&str, &str)],
) -> Result<serde_json::Value, CoreError> {
    let url = …;
    let mut request = self.apply_auth(self.client.post(url.clone()).json(body));
    for (name, value) in extra_headers {
        request = request.header(*name, *value);
    }
    let response = self.send_and_classify(request, &url).await?;
    response.json::<serde_json::Value>().await.map_err(|err| …)
}
```
`api call` generalizes this: arbitrary `Method`, caller path, optional body, header list, GET-query pairs, and the api-call classifier arm.

### Binary-level contract test skeleton (binary 'ign' over wiremock)
```rust
// Source: crates/ignition-cli/tests/version_gateway_contract.rs (isolation + spawn shape)
fn isolated_config() -> (tempfile::TempDir, PathBuf) { /* IGNITION_CLI_CONFIG → tempfile with [profiles.dev] url = mock */ }
// then: Command::cargo_bin("ign")… .env("IGNITION_CLI_CONFIG", config)… .args(["api","call","--json","--method","GET","--path","/data/api/v1/gateway-info"])
// assert: exit code, stdout envelope byte-shape, data == mounted JSON verbatim (field order preserved by serde_json::Value round-trip — verify this assumption in the first task)
```
Note for the planner: `serde_json::Value` is a BTreeMap-backed object — **key ORDER of gateway-verbatim data may reorder vs the wire bytes**. If "verbatim" means byte-identical, the api-call path must capture the raw body text and embed it as a raw JSON value (`serde_json::value::RawValue` / `serde_json::value::to_raw_value`) instead of parsing to `Value`. This is a contract decision to make explicitly in the first task: recommend `RawValue` (parse-free passthrough, byte-order preserved, still renders inside the envelope) with a fallback parse-check that it's valid JSON.

### Live-gate skeleton (env-gated, quiet green no-op)
```rust
// Source: crates/ignition-cli/tests/e2e_webdev.rs:57-73 + live_gateway.rs rig recipe
#[ignore = "opt-in e2e: set IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN (…+ MUTATIONS for generate)"]
#[tokio::test]
async fn license_status_live() {
    let Some(url) = live_url() else { return };            // quiet no-op without envs
    let token = live_token().expect("…");
    // assert envelope: ok==true, data carries the captured wire shape
}
```

---

## Live-Gate & Rig Recipe (established, Phase-2/4 verified)

| Item | Value |
|---|---|
| Rig images | `inductiveautomation/ignition:8.3.6` and `:8.3.3` (docker; Phase 4 used 18188 and 19188 respectively) |
| Commissioning | headless recipe from Phase 4 (04-VERIFICATION addendum): `GATEWAY_ADMIN_PASSWORD=password` env, web commission, then **headless token provisioning** (04-USER-SETUP.md superseded notes: `collection:"core"` + security-properties permissions patch) — makes gates fully automatable |
| Gate env vars | `IGNITION_LIVE_URL`, `IGNITION_LIVE_TOKEN` (full `name:key`); `IGNITION_LIVE_MUTATIONS=1` for bundle-generate (Pitfall 7) |
| Invocation | `cargo test -p ignition-cli --test e2e_api_diagnostics -- --ignored` (no envs = green no-op) |
| Both-rigs rule | Run every live test against BOTH versions; record verbatim captures (state strings, license nesting, timestamp units) in the phase summaries |

Phase-9's live matrix (from success criteria): each curated read answers with live truth on both rigs; bundle generate→wait→download round-trip on both rigs (download verified non-empty + zip magic); api-call gate proves envelope + one deliberate 4xx path (exit 2 + verbatim body) read-only.

---

## State of the Art (repo-internal)

| Old Approach | Current Approach | When Changed | Impact for P9 |
|---|---|---|---|
| `resolve_profile_context` + `resolve_gateway_api` duplicated in main.rs/TUI | `Session::resolve/resolve_degraded/for_url` (concrete `Arc<ReqwestGatewayApi>`) | Phase 8 (2026-09-06) | New dispatch arms call `Session::resolve(profile_flag)`; do NOT resurrect the pre-8 pattern |
| Positional `(String, String, Arc)` context triple | `ResolvedContext` struct (poll_interval typed) | Phase 8 (08-05) | TUI-side wiring follows it (only matters if a new TUI screen is added) |
| Slug growth by convention | Executable Three-Place rule (README parsed via include_str!) | Phase 8 (08-06) | New exit-2 slug = enum + literals + README row in ONE task |
| OutOfBand set = completions only | Reserved mcp/lsp/edit pre-declared (zero rows) | Phase 8 | `api` is NOT pre-reserved — its row lands NOW with its clap command |
| Curated models strict | Version-tolerant directive for the diagnostics slice | Roadmap flag | deny_unknown_fields stays OFF; optional fields explicit |

---

## Open Questions (for the planner)

1. **`api call` verbatim = byte-verbatim or value-verbatim?**
   - Known: `serde_json::Value` reorders object keys (BTreeMap); RawValue preserves bytes.
   - Recommendation: `RawValue` passthrough + validity check; pin the decision in the first contract test (agents likely don't care about key order, but "gateway-verbatim" in the success criterion should mean *no field dropped, no value coerced* — that's guaranteed either way; pick one and document in README's contract exception).
2. **Command topology.** Roadmap names `ign api call`, `license status`, `redundancy status`, `gan status`, "diagnostics bundle". Recommendation: four new top-level families (`Api`, `License`, `Redundancy`, `Gan`, `Diagnostics`) — matches the morning-check verbs and leaves room (e.g. `license` could later gain nothing; `diagnostics` gains threads/deadlocks reads for free as family siblings). Alternative: one `Diagnostics` super-family — rejected: `ign api` and `ign diagnostics` mixing harms discoverability and TUI mapping.
3. **TUI mapping for the five new leaves** (Pitfall 5). Dashboard rows for curated reads; OutOfBand + pinned-test extension for `api call`; decide whether `diagnostics` gets its own screen (recommend: NOT this phase — Dashboard rows keep TUI scope small; a Diagnostics screen can come with TUIX phases).
4. **`gan status` breadth.** `overview/gan` alone (5 fields, verified shape) vs + `/gateway-network/gateways` list. Recommendation: overview/gan as the one-command truth (it is THE morning-check signal: connections up/down + byte rates); expose `gateways` only if a capture shows it's cheap and stable — else leave it to `api call`.
5. **Bundle state vocabulary + units** (uptime, lastSyncTimestamp, fileSize) — resolved by the first live-capture task on both rigs; the plan should schedule captures BEFORE model-finalization tasks.
6. **Does `api call` accept a request body for GET/DELETE?** Recommendation: yes — pass through verbatim (curl parity); the gateway's answer classifies. Header refusal applies regardless of method.
7. **4xx set overlap:** should api-call 404 stay `not_found` (exit 6) or join the catch-all (exit 2)? Current classify maps 404→NotFound globally; keeping that is the least-surprise choice (a typo'd path is "not found", a misuse-shaped 400/405/415/409 is usage). Pin the chosen partition as a table in the contract test.

## Sources

### Primary (HIGH confidence)
- `~/whiskeyhouse/83-api/bruno/Ignition HTTP API/` — license-status, redundancy, gateway-network, thread-diagnostics request definitions (roadmap-designated ground truth for EXT-01)
- `~/whiskeyhouse/83-api/postman/8.3.postman_collection_v2.json` — doc-derived response samples for /licenses, /redundancy, /overview/gan, bundle trio
- `.planning/phases/02-gateway-health-inspection/openapi-8.3.6-phase2-extract.json` — LIVE-captured 8.3.3-rig OpenAPI schemas for `/diagnostics/bundle/*` and `/overview/gan` (incl. `_extract_notes.source`)
- In-tree code (all cited by path:line above): client/{mod,classify,version,status,trial,query,backup}.rs, error.rs, output.rs, session.rs, poll.rs, actions/{version,restart}.rs, cli.rs, main.rs, render.rs, ignition-tui/src/routes.rs, ignition-cli/tests/{e2e_webdev,version_gateway_contract,tui_coverage}.rs, ignition-core/tests/{common/mod,live_gateway}.rs, README.md (output contract + exit table + auth guide)
- `.planning/STATE.md`, `PROJECT.md`, `ROADMAP.md` — Phase-8 decisions, flags, rig constraints

### Secondary (MEDIUM confidence)
- Phase-4 VERIFICATION addendum rig recipes (docker ports, headless provisioning) — verified artifacts, but ports/containers are per-run ephemeral
- Postman response samples are doc-generated placeholders (types may differ from wire) — treat as shape hints, not captures

### Tertiary (LOW confidence / not performed)
- External web corroboration (Inductive Automation forum 8.3 API guide) — search tooling unavailable in this session; NOT required since the roadmap designates the 83-api collection as ground truth and live rigs as the verification oracle. Flagged for honesty: no claim above rests on web-only evidence.

## Metadata

**Confidence breakdown:**
- Wire shapes: HIGH for endpoints/paths (Bruno + live OpenAPI agree); MEDIUM for exact response bodies (Postman samples are placeholders; licenses nesting + bundle states + timestamp units need live capture — scheduled as first tasks)
- Architecture/patterns: HIGH — every pattern cited to in-tree code that compiles and is CI-pinned today
- Pitfalls: HIGH for 1/3/5/6/7/8 (contract- or history-backed); MEDIUM for 2/4 (live-capture and path-pinning decisions pending)

**Research date:** 2026-09-06
**Valid until:** ~2026-10-06 (repo-internal facts stable; live rig recipes re-verify at execution time)
