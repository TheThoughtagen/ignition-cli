# Phase 2: Gateway Health & Inspection - Research

**Researched:** 2026-08-21
**Domain:** Ignition 8.3 Gateway REST API (health/inspection surface) + Rust client extension of the Phase-1 `GatewayApi` seam
**Confidence:** HIGH — every endpoint, auth claim, and error shape below was **verified empirically against a live Ignition 8.3.6 gateway** (Docker, commissioned, trial mode) plus its own `/openapi.json` (577 paths, captured). No CONTEXT.md exists for this phase; recommendations are mine.

## Summary

Phase 2 is the first real exercise of the Phase-1 client seam against live-gateway truth, and that truth changes three Phase-1 assumptions. First, the real `gateway-info` field is **`ignitionVersion`** (`"8.3.6 (b2026042713)"`), not `version` — the Phase-1 `GatewayInfo` model will fail serde-deserialization against every real 8.3 gateway tested and must be corrected in 02-01. Second, **HTTP Basic auth does not work on `/data` REST routes at all** (verified: valid commissioned admin credentials → 401); the only machine-usable auth is the `X-Ignition-API-Token: <name>:<key>` header — note the **full `name:key` string** the gateway UI shows (key-only → 401; name:key → 200/403). Third, error bodies are **route-family-dependent**: `/data/api/v1/*` failures return Jetty HTML pages, `/data/app/*` returns JSON, and an **uncommissioned gateway 302-redirects everything to `/welcome`** — the client needs an HTML/redirect sniffer to classify these into the LOCKED exit taxonomy.

The good news: every HLTH requirement is served by verified native endpoints, all sharing a standard `{items, metadata}` list envelope. `/data/api/v1/overview` is the single best status call (version, java/os platform, uptime ms, CPU/memory/disk, license state incl. trialRemaining). Restart is `POST /data/api/v1/restart-tasks/restart?confirm=true` → literal `true`, followed by a ~40 s window where the webserver still answers (gateway-info → **503**, `/StatusPing` → `{"state":"STARTING"}`) until `{"state":"RUNNING"}` — an unauthenticated readiness primitive that should anchor both `ign wait` and `restart --wait`. Logs are queried (not tailed server-side): `GET /data/api/v1/logs?startTime=<epoch-ms>` is the tail primitive (poll it); `logs/download` returns a **SQLite `.idb`**, not a zip. Token-authenticated POSTs need **no CSRF** (CSRF is only for cookie/session auth). The doctor's "three-part token-setup failure causes" are now precisely understood and reproducible: (1) token not granted a level that satisfies Gateway Read/Write Permissions, (2) Gateway permissions not wired to a level the token holds (default = `Authenticated/Roles/Administrator` role level), (3) `secureChannelRequired` checked while using `http://` (→ 403). Doctor can also use unauthenticated `/StatusPing` to separate "gateway down" from "auth broken", and `/openapi.json` (auth-required) as a spec probe.

**Primary recommendation:** In 02-01, fix `GatewayInfo` (alias `ignitionVersion`, add `name`/`license`), add an HTML/redirect/503 response classifier mapping to the LOCKED taxonomy, and gate Basic auth behind a loud deprecation (it cannot work on 8.3 `/data` — resolve to a clear config error instead of a mysterious 401). Then grow `GatewayApi` capability-by-capability against the endpoint catalog below, extend the wiremock harness with an `IgnitionMock` builder that speaks the `{items, metadata}` envelope + HTML error pages, and pin a `#[ignore]`-gated live-gateway test suite against a Docker rig (one-command recreation documented below).

## User Constraints (from CONTEXT.md)

No CONTEXT.md exists for this phase (no `/gsd-discuss-phase` was run). No locked user decisions. Everything below is research-backed recommendation; the planner has full discretion within the LOCKED Phase-1 contracts:

- Envelope `{ok,profile,data}` / `{ok,profile,error}` and exit taxonomy 1–7 with stable slugs are **FROZEN** (STATE.md) — Phase 2 extends, never reshapes.
- `GatewayApi` is `async_trait`, coarse (one method per capability); auth headers token-XOR-basic enforced by a match; `Secret::expose()` confined to the single header-construction site.

## Verified Endpoint Catalog (the planner's source of truth)

All paths confirmed against a **live 8.3.6 gateway** AND its `/openapi.json` (extract saved at `.planning/phases/02-gateway-health-inspection/openapi-8.3.6-phase2-extract.json`, 46 paths). The 83-api Bruno collection agrees on every path. **ignition-mcp's `/data/api/v1/connections/database`, `/connections/opc`, and `/system/metrics` paths DO NOT EXIST** — they were plausible-looking inventions; do not copy them.

### Status / info (HLTH-01)

| Endpoint | Method | Purpose | Verified response shape (live capture) |
|---|---|---|---|
| `/data/api/v1/gateway-info` | GET | identity + version + license | `{name, redundancyRole, edition, hostname, port, ignitionVersion: "8.3.6 (b2026042713)", deploymentMode, timeZone, timeZoneId, jvmVersion, allowUnsignedModules, license: {mode: "Trial", validForVersion, expirationDate, licenseRestrictions[]}}` |
| `/data/api/v1/overview` | GET | **the status call** — platform + runtime | `{version: "8.3.6 (b2026042713)", redundancy: {role, activityLevel, projectState, …}, java: {version, vendor, name}, os: {name, arch, version}, cloudEnv, uptime: 338137 (ms), timezone, locale, time, memory: [used, max], cpu: 0.0031 (fraction), disk: {total, used}, license: {state: "trial", trialRemaining: 7017 (s)}, …}` |
| `/StatusPing` | GET | readiness probe — **unauthenticated**, plain JSON, answers during restart | `{"state":"RUNNING"}` / `{"state":"STARTING"}` (observed; commissioning-phase states also exist) |
| `/data/api/v1/overview/problems` | GET | web-UI "problems" list | `{items: [{title, description, resolution, url, actionLabel}], metadata}` |

HLTH-01 mapping: version+revision = `ignitionVersion` (one string, revision in parens); platform = `overview.java` + `overview.os`; running state = `/StatusPing` state or `overview` liveness; uptime = `overview.uptime` ms.

### Modules (HLTH-02)

| Endpoint | Method | Notes |
|---|---|---|
| `/data/api/v1/modules/healthy` | GET | `{items: [{id, onStartup, shouldUpgrade, name, version, description, vendorId, vendorName, selfSigned, state: "ACTIVE", licenseState, startupTime}], metadata}` — `state`, `version`, `licenseState` per module |
| `/data/api/v1/modules/quarantined` | GET | same item shape; usually empty |

Both accept the standard list params (`limit, offset, sortBy, search, filter` — `limit=-1` = all). `wait module-ready` can poll `modules/healthy?search=<moduleId>` until the item's `state == "ACTIVE"`.

### Metrics (HLTH-07)

| Endpoint | Method | Verified shape |
|---|---|---|
| `/data/api/v1/systemPerformance/currentGauges` | GET | `{cpu: 4.88 (percent), heapMemory: 2.4e8, maxMemory: 1073741824}` |
| `/data/api/v1/systemPerformance/charts` | GET | historic: `{cpuChartDatapoints: [{histId, timestamp: 1787346747022 (epoch ms), value}], memoryChartDatapoints: {heapMemoryDatapoints: [...], nonHeapMemoryDatapoints: [...]}}` |
| `/data/api/v1/systemPerformance/threads` | GET | thread execution: `{running: 32, waiting: 39, timedWaiting: 51, blocked: 0}` |
| `/data/api/v1/diagnostics/threads/deadlocks` | GET | deadlock detection (bonus; `/overview/problems` includes "Deadlock Detected") |
| `/data/api/v1/diagnostics/threads/threaddump` | GET | full thread dump |

Current CPU/memory also appears in `/overview` (`cpu` as a 0–1 fraction there, vs percent in gauges — normalize in the model, not in users' eyes).

### Sessions + terminate (HLTH-08)

| Endpoint | Method | Notes |
|---|---|---|
| `/data/api/v1/designers` | GET | `{items: [{address, id, lastcomm, memory, project, timeout, timezone, uptime, user}], metadata}` |
| `/data/api/v1/designer/{id}` | GET/DELETE | DELETE = prune/terminate a designer session |
| `/data/perspective/api/v1/sessions/` (trailing slash) | GET | items: `{id, username, authorized, project, clientAddress, lastComm, sessionScope, activePages, pageIds[], recentBytesSent, totalBytesSent, userAgent}` |
| `/data/perspective/api/v1/session/{sessionId}` | GET | detail |
| `/data/perspective/api/v1/session/{sessionId}/pages` | GET | pages of a session |
| `/data/perspective/api/v1/sessions?sessionId=X&message=Y` | **DELETE** | terminate Perspective session(s) — `sessionId` required query param, `message` optional |
| `/data/vision/api/v1/clients` | GET | same item shape as designers + `tagCount` |
| `/data/vision/api/v1/client/{id}` | GET/DELETE | DELETE = terminate Vision client |

Terminating a nonexistent id → 404 (verified). Mutations are audit-logged server-side (official docs).

### Logs + loggers (HLTH-03, HLTH-04)

| Endpoint | Method | Notes |
|---|---|---|
| `/data/api/v1/logs` | GET | `{items: [{timestamp (epoch ms), loggerName, level, message, stack?: [], mdc: {}}], metadata: {total, matching, limit, offset}}`. Params: `startTime, endTime` (epoch ms), `minLevel`, `logger`, `properties`, `allowedMarkers`, `limit, offset, sortBy, search, filter` — **all optional**; `startTime` is the tail cursor (verified: only entries ≥ startTime return) |
| `/data/api/v1/logs/download` | GET | **SQLite database** (`Content-Type: application/x-sqlite3`), `Content-Disposition: attachment; filename=<GatewayName>_Ignition_logs_YYYYMMDD-HHMM.idb` — NOT a zip (igw-cli's `gateway-logs.zip` default name is wrong for 8.3) |
| `/data/api/v1/logs/loggers` | GET | `{items: [{name, level, context}], metadata}` (~1250 loggers on a fresh gateway) |
| `/data/api/v1/logs/loggers/{loggerName}?level=X` | **POST** | set level; body none; verified 200 + level flips. Levels: `TRACE, DEBUG, INFO, WARN, ERROR, FATAL, OFF` (spec-documented). Path-escape logger names (`Common.BasicExecutionEngine`) |
| `/data/api/v1/logs/levelreset` | POST | reset all custom levels to defaults |

**Tail (`-f`) design:** there is no server push — poll `GET /logs?startTime=<lastTs+1>&limit=…` on an interval (2 s default, igw-cli uses adaptive backoff ×1.5 capped 2–30 s). Timestamps are epoch ms; cursor = max timestamp seen.

### Restart + wait (HLTH-09, HLTH-11)

| Endpoint | Method | Notes |
|---|---|---|
| `/data/api/v1/restart-tasks/restart?confirm=true` | POST | returns 200 with literal body `true`. Verified lifecycle: fire → ~5 s grace → webserver stays up but gateway-info → **503** and `/StatusPing` → `{"state":"STARTING"}` → ~40 s total → `{"state":"RUNNING"}` + gateway-info 200. No connection-refused window observed (Jetty never drops) |
| `/data/api/v1/restart-tasks/pending` | GET | `{"pending":[]}` — this is *required-restart* tasks (config changes needing restart), NOT active-restart status; do not use it as the restart-progress signal |

`ign wait` targets (recommendation):
- `wait gateway`: poll `/StatusPing` until `state == "RUNNING"` (unauthenticated, works even when auth is broken, answers during STARTING). igw-cli instead polls gateway-info 200 — also works but conflates auth failures with down-ness; StatusPing is strictly better as the liveness signal.
- `wait restart` (used by `restart --wait`): after POST, poll StatusPing until a non-RUNNING state is observed once (or gateway-info 503), then until RUNNING again; timeout ~5 min. Caveat: a very fast restart could flip back before the first poll — treat "RUNNING after having fired the POST + fixed floor delay (e.g. 5 s)" as success.
- `wait module <id>`: poll `modules/healthy?search=<id>` for `state == "ACTIVE"`.

### Doctor inputs (HLTH-10)

Verified facts that make each doctor check precise:

1. **URL/TCP**: parse URL → TCP dial host:port with short timeout (igw-cli pattern; separates DNS/firewall from HTTP).
2. **Liveness (unauth)**: `/StatusPing` — 200 JSON = webserver+gateway alive; distinguish `RUNNING` vs `STARTING` (mid-restart) vs no answer (down).
3. **Uncommissioned detection**: any `/data` route 302-redirecting to `/welcome` = gateway not commissioned → target-state class error with "run the commissioning wizard" hint (verified on fresh container).
4. **Auth check**: `GET /data/api/v1/gateway-info` with token → 200 = auth+read OK; **401 = token key not recognized** (wrong `name:key` — remind that the UI-copied string includes the `name:` prefix); **403 = token recognized but under-permitted** (see three-part below).
5. **The three-part token-setup failure causes** (HLTH-10's "auth incl. the three-part token-setup failure causes" — now exact):
   a. Token exists and key matches (else 401);
   b. Gateway Read/Write Permissions (`security-properties` singleton: `readPermissions`/`writePermissions`, `AnyOf`) include a security level the token actually holds — default is only `Authenticated/Roles/Administrator`; a token granted just `Authenticated` gets **403** (reproduced live, then fixed by adding `Authenticated` to permissions → 200);
   c. token's `secureChannelRequired=false` when the gateway URL is `http://` (default is **checked** in the create dialog → 403 over http; reproduced per forum + unchecked in test).
   Doctor can read `security-properties` (a config read, needs a working token) and the token's own grant list to produce a specific hint for b.
6. **Write-permission check**: `POST /data/api/v1/scan/projects` (igw-cli's choice — triggers a project rescan, harmless) → 2xx = write OK; 403 = read-only token. Alternative: set+reset a logger level (more visibly mutating; not recommended).
7. **WebDev-route presence**: `GET /system/webdev/<route>` → **404 = route absent** (verified with token); 200/401/403 = route exists (perm varies by route). The WebDev module ships in the standard image (verified in modules list).
8. **Rig detection**: profile-local (config `rig` section + `docker` presence) — no gateway calls; defer detail to Phase 4 but emit the check row now.
9. **Bonus**: `/openapi.json` (requires token; 11.9 MB on 8.3.6) — useful later for capability discovery; do NOT fetch it in routine doctor runs (size).

### DB / OPC connection status (HLTH-05, HLTH-06) — partially verified

- The web UI's Connections→Databases page polls `GET /data/api/v1/resources/list/ignition/database-connection?limit=-1` (verified by network capture). Resource items carry a `healthchecks` map alongside config; OPC is `ignition/opc-connection` (same family).
- The test gateway has zero connections configured, so the **populated `healthchecks` shape is UNVERIFIED** — plan a live-gateway verification step in 02-02 (create a dummy connection against a throwaway DB or the built-in `).* Lowest-risk model: items passthrough + render `name/enabled/healthchecks` as-is.
- `/data/api/v1/overview/connections` exists but returns web-UI presentation objects (`{title, img, actions, lines: [{text: "0/0 healthy resources", error: false}]}`) — usable as a summary but not per-connection status; prefer the resource list.

## Auth Model (empirically verified — closes the STATE.md flagged gap)

1. **Header**: `X-Ignition-API-Token: <name>:<key>` — the FULL string the 8.3 UI shows ("Store API Key" dialog, `name:key`). Key-only → 401 (verified). Official docs: "Most documented endpoints require authentication using an API Token… `X-Ignition-API-Token` custom header."
2. **Basic auth is NOT viable on `/data` routes**: valid commissioned admin user via `Authorization: Basic` → 401 on `/data/api/v1/gateway-info` and `/data/app/session` (verified). 8.3 web login is an OIDC flow (`/idp/default/authn/login` + `POST /idp/default/authn/submit-challenge/basic` + cookies) — not scriptable from a CLI without a browser. **Consequence for the CLI:** the Phase-1 `Credential::Basic` arm can never authenticate a `/data` call. Keep the enum arm (it may matter for WebDev in 8.1-style deployments or future gateway versions) but have the client emit a precise error/hint when Basic is used and a 401 HTML body comes back ("gateway rejected Basic auth — 8.3 /data routes require an API token; see `ign doctor`"). Planner decision: demote Basic in docs; do not silently retry.
3. **CSRF**: token-authenticated POST/PUT/DELETE need **no CSRF header** (verified: set-logger-level via token, no `X-CSRF-Token` → 200). Session/cookie-authenticated mutations need `X-CSRF-Token` from `GET /data/app/session` → `csrfToken` (verified: POST without it → 403 JSON). The CLI is token-only ⇒ no CSRF machinery needed in Phase 2.
4. **Status-code semantics** (verified matrix):

| Response | Meaning | Taxonomy mapping (LOCKED codes) |
|---|---|---|
| 200 + JSON | success | — |
| 401, HTML body (`/data/api/v1/*`) | token missing/not recognized (bad `name:key`) | `Auth` exit 5, hint: token format is `name:key`; regenerate in UI |
| 403, HTML body | token recognized; level doesn't satisfy Gateway read/write permissions OR secureChannelRequired over http | `Auth` exit 5, hint: three-part setup (see doctor §5) |
| 401/403, JSON body (`/data/app/*`) | same classes, JSON flavor | same |
| 302 → `/welcome` | gateway uncommissioned | new target-state slug (exit 6 class) e.g. `gateway_not_commissioned` — planner adds variant + enumerated-test row + README row (contract addition, not reshaping) |
| 503 (during restart) | gateway restarting — webserver up, services down | transient for wait-loops; terminal `Network`/target-state for one-shot commands with "restarting" hint |
| 404, JSON `{message: "No route match for path: …"}` | wrong path / old Ignition version | target-state hint (pre-8.3 gateway?) — relevant to `version` refusal matrix |

5. **API-token lifecycle** (for docs/doctor hints, verified): UI: Platform→Security→API Keys→Create (Basic Token, name, **uncheck "Require secure connections"** for http rigs, pick security level). REST (session+CSRF): `POST /data/api/v1/api-token/generate` → `{key, hash}` then `PUT /data/api/v1/resources/ignition/api-token` with `[{name, collection, enabled, signature, config: {profile: {type: "basic-token", secureChannelRequired, timestamp, securityLevels: [<tree nodes>]}, settings: {tokenHash}}}]`. securityLevels are **object trees, not strings** (strings → 422, verified). Granting intermediate paths logs `WARN gateway.ApiTokenManager: Security level 'Authenticated/Roles' cannot be granted via config and will be ignored` — only leaf paths count.

## Error-Body Sniffing (the client classifier)

Two non-JSON shapes observed (both reproduced live):

1. **HTML** — Jetty error page, `Content-Type: text/html;charset=iso-8859-1`, for `/data/api/v1/*` 401/403/404/500:
   ```html
   <html><head><meta …/><title>Error 401</title></head>
   <body><h2>HTTP ERROR 401 Unauthorized</h2><table>
   <tr><th>URI:</th><td>/data/api/v1/gateway-info</td></tr>
   <tr><th>STATUS:</th><td>401</td></tr>
   <tr><th>MESSAGE:</th><td>Unauthorized</td></tr></table></body></html>
   ```
   Classifier: if `content-type` is `text/html` → never `.json()` the body; extract `<title>Error NNN</title>` + `<th>MESSAGE:</th><td>…</td>` (cheap string scan — don't parse HTML with a crate) for the error envelope's message; map by status as in the matrix above.
2. **Redirect**: 302 with `Location: /welcome…` → uncommissioned; 302 to `/idp/…` on `/data/app/*` → not logged in (shouldn't happen for token auth; treat as auth class).
3. **JSON error** (`/data/app/*` and resource routes): `{message, url, status}` and 422 validation `{messages: [], fieldMessages: []}` — parse and surface `messages[0]`.

Client rule of thumb (prescriptive): dispatch on status first (401/403 → Auth), then on content-type (HTML → sniffer, JSON → structured), then redirect Location (welcome → target-state). Never let a `.json()` parse failure on an HTML body surface as `Internal`.

## Phase-1 Corrections Required (02-01 must-haves)

1. **`GatewayInfo` model**: rename/alias `version` → `ignitionVersion` (serde `#[serde(alias = "version")]` to tolerate any 8.3.x that still ships the old name — unverified for 8.3.0–8.3.2, LOW-confidence drift note). Capture at least `name`, `edition`, `license.mode`, plus passthrough. `below_minimum("8.3.6 (b2026042713)")` already parses correctly (space-split) — add this exact string as a unit-test row.
2. **Real-gateway regression test**: Phase 1's wiremock fixtures used `version`; a live gateway would have failed deserialization. Add a wiremock test with the **captured live gateway-info JSON** (in this doc) as the golden body.
3. **`state`/`uptime` fields on GatewayInfo don't exist** on the real payload — running-state/uptime come from `/overview` (+`/StatusPing`). Drop those fields or mark them Optional and source status from the right endpoints.
4. **Basic-auth path**: keep the enum arm; add the HTML-401-aware hint (see Auth §2). Consider logging a warning whenever a Basic credential is used against `/data`.

## Architecture Patterns

### GatewayApi growth (LOCKED seam, additive only)

One coarse method per capability — Phase 2 sketch:

```rust
#[async_trait::async_trait]
pub trait GatewayApi: Send + Sync {
    async fn gateway_info(&self) -> Result<GatewayInfo, CoreError>;          // 01
    async fn overview(&self) -> Result<Overview, CoreError>;                 // status
    async fn status_ping(&self) -> Result<StatusPing, CoreError>;            // unauth liveness
    async fn modules(&self, query: ListQuery) -> Result<ModuleList, CoreError>;   // healthy+quarantined
    async fn metrics_current(&self) -> Result<CurrentGauges, CoreError>;
    async fn metrics_historic(&self) -> Result<PerformanceCharts, CoreError>;
    async fn metrics_threads(&self) -> Result<ThreadCounts, CoreError>;
    async fn designers(&self, query: ListQuery) -> Result<DesignerList, CoreError>;
    async fn perspective_sessions(&self, query: ListQuery) -> Result<SessionList, CoreError>;
    async fn vision_clients(&self, query: ListQuery) -> Result<VisionClientList, CoreError>;
    async fn terminate_perspective_session(&self, id: &str, message: Option<&str>) -> Result<(), CoreError>;
    async fn terminate_vision_client(&self, id: &str) -> Result<(), CoreError>;
    async fn prune_designer(&self, id: &str) -> Result<(), CoreError>;
    async fn logs(&self, filter: LogQuery) -> Result<LogPage, CoreError>;
    async fn logs_download(&self) -> Result<Vec<u8>, CoreError>;            // .idb bytes
    async fn loggers(&self, query: ListQuery) -> Result<LoggerList, CoreError>;
    async fn set_logger_level(&self, logger: &str, level: Level) -> Result<(), CoreError>;
    async fn reset_logger_levels(&self) -> Result<(), CoreError>;
    async fn restart(&self) -> Result<(), CoreError>;                        // POST confirm=true
    async fn scan_projects(&self) -> Result<(), CoreError>;                  // doctor write probe
}
```

Implementation shape (extends Phase 1's `client/mod.rs` without restructuring):
- Extract the shared request pipeline into a private helper: `get_json<T>(path, query)`, `post_empty(path)` — each does auth headers (the ONE `expose()` match), send, then the **classifier** (status → content-type → redirect), then `.json::<T>()`. The classifier returns `CoreError` variants; HTML sniffing lives in one function.
- `ListQuery` struct (`limit/offset/sortBy/search/filter`, `Default` = `limit=-1`) serializes to query params; every list endpoint takes it. `limit=-1` means "all" (the UI's convention).
- Per-class timeouts: construct a second `reqwest::Client` (or per-request `.timeout()`) for: log download (120 s), everything else default 30 s connect 10 s (Phase-1 values). Restart POST itself is fast (returns `true` immediately) — the *wait* is poller-side.
- Models: typed structs for stable fields + `#[serde(flatten)] extra: BTreeMap<String, Value>` passthrough for unknowns (gateway responses evolve; passthrough keeps `--json` output complete). The `{items, metadata}` envelope gets ONE generic: `struct ListEnvelope<T> { items: Vec<T>, metadata: Metadata }`.

### Wait-loop pattern (shared by `wait`, `restart --wait`, `logs -f`)

```rust
// poll(interval, deadline) with igw-cli-style adaptive backoff: ×1.5 growth,
// clamp to [interval, 30s]; retry Network errors, NEVER retry Auth (fail fast);
// terminal errors abort; deadline expiry → Network-class timeout error with
// last-observation message.
```

Flags: `--interval <secs>` (default 2), `--timeout <secs>` (default 300 for restart, 120 otherwise). Ctrl-C: tokio `signal` → print last state, exit 4-class with `wait_cancelled`… (planner: keep it simple — default Ctrl-C kills with no envelope; document).

### Test architecture (extends Phase-1 harness)

1. **Wiremock unit/contract tests** per capability: mock returns captured live shapes (this doc + spec extract). Cover: happy path, HTML 401, HTML 403, 302→welcome, 503, 404 JSON. Assert `CoreError` class + endpoint + hint.
2. **`IgnitionMock` helper** (new): wraps a `wiremock::MockServer`; registers the standard envelope builder `list_json(path, items)` and `html_error(status, uri)` so 02-02/03/04 tests stay three lines each.
3. **Binary goldens** (snapbox): every new subcommand gets `--compact` + human goldens per the 01-02 harness (`stdout_for_golden`, `SNAPSHOTS=overwrite` flow, `[..]` elision for dynamic values — timestamps/ids everywhere here).
4. **`#[ignore]` live-gateway suite** (new): `IGNITION_LIVE_URL` + `IGNITION_LIVE_TOKEN` envs; when set, run read-only checks against a real gateway (gateway-info, overview, modules, logs?limit=1, loggers). Mutation tests (set level, terminate, restart) behind a second opt-in var. Recreation recipe (verified end-to-end today):
   ```bash
   docker run -d --name ign-research -p 18088:8088 -e ACCEPT_IGNITION_EULA=Y inductiveautomation/ignition:8.3.6
   # commission via http://localhost:18088/welcome (browser): pick "Ignition" standard → trial mode,
   # create admin/<pw>, Finish Setup → Start Gateway. Then UI: Platform→Security→API Keys→Create:
   # Basic Token, UNCHECK "Require secure connections", pick a security level with admin; copy name:key.
   ```
   Note: 8.3 commissioning is a step-wizard (`GET /bootstrap`, `POST /post-step` — reverse-engineered, no 8.1-style single `/api/v1/commissioning` POST); browser automation (Playwright) drives it fine — that's Phase 4 rig territory, do not build it into the CLI now.

### Command surface (recommendation — planner finalizes)

```
ign status [--json]                 # overview + gateway-info merged; license/trial banner line in human mode
ign modules [--quarantined]         # table: id, name, version, state, licenseState
ign metrics [--current|--history|--threads]   # default: current + threads
ign sessions [--type designer|perspective|vision]   # merged table by default
ign sessions terminate --type <t> --id <ID> [--message MSG]   # require_confirmation (exit 2 without --yes)
ign logs [--tail/-f] [--logger L] [--min-level L] [--since <epoch-ms|relative>] [--limit N]
ign logs download [-o FILE]         # default <gateway>-logs-<ts>.idb; note SQLite format
ign logs loggers [--search S]
ign logs loggers set <name> <LEVEL> # LEVEL enum; --yes guarded
ign logs loggers reset              # --yes guarded
ign restart [--wait [--timeout S]]  # --yes guarded ALWAYS (destructive, takes gateway down)
ign wait gateway|restart|module <id> [--interval S --timeout S]
ign doctor [--check-write]          # structured checks[] in data
```

Human mode: tables to stdout, `[profile: NAME]` header (CORE-01), license/trial status line on `status`. JSON mode: passthrough-shaped data (stable additive fields).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---|---|---|---|
| Log tailing | websocket/SSE reader | poll `GET /logs?startTime=cursor` every 2 s | No push exists; the query API IS the interface (spec + live) |
| HTML error parsing | full HTML parser crate | substring scan for `<title>Error NNN</title>` / `<th>MESSAGE:</th>` | Bodies are a fixed Jetty template; 20 lines, zero deps |
| Retry/backoff | retry framework | one `poll()` helper with ×1.5 backoff clamp [2,30]s | reqwest-middleware rejected in STACK.md; the loop is ~30 lines |
| SQLite log reading | SQL queries over download | ship the .idb as-is; `logs` command uses the JSON query API | download is an archival artifact; rusqlify only if a later phase demands it |
| Readiness detection | port-scan heuristics | `/StatusPing` state + gateway-info 200 | purpose-built, unauth, restart-aware (verified) |
| Zip extraction of "log download" | zip crate | none — it's SQLite (`application/x-sqlite3`) | verified content-type; igw-cli's `.zip` assumption is wrong on 8.3 |
| Adaptive table rendering | custom formatter | keep Phase-1 render.rs patterns (plain rows) | consistency; pretty tables can come with the TUI phase |

## Common Pitfalls

### Pitfall 1: Trusting ignition-mcp's endpoint paths
**What goes wrong:** `/data/api/v1/connections/database`, `/connections/opc`, `/system/metrics`, `/data/api/v1/designers`-style guesses — the first three don't exist (404 `No route match for path`); designer-sessions IS at `/designers`.
**Avoid:** every path in this doc's catalog is live-verified + spec-extracted; the extract JSON is committed next to this file.

### Pitfall 2: Token header without the `name:` prefix
Key-only → 401 that looks like "bad token". Verified semantics: 401 = not recognized (format/key wrong), 403 = recognized but under-permitted. Doctor must explain this split explicitly — it's the #1 setup failure (forum threads confirm).

### Pitfall 3: HTML bodies crashing `.json()`
Every `/data/api/v1` error is an HTML Jetty page; `resp.json()` yields a confusing decode error that Phase 1 maps to `Internal`. The classifier (status → content-type → redirect) must run before parsing; golden-test it with the exact captured HTML.

### Pitfall 4: 503 during restart treated as fatal Network
The webserver answers 503 (not connection-refused) while the gateway restarts — one-shot commands should map it to a target-state "restarting" error with a `ign wait restart` hint; wait-loops treat it as expected. Verified transition timing ~40 s.

### Pitfall 5: Basic auth dead-end
Valid credentials → 401 on all `/data` routes. If profiles carry Basic creds (Phase-1 chain does), users get an impenetrable 401. Emit the specific hint (needs API token) instead of passing the raw error through.

### Pitfall 6: Uncommissioned gateway = wall of 302s
A fresh Docker gateway 302s EVERYTHING to `/welcome` (even `/data/api/v1/gateway-info`). Without redirect classification (reqwest follows redirects by default — `redirect(Policy::none)` on error paths or detect final URL), the client sees the welcome HTML with 200. Configure reqwest `redirect::Policy::none()` for API calls and classify 302+`Location: /welcome` as `gateway_not_commissioned`.

### Pitfall 7: `logs/download` is not a zip
`Content-Type: application/x-sqlite3`, filename `.idb`. Naming it `.zip` corrupts user expectations (igw-cli does exactly this).

### Pitfall 8: Perspective sessions trailing slash
`/data/perspective/api/v1/sessions/` (spec has the trailing slash; the module-scoped prefix differs from core `/data/api/v1`). Base-URL joining with trailing-slash normalization (Phase-1 pinned behavior) must not collapse it — test this exact path.

### Pitfall 9: `limit` defaults
List endpoints default `limit=-1`?? — observed default returns everything (368 logs unprompted with limit unset? No — metadata showed limit:-1 when unset, total 368 returned). For `logs` ALWAYS pass an explicit `limit` (default 200 for display; unlimited only for tail dedupe safety) — a 2M-entry gateway log would otherwise flood agents.

### Pitfall 10: Restart without wait is a footgun (HLTH-09 wording: "carefully")
`restart` must be `--yes`-guarded (Phase-1 `require_confirmation`, exit 2 + hint naming `--yes`/`IGNITION_YES=1` — pinned in 01-04) and print "restarting; gateway will be READY in ~1 min; consider --wait" in human mode.

## Code Examples

### Models straight from live captures (drop-in serde, HIGH confidence)

```rust
use serde::{Deserialize, Serialize};

/// GET /data/api/v1/gateway-info — field names match the 8.3.6 gateway exactly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayInfo {
    pub name: String,
    pub redundancy_role: String,        // "Independent"
    pub edition: String,                // "standard"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
    /// "8.3.6 (b2026042713)" — version + build revision in one string.
    /// serde alias tolerates any 8.3.x that ships `version` instead.
    #[serde(alias = "version")]
    pub ignition_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jvm_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<LicenseInfo>,
    #[serde(skip, default)]
    pub endpoint: Option<String>,       // Phase-1 CORE-05 pattern (serde(skip))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseInfo {
    pub mode: String,                   // "Trial" | …
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiration_date: Option<String>,
}

/// GET /StatusPing — unauthenticated readiness. States observed: RUNNING, STARTING.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusPing { pub state: String }

/// The standard 8.3 list envelope — one generic for every list endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListEnvelope<T> {
    pub items: Vec<T>,
    pub metadata: ListMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListMetadata {
    pub total: i64,
    pub matching: i64,
    pub limit: i64,                     // -1 = unlimited
    pub offset: i64,
}

/// GET /data/api/v1/logs — the tail primitive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: i64,                 // epoch ms — the tail cursor
    pub logger_name: String,
    pub level: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stack: Vec<String>,
    #[serde(default)]
    pub mdc: serde_json::Map<String, serde_json::Value>,
}
```

### The response classifier (single site, replaces Phase-1's status ladder)

```rust
// client/classify.rs — runs before any .json(); ONE place that has seen
// status + content-type + redirect location.
pub(crate) enum Classified {
    Ok(reqwest::Response),
    Err(CoreError),
}
pub(crate) async fn classify(resp: reqwest::Response, url: &str) -> Classified {
    use reqwest::StatusCode as S;
    let status = resp.status();
    if status.is_success() { return Classified::Ok(resp); }
    match status {
        S::UNAUTHORIZED | S::FORBIDDEN => Classified::Err(CoreError::Auth {
            status: status.as_u16(),
            endpoint: Some(url.into()),
            // 401 → "token not recognized (format is name:key)";
            // 403 → "under-permitted or secure-channel required" — doctor hints
        }),
        S::SERVICE_UNAVAILABLE => Classified::Err(CoreError::GatewayRestarting { endpoint: url.into() }),
        S::NOT_FOUND if resp.headers().get(CONTENT_TYPE).map_or(false, is_html) => {
            Classified::Err(CoreError::NotFound { endpoint: url.into() }) // e.g. session id absent
        }
        _ => Classified::Err(/* … */),
    }
}
```
(302 handling happens at the client builder: `redirect(Policy::none)`; a 3xx with `Location: /welcome` → `GatewayNotCommissioned`.)

### wiremock: the two fixture shapes

```rust
// HTML error — exact Jetty template captured from the live gateway
Mock::given(method("GET")).and(path("/data/api/v1/gateway-info"))
    .respond_with(ResponseTemplate::new(401)
        .insert_header("Content-Type", "text/html;charset=iso-8859-1")
        .set_body_string(r#"<html><head><meta http-equiv="Content-Type" content="text/html;charset=ISO-8859-1"/><title>Error 401</title></head><body><h2>HTTP ERROR 401 Unauthorized</h2><table><tr><th>URI:</th><td>/data/api/v1/gateway-info</td></tr><tr><th>STATUS:</th><td>401</td></tr><tr><th>MESSAGE:</th><td>Unauthorized</td></tr></table></body></html>"#))
    .expect(1),

// list envelope — every list endpoint looks like this
Mock::given(method("GET")).and(path("/data/api/v1/designers"))
    .respond_with(ResponseTemplate::new(200).set_body_json(json!({
        "items": [], "metadata": {"total": 0, "matching": 0, "limit": -1, "offset": 0, "metrics": {}}
    })))
```

## State of the Art

| Old assumption | Verified 8.3.6 reality | Impact |
|---|---|---|
| gateway-info has `version`, `state`, `uptime` | `ignitionVersion`; no state/uptime | Phase-1 model fix (02-01) |
| Basic auth fallback works | 401 on all `/data` routes | demote Basic; token-only for machine auth |
| Log download = zip | SQLite `.idb` | filename + docs |
| Restart = connection-refused window | webserver stays up, 503 + StatusPing STARTING | wait design |
| 8.1-style `/api/v1/commissioning` POST | step wizard `/bootstrap` + `/post-step` (or UI) | Phase 4 rigs must automate via browser |
| igw-cli `wait gateway` polls gateway-info 200 | `/StatusPing` is unauth + state-aware | better primitive for `ign wait` |

## Open Questions

1. **DB/OPC `healthchecks` populated shape** (HLTH-05/06) — no connections on the test gateway. Resolve in 02-02 verification against a gateway with a configured connection (rig or live). LOW confidence until then; model as passthrough.
2. **8.3.0–8.3.2 response drift** — only 8.3.6 was live-verified; `ignitionVersion` aliasing hedges the known risk. If a live 8.3.1 exists in the org, run the `#[ignore]` suite against it.
3. **StatusPing state enumeration** — observed RUNNING/STARTING; commissioning-era states unenumerated. Treat unknown states as "not ready" + surface the string.
4. **Fast-restart race in `wait restart`** — if the gateway flips back to RUNNING before the first poll observes STARTING, "observe non-RUNNING once" never happens. Mitigation: floor delay (5 s) after POST before accepting RUNNING as terminal success. LOW risk (observed window ~35 s ≫ 2 s poll).
5. **WebDev route probe semantics with no token** — 404-verified with a token; unauth probe (401 vs 404) untested (Phase 5 owns the routes themselves). Doctor should probe with whatever credential it has and interpret accordingly.

## Sources

### Primary (HIGH confidence — live gateway 8.3.6, 2026-08-21)
- **Empirical session**: commissioned Docker `inductiveautomation/ignition:8.3.6` (image already local); every endpoint, status code, body shape, auth behavior, and the restart lifecycle captured first-hand (curl + browser-session fetch).
- **Gateway's own `/openapi.json`** (577 paths, 11.9 MB) — captured; trimmed 46-path extract committed at `.planning/phases/02-gateway-health-inspection/openapi-8.3.6-phase2-extract.json`.
- **Local 83-api collection** (`~/whiskeyhouse/83-api`, 675 Bruno requests) — paths cross-checked; every catalog path above matches.
- **Official docs**: docs.inductiveautomation.com 8.3 — API Documentation page (`X-Ignition-API-Token` header, `/openapi` + `/openapi.json`, audit-logging of mutations) and Docker Image page (`ACCEPT_IGNITION_EULA=Y`, env vars).

### Secondary (MEDIUM-HIGH)
- **IA forum "Ignition 8.3 API Usage Guide"** (t/93935, alexlu + Kevin.Herron) — three-part token setup, secure-connection http trap, HTML 401 body, `X-Ignition-API-Token` confirmation by IA staff.
- **igw-cli** (github.com/alex-mccollum/igw-cli, cloned) — doctor flow (URL→TCP→gateway-info→scan-projects write probe), wait-loop backoff (×1.5, clamp 2–30 s), 401/403 hint wording; endpoint paths agree with the spec.

### Tertiary (context only)
- **ignition-mcp reference client** — superseded where it conflicts (three invented paths); still authoritative for WebDev URL shape (`/system/webdev/<endpoint>`).

## Metadata

**Confidence breakdown:**
- Endpoint catalog: HIGH — live-verified + spec-extracted + 83-api cross-checked (three sources agree)
- Auth model: HIGH — every claim reproduced on a live gateway (Basic failure, name:key format, 401/403 split, no-CSRF-for-tokens, three-part setup)
- Error-body shapes: HIGH — exact HTML/JSON captured and embedded above
- DB/OPC healthchecks detail: LOW — endpoint + envelope verified, populated shape not
- Restart timing: MEDIUM-HIGH — one full lifecycle observed (~40 s); timing varies by hardware/project load

**Research artifacts:** spec extract committed beside this file; research container left running (`ign-research`, port 18088, trial expires ~2 h from research time; `docker rm -f ign-research` to clean).

**Research date:** 2026-08-21
**Valid until:** Ignition 8.4 or breaking 8.3.x API changes (endpoint catalog is version-locked to 8.3.x; re-verify against `/openapi.json` diff if upgrading)
