# Phase 5: WebDev Backend & Tag Operations - Context

**Gathered:** 2026-08-24
**Status:** Ready for planning

<domain>
## Phase Boundary

Ship the CLI's own versioned WebDev routes (tags, tagConfig, alarms, scriptExec, tagHistory), deploy them to a gateway via `ign webdev deploy`/`status`, and operate the complete tag lifecycle: providers, browse, read/write values, config CRUD, UDTs, alarms, history, bulk export/import — reaching the ignition-mcp replacement bar. The `ign script run` *command* remains Phase 7; this phase ships the route it will ride. Also inherited: deciding the fate of the Phase 3 `resource` family defect.

</domain>

<decisions>
## Implementation Decisions

### Tag command grammar & MCP parity (derived — user deferred to conventions)
- Grouped subfamily pattern per established CLI conventions (the `sessions`/`connections`/`logs` family precedent); exact verb naming at planner discretion
- `ignition_tools_summary.json` is the authoritative parity checklist; research resolves the 37-vs-42 divergence against live route behavior
- Agent-stable JSON shapes per the frozen envelope; all-family-keys-always convention inherited (filtered = `[]`, never key-hunt)

### WebDev deploy lifecycle & footprint (derived — user deferred to conventions)
- Routes deploy into a dedicated project (exact name research/planner picks); never pollute user projects; `--project` override at planner discretion
- Version mismatch → refuse with actionable error per roadmap success criterion — no auto-upgrade magic
- Route versioning + handshake contract internals = research/planner territory

### Phase 3 `resource` family fate (user decision)
- Research decides, steered HARD to the native Ignition 8.3 API first: the 575-path openapi extract (committed as `04-*-openapi-8.3.6-phase3-extract.json`) is the evidence base — find real endpoints supporting per-resource operations and plan on them
- Only if the native API cannot support it does research choose among fallbacks: re-point onto Phase 5's own WebDev routes, export/import-machinery round-trip, or drop the family until Phase 7's decode/encode arrives
- Whichever path wins, the e2e witness approach re-points with it (current resource-route witnesses are broken by the defect)

### scriptExec security posture (user decision — LOCKED)
- The scriptExec route MUST carry its own auth mechanism — it is never deployed wide-open relying solely on gateway session auth. Anyone with gateway access must not be able to invoke arbitrary script execution by mere route presence
- Exact mechanism (shared secret/token, config gate, permission probe) = the flagged script-exec security spike → research proposes, planner locks
- Deploy-default posture (skip-unless-requested vs always-deploy) = planner/research recommendation; the auth requirement holds in every posture
- `ign script run` command surfaces in Phase 7 (roadmap-sequenced, not deferred)

### Claude's Discretion
- Tree/table rendering for tag browse; quality/timestamp display conventions
- Route-internal versioning scheme and handshake shape
- Bulk transfer format details (json/xml/csv beyond roadmap minimum), collision-policy plumbing reusing Phase 3 conventions
- Alarm panel/table output shapes

</decisions>

<specifics>
## Specific Ideas

- ignition-mcp is the replacement bar — parity checklist matters for porting existing agents/scripts
- "def need an auth mechanism" on scriptExec — treat as a hard requirement, not a preference

</specifics>

<deferred>
## Deferred Ideas

- `webdev undeploy`/teardown command — not in roadmap success criteria; candidate for a later phase or backlog
- Live tag `watch` streaming command — Phase 6 TUI owns live-watch UX; CLI-side watch not roadmap-scoped here

</deferred>

---

*Phase: 05-webdev-backend-tag-operations*
*Context gathered: 2026-08-24*
