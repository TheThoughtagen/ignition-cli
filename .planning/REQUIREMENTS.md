# Requirements: ignition-cli

**Defined:** 2026-08-20
**Core Value:** One binary that lets a developer (or an AI agent) fully operate and inspect an Ignition 8.3+ gateway — health, projects, tags, rigs — without opening the gateway webpage or Designer.

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### Core Plumbing

- [ ] **CORE-01**: User can configure multiple gateway profiles (add/use/list) with URL, auth, and label, selected via `--profile` flag or `IGNITION_PROFILE` env, with the active profile name visible in every output
- [ ] **CORE-02**: User can authenticate via API token (preferred) or basic auth, supplied through env vars or config, with secrets never echoed in JSON output or logs
- [ ] **CORE-03**: Every subcommand supports `--json` with stable field names, plus `--compact` one-line JSON for agents
- [ ] **CORE-04**: Every command exits with documented stable exit codes (0 ok / 2 usage / distinct config, auth, network, target-state codes)
- [ ] **CORE-05**: Errors render as a machine-readable JSON envelope with code, message, endpoint, and actionable hint (e.g., "WebDev route missing → run ign webdev deploy")
- [ ] **CORE-06**: Commands are non-interactive by default; destructive operations require `--yes` or `IGNITION_YES` env
- [ ] **CORE-07**: User can generate shell completions (bash/zsh/fish)
- [ ] **CORE-08**: `ign version` checks gateway minimum version and refuses cleanly on <8.3.1

### Gateway Health

- [ ] **HLTH-01**: User can view gateway info and status (version, platform, revision, running state)
- [ ] **HLTH-02**: User can view module health for all modules (state, versions)
- [ ] **HLTH-03**: User can list, fetch, download, and tail (`-f`) gateway logs
- [ ] **HLTH-04**: User can get and set logger levels per-logger
- [ ] **HLTH-05**: User can view database connection status
- [ ] **HLTH-06**: User can view OPC connection status
- [ ] **HLTH-07**: User can view system metrics (CPU, memory, historic + current, thread execution)
- [ ] **HLTH-08**: User can view connected sessions: designers, Perspective sessions (incl. terminate), Vision sessions
- [ ] **HLTH-09**: User can restart the gateway with optional `--wait` for ready
- [ ] **HLTH-10**: User can run `ign doctor` — connectivity, auth, read/write permission, WebDev-route presence, and rig detection preflight
- [ ] **HLTH-11**: User can wait/poll for gateway up, restart complete, or module ready

### Projects

- [ ] **PROJ-01**: User can list projects with inheritance/parent info
- [ ] **PROJ-02**: User can create, delete, copy, and rename projects
- [ ] **PROJ-03**: User can export a project to file (full, with collision policy abort/overwrite on import)
- [ ] **PROJ-04**: User can import a project from file or stdin
- [ ] **PROJ-05**: User can list, get, put, and delete individual resources within a project (surgical edit loop)

### WebDev Backend

- [ ] **WEB-01**: User can deploy the CLI's own versioned WebDev routes to a gateway via `ign webdev deploy` and check status via `ign webdev status`
- [ ] **WEB-02**: WebDev routes version-negotiate: CLI refuses WebDev-dependent commands with an actionable error on route mismatch

### Tags

- [ ] **TAGS-01**: User can list tag providers and create/delete providers
- [ ] **TAGS-02**: User can browse tags as a tree with filtering
- [ ] **TAGS-03**: User can read tag values (single or batch)
- [ ] **TAGS-04**: User can write a tag value
- [ ] **TAGS-05**: User can get, create, edit, and delete tag configs (JSON in/out)
- [ ] **TAGS-06**: User can list UDT types and get UDT definitions
- [ ] **TAGS-07**: User can view active alarms, query alarm history, and acknowledge alarms
- [ ] **TAGS-08**: User can query tag history
- [ ] **TAGS-09**: User can bulk export/import tag providers (json/xml/csv) with collision policy (default abort)

### Rig Lifecycle

- [ ] **RIG-01**: User can run `rig up/down/status/reset` driven by compose-file discovery (git-module/WHK-Global conventions), with port-mapping awareness and wait-for-commissioned
- [ ] **RIG-02**: User can view rig logs (passthrough) and trial status
- [ ] **RIG-03**: User can reset trial state (spike decides: wrap existing Playwright resetter vs native headless HTTP+CSRF flow)
- [ ] **RIG-04**: User can snapshot a rig (gwbk download + project/tag exports) and restore it to a repeatable state

### TUI Cockpit

- [ ] **TUI-01**: User can open a ratatui cockpit (`ign tui`) exposing every CLI action through object-list → detail navigation (k9s/lazygit style)
- [ ] **TUI-02**: User can view a live status dashboard (modules, sessions, metrics) with periodic refresh
- [ ] **TUI-03**: User can tail gateway logs with level filtering in the TUI
- [ ] **TUI-04**: User can browse tags and live-watch tag values in the TUI
- [ ] **TUI-05**: User can view and acknowledge alarms in an alarm panel
- [ ] **TUI-06**: User can browse projects/resources and switch profiles in the TUI

### Advanced Operations

- [ ] **SYNC-01**: User can diff two gateways' projects at resource level (`ign project diff <profile-a> <profile-b>`)
- [ ] **SYNC-02**: User can selectively sync resources between gateways (dev→test→prod promotion)
- [ ] **SCRPT-01**: User can execute gateway scripts via `ign script run` (opt-in, disabled by default)
- [ ] **BKUP-01**: User can download and restore gateway backups (gwbk) via native API
- [ ] **BKUP-02**: User can list EAM task history and create EAM tasks (read-heavy; write scope guarded)

### Ecosystem Interop

- [ ] **INTR-01**: User can export a project with `--decode-scripts` (emit `.py` alongside JSON) and import with `--encode-scripts` for round-trip editing in nvim/VS Code
- [ ] **INTR-02**: User can run `ign lint` which delegates to ignition-lint on PATH, passing through exit codes and JSON reports (actionable install hint if absent)
- [ ] **INTR-03**: User can run `ign tag browse --from-export <path>` to browse git-module tag exports offline

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### API Escape Hatch & Extensions

- **EXT-01**: `ign api call --method --path` raw passthrough for uncurated endpoints
- **EXT-02**: License status, diagnostics bundle generation/download, redundancy status, gateway-network/agent status as curated commands
- **EXT-03**: EAM write operations beyond guarded basics
- **EXT-04**: MCP-server transport mode (thin shim over the stable JSON contract)

### TUI Extensions

- **TUIX-01**: Configurable polling cadence per profile (prod vs dev)
- **TUIX-02**: TUI theming

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Ignition 8.1.x support | API variance not worth carrying; git-module v2 precedent; 8.3.1+ only |
| LSP / completions / hover / go-to-def | ignition-nvim + ignition-lsp own editing |
| Lint engine (rules, severities, suppression) | ignition-lint owns it; we delegate only |
| Designer-side Git integration | ignition-git-module owns it |
| Perspective/Vision view editing semantics | Editors + Designer own semantics; resource get/put covers raw-JSON edits |
| Web UI | TUI is the human interface; web UI recreates the webpage problem |
| Daemon / background service | Single binary constraint; poll-based wait/watch |
| OpenAPI-spec discovery subsystem | Scope creep; curated surface is the product (escape hatch in v2) |
| Vision project bin→XML auto-export | git-module roadmap owns it; CLI consumes results |
| gwbk authoring/parsing offline | Kindling/nvim territory; CLI treats gwbk as opaque bytes via native API |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| (to be filled by roadmap) | | |

**Coverage:**
- v1 requirements: 46 total
- Mapped to phases: 0
- Unmapped: 46 ⚠️

---
*Requirements defined: 2026-08-20*
*Last updated: 2026-08-20 after initial definition*
