# Phase 4: Rig Lifecycle & Trial State - Research

**Researched:** 2026-08-22
**Domain:** Docker compose rig orchestration (Rust shell-out) + Ignition 8.3 trial/banners/backup wire APIs + native OIDC login flow
**Confidence:** HIGH (wire claims live-verified against ign-research 8.3.6 on :18088; compose behavior live-verified on Docker Compose v5.1.2 + official docs; rig conventions read from real WHK repos)

## Summary

Phase 4 is two problems sharing one command family. First, **compose orchestration**: shell out to `docker compose` (v2 plugin line, now at v5.1.2) with a one-shot `docker compose config --format json` "resolve" step at discovery time that yields the project name, services, published ports, and volumes — every lifecycle op then runs with explicit `-p <name> -f <file>`. Port pre-flight, status, and reset all consume structured JSON that compose already emits (`ps --format json` is line-delimited objects with a rich `Publishers` array; `docker ps --filter publish=<port>` attributes host-port occupants). Second, **trial state**: the live rig reveals `GET /data/api/v1/trial` is *unauthenticated* and carries `trialSecondsLeft`/`expired` — a better `rig trial status` source than the roadmap's banners endpoint (keep banners as cross-check; both verified live on 8.3.6).

The flagged spike (trial-reset mechanism) is **resolved by evidence in favor of native HTTP**. I walked the real login flow live on 8.3.6: `GET /data/app/login` → 302 chain into the internal IdP OIDC flow (`/idp/default/oidc/auth` → `/idp/default/authn/login?...&token=<T0>`), then **pure-JSON challenge endpoints** (`POST /idp/default/authn/next-challenge`, `POST /idp/default/authn/submit-challenge/basic` with `{token, rememberMe, challenge:{username,password}}`) whose **token rotates on every call**. Steps 1–5 are live-probed (including the clean `{"success":false,"token":...}` bad-credential shape); the post-success legs (complete→redirect→`/data/federate/callback/internal`→session→`GET /data/app/session`→CSRF→`POST /data/api/v1/trial`) are source-verified from the served SPA bundle + the existing resetter's captured-traffic comments. The delegation candidate (Node/Playwright) requires a browser runtime, broke across 8.3.3's UI rewrite, and verifies via DOM text; the native flow has JSON contracts at every step. **But before building any login machinery, spike task 1 must test whether API-token auth (`X-Ignition-API-Token`) satisfies `POST /data/api/v1/trial` directly** — CSRF guards cookie-auth, and token-header auth plausibly bypasses it; if true, trial reset collapses to one authenticated POST through the existing client.

**Primary recommendation:** Build `rig` as compose shell-out with a `config --format json` resolve step; implement trial reset natively in Rust with the token-auth shortcut tried first, the fully-mapped OIDC+CSRF flow as the mechanism, and Playwright delegation documented as fallback only (never shipped); snapshot = `GET /data/api/v1/backup?type=roaming` streamed to disk (gwbk includes config **and** the projects dir), restore = raw-octet-stream POST + restart-wait via poll.rs verbatim.

## User Constraints (accumulated project decisions — no CONTEXT.md exists; STATE.md is binding)

### Locked Decisions (from STATE.md — honor exactly)
- Agentic output contract FROZEN — envelope `{ok,profile,data}`/`{ok,profile,error}`, exit taxonomy 1–7 with stable slugs (`CoreError::Rig` = exit 7 / `rig_error` already exists), errors-on-stderr in all modes
- Secret chain built in exactly one place (`secret_chain()` in main.rs); resolve order env tokens → keyring → USER/PASSWORD
- Two-layer naming: client models wire-faithful, actions re-expose unit-explicit keys (`trial_remaining_s` style)
- Destructive dispatch pattern LOCKED: `require_confirmation` guard fires BEFORE profile/secret/client resolution (exit 2, null profile, zero work) — applies to `rig down --volumes`, `rig reset`, `rig restore`, `rig trial reset`
- JSON data always carries ALL family keys (filtered-out = `[]`) — agents never key-hunt
- `poll.rs` is THE wait engine (×1.5 backoff clamp [interval,30s]) — rig waits reuse it verbatim; probe closures translate retryable-but-aborting errors (e.g. `GatewayNotCommissioned`) into `PollState::Pending` themselves
- Export streams (`download_to_file`), import buffers; timeouts 120s/300s per request class
- `classify()` is the single status→content-type→redirect mapping site; `redirect(Policy::none())` — the IdP flow's 302s must be consumed by a flow-local client path, not by following redirects blindly
- E2E harness convention: `#[ignore]` tests, quiet skip without env, mutations need `IGNITION_LIVE_MUTATIONS=1`
- rig reset semantics per roadmap: teardown with explicit compose project names + `--remove-orphans`, no stale trial state survives
- wait gateway/restart dispatch HEADER-LESS (secret degrades to None) — rig readiness probes ride `/StatusPing` header-less

### Claude's Discretion (planner freedom within this research)
- Exact `[rig]`/`[rigs.*]` TOML shape; command flag spelling; module layout under `crates/ignition-core/src/rig/`
- Trial-reset mechanism tiers (this research prescribes: token-auth → OIDC native → document Playwright fallback)

### Deferred Ideas (OUT OF SCOPE)
- bollard / Docker Engine API for lifecycle (rejected in STACK; compose files are the source of truth)
- Headless commissioning automation (no 8.3 commissioning REST endpoints exist in 83-api; browser wizard only — `rig up` reports "uncommissioned, open /welcome" as data, exit 0)
- `docker run`-style rigs (ign-research itself is a standalone container, NOT compose) — rig lifecycle commands manage compose rigs only
- Tag-provider export as part of snapshot (Phase 5 owns bulk provider export; gwbk already captures tag *config* via gateway data)

## Standard Stack

### Core (all but the first already in the workspace)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `docker` CLI (compose v2 plugin) | v5.1.2 local; require ≥v2 | All rig lifecycle: up/down/ps/config/logs | Compose files are the ecosystem's source of truth; bollard can't read them (verified decision, STACK/ARCHITECTURE) |
| `tokio` (add `process` feature) | 1.53 workspace | `Command` spawn + stdout/stderr line streaming for compose | Already the runtime; **`process` is NOT yet in the workspace feature list — Phase 4 Cargo.toml change** |
| `serde_json` (existing) | — | Parse compose LDJSON / config JSON | LDJSON via `StreamDeserializer` |
| `reqwest` (existing; **no new features**) | 0.13 | trial/banners/backup wire calls; IdP flow if native | `cookies` feature stays OUT — capture the ~4 known `Set-Cookie`s manually in the flow (fixed sequence, not arbitrary browsing); revisit only if the spike shows more cookies |
| `sha2` or mtime naming | — (prefer NO new dep) | snapshot file naming | timestamp-based filename `<rig>-<yyyyMMdd-HHmmss>.gwbk` — no new dependency |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| compose shell-out | bollard | Reimplements Compose Spec (interpolation, `.env`, named-volume conventions); verified trap — reject |
| manual Set-Cookie capture | `reqwest` `cookies` feature | Cookie store is more general; manual is ~30 lines for a fixed 4-cookie sequence and honors STACK's "keep cookies OUT" note; flip only if live flow shows more |
| `docker compose down --dry-run` preview | volume-ls filtering | `--dry-run` output is human-formatted lines (verified), not JSON — use `docker volume ls --filter label=com.docker.compose.project=<p> --format json` for structured "what reset removes" |
| `lsof` port check | docker-only check | `docker ps --filter publish=` misses NON-docker host processes binding the port; run docker filter first (rich attribution), `lsof -nP -iTCP:<port> -sTCP:LISTEN` second (host processes), tolerate lsof absence |

**Installation:** no new crates; one feature addition:
```toml
# Cargo.toml (workspace)
tokio = { version = "1.53", features = ["rt-multi-thread", "macros", "time", "fs", "io-std", "io-util", "process"] }
```

## Architecture Patterns

### Recommended Project Structure
```
crates/ignition-core/src/
├── rig/                    # NEW — compose engine (no printing, serde models out)
│   ├── mod.rs              # RigPlan, RigStatus, discovery (5 levels), port pre-flight
│   ├── compose.rs          # Command builder + spawn + LDJSON parse + exit-code mapping
│   └── (trial login lives in client/ — it's gateway HTTP, not docker)
├── actions/
│   └── rig.rs              # NEW — rig_up/down/status/reset/logs/trial/snapshot/restore verbs
├── client/
│   ├── trial.rs            # NEW — GET /data/api/v1/trial + banners (wire-faithful models)
│   ├── backup.rs           # NEW — GET/POST /data/api/v1/backup (stream down / octet up)
│   └── idp.rs              # NEW (spike-gated) — login+CSRF flow, flow-local redirect handling
└── poll.rs                 # UNTOUCHED — probes translate GatewayNotCommissioned→Pending
```

### Pattern 1: Resolve-then-act (the RigPlan)
**What:** Discovery locates a compose file; then ONE `docker compose -f <file> --project-directory <dir> config --format json` invocation resolves everything (interpolated ports, `.env`-sourced `COMPOSE_PROJECT_NAME` → `.name`, services, volumes, secrets names).
**When to use:** every rig command start (cheap, read-only).
**Why:** honors each rig's own `.env` identity (git-module = `ignition-devops`) while letting every subsequent op pass explicit `-p <name> -f <file>` (LOCKED: no implicit directory-name projects).
**Note (unit-test in implementation):** `--project-directory` defaults to the first `-f` file's dir, which is where `.env` is read from — pass it explicitly to be cwd-independent.

```text
RigPlan {
  name: String,            // from config .name (respects COMPOSE_PROJECT_NAME in .env)
  compose_file: PathBuf,
  project_dir: PathBuf,
  services: Vec<String>,
  host_ports: Vec<u16>,    // from services.*.ports[].published (resolved)
  volumes: Vec<String>,    // named volumes declared
}
```

### Pattern 2: Discovery order (roadmap-locked, levels verified live)
1. `--rig <name>` flag → look up `[rigs.<name>]` in config
2. `IGNITION_RIG` env → same lookup
3. cwd compose: `./docker/compose.yml`, `./docker/docker-compose.yml`, `./compose.yml`, `./compose.yaml`, `./docker-compose.yml`
4. `ignition-git-module/docker/` (docker-compose.yml; also docker-compose-automated.yml + test-rig/ subdir at ports 9188/9143)
5. WHK-Global conventions → **live successor is `whk-environment-orchestration/docker-compose.yml`** (service `ignition`, ports 9088/9043/62541, `restart: unless-stopped`, named vols `gw-data` + `gw-tag-definition`, file-based `ignition-api-token` secret)

**Path-correction (plan-checker, 2026-08-22):** this machine's live copy of whk-environment-orchestration sits at `~/whiskeyhouse/whk-environment-orchestration/` — NOT under `~/Documents/whiskeyhouse/` as first recorded (both home roots exist; layouts differ per machine). Implementation MUST probe both roots (`~/Documents/whiskeyhouse/` first, then `~/whiskeyhouse/`) for BOTH level-4 and level-5 convention repos — never hard-code one root.

Config surface (discretion — prescriptive):
```toml
[rig]
default = "git-module"
[rigs.git-module]
compose_file = "~/Documents/whiskeyhouse/ignition-git-module/docker/docker-compose.yml"
# project_name optional — omit to honor the rig's own .env COMPOSE_PROJECT_NAME
```

### Pattern 3: Port pre-flight (on `up` and `reset`)
```
for port in plan.host_ports:
    docker ps --filter publish=<port> --format json   → occupant container + its project label
      occupied by container in THIS project → fine (recreate)
      occupied by container in ANOTHER project → Rig error: "port <p> in use by <container> (rig <other>)"
    else lsof -nP -iTCP:<port> -sTCP:LISTEN           → host process attribution (non-fatal if lsof absent)
```

### Pattern 4: wait-for-commissioned (poll.rs verbatim, probe translates)
Probe = header-less `GET /StatusPing` (the 02-05 anchor):
- `RUNNING` → `Done`
- `STARTING` / transport refused / 503 → `Pending`
- 3xx → `/welcome` → classify yields `GatewayNotCommissioned` → **probe catches it and returns `Pending("gateway uncommissioned — open http://<host>:<port>/welcome")`** — never let it abort (poll.rs retries only Network/GatewayRestarting; LOCKED, unchanged)
- `Auth` never retried (poll.rs rule) — StatusPing is header-less so this can't fire
- **Fresh-volume rigs terminally report "up, uncommissioned" as data (exit 0 + warning inside `data`, the version-command degradation precedent)** — there is no headless commissioning (verified: no commissioning endpoints in 83-api); the wait deadline (≥180s per PITFALLS 1.7; image pull can add minutes) only covers STARTING→RUNNING on an already-commissioned volume.

### Pattern 5: Trial reset ladder (spike resolution)
```
tier 0: POST /data/api/v1/trial with X-Ignition-API-Token (existing client, one call)
        → 2xx: DONE (no login machinery at all)
tier 1: native OIDC login (client/idp.rs) → session+CSRF → POST trial   [fully mapped, see Code Examples]
tier 2: documented fallback: node reset-trial.mjs (ignition-trial-resetter or WHK-Global e2e/reset_trial.mjs)
        — NEVER shipped as the mechanism; README documents env contract
verify: GET /data/api/v1/trial flips expired=false (and/or banners severity=info with future expireTime)
```

### Anti-Patterns to Avoid
- **Parsing compose's human output** — everything needed is `--format json` (config/ps/ls/volume ls); only `down --dry-run` and `logs` are human-form (logs intentionally passthrough)
- **Dumping compose config into `--json` output** — resolved config contains `GATEWAY_ADMIN_PASSWORD` etc. (PITFALLS 3.6); `rig status --json` serializes an ALLOWLIST (name, services[state/health/publishers], volumes[names], project, compose_file) — never raw config/inspect passthrough
- **Following redirects in the IdP flow with the shared client's `Policy::none()` assumptions** — the login flow CONSUMES 302s (`oidc/auth`→`authn/login`, callback→`/app`); implement flow-local manual Location handling on a dedicated request path, leaving the locked client pipeline untouched
- **`down && up` without `-v` as "reset"** — leaves named volumes → stale trial/DB state (PITFALLS 3.3, the classic "reset didn't work")

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Compose lifecycle | bollard/Engine-API orchestration | `docker compose` subprocess | Interpolation, `.env`, secrets, named-volume conventions, `--wait` health semantics — all free and exactly what WHK rigs already use |
| Port attribution | socket probing / bind tests | `docker ps --filter publish=` + `lsof` | Precise attribution ("in use by container X (rig Y)"), zero false positives |
| Status shape | `docker inspect` assembly | `docker compose ps --format json` | LDJSON with `Publishers` array per service — exactly the port-aware status needed |
| Backoff/retry for waits | ad-hoc loops | `poll.rs` | LOCKED single wait engine |
| gwbk transfer | Vec<u8> buffering | `download_to_file` streaming / octet-stream body | 03-02 precedent; gwbks are tens of MB |
| Login-page HTML parsing | scraping the IdP React app | the JSON challenge endpoints | The form is client-rendered; the XHRs are pure JSON (live-verified) |

**Key insight:** compose v5 already speaks JSON for everything structural; the only things ign adds are discovery conventions, the agentic envelope, pre-flight, and the wait/verify composition.

## Common Pitfalls

### Pitfall 1: Compose LDJSON vs JSON-array inconsistency
**What goes wrong:** `docker compose ps --format json` and `docker ps --format json` emit ONE OBJECT PER LINE; `docker compose ls --format json` and `docker compose config --format json` emit a SINGLE JSON ARRAY. Naive `serde_json::from_str::<Vec<T>>` hangs or fails on the former.
**How to avoid:** parse line-delimited shapes with `serde_json::Deserializer::from_slice(...).into_iter::<T>()` (StreamDeserializer); arrays with plain `from_str`. Pin both in unit tests with recorded fixtures.

### Pitfall 2: IdP token rotation (live-verified)
**What goes wrong:** every `/idp/default/authn/*` POST returns a NEW token; replaying a consumed token → `400 Invalid token` in a **Jetty HTML body** (not JSON).
**How to avoid:** thread the response token forward through the whole flow; treat non-JSON 4xx from these endpoints as flow failure with the HTML title extracted (classify-style sniff).

### Pitfall 3: `up --wait` waits for healthchecks when defined
**What goes wrong:** WHK rigs define healthchecks (`test-rig` git-server: `service_healthy` dependency); `--wait` blocks until healthy, not just running — an unhealthy service exceeds the default and compose exits 1 mid-bring-up.
**How to avoid:** pass `--wait-timeout` explicitly (prescribe ≥300s for image builds/pulls + Ignition start); map compose exit 1 → `CoreError::Rig` carrying compose's stderr tail.

### Pitfall 4: Named-volume project prefixing
**What goes wrong:** reset must remove `<project>_<volume>` volumes; `down -v` only removes volumes DECLARED in the compose file — anonymous strays and orphans from renamed services survive unless `--remove-orphans` is also passed.
**How to avoid:** `reset` = `down -v --remove-orphans` (LOCKED); `status --json` lists what reset would remove via `docker volume ls --filter label=com.docker.compose.project=<p> --format json` (verified shape: `Name`, `Labels` with `com.docker.compose.volume`).

### Pitfall 5: gwbk restore clobbers API tokens (83-api README, primary source)
**What goes wrong:** tokens stored under CORE config (`data/config/CORE/ignition/api-token`) are "modified/cleared often by gwbk restores" — post-restore, stored profiles 401.
**How to avoid:** snapshot/restore output carries a data-level warning ("API tokens may have been reset by restore — re-provision via gateway UI, then `ign doctor`"); 83-api recommends EXTERNAL location for durable tokens — document, don't enforce.

### Pitfall 6: Restore is synchronous AND followed by a restart
**What goes wrong:** `POST /data/api/v1/backup` blocks while restoring, then the gateway restarts — a short timeout kills mid-restore with unknown state; treating the 2xx as "done" reports success while the gateway is still down.
**How to avoid:** 300s request class for both GET (gwbk generation is not instant) and POST; after 2xx, poll StatusPing→RUNNING (poll.rs, deadline ≥300s) before reporting success.

### Pitfall 7: Trial/banner field semantics (live-observed)
**What goes wrong:** banners `expireTime` is epoch **milliseconds** or `null`; an expired trial shows `severity:"warning"` + `expireTime:null` — code expecting a future timestamp misreads expired as active.
**How to avoid:** active = `GET /trial` → `expired:false` (primary, has `trialSecondsLeft`); banners only as cross-check: `type:"trial"` + `data.severity=="info"` + `expireTime>now_ms`. `trialState ∈ {AllInDemo, SomeInDemo, NoneInDemo}` (postman description, live capture matches).

### Pitfall 8: `.env`/`COMPOSE_PROJECT_NAME` resolution is cwd-sensitive
**What goes wrong:** running compose with `-f` from another cwd can skip the rig's `.env` (project name, secrets env), silently creating a directory-named project — the exact collision PITFALLS 3.3 warns about.
**How to avoid:** always pass `--project-directory <dir-of-compose-file>` (Pattern 1); the resolved `.name` from `config --format json` is the single identity truth. Unit-test with a temp dir + `.env`.

## Code Examples

### Trial status wire (LIVE-captured, ign-research 8.3.6, unauthenticated)
```jsonc
// GET /data/api/v1/trial  → 200, no auth
{ "licenseMode": "Trial", "trialState": "AllInDemo", "trialSecondsLeft": 0,
  "expired": true, "emergency": false, "emergencySecondsLeft": 0,
  "development": false, "developmentSecondsLeft": 0 }

// GET /data/api/v1/overview/banners  → 200, no auth
{ "banners": [ { "order": 0, "type": "trial",
  "data": { "severity": "warning", "expireTime": null, "toolTips": [], "actions": [] } } ] }
```
Source: live rig 2026-08-22 (expired state); active-state shape from reset-trial.mjs (`severity:"info"`, `expireTime` epoch-ms). 83-api postman describes trialState domain.

### Native login flow (live-probed 8.3.6 — steps 1–5 executed; 6–8 source-verified from served SPA bundle)
```text
1. GET  /data/app/login
   → 302 Location:/idp/default/oidc/auth?app=gateway&response_type=code&client_id=ignition
        &redirect_uri=/data/federate/callback/internal&scope=openid&state=<JWT>&nonce=<..>&prompt=login&max_age=1
   Set-Cookie: idp-relay--<n>=<v>; HttpOnly

2. GET  /idp/default/oidc/auth?<those params>
   → 302 Location:/idp/default/authn/login?app=gateway&token=<T0>&<same OIDC params>
   Set-Cookie: idp-sid-default--<n>=<v>; HttpOnly; SameSite=Strict; Path=/idp/default

3. POST /idp/default/authn/next-challenge      Content-Type: application/json
   {"token":"<T0>"}          [cookies: both above]
   → 200 {"complete":false,"nextChallenge":[{"type":"basic","config":{}}],
          "rememberMe":false,"passwordExpired":false,"token":"<T1>"}     // TOKEN ROTATES

4. POST /idp/default/authn/submit-challenge/basic
   {"token":"<T1>","rememberMe":false,"challenge":{"username":"<u>","password":"<p>"}}
   → 200 {"success":false,"token":"<T2>"}   ← observed with bad creds (rig pw ≠ default)
   → 200 {"success":true, "token":"<T2>"}   ← valid creds (bundle: onFulfilled reads n.success)

5. POST next-challenge {"token":"<T2>"} → {"complete":true,...,"token":"<T3>"}
   (bundle: complete → redirectToAuthorizationEndpoint)

6. GET  /idp/default/oidc/auth?<orig params>&token=<T3>
   → 302 Location:/data/federate/callback/internal?code=<auth-code>&state=<..>
7. GET  /data/federate/callback/internal?code=..&state=..
   → Set-Cookie: <gateway session>  → 302 /app
8. GET  /data/app/session  (session cookie)  → CSRF token (JSON)          [shape needs creds]
9. POST /data/api/v1/trial   (session cookie + X-CSRF-Token header) → reset
10. verify: GET /data/api/v1/trial → expired:false
```
Steps 6–10 are the resetter's browser-replayed traffic (reset-trial.mjs header comment: "POST /data/api/v1/trial with session cookie + x-csrf-token") mapped onto the discovered endpoints; exact session-cookie name + CSRF JSON field = spike deliverable with creds.

### Compose invocation shapes (live-verified v5.1.2)
```bash
docker compose version                     # "Docker Compose version v5.1.2" — parse major ≥2
docker compose -f <file> --project-directory <dir> config --format json   # resolve: .name/.services/.volumes/ports
docker compose -p <name> -f <file> up -d --wait --wait-timeout 300 --remove-orphans
docker compose -p <name> -f <file> down -v --remove-orphans    # reset teardown half
docker compose -p <name> -f <file> ps --format json             # LDJSON, one object per service
docker compose -p <name> -f <file> logs --tail 200 [-f] [SERVICE]   # passthrough (streaming exception)
docker ps --filter publish=18088 --format '{{.Names}}'          # → "ign-research" (port attribution)
docker volume ls --filter label=com.docker.compose.project=<p> --format json   # reset-preview (LDJSON)
```
`ps --format json` per-object fields (live capture): `Name, Service, Project, State, Health, ExitCode, Status, Publishers:[{URL,TargetPort,PublishedPort,Protocol}], Labels, Networks, Ports`.

### Backup endpoints (83-api postman collection, primary)
```text
GET  /data/api/v1/backup?type=roaming|all [&includePeerLocal=bool]
     Accept: application/octet-stream → .gwbk bytes (STREAM to file; 300s class)
     "roaming" = portable backup (cross-rig); default incl. gateway-specific state

POST /data/api/v1/backup
     Content-Type: application/octet-stream   ← RAW gwbk bytes, NOT multipart
     query: restoreDisabled, disableTempProjectBackup, renameEnabled, newName, restoreLocal
     (restoreDisabled=true → all projects/db/opc connections restored DISABLED — leave false)
     → synchronous restore, gateway restarts afterward → poll StatusPing RUNNING
```
Auth: 401 HTML unauthenticated (live-verified shape) — requires token like every `/data` route.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| docker-compose v1 (Python) | compose v2 plugin (Go), now v5.x | v1 EOL 2023; plugin majors iterate independently | Detect via `docker compose version` parse; v1 never answers to `docker compose` — absence = clear Rig error with install hint |
| PITFALLS-era login guess (`POST /data/app/login` + `/data/app/session` XHR) | OIDC internal-IdP flow (`/idp/default/*`), `/data/app/login` is only a 302 entry href | 8.3.x line (verified 8.3.6) | The forum-documented endpoints 404 on 8.3.6 — use the mapped flow above |
| Trial status via banners only | `GET /data/api/v1/trial` (unauth, richer) | observed 8.3.6 | Primary source for `rig trial status`; banners = cross-check/fallback |
| `#reset-trial-anchor` selector | text-based controls, then JSON endpoints | 8.3.3 UI rewrite | UI automation fragile → native HTTP wins the spike |

**Deprecated/outdated:** anything citing `POST /data/app/login` as an XHR target (404 on 8.3.6, live-verified); `docker-compose` hyphenated binary.

## Open Questions

1. **Does API-token auth satisfy `POST /data/api/v1/trial`?** (spike task 1 — cheapest possible outcome)
   - What we know: unauth POST → 401 (live); CSRF guards session-cookie mutations; token-header is the gateway's non-interactive path and plausibly bypasses CSRF.
   - What's unclear: whether the trial POST accepts token auth at all.
   - Recommendation: with a provisioned admin token, one curl decides; if 2xx — ship tier 0 only, keep the mapped tier-1 flow documented in code comments for future gateway changes.
2. **Post-login session cookie name + `/data/app/session` CSRF field** (steps 8–9 shapes)
   - What we know: endpoint exists (401 HTML unauth — live); resetter replayed cookie+`x-csrf-token` header.
   - Recommendation: spike with creds on ign-research (its trial is expired — good test subject); capture exact names; wiremock-pin.
3. **≥2 gateway minor versions for the reset path** (success criterion)
   - What we know: read endpoints verified on 8.3.6; git-module rig defaults to 8.3.3 (its compose `IGNITION_VERSION:-8.3.3`).
   - Recommendation: e2e gate runs the flow against git-module rig (8.3.3) + ign-research (8.3.6); `IGNITION_RIG_E2E=1`-style env per harness convention.
4. **`config --format json` `.name` reflecting `.env`'s `COMPOSE_PROJECT_NAME` when invoked cwd-elsewhere**
   - What we know: `--project-directory` governs `.env` loading; live config run from the file's own context resolved correctly.
   - Recommendation: implementation unit test with temp dir + `.env` (cheap, removes the MEDIUM flag).
5. **gwbk restore + scan necessity** — restore restarts the gateway (restart re-scans); `POST /scan/config` + `/scan/projects` (endpoints verified in collection) kept as a documented follow-up for volume-level manipulation, not wired into restore-by-default.

## Sources

### Primary (HIGH confidence)
- **Live rig ign-research (8.3.6, :18088)** — probed 2026-08-22: `/StatusPing`, `/data/api/v1/trial`, `/data/api/v1/overview/banners`, `POST /data/api/v1/trial` (401), `/data/app/session` (401 HTML), `/data/app/login` (302 chain), `/idp/default/oidc/auth` (302+cookies), `/idp/default/authn/login` (200), `next-challenge` (200 JSON), `submit-challenge/basic` (success:false + token rotation + 400-Invalid-token on replay), `/data/api/v1/backup` (401), SPA bundles (`IgnitionWebUi.js`, `authentication.js`) decompiled for flow steps 5–7
- **Local Docker (Compose v5.1.2, Engine 29.4.0)** — `up/down/ps/config/ls/logs/volume ls` behaviors, `--filter publish` attribution, LDJSON shapes, `down --dry-run` human-format output
- **Local WHK repos (read directly)** — `ignition-git-module/docker/` (compose, `.env` `COMPOSE_PROJECT_NAME=ignition-devops`, ports 9088/9043, `test-rig/` 9188/9143 + healthcheck/depends_on); `whk-environment-orchestration/docker-compose.yml` (ignition service: restart-policy lesson, `gw-data`/`gw-tag-definition` volumes, secrets incl. file-based API token, `PROJECT_SCAN_ENABLED`); `ignition-trial-resetter/reset-trial.mjs` + `instances/*.env`; `WHK-Global` worktree `e2e/reset_trial.mjs` + `lib/login.mjs`
- **83-api Postman collection** — backup GET/POST full param sets (octet-stream body, `type=roaming`), trial `trialState` domain, scan endpoints; README token/CORE-vs-EXTERNAL gwbk note
- **docs.docker.com** (fetched 2026-08-22) — `compose up` (`--wait` "running|healthy, implies detached", `--wait-timeout`, error exit 1), `compose down` (`-v` named+anonymous volumes, `--remove-orphans`, external never removed)
- **02-RESEARCH.md + live_gateway harness** — uncommissioned 302→`/welcome` classification, `/StatusPing` unauth anchor, Basic-auth non-viability, OIDC login-flow note (independently confirmed by today's probes)

### Secondary (MEDIUM confidence)
- Trial-reset flow steps 6–10 (session cookie → CSRF → POST trial → verify): source-verified from served SPA bundle + resetter's captured-traffic comments; not yet executed end-to-end (rig password unknown to researcher)
- `--project-directory`/`.env` precedence under cwd-elsewhere invocation (Docker docs + one live config run; unit-test to confirm)
- gwbk "includes projects dir" (postman `disableTempProjectBackup`/`restoreDisabled` descriptions imply project restore scope)

### Tertiary (LOW confidence — flagged for validation)
- API-token auth on `POST /data/api/v1/trial` (plausible, untested — spike task 1)
- Exact gateway session cookie name + `/data/app/session` JSON field names (spike with creds)
- Whether gwbk restore also restores trial/license clock state ("repeatable state" = config+projects; trial clock behavior unknown — test during snapshot e2e)

## Metadata

**Confidence breakdown:**
- Compose mechanics: HIGH — live v5.1.2 + official docs fetched today
- Rig discovery/conventions: HIGH — real files read; note WHK-Global's live successor is whk-environment-orchestration
- Trial status wire: HIGH — both endpoints live-captured on 8.3.6
- Trial reset native flow: steps 1–5 HIGH (live-probed); steps 6–10 MEDIUM (source-verified); tier-0 token question LOW (untested)
- Snapshot/restore contract: HIGH shapes (postman primary + live 401); MEDIUM semantics (restart-after-restore, projects-in-gwbk)

**Research date:** 2026-08-22
**Valid until:** 2026-09-22 (compose/gateway both slow-moving; the IdP flow is version-sensitive — re-verify per gateway minor in e2e)
