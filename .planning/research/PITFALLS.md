# Pitfalls Research — ignition-cli v1.1

**Project:** ignition-cli — Rust CLI + ratatui TUI cockpit for Ignition 8.3+ gateways
**Milestone:** v1.1 Agent Surface & IDE Integration (subsequent milestone — adding features to a contract-frozen, live-verified system)
**Researched:** 2026-09-04
**Overall confidence:** HIGH (system-internal pitfalls verified against the actual code/CI/tests; protocol claims verified against primary sources today; gateway-side specifics carry MEDIUM flags inline)

**Mode note:** This is a *subsequent-milestone* pitfalls doc. The v1.0 pitfalls research (gateway API, TUI, docker/rig, agentic CLI foundations) remains valid and lives at commit `9d2cc32` (`.planning/research/PITFALLS.md` as of 2026-08-20). The still-operational carry-forwards are listed at the bottom. This file focuses on the v1.1 risk surfaces: **what breaks when you add features to a system whose output contract, exit taxonomy, TUI↔CLI parity, and route bundle are frozen and CI-enforced.**

---

## Critical Pitfalls

### Pitfall 1: Contract mutation via golden-churn — the "just regenerate the snapshot" reflex

**What goes wrong:**
Every v1.1 feature lands in a repo where ~30 snapbox golden files pin exact JSON shapes, exit codes, and help text. Under deadline pressure, a failing golden gets regenerated (`SNAPSHOTS=overwrite cargo test`) and the diff is skimmed. A field rename, a reordered enum, a changed error message slug, or a renumbered exit code ships. Agents depending on the frozen envelope break silently — they don't file bugs, they hallucinate.

**Why it happens:**
Goldens fail *loudly* for harmless reasons (trailing whitespace — v1.0 hit exactly this with the aligned-table renderer, fixed to ride unpadded) and *silently* for harmful ones. The failure mode looks identical in CI either way, so "make it green" wins over "review the diff."

**How to avoid:**
- Additive-only rule, stated in every v1.1 PLAN's success criteria: new fields OK, new slugs OK, **renames/reorders/renumbers never** without a deliberate contract-version decision.
- The Three-Place rule for new error slugs (v1.0 precedent, enforced by `exit_code_mapping_enumerated` in `error.rs`): (1) the enumerated test in `error.rs`, (2) the README exit table, (3) README prose. New slugs **join the existing exit bucket** (v1.0 added `eam_task_in_flight` and `provider_root_unsupported` to the exit-6 operation/denial bucket — neither renumbered anything).
- Golden regeneration is a review event: the PR description must carry the golden diff, and any diff touching an *existing* line of an *existing* golden needs explicit justification, not just a green check.
- EXT-01's envelope deserves an explicit documented exception *inside* the contract docs: `api call`'s `data` is gateway-verbatim by design (the one sanctioned passthrough) — write that down so the normalization instinct doesn't get applied to it later, breaking passthrough users instead.

**Warning signs:**
- A PR diff that changes existing lines (not appends) inside `tests/contract_*.rs` goldens.
- A new `CoreError` variant whose `code()`/`exit_code()`/`hint()` arms are unwritten (the enumerated test will catch — let it, don't `#[ignore]` it).
- Any commit message containing "regen" or "fix goldens" without a shape rationale.

**Phase to address:** The first phase that adds a slug or output shape (almost certainly the EXT-01/EXT-02 phase) — bake the review ritual into its PLAN; every later phase inherits it.

---

### Pitfall 2: Fighting the tui_coverage walk instead of designing for it

**What goes wrong:**
v1.1 adds roughly a dozen new row-requiring clap nodes (`api call`, the diagnostics verbs, EAM write verbs, tags xml/csv verbs, `edit`, `lsp`, `mcp`, `checkout`…). The CI-enforced bidirectional walk (`crates/ignition-cli/tests/tui_coverage.rs`) will refuse every one of them until each has a `routes()` row — or a sanctioned `Mapping::OutOfBand` entry. Two failure directions: (a) someone treats the red CI as an obstacle and adds OutOfBand rows casually, eroding the parity invariant ("full cockpit" quietly stops being true); (b) someone hides a command from the walk (`hide = true` in clap) so coverage never sees it. Additionally, the pinned test `out_of_band_rows_are_exactly_the_completions_leaf` asserts the OutOfBand set is *exactly* `["completions"]` — protocol and interactive modes **cannot** reuse the mechanism without a deliberate, documented change to that pinned assertion.

**Why it happens:**
The walk is a designed inconvenience. It exists to force a decision per command; decisions are expensive, so people look for a shortcut.

**How to avoid:**
- Decide the classification taxonomy up front, in the first v1.1 phase that ships CLI surface, and write it into `routes.rs` comments next to the existing four-exception state list:
  - **Screen-mappable data verbs** (EXT-02 diagnostics, EAM writes, tags xml/csv): get real Screen rows. No exceptions. These are ordinary data screens.
  - **Interactive/protocol modes** (`edit` shells out to `$EDITOR`, `lsp`, `mcp` are long-running protocol processes): OutOfBand rows, and the pinned `out_of_band_rows_are_exactly_…` test gets updated **in the same PR** with a comment justifying each addition.
  - Never `hide = true`; add a CI grep or clippy deny if tempting.
- The `edit` round-trip can still be TUI-reachable later (a Screen row that invokes the same action with an in-TUI diff/confirm step) — but ship the OutOfBand row first and upgrade deliberately, not silently.

**Warning signs:**
- `every_row_requiring_cli_node_is_mapped_and_no_orphans` red on any v1.1 branch.
- The OutOfBand set growing by more than the three justified entries (`edit`, `lsp`, `mcp`) across the whole milestone.
- Any `hide`/`hide(true)` in `cli.rs`.

**Phase to address:** First v1.1 CLI-surface phase (EXT-01/EXT-02). The classification decision must precede the second command that needs it.

---

### Pitfall 3: Stdout purity violation when protocol modes share the dispatch chassis

**What goes wrong:**
MCP over stdio is newline-delimited JSON-RPC; the spec is blunt: the server **MUST NOT write anything to stdout that is not a valid MCP message** (verified: MCP spec 2025-06-18, Transports). LSP over stdio is Content-Length-framed — equally brittle. The current chassis (`main.rs`) routes diagnostics to stderr and envelopes to stdout — correct for CLI mode — but `render_ok`/`render_error` print to stdout and clap's own error path prints and `exit(2)`s. If `ign mcp` or `ign lsp` ride the normal dispatch path, *any* error, warning, clap usage message, or stray `println!` in a shared helper lands in the protocol stream. The client (nvim, Claude Desktop) logs a parse error or kills the server — and the failure looks like a server bug, not a code-path bug.

**Why it happens:**
The single-exit-point chassis is the project's best structural decision — so the instinct is to extend it rather than bypass it. But protocol modes are not commands; they are alternate front doors.

**How to avoid:**
- Protocol modes must branch **before** envelope/render resolution: match the mode subcommand in `main` and enter a dedicated loop that never touches `render_ok`/`render_error`. The frozen contract is untouched because these modes are additive, not replacements.
- Enforce mechanically, not by convention:
  - Integration test that spawns the real binary, performs a full handshake, exchanges several messages (including one that triggers an internal error), and byte-scans stdout for anything unframed. This test must fail on a single stray byte.
  - `#![deny(clippy::print_stdout)]` (or a lint pass) on the protocol-mode modules.
  - Verify `init_tracing` has no stdout sink reachable from protocol mode.
- Both MCP and LSP get the *same* pattern; the framing differs (newline-delimited vs Content-Length) but the pre-dispatch entry and purity test are shared. Factor framing into distinct modules with their own tests — the classic cross-transport bug is mixing the two framings because one binary hosts both modes.

**Warning signs:**
- "parse error" or unexpected-disconnect in client logs right after server startup.
- Any `print!`/`println!`/`dbg!` reachable from a protocol-mode call path.
- Handshake works but the first tool/request response corrupts the stream.

**Phase to address:** EXT-04 (MCP) phase establishes the pattern — it will likely land first among protocol modes; IDE-02 (LSP) phase reuses it verbatim. The purity test should be written in whichever phase ships first and generalized, not rewritten.

---

### Pitfall 4: Version skew — new features vs stale route bundle or stale installed binary

**What goes wrong:**
Two concrete skew surfaces, both already demonstrated in v1.0:
1. Tag xml/csv transfer will likely require WebDev route changes → the bundle must bump 1.1.0 → 1.2.0 across **seven lockstep copies** (`routes/VERSION`, five `doPost.py` `ROUTE_VERSION` constants, `ROUTE_BUNDLE_VERSION` in `webdev/mod.rs`) with equality-enforcing drift tests. Miss one copy or forget a direction of the stale-refusal and either new CLI refuses old routes forever, or worse — new CLI happily calls routes missing the new logic and misparses.
2. The 07-VERIFICATION note is direct evidence: the author's own `~/.cargo/bin/ign` was stale and exhibited *already-fixed* bugs on the live gateway. A v1.1 user with a v1.0 binary calling v1.1-aware routes (or vice versa) produces confusing failures far from the cause.

**Why it happens:**
The lockstep is enforced for the version *strings* but the refusal logic and the "upgrade hint" path are per-feature work that's easy to skip when the happy path works on the dev machine (which is always freshly deployed).

**How to avoid:**
- Route bump = one atomic commit: all seven copies + both drift tests (new CLI ↔ old route refuses; old CLI ↔ new route refuses) + e2e redeploy gate. Copy the 07-06 pattern exactly — it was live-proven in both directions.
- Any feature that needs route N must fail on route < N with `route_version_mismatch` **plus a hint naming the fix** ("run `ign webdev deploy`"). No feature may silently degrade against an old bundle.
- Protocol modes (MCP/LSP) must report a coherent version triple (CLI version, route bundle version, protocol version) in their handshake/serverInfo so remote agents can diagnose skew themselves.

**Warning signs:**
- `route_version_mismatch` appearing during development on the shared dev rig (stale deploy).
- A drift test that only pins equality of strings, not behavior of the refusal.
- e2e gates passing on a machine where nobody remembers deploying routes recently.

**Phase to address:** Tag-transfer phase owns the bump discipline; EXT-04/IDE-02 phases own coherent version reporting in handshakes.

---

### Pitfall 5: Live-gate erosion — write features verified only against wiremock

**What goes wrong:**
v1.1 is write-heavy against real infrastructure: raw passthrough, EAM writes on a **production controller** (WHK), resource pushes, workspace sync. The single most expensive v1.0 lesson (05-06) was that five plans' worth of wiremock-green routes were **dead on real gateways** (the byte-0 loader contract) because the live run came last. The inverse pressure also exists: live gates mutate real gateways, so verifying "just this once" against the WHK production controller is dangerous — 07-VERIFICATION deliberately declined a live re-probe of `eam task force` because a freed slot would mean dispatching a real task.

**Why it happens:**
Wiremock passes feel sufficient; live rigs are slow (gateway commissioning 30–90s+), stateful, and mutation-risky. Under time pressure the env-gated gate gets "run later."

**How to avoid:**
- Every v1.1 write feature ships with at least one env-gated live gate, run and recorded during the phase — gate-first, not gate-last. 05-06's own words: "the env-gated gates exist to be RUN."
- Endpoint-sensitive features (EXT-02 diagnostics, historian binding, any EAM verb) gate on **both** live rigs (8.3.3 and 8.3.6), not one.
- Reuse the v1.0 harness unchanged: `LIVE_GATE` static mutex serialization (409-race prevention), pre-clean helpers for idempotent re-runs, throwaway compose rigs for destructive suites, production-controller probes limited to read-only or explicitly human-approved mutations (documented in VERIFICATION as human-verification items, never silently skipped).
- EXT-01 is special: its live gate must itself be designed so the passthrough can't nuke the rig (read-only method matrix in the gate; destructive methods verified against wiremock + a refusal test, not against the rig).

**Warning signs:**
- A plan whose verification section lists only wiremock contracts for a mutating path.
- Live gates filtered out of a test run (`cargo test -- --skip live` habitually).
- The WHK controller's task list gaining entries nobody can attribute.

**Phase to address:** Every v1.1 phase with a write path; the harness exists — this is a discipline pitfall, not a tooling one.

---

### Pitfall 6: EXT-01 raw passthrough that leaks the envelope or mis-classifies everything into exit 1

**What goes wrong:**
Three distinct failure shapes:
(a) **Unclassified 4xx → exit 1.** `classify.rs` classifies via path-scoped arms (`is_config_resource_url`, `is_eam_force_url`, …). A passthrough to an arbitrary URL matches none of them; the live-proven v1.0 lesson (`debug/eam-create-422.md`) is that an unclassified 4xx falls through to `internal_error` — "a 4xx validation response surfacing as internal error is itself a taxonomy gap." A raw-passthrough feature whose *entire job* is hitting arbitrary URLs will turn the gateway's rich 4xx surface into a wall of exit-1 "internal errors are bugs."
(b) **Envelope/diagnostics leakage.** Non-2xx bodies are sometimes HTML (auth redirect, gateway mid-restart — v1.0 pitfall 1.2); a serde parse of that is a panic-shaped failure; and any progress/verbose output on stdout breaks the one-envelope rule.
(c) **Binary corruption.** `api call` against an export endpoint returns ZIP bytes; printing those through a UTF-8 JSON envelope corrupts both the file and the terminal.

**Why it happens:**
The passthrough is conceived as "thin" — and thinness gets implemented as *no handling*, which is exactly wrong: the escape hatch must be thin in *transformation* (body verbatim) but strict in *classification and framing* (taxonomy, content-type sniffing, envelope always).

**How to avoid:**
- Install a passthrough catch-all classifier: any 4xx → exit 2 `invalid_input` (or one new additive slug such as `gateway_rejected`, same exit bucket) carrying the status code and body text verbatim in the error payload. 5xx → exit 3-class (gateway-side). Only genuinely unexpected local failures get exit 1.
- Sniff content-type before parsing: `text/html` → surface "gateway returned HTML (auth redirect or not fully up)" mapped to the auth/unreachable class, never parsed as JSON.
- Non-JSON content types require `-o FILE` (documented); attempt to emit binary through the envelope is a usage-class refusal.
- The envelope still wraps success: `{"ok":true,"profile":…,"data":<verbatim gateway body>}`. Document `data` as gateway-verbatim for this command only.

**Warning signs:**
- Exit 1 in any transcript where the gateway clearly returned 4xx.
- `serde_json::Error`/`expected value` in output for a failed call.
- A terminal trashed by binary bytes.

**Phase to address:** EXT-01 phase — the catch-all classifier is the first task, before the happy path is even wired.

---

### Pitfall 7: EXT-01 method/auth-header injection and destructive-by-accident passthrough

**What goes wrong:**
`ign api call --method DELETE /data/api/v1/resources/.../signature` against a live gateway is one flag away. Worse: `-H "X-Ignition-API-Token: <other token>"` overrides or duplicates the profile's auth header, creating confusing auth states and — via verbose request logging — a leak path for *user-supplied* auth headers (v1.0 pitfall 4.4 redacted the CLI's own token; user headers are a new surface). Raw writes to `/data/api/v1/resources/` also collide with signature semantics (v1.0 pitfall 1.4): DELETEs without a current signature 400/409 confusingly.

**Why it happens:**
An escape hatch feels like it should accept everything. But "everything" includes the CLI's own auth invariants, which the user can now violate.

**How to avoid:**
- Non-GET methods require `--yes` (same ladder as `sessions terminate`), with the URL echoed in the confirmation prompt for humans.
- Refuse `X-Ignition-API-Token` (and any profile-auth header) in `-H` with a clear "auth is owned by the profile" error — one source of auth.
- Extend redaction to **all** request headers matching auth patterns in verbose/debug logs, including user-supplied ones.
- When the URL matches `is_config_resource_url` and the method is mutating, append the existing signature hint to the error envelope so raw users discover read-modify-write instead of fighting 400s.

**Warning signs:**
- Verbose logs containing a second token value.
- A CI/e2e test that exercises raw DELETE against the live rig as a *positive* case.

**Phase to address:** EXT-01 phase (same task as the classifier).

---

### Pitfall 8: EXT-02 diagnostics — endpoint variance across 8.3.x point releases

**What goes wrong:**
License, redundancy, GAN, and diagnostics-bundle endpoints are less contractual than the core REST surface the CLI already wraps; shapes and even availability move between 8.3.x minors. v1.0 already lived this at small scale: EAM *history* decoding needed a wire-faithful UUID-string model pinned from a live capture; the working-vs-sent EAM create body differed from every doc sketch. A diagnostics command built against the dev rig's shape breaks on the other rig — and v1.1's promise is curated *reliable* views, so a broken diagnostics command is worse than none.

**Why it happens:**
Endpoint docs describe one version; rigs run two; production runs a third someday.

**How to avoid:**
- Version-tolerant parsing by default: `deny_unknown_fields` OFF, optional fields explicit (present-but-null in the envelope) so agents can rely on field presence as the stable contract even when the gateway omits data.
- Live gates on **both** rigs for every diagnostics verb.
- When an endpoint 404s or shape-shifts, honest degradation: an additive slug (`endpoint_absent_on_gateway_version`-style, exit-6 bucket) with a hint naming the gateway version probed — never exit 1, never a guessed partial payload.
- Curate the payload in the action layer (v1.0's proven pattern): the gateway's shape is an implementation detail; the envelope shape is the contract.

**Warning signs:**
- A command green on 8.3.3, red on 8.3.6 (or vice versa).
- Golden churn every time the suite runs against a different rig — a shape assumption leaking through.

**Phase to address:** EXT-02 phase; both-rig gating is a phase success criterion, not a stretch goal.

---

### Pitfall 9: EXT-03 EAM writes — wire-shape recomposition and guard-ladder erosion

**What goes wrong:**
The EAM create-body split (`config.profile` vs `config.settings`; `config.settings` REQUIRED, `targetGateways` belongs in settings) was learned live the hard way (422 `Settings cannot be null`, pinned in `debug/eam-create-422.md`). Every *new* EAM write verb (delete, new task types, definition edits) re-enters this minefield: the temptation is to compose bodies from docs sketches again. Separately, the guard ladder (`REFUSED_TYPES`/`MUTATING_TYPES`/benign) must extend coherently: a new type token that isn't classified defaults to *unclassified behavior* rather than *refusal*, and the `--help` taxonomy must match the token lists exactly (07-04 line-by-line verified — that equality is load-bearing for trust). New conflict shapes (e.g., deleting an in-flight task) will produce 409/422 variants not covered by `EamTaskInFlight`'s force-URL sniff.

**Why it happens:**
Each new verb looks like the previous one, so the composition helper and the classification lists get copied-and-extended casually instead of through the established seams.

**How to avoid:**
- All new EAM verbs compose bodies through the existing `composed_settings` seam; each new type gets a wire-pinned contract test captured from a live read-back (the `eam-working-definition.json` method), never from docs.
- Guard discipline: any new type token is added to a class **at creation time**; default-refuse is the fallback for anything unlisted; a contract test asserts help-taxonomy == token lists (extend the existing 07-04 check).
- New conflict classifications go through the classify.rs path-scoped-arm pattern (body-sniffing, additive slug, both exit-table places) — same shape as `EamTaskInFlight`, not ad-hoc string matching.
- Task IDs are UUID *strings* on the wire — keep wire-faithful string models (the gap-1 lesson); never coerce to numbers.
- WHK production controller: mutating e2e probes need the explicit mutation-risk policy from 07-VERIFICATION (wiremock pin + captured live envelope may substitute for a repeat probe).

**Warning signs:**
- 422s during development of any EAM verb (means the split was re-guessed).
- A type token appearing in `cli.rs` help text but not in `REFUSED_TYPES`/`MUTATING_TYPES` (or vice versa).
- New 409 text landing as `internal_error` in a transcript.

**Phase to address:** EXT-03 phase.

---

### Pitfall 10: EXT-04 MCP shim — protocol lifecycle, framing, catalog drift, and blocked loops

**What goes wrong:**
Six distinct traps in one feature:
(a) **Framing confusion:** MCP stdio is newline-delimited JSON-RPC with no embedded newlines — *not* LSP's Content-Length framing. One binary hosting both modes makes cross-contamination likely (verified: MCP spec 2025-06-18).
(b) **Handshake/version:** responding to `initialize` with a hardcoded or wrong protocolVersion, or failing politely on an unsupported one; ignoring that version negotiation is part of the initialize result.
(c) **Notification mishandling:** sending a *response* to `notifications/initialized` (notifications get no response — breaks clients) or treating notifications as requests.
(d) **Tool-catalog drift:** hand-maintained tool schemas drift from the clap tree; agents see stale params after every CLI change. This is the *same* failure class tui_coverage was built to kill — reimplementing it as a hand-written catalog undoes that protection.
(e) **Blocked loop:** a long gateway call (import, restart-wait) executed inline in the server loop stalls everything — including `ping` — until the client declares the server dead.
(f) **Lifetime/lifecycle errors:** not exiting on stdin close; prompting interactively (impossible — stdin is the protocol); crashing on one malformed message instead of answering with a JSON-RPC error.

**Why it happens:**
"Thin shim" is read as "small effort" rather than "small *transformation* with strict protocol obligations." The CLI's non-interactive and envelope discipline make the mapping trivial — but the protocol lifecycle itself is new code with new obligations.

**How to avoid:**
- Pin one spec revision (the settled stack is hand-rolled JSON-RPC 2.0 stdio, so pin to the spec text the shim implements — e.g. 2025-06-18), implement `initialize` by echoing a supported version or answering `unsupported protocol version`, and never respond to notifications.
- Derive the tool catalog from `Cli::command()` — the *same* CommandFactory the tui_coverage walk uses — with a CI test asserting tool names/params ↔ clap tree parity. The catalog is then structurally incapable of drifting.
- The shim maps tool call → the same actions layer → the **same frozen envelope** inside the tool-result content, so agent-side parsing stays identical to CLI mode. Mutating tools must preserve `--yes` refusal semantics *inside* the shim: the refusal envelope is the tool result — never a silent escalation or a bypass flag.
- Run gateway calls on a worker/tokio task; keep the loop responsive (answer `ping` immediately); per-operation-class timeouts per v1.0's 1.10 rule; gateway-down yields a JSON-RPC error / `isError: true` tool result, never a hang or crash.
- Exit cleanly on stdin EOF; malformed inbound message → JSON-RPC parse error response, continue serving.
- Non-interactive profile resolution only (env/flags/config at spawn); never prompt.

**Warning signs:**
- Client logs: "invalid JSON", protocol errors right after startup, or server marked unresponsive during a long op.
- The tool list missing a verb that shipped in the same release (catalog is hand-written).
- The shim code containing any `--yes` bypass or guard removal "for agent convenience."

**Phase to address:** EXT-04 phase. A scripted-client CI harness (fake MCP client over real stdio) is the phase's first deliverable — handshake, notification, error, ping, EOF cases all scripted.

---

### Pitfall 11: Tag xml/csv transfer — round-trip fidelity loss and spreadsheet mangling

**What goes wrong:**
JSON shipped as the native interchange in v1.0 precisely because tags are tree-typed: nested UDT structures, arrays, nulls, dataType precision, parameters. CSV flattening loses structure; XML half-solves structure but loses types the same way the WebDev layer stringifies values (v1.0's alarm-history `str()` lesson — numbers/dates arrive as strings, and once stringified the type is gone for good). The real-world corruptor is Excel: `001` → `1`, dates reinterpreted, BOMs added, quoting broken. A `tags export --format csv` → edit → `import` loop that *looks* symmetric silently mutates types, and users blame the gateway.

**Why it happens:**
XML/CSV are requested because humans and spreadsheets want them. The formats get built symmetric-by-appearance instead of export-honest.

**How to avoid:**
- Position xml/csv as **export-honest, import-guarded**: export freely; import validates and produces an explicit **loss report** (dropped fields, coerced types, flattened structures) and refuses ambiguous round-trips without `--yes`.
- RFC4180 quoting everywhere (commas, newlines, and quotes in tag paths/expressions are routine); UTF-8 explicit; unit-pin round-trip fixtures (JSON → csv → parse → JSON + loss list) on the existing v1.0 tag fixtures.
- Import reuses the proven `effective_top_level_names` collision semantics and the `_types_` structural skip — do not re-derive collision logic per format.
- Document loudly: "Excel-touched files must pass validation; the loss report is the safety net."
- If a route change is needed for server-side conversion, it's a bundle bump (Pitfall 4) — prefer CLI-side conversion from the JSON interchange to keep the route bundle frozen.

**Warning signs:**
- A round-trip test comparing JSON == JSON after a csv detour without a loss list (it can only pass if the loss list is empty — which means it's not testing real data).
- First user report of "tags changed after I just re-exported them."

**Phase to address:** Tag-transfer phase.

---

### Pitfall 12: Tag↔historian binding — guessing wire shapes instead of taking the Designer diff

**What goes wrong:**
The 05-06 spike ran five candidate REST bodies against a live InternalHistorian rig and **none produced data**; the documented resolution path is a Designer diff (capture tag/resource config before+after configuring the binding in Designer, replicate that). The v1.1 trap is building the feature on guessed field names before the diff evidence exists — committing a roadmap/phase deliverable the wire doesn't accept. Secondary traps: the diff requires a *licensed* Historian-capable rig (the InternalHistorian fixture may not exhibit the real binding), the binding config may live **outside** the tag config resource (provider/module config — "edit the tag" alone can't replicate it), and shapes may differ 8.3.3 ↔ 8.3.6.

**Why it happens:**
It's the last remaining v1.0 limitation, so there's pressure to *ship the closure* rather than *run the experiment*. Docs sketches feel authoritative; 05-06 proved they aren't.

**How to avoid:**
- Spike-first design, exactly as v1.0 pre-cleared: the phase's plan 01 is a time-boxed live diff on **both** rigs; **no CLI surface commits until the diff lands a wire shape.**
- The roadmap item must explicitly allow the fallback outcome ("documented limitation remains, now with diff evidence of why") — that fallback was legitimate in v1.0 and remains legitimate.
- If the diff lands: new fields ride as additive optional fields in the existing tag config model + a live gate proving data actually flows (the spike's success criterion was data, not a 200).

**Warning signs:**
- A plan sketching REST bodies from documentation alone.
- No licensed-rig access plan before the phase starts.
- Any commit adding binding fields without a captured live diff artifact.

**Phase to address:** Historian-binding phase (spike = its first plan). The roadmapper should hold the feature's "done" definition loose until the spike reports.

---

### Pitfall 13: TUI theming — retrofitting a palette onto hardcoded colors, then fighting style-pinned buffer tests

**What goes wrong:**
After 11 plans of rapid TUI build, `ui/*.rs` almost certainly hardcodes `Color::…` literals. Declaring "themes" while half the widgets bypass the palette produces a fiction that breaks on the first screen someone forgot. Compounding it: `TestBackend` buffer assertions pin exact styles, so theme work churns tests confusingly; and color-mode variance (truecolor / 256 / 16 / mono, plus light-background terminals) means the dev terminal's readability proves nothing — the 16-color and mono renderings are the ones that matter for the floor experience. Parity CI implications are mild (theming shouldn't change `routes()`), but theme plumbing that adds new leaves would trip the walk.

**Why it happens:**
"Theming" sounds like a config file; in a ratatui codebase it's an architecture precondition (tokenization) plus rendering work.

**How to avoid:**
- Phase order inside the theme work: **tokenize first** — a style-tokens module as the *only* place `Color`/`Modifier` literals appear; CI grep enforcing no raw literals outside it; themes are token sets on top.
- Buffer assertions reference tokens or normalize styles during comparison so theme changes don't churn them.
- Respect the environment ladder: `NO_COLOR`, `CLICOLOR_FORCE`, `COLORTERM=truecolor`, TERM 256/16 detection — with a forced-mode override flag for tests; TestBackend renders at each mode.
- Ship **monochrome as the floor theme** (readable in any terminal, light or dark); verify contrast at 16 colors explicitly — that's where themes die.
- Theme plumbing adds zero new clap nodes; if it seems to, it's scope creep (Pitfall 2).

**Warning signs:**
- `Color::` literals appearing outside the tokens module during theme work.
- Buffer-test churn that isn't a deliberate token rename.
- "Looks fine on my terminal" as the only readability evidence.

**Phase to address:** TUI phase; tokenization is its first plan, themes second.

---

### Pitfall 14: TUIX-01 polling cadence — gateway hammering and config drift

**What goes wrong:**
Configurable cadence invites sub-second rates. Against a real gateway that's controller CPU and log spam; against a **production** gateway it's an operational incident with the CLI's name on it. Multiple TUI instances poll in synchronized bursts. And the config itself introduces a new schema surface: an invalid cadence value that *exits* makes the TUI unopenable — the worst possible failure for a config typo.

**Why it happens:**
Cadence is a one-line change from the user's perspective; the blast radius (gateway load, fleet effects, config-schema validity) is invisible locally where the dev rig shrugs at anything.

**How to avoid:**
- Conservative default (e.g. 5–10s), a hard floor (e.g. 1s) with a visible warning when configured below it, and jitter to de-synchronize instances.
- Back off on 429/503/timeouts (halve the rate, respect retry hints) — the TUI must survive a gateway that pushes back.
- Invalid cadence config **degrades to default with a warning** (stderr + TUI status line), never exit; profile validation surfaces the value in `doctor` so `--json` consumers see it too.
- One shared cadence source consumed by all workers (refresh, watch, tail, rig_stream) — not per-worker timers that drift.
- Per-profile scoping (the feature's own requirement) with validation at `profile add`/`profile set` time.

**Warning signs:**
- Gateway metrics/audit logs showing request spikes correlated with TUI sessions.
- The TUI failing to open on a config typo.
- Different screens refreshing at different effective rates.

**Phase to address:** TUI phase (adjacent plan to theming; shares the config-validation work).

---

### Pitfall 15: IDE-01 edit round-trip — adversarial `$EDITOR`, stale pushes, and fail-open encoding

**What goes wrong:**
`$EDITOR` is adversarial: emacs/vscode fork or daemonize (process exit ≠ editing done; vscode needs `--wait` semantics), some IDEs return immediately, editors write backup files or don't flush before exit. Treating exit code as truth causes the CLI to read a half-written file or an unchanged one. Temp-file mistakes compound it: wrong suffix (`.json` for a script) breaks both highlighting and the Flint encode step; temp files in shared TMPDIR leak project code and invite symlink games. The encode-back step can fail on user-introduced syntax — a fail-open push sends broken resources to a live gateway. And the push itself is zip-member surgery + import: pushing clobbers concurrent gateway-side edits unless the read-modify-write signature discipline (v1.0 pitfall 1.4) is honored.

**Why it happens:**
The round-trip is tested with vim, which is the *best-behaved* editor. Everything unusual is deferred until real users hit it.

**How to avoid:**
- Editor handling: spawn with a private tempdir (0700), content-correct suffix and a name derived from the resource path; treat exit code as advisory — content-hash comparison decides "changed vs no-op" (unchanged → clean no-op, no push, no confirmation prompt); on failure paths keep the temp file and tell the user where it is.
- Encode-back: Flint encode failure = **fail closed** — print the codec error, keep the temp file for re-edit, never push. The gateway may *accept* syntactically broken Jython (it fails later at runtime), so CLI-side encode is the only gate that fires before damage.
- Staleness: before push, re-verify the target is unchanged since fetch (signature or re-read compare); if changed, refuse with a "resource changed on gateway" error and a re-fetch path — clobbering is not a `--yes`-able outcome by default.
- Push UX: show a diff summary, require explicit `--yes` (the existing destructive ladder), then push.
- Cleanup: Drop-guard deletes the tempdir on abort/Ctrl-C.
- Editor-fixture tests at least for the three archetypes: blocking (vim), deferred-exit (vscode `--wait` pattern), forked (emacs daemon) — as spawned-process fixtures, not mocks of `std::process`.

**Warning signs:**
- Round-trip works with vim but hangs or mis-fires with anything else.
- Temp files accumulating in `/tmp`.
- A user report of "my edit pushed but other things changed."

**Phase to address:** IDE-01 phase; the editor-fixture harness is its first deliverable.

---

### Pitfall 16: IDE-02 LSP mode — state machine discipline and blocking gateway calls inside the loop

**What goes wrong:**
`lsp-server` 0.10 is a *synchronous crossbeam-channel scaffold* — "you control the message dispatch loop yourself" (verified: docs.rs 0.10.0). So: (a) a blocking gateway call inside the loop stalls every request including `$` notifications and shutdown — nvim appears frozen; (b) state-machine shortcuts break clients: requests before `initialize` must answer `ServerNotInitialized` (-32002); advertising capabilities you don't implement makes nvim send requests that error; incremental sync advertised but not implemented corrupts buffers — full-text sync is the safe default; `shutdown` → `exit` sequencing must be exact; (c) gateway slow/unreachable is *normal*, not exceptional — a completion that blocks on a 10s gateway call is an unusable editor; (d) the server is launched by nvim, so profile resolution must be non-interactive (env/args only) and a gateway error must degrade to empty results with a stderr log, not kill the server; (e) a panic in the loop kills the server mid-session (nvim restarts it, all state lost); (f) stdout framing belongs to lsp-server — any direct write is the Pitfall 3 violation in its purest form.

**Why it happens:**
LSP demos are written against a fast local gateway and a cooperative client. The real deployment is nvim + a gateway that's restarting half the time.

**How to avoid:**
- Async-at-the-edge: worker threads fetch/caches gateway data; the loop serves from cache with TTL and bounded waits; a miss returns empty results (with a log) rather than blocking.
- Honest capabilities: advertise only completion + diagnostics as actually implemented; full-text sync (`change: None`) unless incremental is implemented and tested; strict initialize/shutdown sequencing with a scripted-client CI test (same harness family as MCP's).
- `-32002` for anything pre-initialize; catch_unwind around per-request handling so one panic degrades one response.
- Profile from env/args only; `ign lsp --check` (offline doctor: profile resolvable, gateway reachable, route version OK) so misconfiguration is diagnosable from nvim's log window, which is where stderr goes.
- The gateway-latency contract: completions < 50ms from cache, worst-case bounded; *never* a synchronous gateway round-trip inside a completion request.

**Warning signs:**
- nvim log: "request timed out", server restarts in the middle of a session.
- Completions freezing while a gateway restarts or the network blips.
- One malformed message taking down the server.

**Phase to address:** IDE-02 phase — deliberately **last** among the IDE features: it reuses the protocol-mode pattern from EXT-04 (Pitfall 3) and the cache/latency discipline, and it's the most novel failure surface.

---

### Pitfall 17: IDE-03 workspace checkout — non-injective path mapping, case collisions, and gateway clobbering

**What goes wrong:**
Resource paths → filesystem paths must be a **bijection**. Ignition resource names can differ only in case — and the dev machine is macOS with default case-insensitive APFS, so two distinct resources map to one path and one is silently lost (then pushed back, deleting it server-side). Names can contain characters illegal on some filesystems and can carry traversal (`../`) — the mapping must be escaping + traversal-safe. Partial sync without a state manifest can't distinguish "user edited locally" from "gateway changed since checkout," so `sync --push` clobbers concurrent edits — and in this ecosystem the concurrent editor is real: the Designer and the git-module edit the same projects.

**Why it happens:**
Checkout works perfectly on the happy-path project where no two names differ by case. The bijection property is never tested, so it's never enforced.

**How to avoid:**
- Define the mapping as an explicit injective encoding; unit-test the bijection property over adversarial name fixtures (case pairs, filesystem-hostile characters, traversal attempts, unicode) — a property test, not examples.
- Refuse to map two resources to one path with an explicit error; never last-write-wins.
- Checkout writes a **manifest** (gateway resource path + content hash + signature at checkout). Push = only manifest-tracked modified files, gated by the `--yes` ladder, preceded by a gateway-side diff summary; pull uses the same three-way comparison. The manifest is the conflict detector — without it, every sync is a gamble.
- Refuse traversal-shaped mappings (write outside the checkout root = hard error) — test with hostile names.
- Decide manifest gitignore-vs-committed deliberately and document; the checkout dir is likely a git repo and the manifest's presence changes diffs.

**Warning signs:**
- Files "missing" after checkout on macOS but present in the export.
- `sync --push` pushing files nobody edited.
- Any checkout test suite that only uses well-behaved names.

**Phase to address:** IDE-03 phase; the bijection test and the manifest are plan-01 deliverables.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Hand-written MCP tool catalog | Fast first ship | Permanent drift from the clap tree; agents mis-call tools after every CLI change | Never — derive from `Cli::command()` + CI parity test |
| Hardcoded `Color::` literals in `ui/*.rs` before theming | Speed now | Full-sweep rewrite when the theming phase lands | Only if the tokenization refactor is *scheduled as the theming phase's plan 01* — the debt is then self-extinguishing |
| Stringly-typed xml/csv cells (no type tags) | Simple format | Silent type loss on every round-trip | Only for explicitly documented lossy display export — never for import-capable formats |
| Bypassing `classify.rs` in `api call` ("it's raw, skip classification") | Fewer arms to write | Unclassified 4xx storm → exit 1; agents misread gateway state | Never — the catch-all classifier *is* the feature |
| Per-worker polling timers | Quick wiring | Cadence drift between screens, synchronized gateway hammering | Never — one shared cadence source |
| Regenerating goldens to unblock CI | Green build today | Contract breakage ships silently | Never — regeneration is a review event (Pitfall 1) |
| Extending the dispatch chassis for MCP/LSP | Reuse feeling | Stdout purity violations in every error path | Never — protocol modes get a pre-dispatch front door (Pitfall 3) |
| Ignoring the second rig (8.3.3 XOR 8.3.6) in live gates | One less rig to maintain | Point-release endpoint variance ships (Pitfall 8) | Never for EXT-02/EXT-03/historian; acceptable only for features pinned to version-stable endpoints |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| MCP clients (Claude Desktop, nvim MCP) | Assuming SSE or Content-Length framing; responding to notifications | Newline-delimited JSON-RPC per spec 2025-06-18; notifications get no response; stderr is the log channel |
| nvim as LSP client | Advertising unimplemented capabilities; incremental sync half-implemented; interactive profile prompts at spawn | Honest capabilities; full-text sync by default; env/args profile resolution; `-32002` pre-init |
| `$EDITOR` variants (vscode, emacs daemon) | Treating editor exit as done; assuming the file saved | Content-hash decides; content-correct temp suffix; document `--wait` expectations for IDE editors |
| Excel / spreadsheet round-trips | Trusting a csv file that passed through Excel | Import loss report + refuse ambiguous without `--yes` (Pitfall 11) |
| 8.3.x point releases | Assuming the dev rig's shapes generalize | Both-rig live gates; optional fields; honest `endpoint absent` degradation (Pitfall 8) |
| WebDev route bundle | Forgetting the 7-copy version lockstep on bump | Single-commit bump + both-direction drift tests + redeploy gate (Pitfall 4) |
| Gateway resource signatures (v1.0 1.4) | Fire-and-forget raw writes to `/resources/` | Read-signature-mutate; signature hints surfaced on raw-path conflicts (Pitfalls 7, 15) |
| Designer / git-module concurrency | Assuming the CLI is the only writer | Checkout manifest + three-way compare + `--yes` push (Pitfall 17) |
| EAM on the WHK production controller | Re-probing mutations "just to check" live | Wiremock pin + captured live envelope may stand in for repeat probes; read-only probes only without explicit human approval (Pitfall 5) |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Sub-second polling cadence | Gateway CPU/log spikes; production complaints | Floors + jitter + backoff on 429/503 (Pitfall 14) | Immediately on production gateways; dev rigs hide it |
| Blocking LSP loop on gateway calls | nvim frozen; request timeouts in client log | Cache + TTL + bounded waits + workers (Pitfall 16) | First slow/restarting gateway |
| Blocking MCP loop on long ops (import, restart-wait) | Client marks server dead mid-tool | Worker tasks + per-op-class timeouts + immediate `ping` (Pitfall 10) | First long operation invoked as a tool |
| Zip round-trip per single-resource edit (IDE-01 push = export→surgery→import) | Edit feels slow on large projects | Accept and document (v1.0 accepted the same cost for resource editing); optimize only if painful | WHK-Global-sized projects |
| Golden/contract suite growth | CI timeouts (the v1.0 verifier already hit 10/15-min tool timeouts on the >15-min subprocess-heavy suite) | Targeted goldens per new feature; batch contract tests; keep one live gate per feature, not per case | v1.1 adds ~11 features' worth of goldens to an already slow suite |
| Serialized live e2e gates growing long | Suite wall-time balloons | LIVE_GATE mutex is for *mutation* serialization only — keep read-only probes parallel where safe | When every new feature adds gates to one mutex |

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Verbose logging of user-supplied `-H` headers in `api call` | Second token leaked into agent transcripts | Extend redaction to all auth-pattern headers including user-supplied (Pitfall 7) |
| MCP/LSP shim exposing mutating tools without the CLI's guard semantics | Agents trigger destructive ops accidentally, or a "convenience" bypass ships | The shim preserves refusal semantics server-side: the refusal envelope *is* the tool result; no bypass flags (Pitfall 10) |
| Edit-round-trip temp files in shared TMPDIR | Project code exposure; symlink attacks | Private tempdir 0700, Drop-guard cleanup, unique per session (Pitfall 15) |
| Workspace checkout trusting resource names as paths | Path traversal writing outside the checkout root | Injective escaping scheme + traversal refusal + hostile-name property tests (Pitfall 17) |
| Raw passthrough to session/CSRF endpoints (`/data/app/login` etc.) | Half-authenticated flows; CSRF-shaped state the CLI doesn't own | API-token is the only auth path; document session endpoints as unsupported in `api call` (or refuse with a hint) |
| LSP server running with a broad profile from any directory | Untrusted workspaces gain gateway access | Explicit profile resolution (env/args); document the trust model; `ign lsp --check` shows what would be used |

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| Themes readable only on the dev terminal (dark, truecolor) | Unusable for 16-color/light-terminal users | Monochrome floor theme; contrast verified at 16 colors (Pitfall 13) |
| Invalid cadence config kills TUI startup | Config typo = cockpit won't open | Degrade to default + visible warning; `doctor` surfaces it (Pitfall 14) |
| Encode errors swallowed by the edit round-trip | User believes push succeeded; gateway fails later at runtime | Fail closed with the codec error + kept temp file for re-edit (Pitfall 15) |
| LSP failures invisible (stderr hidden in nvim) | Mystery broken completions | One-line log-window-friendly diagnostics + `ign lsp --check` doctor (Pitfall 16) |
| Raw `api call` errors rendered raw | Agents/users misclassify gateway problems | Envelope still adds classification + hints for known URL classes while the body rides verbatim (Pitfall 6) |
| Workspace sync with no conflict story | Silent clobbering of concurrent edits | Manifest-driven three-way compare; diff shown before push (Pitfall 17) |

## "Looks Done But Isn't" Checklist

- [ ] **EXT-01:** Missing content-type sniffing — verify an HTML 401 body yields the auth class (not a serde panic), and a binary-response URL forces `-o FILE`.
- [ ] **EXT-01:** Missing user-header redaction in verbose logs — verify a supplied auth-pattern header prints redacted.
- [ ] **EXT-01:** Missing the 4xx catch-all — verify an unmatched-path 422 exits 2-class, never 1.
- [ ] **EXT-02:** Missing the second-rig gate — verify live gates ran on **both** 8.3.3 and 8.3.6 and are recorded.
- [ ] **EXT-03:** Missing help-taxonomy ↔ token-list equality for new types — extend the 07-04 line-by-line check.
- [ ] **EXT-04:** Missing "no response to notifications" and "exit on stdin EOF" — scripted-client test must cover both.
- [ ] **EXT-04:** Missing tool-catalog ↔ clap parity CI test — verify it exists and would fail if a verb were added without a tool.
- [ ] **MCP/LSP:** Missing the stdout-purity byte-scan test — verify it exists in whichever phase ships first and actually catches a stray `println!`.
- [ ] **Tag transfer:** Missing the import loss report — verify a lossy csv import refuses without `--yes` and names every loss.
- [ ] **Route bundle:** After a 1.2.0 bump — verify *both directions* of staleness refusal are tested (new CLI↔old route, old CLI↔new route).
- [ ] **IDE-01:** Missing unchanged-file no-op and temp cleanup on Ctrl-C — verify content-hash no-op and Drop-guard.
- [ ] **IDE-01:** Missing signature/re-read staleness check before push — verify a gateway-side concurrent change refuses the push.
- [ ] **IDE-02:** Missing `-32002` pre-init and shutdown→exit sequencing — scripted-client CI test.
- [ ] **IDE-03:** Missing the bijection property test (case pairs, hostile chars, traversal) and the checkout manifest.
- [ ] **Theming:** Missing the "no `Color::` outside the tokens module" CI grep.
- [ ] **TUIX-01:** Missing invalid-config degradation — verify the TUI opens with default cadence + warning on a corrupt config value.
- [ ] **All:** New slugs present in BOTH exit-table places (`error.rs` enumerated test + README table + prose).

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Contract/golden breakage shipped | MEDIUM | Next release restores the old shape as a deprecated alias (additive repair), changelog + version note; agents re-converge on the next sync |
| Route bundle skew in the wild | LOW | `route_version_mismatch` already refuses safely; publish "run `ign webdev deploy`" guidance; both-direction tests prevent recurrence |
| Broken resource pushed to live gateway (IDE-01) | HIGH | Re-export the prior resource from gwbk backup / git-module history; re-import; then add the signature pre-check that was missing |
| Workspace checkout clobbered gateway edits | MEDIUM | Re-pull affected resources from gateway backup/Designer history; rebuild the manifest; add the three-way compare |
| MCP/LSP stdout pollution in the field | LOW | Fix the leak; the byte-scan test makes recurrence impossible; clients reconnect cleanly |
| Cadence hammering incident | LOW | Floor + backoff already bound the blast radius; restart TUI; tighten the floor |
| Excel-corrupted csv imported | LOW–MEDIUM | The loss report should have refused; re-export from gateway as JSON (lossless); if it slipped through, re-import corrected values |
| Unclassified 4xx storm (exit 1 flood) | LOW | Additive catch-all slug; transcripts remain parseable retroactively since the envelope shape held |

## Pitfall-to-Phase Mapping

v1.1 phases are not yet numbered; feature-ID anchors with a recommended grouping follow. Ordering rationale for the roadmapper: (1) the first CLI-surface phase must establish the slug discipline, OutOfBand classification, and protocol-mode front door *before* the second command needs them; (2) tag transfer carries the route-bundle bump and should land before MCP so the derived tool catalog is stable; (3) MCP precedes LSP so the protocol-mode pattern (stdout purity, scripted-client harness, version reporting) is proven once and reused; (4) IDE-01/03 precede IDE-02 because they share the checkout/codec substrate the LSP diagnostics will read.

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| 1 Golden churn / contract mutation | Every phase; codified in the first slug-adding phase | Goldens diff-reviewed; `exit_code_mapping_enumerated` green; no existing-line golden edits without justification |
| 2 tui_coverage fight | First CLI-surface phase | CI walk green; pinned OutOfBand test updated once, with per-entry justification comments |
| 3 Stdout purity | EXT-04 phase (pattern), reused by IDE-02 | Byte-scan integration test over a real spawned binary |
| 4 Version skew | Tag-transfer phase (bump); EXT-04/IDE-02 (version reporting) | Both-direction drift tests; coherent version triple in protocol handshakes |
| 5 Live-gate erosion | Every write-feature phase | Env-gated gates run and recorded; both rigs for endpoint-sensitive features |
| 6–7 EXT-01 classification & injection | EXT-01 phase | Wiremock pins for 4xx/HTML/binary; refusal tests for destructive methods; redaction test |
| 8 EXT-02 point-release variance | EXT-02 phase | Both-rig live gates; honest-degradation slug test |
| 9 EXT-03 guard ladder & composition | EXT-03 phase | Composition contract pins from live read-backs; help-taxonomy equality check |
| 10 EXT-04 protocol lifecycle & drift | EXT-04 phase | Scripted-client harness (handshake, notifications, ping, EOF, malformed); catalog parity test |
| 11 xml/csv fidelity | Tag-transfer phase | Round-trip loss-list fixtures; refusal-without-`--yes` test |
| 12 Historian guess-shapes | Historian phase, plan 01 spike | Captured live diff artifact — or the documented-limitation fallback, either is "done" |
| 13 Theming tokenization | TUI phase, plan 01 | CI grep for stray `Color::`; forced-mode buffer tests incl. 16-color/mono |
| 14 Cadence hammering | TUI phase | Backoff contract tests; profile validation; corrupt-config degradation test |
| 15 Editor round-trip | IDE-01 phase | Editor-archetype fixtures (blocking/deferred/forked); staleness-check pin; no-op test |
| 16 LSP state machine & latency | IDE-02 phase (last IDE phase) | Scripted-client state-machine tests; cache-latency contract; panic-isolation test |
| 17 Checkout bijection & manifest | IDE-03 phase | Bijection property tests over hostile names; manifest three-way compare tests |

## Carry-Forward from v1.0 (still operational)

Full text: `.planning/research/PITFALLS.md` @ commit `9d2cc32`. The items below remain binding constraints on v1.1 design:

- **1.4** Signature-based optimistic concurrency on config resources — now load-bearing for EXT-01 raw writes, IDE-01 pushes, IDE-03 sync.
- **1.7** `wait_until_ready` multi-stage restart poll — reused by any v1.1 flow that restarts/wait on gateways.
- **1.8** WebDev route contract discipline (versioned bundle, refusal both directions) — the basis of Pitfall 4.
- **1.10 / 4.5** Per-operation-class timeouts and progress/cancellation on long ops — now applies to MCP tool calls and LSP request handling.
- **2.1** Never block the TUI event loop — cadence and theme work must not regress it.
- **3.6 / 4.4** Secret redaction (config perms, logs, verbose) — extended by Pitfall 7 to user-supplied headers.
- **4.2** Non-interactive by default — extended by MCP/LSP (never prompt from a protocol process).
- **6.1 / 6.2** Layered testing (wiremock contracts + trait-seam mocks + env-gated live) and the live e2e harness rules (LIVE_GATE, pre-clean, throwaway rigs) — the substrate of Pitfall 5.

## Sources

- **Local system evidence (HIGH):** `crates/ignition-cli/tests/tui_coverage.rs` (bidirectional walk, pinned OutOfBand set, row-requiring rules); `crates/ignition-cli/src/main.rs` (single-exit-point chassis, render/stdout discipline, sanctioned stdout exceptions); `crates/ignition-core/src/error.rs` (exit_code mapping, enumerated test — exit-6 operation/denial bucket verified); `crates/ignition-core/src/client/classify.rs` path-scoped arms (per 07-VERIFICATION-GAPS.md evidence).
- **Local milestone artifacts (HIGH):** `.planning/phases/05-webdev-backend-tag-operations/05-06-SUMMARY.md` (byte-0 loader contract, live-gate discipline, export-shape normalization, historian spike outcome + Designer-diff fallback); `.planning/phases/07-ecosystem-interop-advanced-ops/07-VERIFICATION-GAPS.md` (guard ladder, Two-Place exit-table rule, mutation-risk human-verification policy, stale-binary note, route 1.1.0 lockstep); `.planning/debug/eam-create-422.md` (422→internal_error taxonomy gap, config.profile/settings split); v1.0 `PITFALLS.md` @ `9d2cc32` (carry-forwards).
- **MCP specification (HIGH — fetched 2026-09-04):** modelcontextprotocol.io/specification/2025-06-18/basic/transports — stdio newline-delimited JSON-RPC, no embedded newlines, MUST NOT write non-MCP output to stdout, stderr for logs, stdin-close termination, version negotiation at initialize.
- **lsp-server crate docs (HIGH — fetched 2026-09-04):** docs.rs/lsp-server/0.10.0 — synchronous crossbeam-channel scaffold; handles handshaking/parsing; "you control the message dispatch loop yourself"; IoThreads/Connection/ReqQueue structure.
- **Domain knowledge (MEDIUM, flagged inline):** nvim LSP client behavior specifics; editor-archetype behaviors (vscode `--wait`, emacs daemon); Excel round-trip corruption modes; 8.3.x point-release endpoint drift beyond the two live-verified rigs. Each is backed by a testable prevention in this doc — the phase's verification, not the claim, is the safety net.

---
*Pitfalls research for: ignition-cli v1.1 — Agent Surface & IDE Integration (contract-frozen system)*
*Researched: 2026-09-04*
