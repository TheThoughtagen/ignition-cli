# ign — operate Ignition 8.3+ gateways from the terminal

[Documentation](https://thethoughtagen.github.io/ignition-cli/) · [Releases](https://github.com/TheThoughtagen/ignition-cli/releases) · [Issues](https://github.com/TheThoughtagen/ignition-cli/issues)

One binary that lets a developer (or an AI agent) fully operate and inspect an
Ignition 8.3+ gateway — health, projects, tags, rigs — without opening the
gateway webpage or Designer. Every subcommand is non-interactive by default
and scriptable with JSON output; `ign tui` (below) is the interactive
cockpit over the same actions layer. v1.1 completes the agent surface end
to end: `ign api call` passthrough + curated daily checks, guarded EAM
write lifecycle with blast-radius preview, byte-faithful XML/CSV tag
transfer, a workspace checkout / `ign edit` round-trip, and protocol
transports — MCP (`ign mcp serve`) and LSP (`ign lsp`) — so agents and
editors drive the same binary humans do.

## Watch it in use

### Ignition CLI

Gateway inspection, project discovery, and batch tag reads.

[![Ignition CLI recorded terminal walkthrough](website/static/demos/cli.gif?raw=true)](https://thethoughtagen.github.io/ignition-cli/docs/demos/)

### Ignition CLI TUI

Explore gateway health, browse tags, and inspect projects from the keyboard.

[![Ignition CLI TUI recorded terminal walkthrough](website/static/demos/tui.gif?raw=true)](https://thethoughtagen.github.io/ignition-cli/docs/demos/)

[Open the recordings with playback controls and written summaries](https://thethoughtagen.github.io/ignition-cli/docs/demos/). Recorded against a fictional batch process; playback runs at 1.2×.

## Install

```
cargo install ignition-cli
```

(or grab a prebuilt binary from the
[releases](https://github.com/TheThoughtagen/ignition-cli/releases) —
macOS arm64/x86_64, Linux x86_64/arm64). Requires Rust 1.88+ when building
from source. The binary is `ign`.

Agent harnesses can also install the playbook: the
[`ignition-ops` skills](claude-code-plugin/README.md) (5 SKILL.md files —
contract, gateway, tags, workspace, rigs) ride any
[skills-CLI](https://github.com/vercel-labs/agent-skills)-compatible
agent:

```
npx skills add TheThoughtagen/ignition-cli -g
```

Claude Code can alternatively load them as a plugin (see
[Agent transports](#agent-transports--mcp-ign-mcp-serve-and-lsp-ign-lsp)
below).

## Output contract (for agents)

Success prints a stable envelope on **stdout**; errors go to **stderr** in
both modes (exit code carries the same signal):

- Success: `{"ok": true, "profile": <name|null>, "data": {...}}`
- Failure: `{"ok": false, "profile": <name|null>, "error": {"code": "<slug>", "message": "...", "endpoint": <url|null>, "hint": "..."}}`

`--compact` implies `--json` and renders the same envelope on a single line.
Two documented exceptions: clap usage errors exit 2 and are rendered by clap
itself (not JSON); the exit-2 class also covers destructive operations run
without `--yes`.

One success-path exception: `ign completions <SHELL>` writes the raw
completion script to stdout regardless of `--json` — shells source its
output directly, so it is never JSON-wrapped.

### The `ign api call` contract exception

`ign api call` is the one documented exception on the SUCCESS side:
its `data.result.data` is the gateway's OWN JSON — **gateway-verbatim**
(no field dropped, no value coerced, key order preserved), not a
curated model. A 2xx answer with a **non-JSON** body refuses exit 1
`internal` with an explanatory message — binary endpoints belong to
the logs/backup download pipelines, never this command. Gateway error
bodies on the api-call path ride the error envelope's message
verbatim, truncated at **4 KiB** with an explicit `... [truncated]`
marker. Auth-pattern headers (`Authorization`, `X-Ignition-API-Token`,
`Cookie` — case-insensitive) are refused exit 2 BEFORE any request:
credentials come from the profile, full stop.

### The MCP refusal-prose exception (2026-09-16)

The frozen envelope governs the CLI's own stdout/stderr only —
byte-for-byte, always. Over the MCP transport (`ign mcp serve`), a
`confirmation_required` refusal is the SECOND dated envelope-transport
exception (alongside `ign api call` above): its `message`/`hint` PROSE
adapts to the transport — the JSON names `confirm: true` (the
MCP-native confirmation) and marks `--yes`/`IGNITION_YES=1` CLI-only —
while the shape stays byte-stable (same keys, same order,
`ok:false`/`code`/`profile:null`/`endpoint:null` untouched). The MCP
input schema correspondingly keeps `confirm` OUT of `required` for
guarded verbs: schema-required would invite agents to auto-fill
`confirm:true`, so omission is legal and answered by the refusal.

## Exit codes

| Code | Class         | Meaning                                            | Stable slugs
|------|---------------|----------------------------------------------------|-----------------------------------------------|
| 0    | ok            | success                                            | —
| 1    | internal      | unexpected failure — report as a bug               | `internal`
| 2    | usage         | usage error (rendered by clap), destructive op without `--yes`, an invalid import file, an unreadable command input, or a raw api call the gateway rejected with an unclassified 4xx (body carried verbatim) | `confirmation_required`, `invalid_import_file`, `invalid_input`, `gateway_client_error` (P9)
| 3    | config        | local configuration problem                        | `profile_not_found`, `no_active_profile`, `secret_unavailable`, `password_unavailable`, `config_invalid`, `poll_interval_too_small`
| 4    | network       | gateway, or a module release feed, unreachable / timeout / TLS | `network_error`, `module_feed_unreachable`
| 5    | auth          | gateway rejected credentials                       | `auth_rejected`
| 6    | target_state  | command invalid for the gateway's current state, or a module artifact the feed refused to serve or verify | `gateway_too_old`, `gateway_not_commissioned`, `gateway_restarting`, `not_found`, `project_exists`, `resource_binary`, `trial_not_expired`, `provider_not_found`, `routes_not_deployed`, `webdev_unlicensed`, `route_version_mismatch`, `webdev_route_error`, `tag_collision`, `alarm_journal_missing`, `import_denied`, `session_not_prunable`, `eam_not_controller`, `eam_task_type_refused`, `eam_task_in_flight`, `script_exec_not_configured`, `lint_tool_absent`, `node_tool_absent`, `provider_root_unsupported`, `bundle_not_available`, `module_release_not_found`, `module_digest_mismatch`, `module_feed_unusable`, `module_digest_changed` |
| 7    | rig           | docker/compose rig failure (discovery, lifecycle, port conflicts) | `rig_error` |

The exit-code table lives in exactly two places — this README and
`CoreError::exit_code()` in `crates/ignition-core/src/error.rs` — kept in
sync by two tests: `exit_code_mapping_enumerated` pins the enum to its
exit-code/slug literals, and `readme_exit_table_agreement` parses THIS
table via `include_str!` and cross-checks every slug ↔ exit-code row
against those literals in both directions, so a missing, stale, or
misplaced table row fails the test suite (the Three-Place rule: enum +
literals + this table, all machine-checked).

`ign api call` partitions gateway answers through the same classifier:
401/403 → 5 `auth_rejected`, 404 → 6 `not_found`, 503 → 6
`gateway_restarting`, 500 → 1 `internal`, and every OTHER 4xx → 2
`gateway_client_error` with the response body carried verbatim in the
message (4 KiB cap, `... [truncated]` marker). Usage-class input
problems (auth-pattern headers, bad `--path`, malformed
`--header`/`--query`) refuse exit 2 `invalid_input` BEFORE any
resolution — the envelope's `profile` is null and zero requests hit
the gateway.

## Gateway authentication (8.3)

Machine auth against Ignition 8.3 `/data` REST routes is **token-only**:

- Send the header `X-Ignition-API-Token: <name>:<key>` — the FULL
  `name:key` string the gateway UI shows when you create the key
  (Platform → Security → API Keys). Key-only headers are rejected (401).
- **Basic auth does not work on 8.3 `/data` routes** (verified: valid
  commissioned admin credentials → 401). `ign` keeps the basic-credential
  profile arm for future/legacy surfaces, warns loudly on stderr each
  time it is used against the gateway, and never silently retries —
  create an API token instead.
- Token-authenticated mutations need **no CSRF** machinery (CSRF applies
  only to cookie/session auth).

Creating a working token: gateway UI → Platform → Security → API Keys →
Create (Basic Token) → pick a security level → **uncheck "Require secure
connections"** for `http://` gateways → copy the full `name:key` string.

The two rejection codes carry distinct diagnoses:

| Response | Meaning | What to fix |
|----------|---------|-------------|
| 401 | token not recognized | the header must be the full `name:key` string — no `name:` prefix or wrong key → 401; Basic never works on `/data` |
| 403 | token recognized but under-permitted | three-part setup: (1) token holds an adequate security level, (2) gateway read/write permissions include that level, (3) "Require secure connections" unchecked for http — `ign doctor` diagnoses all three |

### Token-setup troubleshooting (the three-part failure, `ign doctor`'s bread and butter)

A 403 means the gateway RECOGNIZED your token but its security level
doesn't satisfy the gateway's permissions. The setup has exactly three
parts — all three must hold:

1. **The token holds an adequate security level.** When creating the
   key (Platform → Security → API Keys), pick a level the gateway's
   permissions already accept — only leaf paths count (granting
   `Authenticated/Roles` mid-tree logs a WARN and is ignored).
2. **The gateway's read/write permissions include that level.** Under
   8.3 defaults, Gateway Read/Write Permissions only include
   `Authenticated/Roles/Administrator` — a token granted just
   `Authenticated` gets 403 (verified live, then fixed by adding the
   level to the permission lists). `ign doctor` reads the
   `security-properties` config and shows you the actual wiring; when
   even that read 403s, the wiring is your culprit.
3. **"Require secure connections" is unchecked** when the gateway URL
   is `http://` (the create dialog CHECKS it by default — the #1
   http-rig trap; uncheck it).

`ign doctor` walks the whole chain — URL/TCP reachability, liveness
(no credential involved, so down-ness is never confused with bad
auth), commissioning, 401-vs-403, the permissions wiring, write
permission, WebDev-route presence, and Docker presence — and exits 0
whenever the diagnosis completes so agents can parse `checks[]` from a
broken setup without a nonzero exit getting in the way.

### Gateway adoption (`ign adopt`)

Doctor diagnoses the three-part token setup; **adopt fixes it**. One
idempotent, profile-scoped verb walks a commissioned-but-unadopted
gateway to a fully working profile:

```bash
export IGNITION_PASSWORD='<gateway admin password>'
ign adopt --profile se-slot5-backend --project Flux --testing \
  --checkout ~/gateways/slot5 --bake ~/rigs/slot5/restore.gwbk
```

Five bootstrap steps, each reported as a doctor-shaped row, each
skippable when already satisfied — **re-running on an adopted gateway
is an all-skip no-op**:

| Step | What it does |
|------|--------------|
| `login` | The native OIDC dance (session cookie + CSRF) — the same machinery as `ign rig trial reset`, the only credential path that can mint the first key |
| `key` | Mints an Administrator-level API key named `ign-cli` with "Require secure connections" **off** — idempotent by name over the `ignition/api-token` resource list |
| `permissions` | Adds the key's level root to the gateway's read/write permission lists (merge-only, never removes) |
| `probe` | `gateway-info` with the fresh `name:key` — the live end-to-end proof |
| `persist` | Stores the credential in the **OS keyring** and rewrites the profile to `auth = { keyring = … }`; on headless hosts falls back to `IGNITION_TOKEN_<PROFILE>` and prints the token **exactly once** |

Composition flags ride the bootstrap after the key is proven working:
`--project NAME` deploys the CLI's WebDev routes (scriptExec on, the
existing secret lifecycle), `--project … --testing` additionally ships
the embedded Jython test framework + `testing/run`/`testing/tags`
routes and **asserts the suite is live** before reporting green
(discover ≥1 module + a passing smoke run — the empty-suite trap;
a freshly deployed route's first-touch lazy-compile 500 is retried
once), `--checkout DIR` lands every enabled project at
`DIR/<project>` (scripts decoded, existing targets skip), and
`--bake FILE` saves a roaming gwbk **after** everything landed — a
gateway reset restored from it keeps the key, the wiring, and the
routes (live-proven on 8.3.6).

Flags: `--user NAME` (default `admin`), `--key-name NAME` (default
`ign-cli`), `--level PATH` (default `Authenticated/Roles/Administrator`).
The password comes from `IGNITION_PASSWORD` env-only (missing → exit
3, rejected → exit 5). Unlike doctor, adopt **mutates**: a failed
step is a hard error through the normal exit-code taxonomy, never a
skip-ahead table. A same-name key that is disabled or
secure-connections-only is a hard error (its plaintext is
unrecoverable — delete it and re-run).

`--testing` only **smoke-checks** the bundle it deploys (discover ≥1
module + a green `testing.__tests__` sentinel run). Running the suite
itself is [`ign testing run --project NAME`](#commands) — the
standalone verb over the same `testing/run` route, with `--discover`,
`--module`/`--package` selection, `--format junit|text`, and the
verdict in the exit code (0 green, 6 red with the full results still
in the envelope).

Wire shapes, pitfalls, and the fresh-gateway verification log live in
[`.planning/research/ADOPT-RESEARCH.md`](.planning/research/ADOPT-RESEARCH.md).

### Live-gateway verification (opt-in)

An `#[ignore]`-gated suite (`crates/ignition-core/tests/live_gateway.rs`)
runs read-only checks against a real commissioned gateway when
`IGNITION_LIVE_URL` (and optionally `IGNITION_LIVE_TOKEN`) are set:

```bash
cargo test -p ignition-core --test live_gateway -- --ignored
```

With no envs set the suite skips cleanly (green no-op). The file header
carries the one-command Docker rig recipe for reproducing a test gateway.

## Commands

| Command | What it does | Notes |
|---------|--------------|-------|
| `ign version` | CLI version; gateway check when a profile resolves | unreachable gateway degrades to a warning (exit 0) |
| `ign status` | Identity, platform (Java/OS), uptime, CPU/memory/disk, license incl. trial countdown | merges gateway-info + `/overview` + unauthenticated `/StatusPing`; authed read (exit 3 without a secret, exit 5 on bad credentials) |
| `ign modules [--quarantined]` | Every module: `id  name  version  state  licenseState` | default = healthy list; `--quarantined` swaps to the quarantined list (usually empty) |
| `ign metrics [--history]` | Current CPU %/heap and thread execution counts | `--history` appends first/last datapoint summaries per series (`systemPerformance` endpoints) |
| `ign sessions [--type designer\|perspective\|vision]` | All three session families in one merged output (`designers (N)` / `perspective (N)` / `vision (N)` sections) | `--type` filters to one family; JSON always carries all three keys (filtered-out = `[]`); replaces the webpage's Sessions pages |
| `ign sessions terminate --type <T> --id <ID> [--message MSG]` | Terminate a session (designer: prune / vision: close / perspective: terminate, `--message` shown to the user) | **destructive**: exit 2 (`confirmation_required`) without `--yes` or `IGNITION_YES=1`; a nonexistent id exits 6 (`not_found`); pruning a LIVE Designer session answers 409 on the gateway and exits 6 (`session_not_prunable`) — prune removes stale entries only, close the Designer first |
| `ign connections [--type database\|opc]` | Database/OPC connections: `name  enabled  healthchecks` | `healthchecks` is passthrough as the gateway reports it (populated detail LOW-confidence until captured live); replaces the webpage's Connections pages |
| `ign project list` | Every runnable project: `name  title  enabled  parent  inheritable` | inheritance info comes from the list items themselves; JSON items also carry `description` (all six keys always present, null when unset); replaces the webpage's Projects list |
| `ign project new <NAME> [--title --description --parent --inheritable --disabled]` | Create a project | only provided fields ride the create body (never empty-string references); the result is a `find` read-back; audit-logged server-side |
| `ign project copy <SRC> <DST>` | Copy a project with all its resources | non-destructive (creates DST) — no `--yes`; audit-logged server-side |
| `ign project rename <OLD> <NEW>` | Rename a project (native rename, not copy+delete) | non-destructive relabel — no `--yes`; audit-logged server-side |
| `ign project set <NAME> [--title --description --parent --set-enabled\|--disabled --inheritable BOOL]` | Set project fields — `--parent` IS the inheritance move (reparent) | only provided flags ride the modify body (absent = untouched); at least one field required; audit-logged server-side |
| `ign project delete <NAME>` | Delete a project | **destructive**: exit 2 (`confirmation_required`) without `--yes`; the wire DELETE always carries the server's own `confirm=true` query param (both guard layers); a nonexistent name exits 6 (`not_found`); audit-logged server-side |
| `ign project export <NAME> [-o FILE]` | Export a project as a ZIP archive | the ZIP STREAMS to disk chunk-by-chunk (no memory buffering; 120 s per-request timeout); default filename from `Content-Disposition`, else `<name>.zip`; stdout stays data-only — JSON carries `{project, file, bytes, scope}` (see scope metadata below) |
| `ign project import <NAME> --file PATH\|--file - [--collision-policy abort\|overwrite]` | Import a project from a ZIP (`-` reads stdin) | default policy **abort**: importing over an existing name exits 6 (`project_exists`) BEFORE any upload; **overwrite** is destructive — exit 2 without `--yes` and it REPLACES the entire project (resources absent from the ZIP are deleted; merge is Designer-only); a non-ZIP, >512 MB, or structurally-corrupt (truncated) input exits 2 (`invalid_import_file`) before any network I/O — every member is validated up front because the gateway would otherwise accept a truncated ZIP and wipe the target (live-witnessed); a gateway import refusal riding HTTP 200 (`{success:false}`) exits 6 (`import_denied`) with the gateway's problem text; 300 s per-request timeout |
| `ign project diff <PROFILE_A> <PROFILE_B> --project <NAME>` | Compare a project across two gateway profiles — per-resource `added`/`removed`/`changed`/`same` statuses (read-only, no guard) | **statuses are B-relative-to-A**: `added` = in B only, `removed` = in A only, `changed` = differing content after `resource.json` normalization (`attributes.lastModification`/`…Signature` stripped, keys canonicalized — identical content exported from two gateways reports `same`); each side exports once and NOTHING is imported; the envelope's `profile` stays the ACTIVE profile while the data carries `profile_a`/`profile_b`; the root `project.json` rides `project_meta` (title/enabled/parent deltas), never the resource entries; diffing a profile against itself exits 2 `invalid_input`; scope is project-only (see the tag-promotion pipe below); a missing project on either side exits 6 `not_found` |
| `ign project sync <PROFILE_A> <PROFILE_B> --project <NAME> --resource PATH... [--all-changed] [--delete]` | Promote selected resources from A into B (direction is ALWAYS A→B — source A, target B) | **destructive on B**: the whole project is overwrite-imported — exit 2 (`confirmation_required`, profile null, ZERO requests) without `--yes`; at least one of `--resource` (repeatable) or `--all-changed` required (else exit 2 pre-resolution); `--all-changed` promotes everything A has that B lacks or differs on (the diff's `removed`+`changed` under B-relative-to-A labels); default is upsert-ONLY — B's extra resources are never deleted unless `--delete` is passed (then the diff's `added` set — B-only — is removed; an explicit `--resource` path absent in A is a deletion request under `--delete`, `not_found` without); replace_member's descriptor-merge landing rules ride free; B's `project.json` is never touched; `--all-changed` with nothing changed performs NO import (zero-write honesty); JSON data `{scope, profile_a, profile_b, project, synced, removed}` |
| `ign workspace checkout <PROJECT> <TARGET> [--decode-scripts]` | Check a project's resources out to a local tree — mapped paths + the recorded `.ign-workspace.json` manifest + an idempotent `.gitignore` | read-only on the wire (one export GET, ZERO imports); an unrelated non-empty target refuses exit 2 (never clobbered — the pre-existing file is untouched), a corrupt or foreign-manifest target refuses naming what it found, a same-project re-checkout refreshes; `--decode-scripts` decodes embedded JSON scripts to editable `.py` sidecars (nvim-editable; the UNEDITED re-encode is byte-exact — see the workspace section and Script decode/encode below); profile NAME rides into the manifest; JSON data `{project, target, member_count, scripts_decoded}` |
| `ign workspace status [PATH]` | Report workspace drift against the gateway — a PATH/STATE table (clean rows INCLUDED — agents diff full state) + the `x local edits, y gateway drift, z conflicts, n untracked` summary | states are **push-relative** (each row names what push would do — the table in the workspace section below); the project comes from the manifest, never re-typed; a missing/corrupt/foreign-schema manifest refuses exit 2 PRE-resolution (profile null, ZERO requests — the usage-guard convention); read-only (one export GET); JSON data `{project, clean, rows}` |
| `ign workspace push [PATH] [--delete]` | Push local edits to the gateway — manifest-recorded local bytes spliced into a FRESH export, imported exactly once | **destructive**: exit 2 (`confirmation_required`) without `--yes` and the refusal message IS the blast-radius preview (exactly what would be written/deleted); **conflicts refuse EVEN WITH `--yes`** (manual reconciliation — see the workspace section); locally-deleted members need `--delete` (default: reported as `skipped`); untracked files are NEVER imported; an empty selection writes nothing and never prompts; JSON data `{project, wrote, deleted, skipped}`; over MCP the tool carries an OPTIONAL `confirm` boolean and `confirm: true` is the MCP-native confirmation for it — `--yes`/`IGNITION_YES` remain CLI-only, and the conflict refusal is still never confirmable on either surface |
| `ign edit <PROJECT> <RESOURCE_PATH> [--yes]` | Fetch a gateway resource, edit it in `$EDITOR`, push back — the kubectl-edit loop over ONE resource (whole-tree authoring is `ign workspace`'s job) | **destructive**: the push re-imports the project — exit 2 (`confirmation_required`) without `--yes` and the refusal message IS the blast-radius summary; **ZERO stdout in every mode** (the editor owns the terminal — all prose on stderr, see the edit section); an unchanged save is a clean no-op (nothing pushed, never prompted); a gateway changed since fetch refuses EVEN WITH `--yes` (`changed on gateway since fetch` — re-run to fetch fresh); a JSON-breaking save refuses FAIL-CLOSED with the temp tree kept (path printed); no `$EDITOR`/`VISUAL` refuses exit 2 pre-resolution (profile null, zero requests) |
| `ign resource list <PROJECT> [--prefix PREFIX]` | A project's resource members, one path per line | rides project-export ZIP surgery (05-02): the project is exported, the member tree under `<collection>/resources/…` is mapped to user paths (`resources/` stripped — `ignition/script-python/…`); `--prefix` filters client-side; JSON items carry exactly the typed `path` |
| `ign resource get <PROJECT> <PATH>` | Read ONE resource: JSON pretty-printed, text raw — the surgical edit loop's first half | `PATH` keeps its slashes (e.g. `ignition/script-python/myscript`) and addresses a ZIP MEMBER (the file at `<collection>/resources/<rest>`); a binary member (data.bin-class, sniffed from the member bytes) refuses with exit 6 `resource_binary` (use export/import instead — never corrupted through the JSON loop); JSON data carries `{project, path, content_kind, content}` |
| `ign resource put <PROJECT> <PATH> --file PATH\|--file -` | Write ONE resource member (upsert: created if absent, replaced if present) | **destructive**: the whole project is re-imported (`overwrite=true`) after the member surgery — exit 2 (`confirmation_required`) without `--yes`; content is sniffed (json/text); binary input refuses exit 6 `resource_binary` before any network I/O; an unreadable file/stdin exits 2 `invalid_input`; concurrent Designer edits are REPLACED (see the resource section) |
| `ign resource delete <PROJECT> <PATH>` | Delete ONE resource member | **destructive**: exit 2 (`confirmation_required`) without `--yes`; the surgery drops the member and re-imports the project; a nonexistent path/member exits 6 (`not_found`) |
| `ign logs [--logger L] [--min-level L] [--since SPAN] [--limit N]` | Recent log entries, newest first (`ISO-UTC  LEVEL  logger  message`) | `--limit` is ALWAYS explicit (default 200 — the server default is unlimited); `--since` takes EPOCH-MS or `500ms/30s/5min/2h`; sorts `desc(timestamp)` so you see the NEWEST entries, never the oldest 200 |
| `ign logs -f [--interval S] [--timeout S]` | Live tail: entries stream to stdout as they occur | poll-based (no server push exists — `GET /logs?startTime=<cursor>` IS the tail); `--timeout` expiry ends cleanly (exit 0); without it, run until Ctrl-C (default process kill, no envelope); see the streaming exception below |
| `ign logs download [-o FILE]` | Download the log archive — a SQLite `.idb`, never a zip | bytes written exactly as received; default filename from `Content-Disposition`, else `<profile>-logs-<ts>.idb`; `--json` data is `{file, bytes, content_type}` |
| `ign logs loggers [--search S]` | Logger registry: `name  level  context` | explicit limit 200 (same unlimited-default guard) |
| `ign logs loggers set <NAME> <LEVEL>` | Set one logger's level (TRACE..OFF) | **mutation**: exit 2 (`confirmation_required`) without `--yes`; audit-logged server-side |
| `ign logs loggers reset` | Reset ALL custom logger levels to defaults | **mutation**: exit 2 without `--yes`; audit-logged server-side |
| `ign restart [--wait] [--timeout S] [--interval S]` | Restart the gateway — the one big red button | **always `--yes`-guarded** (it takes the whole gateway down); without `--wait`: POST + "READY in ~1 min" advisory; with `--wait`: POST → 5 s floor → polls the unauthenticated `/StatusPing` until RUNNING (default 300 s budget; a timeout exits 4 naming the last observed state) |
| `ign wait gateway [--interval S --timeout S]` | Wait until the gateway reports RUNNING | unauthenticated `/StatusPing` poll — works with no/broken credential; already-RUNNING = immediate success (default 120 s) |
| `ign wait restart [--interval S --timeout S]` | Wait for a restart to complete | shares `restart --wait`'s semantics: a non-RUNNING state observed once → RUNNING completes immediately (witnessed restart, no floor wait); an all-RUNNING wait reports success only after the same 5 s floor — no false positive when run right after `ign restart` |
| `ign wait module <ID> [--interval S --timeout S]` | Wait until a module reports ACTIVE | polls `modules/healthy?search=<id>` (authed); timeout names the id + last observed state |
| `ign doctor [--check-write] [--webdev-route NAME]` | Diagnose the setup: url (parse + TCP dial), liveness (unauth `/StatusPing`), commissioning (302→`/welcome`), auth (401 vs 403), the permissions deep-dive (`security-properties`), write permission, WebDev-route presence, Docker/rig presence | **exits 0 whenever the diagnosis completes** — failing checks are data, not CLI errors (agents parse `checks[]`; humans read the table); `--check-write` fires the harmless `scan/projects` rescan (2xx = write OK, 403 = read-only token); `--webdev-route NAME` probes that route's version action in the CLI's `ign-cli` WebDev project — **405 = absent** (the live-proven 8.3 marker; the earlier 404 assumption was wrong), 402 = module unlicensed, 200 = present (+ handshake version); config errors (no profile) still exit 3 |
| `ign adopt [--user NAME] [--key-name NAME] [--level PATH] [--project NAME] [--testing] [--checkout DIR] [--bake FILE]` | Bootstrap a commissioned gateway to a working profile: native OIDC login → mint an Administrator-level API key (`ign-cli`, secure-connections off, idempotent by name) → merge the level root into gateway read/write permissions → live-probe the fresh `name:key` → persist (OS keyring + profile rewrite; env-var fallback prints the token exactly once) | password from `IGNITION_PASSWORD` env-only (missing → exit 3, rejected → exit 5); **mutating-verb exits** — a failed step is a hard error (doctor-shaped `steps[]` rows, SKIP = already satisfied; re-run on an adopted gateway is an all-skip no-op); a same-name key that is disabled or secure-only is a hard error (delete it and re-run); composition flags run after the key is proven: `--project` deploys the WebDev routes, `--testing` (requires `--project`) ships the Jython test framework + testing routes and asserts discover ≥1 module + a green smoke run, `--checkout DIR` lands every enabled project (scripts decoded), `--bake FILE` saves a restore-ready roaming gwbk; see the [Gateway adoption section](#gateway-adoption-ign-adopt) |
| `ign session login [--user NAME]` | Obtain a live gateway login session through the native OIDC dance — the SAME flow `ign adopt` and the trial-reset ladder already run, exposed as JSON so a browser harness can skip the login form entirely. Returns `{cookie_name, cookie_value, csrf_token, gateway_url, storage_state}`, where `storage_state` is a ready-to-write Playwright `storageState` document (one cookie, empty origins) | **this verb emits a LIVE credential by design — that is its product**, and the exposure is disciplined: the session material (cookie value, CSRF token, `storage_state`) rides the envelope **only under the global `--json` flag**; the human render deliberately prints just the cookie NAME and the gateway URL and withholds the rest (pinned by a unit test over the pure render function, so a future edit cannot leak it silently). Password is env-only via `IGNITION_PASSWORD` (never a flag — the rig family's rule): **missing → exit 3 `password_unavailable`**, **rejected → exit 5 `auth_rejected`**, and neither refusal path emits any session material. `--user` resolves flag → `$IGNITION_USER` → `admin`. The profile resolves DEGRADED (no API token required — logging in is what happens *before* one exists). The storage-state cookie follows Playwright's rules: `domain` is the gateway host **without** its port, `path` is `/`, `httpOnly` true, `sameSite` `Strict`, `expires` `-1` (a session cookie), and `secure` true only for an `https` gateway. Consumed verbatim by the `ign e2e init` scaffold's `global-setup.mjs` |
| `ign webdev deploy [--project NAME] [--with-script-exec] [--rotate-secret] [--with-testing]` | Install the CLI's own WebDev route bundle into the dedicated `ign-cli` project (default) — `tags`, `tagConfig`, `alarms`, `tagHistory` (+ `scriptExec` only with `--with-script-exec`, + the testing bundle only with `--with-testing`) | **not `--yes`-guarded by design**: the dedicated project is CLI-OWNED — born from the first deploy zip, overwrite-REPLACED on every deploy (replace-not-merge is the contract here; user projects are never touched); every WebDev-dependent tag command (Phase 5) refuses exit 6 `routes_not_deployed` naming `ign webdev deploy` until this runs; `--with-script-exec` generates a fresh hex secret (stored in the profile config at 0600) when none exists, `--rotate-secret` regenerates unconditionally (requires `--with-script-exec`); the secret NEVER appears in any output, envelope, or log (it lives in exactly one place: the baked route zip member); `--with-testing` appends the embedded TESTING bundle (the Jython framework, the permanent `testing.__tests__` smoke sentinel, and the POST-only `testing/run`/`testing/tags` routes) — NOTE: a testing-only deploy OVERWRITES and drops `scriptExec`; compose both flags (or let `ign adopt --project … --testing` do it); JSON data `{project, routes, script_exec, secret_rotated, import}` |
| `ign webdev status [--project NAME]` | The version-handshake sweep: probe every route's version action — per-route `{route, status, deployed_version, expected_version}` | **a read — exits 0 whenever the sweep completes** (per-route degradation is DATA, the doctor precedent): `present`/`absent`/`unlicensed`/`auth_gated`/`secret_mismatch`/`version_mismatch` per route; the `ok` flag (data, not exit code) is true only when every always-on route is present with a matching version; scriptExec is probed ONLY when a secret is configured for the profile and never gates `ok` |
| `ign tags provider list` | The gateway's tag providers: `name  enabled  tags  health` (+ `(managed)` marker) | NATIVE config-resource REST (no deployed routes needed) — the healthy seam: `metrics.tagCount` + `healthchecks.status` ride as the gateway reports them; the built-in `System` provider (and any MANAGED-type) is flagged; JSON rows carry `{name, enabled, tag_count, health, managed}` (all keys always) |
| `ign tags provider create <NAME>` | Create a STANDARD tag provider | MVP creates the fixed STANDARD shape only (`{profile:{type:"STANDARD"}, settings:{}}` — the live-proven array-body POST); DB-backed providers are out of scope; audit-logged server-side |
| `ign tags provider delete <NAME>` | Delete a tag provider (find → signature → delete chain) | **destructive**: exit 2 (`confirmation_required`) without `--yes` — the guard fires before ANY resolution (zero network work); a nonexistent provider exits 6 (`provider_not_found`, hint names `ign tags provider list`); the delete embeds the record's server signature on the path; audit-logged server-side |
| `ign tags browse [PATH] [--filter SUBSTR] [--include-properties] [--project NAME]` | Browse tags as a tree — providers at the root, folders/tags nested | needs the deployed routes (see the version-negotiation matrix above — refuses exit 6 pre-deploy); **Property children are filtered out by default** (`--include-properties` keeps them — the display default); `--filter` is a case-insensitive substring on name and full path; human mode renders the indented tree with tagType badges, JSON is the flat `{path, name, tag_type, has_children, data_type}[]` (nesting derivable from `path`) |
| `ign tags browse --from-export PATH [--filter SUBSTR] [--include-properties]` | Browse a tag export OFFLINE — no gateway, no credential, no deployed routes (`profile: null`; the positional browse path and this flag are mutually exclusive) | THREE layouts accepted: the CLI's own `tags export` JSON (the interchange file — provider = file stem), a legacy `<provider>.json` whole tree, or a **git-module directory** (a `tags/` root, or the dir itself when it holds provider folders/`.json` files) — per provider: individual `.json`-per-leaf files (folders = directories, `_types_/*.json` = UDT definitions, names decoded from `%XX`-encoded filenames, dot-entries skipped, `System` excluded, `.tag-config.json` ignored) OR the legacy single-file tree; the output reuses the SAME tree render + flat JSON row shape as the live browse; a nonexistent path or unparseable JSON exits 2 `invalid_input` (zero network) |
| `ign tags read <PATH>... [--project NAME]` | Read one or more tag values — `path  =  value  [quality]  timestamp` | needs the deployed routes; always batch on the wire (a single path is a one-element batch); rows pass through VERBATIM — quality strings carry their own detail (`Good`, `Bad_NotFound`, …) and are never parsed further: a missing tag is DATA (exit 0, quality `Bad_NotFound`), not an error |
| `ign tags write <PATH> --value V [--project NAME]` | Write a value to a tag — returns the post-write quality | needs the deployed routes; **the write-scalar-is-JSON rule**: `--value` parses as a JSON scalar (`42`, `1.5`, `true`, `null`, `"quoted"`); text that does not parse is sent as the bare string (`--value hello` is the string `hello`); arrays/objects refuse exit 2 (`invalid_input`) before any network I/O — the tag value wire slot is a scalar; a nonexistent target writes back a `Bad…` quality (quality is data) |
| `ign tags config get <PATH> [--project NAME]` | A tag's configuration as (pretty) JSON — the surgical edit loop's read half | needs the deployed routes; the gateway hands `value`/`defaultValue` back as STRINGIFIED JSON — the CLI re-parses them into real JSON objects/arrays so agents see structured data, not JSON-in-a-string (unparseable and scalar-parse strings stay strings); JSON data carries `{project, path, tag_type, config}`; a missing tag exits 6 (`not_found` — the route's own denial); **provider-ROOT paths (`[default]` alone) exit 6 `provider_root_unsupported`** (see the bulk export note below); a config carrying history bindings appends an additive `history:` block in human mode (see **Tag↔historian bindings** below) |
| `ign tags config create <PATH> --file FILE\|- [--project NAME]` | Create a tag from a JSON definition (`-` = stdin) | needs the deployed routes; the definition is the configure shape — see the **configure-shape traps table** below; the CLI splits the path into configure's basePath + per-tag name (a bare path rides under `[default]`; the path-derived name wins over any `name` in the definition) and does NOT otherwise reshape the dict; collision policy `'a'` (abort): creating over an existing node refuses server-side; `--file` JSON errors exit 2 (`invalid_input`) pre-resolution |
| `ign tags config edit <PATH> --file FILE\|- [--project NAME]` | Edit a tag's configuration from a JSON definition (`-` = stdin) | the same configure call with collision policy `'o'` scoped to the single named node (edit = overwrite that node); NOT `--yes`-guarded — a single-node edit is not a project-wide destructive |
| `ign tags config delete <PATH>... [--project NAME]` | Delete tag configurations | **destructive**: exit 2 (`confirmation_required`) without `--yes` — the guard fires before ANY resolution (zero network work); the delete is batch on the wire (`deleteTags {paths}`); JSON data `{project, deleted}` |
| `ign tags udt types [--provider NAME] [--project NAME]` | List a provider's UDT types (`[provider]_types_` browse) | needs the deployed routes; JSON data `{project, provider, types: [{name, tag_type}]}` |
| `ign tags udt def <NAME> [--provider NAME] [--project NAME]` | A UDT definition (parameters + nested children, recursive) | needs the deployed routes; the SAME stringified re-parse applies (parameter `defaultValue`s and child values become real JSON); JSON data `{project, provider, name, definition}` |
| `ign tags export <PATH>... [-o FILE] [--format json\|xml\|csv] [--project NAME]` | Export tag subtrees — the bulk-transfer half | needs the deployed routes; `--format json` (default) fetches the gateway's native JSON interchange: the payload is parsed, validated (a list of subtrees), and written PRETTY — default file `<last-path-segment>.json` in the cwd, `-o FILE` overrides, **`-o -` prints the raw pretty payload in every mode** (the fourth sanctioned stdout exception — pipe it into `tags import --file -`); `--format xml` passes the gateway's own XML through RAW-BYTE (CRLF/no-declaration document verbatim, no added trailing newline; default file `.xml`); `--format csv` is **CLI-GENERATED and LOSSY** — see the CSV honesty note below (default file `.csv`; what was dropped prints on stderr and rides `data.loss_report`); JSON data `{project, paths, file, stdout, tag_count, loss_report?}`; **provider-ROOT paths (`[default]` alone, or a bare provider name like `default`) exit 6 `provider_root_unsupported`** — see the bulk export note below |
| `ign tags import --file FILE\|- --provider NAME [--format json\|xml\|csv] [--collision-policy abort\|overwrite] [--project NAME]` | Import a tag export into a target provider | needs the deployed routes; the provider must exist (`ign tags provider create NAME`); **the loss gate (TAGS-12)**: xml/csv inputs are SCANNED before anything is sent — if the scan reports losses, the CLI prints the loss report and refuses exit 2 `invalid_input` (profile null, zero requests) unless `--yes`; with `--yes` the structured scan summary rides `data.loss_report`; **the locked collision matrix**: abort (default) pre-checks by browsing the target and refuses exit 6 (`tag_collision`, hint names `--collision-policy overwrite`) BEFORE any write, then imports with server-side abort as the backstop; overwrite replaces existing tags — **destructive: exit 2 without `--yes`**, no pre-check (the server is the authority); merge is Designer-only (not a value); JSON data `{project, provider, collision_policy, imported, format, top_level_names, loss_facts, failed?, loss_report?}` |
| `ign tags alarms active [--source S] [--priority P] [--state S] [--project NAME]` | List ACTIVE alarms — `eventId (FULL uuid)  source  state  priority  name` | needs the deployed routes; only present filters ride the wire (kwargs passthrough to `system.alarm.queryStatus`); state strings read `'Active, Unacknowledged'` verbatim — never parsed; JSON rows carry `{event_id, source, state, priority, name}` (name null when the event carries none); the printed eventId is copy-pasteable straight into `tags alarms ack` |
| `ign tags alarms history --start T --end T [--project NAME]` | Query alarm history (journal rows, columns dataset-dependent) | needs the deployed routes; **a journal-provisioned gateway only** — default rigs refuse exit 6 `alarm_journal_missing` with the hint naming the provisioning chain (see **Alarm history** below); `--start/--end` take RFC3339 or epoch-ms; rows ride VERBATIM (the journal schema varies by Ignition version — the header IS the column list) |
| `ign tags alarms ack ID... --username NAME [--note NOTE] [--project NAME]` | Acknowledge alarms — the count + the unacknowledged remainder | needs the deployed routes; the gateway-scope 3-arg wire form needs the username, so `--username` is REQUIRED (the CLI never guesses one); **NOT `--yes`-guarded by design** — acknowledging never un-acknowledges anything (a state-advancing, read-adjacent verb); ids: full UUIDs pass through verbatim, SHORT prefixes expand against the active-alarm list (ambiguous → exit 2 naming the candidates; unknown → exit 2 naming the miss); the 8.3 return IS the unacknowledged remainder — `acknowledged` is computed honestly (requested − remainder); JSON data `{project, acknowledged, unacknowledged}` |
| `ign tags history query PATH... --start T --end T [--return-size N] [--aggregation MODE] [--project NAME]` | Query HISTORICAL tag values — the `t_stamp` column + one column per tag | needs the deployed routes; **structurally safe on ANY rig** (zero historians → a well-formed dataset with null values, exit 0); DATA requires a provisioned historian (see **Tag history** below); `t_stamp` is preserved EXACTLY (never renamed) and tag columns ride PROVIDER-RELATIVE (`[default]P5H/T1` surfaces as `P5H/T1` — live-proven); `--start/--end` take RFC3339 or epoch-ms; `--aggregation` defaults to the route's `LastValue`; JSON data `{project, paths, columns, rows, row_count}` |
| `ign rig [--rig NAME] up [--timeout S]` | Bring a Docker compose rig up (`compose up -d --wait`) and wait for the gateway | docker-only (`profile: null` envelope); `--timeout` is BOTH compose's `--wait-timeout` and the commissioned-probe deadline (default 300 s); a fresh-volume rig reports `"up, uncommissioned"` as DATA (exit 0, wizard URL in `warnings`) |
| `ign rig [--rig NAME] down` | Stop the rig (`compose down --remove-orphans`; volumes KEPT) | docker-only; the volume-deleting teardown belongs to `rig reset` |
| `ign rig [--rig NAME] reset [--timeout S]` | Tear the rig down AND remove its volumes, then bring it back up fresh (`down -v --remove-orphans` → pre-flight → `up --wait` → commissioned wait) | **destructive**: exit 2 (`confirmation_required`) without `--yes` or `IGNITION_YES=1`, BEFORE any discovery runs; `removed_volumes` in the data reports exactly what `-v` took; no stale project/trial state survives (a fresh volume usually boots uncommissioned — exit 0, wizard URL in `warnings`) |
| `ign rig [--rig NAME] status` | Structured rig status: services (state/health/ports), volumes, ports occupancy | docker-only; an ALLOWLIST only — never a compose-config passthrough (the resolved config contains gateway passwords); a down rig is exit-0 data |
| `ign rig [--rig NAME] logs [--tail N] [-f] [SERVICE]` | Stream the rig's container logs (`compose logs` passthrough) | raw lines in EVERY mode — the third stdout exception (see §Streaming); `--tail` default 200; `-f` follows until Ctrl-C (default process kill); compose diagnostics go to stderr, never the data stream |
| `ign rig [--rig NAME] trial status` | Show the rig gateway's trial state: licenseMode, trialState, seconds left, expired — plus the banners cross-check | **credential-free** (the trial/banners endpoints answer unauthenticated — verified live on 8.3.3 AND 8.3.6; a fresh rig with no token reports fine); addresses the RIG's derived gateway URL (never the profile's); data `{license_mode, trial_state, trial_remaining_s, expired, emergency, emergency_remaining_s, development, banners: {severity, expire_time_ms, active}, warnings}` — `banners.active` is the Pitfall-7 cross-check (`severity=="info"` AND `expireTime>now_ms`), never the primary truth (`expired` is) |
| `ign rig [--rig NAME] trial reset [--user NAME]` | Reset an EXPIRED trial to a fresh ~2 h window via the mechanism ladder | **destructive**: exit 2 (`confirmation_required`) without `--yes`, BEFORE any discovery; ladder = tier 0 `POST /data/api/v1/trial` with `X-Ignition-API-Token` (token from `IGNITION_TOKEN`) → tier 1 native gateway login (internal-IdP OIDC challenge dance → session cookie + CSRF header), creds `--user`/`IGNITION_USER` + `IGNITION_PASSWORD` (password NEVER a flag); success REQUIRES the read-back flip (`expired` false on re-fetch — a bare 2xx never suffices); a NON-expired trial refuses exit 6 `trial_not_expired` (the gateway 403s resets while active — live-verified); no creds at all → exit 3 |
| `ign rig [--rig NAME] snapshot [-o DIR]` | Snapshot the rig's gateway: native gwbk (`GET /backup?type=roaming`, STREAMED to disk) + per-project exports + `manifest.json` — composed in a timestamped dir | addresses the rig's derived gateway URL; requires `IGNITION_TOKEN` (the backup route 401s unauthenticated — live-verified shape); default dir `./ign-rig-snapshots/<rig>-<yyyyMMdd-HHmmss>/`; data `{dir, gwbk_bytes, projects, manifest_path}` — the manifest names BOTH composition exclusions (see §rig snapshot/restore) |
| `ign rig [--rig NAME] restore --file PATH [--timeout S]` | Restore a gwbk onto the rig's gateway (raw octet-stream POST), wait for the witnessed post-restore RUNNING | **destructive**: exit 2 without `--yes`, BEFORE any discovery; the restore is synchronous and the gateway RESTARTS after — success is a WITNESSED StatusPing→RUNNING (deadline floored at 300 s), never a bare 2xx; data always carries the token-clobber warning (`API tokens may have been reset by restore…`, see §rig snapshot/restore); requires `IGNITION_TOKEN` |
| `ign backup download [-o FILE] [--type roaming\|all]` | Download a gwbk from ANY profiled gateway (streamed to disk — the standalone sibling of `rig snapshot`'s gwbk leg) | `--type roaming` (default) = the portable backup; `all` includes gateway-specific state; default filename from `Content-Disposition`, else `<profile>-backup.gwbk`; read, unguarded; JSON data `{file, type}` |
| `ign backup restore <FILE> --yes` | Restore a gwbk onto THIS gateway (the ACTIVE profile's — `rig restore`'s standalone sibling without the rig wait) | **destructive — the 8th `--yes`-guarded verb**: exit 2 (`confirmation_required`, profile null, ZERO network) without `--yes`; REPLACES this gateway's state, then the gateway restarts and blocks for minutes (the 2xx is acceptance — see §Backups); a nonexistent/empty/non-regular file exits 2 `invalid_input` pre-network; JSON data `{restored: true}` |
| `ign eam history [--limit N] [--search TEXT]` | EAM task run history — `ISO  taskName  [level]  target  detail` rows (newest first) | rides the RUNTIME seam: a stock (non-controller) gateway refuses exit 6 `eam_not_controller` with the manual-flip hint (definitions still list — see below); `--limit` defaults to 200 and is ALWAYS sent explicitly (the server default is unlimited); outcomes are DATA (`Failed` + GNET-not-connected detail read exit 0); JSON data `{items, count}` — items passthrough under the gateway's own camelCase keys (`taskName`, `taskStart` epoch-ms, `taskType`) |
| `ign eam tasks [NAME]` | Task definitions: bare form lists (`name  type  schedule  state`); with a name shows the full definition + its scheduled state | rides the config-resource seam (`com.inductiveautomation.eam/eam-tasks`) — definitions answer on STOCK gateways (no controller needed); JSON list data `{tasks: [{name, task_type, schedule_mode, current_state}]}` (all keys always; `current_state` null on list records — find answers carry it); detail data `{name, definition, state}`; an unknown name exits 6 `not_found` |
| `ign eam task new <NAME> <TYPE> [--target NAME]... [--setting K=V]... [--definition PATH] [--schedule-mode MODE]` | Create a task definition — the typed guard ladder | **the planner-locked ladder**: `eam_backup` + OnDemand (the default schedule) fires UNGUARDED (it never auto-fires and only acts when forced); MUTATING types (`eam_restart`, `eam_sendProject`, `eam_sendResource`, `eam_sendTags`, `eam_activateLicense`, `eam_updateLicense`, `eam_unactivateLicense`) and ANY non-OnDemand `--schedule-mode` (`Immediate`/`Scheduled`/`AtTime`/`AtDelay` — they arm autonomous actions) need `--yes` (exit 2 pre-resolution, zero network); the FLEET-DESTRUCTIVE trio (`eam_restoreBackup`, `eam_installModules`, `eam_remoteUpgrade`) REFUSES outright — exit 6 `eam_task_type_refused` naming the EXT-03 (v2) scope (run them from the EAM console); `--setting K=V` auto-types scalars (bool/int ride typed, else string); `--definition PATH` deep-merges a full-JSON settings file over the composed `config.settings` (objects merge, arrays/scalars replace) — mutually exclusive with `--setting`; the POST body is the config-resource ARRAY shape with the live 8.3.3 profile/settings split (`config.profile` = `{type, scheduleMode}` only; `config.settings` = `{targetGateways, targetGroups, …}`); `targetGateways` defaults to `["_controller"]` when no `--target` is given (the controller itself); JSON data carries the composed definition verbatim |
| `ign eam task force <NAME> --yes` | Force-dispatch a task NOW (find → owner → POST → history read-back) | **destructive — always `--yes`-guarded** (dispatches to the agent targets immediately; exit 2 pre-resolution without); the guard is the two-tier blast-radius gate (see below): the refusal message IS the preview line naming the task, its agent targets, and the factual impact; the owner resolves from the healthcheck's `scheduledTaskState.details.owner` (fallback `eam`); a 2xx is DISPATCH acceptance — the run's OUTCOME lands in history as data (`Failed` + GNET-not-connected detail is the honest shape of an unconfigured agent; trial expiry blocks runs); JSON data `{task, owner, dispatched, history}` |
| `ign eam task suspend <NAME> --yes` | Suspend a task definition's scheduled dispatches (runtime seam) | **guarded — the two-tier blast-radius gate** (exit 2 pre-check + preview-in-the-refusal without `--yes`, zero writes; exit 6 `not_found` on a bad name BEFORE any prompt); on a stock gateway the runtime seam honestly refuses exit 6 `eam_not_controller` (manual-flip hint); TASK-scoped — there is NO agent-level suspend on the wire (see the reconciliation note below); JSON data `{task, action, previous_state, config_suspended, pending, fired, reason}` |
| `ign eam task resume <NAME> --yes` | Resume a suspended task definition (runtime seam) | the suspend inverse — same guard, same seams, same honesty; JSON data as suspend |
| `ign eam task cancel <NAME> --yes` | Cancel a task's PENDING execution (runtime seam) | same guard; nothing pending is an honest no-op (`fired: false` + the reason ride the data — a write we declined is reported, never disguised); a `canCancel: false` row surfaces the gateway's own refusal |
| `ign eam task modify <NAME> [--enable\|--disable] [--schedule-mode MODE] [--setting K=V]... [--description TEXT] --yes` | Rewrite targeted keys of a task definition (config seam — works on stock gateways) | full-record read-modify-write: every key the gateway answered rides back, only the targeted keys change, and the body carries the record's `signature` (a stale signature is refused by the gateway); `--enable`/`--disable` conflict (clap-validated); `--setting` reuses the `eam task new` K=V auto-typing and deep-merges over `config.settings` (repeatable); a modify that targets nothing refuses exit 2 pre-network (a no-op PUT would still rotate the server-side signature); **no `--rename` by wire honesty** — the gateway answers a renamed PUT with 404 empty; rename is the create-new + delete-old composite; the guard is the two-tier gate (preview reflects the post-change impact); JSON data `{task, changed, definition, put_outcome, readback}` |
| `ign eam task delete <NAME> --yes` | Delete a task definition (config seam — works on stock gateways) | **signature-keyed** (the gateway refuses a stale signature) behind the two-tier gate; JSON data `{task, deleted, affected}` — `affected` names the resources the gateway reported |
| `ign script run --code PY\|--file PATH\|- [--project NAME]` | Execute gateway-side Python (Jython) through the secret-gated `scriptExec` route — non-interactive, the route's entire purpose | **the opt-in is STRUCTURAL, not a flag**: `scriptExec` deploys only via `ign webdev deploy --with-script-exec` (which generates + persists the secret); without it the verb exits 6 `script_exec_not_configured` with ZERO HTTP, hint naming the deploy flag; **no `--yes` by design** — the deploy flag IS the opt-in and agents need it non-interactive; `--code`/`--file` are mutually exclusive (both or neither → exit 2 `invalid_input` before any resolution; `--file -` reads stdin — the agent pipe path); each run probes the route's version handshake then execs (two round trips); JSON data `{stdout, result, elapsedMs}` — ALL keys always; a route-side Python exception surfaces its traceback verbatim (exit 6 `webdev_route_error`); NO server-side execution timeout exists — a long-running script holds the HTTP connection (the client's per-request timeout class applies); the secret NEVER appears in any output mode (see the scriptExec posture below) |
| `ign testing run --project NAME [--discover] [--module DOTTED\|--package PREFIX] [--format json\|junit\|text]` | Run the gateway's own Jython test suite through the deployed testing bundle and return a machine verdict — the agent/CI verb (no Designer, no `ign adopt`) | `--project` is **required** (the bundle deploys per project and a run executes THAT project's gateway-side code — there is deliberately no `ign-cli` default); `--discover` lists the discovered test modules; `--discover`/`--module`/`--package` are mutually exclusive (exit 2 `invalid_input` naming both flags, with ZERO HTTP requests); **the verdict rides the BODY, never the status line** — the route answers 200 on green and **207** on failures/errors and both are success transport shapes, so `failed + errors > 0` is the only oracle; a green run exits 0; a RED run renders the FULL results in the **success** envelope and exits **6** with `data.slug = "tests_failed"` (the `ign lint --strict` precedent — the error envelope has no `data` field to carry results, and every test runner puts the report on stdout and the verdict in the exit code; `tests_failed` is NOT a `CoreError` slug and never appears in the exit-code table); an undeployed bundle exits 6 `routes_not_deployed` with a hint naming `ign webdev deploy --with-testing` / `ign adopt --project NAME --testing` (404/405/501 are all absent markers — a raw-status probe runs BEFORE the run POST so an absent bundle can never read as a passing suite); a first-touch lazy-compile 500 is retried exactly once (3 s) on each leg; `--format` renders **client-side** — the route is ALWAYS asked for json, so the verdict and exit code are identical in every format (the route's own junit/text answers stay HTTP 200 and carry no counts), with the rendered document in `data.report` and the structured truth in `data.results`; JSON data `{mode, project, modules, count, verdict, slug, passed, failed, skipped, errors, total, durationMs, report, results}` — ALL keys always; NO server-side execution timeout exists — a long-running suite holds the HTTP connection (the client's per-request timeout class applies) |
| `ign lint PATH... [--strict] [-- ARGS...]` | Lint local project files by delegating to `ignition-lint` (PATH-discovered; no gateway, `profile: null`) | **doctor posture**: exit 0 whenever the tool RAN — findings, `child_exit_code`, and the parsed JSON report ride as data (ALL keys always; `report` null + `stdout` verbatim when unparseable; `stderr_preview` capped at 4000 chars); `--strict` exits with the tool's own code for CI (envelope prints first — the one sanctioned success-path exit exception; 1 = findings at the `--fail-on` threshold); PATHS map to `--target <path>` pairs + `--report-format json` on an ARG VECTOR (never a shell string); anything after `--` passes through verbatim; no tool on PATH → exit 6 `lint_tool_absent` with the install hint (`uv tool install ignition-lint-toolkit`); pair with `project export --decode-scripts` to lint the decoded sidecars |
| `ign e2e doctor [DIR] [--project NAME]` | Diagnose the browser-E2E setup before (or instead of) scaffolding one: `node` (≥20, version-probed), `npm`, `@playwright/test` inside `DIR` (default `./e2e`), a downloaded chromium in the Playwright browsers cache, the gateway's **testing bundle**, and the gateway's **trial** state | **doctor posture — exits 0 whenever the diagnosis COMPLETES**: every finding is a `checks[]` row, never an exit code, so a toolless host, an unreachable gateway, and an undeployed bundle all still return the full report (agents parse `checks[]`; humans read the table). Six rows in a fixed order — `node`, `npm`, `playwright`, `chromium`, `testing-bundle`, `trial` — each in the standard `{name, status, detail, hint}` shape with `status` one of `ok`/`warn`/`fail`/`skip`. Read-only and **offline-safe**: chromium presence is a pure filesystem scan of `PLAYWRIGHT_BROWSERS_PATH` (else the per-OS default — `~/Library/Caches/ms-playwright` on macOS, `~/.cache/ms-playwright` on Linux) for a `chromium-*` or `chromium_headless_shell-*` entry, deliberately **not** `npx playwright install --dry-run`, which would reach for the npm registry. A missing browser or an uninstalled runner is `warn` (one command away), an absent or too-old node is `fail` with the install hint. The two gateway rows report **`skip`** when no profile resolves or no `--project` is given — `--project` has no default, and a profile that will not resolve is not an error for this verb (envelope `profile` is null); an absent bundle is `fail` hinting at `ign webdev deploy --with-testing`, an expired trial is `warn` hinting at `ign rig trial reset` |
| `ign e2e init [DIR] [--project NAME] [--run-project NAME] [--browsers] [--yes]` | Scaffold a Playwright browser-E2E suite into `DIR` (default `./e2e`) and `npm install` it: `playwright.config.mjs`, a `global-setup.mjs` that logs in via `ign session login` and gates on `ign testing run`, `lib/gateway.mjs` (tag/script helpers over the testing routes), `tests/example.spec.mjs`, `.gitignore`, `README.md` — seven members, all embedded in the binary | **requires the global `--yes`**; without it the verb writes NOTHING and spawns NOTHING and refuses exit 2 `confirmation_required` after previewing the target, every member with its would-be status, and both command lines verbatim — **the refusal IS the dry run**, which is why no separate `--dry-run` flag exists. **Idempotent**: an existing file is reported `skipped` and never overwritten, so a re-init over a suite you have edited is safe. A non-empty target that is not already a scaffold (no `playwright.config.mjs`) refuses exit 2 `invalid_input` **naming files it found**, before any write. A host with no `node` on PATH refuses exit 6 `node_tool_absent` with the same install hint `ign e2e doctor`'s node row carries. Both gates run before any write AND are re-run inside the action, so in-process TUI/MCP callers get the same refusals. `npm install` (and, with `--browsers`, the Chromium download) run with ARG VECTORS, never shell strings, streaming their output through; a **non-zero installer is DATA, not an error** — a network-less host is still left with a usable scaffold on disk. `--project` defaults to `ign-cli` (the CLI's own WebDev project), `--run-project` defaults to the effective `--project`; the gateway URL comes from the profile, falling back to `http://localhost:8088` when none resolves (scaffolding must work before a profile exists). JSON data `{dir, files[{path, status}], npm_install, browsers}` — ALL keys always | 
| `ign api call --method M --path /P [--data TEXT] [--header "Name: Value"]... [--query k=v]...` | Raw passthrough to any gateway REST endpoint — the escape hatch for the uncurated endpoint families (any HTTP verb accepted) | the envelope's `data.result.data` IS the gateway's JSON **verbatim** (no field dropped, no value coerced, key order preserved — the documented contract exception above); a non-JSON 2xx body exits 1 `internal` with the explanation (binary endpoints: use logs/backup downloads); auth-pattern headers (`Authorization`, `X-Ignition-API-Token`, `Cookie` — case-insensitive) refuse exit 2 pre-resolve with ZERO gateway requests (credentials come from the profile); `--path` must start with `/`, refuses host-shaped URLs and embedded `?` (use repeatable `--query k=v` — the ONE query mechanism); `--header` splits on the FIRST `:` (repeatable), `--data` is raw text on ANY method (GET/DELETE bodies allowed — curl parity); exit partition: 401/403→5, 404→6, 503→6, 500→1, every other 4xx→2 `gateway_client_error` with the body verbatim (4 KiB cap + `... [truncated]` marker) |
| `ign license status` | The license morning-check in ONE command: license mode + countdown, then per-hardware-key item rows (`name  title  version`) | merges TWO reads — `/data/api/v1/licenses` (the inventory) + `/data/api/v1/trial` (the trial companion); the license MODE lives on the trial wire, not the licenses payload (live-capture fact — fresh-rig `/licenses` carries no mode/edition keys); models are version-tolerant: unknown keys ride the JSON passthrough, license-array ELEMENT shapes were not capturable on fresh rigs (no activations) and stay lenient; JSON data `{license, trial}` — trial carries `license_mode`, `trial_seconds_left`, `expired`; authed read (exit 3 without a credential, exit 5 on bad auth) |
| `ign redundancy status` | The redundancy morning-check: role, project state, activity, peer connection, config access, sync/failover pending, uptime, last-sync | units AS CAPTURED on 8.3.3 + 8.3.6: `uptime` = **ms since gateway start** (wall-clock proven twice); `lastSyncTimestamp` = `-1` = **never synced** (the unit is NOT capture-proven — non-negative values read epoch-ms flagged as inference; human mode says so, JSON rides the raw value); `role` is a String (`Independent`/`Primary`/`Backup` seen — unknown future roles ride, never refuse); peer-connected shapes were not capturable on a single fresh rig (peer fields lenient); JSON data `{status}` with the gateway's own camelCase keys; authed read |
| `ign gan status` | The Gateway Area Network morning-check: total/running connections, in/out byte rates, remote gateways | the zero-connection capture IS the canonical shape — all zeros on a non-GAN gateway are healthy data, not absence of feature; byte rates ride JSON floats (`0.0`), counts ints; one-command truth (`overview/gan`) — the full remote-gateway list stays with `ign api call`; JSON data `{status}`; authed read |
| `ign diagnostics bundle generate` | Start a diagnostics support-bundle generation (POST, no body) — the 200 answer IS the fresh status | capture-encoded state machine (8.3.3 + 8.3.6, 09-02 rigs; `Invalid` added from the 2026-09-07 UAT): the captured vocabulary is `Generating` (mid-generation), `Valid` (terminal/ready), and `Invalid` (terminal "no current bundle" — see `wait`); PascalCase; `state` is a String, never an enum, so an unobserved future state rides verbatim and never refuses; generation takes ~2–6 s on a fresh rig — pair with `wait`; JSON data `{state}`; authed read |
| `ign diagnostics bundle status` | The bundle status: captured state + size | `fileSize` (bytes) is ABSENT while `Generating` (the key is missing on the wire — not null, not 0) and appears with `Valid`, equal to the download's byte count exactly; JSON data `{state, fileSize}`; authed read |
| `ign diagnostics bundle download [-o FILE]` | Download the bundle ZIP (streamed to disk) | the request rides a **300 s per-request timeout override** (the 30 s client default would truncate MB-sized bundles — Pitfall 8); bytes land verbatim (`PK\x03\x04` ZIP magic live-verified); default filename is the gateway's `Content-Disposition` basename, else `ignition-diagnostics-bundle-<unix_ts>.zip`; the ready state persists after download (repeatable — live-captured); JSON data `{file, bytes, content_type}` |
| `ign diagnostics bundle wait [--interval SECS] [--timeout SECS]` | Poll the status until the bundle is ready — defaults 2 s / 300 s (the restart --wait shape) | **the captured vocabulary drives termination** (three states, live-proven on the 09-02 rigs + the 2026-09-07 UAT): `Valid` (terminal/ready) exits 0 with `{state, fileSize}`; `Generating` (mid-generation) keeps polling; **`Invalid` is a TERMINAL steady state — the gateway's "no current bundle" answer** (observed 2026-09-07 on 8.3.6: a `Valid` bundle decays to `Invalid` within ~2 min unprompted and stays `Invalid`) — wait exits IMMEDIATELY exit 6 `bundle_not_available` naming the state; only a fresh `generate` changes it (polling cannot); an UNKNOWN state (outside the captured vocabulary) is honestly NOT terminal — it keeps polling until the deadline, and the deadline message carries the last observation instead of claiming unreachability (the gateway answered, so it never says the gateway was unreachable); a deadline with NO observation stays exit 4 `network_error`; JSON data `{state, fileSize}`; authed read |
| `ign profile add/list/use` | Manage gateway profiles | — |
| `ign completions <SHELL>` | Shell completion scripts | raw stdout regardless of `--json` |

All gateway commands honor the envelope (`--json`/`--compact`) with the
`[profile: NAME]` header in human mode. The inspection trio (`status`,
`modules`, `metrics`) replaces the gateway webpage's Status Overview,
Config > Modules, and Performance & Diagnostics pages; `sessions`,
`connections`, the `logs` tree, and the `project` tree replace its
Sessions, Connections, Logs console, logger-config, and Projects pages.

## TUI cockpit (`ign tui`)

An interactive ratatui cockpit over the SAME actions layer the CLI
dispatches through — every screen is the CLI family rendered live, not
a parallel implementation:

| Screen | What it shows |
|--------|---------------|
| Dashboard | Status/modules/metrics/sessions panels on a 5 s refresh; the global verbs (`version`, `connections`, `wait …`, `doctor`, `restart`) and the profile switcher (`p`) |
| Logs | Live tail (`ign logs -f`) with the level filter (`l`), follow/scrollback, and the loggers family behind `a` |
| Tags | The k9s-style browser: providers → tree → detail with on-demand reads; live watch (`w`); the tags family behind `a`; the Alarms tab carries active/history/ack |
| Projects | Project list → detail (record + resources) → resource preview; the project/resource/webdev families behind `a` |
| Rig | The compose rig's status summary, the full rig verb menu behind `a`, and a raw `rig logs -f` pane (`l`) |

Behavior contract:

- **Interactive-only.** `ign tui` requires a TTY — piped stdout
  refuses with a usage error (exit 2) before the alternate screen is
  ever touched. Profile resolution failures (no profile, missing
  secret) also surface BEFORE the terminal flips: the normal stderr
  envelope and exit taxonomy, not a flash of alt-screen.
- **Silent stdout on success.** The alternate screen owns all display;
  a clean exit prints NOTHING on stdout (no envelope, no summary
  line). Errors after the terminal is restored render to **stderr**
  per the frozen taxonomy — the envelope contract is the CLI's.
- **Confirm parity.** Exactly the CLI's `--yes`-guarded verbs open a
  Confirm modal first (`y` ≡ `--yes`, Esc spawns nothing): restart,
  sessions terminate, loggers set/reset, tags provider/config delete,
  tags import-overwrite, project delete/import-overwrite, resource
  put/delete, rig reset/restore, rig trial reset. Everything else
  fires directly — including `rig down` and `webdev deploy`, which
  the CLI deliberately leaves unguarded.
- **Keybindings.** `q`/Ctrl-C quit (Ctrl-C works even behind a modal;
  `q` never quits behind a modal — it types) · Tab/Shift-Tab cycle
  screens · Enter/Esc navigate (Esc ascends one level on the browser
  screens) · `a` opens the screen's actions menu (grouped and
  prose-labeled — the Projects menu clusters project/resource/webdev
  verbs with each row naming its consequence) · `p` opens the profile
  switcher. Per-screen: `r` refresh (Dashboard/Rig; on Tags it
  refires the deepest-visible pane so a stale error visibly
  reloads), `l` level filter (Logs) / logs pane toggle (Rig), `f`
  follow, `w` watch a tag (Tags), `h` alarm history (Alarms), `t`
  terminate a session (Dashboard). Modals take vim motions: in every
  list-bearing menu `j`/`k` step and `g`/`G` jump to first/last; the
  Result modal scrolls line-wise with `j`/`k` (arrows and PgUp/PgDn
  unchanged) and half-pages with Ctrl-d/Ctrl-u.
- **Coverage is CI-enforced.** A structural test
  (`crates/ignition-cli/tests/tui_coverage.rs`) walks the live clap
  command tree and asserts bidirectional equality with the TUI route
  registry: every CLI leaf (plus the bare-invocable `sessions`,
  `logs`, `logs loggers` forms) has a TUI mapping and no orphan rows
  exist — adding a CLI command without a cockpit surface FAILS CI.
  The only unmapped leaf is `completions` (out-of-band by design).

### Themes & capability degradation (`[ui].theme`)

Four named themes — `default`, `dark`, `light`, `mono` — selected by the
top-level `[ui].theme` config key (set it with any TOML editor; `ign
profile` never needs to be involved). Every theme is AUTHORED at four
capability tiers — truecolor, ANSI-256, ANSI-16, mono — detected at
startup from `NO_COLOR` / `COLORTERM` / `TERM`: the palette steps down
without breaking layout or contrast. `default` and `mono` render
byte-identically at every tier (their non-mono slots are Reset);
`dark` carries WCAG-AAA body contrast (14.2:1 text on black); `light`
keeps black-text structure on either terminal background. Discipline is
CI-enforced: `Color::` literals live ONLY in the style-tokens module
(`ui/theme.rs`) — a grep gate fails any style literal anywhere else.
A wrong-typed or unknown `[ui].theme` value warns and falls back to
default — a config typo never kills TUI startup.

### Polling cadence (`poll_interval_secs`)

Each profile takes an optional `poll_interval_secs` — the Dashboard
refresh period, default 5 s, with ONE source of truth in the code.
Sub-second values are refused at config load (`poll_interval_too_small`,
exit 3 — the hard floor against hammering the gateway). The active value
adopts live across profile switches (same ride-along path as the theme).

## Rigs (Docker compose lifecycle)

`ign rig` manages a **compose rig** — a Docker compose project running
an Ignition gateway plus its satellites. It shells out to
`docker compose` (the v2 plugin line; **compose ≥ v2 required** — the
legacy `docker-compose` v1 binary is not supported and a missing/old
compose fails fast with exit 7 and an install hint). Every operation
starts with a one-shot resolve run (`docker compose -f <file>
--project-directory <dir> config --format json`); the resolved project
`.name` — which honors the rig's own `.env` `COMPOSE_PROJECT_NAME` —
is the identity truth, and every later op passes it as an explicit
`-p <name>` (no implicit directory-name projects, ever).

### Rig discovery (in this order)

| Level | Source | Notes |
|-------|--------|-------|
| 1 | `--rig NAME` | looks up `[rigs.NAME]` in the config; unknown name → exit 7 with the known rigs listed |
| 2 | `IGNITION_RIG` env | same lookup (folded into the flag — one env→flag home) |
| 3 | `[rig].default` | the config's explicit preference — BEATS the cwd scan; a stale default is a loud exit 7 |
| 4 | cwd compose files | `./docker/compose.yml`, `./docker/docker-compose.yml`, `./compose.yml`, `./compose.yaml`, `./docker-compose.yml` |
| 5 | WHK conventions | `ignition-git-module/docker/docker-compose.yml`, then `whk-environment-orchestration/docker-compose.yml` — each probed under BOTH `~/Documents/whiskeyhouse/` and `~/whiskeyhouse/` (first hit wins) |

Config surface:

```toml
[rig]
default = "git-module"

[rigs.git-module]
compose_file = "~/Documents/whiskeyhouse/ignition-git-module/docker/docker-compose.yml"
# project_name optional — omit to honor the rig's own .env
```

`compose_file` expands `~` and `${VAR}`. `IGNITION_RIG_ROOTS`
(path-separated) overrides the convention home roots — for machines
whose WHK checkouts live elsewhere (and for test isolation). Nothing
found → exit 7 with the full search trail in the message.

### The docker-only contract: `profile: null`

Rig verbs are the first commands with **no gateway dependency**: no
profile, secret, or client is resolved, and the envelope echoes
`profile: null` on both success and error. `--profile` has no effect
on `rig`.

### `rig up` semantics

1. compose version gate (fail fast, exit 7 + install hint);
2. port pre-flight — a host port held by ANOTHER project's container
   aborts before anything starts: `port 9088 in use by container X
   (rig Y)` (same-project occupants are recreate-safe; a non-docker
   host process is attributed via `lsof` when available);
3. `up -d --wait --wait-timeout <N> --remove-orphans`;
4. commissioned wait: the rig's own gateway port is derived from the
   resolved mappings (first target 8088 → `http://localhost:<pub>`,
   else target 443 → https) and its `/StatusPing` is polled
   header-less until RUNNING — same budget as `--timeout`.

**Uncommissioned is data, not failure.** A fresh-volume gateway
terminally 302s to `/welcome` — there is no headless commissioning
(verified: no commissioning endpoints exist). `rig up` then exits 0
with `state: "uncommissioned"` and the wizard URL inside `warnings`
(the version-command degradation precedent). A deadline reached while
still STARTING is a real exit-7 failure.

```json
{"ok": true, "profile": null, "data": {"rig": "ignition-devops", "project": "ignition-devops",
 "state": "uncommissioned", "gateway_url": "http://localhost:9088",
 "warnings": ["gateway uncommissioned — open http://localhost:9088/welcome in a browser and complete the commissioning wizard (no headless commissioning exists)"]}}
```

### `rig status` is an allowlist

Status NEVER passes through `docker compose config`/`inspect` output —
the resolved config contains `GATEWAY_ADMIN_PASSWORD` and friends.
The JSON data is exactly `{rig, project, compose_file, services[]:
{name, state, health, exit_code, publishers[]: {published_port,
target_port, protocol}}, volumes: [names], ports_free}` — all keys
always present (empty arrays when none). A down rig is exit-0 data
(`ports_free: true`, empty services).

### `rig reset` semantics (the no-stale-state contract)

`reset` is the phase's destructive verb: it refuses without `--yes`
(exit 2, `confirmation_required`, hint names `--yes` and
`IGNITION_YES=1`) and the guard fires BEFORE any discovery runs — a
refusal costs nothing (no docker, no config scan; binary-pinned by
exiting 2 in a directory with no rig discoverable at all).

The cycle, in order:

1. **preview** — `docker volume ls` label-filtered to the project:
   the `removed_volumes` array in the result data reports exactly
   what reset removes, as it acts;
2. compose version gate (exit 7 + install hint when absent);
3. `down -v --remove-orphans` — the LOCKED teardown: `-v` removes the
   project's named AND anonymous volumes (gateway data, trial state,
   everything), `--remove-orphans` kills renamed-service strays —
   `down && up` without `-v` is the classic stale-state anti-pattern
   reset exists to kill;
4. port pre-flight with FRESH EYES — teardown frees the rig's own
   ports first; if another rig grabbed one mid-cycle, reset aborts
   with attribution and names the torn-down state in the hint;
5. `up -d --wait` (the `rig up` invocation verbatim);
6. commissioned wait — same semantics as `rig up`: a fresh volume
   usually terminally reports the wizard, which is DATA (exit 0,
   `state: "uncommissioned"`, wizard URL in `warnings`); still-STARTING
   at the deadline is a real exit-7 failure.

The result data is `{rig, project, removed_volumes, state, warnings}` —
all keys always. No project, trial, or gateway state survives the
volume deletion; commission from scratch or restore a backup (04-04)
afterward.

### `rig logs` is passthrough

`rig logs` streams `docker compose logs` output RAW: one line per
stdout line, no envelope in ANY mode — compose log lines are not
gateway JSON objects, and wrapping would corrupt them (`rig logs
--json` is the same passthrough; contrast `logs -f --json`, whose
entries ARE gateway NDJSON). `--tail N` (default 200) bounds the
history; `-f` follows until Ctrl-C (default process kill, no
envelope); an optional SERVICE positional filters to one service.
Compose's own stderr diagnostics go to `ign`'s stderr, never the
data stream.

### `rig trial` — state, and the reset ladder

`rig trial status` is **credential-free truth**: both
`GET /data/api/v1/trial` and `GET /data/api/v1/overview/banners`
answer unauthenticated (live-verified on 8.3.3 and 8.3.6, expired AND
active states), so a fresh rig with no provisioned token reports its
trial state fine. The trial endpoint is the PRIMARY source
(`trialState ∈ {AllInDemo, SomeInDemo, NoneInDemo}`,
`trialSecondsLeft` in seconds, the `expired` flag); the trial banner
is the cross-check — `expireTime` is epoch **milliseconds** or
`null`, and an EXPIRED trial shows `severity:"warning"` + `null`
(code expecting a future timestamp misreads expired as active), so
`banners.active` is computed as `severity=="info"` AND
`expireTime>now_ms` and never the reverse derivation.

`rig trial reset` runs the **mechanism ladder** (the spike-resolved
native approach — browser delegation rejected: it needs
Node+chromium, broke across 8.3.3's UI rewrite, and verifies via DOM
text):

1. **tier 0 — token-auth POST** `POST /data/api/v1/trial` with
   `X-Ignition-API-Token` (token from `IGNITION_TOKEN`; token
   mutations need no CSRF). One cheap call; on 2xx it wins.
2. **tier 1 — native gateway login** (the live-verified mechanism,
   end-to-end on 8.3.3: `expired:true → false`, `0 → 7199s`): the
   internal IdP's OIDC challenge dance (rotating tokens, ~4 captured
   cookies replayed by hand, `webui-sid-<id>` session cookie,
   `csrfToken` from `/data/app/session`) → the reset POST with the
   session cookie + `X-CSRF-Token`. Credentials: `--user` (or
   `IGNITION_USER`) + `IGNITION_PASSWORD` — the password NEVER rides
   a flag, env only.

Success REQUIRES the read-back flip: after a 2xx the trial is
re-fetched and `expired` must be false — a bare 2xx never suffices
(the mutation-reads-back discipline). The result data is
`{rig_url, mechanism: "token"|"login", expired_before,
expired_after, trial_remaining_s}`.

**State gate (live-discovered):** the gateway answers 403 to reset
attempts on a NON-expired trial (verified from the browser page with
the exact UI headers) — `ign` surfaces this honestly as exit 6
`trial_not_expired` naming the seconds left, instead of a misleading
auth error. Wait for expiry (watch `rig trial status`) or `rig reset
--yes` for a completely fresh trial volume.

**Verification note:** the reset path is verified end-to-end against
8.3.3 (the git-module rig, by hand during 04-03) and the
login-flow machinery against 8.3.6 (ign-research; steps 1–5 +
bad-credential shapes). Repeatable live gates ship as `#[ignore]`
tests (`cargo test -p ignition-core --test trial_contract --
--ignored` with `IGNITION_LIVE_URL` + credentials +
`IGNITION_LIVE_MUTATIONS=1` against an expired rig). A tier-2
Playwright fallback exists ONLY as this documented env contract —
`ignition-trial-resetter` / WHK-Global's `e2e/reset_trial.mjs` — and
is never shipped as `ign`'s mechanism.

### `rig snapshot` / `rig restore` — repeatable state

`rig snapshot` composes a rig's gateway state HONESTLY into one
directory (default `./ign-rig-snapshots/<rig>-<yyyyMMdd-HHmmss>/`,
`-o` overrides; the stamp is std-only, no clock dependency):

- **`<rig>.gwbk`** — the native roaming backup (`GET
  /data/api/v1/backup?type=roaming`, `Accept:
  application/octet-stream`), STREAMED to disk chunk-by-chunk (never
  buffered in memory — gwbks are tens of MB);
- **`projects/<name>.zip`** — one export per runnable project (the
  `project export` machinery reused; file names percent-encode the
  project name injectively, so `My Project` → `My%20Project.zip`).
  The gwbk *should* already contain projects (postman semantics,
  MEDIUM confidence), but the explicit exports are the honest
  redundancy that makes the manifest truthful either way;
- **`manifest.json`** — the composition record: `{rig, taken_at
  (epoch s), ignition: {version}, gwbk, projects: [{name, file}],
  notes}`. The notes name BOTH exclusions explicitly, verbatim:
  **the trial clock is NOT captured by gwbk** (its restore behavior
  is unknown — reset separately via `rig trial reset`), and
  **tag-provider bulk export is Phase 5 scope** (gwbk captures tag
  *config* via gateway data). No reader can mistake either for a
  silent drop.

`rig restore --file <path.gwbk>` is the guarded inverse
(`--yes`-refusal exit 2 before any discovery, the fifth destructive
verb). The POST is a RAW `application/octet-stream` body — NOT
multipart — with the four scope params (`restoreDisabled`,
`disableTempProjectBackup`, `renameEnabled`, `restoreLocal`) sent
explicitly as `false`. Restore is synchronous AND the gateway
restarts afterward, so BOTH wire directions ride a 300 s per-request
budget and success is a **witnessed** post-restore
StatusPing→RUNNING (the `--timeout` budget floors at 300 s — a short
explicit timeout cannot buy an unknown-state mid-restart report).
A bare 2xx never suffices.

⚠ **API tokens may be clobbered (Pitfall 5).** Tokens stored under
CORE config (`data/config/CORE/ignition/api-token`) are
"modified/cleared often by gwbk restores" (83-api) — after a
restore, stored profiles may 401. The restore data ALWAYS carries
`"API tokens may have been reset by restore — re-provision via
gateway UI, then ign doctor"` as its first warning, in every render
mode. 83-api's recommendation: keep durable tokens in an EXTERNAL
location, not CORE config.

Both verbs address the rig's derived gateway URL and source their
credential from `IGNITION_TOKEN` (the rig-family chain — the backup
route 401s unauthenticated, unlike the trial endpoints; a missing
token is exit 3). The round-trip is pinned live by the opt-in e2e
gate:

```bash
IGNITION_LIVE_URL=http://localhost:9088 \
IGNITION_LIVE_TOKEN='name:key' \
IGNITION_LIVE_MUTATIONS=1 \
cargo test -p ignition-cli --test e2e_rig -- --ignored
```

(the rig must be discoverable — run from the rig's checkout, set
`IGNITION_RIG`, or configure `[rig].default`; **verify with `ign rig
status` first that the intended rig is the one UP** — the gateway
verbs address the derived `localhost:<port>` URL, and a port
collision with another stack would silently point them at the WRONG
gateway). The gate snapshots a
pre-witness project, creates a post-snapshot marker, restores, and
asserts BOTH halves: witness SURVIVED, marker GONE.

### Gateway backups (`ign backup`)

The standalone gwbk surface on ANY profiled gateway — the same wire
`rig snapshot`/`rig restore` ride (Phase 4), without the rig
machinery:

- **`ign backup download [-o FILE] [--type roaming|all]`** — a
  streamed read (never buffered in memory; 300 s per-request
  timeout). `roaming` is the portable backup (cross-gateway — the
  default); `all` includes gateway-specific state. Default filename
  follows the export convention: the gateway's
  `Content-Disposition` basename when it sends one, else
  `<profile>-backup.gwbk` (streamed to a `.part` first, renamed on
  completion — a failed download leaves no half-written impostor).
- **`ign backup restore <FILE> --yes`** — REPLACES this gateway's
  state from the gwbk. Destructive: the 8th `--yes`-guarded verb
  (guard BEFORE resolution — a refusal does zero work). The restore
  POST is synchronous; **the gateway then RESTARTS and blocks for
  minutes** — the 2xx is acceptance, not readiness (`ign wait
  restart` is the readiness witness if you want one; `rig restore`
  bundles that wait for rigs). API tokens inside the backup may
  clobber current ones — see the `rig restore` token warning, same
  mechanics here.
- **There is NO `ign backup list`** — no native listing endpoint
  exists (exactly GET/POST on `/data/api/v1/backup`). Honesty over
  verb theater: your filesystem is the list. The EAM archive store
  (`storage/archived-backups`) is a DIFFERENT thing — it belongs to
  the EAM controller's fleet backups, not this gateway's gwbk files.

The trial clock is NOT captured by a gwbk (see the rig snapshot
exclusions — same mechanics).

## EAM tasks (`ign eam`)

The Enterprise Administration Module's task surface — the read-heavy
family with guarded writes (`eam task new`, `force`, and the
10-04 lifecycle/mutation verbs: `suspend`/`resume`/`cancel`/
`modify`/`delete`).

**The controller state gate.** On a stock gateway the EAM module is
installed but its `module-settings` singleton carries
`installMode: "NotInstalled"` — every RUNTIME endpoint
(`/data/eam/api/v1/*`) answers 403 with "This operation can only be
performed when EAM is configured as a controller." That is a STATE
refusal, not an auth failure: the CLI classifies it to
`eam_not_controller` (exit 6) with the manual-flip hint, never a
misleading `auth_rejected`. **Definitions are different**: they are
plain config resources
(`resources/com.inductiveautomation.eam/eam-tasks`) and answer on
stock gateways — `ign eam tasks` works everywhere; `ign eam history`
needs the controller.

**The manual flip (deliberately NOT a CLI verb — a gateway-ROLE
decision):** config-resource PUT on
`com.inductiveautomation.eam/module-settings` with
`installMode: "Controller"`, array body carrying the current record's
`signature` (find it first: `GET …/resources/find/
com.inductiveautomation.eam/module-settings`). Live-proven during
07-RESEARCH; the CLI surfaces the state and refuses honestly instead
of automating the role change.

**Execution honesty (research Pitfall 3).** Even on a controller,
task EXECUTION needs (1) a Gateway-Network-connected agent target
(even `_controller` self-targets fail with "Gateway network for
agent … not connected" until GNET is configured) and (2) a live
trial/license ("Trial timer is expired" blocks runs). These outcomes
surface as DATA in history rows (`level: Failed` + the gateway's own
`detail` text) — exit 0 reads, never hidden.

**Deferred reads:** the `scheduled`/`retry` list views remain v1
backlog. The `suspend`/`resume`/`cancel` task verbs shipped (the
guarded lifecycle set below); agent-level writes did NOT — see the
reconciliation note. The EAM archive store
(`storage/archived-backups` — a controller's fleet-backup inventory,
needs `serverids`) is a different thing from this gateway's gwbk
files and stays out of MVP scope.

### The guarded task lifecycle verbs (`suspend` / `resume` / `cancel` / `modify` / `delete`)

All five run the **two-tier blast-radius guard**, and `force` composes
onto the same gate:

1. **Tier 0 — pure precheck** (no network): an empty or whitespace
   task name (or a `modify` that targets no keys) refuses exit 2
   `invalid_input` before anything is resolved.
2. **Tier 1 — preview fetch** (read-only): the CLI finds the
   definition and its scheduled state and composes the blast radius —
   task, type, agent targets, pending executions, and the factual
   controller impact. A bad task name refuses exit 6 `not_found`
   HERE, before any prompt: the preview IS the pre-flight.
3. **Tier 2 — confirm-with-preview**: without `--yes` the CLI
   refuses exit 2 `confirmation_required` and **the refusal message
   IS the preview line**
   (`suspend nightly-backup: suspends task nightly-backup … targets:
   [gw-a, _controller] pending: 1`). The refusal names the task, its
   agent targets, and the impact — an agent can read the blast
   radius from stderr alone. Zero mutations have fired.
4. With `--yes` the action runs — and core re-checks the state it
   captured (the authoritative re-read), so a race between preview
   and write still lands honestly (e.g. suspend on an
   already-suspended task refuses, `fired: false` on cancel with
   nothing pending).

**The seam honesty:** `suspend`/`resume`/`cancel` ride the RUNTIME
seam — on a stock gateway they refuse exit 6 `eam_not_controller`
with the manual-flip hint (the preview fetch's definition read still
works everywhere; the refusal comes from the write). `modify` and
`delete` ride the config-resource seam and work on stock gateways.

**The agent-vs-task reconciliation (read this before scripting):**
the EAM wire has **no agent-level suspend/resume**. Suspend, resume,
and cancel are TASK-scoped runtime verbs — an "agent" is suspended
by suspending its tasks, and the blast-radius preview names the
affected agents through `targetGateways` so the task-scoped verb
still answers the fleet question. Genuinely agent-level writes DO
exist on the wire — quarantined-agent delete, agent approve, agent
upgrade — but they are deliberately NOT exposed in v1.1 (deferred
scope; nothing plans them).

### The `eam task new` guard ladder

| type class | examples | guard |
|---|---|---|
| benign | `eam_backup` | none with the default `OnDemand` schedule (it never auto-fires; force owns the dispatch) |
| mutating | `eam_restart`, `eam_sendProject`, `eam_sendResource`, `eam_sendTags`, `eam_activateLicense`, `eam_updateLicense`, `eam_unactivateLicense` | `--yes` — they act on their agent targets when dispatched |
| fleet-destructive | `eam_restoreBackup`, `eam_installModules`, `eam_remoteUpgrade` | REFUSED outright (exit 6 `eam_task_type_refused`) — they push backups/modules/upgrades to every agent; run them from the Ignition EAM console (EXT-03 v2 scope) |

ANY `--schedule-mode` other than `OnDemand` additionally requires
`--yes` (it arms autonomous gateway actions — even `eam_backup`).
An unknown type classifies fail-safe (`--yes`); the server's own
validation is the backstop.

### Settings forms (`--setting` vs `--definition`)

- `--setting K=V` auto-types scalars: a value that parses cleanly
  as `true`/`false` or an integer serializes as a JSON bool/number;
  anything else stays a string. Arrays and objects are OUT of scope
  for K=V.
- `--definition PATH` is the typed/array path: a full-JSON file
  whose top-level object deep-merges over the composed
  `config.settings` (objects merge recursively; arrays and scalars
  REPLACE). Example
  (the live-captured `eam_backup` settings shape):

  ```json
  {
    "targetGateways": ["gw-a", "gw-b"],
    "targetGroups": [],
    "concurrentBackups": 2,
    "forceBackups": true
  }
  ```

  (`--setting` + `--definition` together is a usage error.)

### `eam task force` semantics

`force` = find (owner from `scheduledTaskState.details.owner`,
fallback `eam`) → `POST /eam-tasks/force/{owner}/{name}` (204 =
dispatched) → a history re-read. The history entry rides the result
so the OUTCOME is visible immediately: `level`/`detail` are data —
a `Failed` run with "Gateway network for agent … not connected" or
"Trial timer is expired" is the honest report of the gateway's
execution prerequisites (GNET agent + live trial), never hidden.

A leftover run occupying the task's slot refuses with exit 6
`eam_task_in_flight` — the gateway's own 409 page text ("Task 'X
(forced)' already exists! It must be completed or deleted …") rides
the message verbatim; resolve the run from the EAM console (no
`ign` verb deletes runs).

**`eam_restoreBackup` vs `ign backup restore`** — different axes:
`ign backup restore` restores THIS gateway from gwbk bytes; the EAM
`eam_restoreBackup` type dispatches an ARCHIVED backup to fleet
AGENTS (and is therefore refused in `task new` — run it from the
EAM console).

### Project export/import specifics


**Timeouts.** Long transfers are the classic default-timeout death, so
both operations carry per-request budgets instead of the 30 s client
default: export 120 s, import 300 s. The import is synchronous (the
gateway answers when it finishes — no job IDs). If an import times out
or the connection drops mid-flight, the gateway state is unknown:
**verify with `ign project list`** (and re-run) rather than assuming.

**Scope metadata.** A project export contains views, scripts,
named-queries, and the other module project-resources — it does NOT
contain tag providers, tags, or UDTs: those are gateway configuration,
not project resources (this is why git-module conventions keep a
separate `tags/` tree). Both `project export` and `project import`
carry the same static `scope: {includes, excludes}` arrays in their
JSON data so agents and humans always know what a ZIP does and does
not contain.

**Collision policy.** REST exposes exactly two choices: `abort` (the
default — the CLI pre-checks with `find` and refuses with
`project_exists` before uploading anything) and `overwrite` (replaces
the ENTIRE project — resources absent from the ZIP are deleted).
`merge` is the Designer import popup's mode and is not available via
REST; the CLI rejects it at the flag level by simply not offering it.

### Script decode/encode (`--decode-scripts` / `--encode-scripts`)

`ign project export NAME --decode-scripts [-o DIR]` writes the
export's members to a DIRECTORY (default `<name>-export/`) plus, for
every JSON member with embedded scripts, editable sidecars:

```
MyProj-export/
├── project.json
├── com.inductiveautomation.perspective/resources/views/Dashboard/
│   ├── view.json                  # the member, marker-free
│   ├── view.json.1.py             # decoded + dedented script
│   ├── view.json.2.py
│   └── resource.json
└── scripts-manifest.json          # JSON-pointer addresses → sidecar
```

Edit the `.py` sidecars in nvim/VS Code, lint them with
[ignition-lint](#linting-ign-lint), then re-import the whole tree:
`ign project import NAME --file MyProj-export --encode-scripts`
(the sidecars are spliced back and the manifest stripped before the
standard import path — `--file -` is invalid in this mode). An
edited member changes ONLY at the spliced spans; a sidecar deleted
from the tree keeps the JSON's current value.

**The unedited round-trip is byte-exact.** Decode → encode with no
edits reproduces every member byte-identically (contract-pinned).
This rides the exact "Ignition Flint" escape codec ignition-nvim
uses (backslash-first multi-pass encode, single-pass decode, common
tab-prefix dedent/reindent) with splicing done at raw byte spans —
key order and formatting of the gateway's JSON are never touched.

**Scope honesty.** Only scripts EMBEDDED in JSON resources decode
(Perspective `view.json` component/event scripts, tag event scripts,
…). `ignition/script-python` project resources are ALREADY plain
`.py` members in the export — they land in the tree verbatim and
never grow sidecars. Single-line Ignition expressions (under
`expression` and friends) pass through untouched: a value decodes
only when it carries script-ish escape markers AND decodes to
multiple lines.

### Cross-gateway diff & sync

`ign project diff <PROFILE_A> <PROFILE_B> --project <NAME>` compares
one project's resources across two gateway profiles — the dev→test→
prod promotion workflow's first half. **Direction semantics:** the
output is always **B-relative-to-A** — `added` means the resource
exists only in B, `removed` only in A, `changed` in both with
differing content (after normalization), `same` in both. Human mode
prints grouped ADDED/REMOVED/CHANGED sections plus the
`N same, N added, N removed, N changed` summary; JSON carries
`{scope, profile_a, profile_b, project, project_meta, summary,
entries}` with every key always present.

**Normalization.** Every gateway-written `resource.json` carries
`attributes.lastModification` and `attributes.lastModificationSignature`,
which differ between gateways even for identical content — a byte
compare would report everything CHANGED. The diff strips exactly
those two fields and compares canonicalized (key-sorted) JSON; every
other byte — script bodies, view JSON, descriptors' semantic fields —
compares as-is.

**Scope honesty.** Diff and sync carry `scope: "project"` in their
output metadata: they operate on project resources ONLY. Tag
providers are gateway configuration on a different seam — promote
them with the shipped pipe across profiles instead:

```bash
ign --profile dev tags export [default]P5 -o - | ign --profile prod tags import --file - --provider P5
```

**Two-sided secrets.** Each side resolves its own credential through
the same locked chain (env tokens → keyring → basic env pair), which
means `IGNITION_TOKEN` (and the basic env pair) applies to BOTH sides
unless per-profile keyring entries exist. For real two-gateway use,
store per-profile tokens in the keyring (`ign profile add --keyring`)
so each side authenticates as itself.

**Envelope.** The output envelope keeps its single `profile` field —
the ACTIVE profile, exactly as every other command resolves it —
while `data.profile_a`/`data.profile_b` name both sides explicitly.

**Sync.** `ign project sync <A> <B> --project <NAME> --resource
PATH... [--all-changed] [--delete] --yes` is the second half: the
selected resources from A land in B. The mechanism is the resource
family's surgery generalized to two clients — export both sides,
splice A's member bytes into B's zip (`replace_member`'s put-new
descriptor landing rules ride free), then ONE overwrite-import into
B. Semantics: direction is always explicit A→B; the default is
upsert-only (nothing on B is ever deleted unless `--delete` is
passed); `--all-changed` promotes everything A has that B lacks or
differs on. The `--yes` guard is mandatory — sync implicitly
overwrite-imports the WHOLE project on B (replacing concurrent
Designer edits, the resource-put consequence pattern) — and fires
before either client resolves: a refusal is exit 2 with profile null
and performs zero requests of any kind. An empty effective selection
(`--all-changed` with nothing changed) performs NO import — zero
writes, empty `synced`/`removed` lists.

**e2e witness.** The promotion loop is live-runnable against two
real gateways: `crates/ignition-cli/tests/e2e_projects.rs`
(`project_sync_two_gateways_witness`, `-- --ignored`, needs
`IGNITION_LIVE_URL` + `IGNITION_LIVE_URL_B` +
`IGNITION_LIVE_TOKEN` + `IGNITION_LIVE_MUTATIONS=1`) puts a
differing resource on A, diffs it, syncs it, and re-reads it on B
(the adoption oracle — never trusting the import's success body).
`IGNITION_LIVE_TOKEN` applies to both sides in the harness (the
two-sided-secret caveat above).

### Resource editing — the surgical loop (export-zip surgery)

`ign resource` changes ONE view/script/query member — the
get → edit → put loop:

```bash
ign resource list PlantFloor                       # member paths from the export tree
ign resource get PlantFloor ignition/script-python/e2e/scratch > scratch.json
# ...edit scratch.json...
ign resource put PlantFloor ignition/script-python/e2e/scratch --file scratch.json --yes
ign resource delete PlantFloor com.example/views/OldView/view.json --yes
```

**Mechanism (05-02 re-point).** There are NO per-resource REST
endpoints on 8.3 gateways (the Phase-3 family originally targeted
`/projects/{p}/resources/**` routes that do not exist —
openapi-evidenced against real 8.3.x, 575 paths, zero matches). Every
resource op therefore rides the native export/import round-trip:
export the project to a temp ZIP → perform member surgery (list /
read / replace-inject / remove) → import back with `overwrite=true`.
Paths keep their slashes and address ZIP MEMBERS — the file at
`<collection>/resources/<rest>`, listed as `<collection>/<rest>`
(the core module's folder is `ignition/`, not the
`com.inductiveautomation.ignition` the old docs suggest).

`put` sniffs its input: valid JSON parses, other UTF-8 rides as text
— and BINARY content (a NUL byte near the head, `data.bin`-class
resources like Perspective session-permissions) refuses with exit 6
`resource_binary` on BOTH get and put: binary resources belong to
the export/import family, never the JSON loop.

**Perf honesty (accepted trade).** Every resource op round-trips the
WHOLE project ZIP — heavier than a hypothetical per-resource route,
but rigs and dev projects are small and the alternative was the
family not working at all. **Concurrent-edit race:** because put and
delete re-import the project, edits made in a Designer between the
CLI's export and import are REPLACED (replace-not-merge — resources
absent from the ZIP are deleted). This is the documented, accepted
trade for the CLI's dev-tool scope; both verbs are `--yes`-guarded
and their refusal messages name the consequence.

**e2e witness.** The loop is live-runnable for the first time since
Phase 3: `crates/ignition-cli/tests/e2e_projects.rs` (`-- --ignored`,
needs `IGNITION_LIVE_URL` + `IGNITION_LIVE_TOKEN` +
`IGNITION_LIVE_MUTATIONS=1`) drives list/get/put/delete through the
surgery implementation against a real gateway, including two-sided
put honesty (the export before a put carries the member's old
content; the export after carries the new).

### Workspace — the local edit loop (checkout / status / push)

`ign workspace` is a git-like loop over a project's resources: check
the resources out to a local tree, edit them with any tool, then push
guarded changes back to the gateway.

```bash
ign workspace checkout PlantFloor ./plantfloor --decode-scripts
# ...edit ./plantfloor/... with your editor; commit the tree...
ign workspace status ./plantfloor
ign workspace push ./plantfloor --yes --delete
```

**Scope honesty.** The workspace is PROJECT RESOURCES only — the
source is strictly the project export zip. Tag values NEVER appear in
the tree (tags are `ign tags` verbs, and this is structural: checkout
ingests the export zip and nothing else, so tag data cannot ride in).

**The manifest is the workspace identity.** Checkout records
`.ign-workspace.json` at the tree root: the project, the profile it
resolved against, the checkout time, and every member's
gateway↔local path pair plus a content hash (descriptor-normalized —
gateway `lastModification` volatility never masquerades as drift).
Status and push READ that manifest and never re-derive anything, so
the project is never re-typed. **Commit `.ign-workspace.json`;**
checkout also writes an idempotent `.gitignore` covering
`scripts-manifest.json` and `*.py` — the decode artifacts are
generated, not source.

**Status states are push-relative** — one direction, every row names
what push would do to the GATEWAY:

| State | Meaning | What push does |
|-------|---------|----------------|
| `clean` | both sides match checkout | nothing |
| `local_edit` | changed locally, gateway at baseline | writes the member |
| `gateway_drift` | gateway moved, local at baseline | untouched (re-checkout to refresh) |
| `conflict` | BOTH sides diverged since checkout | refuses — see below |
| `deleted (local)` | member removed locally | removes gateway-side under `--delete`, else reported as skipped |
| `deleted (gateway)` | member removed gateway-side | untouched (re-checkout to refresh) |
| `added` | exported gateway-side since checkout | untouched (re-checkout brings it in) |
| `untracked` | a local file that is not a member | ignored — never imported |

Human mode includes `clean` rows (agents diff the full state) and
ends with the `x local edits, y gateway drift, z conflicts,
n untracked` summary; the JSON's `clean` flag is the same verdict in
one key.

**Push is guarded, spliced, and honest.** The refusal without `--yes`
carries the blast-radius preview AS its message — exactly which
members would be written and deleted. Conflicts refuse EVEN WITH
`--yes`: writing over a concurrent Designer edit is beyond any flag,
so reconcile manually (keep your local change or the gateway's) and
re-checkout. The splice rides the manifest-recorded local bytes into
a FRESH export — never a stale full zip, so members the gateway
deleted since checkout are not resurrected — and imports exactly
once. Deletions are `--delete` opt-in (without it, locally-deleted
members are reported as `skipped`). Untracked files are never
imported. An empty selection performs zero mutations and never
prompts. Over MCP, `confirm` is the field: omitting it returns the
blast-radius preview as the tool result, setting `confirm: true`
executes the push, and neither form can push over a conflict.

**`--decode-scripts` round-trip guarantee.** Embedded JSON scripts
decode to counter-named `<member>.<n>.py` sidecars keyed by
`scripts-manifest.json` (also gitignored); the UNEDITED tree
re-encodes through the codec BYTE-EXACTLY per member — proven at tree
scale by the checkout contract suite. See
[Script decode/encode](#script-decodeencode---decode-scripts----encode-scripts)
for the codec contract.

Editing ONE resource without checking out the whole tree is
[`ign edit`](#edit--the-kubectl-edit-loop-over-one-resource-ign-edit)'s
job — the workspace is for whole-tree authoring.

### Edit — the kubectl-edit loop over one resource (`ign edit`)

`ign edit <PROJECT> <RESOURCE_PATH>` is the single-resource
counterpart of the workspace loop: fetch the project export → decode
the whole tree into a private (0700) temp directory → open the ONE
resource in `$EDITOR` → on save, re-encode, gate, and push the
spliced result back with exactly one project import.

```bash
EDITOR="code --wait" ign edit PlantFloor ignition/script-python/e2e/scratch
```

**The `$EDITOR` contract.** `VISUAL` wins over `EDITOR` (trimmed; an
empty value counts as unset); both unset refuses exit 2 BEFORE
anything resolves — the envelope's `profile` is null and zero
requests hit the gateway. The variable may carry flags:
`EDITOR="code --wait"` splits into an argument vector with the file
path appended LAST (never a shell string) — IDE users need the wait
flag INSIDE `EDITOR` so the CLI observes the saved file, not just the
opened one. The editor's exit code is ADVISORY: whether an edit
happened is decided by CONTENT (a byte-compare of the re-encode
against the untouched re-encode), never by the exit code — editors
that daemonize or fork cannot flip a verdict.

**Behavior table.**

| What you did | What happens |
|--------------|--------------|
| saved unchanged | clean no-op — `edit: no changes — nothing pushed`, exit 0; nothing is pushed, nothing is prompted |
| saved a change | the blast-radius summary renders (exactly which members the push would write); without `--yes` that same summary IS the refusal (exit 2 `confirmation_required`); with `--yes` the push rides one overwrite import |
| the gateway moved since the fetch | refuses exit 2 `invalid_input` — `changed on gateway since fetch` — **EVEN WITH `--yes`** (forcing would clobber a concurrent Designer edit); re-run to fetch fresh and retry |
| saved something the codec cannot encode | refuses FAIL-CLOSED exit 2 with the codec's own reason, and the temp edit tree is KEPT — its path is printed (`the edit tree is preserved at …`); fix the member by hand and re-run to retry |

**Stdout contract.** `ign edit` writes ZERO bytes to stdout in EVERY
mode — the child `$EDITOR` owns the terminal, so all prose (the
no-op line, the blast-radius summary, the pushed line, and every
refusal) renders to stderr. The contract is byte-scan pinned at the
binary level under maximum diagnostics. Success prose is stable and
greppable: `edit: no changes — nothing pushed` and
`edit: pushed N member(s) to <PROJECT>`.

### Streaming output (the stdout exceptions)

`ign logs -f` is a STREAM: entries print to stdout as they arrive, so
there is no single result to wrap. In human mode that means live lines
(`ISO-UTC  LEVEL  logger  message`, profile header first). Under
`--json`/`--compact` the tail emits **NDJSON — one compact entry object
per line, no envelope** — the second sanctioned stdout exception (after
`completions`). Every other command still emits exactly one envelope.
A `--timeout` expiry ends the tail cleanly with exit 0; without it the
tail runs until Ctrl-C, which uses the process default kill (no
envelope — plan for it in pipelines).

`ign rig logs` is the THIRD exception: its output is raw compose log
lines, NOT gateway JSON — so it streams verbatim in EVERY mode
(`--json` changes nothing; there is no envelope to add and no NDJSON
transformation to attempt). `-f` follows until Ctrl-C (default process
kill, no envelope), exactly the `logs -f` pipeline caveat.

`ign tags export -o -` is the FOURTH exception: the export payload IS
the product, so it prints raw in EVERY mode (no envelope even under
`--json`/`--compact` — the rig-logs precedent). This is what makes the
pipe round-trip work:
`ign tags export [default]P5 -o - | ign tags import --file - --provider other`.
File-mode exports (the default, `-o FILE`) keep the normal envelope.
Since Phase 11 the exception covers every format: `--format xml` and
`--format csv` stdout mode write the RAW BYTES — the gateway's XML
document (CRLF line endings, no declaration, trailing CRLF) or the
generated CSV, byte-for-byte, never re-encoded, never pretty-printed,
no added trailing newline. In human mode the artifact summary line
(`exported … → stdout …`) goes to STDERR so a pipe stays pure; json
mode keeps stdout data-only. The loss-gate refusal prose (below) is
stderr-only for the same reason.

`ign edit` is the inverted FIFTH exception: it writes NOTHING to
stdout in ANY mode — the child `$EDITOR` owns the terminal and all
prose renders to stderr — so it never participates in the envelope
system at all (see the
[edit section](#edit--the-kubectl-edit-loop-over-one-resource-ign-edit)).

### Destructive operations

Commands that change gateway state (`sessions terminate`,
`logs loggers set`/`reset`, `project delete`, `project import
--collision-policy overwrite`, `project sync` (the cross-gateway
promotion — overwrite-imports the whole project on its TARGET
profile), `resource put`, `resource delete`
(both re-import the whole project — their refusal messages name the
consequence), `workspace push` (splices local edits into a fresh
export of the workspace's project — its refusal message IS the
blast-radius preview, and its conflict refusal fires even with
`--yes`), `edit` (the single-resource loop — its guarded push
re-imports the project and its refusal message IS the staged diff
summary; its staleness and fail-closed refusals fire regardless of
any flag), `tags provider delete`, `tags config delete`, and
`tags import --collision-policy overwrite` (abort-policy imports need
no `--yes` — the pre-check fails safely before any write),
`restart` — the
big one: it takes
the whole gateway down for ~1 min — and `rig reset`, which deletes
the rig's volumes, plus `rig trial reset`, which restarts the trial
window, and `rig restore`, which REPLACES the gateway's state with a
gwbk, and `backup restore` — `rig restore`'s standalone sibling on
any profiled gateway, same whole-state replacement — plus the EAM
writes: `eam task new` for MUTATING types or any non-OnDemand
schedule (the typed ladder above; `eam_backup` + OnDemand needs no
`--yes` and the fleet-destructive trio refuses outright instead of
guarding), and `eam task force`, which dispatches NOW, plus the guarded task
lifecycle verbs `eam task suspend`/`resume`/`cancel`/`modify`/
`delete` — each refusal carries the blast-radius preview in the
message (the two-tier gate above)) refuse
without `--yes`
(exit 2, `confirmation_required`, hint names both the flag and
`IGNITION_YES=1`) — non-interactive by design, so scripts and agents
pass `--yes` once and humans get a speed bump. `restart` is guarded in
BOTH forms: plain and `--wait`. The guard fires before any network
activity: a refusal never touches the gateway. The rig guards
(`rig reset`, `rig trial reset`, `rig restore`) fire before even rig
DISCOVERY (a refusal does zero work of any
kind). `project delete` and
`project import --collision-policy overwrite` are the doubly-relevant
pair: besides the CLI refusal, delete's wire request always carries
the gateway's own `confirm=true` query param, and overwrite REPLACES
the entire project (abort-policy imports need no `--yes` — they fail
safely server-side). `resource put` joined this family in 05-02: its
member surgery implicitly overwrite-imports the project (the 03-03
unguarded put is superseded), as does `resource delete`. Termination,
restart, project mutations, and resource
writes are
audit-logged server-side by the gateway. Non-destructive project
mutations (`copy`, `rename`, `set`, `export`) create, relabel, or read
rather than destroy, so they carry no `--yes`. `tags alarms ack`
likewise carries no guard — acknowledging is state-advancing but
never destructive (it cannot un-acknowledge anything), the
read-adjacent verb family's line.

## The CLI's WebDev routes (`ign webdev`)

Every tag operation the CLI performs on a gateway rides the CLI's own
WebDev route bundle — small Jython `doPost.py` routes deployed into a
DEDICATED project (`ign-cli` by default) under
`/system/webdev/ign-cli/cli/{route}` (the wire protocol — NOT
`/data/webdev/*`, which does not exist). The bundle is embedded in the
binary at build time and carries a `ROUTE_BUNDLE_VERSION` handshake
(`{"action":"version"}` → `{routeVersion, minCli}`) on every route.

### Deploy semantics

`ign webdev deploy` packs the embedded bundle into a project zip and
imports it with `overwrite=true` — a **clean replace**, never a merge.
The dedicated project is CLI-OWNED: it is born from the first deploy
zip and wholesale-replaced by every later deploy, which is why deploy
carries **no `--yes` guard** (user projects are never touched; you
cannot accidentally deploy over anything you own — use `--project`
only if you deliberately want a different name). There is no
pre-flight project create (the gateway's first-import quirk); the
import is the deployment.

### The version-negotiation refusal matrix

WebDev-dependent commands (the tag family, from Phase 5 on) probe the
canonical `tags` route's handshake BEFORE doing anything, and refuse
exit 6 with an actionable error — no auto-upgrade magic, ever:

| Probe answer          | Slug                   | Hint                                                |
|-----------------------|------------------------|-----------------------------------------------------|
| 405 (route/project absent) | `routes_not_deployed` | run `ign webdev deploy`                             |
| deployed < expected   | `route_version_mismatch` | run `ign webdev deploy` (old routes on the gateway) |
| deployed > expected   | `route_version_mismatch` | update `ign` (the binary is older than the routes)  |
| 402                   | `webdev_unlicensed`   | license the gateway (trial-expired rigs cannot serve `/system/webdev`) |
| 200 body denial, unmapped code | `webdev_route_error` | the route's own `code` + `message` verbatim |

Two wire facts the matrix is built on (live-proven on 8.3): **denials
ride HTTP 200** — WebDev ignores a `status` key in route returns, so
every refusal is detectable only from the body envelope
`{ok, data|error}`; and **405 = absent** (missing routes and missing
projects both answer 405 — `ign doctor`'s earlier 404 assumption was
wrong and has been re-pinned).

**The route-authoring contract (live-bisected 05-06):** every
`doPost.py` must begin with `def doPost(...)` at BYTE 0. Any
module-level content before it — comments, constants, even a blank
line — makes the route silently unloadable on the real WebDev engine
(the servlet answers 200 with an empty body; nothing is logged). The
route sources keep their header docs and `ROUTE_VERSION`/`MIN_CLI`
constants NESTED inside `doPost` for exactly this reason. A second,
related engine quirk: the `exec(...)` CALL form can fail to compile
at deep nesting on this Jython build — use the statement form
(`exec code in globals`) in route scripts.

`ign webdev status` itself is a READ: it exits 0 whenever the sweep
completes and reports per-route degradation as data (the doctor
precedent) — the refusal matrix above belongs to the tag commands
that DEPEND on the routes, not to the sweep that inspects them.

### The configure-shape traps (authoring tag definitions)

`tags config create|edit` definitions ride `system.tag.configure`
verbatim — the CLI never reshapes them. Four traps reproduced live
during Phase 5 research (each one silently breaks or misleadingly
errors on a real 8.3 gateway — author definitions against this
table):

| Trap | Wrong | Right |
|------|-------|-------|
| Discriminator key | `"type": "AtomicTag"` | `"tagType": "AtomicTag"` (`type` is silently ignored) |
| Nesting | slash-names (`"name": "P5/T1"`) — rejected | children NEST under a `tags` array on Folder/UdtType entries |
| Alarms | a name-keyed dict — silently ignored (returns Good, no alarm attached) | alarms are a **LIST** of dicts: `"alarms": [{"name": "HighLimit", "mode": "AboveValue", "setpointA": 100, "priority": "High"}]` |
| Base path | a provider name as configure's first arg | a basePath (`[default]P5`) + per-tag `name`s — the CLI derives both from the PATH you pass, so this trap cannot bite via the CLI |

A minimal memory tag definition: `{"tagType": "AtomicTag",
"dataType": "Int4", "value": 123}`.

### Tag↔historian bindings — the surfaced field set (TAGS-13)

When a tag's config carries history-binding keys, `tags config get`
human mode appends an additive `history:` block after the pretty
JSON — one line per present field. The key names are CAPTURE-LOCKED
to the 13-01 live spike record (both trial rigs, read-back + data
proof; provenance: `.planning/phases/13-composite-engine-workspace-historian-edit/13-LIVE-CAPTURES.md`
§Field-set table). No name below is guessed:

| Config key | Type | `history:` line | Meaning |
|------------|------|-----------------|---------|
| `historyEnabled` | Boolean | `enabled: true` | The binding switch |
| `historyProvider` | String | `provider: <name>` | The storage provider's name (the doc-corrected key — `historicalProvider` is a documented typo that does NOT bind) |
| `sampleMode` | Enum | `sample mode: TagGroup` | The capture enum VERBATIM (`"TagGroup"` binds via the gateway's DEFAULT historical group — no group key is needed and none is guessed) |
| — (no captured key) | — | `historical group: …` | Rendered only if a key is ever captured; the spike bound without one |

Additive-only contract: a config WITHOUT history keys renders
byte-identically to the pre-TAGS-13 output (pinned by test), and the
compact/JSON envelope is unchanged (the full `config` passthrough
already carries the keys there).

**The binding recipe (TAGS-14, live-proven on both trial rigs):**
binding a tag to a storage provider is a plain tag-config write —
the CLI's existing `tags config create|edit` passthrough needs NO
new surface. Provision an InternalHistorian first (native REST, no
database: `POST /data/api/v1/resources/com.inductiveautomation.historian/historian-provider`
with the array body `[{name, type: "com.inductiveautomation.historian/historian-provider",
collection: "core", enabled: true, config: {profile: {type:
"InternalHistorian"}, settings: {}}}]`), then send the COMPLETE node
shape plus the three history keys in ONE write:

```json
{"tagType": "AtomicTag", "dataType": "Int4", "value": 0,
 "historyEnabled": true, "historyProvider": "my-historian",
 "sampleMode": "TagGroup"}
```

Two wire truths make the "complete node shape" mandatory (both from
the 13-01 captures): (1) `tags config` writes REPLACE the whole node
— an edit that sends only the history keys silently drops
`dataType`/`defaultValue` and nulls `value`; (2) NO historical-group
key is needed — at `sampleMode: "TagGroup"` the gateway binds its
DEFAULT historical group. Follow-through: write tag values, then
`ign tags history query` — the tag-group scan stores at 40–50 s
cadence, so allow a couple of scans before expecting the written
values as DATA rows (live-proven on 8.3.6 AND 8.3.3; the e2e gate
`live_tags_history_bindings` re-proves the full chain). Provider
cleanup rides the generic resources API: find
`GET /data/api/v1/resources/find/{module}/{type}/{name}` → signature
→ `DELETE /data/api/v1/resources/{module}/{type}/{name}/{signature}`.

### Bulk export/import (the portability loop)

`tags export` → `tags import` moves a tag subtree between providers
(or gateways) with values intact. `--format json` (both sides'
default) is the gateway's own JSON interchange, parsed and written
pretty — the lossless round-trip. `--format xml` passes the gateway's
own XML document through RAW-BYTE both directions (export: the
gateway's `exportTags format=xml` answer decoded verbatim; import:
your bytes ride base64 to the gateway's `importTags` untouched — the
gateway's parser is the only semantic actor). The collision
conventions are IDENTICAL to project import's (locked in Phase 3):
abort pre-checks and refuses before any write; overwrite is
`--yes`-guarded with no pre-check. `export -o -` pipes into
`import --file -` in any format.

#### The CSV contract (read this before `--format csv`)

The gateway cannot export tags to CSV (official Ignition 8.3 docs:
"Ignition does not export tags to a CSV format").
`ign tags export --format csv` output is **CLI-generated** from the
gateway's JSON interchange and is **LOSSY**:

- **No alarm configurations** — the legacy CSV grammar cannot carry
  them; alarm data in the source is dropped (live-proven: even a CSV
  imported with the undocumented `AlarmStates` column lands nothing).
- **The legacy column set only** — modern properties (tag groups,
  bindings, UDT parameter overrides, scan-class-adjacent and
  history-retention columns) have no legacy column and are dropped.
- **Numeric enum coercion** — `TagType`/`DataType`/`ExpressionType`
  ride legacy numeric codes; folders flatten (the legacy `Path`
  column is unusable — any non-empty value crashes the gateway's
  importer wholesale, so every row carries an empty Path).

The command prints exactly what was dropped on stderr
(`warning: csv export dropped field(s): …`) and carries the
structured twin in `data.loss_report` (`{format, dropped_keys,
coerced, rows}`). Generation is **warn-and-continue, never gated**
(nothing destructive happens — no gate applies); the column-by-column
coverage table (what lands, coerces, drops, or crashes) is documented
in `.planning/phases/11-tag-bulk-transfer-xml-csv/11-LIVE-CAPTURES.md`
§Probe 5, live-proven on 8.3.3 and 8.3.6.

#### The loss gate (imports, TAGS-12)

When importing xml/csv, the CLI SCANS the input before anything is
sent and reports what the import would drop or coerce — UDT type
definition transfer caveats (definitions DO import on this command's
provider-root path, routing to `[provider]_types_/Name`; but exported
`udtParentType` stays provider-qualified, so cross-provider imports
keep pointing at the source provider, and instance parameter overrides
silently drop when the referenced type is unresolvable in the target
provider on 8.3.3), gateway-export-shaped files carrying only edited
properties, and (for CSV, unconditionally) the no-alarms/legacy-columns
facts. If the scan reports losses, the CLI
prints the loss report as prose on STDERR and **refuses exit 2
`invalid_input` unless `--yes`** — the refusal happens before any
network activity (profile null, zero requests). With `--yes` the
import proceeds and the structured scan summary rides the success
envelope as `data.loss_report` (`{format, facts, top_level_names}`),
so agents can read the same findings without parsing prose. The scan
is advisory — the gateway remains the parsing authority; a clean
scan never blocks an import, and a gateway-side parse error still
surfaces as the gateway's own error.

#### Collision policy (xml/csv)

The gateway's `importTags` also accepts an `Ignore` collision policy
(`'i'`) that silently keeps existing tags. `ign` deliberately
surfaces only `abort` and `overwrite` (the same two-value matrix as
`tags import`'s JSON path since 03-02): abort is the safe default
with a real pre-check, overwrite is honest about being destructive
and requires `--yes` — a silent-ignore policy has no honest CLI
story, so it is not a value.

Live-proven payload shapes (05-06): the gateway's `exportTags` never
answers a bare array — one path yields a SINGLE subtree object,
several yield the `{"tags": [...]}` wrapper. `tags export`
NORMALIZES both to the list-of-subtrees interchange (what the export
file carries and `tags import` consumes). A provider-shaped export
subtree (empty `name`) lands its CHILDREN at the import target —
the abort pre-check and the `imported` count therefore key on the
EFFECTIVE top-level names, and the structural `_types_` folder
(present on every provider; the server's own abort policy accepts
configuring it) never counts as a collision.

**Provider-ROOT paths are not supported on 8.3 WebDev threads** —
`system.tag.getConfiguration`/`exportTags` need an RPC context the
WebDev route's threads don't carry (live-proven 8.3.3
b2026012009 on both devops-rig gateways; `ign tags export [default]`
and `ign tags export default` both hit it). The deployed route
refuses honestly — exit 6 `provider_root_unsupported` with the
subtree workaround in the message and hint — where the bracket form
is detected before the call and the bare form (a leading
provider-matching segment) is translated from the gateway's own
`No RpcContext` throw. Subtree paths (`[default]folder`) are the
supported form and behave exactly as documented.

### Browsing tag exports offline (`--from-export`)

`ign tags browse --from-export <PATH>` reads a tag export WITHOUT a
gateway — no profile resolution, no credential, no deployed routes
(the envelope carries `profile: null`; the positional browse path
and the flag are mutually exclusive). Three layouts are accepted:

1. **The CLI's own export** — the JSON file `ign tags export -o`
   writes (the list-of-subtrees interchange). The provider is the
   file stem (`default.json` → `[default]…` rows).
2. **A legacy whole-tree file** — a `<provider>.json` carrying the
   entire provider (the pre-individual-file git-module format).
   Provider = file stem.
3. **A git-module directory** — the layout
   [ignition-git-module](https://github.com/TheThoughtagen/ignition-git-module)
   checks in: a `tags/` root (or the directory itself when it
   directly holds provider folders/`.json` files), one folder or
   legacy `<provider>.json` per provider. In the individual-file
   format, folders are DIRECTORIES, each leaf tag is one `.json`
   (the tag's `name` field is stripped — the filename is the name,
   `%XX`-decoded for reserved characters), and `_types_/*.json` at
   the provider root are the UDT definitions. Dot-entries skip (the
   module's own rule), `.tag-config.json` is config (not a provider),
   and the `System` provider is always excluded.

The rows ride the SAME renderer and flat JSON shape as the live
browse (`{path, name, tag_type, has_children, data_type}` with
bracketed fullPaths) — `--filter` applies client-side. Offline
errors (missing path, unparseable JSON) exit 2 `invalid_input`
before any network. The natural pipe: browse an export from one
gateway's git module, then promote it with
`ign tags import --provider <prov> --file <interchange>` (or the
README's cross-profile pipe).

### Alarms and tag history

The alarms + tagHistory routes close the tag surface (TAGS-07/08).
**The alarm lifecycle is live-proven end-to-end** (configure a
LIST-form alarm → write past the setpoint → `alarms active` shows
`Active, Unacknowledged` → `alarms ack` → the state flips to
`Active, Acknowledged`):

- `alarms active` filters ride `system.alarm.queryStatus` kwargs
  verbatim; quality/state strings are data, never parsed.
- **Alarm history needs a JOURNAL — and default rigs have none.**
  `alarms history` on an unprovisioned rig refuses exit 6
  (`alarm_journal_missing`) naming the missing chain — the honest,
  actionable default-rig path (live-proven). The provisioning chain
  is: (1) a running database reachable from the gateway (e.g. a
  sidecar postgres container), (2) a `ignition/database-connection`
  resource pointing at it, (3) an `ignition/alarm-journal` profile
  referencing that connection, and (4) the
  `ignition/general-alarm-settings` singleton pointed at the journal
  profile. All four steps are native config-resource REST (the same
  resource family as tag providers) — provisionable headlessly once
  a database exists; the e2e gate leaves this as a documented
  stretch (the wiremock pins carry the capability proof).
- `alarms ack` is the gateway-scope 3-arg form (`String[] ids, note,
  username`) — the explicit `--username` is required, and the
  unacknowledged REMAINDER comes back as data (acknowledged =
  requested − remainder, computed client-side honestly).

**Tag history (TAGS-08):** `tags history query` works STRUCTURALLY
on any rig (a well-formed `t_stamp`-keyed dataset, null values
without a historian — exit 0). DATA requires a provisioned
historian: an **InternalHistorian needs no database** — creatable
via native REST
(`POST /data/api/v1/resources/com.inductiveautomation.historian/historian-provider`,
profile type `InternalHistorian`; the e2e gate provisions one
live). **The tag↔historian binding is CLOSED (13-01 spike, both
rigs):** the Phase-5 "documented limitation" was a key-name typo —
`historicalProvider` never bound; the captured field set
`historyEnabled` + `historyProvider` + `sampleMode: "TagGroup"`
(discussed in **Tag↔historian bindings** above) landed, read back,
and returned WRITTEN values from history on 8.3.6 AND 8.3.3. The
e2e gate `live_tags_history_bindings` re-proves the full chain on
licensed trial rigs.

### Phase 5 requirement map

| Requirement | Shipped as |
|-------------|------------|
| WEB-01 (route bundle + deploy) | `ign webdev deploy` (+ the embedded 13-member bundle) |
| WEB-02 (status sweep) | `ign webdev status` |
| TAGS-01 (provider CRUD) | `ign tags provider list/create/delete` |
| TAGS-02 (browse) | `ign tags browse` |
| TAGS-03 (read) | `ign tags read` |
| TAGS-04 (write) | `ign tags write` |
| TAGS-05 (config CRUD) | `ign tags config get/create/edit/delete` |
| TAGS-06 (UDTs) | `ign tags udt types/def` |
| TAGS-07 (alarms) | `ign tags alarms active/history/ack` |
| TAGS-08 (tag history) | `ign tags history query` |
| TAGS-09 (bulk transfer) | `ign tags export/import` |

Parity with the 21-tool MCP tag-domain surface is complete
(intent-corrected, not bug-replicated — the prior-art defects the
research found are corrected in the routes).

### scriptExec — the LOCKED security posture

`scriptExec` (arbitrary Jython execution through the gateway) ships
in the bundle only as a TEMPLATE with a `__IGN_CLI_SECRET__` marker
and deploys ONLY with an explicit `--with-script-exec`:

- the deploy generates a fresh 32-byte hex secret from
  `/dev/urandom`, substitutes it into the template (the placeholder
  can never ship — it is excluded from the plain manifest), and
  persists it in the profile config at 0600; `--rotate-secret`
  regenerates (any route copy deployed with the old secret starts
  refusing);
- the route fail-closes on every action — version included — unless
  the request carries the matching `X-Ignition-CLI-Secret` header:
  no header → `secret_required`, wrong secret → `secret_mismatch`
  (constant-time compare);
- the secret appears in exactly ONE place: the baked zip member on
  the gateway. Never in command output, JSON envelopes, or logs.

Threat-model honesty: the secret gate is a SHARED-SECRET posture, not
real authentication — anyone who can read the gateway project's
resources (e.g. a Designer user or a project export) can read the
secret. It exists to make `scriptExec` an opt-in, auditable surface
with a definite off switch (never deploy it, or redeploy without the
flag), not to protect against gateway insiders. The route keeps its
config at require-auth=false deliberately: an auth layer would lock
the CLI's own token-authenticated calls out (API tokens 401 on WebDev
require-auth — live-verified).

#### The `ign script run` verb contract

`ign script run` (07-03) is the route's entire consumer surface, and
its security posture is the deploy flag — nothing more:

- **Input forms**: `--code PY` (inline — a one-liner's best form),
  `--file PATH`, `--file -` (stdin — the agent pipe path). Giving
  both `--code` and `--file`, or neither, refuses `invalid_input`
  (exit 2) before any resolution work.
- **The opt-in is structural**: a profile with no persisted
  `webdev_secret` can only mean the route was never deployed through
  the flag — the verb refuses exit 6 `script_exec_not_configured`
  with zero HTTP requests, its hint naming
  `ign webdev deploy --with-script-exec` verbatim. There is
  deliberately NO `--yes` guard here: the deploy flag is the opt-in,
  and requiring a second confirmation would only tax the
  non-interactive agents the route exists for.
- **Result shape**: `{stdout, result, elapsedMs}` — all keys always
  (empty string / null / 0 when the answer carried nothing; agents
  never key-hunt). `stdout` is the script's captured output verbatim;
  `result` is a final expression's value or the `_result` global's
  statements; `elapsedMs` is the route-measured wall time.
- **Timeout honesty**: v1.0.0's route has NO server-side execution
  timeout — a long-running script simply holds the HTTP connection
  while it runs (the client rides its existing per-request timeout
  class). Budget long scripts accordingly.
- **Redaction guarantee**: the secret appears in exactly one place
  (the baked zip member) — never in the JSON envelope, the human
  render, logs, or any error path (the refusal and result canaries
  are contract-pinned at both the action and binary levels).
- **Threat-model cross-reference**: the shared-secret honesty note
  above applies to every `script run` invocation — the gate makes
  the surface opt-in and auditable; it does not defend against
  gateway insiders who can read the project's resources.

## Linting (`ign lint`)

`ign lint PATH... [--strict] [-- <extra ignition-lint args>]` delegates
to [ignition-lint](https://github.com/TheThoughtagen/ignition-lint) —
the external linter for Ignition project resources (Perspective view
structure, naming conventions, embedded scripts). The verb is LOCAL:
no gateway, no profile (the envelope carries `profile: null`), and the
tool is discovered on `PATH` (first executable `ignition-lint` wins).

**The doctor posture (default).** The command exits **0 whenever the
tool RAN** — findings are DATA, never a crash:

```json
{"ok":true,"profile":null,"data":{"ran":true,"tool":"/path/ignition-lint",
 "child_exit_code":1,"issues_found":2,
 "report":{"issues":[...],"summary":{"errors":1,"warnings":1}},
 "stdout":"<the tool's raw stdout>","stderr_preview":"<first 4000 chars>"}}
```

All keys always ride (`report` is `null` when stdout was not the
tool's JSON; `stdout` stays verbatim either way). The child's exit
code travels as `child_exit_code` so agents can branch without
parsing.

**`--strict` for CI.** With `--strict` the CLI exits with the
linter's own code LITERALLY (exit 1 = findings at/above the tool's
`--fail-on` threshold, default `error`) — the envelope still prints
first, then the exit mirrors the child (masked to the 0–127 shell
range; a signal-killed tool exits 1). This is the one sanctioned
success-path exit exception.

**No tool installed?** Exit 6 `lint_tool_absent` with the install
hint: `uv tool install ignition-lint-toolkit` (or
`pip install ignition-lint-toolkit`).

**Argument mapping.** Every PATH rides as `--target <path>` on the
tool's arg vector alongside `--report-format json` (the data
contract); anything after `--` passes through verbatim for the
tool's own flags (`--profile perspective`, `--checks naming`,
`--fail-on warning`, …). Spawning is an arg VECTOR — never a shell
string. Pair it with `project export --decode-scripts`: lint the
decoded `.py` sidecars, then re-import with `--encode-scripts`.

## Browser E2E (`ign e2e`)

Two verbs. `ign e2e doctor` diagnoses; `ign e2e init` scaffolds. There is
deliberately **no `ign e2e run`** — the scaffold's own `npm test` is the
runner, and wrapping it would put this CLI in the business of proxying
Playwright's reporter, flags, and exit codes.

```sh
ign e2e doctor                      # what's missing?
ign e2e init ./e2e --yes            # scaffold + npm install
cd e2e && IGNITION_PASSWORD=… npm test
```

**What the scaffold ships.** Seven members, embedded in the binary:
`package.json` (one dev dependency — `@playwright/test`),
`playwright.config.mjs`, `global-setup.mjs`, `lib/gateway.mjs`,
`tests/example.spec.mjs`, `.gitignore`, and a `README.md` documenting
every environment variable it reads.

**What global setup does.** Two steps, both delegating to this CLI so the
scaffold never reimplements the gateway's login dance or its test
protocol:

1. **`ign session login --json`** → writes `data.storage_state` to
   `.auth/state.json`, which `playwright.config.mjs` points
   `use.storageState` at. Every browser context therefore starts already
   authenticated, and no test ever drives the login form.
2. **`ign testing run --project NAME --json`** → runs the gateway's own
   Jython suite FIRST and throws if the verdict is not `passed`, aborting
   before a single page opens. A browser failure caused by broken
   gateway-side code is an expensive way to learn something one HTTP call
   already knew. Skip it with `IGN_E2E_SKIP_GATEWAY_TESTS=1`.

Both steps surface the CLI's own stderr and name its exit code: **3** a
missing password, **5** rejected credentials, **6** a red suite.

**The example suite is green on a fresh rig.** It ships two tests: a tag
round trip through the testing routes (passes on any gateway with the
bundle deployed), and a Perspective shell check that `test.skip()`s —
naming `--run-project` — when the navigation answers 404. A gateway with
no views of your own must not produce a red run.

**TLS.** `lib/gateway.mjs` disables certificate verification **only**
when the gateway host is `localhost`, `127.0.0.1`, or `::1`, and prints
one warning to stderr when it does. A self-signed cert on your own
machine is a papercut; the same relaxation pointed at a remote gateway
silently removes the guarantee HTTPS exists to provide. The identical
guard is duplicated in `playwright.config.mjs` (for `ignoreHTTPSErrors`)
rather than imported — a cross-import between the runner config and a
test helper is a load-order hazard — and a Rust test renders both bodies
and asserts they name the same hosts and the same opt-in variable, so
the two copies cannot drift. For a remote gateway behind a private CA,
do **not** set `IGNITION_ALLOW_INSECURE_LOCAL`; point Node at the CA
with `NODE_EXTRA_CA_CERTS=/path/to/ca.pem` instead.

**`.auth/state.json` holds a live session cookie.** The scaffold's own
`.gitignore` lists the `.auth/` directory, and its README names the file
and what it contains.

## Agent transports — MCP (`ign mcp serve`) and LSP (`ign lsp`)

v1.1 exposes the whole command surface over protocols. Both modes own
stdout completely — the streaming exceptions above apply doubly, since
inside a protocol stream a single stray byte is protocol death. Both are
thin lenses: the command surface they mirror is the CLI's own, so a new
CLI verb becomes an MCP tool or an LSP capability through derivation,
never a second hand-written catalog.

### `ign mcp serve` — the MCP transport

Register with any stdio MCP client:

```
claude mcp add ign -- ign mcp serve    # Claude Code
# generic client: command `ign`, args ["mcp", "serve"]
```

- **The tool catalog derives from `Cli::command()`** — every CLI verb
  surfaces as a tool automatically (83 at v1.1) with global args folded
  into each schema (`--json`/`--compact` forced on, `--yes` replaced by
  the `confirm` field, profile ambient). A CI parity test walks the live
  clap tree, so the catalog structurally cannot drift from the CLI.
- **The frozen envelope rides verbatim** as tool-result content — the
  two documented prose exceptions (`ign api call`'s gateway-verbatim
  data, MCP's own refusal prose below) are dated above and in the
  Contract exceptions section.
- **Guarded writes translate `--yes` to a `confirm` tool-call field.**
  This applies to every `--yes`-guarded verb, including `workspace push`
  (`confirm: true` is the MCP-native confirmation for the splice).
  `confirm` is deliberately OPTIONAL in every guarded schema — a
  required field would invite agents to auto-fill it, defeating the
  human-in-the-loop gate. Omitting it returns the same refusal envelope
  (blast radius and all) as the tool result; unknown keys fail closed
  with `-32602`. Deliberately unguarded verbs (`rig down` and the
  others) carry no `confirm` property at all on either surface — that
  asymmetry is by design, not a gap.
- **`ping` never starves**: tool calls are served concurrently while one
  blocks on the gateway. `protocolVersion` negotiates `2025-06-18`
  (echo-if-equal).
- **Raw-stdout verbs refuse at the bridge**: `ign rig logs` and `ign
  logs --follow` print through in-dispatch sinks, so the MCP bridge
  refuses them before they can corrupt the stream — the refusal names
  the terminal form.

### `ign lsp` — live gateway truth for editors

A stdio LSP server (sync dispatch loop) serving three completion
families (tag paths, named queries, providers), TTL-stamped hover, and
diagnostics from a TTL-cached gateway snapshot:

- **Never a blocking network call inside an LSP request** — handlers
  are pure functions over one snapshot Arc; the request path cannot
  hang the editor.
- **A dead gateway degrades honestly**: diagnostics publish EMPTY (no
  cached oracle, no verdict) instead of spamming errors; the background
  refresher re-resolves per cycle so recovery appears next cycle.
- **Composition, not replacement**: Python `ignition-lsp` keeps statics
  ownership (definition, codeAction, workspace symbols); nvim's native
  multi-client merge composes the two servers. The nvim/zed clients
  live in [ignition-ide-plugins](https://github.com/TheThoughtagen/ignition-ide-plugins)
  (`ignition_live` registers `ign lsp` beside the statics client when
  `ign` is on PATH; virtual buffers re-attach across restarts for both
  clients).

### Agent skills — the playbook layer

The [`ignition-ops`](claude-code-plugin/README.md) skills teach agents
the contract rules this README codifies (envelope streams, exit
taxonomy, guarded-write semantics, loss gates) without reading 1,600
lines first: `ign-contract`, `ign-gateway`, `ign-tags`,
`ign-workspace`, `ign-rigs`. Install with `npx skills add
TheThoughtagen/ignition-cli` (any harness the skills CLI supports) or
as a Claude Code plugin: `/plugin marketplace add
TheThoughtagen/ignition-cli` → `/plugin install
ignition-ops@ignition-cli`.

