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
| `ign profile add/list/use` | Manage gateway profiles | — |
| `ign completions <SHELL>` | Shell completion scripts | raw stdout regardless of `--json` |

All gateway commands honor the envelope (`--json`/`--compact`) with the
`[profile: NAME]` header in human mode. The inspection trio (`status`,
`modules`, `metrics`) replaces the gateway webpage's Status Overview,
Config > Modules, and Performance & Diagnostics pages.
