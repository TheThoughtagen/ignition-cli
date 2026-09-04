# Project Research Summary

**Project:** ignition-cli — v1.1 "Agent Surface & IDE Integration" milestone
**Domain:** Rust CLI/TUI cockpit for Ignition 8.3+ SCADA gateways — 11 new features layered onto a contract-frozen, live-verified v1.0 (3-crate workspace, ~73,900 lines, lean-tree discipline)
**Researched:** 2026-09-04
**Confidence:** HIGH overall (stack versions verified live against crates.io same-day; every architecture integration point verified by reading actual source; pitfalls verified against code/CI/tests; MEDIUM pockets flagged below)

> v1.0 scope baseline lives at `.planning/research/FEATURES-v1.0.md` — reference only, not repeated here.

## Executive Summary

ignition-cli v1.1 is a **subsequent-milestone feature wave on a frozen contract**, and all four research streams converge on the same shape: this is fundamentally *additive integration work inside a single proven binary*, not new-system design. Zero new crates are created; at most **4 new direct dependencies** enter the tree (`quick-xml` 0.42, `csv` 1.4, `lsp-server` 0.10, `lsp-types` 0.97), everything else rides existing machinery (reqwest, zip, tempfile, ratatui 0.30, toml/serde). The headline stack decision is **EXT-04 MCP as a hand-rolled JSON-RPC 2.0 stdio shim (~300–500 lines on serde_json + tokio), NOT the official rmcp SDK** — rmcp's server profile transitively drags in `chrono` (on this project's own v1.0 reject list), `schemars`, `uuid`, and friends for capabilities a tools-only shim never uses; rmcp is documented as the sanctioned escalation path if the MCP surface ever grows beyond tools-only stdio. LSP picks rust-analyzer's `lsp-server` scaffold (tower-lsp is dead — last release 2023-08; the community fork depends on a 0.0.x types crate; async-lsp is pinned two majors behind).

Architecturally, the 11 features classify cleanly: two are **new runtime modes** (MCP and LSP as hidden clap subcommands running their own stdio loops, exactly the way `ign tui` already runs its loop), and nine are **additive growth** of the existing action/client/CLI trees. The single structural addition v1.1 requires is **promoting the binary-private `resolve_gateway_api` into a shared `ignition-core::Session` command-execution core** so MCP tools, LSP features, and CLI arms resolve auth/clients identically and invoke `actions::*` in-process (never shelling out to `ign`, never building a second client). Two cross-cutting invariants are non-negotiable and CI-testable: the **stdout-purity rule** (protocol modes own the entire stdout pipe — one stray `println!` corrupts the JSON-RPC stream) and the **tui_coverage parity walk** (every new invocable command needs a registry row or a sanctioned OutOfBand entry; the pinned OutOfBand assertion must be deliberately updated, never casually).

The dominant risk class is **mutation of frozen contracts under feature pressure**: ~30 snapbox goldens pin exact JSON shapes and exit codes, and the reflex to regenerate failing goldens silently breaks every downstream agent. Mitigations are process, not tooling: additive-only contract rule, golden regeneration as a review event, the Three-Place slug rule, and gate-first live verification on **both** 8.3.3 and 8.3.6 rigs (the v1.0 05-06 lesson: wiremock-green-but-dead-live is the most expensive failure mode this project knows). The build order follows directly from the dependency graph: **contract/config foundations first, independent command families in parallel, composite engine work (workspace/historian/edit) next, and the MCP/LSP transports last** — because they are pure lenses over the command surface and would force protocol churn if landed early.

## Key Findings

### Recommended Stack

Full detail: [STACK.md](STACK.md). The entire v1.1 delta is four crates; the MCP decision is the interesting one.

**Core technologies:**
- `quick-xml` 0.42 (`["serialize"]`) → `ignition-core` — tag-provider bulk-transfer XML read+write; the consensus Rust XML library (398M downloads, MSRV 1.86 ✓); expect hand-rolled Event-loop code for irregular tag-XML corners rather than forcing full serde-derive coverage
- `csv` 1.4 → `ignition-core` — bulk-transfer CSV; BurntSushi canonical crate, serde row-mapping fits the model exactly
- `lsp-server` 0.10 + `lsp-types` 0.97 → `ignition-cli` — rust-analyzer's stdio scaffold for `ign lsp`; sync crossbeam design means the dispatch loop is ours (gateway calls run under a small in-process tokio runtime); LSP 3.17-era types cover everything ignition-nvim needs
- **MCP: zero new crates** — hand-rolled newline-delimited JSON-RPC 2.0 over stdio (serde_json + tokio, already in graph). Two make-or-break rules: stdout purity (spec: MUST NOT write non-MCP output to stdout) and silent-drop of unknown `$/` notifications. rmcp 3.2 escalation path documented for if/when prompts/resources/elicitation/HTTP are ever needed
- Rejected explicitly: rmcp (for now — chrono non-optional), tower-lsp (dead), tower-lsp-server (0.0.x types dep), async-lsp (^0.95 pin), `notify` (no file watching by design), all ratatui theme crates (micro-projects ≤2.5k downloads), `chrono` (standing v1.0 rejection; `jiff` is the sanctioned option if date math ever appears)

**Version requirements:** workspace floor `rust-version = 1.88`, edition 2024 — all picks verified compatible same-day against crates.io.

### Expected Features

Full detail: [FEATURES.md](FEATURES.md). Eleven features, all P1-vs-P2 ranked against gh/kubectl/k9s/btop conventions and local ground truth (83-api Bruno collection, ignition-mcp catalog, ignition-nvim source, ignition-git-module docs).

**Must have (v1.1 launch set):**
- **[1] `ign api call`** — gh-api-shaped raw REST escape hatch; cheapest and most unblocking feature in the milestone
- **[2] Curated diagnostics** — `license status`, `redundancy status`, `gan status`, bundle generate/download/wait (the daily morning-check reads)
- **[4] MCP transport** — initialize/tools-list/tools-call over stdio, **curated tool subset (~30–40) derived from the clap tree**, `--yes` translated to a `confirm` field for write tools, frozen envelope returned verbatim as tool-result content
- **[6] Historian binding closure** — via the documented Designer-diff oracle (create one history tag by hand in Designer, `tags config get` it, diff shapes); e2e data-rows is the only acceptable done gate
- **[9] `ign edit`** — kubectl-edit loop ($EDITOR, content-hash save-detect, error-reopen) over existing put machinery; `--decode-scripts` leg shares the Flint codec with workspace
- **[10] LSP server** — gateway-data feeder to ignition-nvim: live tag-path/named-query completions, hover, diagnostics; **composition with the existing Python `ignition-lsp` (which owns all static knowledge), never replacement**; nvim detection-order slot #0 is a verified one-line sibling-repo patch

**Should have (P2 / v1.1.x):**
- **[3] EAM writes** — suspend/resume/cancel/force/rename/modify/delete behind `--yes`, with blast-radius preview as the differentiator
- **[11] Workspace checkout** — checkout/status/push over a generalized `MemberSource` diff engine; manifest + three-way compare; `--decode-scripts` checkout is the feature that makes the workspace the primary authoring surface
- **[5] Tag xml/csv** — **server-byte-faithful passthrough only** (download what the gateway produces, never CLI-side re-serialization — the kindling-shaped rabbit hole); lossy-import warning report is the differentiator
- **[7] Theming + [8] polling cadence** — k9s/btop-convergent: named semantic palette slots (~15–25 keys), `[ui].theme` top-level config; per-profile `poll_interval_secs` with hard min clamp (a gateway is a real server — sub-second polling is self-DoS)

**Defer (v2+):** EAM fleet-upgrade automation (needs own pre-flight/rollback design), MCP resources/sampling (when a consumer exists), historian provider CRUD, GAN diagram visualization. Standing anti-features carried forward: no daemon, no watch-mode, no OpenAPI discovery, no built-in editor, no tag values in the workspace tree.

### Architecture Approach

Full detail: [ARCHITECTURE.md](ARCHITECTURE.md). Every integration point was verified by reading actual source; the v1.0 locks (envelope `{ok, profile, data}`, exit codes 0–7 in a single mapping site, core-never-prints, GatewayApi one-coarse-method-per-capability in one impl block, tui_coverage bidirectional walk, WebDev bundle version-lock at 1.1.0) all stand and v1.1 must respect them.

**Major components:**
1. **`ignition-core::Session` execute core** (the ONE structural addition) — promotes `resolve_gateway_api` out of `main.rs`; MCP/LSP/CLI all resolve identically and call `actions::*` in-process; protocol layers are translators, never second clients
2. **Hidden-subcommand runtime modes** — `ign mcp` and `ign lsp` as `#[command(hide = true)]` stdio loops branching **before** envelope/render resolution (the `ign tui` / `TuiExited` precedent); OutOfBand registry grows from `["completions"]` to include `mcp`, `lsp`, `edit` with per-entry justification
3. **`MemberSource` diff-engine generalization** — extract "gateway zip OR local fs tree → normalized member list" (~100–200 line refactor of `project_diff`/`scripts_codec` seams); workspace checkout = the diff/sync engine run gateway→fs direction for the first time
4. **Edge format conversion** — xml/csv convert at the action edge against the existing normalized JSON interchange; the gateway route contract never changes and the v1.0 "planner lock" narrows to the wire layer only
5. **Config surface, once** — `[ui].theme` top-level + per-profile `poll_interval_secs` land together (ONE goldens migration); TUI workers already take `period: Duration` args — only spawn sites hardcode constants

**Key anti-patterns to encode in plans:** MCP/LSP building their own reqwest client (five locks re-implemented badly); shelling out to `ign` per tool call; letting protocol modes touch `render_ok`; per-profile theme / global cadence (cross-placed preferences); extending the Python WebDev bundle for anything solvable in Rust (every bundle change is a version-locked deploy event).

### Critical Pitfalls

Full detail: [PITFALLS.md](PITFALLS.md) (17 pitfalls; v1.0 carry-forwards at commit `9d2cc32` remain binding). Top five by blast radius:

1. **Contract mutation via golden-regeneration reflex** — ~30 goldens pin exact shapes; a failing golden regenerated under pressure ships silent breakage to every downstream agent. Avoid: additive-only rule in every PLAN, golden regen = review event (diff in PR description), Three-Place slug rule (enumerated test + README table + prose), new slugs join existing exit buckets — never renumber
2. **Fighting tui_coverage** — every new CLI node fails CI without a registry row; casual OutOfBand erosion or `hide = true` quietly kills the parity invariant. Avoid: decide the Screen-vs-OutOfBand taxonomy in the FIRST CLI-surface phase; update the pinned `out_of_band_rows_are_exactly_…` test once, in the same PR, with justification
3. **Stdout purity when protocol modes share the dispatch chassis** — one stray print corrupts the JSON-RPC stream and looks like a server bug. Avoid: protocol modes branch before render (structural, not convention); byte-scan integration test over the real spawned binary that fails on a single stray byte; `deny(clippy::print_stdout)` in protocol modules; **one shared pattern proven in the MCP phase and reused by LSP** — do not mix newline-delimited (MCP) and Content-Length (LSP) framings
4. **EXT-01 mis-classification: unclassified 4xx → exit-1 "internal error" storm** — the live-proven v1.0 lesson; a passthrough whose whole job is arbitrary URLs will hit this constantly. Avoid: the catch-all classifier is the FIRST task before the happy path (4xx → exit-2 class with verbatim body, 5xx → gateway class); content-type sniffing (HTML → auth class, never serde-panic); binary responses require `-o FILE`; refuse user-supplied auth-pattern headers (redaction extends to them)
5. **Live-gate erosion (wiremock-green-but-dead-live)** — v1.1 is write-heavy against real infrastructure including the WHK production controller. Avoid: gate-first not gate-last, at least one env-gated live gate per write feature recorded during its phase; **both rigs (8.3.3 + 8.3.6) for endpoint-sensitive features** (diagnostics, historian, EAM); EXT-01's live gate itself must be designed so passthrough can't nuke the rig (read-only method matrix)

Milestone-specific entries worth flagging to the roadmapper: the **MCP tool catalog MUST derive from `Cli::command()`** with a CI parity test (a hand-written catalog is the exact drift class tui_coverage was built to kill); **lsp-server 0.10's sync dispatch loop freezes** if gateway calls run inline (cache + TTL + bounded waits, completions <50ms from cache, never a synchronous round-trip in a request handler); **historian binding is spike-first** — no CLI surface commits until the Designer diff lands a wire shape on a licensed rig, and "documented limitation, now with diff evidence" remains a legitimate outcome.

## Implications for Roadmap

Based on combined research, suggested phase structure (consensus across all four researchers; 4 layers, with layer 2 parallelizable — the roadmapper may split it):

### Phase 1: Foundations — Execute Core + Config Schema + Contract Discipline
**Rationale:** Everything downstream depends on the Session seam; the config surface must change exactly once (one goldens migration, not two); and the contract-discipline rituals must be codified *before* the second command needs them (Pitfalls 1–2: "the classification decision must precede the second command that needs it").
**Delivers:** `Session::resolve` + in-process dispatch seam in `ignition-core`; config schema extension (`[ui].theme` top-level + per-profile `poll_interval_secs`, both in ONE migration); TUI worker spawn-site parameterization; codified Three-Place slug rule and Screen-vs-OutOfBand classification taxonomy written into `routes.rs` comments; stdout-purity byte-scan test harness scaffolded (proven by the first protocol mode)
**Addresses:** infrastructure for all 11 features; TUIX-01 config plumbing; theming config plumbing
**Avoids:** Pitfalls 1, 2, 14 (config-typo-kills-TUI — degrade-to-default validation established here)

### Phase 2: Independent Command Families (parallelizable wave)
**Rationale:** These features touch disjoint surfaces and depend only on Phase 1; landing them before transports means the MCP tool catalog mirrors a stable command tree (PITFALLS ordering rationale #2: tag transfer carries the possible route-bundle bump and should land before MCP).
**Delivers:** `ign api call` (catch-all classifier FIRST, then happy path); curated diagnostics (license/redundancy/GAN/bundle, both-rig live gates); EAM write verbs (guard-ladder extension, composition via `composed_settings` seam); tag xml/csv passthrough (quick-xml + csv, server-byte-faithful, loss-report differentiator); TUI theming rendering (**tokenize first** — style-tokens module as the only home of `Color::` literals, CI grep enforcing it — then named palettes) + cadence wiring
**Uses:** all four new crates; `GatewayApi` coarse-method extension; existing `require_confirmation` guard precedent
**Avoids:** Pitfalls 6–9 (EXT-01 classifier/injection, EXT-02 point-release variance, EXT-03 wire-shape recomposition), 11 (export-honest/import-guarded fidelity), 13 (tokenization-first), 4 (route-bump = one atomic commit, both-direction drift tests, if xml/csv needs routes at all)

### Phase 3: Composite / Engine Work
**Rationale:** Workspace checkout generalizes `project_diff` into `MemberSource` — and both historian binding and `ign edit` consume that same engine and codec leg; hardening decode/encode at workspace scale de-risks edit (ARCHITECTURE: workspace before edit).
**Delivers:** `ign workspace checkout/status/push` (injective path mapping + hostile-name property tests as plan-01 deliverables, manifest three-way compare, `--yes` push ladder); tag↔historian binding closure (**spike-first**: plan 01 is the time-boxed Designer-diff on both rigs; CLI surface only after a wire shape lands); `ign edit` round-trip (editor seam trait + archetype fixtures, content-hash no-op, fail-closed encode, staleness check before push)
**Addresses:** features [11], [6], [9]
**Avoids:** Pitfalls 17 (bijection/manifest), 12 (guess-shapes — the roadmap must hold historian's done-definition loose pending the spike), 15 (adversarial $EDITOR)

### Phase 4: Transports — MCP then LSP
**Rationale:** MCP and LSP are pure lenses over the now-complete command surface (FEATURES: "MCP should land AFTER the commands it exposes are stable — it is a lens, not a source"); MCP lands first to prove the protocol-mode pattern (stdout purity, scripted-client harness, coherent version triple in handshakes) once, and LSP reuses it verbatim. LSP is deliberately last: most novel failure surface, and its diagnostics consume Phase 2's lint and diagnostics families.
**Delivers:** `ign mcp serve` (hand-rolled JSON-RPC shim; catalog derived from `Cli::command()` + CI parity test; `--yes` → `confirm`-field guard translation with the refusal envelope AS the tool result — no bypass flags; worker-task gateway calls so `ping` never starves; exit on stdin EOF); `ign lsp` (lsp-server scaffold; honest capabilities, full-text sync default; cache + TTL + bounded waits; `-32002` pre-init; `ign lsp --check` doctor); OutOfBand rows for `mcp`/`lsp`/`edit` finalized with the pinned-test update
**Addresses:** features [4], [10] + the nvim one-line detection patch (sibling repo)
**Avoids:** Pitfalls 3, 10 (lifecycle/framing/catalog-drift/blocked-loop), 16 (state machine, latency)

### Phase Ordering Rationale

- **Dependency-honoring:** Session core is a prerequisite for both transports; config schema is one migration shared by theming + cadence; MemberSource is shared by workspace, historian, and edit; transports wrap everything.
- **Contract safety:** every command family lands before the tool catalog that mirrors it, so the MCP catalog is derived from a stable clap tree and structurally cannot drift.
- **Risk sequencing:** the cheapest, most unblocking feature (`api call`) and the daily-check reads land early; the research-shaped feature (historian) gets its spike early enough in its phase to fall back honestly; the most novel failure surface (LSP) lands last, reusing proven patterns.
- **CI discipline:** each phase's PLAN bakes in its pitfall verifications (per the PITFALLS pitfall-to-phase mapping); the exit-code enumeration test stays green throughout — research found NO new exit codes are needed; all new error surfaces map onto existing classes additively.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 2 (tag xml/csv slice):** pull a REAL multi-level UDT export and decide quick-xml serde-derive vs hand-rolled Event-loop before writing code (STACK open question #1 — wiremock fixtures cannot provide this); validate the loss-report design against real exports
- **Phase 2 (diagnostics slice):** per-endpoint wire-shape verification against BOTH live rigs — 8.3.x point-release variance is the documented failure mode; version-tolerant parsing (deny_unknown_fields OFF, optional fields explicit)
- **Phase 3 (historian slice):** mandatory spike research — 05-06 Designer-diff re-read, licensed-Historian rig access confirmed BEFORE the phase starts, both-rig diff; roadmap holds the done-definition loose (closure OR documented-limitation-with-diff-evidence are both acceptable outcomes)
- **Phase 4 (LSP slice):** verify the sync-loop + in-process tokio `block_on` pattern under the existing tracing setup (STACK open question #3); scripted-client harness design

Phases with standard patterns (skip research-phase):
- **Phase 1:** pure refactor of source-verified code (Session extraction, config keys, worker parameterization) — HIGH confidence, no unknowns
- **Phase 2 (EXT-01/EXT-03 slices):** established v1.0 patterns extended (guard ladder, classify arms, coarse-method trait); local 83-api collection provides ground truth
- **Phase 4 (MCP slice):** transport decision settled by stack research; spec facts verified from modelcontextprotocol.io; the protocolVersion pin ("2025-06-18") is an implementation-time live smoke test, not research

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Every version/deps/MSRV figure pulled live from crates.io API on 2026-09-04; rmcp feature graph read from GitHub `main` manifest; MCP spec facts from modelcontextprotocol.io. MEDIUM on two judgment calls: hand-roll-vs-rmcp verdict (facts HIGH, verdict is engineering judgment — mitigated by documented escalation path + Python-SDK conformance oracle) and degree of Ignition tag-XML resistance to serde derive |
| Features | HIGH | Endpoint surface verified in local 83-api collection, ignition-mcp catalog (42 tools), nvim LSP source, git-module docs; v1.0 limitations read verbatim from shipped SUMMARY/README. MEDIUM on Ignition tag XML/CSV fidelity characteristics (training-data only — validate loss-report against real exports at phase planning) |
| Architecture | HIGH | Every integration point verified by reading actual source (dispatch chassis, classify.rs, tui_coverage walk, config known-keys, worker period args, scripts_codec encode/decode). Library-pick MEDIUMs in this file were subsequently resolved by STACK's same-day crates.io verification — where the two documents differ, STACK.md's crate-level verdicts govern (hand-rolled MCP shim; lsp-server 0.10 over tower-lsp-server/async-lsp) |
| Pitfalls | HIGH | System-internal pitfalls verified against actual code/CI/tests/phase artifacts; protocol claims (MCP stdio framing, lsp-server sync dispatch) verified against primary sources 2026-09-04. MEDIUM on domain behaviors (editor archetypes, Excel corruption, point-release drift beyond the two live rigs) — each backed by a testable prevention, so the phase verification, not the claim, is the safety net |

**Overall confidence:** HIGH — this is the strongest research posture possible for a subsequent milestone: the system is source-verified, the prior milestone's live lessons are documented in-repo, and the open questions are few, specific, and phase-researchable.

### Gaps to Address

- **Ignition tag XML shape vs serde derive:** decide derive-vs-Event-loop after sampling a real multi-level UDT export (Phase 2 tag slice, first task)
- **Historian binding wire shape:** completely unknown pending the Designer-diff spike; requires confirmed licensed-Historian rig access before Phase 3 planning; both closure and documented-limitation-fallback are legitimate done states
- **EXT-02 endpoint wire shapes:** license/redundancy/GAN/bundle shapes vary across 8.3.x point releases — both-rig live verification is a phase success criterion, not a stretch goal
- **MCP protocolVersion pin:** confirm "2025-06-18" acceptance across Claude Code/Claude Desktop at implementation time (live smoke, cheap)
- **LSP under tracing:** verify gateway calls via in-process tokio `block_on` coexist with the existing tracing-subscriber in a sync dispatch loop (STACK open question #3)
- **Contract documented-exception for EXT-01:** write the "api call `data` is gateway-verbatim by design" exception INTO the contract docs during Phase 2 so the normalization instinct never "fixes" passthrough later

## Sources

### Primary (HIGH confidence)
- crates.io API (2026-09-04, same-day): rmcp, tower-lsp, tower-lsp-server, ls-types, async-lsp, lsp-server, lsp-types, quick-xml, csv, jiff, notify, schemars, ratatui-theme search
- rmcp 3.2.0 `Cargo.toml` — github.com/modelcontextprotocol/rust-sdk (`main`): feature graph, chrono non-optionality, MSRV 1.88
- modelcontextprotocol.io spec 2025-06-18, Basic/Transports: stdio framing, stdout purity, version negotiation
- docs.rs/lsp-server/0.10.0: Connection/IoThreads/sync dispatch-loop model; rust-lang/rust-analyzer ownership
- Context7: `/websites/rs_rmcp_rmcp` (ServerHandler/stdio API), `/tafia/quick-xml` (serde bridge)
- **Local system source (read directly):** workspace `Cargo.toml`; `ignition-cli/src/{main,cli}.rs`; `ignition-cli/tests/tui_coverage.rs`; `ignition-core/src/client/{mod,classify}.rs`; `ignition-core/src/{output,error,config,poll}.rs`; `ignition-core/src/actions/{mod,tags,projects,eam,resources,lint}.rs`; `ignition-core/src/{scripts_codec,webdev,rig/compose}.rs`; `ignition-tui/src/{lib,context,workers/*}.rs`
- **Local milestone artifacts:** 05-06 PLAN/SUMMARY (historian spike outcome + Designer-diff path, live-gate discipline); 07-VERIFICATION-GAPS (guard ladder, Two-Place exit rule, route 1.1.0 lockstep, stale-binary evidence); `debug/eam-create-422.md`; v1.0 PITFALLS @ `9d2cc32`; v1.0 ROADMAP/REQUIREMENTS
- **Local ground-truth repos:** `~/whiskeyhouse/83-api` (Bruno collection — full EAM/license/redundancy/GAN/cert endpoint families); `~/whiskeyhouse/ignition-mcp` (42-tool catalog); `~/whiskeyhouse/ignition-nvim` (LSP detection order, Python ignition-lsp feature set); `~/whiskeyhouse/ignition-git-module` (repo layout, Overwrite/Merge/Abort, `tags_importOnStartup`)

### Secondary (MEDIUM confidence)
- Comparable-tool conventions (long-stable, corroborated by v1.0's igw-cli citations): `gh api`, `kubectl edit/--raw`, k9s skins + `refreshRate`, btop `.theme` + `update_ms`, lazygit `gui.theme`, `watch -n`, kubectl-edit loop semantics
- Ignition tag XML/CSV fidelity characteristics (lossy CSV, legacy XML) — training-data, consistent with kindling's reason to exist; validate against real exports
- Editor-archetype behaviors (vscode `--wait`, emacs daemon), Excel round-trip corruption modes, nvim LSP client specifics

### Tertiary (LOW confidence)
- 8.3.x point-release endpoint drift beyond the two live-verified rigs — bounded by both-rig gating and honest-degradation design, not by prediction

---
*Research completed: 2026-09-04*
*Ready for roadmap: yes*
