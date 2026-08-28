# Phase 7: Ecosystem Interop & Advanced Ops - Research

**Researched:** 2026-08-28
**Domain:** cross-gateway project diff/sync, EAM task orchestration, scriptExec surface, Ignition Flint script codec, external tool delegation (ignition-lint, ignition-git-module, ignition-nvim)
**Confidence:** HIGH — EAM/backup/script-export claims verified against a **live 8.3.3 gateway** (ignition-devops rig, port 9088; headless token provisioned per the STATE.md recipe; trimmed openapi extract committed at `07-openapi-extract.json`); script codec + tag-export formats verified against the **actual sibling-repo sources** (ignition-nvim `encoding.py`/`json_scanner.py`, ignition-git-module `GitTagManager.java`, ignition-lint `cli.py`); CLI-side claims verified against the shipped code (webdev/mod.rs, actions/webdev.rs, client/backup.rs, client/resources.rs, actions/resources.rs, actions/tags.rs, cli.rs).

## Summary

Phase 7 is a **composition phase**: nearly every wire seam it needs already exists in the codebase or was live-proven during this research. The four deliverable clusters decompose into (a) new orchestration over shipped machinery (cross-gateway sync = the resource-put surgery pattern with a second client; standalone backup = Phase 4 client methods with a new action surface), (b) genuinely new wire surface that this research **de-risked live** (EAM REST — including the controller-mode state gate, the config-resource task-definition taxonomy, and a full force→history execution round on a trial rig), (c) a CLI verb over the already-shipped secret-gated scriptExec route (wire contract read from the route source), and (d) three interop bridges whose contracts are pinned by local sibling repos (Ignition Flint escape table from ignition-nvim; git-module on-disk tag formats from its Java source; ignition-lint's argparse surface from its CLI).

The single most scope-shaping live finding: **EAM's runtime endpoints all 403 with "This operation can only be performed when EAM is configured as a controller" on a stock gateway**, and the fix is a config-resource PUT flipping `module-settings.installMode` to `Controller` — live-proven on the trial rig, after which task definitions, force dispatch (204), and history all answered. But **task execution requires a Gateway-Network-connected agent target** (even controller-self backup failed with "Gateway network for agent '_controller' is currently not connected") and **trial expiry blocks execution** ("Trial timer is expired" — tier-0 token trial reset works on 8.3.3 and unblocks it). EAM on the CLI should therefore be honestly read-heavy: history/scheduled/definitions as first-class reads, guarded create + force as the guarded writes, with the GNT-agent requirement surfaced as data, not hidden.

The second scope-shaping finding: **`ignition/script-python` resources already export as plain-text `script.py` zip members** (live-proven byte-exact round-trip). The decode/encode story (INTR-01) is therefore about **embedded scripts inside JSON resources** (Perspective `view.json` component scripts, etc.) — the "Ignition Flint encoding" whose exact escape table, decode state machine, and dedent/reindent semantics are already implemented twice (Lua + Python) in ignition-nvim with the explicit invariant `encode(decode(x)) == x`.

**Primary recommendation:** Four plans as sketched, with 07-01 (diff/sync) treated as the heavyweight (new two-client resolution shape + member-level compare with resource.json normalization + sync via existing surgery), 07-02 splitting backup-standalone (small) from EAM (medium, new client/actions family), 07-03 as the smallest (route contract already pinned; ride require_routes + the persisted secret), and 07-04 keeping its three loosely-coupled deliverables as one task each. Every plan grows the known choke files; **every new CLI verb must land its TUI routes row in the same plan** (tui_coverage walks the live clap tree — CI fails otherwise).

## User Constraints (from STATE.md — LOCKED)

No CONTEXT.md exists for this phase (no /gsd-discuss-phase was run). There are no locked *user* decisions beyond the accumulated project decisions in STATE.md, which are LOCKED and constrain all Phase 7 work:

- **Output envelope FROZEN:** `{ok,profile,data}` / `{ok,profile,error}`; exit taxonomy 0–7 with stable slugs; additive-only new slugs; exit-code table lives in exactly two places.
- **WebDev seam:** `POST /system/webdev/{project}/cli/{route}`, 200-BODY envelope is the only success oracle; probe matrix 405=Absent, 402=Unlicensed, 401/403=AuthGated; `require_routes` precondition (405→`routes_not_deployed`, 402→`webdev_unlicensed`, mismatch→`route_version_mismatch`) before every webdev-dependent action.
- **scriptExec route already shipped (Phase 5):** deployed ONLY via `--with-script-exec`; /dev/urandom 32-byte hex secret baked by template substitution, persisted 0600 in profile BEFORE upload; fail-closed constant-time gate; require-auth:false secret-only posture; route machine codes `no_alarm_journal, secret_required, secret_mismatch, unknown_action, not_found, route_error`.
- **gwbk machinery exists (Phase 4):** `download_to_file` with optional Accept param; roaming query on path constant; restore POST = raw octet-stream with 4 explicit-false params (NOT multipart); restore wait deadline MAX-clamps at 300s; snapshot/restore creds = IGNITION_TOKEN only.
- **Resource surgery (05-02/05-07/06-08):** export → zip member surgery → import(overwrite=true); put-new landing requires parent folder resource.json descriptor listing basename; `validate_import` walks+decompresses every member; root-level paths map to member `<X>/resources/<X>`.
- **Collision conventions:** abort default = pre-check refusal (exit 6) before any write; overwrite = --yes-guarded, no pre-check; server is authority.
- **Destructive verbs --yes-guarded with guard firing BEFORE resolution** (exit 2, profile null). Currently guarded: sessions terminate, project delete, import-overwrite, resource put/delete, provider delete, config delete, rig reset/restore/trial-reset.
- **TUI coverage:** `tui_coverage.rs` walks the live clap tree — a future CLI command without a TUI mapping FAILS CI (**Phase 7's `script run` — and every other new verb — must land its TUI surface in the same plan**; recorded Phase 6 decision).
- **Choke files:** client/mod.rs trait/impl block + cli.rs/main.rs/render.rs per capability plan; honest parallelism only when files don't overlap.
- **Edition 2024, MSRV 1.88, zip crate, snapbox goldens, wiremock-first unit tests with recorded-request proofs.**
- Live rigs: ign-research (8.3.6, currently DOWN), ignition-devops (8.3.3, UP — used for this research); headless token provisioning recipe proven (used live during this research; the token value is `NAME:key` under `X-Ignition-API-Token`).

## Focus 1: Cross-Gateway Diff & Selective Sync (SYNC-01/02)

### Comparison basis — export ZIPs, member-level, normalized (HIGH confidence)

The only resource-level truth source is the project export ZIP (triple-verified in Phase 5: no per-resource REST routes exist). Diff algorithm:

1. Export both projects (one per profile) to temp zips via the existing `project_export_to_file` — reuses the streaming seam verbatim.
2. Enumerate members with the existing `resource_members()` helper; map to user paths with the existing `user_path()` (`<collection>/<rest>`; no-slash root files included).
3. Per-member compare with **resource.json normalization**: strip `attributes.lastModification` and `attributes.lastModificationSignature` before hashing. **Live-evidenced volatility:** every gateway-written resource.json carries these (see the tagConfig resource.json in the ign-cli export — `lastModification.actor/timestamp/signature`), so byte-diff would report false CHANGED for identical content exported from two gateways. Keep `scope/version/files` (semantic).
4. `project.json`: compare semantically-relevant fields (title/enabled/parent) or exclude from resource diff and surface as a separate `project_meta` section — recommend exclude + separate section (it's not a resource).
5. Statuses per member: `added` (in B not A — careful with direction semantics), `removed`, `changed`, `same`. Direction: `ign project diff <a> <b>` reports "what it takes to make B like A" OR "B relative to A" — **must be pinned in the plan** (recommend: output is B-relative-to-A, `added` = in B only; sync direction is always explicit A→B in the sync verb).

Output shape: JSON = `{scope: "project", profile_a, profile_b, project, summary: {same, added, removed, changed}, entries: [{path, status}]}` — flat agent shape, all keys always (family convention). Human = grouped status sections + summary line. The **scope field is mandated** by the success criterion ("project-vs-tag-provider scope explicit in command semantics and output metadata").

### Selective sync — resource surgery across clients (HIGH confidence)

`ign project sync <a> <b> --resource <path>... [--all-changed] [--yes]`:

- Export B (target), for each selected resource read the member bytes from A's export, `replace_member` into B's zip (the exact put-new descriptor-merge rules from 05-07 apply — a member landing in a folder whose resource.json doesn't list it silently no-ops), then `project_import(overwrite=true)` into B.
- This is `resource put` generalized to two clients — the orchestration in actions/resources.rs is the template. Sync is implicitly a whole-project overwrite-import on B: **--yes-guarded, consequence-naming refusal** (the resource-put message pattern).
- Deletion sync (resource in B absent in A) = `remove_member` — include only with an explicit `--delete` flag; default sync is additive/upsert-only (safer dev→prod promotion semantics).
- Byte-exact zip equality is not deterministic across writers — member-level honesty is the contract (05-02 lesson); diff/sync tests assert at member level by round-tripping bodies through the public helpers.

### Scope honesty (HIGH confidence)

- **Project scope only for v1 diff/sync.** Tag providers live on a different seam (config-resource REST for provider CRUD; tags route for config export/import). A tag-provider *diff* is cheap (provider lists + `tags export` compare) but provider *sync* = provider create + tags import — different machinery and collision semantics (tag_collision pre-checks from 05-05).
- Recommendation: `ign project diff/sync` carries `scope: "project"` in output metadata; README documents the tag-promotion pipeline (`ign tags export -o - | ... | ign tags import --file -` across profiles — already expressible with shipped verbs). A `--include tag-providers` diff-only flag is a reasonable v1.5; keep sync project-scoped.

### Two-client resolution — a NEW shape (HIGH confidence, design decision for planner)

The CLI resolves exactly ONE active profile per command (main.rs CORE-01). `project diff <profile-a> <profile-b>` needs two clients with independent secret chains. Constraints: the frozen envelope has ONE `profile` field. Recommendation: envelope `profile` = the active profile (resolution unchanged); `data` carries `profile_a`/`profile_b` explicitly. Positional profile names resolve via the same `resolve_selection(use_profile=…)` machinery per side; each side's secret chain resolves independently (env tokens override per-side via existing `--profile` semantics — document that `IGNITION_TOKEN` env applies to BOTH sides unless per-profile keyring entries exist; this is honest and worth a README note).

## Focus 2: EAM Tasks (BKUP-02) — live-proven

### The real 8.3 endpoints (HIGH confidence — openapi extract + live calls)

**Runtime surface** (`/data/eam/api/v1/…`, all standard `{items, metadata}` list envelope where list-shaped):

| Endpoint | Method | Notes |
|---|---|---|
| `/eam-tasks/history` | GET | list params (limit/offset/sortBy/search/filter incl. `field[op]=value`) — item shape live-captured: `{taskId, taskName, taskStart, taskEnd, target, level, detail, taskType}` (times epoch-ms; `level` e.g. `Failed`; `taskName` carries `" (forced)"` suffix on forced runs) |
| `/eam-tasks/scheduled/{running}` | GET | running=true/false split |
| `/eam-tasks/retry` | GET | retry-queue view |
| `/eam-tasks/force/{owner}/{name}` | POST | → **204 live**; owner = `eam` (from the task healthcheck's `scheduledTaskState.details.owner`) |
| `/eam-tasks/cancel/{name}` | POST | |
| `/eam-tasks/suspend/{name}` · `/resume/{name}` | POST | |
| `/eam-tasks/clear-retry/{name}` | DELETE | |
| `/eam-tasks/store-gwbk` · `store-module` · `store-module-from-archive` · `store-upgrade-zip` | POST | EAM archive ingest |
| `/storage/archived-backups` · `/archived-modules` | GET | **requires `serverids` query param** (400 without — live) |
| `/agents` · `/agents-by-group` · `/agent-details/{serverid}` · `/agent-licenses/{serverid}` · `/agent-approval-status/…` · `/quarantined-agents` | GET | fleet inventory |
| `/approve-agent/{serverid}` · `/upgrade-agent/…` | POST | |

**Task definitions are config resources** (`/data/api/v1/resources/com.inductiveautomation.eam/eam-tasks`): POST/PUT take ARRAY bodies; list/find/names follow the standard config-resource family (the tag-provider pattern from 05-04 rides again — `find` returns the definition + a `scheduledTaskState` healthcheck with `currentState/nextScheduled/owner`).

**Task taxonomy (openapi enum, live-accepted):** `profile.type` ∈ {`eam_activateLicense, eam_backup, eam_installModules, eam_remoteUpgrade, eam_restart, eam_restoreBackup, eam_sendProject, eam_sendResource, eam_sendTags, eam_unactivateLicense, eam_updateLicense`}; `profile.scheduleMode` ∈ {`Immediate, AtTime, AtDelay, Scheduled, SuspendedByFailover, OnDemand`}. Live-captured `eam_backup` settings: `{targetGateways: [], targetGroups: [], concurrentBackups: 0, forceBackups: false}` (+validation: "Either targetGateways field or targetGroups field must contain at least one value, unless controllerIsTargetKey is true").

### The state gate + availability (HIGH confidence — live-proven)

- Stock gateway: module jar `com.inductiveautomation.eam` IS installed but `module-settings` singleton has `installMode: "NotInstalled"` → every `/data/eam/api/v1/*` answers **403 "This operation can only be performed when EAM is configured as a controller."** — a *state* refusal, not auth. Flipping via config-resource PUT (`installMode: "Controller"`, array body with current signature) is **live-proven** and instantly activates the runtime surface.
- **Trial rigs CAN run EAM** (this research ran it), but: (1) task execution is trial-gated — "Cannot run task … Trial timer is expired" (tier-0 token trial reset works on 8.3.3 and unblocks); (2) **execution needs a GNET-connected agent target** — even `"_controller"` self-target failed: "Gateway network for agent '_controller' is currently not connected, the connection status is 'NotDefined'". The history/detail field honestly reports this per-run.
- CLI mapping: additive exit-6 slug `eam_not_controller` on the 403 message (message-classified, like `trial_not_expired`); do NOT auto-flip installMode from the CLI (that's a gateway-role decision, README-documented; the research rig flip was manual via curl).

### Guarded create — what "guarded" means (recommendation)

- Expose `eam-tasks` definition create/update as config-resource CRUD with `scheduleMode: OnDemand` as the CLI default (never auto-fires — no schedule); refuse `Immediate`/`Scheduled` without `--yes` (they arm autonomous gateway actions).
- Task types: allow the read-adjacent/benign set by default (`eam_backup`); `--yes` for `eam_restart`, `eam_sendProject`, `eam_sendResource`, `eam_sendTags` (they mutate target agents); refuse-or-guard `eam_restoreBackup`, `eam_installModules`, `eam_remoteUpgrade` (destructive fleet ops). Force/suspend/cancel verbs: force = --yes-guarded (executes now), suspend/cancel = unguarded state-advancing (the alarms-ack precedent).
- **EAM restore vs native restore:** different axes. Native `POST /data/api/v1/backup` (Phase 4) restores THIS gateway from bytes. EAM `eam_restoreBackup` is a fleet-dispatch task pushing an archived backup to AGENTS. Both are legitimate; the CLI should name them distinctly (`ign backup restore` vs `ign eam task force`).

## Focus 3: `script run` (SCRPT-01) — route contract already pinned

### Wire contract (HIGH confidence — read from the shipped route source)

`POST /system/webdev/ign-cli/cli/scriptExec` with header `x-ignition-cli-secret: <hex>` (or `Authorization: Bearer <hex>` — dual-extract in-route), JSON body:

- `{"action": "version"}` → `{ok, data: {routeVersion: "1.0.0", minCli: "1.0"}}`
- `{"action": "exec", "code": "<python>"}` → `{ok, data: {stdout: "<captured>", result: <jv-walked value>, elapsedMs: <int>}}`

Semantics (route source): single-expression code is `eval`'d and its value returned as `result`; statement code is `exec`'d with an optional `_result` global surfaced; stdout is captured and restored; every invocation is audit-logged (sha256-prefix + elapsedMs) via `system.util.logger('ign-cli-scriptexec')`. Errors: `{ok:false, error:{code, message, traceback?}}` at HTTP 200; codes `secret_required | secret_mismatch | unknown_action | route_error`.

**No timeout parameter exists in route v1.0.0** — client-side request timeout only (recommend the existing per-request class; document that a long-running script holds the HTTP connection). No project-scope param either (scripts run gateway-scoped in the WebDev project context).

### Opt-in semantics (HIGH confidence)

The existing `--with-script-exec` deploy flag IS the opt-in — structurally: route not deployed → `require_routes`-style probe 405 → precondition refusal with actionable message; route deployed → the persisted profile secret + fail-closed gate is the auth. **No additional config gate needed.** The deploy/status machinery already probes scriptExec with the secret header when one is configured (actions/webdev.rs `SCRIPT_EXEC_ROUTE` + `SECRET_HEADER` constants exist). `ign script run` should:

1. Resolve the profile's `webdev_secret` (missing → additive exit-6 slug naming `ign webdev deploy --with-script-exec`).
2. Run the scriptExec-specific precondition (version handshake with secret; map `secret_required`/`secret_mismatch` honestly — a mismatch means deployed-elsewhere/stale secret, message already says redeploy or --rotate-secret).
3. POST exec; surface `{stdout, result, elapsedMs}` as data (all keys always).

**No --yes guard** (recommendation): the opt-in is structural (deploy), the verb is the route's entire purpose, and agents need it non-interactive; README documents the shared-secret threat-model note (already written in Phase 5). Secret never appears in output (redaction discipline already proven at action AND binary level). `--code <str>` + `--file <path>` (+ stdin) input forms; `--file -` reads stdin.

**TUI surface is mandatory in the same plan** (clap-walk CI gate) — an input-modal → worker → result-modal flow on the existing one-shot pattern.

## Focus 4: decode/encode scripts (INTR-01) — the Flint codec

### What's already plain vs what needs decoding (HIGH confidence — live-proven)

- **`ignition/script-python` project-script resources export as plain-text `script.py` members** beside their resource.json (`files: ["script.py"]`) — live-proven byte-exact round-trip (imported a script resource, re-exported, identical bytes). **No decode needed for these** — unzipping the export already yields editable .py files; ignition-lint's `--project` mode lints `ignition/script-python` directly.
- **Embedded scripts inside JSON resources** (Perspective view.json component/event scripts, tag event scripts, etc.) are string values under known keys, escaped with the "Ignition Flint encoding". This is the decode/encode target.

### The codec — exact contract from ignition-nvim (HIGH confidence — source-ported twice, Lua + Python, shared vectors)

Encode = ordered multi-pass replacement, **backslash first**: `\` → `\\`, `"` → `\"`, tab → `\t`, backspace → `\b`, newline → `\n`, CR → `\r`, FF → `\f`, `<` → `\u003c`, `>` → `\u003e`, `&` → `\u0026`, `=` → `\u003d`, `'` → `\u0027`. Decode = **single-pass state machine** (multi-pass replace cannot distinguish `\\t` from `\t`; unknown `\uXXXX` escapes keep the backslash). Invariant: **`encode(decode(x)) == x` is sacred** (ignition-nvim's own doc comment). Scripts are stored with leading tab indentation; ignition-nvim `dedent()` strips the common leading-tab prefix (returns `(text, indent_prefix)`) and `reindent()` restores it on save — only non-empty lines reindent.

**Script-bearing JSON keys** (must stay in sync with `lua/ignition/json_parser.lua` SCRIPT_KEYS): `script, code, eventScript, transform, onActionPerformed, onChange, onStartup, onShutdown, expression`. Detection heuristic: `is_encoded_script` (contains `\n`, `\t`, `\"`, or `\u00`).

### Transformation design (recommendation)

Zip-member-level transform, pure functions in the client/resources.rs style (zip bytes in → decoded tree out, .py files + manifest in → zip bytes out):

- `--decode-scripts` on export: after download, walk every `.json` member; parse; walk values; for each string under a SCRIPT_KEY (recursively — scripts nest deep in view JSON), decode + dedent → write sidecar `<member-relative-path>.<index-or-key-path>.py`; emit a manifest (JSON-pointer-style address → sidecar path + indent prefix) so encode is deterministic. Leave the original JSON member in place (marker-free — encode reads the manifest, not markers; safer against partial edits).
- `--encode-scripts` on import: read manifest + sidecars, reindent + re-encode, splice values back, re-zip. Missing sidecar for a manifest entry = use the JSON's current value (never silently drop edits); manifest itself is not uploaded (strip before import).
- **Round-trip fidelity hazards:** (1) serde_json's default map is sorted — re-serialized JSON will reorder keys vs the gateway's pretty-printed export. Either enable serde_json `preserve_order` and re-serialize with identical formatting (fragile) or do **span-level value splicing** (parse to locate, splice strings — the ignition-nvim approach generalized). Recommend: parse with `preserve_order` for addressing, but perform replacement at the raw-string span when possible; **the acceptance test is byte-equality of the re-encoded zip member for unedited scripts** (the sacred invariant at file level). (2) `expression` values are often single-line Ignition expressions, not Python — decode heuristics must not mangle them (ignition-nvim treats expressions as always-valid; the CLI should decode only values that look like scripts — multi-line or containing `\n`).

## Focus 5: `ign lint` delegation (INTR-02)

### ignition-lint CLI surface (HIGH confidence — cli.py source + pyproject)

Binary: `ignition-lint` (console script, package `ignition-lint-toolkit`, repos TheThoughtagen/ignition-lint; also `ignition-lint-server` LSP + `ignition-lint-action` CI entry — delegate to `ignition-lint`).

- Modes (one required): `--project <dir>` (standard Ignition layout — lints `com.inductiveautomation.perspective/views` + `ignition/script-python`), `--target <dir>` (recursive view.json + .py), `--files <patterns>`.
- Key flags: `--report-format text|json` (json = `{issues: [{severity, code, message, file_path, component_path, component_type, line_number, column, suggestion}], summary}`), `--fail-on <severity>` (default error), `--profile` / `--checks` (perspective,naming,scripts), `--schema-mode robust|permissive`, `--component-style/--parameter-style[-rgx]`, `--allow-acronyms`, `--ignore-codes`, `--ignore-file`, `--check-linter` (asset self-test), `-v`.
- **Exit codes:** 0 = clean; 1 = findings at/above `--fail-on` threshold OR usage errors (missing/invalid path, no mode flag). stderr carries emoji-prefixed diagnostics; the report rides stdout.

### Delegation design (recommendation)

`ign lint [--json|--compact already global] [-- target passthrough args…]` → locate `ignition-lint` on PATH (`which`-style search); **absent → additive exit-6 slug `lint_tool_absent` (or usage-class exit 2) with actionable hint**: `uv tool install ignition-lint-toolkit` / `pip install ignition-lint-toolkit` (repo URL). Present → spawn with mapped args (default `--target <path>` for a directory arg, or pass-through), capture stdout/stderr.

**Exit-code semantics — the one real design tension** (open question below): the frozen taxonomy has no "lint findings" code, and doctor's precedent says "the diagnosis completing is success; failing checks are data." Recommendation ladder: (1) default = doctor posture — `ign lint` exits 0 whenever the child RAN, findings + `child_exit_code` + parsed JSON report in data (agents get everything); (2) `--strict` flag flips to literal child-exit passthrough for CI pipelines (documented as the sanctioned exception shape). Spawn discipline: tokio process (the rig/compose precedent), no shell interpolation, arg vector.

## Focus 6: `--from-export` tag browsing (INTR-03)

### git-module on-disk formats (HIGH confidence — GitTagManager.java source)

Repo layout: `<project>/tags/<provider>/…` with per-project config `<project>/tags/.tag-config.json` (`includedProviders`, `excludedTagPaths`, `collisionPolicy` o/m/d; `System` provider always excluded). **Two on-disk formats, auto-detected on import:**

1. **Individual files (new):** one `.json` per leaf tag in a directory hierarchy mirroring the tag tree; folders = directories; **UDT definitions at provider root under `_types_/*.json`**; the tag's `name` field is stripped (encoded in filename); JSON = Ignition's native TAG_GSON deterministic copy (`JsonUtilities.createDeterministicCopy`) — the same native format as `system.tag.exportTags` and the CLI's own tags-export interchange.
2. **Legacy single-file:** one `<provider>.json` per provider containing the whole tree.

**Filesystem name encoding:** `%XX` hex escapes for reserved chars `<>:"/\|?*` + control chars + `%` itself (round-trips on every OS).

### Offline browse design (recommendation)

`ign tags browse --from-export <path>` accepts either a **JSON file** (the CLI's own `tags export` output — the normalized list-of-subtrees interchange — or a legacy `<provider>.json`) or a **directory** (git-module layout: detect `tags/` root or a directory of provider folders/`.json` files; dot-entries skipped per the module's own rule). Parse to the existing `BrowseRow` shape (`path` = bracketed fullPath `[provider]a/b`, `tag_type` from `tagType`, `has_children` from child presence) and reuse the human tree renderer + flat JSON shape verbatim — **no gateway, no route precondition, no credential** (output metadata: `source: "export"`, `origin: <path>`). Provider name: file stem (legacy/CLI export) or the directory under `tags/`. Read-family semantics only — offline browse is a read.

## Focus 7: BKUP-01 gap analysis — standalone backup verbs

Phase 4 shipped `backup_download`/`backup_restore` client methods used by `rig snapshot/restore`. The delta for standalone `ign backup download|restore` on the ACTIVE profile (arbitrary gateways, not rig-scoped):

- **Actions + CLI surface only** — the wire is done. `actions/backup.rs` (or extend an existing family file): `ign backup download [-o FILE]` (roaming default; file default via Content-Disposition/`<host>-backup.gwbk` per the export convention) and `ign backup restore <FILE> [--yes]` — **restore is the 8th --yes-guarded destructive verb** (binary-pinned guard-before-resolution), with the post-restore restart-block window documented (Phase 4 Pitfall 6: 300s class, MAX-clamp).
- **Non-roaming:** `--type roaming|all` param-izes `BACKUP_DOWNLOAD_PATH` (currently a const with `?type=roaming` baked in — one honest signature change; `all` includes gateway-specific state).
- **Backup listing does NOT exist natively** (openapi: exactly GET/POST on `/data/api/v1/backup`) — no `ign backup list` verb; the EAM `storage/archived-backups?serverids=` listing is the EAM-archive store, a different thing (belongs to the EAM family if exposed at all). Say so in the README rather than inventing a verb.

## Standard Stack

### Core (no new dependencies required for the MVP set)

| Library | Version | Purpose | Why Standard |
|---|---|---|---|
| zip | 8.6 (in workspace) | export-zip member surgery (diff/sync/codec) | Already the surgery engine (05-02) |
| reqwest/tokio/serde_json | existing | EAM + backup + scriptExec clients | Existing seams |
| tokio::process | existing | `ign lint` child spawn | The rig/compose precedent (run/run_streaming seams) |

### Supporting (optional, only if text diffs land in scope)

| Library | Version | Purpose | When to Use |
|---|---|---|---|
| similar | 2.x | unified text diffs of decoded members for `project diff --text` human mode | Only if the planner scopes human content diffs; member-status MVP needs zero new deps |

**Recommendation: zero new deps for the four-plan MVP.** `similar` only as a scoped add-on.

## Architecture Patterns

### Recommended structure (choke-file deltas per plan)

```
crates/ignition-core/src/
├── client/
│   ├── eam.rs            (new — runtime endpoints + task-definition config-resource family)
│   ├── backup.rs         (grow — type param + standalone action surface)
│   ├── resources.rs      (grow — diff helpers: normalized member compare, pure)
│   └── scripts_codec.rs  (new, 07-04 — Flint encode/decode + dedent/reindent + sidecar manifest, PURE)
├── actions/
│   ├── projects.rs       (grow — project_diff, project_sync)
│   ├── eam.rs            (new)
│   ├── backup.rs         (new)
│   ├── script.rs         (new — script run)
│   └── tags.rs           (grow — from_export parsing into BrowseRow)
└── webdev/mod.rs         (untouched — scriptExec route shipped)
crates/ignition-cli/src/cli.rs   (Backup/Eam/Script/Lint args + project diff/sync subcommands)
crates/ignition-tui/src/ui/routes.rs  (EVERY new verb lands its row in the SAME plan)
```

### Pattern: two-client resolution (07-01)

`project diff/sync` resolves two profiles through the SAME `resolve_selection` machinery per side; envelope `profile` stays the active profile; `data.profile_a`/`data.profile_b` carry both. Guard-before-resolution applies to sync's --yes on the ACTIVE side only (guard fires before either client resolves — zero network on refusal).

### Pattern: state-gate classification (07-02)

EAM's controller 403 is message-classified into additive `eam_not_controller` (exit 6) exactly like `trial_not_expired` (04-03) — refusal-with-hint over mislabeled auth errors. The 403 body is Jetty HTML (live shape) — classify on message content at the classify() seam.

### Pattern: pure codec module with sacred invariant (07-04)

`scripts_codec` mirrors client/resources.rs: pure functions, unit-testable without a gateway; the acceptance test is `encode(decode(x)) == x` over ported test vectors + byte-equality of unedited members through the full zip round-trip.

### Anti-patterns to avoid

- **Hand-rolling diff at byte level without normalization** — timestamps/signatures differ per gateway; every resource would be CHANGED.
- **Auto-flipping EAM installMode from the CLI** — gateway-role decision; surface the state, don't mutate it.
- **Inventing `ign backup list`** — no native endpoint exists; README honesty over verb theater.
- **A second success oracle for webdev** — scriptExec rides the 200-BODY envelope exactly like every route; HTTP status alone is never success.
- **Markers inside exported JSON for encode addressing** — the manifest-aside approach keeps exported JSON gateway-clean.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---|---|---|---|
| Flint script codec | Ad-hoc string replace | The exact ignition-nvim table + single-pass decoder (port to Rust) | Multi-pass decode corrupts `\\t`; table is cross-validated Lua+Python |
| JSON embedded-script discovery | Regex over raw text | Recursive walk for SCRIPT_KEYS values | Scripts nest arbitrarily deep in view JSON |
| EAM task definitions | Custom REST | The config-resource family pattern (05-04 tag-providers) | Same array-body CRUD, signature discipline, find/names helpers |
| lint child process | shell string | tokio::process arg vector | Injection safety; compose seam precedent |
| Diff/sync member surgery | New zip writer | client/resources.rs helpers (replace_member/remove_member/read_member) | Descriptor-merge + no-op-member rules already live-proven |

## Common Pitfalls

### 1. resource.json volatility poisons diff
**What:** byte-compare flags identical resources as CHANGED (`attributes.lastModification`/`Signature` differ per gateway). **Avoid:** normalize descriptors before hashing (strip the two attribute fields; keep scope/version/files). **Warning sign:** diff of a just-synced pair reports everything CHANGED.

### 2. EAM 403 misread as auth
**What:** `/data/eam/*` 403 says "configured as a controller" — not credential failure. **Avoid:** message-classified `eam_not_controller` slug. **Warning sign:** auth_rejected slugs on a freshly-provisioned token.

### 3. EAM execution needs GNET agents + live trial
**What:** force→204 then history shows `Failed: Gateway network for agent '_controller' … not connected`; expired trial blocks even that ("Trial timer is expired"). **Avoid:** e2e gates reset trial first (tier-0 token POST works on 8.3.3 when expired:true — live-proven this research); surface GNT requirement in task detail output. **Warning sign:** green definitions + red history.

### 4. serde_json key reordering breaks round-trip
**What:** parse→re-serialize reorders/maps JSON vs the gateway's pretty-printed export; encode side splices into a reordered document. **Avoid:** preserve_order + span-level value splicing; byte-equality test on unedited members. **Warning sign:** re-encoded view.json diffs everywhere after a no-op decode/encode cycle.

### 5. `expression` keys are not Python
**What:** Perspective expressions decode-mangled into .py sidecars. **Avoid:** decode only script-looking values (multi-line / contains `\n`); expressions pass through. **Warning sign:** sidecar count >> script count.

### 6. lint exit-code collision
**What:** child exit 1 (findings) vs taxonomy exit 1 (internal). **Avoid:** doctor posture default + `--strict` passthrough; never let findings masquerade as CoreError::Internal. **Warning sign:** agents treating lint findings as CLI crashes.

### 7. Two-sided secret ambiguity in cross-gateway ops
**What:** `IGNITION_TOKEN` env applies to both clients. **Avoid:** README note + per-profile keyring entries for real two-gateway use; data carries profile_a/profile_b. **Warning sign:** auth_rejected on the side you thought had creds.

### 8. Missing TUI row fails CI after the fact
**What:** tui_coverage walks the live clap tree. **Avoid:** every plan's task list includes the routes.rs row for each new verb (script run is the pre-recorded instance). **Warning sign:** CI failure on push, not in-plan.

### 9. put-new descriptor rule in sync
**What:** splicing a member into a folder whose resource.json doesn't list it silently no-ops (05-07). **Avoid:** reuse replace_member (descriptor merge included); post-sync re-export read-back for e2e. **Warning sign:** sync succeeds, target unchanged.

### 10. Legacy vs individual git-module format confusion
**What:** `<provider>.json` (legacy whole tree) vs per-tag files. **Avoid:** auto-detect by shape (Provider-typed JSON at top vs directory tree); dot-entries skipped. **Warning sign:** from-export shows one giant folder.

## Recommended Plan Skeleton Refinements

- **07-01 (diff/sync):** biggest plan — two-client resolution shape + pure normalized-compare helpers + `project diff` verb + `project sync` verb (--yes, --delete opt-in) + TUI rows + wiremock request-sequence proofs (two base URLs in one server) + member-level round-trip tests. Consider splitting diff (read-only) from sync if task count balloons.
- **07-02 (backup + EAM):** backup standalone is small (actions + cli + guard + goldens; type param). EAM is its own family: client/eam.rs + actions/eam.rs + `eam task list/history` reads + guarded definition create + force (--yes) + `eam_not_controller` classification + e2e gated on trial-reset. Keep backup and EAM as separate tasks; they share nothing but the phase.
- **07-03 (script run):** smallest — precondition + secret resolution + exec POST + result shape + redaction tests + TUI input/result modal flow. Route source untouched.
- **07-04 (interop trio):** one task per deliverable: (a) scripts_codec pure module + export/import flags + round-trip byte-equality tests; (b) lint delegation + absence hint + strict mode; (c) from-export parsing (both git-module formats + CLI export JSON) reusing BrowseRow/render.

## Open Questions

1. **lint exit semantics** — default doctor-posture (exit 0, findings in data, child_exit_code field) + `--strict` passthrough, or literal passthrough always? *Ladder:* README-documented default-first (my rec) → user call at planning.
2. **Human text diffs in `project diff`** — member-status only (zero deps) vs `--text` unified diffs (adds `similar`). *Ladder:* status-only MVP; `similar` as a scoped follow-up if UAT demands.
3. **Tag-provider scope surface** — separate `ign tags provider diff` verb vs `--include tag-providers` flag on project diff vs README-documented pipe only. *Ladder:* metadata + README pipe first; diff-only flag second; provider sync never (machinery mismatch).
4. **`script run` input forms** — `--code`, `--file`, `--file -` (stdin): all three or minimal `--code`+`--file`? *Ladder:* all three (stdin is the agent path; TUI refuses stdin per the crossterm rule).
5. **EAM create surface breadth** — which task types unguarded vs --yes vs refused (my rec: backup default; restart/send* --yes; restore/install/upgrade --yes with consequence naming or refuse). *Ladder:* user call at planning; history/reads uncontentious.
6. **Sidecar addressing scheme for decode** — JSON-pointer manifest vs `<member>.<n>.py` flat counter. *Ladder:* counter-named sidecars + JSON-pointer manifest entries (deterministic, rename-friendly); user call if they want path-mirroring trees.
7. **EAM controller-mode helper** — should the CLI ever offer `ign eam enable-controller` (config-resource PUT on module-settings)? *Ladder:* README-documented manual flip (my rec — role decisions shouldn't be one flag away) → explicit verb only if UAT demands.

## Sources

### Primary (HIGH confidence)
- **Live gateway probing** (this research): ignition-devops rig, 8.3.3, ports 9088/9043 — headless token per STATE.md recipe (token form `NAME:key`, header `X-Ignition-API-Token`); `/openapi.json` captured (575 paths); EAM controller flip + task create + force + history round executed live; trial tier-0 token reset re-verified. Trimmed extract: `07-openapi-extract.json` (this dir).
- **Shipped code:** `crates/ignition-core/src/{webdev/mod.rs, client/{backup,resources,mod}.rs, actions/{webdev,resources,tags,projects}.rs}`, `crates/ignition-cli/src/{cli,main}.rs`; route source `webdev/routes/.../scriptExec/doPost.py`.
- **Sibling repos (local checkouts):** `ignition-nvim/packages/ignition-lsp/ignition_lsp/{encoding.py, json_scanner.py}` + `lua/ignition/{encoding.lua, json_parser.lua}` (Flint codec + SCRIPT_KEYS); `ignition-lint/src/ignition_lint/cli.py` + `pyproject.toml` (CLI surface, exit codes, JSON report); `ignition-git-module/git-gateway/.../managers/GitTagManager.java` + `git-common/.../TagExportConfig.java` (on-disk tag formats, `_types_`, fs-name encoding, `.tag-config.json`).

### Secondary (MEDIUM)
- `.planning/phases/03-project-operations/openapi-8.3.6-phase3-extract.json` (8.3.6 cross-version EAM path agreement — config-resource surface identical on 8.3.6).
- `.planning/phases/05-webdev-backend-tag-operations/05-RESEARCH.md` (prior EAM probe: project-resources 403; export/import-as-native conclusions).

### Tertiary (LOW — flagged)
- EAM licensing posture on non-trial (Maker/Standard) editions — untested; trial proven only. Treat "licensed controllers work" as assumption; the state-gate slug covers the unlicensed/refused shape honestly either way.

## Metadata

**Confidence breakdown:**
- EAM endpoints/wire/behavior: HIGH (live-proven + openapi + extract committed)
- gwbk delta + backup API: HIGH (shipped code + openapi)
- scriptExec contract: HIGH (route source is the contract)
- Flint codec + decode/encode design: HIGH (dual-ported source with stated invariant); byte-stable JSON re-serialization: MEDIUM (design recommendation, needs the round-trip test to confirm the splice approach)
- ignition-lint delegation: HIGH (source); exit-code mapping: MEDIUM (design tension, ladder provided)
- git-module formats: HIGH (Java source); legacy-format live file: not exercised (code-read only)
- Cross-gateway diff normalization: MEDIUM-HIGH (volatility live-evidenced; normalization approach is recommendation)

**Research date:** 2026-08-28
**Valid until:** 2026-09-28 (gateway-version-stable domain; re-verify EAM enums if 8.4 lands)
