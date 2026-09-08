# Phase 10: 10-eam-write-operations - Research

**Researched:** 2026-09-08 (desk research; NO live captures — rig access is an execution-phase task per the 09-02 capture-first pattern)
**Domain:** Ignition EAM controller wire surface (runtime lifecycle verbs + config-resource mutations), guard-ladder composition, blast-radius preview
**Confidence:** HIGH on endpoint inventory + repo patterns (openapi extract + live code); MEDIUM on mutation semantics (PUT/delete round-trips are spec-read, not capture-proven — that is exactly what the capture-first plan exists to answer)

## User Constraints (from ROADMAP — no CONTEXT.md exists; the roadmap flags ARE the user context)

### Locked Decisions
- **Gate-first, not gate-last**: every guarded write lands with its confirmation guard + tests from birth; no "add guards later" plan shape.
- **Capture-first for endpoint-sensitive work**: Phase 9 precedent (09-02) — a DEDICATED LIVE-CAPTURE PLAN on both rigs (8.3.3 + 8.3.6) produces `10-LIVE-CAPTURES.md` BEFORE any model task; capture-locked vocabularies encode into later plans.
- **v1.0 `debug/eam-create-422.md` is required reading** for wire-shape honesty (profile/settings split; 422 taxonomy gap — since fixed, see below).
- **Both-rig guidance** applies where endpoint shapes are involved.
- **At least one guarded write live-verified end-to-end against the real WHK controller rig** — the env-gated live gate is recorded DURING the phase, not bolted on after.
- All writes behind the confirmation guard (EAMW-01…06 each say "+ guard"). Note: 07-RESEARCH recommended suspend/cancel as *unguarded* (alarms-ack precedent) — **the Phase 10 roadmap overrides that**: refused without explicit confirmation. Follow the roadmap.
- Session core discipline: all new actions resolve clients through `ignition-core::Session` — no second client construction.
- Frozen JSON envelope contract: additive-only slugs; new error surfaces map to existing exit classes; envelope shapes never renumbered.

### Claude's Discretion
- CLI verb naming/shape under `ign eam …` (subcommand structure, flag names).
- Blast-radius preview composition (which pre-write reads feed it) and its rendering in human/JSON/TUI modes.
- Whether new error slugs are needed or existing classes cover every refusal (research recommends: existing classes suffice — see Taxonomy section).
- Scope boundary: whether `clear-retry`/`retry` (the retry family) is in or out — research recommends OUT (not in EAMW-01…07).

### Deferred Ideas (OUT OF SCOPE)
- Fleet-destructive task types (`eam_restoreBackup`, `eam_installModules`, `eam_remoteUpgrade`) — REFUSED by the existing ladder; EXT-03 (v2) scope. Do not un-refuse them in Phase 10.
- Agent approval/quarantine/upgrade management (`approve-agent`, `quarantined-agents`, `upgrade-agent`) — not in EAMW-01…07; note their existence, plan nothing.
- Renaming/reclassifying v1.0 read verbs (`eam history`, `eam tasks`).

---

## Summary

The Phase 10 endpoint inventory is essentially **already answered by the v1.0 trimmed openapi extract** (`.planning/phases/07-ecosystem-interop-advanced-ops/07-openapi-extract.json`), which the client/eam.rs doc-comment points at. Every verb EAMW-01…07 needs has a declared wire shape:

- **Task lifecycle = RUNTIME verbs** under `/data/eam/api/v1/eam-tasks/`: `POST suspend/{name}`, `POST resume/{name}`, `POST cancel/{name}` (all 204-on-success, 403 with the controller message on stock gateways — already classified to `EamNotController` by the existing path-scoped arm), plus the implemented `force/{owner}/{name}`. The openapi *descriptions* say these suspend/resume/cancel **gateway TASKS** — there is **no agent-level suspend/resume endpoint anywhere in the extract**. The requirement's "suspend an EAM agent" phrasing must reconcile to task-scoped verbs; the persisted mirror of suspension is `config.profile.isSuspended` on the task definition.
- **Pending-execution reads**: `GET /data/eam/api/v1/eam-tasks/scheduled/{running}` returns `{name, owner, type, execStart, message, repeats, canPause, canResume, canCancel, taskState}` — the gateway itself exposes which lifecycle verbs are legal per pending execution. This is the natural pre-write data source for the blast-radius preview and the verb-applicability check.
- **Config mutations** under `/data/api/v1/resources/com.inductiveautomation.eam/eam-tasks` (NOT controller-gated — available on any gateway): array-body `PUT` = "Modify one or more EAM Agent Task resources" (item: `{name, collection, enabled, description, signature, config{profile{type, isSuspended, scheduleMode, scheduleDetails}, settings}, backupConfig}`; query `allowInvalidReferences`), and `DELETE {name}/{signature}?collection=&confirm=` — **delete is signature-keyed**, with a gateway-side `confirm` query ("required when the operation would affect other resources") and a 200 body of `{success, changes[{name,type,collection,newSignature}], problem{message, stacktrace}}`. The gateway's own `changes[]` is a native blast-radius report.

The in-repo machinery for all of this already exists: client HTTP helpers (`put_json`, `post_empty`, `delete_with_query`, `get_json`), the tags.rs **delete-by-signature precedent** (`tag_provider_delete_path(name, signature)`), the pure guard-ladder pattern, `require_confirmation` in main.rs, the TUI `Confirm` modal + `PendingAction` registry, the classify seam arms (including the 422 config-resource arm that fixed the eam-create-422 gap), `ListEnvelope`, and Session resolution. No new dependencies.

**Primary recommendation:** Plan a 09-02-style capture-first plan (Task 1: spin/provision both rigs; Task 2: a scratch-task lifecycle probe — create → suspend → resume → scheduled read → modify-PUT → delete → 4xx probes — captured verbatim on both rigs, with a "Decisions locked by captures" section), then client paths → trait methods → actions (preview + guard) → CLI → TUI → gates, with the live gate plan landing in the same plan set (not appended). Reuse existing slugs — research finds NO new slug is required.

---

## Standard Stack

### Core (all already in-tree — nothing new to install)

| Piece | Where | Purpose | Why Standard |
|---|---|---|---|
| `GatewayApi` trait + `ReqwestGatewayApi` | `crates/ignition-core/src/client/mod.rs` | Add new EAM methods beside `eam_task_create/force/find/definitions/history` (mod.rs:356-395, impls 1348-1396) | The ONE client seam; Session hands it out |
| HTTP helpers | client/mod.rs: `get_json` (514), `post_empty` (646), `delete_with_query` (666), `post_json` (685), `put_json` (727) | suspend/resume/cancel = `post_empty`; PUT = `put_json`; delete = `delete_with_query` | No hand-built request sites (09-05 one-streaming-site discipline) |
| `delete_with_query` + tags delete-by-signature | `client/tags.rs:68-71` (`tag_provider_delete_path(name, signature)`) | EAM delete `{name}/{signature}` has a direct in-repo twin | Byte-identical shape to the proven provider delete |
| `EAM_BASE`, path fns | `client/eam.rs` | Extend with `eam_task_suspend_path(name)` etc. beside `eam_force_path` | Module-scoped prefix keeps classify's 403 arm keyed correctly |
| classify seam arms | `client/classify.rs:80-228` | 403-controller arm (`is_eam_url`, line 89), 422 config-resource arm (line 165), force-409 arm (line 141), 09-01 `api_call` catch-all (line 199) | New verbs inherit honest classification with ZERO new arms (see Taxonomy) |
| Pure guard ladder | `actions/eam.rs:68 task_create_guard` | The composition template for a lifecycle guard — pure, shared by main.rs / TUI / action | Double-check pattern: CLI pre-resolution + action authoritative re-check |
| `require_confirmation(yes, operation)` | `main.rs:2452` | CLI-side guard; exit 2 `confirmation_required` on refusal, pre-resolution (zero network) | The established guard idiom (13 call sites) |
| TUI `Confirm { title, body }` + `PendingAction` | `ignition-tui/src/state.rs:113-116, 190-240` | TUI mirrors CLI guards; OutOfBand rows land WITH clap commands | The routes↔menu parity CI rides this registry |
| `ListEnvelope<T>` + `encode_segment` | client core | `scheduled/{running}` reads; URL segment encoding (hyphen→%2D over-encode is pinned discipline) | Existing envelope contract |
| `Session` | `ignition-core/src/session.rs` | ALL new action resolution | CORE-09: no second client construction |
| wiremock contract tests | `crates/ignition-core/tests/eam_contract.rs` | Extend the per-verb REQUEST-pinning pattern | Existing per-family contract-test harness |
| env-gated live gates | `tests/live_gateway.rs` + Phase-4 rig recipe | Success criterion 5's end-to-end live verification | Established harness; gates run only with env vars set |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|---|---|---|
| Runtime suspend/resume verbs | PUT `config.profile.isSuspended` via config-resource | The PUT mutates the *definition* (persisted); the runtime verbs are the documented operation ("prevents it from executing at the next scheduled time") and are what the webpage calls. Use runtime verbs as primary; whether the runtime verb also syncs `isSuspended` is a CAPTURE question. Do not build on the PUT-isSuspended path until captured. |
| Task-scoped suspend/resume (wire truth) | An invented agent-level suspend | No such endpoint exists in the extract — inventing one is exactly the wire-shape-dishonesty pitfall. Name CLI verbs task-scoped; surface agent inventory reads (`GET /agents`) only if the preview needs them. |
| New slug for lifecycle-state refusals (e.g. resume-when-not-suspended) | Existing `invalid_input` (exit 2, pre-write pure refusal from preview data) | A verb-not-applicable refusal is client-side knowledge (from `canResume`/`canCancel` flags) → `InvalidInput` reads honestly; a gateway-side state refusal would be target-state exit 6 (the `SessionNotPrunable` precedent). Default to `invalid_input` + honest message; only add a slug if a captured gateway refusal has no honest mapping. **Recommendation: no new slugs in Phase 10.** |

---

## Architecture Patterns

### Recommended plan sequence (Phase 9's capture→model→action→CLI→TUI→gates, instantiated)

```
10-01  LIVE-CAPTURE plan (09-02 pattern) → 10-LIVE-CAPTURES.md + 10-RIG-NOTES.md
       (scratch-task lifecycle on BOTH rigs; Decisions-locked-by-captures section)
10-02  Client: path fns + GatewayApi methods (suspend/resume/cancel/scheduled/update/delete)
       + wiremock contract tests pinning every REQUEST
10-03  Actions: lifecycle verbs + blast-radius preview composition + guard re-check
10-04  CLI verbs (clap) + main.rs wiring (pre-resolution guards, preview-before-confirm)
       + TUI PendingAction rows WITH clap commands (parity CI) + README
10-05  Env-gated live gate: at least one guarded write end-to-end on the WHK controller rig
       (gate recorded DURING the phase — same plan set, not bolted on)
(+ gap-closure plans from UAT, the 09-07/08 pattern)
```

### Pattern 1: The guard composition — TWO tiers, because the preview needs network

**What:** The existing guards are pre-resolution/zero-network (the `task new` ladder classifies from parsed args; `force` guards before `resolve_gateway_api`). EAMW-07 changes the game: the confirmation text must NAME the target and controller impact, which requires read-only pre-flight reads. The guard becomes a ladder:

```
Tier 0 (pure, zero network — BEFORE any resolution):
  - verb applicability from args alone where decidable
    (unknown task name is NOT decidable here — it needs find)
  - require_confirmation refusal shape (exit 2) is pre-composed
Tier 1 (read-only pre-flight, network):
  - find(name)          → definition + signature + scheduledTaskState
  - scheduled/running   → canResume/canCancel/taskState for THIS task
  - (optional) history   → last-run outcome context
  → compose the blast-radius preview (target task, type, schedule,
    owner, targetGateways, current state, pending executions)
Tier 2 (the gate):
  - CLI: require_confirmation whose `operation` string embeds the preview facts
  - TUI: Confirm modal whose body IS the preview
  - refusal here = exit 2 confirmation_required, zero writes performed
Tier 3 (authoritative re-check in the action — the task_create double-check):
  - the action re-reads/re-verifies before the write (cheap; keeps core honest)
Tier 4 (the write): post_empty/put_json/delete_with_query
```

**Why:** a pure pre-resolution guard is impossible for preview text (find/scheduled are network reads), but the refusal CLASSES stay pure: no `--yes` → the CLI may still do the read-only pre-flight to render the preview (reads are safe) but must refuse the WRITE. The action-layer re-check keeps the ladder authoritative in core.

### Pattern 2: Modify = read-modify-write with signature (never compose-from-scratch)

**What:** The PUT is "Modify one or more" with array body carrying `signature`. The honest shape is: `find` → clone the full record → mutate ONLY the targeted keys (name/description/enabled/config.profile.scheduleMode/scheduleDetails/settings…) → `put_json` the single-element array carrying the ORIGINAL `signature` + `collection`. Never send a hand-composed partial config — `config.settings` is REQUIRED on the wire (the live 422: "Settings cannot be null"), and the create-composer (`compose_task_definition`) composes CREATE bodies, not modify bodies.

**Example shape (spec-derived; capture-verify before freezing):**
```rust
// POST-suspend (runtime): 204 success, 403 controller-classified, 404 unknown task
api.eam_task_suspend(&name).await?;

// PUT-modify (config resource): array body, original signature echoed
let mut record = api.eam_task_find(name).await?;      // full record + signature
record.config["profile"]["scheduleDetails"] = json!(details);
api.eam_task_update(&record).await?;                   // put_json single-element array
```

### Pattern 3: Capture-locked vocabularies (the 09-04/09-05 lesson)

`currentState` values seen live so far: `IDLE` (wiremock fixture), `Errored` (real 8.3.3 find). `scheduled/{running}` adds `taskState` + the three `can*` booleans. **Never enum these** — String consts + per-element rig provenance (`BUNDLE_GENERATING_STATES` precedent; STATE.md 09-05 decision). Same for history `taskType` — note it is `"backup"` in the live capture while profile.type is `eam_backup` (the tokens DIFFER between the runtime history seam and the config definition seam; passthrough both, never normalize).

### Anti-Patterns to Avoid

- **Composing a modify body from the create-composer** — different shape, settings 422 trap, loses unknown fields (attributes.uuid, version, healthchecks round-trip keys).
- **Guarding after resolution** — the Tier-0/Tier-2 order in Pattern 1; the refusal must precede any write, and refusals-that-need-no-data must precede resolution.
- **Deleting without the signature** — 400 "Missing 'name' or 'signature' parameter"; and the signature is find-derived, never cached across sessions.
- **Parsing execution outcomes into errors** — history `level: "Failed"` is exit-0 DATA (research Pitfall 3, locked in v1.0); same for `problem.message` in mutation 200-bodies: report honestly in the envelope, only *classify* HTTP-status failures.
- **Sending `isSuspended` on create** — server-owned on create (the existing composer pins this); on MODIFY it round-trips from find (capture-verify the echo).

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---|---|---|---|
| Signature-keyed delete | A custom delete request site | `delete_with_query` + a `eam_task_delete_path(name, signature)` beside `tag_provider_delete_path` | Byte-identical precedent in tags.rs; one-request-site discipline |
| Array-body PUT | A bespoke serializer | `put_json` with a `serde_json::Value` single-element array | `post_json(&path, &[definition])` array precedent at client/mod.rs:1386 |
| Confirmation gating | New prompt/flag machinery | `require_confirmation(yes, operation)` (CLI) + `Confirm` modal (TUI) | 13 proven call sites; exit-2 hint already names `IGNITION_YES=1` |
| Guard classification | Per-verb ad-hoc if/else | A pure `task_lifecycle_guard`-style fn in actions/eam.rs (shared main.rs/TUI/action) | The `task_create_guard` triple-consumer pattern is the locked composition |
| Controller-state honesty | Per-verb 403 handling | The existing `is_eam_url` + message-scoped classify arm | Path-scoped; new runtime verbs inherit it with zero code |
| Preview data assembly | New client calls | `eam_task_find` + a new `eam_scheduled` read + optional history | Reads already exist except scheduled; add ONE method |
| Rig provisioning | New recipes | Phase-4 recipe (04-VERIFICATION.md:168-286) + 09-RIG-NOTES format | Proven on both 8.3.3 and 8.3.6 |

**Key insight:** Phase 10 is almost entirely *composition of proven v1.0 machinery over four new endpoints + two new config mutations*. The novel engineering is (a) the two-tier guard with preview, and (b) capture-locked lifecycle vocabularies.

---

## Common Pitfalls

### Pitfall 1: Wire-shape dishonesty — "agent" suspend does not exist
**What goes wrong:** Planning `ign eam agent suspend` against an invented endpoint.
**Why it happens:** The requirement text says "suspend and resume an EAM agent"; the openapi has only task-scoped suspend/resume; `isSuspended` lives on the task definition's profile.
**How to avoid:** CLI verbs are task-scoped (`ign eam task suspend|resume|cancel`); README/`--help` explain that suspending a task's execution is how agent-affecting schedules are paused; `GET /agents` remains an inventory read. Flag the wording reconciliation in the phase plan.
**Warning signs:** any plan task naming an endpoint not in the extract.

### Pitfall 2: Guessing lifecycle vocabularies
**What goes wrong:** Hard-coding `currentState`/`taskState` enums from web memory; a rig's "Paused"/"Suspended" state fails to parse or worse, silently maps wrong.
**How to avoid:** Capture-first (10-01); String consts with per-element provenance; unobserved states ride passthrough and render honestly (09-05 decision verbatim).
**Warning signs:** an enum named `CurrentState` in the diff.

### Pitfall 3: The modify 422 trap returns
**What goes wrong:** A hand-composed PUT body omits `config.settings` (or partial-config semantics assumed) → 422 "Settings cannot be null" — the exact v1.0 live failure, resurrected on the modify path.
**How to avoid:** read-modify-write of the FULL find record (Pattern 2); wiremock tests assert the PUT body echoes settings/attributes; capture plan includes a real modify round-trip (find → PUT → find) to prove echo semantics.
**Warning signs:** a PUT body built from `compose_task_definition`.

### Pitfall 4: Signature staleness / mismatch behavior unknown
**What goes wrong:** PUT/DELETE with a stale signature (another client edited between find and write) — the failure status/body is UNVERIFIED (400? 409? 200+`problem`?). Curated-path 400 currently falls through to `Internal` (exit 1) because the 09-01 `api_call` catch-all only covers `ign api call`.
**How to avoid:** capture the mismatch behavior explicitly (10-01 probe); then classify: likely a route-scoped arm (the designer-prune 409 / force-409 precedents) or `GatewayClientError` extension. Decide the taxonomy in the plan AFTER capture, not before.
**Warning signs:** exit-1 internal errors during live-gate runs.

### Pitfall 5: Taxonomy freeze violations
**What goes wrong:** Renumbering exits, renumbering envelope slugs, landing a new slug across three commits.
**How to avoid:** reuse existing classes (`eam_not_controller` exit 6, `not_found` exit 6, `invalid_input` exit 2, `confirmation_required` exit 2, `eam_task_in_flight` exit 6); any genuinely new slug lands atomically — exit-code enum + README table + include_str! agreement test, one commit (Three-Place rule).
**Warning signs:** a new variant in error.rs without a same-commit README row + agreement test update.

### Pitfall 6: Controller-mode assumption leakage into tests
**What goes wrong:** Wiremock tests only assert the happy 204; the stock-gateway 403 classification for the NEW verbs never gets pinned; a live gate on a non-controller gateway then surfaces `auth_rejected` lies.
**How to avoid:** every new runtime verb gets the paired contract test: happy 204 AND the captured controller-403 (HTML Jetty body) → `eam_not_controller` (the existing eam_contract.rs pattern pins exactly this pair for reads).
**Warning signs:** a new client method with only one mock.

### Pitfall 7: Delete's gateway-side confirm interacts with OUR guard
**What goes wrong:** The gateway's own `?confirm=` gate ("required when the operation would affect other resources") is confused with the CLI's `--yes`. Sending `confirm=true` unconditionally could skip a genuine multi-resource warning; not sending it could 200-with-`success:false` and get mis-parsed as failure.
**How to avoid:** capture the no-confirm affected-resources response FIRST; likely policy: send the delete WITHOUT gateway `confirm`, parse `{success, changes}` — if `success:false` + changes, surface them in the preview and re-run with confirm once the user's `--yes` covers it (planner decision; needs the capture).
**Warning signs:** `confirm=true` hard-coded in the client method.

### Pitfall 8: Live-gate environment traps
**What goes wrong:** Trial expiry blocks only EXECUTION (force) — lifecycle writes (suspend/resume/cancel/modify/delete) are NOT trial/GNET-gated, so a lifecycle-only live gate avoids the v1.0 GNET-not-connected trap entirely; but a force-based live gate hits it (07-RESEARCH pitfall 3; tier-0 trial reset works on 8.3.3).
**How to avoid:** make the mandated end-to-end live write a lifecycle verb (e.g. suspend→verify→resume on a scratch task) rather than force; if force is live-gated, trial-reset first and expect the GNET data outcome in history (exit-0 with `level: Failed` is CORRECT).
**Warning signs:** a live gate that treats history `Failed` as gate failure.

---

## Code Examples

### New client methods (trait additions beside the existing five)
```rust
// client/eam.rs — path fns (module-scoped, classify-keyed prefix preserved)
pub(crate) fn eam_task_suspend_path(name: &str) -> String {
    format!("{EAM_BASE}/eam-tasks/suspend/{}", encode_segment(name))
}
// resume_path / cancel_path identical shape.

// client/mod.rs GatewayApi trait — beside eam_task_force (mod.rs:388)
async fn eam_task_lifecycle(&self, verb: LifecycleVerb, name: &str) -> Result<(), CoreError>;
async fn eam_scheduled(&self, running: bool) -> Result<ListEnvelope<ScheduledTask>, CoreError>;
async fn eam_task_update(&self, record: &EamTaskRecord) -> Result<MutationResult, CoreError>;
async fn eam_task_delete(&self, name: &str, signature: &str) -> Result<MutationResult, CoreError>;
// impls: post_empty(…, true) / get_json / put_json / delete_with_query — NO new request sites
```

### Preview model (actions layer — the blast-radius contract)
```rust
/// What the confirmation gate shows BEFORE any guarded write (EAMW-07).
#[derive(Debug, Serialize)]
pub struct EamBlastRadiusPreview {
    pub task: String,                 // target definition name
    pub task_type: Option<String>,    // config.profile.type
    pub schedule_mode: Option<String>,
    pub is_suspended: Option<bool>,   // config.profile.isSuspended (find)
    pub current_state: Option<String>,// scheduledTaskState.currentState (find)
    pub owner: Option<String>,        // details.owner (force needs it)
    pub target_gateways: Vec<String>, // config.settings.targetGateways
    pub pending: Vec<ScheduledTask>,  // scheduled/{running} rows for THIS task
    pub can_resume: Option<bool>,     // verb applicability, gateway-declared
    pub can_cancel: Option<bool>,
    pub last_run: Option<EamHistoryItem>, // context (history search)
}
```

### Guard re-check (action-side, the task_create double-check pattern)
```rust
pub async fn eam_task_suspend(api: &dyn GatewayApi, name: &str) -> Result<EamLifecycleResult, CoreError> {
    let record = api.eam_task_find(name).await?;           // Tier 3 re-check + preview data
    api.eam_task_suspend(&record.name.clone()).await?;     // Tier 4 write
    // read-back: find again → isSuspended/state honestly surfaced (capture decides if synced)
}
```

---

## State of the Art (repo-relative)

| Old Approach | Current Approach | When Changed | Impact for Phase 10 |
|---|---|---|---|
| 422 falls through to `Internal` (eam-create-422.md complaint) | `S::UNPROCESSABLE_ENTITY if is_config_resource_url` → `InvalidInput` exit 2 (classify.rs:165) | 07-05 gap 3 fix | The documented taxonomy gap is CLOSED — do not re-fix it; PUT/delete 400s are the REMAINING unclassified 4xx on curated paths (Pitfall 4) |
| Unclassified 4xx = Internal always | 09-01 `api_call` catch-all → `GatewayClientError` exit 2 on the `ign api call` path only | Phase 9 | Curated EAM writes still need their own honest arms where capture proves shapes |
| Guard = pre-resolution pure only | Preview-required guards need read-only pre-flight (two-tier, Pattern 1) | Phase 10 introduces | New composition; keep refusal classes pre-write |
| suspend/cancel recommended unguarded (07-RESEARCH) | ALL writes guarded (roadmap EAMW-01…06) | Phase 10 roadmap overrides | Follow the roadmap; the alarms-ack precedent is superseded here |
| Version-tolerance: enums for wire vocabularies | String consts + passthrough (09-04/05 decisions) | Phase 9 | Applies to `currentState`/`taskState`/`can*` handling |

---

## Open Questions (for the capture-first plan to answer on live rigs)

1. **Does runtime suspend/resume sync `config.profile.isSuspended`?** — find read-back before/after POST suspend on both rigs. Decides whether resume/suspend results report the definition flag or only runtime state, and whether `ign eam tasks <name>` shows suspension honestly after the verb.
2. **Full `taskState`/`currentState` vocabulary** from `GET scheduled/{running}` (create an OnDemand + a Scheduled scratch task; force one to get a running row) — becomes `String` consts with provenance. `canPause` vs `canResume` vs `canCancel` truth table per state.
3. **Delete without gateway `?confirm=` when other resources are affected** — exact 200 body (`success:false`? `changes` content?) → decides our confirm policy (Pitfall 7).
4. **Signature-mismatch PUT/DELETE** — status + body (stale signature after an intervening edit) → decides the classify arm (Pitfall 4).
5. **Rename via PUT with changed `name`** — supported rename, new-resource creation, or 4xx? If unsupported: rename = create-new + delete-old composition (planner decides CLI shape).
6. **PUT partial vs full config acceptance** — prove echo-semantics (full-record modify round-trip) and whether omitting `config` entirely is accepted (it should not be relied on either way).
7. **404 body shape on runtime verbs for unknown task names** — Jetty HTML vs JSON → whether `not_found` classification needs a note or rides clean.
8. **`scheduled/{running}` empty + pagination shape** — confirm `{items: [], metadata{…}}` envelope on a quiet controller; confirm the `{running}` path segment takes `true`/`false` literally.
9. **PUT response shape on success** — 200 `{success, changes[]}`? (openapi only documents 400/500 bodies) — pins `MutationResult` parsing.
10. **When does `problem{message, stacktrace}` appear** in mutation 200-bodies (partial failure across array items?) — informs honest success reporting.
11. **8.3.3 vs 8.3.6 drift** on every captured shape (the documented point-release failure mode — every probe labeled per rig).
12. **`scheduleDetails` wire form** — string (cron? ISO timestamp?) for Scheduled/AtTime/AtDelay modes; only needed if Phase 10 exposes schedule editing beyond mode (planner scope decision).

## Sources

### Primary (HIGH confidence)
- `.planning/phases/07-ecosystem-interop-advanced-ops/07-openapi-extract.json` — full endpoint inventory, request/response schemas for suspend/resume/cancel/scheduled/agents/PUT/DELETE (paths quoted verbatim above)
- `crates/ignition-core/src/client/eam.rs` — two-seam documentation, existing models/paths
- `crates/ignition-core/src/actions/eam.rs` (695 lines, read in full) — guard ladder, composer, force choreography, summary projections, test breadth
- `crates/ignition-core/src/client/classify.rs` (arms at 80-228) + `crates/ignition-core/src/error.rs` (slug/exit-code/hint tables)
- `crates/ignition-cli/src/main.rs` (EAM dispatch 1869-1963, `require_confirmation` 2452) + `crates/ignition-cli/src/cli.rs` (EamArgs/EamTaskCommand/ScheduleMode)
- `crates/ignition-core/tests/eam_contract.rs` — request-pinning wiremock pattern incl. the 403-classification pair
- `crates/ignition-tui/src/state.rs` — `Confirm` modal + `PendingAction` registry
- `.planning/debug/eam-create-422.md`, `eam-working-definition.json`, `eam-history-raw.json` — live 8.3.3 shapes (record keys, signature form, healthcheck details, history taskType token)
- `.planning/phases/07-ecosystem-interop-advanced-ops/07-RESEARCH.md` — controller-flip recipe, trial/GNET execution gates, v1.0 guard recommendations
- `.planning/phases/09-agent-surface-api-diagnostics/09-02-PLAN.md` + `09-LIVE-CAPTURES.md` + STATE.md decisions — the capture-first template and Phase 9 conventions

### Secondary (MEDIUM confidence)
- PUT/delete round-trip semantics, rename support, signature-mismatch behavior — spec-read from the extract, NOT capture-proven (explicitly routed to open questions 3-6)

### Tertiary (LOW confidence — flagged)
- Nothing sourced from web memory; all web-adjacent claims deliberately deferred to captures per the phase's both-rig directive.

## Metadata

**Confidence breakdown:**
- Endpoint inventory: HIGH — verbatim from the in-repo openapi extract, cross-checked against client/eam.rs and 07-RESEARCH
- Repo patterns (guard/TUI/classify/Session/tests): HIGH — read from source this session
- Mutation semantics (PUT echo, delete confirm, signature mismatch, rename): MEDIUM — declared shapes only; capture-gated before model freeze
- Pitfalls: HIGH for repo-discipline pitfalls (patterns proven in v1.0/Phase 9); MEDIUM for wire-behavior pitfalls (capture-verify)

**Research date:** 2026-09-08
**Valid until:** capture plan execution (the extract is static; the open questions define its probe list)
