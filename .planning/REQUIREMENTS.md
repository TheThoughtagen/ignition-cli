# Requirements: ignition-cli — v1.1 Agent Surface & IDE Integration

**Defined:** 2026-09-04
**Core Value:** One binary that lets a developer (or an AI agent) fully operate and inspect an Ignition 8.3+ gateway — health, projects, tags, rigs — without opening the gateway webpage or Designer.

## v1.1 Requirements

Requirements for the v1.1 release. Each maps to roadmap phases (continues v1.0 numbering; v1.0 shipped 44 requirements — see `milestones/v1.0-REQUIREMENTS.md`).

### Foundation

- [ ] **CORE-09**: Shared execution core (`ignition-core::Session`) — MCP tools, LSP features, and CLI arms resolve auth/gateway clients identically in-process; no shelling out to `ign`, no second client
- [ ] **CORE-10**: Config schema extension lands in ONE migration — top-level `[ui].theme` key + per-profile `poll_interval_secs`; invalid config degrades to defaults (never kills TUI startup)
- [ ] **CORE-11**: Contract discipline codified before second command lands — Three-Place slug rule (exit-code enum test + README table + prose), OutOfBand taxonomy extended for `mcp`/`lsp`/`edit` with pinned-test update + justification, stdout-purity byte-scan test harness over the real spawned binary

### Agent Surface

- [ ] **EXT-01**: User can call any uncurated gateway REST endpoint via `ign api call --method --path` with body/header control — envelope-wrapped output with gateway-verbatim `data` (documented exception), catch-all 4xx→exit-2 classification with verbatim body, user-supplied auth-pattern headers refused
- [ ] **EXT-02**: User can run the daily gateway check as curated commands — `license status`, `redundancy status`, `gan status`, diagnostics bundle generate/download/wait — live-verified on both rigs (8.3.3 + 8.3.6)
- [ ] **EXT-04**: AI agents can drive `ign` over MCP via `ign mcp serve` (stdio, hand-rolled JSON-RPC 2.0) — curated tool catalog derived from the clap tree (CI parity test), `--yes` translated to a `confirm` tool-call field with refusal envelope as tool result, frozen JSON envelope returned verbatim as tool-result content

### EAM Writes

- [ ] **EAMW-01**: User can suspend an EAM agent with confirmation guard
- [ ] **EAMW-02**: User can resume a suspended EAM agent with confirmation guard
- [ ] **EAMW-03**: User can cancel a pending EAM task execution with confirmation guard
- [ ] **EAMW-04**: User can force-execute a pending EAM task with confirmation guard
- [ ] **EAMW-05**: User can rename and modify EAM agent/task configuration with confirmation guard
- [ ] **EAMW-06**: User can delete an EAM agent or task with confirmation guard
- [ ] **EAMW-07**: User sees a blast-radius preview (target agent/task, controller impact) before any guarded EAM write executes

### Tag Transfer & Historian

- [ ] **TAGS-10**: User can bulk-transfer tags in XML — server-byte-faithful download/upload passthrough (gateway bytes verbatim, never CLI-side re-serialization)
- [ ] **TAGS-11**: User can bulk-transfer tags in CSV — server-byte-faithful passthrough with documented lossy-field behavior
- [ ] **TAGS-12**: User sees a loss-report warning before importing XML/CSV that would drop or coerce tag fields
- [ ] **TAGS-13**: User can view tag↔historian data-flow bindings through `tags config` output on a licensed gateway
- [ ] **TAGS-14**: User can create/update tag↔historian data-flow bindings via the tag write path — spike-gated: Designer-diff determines the wire shape; "documented limitation, now with diff evidence" is a legitimate done state if closure is not achievable

### TUI Polish

- [ ] **TUIX-03**: User can select a named UX theme (monochrome + color palettes) via top-level `[ui].theme` config
- [ ] **TUIX-04**: TUI degrades gracefully across terminal capabilities (truecolor → 256 → 16 → mono) without breaking readability
- [ ] **TUIX-05**: User can configure per-profile polling cadence via `poll_interval_secs` with a hard minimum clamp (sub-second polling refused by config validation)

### IDE Integration

- [ ] **IDE-01**: User can edit a gateway resource locally via `ign edit` — fetch → decode → `$EDITOR` → encode → push round-trip with content-hash no-op detection, fail-closed encode validation, and staleness check before push
- [ ] **IDE-02**: ignition-nvim receives live gateway data through `ign lsp` (stdio LSP server) — tag-path/named-query/provider completions, hover, and diagnostics from cached gateway truth (TTL-stamped, never a blocking network call inside an LSP request); composition with Python `ignition-lsp` which retains statics ownership
- [ ] **IDE-03**: ignition-nvim's LSP detection order includes `ign lsp` as a candidate provider (one-line sibling-repo patch, verified end-to-end)
- [ ] **IDE-04**: User can check out a project's resources to a local directory tree and sync back — `ign workspace checkout/status/push` over the generalized MemberSource diff engine, injective path mapping, manifest three-way compare, `--yes` push guard

## Future Requirements

Deferred beyond v1.1. Tracked but not in current roadmap.

### Extensions

- **EXT-05**: EAM fleet-upgrade automation with its own pre-flight/rollback design
- **EXT-06**: MCP resources/sampling/elicitation surface (escalate to rmcp when a consumer exists)
- **TAGS-15**: Historian provider CRUD
- **TUIX-06**: GAN topology diagram visualization
- **IDE-05**: Watch-mode auto-push editing (requires relaxing the no-daemon constraint)

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| File-watcher watch-mode / auto-push editing | Breaks the no-daemon constraint; `ign edit` + workspace push cover the loop explicitly |
| CLI-side XML/CSV re-serialization of tag exports | The kindling-shaped rabbit hole — passthrough only (TAGS-10/11); re-serialization corrupts fidelity |
| MCP resources/prompts/sampling in the shim | Tools-only stdio keeps the hand-rolled shim ~300-500 lines; rmcp escalation documented for later |
| Hand-written MCP tool catalog | The exact drift class tui_coverage was built to kill — catalog derives from `Cli::command()` + CI test |
| Breaking the frozen JSON envelope (new shapes, renumbered exit codes) | Contract is the product; additive-only slugs, new error surfaces map to existing exit classes |
| Built-in editor UI in TUI | nvim + LSP owns editing; TUI stays a cockpit |
| OpenAPI discovery / codegen from gateway | No stable OpenAPI surface on real 8.3 gateways |
| Tag values in the workspace tree | Values are runtime state, not source; keeps workspace diffable |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| (populated by `/gsd-new-milestone` roadmap step) | | Pending |

**Coverage:**
- v1.1 requirements: 24 total
- Mapped to phases: 0
- Unmapped: 24 ⚠️ (roadmap pending)

---
*Requirements defined: 2026-09-04*
*Last updated: 2026-09-04 after initial definition*
