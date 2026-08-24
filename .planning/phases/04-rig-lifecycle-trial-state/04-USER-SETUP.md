# Phase 4: User Setup Required (04-03 trial state — live e2e gates)

**Generated:** 2026-08-23
**Updated:** 2026-08-24 — the human tasks below are OBSOLETE. All live gates
were executed autonomously via docker compose; token provisioning is now a
fully headless recipe. See the addendum in
[04-VERIFICATION.md](./04-VERIFICATION.md) → "Live Gates — Automated
Execution (addendum)" for what ran, verbatim results, and the copy-paste
provisioning script.

## Status: superseded (no human action required)

- ~~Provision an API token on ign-research~~ — **obsolete**: API tokens can
  be provisioned headlessly (OIDC login ladder + `api-token/generate` +
  `resources/ignition/api-token` create + a `security-properties`
  read/write-permission patch). The full script lives in the VERIFICATION
  addendum; it was executed on a fresh 8.3.6 rig (not ign-research, whose
  admin password remains unknown and which stays untouched).
- ~~Mint a durable token on the git-module rig~~ — **obsolete** for the
  gates (executed on disposable rigs instead); the recipe applies to any
  rig whose admin credentials are known. NOTE the `collection` mystery from
  the 04-03 spike is solved: the value is `"core"` — but registration alone
  is NOT enough: the gateway's `security-properties` singleton must also
  admit the token's security level (`Authenticated`) in
  `readPermissions`/`writePermissions` (AnyOf), otherwise every
  token-authenticated `/data/api/v1/*` call returns 403. Phase 02's
  research had already predicted exactly this.

## Historical content (as written 2026-08-23, for the record)

## What already works (no action needed)

- `ign rig trial status` — credential-free, live-verified on BOTH
  rigs (8.3.3 `ignition-devops` at :9088, 8.3.6 `ign-research` at
  :18088).
- `ign rig trial reset` tier 1 (native login) — **live-verified
  end-to-end on 8.3.3** using `admin` / `password` (the WHK convention
  credentials, confirmed working; also recorded in
  `~/whiskeyhouse/ignition-trial-resetter/instances/tst1.env`).
- The trial-reset state gate (`trial_not_expired`) — live-verified.
- Note: `ign-research` does NOT accept `admin`/`password` (verified:
  `{"success":false}` at the challenge endpoint).

## Environment Variables (to run the live e2e gates)

| Status | Variable | Source | Purpose |
|--------|----------|--------|---------|
| [ ] | `IGNITION_LIVE_URL` | e.g. `http://localhost:18088` (ign-research) or `http://localhost:9088` (git-module rig) | the rig under test |
| [ ] | `IGNITION_LIVE_TOKEN` | provision an API token (below) | the tier-0 probe + authed calls |
| [ ] | `IGNITION_LIVE_USER` / `IGNITION_LIVE_PASSWORD` | rig admin credentials (8.3.3: `admin`/`password` known good) | the tier-1 login e2e |
| [ ] | `IGNITION_LIVE_MUTATIONS` | set to `1` | arms mutation e2e (the existing convention) |

Run them with:

```bash
IGNITION_LIVE_URL=http://localhost:9088 \
IGNITION_LIVE_USER=admin IGNITION_LIVE_PASSWORD=password \
IGNITION_LIVE_MUTATIONS=1 \
cargo test -p ignition-core --test trial_contract -- --ignored
```

(The tier-1 live test quiet-skips unless the rig's trial is EXPIRED —
the state gate is live-verified behavior; the git-module rig's trial
expires ~2 h after its last reset.)

## Remaining human tasks

### 1. Provision an API token on ign-research (8.3.6 — the expired rig)

- **Where:** gateway web UI → Config → Security → API Tokens → Create
  (login requires the rig's admin password, which only you know)
- **Why:** settles the tier-0 question on the 8.3.6 line (does a
  token-auth `POST /data/api/v1/trial` succeed on an expired rig?) and
  unlocks authed live gates against the permanently-expired rig.
- **Verification:** with `IGNITION_LIVE_URL=http://localhost:18088`
  and the token in `IGNITION_LIVE_TOKEN` +
  `IGNITION_LIVE_MUTATIONS=1`, run the `trial_reset_tier0_probe`
  ignored test — it prints the answer either way (TIER 0 WORKS / TIER
  0 REJECTED).

### 2. (Optional) mint a durable token on the git-module rig

- The spike found `POST /data/api/v1/api-token/generate` (session +
  CSRF) and created-token attempts, but the `resources/ignition/
  api-token` create body's `collection` value could not be determined
  headlessly (the config UI is UA-gated against automation). Creating
  one token via the web UI gives `IGNITION_TOKEN` a durable value for
  tier-0 attempts after every `rig reset` (resets wipe provisioned
  tokens — a known Ignition behavior).
