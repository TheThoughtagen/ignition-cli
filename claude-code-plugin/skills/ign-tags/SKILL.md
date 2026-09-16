---
name: ign-tags
description: Tag operations with ign — providers, browse, read/write values, config CRUD, UDTs, alarms, history, and byte-faithful XML/CSV bulk transfer with loss gates. Use when managing Ignition tag providers, values, configurations, or bulk transfers.
user-invocable: false
---

# ign Tag Operations

Tag lifecycle on Ignition 8.3+. Two backends: provider management rides native REST (always available); browse/read/write/config ride the CLI's own deployed WebDev routes. macOS + Linux only.

## Prerequisite: deployed routes

Value and config verbs need the CLI's versioned routes on the gateway (one-time per gateway):

```bash
ign webdev deploy          # deploys the embedded bundle
ign webdev status          # per-route version handshake — verify after any CLI upgrade
```

A version-mismatch refusal (`route_version_mismatch`, exit 6) means: re-deploy, never pin an old CLI.

## Browse, read, write

```bash
ign tags provider list --json          # providers with tag counts + health
ign tags browse --json                 # tree; providers at the root
ign tags browse "Provider/Line1" --json
ign tags read "[Provider]Line1/Motor/Speed" --json   # value + quality + timestamp
ign tags write "[Provider]Line1/Motor/Speed" 42 --json
```

- `write` values parse as JSON scalars (number/bool/null); unparseable input is sent as a string; arrays/objects refuse.
- Offline mode: `ign tags browse --from-export <export.json>` — browse an export with zero gateway contact.

## Tag configuration (the surgical edit loop)

`tags config` is JSON-in/JSON-out CRUD on tag configuration — stringified values are re-parsed so agents see real JSON:

```bash
ign tags config get "[Provider]Line1/Motor" --json
ign tags config create --file motor.json --json
ign tags config edit   "[Provider]Line1/Motor" --set someKey=true --json
ign tags config delete "[Provider]Line1/Motor" --yes --json
```

### Historian bindings (licensed gateways)

Tag↔historian data-flow bindings appear in config output:

```bash
ign tags config get "[Provider]Tank/Temp" --json | jq '.data.history_summary'
```

Binding keys are capture-locked: `historyEnabled`, `historyProvider`, `sampleMode` — set them through `tags config edit/create` to bind a tag into the historian. `historical_group` is absent by design (the gateway needs no such key for a functional binding).

## Bulk transfer: byte-faithful by design

```bash
# XML — the gateway's own bytes, passed through verbatim (round-trip fidelity oracle-proven)
ign tags export "[Provider]Line1" --format xml --out line1.xml
ign tags import --provider NewProvider --file line1.xml --format xml --json

# JSON — the native lossless interchange
ign tags export "[Provider]Line1" --out line1.json
ign tags import --provider NewProvider --file line1.json --json

# CSV — CLI-GENERATED and LOSSY (the gateway cannot export CSV)
ign tags export "[Provider]Line1" --format csv --out line1.csv
```

**Loss gate (imports):** XML/CSV imports run a loss scan **before** the import and **refuse exit 2** with a prose report of what would drop or coerce (alarms, permissions, UDT type definitions land differently, legacy column limits). Pass `--yes` to proceed anyway; the report rides the envelope as `data.loss_report`. The scan is advisory — the gateway's own refusals (e.g. collisions) still apply on top.

- `stdout` mode (`-` as output file) emits the raw payload — no envelope (documented exception).
- Collisions: `abort` (default) refuses; `overwrite` replaces — destructive, needs `--yes`.
- UDT type definitions import at **provider root** basePath (landing in `<provider>_types_/`); folder-scoped imports of type definitions are refused by the gateway itself.

## Alarms, UDTs, history

```bash
ign tags alarms --json                       # active alarms, filterable
ign tags alarms acknowledge --yes --json     # guarded
ign tags udt show "MotorType" --json         # recursive: parameters + nested children
ign tags history "[Provider]Tank/Temp" --json  # historian-backed; needs provisioned historian
```
