# Inspect a gateway

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

Successful commands normally return an envelope containing `ok`, `profile`, and `data`. See the [reference](https://thethoughtagen.github.io/ignition-cli/docs/reference/#output-contract-for-agents) for errors and streaming exceptions.

`ign doctor` can finish with exit code zero while reporting failed checks. Read the checks themselves before assuming the connection works.

## Add other operations when needed

Gateway status and project listing use native REST endpoints. Runtime tag operations need the CLI's WebDev routes. Review those prerequisites and the overwrite behavior of project operations in the [reference](https://thethoughtagen.github.io/ignition-cli/docs/reference/) before using them.
