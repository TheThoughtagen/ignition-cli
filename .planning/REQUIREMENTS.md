# Requirements: ignition-cli — v1.3 Rig Modules & Declared Discovery

**Defined:** 2026-09-20
**Core Value:** One binary that lets a developer (or an AI agent) fully operate and inspect an Ignition 8.3+ gateway — health, projects, tags, rigs — without opening the gateway webpage or Designer.

**Design spec:** [`research/2026-09-20-rig-module-provisioning-DESIGN.md`](research/2026-09-20-rig-module-provisioning-DESIGN.md) (approved 2026-09-20, commit `2fd00c7`)

## v1.3 Requirements

Requirements for the v1.3 release. Numbering continues v1.1 (v1.0 shipped 44,
v1.1 shipped 25 — see `milestones/`). **14 requirements.**

Note: v1.2 shipped outside the milestone workflow (tag `v1.2.0`, PR #8 —
`ign testing run`, `ign session login`, the e2e Playwright scaffold) and has no
requirements archive. LEDG-01 reconciles that.

### Rig Modules

- [ ] **RMOD-01**: User can declare an optional module for a rig and have `ign` provision it on `rig up` — nothing is installed unless asked for; a rig with no module declaration behaves exactly as it does today
- [ ] **RMOD-02**: `ign` fetches the module as a **signed release artifact** pinned by version, verifies it against the release's published sha256 before use, and caches it keyed by version+digest; a digest mismatch is a hard failure, never a warning
- [ ] **RMOD-03**: Provisioning works offline when the pinned artifact is already cached and verified — no network access required on subsequent `rig up`
- [ ] **RMOD-04**: `ign` injects the module through a **generated compose override** (`compose.ign-modules.yml`), never by mutating the user's compose file and never by copying into a running container; the mount survives `rig up` recreates
- [ ] **RMOD-05**: The override sets the gateway's acceptance variables (`ACCEPT_IGNITION_EULA`, `ACCEPT_MODULE_CERTS`, `ACCEPT_MODULE_LICENSES`) so EULA, certificate, and licence gates clear without a human in the commissioning UI
- [ ] **RMOD-06**: The override is `ign`-owned output — regenerated rather than merged, carrying a header saying so, and removable without touching anything the user wrote
- [ ] **RMOD-07**: The mechanism is a **module registry**, not Git-module-specific — a second module is registered and provisioned through the same seam to prove the abstraction holds

### Git Module Commissioning

- [ ] **GITM-01**: `ign` generates the module's `git.yaml` commissioning file from declared config — repo URI, branch, Ignition project name, inheritance, commissioning import flags, default branch
- [ ] **GITM-02**: **No credential is ever written to generated config.** `ign` resolves the git credential from keyring with an env fallback and wires it via `GATEWAY_GIT_USER_SECRET` / `GATEWAY_GIT_USER_SECRET_FILE`; a `user_password` key is never emitted
- [ ] **GITM-03**: `ign` refuses to emit a config unless **exactly one** project claims `gateway_exportResources`, naming the offending projects — zero means gateway-scoped resources are exported by nobody, two or more means competing projects overwrite each other
- [ ] **GITM-04**: `ign` reports the remaining human steps after provisioning (repo access, credential provisioning, first Designer connection) rather than failing silently or pretending they are done

### Declared Rig Discovery

- [ ] **RDISC-01**: Convention roots move out of the binary and into `[rig]` config — the hardcoded `WHK_HOME_ROOTS` and repo relpaths are gone, and rig discovery consults only what the user declared
- [ ] **RDISC-02**: Zero-config convenience survives for anyone who sets roots up once; a user with no declared roots gets a clear error naming every way to configure a rig, not a silent scan of someone else's directory layout

### Planning Ledger

- [ ] **LEDG-01**: v1.2's shipped work is recorded in `MILESTONES.md` with its requirements archived, so the ledger is continuous before v1.3 phases are numbered

## Out of Scope (v1.3)

| Item | Reason |
|------|--------|
| Repairing `docker/docker-compose.yml` upstream | Different repo (`WhiskeyHouse/ignition-git-module`). That rig is broken three ways — the module path is a *directory*, the image is stock with no developer mode, neither accept var is set. Since v1.3's supported path is signed-release-on-stock-image, deletion is likelier correct than repair. |
| The credential committed upstream | `docker/gw-init/git.yaml` is tracked in the git-module repo containing `user_password: abc123` plus a real name and email against live `WHK01` repos. Needs handling in that repo's history. |
| Unsigned or locally built modules | `ign` will never build, sign, or install an unsigned module. Unsigned requires developer mode set before the first module scan, which forces a custom image — out of scope permanently, not just for v1.3. |
| Module-backed CLI verbs (commit/pull/push through the gateway module) | Would create a second, competing sync story against the existing `ign workspace checkout/status/push`. |
| Auto-detecting that a user "probably wants" the Git module | Contradicts RMOD-01. No heuristic ever adds a module. |

## Key Constraints Carried In

- **Signed artifact source:** `WhiskeyHouse/ignition-git-module` GitHub releases. Current: `Git-2.3.4-signed.modl`, 7,704,578 bytes, `sha256:b74070346e587b1e1c14ff5093cc70625f7bfa14c2d24b24e7a0888a619b33eb`. This is `ign`'s first dependency on a third-party artifact feed — hence RMOD-03.
- **Why the override, not a copy:** `user-lib/modules` lives in the *image*, not the `gateway_data` volume. Anything copied into a running container is destroyed on the next recreate.
- **Why signed changes everything:** a signed module loads on the stock `inductiveautomation/ignition` image. No Maven, no custom build, no developer mode.
