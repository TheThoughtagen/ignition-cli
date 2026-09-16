---
sidebar_position: 10
---
# Troubleshooting

## No active profile

Run `ign profile list`, then `ign profile use dev` with the intended profile name. Check that the environment variable referenced by the profile is set in the shell running `ign`.

## Authentication rejected

Run `ign doctor`. For an Ignition 8.3 native REST endpoint, use the full API token in `name:key` form. A 401 indicates rejected authentication. A 403 indicates that the token was recognized but the requested access was refused. Check the token's security level, gateway permissions, and HTTPS requirements.

## WebDev routes are missing

Tag value/config verbs refuse with `routes_not_deployed` until the CLI's routes are on the gateway: `ign webdev deploy`. If they refuse with `route_version_mismatch`, your deployed routes are older than the CLI — re-run `ign webdev deploy` (never pin an old CLI version to match stale routes). `ign webdev status` shows the per-route version handshake.

## MCP or LSP transport misbehaves

After any upgrade, make sure the client launches the NEW binary: PATH resolves the installed `~/.cargo/bin/ign`, not a repo build — run `ign --version` exactly as the client would see it, and re-run `cargo install ignition-cli` after transport-affecting changes. `ign mcp serve` clients that see connection-closed on startup are almost always launching a stale pre-transport binary.

The CLI requires its own route bundle for runtime tags, alarms, and history. Read the WebDev section of the [reference](https://thethoughtagen.github.io/ignition-cli/docs/reference/commands/) before running `ign webdev deploy`. Deploying replaces the CLI-owned route project.

## Import or resource update behaves unexpectedly

Some resource edits use an export / overwrite-import cycle for the whole project. Concurrent Designer changes can be replaced. Read the command's operation notes and inspect a development copy before applying the operation to shared work.

For a bug report, include `ign version`, the subcommand, gateway version, and a redacted error. Open an [issue](https://github.com/TheThoughtagen/ignition-cli/issues).
