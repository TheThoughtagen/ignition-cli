<!-- GSD:project-start source:PROJECT.md -->

## Project

**ignition-cli**

A Rust CLI and ratatui TUI cockpit for Ignition by Inductive Automation gateways — a unified, agentic-friendly developer tool that replaces the gateway webpage and minimizes Designer usage. It wraps gateway REST + WebDev APIs for health checks, project operations, and tag operations, plus Docker test-rig lifecycle control. The TUI is the full cockpit for humans; every command is also scriptable with JSON output for AI agents.

**Core Value:** One binary that lets a developer (or an AI agent) fully operate and inspect an Ignition 8.3+ gateway — health, projects, tags, rigs — without opening the gateway webpage or Designer.

### Constraints

- **Tech stack**: Rust + ratatui for TUI; clap for CLI — keep the dependency tree lean
- **Compatibility**: Ignition 8.3.1+ only (matches ignition-git-module v2 support policy)
- **Agentic usage**: every subcommand must be non-interactive by default with `--json` output; TUI is opt-in via subcommand/flag
- **Simplicity**: "simple but complete" — avoid framework creep; single binary, no daemon required
- **Pairing**: config conventions should interoperate with WHK-Global and git-module rigs (compose file discovery, env/secrets)

<!-- GSD:project-end -->

<!-- GSD:stack-start source:research/STACK.md -->

## Technology Stack

## Recommendation Summary

| Layer | Choice | Version | Confidence |
|-------|--------|---------|------------|
| Workspace layout | Cargo workspace: `ignition-cli` (bin) + `ignition-core` (lib) + `ignition-tui` (lib), `webdev/` payload crate/dir | cargo 1.85+ | HIGH |
| Rust edition / MSRV | Edition 2024, `rust-version = "1.85"` | 1.85 | HIGH |
| CLI framework | `clap` (derive API) + `clap_complete` | 4.6.6 / 4.6.9 | HIGH |
| Serialization / JSON output | `serde` + `serde_json` | 1.0.229 / 1.0.151 | HIGH |
| Async runtime | `tokio` (rt-multi-thread, macros, time, process, signal) | 1.53.1 | HIGH |
| HTTP client | `reqwest` (default rustls TLS, `json` feature) | 0.13.4 | HIGH |
| TUI | `ratatui` (umbrella crate → ratatui-core 0.1.2 + ratatui-crossterm 0.1.2 → crossterm 0.29) | 0.30.2 | HIGH |
| Docker orchestration | Shell out to `docker compose` (Compose v2 CLI plugin) via `tokio::process::Command` | n/a (host tool) | HIGH |
| Config / profiles | Hand-rolled: `toml` + `serde` + `directories` | 1.1.4 / 6.0.0 | HIGH |
| Secrets | `keyring` (default `v1` feature: classic `Entry` API) + env-var fallback | 4.1.6 | MEDIUM |
| Error handling | `thiserror` everywhere + `std::process::ExitCode` in `main` | 2.0.20 | HIGH |
| Logging | `tracing` + `tracing-subscriber` (env-filter) [+ `tracing-appender` for TUI mode] | 0.1.44 / 0.3.23 / 0.2.5 | MEDIUM |
| Time (only if CLI must format/parse timestamps) | `jiff` | 0.2.35 | MEDIUM |
| HTTP mock tests | `wiremock` | 0.6.5 | HIGH |
| CLI integration tests | `assert_cmd` + `predicates` + `tempfile` | 2.2.2 / 3.1.4 / 3.27.0 | HIGH |
| Progress bars (CLI mode only, optional) | `indicatif` | 0.18.6 | LOW (defer) |

## Detailed Choices

### 1. Project Scaffolding — Cargo Workspace, Three Crates

- **`ignition-core` is the seam.** Both front-ends (clap dispatch and TUI) call the same `GatewayClient` + command functions. This makes "every CLI action available in TUI" structural rather than aspirational, and gives wiremock-based tests one surface to target.
- **`ignition-tui` separate from the bin** keeps the binary's non-TUI path compile-clean and lets you feature-gate the TUI (`[features] default = ["tui"]`) for a smaller headless/agent build (`--no-default-features`). Ratatui + crossterm pull a real dependency subtree; agents never render it.
- **`webdev/` in-repo** because tag write/alarm/script endpoints are the CLI's own WebDev backend (per Key Decision). Keep the Python route sources and their JSON request/response serde types versioned together — a shared payload crate (`webdev` types inside `ignition-core`, or a 4th tiny crate if the Python side wants to consume a JSON schema later).
- **`workspace.dependencies`** for single-source-of-truth versions across crates.
- *Single crate* — the TUI↔CLI↔client separation is what keeps the "simple but complete" constraint enforceable; single crates rot into module soup at this scope.
- *xtask* — not needed until there are repo maintenance tasks cargo scripts can't do; add later if wanted.
- *Workspace member per subcommand domain* — framework creep; three crates is the maximum justified by boundaries.

### 2. CLI Framework — clap 4.6 (derive)

- Derive-API subcommands map 1:1 onto the domain (`gateway status`, `project list`, `tag read`, `rig up`, `tui`), generate help/completions for free, and `#[command(propagate_version = true)]` gives every subcommand `--version`.
- Global args (`#[arg(global = true)]`) are exactly what `--json`, `--profile`, `--verbose` need — available on every subcommand without repetition.
- clap's default exit code for usage errors is `2`, distinct from runtime failures (`1`+) — the exit-code contract (below) builds on this.
- It is *the* Rust CLI standard (docs.rs/clap sustainably ~10M+ downloads/mo; maintained under clap-rs org with active 2026 releases — 4.6.6 released 2026-08-06).
- `argh`, `argh_derive` — Google-internal conventions, thinner ecosystem, no completion generation story to match clap_complete.
- `bpaf` — clever combinators, smaller community; nothing here needs it.
- Builder API (vs derive) — more code for identical behavior; derive is the community default.

### 3. JSON Output Mode — serde_json on every subcommand

- The `--json` contract is a *project requirement* (agentic usage). Concretely:
- serde/serde_json are compile-time codegen with zero runtime reflection and are already required by reqwest's `json` feature — no extra tree cost.

### 4. Async Runtime — tokio 1.53

- reqwest is async-on-tokio; ratatui integrates via the standard `tokio::select!` event loop (terminal events + IPC channels + tick). One runtime for HTTP, subprocess, and timeouts.
- `process` feature drives `docker compose` asynchronously (`Command::output().await`, streaming `stdout`/`stderr` for `compose up --wait` / `logs -f` into the TUI without blocking the render loop).
- `signal` supports Ctrl-C handling in long-running rig/TUI flows.
- tokio 1.x is a 6-year-stable LTS line (1.53.1, July 2026); no alternative is credible here.
- `async-std` — effectively dormant; reqwest doesn't target it; would require compat shims.
- `smol` — lovely, wrong network effects for this stack.
- Blocking everywhere (no runtime) — reqwest's blocking client can't drive a ratatui event loop or stream compose output; you'd fight it within the first TUI phase.

### 5. HTTP Client — reqwest 0.13 (rustls default)

- One client, cloned across requests, base-URL'd per gateway profile — mirrors the proven `httpx.AsyncClient` shape in ignition-mcp's `IgnitionClient` (the reference implementation this CLI replaces).
- **0.13 is a real release line with breaking changes you must know up front** (verified from the official CHANGELOG):
- Timeouts: set explicit `connect_timeout` + request `timeout` (ignition-mcp uses 30s; keep parity).
- `ureq` (blocking) — fine for pure CLIs, wrong when the TUI needs concurrent requests + streaming output.
- `hyper` directly — you'd rebuild reqwest.
- `surf`/`isahc` — stagnant; no reason.
- `reqwest-middleware` — retries/backoff can be added at the `GatewayClient` level in ~20 lines; skip the extra layer for v1.

### 6. TUI — ratatui 0.30 (umbrella crate)

- **0.30 (Dec 2025) is ratatui's biggest release** (verified from the repo CHANGELOG): modularized architecture (`ratatui-core`, `ratatui-crossterm` 0.1.2 wrapping **crossterm 0.29** — both verified via crates.io dependency API), `no_std` core, stabilized style system, `ratatui::run(...)` convenience entry, plus 0.30.x polish (Block shadows, scrollbar fixes; 0.30.2 current, June 2026).
- Depending on the **umbrella `ratatui` crate** is the supported path; sub-crates (`ratatui-core`, `ratatui-crossterm`) are for special needs (custom backend version pinning) — don't reach for them directly here.
- Async pattern is standardized (ratatui's own async template): `tokio::select!` { crossterm::event::read (via a blocking-task→channel bridge or the EventStream pattern), mpsc receiver of core results, interval tick } → update model → draw. Every CLI action is a `ignition-core` call spawned as a task; the TUI is a shell over the same commands the CLI dispatches.
- Project constraint says ratatui explicitly; this section is about *how*, not *whether*.
- `tuirealm` 4.1 (formerly `tui-realm`) — an Elm-architecture component framework over ratatui. Active (May 2026), but it's exactly the "framework creep" the project constraints rule out; its stdlib widget set adds tree weight and its abstraction fights you on custom gateway-centric views (tag trees, log tail panes).
- `ratatui-*` third-party widget mega-crates — pull what's needed (Table, Tabs, List, Paragraph, Gauge) from core; add `tui-widgets` (the official extra-widgets repo) later only if a specific widget is missing.
- termion backend — Unix-only; crossterm is cross-platform and the ratatui default.
- `crossterm` as a direct dependency — access events through `ratatui`'s re-export so the crossterm version stays pinned to what `ratatui-crossterm` expects (0.29).

### 7. Docker Orchestration — shell out to `docker compose` (Compose v2 CLI plugin)

- **Rigs are compose files** — the git-module `docker/` directory (compose files, gw-build, gw-init, test-rig) is the declared pattern source, and WHK-Global interop is a project constraint. Compose semantics (`depends_on`, healthchecks, `env_file`, profiles, build args, `--project-directory`) are a *large, moving surface* that the `docker compose` plugin already implements and the ecosystem (including git-module rigs) is tested against.
- **Structured output exists**: `docker compose ps --format json` emits JSON Lines per container (verified in Docker's official command reference) — parses cleanly with serde_json for both TUI status panes and `--json` CLI output.
- **No extra dependency**: Docker is a prerequisite for rigs anyway; the CLI shells out to a tool that must already be installed. Streaming `compose up --wait` / `logs -f` output maps naturally onto tokio `Child` stdout.
- **bollard-compose does not exist** (verified: crates.io search returns nothing; bollard itself has no compose support — it's a Docker *daemon API* client). Using bollard for rigs would mean reimplementing compose file parsing/merge/interpolation and lifecycle ordering — weeks of work to re-create a CLI that's already installed.
- `bollard` 0.21.1 (active, Aug 2026) — **defer, not rejected**: the right tool if v2 needs daemon-level introspection (streaming container events, stats, image pulls with progress). Re-evaluate when a feature actually wants it; keep it out of v1's tree.
- `compose-spec` parsing crates — only relevant if you ever generate/validate compose files programmatically; rigs already have files.
- Shelling to legacy `docker-compose` (Python v1) — dead; require Compose v2 (`docker compose`, check with a version probe and fail with a clear error).

### 8. Config / Profiles — plain TOML files

- Profiles are small, typed, and few (dev/test/prod gateways + rig dirs + auth refs). A `[profile.dev]` table → `GatewayProfile` struct is ~50 lines total; no framework needed — matches the "simple but complete, lean tree" constraint.
- `directories` gives correct XDG/BaseDir behavior on macOS/Linux without hand-rolling (project owner is on macOS; `~/Library/Application Support` handled).
- **`toml` 1.0 finally shipped (2026-02-11), now 1.1.4** (verified from toml-rs changelog) — the parser is stable at semver 1.0 with TOML 1.1 parse support. MSRV 1.85 (hence the edition-2024 recommendation above).

# ~/.config/ignition-cli/config.toml

# secrets NEVER in this file: resolved via keyring or IGNITION_DEV_TOKEN etc.

- `config` 0.15 — actively maintained but pulls a broad dependency surface (many format backends) for a problem that is one file format + env overlay; the overlay logic is ~20 lines of `std::env` matching.
- `figment` 0.10 — last release May 2024 (verified on crates.io); stale for a new greenfield choice.
- JSON/YAML config — TOML is the Rust-native convention, human-editable, comments supported; YAML's tree and ambiguity buy nothing here.

### 9. Secrets — keyring 4.1 + env fallback

- API tokens for prod gateways shouldn't live in plaintext TOML. `keyring::Entry::new("ignition-cli", "profile:prod")` maps 1:1 to profiles; macOS Keychain is the owner's daily driver (Linux secret-service supported too).
- **keyring v4 was re-architected** (repo: `open-source-cooperative/keyring-rs`, verified from GitHub releases): v4.0 (Apr 2026) moved to `keyring-core` and briefly became a "sample app"; **v4.1.0 (Jun 2026) restored the v1-style standalone `Entry` API as the default feature** and stripped the CLI example's deps out — so `keyring = "4.1"` default features is both lean and the familiar API. Active maintenance (4.1.6, Aug 2026).
- Env fallback (`IGNITION_TOKEN_<PROFILE>` / `IGNITION_TOKEN`, `IGNITION_USER`/`IGNITION_PASSWORD`) keeps CI and agents working without a keychain, mirroring ignition-mcp's settings pattern.
- Confidence MEDIUM: v4.1's default-`Entry` behavior verified from release notes but not compiled against; pin `4.1` and smoke-test in the config phase. (If the default store surprises on Linux CI, mark keyring tests `#[cfg(target_os)]`-gated.)
- Plaintext secrets file with `0600` — works, but prod gateway tokens in dotfiles is exactly what leaks; keyring is one small crate.
- `secrecy` crate wrapping — useful hygiene (`SecretString`) but can be added when the config layer lands; not a v1 blocker. (Mention: consider `zeroize`-backed `secrecy` for in-memory token strings — optional polish.)

### 10. Error Handling & Exit Codes — thiserror + std::process::ExitCode

| Code | Meaning | Examples |
|------|---------|----------|
| 0 | success | — |
| 1 | general runtime failure | unexpected internal error |
| 2 | usage error (clap default) | bad flags, missing args |
| 3 | connection failure | gateway unreachable, timeout |
| 4 | auth failure | 401/403, bad token |
| 5 | gateway state error | project not found, rig not running |
| 6 | docker/rig failure | `docker compose` nonzero, docker missing |

- `thiserror` (2.0.20, Aug 2026) gives typed variants that carry structured details for the JSON error envelope (`code` slug + message + optional details), and `#[from]` conversions from reqwest/serde/io.
- `std::process::ExitCode` (stable since Rust 1.61) makes `main` return codes cleanly; the mapping lives in one `impl From<&CliError> for ExitCode`.
- Stable slugs/codes documented alongside `--json` output = the agentic contract. Divergence = breaking change.
- `exitcode` crate — frozen since **2017** (verified on crates.io); a const-module adds nothing over `ExitCode`.
- `anyhow` as the primary error type — great for prototypes, but the `--json` envelope needs typed variants anyway; ad-hoc context strings fight the stable-slug requirement. (Using anyhow inside TUI-internal glue would be acceptable; simplest is to not need it.)
- `miette` 7.6 / `color-eyre` 0.6 — rich diagnostic rendering (spans, snippets) is overkill for a gateway client whose errors are mostly HTTP statuses and compose stderr; both add tree weight and opinionated output that conflicts with the JSON envelope.

### 11. Logging / Diagnostics — tracing (optional but recommended)

### 12. Time — jiff, only if needed

### 13. Testing Stack

| Tool | Version | Use |
|------|---------|-----|
| `wiremock` | 0.6.5 | Mock the gateway `/data` + `/webdev` endpoints for `GatewayClient` tests (status JSON, auth failures, WebDev payloads) |
| `assert_cmd` | 2.2.2 | Binary-level CLI tests incl. `--json` output shape and exit-code contract |
| `predicates` | 3.1.4 | Assertions for assert_cmd |
| `tempfile` | 3.27.0 | Isolated config/profile dirs in tests |
| `mockall` | 0.15.0 | Only if trait-mocking is wanted beyond wiremock; optional |

### 14. Rust Edition & MSRV

## What NOT to Use (summary table)

| Rejected | Reason |
|----------|--------|
| `async-std` / `smol` | Dormant/niche; reqwest + ecosystem are tokio-native |
| `ureq` | Blocking-only; incompatible with async TUI + streaming |
| `tuirealm` (ex tui-realm) | Framework creep (explicit project constraint); smaller ecosystem |
| `bollard` (v1) | No compose support — reimplementing Compose semantics is a trap; revisit for daemon introspection in a later milestone |
| `bollard-compose` | **Does not exist** (verified crates.io, 2026-08) |
| `config` / `figment` | Heavy / stale respectively; profiles need ~100 lines of serde+toml+env |
| `anyhow`-as-primary | Fights the stable JSON error-slug contract |
| `exitcode` crate | Unmaintained since 2017; `std::process::ExitCode` exists |
| `miette` / `color-eyre` | Diagnostic theater for a non-source-language tool; conflicts with JSON envelope |
| `chrono` | Prefer `jiff` if/when datetime is actually needed |
| `indicatif` in TUI | TUI progress = ratatui Gauge/Paragraph; indicatif only ever for CLI-mode long ops (defer) |
| reqwest `cookies` feature | Auth is header-based (API token / Basic) per reference impl; add only if live-gateway testing proves a session-cookie endpoint |

## Installation (workspace skeleton)

# Root Cargo.toml (virtual manifest)

# dev

## Verification Notes

| Claim | Source | Date checked |
|-------|--------|--------------|
| All crate versions (clap 4.6.6, ratatui 0.30.2, reqwest 0.13.4, tokio 1.53.1, serde 1.0.229, serde_json 1.0.151, thiserror 2.0.20, bollard 0.21.1, toml 1.1.4, keyring 4.1.6, directories 6.0.0, tracing 0.1.44/0.3.23, jiff 0.2.35, wiremock 0.6.5, assert_cmd 2.2.2, tempfile 3.27.0, predicates 3.1.4, indicatif 0.18.6, clap_complete 4.6.9, crossterm 0.29.0, ratatui-core/-crossterm 0.1.2) | `crates.io/api/v1/crates/{name}` (`max_stable_version`) via curl+jq | 2026-08-21 (UTC) |
| reqwest 0.13 breaking changes (rustls default, aws-lc, `query`/`form` feature-gated, cookies feature, `danger_accept_invalid_certs`) | Official CHANGELOG fetched from `github.com/seanmonstar/reqwest` (raw master) | 2026-08-21 |
| ratatui 0.30.0 = "biggest release", modular split (ratatui-core, ratatui-crossterm, etc.), 0.30.1/0.30.2 contents, crossterm 0.29 support | Official repo CHANGELOG (raw, main) + crates.io dependency API for `ratatui-crossterm 0.1.2` (shows optional crossterm ^0.28/^0.29) | 2026-08-21 |
| `bollard-compose` does not exist; `tuirealm` renamed from tui-realm, 4.1.0 active | crates.io search API (`?q=bollard-compose` → zero results; `?q=tui-realm` → `tuirealm` 4.1.0, 2026-05) | 2026-08-21 |
| keyring v4.0 re-architecture + v4.1.0 restoring default `v1` `Entry` API, active maintenance | GitHub releases for `open-source-cooperative/keyring-rs` (fetched full release list) | 2026-08-21 |
| `toml` 1.0.0 (2026-02-11) / 1.1.4 / MSRV 1.85 / TOML 1.1 parsing | Official `toml-rs/toml` crate CHANGELOG (raw, main) | 2026-08-21 |
| `docker compose ps --format json` JSON-Lines output | Official Docker command reference (docs.docker.com/reference/cli/docker/compose/ps/) | 2026-08-21 |
| Ignition 8.3 auth = `X-Ignition-API-Token` header preferred, Basic fallback, `ssl_verify` option, no cookie jar | Author's reference implementation `~/whiskeyhouse/ignition-mcp/src/ignition_mcp/ignition_client.py` (read directly) + `83-api` Bruno collection (`api-token/`, `config-api-token/` sections incl. `POST /data/api/v1/api-token/generate`) | 2026-08-21 |
| clap derive/subcommand/global-arg patterns | Context7 `/websites/rs_clap` (docs.rs cookbook/derive tutorial) | 2026-08-21 |
| `exitcode` crate unmaintained (2017) | crates.io `updated_at` | 2026-08-21 |
<!-- GSD:stack-end -->

<!-- GSD:conventions-start source:CONVENTIONS.md -->

## Conventions

Conventions not yet established. Will populate as patterns emerge during development.
<!-- GSD:conventions-end -->

<!-- GSD:architecture-start source:ARCHITECTURE.md -->

## Architecture

Architecture not yet mapped. Follow existing patterns found in the codebase.
<!-- GSD:architecture-end -->

<!-- GSD:skills-start source:skills/ -->

## Project Skills

No project skills found. Add skills to any of: `.claude/skills/`, `.agents/skills/`, `.cursor/skills/`, `.github/skills/`, or `.codex/skills/` with a `SKILL.md` index file.
<!-- GSD:skills-end -->

<!-- GSD:workflow-start source:GSD defaults -->

## GSD Workflow Enforcement

Before using Edit, Write, or other file-changing tools, start work through a GSD command so planning artifacts and execution context stay in sync.

Use these entry points:

- `/gsd-quick` for small fixes, doc updates, and ad-hoc tasks
- `/gsd-debug` for investigation and bug fixing
- `/gsd-execute-phase` for planned phase work

Do not make direct repo edits outside a GSD workflow unless the user explicitly asks to bypass it.
<!-- GSD:workflow-end -->

<!-- GSD:profile-start -->

## Developer Profile

> Profile not yet configured. Run `/gsd-profile-user` to generate your developer profile.
> This section is managed by `generate-claude-profile` -- do not edit manually.
<!-- GSD:profile-end -->
