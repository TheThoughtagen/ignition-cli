# Phase 3: Project Operations - Research

**Researched:** 2026-08-22
**Domain:** Ignition 8.3 project REST API (CRUD, export/import ZIP, resource-level surgical edits) over the Phase-1/2 client seam
**Confidence:** HIGH for project CRUD + export/import endpoints (official IA collection + author's working client agree); **MEDIUM for resource-level endpoints** (single source — see Open Questions); HIGH for repo-internal conventions (read from code)

## Summary

Phase 3 replaces the gateway webpage's project management. Every PROJ requirement is served by the native `/data/api/v1/projects/*` family, fully enumerated in the **official `inductiveautomation/83-api` collection** (local clone at `~/whiskeyhouse/83-api`, verified current with upstream `5c599a8`): list/names/find/parents (reads), create/copy/rename/modify/delete (mutations), and export/import (ZIP transfer). The ignition-mcp Python client (the author's production code, in daily use) agrees on every one of those paths and supplies working payload shapes: create is a POST with a JSON body whose optional fields the server tolerates omitting; delete takes a `confirm=<boolean>` **query param**; rename is native (`POST /projects/rename/{name}` with `{"name": "<new>"}`) — not copy+delete; "move" in the roadmap maps to rename (`mv`), while inheritance reparenting is a separate `PUT /projects/{name}` modify of the `parent` field; import is `POST /projects/import/{name}?overwrite=<bool>` with the ZIP as the raw request body (`Content-Type: application/zip`); export is `GET /projects/export/{name}` returning the ZIP with a `Content-Disposition` filename. The official collection also reveals a **scan-lock family** (`GET/POST /data/api/v1/scan-lock/projects`) that governs concurrent project-system mutation — the answer to the "concurrent import locks" danger question — plus the already-implemented `scan_projects()` capability for post-import rescan.

The one genuinely under-verified area is the **resource-level family** (`/data/api/v1/projects/{project}/resources[/...]` GET/PUT/DELETE): it exists only in ignition-mcp (client + docs + 4 MCP tools) and appears in **neither** the official 83-api collection **nor** any public code search result (grep.app finds only WhiskeyHouse/ignition-mcp). Phase 2 proved this exact client invents plausible-looking paths (`/connections/database`, `/system/metrics` were fake), so this family must be treated as MEDIUM and verified by live capture (wiremock-first development + an `#[ignore]`-gated live verification hook, the 02-03 "connections RAW passthrough until live capture" precedent). The **resource tree layout itself is HIGH confidence** — verified directly from `whk-distillery01-ignition-global`, a real git-module-managed export: `project.json` at root (`{title, description, enabled, inheritable}` — no name, no parent), resource folders `{module-id}/{resource-type}/…/{resource.json + payload files}` where payloads are JSON (`view.json`, `code.py`, `style.json`) **or binary `data.bin`**, and tags/UDTs live entirely outside the export. That export-layout truth directly shapes the scope metadata (Pitfall 5 from project research) and warns that some resources cannot round-trip as JSON.

Repo internals Phase 3 inherits: the destructive dispatch shape (guard → resolve → action) fires `require_confirmation` BEFORE profile resolution (02-03 LOCKED); the per-request timeout override exists (`RequestBuilder::timeout`, logs_download 120s precedent — no second client); `get_bytes` returns a buffered `Vec<u8>` (logs precedent) but the phase's success criteria demand **streamed export to disk**, which requires two workspace dependency additions: **reqwest `stream` feature** (verified: `bytes_stream`/`Body::wrap_stream` are feature-gated) and **tokio `fs` feature**. Import can stay buffered (ZIPs are typically MB-scale) with a long per-request timeout, which sidesteps the chunked-encoding question entirely (send a known `Content-Length` from a `Vec<u8>`).

**Primary recommendation:** Build three plans matching the sketch — (1) project list/new/copy/rename/set/delete with the `--yes` guard verbatim, (2) streaming export + buffered import with `--collision-policy abort|overwrite` and scope metadata, (3) resource ls/get/put/delete + e2e harness skeleton — treating the resource family as wiremock-first with an explicit live-capture verification task, and adding a cheap `#[ignore]` openapi-extract capture (projects+resources paths) that settles the resource-endpoint question the moment a rig token exists.

<user_constraints>
## User Constraints (from CONTEXT.md)

No CONTEXT.md exists for this phase (no `/gsd-discuss-phase` was run). No locked user decisions beyond the STATE.md contracts below. Everything here is research-backed recommendation; the planner has full discretion within the LOCKED prior-phase contracts:

- Agentic output contract FROZEN — envelope exactly `{ok,profile,data}`/`{ok,profile,error}`, exit taxonomy 0–7 with stable slugs (config=3, network=4, auth=5, target-state=6), errors-on-stderr in all modes; additive slugs only.
- Exit-code table lives in exactly two places (`CoreError::exit_code()` + README).
- `GatewayApi` locked on `async_trait`, grown per-capability with one coarse method per capability; trait methods + impl bodies live in the single impl block in `client/mod.rs` (E0119 forbids split impls).
- `classify()` is the only status→error mapping site; pipeline helpers (`get_json`/`post_empty`/`delete_with_query`/`get_bytes`) are the only body-consumption sites; every new capability needs exact-path wiremock pins + recorded-request proofs for path subtleties.
- Destructive-command dispatch shape (guard → resolve_gateway_api → action); `require_confirmation` fires BEFORE profile/secret/client resolution (refusal = exit 2, envelope profile null).
- JSON data always carries ALL family keys for filtered lists (filtered-out = `[]`, endpoints never called).
- Two-column naming: client models stay wire-faithful (gateway-native camelCase), actions re-expose unit-explicit keys.
- Inspection commands REQUIRE a credential (exit 3 without); wait commands degrade header-less.
- actions layer = serde models only, no printing (TUI rides it in Phase 6).
- Every new GatewayApi method must be stubbed into all existing test doubles (~9 inline rigs + `common/mod.rs`).
- wiremock gotchas: `set_body_string` forces text/plain (use `set_body_raw`); scoped `MockGuard` drop unmounts fixtures.
- Live suite is skip-by-default green no-op (`-- --ignored`; needs `IGNITION_LIVE_URL`/`IGNITION_LIVE_TOKEN`; research rig `ign-research` is up on port 18088 but **no token currently exists** — see Open Questions).
</user_constraints>

## Verified Endpoint Catalog

### Project CRUD (HIGH — official 83-api Postman + Bruno, ignition-mcp agreement)

| Capability | Endpoint | Method | Request | Notes |
|---|---|---|---|---|
| List projects | `/data/api/v1/projects/list` | GET | standard list params (`limit/offset/sortBy/search/filter`; `limit=-1` = all) | Official description: "List all **runnable** projects" — `{items, metadata}` envelope (02-02's `ListEnvelope<T>`); item shape presumed = find shape + `name` (MEDIUM — needs live capture) |
| List names only | `/data/api/v1/projects/names` | GET | same list params | Cheaper existence check |
| Project details | `/data/api/v1/projects/find/{name}` | GET | — | Full project record |
| Valid parents | `/data/api/v1/projects/parents` | GET | list params | All projects eligible as parents |
| Valid parents for project | `/data/api/v1/projects/parents/{name}` | GET | list params | Excludes self + descendants (inheritance-cycle guard — inferred from purpose; MEDIUM) |
| Create | `/data/api/v1/projects` | POST | JSON body | See payload table below; server tolerates partial bodies (mcp sends only `{name, enabled}` + optionals) |
| Copy | `/data/api/v1/projects/copy` | POST | `{"fromName": "...", "toName": "..."}` | Exact copy of all resources; `toName` must not exist |
| Rename | `/data/api/v1/projects/rename/{name}` | POST | `{"name": "<newName>"}` | **Native rename exists** — not copy+delete. Official collection body key is `name` |
| Modify / reparent | `/data/api/v1/projects/{name}` | PUT | JSON body **without `name`** (description, title, enabled, parent, inheritable, userSource, tagProvider, defaultDb) | This is "reparenting"/`set`; changing `parent` = inheritance move |
| Delete | `/data/api/v1/projects/{name}` | DELETE | **query param `confirm=<boolean>`** | The API's own confirmation guard — our `--yes` maps to `confirm=true` (both layers, defense in depth) |

**Create payload fields (official body schema):** `name`, `description`, `title`, `enabled`, `parent`, `inheritable`, `defaultDb`, `tagProvider`, `userSource`. Recommended client behavior: always send `name` + `enabled`; send optionals only when provided (omit — never send empty strings for `parent`/`defaultDb`/`tagProvider`/`userSource`, which would reference nonexistent resources). `inheritable` = "this project may serve as a parent" (verified: distillery `project.json` carries it).

### Export / Import (HIGH for paths/params; MEDIUM for response bodies)

| Capability | Endpoint | Method | Request | Notes |
|---|---|---|---|---|
| Export | `/data/api/v1/projects/export/{name}` | GET | — | "Exports the given project as a zip archive" (official). Response body = ZIP bytes; `Content-Disposition` carries a filename (mcp parses it; logs download precedent). Content-type unverified — sniff, don't assume |
| Import | `/data/api/v1/projects/import/{name}` | POST | **query param `overwrite=<bool>`**; body = raw ZIP bytes, `Content-Type: application/zip` | Official: "Set `true` to overwrite an existing project of the same name." Response body unverified (mcp falls back to `{"status":"success"}` on non-JSON; restart returns literal `true` — same family style). **Synchronous, no job IDs** (project-research PITFALLS 1.3 — verified claim from milestone research) |

**Collision policy reality:** the REST surface has exactly `overwrite` (boolean). **No merge mode exists via REST** — Overwrite/Merge/Abort is the Designer import popup vocabulary (git-module convention); REST exposes abort (omit/false) and overwrite only. Recommendation: CLI flag `--collision-policy abort|overwrite`, default `abort` (matches igw-cli tags-import default and git-module convention), rejecting `merge` at clap level with a hint ("Designer-only import mode — not available via REST").

**CSRF:** none needed — token-authenticated POST/PUT/DELETE require no CSRF headers (Phase 2, verified live: set-logger-level via token, no `X-CSRF-Token` → 200; CSRF is cookie/session-only). Import/delete/copy/rename all ride the token mechanism. HIGH by transfer.

### Scan / lock family (HIGH — official collection; already partially in the codebase)

| Endpoint | Method | Purpose |
|---|---|---|
| `/data/api/v1/scan/projects` | GET | Status of the project filesystem scan |
| `/data/api/v1/scan/projects` | POST | "Prompts the system to scan the filesystem for project changes. Will also release the scan lock, if currently being held." **Already implemented** as `GatewayApi::scan_projects()` (client/restart.rs, used by doctor's write check) |
| `/data/api/v1/scan-lock/projects` | GET | Info about the currently held scan-lock, if any |
| `/data/api/v1/scan-lock/projects` | POST | Body `{acquireTimeout, holdTimeout}` — "Prevents changes from being applied to the project system for a limited time… While locked, it is safe to make external changes to the filesystem. The next call to `POST /data/api/v1/scan/projects` will release the lock." |

This answers the concurrency danger question: REST import/copy/delete serialize through the project system internally; the scan-lock exists for **filesystem-level** external mutation (docker volume edits, git-module sync). The CLI does not need scan-lock in Phase 3 (REST mutations handle locking) — document it as the reason concurrent REST imports are safe, and note it as the Phase 4/7 rig/sync primitive. A post-import `scan_projects()` call is unnecessary for REST imports (they mutate through the API) but harmless; do NOT add it automatically.

### Resource-level family (MEDIUM — single source: ignition-mcp; absent from official collection)

| Capability | Endpoint | Method | Request | Notes |
|---|---|---|---|---|
| List resources | `/data/api/v1/projects/{project}/resources` | GET | optional `path=<prefix>` query filter (mcp: `path_prefix` → `params={"path": ...}`) | Presumed `{items, metadata}` envelope (unverified) |
| Get resource | `/data/api/v1/projects/{project}/resources/{resourcePath}` | GET | resourcePath keeps `/` (mcp: `quote(path, safe="/")`), project name fully encoded | Returns resource content (JSON doc or text) |
| Put resource | same | PUT | JSON body = resource content | "If the resource doesn't exist it will be created" (mcp docs) |
| Delete resource | same | DELETE | — | Irreversible |

**Verification status:** these paths appear ONLY in WhiskeyHouse/ignition-mcp (client, tools, docs). grep.app public code search finds zero other users; the official 83-api collection (Postman + Bruno, current with upstream) does not contain them; igw-cli has no project-resource commands. Phase 2 found two invented paths in this same client, so treat as MEDIUM: **wiremock-first development, plus a live-capture gate** (see Open Questions #1). Do not copy mcp's doc-string path examples blindly either — its docs say `com.inductiveautomation.ignition/script-python`, but the real core-module folder in a live 8.3 export is **`ignition/script-python`** (verified from `whk-distillery01-ignition-global`). The wire `resourcePath` format for get/put almost certainly matches the export tree (`{module}/{type}/…/name`), but the exact accepted forms (with/without filename, folder vs leaf) must be live-captured before the put/get loop is trusted.

### What a project export actually contains (HIGH — verified from real export tree)

`whk-distillery01-ignition-global` is a live git-module-managed 8.3 project export; layout (which equals the export ZIP's internal layout in 8.x — the Designer export IS the resource tree):

```
project.json                                  # {title, description, enabled, inheritable} — NO name, NO parent
com.inductiveautomation.perspective/views/{folder path}/{ViewName}/
    resource.json, view.json, thumbnail.png   # JSON payloads + binary thumbnail
com.inductiveautomation.perspective/style-classes/…/resource.json + style.json
com.inductiveautomation.perspective/session-props/…            # props.json
com.inductiveautomation.perspective/{session-permissions,general-properties}/data.bin  # BINARY payloads
com.inductiveautomation.perspective/{page-config,stylesheet,…}
ignition/script-python/{package path}/resource.json + code.py  # NOTE: core module folder is `ignition`
ignition/named-query/…, ignition/global-props/data.bin
com.inductiveautomation.{vision,reporting,alarm-notification,webdev}/…  # other modules' project resources
```

`resource.json` schema (per resource):
```json
{
  "scope": "A" | "G" | ...,
  "version": 1,
  "restricted": false,
  "overridable": true,
  "files": ["code.py"],
  "attributes": {
    "lastModification": {"actor": "user@host", "timestamp": "2025-05-16T20:39:06Z"},
    "lastModificationSignature": "<sha256-hex>"
  }
}
```

**Consequences:**
1. **Scope split confirmed** (PITFALLS 5): tags/tag providers/UDTs are NOT in a project export — they are gateway configuration (the git-module's separate `tags/`+`tags.json`+`udts.json` convention exists precisely because of this). `includes`/`excludes` metadata is static documentation of this truth.
2. **Some resources are binary** (`data.bin`) — a resource `get` can return non-JSON; a resource `put` cannot round-trip them safely. The CLI must detect and refuse (or raw-pass with a warning) binary resources rather than corrupt them.
3. `lastModificationSignature` is a versioning token on every resource — if the REST resource endpoints honor/require it on PUT (analogous to config-resource `signature` params), blind put = lost-update risk. Unknown until live capture; note as an open question feeding the surgical-edit design.

## Recommended Command Surface

Two-column naming LOCKED: wire-faithful client models, unit-explicit action keys. All commands require a credential (inspection-command rule: exit 3 without).

```
ign project list [--json]                       # items: name, title, description, enabled, parent, inheritable (+flatten passthrough)
ign project new NAME [--title T] [--description D] [--parent P] [--inheritable] [--disabled]
ign project copy SRC DST                        # mutation — --yes? (non-destructive: creates new; NO guard — matches copy semantics elsewhere; planner may guard)
ign project rename OLD NEW                      # mutation, non-destructive → no --yes
ign project set NAME [--title/--description/--parent/--enabled/--inheritable/…]  # PUT modify (reparent = "move" under inheritance)
ign project delete NAME                         # DESTRUCTIVE — --yes/IGNITION_YES=1 guard verbatim (02-03 dispatch shape); sends confirm=true
ign project export NAME [-o FILE]               # streams ZIP to disk; JSON data: {project, file, bytes, duration_ms?, scope:{includes,excludes}}
ign project import NAME --file PATH | --file -  # '-' = stdin (read fully, sized guard); --collision-policy abort|overwrite (default abort)
ign resource list PROJECT [--prefix P]          # items: resource paths (+ whatever the envelope carries)
ign resource get PROJECT PATH
ign resource put PROJECT PATH --file PATH|-     # stdin/file content; JSON if parseable else raw text; refuse .bin-class payloads
ign resource delete PROJECT PATH                # DESTRUCTIVE — --yes guard
```

Design notes:
- **`export` writes a file, never stdout-raw** (PROJ-03 says "to file"; a binary ZIP on stdout would fight the envelope contract). Default filename: `Content-Disposition` filename, else `{name}.zip` (logs-download precedent).
- **`import` pre-check**: `GET /projects/find/{name}` first — exists + policy=abort → exit 6 with an additive slug (e.g. `project_exists`) + hint naming `--collision-policy overwrite`, BEFORE uploading. Server 409/400 remains the backstop (classify may need one additive slug for 409 — planner decision).
- **Stdin import**: read to `Vec<u8>` fully, send with known `Content-Length` + `Content-Type: application/zip` (avoids the unverified chunked-encoding question entirely). Refuse >~512 MB with a config-class error (sanity guard).
- **Human output**: table of projects (name/title/enabled/parent/inheritable); export/import print the file path + bytes.

## Architecture Patterns (repo-verified)

### New client capabilities (one coarse method each, single impl block)

```rust
// client/projects.rs (models + path constants + timeout consts)
pub const PROJECTS_LIST_PATH: &str = "/data/api/v1/projects/list";
pub const PROJECT_EXPORT_TIMEOUT: Duration = Duration::from_secs(120);  // logs-download precedent
pub const PROJECT_IMPORT_TIMEOUT: Duration = Duration::from_secs(300);  // imports are heavier

// trait additions (client/mod.rs impl block):
async fn projects(&self, query: &ListQuery) -> Result<ListEnvelope<ProjectRecord>, CoreError>;
async fn project_find(&self, name: &str) -> Result<ProjectRecord, CoreError>;
async fn project_create(&self, body: &ProjectCreate) -> Result<ProjectRecord, CoreError>; // or () — response unverified
async fn project_copy(&self, from: &str, to: &str) -> Result<(), CoreError>;              // post_json helper
async fn project_rename(&self, name: &str, new_name: &str) -> Result<(), CoreError>;
async fn project_modify(&self, name: &str, body: &ProjectModify) -> Result<(), CoreError>;
async fn project_delete(&self, name: &str) -> Result<(), CoreError>;       // delete_with_query confirm=true
async fn project_export_to_file(&self, name: &str, out: &Path) -> Result<ExportMeta, CoreError>; // STREAMS
async fn project_import(&self, name: &str, zip: Vec<u8>, overwrite: bool) -> Result<ImportOutcome, CoreError>;
async fn project_resources(&self, project: &str, prefix: Option<&str>) -> Result<ListEnvelope<ResourceEntry>, CoreError>;
async fn project_resource_get(&self, project: &str, path: &str) -> Result<ResourceContent, CoreError>;
async fn project_resource_put(&self, project: &str, path: &str, body: ResourceBody) -> Result<(), CoreError>;
async fn project_resource_delete(&self, project: &str, path: &str) -> Result<(), CoreError>;
```

### New pipeline helpers (the only body-consumption sites)

- `post_json(path, &body, timeout)` — JSON mutation POST (create/copy/rename). `post_empty` exists; this adds a body.
- `put_json(path, &body, timeout)` — modify + resource put.
- `download_to_file(path, out, timeout)` — `classify()` first (status before body), then `bytes_stream()` → `tokio::fs::File` via `AsyncWriteExt::write_all` on each chunk; capture `Content-Disposition`/`Content-Type` into `ExportMeta`. **Requires workspace dep additions: reqwest feature `stream`, tokio feature `fs`** (verified: reqwest response streaming and `Body::wrap_stream` are `stream`-feature-gated — Context7 `/seanmonstar/reqwest`).

### Streaming + progress (the LOCKED shape)

Export rides the sanctioned dispatch-owned-sink pattern (`logs -f` NDJSON is the precedent): dispatch streams during execution; human mode prints a one-line progress to **stderr** (`exporting {name} … {bytes} written`); `render_ok` intercepts the `ProjectExport` ActionOutput variant before mode dispatch only if human output was already emitted — simplest correct shape: export completes, then `render_ok` prints the envelope (path/bytes/scope). The file IS the artifact; no stdout exception needed. (Phase's "streamed ZIPs to disk" = the HTTP→disk streaming above, not NDJSON-on-stdout.)

### The doubles chore (enumerate it in every plan)

`impl GatewayApi` currently exists for `ReqwestGatewayApi` + inline rigs: `ConnectionsRig`, `DoctorRig`, `BrokenOverview`, `HealthyRig`, `AuthRig`, `TailRig`, `SessionsRig`, `FakeApi` (+ `common/mod.rs` mock helpers). Every new trait method must be stubbed in all of them per plan — cite this list in each plan's tasks so nothing compiles half-way.

### Export/import scope metadata (PROJ-04 success criterion)

Static, documented-once, carried in both `export` and `import` JSON data:

```json
"scope": {
  "includes": ["views", "scripts", "named-queries", "vision-windows", "perspective-themes-styles", "reporting", "alarm-notification-profiles", "webdev-routes", "translations", "sfc-charts"],
  "excludes": ["tag-providers", "tags", "udts", "gateway-config", "database-connections", "users-roles", "alarm-journal", "certificates"]
}
```

(HIGH confidence — verified from the real export tree + git-module's separate tags convention. Keep the arrays as data, not prose, so agents can key off them.)

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---|---|---|---|
| ZIP integrity check on import | Full ZIP parser/validation | Minimal local-file-header magic check (`PK\x03\x04`) or none; rely on gateway's error | The gateway validates imports; a magic-byte guard catches the common "wrong file" mistake cheaply |
| Export file naming | Custom conventions | `Content-Disposition` filename → `{name}.zip` fallback | Matches logs-download precedent; gateway names exports well |
| Collision handling | Retry/merge logic in CLI | Pre-check find + `overwrite` query param; abort otherwise | REST has no merge; server is the authority |
| Resource-tree knowledge | Hardcoded resource-type map | `resource list` passthrough + `--prefix` filter | The gateway owns the taxonomy; `com.inductiveautomation.perspective/views`-style prefixes are user-facing filters, not schema |
| Import progress bars | Percentage estimation | stderr stage markers only | Byte-accurate progress is unknowable for a buffered upload; stage markers match the agentic discipline |

## Common Pitfalls

### 1. Resource endpoints are single-source (THE phase risk)
**What goes wrong:** building three plans of model work atop `/projects/{p}/resources/*` paths that may differ on the wire (Phase 2 caught ignition-mcp inventing `/connections/database`).
**How to avoid:** wiremock-first with the mcp-derived shapes marked MEDIUM; add a live-capture task early (openapi extract + one real GET/PUT round-trip on a scratch project); keep resource models passthrough-heavy (`flatten` extra map) so wire-truth corrections are cheap. Follow the 02-03 connections precedent: RAW passthrough + `live_*` hook + UAT open question until captured.

### 2. Buffered export would violate the phase contract (and OOM on big projects)
**What goes wrong:** copying `get_bytes`'s `Vec<u8>` pattern for export buffers multi-hundred-MB ZIPs in memory.
**How to avoid:** new `download_to_file` pipeline helper; add reqwest `stream` + tokio `fs` features in the FIRST plan that needs them (03-02). Warning sign: `Vec<u8>` appearing in `project_export*` signatures.

### 3. Default-timeout import death (the classic, PITFALLS 1.3)
**What goes wrong:** 30s client default kills a long import mid-flight with unknown gateway state.
**How to avoid:** per-request `RequestBuilder::timeout` overrides — export 120s, import 300s constants in `client/projects.rs` (logs_download 120s precedent; NO second client, NO global timeout change). On timeout: network_error exit 4 with a "verify with `ign project list`" hint (idempotent-retry guidance from milestone research).

### 4. Overwrite semantics surprise
**What goes wrong:** `overwrite=true` REPLACES the whole project — resources absent from the ZIP are gone (replace, not merge). Users expecting merge lose resources.
**How to avoid:** default `abort`; hint text on the overwrite choice says "replaces the entire project"; docs state merge is Designer-only. (Exact wipe semantics unverified live — MEDIUM; conservative wording + e2e test will pin it.)

### 5. Empty-string reference fields on create/modify
**What goes wrong:** sending `"parent": ""`/`"defaultDb": ""` (serde defaults or struct zeros) can create invalid references or clear settings unintentionally.
**How to avoid:** `Option<String>` with `skip_serializing_if = "Option::is_none"` on every optional field; only `name` (+ `enabled`) always sent. Pin the exact serialized body in a wiremock recorded-request proof.

### 6. Path encoding of names with spaces/case
**What goes wrong:** project and resource paths contain spaces and mixed case in the real world (`views/Exchange/CMMS/Page/Asset Management/`); naive path interpolation breaks.
**How to avoid:** percent-encode each path segment (project name: encode everything; resource path: encode per-segment, keep `/`). mcp precedent: `quote(name, safe='')` / `quote(path, safe='/')`. Wiremock-proof a spaced name. Project-name uniqueness/validation is server-side — surface 400/409 messages via classify.

### 7. Binary resources through the JSON surgical loop
**What goes wrong:** `resource get` on a `data.bin` resource returns bytes; naive `serde_json` parse fails or corrupts on put.
**How to avoid:** resource get returns raw body + content-type sniff (the classify/HTML-sniffer discipline inverted); `put` refuses binary-looking payloads with a target-state hint ("resource is binary — use export/import"). Detected via `resource.json`-sibling knowledge or content sniffing; keep it pragmatic.

### 8. Delete needs BOTH guards
**What goes wrong:** relying only on our `--yes` (client-side) or only on `confirm=true` (server-side).
**How to avoid:** both, always: refusal happens pre-resolution (exit 2, profile null — LOCKED shape); the wire request always carries `confirm=true` (wiremock-proven query param, exact-path pin).

### 9. wiremock gotchas inherited (repeat offenders)
`set_body_string` forces `text/plain` — use `set_body_raw` for ZIP/binary fixture bodies; scoped `MockGuard` drops unmount fixtures (keep guards alive for the duration of assertions); pin subtle paths on the REQUEST side (trailing slashes, query params like `confirm=true`/`overwrite=true`/`path=…`).

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|---|---|---|---|
| Designer-gatekeeper project ops | Native REST project CRUD + export/import | 8.3 | This phase replaces the webpage entirely via REST |
| `com.inductiveautomation.ignition/…` core-module paths (mcp docs) | `ignition/…` core-module folder in real 8.3 exports | observed 8.3.6 export | Resource prefixes user pass must match reality; don't hardcode either — passthrough |
| Config-in-IDB | Config-as-resources (`/data/api/v1/resources/*`) | 8.3 | Project *resources* ≠ config *resources* — different families, don't conflate |
| (planned) async job machinery for imports | Synchronous request, per-op timeouts | verified at milestone research | No polling/job IDs to model; timeouts + verify-after are the pattern |

**Deprecated/outdated to avoid:** ignition-mcp's `/connections/database`, `/system/metrics` (proven fake in Phase 2 — do not copy anything from that client without cross-check); mcp's `com.inductiveautomation.ignition` prefix examples (superseded by the observed `ignition` folder).

## Code Examples

### Streaming export to disk (the 03-02 core loop)

```rust
// Source: reqwest docs (Context7 /seanmonstar/reqwest — stream feature) + repo pipeline rules
use tokio::io::AsyncWriteExt;

// inside the new download_to_file pipeline helper (after classify() on the response):
let mut stream = response.bytes_stream();          // requires reqwest feature "stream"
let mut file = tokio::fs::File::create(&out).await?; // requires tokio feature "fs"
let mut written: u64 = 0;
use futures_util_extended::StreamExt as _; // futures-util via reqwest's stream feature (verify exact import at impl time)
while let Some(chunk) = stream.next().await {
    let chunk = chunk.map_err(/* → CoreError::Network */)?;
    file.write_all(&chunk).await?;
    written += chunk.len() as u64;
}
file.flush().await?;
```

(Note: `futures_util` arrives transitively with reqwest's `stream` feature; if the import path needs a direct dev-dep, add it workspace-level — planner verifies during 03-02.)

### Import request (buffered, known length)

```rust
// Source: ignition-mcp import_project (working code) + reqwest raw-body docs
let url = self.url(&format!("/data/api/v1/projects/import/{encoded_name}"));
let request = self.client.post(url)
    .timeout(PROJECT_IMPORT_TIMEOUT)                       // per-request override pattern
    .query(&[("overwrite", if overwrite { "true" } else { "false" })])
    .header(reqwest::header::CONTENT_TYPE, "application/zip")
    .body(zip_bytes);                                       // Vec<u8> → known Content-Length
let response = self.send_and_classify(request).await?;      // classify() first — LOCKED
```

### Destructive dispatch arm (verbatim LOCKED shape)

```rust
// Source: crates/ignition-cli/src/main.rs Sessions::Terminate / Restart arms (02-03, 02-05)
Commands::Project(ProjectArgs { command }) => match command {
    ProjectCommand::Delete { name } => {
        if let Err(err) = require_confirmation(cli.yes, "project delete") {
            return render_failure_exit(&mut config, err, None); // exit 2, profile null, zero I/O
        }
        let (name_, api) = resolve_gateway_api(&mut config, cli.profile.as_deref());
        // ...
    }
}
```

## e2e Harness Skeleton (03-03 deliverable)

Minimal, extensible, dogfoods the binary — the Phase 2 live-suite pattern extended to mutations:

- `crates/ignition-cli/tests/e2e_projects.rs`, every test `#[ignore]`-gated, skip-by-default green no-op.
- Env contract inherited + extended: `IGNITION_LIVE_URL`, `IGNITION_LIVE_TOKEN`, **`IGNITION_LIVE_MUTATIONS=1`** (the 02-04 precedent for mutation gating) — absent any → skip quietly.
- Uses `assert_cmd` to spawn the built `ign` binary (true e2e, not trait-level).
- The loop: `project new ign-e2e-$TS` → `export -o tmp.zip` (assert file + scope metadata) → `resource put` a scratch script resource → `resource get` verify → `import --file tmp.zip --collision-policy abort` into second name (assert `project_exists`-style failure) → `import --collision-policy overwrite` (assert success + resource survived) → `rename` → `copy` → `project delete` cleanup (with `--yes`). Timestamped names so failures leave forensic state, cleanup best-effort.
- Later phases (5's webdev deploy, 4's rig) extend this file — it's the skeleton the roadmap calls for.
- Bonus task (cheap, high value): an `#[ignore]` test or documented curl that fetches `/openapi.json` and saves a trimmed `projects+resources` extract next to the Phase-2 extract — the moment a token exists, the resource-endpoint question closes with an authoritative artifact.

## Plan-Shape Recommendation (validates the 3-plan sketch)

The sketch is correct with two adjustments:

- **03-01 — project CRUD + list/inheritance info.** `project list/new/copy/rename/set/delete`. Includes `find` + `parents` as data plumbing for inheritance display (PROJ-01's "parent info" comes from list items; `parents/:name` powers a `--valid-parents` affordance or `set --parent` validation — planner's call, keep minimal). Delete carries the `--yes` guard + `confirm=true` pin. All CRUD endpoints are HIGH confidence → fast plan.
- **03-02 — export/import.** Streaming download helper + dep additions (`reqwest stream`, `tokio fs`), buffered stdin/file import, `--collision-policy`, pre-check find, timeout constants, **scope metadata lives HERE** (it's export/import output — not 03-03 as sketched).
- **03-03 — resources + e2e skeleton.** The MEDIUM-confidence family isolated last: wiremock-first resource ls/get/put/delete with passthrough-heavy models, binary-resource refusal, the live-capture hooks (openapi extract + resource round-trip check), and the e2e harness skeleton above.

Sequencing: strictly 01 → 02 → 03 (03-02's deps could parallel 03-01 in theory, but wave discipline + the doubles chore argue for sequential; planner decides waves per house convention).

## Open Questions

1. **Resource endpoint wire truth** (MEDIUM confidence today)
   - What we know: ignition-mcp's paths/payloads work in the author's environment; the official collection lacks the family; grep.app shows no third-party usage.
   - What's unclear: exact response envelope for list; whether PUT requires a signature/lastModification token (lost-update semantics); accepted resourcePath forms (leaf file vs folder); binary-resource behavior.
   - Recommendation: wiremock-first + live-capture gate + openapi-extract task; keep models passthrough-heavy; if live capture contradicts paths, fix is contained in 03-03.
2. **List/find response item shape** (MEDIUM)
   - Presumed `{items: [ProjectRecord]}` with create/modify fields + `name`; exact extras (views count? runtimeUsageFlags?) unknown until capture. Mitigation: typed core fields + `flatten` passthrough (the 02-02 pattern).
3. **Import/export response bodies + overwrite wipe semantics** (MEDIUM)
   - Restart returns literal `true`; mcp falls back to `{"status": "success"}`. Treat as opaque-success; e2e + live capture pin them. Whether overwrite drops resources absent from the ZIP: assume yes (replace), verify in e2e.
4. **No live rig token exists right now** (environment fact)
   - `ign-research` is RUNNING on 18088 (`/StatusPing` → RUNNING) but no `IGNITION_LIVE_TOKEN` is set anywhere and the UI admin credentials are not recorded. Research was therefore repo-local + official-collection verified. Creating a token via the UI (02-USER-SETUP §Gateway) takes ~1 minute and unlocks the live suite during execution.
5. **`parents/:name` cycle-guard semantics** (LOW)
   - Inferred purpose (exclude self/descendants); not documented. Only matters if `set --parent` pre-validates; server remains the authority either way.
6. **Create/copy/rename response bodies** (LOW stakes)
   - Probably the created/renamed ProjectRecord or empty; model as `()` + passthrough if a body arrives; capture during e2e.

## Sources

### Primary (HIGH confidence)
- **Official `inductiveautomation/83-api` collection** — local clone `~/whiskeyhouse/83-api` (git remote verified = official repo; current with upstream `5c599a8` "API route updates to Postman and bruno"): full projects family request/response/query-param schemas + descriptions, scan/scan-lock family. Both `postman/8.3.postman_collection_v2.json` and `bruno/` halves read directly.
- **`whk-distillery01-ignition-global`** — real git-module-managed 8.3 project export read directly: `project.json`, resource-tree layout, `resource.json` schema, `data.bin` binary resources, `ignition/` core-module folder, tags-outside-project convention.
- **Repo internals** (read directly): `crates/ignition-core/src/client/{mod,query,logs}.rs` (pipeline helpers, timeout override, LogDownload buffered precedent, `scan_projects()` exists), `crates/ignition-cli/src/{main,render,cli}.rs` (dispatch shape, require_confirmation ordering, render_ok interception), `Cargo.toml` (workspace features: reqwest lacks `stream`, tokio lacks `fs`), `.planning/phases/02-gateway-health-inspection/*` (RESEARCH, PLAN/SUMMARY conventions, USER-SETUP, openapi extract), `.planning/STATE.md` (LOCKED decisions), `.planning/research/*.md` (project-level research).
- **Context7 `/seanmonstar/reqwest`** — `stream` feature gating for `bytes_stream`/`Body::wrap_stream`; raw-body + custom content-type usage.

### Secondary (MEDIUM confidence)
- **`WhiskeyHouse/ignition-mcp`** (author's production client, read directly) — resource-endpoint family (single source), import `Content-Type: application/zip` + `overwrite` param, export `Content-Disposition` parsing, `quote` encoding strategy. Trusted-but-verify: Phase 2 caught two invented paths in this client.
- **docs.inductiveautomation.com** — 8.3 API Documentation page (openapi mechanism, X-Ignition-API-Token, mutation audit-logging: "Mutative rest API requests… are recorded in audit logs"; GETs are not) and the IA forum 8.3 API Usage Guide (config-resource route taxonomy, scan-lock workflow context).
- **Live rig `ign-research` (8.3.6, RUNNING on :18088)** — `/StatusPing` verified RUNNING; no token available, so no authenticated probes were attempted (read-only rule).

### Tertiary (LOW confidence)
- igw-cli README (fetched) — confirms `--collision-policy Abort` default convention and `--yes` mutation-guard vocabulary; no project-resource coverage (absence informed the verification gap, not any positive claim).
- grep.app code search — absence of third-party resource-endpoint usage (absence-of-evidence, noted as such).

## Metadata

**Confidence breakdown:**
- Project CRUD endpoints/payloads: HIGH — official collection + working client agree on every path.
- Export/import mechanics: HIGH (paths/params) / MEDIUM (response bodies, overwrite wipe semantics).
- Resource-level family: MEDIUM — single-source; contained in 03-03 with live-capture gate.
- Scope metadata (includes/excludes): HIGH — verified from real export tree.
- Repo conventions/inheritance: HIGH — read from code/STATE.md.
- Streaming/dep plan: HIGH — features verified against reqwest docs; exact `futures_util` import ergonomics left to impl.

**Research date:** 2026-08-22
**Valid until:** Ignition 8.4 or a breaking 8.3.x API change (re-verify resource family via `/openapi.json` extract once a rig token exists)
