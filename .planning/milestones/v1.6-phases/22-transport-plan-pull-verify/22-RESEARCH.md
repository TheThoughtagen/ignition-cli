# Phase 22: Transport, Plan, Pull & Verify - Research

**Researched:** 2026-10-14
**Domain:** Rust CLI: system-OpenSSH file transport to a Win32-OpenSSH host, read-only deploy diff (`plan`), bootstrap (`pull`) and read-back (`verify`) of a declared Ignition 8.3 config tree
**Confidence:** MEDIUM overall. HIGH on in-repo seams and OpenSSH argv (tested locally). MEDIUM-LOW on Windows host behavior (all INFERRED until C7a/C6/OQ5 are captured on WFLWVM01, which is not provisioned).

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

Already locked by ROADMAP.md / REQUIREMENTS.md and not re-decided here: Decision A (verdict envelope `ok:true`, `data.ok=false`, slug in `data`, exit 6), Decision C (`--plan-sha256`), Decision D (`render`, `plan`, `pull`, `verify` are MCP tools), SF-2 (`plan` always exits 0, no `--detailed-exitcode`), SF-4 (`--base` is the three-way drift input; the apply-side drift gate is Phase 23), SF-5 (no core-strip command), system `ssh`/`sftp` only (no SSH crate), the one layered seam `RemoteShell` under `FileTransport`, `robocopy /MIR` rejected, nothing dot-named and no staging or snapshots inside `data/config/resources/`.

**Host key and transport selection**
- **D-01:** The pinned host key comes from a repo-committed known_hosts file. Add `ssh.known_hosts` (path, relative to the repo root) to the target schema. ign passes `-o UserKnownHostsFile=<that>` with `StrictHostKeyChecking=yes`, and never consults the runner's `~/.ssh/known_hosts`. The pin is reviewed in git, and CI needs no runner state. — **Reversibility:** costly — target schema field plus every committed target file.
- **D-02:** A node selects `LocalFsTransport` through a new `local: { data_dir }` block, mutually exclusive with `ssh`. A node with neither block is refused as `deploy_tree_invalid` when a verb needs host access. Strict parsing catches typos.
- **D-03:** `key_env` stays. ign writes the key to a 0600 temp file inside a 0700 temp dir, passes `-i <file>` with `IdentitiesOnly=yes`, and deletes it on drop. On Windows runners the file gets an owner-only ACL. The key value never appears in argv, logs, errors or output. It falls under the secret canary.
- **D-04:** ssh/sftp timeouts are configurable now through a target `ssh` field (connect and per-operation), with defaults applied when absent. The defaults are revisited once OQ5 numbers exist. — **Reversibility:** costly — schema field shape is committed in target files.

**Delete-set source**
- **D-05:** "Previously declared names" come from an on-box declared-names ledger. Phase 23 apply writes it on the gateway host, outside `data/config/resources/` and next to the journal. Phase 22 defines the ledger format and the reader, and `plan` reads it over the transport. An absent ledger means a first deploy, and the delete set is empty. The ledger's exact location is decided together with Decision B (C6). — **Reversibility:** one-way — on-box file format becomes a contract read by every later plan/apply/rollback across runners.
- **D-06:** A live resource of an allowlisted type that is not rendered and not in the delete set gets a fifth plan status, `unmanaged`. It is never deleted and never counts toward `data.summary.has_changes`. This extends DPLN-02's four statuses, so add a REQUIREMENTS.md note on DPLN-02. — **Reversibility:** one-way — `--json` status vocabulary is the agentic contract.
- **D-07:** A ledger that exists but is unreadable, corrupt, or names a different gateway or mode is a pre-write refusal (`CoreError` envelope) naming the ledger path and the problem. ign never falls back to an empty delete set.
- **D-08:** `plan_sha256` is the hash of the canonical plan: `render_sha256`, each entry's status with its live and rendered file hashes, and the delete set. Any live change after review changes the hash, so Phase 23's `--plan-sha256` refuses it as `plan_stale`.

**Pull behavior**
- **D-09:** A resource carrying an Embedded secret is skipped. Pull writes no file for it and lists it in `data.skipped[]` with its path and the reason, never the value. `plan` then reports it as `unmanaged`, so "render then plan is empty" still holds. The user migrates it to a referenced secret by hand.
- **D-10:** Types: if `manifest.yaml` exists, pull its allowlist. Otherwise generate a manifest from the live types minus the deny list (security singletons, security levels, API tokens, user sources).
- **D-11:** An existing non-empty `gateways/<name>/` is overwritten only with `--yes`. Without `--yes` the overwrite is refused before any write, and the refusal shows what would be replaced. This makes `deploy pull` a guarded op: add a `GUARDED_OPS` entry and a `// guarded:` marker in `main.rs`. It stays in the MCP catalog through the synthetic `confirm` (Decision D). Pull never writes to the gateway.
- **D-12:** Pull reads the target's mode collection into the `modes/<env>/` overlay and `core` into the base `gateways/<name>/` tree, mirroring render's layering, so render reproduces the live effective view. Bootstrapped resources keep the gateway's uuid (research conflict resolution 5).

**Live captures and host**
- **D-13:** WFLWVM01 is not yet provisioned (Ignition 8.3.6, OpenSSH Server, `svc-ignition-deploy`). The phase includes a human setup checkpoint for it. The agent then runs C6, C7a and OQ5 over SSH as `svc-ignition-deploy` and records raw output in `22-LIVE-CAPTURES.md`.
- **D-14:** Gating: C7a blocks the `SftpTransport`/`RemoteShell` Windows code. `LocalFsTransport`, the write half against a scratch dir, `plan`, `pull`, `verify` and surface work go ahead against the 8.3.6 rig in parallel. C6 (Decision B) and OQ5 (hold budget: p95, floored, ceilinged) gate only the phase exit before Phase 23 planning.
- **D-15:** C7a is captured on WFLWVM01 only (wh-win was offered and declined). SC-1's "proven against a real Windows OpenSSH host" closes only on WFLWVM01.
- **D-16:** The OQ5 payload is a tree produced by Phase 22 `pull` from a real prod-like gateway (or the 183-file corpus scaled up if none is reachable), so timings reflect real file counts and sizes.

### Claude's Discretion
- The `plan --diff` human format, the verify log-scrape window (since the scan timestamp), the tag-probe Good/timeout semantics within SF-8, the internal `ObservedState` shape, and whether plan reads files through the transport, the config API, or both for each check.
- The canonical serialization feeding `plan_sha256`, as long as D-08 holds.

### Deferred Ideas (OUT OF SCOPE)
- Using wh-win as a stand-in Win32-OpenSSH host for C7a was declined. Revisit only if WFLWVM01 provisioning stalls the phase.
- Ledger cross-check against `--base` (warn when they disagree) was not selected. It could be added later.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| DPLN-01 | Read and write a gateway config tree over system OpenSSH (and local FS) with batch mode, pinned host key, per-call timeouts, argv-only, client-side hashes | "Hardened ssh/sftp argv" (verified with `ssh -G`), `ProcessRunner`/`RemoteShell`/`TreeReader`+`TreeMirror` split, existence-first sftp read protocol, `sftp -D` real-protocol tests, C7a capture list |
| DPLN-02 | `plan`: added/changed/removed/same, zero writes, zero locks, exit 0, `has_changes`, `plan_sha256` | Plan engine (units, statuses, `unmanaged`), canonical hash format, `ForbidWrites` test wrapper, zero-lock proof |
| DPLN-03 | `core` relation (none, identical, differs, hidden, shadows_removal) plus destructive and lockout flags | Core-relation table, destructive/lockout tables (type names verified against `OPENAPI_TYPES`), Open Question 1 on the live basis |
| DPLN-04 | Exact delete set = previously declared names minus rendered names; never `config-mode.json` or undeclared | Ledger format + strict reader, `GATEWAY_OWNED_FILES` filter, allowlist filter, D-07 refusal |
| DPLN-05 | `--base` marks drift | Three-way rule `live != base && live != desired`; re-hash base directory, never trust `render.json` (WR-03) |
| DPLN-06 | `pull` bootstraps `gateways/<name>/`, `render` then `plan` empty | Pull = inverse mapping + render fixpoint; round-trip invariant test |
| DPLN-07 | `verify` master: scan complete, mode equals target, read-back equal, no "will not be loaded", | Verify check table, verbatim RIG.md log line, `resource_list`/`resource_singleton` read-back, log window rule |
| DPLN-08 | Tag probes Good; absent list = warning | Tag probes ride `tags_read` (WebDev `tags` route, default project `ign-cli`); Good semantics; warning rule |
| DPLN-09 | Red verify = frozen envelope, findings in `data`, exit 6 | `failure_exit_code()` hook precedent (`TestingRunResult`), slug in payload only |
| DSURF-04 | Verdict envelope documented as a dated README contract exception | README section outline + a test that pins the section and its date |
</phase_requirements>

## Project Constraints (from CLAUDE.md)

Extracted from `/Users/pmannion/whiskeyhouse/ignition-cli/.claude/CLAUDE.md` and the user's global `CLAUDE.md`. These carry the authority of locked decisions.

- **Lean dependency tree; single binary; no daemon.** Phase 22 adds at most one crate (`similar`, already in `Cargo.lock`). No SSH crate (also a locked roadmap constraint).
- **Every subcommand is non-interactive by default with `--json`.** `plan`, `pull`, `verify` must never prompt; `pull` overwrite is `--yes`-gated.
- **Exit codes: the source of truth is `README.md` + `crates/ignition-core/src/error.rs`, pinned by `readme_exit_table_agreement` and `exit_code_mapping_enumerated`.** Do not restate codes from memory; read those before specifying any exit code. New slugs follow the four-place rule (variant+`code()`/`exit_code()`/`hint()`, error.rs header table, README table, `EXIT_SLUG_LITERALS` in the enumerated test; deploy slugs also join `CoreError::DEPLOY_SLUGS`).
- **No git worktrees under `.claude/worktrees/`** (shared global `CARGO_TARGET_DIR` corrupts builds). Run sequentially on the main checkout. If a worktree is unavoidable, export a unique `CARGO_TARGET_DIR`. The shell `rm` is wrapped and rejects `-rf`; use `/bin/rm`.
- **`ign` on PATH is the installed binary.** Run `cargo install --path crates/ignition-cli --force` before any manual smoke check of a new verb.
- **Edit/Write only through a GSD workflow** (`/gsd-execute-phase` for planned work).
- **No fake completion:** no `test.skip`/`#[ignore]`-as-evidence/stub tests as proof. Live tests that need a rig or WFLWVM01 are `#[ignore]` by project convention (see `live_deploy_api.rs`) but are never the only evidence for a success criterion that can be proven offline.
- **TDD for new logic** (superpowers:test-driven-development) and verification before completion apply per task.

## Summary

Phase 22 builds the first code that reads a live gateway's config *files*. Everything else in the pipeline (REST client, renderer, envelope, surface tests) already exists and is green (86 `deploy::` unit tests pass in 0.26 s; verified this session). The new work is four things: (1) a layered transport (`ProcessRunner` -> `RemoteShell` -> `TreeReader` + `TreeMirror`, with `SftpTransport` and `LocalFsTransport`), whose write half is built but unreachable from any verb; (2) `plan`, which compares the rendered tree to the live tree and computes statuses, core relations, flags, the delete set (from an on-box ledger) and `plan_sha256`; (3) `pull`, the inverse of `render`, made safe by re-running `render` on its own output until it is clean; (4) master `verify`, a read-only check list whose red result is a success-shaped envelope with exit 6.

The research found one design fork the planner must not decide silently (Open Question 1): D-12 splits `core` into the base tree and the mode collection into the overlay, so `render` yields the *effective view*, yet the delete set and `apply` act on the *mode collection*. If `plan` compares only against the mode collection, "render then plan is empty" (SC-3) fails on any gateway whose allowlisted config still lives in `core` (spec section 13 step 2 requires exactly that to be empty). The recommendation below compares against the effective view (mode over core) and adds an additive `live_in` field so Phase 23 can still see what is not yet materialised in the mode collection. This is `[ASSUMED]` and needs user confirmation.

OpenSSH behavior was tested locally (OpenSSH 10.3p1, `sftp -D /usr/libexec/sftp-server`): a batch aborts on the first failing command (exit 1) unless the command carries a `-` prefix, which hides real errors; `ls -1` prints full paths; sftp interprets glob characters in paths; and OpenSSH rejects a private key with CRLF or no trailing newline. Each drives a concrete rule below. The Windows-side behaviors (path form `/C:/`, CLIXML on stderr, key ACLs, long paths) stay INFERRED until C7a, and WFLWVM01 is not provisioned, so the plan needs a human-action checkpoint and the Windows-specific code lands in a late wave.

**Primary recommendation:** Build bottom-up behind two narrow traits (`TreeReader` for all verbs, `TreeMirror` for nothing yet, enforced by a source-grep test), keep every sftp batch free of remote-derived strings, compute plan and verify from the *effective view*, and gate the late Windows-only code and the Decision B / hold-budget exit on one human checkpoint for WFLWVM01.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Flag parsing, dispatch, envelope, exit decision | CLI binary (`ignition-cli`) | — | Actions never print; `main.rs` decides exit (verdict hook) |
| Plan / pull / verify logic | Core actions + `deploy/` modules (`ignition-core`) | — | Same code for CLI, MCP and TUI rows; pure where possible |
| Render of the declared tree | Core `deploy::render` (exists) | — | plan/pull/verify call it; never reimplemented |
| Process spawn, timeouts, kill | Core `transport::process` | — | One place; mockable like `ComposeRunner` |
| ssh/sftp argv, host-key pin, key temp file | Core `transport::remote` | — | Security-critical, argv-only |
| Reading/writing the gateway's files | Core `transport::{sftp,local}` | Gateway host filesystem | Tags have no REST route; bytes must be hashed client-side |
| Effective-view read-back, mode, scan status, logs, tag quality | Gateway REST (existing `DeployApi`, `GatewayApi`, WebDev `tags` route) | — | The gateway's own answer is the only proof a resource *loaded* |
| Declared-names ledger (on-box) | Gateway host filesystem (written by Phase 23) | Core `ledger.rs` reader | Phase 22 defines format + reader only |
| Declared tree, targets, known_hosts | Repo (git) | — | Reviewed state; `known_hosts` pin lives here (D-01) |
| TUI rows, README, exit table, MCP catalog | Surface tests (`deploy_family_coverage`, `tui_coverage`, mcp tests) | — | Land in the same commit series as each verb (DSURF-01) |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| system `ssh` / `sftp` via `tokio::process` | OpenSSH 10.3p1 (dev Mac); Win32-OpenSSH on hosts | Transport | Locked: no SSH crate; honors `~/.ssh/config`, agents [VERIFIED: `ssh -V` this session] |
| `tokio` | 1.53 (workspace; `process`, `time`, `io-util` already enabled) | Spawn, `timeout`, `kill_on_drop` | Already the runtime [VERIFIED: Cargo.toml workspace deps] |
| `sha2` | 0.10 (workspace) | Client-side file hashes, `plan_sha256` | Already used by `tree.rs::sha256_hex` |
| `base64` | 0.22 (workspace) | `-EncodedCommand` UTF-16LE payload (Wave B) | Already a core dep |
| `tempfile` | 3.27 (core dep) | 0700 temp dir for key + known_hosts copy + sftp download dir | Already promoted to `[dependencies]` |
| `serde` / `serde_json` / `yaml_serde` | workspace | Ledger JSON, plan data, manifest | Existing strict-parse helpers (`deploy::yamlio`) |
| `async-trait` | 0.1 | Dyn-compatible transport traits | Matches `DeployApi`, `ComposeRunner` |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `similar` | 3.2.0 (already locked transitively via `snapbox`; promote to `ignition-core` `[dependencies]`, default features) | `plan --diff` unified text diff | Only for `--diff`; do not hand-roll a differ |
| `wiremock` | 0.6 (dev) | CLI contract tests for gateway endpoints | `contract_deploy`-style binary tests |
| `assert_cmd` / `tempfile` | dev | Binary-level tests | Existing pattern in `contract_deploy.rs` |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| System sftp read half | PowerShell remote listing/hashing for reads | Rejected: per-OS dialect, and "never trust a remote tool's hash" (ARCHITECTURE 4.4). sftp is OS-neutral. |
| `russh` / `ssh2` / `openssh` crate | — | Rejected by roadmap (MSRV 1.89, +124 crates, Unix-only respectively) |
| `walkdir` | `std::fs::read_dir` with sorted entries | Existing `walk_out`/`Reader` already do hardened, sorted, symlink-refusing walks; reuse (promote `walk_out` to `pub(crate)`) |

**Installation:** `similar = "3"` in `crates/ignition-core/Cargo.toml` (it is already in `Cargo.lock` at 3.2.0, so zero new lock entries) [VERIFIED: Cargo.lock:3014-3016, `cargo tree --workspace -i similar`, `cargo info similar` rust-version 1.85 <= workspace 1.88].

**Version verification:** `cargo info similar` returned 3.2.0, license Apache-2.0, rust-version 1.85, features `default = [std, text]`. No other crate is added.

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| similar | crates.io | published 2021-01-24 (about 5.7 yrs) | about 4.5M/week | github.com/mitsuhiko/similar | [OK] | Approved; already in `Cargo.lock` as a dev-tree transitive |

`gsd-tools query package-legitimacy check --ecosystem crates similar` returned `OK` with no reasons. No `postinstall` concept applies to crates. No other package is introduced (hand-rolled YAML emission for the generated manifest, `icacls` via the process runner, no ACL crate).

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```
 ign deploy plan|pull|verify --env E [--repo DIR]            (CLI / MCP / TUI row)
          |
          v
  load repo: targets/E.yaml -> Target ; gateways/<g>/manifest.yaml -> Manifest
          |                       |
          |                       +--> node = nodes.master
          |                              | ssh{host,known_hosts,key_*,timeouts} xor local{data_dir}
          v                              v
   render(...)  [existing, pure]   Session::resolve_side(config, master.profile)  -> DeployApi + GatewayApi
   RenderedTree + render_sha256            (REST: mode, scan status, lock info, list/singleton, logs, tags)
          |                              |
          |        +---------------------+------------------------+
          |        |  TreeReader (read-only trait; the ONLY thing verbs receive)
          |        |    SftpTransport --RemoteShell--ProcessRunner--> ssh/sftp (argv only, BatchMode,
          |        |                                                  pinned known_hosts, kill_on_drop, timeout)
          |        |    LocalFsTransport  --> <data_dir>/config/resources/<coll>/...
          |        +--> ObservedFiles { core: files, mode: files }  (scoped to allowlisted types + tag providers)
          |        +--> ledger.json (on-box, outside resources/)  -> PreviousDeclared | absent = first deploy
          v
   PLAN:  units (resource dirs) x {rendered, mode copy, core copy, ledger}
          -> status {added,changed,removed,same,unmanaged}, core relation, destructive/lockout, delete set
          -> [--base DIR re-hash -> drift]  -> [--diff redacted] -> plan_sha256 -> envelope (exit 0 always)
   PULL:  ObservedFiles -> inverse path map -> SourceTree -> render fixpoint (drop units that fail/ leak)
          -> clobber ladder write of gateways/<g>/{resources,tags,modes/<env>,manifest.yaml}   (never touches the gateway)
   VERIFY: scan idle? mode == target? read-back (resource_list/singleton) == render? log scrape since
          last scan? tag probes Good?  -> envelope ok:true ; data.ok=false => exit 6 set in main.rs after render

  TreeMirror (write half: stage + commit with explicit DeleteSet) -- built + tested against a scratch dir,
  imported by NO verb (source-grep test).  Phase 23 is its first caller.
```

### Recommended Project Structure
```
crates/ignition-core/src/deploy/
├── transport/
│   ├── mod.rs      # TreeReader, TreeMirror, RemoteRoot, Scope, ObservedFiles, DeleteSet, MirrorReport, safe_remote_path()
│   ├── process.rs  # ProcessRunner trait + TokioProcessRunner (stdin piped+closed, kill_on_drop, timeout)
│   ├── remote.rs   # RemoteShell: ssh/sftp argv, hardening options, key + known_hosts temp dir, sentinel parser
│   ├── sftp.rs     # SftpTransport (read half now; stage/commit after C7a)
│   ├── local.rs    # LocalFsTransport (read + write halves)
│   └── winps.rs    # Windows PowerShell script text + EncodedCommand builder  (Wave B, after C7a)
├── ledger.rs       # format v1 + strict reader (+ a writer used by Phase 23 and by tests)
├── plan.rs         # ObservedState, units, statuses, core relation, flags, delete set, drift, plan_sha256
├── pull.rs         # inverse path map, render fixpoint, manifest generation, write ladder
├── verify.rs       # check list + findings + verdict
└── (existing: layout, render, tree, rules, lint, node, manifest, target, ...)
crates/ignition-core/src/actions/deploy.rs   # + deploy_plan, deploy_pull, deploy_verify (thin; no mirror/stage/commit)
```

### Pattern 1: Layered transport, with the write half unreachable from verbs
**What:** `ProcessRunner` (spawn/timeout/kill, mockable like `ComposeRunner`) under `RemoteShell` (builds ssh/sftp argv) under two traits: `TreeReader` (list + read, used by all verbs) and `TreeMirror` (`stage` before a lock, `commit(staged, delete_set)` inside it). Splitting the traits makes "no verb calls the write half" a compile-time fact for `actions::deploy`, and a source-grep test (the `guarded_ops_registry_tracks_every_dispatch_site` genre, `include_str!`) pins that `actions/deploy.rs` and `main.rs` never name `TreeMirror`, `stage` or `commit`.
**When to use:** always; the `stage`/`commit` shape (instead of one `mirror_tree`) lets Decision B swap only the implementation (B(i) staging outside the collection, B(ii) in-place with the lock covering the transfer).
**Example (sketch; names are proposals):**
```rust
#[async_trait::async_trait]
pub trait TreeReader: Send + Sync {
    /// Names only, scoped; fails closed on any transport error.
    async fn list(&self, coll: &Collection, scope: &Scope) -> Result<Vec<TreeEntry>, CoreError>;
    /// Bytes + client-side sha256 for the scoped files, capped per file and in total.
    async fn read(&self, coll: &Collection, scope: &Scope) -> Result<ObservedFiles, CoreError>;
    async fn read_ledger(&self) -> Result<Option<Vec<u8>>, CoreError>; // None = absent
}
```

### Pattern 2: Hardened ssh/sftp argv (verified with `ssh -G`)
**What:** Build argv as a `Vec<OsString>`; never a shell string. Pin every security-relevant option on the command line (command-line `-o` wins over `~/.ssh/config`, so the user's agent/ProxyJump still work). The option set below was run through `ssh -G` on OpenSSH 10.3p1 and every key was accepted and reported at the pinned value [VERIFIED: `ssh -G` this session].

| Option | Value | Why |
|--------|-------|-----|
| `BatchMode` | `yes` | no prompts, ever |
| `StrictHostKeyChecking` | `yes` | D-01; never `no`/`accept-new` |
| `UserKnownHostsFile` | the 0600 copy of the repo `known_hosts` in the temp dir | D-01; read via the symlink-refusing `read_tree_file`, copied, so a swap after review cannot redirect it |
| `GlobalKnownHostsFile` | `/dev/null` (unix) / `NUL` (windows) | runner state never consulted |
| `UpdateHostKeys` / `VerifyHostKeyDNS` | `no` | no silent rewrite or DNS trust |
| `PreferredAuthentications` | `publickey` | with `PasswordAuthentication=no`, `KbdInteractiveAuthentication=no`, `NumberOfPasswordPrompts=0` |
| `IdentitiesOnly` + `-i <tmp key>` | only when `key_env`/`key_file` set | D-03; otherwise leave to the agent |
| `ForwardAgent` / `ForwardX11` / `ClearAllForwardings` / `PermitLocalCommand` | `no` / `no` / `yes` / `no` | no side channels from a user's config |
| `ControlMaster` / `ControlPath` | `no` / `none` | no stale mux sockets outliving a timeout (Win32-OpenSSH has none anyway) |
| `ConnectTimeout`, `ServerAliveInterval`, `ServerAliveCountMax` | from `ssh.connect_timeout_s` (default 10), `5`, `3` | D-04; plus `tokio::time::timeout` per call |
| `LogLevel` | `ERROR` | keep stderr small |
| ssh only | `-T`, `-p <port>`, `user@host` | no TTY |
| sftp only | `-q -b - -P <port>` | `-P` is the port flag for sftp (ssh uses `-p`); `-b -` reads the batch from stdin [CITED: man.openbsd.org/sftp.1] |

Spawn rules: `stdin(Stdio::piped())` for both programs; for `ssh`, drop the handle immediately (EOF) instead of `Stdio::null()` because Win32-OpenSSH's client can hang with stdin from NUL (openssh-bugs Bug 2965; client-side only, so it matters when the *runner* is Windows) [CITED: lists.mindrot.org/pipermail/openssh-bugs/2019-February/020001.html]; for `sftp -b -` write the batch then drop. `kill_on_drop(true)` plus `tokio::time::timeout(d, child.wait_with_output())`; dropping the future on timeout drops the `Child`, which kills it, and `wait_with_output` closes stdin before waiting [VERIFIED: Context7 /websites/rs_tokio_tokio, Child::wait_with_output and Command::kill_on_drop]. Note the existing `DockerCompose::spawn` has none of these (no `kill_on_drop`, no timeout), so `ProcessRunner` is new code, not a reuse.

`key_env` materialisation (D-03): the env var holds the key *text*. Normalise CRLF to LF and ensure a trailing newline before writing, because OpenSSH 10.3 answers `Load key "...": invalid format` for a key with no trailing newline and for CRLF [VERIFIED: local `ssh-keygen -y -f` experiments this session]. Write 0600 inside a 0700 `tempfile::TempDir`; delete on drop. On a Windows client apply an owner-only ACL with `icacls <file> /inheritance:r /grant:r <user>:R` through the process runner (no crate); `[ASSUMED]` until a Windows-client run proves ssh.exe accepts it.

Paths in options: wrap a value containing whitespace in double quotes and refuse `"` or control characters (Windows usernames with spaces make `%TEMP%` paths with spaces realistic). `[ASSUMED]`: quoted `-o Key="C:/a b/x"` parses on Win32-OpenSSH; verify when a Windows client run exists.

### Pattern 3: sftp read protocol that cannot hide an error
**What:** One batch aborts on the first failing command with exit 1; a `-` prefix on a command suppresses that and exits 0 [VERIFIED: local experiment against `sftp -D /usr/libexec/sftp-server`; CITED: man.openbsd.org/sftp.1]. A `-get` therefore turns "permission denied" into a silent partial read, which a diff would misreport as `added`/`removed`. So: never prefix, and never reference a path that may not exist.
1. Session 1: `@ls -1 "<root>"` lists the collection root (module dirs). The `@` prefix suppresses command echo.
2. Session 2: one `@ls -1 "<root>/<module>"` per allowlisted module that session 1 showed present (type dirs).
3. Session 3: one `get -R "<root>/<module>/<type>" "<tmp>/"` per allowlisted type that session 2 showed present, into a fresh local temp dir; then walk it with the symlink-refusing sorted walker and hash locally.
Three handshakes (no ControlMaster on Win32-OpenSSH) is the price of being unable to confuse "absent" with "failed". Any non-zero exit, any stderr line, or any timeout is `transport_failed`. A race (dir vanishes between sessions) is also `transport_failed`, which is correct.
Parse rule for `ls -1`: output lines are the *given directory path prefixed to each entry* (verified locally: `<R>/ignition/tag-definition/P/Folder A`), so strip the exact prefix and take one component; do not assume bare names. `[ASSUMED]` for the Windows server's formatting (C7a).
Never interpolate a remote-derived name into any later command; only `data_dir` (target YAML), manifest `module/type` halves and provider names (validated) are ever interpolated.

`safe_remote_path()` (one function, unit-tested with a hostile table): refuse control characters (including newline), `"`, backslash (after the Windows drive normalisation), and the sftp glob metacharacters `* ? [ ] { }`. Reason: the sftp client globs paths, and `ls "<dir>/odd[1]"` only worked because no `odd1` existed to match the character class; a sibling would silently win [VERIFIED: local experiment]. Module/type segments reuse `client::config_resources::validate_segment` (ASCII `[A-Za-z0-9._-]`). Windows data dirs normalise `C:\Program Files\...` to `/C:/Program Files/...` in one function with test vectors; the `/C:/` form is web-sourced and INFERRED until C7a [CITED: curl test-suite commit and WinSCP FAQ via WebSearch; MEDIUM].

### Pattern 4: Plan engine over resource units, on the effective view
**Unit.** Group files by their *unit directory*: the nearest ancestor (including the file's own directory) that contains `resource.json` or `unary-resource.json`; fall back to the file's parent. `layout::resource_key` is not enough: for `ignition/tag-definition/WHK01/FolderA/tags.json` it returns name `WHK01` and would collapse a whole provider into one entry [VERIFIED: layout.rs:161-172, name is `parts[2]` only when 4+ components]. Tag units therefore come out per folder, resources per `<module>/<type>/<name>`, singletons per `<module>/<type>`.

**Layers.** `ObservedFiles { core, mode }`, each a `BTreeMap<RelPath, FileBlob{bytes, sha256}>` restricted to allowlisted types and `manifest.tag_providers` (tags under the four `TAG_TYPES`). The `<mode>` collection dir is `target.mode`; `core` is the literal collection `core`. `local` overrides everything and `external` underlies it (load order external, core, mode, local [CITED: design spec section 3]); list `local/` for allowlisted types and emit a plan *warning* if any exist (verify's REST read-back catches the mismatch for real).

**Effective copy of a unit:** the mode copy if the unit exists in the mode collection, else the core copy (whole-resource override for config, folder granularity for tags [CITED: spec S1/S2/S2b]).

| Status | Condition (recommended basis, Open Question 1) |
|--------|-----------------------------------------------|
| `added` | rendered unit, no effective copy |
| `changed` | rendered unit, effective copy exists, file set or any byte differs |
| `same` | rendered unit, effective copy byte-equal (same file set) |
| `removed` | not rendered, every ledger file of the unit is in the delete set, an effective copy exists |
| `unmanaged` | not rendered, effective copy exists, unit has no ledger file; never deleted, excluded from `has_changes` (D-06) |

Byte-exact comparison per file, not semantic: render is passthrough (`render_sha256` is a listing of raw sha256s), so an apply of unchanged bytes yields `same` and a no-op UI save that only touches `lastModification` is honestly `changed`. Entry `live_in` (additive key) is a subset of `["core","mode"]`, so Phase 23 can still see that a `same` unit is not yet in the mode collection.

| `core` relation | Definition (recommended; `[ASSUMED]` names per FEATURES 4.1) |
|-----------------|-----------------|
| `none` | no core copy of the unit (also used for `unmanaged`, which has no rendered counterpart; `live_in` tells the truth) |
| `identical` | core copy exists, same file set, byte-equal to rendered |
| `differs` | core copy exists, not identical, unit not removed (mode wins; UI shows the core copy) |
| `hidden` | tag-definition / tag-type-definition folder unit where the mode defines the unit and the core copy has files the rendered unit lacks (core-only content hidden by folder granularity, S2) |
| `shadows_removal` | status `removed` and a core copy of the unit exists, so deleting from the mode collection will not take effect (S2b). Phase 23 blocks on it regardless of flags |

`destructive` (bool + `destructive_reason`): `removed` of anything; reasons `tag_data_loss` for `ignition/tag-provider|tag-group|tag-type-definition|tag-definition`, `connection_removed` for `ignition/database-connection|opc-connection|identity-provider`, `storage_removed` for `ignition/store-and-forward-engine|alarm-journal`, else `resource_removed`. `lockout_risk`: any non-`same` entry of `ignition/identity-provider` or `ignition/security-zone` (the security singletons are already `DENIED_TYPES` and unrenderable). All of these type names are present in `OPENAPI_TYPES` [VERIFIED: rules.rs:462-527]. The table itself is `[ASSUMED]` policy from FEATURES 4.1.

**Delete set** = ledger files minus rendered files, then filtered: only paths whose type is currently allowlisted (others dropped with a warning, never deleted), never a `GATEWAY_OWNED_FILES` name, never dot-named, and only files that exist in the *mode* collection (files only in `core` are `shadows_removal` evidence, not deletions). Reported in `data.delete_set` (`files[]`, byte-sorted).

**Drift (`--base DIR`)**: base = a directory written by `render --out`. Walk it with the existing hardened walker (promote `tree::walk_out` to `pub(crate)`) and hash the real bytes; read `render.json` only for identity (`gateway`, `env`, `mode` must match, else `invalid_input`) and as a consistency check (file list equals the walked list, otherwise refuse the base as modified). This is the "re-hash instead of trusting render.json" resolution of 21-SECURITY R-03/WR-03 [CITED: 21-SECURITY.md R-03]. Per file: `drift = live != base && live != desired`; entry `drift` is true if any file drifts; `null` for every entry when no `--base` was given.

**`plan_sha256`** (D-08; discretion on serialization): SHA-256 over the UTF-8 text
```
ign-deploy:plan:v1 LF
render_sha256 NUL <hex> LF
then per entry in byte order of key, excluding unmanaged:
  E NUL <key> NUL <status> NUL <core> LF
  F NUL <collection path> NUL <live sha256 or -> NUL <desired sha256 or -> LF   (files byte-sorted)
then per delete-set file in byte order:
  D NUL <collection path> LF
```
Excluded on purpose: `drift`, `diff` text, `warnings`, `unmanaged`, so the same review hash is reproducible by CI with or without `--base`. Included: `core`, because a core strip between review and apply changes whether a removal takes effect. Pin a golden vector like `RENDER_GOLDEN`; a change means re-goldening, and Phase 23 recomputes under the lock, so no clock, locale or map-iteration dependence.

### Pattern 5: The ledger (Phase 22 defines the format and reader)
Proposed v1 (JSON, strict `deny_unknown_fields`, size cap 8 MiB, same JSON hardening as `node.rs`):
```json
{"schema_version":1,"gateway":"distillery01","mode":"production","render_sha256":"<hex>","run_id":"<id>","files":["ignition/database-connection/MES/config.json"]}
```
`files` are sorted, unique, valid `RelPath` collection paths. The reader refuses (D-07, one new target-state slug, see Open Question 10) a ledger that is unreadable, not UTF-8 JSON, wrong `schema_version`, names a different `gateway` or `mode`, is unsorted/duplicated, or lists `config-mode.json`, a dot-named component or a path under a type that is not a declared resource path. It never falls back to empty. Absent file = first deploy = empty delete set. Hashes are deliberately not stored: they would invite use as a drift base, which is out of scope (the cross-check with `--base` was deferred). Location is one function `ledger_location(node, gateway, mode)`; the value is decided by C6/Decision B (candidates: home-relative `ign-deploy/<gateway>/<mode>/ledger.json` under B(i); a sibling of `resources/` under `data/` for `LocalFsTransport`). Never inside `data/config/resources/` and never dot-named.

### Pattern 6: Pull = inverse of render, validated by render
Algorithm (all read-only against the gateway):
1. Resolve allowlist: existing `manifest.yaml` (D-10) or generate one from live types intersected with `TYPE_RULES` (render refuses any type without a row, so a generated manifest must not list one) minus `DENIED_TYPES`; tag providers = live `ignition/tag-provider/*` names. Types dropped for lack of a rule go to `data.skipped[]`.
2. Read `core` and the mode collection (scoped). Invert paths: collection `ignition/<tagtype>/<P>/<rest>` becomes repo `tags/<P>/<tagtype>/<rest>`; everything else becomes `resources/<collection path>` (the exact inverse of `layout::collection_path`, CD5 swap [VERIFIED: layout.rs:94-97, 124-147]).
3. Base tree <- core files; `modes/<env>/` overlay <- mode-collection files (D-12).
4. **Render fixpoint:** run `render()` on the assembled `SourceTree`. It collects *all* findings in one raise (`embedded_secret` carries paths and other findings; `deploy_tree_invalid` carries file-named findings), so map each finding's file to its unit, drop those units into `data.skipped[]` (`reason` = `embedded_secret` | the value-free finding message), and re-run until clean (bounded rounds). This satisfies D-09 and also removes anything render would refuse (hostile names, unsupported tag layouts CD5 leaves fail-closed, case-folds), so the written tree always renders.
5. **Invariant check before writing:** `render(tree).files` must equal the effective live files for every kept unit; units where render would rewrite bytes (for example a minted uuid) are skipped with reason `not_round_trippable`, never silently written. This is what makes SC-3 a testable property instead of a hope.
6. Write with the existing clobber-ladder shape. Non-empty `gateways/<name>/` without `--yes` is `confirmation_required` whose operation prose lists what would be replaced (count + first N paths); with `--yes` replace the `resources/`, `tags/` and `modes/<env>/` subtrees (git is the comparison tool, per CONTEXT specifics), keep an existing `manifest.yaml`, write a generated one only when absent. Hand-emit the manifest YAML (`schema_version: 1`, `types:`, `tag_providers:`); component names are already validated by `component_problem`.
Embedded-secret units are reported by path only: `SecretScan` has no field that can hold a JSON string [CITED: lint.rs module doc], so reuse `scan_embedded`/`scan_secrets` for detection and never print values.

### Pattern 7: Verify check list (master, read-only)

| # | Check | Source | Pass | Finding code |
|---|-------|--------|------|--------------|
| 1 | Scan idle | `scan_status(Config)`: `!scan_active` (a standalone verify cannot trigger a scan, so `scan_complete(before, now)` belongs to Phase 23) | idle | `scan_active` |
| 2 | Mode | `deployment_mode()` normalized equals `target.mode` | equal | `mode_mismatch` |
| 3 | Read-back | per declared non-tag type one `resource_list(module, ty, limit=-1)` (list items carry `config`, one call per type) or `resource_singleton`; compare `config` as `serde_json::Value` against the rendered `config.json` plus `attributes.uuid`/`enabled` when rendered | equal | `resource_missing`, `resource_differs` |
| 4 | Tag/other files | tags have no REST route [CITED: spec section 3]; compare the mode collection's file hashes to the render over the transport | no diff | `files_differ` |
| 5 | Loaded | `logs(&LogQuery{min_level: WARN, start_time: last_scan_timestamp - last_scan_duration - slack, limit: DEFAULT_LOG_LIMIT, ..})`; a hit is a message containing `will not be loaded` AND the `module/type/name` of a declared resource | no hit | `unloaded_resource` |
| 6 | Probes | each `verify.tag_probes[]` through `tags_read` (WebDev route) polled to a deadline; Good = quality string starting `Good`; empty/absent list = **warning** `tag_probes_absent`, never a pass | all Good | `tag_probe_not_good`, `tag_probes_unavailable` |

Notes: `lastScanTimestamp` is epoch-ms of the last scan's *end* on the gateway clock [VERIFIED: scan.rs:153]; log timestamps are on the same clock, so the window is gateway-vs-gateway (never the runner clock). The verbatim message to match is `Resource ignition/database-connection/SPIKE_DB_1791578731 does not have required UUID attribute, will not be loaded` with logger `NamedResourceHandler-ignition/database-connection` [CITED: spikes/gateway-modes/results/RIG.md:14, fetched from whk-environment-orchestration PR #60]; whether the REST `logs` route returns that same `message` is `[ASSUMED]` and is a rig capture (write a uuid-less named resource through `LocalFsTransport`, scan, read the log API). Never echo `LogEntry.message` or tag `value`; emit only `{timestamp, logger, resource_key}`. `allowInvalidReferences` is a write-only parameter in the OpenAPI extract (appears only on PUT/POST) so reference validation cannot ride the read-back; record the opt-out in the phase COVERAGE.md [CITED: 20-openapi-extract.json scan this session; extract has 19 paths].

Envelope: `data = {env, gateway, mode, ok, slug, checks:[{id, status: pass|fail|warn, detail, findings[]}], resources_checked, warnings}`; `slug` is `deploy_verify_failed` only when `ok=false`. A `failure_exit_code()` on the result returns `Some(6)` when `!ok`; `main.rs` applies it after rendering, exactly the `TestingRunResult::failure_exit_code` seam [VERIFIED: testing.rs:162-167, main.rs:536-549]. Warnings never fail the verdict. The slug is not a `CoreError` and never joins an exit table.

### Pattern 8: Surface wiring checklist (same commit series as each verb; DSURF-01)
- `cli.rs` `DeployCommand` gains `Plan{env, repo, base, diff}`, `Pull{env, repo}`, `Verify{env, repo, project}` (`--project` default `ign-cli` like every tags verb [VERIFIED: cli.rs:807]).
- `main.rs`: three `ActionOutput` variants + `render_success` arms + human renderers in `render.rs`; dispatch arms resolve the master profile *from the target* (`Session::resolve_side`), not from `--profile`; envelope `profile` = the master profile name.
- `routes.rs`: rows `deploy plan`, `deploy pull`, `deploy verify` -> `Screen::Projects` (the existing `every_deploy_leaf_has_a_projects_route_row` walks both directions).
- `GUARDED_OPS`: `("deploy pull", "<dynamic refusal prose>")` plus an adjacent `// guarded:deploy pull` marker in `main.rs`; the drift test pins markers in both directions. `deploy plan`/`verify` stay unguarded (mirror `deploy_render_is_an_unguarded_catalog_tool` in mcp.rs).
- README: Commands rows (`ign deploy plan`, `ign deploy pull`, `ign deploy verify` followed by a space or backtick), the deploy section, the exit-table rows for new slugs, and the dated verdict exception section.
- `CoreError::DEPLOY_SLUGS` extended with each new deploy slug (README `## Exit codes` must contain it backticked).

### Anti-Patterns to Avoid
- **`-` prefix on sftp commands** to tolerate absent dirs: hides permission errors as empty results.
- **Temp or backup files inside `data/config/resources/`** (including `.tmp` next to a target): the gateway scans that tree and Designer throws `NameInvalidException` on dot-names [CITED: SUMMARY.md gaps]. Write in place; the lock + snapshot are the safety net.
- **`robocopy /MIR`** or any "delete extras": rejected; deletes come only from the bounded list.
- **Trusting `render.json` hashes** for `--base`: re-hash the files.
- **Comparing gateway timestamps to the runner clock.**
- **Echoing gateway log lines, tag values, ssh stderr or file bytes verbatim** in envelopes or errors.
- **Writing `core`.** Phase 22 never writes the gateway at all.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Unified text diff for `--diff` | A line differ | `similar::TextDiff` | Edge cases (context, line endings); already compiled in the tree |
| Secret detection | A new regex scanner | `lint::scan_embedded`, `lint::scan_secrets`, `rules::secret_paths` | Schema-path based, value-free by construction (21-08) |
| Strict YAML/JSON parsing | Ad-hoc parse | `yamlio::strict_parse`, `node::classify` | Size caps, depth caps, duplicate keys, scrubbed errors |
| Hardened directory walk | New walker | `tree.rs` `walk_out`/`Reader` pattern (promote `walk_out`) | Sorted, `symlink_metadata`, UTF-8 names, never follows links |
| Scan/lock/mode decisions | Re-derive | `scan_complete`, `lock_verdict`, `normalize_deployment_mode`, `DeployApi` | Capture-proven predicates (Phase 20) |
| Subprocess spawning | Per-call `Command` | one `ProcessRunner` | Timeouts + `kill_on_drop` + test double in one place |
| Tag path/mapping | New mapping | `layout::collection_path` (+ its inverse written next to it) | CD5 is capture-backed |
| Exit-code decisions | New codes in `main.rs` | `CoreError::exit_code()` + the verdict hook | Frozen taxonomy |

**Key insight:** every correctness-critical piece (secret scan, mapping, hashing, walking, predicates) already exists and is capture-backed. Phase 22 is glue plus the ssh/sftp edge; the risk is in the edge and in the live-basis decision, not in algorithms.

## Common Pitfalls

### Pitfall 1: The live basis (mode collection vs effective view) decides whether SC-3 can hold
**What goes wrong:** plan compares against the mode collection only; `pull` puts `core` into the base tree (D-12); `render` then includes core-only resources; `plan` reports them `added` and "render then plan is empty" fails. Rig note: creating a database connection makes the gateway write a `core/ignition/store-and-forward-engine/<same name>/` copy too [CITED: 20-LIVE-CAPTURES D10/D11], so a core-only allowlisted resource appears even on a clean rig.
**How to avoid:** adopt the effective-view basis (Pattern 4) and surface "not yet in the mode collection" through `live_in`. See Open Question 1.
**Warning signs:** a post-pull plan with only `added` entries whose `core` is `identical`.

### Pitfall 2: Silent partial reads
**What goes wrong:** a read that skips a failed directory looks like deleted resources.
**How to avoid:** Pattern 3 (existence-first, no `-` prefix, stderr = failure); compare entry counts between `ls` and the walked download.

### Pitfall 3: Path injection into sftp batch lines and PowerShell
**What goes wrong:** names with newline, quote or glob characters change which command runs or which path matches.
**How to avoid:** `safe_remote_path()`; no remote-derived string in a command; for PowerShell (Wave B) pass parameters as a base64 JSON blob decoded inside a fixed script, never by string interpolation.

### Pitfall 4: Non-trailing-newline / CRLF key from a CI secret
**What goes wrong:** `Load key ... invalid format`; looks like an auth failure. **How to avoid:** normalise on write (Pattern 2) and test with both defects.

### Pitfall 5: Windows PowerShell 5.1 over ssh (Wave B)
CLIXML on stderr when progress is emitted (prelude `$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'`; parse only an `IGN-RESULT:` sentinel line) [ASSUMED from general knowledge; the web search corroborated the CLIXML mechanism but not this exact prelude]; base64(UTF-16LE) inflates 2.67x, and a `cmd.exe` default shell caps the command line at 8,191 characters, so keep scripts under about 3,000 characters or ship a `.ps1` in the staged archive [CITED: STACK.md section 1]; default shell may be `cmd`; capture exit-code propagation (C7a).

### Pitfall 6: False-red verify from REST normalisation
REST `config` equaled the file `config.json` as a JSON value for `database-translator` (same keys, different order) [CITED: 21-LIVE-CAPTURES.md C21-UUID8 find vs file], but other types may gain defaults or mask fields. **How to avoid:** a rig live gate renders the tracer repo and the widest available type set, writes via `LocalFsTransport`, scans, and runs verify; any false red is fixed by a per-type normalisation table, not by loosening equality globally.

### Pitfall 7: Case-insensitive host filesystems
macOS/Windows `read_dir` and lookups can hide case-variant collisions (21-SECURITY R-07). Compare listed names exactly; never `exists()` a computed name to test presence.

### Pitfall 8: Asynchronous tag-file flush
The gateway wrote tag files 5-15 s *after* an import answered success [CITED: 21-LIVE-CAPTURES CD4]. Live tests that write via the REST/tags API and then read files must poll. Plan itself reads settled state.

### Pitfall 9: Ledger written by a different gateway/mode
A ledger copied between boxes or modes would widen the delete set. D-07's gateway+mode check plus the allowlist filter and `config-mode.json` refusal bound the blast radius.

### Pitfall 10: Secrets via `plan --diff`, verify, errors and stderr
Live files contain Embedded ciphertext (spec: 10 files in the dev snapshot). **How to avoid:** for any file where either side has a `SecretScan` refused location or warning, print `withheld: contains secret-bearing locations (n)` plus both sha256s instead of a diff; cap and scrub `transport_failed.detail` (last N stderr lines, temp-dir paths replaced). Canary tests cover plan, `--diff`, verify, pull, `--json`, stderr and Debug (Phase 23 completes the canary).

### Pitfall 11: Verb accidentally writes
`DeployApi` is one trait including POST/PUT. Test wrapper `ForbidWrites<T: DeployApi>` in `tests/common` panics on `scan_lock_acquire`, `scan_request`, `resource_create`, `resource_update`, `mode_create`; the transport trait split covers files. Optional hardening: a `DeployRead` subset trait with a blanket impl.

### Pitfall 12: Tests that need an sshd or Docker pass nowhere
`ubuntu-latest`, `macos-latest`, `windows-latest` all run `cargo test --workspace` [VERIFIED: .github/workflows/ci.yml]. Required offline evidence: `ProcessRunner` fakes, argv contract tests via `ssh -G`, and a `#[cfg(unix)]` real-protocol test through `sftp -D <sftp-server>`; discover the server at `/usr/libexec/sftp-server` (macOS, present on this host) or `/usr/lib/openssh/sftp-server`. If it is absent on a runner, the test must fail loudly, not skip (project rule against silent skips); `[ASSUMED]` that ubuntu-latest has it, confirm in the first CI run.

## Code Examples

### sftp batch for the read half (argv + stdin; no shell string)
```rust
// Source: pattern verified locally against `sftp -D /usr/libexec/sftp-server`; options verified with `ssh -G`.
let mut argv: Vec<OsString> = vec!["-q".into(), "-b".into(), "-".into(), "-P".into(), port.to_string().into()];
for (k, v) in hardened_options(&known_hosts_copy, key_file.as_deref(), connect_timeout_s) {
    argv.push("-o".into());
    argv.push(format!("{k}={v}").into());
}
argv.push(format!("{user}@{host}").into());
// stdin: "@ls -1 \"/C:/Program Files/.../resources/production\"\n"   (paths from safe_remote_path only)
```

### Verdict hook (existing seam to copy)
```rust
// Source: crates/ignition-core/src/actions/testing.rs:162-167 (pattern), main.rs:536-549 (call site)
pub fn failure_exit_code(&self) -> Option<u8> { if self.ok { None } else { Some(6) } }
// main.rs, after render_ok(&out, ..):
// if let ActionOutput::DeployVerify(v) = &out && let Some(code) = v.failure_exit_code() { return ExitCode::from(code); }
```

### Write-forbidding test double
```rust
// Source: proposal; wraps the existing tests/common FakeDeploy
struct ForbidWrites<T>(T);
// impl DeployApi for ForbidWrites<T>: forward reads; scan_lock_acquire/scan_request/resource_create/
// resource_update/mode_create => panic!("plan/verify/pull must not write")
```

### Local real-protocol test (unix)
```
sftp -q -D /usr/libexec/sftp-server -b - <batch on stdin>
# argv builder gets a test-only `extra_args` seam so `-D <server>` replaces `user@host`.
```

## Captures that block later work (C6, C7a, OQ5) - procedure for `22-LIVE-CAPTURES.md`

Human checkpoint first (D-13): provision WFLWVM01 with Ignition 8.3.6, OpenSSH Server, a non-admin `svc-ignition-deploy` with key auth, a `production` mode, an API token profile; add its host key to the repo `known_hosts`. Mark the plan task `checkpoint:human-action` (autonomous: false). Use a new driver (the existing `capture.sh` `assert_local` refuses non-localhost); stamp each block with the exact command and a script-taken UTC time like `check21`.

- **C7a (blocks the Windows code only, D-14):** sftp path forms (`/C:/Program Files/...` vs `C:/...` vs backslashes) for `ls -1`, `get -R`, `put`; `ls -1`/`ls -l` output format (prefix behaviour); a dir with spaces/commas; `sftp -b -` exit codes on not-found and permission-denied and the stderr text; `ssh host powershell -NoProfile -NonInteractive -EncodedCommand ...` exit-code propagation, default shell, CLIXML on stderr with and without the prelude; max command length; sftp-server long-path (over 260) behaviour; host key types offered (for `known_hosts`); the `administrators_authorized_keys` ACL rule if the account is an admin; Windows-client `ssh.exe` stdin-NUL behaviour if a Windows runner is in scope.
- **C6 (decides Decision B):** can `svc-ignition-deploy` create/write a home-relative staging dir; can it write directly into `<data_dir>\config\resources\production\`; can it write `<data_dir>\ign-deploy\`; **can it read `core\` and `production\`** (plan's effective-view basis needs core reads); is a staging dir ever read as a collection (home-relative: trivially no; if a sibling of `resources/` is proposed, check `GET /mode` and the scan log). Outcome to record: B(i) or B(ii), and the ledger/journal location.
- **OQ5 (sets the hold budget):** with a prod-sized tree from `pull` (D-16): p50/p95 of read-half time over at least 20 runs; write-half time (stage and commit) and a real scan time through a capture-only `#[ignore]` harness that calls the library directly (no verb may); with and without a Defender exclusion; record p95, floor, ceiling and the derived `holdTimeout = 2*(write+scan)+1` replacing the spec's 5 s [CITED: spec section 6.4.3 and S4].
- **Rig-side (no WFLWVM01 needed):** the REST log message shape and window for the "will not be loaded" scrape (above), and the verify false-red sweep (Pitfall 6).

Exit gate (before Phase 23 is planned): Decision B recorded, hold budget recorded. Add the DPLN-02 note for `unmanaged` (D-06) to REQUIREMENTS.md in this phase.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| STACK.md: `robocopy /MIR`, snapshots in `data/config/.ign-deploy/` | Bounded delete list; no dot-names or staging in `resources/` | SUMMARY conflict resolution 2 (2026-10-10) | Supersedes the STACK.md lines |
| ARCHITECTURE 4.4: three coarse verbs `hash_tree/read_tree/mirror_tree` | `TreeReader` (list/read) + `TreeMirror` (stage/commit) | this research | Compile-time read-only proof; B(i)/(ii) swap |
| `plan_id` / `--expect-plan` | `plan_sha256` / `--plan-sha256` | SUMMARY conflict 8 | Use the new names everywhere |
| Test ssh with a fake binary built per OS | `ssh -G` argv contract + `sftp -D <sftp-server>` + `ProcessRunner` fake | this research | No sshd needed on CI |

**Deprecated/outdated:** ARCHITECTURE 7.1 slug list is superseded where SUMMARY conflict 7 differs; `deploy_journal_present` and apply-side slugs belong to Phase 23.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Plan statuses compare against the effective view (mode over core); `live_in` carries mode/core presence | Pattern 4, Pitfall 1 | SC-3 fails pre-strip; or Phase 23 cutover write is invisible. Needs user decision (OQ1) |
| A2 | `hidden` = tag folder unit with core-only files while the mode defines the unit | Pattern 4 | DPLN-03 value mis-set; only tag-heavy gateways affected |
| A3 | Destructive/lockout tables (FEATURES 4.1) are the intended policy | Pattern 4 | Flags over- or under-fire; apply gating in Phase 23 depends on them |
| A4 | `/C:/...` is the Win32-OpenSSH sftp path form | Pattern 3 | Reads fail; one normalisation function to swap (C7a) |
| A5 | `ls -1` on the Windows server prefixes the given directory like the Unix server | Pattern 3 | Listing parser breaks; C7a |
| A6 | REST `logs` returns the "will not be loaded" text with the resource path; window `last_scan_timestamp - last_scan_duration - slack` | Pattern 7 | Check 5 misses or over-reports; rig capture |
| A7 | REST `config` equals file `config.json` for all declared types (proven for `database-translator` only) | Pitfall 6 | False-red verify; rig sweep |
| A8 | Tag probes may ride the WebDev `tags` route and need the `ign-cli` bundle on the target | Pattern 7 | Probes unusable on a gateway without the bundle (finding `tag_probes_unavailable`) |
| A9 | Good = quality string starting `Good`; default probe deadline 30 s at 1 s cadence | Pattern 7 | Slow-start tags flagged red |
| A10 | Defaults `connect_timeout_s=10`, `op_timeout_s=120` | D-04 | Premature timeouts on big trees; revisit after OQ5 |
| A11 | Quoted `-o Key="C:/a b/x"` parses on Win32-OpenSSH; owner-only ACL via `icacls` accepted | Pattern 2 | Windows-client runs fail |
| A12 | PowerShell prelude prevents CLIXML on stderr | Pitfall 5 | Stray stderr parsed as failure; sentinel parsing mitigates |
| A13 | ubuntu-latest has an `sftp-server` binary for the real-protocol test | Pitfall 12 | CI red on first run; add the package or locate the binary |
| A14 | New slug names `transport_failed` (exit 4) and `deploy_ledger_invalid` (exit 6) | Pattern 5/8 | Naming churn; exit class is a contract decision |
| A15 | Pull `--yes` replaces the `resources/`, `tags/`, `modes/<env>/` subtrees and keeps an existing `manifest.yaml` | Pattern 6 | Stale files linger or hand edits lost |
| A16 | `svc-ignition-deploy` can read `core/` and the mode collection | Pattern 4 | Plan needs the REST fallback (`resource_names.modes`, `find?collection=core`) and loses tag core relations (C6) |

## Open Questions

1. **What does `plan` compare against: the effective view or the mode collection only?** (Highest impact.)
   - Known: D-12 makes `render` the effective view; D-05/DPLN-04 delete from the mode collection; spec section 13 step 2 wants an empty plan before any mode exists; step 4.2 later "writes a collection identical to what's live".
   - Unclear: whether an unstripped core-only resource is `same` (effective) or `added` (to be written).
   - Recommendation: effective view + `live_in`. If the user prefers mode-only, SC-3 must be restated as holding after the core strip, and `pull` should not copy core resources that the mode collection lacks into the base tree. Decide in `/gsd-plan-phase` before wave 1.
2. **Where does the ledger live?** Depends on C6/Decision B. Plan the reader and format now; keep `ledger_location()` a single function.
3. **Can the service account read `core/`?** (A16) Add to C6; the effective-view basis depends on it.
4. **REST log shape/window** (A6): rig capture, not WFLWVM01.
5. **Probes need the WebDev bundle** (A8): confirm acceptable for prod, or choose another probe path.
6. **`hidden` definition** (A2) and the destructive/lockout tables (A3): confirm.
7. **Pull overwrite scope** (A15).
8. **MCP exposure of `pull`** creating directories without `confirm` when the target is empty or absent (the 21-SECURITY R-11 pattern). D-11 guards only non-empty overwrite; decide whether MCP should always require `confirm` for `pull`.
9. **WFLWVM01 slip:** if provisioning stalls, SC-1 stays open (wh-win was declined, D-15). The plan should keep Windows-only code isolated in the last wave.
10. **Slug names and classes** for the ledger refusal and transport failure (A14); the four-place rule applies.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `ssh`, `sftp`, `ssh-keygen` | transport tests, argv contract | yes | OpenSSH_10.3p1 (LibreSSL 3.3.6) | none needed |
| local `sftp-server` | real-protocol test | yes (macOS) | `/usr/libexec/sftp-server` | locate `/usr/lib/openssh/sftp-server` on Linux CI (A13) |
| Rust toolchain | build/test | yes | rustc/cargo 1.94.1 (workspace MSRV 1.88) | — |
| Docker + `inductiveautomation/ignition:8.3.6` image | rig live gates (log shape, verify sweep) | yes (OrbStack, image present) | 29.8.2 | live tests are `#[ignore]` |
| `pwsh` | Wave B script logic test on dev Mac | yes | 7.x at `/usr/local/bin/pwsh` | `windows-latest` leg runs `powershell.exe` |
| `gh` | reading the external design spec | yes | — | — |
| WFLWVM01 (Ignition 8.3.6 + Win32-OpenSSH + `svc-ignition-deploy`) | C6, C7a, OQ5, SC-1 | **no, not provisioned** | — | none (D-15); human setup checkpoint |

**Missing dependencies with no fallback:** WFLWVM01 (blocks SC-1 closure, Decision B, the hold budget).
**Missing dependencies with fallback:** none.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | `cargo test` (libtest) + `proptest` 1.11 + `wiremock` 0.6 + `assert_cmd` 2.2 |
| Config file | none; workspace `Cargo.toml`; CI matrix ubuntu/macos/windows |
| Quick run command | `cargo test -p ignition-core --lib deploy::` (86 tests, 0.26 s after a ~40 s compile; baseline green) |
| Full suite command | `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo build -p ignition-cli --no-default-features` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| DPLN-01 | argv hardening (every pinned option reported by `ssh -G`); known_hosts copy; key CRLF/no-newline normalised; timeouts kill the child; `safe_remote_path` hostile table | unit + process | `cargo test -p ignition-core --lib deploy::transport` | no, Wave 0 |
| DPLN-01 | real sftp protocol: existence-first read, error = failure, spaces, glob refusal | integration (`cfg(unix)`) | `cargo test -p ignition-core --test deploy_transport` | no, Wave 0 |
| DPLN-01 | write half: stage/commit with explicit delete set against scratch dir; `config-mode.json` and undeclared files survive; no temp/dot files created | integration | `cargo test -p ignition-core --test deploy_transport mirror` | no, Wave 0 |
| DPLN-01 | no verb names the write half | source-grep | `cargo test -p ignition-core --test deploy_transport write_half_unreachable` | no, Wave 0 |
| DPLN-01 | proven on a real Windows OpenSSH host | live, `#[ignore]` + human | `cargo test -p ignition-core --test live_transport_wflwvm01 -- --ignored --nocapture` | no, Wave B |
| DPLN-02 | statuses incl. `unmanaged`; zero writes (`ForbidWrites`); exit 0; `has_changes`; golden `plan_sha256` | integration | `cargo test -p ignition-core --test deploy_plan` | no, Wave 0 |
| DPLN-03 | each core relation + destructive/lockout flags on seeded layers | integration | `cargo test -p ignition-core --test deploy_plan core_relation` | no, Wave 0 |
| DPLN-04 | delete set = ledger - rendered; allowlist filter; ledger corrupt/other gateway/mode refused; absent = empty | unit + integration | `cargo test -p ignition-core --lib deploy::ledger` and `--test deploy_plan delete_set` | no, Wave 0 |
| DPLN-05 | drift three-way; stale/mixed `--base` refused; `render.json` hashes not trusted | integration | `cargo test -p ignition-core --test deploy_plan drift` | no, Wave 0 |
| DPLN-06 | pull -> render -> plan empty on rig-shaped fixtures; skipped list for Embedded; overwrite refused without `--yes` | integration | `cargo test -p ignition-core --test deploy_pull` | no, Wave 0 |
| DPLN-07 | each verify check fails independently and passes together | integration | `cargo test -p ignition-core --test deploy_verify` | no, Wave 0 |
| DPLN-08 | probe Good/not Good/absent-list warning | integration | `cargo test -p ignition-core --test deploy_verify probes` | no, Wave 0 |
| DPLN-09 | red verify: `ok:true`, `data.ok=false`, exit 6, slug only in `data`; green exit 0 | CLI contract | `cargo test -p ignition-cli --test contract_deploy verify` | extend existing |
| DSURF-04 | README has the dated verdict-exception section; `deploy_verify_failed` absent from exit table | test | `cargo test -p ignition-cli --test deploy_family_coverage` | extend existing |
| DSURF-01/05 | TUI rows, README Commands rows, exit-table rows for new slugs, MCP catalog (`deploy_plan`, `deploy_pull` guarded, `deploy_verify`) | existing + new | `cargo test -p ignition-cli --test deploy_family_coverage --test tui_coverage` and `cargo test -p ignition-cli --lib mcp` | partially exist |
| canary | seeded canary absent from plan, `--diff`, verify, pull, `--json`, stderr, Debug | integration | `cargo test -p ignition-core --test deploy_plan canary` | no, Wave 0 |
| four-place | new slugs in header table, README, `EXIT_SLUG_LITERALS`, `readme_exit_table_agreement` | unit | `cargo test -p ignition-core --lib error::` | exists; extend |
| live (rig) | log-scrape shape, verify false-red sweep, pull/render/plan on a real 8.3.6 | live `#[ignore]` | `cargo test -p ignition-core --test live_deploy_plan -- --ignored --nocapture` | no, Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p ignition-core --lib deploy::` plus the touched integration test file.
- **Per wave merge:** the full suite command above (also `cargo install --path crates/ignition-cli --force` before any manual smoke).
- **Phase gate:** full suite green on all three OS legs, then `/gsd-verify-work`; live rig gate and the WFLWVM01 captures recorded in `22-LIVE-CAPTURES.md`.

### Wave 0 Gaps
- [ ] `tests/common`: `ForbidWrites<T>` wrapper, a `GatewayApi` double with `logs` + WebDev `tags` rows for verify, a `deploy_plan` fixture builder (layers core/mode/ledger over `tests/fixtures/deploy/repos/tracer`)
- [ ] `crates/ignition-core/tests/{deploy_transport,deploy_plan,deploy_pull,deploy_verify}.rs`, `live_deploy_plan.rs`, `live_transport_wflwvm01.rs`
- [ ] `ProcessRunner` fake (records argv/stdin/timeouts, scripted outputs)
- [ ] `similar` promoted to `ignition-core` dependencies
- [ ] Promote `tree::walk_out` to `pub(crate)`; extract the load+render step of `deploy_render` into a shared function

## Security Domain

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes | Key-only ssh (`PreferredAuthentications=publickey`, `BatchMode`); gateway token via the existing secret chain |
| V3 Session Management | no | stateless CLI |
| V4 Access Control | yes | Read-only verbs by construction (trait split + `ForbidWrites`); delete set bounded by allowlist; `svc-ignition-deploy` least privilege (C6) |
| V5 Input Validation | yes | `safe_remote_path`, `validate_segment`, `component_problem`, strict ledger/YAML/JSON parsers with size caps |
| V6 Cryptography | yes | Host key pinning via OpenSSH; sha256 via `sha2`; nothing hand-rolled |
| V7 Error Handling / Logging | yes | value-free errors; capped, scrubbed stderr; no log-line echo |
| V12 Files | yes | `symlink_metadata` everywhere, 0700/0600 temp, no files in the collection, per-file and total size caps |

### Known Threat Patterns for this stack
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Command injection through a path into an sftp batch or PowerShell | Tampering / EoP | argv-only; `safe_remote_path`; no remote-derived strings; base64 JSON parameters |
| Host-key MITM | Spoofing | `StrictHostKeyChecking=yes` + repo `known_hosts` copy + `GlobalKnownHostsFile=/dev/null` |
| Malicious host returns hostile names (`..`, symlinks) in a recursive download | Tampering | sftp client name checks, `get -R` does not follow links [CITED: man page], local walk refuses symlinks and non-UTF-8 |
| Tampered ledger widens deletes | Tampering | strict reader, gateway+mode check, allowlist and `config-mode.json` filters, D-07 refusal |
| Secret ciphertext in `--diff`, verify, stderr | Information disclosure | withhold diffs with secret locations; never echo log lines/tag values; canary tests |
| Private key left on disk / in argv | Information disclosure | 0700 temp dir + 0600 file, drop-deletes; key only by path in `-i` |
| Decompression/size DoS from a hostile host | DoS | per-file (`JSON_MAX_BYTES` class) and total caps, entry-count cap |
| Silent partial read -> wrong plan -> bad apply | Tampering | Pattern 3; `plan_sha256` binds apply to what was reviewed |

## Sources

### Primary (HIGH confidence)
- In-repo source read this session: `deploy/{mod,target,manifest,layout,tree,render,rules,lint}.rs`, `actions/deploy.rs`, `client/{deploy_api,scan,logs,config_resources}.rs`, `error.rs`, `session.rs`, `rig/compose.rs`, `main.rs` (GUARDED_OPS, verdict hook), `cli.rs`, `mcp.rs`, `routes.rs`, `tests/{deploy_family_coverage,contract_deploy}.rs`, `tests/common/fake_deploy.rs`, `.github/workflows/ci.yml`, `Cargo.toml`
- `.planning` docs: 22-CONTEXT, ROADMAP v1.6 + Phase 22, REQUIREMENTS, STATE, research SUMMARY/ARCHITECTURE/STACK/PITFALLS/FEATURES, 20-LIVE-CAPTURES (Decisions D1-D12), 21-LIVE-CAPTURES (CD1-CD7), 21-SECURITY, 20 COVERAGE
- Local experiments this session: OpenSSH_10.3p1 `ssh -G` option sweep; `sftp -D /usr/libexec/sftp-server -b -` (abort/exit codes, `-` prefix, `ls -1` format, spaces, glob); `ssh-keygen -y` with no-newline and CRLF keys
- Context7 `/websites/rs_tokio_tokio`: `Command::kill_on_drop`, `Child::wait_with_output`
- Design spec (PR #60, `docs/superpowers/specs/2026-10-09-prod-gateway-config-as-code-design.md`) and `spikes/gateway-modes/results/RIG.md`, fetched with `gh api`

### Secondary (MEDIUM confidence)
- man.openbsd.org/sftp.1 (batch, `-`/`@` prefixes, `-P`, `-D`, `get -R`)
- WebSearch: Win32-OpenSSH `/C:/` path form (WinSCP FAQ, curl commit); openssh-bugs Bug 2965 (stdin NUL hang)

### Tertiary (LOW confidence)
- PowerShell CLIXML-on-stderr prelude (general knowledge, partially corroborated); all Win32-OpenSSH server behaviors (INFERRED until C7a)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH, one already-locked crate promoted, everything else existing
- Architecture: MEDIUM-HIGH for plan/pull/verify mechanics; MEDIUM for the effective-view basis (Open Question 1)
- Transport (Unix side): HIGH, tested locally; Windows side: LOW-MEDIUM, capture-gated
- Pitfalls: MEDIUM-HIGH

**Research date:** 2026-10-14
**Valid until:** 2026-11-13 (30 days; sooner if C7a/C6 results land first, which supersede the Windows items)
