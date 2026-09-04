# Feature Landscape

**Domain:** Gateway-management / developer CLI+TUI for Ignition 8.3+ (SCADA/industrial platform)
**Researched:** 2026-08-20
**Mode:** Ecosystem (greenfield — what do gateway CLIs have; table stakes vs differentiating)

## Landscape Summary (who else is in this space)

The external Ignition CLI space is **thin** — this niche is wide open (all findings via GitHub API + fetched READMEs, 2026-08-20):

| Tool | Lang | What it does | Overlap |
|------|------|--------------|---------|
| `igw-cli` (alex-mccollum, Feb 2026) | Go | Generic Ignition Gateway API wrapper: `call` passthrough, `doctor`, profiles, logs, diagnostics bundles, backup export/restore, tags import/export, restart, `wait` commands, `--json --select --raw --compact`, stable exit codes, `--yes` mutation guard, OpenAPI-spec discovery | **Highest overlap** — but no TUI, no rig control, no WebDev runtime ops (tag read/write, alarms, history, script exec), no ecosystem interop. HIGH confidence (README fetched) |
| `kindling` (paul-griffith, 48★) | Kotlin | Utilities for Ignition's custom export formats — .gwbk create/extract, project-export editing | Adjacent, not a live-gateway CLI. Already integrated by ignition-nvim. MEDIUM confidence (repo metadata only; README fetch 404'd) |
| `igniscope` (marcelo-6, Mar 2026) | Rust | Parses Ignition project exports and gateway backups (offline) | Offline parsing only, no gateway ops. MEDIUM confidence (repo description) |
| `ignition-agent-tools` (baidixueguo, May 2026) | Java+Python | 8.3 gateway module + Python CLI for tag automation over HTTP | Tag automation only, requires their custom module. MEDIUM confidence |
| Ignition official tooling | — | Gateway web UI, Designer, EAM web UI; Docker image entrypoint (commissioning, gwbk restore); REST API | No CLI/TUI cockpit exists from IA. HIGH confidence |

**Implication:** For Ignition specifically, "table stakes" is defined less by competitors and more by (a) the ignition-mcp 37-tool catalog this CLI replaces (author's own bar), (b) general DevOps-CLI conventions (kubectl/gh/igw-cli patterns: `--json`, exit codes, profiles, `wait`, `doctor`, completion), and (c) the 8.3 native REST surface (~100 endpoint families in the author's 83-api Bruno collection) plus WebDev for runtime ops.

**Key API fact confirmed from local 83-api collection:** native 8.3 REST covers config/CRUD/backups/EAM/sessions/performance — but **not** runtime tag value read/write, alarm queries/acks, tag history, or script *execution* (only running-script *diagnostics*). Those still require the WebDev backend, exactly as ignition-mcp's split assumes. HIGH confidence (675 Bruno requests inspected + ignition-mcp catalog).

---

## Table Stakes

Must-haves. The ignition-mcp catalog is the author's own minimum bar (it replaced the webpage for these); general CLI conventions (validated against igw-cli/kubectl/gh patterns) are the rest. Missing any of these = the user opens the gateway webpage again and the tool loses the daily-driver slot.

### A. Gateway health / inspection

| # | Feature | Command shape (illustrative) | Backend | Complexity | Notes |
|---|---------|------------------------------|---------|------------|-------|
| A1 | Gateway info + status (version, platform, revision, running state) | `ign status` / `ign info` | Native REST | Low | mcp: `get_gateway_info`. First thing every agent runs. |
| A2 | Module health (all modules, state, versions) | `ign modules` | Native REST | Low | mcp: `get_module_health`. |
| A3 | Gateway logs: list, fetch, tail (`-f`), download | `ign logs [tail]` | Native REST | Med | mcp: `get_gateway_logs`; igw-cli has list/download + logger-level mgmt. TUI log tail is a headline use. |
| A4 | Logger level management (get/set per-logger) | `ign logs level set` | Native REST | Low-Med | igw-cli parity; debugging staple. |
| A5 | Database connections status | `ign db` | Native REST | Low | mcp: `get_database_connections`. |
| A6 | OPC connections status | `ign opc` | Native REST | Low | mcp: `get_opc_connections`. |
| A7 | System metrics / performance (CPU, memory, historic + current, thread execution) | `ign metrics` | Native REST | Low-Med | mcp: `get_system_metrics`; 8.3 adds system-performance + thread-diagnostics endpoints — exceed the mcp catalog here. |
| A8 | Connected clients: designers, Perspective sessions, Vision sessions | `ign sessions` | Native REST | Low | mcp: `list_designers` only; 8.3 native has designer-sessions, perspective-sessions (incl. terminate), vision-sessions. |
| A9 | Gateway restart + restart-task status | `ign restart [--wait]` | Native REST | Med | restart-tasks endpoints; restart without wait-for-ready is a footgun — see A11. |
| A10 | Connectivity/auth preflight ("doctor") | `ign doctor` | Native + WebDev probe | Low-Med | igw-cli's best idea: URL, TCP, auth, read/write perm checks; ours adds **WebDev-route presence probe** and rig detection. |
| A11 | Wait/poll primitives: gateway up, restart complete, module ready | `ign wait gateway` | Native REST | Low-Med | Poll loops agents would otherwise hand-roll. igw-cli has `wait gateway/diagnostics-bundle/restart-tasks`. |

### B. Project operations

| # | Feature | Command shape | Backend | Complexity | Notes |
|---|---------|--------------|---------|------------|-------|
| B1 | List projects (+ inheritance/parent info) | `ign project ls` | Native REST | Low | mcp: `list_projects`; 8.3 adds valid-parents queries. |
| B2 | Project CRUD: create, delete, copy, rename | `ign project new/cp/mv/rm` | Native REST | Low-Med | mcp parity. |
| B3 | Export/import project (full, to/from file or stdout) | `ign project export/import` | Native REST | Med | mcp parity; foundation for sync/diff/lint/e2e workflows. |
| B4 | Resource-level ops: list, get, put, delete within a project | `ign resource ls/get/put/rm` | Native REST | Med | mcp: 4 resource tools. The "edit one view without re-importing everything" loop. |
| B5 | Import/export with collision policy (abort/overwrite/merge as API permits) | flag on B3 | Native REST | Low | igw-cli tags import defaults `--collision-policy Abort`; git-module import popup has Overwrite/Merge/Abort — match the convention. |

### C. Tag operations

| # | Feature | Command shape | Backend | Complexity | Notes |
|---|---------|--------------|---------|------------|-------|
| C1 | List tag providers; create/delete providers | `ign tag providers` | Native REST | Low-Med | mcp: 4 provider tools. |
| C2 | Browse tags (tree/filtered) | `ign tag browse` | Native REST | Low-Med | mcp: `browse_tags`. Gate for read/watch. |
| C3 | Read tag values (single/batch) | `ign tag read` | **WebDev** | Med | mcp: `read_tags`. Native REST cannot read runtime values. |
| C4 | Write tag value | `ign tag write` | **WebDev** | Med | mcp: `write_tag`. Needs the shipped WebDev routes (D2). |
| C5 | Tag config CRUD (get/create/edit/delete) | `ign tag get/create/edit/rm` | **WebDev** | Med-High | mcp: 4 tag-config tools. JSON in/out. |
| C6 | UDT definitions: list types, get definition | `ign tag udt ls/get` | **WebDev** | Med | mcp: `list_udt_types`, `get_udt_definition`. (Native tag *export/import* covers UDTs too — consider native for bulk, WebDev for surgical.) |
| C7 | Alarms: active, history, acknowledge | `ign alarm active/history/ack` | **WebDev** | Med | mcp: 3 alarm tools. TUI alarm panel depends on this. |
| C8 | Tag history query | `ign tag history` | **WebDev** | Med | mcp: `get_tag_history`. |
| C9 | Tag provider export/import (bulk, json/xml/csv) | `ign tag export/import` | Native REST | Med | 8.3 native (igw-cli has it); collision policy default Abort. Complements C5 surgical ops. |

### D. Profile / config management

| # | Feature | Command shape | Complexity | Notes |
|---|---------|--------------|------------|-------|
| D1 | Multi-gateway profiles (dev/test/prod): URL, auth, label; `--profile` flag + `IGNITION_PROFILE` env; active default | `ign profile add/use/ls` | Med | kubectl contexts / igw-cli profiles pattern. **Safety: profile name should be visible in every prompt/output** (prod vs dev misfire is THE classic multi-target CLI accident). |
| D2 | Auth: API token (preferred) + basic auth; token from env, file, or keychain; never echo secrets in `--json` output | config + env | Low-Med | mcp uses API-key-or-basic; 83-api notes `data/config/EXTERNAL/ignition/api-token` persists better across gwbk restores. |
| D3 | Env-var overrides for everything (URL, token, user/pass, SSL-verify, WebDev endpoints) — scriptable without config files | `IGNITION_*` env | Low | mcp's `IGNITION_MCP_*` pattern, renamed. |
| D4 | Secrets handling: read from env/files (git-module `gw-secrets/` file pattern exists), optional OS keychain; no plaintext secrets in config by default | config | Med | Interop with rigs' `.env`/`gw-secrets` conventions. |

### E. Agentic / scripting conventions (cross-cutting — do these from day one)

| # | Feature | Complexity | Notes |
|---|---------|------------|-------|
| E1 | `--json` on **every** subcommand, stable field names within major versions | Med | Project constraint; igw-cli documents JSON-field stability policy — copy that discipline. |
| E2 | Stable, documented exit codes (0 ok / 2 usage / config / auth / network / target-state distinctions) | Low-Med | igw-cli: 0/2/6/7 + compat policy. Agents branch on these. |
| E3 | Machine-readable errors: JSON error envelope with code, message, endpoint, hint (e.g., "WebDev route missing → run ign webdev deploy") | Low-Med | mcp returns "clear error with setup instructions" for missing WebDev — keep that behavior. |
| E4 | Non-interactive by default; destructive ops require `--yes` (or `IGNITION_YES` env) | Low | igw-cli mutation-safety pattern; agents and CI both need it. |
| E5 | Shell completion (bash/zsh/fish) via clap | Low | Standard clap_complete; cheap, expected. |
| E6 | `ign version` (+ check gateway min-version and refuse cleanly on <8.3.1) | Low | Support policy from git-module v2. |
| E7 | Sensible default output for humans (tables), `--compact` one-line JSON for agents | Low | igw-cli `--compact` pattern. |

---

## Differentiators

Nobody in the surveyed landscape combines these with the table-stakes API layer. These are the competitive advantages — and the reasons this CLI (not igw-cli, not the webpage, not ignition-mcp) becomes the daily driver.

| # | Feature | Value proposition | Backend | Complexity | Notes |
|---|---------|-------------------|---------|------------|-------|
| F1 | **Docker test-rig lifecycle**: `rig up/down/status/reset` driven by compose-file discovery (git-module `docker/` conventions: docker-compose.yml, gw-init, gw-secrets, test-rig); port mapping awareness (8088→9088 etc.); `rig logs` passthrough; wait-for-commissioned | One command replaces a terminal of `docker compose -f ... --env-file ...` incantations; the author's daily workflow | Docker API / compose CLI | **High** | Pattern source: ignition-git-module/docker + test-rig (verified locally). Delegate to `docker compose` CLI first; Docker API only if needed. |
| F2 | **Trial reset**: `rig trial reset` (+ auto mode) folding ignition-trial-resetter logic | Trial expiry breaks every dev rig; currently 3 separate scripts exist (trial-resetter, WHK-Global e2e/reset_trial.mjs, instance envs) | Headless browser | **High + risk** | Trial reset has NO REST endpoint — needs browser automation (verified: resetter + WHK-Global both use Playwright; mcp catalog lacks it). In Rust this means chromiumoxide/headless_chrome or shelling to the existing Node tool. Flag for phase-level spike. |
| F3 | **Rig snapshot/restore**: snapshot = gwbk download (native `gateway-backups` API) + project/tag exports; restore = gwbk restore | Repeatable test states; nobody offers this | Native REST | Med-High | Native Get/Restore Gateway Backup exists (83-api verified). Great for e2e fixture reset. |
| F4 | **TUI cockpit** (ratatui): every CLI action available; live status dashboard (modules, sessions, metrics), log tail with level filtering, tag browser + live watch, alarm panel, project/resource browser, profile switcher | k9s-for-Ignition; nothing like it exists anywhere in the Ignition ecosystem | Same command layer | **High** | Navigation paradigm: object-list → detail (k9s/lazygit style), not menu-tree. Shares the exact command core with CLI (constraint: TUI is the cockpit, not a viewer). |
| F5 | **Cross-gateway sync/diff**: `ign project diff <profile-a> <profile-b>` (resource-level), then selective sync | Env promotion (dev→test→prod) currently done by hand via webpages | Native REST (export + compare) | Med-High | Build on B3/B4. Compare resource JSON (decoded where possible). |
| F6 | **Shipped + versioned WebDev backend**: `ign webdev deploy/status` installs/updates the CLI's own WebDev routes on a gateway; route version negotiation; doctor probes it | Removes the #1 setup failure of ignition-mcp (unconfigured endpoints) | WebDev module deploy via REST | Med-High | PROJECT.md decision. Version-stamp routes; CLI refuses WebDev commands with actionable error if mismatched. |
| F7 | **Script execution** (opt-in): `ign script run` via WebDev, disabled by default like mcp's `run_gateway_script` | Agents can compute/debug gateway-side | WebDev | Med | Native 8.3 has only running-script *diagnostics* (verified) — execution is WebDev-only. Guard rails mandatory (it's RCE-by-design). |
| F8 | **Ecosystem interop** (see dedicated section): script decode/encode on export/import, tag-export browsing, `ign lint` delegation, e2e driver mode | The whole WhiskeyHouse toolkit becomes one workflow | Mixed | Med | Detailed below. |
| F9 | **Backup/EAM surface beyond the mcp catalog**: gwbk download/upload-restore, EAM task list/history/create (fan-out to agent gateways) | Replaces EAM webpage for common reads; enables F3 | Native REST | Med | 83-api verified: gateway-backups + full eam-tasks family. Decide EAM *write* scope carefully (anti-footgun). |
| F10 | **Tag watch** (TUI): live-updating subscribed tag values | The "designer without Designer" feel for checkout/debug | WebDev polling | Med | Poll-based first (WebDev has no push); C2+C3 dependency. |
| F11 | **Session management**: terminate Perspective sessions/pages (native) | Kick stuck sessions without the webpage | Native REST | Low-Med | 83-api verified (Terminate Perspective Session(s)). |

Complexity note on F2: this is the single riskiest differentiator. Cheapest credible path is wrapping/invoking the existing Playwright resetter (systemd timer pattern already proven) rather than embedding browser automation in the Rust binary. Decide at phase-planning.

---

## Anti-Features

Deliberately NOT building. Each is either owned by a sibling tool, violates a project constraint, or is scope creep.

| Anti-Feature | Why avoid | What to do instead |
|--------------|-----------|--------------------|
| LSP / completions / hover / go-to-def for Ignition scripts | ignition-nvim + ignition-lsp own this (14 `system.*` modules, 239+ functions, Java/Jython stubs) | Interop only: decode/encode scripts so files opened in nvim/VS Code "just work" (see interop). |
| Lint engine (Jython syntax, naming conventions, schema checks) | ignition-lint owns this (rules, severity levels, ignore files, CI action, its own MCP server) | `ign lint` **delegation wrapper**: detect `ignition-lint` on PATH, run it against an exported project or path, pass through exit codes + JSON. Error clearly with install hint if absent. |
| Designer-side Git integration (commit/push/pull/branch/stash UI) | ignition-git-module owns this inside the Designer | Interop only: understand git-module conventions (git.yaml, gw-init, tags/ exports, `tags_importOnStartup`), browse its tag exports, never reimplement. |
| Ignition 8.1.x support | Project decision: API variance (Jython-era web framework, no 8.3 React/REST surface) not worth carrying; git-module v2 set the precedent | Clean version check + helpful refusal (E6). |
| MCP server transport (v1) | Project decision: CLI + `--json` IS the agent interface; MCP serving is a later call | Keep JSON contract stable enough that a thin MCP shim could wrap it later. |
| Perspective/Vision view editing, component trees, form designers | VS Code extension (Project Browser, Component Tree) + Designer own this | B4 resource get/put covers surgical raw-JSON edits; leave semantics to editors. |
| OpenAPI-spec discovery subsystem (igw-cli's `api list/search/sync`) | Scope creep against "simple but complete"; the curated command surface IS the product | Thin escape hatch only: `ign api call --method --path` passthrough for endpoints not yet curated (low cost, unblocks edge cases). |
| Daemon / background service | Constraint: single binary, no daemon | Poll-based wait/watch; user schedules if needed (trial auto-reset may shell to existing timer pattern). |
| Web UI | TUI is the human interface; a web UI recreates the gateway webpage problem | — |
| Vision project management (bin→XML auto-export) | git-module roadmap explicitly owns this | Revisit when git-module ships it; CLI consumes results. |

---

## Feature Dependencies

```
D1 profiles + D2/D3 auth/env  ──►  everything (every command resolves a target)
E1/E2 JSON + exit codes       ──►  everything scripted (build into command core, not after)
A10 doctor                    ──►  depends on D1/D2; probes WebDev (F6) + rig (F1)
F6 webdev deploy              ──►  C3 C4 C5 C6 C7 C8 F7 F10  (all WebDev ops)
A2 modules ── F6: WebDev requires WebDev module installed (doctor should check)
C2 tag browse ──► C3 read ──► F10 watch (TUI)
C1 providers ──► C2/C5 (tag addressing), C9 (bulk)
B3 project export/import ──► B4 resource ops ──► F5 sync/diff ──► (multi-profile D1)
B3 export ──► F8 decode/encode interop ──► nvim/agent edit ──► B3 import (round trip)
A9 restart ──► A11 wait  (restart without wait is incomplete)
F1 rig up ──► F2 trial reset ──► long-running rig
F1 rig ──► F3 snapshot/restore ──► e2e fixture reset (WHK-Global)
Native gwbk (F9) ──► F3
A3 logs ──► TUI log tail (F4)
A5/A6/A7/A8 ──► TUI dashboard (F4)
F4 TUI ──► reuses command layer of A/B/C (no separate API path — architectural invariant)
B3 export ──► ign lint delegation ──► CI loop
```

Key ordering consequences for the roadmap:
1. **Profiles + JSON/exit-code core first** — every later feature sits on them.
2. **WebDev deploy (F6) gates the entire tag-runtime/alarm/history block** — schedule it before or with C3–C8.
3. **Rig lifecycle (F1) unlocks F2/F3 and e2e workflows** — but is independent of the WebDev track; can parallelize.
4. **TUI (F4) comes last-ish**: it consumes every command; building it early forces rework.

---

## Notes from ignition-mcp catalog mapping

The 37-tool catalog maps 1:1 onto table-stakes commands (this is the replacement bar):

| mcp category (tools) | CLI coverage | Notes |
|----------------------|--------------|-------|
| Gateway (6) | A1 A2 A3 A5 A6 A7 | Direct parity. |
| Projects (8) | B1 B2 B3 | Direct parity (+ parents query beyond mcp). |
| Project Resources (4) | B4 | Direct parity. |
| Designers (1) | A8 | **Exceed**: add perspective-sessions + vision-sessions + terminate (native 8.3). |
| Tag Providers (4) | C1 | Direct parity (native). |
| Tag Browse (1) | C2 | Direct parity (native). |
| Tag Values (2) | C3 C4 | WebDev, via shipped backend F6. |
| Tag Config (6) | C5 C6 | WebDev; consider native C9 bulk export/import as complement. |
| Alarms (3) | C7 | WebDev. |
| Historian (1) | C8 | WebDev. |
| Script Execution (1) | F7 | WebDev, opt-in — keep mcp's default-off stance. |

**Coverage stance: parity is table stakes; the CLI should also exceed the catalog** using native 8.3 endpoints the mcp never wrapped (verified in 83-api): gateway backups get/restore (F3/F9), EAM tasks (F9), restart-tasks (A9), system-performance + thread-diagnostics (A7), license-status, diagnostics bundle generation/download, redundancy status, gateway-network/agent status. Which of these graduate from "escape hatch via `ign api call`" to curated commands should be demand-driven — start with the ones in this table (A7/A9/F9).

**Config translation:** mcp's env matrix (gateway URL, API key, user/pass, SSL-verify, five WebDev endpoint vars, script-exec toggle) collapses in the CLI to: profiles (D1) + per-profile WebDev auto-discovery via F6 (endpoint vars should not be user-facing anymore — the CLI knows its own routes) + `IGNITION_ENABLE_SCRIPT_EXECUTION` equivalent for F7.

---

## Ecosystem Interop

### ignition-nvim (LSP/editor — editing belongs there, pairing belongs here)
- **Script decode/encode interop (differentiator F8):** nvim's decoder extracts embedded Python from resource JSON into editable buffers. CLI counterpart for file/agent workflows: `ign project export --decode-scripts` (emit `.py` alongside JSON) and `ign project import --encode-scripts` (round-trip). Same convention = a resource exported by CLI, decoded, edited in nvim/VS Code, re-encoded, imported — one coherent loop. Complexity: Med. Dependency: B3.
- **.gwbk handling:** nvim opens .gwbk via kindling. CLI stays out of gwbk *authoring*; for F3 snapshots the gwbk is opaque bytes via native API (correct division). If offline gwbk extraction is ever needed, delegate to kindling rather than reimplement (note: kindling is Kotlin/GUI-oriented — treat as optional integration, MEDIUM confidence).
- **Explicitly not built:** completions, hover, workspace symbols — nvim/lsp own them.

### ignition-lint (linting engine — theirs)
- **`ign lint` delegation (F8):** wrapper, not engine. Detect binary on PATH → run against path or freshly exported project (B3) → pass through exit codes and `--report-format json`. Absent binary → actionable error with `pip install ignition-lint-toolkit` hint. This slots the CLI into the existing CI story (their GitHub Action exists; our wrapper serves the interactive/e2e path).
- **Explicitly not built:** rules, severities, suppression files.

### ignition-git-module (Designer git + rig conventions — the rig pattern source)
- **Rig conventions (F1):** `rig` commands discover and drive the module repo's docker layout — compose files, `gw-init/git.yaml`, `gw-secrets/` (verified locally: docker/, test-rig/ with git-server + gateway, gw-build). Same env var names (`GATEWAY_ADMIN_USERNAME/PASSWORD`, `GATEWAY_GIT_USER_SECRET`, `IGNITION_VERSION`) so a rig started by hand works with the CLI and vice versa.
- **Tag export browsing:** module exports `tags/` per project (tags.json/udts.json seen in WHK-Global). `ign tag browse --from-export <path>` reads git-module exports offline — complements live browse (C2) and matches the nvim VS Code Tag Browser's data source. Complexity: Low-Med.
- **`tags_importOnStartup` awareness:** rigs relying on it restore tags on restart; CLI tag ops should respect "provider authority" guidance (one authoritative project per provider — from module docs, verified).
- **Explicitly not built:** any git operations, Designer UI, commissioning logic itself (CLI *drives* the compose files that do it).

### WHK-Global (parent project + e2e — the CLI's flagship consumer)
- **E2e driver mode:** WHK-Global's Playwright specs need: healthy gateway (A10/A11), fresh project state (B3 import / F3 restore), fixture tag writes (C4), trial reset (F2), port/env conventions (D1/D3 — resetter instances/*.env pattern; observed ports 8088/9088/9043). The e2e `reset_trial.mjs` already exists as a reusable node script — F2 should wrap or replicate it, not reinvent (spike: Rust browser automation vs shell-out).
- **WebDev independence:** WHK-Global deploys its own WebDev routes (perspective-screenshot etc. seen); the CLI ships its own versioned routes (F6) per PROJECT.md decision — doctor should tolerate both existing side-by-side (different route names).
- **Named queries / scripts on the gateway:** `ignition/` dir (named-query, script-python, event-scripts...) is repo-level content — git-module + editors own it; CLI touches it only via project export/import.

### Cross-cutting interop principle
Interop features are **read/delegate/drive**, never re-implement. The CLI is the operational hub (health, projects, tags, rigs) that makes the sibling specialists (edit, lint, git) reachable from one place — including for AI agents via `--json`.

---

## MVP Recommendation

Prioritize:
1. **Core plumbing**: D1 profiles, D2/D3 auth/env, E1–E7 JSON/exit-codes/non-interactive (everything depends on it)
2. **Read-side inspection**: A1–A8, A10 doctor, A11 wait (immediate daily value, native-only, no WebDev needed)
3. **Projects**: B1–B5 (webpage replacement for project admin)
4. **WebDev deploy (F6) + tag runtime C1–C8** (the ignition-mcp replacement bar; F6 first)
5. **Rig lifecycle F1 (+F3 snapshot)** — differentiator, unblocks e2e; F2 trial reset as a spike/wrap decision

Defer: F4 TUI cockpit (consume the finished command layer), F5 cross-gateway diff, F7 script exec (opt-in flag day), F9 EAM writes, F8 interop niceties (decode/encode, lint wrapper, export browsing) — all post-MVP, all phased by dependency graph above.

## Sources

- Local (HIGH confidence, author's own): `.planning/PROJECT.md`; ignition-mcp readme (37-tool catalog + env matrix + WebDev prerequisites); ignition-git-module readme + docker/ + test-rig/ (rig conventions, git.yaml, tags_importOnStartup, tag export ownership); ignition-nvim readme (decode/encode commands, kindling, VS Code extras incl. Tag Browser); ignition-lint readme (CLI, JSON reports, CI/MCP integrations); ignition-trial-resetter readme + instances/ (trial reset mechanism, env pattern); WHK-Global e2e/reset_trial.mjs + com.inductiveautomation.webdev/ (WebDev routes, IdP stepped login); 83-api Bruno collection (~100 endpoint families, 675 requests — native surface incl. gateway-backups, eam-tasks, perspective-sessions terminate, system-performance, restart-tasks, running-scripts diagnostics-only).
- External: igw-cli README (HIGH — fetched raw): command set, doctor, wait, profiles, JSON/select/raw/compact, exit codes 0/2/6/7, `--yes` mutation guard, OpenAPI discovery. kindling repo metadata (MEDIUM — README fetch 404). igniscope, ignition-agent-tools repo descriptions (MEDIUM, unverified).
- General CLI conventions (kubectl/gh/docker): HIGH confidence standard knowledge; corroborated by igw-cli design.
