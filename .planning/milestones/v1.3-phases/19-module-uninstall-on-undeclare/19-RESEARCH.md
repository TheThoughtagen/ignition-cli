# Phase 19: Module Uninstall on Undeclare - Research

**Researched:** 2026-09-25
**Domain:** Ignition 8.3 gateway module-uninstall REST contract; `ign`'s override-diffing and guard architecture
**Confidence:** HIGH (the endpoint contract and the override dead-code finding are live-verified against a real gateway in this session; the CLI verb shape is a documented open design choice)

## Summary

`DELETE /data/api/v1/modules/uninstall` is real, and this session probed it directly
against a disposable `inductiveautomation/ignition:8.3.3` rig built from the Phase 16
fixture, using the already-built `ign` binary (`adopt` for auth, `ign api call` for the
raw request) — no new code was written to do this. The endpoint takes a JSON body
`{"uninstall": [<gateway_module_id>, ...]}` (the SAME id shape `/modules/healthy`
returns and `ModuleSpec::gateway_module_id` already carries), always answers HTTP 200,
and reports outcome via a body field: `{"success": bool, "failedUninstalls":
{"uninstall": [ids that failed]}}`. This is the exact "denial-rides-200" shape the
codebase already has a precedent for (`CoreError::ImportDenied`, 05-07) — `ign` MUST
parse this field explicitly; the HTTP status alone says nothing.

The critical operational finding, confirmed live: **uninstall only succeeds against a
module whose backing `.modl` file is already absent from `user-lib/modules`** (a
"ghost" registry entry — present in `/modules/healthy`'s id list but with empty
`name`/`version`/`state`, logged by the gateway as `"the file for module '<id>' is
missing and will not be loaded"`). While the module's file is still read-only
bind-mounted (i.e. still declared and provisioned), the SAME call refuses with
`success: false`. This maps precisely onto Phase 19's real workflow: undeclare in
config -> `rig up` removes the mount -> module becomes a ghost entry -> uninstall now
succeeds. Effect is immediate (no restart needed) — the module vanishes from
`/modules/healthy` the instant the 200 with `success:true` returns.

A second, unplanned finding emerged from the live probe and changes the shape of Phase
19's actual scope: **the compose override file (`compose.ign-modules.yml`) is NOT
deleted today when a module is undeclared and `rig up` is run** — contradicting
`modules.rs`'s own doc comment, which claims "the next `up` ... deletes it ... if
nothing is declared." Both the CLI dispatch's `merged_modules.is_empty()` short-circuit
(`main.rs`) and `provision_modules`'s own `declared.is_empty()` early return skip
`write_override` entirely, so its delete branch is dead code on the real undeclare
path — only reachable if a caller deletes the file by hand first, which is exactly what
the Phase 16 live-gate test's §C does (`std::fs::remove_file(&override_path).expect
("delete override by hand")`), masking the gap. Phase 19 needs BOTH the uninstall call
AND a fix so the override file itself stops orphaning on disk.

**Primary recommendation:** Build the "what did `ign` provision last time" diff off the
PRE-overwrite override file (read via `existing_override(plan)` BEFORE this run's
`write_override`/deletion happens), diff its declared ids against this run's merged
declared set, and REPORT the orphaned ids on `rig up` rather than uninstalling them
inline — gate an explicit new verb (`ign rig module uninstall <id> --yes`) behind
`GUARDED_OPS`, matching the orchestrator's stated preference and this repo's existing
guard conventions (destructive verbs are always separate, deliberate commands: `project
delete`, `rig reset`, `rig restore`, `rig trial reset` — never something `rig up` does
as a side effect). Fix the override dead-code path in the same phase, since the diff's
correctness depends on it.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Detect an orphaned (undeclared but gateway-installed) module | `ignition-core` (`rig::modules`) | — | Pure diff logic over `RigEntry::modules` (new) vs. the pre-overwrite override contents; no I/O beyond a file read already performed by `existing_override` |
| Uninstall confirmation gate | `ignition-cli` (`main.rs` dispatch, `GUARDED_OPS`) | — | Every other destructive verb's guard lives at the CLI dispatch layer, not in core (`require_confirmation` is a `main.rs`-local fn); core stays guard-agnostic per existing precedent |
| Issue the uninstall HTTP call | `ignition-core` (`client::GatewayApi`, new method or `api::` extension) | — | Every gateway capability today is a `GatewayApi` trait method (`modules`, `metrics_*`, …) implemented once in `ReqwestGatewayApi`; the uninstall call should follow the same seam, not ride the raw `api call` escape hatch in the shipped feature |
| Parse `{"success", "failedUninstalls"}` and raise `CoreError` on denial | `ignition-core` (new `CoreError` variant + the action function) | — | Mirrors `ImportDenied`'s "curated call site checks the body, not just the status" pattern (05-07) |
| Report result (JSON envelope / human render) | `ignition-cli` (`ActionOutput`, `render_success`) | — | Existing convention: every action's result type implements the render trait; `RigUpResult` already carries `provisioned_modules` this way |

## Package Legitimacy Audit

Not applicable — this phase adds zero new external dependencies. It uses the gateway's
own existing REST surface and the crate's existing `client`/`error`/`rig` modules.

## Architecture Patterns

### System Architecture Diagram

```text
config.toml [rigs.NAME.modules.*]           compose.ign-modules.yml (on disk, PRE-run)
        |                                              |
        v                                              v
merge_declarations(config.modules, --with-module) --> declared (this run's target set)
        |                                              |
        |                              existing_override(plan) [D-10, READ before overwrite]
        |                                              |
        |                                   parse ids out of the override's
        |                                   ACCEPT_MODULE_CERTS line (or track
        |                                   provisioned ids via a small sidecar
        |                                   — see Q3) --> previously-provisioned set
        v                                              |
   provision_modules(declared) -----------> mounts, override_file  <--- diff -----+
        |                                              |
        v                                              v
   rig_up() / rig_reset()                    orphaned = previously-provisioned - declared
        |                                              |
        v                                              v
   compose up -f base -f override         RigUpResult.orphaned_modules (REPORT ONLY,
   (module container state)                this phase's default — no gateway write)
                                                        |
                                                        v
                                    ign rig module uninstall <id> --yes  (deliberate,
                                    separate, GUARDED_OPS verb; NOT invoked by `rig up`)
                                                        |
                                                        v
                                    DELETE /data/api/v1/modules/uninstall
                                    body {"uninstall": [<gateway_module_id>]}
                                                        |
                                            200 {"success": bool,
                                                 "failedUninstalls": {"uninstall": [...]}}
                                                        |
                                    success:true  -> module gone from /modules/healthy
                                                       IMMEDIATELY, no restart
                                    success:false -> CoreError (new variant), exit 6
```

### Recommended Project Structure

No new files needed. Extend in place:
```
crates/ignition-core/src/
├── client/
│   ├── mod.rs           # add GatewayApi::uninstall_modules (or similar)
│   └── status.rs        # (or a new modules.rs client file) — response wire struct
├── rig/
│   └── modules.rs        # add: previously_provisioned_ids(plan) -> Vec<String>,
│                          #      orphaned_modules(previous, declared) -> Vec<String>,
│                          #      fix write_override's now-reachable delete path
├── actions/
│   └── rig.rs            # RigUpResult gains `orphaned_modules`; new module-uninstall
│                          # action function
└── error.rs               # new CoreError variant (see Q5)

crates/ignition-cli/src/
├── cli.rs                 # RigCommand gains a `Module(ModuleArgs)` nested subcommand
│                           # (mirrors `Trial(TrialArgs)`), with `Uninstall { id: String }`
└── main.rs                 # GUARDED_OPS gains ("rig module uninstall", "rig module uninstall")
```

### Pattern 1: Denial-rides-200 parsing (established precedent)
**What:** A gateway endpoint answers HTTP 200 on both success and application-level
refusal; the caller must inspect a body field to know which happened.
**When to use:** Any raw REST call whose failure mode is NOT expressed as a 4xx/5xx —
confirmed live for `/modules/uninstall` in this session (a stock module, a bogus id, and
a still-mounted module all returned HTTP 200 with `success:false`).
**Example (existing precedent to mirror, not modify):**
```rust
// Source: crates/ignition-core/src/error.rs (ImportDenied, 05-07) — read this session
#[error("gateway rejected the project import for {project:?}: {problem}")]
ImportDenied {
    project: String,
    problem: String,
    endpoint: Option<String>,
},
```

### Anti-Patterns to Avoid
- **Treating HTTP 200 as success for this endpoint:** confirmed live — a call that
  changes NOTHING on the gateway (stock module, bogus id, still-mounted module) still
  returns 200. Code that only checks `classify()`'s success path will silently report
  "uninstalled" when nothing happened.
- **Assuming a re-declared, re-mounted module reloads after uninstall:** confirmed live
  — once a module is uninstalled via the API, simply placing its `.modl` file back at
  the mount path and running `rig up` again does NOT bring it back (no ghost entry, no
  quarantine entry, nothing — the gateway's `ModuleManager` does not even log a "Starting
  up module" line for it on the next boot). This needs to be stated plainly to the user
  in the confirmation prose — uninstall is not "undo declare," it is closer to
  permanent without a full `rig reset` (which wipes the whole data volume) or the
  separate `/modules/install` gateway endpoint (out of scope here — not probed).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Auth for the live probe / any future live gate extension | A hand-rolled OIDC client | `actions::adopt::adopt` (already exists, already proven against this exact fixture in 16-03) | Basic auth 401s on commissioned 8.3 `/data` routes (re-confirmed live this session); `adopt` is the ONLY proven path |
| Raw HTTP call for manual verification during planning/spike work | curl / a scratch Rust binary | `ign api call --method DELETE --path ... --data ...` (already shipped, EXT-01) | Zero new code needed — this session's entire probe used only the already-built binary |
| Denial-body parsing | A new ad-hoc JSON check | Mirror `ImportDenied`'s exact pattern (parse `success`, carry the raw ids/problem into a typed `CoreError`) | One documented, working precedent already in this codebase for exactly this HTTP shape |

**Key insight:** Every tool this research needed already ships in `ign` itself
(`adopt`, `rig up`, `modules`, `api call`). Nothing about this phase's *verification*
needs new tooling — only its *implementation* needs new code.

## Runtime State Inventory

> Rename/refactor trigger does not apply (this is new-capability work, not a rename) —
> but the phase's whole premise IS "what runtime state does undeclaring leave behind,"
> so the same discipline applies to the GATEWAY's own state, live-verified this session:

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Gateway-side stored data | A module's DB tables (e.g. `GITPROJECTSCONFIGRECORD`, `GITREPOSUSERSRECORD` for the git module) persist in the gateway's internal DB independent of `.modl` mount presence — observed as "No migration strategy found for table X" warnings on every boot where the module is absent. Uninstalling via the API does not appear to touch these (not independently confirmed — see Open Questions). | None required by `ign` — this is Ignition's own internal state; document it as a known residue, do not attempt to clean it |
| Gateway-side stored data | `data/jar-cache/<gateway_module_id>/` — the exploded JAR cache. Confirmed this directory SURVIVES a successful uninstall (checked live: `docker exec ... ls data/jar-cache/` still showed the git entry after `success:true`). | None required — cosmetic gateway-internal leftover, not `ign`'s concern, but worth a one-line README/pitfall note so a future "why is disk still used" question isn't mistaken for an `ign` bug |
| `ign`-owned file | `compose.ign-modules.yml` (the override) — CONFIRMED this session to NOT be deleted by the current `rig up`/`rig reset` code path when a module is undeclared (see Summary and Q3). This is the one item in this table `ign` DOES own and must fix. | Code fix: make the override's delete branch reachable when the declared set transitions to empty, not just when `provision_modules` happens to be called with a non-empty-then-emptied map |
| OS-registered state | None found. Modules are not OS-registered anywhere (no service, no scheduled task, no launchd/systemd unit per module). | None |
| Secrets/env vars | None — module provisioning carries no secrets; `ACCEPT_MODULE_CERTS`/`ACCEPT_MODULE_LICENSES` are non-secret acceptance flags. | None |

## Common Pitfalls

### Pitfall 1: Trusting HTTP 200 as the uninstall verdict
**What goes wrong:** `ign` reports "module uninstalled" when the gateway actually
refused (module still mounted, wrong id, or the id was never installed).
**Why it happens:** `classify()`'s default path treats any 2xx as success and passes
the response through for `.json()` parsing — it never inspects the BODY for an
application-level `success: false`.
**How to avoid:** Parse `success`/`failedUninstalls` explicitly in the action function,
exactly as `ImportDenied` already does for project import. Raise a typed `CoreError`
when `success` is `false`, naming every id in `failedUninstalls.uninstall`.
**Warning signs:** A test that only asserts `status_code == 200` on the wiremock mock,
never asserting on the response BODY, would pass while this bug ships.

### Pitfall 2: Re-declaring an uninstalled module does not un-uninstall it
**What goes wrong:** A user runs the guarded uninstall verb, regrets it, re-adds the
module to `[rigs.NAME.modules]`, runs `rig up` again, and expects the module back.
**Why it happens:** Confirmed live — the gateway's `ModuleManager` does not re-scan
`user-lib/modules` for a module it has already purged from its internal registry via
the uninstall API. The `.modl` file being present again in the mount changes nothing;
no "Starting up module" log line appears, no quarantine entry appears either.
**How to avoid:** The `--yes` confirmation prose for the new uninstall verb MUST say
this plainly (not just "destructive," but "not undone by re-declaring — full recovery
needs `ign rig reset`, which destroys the rig's data volume"). This is materially more
severe than every other `GUARDED_OPS` entry today (`rig reset`/`restore` are also
destructive but explicitly re-provision; this one has no re-provisioning path at all
short of a full volume wipe).
**Warning signs:** A user support request of the form "I redeclared the module and it
didn't come back" — this should be preventable entirely by clear confirmation prose,
not discovered in the field.

### Pitfall 3: The override file's delete branch is currently dead code
**What goes wrong:** A stale `compose.ign-modules.yml` survives indefinitely on disk
after the last module is undeclared, even though the RUNNING container correctly drops
the mount (confirmed live: env vars and the bind mount both vanish from the recreated
container on the very next `rig up`, because `ModuleProvisioning::default()`'s
`override_files()` is empty and the compose invocation genuinely omits `-f
compose.ign-modules.yml` — so the CONTAINER is right, only the FILE is stale).
**Why it happens:** Two independent short-circuits skip `write_override` entirely when
nothing is declared: `main.rs`'s `if merged_modules.is_empty() { ModuleProvisioning::
default() }` (never calls `provision_modules` at all), and `provision_modules`'s own
`if declared.is_empty() { return Ok(ModuleProvisioning::default()); }` (never calls
`write_override` even if it WERE reached). `write_override`'s "mounts empty -> delete"
branch is therefore only exercised by the unit test that calls it directly and by the
Phase 16 live-gate test, which deletes the file BY HAND before invoking `rig up` —
never by any code path a real user's `rig up` actually executes.
**How to avoid:** This phase must add an explicit delete-if-stale step reachable on the
undeclare path — either by removing the early-return short-circuits (calling
`write_override(plan, service, &[])` even when `declared` is empty, which requires
`service` derivation to also handle the "was declared, now isn't" case since today's
`gateway_service` resolution is itself skipped for an empty `declared` map) or by a
small dedicated cleanup call before/after the short-circuit that just checks
`existing_override(plan)` and removes it when the new declared set is empty. The
diff logic Phase 19 needs anyway (Q3/Q4) makes reading `existing_override` before
overwrite a natural place to also trigger this fix.
**Warning signs:** `rig down`/`rig status`/`rig logs` reading `existing_override(plan)`
and finding a file that no longer matches what the running container actually has
mounted — the doc comment on `existing_override` explicitly (and, per this session,
incorrectly) claims this "never accumulates stale state across cycles."

### Pitfall 4: Container recreate briefly breaks previously-adopted auth
**What goes wrong:** Immediately after a `rig up` that recreates the gateway container
(even on the SAME persisted named volume), the previously-adopted API token can 401 for
roughly 10-20 seconds before settling — observed twice in this session, both times on a
recreate that also involved a module state change (once removing a mount, once
re-adding one after an uninstall).
**Why it happens:** Not fully diagnosed live — plausibly the API-token subsystem's
internal DB table needs a moment to re-attach after container restart, independent of
module activity. Correlation with module changes specifically was NOT isolated (could
be true of any container recreate, module-related or not).
**How to avoid:** Any live test in this phase that recreates the container and then
immediately calls an authenticated endpoint should retry auth (or re-run `adopt`) with
a short backoff rather than treating the first 401 as fatal — the SAME
`healthy_module_ids` retry-loop pattern Phase 16's live gate already uses covers this
if the retry loop wraps auth failures too (today's `healthy_module_ids` in
`live_rig_module_injection.rs` does carry the error through the loop rather than
failing fast — good precedent to keep).
**Warning signs:** A live test flaking specifically right after a `rig_down`/`rig_up`
or a container-recreating `rig_up` call, with `auth_rejected` in the failure, not a
transport error.

## Code Examples

### The uninstall call, exactly as issued and answered live this session
```bash
# Source: this session's live probe against inductiveautomation/ignition:8.3.3,
# via the already-shipped `ign api call` escape hatch — no new code
ign --profile probe api call --method DELETE --path /data/api/v1/modules/uninstall \
  --data '{"uninstall": ["com.axone_io.ignition.git"]}' --json
```
Refused (module still mounted, read-only bind mount present):
```json
{"status": 200, "data": {"success": false, "failedUninstalls": {"uninstall": ["com.axone_io.ignition.git"]}}}
```
Succeeded (module's `.modl` already absent from `user-lib/modules` — a ghost registry
entry: `{"id": "com.axone_io.ignition.git", "name": "", "version": "", ...}` with no
`state` key at all, from `/modules/healthy`):
```json
{"status": 200, "data": {"success": true, "failedUninstalls": {"uninstall": []}}}
```
Bogus/never-installed id — SAME shape as "still mounted", no way to distinguish
"doesn't exist" from "refused to remove" from the response alone:
```json
{"status": 200, "data": {"success": false, "failedUninstalls": {"uninstall": ["com.nonexistent.totally-made-up"]}}}
```
Empty array — harmless no-op, still HTTP 200 `success:true`:
```json
{"status": 200, "data": {"success": true, "failedUninstalls": {"uninstall": []}}}
```
Batch of two failing ids — BOTH ids come back in `failedUninstalls`, single overall
`success:false` (batch-atomicity for a mixed success+failure batch was NOT tested live
this session — see Open Questions):
```json
{"status": 200, "data": {"success": false, "failedUninstalls": {"uninstall": ["com.inductiveautomation.reporting","com.nonexistent.made-up"]}}}
```

### ImportDenied — the exact precedent to mirror for the new variant
```rust
// Source: crates/ignition-core/src/error.rs:112-121 (read this session)
#[error("gateway rejected the project import for {project:?}: {problem}")]
ImportDenied {
    project: String,
    problem: String,
    endpoint: Option<String>,
},
```

## State of the Art

| Old Approach (assumed by RMOD-06/16-02) | Current Approach (this session's finding) | When Changed | Impact |
|--------------------------------------|---------------------------------------------|---------------|--------|
| Deleting the override "fully reverts module provisioning" | Deleting the mount stops the CONTAINER from loading it, but the gateway's own module registry keeps a ghost entry until `DELETE /modules/uninstall` is called explicitly | Falsified live in Phase 16 (16-03), root cause narrowed further in this session | RMOD-06's promise needs BOTH the uninstall call (new) and a fix to the override's own dead delete-path (newly found this session) to actually hold |
| "The next `up`... deletes [the override]... if nothing is declared" (modules.rs doc comment) | Confirmed FALSE live: two independent short-circuits skip `write_override` when nothing is declared | Found live this session (2026-09-25) | The doc comment itself is now a documented-but-wrong claim; Phase 19 should correct it in the same commit that fixes the behavior |

**Deprecated/outdated:** None — this is new-capability research, not a library
migration.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Uninstalling a module does not clean up its gateway-internal DB tables (`GITPROJECTSCONFIGRECORD` etc.) or `data/jar-cache/<id>/` — inferred from the jar-cache directory surviving a successful uninstall in this session's probe, but NOT independently confirmed for the DB tables (they were observed present both before AND after uninstall, but a byte-for-byte before/after diff of the DB was not performed). | Runtime State Inventory, State of the Art | Low — even if wrong, this is gateway-internal residue outside `ign`'s stated responsibility; at most the README's "what uninstall does" prose would need a caveat removed |
| A2 | A mixed batch (`{"uninstall": ["<succeeds>", "<fails>"]}`) either (a) partially succeeds — the removable id is removed despite overall `success:false` — or (b) is atomic and removes nothing when any id fails. NEITHER was directly observed; only two same-outcome batches (both-fail cases) were tested live. | Code Examples, Open Questions | Medium — if `ign` ever batches multiple orphaned-module ids into ONE uninstall call and assumes atomicity that doesn't hold (or vice versa), a partial failure could silently leave the CLI's reported result out of sync with gateway state. Mitigated by recommending ONE uninstall call per module id (see Q4) rather than batching, which sidesteps the ambiguity entirely. |
| A3 | The brief post-recreate 401 (Pitfall 4) is a general container-recreate artifact, not specific to module state changes. Only two data points, both involving a module change, were observed — a control case (recreate with NO module change) was not run. | Common Pitfalls (4) | Low — affects only live-test robustness (retry-loop design), not the shipped feature's correctness |

## Open Questions

1. **Batch-uninstall atomicity (A2 above).**
   - What we know: a batch containing two failing ids returns both in
     `failedUninstalls`, one overall `success:false`.
   - What's unclear: whether a batch with ONE succeeding and ONE failing id partially
     applies the succeeding one, or refuses the whole batch atomically.
   - Recommendation: sidestep it — the planner should have `ign` issue ONE uninstall
     call per orphaned module id (not a single batched call for the whole rig), and
     treat each independently. This also gives cleaner per-module error reporting and
     matches the granularity SC-1 through SC-4 are written at (per-module, not
     per-rig-batch).

2. **Does uninstall touch the gateway's internal DB tables / jar-cache (A1)?**
   - What we know: `data/jar-cache/<gateway_module_id>/` survives a successful
     uninstall (confirmed).
   - What's unclear: whether repeated install/uninstall cycles of the SAME module id
     accumulate jar-cache entries indefinitely (a potential unbounded-growth concern
     for long-lived rigs), since a re-mount after uninstall was shown NOT to reload the
     module at all in this session — so this specific growth path may be moot, but a
     provision -> uninstall -> re-provision-via-a-different-mechanism cycle was not
     tested.
   - Recommendation: out of scope for `ign` to manage (it is gateway-internal state
     `ign` never wrote), but worth one README line acknowledging it, so it is not later
     mistaken for a resource leak `ign` introduced.

3. **Recovery path after an accidental uninstall.**
   - What we know: re-declaring + `rig up` does not restore a module once uninstalled
     (confirmed live, Pitfall 2). `rig reset` (full volume wipe) would definitely work
     but is maximally destructive to everything else on the rig, not just the module.
   - What's unclear: whether the ALREADY-DOCUMENTED `POST /data/api/v1/modules/install
     ?moduleId=...` endpoint (seen in the 83-api Bruno collection, NOT probed live this
     session) can re-register a module without a full data-volume wipe.
   - Recommendation: leave out of Phase 19's scope (RMOD-06's amended half is about
     uninstall, not reinstall), but flag this explicitly in the confirmation prose so
     the user knows `rig reset` is today's only proven recovery path.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Docker | Rig lifecycle, live gate | Yes | 29.4.0 (orbstack, Linux-container backend) | — |
| Docker Compose plugin | Rig lifecycle | Yes | v5.1.2 | — |
| `inductiveautomation/ignition:8.3.3` image | Live probe, Phase 16 fixture reuse | Yes, cached locally | 8.3.3 | — |
| `inductiveautomation/ignition:8.3.6`/`8.3.9` | Cross-version confirmation of the uninstall contract | Yes, cached locally | 8.3.6 / 8.3.9 | Not probed this session on 8.3.6/8.3.9 — the endpoint's shape is a stable, documented REST route (83-api collection is version-agnostic); recommend at least ONE spot-check on 8.3.6 or 8.3.9 during Phase 19's own live gate, since Phase 16 established a precedent of gate-checking both a low and high pinned version |
| Network egress (GitHub, for module artifact fetch) | Re-provisioning the git module during the probe | Yes (confirmed: `curl -sI https://github.com` returned 200) | — | — |
| macOS Keychain | `adopt`'s credential persistence | Yes | — | — |

**Missing dependencies with no fallback:** None.

**Missing dependencies with fallback:** None — everything needed for both research and
the eventual live gate is present today.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]`/`#[tokio::test]`, `wiremock` for HTTP contract tests, a `--ignored` live-gate test for the gateway-observable behavior (the established Phase 16 pattern) |
| Config file | none — cargo-native |
| Quick run command | `cargo test -p ignition-core --lib` (unit/contract, no Docker) |
| Full suite command | `cargo test --workspace` (CI gate 4 of 5) |
| Live gate command | `IGNITION_LIVE_RIG_MODULES=1 cargo test -p ignition-core --test live_rig_module_injection -- --ignored --nocapture` (extend this file, per the phase's own "Read first" instruction) OR a new sibling `live_rig_module_uninstall.rs` if the sequence gets long enough to warrant separation — planner's call |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| SC-1 | A module the rig no longer declares ends ABSENT from `/modules/healthy` | live (gateway-observable state; no unit test can substitute — this is the phase's own stated reason for existing) | `IGNITION_LIVE_RIG_MODULES=1 cargo test -p ignition-core --test live_rig_module_injection -- --ignored --nocapture` (extended) | Extend existing file |
| SC-2 | Uninstall NEVER happens without `--yes`; refuses exit 2 `confirmation_required` | unit (CLI dispatch, the `require_confirmation` precedent) | `cargo test -p ignition-cli --test <new_or_existing>_contract` | New test, existing pattern (`sessions terminate`'s exit-2-without-`--yes` golden is the template) |
| SC-3 | `rig up` with no module changes issues NO uninstall request | unit (wiremock `.expect(0)` on the uninstall route) | `cargo test -p ignition-core --lib rig::modules::tests` or a new `tests/module_uninstall_contract.rs` | New test; `crates/ignition-core/tests/module_fetch_contract.rs`'s `.expect(0)` / `mount_as_scoped` pattern (read this session, lines ~199-230) is the exact precedent to copy |
| SC-4 | A module `ign` never declared through it is NEVER uninstalled | unit (seed a gateway-installed module `ign` did not provision; assert the diff logic excludes it) + live (the roadmap phase text explicitly calls for seeding one) | live-gate extension; unit test on the diff function in isolation | New tests on both layers |

### Sampling Rate
- **Per task commit:** `cargo test -p ignition-core --lib` (fast, no Docker)
- **Per wave merge:** `cargo test --workspace` (CI gate 4/5) + `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`,
  `cargo build -p ignition-cli --no-default-features` — all FIVE gates from
  `.github/workflows/ci.yml` (confirmed this session: `fmt`, `clippy`, `build
  --workspace`, `test --workspace`, `build -p ignition-cli --no-default-features`).
  `.planning/WINDOWS.md` entry id=4 records why all five, not just build/test/clippy,
  must be enumerated in the plan's `<verification>` section — PR #9 went red on `fmt`
  across all three platforms when a plan only listed three of the five.
- **Phase gate:** Full suite green + the live gate (`IGNITION_LIVE_RIG_MODULES=1`)
  green before `/gsd-verify-work`, per SC-1's own "proven live" requirement.

### Wave 0 Gaps
- [ ] A wiremock contract test file for the uninstall route's request/response shape
      (new — no existing file covers `/modules/uninstall`)
- [ ] The diff function (`orphaned_modules` or equivalent) needs its OWN unit tests
      independent of any gateway call — pure logic, cheap to cover exhaustively
      (empty->empty, declared->empty, declared->different-declared, two-module rig
      where only one is undeclared)
- [ ] Live-gate extension to `live_rig_module_injection.rs` (or a new sibling file) —
      §D/§E continuing the existing §A/§B/§C numbering, per the file's own convention

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-------------------|
| V2 Authentication | Indirect | Uninstall rides the SAME `GatewayApi` auth path every other authed call uses (token header, resolved via the profile) — no new auth surface |
| V3 Session Management | No | Stateless API-token auth per existing pattern; no session concept here |
| V4 Access Control | Yes | The `--yes` confirmation gate IS the access-control control for this phase — it is the ONLY thing standing between "config edit" and "gateway module removal." `GUARDED_OPS` is the established mechanism; no new mechanism needed |
| V5 Input Validation | Yes | The module id passed to the uninstall call must be validated the SAME way `provision_modules`/`merge_declarations` already validate ids (`validate_module_id`) — reuse, do not invent a second validator |
| V6 Cryptography | No | Not applicable — no new crypto surface |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|----------------------|
| A malicious or buggy diff removes a module the user DID intend to keep, because the diff logic misreads the override's previous state | Tampering / Repudiation of intent | Confirmed-live design constraint: read the PRE-overwrite override deterministically (`existing_override(plan)`) BEFORE any write, never infer "previously declared" from anything else (e.g. never from the RUNNING container's current mounts, which can already be stale per Pitfall 3) |
| A config file edited by an unauthorized party silently triggers an uninstall on the next `rig up` if the "report only" recommendation is NOT followed and uninstall becomes automatic | Elevation of Privilege | This IS the reasoning behind the roadmap's own locked decision (confirmation required) and this research's Q4 recommendation (report-only default, deliberate separate verb for the actual removal) |
| Uninstalling a module whose id was typo'd/never installed is silently accepted as a no-op-that-looks-like-success by the CALLER, if `ign` does not check `failedUninstalls` | Repudiation | Directly covered by Pitfall 1 — the response body MUST be parsed, not just the status code |

## Sources

### Primary (HIGH confidence — verified live this session)
- Live probe against `inductiveautomation/ignition:8.3.3` (Docker 29.4.0/orbstack,
  Compose v5.1.2) via the Phase 16 fixture
  (`crates/ignition-core/tests/fixtures/live-rig/compose.yml`) and the already-built
  `ign` binary (`adopt`, `rig up`, `modules`, `api call`) — the entire uninstall
  contract (request/response shape, immediacy, the "ghost entry" mechanism, the
  re-declare-does-not-restore finding, the override dead-code finding) came from this
  session's direct interaction with a real gateway, torn down unconditionally
  (`docker compose ... down -v`, keychain entries deleted, `docker ps -a`/`docker
  volume ls` confirmed zero leaks afterward).
- `crates/ignition-core/src/rig/modules.rs` (read in full this session) —
  `provision_modules`, `write_override`, `existing_override`, `ModuleProvisioning`,
  `MountedModule`, the D-08 tri-state doc comments (and where they diverge from
  observed behavior).
- `crates/ignition-cli/src/main.rs` (read: `Commands::Rig` dispatch, `GUARDED_OPS`,
  `require_confirmation`) — the exact guard mechanism and the `merged_modules.
  is_empty()` short-circuit that causes Pitfall 3.
- `crates/ignition-core/src/error.rs` (read: `CoreError` enum head, the exit-code
  table doc comment, `ImportDenied`, `ConfirmationRequired`) — the Three-Place rule and
  the exact precedent to mirror for a new variant.
- `crates/ignition-core/src/client/mod.rs` + `status.rs` (read) — `GatewayApi::
  modules`, `ModuleInfo`, `MODULES_HEALTHY_PATH`/`MODULES_QUARANTINED_PATH`.
- `crates/ignition-core/src/module/mod.rs` (read) — `ModuleSpec`, `GIT_MODULE`,
  `PROJECT_SCAN_ENDPOINT`, `gateway_module_id` values, confirmed byte-for-byte against
  the live probe's own request/response.
- `~/whiskeyhouse/83-api/bruno/Ignition HTTP API/modules/Uninstall Module[s].bru` (read)
  — confirms the route/method/skeletal body shape (but NOT the id semantics or response
  shape, which the live probe supplied).
- `crates/ignition-core/tests/live_rig_module_injection.rs` (read in full) — the
  harness this phase extends; its `adopt_and_build_session`, `healthy_module_ids`
  retry-carrying-error pattern, and unconditional-teardown structure.
- `.planning/phases/16-compose-override-module-injection/16-03-SUMMARY.md` (read) — the
  original SC-4 falsification this phase exists to close.
- `.github/workflows/ci.yml` (read: the `check` job's five `run:` steps) and
  `.planning/WINDOWS.md` (read in full — the id=4 entry).
- `.planning/ROADMAP.md` (read: Phase 19's full entry under `## Phase Details`) — the
  four success criteria and the two locked/open planner notes.

### Secondary (MEDIUM confidence)
- None beyond the primary sources above — this research relied on direct verification
  rather than documentation lookup, since the phase's own stated reason for existing is
  that documentation/tests cannot observe gateway-installed-module state.

### Tertiary (LOW confidence)
- None.

## Metadata

**Confidence breakdown:**
- Endpoint contract (request/response shape, immediacy, ghost-entry mechanism): HIGH —
  live-verified against a real gateway multiple times with multiple input shapes
  (still-mounted, ghost/orphaned, stock module, bogus id, empty array, mixed batch).
- Override dead-code finding (Pitfall 3): HIGH — live-verified by direct file
  inspection and container inspection across two full undeclare-then-up cycles.
- CLI verb shape (report-only vs. inline-uninstall): MEDIUM — this is the roadmap's own
  explicitly "open for the planner" design choice, not a locked decision; this research
  argues for report-only using this repo's own guard conventions as evidence, but it is
  a recommendation, not a verified fact.
- Batch-uninstall atomicity: LOW — not independently tested; sidestepped via a
  per-module-call recommendation rather than resolved.
- Recovery-path completeness (Q3): LOW — `/modules/install` was found in the Bruno
  collection but not probed live.

**Research date:** 2026-09-25
**Valid until:** 30 days (stable REST contract on a pinned image tag; re-verify if the
phase slips past an Ignition 8.3.x point release the fixture doesn't pin, or if the
fixture's image tag changes)
