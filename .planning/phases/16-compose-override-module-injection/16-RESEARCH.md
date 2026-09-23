# Phase 16: Compose Override — Module Injection - Research

**Researched:** 2026-09-23
**Domain:** Docker Compose multi-file override generation; Rust registry-pattern generalization over an existing fetch/verify seam
**Confidence:** HIGH (all seven questions answered from either a live `docker compose`/`docker inspect`/GitHub-API check run this session, or a direct read of the exact source lines cited)

## Summary

Phase 16 turns Phase 15's library-only artifact fetch into a rig that actually
boots with the module loaded. The mechanism is a generated, `ign`-owned
second compose file (`compose.ign-modules.yml`) added via a second `-f` flag
on the existing invocation — never a merge into the user's file, never a
container-level `cp`.

The three empirically-verified facts that most shape the plan:

1. **Compose multi-file merge is APPEND for `volumes:` lists, per-key
   MERGE for `environment:` maps.** Verified this session with a real
   `docker compose -f base.yml -f override.yml config` run (Docker Compose
   v5.1.2) — the base rig's two volumes plus the override's one all survive;
   `ACCEPT_IGNITION_EULA` and `GATEWAY_ADMIN_USERNAME` both survive with the
   override's `ACCEPT_IGNITION_EULA=Y` winning. The override's shape can
   safely add a volume entry without touching the base rig's `gateway_data`
   volume, as long as the override uses the **list** form for `volumes:`
   (not a full-map replace at a key that doesn't exist in short syntax —
   moot here since compose already merges list *items* across files by
   position/append, confirmed live, not just per the docs).
2. **A bind mount whose host source file does not exist becomes a
   DIRECTORY, silently, on both host and container.** Verified this
   session with `docker run -v $PWD/nonexistent.modl:/mnt/nonexistent.modl:ro
   alpine stat /mnt/nonexistent.modl` — Docker auto-created a directory at
   both paths with the file's stat blocks reporting `directory`. This is
   the *exact* bug already recorded against `ignition-git-module`'s
   `docker/modules/Git-unsigned.modl` in the roadmap's Phase 16 pitfalls
   note. `ign` MUST refuse to write the override (or refuse `rig up`) unless
   the cached artifact file already exists as a regular file on disk.
3. **The cached `.modl` file Phase 15 persists is mode `0600` (owner-only
   read), which is unreadable by the stock Ignition image's `2003:2003`
   non-root user.** Verified this session in two ways: (a) `docker inspect
   inductiveautomation/ignition:8.3.6 --format '{{.Config.User}}'` →
   `2003:2003`, confirmed non-root; (b) a standalone Rust program compiled
   against the exact `tempfile = "3.27.0"` pin already in this workspace's
   `Cargo.lock`, replicating `NamedTempFile::new_in(...).persist(...)`
   exactly as `module/fetch.rs:399-470` does it — the persisted file comes
   out `mode 100600`. Nothing in `module/fetch.rs` chmods it. A read-only
   bind mount of that file into the container will fail with permission
   denied for uid 2003 **unless Phase 16 widens the permissions** (e.g.
   `0644`) either right after fetch or at override-generation time,
   immediately before `rig up`.

**Primary recommendation:** generate the override as a small, hand-templated
YAML writer (not a new YAML crate — see §Don't-Hand-Roll reasoning under
Q6), using Compose's **long (mapping) volume syntax**
(`type: bind, source:, target:, read_only: true`), never the short
`source:target:ro` string form — the short form's colon collides with a
Windows drive letter (`C:\...`) and this repo's CI matrix includes
`windows-latest`. Widen the cached artifact's permissions to `0644` as part
of provisioning (recommend inside `module/fetch.rs`'s persist step, or a
new Phase 16 step immediately before generating the override — either
place, but it must happen before every `rig up` that provisions a module,
not just the first). Extend `ModuleSpec` with two new fields — a
gateway-facing module id (distinct from the registry's short slug, needed
because the Git module's real id is reverse-DNS while the second module's
is a bare slug) and nothing else; the existing `asset_name`/`tag`
substitution already tolerates a template with no `{version}` token
(verified this session), so RMOD-07's second module needs no branch beyond
one more `pub const` entry in `MODULES`.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Module registry (`ModuleSpec`, `MODULES`) | `ignition-core` (`module/`) | — | Already the seam (Phase 15); Phase 16 adds fields, not a new layer |
| Compose override generation (`compose.ign-modules.yml`) | `ignition-core` (`rig/`, new submodule) | — | Pure function over `RigPlan` + declared modules → YAML string; must be unit-testable without Docker |
| Compose invocation (`-f base -f override`) | `ignition-core` (`rig/compose.rs`) | — | Existing arg builders (`up_args`/`down_args`/…) own every `docker compose` invocation shape; extend, don't bypass |
| Config surface (`[rigs.NAME.modules.*]`) | `ignition-core` (`config/profile.rs`) | — | `RigEntry` already owns rig-scoped declarative config |
| CLI verb / `--with-module` flag / `--json` envelope | `ignition-cli` (`cli.rs`, dispatch) | — | Existing `RigArgs`/`RigCommand` precedent; Phase 15 was deliberately library-only, this is where the CLI surface lands |
| Artifact fetch + verify + cache | `ignition-core` (`module/fetch.rs`) | — | Already built (Phase 15); Phase 16 consumes `FetchedModule`, does not modify the fetch/verify logic itself (only permissions, see Pitfall 3) |
| Live gateway module-load proof | External (Docker + gateway) | `ignition-core` test harness (`#[ignore]`-gated) | Cannot be proven by a unit test; needs the `live_gateway.rs`/`live_module_fetch.rs` `#[ignore]` convention |

## Package Legitimacy Audit

No new external crate is required for this phase's core deliverable (see Q6
for the YAML-library evaluation and why hand-rolling wins). If the planner
elects a YAML crate instead of hand-rolling, the candidates evaluated this
session (live `crates.io` API, 2026-09-23) are:

| Package | Registry | Age / Last Update | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|--------------------|-----------|--------------|---------|-------------|
| `serde_yaml` | crates.io | Last updated 2024-03-25; `max_stable_version` literally tagged `0.9.34+deprecated` | 411M (lifetime) | github.com/dtolnay/serde-yaml | SUS (deprecated upstream, no fixes will land) | NOT RECOMMENDED — deprecated by its own maintainer |
| `serde_norway` (serde_yaml fork) | crates.io | Last updated 2024-12-21 (~21 months stale as of this research date) | 11.7M | github.com/cafkafk/serde-yaml | SUS (stale for a new dependency choice, same reasoning this codebase already applied to reject `figment`) | NOT RECOMMENDED |
| `serde_yml` | crates.io | Description on crates.io is itself `"DEPRECATED — serde_yml is unmaintained... forwards to noyalib"` | 23.7M | github.com/sebastienrousseau/serde_yml | SLOP-adjacent (self-declared deprecated shim redirecting to an unfamiliar crate name; do not add) | REMOVED |
| `saphyr` | crates.io | Updated 2026-09-19 (4 days before this research), v0.1.0 | 2.2M | github.com/saphyr-rs/saphyr | OK, but no serde derive integration — would require manual node construction for a 6-key structure, more code than hand-rolling | Not recommended (adds a dependency for less convenience than hand-rolling) |
| `yaml-rust2` | crates.io | Updated 2026-09-11, v0.13.0 | 59.3M | github.com/Ethiraric/yaml-rust2 | OK, actively maintained, but likewise no serde derive — a parser/emitter over its own `Yaml` enum | Not recommended (same reasoning as saphyr) |

**Packages removed due to [SLOP]-adjacent verdict:** `serde_yml` (self-deprecated, redirects to an unverified crate name — do not chase the redirect without independent verification).
**Packages flagged as suspicious [SUS]:** `serde_yaml`, `serde_norway` — both stale/deprecated; do not add either as a new dependency.

**Recommendation:** hand-roll the override writer (see Q6). If the planner later needs full round-trip YAML *parsing* (not generation) for some other feature, re-run this audit — the landscape shown above is thin and worth re-checking rather than trusting this table beyond this phase.

## Q1 — Compose multi-file merge semantics (the highest-risk unknown)

**RECOMMENDATION:** The override is safe to add a `volumes:` list entry and
`environment:` map entries without risk to the base rig's `gateway_data`
volume or its existing environment — compose **appends** list-valued keys
(`volumes`, `ports`, etc. — confirmed for `volumes` this session) across
`-f` files and **merges per-key** for map-valued keys (`environment`,
`labels`). Use the override strictly additively: only new `volumes:` list
entries and new/overridden `environment:` keys — never touch any key the
base file already sets to a list unless deliberately appending.

**Evidence (live, this session):**

```bash
$ docker compose -f base.yml -f override.yml config --format json
```

Where `base.yml` declared for the `gateway` service:
```yaml
environment:
  GATEWAY_ADMIN_USERNAME: admin
  ACCEPT_IGNITION_EULA: "N"
volumes:
  - gateway_data:/usr/local/bin/ignition/data
  - ./gw-init:/usr/local/bin/ignition/data/init.d
```
and `override.yml` declared:
```yaml
environment:
  ACCEPT_IGNITION_EULA: "Y"
  ACCEPT_MODULE_CERTS: com.example.module
volumes:
  - ./cache/module.modl:/usr/local/bin/ignition/user-lib/modules/module.modl:ro
```

The resolved config (Docker Compose v5.1.2, verified live) showed:
- `environment`: `{"ACCEPT_IGNITION_EULA": "Y", "ACCEPT_MODULE_CERTS": "com.example.module", "GATEWAY_ADMIN_USERNAME": "admin"}` — override's `ACCEPT_IGNITION_EULA` won for that key; `GATEWAY_ADMIN_USERNAME` (only in base) survived untouched.
- `volumes`: **all three entries present** — `gateway_data` volume, `./gw-init` bind, and the override's `./cache/module.modl` bind, all three.

This matches [Docker's official docs](https://docs.docker.com/compose/how-tos/multiple-compose-files/merge/): "Compose Sequence type Sub-Options like `ports`, `expose`, `external_links`, `dns`, `dns_search`, and `tmpfs` are concatenated"; volumes-as-a-service-key follow the same sequence-merge rule. `environment` (a mapping type) follows Compose's "Mapping type" rule: shallow merge, override wins per key.

**Confidence:** HIGH — `[VERIFIED: live docker compose config run, this session, Docker Compose v5.1.2]` for the volume append and env-key merge behavior specifically as exercised above; `[CITED: docs.docker.com/compose/how-tos/multiple-compose-files/merge/]` for the general rule this instance confirms.

## Q2 — Override file placement and invocation

**RECOMMENDATION:** Write `compose.ign-modules.yml` in `plan.project_dir`
(the same directory `RigPlan.compose_file` resolves against), and thread it
as a **second, absolute** `-f` argument appended immediately after the base
compose file in every arg builder that currently issues one `-f`
(`up_args`, `down_args`, `ps_args`, `logs_args` in
`crates/ignition-core/src/rig/compose.rs:299-365`).

Concretely, extend each of those four pure functions with a new parameter
(e.g. `override_files: &[PathBuf]`) appended as `-f <path>` right after the
existing `-f <base file>` and before the subcommand token (`up`/`down`/…).
Passing an **empty slice must reproduce today's exact pinned output byte-
for-byte** — this is what proves RMOD-01's "a rig with no module
declaration behaves exactly as it does today" (SC-1), and the existing
`up_args_pinned`/`down_args_pinned_with_and_without_volumes` tests
(`compose.rs:701-810`) are the direct regression harness: re-run them
unmodified with the new parameter defaulted to `&[]` and they must still
pass unchanged.

**`--project-directory` / `.env` resolution does NOT change.** Per
[Docker's compose CLI reference](https://docs.docker.com/reference/cli/docker/compose/):
"The project directory is determined by `--project-directory` if set,
otherwise the directory of the first `-f`/`--file` Compose file, otherwise
PWD." Since `up_args`/`down_args`/`ps_args`/`logs_args` already omit
`--project-directory` entirely (only `config_args` — the resolve step —
passes it, per `compose.rs:279-293`) and the override is always the
**second** `-f` argument, never the first, adding it changes nothing about
where `.env`/`COMPOSE_PROJECT_NAME` is read from. This was the single
highest-value check the phase note flagged and it is a clean pass: no
regression.

`rig/compose.rs`'s `DockerCompose::spawn` (line 96-119) does **not** set
`.current_dir()` on the spawned process — the child inherits `ign`'s own
cwd. Combined with the fact that `RigPlan.compose_file` can be a relative
path when discovered via the cwd-candidate scan (`resolve_file`, `rig/
mod.rs:272-281`, builds `PathBuf` from `env.cwd.join(candidate)` which IS
absolute, but the `[rigs.NAME]` config path, `rig/mod.rs:256` `expand_path
(&entry.compose_file)`, stays relative if the user wrote a relative
`compose_file` in TOML), **the override's own path must always be written
as an absolute path** in the arg vector, independent of whatever
`plan.compose_file` happens to be. Do not rely on cwd matching
`plan.project_dir` at `up`-time — compute `override_path =
plan.project_dir.join("compose.ign-modules.yml")` and pass that (joined,
not just the bare filename) as the `-f` value.

**Confidence:** HIGH for the merge/order/`--project-directory` claims
`[CITED: docs.docker.com/reference/cli/docker/compose/]`; HIGH for the
codebase claims `[VERIFIED: crates/ignition-core/src/rig/compose.rs:96-119,
279-365; crates/ignition-core/src/rig/mod.rs:251-281]` (all read directly
this session, line ranges above).

## Q3 — Durability across recreate (SC-3)

**RECOMMENDATION:** Durability is structural, not something that needs a
live rig to prove for the *mechanism* — it needs a live rig only to prove
the *gateway actually loads the module*. Split the test into two tiers,
matching this codebase's existing `#[ignore]`-gated live-test convention
(`tests/live_gateway.rs`, `tests/live_module_fetch.rs`):

1. **Mechanism-level (no live gateway, CI-safe):** `[VERIFIED: docker
   inspect inductiveautomation/ignition:8.3.6 --format
   '{{json .Config.Volumes}}'` → `null`, this session]` — `user-lib/modules`
   is **not** declared as an image `VOLUME`, confirming the design doc's
   claim that it is plain image filesystem content, never a Docker-managed
   volume that would need special handling. Because the override's mount is
   a **bind mount from the host's cache directory** (which is untouched by
   `compose down`/`compose up` — it lives outside any Docker-managed
   storage), the mount is trivially durable as long as (a) the override
   file itself persists on disk between `down` and `up` (it does — `ign`
   only regenerates it, never deletes it as a side effect of `down`), and
   (b) the next `up` still passes `-f compose.ign-modules.yml`. A
   Docker-free integration test (spawn a scripted `ComposeRunner` fake,
   assert the SAME override `-f` argument appears in both the `down_args`
   and subsequent `up_args` call sequence) proves this without Docker,
   following the exact pattern `actions/rig.rs`'s existing `FakeRunner`
   tests already use (`actions/rig.rs:1021-1483`).
2. **Live-rig tier (the actual test, `#[ignore]`-gated):** bring a rig up
   with a declared module, confirm the module loaded (via the gateway's
   `/data/api/v1/...` module-list endpoint, or the existing `GatewayApi`
   surface if it already exposes one — check during planning), run `ign rig
   down` then `ign rig up` again, and re-confirm the module is still
   loaded. This is SC-3's actual proof and cannot be faked; it belongs
   alongside `live_module_fetch.rs`'s existing opt-in convention.

**Confidence:** HIGH for the `Config.Volumes: null` fact `[VERIFIED: docker
inspect, this session]`; HIGH for the mechanism argument (bind mounts are
host-filesystem-backed and outlive container lifecycle by construction —
standard Docker behavior, `[CITED: docs.docker.com — bind mounts]`); the
live-rig test design is a RECOMMENDATION, not yet verified against a real
gateway's module-list API — flagged below as an open question.

## Q4 — Registry generalization (RMOD-07)

**RECOMMENDATION:** `ModuleSpec` (`crates/ignition-core/src/module/mod.rs:24-38`)
needs exactly **one** new field beyond what Phase 15 shipped: a
gateway-facing module id, distinct from the registry's filesystem-safe
`id` slug. Everything else the second module's four stress points exercise
already works with zero branching:

- **Different org (`bw-design-group/ignition-project-scan-endpoint` vs
  `WhiskeyHouse/ignition-git-module`):** `repo: &'static str` already
  carries an arbitrary `owner/repo` string — no change needed.
- **Asset filename carries no version** (`Project-Scan-Endpoint.modl`
  vs `Git-{version}-signed.modl`): `[VERIFIED: standalone rustc compile
  this session]` — `"Project-Scan-Endpoint.modl".replace("{version}",
  "1.0.0")` returns the string unchanged (`str::replace` on a pattern with
  no match is a documented no-op), so `ModuleSpec::asset_name` already
  handles a fixed filename with **zero code change**. Set
  `asset_template: "Project-Scan-Endpoint.modl"` and it works exactly like
  the versioned case.
- **Bare-slug module id (`project-scan-endpoint`) vs reverse-DNS
  (`com.axone_io.ignition.git`):** `[VERIFIED: downloaded, unzipped, and
  read module.xml directly this session]` — `<id>project-scan-endpoint
  </id>` confirmed. This is the field Phase 15's `ModuleSpec` does NOT yet
  carry: the design doc's example (§4.1) sets `ACCEPT_MODULE_CERTS=
  com.axone_io.ignition.git` and `ACCEPT_MODULE_LICENSES=
  com.axone_io.ignition.git` — that value is the Git module's REAL
  (reverse-DNS) gateway id, not `GIT_MODULE.id` (`"git"`, the short
  registry/cache-dir slug). The two modules' ids happen to look similar in
  format for the second module (`project-scan-endpoint` could plausibly be
  used as both the registry slug AND the accept-var value) but they are
  NOT guaranteed to coincide in general — the Git module already proves
  they diverge. Add:
  ```rust
  pub struct ModuleSpec {
      pub id: &'static str,              // existing — registry/cache-dir slug
      pub repo: &'static str,             // existing
      pub tag_template: &'static str,     // existing
      pub asset_template: &'static str,   // existing
      pub gateway_module_id: &'static str, // NEW — the ACCEPT_MODULE_CERTS/LICENSES value, from module.xml's <id>
  }
  ```
  `GIT_MODULE.gateway_module_id = "com.axone_io.ignition.git"` (design
  doc §4.1, not independently re-verified this session — the design doc
  states it directly and Phase 17's research references
  `ProjectConfig.java`; treat as `[CITED: design doc §4.1]` unless the
  planner wants it re-confirmed against the actual Git module's
  `module.xml`). `PROJECT_SCAN_ENDPOINT.gateway_module_id =
  "project-scan-endpoint"` `[VERIFIED: module.xml, this session]`.
- **30 KB vs 7.7 MB:** nothing in `fetch.rs` assumes a size — the size cap
  (`fetch.rs:419-433`) compares the running byte count against the
  release's own published `asset.size`, which is fetched dynamically per
  release. No change needed.

**What would break if a third module arrived:** anything whose target
mount filename inside `user-lib/modules` needs to differ from a
deterministic `<id>.modl` derivation (recommend `format!("{}.modl",
spec.id)` — `id` is already validated to the safe `[a-z0-9-]{1,32}`
charset by `validate_module_id`, so it's already filesystem/URL-safe and
requires no new validation). If a future module ships **more than one**
loadable `.modl` in a single release (a multi-module bundle), the
one-`ModuleSpec`-per-artifact assumption breaks — flag this as an open
question for the planner rather than pre-building for it, since neither of
the two modules in scope needs it.

**Confidence:** HIGH — every claim in this section is independently
`[VERIFIED]` this session except the Git module's exact
`gateway_module_id` string, which is `[CITED: design doc §4.1]` (the design
doc itself cites `GitCommissioningUtils.java` / `ProjectConfig.java` for
the surrounding schema but the `com.axone_io.ignition.git` value itself
reads as asserted, not sourced to a specific line in that doc).

## Q5 — Opt-in config surface (RMOD-01)

**RECOMMENDATION:** Extend `RigEntry` (`crates/ignition-core/src/config/
profile.rs:114-123`) with a new, `BTreeMap`-keyed, default-empty field:

```rust
pub struct RigEntry {
    pub compose_file: String,                    // existing
    pub project_name: Option<String>,             // existing
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub modules: BTreeMap<String, ModuleDeclaration>,  // NEW — keyed by ModuleSpec::id
}

pub struct ModuleDeclaration {
    pub version: String,   // pinned version string, validated by validate_version
}
```

TOML shape (matching this codebase's existing flat-table style, e.g.
`[rigs.known-rig]` seen live in `crates/ignition-cli/tests/contract_rig.rs:
127-131`):

```toml
[rigs.dev]
compose_file = "docker/compose.yml"

[rigs.dev.modules.git]
version = "2.3.4"

[rigs.dev.modules.project-scan-endpoint]
version = "1.0.0"
```

This follows the `#[serde(default, skip_serializing_if = "...is_empty")]`
convention this codebase already applies to `Config.rigs` itself
(`profile.rs:27-31`) and to `RigConfig` (`profile.rs:96-100`) — an omitted
`modules` table round-trips byte-identically, which is the mechanism that
makes SC-1 provable: a config saved/loaded with no `[rigs.NAME.modules.*]`
tables produces the exact same TOML on disk as today.

**The one-off flag** (D3's `--with-module`): recommend `--with-module
<id>@<version>` on `RigCommand::Up` (and `Reset`, since `rig_reset` also
calls `up_args` internally at `actions/rig.rs:351`). The `@` separator is
unambiguous because `validate_module_id`'s charset (`[a-z0-9-]`) and
`validate_version`'s charset (`[A-Za-z0-9._+-]`) both exclude `@` — a
single `rsplit_once('@')` (or `split_once`, either is unambiguous given
neither half can contain `@`) cleanly separates id from version, and
`validate_module_id`/`validate_version` are already there to refuse a
malformed value with the established exit-2 `invalid_input` shape.

**Regression proof recommendation (SC-1, CRITICAL):** the exact scenario
this codebase already tests at `contract_rig.rs:127-155` — a `[rigs.
known-rig]` entry with `compose_file` only, no `modules` table — must
continue producing the exact same `up_args`/`down_args` output vector it
does today. Add a dedicated test asserting `up_args(&plan_with_no_modules,
300)` (or whatever the new signature becomes) equals the CURRENT pinned
vector in `compose.rs:721-737`, byte for byte, with the new
`override_files` parameter fed `&[]`. This is a stronger proof than "no
error was thrown" — it's a literal byte-identity assertion, matching this
codebase's existing "pinned invocation shape" testing philosophy.

**Confidence:** HIGH for the existing-code claims `[VERIFIED: crates/
ignition-core/src/config/profile.rs:27-123; crates/ignition-cli/tests/
contract_rig.rs:127-155]`; MEDIUM for the TOML shape and `--with-module`
flag design — these are RECOMMENDATIONS, not yet locked by the design doc
(§4.3 explicitly leaves "exact TOML shape... to the implementation plan").
Flagged as `[ASSUMED]` in the sense that the planner/user should confirm
the map-of-modules-per-rig shape versus a simpler single-module-per-rig
field, given the design doc's singular phrasing ("the module") — see Open
Questions.

## Q6 — Override generation + idempotence (RMOD-06)

**RECOMMENDATION:** Hand-roll a minimal, templated YAML writer rather than
add a YAML-serialization crate. Reasoning:

1. **No actively-maintained serde-integrated YAML crate exists right now**
   (see Package Legitimacy Audit above — `serde_yaml` is self-deprecated,
   its most visible fork is 21 months stale, and the ecosystem's actively
   maintained YAML crates — `saphyr`, `yaml-rust2` — have no `#[derive
   (Serialize)]` story, meaning either choice requires roughly the same
   amount of manual node-construction code as hand-rolling the six known
   keys directly).
2. **This is generation of a small, fully-controlled subset of YAML from
   already-validated inputs**, not parsing of hostile external data — a
   fundamentally different risk profile than the CSV-quoting problem this
   codebase already declined to hand-roll (11-03's `csv` crate decision).
   Every value that reaches the template is either already run through
   `validate_module_id`/`validate_version` (safe charset, no YAML special
   characters) or a fixed literal (`"Y"`, `read_only: true`) or an absolute
   filesystem path.
3. This matches the exact reasoning this codebase already used to justify
   hand-rolled TOML config over the `config` crate (Tech Stack §8: "small,
   typed, and few... no framework needed").

**Shape recommendation:** use Compose's long (mapping) syntax for the
volume entry — never the short `source:target:mode` string — because the
short form's colon separator collides with a Windows drive letter
(`C:\Users\...`). `[CITED: forums.docker.com — "the root cause is
colon-based source extraction that splits on the first colon regardless,
so for Windows paths like C:\data:/var, the source becomes just the single
letter 'C'"; the long syntax's explicit `type: bind`/`source:`/`target:`
fields avoid the ambiguity entirely]`. Sketch:

```yaml
# Generated by ign — DO NOT EDIT. Deleting this file fully reverts module
# provisioning for this rig. Regenerated on every `ign rig up`/`reset`
# that declares a module; never hand-merged.
services:
  gateway:
    environment:
      ACCEPT_IGNITION_EULA: "Y"
      ACCEPT_MODULE_CERTS: com.axone_io.ignition.git
      ACCEPT_MODULE_LICENSES: com.axone_io.ignition.git
    volumes:
      - type: bind
        source: /Users/dev/.cache/ignition-cli/modules/git/2.3.4-<digest>.modl
        target: /usr/local/bin/ignition/user-lib/modules/git.modl
        read_only: true
```

**Byte-stable regeneration:** serialize the declared modules in a
deterministic order (sort by `ModuleSpec::id`, which is already how a
`BTreeMap<String, ModuleDeclaration>` config field would iterate — see Q5)
and write with `\n` line endings and no trailing whitespace variance.
Recommend the generation function be a pure `fn generate_override(plan:
&RigPlan, declared: &[(ModuleSpec, FetchedModule)]) -> String` unit-tested
with a literal expected-string assertion (this codebase's existing
"pinned" convention — no `insta` or other snapshot-testing crate is present
anywhere in the workspace, confirmed by grep this session; every existing
"golden" test is a hand-written literal `assert_eq!`). Idempotence test:
call the function twice with identical inputs, assert byte-identical
output; call it with the module DECLARATION REMOVED and assert (a) `ign`
deletes the override file entirely (not writes an empty/near-empty one —
"removable without touching anything the user wrote" per RMOD-06 means the
override's mere absence is the revert-to-today state) and (b) a subsequent
`up_args`/`down_args` call reverts to the exact SC-1 pinned vector.

**Confidence:** HIGH for the crate-landscape and Windows-colon facts
(both `[VERIFIED]`/`[CITED]` this session); the generation-function shape
and byte-stability approach are RECOMMENDATIONS consistent with existing
codebase conventions, not yet locked.

## Q7 — CLI surface

**RECOMMENDATION:** Wire module provisioning into the EXISTING `RigCommand::
Up` and `RigCommand::Reset` handlers (`crates/ignition-cli/src/cli.rs:
1072-1131`) rather than adding a standalone `ign rig module ...` verb tree —
RMOD-01's requirement is "provision it on `rig up`", not a separate
lifecycle. Add:

```rust
Up {
    #[arg(long, default_value_t = 300, value_name = "SECS")]
    timeout: u64,
    /// One-off module declaration for this invocation only, bypassing
    /// config — `id@version` (e.g. `git@2.3.4`); repeatable
    #[arg(long, value_name = "ID@VERSION")]
    with_module: Vec<String>,
},
```
(and identically on `Reset`, since it shares the same `up_args` call path
at `actions/rig.rs:351`).

`RigUpResult` (`actions/rig.rs:60-76`) should carry the provisioning
outcome as new, always-present keys (following this struct's existing
"all keys always present" convention, e.g. `provisioned_modules:
Vec<ProvisionedModule>` where each entry names `id`, `version`, `source`
(`cache`/`download` — reusing Phase 15's existing `ArtifactSource` enum
verbatim) — this rides the existing `--json` envelope with zero new
top-level shape.

**Exit code / slug:** reuse the existing taxonomy rather than inventing a
new exit-code bucket. `[VERIFIED: crates/ignition-core/src/error.rs:774-818
(exit_code match), 1597-1650 (Three-Place EXIT_SLUG_LITERALS table)]` — the
mapping is a hard 7-class taxonomy machine-checked against the README by
`readme_exit_table_agreement` (`error.rs:1696` onward), so any new variant
MUST land in an EXISTING exit-code bucket and be added to
`EXIT_SLUG_LITERALS` (and the README's `## Exit codes` table) in lockstep,
or the test fails. Candidates by precedent:
- **A declared module id with no matching registry entry** — this is a
  config/usage-time "known-things" lookup failure, structurally identical
  to `ProfileNotFound { name, known }` `[VERIFIED: error.rs:133]` (exit 3).
  Recommend a parallel `ModuleNotRegistered { id: String, known: Vec
  <String> }`, exit 3, slug `module_not_registered` — NOT exit 7 (`Rig`),
  which this codebase currently reserves for actual Docker/compose
  operational failures (`Self::Rig(_) => 7`, `error.rs:817`), and NOT exit
  2 (`InvalidInput`), which is reserved for hostile/malformed input shape
  (already used by `validate_module_id`/`validate_version` themselves) —
  a well-formed id that's simply not registered is a different failure
  class than a malformed one.
- **The cached artifact directory-instead-of-file pitfall (Pitfall 1
  below)** and **the mode-0600-unreadable-by-container pitfall (Pitfall 2
  below)** are both pre-flight refusals `ign` can catch before ever
  invoking compose — recommend folding these into the existing `Rig(_)`
  class (exit 7) since they ARE rig-provisioning-time failures in the same
  family as "docker compose failed", per the design doc's §5 error-handling
  note ("Docker/compose failures surface through the existing rig error
  class"), OR a new slug if the plan wants them distinguishable in `--json`
  output. Flagged as an open question for the planner — either choice is
  internally consistent with existing precedent, and the phase design doc
  explicitly says "no new codes unless the implementation plan justifies
  one" (§5).

**Confidence:** HIGH for all cited line ranges and the exit-code taxonomy
mechanics; MEDIUM for the specific new-variant proposal (a RECOMMENDATION,
not a locked decision — the planner should confirm against the design
doc's "no new codes unless justified" framing before adding
`ModuleNotRegistered`).

## Pitfalls

### Pitfall 1: A missing cached artifact silently becomes a host+container directory
**What goes wrong:** if `ign` writes the override (or invokes `compose up`)
without first confirming the cached `.modl` path is a regular file that
exists, Docker creates a directory at that path on BOTH the host and inside
the container — the gateway boots successfully but the module never loads,
with no error surfaced anywhere in compose's own output.
**Why it happens:** Docker's bind-mount semantics silently create the
missing path as a directory rather than failing the mount. `[VERIFIED: `
`docker run -v <missing-file-path>:/mnt/x:ro alpine stat /mnt/x` this`
`session — reports "directory", not "No such file"]`.
**How to avoid:** before writing/using the override, `ign` MUST confirm
`std::fs::metadata(&cached_path)?.is_file()` (not merely `.exists()` — a
prior accidental directory-creation would make `.exists()` true but
`.is_file()` false) for every declared module's `FetchedModule.path`, and
refuse loudly if not. This is exactly the upstream bug already on record
for `ignition-git-module`'s `docker/modules/Git-unsigned.modl` (roadmap
Out-of-Scope table) — Phase 16 must not reintroduce it.
**Warning signs:** a rig comes up RUNNING (gateway healthy) but the module
never appears in the gateway's module list; `docker exec` into the
container shows `user-lib/modules/<id>.modl` as a directory with 0 files
inside it.

### Pitfall 2: Cached artifact permissions (0600) unreadable by the non-root gateway user
**What goes wrong:** the module file mounts read-only but the gateway
process (uid 2003) cannot open it — Ignition's module scan either silently
skips an unreadable file or logs a permission error the user has no reason
to associate with `ign`.
**Why it happens:** `[VERIFIED: standalone compile against this repo's`
`exact tempfile = "3.27.0" pin, replicating module/fetch.rs:399-470's`
`NamedTempFile::new_in().persist() sequence, this session]` — the
persisted file comes out mode `0600`; nothing in `fetch.rs` widens it.
`[VERIFIED: docker inspect inductiveautomation/ignition:8.3.6 --format`
`'{{.Config.User}}'` → `2003:2003`, this session]`.
**How to avoid:** widen the cached file's mode (e.g. `0644`) either as part
of `fetch.rs`'s persist step (would also fix this for any other future
consumer of the cache) or as an explicit pre-flight step in Phase 16 right
before generating the override, every time — not just on first fetch,
since a `Refresh`/`AcceptUpstreamChange` re-download (Phase 15's
`FetchPolicy`) re-persists through the same `0600` code path.
**Warning signs:** SC-2's live-rig verification (module LOADED) fails with
no compose-level error; the gateway's own logs show a module-scan
permission failure for the specific `.modl` path.

### Pitfall 3: `-f` order accidentally reversed, or override becomes authoritative
**What goes wrong:** if the override is ever passed as the FIRST `-f`
argument (or the only one, by a bug in argument construction), Compose's
merge-precedence rule means the override's (deliberately minimal) service
definition could be read as the BASE, and the real rig's full service
definition would apply ON TOP — inverting which file "wins," and
potentially changing `--project-directory` inference (Q2) since that's
keyed to the FIRST `-f` file.
**Why it happens:** easy to get backwards when building the arg vector by
hand across four different builder functions (`up_args`/`down_args`/
`ps_args`/`logs_args`).
**How to avoid:** a single shared helper that always appends override
paths AFTER the base file argument, used by all four builders — never four
independent implementations that could drift. Pin the exact order with a
literal `assert_eq!` test per builder (this codebase's existing
convention), including a two-module case to prove ordering is stable
across N declared modules, not just one.
**Warning signs:** `docker compose config` (run manually) shows the base
rig's `gateway_data` volume missing, or `.env` values differ from what the
resolve step (`config_args`) reported.

### Pitfall 4: A stale override survives after a module is undeclared
**What goes wrong:** user removes `[rigs.dev.modules.git]` from config, but
the previously-generated `compose.ign-modules.yml` is left on disk (or is
regenerated as an empty-but-present file); the next `up_args` call still
passes `-f compose.ign-modules.yml` (file exists → path is built into the
args unconditionally) and Compose either errors on an empty file or
silently keeps applying the old override.
**Why it happens:** "regenerated, not merged" (RMOD-06) is easy to
implement as "always write a (possibly empty) file" rather than "write
when modules are declared, DELETE when none are declared."
**How to avoid:** the override write path must be a tri-state operation:
declared modules present → write/overwrite; no modules declared → delete
the file if it exists (idempotent `std::fs::remove_file` tolerating
`NotFound`), and never pass a `-f` for a file `ign` didn't just confirm it
either wrote or intentionally left absent this run.
**Warning signs:** `ign rig up` succeeds with no module declared, but the
gateway still has a module loaded from a prior session; `compose.ign-
modules.yml` exists on disk with no corresponding `[rigs.NAME.modules.*]`
config.

### Pitfall 5: Gateway ignores a module because the accept-var names the wrong id format
**What goes wrong:** `ACCEPT_MODULE_CERTS`/`ACCEPT_MODULE_LICENSES` must
name the module's REAL gateway id (reverse-DNS for Git, bare slug for the
second module) — if `ign` uses the registry's short `id` slug (`"git"`)
instead of the real gateway id (`"com.axone_io.ignition.git"`) for the Git
module specifically, the accept vars silently fail to match and the
gateway re-prompts for cert/license acceptance in the commissioning UI
(defeating RMOD-05's "clears without a human").
**Why it happens:** exactly the trap Q4 identifies — the two ids LOOK the
same in format for the second module (`project-scan-endpoint` could be
used for both roles and nothing would appear wrong in testing), masking
that they are NOT the same field conceptually, and a plan that tests only
against the second module would never catch the Git module regressing.
**How to avoid:** the new `gateway_module_id` field (Q4) must be used for
`ACCEPT_MODULE_CERTS`/`ACCEPT_MODULE_LICENSES` and the mount's INTERNAL
target-path basename should independently use `id` (or a fixed `<id>.modl`
convention) — never conflate the two fields even when they happen to be
equal for one module. A test asserting the two fields differ for
`GIT_MODULE` specifically (`gateway_module_id != id`) is a good regression
guard against silently making them the same field later.
**Warning signs:** live-rig SC-2 verification shows the module NOT loaded
even though the file mounted correctly and is readable; the gateway's
commissioning UI still shows an unaccepted cert/license prompt for a
module `ign` believes it already accepted.

### Pitfall 6: Windows path handling in the generated compose file
**What goes wrong:** a Windows host path like `C:\Users\dev\AppData\
Local\ignition-cli\cache\modules\git\2.3.4-<digest>.modl` used in Compose's
SHORT volume syntax (`source:target:mode`) gets its `source` truncated to
the single drive letter `C`, because compose splits on the first colon.
**Why it happens:** `[CITED: forums.docker.com/t/docker-compose-bind-`
`mount-with-colon-comma-in-path-not-working — "root cause is colon-based`
`source extraction that splits on the first colon regardless"]`. This
CI's matrix explicitly includes `windows-latest`
`[VERIFIED: .github/workflows/ci.yml:9-15, this session]`.
**How to avoid:** already addressed by the Q6 recommendation — use
Compose's LONG (mapping) volume syntax
(`type: bind, source: ..., target: ..., read_only: true`) unconditionally,
which sidesteps colon-splitting entirely regardless of host OS. Golden
tests for override generation should include a literal Windows-style
source path in at least one test case to prove the long-syntax choice
actually avoids the bug (a short-syntax golden would not catch this).
**Warning signs:** `docker compose config` on a Windows contributor's
machine reports an "invalid volume specification" or resolves the source
to a single-letter path.

### Pitfall 7: Cache root or override path is relative, breaking the bind mount at a different cwd
**What goes wrong:** `IGNITION_CLI_CACHE` (Phase 15's cache-root env
override, used by tests) or a user-configured relative `compose_file`
combine to produce a non-absolute path fed into the override's `source:`
field; Compose then resolves that relative path against
`--project-directory`/cwd at `up`-time, which may differ from where it was
computed at provisioning-time.
**Why it happens:** neither `cache_root()` (`module/mod.rs:120-129`) nor
`RigPlan.compose_file` (`rig/mod.rs:256`, via `expand_path`) is guaranteed
absolute in every configuration path — `IGNITION_CLI_CACHE` could
theoretically be set to a relative value, and a `[rigs.NAME]` TOML entry's
`compose_file` is not canonicalized.
**How to avoid:** canonicalize (or explicitly assert-absolute-and-refuse-
if-not) both the cached artifact's `source:` path and the override file's
own `-f` path before ever writing them, rather than trusting whatever
`FetchedModule.path`/`RigPlan.compose_file` happen to already be.
**Warning signs:** the override works from one working directory and
silently mounts the wrong (or a nonexistent, triggering Pitfall 1) path
from another.

## Runtime State Inventory

Not applicable — Phase 16 is new capability (a new generated file type),
not a rename/refactor/migration of existing runtime state. No prior `ign`
version ever wrote `compose.ign-modules.yml` or any `[rigs.NAME.modules.*]`
config, so there is nothing pre-existing to migrate. Confirmed by grep this
session: `[VERIFIED: grep -rn "compose.ign-modules\|rigs\..*\.modules" crates/`
`— zero matches outside this research file]`.

## Code Examples

### Pure arg-builder extension pattern (recommended shape, not yet implemented)
```rust
// Source: pattern derived from crates/ignition-core/src/rig/compose.rs:299-312 (up_args, read this session)
pub fn up_args(plan: &RigPlan, wait_timeout_s: u64, override_files: &[PathBuf]) -> Vec<String> {
    let mut args = vec![
        "-p".into(), plan.name.clone(),
        "-f".into(), plan.compose_file.display().to_string(),
    ];
    for path in override_files {
        args.push("-f".into());
        args.push(path.display().to_string());
    }
    args.extend([
        "up".into(), "-d".into(), "--wait".into(),
        "--wait-timeout".into(), wait_timeout_s.to_string(),
        "--remove-orphans".into(),
    ]);
    args
}
```
An empty `override_files` slice reproduces the exact vector
`up_args_pinned` already asserts at `compose.rs:721-737` — verify this by
running that exact test unmodified against the new signature (with `&[]`)
before adding any module-declaring test case.

### Registry entry pattern for a second module (recommended shape)
```rust
// Source: pattern derived from crates/ignition-core/src/module/mod.rs:60-67 (GIT_MODULE, read this session)
pub const PROJECT_SCAN_ENDPOINT: ModuleSpec = ModuleSpec {
    id: "project-scan-endpoint",
    repo: "bw-design-group/ignition-project-scan-endpoint",
    tag_template: "v{version}",                 // confirmed live: v1.0.0 tag exists
    asset_template: "Project-Scan-Endpoint.modl", // no {version} token — confirmed this is a no-op substitution
    gateway_module_id: "project-scan-endpoint",   // confirmed via module.xml <id>, this session
};

pub const MODULES: &[ModuleSpec] = &[GIT_MODULE, PROJECT_SCAN_ENDPOINT];
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|---------------|--------|
| `serde_yaml` as the default Rust YAML crate | No actively-maintained serde-derive YAML crate exists | `serde_yaml` deprecated 2024-03-25; no successor has achieved parity | Any new Rust project needing YAML *generation* of a small known shape should default to hand-rolling or a non-serde emitter, not assume a serde-yaml-shaped crate is still the answer |
| Compose short volume syntax as default | Long (mapping) syntax recommended whenever host paths might be Windows | Ongoing community pain point (multiple open GitHub issues cited above), not a recent change | Directly load-bearing for this phase given the `windows-latest` CI matrix entry |

**Deprecated/outdated:** `serde_yaml` — deprecated by its own maintainer, `max_stable_version` literally tagged `+deprecated` on crates.io.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|----------------|
| A1 | `GIT_MODULE`'s real gateway module id is `com.axone_io.ignition.git` | Q4, Q7, Pitfall 5 | If wrong, `ACCEPT_MODULE_CERTS`/`ACCEPT_MODULE_LICENSES` for the Git module will not match and RMOD-05 silently fails for the module the milestone cares about most; low-cost to re-verify (download the real `Git-2.3.4-signed.modl` already cached from Phase 15 and read its `module.xml`, same technique used on the second module this session) |
| A2 | A map-of-modules-per-rig (`[rigs.NAME.modules.*]`, N declarable) is the right config shape, vs. a single-module-per-rig field | Q5 | If the design intent was strictly one module per rig, a map adds unused flexibility (low risk) but also unused test surface; if the design intent assumed multiple simultaneous modules are common, a single-field shape would need a breaking config migration later. The design doc phrases this ambiguously ("the module", singular, §4.3) — recommend confirming with the user during `/gsd-discuss-phase` rather than treating either shape as locked |
| A3 | Folding the directory-instead-of-file and permission pre-flight refusals into the existing `Rig(_)` exit-7 class (vs. new slugs) | Q7 | Low risk either way — both choices are internally consistent with existing precedent and the design doc explicitly permits either ("no new codes unless justified"); affects only how granular `--json` error output is, not correctness |
| A4 | Widening the cached artifact to `0644` is safe/sufficient (vs. e.g. `0640` + group ownership, or chowning to uid 2003) | Pitfall 2 | If `0644` still doesn't satisfy some stricter container security posture (e.g., a hardened base image later), the fix is a one-line mode-constant change; low risk, easy to detect (SC-2's live-rig gate would fail loudly) |

## Open Questions

1. **Is `com.axone_io.ignition.git` definitely the Git module's real gateway
   id?**
   - What we know: the design doc states it directly in the same section
     (§4.1) that specifies the accept-var shape; Phase 15's cached artifact
     (`Git-2.3.4-signed.modl`) is already on this machine's cache from the
     live fetch test recorded in `15-02-SUMMARY.md`.
   - What's unclear: I did not personally re-extract and read that
     specific module's `module.xml` this session (I did for the SECOND
     module, confirming the technique works) — only the design doc's
     assertion backs this one.
   - Recommendation: a 30-second check before the plan locks — unzip the
     cached `.modl` and grep `<id>` the same way this research did for the
     second module.

2. **Single-module-per-rig vs. map-of-modules-per-rig config shape (A2).**
   - What we know: RMOD-07 requires the REGISTRY to support ≥2 modules; it
     does not explicitly require any one RIG to run ≥2 modules
     simultaneously — the "second module" could equally be proven via a
     second `[rigs.NAME2]` entry using a different module, never needing
     more than one module per rig at a time.
   - What's unclear: whether real usage wants multiple modules on one rig
     concurrently (plausible — a git-backed dev rig might eventually also
     want a diagnostics module) or whether that's out of scope for v1.3.
   - Recommendation: default to the map shape (Q5) since it costs nothing
     extra in code and naturally supports both the single- and multi-
     module case, but flag for `/gsd-discuss-phase` confirmation given the
     design doc's singular phrasing.

3. **What does a "module loaded" check look like against a live gateway for
   the SC-2/SC-3 live-rig test?**
   - What we know: Ignition 8.3's gateway REST/WebDev surface is already
     the substrate `ign`'s `GatewayApi` wraps for everything else in this
     codebase.
   - What's unclear: whether an existing, already-wired endpoint exposes a
     module list (and its loaded/faulted state) that `ign`'s test harness
     can assert against, or whether this needs a new curated diagnostics
     route (Phase 9's `api call` passthrough precedent) added as part of
     this phase's test infrastructure.
   - Recommendation: check `crates/ignition-core/src/client/` and the
     existing gateway health/diagnostics actions during planning before
     assuming a new endpoint wire is needed; if none exists, the live-rig
     test may need to fall back to an artifact-presence + gateway-log-grep
     proof instead of a structured API check.

4. **Where exactly should the permission-widening (Pitfall 2) live —
   inside `fetch.rs`'s persist step, or as a Phase 16 pre-flight?**
   - What we know: either location fixes the bug; `fetch.rs`'s persist step
     (`fetch.rs:463-470`) is the single choke point every cached artifact
     passes through regardless of caller.
   - What's unclear: whether Phase 15's `fetch.rs` is considered "closed"
     (already shipped, tests green) and thus off-limits for a
     Phase-16-motivated edit, or whether a small, additive permission fix
     there is acceptable scope for this phase.
   - Recommendation: prefer fixing it at the `fetch.rs` persist step (one
     `set_permissions` call, `Cargo.lock` already has no new dependency
     needed — `std::fs::Permissions` is stdlib) since that fixes it for
     every future consumer of the cache, not just Phase 16's override path;
     confirm during planning whether touching a "shipped" Phase 15 file is
     in scope or whether Phase 16 should instead widen permissions
     defensively at its own generation step (belt-and-suspenders is also
     acceptable — cheap, and self-documenting about which phase depends on
     the fix).

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|--------------|-----------|---------|----------|
| Docker Engine + Compose v2 plugin | Live-rig verification (SC-2/SC-3), all merge-semantics research | ✓ | Docker 29.4.0 / Compose v5.1.2 (verified this session) | — |
| GitHub API (unauthenticated) | Registering/verifying the second module's release metadata | ✓ | — (public API, verified reachable this session) | Phase 15's existing rate-limit handling already covers exhaustion |
| A live Ignition 8.3 gateway (for SC-2/SC-3's actual module-load proof) | Live-rig test tier only | Not verified this session (no rig was brought up) | — | The mechanism-level tests (Q3) do not need one; only the true live-rig gate does, matching this codebase's existing `#[ignore]`-gated convention |

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` / `cargo test`, `#[async_trait]`-based `ComposeRunner` fakes (no mocking crate) — matches every prior rig-family phase |
| Config file | none — plain `cargo test` |
| Quick run command | `cargo test -p ignition-core --lib module:: rig::` (scoped to touched modules) |
| Full suite command | `cargo test --workspace` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|---------------------|--------------|
| RMOD-01 | No-declaration rig behaves byte-identically to today | unit (pinned arg-vector `assert_eq!`) | `cargo test -p ignition-core --lib rig::compose::tests::up_args_pinned` (extended) | ✅ existing test, extend signature |
| RMOD-04 | Override generated, never mutates user file, never `cp`s into container | unit (golden-string generation + idempotence) | `cargo test -p ignition-core --lib rig::override_gen` (new) | ❌ Wave 0 |
| RMOD-05 | Accept vars set correctly per module (id-format correct, Pitfall 5) | unit (golden includes both modules, asserts `gateway_module_id != id` for Git) | same new test file | ❌ Wave 0 |
| RMOD-06 | `ign`-owned, regenerated not merged, removable | unit (idempotence + delete-reverts test) | same new test file | ❌ Wave 0 |
| RMOD-07 | Second module proves the registry seam with no branching | unit (registry lookup + arg-builder test parameterized over BOTH `ModuleSpec` entries) | `cargo test -p ignition-core --lib module::tests` (extended) | ✅ existing test, extend |
| SC-2/SC-3 (module actually loads, survives recreate) | live-rig, `#[ignore]`-gated | manual/opt-in | `cargo test -p ignition-core --test live_rig_module_injection -- --ignored` (new file, following `live_module_fetch.rs` convention) | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p ignition-core --lib` (scoped)
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** all FIVE CI gates green before `/gsd-verify-work` — `[VERIFIED: .github/workflows/ci.yml:26-33, this session]`:
  1. `cargo fmt --all --check`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo build --workspace`
  4. `cargo test --workspace`
  5. `cargo build -p ignition-cli --no-default-features`
  (Per `.planning/WINDOWS.md` entry 4: this exact five-item list, verbatim,
  is now a standing requirement for every Phase-16-onward plan's
  `<verification>` section — a plan that lists only three gates has
  already caused one red PR this milestone.)

### Wave 0 Gaps
- [ ] `crates/ignition-core/src/rig/override_gen.rs` (or similar) — override-generation golden tests, covers RMOD-04/05/06
- [ ] `crates/ignition-core/tests/live_rig_module_injection.rs` — `#[ignore]`-gated live-rig proof, covers SC-2/SC-3
- [ ] Extend `crates/ignition-core/src/rig/compose.rs`'s existing pinned tests (`up_args_pinned`, `down_args_pinned_with_and_without_volumes`) rather than replace — covers RMOD-01's regression proof
- [ ] Extend `crates/ignition-core/src/module/mod.rs`'s existing registry tests to cover `PROJECT_SCAN_ENDPOINT` alongside `GIT_MODULE` — covers RMOD-07
- Framework install: none — no new test framework needed

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|-----------------|---------|----------------------|
| V2 Authentication | no | Phase 16 adds no new auth surface (accept vars are not credentials) |
| V3 Session Management | no | — |
| V4 Access Control | no | — |
| V5 Input Validation | yes | Reuse Phase 15's existing `validate_module_id`/`validate_version` (`module/mod.rs:82-113`) for every id/version reaching a filesystem path, YAML value, or process arg — never re-derive a second validator |
| V6 Cryptography | no — Phase 15 already owns the sha256 verification; Phase 16 consumes already-verified bytes and does not re-implement any crypto | — |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|----------------------|
| Path traversal via a hostile module id/version reaching a bind-mount `source:`/`target:` path in generated YAML | Tampering | Already mitigated by Phase 15's `validate_module_id`/`validate_version` — Phase 16 MUST route every id/version through them before it reaches the override template, not just before it reaches the cache path (a second use of the same guard, not a new one) |
| A world-writable or overly-permissive cache file after the Pitfall-2 fix (over-correcting `0600` → e.g. `0666`) | Tampering / Information Disclosure | Widen to exactly `0644` (owner read/write, group+other read-only) — never `0666`/`0777`; the file only needs to be READABLE by the container's non-root uid, never writable by anyone but `ign`'s own user |
| YAML injection via an unsanitized value landing in the generated override (e.g. a future field that embeds free-text) | Tampering | Not applicable to the CURRENT field set (module id/version are charset-validated, all other values are fixed literals or absolute paths already constructed by `ign` itself) — but any FUTURE field added to the override template must go through the same validate-before-template discipline, never raw string interpolation of user-controlled free text |

## Sources

### Primary (HIGH confidence)
- Live `docker compose -f base.yml -f override.yml config --format json` run, Docker Compose v5.1.2, this session — volume-append / environment-merge behavior
- Live `docker run -v <missing-path>:/mnt/x:ro alpine stat /mnt/x`, this session — directory-auto-creation-on-missing-bind-source
- Live `docker inspect inductiveautomation/ignition:8.3.6`, this session — `Config.Volumes: null`, `Config.User: 2003:2003`
- Standalone `rustc`/`cargo` compile against this repo's exact `tempfile = "3.27.0"` pin, this session — confirms `NamedTempFile::new_in().persist()` mode `0600`
- Live GitHub API `GET /repos/bw-design-group/ignition-project-scan-endpoint/releases/tags/v1.0.0`, this session — confirms tag, asset name, sha256 digest match the user-provided facts exactly
- Direct download + unzip + read of `Project-Scan-Endpoint.modl`'s `module.xml`, this session — confirms `<id>project-scan-endpoint</id>`, `requiredIgnitionVersion 8.3.0`, `freeModule false`, and presence of `certificates.p7b`/`signatures.properties`
- `crates/ignition-core/src/module/mod.rs` (full file, read this session)
- `crates/ignition-core/src/module/fetch.rs` (full file, read this session)
- `crates/ignition-core/src/rig/mod.rs` (read this session, lines 1-340)
- `crates/ignition-core/src/rig/compose.rs` (read this session, lines 1-410, 695-810)
- `crates/ignition-core/src/config/profile.rs` (read this session, lines 1-135)
- `crates/ignition-core/src/actions/rig.rs` (read this session, lines 1-120, plus grep-located call sites)
- `crates/ignition-core/src/error.rs` (read this session, lines 730-1135, 1580-1720)
- `crates/ignition-cli/src/cli.rs` (read this session, lines 1050-1180)
- `.github/workflows/ci.yml` (read this session, lines 1-50)
- `crates/ignition-cli/tests/contract_rig.rs` (grep + read this session, lines 120-360)
- `.planning/WINDOWS.md` (read this session)
- `.planning/ROADMAP.md`, `.planning/REQUIREMENTS.md`, `.planning/research/2026-09-20-rig-module-provisioning-DESIGN.md` (read this session, full)

### Secondary (MEDIUM confidence)
- [Docker Compose — Merge Compose files](https://docs.docker.com/compose/how-tos/multiple-compose-files/merge/) — official docs, cited to corroborate the live-verified merge behavior
- [Docker Compose CLI reference](https://docs.docker.com/reference/cli/docker/compose/) — official docs, `--project-directory` default resolution
- [Docker Community Forums — bind mount with colon in path](https://forums.docker.com/t/docker-compose-bind-mount-with-colon-comma-in-path-not-working/146533) — community-sourced but technically precise and consistent with Docker's own documented short-syntax grammar
- Live `crates.io` API queries (`serde_yaml`, `serde_norway`, `serde_yml`, `saphyr`, `yaml-rust2`, `indexmap`), this session

### Tertiary (LOW confidence)
- The design doc's assertion of `com.axone_io.ignition.git` as the Git module's gateway id (`[CITED: design doc §4.1]`, not independently re-derived from the actual cached artifact this session — see Open Question 1)

## Metadata

**Confidence breakdown:**
- Compose merge semantics / invocation shape (Q1, Q2, Q3): HIGH — live-verified this session, not training-data recall
- Registry generalization (Q4): HIGH for the mechanics, MEDIUM for the one unverified Git-module id claim (Open Question 1)
- Config surface / CLI surface (Q5, Q7): MEDIUM — grounded in strong existing-code precedent but genuinely undecided by the design doc, correctly left as RECOMMENDATIONS
- Override generation approach (Q6): HIGH for the crate-landscape audit and Windows-colon fact; MEDIUM for the specific writer-function shape (a recommendation, not yet code)
- Pitfalls: HIGH — five of seven are independently reproduced this session with a live command, not asserted from memory

**Research date:** 2026-09-23
**Valid until:** ~30 days for the codebase-line-range claims (stable unless Phase 15/16 code moves); ~7 days for the crates.io package-landscape audit (fast-moving); the live `docker`/GitHub-API verifications are point-in-time facts about this machine's toolchain and the second module's current release — re-verify if either changes materially before the plan locks
