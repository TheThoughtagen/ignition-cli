---
sidebar_position: 1
---
# Install ignition-cli

The executable is named `ign`. It targets Ignition 8.3+ gateways.

Download a binary for your platform from [GitHub releases](https://github.com/TheThoughtagen/ignition-cli/releases), or install from crates.io with Rust 1.88+:

```sh
cargo install ignition-cli
ign --help
```

Release assets cover macOS arm64 / x86_64 and Linux arm64 / x86_64 (there is no Windows build). Read the selected release's notes before updating.

## Agent harnesses

Install the playbook alongside the binary — five skills (contract, gateway, tags, workspace, rigs) for any [skills-CLI](https://github.com/vercel-labs/agent-skills)-compatible agent:

```sh
npx skills add TheThoughtagen/ignition-cli -g
```

Claude Code can alternatively load them as a plugin: `/plugin marketplace add TheThoughtagen/ignition-cli` → `/plugin install ignition-ops@ignition-cli`.

Installation is interactive by default (source → skills → agents → scope) and always explicit — nothing installs itself, and the skills run only in agents you choose. Already installed? Re-running the same command updates in place (same paths, no duplicates), or use `npx skills update`.

The command reference on this site follows the repository's main branch. Use `ign --help` and each subcommand's `--help` to check the interface available in your installed release.

Continue with [a gateway profile](quickstart.md).
