---
name: ign-contract
description: The ign (ignition-cli) agent contract — frozen JSON envelope, exit-code taxonomy, guarded writes, output modes, and the three surfaces (CLI/MCP/LSP). Use when scripting ign commands, parsing ign output, or driving gateways non-interactively.
user-invocable: false
---

# ign Agent Contract

`ign` is a single Rust binary that operates and inspects Ignition 8.3+ gateways. It is built agent-first: every subcommand is non-interactive by default and machine-readable. This skill covers the rules that make scripting it safe and predictable.

## Platforms

- **macOS** (Apple Silicon + Intel) and **Linux** (x86_64 + arm64). There is **no Windows build** (locked project decision).
- The binary is self-contained; `jq` is optional and only needed for the filtering examples below. All examples are POSIX shell (bash/zsh).

## Install & dependencies

Before any ign task, probe what the task needs and install only what is missing:

| Dep | Probe | Needed for | Install if missing |
|-----|-------|-----------|--------------------|
| `ign` ≥ 1.1 | `command -v ign >/dev/null && ign --version` | everything | see below |
| `jq` | `command -v jq` | envelope filtering in the examples (optional) | `brew install jq` / `apt-get install -y jq` |
| docker + compose v2 plugin | `docker compose version` | `ign rig` verbs only | Docker Desktop (macOS) / Docker Engine (Linux) — compose **≥ v2 required**; the legacy `docker-compose` v1 binary is unsupported (rig verbs fail fast, exit 7 + install hint) |
| Rust 1.88+ toolchain | `cargo --version` | building `ign` from source only | rustup — skip entirely if using the tarball path |
| gateway profile | `ign profile list` | every gateway-touching verb | `ign profile add` (config, not install — see Profiles & auth below) |

### Installing `ign`

```bash
# 1. Probe
command -v ign >/dev/null 2>&1 && ign --version

# 2. Install — exactly ONE path, chosen by probe (never run both)
if command -v cargo >/dev/null 2>&1; then
  # 2a. Preferred: build from source (needs Rust 1.88+ toolchain)
  cargo install ignition-cli
else
  # 2b. Fallback: prebuilt release tarball (no Rust toolchain needed)
  #     Asset format (pinned by .github/workflows/release.yml):
  #     ign-<tag>-<target>.tar.gz — the unversioned latest/download name 404s
  DEST="${HOME}/.local/bin"          # user-owned, no sudo — ensure it is on PATH
  case "$(uname -sm)" in             # CI target triple
    "Darwin arm64")   T="aarch64-apple-darwin" ;;
    "Darwin x86_64")  T="x86_64-apple-darwin" ;;
    "Linux x86_64")   T="x86_64-unknown-linux-gnu" ;;
    "Linux aarch64")  T="aarch64-unknown-linux-gnu" ;;
    *) echo "unsupported host: $(uname -sm) (no Windows build)"; exit 1 ;;
  esac
  TAG=$(curl -fsSLI -o /dev/null -w '%{url_effective}' \
        https://github.com/TheThoughtagen/ignition-cli/releases/latest \
        | grep -o 'tag/[^/]*$' | cut -d/ -f2)   # resolves the release tag, jq-free
  mkdir -p "$DEST"
  curl -fsSL "https://github.com/TheThoughtagen/ignition-cli/releases/download/${TAG}/ign-${TAG}-${T}.tar.gz" \
    | tar xz -C "$DEST" --strip-components=1    # archive wraps the binary in ign-<tag>-<target>/
  # system-wide instead? /usr/local/bin needs root: sudo install -m755 "$DEST/ign" /usr/local/bin/
fi

# 3. Verify
ign --version
```

Rules:

- The tag resolution + target triple above match the release workflow's naming — if `ign --version` 404s or the download fails, check the actual release assets for a renamed format before improvising.
- `.sha256` companions are published per tarball; verify with `shasum -a 256 -c` when the integrity matters (rigs handling licensed gateways).
- If `cargo` is absent and there is no network path to GitHub releases, stop and tell the user; do not improvise alternate install paths.
- Docker/rig work on Apple Silicon runs linux/amd64 images under Rosetta — set `DOCKER_DEFAULT_PLATFORM=linux/amd64` if compose doesn't pin it (see ign-rigs).
- If an `ign` binary exists but `ign --version` is older than the WebDev routes on the target gateway, the versioning discipline below (route mismatch) is the guide — never pin an old CLI.

## Three surfaces, one contract

| Surface | Invocation | Consumers |
|---------|-----------|-----------|
| CLI JSON | `ign <verb> --json` (default: human table on stdout) | Scripts, agents |
| MCP | `ign mcp serve` (stdio JSON-RPC 2.0) | MCP clients — 83 tools derived from the CLI's own argument parser |
| TUI | `ign tui` | Humans only — full cockpit, never scripted |

Every CLI verb's MCP twin exists automatically (catalog derives from the clap tree with a CI parity test), so anything documented for the CLI is drivable over MCP.

## The frozen envelope

With `--json` (or `--compact`, which implies it), every command emits one of:

```
Success: {"ok": true,  "profile": <name|null>, "data": {...}}
Failure: {"ok": false, "profile": <name|null>, "error": {"code": "<slug>", "message": "...", "endpoint": <url|null>, "hint": "..."}}
```

Rules:

- Parse `ok`, then branch. `error.code` is a **stable slug** — key automation on slugs, never on message prose (messages may improve between releases).
- **Streams matter**: a successful dispatch writes the envelope to **stdout**; a failed dispatch writes the failure envelope to **stderr** (`render_error`'s JSON branch) — the exit code carries the same signal either way. Automation must read `error.code` / `error.message` from **stderr** when the exit code is nonzero; stdout stays empty on failure.
- **Exceptions** (raw stdout, never JSON-wrapped): `ign completions <SHELL>` (sourced by shells), `ign tags export --format xml -- -` and CSV/XML file payloads (byte-faithful gateway passthrough), and `ign rig logs` (streams compose log lines directly to stdout in both default and `--json` forms — never parse them as JSON), plus `ign edit` (zero stdout — the editor owns the terminal).

## Exit codes

| Code | Class | Meaning |
|------|-------|---------|
| 0 | ok | success |
| 1 | internal | unexpected failure — report as a bug |
| 2 | usage | clap usage error, **destructive op without `--yes`**, invalid import file, or an `api call` the gateway rejected with an unclassified 4xx |
| 3 | config | local configuration problem (profile not found, no active profile, secret unavailable, config invalid) |
| 4 | network | gateway unreachable / timeout / TLS |
| 5 | auth | gateway rejected credentials |
| 6 | target_state | command invalid for gateway's current state (`not_found`, `eam_not_controller`, `routes_not_deployed`, `bundle_not_available`, …) |
| 7 | rig | docker/compose rig failure |

Scripts should branch on exit class; the `error.code` slug disambiguates within a class.

## Guarded writes

Destructive operations are **refused with exit 2** unless `--yes` is passed (CLI) or `confirm: true` is set (MCP tool call — the field is deliberately optional so agents cannot auto-fill it).

- The refusal message **is** the blast-radius preview (target, scope, controller impact) — read it before deciding.
- Some guards are stronger than `--yes`: workspace `push` **conflicts refuse even with `--yes`** (manual reconciliation required); `ign edit` staleness refusals are never force-able; `ign script` requires a deploy-time secret (structural opt-in, no `--yes` exists).
- Gateway-state gates are honest refusals, not bugs: `eam_not_controller` (exit 6) means the target gateway is not an EAM controller.

## The escape hatch: `ign api call`

For any REST endpoint `ign` doesn't curate:

```bash
ign api call --method GET --path /data/status/statusinfo --json
ign api call --method POST --path /some/endpoint --data '{"k": "v"}' --json
```

- The envelope's `data` is **gateway-verbatim** (raw passthrough, never re-serialized) — the one documented envelope exception.
- User-supplied auth-pattern headers are refused (the CLI owns auth).
- An unclassified 4xx lands as exit 2 / `gateway_client_error` with the gateway's body carried verbatim (up to 4 KiB) — never an exit-1 storm.

## Profiles & auth

- `--profile <NAME>` selects a gateway; default is the active profile in config. `ign profile` manages them.
- Secrets resolve env-first (`IGNITION_USER` / `IGNITION_PASSWORD`), then OS keyring.
- `IGNITION_PROFILE` env var sets the profile for a whole session.

## MCP & LSP one-liners

```bash
ign mcp serve          # register in any MCP client as command: ign, args: ["mcp", "serve"]
ign lsp                # LSP for ignition-nvim / editors — TTL-cached gateway truth
```

## Versioning discipline

Before scripting against a gateway, confirm the CLI's WebDev routes are deployed and version-matched: `ign webdev status` reports per-route `{deployed_version, expected_version}`. A stale deploy refuses with `route_version_mismatch` (exit 6) — fix with `ign webdev deploy`, never by pinning an old CLI.
