# Phase 13 — Live Rig Ops Log (13-01, the Designer-diff spike)

**Purpose:** Ops record for the two disposable trial rigs spun for the roadmap-mandated both-rig Designer-diff spike (13-LIVE-CAPTURES.md). Follows the proven 11-RIG-NOTES / 09-02 recipe VERBATIM (fifth rig-generation run, ~4 min to RUNNING). No product code touched. **No secret material in this file** — token KEYS ride a chmod-600 scratch file (`/tmp/ign-p13-rigs/tokens.env`) plus the scratch profile config (`/tmp/ign-p13-rigs/config.toml`, 0600), both outside the repo. Only NAMES and PATHS appear below.

## Trial windows (the locked 2h budget)

| Rig | Window START (UTC) | RUNNING (UTC) | Window END (UTC) | Note |
| --- | ------------------ | ------------- | ---------------- | ---- |
| A (8.3.6) | 21:46:32 (container created) | 21:49:41 | _pending — recorded at Task 3 teardown_ | 2h image-trial budget ends ~23:46Z |
| B (8.3.3) | 21:46:47 (container created) | 21:50:01 | _pending — recorded at Task 3 teardown_ | same |

Pre-flight (21:44Z): Docker/OrbStack up; **port 18188 was held by `ign-uat-836`** — the Phase-10 UAT disposable rig (that phase complete and verified). Removed (`docker rm -f ign-uat-836`) to free the plan-mandated port; ports 18188/19188 confirmed LISTEN-free before launch. Stale EXITED `ign-p11-*` containers from Phase 11 (torn down after 11-06, exit 137) left as-is — no port conflict; the new rigs use fresh names (`ign-p13-*`). Both images cached locally (Phases 4–11).

## Rigs

| Rig | Image | Host port | Container | Created (UTC) | Commissioned RUNNING (UTC) | Token (NAME only) | Secret/token location (PATH only) |
| --- | ----- | --------- | --------- | ------------- | -------------------------- | ----------------- | --------------------------------- |
| A | `inductiveautomation/ignition:8.3.6` | 18188 → 8088 | `ign-p13-836` (id `95ebf8602683`, ephemeral docker run) | 21:46:32 | 21:49:41 | `p13tok836` | profile `p13r836`.token_env `TOKEN_p13r836` in `/tmp/ign-p13-rigs/tokens.env` (0600) |
| B | `inductiveautomation/ignition:8.3.3` | 19188 → 8088 | `ign-p13-833-ignition-1` (compose project `ign-p13-833`, dir `/tmp/ign-p13-rigs/ign-p13-833/`) | 21:46:47 | 21:50:01 | `p13tok833` | profile `p13r833`.token_env `TOKEN_p13r833` in the same tokens.env |

## Launch commands

Rig A (Phase-4 pattern verbatim):

```bash
docker run -d --name ign-p13-836 -p 18188:8088 \
  -e ACCEPT_EULA=Y -e GATEWAY_ADMIN_PASSWORD=password -e IGNITION_EDITION=standard \
  inductiveautomation/ignition:8.3.6
```

Rig B (own compose project — `docker-compose.yml` in `/tmp/ign-p13-rigs/ign-p13-833/`: image `inductiveautomation/ignition:8.3.3`, port `19188:8088`, env `ACCEPT_EULA=Y` / `GATEWAY_ADMIN_PASSWORD=password` / `IGNITION_EDITION=standard`, named volume `ign-p13-833_gateway_data`):

```bash
docker compose -p ign-p13-833 --project-directory /tmp/ign-p13-rigs/ign-p13-833 up -d
```

## Commissioning — the 09-02 headless wire recipe, fifth consecutive proof

Both rigs reached `{"steps":{"eula":"license"},…}` bootstrap within ~50 s. Recipe via the reused `commission.sh` helper (verbatim 09-02 wire steps): `GET /bootstrap` → `GET /get-step?step=license&name=eula` (**200** both) → `POST /post-step` eula-accept (**201** both) → `POST /post-step` start-gateway (**200** both) → poll `/StatusPing` until `{"state":"RUNNING"}` (10 s interval).

Timeline: containers 21:46:32Z/21:46:47Z → STARTING 21:47:59Z → rig A RUNNING **21:49:41Z**, rig B RUNNING **21:50:01Z** (~3.5 min end-to-end).

## Headless API-token provisioning (fifth rig-generation proof)

The unchanged `provision_token.sh` (Phases 4–11 recipe) with names **`p13tok836`** / **`p13tok833`**; `NAME:KEY` pairs staged to `/tmp/ign-p13-rigs/tokens.env` (chmod 600) **in the same shell batch that provisioned**. Sanity: `GET /data/api/v1/gateway-info` with `X-Ignition-API-Token: NAME:key` → **200 on both rigs** (18188 with `p13tok836`, 19188 with `p13tok833`).

## Scratch CLI config + bundle deploy

- Scratch config: `IGNITION_CLI_CONFIG=/tmp/ign-p13-rigs/config.toml` (0600); profiles `p13r836` (active) / `p13r833` added via `ign profile add … --token-env TOKEN_p13r8XX` (auth mode `token_env`).
- **Binary note:** the installed `~/.cargo/bin/ign` (v1.0.0, Sep 10) AND the stale `target/debug/ign` both predated the current tree's route bundle — rebuilt the current tree debug build (`cargo build -p ignition-cli`) before deploying; tree `ROUTE_BUNDLE_VERSION = 1.3.0`.
- Bundle deploy (current tree, debug build): `ign --profile p13r836 webdev deploy --with-script-exec` / same for `p13r833` → **deployed 5 routes** (tags, tagConfig, alarms, tagHistory, scriptExec) to project `ign-cli` on BOTH rigs; secret staged per the deploy output (profile config, PATH-logging only).

## Historian + spike tag provisioning (unique names per run)

- Historian per rig via the native-REST `provision_internal_historian` shape (e2e_webdev.rs:868): `POST /data/api/v1/resources/com.inductiveautomation.historian/historian-provider` with the ARRAY body `{name, type: "com.inductiveautomation.historian/historian-provider", collection: "core", enabled: true, config: {profile: {type: "InternalHistorian"}, settings: {}}}` → **`success: true` both rigs**. Names **`p13hist836`** (rig A) / **`p13hist833`** (rig B) — virgin namespaces per the 11-06 unique-name discipline.
- Spike tag via the CLI: `echo '{"tagType":"AtomicTag","dataType":"Int4","value":0}' | ign tags config create '[default]P13H/T1' --file -` → **`quality: Good` both rigs**.

## BEFORE captures (committed Task 1)

- `ign tags config get '[default]P13H/T1' --json` → `artifacts/config-before-836.json` / `artifacts/config-before-833.json` (both exit 0, JSON-parse clean). Unbound baseline: no history keys present.

## Post-Designer capture (staged, one command)

`/tmp/ign-p13-rigs/capture.sh` (executable, syntax-checked): per rig runs the AFTER `tags config get`, validates JSON, and writes `artifacts/designer-diff-{836,833}.txt` via `diff <(jq -S . before) <(jq -S . after)` — machine-generated, never hand-edited. **The window's only interactive step is the Designer binding itself (Task 2 checkpoint).**

## Teardown state — INTENTIONAL KEEP-ALIVE (pending Task 3)

Both rigs are deliberately KEPT ALIVE through the Task-2 Designer checkpoint (containers `ign-p13-836` / `ign-p13-833-ignition-1`, named volume `ign-p13-833_gateway_data`, scratch dir `/tmp/ign-p13-rigs/` with tokens.env/config.toml/capture.sh intact). Task 3 performs the spike cleanup per rig (`tags config delete` both spike tags, historian provider delete per the e2e_webdev.rs:899 find→signature→DELETE shape) and records the window END times + final teardown here. If the 2h window expires while Task 2 waits: re-spin per this file's recipe with a NEW unique provider suffix and repeat only the Designer step.

---
*Executed: 2026-09-14, Task 1 of 13-01 (GSD executor, autonomous run). Window T0 21:46Z.*
