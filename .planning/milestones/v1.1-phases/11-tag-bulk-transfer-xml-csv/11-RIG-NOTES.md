# Phase 11 — Live Rig Ops Log (11-01)

**Purpose:** Ops record for the two disposable rigs spun to run the five roadmap-mandated live probes (11-LIVE-CAPTURES.md). Follows the proven 09/10-RIG-NOTES recipe (fourth rig-generation run). No product code touched. **No secret material in this file** — token KEYS and the scriptExec secret VALUES never entered the repo; they ride a chmod-600 scratch file (`/tmp/ign-p11-rigs/tokens.env`, outside the repo) plus the CLI profile config (`/tmp/ign-p11-rigs/config.toml`, 0600, also outside the repo). Only NAMES and PATHS appear below.

## Rigs

| Rig | Image | Host port | Container | Created (UTC) | Commissioned RUNNING (UTC) | Token (NAME only) | scriptExec secret location (PATH only) |
| --- | ----- | --------- | --------- | ------------- | -------------------------- | ----------------- | -------------------------------------- |
| A | `inductiveautomation/ignition:8.3.6` | 18188 → 8088 | `ign-p11-836` (id `94c54075481b`, ephemeral docker run) | 15:25:51Z | 15:29:35Z | `p11tok836` | profile `p11r836`.webdev_secret in `/tmp/ign-p11-rigs/config.toml` (0600); copied to `SECRET_p11r836` in `tokens.env` |
| B | `inductiveautomation/ignition:8.3.3` | 19188 → 8088 | `ign-p11-833-ignition-1` (compose project `ign-p11-833`, dir `/tmp/ign-p11-rigs/ign-p11-833/`, id `74cce61c92b8`) | 15:25:18Z | 15:29:35Z | `p11tok833` | profile `p11r833`.webdev_secret in the same config.toml; copied to `SECRET_p11r833` in `tokens.env` |

Pre-flight: Docker/OrbStack up (engine 29.4.0); ports 18188/19188 free (only stale CLOSED lsof entries from an earlier session); both images cached locally (Phases 4–10). Unrelated running stacks (whk-*, cask-postgres, ignlab, mx-review) untouched. No ign-p11* residue from any prior attempt existed (clean-rig provenance).

## Launch commands

Rig A (Phase-4 pattern verbatim):

```bash
docker run -d --name ign-p11-836 -p 18188:8088 \
  -e ACCEPT_EULA=Y -e GATEWAY_ADMIN_PASSWORD=password -e IGNITION_EDITION=standard \
  inductiveautomation/ignition:8.3.6
```

Rig B (own compose project — `docker-compose.yml` in `/tmp/ign-p11-rigs/ign-p11-833/`: image `inductiveautomation/ignition:8.3.3`, port `19188:8088`, env `ACCEPT_EULA=Y` / `GATEWAY_ADMIN_PASSWORD=password` / `IGNITION_EDITION=standard`, named volume `ign-p11-833_gateway_data:/var/lib/ignition/data`):

```bash
docker compose -p ign-p11-833 --project-directory /tmp/ign-p11-rigs/ign-p11-833 up -d
```

## Commissioning — the 09-02 headless wire recipe, fourth consecutive proof

Both rigs booted to `{"state":"RUNNING","details":"COMMISSIONING"}` within ~30 s (15:26:36Z; warm image layers). Recipe via the reused `commission.sh` helper (verbatim 09-02 wire steps): `GET /bootstrap` (both `{"steps":{"eula":"license"},…}`) → `GET /get-step?step=license&name=eula` (**200**) → `POST /post-step` eula-accept (**201** both) → `POST /post-step` start-gateway (**200** both, `{"gatewayAddress":"http://localhost:8088"}`) → poll `/StatusPing` until body is exactly `{"state":"RUNNING"}`.

Timeline: containers created 15:25:18Z/15:25:51Z → COMMISSIONING 15:26:36Z → EULA-accept + start-gateway ~15:27Z → **both rigs `{"state":"RUNNING"}` at 15:29:35Z** (~3 min end-to-end).

## Headless API-token provisioning (fourth rig-generation proof)

`/tmp/ign-p10-rigs/provision_token.sh URL USER PASS TOKENNAME` (unchanged script; prints `NAME:KEY`). Names **`p11tok836`** / **`p11tok833`**; keys staged to `/tmp/ign-p11-rigs/tokens.env` (chmod 600) **in the same shell batch that provisioned** — the 09-02 lesson applied from the start (no lost-key second pass).

Sanity per rig: `GET /data/api/v1/gateway-info` with `X-Ignition-API-Token: NAME:key` → **200 on both** (18188 with `p11tok836`, 19188 with `p11tok833`).

## Scratch CLI config + bundle deploy

- Scratch config: `IGNITION_CLI_CONFIG=/tmp/ign-p11-rigs/config.toml`; profiles added via `ign profile add p11r836 http://localhost:18188` / `ign profile add p11r833 http://localhost:19188` (auth mode `token_env` → `IGNITION_TOKEN` per spawn).
- Bundle deploy (current 1.1.0 tree, debug build): `ign --profile p11r836 webdev deploy --with-script-exec` / same for `p11r833` → **deployed 5 routes** (tags, tagConfig, alarms, tagHistory, scriptExec) to project `ign-cli` on BOTH rigs; deploy output names the persisted secret location (`stored in the profile config at 0600`); secret VALUES copied into the scratch `tokens.env` as `SECRET_p11r836` / `SECRET_p11r833` (PATH-logging only here, per the no-secret-material rule).
- scriptExec handshake sanity: `POST /system/webdev/ign-cli/cli/scriptExec {"action":"version"}` + `x-ignition-cli-secret` header → **`{"data":{"routeVersion":"1.1.0","minCli":"1.0"},"ok":true}` on both rigs**.

## Teardown state — INTENTIONAL KEEP-ALIVE (explicit per plan verification item)

**Both rigs are deliberately KEPT ALIVE at the end of 11-01** (containers `ign-p11-836` / `ign-p11-833-ignition-1`, named volume `ign-p11-833_ign-p11-833_gateway_data`, scratch dir `/tmp/ign-p11-rigs/`, secret/token files intact) for the 11-06 live gate, which explicitly permits "the kept-alive 11-01 rigs if teardown was deferred — state which in the doc" (11-06-PLAN.md Task 2). The 11-06 gate self-deploys the 1.2.0 bundle over this 1.1.0 deploy, so the stale route version on these rigs is expected and not a hazard. Rig provenance note for 11-06: the probe-created tags (`P11Seed*`, `P11Import*`, `P11UDT*`, `P11Roundtrip*`, `P11Csv*`) remain on the rigs; the 11-06 gate uses `P11Live*` namespaced paths (no collision).

---
*Executed: 2026-09-11, autonomous live-capture run for phase 11 plan 01 Task 1 (GSD executor).*
