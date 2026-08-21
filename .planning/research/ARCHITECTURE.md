# Architecture: ignition-cli

**Domain:** Rust CLI + ratatui TUI wrapping Ignition Gateway REST/WebDev APIs with docker test-rig control
**Researched:** 2026-08-20
**Overall confidence:** HIGH (patterns verified against ratatui official templates, bollard docs, and local ecosystem source)

---

## Component Diagram

```
                        ┌─────────────────────────────────────────────┐
                        │            ign  (single binary)             │
                        │                                             │
  user / agent ──────▶  │  ┌───────────┐      ┌────────────────────┐ │
  (JSON output)         │  │ clap args  │─────▶│  command handlers  │ │
                        │  └───────────┘      └─────────┬──────────┘ │
                        │                               │            │
                        │  ┌───────────┐      ┌─────────▼──────────┐ │
  user ──────────────▶  │  │ ratatui   │─────▶│     actions         │ │
  (interactive)         │  │ TUI loop  │      │  (shared services)  │ │
                        │  └───────────┘      └─────────┬──────────┘ │
                        │                               │            │
                        │  ┌───────────┐      ┌─────────▼──────────┐ │
                        │  │ output    │◀─────│  domain models      │ │
                        │  │ json/table│      │  (serde structs)    │ │
                        │  └───────────┘      └─────────┬──────────┘ │
                        └───────────────────────────────┼────────────┘
                                                        │
                     ┌──────────────────────────────────┼───────────────┐
                     │            ignition-core (lib)    │               │
                     │  ┌────────────────┐  ┌───────────▼────────────┐  │
                     │  │ rig (docker    │  │ GatewayClient          │  │
                     │  │ compose shell) │  │ (reqwest, auth, retry) │  │
                     │  └───────┬────────┘  └───────┬────────┬───────┘  │
                     └──────────┼───────────────────┼────────┼──────────┘
                                │                   │        │
                     ┌──────────▼─────────┐ ┌───────▼──┐ ┌───▼─────────────────┐
                     │  docker compose /  │ │ REST API │ │ WebDev routes       │
                     │  docker CLI        │ │ /data/   │ │ /system/webdev/     │
                     └──────────┬───────── │ api/v1/*  │ │ IgnitionCLI/*       │
                                │         └───────┬──┘ └───┬─────────────────┘
                                │                 │        │
                     ┌──────────▼─────────────────▼────────▼─────────────────┐
                     │        Ignition 8.3 Gateway (dockerized test rig)     │
                     │   ┌────────────────────────────────────────────────┐  │
                     │   │ webdev/ project (this repo): tags, tagConfig,  │  │
                     │   │ alarms, scriptExec, tagHistory                 │  │
                     │   └────────────────────────────────────────────────┘  │
                     └────────────────────────────────────────────────────────┘

  Ecosystem delegation (shell-out, JSON contracts):
    ign rig reset-trial ──▶ node reset-trial.mjs (ignition-trial-resetter)
    ign lint ────────────▶ ignition-lint --report-format json
    rig discovery ───────▶ ~/whiskeyhouse/ignition-git-module/docker, WHK-Global
```

---

## Component Breakdown

### Workspace Layout (3-crate Cargo workspace → one binary)

```
ignition-cli/
├── Cargo.toml                 # [workspace]
├── crates/
│   ├── ignition-core/         # lib: client, actions, models, rig, config
│   ├── ignition-tui/          # lib: ratatui cockpit (depends on core)
│   └── ignition-cli/          # bin "ign" (depends on core + tui)
└── webdev/                    # Ignition-side WebDev project (deployed to gateway)
    └── com.inductiveautomation.webdev/
        └── resources/IgnitionCLI/
            ├── tags/           doPost.py, config.json, resource.json
            ├── tagConfig/      ...
            ├── alarms/
            ├── scriptExec/
            └── tagHistory/
```

**Why a workspace, not a single crate:**
- `ignition-core` compiles without ratatui/clap — fast test iteration on client logic; the API surface is reusable later (e.g., if an MCP transport decision reverses).
- `ignition-tui` as a lib lets the `ign` binary own arg parsing and call `ignition_tui::run(app_state)` — TUI is opt-in (`ign tui` or `--tui`), matching the non-interactive-by-default constraint.
- Still **one binary** (`ign`); workspace members are internal structure, not distribution units.
- Alternative rejected: single crate with features (`tui` feature). Feature-combination rot (debug builds without the feature can't see TUI code) and slower incremental builds. Alternative rejected: separate TUI binary — violates "one binary" constraint.

**Confidence: HIGH** — standard Rust workspace practice for CLI+TUI tools; consistent with how rust-anime/gping-style single-binary TUI tools are structured.

### 1. `ignition-core::client` — GatewayClient

**Responsibility:** All HTTP. Owns auth, base URL, retries, error mapping. No business logic.

- `reqwest` client with rustls (avoid openssl linkage for portable macOS/Linux builds).
- **Auth strategy (mirrors ignition-mcp, verified in its `config.py`/`ignition_client.py`):** prefer gateway **API token** header (8.3 natively supports API tokens — confirmed in 83-api Bruno collection: `config-api-token` endpoints), fall back to HTTP basic auth with gateway user/password. Auth roles required for WebDev script routes.
- Two endpoint families:
  - Native REST: `GET/POST /data/api/v1/*` (health, metrics, projects, resources, logs, modules, backups, designers, alarm journal, tag providers...). Endpoint catalog = the 42-tool `ignition_tools_summary.json` from ignition-mcp + 83-api Bruno collection (both local, verified).
  - WebDev: `/system/webdev/{project}/{route}` (verified URL shape in `ignition_client.py::_webdev_url`). Routes target the `webdev/` project shipped in this repo: `Global/IgnitionCLI/tags`, `/tagConfig`, `/alarms`, `/scriptExec`, `/tagHistory` (naming follows WHK-Global's `GatewayAPI` precedent but under our own project so it's independently deployable).
- **Typed trait for testability:** `trait GatewayApi` with a `ReqwestGateway` impl; actions depend on the trait so unit tests inject a mock. Keep the trait coarse (per capability, not per endpoint) to avoid churn.

### 2. `ignition-core::actions` — shared services layer

**Responsibility:** The verb layer. Every user-visible operation is an action: `health`, `list_projects`, `export_project`, `read_tags`, `write_tag`, `browse_tags`, `list_alarms`, `exec_script`, `rig_up`, `rig_down`, `rig_status`, `webdev_deploy`, ...

- Actions take typed params + a `GatewayApi` handle, return typed **domain models** (`#[derive(Serialize)]` structs).
- **Invariant: CLI handlers and TUI both call actions; neither talks to the client directly.** This is what makes "TUI exposes every CLI capability" cheap — a new action is instantly available to both.
- Actions are async fns; the CLI runs them on a single-shot tokio runtime, the TUI on a long-lived one.

### 3. Output formatting — presentation layer

**Responsibility:** Render domain models for humans or machines.

- Global flag `--json` (and `IGNITION_JSON=1` env for agents): actions' models serialize directly — zero-copy from action to stdout.
- Default human rendering: table/pretty via `comfy-table` (or ratatui `Table` reuse in TUI). Presentation lives in the `ign` binary + a small `render` module — **never inside actions**.
- Exit codes are part of the agent contract: 0 ok, 1 action failure, 2 usage (clap default). Errors as structured JSON on stderr when `--json`.

### 4. `ignition-core::rig` — docker test-rig lifecycle

**Responsibility:** Wrap docker compose; discover and reuse existing rig conventions.

- **Shell out to `docker compose` / `docker` CLI — do NOT use bollard** for lifecycle. Rationale (verified against local conventions):
  - The ecosystem's rigs are compose-file based with `.env`-driven `COMPOSE_PROJECT_NAME` / `COMPOSE_FILE` (verified: `ignition-git-module/docker/.env` sets `COMPOSE_PROJECT_NAME=ignition-devops`, `COMPOSE_FILE=docker-compose.yml`). Compose env interpolation, secrets (`gw-secrets/` files), and build args (`gw-build/`) are all honored by the CLI toolchain for free; reimplementing them over bollard's raw API is a large, wrong-shaped surface.
  - bollard (verified: list/start/stop containers) is the right tool only for raw daemon queries — but `docker compose ps --format json` already gives structured status. Lean dependency tree (project constraint) wins.
- **Rig discovery order** (all names checked in this precedence; matches "interop with existing conventions" constraint):
  1. `--rig <name>` / `-C <dir>` explicit flag
  2. `IGNITION_RIG` env / `[rig]` table in config.toml
  3. `./docker/compose(.yml)` or `./compose.yml` in cwd (the repo's own fixtures + future consumer projects)
  4. `~/whiskeyhouse/ignition-git-module/docker/` (verified path with two compose files + test-rig/)
  5. `~/data/projects/WHK-Global/` (verified: has e2e + trial reset scripts; compose under its own docker dir if present)
- Rig commands map to: `up` (compose up -d --wait), `down`, `reset` (down -v + up), `status` (compose ps --format json), `logs` (compose logs / gateway `/data/api/v1/logs`), `snapshot`/`restore` (gateway backup REST `POST/GET /data/api/v1/backup` — verified endpoints in tool summary).
- **Trial reset is a delegation boundary, not native code** (verified from ignition-trial-resetter README: *"Ignition's 2-hour trial timer can only be reset through the gateway web UI — there's no REST endpoint for it"*). `ign rig reset-trial` shells out to `node reset-trial.mjs` from either ignition-trial-resetter or a vendored copy under `webdev/../scripts/` with env wiring (`GATEWAY_URL`, `GATEWAY_USER`, `GATEWAY_PASS` — verified env names). Do not embed a browser engine in Rust.

### 5. `ignition-tui` — ratatui cockpit

**Pattern: official event-driven-async template** (verified against github.com/ratatui/templates `event-driven-async`: AppEvent enum + mpsc channel + tokio + crossterm `EventStream`; component template adds `action.rs`/`components/`/`tui.rs` split).

- One `AppEvent` enum carries **everything**: `Tick`, `Key`, `Mouse`, `Resize`, and crucially `ActionCompleted(Result<Model>)` / `ActionFailed(Error)` variants — async action results flow back through the same channel as keystrokes.
- Event loop: spawn crossterm `EventStream` reader task + optional tick task → both feed an `mpsc::UnboundedSender<AppEvent>`; main loop `select!`s on the receiver, updates `AppState`, redraws.
- Long-running actions (tag browse, project export, rig up): the loop spawns a tokio task calling the **same actions layer**; the UI stays responsive; results mutate state on arrival.
- State model: `AppState { profile, gateway_status, tags_tree, projects, logs_ring, rig_status, mode/focus }` — plain data, no UI types. Components (health panel, tag tree, alarm table, command palette) are render functions over `AppState`.
- TUI honors the same profile selection as CLI (`ign --profile prod tui`).

### 6. `webdev/` — gateway-side routes shipped in this repo

- Lives at repo root as an importable Ignition WebDev project fragment (`com.inductiveautomation.webdev/resources/IgnitionCLI/<route>/doPost.py + config.json + resource.json`) — exact layout verified against WHK-Global's deployed `GatewayAPI` routes.
- **Versioning:** each route response includes `"cliVersion": "<crate version>"`; the client compares against its own version and warns on mismatch (cheap handshake, avoids silent breakage when CLI upgrades ahead of gateway).
- **Deployment:** `ign webdev deploy [--profile]` imports via the REST project-import API (8.3 resource import endpoints; mechanism choice — zip vs per-resource import — is an open question for a spike, see Open Questions). Idempotent push of the route files.
- Route surface (parity with ignition-mcp's five WebDev endpoints, verified in its `tags.py`/`config.py`): `tags` (browse/read/write), `tagConfig` (CRUD/UDT defs), `alarms` (query/journal), `scriptExec` (run python), `tagHistory` (historian queries).

### 7. Config & profiles — `ignition-core::config`

- **Location:** `~/.config/ignition-cli/config.toml` (via `directories` ProjectDirs; respects XDG on Linux, `~/Library/Application Support` on macOS — but standardize on the XDG-style path override `IGNITION_CLI_CONFIG` for scripted environments).
- **Shape:** profiles table, matching the "multiple gateways" requirement:

```toml
active = "dev"

[profiles.dev]
url = "http://localhost:9088"        # rig port convention: 9088→8088 (verified)
username = "admin"
api_token_env = "IGNITION_TOKEN"     # secrets referenced, never stored
# password_env = "GATEWAY_ADMIN_PASSWORD"   # basic-auth fallback

[profiles.whk]
url = "https://whk-gateway:8043"
api_token_env = "WHK_IGNITION_TOKEN"

[rig]
default = "git-module"               # key into [rigs.*]
[rigs.git-module]
dir = "~/whiskeyhouse/ignition-git-module/docker"
[rigs.local]
dir = "./docker"
```

- **Env precedence (canonical → legacy interop):** `IGNITION_CLI_URL` / `IGNITION_CLI_TOKEN` > profile values > `IGNITION_URL` / `GATEWAY_ADMIN_USERNAME` / `GATEWAY_ADMIN_PASSWORD` (verified names used by `reset_trial.mjs` and git-module compose — accepting them gives zero-config pairing with existing rigs). No plaintext secrets in TOML; only env-var *names* or `password_cmd` (shell snippet, keychain-style).

---

## Data Flow

**Read path (CLI, agentic):**
```
argv → clap → handler → action(model params) → GatewayApi::get_* →
gateway HTTP → typed Model → --json ? serde_json::to_stdout : render::table
```

**Write path (same, with confirmation policy):** mutating actions accept `--yes` for non-interactive use; default (no TTY) requires `--yes`, in TTY prompts. Every mutation's result is a Model → JSON-serializable.

**TUI path:**
```
key press ─┐
tick ──────┼─▶ mpsc<AppEvent> ─▶ app.update(state, event) ─▶ spawn action task ─┐
           │                                                        result ──┘
           └──────────── redraw(state) ◀── AppEvent::ActionCompleted(state)
```

**Rig path:** action → `docker compose -f <file> --project-name <name> up -d --wait` (child process, streamed output, JSON status via `--format json`) → parsed into `RigStatus` model → same render layer.

**WebDev deploy path:** `webdev/` files on disk → `ign webdev deploy` → project import REST call → gateway now serves `/system/webdev/Global/IgnitionCLI/*` → subsequent tag/alarm/script actions hit those routes → responses carry `cliVersion` for handshake.

Direction is always **UI/CLI → actions → client/rig → gateway**; results flow back as typed models. No component reaches around the actions layer.

---

## Build Order

Dependency-driven; each phase produces a usable tool:

1. **Workspace scaffold + config + output layer** — crates, `ign --json --profile` plumbing, config.toml loader, table/json renderers. *(Everything depends on this.)*
2. **GatewayClient + read-only REST actions** (`health`, `info`, `modules`, `logs`, `metrics`) — immediately useful against any 8.3 gateway, no gateway-side setup needed. Validates auth, error mapping, JSON contract.
3. **Rig lifecycle** — compose wrapper + discovery + status. **Must precede WebDev work**: the rig is the test fixture for everything downstream, and it dogfoods `rig up`/`reset` in CI.
4. **WebDev routes + deploy + tag/alarm/script actions** — author `webdev/` project, `webdev deploy`, then `tags read/write/browse`, `alarms`, `exec`. (Depends on 2 for client + 3 for a gateway to test against.)
5. **Project operations** — list/import/export/sync, cross-gateway copy (mostly REST project import/export endpoints; depends on client maturity from 2).
6. **TUI cockpit** — event loop shell can start any time after 2, but full coverage lands last since it *exposes* the completed action surface. Building it against a stable actions layer avoids rework.
7. **Ecosystem wrappers & polish** — `lint` delegation, `reset-trial` delegation, snapshot/restore, cross-gateway sync conveniences.

---

## Testing Strategy

**Layer 1 — unit (no I/O):** actions with a mock `GatewayApi` trait (returns fixture JSON → assert typed models). Config parsing, output rendering, compose-discovery ordering logic (point resolver at temp dirs).

**Layer 2 — HTTP contract:** `wiremock`-style local server (or `mockito` crate) asserting real URL shapes — especially `/system/webdev/{project}/{route}` bodies and auth headers. Keeps the client honest without a gateway.

**Layer 3 — integration against the dockerized rig (dogfoods the product):**
- Fixture compose project under `tests/rig/` using the same conventions as git-module (ports 9088/9043, `ACCEPT_IGNITION_EULA`, admin from env) — or discover the git-module rig when present.
- Test flow literally runs the binary: `ign rig up --wait` → `ign webdev deploy` → `ign tags read --json` → assert on JSON → `ign rig down -v`. The CLI tests the CLI.
- Marked `#[ignore]`-by-default or feature-gated (`` `cargo test --features rig` ``) so CI matrix can split fast/slow; GitHub Actions service containers or a docker-enabled runner for the slow lane.
- Trial-reset explicitly NOT under test here (requires Playwright + lapsed trial; covered by delegation target's own repo).

---

## Ecosystem Interop

**Principle: delegate via subprocess + JSON contracts; share conventions via discovery, never via imports across repos.**

| Sibling | Relationship | Boundary |
|---|---|---|
| **ignition-nvim** | None at runtime (editor-side LSP). Optional future: nvim reads `ign` JSON for "live gateway" features (tag browse in editor). | CLI never invokes nvim; nvim may invoke `ign --json`. Config: no sharing needed. |
| **ignition-lint** | `ign lint` subcommand wraps `ignition-lint --report-format json` (verified flag), re-renders through our output layer, maps exit codes. | Pure shell-out delegation; if binary absent, action returns setup-guidance error (pattern from ignition-mcp's `_WEBDEV_NOT_CONFIGURED`). |
| **ignition-git-module** | Source of rig conventions (compose, gw-build/gw-init/gw-secrets, ports 9088/9043). Rig discovery reads its `docker/` dir. No code coupling. | Convention reuse only. |
| **WHK-Global** | Delegation target for trial reset (`e2e/reset_trial.mjs`); future target for `project pull/sync`. | Subprocess + env contract. |
| **ignition-trial-resetter** | Primary trial-reset target for `rig reset-trial` (verified: no REST path exists). `node reset-trial.mjs` with `GATEWAY_URL/GATEWAY_USER/GATEWAY_PASS` env. | Subprocess; document Node+Playwright as optional rig dependency. |
| **ignition-mcp** | Being replaced. Its 42-tool catalog + 5 WebDev endpoint names are the **coverage checklist** for parity before retirement. | Read its summary JSON as a spec artifact; no runtime link. |
| **83-api (Bruno)** | Endpoint reference for REST coverage; .bru files double as fixtures for wiremock tests. | Spec artifact only. |

---

## Open Questions

1. **WebDev deploy mechanism** — per-resource import vs full project-zip import via 8.3 REST? Needs a short spike against the rig in phase 4 planning. (The import endpoints exist in 83-api; exact minimal-payload form unverified.)
2. **API token bootstrap UX** — creating a token requires an authenticated call; first-run flow for a fresh rig is basic-auth → mint token → store env name? Decide in phase 2.
3. **Script-exec security posture** — `scriptExec` route runs arbitrary Jython on the gateway. Restrict by dedicated gateway role vs allow admin-only; document loudly. Decide when authoring routes (phase 4).
4. **`rig snapshot` semantics** — gateway backup (`/data/api/v1/backup`) vs docker volume snapshot vs both; affects restore fidelity (licensed state, module state). Phase 3/7 question.
5. **TUI background refresh cadence** — poll gateway health/tags on tick vs on-demand; matters for prod profiles where polling is visible noise. Decide in phase 6 design.
6. **Comfy-table vs ratatui-table reuse for CLI human output** — trivial, pick during phase 1; noted so it isn't re-litigated.
7. **Tag history route** — ignition-mcp has `tagHistory`; confirm historian is enabled on default rigs or gate the action behind capability detection.

---

## Sources

- ratatui official templates repo (github.com/ratatui/templates) — event-driven-async & component patterns, verified 2026-08-20 (HIGH)
- bollard docs via Context7 — container API surface, verified 2026-08-20 (HIGH; informed the *decision to avoid* it for compose lifecycle)
- Local source inspection (HIGH): `ignition-git-module/docker/` (compose conventions, .env, ports, secrets), `ignition-mcp/src/` (client auth, WebDev URL shape, endpoint catalog, env-prefix config), `WHK-Global/com.inductiveautomation.webdev/resources/` (route file layout), `WHK-Global/e2e/reset_trial.mjs` (env names), `ignition-trial-resetter/README.md` (no-REST-for-trial-reset constraint), `83-api/bruno/` (8.3 endpoint + API-token surface)
- Cargo workspace conventions: stable, well-established practice (HIGH, training data + consistent with all observed ecosystem repos)
