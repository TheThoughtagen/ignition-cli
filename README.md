# ign — operate Ignition 8.3+ gateways from the terminal

One binary that lets a developer (or an AI agent) fully operate and inspect an
Ignition 8.3+ gateway — health, projects, tags, rigs — without opening the
gateway webpage or Designer. Every subcommand is non-interactive by default
and scriptable with JSON output; the TUI cockpit arrives in a later phase.

## Output contract (for agents)

Success prints a stable envelope on **stdout**; errors go to **stderr** in
both modes (exit code carries the same signal):

- Success: `{"ok": true, "profile": <name|null>, "data": {...}}`
- Failure: `{"ok": false, "profile": <name|null>, "error": {"code": "<slug>", "message": "...", "endpoint": <url|null>, "hint": "..."}}`

`--compact` implies `--json` and renders the same envelope on a single line.
Two documented exceptions: clap usage errors exit 2 and are rendered by clap
itself (not JSON); the exit-2 class also covers destructive operations run
without `--yes`.

One success-path exception: `ign completions <SHELL>` writes the raw
completion script to stdout regardless of `--json` — shells source its
output directly, so it is never JSON-wrapped.

## Exit codes

| Code | Class         | Meaning                                            | Stable slugs
|------|---------------|----------------------------------------------------|-----------------------------------------------|
| 0    | ok            | success                                            | —
| 1    | internal      | unexpected failure — report as a bug               | `internal`
| 2    | usage         | usage error (rendered by clap), destructive op without `--yes`, an invalid import file, or an unreadable command input | `confirmation_required`, `invalid_import_file`, `invalid_input`
| 3    | config        | local configuration problem                        | `profile_not_found`, `no_active_profile`, `secret_unavailable`, `config_invalid`
| 4    | network       | gateway unreachable / timeout / TLS                | `network_error`
| 5    | auth          | gateway rejected credentials                       | `auth_rejected`
| 6    | target_state  | command invalid for the gateway's current state    | `gateway_too_old`, `gateway_not_commissioned`, `gateway_restarting`, `not_found`, `project_exists`, `resource_binary`, `trial_not_expired`, `routes_not_deployed`, `webdev_unlicensed`, `route_version_mismatch`, `webdev_route_error` |
| 7    | rig           | docker/compose rig failure (discovery, lifecycle, port conflicts) | `rig_error` |

The exit-code table lives in exactly two places — this README and
`CoreError::exit_code()` in `crates/ignition-core/src/error.rs` — kept in
sync by the enumerated mapping unit test and the golden-file contract tests.

## Gateway authentication (8.3)

Machine auth against Ignition 8.3 `/data` REST routes is **token-only**:

- Send the header `X-Ignition-API-Token: <name>:<key>` — the FULL
  `name:key` string the gateway UI shows when you create the key
  (Platform → Security → API Keys). Key-only headers are rejected (401).
- **Basic auth does not work on 8.3 `/data` routes** (verified: valid
  commissioned admin credentials → 401). `ign` keeps the basic-credential
  profile arm for future/legacy surfaces, warns loudly on stderr each
  time it is used against the gateway, and never silently retries —
  create an API token instead.
- Token-authenticated mutations need **no CSRF** machinery (CSRF applies
  only to cookie/session auth).

Creating a working token: gateway UI → Platform → Security → API Keys →
Create (Basic Token) → pick a security level → **uncheck "Require secure
connections"** for `http://` gateways → copy the full `name:key` string.

The two rejection codes carry distinct diagnoses:

| Response | Meaning | What to fix |
|----------|---------|-------------|
| 401 | token not recognized | the header must be the full `name:key` string — no `name:` prefix or wrong key → 401; Basic never works on `/data` |
| 403 | token recognized but under-permitted | three-part setup: (1) token holds an adequate security level, (2) gateway read/write permissions include that level, (3) "Require secure connections" unchecked for http — `ign doctor` diagnoses all three |

### Token-setup troubleshooting (the three-part failure, `ign doctor`'s bread and butter)

A 403 means the gateway RECOGNIZED your token but its security level
doesn't satisfy the gateway's permissions. The setup has exactly three
parts — all three must hold:

1. **The token holds an adequate security level.** When creating the
   key (Platform → Security → API Keys), pick a level the gateway's
   permissions already accept — only leaf paths count (granting
   `Authenticated/Roles` mid-tree logs a WARN and is ignored).
2. **The gateway's read/write permissions include that level.** Under
   8.3 defaults, Gateway Read/Write Permissions only include
   `Authenticated/Roles/Administrator` — a token granted just
   `Authenticated` gets 403 (verified live, then fixed by adding the
   level to the permission lists). `ign doctor` reads the
   `security-properties` config and shows you the actual wiring; when
   even that read 403s, the wiring is your culprit.
3. **"Require secure connections" is unchecked** when the gateway URL
   is `http://` (the create dialog CHECKS it by default — the #1
   http-rig trap; uncheck it).

`ign doctor` walks the whole chain — URL/TCP reachability, liveness
(no credential involved, so down-ness is never confused with bad
auth), commissioning, 401-vs-403, the permissions wiring, write
permission, WebDev-route presence, and Docker presence — and exits 0
whenever the diagnosis completes so agents can parse `checks[]` from a
broken setup without a nonzero exit getting in the way.

### Live-gateway verification (opt-in)

An `#[ignore]`-gated suite (`crates/ignition-core/tests/live_gateway.rs`)
runs read-only checks against a real commissioned gateway when
`IGNITION_LIVE_URL` (and optionally `IGNITION_LIVE_TOKEN`) are set:

```bash
cargo test -p ignition-core --test live_gateway -- --ignored
```

With no envs set the suite skips cleanly (green no-op). The file header
carries the one-command Docker rig recipe for reproducing a test gateway.

## Commands

| Command | What it does | Notes |
|---------|--------------|-------|
| `ign version` | CLI version; gateway check when a profile resolves | unreachable gateway degrades to a warning (exit 0) |
| `ign status` | Identity, platform (Java/OS), uptime, CPU/memory/disk, license incl. trial countdown | merges gateway-info + `/overview` + unauthenticated `/StatusPing`; authed read (exit 3 without a secret, exit 5 on bad credentials) |
| `ign modules [--quarantined]` | Every module: `id  name  version  state  licenseState` | default = healthy list; `--quarantined` swaps to the quarantined list (usually empty) |
| `ign metrics [--history]` | Current CPU %/heap and thread execution counts | `--history` appends first/last datapoint summaries per series (`systemPerformance` endpoints) |
| `ign sessions [--type designer\|perspective\|vision]` | All three session families in one merged output (`designers (N)` / `perspective (N)` / `vision (N)` sections) | `--type` filters to one family; JSON always carries all three keys (filtered-out = `[]`); replaces the webpage's Sessions pages |
| `ign sessions terminate --type <T> --id <ID> [--message MSG]` | Terminate a session (designer: prune / vision: close / perspective: terminate, `--message` shown to the user) | **destructive**: exit 2 (`confirmation_required`) without `--yes` or `IGNITION_YES=1`; a nonexistent id exits 6 (`not_found`) |
| `ign connections [--type database\|opc]` | Database/OPC connections: `name  enabled  healthchecks` | `healthchecks` is passthrough as the gateway reports it (populated detail LOW-confidence until captured live); replaces the webpage's Connections pages |
| `ign project list` | Every runnable project: `name  title  enabled  parent  inheritable` | inheritance info comes from the list items themselves; JSON items also carry `description` (all six keys always present, null when unset); replaces the webpage's Projects list |
| `ign project new <NAME> [--title --description --parent --inheritable --disabled]` | Create a project | only provided fields ride the create body (never empty-string references); the result is a `find` read-back; audit-logged server-side |
| `ign project copy <SRC> <DST>` | Copy a project with all its resources | non-destructive (creates DST) — no `--yes`; audit-logged server-side |
| `ign project rename <OLD> <NEW>` | Rename a project (native rename, not copy+delete) | non-destructive relabel — no `--yes`; audit-logged server-side |
| `ign project set <NAME> [--title --description --parent --set-enabled\|--disabled --inheritable BOOL]` | Set project fields — `--parent` IS the inheritance move (reparent) | only provided flags ride the modify body (absent = untouched); at least one field required; audit-logged server-side |
| `ign project delete <NAME>` | Delete a project | **destructive**: exit 2 (`confirmation_required`) without `--yes`; the wire DELETE always carries the server's own `confirm=true` query param (both guard layers); a nonexistent name exits 6 (`not_found`); audit-logged server-side |
| `ign project export <NAME> [-o FILE]` | Export a project as a ZIP archive | the ZIP STREAMS to disk chunk-by-chunk (no memory buffering; 120 s per-request timeout); default filename from `Content-Disposition`, else `<name>.zip`; stdout stays data-only — JSON carries `{project, file, bytes, scope}` (see scope metadata below) |
| `ign project import <NAME> --file PATH\|--file - [--collision-policy abort\|overwrite]` | Import a project from a ZIP (`-` reads stdin) | default policy **abort**: importing over an existing name exits 6 (`project_exists`) BEFORE any upload; **overwrite** is destructive — exit 2 without `--yes` and it REPLACES the entire project (resources absent from the ZIP are deleted; merge is Designer-only); a non-ZIP or >512 MB input exits 2 (`invalid_import_file`) before any network I/O; 300 s per-request timeout |
| `ign resource list <PROJECT> [--prefix PREFIX]` | A project's resource members, one path per line | rides project-export ZIP surgery (05-02): the project is exported, the member tree under `<collection>/resources/…` is mapped to user paths (`resources/` stripped — `ignition/script-python/…`); `--prefix` filters client-side; JSON items carry exactly the typed `path` |
| `ign resource get <PROJECT> <PATH>` | Read ONE resource: JSON pretty-printed, text raw — the surgical edit loop's first half | `PATH` keeps its slashes (e.g. `ignition/script-python/myscript`) and addresses a ZIP MEMBER (the file at `<collection>/resources/<rest>`); a binary member (data.bin-class, sniffed from the member bytes) refuses with exit 6 `resource_binary` (use export/import instead — never corrupted through the JSON loop); JSON data carries `{project, path, content_kind, content}` |
| `ign resource put <PROJECT> <PATH> --file PATH\|--file -` | Write ONE resource member (upsert: created if absent, replaced if present) | **destructive**: the whole project is re-imported (`overwrite=true`) after the member surgery — exit 2 (`confirmation_required`) without `--yes`; content is sniffed (json/text); binary input refuses exit 6 `resource_binary` before any network I/O; an unreadable file/stdin exits 2 `invalid_input`; concurrent Designer edits are REPLACED (see the resource section) |
| `ign resource delete <PROJECT> <PATH>` | Delete ONE resource member | **destructive**: exit 2 (`confirmation_required`) without `--yes`; the surgery drops the member and re-imports the project; a nonexistent path/member exits 6 (`not_found`) |
| `ign logs [--logger L] [--min-level L] [--since SPAN] [--limit N]` | Recent log entries, newest first (`ISO-UTC  LEVEL  logger  message`) | `--limit` is ALWAYS explicit (default 200 — the server default is unlimited); `--since` takes EPOCH-MS or `500ms/30s/5min/2h`; sorts `desc(timestamp)` so you see the NEWEST entries, never the oldest 200 |
| `ign logs -f [--interval S] [--timeout S]` | Live tail: entries stream to stdout as they occur | poll-based (no server push exists — `GET /logs?startTime=<cursor>` IS the tail); `--timeout` expiry ends cleanly (exit 0); without it, run until Ctrl-C (default process kill, no envelope); see the streaming exception below |
| `ign logs download [-o FILE]` | Download the log archive — a SQLite `.idb`, never a zip | bytes written exactly as received; default filename from `Content-Disposition`, else `<profile>-logs-<ts>.idb`; `--json` data is `{file, bytes, content_type}` |
| `ign logs loggers [--search S]` | Logger registry: `name  level  context` | explicit limit 200 (same unlimited-default guard) |
| `ign logs loggers set <NAME> <LEVEL>` | Set one logger's level (TRACE..OFF) | **mutation**: exit 2 (`confirmation_required`) without `--yes`; audit-logged server-side |
| `ign logs loggers reset` | Reset ALL custom logger levels to defaults | **mutation**: exit 2 without `--yes`; audit-logged server-side |
| `ign restart [--wait] [--timeout S] [--interval S]` | Restart the gateway — the one big red button | **always `--yes`-guarded** (it takes the whole gateway down); without `--wait`: POST + "READY in ~1 min" advisory; with `--wait`: POST → 5 s floor → polls the unauthenticated `/StatusPing` until RUNNING (default 300 s budget; a timeout exits 4 naming the last observed state) |
| `ign wait gateway [--interval S --timeout S]` | Wait until the gateway reports RUNNING | unauthenticated `/StatusPing` poll — works with no/broken credential; already-RUNNING = immediate success (default 120 s) |
| `ign wait restart [--interval S --timeout S]` | Wait for a restart to complete | shares `restart --wait`'s semantics: a non-RUNNING state observed once → RUNNING completes immediately (witnessed restart, no floor wait); an all-RUNNING wait reports success only after the same 5 s floor — no false positive when run right after `ign restart` |
| `ign wait module <ID> [--interval S --timeout S]` | Wait until a module reports ACTIVE | polls `modules/healthy?search=<id>` (authed); timeout names the id + last observed state |
| `ign doctor [--check-write] [--webdev-route NAME]` | Diagnose the setup: url (parse + TCP dial), liveness (unauth `/StatusPing`), commissioning (302→`/welcome`), auth (401 vs 403), the permissions deep-dive (`security-properties`), write permission, WebDev-route presence, Docker/rig presence | **exits 0 whenever the diagnosis completes** — failing checks are data, not CLI errors (agents parse `checks[]`; humans read the table); `--check-write` fires the harmless `scan/projects` rescan (2xx = write OK, 403 = read-only token); `--webdev-route NAME` probes that route's version action in the CLI's `ign-cli` WebDev project — **405 = absent** (the live-proven 8.3 marker; the earlier 404 assumption was wrong), 402 = module unlicensed, 200 = present (+ handshake version); config errors (no profile) still exit 3 |
| `ign webdev deploy [--project NAME] [--with-script-exec] [--rotate-secret]` | Install the CLI's own WebDev route bundle into the dedicated `ign-cli` project (default) — `tags`, `tagConfig`, `alarms`, `tagHistory` (+ `scriptExec` only with `--with-script-exec`) | **not `--yes`-guarded by design**: the dedicated project is CLI-OWNED — born from the first deploy zip, overwrite-REPLACED on every deploy (replace-not-merge is the contract here; user projects are never touched); every WebDev-dependent tag command (Phase 5) refuses exit 6 `routes_not_deployed` naming `ign webdev deploy` until this runs; `--with-script-exec` generates a fresh hex secret (stored in the profile config at 0600) when none exists, `--rotate-secret` regenerates unconditionally (requires `--with-script-exec`); the secret NEVER appears in any output, envelope, or log (it lives in exactly one place: the baked route zip member); JSON data `{project, routes, script_exec, secret_rotated, import}` |
| `ign webdev status [--project NAME]` | The version-handshake sweep: probe every route's version action — per-route `{route, status, deployed_version, expected_version}` | **a read — exits 0 whenever the sweep completes** (per-route degradation is DATA, the doctor precedent): `present`/`absent`/`unlicensed`/`auth_gated`/`secret_mismatch`/`version_mismatch` per route; the `ok` flag (data, not exit code) is true only when every always-on route is present with a matching version; scriptExec is probed ONLY when a secret is configured for the profile and never gates `ok` |
| `ign rig [--rig NAME] up [--timeout S]` | Bring a Docker compose rig up (`compose up -d --wait`) and wait for the gateway | docker-only (`profile: null` envelope); `--timeout` is BOTH compose's `--wait-timeout` and the commissioned-probe deadline (default 300 s); a fresh-volume rig reports `"up, uncommissioned"` as DATA (exit 0, wizard URL in `warnings`) |
| `ign rig [--rig NAME] down` | Stop the rig (`compose down --remove-orphans`; volumes KEPT) | docker-only; the volume-deleting teardown belongs to `rig reset` |
| `ign rig [--rig NAME] reset [--timeout S]` | Tear the rig down AND remove its volumes, then bring it back up fresh (`down -v --remove-orphans` → pre-flight → `up --wait` → commissioned wait) | **destructive**: exit 2 (`confirmation_required`) without `--yes` or `IGNITION_YES=1`, BEFORE any discovery runs; `removed_volumes` in the data reports exactly what `-v` took; no stale project/trial state survives (a fresh volume usually boots uncommissioned — exit 0, wizard URL in `warnings`) |
| `ign rig [--rig NAME] status` | Structured rig status: services (state/health/ports), volumes, ports occupancy | docker-only; an ALLOWLIST only — never a compose-config passthrough (the resolved config contains gateway passwords); a down rig is exit-0 data |
| `ign rig [--rig NAME] logs [--tail N] [-f] [SERVICE]` | Stream the rig's container logs (`compose logs` passthrough) | raw lines in EVERY mode — the third stdout exception (see §Streaming); `--tail` default 200; `-f` follows until Ctrl-C (default process kill); compose diagnostics go to stderr, never the data stream |
| `ign rig [--rig NAME] trial status` | Show the rig gateway's trial state: licenseMode, trialState, seconds left, expired — plus the banners cross-check | **credential-free** (the trial/banners endpoints answer unauthenticated — verified live on 8.3.3 AND 8.3.6; a fresh rig with no token reports fine); addresses the RIG's derived gateway URL (never the profile's); data `{license_mode, trial_state, trial_remaining_s, expired, emergency, emergency_remaining_s, development, banners: {severity, expire_time_ms, active}, warnings}` — `banners.active` is the Pitfall-7 cross-check (`severity=="info"` AND `expireTime>now_ms`), never the primary truth (`expired` is) |
| `ign rig [--rig NAME] trial reset [--user NAME]` | Reset an EXPIRED trial to a fresh ~2 h window via the mechanism ladder | **destructive**: exit 2 (`confirmation_required`) without `--yes`, BEFORE any discovery; ladder = tier 0 `POST /data/api/v1/trial` with `X-Ignition-API-Token` (token from `IGNITION_TOKEN`) → tier 1 native gateway login (internal-IdP OIDC challenge dance → session cookie + CSRF header), creds `--user`/`IGNITION_USER` + `IGNITION_PASSWORD` (password NEVER a flag); success REQUIRES the read-back flip (`expired` false on re-fetch — a bare 2xx never suffices); a NON-expired trial refuses exit 6 `trial_not_expired` (the gateway 403s resets while active — live-verified); no creds at all → exit 3 |
| `ign rig [--rig NAME] snapshot [-o DIR]` | Snapshot the rig's gateway: native gwbk (`GET /backup?type=roaming`, STREAMED to disk) + per-project exports + `manifest.json` — composed in a timestamped dir | addresses the rig's derived gateway URL; requires `IGNITION_TOKEN` (the backup route 401s unauthenticated — live-verified shape); default dir `./ign-rig-snapshots/<rig>-<yyyyMMdd-HHmmss>/`; data `{dir, gwbk_bytes, projects, manifest_path}` — the manifest names BOTH composition exclusions (see §rig snapshot/restore) |
| `ign rig [--rig NAME] restore --file PATH [--timeout S]` | Restore a gwbk onto the rig's gateway (raw octet-stream POST), wait for the witnessed post-restore RUNNING | **destructive**: exit 2 without `--yes`, BEFORE any discovery; the restore is synchronous and the gateway RESTARTS after — success is a WITNESSED StatusPing→RUNNING (deadline floored at 300 s), never a bare 2xx; data always carries the token-clobber warning (`API tokens may have been reset by restore…`, see §rig snapshot/restore); requires `IGNITION_TOKEN` |
| `ign profile add/list/use` | Manage gateway profiles | — |
| `ign completions <SHELL>` | Shell completion scripts | raw stdout regardless of `--json` |

All gateway commands honor the envelope (`--json`/`--compact`) with the
`[profile: NAME]` header in human mode. The inspection trio (`status`,
`modules`, `metrics`) replaces the gateway webpage's Status Overview,
Config > Modules, and Performance & Diagnostics pages; `sessions`,
`connections`, the `logs` tree, and the `project` tree replace its
Sessions, Connections, Logs console, logger-config, and Projects pages.

## Rigs (Docker compose lifecycle)

`ign rig` manages a **compose rig** — a Docker compose project running
an Ignition gateway plus its satellites. It shells out to
`docker compose` (the v2 plugin line; **compose ≥ v2 required** — the
legacy `docker-compose` v1 binary is not supported and a missing/old
compose fails fast with exit 7 and an install hint). Every operation
starts with a one-shot resolve run (`docker compose -f <file>
--project-directory <dir> config --format json`); the resolved project
`.name` — which honors the rig's own `.env` `COMPOSE_PROJECT_NAME` —
is the identity truth, and every later op passes it as an explicit
`-p <name>` (no implicit directory-name projects, ever).

### Rig discovery (in this order)

| Level | Source | Notes |
|-------|--------|-------|
| 1 | `--rig NAME` | looks up `[rigs.NAME]` in the config; unknown name → exit 7 with the known rigs listed |
| 2 | `IGNITION_RIG` env | same lookup (folded into the flag — one env→flag home) |
| 3 | `[rig].default` | the config's explicit preference — BEATS the cwd scan; a stale default is a loud exit 7 |
| 4 | cwd compose files | `./docker/compose.yml`, `./docker/docker-compose.yml`, `./compose.yml`, `./compose.yaml`, `./docker-compose.yml` |
| 5 | WHK conventions | `ignition-git-module/docker/docker-compose.yml`, then `whk-environment-orchestration/docker-compose.yml` — each probed under BOTH `~/Documents/whiskeyhouse/` and `~/whiskeyhouse/` (first hit wins) |

Config surface:

```toml
[rig]
default = "git-module"

[rigs.git-module]
compose_file = "~/Documents/whiskeyhouse/ignition-git-module/docker/docker-compose.yml"
# project_name optional — omit to honor the rig's own .env
```

`compose_file` expands `~` and `${VAR}`. `IGNITION_RIG_ROOTS`
(path-separated) overrides the convention home roots — for machines
whose WHK checkouts live elsewhere (and for test isolation). Nothing
found → exit 7 with the full search trail in the message.

### The docker-only contract: `profile: null`

Rig verbs are the first commands with **no gateway dependency**: no
profile, secret, or client is resolved, and the envelope echoes
`profile: null` on both success and error. `--profile` has no effect
on `rig`.

### `rig up` semantics

1. compose version gate (fail fast, exit 7 + install hint);
2. port pre-flight — a host port held by ANOTHER project's container
   aborts before anything starts: `port 9088 in use by container X
   (rig Y)` (same-project occupants are recreate-safe; a non-docker
   host process is attributed via `lsof` when available);
3. `up -d --wait --wait-timeout <N> --remove-orphans`;
4. commissioned wait: the rig's own gateway port is derived from the
   resolved mappings (first target 8088 → `http://localhost:<pub>`,
   else target 443 → https) and its `/StatusPing` is polled
   header-less until RUNNING — same budget as `--timeout`.

**Uncommissioned is data, not failure.** A fresh-volume gateway
terminally 302s to `/welcome` — there is no headless commissioning
(verified: no commissioning endpoints exist). `rig up` then exits 0
with `state: "uncommissioned"` and the wizard URL inside `warnings`
(the version-command degradation precedent). A deadline reached while
still STARTING is a real exit-7 failure.

```json
{"ok": true, "profile": null, "data": {"rig": "ignition-devops", "project": "ignition-devops",
 "state": "uncommissioned", "gateway_url": "http://localhost:9088",
 "warnings": ["gateway uncommissioned — open http://localhost:9088/welcome in a browser and complete the commissioning wizard (no headless commissioning exists)"]}}
```

### `rig status` is an allowlist

Status NEVER passes through `docker compose config`/`inspect` output —
the resolved config contains `GATEWAY_ADMIN_PASSWORD` and friends.
The JSON data is exactly `{rig, project, compose_file, services[]:
{name, state, health, exit_code, publishers[]: {published_port,
target_port, protocol}}, volumes: [names], ports_free}` — all keys
always present (empty arrays when none). A down rig is exit-0 data
(`ports_free: true`, empty services).

### `rig reset` semantics (the no-stale-state contract)

`reset` is the phase's destructive verb: it refuses without `--yes`
(exit 2, `confirmation_required`, hint names `--yes` and
`IGNITION_YES=1`) and the guard fires BEFORE any discovery runs — a
refusal costs nothing (no docker, no config scan; binary-pinned by
exiting 2 in a directory with no rig discoverable at all).

The cycle, in order:

1. **preview** — `docker volume ls` label-filtered to the project:
   the `removed_volumes` array in the result data reports exactly
   what reset removes, as it acts;
2. compose version gate (exit 7 + install hint when absent);
3. `down -v --remove-orphans` — the LOCKED teardown: `-v` removes the
   project's named AND anonymous volumes (gateway data, trial state,
   everything), `--remove-orphans` kills renamed-service strays —
   `down && up` without `-v` is the classic stale-state anti-pattern
   reset exists to kill;
4. port pre-flight with FRESH EYES — teardown frees the rig's own
   ports first; if another rig grabbed one mid-cycle, reset aborts
   with attribution and names the torn-down state in the hint;
5. `up -d --wait` (the `rig up` invocation verbatim);
6. commissioned wait — same semantics as `rig up`: a fresh volume
   usually terminally reports the wizard, which is DATA (exit 0,
   `state: "uncommissioned"`, wizard URL in `warnings`); still-STARTING
   at the deadline is a real exit-7 failure.

The result data is `{rig, project, removed_volumes, state, warnings}` —
all keys always. No project, trial, or gateway state survives the
volume deletion; commission from scratch or restore a backup (04-04)
afterward.

### `rig logs` is passthrough

`rig logs` streams `docker compose logs` output RAW: one line per
stdout line, no envelope in ANY mode — compose log lines are not
gateway JSON objects, and wrapping would corrupt them (`rig logs
--json` is the same passthrough; contrast `logs -f --json`, whose
entries ARE gateway NDJSON). `--tail N` (default 200) bounds the
history; `-f` follows until Ctrl-C (default process kill, no
envelope); an optional SERVICE positional filters to one service.
Compose's own stderr diagnostics go to `ign`'s stderr, never the
data stream.

### `rig trial` — state, and the reset ladder

`rig trial status` is **credential-free truth**: both
`GET /data/api/v1/trial` and `GET /data/api/v1/overview/banners`
answer unauthenticated (live-verified on 8.3.3 and 8.3.6, expired AND
active states), so a fresh rig with no provisioned token reports its
trial state fine. The trial endpoint is the PRIMARY source
(`trialState ∈ {AllInDemo, SomeInDemo, NoneInDemo}`,
`trialSecondsLeft` in seconds, the `expired` flag); the trial banner
is the cross-check — `expireTime` is epoch **milliseconds** or
`null`, and an EXPIRED trial shows `severity:"warning"` + `null`
(code expecting a future timestamp misreads expired as active), so
`banners.active` is computed as `severity=="info"` AND
`expireTime>now_ms` and never the reverse derivation.

`rig trial reset` runs the **mechanism ladder** (the spike-resolved
native approach — browser delegation rejected: it needs
Node+chromium, broke across 8.3.3's UI rewrite, and verifies via DOM
text):

1. **tier 0 — token-auth POST** `POST /data/api/v1/trial` with
   `X-Ignition-API-Token` (token from `IGNITION_TOKEN`; token
   mutations need no CSRF). One cheap call; on 2xx it wins.
2. **tier 1 — native gateway login** (the live-verified mechanism,
   end-to-end on 8.3.3: `expired:true → false`, `0 → 7199s`): the
   internal IdP's OIDC challenge dance (rotating tokens, ~4 captured
   cookies replayed by hand, `webui-sid-<id>` session cookie,
   `csrfToken` from `/data/app/session`) → the reset POST with the
   session cookie + `X-CSRF-Token`. Credentials: `--user` (or
   `IGNITION_USER`) + `IGNITION_PASSWORD` — the password NEVER rides
   a flag, env only.

Success REQUIRES the read-back flip: after a 2xx the trial is
re-fetched and `expired` must be false — a bare 2xx never suffices
(the mutation-reads-back discipline). The result data is
`{rig_url, mechanism: "token"|"login", expired_before,
expired_after, trial_remaining_s}`.

**State gate (live-discovered):** the gateway answers 403 to reset
attempts on a NON-expired trial (verified from the browser page with
the exact UI headers) — `ign` surfaces this honestly as exit 6
`trial_not_expired` naming the seconds left, instead of a misleading
auth error. Wait for expiry (watch `rig trial status`) or `rig reset
--yes` for a completely fresh trial volume.

**Verification note:** the reset path is verified end-to-end against
8.3.3 (the git-module rig, by hand during 04-03) and the
login-flow machinery against 8.3.6 (ign-research; steps 1–5 +
bad-credential shapes). Repeatable live gates ship as `#[ignore]`
tests (`cargo test -p ignition-core --test trial_contract --
--ignored` with `IGNITION_LIVE_URL` + credentials +
`IGNITION_LIVE_MUTATIONS=1` against an expired rig). A tier-2
Playwright fallback exists ONLY as this documented env contract —
`ignition-trial-resetter` / WHK-Global's `e2e/reset_trial.mjs` — and
is never shipped as `ign`'s mechanism.

### `rig snapshot` / `rig restore` — repeatable state

`rig snapshot` composes a rig's gateway state HONESTLY into one
directory (default `./ign-rig-snapshots/<rig>-<yyyyMMdd-HHmmss>/`,
`-o` overrides; the stamp is std-only, no clock dependency):

- **`<rig>.gwbk`** — the native roaming backup (`GET
  /data/api/v1/backup?type=roaming`, `Accept:
  application/octet-stream`), STREAMED to disk chunk-by-chunk (never
  buffered in memory — gwbks are tens of MB);
- **`projects/<name>.zip`** — one export per runnable project (the
  `project export` machinery reused; file names percent-encode the
  project name injectively, so `My Project` → `My%20Project.zip`).
  The gwbk *should* already contain projects (postman semantics,
  MEDIUM confidence), but the explicit exports are the honest
  redundancy that makes the manifest truthful either way;
- **`manifest.json`** — the composition record: `{rig, taken_at
  (epoch s), ignition: {version}, gwbk, projects: [{name, file}],
  notes}`. The notes name BOTH exclusions explicitly, verbatim:
  **the trial clock is NOT captured by gwbk** (its restore behavior
  is unknown — reset separately via `rig trial reset`), and
  **tag-provider bulk export is Phase 5 scope** (gwbk captures tag
  *config* via gateway data). No reader can mistake either for a
  silent drop.

`rig restore --file <path.gwbk>` is the guarded inverse
(`--yes`-refusal exit 2 before any discovery, the fifth destructive
verb). The POST is a RAW `application/octet-stream` body — NOT
multipart — with the four scope params (`restoreDisabled`,
`disableTempProjectBackup`, `renameEnabled`, `restoreLocal`) sent
explicitly as `false`. Restore is synchronous AND the gateway
restarts afterward, so BOTH wire directions ride a 300 s per-request
budget and success is a **witnessed** post-restore
StatusPing→RUNNING (the `--timeout` budget floors at 300 s — a short
explicit timeout cannot buy an unknown-state mid-restart report).
A bare 2xx never suffices.

⚠ **API tokens may be clobbered (Pitfall 5).** Tokens stored under
CORE config (`data/config/CORE/ignition/api-token`) are
"modified/cleared often by gwbk restores" (83-api) — after a
restore, stored profiles may 401. The restore data ALWAYS carries
`"API tokens may have been reset by restore — re-provision via
gateway UI, then ign doctor"` as its first warning, in every render
mode. 83-api's recommendation: keep durable tokens in an EXTERNAL
location, not CORE config.

Both verbs address the rig's derived gateway URL and source their
credential from `IGNITION_TOKEN` (the rig-family chain — the backup
route 401s unauthenticated, unlike the trial endpoints; a missing
token is exit 3). The round-trip is pinned live by the opt-in e2e
gate:

```bash
IGNITION_LIVE_URL=http://localhost:9088 \
IGNITION_LIVE_TOKEN='name:key' \
IGNITION_LIVE_MUTATIONS=1 \
cargo test -p ignition-cli --test e2e_rig -- --ignored
```

(the rig must be discoverable — run from the rig's checkout, set
`IGNITION_RIG`, or configure `[rig].default`; **verify with `ign rig
status` first that the intended rig is the one UP** — the gateway
verbs address the derived `localhost:<port>` URL, and a port
collision with another stack would silently point them at the WRONG
gateway). The gate snapshots a
pre-witness project, creates a post-snapshot marker, restores, and
asserts BOTH halves: witness SURVIVED, marker GONE.

### Project export/import specifics


**Timeouts.** Long transfers are the classic default-timeout death, so
both operations carry per-request budgets instead of the 30 s client
default: export 120 s, import 300 s. The import is synchronous (the
gateway answers when it finishes — no job IDs). If an import times out
or the connection drops mid-flight, the gateway state is unknown:
**verify with `ign project list`** (and re-run) rather than assuming.

**Scope metadata.** A project export contains views, scripts,
named-queries, and the other module project-resources — it does NOT
contain tag providers, tags, or UDTs: those are gateway configuration,
not project resources (this is why git-module conventions keep a
separate `tags/` tree). Both `project export` and `project import`
carry the same static `scope: {includes, excludes}` arrays in their
JSON data so agents and humans always know what a ZIP does and does
not contain.

**Collision policy.** REST exposes exactly two choices: `abort` (the
default — the CLI pre-checks with `find` and refuses with
`project_exists` before uploading anything) and `overwrite` (replaces
the ENTIRE project — resources absent from the ZIP are deleted).
`merge` is the Designer import popup's mode and is not available via
REST; the CLI rejects it at the flag level by simply not offering it.

### Resource editing — the surgical loop (export-zip surgery)

`ign resource` changes ONE view/script/query member — the
get → edit → put loop:

```bash
ign resource list PlantFloor                       # member paths from the export tree
ign resource get PlantFloor ignition/script-python/e2e/scratch > scratch.json
# ...edit scratch.json...
ign resource put PlantFloor ignition/script-python/e2e/scratch --file scratch.json --yes
ign resource delete PlantFloor com.example/views/OldView/view.json --yes
```

**Mechanism (05-02 re-point).** There are NO per-resource REST
endpoints on 8.3 gateways (the Phase-3 family originally targeted
`/projects/{p}/resources/**` routes that do not exist —
openapi-evidenced against real 8.3.x, 575 paths, zero matches). Every
resource op therefore rides the native export/import round-trip:
export the project to a temp ZIP → perform member surgery (list /
read / replace-inject / remove) → import back with `overwrite=true`.
Paths keep their slashes and address ZIP MEMBERS — the file at
`<collection>/resources/<rest>`, listed as `<collection>/<rest>`
(the core module's folder is `ignition/`, not the
`com.inductiveautomation.ignition` the old docs suggest).

`put` sniffs its input: valid JSON parses, other UTF-8 rides as text
— and BINARY content (a NUL byte near the head, `data.bin`-class
resources like Perspective session-permissions) refuses with exit 6
`resource_binary` on BOTH get and put: binary resources belong to
the export/import family, never the JSON loop.

**Perf honesty (accepted trade).** Every resource op round-trips the
WHOLE project ZIP — heavier than a hypothetical per-resource route,
but rigs and dev projects are small and the alternative was the
family not working at all. **Concurrent-edit race:** because put and
delete re-import the project, edits made in a Designer between the
CLI's export and import are REPLACED (replace-not-merge — resources
absent from the ZIP are deleted). This is the documented, accepted
trade for the CLI's dev-tool scope; both verbs are `--yes`-guarded
and their refusal messages name the consequence.

**e2e witness.** The loop is live-runnable for the first time since
Phase 3: `crates/ignition-cli/tests/e2e_projects.rs` (`-- --ignored`,
needs `IGNITION_LIVE_URL` + `IGNITION_LIVE_TOKEN` +
`IGNITION_LIVE_MUTATIONS=1`) drives list/get/put/delete through the
surgery implementation against a real gateway, including two-sided
put honesty (the export before a put carries the member's old
content; the export after carries the new).

### Streaming output (the stdout exceptions)

`ign logs -f` is a STREAM: entries print to stdout as they arrive, so
there is no single result to wrap. In human mode that means live lines
(`ISO-UTC  LEVEL  logger  message`, profile header first). Under
`--json`/`--compact` the tail emits **NDJSON — one compact entry object
per line, no envelope** — the second sanctioned stdout exception (after
`completions`). Every other command still emits exactly one envelope.
A `--timeout` expiry ends the tail cleanly with exit 0; without it the
tail runs until Ctrl-C, which uses the process default kill (no
envelope — plan for it in pipelines).

`ign rig logs` is the THIRD exception: its output is raw compose log
lines, NOT gateway JSON — so it streams verbatim in EVERY mode
(`--json` changes nothing; there is no envelope to add and no NDJSON
transformation to attempt). `-f` follows until Ctrl-C (default process
kill, no envelope), exactly the `logs -f` pipeline caveat.

### Destructive operations

Commands that change gateway state (`sessions terminate`,
`logs loggers set`/`reset`, `project delete`, `project import
--collision-policy overwrite`, `resource put`, `resource delete`
(both re-import the whole project — their refusal messages name the
consequence), `restart` — the
big one: it takes
the whole gateway down for ~1 min — and `rig reset`, which deletes
the rig's volumes, plus `rig trial reset`, which restarts the trial
window, and `rig restore`, which REPLACES the gateway's state with a
gwbk) refuse without `--yes`
(exit 2, `confirmation_required`, hint names both the flag and
`IGNITION_YES=1`) — non-interactive by design, so scripts and agents
pass `--yes` once and humans get a speed bump. `restart` is guarded in
BOTH forms: plain and `--wait`. The guard fires before any network
activity: a refusal never touches the gateway. The rig guards
(`rig reset`, `rig trial reset`, `rig restore`) fire before even rig
DISCOVERY (a refusal does zero work of any
kind). `project delete` and
`project import --collision-policy overwrite` are the doubly-relevant
pair: besides the CLI refusal, delete's wire request always carries
the gateway's own `confirm=true` query param, and overwrite REPLACES
the entire project (abort-policy imports need no `--yes` — they fail
safely server-side). `resource put` joined this family in 05-02: its
member surgery implicitly overwrite-imports the project (the 03-03
unguarded put is superseded), as does `resource delete`. Termination,
restart, project mutations, and resource
writes are
audit-logged server-side by the gateway. Non-destructive project
mutations (`copy`, `rename`, `set`, `export`) create, relabel, or read
rather than destroy, so they carry no `--yes`.

## The CLI's WebDev routes (`ign webdev`)

Every tag operation the CLI performs on a gateway rides the CLI's own
WebDev route bundle — small Jython `doPost.py` routes deployed into a
DEDICATED project (`ign-cli` by default) under
`/system/webdev/ign-cli/cli/{route}` (the wire protocol — NOT
`/data/webdev/*`, which does not exist). The bundle is embedded in the
binary at build time and carries a `ROUTE_BUNDLE_VERSION` handshake
(`{"action":"version"}` → `{routeVersion, minCli}`) on every route.

### Deploy semantics

`ign webdev deploy` packs the embedded bundle into a project zip and
imports it with `overwrite=true` — a **clean replace**, never a merge.
The dedicated project is CLI-OWNED: it is born from the first deploy
zip and wholesale-replaced by every later deploy, which is why deploy
carries **no `--yes` guard** (user projects are never touched; you
cannot accidentally deploy over anything you own — use `--project`
only if you deliberately want a different name). There is no
pre-flight project create (the gateway's first-import quirk); the
import is the deployment.

### The version-negotiation refusal matrix

WebDev-dependent commands (the tag family, from Phase 5 on) probe the
canonical `tags` route's handshake BEFORE doing anything, and refuse
exit 6 with an actionable error — no auto-upgrade magic, ever:

| Probe answer          | Slug                   | Hint                                                |
|-----------------------|------------------------|-----------------------------------------------------|
| 405 (route/project absent) | `routes_not_deployed` | run `ign webdev deploy`                             |
| deployed < expected   | `route_version_mismatch` | run `ign webdev deploy` (old routes on the gateway) |
| deployed > expected   | `route_version_mismatch` | update `ign` (the binary is older than the routes)  |
| 402                   | `webdev_unlicensed`   | license the gateway (trial-expired rigs cannot serve `/system/webdev`) |
| 200 body denial, unmapped code | `webdev_route_error` | the route's own `code` + `message` verbatim |

Two wire facts the matrix is built on (live-proven on 8.3): **denials
ride HTTP 200** — WebDev ignores a `status` key in route returns, so
every refusal is detectable only from the body envelope
`{ok, data|error}`; and **405 = absent** (missing routes and missing
projects both answer 405 — `ign doctor`'s earlier 404 assumption was
wrong and has been re-pinned).

`ign webdev status` itself is a READ: it exits 0 whenever the sweep
completes and reports per-route degradation as data (the doctor
precedent) — the refusal matrix above belongs to the tag commands
that DEPEND on the routes, not to the sweep that inspects them.

### scriptExec — the LOCKED security posture

`scriptExec` (arbitrary Jython execution through the gateway) ships
in the bundle only as a TEMPLATE with a `__IGN_CLI_SECRET__` marker
and deploys ONLY with an explicit `--with-script-exec`:

- the deploy generates a fresh 32-byte hex secret from
  `/dev/urandom`, substitutes it into the template (the placeholder
  can never ship — it is excluded from the plain manifest), and
  persists it in the profile config at 0600; `--rotate-secret`
  regenerates (any route copy deployed with the old secret starts
  refusing);
- the route fail-closes on every action — version included — unless
  the request carries the matching `X-Ignition-CLI-Secret` header:
  no header → `secret_required`, wrong secret → `secret_mismatch`
  (constant-time compare);
- the secret appears in exactly ONE place: the baked zip member on
  the gateway. Never in command output, JSON envelopes, or logs.

Threat-model honesty: the secret gate is a SHARED-SECRET posture, not
real authentication — anyone who can read the gateway project's
resources (e.g. a Designer user or a project export) can read the
secret. It exists to make `scriptExec` an opt-in, auditable surface
with a definite off switch (never deploy it, or redeploy without the
flag), not to protect against gateway insiders. The route keeps its
config at require-auth=false deliberately: an auth layer would lock
the CLI's own token-authenticated calls out (API tokens 401 on WebDev
require-auth — live-verified).
