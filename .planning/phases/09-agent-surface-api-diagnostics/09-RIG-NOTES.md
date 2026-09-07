# Phase 09 — Live Rig Ops Log (09-02)

**Purpose:** Ops record for the two disposable rigs spun to capture live wire truth (09-LIVE-CAPTURES.md). No product code touched. **No secret material in this file** — token KEYS live only in the session shell env (`$T_A` / `$T_B`); only token NAMES appear below.

## Rigs

| Rig | Image | Host port | Container | Created (UTC) | Commissioned (UTC) | Provisioned token (NAME only) |
| --- | ----- | --------- | --------- | ------------- | ------------------ | ----------------------------- |
| A | `inductiveautomation/ignition:8.3.6` | 18188 → 8088 | `ign-p9-836` (id `b63402cfb9d7`) | 2026-09-07T01:48:37Z | 2026-09-07T02:29:30Z | `p9tok836` |
| B | `inductiveautomation/ignition:8.3.3` | 19188 → 8088 | `ign-p9-833` (compose project `ign-p9-833`, dir `/tmp/ign-p9-rigs/ign-p9-833/`) | 2026-09-07T01:48:38Z | 2026-09-07T02:29:30Z | `p9tok833` |

Pre-flight: ports 18188/19188 confirmed free (`lsof` empty) before start; both images already cached locally (pulled during Phase 4). Untouched throughout: flux-scratch (55432), cask-postgres (5433), and all other stacks.

## Launch commands

Rig A (ephemeral `docker run`, per Phase-4 pattern):

```bash
docker run -d --name ign-p9-836 -p 18188:8088 \
  -e ACCEPT_EULA=Y -e GATEWAY_ADMIN_PASSWORD=password -e IGNITION_EDITION=standard \
  inductiveautomation/ignition:8.3.6
```

Rig B (own compose project, per Phase-4 pattern — `docker-compose.yml` in `/tmp/ign-p9-rigs/ign-p9-833/`: image `inductiveautomation/ignition:8.3.3`, port `19188:8088`, env `ACCEPT_EULA=Y` / `GATEWAY_ADMIN_PASSWORD=password` / `IGNITION_EDITION=standard`, named volume `ign-p9-833_gateway_data:/var/lib/ignition/data`):

```bash
docker compose -p ign-p9-833 --project-directory /tmp/ign-p9-rigs/ign-p9-833 up -d
```

## Commissioning — NEW wire-level recipe (supersedes the env-var assumption)

**Finding:** the 8.3.x image entrypoint (`docker-entrypoint.sh`, 732 lines, read in full inside the container) contains **zero** handling of `ACCEPT_EULA` / `GATEWAY_ADMIN_PASSWORD` — those are 8.1-image-era vars. First boot sits at `needs_commissioning` (log: `Resources needing commissioning: eula`) and `/StatusPing` answers `200 {"state":"RUNNING","details":"COMMISSIONING"}` — HTTP 200 does NOT mean commissioned; poll for the `details` field to disappear.

Commissioning was completed **headlessly via the commissioner wizard's own wire API** (endpoints extracted from `/res/sys/js/commissioner/commissioner.js`, then replayed with curl — no browser):

```bash
# 1. List pending steps ({"steps":{"eula":"license"},"edition":"","canReachIA":true})
curl http://localhost:<port>/bootstrap
# 2. Read the EULA text (stepName=eula, id=license — id is the steps-map VALUE)
curl "http://localhost:<port>/get-step?step=license&name=eula"          # 200, HTML agreement
# 3. Accept EULA -> 201
curl -X POST -H 'Content-Type: application/json' \
  -d '{"id":"license","step":"eula","data":{"accept":true}}' \
  http://localhost:<port>/post-step
# 4. Tell the gateway to leave commissioning mode -> 200 {"gatewayAddress":"http://localhost:8088"}
curl -X POST -H 'Content-Type: application/json' \
  -d '{"id":"finished","step":"finished","data":{"startGateway":true}}' \
  http://localhost:<port>/post-step
# 5. Poll /StatusPing until body is exactly {"state":"RUNNING"} (gateway restarts; takes ~2 min)
```

Timeline: first-boot 01:48:37Z → first StatusPing 200 (still COMMISSIONING) 01:49:05Z → EULA accepted (201) on both rigs ~02:12Z → start-gateway POST (200) ~02:28Z → **both rigs `{"state":"RUNNING"}` at 02:29:30Z**. The AUTH (set admin credentials) step never appeared in `bootstrap` on either rig, yet admin/`password` login worked — consistent with the gateway itself consuming `GATEWAY_ADMIN_PASSWORD` at first boot (the env vars were set on both rigs); which layer consumed it is unresolved and irrelevant: login worked, zero browser needed.

## Headless API-token provisioning (Phase-4 recipe, re-proven on both rigs)

`/tmp/ign-p9-rigs/provision_token.sh URL USER PASS TOKENNAME` — verbatim wire recipe from 04-VERIFICATION.md addendum (only change: per-run `mktemp` files instead of fixed `/tmp/sp.json` paths so two rigs never collide):

1. Tier-1 OIDC login ladder (`/data/app/login` redirects → `idp/default/authn/next-challenge` ×2, `submit-challenge/basic` with `{username,password}`) → `webui-sid` cookie + csrfToken from `/data/app/session`.
2. `POST /data/api/v1/api-token/generate` `{}` (session + `X-CSRF-Token`) → `{"key","hash"}`.
3. `POST /data/api/v1/resources/ignition/api-token` (session + CSRF) with ARRAY body `[{"name":N,"collection":"core","enabled":true,"description":"","config":{"profile":{"type":"basic-token","secureChannelRequired":false,"securityLevels":[{"name":"Authenticated","children":[]}],"timestamp":<epoch_ms>},"settings":{"tokenHash":<hash>}}}]` → `.success==true`.
4. **The 403 fix:** `GET /data/api/v1/resources/singleton/ignition/security-properties?collection=core`, then PUT (array body carrying the current `signature`) with `readPermissions`/`writePermissions` both `{"type":"AnyOf","securityLevels":[{"name":"Authenticated","children":[]}]}`.
5. Token is `NAME:key` (key recorded NOWHERE — `$T_A` = `p9tok836:<key>`, `$T_B` = `p9tok833:<key>`, session shell env only).

Sanity per rig: `curl -H "X-Ignition-API-Token: $T_x" http://localhost:<port>/data/api/v1/gateway-info` → **200 on both** (18188 with `p9tok836`, 19188 with `p9tok833`).

## Teardown

Executed at end of Task 2 (see 09-LIVE-CAPTURES.md §Teardown):

```bash
docker rm -f -v ign-p9-836                                   # Rig A (container + anonymous data volume)
docker compose -p ign-p9-833 --project-directory /tmp/ign-p9-rigs/ign-p9-833 down -v --remove-orphans  # Rig B (volume ign-p9-833_gateway_data deleted)
```

Result: zero `ign-p9*` containers/volumes remain; ports 18188/19188 freed.

---
*Executed: 2026-09-07, autonomous live-capture run for phase 09 plan 02 (GSD executor).*
