# Domain Pitfalls — ignition-cli

**Project:** Rust CLI + ratatui TUI wrapping Ignition 8.3 Gateway REST/WebDev APIs with Docker test-rig control
**Researched:** 2026-08-20
**Sources:** Local prior art (ignition-mcp, ignition-trial-resetter, ignition-git-module docker/, WHK-Global e2e), official Inductive Automation 8.3 docs + forum API guide, 83-api Bruno/Postman collection, ratatui 0.30 docs, bollard docs, Docker Compose docs, clig.dev. Confidence noted per pitfall.

---

## 1. Gateway API Pitfalls

### 1.1 API-token auth is a three-part setup — failure modes return generic 401/403
**What goes wrong:** Sending `X-Ignition-API-Token` is necessary but not sufficient. Write operations additionally require: (a) a write-level Security Level defined under Platform > Security > Levels, (b) that level assigned to "Gateway Write Permissions" (AnyOf), and (c) the token itself assigned that security level. Miss any piece and you get a bare `403 Forbidden` with no explanation. Separately, "Require secure connections for API Keys" rejects plain-HTTP tokens even on localhost — a self-inflicted 401 that looks like a bad token. (Verified: official docs + IA forum usage guide, where an IA engineer walked a user through exactly this.) **Confidence: HIGH**

**Warning signs:**
- Health/GET commands work but project/tag writes fail with 403 in `--json` output
- Works from one machine (https rig) but not another (http://localhost rig)
- First-run smoke test against a fresh commissioning-rig gateway

**Prevention:**
- Build a `doctor` / `auth check` subcommand on day one that GETs `/data/api/v1/gateway-info`, then attempts a trivial write-scope call, and translates 401/403 into the three concrete causes ("token lacks write security level", "gateway requires HTTPS for API keys", "bad token")
- Document the three-part setup in `profile add` output
- Per-profile `allow_insecure_tls` flag (scoped to the reqwest client via `danger_accept_invalid_certs`, never process-global — see WHK-Global `e2e/lib/gateway.mjs` which learned this the hard way)

**Phase mapping:** CLI skeleton/profiles phase (first). Auth diagnostics must precede any write commands.

### 1.2 Basic auth and session-cookie flows need `X-CSRF-Token`; don't assume the token header is the only auth path
**What goes wrong:** The web UI and any session-based flow (login via `POST /data/app/login`, session via `GET /data/app/session`) require the `X-CSRF-Token` header on every mutating POST. The trial-resetter had to replay `POST /data/api/v1/trial` with session cookie + CSRF token. Tools that mix session auth and token auth, or that follow redirects into the login page, get HTML error bodies back and misparse them as API failures. Also: HTTP error bodies from the gateway are sometimes HTML (`<html>...Error 401...`), not JSON. (Verified: trial-resetter source comments; forum thread shows HTML 401 bodies.) **Confidence: HIGH**

**Warning signs:**
- `serde_json::Error` "expected value" when parsing error responses
- Any code path that falls back from API token to user/password session auth

**Prevention:**
- Decide the auth model up front: API token is primary; basic auth is a documented fallback only if verified to work against 8.3 (ignition-mcp used it, but the 8.3 canonical path is the token); session+CSRF is reserved for the trial-reset flow only
- Error handling must sniff content-type before parsing: HTML body → surface "gateway returned HTML (auth redirect or gateway not fully up)" instead of a JSON parse panic

**Phase mapping:** Auth/profiles phase; revisit specifically in the trial-reset phase.

### 1.3 Project import/export are SYNCHRONOUS — do not design for job IDs that don't exist
**What goes wrong:** The research question presumed "async job semantics (long-running ops that return job IDs)." The 8.3 surface says otherwise: `POST /data/api/v1/projects/import/{name}` (ZIP body, `overwrite` query param) and `GET /data/api/v1/projects/export/{name}` (raw ZIP bytes in the response) are plain blocking HTTP round-trips — the Bruno collection shows no polling endpoint under `projects/`. The real trap is the inverse of the presumed one: a large project import/export blocks the HTTP connection for the entire operation; a default reqwest timeout (or an agent's patience) kills the request mid-flight with no way to know whether the import landed. The genuinely async surfaces are elsewhere: EAM tasks (`/data/api/v1/eam-tasks/...` with history/suspend/resume), gateway backup restore, and remote-upgrade. (Verified: 83-api collection endpoint shapes; ignition-mcp client code.) **Confidence: HIGH**

**Warning signs:**
- `ClientTimeout` errors on `project import` of WHK-Global-sized projects
- Any roadmap item like "poll import job status"

**Prevention:**
- Disable/raise the per-request timeout for import/export specifically (builder-level timeout override, e.g. 5–10 min), stream export ZIP bytes to disk (`bytes_stream`) rather than buffering in memory
- Make import idempotent-friendly: document `overwrite=` semantics, and treat a timed-out import as "unknown state — verify with `project list` / `resource list`," never as success or failure
- If long-ops later need job semantics, look at EAM tasks, not project endpoints

**Phase mapping:** Project operations phase. Timeout policy belongs in the HTTP client layer from the start.

### 1.4 Config resources use signature-based optimistic concurrency
**What goes wrong:** Modifying, renaming, or deleting 8.3 config resources (tag providers, connections, schedules — everything under `/data/api/v1/resources/`) requires the resource `signature` obtained from a prior read; DELETE literally takes `/name/<signature>` in the URL path. A client that reads, mutates, then deletes with a stale signature gets a conflict; one that never fetches the signature gets a 400. (Verified: official 8.3 OpenAPI docs page + forum guide's "Updating and/or renaming resources will require that the correct signature...".) **Confidence: HIGH**

**Warning signs:**
- 400/409 on tag-provider delete/rename that "should work"
- Any `resources` CRUD implemented as fire-and-forget

**Prevention:**
- Model config resources as read-modify-write in the client layer: `get → signature → mutate`, with conflict errors surfaced as "resource changed since read by <who/audit>" rather than a raw status code
- Note mutative API calls are audit-logged with user + IP + API key name (GETs are not) — leverage `audit` endpoints when debugging "who changed this"

**Phase mapping:** Tag provider/config operations phase (tag ops phase).

### 1.5 Projects don't contain tag providers — "sync projects" ≠ "sync a gateway"
**What goes wrong:** In 8.3, tag providers (and their tags/UDTs as config) live under `data/config/resources/`, outside project export ZIPs. A `project export → project import` cross-gateway sync silently omits tags, themes/config, unless handled separately — the git-module's `git.yaml` commissioning has distinct `commissioning_importTags` / `importThemes` flags precisely because of this separation. A `sync` command that promises more than project resources delivers a gateway that looks synced but has no tags. (Verified: ignition-git-module docker readme + 83-api resource layout.) **Confidence: HIGH**

**Warning signs:**
- Roadmap wording like "sync projects cross-gateway" with no explicit tag/config scope decision
- QA against a rig that happens to share tag definitions via WHK-Global commissioning, masking the gap

**Prevention:**
- Name the scope explicitly in command semantics: `project export` (resources only, documented), and a separate `sync`/`snapshot` concept that composes project resources + tag provider config (via `/data/api/v1/resources/.../tag-provider` reads/writes) if and when cross-gateway sync is built
- Write the limitation into `--json` output metadata (`{"includes": ["project-resources"], "excludes": ["tag-providers", ...]}`) so agents don't over-trust

**Phase mapping:** Project operations phase — decide scope before building sync.

### 1.6 Filesystem-level changes are invisible until a scan or restart
**What goes wrong:** Config/resource files edited directly on disk (volume mounts, snapshot restores, `.gwbk` extraction, token files copied into `data/config/resources/EXTERNAL/...`) are NOT picked up automatically. The gateway needs `POST /data/api/v1/scan/config` (or `scan/projects` for project files) or a restart. Rig snapshot/restore and trial tooling that manipulates volumes will appear to "not work" because the gateway never noticed. (Verified: forum usage guide "Important Notes"; CORE vs EXTERNAL resource inheritance also documented there.) **Confidence: HIGH**

**Warning signs:**
- Snapshot restore "succeeds" but gateway shows old config
- Any rig command that writes into the data volume

**Prevention:**
- After any volume/file-level operation, rig commands must either restart the gateway or call the scan routes before reporting success; encapsulate as `rig restore → scan config → scan projects → wait RUNNING`

**Phase mapping:** Rig lifecycle phase (snapshot/restore specifically).

### 1.7 Gateway restart is a multi-stage wait, not a call-and-return
**What goes wrong:** `POST /data/api/v1/restart-tasks/restart?confirm=` returns immediately; then the port stops accepting, then accepts while modules still load, then the platform reaches `RUNNING`. Polling TCP or the first 200 from `/data/api/v1/gateway-info` reports "up" while Perspective/WebDev aren't ready. Total time varies 30s–several minutes with modules. Also: `GET /data/api/v1/restart-tasks/pending` exists because some config changes queue a required restart — commands that mutate config should surface pending-restart state. (Endpoints verified in 83-api collection; RUNNING-state polling is standard practice reflected across WHK-Global/ignition-mcp tooling.) **Confidence: HIGH (endpoints) / MEDIUM (timing numbers)**

**Warning signs:**
- Intermittent "connection refused" right after `gateway restart` in scripts
- WebDev calls failing after a restart that the health command called healthy

**Prevention:**
- Implement one shared `wait_until_ready(profile, timeout)` primitive: poll `gateway-info` until state == RUNNING with backoff and a generous default timeout (≥180s), and use it after restart, rig up, and restore
- `gateway status` should include `pendingRestart: bool` from restart-tasks/pending

**Phase mapping:** Gateway health phase — the wait primitive is foundational and reused by the rig phase.

### 1.8 WebDev routes are a bespoke contract you own — version-drift and error-shape discipline is on you
**What goes wrong:** The WebDev layer (this CLI ships its own routes) wraps `system.tag.*` etc. in hand-written Jython `doPost` handlers. Prior art shows the failure modes: everything comes back stringified (the alarm-history route `str()`s every column, so numbers/dates arrive as strings), response shapes drift between routes (`{"reads": [...]}` vs bare arrays — WHK-Global's client defensively tries three shapes), errors return 200 with an `{"error": ...}` body unless you're careful, and Jython 2.7 quirks bite (no f-strings). Also WebDev requires the WebDev module licensed on the gateway and a hosting project — deployments to a bare rig fail opaquely. (Verified: ignition-mcp webdev-setup.md scripts, WHK-Global gateway.mjs defensive parsing.) **Confidence: HIGH**

**Warning signs:**
- Rust structs with `String` fields where values are logically numeric/temporal
- Client code with fallback chains trying multiple JSON shapes
- Tests only passing against a gateway where someone manually deployed routes once

**Prevention:**
- Design the route contract once, as a versioned schema (route file header `# contract v1`), with the CLI refusing to talk to a route reporting an incompatible contract version (pass `clientVersion` in, get `contractVersion` back)
- Normalize types on the gateway side where possible, and in the CLI's serde layer where not (custom deserializers for stringified values); never echo raw WebDev JSON in `--json` output
- Non-2xx HTTP codes for genuine errors (routes can return `{'response': {'code': N}}`); map to typed CLI errors
- Ship route deployment as a first-class CLI capability (`webdev deploy`) so tests and rigs can bootstrap routes themselves — this kills the chicken-and-egg problem
- Script-exec route stays opt-in + audit-logged (ignition-mcp precedent: disabled by default)

**Phase mapping:** Tag operations phase (WebDev scaffolding ships with it).

### 1.9 Trial reset is an auth+CSRF web flow, and browser automation is a version-skew minefield
**What goes wrong:** Trial *state* is easy: unauthenticated `GET /data/api/v1/overview/banners` (severity + expireTime). Trial *reset* is `POST /data/api/v1/trial` requiring an authenticated session + `X-CSRF-Token` — the existing resetter drives a headless browser because the login flow (two-step 8.3 form, IdP challenge) was easier automated than replayed. But browser automation broke across 8.3.3's web UI rewrite (the `#reset-trial-anchor` id vanished; selectors are now text-based and fragile). A Rust CLI that leans on UI selectors inherits that fragility; one that replays the HTTP flow owns the login/CSRF/IdP handling. (Verified: extensive comments in ignition-trial-resetter/reset-trial.mjs, written from real breakage.) **Confidence: HIGH**

**Warning signs:**
- Any plan to "just use the banner endpoint" for reset (read-only — it can't reset)
- Selector-based or screenshot-based reset logic in a CLI roadmap
- Trial reset tested only on one gateway minor version

**Prevention:**
- Implement the headless path: login flow (two-step form semantics) → session cookie + CSRF from `/data/app/session` → `POST /data/api/v1/trial` → verify via banners endpoint flipping to active (objective confirmation, as the resetter does)
- Treat banners endpoint as the single source of truth for state; expose `rig trial status --json` from it (free feature)
- Version-slew test the reset across ≥2 gateway versions before trusting it; keep the Playwright script as a documented fallback, not the primary path

**Phase mapping:** Rig lifecycle phase (trial reset feature).

### 1.10 Client-side timeout discipline (learned pattern, generalize it)
**What goes wrong:** One default timeout for all calls fails in both directions: 30s kills big imports; a 10-minute timeout on a tag read makes the TUI feel dead. ignition-mcp already learned the variant: set the HTTP timeout slightly LONGER than the gateway-side execution timeout so the gateway returns a clean error instead of the connection being cut ambiguously. **Confidence: HIGH (pattern from prior art)**

**Prevention:**
- Per-operation-class timeouts in the client layer: fast reads (10s), script exec (exec timeout + 5s), import/export (long/disabled); surfaced as CLI `--timeout` overrides

**Phase mapping:** HTTP client module, skeleton phase.

---

## 2. TUI (ratatui) Pitfalls

### 2.1 Blocking the event loop with synchronous gateway calls
**What goes wrong:** The canonical ratatui loop is `draw → event::read()` — and `crossterm::event::read()` blocks; ratatui ships no async runtime integration. Naively calling `reqwest::blocking` (or a long `project export`) inside the loop freezes the UI: no redraws, no Ctrl-C responsiveness, user assumes a hang and kills the terminal. (Verified: ratatui 0.30 docs — "Ratatui does not provide built-in input handling"; `event::read` is blocking.) **Confidence: HIGH**

**Warning signs:**
- Any `blocking` client in a dependency list that also contains ratatui
- UI updates only after an operation finishes; cursor frozen during gateway calls
- Missing `tick`/timeout in the event wait (no periodic redraw for elapsed-time or progress UI)

**Prevention:**
- Architecture from day one: the TUI is a *viewer over the same command layer the CLI uses*; all I/O runs on a worker (tokio tasks or a thread pool) communicating via `mpsc` channels; the draw loop only ever polls (`event::poll(timeout)`) and drains messages — one event/poll + message-drain loop, redraw on any state change or tick
- Every gateway action fired from the TUI must be cancellable and report progress (this constraint also keeps the CLI's JSON layer clean)

**Phase mapping:** TUI phase architecture spike — but the *command layer separation* that makes it possible must exist from the skeleton phase.

### 2.2 Terminal state corruption on panic
**What goes wrong:** A panic while in raw mode + alternate screen leaves the user's terminal destroyed (no echo, ANSI garbage) — classic ratatui complaint. ratatui ≥0.28 ships `ratatui::init()` which installs a restoring panic hook, and `ratatui::restore()`. Rolling your own terminal setup and forgetting the hook (or restoring in a `Drop` that panics during unwinding) reproduces the bug. (Verified: ratatui 0.30 docs, `init`/`restore`/panic-hook source.) **Confidence: HIGH**

**Warning signs:**
- Manual `enable_raw_mode()` / `EnterAlternateScreen` anywhere in the codebase instead of `ratatui::init()`
- Bug reports containing "terminal frozen after crash"
- Panicking helper threads that print while the alternate screen is active

**Prevention:**
- Use `ratatui::init()`/`restore()` verbatim; add a CI test that panics inside the TUI and asserts the terminal state (testcontainers-style guard is overkill; a simple spawn-in-pty smoke test suffices)
- Prefer typed errors over unwraps in TUI code paths (`clippy::unwrap_used` deny in CI for the tui module); route errors into the UI's error pane instead of panicking

**Phase mapping:** TUI phase, enforced at first TUI PR.

### 2.3 Untestable TUI code (God-object app state)
**What goes wrong:** TUI apps written as one monolithic `run()` loop with inline business logic become untestable without a live gateway + real terminal, so they ship untested. ratatui's answer is the component pattern + `TestBackend` (render to an in-memory buffer and `assert_buffer`/`assert_buffer_lines` against expected content). (Verified: ratatui 0.30 `TestBackend` docs.) **Confidence: HIGH**

**Warning signs:**
- Draw functions that also perform I/O
- Zero tests touching `ui.rs`/views
- State mutated directly from event handlers with no message indirection

**Prevention:**
- Enforce the Elm-ish split: `Model` (pure state) / `update(msg) -> Model` (pure) / `view(&Model)` (pure); workers and I/O live outside; then unit-test `update` with synthetic messages and `view` with `TestBackend` — no gateway needed for 90% of TUI tests
- Snapshot a few key screens (buffer assertions) so widget regressions are caught in CI

**Phase mapping:** TUI phase; the pattern must be in the phase plan, not retrofitted.

### 2.4 TUI as "viewer" scope creep vs. "full cockpit" requirement
**What goes wrong:** PROJECT.md demands every CLI action be available in the TUI. Teams build list/detail views first and quietly never wire destructive/long ops ("import", "rig reset") into the TUI, or bolt them on as blocking calls (see 2.1). The other direction — building interactive-only flows — breaks the agentic contract. **Confidence: MEDIUM (judgment, grounded in PROJECT.md constraints)**

**Prevention:**
- Treat the TUI as a menu over the command layer: each TUI action invokes the exact same command function the CLI subcommand uses (shared `Command` enum), so "full cockpit" coverage is structural, not aspirational; a coverage test can assert every registered CLI subcommand has a TUI action mapping

**Phase mapping:** TUI phase planning; command-layer design in skeleton phase.

---

## 3. Docker / Rig Pitfalls

### 3.1 bollard vs. `docker compose` — compose files are the source of truth, and bollard can't read them
**What goes wrong:** Reimplementing rig lifecycle on bollard (Docker Engine API) means re-implementing compose: file parsing (Compose Spec), interpolation, dependency ordering, named volume conventions, project-name namespacing. The `docker-compose` Rust crates are unmaintained. Compose behavior is subtle (`up` ≠ recreate unless config changed; `down` without `-v` keeps volumes) and every divergence from real compose breaks interop with the git-module/WHK-Global rigs the project must pair with (PROJECT.md constraint). (Verified: bollard docs cover Engine API only — no compose support; Docker docs define Compose Spec behavior.) **Confidence: HIGH**

**Warning signs:**
- Any plan to model rigs as raw container definitions instead of compose files
- Custom YAML structs duplicating compose fields

**Prevention:**
- Opinionated call: shell out to `docker compose` (subprocess) for lifecycle (`up -d --wait`, `down -v`, `ps`, `logs`) with explicit binary resolution (`docker compose` plugin first; treat legacy `docker-compose` v1 as unsupported — it's EOL and ignores the Compose Spec) and a version check at startup; keep the dependency tree lean per PROJECT.md
- Use bollard only if a targeted need emerges later (streaming logs, exec without tty allocation); don't carry it now
- Rig discovery = find compose file + project name, never "create containers ourselves"

**Phase mapping:** Rig lifecycle phase; binary-resolution + version-check helper in that phase's first task.

### 3.2 Compose version skew and the obsolete `version:` key
**What goes wrong:** Compose v1 (Python, `docker-compose`) is EOL and chokes on Compose Spec files; the top-level `version:` element is obsolete and ignored by v2+ (which print a warning). Rigs cloned from older checkouts (git-module's compose files) may carry assumptions that break under newer Docker. If the CLI silently invokes whichever compose binary is first on PATH, behavior differs per machine. (Verified: Docker docs — Compose history page: v1 vs v2/v5, `version` optional/ignored.) **Confidence: HIGH**

**Prevention:**
- Resolve and validate the compose plugin version once (`docker compose version`) and fail with a clear message on v1; never use `version:` in any compose file the CLI itself generates; tolerate (but don't require) its presence in discovered files

**Phase mapping:** Rig lifecycle phase.

### 3.3 Orphaned containers/volumes accumulating on reset
**What goes wrong:** Rig "reset" done as `down && up` (no `-v`) leaves named volumes → stale gateway DB, trial state, and half-old projects produce bizarre "reset didn't work" bugs. Rename a service in a compose file and `up` leaves the old container running as an orphan (compose prints a warning scripts never read). Multiple rigs sharing implicit project names (directory-name default) collide: two checkouts of the same repo both become project `docker`, fighting over the same containers/ports. (Compose semantics verified via Docker docs; orphan behavior is standard compose.) **Confidence: HIGH**

**Warning signs:**
- `rig reset` followed by old data persisting
- `docker ps` showing duplicated `*-gateway` containers after a few weeks of use
- Port-already-bound errors that "resolve after a reboot"

**Prevention:**
- `rig reset` = `down --remove-orphans -v` → `up -d --wait` → `wait_until_ready` (see 1.7); `--remove-orphans` on every lifecycle op
- Derive compose project name explicitly (`-p`) from the rig profile (e.g. directory + profile), never the implicit directory default; store it in the rig profile so `reset`/`down` always target the same namespace
- `rig status --json` should list volumes + orphans it would remove; `rig down` defaults to keeping volumes, with `--volumes` flag for full teardown (destructive ops explicit — clig.dev)

**Phase mapping:** Rig lifecycle phase.

### 3.4 Trial/license state and reset timing — the gateway must be RUNNING for API reset, STOPPED for file-level reset
**What goes wrong:** Two different "trial resets" exist: (a) API/web reset (see 1.9) — requires the gateway up; (b) file/DB-level resets (deleting license/trial state in the data volume, gwbk restores) — require the gateway STOPPED, or the in-memory/DB state wins and/or corrupts. Commands that mix the two (stop container → edit files → start → banner still expired because reset never ran) produce flaky, time-wasting sessions. Fresh commissions additionally re-run first-boot behavior only when the data volume is empty — creds via `GATEWAY_ADMIN_*` env only apply at commissioning. (Verified: reset-trial.mjs; git-module readme commissioning semantics.) **Confidence: HIGH**

**Prevention:**
- Make `rig trial reset` prefer the API path on a running gateway (1.9), fall back to a documented manual/file path; if any file-level reset is implemented, enforce container-stopped as a precondition in the command itself
- Rig up after volume wipe must re-provision API tokens (they're config resources) — script token creation as part of rig bring-up (`rig up --provision` or a post-up hook), or trial-state + auth break together

**Phase mapping:** Rig lifecycle phase (trial reset).

### 3.5 Port conflicts on multi-rig machines
**What goes wrong:** Gateway rigs bind well-known host ports (8088/9043 etc.). Two rigs up simultaneously → second `up` fails with "port already allocated" mid-bring-up, leaving a half-created rig; or worse, CLI commands target rig A's profile but the port now belongs to rig B (profiles store URLs, ports get reused). WHK-Global already runs a non-default port (9043/9088) evidence of this pressure. **Confidence: HIGH (operational certainty)**

**Prevention:**
- On `rig up`: pre-flight check host port availability for all compose port mappings; fail fast with "port 8088 in use by container X (rig Y)" instead of letting compose die mid-flight
- Store the mapped port in the rig profile at bring-up time and have gateway commands validate the profile's URL matches the rig's actual mapping before operating (`rig status` cross-check)

**Phase mapping:** Rig lifecycle phase.

### 3.6 Secrets in container env and inspect output
**What goes wrong:** Compose rigs inject `GATEWAY_ADMIN_PASSWORD` / API tokens as container env — visible to anyone via `docker inspect`. A naive `rig info --json` that dumps container config leaks credentials into agent transcripts and logs; the git-module automated build even bakes creds into a `.gwbk` at build time. (Verified: git-module docker readme; standard docker behavior.) **Confidence: HIGH**

**Warning signs:**
- Any `--json` output containing `Env`, `env`, `password`, `token` keys
- Profiles storing tokens world-readable (`ls -l ~/.config/ignition-cli` shows 644)

**Prevention:**
- Allowlist fields in rig/profile JSON output (never serialize raw container inspect); redact env values matching secret patterns with `***` and a `--reveal` escape hatch for humans only
- Config/profile files created `0600`; tokens read from files (`--token-file`) preferred over flags/env (clig.dev: flags leak via `ps` and history); support the existing `IGNITION_API_TOKEN` file convention WHK-Global uses

**Phase mapping:** Profiles phase (file perms + redaction from the first PR), rig phase (inspect allowlist).

---

## 4. Agentic CLI Pitfalls

### 4.1 Inconsistent JSON shapes across subcommands
**What goes wrong:** The natural implementation returns whatever the gateway/WebDev sent — so `project list` returns an array, `gateway status` an object, `tag read` a third shape; error output differs again (HTML bodies! stringified timestamps! `str()`-ified alarm rows from WebDev). Agents composing commands then need per-command parsing logic and hallucinate fields. This is the #1 agentic-CLI failure mode and the raw-material for it is worst in this domain (see 1.8). **Confidence: HIGH (clig.dev + prior-art evidence)**

**Prevention:**
- One envelope for everything: `{"ok": bool, "data": {...} | [...], "error": {"code", "message", "details"?} | null}` on stdout in `--json` mode; exit code carries the same signal
- Typed serde structs per command output (serde deny_unknown_fields off, but fields explicit); normalize timestamps to RFC3339, statuses to fixed enums; never pass through raw WebDev payloads
- A golden-file test per subcommand's JSON shape; breaking shape changes = minor version bump + changelog note (agents depend on stability)
- Table stakes: `--json` on EVERY subcommand (PROJECT.md requirement), stdout = data only, diagnostics → stderr (clig.dev)

**Phase mapping:** Skeleton phase (envelope + error taxonomy before the third command exists); enforced by CI from then on.

### 4.2 Interactive prompts blocking scripts/agents
**What goes wrong:** First-run wizards, "are you sure?" y/n, and profile pickers hang forever when stdin isn't a TTY (agents, cron, CI). Conversely, skipping all confirmation makes destructive ops (delete project, rig down -v) one typo away. (clig.dev: prompt only if TTY; `--no-input`; scriptable confirmation flags.) **Confidence: HIGH**

**Prevention:**
- Every prompt gated on `stdin.is_terminal()`; non-TTY + missing arg = immediate error naming the exact flag to pass
- Global `--no-input`; destructive ops require `--force` or `--confirm <name>` (type-the-name for severe ops like `project delete` on a non-rig profile) — clig.dev's danger ladder
- Defaults: non-interactive IS the default (PROJECT.md constraint); interactivity is an enhancement, never a requirement

**Phase mapping:** Skeleton phase (flag conventions), each command phase applies them.

### 4.3 Exit code inconsistency
**What goes wrong:** clap exits 2 on usage errors; `main() -> Result` exits 1 on everything else; commands that "fail gracefully" (gateway unreachable, project not found) exit 0 with an error in output — scripts and agents can't branch. (clap's `Error::exit`: usage errors → 2, help/version → 0. Verified.) **Confidence: HIGH**

**Prevention:**
- Fixed taxonomy, documented in `--help` and README: 0 success; 1 unexpected internal error; 2 usage (clap); 3 gateway unreachable/not ready; 4 auth/permission failure; 5 not-found (project/tag/rig); 6 operation timeout; 7 rig/docker failure. Use `std::process::exit(code)` from a single exit-point helper (bypass `Result`-main's blanket 1) — and print the human error to stderr, JSON envelope to stdout when `--json`
- CI test asserting exit codes for representative failure classes

**Phase mapping:** Skeleton phase.

### 4.4 Secrets and noise leaking into logs/transcripts
**What goes wrong:** Beyond 3.6: verbose/debug logging of HTTP headers (Bearer/token), gateway log tailing commands that print credentials from gateway logs, and `--verbose` request-dump debugging that ends up pasted into agent context. **Confidence: HIGH**

**Prevention:**
- Request logging redacts auth headers by default (`X-Ignition-API-Token: ***`); full-header dumps only behind `IGNITION_CLI_DEBUG_SECRETS=1` with a loud warning
- `gateway logs` filters/tail supports redaction patterns; never log profile secrets on load failures (log the key name, not the value)

**Phase mapping:** Skeleton phase (logging setup), gateway-logs command phase.

### 4.5 Long-running commands with no progress or cancellation story
**What goes wrong:** `rig up` (pull + commission + wait = minutes), `project import` (see 1.3), `gateway restart` — an agent-invoked CLI that goes silent looks hung; an agent that kills it mid-compose leaves half-state. (clig.dev: "a command is saying too little when it hangs for several minutes".) **Confidence: HIGH**

**Prevention:**
- Progress to stderr (spinner/steps when TTY, line events when not — agents get parseable stage markers like `[stage] pulling image…`); result JSON on stdout only at completion
- Handle SIGINT gracefully: on Ctrl-C during rig ops, print the state actually reached ("compose up completed, gateway still starting — run `ignition rig status`") instead of dying silently; idempotent re-runs are the recovery model

**Phase mapping:** Rig phase + project phase; signal handling in skeleton.

---

## 5. Scope-Creep Traps (boundaries from sibling tools)

### 5.1 Drifting into linting (ignition-lint's territory)
**What it covers (verified readme):** Jython syntax errors in view.json bindings, naming conventions, deprecated APIs, `now()` polling checks, unused properties — CLI (`ignition-lint`), GitHub Action, pre-commit, MCP server, JSON report output. **Trap:** adding "just a quick tag/script validation" before import, then rule creep into full linting — duplicated maintenance, two sources of truth. **Prevention:** no lint rules in this CLI, ever. If pre-import validation is wanted, delegate: `ignition project import --lint` shells out to `ignition-lint` as an OPTIONAL external dep (it's Python/PyPI — probe for it at runtime, degrade gracefully with a pointer). Record as an explicit Out of Scope entry now. **Phase mapping:** project operations phase design discussion; PROJECT.md already lists it Out of Scope — keep it there. **Confidence: HIGH**

### 5.2 Drifting into LSP/editing/decode-encode (ignition-nvim's territory)
**What it covers (verified readme):** `system.*` completions (239+ functions), Java/Jython completions, project-script navigation, script decode/encode (embedded Python ↔ JSON), .gwbk browsing via Kindling, diagnostics integration — in Neovim + VS Code via a shared LSP. **Trap:** since this CLI can fetch/PUT project resources (native REST), "script editing helpers" or resource-tree browsing with editing affordances creep in. The subtle one: a `resources get/set` command is in-scope plumbing (agents need it) but starts looking like an editing tool if we add templating/snippets. **Prevention:** resource get/put = raw bytes/JSON, no decode-encode, no diff-editing, no snippets. Editing happens in the user's editor with ignition-nvim. If a nicer handoff is wanted later, the boundary-respecting feature is `ignition project resource cat <path>` (stdout) — compose-with-editor is the user's business. **Phase mapping:** project/resources phase. **Confidence: HIGH**

### 5.3 Drifting into Designer/git workflows (ignition-git-module's territory)
**What it covers:** Git embedded in the Designer; its `docker/` dir is rig pattern source (compose, gw-init, gw-secrets, gw-build). **Trap:** adding `ignition project git sync`-style commands, or rig features that re-implement commissioning logic (stash/fetch/pull on gateway start) in the CLI. **Prevention:** rig commands orchestrate compose lifecycle ONLY; they must never duplicate the git-module's idempotent-sync semantics. If a `project sync` from git is demanded later, it should shell out to git against an exported copy, clearly outside v-scope. Compose-file discovery conventions pair with git-module rigs (PROJECT.md constraint) — share the pattern (env/secrets layout, IGNITION_VERSION pinning), not the logic. **Phase mapping:** rig lifecycle phase. **Confidence: HIGH**

### 5.4 Drifting into MCP serving (ignition-mcp's replacement boundary)
ignition-mcp remains the MCP transport until this CLI replaces it (PROJECT.md: not v1). **Trap:** bolting an MCP server mode onto the CLI early "since the tool catalog already exists." **Prevention:** v1 interface = CLI + JSON only; the envelope discipline (4.1) is precisely what makes a later MCP façade trivial. Defer. **Confidence: HIGH**

### 5.5 Generic scope-creep pressure unique to this ecosystem
The gateway API is huge (the 83-api collection has dozens of resource types: alarms, reports, redundancy, EAM, OPC, certificates...). "Complete cockpit" invites building CRUD for everything. **Prevention:** the ignition-mcp 37-tool catalog is the explicitly sanctioned scope of API coverage (PROJECT.md says so) — new endpoint areas require a PROJECT.md requirement first, not a quiet PR. Depth over breadth: health/projects/tags/rigs done excellently beat twenty half-CRUDs. **Confidence: HIGH**

---

## 6. Testing Pitfalls

### 6.1 Live-gateway dependence in unit tests
**What goes wrong:** Tests that need a real gateway become "run manually against my rig" tests — they don't run in CI, rot, and pass/fail depending on the rig's trial/license state. ignition-mcp's own structure is the proven pattern here: unit tests fully mocked; integration tests skipped unless `RUN_LIVE_GATEWAY_TESTS=1`. **Confidence: HIGH**

**Warning signs:**
- Tests with hardcoded `localhost:8088` and real credentials
- `#[ignore]`d tests that nobody un-ignores
- CI that "can't" test the HTTP client layer at all

**Prevention:**
- Layered strategy: (1) client layer unit-tested against `mockito`/`wiremock`-style local servers exercising REAL HTTP semantics incl. HTML error bodies (1.2), timeouts (1.10), and ZIP streaming; (2) command layer tested against a trait-based `GatewayClient` mock (the same seam the TUI uses — 2.1); (3) live-gateway integration behind env flag, run nightly/on-demand against a composed rig, never required for PR merge
- Record/replay fixture library (saved gateway JSON responses) for shape-normalization tests — protects 4.1's golden shapes without a gateway

**Phase mapping:** skeleton phase (trait seam + wiremock), every later phase adds fixtures not live deps.

### 6.2 Flaky compose-based e2e
**What goes wrong:** A compose-based e2e rig flakes via: image pull time (first run: minutes), gateway commissioning (30–90s+ before RUNNING), trial expiry mid-suite, port collisions with dev rigs on the same machine (3.5), and leaked containers from failed runs poisoning the next run. Green-local-red-CI cycles follow. **Confidence: HIGH (operational certainty + rig timing from prior art)**

**Prevention:**
- e2e harness rules: pre-pull the gateway image before the suite; fixed compose project name unique to e2e (`ignition-cli-e2e`) so it can never collide with dev rigs; `down --remove-orphans -v` in a trap/finally on BOTH success and failure; generous `wait_until_ready` (1.7) with backoff as the only readiness gate; one full rig lifecycle per suite (not per test) with project-scope isolation between tests
- Flaky-budget policy: any e2e test that fails twice without a code cause gets a retry-once + quarantine, not a blind `sleep` (never sleep-based synchronization anywhere — poll conditions only)
- Trial-state guard at suite start: assert banners endpoint shows active trial, else reset (1.9) or fail fast with the reason — no mysterious mid-suite license failures
- WebDev route deployment is part of suite setup (via the CLI's own `webdev deploy`, 1.8) — proving bootstrap works, and removing the "manually deployed once" dependency

**Phase mapping:** first appears in tag-ops/WebDev phase (needs routes), fully exercised in rig phase; harness skeleton in the project-ops phase when the first e2e lands.

### 6.3 Testing the TUI by hand only
Covered in 2.3 — the trap restated for testing: without component/TestBackend discipline, TUI testing degenerates to manual clicking, coverage stays ~0, and every refactor risks visual regressions silently. **Phase mapping:** TUI phase. **Confidence: HIGH**

---

## Phase-Specific Warnings Summary

| Phase (likely) | Top pitfalls to address there |
|---|---|
| CLI skeleton + profiles + auth | 1.1, 1.2, 4.1, 4.2, 4.3, 4.4, 3.6 (file perms), 1.10, 6.1 (trait seam) |
| Gateway health | 1.7 (wait primitive), 4.5 (restart progress/cancel), 4.4 (logs redaction) |
| Project operations | 1.3 (sync import/export + timeouts), 1.5 (scope decision), 5.1/5.2 (boundaries), 6.2 (first e2e) |
| Tag ops + WebDev routes | 1.8 (contract versioning, deploy cmd), 1.4 (signatures), 6.2 (route bootstrap in suite) |
| Rig lifecycle + trial reset | 3.1–3.6, 1.9, 1.6 (scan after restore), 4.5 |
| TUI cockpit | 2.1–2.4, 6.3 |

## Sources

- ignition-mcp readme + `ignition_client.py` + `docs/webdev-setup.md` (local prior art; client patterns, WebDev contracts)
- ignition-trial-resetter `reset-trial.mjs` (local; banners endpoint, CSRF/session trial reset, 8.3.3 UI rewrite breakage)
- ignition-git-module `docker/readme.md` (local; commissioning, env/secrets, compose patterns, importTags separation)
- WHK-Global `e2e/lib/gateway.mjs`, `e2e/reset_trial.mjs` (local; scoped TLS, defensive WebDev parsing, token-from-file convention)
- 83-api Bruno/Postman collection (local mirror of inductiveautomation/83-api; endpoint shapes, restart-tasks, projects import/export, resource routes)
- Ignition 8.3 official docs — API Documentation page (X-Ignition-API-Token, signature-bearing DELETE, /openapi, audit logging): docs.inductiveautomation.com/docs/8.3/platform/gateway/openapi
- IA Forum "Ignition 8.3 API Usage Guide" (auth three-part setup, secure-connection flag, CSRF session flow, scan routes, CORE/EXTERNAL resources, HTML error bodies)
- ratatui 0.30 docs via Context7 (init/restore panic hook, blocking event::read, TestBackend, no built-in input/async)
- clap docs via Context7 (Error::exit codes: 2 usage / 0 help)
- bollard docs via Context7 (Engine API only — no compose)
- Docker docs — Compose history (v1 EOL, v2/v5, Compose Spec, `version:` obsolete)
- clig.dev (stdout/stderr discipline, --json, interactivity/TTY rules, dangerous-op confirmation ladder, secrets via files not flags, progress feedback)

**Overall confidence: HIGH** — the majority of pitfalls are verified against primary sources (official docs, local working code that already hit them). The few MEDIUM items (restart timing numbers, TUI-scope judgment) are flagged inline.
