# Phase 09 — Live Rig Ops Log (09-02)

**Purpose:** Ops record for the two disposable rigs spun to capture live wire truth (09-LIVE-CAPTURES.md). No product code touched. **No secret material in this file** — token KEYS never entered the repo; they rode the session shell env plus a chmod-600 scratch file (`/tmp/ign-p9-rigs/tokens.env`, outside the repo, removed at teardown). Only token NAMES appear below.

## Rigs

| Rig | Image | Host port | Container | Created (UTC) | Commissioned (UTC) | Provisioned token (NAME only) |
| --- | ----- | --------- | --------- | ------------- | ------------------ | ----------------------------- |
| A | `inductiveautomation/ignition:8.3.6` | 18188 → 8088 | `ign-p9-836` (id `b63402cfb9d7`) | 2026-09-07T01:48:37Z | 2026-09-07T02:29:30Z | `p9tok836b` |
| B | `inductiveautomation/ignition:8.3.3` | 19188 → 8088 | `ign-p9-833` (compose project `ign-p9-833`, dir `/tmp/ign-p9-rigs/ign-p9-833/`) | 2026-09-07T01:48:38Z | 2026-09-07T02:29:30Z | `p9tok833b` |

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
5. Token is `NAME:key` (key recorded NOWHERE). First provisioning pass created `p9tok836` / `p9tok833`; the executor's shell env did not persist keys between tool calls (keys were never printed — only names), so a second pass created `p9tok836b` / `p9tok833b`, staged in `/tmp/ign-p9-rigs/tokens.env` (chmod 600, outside the repo) and sourced per command batch. The orphaned first-pass token entries died with the rigs at teardown. Operational lesson: source keys from a scratch file immediately after provisioning when the driver may not preserve env between calls.

Sanity per rig: `curl -H "X-Ignition-API-Token: $T_x" http://localhost:<port>/data/api/v1/gateway-info` → **200 on both** (18188 with `p9tok836b`, 19188 with `p9tok833b`).

## Teardown

Executed at end of Task 2 (see 09-LIVE-CAPTURES.md §Teardown):

```bash
docker rm -f -v ign-p9-836                                   # Rig A (container + anonymous data volume)
docker compose -p ign-p9-833 --project-directory /tmp/ign-p9-rigs/ign-p9-833 down -v --remove-orphans  # Rig B (volume ign-p9-833_gateway_data deleted)
```

Result: zero `ign-p9*` containers/volumes remain; ports 18188/19188 freed; capture scratch dirs, `tokens.env` (the only file that ever held key material), and the commissioner.js probe download all removed.

---

# Phase 09 — Live Gate Matrix Run (09-06)

**Purpose:** the phase's verification oracle — the six env-gated live gates (`crates/ignition-cli/tests/e2e_api_diagnostics.rs`, Task 1 commit `d5a1226`) executed against BOTH rigs on real wire truth. No secret material in this file (token keys never left the chmod-600 scratch file `/tmp/ign-p9-rigs/tokens.env`, outside the repo, removed at teardown).

## Rig re-provisioning (2026-09-07, 09-06 Task 2)

Recipe: 09-02 verbatim (docker run for A; compose project for B). Commissioning: the headless wire recipe again (`/bootstrap` → `/get-step` → eula-accept 201 → start-gateway 200 → StatusPoll). Tokens: `provision_token.sh` again, names `p9tok836c` / `p9tok833c`, staged to scratch + sourced per batch.

| Rig | Image | Port | Commissioned RUNNING (UTC) | Token (NAME only) | gateway-info sanity |
| --- | ----- | ---- | -------------------------- | ----------------- | ------------------- |
| A | 8.3.6 | 18188 | 17:08:58Z (`{"state":"RUNNING"}`) | `p9tok836c` | 200 |
| B | 8.3.3 | 19188 | 17:08:58Z | `p9tok833c` | 200 |

(Containers warm-booted to COMMISSIONING in ~30 s this time — image layers already local.)

## Gate matrix results — ALL PASS on BOTH rigs

Every gate's PASS is the recorded evidence for phase success criteria 3 + 4. Test-body wall times from cargo (`finished in …`); session timestamps around them verbatim.

### Rig A — 8.3.6 (18188)

- Read-only gates: `IGNITION_LIVE_URL=…18188 IGNITION_LIVE_TOKEN=… cargo test -p ignition-cli --test e2e_api_diagnostics -- --ignored` → **6/6 PASS, finished in 1.85 s** (run 17:11:08Z→17:13:32Z session window; the first pass at 17:10:23Z was 5/6 — see the gateway-info delta below).
- Bundle round-trip (the ONE mutation, `IGNITION_LIVE_MUTATIONS=1`): **PASS, finished in 3.82 s** (17:13:49Z→17:13:54Z). Real body executed: generate → wait → download (tempdir) → status re-read, ZIP magic + `fileSize` == downloaded bytes all asserted live.
- (Also witnessed WITHOUT mutations: `bundle_round_trip_live` refuses-and-passes green — the opt-in refusal shape, 1.85 s 6/6 run.)

### Rig B — 8.3.3 (19188)

- Read-only gates: → **6/6 PASS, finished in 2.05 s** (17:14:03Z→17:14:06Z), zero discrepancies on first pass.
- Bundle round-trip (`IGNITION_LIVE_MUTATIONS=1`): **PASS, finished in 4.00 s** (17:14:13Z→17:14:18Z).

## Live-truth delta captured this run (recorded per plan Task-2 step 3)

**1. `GET /data/api/v1/gateway-info` BODY — first-time capture** (the 09-02 captures recorded only the 200 status + the 4xx probes, never this body; the gate's original spot-key `version` was a hypothesis and FAILED live on rig A at 17:10:23Z — the run's one honest failure). Verbatim bodies:

- 8.3.6: `{"name":"Ignition-6b9810b95049","redundancyRole":"Independent","edition":"standard","hostname":"localhost","port":"18188","ignitionVersion":"8.3.6 (b2026042713)","deploymentMode":"","timeZone":"Coordinated Universal Time","timeZoneId":"Etc/UTC","jvmVersion":"17.0.18","allowUnsignedModules":false,"license":{"mode":"Trial","validForVersion":8,"expirationDate":"Sep 7, 2026, 7:08:03 PM","licenseRestrictions":[]}}`
- 8.3.3: identical key set/order, values differ (`"ignitionVersion":"8.3.3 (b2026012009)"`, `jvmVersion:"17.0.17"`, container-named `name`, own `port`).

**The delta:** the version rides the gateway-native key **`ignitionVersion`** — NOT `version` (the wiremock mocks in `version_gateway_contract.rs` serve the alias name, which `GatewayInfo` tolerates via `#[serde(alias)]`). Zero point-release drift in the KEY SHAPE between 8.3.3/8.3.6 (values only). Gate fixed to assert `ignitionVersion` (the same key `GatewayInfo` renames in); both rigs then green. The gate file comment cites this run's capture.

**2. Everything else held the 09-02 captures exactly** — trial countdown present, redundancy role `Independent` + ms-magnitude uptime + `-1` sentinel, GAN zeros with running ≤ total, the 4xx partition (unknown path exit 6 `not_found`; DELETE-on-real-path 404-empty → exit 6 `not_found`, the falsified-405 hypothesis staying falsified on a fresh rig), bundle `Generating`→`Valid` PascalCase, download `PK\x03\x04`, ready-state persistence, `fileSize` == downloaded bytes on both rigs.

## Teardown (09-06)

```bash
docker rm -f -v ign-p9-836
docker compose -p ign-p9-833 --project-directory /tmp/ign-p9-rigs/ign-p9-833 down -v --remove-orphans
```

Result: zero `ign-p9*` containers/volumes; ports 18188/19188 freed; `tokens.env` (sole key-material holder) removed; scratch dir cleaned. Final phase check (`cargo test --workspace && cargo clippy --all-targets -- -D warnings && cargo fmt --check`) green — see 09-06-SUMMARY.md.

---
*Executed: 2026-09-07, autonomous live-gate run for phase 09 plan 06 (GSD executor). Prior section: the 09-02 capture-run ops log (kept verbatim for provenance).*
