---
sidebar_position: 2
---
# Inspect a gateway

[Watch the recorded walkthroughs](demos.md) to see the tools in use.

Start with a commissioned development gateway running Ignition 8.3+ and an API token authorized to read its configuration.

Set `IGNITION_TOKEN` in your shell or secret manager to the full `name:key` token issued by the gateway. Avoid putting the token itself into a profile command or committing it to a repository.

```sh
ign profile add dev http://localhost:8088 --token-env IGNITION_TOKEN --active
ign profile list
ign doctor
ign status
ign project list
ign logs --limit 20
```

Replace the URL with your development gateway's address. `--token-env` stores the name of the environment variable. It does not store the token value in the profile.

## Read structured output

```sh
ign --json status
```

Successful commands normally return an envelope containing `ok`, `profile`, and `data`. See the [reference](https://thethoughtagen.github.io/ignition-cli/docs/reference/output-contract-for-agents) for errors and streaming exceptions.

`ign doctor` can finish with exit code zero while reporting failed checks. Read the checks themselves before assuming the connection works.

## The daily check

One pass over gateway health, all curated reads:

```sh
ign status
ign license status
ign redundancy status
ign gan status
```

For anything the curated commands don't cover, `ign api call --method GET --path /some/endpoint` passes through to the gateway verbatim.

## Add other operations when needed

Gateway status and project listing use native REST endpoints. Runtime tag operations need the CLI's WebDev routes. Review those prerequisites and the overwrite behavior of project operations in the [reference](https://thethoughtagen.github.io/ignition-cli/docs/reference/commands/) before using them.

## Try the editing loop

Projects check out to a real directory tree and push back guarded — conflict and staleness refusals are never force-able:

```sh
ign workspace checkout MyProject ./MyProject
ign workspace status ./MyProject
ign workspace push ./MyProject --yes
```

Single resources take the shorter `ign edit MyProject path/to/resource.json` (fetch → `$EDITOR` → encode → push; unchanged saves are clean no-ops).

## Use it from an AI agent

Every command's JSON contract is agent-ready on its own; the transports remove the last mile:

```sh
claude mcp add ign -- ign mcp serve    # MCP: the whole CLI as tools
npx skills add TheThoughtagen/ignition-cli -g   # agent skills (the playbook)
```

`ign lsp` serves live gateway completions/hover/diagnostics to editors — see [ignition-ide-plugins](https://github.com/TheThoughtagen/ignition-ide-plugins) for the nvim client.
