# Phase 2: User Setup Required (OPT-IN ONLY)

**Generated:** 2026-08-21
**Phase:** 02-gateway-health-inspection
**Status:** Incomplete

**Nothing here is required for CI, plan execution, or the wiremock test
suite** — this phase's contracts are fully covered by mocks. These items
exist only to run the opt-in live-gateway suite
(`cargo test -p ignition-core --test live_gateway -- --ignored`) against
a commissioned Ignition 8.3+ gateway you control, and to use `ign`
against a real gateway at all.

## Environment Variables

| Status | Variable | Source | Add to |
|--------|----------|--------|--------|
| [ ] | `IGNITION_LIVE_URL` | Base URL of your commissioned 8.3+ gateway (e.g. `http://localhost:18088`) | shell / `.env` |
| [ ] | `IGNITION_LIVE_TOKEN` | Gateway UI → Platform → Security → API Keys → Create (see below) — copy the FULL `name:key` string | shell / `.env` |
| [ ] | `IGNITION_LIVE_USER` | (optional, Basic-rejection test only) a valid commissioned username | shell |
| [ ] | `IGNITION_LIVE_PASSWORD` | (optional, Basic-rejection test only) that user's password | shell |

## Gateway (Rig) Setup

Only if you don't already have a commissioned 8.3 gateway:

- [ ] **Start a Docker rig**
  ```bash
  docker run -d --name ign-research -p 18088:8088 \
    -e ACCEPT_IGNITION_EULA=Y inductiveautomation/ignition:8.3.6
  ```
- [ ] **Commission it** via `http://localhost:18088/welcome` (browser):
  pick "Ignition" standard → trial mode, create the admin user, Finish
  Setup → Start Gateway.
- [ ] **Create the API token** — Gateway UI → Platform → Security →
  API Keys → Create:
  - Type: **Basic Token**
  - Security level: one with admin
  - **UNCHECK "Require secure connections"** (http rig — leaving it
    checked → 403)
  - Copy the FULL `name:key` string from the dialog (key-only → 401).

## Verification

```bash
export IGNITION_LIVE_URL=http://localhost:18088
export IGNITION_LIVE_TOKEN='yourname:yourkey'
cargo test -p ignition-core --test live_gateway -- --ignored
```

Expected results:
- `live_gateway_info_parses ... ok` (reports the real `ignitionVersion`)
- `live_token_auth_works ... ok`
- With no envs set at all: the same command is a green no-op (skips)

---

**Once all items complete:** Mark status as "Complete" at top of file.
