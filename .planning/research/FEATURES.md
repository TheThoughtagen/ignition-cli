# Feature Research — v1.1 New Features

**Domain:** Gateway-management CLI/TUI for Ignition 8.3+ — v1.1 feature landscape (11 new features on a shipped v1.0 base)
**Researched:** 2026-09-04
**Confidence:** HIGH overall (endpoint surface verified in local 83-api collection + ignition-mcp catalog + v1.0 live-verified docs; comparable-tool patterns from kubectl/gh/k9s/btop/lazygit conventions — MEDIUM where training-data-only, noted inline)
**Mode:** Ecosystem, scoped to the 11 v1.1 features. Stack decisions are settled by the sibling STACK researcher (MCP = hand-rolled JSON-RPC 2.0 stdio shim; LSP = lsp-server 0.10 + lsp-types 0.97; theming/polling/workspace = zero new crates) — **not re-litigated here.** v1.0 scope lives in `FEATURES-v1.0.md`; this file only covers the new surface.

---

## Per-Feature Verdict (scannable summary)

| # | Feature | Table stakes core | Differentiator to chase | Worst anti-feature | Complexity | v1.0 deps |
|---|---------|-------------------|--------------------------|--------------------|------------|-----------|
| 1 | `ign api call` | GET-default raw passthrough, method/body flags, raw-not-contract output | method-aware safety classification | an OpenAPI discovery subsystem (already rejected in v1.0) | LOW | profiles, auth, exit codes |
| 2 | Curated diagnostics | license status, redundancy status, GAN gateways, bundle generate+download+wait | `doctor --deep` / morning-check roll-up | license *activation* writes by default | LOW–MED | status/wait patterns, E-contract |
| 3 | EAM writes | task suspend/resume/cancel/force + rename/modify/delete, each behind `--yes` | blast-radius preview before execution | fleet-wide Upgrade Agent automation | MED | EAM reads, `--yes`, wait |
| 4 | MCP transport | initialize/tools-list/tools-call over stdio JSON-RPC, wrapping the command layer | curated tool subset + confirm-field guard mapping | exposing all ~100 REST families as tools | MED | frozen envelope (payload), whole command layer |
| 5 | Tag bulk xml/csv | server-byte-faithful export + same-endpoint import, format sniffed | lossy-import warning report | CLI-side JSON↔CSV conversion as a "migration" tool | LOW–MED | TAGS-09 export/import |
| 6 | Historian binding closure | close the gap via Designer-diff oracle, prove data flows e2e | full Designer parity for CLI-created history tags | shipping a shape guess without live data proof | MED (research-shaped) | 05-06 fixture, `tags config get` |
| 7 | TUI theming | built-in theme set, named palette keys, persisted choice | user theme file + live switch | per-widget arbitrary color pickers | LOW | TUI cockpit, config plumbing |
| 8 | Polling cadence | global default + per-view override + min clamp | per-panel cadence + pause-on-hidden | sub-second defaults (gateway load) | LOW | TUI poll loop, wait loops |
| 9 | `ign edit` | $EDITOR + temp file + save-detect + error-reopen loop (kubectl-edit pattern) | `--decode-scripts` leg (edit the .py) | a built-in text editor | MED | resource put, tag config put, Flint codec |
| 10 | LSP server | stdio LSP serving gateway-data completions/diagnostics, offline-degraded | live tag-path + named-query completions, live hover | reimplementing the Python LSP's static knowledge | MED–HIGH | browse/config/export, profiles |
| 11 | Workspace checkout | checkout/status/push with deterministic path mapping + manifest | `--decode-scripts` checkout, server-side change detection | bidirectional live-sync daemon (rejected v1.0) | MED–HIGH | export/import, zip surgery, codec |

---

## Feature Landscape

### 1. `ign api call` — raw REST passthrough

**How comparable tools do it:** `gh api <endpoint> [-X METHOD] [-f k=v] [-F typed] [--input -] [--jq]` (GET default, status-coded exit, jq filter, `--paginate`); `kubectl get --raw` / `kubectl proxy` (raw passthrough with zero negotiation). Both are "escape hatches" for endpoints the tool hasn't curated, and both keep raw output **explicitly outside** their stable-output contracts. Pattern is well-established: HIGH confidence.

**Table stakes**

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| GET by default; path relative to gateway base | gh api convention; every raw user expects it | LOW | auto-prefix `/data/api/v1/`; absolute path opt-out via leading `/data` awareness |
| `--method`, `--header`, body via `--field`, `--input -`/file | gh api shape | LOW | JSON body detection; stdin for agents |
| Raw JSON passthrough, labeled non-contract | users must know raw ≠ frozen envelope | LOW | stable envelope fields (e.g. `--jq`-style `--select`) unavailable in raw mode — or clearly marked advisory |
| Exit codes track HTTP status class | agents branch on them | LOW | E2 table gets two slugs: `upstream_error` (4xx/5xx mapped), `raw_not_found` |
| Auth/profile resolution identical to curated commands | one config surface | LOW | free (D1/D2) |

**Differentiators**

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Method-aware safety classification | non-GET requires `--yes` (consistent with v1.0 guard); known-destructive paths (activation, redundancy failover, task queue pause) get a named warning | LOW–MED | a small static path-classifier table; this is the single best safety lever |
| `--select`/jq-style filter over raw responses | agents avoid jq dependency | LOW | reuse existing selector if one exists in v1.0 output path |

**Anti-features**

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| OpenAPI spec discovery/sync subsystem (igw-cli's `api list/search/sync`) | "discover everything" | already rejected in v1.0 as scope creep; the curated surface IS the product | `ign api call` itself is the escape hatch |
| Full curl parity (arbitrary headers, proxies, chunked upload, retries) | "just make it curl" | infinite surface, each flag a support case; auth/proxy policy escapes the tool | auth + content-type are enough; users escalate to curl deliberately |
| Raw output inside the frozen JSON contract | "consistency" | the contract's value is stability; raw responses change with Ignition versions | contract covers curated commands; raw is opt-in instability by definition |

### 2. Curated diagnostics (license, diagnostics bundle, redundancy, GAN)

**What Ignition 8.3 exposes (verified in local 83-api collection, HIGH):**
- `license-status`: Licensing Information, Trial Information. `license-management`: hardware/leased license CRUD (writes). `license-activation`: activate/reactivate/offline flows (writes).
- `redundancy`: Status, Config, Log Events, Force Failover, Re-Sync Configuration, Provider Metrics.
- `gateway-network` (classic GAN): Gateways, Gateway Detail, Live Diagram, Diagnostic Ping, Remote Tag Providers, Task Queue pause/resume/cancel, Toggle Approval, Reset Incoming/Outgoing Connection.
- `agent-management` (8.3 agents): EAM Agents Status/Overview, License Keys, Quarantined agents, Approve/Upgrade/Delete.
- Diagnostics bundle: generate/download + wait (igw-cli already models `wait diagnostics-bundle`; v1.0 has the wait primitive).

**What admins check daily (domain knowledge, HIGH — this is the author's own ops context):** license/trial days remaining, redundancy state (independent/master/backup + backup connection), GAN peers reachable + certificate state, gateway faults in logs, thread diagnostics under load, storage/disk. Cert expiry is the classic silent failure (certificate-management families exist in 83-api).

**Table stakes**

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| `ign license status` (edition, state, expiry, trial days left) | trial expiry + license expiry are day-1 admin checks; ignition-mcp already wraps activation endpoints | LOW | two GETs merged into one shape |
| `ign redundancy status` (state, role, peer, sync state) | degraded redundancy is THE silent SCADA risk | LOW–MED | Status + Provider Metrics merged |
| `ign gan status` / `ign gateways` (connections, state, detail) | GAN health is the daily connectivity check | MED | classic gateway-network family; agent-based status belongs with EAM (below) |
| `ign diagnostics bundle generate/download [--wait]` | support ticket staple; igw-cli parity | MED | async generate + poll + multipart download |

**Differentiators**

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| `ign doctor --deep` (or `ign diagnostics` roll-up) | one command = morning check: license, redundancy, GAN, modules, disk, cert expiry — each PASS/WARN with machine-readable per-check JSON | MED | composes existing v1.0 checks + new reads; agents love one-shot status |
| Cert expiry surfaced in diagnostics | silent-failure class nobody curated | LOW–MED | certificate-management GET; expiry-window thresholds |
| Guarded redundancy re-sync/failover verbs | replaces a webpage trip in an incident | LOW–MED | `--yes` guard; failover also gets a `--confirm-failover` style explicit flag in prompt text |

**Anti-features**

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| License activation/unactivation as first-class writes | "activate from CLI" | wrong activation state can lock a gateway or burn a seat; zero recovery margin via automation | leave behind `ign api call` with the path classifier's warning; revisit only with a real activation runbook |
| GAN task-queue pause/resume/cancel as commands | fleet debugging | pausing the GAN queue wedges cross-gateway ops subtly; rare need | api-call escape hatch |
| Rendering the GAN live diagram in TUI | "the webpage has it" | graph layout in ratatui is a project of its own; data already covered by `gan status` | tabular connections list |

### 3. EAM writes beyond guarded basics

**What write ops exist in Ignition EAM (verified, 83-api `eam-tasks` + `agent-management`, HIGH):**
- Task CRUD: Create, Modify, Rename, Delete (+ Delete multiple), Get config/names, List resources.
- Task lifecycle: **Cancel, Suspend, Resume, Force execution**, Clear retry data, Get retry tasks.
- Task status: Running or Scheduled Tasks, Task History.
- Agent management writes: Approve agent, Delete quarantined agent, Upgrade Agent (+ pre-flight: Retrieve agent info for upgrade, agent modules, projects).
- v1.0 already shipped the guarded basics (task list/history/create per milestone context) — this is the write half.

**How comparable tools do it:** kubectl drain/cordon (guarded, explicit, reversible verbs); `gh workflow run` (explicit execution of a thing defined elsewhere); Terraform's plan→apply split (preview before mutate). The convention: lifecycle verbs are cheap; *execution* verbs and deletes are where guards and previews earn their keep. MEDIUM–HIGH confidence (well-established patterns).

**Table stakes**

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| `ign eam tasks suspend/resume/cancel <task>` | lifecycle verbs are the everyday writes | LOW–MED | 1:1 REST mapping; each behind `--yes`; show affected agent scope in output |
| `ign eam tasks run <task>` (force execution) | "run it now" is the whole point of tasks | MED | returns task id → poll Running/Scheduled + Task History (reuse wait primitives); `--yes` mandatory |
| `ign eam tasks rename/modify/delete` | CRUD parity | LOW–MED | delete behind `--yes`; skip "delete multiple" sugar (see anti-features) |
| Task history/status output shapes | agents poll these | LOW | frozen envelope per E1 |

**Differentiators**

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Blast-radius preview | `run --dry-run` (or default pre-exec summary) prints exactly which agents/resources the task touches before `--yes` | MED | task config GET already has the data; formatting + consistency is the work — Terraform plan feel |
| Agent-group fan-out summary | create/list across agent groups with one clear "N agents affected" line | MED | rides agent-management reads |

**Anti-features**

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| `ign eam agents upgrade` (fleet module push) | "one command upgrades everything" | brickable fleet operation needing pre-flight (version compat, rollback, staging order) — its own milestone's worth of design | ship the pre-flight *reads* (upgrade info, agent modules) and leave the upgrade itself behind api-call; revisit v1.2+ |
| Bulk delete flags for tasks ("delete multiple" endpoint) | convenience | one fat-finger deletes a fleet's schedule; zero legitimate daily use | per-task delete; agents can loop |
| Auto-retry loops around force-execution | "make it reliable" | tasks may be long-running; retry storms on a SCADA fleet | return task id + wait verb; agents decide |

### 4. MCP transport over the existing JSON contract

**How comparable tools do it:** the canonical pattern (HIGH confidence in mechanics — JSON-RPC 2.0 over stdio, newline-delimited messages) is a thin server whose `tools/list` emits one entry per logical operation with a JSON-schema, and whose `tools/call` maps directly onto the existing command layer — returning the CLI's frozen envelope as the tool result content. Wrapping an existing CLI core this way is exactly what ecosystem MCP servers do; the interesting decisions are *which* tools to expose and *how guards translate*, not the transport. (STACK researcher settled: hand-rolled shim, no rmcp — respected here.)

**Table stakes**

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| `initialize` handshake with protocolVersion negotiation + capability listing | protocol requirement; clients refuse without it | LOW | stdio, newline-delimited JSON-RPC |
| `tools/list` with per-tool JSON schema | discovery is the point of MCP | MED | schema generation from the existing clap command definitions keeps one source of truth |
| `tools/call` → command layer (in-process, not subprocess) | agents get the frozen envelope back as result content | MED | reuse the same GatewayApi/action core the CLI calls — architectural invariant from v1.0 TUI work |
| Exit-code → MCP error mapping | CLI exit 2/6/7 etc. must surface as `isError` content with the JSON error envelope | LOW | E3 machine-readable errors become tool-call errors verbatim |
| `ign mcp` as an explicit subcommand | servers are spawned (`command: ign mcp`) by clients like Claude/Cursor | LOW | never a default/daemon behavior (v1.0 anti-feature stance carried forward) |

**Differentiators**

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Curated tool subset (~30–40 tools: reads + guarded writes) | the 37-tool ignition-mcp catalog proved curation is the value; agent context is a budget | LOW–MED | map: v1.0 command tree → tools; write-tools take a `confirm: true` argument replacing `--yes` (the non-interactive guard translation — decide at spec time, document loudly) |
| Server name/version derived from CLI version | one version story for humans and agents | LOW | free |
| Resources (MCP file-like resources) for status snapshots | clients can pin a status read without tool-call ceremony | MED | v1.x stretch; tools suffice day one |

**Anti-features**

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| Exposing all ~100 REST families as generic MCP tools | "completeness" | catalog bloat burns agent context; unsafe paths get automated without judgment — the exact failure ignition-mcp's curation avoided | curated subset + `ign api call` mapped as ONE raw escape-hatch tool (clearly guarded) |
| prompts/sampling/completions MCP capabilities on day one | "full server" | no consumer for them in the primary nvim/Claude flows yet | tools-only capabilities block; add capabilities when a consumer exists |
| Guard-free write tools because "MCP clients are trusted" | agent convenience | an agent auto-approving tool calls is exactly the destructive-ops threat the `--yes` guard exists for | confirm-argument or config-level allowlist per write tool |

### 5. Tag bulk transfer xml/csv

**Ignition's tag export format shapes (domain knowledge, MEDIUM–HIGH — consistent with v1.0 live findings):** JSON is the 8.x-native interchange (v1.0 proved round-trip live); XML is the legacy 7.x format the platform still imports/exports; CSV is a flattened, lossy convenience export. The kindling project exists largely because these custom formats are painful to parse — which is the strongest available evidence that CLI-side re-serialization is the wrong lane. v1.0 deliberately deferred xml/csv to backlog after proving JSON-native; the Designer-diff-adjacent risk is fidelity, not plumbing.

**Table stakes**

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| `ign tags export --format xml|csv` | designers/export pipelines in the wild still ship XML; teams expect format parity with the Designer's export dialog | LOW | **server-byte-faithful passthrough** — download what the gateway produces, never re-serialize |
| `ign tags import --format xml|csv` | importing legacy/CSV sets is a real migration lane | LOW–MED | same native endpoints, correct content-type/multipart form; collision-policy default Abort (v1.0 convention) |
| Format auto-detection on import | files arrive unflagged | LOW | content sniffing (`<?xml`, CSV header) |

**Differentiators**

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Lossy-import report | `ign tags export --format csv --warn-lossy` (or import-time check): parse the payload, diff against the JSON export of the same subtree, print what a round-trip would drop (UDT params, docs, arrays, event scripts…) | MED | the CSV/XML fidelity warning is exactly the trap teams hit; nobody offers it |
| Per-format capability matrix in docs | sets expectations before an ops surprise | LOW | static docs table |

**Anti-features**

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| CLI-side JSON→CSV/XML conversion ("export as anything") | "uniform interface" | re-serializing Ignition's formats is the kindling-shaped rabbit hole and guarantees silent data loss | passthrough only; if conversion is ever needed, delegate to kindling |
| Declaring CSV columns a stable contract | "agents will parse it" | CSV shape is Ignition's, varies across versions; ours-to-break promises invite breakage | agents use JSON (native contract); CSV is for humans/spreadsheets |
| Blessing spreadsheet round-trips as a first-class workflow | "edit in Excel" | cell-embedded JSON blobs + type coercion = classic corruption | allowed but warned loudly (differentiator's lossy report); JSON remains the blessed editing format |

### 6. Tag↔historian binding closure via Designer-diff

**The 05-06 limitation (verified verbatim from v1.0 SUMMARY + README, HIGH):** the bounded spike tried execution scan-class keys, aggregation variations, and `browseHistoricalTags` cross-checks — **no candidate produced data** within budget. Structural query proven; data flow documented as a limitation. The README names the resolution path: **create one history tag by hand in the Designer, `tags config get` it via the CLI, diff the shapes** — find the field(s) the WebDev path isn't setting, set them, prove data flows.

This is a **research-and-fix feature, not a UI surface**: the deliverable is a working `configure a history tag via CLI → data appears in `tags history`` loop, plus whatever route/config changes make it true.

**Table stakes (once resolved)**

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| CLI-configured history tags actually record data | parity with Designer is the bar; v1.0's fixture (InternalHistorian provisioned headlessly) already exists to prove it | MED | the diff oracle makes this deterministic instead of budget-boxed guessing |
| Documented root cause in README + route docs | v1.0 established the honest-limitation pattern; closure must update the doc, not just the code | LOW | one paragraph: missing field(s), why, fix |

**Differentiators**

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| `ign tags history enable <path>` convenience verb (if the fix is a simple field) | one command instead of a config JSON edit | LOW | only if the root cause is a single missing config field — don't build ceremony around a one-line put |

**Anti-features**

| Feature | Why Requested | Why Requested→Problematic | Alternative |
|---------|---------------|---------------------------|-------------|
| Shipping a guessed wire shape without live-data proof | "the diff looks right" | v1.0 already proved guesses burn spike budget; only live rows count | assert-then-prove: shape applied → write → query returns rows → THEN claim parity |
| Expanding scope to historian provider management (historian-config CRUD family) | "while we're in there" | separate surface, zero dependency for the fix | keep in backlog; api-call covers reads |
| Claiming parity until e2e shows rows | milestone pressure | the limitation exists precisely because structural success ≠ data | the e2e gate from 05-06 IS the definition of done |

### 7. TUI theming

**How comparable tools do it:** k9s skins — YAML files of **named UI keys** (`body.fgColor`, `charts.*`, `table.*`…), selected via config/env, community skin packs; lazygit — a small `gui.theme` block of named colors in its config; btop — `.theme` files with ~30 named keys, theme directory, live switching. Convergent pattern: **named semantic palette slots, not per-widget pickers**; a file-based override; ideally hot-swappable. HIGH confidence (very stable, long-lived conventions).

**Table stakes**

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Built-in theme set (default/dark/light + one or two accents) | light-terminals users bounce off an unreadable TUI instantly | LOW | ratatui palette struct; zero new crates (settled) |
| Theme persisted in config | choice survives restarts | LOW | rides existing config plumbing |
| ~15–25 named palette keys (bg, fg, border, selection, table header, status ok/warn/err, bar charts) | matches k9s/btop granularity; enough to restyle, small enough to document | LOW | semantic status colors (alarm=red) must remain **semantic, not themeable away** |

**Differentiators**

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| User theme file (same named keys, TOML) + `ign tui theme ls/use` | k9s-skin-level customization without a code change | LOW | unknown keys warn, don't fail |
| Live theme switch in TUI | btop-style polish | LOW | re-read palette on keybind/command |

**Anti-features**

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|----------------|-------------|
| Arbitrary per-widget color config | "total control" | unmaintainable matrix; every new panel multiplies keys | named semantic slots only |
| Terminal ANSI-16 "compatibility mode" as a separate theme engine | old terminals | ratatui already degrades; a second engine doubles testing | single truecolor-first engine |
| Theme marketplace / pack ecosystem | community enthusiasm | distribution/maintenance burden with no user | document the file format; let users share files informally |

### 8. Configurable polling cadence

**How comparable tools do it:** k9s `refreshRate` (single global, seconds, in config.yaml, default ~2s); btop `update_ms` (global with a hard min clamp); `watch -n <interval>`; htop `delay`. Convergent pattern: **one global default, a per-invocation override, and a sane minimum clamp** — nobody offers per-resource-type cadence matrices. HIGH confidence.

**Table stakes**

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Global default (config) + per-view/command override | k9s/btop/watch convention | LOW | applies to TUI pollers AND `wait`/watch loops (one cadence plumbing, two consumers) |
| Min clamp with warn (e.g. ≥1s) | a gateway is a real server; sub-second polling is self-DoS | LOW | clamp, don't refuse — log/warn once |
| Sensible default (2–5s) | v1.0 shipped some cadence; make it configurable, don't change the feel | LOW | current value becomes the default |

**Differentiators**

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Per-panel cadence in TUI (dashboard fast, tag browser slower) | refresh what you're looking at | LOW–MED | natural extension; keep defaults conservative |
| Backoff on error + pause-on-hidden | don't hammer a gateway that's already struggling; don't poll invisible panels | MED | error backoff is cheap; hidden-panel pause depends on TUI focus model |

**Anti-features**

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|----------------|-------------|
| Sub-second default | "live feel" | SCADA gateways feel latency in polling bursts; WebDev routes are server-side scripts | defaults 2s+, clamp 1s |
| Per-resource-type cadence config matrix | "fine control" | config explosion, zero comparable tool does it, every panel becomes a config surface | global + per-panel |
| Server push / websocket subscriptions | "real-time" | no such endpoint exists on these surfaces; inventing a push layer is a platform, not a flag | polling with cadence control |

### 9. `ign edit` round-trip

**How comparable tools do it:** the gold standard is **kubectl edit** — temp file with explanatory header comments, open `$EDITOR`/`$VISUAL`, on close: content-hash save-detection ("no changes → Edit cancelled"), attempt the update, on validation error **reopen the file with the error injected**, loop until success or explicit abort; conflict detection on concurrent modification. `git commit` contributes: empty content = abort. `gh`/`crontab -e` confirm the temp-file convention is universal. HIGH confidence (canonical, stable patterns).

**Table stakes**

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| `$VISUAL`/`$EDITOR` resolution with fallback | universal convention | LOW | no in-binary editor ever (see anti-features) |
| Temp file + save-detection (content hash) | kubectl semantics; avoids surprise pushes on `:q` | LOW–MED | no-change → clean abort, exit 0 with message |
| Push via existing machinery on save | resource put (zip surgery) / tag config put already exist | MED | this feature is a *loop*, not a new API path |
| Validation/conflict errors reopened in-editor | kubectl's killer UX: fix-and-retry without retyping the command | MED | server 4xx/409 → error text injected as comment at top; loop; explicit abort keyword |
| Guarded push | edit IS a write: `--yes` semantics or an explicit confirm prompt unless non-interactive | LOW | E4 conventions |

**Differentiators**

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| `--decode-scripts` leg | edit the actual `.py` (Flint codec) instead of JSON-embedded script strings — the nvim workflow, on demand, for one resource | MED | ties the codec into the everyday edit path; re-encode on save |
| Diff preview before push | terraform-plan trust: show gateway→edited delta | LOW–MED | reuse diff from cross-gateway compare |
| Works for both project resources and tag configs | one verb, two surfaces users already manipulate | LOW | second call-site of the same loop |

**Anti-features**

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| Built-in TUI text editor | "self-contained" | editors are solved; ratatui text editing is a project; syntax highlighting is table stakes for real work | shell out; $EDITOR is universal |
| Save-on-interval auto-push watcher | "live editing" | accidental pushes + conflict storms; kubectl's loop is user-timed for a reason | push per editor-close with change detection |
| Multi-resource single buffer | "batch editing" | partial-failure semantics on push become incoherent | loop the verb; agents batch fine |

### 10. LSP server feeding ignition-nvim

**Local ground truth (HIGH, read from source):** ignition-nvim already ships and auto-launches a **Python LSP** (`ignition-lsp`, pygls 2.0, stdio): completion for `system.*`/`project.*`/`shared.*` (239+ functions, 14 modules), hover, go-to-def, diagnostics, workspace symbols, project scanning. Its server selection is an **explicit ordered list**: (1) plugin venv `ignition-lsp`, (2) PATH `ignition-lsp`, (3) dev-venv source. The v1.1 Rust LSP (lsp-server 0.10 + lsp-types 0.97 — settled) is therefore **the gateway-data feeder**: the piece the static Python LSP cannot have because it never talks to a gateway.

**What completions/diagnostics make sense from gateway data** (behavior design for this feature):
- *Completions:* live tag paths (from `tags browse`, cached with TTL) in tag-path string contexts; provider names; `project.*`/`shared.*` script function names harvested from the live project's exported script resources; named queries in `system.db.runNamedQuery` call sites; UDT type names; history providers in tag history config; WebDev route names.
- *Diagnostics:* unknown tag path (staleness-caveated), unknown named query, references to deleted/renamed providers — each diagnosable only because the gateway is the source of truth.
- *Hover:* live tag metadata (type, doc, UDT ancestry) and script function signatures from live exports.

**Table stakes**

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| stdio LSP: initialize, textDocument/completion, publishDiagnostics, hover | the nvim client registers a standard client; anything less doesn't attach | MED | lsp-server crate handles framing; scope = gateway data only |
| Offline/graceful degradation | no gateway profile → serve nothing gateway-related, never crash, log once | MED | static-only fallback keeps nvim usable everywhere |
| Never block typing on the network | sync gateway calls inside completion = frozen editor | MED | async prefetch + cached responses; TTL-stamped cache |
| Cache invalidation knobs | stale completions erode trust faster than missing ones | MED | TTL + explicit refresh (command/workspace-config trigger) |

**Differentiators**

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Live tag-path + named-query completions | the single highest-value gateway-only capability — nothing else in the ecosystem does it | MED–HIGH | rides existing browse/export machinery; context detection (which string position is a tag path) is the fiddly part |
| Live hover for tags | "Designer tooltip in the editor" | MED | shares cache with completions |
| `ign lsp` slots as nvim detection candidate #0 | one-line nvim patch (sibling repo); CLI ships the binary | LOW | verified: nvim's ordered list makes this trivial — insert venv-agnostic `ign lsp` first when present |

**Anti-features**

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|----------------|-------------|
| Reimplementing the Python LSP's static knowledge (system.* functions, Jython stubs) | "one server" | v1.0 explicitly deferred to the Python LSP; 239+ function stubs are maintained there; duplication guarantees drift | **composition**: nvim can attach both clients; Rust = gateway truth, Python = statics; document the split |
| Blocking/synchronous gateway reads in completion handlers | simpler code | editor freezes; kills adoption on real projects | prefetch + cache, always |
| Gateway credentials cached to disk by the LSP | latency | secrets at rest in editor cache dirs violates the v1.0 secrets posture (D4) | use profile/env auth in-memory; cache only data, never credentials |
| Diagnostics beyond gateway-truth (Jython lint, style) | "useful linter" | ignition-lint owns this (v1.0 anti-feature, unchanged) | stay in the gateway-data lane |

### 11. Workspace checkout

**Model (local ground truth, HIGH):** ignition-git-module's export model is the reference: project resources land as a file tree in the repo (resources + `tags/` per project), `git.yaml` carries repo/branch/user config, pull-side imports offer **Overwrite/Merge/Abort** collision policies, and `tags_importOnStartup` governs tag restore. The native project export is a zip with per-resource members — v1.0 already does **zip-member surgery** on it (verified live: "no per-resource REST exists on real 8.3 gateways — resource editing rides zip-member surgery"). Workspace checkout = materialize that same zip layout as a directory + a manifest, and reverse the trip. Comparable tools: git worktree/checkout (the naming metaphor users expect), kubectl apply (declarative files → server state), terraform (state drift = manifest diff).

**Table stakes**

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| `ign workspace checkout <project> [dir]` → deterministic file tree | the checkout metaphor demands stable, predictable paths; manifest = terraform-state analog | MED–HIGH | reproduce the native export/zip layout exactly (v1.0-proven), so push = re-assemble changed members; **bijective path mapping** with safe encoding of odd resource names is the core correctness risk |
| `ign workspace status` | which files differ from the gateway — the loop only works with a drift check | MED | manifest hashes vs gateway re-export (server-side truth, not memory) |
| `ign workspace push [path…]` | selective write-back of edited files | MED | rides zip-surgery put; collision policy default Abort (module convention) |
| Manifest at tree root | enables status/diff/push without re-deriving everything | LOW | document it as internal-but-readable |

**Differentiators**

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| `--decode-scripts` checkout | `.py` files alongside JSON — the git-module/nvim editing experience, fully offline-editable, re-encoded on push | MED | shares the Flint codec leg with `ign edit`; this is the feature that makes the workspace the *primary* authoring surface |
| Server-side change detection on push | gateway moved since checkout → refuse/warn (kubectl 409 semantics) | MED | status re-export comparison; prevents silent clobber in multi-editor teams |
| Per-file pull (`ign workspace pull <path>`) | grab one changed resource without full checkout | LOW–MED | subcase of status+push machinery |

**Anti-features**

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| Bidirectional live-sync daemon | "always in sync" | explicitly rejected in v1.0 (no daemon, single binary); sync daemons on SCADA state are conflict farms | explicit checkout/status/push loop |
| Tag values / runtime data in the workspace | "one tree for everything" | tags ride the git-module tag-export lane (`tags/`, `tags_importOnStartup`) — v1.0 established lane separation; mixing values into resources corrupts both | workspace = project **resources** only; tags via existing tag export/import |
| Merge-conflict resolution UI | parity with git-module's Designer conflicts | the module owns Designer-side conflict UX; a TUI merge resolver is a project of its own | detect conflict state, surface it, abort cleanly; humans resolve in git or Designer |
| Live two-way auto-push on save | "like hot reload" | partial-failure + conflict semantics make unattended pushes hazardous on gateways | the `ign edit` loop (explicit, user-timed) |

---

## Feature Dependencies

```
v1.0 base (profiles/auth, JSON+exit codes, --yes guard, export/import,
zip surgery, tag browse/config, Flint codec, TUI command layer, wait primitives)
    └── underpins ALL v1.1 features

[1 api call]     ──requires──> profiles/auth only          (standalone, cheap)
[2 diagnostics]  ──requires──> status/wait patterns; ──enhances──> doctor
[3 EAM writes]   ──requires──> EAM reads (v1.0) + --yes + wait; ──related──> [2] agents status
[4 MCP shim]     ──requires──> entire command layer; ──consumes──> frozen envelope; ──translates──> --yes (confirm field)
[5 xml/csv]      ──requires──> TAGS-09 native export/import (v1.0)
[6 historian]    ──requires──> 05-06 fixture + tags config get + e2e harness (all v1.0)
[7 theming]      ──requires──> TUI v1.0 + config plumbing
[8 polling]      ──requires──> TUI poll loop; ──shared plumbing──> wait loops
[9 ign edit]     ──requires──> resource put + tag config put + Flint codec; ──enhances──> [11]
[10 LSP]         ──requires──> browse/config/export + profiles; ──pairs──> nvim detection order (sibling repo patch)
[11 workspace]   ──requires──> export/import + zip surgery + codec; ──shares codec leg──> [9]

[9 edit] and [11 workspace] share the decode/encode path  (build codec leg once)
[2 diagnostics] and [3 EAM] share agent-management reads   (build agents-status read once)
[4 MCP] should land AFTER the commands it exposes are stable — it is a lens, not a source
```

**Key ordering consequences:**
1. **[1] api call is the cheapest and most unblocking** — land early.
2. **[9]+[11] share the codec leg** — same phase or workspace second.
3. **[10] LSP's hard part is gateway-data cache/context detection** — its nvim integration is a trivial sibling-repo patch (verified), so sequencing risk is internal only.
4. **[4] MCP is a pure lens on the command layer** — schedule after (or with) the v1.1 commands it should expose, never before.
5. **[6] historian is research-shaped and bounded** — small enough to slot early or parallel; e2e gate is the definition of done.
6. **[7]+[8] are pure TUI polish** — zero API surface, safe to parallelize with anything.

## MVP Definition (v1.1 launch set)

### Launch With (v1.1 core)

- [ ] **[1] `ign api call`** — escape hatch for everything uncurated; trivially cheap, disproportionately unblocking (agents + edge endpoints)
- [ ] **[2] diagnostics core**: `license status`, `redundancy status`, `gan status`, bundle generate/download/wait — the daily-check reads
- [ ] **[4] MCP shim** — the agent-facing surface is the project's core value; tools = curated command map, confirm-field guard translation
- [ ] **[10] LSP server** (completion + hover + diagnostics, offline-degraded, cached) + the one-line nvim detection patch
- [ ] **[9] `ign edit`** (resource + tag config, error-reopen loop; `--decode-scripts` if codec leg shared with workspace)
- [ ] **[6] historian closure** — bounded, research-shaped; e2e rows-or-it-didn't-happen

### Add After Validation (v1.1.x)

- [ ] **[3] EAM write verbs** (suspend/resume/cancel/run/rename/modify/delete) — trigger: the read+create base is in daily use; blast-radius preview with it
- [ ] **[11] workspace checkout** (checkout/status/push + manifest) — trigger: `ign edit` loop validated; shares its codec leg
- [ ] **[5] xml/csv transfer** — trigger: a real migration/import need appears (passthrough keeps it cheap whenever it lands)
- [ ] **[7] theming + [8] polling cadence** — trigger: TUI daily-driver adoption; pure polish, any slot

### Future Consideration (v2+)

- [ ] EAM agent fleet upgrade automation — needs its own pre-flight/rollback design (pre-flight *reads* can ship in v1.1.x)
- [ ] MCP resources/sampling capabilities — when a consumer exists
- [ ] Historian provider management (historian-config CRUD) — api-call covers reads meanwhile
- [ ] GAN diagram visualization — webpage owns it

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| [1] api call | HIGH (agents + edge cases) | LOW | **P1** |
| [2] curated diagnostics | HIGH (daily ops) | LOW–MED | **P1** |
| [4] MCP transport | HIGH (core agentic value) | MED | **P1** |
| [6] historian closure | HIGH (closes a shipped limitation) | MED (research) | **P1** |
| [9] ign edit | HIGH (editing loop, nvim pairing) | MED | **P1** |
| [10] LSP server | HIGH (nvim flagship pairing) | MED–HIGH | **P1** |
| [3] EAM writes | MED–HIGH (ops completeness) | MED | P2 |
| [11] workspace checkout | HIGH (authoring surface) | MED–HIGH | P2 |
| [5] xml/csv | MED (migration lane) | LOW–MED | P2 |
| [7] theming | MED (TUI adoption) | LOW | P2 |
| [8] polling cadence | MED (TUI comfort + gateway safety) | LOW | P2 |

## Competitor Feature Analysis

| Feature | Comparable tool pattern | Our approach |
|---------|------------------------|--------------|
| Raw API passthrough | `gh api` (GET default, jq, status exits), `kubectl --raw` | same shape; method-aware `--yes` classification is the SCADA-specific addition |
| Diagnostics | k9s status surfacing, igw-cli diagnostics bundle + wait | merge per-subsystem status into one shape; `doctor --deep` roll-up is our differentiator |
| EAM guarded writes | kubectl drain/cordon, `gh workflow run`, terraform plan→apply | lifecycle verbs 1:1; blast-radius preview before force-run |
| MCP wrapping | canonical thin stdio JSON-RPC servers over existing CLI cores | in-process tools/call into the command layer; frozen envelope as result content; curated subset only |
| Tag formats | kindling exists because Ignition's custom formats are hard to parse | server-byte-faithful passthrough, never re-serialize; lossy-report differentiator |
| Historian binding | — (our own v1.0 spike; no competitor has tag-history CLI at all) | Designer-diff oracle; e2e rows as the done gate |
| Theming | k9s skins (named keys), lazygit theme block, btop .theme live switch | same named-slot model; semantic status colors stay non-themeable |
| Polling | k9s refreshRate, btop update_ms clamp, `watch -n` | global default + per-view override + min clamp |
| Edit loop | kubectl edit (hash save-detect, error-reopen), git commit (empty=abort) | kubectl semantics over existing puts; `--decode-scripts` leg |
| LSP | Python `ignition-lsp` owns statics (verified in nvim source) | Rust LSP = gateway data only; composition, not reimplementation; detection-order slot #0 |
| Workspace checkout | git worktree metaphor + git-module repo layout (Overwrite/Merge/Abort imports, tags_importOnStartup) | native-zip-layout directory + manifest + status/push; resources-only lane |

## Sources

- **Local, HIGH confidence (read directly):**
  - `~/whiskeyhouse/83-api` Bruno collection — endpoint families verified: `eam-tasks` (full lifecycle write set incl. Cancel/Suspend/Resume/Force/Clear-retry), `agent-management` (Approve/Delete-quarantined/Upgrade + pre-flight reads), `license-status`, `license-management`, `license-activation`, `redundancy` (Status/Force-Failover/Re-Sync/Log Events/Provider Metrics), `gateway-network` (Gateways/Diagnostic Ping/Task Queue/Toggle Approval/Reset connections), `certificate-management`, `historian-config`, `call-script`
  - `~/whiskeyhouse/ignition-mcp/ignition_tools_summary.json` — catalog now 42 tools; activation/license endpoints confirmed wrapped
  - `~/whiskeyhouse/ignition-nvim/lua/ignition/lsp.lua` — LSP server detection order (venv → PATH → dev source) read from source; `lsp/README.md` — Python LSP feature set (239+ functions, pygls 2.0)
  - `~/whiskeyhouse/ignition-git-module/readme.md` + `docs/production-mode.md` — git.yaml conventions, Overwrite/Merge/Abort import policies, `tags_importOnStartup`, repo layout
  - `.planning/research/FEATURES-v1.0.md` — v1.0 scope baseline, competitor survey (igw-cli, kindling, igniscope), rejected anti-features carried forward
  - `.planning/PROJECT.md`, `.planning/milestones/v1.0-ROADMAP.md`, `.planning/phases/05-webdev-backend-tag-operations/05-06-{PLAN,SUMMARY}.md`, `README.md` — the 05-06 binding limitation verbatim ("no candidate produced data… Designer-diff follow-up is the resolution path"), JSON-native-only bulk decision, zip-surgery finding
- **Comparable-tool patterns, HIGH–MEDIUM confidence (long-stable ecosystem conventions; kubectl/gh patterns corroborated by v1.0's igw-cli citations):** `gh api` flag surface; `kubectl edit/--raw`; k9s skins + `refreshRate`; btop `.theme` + `update_ms`; lazygit `gui.theme`; `watch -n`; JSON-RPC 2.0 stdio MCP mechanics
- **MEDIUM confidence (training-data domain knowledge, not re-fetched this session):** Ignition tag export CSV/XML fidelity characteristics (lossy CSV, legacy XML) — consistent with kindling's reason to exist; recommend the lossy-report differentiator be validated against real exports at phase planning

---
*Feature research for: ignition-cli v1.1 — gateway CLI/TUI new-feature landscape*
*Researched: 2026-09-04*
