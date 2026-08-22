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
| 2    | usage         | usage error (rendered by clap) or destructive op without `--yes` | `confirmation_required`
| 3    | config        | local configuration problem                        | `profile_not_found`, `no_active_profile`, `secret_unavailable`, `config_invalid`
| 4    | network       | gateway unreachable / timeout / TLS                | `network_error`
| 5    | auth          | gateway rejected credentials                       | `auth_rejected`
| 6    | target_state  | command invalid for the gateway's current state    | `gateway_too_old`, `gateway_not_commissioned`, `gateway_restarting`, `not_found`
| 7    | rig           | docker/compose rig failure (reserved, Phase 4)     | `rig_error`

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
| 403 | token recognized but under-permitted | three-part setup: (1) token holds an adequate security level, (2) gateway read/write permissions include that level, (3) "Require secure connections" unchecked for http — `ign doctor` (later in Phase 2) diagnoses all three |

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
| `ign logs [--logger L] [--min-level L] [--since SPAN] [--limit N]` | Recent log entries, newest first (`ISO-UTC  LEVEL  logger  message`) | `--limit` is ALWAYS explicit (default 200 — the server default is unlimited); `--since` takes EPOCH-MS or `500ms/30s/5min/2h`; sorts `desc(timestamp)` so you see the NEWEST entries, never the oldest 200 |
| `ign logs -f [--interval S] [--timeout S]` | Live tail: entries stream to stdout as they occur | poll-based (no server push exists — `GET /logs?startTime=<cursor>` IS the tail); `--timeout` expiry ends cleanly (exit 0); without it, run until Ctrl-C (default process kill, no envelope); see the streaming exception below |
| `ign logs download [-o FILE]` | Download the log archive — a SQLite `.idb`, never a zip | bytes written exactly as received; default filename from `Content-Disposition`, else `<profile>-logs-<ts>.idb`; `--json` data is `{file, bytes, content_type}` |
| `ign logs loggers [--search S]` | Logger registry: `name  level  context` | explicit limit 200 (same unlimited-default guard) |
| `ign logs loggers set <NAME> <LEVEL>` | Set one logger's level (TRACE..OFF) | **mutation**: exit 2 (`confirmation_required`) without `--yes`; audit-logged server-side |
| `ign logs loggers reset` | Reset ALL custom logger levels to defaults | **mutation**: exit 2 without `--yes`; audit-logged server-side |
| `ign profile add/list/use` | Manage gateway profiles | — |
| `ign completions <SHELL>` | Shell completion scripts | raw stdout regardless of `--json` |

All gateway commands honor the envelope (`--json`/`--compact`) with the
`[profile: NAME]` header in human mode. The inspection trio (`status`,
`modules`, `metrics`) replaces the gateway webpage's Status Overview,
Config > Modules, and Performance & Diagnostics pages; `sessions`,
`connections`, and the `logs` tree replace its Sessions, Connections,
Logs console, and logger-config pages.

### Streaming output (the second stdout exception)

`ign logs -f` is a STREAM: entries print to stdout as they arrive, so
there is no single result to wrap. In human mode that means live lines
(`ISO-UTC  LEVEL  logger  message`, profile header first). Under
`--json`/`--compact` the tail emits **NDJSON — one compact entry object
per line, no envelope** — the second sanctioned stdout exception (after
`completions`). Every other command still emits exactly one envelope.
A `--timeout` expiry ends the tail cleanly with exit 0; without it the
tail runs until Ctrl-C, which uses the process default kill (no
envelope — plan for it in pipelines).

### Destructive operations

Commands that change gateway state (`sessions terminate` and the
`logs loggers set`/`reset` mutations today; `project delete` and `rig
reset` in later phases) refuse without `--yes` (exit 2,
`confirmation_required`, hint names both the flag and `IGNITION_YES=1`)
— non-interactive by design, so scripts and agents pass `--yes` once
and humans get a speed bump. The guard fires before any network
activity: a refusal never touches the gateway. Termination mutations
are audit-logged server-side by the gateway.
