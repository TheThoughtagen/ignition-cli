# Design: opt-in rig module provisioning (Git module first)

**Date:** 2026-09-20
**Status:** Design, awaiting review
**Scope:** `ign` gains the ability to provision an optional, signed Ignition module
into a rig gateway — starting with the Git module — via a generated compose
override. Opt-in only.

---

## 1. Problem

A developer who wants to keep working in the Ignition Designer against a
git-backed project has no supported path through `ign`. Today they must either
clone `ignition-git-module` and run its `docker/test-rig/deploy.sh` (which
builds an **unsigned** module with Maven and requires a custom gateway image
with developer mode baked in), or hand-assemble a compose file.

Neither is something `ign` can offer a user. The unsigned path additionally
forces an image build, because Ignition 8.3 gates an unsigned module on
developer mode being set *before the first module scan* — and the data directory
is a volume seeded from the image, so the flag cannot be appended to a running
container.

## 2. Key enabling fact

**A signed release exists.** `WhiskeyHouse/ignition-git-module` publishes
`Git-<version>-signed.modl` as a GitHub release asset — currently
`Git-2.3.4-signed.modl`, 7,704,578 bytes,
`sha256:b74070346e587b1e1c14ff5093cc70625f7bfa14c2d24b24e7a0888a619b33eb`.

A signed module removes the entire developer-mode constraint. It drops into
`user-lib/modules` on the **stock** `inductiveautomation/ignition` image. No
custom build, no Maven, no developer mode. This is what makes the feature
tractable, and it is the only module artifact `ign` will ever install.

`ign` MUST NOT build, sign, or fetch unsigned modules.

## 3. Principles

1. **Opt-in, never automatic.** A rig comes up without the module unless the
   user asked for it. No discovery heuristic ever adds a module.
2. **Signed releases only**, pinned by version and verified by checksum.
3. **`ign` never mutates files the user owns.** Provisioning is additive, via a
   generated override the user can inspect and delete.
4. **Secrets never land in generated config on disk.**
5. **Guide, don't guess.** Where `ign` cannot complete a step, it says exactly
   what the human must do.

## 4. Architecture

### 4.1 Compose override (D2)

`ign` generates `compose.ign-modules.yml` next to the rig's compose file and
invokes compose with both: `-f <rig compose> -f compose.ign-modules.yml`.

Rationale: `user-lib/modules` lives in the **image**, not the `gateway_data`
volume. Anything copied into a running container is destroyed on the next
`rig up` recreate. A bind mount declared in an override survives recreates,
is visible, is inspectable, and is removable without touching the user's
compose file. Editing the user's compose file was rejected on principle 3;
`docker compose cp` + restart was rejected as silently non-durable.

The override declares, for the gateway service:

- a read-only bind mount of the cached `.modl` into
  `/usr/local/bin/ignition/user-lib/modules/<name>.modl`
- a read-only bind mount of the generated `git.yaml` into
  `/usr/local/bin/ignition/data/git.yaml`
- `ACCEPT_IGNITION_EULA=Y`
- `ACCEPT_MODULE_CERTS=com.axone_io.ignition.git`
- `ACCEPT_MODULE_LICENSES=com.axone_io.ignition.git`
- the secret wiring from §4.4

The override is regenerated, not merged. It is `ign`-owned output and carries a
header saying so.

The mechanism is deliberately **not** Git-module-specific. It is "rig modules";
the Git module is the first and, for now, only registered one.

### 4.2 Artifact fetch and verification

- Resolve the release asset for a **pinned** version from
  `WhiskeyHouse/ignition-git-module`. No floating "latest" by default.
- Verify the downloaded bytes against the release's published sha256 before use.
  A mismatch is a hard, loud failure — never a warning.
- Cache under the `ign` data directory, keyed by version + digest, so repeated
  `rig up` does not re-download.
- Offline behavior: if the artifact is already cached and verified, provisioning
  proceeds with no network access.

### 4.3 Config surface (D3)

Config is the source of truth; a flag covers one-offs.

Declared per rig, naming the module, the pinned version, and the git-backed
projects to commission. A `--with-module` flag provides the same for a single
invocation without writing config.

Exact TOML shape is left to the implementation plan, but it MUST express:
module id, pinned version, and the project list of §4.4.

### 4.4 `git.yaml` generation and the secret flow (D4)

`ign` generates the module's commissioning file. The schema it must emit, taken
from the module's `ProjectConfig`:

`repo_uri`, `repo_branch`, `ignition_projectName`, `ignition_userName`,
`ignition_inheritable`, `ignition_parentName`, `user_name`, `user_email`,
`commissioning_importThemes`, `commissioning_importTags`,
`commissioning_importImages`, `commissioning_enforceBranch`,
`initDefaultBranch`, `gateway_exportResources`.

**`user_password` is never emitted.** The module reads
`GATEWAY_GIT_USER_SECRET` (direct value) first, falling back to
`GATEWAY_GIT_USER_SECRET_FILE` (a path) — see
`GitCommissioningUtils.java:437`. `ign` resolves the credential from keyring,
with an env fallback for CI and agents, matching the existing gateway-token
model, and wires it through one of those two variables. The generated YAML on
disk contains no credential.

**Validation `ign` owns:** `gateway_exportResources` must be true for exactly
one project on a gateway. Zero means gateway-scoped resources (tags, themes,
images) are exported by nobody; two or more means competing projects each write
their own copy of the same resources — a real defect the upstream test rig was
built to reproduce. `ign` refuses to emit a config that is not exactly-one and
explains which projects collided.

### 4.5 Commissioning guidance

The accept variables clear the EULA, certificate, and licence gates
automatically. What remains human work — repo access, credential provisioning,
first Designer connection — is reported as explicit next steps. The
accompanying skill walks the user through selecting the module, choosing a
version, and describing their projects; it does not silently make those choices.

## 5. Error handling

- Checksum mismatch, unreachable release, or unknown pinned version: hard
  failure with the resolved URL and expected digest.
- Missing credential with no keyring entry and no env var: refuse before
  writing anything, naming both resolution paths.
- `gateway_exportResources` violations: refuse, listing the offending projects.
- Docker/compose failures surface through the existing rig error class.

All failures follow the established exit-code contract; no new codes unless the
implementation plan justifies one.

## 6. Testing

- Fetch/verify: digest match, digest mismatch, cache hit, offline-with-cache.
- Override generation: golden file, including regeneration idempotence.
- `git.yaml` generation: golden file; explicit assertion that no
  `user_password` key is ever emitted.
- `gateway_exportResources` validation: zero, one, many.
- Secret resolution: keyring path, env path, neither.
- Live-rig verification of an actual module load is a separate gate, not a unit
  test.

## 7. Out of scope

- **Removing the level-5 WHK convention scan.** Discussed and agreed as a
  problem (hardcoded repo names and home paths in a shipped binary, and the
  source of most WHK references in the codebase), but it is its own change.
- **Repairing `docker/docker-compose.yml` upstream.** That rig is broken three
  ways — `docker/modules/Git-unsigned.modl` is a *directory* rather than a file,
  the image is stock with no developer mode, and neither accept variable is set.
  Since the supported path here is signed-release-on-stock-image, that file is
  likely better deleted than repaired. Upstream repo, upstream PR.
- **The committed credential upstream.** `docker/gw-init/git.yaml` is tracked in
  `ignition-git-module` and contains `user_password: abc123` plus a real name
  and email, pointing at live `WHK01` repositories. Needs handling in that repo.
- **Module-backed CLI verbs.** `ign` will not grow commit/pull/push verbs that
  drive the gateway module; that would compete with the existing
  `ign workspace checkout/status/push` story.
- **Unsigned or locally built modules.** Never supported.

## 8. Decisions locked in discussion

- **D2** — compose override file, not compose-file mutation and not `cp`.
- **D3** — config as source of truth, flag for one-offs, with docs and skill
  guidance.
- **D4** — `ign` generates `git.yaml` and guides its setup; credentials stay out
  of it.
- Opt-in only; signed releases only; pinned and checksum-verified.
