# Phase 22: Transport, Plan, Pull & Verify - Context

**Gathered:** 2026-10-14
**Status:** Ready for planning

<domain>
## Phase Boundary

Both transport halves (system OpenSSH `SftpTransport` over `RemoteShell`, and `LocalFsTransport`) plus the read-only verbs `ign deploy plan`, `ign deploy pull` and master `ign deploy verify`, with the verdict envelope documented (DPLN-01..09, DSURF-04). The write half (mirror driven by an explicit delete set) is built and tested against a scratch directory; no verb calls it until Phase 23. Captures C6, C7a and OQ5 are recorded here; Decision B and the measured hold budget are the exit gate before Phase 23 is planned.

Already locked by ROADMAP.md / REQUIREMENTS.md and not re-decided here: Decision A (verdict envelope `ok:true`, `data.ok=false`, slug in `data`, exit 6), Decision C (`--plan-sha256`), Decision D (`render`, `plan`, `pull`, `verify` are MCP tools), SF-2 (`plan` always exits 0, no `--detailed-exitcode`), SF-4 (`--base` is the three-way drift input; the apply-side drift gate is Phase 23), SF-5 (no core-strip command), system `ssh`/`sftp` only (no SSH crate), the one layered seam `RemoteShell` under `FileTransport`, `robocopy /MIR` rejected, nothing dot-named and no staging or snapshots inside `data/config/resources/`.

</domain>

<decisions>
## Implementation Decisions

### Host key and transport selection
- **D-01:** The pinned host key comes from a repo-committed known_hosts file. Add `ssh.known_hosts` (path, relative to the repo root) to the target schema. ign passes `-o UserKnownHostsFile=<that>` with `StrictHostKeyChecking=yes`, and never consults the runner's `~/.ssh/known_hosts`. The pin is reviewed in git, and CI needs no runner state. — **Reversibility:** costly — target schema field plus every committed target file.
- **D-02:** A node selects `LocalFsTransport` through a new `local: { data_dir }` block, mutually exclusive with `ssh`. A node with neither block is refused as `deploy_tree_invalid` when a verb needs host access. Strict parsing catches typos.
- **D-03:** `key_env` stays. ign writes the key to a 0600 temp file inside a 0700 temp dir, passes `-i <file>` with `IdentitiesOnly=yes`, and deletes it on drop. On Windows runners the file gets an owner-only ACL. The key value never appears in argv, logs, errors or output. It falls under the secret canary.
- **D-04:** ssh/sftp timeouts are configurable now through a target `ssh` field (connect and per-operation), with defaults applied when absent. The defaults are revisited once OQ5 numbers exist. — **Reversibility:** costly — schema field shape is committed in target files.

### Delete-set source
- **D-05:** "Previously declared names" come from an on-box declared-names ledger. Phase 23 apply writes it on the gateway host, outside `data/config/resources/` and next to the journal. Phase 22 defines the ledger format and the reader, and `plan` reads it over the transport. An absent ledger means a first deploy, and the delete set is empty. The ledger's exact location is decided together with Decision B (C6). — **Reversibility:** one-way — on-box file format becomes a contract read by every later plan/apply/rollback across runners.
- **D-06:** A live resource of an allowlisted type that is not rendered and not in the delete set gets a fifth plan status, `unmanaged`. It is never deleted and never counts toward `data.summary.has_changes`. This extends DPLN-02's four statuses, so add a REQUIREMENTS.md note on DPLN-02. — **Reversibility:** one-way — `--json` status vocabulary is the agentic contract.
- **D-07:** A ledger that exists but is unreadable, corrupt, or names a different gateway or mode is a pre-write refusal (`CoreError` envelope) naming the ledger path and the problem. ign never falls back to an empty delete set.
- **D-08:** `plan_sha256` is the hash of the canonical plan: `render_sha256`, each entry's status with its live and rendered file hashes, and the delete set. Any live change after review changes the hash, so Phase 23's `--plan-sha256` refuses it as `plan_stale`.

### Pull behavior
- **D-09:** A resource carrying an Embedded secret is skipped. Pull writes no file for it and lists it in `data.skipped[]` with its path and the reason, never the value. `plan` then reports it as `unmanaged`, so "render then plan is empty" still holds. The user migrates it to a referenced secret by hand.
- **D-10:** Types: if `manifest.yaml` exists, pull its allowlist. Otherwise generate a manifest from the live types minus the deny list (security singletons, security levels, API tokens, user sources).
- **D-11:** An existing non-empty `gateways/<name>/` is overwritten only with `--yes`. Without `--yes` the overwrite is refused before any write, and the refusal shows what would be replaced. This makes `deploy pull` a guarded op: add a `GUARDED_OPS` entry and a `// guarded:` marker in `main.rs`. It stays in the MCP catalog through the synthetic `confirm` (Decision D). Pull never writes to the gateway.
- **D-12:** Pull reads the target's mode collection into the `modes/<env>/` overlay and `core` into the base `gateways/<name>/` tree, mirroring render's layering, so render reproduces the live effective view. Bootstrapped resources keep the gateway's uuid (research conflict resolution 5).

### Live captures and host
- **D-13:** WFLWVM01 is not yet provisioned (Ignition 8.3.6, OpenSSH Server, `svc-ignition-deploy`). The phase includes a human setup checkpoint for it. The agent then runs C6, C7a and OQ5 over SSH as `svc-ignition-deploy` and records raw output in `22-LIVE-CAPTURES.md`.
- **D-14:** Gating: C7a blocks the `SftpTransport`/`RemoteShell` Windows code. `LocalFsTransport`, the write half against a scratch dir, `plan`, `pull`, `verify` and surface work go ahead against the 8.3.6 rig in parallel. C6 (Decision B) and OQ5 (hold budget: p95, floored, ceilinged) gate only the phase exit before Phase 23 planning.
- **D-15:** C7a is captured on WFLWVM01 only (wh-win was offered and declined). SC-1's "proven against a real Windows OpenSSH host" closes only on WFLWVM01.
- **D-16:** The OQ5 payload is a tree produced by Phase 22 `pull` from a real prod-like gateway (or the 183-file corpus scaled up if none is reachable), so timings reflect real file counts and sizes.

### Claude's Discretion
- The `plan --diff` human format, the verify log-scrape window (since the scan timestamp), the tag-probe Good/timeout semantics within SF-8, the internal `ObservedState` shape, and whether plan reads files through the transport, the config API, or both for each check.
- The canonical serialization feeding `plan_sha256`, as long as D-08 holds.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone scope and decisions
- `.planning/ROADMAP.md` — v1.6 header (Decisions A–D, capture discipline) and "Phase 22: Transport, Plan, Pull & Verify" (SC-1..5, Capture first, Exit gate, Constraints)
- `.planning/REQUIREMENTS.md` — DPLN-01..09, DSURF-04 (plus the note to add for D-06)
- `.planning/research/SUMMARY.md` — Conflict Resolutions 2 (transport seam, write flow, bounded delete set), 6 (envelope vs verdict), 7 (slug table); Critical Pitfalls; Spec Flags SF-1..SF-10; capture backlog C6, C7a, OQ5; Decision B
- `.planning/research/ARCHITECTURE.md` — §4.4 transport/staging, §7 slugs and verdicts, module layout
- `.planning/research/STACK.md` — §1 system OpenSSH hardening (BatchMode, stdin handling, EncodedCommand)
- `.planning/research/PITFALLS.md` — 4/5 (delete set, config-mode.json), 12 (plan redaction), 13/15/19 (paths, transport hardening), 18 (staging), 25 (plan hash)
- Design spec: whk-environment-orchestration PR #60, `docs/superpowers/specs/2026-10-09-prod-gateway-config-as-code-design.md` (§3, §4, §5–§6, §9 service-account scope, §13) — external repo, not checked out here

### Prior phase artifacts
- `.planning/phases/20-config-api-client/20-LIVE-CAPTURES.md` — scan, scan-lock, mode and effective-view wire facts (D7)
- `.planning/phases/21-deploy-render/21-CONTEXT.md` — D-01..D-04 (D-04 defers NTFS hostile-name behavior to Phase 22)
- `.planning/phases/21-deploy-render/21-LIVE-CAPTURES.md` — CD1–CD7, tag mapping, uuid rule
- `.planning/phases/21-deploy-render/21-SECURITY.md` — WR-03/R-03: `--out` refresh is not crash-safe, so Phase 22 re-hashes instead of trusting `render.json`

### Contract docs
- `README.md` exit-code table and `crates/ignition-core/src/error.rs` — pinned by `readme_exit_table_agreement` and `exit_code_mapping_enumerated`; DSURF-04 adds the dated contract-exception entry

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/ignition-core/src/client/deploy_api.rs` — `DeployApi` trait (resource_names, resource_list effective view, resource_find with optional collection), with the stateful fake. Used for verify read-back and core/mode classification.
- `crates/ignition-core/src/client/{scan,mode}.rs` — scan-complete predicate and `normalize_deployment_mode` for verify.
- `crates/ignition-core/src/client/logs.rs` — `LogQuery`/`LogEntry` for the "will not be loaded" scrape.
- `crates/ignition-core/src/actions/tags.rs` — tag read for `verify.tag_probes[]` quality checks.
- `crates/ignition-core/src/deploy/{render,tree,layout,manifest,target}.rs` — `render()`, `RenderedTree`, `sha256_hex`, `RenderMarker`, `write_out` (the clobber-ladder precedent for pull's write), `Manifest.verify.tag_probes`, `Target.nodes.*.ssh`.
- `crates/ignition-core/src/rig/compose.rs` — `ComposeRunner` trait plus `DockerCompose`, the precedent for a mockable `ProcessRunner` and argv-only subprocesses.

### Established Patterns
- Actions never print. Progress goes to stderr through a sink. The JSON envelope is the only stdout.
- Verdict post-write shape set in `main.rs` after the envelope (the `ign testing run` precedent).
- New verbs land with TUI rows (`crates/ignition-tui/src/routes.rs` → `Screen::Projects`, `deploy_rows_cover_the_family`), README entries, and exit-table rows in the same commit series.
- `GUARDED_OPS` in `crates/ignition-cli/src/main.rs:3807`, plus `// guarded:` markers, with a drift test walking both directions.
- Error slugs follow the four-place rule (error.rs, README table, exit mapping test, docs).

### Integration Points
- `crates/ignition-cli/src/cli.rs` `DeployCommand` (Render only today) gains Plan, Pull, Verify.
- `crates/ignition-core/src/actions/deploy.rs` gains the thin verb functions.
- New `crates/ignition-core/src/deploy/transport/` (RemoteShell, FileTransport, SftpTransport, LocalFsTransport), `plan.rs`, `ledger.rs` reader.

</code_context>

<specifics>
## Specific Ideas

- Known_hosts lives in the repo (e.g. `targets/known_hosts`) and is referenced per node.
- `unmanaged` is visible in plan output so the user sees what the pipeline leaves alone.
- Pull is a bootstrap, and git is the comparison tool for a re-pull. `--yes` overwrite exists for refreshes.

</specifics>

<deferred>
## Deferred Ideas

- Using wh-win as a stand-in Win32-OpenSSH host for C7a was declined. Revisit only if WFLWVM01 provisioning stalls the phase.
- Ledger cross-check against `--base` (warn when they disagree) was not selected. It could be added later.

</deferred>

---

*Phase: 22-transport-plan-pull-verify*
*Context gathered: 2026-10-14*
