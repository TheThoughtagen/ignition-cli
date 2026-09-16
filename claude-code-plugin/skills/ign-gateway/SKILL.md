---
name: ign-gateway
description: Gateway operations with ign — the daily health check (status, license, redundancy, GAN), diagnostics bundles, logs tailing, doctor diagnosis, and restart/wait primitives. Use when inspecting or operating an Ignition 8.3+ gateway.
user-invocable: false
---

# ign Gateway Operations

Health, logs, and lifecycle for Ignition 8.3+ gateways. Requires the `ign` binary (see the ign-contract skill for the envelope/exit-code rules). macOS + Linux only.

## The morning check

One pass over gateway health — all reads, live-verified on 8.3.3 + 8.3.6:

```bash
ign status --json        # identity, platform, uptime, license (incl. trial countdown)
ign license status --json
ign redundancy status --json
ign gan status --json
```

Then the deeper probes as needed:

```bash
ign modules --json       # module list (healthy by default)
ign metrics --json       # performance gauges + thread counts
ign sessions --json      # designers, Perspective, Vision sessions
ign connections --json   # DB/OPC connections with healthcheck status
```

## Doctor: diagnose before you fix

```bash
ign doctor               # exits 0 whenever the diagnosis completes — failing checks are DATA
```

Doctor reports URL, liveness, commissioning, auth, permissions, WebDev route state, and rig health. **Never abort scripts on doctor findings** — a 403 on the security-properties read is a finding to interpret, not a crash. Add `--check-write` / `--webdev-route NAME` for the deeper ladders.

## Logs

```bash
ign logs --tail 50 --json        # recent entries as NDJSON
ign logs -f                      # follow (streams to stdout)
ign logs --level WARN --json     # filter by level
ign logs loggers --json          # logger inventory; levels are settable
```

## Diagnostics support bundle

Generate, poll, download — the state vocabulary is capture-locked (Generating → Valid; `Invalid` is a terminal steady state, so `wait` exits immediately with `bundle_not_available` instead of polling forever):

```bash
ign diagnostics generate --json
ign diagnostics status --json
ign diagnostics wait --json          # deadline-poll; honest exit on non-generating terminal states
ign diagnostics download --out bundle.gwbkdiag
```

A `bundle_not_available` answer means: run `diagnostics generate` again — waiting on `Invalid` is structurally futile.

## Restart / wait primitives

```bash
ign restart --yes --wait --json    # destructive (guarded) + polls until RUNNING
ign wait ready --json              # poll until the gateway answers
```

`restart` is refused exit 2 (`confirmation_required`) without `--yes`. Script the pair as: wait ready → act → wait ready.

## Typical agent loop

```bash
#!/bin/sh
# morning check — aborts on any nonzero exit from the ign commands below
# (set -e trips on usage exit 2 and config exit 3 too, not just network/auth)
set -e
ign status          --json > /tmp/gw-status.json
ign license status  --json > /tmp/gw-license.json
ign redundancy status --json > /tmp/gw-redundancy.json
jq -e '.ok' /tmp/gw-status.json > /dev/null && echo "gateway up"
jq -r '.data.trial // empty' /tmp/gw-license.json && echo "trial active" || true
```

Notes: `license` and `redundancy` have a **required `status` subcommand** — the bare form is a clap usage error (exit 2). The `jq` lines sit in `&&`/`||` lists, so their failures do not trigger `set -e`. Field names inside `data` are stable per release; prefer `jq -e '.ok'` gates and slug-based `error.code` matching (on **stderr**) over message text.
