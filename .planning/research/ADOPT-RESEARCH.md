# ADOPT-RESEARCH — live wire captures for `ign adopt` (D1)

**Source of truth:** every shape below was live-captured 2026-09-18 on the
`ignition-devops` rig, Ignition **8.3.6 (b2026042713)**, docker image
`inductiveautomation/ignition:8.3.6`, via browser devtools (Playwright network
capture) while clicking the Config UI as `admin` — the same method as the
trial-route capture (04-RESEARCH). Every request rode the native OIDC session
cookie + `X-CSRF-Token` header — the exact machinery `client::idp::login`
already produces.

**Headline: there is NO bespoke api-keys route.** 8.3 API keys are a resource
type (`ignition/api-token`) managed through the **existing resources API** —
plus one tiny generate endpoint for the key material. `ign` already ships the
resources client (`client/resources.rs`); D1 is nearly free.

---

## Step 1 — Key mint (two calls, session+CSRF)

### 1a. `POST /data/api/v1/api-token/generate`

Request: empty JSON body, session cookie + `X-CSRF-Token` header.
Response `200` (live-captured):

```json
{ "key": "OEVWuytN-pOzWum_eDS6gAXEwZS9YlgBZ8-a43a2Y6M",
  "hash": "01fQ4KjcPq2VzxU7eYGvEgKuTH6rlnFJLWtUQhQndtU" }
```

- `key` = the plaintext token, **returned here and nowhere else** (the
  resource list only ever shows the hash). 43-char urlsafe.
- `hash` = what gets stored on the resource.

### 1b. `POST /data/api/v1/resources/ignition/api-token`

Request body — an **array** of resource records (live-captured verbatim):

```json
[{
  "name": "ign-adopt-capture",
  "collection": "core",
  "enabled": true,
  "description": "",
  "config": {
    "profile": {
      "securityLevels": [
        { "name": "Authenticated",
          "description": "Represents a user who has been authenticated by the system.",
          "children": [] }
      ],
      "secureChannelRequired": false,
      "type": "basic-token",
      "timestamp": 1789760514446
    },
    "settings": { "tokenHash": "01fQ4KjcPq2VzxU7eYGvEgKuTH6rlnFJLWtUQhQndtU" }
  }
}]
```

- `secureChannelRequired: false` = the "Require secure connections" checkbox
  UNCHECKED (default from the type descriptor is `true` — adopt must always
  send `false` explicitly or http gateways 403).
- security level encoding: **leaf path list**. A leaf `Authenticated` is
  `{name:"Authenticated", children:[]}`; the Administrator default (from the
  type descriptor's `defaultConfig`) nests:
  `Authenticated > Roles > Administrator` as chained `children` — only the
  chosen leaf goes in `securityLevels` with its full ancestry attached.
- `timestamp` = epoch **milliseconds** at creation.

Response `200` (live-captured):

```json
{ "success": true,
  "changes": [ { "name": "ign-adopt-capture", "type": "ignition/api-token",
                 "collection": "core",
                 "newSignature": "87717b87…" } ],
  "problem": null }
```

### 1c. Idempotent find — `GET /data/api/v1/resources/list/ignition/api-token?limit=20&offset=0&sortBy=asc(name)`

Items carry `name`, `config.profile.securityLevels`,
`config.profile.secureChannelRequired`, `config.settings.tokenHash` (hash
only — never the key), `attributes.uuid`. Adoption = name match in this list.

### 1d. Type descriptor — `GET /data/api/v1/resources/type/ignition/api-token`

`extensionPoints[0] = { typeId: "basic-token", canCreate: true }`;
`defaultConfig.profile` shows the Administrator-level default with
`secureChannelRequired: true`.

### 1e. Live-verified end-to-end

Minted key authenticates immediately with the header `ign` already sends:
`X-Ignition-API-Token: <name>:<key>` → 200 on gateway-info. No propagation
delay.

---

## Step 2 — Permissions wiring

### GET `…/resources/singleton/ignition/security-properties?defaultIfUndefined=true`

(Already modeled in doctor.) `readPermissions`/`writePermissions` are
`{type: "AnyOf", securityLevels: [leaf-paths]}`.

### PUT `…/resources/ignition/security-properties` — the write (live-captured, both variants)

Request body: **array**, whole-singleton replacement config, plus the
`signature` from the last GET (optimistic concurrency — signature rotates on
every write; two live PUTs carried `dee8c946…` then `1555e645…`):

```json
[{ "collection": "core", "enabled": true, "description": "",
   "signature": "<from the GET>",
   "config": { …entire config, with writePermissions edited… } }]
```

The UI also sends `gatewayAuditProfile: null` (absent from GET responses) —
include it or omit it; the server accepted both? (only the with-null variant
live-observed — keep it).

**PITFALLS (live-discovered, all pinned by experiment on the rig):**

1. **The PUT REPLACES the whole config** — `writePermissions` in the body
   overwrites the stored list entirely. Adopt must GET → merge the key's
   level into `readPermissions`+`writePermissions` (add-if-missing, never
   remove others) → PUT.
2. **`securityLevels: []` means PUBLIC, not "nobody"** — an empty AnyOf list
   grants *everyone* write. Checking "Public" in the UI serializes as an
   empty list. Never send `[]` when intending to restrict; conversely, a
   GET-side `[]` means the rig is wide open (this rig was, pre-capture).
3. **Nested permission entries do NOT admit equal-path tokens** (pinned
   2026-09-18, the composition live-run): with
   `writePermissions = [Authenticated>Roles>Administrator]` (the UI's own
   nested serialization, descriptions present AND absent — both tried), a
   token granted exactly `Authenticated/Roles/Administrator` gets **403**
   on writes. With `writePermissions = [{name:"Authenticated",children:[]}]`
   (the BARE ROOT), the same token gets **200**. Only the ancestor (bare
   root) form admits descendant-granted tokens. Consequence for adopt:
   the merge ensures a **bare-root entry** lands (beside any nested
   entry when present — the fresh-gateway default IS the nested form, so
   this is the fix that makes a default gateway's writes work for the
   minted key at all).
4. Node `description` strings are optional in, echoed out — irrelevant
   to matching (tried both ways for pitfall 3).

### Singleton GET response `attributes.lastModification`

`{ actor, timestamp }` + `lastModificationSignature` — useful for adopt's
report row, not needed for the write.

---

## Step 6 — `--bake` round-trip (acceptance PASSED, live-proven)

Sequence executed 2026-09-18 on the 8.3.6 rig:

1. `ign backup download --profile whk-local -o bake-test.gwbk` (roaming,
   3.7 MB, authenticated with the minted key)
2. `ign backup restore … bake-test.gwbk --yes` → 200, gateway 503 during
   restore-boot, back to 200 within ~4 min
3. Post-restore probes, all green:
   - same `X-Ignition-API-Token: ign-adopt-capture:<key>` → **200** (the
     plaintext key survives — the gwbk stores the hash, restore keeps it)
   - `resources/list/ignition/api-token` → 7 items incl. `ign-adopt-capture`
   - security-properties → read/write still `[{name: "Authenticated"}]`

**Conclusion:** a roaming gwbk round-trips API keys AND security-properties.
`rig reset --restore restore.gwbk` after `adopt --bake` is sound — no
"snapshot MEDIUM confidence" caveat remains for these two resources.

---

## What this unlocks in the code (the adopt build order)

1. `client/idp.rs`: reuse `login()` + `GatewaySession` verbatim; add
   `post_json_flow`-style calls for `api-token/generate`, the resources
   POST/PUT (the `trial_reset_via_session` pattern — same session+CSRF tier).
2. Wire models: `GenerateKeyWire {key, hash}`, reuse/extend the resources
   machinery for `ignition/api-token` + the security-properties singleton
   PUT (array-of-one + signature). Regression tests cite this file + the rig.
3. Step 1 idempotency: list-by-name BEFORE generate; if found and
   `secureChannelRequired` is false and level adequate → skip mint.
4. Order: mint → wire permissions (session still open) → probe with the new
   key (gateway-info 200) → routes/testing → bake.

---

## SHIPPED — live verification log (ADOPT-01/02, 2026-09-18, same rig)

`ign adopt` (client/adopt.rs + actions/adopt.rs + CLI) landed against this
research. Every path below live-verified on the 8.3.6 rig:

1. **Mint path, first run** (`ign adopt --profile whk-local`): login OK →
   minted `ign-cli` at Authenticated/Roles/Administrator, secure off →
   permissions SKIP (root-name match) → probe 200 with the fresh key →
   macOS keychain store + profile rewritten to `auth = { keyring }`.
2. **Idempotent re-run**: all five rows SKIP; profile auth untouched.
3. **Keyring resolution**: `ign status --profile whk-local` works with NO
   env vars — the point of the whole verb.
4. **Permissions write path**: writePermissions set to `[]` (the PUBLIC
   encoding — this file's pitfall 2) via a token-PUT; adopt's next run
   merged the Administrator tree back in (`wired … (added)`), verified by
   singleton re-read.
5. **Negative**: wrong `IGNITION_PASSWORD` → exit 5 (`auth_rejected`).
6. **Doctor bug fixed as a by-capture**: the permissions deep-dive read
   `/resources/ignition/security-properties` — 404 as a GET on live 8.3.6.
   Corrected to the singleton route + config-nested parse
   (client/restart.rs + client/mod.rs); live row now shows real wiring.

Not yet in the umbrella (follow-up work): the embedded testing bundle
(D2). Composition (`--project`, `--checkout`, `--bake`) LANDED
(ADOPT-03): live-verified 2026-09-18 — routes (5 deployed, secret
lifecycle reused), checkout (3 projects, re-run skips existing
targets), bake (restore.gwbk 3.7 MB), and the pitfall-3 discovery +
fix that made the composition path work at all (nested-permission 403
→ bare-root merge → token writes 200). MCP: adopt auto-registered by
the catalog walk (verified live: tools/list carries it, 87 tools) —
the env-fallback token riding the JSON result is exactly what agents
need.

## D2 handoff — the embedded testing bundle (next session)

Source: `/Users/pmannion/whiskeyhouse/agentic-ignition-tooling/scripts/scaffold-testing.sh`
(1806 lines, heredoc-embedded files; mirrored in ignition-nvim /
ignition-zed-test plugin copies). The port manifest — 22 of the ~25
files matter for the embedded bundle (skip the 6 `.ignition-stubs/`
editor stubs; they stay a plugin concern):

- 5× `ignition/script-python/testing/<module>/{code.py,resource.json}`
  for runner, assertions, decorators, helpers, reporter (8.1 layout —
  the relocation to the 8.3 resource-folder layout is the patch D2
  retires)
- WebDev routes `com.inductiveautomation.webdev/resources/testing/run/`
  (doGet.py genericized project name, doPost.py generic, config.json,
  resource.json) and `.../testing/tags/` (same four)

Port shape (the webdev bundle precedent): files land under
`crates/ignition-core/webdev/testing/` (or a sibling), embedded via
`include_str!`, shipped by `seam::build_deploy_zip` in the 8.3 layout;
`--testing` on adopt adds the step; the step asserts
`?discover=true` lists ≥1 module before reporting green (the
empty-suite trap). The scaffolder's genericized-vs-generic split
(run/doGet.py carries the project name) becomes a build-time
substitution on the bundle's deploy path.
